use super::atracx::{ATRACX_FRAME_BYTES, AtracxWaveFormat, fact_payload};

pub const ATRACX_HEADER_LEN: u32 = 0x64;

/// The 352 kbps `codec_info` word (`0x0100_28ff`). The two big-endian low bytes
/// (`0x28, 0xff`) are the codec-info bytes emitted at fmt payload bytes 42-43.
const ATRACX_352_CODEC_INFO: u32 = 0x0100_28ff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiffWriteError {
    FrameCountTooLarge(u32),
}

/// `data` chunk size for `frame_count` output frames of `frame_bytes` each
/// (docs/13 §0.3: `data_size = frame_count × frame_bytes`, rate-parameterized).
pub fn atracx_data_size_for_rate(
    frame_count: u32,
    frame_bytes: u32,
) -> Result<u32, RiffWriteError> {
    frame_count
        .checked_mul(frame_bytes)
        .ok_or(RiffWriteError::FrameCountTooLarge(frame_count))
}

/// Total file size = header (`0x64`) + `data` payload, per rate.
pub fn atracx_file_size_for_rate(
    frame_count: u32,
    frame_bytes: u32,
) -> Result<u32, RiffWriteError> {
    ATRACX_HEADER_LEN
        .checked_add(atracx_data_size_for_rate(frame_count, frame_bytes)?)
        .ok_or(RiffWriteError::FrameCountTooLarge(frame_count))
}

/// RIFF chunk size = file size − 8, per rate.
pub fn atracx_riff_size_for_rate(
    frame_count: u32,
    frame_bytes: u32,
) -> Result<u32, RiffWriteError> {
    Ok(atracx_file_size_for_rate(frame_count, frame_bytes)? - 8)
}

pub fn atracx_data_size(frame_count: u32) -> Result<u32, RiffWriteError> {
    atracx_data_size_for_rate(frame_count, ATRACX_FRAME_BYTES)
}

pub fn atracx_file_size(frame_count: u32) -> Result<u32, RiffWriteError> {
    atracx_file_size_for_rate(frame_count, ATRACX_FRAME_BYTES)
}

pub fn atracx_riff_size(frame_count: u32) -> Result<u32, RiffWriteError> {
    atracx_riff_size_for_rate(frame_count, ATRACX_FRAME_BYTES)
}

/// Write the 0x64-byte ATRACX RIFF/WAV header for one native stereo bitrate
/// (docs/13 §0.3). The layout is rate-independent (RIFF/WAVE, `fmt `(52) at
/// 0x0c, `fact`(12) at 0x48, `data` at 0x5c); only `avg_bytes_per_sec`,
/// `block_align`, the two codec-info bytes, and the size fields vary by rate.
/// `frame_bytes` = the codec's per-frame byte count (= `block_align`),
/// `codec_info` = the profile's `0x0100_28nn` word.
pub fn write_atracx_header_for_rate(
    input_sample_frames: u32,
    frame_count: u32,
    frame_bytes: u16,
    codec_info: u32,
) -> Result<Vec<u8>, RiffWriteError> {
    write_atracx_header_for_rate_channels(
        2,
        input_sample_frames,
        frame_count,
        frame_bytes,
        codec_info,
    )
}

/// Write the 0x64-byte ATRACX RIFF/WAV header for one native bitrate at a given
/// channel count. Widens [`write_atracx_header_for_rate`] channel-aware for the
/// docs/14 mono rows (`channels == 1`) while keeping the stereo path
/// (`channels == 2`) byte-identical (`write_atracx_header_for_rate` delegates
/// here with `channels = 2`).
///
/// The header LAYOUT is channel-independent (RIFF/WAVE, `fmt `(52) at 0x0c,
/// `fact`(12) at 0x48, `data` at 0x5c; MEASURED identical offsets across the
/// docs/14 §0.3). Only the `fmt ` payload's `channels`/`channel_mask` fields and
/// the codec-info bytes carry channel/rate context — see
/// [`AtracxWaveFormat::for_rate_channels`]. The `fact` chunk `[N, 2048, 2232]`
/// is channel-independent (MEASURED byte-identical to the stereo rows; the
/// decompile calls `getAtracEncdelay(.., 0xb8, 0xb8)` for ATRACX regardless of
/// channel count).
pub fn write_atracx_header_for_rate_channels(
    channels: u16,
    input_sample_frames: u32,
    frame_count: u32,
    frame_bytes: u16,
    codec_info: u32,
) -> Result<Vec<u8>, RiffWriteError> {
    let data_size = atracx_data_size_for_rate(frame_count, frame_bytes as u32)?;
    let riff_size = atracx_riff_size_for_rate(frame_count, frame_bytes as u32)?;
    let mut bytes = Vec::with_capacity(ATRACX_HEADER_LEN as usize);

    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&52u32.to_le_bytes());
    bytes.extend_from_slice(
        &AtracxWaveFormat::for_rate_channels(channels, frame_bytes, codec_info).to_fmt_payload(),
    );
    bytes.extend_from_slice(b"fact");
    bytes.extend_from_slice(&12u32.to_le_bytes());
    bytes.extend_from_slice(&fact_payload(input_sample_frames));
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());

    debug_assert_eq!(bytes.len(), ATRACX_HEADER_LEN as usize);
    Ok(bytes)
}

/// 352 kbps wrapper over [`write_atracx_header_for_rate`]; kept byte-identical so
/// every existing 352 call site is unchanged.
pub fn write_atracx_header(
    input_sample_frames: u32,
    frame_count: u32,
) -> Result<Vec<u8>, RiffWriteError> {
    write_atracx_header_for_rate(
        input_sample_frames,
        frame_count,
        ATRACX_FRAME_BYTES as u16,
        ATRACX_352_CODEC_INFO,
    )
}
