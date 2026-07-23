use std::cmp::Ordering;

use crate::dsp::fft::{FftError, dft_v_at5};
use crate::dsp::gain::{
    GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS, GC_SET_POINTS_OUTPUT_RECORD_WORDS, GainPassError,
    GcSetPointWords, gc_set_points_at5,
};
use crate::tables::at5::{half_hannwin_at5, ip016_at5_ref, lngain_at5, sc016_at5_ref};

pub const GAIN_DETECT_SLOTS: usize = 4;
pub const GAIN_DETECT_STATE_CHANNELS: usize = 2;
pub const GAIN_DETECT_BANDS: usize = 16;
pub const GAIN_DETECT_HISTORY_PEAK_VALUES: usize = 64;
pub const GAIN_DETECT_RECORD_WORDS: usize = 38;
pub const GAIN_DETECT_POINT_WORDS: usize = 15;
pub const GAIN_DETECT_POINTS: usize = 7;
pub const GAIN_DETECT_EMIT_RECORD_WORDS: usize = 12;
pub const GAIN_DETECT_CANDIDATE_POOL_WORDS: usize = 0x600;
pub const GAIN_DETECT_CANDIDATE_BOUNDS_WORDS: usize = 8;
pub const GAIN_DETECT_MAX_LOCATION: usize = 63;
pub const GAIN_DETECT_MAX_LEVEL_ID: usize = 15;
pub const GAIN_DETECT_PEAK_BINS: usize = 32;
pub const GAIN_DETECT_PRIMARY_HISTORY_PEAK_START: usize = GAIN_DETECT_PEAK_BINS;
pub const GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START: usize = 31;
pub const GAIN_DETECT_PEAK_GROUP_VALUES: usize = 4;
pub const GAIN_DETECT_PEAK_INPUT_VALUES: usize =
    GAIN_DETECT_PEAK_BINS * GAIN_DETECT_PEAK_GROUP_VALUES;
pub const GAIN_DETECT_ACTIVITY_INITIAL_FLAGS: usize = 3;
pub const GAIN_DETECT_ACTIVITY_FLAGS: usize =
    GAIN_DETECT_ACTIVITY_INITIAL_FLAGS + GAIN_DETECT_PEAK_BINS;
pub const GAIN_DETECT_ACTIVITY_INPUT_VALUES: usize =
    GAIN_DETECT_ACTIVITY_FLAGS * GAIN_DETECT_PEAK_GROUP_VALUES;
pub const GAIN_DETECT_WEIGHT_BINS: usize = 8;
pub const GAIN_DETECT_WEIGHT_WINDOW_VALUES: usize = 16;
pub const GAIN_DETECT_WEIGHT_SOURCE_START: usize = (0x600 + 0x1d0) / 4;
pub const GAIN_DETECT_WEIGHT_SOURCE_STRIDE_VALUES: usize = GAIN_DETECT_PEAK_GROUP_VALUES;
pub const GAIN_DETECT_WEIGHT_SOURCE_INPUT_VALUES: usize = GAIN_DETECT_WEIGHT_SOURCE_START
    + (GAIN_DETECT_PEAK_BINS - 1) * GAIN_DETECT_WEIGHT_SOURCE_STRIDE_VALUES
    + GAIN_DETECT_WEIGHT_WINDOW_VALUES;
const GAIN_DETECT_WEIGHT_EPSILON: f32 = 1.0e-8;
const GAIN_DETECT_WEIGHT_AVERAGE_SCALE: f32 = 0.142_857_15;
const GAIN_DETECT_WEIGHT_LOG_SCALE: f32 = 0.180_336_88;
const GAIN_DETECT_LEVEL_LOG2_E: f64 = 1.442_695_021_629_333_5;
const GAIN_DETECT_WEIGHT_DFT_OUTPUT_VALUES: usize = GAIN_DETECT_WEIGHT_BINS + 1;
const GAIN_DETECT_CANDIDATE_POINTER_WORDS: [usize; 3] = [2, 3, 4];
const GAIN_DETECT_CANDIDATE_POOL_BYTES: u32 = (GAIN_DETECT_CANDIDATE_POOL_WORDS * 4) as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectActivityFlags {
    quad_flags: [i32; GAIN_DETECT_ACTIVITY_FLAGS],
}

impl GainDetectActivityFlags {
    pub fn from_native_quad_flags(
        quad_flags: [i32; GAIN_DETECT_ACTIVITY_FLAGS],
    ) -> Result<Self, SigprocError> {
        for flag in quad_flags {
            checked_nonnegative_index("gain_detect_activity_flag", flag, 1)?;
        }
        Ok(Self { quad_flags })
    }

    pub fn quad_flags(&self) -> &[i32; GAIN_DETECT_ACTIVITY_FLAGS] {
        &self.quad_flags
    }

    pub fn should_run_weight(&self, bin_index: usize) -> Result<bool, SigprocError> {
        check_index(
            "gain_detect_weight_bin",
            bin_index,
            GAIN_DETECT_PEAK_BINS - 1,
        )?;
        Ok(self.quad_flags[bin_index..bin_index + 4]
            .iter()
            .any(|flag| *flag != 0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainDetectPeakBins {
    bins: [f32; GAIN_DETECT_PEAK_BINS],
    max_index: usize,
    max_value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectPeakSpan {
    slots: [usize; GAIN_DETECT_HISTORY_PEAK_VALUES],
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainDetectWeight {
    weight: f32,
    accepted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectBandStateFields {
    pub prev_max_slot: usize,
    pub prev_peak_slot: usize,
    pub prev_level_a_bits: u32,
    pub prev_level_b_bits: u32,
    pub gain_records_total: usize,
    pub gain_records_removed: usize,
    pub list_count_primary: i32,
    pub list_count_secondary: i32,
    pub active_chain_count: i32,
    pub stereo_energy_a_bits: u32,
    pub stereo_energy_b_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectBandStateWritebackFields {
    pub prev_peak_slot_plus_32: usize,
    pub current_peak_slot: usize,
    pub previous_level_bits: u32,
    pub current_peak_value_bits: u32,
    pub gain_records_total: usize,
    pub gain_records_removed: usize,
    pub list_count_primary: i32,
    pub list_count_secondary: i32,
    pub active_chain_count: i32,
    pub stereo_energy_a_bits: u32,
    pub stereo_energy_b_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GainDetectWritebackCopies {
    history_primary_words: [u32; GAIN_DETECT_HISTORY_PEAK_VALUES],
    history_secondary_words: [u32; GAIN_DETECT_HISTORY_PEAK_VALUES],
    candidate_pool_words: [u32; GAIN_DETECT_CANDIDATE_POOL_WORDS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectCandidatePoolStackBase {
    absolute_addr: u32,
    stack_offset: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectEmitCandidate {
    words: [i32; GAIN_DETECT_EMIT_RECORD_WORDS],
    next_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectCandidateListRecord {
    words: [i32; GAIN_DETECT_EMIT_RECORD_WORDS],
    next_index: Option<usize>,
    previous_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GainDetectCandidateListBounds {
    head_index: Option<usize>,
    tail_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GainDetectOverSevenPruneResult {
    pub iterations_run: usize,
    pub duplicate_count: usize,
    pub removed_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainDetectCandidatePrepScalars {
    active_span_count: usize,
    branch_flag_a: i32,
    branch_flag_b: i32,
    prep_peak: f32,
    level_a: i32,
    level_b: i32,
    lower_bound: i32,
    upper_bound: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainDetectCandidateSide {
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectCandidateInterval {
    distance_word: i32,
    lower: i32,
    upper: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectCandidateLoopCursor {
    emitted_count: usize,
    remaining_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectCandidateSourceCall {
    source_record_index: usize,
    side: GainDetectCandidateSide,
    source_words: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    source_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GainDetectCandidateSourceQueue {
    records: [([u32; GAIN_DETECT_EMIT_RECORD_WORDS], i32); GAIN_DETECT_HISTORY_PEAK_VALUES],
    record_count: usize,
    cursor_index: usize,
    next_side: GainDetectCandidateSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectCandidateLoopCall {
    source_record_index: usize,
    side: GainDetectCandidateSide,
    source_words: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    source_index: i32,
    destination_words_before: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    destination_words_after: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    destination_index: i32,
    gc_set_result: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectEmitLevelBounds {
    min_level: i32,
    max_level: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GainDetectBandState {
    fields: GainDetectBandStateFields,
}

impl Default for GainDetectBandStateFields {
    fn default() -> Self {
        Self {
            prev_max_slot: 0,
            prev_peak_slot: 0,
            prev_level_a_bits: 0,
            prev_level_b_bits: 0,
            gain_records_total: 0,
            gain_records_removed: 0,
            list_count_primary: 0,
            list_count_secondary: 0,
            active_chain_count: 0,
            stereo_energy_a_bits: 0,
            stereo_energy_b_bits: 0,
        }
    }
}

impl GainDetectWeight {
    pub fn weight(&self) -> f32 {
        self.weight
    }

    pub fn accepted(&self) -> bool {
        self.accepted
    }
}

impl GainDetectBandState {
    pub fn from_native_fields(fields: GainDetectBandStateFields) -> Result<Self, SigprocError> {
        check_index(
            "gain_detect_prev_max_slot",
            fields.prev_max_slot,
            GAIN_DETECT_MAX_LOCATION,
        )?;
        check_index(
            "gain_detect_prev_peak_slot",
            fields.prev_peak_slot,
            GAIN_DETECT_PEAK_BINS - 1,
        )?;
        check_index(
            "gain_detect_records_total",
            fields.gain_records_total,
            GAIN_DETECT_PEAK_BINS,
        )?;
        check_index(
            "gain_detect_records_removed",
            fields.gain_records_removed,
            GAIN_DETECT_PEAK_BINS,
        )?;
        checked_nonnegative_index(
            "gain_detect_list_count_primary",
            fields.list_count_primary,
            GAIN_DETECT_PEAK_BINS,
        )?;
        checked_nonnegative_index(
            "gain_detect_list_count_secondary",
            fields.list_count_secondary,
            GAIN_DETECT_PEAK_BINS,
        )?;
        checked_nonnegative_index(
            "gain_detect_active_chain_count",
            fields.active_chain_count,
            GAIN_DETECT_PEAK_BINS,
        )?;

        Ok(Self { fields })
    }

    pub fn fields(&self) -> GainDetectBandStateFields {
        self.fields
    }

    pub fn prev_max_slot(&self) -> usize {
        self.fields.prev_max_slot
    }

    pub fn prev_peak_slot(&self) -> usize {
        self.fields.prev_peak_slot
    }

    pub fn prev_level_a_bits(&self) -> u32 {
        self.fields.prev_level_a_bits
    }

    pub fn prev_level_b_bits(&self) -> u32 {
        self.fields.prev_level_b_bits
    }

    pub fn gain_records_total(&self) -> usize {
        self.fields.gain_records_total
    }

    pub fn gain_records_removed(&self) -> usize {
        self.fields.gain_records_removed
    }

    pub fn list_count_primary(&self) -> i32 {
        self.fields.list_count_primary
    }

    pub fn list_count_secondary(&self) -> i32 {
        self.fields.list_count_secondary
    }

    pub fn active_chain_count(&self) -> i32 {
        self.fields.active_chain_count
    }

    pub fn stereo_energy_a_bits(&self) -> u32 {
        self.fields.stereo_energy_a_bits
    }

    pub fn stereo_energy_b_bits(&self) -> u32 {
        self.fields.stereo_energy_b_bits
    }
}

pub fn gain_detect_band_state_writeback_at5(
    fields: GainDetectBandStateWritebackFields,
) -> Result<GainDetectBandState, SigprocError> {
    check_index(
        "gain_detect_prev_peak_slot_plus_32",
        fields.prev_peak_slot_plus_32,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;
    let prev_max_slot = fields
        .prev_peak_slot_plus_32
        .checked_sub(GAIN_DETECT_PEAK_BINS)
        .ok_or(SigprocError::IndexOutOfRange {
            name: "gain_detect_prev_peak_slot_plus_32",
            value: fields.prev_peak_slot_plus_32,
            max: GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
        })?;

    GainDetectBandState::from_native_fields(GainDetectBandStateFields {
        prev_max_slot,
        prev_peak_slot: fields.current_peak_slot,
        prev_level_a_bits: fields.previous_level_bits,
        prev_level_b_bits: fields.current_peak_value_bits,
        gain_records_total: fields.gain_records_total,
        gain_records_removed: fields.gain_records_removed,
        list_count_primary: fields.list_count_primary,
        list_count_secondary: fields.list_count_secondary,
        active_chain_count: fields.active_chain_count,
        stereo_energy_a_bits: fields.stereo_energy_a_bits,
        stereo_energy_b_bits: fields.stereo_energy_b_bits,
    })
}

/// Native duplicate-location count over a sorted candidate list
/// (`detect_gainc_data_new_at5` at `0x3b28a..`, verified by
/// `gain_detect_glue_trace.ndjson`): walk the linked chain from the list
/// head and count adjacent records with equal location word 0. The count
/// feeds the prune gate at `ebp-0x56c` word 1/2.
pub fn gain_detect_duplicate_location_count_at5(
    records: &[GainDetectCandidateListRecord],
    head_index: Option<usize>,
) -> Result<usize, SigprocError> {
    let mut duplicates = 0usize;
    let mut previous_location = None;
    let mut cursor = head_index;
    let mut visited = 0usize;
    while let Some(index) = cursor {
        let record = records.get(index).ok_or(SigprocError::IndexOutOfRange {
            name: "gain_detect_chain_index",
            value: index,
            max: records.len().saturating_sub(1),
        })?;
        if previous_location == Some(record.words[0]) {
            duplicates += 1;
        }
        previous_location = Some(record.words[0]);
        cursor = record.next_index;
        visited += 1;
        if visited > records.len() {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_chain_walk",
                value: visited,
                max: records.len(),
            });
        }
    }
    Ok(duplicates)
}

/// Native 32-bin level totals (`detect_gainc_data_new_at5` at
/// `0x3b28a..0x3b2ae`, totals at `ebp-0x3718`): walk the linked chain from
/// the list head accumulating `totals[word0] += word1`.
pub fn gain_detect_level_totals_at5(
    records: &[GainDetectCandidateListRecord],
    head_index: Option<usize>,
) -> Result<[i32; GAIN_DETECT_PEAK_BINS], SigprocError> {
    let mut totals = [0i32; GAIN_DETECT_PEAK_BINS];
    let mut cursor = head_index;
    let mut visited = 0usize;
    while let Some(index) = cursor {
        let record = records.get(index).ok_or(SigprocError::IndexOutOfRange {
            name: "gain_detect_chain_index",
            value: index,
            max: records.len().saturating_sub(1),
        })?;
        let location = record.words[0];
        let slot = checked_nonnegative_index(
            "gain_detect_total_location",
            location,
            GAIN_DETECT_PEAK_BINS - 1,
        )?;
        totals[slot] += record.words[1];
        cursor = record.next_index;
        visited += 1;
        if visited > records.len() {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_chain_walk",
                value: visited,
                max: records.len(),
            });
        }
    }
    Ok(totals)
}

/// Native `detect_gainc_data_new_at5` prune gate (decompiled
/// `if (7 < (local_548 - local_56c[1]) - local_55c[1])` before the pruning
/// loop at native `0x3b2ee`): prune only when the current call's group-0
/// record count minus the freshly counted duplicate-location count minus the
/// loop-local removal count exceeds 7. The 288 `initial_gate` rows in
/// `gain_detect_state_trace.ndjson` prove the third pointer word is 0 at
/// entry for every call/channel/band and retire the older inference that
/// subtracted the carried group-0 preseed count here.
pub fn gain_detect_prune_gate_at5(
    gc_record_count: usize,
    duplicate_location_count: usize,
    fresh_removed_count: usize,
) -> bool {
    (gc_record_count as i32 - duplicate_location_count as i32) - fresh_removed_count as i32 > 7
}

/// Port of the live over-seven prune loop in `detect_gainc_data_new_at5`
/// (native `0x3b2ee..0x3b5c3`, decompile 32035..32468). Each iteration scans
/// the working chain for the best merge (or, when none beats the threshold, a
/// removal) and applies it, then re-derives the duplicate count / level totals
/// and re-tests the gate.
///
/// Pinned by `gain_detect_state_trace.ndjson` (all 84 calls, 2688 initial_gate
/// rows, six gate-pass events; env `ATRAC_GAIN_DETECT_STATE_PRUNE_MAX_CALL=83`,
/// snapshots at calls 6/7/8/44). Five events take the merge branch (calls 6,
/// 15, 44, 73/ch0-b5, 73/ch1-b13), each one iteration; one takes the removal
/// branch (call 73 ch0 band13), also one iteration. The back-edge `remaining`
/// values are 7/7/7/7/6/7 in that call order. See
/// `gain_detect_best_over_seven_choice_at5` for the disproved sign/direction
/// inferences and the `local_296c` two-pool threshold caveat.
pub fn gain_detect_over_seven_prune_at5(
    records: &mut [GainDetectCandidateListRecord],
    bounds: &mut GainDetectCandidateListBounds,
    duplicate_count: usize,
    level_totals: &mut [i32; GAIN_DETECT_PEAK_BINS],
) -> Result<GainDetectOverSevenPruneResult, GainDetectCandidateLoopError> {
    // 2.1a state-trace replay) only ever exercise pool 1 (`local_1d6c`) with
    // same-pool partners (word[8] == 0): pools 0 and 2 are empty, no carried
    // pool-2 removal seeds the gate. The three-pool driver reproduces the
    // observed single-pool merges/removals bit-identically.
    let mut pool0: Vec<GainDetectCandidateListRecord> = Vec::new();
    let mut pool1: Vec<GainDetectCandidateListRecord> = records.to_vec();
    let mut pool2: Vec<GainDetectCandidateListRecord> = Vec::new();
    let (result, final_totals) = {
        let mut pools = GainDetectPrunePools {
            pools: [&mut pool0, &mut pool1, &mut pool2],
            removed: [0, 0, 0],
            duplicates: [0, duplicate_count as i32, 0],
            level_totals: *level_totals,
        };
        let result = gain_detect_over_seven_prune_three_pool_at5(&mut pools)?;
        (result, pools.level_totals)
    };

    // Reflect pool 1's final records / level totals back to the caller's slice.
    records.copy_from_slice(&pool1);
    *level_totals = final_totals;
    *bounds = gain_detect_insert_candidate_list_at5(records)?;
    Ok(result)
}

/// Three-pool over-seven prune loop (native `0x3b2ee..0x3b5c3`, decompile
/// 32035..32469). The working chain and merge/removal decisions run over pool 1
/// (`local_1d6c`); pools 0 and 2 are partner targets only. All bookkeeping is
/// native-exact **incremental**: the merge branch increments the pool-1 dup
/// counter (decompile 32381) and moves a record's word[1] between level-total
/// bins (decompile 32261..32360); the removal branch adjusts per-pool
/// removed/dup counters and subtracts partner/node word[1]s from the shared
/// totals (decompile 32388..32446). Nothing is re-derived from scratch mid-loop
/// — a rebuild would erase the cross-pool total subtractions native leaves in
/// place.
///
/// The gate operand is `(pool1_count - dup[1]) - removed[1]` (decompile 32035 /
/// back edge 32469), where `removed[1]` starts at the carried pool-2 removal
/// count seeded by the caller.
pub fn gain_detect_over_seven_prune_three_pool_at5(
    pools: &mut GainDetectPrunePools,
) -> Result<GainDetectOverSevenPruneResult, GainDetectCandidateLoopError> {
    let [pool0, pool1, pool2] = &mut pools.pools;
    gain_detect_over_seven_prune_slices_at5(
        [&mut pool0[..], &mut pool1[..], &mut pool2[..]],
        &mut pools.removed,
        &mut pools.duplicates,
        &mut pools.level_totals,
    )
}

fn gain_detect_over_seven_prune_slices_at5(
    mut pools: [&mut [GainDetectCandidateListRecord]; GAIN_DETECT_PRUNE_POOLS],
    removed: &mut [i32; GAIN_DETECT_PRUNE_POOLS],
    duplicates: &mut [i32; GAIN_DETECT_PRUNE_POOLS],
    level_totals: &mut [i32; GAIN_DETECT_PEAK_BINS],
) -> Result<GainDetectOverSevenPruneResult, GainDetectCandidateLoopError> {
    let mut iterations_run = 0usize;

    loop {
        let pool1_count = pools[1].len() as i32;
        let remaining = (pool1_count - duplicates[1]) - removed[1];
        if remaining <= 7 {
            break;
        }

        let choice = {
            let totals = *level_totals;
            let Some(choice) = gain_detect_best_over_seven_choice_at5(pools[1], &totals)? else {
                // The gate passed with no active pool-1 record, which no
                // observed call reaches.
                return Err(GainDetectCandidateLoopError::PruneLoopUnsupported {
                    gate_operand: remaining,
                });
            };
            choice
        };

        match choice {
            GainDetectOverSevenChoice::Merge(candidate) => {
                let mut totals = *level_totals;
                gain_detect_apply_over_seven_merge_at5(pools[1], candidate, &mut totals)?;
                *level_totals = totals;
                // Native `local_56c[1] += 1` (decompile 32381): the merge
                // collapses two records onto one location, adding a duplicate.
                duplicates[1] += 1;
            }
            GainDetectOverSevenChoice::Remove { removed_index } => {
                let increments = gain_detect_apply_over_seven_removal_slices_at5(
                    &mut pools,
                    duplicates,
                    level_totals,
                    removed_index,
                )?;
                for (counter, delta) in removed.iter_mut().zip(increments) {
                    *counter += delta;
                }
            }
        }
        iterations_run += 1;
        if iterations_run > GAIN_DETECT_PRUNE_POOL_CAPACITY {
            return Err(GainDetectCandidateLoopError::PruneLoopUnsupported {
                gate_operand: remaining,
            });
        }
    }

    Ok(GainDetectOverSevenPruneResult {
        iterations_run,
        duplicate_count: duplicates[1].max(0) as usize,
        removed_count: removed[1].max(0) as usize,
    })
}

/// Apply a single over-seven prune removal (node in pool 1 at `removed_index`,
/// plus its relative partner) to the three pools, returning the per-pool
/// removed increments `[pool0, pool1, pool2]`. Exposed for the
/// `removal_partner_events.ndjson` replay oracle (docs/12 §2.2), which pins the
/// removal branch's observable post effects per event without re-running the
/// merge scan. The shipping path drives this from
/// `gain_detect_over_seven_prune_three_pool_at5`.
pub fn gain_detect_apply_over_seven_removal_replay_at5(
    pools: &mut GainDetectPrunePools,
    removed_index: usize,
) -> Result<[i32; GAIN_DETECT_PRUNE_POOLS], GainDetectCandidateLoopError> {
    let [pool0, pool1, pool2] = &mut pools.pools;
    gain_detect_apply_over_seven_removal_slices_at5(
        &mut [&mut pool0[..], &mut pool1[..], &mut pool2[..]],
        &mut pools.duplicates,
        &mut pools.level_totals,
        removed_index,
    )
}

pub fn gain_detect_stereo_update_gate_at5(
    channel_count: usize,
    band_index: usize,
    stereo_start_band: usize,
) -> Result<bool, SigprocError> {
    check_index(
        "gain_detect_channel_count",
        channel_count,
        GAIN_DETECT_STATE_CHANNELS,
    )?;
    check_index("gain_detect_band", band_index, GAIN_DETECT_BANDS - 1)?;
    check_index(
        "gain_detect_stereo_start_band",
        stereo_start_band,
        GAIN_DETECT_BANDS,
    )?;

    Ok(channel_count == GAIN_DETECT_STATE_CHANNELS && band_index >= stereo_start_band)
}

pub fn gain_detect_writeback_copies_at5(
    history_primary_words: &[u32],
    history_secondary_words: &[u32],
    candidate_pool_words: &[u32],
) -> Result<GainDetectWritebackCopies, SigprocError> {
    check_storage(
        "gain_detect_history_primary",
        history_primary_words.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_storage(
        "gain_detect_history_secondary",
        history_secondary_words.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_storage(
        "gain_detect_candidate_pool",
        candidate_pool_words.len(),
        GAIN_DETECT_CANDIDATE_POOL_WORDS,
    )?;

    let mut copies = GainDetectWritebackCopies {
        history_primary_words: [0; GAIN_DETECT_HISTORY_PEAK_VALUES],
        history_secondary_words: [0; GAIN_DETECT_HISTORY_PEAK_VALUES],
        candidate_pool_words: [0; GAIN_DETECT_CANDIDATE_POOL_WORDS],
    };
    copies
        .history_primary_words
        .copy_from_slice(&history_primary_words[..GAIN_DETECT_HISTORY_PEAK_VALUES]);
    copies
        .history_secondary_words
        .copy_from_slice(&history_secondary_words[..GAIN_DETECT_HISTORY_PEAK_VALUES]);
    copies
        .candidate_pool_words
        .copy_from_slice(&candidate_pool_words[..GAIN_DETECT_CANDIDATE_POOL_WORDS]);

    Ok(copies)
}

pub fn gain_detect_normalize_candidate_pool_pointers_at5(
    candidate_pool_words: &[u32],
    source_base: GainDetectCandidatePoolStackBase,
) -> Result<[u32; GAIN_DETECT_CANDIDATE_POOL_WORDS], SigprocError> {
    check_storage(
        "gain_detect_candidate_pool",
        candidate_pool_words.len(),
        GAIN_DETECT_CANDIDATE_POOL_WORDS,
    )?;

    let mut normalized = [0; GAIN_DETECT_CANDIDATE_POOL_WORDS];
    normalized.copy_from_slice(&candidate_pool_words[..GAIN_DETECT_CANDIDATE_POOL_WORDS]);
    for record in normalized.chunks_exact_mut(GAIN_DETECT_EMIT_RECORD_WORDS) {
        for pointer_word in GAIN_DETECT_CANDIDATE_POINTER_WORDS {
            if let Some(byte_offset) = source_base.pool_pointer_byte_offset(record[pointer_word]) {
                record[pointer_word] =
                    (source_base.stack_offset as i64 + byte_offset as i64) as i32 as u32;
            }
        }
    }

    Ok(normalized)
}

pub fn gain_detect_secondary_history_at5(
    seed_words: &[u32],
    weight_words: &[u32],
) -> Result<[u32; GAIN_DETECT_HISTORY_PEAK_VALUES], SigprocError> {
    check_storage(
        "gain_detect_secondary_history_seed",
        seed_words.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_storage(
        "gain_detect_secondary_history_weights",
        weight_words.len(),
        GAIN_DETECT_PEAK_BINS,
    )?;

    let mut history = [0; GAIN_DETECT_HISTORY_PEAK_VALUES];
    history.copy_from_slice(&seed_words[..GAIN_DETECT_HISTORY_PEAK_VALUES]);
    history[GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START
        ..GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START + GAIN_DETECT_PEAK_BINS]
        .copy_from_slice(&weight_words[..GAIN_DETECT_PEAK_BINS]);
    Ok(history)
}

pub fn gain_detect_secondary_history_shift_at5(
    previous_history_words: &[u32],
    weight_words: &[u32],
    trailing_word: u32,
) -> Result<[u32; GAIN_DETECT_HISTORY_PEAK_VALUES], SigprocError> {
    check_storage(
        "gain_detect_secondary_history_previous",
        previous_history_words.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_storage(
        "gain_detect_secondary_history_weights",
        weight_words.len(),
        GAIN_DETECT_PEAK_BINS,
    )?;

    let mut history = [0; GAIN_DETECT_HISTORY_PEAK_VALUES];
    let previous_start = GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START;
    let previous_end = previous_start + GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START;
    history[..GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START]
        .copy_from_slice(&previous_history_words[previous_start..previous_end]);
    history[GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START
        ..GAIN_DETECT_SECONDARY_HISTORY_WEIGHT_START + GAIN_DETECT_PEAK_BINS]
        .copy_from_slice(&weight_words[..GAIN_DETECT_PEAK_BINS]);
    history[GAIN_DETECT_HISTORY_PEAK_VALUES - 1] = trailing_word;
    Ok(history)
}

pub fn gain_detect_primary_history_at5(
    seed_words: &[u32],
    peak_words: &[u32],
) -> Result<[u32; GAIN_DETECT_HISTORY_PEAK_VALUES], SigprocError> {
    check_storage(
        "gain_detect_primary_history_seed",
        seed_words.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_storage(
        "gain_detect_primary_history_peaks",
        peak_words.len(),
        GAIN_DETECT_PEAK_BINS,
    )?;

    let mut history = [0; GAIN_DETECT_HISTORY_PEAK_VALUES];
    history.copy_from_slice(&seed_words[..GAIN_DETECT_HISTORY_PEAK_VALUES]);
    history[GAIN_DETECT_PRIMARY_HISTORY_PEAK_START
        ..GAIN_DETECT_PRIMARY_HISTORY_PEAK_START + GAIN_DETECT_PEAK_BINS]
        .copy_from_slice(&peak_words[..GAIN_DETECT_PEAK_BINS]);
    Ok(history)
}

pub fn gain_detect_primary_history_shift_at5(
    previous_history_words: &[u32],
    peak_words: &[u32],
) -> Result<[u32; GAIN_DETECT_HISTORY_PEAK_VALUES], SigprocError> {
    check_storage(
        "gain_detect_primary_history_previous",
        previous_history_words.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_storage(
        "gain_detect_primary_history_peaks",
        peak_words.len(),
        GAIN_DETECT_PEAK_BINS,
    )?;

    let mut history = [0; GAIN_DETECT_HISTORY_PEAK_VALUES];
    history[..GAIN_DETECT_PEAK_BINS].copy_from_slice(
        &previous_history_words[GAIN_DETECT_PEAK_BINS..GAIN_DETECT_HISTORY_PEAK_VALUES],
    );
    history[GAIN_DETECT_PRIMARY_HISTORY_PEAK_START
        ..GAIN_DETECT_PRIMARY_HISTORY_PEAK_START + GAIN_DETECT_PEAK_BINS]
        .copy_from_slice(&peak_words[..GAIN_DETECT_PEAK_BINS]);
    Ok(history)
}

impl GainDetectWritebackCopies {
    pub fn history_primary_words(&self) -> &[u32; GAIN_DETECT_HISTORY_PEAK_VALUES] {
        &self.history_primary_words
    }

    pub fn history_secondary_words(&self) -> &[u32; GAIN_DETECT_HISTORY_PEAK_VALUES] {
        &self.history_secondary_words
    }

    pub fn candidate_pool_words(&self) -> &[u32; GAIN_DETECT_CANDIDATE_POOL_WORDS] {
        &self.candidate_pool_words
    }
}

impl GainDetectCandidatePoolStackBase {
    pub const fn from_native(absolute_addr: u32, stack_offset: i32) -> Self {
        Self {
            absolute_addr,
            stack_offset,
        }
    }

    pub const fn absolute_addr(&self) -> u32 {
        self.absolute_addr
    }

    pub const fn stack_offset(&self) -> i32 {
        self.stack_offset
    }

    fn pool_pointer_byte_offset(&self, word: u32) -> Option<u32> {
        let byte_offset = word.wrapping_sub(self.absolute_addr);
        if byte_offset < GAIN_DETECT_CANDIDATE_POOL_BYTES && byte_offset % 4 == 0 {
            Some(byte_offset)
        } else {
            None
        }
    }
}

impl GainDetectEmitCandidate {
    pub const fn from_native_words(
        words: [i32; GAIN_DETECT_EMIT_RECORD_WORDS],
        next_index: Option<usize>,
    ) -> Self {
        Self { words, next_index }
    }

    pub fn words(&self) -> &[i32; GAIN_DETECT_EMIT_RECORD_WORDS] {
        &self.words
    }

    pub fn next_index(&self) -> Option<usize> {
        self.next_index
    }
}

impl GainDetectCandidateListRecord {
    pub const fn from_native_words(words: [i32; GAIN_DETECT_EMIT_RECORD_WORDS]) -> Self {
        Self {
            words,
            next_index: None,
            previous_index: None,
        }
    }

    pub fn words(&self) -> &[i32; GAIN_DETECT_EMIT_RECORD_WORDS] {
        &self.words
    }

    pub fn next_index(&self) -> Option<usize> {
        self.next_index
    }

    pub fn previous_index(&self) -> Option<usize> {
        self.previous_index
    }

    pub fn as_emit_candidate(&self) -> GainDetectEmitCandidate {
        GainDetectEmitCandidate::from_native_words(self.words, self.next_index)
    }
}

impl GainDetectCandidateListBounds {
    pub fn head_index(&self) -> Option<usize> {
        self.head_index
    }

    pub fn tail_index(&self) -> Option<usize> {
        self.tail_index
    }
}

impl GainDetectCandidatePrepScalars {
    pub fn active_span_count(&self) -> usize {
        self.active_span_count
    }

    pub fn branch_flag_a(&self) -> i32 {
        self.branch_flag_a
    }

    pub fn branch_flag_b(&self) -> i32 {
        self.branch_flag_b
    }

    pub fn prep_peak(&self) -> f32 {
        self.prep_peak
    }

    pub fn level_a(&self) -> i32 {
        self.level_a
    }

    pub fn level_b(&self) -> i32 {
        self.level_b
    }

    pub fn lower_bound(&self) -> i32 {
        self.lower_bound
    }

    pub fn upper_bound(&self) -> i32 {
        self.upper_bound
    }
}

impl GainDetectCandidateInterval {
    pub fn distance_word(&self) -> i32 {
        self.distance_word
    }

    pub fn lower(&self) -> i32 {
        self.lower
    }

    pub fn upper(&self) -> i32 {
        self.upper
    }
}

impl GainDetectCandidateLoopCursor {
    pub fn new(active_span_count: usize) -> Result<Self, SigprocError> {
        if active_span_count > GAIN_DETECT_HISTORY_PEAK_VALUES {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_candidate_active_span_count",
                value: active_span_count,
                max: GAIN_DETECT_HISTORY_PEAK_VALUES,
            });
        }
        Ok(Self {
            emitted_count: 0,
            remaining_count: active_span_count,
        })
    }

    pub fn emitted_count(&self) -> usize {
        self.emitted_count
    }

    pub fn remaining_count(&self) -> usize {
        self.remaining_count
    }

    pub fn should_continue(&self) -> bool {
        self.emitted_count < self.remaining_count.saturating_sub(1)
    }

    pub fn observe_gc_set_result(&mut self, result: i32) -> Result<(), SigprocError> {
        let result = checked_nonnegative_count(
            "gain_detect_candidate_gc_set_result",
            result,
            GAIN_DETECT_HISTORY_PEAK_VALUES,
        )?;
        if result > self.remaining_count {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_candidate_remaining_count",
                value: result,
                max: self.remaining_count,
            });
        }

        self.emitted_count += 1;
        self.remaining_count -= result;
        Ok(())
    }
}

impl GainDetectCandidateSourceCall {
    pub fn source_record_index(&self) -> usize {
        self.source_record_index
    }

    pub fn side(&self) -> GainDetectCandidateSide {
        self.side
    }

    pub fn source_words(&self) -> &[u32; GAIN_DETECT_EMIT_RECORD_WORDS] {
        &self.source_words
    }

    pub fn source_index(&self) -> i32 {
        self.source_index
    }
}

impl GainDetectCandidateSourceQueue {
    pub fn new(
        source_words: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
        source_index: i32,
    ) -> Result<Self, SigprocError> {
        checked_nonnegative_index(
            "gain_detect_candidate_source_index",
            source_index,
            GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
        )?;
        let mut records =
            [([0u32; GAIN_DETECT_EMIT_RECORD_WORDS], 0); GAIN_DETECT_HISTORY_PEAK_VALUES];
        records[0] = (source_words, source_index);
        Ok(Self {
            records,
            record_count: 1,
            cursor_index: 0,
            next_side: GainDetectCandidateSide::Lower,
        })
    }

    pub fn record_count(&self) -> usize {
        self.record_count
    }

    pub fn cursor_index(&self) -> usize {
        self.cursor_index
    }

    pub fn push_destination(
        &mut self,
        destination_words: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
        destination_index: i32,
    ) -> Result<(), SigprocError> {
        checked_nonnegative_index(
            "gain_detect_candidate_destination_index",
            destination_index,
            GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
        )?;
        if self.record_count == GAIN_DETECT_HISTORY_PEAK_VALUES {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_candidate_source_record_count",
                value: self.record_count + 1,
                max: GAIN_DETECT_HISTORY_PEAK_VALUES,
            });
        }
        self.records[self.record_count] = (destination_words, destination_index);
        self.record_count += 1;
        Ok(())
    }

    pub fn next_call(&mut self) -> Result<Option<GainDetectCandidateSourceCall>, SigprocError> {
        loop {
            if self.cursor_index == self.record_count {
                return Ok(None);
            }
            let (source_words, source_index) = self.records[self.cursor_index];
            match self.next_side {
                GainDetectCandidateSide::Lower => {
                    self.next_side = GainDetectCandidateSide::Upper;
                    let flag = checked_nonnegative_index(
                        "gain_detect_candidate_lower_flag",
                        source_words[8] as i32,
                        1,
                    )?;
                    if flag == 1 {
                        return Ok(Some(GainDetectCandidateSourceCall {
                            source_record_index: self.cursor_index,
                            side: GainDetectCandidateSide::Lower,
                            source_words,
                            source_index,
                        }));
                    }
                }
                GainDetectCandidateSide::Upper => {
                    let source_record_index = self.cursor_index;
                    self.cursor_index += 1;
                    self.next_side = GainDetectCandidateSide::Lower;
                    let flag = checked_nonnegative_index(
                        "gain_detect_candidate_upper_flag",
                        source_words[9] as i32,
                        1,
                    )?;
                    if flag == 1 {
                        return Ok(Some(GainDetectCandidateSourceCall {
                            source_record_index,
                            side: GainDetectCandidateSide::Upper,
                            source_words,
                            source_index,
                        }));
                    }
                }
            }
        }
    }
}

impl GainDetectCandidateLoopCall {
    pub fn source_record_index(&self) -> usize {
        self.source_record_index
    }

    pub fn side(&self) -> GainDetectCandidateSide {
        self.side
    }

    pub fn source_words(&self) -> &[u32; GAIN_DETECT_EMIT_RECORD_WORDS] {
        &self.source_words
    }

    pub fn source_index(&self) -> i32 {
        self.source_index
    }

    pub fn destination_words_before(&self) -> &[u32; GAIN_DETECT_EMIT_RECORD_WORDS] {
        &self.destination_words_before
    }

    pub fn destination_words_after(&self) -> &[u32; GAIN_DETECT_EMIT_RECORD_WORDS] {
        &self.destination_words_after
    }

    pub fn destination_index(&self) -> i32 {
        self.destination_index
    }

    pub fn gc_set_result(&self) -> i32 {
        self.gc_set_result
    }
}

impl GainDetectEmitLevelBounds {
    pub const fn new(min_level: i32, max_level: i32) -> Self {
        Self {
            min_level,
            max_level,
        }
    }

    pub fn min_level(&self) -> i32 {
        self.min_level
    }

    pub fn max_level(&self) -> i32 {
        self.max_level
    }

    pub fn observe(&mut self, level: i32) {
        if level < self.min_level {
            self.min_level = level;
        }
        if level > self.max_level {
            self.max_level = level;
        }
    }
}

impl GainDetectPeakBins {
    pub fn bins(&self) -> &[f32; GAIN_DETECT_PEAK_BINS] {
        &self.bins
    }

    pub fn max_index(&self) -> usize {
        self.max_index
    }

    pub fn max_value(&self) -> f32 {
        self.max_value
    }
}

impl GainDetectPeakSpan {
    pub fn slots(&self) -> &[usize] {
        &self.slots[..self.len]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GainDetectRecord {
    words: [i32; GAIN_DETECT_RECORD_WORDS],
}

impl GainDetectRecord {
    pub const fn from_words(words: [i32; GAIN_DETECT_RECORD_WORDS]) -> Self {
        Self { words }
    }

    pub fn words(&self) -> &[i32; GAIN_DETECT_RECORD_WORDS] {
        &self.words
    }

    pub fn point_count(&self) -> Result<usize, SigprocError> {
        checked_nonnegative_count("gain_detect_point_count", self.words[0], GAIN_DETECT_POINTS)
    }

    pub fn point_words(&self) -> Result<[i32; GAIN_DETECT_POINT_WORDS], SigprocError> {
        self.validate_points()?;

        let mut point_words = [0; GAIN_DETECT_POINT_WORDS];
        point_words.copy_from_slice(&self.words[..GAIN_DETECT_POINT_WORDS]);
        Ok(point_words)
    }

    pub fn is_empty(&self) -> bool {
        self.words[0] == 0
    }

    fn validate_points(&self) -> Result<(), SigprocError> {
        let count = self.point_count()?;
        for index in 0..count {
            checked_nonnegative_index(
                "gain_detect_location",
                self.words[1 + index],
                GAIN_DETECT_MAX_LOCATION,
            )?;
            checked_nonnegative_index(
                "gain_detect_level_id",
                self.words[8 + index],
                GAIN_DETECT_MAX_LEVEL_ID,
            )?;
        }
        Ok(())
    }
}

impl Default for GainDetectRecord {
    fn default() -> Self {
        Self {
            words: [0; GAIN_DETECT_RECORD_WORDS],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GainDetectSideData {
    slots: [[GainDetectRecord; GAIN_DETECT_BANDS]; GAIN_DETECT_SLOTS],
}

impl GainDetectSideData {
    pub fn from_native_words(
        words: &[[[i32; GAIN_DETECT_RECORD_WORDS]; GAIN_DETECT_BANDS]; GAIN_DETECT_SLOTS],
    ) -> Result<Self, SigprocError> {
        let mut slots = [[GainDetectRecord::default(); GAIN_DETECT_BANDS]; GAIN_DETECT_SLOTS];
        for slot_index in 0..GAIN_DETECT_SLOTS {
            for band_index in 0..GAIN_DETECT_BANDS {
                let record = GainDetectRecord::from_words(words[slot_index][band_index]);
                record.validate_points()?;
                slots[slot_index][band_index] = record;
            }
        }
        Ok(Self { slots })
    }

    pub fn record(
        &self,
        slot_index: usize,
        band_index: usize,
    ) -> Result<&GainDetectRecord, SigprocError> {
        check_index("gain_detect_slot", slot_index, GAIN_DETECT_SLOTS - 1)?;
        check_index("gain_detect_band", band_index, GAIN_DETECT_BANDS - 1)?;
        Ok(&self.slots[slot_index][band_index])
    }

    pub fn slots(&self) -> &[[GainDetectRecord; GAIN_DETECT_BANDS]; GAIN_DETECT_SLOTS] {
        &self.slots
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigprocError {
    StorageTooShort {
        name: &'static str,
        needed: usize,
        actual: usize,
    },
    CountOutOfRange {
        name: &'static str,
        value: usize,
        max: usize,
    },
    IndexOutOfRange {
        name: &'static str,
        value: usize,
        max: usize,
    },
    Transform(FftError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GainDetectCandidateLoopError {
    Sigproc(SigprocError),
    Gain(GainPassError),
    MissingSourceCall {
        emitted_count: usize,
        remaining_count: usize,
    },
    MissingDestinationHandoff {
        source_index: i32,
        side: GainDetectCandidateSide,
    },
    /// The over-seven prune loop reached the removal branch or an unproven
    /// merge surface. The merge iteration observed in
    /// `gain_detect_state_trace.ndjson` is ported; the removal branch is dead
    PruneLoopUnsupported {
        gate_operand: i32,
    },
    /// The removal branch computed a partner address outside the observed
    /// three-pool geometry (`removal_partner_events.ndjson`, docs/12 §2.2): the
    /// relative pool delta `word[8]` must land in `-1..=1` (absolute pool
    /// `0..=2`) and the record index `word[9]` must be `< 64`. Any other shape
    /// is unobserved and fails explicit rather than aliasing into the wrong
    /// pool.
    PruneRemovalPartnerOutOfRange {
        partner_pool: i32,
        partner_record: i32,
    },
}

impl From<SigprocError> for GainDetectCandidateLoopError {
    fn from(error: SigprocError) -> Self {
        Self::Sigproc(error)
    }
}

impl From<GainPassError> for GainDetectCandidateLoopError {
    fn from(error: GainPassError) -> Self {
        Self::Gain(error)
    }
}

pub fn gain_detect_peak_bins_at5(input: &[f32]) -> Result<GainDetectPeakBins, SigprocError> {
    check_storage(
        "gain_detect_peak_input",
        input.len(),
        GAIN_DETECT_PEAK_INPUT_VALUES,
    )?;

    let mut bins = [0.0; GAIN_DETECT_PEAK_BINS];
    let mut max_index = 0usize;
    let mut max_value = 0.0f32;

    for (bin_index, group) in input[..GAIN_DETECT_PEAK_INPUT_VALUES]
        .chunks_exact(GAIN_DETECT_PEAK_GROUP_VALUES)
        .enumerate()
    {
        let mut peak = group[0].abs();
        for value in &group[1..] {
            let candidate = value.abs();
            if peak < candidate {
                peak = candidate;
            }
        }
        bins[bin_index] = peak;
        if max_value < peak {
            max_value = peak;
            max_index = bin_index;
        }
    }

    Ok(GainDetectPeakBins {
        bins,
        max_index,
        max_value,
    })
}

pub fn gain_detect_activity_flags_at5(
    input: &[f32],
) -> Result<GainDetectActivityFlags, SigprocError> {
    check_storage(
        "gain_detect_activity_input",
        input.len(),
        GAIN_DETECT_ACTIVITY_INPUT_VALUES,
    )?;

    let mut quad_flags = [0; GAIN_DETECT_ACTIVITY_FLAGS];
    for (quad_index, group) in input[..GAIN_DETECT_ACTIVITY_INPUT_VALUES]
        .chunks_exact(GAIN_DETECT_PEAK_GROUP_VALUES)
        .enumerate()
    {
        quad_flags[quad_index] = group.iter().any(|value| *value != 0.0) as i32;
    }

    Ok(GainDetectActivityFlags { quad_flags })
}

pub fn gain_detect_peak_span_at5(
    history_peaks: &[f32],
    prev_max_slot: usize,
    prev_peak_slot_plus_32: usize,
    prev_level_a: f32,
    prev_level_b: f32,
) -> Result<GainDetectPeakSpan, SigprocError> {
    check_storage(
        "gain_detect_history_peaks",
        history_peaks.len(),
        GAIN_DETECT_HISTORY_PEAK_VALUES,
    )?;
    check_index(
        "gain_detect_prev_max_slot",
        prev_max_slot,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;
    check_index(
        "gain_detect_prev_peak_slot_plus_32",
        prev_peak_slot_plus_32,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;

    let level_a_before_b = matches!(
        prev_level_a.partial_cmp(&prev_level_b),
        Some(Ordering::Less) | None
    );
    let (head_slot, start_slot, end_slot) = if level_a_before_b {
        (
            prev_peak_slot_plus_32,
            prev_max_slot,
            prev_peak_slot_plus_32,
        )
    } else {
        (prev_max_slot, prev_max_slot + 1, prev_peak_slot_plus_32 + 1)
    };

    let mut slots = [0usize; GAIN_DETECT_HISTORY_PEAK_VALUES];
    slots[0] = head_slot;
    let mut len = 1usize;
    for candidate in start_slot..end_slot {
        check_index(
            "gain_detect_span_slot",
            candidate,
            GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
        )?;

        let insert_at = slots[1..len]
            .iter()
            .position(|slot| history_peaks[*slot] < history_peaks[candidate])
            .map(|offset| offset + 1)
            .unwrap_or(len);
        slots.copy_within(insert_at..len, insert_at + 1);
        slots[insert_at] = candidate;
        len += 1;
    }

    Ok(GainDetectPeakSpan { slots, len })
}

pub fn gain_detect_weight_at5(dft_output: &[f32]) -> Result<GainDetectWeight, SigprocError> {
    check_storage(
        "gain_detect_weight_dft_output",
        dft_output.len(),
        GAIN_DETECT_WEIGHT_BINS,
    )?;
    let bins = &dft_output[..GAIN_DETECT_WEIGHT_BINS];

    let mut log_sum = 0.0f32;
    let mut energy = bins[7] * bins[7] + bins[0] * bins[0];
    for value in &bins[1..7] {
        log_sum += ln_f32(*value + GAIN_DETECT_WEIGHT_EPSILON);
        energy += *value * *value;
    }

    let norm_log = ln_f32(energy.sqrt() + GAIN_DETECT_WEIGHT_EPSILON);
    let log0 = ln_f32(bins[0] + GAIN_DETECT_WEIGHT_EPSILON);
    let log7 = ln_f32(bins[7] + GAIN_DETECT_WEIGHT_EPSILON);
    let scratch_weight_a = weight_ratio(norm_log, log7, log_sum, log0);
    let scratch_weight_b = weight_ratio(norm_log, log0, log_sum, log7);

    let candidate = if scratch_weight_a < scratch_weight_b {
        scratch_weight_b
    } else {
        scratch_weight_a
    };
    let accepted = candidate > 1.0;
    let weight = if accepted { candidate } else { 1.0 };

    Ok(GainDetectWeight { weight, accepted })
}

pub fn gain_detect_window_weight_at5(
    source_window: &[f32],
) -> Result<GainDetectWeight, SigprocError> {
    check_storage(
        "gain_detect_weight_source_window",
        source_window.len(),
        GAIN_DETECT_WEIGHT_WINDOW_VALUES,
    )?;

    let window = half_hannwin_at5();
    let mut input = [0.0; GAIN_DETECT_WEIGHT_WINDOW_VALUES];
    for index in 0..window.len() {
        input[index] = source_window[index] * window[index];
        input[GAIN_DETECT_WEIGHT_WINDOW_VALUES - 1 - index] =
            source_window[GAIN_DETECT_WEIGHT_WINDOW_VALUES - 1 - index] * window[index];
    }

    let ip_table = ip016_at5_ref();
    let sc_table = sc016_at5_ref();
    let mut dft_output = [0.0; GAIN_DETECT_WEIGHT_DFT_OUTPUT_VALUES];
    dft_v_at5(
        &input,
        1,
        GAIN_DETECT_WEIGHT_WINDOW_VALUES,
        &mut dft_output,
        ip_table,
        sc_table,
    )
    .map_err(SigprocError::Transform)?;

    gain_detect_weight_at5(&dft_output[..GAIN_DETECT_WEIGHT_BINS])
}

pub fn gain_detect_weight_source_windows_at5(
    input: &[f32],
) -> Result<[[f32; GAIN_DETECT_WEIGHT_WINDOW_VALUES]; GAIN_DETECT_PEAK_BINS], SigprocError> {
    check_storage(
        "gain_detect_weight_source_input",
        input.len(),
        GAIN_DETECT_WEIGHT_SOURCE_INPUT_VALUES,
    )?;

    let mut windows = [[0.0; GAIN_DETECT_WEIGHT_WINDOW_VALUES]; GAIN_DETECT_PEAK_BINS];
    for (bin_index, window) in windows.iter_mut().enumerate() {
        let start =
            GAIN_DETECT_WEIGHT_SOURCE_START + bin_index * GAIN_DETECT_WEIGHT_SOURCE_STRIDE_VALUES;
        window.copy_from_slice(&input[start..start + GAIN_DETECT_WEIGHT_WINDOW_VALUES]);
    }
    Ok(windows)
}

pub fn gain_detect_window_weights_at5(
    source_windows: &[[f32; GAIN_DETECT_WEIGHT_WINDOW_VALUES]],
    activity_flags: &GainDetectActivityFlags,
) -> Result<[GainDetectWeight; GAIN_DETECT_PEAK_BINS], SigprocError> {
    check_storage(
        "gain_detect_weight_source_windows",
        source_windows.len(),
        GAIN_DETECT_PEAK_BINS,
    )?;

    let mut weights = [GainDetectWeight {
        weight: 1.0,
        accepted: false,
    }; GAIN_DETECT_PEAK_BINS];
    for bin_index in 0..GAIN_DETECT_PEAK_BINS {
        if activity_flags.should_run_weight(bin_index)? {
            weights[bin_index] = gain_detect_window_weight_at5(&source_windows[bin_index])?;
        }
    }
    Ok(weights)
}

/// The unified per-band detector front window: the native detector reads
/// band floats `500..640` (activity quads from float `500` = byte
/// `0x600 + 0x1d0`, peak groups from float `512` = byte `0x800`, and the
/// 16-float weight source windows at stride 4 from float `500`).
pub const GAIN_DETECT_BAND_WINDOW_VALUES: usize = GAIN_DETECT_ACTIVITY_INPUT_VALUES;
pub const GAIN_DETECT_BAND_WINDOW_PEAK_OFFSET: usize =
    GAIN_DETECT_ACTIVITY_INITIAL_FLAGS * GAIN_DETECT_PEAK_GROUP_VALUES;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainDetectBandFront {
    pub peaks: GainDetectPeakBins,
    pub activity: GainDetectActivityFlags,
    pub weights: [GainDetectWeight; GAIN_DETECT_PEAK_BINS],
}

/// Composed detector front for one band: activity quad flags over the full
/// 140-float window, absolute peak bins over the trailing 128 floats, and
/// rolling window weights over the 16-float source windows at stride 4.
pub fn gain_detect_band_front_at5(
    band_window: &[f32],
) -> Result<GainDetectBandFront, SigprocError> {
    check_storage(
        "gain_detect_band_window",
        band_window.len(),
        GAIN_DETECT_BAND_WINDOW_VALUES,
    )?;

    let activity = gain_detect_activity_flags_at5(band_window)?;
    let peaks = gain_detect_peak_bins_at5(&band_window[GAIN_DETECT_BAND_WINDOW_PEAK_OFFSET..])?;

    let mut source_windows = [[0.0; GAIN_DETECT_WEIGHT_WINDOW_VALUES]; GAIN_DETECT_PEAK_BINS];
    for (bin_index, window) in source_windows.iter_mut().enumerate() {
        let start = bin_index * GAIN_DETECT_WEIGHT_SOURCE_STRIDE_VALUES;
        window.copy_from_slice(&band_window[start..start + GAIN_DETECT_WEIGHT_WINDOW_VALUES]);
    }
    let weights = gain_detect_window_weights_at5(&source_windows, &activity)?;

    Ok(GainDetectBandFront {
        peaks,
        activity,
        weights,
    })
}

/// Outcome of the composed per-band detector pipeline.
#[derive(Debug, Clone)]
pub struct GainDetectBandOutcome {
    pub front: GainDetectBandFront,
    pub span_slots: Vec<usize>,
    pub gc_calls: Vec<GainDetectCandidateLoopCall>,
    pub gc_records: Vec<GainDetectCandidateListRecord>,
    /// Duplicate-location count at the initial prune gate.
    pub duplicate_count: usize,
    pub final_duplicate_count: usize,
    pub prune_iterations_run: usize,
    pub prune_removed_count: usize,
    /// Pool-2 (`local_116c`) removed count for this call (`local_55c[2]`,
    /// decompile 32517): it ages into the *next* call's gate removal seed via
    /// state `0x32c8` -> `local_55c[1]`. Carried through
    /// `time2freq_detector_seed_evolve_at5` as `carried_removed_count`
    /// (docs/12 §2.2). Zero unless the prune removal branch flagged a pool-2
    /// partner.
    pub prune_pool2_removed_count: usize,
    pub level_totals: [i32; GAIN_DETECT_PEAK_BINS],
    pub prune: bool,
    /// Set when the carried candidate pool still compacts to more than the
    /// longer covers the native call-6 ch0/b13 gate hit; remaining hits are
    /// upstream carried-pool/count divergences. `compact_point_words` is then
    /// an all-zero placeholder, not the native record.
    pub prune_blocked: bool,
    pub compact_point_words: [i32; GAIN_DETECT_POINT_WORDS],
    /// The merged list-B candidate records (gc_set output group 0, the native
    /// `local_1d6c` region that the writeback copies to the channel block at
    /// `detect_gainc_data_new_at5+0x3a672`). These carry forward: they become
    /// the next core call's list-A pool (`local_296c`) that
    /// `gain_detect_compact_record_from_candidate_list_at5` compacts into that
    /// call's output point record. Only the candidate data words survive the
    /// round trip; the run-specific linked-list pointer words (`words[2..4]`)
    /// are rebuilt from scratch every frame and are cleared here. Word 10 is
    /// the accumulated merge width and is preserved.
    pub next_pool_records: Vec<GainDetectCandidateListRecord>,
}

/// Preserve the public time-to-frequency output shape on the lean path while
/// carrying the exceptional `prune_blocked` signal to the coding bridge. The
/// normal path returns an empty vector and allocates nothing; marker outcomes
/// are materialized only when a blocked compaction actually occurs.
pub(crate) fn gain_detect_prune_markers_at5(
    band_count: usize,
    blocked: &[bool],
) -> Vec<GainDetectBandOutcome> {
    if !blocked.iter().take(band_count).any(|value| *value) {
        return Vec::new();
    }
    let front = GainDetectBandFront {
        peaks: GainDetectPeakBins {
            bins: [0.0; GAIN_DETECT_PEAK_BINS],
            max_index: 0,
            max_value: 0.0,
        },
        activity: GainDetectActivityFlags {
            quad_flags: [0; GAIN_DETECT_ACTIVITY_FLAGS],
        },
        weights: [GainDetectWeight {
            weight: 0.0,
            accepted: false,
        }; GAIN_DETECT_PEAK_BINS],
    };
    (0..band_count)
        .map(|band| GainDetectBandOutcome {
            front,
            span_slots: Vec::new(),
            gc_calls: Vec::new(),
            gc_records: Vec::new(),
            duplicate_count: 0,
            final_duplicate_count: 0,
            prune_iterations_run: 0,
            prune_removed_count: 0,
            prune_pool2_removed_count: 0,
            level_totals: [0; GAIN_DETECT_PEAK_BINS],
            prune: false,
            prune_blocked: blocked.get(band).copied().unwrap_or(false),
            compact_point_words: [0; GAIN_DETECT_POINT_WORDS],
            next_pool_records: Vec::new(),
        })
        .collect()
}

/// Reusable bounded storage for one native per-band detector call. The native
/// lists are capped at 32 records and the source/call chain at 64 entries.
pub(crate) struct GainDetectScratch {
    gc_calls: [Option<GainDetectCandidateLoopCall>; GAIN_DETECT_HISTORY_PEAK_VALUES],
    gc_records: [GainDetectCandidateListRecord; GAIN_DETECT_PEAK_BINS],
    pool0_records: [GainDetectCandidateListRecord; GAIN_DETECT_PEAK_BINS],
    pool2_records: [GainDetectCandidateListRecord; GAIN_DETECT_PEAK_BINS],
    next_pool_records: [GainDetectCandidateListRecord; GAIN_DETECT_PEAK_BINS],
    compact_candidates: [GainDetectEmitCandidate; GAIN_DETECT_PEAK_BINS],
    compact_visited: [bool; GAIN_DETECT_PEAK_BINS],
    compact_emitted: [(i32, i32); GAIN_DETECT_POINTS],
}

impl Default for GainDetectScratch {
    fn default() -> Self {
        const EMPTY: GainDetectCandidateListRecord =
            GainDetectCandidateListRecord::from_native_words([0i32; GAIN_DETECT_EMIT_RECORD_WORDS]);
        const EMPTY_EMIT: GainDetectEmitCandidate = GainDetectEmitCandidate {
            words: [0i32; GAIN_DETECT_EMIT_RECORD_WORDS],
            next_index: None,
        };
        Self {
            gc_calls: [None; GAIN_DETECT_HISTORY_PEAK_VALUES],
            gc_records: [EMPTY; GAIN_DETECT_PEAK_BINS],
            pool0_records: [EMPTY; GAIN_DETECT_PEAK_BINS],
            pool2_records: [EMPTY; GAIN_DETECT_PEAK_BINS],
            next_pool_records: [EMPTY; GAIN_DETECT_PEAK_BINS],
            compact_candidates: [EMPTY_EMIT; GAIN_DETECT_PEAK_BINS],
            compact_visited: [false; GAIN_DETECT_PEAK_BINS],
            compact_emitted: [(0, 0); GAIN_DETECT_POINTS],
        }
    }
}

impl GainDetectScratch {
    pub(crate) fn next_pool_records(
        &self,
        outcome: &GainDetectLeanOutcome,
    ) -> &[GainDetectCandidateListRecord] {
        &self.next_pool_records[..outcome.next_pool_count]
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GainDetectLeanOutcome {
    pub front: GainDetectBandFront,
    span: GainDetectPeakSpan,
    gc_call_count: usize,
    gc_record_count: usize,
    duplicate_count: usize,
    final_duplicate_count: usize,
    prune_iterations_run: usize,
    prune_removed_count: usize,
    pub prune_pool2_removed_count: usize,
    level_totals: [i32; GAIN_DETECT_PEAK_BINS],
    prune: bool,
    pub prune_blocked: bool,
    pub compact_point_words: [i32; GAIN_DETECT_POINT_WORDS],
    next_pool_count: usize,
}

/// Composed per-band `detect_gainc_data_new_at5` pipeline over the ported
/// stages: band front, peak span over the seeded spectrum history,
/// candidate prep scalars, the gc_set candidate loop, fresh-record list
/// insertion with duplicate counts / level totals / prune gate, and the
/// persistent-pool compaction to the final point words. The caller
/// provides the seeded spectrum/envelope surfaces, previous state scalars,
/// and the rebased persistent pool; band-state writeback and history
/// rotation remain separate ported helpers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gain_detect_band_with_scratch_at5(
    band_window: &[f32],
    spectrum: &[f32],
    envelope: &[f32],
    prev_max_slot: usize,
    prev_peak_slot_plus_32: usize,
    prev_level_a: f32,
    prev_level_b: f32,
    stored_peak_a: f32,
    current_bin0_peak: f32,
    carried_removed_count: usize,
    persistent_records: &mut [GainDetectCandidateListRecord],
    output_records: &mut [u32],
    counts: &mut [i32],
    scratch: &mut GainDetectScratch,
    capture_calls: bool,
) -> Result<GainDetectLeanOutcome, GainDetectCandidateLoopError> {
    let front = gain_detect_band_front_at5(band_window)?;
    let span = gain_detect_peak_span_at5(
        &spectrum[..GAIN_DETECT_HISTORY_PEAK_VALUES.min(spectrum.len())],
        prev_max_slot,
        prev_peak_slot_plus_32,
        prev_level_a,
        prev_level_b,
    )?;
    let prep = gain_detect_candidate_prep_scalars_at5(
        &span,
        prev_max_slot,
        prev_peak_slot_plus_32,
        prev_level_a,
        prev_level_b,
        stored_peak_a,
        current_bin0_peak,
    )?;
    let bounds_words = gain_detect_candidate_bounds_words_at5(&prep);
    let (initial_words, initial_index) =
        gain_detect_candidate_initial_source_words_at5(&prep, span.slots()[0])?;
    let gc_call_count = gain_detect_candidate_gc_set_loop_into_at5(
        &span,
        initial_words,
        initial_index,
        prep.active_span_count(),
        &bounds_words,
        spectrum,
        envelope,
        output_records,
        counts,
        capture_calls.then_some(&mut scratch.gc_calls),
    )?;

    let gc_count = counts.first().copied().unwrap_or(0).max(0) as usize;
    if gc_count > GAIN_DETECT_PEAK_BINS {
        return Err(SigprocError::CountOutOfRange {
            name: "gain_detect_candidate_list_count",
            value: gc_count,
            max: GAIN_DETECT_PEAK_BINS,
        }
        .into());
    }
    let gc_records = &mut scratch.gc_records[..gc_count];
    for record_index in 0..gc_count {
        let base = record_index * GC_SET_POINTS_OUTPUT_RECORD_WORDS;
        let mut words = [0i32; GAIN_DETECT_EMIT_RECORD_WORDS];
        for (word, source) in words
            .iter_mut()
            .zip(&output_records[base..base + GAIN_DETECT_EMIT_RECORD_WORDS])
        {
            *word = *source as i32;
        }
        // Fresh records enter the over-seven gate with width word 10 zero
        // (`gain_detect_state_trace.ndjson`, call 6 ch0 band13 pre-merge).
        words[10] = 0;
        gc_records[record_index] = GainDetectCandidateListRecord::from_native_words(words);
    }
    let gc_bounds = gain_detect_insert_candidate_list_at5(gc_records)?;
    let duplicate_count =
        gain_detect_duplicate_location_count_at5(gc_records, gc_bounds.head_index())?;
    let level_totals = gain_detect_level_totals_at5(gc_records, gc_bounds.head_index())?;

    // Pool 2 (`local_116c`, high-location fresh pool = gc output slab group 1)
    // and pool 0 (`local_296c`, list A = the carried persistent pool). Native
    // addresses removal-branch partners into all three pools relative to the
    // node's pool 1 (decompile 32387); pools 0/2 are partner targets only.
    let pool2_count = counts.get(1).copied().unwrap_or(0).max(0) as usize;
    if pool2_count > GAIN_DETECT_PEAK_BINS {
        return Err(SigprocError::CountOutOfRange {
            name: "gain_detect_candidate_list_count",
            value: pool2_count,
            max: GAIN_DETECT_PEAK_BINS,
        }
        .into());
    }
    let pool2_records = &mut scratch.pool2_records[..pool2_count];
    for record_index in 0..pool2_count {
        let base = GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS
            + record_index * GC_SET_POINTS_OUTPUT_RECORD_WORDS;
        let mut words = [0i32; GAIN_DETECT_EMIT_RECORD_WORDS];
        for (word, source) in words
            .iter_mut()
            .zip(&output_records[base..base + GAIN_DETECT_EMIT_RECORD_WORDS])
        {
            *word = *source as i32;
        }
        pool2_records[record_index] = GainDetectCandidateListRecord::from_native_words(words);
    }
    if persistent_records.len() > GAIN_DETECT_PEAK_BINS {
        return Err(SigprocError::CountOutOfRange {
            name: "gain_detect_candidate_list_count",
            value: persistent_records.len(),
            max: GAIN_DETECT_PEAK_BINS,
        }
        .into());
    }
    let pool0_records = &mut scratch.pool0_records[..persistent_records.len()];
    pool0_records.copy_from_slice(persistent_records);

    // Native over-seven prune/removal gate at `detect_gainc_data_new_at5`
    // decompile 32035 (`7 < (gc_count - duplicates) - removed[1]`), where
    // `removed[1]` is seeded by the carried pool-2 removal count from the
    // previous call (state `0x32c8`, aged into `local_55c[1]`; docs/12 §2.2).
    let prune = gain_detect_prune_gate_at5(gc_count, duplicate_count, carried_removed_count);
    let mut pool2_removed = 0usize;
    let mut final_level_totals = level_totals;
    let prune_result = if prune {
        // Fresh per-pool dup counts (`local_56c`, decompile 32001..32019): pool 1
        // feeds the gate; pool 0 (carried, state `0x3344`) and pool 2 seed the
        // removal branch's per-pool dup bookkeeping. Pool 0 is rebuilt fresh here
        // rather than carried because its only observable use in this call is the
        // per-pool dup decrement on a pool-0 partner removal.
        let pool0_bounds = gain_detect_insert_candidate_list_at5(pool0_records)?;
        let pool0_dup =
            gain_detect_duplicate_location_count_at5(pool0_records, pool0_bounds.head_index())?;
        let pool2_bounds = gain_detect_insert_candidate_list_at5(pool2_records)?;
        let pool2_dup =
            gain_detect_duplicate_location_count_at5(pool2_records, pool2_bounds.head_index())?;

        let mut removed = [0, carried_removed_count as i32, 0];
        let mut duplicates = [pool0_dup as i32, duplicate_count as i32, pool2_dup as i32];
        let mut totals = level_totals;
        let result = gain_detect_over_seven_prune_slices_at5(
            [&mut *pool0_records, &mut *gc_records, &mut *pool2_records],
            &mut removed,
            &mut duplicates,
            &mut totals,
        )?;
        pool2_removed = removed[2].max(0) as usize;
        final_level_totals = totals;
        result
    } else {
        GainDetectOverSevenPruneResult {
            iterations_run: 0,
            duplicate_count,
            removed_count: 0,
        }
    };
    // Final level totals reflect the incremental merge/removal deltas the loop
    // applied (including cross-pool partner subtractions); do not re-derive them
    // from pool 1 alone, which would erase those subtractions.
    let level_totals = final_level_totals;

    // Write pool-1 records back to gc output group 0 and pool-2 removal flags
    // back to group 1 (native writeback stores `local_1d6c`'s 0x600-int slab =
    // pools 1+2, decompile 32520..32526). Pool-0 flags live in
    // `persistent_records`, mutated below before compaction.
    for (record_index, record) in gc_records.iter().enumerate() {
        let base = record_index * GC_SET_POINTS_OUTPUT_RECORD_WORDS;
        for (dest, source) in output_records[base..base + GAIN_DETECT_EMIT_RECORD_WORDS]
            .iter_mut()
            .zip(record.words().iter())
        {
            *dest = *source as u32;
        }
    }
    for (record_index, record) in pool2_records.iter().enumerate() {
        let base = GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS
            + record_index * GC_SET_POINTS_OUTPUT_RECORD_WORDS;
        output_records[base + 5] = record.words()[5] as u32;
    }
    for (record, updated) in persistent_records.iter_mut().zip(pool0_records.iter()) {
        record.words[5] = updated.words()[5];
    }

    // Compact the carried candidate pool (list A) to the point record. Native
    // never exceeds 7 points here when the prior call's pool state matches
    // native. If an upstream carried-pool/count divergence still
    // over-accumulates past 7 emittable points, emit an all-zero placeholder
    // and flag it (`prune_blocked`) rather than crashing.
    let mut prune_blocked = false;
    let compact_point_words = match gain_detect_compact_record_from_candidate_list_with_scratch_at5(
        persistent_records,
        &mut scratch.compact_candidates,
        &mut scratch.compact_visited,
        &mut scratch.compact_emitted,
    ) {
        Ok(record) => record.point_words()?,
        Err(SigprocError::CountOutOfRange {
            name: "gain_detect_emit_point_count",
            ..
        }) => {
            prune_blocked = true;
            [0i32; GAIN_DETECT_POINT_WORDS]
        }
        Err(error) => return Err(error.into()),
    };

    // The merged list-B pool (gc_set output group 0 = native `local_1d6c`).
    // This is what the native writeback copies to the channel block and what
    // becomes the next core call's list-A pool. Carry only the candidate data
    // words; zero only the run-specific linked-list pointer words `words[2..4]`.
    // Word 10 carries the accumulated merge width and must survive into the
    // next pool.
    for (destination, record) in scratch.next_pool_records[..gc_count]
        .iter_mut()
        .zip(gc_records.iter())
    {
        let mut words = *record.words();
        words[2] = 0;
        words[3] = 0;
        words[4] = 0;
        *destination = GainDetectCandidateListRecord::from_native_words(words);
    }

    Ok(GainDetectLeanOutcome {
        front,
        span,
        gc_call_count,
        gc_record_count: gc_count,
        duplicate_count,
        final_duplicate_count: prune_result.duplicate_count,
        prune_iterations_run: prune_result.iterations_run,
        prune_removed_count: prune_result.removed_count,
        prune_pool2_removed_count: pool2_removed,
        level_totals,
        prune,
        prune_blocked,
        compact_point_words,
        next_pool_count: gc_count,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn gain_detect_band_at5(
    band_window: &[f32],
    spectrum: &[f32],
    envelope: &[f32],
    prev_max_slot: usize,
    prev_peak_slot_plus_32: usize,
    prev_level_a: f32,
    prev_level_b: f32,
    stored_peak_a: f32,
    current_bin0_peak: f32,
    carried_removed_count: usize,
    persistent_records: &mut [GainDetectCandidateListRecord],
    output_records: &mut [u32],
    counts: &mut [i32],
) -> Result<GainDetectBandOutcome, GainDetectCandidateLoopError> {
    let mut scratch = GainDetectScratch::default();
    let lean = gain_detect_band_with_scratch_at5(
        band_window,
        spectrum,
        envelope,
        prev_max_slot,
        prev_peak_slot_plus_32,
        prev_level_a,
        prev_level_b,
        stored_peak_a,
        current_bin0_peak,
        carried_removed_count,
        persistent_records,
        output_records,
        counts,
        &mut scratch,
        true,
    )?;
    Ok(GainDetectBandOutcome {
        front: lean.front,
        span_slots: lean.span.slots().to_vec(),
        gc_calls: scratch.gc_calls[..lean.gc_call_count]
            .iter()
            .map(|call| call.expect("captured detector call"))
            .collect(),
        gc_records: scratch.gc_records[..lean.gc_record_count].to_vec(),
        duplicate_count: lean.duplicate_count,
        final_duplicate_count: lean.final_duplicate_count,
        prune_iterations_run: lean.prune_iterations_run,
        prune_removed_count: lean.prune_removed_count,
        prune_pool2_removed_count: lean.prune_pool2_removed_count,
        level_totals: lean.level_totals,
        prune: lean.prune,
        prune_blocked: lean.prune_blocked,
        compact_point_words: lean.compact_point_words,
        next_pool_records: scratch.next_pool_records[..lean.next_pool_count].to_vec(),
    })
}

pub fn gain_detect_candidate_level_at5(
    numerator: f32,
    denominator: f32,
    cap: i32,
) -> Result<i32, SigprocError> {
    let cap = checked_nonnegative_index("gain_detect_candidate_level_cap", cap, 9)? as i32;
    if !(denominator > 0.0 && numerator > denominator) {
        return Ok(0);
    }

    let scaled = ((numerator as f64 / denominator as f64).ln() * GAIN_DETECT_LEVEL_LOG2_E).trunc();
    Ok((scaled as i32 + 1).min(cap))
}

pub fn gain_detect_candidate_prep_scalars_at5(
    span: &GainDetectPeakSpan,
    prev_max_slot: usize,
    prev_peak_slot_plus_32: usize,
    prev_level_a: f32,
    prev_level_b: f32,
    stored_peak_a: f32,
    current_bin0_peak: f32,
) -> Result<GainDetectCandidatePrepScalars, SigprocError> {
    check_index(
        "gain_detect_prev_max_slot",
        prev_max_slot,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;
    check_index(
        "gain_detect_prev_peak_slot_plus_32",
        prev_peak_slot_plus_32,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;
    check_storage("gain_detect_peak_span", span.slots().len(), 1)?;

    let head_is_prev_max = span.slots()[0] == prev_max_slot;
    let (branch_flag_a, branch_flag_b, prep_peak) = if head_is_prev_max {
        (0, 1, prev_level_a)
    } else {
        (1, 0, prev_level_b)
    };

    Ok(GainDetectCandidatePrepScalars {
        active_span_count: span.slots().len(),
        branch_flag_a,
        branch_flag_b,
        prep_peak,
        level_a: gain_detect_candidate_level_at5(stored_peak_a, prev_level_a, 6)?,
        level_b: gain_detect_candidate_level_at5(current_bin0_peak, prev_level_b, 9)?,
        lower_bound: prev_max_slot as i32 - 1,
        upper_bound: prev_peak_slot_plus_32 as i32 + 1,
    })
}

pub fn gain_detect_candidate_bounds_words_at5(
    scalars: &GainDetectCandidatePrepScalars,
) -> [u32; GAIN_DETECT_CANDIDATE_BOUNDS_WORDS] {
    let mut words = [0; GAIN_DETECT_CANDIDATE_BOUNDS_WORDS];
    words[0] = scalars.prep_peak().to_bits();
    words[3] = scalars.level_a() as u32;
    words[6] = scalars.lower_bound() as u32;
    words[7] = scalars.upper_bound() as u32;
    words
}

pub fn gain_detect_candidate_initial_source_words_at5(
    scalars: &GainDetectCandidatePrepScalars,
    head_slot: usize,
) -> Result<([u32; GAIN_DETECT_EMIT_RECORD_WORDS], i32), SigprocError> {
    check_index(
        "gain_detect_candidate_source_index",
        head_slot,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;

    let mut words = [0; GAIN_DETECT_EMIT_RECORD_WORDS];
    words[0] = scalars.prep_peak().to_bits();
    words[3] = scalars.level_a() as u32;
    words[4] = scalars.level_b() as u32;
    words[6] = scalars.lower_bound() as u32;
    words[7] = scalars.upper_bound() as u32;
    words[8] = scalars.branch_flag_a() as u32;
    words[9] = scalars.branch_flag_b() as u32;
    Ok((words, head_slot as i32))
}

pub fn gain_detect_candidate_interval_at5(
    source_words: &[u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    source_index: i32,
    side: GainDetectCandidateSide,
    global_lower: i32,
    global_upper: i32,
) -> Result<Option<GainDetectCandidateInterval>, SigprocError> {
    let source_index = checked_nonnegative_index(
        "gain_detect_candidate_source_index",
        source_index,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )? as i32;
    // `words[6]`/`words[7]` are the candidate's lower/upper *bounds*, not
    // history slot indices: native (`detect_gainc_data_new_at5` 31698..31721 /
    // 31780..31813) uses them only as signed `int` operands and sentinel
    // comparands (`== global_lower` / `== global_upper`), never as array
    // indices. Their native domain is `prev_max_slot - 1 .. prev_peak_slot + 1`
    // = `[-1, 64]`; the first candidate at core call 0 carries lower `-1`
    // (`prev_max_slot = 0`). Accept that signed range instead of the
    // nonnegative-index domain.
    let source_lower =
        check_candidate_bound("gain_detect_candidate_lower", source_words[6] as i32)?;
    let source_upper =
        check_candidate_bound("gain_detect_candidate_upper", source_words[7] as i32)?;

    match side {
        GainDetectCandidateSide::Lower => {
            let flag = checked_nonnegative_index(
                "gain_detect_candidate_lower_flag",
                source_words[8] as i32,
                1,
            )?;
            if flag != 1 {
                return Ok(None);
            }
            let distance_word = if source_lower == global_lower {
                source_index * 2 - 0x20
            } else {
                (source_index - source_lower) * 2 - 2
            };
            Ok(Some(GainDetectCandidateInterval {
                distance_word,
                lower: source_lower,
                upper: source_index,
            }))
        }
        GainDetectCandidateSide::Upper => {
            let flag = checked_nonnegative_index(
                "gain_detect_candidate_upper_flag",
                source_words[9] as i32,
                1,
            )?;
            if flag != 1 {
                return Ok(None);
            }
            let distance_word = if source_upper == global_upper {
                source_index * -2 + 0x5e
            } else {
                (source_upper - source_index) * 2 - 2
            };
            Ok(Some(GainDetectCandidateInterval {
                distance_word,
                lower: source_index,
                upper: source_upper,
            }))
        }
    }
}

pub fn gain_detect_candidate_destination_words_at5(
    source_words: &[u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    source_index: i32,
    destination_index: i32,
    side: GainDetectCandidateSide,
    global_lower: i32,
    global_upper: i32,
) -> Result<Option<([u32; GAIN_DETECT_EMIT_RECORD_WORDS], i32)>, SigprocError> {
    let destination_index = checked_nonnegative_index(
        "gain_detect_candidate_destination_index",
        destination_index,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )? as i32;

    let Some(interval) = gain_detect_candidate_interval_at5(
        source_words,
        source_index,
        side,
        global_lower,
        global_upper,
    )?
    else {
        return Ok(None);
    };

    let mut destination_words = [0; GAIN_DETECT_EMIT_RECORD_WORDS];
    destination_words[1] = interval.distance_word() as u32;
    destination_words[6] = interval.lower() as u32;
    destination_words[7] = interval.upper() as u32;
    Ok(Some((destination_words, destination_index)))
}

pub fn gain_detect_candidate_handoff_words_at5(
    span: &GainDetectPeakSpan,
    source_words: &[u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    source_index: i32,
    side: GainDetectCandidateSide,
    global_lower: i32,
    global_upper: i32,
) -> Result<Option<([u32; GAIN_DETECT_EMIT_RECORD_WORDS], i32)>, SigprocError> {
    let Some(destination_index) =
        gain_detect_candidate_destination_slot_at5(span, source_words, source_index, side)?
    else {
        return Ok(None);
    };

    gain_detect_candidate_destination_words_at5(
        source_words,
        source_index,
        destination_index as i32,
        side,
        global_lower,
        global_upper,
    )
}

pub fn gain_detect_candidate_destination_slot_at5(
    span: &GainDetectPeakSpan,
    source_words: &[u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    source_index: i32,
    side: GainDetectCandidateSide,
) -> Result<Option<usize>, SigprocError> {
    let source_index = checked_nonnegative_index(
        "gain_detect_candidate_source_index",
        source_index,
        GAIN_DETECT_HISTORY_PEAK_VALUES - 1,
    )?;
    // Bounds, not indices: signed operands in native's `<` comparisons only
    // (see `gain_detect_candidate_interval_at5`). Native domain `[-1, 64]`;
    // core call 0 carries lower `-1`.
    let source_lower =
        check_candidate_bound("gain_detect_candidate_lower", source_words[6] as i32)?;
    let source_upper =
        check_candidate_bound("gain_detect_candidate_upper", source_words[7] as i32)?;

    let flag = match side {
        GainDetectCandidateSide::Lower => checked_nonnegative_index(
            "gain_detect_candidate_lower_flag",
            source_words[8] as i32,
            1,
        )?,
        GainDetectCandidateSide::Upper => checked_nonnegative_index(
            "gain_detect_candidate_upper_flag",
            source_words[9] as i32,
            1,
        )?,
    };
    if flag != 1 {
        return Ok(None);
    }

    let Some(source_position) = span.slots().iter().position(|slot| *slot == source_index) else {
        return Ok(None);
    };
    let source_index = source_index as i32;
    let mut candidates = span.slots()[source_position + 1..].iter().copied();
    let destination = match side {
        GainDetectCandidateSide::Lower => {
            candidates.find(|slot| source_lower < *slot as i32 && (*slot as i32) < source_index)
        }
        GainDetectCandidateSide::Upper => {
            candidates.find(|slot| source_index < *slot as i32 && (*slot as i32) < source_upper)
        }
    };
    Ok(destination)
}

pub fn gain_detect_candidate_gc_set_loop_at5(
    span: &GainDetectPeakSpan,
    initial_source_words: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    initial_source_index: i32,
    active_span_count: usize,
    bounds_words: &[u32],
    spectrum: &[f32],
    envelope: &[f32],
    output_records: &mut [u32],
    counts: &mut [i32],
) -> Result<Vec<GainDetectCandidateLoopCall>, GainDetectCandidateLoopError> {
    let mut call_slots = [None; GAIN_DETECT_HISTORY_PEAK_VALUES];
    let call_count = gain_detect_candidate_gc_set_loop_into_at5(
        span,
        initial_source_words,
        initial_source_index,
        active_span_count,
        bounds_words,
        spectrum,
        envelope,
        output_records,
        counts,
        Some(&mut call_slots),
    )?;
    Ok(call_slots[..call_count]
        .iter()
        .map(|call| call.expect("captured detector call"))
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn gain_detect_candidate_gc_set_loop_into_at5(
    span: &GainDetectPeakSpan,
    initial_source_words: [u32; GAIN_DETECT_EMIT_RECORD_WORDS],
    initial_source_index: i32,
    active_span_count: usize,
    bounds_words: &[u32],
    spectrum: &[f32],
    envelope: &[f32],
    output_records: &mut [u32],
    counts: &mut [i32],
    mut call_slots: Option<
        &mut [Option<GainDetectCandidateLoopCall>; GAIN_DETECT_HISTORY_PEAK_VALUES],
    >,
) -> Result<usize, GainDetectCandidateLoopError> {
    check_storage(
        "gain_detect_candidate_bounds_words",
        bounds_words.len(),
        GAIN_DETECT_CANDIDATE_BOUNDS_WORDS,
    )?;
    let mut cursor = GainDetectCandidateLoopCursor::new(active_span_count)?;
    let mut queue =
        GainDetectCandidateSourceQueue::new(initial_source_words, initial_source_index)?;
    let mut call_count = 0usize;

    while cursor.should_continue() {
        let source_call =
            queue
                .next_call()?
                .ok_or(GainDetectCandidateLoopError::MissingSourceCall {
                    emitted_count: cursor.emitted_count(),
                    remaining_count: cursor.remaining_count(),
                })?;
        let Some((destination_words_before, destination_index)) =
            gain_detect_candidate_handoff_words_at5(
                span,
                source_call.source_words(),
                source_call.source_index(),
                source_call.side(),
                bounds_words[6] as i32,
                bounds_words[7] as i32,
            )?
        else {
            return Err(GainDetectCandidateLoopError::MissingDestinationHandoff {
                source_index: source_call.source_index(),
                side: source_call.side(),
            });
        };

        let source =
            GcSetPointWords::from_words(*source_call.source_words(), source_call.source_index());
        let mut destination =
            GcSetPointWords::from_words(destination_words_before, destination_index);
        let gc_set_result = gc_set_points_at5(
            spectrum,
            envelope,
            bounds_words,
            &source,
            &mut destination,
            output_records,
            counts,
        )?;
        let destination_words_after = destination.words();

        cursor.observe_gc_set_result(gc_set_result)?;
        queue.push_destination(destination_words_after, destination_index)?;
        let call = GainDetectCandidateLoopCall {
            source_record_index: source_call.source_record_index(),
            side: source_call.side(),
            source_words: *source_call.source_words(),
            source_index: source_call.source_index(),
            destination_words_before,
            destination_words_after,
            destination_index,
            gc_set_result,
        };
        if let Some(slots) = call_slots.as_deref_mut() {
            slots[call_count] = Some(call);
        }
        call_count += 1;
    }

    Ok(call_count)
}

pub fn gain_detect_insert_candidate_list_at5(
    records: &mut [GainDetectCandidateListRecord],
) -> Result<GainDetectCandidateListBounds, SigprocError> {
    if records.len() > GAIN_DETECT_PEAK_BINS {
        return Err(SigprocError::CountOutOfRange {
            name: "gain_detect_candidate_list_count",
            value: records.len(),
            max: GAIN_DETECT_PEAK_BINS,
        });
    }

    for record in records.iter_mut() {
        record.next_index = None;
        record.previous_index = None;
    }

    let mut bounds = GainDetectCandidateListBounds::default();
    for index in 0..records.len() {
        if records[index].words[5] != 0 {
            continue;
        }

        let location = gain_detect_candidate_location_at5(&records[index])?;
        let mut previous_index = None;
        let mut next_index = bounds.head_index;

        if let Some(head_index) = next_index {
            if location < gain_detect_candidate_location_at5(&records[head_index])? {
                loop {
                    previous_index = next_index;
                    next_index = previous_index.and_then(|previous| records[previous].next_index);
                    match next_index {
                        Some(next)
                            if location < gain_detect_candidate_location_at5(&records[next])? => {}
                        _ => break,
                    }
                }
            }
        }

        if records[index].words[1] < 0 {
            let candidate_tie = gain_detect_candidate_sort_word_at5(&records[index]);
            while let Some(next) = next_index {
                if location != gain_detect_candidate_location_at5(&records[next])? {
                    break;
                }
                if candidate_tie < gain_detect_candidate_sort_word_at5(&records[next]) {
                    previous_index = next_index;
                    next_index = records[next].next_index;
                } else {
                    break;
                }
            }
        } else {
            let candidate_tie = gain_detect_candidate_sort_word_at5(&records[index]);
            while let Some(next) = next_index {
                if location != gain_detect_candidate_location_at5(&records[next])? {
                    break;
                }
                if gain_detect_candidate_sort_word_at5(&records[next]) < candidate_tie {
                    previous_index = next_index;
                    next_index = records[next].next_index;
                } else {
                    break;
                }
            }
        }

        records[index].previous_index = previous_index;
        records[index].next_index = next_index;
        if let Some(previous) = previous_index {
            records[previous].next_index = Some(index);
        } else {
            bounds.head_index = Some(index);
        }
        if let Some(next) = next_index {
            records[next].previous_index = Some(index);
        } else {
            bounds.tail_index = Some(index);
        }
    }

    Ok(bounds)
}

/// Emit-chain rebuild with equal-location fold in `detect_gainc_data_new_at5`
/// (native `0x3b5cc..0x3b69f` plus far block `0x3c403..0x3c422`, objdump; the
/// Ghidra decompile deleted this region). This is the stage between the
/// over-seven prune and the normalize/emission passes:
///
///   0x3b5cc/0x3b5e2: read the pool count and REBUILD the emit chain from
///       scratch (emit head `ebp-0x29ec` reset to 0).
///   Per pool record, POOL INDEX ORDER (stride 0x30):
///       0x3b653/0x3b658: skip the record entirely when word 5 (the
///           prune-removal flag) is nonzero.
///       0x3b67b/0x3b680: walk the descending-location word3 chain while
///           record.loc < node.loc.
///       0x3c403: record.loc > node.loc -> plain insert before the node.
///       0x3c415..0x3c422: EQUAL LOCATION -> FOLD: the incoming record
///           absorbs the existing node's level delta
///           (record.words[1] += node.words[1], a pool write), the old node
///           is unlinked (prev->word3 = node->next), and the incoming record
///           takes the old node's chain position.
///
/// The chain invariant (at most one node per location) means at most one
/// equal-location node can ever be met per insertion. This fold is why the
/// prune gate arithmetic is `count - duplicate_locations - removed <= 7`:
/// duplicates collapse here, so the emission loop sees at most 7 nodes.
///
/// Returns the rebuilt chain's head index. Unreachable records (word-5
/// skipped or folded out) keep no links; only the chain from the head is
/// meaningful afterwards.
pub fn gain_detect_emit_chain_rebuild_fold_at5(
    records: &mut [GainDetectCandidateListRecord],
) -> Result<Option<usize>, SigprocError> {
    if records.len() > GAIN_DETECT_PEAK_BINS {
        return Err(SigprocError::CountOutOfRange {
            name: "gain_detect_candidate_list_count",
            value: records.len(),
            max: GAIN_DETECT_PEAK_BINS,
        });
    }

    for record in records.iter_mut() {
        record.next_index = None;
        record.previous_index = None;
    }

    let mut head_index: Option<usize> = None;
    for index in 0..records.len() {
        // 0x3b653/0x3b658: prune-removal flag (word 5) excludes the record.
        if records[index].words[5] != 0 {
            continue;
        }

        let location = gain_detect_candidate_location_at5(&records[index])?;
        let mut previous_index: Option<usize> = None;
        let mut cursor = head_index;
        while let Some(node) = cursor {
            let node_location = gain_detect_candidate_location_at5(&records[node])?;
            if location < node_location {
                // 0x3b680 (jae not taken): keep walking the descending chain.
                previous_index = Some(node);
                cursor = records[node].next_index;
            } else if location == node_location {
                // 0x3c415..0x3c422: fold. The incoming record absorbs the
                // node's delta (native pool write) and takes its position.
                records[index].words[1] += records[node].words[1];
                cursor = records[node].next_index;
                records[node].next_index = None;
                break;
            } else {
                // 0x3c403 (jne): strictly greater -> insert before the node.
                break;
            }
        }

        records[index].next_index = cursor;
        if let Some(previous) = previous_index {
            records[previous].next_index = Some(index);
        } else {
            head_index = Some(index);
        }
    }

    Ok(head_index)
}

pub fn gain_detect_compact_record_from_candidate_list_at5(
    records: &mut [GainDetectCandidateListRecord],
) -> Result<GainDetectRecord, SigprocError> {
    // Native compaction pipeline inside `detect_gainc_data_new_at5`:
    // rebuild-with-fold (0x3b5cc..0x3b69f + 0x3c403..0x3c422) -> normalize
    // (0x3c1d0..0x3c236, cumulative levels + bounds) -> emission with the
    // adjacent-equal-level skip (0x3a48c..0x3a590).
    let head_index = gain_detect_emit_chain_rebuild_fold_at5(records)?;
    let mut candidates: Vec<_> = records
        .iter()
        .map(|record| record.as_emit_candidate())
        .collect();
    let level_bounds = gain_detect_normalize_emit_chain_levels_at5(&mut candidates, head_index)?;
    gain_detect_compact_record_from_emit_chain_at5(&candidates, head_index, level_bounds)
}

fn gain_detect_compact_record_from_candidate_list_with_scratch_at5(
    records: &mut [GainDetectCandidateListRecord],
    candidates: &mut [GainDetectEmitCandidate],
    visited: &mut [bool],
    emitted: &mut [(i32, i32)],
) -> Result<GainDetectRecord, SigprocError> {
    check_storage(
        "gain_detect_compact_candidates",
        candidates.len(),
        records.len(),
    )?;
    let head_index = gain_detect_emit_chain_rebuild_fold_at5(records)?;
    let candidates = &mut candidates[..records.len()];
    for (candidate, record) in candidates.iter_mut().zip(records.iter()) {
        *candidate = record.as_emit_candidate();
    }
    let level_bounds =
        gain_detect_normalize_emit_chain_levels_with_visited_at5(candidates, head_index, visited)?;
    gain_detect_compact_record_from_emit_chain_with_scratch_at5(
        candidates,
        head_index,
        level_bounds,
        visited,
        emitted,
    )
}

pub fn gain_detect_compact_record_from_emit_chain_at5(
    candidates: &[GainDetectEmitCandidate],
    head_index: Option<usize>,
    bounds: GainDetectEmitLevelBounds,
) -> Result<GainDetectRecord, SigprocError> {
    let mut visited = vec![false; candidates.len()];
    let mut emitted = [(0i32, 0i32); GAIN_DETECT_POINTS];
    gain_detect_compact_record_from_emit_chain_with_scratch_at5(
        candidates,
        head_index,
        bounds,
        &mut visited,
        &mut emitted,
    )
}

fn gain_detect_compact_record_from_emit_chain_with_scratch_at5(
    candidates: &[GainDetectEmitCandidate],
    head_index: Option<usize>,
    bounds: GainDetectEmitLevelBounds,
    visited: &mut [bool],
    emitted: &mut [(i32, i32)],
) -> Result<GainDetectRecord, SigprocError> {
    // Native compact emission loop in `detect_gainc_data_new_at5`
    // (native 0x39c40), region 0x3a48c..0x3a590 plus the far block
    // 0x3c238..0x3c248 (objdump -d libatrac.so.1.2.0). The Ghidra decompile
    // deleted this region, so the semantics are pinned by disassembly, not
    // decompiled C:
    //
    //   0x3a48c..0x3a4b0: clamp the observed level bounds so the emitted
    //       level ids stay in the g_a_lngain_at5 table range:
    //           bounds.max = min(bounds.max, 9)   (0x3a492/0x3a498)
    //           bounds.min = max(bounds.min, -6)  (0x3a4a5/0x3a4aa)
    //   0x3a4b6: prev = 0; emitted count = 0.
    //   per chain node (cumulative level already computed by the normalize
    //   pass, node word[1]):
    //       clamp the level DOWN to bounds.max (0x3a4ef) when it exceeds max;
    //       otherwise, via the far block, clamp UP to bounds.min (0x3c248)
    //       when below min, else leave it (0x3c242).
    //       *** THE SKIP (0x3a4f1/0x3a4f3): if the clamped level == prev, do
    //       not emit this node. *** Otherwise push (location, level_id).
    //       prev = clamped level on both paths (0x3a544).
    //   0x3a54f..0x3a58e: writeout: record.word0 = count; the location/id
    //       stacks are copied out in REVERSE order.
    //
    // Because g_a_lngain_at5 = [-6..=9] (16 entries), after the clamp
    // level<->id is injective (id = level + 6), so the skip is EXACTLY the
    // native DECODER's "adjacent gain level ids in a band must differ"
    // invariant (decoder abort 0x118, decompiled/libatrac.c line 26218;
    // wrapper 0x2000000|inner at lines 1739-1759). No location comparison
    // appears anywhere in this loop; the prior equal-location merge here was
    // retired inference (the Ghidra-deleted region gave no C to anchor it).
    let mut record = GainDetectRecord::default();

    let max_level = bounds.max_level().min(9);
    let min_level = bounds.min_level().max(-6);

    let Some(mut index) = head_index else {
        return Ok(record);
    };

    if index >= candidates.len() {
        return Err(SigprocError::IndexOutOfRange {
            name: "gain_detect_emit_head",
            value: index,
            max: candidates.len().saturating_sub(1),
        });
    }

    check_storage("gain_detect_emit_visited", visited.len(), candidates.len())?;
    check_storage("gain_detect_emit_points", emitted.len(), GAIN_DETECT_POINTS)?;
    visited[..candidates.len()].fill(false);
    let mut emitted_count = 0usize;
    let mut prev_level = 0;
    loop {
        if visited[index] {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_emit_chain",
                value: candidates.len() + 1,
                max: candidates.len(),
            });
        }
        visited[index] = true;

        let candidate = candidates[index];
        let location = checked_nonnegative_index(
            "gain_detect_emit_location",
            candidate.words[0],
            GAIN_DETECT_MAX_LOCATION,
        )?;
        let clamped_level = candidate.words[1].clamp(min_level, max_level);
        if clamped_level != prev_level {
            if emitted_count == GAIN_DETECT_POINTS {
                return Err(SigprocError::CountOutOfRange {
                    name: "gain_detect_emit_point_count",
                    value: emitted_count + 1,
                    max: GAIN_DETECT_POINTS,
                });
            }
            emitted[emitted_count] = (location as i32, gain_detect_level_id_at5(clamped_level));
            emitted_count += 1;
        }
        prev_level = clamped_level;

        let Some(next_index) = candidate.next_index else {
            break;
        };
        if next_index >= candidates.len() {
            return Err(SigprocError::IndexOutOfRange {
                name: "gain_detect_emit_next_record",
                value: next_index,
                max: candidates.len().saturating_sub(1),
            });
        }
        index = next_index;
    }

    record.words[0] = emitted_count as i32;
    for output_index in 0..emitted_count {
        let (location, level_id) = emitted[emitted_count - 1 - output_index];
        record.words[1 + output_index] = location;
        record.words[8 + output_index] = level_id;
    }
    record.validate_points()?;
    Ok(record)
}

pub fn gain_detect_normalize_emit_chain_levels_at5(
    candidates: &mut [GainDetectEmitCandidate],
    head_index: Option<usize>,
) -> Result<GainDetectEmitLevelBounds, SigprocError> {
    let mut visited = vec![false; candidates.len()];
    gain_detect_normalize_emit_chain_levels_with_visited_at5(candidates, head_index, &mut visited)
}

fn gain_detect_normalize_emit_chain_levels_with_visited_at5(
    candidates: &mut [GainDetectEmitCandidate],
    head_index: Option<usize>,
    visited: &mut [bool],
) -> Result<GainDetectEmitLevelBounds, SigprocError> {
    let mut bounds = GainDetectEmitLevelBounds::new(0, 0);
    let Some(mut index) = head_index else {
        return Ok(bounds);
    };

    if index >= candidates.len() {
        return Err(SigprocError::IndexOutOfRange {
            name: "gain_detect_emit_head",
            value: index,
            max: candidates.len().saturating_sub(1),
        });
    }

    check_storage(
        "gain_detect_normalize_visited",
        visited.len(),
        candidates.len(),
    )?;
    visited[..candidates.len()].fill(false);
    let mut cumulative_level = 0;
    loop {
        if visited[index] {
            return Err(SigprocError::CountOutOfRange {
                name: "gain_detect_emit_chain",
                value: candidates.len() + 1,
                max: candidates.len(),
            });
        }
        visited[index] = true;

        cumulative_level += candidates[index].words[1];
        candidates[index].words[1] = cumulative_level;
        bounds.observe(cumulative_level);

        let Some(next_index) = candidates[index].next_index else {
            break;
        };
        if next_index >= candidates.len() {
            return Err(SigprocError::IndexOutOfRange {
                name: "gain_detect_emit_next_record",
                value: next_index,
                max: candidates.len().saturating_sub(1),
            });
        }
        index = next_index;
    }

    Ok(bounds)
}

fn weight_ratio(norm_log: f32, edge_log: f32, middle_log_sum: f32, denominator_log: f32) -> f32 {
    let numerator = norm_log - (edge_log + middle_log_sum) * GAIN_DETECT_WEIGHT_AVERAGE_SCALE;
    let denominator = (norm_log - denominator_log) + GAIN_DETECT_WEIGHT_EPSILON;
    ln_f32(numerator / denominator + GAIN_DETECT_WEIGHT_EPSILON) * GAIN_DETECT_WEIGHT_LOG_SCALE
        + 1.0
}

fn ln_f32(value: f32) -> f32 {
    (value as f64).ln() as f32
}

fn gain_detect_level_id_at5(level: i32) -> i32 {
    let lngain = lngain_at5();
    let level = level.clamp(i32::from(lngain[0]), i32::from(lngain[lngain.len() - 1]));
    let level_id = lngain
        .iter()
        .rposition(|threshold| level >= i32::from(*threshold))
        .expect("native g_a_lngain_at5 table should not be empty");
    level_id as i32
}

fn gain_detect_candidate_location_at5(
    record: &GainDetectCandidateListRecord,
) -> Result<u32, SigprocError> {
    checked_nonnegative_index(
        "gain_detect_candidate_location",
        record.words[0],
        GAIN_DETECT_MAX_LOCATION,
    )
    .map(|value| value as u32)
}

fn gain_detect_candidate_sort_word_at5(record: &GainDetectCandidateListRecord) -> u32 {
    record.words[11] as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GainDetectOverSevenMergeCandidate {
    source_location: i32,
    destination_location: i32,
    source_width: i32,
    source_weight: i32,
    destination_weight: i32,
}

/// Outcome of one over-seven prune-loop iteration's candidate scan
/// (`detect_gainc_data_new_at5` decompile 32036..32261): either a merge with
/// a cost strictly below the working-chain threshold, or (when no merge beats
/// the threshold) the best-cost node selected for removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GainDetectOverSevenChoice {
    Merge(GainDetectOverSevenMergeCandidate),
    /// Remove the selected node (`local_3834`, decompile 32383..32467). Native
    /// sets word[5] = 1 on this record and, when it carries a partner
    /// (word[7] != 0), on its word[9]-linked partner too.
    Remove {
        removed_index: usize,
    },
}

/// Port of the over-seven prune-loop candidate scan in
/// `detect_gainc_data_new_at5` (native `0x3b2ee..0x3b3ab`, decompile
/// 32036..32261).
///
/// The native scan walks the weight-ascending working chain (`local_29cc`,
/// word[2] links, decompile 32043) and, per node, considers merging it into an
/// adjacent-location neighbour reached through the location-sorted list's
/// word[3] (higher location) / word[4] (lower location) links (decompile
/// 32052..32120). The merge cost is
/// `(node_loc - neighbour_loc) * totals[node_loc] * 2` (decompile 32073,
/// 32159); the `< 2` distance gate is directional — the mover's own width for
/// a lower neighbour (`(node[10] + node_loc) - neigh_loc`, decompile 32058)
/// versus the neighbour's width for a higher neighbour
/// (`(neigh[10] + neigh_loc) - node_loc`, decompile 32072). The best cost is
/// the strict minimum below `uVar27 = local_29c4[6] << 2` (decompile 32039),
/// first-wins on ties (decompile 32253 `local_385c < local_3840`).
///
/// This re-port retires two inferences that the all-84-call
/// `gain_detect_state_trace.ndjson` sweep disproves (2688 initial_gate rows,
/// six gate-pass events): the old code only merged when both the source and
/// destination had `words[1] > 0` (positive side), and only permitted
/// descending merges (`source_location > destination_location`). Native calls
/// 15/44 merge negative-side records, and calls 15/44 merge *ascending*
/// (26 -> 27, 27 -> 28). See docs/11 Phase 2 §2.1.
///
/// Working-chain caveat: the true native chain also threads `local_296c`
/// weight 32 setting the threshold to 128). The shipping model carries only
/// the gc pool (`local_1d6c`). For all six observed gate-pass events the
/// winning merge cost is 2 — far below either threshold — and the removal
/// event has no adjacent pair at all, so the `local_296c` head never flips a
/// decision. The min-gc-weight threshold is used here; the two-pool threshold
/// is the smallest next-evidence question if a later call diverges.
fn gain_detect_best_over_seven_choice_at5(
    records: &[GainDetectCandidateListRecord],
    level_totals: &[i32; GAIN_DETECT_PEAK_BINS],
) -> Result<Option<GainDetectOverSevenChoice>, SigprocError> {
    let mut active: Vec<usize> = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| (record.words[5] == 0).then_some(index))
        .collect();
    if active.is_empty() {
        return Ok(None);
    }

    // Weight-ascending working-chain scan order (`local_29cc`, word[6] key).
    // Ties resolve to the lower pool index so the strict-`<` cost comparison
    // reproduces native's first-wins tie-break in the traced events.
    active.sort_by(|left, right| {
        (records[*left].words[6] as u32)
            .cmp(&(records[*right].words[6] as u32))
            .then_with(|| left.cmp(right))
    });

    // `uVar27 = local_29c4[6] << 2`: threshold from the smallest working-chain
    // weight (see the two-pool caveat above).
    let threshold = (records[active[0]].words[6] as u32).wrapping_shl(2);

    // Location -> summed level total (word[1]) and a representative record's
    // width word (word[10]), used for the directional distance gate.
    let width_at = |location: i32| -> i32 {
        records
            .iter()
            .find(|record| record.words[5] == 0 && record.words[0] == location)
            .map(|record| record.words[10])
            .unwrap_or(0)
    };

    let mut best_cost = threshold;
    let mut best_merge: Option<GainDetectOverSevenMergeCandidate> = None;
    // `local_3834` init is `local_29c4` (decompile 32038): the working-chain
    // head. It is the removal fallback when no merge lowers `local_385c`.
    let mut removal_index = active[0];

    for &node_index in &active {
        let node = &records[node_index];
        let node_location = node.words[0];
        let node_width = node.words[10];
        let node_slot = checked_nonnegative_index(
            "gain_detect_prune_source_location",
            node_location,
            GAIN_DETECT_PEAK_BINS - 1,
        )?;

        // Nearest distinct neighbour location on each side (the word[3]/word[4]
        // sorted-list traversal skips equal-location runs).
        let mut lower_neighbour: Option<i32> = None;
        let mut higher_neighbour: Option<i32> = None;
        for &other_index in &active {
            let other_location = records[other_index].words[0];
            if other_location < node_location {
                lower_neighbour = Some(match lower_neighbour {
                    Some(current) => current.max(other_location),
                    None => other_location,
                });
            } else if other_location > node_location {
                higher_neighbour = Some(match higher_neighbour {
                    Some(current) => current.min(other_location),
                    None => other_location,
                });
            }
        }

        for (neighbour_location, distance) in [
            lower_neighbour
                .map(|neighbour| (neighbour, ((node_width + node_location) - neighbour) as u32)),
            higher_neighbour.map(|neighbour| {
                (
                    neighbour,
                    ((width_at(neighbour) + neighbour) - node_location) as u32,
                )
            }),
        ]
        .into_iter()
        .flatten()
        {
            if distance >= 2 {
                continue;
            }
            let cost = ((node_location - neighbour_location)
                .wrapping_mul(level_totals[node_slot])
                .wrapping_mul(2)) as u32;
            if cost >= best_cost {
                continue;
            }
            // Neighbour location range-checked so the apply step's slot lookups
            // cannot panic on a malformed record.
            checked_nonnegative_index(
                "gain_detect_prune_destination_location",
                neighbour_location,
                GAIN_DETECT_PEAK_BINS - 1,
            )?;
            best_cost = cost;
            best_merge = Some(GainDetectOverSevenMergeCandidate {
                source_location: node_location,
                destination_location: neighbour_location,
                source_width: node_width,
                source_weight: node.words[6],
                destination_weight: records
                    .iter()
                    .find(|record| record.words[5] == 0 && record.words[0] == neighbour_location)
                    .map(|record| record.words[6])
                    .unwrap_or(0),
            });
            // `local_3834 = local_378c` (decompile 32257): the removal fallback
            // tracks the node that most recently improved the best cost.
            removal_index = node_index;
        }
    }

    if let Some(candidate) = best_merge {
        Ok(Some(GainDetectOverSevenChoice::Merge(candidate)))
    } else {
        Ok(Some(GainDetectOverSevenChoice::Remove {
            removed_index: removal_index,
        }))
    }
}

fn gain_detect_apply_over_seven_merge_at5(
    records: &mut [GainDetectCandidateListRecord],
    candidate: GainDetectOverSevenMergeCandidate,
    level_totals: &mut [i32; GAIN_DETECT_PEAK_BINS],
) -> Result<(), SigprocError> {
    let source_slot = checked_nonnegative_index(
        "gain_detect_prune_source_location",
        candidate.source_location,
        GAIN_DETECT_PEAK_BINS - 1,
    )?;
    let destination_slot = checked_nonnegative_index(
        "gain_detect_prune_destination_location",
        candidate.destination_location,
        GAIN_DETECT_PEAK_BINS - 1,
    )?;
    let merged_width =
        (candidate.source_location - candidate.destination_location).abs() + candidate.source_width;

    let source_indices: Vec<usize> = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (record.words[5] == 0 && record.words[0] == candidate.source_location).then_some(index)
        })
        .collect();
    let destination_indices: Vec<usize> = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (record.words[5] == 0 && record.words[0] == candidate.destination_location)
                .then_some(index)
        })
        .collect();

    for index in source_indices {
        let record = &mut records[index];
        level_totals[source_slot] -= record.words[1];
        record.words[10] = merged_width;
        record.words[0] = candidate.destination_location;
        record.words[6] += candidate.destination_weight;
        level_totals[destination_slot] += record.words[1];
    }
    for index in destination_indices {
        let record = &mut records[index];
        record.words[6] += candidate.source_weight;
        record.words[10] = merged_width;
    }

    Ok(())
}

/// Number of aged candidate pools threaded through the over-seven prune loop.
/// Native `detect_gainc_data_new_at5` keeps three (`local_296c` = pool 0 /
/// list A, `local_1d6c` = pool 1 / gc working pool, `local_116c` = pool 2 /
/// high-location fresh pool), each 64 records of 12 i32 words.
pub const GAIN_DETECT_PRUNE_POOLS: usize = 3;
/// Native per-pool record capacity (64 records, decompile layout deltas
/// 0xc00/0x1800 bytes = 0x300 words each). A partner `word[9]` must stay below
/// this.
pub const GAIN_DETECT_PRUNE_POOL_CAPACITY: usize = 64;

/// The three aged candidate pools plus their per-pool removed/duplicate
/// counters and the shared level totals, threaded through the over-seven prune
/// loop's removal branch. Mirrors native `detect_gainc_data_new_at5` state:
/// `pools[0..3]` = `local_296c`/`local_1d6c`/`local_116c`, `removed` =
/// `local_55c[0..3]`, `duplicates` = `local_56c[0..3]`, `level_totals` =
/// `local_371c` (summed from pool 1's sorted list only, decompile 32029..32033;
/// the removal branch subtracts cross-pool partner word[1]s from it too,
/// decompile 32405/32446 — that skew is native).
///
/// The working chain and every merge/removal decision run over pool 1
/// (`local_1d6c`) only (decompile 31827..31839); pools 0 and 2 are touched
/// solely as partner targets.
pub struct GainDetectPrunePools<'a> {
    pub pools: [&'a mut Vec<GainDetectCandidateListRecord>; GAIN_DETECT_PRUNE_POOLS],
    pub removed: [i32; GAIN_DETECT_PRUNE_POOLS],
    pub duplicates: [i32; GAIN_DETECT_PRUNE_POOLS],
    pub level_totals: [i32; GAIN_DETECT_PEAK_BINS],
}

/// True when record `index` in pool `pool` has a same-location neighbour in
/// that pool's location-sorted list (native word[3]/word[4] links, decompile
/// 32390..32403 for the partner, 32431..32444 for the node): the dup counter
/// `local_56c[pool]` is decremented by 1 exactly when this holds. Rebuilt from
/// the pool's live (word[5]==0) records via the ported list insertion, which is
/// equivalent to the native incremental links for the single removal applied at
/// each call site.
fn gain_detect_prune_same_location_neighbour_at5(
    pool: &mut [GainDetectCandidateListRecord],
    index: usize,
) -> Result<bool, SigprocError> {
    gain_detect_insert_candidate_list_at5(pool)?;
    let location = pool[index].words[0];
    let neighbour_matches = |neighbour: Option<usize>| -> bool {
        neighbour.is_some_and(|n| pool[n].words[0] == location)
    };
    Ok(neighbour_matches(pool[index].previous_index) || neighbour_matches(pool[index].next_index))
}

/// Port of the over-seven prune-loop removal branch's observable effect on the
/// three aged candidate pools (`detect_gainc_data_new_at5` else-arm, native
/// `0x3bfbc..0x3c128`, decompile 32383..32467).
///
/// `removed_index` is the working-chain node selected for removal; it always
/// lives in pool 1 (`local_1d6c`, the only pool the working chain threads,
/// decompile 31827..31839). When it carries a partner (word[7] != 0, decompile
/// 32384) the partner is addressed *relative* to the node's pool via
/// `word[8]` (relative pool delta, `-1/0/+1`) and `word[9]` (record index in
/// the partner's pool):
///
///   partner_pool  = 1 + word[8]        (absolute 0..=2)
///   partner_index = word[9]            (0..=63)
///
/// (Native: `local_296c + word9*0xc + (word8+1)*0x300`, decompile 32387, with
/// `local_296c` the pool-0 base and the `*0x300`-word / `*0xc00`-byte pool
/// stride.) The partner unconditionally (no already-flagged / self checks in
/// native) gets word[5] = 1, its pool's removed counter += 1, its pool's dup
/// counter -= same-location-neighbour, and its word[1] subtracted from the
/// shared level totals — even when the partner is in pool 0 or pool 2, whose
/// word[1]s never contributed to the totals (decompile 32405..32406). The node
/// then gets the same treatment with hardcoded pool 1 (decompile 32429..32446).
///
/// Returns the per-pool removed increments `[pool0, pool1, pool2]` so the caller
/// can feed the pool-1 value into the gate and carry the pool-2 value into the
/// next call (native `local_55c`).
///
/// (64 events, `atx_gain_detect_removal_trace_v1`); the partner-pool histogram
/// is {no_partner:4, pool0:4, pool1:44, pool2:12}, 15 events change dup
/// counters, and the call-1981 ch1 band12 pool-0 event shows removed
/// [0,0,0]->[1,1,0] with node-loc and partner-loc word[1]s both subtracted from
/// the totals. Objdump-verified at native 0x3bfbc..0x3c128.
fn gain_detect_apply_over_seven_removal_slices_at5(
    pools: &mut [&mut [GainDetectCandidateListRecord]; GAIN_DETECT_PRUNE_POOLS],
    duplicates: &mut [i32; GAIN_DETECT_PRUNE_POOLS],
    level_totals: &mut [i32; GAIN_DETECT_PEAK_BINS],
    removed_index: usize,
) -> Result<[i32; GAIN_DETECT_PRUNE_POOLS], GainDetectCandidateLoopError> {
    let mut increments = [0i32; GAIN_DETECT_PRUNE_POOLS];

    // Partner first (native flags the partner before the node, decompile
    // 32384..32428), addressed relative to the node's pool 1.
    let (has_partner, partner_word8, partner_word9) = {
        let node = &pools[1][removed_index];
        (node.words[7] != 0, node.words[8], node.words[9])
    };
    if has_partner {
        let partner_pool = partner_word8 + 1;
        if !(0..GAIN_DETECT_PRUNE_POOLS as i32).contains(&partner_pool)
            || !(0..GAIN_DETECT_PRUNE_POOL_CAPACITY as i32).contains(&partner_word9)
        {
            return Err(
                GainDetectCandidateLoopError::PruneRemovalPartnerOutOfRange {
                    partner_pool,
                    partner_record: partner_word9,
                },
            );
        }
        let partner_pool = partner_pool as usize;
        let partner_index = partner_word9 as usize;
        if partner_index >= pools[partner_pool].len() {
            return Err(SigprocError::IndexOutOfRange {
                name: "gain_detect_prune_removal_partner",
                value: partner_index,
                max: pools[partner_pool].len().saturating_sub(1),
            }
            .into());
        }
        gain_detect_prune_flag_removal_at5(
            pools,
            duplicates,
            level_totals,
            partner_pool,
            partner_index,
        )?;
        increments[partner_pool] += 1;
    }

    // Then the node itself, in pool 1.
    gain_detect_prune_flag_removal_at5(pools, duplicates, level_totals, 1, removed_index)?;
    increments[1] += 1;

    Ok(increments)
}

/// Flag record `index` in pool `pool` removed and apply its native side effects:
/// word[5] = 1, dup counter `-= same-location-neighbour`, shared level totals
/// `-= word[1]` at the record's location bin. Shared across the partner and
/// node arms of the removal branch (decompile 32388..32406 / 32429..32446).
fn gain_detect_prune_flag_removal_at5(
    pools: &mut [&mut [GainDetectCandidateListRecord]; GAIN_DETECT_PRUNE_POOLS],
    duplicates: &mut [i32; GAIN_DETECT_PRUNE_POOLS],
    level_totals: &mut [i32; GAIN_DETECT_PEAK_BINS],
    pool: usize,
    index: usize,
) -> Result<(), GainDetectCandidateLoopError> {
    let neighbour = gain_detect_prune_same_location_neighbour_at5(pools[pool], index)?;
    if neighbour {
        duplicates[pool] -= 1;
    }
    let record = &mut pools[pool][index];
    let slot = checked_nonnegative_index(
        "gain_detect_prune_removal_location",
        record.words[0],
        GAIN_DETECT_PEAK_BINS - 1,
    )?;
    level_totals[slot] -= record.words[1];
    record.words[5] = 1;
    Ok(())
}

fn check_storage(name: &'static str, actual: usize, needed: usize) -> Result<(), SigprocError> {
    if actual < needed {
        Err(SigprocError::StorageTooShort {
            name,
            needed,
            actual,
        })
    } else {
        Ok(())
    }
}

fn checked_nonnegative_count(
    name: &'static str,
    value: i32,
    max: usize,
) -> Result<usize, SigprocError> {
    if value < 0 {
        return Err(SigprocError::CountOutOfRange {
            name,
            value: value as usize,
            max,
        });
    }
    let value = value as usize;
    if value > max {
        Err(SigprocError::CountOutOfRange { name, value, max })
    } else {
        Ok(value)
    }
}

fn checked_nonnegative_index(
    name: &'static str,
    value: i32,
    max: usize,
) -> Result<usize, SigprocError> {
    if value < 0 {
        return Err(SigprocError::IndexOutOfRange {
            name,
            value: value as usize,
            max,
        });
    }
    check_index(name, value as usize, max)
}

fn check_index(name: &'static str, value: usize, max: usize) -> Result<usize, SigprocError> {
    if value > max {
        Err(SigprocError::IndexOutOfRange { name, value, max })
    } else {
        Ok(value)
    }
}

/// Range check for a candidate lower/upper *bound* (a signed operand, not an
/// array index). Native's domain is `[-1, GAIN_DETECT_HISTORY_PEAK_VALUES]`.
fn check_candidate_bound(name: &'static str, value: i32) -> Result<i32, SigprocError> {
    if value < -1 || value > GAIN_DETECT_HISTORY_PEAK_VALUES as i32 {
        return Err(SigprocError::IndexOutOfRange {
            name,
            value: value as usize,
            max: GAIN_DETECT_HISTORY_PEAK_VALUES,
        });
    }
    Ok(value)
}

#[cfg(test)]
mod scratch_tests {
    use super::*;

    #[test]
    fn lean_gain_core_matches_diagnostics_and_reuses_backing_storage() {
        let band_window = [0.0f32; GAIN_DETECT_BAND_WINDOW_VALUES];
        let spectrum = [0.0f32; GAIN_DETECT_HISTORY_PEAK_VALUES];
        let envelope = [0.0f32; GAIN_DETECT_HISTORY_PEAK_VALUES];
        let mut diagnostic_persistent = Vec::new();
        let mut lean_persistent = Vec::new();
        let mut diagnostic_output = vec![0u32; 2 * GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS];
        let mut lean_output = diagnostic_output.clone();
        let mut diagnostic_counts = vec![0i32; 2];
        let mut lean_counts = diagnostic_counts.clone();

        let diagnostic = gain_detect_band_at5(
            &band_window,
            &spectrum,
            &envelope,
            0,
            GAIN_DETECT_PEAK_BINS,
            0.0,
            0.0,
            0.0,
            0.0,
            0,
            &mut diagnostic_persistent,
            &mut diagnostic_output,
            &mut diagnostic_counts,
        )
        .unwrap();

        let mut scratch = GainDetectScratch::default();
        let addresses = (
            scratch.gc_calls.as_ptr(),
            scratch.gc_records.as_ptr(),
            scratch.pool0_records.as_ptr(),
            scratch.pool2_records.as_ptr(),
            scratch.next_pool_records.as_ptr(),
            scratch.compact_candidates.as_ptr(),
            scratch.compact_visited.as_ptr(),
            scratch.compact_emitted.as_ptr(),
        );
        let lean = gain_detect_band_with_scratch_at5(
            &band_window,
            &spectrum,
            &envelope,
            0,
            GAIN_DETECT_PEAK_BINS,
            0.0,
            0.0,
            0.0,
            0.0,
            0,
            &mut lean_persistent,
            &mut lean_output,
            &mut lean_counts,
            &mut scratch,
            false,
        )
        .unwrap();

        assert_eq!(lean.compact_point_words, diagnostic.compact_point_words);
        assert_eq!(lean.prune_blocked, diagnostic.prune_blocked);
        assert_eq!(lean_output, diagnostic_output);
        assert_eq!(lean_counts, diagnostic_counts);
        assert_eq!(lean_persistent, diagnostic_persistent);
        assert_eq!(
            scratch.next_pool_records(&lean),
            diagnostic.next_pool_records
        );

        let _ = gain_detect_band_with_scratch_at5(
            &band_window,
            &spectrum,
            &envelope,
            0,
            GAIN_DETECT_PEAK_BINS,
            0.0,
            0.0,
            0.0,
            0.0,
            0,
            &mut lean_persistent,
            &mut lean_output,
            &mut lean_counts,
            &mut scratch,
            false,
        )
        .unwrap();
        assert_eq!(
            addresses,
            (
                scratch.gc_calls.as_ptr(),
                scratch.gc_records.as_ptr(),
                scratch.pool0_records.as_ptr(),
                scratch.pool2_records.as_ptr(),
                scratch.next_pool_records.as_ptr(),
                scratch.compact_candidates.as_ptr(),
                scratch.compact_visited.as_ptr(),
                scratch.compact_emitted.as_ptr(),
            )
        );
    }
}
