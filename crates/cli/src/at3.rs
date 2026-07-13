use clap::Parser;
use hound::WavReader;
use std::ffi::OsString;
use std::fs;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(name = "atrac at3", version, about = "ATRAC3 encoder")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Encode 16-bit 44.1 kHz PCM WAV.
    Encode {
        /// Bitrate in kbps.
        #[arg(short = 'b', long, default_value = "132")]
        bitrate: u32,
        /// Input WAV file (16-bit, 44.1 kHz, stereo).
        input: String,
        /// Output ATRAC3 WAV file.
        output: String,
    },
}

const WAVE_SAMPLE_RATE: u32 = 44_100;
const SAMPLES_PER_FRAME: u32 = 1024;
const ATRAC3_ENCODER_DELAY: usize = 69;
const CLEAN_PRIMING_SOUND_UNITS: usize = 2;
const DBA_PRIMING_SOUND_UNITS: usize = 1;

fn sound_units_to_encode(sample_frames: usize, dba_priming: bool) -> usize {
    let frame = SAMPLES_PER_FRAME as usize;
    if dba_priming {
        // No-loop DBA wrapper model. Sony's at3tool prefill/discard path is more
        // subtle than this count alone: native traces show a hidden
        // (sound_unit - 1) * 1024 + 69 input window, but the wrapper also drops
        // the first nonzero returned frame. Keep this paired with the retained
        // DBA base below unless that second discard is implemented too.
        (sample_frames + frame - ATRAC3_ENCODER_DELAY) / frame + 2
    } else {
        // No-loop clean path: two non-written sound units, with the first input
        // window starting at -955 samples via (sound_unit - 1) * 1024 + 69.
        sample_frames / frame + 2 + CLEAN_PRIMING_SOUND_UNITS
    }
}

fn priming_sound_units(dba_priming: bool) -> usize {
    if dba_priming {
        DBA_PRIMING_SOUND_UNITS
    } else {
        CLEAN_PRIMING_SOUND_UNITS
    }
}

fn write_sound_unit(sound_unit: usize, dba_priming: bool) -> bool {
    sound_unit >= priming_sound_units(dba_priming)
}

fn input_base_frame_for_sound_unit(
    sound_unit: usize,
    dba_priming: bool,
    input_delay: usize,
) -> isize {
    let frame = SAMPLES_PER_FRAME as isize;
    if dba_priming {
        // Retained DBA production baseline. Matching native call 0 would use
        // (sound_unit - 1) * 1024 + 69, but applying that without also modeling
        // at3tool's extra wrapper-level discard regressed decoded metrics.
        sound_unit as isize * frame + input_delay as isize
    } else {
        // Clean production parity follows Sony's delayed preroll schedule.
        (sound_unit as isize - 1) * frame + input_delay as isize
    }
}

fn write_at3_header(
    file: &mut fs::File,
    sample_count: u32,
    frame_size: u32,
    channels: u16,
    joint_stereo: bool,
    total_data_size: u32,
) -> std::io::Result<()> {
    fn push_u16(buf: &mut Vec<u8>, x: u16) {
        buf.extend_from_slice(&x.to_le_bytes());
    }
    fn push_u32(buf: &mut Vec<u8>, x: u32) {
        buf.extend_from_slice(&x.to_le_bytes());
    }

    let header_size: usize = 80;
    let file_size = header_size as u64 + u64::from(total_data_size);
    let mut header = Vec::with_capacity(header_size);

    header.extend_from_slice(b"RIFF");
    push_u32(&mut header, (file_size - 8) as u32);
    header.extend_from_slice(b"WAVE");
    header.extend_from_slice(b"fmt ");
    push_u32(&mut header, 32);

    push_u16(&mut header, 0x0270);
    push_u16(&mut header, channels);
    push_u32(&mut header, WAVE_SAMPLE_RATE);
    let byte_rate = (u64::from(frame_size) * u64::from(WAVE_SAMPLE_RATE))
        .div_ceil(u64::from(SAMPLES_PER_FRAME)) as u32;
    push_u32(&mut header, byte_rate);
    push_u16(&mut header, frame_size as u16);
    push_u16(&mut header, 0);
    push_u16(&mut header, 14);
    push_u16(&mut header, 1);
    push_u32(&mut header, 0x1000);
    let mode = u16::from(joint_stereo);
    push_u16(&mut header, mode);
    push_u16(&mut header, mode);
    push_u16(&mut header, 1);
    push_u16(&mut header, 0);

    header.extend_from_slice(b"fact");
    push_u32(&mut header, 12);
    push_u32(&mut header, sample_count);
    push_u32(&mut header, SAMPLES_PER_FRAME);
    push_u32(&mut header, SAMPLES_PER_FRAME);

    header.extend_from_slice(b"data");
    push_u32(&mut header, total_data_size);

    debug_assert_eq!(header_size, header.len());
    file.write_all(&header)
}

pub fn run_args(args: &[OsString]) -> anyhow::Result<()> {
    let cli = Cli::try_parse_from(args)?;
    match cli.command {
        Some(Commands::Encode {
            bitrate,
            input,
            output,
        }) => {
            let mut reader =
                WavReader::open(&input).map_err(|e| anyhow::anyhow!("failed to open WAV: {e}"))?;
            let spec = reader.spec();
            anyhow::ensure!(
                spec.sample_rate == 44100,
                "sample rate must be 44100 Hz, got {}",
                spec.sample_rate
            );
            anyhow::ensure!(
                spec.bits_per_sample == 16,
                "bits per sample must be 16, got {}",
                spec.bits_per_sample
            );
            anyhow::ensure!(
                spec.channels == 1 || spec.channels == 2,
                "channels must be 1 or 2, got {}",
                spec.channels
            );

            let is_mono_input = spec.channels == 1;
            let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
            let total_pcm = samples.len() as u32;
            let sample_frames = samples.len() / spec.channels as usize;

            anyhow::ensure!(
                sample_frames > 0,
                "not enough samples: need at least one sample, got {total_pcm}"
            );

            anyhow::ensure!(
                matches!((spec.channels, bitrate), (1, 52 | 66) | (2, 66 | 105 | 132)),
                "unsupported ATRAC3 bitrate/channel combination: {bitrate} kbps, {} channel(s); supported mono rates are 52 and 66 kbps, supported stereo rates are 66, 105, and 132 kbps",
                spec.channels
            );

            // Mono bitrates encode internally at double-bitrate stereo, then
            // write only the first half of each frame (L=R for mono input).
            let is_mono_bitrate = bitrate == 52 || (bitrate == 66 && is_mono_input);
            let _ = is_mono_bitrate;
            let (frame_size, encoder_bitrate, joint_stereo, output_channels) = match bitrate {
                132 => (384u32, 132u32, false, 2u16),
                105 => (304, 105, false, 2),
                66 if is_mono_input => (192, 132, false, 1),
                66 => (192, 66, true, 2),
                52 => (152, 105, false, 1),
                _ => (384, 132, false, 2),
            };

            eprintln!("encoding {total_pcm} samples at {bitrate} kbps...");

            let mut encoder = at3::dsp::encode::Atrac3Encoder::new(encoder_bitrate, joint_stereo);
            if let Some(trace_dir) = std::env::var_os("ATRAC3_PRODUCTION_TRACE_DIR") {
                let max_trace_frames =
                    if let Some(value) = std::env::var_os("ATRAC3_PRODUCTION_TRACE_MAX_FRAMES") {
                        Some(value.to_string_lossy().parse::<u32>().map_err(|err| {
                            anyhow::anyhow!("invalid ATRAC3_PRODUCTION_TRACE_MAX_FRAMES: {err}")
                        })?)
                    } else {
                        None
                    };
                encoder.enable_production_trace_with_max_frames(
                    std::path::PathBuf::from(trace_dir),
                    max_trace_frames,
                )?;
            }
            let dba_priming = encoder.enc_algo() == 0;
            let input_delay = ATRAC3_ENCODER_DELAY;
            let num_sound_units = sound_units_to_encode(sample_frames, dba_priming);
            let internal_frame_size = match encoder_bitrate {
                132 => 384usize,
                105 => 304,
                66 => 192,
                _ => 384,
            };
            let mut out_buf = vec![0u8; 48 * 1024];
            let silence_frame = {
                let mut silence_encoder =
                    at3::dsp::encode::Atrac3Encoder::new(encoder_bitrate, joint_stereo);
                let silence0 = [0.0f32; 1024];
                let silence1 = [0.0f32; 1024];
                let silence_refs: [&[f32; 1024]; 2] = [&silence0, &silence1];
                let mut frame = vec![0u8; internal_frame_size];
                let bit_count = silence_encoder.encode_frame(&silence_refs, &mut frame);
                let byte_count = ((bit_count.max(0) as u32 + 7) >> 3) as usize;
                anyhow::ensure!(
                    bit_count >= 0 && byte_count <= internal_frame_size,
                    "failed to build fallback silence frame"
                );
                frame
            };
            let mut frames = Vec::new();
            let mut frames_ok = 0u32;
            let mut fallback_frames = 0u32;

            for su in 0..num_sound_units {
                let write_frame = write_sound_unit(su, dba_priming);
                let requested_channels = encoder.channel_count().max(1);
                let mut trace_input_pcm =
                    Vec::with_capacity(1024 * 2 * usize::from(requested_channels));
                let mut pcm0: [f32; 1024] = [0.0; 1024];
                let mut pcm1: [f32; 1024] = [0.0; 1024];
                let input_base_frame =
                    input_base_frame_for_sound_unit(su, dba_priming, input_delay);
                if is_mono_input {
                    for i in 0..1024usize {
                        let idx = input_base_frame + i as isize;
                        let sample = if idx >= 0 && (idx as usize) < samples.len() {
                            samples[idx as usize]
                        } else {
                            0
                        };
                        pcm0[i] = sample as f32;
                        pcm1[i] = sample as f32;
                        trace_input_pcm.extend_from_slice(&sample.to_le_bytes());
                        if requested_channels > 1 {
                            trace_input_pcm.extend_from_slice(&sample.to_le_bytes());
                        }
                    }
                } else {
                    let base = input_base_frame * 2;
                    for i in 0..1024usize {
                        let idx = base + (i * 2) as isize;
                        let (left, right) = if idx >= 0 {
                            let idx = idx as usize;
                            let left = if idx < samples.len() { samples[idx] } else { 0 };
                            let right = if idx + 1 < samples.len() {
                                samples[idx + 1]
                            } else {
                                0
                            };
                            (left, right)
                        } else {
                            (0, 0)
                        };
                        pcm0[i] = left as f32;
                        pcm1[i] = right as f32;
                        trace_input_pcm.extend_from_slice(&left.to_le_bytes());
                        trace_input_pcm.extend_from_slice(&right.to_le_bytes());
                    }
                }
                let pcm_refs: [&[f32; 1024]; 2] = [&pcm0, &pcm1];

                let input_byte_count = trace_input_pcm.len() as u32;
                let input_byte_count_arg = input_byte_count / u32::from(requested_channels);
                let scheduled_start = (su * 1024) as u64;
                let scheduled_end = scheduled_start + 1024;
                let actual_start = input_base_frame.max(0) as u64;
                let actual_end = actual_start + 1024;
                let payload_offset = if write_frame {
                    Some(frames.len() as u64)
                } else {
                    None
                };
                encoder.begin_production_trace_frame_with_pcm(
                    at3::dsp::encode::ProductionTraceFrameContext {
                        sound_frame_call_idx: su as u32,
                        frame_index: su as u32 + 1,
                        frame_sequence_arg: su as i32,
                        requested_channels,
                        input_byte_count_arg,
                        input_byte_count,
                        input_sample_frame_count: 1024,
                        scheduled_input_sample_frame_start: scheduled_start,
                        scheduled_input_sample_frame_end: scheduled_end,
                        actual_input_sample_frame_start: actual_start,
                        actual_input_sample_frame_end: actual_end,
                        priming_frame: !write_frame,
                        write_frame,
                        payload_offset,
                    },
                    &trace_input_pcm,
                )?;
                let bit_count = encoder.encode_frame(&pcm_refs, &mut out_buf);
                if bit_count < 0 {
                    eprintln!("warning: sound unit {su} encoding failed, using silence");
                    if write_frame {
                        frames.extend_from_slice(&silence_frame[..frame_size as usize]);
                        fallback_frames += 1;
                    }
                    continue;
                }
                let byte_count = ((bit_count as u32 + 7) >> 3) as usize;
                if byte_count > internal_frame_size {
                    eprintln!(
                        "warning: sound unit {su} overflow {byte_count} > {internal_frame_size}, using silence"
                    );
                    if write_frame {
                        frames.extend_from_slice(&silence_frame[..frame_size as usize]);
                        fallback_frames += 1;
                    }
                } else if write_frame {
                    frames.extend_from_slice(&out_buf[..frame_size as usize]);
                    frames_ok += 1;
                }
            }
            encoder.finish_production_trace()?;

            let total_data = frames.len() as u32;
            let mut output_file = fs::File::create(&output)?;
            write_at3_header(
                &mut output_file,
                sample_frames as u32,
                frame_size,
                output_channels,
                joint_stereo,
                total_data,
            )?;
            output_file.write_all(&frames)?;

            eprintln!(
                "wrote {} bytes ({} encoded, {} fallback, {} total) to {output}",
                total_data as usize + 80,
                frames_ok,
                fallback_frames,
                frames_ok + fallback_frames,
            );
            let diagnostics = encoder.diagnostics();
            eprintln!(
                "diagnostics: {} tone drops, {} BFU trims, {} channel rejects",
                diagnostics.tone_payload_drop_events,
                diagnostics.bfu_idwl_decrement_events,
                diagnostics.channel_encode_reject_events,
            );
            Ok(())
        }
        None => {
            println!("atrac3: classic ATRAC3 encoder. Use --help for commands.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ATRAC3_ENCODER_DELAY, SAMPLES_PER_FRAME, input_base_frame_for_sound_unit,
        priming_sound_units, sound_units_to_encode, write_sound_unit,
    };

    #[test]
    fn clean_sound_unit_count_includes_sony_priming_calls() {
        assert_eq!(priming_sound_units(false), 2);
        assert_eq!(sound_units_to_encode(8192, false), 12);
        assert_eq!(sound_units_to_encode(32_768, false), 36);
        assert_eq!(sound_units_to_encode(580_078, false), 570);
        assert_eq!(sound_units_to_encode(7_787_435, false), 7608);
        assert_eq!(sound_units_to_encode(10_142_872, false), 9909);
    }

    #[test]
    fn clean_payload_frame_count_matches_sony_payload_count() {
        for (sample_frames, expected_payloads) in [
            (8192, 10),
            (32_768, 34),
            (580_078, 568),
            (7_787_435, 7606),
            (10_142_872, 9907),
        ] {
            assert_eq!(
                sound_units_to_encode(sample_frames, false) - priming_sound_units(false),
                expected_payloads
            );
        }
    }

    #[test]
    fn clean_schedule_primes_preroll_then_first_delayed_window() {
        assert!(!write_sound_unit(0, false));
        assert!(!write_sound_unit(1, false));
        assert!(write_sound_unit(2, false));
        assert_eq!(
            input_base_frame_for_sound_unit(0, false, ATRAC3_ENCODER_DELAY),
            ATRAC3_ENCODER_DELAY as isize - SAMPLES_PER_FRAME as isize
        );
        assert_eq!(
            input_base_frame_for_sound_unit(1, false, ATRAC3_ENCODER_DELAY),
            ATRAC3_ENCODER_DELAY as isize
        );
        assert_eq!(
            input_base_frame_for_sound_unit(2, false, ATRAC3_ENCODER_DELAY),
            SAMPLES_PER_FRAME as isize + ATRAC3_ENCODER_DELAY as isize
        );
    }

    #[test]
    fn dba_sound_unit_count_keeps_delay_offset() {
        assert_eq!(priming_sound_units(true), 1);
        assert_eq!(sound_units_to_encode(8192, true), 10);
        assert_eq!(sound_units_to_encode(580_078, true), 569);
        assert_eq!(sound_units_to_encode(10_142_872, true), 9908);
    }

    #[test]
    fn dba_schedule_retains_existing_one_unit_preroll() {
        assert!(!write_sound_unit(0, true));
        assert!(write_sound_unit(1, true));
        assert_eq!(
            input_base_frame_for_sound_unit(0, true, ATRAC3_ENCODER_DELAY),
            ATRAC3_ENCODER_DELAY as isize
        );
        assert_eq!(
            input_base_frame_for_sound_unit(1, true, ATRAC3_ENCODER_DELAY),
            SAMPLES_PER_FRAME as isize + ATRAC3_ENCODER_DELAY as isize
        );
        assert_eq!(
            input_base_frame_for_sound_unit(2, true, ATRAC3_ENCODER_DELAY),
            2 * SAMPLES_PER_FRAME as isize + ATRAC3_ENCODER_DELAY as isize
        );
    }
}
