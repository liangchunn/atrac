use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use at3p::encoder::payload::{
    ComputedFileError, ComputedWriteError, EncodePhase, EncodeProgress,
    write_computed_atracx_file_for_mono_profile_with_progress,
    write_computed_atracx_file_for_profile_with_progress,
};
use at3p::encoder::profile::{EncodeProfile, profile_by_bitrate_and_channels};
use at3p::riff::read::{
    PcmChannels, RiffReadError, load_target_pcm_wave_for_channels, parse_wave_format,
};

const USAGE: &str = "usage: atrac at3p encode -b <kbps> <input.wav> <output.wav>";
const TEMP_CREATE_ATTEMPTS: u64 = 128;
static TEMP_OUTPUT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The nine native ATRAC3plus stereo 44.1 kHz bitrates (gAtracCodecParam stereo
/// rows 10-18). All nine encode end-to-end via the computed pipeline.
const SUPPORTED_BITRATES_KBPS: [u32; 9] = [48, 64, 96, 128, 160, 192, 256, 320, 352];

/// The five native ATRAC3plus MONO 44.1 kHz bitrates (gAtracCodecParam rows
/// 5-9, docs/14 §2.1). All five are accepted for MONO input at the config
/// layer. 128 kbps (docs/14 §1.3), 96 kbps (docs/14 §2.1), 64 kbps
/// (docs/14 §3.1), and 48 kbps (docs/14 §4.1) mono are LANDED: each encodes
/// end-to-end through the computed pipeline and writes output. 32 kbps is also
/// landed (docs/14 §5.1), closing all five native mono rows.
const SUPPORTED_MONO_BITRATES_KBPS: [u32; 5] = [32, 48, 64, 96, 128];

/// A parsed CLI invocation, BEFORE channel-aware profile resolution. The bitrate
/// is validated numerically; the profile is resolved in [`run`] once the input's
/// channel count is known (native at3tool matches `getAtracEncodeSetting` on
/// `(bitrate, channels, sample_rate)`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodeCommand {
    bitrate: u32,
    input: PathBuf,
    output: PathBuf,
}

/// Interactive CLI renderer for the library's exact native-schedule progress.
/// Redirected stderr stays clean; callers embedding the library receive every
/// update directly through the callback-enabled assembly entry points.
struct CliProgress {
    enabled: bool,
    active_line: bool,
}

impl CliProgress {
    fn new() -> Self {
        Self {
            enabled: io::stderr().is_terminal(),
            active_line: false,
        }
    }

    fn update(&mut self, progress: EncodeProgress) {
        if !self.enabled {
            return;
        }

        let phase = match progress.phase {
            EncodePhase::Encoding => "Encoding",
            EncodePhase::Flushing => "Flushing",
        };
        let percent = f64::from(progress.completed_steps) * 100.0 / f64::from(progress.total_steps);
        let mut stderr = io::stderr().lock();
        let _ = write!(
            stderr,
            "\r{phase}: {percent:5.1}% ({}/{}) - {}/{} output frames",
            progress.completed_steps,
            progress.total_steps,
            progress.completed_output_frames,
            progress.total_output_frames,
        );
        let _ = stderr.flush();
        self.active_line = true;
    }

    fn finish(&mut self) {
        if self.active_line {
            eprintln!();
            self.active_line = false;
        }
    }
}

pub fn run_args(args: &[OsString]) -> Result<(), String> {
    let command = parse_args(&args)?;

    // Read the input once and peek its channel count WITHOUT the strict
    // stereo-only gates, then resolve the profile by (bitrate, channels) —
    // mirroring the native at3tool `getAtracEncodeSetting` match (docs/14 §0.1).
    let input = fs::read(&command.input).map_err(|err| {
        format!(
            "failed to read input WAV `{}`: {err}",
            command.input.display()
        )
    })?;
    let format = parse_wave_format(&input)
        .map_err(|err| format!("unsupported input WAV: {}", describe_riff_error(&err)))?;
    let channels = match format.channels {
        1 | 2 => format.channels,
        other => {
            return Err(format!(
                "unsupported input WAV: {}",
                describe_riff_error(&RiffReadError::UnsupportedChannelCount(other))
            ));
        }
    };
    let profile = profile_by_bitrate_and_channels(command.bitrate, channels)
        .ok_or_else(|| classify_rejected_bitrate(command.bitrate, channels))?;

    let pcm = load_target_pcm_wave_for_channels(&input, channels)
        .map_err(|err| format!("unsupported input WAV: {}", describe_riff_error(&err)))?;
    drop(input);
    encode_computed(&profile, pcm, &command.output)
}

fn parse_args(args: &[OsString]) -> Result<EncodeCommand, String> {
    if args.len() < 2 {
        return Err(USAGE.to_owned());
    }

    if args[1].as_os_str() != OsStr::new("encode") {
        return Err(format!(
            "unsupported mode `{}`; only `encode` is supported\n{USAGE}",
            display_arg(&args[1])
        ));
    }

    if args.len() != 6 {
        return Err(format!("malformed encode command\n{USAGE}"));
    }

    if args[2].as_os_str() != OsStr::new("-b") && args[2].as_os_str() != OsStr::new("--bitrate") {
        return Err(format!(
            "unsupported encode options; expected `-b <kbps>`\n{USAGE}"
        ));
    }

    let bitrate_text = args[3]
        .to_str()
        .ok_or_else(|| format!("invalid bitrate: expected UTF-8 numeric kbps value\n{USAGE}"))?;
    let bitrate = bitrate_text.parse::<u32>().map_err(|_| {
        format!("invalid bitrate `{bitrate_text}`: expected numeric kbps value\n{USAGE}")
    })?;

    // Bitrate is validated numerically here; the channel-aware profile match
    // happens in `run` once the input's channel count is known.
    Ok(EncodeCommand {
        bitrate,
        input: PathBuf::from(&args[4]),
        output: PathBuf::from(&args[5]),
    })
}

/// Classify a `(bitrate, channels)` pair with no gAtracCodecParam ATRAC3plus row
/// into a native-fact-carrying error message, mirroring at3tool
/// `getAtracEncodeSetting`'s `(bitrate, channels, sample_rate)` match miss
/// ("Not Supported Param", exit 1). `channels` is the peeked input channel count
/// (1 or 2), so the message reflects what the native tool would say for THAT
/// input shape (measured sweep, docs/14 §2 evidence C).
///
/// - 52/66 kbps (either channel count) → ATRAC3 (non-plus) codec family
///   (gAtracCodecParam codec_kind 3, 1024 samples/frame); native at3tool encodes
///   52/66 mono as ATRAC3, a different codec — out of scope for this crate.
/// - mono input, any other unmatched rate (160/192/256/320/352, 384/24/16/105/
///   132, …) → no `(bitrate, 1 ch, 44100)` mono row; list the five mono rates.
/// - stereo input → the stereo classes (32 kbps mono-only; 105/132 ATRAC3;
///   else the nine stereo rates), see [`classify_rejected_stereo_bitrate`].
fn classify_rejected_bitrate(bitrate: u32, channels: u16) -> String {
    // 52/66 exist natively at BOTH channel counts but only as ATRAC3 (non-plus):
    // a different codec family, out of scope regardless of channels (measured
    // sweep: native encodes 52/66 mono as ATRAC3 non-plus, docs/14 §2 evidence C).
    if matches!(bitrate, 52 | 66) {
        return atrac3_family_message(bitrate);
    }
    match channels {
        1 => format!(
            "unsupported bitrate {bitrate} kbps for mono input: no native ATRAC3plus \
             (bitrate {bitrate}, 1 ch, 44100 Hz) gAtracCodecParam row (native at3tool prints \
             \"Not Supported Param\", exit 1); supported ATRAC3plus mono rates are {}",
            mono_bitrates_list()
        ),
        _ => classify_rejected_stereo_bitrate(bitrate),
    }
}

/// The stereo-input rejection classes (unchanged native facts; the 32 kbps
/// wording is refreshed to the docs/14 reality).
fn classify_rejected_stereo_bitrate(bitrate: u32) -> String {
    match bitrate {
        32 => format!(
            "unsupported bitrate 32 kbps for stereo input: ATRAC3plus 32 kbps exists only as a \
             mono row (native gAtracCodecParam has no 32 kbps stereo row); a stereo input at 32 \
             kbps is a native reject (measured docs/13 sweep). ATRAC3plus mono rates \
             32/48/64/96/128 kbps are supported for mono input; supported stereo rates are {}",
            supported_bitrates_list()
        ),
        52 | 66 | 105 | 132 => atrac3_family_message(bitrate),
        _ => format!(
            "unsupported bitrate {bitrate} kbps; supported ATRAC3plus stereo rates are {}",
            supported_bitrates_list()
        ),
    }
}

fn atrac3_family_message(bitrate: u32) -> String {
    format!(
        "unsupported bitrate {bitrate} kbps: {bitrate} kbps is an ATRAC3 (non-plus) rate \
         (native gAtracCodecParam codec_kind 3, 1024 samples/frame), a different codec \
         family; out of scope"
    )
}

fn supported_bitrates_list() -> String {
    SUPPORTED_BITRATES_KBPS
        .iter()
        .map(|rate| rate.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn mono_bitrates_list() -> String {
    SUPPORTED_MONO_BITRATES_KBPS
        .iter()
        .map(|rate| rate.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn encode_computed(profile: &EncodeProfile, pcm: PcmChannels, output: &Path) -> Result<(), String> {
    // The raw WAV has already been released, but the strict decoder's one
    // complete channel-aware i16 carrier remains retained. Streaming input is
    // a separate RIFF-reader boundary; docs/16 S5 removes compressed-output
    // buffering without claiming bounded input memory.
    let (mut file, pending) = create_pending_output(output)?;
    let mut progress = CliProgress::new();
    let result = match profile.channels {
        1 => write_computed_atracx_file_for_mono_profile_with_progress(
            &mut file,
            profile,
            pcm.info.sample_frames,
            pcm.channels(),
            |update| progress.update(update),
        ),
        2 => write_computed_atracx_file_for_profile_with_progress(
            &mut file,
            profile,
            pcm.info.sample_frames,
            pcm.channel(0).expect("strict stereo decode has channel 0"),
            pcm.channel(1).expect("strict stereo decode has channel 1"),
            |update| progress.update(update),
        ),
        channels => unreachable!("validated ATRAC3plus profile has {channels} channels"),
    };
    progress.finish();
    if let Err(error) = result {
        drop(file);
        return Err(describe_computed_write_error(output, &error));
    }

    if let Err(error) = file.flush() {
        drop(file);
        return Err(format!(
            "failed to flush temporary output for `{}`: {error}",
            output.display()
        ));
    }
    drop(file);

    pending.commit(output).map_err(|error| {
        format!(
            "failed to replace output `{}` with completed temporary file: {error}",
            output.display()
        )
    })
}

struct PendingOutput {
    path: PathBuf,
    committed: bool,
}

impl PendingOutput {
    fn commit(mut self, output: &Path) -> io::Result<()> {
        fs::rename(&self.path, output)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_pending_output(output: &Path) -> Result<(File, PendingOutput), String> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let output_name = output.file_name().unwrap_or_else(|| OsStr::new("output"));

    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let sequence = TEMP_OUTPUT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(".");
        temp_name.push(output_name);
        temp_name.push(format!(".at3p-{}-{sequence}.tmp", std::process::id()));
        let path = parent.join(temp_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                return Ok((
                    file,
                    PendingOutput {
                        path,
                        committed: false,
                    },
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create temporary output beside `{}`: {error}",
                    output.display()
                ));
            }
        }
    }

    Err(format!(
        "failed to create a unique temporary output beside `{}` after {TEMP_CREATE_ATTEMPTS} attempts",
        output.display()
    ))
}

fn describe_computed_write_error(output: &Path, error: &ComputedWriteError) -> String {
    match error {
        ComputedWriteError::File(error) => format!(
            "failed to assemble computed ATRACX file: {}",
            describe_computed_file_error(error)
        ),
        ComputedWriteError::Io { stage, source } => format!(
            "failed to write temporary output for `{}` at {stage:?}: {source}",
            output.display()
        ),
    }
}

fn describe_computed_file_error(error: &ComputedFileError) -> String {
    match error {
        ComputedFileError::InputTooShort {
            input_sample_frames,
            minimum,
        } => format!(
            "too short input file: {input_sample_frames} sample frames; the native encoder \
             requires at least {minimum} (native at3tool checkEncodeParam rejection)"
        ),
        ComputedFileError::UnsupportedInputShape {
            expected_sample_frames,
            actual_sample_frames,
            left_len,
            right_len,
        } => format!(
            "unsupported input shape: expected {expected_sample_frames} sample frames with matching \
             left/right channels, got {actual_sample_frames} sample frames (left {left_len}, right {right_len})"
        ),
        ComputedFileError::UnsupportedMonoInputShape {
            expected_sample_frames,
            channel_count,
            channel_len,
        } => format!(
            "unsupported mono input shape: expected 1 channel of {expected_sample_frames} sample \
             frames, got {channel_count} channel(s) (first channel {channel_len})"
        ),
        ComputedFileError::UnsupportedProfile { bitrate_kbps } => format!(
            "unsupported bitrate {bitrate_kbps} kbps: no native ATRAC3plus stereo profile exists"
        ),
        other => format!("{other:?}"),
    }
}

fn describe_riff_error(error: &RiffReadError) -> String {
    match error {
        RiffReadError::TooShort => "file is too short to contain a RIFF/WAVE header".to_owned(),
        RiffReadError::InvalidRiffId(id) => {
            format!("expected RIFF chunk id, found `{}`", fourcc_text(id))
        }
        RiffReadError::InvalidWaveForm(form) => {
            format!("expected WAVE form, found `{}`", fourcc_text(form))
        }
        RiffReadError::TruncatedChunkHeader { offset } => {
            format!("chunk header at byte {offset} is truncated")
        }
        RiffReadError::TruncatedChunkPayload { offset, size } => {
            format!("chunk payload at byte {offset} with size {size} is truncated")
        }
        RiffReadError::ChunkOffsetOverflow { offset, size } => {
            format!("chunk at byte {offset} with size {size} overflows the file")
        }
        RiffReadError::MissingFmtChunk => "missing `fmt ` chunk".to_owned(),
        RiffReadError::MissingDataChunk => "missing `data` chunk".to_owned(),
        RiffReadError::FmtChunkTooShort { size } => {
            format!("`fmt ` chunk is too short: expected at least 16 bytes, got {size}")
        }
        RiffReadError::UnsupportedFormatTag(tag) => {
            format!("unsupported WAV format tag {tag}; expected PCM format tag 1")
        }
        RiffReadError::UnsupportedChannelCount(channels) => {
            format!("unsupported channel count {channels}; expected mono (1) or stereo (2)")
        }
        RiffReadError::UnsupportedSampleRate(rate) => {
            format!("unsupported sample rate {rate} Hz; expected 44100 Hz")
        }
        RiffReadError::UnsupportedBlockAlign(block_align) => {
            format!("unsupported block align {block_align}; expected 4 bytes")
        }
        RiffReadError::UnsupportedBitsPerSample(bits) => {
            format!("unsupported bits per sample {bits}; expected 16")
        }
        RiffReadError::DataSizeNotAligned {
            data_size,
            block_align,
        } => format!("data chunk size {data_size} is not aligned to block align {block_align}"),
        RiffReadError::SampleDataTooLarge(size) => {
            format!("sample data is too large to load on this platform: {size} bytes")
        }
    }
}

fn fourcc_text(id: &[u8; 4]) -> String {
    id.iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect()
}

fn display_arg(arg: &OsString) -> String {
    arg.to_string_lossy().into_owned()
}
