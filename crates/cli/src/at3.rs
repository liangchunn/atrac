use std::io::Write;
use std::path::Path;

use at3::encoder::stream::{Atrac3StreamConfig, Atrac3StreamEncoder, PCM_BLOCK_FRAMES};
use hound::SampleFormat;

use crate::args::EncodeArgs;
use crate::output::create_pending_output;
use crate::pcm::PcmWaveStream;

pub fn run(args: EncodeArgs) -> anyhow::Result<()> {
    encode(args.bitrate, &args.input, &args.output)
}

fn encode(bitrate: u32, input: &Path, output: &Path) -> anyhow::Result<()> {
    let mut reader = PcmWaveStream::open(input)?;
    let metadata = reader.metadata();
    let spec = metadata.spec;
    anyhow::ensure!(
        spec.sample_rate == 44_100,
        "sample rate must be 44100 Hz, got {}",
        spec.sample_rate
    );
    anyhow::ensure!(
        spec.bits_per_sample == 16,
        "bits per sample must be 16, got {}",
        spec.bits_per_sample
    );
    anyhow::ensure!(
        spec.sample_format == SampleFormat::Int,
        "sample format must be integer PCM"
    );
    anyhow::ensure!(
        spec.channels == 1 || spec.channels == 2,
        "channels must be 1 or 2, got {}",
        spec.channels
    );
    let total_pcm = u64::from(metadata.sample_frames) * u64::from(spec.channels);
    anyhow::ensure!(
        metadata.sample_frames > 0,
        "not enough samples: need at least one sample, got {total_pcm}"
    );
    anyhow::ensure!(
        matches!((spec.channels, bitrate), (1, 52 | 66) | (2, 66 | 105 | 132)),
        "unsupported ATRAC3 bitrate/channel combination: {bitrate} kbps, {} channel(s); supported mono rates are 52 and 66 kbps, supported stereo rates are 66, 105, and 132 kbps",
        spec.channels
    );

    eprintln!("encoding {total_pcm} samples at {bitrate} kbps...");
    let (file, pending) = create_pending_output(output, "at3").map_err(anyhow::Error::msg)?;
    let mut encoder = Atrac3StreamEncoder::new(
        file,
        Atrac3StreamConfig {
            bitrate_kbps: bitrate,
            channels: spec.channels,
        },
        metadata.sample_frames,
    )?;
    if let Some(trace_dir) = std::env::var_os("ATRAC3_PRODUCTION_TRACE_DIR") {
        let max_trace_frames =
            if let Some(value) = std::env::var_os("ATRAC3_PRODUCTION_TRACE_MAX_FRAMES") {
                Some(value.to_string_lossy().parse::<u32>().map_err(|error| {
                    anyhow::anyhow!("invalid ATRAC3_PRODUCTION_TRACE_MAX_FRAMES: {error}")
                })?)
            } else {
                None
            };
        encoder.enable_production_trace_with_max_frames(trace_dir, max_trace_frames)?;
    }

    let mut blocks: Vec<Vec<i16>> = (0..spec.channels)
        .map(|_| Vec::with_capacity(PCM_BLOCK_FRAMES))
        .collect();
    while reader.read_block(&mut blocks, PCM_BLOCK_FRAMES)? != 0 {
        match spec.channels {
            1 => encoder.push_pcm(&[blocks[0].as_slice()])?,
            2 => encoder.push_pcm(&[blocks[0].as_slice(), blocks[1].as_slice()])?,
            _ => unreachable!("validated ATRAC3 channel count"),
        }
    }
    drop(reader);
    let (mut file, summary) = encoder.finish()?;
    file.flush().map_err(|error| {
        anyhow::anyhow!(
            "failed to flush temporary output for `{}`: {error}",
            output.display()
        )
    })?;
    drop(file);
    pending.commit(output).map_err(|error| {
        anyhow::anyhow!(
            "failed to replace output `{}` with completed temporary file: {error}",
            output.display()
        )
    })?;

    eprintln!(
        "wrote {} bytes ({} encoded, {} fallback, {} total) to {}",
        summary.file_bytes,
        summary.encoded_frames,
        summary.fallback_frames,
        summary.encoded_frames + summary.fallback_frames,
        output.display(),
    );
    eprintln!(
        "diagnostics: {} tone drops, {} BFU trims, {} channel rejects",
        summary.diagnostics.tone_payload_drop_events,
        summary.diagnostics.bfu_idwl_decrement_events,
        summary.diagnostics.channel_encode_reject_events,
    );
    Ok(())
}
