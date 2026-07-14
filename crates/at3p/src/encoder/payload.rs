//!
//! This module hosts the computed assembly path ([`assemble_computed_payload`] /
//!   [`assemble_computed_atracx_file`], docs/11 Phase 3 §3.1, generalized by
//!   docs/12 §0.1) drives the pure [`ComputedFlushScheduler`] over raw PCM and
//!   COMPUTES every 2048-byte output frame for ANY input of
//!   `N >= MIN_INPUT_SAMPLE_FRAMES` sample frames. The whole encode/flush/output
//!   schedule is derived from `N` by [`ComputedSchedule352`] (native contract:
//!   left/right length mismatch, fail explicitly with a typed error instead of
//!   guessing.

use std::fmt;
use std::io::{self, Write};

use super::flush::{
    ComputedFlushError, ComputedFlushScheduler, ComputedFrameResult, ComputedSchedule352,
    FrameSource, IncrementalComputedFlushScheduler, InputTooShort,
};
use crate::encoder::coding_params::CodingParams;
use crate::encoder::computed_frame::COMPUTED_FRAME_BYTES;
use crate::encoder::frontend::{CurrentPcmFrameError, prepare_current_pcm_frame};
use crate::encoder::profile::Atrac3plusProfile;
use crate::riff::write::{
    ATRACX_HEADER_LEN, RiffWriteError, write_atracx_header, write_atracx_header_for_rate,
    write_atracx_header_for_rate_channels,
};

/// The native wrapper phase responsible for a computed-encode progress update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodePhase {
    /// A PCM-bearing `atrac_encode` wrapper call.
    Encoding,
    /// An `atrac_flush_encode` wrapper call, including the final done call.
    Flushing,
}

/// Progress after one successful computed encode or flush wrapper call.
///
/// `completed_steps / total_steps` is the work-oriented progress fraction. It
/// includes priming calls that emit no frame and the final flush done call, so
/// it advances monotonically from the first encoder call through completion.
/// `completed_output_frames / total_output_frames` separately describes the
/// native output schedule. Totals come from [`ComputedSchedule352`], whose
/// length-dependent call and frame counts are pinned against native traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeProgress {
    pub phase: EncodePhase,
    pub completed_steps: u32,
    pub total_steps: u32,
    pub completed_output_frames: u32,
    pub total_output_frames: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComputedPayloadError {
    Scheduler(ComputedFlushError),
    Prepare(CurrentPcmFrameError),
    UnexpectedZeroOutput {
        source: FrameSource,
        core_call_index: Option<u32>,
        produced_bytes: usize,
    },
    MissingFrameBytes {
        source: FrameSource,
        core_call_index: Option<u32>,
        output_frame_index: u32,
        produced_bytes: usize,
    },
    UnexpectedProducedBytes {
        source: FrameSource,
        core_call_index: Option<u32>,
        output_frame_index: u32,
        expected: usize,
        actual: usize,
    },
    UnexpectedOutputFrameOrder {
        expected: u32,
        actual: u32,
    },
    IncompleteOutputFrames {
        expected: usize,
        actual: usize,
    },
    SchedulerNotDone {
        flush_calls: u32,
    },
    FinalPayloadLength {
        expected: usize,
        actual: usize,
    },
}

impl From<ComputedFlushError> for ComputedPayloadError {
    fn from(value: ComputedFlushError) -> Self {
        Self::Scheduler(value)
    }
}

impl From<CurrentPcmFrameError> for ComputedPayloadError {
    fn from(value: CurrentPcmFrameError) -> Self {
        Self::Prepare(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComputedFileError {
    /// The input is shorter than the native minimum accepted length
    /// ([`MIN_INPUT_SAMPLE_FRAMES`](super::flush::MIN_INPUT_SAMPLE_FRAMES) =
    /// 6144). Native `at3tool` `checkEncodeParam`
    /// rejects `N < 6144` with "too short input file" before any library call
    /// (`len_edges_run.json`); there is no native schedule below the minimum, so
    /// this fails explicit, never guesses.
    InputTooShort {
        input_sample_frames: u32,
        minimum: u32,
    },
    /// The left/right channel lengths do not match the declared
    /// `input_sample_frames` (a malformed call, not a native shape). Kept as
    /// `UnsupportedInputShape` for the CLI error-formatting contract in
    /// `src/main.rs`; `expected_sample_frames == actual_sample_frames == N`, and
    /// the channel lengths differ from `N`.
    UnsupportedInputShape {
        expected_sample_frames: u32,
        actual_sample_frames: u32,
        left_len: usize,
        right_len: usize,
    },
    /// The mono entry ([`assemble_computed_atracx_file_for_mono_profile`]) was
    /// handed a channel-vec that is not exactly one channel of
    /// `expected_sample_frames` samples (a malformed call, not a native shape;
    /// docs/14 §0.4). `channel_count` is the supplied channel count and
    /// `channel_len` the first channel's length (0 when empty). Distinct from the
    /// stereo [`UnsupportedInputShape`](Self::UnsupportedInputShape) — never
    /// overload that variant's `left_len`/`right_len` with fabricated fields.
    UnsupportedMonoInputShape {
        expected_sample_frames: u32,
        channel_count: usize,
        channel_len: usize,
    },
    UnexpectedInputChannelCount {
        expected: usize,
        actual: usize,
    },
    UnexpectedInputChunkFrames {
        core_call_index: u32,
        expected: usize,
        actual: usize,
    },
    MismatchedInputChunkFrames {
        core_call_index: u32,
        channel: usize,
        expected: usize,
        actual: usize,
    },
    StreamInputAlreadyComplete,
    IncompleteStreamInput {
        expected_sample_frames: u32,
        actual_sample_frames: u32,
    },
    Header(RiffWriteError),
    Payload(ComputedPayloadError),
    FinalFileLength {
        expected: usize,
        actual: usize,
    },
    /// A bitrate lookup failed or a channel-specific helper received a profile
    /// for the other channel mode.
    UnsupportedProfile {
        bitrate_kbps: u32,
    },
}

impl From<RiffWriteError> for ComputedFileError {
    fn from(value: RiffWriteError) -> Self {
        Self::Header(value)
    }
}

impl From<ComputedPayloadError> for ComputedFileError {
    fn from(value: ComputedPayloadError) -> Self {
        Self::Payload(value)
    }
}

impl From<InputTooShort> for ComputedFileError {
    fn from(value: InputTooShort) -> Self {
        Self::InputTooShort {
            input_sample_frames: value.input_sample_frames,
            minimum: value.minimum,
        }
    }
}

/// The exact streaming-file region whose `write_all` call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputedWriteStage {
    Header,
    OutputFrame {
        source: FrameSource,
        core_call_index: Option<u32>,
        output_frame_index: u32,
    },
}

/// Typed failure from a frame-oriented computed file write.
///
/// Codec/container validation remains represented by [`ComputedFileError`].
/// Sink failures retain both their exact write stage and the original
/// [`io::Error`], including through [`std::error::Error::source`].
#[derive(Debug)]
pub enum ComputedWriteError {
    File(ComputedFileError),
    Io {
        stage: ComputedWriteStage,
        source: io::Error,
    },
}

impl From<ComputedFileError> for ComputedWriteError {
    fn from(value: ComputedFileError) -> Self {
        Self::File(value)
    }
}

impl fmt::Display for ComputedWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => write!(formatter, "computed file error: {error:?}"),
            Self::Io { stage, source } => {
                write!(
                    formatter,
                    "computed output write failed at {stage:?}: {source}"
                )
            }
        }
    }
}

impl std::error::Error for ComputedWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File(_) => None,
            Self::Io { source, .. } => Some(source),
        }
    }
}

/// Drive the pure [`ComputedFlushScheduler`] over the per-core-call PCM supply
/// for the derived schedule and assemble the computed `data` payload. Reads ZERO
/// `input_sample_frames` that produced `frames`.
pub fn assemble_computed_payload(
    schedule: &ComputedSchedule352,
    frames: Vec<[Vec<f32>; 2]>,
) -> Result<Vec<u8>, ComputedPayloadError> {
    assemble_computed_payload_with_progress(schedule, frames, |_| {})
}

/// [`assemble_computed_payload`] with a progress callback invoked after every
/// successful encode and flush wrapper call.
pub fn assemble_computed_payload_with_progress<F>(
    schedule: &ComputedSchedule352,
    frames: Vec<[Vec<f32>; 2]>,
    on_progress: F,
) -> Result<Vec<u8>, ComputedPayloadError>
where
    F: FnMut(EncodeProgress),
{
    // 352 params (selector 30, budget 16379, 2048 frame bytes, mode_a 2).
    assemble_computed_payload_for_params_with_progress(
        schedule,
        frames,
        CodingParams {
            selector: 30,
            budget: 16379,
            frame_bytes: COMPUTED_FRAME_BYTES as u32,
            // Stereo anchor (`handle+0x94` == 2).
            channels: 2,
            mode_a: 2,
            band_index: crate::encoder::coding_params::FULL_BAND_INDEX,
            // selector 30 > 0x12 → GHA enabled (docs/13 §5.1).
            gha_enabled: true,
            // selector 30 > 0x12 → mode_cc set (detector chain, docs/13 §5.2).
            mode_cc: true,
        },
        on_progress,
    )
}

/// Like [`assemble_computed_payload`] but for an explicit per-rate
/// [`CodingParams`] (docs/13 §1.1): drives the per-rate
/// [`ComputedFlushScheduler`] and sizes/validates every output frame at
/// `params.frame_bytes`. At the 352 params this equals
/// [`assemble_computed_payload`].
///
/// Pair-shaped for the nine stereo rates. Reshapes the `[left, right]` pair
/// supply to the channel-vec representation (docs/14 §0.4) and delegates to
/// [`assemble_computed_payload_for_params_channels`] — the exact delegation
/// pattern [`ComputedFlushScheduler::new_for_params`] uses — so the stereo
/// output stays bit-for-bit unchanged (pure reshape, no sample changes).
pub fn assemble_computed_payload_for_params(
    schedule: &ComputedSchedule352,
    frames: Vec<[Vec<f32>; 2]>,
    params: CodingParams,
) -> Result<Vec<u8>, ComputedPayloadError> {
    assemble_computed_payload_for_params_with_progress(schedule, frames, params, |_| {})
}

/// [`assemble_computed_payload_for_params`] with per-wrapper-call progress.
pub fn assemble_computed_payload_for_params_with_progress<F>(
    schedule: &ComputedSchedule352,
    frames: Vec<[Vec<f32>; 2]>,
    params: CodingParams,
    on_progress: F,
) -> Result<Vec<u8>, ComputedPayloadError>
where
    F: FnMut(EncodeProgress),
{
    let channel_frames: Vec<Vec<Vec<f32>>> = frames
        .into_iter()
        .map(|[left, right]| vec![left, right])
        .collect();
    assemble_computed_payload_for_params_channels_with_progress(
        schedule,
        channel_frames,
        params,
        on_progress,
    )
}

/// Like [`assemble_computed_payload_for_params`] but for a channel-vec PCM
/// supply (`frames[core_call][channel][sample]`, docs/14 §0.4): drives the
/// per-rate [`ComputedFlushScheduler`] via
/// [`ComputedFlushScheduler::new_for_params_channels`] (which validates each
/// core call's channel count against `params.channels`). The pair-shaped
/// [`assemble_computed_payload_for_params`] delegates here after reshaping;
/// the 128 kbps mono path (docs/14 §1.3) supplies a 1-channel frame vector.
pub fn assemble_computed_payload_for_params_channels(
    schedule: &ComputedSchedule352,
    frames: Vec<Vec<Vec<f32>>>,
    params: CodingParams,
) -> Result<Vec<u8>, ComputedPayloadError> {
    assemble_computed_payload_for_params_channels_with_progress(schedule, frames, params, |_| {})
}

/// [`assemble_computed_payload_for_params_channels`] with a callback after each
/// successful scheduler call and output-frame append. Existing assembly entry
/// points delegate here with a no-op callback, preserving their behavior.
pub fn assemble_computed_payload_for_params_channels_with_progress<F>(
    schedule: &ComputedSchedule352,
    frames: Vec<Vec<Vec<f32>>>,
    params: CodingParams,
    mut on_progress: F,
) -> Result<Vec<u8>, ComputedPayloadError>
where
    F: FnMut(EncodeProgress),
{
    let frame_bytes = params.frame_bytes as usize;
    let total_output_frames = schedule.total_output_frames() as usize;
    let total_steps = schedule.encode_calls() + schedule.flush_wrapper_calls();
    let payload_bytes = total_output_frames * frame_bytes;
    let mut scheduler = ComputedFlushScheduler::new_for_params_channels(
        schedule.input_sample_frames(),
        frames,
        params,
    )?;
    let mut payload = Vec::with_capacity(payload_bytes);
    let mut next_output_frame_index = 0u32;

    for core_call_index in 0..schedule.encode_calls() {
        let result =
            scheduler.encode_chunk(schedule.expected_encode_sample_frames(core_call_index))?;
        append_computed_output_frame(
            &mut payload,
            &mut next_output_frame_index,
            result,
            frame_bytes,
        )?;
        on_progress(EncodeProgress {
            phase: EncodePhase::Encoding,
            completed_steps: core_call_index + 1,
            total_steps,
            completed_output_frames: next_output_frame_index,
            total_output_frames: schedule.total_output_frames(),
        });
    }

    for flush_call_index in 0..schedule.flush_wrapper_calls() {
        let result = scheduler.flush()?;
        append_computed_output_frame(
            &mut payload,
            &mut next_output_frame_index,
            result,
            frame_bytes,
        )?;
        on_progress(EncodeProgress {
            phase: EncodePhase::Flushing,
            completed_steps: schedule.encode_calls() + flush_call_index + 1,
            total_steps,
            completed_output_frames: next_output_frame_index,
            total_output_frames: schedule.total_output_frames(),
        });
    }

    if next_output_frame_index as usize != total_output_frames {
        return Err(ComputedPayloadError::IncompleteOutputFrames {
            expected: total_output_frames,
            actual: next_output_frame_index as usize,
        });
    }

    if !scheduler.is_done() {
        return Err(ComputedPayloadError::SchedulerNotDone {
            flush_calls: scheduler.flush_calls(),
        });
    }

    if payload.len() != payload_bytes {
        return Err(ComputedPayloadError::FinalPayloadLength {
            expected: payload_bytes,
            actual: payload.len(),
        });
    }

    Ok(payload)
}

/// Frame-oriented compatibility collector (docs/16 S3). It retains only the
/// caller's decoded `i16` channels, prepares one converted frame, immediately
/// advances [`IncrementalComputedFlushScheduler`], and appends at most one
/// returned encoded frame before preparing the next call. The complete payload
/// remains intentionally buffered for the existing `Vec<u8>` public APIs.
fn assemble_computed_payload_from_pcm_channels_with_progress<F>(
    schedule: &ComputedSchedule352,
    channels: &[&[i16]],
    params: CodingParams,
    mut on_progress: F,
) -> Result<Vec<u8>, ComputedPayloadError>
where
    F: FnMut(EncodeProgress),
{
    let frame_bytes = params.frame_bytes as usize;
    let total_output_frames = schedule.total_output_frames() as usize;
    let total_steps = schedule.encode_calls() + schedule.flush_wrapper_calls();
    let payload_bytes = total_output_frames * frame_bytes;
    let mut scheduler =
        IncrementalComputedFlushScheduler::new(schedule.input_sample_frames(), params)?;
    let mut payload = Vec::with_capacity(payload_bytes);
    let mut next_output_frame_index = 0u32;

    for core_call_index in 0..schedule.encode_calls() {
        let valid_sample_frames = schedule.expected_encode_sample_frames(core_call_index);
        let source_offset = core_call_index as usize * super::frontend::FRONTEND_FRAME_SAMPLES;
        let frame =
            prepare_current_pcm_frame(channels, source_offset, valid_sample_frames as usize)?;
        let result = scheduler.encode_chunk(valid_sample_frames, &frame)?;
        append_computed_output_frame(
            &mut payload,
            &mut next_output_frame_index,
            result,
            frame_bytes,
        )?;
        on_progress(EncodeProgress {
            phase: EncodePhase::Encoding,
            completed_steps: core_call_index + 1,
            total_steps,
            completed_output_frames: next_output_frame_index,
            total_output_frames: schedule.total_output_frames(),
        });
    }

    for flush_call_index in 0..schedule.flush_wrapper_calls() {
        let result = scheduler.flush()?;
        append_computed_output_frame(
            &mut payload,
            &mut next_output_frame_index,
            result,
            frame_bytes,
        )?;
        on_progress(EncodeProgress {
            phase: EncodePhase::Flushing,
            completed_steps: schedule.encode_calls() + flush_call_index + 1,
            total_steps,
            completed_output_frames: next_output_frame_index,
            total_output_frames: schedule.total_output_frames(),
        });
    }

    if next_output_frame_index as usize != total_output_frames {
        return Err(ComputedPayloadError::IncompleteOutputFrames {
            expected: total_output_frames,
            actual: next_output_frame_index as usize,
        });
    }
    if !scheduler.is_done() {
        return Err(ComputedPayloadError::SchedulerNotDone {
            flush_calls: scheduler.flush_calls(),
        });
    }
    if payload.len() != payload_bytes {
        return Err(ComputedPayloadError::FinalPayloadLength {
            expected: payload_bytes,
            actual: payload.len(),
        });
    }
    Ok(payload)
}

/// Compute the full ATRACX file from raw stereo i16 PCM of ANY length
/// `N >= MIN_INPUT_SAMPLE_FRAMES`: derive the schedule from `N`, guard the input
/// shape, prepare and consume one PCM call at a time, compute the payload, and
/// prepend the native header (`write_atracx_header(N,
/// schedule.total_output_frames())`).
pub fn assemble_computed_atracx_file(
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
) -> Result<Vec<u8>, ComputedFileError> {
    assemble_computed_atracx_file_with_progress(input_sample_frames, left, right, |_| {})
}

/// [`assemble_computed_atracx_file`] with per-wrapper-call progress.
pub fn assemble_computed_atracx_file_with_progress<F>(
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
    on_progress: F,
) -> Result<Vec<u8>, ComputedFileError>
where
    F: FnMut(EncodeProgress),
{
    // The channels must both hold exactly `N` sample frames. (The native minimum
    // guard lives in `ComputedSchedule352::new`, invoked below.)
    if left.len() != input_sample_frames as usize || right.len() != input_sample_frames as usize {
        return Err(ComputedFileError::UnsupportedInputShape {
            expected_sample_frames: input_sample_frames,
            actual_sample_frames: input_sample_frames,
            left_len: left.len(),
            right_len: right.len(),
        });
    }

    // Reject `N < 6144` (native minimum) with the typed too-short error.
    let schedule = ComputedSchedule352::new(input_sample_frames)?;
    let total_output_frames = schedule.total_output_frames();
    let payload_bytes = total_output_frames as usize * COMPUTED_FRAME_BYTES;
    let file_bytes = ATRACX_HEADER_LEN as usize + payload_bytes;

    let params = CodingParams::for_profile(&crate::encoder::profile::ATRAC3PLUS_352);
    let payload = assemble_computed_payload_from_pcm_channels_with_progress(
        &schedule,
        &[left, right],
        params,
        on_progress,
    )?;

    let mut bytes = write_atracx_header(input_sample_frames, total_output_frames)?;
    bytes.extend_from_slice(&payload);

    if bytes.len() != file_bytes {
        return Err(ComputedFileError::FinalFileLength {
            expected: file_bytes,
            actual: bytes.len(),
        });
    }

    Ok(bytes)
}

/// Bitrate-aware entry point (docs/13 §0.1–§5.2): assemble the computed ATRACX
/// file for `profile`. All nine native ATRAC3plus stereo rates route through the
/// computed pipeline. 352 kbps delegates verbatim to
/// [`assemble_computed_atracx_file`] (so its shipped output stays
/// byte-identical); the other eight run
/// [`assemble_computed_atracx_file_for_rate`]. Profiles exist only for those
/// nine native stereo rows, so no accepted profile remains unported.
pub fn assemble_computed_atracx_file_for_profile(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
) -> Result<Vec<u8>, ComputedFileError> {
    assemble_computed_atracx_file_for_profile_with_progress(
        profile,
        input_sample_frames,
        left,
        right,
        |_| {},
    )
}

/// [`assemble_computed_atracx_file_for_profile`] with per-wrapper-call
/// progress. The callback is not invoked when profile, shape, or minimum-length
/// validation fails before the computed scheduler starts.
pub fn assemble_computed_atracx_file_for_profile_with_progress<F>(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
    on_progress: F,
) -> Result<Vec<u8>, ComputedFileError>
where
    F: FnMut(EncodeProgress),
{
    if profile.channels() != 2 {
        return Err(ComputedFileError::UnsupportedProfile {
            bitrate_kbps: profile.bitrate_kbps(),
        });
    }
    if profile.bitrate_kbps() == 352 {
        assemble_computed_atracx_file_with_progress(input_sample_frames, left, right, on_progress)
    } else {
        assemble_computed_atracx_file_for_rate_with_progress(
            profile,
            input_sample_frames,
            left,
            right,
            on_progress,
        )
    }
}

/// Stream a computed stereo ATRACX file to `writer` without buffering the
/// compressed payload or complete output file.
pub fn write_computed_atracx_file_for_profile<W>(
    writer: &mut W,
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
) -> Result<(), ComputedWriteError>
where
    W: Write,
{
    write_computed_atracx_file_for_profile_with_progress(
        writer,
        profile,
        input_sample_frames,
        left,
        right,
        |_| {},
    )
}

/// [`write_computed_atracx_file_for_profile`] with progress after every
/// successful wrapper call. An output-bearing call is successful only after
/// its complete frame has been accepted by `writer`.
pub fn write_computed_atracx_file_for_profile_with_progress<W, F>(
    writer: &mut W,
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
    on_progress: F,
) -> Result<(), ComputedWriteError>
where
    W: Write,
    F: FnMut(EncodeProgress),
{
    if profile.channels() != 2 {
        return Err(ComputedFileError::UnsupportedProfile {
            bitrate_kbps: profile.bitrate_kbps(),
        }
        .into());
    }
    if left.len() != input_sample_frames as usize || right.len() != input_sample_frames as usize {
        return Err(ComputedFileError::UnsupportedInputShape {
            expected_sample_frames: input_sample_frames,
            actual_sample_frames: input_sample_frames,
            left_len: left.len(),
            right_len: right.len(),
        }
        .into());
    }

    let schedule = ComputedSchedule352::new(input_sample_frames)
        .map_err(ComputedFileError::from)
        .map_err(ComputedWriteError::from)?;
    let mut stream =
        super::stream::Atrac3plusStreamEncoder::new(writer, profile, input_sample_frames)?;
    let mut on_progress = on_progress;
    for core_call_index in 0..schedule.encode_calls() {
        let offset = core_call_index as usize * super::frontend::FRONTEND_FRAME_SAMPLES;
        let frames = schedule.expected_encode_sample_frames(core_call_index) as usize;
        stream.push_pcm_with_progress(
            &[
                &left[offset..offset + frames],
                &right[offset..offset + frames],
            ],
            &mut on_progress,
        )?;
    }
    stream.finish_with_progress(on_progress).map(|_| ())
}

/// Mono entry point (docs/14 §0.1, §0.4, §1.3, §2.1) — the channel-aware sibling of
/// [`assemble_computed_atracx_file_for_profile`]. Validates the call in the
/// native at3tool precedence order (`checkEncodeParam` runs BEFORE the
/// `getAtracEncodeSetting` row/rate match, decompiled/at3tool.c 1787-1789; E2),
/// then routes by rate:
///
/// 1. **profile guard** — `profile` must use mono channel mode, else
///    [`ComputedFileError::UnsupportedProfile`].
/// 2. **shape guard** — exactly one channel of `input_sample_frames` samples,
///    else [`ComputedFileError::UnsupportedMonoInputShape`] (a malformed call,
///    not a native shape).
/// 3. **too-short guard** — `N >= 6144`, else [`ComputedFileError::InputTooShort`].
///    Native `checkEncodeParam` rejects `N < 0x1800` BEFORE the rate/row match,
///    channel-independent (E2); the schedule law is channel-independent (E3), so
///    [`ComputedSchedule352::new`] governs the minimum here too.
/// 4. **rate routing** — all five mono rows are LANDED: 128 kbps (docs/14 §1.3),
///    96 kbps (docs/14 §2.1), 64 kbps (docs/14 §3.1), 48 kbps (docs/14 §4.1),
///    and 32 kbps (docs/14 §5.1). Each runs the channel-vec computed pipeline
///    (proven end-to-end by `tests/computed_frames_mono.rs` — 128: 51/77,
///    96: 39/77, 64: 32/77, 48: 34/77, 32: 25/77 frames byte-exact incl. frame 0
///    vs the native oracle) and returns the assembled ATRACX file. Guard (a)
///    above already rejects any other rate with
///    [`ComputedFileError::UnsupportedProfile`], so no accepted mono profile
///    remains unported (the retired "accepted but unported" state, mirroring the
///    stereo close-out).
pub fn assemble_computed_atracx_file_for_mono_profile(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    channels: &[Vec<i16>],
) -> Result<Vec<u8>, ComputedFileError> {
    assemble_computed_atracx_file_for_mono_profile_with_progress(
        profile,
        input_sample_frames,
        channels,
        |_| {},
    )
}

/// [`assemble_computed_atracx_file_for_mono_profile`] with per-wrapper-call
/// progress. Validation and error precedence are identical to the no-callback
/// entry point.
pub fn assemble_computed_atracx_file_for_mono_profile_with_progress<F>(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    channels: &[Vec<i16>],
    on_progress: F,
) -> Result<Vec<u8>, ComputedFileError>
where
    F: FnMut(EncodeProgress),
{
    // (a) profile guard: this entry point accepts validated mono profiles.
    if profile.channels() != 1 {
        return Err(ComputedFileError::UnsupportedProfile {
            bitrate_kbps: profile.bitrate_kbps(),
        });
    }

    // (b) shape guard: exactly one channel whose length is the declared N.
    if channels.len() != 1 || channels[0].len() != input_sample_frames as usize {
        return Err(ComputedFileError::UnsupportedMonoInputShape {
            expected_sample_frames: input_sample_frames,
            channel_count: channels.len(),
            channel_len: channels.first().map_or(0, Vec::len),
        });
    }

    // (c) too-short guard (native precedence: BEFORE the rate/row match, E2).
    let schedule = ComputedSchedule352::new(input_sample_frames)?;

    // (d) rate routing. All five mono rows — 128 (docs/14 §1.3), 96 (docs/14
    // §2.1), 64 (docs/14 §3.1), 48 (docs/14 §4.1), 32 (docs/14 §5.1) — run the
    // shared rate-generic computed assembly path. Guard (a) already rejected any
    // other rate, so no accepted mono profile remains unported.
    assemble_computed_mono_file_for_rate(
        profile,
        &schedule,
        input_sample_frames,
        channels,
        on_progress,
    )
}

/// Stream a computed mono ATRACX file to `writer` without buffering the
/// compressed payload or complete output file.
pub fn write_computed_atracx_file_for_mono_profile<W>(
    writer: &mut W,
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    channels: &[Vec<i16>],
) -> Result<(), ComputedWriteError>
where
    W: Write,
{
    write_computed_atracx_file_for_mono_profile_with_progress(
        writer,
        profile,
        input_sample_frames,
        channels,
        |_| {},
    )
}

/// [`write_computed_atracx_file_for_mono_profile`] with progress after every
/// successful wrapper call. Validation order is identical to the buffered
/// mono entry point.
pub fn write_computed_atracx_file_for_mono_profile_with_progress<W, F>(
    writer: &mut W,
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    channels: &[Vec<i16>],
    on_progress: F,
) -> Result<(), ComputedWriteError>
where
    W: Write,
    F: FnMut(EncodeProgress),
{
    if profile.channels() != 1 {
        return Err(ComputedFileError::UnsupportedProfile {
            bitrate_kbps: profile.bitrate_kbps(),
        }
        .into());
    }
    if channels.len() != 1 || channels[0].len() != input_sample_frames as usize {
        return Err(ComputedFileError::UnsupportedMonoInputShape {
            expected_sample_frames: input_sample_frames,
            channel_count: channels.len(),
            channel_len: channels.first().map_or(0, Vec::len),
        }
        .into());
    }

    let schedule = ComputedSchedule352::new(input_sample_frames)
        .map_err(ComputedFileError::from)
        .map_err(ComputedWriteError::from)?;
    let mut stream =
        super::stream::Atrac3plusStreamEncoder::new(writer, profile, input_sample_frames)?;
    let mut on_progress = on_progress;
    for core_call_index in 0..schedule.encode_calls() {
        let offset = core_call_index as usize * super::frontend::FRONTEND_FRAME_SAMPLES;
        let frames = schedule.expected_encode_sample_frames(core_call_index) as usize;
        stream
            .push_pcm_with_progress(&[&channels[0][offset..offset + frames]], &mut on_progress)?;
    }
    stream.finish_with_progress(on_progress).map(|_| ())
}

/// Assemble the full MONO ATRACX file for a LANDED mono rate (128 kbps docs/14
/// §1.3; 96 kbps docs/14 §2.1; 64 kbps docs/14 §3.1; 48 kbps docs/14 §4.1;
/// 32 kbps docs/14 §5.1): the mono sibling of
/// [`assemble_computed_atracx_file_for_rate`], rate-generic via
/// `profile.frame_bytes()` / [`CodingParams::for_profile`] / `profile.codec_info()`.
/// The frame SCHEDULE is channel-independent (E3), so the caller's
/// [`ComputedSchedule352`] governs the output-frame count; one current-call
/// scalar frame is prepared and immediately driven through the incremental
/// computed pipeline with the mono [`CodingParams`] (`channels == 1`). The
/// header is the mono row's (`write_atracx_header_for_rate_channels(1, …)`, pinned
/// byte-for-byte by tests/container_by_rate_mono.rs). Like the per-rate stereo
/// path there is NO packer-boundary byte oracle here by design (x87 divergence
/// past the parity horizon; the perceptual battery owns full-file acceptance).
/// The composed-parity anchor is `tests/computed_frames_mono.rs` (128: 51/77;
/// 96: 39/77; 64: 32/77; 48: 34/77; 32: 25/77 frames byte-exact incl. frame 0 vs
/// the native oracle).
fn assemble_computed_mono_file_for_rate<F>(
    profile: &Atrac3plusProfile,
    schedule: &ComputedSchedule352,
    input_sample_frames: u32,
    channels: &[Vec<i16>],
    on_progress: F,
) -> Result<Vec<u8>, ComputedFileError>
where
    F: FnMut(EncodeProgress),
{
    let total_output_frames = schedule.total_output_frames();
    let frame_bytes = profile.frame_bytes();
    let payload_bytes = total_output_frames as usize * frame_bytes as usize;
    let file_bytes = ATRACX_HEADER_LEN as usize + payload_bytes;

    let params = CodingParams::for_profile(profile);
    let payload = assemble_computed_payload_from_pcm_channels_with_progress(
        schedule,
        &[channels[0].as_slice()],
        params,
        on_progress,
    )?;

    let mut bytes = write_atracx_header_for_rate_channels(
        1,
        input_sample_frames,
        total_output_frames,
        frame_bytes as u16,
        profile.codec_info(),
    )?;
    bytes.extend_from_slice(&payload);

    if bytes.len() != file_bytes {
        return Err(ComputedFileError::FinalFileLength {
            expected: file_bytes,
            actual: bytes.len(),
        });
    }

    Ok(bytes)
}

/// Compute the full per-rate ATRACX file (docs/13 §1.1–§5.2): the shared
/// pipeline behind 320/256/192/160/128/96/64/48. The final two use the
/// mode_cc==0 low-selector `set_gainc_at5` path.
/// Mirrors [`assemble_computed_atracx_file`] but threads the profile's per-rate
/// coding params ([`CodingParams::for_profile`]) into the computed pipeline and
/// sizes the payload / RIFF header at `profile.frame_bytes()` via
/// [`write_atracx_header_for_rate`]. The frame SCHEDULE is rate-independent
/// (docs/13 §2.3), so [`ComputedSchedule352`] governs the output-frame count.
/// There is NO packer-boundary byte oracle at a non-352 rate by design (x87
/// divergence; the perceptual battery owns full-file acceptance) — this path is
/// structurally exact (frame count / sizing / container) but not byte-pinned
/// against native payload bytes.
pub fn assemble_computed_atracx_file_for_rate(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
) -> Result<Vec<u8>, ComputedFileError> {
    assemble_computed_atracx_file_for_rate_with_progress(
        profile,
        input_sample_frames,
        left,
        right,
        |_| {},
    )
}

/// [`assemble_computed_atracx_file_for_rate`] with per-wrapper-call progress.
pub fn assemble_computed_atracx_file_for_rate_with_progress<F>(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    left: &[i16],
    right: &[i16],
    on_progress: F,
) -> Result<Vec<u8>, ComputedFileError>
where
    F: FnMut(EncodeProgress),
{
    if profile.channels() != 2 {
        return Err(ComputedFileError::UnsupportedProfile {
            bitrate_kbps: profile.bitrate_kbps(),
        });
    }
    // Both channels must hold exactly `N` sample frames.
    if left.len() != input_sample_frames as usize || right.len() != input_sample_frames as usize {
        return Err(ComputedFileError::UnsupportedInputShape {
            expected_sample_frames: input_sample_frames,
            actual_sample_frames: input_sample_frames,
            left_len: left.len(),
            right_len: right.len(),
        });
    }

    // Reject `N < 6144` (native minimum) with the typed too-short error.
    let schedule = ComputedSchedule352::new(input_sample_frames)?;
    let total_output_frames = schedule.total_output_frames();
    let frame_bytes = profile.frame_bytes();
    let payload_bytes = total_output_frames as usize * frame_bytes as usize;
    let file_bytes = ATRACX_HEADER_LEN as usize + payload_bytes;

    let params = CodingParams::for_profile(profile);
    let payload = assemble_computed_payload_from_pcm_channels_with_progress(
        &schedule,
        &[left, right],
        params,
        on_progress,
    )?;

    let mut bytes = write_atracx_header_for_rate(
        input_sample_frames,
        total_output_frames,
        frame_bytes as u16,
        profile.codec_info(),
    )?;
    bytes.extend_from_slice(&payload);

    if bytes.len() != file_bytes {
        return Err(ComputedFileError::FinalFileLength {
            expected: file_bytes,
            actual: bytes.len(),
        });
    }

    Ok(bytes)
}

pub(crate) fn write_computed_output_frame<W>(
    writer: &mut W,
    next_output_frame_index: &mut u32,
    written_payload_bytes: &mut usize,
    result: ComputedFrameResult,
    expected_frame_bytes: usize,
) -> Result<(), ComputedWriteError>
where
    W: Write,
{
    match result.output_frame_index {
        Some(output_frame_index) => {
            if output_frame_index != *next_output_frame_index {
                return Err(ComputedFileError::Payload(
                    ComputedPayloadError::UnexpectedOutputFrameOrder {
                        expected: *next_output_frame_index,
                        actual: output_frame_index,
                    },
                )
                .into());
            }
            if result.produced_bytes != expected_frame_bytes {
                return Err(ComputedFileError::Payload(
                    ComputedPayloadError::UnexpectedProducedBytes {
                        source: result.source,
                        core_call_index: result.core_call_index,
                        output_frame_index,
                        expected: expected_frame_bytes,
                        actual: result.produced_bytes,
                    },
                )
                .into());
            }
            let frame = result.frame_bytes.ok_or_else(|| {
                ComputedFileError::Payload(ComputedPayloadError::MissingFrameBytes {
                    source: result.source,
                    core_call_index: result.core_call_index,
                    output_frame_index,
                    produced_bytes: result.produced_bytes,
                })
            })?;
            if frame.len() != expected_frame_bytes {
                return Err(ComputedFileError::Payload(
                    ComputedPayloadError::UnexpectedProducedBytes {
                        source: result.source,
                        core_call_index: result.core_call_index,
                        output_frame_index,
                        expected: expected_frame_bytes,
                        actual: frame.len(),
                    },
                )
                .into());
            }
            writer
                .write_all(&frame)
                .map_err(|source| ComputedWriteError::Io {
                    stage: ComputedWriteStage::OutputFrame {
                        source: result.source,
                        core_call_index: result.core_call_index,
                        output_frame_index,
                    },
                    source,
                })?;
            *written_payload_bytes += frame.len();
            *next_output_frame_index += 1;
        }
        None => {
            if result.produced_bytes != 0 || result.frame_bytes.is_some() {
                return Err(ComputedFileError::Payload(
                    ComputedPayloadError::UnexpectedZeroOutput {
                        source: result.source,
                        core_call_index: result.core_call_index,
                        produced_bytes: result.produced_bytes,
                    },
                )
                .into());
            }
        }
    }
    Ok(())
}

fn append_computed_output_frame(
    payload: &mut Vec<u8>,
    next_output_frame_index: &mut u32,
    result: ComputedFrameResult,
    frame_bytes: usize,
) -> Result<(), ComputedPayloadError> {
    match result.output_frame_index {
        Some(output_frame_index) => {
            if output_frame_index != *next_output_frame_index {
                return Err(ComputedPayloadError::UnexpectedOutputFrameOrder {
                    expected: *next_output_frame_index,
                    actual: output_frame_index,
                });
            }
            if result.produced_bytes != frame_bytes {
                return Err(ComputedPayloadError::UnexpectedProducedBytes {
                    source: result.source,
                    core_call_index: result.core_call_index,
                    output_frame_index,
                    expected: frame_bytes,
                    actual: result.produced_bytes,
                });
            }
            let frame_bytes =
                result
                    .frame_bytes
                    .ok_or(ComputedPayloadError::MissingFrameBytes {
                        source: result.source,
                        core_call_index: result.core_call_index,
                        output_frame_index,
                        produced_bytes: result.produced_bytes,
                    })?;
            payload.extend_from_slice(&frame_bytes);
            *next_output_frame_index += 1;
        }
        None => {
            if result.produced_bytes != 0 || result.frame_bytes.is_some() {
                return Err(ComputedPayloadError::UnexpectedZeroOutput {
                    source: result.source,
                    core_call_index: result.core_call_index,
                    produced_bytes: result.produced_bytes,
                });
            }
        }
    }
    Ok(())
}
