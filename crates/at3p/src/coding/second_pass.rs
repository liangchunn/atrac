//! Scoped composition of `second_bit_allocation_at5` (native 0x47ed0)
//! ATRAC3plus 352 kbps (channel count 2, selector 0x1e).
//!
//! Native source of truth is the decompiled boundary at
//! `0x00057ed0..0x00058775` plus `second_io_trace.ndjson`. The scoped
//! wiring, per iteration of the candidate-step search (at most 8):
//!
//! 1. Per channel, save the current `+0x1b5f8` word-length row, build
//!    candidate weights from the base weights at block `+0x1cc`
//!    (`sa_pcfx` for positive steps, the selected MCFX table
//!    otherwise; the 48 kHz tail overwrite never runs at 44.1 kHz),
//!    round/clamp against the `+0x02` max row, and zero bands whose
//!    `+0x14c` activity copy is zero.
//! 2. Mark dirty bands (first iteration: live band with a positive
//!    `+0xcc` word or changed row; later iterations: changed rows
//!    only). Dirty bands re-run `quant_nontone_nspecs_at5` and the
//!    earliest-strict-minimum candidate scan (new pick into the
//!    `+0xb08` row, new cost row at `+0xb88`); clean bands with a
//!    positive row reuse the cost at their current pick. The sum
//!    lands in the channel `+0x46` slot.
//! 3. Store `+0x12a` plus the channel sums to `+0x12e` and consult
//!    the native stop predicate (in-window, positive step cap with
//!    met budget, or negative step cap), else update the step
//!    direction with reversal half-stepping.
//!
//! Epilogue: the traced 352 kbps path always enters with side
//! `+0x84 != 0` and `+0x88 == 0`, so per channel the new IDWL seed is
//! `2 * channels + sum(calc_nbits_for_idwl_ch_init_at5)` and
//! `+0x88` is set to 1; the seed delta is applied to `+0x11a`,
//! `+0x12a`, and the returned `+0x12e`. The `+0x84 == 0` WLC state
//! reset branch and the `+0x88 != 0` `calc_nbits_for_idwl_ch_at5`
//! update branch are rejected instead of guessed.

use crate::coding::allocation::{
    AllocationError, SecondCandidateSearchAction, advance_second_candidate_search_at5,
    compute_second_candidate_weights_at5, initial_second_candidate_search_state_at5,
    mark_second_candidate_changes_at5, round_and_clamp_word_lengths_at5,
    second_positive_step_pcfx_at5, select_second_mcfx_at5, zero_inactive_word_lengths_at5,
};
use crate::coding::quant::QuantError;
use crate::coding::quant_cost::quant_nontone_costs_at5;
use crate::coding::zeroth_pass::{ZEROTH_BANDS_AT5, ZerothQuantBandRaw};

#[derive(Debug)]
pub enum SecondPassError {
    Allocation(AllocationError),
    Quant(QuantError),
    /// The input would take a native branch outside the scoped
    /// 352 kbps 44.1 kHz path.
    OutOfScope(&'static str),
}

impl From<AllocationError> for SecondPassError {
    fn from(error: AllocationError) -> Self {
        SecondPassError::Allocation(error)
    }
}

impl From<QuantError> for SecondPassError {
    fn from(error: QuantError) -> Self {
        SecondPassError::Quant(error)
    }
}

/// One channel's second-pass surface. The word-length, pick, and
/// pick-cost rows are carried state from the zeroth pass and are
/// updated in place like the native `+0x1b5f8`/`+0xb08`/`+0xb88`
/// rows.
#[derive(Debug)]
pub struct SecondChannelState<'a> {
    /// Base allocation weights at block `+0x1cc` (zeroth output; the
    /// candidate loop always rebuilds from these).
    pub base_weights: &'a [f32],
    /// Per-band maximum word lengths at block `+0x02`.
    pub max_word_lengths: &'a [i16],
    /// The `+0x14c` activity copy written by the zeroth pass.
    pub activity: &'a [i32],
    /// Per-band quant inputs (spectrum window, `+0xcc` idsf word,
    /// `+0x24c` scale, spec count). The `+0xcc` word doubles as the
    /// dirty-mask live-band test (`0 < *(block + 0xcc + band * 4)`).
    pub quant_bands: &'a [ZerothQuantBandRaw<'a>],
    /// Current `+0x1b5f8` word-length row.
    pub word_lengths: Vec<i32>,
    /// Current `+0xb08` pick row.
    pub picks: Vec<i32>,
    /// Cost of each band's current pick (`+0xb88` row at the pick
    /// index), reused for clean bands with a positive row.
    pub pick_costs: Vec<i16>,
    /// `calc_nbits_for_idwl_ch_init_at5` result (external boundary,
    /// evaluated after the search loop).
    pub idwl_init_bits: i32,
}

/// Frame-level second-pass inputs.
#[derive(Debug)]
pub struct SecondFrameState<'a> {
    pub channels: Vec<SecondChannelState<'a>>,
    /// `param_4`.
    pub band_count: usize,
    /// `param_5`.
    pub budget_limit: i32,
    /// `param_6`.
    pub selector: u32,
    /// Config byte at `+0x1dc`. `& 0x7c` selects the raise-weight seed
    /// (pcfx vs flat) and the fit tolerance (0.95 vs 0.5) — both ported.
    pub header_flags_1dc: u32,
    /// Config word at `+0xac`; the scoped path requires 44100.
    pub sample_rate: i32,
    /// `*(object + 0x1074)` and `sa_nencodetbls[encode selector]`.
    pub quant_state: usize,
    pub quant_candidate_count: usize,
    /// Side gates at `+0x84`/`+0x88` selecting the IDWL epilogue prong
    /// (decompile 39326–39383): `+0x84 == 0` takes the WLC-reset prong,
    /// else `+0x88 == 0` the init prong; `+0x88 != 0` (steady costing)
    /// stays fail-explicit.
    pub side_gate_84: u32,
    pub side_gate_88: u32,
    /// Config word `cfg+0xc4` (the WLC word-group count, 32 on the 352
    /// path) — the value the WLC-reset prong writes to record word [7]
    /// and charges 3 bits per group per channel (decompile 39335–39344;
    /// natively captured as `word_group_count_c4_u32 == 32` in
    /// `second_io_flag_path_trace.ndjson`).
    pub word_group_count_c4: i32,
    /// Entry side shorts `+0x11a`, `+0x12a`, and `+0x12e`.
    pub idwl_bits_11a: i16,
    pub base_total_12a: i16,
    pub current_bits_12e: i16,
}

#[derive(Debug)]
pub struct SecondChannelOutput {
    /// Final `+0x1b5f8` word-length row.
    pub word_lengths: Vec<i32>,
    /// Final `+0xb08` pick row.
    pub picks: Vec<i32>,
    /// Final cost at each band's pick.
    pub pick_costs: Vec<i16>,
    /// The channel `+0x46` slot total from the last iteration.
    pub quant_state_total: i16,
}

#[derive(Debug)]
pub struct SecondFrameOutput {
    pub channels: Vec<SecondChannelOutput>,
    /// Candidate-search iterations executed (1..=8).
    pub iterations: usize,
    /// Updated side words: `+0x11a`, `+0x12a`, and the returned
    /// `+0x12e`.
    pub idwl_bits_11a: i16,
    pub base_total_12a: i16,
    pub extended_total_12e: i16,
    pub return_bits: i16,
    /// The native store `*(side + 0x88) = 1` — fires only on the IDWL
    /// init prong (decompile 39357); the WLC-reset prong leaves `+0x88`
    /// untouched (captured exit gate88 == 0 in
    /// `second_io_flag_path_trace.ndjson`).
    pub side_gate_88_set: bool,
}

/// Run the scoped second pass. Word-length rows, pick rows, `+0x46`
/// totals, and the `+0x11a/+0x12a/+0x12e` updates mirror the native
/// stores.
pub fn second_bit_allocation_frame_at5(
    state: &mut SecondFrameState<'_>,
) -> Result<SecondFrameOutput, SecondPassError> {
    let channel_count = state.channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(AllocationError::UnsupportedChannelCount(channel_count).into());
    }
    // Config flag-word (`cfg+0x1dc & 0x7c`) alternate behaviors
    // (`second_bit_allocation_at5`, native 0x47ed0 / Ghidra 0x57ed0):
    //   (a) raise-direction weight seed (decompile 39074–39118): flag==0 seeds
    //       `sa_pcfx[i]*step + base[i]` (the ported pcfx path incl. the 48000
    //       sub-branch); flag!=0 seeds a flat `base[i] + step` — NO table
    //       scaling, NO 48000 sub-branch.
    //   (b) fit-loop tolerance (decompile 39270–39290): `0.95` when flag==0,
    //       `0.5` when flag!=0 (LAB_0005865d / LAB_00058784).
    // The lower-direction seed (decompile 39120+) is NOT flag-gated.
    let flags_nonzero = state.header_flags_1dc & 0x7c != 0;
    if state.sample_rate != 44100 {
        return Err(SecondPassError::OutOfScope(
            "non-44.1 kHz rates take the 48 kHz tail-weight overwrite",
        ));
    }
    // IDWL epilogue prong selection (native fork, decompile 39326–39383):
    // `+0x84 == 0` is checked FIRST and takes the WLC-reset prong (now
    // ported below — the flag-set calc entry clears `+0x84`); otherwise
    // `+0x88 == 0` takes the init prong (ported). The remaining steady
    // costing prong (`+0x84 != 0 && +0x88 != 0`,
    // `calc_nbits_for_idwl_ch_at5`, decompile 39368–39383) stays
    // fail-explicit.
    if state.side_gate_84 != 0 && state.side_gate_88 != 0 {
        return Err(SecondPassError::OutOfScope(
            "side +0x84 != 0 && +0x88 != 0 takes the calc_nbits_for_idwl_ch_at5 update branch",
        ));
    }
    for channel in &state.channels {
        if channel.word_lengths.len() < state.band_count
            || channel.picks.len() < state.band_count
            || channel.pick_costs.len() < state.band_count
            || channel.quant_bands.len() < state.band_count
        {
            return Err(AllocationError::WordLengthsTooShort {
                needed: state.band_count,
                actual: channel
                    .word_lengths
                    .len()
                    .min(channel.picks.len())
                    .min(channel.pick_costs.len())
                    .min(channel.quant_bands.len()),
            }
            .into());
        }
    }

    let mcfx = select_second_mcfx_at5(channel_count, state.selector)?;
    let pcfx = second_positive_step_pcfx_at5();

    let mut search = initial_second_candidate_search_state_at5(
        i32::from(state.current_bits_12e),
        state.budget_limit,
    );
    let mut totals = vec![0i16; channel_count];
    let mut extended: i16;
    let mut iterations = 0usize;
    loop {
        let first_iteration = iterations == 0;
        for (index, channel) in state.channels.iter_mut().enumerate() {
            let saved = channel.word_lengths.clone();
            // Raise direction with flag!=0 uses a flat `base[i] + step` seed
            // (native alternate, decompile 39108–39116): model it as a
            // uniform coefficient of 1.0 so `1.0*step + base[i]` == the flat
            // add, with no table scaling. flag==0 raise keeps pcfx; the lower
            // direction (`step <= 0`) always uses mcfx (not flag-gated).
            let flat_ones;
            let coefficients: &[f32] = if search.step > 0.0 {
                if flags_nonzero {
                    flat_ones = vec![1.0f32; ZEROTH_BANDS_AT5.max(state.band_count)];
                    &flat_ones
                } else {
                    &pcfx
                }
            } else {
                &mcfx.values
            };
            let mut weights = vec![0.0f32; ZEROTH_BANDS_AT5.max(state.band_count)];
            compute_second_candidate_weights_at5(
                channel.base_weights,
                search.step,
                coefficients,
                &mut weights,
                state.band_count,
            )?;
            let mut row = vec![0i32; ZEROTH_BANDS_AT5.max(state.band_count)];
            round_and_clamp_word_lengths_at5(
                &weights,
                channel.max_word_lengths,
                &mut row,
                state.band_count,
            )?;
            zero_inactive_word_lengths_at5(channel.activity, &mut row, state.band_count)?;
            row.truncate(channel.word_lengths.len());

            let nspecs: Vec<i32> = channel.quant_bands[..state.band_count]
                .iter()
                .map(|band| band.idsf as i32)
                .collect();
            let mut changed = vec![0i32; state.band_count];
            mark_second_candidate_changes_at5(
                &row,
                &saved,
                &nspecs,
                &mut changed,
                state.band_count,
                first_iteration,
            )?;

            let mut total: i16 = 0;
            for band in 0..state.band_count {
                if changed[band] == 0 {
                    if row[band] > 0 {
                        total = total.wrapping_add(channel.pick_costs[band]);
                    }
                    continue;
                }
                let raw = &channel.quant_bands[band];
                let costs = quant_nontone_costs_at5(
                    raw.spectrum,
                    row[band] as usize,
                    raw.idsf,
                    raw.scale,
                    raw.count,
                    state.quant_state,
                    state.quant_candidate_count,
                )?;
                let mut best = costs[0] as i16;
                let mut best_index = 0usize;
                for (candidate, cost) in costs
                    .iter()
                    .enumerate()
                    .take(state.quant_candidate_count.min(costs.len()))
                    .skip(1)
                {
                    if (*cost as i16) < best {
                        best = *cost as i16;
                        best_index = candidate;
                    }
                }
                total = total.wrapping_add(best);
                channel.picks[band] = best_index as i32;
                channel.pick_costs[band] = best;
            }
            channel.word_lengths = row;
            totals[index] = total;
        }

        extended = state.base_total_12a;
        for total in &totals {
            extended = extended.wrapping_add(*total);
        }
        iterations += 1;

        let update = advance_second_candidate_search_at5(
            search,
            i32::from(extended),
            state.budget_limit,
            flags_nonzero,
        );
        match update.action {
            SecondCandidateSearchAction::Stop | SecondCandidateSearchAction::Exhausted => break,
            SecondCandidateSearchAction::Continue => search = update.state,
        }
    }

    // Epilogue: the native `+0x84`/`+0x88` IDWL fork (decompile 39326–39383).
    // `local_28e` is re-read from the (loop-unchanged) `+0x12a`, so the delta
    // applies to the entry base total in every prong.
    let (idwl_total, side_gate_88_set) = if state.side_gate_84 == 0 {
        // WLC-reset prong (decompile 39326–39345), checked FIRST: per channel
        // native resets the `block+0x460` WLC record words — [0]=0, [5]=0,
        // [6]=0, [8]=0, [9]=0, [7]=`cfg[0xc4]` — and accumulates
        // `cfg[0xc4] * 3` idwl bits on the `param_3 * 2` base (stereo at
        // c4=32: 2*2 + 2*32*3 = 196, matching the captured
        // `second_io_flag_path_trace.ndjson` returns, entry/exit `+0x11a` both
        // 196). No IDWL leaf calls and NO `*(side+0x88) = 1` store (captured
        // exit gate88 == 0). This is the prong the flag-set calc entry
        // (`cfg+0x1dc & 0x7c` → `shared+0x84 = 0`) drives. The record-word
        // resets themselves live in the caller's WLC block state (the
        // composed calc rebuilds its fifth-seed blocks from rows; a future
        // consumer of the reset words needs its own native evidence).
        (
            channel_count as i32 * 2 + channel_count as i32 * state.word_group_count_c4 * 3,
            false,
        )
    } else {
        // Init prong (`+0x88 == 0`, decompile 39346–39357): the per-channel
        // `calc_nbits_for_idwl_ch_init_at5` bits, then `*(side+0x88) = 1`.
        let mut total = channel_count as i32 * 2;
        for channel in &state.channels {
            total = total.wrapping_add(channel.idwl_init_bits);
        }
        (total, true)
    };
    let delta = state.idwl_bits_11a.wrapping_sub(idwl_total as i16);
    let new_12e = extended.wrapping_sub(delta);

    let channels = state
        .channels
        .iter()
        .enumerate()
        .map(|(index, channel)| SecondChannelOutput {
            word_lengths: channel.word_lengths.clone(),
            picks: channel.picks.clone(),
            pick_costs: channel.pick_costs.clone(),
            quant_state_total: totals[index],
        })
        .collect();

    Ok(SecondFrameOutput {
        channels,
        iterations,
        idwl_bits_11a: idwl_total as i16,
        base_total_12a: state.base_total_12a.wrapping_sub(delta),
        extended_total_12e: new_12e,
        return_bits: new_12e,
        side_gate_88_set,
    })
}
