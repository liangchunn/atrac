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
mod riff;
#[allow(dead_code)]
mod tables;

pub use encoder::payload::{
    ComputedFileError as FileEncodeError, ComputedPayloadError as PayloadEncodeError,
    ComputedWriteError as EncodeError, ComputedWriteStage as WriteStage, EncodePhase,
    EncodeProgress,
};
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
) -> Result<Vec<u8>, FileEncodeError> {
    match profile.channel_mode() {
        ChannelMode::Mono => encoder::payload::assemble_computed_atracx_file_for_mono_profile(
            profile,
            input_sample_frames,
            channels,
        ),
        ChannelMode::Stereo if channels.len() == 2 => {
            encoder::payload::assemble_computed_atracx_file_for_profile(
                profile,
                input_sample_frames,
                &channels[0],
                &channels[1],
            )
        }
        ChannelMode::Stereo => Err(FileEncodeError::UnsupportedInputShape {
            expected_sample_frames: input_sample_frames,
            actual_sample_frames: input_sample_frames,
            left_len: channels.first().map_or(0, Vec::len),
            right_len: channels.get(1).map_or(0, Vec::len),
        }),
    }
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
