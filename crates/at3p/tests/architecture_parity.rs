use at3p::{
    ATRAC3PLUS_MONO_PROFILES, ATRAC3PLUS_STEREO_PROFILES, Atrac3plusEncoder, Atrac3plusProfile,
    encode_to_vec,
};

fn generated_pcm(channels: usize, frames: usize) -> Vec<Vec<i16>> {
    (0..channels)
        .map(|channel| {
            (0..frames)
                .map(|frame| ((frame as i32 * 43 + channel as i32 * 997) % 50_003 - 25_001) as i16)
                .collect()
        })
        .collect()
}

fn stream(profile: &Atrac3plusProfile, pcm: &[Vec<i16>]) -> Vec<u8> {
    let mut encoder = Atrac3plusEncoder::new(Vec::new(), profile, pcm[0].len() as u32).unwrap();
    let mut offset = 0;
    while let Some(frames) = encoder.expected_next_chunk_frames() {
        let chunk: Vec<&[i16]> = pcm
            .iter()
            .map(|channel| &channel[offset..offset + frames])
            .collect();
        encoder.push_pcm(&chunk).unwrap();
        offset += frames;
    }
    encoder.finish().unwrap().0
}

fn buffered(profile: &Atrac3plusProfile, pcm: &[Vec<i16>]) -> Vec<u8> {
    encode_to_vec(profile, pcm[0].len() as u32, pcm).unwrap()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn every_supported_profile_matches_buffered_and_recorded_output() {
    let expected = [
        (1060, 0x194a_b109_e45a_d38d),
        (1500, 0x8ecc_e828_9ad5_fd64),
        (1980, 0xbe40_093b_2834_690e),
        (2900, 0x13af_77d4_c935_6e61),
        (3820, 0xbfa1_1d82_8553_2d83),
        (1500, 0x328a_d726_5bab_073c),
        (1980, 0x5011_0eb8_3f09_63d5),
        (2900, 0xaaae_7c01_f401_afa3),
        (3820, 0x9956_b847_f394_4a2a),
        (4780, 0x283e_3474_68d9_bef0),
        (5700, 0x0c7f_7f40_10c5_3a7d),
        (7540, 0xafa7_aaf3_70b3_0712),
        (9420, 0x75a5_90fe_e5ff_7054),
        (10340, 0x60e4_2efe_0db9_4146),
    ];
    for (profile, expected) in ATRAC3PLUS_MONO_PROFILES
        .iter()
        .chain(ATRAC3PLUS_STEREO_PROFILES.iter())
        .zip(expected)
    {
        let pcm = generated_pcm(profile.channels() as usize, 6144);
        let streamed = stream(profile, &pcm);
        assert_eq!(streamed, buffered(profile, &pcm));
        assert_eq!(
            (streamed.len(), fnv1a64(&streamed)),
            expected,
            "{} kbps, {} channel(s)",
            profile.bitrate_kbps(),
            profile.channels()
        );
    }
}

#[test]
fn low_middle_and_high_rates_preserve_partial_final_blocks() {
    let profiles = [
        ATRAC3PLUS_MONO_PROFILES[0],
        ATRAC3PLUS_STEREO_PROFILES[3],
        ATRAC3PLUS_STEREO_PROFILES[8],
    ];
    let expected = [
        (1060, 0xbeb8_bbfc_a3df_19ac),
        (3820, 0xef5f_8c63_f270_92df),
        (10340, 0xc930_c331_c4c9_e8e9),
    ];
    for (profile, expected) in profiles.into_iter().zip(expected) {
        let pcm = generated_pcm(profile.channels() as usize, 6145);
        let streamed = stream(&profile, &pcm);
        assert_eq!(streamed, buffered(&profile, &pcm));
        assert_eq!(
            (streamed.len(), fnv1a64(&streamed)),
            expected,
            "{} kbps, {} channel(s)",
            profile.bitrate_kbps(),
            profile.channels()
        );
    }
}
