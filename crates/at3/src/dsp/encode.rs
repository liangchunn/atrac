//! `encode_mddata_at3` orchestrator (milestone 6d + 8).
//!
//! Ports the control flow of `encode_mddata_at3` (`0x65c98`) from
//! `libatrac.so.1.2.0` using the already-validated leaf functions
//! from `dsp::tone` and `dsp::quant`.
//!
//! Half 1 (deterministic pipeline): implemented and validated.
//! Half 2 (iterative bit-allocation convergence loop): in progress (Phase 3–4).

use crate::config::{Atrac3Profile, EncoderStrategy};
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
) -> i32 {
    let result = encode_mddata_at3(specs_a, specs_b, state, huff_tables, spec_huff);
    if result.remaining_budget == -0x8000 {
        return -1;
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
        return -1;
    }
    *buf_offset += pack_result; // accumulate total bits across calls
    pack_result
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
    joint_stereo: bool,
    enc_algo: EncAlgo,
    dba_frame_encoder: Option<crate::dsp::dba::DbaFrameEncoder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncAlgo {
    Dba,
    Clean,
}

impl EncAlgo {
    fn from_profile(profile: Atrac3Profile) -> Self {
        match profile.strategy() {
            EncoderStrategy::Dba => Self::Dba,
            EncoderStrategy::Clean => Self::Clean,
        }
    }
}

fn has_gain_side_info(current: &[GainInfo; 4], previous: &[GainInfo; 4]) -> bool {
    current
        .iter()
        .chain(previous.iter())
        .any(|gain| gain.count != 0)
}

fn dba_frame_config(profile: Atrac3Profile) -> Option<crate::dsp::dba::DbaFrameConfig> {
    match (profile.bitrate_kbps(), profile.channels()) {
        (52, 1) | (105, 2) => Some(crate::dsp::dba::DbaFrameConfig::sony_105_stereo()),
        (66, 2) => Some(crate::dsp::dba::DbaFrameConfig::sony_66_stereo()),
        _ => None,
    }
}

impl Atrac3Encoder {
    /// Creates a new frame encoder for a validated ATRAC3 profile.
    ///
    /// Initialises all per-channel DSP and encoder state with defaults.
    /// The bit budget per channel is derived from the ATRAC3 frame size for the
    /// given bitrate: budget = (bitrate * 1024 / sample_rate) * 8 bits / 2 channels.
    /// For 132 kbps at 44.1 kHz: 132000 * 1024 / 44100 = ~3066 bits/frame,
    /// divided by 2 channels ≈ 1533, rounded to 1536 by the binary's overheads.
    pub fn new(profile: Atrac3Profile) -> Self {
        let enc_algo = EncAlgo::from_profile(profile);
        let frame_bytes = profile.internal_frame_bytes();
        debug_assert_eq!(
            frame_bytes,
            match profile.encoder_bitrate_kbps() {
                66 => 192,
                105 => 304,
                132 => 384,
                _ => unreachable!("validated ATRAC3 internal bitrate"),
            }
        );
        let joint_stereo = profile.is_joint_stereo();
        let channel_bytes: [usize; 2] = if joint_stereo {
            [144, 48]
        } else {
            [frame_bytes / 2, frame_bytes / 2]
        };
        let dba_frame_encoder =
            dba_frame_config(profile).map(crate::dsp::dba::DbaFrameEncoder::new);
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
            joint_stereo,
            enc_algo,
            dba_frame_encoder,
        }
    }

    fn companion_spectra(
        &mut self,
        channel: usize,
        delayed_bands: &[[f32; 256]; 4],
        primary_spectra: &[[f32; 256]; 4],
        has_gain_side_info: bool,
    ) -> [[f32; 256]; 4] {
        if !has_gain_side_info {
            self.companion_forward_transforms[channel].set_overlap_from_bands(delayed_bands);
            return *primary_spectra;
        }

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
        [s0, s1, s2, s3]
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
        if out_buffer.len() < self.frame_bytes {
            return -1;
        }
        out_buffer.fill(0);

        if self.enc_algo == EncAlgo::Dba {
            let Some(encoder) = self.dba_frame_encoder.as_mut() else {
                return -1;
            };
            return match encoder.encode_frame(pcm, out_buffer) {
                Ok(()) => (self.frame_bytes * 8) as i32,
                Err(code) => code,
            };
        }

        // --- QMF analysis for both channels ---
        let bands_ch0_raw: [[f32; 256]; 4] = {
            let mut b0 = [0.0f32; 256];
            let mut b1 = [0.0f32; 256];
            let mut b2 = [0.0f32; 256];
            let mut b3 = [0.0f32; 256];
            {
                let mut bm: [&mut [f32]; 4] = [&mut b0, &mut b1, &mut b2, &mut b3];
                self.filter_banks[0].analysis(pcm[0], &mut bm);
            }
            [b0, b1, b2, b3]
        };
        let bands_ch1_raw: [[f32; 256]; 4] = {
            let mut b0 = [0.0f32; 256];
            let mut b1 = [0.0f32; 256];
            let mut b2 = [0.0f32; 256];
            let mut b3 = [0.0f32; 256];
            {
                let mut bm: [&mut [f32]; 4] = [&mut b0, &mut b1, &mut b2, &mut b3];
                self.filter_banks[1].analysis(pcm[1], &mut bm);
            }
            [b0, b1, b2, b3]
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
            crate::dsp::gain::GainProcessor::gaincontrol_at3(
                gain_refs,
                &self.gain_current[0],
                &mut self.gain_next[0],
            );

            let subband_info = crate::dsp::gain::SubbandInfo {
                current: self.gain_current[0].clone(),
                next: self.gain_next[0].clone(),
            };
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
            crate::dsp::gain::GainProcessor::gaincontrol_at3(
                gain_refs,
                &self.gain_current[1],
                &mut self.gain_next[1],
            );

            let subband_info = crate::dsp::gain::SubbandInfo {
                current: self.gain_current[1].clone(),
                next: self.gain_next[1].clone(),
            };
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
        );
        let bands_ch1_a = self.companion_spectra(
            1,
            &delayed_bands_ch1,
            &bands_ch1_b,
            has_gain_side_info(&self.gain_next[1], &self.gain_current[1]),
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
        let n_bits_ch0 = encode_channel_inner(
            &specs_a_ch0,
            &specs_b_ch0,
            &mut self.enc_states[0],
            &self.tone_huff,
            &self.spec_huff,
            out_buffer,
            &mut buf_offset,
            0,
        );
        if n_bits_ch0 < 0 || ((n_bits_ch0 as usize + 7) >> 3) > self.channel_bytes[0] {
            self.enc_states = saved_states;
            self.advance_gain_state();
            return -1;
        }

        buf_offset = (self.channel_bytes[0] * 8) as i32;
        let n_bits_ch1 = encode_channel_inner(
            &specs_a_ch1,
            &specs_b_ch1,
            &mut self.enc_states[1],
            &self.tone_huff,
            &self.spec_huff,
            out_buffer,
            &mut buf_offset,
            1,
        );
        if n_bits_ch1 < 0 || ((n_bits_ch1 as usize + 7) >> 3) > self.channel_bytes[1] {
            self.enc_states = saved_states;
            self.advance_gain_state();
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

        (self.frame_bytes * 8) as i32
    }

    fn advance_gain_state(&mut self) {
        self.gain_current[0] = self.gain_next[0].clone();
        self.gain_next[0] = Default::default();
        self.gain_current[1] = self.gain_next[1].clone();
        self.gain_next[1] = Default::default();
    }
}
