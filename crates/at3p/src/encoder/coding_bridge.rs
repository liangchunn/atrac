//! PCM->coding-init bridge (docs/09 Phase 3 Slice D).
//!
//! Assembles the coding stage's `init_channel_block_at5` entry surface
//! (`InitFrameState`) from the computed encoder front (Slice C:
//! `frontend_core_call_at5` -> `time2freq_at5`). This is a pure assembler: it
//! routes the frontend's per-channel MDCT surfaces and detector gain records
//! into the shape `init_channel_block_frame_at5` consumes, and sources the
//! remaining entry inputs the frontend does not (yet) produce from a
//! caller-supplied auxiliary (`CodingBridgeChannelAux`, trace-fed in the test).
//!
//! ## Pinned native facts (against `init_cb_io_trace.ndjson`, core call 7)
//!
//! * **Spectrum A/B assignment (pinned empirically, not guessed).** The init
//!   entry's `spec_a` (edx, native `init_channel_block_at5` arg param_2) is the
//!   frontend's **delayed** MDCT surface (`Time2FreqChannelOutput.delayed_out`),
//!   and `spec_b` (ecx, param_3) is the frontend's **main** MDCT surface
//!   (`Time2FreqChannelOutput.spectra`). Verified: on core call 7, computed
//!   `delayed_out` reproduces the captured `spec_a` to a whole-buffer max abs of
//!   ~6e-4 (effectively bit-exact), and computed `spectra` reproduces the
//!   captured `spec_b` within the Phase-0 tolerance (`FloatTolerance::new(0.01,
//!   0.001)`) on every band except the three documented post-detector
//!   injection/merge caveat bands (ch0 band10, ch1 band9, ch1 band10 — docs/05
//!   §4.2 rider), where the upstream frontend surface itself is not yet
//!   bit-exact. The `time2freq_assembly_trace` handoff pins the same routing:
//!   its `spectrum_a == delayed_out` and `spectrum_b == spectra` byte-for-byte
//!   on the clean bands, and those match the init entry's `spec_a`/`spec_b`.
//!   The 2048-float buffer is shared: init re-partitions the same coefficients
//!   into 32 scale-factor bands via `isps`, while time2freq lays them out as 16
//!   QMF bands x 128; the flat 2048 order is identical.
//!
//! * **Gain records (38-word) — point prefix computed, float tail zeroed
//!   (unread).** Init reads only words 0..14 of each 38-word gain record (the
//!   point count at word 0, the +0x4 locations at words 1..7, and the +0x20
//!   level ids at words 8..14; `gain_spread_over_3` / `classify_gain_records` in
//!   `src/coding/init_block.rs` touch nothing past word 14). That 15-word point
//!   prefix is the `time2freq_at5` exit record (`final_records`), which the
//!   frontend computes byte-exact: `output.final_records[band]` equals the
//!   captured `gain_a_records[band][0..15]` for all 16 bands x 2 channels on
//!   core call 7. Init reads native's ONE in-place record buffer `obj+0x8/0xc`,
//!   which the post-detector gain-record harmonization MUTATES after detection;
//!   `final_records` is that post-mutation buffer. On the 352 path (and every
//!   rate whose harmonization gate is closed) it equals the raw detector return
//!   `detector_outcomes[band].compact_point_words` by construction, so the two
//!   agree byte-for-byte there; at a first-live gate rate (96 kbps) they differ
//!   and `final_records` is the native-correct source. Words 15..37 are the
//!   gain-window level/envelope floats (e.g. word 21 = 1.0f, word 25/31 =
//!   computed levels); the port's `gain_detect_compact_record_from_emit_chain_at5`
//!   leaves them zero, and — crucially — **no consumer in the encode path ever
//!   reads them** (see the read-set inventory on
//!   [`GainRollState`]). The from-PCM pipeline therefore builds every 38-word
//!   record as `[15-word point prefix ++ 23 zero words]`; the boundary replay
//!   tests still trace-feed the captured tails via
//!   `CodingBridgeChannelAux::gain_a_record_tails` (correct test discipline).
//!
//! * **gainB / gain_b_records — the per-channel gain double-buffer carry.** The
//!   claim "at 352 the second gain array (attack/gainB) is empty" is only a
//!   call-7-specific observation, **not** a general truth: gainB carries the
//!   PREVIOUS call's COMPLETE gainA content (records AND the `+0x9c8`/`+0xa48`
//!   fields), and that content is nonzero at calls 12/59/60/66. The mechanism is
//!   the head-of-`at5enc_sigproc` double-buffer swap. Per call, per channel
//!   object entry (`at5enc_sigproc`, decompile fn at line 42766; Ghidra
//!   `0x5f2b0` = native `0x4f2b0`, Ghidra = native + `0x10000`; swap at decompile
//!   42960–42964):
//!
//!   ```text
//!   iVar17 = *(int *)(iVar12 + iVar9 * 4);      // object entry
//!   uVar7  = *(undefined4 *)(iVar17 + 0xc);
//!   *(undefined4 *)(iVar17 + 0xc) = *(undefined4 *)(iVar17 + 8);
//!   *(undefined4 *)(iVar17 + 8) = uVar7;        // SWAP obj+0x8 <-> obj+0xc
//!   ```
//!
//!   This sits in the SAME head loop as the GHA-arena 5-slot ring rotation and
//!   the detector-history memmoves, and runs BEFORE the detector writes fresh
//!   records into the (new) A buffer and before init writes A's `+0x9c8`/`+0xa48`.
//!   Consequence at call N: gainB (obj+0xc) holds call N−1's complete gainA
//!   content. Verified byte-exact against `init_cb_io_trace.ndjson`
//!   (calls 59→60 consecutive): `gain_b_records(60) == gain_a_records(59)`
//!   (all 608 words, both channels), `entry b_9c8(60) == return a_9c8(59)`, and
//!   `entry b_a48(60) == return a_a48(59)` — the roll is exactly init's A-side
//!   write of the previous call, seen through the swap. At call 0 the calloc-zero
//!   start gives `b_9c8 = [0;32]`, `b_a48 = [0;32]`, all-zero gain_b prefixes; at
//!   call 7 (init over all-zero call-6 spectra) `b_9c8 = [0;32]`, `b_a48 =
//!   [1.0;32]`. The from-PCM pipeline models this carry with [`GainRollState`]
//!   (rolled forward by [`advance_gain_roll`]); the boundary replay tests still
//!   trace-feed the captured values via the aux.
//!
//! * **Config / side scalars (computed 352 constants).** `channel_count = 2`
//!   (`ATRAC3PLUS_352.channels()`), `selector = 30`, `band_count = 32`,
//!   `gain_band_count = 16`, `flags_1dc = 0`, `sr_ac = 44100`
//!   (`ATRAC3PLUS_352.sample_rate()`) are frame-invariant 352 config computed here
//!   and asserted byte-exact vs the captured `side_fields`. `join_flags_50`
//!   (init's per-group joint-stereo flags, cfg row `+0x50..+0x90`) is NOT
//!   frame-invariant: `assemble_init_frame_state_at5` seeds it `[0;16]` (correct
//!   through core call 58), but `assemble_calc_frame_entry_at5` overrides it
//!   from the computed `ZerothBridgeFrameAux::tone_secondary_words` (the SAME
//!   cfg memory, the one-call-delayed stereo swap flags); [`init_roll_step`]
//!   applies the same override for the rolling per-call init runs. The captured
//!   prepacker cfg blobs show group-1 words nonzero at core calls 59/60/66,
//!   where the init joint-stereo spectrum SWAP branch is LIVE (ported in place
//!   by slice 2.1e, `src/coding/init_block.rs`, decompile 34711–34766).
//!   The per-channel object-side scalars `objside_1c = 0`, `objside_14 = 0`,
//!   and init's 352-scoped `y_index = 16` are verified at all six captured
//!   calls (0/7/12/59/60/66), so the init aux emits them as constants. The
//!   computed calc entry separately reads per-rate sigproc detector word 0
//!   (10/11/14/16 at selectors 24/25/27/30). The native heap pointers
//!   `objside_ptr` / `spec_b_ptr`
//!   (which only feed the init OUTPUT pointer-identity fields
//!   `block_100c`/`block_1010`, and have no `src/` consumer) are emitted as named
//!   synthetic identity-only constants ([`SYNTHETIC_OBJSIDE_PTR`] /
//!   [`SYNTHETIC_SPEC_B_PTR`]). The boundary replay tests still trace-feed the
//!   real captured values via the aux.
//!
//! ## Gain-record float tail (words 15..37) — zeroed, proven unread
//!
//! The gain-window level/envelope float tail is intentionally NOT derived. It is
//! unread by every consumer, so the from-PCM path zeroes it. Read-set inventory
//! (each consumer proven to touch only words 0..16):
//!   * init: `gain_spread_over_3` reads words 0, 8..8+count of records 0..1;
//!     last-nonzero scan reads word 0; the duplicate fold reads words 0, 1.., 8..
//!     — all within words 0..14 (`src/coding/init_block.rs`).
//!   * zeroth: `zeroth_gain_records_from_records` reads words 0..16 (A only).
//!   * calc: `gain_rows_from_records` reads words 0, 8..16 (both arrays).
//!   * packer: proven unread by
//!     `tests/composed_frame.rs::zeroing_gain_record_float_tails_does_not_change_packed_bytes`.
//!   * detector: the rolling detector takes no record-buffer input and replays
//!     bit-exact across all 84 calls (slice 2.1a), so it does not consume it.
//! The gain-tail DERIVATION (`gainc_window_enc_at5`) stays UNPORTED: the tail is
//! zeros by this read-set argument, not by a guessed computation.

use crate::coding::allocation::{AllocationError, ZerothGhaChannelFlags};
use crate::coding::calc_block::{CalcChannelEntry, CalcFrameEntry, CalcGainRow};
use crate::coding::init_block::{
    InitChannelState, InitFrameOutput, InitFrameState, init_channel_block_frame_at5,
    init_high_frequency_cut_gate_open, init_high_frequency_cut_start_at5,
};
use crate::coding::normalize::{
    NormalizeError, clip_normalized_mdspec_at5, norm_channel_difference_idsf_at5,
    normalize_mdspec_at5, normalize_mdspec_average_at5,
};
use crate::coding::zeroth_pass::{
    JointStereoChannelInput, JointStereoProducerInput, JointStereoProducerOutput, ZEROTH_BANDS_AT5,
    ZerothChannelState, ZerothFrameState, ZerothGainRecord, ZerothIdsfInput, ZerothPassError,
    ZerothQuantBandRaw, zeroth_bit_allocation_frame_at5, zeroth_joint_stereo_producer_at5,
};
use crate::dsp::sigproc::GAIN_DETECT_POINT_WORDS;
use crate::dsp::sigproc_shell::{
    SIGPROC_DETECTOR_ROW_A_POWER_HISTORY0, SIGPROC_DETECTOR_ROW_B_POWER_HISTORY0,
};
use crate::dsp::time2freq::Time2FreqChannelOutput;
use crate::encoder::cfg_bridge::cfg_shape_count_b8;
use crate::encoder::frontend::{FRONTEND_BAND_COUNT, FrontendCoreCallReport, FrontendState};
use crate::encoder::profile::ATRAC3PLUS_352;
use crate::tables::at5::{isps_at5, nsps_at5, y_at5};

/// Flat MDCT spectrum length init consumes (16 QMF bands x 128 == 2048).
pub const CODING_BRIDGE_SPECTRUM_WORDS: usize = 2048;
/// 38-word gain-record layout (`GAIN_DETECT_RECORD_WORDS`).
pub const CODING_BRIDGE_GAIN_RECORD_WORDS: usize = 38;
/// 15-word point prefix init reads (`TIME2FREQ_POINT_WORDS`).
pub const CODING_BRIDGE_POINT_WORDS: usize = GAIN_DETECT_POINT_WORDS;
/// Gain-record float-tail width (words 15..37), the CUT surface.
pub const CODING_BRIDGE_GAIN_TAIL_WORDS: usize =
    CODING_BRIDGE_GAIN_RECORD_WORDS - CODING_BRIDGE_POINT_WORDS;
/// Gain-record band count (`side +0xbc`, init `gain_band_count`).
pub const CODING_BRIDGE_GAIN_BAND_COUNT: usize = FRONTEND_BAND_COUNT;
/// Init scale-factor band count (`side +0xb4`, init `band_count`).
pub const CODING_BRIDGE_INIT_BAND_COUNT: usize = 32;
/// Aux seed length for `b_9c8`/`b_a48` (init reads 32).
pub const CODING_BRIDGE_SEED_WORDS: usize = 32;

/// Block selector (`cfg+0x1e8`) for the 352 path (docs/06: selector 30). The
/// live path threads the per-rate selector (29 at 320, docs/13 §1.1) via
/// `assemble_init_frame_state_with_selector_at5` /
/// `assemble_calc_frame_entry_with_init_for_params_at5`; this constant is the
/// 352 value the wrappers and boundary-replay tests use. NOTE: this is the
/// bitrate selector, distinct from [`CALC_BRIDGE_ENCODE_SELECTOR`] (the shared
/// `sa_nencodetbls` index, rate-independent 1).
pub const CODING_BRIDGE_SELECTOR: i32 = 30;
/// Side `+0x1dc` flags (352: no joint-stereo scaling).
pub const CODING_BRIDGE_FLAGS_1DC: u32 = 0;

/// Synthetic identity-only object-side pointer (`*(objentry+0x30)`), fed to the
/// from-PCM init entry as `objside_ptr`. Native routes it to the init OUTPUT
/// pointer-identity field `block_100c` only; no `src/` consumer reads it (only
/// the boundary replay tests pin the real captured value). Follows the
/// `SYNTHETIC_ARENA_HEADER = 0x1000_0000` precedent.
pub const SYNTHETIC_OBJSIDE_PTR: u32 = 0x1000_0000;
/// Synthetic identity-only spectrum-B pointer, fed to the from-PCM init entry as
/// `spec_b_ptr`. Native routes it to the init OUTPUT pointer-identity field
/// `block_1010` only; no `src/` consumer reads it. Identity-only, unread
/// downstream (see [`SYNTHETIC_OBJSIDE_PTR`]).
pub const SYNTHETIC_SPEC_B_PTR: u32 = 0x1000_0010;

#[derive(Debug, Clone, PartialEq)]
pub enum CodingBridgeError {
    /// The frontend report carried no `time2freq` output (GHA/detector gate was
    /// closed — never true for the always-open 352 gate).
    NoTime2Freq,
    /// Channel-count mismatch between the frontend output and the aux.
    ChannelCount { time2freq: usize, aux: usize },
    /// A detector band emitted an all-zero placeholder point record
    /// (`prune_blocked`); the point prefix is not native-valid, so refuse to
    /// assemble rather than emit an unverified record.
    PruneBlocked { channel: usize, band: usize },
    /// An input surface had the wrong length.
    ShapeMismatch {
        field: &'static str,
        channel: usize,
        expected: usize,
        actual: usize,
    },
    /// `init_channel_block_frame_at5` rejected the Slice-D-assembled init state
    /// (an out-of-scope native gate opened). Detail is on the wrapped debug.
    InitBlock,
    /// A `norm_channel_block_at5` sub-function rejected the normalized surface.
    Normalize(NormalizeError),
    /// A zeroth-pass weight/round helper rejected the surface.
    Allocation(AllocationError),
    /// The composed `zeroth_bit_allocation_frame_at5` pass rejected the
    /// assembled zeroth-entry surface (an out-of-scope native gate opened,
    /// or a leaf failed). Detail is on the wrapped debug.
    Zeroth(ZerothPassError),
    /// `mode_a == 3` selected the joint-stereo producer arm, but no rolling
    /// [`FrontendState`] was threaded to source its masking inputs from. Only
    /// reachable via a programming error — the live per-rate driver always
    /// supplies its frontend, and the mode-2 (352/320) wrappers never take
    /// this arm.
    JointStereoMissingFrontend,
}

impl From<NormalizeError> for CodingBridgeError {
    fn from(error: NormalizeError) -> Self {
        CodingBridgeError::Normalize(error)
    }
}

impl From<AllocationError> for CodingBridgeError {
    fn from(error: AllocationError) -> Self {
        CodingBridgeError::Allocation(error)
    }
}

impl From<ZerothPassError> for CodingBridgeError {
    fn from(error: ZerothPassError) -> Self {
        CodingBridgeError::Zeroth(error)
    }
}

/// Per-channel entry inputs the frontend does not (yet) produce — trace-fed in
/// the Slice D test from the captured `init_cb_io_trace` entry. Each field's
/// native source and status (352-invariant vs CUT) is documented on the module.
#[derive(Debug, Clone)]
pub struct CodingBridgeChannelAux {
    /// `*(objentry+0x30)+0x1c` — 352-invariant (0); verified vs capture.
    pub objside_1c: i32,
    /// `*(objentry+0x30)+0x14` — 352-invariant (0); verified vs capture.
    pub objside_14: i32,
    /// `*(objentry+0x30)` native heap pointer -> block `+0x100c`. Identity-only
    /// (no `src/` consumer). From-PCM: [`SYNTHETIC_OBJSIDE_PTR`]; trace-fed in
    /// the boundary replay tests.
    pub objside_ptr: u32,
    /// Spectrum-B native pointer -> block `+0x1010`. Identity-only (no `src/`
    /// consumer). From-PCM: [`SYNTHETIC_SPEC_B_PTR`]; trace-fed in the tests.
    pub spec_b_ptr: u32,
    /// `g_a_y_at5` index `*(*(objentry+0x10))` — 352 init-boundary aux value
    /// (16); trace-fed for boundary replay. The computed calc-entry bridge
    /// separately reads the live per-rate sigproc detector word 0.
    pub y_index: i32,
    /// gainB `+0x9c8` word-length seed (32) — the PREVIOUS call's gainA `+0x9c8`
    /// (init's A-side write, seen through the head-of-frame double-buffer swap).
    /// From-PCM: [`GainRollState::a_9c8`]; trace-fed in the boundary replay tests.
    pub b_9c8: Vec<i32>,
    /// gainB `+0xa48` weight seed (32) — the PREVIOUS call's gainA `+0xa48`
    /// (see `b_9c8`). From-PCM: [`GainRollState::a_a48`]; trace-fed in the tests.
    pub b_a48: Vec<f32>,
    /// Gain-record A float tails (words 15..37), one per gain band (16). Unread
    /// by every consumer (read-set inventory on [`GainRollState`]). From-PCM:
    /// all-zero; trace-fed from the captured entry in the boundary replay tests.
    pub gain_a_record_tails: Vec<[u32; CODING_BRIDGE_GAIN_TAIL_WORDS]>,
    /// Gain array B records (16 x 38 words) — the PREVIOUS call's gainA records
    /// (the double-buffer carry), with unread float tails. From-PCM:
    /// [`GainRollState::records`]; trace-fed in the boundary replay tests.
    pub gain_b_records: Vec<u32>,
}

/// Assemble the computed `InitFrameState` at the current core call from the
/// frontend report plus the per-channel trace-fed aux.
///
/// Computed here: the two per-channel MDCT spectra (routed delayed->spec_a,
/// main->spec_b), the 15-word point prefix of every gain-A record (from the
/// detector return), and the frame-invariant 352 config/side scalars. Trace-fed
/// from `aux`: the 23-word gain-record float tails (CUT), the empty gainB
/// records and gainB seeds, the object-side pointers, and the 352-invariant
/// object-side scalars (which are also asserted vs capture in the test).
pub fn assemble_init_frame_state_at5(
    report: &FrontendCoreCallReport,
    aux: &[CodingBridgeChannelAux],
    flags_1dc: u32,
) -> Result<InitFrameState, CodingBridgeError> {
    // 352 wrapper: the block selector is 30, full-band gain scan (16). The live
    // per-rate path routes through `assemble_init_frame_state_with_selector_at5`.
    assemble_init_frame_state_with_selector_at5(
        report,
        aux,
        flags_1dc,
        CODING_BRIDGE_SELECTOR,
        CODING_BRIDGE_GAIN_BAND_COUNT,
        // 352 is full-band: the HF-cut gate is closed (extent 32 not in
        // 0x18..0x20), so the per-frame extent is the static 32.
        CODING_BRIDGE_INIT_BAND_COUNT,
    )
}

/// Like [`assemble_init_frame_state_at5`] but with an explicit per-rate block
/// selector (`cfg+0x1e8`; 30 at 352, 29 at 320 — docs/13 §1.1). Every
/// selector-dependent init value read is identical at 29 vs 30 (evidence 5), so
/// this is parameter threading; the selector still lands in
/// `InitFrameState.selector` to stay observation-honest.
pub fn assemble_init_frame_state_with_selector_at5(
    report: &FrontendCoreCallReport,
    aux: &[CodingBridgeChannelAux],
    flags_1dc: u32,
    selector: i32,
    gain_band_count: usize,
    effective_band_limit: usize,
) -> Result<InitFrameState, CodingBridgeError> {
    let channels_t2f = report
        .time2freq
        .as_ref()
        .ok_or(CodingBridgeError::NoTime2Freq)?;
    if channels_t2f.len() != aux.len() {
        return Err(CodingBridgeError::ChannelCount {
            time2freq: channels_t2f.len(),
            aux: aux.len(),
        });
    }

    let channel_count = channels_t2f.len();
    let mut channels = Vec::with_capacity(channel_count);
    for (index, (output, channel_aux)) in channels_t2f.iter().zip(aux).enumerate() {
        channels.push(assemble_channel(
            index,
            output,
            channel_aux,
            gain_band_count,
        )?);
    }

    Ok(InitFrameState {
        channels,
        channel_count,
        selector,
        band_count: CODING_BRIDGE_INIT_BAND_COUNT,
        // PER-FRAME effective band extent (`cfg+0xb4`, post the sigproc
        // `+0x1dc & 0x7c` override). Distinct from `band_count`: the fixed
        // 32-wide processing extent stays 32, but the high-frequency spectral
        // cut gate (decompile 34685) reads THIS per-frame value. The live driver
        // threads `report.sigproc.writeback.band_limit`; at the full-band rates
        // this equals 32 so the cut gate stays closed (byte-identical).
        extent_b4: effective_band_limit,
        // Per-FRAME effective gain scan bound (`+0x1b48c` seed =
        // `g_a_x_at5[effective_band_limit]+1`): 16 full-band; 13 (no override) or
        // 16 (override) per frame at 192 (docs/13 §3.1 slice 3). init's
        // `classify_gain_records` scans down from this over the 16-wide detector
        // gain records. The live driver threads this call's post-override value.
        gain_band_count,
        // Live config flag word (`cfg+0x1dc`) as it stands when the coding
        // stages run (post-shift, post-setter). Was hardcoded
        // `CODING_BRIDGE_FLAGS_1DC = 0`; the sine-mode hysteresis (docs/12
        // §4.3 b-residual) makes it nonzero on mask-1 frames.
        flags_1dc,
        sr_ac: ATRAC3PLUS_352.sample_rate(),
        join_flags_50: vec![0i32; CODING_BRIDGE_GAIN_BAND_COUNT],
    })
}

fn assemble_channel(
    channel: usize,
    output: &Time2FreqChannelOutput,
    aux: &CodingBridgeChannelAux,
    gain_band_count: usize,
) -> Result<InitChannelState, CodingBridgeError> {
    check_len(
        "spectra",
        channel,
        output.spectra.len(),
        CODING_BRIDGE_SPECTRUM_WORDS,
    )?;
    check_len(
        "delayed_out",
        channel,
        output.delayed_out.len(),
        CODING_BRIDGE_SPECTRUM_WORDS,
    )?;
    // A reduced-band rate yields `band_count` (13 at 192) detector outcomes; the
    // 16-wide gain buffer's tail is zero-filled (docs/13 §3.1). Reject only a
    // count that overflows the buffer.
    if output.detector_outcomes.len() > CODING_BRIDGE_GAIN_BAND_COUNT {
        return Err(CodingBridgeError::ShapeMismatch {
            field: "detector_outcomes",
            channel,
            expected: CODING_BRIDGE_GAIN_BAND_COUNT,
            actual: output.detector_outcomes.len(),
        });
    }
    if gain_band_count > CODING_BRIDGE_GAIN_BAND_COUNT {
        return Err(CodingBridgeError::ShapeMismatch {
            field: "detector_outcomes",
            channel,
            expected: CODING_BRIDGE_GAIN_BAND_COUNT,
            actual: gain_band_count,
        });
    }
    check_len("b_9c8", channel, aux.b_9c8.len(), CODING_BRIDGE_SEED_WORDS)?;
    check_len("b_a48", channel, aux.b_a48.len(), CODING_BRIDGE_SEED_WORDS)?;
    check_len(
        "gain_a_record_tails",
        channel,
        aux.gain_a_record_tails.len(),
        CODING_BRIDGE_GAIN_BAND_COUNT,
    )?;
    check_len(
        "gain_b_records",
        channel,
        aux.gain_b_records.len(),
        CODING_BRIDGE_GAIN_BAND_COUNT * CODING_BRIDGE_GAIN_RECORD_WORDS,
    )?;

    // Gain array A: computed 15-word point prefix (`assemble_gain_a_records`)
    // with the trace-fed 23-word tail overwritten onto it (boundary replay
    // discipline). From-PCM the tails are all-zero, so the copy is a no-op.
    //
    // mode_cc==0 (64/48): `assemble_gain_a_records` already returned the full
    // native plane rows (word-15 flag + real float tail). Do NOT overwrite the
    // tail — the plane tail IS the native obj+0x8 content. The tail overwrite is
    // the mode_cc==1 prefix path only (trace-fed boundary replay at 352 stays
    // identical).
    let mut gain_a_records =
        assemble_gain_a_records_with_band_count(channel, output, gain_band_count)?;
    if output.final_plane_rows.is_none() {
        for band in 0..CODING_BRIDGE_GAIN_BAND_COUNT {
            let base = band * CODING_BRIDGE_GAIN_RECORD_WORDS;
            // Words 15..37: the gain-window float tail (trace-fed; see aux doc).
            gain_a_records
                [base + CODING_BRIDGE_POINT_WORDS..base + CODING_BRIDGE_GAIN_RECORD_WORDS]
                .copy_from_slice(&aux.gain_a_record_tails[band]);
        }
    }

    // Spectrum routing (pinned): spec_a = delayed, spec_b = main.
    let spectrum_a = output.delayed_out.clone();
    let spectrum_b = output.spectra.clone();

    Ok(InitChannelState {
        objside_1c: aux.objside_1c,
        objside_14: aux.objside_14,
        objside_ptr: aux.objside_ptr,
        spec_b_ptr: aux.spec_b_ptr,
        y_index: aux.y_index,
        gain_a_records,
        gain_b_records: aux.gain_b_records.clone(),
        b_9c8: aux.b_9c8.clone(),
        b_a48: aux.b_a48.clone(),
        spectrum_a,
        spectrum_b,
    })
}

/// Assemble one channel's 16x38-word gainA record buffer from the
/// `time2freq_at5` exit records.
///
/// Two source laws, keyed on the detector mode:
///
/// - **mode_cc==0 (64/48 kbps `set_gainc_at5` dispatch):** the channel output
///   carries `final_plane_rows` = the full 16x38-word post-writeback `cur_plane`
///   rows (native `*(chobj+0x8)` content). Every word is carried WHOLE: point
///   prefix 0..15, the LIVE word-15 tonality-prepass flag (`+0x3c`), and the
///   float tail 16..38. At mode_cc==0 `detector_outcomes` is empty, so the
///   15-word `final_records` prefix is not the record source here — the plane
///   rows are. The "unread tail" law below does NOT apply at mode_cc==0: the
///   tail words are native obj+0x8 content read by zeroth/calc/packer.
///
/// - **mode_cc==1 (96..352 kbps detector path, `final_plane_rows` is `None`):**
///   each record is the 15-word POST-HARMONIZATION point prefix
///   (`output.final_records[band]`, init's `obj+0x8`/`obj+0xc` read) followed by
///   23 ZERO tail words. The tail is unread by every consumer (read-set
///   inventory on [`GainRollState`]), so zeroing it is exact for the from-PCM
///   path. Refuses a `prune_blocked` band (its point prefix is not
///   native-valid).
///
/// Shared by [`assemble_channel`] (which, on the mode_cc==1 prefix path only,
/// then overwrites the tail with the trace-fed value for boundary replay) and
/// [`advance_gain_roll`] (which stores exactly this into the roll).
///
/// Native has ONE in-place record buffer per channel: `time2freq_at5`'s
/// post-detector gain-record harmonization region (band-0 attack injection,
/// adjacent-band merge, and — for a first-live rate like 96 kbps where the gate
/// opens — the cross-channel harmonization at decompile `33193..33404`) MUTATES
/// that buffer after detection, and `init_channel_block_at5` reads the mutated
/// buffer. `output.final_records` holds exactly that post-mutation buffer.
/// When the harmonization gate is CLOSED (352/320/256/192/160/128 kbps,
/// `param7 >= 0x18` / bandwidth) `final_records[band]` equals the raw
/// `detector_outcomes[band].compact_point_words` by construction (stage 5 only
/// rewrites the records when the gate is open), so reading `final_records` is
/// byte-neutral at every landed rate.
///
/// Public so the from-scratch prepacker `gainb` window builder (docs/11 §2.2,
/// `serialize_gainb_window`) can lay down the SAME 16x38-word records the init
/// entry consumes — the packer reads only their 15-word point prefixes
/// (`parse_gain_rows`), which are exactly these detector point prefixes.
pub fn assemble_gain_a_records(
    channel: usize,
    output: &Time2FreqChannelOutput,
) -> Result<Vec<u32>, CodingBridgeError> {
    assemble_gain_a_records_with_band_count(channel, output, output.detector_outcomes.len())
}

pub(crate) fn assemble_gain_a_records_with_band_count(
    channel: usize,
    output: &Time2FreqChannelOutput,
    detector_band_count: usize,
) -> Result<Vec<u32>, CodingBridgeError> {
    let mut records = vec![0u32; CODING_BRIDGE_GAIN_BAND_COUNT * CODING_BRIDGE_GAIN_RECORD_WORDS];
    // mode_cc==0 (64/48 kbps `set_gainc_at5` dispatch): the channel output carries
    // the full post-writeback plane rows (native `*(chobj+0x8)`), 16x38 words each.
    // `set_gainc` runs all 16 bands, so every row is live. Carry the whole rows —
    // words 0..15 (point prefix), word 15 (tonality-prepass flag), and the float
    // tail 16..38 are all native obj+0x8 content that init/roll/packer read. There
    // is no detector-outcome prefix to fold and no prune check applies
    // (`detector_outcomes` is empty on this path).
    if let Some(rows) = output.final_plane_rows.as_ref() {
        if rows.len() < CODING_BRIDGE_GAIN_BAND_COUNT {
            return Err(CodingBridgeError::ShapeMismatch {
                field: "final_plane_rows",
                channel,
                expected: CODING_BRIDGE_GAIN_BAND_COUNT,
                actual: rows.len(),
            });
        }
        for band in 0..CODING_BRIDGE_GAIN_BAND_COUNT {
            let base = band * CODING_BRIDGE_GAIN_RECORD_WORDS;
            records[base..base + CODING_BRIDGE_GAIN_RECORD_WORDS]
                .copy_from_slice(&rows[band][..CODING_BRIDGE_GAIN_RECORD_WORDS]);
        }
        return Ok(records);
    }
    // At a reduced-band rate (192) `time2freq_at5` produces only `band_count`
    // (13) detector outcomes; the [band_count..16) gain records stay zero
    // (count 0), matching native's untouched tail (init's gain scan bound
    // `+0x1b48c` is band_count, so those records are never classified/packed —
    // docs/13 §3.1). The buffer stays 16-wide.
    let filled = detector_band_count.min(CODING_BRIDGE_GAIN_BAND_COUNT);
    if output.final_records.len() < filled {
        return Err(CodingBridgeError::ShapeMismatch {
            field: "final_records",
            channel,
            expected: filled,
            actual: output.final_records.len(),
        });
    }
    for band in 0..filled {
        if output
            .detector_outcomes
            .get(band)
            .is_some_and(|outcome| outcome.prune_blocked)
        {
            return Err(CodingBridgeError::PruneBlocked { channel, band });
        }
        let base = band * CODING_BRIDGE_GAIN_RECORD_WORDS;
        // Words 0..14: the POST-harmonization point prefix (init's obj+0x8/0xc
        // read). Native mutates the per-channel record buffer in place during
        // `time2freq_at5` stage 5; `final_records` is that mutated buffer.
        for (word, value) in output.final_records[band].iter().enumerate() {
            records[base + word] = *value as u32;
        }
        // Words 15..37 stay zero (unread tail).
    }
    Ok(records)
}

fn check_len(
    field: &'static str,
    channel: usize,
    actual: usize,
    expected: usize,
) -> Result<(), CodingBridgeError> {
    if actual != expected {
        return Err(CodingBridgeError::ShapeMismatch {
            field,
            channel,
            expected,
            actual,
        });
    }
    Ok(())
}

// ===========================================================================
// docs/11 §2.1 slice 2.1f — Slice-D init aux from the rolling gain double-buffer.
//
// The init entry's B-side gain surface (`gain_b_records`, `b_9c8`, `b_a48`) is
// NOT trace-fed in the from-PCM pipeline; it is the PREVIOUS call's A-side gain
// content, carried through the head-of-`at5enc_sigproc` double-buffer swap
// (`obj+0x8` <-> `obj+0xc`, decompile 42960–42964, `at5enc_sigproc` fn at 42766,
// native `0x4f2b0`). See the module doc "gainB / gain_b_records" section for the
// ===========================================================================

/// Per-channel rolling model of the gain double-buffer carry (`obj+0x8` gainA
/// this call becomes `obj+0xc` gainB next call, via the head-of-frame swap).
///
/// Each channel stores the PREVIOUS core call's assembled gainA content, which
/// the current call reads as its gainB entry surface:
///   * [`records`](Self::records): the 16x38-word gainA record buffer (point
///     prefixes ++ zero tails, exactly as [`assemble_gain_a_records`] builds
///     them).
///   * [`a_9c8`](Self::a_9c8): the previous call's `InitChannelOutput.a_9c8`
///     (32 i32 idsf decision words).
///   * [`a_a48`](Self::a_a48): the previous call's `InitChannelOutput.a_a48`
///     (32 f32 band-average weights).
///
/// [`new_zeroed`](Self::new_zeroed) models the calloc-zero handle start (matches
/// the captured call-0 entry `b_9c8`/`b_a48` and the all-zero gain_b prefixes);
/// [`advance_gain_roll`] rolls it forward after each call's init runs.
///
/// **Gain-record float tails are unread**, so the roll stores zero tails (never
/// the captured floats). Read-set inventory (each consumer proven to touch only
/// words 0..16 of a record):
///   * init `gain_spread_over_3`/`classify_gain_records`: words 0..14.
///   * zeroth `zeroth_gain_records_from_records`: words 0..16.
///   * calc `gain_rows_from_records`: words 0, 8..16.
///   * packer: proven unread by
///     `tests/composed_frame.rs::zeroing_gain_record_float_tails_does_not_change_packed_bytes`.
///   * detector: the rolling detector takes no record input (slice 2.1a).
#[derive(Debug, Clone)]
pub struct GainRollState {
    channels: Vec<GainRollChannel>,
}

#[derive(Debug, Clone)]
struct GainRollChannel {
    /// Previous call's 16x38-word gainA records (point prefix ++ zero tails).
    records: Vec<u32>,
    /// Previous call's gainA `+0x9c8` (32 i32 idsf words).
    a_9c8: Vec<i32>,
    /// Previous call's gainA `+0xa48` (32 f32 band-average weights).
    a_a48: Vec<f32>,
}

impl GainRollState {
    /// Calloc-zero start (all bytes zero for every channel), modeling the native
    /// handle-init state. Verified against the captured call-0 entry
    /// (`b_9c8 = [0;32]`, `b_a48 = [0;32]`, all-zero gain_b prefixes).
    pub fn new_zeroed(channel_count: usize) -> Self {
        let channels = (0..channel_count)
            .map(|_| GainRollChannel {
                records: vec![
                    0u32;
                    CODING_BRIDGE_GAIN_BAND_COUNT * CODING_BRIDGE_GAIN_RECORD_WORDS
                ],
                a_9c8: vec![0i32; CODING_BRIDGE_SEED_WORDS],
                a_a48: vec![0.0f32; CODING_BRIDGE_SEED_WORDS],
            })
            .collect();
        GainRollState { channels }
    }

    /// Number of channels tracked by the roll.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

/// Build the per-channel init aux (`CodingBridgeChannelAux`) for the current
/// core call from the rolling gain double-buffer carry.
///
/// B-side gain surface (`gain_b_records`, `b_9c8`, `b_a48`) comes from `roll`
/// (the previous call's A-side content, seen through the swap). The remaining
/// fields are 352-invariant / identity-only constants:
///   * `objside_1c = 0`, `objside_14 = 0`, `y_index = 16` (the reduced-band
///     init boundary is deliberately not widened by the calc slice);
///   * `objside_ptr` / `spec_b_ptr` = the synthetic identity-only pointers
///     ([`SYNTHETIC_OBJSIDE_PTR`] / [`SYNTHETIC_SPEC_B_PTR`], unread downstream);
///   * `gain_a_record_tails` = all-zero (the unread tail; see [`GainRollState`]).
pub fn coding_init_aux_from_frontend(roll: &GainRollState) -> Vec<CodingBridgeChannelAux> {
    roll.channels
        .iter()
        .map(|ch| CodingBridgeChannelAux {
            objside_1c: 0,
            objside_14: 0,
            objside_ptr: SYNTHETIC_OBJSIDE_PTR,
            spec_b_ptr: SYNTHETIC_SPEC_B_PTR,
            y_index: 16,
            b_9c8: ch.a_9c8.clone(),
            b_a48: ch.a_a48.clone(),
            gain_a_record_tails: vec![
                [0u32; CODING_BRIDGE_GAIN_TAIL_WORDS];
                CODING_BRIDGE_GAIN_BAND_COUNT
            ],
            gain_b_records: ch.records.clone(),
        })
        .collect()
}

/// Roll the gain double-buffer forward after the coding stage of the current
/// core call: store this call's assembled gainA records (the detector point
/// prefixes with zero tails) and this call's `InitChannelOutput.a_9c8`/`a_a48`,
/// so the NEXT call reads them as its gainB entry surface.
///
/// `report` supplies the post-harmonization point prefixes and the live
/// per-frame gain-band count — the exact same assembly the init entry uses, so
/// the roll carries byte-identical records even when the lean encoder report
/// omits detector diagnostics; `init_out` supplies the A-side
/// `+0x9c8`/`+0xa48` init just wrote. Refuses a `prune_blocked` band (same as
/// [`assemble_channel`]).
///
/// Call this AFTER running init for the current call (so `init_out` is the
/// current call's output). The native swap that reads the carried content
/// happens at the head of the NEXT call.
pub fn advance_gain_roll(
    roll: &mut GainRollState,
    report: &FrontendCoreCallReport,
    init_out: &InitFrameOutput,
) -> Result<(), CodingBridgeError> {
    let channels_t2f = report
        .time2freq
        .as_ref()
        .ok_or(CodingBridgeError::NoTime2Freq)?;
    if channels_t2f.len() != roll.channels.len() || init_out.channels.len() != roll.channels.len() {
        return Err(CodingBridgeError::ChannelCount {
            time2freq: channels_t2f.len(),
            aux: roll.channels.len(),
        });
    }
    let gain_band_count = report.sigproc.writeback.band_count as usize;
    for (channel, ((output, iout), slot)) in channels_t2f
        .iter()
        .zip(&init_out.channels)
        .zip(&mut roll.channels)
        .enumerate()
    {
        slot.records = assemble_gain_a_records_with_band_count(channel, output, gain_band_count)?;
        slot.a_9c8 = iout.a_9c8.clone();
        slot.a_a48 = iout.a_a48.clone();
    }
    Ok(())
}

/// Run one core call's init stage against the rolling gain double-buffer and
/// roll the buffer forward. Reusable per-call driver for the capstone loop and
/// the oracle test, so neither duplicates the "assemble aux → override join
/// flags → run init → advance" sequence.
///
/// Feeds init's per-group joint-stereo flags (`join_flags_50`, cfg
/// `+0x50..+0x90`) from the computed secondary tone words
/// ([`zeroth_tone_words_from_frontend`] of `state`) — the same override
/// `assemble_calc_frame_entry_at5` applies — because the init joint-stereo
/// spectrum SWAP (live at core calls 59/60/66) runs BEFORE the a_9c8/a_a48
/// derivation, so wrong flags would corrupt the rolled values at 60/61/67.
///
/// Returns the assembled init aux used this call (the caller can pin it against
/// the captured entry). `state` must be the rolling `FrontendState` at the
/// current core call; `report` is that call's frontend report.
pub fn init_roll_step(
    roll: &mut GainRollState,
    state: &FrontendState,
    report: &FrontendCoreCallReport,
) -> Result<Vec<CodingBridgeChannelAux>, CodingBridgeError> {
    let aux = coding_init_aux_from_frontend(roll);
    // Live config flag word (`cfg+0x1dc`) as it stands after this core call's
    // frontend ran — same word the coding-stage init sees (docs/12 §4.3
    // b-residual). Native runs a single `init_channel_block_at5` per call with
    // this word; the Rust double-buffer roll must see the same value so its
    // ×0.94 spectrum scaling matches.
    // Per-rate selector comes from the rolling frontend (`state.selector`). The
    // gain scan bound is the PER-FRAME effective `+0x1b48c` fan-out — the sigproc
    // shell epilogue writes `g_a_x_at5[param_5]+1` (post `+0x1dc & 0x7c` override)
    // to every channel's `+0x1b48c` every call (decompile 43506-43515), so init
    // reads THIS call's post-override band_count, not the static per-rate value
    // (docs/13 §3.1 slice 3). `report.sigproc.writeback.band_count` is that
    // fan-out: 16 full-band, 13/16 per-frame at 192. At full-band it always
    // equals the static `g_a_x_at5[state.band_limit]+1`, so 352 is unchanged.
    let gain_band_count = report.sigproc.writeback.band_count as usize;
    let mut init_state = assemble_init_frame_state_with_selector_at5(
        report,
        &aux,
        state.sigproc.header_flag_word,
        state.selector,
        gain_band_count,
        // PER-FRAME effective band extent (`cfg+0xb4`) the HF-cut gate reads —
        // the same post-override fan-out as `band_count` above, from
        // `report.sigproc.writeback.band_limit`. This init run advances the gain
        // roll (a_9c8/a_a48 derive from the cut spectra at 64/48 low-selector).
        report.sigproc.writeback.band_limit as usize,
    )?;
    // Same cfg row `+0x50..+0x90` as the zeroth's secondary tone words: the
    // one-call-delayed stereo swap flags. See `assemble_calc_frame_entry_at5`.
    let (_primary, secondary, _flag) = zeroth_tone_words_from_frontend(state);
    init_state.join_flags_50 = secondary;
    let init_out =
        init_channel_block_frame_at5(&mut init_state).map_err(|_| CodingBridgeError::InitBlock)?;
    advance_gain_roll(roll, report, &init_out)?;
    Ok(aux)
}

// ===========================================================================
// docs/11 Phase 1 bridge 1.1 — computed `CalcFrameEntry` bridge (full zeroth).
//
// The `calc_channel_block_at5` entry surface (`CalcFrameEntry`) is the state
// left by the native pipeline **`init_channel_block_at5` -> `norm_channel_block_at5`
// -> `zeroth_bit_allocation_at5`** (docs/06 Step 0.1). This bridge now runs the
// FULL composed zeroth pass (`zeroth_bit_allocation_frame_at5`) over the init
// output + norm surfaces + a small frontend/arena-owned aux, so every
// calc-entry OUTPUT-side field the calc trace captures is COMPUTED. Verified
// field-by-field against `calc_cb_io_trace.ndjson` call 7, cross-checked against
// the zeroth's own OUTPUT at the shared calls 0/7/12 in `zeroth_io_trace.ndjson`.
//
//   * **From the computed init OUTPUT (bridge 1.0 Slice D, byte-exact):**
//     `aux_3cc` (block+0x3cc weights, unchanged by norm/zeroth), `o_1b578`
//     (zeroed selectors), `o_1b678` (the spectrum-B idsf, the zeroth IDSF-leaf
//     scale-factor source), and the pointer surfaces. `idsf_cc` (block+0xcc)
//     and `o_1b6f8` are all-zero at entry (calc seeds them itself), and
//     `mode_1074 = 0`.
//   * **Norm-derived (Phase-0 tolerance):** `spectrum` = the **normalized main
//     mdspec** (`normalize_mdspec_at5(spec_b, idsf)`); `scale_24c` =
//     `normalize_mdspec_average_at5(max_b_2cc, idsf)`. Both feed the zeroth
//     quant path (`block+0x24c` scale, `block+0x1010` spectrum pointer).
//   * **Computed by the zeroth pass (bridge 1.1):** `max_wl_02` (the block+0x02
//     head row AFTER the zeroth relax rule = `[word0] ++ max_word_lengths`),
//     `base_weights_1cc` (block+0x1cc), `o_1b5f8` (the final word-length row),
//     `activity_14c` (the +0x14c copy), the `block+0xb08` quant plane
//     (`plane_b08`), the two `block+0x46` slot shorts (`slot_46`), the
//     `block+0x9f8` IDCT window (`idct_9f8`), and the shared bit words
//     `shared_s_11c`/`s_11e` (side +0x11c/+0x11e), `shared_s_12a`/`s_12e`
//     (the +0x12a/+0x12e totals). The tone-activity-gated inactive zeroing
//     (all-zero word-length rows at the silent priming frame) comes free with
//     the full pass. `plane_d88` is NOT written by the zeroth — verified all
//     zero at every captured calc entry — so it is `vec![0u32; 160]`, pinned by
//     the capture assertion in the test.
//   * **Gain rows (byte-exact):** `prev_gain_08` = obj+0x8 = the detector gain
//     array A, `cur_gain_0c` = obj+0xc = the empty 352 gainB array.
//   * **Config / ctx / shared (computed 352 constants, asserted vs capture):**
//     `selector = 30`, `budget = 16379`, the frame-invariant ctx scalars, the
//     shared words, `shared_row_94`/`shared_row_d4`, the frame-invariant object
//     config, `y_index = 16`, `objside_14 = objside_1c = 0`, and `shared_s_11a`
//     (the zeroth `((band*3)+2)*channels` seed). `ctx_active_b0`/`config_b0`
//     (active count) and `ctx_level_groups_c0`/`config_c0` (grouped count) come
//     from the zeroth active-count trim (32 / 16 at call 7; 0 / 1 at call 0).
//
// ## Remaining aux (frontend/arena-owned rolling state — Phase 2 / 2.1 owner)
//
// The calc-entry CUT is retired. The remaining zeroth-ENTRY aux feeds the
// zeroth surface, not the calc-entry output. Slice 2.1b (docs/11 §2.1) retires
// the two float-derived-decision inputs from the from-PCM pipeline:
//   * the tone-activity floats (objside +0x184/+0x284) are now computed by
//     `zeroth_tone_activity_from_frontend` from the rolling detector history
//     rows 0x61/0xa1 — written by `check_channel_correlation_at5` at the
//     `at5enc_sigproc` tail and rolled down by the head-of-frame history shift
//     (native evidence on `SIGPROC_DETECTOR_ROW_A_POWER_HISTORY0`);
//   * band activity (records +0x988) is computed by
//     `zeroth_band_activity_from_frontend` from the `time2freq_at5` tonality
//     flags (force-zeroed on the 352 `mode_cc == 1` path, decompile 32870).
// Slice 2.1c (docs/11 §2.1) retires `gha_bits` (side +0x126): it is computed by
// `gha_packing_prep_from_frontend(&FrontendState).total_bits as i16` over the
// rolling GHA arena ring (`packer_bridge`), the same single
// `calc_nbits_for_gha_at5` invocation native runs per call. Slice 2.1d (docs/11
// §2.1) retires the last three zeroth-ENTRY aux fields — the tone words (config
// +0x08../+0x50..) and the +0x94 tone flag — via
// `zeroth_tone_words_from_frontend(&FrontendState)`: the primary words are
// calloc-zero (`param_5==3` block never runs at 352), the secondary words are
// `state.sigproc.header_swap_words` (the one-call-delayed stereo swap flags the
// `at5enc_sigproc` tail writes to cfg +0x50..), and the flag is `false` (init
// zeroes cfg +0x94 every call and it is never re-stored before the zeroth read).
// After 2.1d NO zeroth-ENTRY aux field remains captured in the from-PCM
// pipeline. The boundary replay tests still build every field from the captured
// zeroth entries (correct test discipline).
// ===========================================================================

/// Frame bit budget for the 352 kbps single-block frame
/// (`frame_bytes*8 - block_count*2 - 3 = 2048*8 - 2 - 3`). The live path threads
/// the per-rate budget (14907 at 320) via
/// `assemble_calc_frame_entry_with_init_for_params_at5`; this constant is the 352
/// value the wrapper and boundary-replay tests use. See
/// [`crate::encoder::coding_params::frame_bit_budget`] for the law and
/// `zeroth_budget_by_rate.ndjson` for the per-rate oracle.
pub const CALC_BRIDGE_BUDGET: i32 = 16379;
/// Object-side quant-plane length (`block+0xb08`/`+0xd88`, 32 picks + 128 cost
/// words).
pub const CALC_BRIDGE_PLANE_WORDS: usize = 160;
/// IDCT state length (`block+0x9f8`).
pub const CALC_BRIDGE_IDCT_WORDS: usize = 68;
/// Quantized spectral plane length (`obj+0x1b6f8`, all-zero at calc entry).
pub const CALC_BRIDGE_QUANTIZED_WORDS: usize = 2048;
/// Object shared-config WLC subgroup count (`obj+0xb8`; also the zeroth IDSF
/// leaf group count) at the full-band / 28-unit extents. This is the constant
/// value of the per-frame `+0xb8` shape-count law
/// ([`crate::encoder::cfg_bridge::cfg_shape_count_b8`]) at band_index 28/29/32
/// (160/192/256/320/352); the live params path derives the per-frame value from
/// that law (9 at band_index 27 / 128).
pub const CALC_BRIDGE_GROUP_COUNT_B8: u32 = 10;
/// Object shared-config ATX block/channel mode (`obj+0xa8`).
pub const CALC_BRIDGE_CONFIG_A8: u32 = 2;
/// Object shared-config fixbits index (`obj+0x90`; the zeroth IDCT
/// `bandwidth_mode`).
pub const CALC_BRIDGE_CONFIG_90: u32 = 1;
/// Zeroth tone-group / band-activity group count (`piVar9[0x2f]`, 16 at 352).
pub const CALC_BRIDGE_TONE_GROUP_COUNT: usize = 16;
/// Shared encode-selector word (`**(obj+0x1008)`; decompile 36692) indexing
/// `sa_nencodetbls` for the quant candidate count. 1 on the 352 path (pinned
/// by `zeroth_io_trace` `encode_selector_u32` at calls 0/7/12).
pub const CALC_BRIDGE_ENCODE_SELECTOR: usize = 1;

/// Per-channel zeroth-ENTRY inputs the frontend/arena does not yet rotate.
/// Every field is captured rolling state owned by a later phase.
#[derive(Debug, Clone)]
pub struct ZerothBridgeChannelAux {
    /// Band-activity row at records `+0x988` (16 i32) feeding the `+0x122`
    /// band-activity side-data bits and the `+0x980/+0x984` summary. Written
    /// by `time2freq_at5` (native `0x3c480`, Ghidra `0x4c480`, decompile
    /// 32924..32944) as the per-band tonality flag (`block+0x3c`, stride
    /// `0x98`) copied to `records + 0x988 + band*4`; on the 352 path
    /// (`mode_cc == 1`) those flags are force-zeroed (decompile 32870..32872),
    /// so the row is all-zero at every captured call. Computed in the from-PCM
    /// pipeline via `zeroth_band_activity_from_frontend`
    /// (`Time2FreqChannelOutput.tonality.flags`); the boundary replay tests
    /// still build it from the captured `arena_activity_988_i32`.
    pub band_activity: Vec<i32>,
}

/// Frame-level zeroth-ENTRY inputs the frontend/arena does not yet produce.
#[derive(Debug, Clone)]
pub struct ZerothBridgeFrameAux {
    /// Primary tone-activity floats at `*(ch0+0x10)+0x184` (16 f32) gating the
    /// tone-span inactive zeroing (`zeroth_bit_allocation_at5`, native
    /// `0x42360`, decompile 36575..36607; the ONLY consumed decision is the
    /// per-group `== 0.0` equality). This is the one-call-delayed a-power
    /// history row `0x61`: fresh a-power lands at row `0x71` from
    /// `check_channel_correlation_at5` at the `at5enc_sigproc` tail (native
    /// call site `0x51285`) and the head-of-frame history shift rolls it down
    /// (see `SIGPROC_DETECTOR_ROW_A_POWER_HISTORY0`). Computed in the from-PCM
    /// pipeline via `zeroth_tone_activity_from_frontend`; captured as
    /// `tone_primary_activity_184_f32_bits` in the boundary replay tests.
    pub primary_tone_activity: Vec<f32>,
    /// Secondary tone-activity floats at `*(ch0+0x10)+0x284` (16 f32), read
    /// only when `channel_count == 2`. The one-call-delayed b-power history
    /// row `0xa1` (fresh b-power at row `0xb1`). Captured as
    /// `tone_secondary_activity_284_f32_bits`. Same source/owner as the
    /// primary row.
    pub secondary_tone_activity: Vec<f32>,
    /// Tone-primary words at config `+0x08..+0x48` (`piVar9[2..0x12]`, 16 i32)
    /// for the `+0x120` tone block. The ONLY encode-path writer is the zeroth's
    /// own `param_5 == 3` masking block, which never runs on the 352
    /// `param_5 == 2` path; the cfg block is calloc-zero at handle init, so this
    /// is all-zero. Computed in the from-PCM pipeline via
    /// `zeroth_tone_words_from_frontend` (constant `[0;16]` with the native why);
    /// captured from the zeroth tone-word entry only in the boundary replay
    /// tests.
    pub tone_primary_words: Vec<i32>,
    /// Tone-secondary words at config `+0x50..+0x90` (`piVar9[0x14..0x24]`,
    /// 16 i32). Written by the `at5enc_sigproc` stereo tail (decompile
    /// 43556..43558: `cfg[0x14 + i] = detector row 0xe1[i]`) — the
    /// one-call-delayed stereo swap flags. Computed in the from-PCM pipeline via
    /// `zeroth_tone_words_from_frontend` from
    /// `state.sigproc.header_swap_words` (the ported swap-tail store,
    /// `sigproc_shell.rs:499`); captured from the zeroth entry only in the
    /// boundary replay tests. Init reads the same cfg row as its per-group
    /// joint-stereo flags (`join_flags_50`).
    pub tone_secondary_words: Vec<i32>,
    /// Config `+0x94` (`piVar9[0x25]`) 9/1 header-word tone flag. Zeroed by
    /// `init_channel_block_at5` unconditionally every call (decompile
    /// 34401..34403) and never re-stored before the zeroth read (37675 → header
    /// word `+0x128`). Computed in the from-PCM pipeline via
    /// `zeroth_tone_words_from_frontend` (constant `false`); captured from the
    /// tone entry only in the boundary replay tests.
    pub tone_flag_25: bool,
    /// `calc_nbits_for_gha_at5` total bits at side `+0x126`. Computed in the
    /// from-PCM pipeline (slice 2.1c) as `prep.total_bits as i16`, where `prep`
    /// is `gha_packing_prep_from_frontend(&FrontendState)` over the rolling ring
    /// slot 0 — the same single `calc_nbits_for_gha_at5` invocation native runs
    /// per call (`zeroth_bit_allocation_at5`, decompile 37672-37675:
    /// `sVar14 = calc_nbits_for_gha_at5(); *(short *)(iVar8 + 0x126) = sVar14`).
    /// The boundary replay tests still trace-feed it from `zeroth_io_trace` side
    /// `+0x126` (correct test discipline).
    pub gha_bits: i16,
}

/// Compute the two tone-activity float rows the zeroth pass reads
/// (`ZerothBridgeFrameAux::primary_tone_activity` /
/// `secondary_tone_activity`, objside `+0x184` / `+0x284`) from the rolling
/// detector struct.
///
/// Reads `state.sigproc.detector_words` at the one-call-delayed a/b power
/// history rows `SIGPROC_DETECTOR_ROW_A_POWER_HISTORY0` (`0x61`) and
/// `..B_POWER_HISTORY0` (`0xa1`), decoding each of the 16 words as f32 bits.
/// Call after `frontend_core_call_at5` for the current core call returns: the
/// rows then hold exactly the surface the zeroth of that call reads (the a/b
/// power `check_channel_correlation_at5` wrote at the tail of the previous
/// call, rolled down one row by the head-of-frame history shift).
///
/// The zeroth consumes only the `== 0.0` decision per group; the float values
/// carry native x87-vs-f32 intermediate drift that is not chased (project
/// float rule).
pub fn zeroth_tone_activity_from_frontend(state: &FrontendState) -> (Vec<f32>, Vec<f32>) {
    let words = &state.sigproc.detector_words;
    let read_row = |base: usize| -> Vec<f32> {
        (0..FRONTEND_BAND_COUNT)
            .map(|i| f32::from_bits(words[base + i]))
            .collect()
    };
    (
        read_row(SIGPROC_DETECTOR_ROW_A_POWER_HISTORY0),
        read_row(SIGPROC_DETECTOR_ROW_B_POWER_HISTORY0),
    )
}

/// The five "masking" inputs the joint-stereo producer leaf
/// [`crate::coding::zeroth_pass::zeroth_joint_stereo_producer_at5`] reads from
/// the native per-channel `*(chan_obj+0x10)` buffer (modeled here as
/// `state.sigproc.detector_words`). Surfaced by
/// [`zeroth_joint_masking_inputs_from_frontend`]; see that fn for the native
/// source and the UNWIRED caveat.
#[derive(Debug, Clone)]
pub struct ZerothJointMaskingInputs {
    /// Active band count at `*(chan_obj+0x10)[0]` (`detector_words[0]`). Written
    /// by the shell's mode-aware `detector_words[0]` write
    /// (`at5enc_sigproc` 43239-43246 / 43496-43498): the `param_4 == 2` stereo
    /// path writes the default `0x10` (16, read at 352), while the mode-3
    /// (`param_4 == 3`) path writes `sa_intensity_band_44kHz[selector]` — 14 at
    /// 256 (selector 27), surfaced by
    /// [`crate::dsp::sigproc_shell::sigproc_intensity_band_count`]. The reader
    /// returns `detector_words[0]` unchanged either way.
    pub band_count: usize,
    /// Tone-state band count the `param_5 == 3` masking block scans up to. NOT a
    /// detector word: native reads `local_32c = *(cfg+0xbc)` (decompile
    /// 36151/36247), the PER-FRAME effective QMF/gain band count. The
    /// `at5enc_sigproc` shell epilogue writes `g_a_x_at5[param_5]+1` there, where
    /// `param_5` is forced to `0x20` on any frame whose `+0x1dc & 0x7c` override
    /// fires (else the static `band_index`, `cfg+0xb4`). Threaded in by the caller
    /// (docs/13 §3.1 slice 3 — LANDED): `g_a_x_at5[effective_band_limit] + 1` =
    /// **16** at 256/320/352 (band_index 32, override invisible), and **13** (no
    /// override) or **16** (override) per frame at 192 (band_index 29). This
    /// corrects the earlier §2.3 (w)/(x) "rate-independent 16" claim — the
    /// discovery sweep's packed QU count flips 29↔32 per frame
    /// (`qu_extent_override_192_run.json`).
    pub tone_state_bc: usize,
    /// Gate flags at objside `+0x04` (`detector_words[1 + b]`, 16 i32). All-zero
    /// for stereo — no encode-path writer targets this row (confirmed all-zero
    /// at 352 and 256).
    pub gate_04: Vec<i32>,
    /// Masking-energy floats at objside `+0x84` (`detector_words[0x21 + b]`, 16
    /// f32). The one-call-delayed slot-1 correlation dB (fresh at detector row
    /// `0x31`, `SIGPROC_DETECTOR_ROW_CORRELATION_DB`), rolled down by the
    /// head-of-frame history shift.
    pub energy_84: Vec<f32>,
    /// Second masking floats at objside `+0x4c4` (`detector_words[0x131 + b]`, 16
    /// f32). The rolled slot-6 stereo correlation dB (fresh at detector row
    /// `0x181`, `SIGPROC_DETECTOR_ROW_STEREO_DB`).
    pub masking2_4c4: Vec<f32>,
}

/// Surface the five joint-stereo masking inputs the zeroth's `param_5 == 3`
/// producer leaf reads, from the rolling `FrontendState`.
///
/// The reader is `zeroth_bit_allocation_at5`'s joint-stereo arm
/// (`param_5 == 3`), which reaches the per-channel detector/masking buffer via
/// `*(chan_obj+0x10)` — the same buffer this crate models as
/// `state.sigproc.detector_words` — and reads:
///
/// * **`band_count`** at `[0]` (`detector_words[0]`): the active band count,
///   forced to `0x10` for stereo by the `param_4 == 2` band-limit writeback.
/// * **`gate_04`** at `+0x04` (`detector_words[1..17]`, 16 i32): gate flags,
///   all-zero for stereo (no encode-path writer; confirmed 352 and 256).
/// * **`energy_84`** at `+0x84` (`detector_words[0x21..0x31]`, 16 f32): the
///   one-call-delayed slot-1 `check_channel_correlation_at5` difference dB. The
///   fresh value lands at detector row `0x31`
///   (`SIGPROC_DETECTOR_ROW_CORRELATION_DB`) and the head-of-frame history shift
///   rolls it down into `0x21` — the same rows/machinery
///   `tests/dsp_sigproc_shell.rs` already validates byte/tolerance-exact.
/// * **`masking2_4c4`** at `+0x4c4` (`detector_words[0x131..0x141]`, 16 f32): the
///   rolled slot-6 stereo correlation dB, fresh at detector row `0x181`
///   (`SIGPROC_DETECTOR_ROW_STEREO_DB`, likewise validated by that test).
///
/// `tone_state_bc` is NOT a detector word and NOT derived here: the caller
/// threads the PER-FRAME effective band count (native `local_32c = *(cfg+0xbc)`,
/// decompile 36151/36247 = `g_a_x_at5[effective_band_limit]+1`, the shell
/// epilogue's post-`+0x1dc & 0x7c`-override fan-out). It is **16** full-band
/// (band_index 32, override invisible at 352/320/256) and **13** (no override)
/// or **16** (override) per frame at 192. The coding-side threading of the
/// override is a docs/13 §3.1 slice-3 port (LANDED); per-frame QU counts flip
/// 29↔32 on the discovery inputs (`qu_extent_override_192_run.json`).
///
/// Call this AFTER `frontend_core_call_at5` for the current core call returns:
/// the detector rows then hold exactly the surface the zeroth of that call
/// reads (mirroring [`zeroth_tone_activity_from_frontend`]).
///
/// This surfaces the values for characterization; the `param_5 == 3` producer
/// leaf ([`crate::coding::zeroth_pass::zeroth_joint_stereo_producer_at5`]) is
/// now wired into the 256 kbps joint-stereo coding path (docs/13 §2.3), which
/// runs end-to-end rather than returning `UnportedBitrate`.
///
/// `detector_words` is a fixed-length invariant (`SIGPROC_DETECTOR_STRUCT_WORDS`
/// = 0x200); a shorter buffer is a programming error, so this indexes without
/// bounds checks exactly like [`zeroth_tone_activity_from_frontend`].
pub fn zeroth_joint_masking_inputs_from_frontend(
    state: &FrontendState,
    tone_state_bc: usize,
) -> ZerothJointMaskingInputs {
    let words = &state.sigproc.detector_words;
    // Gate flags at objside +0x04 (word 1..17), read as i32.
    let gate_04 = (0..FRONTEND_BAND_COUNT)
        .map(|b| words[1 + b] as i32)
        .collect();
    // Masking energy at objside +0x84 (word 0x21..0x31): rolled slot-1
    // correlation dB (fresh row 0x31), decoded as f32 bits.
    let energy_84 = (0..FRONTEND_BAND_COUNT)
        .map(|b| f32::from_bits(words[0x21 + b]))
        .collect();
    // Second masking at objside +0x4c4 (word 0x131..0x141): rolled slot-6
    // stereo dB (fresh row 0x181), decoded as f32 bits.
    let masking2_4c4 = (0..FRONTEND_BAND_COUNT)
        .map(|b| f32::from_bits(words[0x131 + b]))
        .collect();
    ZerothJointMaskingInputs {
        // Active band count at objside [0]; forced to 16 for stereo.
        band_count: words[0] as usize,
        // Tone-state scan bound = the PER-FRAME effective band_count the caller
        // threads (native `local_32c = *(cfg+0xbc)`; decompile 36151/36247). It
        // is `g_a_x_at5[effective_band_limit]+1` — the shell epilogue's
        // post-override fan-out (docs/13 §3.1 slice 3). 16 full-band; 13 or 16
        // per-frame at 192. NOT a detector word and NOT the static per-rate
        // value; passed in so the coding stage sees this frame's override state.
        tone_state_bc,
        gate_04,
        energy_84,
        masking2_4c4,
    }
}

/// Compute the per-channel band-activity rows the zeroth pass reads
/// (`ZerothBridgeChannelAux::band_activity`, records `+0x988`) from the
/// frontend's `time2freq_at5` tonality flags for the current core call.
///
/// `time2freq_at5` (native `0x3c480`, decompile 32924..32944) copies each
/// band's tonality flag (`block+0x3c`) to `records + 0x988 + band*4`; on the
/// 352 path (`mode_cc == 1`) the flags are force-zeroed (decompile
/// 32870..32872), so every entry is 0. Each channel yields the FULL 16-wide
/// [`FRONTEND_BAND_COUNT`] i32 row (records `+0x988` is the 16-wide gain-band
/// buffer): at a reduced-band rate (192) `time2freq_at5` only produces
/// `band_count` (13) tonality flags, so the [band_count..16) tail is
/// zero-padded — matching native's untouched (calloc-zero, mode_cc-cleared)
/// tail (docs/13 §3.1). Errors with `NoTime2Freq` when the report carried no
/// time2freq output (the always-open gate never closes).
pub fn zeroth_band_activity_from_frontend(
    report: &FrontendCoreCallReport,
) -> Result<Vec<ZerothBridgeChannelAux>, CodingBridgeError> {
    let channels = report
        .time2freq
        .as_ref()
        .ok_or(CodingBridgeError::NoTime2Freq)?;
    Ok(channels
        .iter()
        .map(|output| {
            let mut band_activity = vec![0i32; CODING_BRIDGE_GAIN_BAND_COUNT];
            for (band, &flag) in output
                .tonality
                .flags
                .iter()
                .take(CODING_BRIDGE_GAIN_BAND_COUNT)
                .enumerate()
            {
                band_activity[band] = i32::from(flag);
            }
            ZerothBridgeChannelAux { band_activity }
        })
        .collect())
}

/// Compute the three zeroth-ENTRY tone-word inputs the zeroth pass reads from
/// the shared cfg window (`ZerothBridgeFrameAux::tone_primary_words` /
/// `tone_secondary_words` / `tone_flag_25`, config `+0x08..+0x48` /
/// `+0x50..+0x90` / `+0x94`) from the rolling `FrontendState`. Returns
/// `(primary_words, secondary_words, tone_flag)`.
///
/// The reader is `zeroth_bit_allocation_at5` (native `0x42360`, Ghidra
/// `0x52360`; decompile fn at line 35840). It reaches the shared cfg window via
/// `piVar9 = *(int **)(iVar15 + 4)` (decompile 35980) — the same `*(obj0+4)`
/// `cfg[0,0x400)` window bridge 1.7 builds — and reads:
///
/// * **Primary tone words** `piVar9[2..0x12]` (cfg `+0x08..+0x48`, 16 i32),
///   summed at decompile 37629 into the `+0x120` tone-block primary summary
///   flags `piVar9[0]/[1]` (decompile 37623..37641). The ONLY encode-path
///   writer of these words is the zeroth's OWN `param_5 == 3` masking block
///   (`LAB_000524a6` at decompile ~36016; write sites 36179..36195
///   `*(iVar21 + 8 + i*4) = 0/1`, `iVar21 = *(iVar15 + 4)` = cfg). The 352 path
///   invokes the zeroth with `param_5 == 2` (pinned by
///   the `param_5 == 3` block is documented never-running in `zeroth_pass.rs`),
///   so that block never runs. A static sweep of the decompile found no other
///   store to these words; the cfg block is calloc-zero at handle init (Slice
///   D). Empirically all-zero at every captured call (zeroth_io_trace
///   `tone_words_u32[2..18]` at calls 0/7/12, AND the 77-frame prepacker cfg
///   blobs). Therefore constant `vec![0i32; 16]`.
///
/// * **Secondary tone words** `piVar9[0x14..0x24]` (cfg `+0x50..+0x90`, 16 i32),
///   summed at decompile 37648..37666 into `piVar9[0x12]/[0x13]`. The ONLY
///   encode-path writer is the `at5enc_sigproc` (Ghidra `0x5f2b0`; decompile
///   fn at 42766) stereo tail: after `check_channel_correlation_at5` refreshes
///   detector rows `0x31`/`0x71`/`0xb1` and the fresh swap decision lands at
///   row `0xf1`, decompile 43556..43558 stores `cfg[0x14 + i] = detector row
///   `0xe1`[i]` for i < 16 (`piVar5[i + 0x14] = __dest[i]`, `__dest = detector
///   + 0xe1`, `piVar5` = cfg). Row `0xe1` is last call's row `0xf1` after the
///   head-of-frame history rotation (`copy_within(0xf1.., 0xe1)`,
///   `sigproc_shell.rs:48`) — i.e. the one-call-delayed swap flags. This surface
///   is already fully ported: `sigproc_stereo_swap_update_at5`
///   (`sigproc_shell.rs:166`) computes the fresh decision and the composed pass
///   stores exactly the one-call-delayed `previous_swap` into
///   `state.sigproc.header_swap_words` (`sigproc_shell.rs:499`). Call this AFTER
///   `frontend_core_call_at5` for the current core call N returns: the row then
///   holds exactly what native wrote into cfg `+0x50..+0x90` during call N's
///   sigproc — the surface call N's zeroth reads. So
///   `tone_secondary_words[i] = state.sigproc.header_swap_words[i] as i32`.
///
/// * **Tone flag** `piVar9[0x25]` (cfg `+0x94`), read once at decompile 37675
///   (`+0x128` header word = 9 if set else 1). `init_channel_block_at5` (Ghidra
///   `0x4f870`, decompile fn at 34336) zeroes cfg `+0x118/+0x94/+0x98/+0x9c`
///   unconditionally at its head EVERY call (decompile 34401..34403), and init
///   runs after sigproc and before normalize/zeroth within each core call. A
///   static sweep found no store to cfg `+0x94` between init's zeroing and the
///   zeroth's read (the only other `+0x94` stores in the library are the DECODER
///   `unpack_channel_block_at5` at 26130..26135 and u16 side-struct row writes
///   `side+0x94+i*2`, a different struct). Therefore constant `false`.
///
/// The three fields are integer decision words (tone-block summaries + a header
/// selector), pinned EXACTLY — no tolerance.
pub fn zeroth_tone_words_from_frontend(state: &FrontendState) -> (Vec<i32>, Vec<i32>, bool) {
    // Primary words (cfg +0x08..+0x48): never written on the 352 param_5==2
    // path; calloc-zero. See doc above.
    let primary = vec![0i32; FRONTEND_BAND_COUNT];
    // Secondary words (cfg +0x50..+0x90): the one-call-delayed stereo swap
    // flags native's sigproc tail wrote, held in the rolling frontend.
    let secondary = state
        .sigproc
        .header_swap_words
        .iter()
        .take(FRONTEND_BAND_COUNT)
        .map(|&w| w as i32)
        .collect();
    // Tone flag (cfg +0x94): zeroed by init every call, never re-stored. See
    // doc above.
    let tone_flag = false;
    (primary, secondary, tone_flag)
}

/// Build the per-band `CalcGainRow`s (count + 8 level ids) from a 16x38-word
/// gain array (`obj+0x8` / `obj+0xc`, 0x98-byte / 38-word stride): word 0 is
/// the point count, words 8..16 are the `+0x20` level ids.
fn gain_rows_from_records(records: &[u32]) -> Vec<CalcGainRow> {
    (0..CODING_BRIDGE_GAIN_BAND_COUNT)
        .map(|band| {
            let base = band * CODING_BRIDGE_GAIN_RECORD_WORDS;
            CalcGainRow {
                count: records[base] as i32,
                level_ids: (0..8).map(|k| records[base + 8 + k] as i32).collect(),
            }
        })
        .collect()
}

/// Parse a 16x38-word gain array (`obj+0x8`, 0x98-byte stride) into the
/// zeroth's per-band gain records: word 0 = point count, words 1..8 = the
/// `+0x4` locations, words 8..16 = the `+0x20` level ids. Same layout as
/// `gain_rows_from_records`, but keeping the full location/level prefix the
/// zeroth gain scoring reads.
fn zeroth_gain_records_from_records(records: &[u32]) -> Vec<ZerothGainRecord> {
    (0..CODING_BRIDGE_GAIN_BAND_COUNT)
        .map(|band| {
            let base = band * CODING_BRIDGE_GAIN_RECORD_WORDS;
            let mut locations = [0i32; 7];
            let mut levels = [0i32; 8];
            for (k, slot) in locations.iter_mut().enumerate() {
                *slot = records[base + 1 + k] as i32;
            }
            for (k, slot) in levels.iter_mut().enumerate() {
                *slot = records[base + 8 + k] as i32;
            }
            ZerothGainRecord {
                point_count: records[base] as i32,
                locations,
                levels,
            }
        })
        .collect()
}

/// Per-channel norm surfaces the zeroth quant path reads, computed once
/// off the init output.
struct ChannelNorm {
    /// The normalized main mdspec (`block+0x1010` spectrum pointer source).
    spectrum: Vec<f32>,
    /// The per-band norm scale (`block+0x24c`).
    scale_24c: Vec<f32>,
    /// The per-band idsf the norm used (`block+0xcc` == 0 at zeroth entry),
    /// kept for the calc-entry `idsf_cc` field.
    idsf_cc: Vec<i32>,
    /// Gain array A as zeroth records (`obj+0x8` detector writeback).
    gain_records: Vec<ZerothGainRecord>,
    /// The per-band quant windows (`block+0x1010 + isps[band]*4`,
    /// `nsps[band]` floats each) into the normalized spectrum.
    quant_windows: Vec<Vec<f32>>,
}

/// Assemble the computed `calc_channel_block_at5` entry surface
/// (`CalcFrameEntry`) at the current core call by running the FULL composed
/// zeroth pass over the init output + norm surfaces + the small
/// frontend/arena-owned aux.
///
/// Composes bridge 1.0 Slice D (`assemble_init_frame_state_at5` +
/// `init_channel_block_frame_at5`), the norm stage (`normalize_mdspec_at5` +
/// `normalize_mdspec_average_at5`), and `zeroth_bit_allocation_frame_at5`; sources
/// the frame-invariant config as 352 constants and the gain rows from the
/// detector; and takes the frontend/arena rolling state (band activity, tone
/// floats/words, gha bits) from `zeroth_aux`. See the module doc for the full
/// field-source map and the remaining-aux note.
pub fn assemble_calc_frame_entry_at5(
    report: &FrontendCoreCallReport,
    init_aux: &[CodingBridgeChannelAux],
    zeroth_aux: &ZerothBridgeFrameAux,
    zeroth_channel_aux: &[ZerothBridgeChannelAux],
) -> Result<CalcFrameEntry, CodingBridgeError> {
    // Test/replay convenience wrapper: the captured frames it replays all have
    // feed the zero constant to preserve their byte-parity pins.
    Ok(assemble_calc_frame_entry_with_init_at5(
        report,
        init_aux,
        zeroth_aux,
        zeroth_channel_aux,
        CODING_BRIDGE_FLAGS_1DC,
    )?
    .0)
}

/// Per-channel init gain-classification header words the packer reads from
/// `range_b [0x1b484, 0x1b494)` (`pack_gain_block`, `src/bitstream/frame.rs`):
/// `obj+0x1b484` record-present flag, `+0x1b488` delta flag, `+0x1b48c` prev
/// count, `+0x1b490` gain row count. Sourced directly from
/// [`InitChannelOutput`](crate::coding::init_block::InitChannelOutput) — init
/// already reproduces them byte-exact (Slice D). Consumed by the from-scratch
/// prepacker init-header serializer (docs/11 §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitGainHeaderWords {
    /// `obj+0x1b484` (`InitChannelOutput.obj_1b484`): record-present flag. Word
    /// `0x1b480` is packer-unread (left 0 by the serializer).
    pub obj_1b484: i32,
    /// `obj+0x1b488` (`InitChannelOutput.obj_1b488`): count-differs delta flag.
    pub obj_1b488: i32,
    /// `obj+0x1b48c` (`InitChannelOutput.obj_1b48c`): last-nonzero prev count.
    pub obj_1b48c: i32,
    /// `obj+0x1b490` (`InitChannelOutput.obj_1b490`): gain row count (the count
    /// `parse_gain_rows` uses).
    pub obj_1b490: i32,
}

/// Like [`assemble_calc_frame_entry_at5`], but also returns the per-channel init
/// gain-classification header words ([`InitGainHeaderWords`], from the same init
/// run) the from-scratch prepacker serializer needs. The calc entry is
/// identical to what [`assemble_calc_frame_entry_at5`] returns.
pub fn assemble_calc_frame_entry_with_init_at5(
    report: &FrontendCoreCallReport,
    init_aux: &[CodingBridgeChannelAux],
    zeroth_aux: &ZerothBridgeFrameAux,
    zeroth_channel_aux: &[ZerothBridgeChannelAux],
    flags_1dc: u32,
) -> Result<(CalcFrameEntry, Vec<InitGainHeaderWords>), CodingBridgeError> {
    // 352 wrapper: selector 30, budget 16379, mode_a 2 (joint-stereo producer
    // DEAD, so no frontend is needed). The live per-rate path routes through
    // `assemble_calc_frame_entry_with_init_for_params_at5`. The mode-2 path never
    // exercises the producer, so the effective tone-primary words the params fn
    // returns are always init's `zeroth_aux.tone_primary_words`; drop them here
    let (entry, headers, _tone_primary_effective) =
        assemble_calc_frame_entry_with_init_for_params_at5(
            report,
            init_aux,
            zeroth_aux,
            zeroth_channel_aux,
            flags_1dc,
            CODING_BRIDGE_SELECTOR,
            CALC_BRIDGE_BUDGET,
            2,
            CODING_BRIDGE_INIT_BAND_COUNT as u32,
            None,
        )?;
    Ok((entry, headers))
}

/// Like [`assemble_calc_frame_entry_with_init_at5`] but with an explicit per-rate
/// block `selector` (`cfg+0x1e8`), frame bit `budget` (`cfg+0x1e0`), and the
/// `mode_a` joint-stereo producer gate threaded into the init/zeroth/calc entry
/// surfaces (docs/13 §1.1 / §2.3). At `(30, 16379, mode_a = 2)` this is
/// byte-identical to the 352 wrapper.
///
/// `mode_a == 3` (48-256 kbps) runs the zeroth `param_5 == 3` joint/intensity-
/// stereo producer arm ([`zeroth_joint_stereo_producer_at5`]) and routes its
/// `shared_row_94` into the SHARED `+0x94` cross-zero row (the consumer
/// `apply_zeroth_stereo_cross_zero_at5`, already wired in the zeroth pass); it
/// requires `frontend` to be `Some` (the rolling [`FrontendState`] the producer
/// reads its masking inputs from). `mode_a == 2` (320/352) skips the arm
/// wholesale — `frontend` may be `None` and the SHARED `+0x94` row stays init's
/// all-zero `row_94`, so those rates are byte-identical.
///
/// `band_index` is the PER-FRAME effective band extent (`cfg+0xb4` post the
/// sigproc `+0x1dc & 0x7c` override; docs/13 §3.1 slice 3): the live driver
/// threads `report.sigproc.writeback.band_limit`, not a static per-rate word. At
/// full-band (32) the override is invisible, so 256/320/352 stay byte-identical.
#[allow(clippy::too_many_arguments)]
/// Route the final, post-cross-zero zeroth side rows into the calc entry.
/// these mutated rows from the producer/init rows used to seed zeroth.
#[doc(hidden)]
pub fn calc_entry_cross_rows_from_zeroth_at5(
    primary: &[i16],
    secondary: &[i16],
) -> (Vec<i16>, Vec<i16>) {
    (primary.to_vec(), secondary.to_vec())
}

pub fn assemble_calc_frame_entry_with_init_for_params_at5(
    report: &FrontendCoreCallReport,
    init_aux: &[CodingBridgeChannelAux],
    zeroth_aux: &ZerothBridgeFrameAux,
    zeroth_channel_aux: &[ZerothBridgeChannelAux],
    flags_1dc: u32,
    selector: i32,
    budget: i32,
    mode_a: u32,
    band_index: u32,
    frontend: Option<&FrontendState>,
) -> Result<(CalcFrameEntry, Vec<InitGainHeaderWords>, Vec<i32>), CodingBridgeError> {
    // Per-FRAME effective band extent (docs/13 §3.1 slice 3). `band_index`
    // (`cfg+0xb4`, the scale-factor / quant-unit count, post `+0x1dc & 0x7c`
    // override) drives the normalize/quant computation extent and the zeroth band
    // count (which triggers the `{29,30,31}->32/16` finalize round-up that zeroes
    // word-length rows [band_index..32) — the native header stays full-band
    // 32/16). `gain_band_count` (`g_a_x_at5[band_index]+1`, the QMF/gain-group
    // count `+0x1b48c` seed) bounds init's gain-record classification and equals
    // this call's `report.sigproc.writeback.band_count`. 32 / 16 full-band; per
    // frame 29 / 13 (no override) or 32 / 16 (override) at 192.
    let quant_units = band_index as usize;
    let gain_band_count = usize::from(crate::tables::at5::x_at5()[quant_units]) + 1;

    // Bridge 1.0: the init entry surface, then run init to the byte-exact output.
    let mut init_state = assemble_init_frame_state_with_selector_at5(
        report,
        init_aux,
        flags_1dc,
        selector,
        gain_band_count,
        // PER-FRAME effective band extent (`cfg+0xb4`) for the HF-cut gate: the
        // same post-override quant-unit count threaded above. At 64/48 mono &
        // stereo (selector < 0x10, extent 27/26) the gate opens and init zeroes
        // the top spectral lines; at full-band it stays closed.
        quant_units,
    )?;
    // Init's per-group joint-stereo flags (`init_channel_block_at5` decompile
    // 34716: `*(int *)(iVar26 + 0x50 + i*4) == 1` → spectral joint-stereo SWAP
    // branch) read the SAME cfg row `+0x50..+0x90` as the zeroth's secondary
    // tone words — identical native memory. Feed `join_flags_50` from the
    // computed `tone_secondary_words` (the one-call-delayed stereo swap flags),
    // branch stays inactive through core call 58; it becomes LIVE at core calls
    // 59/60/66 (e.g. call 59 = cfg group-14 flag = 1), where the init port
    // exchanges the flagged 128-float group between ch0/ch1 in BOTH spectra in
    // place (`init_block.rs`, decompile 34711–34766). Init mutates the
    // caller-owned spectra, so `ist.spectrum_b` below observes the swapped
    // surface used for the normalization pass — matching native.
    init_state.join_flags_50 = zeroth_aux.tone_secondary_words.clone();
    let init_out =
        init_channel_block_frame_at5(&mut init_state).map_err(|_| CodingBridgeError::InitBlock)?;

    // Init gain-classification header words the from-scratch prepacker serializer
    // needs (`range_b [0x1b484, 0x1b494)`), captured from this init run.
    let init_gain_headers: Vec<InitGainHeaderWords> = init_out
        .channels
        .iter()
        .map(|iout| InitGainHeaderWords {
            obj_1b484: iout.obj_1b484,
            obj_1b488: iout.obj_1b488,
            obj_1b48c: iout.obj_1b48c,
            obj_1b490: iout.obj_1b490,
        })
        .collect();

    let n = init_state.channels.len();
    if zeroth_channel_aux.len() != n {
        return Err(CodingBridgeError::ChannelCount {
            time2freq: n,
            aux: zeroth_channel_aux.len(),
        });
    }

    // `bands` is the fixed 32-unit STORAGE width for calc rows/planes. The
    // zeroth finalizer below separately produces the native processing/header
    // extent: 160 falls through to 28/12, 192's 29/13 rounds to 32/16, and
    // full-band rates stay 32/16. Units [quant_units..32) receive no word
    // length and remain zero in storage.
    let bands = CODING_BRIDGE_INIT_BAND_COUNT;
    let isps = isps_at5();
    let nsps = nsps_at5();
    // Band -> unit boundaries (`g_a_y_at5`): unit range of band b is
    // `y[b]..y[b+1]`; unit u's normalized-spectrum line range is
    // `isps[u]..isps[u+1]`. Consumed by the a48 ch1 sign flip below.
    let y = y_at5();

    // --- Pass 1: per-channel norm surfaces (spectrum, scale, quant windows). ---
    let mut norms: Vec<ChannelNorm> = Vec::with_capacity(n);
    for ci in 0..n {
        let ist = &init_state.channels[ci];
        let iout = &init_out.channels[ci];
        check_len(
            "band_activity",
            ci,
            zeroth_channel_aux[ci].band_activity.len(),
            CODING_BRIDGE_GAIN_BAND_COUNT,
        )?;

        // norm stage: the spectrum-B idsf drives normalization.
        let idsf_u32: Vec<u32> = iout.idsf_1b678.iter().map(|&v| v as u32).collect();
        // spectrum = normalize(spec_b) then clip (a no-op at 352 idsf levels).
        // Only the active `quant_units` units are normalized (29 at 192); the
        // [quant_units..32) lines stay un-normalized but are never quantized
        // (their word length is 0). `scale_24c`/`idsf_cc` stay 32-wide (calc
        // reads them for 0..32) with the tail left zero.
        let mut spectrum = ist.spectrum_b.clone();
        // High-frequency spectral cut on the norm-stage copy (decompile
        // 34685-34710). Native shares ONE physical spectrum buffer per channel
        // between init and normalize, so the init cut aliases straight into what
        // normalize reads; Rust ran the cut in place on `init_state` above, so
        // `ist.spectrum_b` (cloned here) already carries the zeros. Re-apply
        // under the same gate/law so the norm surface stays cut independent of
        // that aliasing — the composed normalized spectrum must carry the zeros.
        if init_high_frequency_cut_gate_open(selector, init_state.sr_ac, quant_units) {
            let start = init_high_frequency_cut_start_at5(quant_units);
            for value in spectrum[start..].iter_mut() {
                *value = 0.0;
            }
        }
        normalize_mdspec_at5(&mut spectrum, &idsf_u32, quant_units)?;
        clip_normalized_mdspec_at5(&mut spectrum, &idsf_u32, quant_units)?;
        // scale_24c = normalize(max_b, idsf) = block+0x2cc / g_a_sftbl_at5[idsf].
        let mut scale_24c = vec![0f32; bands];
        normalize_mdspec_average_at5(&mut scale_24c, &iout.max_b_2cc, &idsf_u32, quant_units)?;

        // Per-band quant windows into the normalized spectrum: the zeroth
        // reads `block+0x1010 + isps[band]*4` for `nsps[band]` floats (the
        // per-band idsf `block+0xcc` is 0 at zeroth entry).
        let quant_windows: Vec<Vec<f32>> = (0..quant_units)
            .map(|band| {
                let start = usize::from(isps[band]);
                let count = usize::from(nsps[band]);
                spectrum[start..start + count].to_vec()
            })
            .collect();

        let gain_records = zeroth_gain_records_from_records(&ist.gain_a_records);
        norms.push(ChannelNorm {
            spectrum,
            scale_24c,
            idsf_cc: vec![0i32; bands],
            gain_records,
            quant_windows,
        });
    }

    // --- Pass 2: build the zeroth-pass input surface and run it. ---
    // The quant raw rows borrow the per-channel norm windows/scale.
    let quant_raws: Vec<Vec<ZerothQuantBandRaw<'_>>> = norms
        .iter()
        .map(|norm| {
            (0..quant_units)
                .map(|band| ZerothQuantBandRaw {
                    spectrum: &norm.quant_windows[band],
                    // `block+0xcc` per-band idsf == 0 at zeroth entry.
                    idsf: norm.idsf_cc[band] as usize,
                    scale: norm.scale_24c[band],
                    count: usize::from(nsps[band]),
                })
                .collect()
        })
        .collect();

    // The zeroth head/word rows the pass reads (block+0x02): the mode word0
    // and the max word-length row.
    let max_word_length_rows: Vec<Vec<i16>> = init_out
        .channels
        .iter()
        .map(|iout| iout.word_rows[1..=bands].to_vec())
        .collect();

    let zeroth_channels: Vec<ZerothChannelState<'_>> = (0..n)
        .map(|ci| {
            let iout = &init_out.channels[ci];
            ZerothChannelState {
                idsf_activity: &iout.word_lengths_4c,
                weight_scale: iout.scaled_454,
                aux_weights: &iout.weight_3cc,
                max_word_lengths: &max_word_length_rows[ci],
                quant_bands: &quant_raws[ci],
                gain_records: &norms[ci].gain_records,
                gain_band_count: iout.obj_1b490 as usize,
                gha_flags: ZerothGhaChannelFlags {
                    has_nonzero_band: iout.obj_1b484 != 0,
                    trimmed_differs: iout.obj_1b488 != 0,
                },
                band_activity: &zeroth_channel_aux[ci].band_activity,
                mode_word_0: iout.word0,
                // The +0x1b678 idsf scale-factor row (all-zero at zeroth
                idsf_scale_factors: &iout.idsf_1b678,
                // Energy class (block+0x42) and tonality (block+0x458) for the
                // 44.1 low-selector boost ladder (docs/13 §3.2 slice 3; live at
                // selector < 0x19, e.g. 160). Unused at 320/352 (selector >=
                // 0x19), where the ladder gate is closed.
                class_42: iout.class_42,
                tonality_458: iout.tonality_458,
                // Init's Block N transient byte (block+0x45c), gating the
                // zeroth Block D transient weight boost (decompile
                // 36528-36539; live at 96 kbps selector 19, `false` at
                // 128+/352 where the selector gate is closed).
                transient_45c: iout.transient_45c,
            }
        })
        .collect();

    // Joint/intensity-stereo producer (docs/13 §2.3): the zeroth
    // `param_5 == 3` arm (`zeroth_joint_stereo_producer_at5`). LIVE at 48-256
    // kbps (mode_a == 3); DEAD at 320/352 (mode_a == 2), so those rates keep
    // init's all-zero `row_94` and stay byte-identical. Its `shared_row_94`
    // feeds the cross-zero row below (the already-wired consumer
    // `apply_zeroth_stereo_cross_zero_at5` zeroes ch1's word lengths for the
    // joined units) and the calc entry's `shared_row_94`.
    let producer_out: Option<JointStereoProducerOutput> = if mode_a == 3 {
        let state = frontend.ok_or(CodingBridgeError::JointStereoMissingFrontend)?;
        // The producer's tone-state scan bound is `*(cfg+0xbc)` = the per-frame
        // effective band_count (`gain_band_count` above == `g_a_x_at5[band_index]
        // +1` for THIS frame's post-override `band_index`; docs/13 §3.1 slice 3).
        let masking = zeroth_joint_masking_inputs_from_frontend(state, gain_band_count);
        // side+4: per-unit IDSF of the channel difference (ch0 - ch1) over the
        // post-init-swap main mdspec (`spectrum_b`), 32 units — the
        // `norm_channel_block_at5` param_5 == 3 branch (normalize.rs). Native
        // side+4 is a scale-factor index row (u32); the producer reads it as an
        // i32 floor threshold.
        let (side4_u32, _band_max) = norm_channel_difference_idsf_at5(
            &init_state.channels[0].spectrum_b,
            &init_state.channels[1].spectrum_b,
            ZEROTH_BANDS_AT5,
        )
        .map_err(CodingBridgeError::Normalize)?;
        let side_04: Vec<i32> = side4_u32.iter().map(|&v| v as i32).collect();
        let input = JointStereoProducerInput {
            // param_5 == mode_a (docs/13 §2.3 (r)); the leaf gates on `== 3`.
            param_5: mode_a,
            selector: selector as u32,
            band_count: masking.band_count,
            tone_state_bc: masking.tone_state_bc,
            gate_04: &masking.gate_04,
            energy_84: &masking.energy_84,
            masking2_4c4: &masking.masking2_4c4,
            side_04: &side_04,
            channels: [
                JointStereoChannelInput {
                    scale_factors: &init_out.channels[0].idsf_1b678,
                    aux_weights: &init_out.channels[0].weight_3cc,
                    spectrum: &init_out.channels[0].a_a48,
                },
                JointStereoChannelInput {
                    scale_factors: &init_out.channels[1].idsf_1b678,
                    aux_weights: &init_out.channels[1].weight_3cc,
                    spectrum: &init_out.channels[1].a_a48,
                },
            ],
        };
        Some(zeroth_joint_stereo_producer_at5(&input))
    } else {
        None
    };

    // Cross flags at zeroth ENTRY. At mode_a == 3 the producer supplies +0x94;
    // at mode_a == 2 it is init's all-zero row. +0xd4 enters from init as zero.
    // `zeroth_bit_allocation_frame_at5` mutates both rows during stereo
    // cross-zero; calc must consume the returned rows, not these entry values.
    let cross_primary_flags: Vec<i16> = match &producer_out {
        Some(out) => out.shared_row_94.to_vec(),
        None => init_out.shared.row_94.iter().map(|&v| v as i16).collect(),
    };
    let cross_secondary_flags: Vec<i16> =
        init_out.shared.row_d4.iter().map(|&v| v as i16).collect();

    // Effective tone-primary words (cfg group-2 side data `cfg[0x08+k*4]`,
    // docs/13 §2.3). At mode_a == 3 the native zeroth `param_5 == 3` arm writes
    // its per-band `band_join` DIRECTLY into these cfg words (decompile
    // 36150-36221; the pack reader 46633-46781 emits them via the summarizer
    // head/inner law). Route the producer's `band_join` here so both the ported
    // `zeroth_tone_side_bits_at5` accounting (via `ZerothFrameState`) AND the cfg
    // group-2 packing (via the value returned to `compute_output_frame`) see the
    // native words. At mode_a == 2 (320/352) the producer is DEAD, so this is
    // init's all-zero `zeroth_aux.tone_primary_words` — byte-identical to before.
    let tone_primary_effective: Vec<i32> = match &producer_out {
        Some(out) => out.band_join.to_vec(),
        None => zeroth_aux.tone_primary_words.to_vec(),
    };

    let mut zeroth_state = ZerothFrameState {
        channels: zeroth_channels,
        // Per-FRAME effective quant-unit count (`cfg+0xb4` band_index, post
        // override; 29 or 32 per frame at 192). This is the loop cap for
        // word-length/quant computation AND the `iVar10` the finalize round-up
        // tests: at 29-31 it zeroes rows [band_index..32) and keeps the header
        // 32/16 (docs/13 §3.1 slice 3). 32 full-band.
        band_count: quant_units,
        // Native finalizer fallback group count (`cfg+0xbc`): 12 at the 160
        // 28-unit fallthrough. The 29..31 round-up below still yields 16, as
        // do full-band rates.
        tone_group_count: gain_band_count,
        selector: selector as u32,
        sample_rate: ATRAC3PLUS_352.sample_rate() as i32,
        header_flags_1dc: flags_1dc,
        // The zeroing gate word `*(obj+0x30)+0x1c` (objside_1c, 0 at 352).
        object_mode_1c: init_state.channels[0].objside_1c as u32,
        primary_tone_activity: &zeroth_aux.primary_tone_activity,
        secondary_tone_activity: &zeroth_aux.secondary_tone_activity,
        cross_primary_flags,
        cross_secondary_flags,
        quant_state: 0,
        // The quant candidate count `sa_nencodetbls[encode_selector]`
        // (decompile 36692). The 352-path shared encode selector is 1
        // (pinned by `zeroth_io_trace` `encode_selector_u32`), so this is
        // `sa_nencodetbls[1] == 8`.
        quant_candidate_count: crate::tables::at5::nencodetbls_at5()[CALC_BRIDGE_ENCODE_SELECTOR]
            as usize,
        // `cfg+0xb4` per-frame effective band_index (29 or 32 at 192): the
        // active-count trim scans down from here over the 32-wide word-length rows
        // (tail already zeroed by finalize), so `active_b0` <= band_index — this
        // is what flips the packed QU count 29↔32 per frame (docs/13 §3.1 slice 3).
        shared_band_count_b4: quant_units,
        // side+0x8c == 1 on the 352 path: score the IDSF leaf from rows. The
        // group count is the per-FRAME `+0xb8` shape-count law (cfg_shape_count_b8):
        // 9 at band_index 27 (128), 10 at 28/29/32 (160/192/256/320/352).
        idsf_input: ZerothIdsfInput::FromRows {
            group_count: cfg_shape_count_b8(band_index) as usize,
        },
        idct_bandwidth_mode: CALC_BRIDGE_CONFIG_90 as usize,
        gha_bits: zeroth_aux.gha_bits,
        // Native words at mode-3 (producer `band_join`), init's zero row at
        // mode-2 (see `tone_primary_effective` above).
        tone_primary_words: &tone_primary_effective,
        tone_secondary_words: &zeroth_aux.tone_secondary_words,
        tone_flag_25: zeroth_aux.tone_flag_25,
        // relax gate `*(obj+0x30)+0x14` (objside_14, 0 at 352).
        relax_gate_zero: init_state.channels[0].objside_14 == 0,
        frame_bit_budget: budget,
    };
    let zeroth = zeroth_bit_allocation_frame_at5(&mut zeroth_state)?;

    let active_b0 = zeroth.active_counts.active_band_count as u32;
    let level_groups = zeroth.active_counts.group_count as u32;
    let calc_bands = zeroth.band_shape.word_length_count;

    // --- Pass 3: assemble the per-channel calc entry surface. ---
    let mut channels = Vec::with_capacity(n);
    for ci in 0..n {
        let ist = &init_state.channels[ci];
        let iout = &init_out.channels[ci];
        let zc = &zeroth.channels[ci];
        let norm = &norms[ci];
        // gain rows: obj+0x8 (detector array A) / obj+0xc (empty gainB).
        let prev_gain_08 = gain_rows_from_records(&ist.gain_a_records);
        let cur_gain_0c = gain_rows_from_records(&ist.gain_b_records);

        // block+0x02 head row AFTER the zeroth relax rule: word0 ++ the
        // (possibly relaxed) max word-length row.
        let mut max_wl_02 = Vec::with_capacity(zc.max_word_lengths.len() + 1);
        max_wl_02.push(iout.word0);
        max_wl_02.extend_from_slice(&zc.max_word_lengths);

        // a48 ch1 spectrum sign flip (decompile 36209-36214): the native
        // `param_5 == 3` arm's a48 sub-path flips the sign byte (`^0x80` on
        // byte 3, == IEEE negate) of ch1's NORMALIZED-spectrum f32 lines for
        // every unit of a band whose subloop_4 join fired — exactly the bands
        // with `band_join[band] != 0` (the later masking loop sets only x94,
        // never band_join, and never flips). Native flips `param_2[1]`, the
        // per-channel norm scratch consumed by calc/quant; here that is ch1's
        // calc-entry `spectrum` (= `norm.spectrum`). Only ch1 at mode_a == 3
        // (producer `Some`) is touched, so ch0 and 320/352 (mode-2) stay
        // byte-exact. The producer already consumed the pre-flip `a_a48`
        // surface above, so flipping this clone here is order-safe.
        let mut spectrum = norm.spectrum.clone();
        if ci == 1 {
            if let Some(out) = &producer_out {
                // `band_join` is per-band (16 bands); `y[band]..y[band+1]` are
                // its units, `isps[u]..isps[u+1]` a unit's normalized lines.
                for band in 0..out.band_join.len() {
                    if out.band_join[band] == 0 {
                        continue;
                    }
                    let unit_lo = usize::from(y[band]);
                    let unit_hi = usize::from(y[band + 1]);
                    for u in unit_lo..unit_hi {
                        let line_lo = usize::from(isps[u]);
                        let line_hi = usize::from(isps[u + 1]);
                        for line in &mut spectrum[line_lo..line_hi] {
                            *line = -*line;
                        }
                    }
                }
            }
        }

        channels.push(CalcChannelEntry {
            max_wl_02,
            activity_14c: zc.activity_copy.clone(),
            base_weights_1cc: zc.base_weights_1cc.clone(),
            idsf_cc: norm.idsf_cc.clone(),
            scale_24c: norm.scale_24c.clone(),
            aux_3cc: iout.weight_3cc.clone(),
            slot_46: zc.slot_46.clone(),
            // Init Block M word-length seed (block+0x4c), read only by the
            // low-selector (< 0x10) ladder on the 64/48 kbps path.
            b_4c: iout.word_lengths_4c.clone(),
            idct_9f8: zc.idct_9f8.clone(),
            plane_b08: zc.plane_b08.clone(),
            // The zeroth writes only the +0xb08 plane; +0xd88 stays zero
            // (verified all-zero at every captured calc entry).
            plane_d88: vec![0u32; CALC_BRIDGE_PLANE_WORDS],
            // Same cfg +0x50 row init already consumed for the per-group
            // stereo swap (native init 0x3f870 / decompile 34716). Calc's
            // section-12 pwc and adjust read that unchanged config row; do
            // not replace the live delayed tone-secondary flags with zeros.
            config_50: zeroth_aux
                .tone_secondary_words
                .iter()
                .map(|&value| value as u32)
                .collect(),
            config_90: CALC_BRIDGE_CONFIG_90,
            config_a8: CALC_BRIDGE_CONFIG_A8,
            config_ac: ATRAC3PLUS_352.sample_rate(),
            config_b0: active_b0,
            // Per-FRAME `+0xb8` shape-count law (9 at band_index 27 / 128, else 10).
            config_b8: cfg_shape_count_b8(band_index),
            config_c0: level_groups,
            config_c4: calc_bands as u32,
            cur_gain_0c,
            prev_gain_08,
            mode_1074: 0,
            o_1b578: iout.obj_1b578.clone(),
            o_1b5f8: zc.word_lengths.clone(),
            o_1b678: iout.idsf_1b678.clone(),
            o_1b6f8: vec![0i16; CALC_BRIDGE_QUANTIZED_WORDS],
            // Calc loads `y_index` through the channel's cfg pointer; the
            // pointee is sigproc detector word 0. Native calc-entry traces pin
            // 10/11/14/16 at selectors 24/25/27/30. Keep init's 352-scoped
            // aux value untouched in this slice and substitute the live word
            // only on the computed calc surface; boundary wrappers without a
            // frontend retain their trace-fed `ist.y_index`.
            y_index: frontend
                .and_then(|state| state.sigproc.detector_words.first())
                .map_or(ist.y_index, |&word| word as i32),
            objside_14: ist.objside_14,
            objside_1c: ist.objside_1c,
            spectrum,
        });
    }

    // Calc SHARED +0x94/+0xd4 are the FINAL rows after zeroth's stereo
    // cross-zero loop, not the producer/init entry rows. Native
    // zeroth_bit_allocation_at5 clears +0x94 when either channel is zero
    // (0x434f5) and sets +0xd4 when ch1 is zero while ch0 is live (0x43584),
    // then calc consumes those mutated rows. Route the composed zeroth output.
    // The producer's per-band `out.band_join` is now wired: it feeds the cfg
    // group-2 side words (`tone_primary_effective`, returned below and packed by
    // `compute_output_frame`), the ported `zeroth_tone_side_bits_at5` accounting,
    // and the a48 ch1 spectrum sign flip (above).
    // TODO(docs/13 §2.3): route the producer's merged `out.scale_factors` into
    // the calc `o_1b678` (a coding-VALUE change). The (ff) live test proved this
    // is a no-op for antiphase (ch1 word lengths all-zero), so it stays deferred;
    // `o_1b678` keeps init's `idsf_1b678`.
    let (shared_row_94, shared_row_d4) = calc_entry_cross_rows_from_zeroth_at5(
        &zeroth.cross_primary_flags,
        &zeroth.cross_secondary_flags,
    );

    let entry = CalcFrameEntry {
        channels,
        // The computed driver replaces this with its rolling object state.
        // Fifteen is also the native post-priming value: section 12's
        // `local_cf4 == 0` arm writes the five owned words to 0xf before the
        // first output-bearing call.
        prior_level_words: vec![vec![15; 8]; n],
        selector: selector as u32,
        budget,
        ctx_flags_1dc: flags_1dc,
        ctx_quant_band_b4: calc_bands as i32,
        ctx_active_b0: active_b0 as i32,
        ctx_level_groups_c0: level_groups as i32,
        ctx_field_90: 1,
        ctx_field_c4: calc_bands as i32,
        shared_word_84: 1,
        shared_word_88: 0,
        shared_word_8c: 1,
        shared_word_90: 4,
        shared_row_94,
        shared_row_d4,
        // Computed by the zeroth pass: +0x11a seed and the +0x11c/+0x11e/
        // +0x12a/+0x12e shared bit words.
        shared_s_11a: zeroth.side.idwl_bits_11a,
        shared_s_11c: zeroth.side.idsf_bits_11c,
        shared_s_11e: zeroth.side.idct_bits_11e,
        shared_s_12a: zeroth.totals.base_total_12a,
        shared_s_12e: zeroth.totals.extended_total_12e,
    };
    Ok((entry, init_gain_headers, tone_primary_effective))
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::dsp::sigproc::gain_detect_prune_markers_at5;
    use crate::dsp::time2freq::{TIME2FREQ_BANDS_AT5, TonalityChannel};

    #[test]
    fn assemble_channel_rejects_detector_outcome_overflow() {
        let overflow = CODING_BRIDGE_GAIN_BAND_COUNT + 1;
        let output = Time2FreqChannelOutput {
            spectra: vec![0.0; CODING_BRIDGE_SPECTRUM_WORDS],
            delayed_out: vec![0.0; CODING_BRIDGE_SPECTRUM_WORDS],
            final_records: vec![[0; CODING_BRIDGE_POINT_WORDS]; overflow],
            tonality: TonalityChannel {
                flags: [false; TIME2FREQ_BANDS_AT5],
                tonality: [1.0; TIME2FREQ_BANDS_AT5],
                scales: [1.0; TIME2FREQ_BANDS_AT5],
            },
            band_outcomes: Vec::new(),
            detector_outcomes: gain_detect_prune_markers_at5(overflow, &vec![true; overflow]),
            final_plane_rows: None,
        };
        let aux = CodingBridgeChannelAux {
            objside_1c: 0,
            objside_14: 0,
            objside_ptr: 0,
            spec_b_ptr: 0,
            y_index: 0,
            b_9c8: vec![0; CODING_BRIDGE_SEED_WORDS],
            b_a48: vec![0.0; CODING_BRIDGE_SEED_WORDS],
            gain_a_record_tails: vec![
                [0; CODING_BRIDGE_GAIN_TAIL_WORDS];
                CODING_BRIDGE_GAIN_BAND_COUNT
            ],
            gain_b_records: vec![
                0;
                CODING_BRIDGE_GAIN_BAND_COUNT * CODING_BRIDGE_GAIN_RECORD_WORDS
            ],
        };

        let error = match assemble_channel(0, &output, &aux, CODING_BRIDGE_GAIN_BAND_COUNT) {
            Ok(_) => panic!("detector outcome overflow should be rejected"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            CodingBridgeError::ShapeMismatch {
                field: "detector_outcomes",
                channel: 0,
                expected: CODING_BRIDGE_GAIN_BAND_COUNT,
                actual: overflow,
            }
        );
    }
}
