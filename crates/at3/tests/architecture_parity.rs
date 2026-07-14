use std::io::{self, Write};

use at3::Atrac3Profile;
use at3::encoder::stream::{
    Atrac3StreamEncoder, Atrac3StreamError, Atrac3WriteStage, PCM_BLOCK_FRAMES,
};

const PROFILES: [(u32, u16); 5] = [(52, 1), (66, 1), (66, 2), (105, 2), (132, 2)];

fn generated_pcm(channels: usize, frames: usize) -> Vec<Vec<i16>> {
    (0..channels)
        .map(|channel| {
            (0..frames)
                .map(|frame| ((frame as i32 * 43 + channel as i32 * 997) % 50_003 - 25_001) as i16)
                .collect()
        })
        .collect()
}

fn encode(bitrate_kbps: u32, channels: u16, frames: usize) -> Vec<u8> {
    let pcm = generated_pcm(channels as usize, frames);
    let profile = Atrac3Profile::new(bitrate_kbps, channels).unwrap();
    let mut encoder = Atrac3StreamEncoder::new(Vec::new(), profile, frames as u32).unwrap();
    let mut offset = 0;
    while offset < frames {
        let end = usize::min(offset + PCM_BLOCK_FRAMES, frames);
        let chunk: Vec<&[i16]> = pcm.iter().map(|channel| &channel[offset..end]).collect();
        encoder.push_pcm(&chunk).unwrap();
        offset = end;
    }
    encoder.finish().unwrap().0
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn every_supported_profile_matches_the_recorded_exact_block_output() {
    let expected = [
        (536, 0xb182_3be5_5396_8e16),
        (848, 0xa0af_a901_5168_96f6),
        (656, 0x001e_0a08_a5c4_08ad),
        (992, 0xcd86_8f9f_8ee5_3faa),
        (1616, 0xcd4b_36fe_a609_9d76),
    ];
    for ((bitrate_kbps, channels), expected) in PROFILES.into_iter().zip(expected) {
        let output = encode(bitrate_kbps, channels, 2048);
        assert_eq!(
            (output.len(), fnv1a64(&output)),
            expected,
            "{bitrate_kbps} kbps, {channels} channel(s)"
        );
    }
}

#[test]
fn clean_and_dba_partial_final_blocks_match_recorded_output() {
    let cases = [(52, 1), (66, 2), (132, 2)];
    let expected = [
        (536, 0x8b53_88a2_28b6_f890),
        (656, 0xf543_6834_a8cd_2e92),
        (1616, 0xff29_a7ef_f5fd_7633),
    ];
    for ((bitrate_kbps, channels), expected) in cases.into_iter().zip(expected) {
        let output = encode(bitrate_kbps, channels, 2049);
        assert_eq!(
            (output.len(), fnv1a64(&output)),
            expected,
            "{bitrate_kbps} kbps, {channels} channel(s)"
        );
    }
}

#[derive(Debug)]
struct FailImmediately;

impl Write for FailImmediately {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("injected header failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn empty_invalid_and_header_failure_errors_are_stable() {
    let profile = Atrac3Profile::new(132, 2).unwrap();
    assert!(matches!(
        Atrac3StreamEncoder::new(Vec::new(), profile, 0),
        Err(Atrac3StreamError::EmptyInput)
    ));
    assert!(Atrac3Profile::new(52, 2).is_err());
    assert!(matches!(
        Atrac3StreamEncoder::new(FailImmediately, profile, 2048),
        Err(Atrac3StreamError::Io {
            stage: Atrac3WriteStage::Header,
            ..
        })
    ));
}
