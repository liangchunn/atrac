//! Pure ATRAC3plus encoder and container inspection library.
//!
//! The crate-root profile and encoder types are the supported encoding API.
//! Native-layout, coding-pass, DSP, table, and packer modules are private
//! implementation and reference infrastructure.

#[allow(dead_code)]
mod bitstream;
#[allow(dead_code)]
mod coding;
#[allow(dead_code)]
mod dsp;
#[allow(dead_code)]
mod encoder;
#[allow(dead_code)]
mod gha;
#[allow(dead_code)]
mod pipeline;
#[allow(dead_code)]
mod riff;
#[allow(dead_code)]
mod tables;

pub use encoder::payload::{EncodeError, EncodePhase, EncodeProgress, WriteStage};
pub use encoder::profile::{
    ATRAC3PLUS_MONO_PROFILES, ATRAC3PLUS_STEREO_PROFILES, Atrac3plusProfile, ChannelMode,
    UnsupportedProfile, mono_profile_by_bitrate_kbps, mono_profile_by_frame_bytes,
    profile_by_bitrate_and_channels, stereo_profile_by_bitrate_kbps, stereo_profile_by_frame_bytes,
};
pub use encoder::stream::{
    Atrac3plusStreamEncoder as Atrac3plusEncoder, Atrac3plusStreamSummary as EncodeSummary,
    PCM_BLOCK_FRAMES,
};

/// Encode complete deinterleaved PCM channels into an ATRACX byte vector.
pub fn encode_to_vec(
    profile: &Atrac3plusProfile,
    input_sample_frames: u32,
    channels: &[Vec<i16>],
) -> Result<Vec<u8>, EncodeError> {
    if channels.len() != profile.channels() as usize {
        return Err(
            encoder::payload::ComputedFileError::UnexpectedInputChannelCount {
                expected: profile.channels() as usize,
                actual: channels.len(),
            }
            .into(),
        );
    }
    if channels
        .iter()
        .any(|channel| channel.len() != input_sample_frames as usize)
    {
        let error = match profile.channel_mode() {
            ChannelMode::Mono => encoder::payload::ComputedFileError::UnsupportedMonoInputShape {
                expected_sample_frames: input_sample_frames,
                channel_count: channels.len(),
                channel_len: channels.first().map_or(0, Vec::len),
            },
            ChannelMode::Stereo => encoder::payload::ComputedFileError::UnsupportedInputShape {
                expected_sample_frames: input_sample_frames,
                actual_sample_frames: input_sample_frames,
                left_len: channels.first().map_or(0, Vec::len),
                right_len: channels.get(1).map_or(0, Vec::len),
            },
        };
        return Err(error.into());
    }

    let mut encoder = Atrac3plusEncoder::new(Vec::new(), profile, input_sample_frames)?;
    let mut offset = 0;
    while let Some(frames) = encoder.expected_next_chunk_frames() {
        let chunk = channels
            .iter()
            .map(|channel| &channel[offset..offset + frames])
            .collect::<Vec<_>>();
        encoder.push_pcm(&chunk)?;
        offset += frames;
    }
    encoder.finish().map(|(bytes, _summary)| bytes)
}

/// Deliberately supported RIFF/WAVE inspection helpers used before encoding.
pub mod container {
    pub use crate::riff::read::{
        Chunk, PcmChannels, PcmFormat, PcmWaveInfo, RiffReadError, RiffWaveChunks, StereoPcm,
        inspect_target_pcm_wave_for_channels, inspect_wave_chunks, inspect_wave_format,
        load_target_pcm_wave, load_target_pcm_wave_for_channels, parse_target_pcm_wave,
        parse_target_pcm_wave_for_channels, parse_wave_format, walk_wave_chunks,
    };
}
