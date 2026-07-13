use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "atrac",
    version,
    about = "ATRAC3 and ATRAC3plus encoder; the codec is selected by bitrate",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn parse_env() -> Self {
        Self::parse()
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Encode 16-bit 44.1 kHz PCM WAV.
    Encode(EncodeArgs),
}

#[derive(Debug, Args)]
pub struct EncodeArgs {
    /// Bitrate in kbps.
    #[arg(short = 'b', long)]
    pub bitrate: u32,
    /// Input WAV file (16-bit, 44.1 kHz, mono or stereo).
    pub input: PathBuf,
    /// Output ATRAC WAV file.
    pub output: PathBuf,
}

pub const AT3_BITRATES_KBPS: [u32; 4] = [52, 66, 105, 132];
pub const AT3P_BITRATES_KBPS: [u32; 10] = [32, 48, 64, 96, 128, 160, 192, 256, 320, 352];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Codec {
    At3,
    At3p,
}

impl Codec {
    pub fn for_bitrate(bitrate: u32) -> Option<Self> {
        if AT3_BITRATES_KBPS.contains(&bitrate) {
            Some(Self::At3)
        } else if AT3P_BITRATES_KBPS.contains(&bitrate) {
            Some(Self::At3p)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::{CommandFactory, Parser};

    use super::{AT3_BITRATES_KBPS, AT3P_BITRATES_KBPS, Cli, Codec, Command};

    #[test]
    fn command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_the_flattened_encode_command() {
        let cli = Cli::try_parse_from([
            "atrac",
            "encode",
            "--bitrate",
            "192",
            "input.wav",
            "output.wav",
        ])
        .unwrap();

        let Command::Encode(args) = cli.command;
        assert_eq!(args.bitrate, 192);
        assert_eq!(args.input, PathBuf::from("input.wav"));
        assert_eq!(args.output, PathBuf::from("output.wav"));
    }

    #[test]
    fn bitrate_selects_the_codec() {
        for bitrate in AT3_BITRATES_KBPS {
            assert_eq!(Codec::for_bitrate(bitrate), Some(Codec::At3));
        }
        for bitrate in AT3P_BITRATES_KBPS {
            assert_eq!(Codec::for_bitrate(bitrate), Some(Codec::At3p));
        }
        assert_eq!(Codec::for_bitrate(384), None);
    }

    #[test]
    fn codec_bitrate_sets_do_not_overlap() {
        for bitrate in AT3_BITRATES_KBPS {
            assert!(
                !AT3P_BITRATES_KBPS.contains(&bitrate),
                "{bitrate} kbps belongs to both codecs"
            );
        }
    }

    #[test]
    fn encode_requires_an_explicit_bitrate() {
        let error =
            Cli::try_parse_from(["atrac", "encode", "input.wav", "output.wav"]).unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }
}
