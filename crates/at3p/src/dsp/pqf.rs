//! PQF analysis filterbank inlined in `at5enc_sigproc` (native
//! `0x4f68c..0x50502`, decompile from `decompiled/libatrac.c` line 43029).
//!
//! Per channel and frame the shell concatenates the 384-float delay line
//! (channel scratch `+0x12000`) with 2048 fresh input samples, slides a
//! 16-sample hop across 128 iterations, and for each hop runs a 384-tap
//! polyphase FIR (12 sections of 16 lanes over the two 192-float
//! `ana_coef` banks) followed by a fast 16-point cosine butterfly
//! (`c_iv16`/`c_ii16`/`c_ii8`/`c_ii4`/`c_ii2`), scattering the 16 outputs
//! into slot 8 of each band's scratch block. The last 384 samples of the
//! concatenated buffer become the next frame's delay line.

use crate::tables::at5::{
    pqf_ana_coef_at5, pqf_c_ii2_at5, pqf_c_ii4_at5, pqf_c_ii8_at5, pqf_c_ii16_at5, pqf_c_iv16_at5,
};

pub const PQF_BANDS: usize = 16;
pub const PQF_DELAY_FLOATS: usize = 0x180;
pub const PQF_INPUT_SAMPLES: usize = 0x800;
pub const PQF_OUTPUT_SAMPLES: usize = 0x80;
pub const PQF_SECTIONS: usize = 12;
pub const PQF_COEF_BANK_FLOATS: usize = PQF_SECTIONS * PQF_BANDS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PqfError {
    DelayWrongLength { needed: usize, actual: usize },
    InputWrongLength { needed: usize, actual: usize },
}

#[derive(Debug, Clone)]
pub struct PqfAnalysisOutput {
    /// Slot-8 rows: `subbands[band][hop]` for the 16 bands.
    pub subbands: Vec<Vec<f32>>,
    /// Next frame's delay line (the last 384 samples of delay ++ input).
    pub delay: Vec<f32>,
}

/// The 16-lane polyphase accumulation for one hop. `window` is the
/// sliding view starting at `buffer + 16 * hop`; the native code reads
/// `window[0x10..0x190)`. Lanes 0..7 pair `window[0x10 + 0x20k + j]`
/// (bank A) with the reflected `window[0x1f + 0x20k - j]` (bank B);
/// lanes 8..15 use the upper half-section the same way. Each section's
/// pair-sum is accumulated with an f32 store per step, matching the
/// native `fstps` per section.
fn polyphase(window: &[f32], bank_a: &[f32], bank_b: &[f32]) -> [f32; PQF_BANDS] {
    let mut acc = [0.0f32; PQF_BANDS];
    for section in 0..PQF_SECTIONS {
        let base = 0x20 * section;
        for lane in 0..PQF_BANDS {
            let (sample_a, sample_b) = if lane < 8 {
                (window[0x10 + base + lane], window[0x1f + base - lane])
            } else {
                (
                    window[0x20 + base + lane - 8],
                    window[0x2f + base - (lane - 8)],
                )
            };
            let coef_index = PQF_BANDS * section + lane;
            acc[lane] = sample_b * bank_b[coef_index] + sample_a * bank_a[coef_index] + acc[lane];
        }
    }
    acc
}

/// The fast 16-point cosine butterfly, transcribed operation-for-
/// operation from the decompile so the lifting-difference output chain
/// (`out[n] = plan[n] - out[n-1]`) keeps the native rounding structure.
fn butterfly(
    acc: &[f32; PQF_BANDS],
    civ16: &[f32; 16],
    cii16: &[f32; 8],
    cii8: &[f32; 8],
    cii4: &[f32; 4],
    cii2: &[f32; 4],
) -> [f32; PQF_BANDS] {
    let v = [
        acc[7] * civ16[0],
        acc[6] * civ16[1],
        acc[5] * civ16[2],
        acc[4] * civ16[3],
        acc[3] * civ16[4],
        acc[2] * civ16[5],
        acc[1] * civ16[6],
        acc[0] * civ16[7],
        acc[8] * civ16[8],
        acc[9] * civ16[9],
        acc[10] * civ16[10],
        acc[11] * civ16[11],
        acc[12] * civ16[12],
        acc[13] * civ16[13],
        acc[14] * civ16[14],
        acc[15] * civ16[15],
    ];

    let s0 = v[7] + v[0] + v[15] + v[8];
    let t_a = (v[0] - v[15]) * cii16[0];
    let t_b = (v[7] - v[8]) * cii16[7];
    let u0 = t_a + t_b;
    let p0 = ((v[0] + v[15]) - v[7] - v[8]) * cii8[0];
    let q0 = cii8[0] * (t_a - t_b);

    let s1 = v[1] + v[14] + v[6] + v[9];
    let t_c = (v[6] - v[9]) * cii16[6];
    let p1 = ((v[1] + v[14]) - v[6] - v[9]) * cii8[1];
    let t_d = (v[1] - v[14]) * cii16[1];
    let u1 = t_d + t_c;
    let q1 = cii8[1] * (t_d - t_c);

    let s2 = v[5] + v[2] + v[13] + v[10];
    let p2 = ((v[2] + v[13]) - v[5] - v[10]) * cii8[2];
    let t_e = (v[2] - v[13]) * cii16[2];
    let t_f = (v[5] - v[10]) * cii16[5];
    let u2 = t_e + t_f;
    let q2 = cii8[2] * (t_e - t_f);

    let s3 = v[4] + v[3] + v[12] + v[11];
    let t_g = (v[3] - v[12]) * cii16[3];
    let p3 = ((v[3] + v[12]) - v[4] - v[11]) * cii8[3];
    let t_h = (v[4] - v[11]) * cii16[4];
    let u3 = t_g + t_h;
    let q3 = cii8[3] * (t_g - t_h);

    let mut out = [0.0f32; PQF_BANDS];
    out[0] = (s1 + s0 + s2 + s3) * 0.5;
    let m0 = cii2[0] * (((s0 - s1) - s2) + s3);
    let d0 = (s0 - s3) * cii4[0];
    let d1 = (s1 - s2) * cii4[1];
    let m1 = cii2[1] * d0 - cii2[2] * d1;
    let d2 = (p0 - p3) * cii4[2];
    let d3 = (p1 - p2) * cii4[3];
    let t2 = (p1 + p0 + p2 + p3) * 0.5;
    let e544 = (d2 + d3) * 0.5 - t2;
    let e534 = (((p0 - p1) - p2) + p3) * cii2[0] - e544;
    let e524 = (cii2[1] * d2 - d3 * cii2[2]) - e534;
    let t1 = (u0 + u1 + u2 + u3) * 0.5;
    out[1] = t1 - out[0];
    out[2] = t2 - out[1];
    let e56c = cii2[0] * (((u0 - u1) - u2) + u3);
    let d4 = cii4[0] * (u0 - u3);
    let d5 = cii4[1] * (u1 - u2);
    let e564 = cii2[1] * d4 - cii2[2] * d5;
    let t3 = (q0 + q1 + q2 + q3) * 0.5 - t1;
    out[3] = t3 - out[2];
    let e548 = (d4 + d5) * 0.5 - t3;
    out[4] = (d0 + d1) * 0.5 - out[3];
    let d6 = (q0 - q3) * cii4[2];
    let d7 = (q1 - q2) * cii4[3];
    out[5] = e548 - out[4];
    out[6] = e544 - out[5];
    let e570 = (((((d6 + d7) - q0) - q1) - q2) - q3) * 0.5;
    let e568 = (((q0 - q1) - q2) + q3) * cii2[0] - e570;
    let e560 = (d6 * cii2[1] - d7 * cii2[2]) - e568;
    let e540 = e570 - e548;
    out[7] = e540 - out[6];
    let e538 = e56c - e540;
    out[8] = m0 - out[7];
    out[9] = e538 - out[8];
    let e530 = e568 - e538;
    out[10] = e534 - out[9];
    out[11] = e530 - out[10];
    let e528 = e564 - e530;
    out[12] = m1 - out[11];
    out[13] = e528 - out[12];
    let e520 = e560 - e528;
    out[14] = e524 - out[13];
    out[15] = e520 - out[14];
    out
}

/// One channel's PQF analysis pass: `delay` is the 384-float line from
/// channel scratch `+0x12000`, `input` the frame's 2048 samples.
pub fn pqf_analysis_at5(delay: &[f32], input: &[f32]) -> Result<PqfAnalysisOutput, PqfError> {
    if delay.len() != PQF_DELAY_FLOATS {
        return Err(PqfError::DelayWrongLength {
            needed: PQF_DELAY_FLOATS,
            actual: delay.len(),
        });
    }
    if input.len() != PQF_INPUT_SAMPLES {
        return Err(PqfError::InputWrongLength {
            needed: PQF_INPUT_SAMPLES,
            actual: input.len(),
        });
    }

    let ana_coef = pqf_ana_coef_at5();
    let bank_a = &ana_coef[..PQF_COEF_BANK_FLOATS];
    let bank_b = &ana_coef[PQF_COEF_BANK_FLOATS..2 * PQF_COEF_BANK_FLOATS];
    let civ16 = pqf_c_iv16_at5();
    let cii16 = pqf_c_ii16_at5();
    let cii8 = pqf_c_ii8_at5();
    let cii4 = pqf_c_ii4_at5();
    let cii2 = pqf_c_ii2_at5();

    let mut buffer = Vec::with_capacity(PQF_DELAY_FLOATS + PQF_INPUT_SAMPLES);
    buffer.extend_from_slice(delay);
    buffer.extend_from_slice(input);

    let mut subbands = vec![vec![0.0f32; PQF_OUTPUT_SAMPLES]; PQF_BANDS];
    for hop in 0..PQF_OUTPUT_SAMPLES {
        let window = &buffer[hop * 16..];
        let acc = polyphase(window, bank_a, bank_b);
        let out = butterfly(&acc, &civ16, &cii16, &cii8, &cii4, &cii2);
        for (band, value) in out.iter().enumerate() {
            subbands[band][hop] = *value;
        }
    }

    Ok(PqfAnalysisOutput {
        subbands,
        delay: buffer[PQF_INPUT_SAMPLES..].to_vec(),
    })
}
