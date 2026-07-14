//! Private ATRAC3 frame cores and bitstream emitters.

use crate::config::{Atrac3Profile, EncoderStrategy};

pub(crate) mod bitstream;
pub(crate) mod clean;
pub(crate) mod coding;
pub(crate) mod dba;
pub(crate) mod dba_bitstream;

pub(crate) const FRAME_SAMPLES: usize = 1024;

/// One internal ATRAC3 sound unit. Mono profiles deliberately duplicate their
/// input into both internal channels to preserve the recovered encoder law.
#[derive(Clone, Copy)]
pub(crate) struct PcmFrame<'a> {
    channels: [&'a [f32; FRAME_SAMPLES]; 2],
}

impl<'a> PcmFrame<'a> {
    pub(crate) fn new(channels: [&'a [f32; FRAME_SAMPLES]; 2]) -> Self {
        Self { channels }
    }

    fn channels(&self) -> &[&'a [f32; FRAME_SAMPLES]; 2] {
        &self.channels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedFrame {
    bit_count: usize,
}

impl EncodedFrame {
    pub(crate) fn bit_count(self) -> usize {
        self.bit_count
    }

    pub(crate) fn byte_count(self) -> usize {
        self.bit_count.div_ceil(8)
    }
}

/// Typed failures from the strategy core, before the stream applies its
/// explicit silence-substitution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameEncodeError {
    OutputTooSmall { needed: usize, actual: usize },
    CleanRejected(clean::CleanFrameError),
    DbaRejected(dba::DbaFrameError),
}

impl FrameEncodeError {
    pub(crate) fn is_silence_fallback_eligible(self) -> bool {
        matches!(self, Self::CleanRejected(_) | Self::DbaRejected(_))
    }
}

/// The sole strategy switch for classic ATRAC3 frame encoding.
pub(crate) enum EncoderCore {
    Clean(Box<clean::CleanEncoder>),
    Dba(Box<dba::DbaFrameEncoder>),
}

impl EncoderCore {
    pub(crate) fn new(profile: Atrac3Profile) -> Self {
        match profile.strategy() {
            EncoderStrategy::Clean => Self::Clean(Box::new(clean::CleanEncoder::new(profile))),
            EncoderStrategy::Dba => Self::Dba(Box::new(
                dba::DbaFrameEncoder::for_profile(profile)
                    .expect("validated DBA profile must have a core configuration"),
            )),
        }
    }

    pub(crate) fn encode_frame(
        &mut self,
        pcm: PcmFrame<'_>,
        output: &mut [u8],
        frame_bytes: usize,
    ) -> Result<EncodedFrame, FrameEncodeError> {
        if output.len() < frame_bytes {
            return Err(FrameEncodeError::OutputTooSmall {
                needed: frame_bytes,
                actual: output.len(),
            });
        }
        match self {
            Self::Clean(encoder) => encoder
                .encode_frame(pcm.channels(), output)
                .map(|bit_count| EncodedFrame { bit_count })
                .map_err(FrameEncodeError::CleanRejected),
            Self::Dba(encoder) => encoder
                .encode_frame(pcm.channels(), output)
                .map(|()| EncodedFrame {
                    bit_count: frame_bytes * 8,
                })
                .map_err(FrameEncodeError::DbaRejected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_capacity_failure_is_not_fallback_eligible() {
        let profile = Atrac3Profile::new(132, 2).unwrap();
        let mut core = EncoderCore::new(profile);
        let pcm = [0.0f32; FRAME_SAMPLES];
        let error = core
            .encode_frame(
                PcmFrame::new([&pcm, &pcm]),
                &mut [0u8; 4],
                profile.internal_frame_bytes(),
            )
            .unwrap_err();
        assert_eq!(
            error,
            FrameEncodeError::OutputTooSmall {
                needed: profile.internal_frame_bytes(),
                actual: 4,
            }
        );
        assert!(!error.is_silence_fallback_eligible());
    }

    #[test]
    fn codec_rejections_are_fallback_eligible() {
        assert!(
            FrameEncodeError::CleanRejected(clean::CleanFrameError::ChannelRejected { channel: 0 })
                .is_silence_fallback_eligible()
        );
        assert!(
            FrameEncodeError::DbaRejected(dba::DbaFrameError::MissingGainResult { channel: 1 })
                .is_silence_fallback_eligible()
        );
    }
}
