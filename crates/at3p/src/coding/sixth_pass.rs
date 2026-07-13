//! Scoped composition of `sixth_bit_allocation_at5` (native 0x31cb0)
//! ATRAC3plus 352 kbps.
//!
//! Native source of truth is the decompiled boundary at
//! `0x00031cb0..0x000320f5` plus `sixth_io_trace.ndjson`. The pass
//! makes a single sweep over the `param_6` band order: for each band
//! with a positive `+0xcc` idsf word and a positive word length,
//! under the budget threshold (`max(nsps >> 4, side + 0x90)`), it
//! decrements the `+0xcc` word, snapshots the IDCT block states, and
//! runs `calc_nbits_var_rebitalloc_at5` at the *unchanged* word
//! length — the nested quant reads the decremented scale-factor index
//! live — accepting (trial cost row adopted, selector updated,
//! `+0x11e/+0x12a/+0x12e` and the `+0x46` slot rebased) or rolling
//! back (idsf word and IDCT states restored) against the budget. The
//! word-length rows and the IDWL side state are never touched.
//!
//! On the traced 352 kbps target every `+0xcc` word is zero, so all
//! six captured calls are live no-ops; the decrement/trial arithmetic
//! is ported from the decompile and covered synthetically.

use crate::coding::bitcount::{
    BitcountError, IdctBlockState, VarRebitallocInput, calc_nbits_var_rebitalloc_at5,
};
use crate::coding::fifth_pass::live_idct_bits;
use crate::coding::quant::QuantError;
use crate::coding::quant_cost::quant_nontone_costs_at5;
use crate::coding::zeroth_pass::ZerothQuantBandRaw;
use crate::tables::at5::nsps_at5;

const SIXTH_CANDIDATES_AT5: usize = 8;

#[derive(Debug)]
pub enum SixthPassError {
    Bitcount(BitcountError),
    Quant(QuantError),
    OutOfScope(&'static str),
    RowTooShort { needed: usize, actual: usize },
}

impl From<BitcountError> for SixthPassError {
    fn from(error: BitcountError) -> Self {
        SixthPassError::Bitcount(error)
    }
}

impl From<QuantError> for SixthPassError {
    fn from(error: QuantError) -> Self {
        SixthPassError::Quant(error)
    }
}

/// One channel's sixth-pass surface. The idsf row, selector row, cost
/// rows, IDCT state, and `+0x46` slot are carried allocation state.
#[derive(Debug)]
pub struct SixthChannelState<'a> {
    /// `+0x1b5f8` word-length row (read-only in this pass).
    pub word_lengths: Vec<i32>,
    /// `+0x1b578` quant-table selector row.
    pub selector_row: Vec<i32>,
    /// The block `+0xcc` per-band idsf words (decremented by accepted
    /// trials).
    pub band_idsf: Vec<i32>,
    /// Active/trial cost rows, flattened `band * 8 + candidate`.
    pub active_costs: Vec<i16>,
    pub trial_costs: Vec<i16>,
    /// IDCT block state at `+0x9f8`.
    pub idct_block: IdctBlockState,
    /// The channel `+0x46` slot total.
    pub quant_bits_46: i16,
    /// Per-band quant inputs (spectrum window, `+0x24c` scale, spec
    /// count); the `idsf` field is superseded by the live `band_idsf`
    /// row.
    pub quant_bands: &'a [ZerothQuantBandRaw<'a>],
    /// Object mode word and config `+0x90` (IDCT leaf inputs).
    pub obj_mode: u32,
    pub fixbits_index: usize,
    /// The channel's live plane/bandwidth word `*(obj + 0x1074)` — the
    /// descriptor state passed to the trial recost quant (native loads
    /// it before the `calc_nbits_var_rebitalloc_at5` call, which
    /// forwards it untouched into `quant_nontone_nspecs_at5` at 0xc150,
    pub quant_state: usize,
}

/// Frame-level sixth-pass inputs.
#[derive(Debug)]
pub struct SixthFrameState<'a> {
    pub channels: Vec<SixthChannelState<'a>>,
    /// `param_4`.
    pub band_count: usize,
    /// `param_5`.
    pub budget_limit: i32,
    /// `param_6`: band order, `band_count * channels` entries.
    pub order: &'a [i32],
    /// Config `+0xb0` (IDCT leaf band count).
    pub active_band_count: usize,
    /// Side `+0x90`.
    pub threshold_90: i32,
    /// `sa_nencodetbls[encode selector]`.
    pub quant_candidate_count: usize,
    /// Entry side shorts `+0x11e`, `+0x12a`, `+0x12e`.
    pub idct_bits_11e: i16,
    pub base_total_12a: i16,
    pub current_bits_12e: i16,
}

#[derive(Debug)]
pub struct SixthChannelOutput {
    pub word_lengths: Vec<i32>,
    pub selector_row: Vec<i32>,
    pub band_idsf: Vec<i32>,
    pub active_costs: Vec<i16>,
    pub quant_bits_46: i16,
    pub idct_block: IdctBlockState,
}

#[derive(Debug)]
pub struct SixthFrameOutput {
    pub channels: Vec<SixthChannelOutput>,
    pub idct_bits_11e: i16,
    pub base_total_12a: i16,
    pub extended_total_12e: i16,
    pub return_bits: i16,
    pub trials: usize,
    pub accepts: usize,
}

/// Run the scoped sixth pass. The idsf decrements, selector/cost-row
/// adoption, and the `+0x11e/+0x12a/+0x12e`/`+0x46` updates mirror
/// the native stores; word-length rows pass through untouched.
pub fn sixth_bit_allocation_frame_at5(
    state: &mut SixthFrameState<'_>,
) -> Result<SixthFrameOutput, SixthPassError> {
    let channel_count = state.channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(SixthPassError::OutOfScope(
            "only one- or two-channel sixth calls are supported",
        ));
    }
    if state.order.len() < state.band_count * channel_count {
        return Err(SixthPassError::RowTooShort {
            needed: state.band_count * channel_count,
            actual: state.order.len(),
        });
    }
    for channel in &state.channels {
        let needed = state.band_count;
        let actual = channel
            .word_lengths
            .len()
            .min(channel.selector_row.len())
            .min(channel.band_idsf.len())
            .min(channel.quant_bands.len());
        if actual < needed
            || channel.active_costs.len() < needed * SIXTH_CANDIDATES_AT5
            || channel.trial_costs.len() < needed * SIXTH_CANDIDATES_AT5
        {
            return Err(SixthPassError::RowTooShort { needed, actual });
        }
    }

    let nsps = nsps_at5();
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

    if i32::from(bits_12e) <= state.budget_limit.wrapping_sub(state.threshold_90) {
        for index in 0..state.band_count * channel_count {
            let entry = state.order[index] as usize;
            let (channel, band) = if entry < state.band_count {
                (0usize, entry)
            } else {
                (1usize, entry - state.band_count)
            };
            let idsf = state.channels[channel].band_idsf[band];
            if idsf <= 0 || rows[channel][band] <= 0 {
                continue;
            }
            let mut threshold = i32::from(nsps[band] >> 4);
            if threshold < state.threshold_90 {
                threshold = state.threshold_90;
            }
            if i32::from(bits_12e) > state.budget_limit.wrapping_sub(threshold) {
                continue;
            }

            trials += 1;
            state.channels[channel].band_idsf[band] = idsf - 1;
            let saved_idct = idct_blocks.clone();

            let raw = &state.channels[channel].quant_bands[band];
            // Descriptor state = the channel's live plane word
            // `*(obj + 0x1074)` (disasm 0x49454 `mov 0x1074(%ecx),%edx`
            // -> var call; var forwards it into 0xc150 `state * 0x540`),
            // not a hardcoded 0.
            let costs = quant_nontone_costs_at5(
                raw.spectrum,
                rows[channel][band] as usize,
                (idsf - 1) as usize,
                raw.scale,
                raw.count,
                state.channels[channel].quant_state,
                state.quant_candidate_count,
            )?;
            for (candidate, cost) in costs.iter().enumerate().take(SIXTH_CANDIDATES_AT5) {
                state.channels[channel].trial_costs[band * SIXTH_CANDIDATES_AT5 + candidate] =
                    *cost as i16;
            }

            let var = calc_nbits_var_rebitalloc_at5(
                VarRebitallocInput {
                    quant_unit: band,
                    channel_index: channel,
                    channel_count,
                    old_selector: state.channels[channel].selector_row[band] as usize,
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

            if i32::from(bits_12e).wrapping_add(var.bit_delta) <= state.budget_limit {
                let base = band * SIXTH_CANDIDATES_AT5;
                let channel_state = &mut state.channels[channel];
                let (active, trial) = (&mut channel_state.active_costs, &channel_state.trial_costs);
                active[base..base + SIXTH_CANDIDATES_AT5]
                    .copy_from_slice(&trial[base..base + SIXTH_CANDIDATES_AT5]);
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
                state.channels[channel].band_idsf[band] = idsf;
                idct_blocks = saved_idct;
            }
        }
    }

    let channels = state
        .channels
        .iter()
        .enumerate()
        .map(|(index, channel)| SixthChannelOutput {
            word_lengths: channel.word_lengths.clone(),
            selector_row: channel.selector_row.clone(),
            band_idsf: channel.band_idsf.clone(),
            active_costs: channel.active_costs.clone(),
            quant_bits_46: channel.quant_bits_46,
            idct_block: idct_blocks[index].clone(),
        })
        .collect();
    Ok(SixthFrameOutput {
        channels,
        idct_bits_11e: bits_11e,
        base_total_12a: bits_12a,
        extended_total_12e: bits_12e,
        return_bits: bits_12e,
        trials,
        accepts,
    })
}
