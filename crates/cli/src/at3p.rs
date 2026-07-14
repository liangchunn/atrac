use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use at3p::container::{RiffReadError, inspect_target_pcm_wave_for_channels, inspect_wave_format};
use at3p::{
    ATRAC3PLUS_MONO_PROFILES, ATRAC3PLUS_STEREO_PROFILES, Atrac3plusEncoder, Atrac3plusProfile,
    EncodeError, PCM_BLOCK_FRAMES, profile_by_bitrate_and_channels,
};

use crate::args::EncodeArgs;
use crate::output::create_pending_output;
use crate::pcm::PcmWaveStream;
use crate::progress::CliProgress;

pub fn run(command: EncodeArgs) -> Result<(), String> {
    // Preserve the native-style validation precedence: permissive format/channel
    // peek, profile resolution, then strict channel-aware PCM validation.
    let mut input = File::open(&command.input).map_err(|err| {
        format!(
            "failed to read input WAV `{}`: {err}",
            command.input.display()
        )
    })?;
    let format = inspect_wave_format(&mut input)
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
    let info = inspect_target_pcm_wave_for_channels(&mut input, channels)
        .map_err(|err| format!("unsupported input WAV: {}", describe_riff_error(&err)))?;
    input.seek(SeekFrom::Start(0)).map_err(|error| {
        format!(
            "failed to rewind input WAV `{}`: {error}",
            command.input.display()
        )
    })?;
    let pcm = PcmWaveStream::from_file(input)
        .map_err(|error| format!("unsupported input WAV: {error}"))?;
    pcm.validate_strict_info(&info)
        .map_err(|error| format!("unsupported input WAV: {error}"))?;
    encode_computed(&profile, pcm, &command.output)
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
    ATRAC3PLUS_STEREO_PROFILES
        .iter()
        .map(|profile| profile.bitrate_kbps().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn mono_bitrates_list() -> String {
    ATRAC3PLUS_MONO_PROFILES
        .iter()
        .map(|profile| profile.bitrate_kbps().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn encode_computed(
    profile: &Atrac3plusProfile,
    mut pcm: PcmWaveStream,
    output: &Path,
) -> Result<(), String> {
    let input_sample_frames = pcm.metadata().sample_frames;
    let (file, pending) = create_pending_output(output, "at3p")?;
    let mut progress = CliProgress::new();
    let mut encoder = Atrac3plusEncoder::new(file, profile, input_sample_frames)
        .map_err(|error| describe_computed_write_error(output, &error))?;
    let mut blocks: Vec<Vec<i16>> = (0..profile.channels())
        .map(|_| Vec::with_capacity(PCM_BLOCK_FRAMES))
        .collect();
    loop {
        let frames = match pcm.read_block(&mut blocks, PCM_BLOCK_FRAMES) {
            Ok(frames) => frames,
            Err(error) => {
                progress.finish();
                return Err(format!("failed to read input WAV: {error}"));
            }
        };
        if frames == 0 {
            break;
        }
        let result = match profile.channels() {
            1 => encoder
                .push_pcm_with_progress(&[blocks[0].as_slice()], |update| progress.update(update)),
            2 => encoder
                .push_pcm_with_progress(&[blocks[0].as_slice(), blocks[1].as_slice()], |update| {
                    progress.update(update)
                }),
            channels => unreachable!("validated ATRAC3plus profile has {channels} channels"),
        };
        if let Err(error) = result {
            progress.finish();
            return Err(describe_computed_write_error(output, &error));
        }
    }
    drop(pcm);
    let (mut file, summary) = encoder
        .finish_with_progress(|update| progress.update(update))
        .map_err(|error| describe_computed_write_error(output, &error))?;
    progress.finish();
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
    })?;

    eprintln!(
        "wrote {} bytes ({} frames) to {}",
        summary.file_bytes,
        summary.output_frames,
        output.display(),
    );
    Ok(())
}

fn describe_computed_write_error(output: &Path, error: &EncodeError) -> String {
    format!(
        "failed to encode temporary output for `{}`: {error}",
        output.display()
    )
}

fn describe_riff_error(error: &RiffReadError) -> String {
    match error {
        RiffReadError::Io(kind) => format!("I/O error while reading WAV header: {kind:?}"),
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
