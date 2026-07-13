//! `g_a_encode_setting_atx` stereo row facts (libatrac.so.1.2.0).
//!
//! Native evidence (this repo; libatrac.so.1.2.0 sha1
//! 5f8d118d1be4c713d05449c7e21ee0b428bcb2ce): `g_a_encode_setting_atx`, nm
//! offset 0x88ee0, 41 rows × 0x1c bytes, 7 × u32 per row: bitrate(+0),
//! frame_bytes(+4), sample_rate(+8), channel_mode(+0xc), bandwidth_hz(+0x10),
//! mode_a(+0x14), mode_b(+0x18). The 44.1 kHz stereo rows are library indices
//! 6-14, dumped directly from .rodata below.
//!
//! The library validates and then exact-row-matches against this table at init:
//! `atx_set_config_info` (libatrac.c 3794-3830, native 0xad90) first gates
//! `sample_rate ∈ {44100, 48000}` (else 0x201), `channel_mode` selector 1..7
//! (else 0x202), and `frame_bytes` with `(fb - 1) < 0x2000 && (fb & 7) == 0`
//! (else 0x203); the exact `(frame_bytes, sample_rate, channel_mode)` row match
//! then happens at init (miss → 0x124). [`stereo_setting_by_row_match`] mirrors
//! that exact-match key.
//!
//! `mode_a` (+0x14) semantics — RESOLVED 2026-07-08 (docs/13 §2.3 (r), Appendix
//! B): `mode_a` is NOT the `+0xcc`/`+0xd0` gate word — that gate compares
//! `handle+0x44`, which `atx_init_encode` zeroes per block (`piVar3[0x11] = 0`,
//! decompile 48535), so it reads 0 at every stereo rate. `mode_a` IS the zeroth
//! `param_5` config word `*(handle+0x190)`: the per-block channel-mode /
//! joint-intensity-stereo producer gate. Static chain (libatrac.so 5f8d118d…,
//! Intel disasm): row+0x14 → outer-init `param_1[0x11]` → `atx_init_encode` 3rd
//! stack arg (decompile 3587) → `ecx` at the default single-block call `.L24363`
//! (native `0x5b6c9` `mov ecx,[ebp+0x10]`; `edx = 0` = block 0) →
//! `atx_init_encode_block` writes `arr190[0] = ecx` (native `0x5a916`, the
//! `param_1[param_2 + 0x64]` slot, decompile ~48449; `ecx == 4` would instead
//! store 1, but `mode_a ∈ {2,3}`) → `atx_encode_core` reads
//! `param_5 = *(handle+0x190)` (native `0x55d3f`) → `zeroth_bit_allocation_at5`.
//! So `mode_a == 3` (rates 48-256) makes the zeroth `param_5 == 3` joint-stereo
//! producer arm LIVE; `mode_a == 2` (320/352) keeps it DEAD — matching the (q)
//! runtime trace `param_5` = 3@256 / 2@320,352 and extending it to all nine
//! rates statically. Pinned by
//! `tests/native_traces.rs::param5_config_word_law_is_encode_setting_mode_a`
//! The producer arm and its consumers are a deferred multi-boundary port
//! (docs/13 §2.3); this crate reads `mode_a` only via that future port.
//!
//! `mode_b` (+0x18) still has no observed consumer for plain stereo (outer-init
//! `local_38`, decompile 3525-3530, is a no-op at `mode_b == 2`): recorded only.
//! `bandwidth_hz` (+0x10) is runtime-pinned as the `atx_init_encode_block`
//! bandwidth argument (tests/init_words_oracle.rs); not consumed by an encode
//! decision in this crate yet.

/// A `g_a_encode_setting_atx` row. Named generically (mono rows 0-4 and stereo
/// rows 6-14 share this shape); [`StereoEncodeSetting`] is a zero-churn alias
/// for the stereo-facing call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeSetting {
    pub bitrate_kbps: u32,
    pub frame_bytes: u32,
    pub sample_rate: u32,
    pub channel_mode: u32,
    /// Row+0x10. Runtime-pinned as the `atx_init_encode_block` bandwidth
    /// argument (§0.2 oracle); NOT consumed by any encode decision yet.
    pub bandwidth_hz: u32,
    /// Row+0x14. RESOLVED (docs/13 §2.3 (r)): this IS the zeroth `param_5`
    /// config word `*(handle+0x190)` — the per-block joint/intensity-stereo
    /// producer gate. `mode_a == 3` (48-256) → zeroth `param_5 == 3` arm LIVE;
    /// `== 2` (320/352) → DEAD. NOT the `+0xcc`/`+0xd0` gate (that is
    /// `handle+0x44`, zeroed per block). See the module header for the static
    /// init chain; the producer arm itself is a deferred port (docs/13 §2.3).
    pub mode_a: u32,
    /// Row+0x18. No observed consumer (§0.2 oracle residual row). Recorded
    /// only. Stereo rows carry 2; the mono rows carry 0 (docs/14 §2.1) — the
    /// mode_b reader was never identified in the stereo program, so watch the
    /// mono sweeps for a first consumer (docs/14 Appendix B row Q-M-modeb).
    pub mode_b: u32,
}

/// Zero-churn alias for the stereo-facing call sites and tests that predate the
/// mono rows (the row struct is now shape-generic, [`EncodeSetting`]).
pub type StereoEncodeSetting = EncodeSetting;

/// The nine 44.1 kHz stereo rows of `g_a_encode_setting_atx` (library indices
/// 6-14), ordered by ascending bitrate. Values are the direct .rodata dump.
///
/// NOTE: library row index 5 — `(24, 144, 44100, 2, 6890, 3, 2)` — is a real
/// 24 kbps stereo setting reachable ONLY via the library API, not via at3tool
/// (the `gAtracCodecParam` driver table has no 24 kbps stereo row, and the
/// measured native tool sweep rejects `-br 24`). It is deliberately EXCLUDED so
/// this table stays keyed to the nine tool-reachable stereo rates.
pub const STEREO_ENCODE_SETTINGS: [StereoEncodeSetting; 9] = [
    StereoEncodeSetting {
        bitrate_kbps: 48,
        frame_bytes: 280,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 13781,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 64,
        frame_bytes: 376,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 15159,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 96,
        frame_bytes: 560,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 15159,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 128,
        frame_bytes: 744,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 15159,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 160,
        frame_bytes: 936,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 16537,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 192,
        frame_bytes: 1120,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 17915,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 256,
        frame_bytes: 1488,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 22050,
        mode_a: 3,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 320,
        frame_bytes: 1864,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 22050,
        mode_a: 2,
        mode_b: 2,
    },
    StereoEncodeSetting {
        bitrate_kbps: 352,
        frame_bytes: 2048,
        sample_rate: 44_100,
        channel_mode: 2,
        bandwidth_hz: 22050,
        mode_a: 2,
        mode_b: 2,
    },
];

/// Exact-row-match lookup keyed on `(frame_bytes, sample_rate, channel_mode)`,
/// mirroring the native `atx_set_config_info` init match. `None` means no stereo
/// row matches that key.
pub fn stereo_setting_by_row_match(
    frame_bytes: u32,
    sample_rate: u32,
    channel_mode: u32,
) -> Option<StereoEncodeSetting> {
    STEREO_ENCODE_SETTINGS.iter().copied().find(|row| {
        row.frame_bytes == frame_bytes
            && row.sample_rate == sample_rate
            && row.channel_mode == channel_mode
    })
}

// ===========================================================================
// docs/14 §0.1 / §2.1 — the five 44.1 kHz MONO rows of `g_a_encode_setting_atx`
// (library indices 0-4), dumped from the same .rodata table.
// ===========================================================================

/// The five 44.1 kHz mono rows of `g_a_encode_setting_atx` (library indices
/// 0-4), ordered by ascending bitrate. Values are the direct .rodata dump
/// (libatrac.so.1.2.0 sha1 5f8d118d…, nm 0x88ee0, docs/14 §2.1 evidence B).
///
/// `mode_a == 1` for every mono row (stereo rows carry 3 at 48-256 / 2 at
/// 320-352): the zeroth `param_5` joint/intensity-stereo producer arm is keyed
/// on `mode_a` and NEVER stores 3 at mono, so the whole joint-stereo machinery
/// (docs/13 §2.3) is structurally DEAD at mono (docs/14 §2.2). `mode_b == 0`
/// (stereo rows carry 2): no known consumer in the stereo program — recorded
/// only, watched per mono sweep (docs/14 Appendix B row Q-M-modeb). Bandwidths
/// are the `atx_init_encode_block` bandwidth argument (mono 48/64 match their
/// stereo siblings; 96 = stereo-160's 16537; 128 = full 22050; 32 = a NEW
/// sub-26 extent, docs/14 §2.2/Appendix A).
pub const MONO_ENCODE_SETTINGS: [EncodeSetting; 5] = [
    EncodeSetting {
        bitrate_kbps: 32,
        frame_bytes: 192,
        sample_rate: 44_100,
        channel_mode: 1,
        bandwidth_hz: 11025,
        mode_a: 1,
        mode_b: 0,
    },
    EncodeSetting {
        bitrate_kbps: 48,
        frame_bytes: 280,
        sample_rate: 44_100,
        channel_mode: 1,
        bandwidth_hz: 13781,
        mode_a: 1,
        mode_b: 0,
    },
    EncodeSetting {
        bitrate_kbps: 64,
        frame_bytes: 376,
        sample_rate: 44_100,
        channel_mode: 1,
        bandwidth_hz: 15159,
        mode_a: 1,
        mode_b: 0,
    },
    EncodeSetting {
        bitrate_kbps: 96,
        frame_bytes: 560,
        sample_rate: 44_100,
        channel_mode: 1,
        bandwidth_hz: 16537,
        mode_a: 1,
        mode_b: 0,
    },
    EncodeSetting {
        bitrate_kbps: 128,
        frame_bytes: 744,
        sample_rate: 44_100,
        channel_mode: 1,
        bandwidth_hz: 22050,
        mode_a: 1,
        mode_b: 0,
    },
];

/// Exact-row-match lookup over the five mono rows keyed on
/// `(frame_bytes, sample_rate, channel_mode)`, mirroring the native
/// `atx_set_config_info` init match. `channel_mode` is load-bearing: mono
/// 48/64/96/128 share `frame_bytes` with their stereo siblings, so only the
/// channel mode (1 for mono, 2 for stereo — see [`stereo_setting_by_row_match`])
/// disambiguates them. `None` means no mono row matches that key.
pub fn mono_setting_by_row_match(
    frame_bytes: u32,
    sample_rate: u32,
    channel_mode: u32,
) -> Option<EncodeSetting> {
    MONO_ENCODE_SETTINGS.iter().copied().find(|row| {
        row.frame_bytes == frame_bytes
            && row.sample_rate == sample_rate
            && row.channel_mode == channel_mode
    })
}
