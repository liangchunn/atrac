use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Write};

use crate::config::{Atrac3Profile, EncoderStrategy, UnsupportedProfile};
use crate::core::{EncoderCore, FrameEncodeError, PcmFrame};

pub const PCM_BLOCK_FRAMES: usize = 1024;
const ENCODER_DELAY: usize = 69;
const HEADER_BYTES: u64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStage {
    Header,
    OutputFrame { output_frame_index: u32 },
}

/// The stream phase responsible for an ATRAC3 encode progress update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodePhase {
    /// Work completed while PCM was being supplied to the stream.
    Encoding,
    /// Tail work completed after all PCM had been supplied.
    Flushing,
}

/// Progress after one successfully encoded ATRAC3 sound unit.
///
/// `completed_steps / total_steps` includes priming sound units that do not
/// produce output. `completed_output_frames / total_output_frames` separately
/// reports the number of payload frames written to the output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeProgress {
    pub phase: EncodePhase,
    pub completed_steps: u32,
    pub total_steps: u32,
    pub completed_output_frames: u32,
    pub total_output_frames: u32,
}

#[derive(Debug)]
pub enum Atrac3StreamError {
    UnsupportedProfile(UnsupportedProfile),
    EmptyInput,
    RiffSizeOverflow {
        payload_bytes: u64,
    },
    SilenceFrame,
    FrameOutputTooSmall {
        needed: usize,
        actual: usize,
    },
    WrongChannelCount {
        expected: usize,
        actual: usize,
    },
    MismatchedChannelLengths {
        channel: usize,
        expected: usize,
        actual: usize,
    },
    WrongChunkFrames {
        expected: usize,
        actual: usize,
        received: u32,
    },
    InputAlreadyComplete,
    IncompleteInput {
        expected: u32,
        actual: u32,
    },
    IncompleteSchedule {
        expected_sound_units: usize,
        actual_sound_units: usize,
    },
    FinalPayloadLength {
        expected: u64,
        actual: u64,
    },
    Io {
        stage: WriteStage,
        source: io::Error,
    },
}

impl fmt::Display for Atrac3StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProfile(error) => error.fmt(f),
            Self::EmptyInput => write!(f, "not enough samples: need at least one sample"),
            Self::RiffSizeOverflow { payload_bytes } => write!(
                f,
                "ATRAC3 output is too large for a RIFF/WAVE container ({payload_bytes} payload bytes)"
            ),
            Self::SilenceFrame => write!(f, "failed to build fallback silence frame"),
            Self::FrameOutputTooSmall { needed, actual } => write!(
                f,
                "ATRAC3 frame output buffer has {actual} bytes; need {needed}"
            ),
            Self::WrongChannelCount { expected, actual } => {
                write!(f, "expected {expected} PCM channel(s), got {actual}")
            }
            Self::MismatchedChannelLengths {
                channel,
                expected,
                actual,
            } => write!(
                f,
                "PCM channel {channel} has {actual} frames; expected {expected}"
            ),
            Self::WrongChunkFrames {
                expected,
                actual,
                received,
            } => write!(
                f,
                "PCM chunk after {received} frames has {actual} frames; expected {expected}"
            ),
            Self::InputAlreadyComplete => write!(f, "PCM input is already complete"),
            Self::IncompleteInput { expected, actual } => write!(
                f,
                "PCM input ended after {actual} frames; expected {expected}"
            ),
            Self::IncompleteSchedule {
                expected_sound_units,
                actual_sound_units,
            } => write!(
                f,
                "ATRAC3 schedule ended after {actual_sound_units} sound units; expected {expected_sound_units}"
            ),
            Self::FinalPayloadLength { expected, actual } => {
                write!(f, "ATRAC3 payload has {actual} bytes; expected {expected}")
            }
            Self::Io { stage, source } => write!(f, "ATRAC3 {stage:?} I/O failed: {source}"),
        }
    }
}

impl std::error::Error for Atrac3StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedProfile(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<UnsupportedProfile> for Atrac3StreamError {
    fn from(value: UnsupportedProfile) -> Self {
        Self::UnsupportedProfile(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atrac3StreamSummary {
    pub input_sample_frames: u32,
    pub output_frames: u32,
    pub encoded_frames: u32,
    pub fallback_frames: u32,
    pub payload_bytes: u64,
    pub file_bytes: u64,
}

pub struct Atrac3StreamEncoder<W: Write> {
    writer: W,
    encoder: EncoderCore,
    profile: Atrac3Profile,
    sample_frames: u32,
    sound_units: usize,
    next_sound_unit: usize,
    received_frames: u32,
    buffer_start: u32,
    pcm: Vec<VecDeque<i16>>,
    out_buf: Vec<u8>,
    silence_frame: Vec<u8>,
    expected_payload_bytes: u64,
    written_payload_bytes: u64,
    encoded_frames: u32,
    fallback_frames: u32,
    frame_failure_policy: FrameFailurePolicy,
}

/// Stream-level decision applied after a typed core failure. The default
/// preserves the historical ATRAC3 silence-frame substitution behavior while
/// keeping capacity/programming failures fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameFailurePolicy {
    SubstituteSilenceForCodecFailure,
}

impl FrameFailurePolicy {
    fn substitutes(self, error: FrameEncodeError) -> bool {
        match self {
            Self::SubstituteSilenceForCodecFailure => error.is_silence_fallback_eligible(),
        }
    }
}

impl<W: Write> Atrac3StreamEncoder<W> {
    pub fn new(
        mut writer: W,
        profile: Atrac3Profile,
        sample_frames: u32,
    ) -> Result<Self, Atrac3StreamError> {
        if sample_frames == 0 {
            return Err(Atrac3StreamError::EmptyInput);
        }
        let encoder = EncoderCore::new(profile);
        let sound_units = sound_units_to_encode(sample_frames as usize, profile);
        let payload_frames = sound_units - profile.priming_sound_units();
        let expected_payload_bytes = (payload_frames as u64)
            .checked_mul(profile.frame_bytes() as u64)
            .ok_or(Atrac3StreamError::RiffSizeOverflow {
                payload_bytes: u64::MAX,
            })?;
        if expected_payload_bytes > u64::from(u32::MAX)
            || HEADER_BYTES + expected_payload_bytes - 8 > u64::from(u32::MAX)
        {
            return Err(Atrac3StreamError::RiffSizeOverflow {
                payload_bytes: expected_payload_bytes,
            });
        }

        let silence_frame = build_silence_frame(profile)?;
        let header = build_header(
            sample_frames,
            profile.frame_bytes() as u32,
            profile.channels(),
            profile.is_joint_stereo(),
            expected_payload_bytes as u32,
        );
        writer
            .write_all(&header)
            .map_err(|source| Atrac3StreamError::Io {
                stage: WriteStage::Header,
                source,
            })?;

        Ok(Self {
            writer,
            encoder,
            profile,
            sample_frames,
            sound_units,
            next_sound_unit: 0,
            received_frames: 0,
            buffer_start: 0,
            pcm: (0..profile.channels())
                .map(|_| VecDeque::with_capacity(PCM_BLOCK_FRAMES * 2))
                .collect(),
            out_buf: vec![0; 48 * 1024],
            silence_frame,
            expected_payload_bytes,
            written_payload_bytes: 0,
            encoded_frames: 0,
            fallback_frames: 0,
            frame_failure_policy: FrameFailurePolicy::SubstituteSilenceForCodecFailure,
        })
    }

    pub fn push_pcm(&mut self, channels: &[&[i16]]) -> Result<(), Atrac3StreamError> {
        self.push_pcm_with_progress(channels, |_| {})
    }

    /// Number of PCM sample frames expected in the next chunk.
    pub fn expected_next_chunk_frames(&self) -> Option<usize> {
        (self.received_frames < self.sample_frames).then(|| {
            usize::min(
                PCM_BLOCK_FRAMES,
                (self.sample_frames - self.received_frames) as usize,
            )
        })
    }

    /// Supply one PCM chunk and report progress after every sound unit encoded.
    pub fn push_pcm_with_progress<F>(
        &mut self,
        channels: &[&[i16]],
        mut on_progress: F,
    ) -> Result<(), Atrac3StreamError>
    where
        F: FnMut(EncodeProgress),
    {
        if channels.len() != self.profile.channels() as usize {
            return Err(Atrac3StreamError::WrongChannelCount {
                expected: self.profile.channels() as usize,
                actual: channels.len(),
            });
        }
        if self.received_frames == self.sample_frames {
            return Err(Atrac3StreamError::InputAlreadyComplete);
        }
        let expected = self
            .expected_next_chunk_frames()
            .expect("incomplete input has a next PCM chunk");
        let actual = channels.first().map_or(0, |channel| channel.len());
        if actual != expected {
            return Err(Atrac3StreamError::WrongChunkFrames {
                expected,
                actual,
                received: self.received_frames,
            });
        }
        for (index, channel) in channels.iter().enumerate().skip(1) {
            if channel.len() != actual {
                return Err(Atrac3StreamError::MismatchedChannelLengths {
                    channel: index,
                    expected: actual,
                    actual: channel.len(),
                });
            }
        }
        for (buffer, channel) in self.pcm.iter_mut().zip(channels) {
            buffer.extend(channel.iter().copied());
        }
        self.received_frames += actual as u32;
        self.process_ready(false, &mut on_progress)
    }

    pub fn finish(self) -> Result<(W, Atrac3StreamSummary), Atrac3StreamError> {
        self.finish_with_progress(|_| {})
    }

    /// Finish the stream and report progress after every tail sound unit.
    pub fn finish_with_progress<F>(
        mut self,
        mut on_progress: F,
    ) -> Result<(W, Atrac3StreamSummary), Atrac3StreamError>
    where
        F: FnMut(EncodeProgress),
    {
        if self.received_frames != self.sample_frames {
            return Err(Atrac3StreamError::IncompleteInput {
                expected: self.sample_frames,
                actual: self.received_frames,
            });
        }
        self.process_ready(true, &mut on_progress)?;
        if self.next_sound_unit != self.sound_units {
            return Err(Atrac3StreamError::IncompleteSchedule {
                expected_sound_units: self.sound_units,
                actual_sound_units: self.next_sound_unit,
            });
        }
        if self.written_payload_bytes != self.expected_payload_bytes {
            return Err(Atrac3StreamError::FinalPayloadLength {
                expected: self.expected_payload_bytes,
                actual: self.written_payload_bytes,
            });
        }
        let summary = Atrac3StreamSummary {
            input_sample_frames: self.sample_frames,
            output_frames: self.encoded_frames + self.fallback_frames,
            encoded_frames: self.encoded_frames,
            fallback_frames: self.fallback_frames,
            payload_bytes: self.written_payload_bytes,
            file_bytes: HEADER_BYTES + self.written_payload_bytes,
        };
        Ok((self.writer, summary))
    }

    fn process_ready<F>(
        &mut self,
        finalizing: bool,
        on_progress: &mut F,
    ) -> Result<(), Atrac3StreamError>
    where
        F: FnMut(EncodeProgress),
    {
        while self.next_sound_unit < self.sound_units {
            let base = input_base_frame_for_sound_unit(
                self.next_sound_unit,
                self.profile.strategy() == EncoderStrategy::Dba,
                ENCODER_DELAY,
            );
            if !finalizing && base + PCM_BLOCK_FRAMES as isize > self.received_frames as isize {
                break;
            }
            self.encode_sound_unit(base)?;
            self.next_sound_unit += 1;
            on_progress(self.progress(if finalizing {
                EncodePhase::Flushing
            } else {
                EncodePhase::Encoding
            }));
            self.discard_consumed_pcm();
        }
        Ok(())
    }

    fn progress(&self, phase: EncodePhase) -> EncodeProgress {
        let priming = self.profile.priming_sound_units();
        EncodeProgress {
            phase,
            completed_steps: self.next_sound_unit as u32,
            total_steps: self.sound_units as u32,
            completed_output_frames: self.next_sound_unit.saturating_sub(priming) as u32,
            total_output_frames: self.sound_units.saturating_sub(priming) as u32,
        }
    }

    fn encode_sound_unit(&mut self, input_base_frame: isize) -> Result<(), Atrac3StreamError> {
        let sound_unit = self.next_sound_unit;
        let write_frame = write_sound_unit(sound_unit, self.profile);
        let mut pcm0 = [0.0f32; PCM_BLOCK_FRAMES];
        let mut pcm1 = [0.0f32; PCM_BLOCK_FRAMES];
        for index in 0..PCM_BLOCK_FRAMES {
            let source_index = input_base_frame + index as isize;
            let left = self.sample_at(0, source_index);
            let right = if self.profile.channels() == 1 {
                left
            } else {
                self.sample_at(1, source_index)
            };
            pcm0[index] = f32::from(left);
            pcm1[index] = f32::from(right);
        }
        let encoded = self.encoder.encode_frame(
            PcmFrame::new([&pcm0, &pcm1]),
            &mut self.out_buf,
            self.profile.internal_frame_bytes(),
        );
        if !write_frame {
            return Ok(());
        }

        let bytes = match encoded {
            Ok(frame) if frame.byte_count() <= self.silence_frame.len() => {
                self.encoded_frames += 1;
                &self.out_buf[..self.profile.frame_bytes()]
            }
            Ok(_) => {
                self.fallback_frames += 1;
                &self.silence_frame[..self.profile.frame_bytes()]
            }
            Err(error) if self.frame_failure_policy.substitutes(error) => {
                self.fallback_frames += 1;
                &self.silence_frame[..self.profile.frame_bytes()]
            }
            Err(FrameEncodeError::OutputTooSmall { needed, actual }) => {
                return Err(Atrac3StreamError::FrameOutputTooSmall { needed, actual });
            }
            Err(_) => unreachable!("all non-capacity core errors are fallback eligible"),
        };
        let output_frame_index =
            (self.written_payload_bytes / self.profile.frame_bytes() as u64) as u32;
        self.writer
            .write_all(bytes)
            .map_err(|source| Atrac3StreamError::Io {
                stage: WriteStage::OutputFrame { output_frame_index },
                source,
            })?;
        self.written_payload_bytes += bytes.len() as u64;
        Ok(())
    }

    fn sample_at(&self, channel: usize, index: isize) -> i16 {
        if index < 0 || index >= self.sample_frames as isize {
            return 0;
        }
        let index = index as u32;
        let offset = index
            .checked_sub(self.buffer_start)
            .expect("streaming ATRAC3 PCM was discarded too early") as usize;
        *self.pcm[channel]
            .get(offset)
            .expect("streaming ATRAC3 PCM window was not supplied")
    }

    fn discard_consumed_pcm(&mut self) {
        let next_required = if self.next_sound_unit < self.sound_units {
            input_base_frame_for_sound_unit(
                self.next_sound_unit,
                self.profile.strategy() == EncoderStrategy::Dba,
                ENCODER_DELAY,
            )
            .max(0) as u32
        } else {
            self.received_frames
        };
        let discard = usize::min(
            next_required.saturating_sub(self.buffer_start) as usize,
            self.pcm.first().map_or(0, VecDeque::len),
        );
        for channel in &mut self.pcm {
            channel.drain(..discard);
        }
        self.buffer_start += discard as u32;
    }

    #[cfg(test)]
    fn buffered_frames(&self) -> usize {
        self.pcm.first().map_or(0, VecDeque::len)
    }
}

fn build_silence_frame(profile: Atrac3Profile) -> Result<Vec<u8>, Atrac3StreamError> {
    let mut encoder = EncoderCore::new(profile);
    let silence0 = [0.0f32; PCM_BLOCK_FRAMES];
    let silence1 = [0.0f32; PCM_BLOCK_FRAMES];
    let mut frame = vec![0; profile.internal_frame_bytes()];
    let encoded = encoder
        .encode_frame(
            PcmFrame::new([&silence0, &silence1]),
            &mut frame,
            profile.internal_frame_bytes(),
        )
        .map_err(|_| Atrac3StreamError::SilenceFrame)?;
    if encoded.byte_count() > profile.internal_frame_bytes() {
        return Err(Atrac3StreamError::SilenceFrame);
    }
    Ok(frame)
}

fn build_header(
    sample_count: u32,
    frame_size: u32,
    channels: u16,
    joint_stereo: bool,
    total_data_size: u32,
) -> Vec<u8> {
    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    fn push_u32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    let file_size = HEADER_BYTES + u64::from(total_data_size);
    let mut header = Vec::with_capacity(HEADER_BYTES as usize);
    header.extend_from_slice(b"RIFF");
    push_u32(&mut header, (file_size - 8) as u32);
    header.extend_from_slice(b"WAVEfmt ");
    push_u32(&mut header, 32);
    push_u16(&mut header, 0x0270);
    push_u16(&mut header, channels);
    push_u32(&mut header, Atrac3Profile::SAMPLE_RATE);
    let byte_rate = (u64::from(frame_size) * u64::from(Atrac3Profile::SAMPLE_RATE))
        .div_ceil(PCM_BLOCK_FRAMES as u64) as u32;
    push_u32(&mut header, byte_rate);
    push_u16(&mut header, frame_size as u16);
    push_u16(&mut header, 0);
    push_u16(&mut header, 14);
    push_u16(&mut header, 1);
    push_u32(&mut header, 0x1000);
    let mode = u16::from(joint_stereo);
    push_u16(&mut header, mode);
    push_u16(&mut header, mode);
    push_u16(&mut header, 1);
    push_u16(&mut header, 0);
    header.extend_from_slice(b"fact");
    push_u32(&mut header, 12);
    push_u32(&mut header, sample_count);
    push_u32(&mut header, Atrac3Profile::FRAME_SAMPLES);
    push_u32(&mut header, Atrac3Profile::FRAME_SAMPLES);
    header.extend_from_slice(b"data");
    push_u32(&mut header, total_data_size);
    debug_assert_eq!(header.len(), HEADER_BYTES as usize);
    header
}

fn sound_units_to_encode(sample_frames: usize, profile: Atrac3Profile) -> usize {
    if profile.strategy() == EncoderStrategy::Dba {
        (sample_frames + PCM_BLOCK_FRAMES - ENCODER_DELAY) / PCM_BLOCK_FRAMES + 2
    } else {
        sample_frames / PCM_BLOCK_FRAMES + 2 + profile.priming_sound_units()
    }
}

fn write_sound_unit(sound_unit: usize, profile: Atrac3Profile) -> bool {
    sound_unit >= profile.priming_sound_units()
}

fn input_base_frame_for_sound_unit(
    sound_unit: usize,
    dba_priming: bool,
    input_delay: usize,
) -> isize {
    if dba_priming {
        sound_unit as isize * PCM_BLOCK_FRAMES as isize + input_delay as isize
    } else {
        (sound_unit as isize - 1) * PCM_BLOCK_FRAMES as isize + input_delay as isize
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    struct FailAfter {
        limit: usize,
        written: usize,
    }

    impl Write for FailAfter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.written >= self.limit {
                return Err(io::Error::other("injected failure"));
            }
            let written = usize::min(bytes.len(), self.limit - self.written);
            self.written += written;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn generated_pcm(channels: u16, frames: usize) -> Vec<Vec<i16>> {
        (0..channels)
            .map(|channel| {
                (0..frames)
                    .map(|frame| {
                        ((frame as i32 * 97 + i32::from(channel) * 811) % 60_001 - 30_000) as i16
                    })
                    .collect()
            })
            .collect()
    }

    fn legacy_buffered_encode(profile: Atrac3Profile, pcm: &[Vec<i16>]) -> Vec<u8> {
        let mut encoder = EncoderCore::new(profile);
        let dba_priming = profile.strategy() == EncoderStrategy::Dba;
        let sound_units = sound_units_to_encode(pcm[0].len(), profile);
        let internal_frame_size = profile.internal_frame_bytes();
        let silence = build_silence_frame(profile).unwrap();
        let mut out_buf = vec![0; 48 * 1024];
        let mut payload = Vec::new();
        for sound_unit in 0..sound_units {
            let base = input_base_frame_for_sound_unit(sound_unit, dba_priming, ENCODER_DELAY);
            let mut left = [0.0f32; PCM_BLOCK_FRAMES];
            let mut right = [0.0f32; PCM_BLOCK_FRAMES];
            for frame in 0..PCM_BLOCK_FRAMES {
                let index = base + frame as isize;
                if index >= 0 && (index as usize) < pcm[0].len() {
                    left[frame] = f32::from(pcm[0][index as usize]);
                    right[frame] = if profile.channels() == 1 {
                        left[frame]
                    } else {
                        f32::from(pcm[1][index as usize])
                    };
                }
            }
            let encoded = encoder.encode_frame(
                PcmFrame::new([&left, &right]),
                &mut out_buf,
                internal_frame_size,
            );
            if write_sound_unit(sound_unit, profile) {
                let fallback = encoded
                    .as_ref()
                    .map_or(true, |frame| frame.byte_count() > internal_frame_size);
                if fallback {
                    payload.extend_from_slice(&silence[..profile.frame_bytes()]);
                } else {
                    payload.extend_from_slice(&out_buf[..profile.frame_bytes()]);
                }
            }
        }
        let mut output = build_header(
            pcm[0].len() as u32,
            profile.frame_bytes() as u32,
            profile.channels(),
            profile.is_joint_stereo(),
            payload.len() as u32,
        );
        output.extend_from_slice(&payload);
        output
    }

    fn streaming_encode(profile: Atrac3Profile, pcm: &[Vec<i16>]) -> Vec<u8> {
        let mut encoder =
            Atrac3StreamEncoder::new(Vec::new(), profile, pcm[0].len() as u32).unwrap();
        let mut offset = 0;
        while offset < pcm[0].len() {
            let end = usize::min(offset + PCM_BLOCK_FRAMES, pcm[0].len());
            match profile.channels() {
                1 => encoder.push_pcm(&[&pcm[0][offset..end]]).unwrap(),
                2 => encoder
                    .push_pcm(&[&pcm[0][offset..end], &pcm[1][offset..end]])
                    .unwrap(),
                _ => unreachable!(),
            }
            offset = end;
        }
        encoder.finish().unwrap().0
    }

    #[test]
    fn schedule_matches_existing_clean_and_dba_counts() {
        let clean = Atrac3Profile::new(132, 2).unwrap();
        let dba = Atrac3Profile::new(66, 2).unwrap();
        assert_eq!(sound_units_to_encode(8192, clean), 12);
        assert_eq!(sound_units_to_encode(580_078, clean), 570);
        assert_eq!(sound_units_to_encode(8192, dba), 10);
        assert_eq!(sound_units_to_encode(580_078, dba), 569);
        assert_eq!(
            input_base_frame_for_sound_unit(0, false, ENCODER_DELAY),
            -955
        );
        assert_eq!(input_base_frame_for_sound_unit(0, true, ENCODER_DELAY), 69);
    }

    #[test]
    fn rolling_pcm_storage_stays_bounded() {
        let profile = Atrac3Profile::new(132, 2).unwrap();
        let mut encoder = Atrac3StreamEncoder::new(Vec::new(), profile, 8193).unwrap();
        let block = vec![0i16; PCM_BLOCK_FRAMES];
        for _ in 0..8 {
            encoder.push_pcm(&[&block, &block]).unwrap();
            assert!(encoder.buffered_frames() <= PCM_BLOCK_FRAMES * 2);
        }
        let tail = [0i16; 1];
        encoder.push_pcm(&[&tail, &tail]).unwrap();
        assert!(encoder.buffered_frames() <= PCM_BLOCK_FRAMES * 2);
        encoder.finish().unwrap();
    }

    #[test]
    fn progress_covers_encoding_priming_and_flushing_tail() {
        let profile = Atrac3Profile::new(132, 2).unwrap();
        let pcm = generated_pcm(2, 1024);
        let mut encoder = Atrac3StreamEncoder::new(Vec::new(), profile, 1024).unwrap();
        let mut progress = Vec::new();
        encoder
            .push_pcm_with_progress(&[&pcm[0], &pcm[1]], |update| progress.push(update))
            .unwrap();
        let (_, summary) = encoder
            .finish_with_progress(|update| progress.push(update))
            .unwrap();

        assert_eq!(progress.len(), 5);
        assert_eq!(progress[0].phase, EncodePhase::Encoding);
        assert_eq!(progress[0].completed_steps, 1);
        assert_eq!(progress[0].completed_output_frames, 0);
        assert_eq!(progress[1].completed_output_frames, 0);
        assert!(
            progress[1..]
                .iter()
                .all(|update| update.phase == EncodePhase::Flushing)
        );
        assert_eq!(
            progress.last(),
            Some(&EncodeProgress {
                phase: EncodePhase::Flushing,
                completed_steps: 5,
                total_steps: 5,
                completed_output_frames: 3,
                total_output_frames: 3,
            })
        );
        assert_eq!(summary.input_sample_frames, 1024);
        assert_eq!(summary.output_frames, 3);
        assert_eq!(
            summary.output_frames,
            summary.encoded_frames + summary.fallback_frames
        );
    }

    #[test]
    fn dba_progress_can_begin_after_the_first_pcm_chunk() {
        let profile = Atrac3Profile::new(66, 2).unwrap();
        let pcm = generated_pcm(2, 2048);
        let mut encoder = Atrac3StreamEncoder::new(Vec::new(), profile, 2048).unwrap();
        let mut progress = Vec::new();
        encoder
            .push_pcm_with_progress(&[&pcm[0][..1024], &pcm[1][..1024]], |update| {
                progress.push(update)
            })
            .unwrap();
        assert!(progress.is_empty());
        encoder
            .push_pcm_with_progress(&[&pcm[0][1024..], &pcm[1][1024..]], |update| {
                progress.push(update)
            })
            .unwrap();
        encoder
            .finish_with_progress(|update| progress.push(update))
            .unwrap();

        assert_eq!(progress.len(), 4);
        assert_eq!(progress[0].phase, EncodePhase::Encoding);
        assert_eq!(progress[0].completed_output_frames, 0);
        assert_eq!(progress.last().unwrap().completed_steps, 4);
        assert_eq!(progress.last().unwrap().total_steps, 4);
        assert_eq!(progress.last().unwrap().completed_output_frames, 3);
        assert_eq!(progress.last().unwrap().total_output_frames, 3);
    }

    #[test]
    fn streaming_matches_legacy_windows_for_clean_dba_mono_and_stereo() {
        let mut cases = Vec::new();
        for frames in [68, 69, 1024, 1025] {
            cases.push((132, 2, frames));
            cases.push((66, 2, frames));
        }
        cases.push((66, 1, 1025));
        cases.push((52, 1, 1025));

        for (bitrate_kbps, channels, frames) in cases {
            let profile = Atrac3Profile::new(bitrate_kbps, channels).unwrap();
            let pcm = generated_pcm(channels, frames);
            assert_eq!(
                streaming_encode(profile, &pcm),
                legacy_buffered_encode(profile, &pcm),
                "{bitrate_kbps} kbps, {channels} channel(s), {frames} frames"
            );
        }
    }

    #[test]
    fn payload_writer_failures_are_typed() {
        let writer = FailAfter {
            limit: HEADER_BYTES as usize,
            written: 0,
        };
        let profile = Atrac3Profile::new(132, 2).unwrap();
        let mut encoder = Atrac3StreamEncoder::new(writer, profile, 1024).unwrap();
        let pcm = generated_pcm(2, 1024);
        encoder.push_pcm(&[&pcm[0], &pcm[1]]).unwrap();
        assert!(matches!(
            encoder.finish(),
            Err(Atrac3StreamError::Io {
                stage: WriteStage::OutputFrame { .. },
                ..
            })
        ));
    }
}
