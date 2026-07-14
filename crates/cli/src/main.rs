mod args;
mod at3;
mod at3p;
mod output;
mod pcm;
mod progress;

use std::process::ExitCode;

use args::{AT3_BITRATES_KBPS, AT3P_BITRATES_KBPS, Cli, Codec, Command};

fn main() -> ExitCode {
    match run(Cli::parse_env()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let Command::Encode(args) = cli.command;
    match Codec::for_bitrate(args.bitrate) {
        Some(Codec::At3) => at3::run(args).map_err(|error| error.to_string()),
        Some(Codec::At3p) => at3p::run(args),
        None => Err(format!(
            "unsupported bitrate {} kbps; supported ATRAC3 rates are {} and supported ATRAC3plus rates are {}",
            args.bitrate,
            bitrate_list(&AT3_BITRATES_KBPS),
            bitrate_list(&AT3P_BITRATES_KBPS),
        )),
    }
}

fn bitrate_list(bitrates: &[u32]) -> String {
    bitrates
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
