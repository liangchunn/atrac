//! Composed `at5enc_sigproc` frame-front driver (docs/05 Step 4.3 exit,
//! docs/09 Phase 3 Slice C).
//!
//! Threads the whole encoder front per core call in native order:
//!
//! 1. `sigproc_frame_at5` — state-rotation prologue (detector history, the
//!    five-slot channel pointer ring at `+0x14..+0x24`, flag shift), band-slot
//!    roll, PQF analysis into slot 8, the stereo dB metric, band-limit
//!    writeback, and the stereo swap decision (native `0x4f2b0`).
//! 2. `extract_ghwave_at5` — the live GHA whole-boundary driver, when the gate
//!    `param_2[6] != 0 && param_2[5] == 0` is open (native call site
//!    `0x5054f`). Its output rows/records land in the current channel's
//!    `*(obj+0x24)` arena and its residual subtraction mutates slot 0 of every
//!    band window.
//! 3. the caller can then hand the rolled slot-0..1 windows to `time2freq_at5`
//!    (native call site `0x50623`).
//!
//! The five-slot pointer ring at each channel object `+0x14..+0x24` (rotated
//! once per frame by `sigproc_rotate_channel_pointers_at5`) is the arena ring:
//! extract writes to `*(obj+0x24)` (ring slot 4), reads its delayed row/record
//! state from `*(obj+0x20)` (ring slot 3, the previous frame's output), and the
//! packer reads `*(obj+0x14)` (ring slot 0). Because the ring rotates left once
//! per frame and holds five slots, the packer at core call `N` reads the
//! extract output written four calls earlier at core call `N-4` (overwritten at
//! `N+1`). This driver reproduces that rotation with an owned per-channel arena
//! ring so that, after running core calls 0..7 from PCM, the slot the packer
//! reads at call 7 holds core call 3's extract output.

use crate::dsp::scalar::cp_short_to_scalar_at5;
use crate::dsp::set_gainc::{
    SET_GAINC_BANDS, SET_GAINC_HISTORY_A_FLOATS, SET_GAINC_HISTORY_B_FLOATS,
    SET_GAINC_SCRATCH_FLOATS, SetGaincPlane, SetGaincRow,
};
use crate::dsp::sigproc::{
    GAIN_DETECT_BAND_WINDOW_VALUES, GAIN_DETECT_PEAK_BINS, GainDetectScratch,
};
use crate::dsp::sigproc_shell::{
    SIGPROC_BAND_COUNT, SIGPROC_BAND_SLOT_FLOATS, SIGPROC_BAND_SLOTS, SIGPROC_CHANNEL_RING_SLOTS,
    SIGPROC_DETECTOR_ARENA_WORDS, SIGPROC_PQF_DELAY_FLOATS, SigprocChannelPointers,
    SigprocFrameError, SigprocFrameParams, SigprocFrameReport, SigprocFrameState,
    sigproc_frame_at5,
};
use crate::dsp::time2freq::{
    TIME2FREQ_POINT_WORDS, Time2FreqChannelOutput, Time2FreqChannelState,
    Time2FreqDetectorBandSeed, Time2FreqError, Time2FreqParams, Time2FreqSetGaincChannel,
    Time2FreqSetGaincState, time2freq_at5, time2freq_at5_with_set_gainc,
    time2freq_detector_seed_evolve_at5, time2freq_encode_at5,
};
use crate::gha::extract::{
    EXTRACT_GHA_ROW_WORD_COUNT_AT5, GhaExtractError, GhaExtractInput, GhaExtractOutput,
    extract_ghwave_at5,
};
use crate::gha::synthesis::GhaWaveRecord;

pub const FRONTEND_FRAME_SAMPLES: usize = 2048;

#[derive(Default)]
pub(crate) struct FrontendScratch {
    gain_detect: GainDetectScratch,
}
/// First rolled slot of the 384-sample band window `extract_ghwave_at5` reads.
/// The slot ring rolls newest→slot 8, oldest→slot 0; `extract_ghwave_at5`
/// reads slots 4..6 (empirically: reproduces the native extract-output timing
/// so the packer's ring-delayed read at call 7 matches core call 3's output).
pub const FRONTEND_EXTRACT_FIRST_SLOT: usize = 4;
/// The 384-sample band window `extract_ghwave_at5` reads per band (slots 4..6).
pub const FRONTEND_EXTRACT_WINDOW_SLOTS: usize = 3;
/// The 256-sample MDCT window `time2freq_at5` reads per band (slots 0..1).
pub const FRONTEND_TIME2FREQ_WINDOW_SLOTS: usize = 2;
/// The residual pass subtracts the first `0x80` samples (slot 4) in place.
pub const FRONTEND_RESIDUAL_SAMPLES: usize = 0x80;

/// The 352 kbps shell parameters pinned by `sigproc_shell_trace`:
/// `param_3 = 2` channels, `param_4 = 2` (mode), `param_5 = 0x20` band limit,
/// and the always-open GHA gate.
pub const FRONTEND_CHANNEL_COUNT: usize = 2;
pub const FRONTEND_MODE: u32 = 2;
pub const FRONTEND_BAND_LIMIT: i32 = 0x20;
/// Active GHA/time2freq band count (`g_a_x_at5[0x20] + 1`).
pub const FRONTEND_BAND_COUNT: usize = 16;
/// `extract_ghwave_at5`'s `param_3` (ecx threshold/profile, header word `0x7a`).
/// This is the ATRAC3plus block **selector** (`cfg+0x1e8`), 30 at 352. The live
/// path threads the per-rate value via [`FrontendState::selector`]; this named
/// constant is the 352 value used by the trace-fed boundary tests and as the
/// [`FrontendState::new_zeroed`] default. 29 at 320 (docs/13 §1.1).
pub const FRONTEND_EXTRACT_PARAM_3: i32 = 30;

/// Slot-0-base float offset of the 140-float detector front window
/// (native `local_394c + 0x7d0`; slot 3 floats `116..128` ++ slot 4).
pub const FRONTEND_DETECTOR_WINDOW_OFFSET: usize = 500;

/// `time2freq_at5` `param_6` (= `mode_a`) for the 352 target (pinned by
/// `time2freq_trace`): 2 at 320/352. The live path threads the per-rate `mode_a`
/// via [`FrontendState::sigproc_mode`] (3 at 48-256 — where the `param_6 == 3`
/// stereo flag-reconcile + 33042 harmonization go live, docs/13 §5.2 (jjj));
/// this named constant is the 352 value (= its `mode_a`) kept for reference.
pub const FRONTEND_TIME2FREQ_PARAM6: i32 = 2;
/// `time2freq_at5`'s stack "bandwidth" arg — the SAME ATRAC3plus block selector
/// (`cfg+0x1e8`) `at5enc_sigproc` passes to both `extract_ghwave_at5` and
/// `time2freq_at5` (disassembly `0x4f2f3` load → `0x50605` push). 30 at 352,
/// threaded per-rate via [`FrontendState::selector`]; kept as the 352 constant
/// for the trace-fed boundary tests and the [`FrontendState::new_zeroed`]
/// default (docs/13 §1.1).
pub const FRONTEND_TIME2FREQ_BANDWIDTH: i32 = 30;
/// `time2freq_at5` `mode_cc` (`cfg+0xcc`) for the 352 target: 1 (the
/// `detect_gainc_data_new_at5` chain). The live path threads the per-rate value
/// via [`FrontendState::mode_cc`] (0 at 48/64 → the `set_gainc_at5` dispatch,
/// docs/13 §5.2); this named constant is the 352 value.
pub const FRONTEND_TIME2FREQ_MODE_CC: i32 = 1;

/// Number of all-zero flush core calls `at3tool` appends after the PCM encode
/// pin; the length-agnostic flush count is
/// [`flush_processing_calls_352`] (8 or 9, content-independent — decided by the
/// final encode call's sample count).
pub const FRONTEND_FLUSH_CALLS: usize = 8;

/// The number of all-zero PCM-processing flush core calls `at3tool` appends
/// after the encode calls, as a function of the FINAL encode call's sample count
/// `n_last`. `atx_encode_core` (native `0x559f0`; Ghidra decompile comment
/// `0x659f0` = native + 0x10000; decompiled/libatrac.c lines 46032-46038) stores
/// `state+0x1c = ((n + 0x396f) >> 11) + 1` on EVERY encode call with `n > 0`, so
/// the LAST encode call's sample count decides the drain length: 8 for
/// `n_last ∈ [1, 1680]`, 9 for `n_last ∈ [1681, 2048]` (incl. the full 2048-sample
/// final call of an exact-multiple input). `atx_flush_encode` (native `0x5a240`;
/// decompile 47805-47833) then drains that many all-zero frames.
///
/// Pinned by the `len_edges_run.json` N=7824 (n_last 1680 → 8) vs N=7825
/// (n_last = 464) this is [`FRONTEND_FLUSH_CALLS`] = 8.
pub fn flush_processing_calls_352(final_call_sample_frames: usize) -> usize {
    ((final_call_sample_frames + 0x396f) >> 11) + 1
}

/// Errors from preparing one native-width PCM core call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentPcmFrameError {
    /// A core call can consume at most 2048 valid samples per channel.
    TooManySamples { maximum: usize, actual: usize },
    /// The requested valid prefix is outside one source channel.
    SourceRange {
        channel: usize,
        source_offset: usize,
        valid_sample_frames: usize,
        channel_len: usize,
    },
}

/// Prepare exactly one fixed-width scalar PCM frame for a computed core call.
///
/// `source_offset..source_offset + valid_sample_frames` is copied from every
/// channel into a fresh zeroed 2048-sample `i16` frame, then converted through
/// the native-pinned [`cp_short_to_scalar_at5`] leaf. The fresh zeroing preserves
/// at3tool's `memset`-before-`fread_pcm` law for both final partial calls and
/// processing-flush calls (`valid_sample_frames == 0`). The returned storage is
/// bounded by `channels.len() * FRONTEND_FRAME_SAMPLES` and contains no past or
/// future core calls.
pub fn prepare_current_pcm_frame(
    channels: &[&[i16]],
    source_offset: usize,
    valid_sample_frames: usize,
) -> Result<Vec<Vec<f32>>, CurrentPcmFrameError> {
    if valid_sample_frames > FRONTEND_FRAME_SAMPLES {
        return Err(CurrentPcmFrameError::TooManySamples {
            maximum: FRONTEND_FRAME_SAMPLES,
            actual: valid_sample_frames,
        });
    }
    let Some(source_end) = source_offset.checked_add(valid_sample_frames) else {
        return Err(CurrentPcmFrameError::SourceRange {
            channel: 0,
            source_offset,
            valid_sample_frames,
            channel_len: channels.first().map_or(0, |channel| channel.len()),
        });
    };
    for (channel_index, channel) in channels.iter().enumerate() {
        if source_end > channel.len() {
            return Err(CurrentPcmFrameError::SourceRange {
                channel: channel_index,
                source_offset,
                valid_sample_frames,
                channel_len: channel.len(),
            });
        }
    }

    let mut prepared = Vec::with_capacity(channels.len());
    for channel in channels {
        let mut pcm = [0i16; FRONTEND_FRAME_SAMPLES];
        pcm[..valid_sample_frames].copy_from_slice(&channel[source_offset..source_end]);
        let mut scalar = [0.0f32; FRONTEND_FRAME_SAMPLES];
        cp_short_to_scalar_at5(&mut scalar, &pcm, FRONTEND_FRAME_SAMPLES)
            .expect("fixed 2048-sample frame converts");
        prepared.push(scalar.to_vec());
    }
    Ok(prepared)
}

/// Per-core-call scalar PCM frames for the whole 352 kbps encode of a stereo
/// PCM stream of ANY length, matching `at3tool`'s driver byte-for-byte.
///
/// Native sources:
/// - `at3tool` memsets the whole input buffer before every `fread_pcm`
///   (decompiled/at3tool.c ~2114-2141), so the final partial encode call is
///   zero-padded to a full [`FRONTEND_FRAME_SAMPLES`] frame. There is NO extra
///   partial encode call when `len % 2048 == 0` (`len_edges_run.json` N=6144/20480
///   → `final_call_input_bytes == 8192`).
/// - `atx_flush_encode` (decompile 47805..47833) then runs
///   [`flush_processing_calls_352`] all-zero frames; `cp_short_to_SCALAR_at5`
///   always converts 0x800 samples (decompile 47796).
///
/// The result has `ceil(len / 2048) + flush_processing_calls_352(n_last)` entries,
/// each a `[left, right]` pair of `FRONTEND_FRAME_SAMPLES` scalar (f32) samples.
/// `76 + 8 = 84` core calls, matching `core_boundary_trace.ndjson` (calls 0..74
/// sample_count 2048, call 75 sample_count 464, calls 76..83 sample_count 0).
///
/// Thin stereo wrapper over the channel-count-generic [`core_call_pcm_frames`]
/// (docs/14 §0.4): the chunking/zero-pad/flush law is channel-independent
/// (MEASURED — the mono init-word oracle records the same 84-core-call schedule
/// this reshapes the generic per-channel output back to the shipped
/// `[left, right]` pair for every existing stereo caller. The delegation is
/// structural, so stereo output is byte-identical.
pub fn core_call_pcm_frames_352(left: &[i16], right: &[i16]) -> Vec<[Vec<f32>; 2]> {
    core_call_pcm_frames(&[left, right])
        .into_iter()
        .map(|channels| {
            let mut it = channels.into_iter();
            let left = it.next().expect("stereo supply has a left channel");
            let right = it.next().expect("stereo supply has a right channel");
            [left, right]
        })
        .collect()
}

/// Channel-count-generic per-core-call scalar PCM chunker (docs/14 §0.4): the
/// same driver law as [`core_call_pcm_frames_352`] for ANY channel count
/// (`channels.len()` in `{1, 2}` for this crate). `total = min(channel lens)`,
/// `ceil(total / 2048)` encode calls (each 2048 samples, the last zero-padded
/// per the `memset`-before-`fread_pcm` law), then
/// `flush_processing_calls_352(n_last)` all-zero flush calls; each sample is
/// converted with `cp_short_to_scalar_at5`. The core-call schedule is derived
/// from `total` alone and is channel-independent (native `atracEncCore`:
/// `n×channels×2` bytes per call, same memset zero-padding as stereo, E3).
///
/// Returns `Vec<Vec<Vec<f32>>>` indexed `[core_call][channel][sample]`. All five
/// mono shipping paths — 128 kbps (docs/14 §1.3), 96 kbps (docs/14 §2.1),
/// 64 kbps (docs/14 §3.1), 48 kbps (docs/14 §4.1), and 32 kbps (docs/14 §5.1) —
/// supply their single channel here and step the resulting carrier through the
/// channel-vec computed pipeline.
pub fn core_call_pcm_frames(channels: &[&[i16]]) -> Vec<Vec<Vec<f32>>> {
    let channel_count = channels.len();
    let total = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    let encode_calls = total.div_ceil(FRONTEND_FRAME_SAMPLES).max(1);
    let final_call_sample_frames = total - FRONTEND_FRAME_SAMPLES * (encode_calls - 1);
    let flush_calls = flush_processing_calls_352(final_call_sample_frames);
    let core_calls = encode_calls + flush_calls;

    let mut out = Vec::with_capacity(core_calls);
    for call in 0..core_calls {
        let mut per_channel = Vec::with_capacity(channel_count);
        for &channel in channels {
            // `memset` before each `fread_pcm`: every call starts fully zeroed, so
            // the tail-partial and flush calls are zero-padded implicitly.
            let mut i16_buf = vec![0i16; FRONTEND_FRAME_SAMPLES];
            if call < encode_calls {
                let base = call * FRONTEND_FRAME_SAMPLES;
                let available = total.saturating_sub(base).min(FRONTEND_FRAME_SAMPLES);
                i16_buf[..available].copy_from_slice(&channel[base..base + available]);
            }
            let mut f = vec![0.0f32; FRONTEND_FRAME_SAMPLES];
            cp_short_to_scalar_at5(&mut f, &i16_buf, FRONTEND_FRAME_SAMPLES)
                .expect("full 2048-sample frame converts");
            per_channel.push(f);
        }
        out.push(per_channel);
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrontendError {
    Sigproc(SigprocFrameError),
    Extract(GhaExtractError),
    Time2Freq(Time2FreqError),
    PcmShape {
        needed: usize,
        actual: usize,
    },
    ChannelCount {
        channel_count: usize,
    },
    /// A previous core call errored and left the rolling frontend state
    /// (detector seeds, ring, PQF delay) torn. Every subsequent call fails
    /// explicitly and cheaply with this variant instead of computing on the
    /// torn state and silently producing wrong output (docs/12 §2.2). `at`
    /// names the poisoning point ("run_time2freq", "sigproc", "extract").
    Poisoned {
        at: &'static str,
    },
}

impl From<SigprocFrameError> for FrontendError {
    fn from(error: SigprocFrameError) -> Self {
        FrontendError::Sigproc(error)
    }
}

impl From<GhaExtractError> for FrontendError {
    fn from(error: GhaExtractError) -> Self {
        FrontendError::Extract(error)
    }
}

impl From<Time2FreqError> for FrontendError {
    fn from(error: Time2FreqError) -> Self {
        FrontendError::Time2Freq(error)
    }
}

/// channels, bands, and rates; `set_gainc_io_trace` call 0, test_64/test_48
/// sha1-identical). Every prev-plane row is all-zero except word 18 (`+0x48`),
/// words 28/29 (`+0x70`/`+0x74`), word 31 (`+0x7c`), word 33 (`+0x84`) = 4.0f,
/// and words 36/37 (`+0x90`/`+0x94`) = 128.0f. History A is `[4.0; 32] ++ [0.0]`,
/// history B is `[4.0; 32]`.
fn set_gainc_init_prev_row() -> SetGaincRow {
    let mut row: SetGaincRow = [0u32; crate::dsp::set_gainc::SET_GAINC_ROW_WORDS];
    let f4 = 4.0f32.to_bits();
    let f128 = 128.0f32.to_bits();
    row[0x48 / 4] = f4;
    row[0x70 / 4] = f4;
    row[0x74 / 4] = f4;
    row[0x7c / 4] = f4;
    row[0x84 / 4] = f4;
    row[0x90 / 4] = f128;
    row[0x94 / 4] = f128;
    row
}

fn set_gainc_init_history_a() -> [f32; SET_GAINC_HISTORY_A_FLOATS] {
    let mut history = [4.0f32; SET_GAINC_HISTORY_A_FLOATS];
    history[SET_GAINC_HISTORY_A_FLOATS - 1] = 0.0;
    history
}

/// The all-zero core-call-0 detector band seed. Every persistent surface is
/// zero (the native `malloc(0x1caf0)` channel block) except
/// `prev_peak_slot_plus_32`, which is 32: the native entry adds `+0x20` to the
/// zero-stored peak slot before the peak span runs
/// (`detect_gainc_data_new_at5` line 31555). `band_window` /
/// `current_bin0_peak` are refreshed from the live band scratch each call.
fn zeroed_detector_seed() -> Time2FreqDetectorBandSeed {
    let slab_words = crate::dsp::gain::GC_SET_POINTS_OUTPUT_GROUPS
        * crate::dsp::gain::GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS;
    Time2FreqDetectorBandSeed {
        band_window: vec![0.0f32; GAIN_DETECT_BAND_WINDOW_VALUES],
        spectrum: vec![0.0f32; 2 * GAIN_DETECT_PEAK_BINS],
        envelope: vec![0.0f32; 2 * GAIN_DETECT_PEAK_BINS],
        prev_max_slot: 0,
        prev_peak_slot_plus_32: GAIN_DETECT_PEAK_BINS,
        prev_level_a: 0.0,
        prev_level_b: 0.0,
        stored_peak_a: 0.0,
        current_bin0_peak: 0.0,
        carried_removed_count: 0,
        persistent_records: Vec::new(),
        output_records: vec![0u32; slab_words],
        counts: vec![0i32; crate::dsp::gain::GC_SET_POINTS_OUTPUT_GROUPS],
    }
}

/// One channel's GHA arena as stored in a ring slot: the `*(obj+0x24)+4`
/// output rows and their per-band wave records, plus the header scalars the
/// residual pass and the packer read. Freshly allocated slots hold all-zero
/// rows and no records (the "no previous frame" delayed state).
///
/// The `shared`/`opposite` per-band flags and `header_band_count` are written
/// into the same extract OUTPUT arena root (`*(obj+0x24)`, ring slot 4) as the
/// rows/records, so they travel WITH the arena through the 5-slot ring and are
/// ring-delayed to pack time exactly like the row/record content: the flags the
/// packer reads at core call `N` are the share gates + header word extract
/// computed at core call `N-4`.
///
/// Native evidence (docs/11 §2.1 slice 2.1c, E1/E2):
/// - `extract_ghwave_at5` (decompile ~41560) resolves the output arena root at
///   line 41609 (`iVar15 = **(int **)(*param_1 + 0x24)`).
/// - The per-band share gates (decompile ~41719-41746, stereo path after
///   `check_channel_correlation_at5`) write byte `0x318 + band*4` (shared) and
///   `0x360 + band*4` (opposite/stereo); the mono path (41749-41757) zeroes
///   both. Byte `0x318` == arena word `0xc6` (the packer's per-band shared
///   base), byte `0x360` == arena word `0xd8` (stereo base) — exactly the flag
///   families `serialize_gha_header_block` writes and `pack_gha_header` reads.
/// - The extract tail writes channel-0 header words `[active, mode, band_count]`
///   into the same root; the packer reads `band_count` as `arena_u32(2)`.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendArena {
    pub rows: Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>,
    pub records: Vec<Vec<GhaWaveRecord>>,
    pub header_mode: u32,
    pub header_active: u32,
    /// Header word 2 (`arena_root[2]`, the packer's `arena_u32(2)` nbands). The
    /// extract tail writes channel-0's `[active, mode, band_count]` here.
    pub header_band_count: u32,
    /// Per-band shared-flag family (arena word `0xc6 + band`, byte `0x318`):
    /// the extract share gate output (`GhaExtractOutput.shared`).
    pub shared: Vec<u32>,
    /// Per-band stereo/opposite-flag family (arena word `0xd8 + band`, byte
    /// `0x360`): the extract share gate output (`GhaExtractOutput.opposite`).
    pub opposite: Vec<u32>,
}

impl FrontendArena {
    fn empty(band_count: usize) -> Self {
        // The native arena is a calloc'd fresh block, so a freshly allocated
        // slot's flags/header word are all zero (the "no previous frame"
        // delayed state).
        FrontendArena {
            rows: vec![[0u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]; band_count],
            records: vec![Vec::new(); band_count],
            header_mode: 0,
            header_active: 0,
            header_band_count: 0,
            shared: vec![0u32; band_count],
            opposite: vec![0u32; band_count],
        }
    }

    /// Per-band wave counts (`row_words[8]`).
    pub fn wave_counts(&self) -> Vec<u32> {
        self.rows.iter().map(|row| row[8]).collect()
    }
}

/// Owned cross-frame encoder-front state. Everything the native shell threads
/// between `at5enc_sigproc` calls: the sigproc scratch (detector words,
/// per-channel pointer sets, band-slot blocks, PQF delay lines, header flag and
/// swap words) plus the per-channel five-slot GHA arena ring.
#[derive(Debug, Clone)]
pub struct FrontendState {
    pub channel_count: usize,
    pub band_count: usize,
    /// The ATRAC3plus block selector (`cfg+0x1e8`) `at5enc_sigproc` loads from
    /// the shared config and passes as `extract_ghwave_at5`'s `param_3` and
    /// `time2freq_at5`'s "bandwidth" arg. 30 at 352, 29 at 320 (docs/13 §1.1).
    /// Threaded here so the live path is per-rate; [`FRONTEND_EXTRACT_PARAM_3`] /
    /// [`FRONTEND_TIME2FREQ_BANDWIDTH`] remain the 352 constants for the
    /// trace-fed boundary tests.
    pub selector: i32,
    /// The `at5enc_sigproc` band limit (`param_5` = `handle+0x1a4` band_index,
    /// `cfg+0xb4`) the shell's band-limit epilogue writes back and hands to
    /// `extract`/`time2freq`. **32** (full-band) at 256/320/352, **29** at 192
    /// (docs/13 §3.1). Feeds [`sigproc_band_limit_writeback_at5`], which derives
    /// the QMF/gain `band_count` (`g_a_x_at5[band_limit] + 1`; 16 full-band, 13 at
    /// 192) the time2freq extent + `+0x1b48c` gain scan read. Set by the per-rate
    /// driver ([`FrameDriver::for_params`]); defaults to
    /// [`FRONTEND_BAND_LIMIT`] (32) so every existing full-band caller is
    /// unchanged.
    pub band_limit: i32,
    /// The per-call `sigproc_frame_at5` mode (`mode_a` / `param_5`, docs/13
    /// §2.3): [`FRONTEND_MODE`] (2) at 320/352 — the default the shell's
    /// `param_4 == 2` stereo band-limit path takes, byte-identical to before —
    /// and 3 at 48-256, where the `param_4 == 3` path writes the intensity
    /// band count (14 at 256) the joint-stereo producer reads. Set by the
    /// per-rate driver ([`FrameDriver::for_params`]); defaults to 2 so
    /// every existing (mode-2) caller is unchanged.
    pub sigproc_mode: u32,
    /// The per-rate GHA-enable config word (`cfg+0xd0`, docs/13 §5.1) the extract
    /// boundary reads as `header_0xd0_enabled`: `true` at 96-352 (the sine/general
    /// analysis arms), `false` at 48/64 (the disabled fallback — the peaked-band
    /// short-circuit to header mode 3, plus the sine mask-1/2 arm which stays live
    /// under `+0xd0 == 0`). Set by the per-rate driver
    /// ([`FrameDriver::for_params`]) from [`CodingParams::gha_enabled`];
    /// defaults to `true` so every existing full-band (96-352) caller is
    /// unchanged. Note this is NOT the rate-independent shell extract gate
    /// (`gha_gate_open`, evidence item 1): that stays `true` at every rate.
    pub gha_enabled: bool,
    /// The low-rate gain-detector mode word (`cfg+0xcc`, docs/13 §5.2) the
    /// `time2freq_at5` dispatch reads: `false` at 48/64 (the `mode_cc == 0`
    /// descending `set_gainc_at5` path), `true` at 96-352 (`detect_gainc_data_new_at5`).
    /// Set by the per-rate driver ([`FrameDriver::for_params`]) from
    /// [`CodingParams::mode_cc`]; defaults to `true` so every existing
    /// (detector-path) caller is unchanged.
    ///
    /// [`CodingParams::mode_cc`]: crate::encoder::coding_params::CodingParams::mode_cc
    /// [`FrameDriver::for_params`]: crate::encoder::frame::FrameDriver::for_params
    pub mode_cc: bool,
    pub sigproc: SigprocFrameState,
    /// `arena_ring[channel]` is the channel object's `+0x14..+0x24` ring:
    /// index 0 is `*(obj+0x14)` (packer read), index 3 is `*(obj+0x20)`
    /// (extract delayed input), index 4 is `*(obj+0x24)` (extract output).
    pub arena_ring: Vec<Vec<FrontendArena>>,
    /// `detector_seeds[channel][band]` is the per-band cross-frame detector
    /// state that lives in the native `malloc(0x1caf0)` channel block
    /// (candidate pool, spectrum/envelope histories, prev scalars, gc slab,
    /// counts). All zero at core call 0 except `prev_peak_slot_plus_32`, which
    /// starts at 32 because the native entry unconditionally adds `+0x20` to
    /// the zero-stored peak slot (`detect_gainc_data_new_at5` line 31555).
    pub detector_seeds: Vec<Vec<Time2FreqDetectorBandSeed>>,
    /// `previous_records[channel][band]` is the prior core call's final point
    /// record (`time2freq_at5`'s prologue cur/prev swap: previous(N) =
    /// final(N-1)). All zero at core call 0.
    pub previous_records: Vec<Vec<[i32; TIME2FREQ_POINT_WORDS]>>,
    /// `set_gainc_history_a[channel][band]` — the persistent 33-float history-A
    /// row the mode_cc==0 `set_gainc_at5` leaf reads+writes per call (native
    /// init (`[4.0; 32] ++ [0.0]`, `set_gainc_io_trace` call 0 test_64/test_48,
    /// sha1-identical). Only threaded on the mode_cc==0 path.
    pub set_gainc_history_a: Vec<Vec<[f32; SET_GAINC_HISTORY_A_FLOATS]>>,
    /// `set_gainc_history_b[channel][band]` — the persistent 32-float history-B
    /// row (native `chobj+0x874+band*0x80`). Pinned init: `[4.0; 32]`.
    pub set_gainc_history_b: Vec<Vec<[f32; SET_GAINC_HISTORY_B_FLOATS]>>,
    /// `set_gainc_prev_plane[channel]` — last frame's post-everything current
    /// gain-record plane (`*(chobj+0xc)`), read by the leaf and (word 18) mutated
    /// by the ch1 stereo pre-adjust. Persisted from each frame's post-stage-5
    /// current plane. Pinned init: every row all-zero except words 18/28/29/31/33
    /// = 4.0 and 36/37 = 128.0.
    pub set_gainc_prev_plane: Vec<SetGaincPlane>,
    /// Set to the poisoning point when a core call errors mid-way and leaves the
    /// rolling state (detector seeds, ring, PQF delay) inconsistent. Once set,
    /// every subsequent `frontend_core_call_at5` fails fast with
    /// `FrontendError::Poisoned` rather than computing on torn state. The run is
    /// dead either way; the point is to fail explicitly and cheaply instead of
    /// silently rolling wrong state forward (docs/12 §2.2). `None` while healthy.
    pub poisoned: Option<&'static str>,
}

impl FrontendState {
    /// The all-zero cross-frame state at core call 0. Verified against
    /// `sigproc_shell_trace`'s call-0 `sigproc_entry` snapshot: detector words,
    /// PQF delay lines, band-slot heads, and the header flag word are all zero.
    pub fn new_zeroed(channel_count: usize) -> Self {
        Self::new_zeroed_for_selector(channel_count, FRONTEND_EXTRACT_PARAM_3)
    }

    /// Like [`new_zeroed`](Self::new_zeroed) but with an explicit per-rate block
    /// selector (`cfg+0x1e8`; 30 at 352, 29 at 320). Used by the per-rate
    /// computed driver (docs/13 §1.1); every other cross-frame surface is
    /// rate-independent so the rest of the zeroed state is identical.
    pub fn new_zeroed_for_selector(channel_count: usize, selector: i32) -> Self {
        let band_count = FRONTEND_BAND_COUNT;
        let block_floats = SIGPROC_BAND_COUNT * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS;
        let sigproc = SigprocFrameState {
            detector_words: vec![0u32; SIGPROC_DETECTOR_ARENA_WORDS],
            channel_pointers: vec![
                SigprocChannelPointers {
                    current_records: 0,
                    previous_records: 0,
                    ring: [0u32; SIGPROC_CHANNEL_RING_SLOTS],
                };
                channel_count
            ],
            band_blocks: vec![vec![0.0f32; block_floats]; channel_count],
            pqf_delay: vec![vec![0.0f32; SIGPROC_PQF_DELAY_FLOATS]; channel_count],
            header_flag_word: 0,
            header_swap_words: vec![0u32; SIGPROC_BAND_COUNT],
        };
        let arena_ring =
            vec![vec![FrontendArena::empty(band_count); SIGPROC_CHANNEL_RING_SLOTS]; channel_count];
        let detector_seeds = vec![vec![zeroed_detector_seed(); band_count]; channel_count];
        let previous_records = vec![vec![[0i32; TIME2FREQ_POINT_WORDS]; band_count]; channel_count];
        let set_gainc_history_a =
            vec![vec![set_gainc_init_history_a(); SET_GAINC_BANDS]; channel_count];
        let set_gainc_history_b =
            vec![vec![[4.0f32; SET_GAINC_HISTORY_B_FLOATS]; SET_GAINC_BANDS]; channel_count];
        let set_gainc_prev_plane =
            vec![[set_gainc_init_prev_row(); SET_GAINC_BANDS]; channel_count];
        FrontendState {
            channel_count,
            band_count,
            selector,
            // Default full-band (32); the per-rate driver overrides this to
            // `params.band_index` (29 at 192) after construction (docs/13 §3.1).
            band_limit: FRONTEND_BAND_LIMIT,
            // Default to the mode-2 stereo path; the per-rate driver overrides
            // this to `mode_a` (3 at 48-256) after construction (docs/13 §2.3).
            sigproc_mode: FRONTEND_MODE,
            // Default GHA-enabled (`cfg+0xd0 != 0`, the 96-352 path); the per-rate
            // driver overrides this to `params.gha_enabled` (false at 48/64) after
            // construction, mirroring the band_limit/sigproc_mode pattern
            // (docs/13 §5.1).
            gha_enabled: true,
            // Default to the mode_cc != 0 (detector) path; the per-rate driver
            // overrides this to `params.mode_cc` (false at 48/64) after
            // construction, mirroring the gha_enabled pattern (docs/13 §5.2).
            mode_cc: true,
            sigproc,
            arena_ring,
            detector_seeds,
            previous_records,
            set_gainc_history_a,
            set_gainc_history_b,
            set_gainc_prev_plane,
            poisoned: None,
        }
    }

    fn band_block_slice(
        &self,
        channel: usize,
        band: usize,
        first_slot: usize,
        slots: usize,
    ) -> &[f32] {
        let start = band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS
            + first_slot * SIGPROC_BAND_SLOT_FLOATS;
        &self.sigproc.band_blocks[channel][start..start + slots * SIGPROC_BAND_SLOT_FLOATS]
    }

    /// The 384-sample (slots 4..6) band windows `extract_ghwave_at5` reads.
    pub fn extract_band_windows(&self) -> Vec<Vec<Vec<f32>>> {
        (0..self.channel_count)
            .map(|channel| {
                (0..self.band_count)
                    .map(|band| {
                        self.band_block_slice(
                            channel,
                            band,
                            FRONTEND_EXTRACT_FIRST_SLOT,
                            FRONTEND_EXTRACT_WINDOW_SLOTS,
                        )
                        .to_vec()
                    })
                    .collect()
            })
            .collect()
    }

    /// The 256-sample (slots 0..1) MDCT windows `time2freq_at5` reads, with the
    /// GHA residual subtraction already applied (four calls earlier, at slot 4,
    /// then rolled down to slot 0).
    pub fn time2freq_band_inputs(&self, channel: usize) -> Vec<Vec<f32>> {
        (0..self.band_count)
            .map(|band| {
                self.band_block_slice(channel, band, 0, FRONTEND_TIME2FREQ_WINDOW_SLOTS)
                    .to_vec()
            })
            .collect()
    }

    /// The 133-float `set_gainc_at5` scratch (native `param_2` reads floats
    /// `[+0x3fc, +0x610)` only) for one (channel, band): slot-0-based band-block
    /// floats `255..388` (slot 1 tail float 255, slot 2 floats 256..384, slot 3
    /// head floats 384..388). `scratch[0]` = native `param_2+0x3fc`.
    pub fn set_gainc_scratch(
        &self,
        channel: usize,
        band: usize,
    ) -> [f32; SET_GAINC_SCRATCH_FLOATS] {
        let base = band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS + 255;
        let mut scratch = [0.0f32; SET_GAINC_SCRATCH_FLOATS];
        scratch.copy_from_slice(
            &self.sigproc.band_blocks[channel][base..base + SET_GAINC_SCRATCH_FLOATS],
        );
        scratch
    }

    /// The 140-float detector front window (native `local_394c + 0x7d0`):
    /// slot-0-base floats `500..640` (slot 3's last 12 floats ++ slot 4). Read
    /// post-residual: the extract residual has already overwritten slot 4 in
    /// place this frame.
    pub fn detector_band_window(&self, channel: usize, band: usize) -> Vec<f32> {
        let base =
            band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS + FRONTEND_DETECTOR_WINDOW_OFFSET;
        self.sigproc.band_blocks[channel][base..base + GAIN_DETECT_BAND_WINDOW_VALUES].to_vec()
    }

    /// The arena the packer reads this frame (`*(obj+0x14)`, ring slot 0).
    pub fn packer_arena(&self, channel: usize) -> &FrontendArena {
        &self.arena_ring[channel][0]
    }

    /// The arena extract wrote this frame (`*(obj+0x24)`, ring slot 4).
    pub fn current_output_arena(&self, channel: usize) -> &FrontendArena {
        &self.arena_ring[channel][SIGPROC_CHANNEL_RING_SLOTS - 1]
    }
}

/// What one core call produced.
#[derive(Debug, Clone)]
pub struct FrontendCoreCallReport {
    pub sigproc: SigprocFrameReport,
    pub gha_ran: bool,
    /// The pre-residual slot-4..6 band windows handed to `extract_ghwave_at5`
    /// (`None` when the gate is closed). Captured before the residual pass
    /// mutates slot 4, so it equals the native extract entry band buffers.
    pub extract_input_band_windows: Option<Vec<Vec<Vec<f32>>>>,
    /// The live GHA output for this call (`None` when the gate is closed).
    pub extract_output: Option<GhaExtractOutput>,
    /// Per-channel `time2freq_at5` output (MDCT spectra + final point records +
    /// tonality) for this call (`None` when the GHA/detector gate is closed).
    /// The detector seeds have already been evolved into `state` for the next
    /// call before this is returned.
    pub time2freq: Option<Vec<Time2FreqChannelOutput>>,
}

/// Run one `at5enc_sigproc` core call from PCM.
///
/// `pcm_inputs[channel]` holds this call's 2048 fresh SCALAR samples. The ring
/// rotates once (mirroring the native prologue), `sigproc_frame_at5` rolls the
/// band slots and runs PQF, and — when the GHA gate is open — the live
/// `extract_ghwave_at5` boundary runs over the rolled slot-0..2 windows and the
/// previous frame's delayed arena (ring slot 3), writes its output into ring
/// slot 4, and its residual is applied to slot 0 of every band window.
///
/// `record_arena_header` anchors the extract row word 9 (a native heap
/// address); it only affects that pointer word, not the wave counts or records.
pub fn frontend_core_call_at5(
    state: &mut FrontendState,
    pcm_inputs: &[&[f32]],
    record_arena_header: i32,
) -> Result<FrontendCoreCallReport, FrontendError> {
    frontend_core_call_with_capture_at5(state, pcm_inputs, record_arena_header, true, None)
}

/// Encoder-facing frontend call. It advances exactly the same rolling state as
/// [`frontend_core_call_at5`] without retaining the large extract diagnostics.
pub(crate) fn frontend_encode_call_at5(
    state: &mut FrontendState,
    pcm_inputs: &[&[f32]],
    record_arena_header: i32,
) -> Result<FrontendCoreCallReport, FrontendError> {
    let mut scratch = FrontendScratch::default();
    frontend_core_call_with_capture_at5(
        state,
        pcm_inputs,
        record_arena_header,
        false,
        Some(&mut scratch),
    )
}

pub(crate) fn frontend_encode_call_with_scratch_at5(
    state: &mut FrontendState,
    pcm_inputs: &[&[f32]],
    record_arena_header: i32,
    scratch: &mut FrontendScratch,
) -> Result<FrontendCoreCallReport, FrontendError> {
    frontend_core_call_with_capture_at5(
        state,
        pcm_inputs,
        record_arena_header,
        false,
        Some(scratch),
    )
}

fn frontend_core_call_with_capture_at5(
    state: &mut FrontendState,
    pcm_inputs: &[&[f32]],
    record_arena_header: i32,
    capture_extract_diagnostics: bool,
    mut scratch: Option<&mut FrontendScratch>,
) -> Result<FrontendCoreCallReport, FrontendError> {
    // Fatal-per-run: once a prior call tore the rolling state, refuse to compute
    // on it. The run is dead; report the original poisoning point cheaply.
    if let Some(at) = state.poisoned {
        return Err(FrontendError::Poisoned { at });
    }

    // Input-shape validation runs before any state mutation, so these errors do
    // not poison the state (a caller can retry with well-formed input).
    if pcm_inputs.len() < state.channel_count {
        return Err(FrontendError::ChannelCount {
            channel_count: pcm_inputs.len(),
        });
    }
    for input in pcm_inputs.iter().take(state.channel_count) {
        if input.len() < FRONTEND_FRAME_SAMPLES {
            return Err(FrontendError::PcmShape {
                needed: FRONTEND_FRAME_SAMPLES,
                actual: input.len(),
            });
        }
    }

    // Everything past here mutates rolling state; a failure leaves it torn, so
    // poison the state before propagating.
    match frontend_core_call_body_at5(
        state,
        pcm_inputs,
        record_arena_header,
        capture_extract_diagnostics,
        scratch.as_deref_mut(),
    ) {
        Ok(report) => Ok(report),
        Err(error) => {
            if state.poisoned.is_none() {
                state.poisoned = Some(frontend_error_stage(&error));
            }
            Err(error)
        }
    }
}

/// Map a state-mutating frontend error to the pipeline stage that produced it,
/// for the poison marker.
fn frontend_error_stage(error: &FrontendError) -> &'static str {
    match error {
        FrontendError::Sigproc(_) => "sigproc",
        FrontendError::Extract(_) => "extract",
        FrontendError::Time2Freq(_) => "run_time2freq",
        FrontendError::PcmShape { .. } => "pcm_shape",
        FrontendError::ChannelCount { .. } => "channel_count",
        FrontendError::Poisoned { at } => at,
    }
}

fn frontend_core_call_body_at5(
    state: &mut FrontendState,
    pcm_inputs: &[&[f32]],
    record_arena_header: i32,
    capture_extract_diagnostics: bool,
    mut scratch: Option<&mut FrontendScratch>,
) -> Result<FrontendCoreCallReport, FrontendError> {
    // Prologue ring rotation, in lockstep with the native pointer-ring rotation
    // `sigproc_frame_at5` performs on `SigprocChannelPointers.ring`.
    for channel in 0..state.channel_count {
        state.arena_ring[channel].rotate_left(1);
    }

    let params = SigprocFrameParams {
        channel_count: state.channel_count,
        // Per-rate sigproc mode (`mode_a`): 2 at 320/352 (== FRONTEND_MODE,
        // unchanged), 3 at 48-256 (docs/13 §2.3).
        mode: state.sigproc_mode,
        // Per-rate band limit (`param_5` = band_index): 32 full-band, 29 at 192
        // (docs/13 §3.1). The writeback derives band_count = g_a_x_at5[bl]+1.
        band_limit: state.band_limit,
        // `cfg+0x1e8` (30 at 352, 29 at 320). Production is mode 2, so this is
        // inert for the band_count write; threaded to stay faithful.
        selector: state.selector,
        // The rate-INDEPENDENT shell extract gate (`at5enc_sigproc` decompile
        // 43499 `param_2[6] != 0 && param_2[5] == 0`): words [5]=0/[6]=1 pinned
        // at 64 too (docs/13 §5.1 evidence 1), identical to the 352 shell trace,
        // so `extract` is called at every rate. The per-rate GHA difference enters
        // only through the in-extract `cfg+0xd0` read (`gha_enabled` above), NOT
        // this gate.
        gha_gate_open: true,
    };
    let sigproc_report = sigproc_frame_at5(&mut state.sigproc, pcm_inputs, &params)?;

    let mut extract_output = None;
    let mut extract_input_band_windows = None;
    let mut gha_ran = false;
    if sigproc_report.gha_should_run {
        let input_windows = state.extract_band_windows();
        if capture_extract_diagnostics {
            extract_input_band_windows = Some(input_windows.clone());
        }
        let mut output = run_extract(state, input_windows, record_arena_header)?;
        gha_ran = true;
        // Store the fresh output into the current-output slot (`*(obj+0x24)`).
        let last = SIGPROC_CHANNEL_RING_SLOTS - 1;
        for channel in 0..state.channel_count {
            let arena = &mut state.arena_ring[channel][last];
            if capture_extract_diagnostics {
                arena.rows = output.output_rows[channel].clone();
                arena.records = output.row_records[channel].clone();
            } else {
                arena.rows = std::mem::take(&mut output.output_rows[channel]);
                arena.records = std::mem::take(&mut output.row_records[channel]);
            }
            arena.header_mode = output.header.mode as u32;
            arena.header_active = output.header_words[0];
            // Extract writes channel-0's header words `[active, mode,
            // band_count]` and the per-band share gates into the same output
            // arena root (`*(obj+0x24)`), so they ride the ring alongside the
            // rows/records (slice 2.1c, E1/E2).
            arena.header_band_count = output.header_words[2];
            let can_move_shared =
                !capture_extract_diagnostics && channel + 1 == state.channel_count;
            if can_move_shared {
                arena.shared = std::mem::take(&mut output.shared);
                arena.opposite = std::mem::take(&mut output.opposite);
            } else {
                arena.shared = output.shared.clone();
                arena.opposite = output.opposite.clone();
            }
        }
        // Apply the residual subtraction back into slot 4 (the first slot of
        // the extract window) of every band. It rolls down toward slot 0 over
        // the next four frames, so it reaches the slot-0..1 windows
        // `time2freq_at5` reads four calls later — matching the native band
        // scratch that extract subtracts in place.
        for channel in 0..state.channel_count {
            for band in 0..state.band_count {
                let start = band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS
                    + FRONTEND_EXTRACT_FIRST_SLOT * SIGPROC_BAND_SLOT_FLOATS;
                let residual = &output.band_windows[channel][band][..FRONTEND_RESIDUAL_SAMPLES];
                state.sigproc.band_blocks[channel][start..start + FRONTEND_RESIDUAL_SAMPLES]
                    .copy_from_slice(residual);
            }
        }
        // Native `extract_ghwave_at5` mask-1 setter (decompile 42139–42144;
        // disassembly-verified `or eax,1` store at native 0x4f067–0x4f070):
        // when the GHA header mode mask resolves to 1 (top-2-dominant sine
        // path), OR bit 0 into the config flag word `cfg+0x1dc`. Within this
        // core call bit 0 is invisible to every consumer mask
        // (`&2`/`&0x7c`/`&0x10`); the effect propagates via the NEXT call's
        // shell prologue shift (`sigproc_shift_flag_word_at5`, native
        // 0x4f485), so applying it after extract returns is order-safe.
        if output.header.sets_global_mode_flag {
            state.sigproc.header_flag_word |= 1;
        }
        if capture_extract_diagnostics {
            extract_output = Some(output);
        }
    }

    // Detector + MDCT: run `time2freq_at5` live over the active QMF bands x 2
    // channels and evolve the cross-frame detector seeds into `state` for the
    // next call. The band extent is the shell's band-limit-epilogue band_count
    // (`g_a_x_at5[band_limit] + 1`): 16 full-band, 13 at 192 (docs/13 §3.1) —
    // the same value native fans into every channel's `+0x1b48c` and passes as
    // the `time2freq_at5` band-count argument (decompile 43506/43562). Bands
    // >= band_count keep their previous seed/record state (native does not
    // touch them).
    let time2freq_band_count = sigproc_report.writeback.band_count as usize;
    let mut time2freq = None;
    if sigproc_report.gha_should_run {
        time2freq = Some(run_time2freq(
            state,
            time2freq_band_count,
            capture_extract_diagnostics,
            scratch.as_deref_mut(),
        )?);
    }

    Ok(FrontendCoreCallReport {
        sigproc: sigproc_report,
        gha_ran,
        extract_input_band_windows,
        extract_output,
        time2freq,
    })
}

/// Run the live detector + MDCT (`time2freq_at5`) for one core call and evolve
/// the per-band detector seeds. Reads the post-residual band scratch (slot-0..1
/// MDCT windows and slot-3..4 detector windows) out of `state`, threads the
/// owned per-band seeds through `time2freq_at5`, then rewrites `state`'s seeds
/// with the native writeback evolution and updates `previous_records`.
fn run_time2freq(
    state: &mut FrontendState,
    active_band_count: usize,
    capture_diagnostics: bool,
    mut scratch: Option<&mut FrontendScratch>,
) -> Result<Vec<Time2FreqChannelOutput>, FrontendError> {
    let channel_count = state.channel_count;
    // `band_count` bounds the time2freq PROCESSING extent + the seed/record
    // evolution (13 at 192, 16 full-band). The persistent buffers stay
    // `state.band_count`-wide (16); bands >= `band_count` keep prior state.
    let band_count = active_band_count.min(state.band_count);
    let mode_cc = state.mode_cc;

    let params = Time2FreqParams {
        channel_count,
        // `param_6` = `mode_a` (docs/13 §5.2 (jjj)): 2 at 320/352, 3 at 48-256.
        // Refutes the old 352 constant `FRONTEND_TIME2FREQ_PARAM6 = 2` below 320.
        param6: state.sigproc_mode as i32,
        // The block selector (`cfg+0x1e8`) — the live per-rate value (30/352,
        // 29/320). Every selector-dependent value read is identical at 29 vs 30
        // (docs/13 §1.1 evidence 5), so this is parameter threading only.
        bandwidth: state.selector,
        band_limit: band_count,
        // `cfg+0xcc` — 1 at 96-352 (detector), 0 at 48/64 (set_gainc dispatch).
        mode_cc: i32::from(mode_cc),
        detector_gate_open: true,
    };

    let mut channel_states: Vec<Time2FreqChannelState> = Vec::with_capacity(channel_count);
    let mut outputs;

    if mode_cc {
        // Detector path: refresh each seed's fresh detector window + current
        // front bin-0 peak, then move the seeds into the channel states.
        for channel in 0..channel_count {
            let mut seeds = std::mem::take(&mut state.detector_seeds[channel]);
            for (band, seed) in seeds.iter_mut().enumerate() {
                let window = state.detector_band_window(channel, band);
                seed.current_bin0_peak = detector_window_bin0_peak(&window);
                seed.band_window = window;
            }
            channel_states.push(Time2FreqChannelState {
                band_inputs: state.time2freq_band_inputs(channel),
                previous_records: state.previous_records[channel].clone(),
                detector_seeds: seeds,
                // Native `time2freq_at5` tonality-prepass skip (decompile 32786):
                // the prepass runs only when `(*(cfg+0x1dc) & 0x10) == 0`.
                prepass_disabled: state.sigproc.header_flag_word & 0x10 != 0,
            });
        }
        outputs = if capture_diagnostics {
            time2freq_at5(&mut channel_states, &params)?
        } else {
            time2freq_encode_at5(
                &mut channel_states,
                &params,
                &mut scratch
                    .as_deref_mut()
                    .expect("encoder call supplies frontend scratch")
                    .gain_detect,
            )?
        };
    } else {
        // mode_cc == 0 (64/48): the descending `set_gainc_at5` dispatch over the
        // shared detector arena + per-channel persistent history / prev+cur
        // planes (docs/13 §5.2). Scratch is the slot-0-based band-block floats
        // 255..388; histories/prev-plane are threaded from `FrontendState` and
        // persisted back after the call.
        let mut sg_channels = Vec::with_capacity(channel_count);
        for channel in 0..channel_count {
            let scratch: Vec<[f32; SET_GAINC_SCRATCH_FLOATS]> = (0..SET_GAINC_BANDS)
                .map(|band| state.set_gainc_scratch(channel, band))
                .collect();
            sg_channels.push(Time2FreqSetGaincChannel {
                scratch,
                history_a: std::mem::take(&mut state.set_gainc_history_a[channel]),
                history_b: std::mem::take(&mut state.set_gainc_history_b[channel]),
                prev_plane: state.set_gainc_prev_plane[channel],
                cur_plane: [[0u32; crate::dsp::set_gainc::SET_GAINC_ROW_WORDS]; SET_GAINC_BANDS],
            });
            channel_states.push(Time2FreqChannelState {
                band_inputs: state.time2freq_band_inputs(channel),
                previous_records: Vec::new(),
                detector_seeds: Vec::new(),
                prepass_disabled: state.sigproc.header_flag_word & 0x10 != 0,
            });
        }
        let mut set_gainc_state = Time2FreqSetGaincState {
            detector_words: &mut state.sigproc.detector_words,
            // Header `+0x1c` = 0 at 64/48 (`set_gainc_io_trace` header_0x1c_u32);
            // this is the stage-5 side field, `!= 2` keeps the harmonization open.
            header_1c: 0,
            // cfg+0x50 reconcile direction words: probe 2026-07-10 all-zero at 64
            // kbps calls 0/7/50 + no static writer (calloc-zero). Direction 0 =
            // copy ch0 → ch1.
            direction_words: [0u32; SET_GAINC_BANDS],
            channels: sg_channels,
        };
        outputs = time2freq_at5_with_set_gainc(&mut channel_states, &params, &mut set_gainc_state)?;
        // Persist: histories back, cur_plane → prev_plane. Consume the state to
        // release the `detector_words` borrow (already mutated in place).
        let Time2FreqSetGaincState {
            channels: mut sg_out,
            ..
        } = set_gainc_state;
        for channel in 0..channel_count {
            state.set_gainc_history_a[channel] = std::mem::take(&mut sg_out[channel].history_a);
            state.set_gainc_history_b[channel] = std::mem::take(&mut sg_out[channel].history_b);
            state.set_gainc_prev_plane[channel] = sg_out[channel].cur_plane;
        }
    }

    // `time2freq_at5` sizes its `spectra`/`delayed_out` to `band_count * 128`
    // (1664 at 192). Native's MDCT output buffer (`block+0x1010`) is the
    // FULL-WIDTH 2048-float block regardless — only the active band_count*128
    // lines are written; the [band_count*128 .. 2048) tail stays zero and feeds
    // the (inactive) scale-factor units [band_count..16). Pad to the full-band
    // width so the coding bridge's fixed 2048-line init/normalize surface (and
    // the calc's 32-unit buffers) see the same geometry at every rate (docs/13
    // §3.1). No-op at full-band (band_count == state.band_count).
    let full_spectrum_len = state.band_count * crate::dsp::mdct::MDCT_128_OUTPUT_COUNT;
    for out in outputs.iter_mut() {
        out.spectra.resize(full_spectrum_len, 0.0);
        out.delayed_out.resize(full_spectrum_len, 0.0);
    }

    // Evolve each seed from its post-detect state + outcome, and update the
    // previous-record swap for next call — but only over the active
    // `band_count` bands. Bands >= band_count keep their pre-call seed and
    // prior previous_record (native leaves them untouched; at 192 those are
    // the QMF bands 13..16 above the reduced 1664-line extent, never read
    // again since the next call also stops at band_count). At full-band
    // (band_count == state.band_count) this evolves every band, identical to
    // before.
    // Detector-path only: on the mode_cc == 0 path native never touches the
    // detector seeds / detect-path previous_records at 64/48, so skip both (the
    // set_gainc persistence above carries the cross-frame state instead).
    if mode_cc {
        let persistent_bands = state.band_count;
        for channel in 0..channel_count {
            if !capture_diagnostics {
                state.detector_seeds[channel] =
                    std::mem::take(&mut channel_states[channel].detector_seeds);
                for band in 0..band_count {
                    state.previous_records[channel][band] = outputs[channel].final_records[band];
                }
                continue;
            }
            let mut next_seeds = Vec::with_capacity(persistent_bands);
            for band in 0..persistent_bands {
                if band < band_count {
                    let next_window = state.detector_band_window(channel, band);
                    let post_seed = &channel_states[channel].detector_seeds[band];
                    let outcome = &outputs[channel].detector_outcomes[band];
                    next_seeds.push(time2freq_detector_seed_evolve_at5(
                        post_seed,
                        outcome,
                        next_window,
                    )?);
                } else {
                    next_seeds.push(channel_states[channel].detector_seeds[band].clone());
                }
            }
            state.detector_seeds[channel] = next_seeds;
            for band in 0..band_count {
                state.previous_records[channel][band] = outputs[channel].final_records[band];
            }
        }
    }

    Ok(outputs)
}

/// Fresh front bin-0 peak (native `local_9c[0]`, the level-b numerator): bin 0
/// of the detector-window peak scan (floats `512..640`, 32 abs-max quads).
/// Uses the same ported scan the detector runs internally so the value is
/// bit-identical to the front the `gain_detect_band_at5` recomputes.
fn detector_window_bin0_peak(window: &[f32]) -> f32 {
    let peak_offset = crate::dsp::sigproc::GAIN_DETECT_BAND_WINDOW_PEAK_OFFSET;
    crate::dsp::sigproc::gain_detect_peak_bins_at5(&window[peak_offset..])
        .map(|peaks| peaks.bins()[0])
        .unwrap_or(0.0)
}

fn run_extract(
    state: &FrontendState,
    band_windows: Vec<Vec<Vec<f32>>>,
    record_arena_header: i32,
) -> Result<GhaExtractOutput, FrontendError> {
    // Delayed rows/records come from ring slot 3 (`*(obj+0x20)`), the previous
    // frame's extract output.
    let delayed_slot = SIGPROC_CHANNEL_RING_SLOTS - 2;
    let delayed_rows: Vec<Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>> = (0..state.channel_count)
        .map(|channel| state.arena_ring[channel][delayed_slot].rows.clone())
        .collect();
    let delayed_records: Vec<Vec<Vec<GhaWaveRecord>>> = (0..state.channel_count)
        .map(|channel| state.arena_ring[channel][delayed_slot].records.clone())
        .collect();
    let delayed_header_mode = state.arena_ring[0][delayed_slot].header_mode;
    // The delayed residual row uses the inverse-mix flags from the same
    // delayed arena, not the fresh frame's flags. Native keeps this header
    // shared across the two channel row arenas on the GHA-disabled path.
    let delayed_opposite = state.arena_ring[0][delayed_slot].opposite.clone();

    let input = GhaExtractInput {
        channel_count: state.channel_count,
        band_count: state.band_count,
        // The block selector (`cfg+0x1e8`) — the live per-rate value (30/352,
        // 29/320); identical extract behavior at 29 vs 30 (docs/13 §1.1).
        param_3: state.selector,
        profile_selector: 0,
        header_flag_word: state.sigproc.header_flag_word,
        // Per-rate GHA enable (`cfg+0xd0`): false at 48/64 selects the disabled
        // fallback, true at 96-352 the analysis arms (docs/13 §5.1). The sine
        // mask-1/2 arm dispatches on the front mode decision, so it stays live
        // even when this is false (evidence item 3).
        header_0xd0_enabled: state.gha_enabled,
        band_windows,
        delayed_rows,
        delayed_records,
        delayed_header_mode,
        delayed_opposite: Some(delayed_opposite),
        record_arena_header,
    };
    Ok(extract_ghwave_at5(input)?)
}

#[cfg(test)]
mod lean_path_tests {
    use super::*;
    use crate::encoder::coding_params::CodingParams;
    use crate::encoder::profile::{ATRAC3PLUS_128, ATRAC3PLUS_MONO_64};

    fn assert_time2freq_parity(full: &[Time2FreqChannelOutput], lean: &[Time2FreqChannelOutput]) {
        for (full_channel, lean_channel) in full.iter().zip(lean) {
            assert_eq!(
                full_channel
                    .spectra
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                lean_channel
                    .spectra
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                full_channel
                    .delayed_out
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                lean_channel
                    .delayed_out
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
            assert_eq!(full_channel.final_records, lean_channel.final_records);
            assert_eq!(full_channel.tonality, lean_channel.tonality);
            assert_eq!(full_channel.band_outcomes, lean_channel.band_outcomes);
            assert!(!full_channel.detector_outcomes.is_empty());
            assert!(lean_channel.detector_outcomes.is_empty());
        }
    }

    #[test]
    fn lean_encoder_call_matches_full_state_without_retaining_diagnostics() {
        let left = [0.0f32; FRONTEND_FRAME_SAMPLES];
        let right = [0.0f32; FRONTEND_FRAME_SAMPLES];
        let inputs = [&left[..], &right[..]];
        let mut full_state = FrontendState::new_zeroed(FRONTEND_CHANNEL_COUNT);
        let mut lean_state = full_state.clone();

        let full = frontend_core_call_at5(&mut full_state, &inputs, 0).unwrap();
        let lean = frontend_encode_call_at5(&mut lean_state, &inputs, 0).unwrap();

        assert!(full.extract_input_band_windows.is_some());
        assert!(full.extract_output.is_some());
        assert!(lean.extract_input_band_windows.is_none());
        assert!(lean.extract_output.is_none());
        assert!(lean.gha_ran);
        assert_time2freq_parity(
            full.time2freq.as_ref().unwrap(),
            lean.time2freq.as_ref().unwrap(),
        );
        assert_eq!(format!("{full_state:?}"), format!("{lean_state:?}"));

        let mut left = [0.0f32; FRONTEND_FRAME_SAMPLES];
        let mut right = [0.0f32; FRONTEND_FRAME_SAMPLES];
        for call in 1..100u32 {
            for (index, sample) in left.iter_mut().enumerate() {
                *sample = ((index as i32 * 31 + call as i32 * 17) % 257 - 128) as f32 / 128.0;
            }
            for (index, sample) in right.iter_mut().enumerate() {
                *sample = ((index as i32 * 19 + call as i32 * 29) % 251 - 125) as f32 / 128.0;
            }
            let inputs = [&left[..], &right[..]];
            let full = frontend_core_call_at5(&mut full_state, &inputs, 0).unwrap();
            let lean = frontend_encode_call_at5(&mut lean_state, &inputs, 0).unwrap();
            assert_time2freq_parity(
                full.time2freq.as_ref().unwrap(),
                lean.time2freq.as_ref().unwrap(),
            );
            assert_eq!(format!("{full_state:?}"), format!("{lean_state:?}"));
        }
    }

    #[test]
    fn lean_encoder_call_matches_full_state_for_parity_profiles() {
        for (profile, sample_frames) in [(ATRAC3PLUS_MONO_64, 6144usize), (ATRAC3PLUS_128, 6145)] {
            let params = CodingParams::for_profile(&profile);
            let channel_count = profile.channels() as usize;
            let mut full_state =
                FrontendState::new_zeroed_for_selector(channel_count, params.selector);
            full_state.sigproc_mode = params.mode_a;
            full_state.band_limit = params.band_index as i32;
            full_state.gha_enabled = params.gha_enabled;
            full_state.mode_cc = params.mode_cc;
            let mut lean_state = full_state.clone();
            let mut scratch = FrontendScratch::default();

            for call in 0..16usize {
                let mut channels = vec![vec![0.0f32; FRONTEND_FRAME_SAMPLES]; channel_count];
                let source_start = call * FRONTEND_FRAME_SAMPLES;
                for (channel, samples) in channels.iter_mut().enumerate() {
                    for (index, sample) in samples.iter_mut().enumerate() {
                        let frame = source_start + index;
                        if frame < sample_frames {
                            *sample = ((frame as i32 * 43 + channel as i32 * 997) % 50_003 - 25_001)
                                as f32;
                        }
                    }
                }
                let inputs: Vec<&[f32]> = channels.iter().map(Vec::as_slice).collect();
                let full = frontend_core_call_at5(&mut full_state, &inputs, 0).unwrap();
                let lean = frontend_encode_call_with_scratch_at5(
                    &mut lean_state,
                    &inputs,
                    0,
                    &mut scratch,
                )
                .unwrap();
                assert_time2freq_parity(
                    full.time2freq.as_ref().unwrap(),
                    lean.time2freq.as_ref().unwrap(),
                );
                assert_eq!(
                    format!("{full_state:?}"),
                    format!("{lean_state:?}"),
                    "{} kbps call {call}",
                    profile.bitrate_kbps(),
                );
            }
        }
    }
}
