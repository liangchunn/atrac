use std::io::Write;

use super::coding_params::CodingParams;
use super::flush::{ComputedSchedule352, IncrementalComputedFlushScheduler};
use super::frontend::FRONTEND_FRAME_SAMPLES;
use super::payload::{
    ComputedFileError, ComputedPayloadError, ComputedWriteError, ComputedWriteStage, EncodePhase,
    EncodeProgress, write_computed_output_frame,
};
use super::profile::Atrac3plusProfile;
use crate::riff::write::{
    ATRACX_HEADER_LEN, write_atracx_header, write_atracx_header_for_rate,
    write_atracx_header_for_rate_channels,
};

pub const PCM_BLOCK_FRAMES: usize = FRONTEND_FRAME_SAMPLES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atrac3plusStreamSummary {
    pub input_sample_frames: u32,
    pub output_frames: u32,
    pub payload_bytes: usize,
    pub file_bytes: usize,
}

/// Incremental PCM-to-ATRAC3plus file writer. The caller owns WAV decoding and
/// supplies one native wrapper block at a time; this type owns only one
/// converted PCM frame, codec state, and at most one encoded frame.
pub struct Atrac3plusStreamEncoder<W: Write> {
    writer: W,
    schedule: ComputedSchedule352,
    scheduler: IncrementalComputedFlushScheduler,
    channel_count: usize,
    frame_bytes: usize,
    total_steps: u32,
    input_sample_frames: u32,
    consumed_sample_frames: u32,
    next_output_frame_index: u32,
    written_payload_bytes: usize,
    header_len: usize,
    pcm_frame: Vec<Vec<f32>>,
}

impl<W: Write> Atrac3plusStreamEncoder<W> {
    pub fn new(
        mut writer: W,
        profile: &Atrac3plusProfile,
        input_sample_frames: u32,
    ) -> Result<Self, ComputedWriteError> {
        let schedule = ComputedSchedule352::new(input_sample_frames)
            .map_err(ComputedFileError::from)
            .map_err(ComputedWriteError::from)?;
        let header = match profile.channels() {
            2 if profile.bitrate_kbps() == 352 => {
                write_atracx_header(input_sample_frames, schedule.total_output_frames())
            }
            2 => write_atracx_header_for_rate(
                input_sample_frames,
                schedule.total_output_frames(),
                profile.frame_bytes() as u16,
                profile.codec_info(),
            ),
            1 => write_atracx_header_for_rate_channels(
                1,
                input_sample_frames,
                schedule.total_output_frames(),
                profile.frame_bytes() as u16,
                profile.codec_info(),
            ),
            _ => unreachable!("validated ATRAC3plus channel count"),
        }
        .map_err(ComputedFileError::from)?;
        writer
            .write_all(&header)
            .map_err(|source| ComputedWriteError::Io {
                stage: ComputedWriteStage::Header,
                source,
            })?;

        let params = CodingParams::for_profile(profile);
        let scheduler = IncrementalComputedFlushScheduler::new(input_sample_frames, params)
            .map_err(ComputedPayloadError::from)
            .map_err(ComputedFileError::from)?;
        let total_steps = schedule.encode_calls() + schedule.flush_wrapper_calls();
        Ok(Self {
            writer,
            schedule,
            scheduler,
            channel_count: profile.channels() as usize,
            frame_bytes: profile.frame_bytes() as usize,
            total_steps,
            input_sample_frames,
            consumed_sample_frames: 0,
            next_output_frame_index: 0,
            written_payload_bytes: 0,
            header_len: header.len(),
            pcm_frame: vec![vec![0.0; FRONTEND_FRAME_SAMPLES]; profile.channels() as usize],
        })
    }

    pub fn expected_next_chunk_frames(&self) -> Option<usize> {
        let call = self.scheduler.encode_calls();
        (call < self.schedule.encode_calls())
            .then(|| self.schedule.expected_encode_sample_frames(call) as usize)
    }

    pub fn push_pcm(&mut self, channels: &[&[i16]]) -> Result<EncodeProgress, ComputedWriteError> {
        let core_call_index = self.scheduler.encode_calls();
        let Some(expected) = self.expected_next_chunk_frames() else {
            return Err(ComputedFileError::StreamInputAlreadyComplete.into());
        };
        if channels.len() != self.channel_count {
            return Err(ComputedFileError::UnexpectedInputChannelCount {
                expected: self.channel_count,
                actual: channels.len(),
            }
            .into());
        }
        let actual = channels.first().map_or(0, |channel| channel.len());
        if actual != expected {
            return Err(ComputedFileError::UnexpectedInputChunkFrames {
                core_call_index,
                expected,
                actual,
            }
            .into());
        }
        for (channel_index, channel) in channels.iter().enumerate().skip(1) {
            if channel.len() != expected {
                return Err(ComputedFileError::MismatchedInputChunkFrames {
                    core_call_index,
                    channel: channel_index,
                    expected,
                    actual: channel.len(),
                }
                .into());
            }
        }

        for destination in &mut self.pcm_frame {
            destination.fill(0.0);
        }
        for (destination, source) in self.pcm_frame.iter_mut().zip(channels) {
            for (destination, &source) in destination.iter_mut().zip(source.iter()) {
                *destination = f32::from(source);
            }
        }
        let result = self
            .scheduler
            .encode_chunk(expected as u32, &self.pcm_frame)
            .map_err(ComputedPayloadError::from)
            .map_err(ComputedFileError::from)?;
        write_computed_output_frame(
            &mut self.writer,
            &mut self.next_output_frame_index,
            &mut self.written_payload_bytes,
            result,
            self.frame_bytes,
        )?;
        self.consumed_sample_frames += expected as u32;
        Ok(EncodeProgress {
            phase: EncodePhase::Encoding,
            completed_steps: core_call_index + 1,
            total_steps: self.total_steps,
            completed_output_frames: self.next_output_frame_index,
            total_output_frames: self.schedule.total_output_frames(),
        })
    }

    pub fn push_pcm_with_progress<F>(
        &mut self,
        channels: &[&[i16]],
        mut on_progress: F,
    ) -> Result<(), ComputedWriteError>
    where
        F: FnMut(EncodeProgress),
    {
        on_progress(self.push_pcm(channels)?);
        Ok(())
    }

    pub fn finish(self) -> Result<(W, Atrac3plusStreamSummary), ComputedWriteError> {
        self.finish_with_progress(|_| {})
    }

    pub fn finish_with_progress<F>(
        mut self,
        mut on_progress: F,
    ) -> Result<(W, Atrac3plusStreamSummary), ComputedWriteError>
    where
        F: FnMut(EncodeProgress),
    {
        if self.consumed_sample_frames != self.input_sample_frames {
            return Err(ComputedFileError::IncompleteStreamInput {
                expected_sample_frames: self.input_sample_frames,
                actual_sample_frames: self.consumed_sample_frames,
            }
            .into());
        }
        for flush_call_index in 0..self.schedule.flush_wrapper_calls() {
            let result = self
                .scheduler
                .flush()
                .map_err(ComputedPayloadError::from)
                .map_err(ComputedFileError::from)?;
            write_computed_output_frame(
                &mut self.writer,
                &mut self.next_output_frame_index,
                &mut self.written_payload_bytes,
                result,
                self.frame_bytes,
            )?;
            on_progress(EncodeProgress {
                phase: EncodePhase::Flushing,
                completed_steps: self.schedule.encode_calls() + flush_call_index + 1,
                total_steps: self.total_steps,
                completed_output_frames: self.next_output_frame_index,
                total_output_frames: self.schedule.total_output_frames(),
            });
        }

        let expected_output_frames = self.schedule.total_output_frames() as usize;
        if self.next_output_frame_index as usize != expected_output_frames {
            return Err(
                ComputedFileError::Payload(ComputedPayloadError::IncompleteOutputFrames {
                    expected: expected_output_frames,
                    actual: self.next_output_frame_index as usize,
                })
                .into(),
            );
        }
        if !self.scheduler.is_done() {
            return Err(
                ComputedFileError::Payload(ComputedPayloadError::SchedulerNotDone {
                    flush_calls: self.scheduler.flush_calls(),
                })
                .into(),
            );
        }
        let expected_payload_bytes = expected_output_frames * self.frame_bytes;
        if self.written_payload_bytes != expected_payload_bytes {
            return Err(
                ComputedFileError::Payload(ComputedPayloadError::FinalPayloadLength {
                    expected: expected_payload_bytes,
                    actual: self.written_payload_bytes,
                })
                .into(),
            );
        }
        let expected_file_bytes = ATRACX_HEADER_LEN as usize + expected_payload_bytes;
        let actual_file_bytes = self.header_len + self.written_payload_bytes;
        if actual_file_bytes != expected_file_bytes {
            return Err(ComputedFileError::FinalFileLength {
                expected: expected_file_bytes,
                actual: actual_file_bytes,
            }
            .into());
        }

        let summary = Atrac3plusStreamSummary {
            input_sample_frames: self.input_sample_frames,
            output_frames: self.next_output_frame_index,
            payload_bytes: self.written_payload_bytes,
            file_bytes: actual_file_bytes,
        };
        Ok((self.writer, summary))
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::encoder::payload::{
        assemble_computed_atracx_file_for_mono_profile, assemble_computed_atracx_file_for_profile,
    };
    use crate::encoder::profile::{ATRAC3PLUS_352, profile_by_bitrate_and_channels};

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

    fn generated_pcm(channels: usize, frames: usize) -> Vec<Vec<i16>> {
        (0..channels)
            .map(|channel| {
                (0..frames)
                    .map(|frame| {
                        ((frame as i32 * 43 + channel as i32 * 997) % 50_003 - 25_001) as i16
                    })
                    .collect()
            })
            .collect()
    }

    fn stream(profile: &Atrac3plusProfile, pcm: &[Vec<i16>]) -> Vec<u8> {
        let mut encoder =
            Atrac3plusStreamEncoder::new(Vec::new(), profile, pcm[0].len() as u32).unwrap();
        let mut offset = 0;
        while let Some(frames) = encoder.expected_next_chunk_frames() {
            match profile.channels() {
                1 => encoder
                    .push_pcm(&[&pcm[0][offset..offset + frames]])
                    .unwrap(),
                2 => encoder
                    .push_pcm(&[
                        &pcm[0][offset..offset + frames],
                        &pcm[1][offset..offset + frames],
                    ])
                    .unwrap(),
                _ => unreachable!(),
            };
            offset += frames;
        }
        encoder.finish().unwrap().0
    }

    #[test]
    fn streaming_matches_buffered_stereo_with_partial_final_block() {
        let pcm = generated_pcm(2, 6145);
        let expected =
            assemble_computed_atracx_file_for_profile(&ATRAC3PLUS_352, 6145, &pcm[0], &pcm[1])
                .unwrap();
        assert_eq!(stream(&ATRAC3PLUS_352, &pcm), expected);
    }

    #[test]
    fn streaming_matches_buffered_mono_with_exact_final_block() {
        let profile = profile_by_bitrate_and_channels(128, 1).unwrap();
        let pcm = generated_pcm(1, 6144);
        let expected =
            assemble_computed_atracx_file_for_mono_profile(&profile, 6144, &pcm).unwrap();
        assert_eq!(stream(&profile, &pcm), expected);
    }

    #[test]
    fn finish_rejects_incomplete_pcm() {
        let pcm = generated_pcm(2, PCM_BLOCK_FRAMES);
        let mut encoder = Atrac3plusStreamEncoder::new(Vec::new(), &ATRAC3PLUS_352, 6144).unwrap();
        encoder.push_pcm(&[&pcm[0], &pcm[1]]).unwrap();
        assert!(matches!(
            encoder.finish(),
            Err(ComputedWriteError::File(
                ComputedFileError::IncompleteStreamInput {
                    expected_sample_frames: 6144,
                    actual_sample_frames: 2048,
                }
            ))
        ));
    }

    #[test]
    fn output_frame_writer_failures_are_typed() {
        let writer = FailAfter {
            limit: ATRACX_HEADER_LEN as usize,
            written: 0,
        };
        let pcm = generated_pcm(2, 6144);
        let mut encoder = Atrac3plusStreamEncoder::new(writer, &ATRAC3PLUS_352, 6144).unwrap();
        for offset in (0..6144).step_by(PCM_BLOCK_FRAMES) {
            encoder
                .push_pcm(&[
                    &pcm[0][offset..offset + PCM_BLOCK_FRAMES],
                    &pcm[1][offset..offset + PCM_BLOCK_FRAMES],
                ])
                .unwrap();
        }
        assert!(matches!(
            encoder.finish(),
            Err(ComputedWriteError::Io {
                stage: ComputedWriteStage::OutputFrame { .. },
                ..
            })
        ));
    }
}
