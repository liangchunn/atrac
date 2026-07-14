//! Validated ATRAC3plus profiles for the fourteen 44.1 kHz rows reachable
//! through the native tool: five mono and nine stereo.
//!
//! Each value merges the corresponding `gAtracCodecParam` driver row with the
//! `g_a_encode_setting_atx` row facts used by the computed coding pipeline.

use std::fmt;

/// The channel layout encoded by an ATRAC3plus profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    Mono,
    Stereo,
}

impl ChannelMode {
    pub const fn channels(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }

    pub(crate) const fn native_selector(self) -> u32 {
        self.channels() as u32
    }
}

/// A rejected ATRAC3plus bitrate/channel tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedProfile {
    bitrate_kbps: u32,
    channels: u16,
}

impl UnsupportedProfile {
    pub const fn bitrate_kbps(self) -> u32 {
        self.bitrate_kbps
    }

    pub const fn channels(self) -> u16 {
        self.channels
    }
}

impl fmt::Display for UnsupportedProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported ATRAC3plus bitrate/channel combination: {} kbps, {} channel(s)",
            self.bitrate_kbps, self.channels
        )
    }
}

impl std::error::Error for UnsupportedProfile {}

/// A complete, immutable ATRAC3plus encoding profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atrac3plusProfile {
    bitrate_kbps: u32,
    channel_mode: ChannelMode,
    frame_bytes: u32,
    codec_info: u32,
    bandwidth_hz: u32,
    mode_a: u32,
}

impl Atrac3plusProfile {
    pub const SAMPLE_RATE: u32 = 44_100;
    pub const FRAME_SAMPLES: u32 = 2048;

    /// Validate the complete native `(bitrate, channels, sample_rate)` tuple.
    pub const fn new(bitrate_kbps: u32, channels: u16) -> Result<Self, UnsupportedProfile> {
        let profile = match (channels, bitrate_kbps) {
            (1, 32) => ATRAC3PLUS_MONO_32,
            (1, 48) => ATRAC3PLUS_MONO_48,
            (1, 64) => ATRAC3PLUS_MONO_64,
            (1, 96) => ATRAC3PLUS_MONO_96,
            (1, 128) => ATRAC3PLUS_MONO_128,
            (2, 48) => ATRAC3PLUS_48,
            (2, 64) => ATRAC3PLUS_64,
            (2, 96) => ATRAC3PLUS_96,
            (2, 128) => ATRAC3PLUS_128,
            (2, 160) => ATRAC3PLUS_160,
            (2, 192) => ATRAC3PLUS_192,
            (2, 256) => ATRAC3PLUS_256,
            (2, 320) => ATRAC3PLUS_320,
            (2, 352) => ATRAC3PLUS_352,
            _ => {
                return Err(UnsupportedProfile {
                    bitrate_kbps,
                    channels,
                });
            }
        };
        Ok(profile)
    }

    const fn mono(bitrate_kbps: u32, frame_bytes: u32, bandwidth_hz: u32) -> Self {
        Self::build(
            bitrate_kbps,
            ChannelMode::Mono,
            frame_bytes,
            bandwidth_hz,
            1,
        )
    }

    const fn stereo(bitrate_kbps: u32, frame_bytes: u32, bandwidth_hz: u32, mode_a: u32) -> Self {
        Self::build(
            bitrate_kbps,
            ChannelMode::Stereo,
            frame_bytes,
            bandwidth_hz,
            mode_a,
        )
    }

    const fn build(
        bitrate_kbps: u32,
        channel_mode: ChannelMode,
        frame_bytes: u32,
        bandwidth_hz: u32,
        mode_a: u32,
    ) -> Self {
        let codec_info = 0x0100_0000
            | (1 << 13)
            | (channel_mode.native_selector() << 10)
            | (frame_bytes / 8 - 1);
        Self {
            bitrate_kbps,
            channel_mode,
            frame_bytes,
            codec_info,
            bandwidth_hz,
            mode_a,
        }
    }

    pub const fn bitrate_kbps(self) -> u32 {
        self.bitrate_kbps
    }

    pub const fn channel_mode(self) -> ChannelMode {
        self.channel_mode
    }

    pub const fn channels(self) -> u16 {
        self.channel_mode.channels()
    }

    pub const fn sample_rate(self) -> u32 {
        Self::SAMPLE_RATE
    }

    pub const fn frame_samples(self) -> u32 {
        Self::FRAME_SAMPLES
    }

    pub const fn frame_bytes(self) -> u32 {
        self.frame_bytes
    }

    pub const fn codec_info(self) -> u32 {
        self.codec_info
    }

    pub(crate) const fn encode_algorithm(self) -> u32 {
        1
    }

    pub(crate) const fn bandwidth_hz(self) -> u32 {
        self.bandwidth_hz
    }

    pub(crate) const fn mode_a(self) -> u32 {
        self.mode_a
    }
}

pub const ATRAC3PLUS_48: Atrac3plusProfile = Atrac3plusProfile::stereo(48, 280, 13_781, 3);
pub const ATRAC3PLUS_64: Atrac3plusProfile = Atrac3plusProfile::stereo(64, 376, 15_159, 3);
pub const ATRAC3PLUS_96: Atrac3plusProfile = Atrac3plusProfile::stereo(96, 560, 15_159, 3);
pub const ATRAC3PLUS_128: Atrac3plusProfile = Atrac3plusProfile::stereo(128, 744, 15_159, 3);
pub const ATRAC3PLUS_160: Atrac3plusProfile = Atrac3plusProfile::stereo(160, 936, 16_537, 3);
pub const ATRAC3PLUS_192: Atrac3plusProfile = Atrac3plusProfile::stereo(192, 1120, 17_915, 3);
pub const ATRAC3PLUS_256: Atrac3plusProfile = Atrac3plusProfile::stereo(256, 1488, 22_050, 3);
pub const ATRAC3PLUS_320: Atrac3plusProfile = Atrac3plusProfile::stereo(320, 1864, 22_050, 2);
pub const ATRAC3PLUS_352: Atrac3plusProfile = Atrac3plusProfile::stereo(352, 2048, 22_050, 2);

pub const ATRAC3PLUS_STEREO_PROFILES: [Atrac3plusProfile; 9] = [
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

pub const ATRAC3PLUS_MONO_32: Atrac3plusProfile = Atrac3plusProfile::mono(32, 192, 11_025);
pub const ATRAC3PLUS_MONO_48: Atrac3plusProfile = Atrac3plusProfile::mono(48, 280, 13_781);
pub const ATRAC3PLUS_MONO_64: Atrac3plusProfile = Atrac3plusProfile::mono(64, 376, 15_159);
pub const ATRAC3PLUS_MONO_96: Atrac3plusProfile = Atrac3plusProfile::mono(96, 560, 16_537);
pub const ATRAC3PLUS_MONO_128: Atrac3plusProfile = Atrac3plusProfile::mono(128, 744, 22_050);

pub const ATRAC3PLUS_MONO_PROFILES: [Atrac3plusProfile; 5] = [
    ATRAC3PLUS_MONO_32,
    ATRAC3PLUS_MONO_48,
    ATRAC3PLUS_MONO_64,
    ATRAC3PLUS_MONO_96,
    ATRAC3PLUS_MONO_128,
];

pub fn stereo_profile_by_bitrate_kbps(bitrate_kbps: u32) -> Option<Atrac3plusProfile> {
    Atrac3plusProfile::new(bitrate_kbps, 2).ok()
}

pub fn stereo_profile_by_frame_bytes(frame_bytes: u32) -> Option<Atrac3plusProfile> {
    ATRAC3PLUS_STEREO_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.frame_bytes() == frame_bytes)
}

pub fn mono_profile_by_bitrate_kbps(bitrate_kbps: u32) -> Option<Atrac3plusProfile> {
    Atrac3plusProfile::new(bitrate_kbps, 1).ok()
}

pub fn mono_profile_by_frame_bytes(frame_bytes: u32) -> Option<Atrac3plusProfile> {
    ATRAC3PLUS_MONO_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.frame_bytes() == frame_bytes)
}

pub fn profile_by_bitrate_and_channels(
    bitrate_kbps: u32,
    channels: u16,
) -> Option<Atrac3plusProfile> {
    Atrac3plusProfile::new(bitrate_kbps, channels).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_matrix_has_consistent_native_fields() {
        for profile in ATRAC3PLUS_MONO_PROFILES
            .iter()
            .chain(ATRAC3PLUS_STEREO_PROFILES.iter())
        {
            assert_eq!(profile.sample_rate(), 44_100);
            assert_eq!(profile.frame_samples(), 2048);
            assert_eq!(profile.encode_algorithm(), 1);
            assert_eq!(
                (profile.codec_info() & 0x3ff) * 8 + 8,
                profile.frame_bytes()
            );
            assert_eq!(
                (profile.codec_info() >> 10) & 7,
                u32::from(profile.channels())
            );
        }
    }

    #[test]
    fn invalid_tuples_have_no_fallback_profile() {
        assert!(Atrac3plusProfile::new(32, 2).is_err());
        assert!(Atrac3plusProfile::new(160, 1).is_err());
        assert!(Atrac3plusProfile::new(352, 3).is_err());
    }
}
