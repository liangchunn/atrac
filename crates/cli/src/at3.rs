use std::io::Write;
use std::path::Path;

use at3::{Atrac3Encoder, Atrac3Profile, PCM_BLOCK_FRAMES};
use hound::SampleFormat;

use crate::args::EncodeArgs;
use crate::output::create_pending_output;
use crate::pcm::PcmWaveStream;
use crate::progress::CliProgress;

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
    let profile = Atrac3Profile::new(bitrate, spec.channels)?;

    eprintln!("encoding {total_pcm} samples at {bitrate} kbps...");
    let (file, pending) = create_pending_output(output, "at3").map_err(anyhow::Error::msg)?;
    let mut encoder = Atrac3Encoder::new(file, profile, metadata.sample_frames)?;
    let mut progress = CliProgress::new();
    let mut blocks: Vec<Vec<i16>> = (0..spec.channels)
        .map(|_| Vec::with_capacity(PCM_BLOCK_FRAMES))
        .collect();
    loop {
        let frames = match reader.read_block(&mut blocks, PCM_BLOCK_FRAMES) {
            Ok(frames) => frames,
            Err(error) => {
                progress.finish();
                return Err(error.into());
            }
        };
        if frames == 0 {
            break;
        }
        let result = match spec.channels {
            1 => encoder
                .push_pcm_with_progress(&[blocks[0].as_slice()], |update| progress.update(update)),
            2 => encoder
                .push_pcm_with_progress(&[blocks[0].as_slice(), blocks[1].as_slice()], |update| {
                    progress.update(update)
                }),
            _ => unreachable!("validated ATRAC3 channel count"),
        };
        if let Err(error) = result {
            progress.finish();
            return Err(error.into());
        }
    }
    drop(reader);
    let (mut file, summary) = match encoder.finish_with_progress(|update| progress.update(update)) {
        Ok(result) => result,
        Err(error) => {
            progress.finish();
            return Err(error.into());
        }
    };
    progress.finish();
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
    Ok(())
}
