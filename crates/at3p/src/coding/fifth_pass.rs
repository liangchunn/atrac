//! Scoped composition of `fifth_bit_allocation_at5` (native 0x48ad0)
//! over the shipped 44.1 kHz stereo path (352 kbps and the other
//! stereo rates) plus the widened 44.1 kHz MONO path (docs/14 §1.1).
//! Channel count is `param_3` (1 or 2); the whole function is
//! parameterized on it (entry/upgrade loops bound by `param_3`), and
//! the stereo-merge stage is gated on `param_3 == 2` (decompile 39469).
//!
//! Native source of truth is the decompiled boundary at
//! `0x00058ad0..0x00059750` plus `fifth_io_trace.ndjson`. Wiring:
//!
//! 1. Entry: each channel's `+0x1b578` selector row is copied into
//!    the IDCT aux row (`block + 0xa84`), `calc_nbits_for_idct_at5(1)`
//!    re-costs the side data, and the delta rebases
//!    `+0x11e/+0x12a/+0x12e`.
//! 2. Stereo merge stage (channel_count == 2 only, decompile 39469;
//!    the merge stage's sole read of the stereo-order argument param_7
//!    is at 39473, inside that gate): over the `param_7` band order
//!    (bounded by `g_a_y_at5[*(*(obj0 + 0x10))]`), when the channel
//!    word lengths differ, both are nonzero, and the scale factors are
//!    within 11, the smaller word length is raised by one and trialed.
//!    At channel_count == 1 the else-arm (decompile 39590–39592) just
//!    loads the same `+0x90` threshold word and skips the stage.
//! 3. Upgrade stage: seven rounds over the `param_6` order raising
//!    eligible bands (`0 < wl < max`, `+0xcc` word `<= 0xe`); a
//!    rejected trial marks the band ineligible.
//!
//! Each trial snapshots the IDCT block states, re-quantizes the band
//! at the trial word length into the trial cost rows
//! (`side + 0x138`, descriptor state = the channel's live plane word
//! `*(obj + 0x1074)`, which native loads before the var call and var
//! forwards untouched into `quant_nontone_nspecs_at5` at 0xc150 — 0 for
//! active/trial rows, snapshots WLC state via `copy_wlcinfo_at5`,
//! recomputes the IDWL side bits with `calc_nbits_for_idwl_ch_at5`
//! per channel (`selector_mode` = the trial channel index; the object
//! modes are 0/1 so exactly the trial channel's rows re-extract), and
//! applies or rolls back against the budget.
//!
//! Aliasing pinned by the trace: both channels' previous objects are
//! channel 0's object, so the IDWL previous rows and the IDCT
//! previous-source rows are channel 0's live word-length row, and the
//! IDCT `+0xb04` previous row is channel 0's live aux row; all three
//! views are refreshed from live state before each leaf call. The
//! WLC side block (`block0 + 0x768`) is shared between both channels'
//! block states and is mirrored around each leaf call.
//!
//! The traced 352 kbps path always enters with side `+0x84 = 1` and
//! `+0x88 = 1`, so the per-trial IDWL branch is the update family;
//! the `+0x84 == 0` WLC reset and `+0x88 == 0` init branches are
//! rejected instead of guessed.

use crate::coding::bitcount::{
    BitcountError, IdctBlockState, IdctChannelState, IdwlBlockState, IdwlChannelState,
    IdwlSideState, VarRebitallocInput, calc_nbits_for_idct_at5, calc_nbits_for_idwl_ch_at5,
    calc_nbits_var_rebitalloc_at5, copy_wlcinfo_at5,
};
use crate::coding::quant::QuantError;
use crate::coding::quant_cost::quant_nontone_costs_at5;
use crate::coding::zeroth_pass::ZerothQuantBandRaw;
use crate::tables::at5::nsps_at5;

pub const FIFTH_BANDS_AT5: usize = 32;
const FIFTH_CANDIDATES_AT5: usize = 8;

#[derive(Debug)]
pub enum FifthPassError {
    Bitcount(BitcountError),
    Quant(QuantError),
    /// The input would take a native branch outside the scoped
    /// 352 kbps 44.1 kHz stereo path.
    OutOfScope(&'static str),
    RowTooShort {
        needed: usize,
        actual: usize,
    },
}

impl From<BitcountError> for FifthPassError {
    fn from(error: BitcountError) -> Self {
        FifthPassError::Bitcount(error)
    }
}

impl From<QuantError> for FifthPassError {
    fn from(error: QuantError) -> Self {
        FifthPassError::Quant(error)
    }
}

/// One channel's fifth-pass surface. Row/cost/state fields are
/// carried allocation state, mutated in place like the native rows.
#[derive(Debug)]
pub struct FifthChannelState<'a> {
    /// `+0x1b5f8` word-length row.
    pub word_lengths: Vec<i32>,
    /// `+0x1b578` quant-table selector row (the active pick copy).
    pub selector_row: Vec<i32>,
    /// Active cost rows (`side + 0x130` base `+ 0x80`), flattened
    /// `band * 8 + candidate`.
    pub active_costs: Vec<i16>,
    /// Trial cost rows (`side + 0x138` base `+ 0x80`), flattened.
    pub trial_costs: Vec<i16>,
    /// IDCT block state at `+0x9f8`.
    pub idct_block: IdctBlockState,
    /// WLC block state at `+0x460`.
    pub wlc_block: IdwlBlockState,
    /// The channel `+0x46` slot total (slot `+0x1074` = 0).
    pub quant_bits_46: i16,
    /// Scale factors at `+0x1b678` (stereo-merge gate).
    pub scale_factors: &'a [i32],
    /// Per-band maximum word lengths at block `+0x02`.
    pub max_word_lengths: &'a [i16],
    /// Per-band quant inputs (spectrum window, `+0xcc` idsf word,
    /// `+0x24c` scale, spec count); the `+0xcc` word also gates the
    /// upgrade stage (`> 0xe` skips).
    pub quant_bands: &'a [ZerothQuantBandRaw<'a>],
    /// Object mode word (`*(object)`; 0 for channel 0, 1 for
    /// channel 1 on the scoped path).
    pub obj_mode: u32,
    /// The channel's live plane/bandwidth word `*(obj + 0x1074)` — the
    /// descriptor state passed to the trial recost quant. Native loads
    /// it before every `calc_nbits_var_rebitalloc_at5` call inside
    /// `second_bit_allocation_at5` (disasm 0x48f53/0x49454/0x51892
    /// `mov 0x1074(%ecx),%edx`); var forwards its param_2/edx untouched
    /// into `quant_nontone_nspecs_at5` (native 0xc150, `state * 0x540`).
    pub quant_state: usize,
    /// Config `+0x90` (IDCT fixbits index).
    pub fixbits_index: usize,
    /// Context `+0x1c`.
    pub context_kind: u32,
    /// Config `+0xc4` and `+0xb8`.
    pub word_count: usize,
    pub group_count: usize,
}

/// Frame-level fifth-pass inputs.
#[derive(Debug)]
pub struct FifthFrameState<'a> {
    pub channels: Vec<FifthChannelState<'a>>,
    /// `param_4`.
    pub band_count: usize,
    /// `param_5`.
    pub budget_limit: i32,
    /// `param_6`: upgrade order, `band_count * channels` entries
    /// (values `>= band_count` address channel 1).
    pub order: &'a [i32],
    /// `param_7`: stereo-merge band order. Consumed only at
    /// channel_count == 2 (native's sole read is at decompile 39473,
    /// inside the `param_3 == 2` gate at 39469); ignored at mono.
    pub stereo_order: &'a [i32],
    /// `g_a_y_at5[*(*(obj0 + 0x10))]`. Bounds the stereo-merge stage,
    /// which runs only at channel_count == 2 (decompile 39469); at
    /// mono the merge stage is skipped regardless of this value.
    pub stereo_bound: usize,
    /// Config `+0xb0` (IDCT leaf band count; the zeroth active
    /// count).
    pub active_band_count: usize,
    /// Side `+0x90`.
    pub threshold_90: i32,
    /// Side gates `+0x84`/`+0x88`; the scoped path requires both
    /// nonzero (IDWL update branch).
    pub side_gate_84: u32,
    pub side_gate_88: u32,
    /// The shared WLC side block at `block0 + 0x768`.
    pub shared_wlc_side: IdwlSideState,
    /// Context `+0x1c` word (`copy_wlcinfo_at5` param 4).
    pub mode_1c: u32,
    /// `sa_nencodetbls[encode selector]`.
    pub quant_candidate_count: usize,
    /// Entry side shorts `+0x11a`, `+0x11e`, `+0x12a`, `+0x12e`.
    pub idwl_bits_11a: i16,
    pub idct_bits_11e: i16,
    pub base_total_12a: i16,
    pub current_bits_12e: i16,
}

#[derive(Debug)]
pub struct FifthChannelOutput {
    pub word_lengths: Vec<i32>,
    pub selector_row: Vec<i32>,
    pub active_costs: Vec<i16>,
    pub quant_bits_46: i16,
    pub idct_block: IdctBlockState,
    pub wlc_block: IdwlBlockState,
}

#[derive(Debug)]
pub struct FifthFrameOutput {
    pub channels: Vec<FifthChannelOutput>,
    pub shared_wlc_side: IdwlSideState,
    /// Updated side shorts.
    pub idwl_bits_11a: i16,
    pub idct_bits_11e: i16,
    pub base_total_12a: i16,
    pub extended_total_12e: i16,
    /// The returned `+0x12e` running total.
    pub return_bits: i16,
    /// Trial diagnostics (native: var call count / accepted trials).
    pub trials: usize,
    pub accepts: usize,
}

struct PassState<'a> {
    rows: Vec<Vec<i32>>,
    selectors: Vec<Vec<i32>>,
    active_costs: Vec<Vec<i16>>,
    trial_costs: Vec<Vec<i16>>,
    idct_blocks: Vec<IdctBlockState>,
    wlc_blocks: Vec<IdwlBlockState>,
    wlc_scratch: Vec<IdwlBlockState>,
    shared_side: IdwlSideState,
    bits_46: Vec<i16>,
    meta: Vec<ChannelMeta<'a>>,
    band_count: usize,
    active_band_count: usize,
    budget: i32,
    mode_1c: u32,
    candidate_count: usize,
    /// `side+0x84 == 0`: every trial's IDWL recompute takes the WLC-reset
    /// prong (decompile 39508–39525 / 39660–39679) instead of the steady
    /// `calc_nbits_for_idwl_ch_at5` loop.
    wlc_reset: bool,
    bits_11a: i16,
    bits_11e: i16,
    bits_12a: i16,
    bits_12e: i16,
    trials: usize,
    accepts: usize,
}

struct ChannelMeta<'a> {
    obj_mode: u32,
    quant_state: usize,
    fixbits_index: usize,
    context_kind: u32,
    word_count: usize,
    group_count: usize,
    max_word_lengths: &'a [i16],
    quant_bands: &'a [ZerothQuantBandRaw<'a>],
    scale_factors: &'a [i32],
}

/// The IDCT leaf over live state: both channels' previous-source rows
/// are channel 0's live word-length row, and every block's `+0xb04`
/// previous row is channel 0's live aux row. Shared with the sixth
/// pass, whose var callback needs the same aliasing model.
pub(crate) fn live_idct_bits(
    obj_modes: &[u32],
    fixbits_indices: &[usize],
    rows: &[Vec<i32>],
    active_band_count: usize,
    blocks: &mut [IdctBlockState],
) -> Result<i32, BitcountError> {
    let aux0 = blocks[0].aux;
    for block in blocks.iter_mut() {
        block.previous = aux0;
    }
    let source_rows: Vec<Vec<u32>> = rows
        .iter()
        .map(|row| row.iter().map(|&value| value as u32).collect())
        .collect();
    let previous_row: Vec<u32> = rows[0].iter().map(|&value| value as u32).collect();
    let channels: Vec<IdctChannelState<'_>> = obj_modes
        .iter()
        .zip(fixbits_indices)
        .enumerate()
        .map(|(index, (mode, fixbits_index))| IdctChannelState {
            mode: *mode,
            bandwidth_mode: *fixbits_index,
            band_count: active_band_count,
            idct_source: &source_rows[index],
            previous_idct_source: &previous_row,
        })
        .collect();
    calc_nbits_for_idct_at5(&channels, blocks, 1)
}

impl<'a> PassState<'a> {
    /// One budget-gated word-length trial (native merge/upgrade
    /// bodies at `0x00058bxx` and `0x00058exx`). The caller has
    /// already stored the raised word length into the row. Returns
    /// whether the trial was accepted.
    fn trial(&mut self, channel: usize, band: usize, old_wl: i32) -> Result<bool, FifthPassError> {
        self.trials += 1;
        let saved_idct = self.idct_blocks.clone();

        // The nested quant refresh at the trial word length into the
        // channel's trial rows. Descriptor state = the channel's live
        // plane word `*(obj + 0x1074)` (native loads it before the var
        // call; var forwards it untouched into 0xc150 `state * 0x540`),
        // not a hardcoded 0.
        let raw = &self.meta[channel].quant_bands[band];
        let costs = quant_nontone_costs_at5(
            raw.spectrum,
            self.rows[channel][band] as usize,
            raw.idsf,
            raw.scale,
            raw.count,
            self.meta[channel].quant_state,
            self.candidate_count,
        )?;
        for (candidate, cost) in costs.iter().enumerate().take(FIFTH_CANDIDATES_AT5) {
            self.trial_costs[channel][band * FIFTH_CANDIDATES_AT5 + candidate] = *cost as i16;
        }

        let var = {
            let obj_modes: Vec<u32> = self.meta.iter().map(|meta| meta.obj_mode).collect();
            let fixbits_indices: Vec<usize> =
                self.meta.iter().map(|meta| meta.fixbits_index).collect();
            let rows = &self.rows;
            let active_band_count = self.active_band_count;
            calc_nbits_var_rebitalloc_at5(
                VarRebitallocInput {
                    quant_unit: band,
                    channel_index: channel,
                    channel_count: self.meta.len(),
                    old_selector: self.selectors[channel][band] as usize,
                    selector_count: self.candidate_count,
                    current_idct_bits: i32::from(self.bits_11e),
                    source_costs: &self.active_costs[channel],
                    target_costs: &self.trial_costs[channel],
                },
                &mut self.idct_blocks,
                |blocks| {
                    live_idct_bits(
                        &obj_modes,
                        &fixbits_indices,
                        rows,
                        active_band_count,
                        blocks,
                    )
                },
            )?
        };

        // WLC snapshot, then the per-channel IDWL update recompute
        // with selector_mode = the trial channel index.
        self.wlc_blocks[0].side = self.shared_side.clone();
        copy_wlcinfo_at5(
            &self.wlc_blocks,
            &mut self.wlc_scratch,
            self.meta.len(),
            self.mode_1c,
            channel,
        )?;
        let mut side_bits: i32 = self.meta.len() as i32 * 2;
        if self.wlc_reset {
            // `+0x84 == 0` WLC-reset prong (decompile 39508–39525 /
            // 39660–39679, identical at both native trial sites): per channel
            // the `block+0x460` record words are reset — [0] (mode) = 0 and
            // [5..10] (`selector_fields_14_24`) = [0, 0, cfg[0xc4], 0, 0] —
            // and the side bits accumulate `cfg[0xc4] * 3` on the
            // `param_3 * 2` base. No IDWL leaf calls, no shared-side
            // threading. `word_count` is the channel's cfg `+0xc4` (32,
            // captured in `second_io_flag_path_trace.ndjson`).
            for index in 0..self.meta.len() {
                let c4 = self.meta[index].word_count as i32;
                self.wlc_blocks[index].mode = 0;
                self.wlc_blocks[index].selector_fields_14_24 = [0, 0, c4, 0, 0];
                side_bits = side_bits.wrapping_add(c4 * 3);
            }
        } else {
            for index in 0..self.meta.len() {
                self.wlc_blocks[index].side = self.shared_side.clone();
                let channel_state = IdwlChannelState {
                    mode: self.meta[index].obj_mode,
                    context_kind: self.meta[index].context_kind,
                    word_count: self.meta[index].word_count,
                    group_count: self.meta[index].group_count,
                    word_lengths: &self.rows[index],
                    previous_word_lengths: &self.rows[0],
                };
                let bits = calc_nbits_for_idwl_ch_at5(
                    &channel_state,
                    &mut self.wlc_blocks[index],
                    channel as u32,
                    band,
                )?;
                self.shared_side = self.wlc_blocks[index].side.clone();
                side_bits = side_bits.wrapping_add(bits);
            }
        }

        let delta = var
            .bit_delta
            .wrapping_sub(i32::from(self.bits_11a).wrapping_sub(side_bits));
        if self.budget < i32::from(self.bits_12e).wrapping_add(delta) {
            // Rejected: restore WLC, the row, and the IDCT states.
            copy_wlcinfo_at5(
                &self.wlc_scratch,
                &mut self.wlc_blocks,
                self.meta.len(),
                self.mode_1c,
                channel,
            )?;
            if channel == 0 {
                self.shared_side = self.wlc_blocks[0].side.clone();
            }
            self.rows[channel][band] = old_wl;
            self.idct_blocks = saved_idct;
            return Ok(false);
        }

        // Accepted: adopt the trial cost row, the selector, and the
        // side-word deltas.
        let base = band * FIFTH_CANDIDATES_AT5;
        let (active, trial) = (&mut self.active_costs[channel], &self.trial_costs[channel]);
        active[base..base + FIFTH_CANDIDATES_AT5]
            .copy_from_slice(&trial[base..base + FIFTH_CANDIDATES_AT5]);
        self.selectors[channel][band] = var.word_length as i32;
        let delta_12a = (side_bits as i16)
            .wrapping_sub(self.bits_11a)
            .wrapping_add(var.idct_bits as i16)
            .wrapping_sub(self.bits_11e);
        self.bits_12a = self.bits_12a.wrapping_add(delta_12a);
        self.bits_46[channel] =
            self.bits_46[channel].wrapping_add((delta as i16).wrapping_sub(delta_12a));
        self.bits_12e = self.bits_12e.wrapping_add(delta as i16);
        self.bits_11a = side_bits as i16;
        self.bits_11e = var.idct_bits as i16;
        self.accepts += 1;
        Ok(true)
    }
}

/// Run the scoped fifth pass. Word-length rows, selector rows, active
/// cost rows, `+0x46` totals, WLC/IDCT block states, and the
/// `+0x11a/+0x11e/+0x12a/+0x12e` updates mirror the native stores.
pub fn fifth_bit_allocation_frame_at5(
    state: &mut FifthFrameState<'_>,
) -> Result<FifthFrameOutput, FifthPassError> {
    let channel_count = state.channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(FifthPassError::OutOfScope(
            "fifth pass is implemented for channel_count 1 (mono) and 2 (stereo) only",
        ));
    }
    // Trial-recompute IDWL prong selection (native fork, identical at both
    // fifth trial sites — decompile 39508–39543 and 39660–39699): `+0x84 == 0`
    // is checked FIRST and takes the WLC-reset prong (ported in
    // `PassState::trial`; the flag-set calc entry clears `+0x84` and the
    // second pass's reset prong leaves `+0x88` at 0); otherwise `+0x88 == 0`
    // takes the init-leaf prong (still fail-explicit — unreached on both the
    // scoped flag==0 path, where second sets `+0x88 = 1`, and the flag-set
    // path); `+0x88 != 0` is the steady `calc_nbits_for_idwl_ch_at5` prong.
    if state.side_gate_84 != 0 && state.side_gate_88 == 0 {
        return Err(FifthPassError::OutOfScope(
            "side +0x84 != 0 && +0x88 == 0 takes the calc_nbits_for_idwl_ch_init_at5 branch",
        ));
    }
    // The upgrade stage consumes `band_count * channel_count` order
    // entries at both channel modes; the stereo-merge stage reads
    // `stereo_order` only at channel_count == 2 (decompile 39473 inside
    // the 39469 gate), so require its length only there.
    if state.order.len() < state.band_count * channel_count
        || (channel_count == 2 && state.stereo_order.len() < state.stereo_bound)
    {
        return Err(FifthPassError::RowTooShort {
            needed: state.band_count * channel_count,
            actual: if channel_count == 2 {
                state.order.len().min(state.stereo_order.len())
            } else {
                state.order.len()
            },
        });
    }
    for channel in &state.channels {
        let needed = state.band_count;
        let actual = channel
            .word_lengths
            .len()
            .min(channel.selector_row.len())
            .min(channel.scale_factors.len())
            .min(channel.max_word_lengths.len())
            .min(channel.quant_bands.len());
        if actual < needed
            || channel.active_costs.len() < needed * FIFTH_CANDIDATES_AT5
            || channel.trial_costs.len() < needed * FIFTH_CANDIDATES_AT5
        {
            return Err(FifthPassError::RowTooShort { needed, actual });
        }
    }

    let nsps = nsps_at5();
    let mut pass = PassState {
        rows: state
            .channels
            .iter()
            .map(|channel| channel.word_lengths.clone())
            .collect(),
        selectors: state
            .channels
            .iter()
            .map(|channel| channel.selector_row.clone())
            .collect(),
        active_costs: state
            .channels
            .iter()
            .map(|channel| channel.active_costs.clone())
            .collect(),
        trial_costs: state
            .channels
            .iter()
            .map(|channel| channel.trial_costs.clone())
            .collect(),
        idct_blocks: state
            .channels
            .iter()
            .map(|channel| channel.idct_block.clone())
            .collect(),
        wlc_blocks: state
            .channels
            .iter()
            .map(|channel| channel.wlc_block.clone())
            .collect(),
        wlc_scratch: vec![IdwlBlockState::default(); channel_count],
        shared_side: state.shared_wlc_side.clone(),
        bits_46: state
            .channels
            .iter()
            .map(|channel| channel.quant_bits_46)
            .collect(),
        meta: state
            .channels
            .iter()
            .map(|channel| ChannelMeta {
                obj_mode: channel.obj_mode,
                quant_state: channel.quant_state,
                fixbits_index: channel.fixbits_index,
                context_kind: channel.context_kind,
                word_count: channel.word_count,
                group_count: channel.group_count,
                max_word_lengths: channel.max_word_lengths,
                quant_bands: channel.quant_bands,
                scale_factors: channel.scale_factors,
            })
            .collect(),
        band_count: state.band_count,
        active_band_count: state.active_band_count,
        budget: state.budget_limit,
        mode_1c: state.mode_1c,
        candidate_count: state.quant_candidate_count,
        // `+0x84 == 0` selects the WLC-reset prong at every trial site.
        wlc_reset: state.side_gate_84 == 0,
        bits_11a: state.idwl_bits_11a,
        bits_11e: state.idct_bits_11e,
        bits_12a: state.base_total_12a,
        bits_12e: state.current_bits_12e,
        trials: 0,
        accepts: 0,
    };

    // Entry stage: adopt the selector rows as IDCT aux and re-cost.
    for index in 0..channel_count {
        for band in 0..FIFTH_BANDS_AT5 {
            pass.idct_blocks[index].aux[band] = pass.selectors[index][band] as u32;
        }
    }
    let obj_modes: Vec<u32> = pass.meta.iter().map(|meta| meta.obj_mode).collect();
    let fixbits_indices: Vec<usize> = pass.meta.iter().map(|meta| meta.fixbits_index).collect();
    let recost = live_idct_bits(
        &obj_modes,
        &fixbits_indices,
        &pass.rows,
        pass.active_band_count,
        &mut pass.idct_blocks,
    )? as i16;
    let rebase = pass.bits_11e.wrapping_sub(recost);
    pass.bits_12e = pass.bits_12e.wrapping_sub(rebase);
    pass.bits_12a = pass.bits_12a.wrapping_sub(rebase);
    pass.bits_11e = recost;

    // Stereo merge stage — channel_count == 2 only (decompile 39469;
    // its sole read of param_7 is at 39473 inside that gate). At mono
    // the native else-arm (39590–39592) just reloads the `+0x90`
    // threshold and falls through to the upgrade stage.
    let threshold_90 = state.threshold_90;
    if channel_count == 2
        && i32::from(pass.bits_12e) <= pass.budget.wrapping_sub(threshold_90)
        && state.stereo_bound > 0
    {
        for index in 0..state.stereo_bound {
            let band = state.stereo_order[index] as usize;
            if band >= pass.band_count {
                continue;
            }
            let left = pass.rows[0][band];
            let right = pass.rows[1][band];
            if left == right || left == 0 || right == 0 {
                continue;
            }
            let sf_delta =
                (pass.meta[0].scale_factors[band] - pass.meta[1].scale_factors[band]).abs();
            if sf_delta >= 0xb {
                continue;
            }
            let mut threshold = i32::from(nsps[band] >> 4);
            if threshold < threshold_90 {
                threshold = threshold_90;
            }
            if i32::from(pass.bits_12e) > pass.budget.wrapping_sub(threshold) {
                continue;
            }
            let target = usize::from(right < left);
            let old_wl = pass.rows[target][band];
            if old_wl >= 7 {
                continue;
            }
            pass.rows[target][band] = old_wl + 1;
            pass.trial(target, band, old_wl)?;
        }
    }

    // Upgrade stage: seven rounds over the band order.
    if i32::from(pass.bits_12e) <= pass.budget.wrapping_sub(threshold_90) {
        let mut eligible = vec![vec![1i32; pass.band_count]; channel_count];
        for _round in 0..7 {
            for index in 0..pass.band_count * channel_count {
                let entry = state.order[index] as usize;
                let (channel, band) = if entry < pass.band_count {
                    (0usize, entry)
                } else {
                    (1usize, entry - pass.band_count)
                };
                let mut threshold = i32::from(nsps[band] >> 4);
                if threshold < threshold_90 {
                    threshold = threshold_90;
                }
                if i32::from(pass.bits_12e) > pass.budget.wrapping_sub(threshold) {
                    continue;
                }
                if eligible[channel][band] != 1 {
                    continue;
                }
                let wl = pass.rows[channel][band];
                if wl < 1
                    || i32::from(pass.meta[channel].max_word_lengths[band]) <= wl
                    || pass.meta[channel].quant_bands[band].idsf as i32 > 0xe
                {
                    continue;
                }
                pass.rows[channel][band] = wl + 1;
                if !pass.trial(channel, band, wl)? {
                    eligible[channel][band] = 0;
                }
            }
        }
    }

    let channels = (0..channel_count)
        .map(|index| FifthChannelOutput {
            word_lengths: pass.rows[index].clone(),
            selector_row: pass.selectors[index].clone(),
            active_costs: pass.active_costs[index].clone(),
            quant_bits_46: pass.bits_46[index],
            idct_block: pass.idct_blocks[index].clone(),
            wlc_block: pass.wlc_blocks[index].clone(),
        })
        .collect();
    Ok(FifthFrameOutput {
        channels,
        shared_wlc_side: pass.shared_side,
        idwl_bits_11a: pass.bits_11a,
        idct_bits_11e: pass.bits_11e,
        base_total_12a: pass.bits_12a,
        extended_total_12e: pass.bits_12e,
        return_bits: pass.bits_12e,
        trials: pass.trials,
        accepts: pass.accepts,
    })
}
