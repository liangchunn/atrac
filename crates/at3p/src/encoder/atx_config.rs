use super::profile::{mono_profile_by_frame_bytes, stereo_profile_by_frame_bytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedCodecInfo {
    pub raw: u32,
    pub codec_family: u8,
    pub frame_bytes: u32,
    pub channel_mode: u8,
    pub sample_rate_id: u8,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtxConfigError {
    UnsupportedCodecInfo(u32),
    UnsupportedSampleRateId(u8),
    UnsupportedSampleRate(u32),
    UnsupportedChannelMode(u8),
    UnsupportedFrameBytes(u32),
}

/// Decode an ATRAC3plus codec_info word into its native bitfields and accept it
/// iff it names one of the nine 44.1 kHz stereo rows.
///
/// Mirrors `atrac_init_encode` (libatrac.c 3439-3445, native 0x9d80):
/// `frame_bytes = (ci & 0x3ff) * 8 + 8`, `channel_mode = (ci >> 10) & 7`,
/// `sample_rate_id = (ci >> 13) & 7`, `codec_family = ci >> 24` (must be 1 for
/// ATRAC3plus). Widened from 352-only to the nine stereo rows (docs/13 §0.1)
/// and, per docs/14 §0.1, to the five 44.1 kHz MONO rows: codec_family != 1,
/// the 48 kHz sample-rate id, an unsupported channel selector, and a
/// frame_bytes that is not a row at the decoded channel mode each keep a
/// distinct typed rejection. The frame-size check is keyed BY channel mode
/// (channel_mode 2 → the nine stereo frame sizes; channel_mode 1 → the five
/// mono frame sizes 192/280/376/560/744), so e.g. a channel_mode-1 word with a
/// stereo-only frame size (like 936) still rejects.
pub fn decode_target_codec_info(codec_info: u32) -> Result<DecodedCodecInfo, AtxConfigError> {
    let codec_family = (codec_info >> 24) as u8;
    if codec_family != 1 {
        return Err(AtxConfigError::UnsupportedCodecInfo(codec_info));
    }

    let sample_rate_id = ((codec_info >> 13) & 7) as u8;
    let sample_rate = match sample_rate_id {
        1 => 44_100,
        // sample_rate_id 0 → 32000, 2 → 48000 are library-valid srates but out
        // of scope here; anything else is likewise rejected typed.
        other => return Err(AtxConfigError::UnsupportedSampleRateId(other)),
    };

    let channel_mode = ((codec_info >> 10) & 7) as u8;
    let frame_bytes = (codec_info & 0x3ff) * 8 + 8;
    // Row match keyed by channel mode (mirrors the library's exact-row-match
    // miss at init). Stereo mode 2 and mono mode 1 have disjoint row sets;
    // every other selector is an unsupported channel mode.
    let frame_bytes_is_a_row = match channel_mode {
        2 => stereo_profile_by_frame_bytes(frame_bytes).is_some(),
        1 => mono_profile_by_frame_bytes(frame_bytes).is_some(),
        _ => return Err(AtxConfigError::UnsupportedChannelMode(channel_mode)),
    };
    if !frame_bytes_is_a_row {
        return Err(AtxConfigError::UnsupportedFrameBytes(frame_bytes));
    }

    Ok(DecodedCodecInfo {
        raw: codec_info,
        codec_family,
        frame_bytes,
        channel_mode,
        sample_rate_id,
        sample_rate,
    })
}

/// Serialize the low two big-endian config bytes for an ATRAC3plus target,
/// mirroring the native `codec_info` low-word layout `(sample_rate_id << 13) |
/// (channel_mode << 10) | (frame_bytes / 8 - 1)`. Widened from 352-only to the
/// nine stereo rows and, per docs/14 §0.1, the five mono rows: any non-44.1 kHz
/// srate, an unsupported channel mode, or a frame_bytes outside the rows for the
/// requested channel mode keeps its typed rejection. The frame-size check is
/// keyed by channel mode (2 → nine stereo sizes; 1 → five mono sizes), so a mono
/// channel mode with a stereo-only frame size (e.g. 936) still rejects.
pub fn serialize_config(
    sample_rate: u32,
    channel_mode: u8,
    frame_bytes: u32,
) -> Result<[u8; 2], AtxConfigError> {
    let sample_rate_id = match sample_rate {
        44_100 => 1u16,
        other => return Err(AtxConfigError::UnsupportedSampleRate(other)),
    };
    let frame_bytes_is_a_row = match channel_mode {
        2 => stereo_profile_by_frame_bytes(frame_bytes).is_some(),
        1 => mono_profile_by_frame_bytes(frame_bytes).is_some(),
        _ => return Err(AtxConfigError::UnsupportedChannelMode(channel_mode)),
    };
    if !frame_bytes_is_a_row {
        return Err(AtxConfigError::UnsupportedFrameBytes(frame_bytes));
    }

    let frame_units_minus_one = ((frame_bytes >> 3) - 1) as u16;
    let packed = (sample_rate_id << 13) | (u16::from(channel_mode) << 10) | frame_units_minus_one;
    Ok(packed.to_be_bytes())
}
