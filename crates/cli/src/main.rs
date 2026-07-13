mod at3;
mod at3p;
mod output;
mod pcm;

use std::ffi::{OsStr, OsString};
use std::process::ExitCode;

const USAGE: &str = "usage: atrac <at3|at3p> encode -b <kbps> <input.wav> <output.at3>";

fn main() -> ExitCode {
    match run(std::env::args_os().collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<(), String> {
    if args.len() < 2 {
        return Err(USAGE.to_owned());
    }

    let codec = args[1].clone();
    let mut codec_args = Vec::with_capacity(args.len() - 1);
    codec_args.push(args[0].clone());
    codec_args.extend(args.into_iter().skip(2));

    if codec == OsStr::new("at3") {
        at3::run_args(&codec_args).map_err(|error| error.to_string())
    } else if codec == OsStr::new("at3p") {
        at3p::run_args(&codec_args)
    } else {
        Err(format!(
            "unsupported codec `{}`; expected `at3` or `at3p`\n{USAGE}",
            codec.to_string_lossy()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::run;
    use std::ffi::OsString;

    #[test]
    fn missing_codec_reports_usage() {
        let error = run(vec![OsString::from("atrac")]).unwrap_err();
        assert!(error.contains("usage: atrac"));
    }

    #[test]
    fn unknown_codec_is_rejected() {
        let error = run(vec![OsString::from("atrac"), OsString::from("unknown")]).unwrap_err();
        assert!(error.contains("unsupported codec `unknown`"));
    }
}
