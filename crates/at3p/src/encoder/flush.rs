//! Frame-count, encode, and flush scheduling for the ATRAC3plus encoder.
//!
//! Evidence:
//! - `atrac_encode` wrapper native `0x000096d0` / decompile comment `0x196d0`
//!   passes `input_bytes / (2 * channels)` as the core sample count.
//! - `atrac_flush_encode` wrapper native `0x000092a0` / decompile comment
//!   `0x192a0` zeroes produced bytes, delegates to `atx_flush_encode`, and
//!   returns done=1/produced=0 when flushing is complete.
//! - `atx_flush_encode` native `0x0005a240` / decompile comment `0x6a240`
//!   decrements `state+0x1c` before calling `atx_encode(... sample_count=0 ...)`;
//!   otherwise it sets `*produced=0`, `*done=1`.

/// PCM sample frames a full (non-final) encode core call consumes (native
/// `atx_encode_core` reads `input_bytes / (2 * channels)` = 2048 for the 352
/// stereo path).
pub const CORE_CALL_SAMPLE_FRAMES: u32 = 2048;

/// The first output-bearing global core call (native delay/priming: calls 0..6
/// produce no output frame). Universal across all input lengths — for
/// sub-priming inputs the early flush calls simply produce 0 bytes
/// (`len_edges_run.json` N=6144: 4 zero-producing flush calls then 5
/// output-bearing ones).
pub const FIRST_OUTPUT_CORE_CALL: u32 = 7;

/// The native minimum accepted input length. `at3tool` `checkEncodeParam`
/// (decompiled/at3tool.c line 1803; reject at lines 1813-1817) rejects
/// `samples < 0x1800` (6144) with error `0x81000003` ("too short input file")
/// BEFORE any library call — observed on `len_edges_run.json` N=2048/2049/5000
/// (zero ATRAC API calls). There is no native schedule below this, so the
/// computed schedule fails explicitly for `N < MIN_INPUT_SAMPLE_FRAMES`.
pub const MIN_INPUT_SAMPLE_FRAMES: u32 = 0x1800;

/// A fail-explicit typed error for an input shorter than the native minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputTooShort {
    pub input_sample_frames: u32,
    pub minimum: u32,
}

/// The encode/flush/output schedule for any supported profile and input of
/// `input_sample_frames >= MIN_INPUT_SAMPLE_FRAMES` PCM sample frames per
///
/// `len_edges_batch_v1`; sibling `syn_len_*_api_trace.ndjson` per-call traces):
///
/// - **Encode wrapper calls** = `ceil(N / 2048)`. Every call passes 2048 samples
///   except the last, which passes `n_last = N - 2048*(encode_calls-1) ∈ [1,
///   2048]`. There is NO extra partial call when `N % 2048 == 0` (observed:
///   N=6144/20480 → `final_call_input_bytes == 8192`, i.e. 2048 samples).
/// - **Flush processing calls** = `((n_last + 0x396f) >> 11) + 1` (8 when
///   `1 <= n_last <= 1680`, 9 when `1681 <= n_last <= 2048`). `atx_encode_core`
///   (native `0x559f0`; Ghidra decompile comment `0x659f0` = native + 0x10000;
///   decompiled/libatrac.c lines 46032-46038) stores `state+0x1c = ((n + 0x396f)
///   >> 11) + 1` on EVERY encode call with `n > 0`, so the LAST call's sample
///   count wins. Pinned by the N=7824 (n_last 1680 → 8) vs N=7825 (n_last 1681 →
///   9) one-sample boundary.
/// - **Flush wrapper calls** = `flush_processing_calls + 1`: `atx_flush_encode`
///   (native `0x5a240`; decompile 47805-47833) decrements `state+0x1c` per
///   processing call feeding an all-zero frame, and the next wrapper call returns
///   `produced=0, done=1`.
/// - **Output frames** = `encode_calls + flush_processing_calls - 7` =
///   `floor((N + 367) / 2048) + 2` (algebraically identical). Every output-bearing
///   call produces exactly 2048 bytes; the first output is at global core call 7.
///   dance 7787435→3804.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeSchedule {
    input_sample_frames: u32,
    encode_calls: u32,
    final_call_sample_frames: u32,
    flush_processing_calls: u32,
}

impl EncodeSchedule {
    /// Derive the schedule from the input sample-frame count. Returns
    /// [`InputTooShort`] for `N < MIN_INPUT_SAMPLE_FRAMES` (there is no native
    /// schedule below the 6144 minimum — fail explicit, never guess).
    pub fn new(input_sample_frames: u32) -> Result<Self, InputTooShort> {
        if input_sample_frames < MIN_INPUT_SAMPLE_FRAMES {
            return Err(InputTooShort {
                input_sample_frames,
                minimum: MIN_INPUT_SAMPLE_FRAMES,
            });
        }

        // encode_calls = ceil(N / 2048); the last call reads the remainder
        // (2048 for an exact multiple — no extra partial call).
        let encode_calls = input_sample_frames.div_ceil(CORE_CALL_SAMPLE_FRAMES);
        let final_call_sample_frames =
            input_sample_frames - CORE_CALL_SAMPLE_FRAMES * (encode_calls - 1);

        // flush_processing_calls = ((n_last + 0x396f) >> 11) + 1, set by
        // `atx_encode_core` from the LAST encode call's sample count.
        let flush_processing_calls = ((final_call_sample_frames + 0x396f) >> 11) + 1;

        Ok(Self {
            input_sample_frames,
            encode_calls,
            final_call_sample_frames,
            flush_processing_calls,
        })
    }

    /// Total input sample frames per channel (`N`).
    pub fn input_sample_frames(&self) -> u32 {
        self.input_sample_frames
    }

    /// Encode wrapper calls = `ceil(N / 2048)`.
    pub fn encode_calls(&self) -> u32 {
        self.encode_calls
    }

    /// Sample frames the final encode call consumes = `n_last ∈ [1, 2048]`.
    pub fn final_call_sample_frames(&self) -> u32 {
        self.final_call_sample_frames
    }

    /// PCM-processing flush calls that feed an all-zero frame = `((n_last +
    /// 0x396f) >> 11) + 1` (8 or 9).
    pub fn flush_processing_calls(&self) -> u32 {
        self.flush_processing_calls
    }

    /// Flush WRAPPER calls = `flush_processing_calls + 1` (the last returns
    /// `produced=0, done=1`).
    pub fn flush_wrapper_calls(&self) -> u32 {
        self.flush_processing_calls + 1
    }

    /// Total global core calls the driver runs = `encode_calls +
    /// flush_processing_calls` (encode wrappers + PCM-processing flush wrappers;
    /// the trailing done wrapper processes no PCM).
    pub fn total_core_calls(&self) -> u32 {
        self.encode_calls + self.flush_processing_calls
    }

    /// Total output frames = `encode_calls + flush_processing_calls - 7`.
    pub fn total_output_frames(&self) -> u32 {
        self.total_core_calls() - FIRST_OUTPUT_CORE_CALL
    }

    /// The number of output-bearing encode calls (encode calls at or past the
    /// first-output core call).
    pub fn encode_output_frames(&self) -> u32 {
        self.encode_calls.saturating_sub(FIRST_OUTPUT_CORE_CALL)
    }

    /// Sample frames the encode core call at `core_call_index` consumes: 2048 for
    /// every call except the last (`n_last`).
    pub fn expected_encode_sample_frames(&self, core_call_index: u32) -> u32 {
        if core_call_index + 1 < self.encode_calls {
            CORE_CALL_SAMPLE_FRAMES
        } else {
            self.final_call_sample_frames
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSource {
    Encode,
    Flush,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlushScheduleError {
    WrongSampleFrameCount {
        core_call_index: u32,
        expected: u32,
        actual: u32,
    },
    FlushBeforeInputExhausted {
        encode_calls: u32,
        expected: u32,
    },
    EncodeAfterFlushStarted,
    TooManyEncodeCalls,
    FlushAlreadyDone,
}

// Computes each output frame from PCM via [`FrameDriver`]. It reproduces
// the native schedule contract for ANY input of
// `N >= MIN_INPUT_SAMPLE_FRAMES` sample frames per channel, driven by a
// [`EncodeSchedule`] (see its doc comment for the native sources): encode
// calls + PCM-processing flush calls (8 or 9) → output frames, first output at
// (N=154064) this is exactly the archived 76 encode + 9 flush → 77 output
// contract.
//
// The driver's per-call PCM is supplied at construction (the schedule-derived
// chunking: `encode_calls` encode frames including the
// zero-padded tail + `flush_processing_calls` zero flush frames), so
// `encode_chunk` / `flush` keep the schedule's sample-frame-count contract while
// feeding the driver the real f32 samples for the current core call.
// ===========================================================================

use crate::encoder::coding_params::CodingParams;
use crate::encoder::frame::{DEFAULT_FRAME_BYTES, FrameDriver, FrameError};
use crate::encoder::frontend::FRONTEND_FRAME_SAMPLES;

/// Errors from the computed flush scheduler.
#[derive(Debug, Clone, PartialEq)]
pub enum FlushError {
    /// The input is shorter than the native minimum (`N < MIN_INPUT_SAMPLE_FRAMES`;
    /// native `at3tool` rejects it before any library call — fail explicit).
    InputTooShort(InputTooShort),
    /// A schedule contract violation (same set as [`FlushScheduleError`]).
    Schedule(FlushScheduleError),
    /// The per-call PCM frame supply had the wrong length (must be exactly the
    /// derived schedule chunking:
    /// `encode_calls + flush_processing_calls`).
    FrameSupplyLen { expected: usize, actual: usize },
    /// A supplied per-call PCM frame had the wrong channel count (must equal the
    /// driver's `params.channels`; docs/14 §0.4 channel-vec supply).
    FrameChannelCount {
        core_call_index: u32,
        expected: usize,
        actual: usize,
    },
    /// A supplied per-call PCM frame had the wrong sample count.
    FrameSampleLen {
        core_call_index: u32,
        expected: usize,
        actual: usize,
    },
    /// The computed single-frame assembly failed at this core call.
    Compute {
        core_call_index: u32,
        error: FrameError,
    },
}

impl From<FlushScheduleError> for FlushError {
    fn from(error: FlushScheduleError) -> Self {
        FlushError::Schedule(error)
    }
}

impl From<InputTooShort> for FlushError {
    fn from(error: InputTooShort) -> Self {
        FlushError::InputTooShort(error)
    }
}

/// A computed output-frame result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameResult {
    pub source: FrameSource,
    pub core_call_index: Option<u32>,
    pub output_frame_index: Option<u32>,
    pub produced_bytes: usize,
    /// The computed 2048-byte frame for an output-bearing call; `None` for a
    /// priming/done call.
    pub frame_bytes: Option<Vec<u8>>,
    pub done: bool,
    pub flush_remaining: u32,
}

/// Computed-frame flush scheduler. Owns the [`FrameDriver`], the derived
/// [`EncodeSchedule`], and the per-call PCM supply; computes each output
/// frame at its scheduled call.
pub struct BufferedFlushScheduler {
    schedule: EncodeSchedule,
    /// Per-core-call PCM frames indexed `[core_call][channel][sample]`
    /// (`encode_calls + flush_processing_calls` entries: encode + PCM-processing
    /// flush; the trailing flush wrapper call is the "done" call and processes no
    /// PCM). Channel-vec (docs/14 §0.4): the pair-shaped `new`/`new_for_params`
    /// constructors convert to this internally so stereo callers are unchanged.
    frames: Vec<Vec<Vec<f32>>>,
    driver: FrameDriver,
    encode_calls: u32,
    flush_calls: u32,
    flush_remaining: u32,
    output_frames_emitted: u32,
    input_exhausted: bool,
    flush_started: bool,
    flush_done: bool,
}

impl BufferedFlushScheduler {
    /// Total core-call PCM frames the driver expects for a schedule:
    /// `encode_calls + flush_processing_calls` (the trailing flush wrapper call
    /// is the "done" call and processes no PCM).
    pub fn expected_frame_supply(schedule: &EncodeSchedule) -> usize {
        schedule.total_core_calls() as usize
    }

    /// Build the scheduler from the input sample-frame count `N` and the full
    /// length-agnostic schedule chunking
    /// (`src/encoder/frontend.rs`). Rejects `N < MIN_INPUT_SAMPLE_FRAMES` with the
    /// typed too-short error. The supply must be exactly
    /// [`expected_frame_supply`](Self::expected_frame_supply) frames for the
    /// derived schedule, each a `[left, right]` pair of [`FRONTEND_FRAME_SAMPLES`]
    /// f32 samples.
    pub fn new(input_sample_frames: u32, frames: Vec<[Vec<f32>; 2]>) -> Result<Self, FlushError> {
        // 352 params (selector 30, budget 16379, 2048 frame bytes, mode_a 2).
        Self::new_for_params(
            input_sample_frames,
            frames,
            CodingParams {
                selector: 30,
                budget: 16379,
                frame_bytes: DEFAULT_FRAME_BYTES as u32,
                // Stereo anchor (`handle+0x94` == 2).
                channels: 2,
                mode_a: 2,
                band_index: crate::encoder::coding_params::FULL_BAND_INDEX,
                // selector 30 > 0x12 → GHA enabled (docs/13 §5.1).
                gha_enabled: true,
                // selector 30 > 0x12 → mode_cc set (detector chain, docs/13 §5.2).
                mode_cc: true,
            },
        )
    }

    /// Like [`new`](Self::new) but for an explicit per-rate [`CodingParams`]
    /// (docs/13 §1.1): the owned [`FrameDriver`] is seeded with `params`
    /// so every computed frame is per-rate (selector/budget/frame_bytes). The
    /// encode/flush/output SCHEDULE is rate-independent (docs/13 §2.3), so the
    /// same [`EncodeSchedule`] governs. At the 352 params this equals
    /// [`new`](Self::new).
    pub fn new_for_params(
        input_sample_frames: u32,
        frames: Vec<[Vec<f32>; 2]>,
        params: CodingParams,
    ) -> Result<Self, FlushError> {
        // Stereo pair → channel-vec converter (docs/14 §0.4): every existing
        // caller/test keeps the `[left, right]` pair supply and this reshapes it
        // to the channel-vec representation without touching the samples.
        let channel_frames: Vec<Vec<Vec<f32>>> = frames
            .into_iter()
            .map(|[left, right]| vec![left, right])
            .collect();
        Self::new_for_params_channels(input_sample_frames, channel_frames, params)
    }

    /// Like [`new_for_params`](Self::new_for_params) but for a channel-vec PCM
    /// supply (`frames[core_call][channel][sample]`, docs/14 §0.4): each core
    /// call must carry exactly `params.channels` channels of
    /// [`FRONTEND_FRAME_SAMPLES`] f32 samples. The pair-shaped constructors
    /// delegate here after reshaping. All five mono shipping paths — 128 kbps
    /// (docs/14 §1.3), 96 kbps (docs/14 §2.1), 64 kbps (docs/14 §3.1), 48 kbps
    /// (docs/14 §4.1), and 32 kbps (docs/14 §5.1) — drive a 1-channel scheduler
    /// here.
    pub fn new_for_params_channels(
        input_sample_frames: u32,
        frames: Vec<Vec<Vec<f32>>>,
        params: CodingParams,
    ) -> Result<Self, FlushError> {
        let schedule = EncodeSchedule::new(input_sample_frames)?;
        let expected = Self::expected_frame_supply(&schedule);
        if frames.len() != expected {
            return Err(FlushError::FrameSupplyLen {
                expected,
                actual: frames.len(),
            });
        }
        let channel_count = params.channels as usize;
        for (call, frame) in frames.iter().enumerate() {
            if frame.len() != channel_count {
                return Err(FlushError::FrameChannelCount {
                    core_call_index: call as u32,
                    expected: channel_count,
                    actual: frame.len(),
                });
            }
            for channel in frame {
                if channel.len() != FRONTEND_FRAME_SAMPLES {
                    return Err(FlushError::FrameSampleLen {
                        core_call_index: call as u32,
                        expected: FRONTEND_FRAME_SAMPLES,
                        actual: channel.len(),
                    });
                }
            }
        }
        Ok(Self {
            schedule,
            frames,
            driver: FrameDriver::for_params(params),
            encode_calls: 0,
            flush_calls: 0,
            // Before input is exhausted, the eventual flush drain length is the
            // full wrapper count (processing + 1). Reset to the processing count
            // once the final encode call runs.
            flush_remaining: schedule.flush_wrapper_calls(),
            output_frames_emitted: 0,
            input_exhausted: false,
            flush_started: false,
            flush_done: false,
        })
    }

    /// The derived schedule for this scheduler's input length.
    pub fn schedule(&self) -> &EncodeSchedule {
        &self.schedule
    }

    /// One encode wrapper call for a chunk of `sample_frames` samples. The
    /// sample-count contract is derived from the schedule; the frame bytes are
    /// computed from the stored PCM for this core call.
    pub fn encode_chunk(&mut self, sample_frames: u32) -> Result<FrameResult, FlushError> {
        if self.flush_started {
            return Err(FlushScheduleError::EncodeAfterFlushStarted.into());
        }
        if self.encode_calls >= self.schedule.encode_calls() {
            return Err(FlushScheduleError::TooManyEncodeCalls.into());
        }
        let core_call_index = self.encode_calls;
        let expected = self.schedule.expected_encode_sample_frames(core_call_index);
        if sample_frames != expected {
            return Err(FlushScheduleError::WrongSampleFrameCount {
                core_call_index,
                expected,
                actual: sample_frames,
            }
            .into());
        }

        // Drive the frontend + compute this core call's frame (if output-bearing).
        let frame = self.drive(core_call_index)?;

        self.encode_calls += 1;
        if self.encode_calls == self.schedule.encode_calls() {
            self.input_exhausted = true;
            self.flush_remaining = self.schedule.flush_processing_calls();
        }

        let output_frame_index = if core_call_index >= FIRST_OUTPUT_CORE_CALL {
            Some(core_call_index - FIRST_OUTPUT_CORE_CALL)
        } else {
            None
        };
        if output_frame_index.is_some() {
            self.output_frames_emitted += 1;
        }
        Ok(self.result(
            FrameSource::Encode,
            Some(core_call_index),
            output_frame_index,
            frame,
            false,
        ))
    }

    /// One flush wrapper call. The frame bytes are computed from the stored
    /// zero-PCM flush frame for this core call.
    pub fn flush(&mut self) -> Result<FrameResult, FlushError> {
        if !self.input_exhausted {
            return Err(FlushScheduleError::FlushBeforeInputExhausted {
                encode_calls: self.encode_calls,
                expected: self.schedule.encode_calls(),
            }
            .into());
        }
        if self.flush_done {
            return Err(FlushScheduleError::FlushAlreadyDone.into());
        }

        self.flush_started = true;
        let flush_call_index = self.flush_calls;
        self.flush_calls += 1;

        if flush_call_index < self.schedule.flush_processing_calls() {
            self.flush_remaining -= 1;
            // The PCM-processing flush core calls follow the encode calls; the
            // Nth processing flush is global core call `encode_calls + N`.
            let core_call_index = self.schedule.encode_calls() + flush_call_index;
            // For short inputs the first PCM-processing flush calls can land
            // BEFORE the first-output core call 7 (native delay/priming) — they
            // still drive the frontend but produce 0 bytes. `drive` returns
            // `None` for such a priming call (`len_edges_run.json` N=6144: 4
            // zero-producing flush calls then 5 output-bearing ones).
            let output_frame_index = core_call_index.checked_sub(FIRST_OUTPUT_CORE_CALL);
            let frame = self.drive(core_call_index)?;
            if output_frame_index.is_some() {
                self.output_frames_emitted += 1;
            }
            Ok(self.result(
                FrameSource::Flush,
                Some(core_call_index),
                output_frame_index,
                frame,
                false,
            ))
        } else {
            // The trailing flush wrapper call is the "done" call — no PCM, no
            // output.
            self.flush_done = true;
            Ok(self.result(FrameSource::Flush, None, None, None, true))
        }
    }

    /// Drive the computed pipeline one core call and return the computed frame
    /// bytes for an output-bearing call (`None` for a priming call).
    fn drive(&mut self, core_call_index: u32) -> Result<Option<Vec<u8>>, FlushError> {
        let frame = &self.frames[core_call_index as usize];
        let inputs: Vec<&[f32]> = frame.iter().map(Vec::as_slice).collect();
        let computed = self
            .driver
            .step_channels(&inputs)
            .map_err(|error| FlushError::Compute {
                core_call_index,
                error,
            })?;
        Ok(computed.map(|c| c.bytes))
    }

    pub fn encode_calls(&self) -> u32 {
        self.encode_calls
    }
    pub fn flush_calls(&self) -> u32 {
        self.flush_calls
    }
    pub fn flush_remaining(&self) -> u32 {
        self.flush_remaining
    }
    pub fn output_frames_emitted(&self) -> u32 {
        self.output_frames_emitted
    }
    pub fn is_done(&self) -> bool {
        self.flush_done
    }

    fn result(
        &self,
        source: FrameSource,
        core_call_index: Option<u32>,
        output_frame_index: Option<u32>,
        frame_bytes: Option<Vec<u8>>,
        done: bool,
    ) -> FrameResult {
        let produced_bytes = frame_bytes.as_ref().map_or(0, Vec::len);
        FrameResult {
            source,
            core_call_index,
            output_frame_index,
            produced_bytes,
            frame_bytes,
            done,
            flush_remaining: self.flush_remaining,
        }
    }
}

/// Frame-oriented computed scheduler (docs/16 S2).
///
/// Unlike [`BufferedFlushScheduler`], this scheduler owns no PCM collection
/// indexed by core call. The caller supplies exactly one converted channel frame
/// to [`encode_chunk`](Self::encode_chunk), and the scheduler drives it
/// immediately through its persistent [`FrameDriver`]. A single
/// channel-count-sized zero frame is retained for native processing-flush calls;
/// the trailing done call performs no core processing.
pub struct IncrementalFlushScheduler {
    schedule: EncodeSchedule,
    driver: FrameDriver,
    channel_count: usize,
    zero_frame: Vec<Vec<f32>>,
    encode_calls: u32,
    flush_calls: u32,
    flush_remaining: u32,
    output_frames_emitted: u32,
    input_exhausted: bool,
    flush_started: bool,
    flush_done: bool,
}

impl IncrementalFlushScheduler {
    /// Construct only the native-derived schedule, persistent codec driver, and
    /// fixed-width flush storage. No caller PCM is retained here.
    pub fn new(input_sample_frames: u32, params: CodingParams) -> Result<Self, FlushError> {
        let schedule = EncodeSchedule::new(input_sample_frames)?;
        let channel_count = params.channels as usize;
        Ok(Self {
            schedule,
            driver: FrameDriver::for_params(params),
            channel_count,
            zero_frame: vec![vec![0.0; FRONTEND_FRAME_SAMPLES]; channel_count],
            encode_calls: 0,
            flush_calls: 0,
            flush_remaining: schedule.flush_wrapper_calls(),
            output_frames_emitted: 0,
            input_exhausted: false,
            flush_started: false,
            flush_done: false,
        })
    }

    pub fn schedule(&self) -> &EncodeSchedule {
        &self.schedule
    }

    /// Advance one encode wrapper call using only the caller's current converted
    /// PCM frame. Lifecycle and sample-count checks retain the legacy scheduler's
    /// precedence; channel and fixed-width checks follow before codec state is
    /// touched.
    pub fn encode_chunk(
        &mut self,
        sample_frames: u32,
        frame: &[Vec<f32>],
    ) -> Result<FrameResult, FlushError> {
        if self.flush_started {
            return Err(FlushScheduleError::EncodeAfterFlushStarted.into());
        }
        if self.encode_calls >= self.schedule.encode_calls() {
            return Err(FlushScheduleError::TooManyEncodeCalls.into());
        }
        let core_call_index = self.encode_calls;
        let expected = self.schedule.expected_encode_sample_frames(core_call_index);
        if sample_frames != expected {
            return Err(FlushScheduleError::WrongSampleFrameCount {
                core_call_index,
                expected,
                actual: sample_frames,
            }
            .into());
        }
        self.validate_frame(core_call_index, frame)?;

        let computed = Self::drive(&mut self.driver, core_call_index, frame)?;
        self.encode_calls += 1;
        if self.encode_calls == self.schedule.encode_calls() {
            self.input_exhausted = true;
            self.flush_remaining = self.schedule.flush_processing_calls();
        }

        let output_frame_index = core_call_index.checked_sub(FIRST_OUTPUT_CORE_CALL);
        if output_frame_index.is_some() {
            self.output_frames_emitted += 1;
        }
        Ok(self.result(
            FrameSource::Encode,
            Some(core_call_index),
            output_frame_index,
            computed,
            false,
        ))
    }

    /// Advance one flush wrapper call. Processing calls consume the scheduler's
    /// one fixed all-zero frame; the final done call consumes no PCM.
    pub fn flush(&mut self) -> Result<FrameResult, FlushError> {
        if !self.input_exhausted {
            return Err(FlushScheduleError::FlushBeforeInputExhausted {
                encode_calls: self.encode_calls,
                expected: self.schedule.encode_calls(),
            }
            .into());
        }
        if self.flush_done {
            return Err(FlushScheduleError::FlushAlreadyDone.into());
        }

        self.flush_started = true;
        let flush_call_index = self.flush_calls;
        self.flush_calls += 1;
        if flush_call_index < self.schedule.flush_processing_calls() {
            self.flush_remaining -= 1;
            let core_call_index = self.schedule.encode_calls() + flush_call_index;
            let output_frame_index = core_call_index.checked_sub(FIRST_OUTPUT_CORE_CALL);
            let computed = Self::drive(&mut self.driver, core_call_index, &self.zero_frame)?;
            if output_frame_index.is_some() {
                self.output_frames_emitted += 1;
            }
            Ok(self.result(
                FrameSource::Flush,
                Some(core_call_index),
                output_frame_index,
                computed,
                false,
            ))
        } else {
            self.flush_done = true;
            Ok(self.result(FrameSource::Flush, None, None, None, true))
        }
    }

    pub fn encode_calls(&self) -> u32 {
        self.encode_calls
    }
    pub fn flush_calls(&self) -> u32 {
        self.flush_calls
    }
    pub fn flush_remaining(&self) -> u32 {
        self.flush_remaining
    }
    pub fn output_frames_emitted(&self) -> u32 {
        self.output_frames_emitted
    }
    pub fn is_done(&self) -> bool {
        self.flush_done
    }

    fn validate_frame(&self, core_call_index: u32, frame: &[Vec<f32>]) -> Result<(), FlushError> {
        if frame.len() != self.channel_count {
            return Err(FlushError::FrameChannelCount {
                core_call_index,
                expected: self.channel_count,
                actual: frame.len(),
            });
        }
        for channel in frame {
            if channel.len() != FRONTEND_FRAME_SAMPLES {
                return Err(FlushError::FrameSampleLen {
                    core_call_index,
                    expected: FRONTEND_FRAME_SAMPLES,
                    actual: channel.len(),
                });
            }
        }
        Ok(())
    }

    fn drive(
        driver: &mut FrameDriver,
        core_call_index: u32,
        frame: &[Vec<f32>],
    ) -> Result<Option<Vec<u8>>, FlushError> {
        let inputs: Vec<&[f32]> = frame.iter().map(Vec::as_slice).collect();
        let computed = driver
            .step_channels(&inputs)
            .map_err(|error| FlushError::Compute {
                core_call_index,
                error,
            })?;
        Ok(computed.map(|computed| computed.bytes))
    }

    fn result(
        &self,
        source: FrameSource,
        core_call_index: Option<u32>,
        output_frame_index: Option<u32>,
        frame_bytes: Option<Vec<u8>>,
        done: bool,
    ) -> FrameResult {
        let produced_bytes = frame_bytes.as_ref().map_or(0, Vec::len);
        FrameResult {
            source,
            core_call_index,
            output_frame_index,
            produced_bytes,
            frame_bytes,
            done,
            flush_remaining: self.flush_remaining,
        }
    }
}
