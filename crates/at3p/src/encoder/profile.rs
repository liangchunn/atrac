//! ATRAC3plus stereo encode profiles — the nine 44.1 kHz stereo rows of the
//! at3tool `gAtracCodecParam` driver table.
//!
//! Native evidence (this repo; at3tool sha1
//! c2bf98e178a48159336887c6c21e23928197b806): `gAtracCodecParam`, native vaddr
//! 0x804cf20 / ELF file offset 0x4f20, 19 rows × 0x24 bytes. Fields per row:
//! codec_kind(+0), bitrate(+4), channels(+8), frame_samples(+0xc),
//! sample_rate(+0x10), block_align / frame_bytes(+0x14), codec_info(+0x18),
//! enc_alg(+0x1c), mono2st(+0x20). The driver matcher `getAtracEncodeSetting`
//! (at3tool.c 2304-2334, native 0x804c0c2) matches a row on
//! `(bitrate=+4, channels=+8, sample_rate=+0x10)`; a miss returns error
//! 0x81000006 "Not Supported Param".
//!
//! Rows 10-18 are the nine stereo ATRAC3plus profiles ported here (codec_kind
//! 5 = ATRAC3plus, channels 2, frame_samples 2048, enc_alg 1, mono2st 0). Rows
//! 5-9 are ATRAC3plus MONO (32/48/64/96/128 kbps — now IN scope, docs/14 §0.1;
//! 32 kbps exists ONLY as mono) and rows 0-4 are ATRAC3 non-plus
//! (52/66/105/132 kbps, codec_kind 3, 1024 samples/frame — different codec).
//!
//! Mono rows 5-9 (docs/14 §2.1): codec_kind 5, channels 1, frame_samples 2048,
//! sample_rate 44100, enc_alg 1, mono2st 0 (at3tool's `convertPcmMono2Stereo`
//! is gated on the codec-param `mono_to_stereo` field, which is 0 for all five
//! ATRAC3plus mono rows — a TRUE 1-channel encode, docs/14 §2.1). Their
//! `codec_info` obeys the SAME law with `channel_mode = 1`:
//! `0x0100_0000 | (1 << 13) | (1 << 10) | (frame_bytes / 8 - 1)` — synthesis
//! matches all five .rodata dump literals (0x01002417/0x01002422/0x0100242e/
//! 0x01002445/0x0100245c), cross-checked by the mono profile row test in
//! `tests/encoder_config.rs`.
//!
//! The `codec_info` word of every stereo row is
//! `0x0100_0000 | (sample_rate_id << 13) | (channel_mode << 10) |
//! (frame_bytes / 8 - 1)` with `sample_rate_id = 1` (44.1 kHz) and
//! `channel_mode = 2` (stereo). The library decodes it back in
//! `atrac_init_encode` (libatrac.c 3439-3445, native 0x9d80):
//! `frame_bytes = (ci & 0x3ff) * 8 + 8`, `channel_mode = (ci >> 10) & 7`,
//! `sample_rate_id = (ci >> 13) & 7`, `codec_family = ci >> 24`. The founding
//! §2.1 static columns for frame_bytes / codecInfo are pinned by this direct
//! .rodata dump.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeProfile {
    pub codec_kind: u32,
    pub bitrate_kbps: u32,
    pub channels: u16,
    pub sample_rate: u32,
    pub frame_samples: u32,
    pub frame_bytes: u32,
    pub codec_info: u32,
    pub encode_algorithm: u32,
    pub mono_to_stereo: bool,
}

/// Build one stereo profile from the two per-rate `gAtracCodecParam` facts that
/// vary (bitrate, frame_bytes); the rest are the shared stereo-row constants.
/// The `codec_info` is synthesized by the native formula and cross-checked
/// against the .rodata dump literal by [`stereo_codec_info_synthesis_matches_dump`].
const fn stereo_profile(bitrate_kbps: u32, frame_bytes: u32) -> EncodeProfile {
    EncodeProfile {
        codec_kind: 5,
        bitrate_kbps,
        channels: 2,
        sample_rate: 44_100,
        frame_samples: 2048,
        frame_bytes,
        // 0x01000000 (family 1) | (sample_rate_id 1 << 13) |
        // (channel_mode 2 << 10) | (frame_bytes/8 - 1).
        codec_info: 0x0100_0000 | (1 << 13) | (2 << 10) | (frame_bytes / 8 - 1),
        encode_algorithm: 1,
        mono_to_stereo: false,
    }
}

// The nine stereo rows (gAtracCodecParam rows 10-18). Frame bytes and codec_info
// low bytes verified against the dump: 280/0x22, 376/0x2e, 560/0x45, 744/0x5c,
// 936/0x74, 1120/0x8b, 1488/0xb9, 1864/0xe8, 2048/0xff.
pub const ATRAC3PLUS_48: EncodeProfile = stereo_profile(48, 280);
pub const ATRAC3PLUS_64: EncodeProfile = stereo_profile(64, 376);
pub const ATRAC3PLUS_96: EncodeProfile = stereo_profile(96, 560);
pub const ATRAC3PLUS_128: EncodeProfile = stereo_profile(128, 744);
pub const ATRAC3PLUS_160: EncodeProfile = stereo_profile(160, 936);
pub const ATRAC3PLUS_192: EncodeProfile = stereo_profile(192, 1120);
pub const ATRAC3PLUS_256: EncodeProfile = stereo_profile(256, 1488);
pub const ATRAC3PLUS_320: EncodeProfile = stereo_profile(320, 1864);

/// The 352 kbps stereo profile (shipped target). Kept as a named const so every
/// existing 352 call site compiles unchanged; it equals the 352 row of the
/// stereo table below.
pub const ATRAC3PLUS_352: EncodeProfile = stereo_profile(352, 2048);

/// The nine ATRAC3plus stereo 44.1 kHz profiles, ordered by ascending bitrate
/// (gAtracCodecParam rows 10-18).
pub const ATRAC3PLUS_STEREO_PROFILES: [EncodeProfile; 9] = [
    ATRAC3PLUS_48,
    ATRAC3PLUS_64,
    ATRAC3PLUS_96,
    ATRAC3PLUS_128,
    ATRAC3PLUS_160,
    ATRAC3PLUS_192,
    ATRAC3PLUS_256,
    ATRAC3PLUS_320,
    ATRAC3PLUS_352,
];

/// Typed lookup: the stereo profile for `bitrate_kbps`, mirroring the at3tool
/// `getAtracEncodeSetting` bitrate match (restricted to the nine stereo rows).
/// `None` means no stereo ATRAC3plus row exists at that rate (the CLI turns this
/// into a classified typed rejection — mono-only 32, ATRAC3 family, or plain
/// unsupported).
pub fn stereo_profile_by_bitrate_kbps(bitrate_kbps: u32) -> Option<EncodeProfile> {
    ATRAC3PLUS_STEREO_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.bitrate_kbps == bitrate_kbps)
}

/// The stereo profile whose `frame_bytes` matches, mirroring the exact
/// `frame_bytes` row match the library performs at init after decoding the
/// codec_info bitfield. `None` means no stereo row has that frame size.
pub fn stereo_profile_by_frame_bytes(frame_bytes: u32) -> Option<EncodeProfile> {
    ATRAC3PLUS_STEREO_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.frame_bytes == frame_bytes)
}

// ===========================================================================
// docs/14 §0.1 — the five ATRAC3plus MONO rows (gAtracCodecParam rows 5-9).
// ===========================================================================

/// Build one mono profile from the two per-rate `gAtracCodecParam` facts that
/// vary (bitrate, frame_bytes); the rest are the shared mono-row constants
/// (docs/14 §2.1). The `codec_info` is synthesized by the SAME native formula
/// as stereo but with `channel_mode = 1`, and cross-checked against the .rodata
/// dump literal by the mono profile row test in `tests/encoder_config.rs`.
const fn mono_profile(bitrate_kbps: u32, frame_bytes: u32) -> EncodeProfile {
    EncodeProfile {
        codec_kind: 5,
        bitrate_kbps,
        channels: 1,
        sample_rate: 44_100,
        frame_samples: 2048,
        frame_bytes,
        // 0x01000000 (family 1) | (sample_rate_id 1 << 13) |
        // (channel_mode 1 << 10) | (frame_bytes/8 - 1).
        codec_info: 0x0100_0000 | (1 << 13) | (1 << 10) | (frame_bytes / 8 - 1),
        encode_algorithm: 1,
        // at3tool `convertPcmMono2Stereo` never fires for A3+ mono (mono2st == 0,
        // docs/14 §2.1): a TRUE 1-channel library encode.
        mono_to_stereo: false,
    }
}

// The five mono rows (gAtracCodecParam rows 5-9). Frame bytes and codec_info
// low bytes verified against the dump: 192/0x17, 280/0x22, 376/0x2e, 560/0x45,
// 744/0x5c. 32 kbps is the only NEW frame size (192); the other four share
// frame bytes with their stereo siblings but carry channel_mode 1.
pub const ATRAC3PLUS_MONO_32: EncodeProfile = mono_profile(32, 192);
pub const ATRAC3PLUS_MONO_48: EncodeProfile = mono_profile(48, 280);
pub const ATRAC3PLUS_MONO_64: EncodeProfile = mono_profile(64, 376);
pub const ATRAC3PLUS_MONO_96: EncodeProfile = mono_profile(96, 560);
pub const ATRAC3PLUS_MONO_128: EncodeProfile = mono_profile(128, 744);

/// The five ATRAC3plus mono 44.1 kHz profiles, ordered by ascending bitrate
/// (gAtracCodecParam rows 5-9).
pub const ATRAC3PLUS_MONO_PROFILES: [EncodeProfile; 5] = [
    ATRAC3PLUS_MONO_32,
    ATRAC3PLUS_MONO_48,
    ATRAC3PLUS_MONO_64,
    ATRAC3PLUS_MONO_96,
    ATRAC3PLUS_MONO_128,
];

/// Typed lookup: the mono profile for `bitrate_kbps` (restricted to the five
/// mono rows). `None` means no mono ATRAC3plus row exists at that rate.
pub fn mono_profile_by_bitrate_kbps(bitrate_kbps: u32) -> Option<EncodeProfile> {
    ATRAC3PLUS_MONO_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.bitrate_kbps == bitrate_kbps)
}

/// The mono profile whose `frame_bytes` matches, mirroring the exact
/// `frame_bytes` row match the library performs at init (keyed by channel mode
/// 1). `None` means no mono row has that frame size. Note four of the five mono
/// frame sizes (280/376/560/744) coincide with stereo rows, so the channel mode
/// is load-bearing for disambiguation (see [`profile_by_bitrate_and_channels`]).
pub fn mono_profile_by_frame_bytes(frame_bytes: u32) -> Option<EncodeProfile> {
    ATRAC3PLUS_MONO_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.frame_bytes == frame_bytes)
}

/// Channel-aware ATRAC3plus profile lookup over the 14 tool-reachable A3+ rows,
/// mirroring the native at3tool `getAtracEncodeSetting` match on
/// `(bitrate, channels, sample_rate)` (at3tool.c 2304-2334, native 0x804c0c2;
/// docs/14 §2.1). Stereo (`channels == 2`) resolves the nine stereo rows; mono
/// (`channels == 1`) resolves the five mono rows. Any other channel count, or a
/// `(bitrate, channels)` pair with no gAtracCodecParam row, is `None` — a native
/// "Not Supported Param" reject (measured sweep, docs/14 §2 evidence C: mono
/// accepts 32/48/64/96/128 only; stereo rejects 32).
pub fn profile_by_bitrate_and_channels(bitrate_kbps: u32, channels: u16) -> Option<EncodeProfile> {
    match channels {
        1 => mono_profile_by_bitrate_kbps(bitrate_kbps),
        2 => stereo_profile_by_bitrate_kbps(bitrate_kbps),
        _ => None,
    }
}
