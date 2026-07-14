use std::fmt;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

use at3p::container::PcmWaveInfo;
use hound::{SampleFormat, WavIntoSamples, WavReader, WavSpec};

pub const MAX_PCM_BLOCK_FRAMES: usize = 2048;

#[derive(Debug, Clone, Copy)]
pub struct PcmWaveMetadata {
    pub spec: WavSpec,
    pub sample_frames: u32,
}

#[derive(Debug)]
pub enum PcmStreamError {
    Open(io::Error),
    Decode(hound::Error),
    InvalidBufferCount { expected: usize, actual: usize },
    InvalidBlockFrames(usize),
    PrematureEof { frame: u32, channel: u16 },
    ExcessSamples { declared_frames: u32 },
    MetadataMismatch(String),
}

impl fmt::Display for PcmStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(error) => write!(formatter, "failed to open WAV: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode WAV sample: {error}"),
            Self::InvalidBufferCount { expected, actual } => write!(
                formatter,
                "PCM reader expected {expected} channel buffer(s), got {actual}"
            ),
            Self::InvalidBlockFrames(frames) => write!(
                formatter,
                "PCM block size must be between 1 and {MAX_PCM_BLOCK_FRAMES}, got {frames}"
            ),
            Self::PrematureEof { frame, channel } => write!(
                formatter,
                "WAV sample data ended early at frame {frame}, channel {channel}"
            ),
            Self::ExcessSamples { declared_frames } => write!(
                formatter,
                "WAV contains samples beyond its declared {declared_frames} frame(s)"
            ),
            Self::MetadataMismatch(message) => {
                write!(formatter, "WAV parser metadata mismatch: {message}")
            }
        }
    }
}

impl std::error::Error for PcmStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open(error) => Some(error),
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

pub struct PcmWaveStream {
    metadata: PcmWaveMetadata,
    samples: WavIntoSamples<BufReader<File>, i16>,
    frames_read: u32,
    exhaustion_checked: bool,
}

impl PcmWaveStream {
    pub fn open(path: &Path) -> Result<Self, PcmStreamError> {
        let file = File::open(path).map_err(PcmStreamError::Open)?;
        Self::from_file(file)
    }

    pub fn from_file(file: File) -> Result<Self, PcmStreamError> {
        let reader = WavReader::new(BufReader::new(file)).map_err(PcmStreamError::Decode)?;
        let metadata = PcmWaveMetadata {
            spec: reader.spec(),
            sample_frames: reader.duration(),
        };
        Ok(Self {
            metadata,
            samples: reader.into_samples::<i16>(),
            frames_read: 0,
            exhaustion_checked: false,
        })
    }

    pub fn metadata(&self) -> PcmWaveMetadata {
        self.metadata
    }

    pub fn validate_strict_info(&self, info: &PcmWaveInfo) -> Result<(), PcmStreamError> {
        let spec = self.metadata.spec;
        if spec.sample_format != SampleFormat::Int {
            return Err(PcmStreamError::MetadataMismatch(
                "strict PCM header was decoded as floating point".to_owned(),
            ));
        }
        if spec.channels != info.format.channels
            || spec.sample_rate != info.format.sample_rate
            || spec.bits_per_sample != info.format.bits_per_sample
            || self.metadata.sample_frames != info.sample_frames
        {
            return Err(PcmStreamError::MetadataMismatch(format!(
                "strict parser reports {} channel(s), {} Hz, {} bits, {} frames; hound reports {} channel(s), {} Hz, {} bits, {} frames",
                info.format.channels,
                info.format.sample_rate,
                info.format.bits_per_sample,
                info.sample_frames,
                spec.channels,
                spec.sample_rate,
                spec.bits_per_sample,
                self.metadata.sample_frames,
            )));
        }
        Ok(())
    }

    pub fn read_block(
        &mut self,
        channels: &mut [Vec<i16>],
        max_frames: usize,
    ) -> Result<usize, PcmStreamError> {
        let channel_count = self.metadata.spec.channels as usize;
        if channels.len() != channel_count {
            return Err(PcmStreamError::InvalidBufferCount {
                expected: channel_count,
                actual: channels.len(),
            });
        }
        if max_frames == 0 || max_frames > MAX_PCM_BLOCK_FRAMES {
            return Err(PcmStreamError::InvalidBlockFrames(max_frames));
        }
        for channel in channels.iter_mut() {
            channel.clear();
            if channel.capacity() < max_frames {
                channel.reserve(max_frames - channel.capacity());
            }
        }

        let remaining = (self.metadata.sample_frames - self.frames_read) as usize;
        let frames = usize::min(max_frames, remaining);
        for frame_offset in 0..frames {
            for (channel_index, channel) in channels.iter_mut().enumerate() {
                match self.samples.next() {
                    Some(Ok(sample)) => channel.push(sample),
                    Some(Err(error)) => return Err(PcmStreamError::Decode(error)),
                    None => {
                        return Err(PcmStreamError::PrematureEof {
                            frame: self.frames_read + frame_offset as u32,
                            channel: channel_index as u16,
                        });
                    }
                }
            }
        }
        self.frames_read += frames as u32;
        if self.frames_read == self.metadata.sample_frames {
            self.ensure_exhausted()?;
        }
        Ok(frames)
    }

    pub fn ensure_exhausted(&mut self) -> Result<(), PcmStreamError> {
        if self.exhaustion_checked {
            return Ok(());
        }
        self.exhaustion_checked = true;
        match self.samples.next() {
            None => Ok(()),
            Some(Ok(_)) => Err(PcmStreamError::ExcessSamples {
                declared_frames: self.metadata.sample_frames,
            }),
            Some(Err(error)) => Err(PcmStreamError::Decode(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use hound::{WavSpec, WavWriter};

    use super::*;

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn temp_wave_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "atrac-{label}-{}-{}.wav",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn streams_and_deinterleaves_across_block_boundaries() {
        let path = temp_wave_path("pcm-stream");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        for frame in 0..2051i16 {
            writer.write_sample(frame).unwrap();
            writer.write_sample(-frame).unwrap();
        }
        writer.finalize().unwrap();

        let mut reader = PcmWaveStream::open(&path).unwrap();
        assert_eq!(reader.metadata().sample_frames, 2051);
        let mut blocks = vec![Vec::with_capacity(1024), Vec::with_capacity(1024)];
        let mut left = Vec::new();
        let mut right = Vec::new();
        loop {
            let frames = reader.read_block(&mut blocks, 1024).unwrap();
            if frames == 0 {
                break;
            }
            assert!(blocks.iter().all(|channel| channel.capacity() <= 1024));
            left.extend_from_slice(&blocks[0]);
            right.extend_from_slice(&blocks[1]);
        }
        assert_eq!(left.len(), 2051);
        for (frame, (&left, &right)) in left.iter().zip(&right).enumerate() {
            assert_eq!(left, frame as i16);
            assert_eq!(right, -(frame as i16));
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_a_partial_interleaved_frame() {
        let path = temp_wave_path("partial-frame");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&42u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&44_100u32.to_le_bytes());
        bytes.extend_from_slice(&176_400u32.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&6u32.to_le_bytes());
        for sample in [1i16, 2, 3] {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(&path, bytes).unwrap();

        assert!(matches!(
            PcmWaveStream::open(&path),
            Err(PcmStreamError::Decode(_))
        ));
        fs::remove_file(path).unwrap();
    }
}
