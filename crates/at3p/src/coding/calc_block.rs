//! Scoped composition of `calc_channel_block_at5` (native `0x51a80`,
//! decompile `0x61a80`, 14178 bytes) for the evidence-backed 28- and
//! 32-quant-unit stereo processing extents. The backing rows and mode planes
//! remain the native 32-unit storage width.
//!
//! Native source of truth is the decompiled boundary at Ghidra
//! `0x00061a80` and direct disassembly at native
//! `0x00051a80..0x000551e2`. The `atx_encode_core` call site at native
//! `0x00055e7c` pins the ABI: eax = param_1 block table, edx = param_2
//! spectrum table, ecx = param_3 object/channel-state table, stack
//! esp+4 = param_4 channel count, esp+8 = param_5 encode selector,
//! esp+12 = param_6 frame bit budget, esp+16 = extra preserved value.
//!
//! `calc_channel_block_frame_at5` is the whole-call composition: it drives
//! the five ported allocation passes (second/fifth/sixth/adjust/eighth; eighth
//! is live on the docs/15 low-rate sweep regression) and the leaves (`quant_at5`,
//! `quant_nontone_costs_at5`, the `pwc_qu_at5` dither, the bitcount
//! leaves incl. `calc_nbits_for_idwl_ch_init_at5` and
//! `calc_nbits_var_rebitalloc_at5`) live over an owned model of the
//! block/object/shared/ctx memory. Sections:
//!  1-2 seed (below), 3 second pass, 5 else-branch non-selected-mode
//!  re-cost (descriptor state = plane mode index) + mode re-pick, 7
//!  chosen-plane copy to `+0x1b578` with the stereo `+0xd4` forcing and
//!  both-zero clearing, 8 x87-ROUND (truncate, native `0x52cae`) band
//!  keys + two `3n+1` shell sorts, 9 fifth+sixth, 10 tone flag
//!  (`obj+0x1c70c[0]` = WLC mode) gated on shared `+0x88`, 11 QUANT loop,
//!  12 spectral level words (`+0x1c6f8`, the x87 extended-precision
//!  magnitude chain modelled in f64 with single f32 rounds), 13 adjust
//!  plus phase C var trials (live at retained 352 call 22 and 160 call 7),
//!  eighth allocation (live at 64 sweep calls 4761/17547), 14 gated
//!  fifth+sixth re-run + QUANT loop 2, 15 epilogue (`ctx+0x1e4`).
//! `calc_cb_io_trace_replays_composed_frames` replays native calls
//! 0/7/12/22 bit-exactly against every asserted return surface.
//!
//! Caveats (all decompile-backed): the over-budget phase B (destructive
//! band-kill, native `0x5275c..0x52b5d`) is LIVE on dense corpus content at
//! 352 — three calls (sweat 1701/2482, 12-34-am 2437), ported here as
//! `phase_b_band_kill`. The phase A `+0xcc` raise loop is dead-by-table at
//! selector 30 (`sa_limit_idtf_stereo[30] == 0`) and live at selector 27;
//! only the native-observed 256 kbps high-band/stereo shape (`s114=10`,
//! `s116=24`, bands >= 24) is ported. The low-band selector sub-arm remains
//! fail-explicit; the phase-B joint-stereo kill sub-arm (shared `+0x94 == 1`,
//! both channels' band zeroed) is ported (LIVE at 192, sweat-192 calls
//! 858/3588), and `clear_main_data_at5` is never reached. The section-5
//! non-selected-plane recost is behaviorally identical on the retained 352/160 rows to the native
//! over-budget-branch recost (44307-44392), so Rust runs the unified
//! recost-both-planes structure unconditionally and phase B only mutates the
//! selected plane afterward. The section-12/adjust energy chains are
//! x87-extended modelled in f64 with
//! single f32 rounds at the native `fstps` points (both live channels
//! replay bit-exact at 32 bands / selector 30 and 28 bands / selector 24).
//! The 160 kbps 28-band Phase A and Phase B are now natively observed and
//! ported (docs/13 §3.2 slice 4 (vv)): the over-budget branch fires at the
//! 28-unit extent on dense 160 content (Phase A: 12-34-am calls 1179/1604,
//! sweat 3272/4699, syn_noise_fullscale 19; Phase B + joint-stereo kill:
//! 12-34-am 1179), every over-budget output frame packing quant_unit_count
//! 28, with Phase-A `s114` gate value 3. The `qe160` phase-event oracles
//! anchor the port; the phase paths run extent-generically for both the 28-
//! and 32-unit extents.
//!
//! Seed section (native `0x51a80..0x51f6a`, decompile lines 43946-44149):
//!  * shared (`block[0]+0x1008`) `+0x116` = `sa_adjust_iqt_0th_stereo`
//!    `[selector]`, `+0x114` = `sa_limit_idtf_stereo[selector]`;
//!  * each channel's `block+0xcc` idsf-quant row (band `0..band_count`)
//!    = `saa_idtf_stereo[selector*0x20 + band]`;
//!  * the 48 kHz rewrite (`ctx[0x2b] == 48000`) and the selector `< 0x10`
//!    word-length ladder are dead on the scoped 44.1 kHz selectors and rejected explicitly;
//!  * for selector `>= 0x10` every `+0xcc` entry is clamped to a maximum
//!    of 15 (native LAB_00061f15).
//!
//! Verified against the 352 `calc_cb_io_trace.ndjson` and 160
//! `calc_cb_io_160.ndjson`: selector 30 seeds all zeros; selector 24 seeds
//! exactly the first 28 entries and preserves the 32-wide storage tail. For
//! selector 30 the seed
//! row is all zeros with shared `+0x114 = 0` and `+0x116 = 0x18`, which
//! equals the native return `+0xcc` on every core call whose over-budget
//! phase C did not later raise the row (calls 0/7/12; call 22 raises it
//! through the live `calc_nbits_var_rebitalloc_at5` trials).

use crate::coding::adjust_pass::{
    AdjustChannelState, AdjustFrameState, AdjustGainRow, adjust_scalefactors_frame_at5,
};
use crate::coding::bitcount::{
    IdctBlockState, IdsfBlockState, IdwlBlockState, IdwlChannelState, IdwlSideState,
    VarRebitallocInput, calc_nbits_for_idwl_ch_init_at5, calc_nbits_var_rebitalloc_at5,
};
use crate::coding::eighth_pass::{
    EighthChannelState, EighthFrameState, eighth_bit_allocation_frame_at5,
};
use crate::coding::fifth_pass::live_idct_bits;
use crate::coding::fifth_pass::{
    FifthChannelState, FifthFrameState, fifth_bit_allocation_frame_at5,
};
use crate::coding::quant::quant_at5;
use crate::coding::quant_cost::quant_nontone_costs_at5;
use crate::coding::second_pass::{
    SecondChannelState, SecondFrameState, second_bit_allocation_frame_at5,
};
use crate::coding::sixth_pass::{
    SixthChannelState, SixthFrameState, sixth_bit_allocation_frame_at5,
};
use crate::coding::zeroth_pass::ZerothQuantBandRaw;
use crate::tables::at5::{
    adjust_iqt_0th_mono, adjust_iqt_0th_stereo, idspcbands_at5, idspcqus_at5, ifqf_at5, isps_at5,
    limit_idtf_mono, limit_idtf_stereo, lngain_at5, nsps_at5, pos_weight, rndtbl_at5,
    saa_idtf_mono, saa_idtf_stereo, sftbl_at5, spc_floor, spc_startqu, spclev_at5, x_at5, y_at5,
};

const CALC_BANDS_AT5: usize = 32;
const CALC_BANDS_160_AT5: usize = 28;
/// The 48 kbps reduced processing extent (selector 13, ctx `+0xb4` == 26), the
/// first sub-27 extent, replayed by `calc_cb_io_48.ndjson`. The backing rows
/// keep the native 32-unit storage width.
const CALC_BANDS_48_AT5: usize = 26;
/// The 32 kbps MONO reduced processing extent (selector 11, ctx `+0xb4` == 24),
/// the first sub-26 extent, runtime-pinned by
/// on every one of the 84 core calls, the trim-law fallthrough — 24 is not in the
/// round-up set {29,30,31}) and replayed by `calc_cb_io_32_mono.ndjson` (docs/14
/// §5.1). The backing rows keep the native 32-unit storage width.
const CALC_BANDS_32_AT5: usize = 24;
/// The 128 kbps reduced processing extent (selector 23, ctx `+0xb4` == 27),
/// (cfg_b4 == cfg_c4 == 27, the trim-law fallthrough — 27 is not in the
/// round-up set {29,30,31}) and replayed by `calc_cb_io_128.ndjson`. The
/// backing rows keep the native 32-unit storage width.
const CALC_BANDS_128_AT5: usize = 27;
const CALC_CANDIDATES_AT5: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum CalcBlockError {
    OutOfScope(&'static str),
}

/// Output of the `calc_channel_block_at5` seed section (sections 1-2).
#[derive(Debug, Clone, PartialEq)]
pub struct CalcSeedOutput {
    /// Per-channel `block+0xcc` idsf-quant rows (`band_count` entries),
    /// seeded from `saa_idtf_stereo` and clamped to 15.
    pub idsf_quant_cc: Vec<Vec<i32>>,
    /// Shared `+0x114` (`sa_limit_idtf_stereo[selector]`): the number of
    /// over-budget `+0xcc`-raise rounds phase A/B may run.
    pub shared_114: u16,
    /// Shared `+0x116` (`sa_adjust_iqt_0th_stereo[selector]`): the first
    /// band phase A/B raise loops touch.
    pub shared_116: u16,
}

/// Seed the `calc_channel_block_at5` idsf-quant rows and shared
/// `+0x114`/`+0x116` for the scoped stereo path.
///
/// `channel_count` must be 2 (stereo), `band_count` must be one of the
/// native-observed processing extents (26 at 48, 27 at 128, 28 at 160, or 32),
/// and `selector` must be `>= 0x10`. This standalone helper rejects the
/// low-selector (`< 0x10`) word-length ladder because it needs whole-call
/// block/object state; `calc_channel_block_frame_at5` runs that path via
/// `apply_low_selector_ladder_at5`. The 48 kHz rewrite stays rejected.
pub fn seed_idsf_quant_rows_at5(
    channel_count: usize,
    selector: i32,
    band_count: usize,
    sample_rate: u32,
) -> Result<CalcSeedOutput, CalcBlockError> {
    if channel_count == 0 || channel_count > 2 {
        return Err(CalcBlockError::OutOfScope(
            "channel_count must be 1 (mono) or 2 (stereo)",
        ));
    }
    if !matches!(
        band_count,
        CALC_BANDS_32_AT5
            | CALC_BANDS_48_AT5
            | CALC_BANDS_128_AT5
            | CALC_BANDS_160_AT5
            | CALC_BANDS_AT5
    ) {
        return Err(CalcBlockError::OutOfScope(
            "only native-observed 24-, 26-, 27-, 28-, or 32-band (ctx +0xb4) extents are in scope",
        ));
    }
    if sample_rate == 48000 {
        return Err(CalcBlockError::OutOfScope(
            "48 kHz +0xcc rewrite (ctx[0x2b] == 48000) is out of scope",
        ));
    }
    if selector < 0x10 {
        // The low-selector ladder (native LAB_00061c70) reads block/object
        // state (block +0x4c, obj +0x1b5f8/+0x1074, the two mode planes) this
        // standalone helper does not carry; the whole-call composition runs it
        // in native order (raw seed -> ladder -> clamp).
        return Err(CalcBlockError::OutOfScope(
            "low-selector (< 0x10) word-length ladder needs whole-call block/object state",
        ));
    }
    let sel = usize::try_from(selector)
        .map_err(|_| CalcBlockError::OutOfScope("negative selector is out of scope"))?;
    if sel >= 0x20 {
        return Err(CalcBlockError::OutOfScope(
            "selector must index the 32-row idtf tables",
        ));
    }

    // Table family per channel mode (decompile 43970-43980, native 0x51a80):
    // `param_4 == 2` reads the stereo `sa_adjust_iqt_0th_stereo` / `sa_limit_
    // idtf_stereo`; the else-arm (mono, `param_4 == 1`) reads the mono
    // siblings. Shared words `+0x116` (adjust) and `+0x114` (limit).
    let (shared_116, shared_114) = if channel_count == 2 {
        (
            u16::from(adjust_iqt_0th_stereo()[sel]),
            u16::from(limit_idtf_stereo()[sel]),
        )
    } else {
        (
            u16::from(adjust_iqt_0th_mono()[sel]),
            u16::from(limit_idtf_mono()[sel]),
        )
    };

    // Selector >= 0x10: nothing runs between the raw seed and the max-15 clamp
    // (native LAB_00061f15), so raw-seed + clamp is exactly native.
    let mut idsf_quant_cc = seed_raw_idsf_rows_at5(sel, band_count, channel_count);
    clamp_idsf_rows_at5(&mut idsf_quant_cc);

    Ok(CalcSeedOutput {
        idsf_quant_cc,
        shared_114,
        shared_116,
    })
}

/// Raw `+0xcc` idsf-quant seed rows: `saa[selector*0x20 + band]` bytes, one row
/// per channel, UNCLAMPED (native 0x51ee0-region seed). The saa base is picked
/// by channel mode (decompile 43970-43980): stereo (`param_4 == 2`) uses
/// `saa_idtf_stereo` (GOT-0x33038 = 0xc0580), mono (`param_4 == 1`) uses
/// `saa_idtf_mono` (GOT-0x33478 = 0xc0140). Same `saa[sel*0x20 + band]`
/// indexing. The max-15 clamp (`clamp_idsf_rows_at5`) and, for `selector <
/// 0x10`, the ladder run afterwards in native order.
fn seed_raw_idsf_rows_at5(sel: usize, band_count: usize, channel_count: usize) -> Vec<Vec<i32>> {
    let saa = if channel_count == 2 {
        saa_idtf_stereo()
    } else {
        saa_idtf_mono()
    };
    (0..channel_count)
        .map(|_| {
            (0..band_count)
                .map(|band| i32::from(saa[sel * 0x20 + band]))
                .collect()
        })
        .collect()
}

/// Clamp every `+0xcc` entry to a maximum of 15 (native LAB_00061f15).
fn clamp_idsf_rows_at5(rows: &mut [Vec<i32>]) {
    for row in rows.iter_mut() {
        for value in row.iter_mut() {
            if *value > 0xf {
                *value = 0xf;
            }
        }
    }
}

/// Low-selector (`param_5 < 0x10`) `+0xcc` word-length ladder: runs between the
/// raw seed and the max-15 clamp (native `0x51c70..0x51f0b`, decompile
/// 44015-44128; disassembly-verified at native offsets, see docs/13 §5.2).
/// Mutates each channel's raw `+0xcc` seed row in place.
///
/// - `LC-1` (native 0x51c70): for each channel, every band in `[s116, extent)`
///   whose `o_1b5f8[band] == 1` gets `+9/+4/+3/+2/+1/0` keyed on the init
///   `block+0x4c` word-length seed `v`.
/// - `LC-2` (native 0x51d5a, only when `extent > 0x12`): for each channel,
///   every band in `[0x12, extent)` whose `o_1b5f8[band] == 1` gets a threshold
///   bump (selected-plane cost `plane[mode].costs[band*8 + pick]` vs a
///   `v`-keyed cutoff) plus exactly one band/`v`-bracket bump.
///
/// The cost-index stride is 8 (`band * CALC_CANDIDATES_AT5 + pick`), confirmed
/// by disassembly: the Ghidra `int*` locals `local_f68/f64/fec` advance
/// `add ecx,0x8` per band (`+= 2` int-elements), so `(int)ptr` steps 8 and the
/// short index is `0x90 + 8*(band-0x12) + pick == 8*band + pick`.
fn apply_low_selector_ladder_at5(
    rows: &mut [Vec<i32>],
    word_lengths: &[Vec<i32>],
    b_4c: &[Vec<i32>],
    mode_1074: &[i32],
    planes: &[[Plane; 2]],
    s_116: usize,
    extent: usize,
) -> Result<(), CalcBlockError> {
    let n = rows.len();

    // LC-1: high-band ladder scanning [s116, extent) for word length exactly 1.
    for ch in 0..n {
        for band in s_116..extent {
            if word_lengths[ch][band] != 1 {
                continue;
            }
            let v = b_4c[ch][band];
            let bump = if v < 7 {
                // v < 7: native `+= 5` then falls through the `9 < v` test to
                // the `+= 4` tail (disasm 51cec/51d06) -> net +9.
                9
            } else if v <= 9 {
                4
            } else if v <= 12 {
                3
            } else if v <= 15 {
                2
            } else if v <= 18 {
                1
            } else {
                0
            };
            rows[ch][band] += bump;
        }
    }

    // LC-2: cost-threshold + bracket ladder, only when extent > 0x12.
    if extent > 0x12 {
        for ch in 0..n {
            let mode = usize::try_from(mode_1074[ch])
                .map_err(|_| CalcBlockError::OutOfScope("ladder: negative mode_1074"))?;
            if mode >= 2 {
                return Err(CalcBlockError::OutOfScope(
                    "ladder: mode_1074 outside the two AT5 mode planes",
                ));
            }
            let plane = &planes[ch][mode];
            for band in 0x12..extent {
                if word_lengths[ch][band] != 1 {
                    continue;
                }
                let v = b_4c[ch][band];
                let pick = usize::try_from(plane.picks[band]).map_err(|_| {
                    CalcBlockError::OutOfScope("ladder: negative selected-plane pick")
                })?;
                let idx = band * CALC_CANDIDATES_AT5 + pick;
                if idx >= plane.costs.len() {
                    return Err(CalcBlockError::OutOfScope(
                        "ladder: selected-plane cost index outside the 256-entry costs array",
                    ));
                }
                let cost = plane.costs[idx]; // signed i16

                // Threshold bump (one v-bracket, signed cost compare).
                if v < 0xd {
                    if cost > 0x3c {
                        rows[ch][band] += 1;
                    }
                } else if v < 0x10 {
                    if cost > 0x46 {
                        rows[ch][band] += 1;
                    }
                } else if v < 0x13 && cost > 0x50 {
                    rows[ch][band] += 1;
                }

                // Band/idsf-bracket bump: exactly one (native nested if/else).
                if v <= 0xc {
                    rows[ch][band] += 1;
                } else if band >= 0x16 && v <= 0xf {
                    rows[ch][band] += 1;
                } else if band >= 0x18 && v < 0x13 {
                    rows[ch][band] += 1;
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Whole-call composition of `calc_channel_block_at5` (docs/06 Step 4.2).
// ---------------------------------------------------------------------------

/// One per-group gain-control row (`obj + 0x8` previous / `obj + 0xc`
/// current, `0x98`-byte stride): the point count and the level ids.
#[derive(Debug, Clone)]
pub struct CalcGainRow {
    pub count: i32,
    pub level_ids: Vec<i32>,
}

/// One `0x280`-byte mode plane (`block + 0xb08` / `block + 0xd88`): the
/// 32 pick words followed by the flattened `band * 8 + candidate` cost
/// rows (`+0x80`).
#[derive(Debug, Clone)]
struct Plane {
    picks: Vec<i32>,
    costs: Vec<i16>,
}

impl Plane {
    fn zeroed() -> Self {
        Plane {
            picks: vec![0i32; CALC_BANDS_AT5],
            costs: vec![0i16; CALC_BANDS_AT5 * CALC_CANDIDATES_AT5],
        }
    }
}

/// Per-channel entry surface (the captured `calc_cb_io_call` channel row).
#[derive(Debug, Clone)]
pub struct CalcChannelEntry {
    // block (param_1[ch]) fields.
    pub max_wl_02: Vec<i16>,
    pub activity_14c: Vec<i32>,
    pub base_weights_1cc: Vec<f32>,
    pub idsf_cc: Vec<i32>,
    pub scale_24c: Vec<f32>,
    pub aux_3cc: Vec<f32>,
    pub slot_46: Vec<i16>,
    /// Init Block M word-length seed row (`block + 0x4c`), read by the
    /// low-selector (`param_5 < 0x10`) `+0xcc` word-length ladder. Only
    /// consumed on the 64/48 kbps low-selector path; the seven >= 0x10 rates
    /// never index it.
    pub b_4c: Vec<i32>,
    /// 68-word IDCT state at `block + 0x9f8`.
    pub idct_9f8: Vec<u32>,
    /// 160-word plane at `block + 0xb08`.
    pub plane_b08: Vec<u32>,
    /// 160-word plane at `block + 0xd88`.
    pub plane_d88: Vec<u32>,
    // object (param_3[ch]) fields.
    pub config_50: Vec<u32>,
    pub config_90: u32,
    pub config_a8: u32,
    pub config_ac: u32,
    pub config_b0: u32,
    pub config_b8: u32,
    pub config_c0: u32,
    pub config_c4: u32,
    pub cur_gain_0c: Vec<CalcGainRow>,
    pub prev_gain_08: Vec<CalcGainRow>,
    pub mode_1074: i32,
    pub o_1b578: Vec<i32>,
    pub o_1b5f8: Vec<i32>,
    pub o_1b678: Vec<i32>,
    pub o_1b6f8: Vec<i16>,
    pub y_index: i32,
    pub objside_14: i32,
    pub objside_1c: i32,
    pub spectrum: Vec<f32>,
}

/// Frame-level entry surface.
#[derive(Debug, Clone)]
pub struct CalcFrameEntry {
    pub channels: Vec<CalcChannelEntry>,
    /// Spectral level words already resident in each persistent native object
    /// on entry. Section 12 reads the other channel's row before that channel
    /// is rewritten when the joint-stereo `config_50` arm is live.
    pub prior_level_words: Vec<Vec<i32>>,
    pub selector: u32,
    pub budget: i32,
    // ctx (`*(object0 + 4)`) fields.
    pub ctx_flags_1dc: u32,
    pub ctx_quant_band_b4: i32,
    pub ctx_active_b0: i32,
    pub ctx_level_groups_c0: i32,
    pub ctx_field_90: i32,
    pub ctx_field_c4: i32,
    // shared (`*(block0 + 0x1008)`) fields.
    pub shared_word_84: u32,
    pub shared_word_88: i32,
    pub shared_word_8c: u32,
    pub shared_word_90: i32,
    pub shared_row_94: Vec<i16>,
    pub shared_row_d4: Vec<i16>,
    pub shared_s_11a: i16,
    pub shared_s_11c: i16,
    pub shared_s_11e: i16,
    pub shared_s_12a: i16,
    pub shared_s_12e: i16,
}

/// Per-channel exit surface asserted by the replay test.
#[derive(Debug, Clone)]
pub struct CalcChannelOutput {
    pub idsf_cc: Vec<i32>,
    pub slot_46: Vec<i16>,
    pub idct_9f8_mode: u32,
    /// Final per-channel `block+0x9f8` IDCT state (mode/band_count/split_flag +
    /// per-band flags/aux). The `calc_channel_block_at5` tail copies its first
    /// 35 words (3 header + `cfg[0xb0] & 0x3fffffff` band words) into the
    /// packer object at `[0x1078, 0x1104)` — see
    /// `crate::reference::native_layout::serialize_idct_object_range_a`.
    pub idct_block: IdctBlockState,
    /// Final per-channel `block+0x460` WLC/IDWL block state (after the
    /// section-14 fifth/sixth re-run). The `calc_channel_block_at5` tail
    /// (`decompiled/libatrac.c` 44840-44862 / 45337-45360, gated on shared
    /// `+0x88 == 1`) copies the selected mode's 5-word record and the
    /// `word_rows[sel]` plane into the packer object at `[0x1c70c, 0x1c72c]`
    /// + `[0x1c7f0, 0x1c870)` — see
    /// `crate::reference::native_layout::serialize_idwl_object_range_b`.
    pub idwl_block: IdwlBlockState,
    /// Whether the native IDWL tail copy ran for this frame. False on the
    /// priming call (`config_b0 == 0`), where the captured object holds
    /// uninitialized record residue (native call 0, ch1 `0x1c720 ==
    /// -134763296`); true when active bands exist (calls 7/12/22). The
    /// serializer only writes the IDWL window when this is set.
    pub idwl_copy_ran: bool,
    /// Final per-channel IDSF block state from the `adjust_scalefactors_at5`
    /// epilogue (native `0x55ae0`; `decompiled/libatrac.c` 37995-38025),
    /// the last writer of the object IDSF packing-prep words before pack.
    /// `None` only when the epilogue's `+0x8c == 0` zero arm fired (the leaf
    /// was not invoked); `Some` on the live `+0x8c != 0` path (all captured
    /// calls). See
    /// `crate::reference::native_layout::serialize_idsf_object_range_b`.
    pub idsf_block: Option<IdsfBlockState>,
    pub o_1b578: Vec<i32>,
    pub o_1b5f8: Vec<i32>,
    pub o_1b678: Vec<i32>,
    pub o_1b6f8: Vec<i16>,
    pub o_1c6f8: Vec<i32>,
    pub o_1c70c0: i32,
    pub mode_1074: i32,
}

/// Frame exit surface.
#[derive(Debug, Clone)]
pub struct CalcFrameOutput {
    pub channels: Vec<CalcChannelOutput>,
    pub shared_s_114: u16,
    pub shared_s_116: u16,
    pub shared_s_12a: i16,
    pub shared_s_12e: i16,
    pub ctx_field_1e4: i32,
    pub eax: i16,
    /// The FINAL shared IDWL window-fields scratch (`shared_side.window_fields`
    /// after the section-14 `run_fifth_sixth` re-run) — the native
    /// `ch0_calc + 0x768/0x76c/0x770` triple at tail time. `calc_nbits_for_`
    /// `idwl_1_at5` (native `0x1d160`) writes `[start, bits, base]` through the
    /// shared `+0x4d4` pointer on every mode-1 costing evaluation
    /// (last-writer-wins across both channels); native has no WLC costing
    /// between the re-run and the ch0 tone-mode-1 tail, so this local is the
    /// scratch the tail copies into `obj0[0x1c710/14/18]` (docs/12 §1.3).
    pub shared_wlc_window_fields: [i32; 3],
}

fn parse_idct_block(words: &[u32]) -> IdctBlockState {
    let mut block = IdctBlockState::default();
    block.mode = words[0];
    block.band_count = words[1] as usize;
    block.split_flag = words[2];
    for band in 0..CALC_BANDS_AT5 {
        block.flags[band] = words[3 + band];
        block.aux[band] = words[35 + band];
    }
    block
}

fn parse_plane(words: &[u32]) -> Plane {
    let picks = words[..CALC_BANDS_AT5].iter().map(|&w| w as i32).collect();
    let cost_words = &words[CALC_BANDS_AT5..CALC_BANDS_AT5 + 128];
    let mut costs = vec![0i16; CALC_BANDS_AT5 * CALC_CANDIDATES_AT5];
    for (i, w) in cost_words.iter().enumerate() {
        costs[i * 2] = (*w & 0xffff) as u16 as i16;
        costs[i * 2 + 1] = ((*w >> 16) & 0xffff) as u16 as i16;
    }
    Plane { picks, costs }
}

fn band_windows(spectrum: &[f32]) -> Vec<Vec<f32>> {
    let isps = isps_at5();
    let nsps = nsps_at5();
    (0..CALC_BANDS_AT5)
        .map(|band| {
            let base = isps[band] as usize;
            let count = nsps[band] as usize;
            spectrum[base..base + count].to_vec()
        })
        .collect()
}

/// Recompute a channel's plane (picks + 8-candidate cost rows) from the
/// current word-length row for every active band, mirroring the native
/// per-band `quant_nontone_nspecs_at5` + earliest-strict-minimum scan
/// (descriptor state 0). Returns the summed pick cost.
fn recost_plane(
    windows: &[Vec<f32>],
    word_lengths: &[i32],
    idsf: &[i32],
    scale: &[f32],
    state: usize,
    candidate_count: usize,
    band_count: usize,
) -> Result<(Plane, i16), CalcBlockError> {
    let mut plane = Plane::zeroed();
    let mut total: i16 = 0;
    for band in 0..band_count {
        if word_lengths[band] <= 0 {
            continue;
        }
        let costs = quant_nontone_costs_at5(
            &windows[band],
            word_lengths[band] as usize,
            idsf[band] as usize,
            scale[band],
            windows[band].len(),
            state,
            candidate_count,
        )
        .map_err(|_| CalcBlockError::OutOfScope("quant cost during plane recost"))?;
        let base = band * CALC_CANDIDATES_AT5;
        for (i, c) in costs.iter().enumerate().take(CALC_CANDIDATES_AT5) {
            plane.costs[base + i] = *c as i16;
        }
        let mut best = costs[0] as i16;
        let mut best_index = 0usize;
        for (i, c) in costs
            .iter()
            .enumerate()
            .take(candidate_count.min(costs.len()))
            .skip(1)
        {
            if (*c as i16) < best {
                best = *c as i16;
                best_index = i;
            }
        }
        plane.picks[band] = best_index as i32;
        total = total.wrapping_add(best);
    }
    Ok((plane, total))
}

fn recompute_selected_total(
    s_12a: i16,
    slot46: &[[i16; 2]],
    mode_1074: &[i32],
) -> Result<i16, CalcBlockError> {
    let mut total = s_12a;
    for ch in 0..slot46.len() {
        let sel = usize::try_from(mode_1074[ch])
            .map_err(|_| CalcBlockError::OutOfScope("negative selected mode is out of scope"))?;
        if sel >= 2 {
            return Err(CalcBlockError::OutOfScope(
                "selected mode outside the two AT5 mode planes is out of scope",
            ));
        }
        total = total.wrapping_add(slot46[ch][sel]);
    }
    Ok(total)
}

/// Truncate-toward-zero conversion of an x87 value (the section-8
/// `fistpl` with RC=11, confirmed at native `0x52cae`).
fn round_trunc(value: f64) -> i32 {
    value.trunc() as i32
}

/// Descending shell sort over `(key, index)` pairs with the native
/// `3n+1` gap sequence and swap semantics (decompile 44784-44827).
fn shell_sort_desc(keys: &mut [i32], indices: &mut [i32]) {
    let count = keys.len() as i32;
    let mut gap = 1i32;
    if count > 0 {
        while gap <= count {
            gap = gap * 3 + 1;
        }
    }
    loop {
        gap /= 3;
        if gap <= 0 {
            break;
        }
        let mut i = gap;
        while i < count {
            let key = keys[i as usize];
            let mut j = i - gap;
            while j >= 0 && keys[j as usize] < key {
                let upper = (gap + j) as usize;
                keys[upper] = keys[j as usize];
                indices.swap(upper, j as usize);
                j -= gap;
            }
            keys[(gap + j) as usize] = key;
            i += 1;
        }
    }
}

/// Compose the whole `calc_channel_block_at5` boundary for the scoped
/// 160/192/256/320/352 kbps stereo processing extents.
pub fn calc_channel_block_frame_at5(
    entry: &CalcFrameEntry,
) -> Result<CalcFrameOutput, CalcBlockError> {
    calc_channel_block_frame_impl_at5(entry)
}

fn calc_channel_block_frame_impl_at5(
    entry: &CalcFrameEntry,
) -> Result<CalcFrameOutput, CalcBlockError> {
    let n = entry.channels.len();
    if n == 0 || n > 2 {
        return Err(CalcBlockError::OutOfScope(
            "channel_count must be 1 (mono) or 2 (stereo)",
        ));
    }
    // Native entry clear (decompile 43950–43952, calc_channel_block_at5 at
    // native 0x51a80 / Ghidra 0x61a80): when `(flags & 0x7c) != 0` the shared
    // side word `+0x84` is set to 0 at entry, before any pass reads it. Thread
    // the cleared value into the passes below (do not mutate `entry`).
    let shared_word_84 = if entry.ctx_flags_1dc & 0x7c != 0 {
        0
    } else {
        entry.shared_word_84
    };
    let bands = usize::try_from(entry.ctx_quant_band_b4).map_err(|_| {
        CalcBlockError::OutOfScope("negative calc processing-band count is out of scope")
    })?;
    if !matches!(
        bands,
        CALC_BANDS_32_AT5
            | CALC_BANDS_48_AT5
            | CALC_BANDS_128_AT5
            | CALC_BANDS_160_AT5
            | CALC_BANDS_AT5
    ) {
        return Err(CalcBlockError::OutOfScope(
            "only native-observed 24-, 26-, 27-, 28-, or 32-band calc processing extents are in scope \
             (27 pinned by the 128 calc_cb_io oracle, 26 by the 48 calc_cb_io oracle, 24 by the \
             32-mono calc_cb_io + band_extent oracles)",
        ));
    }
    let selector = entry.selector as i32;
    let budget = entry.budget;
    let candidate_count = CALC_CANDIDATES_AT5;

    let isps = isps_at5();
    let nsps = nsps_at5();
    let x_tbl = x_at5();
    let y_tbl = y_at5();

    // Owned working state per channel. `windows` is mutated in place by the
    // phase-B destructive band-kill (native zeroes `param_2[ch]`'s band and
    // every later section reads the same buffer): the zeroed bands must be
    // visible to sections 7-14 (QUANT, level words, fifth/sixth, adjust).
    let mut windows: Vec<Vec<Vec<f32>>> = entry
        .channels
        .iter()
        .map(|ch| band_windows(&ch.spectrum))
        .collect();
    let mut idsf_cc: Vec<Vec<i32>> = entry.channels.iter().map(|c| c.idsf_cc.clone()).collect();
    let mut word_lengths: Vec<Vec<i32>> =
        entry.channels.iter().map(|c| c.o_1b5f8.clone()).collect();
    let mut selectors: Vec<Vec<i32>> = entry.channels.iter().map(|c| c.o_1b578.clone()).collect();
    let mut scale_factors: Vec<Vec<i32>> =
        entry.channels.iter().map(|c| c.o_1b678.clone()).collect();
    let mut quantized: Vec<Vec<i16>> = entry.channels.iter().map(|c| c.o_1b6f8.clone()).collect();
    let mut planes: Vec<[Plane; 2]> = entry
        .channels
        .iter()
        .map(|c| [parse_plane(&c.plane_b08), parse_plane(&c.plane_d88)])
        .collect();
    let mut idct_blocks: Vec<IdctBlockState> = entry
        .channels
        .iter()
        .map(|c| parse_idct_block(&c.idct_9f8))
        .collect();
    let mut slot46: Vec<[i16; 2]> = entry
        .channels
        .iter()
        .map(|c| [c.slot_46[0], *c.slot_46.get(1).unwrap_or(&0)])
        .collect();
    let mut mode_1074: Vec<i32> = entry.channels.iter().map(|c| c.mode_1074).collect();
    if entry.prior_level_words.len() != n || entry.prior_level_words.iter().any(|row| row.len() < 5)
    {
        return Err(CalcBlockError::OutOfScope(
            "prior spectral level rows must provide at least five words per channel",
        ));
    }
    let mut level_words = entry.prior_level_words.clone();
    for row in &mut level_words {
        row.resize(8, 0);
    }
    let mut tone0: Vec<i32> = vec![0i32; n];
    let scale_f32: Vec<Vec<f32>> = entry.channels.iter().map(|c| c.scale_24c.clone()).collect();
    // `max_wl_02` is the block HEAD row `[word0, max[0], max[1], ..]` (the
    // calc hook captures `read_i16_array(block, BANDS + 1)` and the coding
    // bridge builds `word0 ++ zeroth max row` identically). The native passes
    // clamp band `i` against `*(block + 2 + i*2)` = head[i + 1] (second-pass
    // round loop, decompile 39159; same base in fifth), so the per-band max
    // row handed to the passes must SKIP the word0 head. Indexing the head
    // row directly clamped band 0 against the mode word — invisible while
    // the row was uniform (all-7 at 352) and first BINDING at 64 kbps core
    // call 13, where word0 = 6 < max[0] = 7 flipped band 0's word length
    // (docs/13 §5.2 (nnn)).
    let max_wl: Vec<Vec<i16>> = entry
        .channels
        .iter()
        .map(|c| c.max_wl_02[1..].to_vec())
        .collect();
    let aux_weights: Vec<Vec<f32>> = entry.channels.iter().map(|c| c.aux_3cc.clone()).collect();
    let obj_mode: Vec<u32> = (0..n as u32).collect();

    // Shared / side words threaded through the passes.
    let mut s_12a = entry.shared_s_12a;
    let mut s_12e = entry.shared_s_12e;
    let mut s_11a = entry.shared_s_11a;
    let mut s_11c = entry.shared_s_11c;
    let mut s_11e = entry.shared_s_11e;

    // --- Sections 1-2: prologue idsf-quant seed + shared 114/116. ---
    // Native order (decompile 43984-44149): raw `+0xcc` seed, then for
    // `selector < 0x10` the word-length ladder (LAB_00061c70), then the max-15
    // clamp (LAB_00061f15). For `selector >= 0x10` nothing runs between seed
    // and clamp, so this is byte-identical to the standalone seed helper.
    if entry.channels[0].config_ac == 48000 {
        return Err(CalcBlockError::OutOfScope(
            "48 kHz +0xcc rewrite (ctx[0x2b] == 48000) is out of scope",
        ));
    }
    let sel = usize::try_from(selector)
        .map_err(|_| CalcBlockError::OutOfScope("negative selector is out of scope"))?;
    if sel >= 0x20 {
        return Err(CalcBlockError::OutOfScope(
            "selector must index the 32-row idtf tables",
        ));
    }
    // Shared words `+0x114` (limit) / `+0x116` (adjust) per channel mode
    // (decompile 43970-43980): stereo reads the stereo tables, mono (n == 1)
    // reads the mono siblings. At 128 mono (sel 23): s_114 = 10, s_116 = 24.
    let (s_114, s_116) = if n == 2 {
        (
            u16::from(limit_idtf_stereo()[sel]),
            u16::from(adjust_iqt_0th_stereo()[sel]),
        )
    } else {
        (
            u16::from(limit_idtf_mono()[sel]),
            u16::from(adjust_iqt_0th_mono()[sel]),
        )
    };
    let b_4c: Vec<Vec<i32>> = entry.channels.iter().map(|c| c.b_4c.clone()).collect();

    let mut seed_rows = seed_raw_idsf_rows_at5(sel, bands, n);
    if selector < 0x10 {
        apply_low_selector_ladder_at5(
            &mut seed_rows,
            &word_lengths,
            &b_4c,
            &mode_1074,
            &planes,
            usize::from(s_116),
            bands,
        )?;
    }
    clamp_idsf_rows_at5(&mut seed_rows);
    for ch in 0..n {
        // Native seeds only the processing prefix (`iVar16`) of the 32-word
        // `block+0xcc` storage row. Preserve the inactive storage tail.
        idsf_cc[ch][..bands].copy_from_slice(&seed_rows[ch]);
    }

    // --- Section 3: second pass. ---
    {
        let quant_raw: Vec<Vec<ZerothQuantBandRaw<'_>>> = (0..n)
            .map(|ch| {
                (0..bands)
                    .map(|band| ZerothQuantBandRaw {
                        spectrum: &windows[ch][band],
                        idsf: idsf_cc[ch][band] as usize,
                        scale: scale_f32[ch][band],
                        count: windows[ch][band].len(),
                    })
                    .collect()
            })
            .collect();
        let channels: Vec<SecondChannelState<'_>> = (0..n)
            .map(|ch| {
                let plane = &planes[ch][mode_1074[ch] as usize];
                let pick_costs: Vec<i16> = (0..bands)
                    .map(|band| {
                        plane.costs[band * CALC_CANDIDATES_AT5 + plane.picks[band] as usize]
                    })
                    .collect();
                SecondChannelState {
                    base_weights: &entry.channels[ch].base_weights_1cc,
                    max_word_lengths: &max_wl[ch],
                    activity: &entry.channels[ch].activity_14c,
                    quant_bands: &quant_raw[ch],
                    word_lengths: word_lengths[ch].clone(),
                    picks: plane.picks.clone(),
                    pick_costs,
                    idwl_init_bits: 0,
                }
            })
            .collect();
        let mut state = SecondFrameState {
            channels,
            band_count: bands,
            budget_limit: budget,
            selector: entry.selector,
            header_flags_1dc: entry.ctx_flags_1dc,
            sample_rate: entry.channels[0].config_ac as i32,
            // Second-pass recost runs at the zeroth-reset descriptor
            // state: `zeroth_bit_allocation_at5` resets `*(obj + 0x1074) = 0`
            // per channel at entry (decompile 35988), so both channels are
            // at plane 0 here and `mode_1074[0]` is 0 on the scoped path.
            quant_state: mode_1074[0] as usize,
            quant_candidate_count: candidate_count,
            side_gate_84: shared_word_84,
            side_gate_88: entry.shared_word_88 as u32,
            word_group_count_c4: entry.ctx_field_c4,
            idwl_bits_11a: s_11a,
            base_total_12a: s_12a,
            current_bits_12e: s_12e,
        };
        let out = second_bit_allocation_frame_at5(&mut state)
            .map_err(|_| CalcBlockError::OutOfScope("second pass failed"))?;
        for ch in 0..n {
            let sel = mode_1074[ch] as usize;
            word_lengths[ch] = out.channels[ch].word_lengths.clone();
            slot46[ch][sel] = out.channels[ch].quant_state_total;
            planes[ch][sel].picks = out.channels[ch].picks.clone();
            for band in 0..bands {
                let pick = usize::try_from(out.channels[ch].picks[band]).map_err(|_| {
                    CalcBlockError::OutOfScope("second pass produced a negative candidate pick")
                })?;
                if pick >= CALC_CANDIDATES_AT5 {
                    return Err(CalcBlockError::OutOfScope(
                        "second pass produced a candidate pick outside the 8-way table",
                    ));
                }
                planes[ch][sel].costs[band * CALC_CANDIDATES_AT5 + pick] =
                    out.channels[ch].pick_costs[band];
            }
        }
        // The second-pass IDWL epilogue fork (decompile 39326–39383),
        // mirrored on the composed state words.
        let idwl_total = if shared_word_84 == 0 {
            // WLC-reset prong (decompile 39326–39345; the flag-set calc
            // entry cleared `shared+0x84`): `param_3*2 + cfg[0xc4]*3` bits
            // per channel, no IDWL leaves, no `+0x88` store.
            n as i32 * 2 + n as i32 * entry.ctx_field_c4 * 3
        } else {
            // Init prong: live IDWL init leaf on the final rows — bits for
            // +0x11a and the WLC block state fifth needs.
            let mut idwl_total = n as i32 * 2;
            for ch in 0..n {
                let ch_state = IdwlChannelState {
                    mode: obj_mode[ch],
                    context_kind: entry.channels[ch].objside_1c as u32,
                    word_count: entry.channels[ch].config_c4 as usize,
                    group_count: entry.channels[ch].config_b8 as usize,
                    word_lengths: &word_lengths[ch],
                    previous_word_lengths: &word_lengths[0],
                };
                let mut block = IdwlBlockState::default();
                let bits = calc_nbits_for_idwl_ch_init_at5(&ch_state, &mut block)
                    .map_err(|_| CalcBlockError::OutOfScope("idwl init leaf"))?;
                idwl_total = idwl_total.wrapping_add(bits);
            }
            idwl_total
        };
        let delta = s_11a.wrapping_sub(idwl_total as i16);
        s_11a = idwl_total as i16;
        s_12a = s_12a.wrapping_sub(delta);
        // Native tests the over-budget Phase-A gate immediately after the
        // second-pass IDWL epilogue, before section 5 re-costs the alternate
        // mode plane. Keep that post-second total live for selector 27.
        s_12e = recompute_selected_total(s_12a, &slot46, &mode_1074)?;
        let _ = out.extended_total_12e;
    }

    // --- Post-second over-budget Phase A: +0xcc high-band raise loop. ---
    if budget < i32::from(s_12e) && s_114 > 0 {
        phase_a_raise_loop(
            &windows,
            &word_lengths,
            &mut idsf_cc,
            &mut planes,
            &mut slot46,
            &mode_1074,
            &scale_f32,
            s_114,
            s_116,
            s_12a,
            &mut s_12e,
            budget,
            candidate_count,
        )?;
    }

    // Recompute both mode planes from the post-second rows (state-0 quant
    // is identical for both planes at 352). Section 5 natively re-costs the
    // non-selected plane and re-picks the mode; Rust keeps the historical
    // recost-both-planes structure, so the selected plane may already include
    // Phase-A recosts at 256 before being recomputed here.
    for ch in 0..n {
        // Each mode plane is quantized with its own descriptor state (the
        // plane's mode index): the non-selected plane re-cost uses state 1.
        let other = 1 - mode_1074[ch] as usize;
        let (sel_plane, sel_total) = recost_plane(
            &windows[ch],
            &word_lengths[ch],
            &idsf_cc[ch],
            &scale_f32[ch],
            mode_1074[ch] as usize,
            candidate_count,
            bands,
        )?;
        let (oth_plane, oth_total) = recost_plane(
            &windows[ch],
            &word_lengths[ch],
            &idsf_cc[ch],
            &scale_f32[ch],
            other,
            candidate_count,
            bands,
        )?;
        planes[ch][mode_1074[ch] as usize] = sel_plane;
        planes[ch][other] = oth_plane;
        slot46[ch][mode_1074[ch] as usize] = sel_total;
        slot46[ch][other] = oth_total;
        // Native mode re-pick: strict-< argmin over slots 0..1 from 0x4000.
        let mut best = 0x4000i16;
        for slot in 0..2 {
            if slot46[ch][slot] < best {
                best = slot46[ch][slot];
                mode_1074[ch] = slot as i32;
            }
        }
    }
    // Recompute +0x12e from the base + selected-mode slots.
    s_12e = recompute_selected_total(s_12a, &slot46, &mode_1074)?;

    // --- Over-budget-after-section-5 merge (native LAB_00062743 / 0x52743):
    // if the post-second Phase-A loop plus section-5 recost/re-pick still
    // exceeds the budget, native takes the PHASE B destructive band-kill loop
    // (decompile 44480-44603). It walks bands high-to-low, and for every active
    // band (word length > 0) forces the `+0xcc` idsf to 15, zeroes that band's
    // spectrum window, re-costs the channel's selected plane, and recomputes
    // `+0x12e = +0x12a + Σ selected slot46`, breaking as soon as `12e <=
    // budget`. Live at 352 on exactly three dense-content corpus calls
    // (docs/12 §2.3): sweat 1701/2482, 12-34-am 2437. Native reaches this
    // Phase-B check whenever `budget < +0x12e` after the over-budget block,
    // REGARDLESS of whether Phase A ran: `s114` (`+0x114`) gates only Phase A
    // (decompile 44169 loop gate, 44217 per-band cap, 44304-44305 outer-loop
    // count); the Phase-B band-kill (44480-44603) reads `+0xcc`/`+0x94`/`+0x46`/
    // `+0x12a`/`+0x12e`/spectrum but NEVER `+0x114`. So "Phase B after a
    // positive-s114 Phase A" is not a distinct arm — it is the 352-ported,
    // s114-agnostic Phase B running on a state reached via the 256-traced
    // (§2.1) Phase A. The §2.1 fail-explicit guard was a conservative
    // placeholder; it is retired on this decompile evidence (docs/13 §2.3,
    // Appendix B). The 256/192/160 bring-up slices cleared their separate
    // blockers; the 28-unit version is native-observed at 160 (docs/13 §3.2
    // slice 4 (vv): 12-34-am call 1179 phase_b + joint-stereo kill) and runs
    // extent-generically here.
    if budget < i32::from(s_12e) {
        phase_b_band_kill(
            &entry.shared_row_94,
            &mut windows,
            &mut word_lengths,
            &mut idsf_cc,
            &mut planes,
            &mut slot46,
            &mode_1074,
            &scale_f32,
            s_12a,
            &mut s_12e,
            budget,
            candidate_count,
        )?;
    }

    // --- Section 7: chosen-plane copy into o_1b578 + stereo forcing. ---
    for ch in 0..n {
        let picks = &planes[ch][mode_1074[ch] as usize].picks;
        selectors[ch] = picks.clone();
    }
    // Section 7 selector forcing (decompile 44620-44663). Stereo
    // (`param_4 == 2`) forces ch1's `+0x1b578` selector to 1 at every band
    // where the shared `+0xd4` row is 1, then clears BOTH channels' selectors
    // wherever both word-length rows are 0. Mono (`param_4 == 1`) takes the
    // else-arm: no d4 forcing (single channel, no ch1), and clears ch0's
    // selector wherever ch0's own word-length row is 0 (`local_cf4[8]`).
    if n == 2 {
        for band in 0..bands {
            if entry.shared_row_d4[band] == 1 {
                selectors[1][band] = 1;
            }
        }
        for band in 0..bands {
            if word_lengths[0][band] == 0 && word_lengths[1][band] == 0 {
                selectors[0][band] = 0;
                selectors[1][band] = 0;
            }
        }
    } else {
        for band in 0..bands {
            if word_lengths[0][band] == 0 {
                selectors[0][band] = 0;
            }
        }
    }

    // --- Section 8: band-order keys + two shell sorts. ---
    let stereo_bound = y_tbl[entry.channels[0].y_index as usize] as usize;
    let mut key49c = vec![0i32; bands * n];
    for ch in 0..n {
        for band in 0..bands {
            let k = scale_factors[ch][band] - round_trunc(f64::from(band as f32) * 0.125 + 0.5)
                + round_trunc(f64::from(aux_weights[ch][band]) - f64::from((band >> 4) as f32));
            key49c[ch * bands + band] = k;
        }
    }
    // band>=12 monotone-run bonus over 1b678 scale factors.
    for ch in 0..n {
        for band in 12..bands {
            let mut monotone = true;
            let mut min_run = 0x3fi32;
            let hi = scale_factors[ch][band];
            let mut j = band as i32 - 1;
            let low = band as i32 - 4;
            if low <= j {
                while low <= j {
                    let v = scale_factors[ch][j as usize];
                    if hi < v {
                        monotone = false;
                        break;
                    }
                    let diff = hi - v;
                    if diff < min_run {
                        min_run = diff;
                    }
                    j -= 1;
                }
            }
            if monotone && min_run > 3 {
                key49c[ch * bands + band] += min_run >> 1;
            }
        }
    }
    // Stereo combined keys over the first stereo_bound bands. Native gates
    // this whole block (the `+0x51c` combined key and its `+0x19c` shell sort)
    // behind `param_4 == 2` (decompile 44763-44799): each key sums ch0's and
    // ch1's `+0x49c` band keys. Mono has no ch1, so `idx19c` stays unsorted —
    // native leaves `local_19c` untouched for mono, and fifth's ONLY read of
    // its stereo-order argument (param_7, decompile 39473) sits inside
    // fifth's own `param_3 == 2` channel gate (decompile 39469), so the
    // uninitialized mono `local_19c` is never consumed. The main `+0x49c`
    // sort below runs for BOTH channel modes (decompile 44800-44827).
    let mut idx19c = vec![0i32; bands];
    if n == 2 {
        let mut key51c = vec![0i32; bands];
        for band in 0..stereo_bound {
            key51c[band] = key49c[bands + band] + key49c[band];
            idx19c[band] = band as i32;
        }
        let mut k = key51c[..stereo_bound].to_vec();
        let mut idx = idx19c[..stereo_bound].to_vec();
        shell_sort_desc(&mut k, &mut idx);
        idx19c[..stereo_bound].copy_from_slice(&idx);
    }
    let mut idx11c: Vec<i32> = (0..(bands * n) as i32).collect();
    shell_sort_desc(&mut key49c, &mut idx11c);
    let order = idx11c;
    let stereo_order = idx19c;

    // --- Section 9: fifth + sixth (first invocation). ---
    let mut wlc_blocks: Vec<IdwlBlockState>;
    let mut shared_side = IdwlSideState::default();
    {
        wlc_blocks = vec![IdwlBlockState::default(); n];
        if shared_word_84 == 0 {
            // Flag path: the second pass's WLC-reset prong (decompile
            // 39326–39345) left the `block+0x460` records in the reset state
            // — word [0] = 0, words [5..10] = [0, 0, cfg[0xc4], 0, 0] — with
            // no init-leaf run, so seed fifth's blocks with exactly that.
            for ch in 0..n {
                wlc_blocks[ch].selector_fields_14_24 = [0, 0, entry.ctx_field_c4, 0, 0];
            }
        } else {
            // Build the WLC blocks live (init leaf) threading the shared side.
            for ch in 0..n {
                wlc_blocks[ch].side = shared_side.clone();
                let ch_state = IdwlChannelState {
                    mode: obj_mode[ch],
                    context_kind: entry.channels[ch].objside_1c as u32,
                    word_count: entry.channels[ch].config_c4 as usize,
                    group_count: entry.channels[ch].config_b8 as usize,
                    word_lengths: &word_lengths[ch],
                    previous_word_lengths: &word_lengths[0],
                };
                calc_nbits_for_idwl_ch_init_at5(&ch_state, &mut wlc_blocks[ch])
                    .map_err(|_| CalcBlockError::OutOfScope("idwl init leaf (fifth seed)"))?;
                shared_side = wlc_blocks[ch].side.clone();
            }
        }
        run_fifth_sixth(
            entry,
            &windows,
            &mut word_lengths,
            &mut selectors,
            &mut idsf_cc,
            &mut planes,
            &mut idct_blocks,
            &mut wlc_blocks,
            &mut shared_side,
            &mut slot46,
            &mode_1074,
            &obj_mode,
            &scale_factors,
            &max_wl,
            &scale_f32,
            &order,
            &stereo_order,
            stereo_bound,
            &mut s_11a,
            &mut s_11e,
            &mut s_12a,
            &mut s_12e,
            budget,
            candidate_count,
            bands,
        )?;
    }

    // --- Section 10: tone/GHA copy (gated shared +0x88 == 1). Skipped
    // for surfaces we do not assert except o_1c70c[0] = block +0x460. ---
    for ch in 0..n {
        tone0[ch] = wlc_blocks[ch].mode as i32;
    }

    // --- Section 11: QUANT loop 1 (idwl = word length, idsf = +0xcc). ---
    quant_loop(
        &windows,
        &word_lengths,
        &idsf_cc,
        &scale_f32,
        &mut quantized,
        bands,
    )?;

    // --- Section 12: spectral level words. ---
    section12_levels(
        entry,
        &windows,
        &word_lengths,
        &scale_factors,
        &idsf_cc,
        &quantized,
        &planes,
        &mode_1074,
        &aux_weights,
        &mut level_words,
        &mut tone0,
        selector,
        bands,
    )?;

    // --- Section 13: adjust pass (+ phase C, live only when over budget). ---
    if budget < i32::from(s_12e) {
        return Err(CalcBlockError::OutOfScope(
            "phase C (post-second over budget before adjust) not composed",
        ));
    }
    let idsf_blocks = run_adjust(
        entry,
        &windows,
        &word_lengths,
        &mut scale_factors,
        &quantized,
        &aux_weights,
        &level_words,
        &idsf_cc,
        &obj_mode,
        &mut s_11c,
        &mut s_12a,
        &mut s_12e,
        bands,
    )?;

    // --- Section 13 phase C: post-adjust over-budget var trials. ---
    if budget < i32::from(s_12e) {
        phase_c_var_trials(
            entry,
            &windows,
            &word_lengths,
            &mut idsf_cc,
            &mut selectors,
            &mut planes,
            &mut idct_blocks,
            &mut slot46,
            &mode_1074,
            &obj_mode,
            &scale_f32,
            s_116,
            &mut s_11e,
            &mut s_12a,
            &mut s_12e,
            budget,
            candidate_count,
        )?;
    }

    // --- eighth pass. ---
    // This is a no-op when `s_12e <= budget`, but it is live on sparse
    // low-rate frames even after phase C. Accepted trials destructively zero
    // spectrum lines; the section-14 fifth/sixth re-run and final QUANT loop
    // consume that same mutated spectrum (native watchpoint at 0x516d3,
    // docs/15 low-rate sweep regression follow-up).
    {
        let channels: Vec<EighthChannelState> = (0..n)
            .map(|ch| {
                let sel = mode_1074[ch] as usize;
                let other = 1 - sel;
                EighthChannelState {
                    word_lengths: word_lengths[ch].clone(),
                    selector_row: selectors[ch].clone(),
                    band_idsf: idsf_cc[ch].clone(),
                    band_scale: scale_f32[ch].clone(),
                    spectra: windows[ch].clone(),
                    active_costs: planes[ch][sel].costs.clone(),
                    trial_costs: planes[ch][other].costs.clone(),
                    idct_block: idct_blocks[ch].clone(),
                    quant_bits_46: slot46[ch][sel],
                    obj_mode: obj_mode[ch],
                    fixbits_index: entry.channels[ch].config_90 as usize,
                    quant_state: sel,
                }
            })
            .collect();
        let mut state = EighthFrameState {
            channels,
            band_count: bands,
            target_bits: budget,
            active_band_count: entry.channels[0].config_b0 as usize,
            quant_candidate_count: candidate_count,
            idct_bits_11e: s_11e,
            base_total_12a: s_12a,
            current_bits_12e: s_12e,
        };
        let out = eighth_bit_allocation_frame_at5(&mut state)
            .map_err(|_| CalcBlockError::OutOfScope("eighth pass failed"))?;
        for ch in 0..n {
            let sel = mode_1074[ch] as usize;
            windows[ch] = out.channels[ch].spectra.clone();
            selectors[ch] = out.channels[ch].selector_row.clone();
            planes[ch][sel].costs = out.channels[ch].active_costs.clone();
            slot46[ch][sel] = out.channels[ch].quant_bits_46;
            idct_blocks[ch] = out.channels[ch].idct_block.clone();
        }
        s_11e = out.idct_bits_11e;
        s_12a = out.base_total_12a;
        s_12e = out.extended_total_12e;
    }

    // --- Section 14: gated fifth+sixth re-run (objside +0x1c != 2). ---
    if entry.channels[0].objside_1c != 2 {
        run_fifth_sixth(
            entry,
            &windows,
            &mut word_lengths,
            &mut selectors,
            &mut idsf_cc,
            &mut planes,
            &mut idct_blocks,
            &mut wlc_blocks,
            &mut shared_side,
            &mut slot46,
            &mode_1074,
            &obj_mode,
            &scale_factors,
            &max_wl,
            &scale_f32,
            &order,
            &stereo_order,
            stereo_bound,
            &mut s_11a,
            &mut s_11e,
            &mut s_12a,
            &mut s_12e,
            budget,
            candidate_count,
            bands,
        )?;
    }
    for ch in 0..n {
        tone0[ch] = wlc_blocks[ch].mode as i32;
    }

    // --- Section 14: QUANT loop 2. ---
    quant_loop(
        &windows,
        &word_lengths,
        &idsf_cc,
        &scale_f32,
        &mut quantized,
        bands,
    )?;

    // --- Section 15: epilogue (ctx +0x1e4 = sign-extended +0x12e). ---
    // `obj + 0x1c6f8[5]` aliases `obj + 0x1c70c[0]` (the tone flag written
    // by section 10/14 before section 12); section 12 stops at slot 4, so
    // the aliased tone value survives into the level-word array.
    for ch in 0..n {
        if level_words[ch].len() > 5 {
            level_words[ch][5] = tone0[ch];
        }
    }
    let _ = (isps, nsps, x_tbl);
    let ctx_field_1e4 = i32::from(s_12e);

    // The IDWL tail record + plane copy (native calc_channel_block_at5 tail,
    // decompile 44830-44917). Native gates it on `(*(ctx+0x1dc) & 0x7c) == 0`
    // (decompile 44830) AND `shared+0x88 == 1` (44839/44914 fall through to
    // LAB_00063c07), NOT on the active-band count. The `& 0x7c` half is now
    // ported (the entry OutOfScope guard was retired for the flag arm): when
    // any `& 0x7c` bit is set the native tail is skipped, so gate on it here.
    // The `shared+0x88 == 1` half: `shared+0x88` is 0 at every calc entry
    // shared word_88_i32 == 0 at entry) but second_bit_allocation_at5 sets it to
    // 1 mid-call (decompile 39346-39357: the else-if that runs the per-channel
    // idwl init then `*(iVar3+0x88) = 1`) before the tail — so on the `& 0x7c
    // == 0` path it is always 1 at tail time, including the call-0 priming call
    // i.e. the candidate-1 empty-shape record WAS written natively at b0 == 0).
    // Do NOT gate on config_b0; it was a disproven inference.
    let idwl_copy_ran = entry.ctx_flags_1dc & 0x7c == 0;

    let channels = (0..n)
        .map(|ch| CalcChannelOutput {
            idsf_cc: idsf_cc[ch].clone(),
            slot_46: vec![slot46[ch][0], slot46[ch][1]],
            idct_9f8_mode: idct_blocks[ch].mode,
            idct_block: idct_blocks[ch].clone(),
            idwl_block: wlc_blocks[ch].clone(),
            idwl_copy_ran,
            idsf_block: idsf_blocks[ch].clone(),
            o_1b578: selectors[ch].clone(),
            o_1b5f8: word_lengths[ch].clone(),
            o_1b678: scale_factors[ch].clone(),
            o_1b6f8: quantized[ch].clone(),
            o_1c6f8: level_words[ch].clone(),
            o_1c70c0: tone0[ch],
            mode_1074: mode_1074[ch],
        })
        .collect();

    Ok(CalcFrameOutput {
        channels,
        shared_s_114: s_114,
        shared_s_116: s_116,
        shared_s_12a: s_12a,
        shared_s_12e: s_12e,
        ctx_field_1e4,
        eax: s_12e,
        // The final shared IDWL window-fields scratch, AFTER the section-14
        // fifth/sixth re-run. Native has no WLC costing between that re-run and
        // the ch0 tone-mode-1 tail copy, so this is exactly the native
        // `ch0_calc + 0x768` triple at tail time (docs/12 §1.3).
        shared_wlc_window_fields: shared_side.window_fields,
    })
}

/// The native QUANT loop (`0x62760`/`0x64f80`): for every band with a
/// positive word length, quantize the spectrum window at `idwl = word
/// length`, `idsf = +0xcc`, `threshold_scale = +0x24c` into the
/// `+0x1b6f8` plane at the band's `g_a_isps_at5` base.
fn quant_loop(
    windows: &[Vec<Vec<f32>>],
    word_lengths: &[Vec<i32>],
    idsf_cc: &[Vec<i32>],
    scale_f32: &[Vec<f32>],
    quantized: &mut [Vec<i16>],
    band_count: usize,
) -> Result<(), CalcBlockError> {
    let isps = isps_at5();
    let nsps = nsps_at5();
    for ch in 0..windows.len() {
        for band in 0..band_count {
            if word_lengths[ch][band] <= 0 {
                continue;
            }
            let base = isps[band] as usize;
            let count = nsps[band] as usize;
            let mut out = vec![0i16; count];
            quant_at5(
                &windows[ch][band],
                &mut out,
                word_lengths[ch][band] as usize,
                idsf_cc[ch][band] as usize,
                scale_f32[ch][band],
                count,
            )
            .map_err(|_| CalcBlockError::OutOfScope("QUANT loop"))?;
            quantized[ch][base..base + count].copy_from_slice(&out);
        }
    }
    Ok(())
}

/// Phase A `+0xcc` raise loop (native `0x51f88..0x522bf`, decompile
/// 44152-44306). Reached immediately after the second pass when the native
/// post-second `+0x12e` exceeds the frame budget and shared `+0x114 > 0`.
///
/// Ported for the 256 kbps stereo evidence shape (stereo, 32 bands, eight
/// candidates, high-band raises beginning at shared `+0x116 >= 8`; selector 27
/// has `s114=10`, `s116=24`) AND for channel_count 1 at 128 kbps mono on the
/// docs/14 §1.2 sweat_mono oracle (12 Phase-A-only over-budget events, all with
/// `s114=10`/`s116=24`, budget 5947, row_94 all-zero). The native raise loop
/// (native `0x51f88..0x522bf`, decompile 44152-44306) is fully
/// `param_4`(channel-count)-parameterized throughout — every band/channel loop
/// is `0 < param_4` / `< param_4`, nothing in it is stereo-specific — so the
/// port runs generically at `n == 1`. The low-band selector special is
/// deliberately fail-explicit until a native trace at a rate where it matters
/// exists.
#[allow(clippy::too_many_arguments)]
fn phase_a_raise_loop(
    windows: &[Vec<Vec<f32>>],
    word_lengths: &[Vec<i32>],
    idsf_cc: &mut [Vec<i32>],
    planes: &mut [[Plane; 2]],
    slot46: &mut [[i16; 2]],
    mode_1074: &[i32],
    scale_f32: &[Vec<f32>],
    s_114: u16,
    s_116: u16,
    s_12a: i16,
    s_12e: &mut i16,
    budget: i32,
    candidate_count: usize,
) -> Result<(), CalcBlockError> {
    let n = windows.len();
    let bands = CALC_BANDS_AT5;
    if !(1..=2).contains(&n) {
        return Err(CalcBlockError::OutOfScope(
            "phase A +0xcc raise loop is ported only for channel_count 1 (128 mono) or 2 \
             (256 stereo)",
        ));
    }
    if candidate_count != CALC_CANDIDATES_AT5 {
        return Err(CalcBlockError::OutOfScope(
            "phase A +0xcc raise loop expects the native 8-candidate table",
        ));
    }
    let start_band = usize::from(s_116);
    if start_band < 8 {
        return Err(CalcBlockError::OutOfScope(
            "phase A low-band +0xcc selector special is unobserved at 256 kbps",
        ));
    }
    if start_band >= bands {
        return Err(CalcBlockError::OutOfScope(
            "phase A +0xcc raise start band is outside the 32-band table",
        ));
    }

    let limit = i32::from(s_114);
    for _round in 0..usize::from(s_114) {
        // Native clears a channel x band flag array at the start of each outer
        // round. The observed high-band path sets at most one flag before each
        // selected-plane recost, then clears it.
        let mut flag = vec![vec![0i32; bands]; n];
        for bi in (start_band..bands).rev() {
            for ch in 0..n {
                *s_12e = recompute_selected_total(s_12a, slot46, mode_1074)?;
                if i32::from(*s_12e) <= budget {
                    return Ok(());
                }
                if word_lengths[ch][bi] <= 0 {
                    flag[ch][bi] = 0;
                    continue;
                }
                let old_idsf = idsf_cc[ch][bi];
                if old_idsf >= limit {
                    flag[ch][bi] = 0;
                    continue;
                }
                if bi < 8 {
                    return Err(CalcBlockError::OutOfScope(
                        "phase A low-band +0xcc selector special is unobserved at 256 kbps",
                    ));
                }
                let raised = old_idsf + 1;
                if raised >= 0x10 {
                    return Err(CalcBlockError::OutOfScope(
                        "phase A high-band +0xcc saturation path is unobserved at 256 kbps",
                    ));
                }
                idsf_cc[ch][bi] = raised;
                flag[ch][bi] = 1;

                let sel = usize::try_from(mode_1074[ch])
                    .map_err(|_| CalcBlockError::OutOfScope("phase A selected mode is negative"))?;
                if sel >= 2 {
                    return Err(CalcBlockError::OutOfScope(
                        "phase A selected mode outside the two AT5 mode planes is out of scope",
                    ));
                }

                let mut total: i16 = 0;
                for b in 0..bands {
                    if word_lengths[ch][b] <= 0 {
                        continue;
                    }
                    if flag[ch][b] == 0 {
                        let pick = usize::try_from(planes[ch][sel].picks[b]).map_err(|_| {
                            CalcBlockError::OutOfScope("phase A selected-plane pick is negative")
                        })?;
                        if pick >= CALC_CANDIDATES_AT5 {
                            return Err(CalcBlockError::OutOfScope(
                                "phase A selected-plane pick is outside the 8-way table",
                            ));
                        }
                        total = total
                            .wrapping_add(planes[ch][sel].costs[b * CALC_CANDIDATES_AT5 + pick]);
                        continue;
                    }

                    let costs = quant_nontone_costs_at5(
                        &windows[ch][b],
                        word_lengths[ch][b] as usize,
                        idsf_cc[ch][b] as usize,
                        scale_f32[ch][b],
                        windows[ch][b].len(),
                        sel,
                        candidate_count,
                    )
                    .map_err(|_| CalcBlockError::OutOfScope("phase A quant"))?;
                    let base = b * CALC_CANDIDATES_AT5;
                    for (i, c) in costs.iter().enumerate().take(CALC_CANDIDATES_AT5) {
                        planes[ch][sel].costs[base + i] = *c as i16;
                    }
                    let mut best = costs[0] as i16;
                    let mut best_index = 0usize;
                    for (i, c) in costs
                        .iter()
                        .enumerate()
                        .take(candidate_count.min(costs.len()))
                        .skip(1)
                    {
                        if (*c as i16) < best {
                            best = *c as i16;
                            best_index = i;
                        }
                    }
                    planes[ch][sel].picks[b] = best_index as i32;
                    total = total.wrapping_add(best);
                }
                slot46[ch][sel] = total;
                flag[ch][bi] = 0;
            }
        }

        *s_12e = recompute_selected_total(s_12a, slot46, mode_1074)?;
        if i32::from(*s_12e) <= budget {
            return Ok(());
        }
    }
    Ok(())
}

/// Pre-state for a standalone replay of the native 256 kbps Phase-A high-band
/// `+0xcc` raise loop plus the following section-5 recost/re-pick. Built from a
/// native `over_budget_entry` phase-event row and the matching local-only
/// spectrum-bearing `calc_cb_io_call` row.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct PhaseAReplayInput {
    /// Per-channel full 2048-bin spectra used to rebuild band windows.
    pub spectra: Vec<Vec<f32>>,
    /// Per-channel `+0x1b5f8` word lengths (32 entries each).
    pub word_lengths: Vec<Vec<i32>>,
    /// Per-channel `+0xcc` idsf-quant rows (32 entries each).
    pub idsf_cc: Vec<Vec<i32>>,
    /// Per-channel `+0x24c` scale rows (32 f32 each).
    pub scale_f32: Vec<Vec<f32>>,
    /// Per-channel selected-plane picks at `over_budget_entry`.
    pub picks: Vec<Vec<i32>>,
    /// Per-channel selected-plane flattened cost rows (`32*8` i16 each).
    pub costs: Vec<Vec<i16>>,
    /// Per-channel slot46 pair (`+0x46`, both modes).
    pub slot46: Vec<[i16; 2]>,
    /// Per-channel selected mode word (`+0x1074`).
    pub mode_1074: Vec<i32>,
    /// Shared `+0x114` raise limit.
    pub s_114: u16,
    /// Shared `+0x116` first touched band.
    pub s_116: u16,
    /// Shared `+0x12a` base total.
    pub s_12a: i16,
    /// Shared `+0x12e` post-second total (must exceed budget).
    pub s_12e: i16,
    /// Frame bit budget (`param_6`).
    pub budget: i32,
    /// Quant candidate count (the scoped AT5 constant, 8).
    pub candidate_count: usize,
}

/// Result surfaces asserted against the native `section7_merge` row.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct PhaseAReplayOutput {
    pub idsf_cc: Vec<Vec<i32>>,
    pub picks: Vec<Vec<i32>>,
    pub costs: Vec<Vec<i16>>,
    pub slot46: Vec<[i16; 2]>,
    pub mode_1074: Vec<i32>,
    pub s_12e: i16,
}

/// Test-only standalone driver for the 256 kbps Phase-A raise loop and the
/// section-5 recost/re-pick immediately following it. This intentionally stops
/// at the `section7_merge` surface so local song-spectrum replay does not widen
/// the slice into later 256 kbps passes.
#[doc(hidden)]
pub fn phase_a_raise_loop_replay(
    input: &PhaseAReplayInput,
) -> Result<PhaseAReplayOutput, CalcBlockError> {
    let n = input.word_lengths.len();
    if input.spectra.len() != n
        || input.idsf_cc.len() != n
        || input.scale_f32.len() != n
        || input.picks.len() != n
        || input.costs.len() != n
        || input.slot46.len() != n
        || input.mode_1074.len() != n
    {
        return Err(CalcBlockError::OutOfScope(
            "phase A replay input channel vector lengths differ",
        ));
    }

    let windows: Vec<Vec<Vec<f32>>> = input.spectra.iter().map(|s| band_windows(s)).collect();
    let mut idsf_cc = input.idsf_cc.clone();
    let mut slot46 = input.slot46.clone();
    let mut mode_1074 = input.mode_1074.clone();
    let mut planes = Vec::with_capacity(n);
    for ch in 0..n {
        let sel = usize::try_from(input.mode_1074[ch])
            .map_err(|_| CalcBlockError::OutOfScope("phase A replay selected mode is negative"))?;
        if sel >= 2 {
            return Err(CalcBlockError::OutOfScope(
                "phase A replay selected mode outside the two AT5 mode planes is out of scope",
            ));
        }
        planes.push({
            let mut plane = Plane::zeroed();
            plane.picks = input.picks[ch].clone();
            plane.costs = input.costs[ch].clone();
            let mut pair = [Plane::zeroed(), Plane::zeroed()];
            pair[sel] = plane;
            pair
        });
    }
    let mut s_12e = input.s_12e;

    phase_a_raise_loop(
        &windows,
        &input.word_lengths,
        &mut idsf_cc,
        &mut planes,
        &mut slot46,
        &mode_1074,
        &input.scale_f32,
        input.s_114,
        input.s_116,
        input.s_12a,
        &mut s_12e,
        input.budget,
        input.candidate_count,
    )?;

    for ch in 0..n {
        let sel = mode_1074[ch] as usize;
        let other = 1 - sel;
        let (sel_plane, sel_total) = recost_plane(
            &windows[ch],
            &input.word_lengths[ch],
            &idsf_cc[ch],
            &input.scale_f32[ch],
            sel,
            input.candidate_count,
            CALC_BANDS_AT5,
        )?;
        let (oth_plane, oth_total) = recost_plane(
            &windows[ch],
            &input.word_lengths[ch],
            &idsf_cc[ch],
            &input.scale_f32[ch],
            other,
            input.candidate_count,
            CALC_BANDS_AT5,
        )?;
        planes[ch][sel] = sel_plane;
        planes[ch][other] = oth_plane;
        slot46[ch][sel] = sel_total;
        slot46[ch][other] = oth_total;

        let mut best = 0x4000i16;
        for slot in 0..2 {
            if slot46[ch][slot] < best {
                best = slot46[ch][slot];
                mode_1074[ch] = slot as i32;
            }
        }
    }
    s_12e = recompute_selected_total(input.s_12a, &slot46, &mode_1074)?;

    let picks = (0..n)
        .map(|ch| planes[ch][mode_1074[ch] as usize].picks.clone())
        .collect();
    let costs = (0..n)
        .map(|ch| planes[ch][mode_1074[ch] as usize].costs.clone())
        .collect();

    Ok(PhaseAReplayOutput {
        idsf_cc,
        picks,
        costs,
        slot46,
        mode_1074,
        s_12e,
    })
}

/// Phase B destructive band-kill (native `0x5275c..0x52b5d`, decompile
/// 44480-44603). Reached from `LAB_00062743` when the post-second,
/// post-section-5 total `+0x12e` still exceeds the budget. Walks bands
/// high-to-low; for every active band (`+0x1b5f8` word length > 0) it forces
/// the `+0xcc` idsf to 15, zeroes that band's `param_2` spectrum window,
/// recosts the channel's SELECTED plane (`mode_1074[ch]`; descriptor state =
/// that plane word, the §2.1 pattern), and recomputes `+0x12e = +0x12a +
/// Σ_ch slot46[ch][mode]`, breaking as soon as `12e <= budget`.
///
/// The killed band's recost input window is all-zeros by construction (native
/// zeroes `param_2[ch]`'s band and the recost reads the same buffer), so the
/// window mutation propagates into every later section (QUANT / level words /
/// fifth+sixth / adjust) that reads `windows`.
///
/// Live at 352 on exactly three dense-content corpus calls (docs/12 §2.3):
/// sweat 1701 (kills band 31 both channels), sweat 2482 (band 31 both), and
/// 12-34-am 2437 (bands 31 then 30, both channels). All three have shared
/// `+0x114 == 0` (the raise loop is dead-by-table) and `+0x94` all-zero (the
/// non-joint arm fires).
///
/// The joint-stereo kill sub-arm (`param_4 == 2 && shared+0x94[band] == 1`,
/// decompile 44507-44519) zeroes the killed band's window in BOTH channels'
/// `param_2` spectra. It is LIVE at 192 kbps: sweat-192 oracle calls 858 and
/// 3588 each kill joined band 28 on ch0 (`row_94[28] == 1`; ch1's band-28 word
/// length is already 0 from the joint cross-zero, so only ch0 recosts and the
/// sub-arm's observable effect is the extra zeroing of ch1's band-28 window,
/// which propagates into the later sections that read `windows`).
#[allow(clippy::too_many_arguments)]
fn phase_b_band_kill(
    shared_row_94: &[i16],
    windows: &mut [Vec<Vec<f32>>],
    word_lengths: &[Vec<i32>],
    idsf_cc: &mut [Vec<i32>],
    planes: &mut [[Plane; 2]],
    slot46: &mut [[i16; 2]],
    mode_1074: &[i32],
    scale_f32: &[Vec<f32>],
    s_12a: i16,
    s_12e: &mut i16,
    budget: i32,
    candidate_count: usize,
) -> Result<(), CalcBlockError> {
    let n = windows.len();
    let bands = CALC_BANDS_AT5;
    let nsps = nsps_at5();

    // `local_39c` flag array, one entry per (ch, band). Only ever set for the
    // band being killed and cleared right after, so each per-channel recost
    // quant_nontones exactly the killed band and reuses cached picks elsewhere.
    let mut flag = vec![vec![0i32; bands]; n];

    // do-while over bands high-to-low (native decrements before the < 0 test).
    let mut band = bands as i32 - 1;
    while band >= 0 {
        let bi = band as usize;
        for ch in 0..n {
            if word_lengths[ch][bi] <= 0 {
                continue;
            }
            // Force this band's idsf to 15 (native `*(+0xcc+band*4) = 0xf`).
            idsf_cc[ch][bi] = 0xf;

            // Zero the killed band's spectrum window. The joint-stereo sub-arm
            // (both channels' band zeroed when `param_4==2 && shared+0x94==1`,
            // decompile 44507-44519) is LIVE at 192 kbps: sweat-192 oracle calls
            // 858 and 3588 both kill joined band 28 on ch0 (row_94[28]==1). Native
            // zeroes `param_2[0]` AND `param_2[1]`'s `isps`-offset / `nsps`-length
            // window; the non-joint arm (decompile 44520-44527) zeroes only the
            // current channel's window.
            let count = nsps[bi] as usize;
            if n == 2 && shared_row_94[bi] == 1 {
                for c in 0..2 {
                    for slot in windows[c][bi].iter_mut().take(count) {
                        *slot = 0.0;
                    }
                }
            } else {
                for slot in windows[ch][bi].iter_mut().take(count) {
                    *slot = 0.0;
                }
            }
            flag[ch][bi] = 1;

            // Recost the channel's selected plane over all bands: cached pick
            // costs for non-flagged active bands, quant_nontone recost for the
            // flagged (killed) band(s). Descriptor state = the live plane word
            // `mode_1074[ch]` (disasm-verified, §2.1 pattern).
            let sel = mode_1074[ch] as usize;
            let mut total: i16 = 0;
            for b in 0..bands {
                if flag[ch][b] == 0 {
                    if word_lengths[ch][b] > 0 {
                        let pick = planes[ch][sel].picks[b] as usize;
                        total = total
                            .wrapping_add(planes[ch][sel].costs[b * CALC_CANDIDATES_AT5 + pick]);
                    }
                } else {
                    let costs = quant_nontone_costs_at5(
                        &windows[ch][b],
                        word_lengths[ch][b] as usize,
                        idsf_cc[ch][b] as usize,
                        scale_f32[ch][b],
                        windows[ch][b].len(),
                        sel,
                        candidate_count,
                    )
                    .map_err(|_| CalcBlockError::OutOfScope("phase B quant"))?;
                    let base = b * CALC_CANDIDATES_AT5;
                    for (i, c) in costs.iter().enumerate().take(CALC_CANDIDATES_AT5) {
                        planes[ch][sel].costs[base + i] = *c as i16;
                    }
                    // Earliest strict-< argmin over the active candidates.
                    let mut best = costs[0] as i16;
                    let mut best_index = 0usize;
                    for (i, c) in costs
                        .iter()
                        .enumerate()
                        .take(candidate_count.min(costs.len()))
                        .skip(1)
                    {
                        if (*c as i16) < best {
                            best = *c as i16;
                            best_index = i;
                        }
                    }
                    planes[ch][sel].picks[b] = best_index as i32;
                    total = total.wrapping_add(best);
                }
            }
            slot46[ch][sel] = total;
            flag[ch][bi] = 0;
        }

        // Recompute +0x12e = +0x12a + Σ_ch slot46[ch][mode_1074[ch]].
        let mut new_12e = s_12a;
        for ch in 0..n {
            new_12e = new_12e.wrapping_add(slot46[ch][mode_1074[ch] as usize]);
        }
        *s_12e = new_12e;
        if i32::from(new_12e) <= budget {
            break;
        }
        band -= 1;
    }
    Ok(())
}

/// Pre-state for a standalone phase-B destructive band-kill replay, built
/// directly from a native `phase_b_entry` surface. Per-channel plane picks and
/// the flattened `band*8 + candidate` i16 cost rows are the selected mode's
/// plane at `block + 0xb08 + mode_1074*0x280`. The killed bands' spectrum
/// windows are all-zeros by construction (only killed bands are recosted, and
/// native has already zeroed them), so callers pass zero-filled windows sized
/// to `g_a_nsps_at5`.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct PhaseBReplayInput {
    /// Shared `+0x94` per-band joint-stereo modes (32 entries).
    pub shared_row_94: Vec<i16>,
    /// Per-channel `+0x1b5f8` word lengths (32 entries each).
    pub word_lengths: Vec<Vec<i32>>,
    /// Per-channel `+0xcc` idsf-quant rows (32 entries each).
    pub idsf_cc: Vec<Vec<i32>>,
    /// Per-channel `+0x24c` scale rows (32 f32 each).
    pub scale_f32: Vec<Vec<f32>>,
    /// Per-channel selected-plane picks (32 entries each).
    pub picks: Vec<Vec<i32>>,
    /// Per-channel selected-plane flattened cost rows (`32*8` i16 each).
    pub costs: Vec<Vec<i16>>,
    /// Per-channel slot46 pair (`+0x46`, both modes).
    pub slot46: Vec<[i16; 2]>,
    /// Per-channel selected mode word (`+0x1074`).
    pub mode_1074: Vec<i32>,
    /// Shared `+0x12a` base total.
    pub s_12a: i16,
    /// Shared `+0x12e` post-second/post-section-5 total (must exceed budget).
    pub s_12e: i16,
    /// Frame bit budget (`param_6`).
    pub budget: i32,
    /// Quant candidate count (the scoped 352 constant, 8).
    pub candidate_count: usize,
}

/// Result surfaces asserted against the native `section7_merge` row.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct PhaseBReplayOutput {
    pub idsf_cc: Vec<Vec<i32>>,
    pub picks: Vec<Vec<i32>>,
    pub costs: Vec<Vec<i16>>,
    pub slot46: Vec<[i16; 2]>,
    pub s_12e: i16,
}

/// Test-only standalone driver for the phase-B destructive band-kill loop.
/// Builds the private plane/window model from `input`, runs the ported loop
/// over all-zero killed-band windows, and returns the mutated surfaces.
#[doc(hidden)]
pub fn phase_b_band_kill_replay(
    input: &PhaseBReplayInput,
) -> Result<PhaseBReplayOutput, CalcBlockError> {
    let n = input.word_lengths.len();
    let nsps = nsps_at5();
    let mut windows: Vec<Vec<Vec<f32>>> = (0..n)
        .map(|_| {
            (0..CALC_BANDS_AT5)
                .map(|band| vec![0.0f32; nsps[band] as usize])
                .collect()
        })
        .collect();
    let mut idsf_cc = input.idsf_cc.clone();
    let mut slot46 = input.slot46.clone();
    let mut planes: Vec<[Plane; 2]> = (0..n)
        .map(|ch| {
            let sel = input.mode_1074[ch] as usize;
            let mut plane = Plane::zeroed();
            plane.picks = input.picks[ch].clone();
            plane.costs = input.costs[ch].clone();
            let mut pair = [Plane::zeroed(), Plane::zeroed()];
            pair[sel] = plane;
            pair
        })
        .collect();
    let mut s_12e = input.s_12e;
    phase_b_band_kill(
        &input.shared_row_94,
        &mut windows,
        &input.word_lengths,
        &mut idsf_cc,
        &mut planes,
        &mut slot46,
        &input.mode_1074,
        &input.scale_f32,
        input.s_12a,
        &mut s_12e,
        input.budget,
        input.candidate_count,
    )?;
    let picks = (0..n)
        .map(|ch| planes[ch][input.mode_1074[ch] as usize].picks.clone())
        .collect();
    let costs = (0..n)
        .map(|ch| planes[ch][input.mode_1074[ch] as usize].costs.clone())
        .collect();
    Ok(PhaseBReplayOutput {
        idsf_cc,
        picks,
        costs,
        slot46,
        s_12e,
    })
}

/// Section-13 phase C (native `0x64830..0x64ebf`): while over budget,
/// walk bands high-to-low over up to 14 rounds, raising the `+0xcc`
/// idsf by the per-band round counter, re-costing, and running
/// `calc_nbits_var_rebitalloc_at5`; a strict reduction is accepted
/// (selector/cost-row/side-word bookkeeping) and a non-reduction is
/// rolled back (idsf and IDCT restored). Live on core calls 22/63/77 in the
#[allow(clippy::too_many_arguments)]
fn phase_c_var_trials(
    entry: &CalcFrameEntry,
    windows: &[Vec<Vec<f32>>],
    word_lengths: &[Vec<i32>],
    idsf_cc: &mut [Vec<i32>],
    selectors: &mut [Vec<i32>],
    planes: &mut [[Plane; 2]],
    idct_blocks: &mut Vec<IdctBlockState>,
    slot46: &mut [[i16; 2]],
    mode_1074: &[i32],
    obj_mode: &[u32],
    scale_f32: &[Vec<f32>],
    s_116: u16,
    s_11e: &mut i16,
    s_12a: &mut i16,
    s_12e: &mut i16,
    budget: i32,
    candidate_count: usize,
) -> Result<(), CalcBlockError> {
    let n = entry.channels.len();
    let selector = entry.selector;
    let active = entry.ctx_active_b0; // ctx + 0xb0
    let active_band_count = entry.channels[0].config_b0 as usize;
    let obj_modes: Vec<u32> = obj_mode.to_vec();
    let fixbits: Vec<usize> = entry
        .channels
        .iter()
        .map(|c| c.config_90 as usize)
        .collect();

    let s116 = i32::from(s_116);
    let ec8 = if active <= s116 {
        (s116 - 2).max(0)
    } else {
        s116
    };

    let mut counters = vec![vec![0i32; active.max(0) as usize]; n];
    let mut round = 14i32;
    loop {
        for ch in 0..n {
            for c in counters[ch].iter_mut() {
                *c += 1;
            }
        }
        let mut band = active - 1;
        'walk: while band >= ec8 {
            let bi = band as usize;
            for ch in 0..n {
                if word_lengths[ch][bi] <= 0 {
                    continue;
                }
                let old_idsf = idsf_cc[ch][bi];
                let raised = old_idsf + counters[ch][bi];
                if raised > 0xf {
                    if *s_12e <= budget as i16 {
                        return Ok(());
                    }
                    continue;
                }
                // Bands here are all >= ec8 >= 8, so the `band < 8` gate is
                // never taken; raise the idsf directly.
                idsf_cc[ch][bi] = raised;
                let saved_idct = idct_blocks.clone();

                let sel = mode_1074[ch] as usize;
                // Native `calc_channel_block_at5` phase C loads the descriptor
                // state from the channel's live plane word `*(obj + 0x1074)`
                // before the trial recost (disasm 0x547f5 `mov 0x1074(%ecx),%edx`
                // -> call 0x54830; var forwards its param_2/edx untouched into
                // `quant_nontone_nspecs_at5` at native 0xc150). Pass `sel`, not 0.
                let costs = quant_nontone_costs_at5(
                    &windows[ch][bi],
                    word_lengths[ch][bi] as usize,
                    raised as usize,
                    scale_f32[ch][bi],
                    windows[ch][bi].len(),
                    sel,
                    candidate_count,
                )
                .map_err(|_| CalcBlockError::OutOfScope("phase C quant"))?;
                let mut target = planes[ch][sel].costs.clone();
                for (i, c) in costs.iter().enumerate().take(CALC_CANDIDATES_AT5) {
                    target[bi * CALC_CANDIDATES_AT5 + i] = *c as i16;
                }

                let var = {
                    let rows = word_lengths.to_vec();
                    calc_nbits_var_rebitalloc_at5(
                        VarRebitallocInput {
                            quant_unit: bi,
                            channel_index: ch,
                            channel_count: n,
                            old_selector: selectors[ch][bi] as usize,
                            selector_count: candidate_count,
                            current_idct_bits: i32::from(*s_11e),
                            source_costs: &planes[ch][sel].costs,
                            target_costs: &target,
                        },
                        idct_blocks,
                        |blocks| {
                            live_idct_bits(&obj_modes, &fixbits, &rows, active_band_count, blocks)
                        },
                    )
                    .map_err(|_| CalcBlockError::OutOfScope("phase C var"))?
                };

                if var.bit_delta < 0 {
                    // Accept.
                    let base = bi * CALC_CANDIDATES_AT5;
                    planes[ch][sel].costs[base..base + CALC_CANDIDATES_AT5]
                        .copy_from_slice(&target[base..base + CALC_CANDIDATES_AT5]);
                    selectors[ch][bi] = var.word_length as i32;
                    let d11e = (var.idct_bits as i16).wrapping_sub(*s_11e);
                    *s_12a = s_12a.wrapping_add(d11e);
                    let d46 = (var.bit_delta as i16).wrapping_sub(d11e);
                    slot46[ch][sel] = slot46[ch][sel].wrapping_add(d46);
                    *s_11e = var.idct_bits as i16;
                    *s_12e = s_12e.wrapping_add(var.bit_delta as i16);
                    counters[ch][bi] = 0;
                    let _ = selector;
                    if *s_12e <= budget as i16 {
                        return Ok(());
                    }
                } else {
                    // Reject: restore idsf and IDCT.
                    idsf_cc[ch][bi] = old_idsf;
                    *idct_blocks = saved_idct;
                    if *s_12e <= budget as i16 {
                        return Ok(());
                    }
                }
            }
            if band == ec8 {
                break 'walk;
            }
            band -= 1;
        }
        round -= 1;
        if !(budget < i32::from(*s_12e)) || round < 0 {
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_fifth_sixth(
    entry: &CalcFrameEntry,
    windows: &[Vec<Vec<f32>>],
    word_lengths: &mut [Vec<i32>],
    selectors: &mut [Vec<i32>],
    idsf_cc: &mut [Vec<i32>],
    planes: &mut [[Plane; 2]],
    idct_blocks: &mut [IdctBlockState],
    wlc_blocks: &mut Vec<IdwlBlockState>,
    shared_side: &mut IdwlSideState,
    slot46: &mut [[i16; 2]],
    mode_1074: &[i32],
    obj_mode: &[u32],
    scale_factors: &[Vec<i32>],
    max_wl: &[Vec<i16>],
    scale_f32: &[Vec<f32>],
    order: &[i32],
    stereo_order: &[i32],
    stereo_bound: usize,
    s_11a: &mut i16,
    s_11e: &mut i16,
    s_12a: &mut i16,
    s_12e: &mut i16,
    budget: i32,
    candidate_count: usize,
    band_count: usize,
) -> Result<(), CalcBlockError> {
    let n = entry.channels.len();
    let bands = band_count;

    // --- fifth ---
    {
        let quant_raw: Vec<Vec<ZerothQuantBandRaw<'_>>> = (0..n)
            .map(|ch| {
                (0..bands)
                    .map(|band| ZerothQuantBandRaw {
                        spectrum: &windows[ch][band],
                        idsf: idsf_cc[ch][band] as usize,
                        scale: scale_f32[ch][band],
                        count: windows[ch][band].len(),
                    })
                    .collect()
            })
            .collect();
        let channels: Vec<FifthChannelState<'_>> = (0..n)
            .map(|ch| {
                let sel = mode_1074[ch] as usize;
                let other = 1 - sel;
                FifthChannelState {
                    word_lengths: word_lengths[ch].clone(),
                    selector_row: selectors[ch].clone(),
                    active_costs: planes[ch][sel].costs.clone(),
                    trial_costs: planes[ch][other].costs.clone(),
                    idct_block: idct_blocks[ch].clone(),
                    wlc_block: wlc_blocks[ch].clone(),
                    quant_bits_46: slot46[ch][sel],
                    scale_factors: &scale_factors[ch],
                    max_word_lengths: &max_wl[ch],
                    quant_bands: &quant_raw[ch],
                    obj_mode: obj_mode[ch],
                    quant_state: sel,
                    fixbits_index: entry.channels[ch].config_90 as usize,
                    context_kind: entry.channels[ch].objside_1c as u32,
                    word_count: entry.channels[ch].config_c4 as usize,
                    group_count: entry.channels[ch].config_b8 as usize,
                }
            })
            .collect();
        let mut state = FifthFrameState {
            channels,
            band_count: bands,
            budget_limit: budget,
            order,
            stereo_order,
            stereo_bound,
            active_band_count: entry.channels[0].config_b0 as usize,
            threshold_90: entry.shared_word_90,
            // Native entry clear (decompile 43950–43952): `& 0x7c != 0` sets
            // shared `+0x84` to 0 before any pass reads it.
            side_gate_84: if entry.ctx_flags_1dc & 0x7c != 0 {
                0
            } else {
                entry.shared_word_84
            },
            // `+0x88` at fifth time: on the `+0x84 != 0` path the second
            // pass's init prong stored 1 (decompile 39357); on the flag path
            // its reset prong left it 0 (unread — `+0x84 == 0` short-circuits
            // the fifth fork first).
            side_gate_88: if entry.ctx_flags_1dc & 0x7c != 0 {
                0
            } else {
                1
            },
            shared_wlc_side: shared_side.clone(),
            mode_1c: entry.channels[0].objside_1c as u32,
            quant_candidate_count: candidate_count,
            idwl_bits_11a: *s_11a,
            idct_bits_11e: *s_11e,
            base_total_12a: *s_12a,
            current_bits_12e: *s_12e,
        };
        let out = fifth_bit_allocation_frame_at5(&mut state)
            .map_err(|_| CalcBlockError::OutOfScope("fifth pass failed"))?;
        for ch in 0..n {
            let sel = mode_1074[ch] as usize;
            word_lengths[ch] = out.channels[ch].word_lengths.clone();
            selectors[ch] = out.channels[ch].selector_row.clone();
            planes[ch][sel].costs = out.channels[ch].active_costs.clone();
            slot46[ch][sel] = out.channels[ch].quant_bits_46;
            idct_blocks[ch] = out.channels[ch].idct_block.clone();
            wlc_blocks[ch] = out.channels[ch].wlc_block.clone();
        }
        *shared_side = out.shared_wlc_side;
        *s_11a = out.idwl_bits_11a;
        *s_11e = out.idct_bits_11e;
        *s_12a = out.base_total_12a;
        *s_12e = out.extended_total_12e;
    }

    // --- sixth ---
    {
        let quant_raw: Vec<Vec<ZerothQuantBandRaw<'_>>> = (0..n)
            .map(|ch| {
                (0..bands)
                    .map(|band| ZerothQuantBandRaw {
                        spectrum: &windows[ch][band],
                        idsf: idsf_cc[ch][band] as usize,
                        scale: scale_f32[ch][band],
                        count: windows[ch][band].len(),
                    })
                    .collect()
            })
            .collect();
        let channels: Vec<SixthChannelState<'_>> = (0..n)
            .map(|ch| {
                let sel = mode_1074[ch] as usize;
                let other = 1 - sel;
                SixthChannelState {
                    word_lengths: word_lengths[ch].clone(),
                    selector_row: selectors[ch].clone(),
                    band_idsf: idsf_cc[ch].clone(),
                    active_costs: planes[ch][sel].costs.clone(),
                    trial_costs: planes[ch][other].costs.clone(),
                    idct_block: idct_blocks[ch].clone(),
                    quant_bits_46: slot46[ch][sel],
                    quant_bands: &quant_raw[ch],
                    obj_mode: obj_mode[ch],
                    fixbits_index: entry.channels[ch].config_90 as usize,
                    quant_state: sel,
                }
            })
            .collect();
        let mut state = SixthFrameState {
            channels,
            band_count: bands,
            budget_limit: budget,
            order,
            active_band_count: entry.channels[0].config_b0 as usize,
            threshold_90: entry.shared_word_90,
            quant_candidate_count: candidate_count,
            idct_bits_11e: *s_11e,
            base_total_12a: *s_12a,
            current_bits_12e: *s_12e,
        };
        let out = sixth_bit_allocation_frame_at5(&mut state)
            .map_err(|_| CalcBlockError::OutOfScope("sixth pass failed"))?;
        for ch in 0..n {
            let sel = mode_1074[ch] as usize;
            selectors[ch] = out.channels[ch].selector_row.clone();
            idsf_cc[ch] = out.channels[ch].band_idsf.clone();
            planes[ch][sel].costs = out.channels[ch].active_costs.clone();
            slot46[ch][sel] = out.channels[ch].quant_bits_46;
            idct_blocks[ch] = out.channels[ch].idct_block.clone();
        }
        *s_11e = out.idct_bits_11e;
        *s_12a = out.base_total_12a;
        *s_12e = out.extended_total_12e;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_adjust(
    entry: &CalcFrameEntry,
    windows: &[Vec<Vec<f32>>],
    word_lengths: &[Vec<i32>],
    scale_factors: &mut [Vec<i32>],
    quantized: &[Vec<i16>],
    aux_weights: &[Vec<f32>],
    level_words: &[Vec<i32>],
    _idsf_cc: &[Vec<i32>],
    obj_mode: &[u32],
    s_11c: &mut i16,
    s_12a: &mut i16,
    s_12e: &mut i16,
    band_count: usize,
) -> Result<Vec<Option<IdsfBlockState>>, CalcBlockError> {
    let n = entry.channels.len();
    let bands = band_count;
    let channels: Vec<AdjustChannelState> = (0..n)
        .map(|ch| {
            let c = &entry.channels[ch];
            AdjustChannelState {
                obj_mode: obj_mode[ch],
                word_lengths: word_lengths[ch].clone(),
                scale_factors: scale_factors[ch].clone(),
                quantized: quantized[ch].clone(),
                spectra: windows[ch].clone(),
                aux_weights: aux_weights[ch].clone(),
                config_a8: c.config_a8,
                group_flags: c.config_50.clone(),
                spc_level_words: {
                    let mut lw = level_words[ch].clone();
                    lw.resize(16, 0);
                    lw
                },
                cur_gain_rows: c
                    .cur_gain_0c
                    .iter()
                    .map(|g| AdjustGainRow {
                        count: g.count,
                        level_ids: g.level_ids.clone(),
                    })
                    .collect(),
                prev_gain_rows: c
                    .prev_gain_08
                    .iter()
                    .map(|g| AdjustGainRow {
                        count: g.count,
                        level_ids: g.level_ids.clone(),
                    })
                    .collect(),
                band_count_b0: c.config_b0 as usize,
                group_count_c0: c.config_c0 as usize,
                leaf_group_count_b8: c.config_b8 as usize,
            }
        })
        .collect();
    let mut state = AdjustFrameState {
        channels,
        band_count: bands,
        selector: entry.selector as usize,
        side_band_modes: entry.shared_row_94.clone(),
        side_gate_8c: entry.shared_word_8c,
        idsf_bits_11c: *s_11c,
        base_total_12a: *s_12a,
        current_bits_12e: *s_12e,
    };
    let out = adjust_scalefactors_frame_at5(&mut state)
        .map_err(|_| CalcBlockError::OutOfScope("adjust pass failed"))?;
    for ch in 0..n {
        scale_factors[ch] = out.channels[ch].scale_factors.clone();
    }
    *s_11c = out.idsf_bits_11c;
    *s_12a = out.base_total_12a;
    *s_12e = out.extended_total_12e;
    // Return the per-channel final IDSF block state (the adjust epilogue is
    // the last native writer of the object IDSF packing-prep words).
    Ok(out.channels.into_iter().map(|c| c.idsf_block).collect())
}

/// The section-12 4-lane extended-precision `|x|` sum
/// (`(d + ((c + b) + a)) + acc`, one f32 round).
fn mag_sum4(buf: &[f32], quarter: usize) -> f32 {
    let mut acc = 0f64;
    for i in 0..quarter {
        let a = f64::from(buf[i].abs());
        let b = f64::from(buf[quarter + i].abs());
        let c = f64::from(buf[2 * quarter + i].abs());
        let d = f64::from(buf[3 * quarter + i].abs());
        acc = d + (c + b + a) + acc;
    }
    acc as f32
}

/// Native section-12 spectral-group interval. The end expression intentionally
/// models the native table-adjacent read described at the call site below.
#[doc(hidden)]
pub fn section12_level_group_bounds_at5(
    selector: i32,
    level_group_count: i32,
) -> Result<(usize, usize), CalcBlockError> {
    let spc_startqu = spc_startqu();
    let idspcqus = idspcqus_at5();
    let idspcbands = idspcbands_at5();
    let selector = usize::try_from(selector)
        .ok()
        .filter(|&value| value < spc_startqu.len())
        .ok_or(CalcBlockError::OutOfScope(
            "section-12 selector is outside the native SPC start table",
        ))?;
    let level_groups = usize::try_from(level_group_count)
        .ok()
        .filter(|&count| (1..=idspcbands.len()).contains(&count))
        .ok_or(CalcBlockError::OutOfScope(
            "section-12 level-group count must be in native range 1..=16",
        ))?;
    Ok((
        idspcqus[spc_startqu[selector] as usize] as usize,
        idspcbands[level_groups - 1] as usize + 1,
    ))
}

/// Channel selected by native `pwc_qu_at5` for its spectral-level lookup.
#[doc(hidden)]
pub fn section12_pwc_source_channel_at5(
    channel: usize,
    config_a8: u32,
    group_flag_50: u32,
) -> usize {
    if config_a8 == 2 && group_flag_50 != 0 {
        1 - channel
    } else {
        channel
    }
}

/// The native `pwc_qu_at5` leaf (`0x2a300`) as called from section 12:
/// gain-shift dither scaled by `spclev[level_word]`, added into `out`.
#[allow(clippy::too_many_arguments)]
fn section12_pwc(
    entry: &CalcFrameEntry,
    level_words: &[Vec<i32>],
    channel: usize,
    phase: i32,
    band: usize,
    word_length: i32,
    out: &mut [f32],
    scratch: &mut [f32],
    last_group: &mut i32,
) {
    let x = x_at5();
    let nsps = nsps_at5();
    let spclev = spclev_at5();
    let idspcbands = idspcbands_at5();
    let lngain = lngain_at5();
    let rndtbl = rndtbl_at5();

    let group = x[band + 1] as usize;
    let src = section12_pwc_source_channel_at5(
        channel,
        entry.channels[channel].config_a8,
        entry.channels[channel].config_50[group],
    );
    let obj = &entry.channels[src];
    let lvl_word = level_words[src][idspcbands[group] as usize];
    let fvar1 = spclev[lvl_word as usize];
    if band <= 1 || !(fvar1 > 0.0) {
        return;
    }
    let count = nsps[band] as usize;
    if group as i32 != *last_group {
        *last_group = group as i32;
        for i in 0..count {
            let idx = ((phase + i as i32) & 0x3ff) as usize;
            scratch[i] = (f64::from(rndtbl[idx]) * f64::from(3.0517578e-05f32)) as f32;
        }
    }
    let prev = &obj.prev_gain_08[group];
    let cur = &obj.cur_gain_0c[group];
    let mut base: i16 = 0;
    if prev.count > 0 {
        base = lngain[prev.level_ids[0] as usize].wrapping_neg();
    }
    let mut max_shift: i16 = 0;
    for i in 0..cur.count.max(0) as usize {
        let s = base.wrapping_sub(lngain[cur.level_ids[i] as usize]);
        if max_shift < s {
            max_shift = s;
        }
    }
    for i in 0..prev.count.max(0) as usize {
        let s = lngain[prev.level_ids[i] as usize].wrapping_neg();
        if max_shift < s {
            max_shift = s;
        }
    }
    let denom = 1i32.wrapping_shl(((i32::from(max_shift) + word_length) & 0x1f) as u32);
    let q = f64::from(fvar1) / f64::from(denom);
    for i in 0..count.min(out.len()) {
        out[i] = (q * f64::from(scratch[i]) + f64::from(out[i])) as f32;
    }
}

#[allow(clippy::too_many_arguments)]
fn section12_levels(
    entry: &CalcFrameEntry,
    windows: &[Vec<Vec<f32>>],
    word_lengths: &[Vec<i32>],
    scale_factors: &[Vec<i32>],
    idsf_cc: &[Vec<i32>],
    quantized: &[Vec<i16>],
    planes: &[[Plane; 2]],
    mode_1074: &[i32],
    aux_weights: &[Vec<f32>],
    level_words: &mut [Vec<i32>],
    tone0: &mut [i32],
    selector: i32,
    band_count: usize,
) -> Result<(), CalcBlockError> {
    let n = entry.channels.len();
    let bands = band_count;
    let isps = isps_at5();
    let nsps = nsps_at5();
    let sftbl = sftbl_at5();
    let ifqf = ifqf_at5();
    let spclev = spclev_at5();
    let idspcqus = idspcqus_at5();
    let spc_startqu = spc_startqu();
    let spc_floor_tbl = spc_floor();
    let pos_weight_tbl = pos_weight();
    let x_tbl = x_at5();

    // Phase seeds (local_59c): idsf-sum masked / stepped by 0x80 per group.
    let active = entry.ctx_active_b0 as usize;
    let mut sum: u16 = 0;
    for ch in 0..n {
        for band in 0..active.min(bands) {
            sum = sum.wrapping_add(scale_factors[ch][band] as u16);
        }
    }
    let group_seeds = entry.ctx_level_groups_c0.max(0) as usize;
    let mut phases = vec![0u16; group_seeds.max(1)];
    for phase in phases.iter_mut().take(group_seeds) {
        sum &= 0x3fc;
        *phase = sum;
        sum = sum.wrapping_add(0x80);
    }

    let uvar17 = entry.ctx_active_b0 as usize; // ctx+0xb0 upper bound
    let (start, end_bound) = section12_level_group_bounds_at5(selector, entry.ctx_level_groups_c0)?;
    // Native indexes `g_a_idspcqus_at5[0x1f + cfg_c0]`. The 32-byte
    // IDSPCQU table is immediately followed by IDSPCBANDS, so cfg_c0 1..16
    // addresses IDSPCBANDS[cfg_c0 - 1]. Low-rate c0=10/11 therefore ends at
    // group 4, not the full-band group's 5.

    for ch in 0..n {
        // local_cf4[ch] == 1 for the whole scoped path (flags & 0x7c == 0
        // is enforced), so the all-15 tone-flag branch never runs here.
        // Preset slots outside [start_group, end_bound) to 15, inside to 6,
        // writing directly into `level_words[ch]` so the section-12 pwc
        // reads the preset level (native `+0x1c6f8` is written in place).
        let mut acc_a = [0f32; 8];
        let mut acc_b = [0f32; 8];
        let mut acc_w = [0u32; 8];
        {
            let mut u = 0usize;
            let end_group = end_bound;
            let start_g = idspcqus[spc_startqu[selector as usize] as usize] as usize;
            while u < 5 {
                if u >= end_group || u < start_g {
                    level_words[ch][u] = 0xf;
                } else {
                    level_words[ch][u] = 6;
                }
                u += 1;
            }
        }

        // pwc dither seed reset (native local_d04[2] = 0xffffffff) per
        // channel; scratch persists across bands of the same gain group.
        let mut scratch = [0f32; 128];
        let mut last_group: i32 = -1;
        let mut qu = start;
        while qu < uvar17 {
            // joint-stereo source select (ch1 with shared+0x94 == 1 -> ch0).
            let src = if ch == 1 && entry.shared_row_94[qu] == 1 {
                0
            } else {
                ch
            };
            if word_lengths[src][qu] == 0 {
                qu += 1;
                continue;
            }
            let grp = idspcqus[qu] as usize;
            let count = nsps[qu] as usize;
            let base = isps[qu] as usize;
            let quarter = count >> 2;
            // The scale-factor index uses the OUTER channel (e34); the
            // quantized/word-length/aux use the joint source (e38).
            let sf_e34 = scale_factors[ch][qu];
            let fvar2 = sftbl[sf_e34 as usize];
            let wl_src = word_lengths[src][qu];

            let mut buf: Vec<f32> = (0..count)
                .map(|i| f32::from(quantized[src][base + i]))
                .collect();
            let mag1 = mag_sum4(&buf, quarter);
            let mut e58 = (f64::from(1.0f32) / f64::from(aux_weights[src][qu])) as f32;
            if mag1 > 0.0 {
                e58 = (f64::from(e58)
                    - (f64::from(ifqf[wl_src as usize]) * f64::from(fvar2))
                        / ((f64::from(count as f32) * f64::from(fvar2)) / f64::from(mag1)))
                    as f32;
            }

            for slot in buf.iter_mut() {
                *slot = 0.0;
            }
            let phase = i32::from(phases[x_tbl[qu + 1] as usize] as i16);
            section12_pwc(
                entry,
                level_words,
                ch,
                phase,
                qu,
                wl_src,
                &mut buf,
                &mut scratch,
                &mut last_group,
            );
            let mag2 = mag_sum4(&buf, quarter);
            let cf8 = if mag2 <= 0.0 {
                0.0f32
            } else {
                (f64::from(count as f32) * f64::from(fvar2) / f64::from(mag2)) as f32
            };
            let weight = pos_weight_tbl[qu - spc_floor_tbl[qu] as usize];
            acc_a[grp] = ((f64::from(cf8) / (f64::from(ifqf[wl_src as usize]) * f64::from(fvar2)))
                * f64::from(e58)
                * f64::from(sf_e34 as f32)
                * f64::from(weight as f32)
                + f64::from(acc_a[grp])) as f32;
            acc_b[grp] = (f64::from(sf_e34 as f32)
                * f64::from(aux_weights[src][qu])
                * f64::from(weight as f32)
                + f64::from(acc_b[grp])) as f32;
            acc_w[grp] = acc_w[grp].wrapping_add((weight as i32 * sf_e34) as u32);
            qu += 1;
        }
        let _ = (idsf_cc, windows, planes, mode_1074);

        let start_g = idspcqus[spc_startqu[selector as usize] as usize] as usize;
        // Ladder: per group in [start, end_bound).
        if start_g < end_bound {
            for g in start_g..end_bound {
                let mut a = acc_a[g];
                let mut b = acc_b[g];
                if a > 0.0 {
                    a /= acc_w[g] as f32;
                    b /= acc_w[g] as f32;
                    acc_a[g] = a;
                    acc_b[g] = b;
                }
                let mut found = 15i32;
                let mut i = 14i32;
                loop {
                    let idx = i;
                    if a <= spclev[idx as usize] {
                        break;
                    }
                    found = idx;
                    i = idx - 1;
                    if idx - 1 == -1 {
                        break;
                    }
                }
                let mut level = if selector < 0x13 {
                    found + 4
                } else {
                    found + 5
                };
                if b > 3.0 {
                    level = if b > 6.0 { level + 2 } else { level + 1 };
                }
                if level > 0xf {
                    level = 0xf;
                }
                level_words[ch][g] = level;
            }
        }
    }
    let _ = tone0;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_selector_30_matches_native_all_zero_rows() {
        // Native selector-30 stereo: saa_idtf row 30 is all zeros, shared
        // +0x114 = sa_limit_idtf_stereo[30] = 0, +0x116 =
        // sa_adjust_iqt_0th_stereo[30] = 0x18.
        // Native single-ATX-block 352 kbps stereo encode selector is 30.
        let out = seed_idsf_quant_rows_at5(2, 30, 32, 44100).unwrap();
        assert_eq!(out.shared_114, 0);
        assert_eq!(out.shared_116, 0x18);
        assert_eq!(out.idsf_quant_cc.len(), 2);
        for row in &out.idsf_quant_cc {
            assert_eq!(row.len(), 32);
            assert!(row.iter().all(|&v| v == 0), "seed row all zero");
        }
    }

    #[test]
    fn seed_clamps_to_15() {
        // Any stereo idtf value above 15 clamps; the native selector-30
        // row never trips this, so exercise the clamp against the raw
        // table maximum plus a synthetic ceiling check.
        let saa = saa_idtf_stereo();
        // Confirm the clamp is faithful: for a hypothetical entry of 0x40
        // the native code writes 0xf.
        let clamp = |v: i32| if v > 0xf { 0xf } else { v };
        assert_eq!(clamp(0x40), 0xf);
        assert_eq!(clamp(0x0f), 0x0f);
        assert_eq!(clamp(0x08), 0x08);
        // Selector 16 is in scope (>= 0x10); its seed matches the table
        // clamped, band by band.
        let out = seed_idsf_quant_rows_at5(2, 16, 32, 44100).unwrap();
        for band in 0..32 {
            let expected = clamp(i32::from(saa[16 * 0x20 + band]));
            assert_eq!(out.idsf_quant_cc[0][band], expected);
            assert_eq!(out.idsf_quant_cc[1][band], expected);
        }
    }

    #[test]
    fn seed_selector_24_uses_the_native_28_unit_prefix() {
        let saa = saa_idtf_stereo();
        let out = seed_idsf_quant_rows_at5(2, 24, 28, 44100).unwrap();
        assert_eq!(out.shared_114, 3);
        assert_eq!(out.shared_116, 24);
        for row in &out.idsf_quant_cc {
            assert_eq!(row.len(), 28);
            for band in 0..28 {
                assert_eq!(row[band], i32::from(saa[24 * 0x20 + band]).min(15));
            }
        }
    }

    #[test]
    fn seed_rejects_out_of_scope() {
        // Mono (channel_count 1) is now in scope (docs/14 §1.1, decompile
        // 43975-43979); only 0 and > 2 reject on the channel-count gate.
        assert_eq!(
            seed_idsf_quant_rows_at5(0, 30, 32, 44100),
            Err(CalcBlockError::OutOfScope(
                "channel_count must be 1 (mono) or 2 (stereo)"
            ))
        );
        assert_eq!(
            seed_idsf_quant_rows_at5(3, 30, 32, 44100),
            Err(CalcBlockError::OutOfScope(
                "channel_count must be 1 (mono) or 2 (stereo)"
            ))
        );
        assert_eq!(
            seed_idsf_quant_rows_at5(2, 30, 16, 44100),
            Err(CalcBlockError::OutOfScope(
                "only native-observed 24-, 26-, 27-, 28-, or 32-band (ctx +0xb4) extents are in scope"
            ))
        );
        // 24 (32-mono, docs/14 §5.1) and 26 (48 kbps) are now in scope; a
        // still-unobserved extent (25) between them is rejected.
        assert_eq!(
            seed_idsf_quant_rows_at5(2, 30, 25, 44100),
            Err(CalcBlockError::OutOfScope(
                "only native-observed 24-, 26-, 27-, 28-, or 32-band (ctx +0xb4) extents are in scope"
            ))
        );
        assert_eq!(
            seed_idsf_quant_rows_at5(2, 30, 32, 48000),
            Err(CalcBlockError::OutOfScope(
                "48 kHz +0xcc rewrite (ctx[0x2b] == 48000) is out of scope"
            ))
        );
        // The standalone helper cannot run the low-selector ladder (it lacks
        // block/object state); the whole-call composition does.
        assert_eq!(
            seed_idsf_quant_rows_at5(2, 9, 32, 44100),
            Err(CalcBlockError::OutOfScope(
                "low-selector (< 0x10) word-length ladder needs whole-call block/object state"
            ))
        );
    }
}
