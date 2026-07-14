//! Coding->packer bridge (docs/09 Phase 3 Slice E): serialize the
//! `calc_channel_block_at5` decision outputs into the packer's object memory
//! windows.
//!
//! `pack_frame_at5` (`src/bitstream/frame.rs`) replays a per-block
//! [`ObjectState`](crate::bitstream::frame::ObjectState) whose `range_a`
//! (`[0, 0x1110)`) and `range_b` (`[0x1b480, 0x1cc00)`) windows are captured raw
//! object memory. Phase 2 proved the packer byte-exact from those *captured*
//! windows. Slice E proves the packer's calc-decision surface is *buildable* by
//! writing it from the coding outputs
//! ([`CalcChannelOutput`](crate::coding::calc_block::CalcChannelOutput), produced
//! byte-exact by `calc_channel_block_frame_at5`) instead of captured.
//!
//! # What this serializes (byte-exact vs the captured window)
//!
//! `CalcChannelOutput` produces exactly the contiguous object region
//! `[0x1b578, 0x1c718)` in `range_b`, plus one word at `0x1074` in `range_a`.
//! The region tiles with no gaps:
//!
//! | Object offset        | Bytes  | Source field            | Packer read |
//! |----------------------|--------|-------------------------|-------------|
//! | `0x1b578 + qu*4`     | 32*4   | `o_1b578` (selectors)   | IDCT value / spectral selector |
//! | `0x1b5f8 + qu*4`     | 32*4   | `o_1b5f8` (word lengths)| spectral word length / IDWL(4) current |
//! | `0x1b678 + band*4`   | 32*4   | `o_1b678` (idsf/scalef.)| IDSF(4) current values |
//! | `0x1b6f8 + isps*2`   | 2048*2 | `o_1b6f8` (quantized)   | spectral descriptor samples |
//! | `0x1c6f8 + k*4`      | 8*4    | `o_1c6f8` (level words) | IDSPCQU tail + `0x1c70c` idwl mode |
//! | `0x1074`  (range_a)  | 4      | `mode_1074`             | IDCT 1-bit / spectral bandwidth word |
//!
//! `o_1c6f8[5]` aliases `0x1c70c` (`0x1c6f8 + 5*4 == 0x1c70c`), the WLC/tone
//! flag the packer reads as the IDWL mode (`CalcChannelOutput.o_1c70c0`); the
//! native calc pass writes `level_words[5] = tone0`, so serializing `o_1c6f8`
//! already lays down that byte range. Confirmed byte-exact for both channels of
//! native output frame 0 (core call 7) against `frame0_prepacker_state`
//! `range_b`/`range_a`.
//!
//! # Block->object IDCT copy (`range_a [0x1078, 0x1104)`, computed)
//!
//! The tail of `calc_channel_block_at5` (native offset `0x51a80`; Ghidra
//! comment `0x61a80`; `decompiled/libatrac.c` line 43813) copies the block's
//! final IDCT state (`block+0x9f8`) into the packer object's `range_a` window.
//! The two decompile-duplicated tail sites (lines 44918-44930 guarded single
//! object, 45400-45416 per-channel loop) copy:
//!
//! ```text
//! obj[0x1078] = block[0x9f8]   // IDCT mode
//! obj[0x107c] = block[0x9fc]   // band count
//! obj[0x1080] = block[0xa00]   // split flag
//! for i in 0..(cfg[0xb0] & 0x3fffffff):
//!     obj[0x1084 + i*4] = block[0xa04 + i*4]   // per-band flags
//! ```
//!
//! where `cfg = *(int *)(obj + 4)` is the shared cfg window and `cfg[0xb0]` is
//! copied object window is exactly `[0x1078, 0x1104)` (3 header words + 32 band
//! words = 35 words). The block's aux words (`block+0xa38..`) are NOT copied.
//! calls 7/12/22, both channels): the captured object `o_1078_i32` equals the
//! block return `b_9f8_i32` for exactly the first 35 words and diverges after.
//! [`serialize_idct_object_range_a`] emits this window from
//! [`CalcChannelOutput::idct_block`] (the final `block+0x9f8` state).
//!
//! # CUT (sourced from the captured window, NOT a `CalcChannelOutput` field)
//!
//! Everything else the packer reads is produced by stages this serializer does
//! not compose. It stays captured-CUT and is labeled here so a later slice can
//! retire it:
//!
//! * `range_b [0x1b480, 0x1b494)` — gain classification header words
//!   `0x1b484/0x1b488/0x1b48c/0x1b490` (record-present flag, delta flag, prev
//!   count, row count). Owner: Slice D init (`init_channel_block_frame_at5`
//!   reproduces them byte-exact); composed by the bridge 1.1-era init pass, not
//!   this serializer. The `0x1b494..0x1b578` NGC/IDLEV/IDLOC packing-prep window
//!   is NO LONGER cut — [`serialize_gain_modes_range_b`] (bridge 1.3) computes
//!   it from the gain rows.
//! * `range_b [0x1c718, 0x1cc00)` — the GHA modes/active-flags region
//!   (`0x1c75c/0x1c760/0x1c764/0x1c768/0x1c770/0x1c7b0`) is NO LONGER cut —
//!   [`serialize_gha_selectors_range_b`] (bridge 1.5) computes it from the arena
//!   state via [`compute_gha_packing_prep`]. The IDWL packing-prep
//!   (`0x1c71c/0x1c720/0x1c724/0x1c728/0x1c72c/0x1c7f0`) and IDSF mode +
//!   packing-prep (`0x1c73c/0x1c740/0x1c744/0x1c748/0x1c74c/0x1c750/0x1c754/`
//!   `0x1c758/0x1c8f0/0x1ca70`) portions are also NO LONGER cut —
//!   [`serialize_idwl_object_range_b`] (from [`CalcChannelOutput::idwl_block`],
//!   bridge 1.4) and [`serialize_idsf_object_range_b`] (from
//!   [`CalcChannelOutput::idsf_block`], bridge 1.4) compute them.
//! * `gha_arena` / `gha_p1` / `gha_records` windows — NO LONGER cut:
//!   [`serialize_gha_header_block`] / [`serialize_gha_p1_window`] /
//!   [`gha_channel_records_to_waves`] (bridge 1.5) serialize them, and the cfg
//!   IDSF predictor map (`0x11c`) is written by [`serialize_gha_cfg_map`]. CUT
//!   CAVEAT: the arena ROW/RECORD CONTENT at call 7 is ring-delayed frontend
//!   output (the packer reads `*(obj+0x14)`, an arena from an earlier core
//!   call). Phase 2.1 owns that rotation; this bridge computes the decision
//!   surface + byte layout from content parsed out of the captured arena.
//! * remaining CUT: the `cfg` float scratch regions, the `*(obj+8)` `gainb`
//!   gain-row buffer, and the `range_b [0x1b480, 0x1b494)` gain-classification
//!   init header. Not owned by this serializer.

use crate::bitstream::frame::{GhaWave, ObjectWindow};
use crate::coding::allocation::{
    AllocationError, ZerothActivitySummary, ZerothGainLevelBand, ZerothGainLocationBand,
    zeroth_activity_summary_at5, zeroth_gain_idlev_mode_at5, zeroth_gain_idlev_mode_ch1_at5,
    zeroth_gain_idloc_mode_at5, zeroth_gain_idloc_mode_ch1_at5, zeroth_gain_ngc_mode_at5,
};
use crate::coding::bitcount::{IdctBlockState, IdsfBlockState, IdwlBlockState};
use crate::coding::calc_block::CalcChannelOutput;
use crate::gha::bitcount::{
    GhaNbitsRow, GhaNbitsSelectorChannel, GhaNbitsSelectorRow, calc_nbits_for_gha_at5,
    calc_nbits_gha_flag_summary_at5, calc_nbits_gha_swap_plan_at5,
};
use crate::gha::synthesis::GhaWaveRecord;

/// Object offset of the packer `range_b` window base (`[0x1b480, 0x1cc00)`).
pub const OBJECT_RANGE_B_BASE: usize = 0x1b480;

/// `o_1b578` selector plane (32 words, IDCT value / spectral selector).
pub const OFFSET_1B578: usize = 0x1b578;
/// `o_1b5f8` word-length plane (32 words).
pub const OFFSET_1B5F8: usize = 0x1b5f8;
/// `o_1b678` scale-factor / idsf plane (32 words).
pub const OFFSET_1B678: usize = 0x1b678;
/// `o_1b6f8` quantized i16 spectral plane (2048 samples).
pub const OFFSET_1B6F8: usize = 0x1b6f8;
/// `o_1c6f8` spectral level words (8 words; word 5 aliases `0x1c70c`).
pub const OFFSET_1C6F8: usize = 0x1c6f8;
/// `mode_1074` descriptor bandwidth / plane mode (1 word in `range_a`).
pub const OFFSET_1074: usize = 0x1074;

// --- Block->object IDCT copy (`range_a`) offsets --------------------------

/// IDCT mode word (`0x1078`; `block+0x9f8`). Packer reads 2 bits + dispatch.
pub const OFFSET_1078: usize = 0x1078;
/// IDCT band count word (`0x107c`; `block+0x9fc`). Explicit split count.
pub const OFFSET_107C: usize = 0x107c;
/// IDCT split flag (`0x1080`; `block+0xa00`).
pub const OFFSET_1080: usize = 0x1080;
/// IDCT per-band flags base (`0x1084 + i*4`; `block+0xa04 + i*4`).
pub const OFFSET_1084: usize = 0x1084;
/// The inclusive-exclusive object byte range the IDCT copy fills in `range_a`:
/// `[0x1078, 0x1104)` (3 header words + 32 band words = 35 words), when
/// `cfg[0xb0] & 0x3fffffff == 32`.
pub const IDCT_RANGE_A: std::ops::Range<usize> = 0x1078..0x1104;

/// Native mask on the per-band copy count (`cfg[0xb0] & 0x3fffffff`).
const IDCT_COUNT_MASK: u32 = 0x3fff_ffff;

/// The inclusive-exclusive object byte range the calc-decision surface fills in
/// `range_b`: `[0x1b578, 0x1c718)`, contiguous.
pub const CALC_RANGE_B: std::ops::Range<usize> = 0x1b578..0x1c718;

// --- Gain packing-prep (`range_b`) offsets (bridge 1.3) --------------------
//
// Written by the gain side-data section of `zeroth_bit_allocation_at5` (native
// `0x52360`; `decompiled/libatrac.c` lines 36782-37574), per object, gated on
// `obj[0x1b484] != 0`. `*obj` (the object's channel index) picks the branch:
// 0 -> ch0, else -> ch1. See [`serialize_gain_modes_range_b`].

/// NGC mode word (`piVar24[0x6d25]`; decompile line 36901). Both channels.
pub const OFFSET_1B494: usize = 0x1b494;
/// NGC range-mode width (`0x6d26`; 36845). ch0 only, written unconditionally
/// during scoring when active (can hold the `0x4000` sentinel).
pub const OFFSET_1B498: usize = 0x1b498;
/// NGC range-mode minimum (`0x6d27`; 36846). ch0 only.
pub const OFFSET_1B49C: usize = 0x1b49c;
/// IDLEV mode word (`0x6d28`; 37170). Both channels.
pub const OFFSET_1B4A0: usize = 0x1b4a0;
/// IDLEV range-mode width (`0x6d29`; 37023). ch0 only (can hold `0x4000`).
pub const OFFSET_1B4A4: usize = 0x1b4a4;
/// IDLEV range-mode minimum (`0x6d2a`; 37025). ch0 only.
pub const OFFSET_1B4A8: usize = 0x1b4a8;
/// IDLEV per-row copy-flag base (`0x1b4ac + r*4`; `0x6d2b`; 37086/37106). ch1
/// only, one word per row for `r < row_count`.
pub const OFFSET_1B4AC: usize = 0x1b4ac;
/// IDLOC mode word (`0x6d3b`; 37566). Both channels.
pub const OFFSET_1B4EC: usize = 0x1b4ec;
/// IDLOC range-mode width (`0x6d3c`; 37328). ch0 only (can hold `0x4000`).
pub const OFFSET_1B4F0: usize = 0x1b4f0;
/// IDLOC range-mode minimum (`0x6d3d`; 37330). ch0 only.
pub const OFFSET_1B4F4: usize = 0x1b4f4;
/// IDLOC per-row copy-flag base (`0x1b4f8 + r*4`; `0x6d3e`; 37442/37479). ch1
/// only, one word per row for `r < row_count`.
pub const OFFSET_1B4F8: usize = 0x1b4f8;
/// IDLOC full-copy progress-marker base (`0x1b538 + m*4`; `0x6d4e`; 37520/37548).
/// ch1 only; written as a PREFIX (0 at band start, 1 after the band survives; on
/// the mismatch sentinel the loop breaks leaving a trailing 0 and no later rows).
pub const OFFSET_1B538: usize = 0x1b538;

/// The inclusive-exclusive object byte range this bridge owns in `range_b`:
/// `[0x1b494, 0x1b578)`. The header words `[0x1b484, 0x1b494)` (Slice D init)
/// and the calc surface `[0x1b578, ...)` sit either side.
pub const GAIN_MODE_RANGE_B: std::ops::Range<usize> = 0x1b494..0x1b578;

/// The native 7-point maximum for a gain row (record words 1..8 locations,
/// 8..15 levels). Every captured object across all 154 objects stays within it;
/// a larger count is an untraceable arm.
const GAIN_MAX_POINTS: usize = 7;

// --- IDWL packing-prep (`range_b`) offsets (bridge 1.4) --------------------
//
// Written by the tail of `calc_channel_block_at5` (native `0x51a80`;
// `decompiled/libatrac.c` tail sites 44840-44862 / 45337-45360), per channel,
// gated on shared `+0x88 == 1` at the tail. Given the final selected WLC mode
// `m = block[0x460]`, the 5-word record is `block[0x474 + m*0x14 .. +0x14)` and
// `sel = record[4]` selects the 32-word plane `block[0x4d8 + sel*0x80]`. See
// [`serialize_idwl_object_range_b`].

/// IDWL WLC mode word (`obj[0x1c70c] = block[0x460]`). Written by the level-word
/// path too (aliases `o_1c6f8[5]`); the tail copy re-writes it identically.
pub const OFFSET_1C70C: usize = 0x1c70c;
/// IDWL selector word (`obj[0x1c71c] = record[4]`, `block[0x484 + m*0x14]`).
pub const OFFSET_1C71C: usize = 0x1c71c;
/// IDWL record word 0 (`obj[0x1c720] = record[0]`, `block[0x474 + m*0x14]`).
pub const OFFSET_1C720: usize = 0x1c720;
/// IDWL record word 1 (`obj[0x1c724] = record[1]`, `block[0x478 + m*0x14]`).
pub const OFFSET_1C724: usize = 0x1c724;
/// IDWL record word 2 (`obj[0x1c728] = record[2]`, `block[0x47c + m*0x14]`).
pub const OFFSET_1C728: usize = 0x1c728;
/// IDWL record word 3 (`obj[0x1c72c] = record[3]`, `block[0x480 + m*0x14]`).
pub const OFFSET_1C72C: usize = 0x1c72c;
/// IDWL word-length plane base (`obj[0x1c7f0 + i*4] = block[0x4d8 + sel*0x80]`,
/// 32 words = `word_rows[sel]`).
pub const OFFSET_1C7F0: usize = 0x1c7f0;

// --- ch0-only tone-mode-1 tail (docs/12 §1.3) -----------------------------
//
// After the generic per-channel IDWL copy loop, `calc_channel_block_at5` has a
// ch0-only branch on `obj0[0x1c70c] == 1` copying exactly three window-fields
// scratch words (decompile 44866-44869 / 45363-45366):
//
// ```text
// obj0[0x1c710] = ch0_calc[0x768]   // prefix_count  (pack: 5-bit, gated count != 0)
// obj0[0x1c714] = ch0_calc[0x76c]   // residual_bits (pack: 2-bit field write)
// obj0[0x1c718] = ch0_calc[0x770]   // residual_base (pack: 3-bit)
// ```
//
// `ch0_calc + 0x768` is a SHARED scratch: `second_bit_allocation_at5`
// (decompile 39048) points every channel's calc-block `+0x4d4` at it, and
// `calc_nbits_for_idwl_1_at5` (native `0x1d160`) writes `[start, bits, base]`
// through it UNCONDITIONALLY at the end of every mode-1 costing evaluation
// (last-writer-wins across BOTH channels). The Rust computed path models this
// aliasing with the `shared_side.window_fields` local threaded through the
// costing loop and both `run_fifth_sixth` invocations; native has no WLC
// costing between the section-14 re-run and this tail, so the tail reads the
// FINAL shared value (`CalcFrameOutput::shared_wlc_window_fields`), NOT ch0's
// possibly-stale per-block `idwl_block.side.window_fields`. Native-observed on
// the dance-the-night sweep of record (7 rows, output frames 3792-3803, the
// fade-out tail; docs/12 §1.3).

/// IDWL ch0 tone-mode-1 aux word 0 (`obj0[0x1c710] = ch0_calc[0x768]`); pack
/// leaf `prefix_count` (5-bit).
pub const OFFSET_1C710: usize = 0x1c710;
/// IDWL ch0 tone-mode-1 aux word 1 (`obj0[0x1c714] = ch0_calc[0x76c]`); pack
/// leaf `residual_bits` (2-bit field write).
pub const OFFSET_1C714: usize = 0x1c714;
/// IDWL ch0 tone-mode-1 aux word 2 (`obj0[0x1c718] = ch0_calc[0x770]`); pack
/// leaf `residual_base` (3-bit).
pub const OFFSET_1C718: usize = 0x1c718;

// --- ch0-only tone-mode-2 tail (docs/13 §3.1 slice 2) ---------------------
//
// After the generic per-channel IDWL copy loop and the ch0 mode-1 tail,
// `calc_channel_block_at5` has a ch0-only branch on `obj0[0x1c70c] == 2`
// (decompile 44871-44901 / 45368-45398). Given `selector_b = obj0[0x1c724]`
// (the winning candidate row) it reads the per-row side record
// `rec = ch0_calc_block + selector_b*0x8c` (stride 0x8c == 35*4 ==
// `IDWL_SG_ROW_WORDS_AT5`):
//
// ```text
// obj0[0x1c730] = *(rec + 0x7f4)              // field_4bits  = side.rows[selector_b][32]
// obj0[0x1c734] = *(rec + 0x7f8)              // field_3bits  = side.rows[selector_b][33]
// obj0[0x1c738] = *(ch0_calc_block + 0x9a4)   // subgroup_flag = side.subgroup_flag
// obj0[0x1c870 + i*4] = *(rec + 0x774 + i*4)  // symbol plane  = side.rows[selector_b][i], i in 0..32
// ```
//
// (`+0x9a4 == 0x774 + 4*0x8c`, the word right after the four row records.) The
// shared-cfg group-flag law (`cfg[0xd4]`/`cfg[0xd8..]`) is emitted separately
// by [`serialize_idwl_mode2_cfg_words`]. Native-observed at 192 (sweat output
// frame 5556, block 0, quant_unit_count 29; discovery_sweep_192_run.json).

/// IDWL ch0 tone-mode-2 field_4bits (`obj0[0x1c730] = side.rows[selector_b][32]`).
pub const OFFSET_1C730: usize = 0x1c730;
/// IDWL ch0 tone-mode-2 field_3bits (`obj0[0x1c734] = side.rows[selector_b][33]`).
pub const OFFSET_1C734: usize = 0x1c734;
/// IDWL ch0 tone-mode-2 subgroup flag (`obj0[0x1c738] = side.subgroup_flag`).
pub const OFFSET_1C738: usize = 0x1c738;
/// IDWL ch0 tone-mode-2 symbol plane base (`obj0[0x1c870 + i*4]`, 32 words).
pub const OFFSET_1C870: usize = 0x1c870;

/// Shared-cfg IDWL mode-2 group count word (`cfg[0xd4] = count >> 1`).
pub const CFG_OFFSET_D4: usize = 0xd4;
/// Shared-cfg IDWL mode-2 group-flag plane base (`cfg[0xd8 + g*4]`).
pub const CFG_OFFSET_D8: usize = 0xd8;

/// The native 32-word IDWL plane length (`word_rows` row width, the copy loop's
/// `for (iVar15 = 0x20; ...)`).
const IDWL_PLANE_WORDS: usize = 32;

/// The inclusive-exclusive object byte range the IDWL copy owns in `range_b`:
/// the 5-word record `[0x1c71c, 0x1c730)` + mode word `0x1c70c` + the 32-word
/// plane `[0x1c7f0, 0x1c870)`.
pub const IDWL_PLANE_RANGE_B: std::ops::Range<usize> = 0x1c7f0..0x1c870;

// --- IDSF packing-prep (`range_b`) offsets (bridge 1.4) --------------------
//
// Written by `calc_nbits_for_idsf_ch_at5` (native `0x50e80`; `decompiled/`
// `libatrac.c` write sites 35249-35251, 35257, 35270, 35645-35648,
// 35833-35836; int-indexed from the object base), last invoked from the
// `adjust_scalefactors_at5` epilogue (native `0x55ae0`; decompiled
// 37995-38025). Field <-> `IdsfBlockState` mapping verified against the leaf.
// See [`serialize_idsf_object_range_b`].

/// IDSF dispatch mode word (`param_1[0x71cf]`; `IdsfBlockState::mode`).
pub const OFFSET_1C73C: usize = 0x1c73c;
/// IDSF mode-1 prefix start (`param_1[0x71d0]`; `IdsfBlockState::start`).
pub const OFFSET_1C740: usize = 0x1c740;
/// IDSF mode-1 prefix count / bits (`param_1[0x71d1]`; `IdsfBlockState::count`).
pub const OFFSET_1C744: usize = 0x1c744;
/// IDSF residual base (`param_1[0x71d2]`; `IdsfBlockState::field_0x1c748`).
pub const OFFSET_1C748: usize = 0x1c748;
/// IDSF huffman selector (`param_1[0x71d3]`; `IdsfBlockState::huffman_selector`).
pub const OFFSET_1C74C: usize = 0x1c74c;
/// IDSF mode selector (`param_1[0x71d4]`; `IdsfBlockState::mode_selector`).
pub const OFFSET_1C750: usize = 0x1c750;
/// IDSF codebook selector (`param_1[0x71d5]`; `IdsfBlockState::codebook_selector`).
pub const OFFSET_1C754: usize = 0x1c754;
/// IDSF compact base (`param_1[0x71d6]`; `IdsfBlockState::compact_base`).
pub const OFFSET_1C758: usize = 0x1c758;
/// IDSF shifted-rows plane base (`param_1[i + 0x723c]`; `shifted_rows`, 3 rows
/// of 32 words at stride `0x20`: `0x1c8f0`/`0x1c970`/`0x1c9f0`).
pub const OFFSET_1C8F0: usize = 0x1c8f0;
/// IDSF transformed-row plane base (`param_1[i + 0x729c]`; `transformed`,
/// 32 words at `0x1ca70`).
pub const OFFSET_1CA70: usize = 0x1ca70;

/// The native plane row width (32 words) and the shifted-rows count (3).
const IDSF_PLANE_WORDS: usize = 32;
const IDSF_SHIFTED_ROWS: usize = 3;

/// The header word range the IDSF leaf owns: `[0x1c73c, 0x1c75c)` (8 words).
pub const IDSF_HEADER_RANGE_B: std::ops::Range<usize> = 0x1c73c..0x1c75c;
/// The shifted-rows plane range: `[0x1c8f0, 0x1ca70)` (3 rows * 32 words).
pub const IDSF_SHIFTED_RANGE_B: std::ops::Range<usize> = 0x1c8f0..0x1ca70;
/// The transformed-row plane range: `[0x1ca70, 0x1caf0)` (32 words).
pub const IDSF_TRANSFORMED_RANGE_B: std::ops::Range<usize> = 0x1ca70..0x1caf0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackerBridgeError {
    /// A serialized field's object offset falls outside the target window.
    OffsetOutOfWindow {
        offset: usize,
        window_base: usize,
        window_len: usize,
    },
    /// The native per-band copy count (`cfg[0xb0] & 0x3fffffff`) exceeds the
    /// available `IdctBlockState.flags` — never silently truncate (fail on the
    /// untraceable arm instead of guessing).
    IdctBandCountExceedsFlags { count: usize, flags_len: usize },
    /// A ch1 gain object was serialized without its reference (ch0) rows. The
    /// native branch (`*obj != 0`) reads `*(obj[10] + 8)` for every candidate, so
    /// the previous rows are mandatory — never guess them.
    GainMissingPreviousRows,
    /// A gain row's point count exceeds the native 7-point maximum
    /// (`GAIN_MAX_POINTS`). Untraceable arm: the record's location/level arrays
    /// are only 7 wide, so a larger count is unobserved — fail instead of guess.
    GainRowCountExceedsMax { count: usize, max: usize },
    /// A gain-selection leaf failed (a location/delta out of its table range, or
    /// a reference row too short). Propagated from `zeroth_gain_*_at5`.
    GainSelection(AllocationError),
    /// The final IDWL selected mode is out of range `{0,1,2,3}`. The native tail
    /// indexes a 5-word per-mode record (`block[0x474 + m*0x14]`); a larger mode
    /// would read past the four records — fail on the untraceable arm.
    IdwlModeOutOfRange { mode: u32 },
    /// The IDWL selector (`record[4]`) selects a `word_rows` plane row; a value
    /// outside `0..3` would index past the four rows. Untraceable arm.
    IdwlSelectorOutOfRange { selector: i32 },
    /// The IDSF window was requested for the live path but the computed
    /// `IdsfBlockState` is missing (the epilogue `+0x8c == 0` zero arm did not
    /// produce a block state). Never guess the plane/selector words.
    IdsfBlockStateMissing,
    /// The GHA band count (`arena_root[2]`) exceeds the native 16-band arena
    /// (`MAX_EXTRACT_BANDS_AT5`). A larger count indexes past the flag/row
    /// arrays — fail on the untraceable arm.
    GhaBandCountExceedsMax { band_count: usize, max: usize },
    /// The total wave count across all channels/bands exceeds the native
    /// `0x30` arena limit (`extract_ghwave_wave_limit_at5`). A larger total
    /// overflows the record arena — fail instead of guess.
    GhaWaveTotalExceedsMax { total: usize, max: usize },
    /// The GHA group has an unsupported channel count (only mono/stereo are
    /// evidenced). Never guess a wider layout.
    GhaUnsupportedChannelCount { channel_count: usize },
    /// Header mode 0 selects the IDAM-write arm (selector word `0x1c76c`), a
    /// non-352 profile this bridge does not model. `calc_nbits_for_gha_at5`
    /// would emit `Some(idam_selector)` and the native would write `0x1c76c`;
    /// implementing that write speculatively is prohibited (rule 15). At 352
    /// `arena_root[1] == 1` always, so this arm is dead. Fail explicitly.
    GhaHeaderModeZeroUnsupported,
    /// A channel claims `has_previous` but has no earlier channel to reference
    /// (`ch == 0`). The pinned call-7 model is `ch0.has_previous == false`; a
    /// call contradicting it must STOP, not invent a variant.
    GhaHasPreviousWithoutReference { channel: usize },
    /// `calc_nbits_for_gha_at5` returned `None` for a dispatch selector that the
    /// packer unconditionally reads (IDLOC/NWAVS/FREQ/IDSF). Never write a guess.
    GhaSelectorMissing {
        channel: usize,
        family: &'static str,
    },
    /// The `gainb` window builder was given the wrong record buffer length
    /// (must be `GAINB_ROW_COUNT * 38` u32).
    GainbRecordsLen { expected: usize, actual: usize },
    /// The `gainb` window builder was given the wrong band-activity length (must
    /// be `GAINB_ROW_COUNT` i32).
    GainbActivityLen { expected: usize, actual: usize },
}

impl From<AllocationError> for PackerBridgeError {
    fn from(error: AllocationError) -> Self {
        PackerBridgeError::GainSelection(error)
    }
}

fn put_bytes(
    window: &mut ObjectWindow,
    offset: usize,
    data: &[u8],
) -> Result<(), PackerBridgeError> {
    let rel = offset
        .checked_sub(window.mem_offset)
        .filter(|rel| rel + data.len() <= window.bytes.len())
        .ok_or(PackerBridgeError::OffsetOutOfWindow {
            offset,
            window_base: window.mem_offset,
            window_len: window.bytes.len(),
        })?;
    window.bytes[rel..rel + data.len()].copy_from_slice(data);
    Ok(())
}

fn put_i32_array(
    window: &mut ObjectWindow,
    offset: usize,
    values: &[i32],
) -> Result<(), PackerBridgeError> {
    for (i, value) in values.iter().enumerate() {
        put_bytes(window, offset + i * 4, &value.to_le_bytes())?;
    }
    Ok(())
}

fn put_i16_array(
    window: &mut ObjectWindow,
    offset: usize,
    values: &[i16],
) -> Result<(), PackerBridgeError> {
    for (i, value) in values.iter().enumerate() {
        put_bytes(window, offset + i * 2, &value.to_le_bytes())?;
    }
    Ok(())
}

/// Serialize one channel's calc-decision surface into its `range_b` window,
/// in place. Writes ONLY the `CalcChannelOutput` fields (the contiguous
/// `[0x1b578, 0x1c718)` region); every other byte in the window is left as the
/// caller provided it (captured-CUT — see the module CUT list).
///
/// The written bytes are byte-exact against the captured `frame0_prepacker_state`
/// `range_b` for both channels of native output frame 0 (see
/// `tests/packer_bridge.rs`).
pub fn serialize_calc_object_range_b(
    window: &mut ObjectWindow,
    out: &CalcChannelOutput,
) -> Result<(), PackerBridgeError> {
    // Selectors -> IDCT value (0x1b578 + i*4) and spectral descriptor selector.
    put_i32_array(window, OFFSET_1B578, &out.o_1b578)?;
    // Word lengths -> spectral word length / IDWL(4) current values.
    put_i32_array(window, OFFSET_1B5F8, &out.o_1b5f8)?;
    // Scale factors / idsf -> IDSF(4) current values.
    put_i32_array(window, OFFSET_1B678, &out.o_1b678)?;
    // Quantized i16 plane -> spectral descriptor samples (read via u16).
    put_i16_array(window, OFFSET_1B6F8, &out.o_1b6f8)?;
    // Level words -> IDSPCQU tail; word 5 (0x1c70c) is the IDWL mode / tone flag
    // (`o_1c70c0`), already carried by `o_1c6f8[5]` (native `level_words[5]`).
    put_i32_array(window, OFFSET_1C6F8, &out.o_1c6f8)?;
    Ok(())
}

/// Serialize the calc-decision surface of `range_a`: the single word
/// `mode_1074` at object offset `0x1074` (the packer's IDCT 1-bit field and the
/// spectral descriptor bandwidth word). Byte-exact vs the captured `range_a`.
pub fn serialize_calc_object_range_a(
    window: &mut ObjectWindow,
    out: &CalcChannelOutput,
) -> Result<(), PackerBridgeError> {
    put_bytes(window, OFFSET_1074, &out.mode_1074.to_le_bytes())
}

/// Serialize the block->object IDCT copy into `range_a`: the tail of
/// `calc_channel_block_at5` (native `0x51a80`; `decompiled/libatrac.c`
/// 43813; tail sites 44918-44930 / 45400-45416) copies the block's final IDCT
/// state (`block+0x9f8`) into the packer object at `[0x1078, 0x1104)`.
///
/// Writes `0x1078 = idct.mode`, `0x107c = idct.band_count`, `0x1080 =
/// idct.split_flag`, then `idct.flags[i]` at `0x1084 + i*4` for
/// `i in 0..(config_b0 & 0x3fffffff)`, where `config_b0` is the shared cfg
/// emits). The block's aux words (`block+0xa38..`) are NOT copied. Byte-exact
/// vs the captured `range_a` `[0x1078, 0x1104)` for both channels of native
/// output frame 0 (core call 7) — see `tests/composed_frame.rs`.
pub fn serialize_idct_object_range_a(
    window: &mut ObjectWindow,
    idct: &IdctBlockState,
    config_b0: u32,
) -> Result<(), PackerBridgeError> {
    let count = (config_b0 & IDCT_COUNT_MASK) as usize;
    if count > idct.flags.len() {
        // Untraceable arm: the native loop would read past the parsed flags.
        return Err(PackerBridgeError::IdctBandCountExceedsFlags {
            count,
            flags_len: idct.flags.len(),
        });
    }
    // 3 header words (mode / band count / split flag).
    put_bytes(window, OFFSET_1078, &idct.mode.to_le_bytes())?;
    put_bytes(window, OFFSET_107C, &(idct.band_count as u32).to_le_bytes())?;
    put_bytes(window, OFFSET_1080, &idct.split_flag.to_le_bytes())?;
    // Per-band flags for `cfg[0xb0] & 0x3fffffff` words.
    for i in 0..count {
        put_bytes(window, OFFSET_1084 + i * 4, &idct.flags[i].to_le_bytes())?;
    }
    Ok(())
}

/// One parsed gain-record row from the `*(obj + 8)` gain buffer (stride `0x98`):
/// word `+0x0` = gain-point count `n`, `+0x4 + k*4` = location[k], `+0x20 + k*4`
/// = level[k] for `k < n`. The caller parses exactly `obj[0x1b490]` rows (the
/// native row count). Locations/levels hold at least `count` entries.
#[derive(Debug, Clone)]
pub struct GainModeRow {
    pub count: usize,
    pub locations: Vec<i32>,
    pub levels: Vec<i32>,
}

/// Serialize one object's gain NGC/IDLEV/IDLOC packing-prep window
/// `[0x1b494, 0x1b578)` in place, from the computed gain rows.
///
/// # Native source
///
/// The gain side-data section of `zeroth_bit_allocation_at5` (native `0x52360`;
/// `decompiled/libatrac.c` lines 36782-37574). The whole contribution is gated
/// on `obj[0x1b484] != 0` (`piVar24[0x6d21]`, `active`): when inactive the native
/// writes NOTHING here, so this returns `Ok` leaving the window untouched. The
/// object's channel index (`*obj`) selects the branch: `0` -> ch0, else -> ch1
/// (the native `*piVar24 == 0` test). ch1 requires `prev_rows` (ch0's rows), read
/// via `*(obj[10] + 8)` at the CURRENT channel's row count.
///
/// # Write set (per branch — see the decompile lines)
///
/// * Both: `0x1b494` NGC mode (36901), `0x1b4a0` IDLEV mode (37170), `0x1b4ec`
///   IDLOC mode (37566) — each the argmin (strict `<`, earliest wins) of the four
///   candidate bit costs the leaves score.
/// * ch0 only (written unconditionally while scoring, whichever mode wins):
///   `0x1b498`/`0x1b49c` NGC range width/min (36845/36846), `0x1b4a4`/`0x1b4a8`
///   IDLEV width/min (37023/37025), `0x1b4f0`/`0x1b4f4` IDLOC width/min
///   (37328/37330). A width can be the `0x4000` sentinel — a real native value.
/// * ch1 only: IDLEV per-row copy flags `0x1b4ac + r*4` (37086/37106) and IDLOC
///   per-row copy flags `0x1b4f8 + r*4` (37442/37479) for `r < row_count`; IDLOC
///   full-copy progress markers `0x1b538 + m*4` (37520/37548) as a PREFIX (the
///   leaf's `copy_markers` — a trailing 0 with nothing after it on the mismatch
///   sentinel).
///
/// Byte-exact vs the captured `frame0_prepacker_state` `range_b` `[0x1b494,
/// 0x1b578)` across all 77 core calls / 154 objects — see `tests/composed_frame.rs`.
pub fn serialize_gain_modes_range_b(
    window: &mut ObjectWindow,
    channel_index: u32,
    active: bool,
    rows: &[GainModeRow],
    prev_rows: Option<&[GainModeRow]>,
) -> Result<(), PackerBridgeError> {
    // Native guard: `piVar24[0x6d21] != 0`. Inactive objects write nothing.
    if !active {
        return Ok(());
    }
    for row in rows {
        if row.count > GAIN_MAX_POINTS {
            return Err(PackerBridgeError::GainRowCountExceedsMax {
                count: row.count,
                max: GAIN_MAX_POINTS,
            });
        }
    }

    // Leaf inputs: point counts and the level/location band views.
    let point_counts: Vec<i32> = rows.iter().map(|row| row.count as i32).collect();
    let level_bands: Vec<ZerothGainLevelBand<'_>> = rows
        .iter()
        .map(|row| ZerothGainLevelBand {
            count: row.count,
            levels: &row.levels,
        })
        .collect();
    let location_bands: Vec<ZerothGainLocationBand<'_>> = rows
        .iter()
        .map(|row| ZerothGainLocationBand {
            count: row.count,
            locations: &row.locations,
            levels: &row.levels,
        })
        .collect();

    if channel_index == 0 {
        // ch0 branch: no reference channel; range width/min written alongside.
        let ngc = zeroth_gain_ngc_mode_at5(&point_counts, None)?;
        put_bytes(window, OFFSET_1B494, &(ngc.mode as u32).to_le_bytes())?;
        put_bytes(
            window,
            OFFSET_1B498,
            &ngc.fixed_width.unwrap_or(0).to_le_bytes(),
        )?;
        put_bytes(
            window,
            OFFSET_1B49C,
            &ngc.fixed_min.unwrap_or(0).to_le_bytes(),
        )?;

        let idlev = zeroth_gain_idlev_mode_at5(&level_bands)?;
        put_bytes(window, OFFSET_1B4A0, &(idlev.mode as u32).to_le_bytes())?;
        put_bytes(window, OFFSET_1B4A4, &idlev.fixed_width.to_le_bytes())?;
        put_bytes(window, OFFSET_1B4A8, &idlev.fixed_min.to_le_bytes())?;

        let idloc = zeroth_gain_idloc_mode_at5(&location_bands)?;
        put_bytes(window, OFFSET_1B4EC, &(idloc.mode as u32).to_le_bytes())?;
        put_bytes(window, OFFSET_1B4F0, &idloc.fixed_width.to_le_bytes())?;
        put_bytes(window, OFFSET_1B4F4, &idloc.fixed_min.to_le_bytes())?;
    } else {
        // ch1 branch: scored against ch0's rows; per-row copy flags + markers.
        let prev = prev_rows.ok_or(PackerBridgeError::GainMissingPreviousRows)?;
        let prev_counts: Vec<i32> = prev.iter().map(|row| row.count as i32).collect();
        let prev_level_bands: Vec<ZerothGainLevelBand<'_>> = prev
            .iter()
            .map(|row| ZerothGainLevelBand {
                count: row.count,
                levels: &row.levels,
            })
            .collect();
        let prev_location_bands: Vec<ZerothGainLocationBand<'_>> = prev
            .iter()
            .map(|row| ZerothGainLocationBand {
                count: row.count,
                locations: &row.locations,
                levels: &row.levels,
            })
            .collect();

        let ngc = zeroth_gain_ngc_mode_at5(&point_counts, Some(&prev_counts))?;
        put_bytes(window, OFFSET_1B494, &(ngc.mode as u32).to_le_bytes())?;

        let idlev = zeroth_gain_idlev_mode_ch1_at5(&level_bands, &prev_level_bands)?;
        put_bytes(window, OFFSET_1B4A0, &(idlev.mode as u32).to_le_bytes())?;
        for (r, flag) in idlev.copy_flags.iter().enumerate() {
            put_bytes(window, OFFSET_1B4AC + r * 4, &flag.to_le_bytes())?;
        }

        let idloc = zeroth_gain_idloc_mode_ch1_at5(&location_bands, &prev_location_bands)?;
        put_bytes(window, OFFSET_1B4EC, &(idloc.mode as u32).to_le_bytes())?;
        for (r, flag) in idloc.copy_flags.iter().enumerate() {
            put_bytes(window, OFFSET_1B4F8 + r * 4, &flag.to_le_bytes())?;
        }
        // Full-copy progress markers: PREFIX only (the leaf returns exactly the
        // native-written prefix; trailing stale slots stay as the caller left them).
        for (m, marker) in idloc.copy_markers.iter().enumerate() {
            put_bytes(window, OFFSET_1B538 + m * 4, &marker.to_le_bytes())?;
        }
    }

    Ok(())
}

// ===========================================================================
// docs/11 §2.2 — the two from-scratch prepacker windows the substitution
// harness never built from computation: the init gain-classification header
// (`range_b [0x1b484, 0x1b494)`) and the `gainb` window (`*(obj+8)`, len 0xb00).
// ===========================================================================

/// Init gain-classification header word `0x1b484` (record-present flag).
pub const OFFSET_1B484: usize = 0x1b484;
/// Init gain-classification header word `0x1b488` (count-differs delta flag).
pub const OFFSET_1B488: usize = 0x1b488;
/// Init gain-classification header word `0x1b48c` (last-nonzero prev count).
pub const OFFSET_1B48C: usize = 0x1b48c;
/// Init gain-classification header word `0x1b490` (gain row count).
pub const OFFSET_1B490: usize = 0x1b490;
/// The init gain-classification header write set (`range_b [0x1b484, 0x1b494)`).
/// Word `0x1b480` is packer-unread and left as the caller provided it.
pub const INIT_GAIN_HEADER_RANGE_B: std::ops::Range<usize> = 0x1b484..0x1b494;

/// 38-word (0x98-byte) stride of each `gainb` gain-record row.
pub const GAINB_ROW_STRIDE: usize = 0x98;
/// Number of `gainb` gain-record rows (`[0, 0x980)` = 16 rows × 0x98).
pub const GAINB_ROW_COUNT: usize = 16;
/// `gainb` band-activity summary flag words (`+0x980` any, `+0x984` partial).
pub const OFFSET_GAINB_980: usize = 0x980;
pub const OFFSET_GAINB_984: usize = 0x984;
/// `gainb` per-band activity row base (`+0x988 + k*4`, k < 16).
pub const OFFSET_GAINB_988: usize = 0x988;
/// Total `gainb` window length (`*(obj+8)` window `[0, 0xb00)`).
pub const GAINB_WINDOW_LEN: usize = 0xb00;

/// Serialize the init gain-classification header words into `range_b`
/// `[0x1b484, 0x1b494)` in place (docs/11 §2.2 (a)2). These are read by
/// `pack_gain_block` (`src/bitstream/frame.rs`): `0x1b484` present flag (1 bit),
/// `0x1b490`-1 (4 bit row count), `0x1b488` delta flag (1 bit), and `0x1b48c`-1
/// (4 bit) when the delta flag is set. Word `0x1b480` is packer-unread and left
/// untouched. Every field comes straight from the init run
/// ([`InitGainHeaderWords`], which mirrors `InitChannelOutput.obj_1b484/488/48c/490`
/// — init reproduces them byte-exact, Slice D).
pub fn serialize_init_gain_header_range_b(
    window: &mut ObjectWindow,
    header: &crate::encoder::coding_bridge::InitGainHeaderWords,
) -> Result<(), PackerBridgeError> {
    // 0x1b480 left as-is (packer-unread).
    put_bytes(window, OFFSET_1B484, &header.obj_1b484.to_le_bytes())?;
    put_bytes(window, OFFSET_1B488, &header.obj_1b488.to_le_bytes())?;
    put_bytes(window, OFFSET_1B48C, &header.obj_1b48c.to_le_bytes())?;
    put_bytes(window, OFFSET_1B490, &header.obj_1b490.to_le_bytes())?;
    Ok(())
}

/// Build one object's `gainb` window (`*(obj+8)`, `mem_offset == 0`, length
/// [`GAINB_WINDOW_LEN`]) from computation (docs/11 §2.2 (a)1).
///
/// Layout / native write sources:
/// * `[0, 0x980)` — 16 gain rows at stride 0x98 = the current call's assembled
///   gain-A records (`gain_a_records`, exactly [`assemble_gain_a_records`]'s 16×38
///   words: a 15-word detector point prefix per band + zero tail). The packer
///   reads only the 15-word point prefix per row (`parse_gain_rows`), and words
///   15..37 are packer-unread (pinned by
///   `zeroing_gain_record_float_tails_does_not_change_packed_bytes`), so the
///   caller supplies the 16×38-word buffer with zero tails.
/// * `0x980`/`0x984` — the zeroth band-activity summary flag words (any / partial).
///   Native writer: the zeroth band-activity block (`clear_main_data_at5` twin at
///   native `0x2d090`, decompile 24480-24553): sum of the per-band flags →
///   sum==0 → (0,0); sum==count → (1,0); else (1,1). Computed here via
///   [`zeroth_activity_summary_at5`] over `band_activity`.
/// * `0x988 + k*4` (k < 16) — the per-band activity row = `band_activity`
///   (`zeroth_band_activity_from_frontend`; force-zeroed on the 352 path).
/// * `[0x9c8, 0xb00)` — zero by policy. Native memory holds nonzero scratch there
///   but it is OUTSIDE the packer read set (the only `gainb` reads are
///   `parse_gain_rows` row prefixes and `pack_gain_side_gainb`
///   are zero on ALL 154 objects across ALL 77 captured frames.
///
/// `gain_a_records` must be exactly `GAINB_ROW_COUNT * 38` u32; `band_activity`
/// must be exactly `GAINB_ROW_COUNT` i32.
///
/// `group_count` is the gainb band-activity count (`cfg+0xc8`, the shared
/// band-shape group count) over which the `0x980`/`0x984` any/partial summary is
/// evaluated — native `piVar9[0x32]`, decompile 37580. It equals the count the
/// pack's per-band `0x988` loop consumes (`pack_gain_side_gainb`, `cfg+0xc8`), so
/// the summary and the pack loop always agree. Summing the any/partial decision
/// over the full 16-row window instead misclassifies an all-active
/// `group_count`-band frame (whose `band_activity[group_count..16] == 0`) as
/// PARTIAL, packing `2 + group_count` bits where native packs 2 — the 64/48
/// final-flush over-budget bug (docs/13 §5.2 slice 7). At 352 the group count is
/// 16 (`== GAINB_ROW_COUNT`), so the summary is unchanged there.
pub fn serialize_gainb_window(
    gain_a_records: &[u32],
    band_activity: &[i32],
    group_count: usize,
) -> Result<ObjectWindow, PackerBridgeError> {
    if gain_a_records.len() != GAINB_ROW_COUNT * 38 {
        return Err(PackerBridgeError::GainbRecordsLen {
            expected: GAINB_ROW_COUNT * 38,
            actual: gain_a_records.len(),
        });
    }
    if band_activity.len() != GAINB_ROW_COUNT {
        return Err(PackerBridgeError::GainbActivityLen {
            expected: GAINB_ROW_COUNT,
            actual: band_activity.len(),
        });
    }
    let mut window = ObjectWindow::new(0, vec![0u8; GAINB_WINDOW_LEN]);

    // Gain rows: 16 × 38 words at stride 0x98 = [0, 0x980).
    for row in 0..GAINB_ROW_COUNT {
        let base = row * GAINB_ROW_STRIDE;
        for word in 0..38 {
            let value = gain_a_records[row * 38 + word];
            put_bytes(&mut window, base + word * 4, &value.to_le_bytes())?;
        }
    }

    // Band-activity summary flags (0x980 any / 0x984 partial). Native evaluates
    // these over cfg+0xc8 (`group_count`, `piVar9[0x32]`, decompile 37580), the
    // SAME count the pack per-band 0x988 loop consumes — not the full 16-row
    // window. See the fn doc for the 64/48 all-active misclassification this
    // avoids.
    let summary: ZerothActivitySummary = zeroth_activity_summary_at5(band_activity, group_count)?;
    put_bytes(
        &mut window,
        OFFSET_GAINB_980,
        &summary.any_flag.to_le_bytes(),
    )?;
    put_bytes(
        &mut window,
        OFFSET_GAINB_984,
        &summary.partial_flag.to_le_bytes(),
    )?;

    // Per-band activity row (0x988 + k*4).
    for (k, &flag) in band_activity.iter().enumerate() {
        put_bytes(&mut window, OFFSET_GAINB_988 + k * 4, &flag.to_le_bytes())?;
    }

    // [0x9c8, 0xb00) stays zero (packer-unread residue).
    Ok(window)
}

/// Return the final selected WLC mode's 5-word record from an `IdwlBlockState`.
/// The native tail reads `block[0x474 + mode*0x14 + k*4]` for `k in 0..5`; in
/// the Rust port the four records live in `selector_fields_14_24` (mode 0),
/// `_28_38` (mode 1), `_3c_4c` (mode 2), `_50_60` (mode 3). Array index `k`
/// equals the native record word index (`record[0..4]`).
fn idwl_record(block: &IdwlBlockState) -> Result<[i32; 5], PackerBridgeError> {
    match block.mode {
        0 => Ok(block.selector_fields_14_24),
        1 => Ok(block.selector_fields_28_38),
        2 => Ok(block.selector_fields_3c_4c),
        3 => Ok(block.selector_fields_50_60),
        mode => Err(PackerBridgeError::IdwlModeOutOfRange { mode }),
    }
}

/// Serialize one channel's IDWL packing-prep window into its `range_b` window,
/// in place, from the computed final WLC block state.
///
/// # Native source
///
/// The tail of `calc_channel_block_at5` (native `0x51a80`; `decompiled/`
/// `libatrac.c` tail sites 44840-44862 / 45337-45360), gated on shared
/// `+0x88 == 1`. When the copy ran (`copy_ran`), for the final WLC mode
/// `m = block.mode` it copies the 5-word per-mode record
/// (`block[0x474 + m*0x14]`) and the 32-word `word_rows[record[4]]` plane:
///
/// ```text
/// obj[0x1c70c] = m               // WLC mode (also written by the level path)
/// obj[0x1c71c] = record[4]       // selector (sel)
/// obj[0x1c720] = record[0]
/// obj[0x1c724] = record[1]
/// obj[0x1c728] = record[2]
/// obj[0x1c72c] = record[3]
/// obj[0x1c7f0 + i*4] = word_rows[sel][i]   for i in 0..32
/// ```
///
/// `copy_ran` reflects the native tail gate `(*(ctx+0x1dc) & 0x7c) == 0 &&
/// shared+0x88 == 1` (decompile 44830/44839/44914). In the composed encode path
/// this is unconditionally true — the copy runs on EVERY core call, including
/// the call-0 priming call where `config_b0 == 0` (superseding the earlier
/// [3,0,0,0,0,0,1,0,..], i.e. the empty-shape record WAS written natively at
/// b0 == 0). The earlier call-0 byte mismatch traced to ch1's `obj[0x1c720]`
/// slot holding native uninitialized-stack garbage (-134763296) at word-count 0,
/// which never reaches the bitstream because pack leaves read the huffman
/// selector only when the count word (`obj[0x1c728]`) is non-zero.
/// The `copy_ran` parameter is retained for unit tests, which drive both values
/// directly to exercise the write/skip branch; when false this returns `Ok`
/// writing nothing (the caller keeps the captured bytes).
///
/// # ch0 tone-mode-1 tail (docs/12 §1.3, native-observed)
///
/// When `copy_ran && channel_index == 0 && block.mode == 1` the native tail
/// takes the obj0-only mode-1 branch (decompile 44866-44869 / 45363-45366)
/// after the generic copy, writing three window-fields scratch words:
///
/// ```text
/// obj0[0x1c710] = ch0_calc[0x768]   // prefix_count
/// obj0[0x1c714] = ch0_calc[0x76c]   // residual_bits
/// obj0[0x1c718] = ch0_calc[0x770]   // residual_base
/// ```
///
/// `ch0_calc + 0x768` is the SHARED window-fields scratch aliased across both
/// channels' mode-1 costing (see the [`OFFSET_1C710`] block comment). The tail
/// therefore reads the FINAL shared value, passed in as `shared_window_fields`
/// (`CalcFrameOutput::shared_wlc_window_fields`), NOT ch0's per-block
/// `block.side.window_fields` (which may be stale if ch1's mode-1 costing ran
/// later). Native-observed on the dance-the-night sweep of record (7 rows,
/// output frames 3792-3803, the fade-out tail).
///
/// # ch0 tone-mode-2 tail (docs/13 §3.1 slice 2, native-observed at 192)
///
/// When `copy_ran && channel_index == 0 && block.mode == 2` the native tail
/// takes the obj0-only mode-2 branch (decompile 44871-44901 / 45368-45398)
/// after the generic copy, writing the side-record symbol plane at `0x1c870`
/// (32 words) plus the two packed field words `0x1c730`/`0x1c734` and the
/// subgroup flag `0x1c738`, all sourced from `block.side.rows[selector_b]` /
/// `block.side.subgroup_flag` (`selector_b = block.selector_fields_3c_4c[1]`,
/// guarded to `0..4`). The shared-cfg group-flag law (`cfg[0xd4]`/`cfg[0xd8..]`)
/// is emitted separately by [`serialize_idwl_mode2_cfg_words`] (the cfg window
/// this serializer does not own). Native-observed at 192 (sweat output frame
/// discovery_sweep_192_run.json). At 352/320/256 ch0 mode 2 is never selected
/// (the dance/discovery ch0 histograms never show 2), so this arm is dead there
/// and the packed byte output is unchanged.
///
/// Byte-exact vs the captured `range_b` for both channels of native output
/// frame 0 (core call 7) over the modeled write set — see
/// `tests/composed_frame.rs`.
pub fn serialize_idwl_object_range_b(
    window: &mut ObjectWindow,
    channel_index: u32,
    block: &IdwlBlockState,
    copy_ran: bool,
    shared_window_fields: &[i32; 3],
) -> Result<(), PackerBridgeError> {
    if !copy_ran {
        // Priming call: the native tail copy did not run; leave the residue.
        return Ok(());
    }
    let mode = block.mode;
    let record = idwl_record(block)?;
    let sel = record[4];
    if !(0..IDWL_ROW_COUNT_I32).contains(&sel) {
        return Err(PackerBridgeError::IdwlSelectorOutOfRange { selector: sel });
    }

    // Mode word (also laid down by the level-word path; re-written identically).
    put_bytes(window, OFFSET_1C70C, &mode.to_le_bytes())?;
    // 5-word record: sel then record[0..3].
    put_bytes(window, OFFSET_1C71C, &record[4].to_le_bytes())?;
    put_bytes(window, OFFSET_1C720, &record[0].to_le_bytes())?;
    put_bytes(window, OFFSET_1C724, &record[1].to_le_bytes())?;
    put_bytes(window, OFFSET_1C728, &record[2].to_le_bytes())?;
    put_bytes(window, OFFSET_1C72C, &record[3].to_le_bytes())?;
    // 32-word plane: word_rows[sel].
    let plane = &block.word_rows[sel as usize];
    put_i32_array(window, OFFSET_1C7F0, &plane[..IDWL_PLANE_WORDS])?;

    // ch0-only tone-mode-1 tail: copy the SHARED window-fields triple (docs/12
    // §1.3; decompile 44866-44869 / 45363-45366). Gated exactly like native:
    // the generic copy ran, this is ch0, and the mode word is 1.
    if channel_index == 0 && mode == 1 {
        put_bytes(window, OFFSET_1C710, &shared_window_fields[0].to_le_bytes())?;
        put_bytes(window, OFFSET_1C714, &shared_window_fields[1].to_le_bytes())?;
        put_bytes(window, OFFSET_1C718, &shared_window_fields[2].to_le_bytes())?;
    }

    // ch0-only tone-mode-2 tail: copy the side-record symbol plane + the two
    // packed field words + the subgroup flag (docs/13 §3.1 slice 2; decompile
    // 44871-44901 / 45368-45398). Native-observed at 192 (sweat output frame
    // 5556). `selector_b = record[1]` picks the side row.
    if channel_index == 0 && mode == 2 {
        let selector_b = block.selector_fields_3c_4c[1];
        if !(0..IDWL_ROW_COUNT_I32).contains(&selector_b) {
            return Err(PackerBridgeError::IdwlSelectorOutOfRange {
                selector: selector_b,
            });
        }
        let row = &block.side.rows[selector_b as usize];
        put_bytes(window, OFFSET_1C730, &row[32].to_le_bytes())?;
        put_bytes(window, OFFSET_1C734, &row[33].to_le_bytes())?;
        put_bytes(
            window,
            OFFSET_1C738,
            &block.side.subgroup_flag.to_le_bytes(),
        )?;
        put_i32_array(window, OFFSET_1C870, &row[..IDWL_PLANE_WORDS])?;
    }
    Ok(())
}

/// Emit the shared-cfg IDWL mode-2 group-flag words for a ch0 tone-mode-2 block
/// (docs/13 §3.1 slice 2). The native `calc_channel_block_at5` mode-2 tail
/// (decompile 44871-44901 / 45368-45398) writes, into the SHARED cfg window
/// (`*(obj+4)`):
///
/// ```text
/// cfg[0xd4] = obj0[0x1c728] >> 1              // group count = count >> 1
/// for g in 0..cfg[0xd4]:
///     cfg[0xd8 + g*4] = 1
///     for i in g*2 .. g*2+2:
///         if symbol_plane[i] != 0 { cfg[0xd8 + g*4] = 0; break }
/// ```
///
/// where `count = obj0[0x1c728] = block.selector_fields_3c_4c[2]` and the
/// symbol plane `obj0[0x1c870 + i*4]` is `block.side.rows[selector_b][i]` with
/// `selector_b = block.selector_fields_3c_4c[1]` — the SAME row the packing-prep
/// tail lays down, so the group flags derive from it directly. `count >> 1` uses
/// native i32 arithmetic-shift semantics; counts are `>= 0` in practice.
///
/// This runs ONCE per block on the shared cfg (both channels' cfg clones must
/// see it), so the caller invokes it before cloning the shared window
/// (`src/encoder/computed_frame.rs`).
pub fn serialize_idwl_mode2_cfg_words(
    cfg: &mut ObjectWindow,
    block: &IdwlBlockState,
) -> Result<(), PackerBridgeError> {
    let selector_b = block.selector_fields_3c_4c[1];
    if !(0..IDWL_ROW_COUNT_I32).contains(&selector_b) {
        return Err(PackerBridgeError::IdwlSelectorOutOfRange {
            selector: selector_b,
        });
    }
    let count = block.selector_fields_3c_4c[2];
    let group_count = count >> 1; // native i32 arithmetic shift.
    let row = &block.side.rows[selector_b as usize];
    put_bytes(cfg, CFG_OFFSET_D4, &group_count.to_le_bytes())?;
    for g in 0..group_count.max(0) as usize {
        let mut flag: i32 = 1;
        for i in (g * 2)..(g * 2 + 2) {
            if row[i] != 0 {
                flag = 0;
                break;
            }
        }
        put_bytes(cfg, CFG_OFFSET_D8 + g * 4, &flag.to_le_bytes())?;
    }
    Ok(())
}

/// Serialize one channel's IDSF packing-prep window into its `range_b` window,
/// in place, from the computed final IDSF block state.
///
/// # Native source
///
/// `calc_nbits_for_idsf_ch_at5` (native `0x50e80`; `decompiled/libatrac.c`
/// write sites 35249-35251, 35257, 35270, 35645-35648, 35833-35836), last
/// invoked from the `adjust_scalefactors_at5` epilogue (native `0x55ae0`;
/// decompiled 37995-38025). The leaf writes the 8 header words and the two
/// planes; the field <-> object-offset mapping (verified against the Rust
/// leaf + decompile):
///
/// ```text
/// obj[0x1c73c] = mode              obj[0x1c74c] = huffman_selector
/// obj[0x1c740] = start             obj[0x1c750] = mode_selector
/// obj[0x1c744] = count             obj[0x1c754] = codebook_selector
/// obj[0x1c748] = field_0x1c748     obj[0x1c758] = compact_base
/// obj[0x1c8f0 + r*0x80 + i*4] = shifted_rows[r][i]   r in 0..3, i in 0..32
/// obj[0x1ca70 + i*4]          = transformed[i]       i in 0..32
/// ```
///
/// # Gate
///
/// When `state == None` the epilogue took the `+0x8c == 0` zero arm (native
/// 38004-38010): only `obj[0x1c73c]=0`, `obj[0x1c74c]=0`, and (ch0)
/// `obj[0x1c750]=0` are written. The captured calls all have `+0x8c == 1`, so
/// the live tests exercise the `Some` path.
///
/// Byte-exact vs the captured `range_b` for both channels of native output
/// frame 0 (core call 7) over the modeled write set — see
/// `tests/composed_frame.rs`.
pub fn serialize_idsf_object_range_b(
    window: &mut ObjectWindow,
    channel_index: u32,
    state: Option<&IdsfBlockState>,
) -> Result<(), PackerBridgeError> {
    let Some(block) = state else {
        // `+0x8c == 0` zero arm: only mode / huffman / (ch0) mode-selector = 0.
        put_bytes(window, OFFSET_1C73C, &0u32.to_le_bytes())?;
        put_bytes(window, OFFSET_1C74C, &0u32.to_le_bytes())?;
        if channel_index == 0 {
            put_bytes(window, OFFSET_1C750, &0u32.to_le_bytes())?;
        }
        return Ok(());
    };

    // 8 header words.
    put_bytes(window, OFFSET_1C73C, &block.mode.to_le_bytes())?;
    put_bytes(window, OFFSET_1C740, &(block.start as i32).to_le_bytes())?;
    put_bytes(window, OFFSET_1C744, &(block.count as i32).to_le_bytes())?;
    put_bytes(window, OFFSET_1C748, &block.field_0x1c748.to_le_bytes())?;
    put_bytes(
        window,
        OFFSET_1C74C,
        &(block.huffman_selector as i32).to_le_bytes(),
    )?;
    put_bytes(
        window,
        OFFSET_1C750,
        &(block.mode_selector as i32).to_le_bytes(),
    )?;
    put_bytes(
        window,
        OFFSET_1C754,
        &(block.codebook_selector as i32).to_le_bytes(),
    )?;
    put_bytes(window, OFFSET_1C758, &block.compact_base.to_le_bytes())?;
    // shifted_rows plane: 3 rows of 32 words, stride 0x80.
    for (r, row) in block
        .shifted_rows
        .iter()
        .take(IDSF_SHIFTED_ROWS)
        .enumerate()
    {
        put_i32_array(window, OFFSET_1C8F0 + r * 0x80, &row[..IDSF_PLANE_WORDS])?;
    }
    // transformed row plane: 32 words.
    put_i32_array(window, OFFSET_1CA70, &block.transformed[..IDSF_PLANE_WORDS])?;
    Ok(())
}

/// The native `word_rows` row count (4); `record[4]` selects one of these rows.
const IDWL_ROW_COUNT_I32: i32 = 4;

// --- GHA packing-prep + arena serializer (bridge 1.5) ----------------------
//
// The packer (`pack_gha_header`/`pack_gha_channel`, `src/bitstream/frame.rs`)
// reads a per-group GHA surface from three window families:
//
// * the shared header block (`gha_arena`, `*(*(obj+0x14))`): word 0 header
//   active, word 1 header mode, word 2 `nbands`, the wave-record arena at
//   `+0xc` (one `0x10`-byte record per wave), and three per-band flag families
//   as `(gate, subgate, per-band base)` word triples — shared `(0xc4,0xc5,0xc6)`,
//   swap `(0xe8,0xe9,0xea)`, stereo/opposite `(0xd6,0xd7,0xd8)`;
// * the per-channel `gha_p1` window (`*(obj+0x14)`): word 0 arena back-pointer,
//   then rows at byte `+4` stride `0x28` (10 words each), row word 9 the
//   ABSOLUTE record pointer `= header + 0xc + 0x10*cumulative`;
// * the per-channel `range_b` dispatch selectors `0x1c75c/0x1c760/0x1c764/`
//   `0x1c768` (IDLOC/NWAVS/FREQ/IDSF), the per-band FREQ reverse words
//   `0x1c770+r*4`, the per-band active flags `0x1c7b0+r*4`, and the shared cfg
//   IDSF predictor map `*(obj+4)+0x11c`.
//
// This bridge COMPUTES the five dispatch selectors, per-band FREQ reverse
// words, per-band active flags, the cfg predictor map, and the three
// `[gate,subgate]` header summary pairs + swap flags by running the ported
// `calc_nbits_for_gha_at5` (`src/gha/bitcount.rs`, native `0x0000ff40`,
// `param_3 == 1` production path) over the arena state, and SERIALIZES the
// whole surface into the byte windows the packer reads. Native decision
// evidence: `decompiled/libatrac.c` lines 6865-6965 (swap flags + the three
// summary sections) and 7770 (`active = shared_flag == 0` for a has-previous
// channel).
//
// CONTENT PROVENANCE (documented cut, owned by Phase 2.1): the arena ROW/RECORD
// content the packer reads at call 7 is ring-delayed frontend output (the
// packer reads `*(obj+0x14)`, an arena from an earlier core call). This bridge
// does NOT own that rotation: tests parse the captured packer-domain arena into
// Rust structures, then compute/serialize from those. What is genuinely
// COMPUTED here is the decision surface (selectors/reverse/active/map/summaries/
// swap) plus the full byte-layout serialization.

/// The native GHA arena band cap (`MAX_EXTRACT_BANDS_AT5`).
const GHA_MAX_BANDS: usize = 16;
/// The native GHA total-wave arena cap (`extract_ghwave_wave_limit_at5`, `0x30`).
const GHA_MAX_WAVE_TOTAL: usize = 0x30;
/// Header-block word index of `header active` (`arena_root[0]`).
const GHA_HDR_WORD_ACTIVE: usize = 0;
/// Header-block word index of `header mode` (`arena_root[1]`).
const GHA_HDR_WORD_MODE: usize = 1;
/// Header-block word index of `nbands` (`arena_root[2]`).
const GHA_HDR_WORD_NBANDS: usize = 2;
/// Header-block byte offset of the wave-record arena (`+0xc`).
const GHA_HDR_RECORD_ARENA: usize = 0xc;
/// Native record stride (`0x10`, four words `[idsf, idam, phase, freq]`).
const GHA_HDR_RECORD_STRIDE: usize = 0x10;
/// Shared-flag family header words `(gate, subgate, per-band base)`.
const GHA_HDR_SHARED: (usize, usize, usize) = (0xc4, 0xc5, 0xc6);
/// Swap-flag family header words `(gate, subgate, per-band base)`.
const GHA_HDR_SWAP: (usize, usize, usize) = (0xe8, 0xe9, 0xea);
/// Stereo/opposite-flag family header words `(gate, subgate, per-band base)`.
const GHA_HDR_STEREO: (usize, usize, usize) = (0xd6, 0xd7, 0xd8);
/// Per-channel `gha_p1` row stride (`0x28`, 10 words) and rows base byte (`+4`).
const GHA_P1_ROW_STRIDE: usize = 0x28;
const GHA_P1_ROWS_BASE: usize = 4;

/// GHA dispatch selector word `0x1c75c` (IDLOC mode; ch1 writes a 1-bit gate).
pub const OFFSET_GHA_IDLOC_MODE: usize = 0x1c75c;
/// GHA dispatch selector word `0x1c760` (NWAVS mode, `&3`).
pub const OFFSET_GHA_NWAVS_MODE: usize = 0x1c760;
/// GHA dispatch selector word `0x1c764` (FREQ mode, `&1`).
pub const OFFSET_GHA_FREQ_MODE: usize = 0x1c764;
/// GHA dispatch selector word `0x1c768` (IDSF mode, `&3`).
pub const OFFSET_GHA_IDSF_MODE: usize = 0x1c768;
/// GHA dispatch selector word `0x1c76c` (IDAM mode). Written ONLY in header
/// mode 0 (never at 352 — see [`PackerBridgeError::GhaHeaderModeZeroUnsupported`]).
pub const OFFSET_GHA_IDAM_MODE: usize = 0x1c76c;
/// GHA per-band FREQ reverse-mode base (`0x1c770 + band*4`, read when FREQ 0).
pub const OFFSET_GHA_FREQMODE_BASE: usize = 0x1c770;
/// GHA per-band active-flag base (`0x1c7b0 + band*4`).
pub const OFFSET_GHA_ACTIVE_BASE: usize = 0x1c7b0;
/// Shared cfg IDSF predictor-map base (`*(obj+4)+0x11c`, i32 entries).
pub const OFFSET_GHA_CFG_MAP: usize = 0x11c;

/// One GHA arena row: the native 10-word row (`gha_p1` row words 0..9) paired
/// with the wave records it points at (`GhaWaveRecord`, 1:1 with the packer's
/// `GhaWave`). Row word layout: `[0]` start-window flag, `[1]` end-window flag,
/// `[2]` window start, `[3]` window end, `[4..8]` the four IDLOC gain-window
/// words (`window_words`), `[8]` `nwavs`, `[9]` the absolute record pointer
/// (recomputed by the serializer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaArenaRow {
    pub words: [u32; 10],
    pub records: Vec<GhaWaveRecord>,
}

/// The computed per-channel GHA dispatch surface (one per block object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaChannelSelectors {
    /// `0x1c75c` IDLOC mode.
    pub idloc: u32,
    /// `0x1c760` NWAVS mode.
    pub nwavs: u32,
    /// `0x1c764` FREQ mode.
    pub freq: u32,
    /// `0x1c768` IDSF mode.
    pub idsf: u32,
    /// `0x1c76c` IDAM mode — `None` at 352 (header mode 1); word untouched.
    pub idam: Option<u32>,
    /// Per-band FREQ reverse words (`0x1c770 + band*4`). `None` = untouched
    /// (row had < 2 frequencies), leaving the prior/stale value in place.
    pub freq_modes: Vec<Option<bool>>,
    /// Per-band active flags (`0x1c7b0 + band*4`).
    pub active_flags: Vec<bool>,
    /// IDSF predictor map for the shared cfg (`0x11c`, i32 prefix). Empty for a
    /// no-previous channel.
    pub compact_map: Vec<i32>,
}

/// The three header `[gate, subgate]` summary pairs (any / mixed) written by
/// `calc_nbits_for_gha_at5` (decompile 6885-6965).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhaHeaderSummaries {
    /// `(0xc4, 0xc5)` shared-flag summary.
    pub shared: (bool, bool),
    /// `(0xd6, 0xd7)` stereo/opposite-flag summary.
    pub stereo: (bool, bool),
    /// `(0xe8, 0xe9)` swap-flag summary.
    pub swap: (bool, bool),
}

/// The full computed GHA packing-prep decision surface for one block group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaPackingPrep {
    /// Per-band swap flags (`0xea + band`), computed by the stereo swap plan.
    pub swap_flags: Vec<bool>,
    /// The post-swap arena rows the serializer emits (per channel).
    pub post_swap_channels: Vec<Vec<GhaArenaRow>>,
    /// The per-channel dispatch surface (empty when the header is inactive).
    pub channels: Vec<GhaChannelSelectors>,
    /// The three header summary pairs.
    pub summaries: GhaHeaderSummaries,
    /// The `calc_nbits_for_gha_at5` total bit estimate.
    pub total_bits: usize,
}

/// Apply the planned per-band stereo swaps to a two-channel arena. Swaps the
/// full [`GhaArenaRow`] (row words AND records travel together — the native
/// swap moves the 10-word row whose word 9 is the record pointer, so the
/// pointed-at records move with it). Mono / mismatched channel counts no-op.
fn apply_gha_row_swaps(channels: &mut [Vec<GhaArenaRow>], swap_flags: &[bool]) {
    if channels.len() != 2 {
        return;
    }
    let (first, second) = channels.split_at_mut(1);
    for (band, &swap) in swap_flags.iter().enumerate() {
        if swap && band < first[0].len() && band < second[0].len() {
            std::mem::swap(&mut first[0][band], &mut second[0][band]);
        }
    }
}

/// Band-major, channel-minor record-arena placement. Returns, per
/// `[channel][band]`, the byte offset within the header block where that row's
/// records live (`0xc + 0x10*cumulative`). A shared band (`shared_flags[band]`)
/// makes channel > 0 REUSE channel 0's slot (native: the sine/general
/// dispatch copies the full row to channel 1 with the same record pointer),
/// consuming no new cumulative slot. Replicates the
/// `extract_ghwave_record_pointer_plan_at5` arithmetic across the group.
pub fn gha_record_slot_offsets(
    channels: &[Vec<GhaArenaRow>],
    band_count: usize,
    shared_flags: &[bool],
) -> Vec<Vec<usize>> {
    let channel_count = channels.len();
    let mut offsets = vec![vec![0usize; band_count]; channel_count];
    let mut cumulative = 0usize;
    for band in 0..band_count {
        for channel in 0..channel_count {
            if channel > 0 && shared_flags.get(band).copied().unwrap_or(false) {
                offsets[channel][band] = offsets[0][band];
            } else {
                offsets[channel][band] = GHA_HDR_RECORD_ARENA + cumulative * GHA_HDR_RECORD_STRIDE;
                cumulative += channels[channel]
                    .get(band)
                    .map(|row| row.records.len())
                    .unwrap_or(0);
            }
        }
    }
    offsets
}

/// Compute the GHA packing-prep decision surface for one block group.
///
/// Runs the pinned production pipeline: stereo swap plan
/// ([`calc_nbits_gha_swap_plan_at5`]) -> apply -> [`calc_nbits_for_gha_at5`]
/// (native `0x0000ff40`, `param_3 == 1`), then packages the write sets the
/// packer reads.
///
/// The header-inactive gate runs FIRST, before the swap plan, matching native
/// order: `calc_nbits_for_gha_at5` (decompile 6811-6815) checks arena ring slot
/// 0 as its first statement and `return 1` before touching the swap plan or any
/// dispatch surface. Under inactive (the 64/48 rates, `+0xd0 == 0`) the arena is
/// returned UN-swapped with an empty per-channel dispatch surface and a 1-bit
/// section cost.
///
/// `channels` are the PRE-swap arena rows (the test un-swaps the captured
/// post-swap arena using the captured `0xea` flags before calling, so the plan
/// re-derives them). `has_previous[ch]` follows the pinned model: `ch0 == false`
/// (its `prev_obj_ptr == obj_ptr`), `ch1 == true` (`prev_obj_ptr == obj0`); a
/// has-previous channel uses the PREVIOUS channel's post-swap rows/records as
/// its reference. `shared_flags`/`stereo_flags` are the per-band header flags
/// (`0xc6..` / `0xd8..`, sourced from the frontend extract share gates — a
/// documented content input, not computed here).
///
/// # Errors
///
/// Typed, never-truncating failures: `band_count > 16`
/// ([`PackerBridgeError::GhaBandCountExceedsMax`]); wave total `> 0x30`
/// ([`PackerBridgeError::GhaWaveTotalExceedsMax`]); channel count outside 1..=2
/// ([`PackerBridgeError::GhaUnsupportedChannelCount`]); header mode 0's
/// IDAM-write arm ([`PackerBridgeError::GhaHeaderModeZeroUnsupported`] — dead at
/// 352, never modeled speculatively); a has-previous `ch0`
/// ([`PackerBridgeError::GhaHasPreviousWithoutReference`]).
pub fn compute_gha_packing_prep(
    header_active: bool,
    header_mode: usize,
    band_count: usize,
    channels: &[Vec<GhaArenaRow>],
    has_previous: &[bool],
    shared_flags: &[bool],
    stereo_flags: &[bool],
) -> Result<GhaPackingPrep, PackerBridgeError> {
    let channel_count = channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(PackerBridgeError::GhaUnsupportedChannelCount { channel_count });
    }
    if band_count > GHA_MAX_BANDS {
        return Err(PackerBridgeError::GhaBandCountExceedsMax {
            band_count,
            max: GHA_MAX_BANDS,
        });
    }
    // Native's 0x30 wave-limit count (decompile 42650–42675): channel 0
    // counts every band's wave count; a channel > 0 row is counted ONLY
    // when its per-band shared flag (header `+0xc6` array) is 0, because a
    // shared band aliases channel 0's arena slot and consumes no new
    // records (see `gha_record_slot_offsets` directly above). Summing both
    // channels unconditionally double-counts shared bands.
    let mut total = 0usize;
    for (channel, rows) in channels.iter().enumerate() {
        for (band, row) in rows.iter().enumerate() {
            if channel > 0 && shared_flags.get(band).copied().unwrap_or(false) {
                continue;
            }
            total += row.records.len();
        }
    }
    if total > GHA_MAX_WAVE_TOTAL {
        return Err(PackerBridgeError::GhaWaveTotalExceedsMax {
            total,
            max: GHA_MAX_WAVE_TOTAL,
        });
    }
    if header_active && header_mode == 0 {
        // IDAM-write arm (`0x1c76c`); non-352 only. Never modeled speculatively.
        return Err(PackerBridgeError::GhaHeaderModeZeroUnsupported);
    }
    for (channel, &previous) in has_previous.iter().enumerate().take(channel_count) {
        if previous && channel == 0 {
            return Err(PackerBridgeError::GhaHasPreviousWithoutReference { channel });
        }
    }

    if !header_active {
        // Header-inactive early-out. `calc_nbits_for_gha_at5` (native 0x1ff40,
        // decompile 6811-6815) checks `*piVar9 == 0` (arena ring slot 0) as its
        // FIRST statement and `return 1` — BEFORE the stereo swap plan (6826+),
        // any selector dispatch, or any cfg/object write. So under inactive the
        // arena stays UN-swapped (native never swaps an inactive arena) and the
        // GHA section costs exactly 1 bit. This early-out must precede the swap
        // plan to match that native order: `swap_flags` all-false, the post rows
        // are the pre-swap input untouched, no dispatch surface. (Never at 352 —
        // `active == 1`.)
        return Ok(GhaPackingPrep {
            swap_flags: vec![false; band_count],
            post_swap_channels: channels.to_vec(),
            channels: Vec::new(),
            summaries: GhaHeaderSummaries {
                shared: (false, false),
                stereo: (false, false),
                swap: (false, false),
            },
            total_bits: 1,
        });
    }

    // Stereo swap plan over the PRE-swap `nwavs` (row word 8), then apply.
    let swap_flags = if channel_count == 2 {
        let rows0: Vec<GhaNbitsRow> = channels[0]
            .iter()
            .map(|row| GhaNbitsRow {
                nwavs: row.words[8] as usize,
            })
            .collect();
        let rows1: Vec<GhaNbitsRow> = channels[1]
            .iter()
            .map(|row| GhaNbitsRow {
                nwavs: row.words[8] as usize,
            })
            .collect();
        calc_nbits_gha_swap_plan_at5(&[&rows0, &rows1], band_count).swap_flags
    } else {
        vec![false; band_count]
    };

    let mut post: Vec<Vec<GhaArenaRow>> = channels.to_vec();
    apply_gha_row_swaps(&mut post, &swap_flags);

    // Selector-row storage (borrowed by the `calc_nbits` channels below).
    let selector_rows: Vec<Vec<GhaNbitsSelectorRow<'_>>> = post
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|row| GhaNbitsSelectorRow {
                    window_words: [row.words[4], row.words[5], row.words[6], row.words[7]],
                    nwavs: row.words[8] as usize,
                    records: &row.records,
                })
                .collect()
        })
        .collect();
    let selector_channels: Vec<GhaNbitsSelectorChannel<'_>> = (0..channel_count)
        .map(|channel| {
            let previous = has_previous.get(channel).copied().unwrap_or(false);
            let previous_rows: &[GhaNbitsSelectorRow<'_>] = if previous {
                &selector_rows[channel - 1]
            } else {
                &[]
            };
            GhaNbitsSelectorChannel {
                has_previous: previous,
                rows: &selector_rows[channel],
                previous_rows,
            }
        })
        .collect();

    let result = calc_nbits_for_gha_at5(
        header_active,
        header_mode,
        band_count,
        &selector_channels,
        shared_flags,
        stereo_flags,
        &swap_flags,
    );

    let mut per_channel = Vec::with_capacity(channel_count);
    for channel in 0..channel_count {
        let selectors = &result.selectors[channel];
        let require = |value: Option<u32>, family: &'static str| {
            value.ok_or(PackerBridgeError::GhaSelectorMissing { channel, family })
        };
        per_channel.push(GhaChannelSelectors {
            idloc: require(selectors[0], "idloc")?,
            nwavs: require(selectors[1], "nwavs")?,
            freq: require(selectors[2], "freq")?,
            idsf: require(selectors[3], "idsf")?,
            idam: selectors[4],
            freq_modes: result.reverse_modes[channel].clone(),
            active_flags: result.active_flags[channel].clone(),
            compact_map: result.compact_maps[channel].clone(),
        });
    }

    let summarize = |flags: &[bool]| {
        let summary = calc_nbits_gha_flag_summary_at5(flags, band_count);
        (summary.any, summary.mixed)
    };
    let summaries = GhaHeaderSummaries {
        shared: summarize(shared_flags),
        stereo: summarize(stereo_flags),
        swap: summarize(&swap_flags),
    };

    Ok(GhaPackingPrep {
        swap_flags,
        post_swap_channels: post,
        channels: per_channel,
        summaries,
        total_bits: result.total_bits,
    })
}

/// The pinned `has_previous` model for the two-block 352 group (docs/11 §2.1
/// slice 2.1c, E6): ch0 has no earlier channel to reference
/// (`prev_obj_ptr == obj_ptr`), ch1 references ch0 (`prev_obj_ptr == obj0`).
/// Pinned by bridge 1.5's all-77-frame sweep; `compute_gha_packing_prep` errors
/// on a has-previous ch0.
pub const GHA_HAS_PREVIOUS_352: [bool; 2] = [false, true];

/// Build the GHA packing-prep decision surface for the current core call
/// directly from the rolling [`FrontendState`], with no captured arena input.
///
/// The packer reads ring slot 0 (`*(obj+0x14)`,
/// [`FrontendState::packer_arena`]), which — after the ring's `rotate_left(1)`
/// per core call — holds the extract output of four calls earlier (E7). That
/// slot's rows/records are the PRE-swap extract output (extract does not swap;
/// the swap is a pack-time plan `compute_gha_packing_prep` re-derives), so they
/// feed the prep directly — unlike the test-side `parse_gha_group` path, which
/// must UN-swap captured POST-swap state first (slice 2.1c).
///
/// Inputs assembled from ring slot 0:
/// - per channel `Vec<GhaArenaRow>` zipping `packer_arena(ch).rows` +
///   `.records`;
/// - `header_active`/`header_mode`/`header_band_count` from channel 0's arena
///   (extract writes the header to ch0's output root, E1/E2);
/// - `shared`/`opposite` per-band flags from channel 0's arena (the share gates
///   are written into the same channel-0 output root, E1);
/// - `has_previous = [`[`GHA_HAS_PREVIOUS_352`]`]` (E6).
///
/// The native swap application mutates the packer arena in place at call `N`,
/// but that slot is overwritten by fresh extract output at call `N+1`, so this
/// copy-based prep (which never mutates the ring) is behaviorally equivalent —
/// there is no cross-call persistence of the swap (E7 note).
///
/// # Errors
///
/// Propagates every [`compute_gha_packing_prep`] error, plus
/// [`PackerBridgeError::GhaUnsupportedChannelCount`] when the frontend is not a
/// 1..=2-channel group.
pub fn gha_packing_prep_from_frontend(
    state: &crate::encoder::frontend::FrontendState,
) -> Result<GhaPackingPrep, PackerBridgeError> {
    let channel_count = state.channel_count;
    if !(1..=2).contains(&channel_count) {
        return Err(PackerBridgeError::GhaUnsupportedChannelCount { channel_count });
    }
    // The active band count is the ARENA header nbands (`arena_root[2]`,
    // `header_band_count` = the extract scheduler group count), NOT the full
    // 16-band frontend width — exactly the `arena_u32(2)` the packer reads and
    // `parse_gha_group` parses. The full-width rows/records vectors are sliced
    // to this count by `compute_gha_packing_prep` (bands >= nbands are ignored).
    let band_count = state.packer_arena(0).header_band_count as usize;

    // Slice to the active GHA width (`nbands`), matching the packer-domain
    // arena the captured `parse_gha_group` builds (which reads exactly `nbands`
    // rows). Bands >= nbands hold stale/unused row state.
    let channels: Vec<Vec<GhaArenaRow>> = (0..channel_count)
        .map(|ch| {
            let arena = state.packer_arena(ch);
            arena
                .rows
                .iter()
                .zip(arena.records.iter())
                .take(band_count)
                .map(|(words, records)| GhaArenaRow {
                    words: *words,
                    records: records.clone(),
                })
                .collect()
        })
        .collect();

    // The header + share gates live in channel 0's output arena root.
    let header = state.packer_arena(0);
    let shared_flags: Vec<bool> = header.shared.iter().map(|&w| w != 0).collect();
    let stereo_flags: Vec<bool> = header.opposite.iter().map(|&w| w != 0).collect();
    let has_previous = &GHA_HAS_PREVIOUS_352[..channel_count];

    compute_gha_packing_prep(
        header.header_active != 0,
        header.header_mode as usize,
        band_count,
        &channels,
        has_previous,
        &shared_flags,
        &stereo_flags,
    )
}

fn write_gha_flag_family(
    window: &mut ObjectWindow,
    band_count: usize,
    (gate, subgate, base): (usize, usize, usize),
    flags: &[bool],
) -> Result<(), PackerBridgeError> {
    let summary = calc_nbits_gha_flag_summary_at5(flags, band_count);
    put_bytes(window, gate * 4, &u32::from(summary.any).to_le_bytes())?;
    put_bytes(window, subgate * 4, &u32::from(summary.mixed).to_le_bytes())?;
    for (band, &flag) in flags.iter().take(band_count).enumerate() {
        put_bytes(window, (base + band) * 4, &u32::from(flag).to_le_bytes())?;
    }
    Ok(())
}

/// Serialize the shared GHA header block (`gha_arena`) in place, from the
/// post-swap group arena. Writes `arena_root[0..3]` (active / mode / nbands),
/// the wave-record arena at `+0xc` in band-major channel-minor cumulative order
/// (via [`gha_record_slot_offsets`]), and — for stereo groups — the three
/// header flag families: `shared (0xc4/0xc5/0xc6..)`, `swap (0xe8/0xe9/0xea..)`,
/// `stereo (0xd6/0xd7/0xd8..)`. Each family's gate/subgate are the
/// `any`/`mixed` summary; the per-band words are the input flags (the shared
/// and stereo per-band flags are a documented content input — written by the
/// frontend extract share gates, sourced captured; only the swap per-band flags
/// and all summaries are computed here). Native evidence: decompile 6865-6965.
///
/// The record arena is written by its owning slot only (a shared band's channel
/// > 0 reuses channel 0's identical records, so it is skipped to avoid a
/// redundant write). Every byte outside the written set is left as the caller
/// provided it (captured-CUT record CONTENT, ring-delayed — see the module CUT
/// list).
#[allow(clippy::too_many_arguments)]
pub fn serialize_gha_header_block(
    window: &mut ObjectWindow,
    active: u32,
    mode: u32,
    band_count: u32,
    channels: &[Vec<GhaArenaRow>],
    shared_flags: &[bool],
    stereo_flags: &[bool],
    swap_flags: &[bool],
) -> Result<(), PackerBridgeError> {
    let bands = band_count as usize;
    put_bytes(window, GHA_HDR_WORD_ACTIVE * 4, &active.to_le_bytes())?;
    put_bytes(window, GHA_HDR_WORD_MODE * 4, &mode.to_le_bytes())?;
    put_bytes(window, GHA_HDR_WORD_NBANDS * 4, &band_count.to_le_bytes())?;

    let offsets = gha_record_slot_offsets(channels, bands, shared_flags);
    for band in 0..bands {
        for (channel, rows) in channels.iter().enumerate() {
            // Shared band, channel > 0: reuses channel 0's slot — already written.
            if channel > 0 && shared_flags.get(band).copied().unwrap_or(false) {
                continue;
            }
            let Some(row) = rows.get(band) else {
                continue;
            };
            let base = offsets[channel][band];
            for (wave, record) in row.records.iter().enumerate() {
                let at = base + wave * GHA_HDR_RECORD_STRIDE;
                put_bytes(window, at, &(record.scale_index as u32).to_le_bytes())?;
                put_bytes(
                    window,
                    at + 4,
                    &(record.amplitude_index as u32).to_le_bytes(),
                )?;
                put_bytes(window, at + 8, &(record.phase_index as u32).to_le_bytes())?;
                put_bytes(window, at + 12, &(record.frequency as u32).to_le_bytes())?;
            }
        }
    }

    if channels.len() == 2 {
        write_gha_flag_family(window, bands, GHA_HDR_SHARED, shared_flags)?;
        write_gha_flag_family(window, bands, GHA_HDR_SWAP, swap_flags)?;
        write_gha_flag_family(window, bands, GHA_HDR_STEREO, stereo_flags)?;
    }
    Ok(())
}

/// Serialize one channel's `gha_p1` window in place, from its post-swap rows.
/// Writes word 0 (the arena back-pointer `header_ptr`) and each row at byte
/// `+4 + r*0x28`: row words 0..8 from the parsed row, and row word 9 the
/// recomputed ABSOLUTE record pointer `header_ptr + slot_offsets[r]` (where
/// `slot_offsets[r] == 0xc + 0x10*cumulative` for that band's slot). Only rows
/// `0..channel_rows.len()` are written; the rest of the window is left as the
/// caller provided it.
pub fn serialize_gha_p1_window(
    window: &mut ObjectWindow,
    header_ptr: u32,
    channel_rows: &[GhaArenaRow],
    slot_offsets: &[usize],
) -> Result<(), PackerBridgeError> {
    put_bytes(window, 0, &header_ptr.to_le_bytes())?;
    for (row_index, row) in channel_rows.iter().enumerate() {
        let base = GHA_P1_ROWS_BASE + row_index * GHA_P1_ROW_STRIDE;
        for (word_index, &word) in row.words.iter().take(9).enumerate() {
            put_bytes(window, base + word_index * 4, &word.to_le_bytes())?;
        }
        let pointer = header_ptr.wrapping_add(slot_offsets[row_index] as u32);
        put_bytes(window, base + 9 * 4, &pointer.to_le_bytes())?;
    }
    Ok(())
}

/// Convert one channel's post-swap arena rows to the packer's
/// `Vec<Vec<GhaWave>>` record surface (`ObjectState.gha_records`). Each
/// [`GhaWaveRecord`] maps 1:1 to a [`GhaWave`]: `scale_index -> idsf`,
/// `amplitude_index -> idam`, `phase_index -> phase`, `frequency -> freq`.
pub fn gha_channel_records_to_waves(channel_rows: &[GhaArenaRow]) -> Vec<Vec<GhaWave>> {
    channel_rows
        .iter()
        .map(|row| {
            row.records
                .iter()
                .map(|record| GhaWave {
                    idsf: record.scale_index as u32,
                    idam: record.amplitude_index as u32,
                    phase: record.phase_index as u32,
                    freq: record.frequency as u32,
                })
                .collect()
        })
        .collect()
}

/// Serialize one channel's GHA dispatch selectors into its `range_b` window in
/// place: the four selector words `0x1c75c/0x1c760/0x1c764/0x1c768`, the IDAM
/// word `0x1c76c` only when `idam` is `Some` (never at 352), the per-band FREQ
/// reverse words `0x1c770 + band*4` only where `Some` (a `None` leaves the
/// prior/stale value), and the per-band active flags `0x1c7b0 + band*4`.
pub fn serialize_gha_selectors_range_b(
    window: &mut ObjectWindow,
    selectors: &GhaChannelSelectors,
) -> Result<(), PackerBridgeError> {
    put_bytes(
        window,
        OFFSET_GHA_IDLOC_MODE,
        &selectors.idloc.to_le_bytes(),
    )?;
    put_bytes(
        window,
        OFFSET_GHA_NWAVS_MODE,
        &selectors.nwavs.to_le_bytes(),
    )?;
    put_bytes(window, OFFSET_GHA_FREQ_MODE, &selectors.freq.to_le_bytes())?;
    put_bytes(window, OFFSET_GHA_IDSF_MODE, &selectors.idsf.to_le_bytes())?;
    if let Some(idam) = selectors.idam {
        put_bytes(window, OFFSET_GHA_IDAM_MODE, &idam.to_le_bytes())?;
    }
    for (band, reverse) in selectors.freq_modes.iter().enumerate() {
        if let Some(reverse) = reverse {
            put_bytes(
                window,
                OFFSET_GHA_FREQMODE_BASE + band * 4,
                &u32::from(*reverse).to_le_bytes(),
            )?;
        }
    }
    for (band, &flag) in selectors.active_flags.iter().enumerate() {
        put_bytes(
            window,
            OFFSET_GHA_ACTIVE_BASE + band * 4,
            &u32::from(flag).to_le_bytes(),
        )?;
    }
    Ok(())
}

/// Serialize the IDSF predictor map into the shared cfg window in place: the
/// i32 prefix at `0x11c + i*4`. A no-previous channel's map is empty (no write);
/// bytes beyond the written prefix are left as the caller provided them (a
/// no-previous channel never reads the map, and beyond-prefix slots may hold
/// stale earlier-frame values).
pub fn serialize_gha_cfg_map(
    cfg: &mut ObjectWindow,
    compact_map: &[i32],
) -> Result<(), PackerBridgeError> {
    put_i32_array(cfg, OFFSET_GHA_CFG_MAP, compact_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(base: usize, len: usize) -> ObjectWindow {
        ObjectWindow::new(base, vec![0u8; len])
    }

    fn gain_row(count: usize, locations: &[i32], levels: &[i32]) -> GainModeRow {
        GainModeRow {
            count,
            locations: locations.to_vec(),
            levels: levels.to_vec(),
        }
    }

    #[test]
    fn serializes_calc_region_at_expected_offsets() {
        let mut win = window(OBJECT_RANGE_B_BASE, 0x1780);
        let out = CalcChannelOutput {
            idsf_cc: vec![0; 32],
            slot_46: vec![0; 2],
            idct_9f8_mode: 0,
            idct_block: IdctBlockState::default(),
            idwl_block: IdwlBlockState::default(),
            idwl_copy_ran: false,
            idsf_block: None,
            o_1b578: (0..32).collect(),
            o_1b5f8: (100..132).collect(),
            o_1b678: (200..232).collect(),
            o_1b6f8: vec![7i16; 2048],
            o_1c6f8: vec![1, 2, 3, 4, 5, 9, 7, 8],
            o_1c70c0: 9,
            mode_1074: 0,
        };
        serialize_calc_object_range_b(&mut win, &out).unwrap();
        // Spot-check the tiling boundaries.
        assert_eq!(win.bytes[0xf8..0xfc], 0i32.to_le_bytes()); // 0x1b578 word0
        assert_eq!(win.bytes[0x178..0x17c], 100i32.to_le_bytes()); // 0x1b5f8 word0
        assert_eq!(win.bytes[0x1f8..0x1fc], 200i32.to_le_bytes()); // 0x1b678 word0
        assert_eq!(win.bytes[0x278..0x27a], 7i16.to_le_bytes()); // 0x1b6f8 sample0
        // 0x1c6f8 word 5 == 0x1c70c (tone flag alias).
        let rel = OFFSET_1C6F8 + 5 * 4 - OBJECT_RANGE_B_BASE;
        assert_eq!(rel, 0x1c70c - OBJECT_RANGE_B_BASE);
        assert_eq!(win.bytes[rel..rel + 4], 9i32.to_le_bytes());
    }

    #[test]
    fn serializes_idct_range_a_at_expected_offsets() {
        // range_a window base is 0 (mem_offset 0), spanning [0, 0x1110).
        let mut win = window(0, 0x1110);
        let mut idct = IdctBlockState::default();
        idct.mode = 3;
        idct.band_count = 32;
        idct.split_flag = 1;
        for band in 0..32 {
            idct.flags[band] = 1000 + band as u32;
        }
        serialize_idct_object_range_a(&mut win, &idct, 32).unwrap();
        assert_eq!(win.bytes[0x1078..0x107c], 3u32.to_le_bytes());
        assert_eq!(win.bytes[0x107c..0x1080], 32u32.to_le_bytes());
        assert_eq!(win.bytes[0x1080..0x1084], 1u32.to_le_bytes());
        assert_eq!(win.bytes[0x1084..0x1088], 1000u32.to_le_bytes());
        // Last band word lands at 0x1084 + 31*4 == 0x1100, so the copy ends at
        // exactly 0x1104.
        assert_eq!(win.bytes[0x1100..0x1104], 1031u32.to_le_bytes());
        assert_eq!(0x1084 + 31 * 4 + 4, 0x1104);
    }

    #[test]
    fn idct_count_exceeding_flags_errors() {
        let mut win = window(0, 0x1110);
        let idct = IdctBlockState::default(); // flags.len() == 32
        // config_b0 masked = 33 > 32.
        assert!(matches!(
            serialize_idct_object_range_a(&mut win, &idct, 33),
            Err(PackerBridgeError::IdctBandCountExceedsFlags {
                count: 33,
                flags_len: 32
            })
        ));
    }

    #[test]
    fn idct_count_uses_low_30_bits() {
        // Native masks with 0x3fffffff: the top two bits must not inflate count.
        let mut win = window(0, 0x1110);
        let idct = IdctBlockState::default();
        // 0xc0000020 & 0x3fffffff == 32.
        serialize_idct_object_range_a(&mut win, &idct, 0xc000_0020).unwrap();
    }

    #[test]
    fn out_of_window_offset_errors() {
        let mut win = window(OBJECT_RANGE_B_BASE, 4); // too small
        let out = CalcChannelOutput {
            idsf_cc: vec![],
            slot_46: vec![],
            idct_9f8_mode: 0,
            idct_block: IdctBlockState::default(),
            idwl_block: IdwlBlockState::default(),
            idwl_copy_ran: false,
            idsf_block: None,
            o_1b578: vec![1],
            o_1b5f8: vec![],
            o_1b678: vec![],
            o_1b6f8: vec![],
            o_1c6f8: vec![],
            o_1c70c0: 0,
            mode_1074: 0,
        };
        assert!(matches!(
            serialize_calc_object_range_b(&mut win, &out),
            Err(PackerBridgeError::OffsetOutOfWindow { .. })
        ));
    }

    // --- Gain packing-prep serializer (bridge 1.3) ------------------------

    fn gain_window() -> ObjectWindow {
        // range_b spans [0x1b480, 0x1cc00); base at 0x1b480.
        window(OBJECT_RANGE_B_BASE, 0x1780)
    }

    #[test]
    fn inactive_object_writes_nothing() {
        let mut win = gain_window();
        let rows = vec![gain_row(1, &[0], &[0])];
        serialize_gain_modes_range_b(&mut win, 0, false, &rows, None).unwrap();
        assert!(
            win.bytes.iter().all(|&b| b == 0),
            "inactive object must leave the window untouched"
        );
    }

    #[test]
    fn ch1_without_prev_rows_errors() {
        let mut win = gain_window();
        let rows = vec![gain_row(1, &[0], &[0])];
        assert!(matches!(
            serialize_gain_modes_range_b(&mut win, 1, true, &rows, None),
            Err(PackerBridgeError::GainMissingPreviousRows)
        ));
    }

    #[test]
    fn row_count_over_max_errors() {
        let mut win = gain_window();
        // count 8 > GAIN_MAX_POINTS (7); the arrays are sized to match so only
        // the count guard trips.
        let rows = vec![gain_row(8, &[0; 8], &[0; 8])];
        assert!(matches!(
            serialize_gain_modes_range_b(&mut win, 0, true, &rows, None),
            Err(PackerBridgeError::GainRowCountExceedsMax { count: 8, max: 7 })
        ));
    }

    #[test]
    fn ch0_writes_only_the_mode_and_range_words() {
        // Poison-fill the window; only ch0's 9 mode/width/min words may change.
        let mut win = ObjectWindow::new(OBJECT_RANGE_B_BASE, vec![0xAAu8; 0x1780]);
        let rows = vec![
            gain_row(2, &[3, 9], &[1, 2]),
            gain_row(2, &[4, 10], &[2, 3]),
        ];
        serialize_gain_modes_range_b(&mut win, 0, true, &rows, None).unwrap();

        let written = [
            OFFSET_1B494,
            OFFSET_1B498,
            OFFSET_1B49C,
            OFFSET_1B4A0,
            OFFSET_1B4A4,
            OFFSET_1B4A8,
            OFFSET_1B4EC,
            OFFSET_1B4F0,
            OFFSET_1B4F4,
        ];
        // A byte is "written" iff it falls inside one of the 9 modeled words.
        let in_write_set = |offset: usize| written.iter().any(|&w| offset >= w && offset < w + 4);
        for offset in GAIN_MODE_RANGE_B {
            if in_write_set(offset) {
                continue;
            }
            let rel = offset - OBJECT_RANGE_B_BASE;
            assert_eq!(
                win.bytes[rel], 0xAA,
                "ch0 byte {offset:#x} outside the write set must stay poison"
            );
        }
    }

    #[test]
    fn ch1_writes_flags_and_marker_prefix() {
        // ch1 writes 3 mode words + per-row flags + marker prefix; the range/min
        // words (ch0-only) must stay poison.
        let mut win = ObjectWindow::new(OBJECT_RANGE_B_BASE, vec![0xAAu8; 0x1780]);
        let rows = vec![gain_row(1, &[0], &[1]), gain_row(1, &[2], &[3])];
        let prev = vec![gain_row(1, &[0], &[1]), gain_row(1, &[2], &[3])];
        serialize_gain_modes_range_b(&mut win, 1, true, &rows, Some(&prev)).unwrap();

        // ch0-only width/min words are never written by ch1: stay poison.
        for offset in [
            OFFSET_1B498,
            OFFSET_1B49C,
            OFFSET_1B4A4,
            OFFSET_1B4A8,
            OFFSET_1B4F0,
            OFFSET_1B4F4,
        ] {
            let rel = offset - OBJECT_RANGE_B_BASE;
            assert_eq!(
                &win.bytes[rel..rel + 4],
                &[0xAA; 4],
                "ch1 must not write the ch0-only word {offset:#x}"
            );
        }
    }

    // --- IDWL packing-prep serializer (bridge 1.4) ------------------------

    fn idwl_block_mode3() -> IdwlBlockState {
        // ch0 call-7 record: sel=0, record[0]=1, record[1]=0, record[2]=32.
        let mut block = IdwlBlockState::default();
        block.mode = 3;
        block.selector_fields_50_60 = [1, 0, 32, 0, 0]; // [rec0, rec1, rec2, rec3, sel]
        for (i, slot) in block.word_rows[0].iter_mut().enumerate() {
            *slot = 100 + i as i32;
        }
        block
    }

    #[test]
    fn idwl_copy_skipped_when_not_run() {
        let mut win = ObjectWindow::new(OBJECT_RANGE_B_BASE, vec![0xAAu8; 0x1780]);
        serialize_idwl_object_range_b(&mut win, 0, &idwl_block_mode3(), false, &[0; 3]).unwrap();
        assert!(
            win.bytes.iter().all(|&b| b == 0xAA),
            "copy_ran=false must leave the window untouched (native residue)"
        );
    }

    #[test]
    fn idwl_writes_record_and_plane_at_expected_offsets() {
        let mut win = window(OBJECT_RANGE_B_BASE, 0x1780);
        serialize_idwl_object_range_b(&mut win, 0, &idwl_block_mode3(), true, &[0; 3]).unwrap();
        let at = |off: usize| {
            let rel = off - OBJECT_RANGE_B_BASE;
            i32::from_le_bytes(win.bytes[rel..rel + 4].try_into().unwrap())
        };
        assert_eq!(at(OFFSET_1C70C), 3); // mode
        assert_eq!(at(OFFSET_1C71C), 0); // sel (record[4])
        assert_eq!(at(OFFSET_1C720), 1); // record[0]
        assert_eq!(at(OFFSET_1C724), 0); // record[1]
        assert_eq!(at(OFFSET_1C728), 32); // record[2]
        assert_eq!(at(OFFSET_1C72C), 0); // record[3]
        assert_eq!(at(OFFSET_1C7F0), 100); // plane word 0 = word_rows[0][0]
        assert_eq!(at(OFFSET_1C7F0 + 31 * 4), 131); // plane word 31
        // The plane ends exactly at 0x1c870.
        assert_eq!(OFFSET_1C7F0 + IDWL_PLANE_WORDS * 4, 0x1c870);
    }

    #[test]
    fn idwl_ch0_tone_mode1_writes_shared_window_triple() {
        // docs/12 §1.3: ch0 tone mode 1 is native-observed (dance sweep, 7 rows)
        // and no longer errors. The tail copies the SHARED window-fields triple
        // into 0x1c710/14/18. mode=1 selects record[4] from selector_fields_28_38
        // (record[4]=sel); keep sel in range so the plane copy succeeds.
        let mut win = window(OBJECT_RANGE_B_BASE, 0x1780);
        let mut block = IdwlBlockState::default();
        block.mode = 1; // ch0 tone mode 1: ported arm.
        block.selector_fields_28_38 = [7, 8, 9, 10, 0]; // [rec0..3, sel=0]
        let shared = [11i32, 22, 33];
        serialize_idwl_object_range_b(&mut win, 0, &block, true, &shared)
            .expect("ch0 tone mode 1 serializes (no error)");
        let at = |off: usize| {
            let rel = off - OBJECT_RANGE_B_BASE;
            i32::from_le_bytes(win.bytes[rel..rel + 4].try_into().unwrap())
        };
        // Generic record/plane words still written for mode 1.
        assert_eq!(at(OFFSET_1C70C), 1); // mode
        assert_eq!(at(OFFSET_1C71C), 0); // sel (record[4])
        assert_eq!(at(OFFSET_1C720), 7); // record[0]
        assert_eq!(at(OFFSET_1C724), 8); // record[1]
        assert_eq!(at(OFFSET_1C728), 9); // record[2]
        assert_eq!(at(OFFSET_1C72C), 10); // record[3]
        // The mode-1 tail: the SHARED window triple, little-endian.
        assert_eq!(at(OFFSET_1C710), 11); // prefix_count
        assert_eq!(at(OFFSET_1C714), 22); // residual_bits
        assert_eq!(at(OFFSET_1C718), 33); // residual_base
    }

    #[test]
    fn idwl_ch0_tone_mode2_serializes_plane_and_cfg_group_law() {
        // docs/13 §3.1 slice 2: ch0 tone mode 2 is native-observed at 192 (sweat
        // output frame 5556) and PORTED. The tail copies the side-record symbol
        // plane + field words + subgroup flag into range_b; the cfg group-flag
        // law lands in the shared cfg. Construct a known block and verify both.
        let mut block = IdwlBlockState::default();
        block.mode = 2;
        // record = [huffman(rec0), selector_b(rec1), count(rec2), rec3, sel(rec4)].
        // count = 5 is ODD: group_count = 5 >> 1 = 2, so symbol[4] is a tail the
        // cfg group loop never reaches (exercises the odd-count boundary).
        block.selector_fields_3c_4c = [0, 1, 5, 0, 0];
        block.side.subgroup_flag = 1;
        // side row 1 = the winning candidate row. Symbols row[0..32]; field words
        // at row[32]/row[33]. Pair-zero law: group 0 (i=0,1) both zero -> flag 1;
        // group 1 (i=2,3) has a nonzero -> flag 0.
        let row = &mut block.side.rows[1];
        row[0] = 0;
        row[1] = 0;
        row[2] = 5;
        row[3] = 0;
        row[4] = 6; // odd-count tail symbol (not in any cfg group).
        row[32] = 9; // field_4bits
        row[33] = 7; // field_3bits

        let mut win = window(OBJECT_RANGE_B_BASE, 0x1780);
        serialize_idwl_object_range_b(&mut win, 0, &block, true, &[0; 3])
            .expect("ch0 tone mode 2 serializes (no error)");
        let at = |off: usize| {
            let rel = off - OBJECT_RANGE_B_BASE;
            i32::from_le_bytes(win.bytes[rel..rel + 4].try_into().unwrap())
        };
        // Generic record words (from selector_fields_3c_4c).
        assert_eq!(at(OFFSET_1C70C), 2); // mode
        assert_eq!(at(OFFSET_1C71C), 0); // sel (record[4])
        assert_eq!(at(OFFSET_1C720), 0); // record[0] (huffman selector)
        assert_eq!(at(OFFSET_1C724), 1); // record[1] (selector_b)
        assert_eq!(at(OFFSET_1C728), 5); // record[2] (count)
        // Mode-2 tail: field words + subgroup flag.
        assert_eq!(at(OFFSET_1C730), 9); // field_4bits = row[32]
        assert_eq!(at(OFFSET_1C734), 7); // field_3bits = row[33]
        assert_eq!(at(OFFSET_1C738), 1); // subgroup_flag
        // Symbol plane = row[0..32].
        assert_eq!(at(OFFSET_1C870), 0); // symbol 0
        assert_eq!(at(OFFSET_1C870 + 2 * 4), 5); // symbol 2
        assert_eq!(at(OFFSET_1C870 + 4 * 4), 6); // symbol 4 (odd tail)
        assert_eq!(OFFSET_1C870 + IDWL_PLANE_WORDS * 4, 0x1c8f0);

        // cfg group-flag law on the shared cfg window.
        let mut cfg = window(0, 0x400);
        serialize_idwl_mode2_cfg_words(&mut cfg, &block).unwrap();
        let cfg_at = |off: usize| i32::from_le_bytes(cfg.bytes[off..off + 4].try_into().unwrap());
        assert_eq!(cfg_at(CFG_OFFSET_D4), 2); // count >> 1
        assert_eq!(cfg_at(CFG_OFFSET_D8), 1); // group 0: symbols 0,1 both zero
        assert_eq!(cfg_at(CFG_OFFSET_D8 + 4), 0); // group 1: symbol 2 nonzero
    }

    #[test]
    fn idwl_mode2_cfg_selector_out_of_range_errors() {
        let mut cfg = window(0, 0x400);
        let mut block = IdwlBlockState::default();
        block.mode = 2;
        block.selector_fields_3c_4c = [0, 4, 2, 0, 0]; // selector_b = 4 (>= row count).
        assert!(matches!(
            serialize_idwl_mode2_cfg_words(&mut cfg, &block),
            Err(PackerBridgeError::IdwlSelectorOutOfRange { selector: 4 })
        ));
    }

    #[test]
    fn idwl_ch1_mode1_does_not_write_shared_triple() {
        // The ch0-only mode-1 tail must NOT fire on ch1 (dispatch-5 context):
        // native gates the tail on `obj0` (channel 0) only.
        let mut win = ObjectWindow::new(OBJECT_RANGE_B_BASE, vec![0xAAu8; 0x1780]);
        let mut block = IdwlBlockState::default();
        block.mode = 1;
        block.selector_fields_28_38 = [1, 2, 3, 4, 0]; // sel=0 in range
        serialize_idwl_object_range_b(&mut win, 1, &block, true, &[11, 22, 33])
            .expect("ch1 mode 1 serializes");
        // The ch0-only aux words stay untouched (poison), for ch1.
        for offset in [OFFSET_1C710, OFFSET_1C714, OFFSET_1C718] {
            let rel = offset - OBJECT_RANGE_B_BASE;
            assert_eq!(
                &win.bytes[rel..rel + 4],
                &[0xAA; 4],
                "ch1 must not write the ch0-only tone word {offset:#x}"
            );
        }
    }

    #[test]
    fn idwl_selector_out_of_range_errors() {
        let mut win = window(OBJECT_RANGE_B_BASE, 0x1780);
        let mut block = IdwlBlockState::default();
        block.mode = 3;
        block.selector_fields_50_60 = [0, 0, 0, 0, 4]; // sel=4 (>= row count).
        assert!(matches!(
            serialize_idwl_object_range_b(&mut win, 0, &block, true, &[0; 3]),
            Err(PackerBridgeError::IdwlSelectorOutOfRange { selector: 4 })
        ));
    }

    // --- IDSF packing-prep serializer (bridge 1.4) ------------------------

    fn idsf_block_ch0() -> IdsfBlockState {
        // ch0 call-7: mode=3,start=0,count=5,base=27,huff=3,mode_sel=1,cb=55,cbase=46.
        let mut block = IdsfBlockState::default();
        block.mode = 3;
        block.start = 0;
        block.count = 5;
        block.field_0x1c748 = 27;
        block.huffman_selector = 3;
        block.mode_selector = 1;
        block.codebook_selector = 55;
        block.compact_base = 46;
        block
    }

    #[test]
    fn idsf_writes_header_and_planes_at_expected_offsets() {
        let mut win = window(OBJECT_RANGE_B_BASE, 0x1780);
        let block = idsf_block_ch0();
        serialize_idsf_object_range_b(&mut win, 0, Some(&block)).unwrap();
        let at = |off: usize| {
            let rel = off - OBJECT_RANGE_B_BASE;
            i32::from_le_bytes(win.bytes[rel..rel + 4].try_into().unwrap())
        };
        assert_eq!(at(OFFSET_1C73C), 3); // mode
        assert_eq!(at(OFFSET_1C740), 0); // start
        assert_eq!(at(OFFSET_1C744), 5); // count
        assert_eq!(at(OFFSET_1C748), 27); // base
        assert_eq!(at(OFFSET_1C74C), 3); // huffman
        assert_eq!(at(OFFSET_1C750), 1); // mode_selector
        assert_eq!(at(OFFSET_1C754), 55); // codebook
        assert_eq!(at(OFFSET_1C758), 46); // compact_base
        // Plane bases: row 1 at 0x1c970, transformed at 0x1ca70.
        assert_eq!(OFFSET_1C8F0 + 0x80, 0x1c970);
        assert_eq!(OFFSET_1C8F0 + 2 * 0x80, 0x1c9f0);
        assert_eq!(OFFSET_1CA70, 0x1ca70);
    }

    #[test]
    fn idsf_zero_arm_writes_only_mode_words() {
        // `+0x8c == 0` gate: state None writes only 0x1c73c/0x1c74c (+ ch0 0x1c750).
        let mut win = ObjectWindow::new(OBJECT_RANGE_B_BASE, vec![0xAAu8; 0x1780]);
        serialize_idsf_object_range_b(&mut win, 0, None).unwrap();
        let poison_zero = [OFFSET_1C73C, OFFSET_1C74C, OFFSET_1C750];
        for off in IDSF_HEADER_RANGE_B {
            let rel = off - OBJECT_RANGE_B_BASE;
            let in_zero = poison_zero.iter().any(|&w| off >= w && off < w + 4);
            if in_zero {
                assert_eq!(win.bytes[rel], 0, "zero-arm word {off:#x} must be zeroed");
            } else {
                assert_eq!(win.bytes[rel], 0xAA, "byte {off:#x} must stay poison");
            }
        }
        // ch1 zero arm does NOT write 0x1c750.
        let mut win1 = ObjectWindow::new(OBJECT_RANGE_B_BASE, vec![0xAAu8; 0x1780]);
        serialize_idsf_object_range_b(&mut win1, 1, None).unwrap();
        let rel = OFFSET_1C750 - OBJECT_RANGE_B_BASE;
        assert_eq!(
            &win1.bytes[rel..rel + 4],
            &[0xAA; 4],
            "ch1 zero arm must not write 0x1c750"
        );
    }

    // --- GHA arena serializer (bridge 1.5) --------------------------------

    fn arena_row(nwavs: usize) -> GhaArenaRow {
        GhaArenaRow {
            words: [0, 0, 0, 0, 0, 0, 0, 0, nwavs as u32, 0],
            records: vec![
                GhaWaveRecord {
                    scale_index: 0,
                    amplitude_index: 0,
                    phase_index: 0,
                    frequency: 0,
                };
                nwavs
            ],
        }
    }

    #[test]
    fn gha_placement_is_band_major_channel_minor() {
        // Call-7 shape: ch0 = [14, 5], ch1 = [12, 0], no sharing.
        let channels = vec![
            vec![arena_row(14), arena_row(5)],
            vec![arena_row(12), arena_row(0)],
        ];
        let offsets = gha_record_slot_offsets(&channels, 2, &[false, false]);
        assert_eq!(offsets[0][0], 0xc); // ch0 b0 @ cum 0
        assert_eq!(offsets[1][0], 0xec); // ch1 b0 @ cum 14
        assert_eq!(offsets[0][1], 0x1ac); // ch0 b1 @ cum 26
        assert_eq!(offsets[1][1], 0x1fc); // ch1 b1 @ cum 31
    }

    #[test]
    fn gha_placement_shared_band_reuses_channel0_slot() {
        // Extract-domain shape: band 0 shared -> ch1 b0 reuses ch0 b0's slot.
        let channels = vec![
            vec![arena_row(8), arena_row(6)],
            vec![arena_row(8), arena_row(4)],
        ];
        let offsets = gha_record_slot_offsets(&channels, 2, &[true, false]);
        assert_eq!(offsets[0][0], 0xc);
        assert_eq!(offsets[1][0], 0xc); // shared: reuse ch0
        assert_eq!(offsets[0][1], 0x8c); // ch0 b1 @ cum 8
        assert_eq!(offsets[1][1], 0xec); // ch1 b1 @ cum 14 (shared band consumed no slot)
    }

    #[test]
    fn gha_header_mode_zero_is_unsupported() {
        let channels = vec![vec![arena_row(1), arena_row(0)]];
        assert!(matches!(
            compute_gha_packing_prep(true, 0, 2, &channels, &[false], &[], &[]),
            Err(PackerBridgeError::GhaHeaderModeZeroUnsupported)
        ));
    }

    #[test]
    fn gha_wave_total_over_max_errors() {
        let channels = vec![vec![arena_row(0x28)], vec![arena_row(0x10)]];
        assert!(matches!(
            compute_gha_packing_prep(true, 1, 1, &channels, &[false, true], &[false], &[false]),
            Err(PackerBridgeError::GhaWaveTotalExceedsMax {
                total: 0x38,
                max: 0x30
            })
        ));
    }

    #[test]
    fn gha_wave_total_counts_shared_band_once() {
        // Native's 0x30 count (decompile 42650–42675) skips a channel > 0
        // row whose per-band shared flag is set: the shared band aliases
        // channel 0's arena slot and consumes no new records. A single
        // shared band with 0x20 waves on each channel is 0x20 total, not
        // 0x40 — it must NOT trip the cap.
        let channels = vec![vec![arena_row(0x20)], vec![arena_row(0x20)]];
        let prep =
            compute_gha_packing_prep(true, 1, 1, &channels, &[false, true], &[true], &[false]);
        assert!(
            prep.is_ok(),
            "shared band must be counted once (0x20), got {prep:?}"
        );

        // An UNSHARED band with the same per-channel totals really is 0x40
        // > 0x30 and still errors (the skip is gated on the shared flag).
        let unshared =
            compute_gha_packing_prep(true, 1, 1, &channels, &[false, true], &[false], &[false]);
        assert!(matches!(
            unshared,
            Err(PackerBridgeError::GhaWaveTotalExceedsMax {
                total: 0x40,
                max: 0x30
            })
        ));

        // A shared band whose channel 0 count alone exceeds the cap still
        // errors (the skip removes only the channel > 0 duplicate).
        let over = vec![vec![arena_row(0x31)], vec![arena_row(0x31)]];
        assert!(matches!(
            compute_gha_packing_prep(true, 1, 1, &over, &[false, true], &[true], &[false]),
            Err(PackerBridgeError::GhaWaveTotalExceedsMax {
                total: 0x31,
                max: 0x30
            })
        ));
    }
}
