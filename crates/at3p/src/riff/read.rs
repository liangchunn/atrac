#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub id: [u8; 4],
    pub offset: usize,
    pub payload_offset: usize,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiffWaveChunks {
    pub riff_size: u32,
    pub form: [u8; 4],
    pub chunks: Vec<Chunk>,
    pub fmt: Chunk,
    pub data: Chunk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmWaveInfo {
    pub chunks: RiffWaveChunks,
    pub format: PcmFormat,
    pub sample_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StereoPcm {
    pub info: PcmWaveInfo,
    frames: Vec<[i16; 2]>,
}

impl StereoPcm {
    pub fn frames(&self) -> &[[i16; 2]] {
        &self.frames
    }

    pub fn frame(&self, index: usize) -> Option<[i16; 2]> {
        self.frames.get(index).copied()
    }
}

/// A channel-vec PCM carrier (docs/14 §0.4): the de-interleaved per-channel i16
/// streams (`channels[c][sample]`) plus the parsed header. The generalization of
/// [`StereoPcm`] over 1 or 2 channels; the stereo loader is a thin reshape of
/// this into `[i16; 2]` frame pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmChannels {
    pub info: PcmWaveInfo,
    channels: Vec<Vec<i16>>,
}

impl PcmChannels {
    /// The de-interleaved per-channel i16 sample streams. `channels()[c]` has
    /// `info.sample_frames` samples.
    pub fn channels(&self) -> &[Vec<i16>] {
        &self.channels
    }

    /// One channel's i16 sample stream (`None` if out of range).
    pub fn channel(&self, index: usize) -> Option<&[i16]> {
        self.channels.get(index).map(Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiffReadError {
    TooShort,
    InvalidRiffId([u8; 4]),
    InvalidWaveForm([u8; 4]),
    TruncatedChunkHeader { offset: usize },
    TruncatedChunkPayload { offset: usize, size: u32 },
    ChunkOffsetOverflow { offset: usize, size: u32 },
    MissingFmtChunk,
    MissingDataChunk,
    FmtChunkTooShort { size: u32 },
    UnsupportedFormatTag(u16),
    UnsupportedChannelCount(u16),
    UnsupportedSampleRate(u32),
    UnsupportedBlockAlign(u16),
    UnsupportedBitsPerSample(u16),
    DataSizeNotAligned { data_size: u32, block_align: u16 },
    SampleDataTooLarge(u32),
}

pub fn walk_wave_chunks(bytes: &[u8]) -> Result<RiffWaveChunks, RiffReadError> {
    if bytes.len() < 12 {
        return Err(RiffReadError::TooShort);
    }

    let riff_id = fourcc_at(bytes, 0);
    if &riff_id != b"RIFF" {
        return Err(RiffReadError::InvalidRiffId(riff_id));
    }

    let form = fourcc_at(bytes, 8);
    if &form != b"WAVE" {
        return Err(RiffReadError::InvalidWaveForm(form));
    }

    let riff_size = read_u32_le(bytes, 4);
    let mut chunks = Vec::new();
    let mut fmt = None;
    let mut data = None;
    let mut offset = 12usize;

    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err(RiffReadError::TruncatedChunkHeader { offset });
        }

        let id = fourcc_at(bytes, offset);
        let size = read_u32_le(bytes, offset + 4);
        let payload_offset = offset
            .checked_add(8)
            .ok_or(RiffReadError::ChunkOffsetOverflow { offset, size })?;
        let size_usize = usize::try_from(size)
            .map_err(|_| RiffReadError::ChunkOffsetOverflow { offset, size })?;
        let payload_end = payload_offset
            .checked_add(size_usize)
            .ok_or(RiffReadError::ChunkOffsetOverflow { offset, size })?;
        if payload_end > bytes.len() {
            return Err(RiffReadError::TruncatedChunkPayload { offset, size });
        }

        let chunk = Chunk {
            id,
            offset,
            payload_offset,
            size,
        };
        chunks.push(chunk);

        if &id == b"fmt " {
            fmt = Some(chunk);
        } else if &id == b"data" {
            data = Some(chunk);
        }

        if fmt.is_some() && data.is_some() {
            break;
        }

        let padded_size = size_usize
            .checked_add(size_usize & 1)
            .ok_or(RiffReadError::ChunkOffsetOverflow { offset, size })?;
        offset = payload_offset
            .checked_add(padded_size)
            .ok_or(RiffReadError::ChunkOffsetOverflow { offset, size })?;
    }

    Ok(RiffWaveChunks {
        riff_size,
        form,
        chunks,
        fmt: fmt.ok_or(RiffReadError::MissingFmtChunk)?,
        data: data.ok_or(RiffReadError::MissingDataChunk)?,
    })
}

/// Light header peek for CLI channel classification (docs/14 §0.1): walk the
/// RIFF/WAVE chunks and parse the `fmt ` fields WITHOUT the stereo-only gates
/// (channel count, block align, etc.). This is intentionally permissive so the
/// CLI can read the channel count of ANY PCM WAV and resolve the channel-aware
/// profile before the strict, channel-specific reader runs; the format-tag / fmt
/// size guards are kept so a non-PCM or truncated `fmt ` still fails typed.
/// `parse_target_pcm_wave` / `load_target_pcm_wave` remain the stereo entries;
/// docs/14 §0.4 landed the channel-aware strict reader
/// ([`parse_target_pcm_wave_for_channels`] / [`load_target_pcm_wave_for_channels`])
/// for the mono PCM carrier, and the stereo entries now delegate to it with
/// `channels = 2`.
pub fn parse_wave_format(bytes: &[u8]) -> Result<PcmFormat, RiffReadError> {
    let chunks = walk_wave_chunks(bytes)?;
    if chunks.fmt.size < 16 {
        return Err(RiffReadError::FmtChunkTooShort {
            size: chunks.fmt.size,
        });
    }

    let fmt = chunks.fmt.payload_offset;
    let format = PcmFormat {
        format_tag: read_u16_le(bytes, fmt),
        channels: read_u16_le(bytes, fmt + 2),
        sample_rate: read_u32_le(bytes, fmt + 4),
        avg_bytes_per_sec: read_u32_le(bytes, fmt + 8),
        block_align: read_u16_le(bytes, fmt + 12),
        bits_per_sample: read_u16_le(bytes, fmt + 14),
    };

    if format.format_tag != 1 {
        return Err(RiffReadError::UnsupportedFormatTag(format.format_tag));
    }

    Ok(format)
}

/// Strict channel-aware PCM validation (docs/14 §0.4). `channels` is the EXPECTED
/// channel count (1 or 2); an expected count outside `{1, 2}` fails explicit with
/// [`UnsupportedChannelCount`](RiffReadError::UnsupportedChannelCount) rather than
/// guessing. The file-field gate ORDER is identical to the shipped stereo reader
/// (format_tag → channels → sample_rate → block_align → bits → data alignment),
/// only parameterized on the expected channel count:
///
/// - `format_tag == 1` (plain PCM) — the native at3tool `ParseWaveHeader` gate
///   (decompiled/at3tool.c 821-1050; `sVar2 != 1 → return -0x7bfff001`, E1).
/// - `channels == channels` and `bits_per_sample == 16` — native reads these and
///   rejects `channels == 0 || bits == 0` at the `data` chunk (E1).
/// - `sample_rate == 44100` — the only sample rate any A3+ mono/stereo row carries
///   (native rejects other rates later at `getAtracEncodeSetting`'s row match;
///   the repo reader stands in for that here).
/// - `block_align == channels * 2` (exact) and `data.size % block_align == 0` —
///   these two are the REPO's established strictness, NOT native gates: native
///   reads `block_align` (`param_2[6]`) but never validates it, and a non-multiple
///   data size silently truncates by integer division (E1). Kept because the
///   shipped stereo reader has always been this strict (it gates `block_align == 4`).
///
/// `sample_frames = data.size / block_align` mirrors native's
/// `data_size / (channels * (bits >> 3))` with `bits == 16` (E1).
pub fn parse_target_pcm_wave_for_channels(
    bytes: &[u8],
    channels: u16,
) -> Result<PcmWaveInfo, RiffReadError> {
    // Fail-explicit on an unsupported EXPECTED channel count (never guess).
    if channels != 1 && channels != 2 {
        return Err(RiffReadError::UnsupportedChannelCount(channels));
    }

    let chunks = walk_wave_chunks(bytes)?;
    if chunks.fmt.size < 16 {
        return Err(RiffReadError::FmtChunkTooShort {
            size: chunks.fmt.size,
        });
    }

    let fmt = chunks.fmt.payload_offset;
    let format = PcmFormat {
        format_tag: read_u16_le(bytes, fmt),
        channels: read_u16_le(bytes, fmt + 2),
        sample_rate: read_u32_le(bytes, fmt + 4),
        avg_bytes_per_sec: read_u32_le(bytes, fmt + 8),
        block_align: read_u16_le(bytes, fmt + 12),
        bits_per_sample: read_u16_le(bytes, fmt + 14),
    };

    if format.format_tag != 1 {
        return Err(RiffReadError::UnsupportedFormatTag(format.format_tag));
    }
    if format.channels != channels {
        return Err(RiffReadError::UnsupportedChannelCount(format.channels));
    }
    if format.sample_rate != 44_100 {
        return Err(RiffReadError::UnsupportedSampleRate(format.sample_rate));
    }
    if format.block_align != channels * 2 {
        return Err(RiffReadError::UnsupportedBlockAlign(format.block_align));
    }
    if format.bits_per_sample != 16 {
        return Err(RiffReadError::UnsupportedBitsPerSample(
            format.bits_per_sample,
        ));
    }
    if chunks.data.size % u32::from(format.block_align) != 0 {
        return Err(RiffReadError::DataSizeNotAligned {
            data_size: chunks.data.size,
            block_align: format.block_align,
        });
    }

    Ok(PcmWaveInfo {
        sample_frames: chunks.data.size / u32::from(format.block_align),
        chunks,
        format,
    })
}

/// The shipped stereo entry — delegates to [`parse_target_pcm_wave_for_channels`]
/// with `channels = 2` (byte-identical gates and result).
pub fn parse_target_pcm_wave(bytes: &[u8]) -> Result<PcmWaveInfo, RiffReadError> {
    parse_target_pcm_wave_for_channels(bytes, 2)
}

/// Load and de-interleave a strict channel-aware PCM WAV (docs/14 §0.4). `channels`
/// is the EXPECTED count (1 or 2). Returns one i16 stream per channel of
/// `info.sample_frames` samples each (a straight copy for mono).
pub fn load_target_pcm_wave_for_channels(
    bytes: &[u8],
    channels: u16,
) -> Result<PcmChannels, RiffReadError> {
    let info = parse_target_pcm_wave_for_channels(bytes, channels)?;
    let data_len = usize::try_from(info.chunks.data.size)
        .map_err(|_| RiffReadError::SampleDataTooLarge(info.chunks.data.size))?;
    let data_start = info.chunks.data.payload_offset;
    let data = &bytes[data_start..data_start + data_len];

    let channel_count = usize::from(channels);
    let frame_bytes = channel_count * 2;
    let mut streams: Vec<Vec<i16>> =
        vec![Vec::with_capacity(info.sample_frames as usize); channel_count];
    for frame in data.chunks_exact(frame_bytes) {
        for (c, stream) in streams.iter_mut().enumerate() {
            let sample = i16::from_le_bytes(frame[c * 2..c * 2 + 2].try_into().unwrap());
            stream.push(sample);
        }
    }

    Ok(PcmChannels {
        info,
        channels: streams,
    })
}

/// The shipped stereo loader — delegates to [`load_target_pcm_wave_for_channels`]
/// with `channels = 2` and reshapes the two i16 streams into `[left, right]`
/// frame pairs (byte-identical to the previous direct de-interleave).
pub fn load_target_pcm_wave(bytes: &[u8]) -> Result<StereoPcm, RiffReadError> {
    let pcm = load_target_pcm_wave_for_channels(bytes, 2)?;
    let frames = pcm.channels[0]
        .iter()
        .zip(&pcm.channels[1])
        .map(|(&left, &right)| [left, right])
        .collect();
    Ok(StereoPcm {
        info: pcm.info,
        frames,
    })
}

fn fourcc_at(bytes: &[u8], offset: usize) -> [u8; 4] {
    bytes[offset..offset + 4].try_into().unwrap()
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
