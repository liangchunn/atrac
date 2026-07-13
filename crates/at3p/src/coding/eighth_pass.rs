//! Scoped composition of `eighth_bit_allocation_at5` (native 0x51380)
//! ATRAC3plus 352 kbps.
//!
//! Native source of truth is the decompiled boundary at
//! `0x00051380..0x00051a7c` (Ghidra `0x00061380`), direct disassembly,
//! and `eighth_io_trace.ndjson`. The pass is the over-budget recovery
//! stage: it runs only while the current bit total (side `+0x12e`)
//! exceeds the frame target (`param_6`). Walking bands from high to
//! low, for each channel with a positive word length it raises a
//! local idsf trial value from the `+0xcc` word (which itself is
//! never written) up to `0x3d`; each trial computes the threshold
//! `pow(10, trial_idsf * 0.030103f + 0.05f) * 0.5f
//! / (g_a_nsteps_at5[wl] + 0.5f)` scaled by the block `+0x24c` band
//! level, sorts the band's absolute spectrum descending (shell sort
//! with a parallel index array), zeroes spectral lines strictly below
//! the scaled threshold, and re-runs `calc_nbits_var_rebitalloc_at5`
//! at the *unchanged* word length — the nested quant reads the pruned
//! spectrum and the unchanged scale-factor index live. A negative var
//! return accepts (trial cost row adopted into the active row,
//! selector stored at obj `+0x1b578`, `+0x11e/+0x12a/+0x12e` and the
//! `+0x46` slot rebased); otherwise the spectrum backup and every
//! channel's IDCT state are restored.
//!
//! On the traced 352 kbps target the entry gate never opens: all 84
//! native calls enter with `+0x12e <= target` (fifth/sixth already
//! stop at budget), so every captured call is a live no-op. The
//! threshold/zero/trial arithmetic is ported from the decompile and
//! covered synthetically through the verified quant/var leaves.

use crate::coding::bitcount::{
    BitcountError, IdctBlockState, VarRebitallocInput, calc_nbits_var_rebitalloc_at5,
};
use crate::coding::fifth_pass::live_idct_bits;
use crate::coding::quant::QuantError;
use crate::coding::quant_cost::quant_nontone_costs_at5;
use crate::tables::at5::{NSTEPS_AT5_ENTRIES, nsteps_at5};

const EIGHTH_CANDIDATES_AT5: usize = 8;
/// Highest idsf value a trial can reach (`cmp $0x3c` on the
/// pre-increment value keeps trials running through `0x3d`).
const EIGHTH_IDSF_TRIAL_MAX_AT5: i32 = 0x3d;

/// Native `pow(10.0, (double)(idsf as f32 * 0.030103f + 0.05f))` for
/// the whole scoped trial domain `idsf = 0..=0x3d`, pinned from the
/// inferior glibc i386 libm by `eighth_io_trace.ndjson` (`pow_table`).
/// The x87 argument chain is exact in f64 (the product needs <= 30
/// mantissa bits and the sum <= 31), but the pow result itself is
/// libm-specific: the host libm here differs by 1 ulp at idsf 30, so
/// the native results are embedded rather than recomputed.
pub const EIGHTH_POW10_BITS_AT5: [u64; 62] = [
    0x3ff1f3c99ff00b3a,
    0x3ff33da4a80b9ebd,
    0x3ff49f2c74f99b72,
    0x3ff61a1407554dc7,
    0x3ff7b02d987687dd,
    0x3ff9636cd81c38f7,
    0x3ffb35e9534391b9,
    0x3ffd29e107203d36,
    0x3fff41bb23608387,
    0x4000c0057f912e64,
    0x4001f3c9a21e6d5f,
    0x40033da4aa62149f,
    0x40049f2c777b0572,
    0x40061a140a04c120,
    0x4007b02d9b57526e,
    0x4009636cdb31e557,
    0x400b35e95691eb90,
    0x400d29e10aab55e8,
    0x400f41bb272cb739,
    0x4010c005819a2bdc,
    0x4011f3c9a44ccf84,
    0x40133da4acb88a82,
    0x40149f2c79fc6f72,
    0x40161a140cb43479,
    0x4017b02d9e381d00,
    0x4019636cde4791b8,
    0x401b35e959e04567,
    0x401d29e10e366e9b,
    0x401f41bb2af8eaeb,
    0x4020c00583a32954,
    0x4021f3c9a67b31aa,
    0x40233da4af0f0065,
    0x40249f2c7c7dd973,
    0x40261a140f63a7d3,
    0x4027b02da118e792,
    0x4029636ce15d3e19,
    0x402b35e95d2e9f3f,
    0x402d29e111c1874e,
    0x402f41bb2ec51e9e,
    0x4030c00585ac26cd,
    0x4031f3c9a8a993cf,
    0x40333da4b1657648,
    0x40349f2c7eff4374,
    0x40361a1412131b2d,
    0x4037b02da3f9b225,
    0x4039636ce472ea7b,
    0x403b35e9607cf917,
    0x403d29e1154ca002,
    0x403f41bb32915251,
    0x4040c00587b52446,
    0x4041f3c9aad7f5f5,
    0x40433da4b3bbec2b,
    0x40449f2c8180ad75,
    0x40461a1414c28e87,
    0x4047b02da6da7cb8,
    0x4049636ce78896dc,
    0x404b35e963cb52f0,
    0x404d29e118d7b8b6,
    0x404f41bb365d8604,
    0x4050c00589be21bf,
    0x4051f3c9ad06581b,
    0x40533da4b612620f,
];

#[derive(Debug)]
pub enum EighthPassError {
    Bitcount(BitcountError),
    Quant(QuantError),
    OutOfScope(&'static str),
    RowTooShort { needed: usize, actual: usize },
}

impl From<BitcountError> for EighthPassError {
    fn from(error: BitcountError) -> Self {
        EighthPassError::Bitcount(error)
    }
}

impl From<QuantError> for EighthPassError {
    fn from(error: QuantError) -> Self {
        EighthPassError::Quant(error)
    }
}

/// The per-trial threshold base `local_840` before the band-scale
/// multiply: `pow * 0.5f / (nsteps[wl] + 0.5f)`. The `pow * 0.5`
/// scale and `nsteps + 0.5` sum are exact; native performs the
/// division in x87 extended precision and rounds once to f32, while
/// this f64 division double-rounds — a divergence would need the
/// exact quotient within ~2^-40 of an f32 boundary, and any live
pub fn eighth_threshold_base_at5(
    trial_idsf: i32,
    word_length: i32,
) -> Result<f32, EighthPassError> {
    if !(0..=EIGHTH_IDSF_TRIAL_MAX_AT5).contains(&trial_idsf) {
        return Err(EighthPassError::OutOfScope(
            "trial idsf outside the pinned native pow domain",
        ));
    }
    if !(0..NSTEPS_AT5_ENTRIES as i32).contains(&word_length) {
        return Err(EighthPassError::OutOfScope(
            "word length outside the native nsteps table",
        ));
    }
    let pow = f64::from_bits(EIGHTH_POW10_BITS_AT5[trial_idsf as usize]);
    let nsteps = f64::from(nsteps_at5()[word_length as usize]);
    Ok(((pow * 0.5) / (nsteps + 0.5)) as f32)
}

/// The native descending shell sort over the band's absolute spectrum
/// (`0x00051539..0x000515bb`): values shift like a normal insertion
/// pass while the parallel index array swaps at each step.
fn shell_sort_descending_at5(values: &mut [f32], indices: &mut [usize]) {
    let count = values.len() as i32;
    let mut gap: i32 = 1;
    if count != 0 {
        while gap <= count {
            gap = gap * 3 + 1;
        }
    }
    loop {
        gap /= 3;
        if gap <= 0 {
            break;
        }
        for index in gap..count {
            let key = values[index as usize];
            let mut cursor = index - gap;
            if cursor >= 0 {
                while values[cursor as usize] < key {
                    let upper = (cursor + gap) as usize;
                    values[upper] = values[cursor as usize];
                    indices.swap(upper, cursor as usize);
                    cursor -= gap;
                    if cursor < 0 {
                        break;
                    }
                }
            }
            values[(cursor + gap) as usize] = key;
        }
    }
}

/// One channel's eighth-pass surface. The word-length row and the
/// `+0xcc` idsf row are read-only in this pass; the spectrum windows,
/// selector row, cost rows, IDCT state, and `+0x46` slot are carried
/// allocation state.
#[derive(Debug)]
pub struct EighthChannelState {
    /// `+0x1b5f8` word-length row (read-only in this pass).
    pub word_lengths: Vec<i32>,
    /// `+0x1b578` quant-table selector row.
    pub selector_row: Vec<i32>,
    /// The block `+0xcc` per-band idsf words (read-only: eighth
    /// raises only a local trial copy).
    pub band_idsf: Vec<i32>,
    /// The block `+0x24c` per-band scale levels.
    pub band_scale: Vec<f32>,
    /// Per-band spectrum windows (`spectrum + g_a_isps_at5[band]`,
    /// `g_a_nsps_at5[band]` floats); zeroed lines persist on accept.
    pub spectra: Vec<Vec<f32>>,
    /// Active/trial cost rows, flattened `band * 8 + candidate`.
    pub active_costs: Vec<i16>,
    pub trial_costs: Vec<i16>,
    /// IDCT block state at `+0x9f8`.
    pub idct_block: IdctBlockState,
    /// The channel `+0x46` slot total.
    pub quant_bits_46: i16,
    /// Object mode word and config `+0x90` (IDCT leaf inputs).
    pub obj_mode: u32,
    pub fixbits_index: usize,
    /// The channel's live plane/bandwidth word `*(obj + 0x1074)` — the
    /// descriptor state passed to the trial recost quant. Native loads
    /// it before the `calc_nbits_var_rebitalloc_at5` call, which
    /// forwards it untouched into `quant_nontone_nspecs_at5` at 0xc150
    pub quant_state: usize,
}

/// Frame-level eighth-pass inputs.
#[derive(Debug)]
pub struct EighthFrameState {
    pub channels: Vec<EighthChannelState>,
    /// `param_5` (bands walked high to low).
    pub band_count: usize,
    /// `param_6`: the frame bit target the pass reduces toward.
    pub target_bits: i32,
    /// Config `+0xb0` (IDCT leaf band count).
    pub active_band_count: usize,
    /// `sa_nencodetbls[encode selector]`.
    pub quant_candidate_count: usize,
    /// Entry side shorts `+0x11e`, `+0x12a`, `+0x12e`.
    pub idct_bits_11e: i16,
    pub base_total_12a: i16,
    pub current_bits_12e: i16,
}

#[derive(Debug)]
pub struct EighthChannelOutput {
    pub word_lengths: Vec<i32>,
    pub selector_row: Vec<i32>,
    pub band_idsf: Vec<i32>,
    pub spectra: Vec<Vec<f32>>,
    pub active_costs: Vec<i16>,
    pub quant_bits_46: i16,
    pub idct_block: IdctBlockState,
}

#[derive(Debug)]
pub struct EighthFrameOutput {
    pub channels: Vec<EighthChannelOutput>,
    pub idct_bits_11e: i16,
    pub base_total_12a: i16,
    pub extended_total_12e: i16,
    pub trials: usize,
    pub accepts: usize,
}

/// Run the scoped eighth pass. The entry/inner budget gates, trial
/// idsf ladder, threshold zeroing, and accept/reject arithmetic
/// mirror the native stores; the word-length and `+0xcc` rows pass
/// through untouched.
pub fn eighth_bit_allocation_frame_at5(
    state: &mut EighthFrameState,
) -> Result<EighthFrameOutput, EighthPassError> {
    let channel_count = state.channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(EighthPassError::OutOfScope(
            "only one- or two-channel eighth calls are supported",
        ));
    }
    for channel in &state.channels {
        let needed = state.band_count;
        let actual = channel
            .word_lengths
            .len()
            .min(channel.selector_row.len())
            .min(channel.band_idsf.len())
            .min(channel.band_scale.len())
            .min(channel.spectra.len());
        if actual < needed
            || channel.active_costs.len() < needed * EIGHTH_CANDIDATES_AT5
            || channel.trial_costs.len() < needed * EIGHTH_CANDIDATES_AT5
        {
            return Err(EighthPassError::RowTooShort { needed, actual });
        }
    }

    let obj_modes: Vec<u32> = state
        .channels
        .iter()
        .map(|channel| channel.obj_mode)
        .collect();
    let fixbits_indices: Vec<usize> = state
        .channels
        .iter()
        .map(|channel| channel.fixbits_index)
        .collect();
    let rows: Vec<Vec<i32>> = state
        .channels
        .iter()
        .map(|channel| channel.word_lengths.clone())
        .collect();
    let mut idct_blocks: Vec<IdctBlockState> = state
        .channels
        .iter()
        .map(|channel| channel.idct_block.clone())
        .collect();
    let mut bits_11e = state.idct_bits_11e;
    let mut bits_12a = state.base_total_12a;
    let mut bits_12e = state.current_bits_12e;
    let mut trials = 0usize;
    let mut accepts = 0usize;

    'pass: {
        // Entry gate: the pass runs only while over target.
        if i32::from(bits_12e) <= state.target_bits {
            break 'pass;
        }
        for band in (0..state.band_count).rev() {
            for channel in 0..channel_count {
                if rows[channel][band] > 0 {
                    let entry_idsf = state.channels[channel].band_idsf[band];
                    let quant_state = state.channels[channel].quant_state;
                    if i32::from(bits_12e) <= state.target_bits {
                        break 'pass;
                    }
                    if entry_idsf < EIGHTH_IDSF_TRIAL_MAX_AT5 {
                        let mut trial_idsf = entry_idsf + 1;
                        loop {
                            trials += 1;
                            let word_length = rows[channel][band];
                            let scale = state.channels[channel].band_scale[band];
                            let threshold = eighth_threshold_base_at5(trial_idsf, word_length)?;
                            // f32 * f32 is exact in f64, matching the
                            // native x87 extended compare.
                            let scaled = f64::from(threshold) * f64::from(scale);

                            let window = &mut state.channels[channel].spectra[band];
                            let count = window.len();
                            let mut values: Vec<f32> =
                                window.iter().map(|value| value.abs()).collect();
                            let mut indices: Vec<usize> = (0..count).collect();
                            shell_sort_descending_at5(&mut values, &mut indices);
                            let backup = window.clone();
                            // Zero from the smallest line upward while
                            // the scaled threshold is strictly greater
                            // (`test $0x45, %ah`: less, equal, and
                            // unordered all break).
                            let mut cursor = count;
                            while cursor > 0 {
                                cursor -= 1;
                                if !(scaled > f64::from(values[cursor])) {
                                    break;
                                }
                                window[indices[cursor]] = 0.0;
                            }

                            let saved_idct = idct_blocks.clone();
                            // The nested quant re-runs at the unchanged
                            // word length and unchanged `+0xcc` idsf;
                            // only the pruned spectrum changes cost.
                            // Descriptor state = the channel's live plane
                            // word `*(obj + 0x1074)` (native loads it
                            // before the var call, which forwards it into
                            // 0xc150 `state * 0x540`), not a hardcoded 0.
                            let costs = quant_nontone_costs_at5(
                                window,
                                word_length as usize,
                                entry_idsf as usize,
                                scale,
                                count,
                                quant_state,
                                state.quant_candidate_count,
                            )?;
                            for (candidate, cost) in
                                costs.iter().enumerate().take(EIGHTH_CANDIDATES_AT5)
                            {
                                state.channels[channel].trial_costs
                                    [band * EIGHTH_CANDIDATES_AT5 + candidate] = *cost as i16;
                            }

                            let var = calc_nbits_var_rebitalloc_at5(
                                VarRebitallocInput {
                                    quant_unit: band,
                                    channel_index: channel,
                                    channel_count,
                                    old_selector: state.channels[channel].selector_row[band]
                                        as usize,
                                    selector_count: state.quant_candidate_count,
                                    current_idct_bits: i32::from(bits_11e),
                                    source_costs: &state.channels[channel].active_costs,
                                    target_costs: &state.channels[channel].trial_costs,
                                },
                                &mut idct_blocks,
                                |blocks| {
                                    live_idct_bits(
                                        &obj_modes,
                                        &fixbits_indices,
                                        &rows,
                                        state.active_band_count,
                                        blocks,
                                    )
                                },
                            )?;

                            if var.bit_delta < 0 {
                                let base = band * EIGHTH_CANDIDATES_AT5;
                                let channel_state = &mut state.channels[channel];
                                let (active, trial) =
                                    (&mut channel_state.active_costs, &channel_state.trial_costs);
                                active[base..base + EIGHTH_CANDIDATES_AT5]
                                    .copy_from_slice(&trial[base..base + EIGHTH_CANDIDATES_AT5]);
                                channel_state.selector_row[band] = var.word_length as i32;
                                let delta_12a = (var.idct_bits as i16).wrapping_sub(bits_11e);
                                bits_12a = bits_12a.wrapping_add(delta_12a);
                                channel_state.quant_bits_46 = channel_state
                                    .quant_bits_46
                                    .wrapping_add((var.bit_delta as i16).wrapping_sub(delta_12a));
                                bits_11e = var.idct_bits as i16;
                                bits_12e = bits_12e.wrapping_add(var.bit_delta as i16);
                                accepts += 1;
                            } else {
                                state.channels[channel].spectra[band].copy_from_slice(&backup);
                                idct_blocks = saved_idct;
                            }

                            if i32::from(bits_12e) <= state.target_bits {
                                break 'pass;
                            }
                            if trial_idsf >= EIGHTH_IDSF_TRIAL_MAX_AT5 {
                                break;
                            }
                            trial_idsf += 1;
                        }
                    }
                }
                if i32::from(bits_12e) <= state.target_bits {
                    break 'pass;
                }
            }
            if i32::from(bits_12e) <= state.target_bits {
                break 'pass;
            }
        }
    }

    let channels = state
        .channels
        .iter()
        .enumerate()
        .map(|(index, channel)| EighthChannelOutput {
            word_lengths: channel.word_lengths.clone(),
            selector_row: channel.selector_row.clone(),
            band_idsf: channel.band_idsf.clone(),
            spectra: channel.spectra.clone(),
            active_costs: channel.active_costs.clone(),
            quant_bits_46: channel.quant_bits_46,
            idct_block: idct_blocks[index].clone(),
        })
        .collect();
    Ok(EighthFrameOutput {
        channels,
        idct_bits_11e: bits_11e,
        base_total_12a: bits_12a,
        extended_total_12e: bits_12e,
        trials,
        accepts,
    })
}
