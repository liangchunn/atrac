pub const ATRACX_FMT_PAYLOAD_LEN: usize = 52;
pub const ATRACX_FACT_PAYLOAD_LEN: usize = 12;
pub const ATRACX_FRAME_SAMPLES: u32 = 2048;
pub const ATRACX_FRAME_BYTES: u32 = 2048;
pub const ATRACX_DELAY_PLUS_FRAME_SAMPLES: u32 = 2232;
pub const ATRACX_GUID: [u8; 16] = [
    0xbf, 0xaa, 0x23, 0xe9, 0x58, 0xcb, 0x71, 0x44, 0xa1, 0x19, 0xff, 0xfa, 0x01, 0xe4, 0xce, 0x62,
];

/// The native `atracx_dwChannelMask` table (at3tool `.data`, link vaddr
/// `0x0804e540`; STATIC dump via `greadelf -x .data`, 9 `u32` entries, table
/// ends at `0x0804e564`). `setAtxHeader` (at3tool.c decompile, native link
/// vaddr `0x804b5c7`) emits the `fmt ` payload's `dwChannelMask` field as a
/// straight table lookup by channel count: `*(param_1 + 10) =
/// atracx_dwChannelMask[channels]` (and `param_1[1] = (short)channels`). Only
/// index 1 (mono, mask `0x1`) and index 2 (stereo, mask `0x3`) are in scope for
/// this encoder; the full 9-entry table is kept as the verbatim evidence dump.
/// The stereo entry `0x3` matches the shipped stereo constant.
pub const ATRACX_DW_CHANNEL_MASK: [u32; 9] = [
    0x0000_0000,
    0x0000_0001,
    0x0000_0003,
    0x0000_0007,
    0x0000_0107,
    0x0000_0000,
    0x0000_003f,
    0x0000_013f,
    0x0000_063f,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtracxWaveFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub cb_size: u16,
    pub frame_samples: u16,
    pub channel_mask: u32,
    pub guid: [u8; 16],
    pub extra_revision: u16,
    pub codec_info_low_bytes: [u8; 2],
    pub reserved_tail: [u8; 8],
}

/// Native `avg_bytes_per_sec` law for an ATRAC3plus stereo `fmt ` payload
/// (docs/13 §0.3, `Q-avg-Bps` RESOLVED by measurement).
///
/// Native source of truth: `setAtxHeader` (at3tool.c decompile, native link
/// vaddr `0x804b5c7`). The decompile reads
/// `(int)ROUND((double)(frame_bytes * samplerate) / (double)samples_per_frame
/// + 0.5)` with `samplerate = 44100`, `samples_per_frame = 2048`. The nine
/// captured 2026-07-07) pin the ACTUAL semantics as a **truncating** int cast,
/// i.e. round-half-up-to-nearest of the exact quotient:
///
/// ```text
/// avg_bytes_per_sec = trunc(frame_bytes * 44100 / 2048 + 0.5)
/// ```
///
/// Reading the decompile's `ROUND` as x87 `rint` (round-half-**even** on
/// `x + 0.5`) would emit 6030/8097/20156/24118 at 48/64/160/192 kbps — refuted
/// by the measured headers (6029/8096/20155/24117). The exact integer form used
/// below is provably identical here: `frame_bytes * 44100 <= 90_316_800` is
/// exact in `f64`, `/2048` is a power-of-two divide, `+0.5` is exact, and
/// `trunc` of the positive result equals integer division of
/// `(frame_bytes * 44100 + 1024) / 2048`.
///
/// The field is channel-independent: the five MEASURED mono headers
/// confirm the SAME law — 32 kbps / frame_bytes 192 (the only sub-280 point of
/// the law, mono-only) gives 4134, and the four shared-frame-bytes mono rates
/// (280/376/560/744) reproduce the stereo rows exactly (6029/8096/12059/16021).
pub const fn atracx_avg_bytes_per_sec(frame_bytes: u32) -> u32 {
    (frame_bytes * 44_100 + 1024) / 2048
}

pub fn fact_payload(input_sample_frames: u32) -> [u8; ATRACX_FACT_PAYLOAD_LEN] {
    let mut bytes = [0u8; ATRACX_FACT_PAYLOAD_LEN];
    bytes[0..4].copy_from_slice(&input_sample_frames.to_le_bytes());
    bytes[4..8].copy_from_slice(&ATRACX_FRAME_SAMPLES.to_le_bytes());
    bytes[8..12].copy_from_slice(&ATRACX_DELAY_PLUS_FRAME_SAMPLES.to_le_bytes());
    bytes
}

impl AtracxWaveFormat {
    /// Build the ATRAC3plus stereo `fmt ` payload for one native bitrate from
    /// the two primitive per-rate facts that vary: `frame_bytes` (the codec's
    /// per-frame byte count = `block_align`) and the profile's `codec_info` word
    /// (`0x0100_28nn`), whose two big-endian low bytes land at fmt payload bytes
    /// 42-43. Everything else is a rate-independent stereo constant, proven by
    /// all nine measured headers
    /// 0xfffe, `channels` 2, `sample_rate` 44100, `bits_per_sample` 0, `cb_size`
    /// 34, `frame_samples` **2048 at every rate** (the codec's samples-per-frame,
    /// NOT `frame_bytes` — they only coincide at 352), `channel_mask` 3, the
    /// ATRACX GUID, `revision` 1, and an 8-byte zero tail. `avg_bytes_per_sec`
    /// follows [`atracx_avg_bytes_per_sec`]. This constructor takes primitive
    /// params (not an `Atrac3plusProfile`) so `riff` stays independent of the codec
    /// modules (docs/02).
    pub const fn for_rate(frame_bytes: u16, codec_info: u32) -> Self {
        Self::for_rate_channels(2, frame_bytes, codec_info)
    }

    /// Build the ATRAC3plus `fmt ` payload for one native bitrate at a given
    /// channel count. Widens [`AtracxWaveFormat::for_rate`] channel-aware for
    /// the docs/14 mono rows (`channels == 1`) while keeping the stereo path
    /// (`channels == 2`) provably byte-identical (`for_rate` delegates here with
    /// `channels = 2`).
    ///
    /// Two fields track the channel count, both from `setAtxHeader` (at3tool.c
    /// decompile, native link vaddr `0x804b5c7`): `channels` is set straight
    /// from the codec-param channel count (`param_1[1] = (short)channels`), and
    /// `channel_mask` is the `atracx_dwChannelMask[channels]` table lookup
    /// (`*(param_1 + 10) = atracx_dwChannelMask[channels]`; STATIC table
    /// [`ATRACX_DW_CHANNEL_MASK`], at3tool `.data` `0x0804e540`). Everything else
    /// is channel-independent native code (same lines as the stereo path,
    /// docs/13 §0.3): `format_tag` 0xfffe, `sample_rate` 44100,
    /// `bits_per_sample` 0, `cb_size` 34, `frame_samples` 2048, the ATRACX GUID,
    /// `revision` 1, the two big-endian codec-info low bytes, and the 8-byte
    /// zero tail. `avg_bytes_per_sec` follows [`atracx_avg_bytes_per_sec`].
    ///
    /// The five MEASURED mono headers
    /// §0.3) pin `channels == 1` and `channel_mask == 0x1` at all five mono
    /// rates. Only channel counts 1 and 2 are in scope; the table const is the
    /// evidence dump, this constructor is exercised only with 1 or 2.
    pub const fn for_rate_channels(channels: u16, frame_bytes: u16, codec_info: u32) -> Self {
        Self {
            format_tag: 0xfffe,
            channels,
            sample_rate: 44_100,
            avg_bytes_per_sec: atracx_avg_bytes_per_sec(frame_bytes as u32),
            block_align: frame_bytes,
            bits_per_sample: 0,
            cb_size: 34,
            frame_samples: 2048,
            channel_mask: ATRACX_DW_CHANNEL_MASK[channels as usize],
            guid: ATRACX_GUID,
            extra_revision: 1,
            codec_info_low_bytes: [(codec_info >> 8) as u8, codec_info as u8],
            reserved_tail: [0; 8],
        }
    }

    pub fn to_fmt_payload(self) -> [u8; ATRACX_FMT_PAYLOAD_LEN] {
        let mut bytes = [0u8; ATRACX_FMT_PAYLOAD_LEN];
        bytes[0..2].copy_from_slice(&self.format_tag.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.channels.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.sample_rate.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.avg_bytes_per_sec.to_le_bytes());
        bytes[12..14].copy_from_slice(&self.block_align.to_le_bytes());
        bytes[14..16].copy_from_slice(&self.bits_per_sample.to_le_bytes());
        bytes[16..18].copy_from_slice(&self.cb_size.to_le_bytes());
        bytes[18..20].copy_from_slice(&self.frame_samples.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.channel_mask.to_le_bytes());
        bytes[24..40].copy_from_slice(&self.guid);
        bytes[40..42].copy_from_slice(&self.extra_revision.to_le_bytes());
        bytes[42..44].copy_from_slice(&self.codec_info_low_bytes);
        bytes[44..52].copy_from_slice(&self.reserved_tail);
        bytes
    }
}
