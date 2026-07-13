//! Scoped composition of `zeroth_bit_allocation_at5` (native 0x42360)
//! ATRAC3plus 352 kbps (`param_5 = 2`, selector `param_6 = 0x20`).
//!
//! The wiring order and the out-of-scope gates are recorded in
//! docs/06 ("Zeroth composition map"). The 44.1 kHz low-selector boost
//! ladder (selector `< 0x19`) is now ported for the stereo path
//! (docs/13 §3.2 slice 3; `zeroth_bit_allocation_at5` native 0x42360,
//! decompile 36322-36489). The 48 kHz tail-band weights, the ladder's
//! mono Block B/C weight arms, and the transient block
//! (`selector - 0xb < 9`, decompile 36528-36539) still never run on the
//! scoped path, and this composition rejects inputs that would need them
//! instead of guessing their behavior.

use crate::coding::allocation::{
    AllocationError, ZerothActiveBandCounts, ZerothActivitySummary, ZerothBandShape,
    ZerothFinalBitTotals, ZerothGainLevelBand, ZerothGainLocationBand, ZerothGainSideChannelCosts,
    ZerothGhaChannelFlags, ZerothInactiveZeroingMode, ZerothQuantBandInput, ZerothSideBitWords,
    ZerothToneSideBits, apply_zeroth_aux_weight_bonus_at5, apply_zeroth_stereo_cross_zero_at5,
    apply_zeroth_transient_boost_at5, compute_zeroth_active_band_counts_at5,
    compute_zeroth_base_weights_at5, compute_zeroth_flagged_base_weights_at5,
    compute_zeroth_gain_record_flags_at5, compute_zeroth_gha_bit_seed_at5,
    compute_zeroth_side_data_bit_seed_at5, copy_word_lengths_to_activity_at5,
    finalize_zeroth_band_shape_at5, round_and_clamp_word_lengths_at5,
    select_zeroth_inactive_zeroing_mode_at5, select_zeroth_wcfx_at5,
    zero_inactive_word_lengths_at5, zero_tone_span_inactive_word_lengths_at5,
    zeroth_band_activity_bits_at5, zeroth_final_bit_totals_at5, zeroth_gain_idlev_mode_at5,
    zeroth_gain_idlev_mode_ch1_at5, zeroth_gain_idloc_mode_at5, zeroth_gain_idloc_mode_ch1_at5,
    zeroth_gain_ngc_mode_at5, zeroth_gain_side_data_total_at5, zeroth_quant_table_selection_at5,
    zeroth_tone_side_bits_at5,
};
use crate::coding::bitcount::{
    BitcountError, IdctBlockState, IdctChannelState, IdsfBlockState, IdsfChannelState,
    calc_nbits_for_idct_at5, calc_nbits_for_idsf_ch_at5,
};
use crate::coding::quant::QuantError;
use crate::coding::quant_cost::QUANT_COST_CANDIDATES;

pub const ZEROTH_BANDS_AT5: usize = 32;
pub const ZEROTH_GAIN_BANDS_AT5: usize = 16;

/// Quant-plane length at `block+0xb08` (32 pick i32 words ++ 128 cost
/// u32 words, each cost word packing two consecutive LE u16 costs).
pub const ZEROTH_QUANT_PLANE_WORDS: usize = 160;
/// Word index where the cost region starts inside the plane
/// (`block+0xb88 - block+0xb08 == 0x80` bytes == 32 words).
pub const ZEROTH_QUANT_PLANE_COST_OFFSET: usize = 32;
/// IDCT state window length at `block+0x9f8` (mode / count / split +
/// 32 flags + 32 aux + the `+0xb04` tail word). See
/// `serialize_idct_object_range_a` (packer_bridge) for the same layout.
pub const ZEROTH_IDCT_WINDOW_WORDS: usize = 68;

#[derive(Debug, Clone, PartialEq)]
pub enum ZerothPassError {
    Allocation(AllocationError),
    Quant(QuantError),
    /// A composed `calc_nbits_for_idct_at5(0)` / `calc_nbits_for_idsf_ch_at5`
    /// leaf rejected the zeroth-time surface.
    Bitcount(BitcountError),
    /// The input would take a native branch outside the scoped
    /// 352 kbps 44.1 kHz path (see the docs/06 composition map).
    OutOfScope(&'static str),
}

impl From<AllocationError> for ZerothPassError {
    fn from(error: AllocationError) -> Self {
        ZerothPassError::Allocation(error)
    }
}

impl From<QuantError> for ZerothPassError {
    fn from(error: QuantError) -> Self {
        ZerothPassError::Quant(error)
    }
}

impl From<BitcountError> for ZerothPassError {
    fn from(error: BitcountError) -> Self {
        ZerothPassError::Bitcount(error)
    }
}

/// One 0x98-stride gain record row (word 0, words 1..7, words 8..15).
#[derive(Debug, Clone, Copy, Default)]
pub struct ZerothGainRecord {
    pub point_count: i32,
    pub locations: [i32; 7],
    pub levels: [i32; 8],
}

/// Per-band raw quant-selection inputs; the composition adds the
/// computed word length before calling the leaf.
#[derive(Debug, Clone, Copy)]
pub struct ZerothQuantBandRaw<'a> {
    pub spectrum: &'a [f32],
    pub idsf: usize,
    pub scale: f32,
    pub count: usize,
}

/// One channel's zeroth-pass input surface on the scoped path.
#[derive(Debug)]
pub struct ZerothChannelState<'a> {
    /// Integer idsf activity row at block `+0x4c`.
    pub idsf_activity: &'a [i32],
    /// Weight scale at block `+0x454`.
    pub weight_scale: f32,
    /// Auxiliary weights at block `+0x3cc`.
    pub aux_weights: &'a [f32],
    /// Per-band maximum word lengths at block `+0x2` (i16).
    pub max_word_lengths: &'a [i16],
    /// Per-band quant-selection inputs (spectrum window, idsf, scale,
    /// spec count).
    pub quant_bands: &'a [ZerothQuantBandRaw<'a>],
    /// Current gain records at `*(channel + 8)` (0x98 stride).
    pub gain_records: &'a [ZerothGainRecord],
    /// Trimmed gain band count at channel `+0x6d24`.
    pub gain_band_count: usize,
    /// Entry/trim flags at `+0x6d21`/`+0x6d22`, computed upstream.
    pub gha_flags: ZerothGhaChannelFlags,
    /// Band activity flags at records `+0x988..`.
    pub band_activity: &'a [i32],
    /// The i16 mode word at `*param_1[ch]` gating the relax rewrite
    /// (`*psVar11 < 7`).
    pub mode_word_0: i16,
    /// The channel's own scale-factor row at object `+0x1b678`
    /// (`param_1[band + 0x6d9e]`). Read by the `+0x11c` IDSF leaf
    /// (`calc_nbits_for_idsf_ch_at5`) on the enabled `side+0x8c == 1`
    /// path; `previous_scale_factors` for the leaf is channel 0's row.
    /// scale factors yet), but carried per channel so the leaf runs
    /// inside the pass rather than trace-fed.
    pub idsf_scale_factors: &'a [i32],
    /// Per-channel energy class at block `+0x42` (i16;
    /// `InitChannelOutput::class_42`). Read by the 44.1 low-selector boost
    /// ladder Block A (`zeroth_bit_allocation_at5` native 0x42360;
    /// decompile 36323-36340). Unused when the ladder gate is closed
    /// (selector `>= 0x19`).
    pub class_42: i16,
    /// Per-channel tonality at block `+0x458` (f32;
    /// `InitChannelOutput::tonality_458`). Read by the ladder Block C
    /// tonality bump (decompile 36455-36489). Unused when the ladder gate
    /// is closed.
    pub tonality_458: f32,
    /// Per-channel transient byte at block `+0x45c` (bool;
    /// `InitChannelOutput::transient_45c`, produced by init's Block N).
    /// Gates the Block D transient weight boost (decompile 36528-36539).
    /// Unused when the Block D selector gate is closed (selector outside
    /// field as `false`.
    pub transient_45c: bool,
}

/// How the zeroth `+0x11c` IDSF side-data bit count is produced
/// (decompile lines 36747-36779). The native gate is `side+0x8c`.
#[derive(Debug)]
pub enum ZerothIdsfInput<'a> {
    /// `side+0x8c == 0`: the disabled path — 2 bits per channel plus
    /// `active_count * 6` per channel (the native also zeroes the
    /// `+0x1c73c/4c/50` state words, which this composition does not
    /// model as they are unused downstream).
    Disabled,
    /// Synthetic-test path only: the per-channel `calc_nbits_for_idsf_ch_at5`
    /// returns are supplied directly rather than scored from rows. Never
    /// used by the replay or the bridge — those take `FromRows`.
    Precomputed(&'a [i32]),
    /// `side+0x8c == 1`: score the leaf inside the pass from each
    /// channel's `+0x1b678` scale-factor row (`ZerothChannelState::idsf_scale_factors`),
    /// with channel 0's row as the previous reference. `group_count` is
    /// the leaf group count at object `+0xb8` (10 on the 352 path).
    FromRows { group_count: usize },
}

/// Block-level zeroth-pass inputs.
#[derive(Debug)]
pub struct ZerothFrameState<'a> {
    pub channels: Vec<ZerothChannelState<'a>>,
    /// `piVar9[0x2d]`.
    pub band_count: usize,
    /// `piVar9[0x2f]` (tone group count; also the entry-flag scan cap).
    pub tone_group_count: usize,
    /// `param_6`.
    pub selector: u32,
    /// `piVar9[0x2b]`; the scoped path requires 44100.
    pub sample_rate: i32,
    /// Header flag byte at tone state `+0x1dc` (`piVar9[0x77]`).
    /// `& 0x7c` selects the weight seed: 0 takes the wcfx-table product,
    /// nonzero the `/10` alternate (decompile 36288 / 36493–36505).
    pub header_flags_1dc: u32,
    /// Object mode at `*(channel + 0x30) + 0x1c` (zeroing gate).
    pub object_mode_1c: u32,
    /// Tone activity float rows at `*(ch0 + 0x10) + 0x184`/`+0x284`.
    pub primary_tone_activity: &'a [f32],
    pub secondary_tone_activity: &'a [f32],
    /// Stereo cross-zero side flag rows at side `+0x94`/`+0xd4`
    /// (carried state, updated in place by the pass).
    pub cross_primary_flags: Vec<i16>,
    pub cross_secondary_flags: Vec<i16>,
    /// Quant-selection state index and `sa_nencodetbls[selector]`.
    pub quant_state: usize,
    pub quant_candidate_count: usize,
    /// Shared band count at `*(obj + 0xb4)` bounding the trim scan.
    pub shared_band_count_b4: usize,
    /// How the `+0x11c` IDSF side-data bits are produced (the
    /// `side+0x8c` gate). The enabled `FromRows` path scores the leaf
    /// inside the pass from `ZerothChannelState::idsf_scale_factors`.
    pub idsf_input: ZerothIdsfInput<'a>,
    /// IDCT `bandwidth_mode` (`g_a_idct_fixbits_at5` index = object
    /// `+0x90`; 1 on the 352 path). The `+0x11e` IDCT side-data bits are
    /// computed inside the pass by `calc_nbits_for_idct_at5(0)` over the
    /// just-computed word-length rows.
    pub idct_bandwidth_mode: usize,
    /// `calc_nbits_for_gha_at5` result (external boundary): the zeroth
    /// itself calls it with `param_3 == 1` (call site native 0x44c22).
    /// Modeled by `compute_gha_packing_prep` at composition level
    /// (bridge 1.5); still an input here pending the Phase-2.1 arena
    /// rotation.
    pub gha_bits: i16,
    /// Tone activity words at `piVar9[2..]` and `piVar9[0x14..]` for
    /// the `+0x120` block. The gate/count words the native epilogue
    /// reads (`piVar9[0x2c]`/`[0x30]`) are not inputs: the trim
    /// overwrites them with the active/grouped counts (decompile
    /// stores at native `0x436xx`; pinned live by
    /// `zeroth_io_trace.ndjson` tone-word entry/return deltas).
    pub tone_primary_words: &'a [i32],
    pub tone_secondary_words: &'a [i32],
    /// `piVar9[0x25]` for the 9/1 header word at `+0x128`.
    pub tone_flag_25: bool,
    /// `*(*(*param_3 + 0x30) + 0x14) == 0` (relax gate).
    pub relax_gate_zero: bool,
    /// `param_7`.
    pub frame_bit_budget: i32,
}

/// One channel's gain side-data outcome (mode words written when the
/// entry flag is set).
#[derive(Debug)]
pub struct ZerothGainChannelOutcome {
    pub ngc_mode: usize,
    pub ngc_bits: i32,
    pub idlev_mode: usize,
    pub idlev_bits: i32,
    pub idloc_mode: usize,
    pub idloc_bits: i32,
}

#[derive(Debug)]
pub struct ZerothChannelOutput {
    /// Final selected word lengths (row at `+0x1b5f8`, 32 entries).
    pub word_lengths: Vec<i32>,
    /// The block's i16 max word-length row (`+0x02`) after the relax
    /// rule; the native rewrite targets `((short *)param_1[ch])[i + 1]`,
    /// not the `+0x1b5f8` selection rows.
    pub max_word_lengths: Vec<i16>,
    /// The 32-word copy-back at `+0x14c`.
    pub activity_copy: Vec<i32>,
    /// Quant-table picks (`+0xb08` row) and the state-0 total at
    /// `+0x46` (slot 1 takes the `0x4000` sentinel).
    pub quant_picks: Vec<Option<usize>>,
    pub quant_state_total: u16,
    /// The serialized `block+0xb08` quant plane (160 u32: 32 pick i32
    /// words ++ 128 cost u32 words, each packing two consecutive LE u16
    /// costs). Skipped bands keep their zero-initialized state.
    pub plane_b08: Vec<u32>,
    /// The two `block+0x46` slot shorts: `[quant_state_total, 0x4000]`
    /// (state 0 real, slot 1 the native sentinel).
    pub slot_46: Vec<i16>,
    /// The serialized `block+0x9f8` IDCT state window (68 u32); see
    /// `ZEROTH_IDCT_WINDOW_WORDS`. All-zero when the active count is 0
    /// (the leaf's `+0xb0 > 0` gate returns without writing).
    pub idct_9f8: Vec<u32>,
    /// The per-channel zeroth base-weight vector (`block+0x1cc`,
    /// `f32(word_lengths_4c) * scaled_454 * g_wcfx[band]` plus the aux
    /// bonus over `weight_3cc`).
    pub base_weights_1cc: Vec<f32>,
    /// This channel's `calc_nbits_for_idsf_ch_at5` leaf return on the
    /// enabled path (`FromRows`), `None` on the disabled/precomputed
    /// paths. Cross-checked against the captured nested leaf returns.
    pub idsf_leaf_bits: Option<i32>,
    /// Gain side-data mode words, `None` when the entry flag is 0.
    pub gain: Option<ZerothGainChannelOutcome>,
    /// Band-activity summary words at records `+0x980`/`+0x984`.
    pub activity_summary: ZerothActivitySummary,
}

#[derive(Debug)]
pub struct ZerothFrameOutput {
    pub entry_flags: Vec<u32>,
    pub channels: Vec<ZerothChannelOutput>,
    pub cross_primary_flags: Vec<i16>,
    pub cross_secondary_flags: Vec<i16>,
    /// `piVar9[0x31]`/`piVar9[0x32]`.
    pub band_shape: ZerothBandShape,
    /// `+0xb0` and `+0xc0`.
    pub active_counts: ZerothActiveBandCounts,
    /// The seven side words at `+0x118..+0x124`.
    pub side: ZerothSideBitWords,
    /// Tone side-bit detail for `+0x120`.
    pub tone_side: ZerothToneSideBits,
    /// `+0x126/128/12a/12e` plus the relax flag.
    pub totals: ZerothFinalBitTotals,
    /// The native return value (the `+0x12e` total).
    pub return_bits: i16,
}

fn location_bands(records: &[ZerothGainRecord]) -> Vec<ZerothGainLocationBand<'_>> {
    records
        .iter()
        .map(|record| ZerothGainLocationBand {
            count: record.point_count.max(0) as usize,
            locations: &record.locations,
            levels: &record.levels,
        })
        .collect()
}

fn level_bands(records: &[ZerothGainRecord]) -> Vec<ZerothGainLevelBand<'_>> {
    records
        .iter()
        .map(|record| ZerothGainLevelBand {
            count: record.point_count.max(0) as usize,
            levels: &record.levels,
        })
        .collect()
}

/// Serialize the `block+0xb08` quant plane from one channel's per-band
/// picks and cost rows (decompile lines 36662-36725). Picks are 32 i32
/// words at `block+0xb08`; costs are 32 bands x 8 i16 at `block+0xb88`
/// (word offset 32), each 4-byte plane word packing two consecutive LE
/// u16 costs. Bands the native loop skips (`word_length < 1`) leave both
/// their pick word and their 4 cost words in their zero-initialized
/// state.
fn serialize_quant_plane(
    picks: &[Option<usize>],
    cost_rows: &[Option<[u16; QUANT_COST_CANDIDATES]>],
    band_count: usize,
) -> Vec<u32> {
    let mut plane = vec![0u32; ZEROTH_QUANT_PLANE_WORDS];
    for band in 0..band_count.min(ZEROTH_QUANT_PLANE_COST_OFFSET) {
        if let Some(pick) = picks.get(band).copied().flatten() {
            // `*(int *)(psVar11 + band*2) = local_340` — pick i32 word.
            plane[band] = pick as u32;
        }
        if let Some(costs) = cost_rows.get(band).and_then(|row| row.as_ref()) {
            // 8 i16 costs at `block+0xb88 + band*0x10` -> 4 plane words.
            let base = ZEROTH_QUANT_PLANE_COST_OFFSET + band * 4;
            for pair in 0..4 {
                let lo = costs[pair * 2];
                let hi = costs[pair * 2 + 1];
                plane[base + pair] = u32::from(lo) | (u32::from(hi) << 16);
            }
        }
    }
    plane
}

/// Serialize the `block+0x9f8` IDCT state window from the leaf's block
/// state (68 u32). Layout (bridge 1.6 `serialize_idct_object_range_a`
/// consumes the same offsets): `[mode @0x9f8][count @0x9fc][split
/// @0xa00][flags[32] @0xa04..0xa84][aux[32] @0xa84..0xb04][+0xb04 tail]`.
/// At zeroth time the aux rows and the `+0xb04` tail are zero (the leaf
/// leaves `block.aux`/`block.previous` untouched here).
fn serialize_idct_window(block: &IdctBlockState) -> Vec<u32> {
    let mut window = vec![0u32; ZEROTH_IDCT_WINDOW_WORDS];
    window[0] = block.mode; // +0x9f8
    window[1] = block.band_count as u32; // +0x9fc
    window[2] = block.split_flag; // +0xa00
    for band in 0..ZEROTH_BANDS_AT5 {
        window[3 + band] = block.flags[band]; // +0xa04 + band*4
    }
    // words 35..67 (+0xa84 aux, +0xb04 tail) stay zero at zeroth time.
    window
}

/// Block A of the 44.1 kHz low-selector boost ladder (`zeroth_bit_allocation_at5`
/// native 0x42360; decompile 36323-36340): the energy-class weight bump added
/// to weights[0..8]. `class_42` is the i16 at block `+0x42`. The `<= 3.3` /
/// class comparisons here are native decisions, matched in exact f32.
pub fn zeroth_ladder_class_bump(class_42: i16) -> f32 {
    if class_42 < 3 {
        if class_42 == 2 { 0.5 } else { 0.25 }
    } else {
        f32::from(class_42) * 0.125 + 0.75
    }
}

/// Block B of the ladder, the stereo 44.1 arm (decompile 36341-36357): the
/// selector-gated bump added to weights[0..8]. Only reached when the block is
/// stereo (`param_4 == 2`) and the rate is 44100. At the 160 selector (24 >=
/// 0x10) this is 0.5.
pub fn zeroth_ladder_stereo_bump(selector: u32) -> f32 {
    if selector < 0xe {
        0.7
    } else if selector < 0x10 {
        1.0
    } else {
        0.5
    }
}

/// Block C of the ladder, the tonality bump (decompile 36455-36489): applied to
/// weights[0..8] when the channel's computed gain-record entry flag is 0 and
/// the stereo/selector gate holds. `tonality_458` is the f32 at block `+0x458`.
/// The native nests the `<=` compares with 3.3 outermost, then 3.2, 3.1, 3.0,
/// 2.9; this is the equivalent flat mapping in exact f32 (the comparisons are
/// native decision boundaries — do not widen to f64).
pub fn zeroth_ladder_tonality_bump(tonality_458: f32) -> f32 {
    if tonality_458 <= 3.3 {
        if tonality_458 <= 3.2 {
            if tonality_458 <= 3.1 {
                if tonality_458 <= 3.0 {
                    if tonality_458 <= 2.9 { -0.75 } else { -0.5 }
                } else {
                    -0.25
                }
            } else {
                0.0
            }
        } else {
            0.25
        }
    } else {
        0.5
    }
}

/// Run the scoped zeroth pass. Word-length rows, side-data words, the
/// gain side-data modes, and the returned bit total mirror the native
/// stores documented in the docs/06 composition map.
pub fn zeroth_bit_allocation_frame_at5(
    state: &mut ZerothFrameState<'_>,
) -> Result<ZerothFrameOutput, ZerothPassError> {
    let channel_count = state.channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(AllocationError::UnsupportedChannelCount(channel_count).into());
    }
    // Config flag-word (`cfg+0x1dc & 0x7c`) weight-seed fork
    // (`zeroth_bit_allocation_at5`, native 0x42360; decompile 36288 reads the
    // word as `piVar9 + 0x77`, i.e. cfg word 0x77 == byte 0x1dc): flag==0
    // takes the wcfx-table product (+ the guarded 48000 sub-block and the
    // 44.1 low-selector boost ladder at 36322-36489); flag!=0 takes the `/10`
    // alternate `weight[i] = (idsf_activity[i] * scale_454) / 10.0` (decompile
    // 36493–36505) and SKIPS the wcfx table, the 48000 sub-branch, and the
    // 36322-36489 low-selector ladder (which lives inside the flag==0 branch).
    // Both branches rejoin at the common `+0x3cc` aux adder (decompile
    // 36507–36527), and then BOTH reach the transient bump block (Block D,
    // decompile 36528-36539), which is OUTSIDE the flag fork. docs/12 §4.3
    // b-residual; oracle: `syn_sweep_log_352/zeroth_io_flag_path_trace.ndjson`
    // (tone word 0x77 == 127 on the three captured mask-1 calls).
    let flags_1dc_set = state.header_flags_1dc & 0x7c != 0;
    if state.sample_rate != 44100 {
        return Err(ZerothPassError::OutOfScope(
            "non-44.1 kHz rates take the 48 kHz weight branches",
        ));
    }
    // The 44.1 low-selector boost ladder (decompile 36322-36489, gated
    // `param_6 < 0x19 && rate == 0xac44`) runs inside the flag==0 branch. Its
    // Block A (energy class) runs for any channel count; Blocks B and C have
    // separate stereo (`param_4 == 2`) and mono (`param_4 == 1`) weight arms
    // (36341-36453). Both channel modes are now ported for 44.1 kHz (the mono
    // Block B arm 36377-36408, the mono Block C gate `10 < sel`); the 48 kHz
    // arms (36359-36375 stereo, 36409-36453 mono) stay gated out by the rate
    // guard above. Wired per channel below (docs/14 §1.1, oracle
    // Block D transient weight boost (decompile 36528-36539) runs OUTSIDE
    // the flag fork, wired per channel below immediately after the common
    // `+0x3cc` aux adder. Its native gate is
    // `((param_4 == 2 && (param_6 - 0xb) < 9) || (param_6 == 9 && param_4
    // == 1)) && block+0x45c != 0` — the same stereo `0xb..=0x13` / mono `9`
    // selector window as init's Block N producer. Statically dead at the
    // 160/128/... selectors (24/23: `sel - 0xb >= 9`), so the byte is
    // `false` there and the boost is a no-op even with the byte fed.
    // Ported at 96 kbps (selector 19). See the per-channel wiring after the
    // aux adder.

    // Stage 2: gain-record entry flags (LAB_00052437).
    let point_count_rows: Vec<Vec<u32>> = state
        .channels
        .iter()
        .map(|channel| {
            channel
                .gain_records
                .iter()
                .map(|record| record.point_count.max(0) as u32)
                .collect()
        })
        .collect();
    let point_count_refs: Vec<&[u32]> = point_count_rows.iter().map(Vec::as_slice).collect();
    let entry_flags = compute_zeroth_gain_record_flags_at5(
        &point_count_refs,
        state.tone_group_count,
        channel_count,
    )?;

    // Stage 4: WCFX selection and per-channel weights.
    let wcfx = select_zeroth_wcfx_at5(channel_count, state.selector)?;
    let zeroing_mode = select_zeroth_inactive_zeroing_mode_at5(
        state.object_mode_1c,
        channel_count,
        state.selector,
    )?;

    let mut word_length_rows: Vec<Vec<i32>> = Vec::with_capacity(channel_count);
    let mut base_weight_rows: Vec<Vec<f32>> = Vec::with_capacity(channel_count);
    for (channel_index, channel) in state.channels.iter().enumerate() {
        let mut weights = vec![0.0f32; ZEROTH_BANDS_AT5];
        if flags_1dc_set {
            // Flag-set `/10` seed (decompile 36493–36505): no wcfx table, no
            // 48000 sub-branch, no 36322-36489 low-selector ladder.
            compute_zeroth_flagged_base_weights_at5(
                channel.idsf_activity,
                channel.weight_scale,
                &mut weights,
                state.band_count,
            )?;
        } else {
            compute_zeroth_base_weights_at5(
                channel.idsf_activity,
                channel.weight_scale,
                &wcfx.values,
                &mut weights,
                state.band_count,
            )?;
            // 44.1 kHz low-selector boost ladder (decompile 36322-36489). The
            // outer gate is `param_6 < 0x19 && rate == 0xac44`; the rate is
            // pinned to 44100 by the guard above, so the live gate here is
            // `selector < 0x19`. All three blocks add into weights[0..8] (a
            // fixed 8, independent of band_count) in native order.
            if state.selector < 0x19 {
                // Block A: energy-class bump (36323-36340).
                let class_bump = zeroth_ladder_class_bump(channel.class_42);
                for weight in weights.iter_mut().take(8) {
                    *weight += class_bump;
                }
                // Block B: the 44.1 selector bump. The stereo arm
                // (36341-36357) bumps weights[0..8] by one scalar; the mono arm
                // (36377-36408) has its own selector ladder over wider ranges.
                // The 48000 arms (36359-36375, 36409-36453) are gated out by
                // the rate guard above. Exact f32 constants (0.5 / 0.25 exact).
                if channel_count == 2 {
                    let stereo_bump = zeroth_ladder_stereo_bump(state.selector);
                    for weight in weights.iter_mut().take(8) {
                        *weight += stereo_bump;
                    }
                } else if state.selector < 0xc {
                    // sel < 0xc: weights[0..8] += 0.5, weights[8..14] += 0.25.
                    for weight in weights.iter_mut().take(8) {
                        *weight += 0.5;
                    }
                    for weight in weights.iter_mut().take(0xe).skip(8) {
                        *weight += 0.25;
                    }
                } else if state.selector < 0xe {
                    // sel < 0xe: weights[0..12] += 0.5.
                    for weight in weights.iter_mut().take(0xc) {
                        *weight += 0.5;
                    }
                } else {
                    // else (128 mono, sel 23, live): weights[0..8] += 0.5.
                    for weight in weights.iter_mut().take(8) {
                        *weight += 0.5;
                    }
                }
                // Block C: tonality bump (36455-36489). Gate:
                // `entry_flag[ch] == 0 && ((0xc < sel && stereo) || (10 < sel
                // && mono))`. At mono sel 23 this is LIVE (10 < 23).
                // `entry_flags` is the stage-2 gain-record flag (native
                // `local_fc[ch + 6]`) computed above.
                if entry_flags[channel_index] == 0
                    && ((0xc < state.selector && channel_count == 2)
                        || (10 < state.selector && channel_count == 1))
                {
                    let tonality_bump = zeroth_ladder_tonality_bump(channel.tonality_458);
                    for weight in weights.iter_mut().take(8) {
                        *weight += tonality_bump;
                    }
                }
            }
        }
        // Common `+0x3cc` aux adder (decompile 36507–36527), both branches.
        apply_zeroth_aux_weight_bonus_at5(channel.aux_weights, &mut weights, state.band_count)?;

        // Block D transient weight boost (decompile 36528-36539), OUTSIDE
        // the flag fork, immediately after the aux adder. Gate:
        // `((param_4 == 2 && (param_6 - 0xb) < 9) || (param_6 == 9 &&
        // param_4 == 1)) && block+0x45c != 0`. First live at 96 kbps
        // (selector 19); the byte is `false` at 160/128/... so the boost
        // is a no-op there even with the field fed.
        if ((channel_count == 2 && (0xb..0x14).contains(&state.selector))
            || (channel_count == 1 && state.selector == 9))
            && channel.transient_45c
        {
            apply_zeroth_transient_boost_at5(channel.aux_weights, &mut weights)?;
        }

        // Stage 5: round/clamp, inactive zeroing.
        let mut row = vec![0i32; ZEROTH_BANDS_AT5];
        round_and_clamp_word_lengths_at5(
            &weights,
            channel.max_word_lengths,
            &mut row,
            state.band_count,
        )?;
        base_weight_rows.push(weights);
        match zeroing_mode {
            ZerothInactiveZeroingMode::FullBandActivity => {
                zero_inactive_word_lengths_at5(channel.idsf_activity, &mut row, state.band_count)?;
            }
            ZerothInactiveZeroingMode::ToneGroupSpans => {
                zero_tone_span_inactive_word_lengths_at5(
                    state.primary_tone_activity,
                    (channel_count == 2).then_some(state.secondary_tone_activity),
                    channel.idsf_activity,
                    &mut row,
                    state.tone_group_count,
                )?;
            }
        }
        word_length_rows.push(row);
    }

    // Stage 5: stereo cross-zero on channel 1, then copy-back.
    if channel_count == 2 {
        let (left, right) = word_length_rows.split_at_mut(1);
        apply_zeroth_stereo_cross_zero_at5(
            &left[0],
            &mut right[0],
            &mut state.cross_primary_flags,
            &mut state.cross_secondary_flags,
            state.band_count,
        )?;
    }
    let mut activity_copies: Vec<Vec<i32>> = Vec::with_capacity(channel_count);
    for row in &word_length_rows {
        let mut copy = vec![0i32; ZEROTH_BANDS_AT5];
        copy_word_lengths_to_activity_at5(row, &mut copy)?;
        activity_copies.push(copy);
    }

    // Stage 5: band-shape finalization.
    let mut row_refs: Vec<&mut [i32]> =
        word_length_rows.iter_mut().map(Vec::as_mut_slice).collect();
    let band_shape = finalize_zeroth_band_shape_at5(
        &mut row_refs,
        state.band_count,
        state.band_count,
        state.tone_group_count,
    )?;

    // Stage 6: quant-table selection (slot 0 real, slot 1 sentinel). Also
    // serialize the `block+0xb08` plane (picks + cost rows) and the two
    // `block+0x46` slot shorts per channel.
    let mut quant_picks: Vec<Vec<Option<usize>>> = Vec::with_capacity(channel_count);
    let mut quant_totals: Vec<u16> = Vec::with_capacity(channel_count);
    let mut quant_planes: Vec<Vec<u32>> = Vec::with_capacity(channel_count);
    let mut slot_46_rows: Vec<Vec<i16>> = Vec::with_capacity(channel_count);
    for (channel, row) in state.channels.iter().zip(&word_length_rows) {
        if channel.quant_bands.len() < state.band_count {
            return Err(AllocationError::NspecsTooShort {
                needed: state.band_count,
                actual: channel.quant_bands.len(),
            }
            .into());
        }
        let bands: Vec<Option<ZerothQuantBandInput<'_>>> = (0..state.band_count)
            .map(|band| {
                if row[band] < 1 {
                    None
                } else {
                    let raw = &channel.quant_bands[band];
                    Some(ZerothQuantBandInput {
                        spectrum: raw.spectrum,
                        word_length: row[band] as usize,
                        idsf: raw.idsf,
                        scale: raw.scale,
                        count: raw.count,
                    })
                }
            })
            .collect();
        let selection = zeroth_quant_table_selection_at5(
            &bands,
            state.quant_state,
            state.quant_candidate_count,
        )?;
        quant_planes.push(serialize_quant_plane(
            &selection.picks,
            &selection.cost_rows,
            state.band_count,
        ));
        // `+0x46` slot 0 = the real state total, slot 1 = the 0x4000
        // sentinel the native writes for the unevaluated state.
        slot_46_rows.push(vec![
            selection.state_total as i16,
            crate::coding::allocation::ZEROTH_QUANT_STATE_SENTINEL as i16,
        ]);
        quant_picks.push(selection.picks);
        quant_totals.push(selection.state_total);
    }

    // Stage 7: active-count trim and the +0x118/11a/11c/11e seeds.
    let final_rows: Vec<&[i32]> = word_length_rows.iter().map(Vec::as_slice).collect();
    let active_counts = compute_zeroth_active_band_counts_at5(
        &final_rows,
        channel_count,
        state.shared_band_count_b4,
    )?;
    let seed =
        compute_zeroth_side_data_bit_seed_at5(channel_count, band_shape.word_length_count as u32)?;

    // Stage 7: the `+0x11c` IDSF side-data bits (decompile 36747-36779).
    // Zero when the active count is 0; else 2 bits per channel plus the
    // per-channel path selected by `side+0x8c`.
    let active_band_count = active_counts.active_band_count;
    let mut idsf_leaf_bits: Vec<Option<i32>> = vec![None; channel_count];
    let idsf_bits_11c = if active_band_count == 0 {
        0
    } else {
        match &state.idsf_input {
            ZerothIdsfInput::Disabled => {
                // Disabled path: 2 bits per channel plus
                // active_count * 6 per channel (the native also zeroes
                // the +0x1c73c/4c/50 state words).
                let mut bits = (channel_count as i32).wrapping_mul(2);
                for _ in 0..channel_count {
                    bits = bits.wrapping_add(active_band_count as i32 * 6);
                }
                bits as u16 as i16
            }
            ZerothIdsfInput::Precomputed(bits) => {
                if bits.len() < channel_count {
                    return Err(AllocationError::IdsfBitsTooShort {
                        needed: channel_count,
                        actual: bits.len(),
                    }
                    .into());
                }
                let mut total = (channel_count as i32).wrapping_mul(2);
                for value in &bits[..channel_count] {
                    total = total.wrapping_add(*value);
                }
                total as u16 as i16
            }
            ZerothIdsfInput::FromRows { group_count } => {
                // Enabled path: score `calc_nbits_for_idsf_ch_at5` per
                // channel. `mode` = channel index (ch0 fresh, ch1
                // previous), `scale_factors` = own +0x1b678 row,
                // `previous_scale_factors` = ch0's row, `band_count` =
                // active count, `group_count` = object +0xb8.
                let previous = state.channels[0].idsf_scale_factors;
                let mut total = (channel_count as i32).wrapping_mul(2);
                for (index, channel) in state.channels.iter().enumerate() {
                    let mut block = IdsfBlockState::default();
                    let leaf = calc_nbits_for_idsf_ch_at5(
                        &IdsfChannelState {
                            mode: index as u32,
                            band_count: active_band_count,
                            group_count: *group_count,
                            scale_factors: channel.idsf_scale_factors,
                            previous_scale_factors: previous,
                        },
                        &mut block,
                    )?;
                    // The zeroth-time IdsfBlockState is discarded: adjust
                    // (bridge 1.4) re-runs the leaf last before pack and
                    // owns that surface.
                    idsf_leaf_bits[index] = Some(leaf);
                    total = total.wrapping_add(leaf);
                }
                total as u16 as i16
            }
        }
    };

    // Stage 7: the `+0x11e` IDCT side-data bits and the block+0x9f8 IDCT
    // window (decompile 36780-36781). The leaf runs with selector 0 over
    // the just-computed word-length rows: `idct_source` = own row,
    // `previous_idct_source` = channel 0's row, `mode` = channel index,
    // `bandwidth_mode` = object +0x90. When the active count is 0 the
    // leaf's `+0xb0 > 0` gate returns 0 and leaves the window all-zero.
    let idct_source_rows: Vec<Vec<u32>> = word_length_rows
        .iter()
        .map(|row| row.iter().map(|&value| value as u32).collect())
        .collect();
    let idct_previous: Vec<u32> = word_length_rows[0]
        .iter()
        .map(|&value| value as u32)
        .collect();
    let idct_channels: Vec<IdctChannelState<'_>> = (0..channel_count)
        .map(|index| IdctChannelState {
            mode: index as u32,
            bandwidth_mode: state.idct_bandwidth_mode,
            band_count: active_band_count,
            idct_source: &idct_source_rows[index],
            previous_idct_source: &idct_previous,
        })
        .collect();
    let mut idct_blocks: Vec<IdctBlockState> = (0..channel_count)
        .map(|_| IdctBlockState::default())
        .collect();
    let idct_bits_11e = calc_nbits_for_idct_at5(&idct_channels, &mut idct_blocks, 0)? as i16;
    let idct_windows: Vec<Vec<u32>> = idct_blocks.iter().map(serialize_idct_window).collect();

    // Stage 8: gain side data.
    let gha_flags: Vec<ZerothGhaChannelFlags> = state
        .channels
        .iter()
        .map(|channel| channel.gha_flags)
        .collect();
    let gain_seed = compute_zeroth_gha_bit_seed_at5(channel_count, &gha_flags)?;
    let mut gain_outcomes: Vec<Option<ZerothGainChannelOutcome>> =
        Vec::with_capacity(channel_count);
    let mut gain_costs: Vec<ZerothGainSideChannelCosts> = Vec::with_capacity(channel_count);
    for (index, channel) in state.channels.iter().enumerate() {
        // The gain scoring gate is the input `+0x6d21` word (decompile
        // line 36800), not the locally computed entry flags — those
        // only feed the out-of-scope low-selector weight ladder.
        if !channel.gha_flags.has_nonzero_band {
            gain_outcomes.push(None);
            gain_costs.push(ZerothGainSideChannelCosts {
                active: false,
                ngc_bits: 0,
                idlev_bits: 0,
                idloc_bits: 0,
            });
            continue;
        }
        let bands = channel.gain_band_count;
        let counts: Vec<i32> = channel.gain_records[..bands]
            .iter()
            .map(|record| record.point_count)
            .collect();
        let levels = level_bands(&channel.gain_records[..bands]);
        let locations = location_bands(&channel.gain_records[..bands]);
        let outcome = if index == 0 {
            let ngc = zeroth_gain_ngc_mode_at5(&counts, None)?;
            let idlev = zeroth_gain_idlev_mode_at5(&levels)?;
            let idloc = zeroth_gain_idloc_mode_at5(&locations)?;
            ZerothGainChannelOutcome {
                ngc_mode: ngc.mode,
                ngc_bits: ngc.candidates[ngc.mode],
                idlev_mode: idlev.mode,
                idlev_bits: idlev.candidates[idlev.mode],
                idloc_mode: idloc.mode,
                idloc_bits: idloc.candidates[idloc.mode],
            }
        } else {
            let reference = &state.channels[0];
            let reference_counts: Vec<i32> = reference.gain_records[..bands]
                .iter()
                .map(|record| record.point_count)
                .collect();
            let reference_levels = level_bands(&reference.gain_records[..bands]);
            let reference_locations = location_bands(&reference.gain_records[..bands]);
            let ngc = zeroth_gain_ngc_mode_at5(&counts, Some(&reference_counts))?;
            let idlev = zeroth_gain_idlev_mode_ch1_at5(&levels, &reference_levels)?;
            let idloc = zeroth_gain_idloc_mode_ch1_at5(&locations, &reference_locations)?;
            ZerothGainChannelOutcome {
                ngc_mode: ngc.mode,
                ngc_bits: ngc.candidates[ngc.mode],
                idlev_mode: idlev.mode,
                idlev_bits: idlev.candidates[idlev.mode],
                idloc_mode: idloc.mode,
                idloc_bits: idloc.candidates[idloc.mode],
            }
        };
        gain_costs.push(ZerothGainSideChannelCosts {
            active: true,
            ngc_bits: outcome.ngc_bits,
            idlev_bits: outcome.idlev_bits,
            idloc_bits: outcome.idloc_bits,
        });
        gain_outcomes.push(Some(outcome));
    }
    let gain_total = zeroth_gain_side_data_total_at5(i32::from(gain_seed.gha_bits), &gain_costs);

    // Stage 8: the +0x122 band-activity and +0x120 tone blocks.
    let activity_rows: Vec<&[i32]> = state
        .channels
        .iter()
        .map(|channel| channel.band_activity)
        .collect();
    let (activity_summaries, activity_bits_122) =
        zeroth_band_activity_bits_at5(&activity_rows, band_shape.group_count)?;
    // The native `+0x120` block reads `piVar9[0x2c]`/`[0x30]` after
    // the trim stored the active band count and the grouped count
    // there, so the tone gate and group width use the computed counts.
    let tone_side = zeroth_tone_side_bits_at5(
        active_counts.active_band_count as i32,
        active_counts.group_count as i32,
        channel_count,
        state.tone_primary_words,
        state.tone_secondary_words,
    )?;

    // Stage 8: epilogue totals and the relax rule. The per-channel
    // extras are the quant state totals just written to the +0x46
    // slot selected by the (zeroed) +0x1074 index.
    let side = ZerothSideBitWords {
        mode_bits_118: seed.mode_bits_118,
        idwl_bits_11a: seed.idwl_bits_11a,
        idsf_bits_11c,
        idct_bits_11e,
        tone_bits_120: tone_side.bits_120,
        activity_bits_122,
        gain_bits_124: gain_total.gain_bits_124,
    };
    let extras: Vec<i16> = quant_totals.iter().map(|total| *total as i16).collect();
    // The relax rewrite targets the block's i16 head row
    // (`psVar11 = (short *)param_1[ch]`): the mode word at `+0x00`
    // gates on `< 7` and the band words at `+0x02..` (the max
    // word-length row) are forced to 7. The `+0x1b5f8` selection rows
    // are untouched (pinned live by `zeroth_io_trace.ndjson` call 7,
    // where the budget condition holds but the selection rows stay).
    let mut relax_rows: Vec<Vec<i16>> = state
        .channels
        .iter()
        .map(|channel| {
            let mut view = Vec::with_capacity(channel.max_word_lengths.len() + 1);
            view.push(channel.mode_word_0);
            view.extend_from_slice(channel.max_word_lengths);
            view
        })
        .collect();
    let mut relax_refs: Vec<&mut [i16]> = relax_rows.iter_mut().map(Vec::as_mut_slice).collect();
    let totals = zeroth_final_bit_totals_at5(
        &side,
        state.gha_bits,
        state.tone_flag_25,
        &extras,
        state.relax_gate_zero,
        state.frame_bit_budget,
        &mut relax_refs,
        // The relax loop bound is the prologue band count
        // (`piVar9[0x2d]`), not the finalized word-length count.
        state.band_count,
    )?;

    let channels = state
        .channels
        .iter()
        .enumerate()
        .map(|(index, _)| ZerothChannelOutput {
            word_lengths: word_length_rows[index].clone(),
            max_word_lengths: relax_rows[index][1..].to_vec(),
            activity_copy: activity_copies[index].clone(),
            quant_picks: quant_picks[index].clone(),
            quant_state_total: quant_totals[index],
            plane_b08: quant_planes[index].clone(),
            slot_46: slot_46_rows[index].clone(),
            idct_9f8: idct_windows[index].clone(),
            base_weights_1cc: base_weight_rows[index].clone(),
            idsf_leaf_bits: idsf_leaf_bits[index],
            gain: gain_outcomes[index].take(),
            activity_summary: activity_summaries[index],
        })
        .collect();

    Ok(ZerothFrameOutput {
        entry_flags,
        channels,
        cross_primary_flags: state.cross_primary_flags.clone(),
        cross_secondary_flags: state.cross_secondary_flags.clone(),
        band_shape,
        active_counts,
        side,
        tone_side,
        totals,
        return_bits: totals.extended_total_12e,
    })
}

/// One channel's read-only surface for the joint/intensity-stereo producer.
/// Every slice is indexed by unit (0..32); the first 32 entries are used.
#[derive(Debug, Clone, Copy)]
pub struct JointStereoChannelInput<'a> {
    /// 6-bit scale factors at object `+0x1b678` (`local_ec`/`local_e8`).
    pub scale_factors: &'a [i32],
    /// Auxiliary weights at block `+0x3cc` (`local_fc[2]`/`local_fc[3]`).
    pub aux_weights: &'a [f32],
    /// Per-unit spectrum at arena `+0xa48` used by the a48 correlation gate.
    pub spectrum: &'a [f32],
}

/// All entry inputs the `param_5 == 3` producer arm reads (decompile
/// 36016-36266). Band-indexed rows carry 16 entries; unit-indexed rows carry
/// 32. `channels[0]`/`channels[1]` are `param_3[0]`/`param_3[1]`.
#[derive(Debug, Clone, Copy)]
pub struct JointStereoProducerInput<'a> {
    /// `param_5` (`handle+0x190` config word, `mode_a`). The arm runs only
    /// when this is 3 (48-256 kbps); 320/352 pass 2 and skip it wholesale.
    pub param_5: u32,
    /// `param_6` selector; picks the `sa_tc_*`/`sa_mm_*` table pair.
    pub selector: u32,
    /// `tone_activity[0]` (`local_334`): subloop_1/2/3 band bound and the
    /// final-masking loop start.
    pub band_count: usize,
    /// `tone_state+0xbc` (`local_32c`): subloop_4/5 band bound and the
    /// final-masking loop end.
    pub tone_state_bc: usize,
    /// `tone_activity[band+1]` (`side+4` gate word), 16 bands.
    pub gate_04: &'a [i32],
    /// `tone_activity[0x21+band]` energy (f32), 16 bands.
    pub energy_84: &'a [f32],
    /// `tone_activity[0x131+band]` masking margin (f32), 16 bands.
    pub masking2_4c4: &'a [f32],
    /// `side+4` floor threshold (i32), 32 units (pre-offset; index by unit).
    pub side_04: &'a [i32],
    /// The two channels' read-only surfaces (`param_3[0]`, `param_3[1]`).
    pub channels: [JointStereoChannelInput<'a>; 2],
}

/// The producer arm's decision outputs (leaf-parity target).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointStereoProducerOutput {
    /// `side+0x94` per-unit intensity/joint flags (32 i16).
    pub shared_row_94: [i16; 32],
    /// `tone_state+8` per-band join array (16 i32).
    pub band_join: [i32; 16],
    /// Each channel's `+0x1b678` scale-factor row after the merge sub-arm
    /// (32 i32 each; equals the input row when no `|sf0-sf1| == 1` adjacency
    /// is merged).
    pub scale_factors: [[i32; 32]; 2],
}

/// x87 `ROUND`/`rint` (round half to even), then integer-truncate. Matches the
/// two `(int)ROUND(...)` casts at decompile 36087-36088 / 36139. Widening the
/// f32 operand to f64 first is exact, so the decision is x87-faithful.
fn zeroth_joint_round(value: f64) -> i32 {
    value.round_ties_even() as i32
}

/// Table pair selection for the producer arm (decompile 36034-36049):
/// `sel>=0x1b`→256, `0x13<=sel<0x1b`→096, `0xd<=sel<0x13`→064, else 032.
/// `sa_tc_*` is per-band (16 f32); `sa_mm_*` is per-unit (32 i32).
fn zeroth_joint_tables(selector: u32) -> ([f32; 16], [i32; 32]) {
    use crate::tables::at5;
    if selector >= 0x1b {
        (at5::tc_256_at5(), at5::mm_256_at5())
    } else if selector >= 0x13 {
        (at5::tc_096_at5(), at5::mm_096_at5())
    } else if selector >= 0xd {
        (at5::tc_064_at5(), at5::mm_064_at5())
    } else {
        (at5::tc_032_at5(), at5::mm_032_at5())
    }
}

/// The ATRAC3plus joint/intensity-stereo **producer leaf**: the
/// `param_5 == 3` arm of `zeroth_bit_allocation_at5` (native `0x42360`;
/// arm entry `0x424a6`; decompile `libatrac.c` lines 36016-36266). At
/// 48-256 kbps native turns on per-band joint/intensity stereo through this
/// arm; at 320/352 (`param_5 == 2`) it is dead, which is why those rates
/// already pass. This function ports the arm as a leaf (rung-2 parity); it is
/// now composed into the 48–256 kbps encode pipelines (docs/13 §2.3–§5.2).
///
/// It reproduces the arm's two decision outputs — `side+0x94` per-unit
/// intensity flags (`shared_row_94`) and `tone_state+8` per-band join
/// (`band_join`) — plus the merge sub-arm's mutation of the two channels'
/// `+0x1b678` scale-factor rows. The seven sub-loops run in native order:
///
/// 1. subloop_1 tone-equality (36050-36073), band `1..band_count`.
/// 2. word-length estimate `local_9c` (36074-36102), band `0..band_count`.
/// 3. subloop_3a gate (36103-36126), band `0..band_count`.
/// 4. subloop_3b energy-clear (36127-36148), band `0..band_count`.
/// 5. subloop_4 band-join tree + a48 (36150-36221), band `0..tone_state_bc`.
/// 6. subloop_5 merge (36222-36252), band `0..tone_state_bc`.
/// 7. final masking loop (36254-36265), band `band_count..tone_state_bc`.
///
/// The per-line spectrum sign-flip at 36209-36214 is not applied inside this
/// leaf (it mutates spectrum bytes as a downstream intensity CONSUMER effect,
/// not a leaf-parity output). It is now applied at the bridge/composition layer:
/// `assemble_calc_frame_entry_with_init_for_params_at5` flips ch1's normalized-
/// spectrum lines for every unit of a band whose subloop_4 join fired (exactly
/// `band_join[band] != 0`), matching decompile 36209-36214. Note the file header
/// above states the `param_5 == 3` masking block "never runs on the scoped [352]
/// path" — still true: this leaf is unwired at 320/352, live only at 48-256.
///
/// Validated against the `atx_zeroth_joint_replay_v1` replay oracle
/// `tests/native_traces.rs::zeroth_joint_stereo_producer_matches_replay_256`.
/// The arm reproduces `band_join` and the scale-factor rows for all six oracle
/// calls, and `shared_row_94` for the five non-priming calls. On the silent
/// `anti:2` priming frame the arm emits units 26-31, but the oracle captured
/// `side+0x94` AFTER the separate cross-zero consumer
/// (`apply_zeroth_stereo_cross_zero_at5`, native 36607-36628) masks every unit
/// whose computed word length is 0 — all of them on a silent frame — so the
/// captured `x94_return` is all-zero. That consumer is out of this producer
/// leaf's scope; the test documents the relationship.
pub fn zeroth_joint_stereo_producer_at5(
    input: &JointStereoProducerInput<'_>,
) -> JointStereoProducerOutput {
    let mut shared_row_94 = [0i16; 32];
    let mut band_join = [0i32; 16];
    let mut scale_factors = [[0i32; 32]; 2];
    scale_factors[0].copy_from_slice(&input.channels[0].scale_factors[..ZEROTH_BANDS_AT5]);
    scale_factors[1].copy_from_slice(&input.channels[1].scale_factors[..ZEROTH_BANDS_AT5]);

    // Gate: the arm is skipped wholesale unless param_5 == 3 (mode_a). All
    // outputs stay at entry state (x94/band_join zero, sf == input).
    if input.param_5 != 3 {
        return JointStereoProducerOutput {
            shared_row_94,
            band_join,
            scale_factors,
        };
    }

    let y = crate::tables::at5::y_at5();
    let (tc, mm) = zeroth_joint_tables(input.selector);
    let aux0 = input.channels[0].aux_weights;
    let aux1 = input.channels[1].aux_weights;
    let spec0 = input.channels[0].spectrum;
    let spec1 = input.channels[1].spectrum;
    let side = input.side_04;
    let energy = input.energy_84;
    let masking = input.masking2_4c4;
    let gate = input.gate_04;
    let band_count = input.band_count;
    let tone_state_bc = input.tone_state_bc;
    let unit_range = |band: usize| (y[band] as usize)..(y[band + 1] as usize);

    // 1. subloop_1 tone-equality (36050-36073): band 1..band_count.
    for band in 1..band_count {
        if tc[band] < energy[band] {
            for unit in unit_range(band) {
                if scale_factors[0][unit] == scale_factors[1][unit]
                    && scale_factors[0][unit] >= side[unit]
                {
                    shared_row_94[unit] = 1;
                }
            }
        }
    }

    // 2. word-length estimate local_9c (36074-36102): band 0..band_count.
    let mut local_9c = [0i32; 32];
    for band in 0..band_count {
        for unit in unit_range(band) {
            let weight = aux0[unit].max(aux1[unit]);
            let word_length = (zeroth_joint_round(f64::from(weight) * 0.333) + mm[unit])
                - zeroth_joint_round(f64::from(energy[band]) * 0.125);
            local_9c[unit] = word_length.max(3);
        }
    }

    // 3. subloop_3a gate (36103-36126): band 0..band_count, gate_04 != 0.
    for band in 0..band_count {
        if gate[band] != 0 {
            for unit in unit_range(band) {
                if shared_row_94[unit] == 0 {
                    let max_sf = scale_factors[0][unit].max(scale_factors[1][unit]);
                    if local_9c[unit] <= max_sf - side[unit] {
                        shared_row_94[unit] = 1;
                    }
                }
            }
        }
    }

    // 4. subloop_3b energy-clear (36127-36148): band 0..band_count.
    for band in 0..band_count {
        if energy[band] < 40.0 {
            for unit in unit_range(band) {
                let weight = aux0[unit].max(aux1[unit]);
                if zeroth_joint_round(f64::from(weight)) > 6 {
                    shared_row_94[unit] = 0;
                }
            }
        }
    }

    // 5. subloop_4 band-join tree + a48 (36150-36221): band 0..tone_state_bc.
    for band in 0..tone_state_bc {
        let start = y[band] as usize;
        let end = y[band + 1] as usize;
        let first = (scale_factors[0][start] - scale_factors[1][start]).abs();
        let mut min_diff = first;
        let mut max_diff = first;
        for unit in (start + 1)..end {
            let diff = (scale_factors[0][unit] - scale_factors[1][unit]).abs();
            if diff < min_diff {
                min_diff = diff;
            }
            if diff > max_diff {
                max_diff = diff;
            }
        }
        let join = if energy[band] >= -11.0 {
            0
        } else if max_diff < 2 {
            if masking[band] >= -11.0 {
                i32::from(max_diff == min_diff)
            } else {
                1
            }
        } else {
            0
        };
        band_join[band] = join;
        if join != 0 {
            for unit in start..end {
                // fabs((double)(spec0 - spec1)); the f32->f64 widen is exact,
                // matching the x87 80-bit subtraction for the < 1.0 decision.
                if (f64::from(spec0[unit]) - f64::from(spec1[unit])).abs() < 1.0 {
                    shared_row_94[unit] = 1;
                }
            }
        }
    }

    // 6. subloop_5 merge (36222-36252): band 0..tone_state_bc; reads x94 as of
    // after step 5 and mutates adjacent (|sf0-sf1| == 1) scale factors.
    for band in 0..tone_state_bc {
        if band_join[band] != 0 || energy[band] >= 60.0 {
            for unit in unit_range(band) {
                if shared_row_94[unit] != 0 {
                    let sf0 = scale_factors[0][unit];
                    let sf1 = scale_factors[1][unit];
                    if sf0 == sf1 + 1 {
                        scale_factors[0][unit] = sf1;
                    } else if sf0 + 1 == sf1 {
                        scale_factors[1][unit] = sf0;
                    }
                }
            }
        }
    }

    // 7. final masking loop (36254-36265): band band_count..tone_state_bc.
    // Strict `>` (unlike the `>=` in step 5); joins the top units (30,31 at
    // 256) on every live-content call.
    for band in band_count..tone_state_bc {
        if masking[band] > -11.0 {
            for unit in unit_range(band) {
                shared_row_94[unit] = 1;
            }
        }
    }

    JointStereoProducerOutput {
        shared_row_94,
        band_join,
        scale_factors,
    }
}
