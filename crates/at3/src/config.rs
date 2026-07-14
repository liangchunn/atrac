use std::fmt;

/// The channel layout encoded by an ATRAC3 profile.
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
}

/// A rejected ATRAC3 bitrate/channel tuple.
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
            "unsupported ATRAC3 bitrate/channel combination: {} kbps, {} channel(s); supported mono rates are 52 and 66 kbps, supported stereo rates are 66, 105, and 132 kbps",
            self.bitrate_kbps, self.channels
        )
    }
}

impl std::error::Error for UnsupportedProfile {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncoderStrategy {
    Clean,
    Dba,
}

/// A fully validated ATRAC3 encoding profile.
///
/// Construction validates the complete bitrate/channel tuple and fixes all
/// derived container, core-bitrate, stereo-mode, strategy, and priming facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atrac3Profile {
    bitrate_kbps: u32,
    channel_mode: ChannelMode,
    frame_bytes: usize,
    encoder_bitrate_kbps: u32,
    internal_frame_bytes: usize,
    joint_stereo: bool,
    strategy: EncoderStrategy,
    priming_sound_units: usize,
}

impl Atrac3Profile {
    pub const fn new(bitrate_kbps: u32, channels: u16) -> Result<Self, UnsupportedProfile> {
        let profile = match (channels, bitrate_kbps) {
            (1, 52) => Self::mono(52, 152, 105, 304, EncoderStrategy::Dba, 1),
            (1, 66) => Self::mono(66, 192, 132, 384, EncoderStrategy::Clean, 2),
            (2, 66) => Self::stereo(66, 192, true, EncoderStrategy::Dba, 1),
            (2, 105) => Self::stereo(105, 304, false, EncoderStrategy::Dba, 1),
            (2, 132) => Self::stereo(132, 384, false, EncoderStrategy::Clean, 2),
            _ => {
                return Err(UnsupportedProfile {
                    bitrate_kbps,
                    channels,
                });
            }
        };
        Ok(profile)
    }

    const fn mono(
        bitrate_kbps: u32,
        frame_bytes: usize,
        encoder_bitrate_kbps: u32,
        internal_frame_bytes: usize,
        strategy: EncoderStrategy,
        priming_sound_units: usize,
    ) -> Self {
        Self {
            bitrate_kbps,
            channel_mode: ChannelMode::Mono,
            frame_bytes,
            encoder_bitrate_kbps,
            internal_frame_bytes,
            joint_stereo: false,
            strategy,
            priming_sound_units,
        }
    }

    const fn stereo(
        bitrate_kbps: u32,
        frame_bytes: usize,
        joint_stereo: bool,
        strategy: EncoderStrategy,
        priming_sound_units: usize,
    ) -> Self {
        Self {
            bitrate_kbps,
            channel_mode: ChannelMode::Stereo,
            frame_bytes,
            encoder_bitrate_kbps: bitrate_kbps,
            internal_frame_bytes: frame_bytes,
            joint_stereo,
            strategy,
            priming_sound_units,
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

    pub const fn frame_bytes(self) -> usize {
        self.frame_bytes
    }

    pub const fn is_joint_stereo(self) -> bool {
        self.joint_stereo
    }

    pub(crate) const fn encoder_bitrate_kbps(self) -> u32 {
        self.encoder_bitrate_kbps
    }

    pub(crate) const fn internal_frame_bytes(self) -> usize {
        self.internal_frame_bytes
    }

    pub(crate) const fn strategy(self) -> EncoderStrategy {
        self.strategy
    }

    pub(crate) const fn priming_sound_units(self) -> usize {
        self.priming_sound_units
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_matrix_owns_all_derived_facts() {
        let cases = [
            (52, 1, 152, 105, EncoderStrategy::Dba, 1),
            (66, 1, 192, 132, EncoderStrategy::Clean, 2),
            (66, 2, 192, 66, EncoderStrategy::Dba, 1),
            (105, 2, 304, 105, EncoderStrategy::Dba, 1),
            (132, 2, 384, 132, EncoderStrategy::Clean, 2),
        ];
        for (rate, channels, bytes, encoder_rate, strategy, priming) in cases {
            let profile = Atrac3Profile::new(rate, channels).unwrap();
            assert_eq!(profile.bitrate_kbps(), rate);
            assert_eq!(profile.channels(), channels);
            assert_eq!(profile.frame_bytes(), bytes);
            assert_eq!(profile.encoder_bitrate_kbps(), encoder_rate);
            assert_eq!(profile.strategy(), strategy);
            assert_eq!(profile.priming_sound_units(), priming);
        }
    }

    #[test]
    fn invalid_tuples_have_no_fallback_profile() {
        assert!(Atrac3Profile::new(52, 2).is_err());
        assert!(Atrac3Profile::new(132, 1).is_err());
        assert!(Atrac3Profile::new(66, 3).is_err());
    }
}
