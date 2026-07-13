//! `encode_mddata_at3` orchestrator (milestone 6d + 8).
//!
//! Ports the control flow of `encode_mddata_at3` (`0x65c98`) from
//! `libatrac.so.1.2.0` using the already-validated leaf functions
//! from `dsp::tone` and `dsp::quant`.
//!
//! Half 1 (deterministic pipeline): implemented and trace-validated.
//! Half 2 (iterative bit-allocation convergence loop): in progress (Phase 3–4).

use crate::dsp::gain::GainInfo;
use crate::dsp::pack::ToneComponentNbits;
use crate::dsp::pack::ToneGroupNbits;
use crate::dsp::pack::nbits_for_packdata as nbits_for_packdata_full;
use crate::dsp::pack::pack_mddata_at3;
use crate::dsp::quant::{
    HuffTableSet, calc_bitnumber, iorder_from_max, nbits_for_adjust, nbits_for_sheader,
    set_idtf_and_limwl, translate_to_idwl,
};
use crate::dsp::tone::{
    BinDescriptor, ToneComponent, extract_multitone_with_groups, extract_single_tones, is_attack,
    set_cuidsf_from_spec, set_quidsf_from_cuidsf, single_tone_check,
};
use crate::tables::quant::{CTX_A_MASKH, CTX_A_MASKS};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncodeFitterDiagnostics {
    pub tone_payload_drop_events: u32,
    pub bfu_idwl_decrement_events: u32,
    pub channel_encode_reject_events: u32,
}

impl EncodeFitterDiagnostics {
    fn add_assign(&mut self, other: Self) {
        self.tone_payload_drop_events = self
            .tone_payload_drop_events
            .saturating_add(other.tone_payload_drop_events);
        self.bfu_idwl_decrement_events = self
            .bfu_idwl_decrement_events
            .saturating_add(other.bfu_idwl_decrement_events);
        self.channel_encode_reject_events = self
            .channel_encode_reject_events
            .saturating_add(other.channel_encode_reject_events);
    }

    fn delta_from(self, before: Self) -> Self {
        Self {
            tone_payload_drop_events: self
                .tone_payload_drop_events
                .saturating_sub(before.tone_payload_drop_events),
            bfu_idwl_decrement_events: self
                .bfu_idwl_decrement_events
                .saturating_sub(before.bfu_idwl_decrement_events),
            channel_encode_reject_events: self
                .channel_encode_reject_events
                .saturating_sub(before.channel_encode_reject_events),
        }
    }
}

/// Per-channel encoder state for `encode_mddata_at3`.
///
/// Mirrors the ~0x61f8-byte struct allocated by `init_at3enc` at `0x64cf0`,
/// holding only the fields needed by Half 2 of `encode_mddata_at3`.
#[derive(Debug, Clone)]
pub struct EncoderChannelState {
    /// Number of BFUs — spectral-section count (state[0] / byte offset 0x00).
    pub spectral_bfu_count: i32,
    /// Number of BFUs — tone-section count (state[1] / byte offset 0x04).
    pub bfu_count: i32,
    /// Coding mode (state[2]). Error if == 3.
    pub coding_mode: i32,
    /// Huffman table selector (state[3]).
    pub table_idx: i32,
    /// Joint-stereo flag (state[0x44]).
    pub joint_stereo: i32,
    /// Total bit budget for this channel (state[0x1872]).
    pub total_bit_budget: f32,
    /// Raw spectral coefficients (state[0x145f area], 1024 f32, Half 1 input).
    pub spectral_data: [f32; 1024],
    /// Quantized mantissas (state[0x145f area], 1024 i32, written by
    /// `quant_nontone_nspecs`, read by `pack_mddata_at3`).
    pub quantized_mantissas: [i32; 1024],
    /// Per-BFU IDWL array (state[0x141f] / byte offset 0x507c), read as the
    /// pre-call persisted IDWL array and written back by `encode_mddata_at3`.
    pub final_idwl: [i32; 32],
    /// Final per-BFU spread after convergence (state[0x143f] / byte offset 0x50fc).
    pub final_spread: [i32; 32],
    /// Legacy spectral-BFU activity scratch.
    pub adjust_flags: [i32; 32],
    /// Gain-control adjustment records (state[0x10], state[0x50], state[0x90], state[0xd0]).
    pub gain_control: [GainInfo; 4],
    /// Previous shadow state's gain-control records (state[0x61e8]+0x10...),
    /// consulted by `encode_mddata_at3` for attack classification.
    pub previous_gain_control: [GainInfo; 4],
    /// Packing enabled flag (state[0x6180]).
    pub packing_enabled: i32,
    /// Byte count for `put_chsunit_at3` (state[0x61c4]).
    pub bfu_byte_count: i32,
    /// Number of tone groups (state[0x110]).
    pub tone_group_count: i32,
    /// Extracted tone components (state[0x4278 area], up to 64).
    pub tone_components: Vec<ToneComponent>,
    /// Diagnostic counters for post-pack fitter behavior.
    pub diagnostics: EncodeFitterDiagnostics,
}

impl Default for EncoderChannelState {
    fn default() -> Self {
        Self {
            spectral_bfu_count: 29,
            bfu_count: 3,
            coding_mode: 0,
            table_idx: 0,
            joint_stereo: 0,
            total_bit_budget: 0.0,
            spectral_data: [0.0f32; 1024],
            quantized_mantissas: [0i32; 1024],
            final_idwl: [0i32; 32],
            final_spread: [0i32; 32],
            adjust_flags: [0i32; 32],
            gain_control: core::array::from_fn(|_| GainInfo::new()),
            previous_gain_control: core::array::from_fn(|_| GainInfo::new()),
            packing_enabled: 0,
            bfu_byte_count: 0,
            tone_group_count: 0,
            tone_components: Vec::new(),
            diagnostics: EncodeFitterDiagnostics::default(),
        }
    }
}

/// Result of `encode_mddata_at3`.
pub struct EncodeResult {
    /// Remaining bit budget (or −0x8000 on error).
    pub remaining_budget: i32,
}

/// `encode_mddata_at3` (`0x65c98`): encodes spectral data for one
/// channel into the encoder state.
///
/// Half 1 (deterministic pipeline): implemented and validated.
/// Half 2 (iterative convergence): in progress.
pub fn encode_mddata_at3(
    specs_a: &[f32],
    specs_b: &[f32],
    state: &mut EncoderChannelState,
    huff_tables: &HuffTableSet,
    spec_huff: &HuffTableSet,
) -> EncodeResult {
    let bit_budget_base = state.total_bit_budget as i32;

    // --- Step 1: Initialisation ---
    let mut tone_specs = specs_b.to_vec();
    tone_specs.resize(tone_specs.len().max(1024), 0.0);

    let mut idtf_scratch = [0i32; 6];
    let mut init_limit = 0i32;
    let mut init_wl = 0i32;
    set_idtf_and_limwl(&mut idtf_scratch, &mut init_limit, &mut init_wl);

    // --- Step 2: Descriptor table population ---
    let mut descriptors_a = vec![
        BinDescriptor {
            peak_value: 0,
            idsf: 0,
            tone_template: [0; 16],
        };
        256
    ];
    let mut descriptors_b = vec![
        BinDescriptor {
            peak_value: 0,
            idsf: 0,
            tone_template: [0; 16],
        };
        256
    ];
    set_cuidsf_from_spec(specs_b, &mut descriptors_a, 256);
    set_cuidsf_from_spec(specs_a, &mut descriptors_b, 256);

    // --- Step 3: Peak-finding on table B ---
    let mut max_peak_bin = 0;
    let mut max_peak_val = descriptors_b[0].idsf;
    for (i, d) in descriptors_b.iter().enumerate().skip(1).take(255) {
        if d.idsf > max_peak_val {
            max_peak_val = d.idsf;
            max_peak_bin = i as i32;
        }
    }

    // --- Step 4: Tone check ---
    let stc_result = single_tone_check(&descriptors_a);

    let mut total_bits = 0i32;
    let mut tone_extracted = false;
    let mut tone_group_count = 0i32;
    let mut tone_components = Vec::new();
    let mut multitone_bits_for_lock = 0i32;
    let mut single_tone_mode = false;

    if stc_result > 0 && state.packing_enabled == 0 && state.spectral_bfu_count > 25 {
        single_tone_mode = true;
        let shdr = nbits_for_sheader(state.joint_stereo != 0);
        let adj = nbits_for_adjust(1, &[]); // simplified: per_bfu_vals not yet computed at Half 1
        let budget = bit_budget_base - shdr - adj - 0xd;

        let radius = 2i32;
        let span = 3i32;

        let single_bits = extract_single_tones(
            budget,
            stc_result,
            radius,
            max_peak_bin,
            span,
            256,
            &mut tone_specs,
            &mut descriptors_a,
            huff_tables,
            &mut tone_components,
        );
        if single_bits == -0x8000 {
            return EncodeResult {
                remaining_budget: -0x8000,
            };
        }
        total_bits += single_bits;
        tone_extracted = single_bits > 0;
        if tone_extracted {
            tone_group_count = stc_result;
        }
    }

    // --- Step 5: Attack check ---
    let has_gain_attack = |gains: &[GainInfo; 4], bfu_count: i32| -> bool {
        gains
            .iter()
            .take(bfu_count.clamp(0, 4) as usize)
            .any(|gain| {
                let count = gain.count.clamp(0, 7) as usize;
                count > 0 && is_attack(&gain.level[..count])
            })
    };
    let mut is_attack_result = has_gain_attack(&state.gain_control, state.bfu_count);
    if !is_attack_result {
        is_attack_result = has_gain_attack(&state.previous_gain_control, state.bfu_count);
    }

    let desc_count = {
        let pos = crate::dsp::quant::ispof_iqt_at3(state.spectral_bfu_count as u32);
        if pos < 0 { (pos + 3) >> 2 } else { pos >> 2 }
    }
    .clamp(0, descriptors_b.len() as i32);
    let descriptor_avg = if desc_count > 0 {
        descriptors_b
            .iter()
            .take(desc_count as usize)
            .map(|d| d.idsf)
            .sum::<i32>() as f32
            / desc_count as f32
    } else {
        0.0
    };
    // --- Step 6: Multitone extraction ---
    if !tone_extracted {
        let shdr = nbits_for_sheader(state.joint_stereo != 0);
        let adj = nbits_for_adjust(1, &[]); // simplified: per_bfu_vals not yet computed
        let budget = bit_budget_base - shdr - adj - 0xd - total_bits;

        let (multitone_bits, multitone_group_count) = extract_multitone_with_groups(
            budget,
            desc_count,
            state.bfu_count,
            descriptor_avg,
            &mut tone_specs,
            &mut descriptors_a,
            &mut descriptors_b,
            huff_tables,
            &mut tone_components,
        );
        if multitone_bits == -0x8000 {
            return EncodeResult {
                remaining_budget: -0x8000,
            };
        }
        total_bits += multitone_bits;
        multitone_bits_for_lock = multitone_bits;
        if multitone_bits > 0 {
            tone_group_count = multitone_group_count;
        }
    }
    // Store extracted tone data in encoder state for pack_mddata_at3.
    state.tone_group_count = if tone_components.is_empty() {
        0
    } else {
        tone_group_count
    };
    state.tone_components = tone_components;

    // --- Step 7: Scale/quant propagation ---
    let mut quidsf = [0i32; 32];
    let mut idwl_quidsf = [0i32; 32];
    set_quidsf_from_cuidsf(&descriptors_a, &mut quidsf, 29);
    set_quidsf_from_cuidsf(&descriptors_b, &mut idwl_quidsf, 29);

    let bit_idsf = [0i32; 32];

    let mut quant_specs = tone_specs;
    quant_specs.resize(quant_specs.len().max(1024), 0.0);
    for (i, &idsf) in quidsf.iter().enumerate().take(29) {
        let sf = crate::dsp::quant::scfof_id_at3(idsf as u32) as f32;
        if sf <= 0.0 {
            return EncodeResult {
                remaining_budget: -0x8000,
            };
        }
        let pos = crate::dsp::quant::ispof_iqt_at3(i as u32);
        let nsps = crate::dsp::quant::nsps_inqt_at3(i as u32);
        if pos < 0 || nsps < 0 {
            return EncodeResult {
                remaining_budget: -0x8000,
            };
        }
        let start = pos as usize;
        let end = (start + nsps as usize).min(quant_specs.len());
        for sample in &mut quant_specs[start..end] {
            *sample *= 1.0 / sf;
        }
    }

    // --- Step 8: Spread/IDWL computation ---
    let quidsf_count = 29i32;

    let mut spread = [0.0f32; 32];
    for (i, slot) in spread.iter_mut().enumerate().take(quidsf_count as usize) {
        let diff = idwl_quidsf[i] as f32 - descriptor_avg;
        let factor: f32 = if i < 8 {
            3.0
        } else if i < 12 {
            3.3
        } else if i < 16 {
            3.4
        } else if i < 18 {
            3.5
        } else if i < 26 {
            3.6
        } else if i < 28 {
            3.8
        } else {
            4.0
        };
        *slot = diff / factor;
    }

    let attack_adjust = is_attack_result;
    if attack_adjust {
        for s in spread.iter_mut().take(8) {
            *s += 0.7;
        }
        for s in spread.iter_mut().skip(8).take(10) {
            *s += 0.5;
        }
        if spread[0] < 6.0 {
            spread[0] = 6.0;
        }
    } else {
        if spread[0] < 6.0 {
            spread[0] = 6.0;
        }
        for s in spread.iter_mut().skip(1).take(3) {
            if *s < 3.0 {
                *s = 3.0;
            }
        }
    }

    let fine_spread_base = spread;

    let mut idwl_out = [0i32; 32];
    let ctx = if single_tone_mode {
        CTX_A_MASKS
    } else {
        CTX_A_MASKH
    };
    let initial_threshold = translate_to_idwl(
        &ctx,
        1,
        &spread,
        &idwl_quidsf,
        &mut idwl_out,
        quidsf_count,
        7,
    );
    let mut initial_bfu_lock = [0i32; 32];
    for i in 0..quidsf_count as usize {
        if quidsf[i] < initial_threshold {
            initial_bfu_lock[i] = 1;
            idwl_out[i] = 0;
        }
        if multitone_bits_for_lock == 0 {
            if idwl_out[i] == 0 {
                initial_bfu_lock[i] = 1;
            }
            if quidsf[i] as f32 <= descriptor_avg {
                initial_bfu_lock[i] = 1;
            }
        }
    }

    // --- Step 9: IDWL flags ---
    let mut flags = [0i32; 32];
    flags
        .iter_mut()
        .take(quidsf_count as usize)
        .for_each(|v| *v = 1);

    // --- Step 10: Bit allocation ---
    let mut bits_out = [0i32; 32];
    let bit_total = calc_bitnumber(
        &flags,
        &bit_idsf,
        &idwl_out,
        &quant_specs,
        &mut bits_out,
        quidsf_count,
        state.table_idx,
        spec_huff,
    );
    if bit_total == -0x8000 {
        return EncodeResult {
            remaining_budget: -0x8000,
        };
    }
    for (i, &idwl) in idwl_out.iter().enumerate().take(quidsf_count as usize) {
        if idwl <= 0 {
            continue;
        }
        let scale = crate::dsp::quant::tfof_id(0, idwl);
        let pos = crate::dsp::quant::ispof_iqt_at3(i as u32);
        let nsps = crate::dsp::quant::nsps_inqt_at3(i as u32);
        if pos < 0 || nsps < 0 {
            continue;
        }
        let spec_start = pos as usize;
        let spec_end = (spec_start + nsps as usize).min(quant_specs.len());
        let mut mantissas = vec![0i32; nsps as usize];
        let bits = crate::dsp::quant::quant_nontone_nspecs(
            state.table_idx,
            idwl,
            scale,
            nsps,
            &quant_specs[spec_start..spec_end],
            &mut mantissas,
            spec_huff,
        );
        if bits >= 0 {
            let end = spec_start
                + mantissas
                    .len()
                    .min(state.quantized_mantissas.len().saturating_sub(spec_start));
            state.quantized_mantissas[spec_start..end]
                .copy_from_slice(&mantissas[..end - spec_start]);
        }
    }
    total_bits += bit_total;

    // ─── Half 2: Iterative bit-allocation convergence ───

    let spec_count = quidsf_count;
    let bfu_num = spec_count as usize;

    // Save original spread for later reference
    let saved_spread = spread;

    // ITFB-group initial IDWL ceilings.
    // local_8bc is zeroed by set_idtf_and_limwl, so all ceilings start at 0.
    // They are increased adaptively by Loop 2 during convergence.
    let mut ceiling = [0i32; 32];

    // Flag array with all 1s for full recomputation in calc_bitnumber
    let changed_all_flag = [1i32; 32];

    #[allow(unused_assignments)]
    let mut iter_total_bits = bit_total;
    #[allow(unused_assignments)]
    let mut final_bfu_count = 0i32;
    // Convergence budget matching C lines 54566/54588 and 54661-54664:
    //   local_a8dc = budget - shdr - adj - spectral_bfu_count*3 - 0xd - tone_bits
    //   if joint_stereo == 0 { local_a8dc += 2 }
    let overh = nbits_for_sheader(state.joint_stereo != 0);
    let tone_bits_only = total_bits - bit_total;
    let gain_counts_for_adjust: Vec<i32> = state
        .gain_control
        .iter()
        .take(state.bfu_count.max(0) as usize)
        .map(|info| info.count)
        .collect();
    let adjust_bits = nbits_for_adjust(state.bfu_count, &gain_counts_for_adjust);
    let remaining_budget =
        bit_budget_base - overh - adjust_bits - spec_count * 3 - 0xd - tone_bits_only
            + if state.joint_stereo == 0 && state.tone_components.is_empty() {
                2
            } else {
                0
            };
    let density_pos = crate::dsp::quant::ispof_iqt_at3(spec_count as u32).max(0) + 0x100;
    let density = remaining_budget as f64 / density_pos as f64;
    let density_ceil = if density <= 0.90 {
        12
    } else if density <= 0.95 {
        11
    } else if density <= 1.00 {
        9
    } else if density <= 1.05 {
        7
    } else if density <= 1.10 {
        5
    } else if density <= 1.15 {
        3
    } else {
        0
    };

    if bit_total != -0x8000 {
        // Save pre-convergence IDWL as the upper bound for fine-tuning.
        let initial_idwl = idwl_out;

        // ─── Loop 1: Spread adjustment PID (max 15 iterations) ───
        let mut delta = 2.0f32;
        let mut step = 4.0f32;
        let mut iter = 0;
        let mut adjusted_spread = spread;
        let mut fine_adjusted_spread = fine_spread_base;

        // Region-based spread adjustment multipliers
        let region_mult = |bfu: usize| -> f32 {
            if bfu == 0 {
                0.2
            } else if bfu == 1 {
                0.3
            } else if bfu < 8 {
                0.4
            } else if bfu < 18 {
                0.6
            } else {
                1.0
            }
        };

        let ctx_arr = ctx;

        loop {
            // A: Adjust spreads
            // Per RE doc (lines 54907-54942):
            // - delta > 0: add full delta uniformly to every BFU
            // - delta <= 0: use region multipliers (BFU 0 gets 0.2×, etc.)
            for i in 0..bfu_num {
                adjusted_spread[i] = if delta > 0.0 {
                    saved_spread[i] + delta
                } else {
                    saved_spread[i] + delta * region_mult(i)
                };
                fine_adjusted_spread[i] = if delta > 0.0 {
                    fine_spread_base[i] + delta
                } else {
                    fine_spread_base[i] + delta * region_mult(i)
                };
            }

            // B: Backup IDWL
            let prev_idwl = idwl_out;

            // C: translate_to_idwl
            translate_to_idwl(
                &ctx_arr,
                1,
                &adjusted_spread,
                &idwl_quidsf,
                &mut idwl_out,
                spec_count,
                7,
            );

            // D: Post-process — if BFU 0 idwl < 6 and budget small, force to 6
            if idwl_out[0] < 6 && remaining_budget < 10 {
                idwl_out[0] = 6;
            }
            // C lines 54967-54969: only zero BFUs whose max_idwl (initial
            // IDWL) is zero — not BFUs that dropped to zero in this PID
            // iteration. This prevents the PID from irreversibly locking
            // BFUs to zero.
            for i in 0..bfu_num {
                if initial_idwl[i] <= 0 {
                    idwl_out[i] = 0;
                }
            }

            // E: Detect changes
            let mut changed = [0i32; 32];
            for (c, (n, p)) in changed
                .iter_mut()
                .zip(idwl_out.iter().zip(&prev_idwl))
                .take(bfu_num)
            {
                *c = (n != p) as i32;
            }

            // F: calc_bitnumber
            let mut bits_out2 = bits_out;
            let new_total = calc_bitnumber(
                &changed,
                &bit_idsf,
                &idwl_out,
                &quant_specs,
                &mut bits_out2,
                spec_count,
                state.table_idx,
                spec_huff,
            );
            if new_total == -0x8000 {
                return EncodeResult {
                    remaining_budget: -0x8000,
                };
            }
            bits_out = bits_out2;
            iter_total_bits = new_total;

            // G: Check convergence
            if iter_total_bits <= remaining_budget && iter > 5 {
                break;
            }

            // H: Adjust delta
            if iter_total_bits < remaining_budget {
                delta += step;
            } else {
                delta -= step;
            }

            // I: Modulate step size
            if iter < 7 {
                step *= 0.5;
            } else {
                step *= 1.5;
            }

            iter += 1;
            if iter >= 15 {
                break;
            }
        }
        // If PID couldn't converge within budget, return the best effort.

        // ─── Loop 2: Per-BFU IDWL increment (4 passes) ───
        for _pass in 0..4 {
            for i in (0..bfu_num).rev() {
                while idwl_out[i] > 0
                    && ceiling[i] < density_ceil
                    && remaining_budget < iter_total_bits
                {
                    ceiling[i] += 1;
                    let mut bits_out3 = bits_out;
                    let nt = calc_bitnumber(
                        &changed_all_flag,
                        &bit_idsf,
                        &idwl_out,
                        &quant_specs,
                        &mut bits_out3,
                        spec_count,
                        state.table_idx,
                        spec_huff,
                    );
                    if nt == -0x8000 {
                        return EncodeResult {
                            remaining_budget: -0x8000,
                        };
                    }
                    bits_out = bits_out3;
                    iter_total_bits = nt;
                }
            }
        }

        // ─── Loop 3: Per-BFU IDWL decrement with priority ───
        // Decrement LOW-energy BFUs first (ascending key order) to preserve
        // high-energy BFUs that carry more signal. The key is quidsf - i/2;
        // lower key = lower energy = decrement first.
        if remaining_budget < iter_total_bits {
            let mut keys = [0i32; 32];
            for i in 0..bfu_num {
                keys[i] = if single_tone_mode {
                    (idwl_quidsf[i] + 1) * 32 - i as i32
                } else {
                    idwl_quidsf[i] - (i as i32) / 2
                };
            }
            let mut order = [0i32; 32];
            iorder_from_max(&keys, &mut order, spec_count);
            // Reverse: iorder_from_max gives descending (highest first),
            // but we want ascending (lowest first) for decrement.
            let mut rev_order = [0i32; 32];
            for (dst, &v) in rev_order.iter_mut().zip(order[..bfu_num].iter().rev()) {
                *dst = v;
            }

            for &i in rev_order.iter().take(bfu_num) {
                let i = i as usize;
                while idwl_out[i] > 0 && remaining_budget < iter_total_bits {
                    idwl_out[i] -= 1;
                    // Use all-1s flags so calc_bitnumber recomputes ALL BFUs,
                    // keeping bits_out accurate for subsequent decrements.
                    let mut bits_out4 = bits_out;
                    let nt = calc_bitnumber(
                        &changed_all_flag,
                        &bit_idsf,
                        &idwl_out,
                        &quant_specs,
                        &mut bits_out4,
                        spec_count,
                        state.table_idx,
                        spec_huff,
                    );
                    if nt == -0x8000 {
                        return EncodeResult {
                            remaining_budget: -0x8000,
                        };
                    }
                    bits_out = bits_out4;
                    iter_total_bits = nt;
                }
            }
        }

        // Original C trims trailing zero-IDWL BFUs (line 55122):
        //   while aiStack_120[local_a8f4] == 0 && local_a8f4 > 1:
        //       local_a8f4--
        final_bfu_count = bfu_num as i32;
        while final_bfu_count > 1 && idwl_out[(final_bfu_count - 1) as usize] == 0 {
            final_bfu_count -= 1;
        }
        state.spectral_bfu_count = final_bfu_count;

        // ─── State writeback ───
        let written_bfu_count = final_bfu_count as usize;
        state.final_idwl[..written_bfu_count].copy_from_slice(&idwl_out[..written_bfu_count]);
        state.final_spread[..written_bfu_count].copy_from_slice(&quidsf[..written_bfu_count]);

        // Compute per-BFU adjust flags (1 = active BFU, 0 = inactive).
        // These are the small values the binary stores at state+0x10
        // and reads back in nbits_for_adjust. NOT the spectral bit counts.
        for (i, &idwl) in idwl_out.iter().enumerate().take(written_bfu_count) {
            state.adjust_flags[i] = if idwl > 0 { 1 } else { 0 };
        }

        // Populate quantized mantissas from the converged IDWL values.
        // These are written to state+0x145f by the original encode_mddata_at3
        // and read back by nbits_for_spectrum inside nbits_for_packdata.
        for (i, &idwl) in idwl_out.iter().enumerate().take(bfu_num) {
            if idwl <= 0 {
                continue;
            }
            let scale = crate::dsp::quant::tfof_id(0, idwl);
            let pos = crate::dsp::quant::ispof_iqt_at3(i as u32);
            let nsps = crate::dsp::quant::nsps_inqt_at3(i as u32);
            if pos < 0 || nsps < 0 {
                continue;
            }
            let spec_start = pos as usize;
            let spec_end = (spec_start + nsps as usize).min(quant_specs.len());
            let mut mantissas = vec![0i32; nsps as usize];
            let _bits = crate::dsp::quant::quant_nontone_nspecs(
                state.table_idx,
                idwl,
                scale,
                nsps,
                &quant_specs[spec_start..spec_end],
                &mut mantissas,
                spec_huff,
            );
            if _bits >= 0 {
                let end = spec_start
                    + mantissas
                        .len()
                        .min(state.quantized_mantissas.len().saturating_sub(spec_start));
                state.quantized_mantissas[spec_start..end]
                    .copy_from_slice(&mantissas[..end - spec_start]);
            }
        }

        // Update total consumed from the converged state.
        total_bits = iter_total_bits;

        // ─── Fine-tuning sub-loops (C lines 55181–55517) ───
        let removed_bfu = spec_count - final_bfu_count;
        let mut fine_tune_remaining =
            bit_budget_base - ((remaining_budget + removed_bfu * 3) - total_bits);
        fine_tune_idwl(
            &quant_specs,
            &mut idwl_out,
            &idwl_quidsf,
            &mut ceiling,
            bfu_num,
            spec_huff,
            state,
            bit_budget_base,
            &mut fine_tune_remaining,
            &initial_idwl,
            final_bfu_count as usize,
            &fine_adjusted_spread,
            &initial_bfu_lock,
        );
        // Recompute final_bfu_count after fine-tuning.
        final_bfu_count = bfu_num as i32;
        while final_bfu_count > 1 && idwl_out[(final_bfu_count - 1) as usize] == 0 {
            final_bfu_count -= 1;
        }
        // Store fine-tuned remaining back into total_bits for the formula below.
        total_bits = fine_tune_remaining;
    }

    // ─── Final validation: remaining formula matching C line 55145 ───
    // After fine-tuning, total_bits holds the fine-tuned remaining.
    let remaining = total_bits;

    // Full reconciliation check against nbits_for_packdata.
    // Disabled until tone extraction state is wired (task #11) and
    // fine-tuning sub-loops are ported (task #12). The binary enforces
    // exact equality at lines 55519-55523.
    // Build tone-group data from stored extraction output.
    let mut tone_groups_for_nbits: Vec<ToneGroupNbits> = Vec::new();
    if !state.tone_components.is_empty() {
        let mut group = ToneGroupNbits {
            idwl: state.tone_components[0].idwl,
            table_idx: state.tone_components[0].table_idx,
            has_tone: [0i32; 4],
            per_bfu_tone_count: vec![0i32; bfu_num],
            components: Vec::new(),
        };
        let quidsf = &state.final_spread;
        for comp in &state.tone_components {
            let width = crate::dsp::quant::twidof_id_at3(comp.width_id as u32).max(0) as usize;
            let mantissas: Vec<i32> = comp.mantissas.iter().take(width).copied().collect();
            group.components.push(ToneComponentNbits {
                position: comp.position,
                mantissas,
            });
            let pos = comp.position >> 2;
            for (b, &qsf) in quidsf.iter().enumerate().take(bfu_num) {
                let start = crate::dsp::quant::ispof_iqt_at3(b as u32);
                let end = if b + 1 < bfu_num {
                    crate::dsp::quant::ispof_iqt_at3((b + 1) as u32)
                } else {
                    1024
                };
                if start >= 0 && pos >= start && pos < end {
                    let itb = crate::dsp::quant::itfbof_iqt(qsf);
                    let g = crate::dsp::pack::itbgrpof_itb_at3(itb as u32);
                    if g >= 0 && (g as usize) < 4 {
                        group.has_tone[g as usize] = 1;
                    }
                    group.per_bfu_tone_count[b] += 1;
                    break;
                }
            }
        }
        tone_groups_for_nbits.push(group);
    }

    let gain_counts: Vec<i32> = state
        .gain_control
        .iter()
        .take(state.bfu_count.max(0) as usize)
        .map(|info| info.count)
        .collect();
    let _full_pack = nbits_for_packdata_full(
        state.bfu_count,
        spec_count,
        state.tone_group_count,
        state.coding_mode,
        state.table_idx,
        &idwl_out,
        &state.quantized_mantissas,
        &tone_groups_for_nbits,
        huff_tables,
        spec_huff,
        state.joint_stereo != 0,
        state.bfu_count,
        &gain_counts,
    );
    EncodeResult {
        remaining_budget: remaining,
    }
}

/// Fine-tuning sub-loops (C lines 55181–55484): post-convergence passes
/// that iteratively raise per-BFU IDWL values to fill the remaining budget.
///
/// Each block calls `quant_nontone_nspecs` to check the new bit cost and
/// commits if the new cost fits within `total_bit_budget`.
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn fine_tune_idwl(
    quant_specs: &[f32],
    idwl_out: &mut [i32; 32],
    quidsf: &[i32; 32],
    ceiling: &mut [i32; 32],
    bfu_num: usize,
    spec_huff: &HuffTableSet,
    state: &mut EncoderChannelState,
    total_bit_budget: i32,
    remaining: &mut i32,
    max_idwl: &[i32; 32],
    final_bfu_count: usize,
    adjusted_spread: &[f32; 32],
    initial_bfu_lock: &[i32; 32],
) {
    let active_bfu_count = final_bfu_count.min(bfu_num);
    let spec_count = active_bfu_count as i32;
    let table_idx = state.table_idx;
    let mut per_bfu_bit_counts = [0i32; 32];

    // Populate per_bfu_bit_counts from the current idwl_out.
    for i in 0..active_bfu_count {
        let idwl = idwl_out[i];
        if idwl <= 0 {
            per_bfu_bit_counts[i] = 0;
            continue;
        }
        let scale = crate::dsp::quant::tfof_id(0, idwl);
        let pos = crate::dsp::quant::ispof_iqt_at3(i as u32);
        let nsps = crate::dsp::quant::nsps_inqt_at3(i as u32);
        if pos < 0 || nsps < 0 {
            per_bfu_bit_counts[i] = 0;
            continue;
        }
        let spec_start = pos as usize;
        let spec_end = (spec_start + nsps as usize).min(quant_specs.len());
        let mut mantissas = vec![0i32; nsps as usize];
        let bits = crate::dsp::quant::quant_nontone_nspecs(
            table_idx,
            idwl,
            scale,
            nsps,
            &quant_specs[spec_start..spec_end],
            &mut mantissas,
            spec_huff,
        );
        if bits >= 0 {
            per_bfu_bit_counts[i] = bits;
            let end = spec_start
                + mantissas
                    .len()
                    .min(state.quantized_mantissas.len().saturating_sub(spec_start));
            state.quantized_mantissas[spec_start..end]
                .copy_from_slice(&mantissas[..end - spec_start]);
        }
    }

    // ─── Block 0: spread-minus-IDWL and candidate collection ───
    let mut spread_minus_idwl = [0.0f32; 32];
    let mut bfu_lock = [0i32; 32];
    let mut upgrade_candidates = [0i32; 32];

    for i in 0..active_bfu_count {
        if idwl_out[i] == 0 {
            spread_minus_idwl[i] = f32::from_bits(0xc11f_d70a);
        } else {
            spread_minus_idwl[i] = adjusted_spread[i] - idwl_out[i] as f32;
        }
        bfu_lock[i] = -1;
    }

    // Find min and max spread_minus_idwl across active BFUs.
    let mut smin = spread_minus_idwl[0];
    let mut smax = smin;
    for &v in spread_minus_idwl
        .iter()
        .skip(1)
        .take(active_bfu_count.saturating_sub(1))
    {
        if v < smin {
            smin = v;
        }
        if v > smax {
            smax = v;
        }
    }
    if smax - smin < 1e-6f32 {
        return; // no variation — nothing to fine-tune
    }

    // Partition into 10 spread bands, collect candidates.
    let mut candidate_count = 0i32;
    for band in 0..10 {
        let threshold = smax - (smax - smin) * (band as f32 + 1.0) / 10.0;
        for i in (0..active_bfu_count).rev() {
            if bfu_lock[i] == -1 && spread_minus_idwl[i] >= threshold {
                bfu_lock[i] = candidate_count;
                upgrade_candidates[candidate_count as usize] = i as i32;
                candidate_count += 1;
            }
        }
    }

    if candidate_count <= 0 {
        return;
    }

    // ─── Helper: try requantize a BFU, commit if within budget ───
    let try_requantize = |bfu: usize,
                          idwl_try: i32,
                          remaining_: &mut i32,
                          per_bfu_bit_counts_: &mut [i32; 32],
                          state_: &mut EncoderChannelState|
     -> bool {
        let scale = crate::dsp::quant::tfof_id(0, idwl_try);
        let pos = crate::dsp::quant::ispof_iqt_at3(bfu as u32);
        let nsps = crate::dsp::quant::nsps_inqt_at3(bfu as u32);
        if pos < 0 || nsps < 0 {
            return false;
        }
        let spec_start = pos as usize;
        let spec_end = (spec_start + nsps as usize).min(quant_specs.len());
        let mut mantissas = vec![0i32; nsps as usize];
        let new_bits = crate::dsp::quant::quant_nontone_nspecs(
            table_idx,
            idwl_try,
            scale,
            nsps,
            &quant_specs[spec_start..spec_end],
            &mut mantissas,
            spec_huff,
        );
        if new_bits == -0x8000 {
            return false;
        }
        let delta = new_bits - per_bfu_bit_counts_[bfu];
        let new_remaining = *remaining_ + delta;
        if new_remaining > total_bit_budget {
            return false;
        }
        per_bfu_bit_counts_[bfu] = new_bits;
        *remaining_ = new_remaining;
        let end = spec_start
            + mantissas
                .len()
                .min(state_.quantized_mantissas.len().saturating_sub(spec_start));
        state_.quantized_mantissas[spec_start..end].copy_from_slice(&mantissas[..end - spec_start]);
        true
    };

    // ─── Block 1: Spread-band priority re-quantization ───
    let mut bfu_locked = *initial_bfu_lock;
    let mut budget_almost_full = false;
    for &c_bfu in upgrade_candidates.iter().take(candidate_count as usize) {
        if budget_almost_full {
            break;
        }
        let bfu = c_bfu as usize;
        if bfu >= active_bfu_count
            || initial_bfu_lock[bfu] != 0
            || idwl_out[bfu] <= 0
            || idwl_out[bfu] >= 7
        {
            continue;
        }
        let old_idwl = idwl_out[bfu];
        let tried;
        if old_idwl < max_idwl[bfu] {
            idwl_out[bfu] = old_idwl + 1;
            tried = true;
        } else if ceiling[bfu] > 0 {
            ceiling[bfu] -= 1;
            tried = true;
        } else {
            tried = false;
        }
        if tried {
            let committed = try_requantize(
                bfu,
                idwl_out[bfu],
                remaining,
                &mut per_bfu_bit_counts,
                state,
            );
            if committed {
                if *remaining + 8 > total_bit_budget {
                    budget_almost_full = true;
                }
            } else {
                idwl_out[bfu] = old_idwl;
                if old_idwl >= max_idwl[bfu] {
                    ceiling[bfu] += 1;
                }
            }
        }
    }

    // ─── Block 3: Aggressive IDWL increment (7 passes) ───
    for _pass in 0..7 {
        if *remaining + 2 > total_bit_budget {
            break;
        }
        for i in 0..active_bfu_count {
            if *remaining + 2 > total_bit_budget {
                break;
            }
            if idwl_out[i] > 0 && idwl_out[i] < max_idwl[i] && bfu_locked[i] == 0 {
                let old = idwl_out[i];
                idwl_out[i] = old + 1;
                let committed =
                    try_requantize(i, idwl_out[i], remaining, &mut per_bfu_bit_counts, state);
                if !committed {
                    idwl_out[i] = old;
                    bfu_locked[i] = 1;
                }
            }
        }
    }

    // ─── Block 4: Priority-ordered increment (4 passes) ───
    let mut order_keys = [0i32; 32];
    for i in 0..active_bfu_count {
        order_keys[i] = (quidsf[i] + 1) * 32 - i as i32;
    }
    let mut order = [0i32; 32];
    crate::dsp::quant::iorder_from_max(&order_keys, &mut order, spec_count);
    for pass in 0..4 {
        if *remaining + 2 > total_bit_budget {
            break;
        }
        for &idx in order.iter().take(active_bfu_count) {
            if idx < 0 {
                continue;
            }
            let i = idx as usize;
            if *remaining + 2 > total_bit_budget {
                break;
            }
            if idwl_out[i] > 0 && idwl_out[i] < pass + 3 {
                let old = idwl_out[i];
                idwl_out[i] = old + 1;
                let committed =
                    try_requantize(i, idwl_out[i], remaining, &mut per_bfu_bit_counts, state);
                if !committed {
                    idwl_out[i] = old;
                }
            }
        }
    }
    // ─── Block 5: Aggressive increment round 2 (7 passes) ───
    for _pass in 0..7 {
        if *remaining + 2 > total_bit_budget {
            break;
        }
        for i in 0..active_bfu_count {
            if *remaining + 2 > total_bit_budget {
                break;
            }
            if idwl_out[i] > 0 && idwl_out[i] < 7 {
                let old = idwl_out[i];
                idwl_out[i] = old + 1;
                let committed =
                    try_requantize(i, idwl_out[i], remaining, &mut per_bfu_bit_counts, state);
                if !committed {
                    idwl_out[i] = old;
                }
            }
        }
    }

    // ─── Block 6: SNR energy check — decrement spread if noise exceeds signal
    // (C lines 55486-55518, assembly 0x679aa-0x67abd).
    for bfu in 0..active_bfu_count {
        let idwl = idwl_out[bfu];
        let nsteps = crate::dsp::quant::nstepsof_idwl_at3(idwl as u32);
        if nsteps < 0 {
            continue;
        }
        let pos = crate::dsp::quant::ispof_iqt_at3(bfu as u32);
        let nsps = crate::dsp::quant::nsps_inqt_at3(bfu as u32);
        if pos < 0 || nsps < 0 {
            continue;
        }
        let spec_start = pos as usize;
        let spec_end = (spec_start + nsps as usize).min(quant_specs.len());
        let inv_scale = 1.0f64 / (nsteps as f64 + 0.5);
        let mut signal_power = 0.0f64;
        let mut noise_power = 0.0f64;
        for k in spec_start..spec_end {
            if k < quant_specs.len() {
                let s = quant_specs[k] as f64;
                signal_power += s * s;
            }
            if k < state.quantized_mantissas.len() {
                let m = state.quantized_mantissas[k] as f64;
                noise_power += (inv_scale * m) * (inv_scale * m);
            }
        }
        if signal_power * 1.25 < noise_power && state.final_spread[bfu] > 0 {
            state.final_spread[bfu] -= 1;
        }
    }

    // Write back updated final_idwl and quantized mantissas.
    state.final_idwl[..active_bfu_count].copy_from_slice(&idwl_out[..active_bfu_count]);
    for i in 0..active_bfu_count {
        state.adjust_flags[i] = if idwl_out[i] > 0 { 1 } else { 0 };
    }
}

/// `encode_channel`: full per-channel encode→pack pipeline.
///
/// Takes MDCT spectral data for two channels (as in the original
/// `encode_mddata_at3`), runs the full encoding pipeline, computes the
/// packing bit budget, and writes the bitstream into `out_buffer`.
///
/// Returns the number of bits written, or −1 on error.
#[allow(clippy::too_many_arguments)]
pub fn encode_channel(
    specs_a: &[f32],
    specs_b: &[f32],
    state: &mut EncoderChannelState,
    huff_tables: &HuffTableSet,
    spec_huff: &HuffTableSet,
    out_buffer: &mut [u8],
    buf_offset: &mut i32,
    channel_index: usize,
) -> i32 {
    encode_channel_inner(
        specs_a,
        specs_b,
        state,
        huff_tables,
        spec_huff,
        out_buffer,
        buf_offset,
        channel_index,
    )
    .pack_bits
}

#[derive(Debug, Clone)]
struct EncodeChannelOutcome {
    pack_bits: i32,
    mddata_return_value: i32,
    mddata_final_idwl: [i32; 32],
    mddata_final_spread: [i32; 32],
    mddata_tone_group_count: i32,
    mddata_tone_component_count: usize,
}

#[allow(clippy::too_many_arguments)]
fn encode_channel_inner(
    specs_a: &[f32],
    specs_b: &[f32],
    state: &mut EncoderChannelState,
    huff_tables: &HuffTableSet,
    spec_huff: &HuffTableSet,
    out_buffer: &mut [u8],
    buf_offset: &mut i32,
    channel_index: usize,
) -> EncodeChannelOutcome {
    let result = encode_mddata_at3(specs_a, specs_b, state, huff_tables, spec_huff);
    let mddata_return_value = result.remaining_budget;
    let mddata_final_idwl = state.final_idwl;
    let mddata_final_spread = state.final_spread;
    let mddata_tone_group_count = state.tone_group_count;
    let mddata_tone_component_count = state.tone_components.len();
    if result.remaining_budget == -0x8000 {
        return EncodeChannelOutcome {
            pack_bits: -1,
            mddata_return_value,
            mddata_final_idwl,
            mddata_final_spread,
            mddata_tone_group_count,
            mddata_tone_component_count,
        };
    }

    let mddata_pack_budget = result.remaining_budget.max(0);

    // Build tone packing data from stored extraction output.
    let bfu_count = state.spectral_bfu_count.max(0) as usize;
    let mut pack_components: Vec<crate::dsp::pack::PackToneComponent> = Vec::new();

    for comp in &state.tone_components {
        let width = crate::dsp::quant::twidof_id_at3(comp.width_id as u32).max(0) as usize;
        let mantissas: Vec<i32> = comp.mantissas.iter().take(width).copied().collect();

        pack_components.push(crate::dsp::pack::PackToneComponent {
            idwl: comp.idwl,
            idsf: comp.idsf,
            coded_len: comp.width_id,
            table_idx: comp.table_idx,
            mantissas,
            position: comp.position,
        });
    }

    // Slice output buffer at byte-aligned offset.
    // buf_offset tracks cumulative bits; convert to bytes for slicing.
    let byte_offset = ((*buf_offset as u32 + 7) >> 3) as usize;
    let out_slice = &mut out_buffer[byte_offset..];
    let max_pack_bits = state.total_bit_budget.max(0.0) as i32;

    out_slice.fill(0);
    let mut pack_result = pack_mddata_at3(
        state.packing_enabled,
        state.bfu_count,
        state.spectral_bfu_count,
        state.coding_mode,
        state.table_idx,
        state.tone_group_count,
        pack_components.len() as i32,
        &state.gain_control,
        &pack_components,
        None,
        &state.final_idwl,
        &state.final_spread,
        &state.quantized_mantissas,
        huff_tables,
        spec_huff,
        out_slice,
        mddata_pack_budget,
        channel_index,
        state.joint_stereo != 0,
    );
    if pack_result > max_pack_bits {
        if !pack_components.is_empty() {
            state.diagnostics.tone_payload_drop_events =
                state.diagnostics.tone_payload_drop_events.saturating_add(1);
            pack_components.clear();
            state.tone_components.clear();
            state.tone_group_count = 0;
            out_slice.fill(0);
            pack_result = pack_mddata_at3(
                state.packing_enabled,
                state.bfu_count,
                state.spectral_bfu_count,
                state.coding_mode,
                state.table_idx,
                state.tone_group_count,
                0,
                &state.gain_control,
                &pack_components,
                None,
                &state.final_idwl,
                &state.final_spread,
                &state.quantized_mantissas,
                huff_tables,
                spec_huff,
                out_slice,
                mddata_pack_budget,
                channel_index,
                state.joint_stereo != 0,
            );
        }

        for bfu in (0..bfu_count).rev() {
            while state.final_idwl[bfu] > 0 && pack_result > max_pack_bits {
                state.diagnostics.bfu_idwl_decrement_events = state
                    .diagnostics
                    .bfu_idwl_decrement_events
                    .saturating_add(1);
                state.final_idwl[bfu] -= 1;
                requantize_bfu_for_idwl(bfu, specs_b, state, spec_huff);
                out_slice.fill(0);
                pack_result = pack_mddata_at3(
                    state.packing_enabled,
                    state.bfu_count,
                    state.spectral_bfu_count,
                    state.coding_mode,
                    state.table_idx,
                    state.tone_group_count,
                    pack_components.len() as i32,
                    &state.gain_control,
                    &pack_components,
                    None,
                    &state.final_idwl,
                    &state.final_spread,
                    &state.quantized_mantissas,
                    huff_tables,
                    spec_huff,
                    out_slice,
                    mddata_pack_budget,
                    channel_index,
                    state.joint_stereo != 0,
                );
            }
            if pack_result <= max_pack_bits {
                break;
            }
        }
    }

    if pack_result == -1 || pack_result > max_pack_bits {
        return EncodeChannelOutcome {
            pack_bits: -1,
            mddata_return_value,
            mddata_final_idwl,
            mddata_final_spread,
            mddata_tone_group_count,
            mddata_tone_component_count,
        };
    }
    *buf_offset += pack_result; // accumulate total bits across calls
    EncodeChannelOutcome {
        pack_bits: pack_result,
        mddata_return_value,
        mddata_final_idwl,
        mddata_final_spread,
        mddata_tone_group_count,
        mddata_tone_component_count,
    }
}

fn requantize_bfu_for_idwl(
    bfu: usize,
    specs_b: &[f32],
    state: &mut EncoderChannelState,
    spec_huff: &HuffTableSet,
) {
    let pos = crate::dsp::quant::ispof_iqt_at3(bfu as u32);
    let nsps = crate::dsp::quant::nsps_inqt_at3(bfu as u32);
    if pos < 0 || nsps <= 0 {
        return;
    }

    let start = pos as usize;
    let end = (start + nsps as usize).min(state.quantized_mantissas.len());
    if state.final_idwl[bfu] <= 0 {
        state.adjust_flags[bfu] = 0;
        state.quantized_mantissas[start..end].fill(0);
        return;
    }

    state.adjust_flags[bfu] = 1;
    let sf = crate::dsp::quant::scfof_id_at3(state.final_spread[bfu] as u32) as f32;
    if sf <= 0.0 {
        state.quantized_mantissas[start..end].fill(0);
        return;
    }

    let mut normalized = vec![0.0f32; end - start];
    for (dst, src) in normalized.iter_mut().zip(&specs_b[start..end]) {
        *dst = *src / sf;
    }

    let mut mantissas = vec![0i32; end - start];
    let bits = crate::dsp::quant::quant_nontone_nspecs(
        state.table_idx,
        state.final_idwl[bfu],
        crate::dsp::quant::tfof_id(0, state.final_idwl[bfu]),
        nsps,
        &normalized,
        &mut mantissas,
        spec_huff,
    );
    if bits >= 0 {
        state.quantized_mantissas[start..end].copy_from_slice(&mantissas[..end - start]);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProductionTraceFrameContext {
    pub sound_frame_call_idx: u32,
    pub frame_index: u32,
    pub frame_sequence_arg: i32,
    pub requested_channels: u16,
    pub input_byte_count_arg: u32,
    pub input_byte_count: u32,
    pub input_sample_frame_count: u32,
    pub scheduled_input_sample_frame_start: u64,
    pub scheduled_input_sample_frame_end: u64,
    pub actual_input_sample_frame_start: u64,
    pub actual_input_sample_frame_end: u64,
    pub priming_frame: bool,
    pub write_frame: bool,
    pub payload_offset: Option<u64>,
}

#[derive(Serialize)]
struct RustProductionTraceIndex {
    source: &'static str,
    hook_schema_version: u32,
    hook_profile: &'static str,
    frames_seen: u32,
    stopped_at_max: bool,
    total_hits: u32,
    per_symbol: BTreeMap<&'static str, u32>,
    hook_set: Vec<&'static str>,
    frame_bytes: usize,
    channel_count: u16,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct RustSoundFrameManifest {
    call_idx: u32,
    frame_index: u32,
    frame_sequence_arg: i32,
    input_ptr: u32,
    output_ptr: u32,
    ctx_ptr: u32,
    input_byte_count: u32,
    input_byte_count_arg: u32,
    input_pcm: String,
    scheduled_input_sample_frame_start: u64,
    scheduled_input_sample_frame_end: u64,
    actual_input_sample_frame_start: u64,
    actual_input_sample_frame_end: u64,
    input_sample_frame_count: u32,
    requested_channels: u16,
    ctx_channel_count: u16,
    ctx_frame_bytes: usize,
    payload_offset: Option<u64>,
    priming_frame: bool,
    write_frame: bool,
    skip_frame: bool,
    skip_reason: String,
    nested_at3enc_proc_call_idx: Option<u32>,
    return_value: i32,
    error: bool,
}

#[derive(Serialize)]
struct RustAt3encProcManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame_index: u32,
    frame_sequence_arg: i32,
    param_1_pcm_base: u32,
    param_2_out_base: u32,
    param_3_ctx_base: u32,
    channel_count: u16,
    frame_bytes: usize,
    payload_offset: Option<u64>,
    priming_frame: bool,
    write_frame: bool,
    skip_frame: bool,
    skip_reason: String,
    output_buffer: String,
    output_byte_count: usize,
    return_value: i32,
}

#[derive(Serialize)]
struct RustDbaAt3encProcAltManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame_index: u32,
    frame_sequence_arg: i32,
    param_1_pcm_base: u32,
    param_2_out_base: u32,
    param_3_ctx_base: u32,
    frame_bytes: usize,
    output_buffer: String,
    output_byte_count: usize,
    return_value: i32,
}

#[derive(Serialize)]
struct RustDbaAt3EncEncodeManifest {
    call_idx: u32,
    sample_rate: i32,
    param_2: u32,
    param_3: u32,
    input_sample_count_ptr: u32,
    context_ptr: u32,
    return_value: i32,
}

#[derive(Serialize)]
struct RustDbaQmfManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    channel: u32,
    pcm_input: String,
    history_before: String,
    output_interleaved: String,
    history_after: String,
}

#[derive(Serialize)]
struct RustDbaSelectChconvManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    threshold: i32,
    input_modes: Vec<i32>,
    output_modes: Vec<i32>,
    tonal_spectrum: String,
    nontonal_spectrum: String,
    coefficient: i32,
}

#[derive(Serialize)]
struct RustDbaChconvManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    threshold: i32,
    bands_before: Vec<String>,
    bands_after: Vec<String>,
    modes_before: Vec<i32>,
    modes_after: Vec<i32>,
    abs_modes_before: Vec<i32>,
    abs_modes_after: Vec<i32>,
    smooth_coefficients_before: Vec<u32>,
    smooth_coefficients_after: Vec<u32>,
    target_coefficients_before: Vec<u32>,
    target_coefficients_after: Vec<u32>,
    previous_coefficient_before: i32,
    previous_coefficient_after: i32,
    current_coefficient_before: i32,
    current_coefficient_after: i32,
    energy_history_before: Vec<u32>,
    energy_history_after: Vec<u32>,
}

#[derive(Serialize)]
struct RustDbaGainMdctManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    channel: u32,
    input_bands: String,
    history_before: String,
    mdct_history_before: String,
    pre_mdct_bands: String,
    history_mid: String,
    mdct_history_mid: String,
    output_spectrum: String,
    history_after: String,
    mdct_history_after: String,
    gain_side_info: Vec<i32>,
    gain_side_info_ext: Vec<i32>,
    initial_nunits: i32,
    available_bits: i32,
    channel_mode: i32,
}

#[derive(Serialize)]
struct RustDbaMainsubManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    tonal_spectrum: String,
    tonal_bfu_count: i32,
    nontonal_spectrum: String,
    nontonal_bfu_count: i32,
    base_position: i32,
    position_scale: i32,
    mode: i32,
    fixed_splice: i32,
    splice_position: i32,
}

#[derive(Serialize)]
struct RustDbaAt3dataManifest {
    call_idx: u32,
    at3enc_proc_alt_call_idx: u32,
    channel: u32,
    param_2_channel_block: u32,
    spectrum_in: String,
    initial_nunits: i32,
    available_bits: i32,
    channel_mode: i32,
    prior_tone_counts: Vec<i32>,
    prelude_cumulative_idsfs: Vec<i32>,
    prelude_allocations: Vec<i32>,
    prelude_nunits: i32,
    prelude_ntones: i32,
    prelude_qpoint: i32,
    prelude_bit_budget: i32,
    prelude_high_rate: bool,
    high_rate_spectrum_after: String,
    high_rate_tone_table: String,
    high_rate_idsfs: String,
    high_rate_presence: Vec<i32>,
    high_rate_allocations: Vec<i32>,
    high_rate_ntones: i32,
    high_rate_tone_count: i32,
    high_rate_tone_cost: i32,
    high_rate_bit_budget: i32,
    post_tone_spectrum: String,
    post_tone_idsfs: String,
    post_tone_scores: Vec<i32>,
    post_tone_presence: Vec<i32>,
    post_tone_allocations: Vec<i32>,
    post_tone_ntones: i32,
    post_tone_coding_layout: i32,
    post_tone_mode: i32,
    post_tone_table: String,
    post_tone_cost: i32,
    post_tone_bit_budget: i32,
    post_tone_local_11c: Vec<i32>,
    param_1_0xb56: i32,
    channel_flags: i32,
    balance_spectrum: String,
    balance_idsfs: String,
    balance_tone_table: String,
    balance_presence: Vec<i32>,
    balance_allocations: Vec<i32>,
    balance_scores: Vec<i32>,
    balance_local_11c: Vec<i32>,
    balance_bit_budget: i32,
    balance_tone_mode: i32,
    presence_after: Vec<i32>,
    allocation_after: String,
    nunits: i32,
    ntones: i32,
    tone_table: String,
    return_value: i32,
}

#[derive(Serialize)]
struct RustDbaPackManifest {
    call_idx: u32,
    at3enc_proc_alt_call_idx: u32,
    frame_bytes: usize,
    output_byte_count: usize,
    output_buffer: String,
    return_value: i32,
}

#[derive(Serialize)]
struct RustGainInfoManifest {
    count: i32,
    location: [i32; 7],
    level: [i32; 8],
}

impl From<&GainInfo> for RustGainInfoManifest {
    fn from(value: &GainInfo) -> Self {
        Self {
            count: value.count,
            location: value.location,
            level: value.level,
        }
    }
}

#[derive(Serialize)]
struct RustBandsplitManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    call_in_sound_frame: Option<u32>,
    channel: Option<u32>,
    input: String,
    bands: Vec<String>,
    history_before: Vec<String>,
    history_after: Vec<String>,
}

#[derive(Serialize)]
struct RustQmfManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    call_in_sound_frame: Option<u32>,
    channel: Option<u32>,
    qmf_stage: Option<u32>,
    sample_count: usize,
    input: String,
    lower: String,
    upper: String,
    history_before: String,
    history_after: String,
}

#[derive(Serialize)]
struct RustGaincontrolManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    call_in_sound_frame: Option<u32>,
    channel: Option<u32>,
    inputs: Vec<String>,
    current: Vec<RustGainInfoManifest>,
    next_before: Vec<RustGainInfoManifest>,
    next_after: Vec<RustGainInfoManifest>,
    return_value: i32,
}

#[derive(Serialize)]
struct RustMdctManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    call_in_sound_frame: Option<u32>,
    channel: Option<u32>,
    band: u32,
    parity: u32,
    input: String,
    output: String,
}

#[derive(Serialize)]
struct RustEncodeMddataManifest {
    call_idx: u32,
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    call_in_sound_frame: Option<u32>,
    channel: Option<u32>,
    spectral_a: String,
    spectral_b: String,
    idwl_array_before: String,
    total_bit_budget: f32,
    joint_stereo: i32,
    tone_group_count: i32,
    tone_component_count: usize,
    final_idwl_after: String,
    final_spread_after: String,
    checkpoints: Vec<serde_json::Value>,
    return_value: i32,
}

#[derive(Clone, Copy)]
struct RustTraceContext {
    sound_frame_call_idx: Option<u32>,
    frame: u32,
    frame_sequence_arg: i32,
    call_in_sound_frame: Option<u32>,
}

struct ProductionTraceWriter {
    out_dir: PathBuf,
    frame_bytes: usize,
    channel_count: u16,
    max_sound_frames: Option<u32>,
    stopped_at_max: bool,
    recording_enabled: bool,
    active_frame: Option<ProductionTraceFrameContext>,
    sound_frames: Vec<RustSoundFrameManifest>,
    at3enc_proc: Vec<RustAt3encProcManifest>,
    bandsplit: Vec<RustBandsplitManifest>,
    qmf: Vec<RustQmfManifest>,
    gaincontrol: Vec<RustGaincontrolManifest>,
    mdct: Vec<RustMdctManifest>,
    encode_mddata: Vec<RustEncodeMddataManifest>,
    dba_qmf: Vec<RustDbaQmfManifest>,
    dba_select_chconv: Vec<RustDbaSelectChconvManifest>,
    dba_chconv: Vec<RustDbaChconvManifest>,
    dba_gain_mdct: Vec<RustDbaGainMdctManifest>,
    dba_mainsub: Vec<RustDbaMainsubManifest>,
    dba_at3data: Vec<RustDbaAt3dataManifest>,
    dba_pack: Vec<RustDbaPackManifest>,
    dba_frame_output: Vec<RustDbaPackManifest>,
    production_call_counts: BTreeMap<&'static str, BTreeMap<u32, u32>>,
    errors: Vec<String>,
}

impl ProductionTraceWriter {
    fn new(
        out_dir: PathBuf,
        frame_bytes: usize,
        channel_count: u16,
        max_sound_frames: Option<u32>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&out_dir)?;
        Ok(Self {
            out_dir,
            frame_bytes,
            channel_count,
            max_sound_frames,
            stopped_at_max: false,
            recording_enabled: true,
            active_frame: None,
            sound_frames: Vec::new(),
            at3enc_proc: Vec::new(),
            bandsplit: Vec::new(),
            qmf: Vec::new(),
            gaincontrol: Vec::new(),
            mdct: Vec::new(),
            encode_mddata: Vec::new(),
            dba_qmf: Vec::new(),
            dba_select_chconv: Vec::new(),
            dba_chconv: Vec::new(),
            dba_gain_mdct: Vec::new(),
            dba_mainsub: Vec::new(),
            dba_at3data: Vec::new(),
            dba_pack: Vec::new(),
            dba_frame_output: Vec::new(),
            production_call_counts: BTreeMap::new(),
            errors: Vec::new(),
        })
    }

    fn begin_sound_frame(
        &mut self,
        context: ProductionTraceFrameContext,
        input_pcm: Option<&[u8]>,
    ) -> std::io::Result<()> {
        if self
            .max_sound_frames
            .is_some_and(|max| self.sound_frames.len() as u32 >= max)
        {
            self.stopped_at_max = true;
            self.recording_enabled = false;
            self.active_frame = None;
            return Ok(());
        }
        self.recording_enabled = true;
        let input_pcm = if let Some(input_pcm) = input_pcm {
            let name = format!("sound_frame_{}_input_pcm.bin", context.sound_frame_call_idx);
            std::fs::write(self.out_dir.join(&name), input_pcm)?;
            name
        } else {
            String::new()
        };
        self.active_frame = Some(context);
        self.sound_frames.push(RustSoundFrameManifest {
            call_idx: context.sound_frame_call_idx,
            frame_index: context.frame_index,
            frame_sequence_arg: context.frame_sequence_arg,
            input_ptr: 0,
            output_ptr: 0,
            ctx_ptr: 0,
            input_byte_count: context.input_byte_count,
            input_byte_count_arg: context.input_byte_count_arg,
            input_pcm,
            scheduled_input_sample_frame_start: context.scheduled_input_sample_frame_start,
            scheduled_input_sample_frame_end: context.scheduled_input_sample_frame_end,
            actual_input_sample_frame_start: context.actual_input_sample_frame_start,
            actual_input_sample_frame_end: context.actual_input_sample_frame_end,
            input_sample_frame_count: context.input_sample_frame_count,
            requested_channels: context.requested_channels,
            ctx_channel_count: self.channel_count,
            ctx_frame_bytes: self.frame_bytes,
            payload_offset: context.payload_offset,
            priming_frame: context.priming_frame,
            write_frame: context.write_frame,
            skip_frame: !context.write_frame,
            skip_reason: if context.write_frame {
                String::new()
            } else if context.priming_frame {
                "priming".to_string()
            } else {
                "not_written".to_string()
            },
            nested_at3enc_proc_call_idx: None,
            return_value: 0,
            error: false,
        });
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording_enabled
    }

    fn begin_at3enc_proc(&mut self) -> Option<u32> {
        if !self.recording_enabled {
            return None;
        }
        let call_idx = self.at3enc_proc.len() as u32;
        let context = self.active_frame;
        let sound_frame_call_idx = context.map(|ctx| ctx.sound_frame_call_idx);
        let frame_index = context.map_or(call_idx + 1, |ctx| ctx.frame_index);
        let frame_sequence_arg = context.map_or(call_idx as i32, |ctx| ctx.frame_sequence_arg);
        let payload_offset = context.and_then(|ctx| ctx.payload_offset);
        let priming_frame = context.is_some_and(|ctx| ctx.priming_frame);
        let write_frame = context.is_none_or(|ctx| ctx.write_frame);
        for cap in &mut self.sound_frames {
            if Some(cap.call_idx) == sound_frame_call_idx {
                cap.nested_at3enc_proc_call_idx = Some(call_idx);
                break;
            }
        }
        self.at3enc_proc.push(RustAt3encProcManifest {
            call_idx,
            sound_frame_call_idx,
            frame_index,
            frame_sequence_arg,
            param_1_pcm_base: 0,
            param_2_out_base: 0,
            param_3_ctx_base: 0,
            channel_count: self.channel_count,
            frame_bytes: self.frame_bytes,
            payload_offset,
            priming_frame,
            write_frame,
            skip_frame: !write_frame,
            skip_reason: if write_frame {
                String::new()
            } else if priming_frame {
                "priming".to_string()
            } else {
                "not_written".to_string()
            },
            output_buffer: String::new(),
            output_byte_count: 0,
            return_value: 0,
        });
        Some(call_idx)
    }

    fn finish_at3enc_proc(
        &mut self,
        call_idx: u32,
        bit_count: i32,
        output: Option<&[u8]>,
    ) -> Option<String> {
        let byte_count = if bit_count >= 0 {
            ((bit_count as u32 + 7) >> 3) as usize
        } else {
            0
        };
        let return_value = if bit_count >= 0 { 0 } else { bit_count };
        let output_buffer = if byte_count != 0 {
            output.map(|output| {
                let name = format!("at3enc_proc_{call_idx}_out.bin");
                self.write_bytes_blob(name, &output[..byte_count.min(output.len())])
            })
        } else {
            None
        };
        if let Some(cap) = self
            .at3enc_proc
            .iter_mut()
            .find(|cap| cap.call_idx == call_idx)
        {
            cap.return_value = return_value;
            cap.output_byte_count = byte_count;
            if let Some(name) = &output_buffer {
                cap.output_buffer = name.clone();
            }
            if bit_count < 0 {
                cap.write_frame = false;
                cap.skip_frame = true;
                cap.skip_reason = "error".to_string();
            }
        }
        if let Some(context) = self.active_frame.take()
            && let Some(cap) = self
                .sound_frames
                .iter_mut()
                .find(|cap| cap.call_idx == context.sound_frame_call_idx)
        {
            cap.return_value = return_value;
            cap.error = bit_count < 0;
            if bit_count < 0 {
                cap.write_frame = false;
                cap.skip_frame = true;
                cap.skip_reason = "error".to_string();
            }
        }
        output_buffer
    }

    fn next_context(&mut self, kind: &'static str) -> RustTraceContext {
        let sound_frame_call_idx = self.active_frame.map(|ctx| ctx.sound_frame_call_idx);
        let frame = self
            .active_frame
            .map_or(self.sound_frames.len() as u32, |ctx| ctx.frame_index);
        let frame_sequence_arg = self
            .active_frame
            .map_or(frame.saturating_sub(1) as i32, |ctx| ctx.frame_sequence_arg);
        let call_in_sound_frame = sound_frame_call_idx.map(|idx| {
            let by_frame = self.production_call_counts.entry(kind).or_default();
            let next = by_frame.get(&idx).copied().unwrap_or(0);
            by_frame.insert(idx, next + 1);
            next
        });
        RustTraceContext {
            sound_frame_call_idx,
            frame,
            frame_sequence_arg,
            call_in_sound_frame,
        }
    }

    fn record_bandsplit(
        &mut self,
        channel: usize,
        pcm: &[f32; 1024],
        bands: &[[f32; 256]; 4],
        qmf_traces: &[crate::dsp::qmf::QmfTraceCapture],
    ) {
        for trace in qmf_traces {
            self.record_qmf(trace);
        }
        let idx = self.bandsplit.len() as u32;
        let ctx = self.next_context("bandsplit");
        let input = self.write_f32_blob(format!("bandsplit_pcm_{idx}.bin"), pcm);
        let mut band_names = Vec::new();
        for (band, values) in bands.iter().enumerate() {
            band_names.push(self.write_f32_blob(format!("bandsplit_band{band}_{idx}.bin"), values));
        }
        let mut history_before = Vec::new();
        let mut history_after = Vec::new();
        for (stage, trace) in qmf_traces.iter().enumerate() {
            history_before.push(self.write_f32_blob(
                format!("bandsplit_history{stage}_before_{idx}.bin"),
                &trace.history_before,
            ));
            history_after.push(self.write_f32_blob(
                format!("bandsplit_history{stage}_after_{idx}.bin"),
                &trace.history_after,
            ));
        }
        self.bandsplit.push(RustBandsplitManifest {
            call_idx: idx,
            sound_frame_call_idx: ctx.sound_frame_call_idx,
            frame: ctx.frame,
            frame_sequence_arg: ctx.frame_sequence_arg,
            call_in_sound_frame: ctx.call_in_sound_frame,
            channel: Some(channel as u32),
            input,
            bands: band_names,
            history_before,
            history_after,
        });
    }

    fn record_qmf(&mut self, trace: &crate::dsp::qmf::QmfTraceCapture) {
        let idx = self.qmf.len() as u32;
        let ctx = self.next_context("qmf");
        let local = ctx.call_in_sound_frame.unwrap_or(idx);
        let input = self.write_f32_blob(format!("qmf_in_{idx}.bin"), &trace.input);
        let lower = self.write_f32_blob(format!("qmf_lower_{idx}.bin"), &trace.lower);
        let upper = self.write_f32_blob(format!("qmf_upper_{idx}.bin"), &trace.upper);
        let history_before = self.write_f32_blob(
            format!("qmf_history_before_{idx}.bin"),
            &trace.history_before,
        );
        let history_after =
            self.write_f32_blob(format!("qmf_history_after_{idx}.bin"), &trace.history_after);
        self.qmf.push(RustQmfManifest {
            call_idx: idx,
            sound_frame_call_idx: ctx.sound_frame_call_idx,
            frame: ctx.frame,
            frame_sequence_arg: ctx.frame_sequence_arg,
            call_in_sound_frame: ctx.call_in_sound_frame,
            channel: Some(local / 3),
            qmf_stage: Some(local % 3),
            sample_count: trace.input.len(),
            input,
            lower,
            upper,
            history_before,
            history_after,
        });
    }

    fn record_gaincontrol(
        &mut self,
        channel: usize,
        inputs: [&[f32]; 4],
        current: &[GainInfo; 4],
        next_before: &[GainInfo; 4],
        next_after: &[GainInfo; 4],
        return_value: i32,
    ) {
        let idx = self.gaincontrol.len() as u32;
        let ctx = self.next_context("gaincontrol");
        let mut input_names = Vec::new();
        for (band, input) in inputs.iter().enumerate() {
            input_names.push(
                self.write_f32_blob(format!("gaincontrol_{idx}_band{band}_input.bin"), input),
            );
        }
        self.gaincontrol.push(RustGaincontrolManifest {
            call_idx: idx,
            sound_frame_call_idx: ctx.sound_frame_call_idx,
            frame: ctx.frame,
            frame_sequence_arg: ctx.frame_sequence_arg,
            call_in_sound_frame: ctx.call_in_sound_frame,
            channel: Some(channel as u32),
            inputs: input_names,
            current: current.iter().map(RustGainInfoManifest::from).collect(),
            next_before: next_before.iter().map(RustGainInfoManifest::from).collect(),
            next_after: next_after.iter().map(RustGainInfoManifest::from).collect(),
            return_value,
        });
    }

    fn record_mdct(&mut self, channel: usize, inputs: &[[f32; 512]; 4], outputs: &[[f32; 256]; 4]) {
        const PARITY: [u32; 4] = [0, 1, 0, 1];
        for band in 0..4 {
            let idx = self.mdct.len() as u32;
            let ctx = self.next_context("mdct");
            let input = self.write_f32_blob(format!("mdct_in_{idx}.bin"), inputs[band]);
            let output = self.write_f32_blob(format!("mdct_out_{idx}.bin"), outputs[band]);
            self.mdct.push(RustMdctManifest {
                call_idx: idx,
                sound_frame_call_idx: ctx.sound_frame_call_idx,
                frame: ctx.frame,
                frame_sequence_arg: ctx.frame_sequence_arg,
                call_in_sound_frame: ctx.call_in_sound_frame,
                channel: Some(channel as u32),
                band: band as u32,
                parity: PARITY[band],
                input,
                output,
            });
        }
    }

    fn begin_encode_mddata(
        &mut self,
        channel: usize,
        specs_a: &[f32; 1024],
        specs_b: &[f32; 1024],
        state: &EncoderChannelState,
    ) -> u32 {
        let idx = self.encode_mddata.len() as u32;
        let ctx = self.next_context("encode_mddata");
        let spectral_a = self.write_f32_blob(format!("encode_mddata_{idx}_spec_a.bin"), specs_a);
        let spectral_b = self.write_f32_blob(format!("encode_mddata_{idx}_spec_b.bin"), specs_b);
        let idwl_array_before = self.write_i32_blob(
            format!("encode_mddata_{idx}_idwl_before.bin"),
            state.final_idwl,
        );
        self.encode_mddata.push(RustEncodeMddataManifest {
            call_idx: idx,
            sound_frame_call_idx: ctx.sound_frame_call_idx,
            frame: ctx.frame,
            frame_sequence_arg: ctx.frame_sequence_arg,
            call_in_sound_frame: ctx.call_in_sound_frame,
            channel: Some(channel as u32),
            spectral_a,
            spectral_b,
            idwl_array_before,
            total_bit_budget: state.total_bit_budget,
            joint_stereo: state.joint_stereo,
            tone_group_count: 0,
            tone_component_count: 0,
            final_idwl_after: String::new(),
            final_spread_after: String::new(),
            checkpoints: Vec::new(),
            return_value: 0,
        });
        idx
    }

    fn finish_encode_mddata(&mut self, call_idx: u32, outcome: &EncodeChannelOutcome) {
        let final_idwl = self.write_i32_blob(
            format!("encode_mddata_{call_idx}_final_idwl.bin"),
            outcome.mddata_final_idwl,
        );
        let final_spread = self.write_i32_blob(
            format!("encode_mddata_{call_idx}_final_spread.bin"),
            outcome.mddata_final_spread,
        );
        if let Some(cap) = self
            .encode_mddata
            .iter_mut()
            .find(|cap| cap.call_idx == call_idx)
        {
            cap.return_value = outcome.mddata_return_value;
            cap.tone_group_count = outcome.mddata_tone_group_count;
            cap.tone_component_count = outcome.mddata_tone_component_count;
            cap.final_idwl_after = final_idwl;
            cap.final_spread_after = final_spread;
        }
    }

    fn record_dba_frame_trace(
        &mut self,
        at3enc_proc_alt_call_idx: u32,
        frame_trace: &crate::dsp::dba::DbaProductionFrameTrace,
    ) {
        let ctx = self.next_context("dba_frame");
        for cap in &frame_trace.qmf {
            let pcm_input = self.write_f32_blob(
                format!(
                    "dba_qmf_{at3enc_proc_alt_call_idx}_ch{}_pcm.bin",
                    cap.channel
                ),
                cap.pcm_input,
            );
            let history_before = self.write_f32_blob(
                format!(
                    "dba_qmf_{at3enc_proc_alt_call_idx}_ch{}_hist_before.bin",
                    cap.channel
                ),
                cap.history_before,
            );
            let output_interleaved = self.write_f32_blob(
                format!(
                    "dba_qmf_{at3enc_proc_alt_call_idx}_ch{}_out.bin",
                    cap.channel
                ),
                cap.output_interleaved,
            );
            let history_after = self.write_f32_blob(
                format!(
                    "dba_qmf_{at3enc_proc_alt_call_idx}_ch{}_hist_after.bin",
                    cap.channel
                ),
                cap.history_after,
            );
            self.dba_qmf.push(RustDbaQmfManifest {
                call_idx: at3enc_proc_alt_call_idx,
                sound_frame_call_idx: ctx.sound_frame_call_idx,
                frame: ctx.frame,
                frame_sequence_arg: ctx.frame_sequence_arg,
                channel: cap.channel,
                pcm_input,
                history_before,
                output_interleaved,
                history_after,
            });
        }

        for cap in &frame_trace.select_chconv {
            let tonal_spectrum = self.write_f32_blob(
                format!("select_chconv_{at3enc_proc_alt_call_idx}_tonal.bin"),
                cap.tonal_spectrum,
            );
            let nontonal_spectrum = self.write_f32_blob(
                format!("select_chconv_{at3enc_proc_alt_call_idx}_nontonal.bin"),
                cap.nontonal_spectrum,
            );
            self.dba_select_chconv.push(RustDbaSelectChconvManifest {
                call_idx: at3enc_proc_alt_call_idx,
                sound_frame_call_idx: ctx.sound_frame_call_idx,
                frame: ctx.frame,
                frame_sequence_arg: ctx.frame_sequence_arg,
                threshold: cap.threshold,
                input_modes: cap.input_modes.to_vec(),
                output_modes: cap.output_modes.to_vec(),
                tonal_spectrum,
                nontonal_spectrum,
                coefficient: cap.coefficient,
            });
        }

        for cap in &frame_trace.chconv {
            let bands_before = vec![
                self.write_f32_blob(
                    format!("dba_chconv_{at3enc_proc_alt_call_idx}_ch0_before.bin"),
                    cap.bands_before[0],
                ),
                self.write_f32_blob(
                    format!("dba_chconv_{at3enc_proc_alt_call_idx}_ch1_before.bin"),
                    cap.bands_before[1],
                ),
            ];
            let bands_after = vec![
                self.write_f32_blob(
                    format!("dba_chconv_{at3enc_proc_alt_call_idx}_ch0_after.bin"),
                    cap.bands_after[0],
                ),
                self.write_f32_blob(
                    format!("dba_chconv_{at3enc_proc_alt_call_idx}_ch1_after.bin"),
                    cap.bands_after[1],
                ),
            ];
            self.dba_chconv.push(RustDbaChconvManifest {
                call_idx: at3enc_proc_alt_call_idx,
                sound_frame_call_idx: ctx.sound_frame_call_idx,
                frame: ctx.frame,
                frame_sequence_arg: ctx.frame_sequence_arg,
                threshold: cap.threshold,
                bands_before,
                bands_after,
                modes_before: cap.modes_before.to_vec(),
                modes_after: cap.modes_after.to_vec(),
                abs_modes_before: cap.abs_modes_before.to_vec(),
                abs_modes_after: cap.abs_modes_after.to_vec(),
                smooth_coefficients_before: Self::f32_bits(&cap.smooth_coefficients_before),
                smooth_coefficients_after: Self::f32_bits(&cap.smooth_coefficients_after),
                target_coefficients_before: Self::f32_bits(&cap.target_coefficients_before),
                target_coefficients_after: Self::f32_bits(&cap.target_coefficients_after),
                previous_coefficient_before: cap.previous_coefficient_before,
                previous_coefficient_after: cap.previous_coefficient_after,
                current_coefficient_before: cap.current_coefficient_before,
                current_coefficient_after: cap.current_coefficient_after,
                energy_history_before: Self::chconv_energy_bits(&cap.energy_history_before),
                energy_history_after: Self::chconv_energy_bits(&cap.energy_history_after),
            });
        }

        for cap in &frame_trace.gain_mdct {
            let channel = cap.channel;
            let input_bands = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_input.bin"),
                cap.input_bands,
            );
            let history_before = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_hist_before.bin"),
                cap.history_before,
            );
            let mdct_history_before = self.write_f32_blob(
                format!(
                    "dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_mdct_hist_before.bin"
                ),
                cap.mdct_history_before,
            );
            let pre_mdct_bands = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_pre.bin"),
                cap.pre_mdct_bands,
            );
            let history_mid = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_hist_mid.bin"),
                cap.history_mid,
            );
            let mdct_history_mid = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_mdct_hist_mid.bin"),
                cap.mdct_history_mid,
            );
            let output_spectrum = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_out.bin"),
                cap.output_spectrum,
            );
            let history_after = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_hist_after.bin"),
                cap.history_after,
            );
            let mdct_history_after = self.write_f32_blob(
                format!("dba_gain_mdct_{at3enc_proc_alt_call_idx}_ch{channel}_mdct_hist_after.bin"),
                cap.mdct_history_after,
            );
            self.dba_gain_mdct.push(RustDbaGainMdctManifest {
                call_idx: at3enc_proc_alt_call_idx,
                sound_frame_call_idx: ctx.sound_frame_call_idx,
                frame: ctx.frame,
                frame_sequence_arg: ctx.frame_sequence_arg,
                channel,
                input_bands,
                history_before,
                mdct_history_before,
                pre_mdct_bands,
                history_mid,
                mdct_history_mid,
                output_spectrum,
                history_after,
                mdct_history_after,
                gain_side_info: cap.gain_side_info.to_vec(),
                gain_side_info_ext: cap.gain_side_info_ext.to_vec(),
                initial_nunits: cap.initial_nunits,
                available_bits: cap.available_bits,
                channel_mode: cap.channel_mode,
            });
        }

        for cap in &frame_trace.mainsub {
            let tonal_spectrum = self.write_f32_blob(
                format!("dba_mainsub_{at3enc_proc_alt_call_idx}_tonal.bin"),
                cap.tonal_spectrum,
            );
            let nontonal_spectrum = self.write_f32_blob(
                format!("dba_mainsub_{at3enc_proc_alt_call_idx}_nontonal.bin"),
                cap.nontonal_spectrum,
            );
            self.dba_mainsub.push(RustDbaMainsubManifest {
                call_idx: at3enc_proc_alt_call_idx,
                sound_frame_call_idx: ctx.sound_frame_call_idx,
                frame: ctx.frame,
                frame_sequence_arg: ctx.frame_sequence_arg,
                tonal_spectrum,
                tonal_bfu_count: cap.tonal_bfu_count,
                nontonal_spectrum,
                nontonal_bfu_count: cap.nontonal_bfu_count,
                base_position: cap.base_position,
                position_scale: cap.position_scale,
                mode: cap.mode,
                fixed_splice: cap.fixed_splice,
                splice_position: cap.splice_position,
            });
        }

        for cap in &frame_trace.at3data {
            let idx = self.dba_at3data.len() as u32;
            let spectrum_in = self.write_f32_blob(
                format!("dba_at3data_{idx}_spectrum_in.bin"),
                cap.spectrum_in,
            );
            let balance_spectrum = self.write_f32_blob(
                format!("dba_at3data_{idx}_balance_spectrum.bin"),
                cap.balance_spectrum,
            );
            let balance_idsfs = self.write_u32_blob(
                format!("dba_at3data_{idx}_balance_idsfs.bin"),
                cap.balance_idsfs,
            );
            let allocation_after = self.write_i32_blob(
                format!("dba_at3data_{idx}_allocation_after.bin"),
                cap.allocation_after,
            );
            let component_base = cap.param_2_channel_block.wrapping_add(0x540);
            let tone_words =
                crate::dsp::dba::dba_tone_table_trace_words(&cap.tone_table, component_base);
            let tone_table =
                self.write_i32_blob(format!("dba_at3data_{idx}_tone_table.bin"), tone_words);
            let zero_scores = vec![0; 33];
            self.dba_at3data.push(RustDbaAt3dataManifest {
                call_idx: idx,
                at3enc_proc_alt_call_idx,
                channel: cap.channel,
                param_2_channel_block: cap.param_2_channel_block,
                spectrum_in,
                initial_nunits: cap.initial_nunits,
                available_bits: cap.available_bits,
                channel_mode: cap.channel_mode,
                prior_tone_counts: cap.prior_tone_counts.to_vec(),
                prelude_cumulative_idsfs: cap.presence_after.to_vec(),
                prelude_allocations: cap.allocation_after.to_vec(),
                prelude_nunits: cap.nunits,
                prelude_ntones: cap.ntones,
                prelude_qpoint: 0,
                prelude_bit_budget: cap.available_bits,
                prelude_high_rate: cap.prelude_high_rate,
                high_rate_spectrum_after: balance_spectrum.clone(),
                high_rate_tone_table: tone_table.clone(),
                high_rate_idsfs: balance_idsfs.clone(),
                high_rate_presence: cap.presence_after.to_vec(),
                high_rate_allocations: cap.allocation_after.to_vec(),
                high_rate_ntones: cap.ntones,
                high_rate_tone_count: cap.tone_table.components.len() as i32,
                high_rate_tone_cost: 0,
                high_rate_bit_budget: cap.return_value,
                post_tone_spectrum: balance_spectrum.clone(),
                post_tone_idsfs: balance_idsfs.clone(),
                post_tone_scores: zero_scores.clone(),
                post_tone_presence: cap.presence_after.to_vec(),
                post_tone_allocations: cap.allocation_after.to_vec(),
                post_tone_ntones: cap.ntones,
                post_tone_coding_layout: cap.post_tone_coding_layout,
                post_tone_mode: cap.balance_tone_mode,
                post_tone_table: tone_table.clone(),
                post_tone_cost: 0,
                post_tone_bit_budget: cap.return_value,
                post_tone_local_11c: vec![0; 32],
                param_1_0xb56: cap.param_1_0xb56,
                channel_flags: cap.channel_flags,
                balance_spectrum,
                balance_idsfs,
                balance_tone_table: tone_table.clone(),
                balance_presence: cap.presence_after.to_vec(),
                balance_allocations: cap.allocation_after.to_vec(),
                balance_scores: zero_scores,
                balance_local_11c: vec![0; 32],
                balance_bit_budget: cap.return_value,
                balance_tone_mode: cap.balance_tone_mode,
                presence_after: cap.presence_after.to_vec(),
                allocation_after,
                nunits: cap.nunits,
                ntones: cap.ntones,
                tone_table,
                return_value: cap.return_value,
            });
        }
    }

    fn record_dba_pack_frame_output(
        &mut self,
        at3enc_proc_alt_call_idx: u32,
        output_buffer: Option<String>,
        bit_count: i32,
    ) {
        let return_value = if bit_count >= 0 { 0 } else { bit_count };
        let output_byte_count = if bit_count >= 0 { self.frame_bytes } else { 0 };
        let output_buffer = output_buffer.unwrap_or_default();
        let pack = RustDbaPackManifest {
            call_idx: at3enc_proc_alt_call_idx,
            at3enc_proc_alt_call_idx,
            frame_bytes: self.frame_bytes,
            output_byte_count,
            output_buffer: output_buffer.clone(),
            return_value,
        };
        let frame = RustDbaPackManifest {
            call_idx: at3enc_proc_alt_call_idx,
            at3enc_proc_alt_call_idx,
            frame_bytes: self.frame_bytes,
            output_byte_count,
            output_buffer,
            return_value,
        };
        self.dba_pack.push(pack);
        self.dba_frame_output.push(frame);
    }

    fn f32_bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    fn chconv_energy_bits(history: &[[[f32; 2]; 3]; 4]) -> Vec<u32> {
        history
            .iter()
            .flat_map(|band| band.iter())
            .flat_map(|slot| slot.iter())
            .map(|value| value.to_bits())
            .collect()
    }

    fn finish(&mut self) -> std::io::Result<()> {
        self.write_json("sound_frame.json", &self.sound_frames)?;
        self.write_json("at3enc_proc.json", &self.at3enc_proc)?;
        let dba_active = !self.dba_qmf.is_empty()
            || !self.dba_gain_mdct.is_empty()
            || !self.dba_at3data.is_empty();
        if dba_active {
            let at3enc_encode: Vec<_> = self
                .at3enc_proc
                .iter()
                .map(|cap| RustDbaAt3EncEncodeManifest {
                    call_idx: cap.call_idx,
                    sample_rate: 0,
                    param_2: 0,
                    param_3: 0,
                    input_sample_count_ptr: 0,
                    context_ptr: 0,
                    return_value: cap.return_value,
                })
                .collect();
            let at3enc_proc_alt: Vec<_> = self
                .at3enc_proc
                .iter()
                .map(|cap| RustDbaAt3encProcAltManifest {
                    call_idx: cap.call_idx,
                    sound_frame_call_idx: cap.sound_frame_call_idx,
                    frame_index: cap.frame_index,
                    frame_sequence_arg: cap.frame_sequence_arg,
                    param_1_pcm_base: cap.param_1_pcm_base,
                    param_2_out_base: cap.param_2_out_base,
                    param_3_ctx_base: cap.param_3_ctx_base,
                    frame_bytes: cap.frame_bytes,
                    output_buffer: cap.output_buffer.clone(),
                    output_byte_count: cap.output_byte_count,
                    return_value: cap.return_value,
                })
                .collect();
            self.write_json("at3EncEncode.json", &at3enc_encode)?;
            self.write_json("at3enc_proc_alt.json", &at3enc_proc_alt)?;
        }
        if !self.bandsplit.is_empty() {
            self.write_json("bandsplit.json", &self.bandsplit)?;
        }
        if !self.qmf.is_empty() {
            self.write_json("qmf.json", &self.qmf)?;
        }
        if !self.gaincontrol.is_empty() {
            self.write_json("gaincontrol.json", &self.gaincontrol)?;
        }
        if !self.mdct.is_empty() {
            self.write_json("mdct.json", &self.mdct)?;
        }
        if !self.encode_mddata.is_empty() {
            self.write_json("encode_mddata.json", &self.encode_mddata)?;
        }
        if !self.dba_qmf.is_empty() {
            self.write_json("dba_qmf.json", &self.dba_qmf)?;
        }
        if !self.dba_select_chconv.is_empty() {
            self.write_json("select_chconv.json", &self.dba_select_chconv)?;
        }
        if !self.dba_chconv.is_empty() {
            self.write_json("dba_chconv.json", &self.dba_chconv)?;
        }
        if !self.dba_gain_mdct.is_empty() {
            self.write_json("dba_gain_mdct.json", &self.dba_gain_mdct)?;
        }
        if !self.dba_mainsub.is_empty() {
            self.write_json("dba_mainsub.json", &self.dba_mainsub)?;
        }
        if !self.dba_at3data.is_empty() {
            self.write_json("dba_at3data.json", &self.dba_at3data)?;
        }
        if !self.dba_pack.is_empty() {
            self.write_json("dba_pack.json", &self.dba_pack)?;
        }
        if !self.dba_frame_output.is_empty() {
            self.write_json("dba_frame_output.json", &self.dba_frame_output)?;
        }
        let mut per_symbol = BTreeMap::new();
        per_symbol.insert("Atrac3EncodeSoundFrame", self.sound_frames.len() as u32);
        per_symbol.insert(
            "Atrac3EncodeSoundFrame:return",
            self.sound_frames.len() as u32,
        );
        per_symbol.insert("at3enc_proc", self.at3enc_proc.len() as u32);
        per_symbol.insert("at3enc_proc:return", self.at3enc_proc.len() as u32);
        if dba_active {
            per_symbol.insert("at3EncEncode", self.sound_frames.len() as u32);
            per_symbol.insert("at3EncEncode:return", self.sound_frames.len() as u32);
            per_symbol.insert("at3enc_proc_alt", self.at3enc_proc.len() as u32);
            per_symbol.insert("at3enc_proc_alt:return", self.at3enc_proc.len() as u32);
            per_symbol.insert("dba_qmf", self.dba_qmf.len() as u32);
            per_symbol.insert("dba_qmf:return", self.dba_qmf.len() as u32);
            per_symbol.insert("select_chconv", self.dba_select_chconv.len() as u32);
            per_symbol.insert("select_chconv:return", self.dba_select_chconv.len() as u32);
            per_symbol.insert("dba_chconv", self.dba_chconv.len() as u32);
            per_symbol.insert("dba_chconv:return", self.dba_chconv.len() as u32);
            per_symbol.insert("dba_gain_mdct", self.dba_gain_mdct.len() as u32);
            per_symbol.insert("dba_gain_mdct:return", self.dba_gain_mdct.len() as u32);
            per_symbol.insert("dba_mainsub", self.dba_mainsub.len() as u32);
            per_symbol.insert("dba_mainsub:return", self.dba_mainsub.len() as u32);
            per_symbol.insert("dba_at3data", self.dba_at3data.len() as u32);
            per_symbol.insert("dba_at3data:return", self.dba_at3data.len() as u32);
            per_symbol.insert("dba_pack", self.dba_pack.len() as u32);
            per_symbol.insert("dba_pack:return", self.dba_pack.len() as u32);
            per_symbol.insert("dba_frame_output", self.dba_frame_output.len() as u32);
        } else if !self.bandsplit.is_empty() {
            per_symbol.insert("bandsplit_at3", self.bandsplit.len() as u32);
            per_symbol.insert("bandsplit_at3:return", self.bandsplit.len() as u32);
            per_symbol.insert("qmf", self.qmf.len() as u32);
            per_symbol.insert("qmf:return", self.qmf.len() as u32);
            per_symbol.insert("gaincontrol_at3", self.gaincontrol.len() as u32);
            per_symbol.insert("gaincontrol_at3:return", self.gaincontrol.len() as u32);
            per_symbol.insert("winormal_mdct_256", self.mdct.len() as u32);
            per_symbol.insert("winormal_mdct_256:return", self.mdct.len() as u32);
            per_symbol.insert("encode_mddata_at3", self.encode_mddata.len() as u32);
            per_symbol.insert("encode_mddata_at3:return", self.encode_mddata.len() as u32);
        }
        let total_hits = per_symbol.values().copied().sum();
        let index = RustProductionTraceIndex {
            source: "rust-production-trace",
            hook_schema_version: 3,
            hook_profile: if dba_active {
                "dba-production"
            } else {
                "production"
            },
            frames_seen: self.sound_frames.len() as u32,
            stopped_at_max: self.stopped_at_max,
            total_hits,
            per_symbol,
            hook_set: if dba_active {
                vec![
                    "Atrac3EncodeSoundFrame",
                    "at3EncEncode",
                    "at3enc_proc",
                    "at3enc_proc_alt",
                    "dba_qmf",
                    "select_chconv",
                    "dba_chconv",
                    "dba_gain_mdct",
                    "dba_mainsub",
                    "dba_at3data",
                    "dba_pack",
                    "dba_frame_output",
                ]
            } else {
                vec![
                    "Atrac3EncodeSoundFrame",
                    "at3enc_proc",
                    "bandsplit_at3",
                    "qmf",
                    "gaincontrol_at3",
                    "winormal_mdct_256",
                    "encode_mddata_at3",
                ]
            },
            frame_bytes: self.frame_bytes,
            channel_count: self.channel_count,
            errors: self.errors.clone(),
        };
        self.write_json("index.json", &index)
    }

    fn write_f32_blob<T: AsRef<[f32]>>(&mut self, name: String, values: T) -> String {
        let mut data = Vec::with_capacity(values.as_ref().len() * 4);
        for value in values.as_ref() {
            data.extend_from_slice(&value.to_le_bytes());
        }
        if let Err(err) = std::fs::write(self.out_dir.join(&name), data) {
            self.errors.push(format!("write {name}: {err}"));
        }
        name
    }

    fn write_i32_blob<T: AsRef<[i32]>>(&mut self, name: String, values: T) -> String {
        let mut data = Vec::with_capacity(values.as_ref().len() * 4);
        for value in values.as_ref() {
            data.extend_from_slice(&value.to_le_bytes());
        }
        if let Err(err) = std::fs::write(self.out_dir.join(&name), data) {
            self.errors.push(format!("write {name}: {err}"));
        }
        name
    }

    fn write_u32_blob<T: AsRef<[u32]>>(&mut self, name: String, values: T) -> String {
        let mut data = Vec::with_capacity(values.as_ref().len() * 4);
        for value in values.as_ref() {
            data.extend_from_slice(&value.to_le_bytes());
        }
        if let Err(err) = std::fs::write(self.out_dir.join(&name), data) {
            self.errors.push(format!("write {name}: {err}"));
        }
        name
    }

    fn write_bytes_blob(&mut self, name: String, values: &[u8]) -> String {
        if let Err(err) = std::fs::write(self.out_dir.join(&name), values) {
            self.errors.push(format!("write {name}: {err}"));
        }
        name
    }

    fn write_json<T: Serialize>(&self, name: &str, value: &T) -> std::io::Result<()> {
        let mut file = std::fs::File::create(self.out_dir.join(name))?;
        serde_json::to_writer_pretty(&mut file, value).map_err(std::io::Error::other)?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

/// Full PCM→AT3 encoder for one stereo frame.
///
/// Mirrors the `at3enc_proc` call chain:
/// `bandsplit_at3` → `gaincontrol_at3` → `forward_transform_at3` →
/// `encode_mddata_at3` → `pack_mddata_at3`.
///
/// Maintains per-channel QMF histories, MDCT overlaps, gain schedules,
/// and encoder state across consecutive frames.
pub struct Atrac3Encoder {
    filter_banks: [crate::dsp::qmf::Atrac3AnalysisFilterBank; 2],
    forward_transforms: [crate::dsp::mdct::Atrac3ForwardTransform; 2],
    companion_forward_transforms: [crate::dsp::mdct::Atrac3ForwardTransform; 2],
    gain_current: [[GainInfo; 4]; 2],
    gain_next: [[GainInfo; 4]; 2],
    subband_prev: [[[f32; 256]; 4]; 2],
    subband_prev2: [[[f32; 256]; 4]; 2],
    enc_states: [EncoderChannelState; 2],
    tone_huff: HuffTableSet,
    spec_huff: HuffTableSet,
    frame_bytes: usize,
    channel_bytes: [usize; 2],
    channel_count: u16,
    joint_stereo: bool,
    enc_algo: EncAlgo,
    dba_frame_encoder: Option<crate::dsp::dba::DbaFrameEncoder>,
    diagnostics: EncodeFitterDiagnostics,
    production_trace: Option<ProductionTraceWriter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncAlgo {
    Dba,
    Clean,
}

impl EncAlgo {
    fn from_sony_params(bitrate_kbps: u32, channels: u16) -> Self {
        match (bitrate_kbps, channels) {
            (52, 1) | (66, 2) | (105, 2) => Self::Dba,
            (66, 1) | (132, 2) => Self::Clean,
            _ => Self::Clean,
        }
    }

    fn sony_value(self) -> i32 {
        match self {
            Self::Dba => 0,
            Self::Clean => 1,
        }
    }
}

fn sony_frame_bytes(bitrate_kbps: u32, channels: u16) -> usize {
    match (bitrate_kbps, channels) {
        (52, 1) => 304,
        (66, 1) => 384,
        (66, 2) => 192,
        (105, 2) => 304,
        (132, 2) => 384,
        _ => match bitrate_kbps {
            132 => 384,
            105 => 304,
            66 | 52 => 192,
            _ => 384,
        },
    }
}

fn uses_internal_stereo_frame(bitrate_kbps: u32, channels: u16) -> bool {
    matches!((bitrate_kbps, channels), (52, 1) | (66, 1))
}

fn mdct_trace_inputs(
    previous: &[[f32; 256]; 4],
    current: &[[f32; 256]; 4],
    subband_info: &crate::dsp::gain::SubbandInfo,
) -> [[f32; 512]; 4] {
    core::array::from_fn(|band| {
        let mut block = [0.0f32; 512];
        if !subband_info.is_band_empty(band) {
            let mut scales = [0.0f32; 512];
            let ok = crate::dsp::gain::GainProcessor::compute_scales(
                &subband_info.current[band],
                &subband_info.next[band],
                &mut scales,
            );
            if ok {
                crate::dsp::gain::GainProcessor::modulate(
                    &previous[band],
                    &current[band],
                    &scales,
                    &mut block,
                );
                return block;
            }
        }
        block[..256].copy_from_slice(&previous[band]);
        block[256..].copy_from_slice(&current[band]);
        block
    })
}

fn mdct_trace_inputs_no_gain(
    previous: &[[f32; 256]; 4],
    current: &[[f32; 256]; 4],
) -> [[f32; 512]; 4] {
    core::array::from_fn(|band| {
        let mut block = [0.0f32; 512];
        block[..256].copy_from_slice(&previous[band]);
        block[256..].copy_from_slice(&current[band]);
        block
    })
}

fn has_gain_side_info(current: &[GainInfo; 4], previous: &[GainInfo; 4]) -> bool {
    current
        .iter()
        .chain(previous.iter())
        .any(|gain| gain.count != 0)
}

fn dba_frame_config(bitrate_kbps: u32, channels: u16) -> Option<crate::dsp::dba::DbaFrameConfig> {
    match (bitrate_kbps, channels) {
        (52, 1) | (105, 2) => Some(crate::dsp::dba::DbaFrameConfig::sony_105_stereo()),
        (66, 2) => Some(crate::dsp::dba::DbaFrameConfig::sony_66_stereo()),
        _ => None,
    }
}

impl Atrac3Encoder {
    /// Creates a new encoder for the given bitrate (kbps) and joint-stereo mode.
    ///
    /// Initialises all per-channel DSP and encoder state with defaults.
    /// The bit budget per channel is derived from the ATRAC3 frame size for the
    /// given bitrate: budget = (bitrate * 1024 / sample_rate) * 8 bits / 2 channels.
    /// For 132 kbps at 44.1 kHz: 132000 * 1024 / 44100 = ~3066 bits/frame,
    /// divided by 2 channels ≈ 1533, rounded to 1536 by the binary's overheads.
    pub fn new(bitrate_kbps: u32, joint_stereo: bool) -> Self {
        let channels = if bitrate_kbps == 52 || (bitrate_kbps == 66 && !joint_stereo) {
            1
        } else {
            2
        };
        Self::new_with_channels(bitrate_kbps, channels)
    }

    pub fn new_with_channels(bitrate_kbps: u32, channels: u16) -> Self {
        let enc_algo = EncAlgo::from_sony_params(bitrate_kbps, channels);
        let frame_bytes = sony_frame_bytes(bitrate_kbps, channels);
        let joint_stereo = bitrate_kbps == 66 && channels == 2;
        let channel_bytes: [usize; 2] = if joint_stereo {
            [144, 48]
        } else if enc_algo == EncAlgo::Dba || uses_internal_stereo_frame(bitrate_kbps, channels) {
            [frame_bytes / 2, frame_bytes / 2]
        } else if channels == 1 {
            [frame_bytes, 0]
        } else {
            [frame_bytes / 2, frame_bytes / 2]
        };
        let dba_frame_encoder =
            dba_frame_config(bitrate_kbps, channels).map(crate::dsp::dba::DbaFrameEncoder::new);
        let channel_bfu_counts: [i32; 2] = [29, 29];
        let tone_huff = HuffTableSet::build_tone();
        let spec_huff = HuffTableSet::build_spec();
        Self {
            filter_banks: [
                crate::dsp::qmf::Atrac3AnalysisFilterBank::new(),
                crate::dsp::qmf::Atrac3AnalysisFilterBank::new(),
            ],
            forward_transforms: [
                crate::dsp::mdct::Atrac3ForwardTransform::new(),
                crate::dsp::mdct::Atrac3ForwardTransform::new(),
            ],
            companion_forward_transforms: [
                crate::dsp::mdct::Atrac3ForwardTransform::new(),
                crate::dsp::mdct::Atrac3ForwardTransform::new(),
            ],
            gain_current: Default::default(),
            gain_next: Default::default(),
            subband_prev: [[[0.0f32; 256]; 4]; 2],
            subband_prev2: [[[0.0f32; 256]; 4]; 2],
            enc_states: [
                EncoderChannelState {
                    total_bit_budget: (channel_bytes[0] * 8) as f32,
                    table_idx: 0,
                    joint_stereo: 0,
                    spectral_bfu_count: channel_bfu_counts[0],
                    ..Default::default()
                },
                EncoderChannelState {
                    total_bit_budget: (channel_bytes[1] * 8) as f32,
                    table_idx: 0,
                    joint_stereo: if joint_stereo { 1 } else { 0 },
                    spectral_bfu_count: channel_bfu_counts[1],
                    ..Default::default()
                },
            ],
            tone_huff,
            spec_huff,
            frame_bytes,
            channel_bytes,
            channel_count: channels,
            joint_stereo,
            enc_algo,
            dba_frame_encoder,
            diagnostics: EncodeFitterDiagnostics::default(),
            production_trace: None,
        }
    }

    pub fn enc_algo(&self) -> i32 {
        self.enc_algo.sony_value()
    }

    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    pub fn diagnostics(&self) -> EncodeFitterDiagnostics {
        self.diagnostics
    }

    pub fn reset_diagnostics(&mut self) {
        self.diagnostics = EncodeFitterDiagnostics::default();
        for state in &mut self.enc_states {
            state.diagnostics = EncodeFitterDiagnostics::default();
        }
    }

    pub fn enable_production_trace<P: AsRef<Path>>(&mut self, out_dir: P) -> std::io::Result<()> {
        self.enable_production_trace_with_max_frames(out_dir, None)
    }

    pub fn enable_production_trace_with_max_frames<P: AsRef<Path>>(
        &mut self,
        out_dir: P,
        max_sound_frames: Option<u32>,
    ) -> std::io::Result<()> {
        self.production_trace = Some(ProductionTraceWriter::new(
            out_dir.as_ref().to_path_buf(),
            self.frame_bytes,
            self.channel_count,
            max_sound_frames,
        )?);
        Ok(())
    }

    pub fn begin_production_trace_frame(
        &mut self,
        context: ProductionTraceFrameContext,
    ) -> std::io::Result<()> {
        if let Some(trace) = &mut self.production_trace {
            trace.begin_sound_frame(context, None)?;
        }
        Ok(())
    }

    pub fn begin_production_trace_frame_with_pcm(
        &mut self,
        context: ProductionTraceFrameContext,
        input_pcm: &[u8],
    ) -> std::io::Result<()> {
        if let Some(trace) = &mut self.production_trace {
            trace.begin_sound_frame(context, Some(input_pcm))?;
        }
        Ok(())
    }

    pub fn finish_production_trace(&mut self) -> std::io::Result<()> {
        if let Some(trace) = &mut self.production_trace {
            trace.finish()?;
        }
        Ok(())
    }

    fn begin_trace_at3enc_proc(&mut self) -> Option<u32> {
        self.production_trace
            .as_mut()
            .and_then(ProductionTraceWriter::begin_at3enc_proc)
    }

    fn finish_trace_at3enc_proc(
        &mut self,
        call_idx: Option<u32>,
        return_value: i32,
        output: Option<&[u8]>,
    ) -> Option<String> {
        if let (Some(trace), Some(call_idx)) = (&mut self.production_trace, call_idx) {
            trace.finish_at3enc_proc(call_idx, return_value, output)
        } else {
            None
        }
    }

    fn production_trace_recording(&self) -> bool {
        self.production_trace
            .as_ref()
            .is_some_and(ProductionTraceWriter::is_recording)
    }

    fn companion_spectra(
        &mut self,
        channel: usize,
        delayed_bands: &[[f32; 256]; 4],
        primary_spectra: &[[f32; 256]; 4],
        has_gain_side_info: bool,
        trace_recording: bool,
    ) -> [[f32; 256]; 4] {
        if !has_gain_side_info {
            self.companion_forward_transforms[channel].set_overlap_from_bands(delayed_bands);
            return *primary_spectra;
        }

        let overlap = self.companion_forward_transforms[channel].overlap_snapshot();
        let mdct_inputs = mdct_trace_inputs_no_gain(&overlap, delayed_bands);
        let mut s0 = [0.0f32; 256];
        let mut s1 = [0.0f32; 256];
        let mut s2 = [0.0f32; 256];
        let mut s3 = [0.0f32; 256];
        {
            let bands_arr: [&[f32; 256]; 4] = [
                &delayed_bands[0],
                &delayed_bands[1],
                &delayed_bands[2],
                &delayed_bands[3],
            ];
            let mut sp: [&mut [f32; 256]; 4] = [&mut s0, &mut s1, &mut s2, &mut s3];
            self.companion_forward_transforms[channel]
                .transform_with_gain(&bands_arr, &mut sp, None);
        }
        let spectra = [s0, s1, s2, s3];
        if trace_recording && let Some(trace) = &mut self.production_trace {
            trace.record_mdct(channel, &mdct_inputs, &spectra);
        }
        spectra
    }

    /// Encodes one stereo PCM sound unit (1024 samples per channel) into AT3 bytes.
    ///
    /// Returns the packed bitstream for this frame, or an empty vector on error.
    /// The output buffer is sized to hold a full AT3 frame (~4096 bytes max for
    /// 132 kbps stereo).
    ///
    /// Pipeline per channel:
    /// 1. QMF analysis: 1024 PCM → 4×256 subbands
    /// 2. Gain control: detect transients, produce gain schedules for next frame
    /// 3. MDCT with gain modulation: 4×256 subbands → 4×256 MDCT spectra
    /// 4. Encode: `encode_mddata_at3` (scale/quant/bit allocation)
    /// 5. Pack: `pack_mddata_at3` (Huffman-coded bitstream)
    pub fn encode_frame(&mut self, pcm: &[&[f32; 1024]; 2], out_buffer: &mut [u8]) -> i32 {
        let trace_call_idx = self.begin_trace_at3enc_proc();
        if out_buffer.len() < self.frame_bytes {
            self.finish_trace_at3enc_proc(trace_call_idx, -1, None);
            return -1;
        }
        out_buffer.fill(0);

        if self.enc_algo == EncAlgo::Dba {
            let trace_recording = self.production_trace_recording();
            let mut dba_frame_trace = crate::dsp::dba::DbaProductionFrameTrace::default();
            let Some(encoder) = self.dba_frame_encoder.as_mut() else {
                self.finish_trace_at3enc_proc(trace_call_idx, -1, None);
                return -1;
            };
            let encode_result = if trace_recording {
                encoder.encode_frame_with_trace(pcm, out_buffer, &mut dba_frame_trace)
            } else {
                encoder.encode_frame(pcm, out_buffer)
            };
            let result = match encode_result {
                Ok(()) => (self.frame_bytes * 8) as i32,
                Err(code) => code,
            };
            if trace_recording
                && let (Some(trace), Some(call_idx)) = (&mut self.production_trace, trace_call_idx)
            {
                trace.record_dba_frame_trace(call_idx, &dba_frame_trace);
            }
            let output = if result >= 0 {
                Some(&out_buffer[..self.frame_bytes])
            } else {
                None
            };
            let output_name = self.finish_trace_at3enc_proc(trace_call_idx, result, output);
            if trace_recording
                && let (Some(trace), Some(call_idx)) = (&mut self.production_trace, trace_call_idx)
            {
                trace.record_dba_pack_frame_output(call_idx, output_name, result);
            }
            return result;
        }
        let trace_recording = self.production_trace_recording();

        // --- QMF analysis for both channels ---
        let bands_ch0_raw: [[f32; 256]; 4] = {
            let mut b0 = [0.0f32; 256];
            let mut b1 = [0.0f32; 256];
            let mut b2 = [0.0f32; 256];
            let mut b3 = [0.0f32; 256];
            let mut qmf_traces = Vec::new();
            {
                let mut bm: [&mut [f32]; 4] = [&mut b0, &mut b1, &mut b2, &mut b3];
                if trace_recording {
                    self.filter_banks[0].analysis_with_trace(pcm[0], &mut bm, &mut qmf_traces);
                } else {
                    self.filter_banks[0].analysis(pcm[0], &mut bm);
                }
            }
            let bands = [b0, b1, b2, b3];
            if trace_recording && let Some(trace) = &mut self.production_trace {
                trace.record_bandsplit(0, pcm[0], &bands, &qmf_traces);
            }
            bands
        };
        let bands_ch1_raw: [[f32; 256]; 4] = {
            let mut b0 = [0.0f32; 256];
            let mut b1 = [0.0f32; 256];
            let mut b2 = [0.0f32; 256];
            let mut b3 = [0.0f32; 256];
            let mut qmf_traces = Vec::new();
            {
                let mut bm: [&mut [f32]; 4] = [&mut b0, &mut b1, &mut b2, &mut b3];
                if trace_recording {
                    self.filter_banks[1].analysis_with_trace(pcm[1], &mut bm, &mut qmf_traces);
                } else {
                    self.filter_banks[1].analysis(pcm[1], &mut bm);
                }
            }
            let bands = [b0, b1, b2, b3];
            if trace_recording && let Some(trace) = &mut self.production_trace {
                trace.record_bandsplit(1, pcm[1], &bands, &qmf_traces);
            }
            bands
        };

        // --- Joint-stereo M/S matrixing on QMF subbands ---
        let (bands_ch0_raw, bands_ch1_raw): ([[f32; 256]; 4], [[f32; 256]; 4]) =
            if self.joint_stereo {
                let mut m = bands_ch0_raw;
                let s = bands_ch1_raw;
                let mut s_out = s;
                for band in 0..4 {
                    for i in 0..256 {
                        let l = m[band][i];
                        let r = s[band][i];
                        m[band][i] = (l + r) * 0.5;
                        s_out[band][i] = (l - r) * 0.5;
                    }
                }
                (m, s_out)
            } else {
                (bands_ch0_raw, bands_ch1_raw)
            };

        let bands_ch0_b: [[f32; 256]; 4];
        let bands_ch1_b: [[f32; 256]; 4];
        let delayed_bands_ch0: [[f32; 256]; 4];
        let delayed_bands_ch1: [[f32; 256]; 4];

        // --- Channel 0: gain → MDCT ---
        {
            let current_bands = bands_ch0_raw;
            let delayed_bands = self.subband_prev[0];
            let delayed2_bands = self.subband_prev2[0];

            let mut gain_bufs: [Vec<f32>; 4] = [
                vec![0.0f32; 768],
                vec![0.0f32; 768],
                vec![0.0f32; 768],
                vec![0.0f32; 768],
            ];
            gain_bufs[0][0..256].copy_from_slice(&delayed2_bands[0]);
            gain_bufs[0][256..512].copy_from_slice(&delayed_bands[0]);
            gain_bufs[0][512..768].copy_from_slice(&current_bands[0]);
            gain_bufs[1][0..256].copy_from_slice(&delayed2_bands[1]);
            gain_bufs[1][256..512].copy_from_slice(&delayed_bands[1]);
            gain_bufs[1][512..768].copy_from_slice(&current_bands[1]);
            gain_bufs[2][0..256].copy_from_slice(&delayed2_bands[2]);
            gain_bufs[2][256..512].copy_from_slice(&delayed_bands[2]);
            gain_bufs[2][512..768].copy_from_slice(&current_bands[2]);
            gain_bufs[3][0..256].copy_from_slice(&delayed2_bands[3]);
            gain_bufs[3][256..512].copy_from_slice(&delayed_bands[3]);
            gain_bufs[3][512..768].copy_from_slice(&current_bands[3]);
            let gain_refs: [&[f32]; 4] =
                [&gain_bufs[0], &gain_bufs[1], &gain_bufs[2], &gain_bufs[3]];
            let next_before = self.gain_next[0].clone();
            let gain_ok = crate::dsp::gain::GainProcessor::gaincontrol_at3(
                gain_refs,
                &self.gain_current[0],
                &mut self.gain_next[0],
            );
            if trace_recording && let Some(trace) = &mut self.production_trace {
                trace.record_gaincontrol(
                    0,
                    gain_refs,
                    &self.gain_current[0],
                    &next_before,
                    &self.gain_next[0],
                    if gain_ok { 0 } else { -1 },
                );
            }

            let subband_info = crate::dsp::gain::SubbandInfo {
                current: self.gain_current[0].clone(),
                next: self.gain_next[0].clone(),
            };
            let mdct_inputs = mdct_trace_inputs(&delayed2_bands, &delayed_bands, &subband_info);
            let mut s0 = [0.0f32; 256];
            let mut s1 = [0.0f32; 256];
            let mut s2 = [0.0f32; 256];
            let mut s3 = [0.0f32; 256];
            {
                let bands_arr: [&[f32; 256]; 4] = [
                    &delayed_bands[0],
                    &delayed_bands[1],
                    &delayed_bands[2],
                    &delayed_bands[3],
                ];
                let mut sp: [&mut [f32; 256]; 4] = [&mut s0, &mut s1, &mut s2, &mut s3];
                self.forward_transforms[0].transform_with_gain(
                    &bands_arr,
                    &mut sp,
                    Some(&subband_info),
                );
            }
            let spectra = [s0, s1, s2, s3];
            if trace_recording && let Some(trace) = &mut self.production_trace {
                trace.record_mdct(0, &mdct_inputs, &spectra);
            }
            self.subband_prev2[0] = delayed_bands;
            self.subband_prev[0] = current_bands;
            delayed_bands_ch0 = delayed_bands;
            bands_ch0_b = spectra;
        }

        // --- Channel 1: gain → MDCT ---
        {
            let current_bands = bands_ch1_raw;
            let delayed_bands = self.subband_prev[1];
            let delayed2_bands = self.subband_prev2[1];

            let mut gain_bufs: [Vec<f32>; 4] = [
                vec![0.0f32; 768],
                vec![0.0f32; 768],
                vec![0.0f32; 768],
                vec![0.0f32; 768],
            ];
            gain_bufs[0][0..256].copy_from_slice(&delayed2_bands[0]);
            gain_bufs[0][256..512].copy_from_slice(&delayed_bands[0]);
            gain_bufs[0][512..768].copy_from_slice(&current_bands[0]);
            gain_bufs[1][0..256].copy_from_slice(&delayed2_bands[1]);
            gain_bufs[1][256..512].copy_from_slice(&delayed_bands[1]);
            gain_bufs[1][512..768].copy_from_slice(&current_bands[1]);
            gain_bufs[2][0..256].copy_from_slice(&delayed2_bands[2]);
            gain_bufs[2][256..512].copy_from_slice(&delayed_bands[2]);
            gain_bufs[2][512..768].copy_from_slice(&current_bands[2]);
            gain_bufs[3][0..256].copy_from_slice(&delayed2_bands[3]);
            gain_bufs[3][256..512].copy_from_slice(&delayed_bands[3]);
            gain_bufs[3][512..768].copy_from_slice(&current_bands[3]);
            let gain_refs: [&[f32]; 4] =
                [&gain_bufs[0], &gain_bufs[1], &gain_bufs[2], &gain_bufs[3]];
            let next_before = self.gain_next[1].clone();
            let gain_ok = crate::dsp::gain::GainProcessor::gaincontrol_at3(
                gain_refs,
                &self.gain_current[1],
                &mut self.gain_next[1],
            );
            if trace_recording && let Some(trace) = &mut self.production_trace {
                trace.record_gaincontrol(
                    1,
                    gain_refs,
                    &self.gain_current[1],
                    &next_before,
                    &self.gain_next[1],
                    if gain_ok { 0 } else { -1 },
                );
            }

            let subband_info = crate::dsp::gain::SubbandInfo {
                current: self.gain_current[1].clone(),
                next: self.gain_next[1].clone(),
            };
            let mdct_inputs = mdct_trace_inputs(&delayed2_bands, &delayed_bands, &subband_info);
            let mut s0 = [0.0f32; 256];
            let mut s1 = [0.0f32; 256];
            let mut s2 = [0.0f32; 256];
            let mut s3 = [0.0f32; 256];
            {
                let bands_arr: [&[f32; 256]; 4] = [
                    &delayed_bands[0],
                    &delayed_bands[1],
                    &delayed_bands[2],
                    &delayed_bands[3],
                ];
                let mut sp: [&mut [f32; 256]; 4] = [&mut s0, &mut s1, &mut s2, &mut s3];
                self.forward_transforms[1].transform_with_gain(
                    &bands_arr,
                    &mut sp,
                    Some(&subband_info),
                );
            }
            let spectra = [s0, s1, s2, s3];
            if trace_recording && let Some(trace) = &mut self.production_trace {
                trace.record_mdct(1, &mdct_inputs, &spectra);
            }
            self.subband_prev2[1] = delayed_bands;
            self.subband_prev[1] = current_bands;
            delayed_bands_ch1 = delayed_bands;
            bands_ch1_b = spectra;
        }

        let bands_ch0_a = self.companion_spectra(
            0,
            &delayed_bands_ch0,
            &bands_ch0_b,
            has_gain_side_info(&self.gain_next[0], &self.gain_current[0]),
            trace_recording,
        );
        let bands_ch1_a = self.companion_spectra(
            1,
            &delayed_bands_ch1,
            &bands_ch1_b,
            has_gain_side_info(&self.gain_next[1], &self.gain_current[1]),
            trace_recording,
        );

        // Flatten spectra: 4 bands × 256 = 1024 per channel
        let mut specs_a_ch0 = [0.0f32; 1024];
        let mut specs_b_ch0 = [0.0f32; 1024];
        let mut specs_a_ch1 = [0.0f32; 1024];
        let mut specs_b_ch1 = [0.0f32; 1024];
        for b in 0..4 {
            let off = b * 256;
            specs_a_ch0[off..off + 256].copy_from_slice(&bands_ch0_a[b]);
            specs_b_ch0[off..off + 256].copy_from_slice(&bands_ch0_b[b]);
            specs_a_ch1[off..off + 256].copy_from_slice(&bands_ch1_a[b]);
            specs_b_ch1[off..off + 256].copy_from_slice(&bands_ch1_b[b]);
        }

        // --- Encode + pack per channel ---
        self.enc_states[0].previous_gain_control = self.gain_current[0].clone();
        self.enc_states[1].previous_gain_control = self.gain_current[1].clone();
        self.enc_states[0].gain_control = self.gain_next[0].clone();
        self.enc_states[1].gain_control = self.gain_next[1].clone();
        let saved_states = self.enc_states.clone();
        let mut buf_offset = 0i32;
        let ch0_diag_before = self.enc_states[0].diagnostics;
        let ch0_encode_trace = if trace_recording {
            self.production_trace.as_mut().map(|trace| {
                trace.begin_encode_mddata(0, &specs_a_ch0, &specs_b_ch0, &self.enc_states[0])
            })
        } else {
            None
        };
        let ch0_outcome = encode_channel_inner(
            &specs_a_ch0,
            &specs_b_ch0,
            &mut self.enc_states[0],
            &self.tone_huff,
            &self.spec_huff,
            out_buffer,
            &mut buf_offset,
            0,
        );
        if let (Some(trace), Some(call_idx)) = (&mut self.production_trace, ch0_encode_trace) {
            trace.finish_encode_mddata(call_idx, &ch0_outcome);
        }
        let n_bits_ch0 = ch0_outcome.pack_bits;
        let ch0_diag_delta = self.enc_states[0].diagnostics.delta_from(ch0_diag_before);
        self.diagnostics.add_assign(ch0_diag_delta);
        if n_bits_ch0 < 0 || ((n_bits_ch0 as usize + 7) >> 3) > self.channel_bytes[0] {
            self.diagnostics.channel_encode_reject_events = self
                .diagnostics
                .channel_encode_reject_events
                .saturating_add(1);
            self.enc_states = saved_states;
            self.advance_gain_state();
            self.finish_trace_at3enc_proc(trace_call_idx, -1, None);
            return -1;
        }

        buf_offset = (self.channel_bytes[0] * 8) as i32;
        let ch1_diag_before = self.enc_states[1].diagnostics;
        let ch1_encode_trace = if trace_recording {
            self.production_trace.as_mut().map(|trace| {
                trace.begin_encode_mddata(1, &specs_a_ch1, &specs_b_ch1, &self.enc_states[1])
            })
        } else {
            None
        };
        let ch1_outcome = encode_channel_inner(
            &specs_a_ch1,
            &specs_b_ch1,
            &mut self.enc_states[1],
            &self.tone_huff,
            &self.spec_huff,
            out_buffer,
            &mut buf_offset,
            1,
        );
        if let (Some(trace), Some(call_idx)) = (&mut self.production_trace, ch1_encode_trace) {
            trace.finish_encode_mddata(call_idx, &ch1_outcome);
        }
        let n_bits_ch1 = ch1_outcome.pack_bits;
        let ch1_diag_delta = self.enc_states[1].diagnostics.delta_from(ch1_diag_before);
        self.diagnostics.add_assign(ch1_diag_delta);
        if n_bits_ch1 < 0 || ((n_bits_ch1 as usize + 7) >> 3) > self.channel_bytes[1] {
            self.diagnostics.channel_encode_reject_events = self
                .diagnostics
                .channel_encode_reject_events
                .saturating_add(1);
            self.enc_states = saved_states;
            self.advance_gain_state();
            self.finish_trace_at3enc_proc(trace_call_idx, -1, None);
            return -1;
        }

        // For joint-stereo, reverse ch1 bytes in the output buffer.
        // The Sony decoder expects ch1 (S channel) bytes in reversed order.
        if self.joint_stereo {
            let start = self.channel_bytes[0];
            let end = start + self.channel_bytes[1];
            if end <= out_buffer.len() {
                out_buffer[start..end].reverse();
            }
        }

        self.advance_gain_state();

        let result = (self.frame_bytes * 8) as i32;
        self.finish_trace_at3enc_proc(
            trace_call_idx,
            result,
            Some(&out_buffer[..self.frame_bytes]),
        );
        result
    }

    fn advance_gain_state(&mut self) {
        self.gain_current[0] = self.gain_next[0].clone();
        self.gain_next[0] = Default::default();
        self.gain_current[1] = self.gain_next[1].clone();
        self.gain_next[1] = Default::default();
    }
}
