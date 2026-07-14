//! docs/11 Phase 2 §2.2 — the fully computed single-frame assembly and the
//! rolling computed driver.
//!
//! Phase 1 proved every packer window computable by SUBSTITUTING computed
//! windows into a CAPTURED call-7 prepacker state
//! (`tests/composed_frame.rs::phase1_computed_pipeline_from_pcm_packs_byte_exact`).
//! This module removes the captured scaffold: [`build_computed_prepacker_state`]
//! assembles the whole [`FramePrepackerState`] FROM SCRATCH out of the computed
//! per-call outputs, and [`ComputedFrameDriver`] drives the single-call assembly
//! over every core call with owned rolling state.
//!
//!
//! Frame-invariant over all 77 captured frames: one block group (`block_count =
//! 1`, `nblk = 2`), `frame_bytes = 2048`, two objects with `channel_index`
//! 0 / 1, both with `previous_index = Some(0)` (native `*(obj+0x28)` points at
//! the ch0 object for both channels on every captured frame). Per-object window
//! geometry: `range_a` mem_offset 0 len 0x1110; `range_b` mem_offset 0x1b480 len
//! 0x1780; `cfg` mem_offset 0 len 0x400; `gainb` mem_offset 0 len 0xb00;
//! `gha_arena` mem_offset 0 len 0x800; `gha_p1` mem_offset 0 len 0x1000.
//!
//! Windows are zero-filled to that geometry, then the EXISTING serializers write
//! their computed content onto them, mirroring the substitution order of the
//! Phase-1 capstone (`substitute_calc_surface`, then cfg, IDCT, gain modes,
//! IDWL/IDSF, GHA), plus the two new §2.2 windows (`gainb`, init header).
//!
//! # Divergence policy (docs/11 §5 anchor-early / free-late)
//!
//! The computed call-7 spectrum carries a documented ~1e-4 x87 drift that flips
//! one quant-cost bit (ch0 band 30, docs/11 §1.1), so the pure-computed frames
//! are NOT expected to be byte-exact vs native past the parity horizon. Past the
//! drift front the computed values may select packer dispatch arms outside the
//! proven live set, which fail with explicit errors BY DESIGN — this module
//! never wires new arms; it surfaces the error to the caller.

use crate::bitstream::frame::{
    BlockGroup, FramePrepackerState, ObjectState, ObjectWindow, pack_frame_at5,
};
use crate::bitstream::writer::BitWriter;
use crate::coding::allocation::{
    ZerothActivitySummary, zeroth_activity_summary_at5, zeroth_band_shape_counts_at5,
};
use crate::coding::calc_block::{CalcFrameEntry, CalcFrameOutput, calc_channel_block_frame_at5};
use crate::encoder::cfg_bridge::{CfgPerFrame352, build_cfg_window};
use crate::encoder::coding_bridge::{
    CodingBridgeError, GainRollState, InitGainHeaderWords, ZerothBridgeChannelAux,
    ZerothBridgeFrameAux, assemble_calc_frame_entry_with_init_for_params_at5,
    assemble_gain_a_records, init_roll_step, zeroth_band_activity_from_frontend,
    zeroth_tone_activity_from_frontend, zeroth_tone_words_from_frontend,
};
use crate::encoder::coding_params::CodingParams;
use crate::encoder::frontend::{
    FRONTEND_CHANNEL_COUNT, FrontendCoreCallReport, FrontendError, FrontendState,
    frontend_core_call_at5,
};
use crate::encoder::packer_bridge::{
    GHA_HAS_PREVIOUS_352, GhaPackingPrep, PackerBridgeError, gha_channel_records_to_waves,
    gha_packing_prep_from_frontend, gha_record_slot_offsets, serialize_calc_object_range_a,
    serialize_calc_object_range_b, serialize_gainb_window, serialize_gha_cfg_map,
    serialize_gha_header_block, serialize_gha_p1_window, serialize_gha_selectors_range_b,
    serialize_idct_object_range_a, serialize_idsf_object_range_b, serialize_idwl_mode2_cfg_words,
    serialize_idwl_object_range_b, serialize_init_gain_header_range_b,
};
use crate::encoder::profile::ATRAC3PLUS_352;
use crate::pipeline::syntax::{FrameSyntax, FrameSyntaxError};

/// Frame byte length of the 352 stereo single-block frame.
pub const COMPUTED_FRAME_BYTES: usize = 2048;
/// Block-group count for the 352 stereo path (`atx_state + 0xc`).
pub const COMPUTED_BLOCK_COUNT: usize = 1;
/// Blocks per group (`cfg + 0xa8`).
pub const COMPUTED_NBLK: usize = 2;
/// `range_a` window length (`[0, 0x1110)`).
pub const RANGE_A_LEN: usize = 0x1110;
/// `range_b` window base (`[0x1b480, 0x1cc00)`).
pub const RANGE_B_BASE: usize = 0x1b480;
/// `range_b` window length.
pub const RANGE_B_LEN: usize = 0x1780;
/// `cfg` window length (`[0, 0x400)`).
pub const CFG_LEN: usize = 0x400;
/// `gainb` window length (`*(obj+8)`, `[0, 0xb00)`).
pub const GAINB_LEN: usize = 0xb00;
/// GHA header arena window length (`gha_hdr`).
pub const GHA_ARENA_LEN: usize = 0x800;
/// GHA per-channel `gha_p1` window length.
pub const GHA_P1_LEN: usize = 0x1000;

/// The synthetic identity-only GHA arena back-pointer for the from-scratch
/// `gha_p1` window (packer-UNREAD per docs/11 §2.1c E4 — the packer never reads
/// the p1 base pointer nor row word 9). Follows the
/// `frontend::SYNTHETIC_ARENA_HEADER` precedent.
pub const SYNTHETIC_ARENA_HEADER: i32 = 0x1000_0000;

/// The error surface of the computed single-frame assembly + rolling driver.
#[derive(Debug, Clone, PartialEq)]
pub enum ComputedFrameError {
    /// The frontend core call failed.
    Frontend(FrontendError),
    /// The coding bridge (init roll / calc-entry assembly) failed.
    Coding(CodingBridgeError),
    /// A packer bridge serializer failed (typically an untraceable dispatch arm
    /// past the parity horizon — surfaced, never wired).
    Packer(PackerBridgeError),
    /// `calc_channel_block_frame_at5` rejected the computed entry. Carries the
    /// underlying guard so the probe names the branch (docs/12 §2.3).
    Calc(crate::coding::calc_block::CalcBlockError),
    /// The GHA prep is missing a channel's dispatch surface the from-scratch
    /// serializer needs (the header was inactive but the state expects two
    /// channels). Never guess a surface.
    GhaChannelMissing { channel: usize },
    /// `pack_frame_at5` failed on the assembled state — the frame is not a legal
    /// bitstream (a dispatch arm outside the proven live set, past the horizon).
    Pack(crate::bitstream::frame::FrameAssemblyError),
    /// The assembled reference layout could not be represented as typed frame
    /// syntax without violating structural invariants.
    Syntax(FrameSyntaxError),
}

impl From<FrontendError> for ComputedFrameError {
    fn from(error: FrontendError) -> Self {
        ComputedFrameError::Frontend(error)
    }
}
impl From<CodingBridgeError> for ComputedFrameError {
    fn from(error: CodingBridgeError) -> Self {
        ComputedFrameError::Coding(error)
    }
}
impl From<PackerBridgeError> for ComputedFrameError {
    fn from(error: PackerBridgeError) -> Self {
        ComputedFrameError::Packer(error)
    }
}
impl From<FrameSyntaxError> for ComputedFrameError {
    fn from(error: FrameSyntaxError) -> Self {
        Self::Syntax(error)
    }
}

/// The computed per-object windows the from-scratch state builder needs, one per
/// channel. Everything is produced by the existing computed pipeline; this is a
/// plain carrier so the builder stays a pure assembler.
pub struct ComputedObjectInputs {
    /// Init gain-classification header words (`range_b [0x1b484, 0x1b494)`).
    pub init_header: InitGainHeaderWords,
    /// This channel's assembled 16×38-word gain-A records (point prefix + zero
    /// tail), laid into the `gainb` window rows.
    pub gain_a_records: Vec<u32>,
    /// This channel's per-band activity row (`gainb +0x988`).
    pub band_activity: Vec<i32>,
}

/// Assemble the whole [`FramePrepackerState`] FROM SCRATCH (docs/11 §2.2 (a)3)
/// from the computed calc entry/output, the per-frame cfg fields, the GHA prep,
/// and the per-object [`ComputedObjectInputs`].
///
/// Windows are zero-filled to the frame-invariant geometry, then the existing
/// serializers write their computed content onto them (calc surface first, then
/// cfg, IDCT, gain modes, IDWL/IDSF, gainb, init header, and the GHA surface).
///
/// `shared`/`stereo` per-band flags and `nbands`/`active`/`mode` are read from
/// the rolling frontend's ring-slot-0 arena (via `frontend`), exactly as the
/// capstone's `substitute_gha_from_frontend` does; `prep` is the SAME
/// `gha_packing_prep_from_frontend` result whose `total_bits` fed `gha_bits`.
pub fn build_computed_prepacker_state(
    out: &CalcFrameOutput,
    per_frame: &CfgPerFrame352,
    frontend: &FrontendState,
    prep: &GhaPackingPrep,
    objects: &[ComputedObjectInputs],
) -> Result<FramePrepackerState, ComputedFrameError> {
    // 352 wrapper: selector 30, budget 16379, 2048 frame bytes.
    build_computed_prepacker_state_for_params(
        out,
        per_frame,
        frontend,
        prep,
        objects,
        30,
        16379,
        COMPUTED_FRAME_BYTES,
        0x20,
        16,
    )
}

/// Like [`build_computed_prepacker_state`] but with an explicit per-rate block
/// `selector`, frame bit `budget`, and `frame_bytes` (docs/13 §1.1). The window
/// geometry (`range_a`/`range_b`/`cfg`/`gainb`/`gha_*`) is rate-INDEPENDENT
/// (heap-block shapes, docs/13 §2.2); only the cfg selector/budget words and the
/// final frame byte length vary. At (30, 16379, 2048) this is byte-identical to
/// the shipped 352 path.
#[allow(clippy::too_many_arguments)]
pub fn build_computed_prepacker_state_for_params(
    out: &CalcFrameOutput,
    per_frame: &CfgPerFrame352,
    frontend: &FrontendState,
    prep: &GhaPackingPrep,
    objects: &[ComputedObjectInputs],
    selector: i32,
    budget: i32,
    frame_bytes: usize,
    band_index: u32,
    band_count: u32,
) -> Result<FramePrepackerState, ComputedFrameError> {
    // Object count is profile-driven (docs/14 §0.4): the `nblk` written into the
    // block group is `objects.len()` (2 for the nine stereo rows == COMPUTED_NBLK,
    // 1 for the five mono rows). The caller (`compute_output_frame`) builds exactly
    // `params.channels` objects, so this is where the driver's channel count is
    // realized. Fail-explicit on an empty group rather than pack a headerless
    // block. Only stereo (2) reaches here in any shipping/test path.
    let nblk = objects.len();
    if nblk == 0 {
        return Err(ComputedFrameError::GhaChannelMissing { channel: 0 });
    }

    // Shared cfg window (both objects carry the identical block). The per-FRAME
    // effective band extent (`cfg+0xb4`/`+0xbc`, post `+0x1dc & 0x7c` override) is
    // packer-unread, so it cannot change the packed bytes (docs/13 §3.1 slice 3);
    // threaded for observation honesty. The channel count (`nblk` = one object
    // per channel) threads the two channel-mode cfg words: `cfg+0xa0 =
    // (channel_count != 1)` — the 2-bit block-type header the packer EMITS
    // (decompile 34413) — and `cfg+0xa8 = channel_count` (decompile 48766; the
    // native packer's `iVar19` word, mirrored by `BlockGroup.nblk` in Rust).
    let mut cfg = build_cfg_window(
        per_frame,
        selector as u32,
        budget,
        band_index,
        band_count,
        nblk as u32,
    );
    debug_assert_eq!(cfg.bytes.len(), CFG_LEN);
    // The IDCT copy count is the shared-cfg `0xb0` word (= active band count).
    let config_b0 = per_frame.active_b0;

    // The gainb band-activity group count (`cfg+0xc8`, `piVar9[0x32]`): the count
    // the pack per-band `0x988` loop consumes AND the count native evaluates the
    // `0x980`/`0x984` any/partial summary over (decompile 37580). Computed with
    // the same band-shape law `build_cfg_window` writes into `cfg+0xc8`, so the
    // gainb summary agrees with both the pack loop and the `+0x122` accounting.
    let gainb_group_count = zeroth_band_shape_counts_at5(
        band_index as usize,
        band_index as usize,
        band_count as usize,
    )
    .group_count;

    // ch0 IDWL tone-mode-2 shared-cfg group law (docs/13 §3.1 slice 2). Native
    // `calc_channel_block_at5` writes cfg[0xd4]/cfg[0xd8..] into the SHARED cfg
    // window (`*(obj+4)`) once per block when ch0 selects mode 2, gated on the
    // same copy-ran region as the mode-1 tail. Emit it BEFORE the per-channel
    // `cfg.clone()` so both objects' cfg views carry the words. Dead at
    // 352/320/256 (ch0 mode 2 never selected), so byte output is unchanged.
    if out.channels[0].idwl_copy_ran && out.channels[0].idwl_block.mode == 2 {
        serialize_idwl_mode2_cfg_words(&mut cfg, &out.channels[0].idwl_block)?;
    }

    // GHA arena metadata + per-band flags from ring slot 0 (channel 0's arena
    // root, where extract writes the header and share gates — docs/11 §2.1c).
    let arena = frontend.packer_arena(0);
    let gha_active = arena.header_active;
    let gha_mode = arena.header_mode;
    let gha_nbands = arena.header_band_count as usize;
    let gha_shared: Vec<bool> = arena.shared.iter().map(|&w| w != 0).collect();
    let gha_stereo: Vec<bool> = arena.opposite.iter().map(|&w| w != 0).collect();
    let offsets = gha_record_slot_offsets(&prep.post_swap_channels, gha_nbands, &gha_shared);

    // Serialize the shared GHA header block once (into a zeroed window), then
    // clone it into both objects — the non-write-set bytes stay zero (packer-
    // UNREAD residue, proven by the from-scratch anchor test).
    let mut gha_header = ObjectWindow::new(0, vec![0u8; GHA_ARENA_LEN]);
    serialize_gha_header_block(
        &mut gha_header,
        gha_active,
        gha_mode,
        gha_nbands as u32,
        &prep.post_swap_channels,
        &gha_shared,
        &gha_stereo,
        &prep.swap_flags,
    )?;

    let mut group_objects = Vec::with_capacity(nblk);
    for (ch, obj_inputs) in objects.iter().enumerate() {
        let co = &out.channels[ch];
        let channel_index = ch as u32;

        // range_a: zero, then the Slice E calc surface word 0x1074 + the IDCT
        // copy [0x1078, 0x1104).
        let mut range_a = ObjectWindow::new(0, vec![0u8; RANGE_A_LEN]);
        serialize_calc_object_range_a(&mut range_a, co)?;
        serialize_idct_object_range_a(&mut range_a, &co.idct_block, config_b0)?;

        // range_b: zero, then the init gain header, the Slice E calc surface,
        // the gain modes, the IDWL/IDSF side data. The GHA selectors are written
        // after the arena content below (matching the capstone order).
        let mut range_b = ObjectWindow::new(RANGE_B_BASE, vec![0u8; RANGE_B_LEN]);
        serialize_init_gain_header_range_b(&mut range_b, &obj_inputs.init_header)?;
        serialize_calc_object_range_b(&mut range_b, co)?;
        // gain modes: source `active` + row count from the init header, the rows
        // from this channel's computed gain-A records; ch1's reference rows come
        // from the ch0 object's records at ch1's row count (native `*(obj[10]+8)`).
        let active = obj_inputs.init_header.obj_1b484 != 0;
        let row_count = obj_inputs.init_header.obj_1b490.max(0) as usize;
        let rows = gain_mode_rows_from_records(&obj_inputs.gain_a_records, row_count);
        let prev_rows = if channel_index == 0 {
            None
        } else {
            Some(gain_mode_rows_from_records(
                &objects[0].gain_a_records,
                row_count,
            ))
        };
        crate::encoder::packer_bridge::serialize_gain_modes_range_b(
            &mut range_b,
            channel_index,
            active,
            &rows,
            prev_rows.as_deref(),
        )?;
        serialize_idwl_object_range_b(
            &mut range_b,
            channel_index,
            &co.idwl_block,
            co.idwl_copy_ran,
            // The ch0 tone-mode-1 tail reads the FINAL shared window-fields
            // scratch (last-writer-wins across both channels' mode-1 costing),
            // not ch0's per-block copy (docs/12 §1.3).
            &out.shared_wlc_window_fields,
        )?;
        serialize_idsf_object_range_b(&mut range_b, channel_index, co.idsf_block.as_ref())?;

        // gainb window (built from scratch by the §2.2 serializer).
        let gainb = serialize_gainb_window(
            &obj_inputs.gain_a_records,
            &obj_inputs.band_activity,
            gainb_group_count,
        )?;
        debug_assert_eq!(gainb.bytes.len(), GAINB_LEN);

        // cfg: the shared window + the GHA IDSF predictor map for this channel.
        // Under GHA-inactive (`+0xd0 == 0`, selector <= 0x12: the 64/48 rates)
        // the extract's DisabledFallback writes `arena_root[0] = 0`, so
        // `calc_nbits_for_gha_at5` (native 0x1ff40, decompile 6811-6815) takes
        // its FIRST-statement early-out (`*piVar9 == 0 -> return 1`) BEFORE any
        // dispatch surface: it emits ZERO GHA writes — no cfg+0x11c IDSF
        // predictor map, no range_b GHA selector words. `compute_gha_packing_prep`
        // mirrors this by returning an empty `channels` vec, so the cfg map and
        // selector words stay unwritten exactly when the header is inactive.
        let mut cfg_ch = cfg.clone();
        if let Some(prep_ch) = prep.channels.get(ch) {
            serialize_gha_cfg_map(&mut cfg_ch, &prep_ch.compact_map)?;

            // GHA selectors into range_b (after the calc/side-data writes above).
            serialize_gha_selectors_range_b(&mut range_b, prep_ch)?;
        }

        // gha_p1 window + records.
        let mut gha_p1 = ObjectWindow::new(0, vec![0u8; GHA_P1_LEN]);
        serialize_gha_p1_window(
            &mut gha_p1,
            SYNTHETIC_ARENA_HEADER as u32,
            &prep.post_swap_channels[ch],
            &offsets[ch],
        )?;
        let gha_records = gha_channel_records_to_waves(&prep.post_swap_channels[ch]);

        group_objects.push(ObjectState {
            channel_index,
            range_a,
            range_b,
            cfg: cfg_ch,
            // Both channels reference the ch0 object (native `*(obj+0x28)`).
            previous_index: Some(0),
            gainb,
            gha_arena: gha_header.clone(),
            gha_p1,
            gha_records,
        });
    }

    Ok(FramePrepackerState {
        frame_bytes,
        block_count: COMPUTED_BLOCK_COUNT,
        groups: vec![BlockGroup {
            nblk,
            objects: group_objects,
        }],
    })
}

/// Parse `count` [`GainModeRow`](crate::encoder::packer_bridge::GainModeRow)s
/// out of a 16×38-word gain-A record buffer (stride 38 words): word 0 = point
/// count `n` (clamped to the native 7-point maximum), words `1 + k` = location,
/// words `8 + k` = level for `k < n`. Mirrors `parse_gain_rows` in the packer.
fn gain_mode_rows_from_records(
    records: &[u32],
    count: usize,
) -> Vec<crate::encoder::packer_bridge::GainModeRow> {
    (0..count)
        .map(|r| {
            let base = r * 38;
            let n = (records[base] as usize).min(7);
            let locations = (0..n).map(|k| records[base + 1 + k] as i32).collect();
            let levels = (0..n).map(|k| records[base + 8 + k] as i32).collect();
            crate::encoder::packer_bridge::GainModeRow {
                count: n,
                locations,
                levels,
            }
        })
        .collect()
}

/// The coding-side inputs assembled up to the calc ENTRY for one output frame
/// (returned by [`ComputedFrameDriver::assemble_calc_entry`]). The per-frame
/// packed `quant_unit_count` (`frame.channels[0].config_b0` = the zeroth
/// active-band trim capped by this frame's effective band extent) is already
/// fixed here; the calc/pack stages consume the rest without changing it.
struct ComputedCalcEntry {
    frame: CalcFrameEntry,
    init_headers: Vec<InitGainHeaderWords>,
    tone_primary_effective: Vec<i32>,
    zeroth_aux: ZerothBridgeFrameAux,
    zeroth_channel_aux: Vec<ZerothBridgeChannelAux>,
    gha_prep: GhaPackingPrep,
    effective_band_limit: u32,
    effective_band_count: u32,
}

/// One computed output frame: the packed 2048 bytes plus the pieces used to
/// build them (returned for test pins).
pub struct ComputedFrame {
    /// The packed 2048-byte ATRAC3plus frame.
    pub bytes: Vec<u8>,
    /// The assembled from-scratch prepacker state.
    pub state: FramePrepackerState,
    /// The computed calc output.
    pub calc_out: CalcFrameOutput,
}

/// Rolling computed driver (docs/11 §2.2 (b)). Owns the frontend rolling state
/// and the gain double-buffer roll; each output-bearing core call is computed
///
/// Per call: `frontend_core_call_at5` then `init_roll_step` (EVERY call — the
/// roll carries per-call). For output-bearing calls (7..=83) it additionally
/// builds the zeroth aux from the frontend, assembles the calc entry (with the
/// init header), runs calc, builds the from-scratch state, and packs.
pub struct ComputedFrameDriver {
    frontend: FrontendState,
    roll: GainRollState,
    /// Persistent object `+0x1c6f8` rows read by section 12 before its
    /// per-channel rewrite. Native priming leaves the five spectral words at
    /// the zero-amplitude sentinel 15 before the first packed frame.
    prior_level_words: Vec<Vec<i32>>,
    /// The per-rate coding params (selector / budget / frame_bytes) threaded into
    /// every output frame. 352 for [`new`](Self::new); per-rate for
    /// [`for_params`](Self::for_params) (docs/13 §1.1).
    params: CodingParams,
    /// The next core-call index this driver will process (starts at 0).
    next_core_call: u32,
}

/// The first output-bearing core call (native delay/priming: calls 0..6 produce
/// no output frame).
pub const FIRST_OUTPUT_CORE_CALL: u32 = 7;

impl Default for ComputedFrameDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputedFrameDriver {
    /// A fresh 352 driver with calloc-zero rolling state (matching the native
    /// handle-init start).
    pub fn new() -> Self {
        Self::for_params(CodingParams {
            selector: 30,
            budget: 16379,
            frame_bytes: COMPUTED_FRAME_BYTES as u32,
            // Stereo anchor (`handle+0x94` == 2 == COMPUTED_NBLK).
            channels: 2,
            mode_a: 2,
            band_index: crate::encoder::coding_params::FULL_BAND_INDEX,
            // selector 30 > 0x12 → GHA enabled (the 96-352 analysis arms).
            gha_enabled: true,
            // selector 30 > 0x12 → mode_cc set (the detector chain, 96-352).
            mode_cc: true,
        })
    }

    /// A fresh driver for an explicit per-rate [`CodingParams`] (docs/13 §1.1).
    /// The frontend selector is seeded from `params.selector` so the whole
    /// rolling front, the coding entries, the cfg window, and the packed frame
    /// size are per-rate. At the 352 params this equals [`new`](Self::new).
    pub fn for_params(params: CodingParams) -> Self {
        // Channel count is profile-driven (`params.channels`, docs/14 §0.4): 2
        // for the nine stereo rows (== FRONTEND_CHANNEL_COUNT, so every stereo
        // driver is seeded exactly as before), 1 for the five mono rows. The
        // all five mono coding paths — 128 kbps (docs/14 §1.3), 96 kbps (docs/14
        // §2.1), 64 kbps (docs/14 §3.1), 48 kbps (docs/14 §4.1), and 32 kbps
        // (docs/14 §5.1) — are landed, so a 1-channel driver IS stepped there.
        let channel_count = params.channels as usize;
        let mut frontend = FrontendState::new_zeroed_for_selector(channel_count, params.selector);
        // The per-call sigproc mode is `mode_a` (docs/13 §2.3): 2 at 320/352
        // (the default, so the frontend is unchanged), 3 at 48-256 (the mode-3
        // path surfaces the intensity band_count = 14 the joint-stereo producer
        // reads at 256).
        frontend.sigproc_mode = params.mode_a;
        // Per-rate band limit (`param_5` = band_index): 32 full-band, 29 at 192
        // (docs/13 §3.1). Drives the sigproc band-limit writeback → the
        // time2freq band extent + the `+0x1b48c` gain scan band_count.
        frontend.band_limit = params.band_index as i32;
        // Per-rate GHA enable (`cfg+0xd0`): false at 48/64 (disabled fallback),
        // true at 96-352 (analysis arms), threaded into the frontend extract
        // `header_0xd0_enabled` read (docs/13 §5.1).
        frontend.gha_enabled = params.gha_enabled;
        // Per-rate low-rate gain-detector mode (`cfg+0xcc`): true at 96-352 (the
        // `detect_gainc_data_new_at5` chain), false at 48/64 (the `set_gainc_at5`
        // dispatch), threaded into the frontend `time2freq_at5` mode_cc argument
        // (docs/13 §5.2). Defaults to true, so 96-352 are unchanged.
        frontend.mode_cc = params.mode_cc;
        ComputedFrameDriver {
            frontend,
            roll: GainRollState::new_zeroed(channel_count),
            prior_level_words: vec![vec![15; 8]; channel_count],
            params,
            next_core_call: 0,
        }
    }

    /// The next core-call index [`step`](Self::step) will process.
    pub fn next_core_call(&self) -> u32 {
        self.next_core_call
    }

    /// Advance one core call over a `[left, right]` pair of
    /// [`FRONTEND_FRAME_SAMPLES`](crate::encoder::frontend::FRONTEND_FRAME_SAMPLES)
    /// scalar (f32) samples. Runs the frontend + init roll (every call); for an
    /// output-bearing call (`>= FIRST_OUTPUT_CORE_CALL`) it computes and packs
    /// the output frame, returning `Some(frame)`; for a priming call it returns
    /// `None`.
    pub fn step(
        &mut self,
        inputs: [&[f32]; FRONTEND_CHANNEL_COUNT],
    ) -> Result<Option<ComputedFrame>, ComputedFrameError> {
        // Stereo wrapper: delegate to the channel-slice core (docs/14 §0.4). The
        // fixed-arity `[&[f32]; 2]` signature keeps every existing stereo call
        // site (`driver.step([&l, &r])`) compiling unchanged.
        self.step_channels(&inputs)
    }

    /// Channel-slice core of [`step`](Self::step) (docs/14 §0.4): advance one
    /// core call over `inputs.len()` channel windows (each
    /// [`FRONTEND_FRAME_SAMPLES`](crate::encoder::frontend::FRONTEND_FRAME_SAMPLES)
    /// scalar f32 samples). The frontend validates the channel count against its
    /// own `channel_count` per call. For a stereo driver `inputs.len() == 2`, so
    /// this is byte-identical to the previous `step` body. All five mono
    /// shipping paths (docs/14 §1.3/§2.1/§3.1/§4.1/§5.1) step a 1-channel driver
    /// here.
    pub fn step_channels(
        &mut self,
        inputs: &[&[f32]],
    ) -> Result<Option<ComputedFrame>, ComputedFrameError> {
        let core_call = self.next_core_call;
        let report = frontend_core_call_at5(&mut self.frontend, inputs, SYNTHETIC_ARENA_HEADER)?;
        // The init roll carries per-call; run it for EVERY call (priming too), so
        // the gain double-buffer is correct at every output call.
        let init_aux = init_roll_step(&mut self.roll, &self.frontend, &report)?;
        self.next_core_call += 1;

        if core_call < FIRST_OUTPUT_CORE_CALL {
            return Ok(None);
        }

        let frame = self.compute_output_frame(&report, &init_aux)?;
        Ok(Some(frame))
    }

    /// Advance one core call and return the per-frame packed `quant_unit_count`
    /// (cfg 0xb0 = the zeroth active-band trim capped by this frame's effective
    /// band extent, docs/13 §3.1 slice 3) WITHOUT running the calc/pack stages;
    /// `None` for a priming call. Rolls the frontend + gain double-buffer exactly
    /// like [`step`](Self::step).
    ///
    /// The returned value is byte-identical to the cfg 0xb0 word
    /// [`build_cfg_window`] writes into the packed state
    /// (`per_frame.active_b0 = frame.channels[0].config_b0`) — the exact word
    /// native captures at pack checkpoint 0x55f25 — but is read at the calc
    /// ENTRY, before the downstream calc dispatch. This lets the per-frame
    /// band-extent override oracle pin the QU count even on frames whose
    /// downstream calc takes an arm outside the proven live set: at 192,
    /// `syn_noise_fullscale` (whose frames never light the override) reaches
    /// the registered fail-explicit Phase-B joint-stereo-kill stub on two
    /// frames (first = output frame 59), where native's branch liveness shows
    /// its single over-budget frame (output frame 103) resolved within Phase A
    /// — a pre-existing 192 allocation divergence registered in docs/13
    /// Appendix B, a SEPARATE boundary from the extent override this slice
    /// ports. Every other discovery synthetic (incl. the all-override
    /// `syn_tone_997` and mixed `syn_sweep_log`) completes the FULL pipeline.
    pub fn step_qu_count(
        &mut self,
        inputs: [&[f32]; FRONTEND_CHANNEL_COUNT],
    ) -> Result<Option<u32>, ComputedFrameError> {
        let core_call = self.next_core_call;
        let report = frontend_core_call_at5(&mut self.frontend, &inputs, SYNTHETIC_ARENA_HEADER)?;
        let init_aux = init_roll_step(&mut self.roll, &self.frontend, &report)?;
        self.next_core_call += 1;

        if core_call < FIRST_OUTPUT_CORE_CALL {
            return Ok(None);
        }

        let entry = self.assemble_calc_entry(&report, &init_aux)?;
        Ok(Some(entry.frame.channels[0].config_b0))
    }

    /// Assemble the coding-side inputs for one output-bearing core call, UP TO
    /// AND INCLUDING the calc ENTRY (the zeroth output). This is the point at
    /// which the per-frame packed `quant_unit_count` (cfg 0xb0 = the zeroth
    /// active-band trim capped by this frame's effective band extent) is fixed;
    /// the downstream calc/pack stages do not change it. Shared by
    /// [`compute_output_frame`](Self::compute_output_frame) and
    /// [`step_qu_count`](Self::step_qu_count).
    fn assemble_calc_entry(
        &self,
        report: &FrontendCoreCallReport,
        init_aux: &[crate::encoder::coding_bridge::CodingBridgeChannelAux],
    ) -> Result<ComputedCalcEntry, ComputedFrameError> {
        // Zeroth aux entirely from the rolling frontend.
        let (primary, secondary) = zeroth_tone_activity_from_frontend(&self.frontend);
        let (tone_primary_words, tone_secondary_words, tone_flag_25) =
            zeroth_tone_words_from_frontend(&self.frontend);
        let gha_prep = gha_packing_prep_from_frontend(&self.frontend)?;
        let zeroth_aux = ZerothBridgeFrameAux {
            primary_tone_activity: primary,
            secondary_tone_activity: secondary,
            tone_primary_words,
            tone_secondary_words,
            tone_flag_25,
            gha_bits: gha_prep.total_bits as i16,
        };
        let zeroth_channel_aux = zeroth_band_activity_from_frontend(report)?;

        // Live config flag word (`cfg+0x1dc`) as it stands after this core
        // call's frontend ran (post shell shift, post extract mask-1 setter):
        // the sine-mode hysteresis word the coding-stage consumers read
        // (docs/12 §4.3 b-residual). Was hardcoded 0.
        let flags_1dc = self.frontend.sigproc.header_flag_word;
        // Per-frame effective band extent (docs/13 §3.1 slice 3). The sigproc
        // shell epilogue's `cfg+0x1dc & 0x7c` override forces the band limit to
        // 0x20 on flagged frames; `report.sigproc.writeback` is the ported
        // epilogue's POST-override fan-out (band_limit → cfg+0xb4, band_count →
        // cfg+0xbc, every channel's `+0x1b48c` gain cap; decompile 43502-43527).
        // The coding stages read THIS per-frame value, not the static per-rate
        // `self.params.band_index`. At band_index 32 (256/320/352) the override
        // is invisible — effective == static 32/16 on every frame — so those
        // rates stay byte-identical; at 192 it flips 29↔32 per frame.
        let effective_band_limit = report.sigproc.writeback.band_limit as u32;
        let effective_band_count = report.sigproc.writeback.band_count;
        let (frame, init_headers, tone_primary_effective) =
            assemble_calc_frame_entry_with_init_for_params_at5(
                report,
                init_aux,
                &zeroth_aux,
                &zeroth_channel_aux,
                flags_1dc,
                self.params.selector,
                self.params.budget,
                // Joint-stereo producer gate + the rolling frontend it reads its
                // masking inputs from (docs/13 §2.3). LIVE only at mode_a == 3.
                self.params.mode_a,
                // Per-FRAME effective band extent (`cfg+0xb4` band_index post
                // override; docs/13 §3.1 slice 3).
                effective_band_limit,
                Some(&self.frontend),
            )?;
        Ok(ComputedCalcEntry {
            frame,
            init_headers,
            tone_primary_effective,
            zeroth_aux,
            zeroth_channel_aux,
            gha_prep,
            effective_band_limit,
            effective_band_count,
        })
    }

    /// Assemble + pack one output-bearing frame from the current rolling state
    /// (docs/11 §2.2 (b), the pure-computed recipe of
    /// `tests/composed_frame.rs::computed_calc_call7` MINUS the captured-spectrum
    /// / caveat-band patches).
    fn compute_output_frame(
        &mut self,
        report: &FrontendCoreCallReport,
        init_aux: &[crate::encoder::coding_bridge::CodingBridgeChannelAux],
    ) -> Result<ComputedFrame, ComputedFrameError> {
        let ComputedCalcEntry {
            mut frame,
            init_headers,
            tone_primary_effective,
            zeroth_aux,
            zeroth_channel_aux,
            gha_prep,
            effective_band_limit,
            effective_band_count,
        } = self.assemble_calc_entry(report, init_aux)?;
        frame.prior_level_words.clone_from(&self.prior_level_words);
        let out = calc_channel_block_frame_at5(&frame).map_err(ComputedFrameError::Calc)?;
        self.prior_level_words = out
            .channels
            .iter()
            .map(|channel| channel.o_1c6f8.clone())
            .collect();

        // Per-frame cfg inputs from the computed entry/output + the computed tone
        // summaries (docs/11 §2.2 (a)3 — NEW vs the capstone, which hardcoded the
        // stereo groups to zero).
        let per_frame = CfgPerFrame352 {
            active_b0: frame.channels[0].config_b0,
            level_groups_c0: frame.channels[0].config_c0,
            stereo_group1: stereo_group_at5(
                &zeroth_aux.tone_secondary_words,
                frame.channels[0].config_c0 as usize,
            )?,
            // Group 2 from the bridge's effective tone-primary words: at
            // mode_a == 3 these are the joint-stereo producer's per-band
            // `band_join` (native `cfg[0x08+k*4]` writer, docs/13 §2.3); at
            // mode_a == 2 (320/352) they equal `zeroth_aux.tone_primary_words`
            // (init's all-zero row), so those rates stay byte-identical.
            stereo_group2: stereo_group_at5(
                &tone_primary_effective,
                frame.channels[0].config_c0 as usize,
            )?,
            bits_1e4: out.ctx_field_1e4,
        };

        // Per-object inputs: gain-A records + band activity from the frontend.
        let channels_t2f = report
            .time2freq
            .as_ref()
            .ok_or(ComputedFrameError::Coding(CodingBridgeError::NoTime2Freq))?;
        // Object count is profile-driven (`params.channels`, docs/14 §0.4): 2 for
        // the nine stereo rows (== COMPUTED_NBLK, so byte-identical), 1 for the
        // five mono rows. All five mono rows — 128 kbps (docs/14 §1.3), 96 kbps
        // (docs/14 §2.1), 64 kbps (docs/14 §3.1), 48 kbps (docs/14 §4.1), and
        // 32 kbps (docs/14 §5.1) — reach here with a single object.
        let channel_count = self.params.channels as usize;
        let mut objects = Vec::with_capacity(channel_count);
        for ch in 0..channel_count {
            let gain_a_records = assemble_gain_a_records(ch, &channels_t2f[ch])?;
            objects.push(ComputedObjectInputs {
                init_header: init_headers[ch],
                gain_a_records,
                band_activity: zeroth_channel_aux[ch].band_activity.clone(),
            });
        }

        let state = build_computed_prepacker_state_for_params(
            &out,
            &per_frame,
            &self.frontend,
            &gha_prep,
            &objects,
            self.params.selector,
            self.params.budget,
            self.params.frame_bytes as usize,
            // Per-FRAME effective band extent (cfg+0xb4/+0xbc observation words;
            // docs/13 §3.1 slice 3). These are packer-unread, but native truth is
            // the per-frame shell-written values, not the static per-rate ones.
            effective_band_limit,
            effective_band_count,
        )?;

        let syntax = FrameSyntax::from_reference(&state)?;
        debug_assert_eq!(syntax.frame_bytes(), state.frame_bytes);
        debug_assert_eq!(syntax.groups().len(), state.groups.len());
        debug_assert_eq!(syntax.to_reference()?, state);

        let mut bytes = vec![0u8; state.frame_bytes];
        let mut writer = BitWriter::new(&mut bytes);
        pack_frame_at5(&state, &mut writer).map_err(ComputedFrameError::Pack)?;

        Ok(ComputedFrame {
            bytes,
            state,
            calc_out: out,
        })
    }
}

/// Derive a cfg stereo-config side-data group `(head, inner, [words; 16])` from a
/// 16-entry tone-word row (docs/11 §2.2 (a)3). The head/inner are the ported
/// [`zeroth_activity_summary_at5`] `any`/`partial` flags over native's active
/// `cfg+0xc0` prefix; the per-band words retain the full storage row.
///
/// Orchestrator-verified captured shape at 352 (mode_a == 2): group-1 (from the
/// secondary/swap words) is nonzero only on core calls 59/60 (word 14 = 1 → sum
/// 1 of 16 → head/inner (1,1)) and 66 (words 5,6 = 1 → sum 2 → (1,1)); group-2
/// (primary words, all-zero at 352) is zero everywhere → sum 0 → (0,0).
///
/// At mode_a == 3 (48-256 kbps) the native zeroth `param_5 == 3` arm writes its
/// per-band `band_join` directly into the group-2 words `cfg[0x08+k*4]`
/// (decompile 36150-36221); the bridge routes those into
/// `tone_primary_effective`, so on 256 antiphase all 16 words are 1 → sum 16 →
#[doc(hidden)]
pub fn stereo_group_at5(
    words: &[i32],
    active_count: usize,
) -> Result<(u32, u32, [u32; 16]), ComputedFrameError> {
    let summary: ZerothActivitySummary =
        zeroth_activity_summary_at5(words, active_count).map_err(PackerBridgeError::from)?;
    let mut k = [0u32; 16];
    for (i, slot) in k.iter_mut().enumerate() {
        *slot = words.get(i).copied().unwrap_or(0) as u32;
    }
    Ok((summary.any_flag, summary.partial_flag, k))
}

/// Ties `ATRAC3PLUS_352` into the module's constant set so a scope-widening
/// caller cannot silently pass a different profile (the geometry constants above
/// are 352-only; assert the profile still names the scoped stereo path).
const _: () = {
    assert!(ATRAC3PLUS_352.channels() as usize == COMPUTED_NBLK);
    assert!(!GHA_HAS_PREVIOUS_352[0] && GHA_HAS_PREVIOUS_352[1]);
};
