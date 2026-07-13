use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Write};
use std::path::Path;

use crate::dsp::encode::{Atrac3Encoder, EncodeFitterDiagnostics, ProductionTraceFrameContext};

pub const PCM_BLOCK_FRAMES: usize = 1024;
const SAMPLE_RATE: u32 = 44_100;
const ENCODER_DELAY: usize = 69;
const CLEAN_PRIMING_SOUND_UNITS: usize = 2;
const DBA_PRIMING_SOUND_UNITS: usize = 1;
const HEADER_BYTES: u64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atrac3StreamConfig {
    pub bitrate_kbps: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atrac3WriteStage {
    Header,
    Payload,
    Trace,
}

#[derive(Debug)]
pub enum Atrac3StreamError {
    UnsupportedConfig {
        bitrate_kbps: u32,
        channels: u16,
    },
    EmptyInput,
    RiffSizeOverflow {
        payload_bytes: u64,
    },
    SilenceFrame,
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
        stage: Atrac3WriteStage,
        source: io::Error,
    },
}

impl fmt::Display for Atrac3StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedConfig {
                bitrate_kbps,
                channels,
            } => write!(
                f,
                "unsupported ATRAC3 bitrate/channel combination: {bitrate_kbps} kbps, {channels} channel(s); supported mono rates are 52 and 66 kbps, supported stereo rates are 66, 105, and 132 kbps"
            ),
            Self::EmptyInput => write!(f, "not enough samples: need at least one sample"),
            Self::RiffSizeOverflow { payload_bytes } => write!(
                f,
                "ATRAC3 output is too large for a RIFF/WAVE container ({payload_bytes} payload bytes)"
            ),
            Self::SilenceFrame => write!(f, "failed to build fallback silence frame"),
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
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atrac3StreamSummary {
    pub encoded_frames: u32,
    pub fallback_frames: u32,
    pub payload_bytes: u64,
    pub file_bytes: u64,
    pub diagnostics: EncodeFitterDiagnostics,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedConfig {
    frame_size: usize,
    encoder_bitrate: u32,
    joint_stereo: bool,
    output_channels: u16,
}

impl ResolvedConfig {
    fn resolve(config: Atrac3StreamConfig) -> Result<Self, Atrac3StreamError> {
        match (config.channels, config.bitrate_kbps) {
            (2, 132) => Ok(Self {
                frame_size: 384,
                encoder_bitrate: 132,
                joint_stereo: false,
                output_channels: 2,
            }),
            (2, 105) => Ok(Self {
                frame_size: 304,
                encoder_bitrate: 105,
                joint_stereo: false,
                output_channels: 2,
            }),
            (2, 66) => Ok(Self {
                frame_size: 192,
                encoder_bitrate: 66,
                joint_stereo: true,
                output_channels: 2,
            }),
            (1, 66) => Ok(Self {
                frame_size: 192,
                encoder_bitrate: 132,
                joint_stereo: false,
                output_channels: 1,
            }),
            (1, 52) => Ok(Self {
                frame_size: 152,
                encoder_bitrate: 105,
                joint_stereo: false,
                output_channels: 1,
            }),
            _ => Err(Atrac3StreamError::UnsupportedConfig {
                bitrate_kbps: config.bitrate_kbps,
                channels: config.channels,
            }),
        }
    }
}

pub struct Atrac3StreamEncoder<W: Write> {
    writer: W,
    encoder: Atrac3Encoder,
    config: Atrac3StreamConfig,
    resolved: ResolvedConfig,
    sample_frames: u32,
    dba_priming: bool,
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
}

impl<W: Write> Atrac3StreamEncoder<W> {
    pub fn new(
        mut writer: W,
        config: Atrac3StreamConfig,
        sample_frames: u32,
    ) -> Result<Self, Atrac3StreamError> {
        if sample_frames == 0 {
            return Err(Atrac3StreamError::EmptyInput);
        }
        let resolved = ResolvedConfig::resolve(config)?;
        let encoder = Atrac3Encoder::new(resolved.encoder_bitrate, resolved.joint_stereo);
        let dba_priming = encoder.enc_algo() == 0;
        let sound_units = sound_units_to_encode(sample_frames as usize, dba_priming);
        let payload_frames = sound_units - priming_sound_units(dba_priming);
        let expected_payload_bytes = (payload_frames as u64)
            .checked_mul(resolved.frame_size as u64)
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

        let internal_frame_size = match resolved.encoder_bitrate {
            132 => 384,
            105 => 304,
            66 => 192,
            _ => unreachable!("resolved ATRAC3 encoder bitrate"),
        };
        let silence_frame = build_silence_frame(
            resolved.encoder_bitrate,
            resolved.joint_stereo,
            internal_frame_size,
        )?;
        let header = build_header(
            sample_frames,
            resolved.frame_size as u32,
            resolved.output_channels,
            resolved.joint_stereo,
            expected_payload_bytes as u32,
        );
        writer
            .write_all(&header)
            .map_err(|source| Atrac3StreamError::Io {
                stage: Atrac3WriteStage::Header,
                source,
            })?;

        Ok(Self {
            writer,
            encoder,
            config,
            resolved,
            sample_frames,
            dba_priming,
            sound_units,
            next_sound_unit: 0,
            received_frames: 0,
            buffer_start: 0,
            pcm: (0..config.channels)
                .map(|_| VecDeque::with_capacity(PCM_BLOCK_FRAMES * 2))
                .collect(),
            out_buf: vec![0; 48 * 1024],
            silence_frame,
            expected_payload_bytes,
            written_payload_bytes: 0,
            encoded_frames: 0,
            fallback_frames: 0,
        })
    }

    pub fn enable_production_trace<P: AsRef<Path>>(&mut self, out_dir: P) -> io::Result<()> {
        self.encoder.enable_production_trace(out_dir)
    }

    pub fn enable_production_trace_with_max_frames<P: AsRef<Path>>(
        &mut self,
        out_dir: P,
        max_sound_frames: Option<u32>,
    ) -> io::Result<()> {
        self.encoder
            .enable_production_trace_with_max_frames(out_dir, max_sound_frames)
    }

    pub fn push_pcm(&mut self, channels: &[&[i16]]) -> Result<(), Atrac3StreamError> {
        if channels.len() != self.config.channels as usize {
            return Err(Atrac3StreamError::WrongChannelCount {
                expected: self.config.channels as usize,
                actual: channels.len(),
            });
        }
        if self.received_frames == self.sample_frames {
            return Err(Atrac3StreamError::InputAlreadyComplete);
        }
        let expected = usize::min(
            PCM_BLOCK_FRAMES,
            (self.sample_frames - self.received_frames) as usize,
        );
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
        self.process_ready(false)
    }

    pub fn finish(mut self) -> Result<(W, Atrac3StreamSummary), Atrac3StreamError> {
        if self.received_frames != self.sample_frames {
            return Err(Atrac3StreamError::IncompleteInput {
                expected: self.sample_frames,
                actual: self.received_frames,
            });
        }
        self.process_ready(true)?;
        self.encoder
            .finish_production_trace()
            .map_err(|source| Atrac3StreamError::Io {
                stage: Atrac3WriteStage::Trace,
                source,
            })?;
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
            encoded_frames: self.encoded_frames,
            fallback_frames: self.fallback_frames,
            payload_bytes: self.written_payload_bytes,
            file_bytes: HEADER_BYTES + self.written_payload_bytes,
            diagnostics: self.encoder.diagnostics(),
        };
        Ok((self.writer, summary))
    }

    fn process_ready(&mut self, finalizing: bool) -> Result<(), Atrac3StreamError> {
        while self.next_sound_unit < self.sound_units {
            let base = input_base_frame_for_sound_unit(
                self.next_sound_unit,
                self.dba_priming,
                ENCODER_DELAY,
            );
            if !finalizing && base + PCM_BLOCK_FRAMES as isize > self.received_frames as isize {
                break;
            }
            self.encode_sound_unit(base)?;
            self.next_sound_unit += 1;
            self.discard_consumed_pcm();
        }
        Ok(())
    }

    fn encode_sound_unit(&mut self, input_base_frame: isize) -> Result<(), Atrac3StreamError> {
        let sound_unit = self.next_sound_unit;
        let write_frame = write_sound_unit(sound_unit, self.dba_priming);
        let requested_channels = self.encoder.channel_count().max(1);
        let mut trace_input_pcm =
            Vec::with_capacity(PCM_BLOCK_FRAMES * 2 * usize::from(requested_channels));
        let mut pcm0 = [0.0f32; PCM_BLOCK_FRAMES];
        let mut pcm1 = [0.0f32; PCM_BLOCK_FRAMES];
        for index in 0..PCM_BLOCK_FRAMES {
            let source_index = input_base_frame + index as isize;
            let left = self.sample_at(0, source_index);
            let right = if self.config.channels == 1 {
                left
            } else {
                self.sample_at(1, source_index)
            };
            pcm0[index] = f32::from(left);
            pcm1[index] = f32::from(right);
            trace_input_pcm.extend_from_slice(&left.to_le_bytes());
            if requested_channels > 1 {
                trace_input_pcm.extend_from_slice(&right.to_le_bytes());
            }
        }
        let pcm_refs: [&[f32; PCM_BLOCK_FRAMES]; 2] = [&pcm0, &pcm1];
        let input_byte_count = trace_input_pcm.len() as u32;
        let scheduled_start = (sound_unit * PCM_BLOCK_FRAMES) as u64;
        let actual_start = input_base_frame.max(0) as u64;
        self.encoder
            .begin_production_trace_frame_with_pcm(
                ProductionTraceFrameContext {
                    sound_frame_call_idx: sound_unit as u32,
                    frame_index: sound_unit as u32 + 1,
                    frame_sequence_arg: sound_unit as i32,
                    requested_channels,
                    input_byte_count_arg: input_byte_count / u32::from(requested_channels),
                    input_byte_count,
                    input_sample_frame_count: PCM_BLOCK_FRAMES as u32,
                    scheduled_input_sample_frame_start: scheduled_start,
                    scheduled_input_sample_frame_end: scheduled_start + PCM_BLOCK_FRAMES as u64,
                    actual_input_sample_frame_start: actual_start,
                    actual_input_sample_frame_end: actual_start + PCM_BLOCK_FRAMES as u64,
                    priming_frame: !write_frame,
                    write_frame,
                    payload_offset: write_frame.then_some(self.written_payload_bytes),
                },
                &trace_input_pcm,
            )
            .map_err(|source| Atrac3StreamError::Io {
                stage: Atrac3WriteStage::Trace,
                source,
            })?;

        let bit_count = self.encoder.encode_frame(&pcm_refs, &mut self.out_buf);
        let byte_count = if bit_count < 0 {
            None
        } else {
            Some(((bit_count as u32 + 7) >> 3) as usize)
        };
        if !write_frame {
            return Ok(());
        }

        let bytes = if byte_count.is_none_or(|count| count > self.silence_frame.len()) {
            self.fallback_frames += 1;
            &self.silence_frame[..self.resolved.frame_size]
        } else {
            self.encoded_frames += 1;
            &self.out_buf[..self.resolved.frame_size]
        };
        self.writer
            .write_all(bytes)
            .map_err(|source| Atrac3StreamError::Io {
                stage: Atrac3WriteStage::Payload,
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
            input_base_frame_for_sound_unit(self.next_sound_unit, self.dba_priming, ENCODER_DELAY)
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

fn build_silence_frame(
    encoder_bitrate: u32,
    joint_stereo: bool,
    frame_size: usize,
) -> Result<Vec<u8>, Atrac3StreamError> {
    let mut encoder = Atrac3Encoder::new(encoder_bitrate, joint_stereo);
    let silence0 = [0.0f32; PCM_BLOCK_FRAMES];
    let silence1 = [0.0f32; PCM_BLOCK_FRAMES];
    let refs = [&silence0, &silence1];
    let mut frame = vec![0; frame_size];
    let bit_count = encoder.encode_frame(&refs, &mut frame);
    let byte_count = ((bit_count.max(0) as u32 + 7) >> 3) as usize;
    if bit_count < 0 || byte_count > frame_size {
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
    push_u32(&mut header, SAMPLE_RATE);
    let byte_rate =
        (u64::from(frame_size) * u64::from(SAMPLE_RATE)).div_ceil(PCM_BLOCK_FRAMES as u64) as u32;
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
    push_u32(&mut header, PCM_BLOCK_FRAMES as u32);
    push_u32(&mut header, PCM_BLOCK_FRAMES as u32);
    header.extend_from_slice(b"data");
    push_u32(&mut header, total_data_size);
    debug_assert_eq!(header.len(), HEADER_BYTES as usize);
    header
}

fn sound_units_to_encode(sample_frames: usize, dba_priming: bool) -> usize {
    if dba_priming {
        (sample_frames + PCM_BLOCK_FRAMES - ENCODER_DELAY) / PCM_BLOCK_FRAMES + 2
    } else {
        sample_frames / PCM_BLOCK_FRAMES + 2 + CLEAN_PRIMING_SOUND_UNITS
    }
}

fn priming_sound_units(dba_priming: bool) -> usize {
    if dba_priming {
        DBA_PRIMING_SOUND_UNITS
    } else {
        CLEAN_PRIMING_SOUND_UNITS
    }
}

fn write_sound_unit(sound_unit: usize, dba_priming: bool) -> bool {
    sound_unit >= priming_sound_units(dba_priming)
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

    fn legacy_buffered_encode(config: Atrac3StreamConfig, pcm: &[Vec<i16>]) -> Vec<u8> {
        let resolved = ResolvedConfig::resolve(config).unwrap();
        let mut encoder = Atrac3Encoder::new(resolved.encoder_bitrate, resolved.joint_stereo);
        let dba_priming = encoder.enc_algo() == 0;
        let sound_units = sound_units_to_encode(pcm[0].len(), dba_priming);
        let internal_frame_size = match resolved.encoder_bitrate {
            132 => 384,
            105 => 304,
            66 => 192,
            _ => unreachable!(),
        };
        let silence = build_silence_frame(
            resolved.encoder_bitrate,
            resolved.joint_stereo,
            internal_frame_size,
        )
        .unwrap();
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
                    right[frame] = if config.channels == 1 {
                        left[frame]
                    } else {
                        f32::from(pcm[1][index as usize])
                    };
                }
            }
            let bit_count = encoder.encode_frame(&[&left, &right], &mut out_buf);
            if write_sound_unit(sound_unit, dba_priming) {
                let byte_count = ((bit_count.max(0) as u32 + 7) >> 3) as usize;
                if bit_count < 0 || byte_count > internal_frame_size {
                    payload.extend_from_slice(&silence[..resolved.frame_size]);
                } else {
                    payload.extend_from_slice(&out_buf[..resolved.frame_size]);
                }
            }
        }
        let mut output = build_header(
            pcm[0].len() as u32,
            resolved.frame_size as u32,
            resolved.output_channels,
            resolved.joint_stereo,
            payload.len() as u32,
        );
        output.extend_from_slice(&payload);
        output
    }

    fn streaming_encode(config: Atrac3StreamConfig, pcm: &[Vec<i16>]) -> Vec<u8> {
        let mut encoder =
            Atrac3StreamEncoder::new(Vec::new(), config, pcm[0].len() as u32).unwrap();
        let mut offset = 0;
        while offset < pcm[0].len() {
            let end = usize::min(offset + PCM_BLOCK_FRAMES, pcm[0].len());
            match config.channels {
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
        assert_eq!(sound_units_to_encode(8192, false), 12);
        assert_eq!(sound_units_to_encode(580_078, false), 570);
        assert_eq!(sound_units_to_encode(8192, true), 10);
        assert_eq!(sound_units_to_encode(580_078, true), 569);
        assert_eq!(
            input_base_frame_for_sound_unit(0, false, ENCODER_DELAY),
            -955
        );
        assert_eq!(input_base_frame_for_sound_unit(0, true, ENCODER_DELAY), 69);
    }

    #[test]
    fn rolling_pcm_storage_stays_bounded() {
        let config = Atrac3StreamConfig {
            bitrate_kbps: 132,
            channels: 2,
        };
        let mut encoder = Atrac3StreamEncoder::new(Vec::new(), config, 8193).unwrap();
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
    fn streaming_matches_legacy_windows_for_clean_dba_mono_and_stereo() {
        let mut cases = Vec::new();
        for frames in [68, 69, 1024, 1025] {
            cases.push((132, 2, frames));
            cases.push((66, 2, frames));
        }
        cases.push((66, 1, 1025));
        cases.push((52, 1, 1025));

        for (bitrate_kbps, channels, frames) in cases {
            let config = Atrac3StreamConfig {
                bitrate_kbps,
                channels,
            };
            let pcm = generated_pcm(channels, frames);
            assert_eq!(
                streaming_encode(config, &pcm),
                legacy_buffered_encode(config, &pcm),
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
        let config = Atrac3StreamConfig {
            bitrate_kbps: 132,
            channels: 2,
        };
        let mut encoder = Atrac3StreamEncoder::new(writer, config, 1024).unwrap();
        let pcm = generated_pcm(2, 1024);
        encoder.push_pcm(&[&pcm[0], &pcm[1]]).unwrap();
        assert!(matches!(
            encoder.finish(),
            Err(Atrac3StreamError::Io {
                stage: Atrac3WriteStage::Payload,
                ..
            })
        ));
    }
}
