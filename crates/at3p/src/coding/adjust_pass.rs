//! Scoped composition of `adjust_scalefactors_at5` (native 0x45ae0)
//! and its `pwc_qu_at5` leaf (native 0x2a300) for the first
//! 352 kbps.
//!
//! Native source of truth is the decompiled boundaries at Ghidra
//! `0x00055ae0` / `0x0003a300`, direct disassembly at native
//! `0x00045ae0..0x00046503` and `0x0002a300..0x0002a51a` (including
//! the call-site ABI at `0x00054539..0x0005455b`: eax = block table,
//! edx = spectrum table, ecx = object table; stack = channel count,
//! band count, encode selector), and `adjust_io_trace.ndjson`.
//!
//! For bands from `sa_adjscl_startqu[selector]` upward the pass
//! rebuilds each channel's dequantization buffer from the `+0x1b6f8`
//! quantized words (joint-stereo bands with side `+0x94 == 1` reuse
//! channel 0's cached buffer), adds `pwc_qu_at5` noise seeded by the
//! `+0x1b678` idsf-sum phase ladder, compares reconstructed versus
//! original spectral energy, and rewrites the `+0x1b678` idsf row:
//! upward `x1.587401` steps clamped to `original + 5/10` with an
//! overshoot and aux-weight/flatness decrement, or downward
//! `x0.62996054` steps with floor 1 and a `x0.70710677/x0.7937005`
//! increment. The epilogue recounts the IDSF side data
//! (`calc_nbits_for_idsf_ch_at5` when side `+0x8c != 0`, else 6-bit
//! raw modes with `+0x1c73c/+0x1c74c/+0x1c750` zeroed) and rebases
//! side `+0x11c/+0x12a/+0x12e`.
//!
//! Float model: the x87 energy accumulations, magnitude sum, and
//! quotients are modeled in f64 with single f32 rounds at the native
//! `fstps` points. Products of two f32 values are exact in f64, so
//! every threshold comparison matches native exactly; the energy sums
//! and quotients double-round (f64 versus x87 extended), which can
//! only diverge when the exact value sits within ~2^-40 of an f32
//! boundary. The live replay covers both live calls bit-exactly.

use crate::coding::bitcount::{
    BitcountError, IdsfBlockState, IdsfChannelState, calc_nbits_for_idsf_ch_at5,
};
use crate::tables::at5::{
    ADJSCL_STARTQU_AT5_ENTRIES, IDSPCBANDS_AT5_ENTRIES, adjscl_startqu_at5, idspcbands_at5,
    ifqf_at5, isps_at5, lngain_at5, nsps_at5, rndtbl_at5, sftbl_at5, spclev_at5, x_at5,
};

const ADJUST_BANDS_AT5: usize = 32;
const ADJUST_GROUPS_AT5: usize = 16;
const ADJUST_WINDOW_AT5: usize = 128;
const ADJUST_GAIN_LEVELS_AT5: usize = 8;
const ADJUST_QUANT_PLANE_AT5: usize = 2048;
const ADJUST_IDSF_MAX_AT5: i32 = 0x3e;

#[derive(Debug)]
pub enum AdjustPassError {
    Bitcount(BitcountError),
    OutOfScope(&'static str),
    RowTooShort { needed: usize, actual: usize },
}

impl From<BitcountError> for AdjustPassError {
    fn from(error: BitcountError) -> Self {
        AdjustPassError::Bitcount(error)
    }
}

/// One per-group gain-control row surface (`obj + 0x8` / `obj + 0xc`,
/// `0x98`-byte stride): the point count word and the level ids at
/// word offset 8, consumed by the `pwc_qu_at5` shift search.
#[derive(Debug, Clone)]
pub struct AdjustGainRow {
    pub count: i32,
    pub level_ids: Vec<i32>,
}

/// One channel's adjust-pass surface.
#[derive(Debug)]
pub struct AdjustChannelState {
    /// Object mode word (`obj + 0`), also the IDSF leaf mode.
    pub obj_mode: u32,
    /// `+0x1b5f8` word-length row (read-only).
    pub word_lengths: Vec<i32>,
    /// `+0x1b678` scale-factor row (rewritten in place).
    pub scale_factors: Vec<i32>,
    /// `+0x1b6f8` quantized spectrum plane (2048 i16 words).
    pub quantized: Vec<i16>,
    /// Original spectrum windows per band (from the spectrum table).
    pub spectra: Vec<Vec<f32>>,
    /// Block `+0x3cc` aux weight row.
    pub aux_weights: Vec<f32>,
    /// Config `+0xa8` and the `+0x50` per-group flags (the pwc
    /// channel-swap gate).
    pub config_a8: u32,
    pub group_flags: Vec<u32>,
    /// `obj + 0x1c6f8` per-slot spectral level words.
    pub spc_level_words: Vec<i32>,
    /// Gain rows at `obj + 0xc` (current) and `obj + 0x8` (previous).
    pub cur_gain_rows: Vec<AdjustGainRow>,
    pub prev_gain_rows: Vec<AdjustGainRow>,
    /// Config counts: `+0xb0` band count, `+0xc0` phase-seed count,
    /// `+0xb8` IDSF leaf group count.
    pub band_count_b0: usize,
    pub group_count_c0: usize,
    pub leaf_group_count_b8: usize,
}

/// Frame-level adjust-pass inputs.
#[derive(Debug)]
pub struct AdjustFrameState {
    pub channels: Vec<AdjustChannelState>,
    /// `param_5`: bands walked from the selector start upward.
    pub band_count: usize,
    /// `param_6`: the encode selector.
    pub selector: usize,
    /// Side `+0x94` per-band mode row (1 selects the joint path).
    pub side_band_modes: Vec<i16>,
    /// Side `+0x8c` recount gate.
    pub side_gate_8c: u32,
    /// Entry side shorts `+0x11c`, `+0x12a`, `+0x12e`.
    pub idsf_bits_11c: i16,
    pub base_total_12a: i16,
    pub current_bits_12e: i16,
}

#[derive(Debug)]
pub struct AdjustChannelOutput {
    pub scale_factors: Vec<i32>,
    /// The IDSF leaf return for this channel (side `+0x8c != 0`).
    pub leaf_bits: Option<i32>,
    /// The final IDSF block state the epilogue leaf wrote for this channel
    /// (`+0x8c != 0` path). `None` on the `+0x8c == 0` zero arm, where the
    /// native writes only `obj[0x1c73c]=0`/`0x1c74c=0`/`0x1c750=0` and does
    /// not invoke the leaf. Consumed by
    /// `crate::encoder::packer_bridge::serialize_idsf_object_range_b`.
    pub idsf_block: Option<IdsfBlockState>,
}

#[derive(Debug)]
pub struct AdjustFrameOutput {
    pub channels: Vec<AdjustChannelOutput>,
    pub idsf_bits_11c: i16,
    pub base_total_12a: i16,
    pub extended_total_12e: i16,
    /// The recount total left in eax by the native epilogue.
    pub recount_bits: i32,
    /// Set when the `+0x8c == 0` branch zeroed the block-state words.
    pub block_state_zeroed: bool,
    pub pwc_calls: usize,
}

/// Persistent `pwc_qu_at5` scratch across one adjust call: the last
/// refreshed group and the dither cache (`local_23c`). A later band
/// in the same group reuses the cache verbatim, including any stale
/// tail beyond the refreshing band's window.
#[derive(Debug)]
struct PwcState {
    last_group: i32,
    scratch: [f32; ADJUST_WINDOW_AT5],
}

/// The `pwc_qu_at5` leaf (native 0x2a300): when the group's spectral
/// level is positive and the band is 2 or higher, add
/// `level / 2^(max gain shift + word length)`-scaled dither into the
/// dequantization buffer. Config `+0xa8 == 2` with a set `+0x50`
/// group flag reads the other channel's rows.
#[allow(clippy::too_many_arguments)]
fn pwc_qu_at5(
    channels: &[AdjustChannelState],
    channel: usize,
    phase: i32,
    band: usize,
    word_length: i32,
    out: &mut [f32],
    state: &mut PwcState,
) -> Result<(), AdjustPassError> {
    let x_table = x_at5();
    let idspcbands = idspcbands_at5();
    let nsps = nsps_at5();
    let spclev = spclev_at5();
    let lngain = lngain_at5();
    let rndtbl = rndtbl_at5();

    let group = x_table[band + 1] as usize;
    let mut source = channel;
    if channels[channel].config_a8 == 2 && channels[channel].group_flags[group] != 0 {
        if channels.len() != 2 {
            return Err(AdjustPassError::OutOfScope(
                "pwc channel swap needs two channels",
            ));
        }
        source = 1 - channel;
    }
    let meta = &channels[source];
    let level_index = idspcbands[group] as usize;
    let word = meta.spc_level_words[level_index];
    if !(0..spclev.len() as i32).contains(&word) {
        return Err(AdjustPassError::OutOfScope(
            "spectral level word outside the native table",
        ));
    }
    let level = spclev[word as usize];
    // Native gates: band >= 2 and level strictly positive.
    if band < 2 || !(level > 0.0) {
        return Ok(());
    }
    let count = nsps[band] as usize;
    if group as i32 != state.last_group {
        state.last_group = group as i32;
        for index in 0..count {
            let entry = ((phase + index as i32) & 0x3ff) as usize;
            // (float)rndtbl[i] * 3.0517578e-05f: exact in f32.
            state.scratch[index] = (f64::from(rndtbl[entry]) * f64::from(3.0517578e-05f32)) as f32;
        }
    }

    fn row_levels(row: &AdjustGainRow) -> Result<&AdjustGainRow, AdjustPassError> {
        if row.count > ADJUST_GAIN_LEVELS_AT5 as i32 || row.level_ids.len() < ADJUST_GAIN_LEVELS_AT5
        {
            return Err(AdjustPassError::OutOfScope(
                "gain row outside the captured level window",
            ));
        }
        Ok(row)
    }
    let prev = row_levels(&meta.prev_gain_rows[group])?;
    let cur = row_levels(&meta.cur_gain_rows[group])?;
    let gain = |id: i32| -> Result<i16, AdjustPassError> {
        lngain
            .get(id as usize)
            .copied()
            .ok_or(AdjustPassError::OutOfScope(
                "gain level id outside g_a_lngain_at5",
            ))
    };
    let mut base: i16 = 0;
    if prev.count > 0 {
        base = gain(prev.level_ids[0])?.wrapping_neg();
    }
    let mut max_shift: i16 = 0;
    for index in 0..cur.count.max(0) as usize {
        let shift = base.wrapping_sub(gain(cur.level_ids[index])?);
        if max_shift < shift {
            max_shift = shift;
        }
    }
    for index in 0..prev.count.max(0) as usize {
        let shift = gain(prev.level_ids[index])?.wrapping_neg();
        if max_shift < shift {
            max_shift = shift;
        }
    }
    // Native: fildl(1 << ((shift + wl) & 0x1f)) then level / that in
    // extended precision, never rounded until the per-element store.
    let denom = 1i32.wrapping_shl(((i32::from(max_shift) + word_length) & 0x1f) as u32);
    let quotient = f64::from(level) / f64::from(denom);
    for index in 0..count.min(out.len()) {
        out[index] = (quotient * f64::from(state.scratch[index]) + f64::from(out[index])) as f32;
    }
    Ok(())
}

/// The per-band magnitude sum (native `0x45fe8..0x46090`): four
/// forward lanes of `fabs` accumulated as
/// `acc = (|d| + (|c| + (|b| + |a|))) + acc` with an f64 accumulator,
/// mirrored to f32 for the flatness quotient.
fn magnitude_sum_at5(buffer: &[f32]) -> f32 {
    let quarter = buffer.len() >> 2;
    let mut acc = 0f64;
    for index in 0..quarter {
        let a = f64::from(buffer[index].abs());
        let b = f64::from(buffer[quarter + index].abs());
        let c = f64::from(buffer[2 * quarter + index].abs());
        let d = f64::from(buffer[3 * quarter + index].abs());
        acc = (d + (c + (b + a))) + acc;
    }
    acc as f32
}

/// Run the scoped adjust pass.
pub fn adjust_scalefactors_frame_at5(
    state: &mut AdjustFrameState,
) -> Result<AdjustFrameOutput, AdjustPassError> {
    let channel_count = state.channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(AdjustPassError::OutOfScope(
            "only one- or two-channel adjust calls are supported",
        ));
    }
    if state.band_count > ADJUST_BANDS_AT5 || state.selector >= ADJSCL_STARTQU_AT5_ENTRIES {
        return Err(AdjustPassError::OutOfScope(
            "band count or selector outside the native tables",
        ));
    }
    if state.side_band_modes.len() < state.band_count {
        return Err(AdjustPassError::RowTooShort {
            needed: state.band_count,
            actual: state.side_band_modes.len(),
        });
    }
    let nsps = nsps_at5();
    let isps = isps_at5();
    for channel in &state.channels {
        let needed = state.band_count;
        let actual = channel
            .word_lengths
            .len()
            .min(channel.scale_factors.len())
            .min(channel.spectra.len())
            .min(channel.aux_weights.len());
        if actual < needed {
            return Err(AdjustPassError::RowTooShort { needed, actual });
        }
        if channel.quantized.len() < ADJUST_QUANT_PLANE_AT5
            || channel.group_flags.len() < ADJUST_GROUPS_AT5
            || channel.spc_level_words.len() < IDSPCBANDS_AT5_ENTRIES
            || channel.cur_gain_rows.len() < ADJUST_GROUPS_AT5
            || channel.prev_gain_rows.len() < ADJUST_GROUPS_AT5
            || channel.group_count_c0 > ADJUST_GROUPS_AT5
        {
            return Err(AdjustPassError::OutOfScope(
                "channel surface narrower than the native layout",
            ));
        }
        for band in 0..needed {
            if channel.spectra[band].len() < nsps[band] as usize {
                return Err(AdjustPassError::RowTooShort {
                    needed: nsps[band] as usize,
                    actual: channel.spectra[band].len(),
                });
            }
        }
    }

    // Phase seeds: the u16 idsf-word sum masked and stepped by 0x80
    // per group (`local_3c`).
    let mut sum: u16 = 0;
    for channel in &state.channels {
        for band in 0..channel.band_count_b0.min(channel.scale_factors.len()) {
            sum = sum.wrapping_add(channel.scale_factors[band] as u16);
        }
    }
    let mut phases = [0u16; ADJUST_GROUPS_AT5];
    for phase in phases
        .iter_mut()
        .take(state.channels[0].group_count_c0.min(ADJUST_GROUPS_AT5))
    {
        sum &= 0x3fc;
        *phase = sum;
        sum += 0x80;
    }

    // Threshold constants: the low-selector stereo / selector-9 mono
    // profile uses the 2^(1/3) pair and a 10-step cap.
    let wide = (state.selector < 0x18 && channel_count == 2)
        || (state.selector == 9 && channel_count == 1);
    let (overshoot_up, overshoot_down, cap) = if wide {
        (1.2599211f32, 0.7937005f32, 10i32)
    } else {
        (1.122462f32, 0.70710677f32, 5i32)
    };

    let ifqf = ifqf_at5();
    let sftbl = sftbl_at5();
    let x_table = x_at5();
    let start = adjscl_startqu_at5()[state.selector] as usize;
    let mut pwc_state = PwcState {
        last_group: -1,
        scratch: [0f32; ADJUST_WINDOW_AT5],
    };
    let mut upper = [0f32; ADJUST_WINDOW_AT5];
    let mut lower = [0f32; ADJUST_WINDOW_AT5];
    let mut pwc_calls = 0usize;

    for band in start..state.band_count {
        let count = nsps[band] as usize;
        let base = isps[band] as usize;
        let group = x_table[band + 1] as usize;
        for channel in 0..channel_count {
            // `local_674`: channel 1 follows channel 0's rows on
            // joint bands (side +0x94 == 1).
            let effective = if channel == 1 {
                usize::from(state.side_band_modes[band] != 1)
            } else {
                channel
            };
            if state.channels[effective].word_lengths[band] <= 0 {
                continue;
            }
            let use_lower = channel == 1 && effective == 0;
            if !use_lower {
                let joint_fill = channel == 0 && state.side_band_modes[band] == 1;
                for index in 0..count {
                    let value = f32::from(state.channels[channel].quantized[base + index]);
                    upper[index] = value;
                    if joint_fill {
                        lower[index] = value;
                    }
                }
            }
            let original_idsf = state.channels[channel].scale_factors[band];
            if original_idsf <= 0 {
                continue;
            }

            let word_length = state.channels[effective].word_lengths[band];
            if !(0..ifqf.len() as i32).contains(&word_length)
                || !(0..sftbl.len() as i32).contains(&original_idsf)
            {
                return Err(AdjustPassError::OutOfScope(
                    "word length or idsf outside the native tables",
                ));
            }
            {
                let buffer: &mut [f32] = if use_lower {
                    &mut lower[..count]
                } else {
                    &mut upper[..count]
                };
                pwc_qu_at5(
                    &state.channels,
                    channel,
                    i32::from(phases[group] as i16),
                    band,
                    word_length,
                    buffer,
                    &mut pwc_state,
                )?;
            }
            pwc_calls += 1;
            let buffer: &[f32] = if use_lower {
                &lower[..count]
            } else {
                &upper[..count]
            };

            // fVar6 = ifqf[wl] * sftbl[idsf], one f32 round.
            let dequant_scale = (f64::from(ifqf[word_length as usize])
                * f64::from(sftbl[original_idsf as usize])) as f32;
            let spectrum_scale = sftbl[original_idsf as usize];
            // Extended-precision energy accumulation from index
            // count-1 downward, one f32 round after the scale.
            let mut quant_acc = 0f64;
            let mut spec_acc = 0f64;
            let window = &state.channels[channel].spectra[band];
            for index in (0..count).rev() {
                quant_acc += f64::from(buffer[index]) * f64::from(buffer[index]);
                spec_acc += f64::from(window[index]) * f64::from(window[index]);
            }
            let spec_energy =
                (spec_acc * (f64::from(spectrum_scale) * f64::from(spectrum_scale))) as f32;
            let quant_energy_wide =
                quant_acc * (f64::from(dequant_scale) * f64::from(dequant_scale));
            // Native zero gates: the quantized side checks the
            // extended value before rounding, the spectrum side the
            // stored f32.
            if quant_energy_wide == 0.0 || spec_energy == 0.0 {
                continue;
            }
            let mut quant_energy = quant_energy_wide as f32;

            let aux = state.channels[effective].aux_weights[band];
            let mirror = magnitude_sum_at5(buffer);
            let mut flatness = if mirror > 0.0 {
                ((count as f64 * f64::from(spectrum_scale)) / f64::from(mirror)) as f32
            } else {
                0.0
            };
            flatness = (f64::from(flatness) / (f64::from(aux) * f64::from(dequant_scale))) as f32;

            let row = &mut state.channels[channel].scale_factors[band];
            if quant_energy < spec_energy {
                // Case A: underquantized — ladder the idsf upward.
                if *row <= ADJUST_IDSF_MAX_AT5 {
                    loop {
                        let product = f64::from(quant_energy) * f64::from(1.587401f32);
                        *row += 1;
                        let more = f64::from(spec_energy) > product;
                        quant_energy = product as f32;
                        if !more || *row > ADJUST_IDSF_MAX_AT5 {
                            break;
                        }
                    }
                }
                if f64::from(overshoot_up) * f64::from(spec_energy) < f64::from(quant_energy) {
                    *row -= 1;
                }
                let decrement = if !(aux > 3.0) {
                    flatness > 3.0
                } else if flatness > 1.5 {
                    true
                } else {
                    flatness < 0.75
                };
                if decrement {
                    *row -= 1;
                }
                if *row < original_idsf {
                    *row = original_idsf;
                } else if *row - original_idsf > cap {
                    *row = original_idsf + cap;
                }
            } else if aux < 6.0 || flatness < 0.5 {
                // Case B: overquantized — ladder the idsf downward.
                if quant_energy > spec_energy && *row > 0 {
                    loop {
                        let product = f64::from(quant_energy) * f64::from(0.62996054f32);
                        *row -= 1;
                        let more = product > f64::from(spec_energy);
                        quant_energy = product as f32;
                        if !more || *row <= 0 {
                            break;
                        }
                    }
                }
                if f64::from(quant_energy) < f64::from(overshoot_down) * f64::from(spec_energy) {
                    *row += 1;
                }
            }
        }
    }

    // Epilogue: IDSF side-data recount and side rebasing.
    let mut recount: i32 = 0;
    let mut block_state_zeroed = false;
    let mut leaf_bits: Vec<Option<i32>> = vec![None; channel_count];
    let mut idsf_blocks: Vec<Option<IdsfBlockState>> = vec![None; channel_count];
    if state.channels[0].band_count_b0 > 0 {
        recount = channel_count as i32 * 2;
        if state.side_gate_8c == 0 {
            block_state_zeroed = true;
            for channel in &state.channels {
                recount += channel.band_count_b0 as i32 * 6;
            }
        } else {
            let previous: Vec<i32> = state.channels[0].scale_factors.clone();
            for (index, channel) in state.channels.iter().enumerate() {
                // Retain the block state the leaf writes (the object IDSF
                // packing-prep words) instead of discarding it; it is the last
                // native writer of those words before pack.
                let mut idsf_block = IdsfBlockState::default();
                let leaf = calc_nbits_for_idsf_ch_at5(
                    &IdsfChannelState {
                        mode: channel.obj_mode,
                        band_count: channel.band_count_b0,
                        group_count: channel.leaf_group_count_b8,
                        scale_factors: &channel.scale_factors,
                        previous_scale_factors: &previous,
                    },
                    &mut idsf_block,
                )?;
                leaf_bits[index] = Some(leaf);
                idsf_blocks[index] = Some(idsf_block);
                recount += leaf;
            }
        }
    }
    let old_12a = state.base_total_12a;
    let new_12a = old_12a
        .wrapping_sub(state.idsf_bits_11c)
        .wrapping_add(recount as i16);
    let new_12e = state
        .current_bits_12e
        .wrapping_sub(old_12a)
        .wrapping_add(new_12a);

    let channels = state
        .channels
        .iter()
        .zip(leaf_bits.iter().zip(idsf_blocks.into_iter()))
        .map(|(channel, (leaf, idsf_block))| AdjustChannelOutput {
            scale_factors: channel.scale_factors.clone(),
            leaf_bits: *leaf,
            idsf_block,
        })
        .collect();
    Ok(AdjustFrameOutput {
        channels,
        idsf_bits_11c: recount as i16,
        base_total_12a: new_12a,
        extended_total_12e: new_12e,
        recount_bits: recount,
        block_state_zeroed,
        pwc_calls,
    })
}
