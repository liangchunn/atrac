use crate::dsp::quant::ispof_iqt_at3;
use crate::tables::dba;
use crate::tables::dba::{DBA_HUF_MASK, DBA_NBITS_WL2_QUAD, DBA_NORM_FACT, DBA_SCALE_LOOKUP};
use crate::tables::{NSPS1024_TABLE, QTSTART_TABLE};

#[cfg_attr(not(test), allow(dead_code))]
const DBA_QMF_HISTORY: usize = 138;
#[cfg_attr(not(test), allow(dead_code))]
const DBA_QMF_WORK: usize = DBA_QMF_HISTORY + 1024;
#[cfg_attr(not(test), allow(dead_code))]
const DBA_GAIN_MDCT_HISTORY: usize = 0x600;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DBA_GAIN_INFO_STRIDE: usize = 0x33;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DBA_GAIN_INFO_EXT_PREFIX: usize = 8;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DBA_GAIN_INFO_WORDS: usize = 4 * DBA_GAIN_INFO_STRIDE;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DBA_GAIN_INFO_EXT_WORDS: usize = DBA_GAIN_INFO_EXT_PREFIX + DBA_GAIN_INFO_WORDS;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DBA_GAIN_BAND0_COUNT_COMPACT_OFFSET: usize = 0xc4;
#[cfg_attr(not(test), allow(dead_code))]
const DBA_MDCT_SCRATCH: usize = 1024;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct DbaAnalysisFilterBank {
    history: [f32; DBA_QMF_HISTORY],
}

impl DbaAnalysisFilterBank {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new() -> Self {
        Self {
            history: [0.0; DBA_QMF_HISTORY],
        }
    }

    pub(crate) fn analysis(&mut self, pcm: &[f32; 1024], bands: &mut [[f32; 256]; 4]) {
        let mut work = [0.0f32; DBA_QMF_WORK];
        work[..DBA_QMF_HISTORY].copy_from_slice(&self.history);
        work[DBA_QMF_HISTORY..].copy_from_slice(pcm);

        #[allow(clippy::needless_range_loop)]
        for sample in 0..256 {
            let base = sample * 4;
            let c0 = dba::DBA_QMF_COEFFICIENTS[0] as f64;
            let mut f22 = pcm[base + 1] as f64 + work[base + 0x5d] as f64 * c0;
            let mut f3 = pcm[base] as f64 * c0 + work[base + 0x5c] as f64;
            let mut f4 = pcm[base + 3] as f64 + work[base + 0x5f] as f64 * c0;
            let mut f5 = pcm[base + 2] as f64 * c0 + work[base + 0x5e] as f64;

            for coeff_idx in (1..=0x16).rev() {
                let reverse_idx = 0x17 - coeff_idx;
                let coeff = dba::DBA_QMF_COEFFICIENTS[coeff_idx] as f64;
                let reverse_coeff = dba::DBA_QMF_COEFFICIENTS[reverse_idx] as f64;
                let tap = coeff_idx * 2;
                f4 += coeff * work[base + tap + 0x5f] as f64;
                f22 += coeff * work[base + tap + 0x5d] as f64;
                f3 += reverse_coeff * work[base + tap + 0x5c] as f64;
                f5 += reverse_coeff * work[base + tap + 0x5e] as f64;
            }

            let a0 = (f22 + f3) as f32;
            let a1 = (f22 - f3) as f32;
            let a2 = (f4 + f5) as f32;
            let a3 = (f4 - f5) as f32;
            work[base + 0x5c] = a0;
            work[base + 0x5d] = a1;
            work[base + 0x5e] = a2;
            work[base + 0x5f] = a3;

            let mut f6 = work[base + 3] as f64 * c0 + a3 as f64;
            let mut f7 = work[base + 2] as f64 * c0 + a2 as f64;
            let mut f10 = a1 as f64 * c0 + work[base + 1] as f64;
            let mut f22 = a0 as f64 * c0 + work[base] as f64;
            let mut ptr = base + 0x58;

            for coeff_idx in (1..=0x16).rev() {
                let reverse_idx = 0x17 - coeff_idx;
                let coeff = dba::DBA_QMF_COEFFICIENTS[coeff_idx] as f64;
                let reverse_coeff = dba::DBA_QMF_COEFFICIENTS[reverse_idx] as f64;
                f6 += coeff * work[ptr + 3] as f64;
                f7 += coeff * work[ptr + 2] as f64;
                f22 += reverse_coeff * work[ptr] as f64;
                f10 += reverse_coeff * work[ptr + 1] as f64;
                ptr -= 4;
            }

            bands[0][sample] = (f7 + f22) as f32;
            bands[1][sample] = (f7 - f22) as f32;
            bands[2][sample] = (f6 - f10) as f32;
            bands[3][sample] = (f6 + f10) as f32;
        }

        self.history.copy_from_slice(&work[1024..]);
    }
}

impl Default for DbaAnalysisFilterBank {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DbaState {
    pub bit_budget: i32,
    pub bits_per_sample: i32,
    pub min_idsf_idx: i32,
    pub budget_ceil: i32,
}

pub fn init_dba_at3(sample_rate: i32, bit_budget: i32) -> DbaState {
    let mut state = DbaState {
        bit_budget,
        ..DbaState::default()
    };
    if bit_budget <= 0 {
        return state;
    }

    state.bits_per_sample = bit_budget.wrapping_shl(11) / sample_rate;
    state.min_idsf_idx = (1..=32)
        .find(|&idx| ispof_iqt_at3(idx as u32) >= state.bits_per_sample)
        .unwrap_or(33);
    let rounded = state.bits_per_sample.wrapping_add(0xff);
    state.budget_ceil = if rounded < 0 {
        state.bits_per_sample.wrapping_add(0x1fe) >> 8
    } else {
        rounded >> 8
    };
    state
}

#[derive(Debug, Clone, Copy)]
pub struct DbaMainsubParams<'a> {
    pub tonal_spectrum: &'a [f32],
    pub tonal_bfu_count: i32,
    pub nontonal_spectrum: &'a [f32],
    pub nontonal_bfu_count: i32,
    pub base_position: i32,
    pub position_scale: i32,
    pub mode: i32,
    pub fixed_splice: i32,
}

fn bfu_peak(spectrum: &[f32], bfu: usize) -> f64 {
    let start = QTSTART_TABLE[bfu] as usize;
    let end = QTSTART_TABLE[bfu + 1] as usize;
    spectrum[start..end]
        .iter()
        .map(|sample| sample.abs() as f64)
        .fold(0.0, f64::max)
}

fn trunc_i32(value: f64) -> i32 {
    value.trunc() as i32
}

pub fn dba_mainsub(params: DbaMainsubParams<'_>) -> i32 {
    let mut tonal_energy = 0.0;
    let last_tonal_bfu = (params.tonal_bfu_count - 1).min(18);
    if last_tonal_bfu >= 0 {
        for bfu in (0..=last_tonal_bfu as usize).rev() {
            tonal_energy += NSPS1024_TABLE[bfu] as f64 * bfu_peak(params.tonal_spectrum, bfu);
        }
    }

    let mut nontonal_energy = 0.0;
    if params.nontonal_bfu_count > 0 {
        for (bfu, &nsps) in NSPS1024_TABLE
            .iter()
            .enumerate()
            .take(params.nontonal_bfu_count as usize)
        {
            nontonal_energy += nsps as f64 * bfu_peak(params.nontonal_spectrum, bfu);
        }
    }
    nontonal_energy *= 2.0;
    if params.mode != 3 {
        nontonal_energy *= 2.0;
    }

    let target = params.position_scale.wrapping_mul(16).wrapping_sub(59);
    if nontonal_energy < 1.0 || (params.mode != 3 && nontonal_energy < 4.0) {
        return target;
    }

    if params.fixed_splice != 0
        || ((tonal_energy <= nontonal_energy * 3.0 || params.mode != 3)
            && tonal_energy <= nontonal_energy * 6.0)
    {
        if tonal_energy <= nontonal_energy || params.mode != 3 {
            return params.base_position;
        }
        let offset = trunc_i32(
            (target.wrapping_sub(params.base_position) as f64)
                * (1.0 / (tonal_energy * 10.0))
                * (tonal_energy - nontonal_energy),
        );
        return params.base_position.wrapping_add(offset & !7);
    }

    let mut offset = target.wrapping_sub(params.base_position);
    if tonal_energy < nontonal_energy * 12.0 {
        if tonal_energy <= nontonal_energy * 10.0 {
            if nontonal_energy * 5.0 < tonal_energy {
                offset = trunc_i32(offset as f64 * 1.25);
            }
        } else {
            offset = trunc_i32(offset as f64 * 1.5);
        }
        offset = trunc_i32(
            offset as f64 * (1.0 / (tonal_energy * 2.0)) * (tonal_energy - nontonal_energy),
        );
    }
    params.base_position.wrapping_add(offset & !7)
}

pub(crate) fn dba_magic_round_bits(sample: f32, scale: f32) -> u32 {
    let rounded = sample * scale + 12_582_912.0;
    rounded.to_bits()
}

fn dba_tone_quantized_sample(sample: f32, scale: f32) -> i32 {
    let rounded = dba_magic_round_bits(sample, scale);
    (rounded as u16 as i16) as i32
}

pub fn set_best_idsf4_tone(
    samples: &[f32],
    table_offset: i32,
    sample_count: usize,
    current_idsf: i32,
) -> i32 {
    let max_bits = samples[..4]
        .iter()
        .map(|sample| sample.to_bits().wrapping_mul(2))
        .max()
        .unwrap();
    let fraction = max_bits & 0x00ff_ffff;
    let mut min_idsf = (max_bits >> 24).wrapping_mul(3).wrapping_sub(0x16c);
    if fraction > 0x0096_5fe9 {
        min_idsf = min_idsf.wrapping_add(1);
    }
    if fraction < 0x0042_8a30 {
        min_idsf = min_idsf.wrapping_sub(1);
    }
    if min_idsf >= 64 {
        min_idsf = 0;
    }

    let min_idsf = min_idsf as i32;
    let mut candidate = (min_idsf + 3).min(64) - 1;
    if candidate < min_idsf {
        return current_idsf;
    }

    let mut best_error = 1.0e10_f32;
    let mut result = current_idsf;
    let mut phase = (candidate as u32).wrapping_mul(0x002b_0000);
    loop {
        let table_index = (((phase >> 23) as i32 + table_offset) * 3 - candidate - 1) as usize;
        let scale = f32::from_bits(
            DBA_NORM_FACT[table_index]
                .to_bits()
                .wrapping_sub(phase & 0x7f80_0000),
        );
        let mut quantized_or = 0u32;
        let mut max_error = 0.0_f32;
        for &sample in &samples[..sample_count] {
            let scaled = sample * scale;
            let quantized = dba_tone_quantized_sample(sample, scale);
            quantized_or |= quantized as u32;
            max_error = max_error.max((scaled - quantized as f32).abs());
        }

        if max_error < best_error * scale {
            result = candidate;
            best_error = max_error * (1.0 / scale);
            if quantized_or & 1 == 0 && candidate < 61 {
                let max_shifts = ((61 - candidate + 2) / 3) as u32;
                let shifts = quantized_or.trailing_zeros().min(max_shifts);
                result = candidate + shifts as i32 * 3;
            }
        }

        candidate -= 1;
        phase = phase.wrapping_sub(0x002b_0000);
        if candidate < min_idsf {
            return result;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectChconvResult {
    pub coefficient: i32,
    pub output_modes: [i32; 4],
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbaChconvState {
    pub(crate) threshold: i32,
    pub(crate) modes: [i32; 4],
    pub(crate) abs_modes: [i32; 4],
    pub(crate) smooth_coefficients: [f32; 4],
    pub(crate) target_coefficients: [f32; 4],
    pub(crate) previous_coefficient: i32,
    pub(crate) current_coefficient: i32,
    pub(crate) energy_history: [[[f32; 2]; 3]; 4],
}

impl DbaChconvState {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new(threshold: i32) -> Self {
        Self {
            threshold,
            modes: [-3; 4],
            abs_modes: [0; 4],
            smooth_coefficients: [1.0; 4],
            target_coefficients: [1.0; 4],
            previous_coefficient: 0,
            current_coefficient: 15,
            energy_history: [[[0.0; 2]; 3]; 4],
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DbaChconvResult {
    pub(crate) coefficient: i32,
    pub(crate) modes: [i32; 4],
}

const CHCONV_F_2304: f32 = 2304.0;
const CHCONV_F_96: f32 = 96.0;
const CHCONV_F_19_697716: f32 = 19.697716_f32;
const CHCONV_F_4_791992E12: f32 = 4_791_992_000_000.0_f32;
const CHCONV_F_0_1: f32 = 0.1_f32;
const CHCONV_F_16: f32 = 16.0;
const CHCONV_F_14: f32 = 14.0;
const CHCONV_F_12582912: f32 = 12_582_912.0;
const CHCONV_F_3_94: f32 = 3.94;
const CHCONV_F_256: f32 = 256.0;
const CHCONV_F_1_6802721: f32 = 1.6802721;
const CHCONV_F_1_6666666: f32 = 1.6666666;
const CHCONV_F_0_05: f32 = 0.05;
const CHCONV_F_0_5: f32 = 0.5;
const CHCONV_F_0_125: f32 = 0.125;

fn select_chconv_into_state(
    tonal_spectrum: &[f32],
    nontonal_spectrum: &[f32],
    state: &mut DbaChconvState,
) -> SelectChconvResult {
    select_chconv_with_energies(state, |band| {
        let mut tonal_energy: f64 = 0.0;
        let mut nontonal_energy: f64 = 0.0;
        for j in 0..256usize {
            let idx = band + j * 4;
            tonal_energy += tonal_spectrum[idx].abs() as f64;
            nontonal_energy += nontonal_spectrum[idx].abs() as f64;
        }
        (tonal_energy as f32, nontonal_energy as f32)
    })
}

fn select_chconv_bands_into_state(
    bands: &[[[f32; 256]; 4]; 2],
    state: &mut DbaChconvState,
) -> SelectChconvResult {
    select_chconv_with_energies(state, |band| {
        let mut tonal_energy: f64 = 0.0;
        let mut nontonal_energy: f64 = 0.0;
        #[allow(clippy::needless_range_loop)]
        for sample in 0..256usize {
            tonal_energy += bands[0][band][sample].abs() as f64;
            nontonal_energy += bands[1][band][sample].abs() as f64;
        }
        (tonal_energy as f32, nontonal_energy as f32)
    })
}

fn select_chconv_with_energies<F>(
    state: &mut DbaChconvState,
    mut band_energies: F,
) -> SelectChconvResult
where
    F: FnMut(usize) -> (f32, f32),
{
    let threshold = state.threshold;
    let mut tonal_accum: f64 = CHCONV_F_0_1 as f64;
    let mut nontonal_accum: f64 = CHCONV_F_0_1 as f64;
    let mut output_modes = [0i32; 4];

    #[allow(clippy::needless_range_loop)]
    for band in 0..4usize {
        let (tonal_energy, nontonal_energy) = band_energies(band);
        let band_state = &mut state.energy_history[band];
        band_state[2] = band_state[1];
        band_state[1] = band_state[0];
        band_state[0] = [tonal_energy, nontonal_energy];

        let previous_mode = state.modes[band];

        let (tonal_ratio, nontonal_ratio) = if (band as i32) < threshold {
            if previous_mode == 0 {
                (CHCONV_F_2304 as f64, CHCONV_F_96 as f64)
            } else if previous_mode == 1 {
                (CHCONV_F_96 as f64, CHCONV_F_2304 as f64)
            } else {
                (CHCONV_F_2304 as f64, CHCONV_F_2304 as f64)
            }
        } else {
            (CHCONV_F_19_697716 as f64, CHCONV_F_19_697716 as f64)
        };

        let tonal = tonal_energy as f64;
        let nontonal = nontonal_energy as f64;
        let new_mode = if nontonal <= nontonal_ratio * tonal {
            if tonal <= tonal_ratio * nontonal {
                if nontonal + tonal < CHCONV_F_4_791992E12 as f64 {
                    -3
                } else {
                    3
                }
            } else {
                1
            }
        } else {
            0
        };

        state.abs_modes[band] = previous_mode.abs();
        state.modes[band] = new_mode;
        output_modes[band] = new_mode;

        if (band as i32) >= threshold && (new_mode == -3 || new_mode == 3) {
            tonal_accum = tonal_accum * CHCONV_F_16 as f64 + tonal_energy as f64;
            nontonal_accum = nontonal_accum * CHCONV_F_16 as f64 + nontonal_energy as f64;
        }
    }

    state.previous_coefficient = state.current_coefficient;

    let total_accum = tonal_accum + nontonal_accum;
    let mut coefficient_base = 0;
    let mut min_accum = tonal_accum;
    if nontonal_accum < tonal_accum {
        coefficient_base = 8;
        min_accum = nontonal_accum;
    }

    let magic_rounded = ((1.0_f64 / total_accum) * min_accum * CHCONV_F_14 as f64
        + CHCONV_F_12582912 as f64) as f32;
    let magic_low_bits = (magic_rounded.to_bits() & 7) as i32;
    coefficient_base += magic_low_bits;

    let coefficient = if coefficient_base == 0 || coefficient_base == 8 {
        coefficient_base + 1
    } else {
        coefficient_base
    };
    state.current_coefficient = coefficient;

    SelectChconvResult {
        coefficient,
        output_modes,
    }
}

pub fn select_chconv(
    tonal_spectrum: &[f32],
    nontonal_spectrum: &[f32],
    threshold: i32,
    input_modes: &[i32; 4],
) -> SelectChconvResult {
    let mut state = DbaChconvState::new(threshold);
    state.modes = *input_modes;
    state.current_coefficient = 0;
    select_chconv_into_state(tonal_spectrum, nontonal_spectrum, &mut state)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_interleave_bands(bands: &[[f32; 256]; 4]) -> [f32; 1024] {
    let mut interleaved = [0.0f32; 1024];
    dba_interleave_bands_into(bands, &mut interleaved);
    interleaved
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_interleave_bands_into(bands: &[[f32; 256]; 4], interleaved: &mut [f32; 1024]) {
    for sample in 0..256 {
        for band in 0..4 {
            interleaved[sample * 4 + band] = bands[band][sample];
        }
    }
}

fn dba_chconv_target(tonal_energy: f32, nontonal_energy: f32) -> f32 {
    if nontonal_energy <= tonal_energy * CHCONV_F_3_94
        || tonal_energy * CHCONV_F_256 <= nontonal_energy
    {
        if nontonal_energy * CHCONV_F_3_94 < tonal_energy
            && tonal_energy < nontonal_energy * CHCONV_F_256
        {
            return (tonal_energy - nontonal_energy)
                * CHCONV_F_1_6802721
                * (1.0 / (nontonal_energy + tonal_energy));
        }
        if nontonal_energy == 0.0
            || tonal_energy == 0.0
            || (tonal_energy < nontonal_energy * CHCONV_F_256
                && nontonal_energy < tonal_energy * CHCONV_F_256)
        {
            return 1.0;
        }
        CHCONV_F_1_6666666
    } else {
        (nontonal_energy - tonal_energy)
            * CHCONV_F_1_6802721
            * (1.0 / (nontonal_energy + tonal_energy))
    }
}

fn dba_chconv_weight_indices(mode_abs: i32, mut previous: i32, mut current: i32) -> (usize, usize) {
    if mode_abs == 1 {
        previous += 8;
        current += 8;
    } else if mode_abs == 0 {
        previous = (previous ^ 8) + 8;
        current = (current ^ 8) + 8;
    } else if mode_abs == 3 {
        previous &= 7;
        current &= 7;
    }
    (previous as usize, current as usize)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn dba_channel_convert(
    bands: &mut [[[f32; 256]; 4]; 2],
    state: &mut DbaChconvState,
) -> DbaChconvResult {
    if state.threshold < 0 {
        return DbaChconvResult {
            coefficient: state.current_coefficient,
            modes: state.modes,
        };
    }

    let selected = select_chconv_bands_into_state(bands, state);

    #[allow(clippy::needless_range_loop)]
    for band in 0..4 {
        let mode_abs = state.modes[band].abs();
        if (band as i32) < state.threshold {
            let tonal_energy = state.energy_history[band][0][0];
            let nontonal_energy = state.energy_history[band][0][1];
            let current_mode = mode_abs as usize;
            let previous_mode = state.abs_modes[band] as usize;
            let chsw_current_hi = dba::DBA_CHSWCOEF[current_mode + 1];
            let chsw_current_lo = dba::DBA_CHSWCOEF[current_mode];
            let chsw_previous_hi = dba::DBA_CHSWCOEF[previous_mode + 1];
            let chsw_previous_lo = dba::DBA_CHSWCOEF[previous_mode];
            let previous_smooth = state.smooth_coefficients[band];
            let mut target = if mode_abs == 3 {
                dba_chconv_target(tonal_energy, nontonal_energy)
            } else {
                1.0
            };
            if CHCONV_F_0_05 <= target - previous_smooth {
                target = previous_smooth + CHCONV_F_0_05;
            }
            if CHCONV_F_0_05 <= previous_smooth - target {
                target = previous_smooth - CHCONV_F_0_05;
            }
            state.target_coefficients[band] = target;
            state.smooth_coefficients[band] = target;

            {
                let current_hi_scaled = chsw_current_hi * target;
                #[allow(clippy::needless_range_loop)]
                for sample in 0..8 {
                    let ramp = (sample as i32 - 8) as f32;
                    let left = bands[0][band][sample];
                    let right = bands[1][band][sample];
                    let mid = (left + right) * CHCONV_F_0_5;
                    let left_coef = chsw_current_lo
                        - ramp * (chsw_previous_lo - chsw_current_lo) * CHCONV_F_0_125;
                    let right_coef = current_hi_scaled
                        - ramp
                            * (chsw_previous_hi * previous_smooth - current_hi_scaled)
                            * CHCONV_F_0_125;
                    bands[1][band][sample] = (left - left_coef * mid) * (1.0 / right_coef);
                    bands[0][band][sample] = mid;
                }
                #[allow(clippy::needless_range_loop)]
                for sample in 8..256 {
                    let left = bands[0][band][sample];
                    let right = bands[1][band][sample];
                    let mid = (left + right) * CHCONV_F_0_5;
                    bands[1][band][sample] =
                        (left - mid * chsw_current_lo) * (1.0 / current_hi_scaled);
                    bands[0][band][sample] = mid;
                }
            }
        } else {
            let (previous_idx, current_idx) = dba_chconv_weight_indices(
                mode_abs,
                state.previous_coefficient,
                state.current_coefficient,
            );
            let previous_weight = dba::DBA_WT_COMP[previous_idx];
            let current_weight = dba::DBA_WT_COMP[current_idx];
            {
                #[allow(clippy::needless_range_loop)]
                for sample in 0..8 {
                    let ramp = (sample as i32 - 8) as f32;
                    let denom =
                        current_weight - ramp * (previous_weight - current_weight) * CHCONV_F_0_125;
                    bands[0][band][sample] =
                        (bands[1][band][sample] + bands[0][band][sample]) * (1.0 / denom);
                }
                #[allow(clippy::needless_range_loop)]
                for sample in 8..256 {
                    bands[0][band][sample] =
                        (bands[1][band][sample] + bands[0][band][sample]) * (1.0 / current_weight);
                }
            }
        }
    }

    DbaChconvResult {
        coefficient: selected.coefficient,
        modes: selected.output_modes,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct DbaGainMdctState {
    history: [f32; DBA_GAIN_MDCT_HISTORY],
}

impl DbaGainMdctState {
    pub(crate) fn new() -> Self {
        Self {
            history: [0.0; DBA_GAIN_MDCT_HISTORY],
        }
    }
}

impl Default for DbaGainMdctState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone)]
pub(crate) struct DbaGainScheduleState {
    side_info_ext: [i32; DBA_GAIN_INFO_EXT_WORDS],
    band0_gain_count: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
impl DbaGainScheduleState {
    pub(crate) fn new() -> Self {
        Self {
            side_info_ext: [0; DBA_GAIN_INFO_EXT_WORDS],
            band0_gain_count: 0,
        }
    }

    pub(crate) fn side_info(&self) -> &[i32] {
        &self.side_info_ext[DBA_GAIN_INFO_EXT_PREFIX..]
    }

    pub(crate) fn side_info_ext(&self) -> &[i32] {
        &self.side_info_ext
    }

    pub(crate) fn band0_gain_count(&self) -> i32 {
        self.band0_gain_count
    }
}

impl Default for DbaGainScheduleState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DbaGainScheduleMode {
    pub(crate) channel_mode: i32,
    pub(crate) chconv_abs_modes: [i32; 4],
    pub(crate) chconv_modes: [i32; 4],
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbaGainMdctFrameResult {
    pub(crate) spectrum: [f32; 1024],
    pub(crate) gain_side_info: [i32; DBA_GAIN_INFO_WORDS],
    pub(crate) gain_side_info_ext: [i32; DBA_GAIN_INFO_EXT_WORDS],
    pub(crate) band0_gain_count: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Default)]
pub(crate) struct DbaGainMdctChannelState {
    schedule: DbaGainScheduleState,
    mdct: DbaGainMdctState,
}

#[cfg_attr(not(test), allow(dead_code))]
impl DbaGainMdctChannelState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn transform(
        &mut self,
        input_bands: &[f32; 1024],
        mode: DbaGainScheduleMode,
    ) -> DbaGainMdctFrameResult {
        dba_generate_gain_side_info(input_bands, mode, &mut self.schedule);
        let gain_side_info =
            core::array::from_fn(|idx| self.schedule.side_info_ext[DBA_GAIN_INFO_EXT_PREFIX + idx]);
        let gain_side_info_ext = self.schedule.side_info_ext;
        let band0_gain_count = self.schedule.band0_gain_count();
        dba_apply_scheduled_gain(&gain_side_info_ext, &mut self.mdct);
        let spectrum = dba_mdct_after_scheduled_gain(input_bands, &gain_side_info, &mut self.mdct);
        DbaGainMdctFrameResult {
            spectrum,
            gain_side_info,
            gain_side_info_ext,
            band0_gain_count,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DbaFrameConfig {
    pub(crate) frame_bytes: usize,
    pub(crate) channel_bytes: usize,
    pub(crate) js_enabled: bool,
    pub(crate) initial_nunits: [i32; 2],
    pub(crate) base_available_bits: [i32; 2],
    pub(crate) splice_scale: i32,
    pub(crate) splice_mode: i32,
}

impl DbaFrameConfig {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn sony_66_stereo() -> Self {
        Self {
            frame_bytes: 192,
            channel_bytes: 96,
            js_enabled: true,
            initial_nunits: [27, 12],
            base_available_bits: [1133, 357],
            splice_scale: 96,
            splice_mode: 3,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn sony_105_stereo() -> Self {
        Self {
            frame_bytes: 304,
            channel_bytes: 152,
            js_enabled: false,
            initial_nunits: [28, 28],
            base_available_bits: [1197, 1197],
            splice_scale: 0,
            splice_mode: 0,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct DbaFrameEncoder {
    config: DbaFrameConfig,
    qmf: [DbaAnalysisFilterBank; 2],
    chconv: DbaChconvState,
    gain_mdct: [DbaGainMdctChannelState; 2],
}

#[cfg_attr(not(test), allow(dead_code))]
impl DbaFrameEncoder {
    pub(crate) fn new(config: DbaFrameConfig) -> Self {
        let chconv_threshold = if config.js_enabled { 1 } else { -1 };
        Self {
            config,
            qmf: std::array::from_fn(|_| DbaAnalysisFilterBank::new()),
            chconv: DbaChconvState::new(chconv_threshold),
            gain_mdct: std::array::from_fn(|_| DbaGainMdctChannelState::new()),
        }
    }

    pub(crate) fn encode_frame(
        &mut self,
        pcm: &[&[f32; 1024]; 2],
        output: &mut [u8],
    ) -> Result<(), i32> {
        if output.len() < self.config.frame_bytes {
            return Err(-1);
        }
        output[..self.config.frame_bytes].fill(0);

        let mut qmf_bands = [[[0.0f32; 256]; 4]; 2];
        for channel in 0..2 {
            self.qmf[channel].analysis(pcm[channel], &mut qmf_bands[channel]);
        }

        if self.config.js_enabled {
            dba_channel_convert(&mut qmf_bands, &mut self.chconv);
        }

        let mut gain_results: [Option<DbaGainMdctFrameResult>; 2] = [None, None];
        for channel in 0..2 {
            let channel_mode = self.channel_mode(channel);
            let input = dba_interleave_bands(&qmf_bands[channel]);
            let mode = if channel_mode != 0 {
                DbaGainScheduleMode {
                    channel_mode,
                    chconv_abs_modes: self.chconv.abs_modes,
                    chconv_modes: self.chconv.modes,
                }
            } else {
                DbaGainScheduleMode::default()
            };
            gain_results[channel] = Some(self.gain_mdct[channel].transform(&input, mode));
        }

        if self.config.js_enabled {
            self.encode_js_frame(gain_results, output)
        } else {
            self.encode_independent_frame(gain_results, output)
        }
    }

    fn channel_mode(&self, channel: usize) -> i32 {
        if self.config.js_enabled && channel == 1 {
            self.config.base_available_bits[0]
        } else {
            0
        }
    }

    fn channel_flags(&self) -> i32 {
        if self.config.js_enabled {
            self.chconv.previous_coefficient
        } else {
            0
        }
    }

    fn at3data_or_fallback(
        &self,
        gain: &DbaGainMdctFrameResult,
        channel: usize,
        available_bits: i32,
    ) -> DbaAt3DataResult {
        if available_bits < 0x28 {
            return dba_fallback_at3data();
        }
        let prior_gain_counts = dba_gain_event_counts(&gain.gain_side_info_ext);
        let params = DbaAt3DataParams {
            spectrum: &gain.spectrum,
            initial_nunits: self.config.initial_nunits[channel],
            available_bits,
            channel_mode: self.channel_mode(channel),
            prior_tone_counts: &prior_gain_counts,
            channel_flags: self.channel_flags(),
            param_1_0xb56: gain.band0_gain_count,
        };
        dba_at3data(params)
    }

    fn pack_channel(
        &self,
        data: &DbaAt3DataResult,
        gain: &DbaGainMdctFrameResult,
        channel: usize,
        output: &mut [u8],
        byte_offset: usize,
    ) -> Result<usize, i32> {
        crate::dsp::dba_pack::dba_pack_channel(
            &crate::dsp::dba_pack::DbaPackChannel {
                data,
                gain_side_info_ext: &gain.gain_side_info_ext,
                channel_mode: self.channel_mode(channel),
                channel_flags: self.channel_flags(),
            },
            self.chconv.abs_modes,
            output,
            byte_offset,
        )
    }

    fn encode_js_frame(
        &self,
        gain_results: [Option<DbaGainMdctFrameResult>; 2],
        output: &mut [u8],
    ) -> Result<(), i32> {
        let [Some(gain0), Some(gain1)] = gain_results else {
            return Err(-1);
        };
        let fixed_splice = dba_gain_event_counts(&gain1.gain_side_info_ext)[0];
        let ch0_available = dba_mainsub(DbaMainsubParams {
            tonal_spectrum: &gain0.spectrum,
            tonal_bfu_count: self.config.initial_nunits[0],
            nontonal_spectrum: &gain1.spectrum,
            nontonal_bfu_count: self.config.initial_nunits[1],
            base_position: self.config.base_available_bits[0],
            position_scale: self.config.splice_scale,
            mode: self.config.splice_mode,
            fixed_splice,
        });
        let data0 = self.at3data_or_fallback(&gain0, 0, ch0_available);
        let ch0_end = self.pack_channel(&data0, &gain0, 0, output, 0)?;
        let ch1_available = (self.config.frame_bytes as i32 - ch0_end as i32)
            .wrapping_mul(8)
            .wrapping_sub(0x1b);
        let data1 = self.at3data_or_fallback(&gain1, 1, ch1_available);
        self.pack_channel(&data1, &gain1, 1, output, ch0_end)?;
        output[ch0_end..self.config.frame_bytes].reverse();
        Ok(())
    }

    fn encode_independent_frame(
        &self,
        gain_results: [Option<DbaGainMdctFrameResult>; 2],
        output: &mut [u8],
    ) -> Result<(), i32> {
        for (channel, gain) in gain_results.into_iter().enumerate() {
            let Some(gain) = gain else {
                return Err(-1);
            };
            let data =
                self.at3data_or_fallback(&gain, channel, self.config.base_available_bits[channel]);
            self.pack_channel(
                &data,
                &gain,
                channel,
                output,
                channel * self.config.channel_bytes,
            )?;
        }
        Ok(())
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_event_counts(gain_side_info_ext: &[i32; DBA_GAIN_INFO_EXT_WORDS]) -> [i32; 4] {
    std::array::from_fn(|band| {
        gain_side_info_ext[DBA_GAIN_INFO_EXT_PREFIX + band * DBA_GAIN_INFO_STRIDE + 0x2a]
    })
}

const DBA_GAIN_PEAK_FLOOR: f32 = f32::from_bits(0x508b_771f);
const DBA_GAIN_EVENT_FLOOR: f32 = f32::from_bits(0x512e_54e6);
const DBA_GAIN_FALLBACK_LIMIT: f32 = f32::from_bits(0x568b_771f);
const DBA_GAIN_MAX_SCALE: f32 = f32::from_bits(0x27ea_f459);
const DBA_GAIN_SQRT_2: f32 = f32::from_bits(0x3fb5_04f3);
const DBA_GAIN_DECAY_RATIO: f32 = f32::from_bits(0x3fec_cccd);
const DBA_GAIN_RISE_RATIO: f32 = f32::from_bits(0x3fcc_cccd);

#[cfg_attr(not(test), allow(dead_code))]
fn dba_max_abs_words_by_band(input_bands: &[f32; 1024]) -> [[i32; 32]; 4] {
    let mut maxima = [[0i32; 32]; 4];
    #[allow(clippy::needless_range_loop)]
    for group in 0..32 {
        #[allow(clippy::needless_range_loop)]
        for band in 0..4 {
            let mut peak = 0u32;
            for sample in 0..8 {
                let idx = (group * 8 + sample) * 4 + band;
                peak = peak.max(input_bands[idx].to_bits().wrapping_shl(1));
            }
            maxima[band][group] = (peak >> 1) as i32;
        }
    }
    maxima
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_word_to_f32(word: i32) -> f32 {
    f32::from_bits(word as u32)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_f32_to_word(value: f32) -> i32 {
    value.to_bits() as i32
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_delta(current: f32, previous: f32) -> i32 {
    ((current * (1.0 / previous) * DBA_GAIN_SQRT_2).to_bits() >> 23) as i32 - 0x7f
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_mode_for_band(mode: DbaGainScheduleMode, band: usize) -> i32 {
    if mode.channel_mode == 0 {
        return band as i32;
    }
    if mode.chconv_abs_modes[band] == mode.chconv_modes[band] {
        -i32::from(mode.chconv_abs_modes[band] == 3)
    } else {
        5
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_sequence(
    previous_maxima: &[i32; 32],
    current_maxima: &[i32; 32],
    next_band_first: i32,
) -> [f32; 65] {
    let mut sequence = [0.0f32; 65];
    for (dst, &word) in sequence[..32].iter_mut().zip(previous_maxima) {
        *dst = dba_gain_word_to_f32(word);
    }
    for (dst, &word) in sequence[32..64].iter_mut().zip(current_maxima) {
        *dst = dba_gain_word_to_f32(word);
    }
    sequence[64] = dba_gain_word_to_f32(next_band_first);
    sequence
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_group_peaks(sequence: &[f32; 65], carry_peak: f32) -> ([f32; 9], usize, f32) {
    let mut peaks = [0.0f32; 9];
    peaks[0] = carry_peak;
    let mut cursor = 0usize;
    let mut peak = carry_peak;
    #[allow(clippy::needless_range_loop)]
    for slot in 1..=8 {
        let start = cursor;
        let mut next_cursor = cursor;
        peak = sequence[next_cursor];
        loop {
            let candidate_idx = next_cursor + 1;
            let candidate = sequence[candidate_idx];
            if candidate <= peak {
                next_cursor = candidate_idx;
                if start + 3 <= candidate_idx {
                    break;
                }
            } else {
                next_cursor = candidate_idx;
                peak = candidate;
                if candidate_idx >= start + 3 {
                    break;
                }
            }
        }
        cursor = next_cursor + 1;
        peaks[slot] = peak;
    }
    (peaks, cursor, peak)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_tail_peak(sequence: &[f32; 65], mut cursor: usize, limit: usize, mut peak: f32) -> f32 {
    while cursor < limit {
        let mut scan_peak = peak;
        while scan_peak < sequence[cursor] {
            scan_peak = sequence[cursor];
            cursor += 1;
            if limit <= cursor {
                return scan_peak;
            }
        }
        peak = scan_peak;
        cursor += 1;
    }
    peak
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_insert_decay_events(
    side_info_ext: &mut [i32; DBA_GAIN_INFO_EXT_WORDS],
    loc_base: usize,
    sequence: &[f32; 65],
    peaks: &[f32; 9],
    mut baseline: f32,
) -> (i32, usize) {
    let mut limit = baseline * DBA_GAIN_DECAY_RATIO;
    let mut budget = 4;
    let mut insert = 7usize;
    side_info_ext[loc_base + 7] = 32;

    let mut peak_idx = 8i32;
    while peak_idx >= 0 {
        let peak = peaks[peak_idx as usize];
        if baseline <= peak {
            if DBA_GAIN_EVENT_FLOOR < peak && limit < peak {
                let mut loc = peak_idx * 4;
                if peak_idx != 0
                    && sequence[loc as usize] < limit
                    && sequence[(loc - 1) as usize] < limit
                {
                    loc = (loc - 1) - i32::from(sequence[(loc - 2) as usize] < limit);
                }

                insert -= 1;
                side_info_ext[loc_base + insert] = loc;
                let mut delta = dba_gain_delta(peak, baseline);
                if budget < delta {
                    delta = budget;
                }
                side_info_ext[loc_base + insert + 8] = -delta;
                budget -= delta;
                if budget < 1 || insert == 5 {
                    break;
                }
            }
            limit = peak * DBA_GAIN_DECAY_RATIO;
            baseline = peak;
        }
        peak_idx -= 1;
    }

    (budget, insert)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_insert_rise_events(
    side_info_ext: &mut [i32; DBA_GAIN_INFO_EXT_WORDS],
    loc_base: usize,
    sequence: &[f32; 65],
    first_decay: usize,
    mut budget: i32,
    mode_for_band: i32,
    previous_peak: f32,
) -> usize {
    let mut count = 0usize;
    if budget <= 0 {
        return count;
    }

    let ratio = if mode_for_band == -1 {
        2.0
    } else {
        DBA_GAIN_RISE_RATIO
    };
    let mut baseline = previous_peak.max(sequence[0]);
    let mut cursor = 0usize;
    let mut limit = side_info_ext[loc_base + first_decay].max(0) as usize;
    let mut threshold = baseline * ratio;

    while cursor < limit {
        while baseline <= sequence[cursor + 1] {
            let peak = sequence[cursor + 1];
            if DBA_GAIN_EVENT_FLOOR < peak && threshold < peak {
                let event_loc = cursor as i32;
                side_info_ext[loc_base + count] = event_loc;
                let mut delta = dba_gain_delta(peak, baseline);
                let mut target = count;
                if count > 0
                    && side_info_ext[loc_base + count - 1] == event_loc - 1
                    && side_info_ext[loc_base + count + 7] <= delta
                {
                    target = count - 1;
                    side_info_ext[loc_base + target] = event_loc;
                    budget += side_info_ext[loc_base + count + 7];
                    delta += side_info_ext[loc_base + count + 7];
                }
                if budget < delta {
                    delta = budget;
                }
                budget -= delta;
                side_info_ext[loc_base + target + 8] = delta;
                count = target + 1;
                if budget < 1 || count == first_decay {
                    return count;
                }
                limit = side_info_ext[loc_base + first_decay].max(0) as usize;
            }

            threshold = peak * ratio;
            cursor += 1;
            baseline = peak;
            if limit <= cursor {
                return count;
            }
        }
        cursor += 1;
    }

    count
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_force_low_band_event(
    side_info_ext: &mut [i32; DBA_GAIN_INFO_EXT_WORDS],
    sequence: &[f32; 65],
    previous_peak_max: f32,
) -> Option<usize> {
    let band1_base = DBA_GAIN_INFO_EXT_PREFIX + DBA_GAIN_INFO_STRIDE;
    let band1_count = side_info_ext[band1_base + 0x2a];
    if band1_count <= 0 || DBA_GAIN_FALLBACK_LIMIT < previous_peak_max {
        return None;
    }

    let mut min_level = 4;
    let mut max_level = 4;
    for idx in 0..band1_count as usize {
        let level = side_info_ext[band1_base + idx];
        min_level = min_level.min(level);
        max_level = max_level.max(level);
    }
    if max_level - min_level <= 1 {
        return None;
    }

    let loc = side_info_ext[band1_base - 8];
    for &value in sequence.iter().take((loc + 1).max(0) as usize) {
        if DBA_GAIN_FALLBACK_LIMIT < value {
            return None;
        }
    }

    let band0_base = DBA_GAIN_INFO_EXT_PREFIX;
    side_info_ext[band0_base - 8] = loc;
    side_info_ext[band0_base] = 5;
    Some(1)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_finalize_gain_events(
    side_info_ext: &mut [i32; DBA_GAIN_INFO_EXT_WORDS],
    loc_base: usize,
    count: usize,
) {
    let mut level = 4;
    for idx in (0..count).rev() {
        level += side_info_ext[loc_base + idx + 8];
        side_info_ext[loc_base + idx + 8] = level;
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn dba_generate_gain_side_info(
    input_bands: &[f32; 1024],
    mode: DbaGainScheduleMode,
    state: &mut DbaGainScheduleState,
) {
    let current_maxima = dba_max_abs_words_by_band(input_bands);

    for band in (0..4).rev() {
        let base = DBA_GAIN_INFO_EXT_PREFIX + band * DBA_GAIN_INFO_STRIDE;
        let loc_base = base - 8;
        let previous_count = state.side_info_ext[base + 0x2a];
        let mut previous_maxima = [0i32; 32];
        previous_maxima.copy_from_slice(&state.side_info_ext[base + 8..base + 40]);
        let mut previous_peak_max = DBA_GAIN_PEAK_FLOOR.to_bits();
        for &word in &previous_maxima {
            previous_peak_max = previous_peak_max.max(word as u32);
        }

        state.side_info_ext[base + 8..base + 40].copy_from_slice(&current_maxima[band]);
        let next_band_first = current_maxima
            .get(band + 1)
            .map(|maxima| maxima[0])
            .unwrap_or(current_maxima[band][31]);
        let sequence = dba_gain_sequence(&previous_maxima, &current_maxima[band], next_band_first);
        let mode_for_band = dba_gain_mode_for_band(mode, band);
        let carry_peak = dba_gain_word_to_f32(state.side_info_ext[base + 0x29]);
        let (peaks, cursor, mut peak) = dba_gain_group_peaks(&sequence, carry_peak);
        state.side_info_ext[base + 0x29] = dba_gain_f32_to_word(peak);

        let limit = if mode_for_band > 0 {
            (8 - mode_for_band).max(0) as usize * 8
        } else {
            64
        };
        peak = dba_gain_tail_peak(&sequence, cursor, limit, peak).max(DBA_GAIN_PEAK_FLOOR);

        let (decay_budget, first_decay) = dba_gain_insert_decay_events(
            &mut state.side_info_ext,
            loc_base,
            &sequence,
            &peaks,
            peak,
        );
        let previous_peak = dba_gain_word_to_f32(state.side_info_ext[base + 0x28]);
        let mut rise_budget =
            0x83 - ((previous_peak * DBA_GAIN_MAX_SCALE * DBA_GAIN_SQRT_2).to_bits() >> 23) as i32;
        rise_budget = rise_budget.min(0xf) - decay_budget;

        let mut count = dba_gain_insert_rise_events(
            &mut state.side_info_ext,
            loc_base,
            &sequence,
            first_decay,
            rise_budget,
            mode_for_band,
            previous_peak,
        );

        for idx in first_decay..7 {
            state.side_info_ext[loc_base + count + 8] = state.side_info_ext[loc_base + idx + 8];
            state.side_info_ext[loc_base + count] = state.side_info_ext[loc_base + idx];
            count += 1;
        }

        dba_finalize_gain_events(&mut state.side_info_ext, loc_base, count);
        state.side_info_ext[base + 0x28] = previous_peak_max as i32;
        if band == 0 {
            state.band0_gain_count = previous_count;
            state.side_info_ext[DBA_GAIN_INFO_EXT_PREFIX + DBA_GAIN_BAND0_COUNT_COMPACT_OFFSET] =
                state.band0_gain_count;
        }

        if count == 0
            && band == 0
            && mode.channel_mode == 0
            && state.side_info_ext[base + 0x2a] == 0
            && let Some(forced_count) = dba_gain_force_low_band_event(
                &mut state.side_info_ext,
                &sequence,
                dba_gain_word_to_f32(previous_peak_max as i32),
            )
        {
            count = forced_count;
        }

        state.side_info_ext[base + 0x2a] = count as i32;
        state.side_info_ext[base + count] = 4;
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_level(gain_side_info: &[i32], band: usize) -> usize {
    gain_side_info
        .get(band * DBA_GAIN_INFO_STRIDE)
        .copied()
        .and_then(|level| usize::try_from(level).ok())
        .filter(|&level| level < dba::DBA_GAIN_TABLE.len())
        .unwrap_or(4)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_level_ext(gain_side_info_ext: &[i32], band: usize, offset: usize) -> usize {
    gain_side_info_ext
        .get(DBA_GAIN_INFO_EXT_PREFIX + band * DBA_GAIN_INFO_STRIDE + offset)
        .copied()
        .and_then(|level| usize::try_from(level).ok())
        .filter(|&level| level < 16)
        .unwrap_or(4)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_gain_interp_scale(index: i32) -> f32 {
    let frac = index & 7;
    let bits = (index as u32)
        .wrapping_mul(0x0010_0000)
        .wrapping_add(dba::DBA_GAIN_TABLE[16 + frac as usize].to_bits());
    f32::from_bits(bits)
}

fn dba_scale_gain_sample(value: f32, scale: f32) -> f32 {
    value * scale
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_scale_history_group(
    history: &mut [f32; DBA_GAIN_MDCT_HISTORY],
    band: usize,
    group: usize,
    scale: f32,
) {
    let base = group * 32 + band;
    for sample in 0..8 {
        let idx = base + sample * 4;
        history[idx] = dba_scale_gain_sample(history[idx], scale);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn dba_apply_scheduled_gain(gain_side_info_ext: &[i32], state: &mut DbaGainMdctState) {
    for band in (0..4).rev() {
        let base = DBA_GAIN_INFO_EXT_PREFIX + band * DBA_GAIN_INFO_STRIDE;
        let Some(&count_value) = gain_side_info_ext.get(base + 0x2a) else {
            continue;
        };
        let Ok(count) = usize::try_from(count_value) else {
            continue;
        };
        if count == 0 || count > 7 || base < 8 || base + count >= gain_side_info_ext.len() {
            continue;
        }

        let mut level = dba_gain_level_ext(gain_side_info_ext, band, 0) as i32;
        let mut group = 0usize;
        for event in 0..count {
            let loc = gain_side_info_ext[base - 8 + event].clamp(0, 32) as usize;
            if level != 4 {
                let scale = dba::DBA_GAIN_TABLE[level as usize];
                while group < loc.min(32) {
                    dba_scale_history_group(&mut state.history, band, group, scale);
                    group += 1;
                }
                if loc < 32 {
                    let idx = loc * 32 + band;
                    state.history[idx] = dba_scale_gain_sample(state.history[idx], scale);
                }
            }

            let next_level = gain_side_info_ext[base + event + 1].clamp(0, 15);
            if loc < 32 {
                let delta = next_level - level;
                for sample in 1..8 {
                    let interp = dba_gain_interp_scale(level * 8 + delta * sample as i32);
                    let idx = loc * 32 + sample * 4 + band;
                    state.history[idx] = dba_scale_gain_sample(state.history[idx], interp);
                }
            }
            group = (loc + 1).min(32);
            level = next_level;
        }

        if level != 4 {
            let scale = dba::DBA_GAIN_TABLE[level as usize];
            while group < 32 {
                dba_scale_history_group(&mut state.history, band, group, scale);
                group += 1;
            }
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_fcb_from_frtbl_negative(index: i32) -> f32 {
    dba::DBA_FCB[(128 + index) as usize]
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_fcb_end_negative(index: i32) -> f32 {
    dba::DBA_FCB[(127 + index) as usize]
}

fn dba_mdct_after_scheduled_gain(
    pre_mdct_bands: &[f32; 1024],
    gain_side_info: &[i32],
    state: &mut DbaGainMdctState,
) -> [f32; 1024] {
    let mut scratch = [0.0f32; DBA_MDCT_SCRATCH];
    let gains = [
        dba::DBA_GAIN_TABLE[dba_gain_level(gain_side_info, 0)],
        dba::DBA_GAIN_TABLE[dba_gain_level(gain_side_info, 1)],
        dba::DBA_GAIN_TABLE[dba_gain_level(gain_side_info, 2)],
        dba::DBA_GAIN_TABLE[dba_gain_level(gain_side_info, 3)],
    ];

    for group in 0..128 {
        let perm = dba::DBA_MDCT_PERM[group] as usize;
        let fr = perm * 4;
        let fr0 = dba::DBA_FRTBL[fr] as f64;
        let fr1 = dba::DBA_FRTBL[fr + 1] as f64;
        let fr2 = dba::DBA_FRTBL[fr + 2] as f64;
        let fr3 = dba::DBA_FRTBL[fr + 3] as f64;

        for lane in 0..4 {
            let gain = gains[lane] as f64;
            let prev = state.history[0x400 + group * 4 + lane] as f64;
            let mirrored = state.history[(127 - perm) * 4 + lane] as f64;
            let forward = state.history[0x200 + perm * 4 + lane] as f64;
            let folded = (mirrored * fr0 + forward) * fr2;
            let next_history = (forward * fr0 - mirrored) * fr1;
            state.history[0x400 + group * 4 + lane] = next_history as f32;

            let high = (folded - gain * prev) * fr3;
            scratch[group * 8 + 4 + lane] = high as f32;
            scratch[group * 8 + lane] = (folded + gain * prev + high) as f32;
        }
    }

    state.history[..1024].copy_from_slice(pre_mdct_bands);

    for group in 0..64 {
        let coeff = dba_fcb_from_frtbl_negative(-128 + group as i32 * 2) as f64;
        let base = group * 16;
        for lane in 0..4 {
            let a = scratch[base + lane] as f64;
            let b = scratch[base + 4 + lane] as f64;
            let c = scratch[base + 8 + lane] as f64;
            let d = scratch[base + 12 + lane] as f64;
            let high = (b - d) * coeff;
            let low = (a - c) * coeff;
            scratch[base + 12 + lane] = high as f32;
            scratch[base + lane] = (a + c + high) as f32;
            scratch[base + 8 + lane] = low as f32;
            scratch[base + 4 + lane] = (b + d + low) as f32;
        }
    }

    let mut step = 4usize;
    while step != 0x80 {
        let mut coeff_index = step as i32 / 2 - 0x80;
        let mut base = 0usize;
        while coeff_index < 1 {
            let stop = base + step * 4;
            let coeff = dba_fcb_end_negative(coeff_index) as f64;
            while base != stop {
                for lane in 0..4 {
                    let lo = scratch[base + lane] as f64;
                    let lo_pair = scratch[base + 4 + lane] as f64;
                    let hi = scratch[base + step * 4 + lane] as f64;
                    let hi_pair = scratch[base + step * 4 + 4 + lane] as f64;
                    scratch[base + step * 4 + lane] = ((lo - hi) * coeff) as f32;
                    scratch[base + step * 4 + 4 + lane] = ((lo_pair - hi_pair) * coeff) as f32;
                    scratch[base + lane] = (lo + hi) as f32;
                    scratch[base + 4 + lane] = (lo_pair + hi_pair) as f32;
                }
                base += 8;
            }

            let mut tail = step as i32 * 2 - 1;
            base -= step * 4;
            while tail > 0 {
                let tail_base = tail as usize * 4;
                for lane in 0..4 {
                    scratch[base + lane] = (scratch[base + tail_base + lane] as f64
                        + scratch[base + lane] as f64)
                        as f32;
                    scratch[base + 4 + lane] = (scratch[base + tail_base + lane - 4] as f64
                        + scratch[base + 4 + lane] as f64)
                        as f32;
                    scratch[base + 8 + lane] = (scratch[base + tail_base + lane - 8] as f64
                        + scratch[base + 8 + lane] as f64)
                        as f32;
                    scratch[base + 12 + lane] = (scratch[base + tail_base + lane - 12] as f64
                        + scratch[base + 12 + lane] as f64)
                        as f32;
                }
                base += 16;
                tail -= 8;
            }

            base += step * 4;
            coeff_index += step as i32;
        }
        step *= 2;
    }

    const INV_SQRT_2: f64 = 0.70710677;
    for idx in 0..512 {
        let a = scratch[idx] as f64;
        let b = scratch[512 + idx] as f64;
        scratch[idx] = (a + b) as f32;
        scratch[512 + idx] = ((a - b) * INV_SQRT_2) as f32;
    }

    let mut output = [0.0f32; 1024];
    for group in 0..128 {
        for lane in 0..4 {
            scratch[group * 4 + lane] =
                (scratch[group * 4 + lane] as f64 + scratch[1020 - group * 4 + lane] as f64) as f32;
        }

        output[group] = scratch[group * 4];
        output[255 - group] = scratch[1020 - group * 4];
        output[256 + group] = scratch[1021 - group * 4];
        output[511 - group] = scratch[group * 4 + 1];
        output[512 + group] = scratch[group * 4 + 2];
        output[767 - group] = scratch[1022 - group * 4];
        output[768 + group] = scratch[1023 - group * 4];
        output[1023 - group] = scratch[group * 4 + 3];
    }

    output
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn dba_gain_mdct(
    pre_mdct_bands: &[f32; 1024],
    gain_side_info: &[i32],
    gain_side_info_ext: &[i32],
    state: &mut DbaGainMdctState,
) -> [f32; 1024] {
    dba_apply_scheduled_gain(gain_side_info_ext, state);
    dba_mdct_after_scheduled_gain(pre_mdct_bands, gain_side_info, state)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_max_abs_prepass(spectrum: &[f32]) -> [u32; 256] {
    let mut idsfs = [0u32; 256];
    for (idsf, samples) in idsfs.iter_mut().zip(spectrum[..1024].chunks_exact(4)) {
        let peak = samples
            .iter()
            .map(|sample| sample.to_bits().wrapping_mul(2))
            .max()
            .unwrap();
        let fraction = peak & 0x00ff_ffff;
        let mut value = (peak >> 24).wrapping_mul(3).wrapping_sub(0x16c);
        if fraction > 0x0096_5fe9 {
            value = value.wrapping_add(1);
        }
        if fraction < 0x0042_8a30 {
            value = value.wrapping_sub(1);
        }
        *idsf = value & 0u32.wrapping_sub(u32::from(value < 0x40));
    }
    idsfs
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
struct DbaPreludeParams<'a> {
    idsfs: &'a [u32; 256],
    initial_nunits: i32,
    available_bits: i32,
    channel_mode: i32,
    prior_tone_counts: &'a [i32; 4],
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct DbaPreludeResult {
    cumulative_idsfs: [i32; 32],
    idsf_sums: [i32; 32],
    strong_idsf_counts: [i32; 33],
    allocations: [i32; 32],
    nunits: i32,
    ntones: i32,
    qpoint: i32,
    bit_budget: i32,
    high_rate: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_allocation_prelude(params: DbaPreludeParams<'_>) -> DbaPreludeResult {
    let mut cumulative_idsfs = [0i32; 32];
    let mut idsf_sums = [0i32; 32];
    let mut allocations = [0i32; 32];
    let mut strong_idsf_counts = [0i32; 33];
    let mut idsf_index = 0usize;
    let mut cumulative_idsf = 0i32;
    let mut strong_idsf_count = 0i32;
    let mut global_peak = 0i32;
    let mut nunits = params.initial_nunits;

    for bfu in 0..32 {
        let end = (dba::DBA_QTEND[bfu] >> 2) as usize;
        let mut sum = 0i32;
        let mut peak = 0i32;
        for &idsf in &params.idsfs[idsf_index..end] {
            let idsf = idsf as i32;
            sum = sum.wrapping_add(idsf);
            peak = peak.max(idsf);
            strong_idsf_count += i32::from(idsf > 7);
        }
        idsf_index = end;
        idsf_sums[bfu] = sum;

        if global_peak < peak {
            global_peak = peak;
            if params.channel_mode == 0 {
                let wide_threshold = ((dba::DBA_QTSTART[bfu] >> 7) & 1) != 0 && bfu < 22;
                let threshold = if wide_threshold { 16 } else { 8 };
                if params.available_bits > strong_idsf_count * threshold {
                    let candidate = if bfu <= 26 {
                        28
                    } else {
                        (bfu as i32 + 2).min(32)
                    };
                    nunits = nunits.max(candidate);
                }
            }
        } else if peak < 3 || sum < dba::DBA_ATH_THRESHOLD[bfu] {
            peak = 0;
            sum = 0;
        }

        cumulative_idsf = cumulative_idsf.wrapping_add(sum);
        cumulative_idsfs[bfu] = cumulative_idsf;
        allocations[bfu] = peak;
        strong_idsf_counts[bfu + 1] = strong_idsf_count;
    }

    let nunits_index = nunits as usize;
    let selected_strong_idsfs = strong_idsf_counts[nunits_index];
    let qpoint = if params.available_bits * 2 < selected_strong_idsfs * 12 {
        11
    } else {
        10
    };
    let ntones = (dba::DBA_QTSTART[nunits_index] + 0xff) >> 8;
    let prior_tones = params.prior_tone_counts[..ntones as usize]
        .iter()
        .copied()
        .sum::<i32>();
    let bit_budget = params
        .available_bits
        .wrapping_sub(ntones * 3 + prior_tones * 9)
        .wrapping_sub(nunits * 3);

    DbaPreludeResult {
        cumulative_idsfs,
        idsf_sums,
        strong_idsf_counts,
        allocations,
        nunits,
        ntones,
        qpoint,
        bit_budget,
        high_rate: selected_strong_idsfs * 32 < bit_budget,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DbaToneComponent {
    pub(crate) quantized: [i32; 4],
    pub(crate) position: i32,
    pub(crate) idsf: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DbaToneBank {
    pub(crate) active_quarters: [i32; 4],
    pub(crate) idwl: i32,
    pub(crate) width: i32,
    pub(crate) groups: [Vec<usize>; 16],
}

impl DbaToneBank {
    pub(crate) fn new(idwl: i32) -> Self {
        Self {
            active_quarters: [0; 4],
            idwl,
            width: 3,
            groups: std::array::from_fn(|_| Vec::new()),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DbaToneTable {
    pub(crate) banks: [DbaToneBank; 2],
    pub(crate) components: Vec<DbaToneComponent>,
}

impl DbaToneTable {
    fn new() -> Self {
        Self {
            banks: [DbaToneBank::new(5), DbaToneBank::new(7)],
            components: Vec::new(),
        }
    }

    fn set_component(&mut self, slot: usize, component: DbaToneComponent) {
        if self.components.len() <= slot {
            self.components
                .resize(slot + 1, DbaToneComponent::default());
        }
        self.components[slot] = component;
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
struct DbaHighRateResult {
    residual_spectrum: Vec<f32>,
    residual_idsfs: [u32; 256],
    presence: [i32; 32],
    allocations: [i32; 32],
    ntones: i32,
    tone_count: i32,
    tone_cost: i32,
    bit_budget: i32,
    tone_table: DbaToneTable,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
struct DbaLowRateResult {
    residual_spectrum: Vec<f32>,
    residual_idsfs: [u32; 256],
    allocation_scores: [i32; 32],
    presence: [i32; 32],
    allocations: [i32; 32],
    ntones: i32,
    coding_layout: i32,
    tone_mode: i32,
    tone_cost: i32,
    bit_budget: i32,
    tone_table: DbaToneTable,
}

fn dba_idsf_from_peak_bits(peak: u32) -> u32 {
    let fraction = peak & 0x00ff_ffff;
    let mut value = (peak >> 24).wrapping_mul(3).wrapping_sub(0x16c);
    if fraction > 0x0096_5fe9 {
        value = value.wrapping_add(1);
    }
    if fraction < 0x0042_8a30 {
        value = value.wrapping_sub(1);
    }
    value & 0u32.wrapping_sub(u32::from(value < 0x40))
}

fn dba_idsf_for_four(samples: &[f32]) -> u32 {
    let peak = samples[..4]
        .iter()
        .map(|sample| sample.to_bits().wrapping_mul(2))
        .max()
        .unwrap();
    dba_idsf_from_peak_bits(peak)
}

fn dba_tone_scale(idsf: i32, idwl: i32) -> f32 {
    let phase = (idsf as u32).wrapping_mul(0x002b_0000);
    let table_index = ((phase >> 23) as i32 + idwl) * 3 - idsf - 1;
    f32::from_bits(
        DBA_NORM_FACT[table_index as usize]
            .to_bits()
            .wrapping_sub(phase & 0x7f80_0000),
    )
}

fn dba_tone_inverse(scale: f32) -> f32 {
    1.0f32 / scale
}

fn dba_tone_sub_quantized(sample: f32, quantized: i32, inverse: f32) -> f32 {
    sample - quantized as f32 * inverse
}

fn dba_tone_add_quantized(sample: f32, quantized: i32, inverse: f32) -> f32 {
    sample + quantized as f32 * inverse
}

fn dba_restore_tone(residual: &mut [f32], component: DbaToneComponent, idwl: i32, width: i32) {
    let scale = dba_tone_scale(component.idsf, idwl);
    let inverse = dba_tone_inverse(scale);
    let nbits = (dba::dba_hcspec_table(idwl)[0] >> 16) as i32;
    for index in (0..=width as usize).rev() {
        let mut quantized = component.quantized[index];
        if 1 << (nbits - 1) <= quantized {
            quantized -= 1 << nbits;
        }
        let position = component.position as usize + index;
        residual[position] = dba_tone_add_quantized(residual[position], quantized, inverse);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_high_rate_tone_extract(
    spectrum: &[f32],
    initial_idsfs: &[u32; 256],
    prelude: &DbaPreludeResult,
) -> Option<DbaHighRateResult> {
    if !prelude.high_rate {
        return None;
    }

    let mut residual = spectrum[..1024].to_vec();
    let mut tone_table = DbaToneTable::new();
    let mut tone_count = 0i32;
    let mut next_slot = 0usize;
    let mut tone_cost = 0x16i32;
    let mut ntones = prelude.ntones;
    let floor_ntones = dba::DBA_QTSTART[prelude.nunits as usize] >> 8;
    if ntones < floor_ntones {
        tone_cost += (floor_ntones - ntones) * 3;
        ntones = floor_ntones;
    }

    let threshold = dba::DBA_SCALE_FACTOR_TABLE[18].to_bits().wrapping_mul(2);
    let scan_limit = dba::DBA_QTSTART[prelude.nunits as usize] as usize;

    'passes: loop {
        let count_before = tone_count;
        let mut position = 0usize;
        while position < scan_limit {
            while position < scan_limit && residual[position].to_bits().wrapping_mul(2) < threshold
            {
                position += 1;
            }
            if position >= scan_limit {
                break;
            }

            let component_position = position.min(1020);
            let candidate_idsf = dba_idsf_for_four(&residual[component_position..]) as i32;
            let preferred_bank = usize::from(candidate_idsf > 30);
            let group = position >> 6;
            let mut bank = preferred_bank;
            let mut component_slot = Some(next_slot);

            if tone_table.banks[bank].groups[group].len() == 7 {
                bank = 1 - bank;
                if tone_table.banks[bank].groups[group].len() == 7 {
                    bank = 1 - bank;
                    let mut weakest_idsf = candidate_idsf;
                    let mut weakest_index = None;
                    for (index, &slot) in tone_table.banks[bank].groups[group].iter().enumerate() {
                        let idsf = tone_table.components[slot].idsf;
                        if idsf < weakest_idsf {
                            weakest_idsf = idsf;
                            weakest_index = Some(index);
                        }
                    }
                    component_slot = weakest_index.map(|index| {
                        let slot = tone_table.banks[bank].groups[group][index];
                        let component = tone_table.components[slot];
                        dba_restore_tone(
                            &mut residual,
                            component,
                            tone_table.banks[bank].idwl,
                            tone_table.banks[bank].width,
                        );
                        tone_table.banks[bank].groups[group].remove(index);
                        next_slot = next_slot.saturating_sub(1);
                        tone_count -= 1;
                        slot
                    });
                }
            }

            if let Some(slot) = component_slot {
                tone_count += 1;
                next_slot += 1;
                let quarter = position >> 8;
                if tone_table.banks[bank].active_quarters[quarter] == 0 {
                    tone_table.banks[bank].active_quarters[quarter] = 1;
                    tone_cost += 0xc;
                }
                tone_table.banks[bank].groups[group].push(slot);

                let current_idsf = tone_table
                    .components
                    .get(slot)
                    .map_or(0, |component| component.idsf);
                let idwl = tone_table.banks[bank].idwl;
                let idsf = set_best_idsf4_tone(
                    &residual[component_position..component_position + 4],
                    idwl,
                    4,
                    current_idsf,
                );
                let scale = dba_tone_scale(idsf, idwl);
                let inverse = dba_tone_inverse(scale);
                let mask = DBA_HUF_MASK[(idwl - 2) as usize] as i32;
                let mut component = DbaToneComponent {
                    position: component_position as i32,
                    idsf,
                    ..DbaToneComponent::default()
                };
                for index in 0..4 {
                    let sample = residual[component_position + index];
                    let quantized = dba_tone_quantized_sample(sample, scale);
                    component.quantized[index] = quantized & mask;
                    residual[component_position + index] =
                        dba_tone_sub_quantized(sample, quantized, inverse);
                }
                tone_table.set_component(slot, component);

                tone_cost += 0x1c + bank as i32 * 8;
                if tone_count > 0x3f || prelude.bit_budget.wrapping_sub(200) < tone_cost {
                    break 'passes;
                }
                position += 4;
            } else {
                position += 1;
            }
        }

        if count_before == tone_count {
            break;
        }
    }

    let mut residual_idsfs = *initial_idsfs;
    let mut presence = prelude.cumulative_idsfs;
    let mut allocations = prelude.allocations;
    let mut idsf_index = 0usize;
    for bfu in 0..prelude.nunits as usize {
        let end = (dba::DBA_QTEND[bfu] >> 2) as usize;
        let mut peak = 0u32;
        #[allow(clippy::needless_range_loop)]
        for group in idsf_index..end {
            let start = group * 4;
            let idsf = dba_idsf_for_four(&residual[start..]);
            residual_idsfs[group] = idsf;
            peak = peak.max(idsf);
        }
        idsf_index = end;
        allocations[bfu] = peak as i32;
        presence[bfu] = i32::from(peak != 0);
    }

    Some(DbaHighRateResult {
        residual_spectrum: residual,
        residual_idsfs,
        presence,
        allocations,
        ntones,
        tone_count,
        tone_cost,
        bit_budget: prelude.bit_budget.wrapping_sub(tone_cost),
        tone_table,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_low_rate_restore_component(residual: &mut [f32], component: DbaToneComponent) {
    let inverse = dba_tone_inverse(dba_tone_scale(component.idsf, 3));
    for index in (0..4).rev() {
        let mut quantized = component.quantized[index];
        if quantized >= 4 {
            quantized -= 8;
        }
        let position = component.position as usize + index;
        residual[position] = dba_tone_add_quantized(residual[position], quantized, inverse);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_low_rate_component_cost(component: &DbaToneComponent) -> i32 {
    component.quantized.iter().fold(12, |cost, &value| {
        cost + (dba::DBA_HCSPEC03[value as usize] >> 16) as i32
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_low_rate_initial_allocations(prelude: &DbaPreludeResult) -> ([i32; 32], [i32; 32]) {
    let mut scores = [0i32; 32];
    let mut presence = prelude.cumulative_idsfs;
    let total_idsf = prelude.cumulative_idsfs[31];
    for bfu in 0..prelude.nunits as usize {
        let peak = prelude.allocations[bfu];
        let mut score = peak;
        let mut word_length = 0;
        if peak > 2 {
            score = peak.wrapping_mul(0x100).wrapping_sub(total_idsf);
            word_length = (score >> prelude.qpoint).clamp(1, 7);
        }
        scores[bfu] = score;
        presence[bfu] = word_length;
    }
    (scores, presence)
}

#[cfg_attr(not(test), allow(dead_code))]
fn dba_low_rate_allocate(
    spectrum: &[f32],
    initial_idsfs: &[u32; 256],
    prelude: &DbaPreludeResult,
    channel_mode: i32,
) -> Option<DbaLowRateResult> {
    if prelude.high_rate {
        return None;
    }

    let (allocation_scores, mut presence) = dba_low_rate_initial_allocations(prelude);
    let mut residual = spectrum[..1024].to_vec();
    let residual_idsfs = *initial_idsfs;
    let mut allocations = prelude.allocations;
    let mut tone_table = DbaToneTable {
        banks: [DbaToneBank::new(3), DbaToneBank::new(0)],
        components: Vec::new(),
    };
    tone_table.banks[1].width = 0;

    if channel_mode != 0 {
        return Some(DbaLowRateResult {
            residual_spectrum: residual,
            residual_idsfs,
            allocation_scores,
            presence,
            allocations,
            ntones: prelude.ntones,
            coding_layout: 0,
            tone_mode: 0,
            tone_cost: 0,
            bit_budget: prelude.bit_budget,
            tone_table,
        });
    }

    let mut fixed_cost = 8i32;
    let mut spectral_cost = 0i32;
    let mut zero_tail_count = 0i32;
    let mut start_bfu = usize::from(prelude.bit_budget < 0x44c) * 8;
    let mut tone_count = 0usize;

    while start_bfu < prelude.nunits as usize {
        let nsps = dba::DBA_NSPS[start_bfu];
        if 64 - (nsps >> 4) < tone_count as i32
            || prelude.bit_budget.wrapping_sub(600)
                < fixed_cost.wrapping_add((tone_count * 0x18) as i32)
        {
            break;
        }

        if presence[start_bfu] != 0 {
            let mut search_position = dba::DBA_QTSTART[start_bfu];
            let threshold_band = if nsps == 0x20 {
                1
            } else {
                search_position >> 8
            };
            let threshold_index = threshold_band * 8
                + if allocation_scores[start_bfu] > 0x4ff {
                    presence[start_bfu]
                } else {
                    0
                };
            let threshold_idsf =
                allocations[start_bfu].wrapping_sub(dba::DBA_TONE_THRESH[threshold_index as usize]);

            if tone_count > 0 {
                let previous_position = tone_table.components[tone_count - 1].position;
                if search_position < previous_position + 4 {
                    allocations[start_bfu] = -allocations[start_bfu];
                    let overlap = previous_position - search_position + 4;
                    presence[start_bfu] = overlap;
                    presence[start_bfu - 1] = presence[start_bfu - 1] + overlap - 4;
                    search_position = previous_position + 4;
                }
            }

            let end = dba::DBA_QTEND[start_bfu];
            let threshold = dba::DBA_SCALE_FACTOR_TABLE[threshold_idsf.max(0) as usize]
                .to_bits()
                .wrapping_mul(2);
            let candidate_limit = (nsps + 8) >> 4;
            let mut candidates = Vec::new();
            let mut candidate_overflow = false;

            while search_position < end {
                while search_position < end
                    && residual[search_position as usize].to_bits().wrapping_mul(2) < threshold
                {
                    search_position += 1;
                }
                if search_position >= end {
                    break;
                }
                if candidates.len() == candidate_limit as usize {
                    candidate_overflow = true;
                    break;
                }
                candidates.push(search_position);
                search_position += 4;
            }
            if candidate_overflow {
                candidates.clear();
            }

            if !candidates.is_empty() {
                if allocations[start_bfu] >= 0 {
                    allocations[start_bfu] = -allocations[start_bfu];
                    presence[start_bfu] = 0;
                }

                for candidate in candidates {
                    let mut position = candidate.min(0x3fc);
                    let mut width = 4i32;
                    if tone_count > 0
                        && tone_table.components[tone_count - 1].position + 4 == position
                    {
                        while width > 1
                            && residual[(position - 1) as usize].abs()
                                >= residual[(position + 3) as usize].abs()
                        {
                            position -= 1;
                            width -= 1;
                        }
                    }

                    let mut group = (position >> 6) as usize;
                    if tone_table.banks[0].groups[group].len() == 7 {
                        continue;
                    }

                    let idsf = set_best_idsf4_tone(
                        &residual[position as usize..position as usize + 4],
                        3,
                        4,
                        0,
                    );
                    let scale = dba_tone_scale(idsf, 3);
                    let inverse = dba_tone_inverse(scale);
                    let mut component = DbaToneComponent {
                        position,
                        idsf,
                        ..DbaToneComponent::default()
                    };
                    for index in 0..4 {
                        let sample = residual[position as usize + index];
                        let quantized = dba_tone_quantized_sample(sample, scale);
                        component.quantized[index] = quantized & 7;
                        residual[position as usize + index] =
                            dba_tone_sub_quantized(sample, quantized, inverse);
                    }

                    if component.quantized[0] == 0 && position <= 0x3fc {
                        let leading_zeros = component
                            .quantized
                            .iter()
                            .position(|&value| value != 0)
                            .unwrap_or(4);
                        if position + (leading_zeros as i32) < end {
                            if position + (leading_zeros as i32) < 0x3fd {
                                component.quantized.copy_within(leading_zeros.., 0);
                                component.quantized[4 - leading_zeros..].fill(0);
                                component.position += leading_zeros as i32;
                                position = component.position;
                            }
                            group = (position >> 6) as usize;
                        } else {
                            dba_low_rate_restore_component(&mut residual, component);
                            continue;
                        }
                    }

                    spectral_cost += dba_low_rate_component_cost(&component);
                    presence[start_bfu] += width;
                    let quarter = group >> 2;
                    if tone_table.banks[0].active_quarters[quarter] == 0 {
                        tone_table.banks[0].active_quarters[quarter] = 1;
                        fixed_cost += 0xc;
                    }
                    let slot = tone_count;
                    tone_table.banks[0].groups[group].push(slot);
                    zero_tail_count += i32::from(component.quantized[3] == 0);
                    tone_table.set_component(slot, component);
                    tone_count += 1;
                }
            }
        }

        start_bfu += 1;
    }

    if tone_count == 0 {
        spectral_cost = 0;
    } else if tone_count == 1
        && (prelude.bit_budget < 0x44c || tone_table.components[0].position < 0x80)
    {
        dba_low_rate_restore_component(&mut residual, tone_table.components[0]);
        tone_count = 0;
        spectral_cost = 0;
    }

    let mut coding_layout = 0i32;
    let mut tone_mode = 0i32;
    let mut tone_cost = 0i32;
    if tone_count != 0 {
        tone_mode = 1;
        let mut fixed_spectrum_cost = (tone_count * 0x18) as i32;
        if zero_tail_count == tone_count as i32 {
            loop {
                let width = tone_table.banks[0].width as usize;
                let mut stop = false;
                for component in tone_table.components[..tone_count].iter_mut().rev() {
                    if component.quantized[width] != 0 {
                        component.position += 1;
                        component.quantized.copy_within(1..=width, 0);
                    }
                    if component.quantized[width - 1] != 0
                        && (component.quantized[0] != 0 || (component.position & 0x3f) == 0x3f)
                    {
                        stop = true;
                    }
                }
                tone_table.banks[0].width -= 1;
                if stop {
                    break;
                }
            }
            let removed = (3 - tone_table.banks[0].width) * tone_count as i32;
            spectral_cost -= removed;
            fixed_spectrum_cost -= removed * 3;
        } else {
            let nonzero_tail = tone_count as i32 - zero_tail_count;
            let extra_metadata = nonzero_tail * 0xc;
            let signed_tail_count = nonzero_tail * 2 - zero_tail_count;
            let alternate_spectral = spectral_cost + extra_metadata + signed_tail_count;
            let alternate_fixed = fixed_spectrum_cost + extra_metadata + signed_tail_count * 3;
            if alternate_spectral.min(alternate_fixed) < spectral_cost.min(fixed_spectrum_cost) {
                tone_table.banks[1] = tone_table.banks[0].clone();
                let mut valid = true;
                let mut split_count = tone_count;
                for slot in 0..tone_count {
                    if tone_table.components[slot].quantized[3] != 0 {
                        let position = tone_table.components[slot].position + 2;
                        let quarter = (position >> 8) as usize;
                        let group = (position >> 6) as usize;
                        if tone_table.banks[1].active_quarters[quarter] == 0
                            || split_count == 64
                            || tone_table.banks[1].groups[group].len() == 7
                            || position >= 0x3fe
                        {
                            valid = false;
                            break;
                        }
                        split_count += 1;
                    }
                }

                if valid {
                    for slot in 0..tone_count {
                        if tone_table.components[slot].quantized[3] != 0 {
                            let source = tone_table.components[slot];
                            let split = DbaToneComponent {
                                quantized: [source.quantized[2], source.quantized[3], 0, 0],
                                position: source.position + 2,
                                idsf: source.idsf,
                            };
                            tone_table.components[slot].quantized[2] = 0;
                            tone_table.components[slot].quantized[3] = 0;
                            let new_slot = tone_table.components.len();
                            let group = (split.position >> 6) as usize;
                            tone_table.banks[1].groups[group].push(new_slot);
                            tone_table.components.push(split);
                        }
                    }
                    spectral_cost = alternate_spectral + split_count as i32;
                    fixed_spectrum_cost = alternate_fixed + split_count as i32 * 3;
                    tone_count = split_count;
                    loop {
                        let width = tone_table.banks[0].width as usize;
                        let mut stop = false;
                        for component in tone_table.components[..tone_count].iter_mut().rev() {
                            if component.quantized[width] != 0 {
                                component.position += 1;
                                component.quantized.copy_within(1..=width, 0);
                            }
                            if component.quantized[width - 1] != 0
                                && (component.quantized[0] != 0
                                    || (component.position & 0x3f) == 0x3f)
                            {
                                stop = true;
                            }
                        }
                        tone_table.banks[0].width -= 1;
                        if stop {
                            break;
                        }
                    }
                    let removed = (3 - tone_table.banks[0].width) * tone_count as i32;
                    spectral_cost -= removed;
                    fixed_spectrum_cost -= removed * 3;
                }
            }
        }

        let original_spectral = spectral_cost;
        if fixed_spectrum_cost < spectral_cost {
            spectral_cost = fixed_spectrum_cost;
        }
        coding_layout = i32::from(fixed_spectrum_cost < original_spectral);
        tone_cost = fixed_cost + prelude.ntones + spectral_cost;
    }

    Some(DbaLowRateResult {
        residual_spectrum: residual,
        residual_idsfs,
        allocation_scores,
        presence,
        allocations,
        ntones: prelude.ntones,
        coding_layout,
        tone_mode,
        tone_cost,
        bit_budget: prelude.bit_budget.wrapping_sub(tone_cost),
        tone_table,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
struct DbaBumpResult {
    idsfs: Vec<u32>,
    presence: [i32; 32],
    allocations: [i32; 32],
    scores: [i32; 33],
}

#[cfg_attr(not(test), allow(dead_code))]
#[allow(clippy::too_many_arguments)]
fn dba_allocation_bump(
    spectrum: &[f32],
    idsfs: &[u32],
    presence_in: &[i32; 32],
    allocations_in: &[i32; 32],
    scores_in: &[i32; 33],
    idsf_sums: &[i32; 32],
    nunits: i32,
    qpoint: i32,
    available_bits: i32,
) -> DbaBumpResult {
    let mut idsfs = idsfs.to_vec();
    let mut presence = *presence_in;
    let mut allocations = *allocations_in;
    let mut scores = *scores_in;

    for band in 0..nunits as usize {
        let allocation = allocations[band];
        if allocation < 0 {
            let mut peak_idsf = 0i32;
            let end = dba::DBA_QTEND[band];
            let start = (dba::DBA_QTSTART[band] >> 2) as usize;
            let mut idsf_sum = 0i32;
            #[allow(clippy::needless_range_loop)]
            for group in start..(end >> 2) as usize {
                let position = group * 4;
                let s0 = spectrum[position].to_bits().wrapping_mul(2);
                let s1 = spectrum[position + 1].to_bits().wrapping_mul(2);
                let s2 = spectrum[position + 2].to_bits().wrapping_mul(2);
                let s3 = spectrum[position + 3].to_bits().wrapping_mul(2);
                let mut peak = s0.max(s1);
                peak = peak.max(s2);
                peak = peak.max(s3);
                let exponent = (peak >> 24) as i32 * 3;
                let mut idsf = exponent - 0x16c;
                if 0x965fe9 < (peak & 0xffffff) {
                    idsf = exponent - 0x16b;
                }
                let idsf = idsf - i32::from((peak & 0xffffff) < 0x428a30);
                let idsf = idsf & 0i32.wrapping_sub(i32::from((idsf as u32) < 0x40));
                idsfs[group] = idsf as u32;
                peak_idsf = peak_idsf.max(idsf);
                idsf_sum = idsf_sum.wrapping_add(idsf);
            }
            allocations[band] = peak_idsf;
            let score;
            if presence[band] == 0 {
                scores[band + 1] = scores[band + 1].wrapping_add(0x100);
                score = scores[band + 1];
            } else {
                let mut table_index = ((presence[band] - 1) as u32) >> 1;
                if table_index > 4 {
                    table_index = 4;
                }
                let delta = (allocation + peak_idsf) * dba::DBA_BITCOUNT_R[table_index as usize];
                let mut adjustment;
                let current;
                if available_bits < 0x44c {
                    current = scores[band + 1];
                    adjustment = delta * 2;
                } else {
                    adjustment = delta;
                    current = scores[band + 1];
                    if dba::DBA_NSPS[band] < 0x11 && 0x1800 < current {
                        adjustment = current - 0x400;
                        if delta * 2 < current - 0x400 {
                            adjustment = delta * 2;
                        }
                    }
                }
                if band > 1 {
                    scores[band] -= adjustment >> 3;
                }
                if band < 0x1f {
                    scores[band + 2] -= adjustment >> 2;
                }
                score = current.wrapping_sub(adjustment);
                scores[band + 1] = score;
            }
            let mut new_presence = 0i32;
            if 7 < allocations[band]
                && dba::DBA_ATH_THRESHOLD[band] < idsf_sum
                && (0x3ff < score || 0x708 < available_bits)
            {
                new_presence = (score >> qpoint).clamp(1, 7);
            }
            presence[band] = new_presence;
        } else if qpoint == 10 {
            if idsf_sums[band].wrapping_mul(8) < allocation.wrapping_mul(dba::DBA_NSPS[band]) {
                scores[band + 1] = scores[band + 1].wrapping_add(0x100);
            }
        } else {
            if allocation.wrapping_mul(dba::DBA_NSPS[band]) < idsf_sums[band].wrapping_mul(5)
                && allocation < 0x3e
            {
                let value = allocation + 1;
                allocations[band] = value;
                if value < 0x3e {
                    allocations[band] = value + 1;
                }
            } else if allocation < 0x3e {
                allocations[band] = allocation + 1;
            }
        }
    }

    DbaBumpResult {
        idsfs,
        presence,
        allocations,
        scores,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
struct DbaRefineResult {
    idsfs: [u32; 256],
    presence: [i32; 32],
    allocations: [i32; 32],
    nunits: i32,
    ntones: i32,
    return_value: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[allow(clippy::too_many_arguments)]
fn dba_balance_and_trim(
    spectrum: &[f32],
    idsfs: &[u32],
    presence_in: &[i32; 32],
    allocations_in: &[i32; 32],
    scores_in: &[i32; 33],
    nunits_in: i32,
    ntones_in: i32,
    qpoint: i32,
    bit_budget: i32,
    available_bits: i32,
    channel_mode: i32,
    channel_flags: i32,
    param_1_0xb56: i32,
    tone_mode: i32,
    prior_tone_counts: &[i32],
    tone_table_active_quarters: &[[i32; 4]; 2],
) -> DbaRefineResult {
    let mut presence = *presence_in;
    let mut allocations = *allocations_in;
    let mut scores = *scores_in;
    let mut freed_budget = bit_budget;
    let nunits = nunits_in;
    let mut ntones = ntones_in;
    let idsfs_mut = idsfs.to_vec();

    let mut last_band = nunits - 1;
    let mut cost_accum = 0i32;
    let mut full_nsps = 0i32;
    let mut carry = 0i32;
    let mut scan_band = 0i32;

    loop {
        while scan_band < nunits && presence[scan_band as usize] == 0 {
            scan_band += 1;
            if nunits <= scan_band {
                break;
            }
        }
        if nunits <= scan_band {
            break;
        }
        let band = scan_band as usize;
        let presence_val = presence[band];
        let adj_ok = (band < 2 || scores[band] - 0xf00 <= scores[band + 1])
            && (last_band <= scan_band || scores[band + 2] - 0x1400 <= scores[band + 1]);
        if !adj_ok {
            presence[band] = 0;
            scan_band += 1;
            continue;
        }
        let nsps = dba::DBA_NSPS[band];
        carry = carry.wrapping_add(nsps);
        if presence_val == 7 {
            full_nsps = full_nsps.wrapping_add(nsps);
        }
        let qtstart = dba::DBA_QTSTART[band] as usize;
        let group_count = (nsps as usize) / 8;
        let mut bits_est = nsps
            .wrapping_mul(dba::dba_bitcount_bits0_view(presence_val))
            .wrapping_add(0x3c);
        let thresh = allocations[band] - dba::dba_bitcount_bits1_view(presence_val);
        for g in 0..group_count {
            let idx0 = qtstart / 4 + g * 2;
            if (idsfs_mut[idx0] as i32) < thresh {
                bits_est -= dba::dba_bitcount_r_view(presence_val);
            }
            if (idsfs_mut[idx0 + 1] as i32) < thresh {
                bits_est -= dba::dba_bitcount_r_view(presence_val);
            }
        }
        cost_accum = cost_accum.wrapping_add(bits_est);
        scan_band += 1;
    }

    while last_band > 0 && presence[last_band as usize] == 0 {
        freed_budget = freed_budget.wrapping_add(3);
        last_band -= 1;
    }
    let active_nunits = last_band + 1;

    let mut target_bits;
    if channel_mode == 0 || channel_flags & 7 == 7 {
        let mut tone_hi = ntones - 1;
        let tone_banks = tone_mode;
        if tone_hi >= 1 {
            let mut band_idx = tone_hi;
            loop {
                if prior_tone_counts[band_idx as usize] != 0 {
                    break;
                }
                let mut all_empty = true;
                if tone_banks > 0 {
                    #[allow(clippy::needless_range_loop)]
                    for bank in 0..tone_banks as usize {
                        let aq = tone_table_active_quarters[bank][band_idx as usize];
                        if aq != 0 {
                            all_empty = false;
                            break;
                        }
                    }
                }
                if !all_empty {
                    break;
                }
                band_idx -= 1;
                tone_hi = band_idx;
                if band_idx < 1 {
                    break;
                }
            }
        }
        target_bits = (ntones - (tone_hi + 1)) * (tone_banks + 3);
        ntones = tone_hi + 1;
    } else {
        target_bits = 0;
    }
    target_bits = target_bits.wrapping_add(freed_budget);

    let mut scaled_budget = target_bits * 10;
    let enter_scaling = if scaled_budget <= cost_accum {
        true
    } else {
        carry = carry.wrapping_sub(full_nsps);
        carry != 0
    };
    if enter_scaling {
        if 599 < target_bits {
            scaled_budget = target_bits * 9;
        }
        let denom = carry.wrapping_mul(10);
        let numer = (scaled_budget.wrapping_sub(cost_accum)).wrapping_mul(0x400);
        carry = if denom != 0 { numer / denom } else { 0 };
        if qpoint == 10 && (scaled_budget.wrapping_sub(cost_accum)) < 0 {
            carry = carry.wrapping_add(carry >> 3);
            scores[1] = scores[1].wrapping_sub(carry);
            scores[2] = scores[2].wrapping_sub((carry * 0xbc) >> 8);
            let mut i = 2;
            while i < 8 {
                scores[i + 1] = scores[i + 1].wrapping_sub((carry * 0x61) >> 8);
                i += 1;
            }
            let mut i = 8;
            while i < 0x12 {
                scores[i + 1] = scores[i + 1].wrapping_sub((carry * 0x4a) >> 8);
                i += 1;
            }
            let mut i = 0;
            while i < 0x12 {
                while carry + scores[i + 1] >= 0 || allocations[i] > 0x3e {
                    i += 1;
                    if i > 0x11 {
                        break;
                    }
                }
                if i > 0x11 {
                    break;
                }
                allocations[i] += 1;
                i += 1;
            }
            let mut i = 0x12;
            while i < active_nunits as usize {
                while carry + scores[i + 1] > 0x3ff {
                    i += 1;
                    if i >= active_nunits as usize {
                        break;
                    }
                }
                if i >= active_nunits as usize {
                    break;
                }
                let adj = (carry + scores[i + 1] - 0x400) >> 10;
                let mut capped_alloc = allocations[i] - adj;
                if 0x3f < capped_alloc {
                    capped_alloc = 0x3f;
                }
                allocations[i] = capped_alloc;
                i += 1;
            }
        }
    }

    let running_score_init = target_bits * 10;
    let mut running_score = running_score_init;
    let mut hi_band = last_band;
    loop {
        while hi_band >= 0 && presence[hi_band as usize] == 0 {
            hi_band -= 1;
            if hi_band < 0 {
                break;
            }
        }
        if hi_band < 0 {
            break;
        }
        let band = hi_band as usize;
        let presence_bits = ((carry + scores[band + 1]) >> qpoint).clamp(1, 7);
        presence[band] = presence_bits;
        let nsps = dba::DBA_NSPS[band];
        let qtstart = dba::DBA_QTSTART[band] as usize;
        let group_count = (nsps as usize) / 8;
        let mut bit_cost = nsps
            .wrapping_mul(dba::dba_bitcount_bits0_view(presence_bits))
            .wrapping_add(0x3c);
        let thresh = allocations[band] - dba::dba_bitcount_bits1_view(presence_bits);
        for g in 0..group_count {
            let idx0 = qtstart / 4 + g * 2;
            if (idsfs_mut[idx0] as i32) < thresh {
                bit_cost -= dba::dba_bitcount_r_view(presence_bits);
            }
            if (idsfs_mut[idx0 + 1] as i32) < thresh {
                bit_cost -= dba::dba_bitcount_r_view(presence_bits);
            }
        }
        running_score = running_score.wrapping_sub(bit_cost);
        scores[band + 1] = running_score;
        hi_band -= 1;
    }

    let mut used_bits = 0i32;
    let mut slack;
    let mut band = 0usize;
    let mut alloc_marks = [-1i32; 32];
    #[allow(unused_assignments)]
    let mut final_nunits = active_nunits;

    loop {
        let presence_bits = presence[band];
        if presence_bits == 0 {
            alloc_marks[band] = -1;
        } else {
            let nsps_band = dba::DBA_NSPS[band];
            if (target_bits - used_bits) * 2 - 0x10 < nsps_band {
                presence[band] = 0;
                alloc_marks[band] = -1;
            } else {
                let headroom = scores[band + 1] + used_bits * -10;
                if headroom < 0xfa1 || 6 < presence_bits {
                    if headroom < 300 && band != 0 {
                        if presence_bits < 2 {
                            if allocations[band] != 0x3f {
                                allocations[band] += 1;
                            }
                        } else {
                            presence[band] = presence_bits - 1;
                        }
                    }
                } else {
                    presence[band] = presence_bits + 1;
                }
                let new_bits = countbits_nontone_specs_generic(
                    presence[band],
                    allocations[band],
                    nsps_band,
                    &spectrum[dba::DBA_QTSTART[band] as usize..],
                );
                scores[band + 1] = new_bits;
                if new_bits * 2 == nsps_band * dba::DBA_DAT_000D3CC0[presence[band] as usize] + 0xc
                {
                    presence[band] = 0;
                    alloc_marks[band] = -1;
                } else {
                    used_bits += new_bits;
                    alloc_marks[band] = allocations[band];
                }
            }
        }
        band += 1;
        if band >= active_nunits as usize {
            while last_band > 0 && presence[last_band as usize] == 0 {
                target_bits += 3;
                last_band -= 1;
            }
            last_band += 1;
            final_nunits = last_band;
            slack = target_bits - used_bits;

            if param_1_0xb56 + prior_tone_counts[0] != 0 && channel_mode == 0 {
                #[allow(clippy::needless_range_loop)]
                for i in 0..5 {
                    if (alloc_marks[i] as u32) <= 0x40 {
                        alloc_marks[i] = 0x40;
                    }
                }
            }

            let mut spent_bits = 0i32;
            loop {
                let mut best_val: i32 = -1;
                let mut best_band = 0;
                let mut found = false;
                #[allow(clippy::needless_range_loop)]
                for i in 0..final_nunits as usize {
                    if alloc_marks[i] > best_val {
                        best_val = alloc_marks[i];
                        best_band = i;
                        found = true;
                    }
                }
                if !found || best_val < 0 {
                    return DbaRefineResult {
                        idsfs: idsfs_mut[..256].try_into().unwrap(),
                        presence,
                        allocations,
                        nunits: final_nunits,
                        ntones,
                        return_value: available_bits - (target_bits - spent_bits),
                    };
                }
                let mut band_bits = scores[best_band + 1];
                let mut projected_bits = spent_bits + band_bits;
                alloc_marks[best_band] = -1;

                if target_bits < projected_bits {
                    let min_bits = (dba::DBA_NSPS[best_band] >> 1)
                        * dba::DBA_DAT_000D3CC0[presence[best_band] as usize]
                        + 6;
                    while final_nunits > 1 {
                        let last = final_nunits - 1;
                        if best_band as i32 >= last {
                            break;
                        }
                        if presence[last as usize] != 0 && alloc_marks[last as usize] == -1 {
                            break;
                        }
                        target_bits += 3;
                        presence[last as usize] = 0;
                        alloc_marks[last as usize] = -1;
                        final_nunits = last;
                    }
                    if target_bits < projected_bits {
                        allocations[best_band] += 1;
                        if allocations[best_band] < 0x40 && min_bits < band_bits {
                            loop {
                                band_bits = countbits_nontone_specs_generic(
                                    presence[best_band],
                                    allocations[best_band],
                                    dba::DBA_NSPS[best_band],
                                    &spectrum[dba::DBA_QTSTART[best_band] as usize..],
                                );
                                projected_bits = spent_bits + band_bits;
                                if projected_bits <= target_bits {
                                    break;
                                }
                                allocations[best_band] += 1;
                                if allocations[best_band] > 0x3f {
                                    break;
                                }
                                if min_bits >= band_bits {
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    let cur_presence = presence[best_band];
                    if cur_presence < 7
                        && (dba::DBA_NSPS[best_band] >> 1)
                            * dba::DBA_ZEROBITS[cur_presence as usize]
                            < (target_bits - spent_bits) - 7
                    {
                        let mut boosted_presence = cur_presence + 1;
                        if boosted_presence < 7 && dba::DBA_NSPS[best_band] * 4 < slack {
                            boosted_presence = cur_presence + 2;
                        }
                        let new_bits = countbits_nontone_specs_generic(
                            boosted_presence,
                            allocations[best_band],
                            dba::DBA_NSPS[best_band],
                            &spectrum[dba::DBA_QTSTART[best_band] as usize..],
                        );
                        if new_bits - band_bits <= slack.max(0) {
                            slack -= new_bits - band_bits;
                            presence[best_band] = boosted_presence;
                            projected_bits = spent_bits + new_bits;
                            band_bits = new_bits;
                        }
                    }
                }
                spent_bits = projected_bits;
                if target_bits < projected_bits {
                    spent_bits = projected_bits - band_bits;
                    slack += band_bits;
                    presence[best_band] = 0;
                }
            }
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[allow(clippy::too_many_arguments)]
fn dba_refine_allocations(
    spectrum: &[f32],
    idsfs: &[u32],
    presence: &[i32; 32],
    allocations: &[i32; 32],
    scores: &[i32; 33],
    idsf_sums: &[i32; 32],
    run_allocation_bump: bool,
    nunits: i32,
    ntones: i32,
    qpoint: i32,
    bit_budget: i32,
    available_bits: i32,
    channel_mode: i32,
    channel_flags: i32,
    param_1_0xb56: i32,
    tone_mode: i32,
    prior_tone_counts: &[i32],
    tone_table_active_quarters: &[[i32; 4]; 2],
) -> DbaRefineResult {
    if run_allocation_bump {
        let bumped = dba_allocation_bump(
            spectrum,
            idsfs,
            presence,
            allocations,
            scores,
            idsf_sums,
            nunits,
            qpoint,
            available_bits,
        );
        dba_balance_and_trim(
            spectrum,
            &bumped.idsfs,
            &bumped.presence,
            &bumped.allocations,
            &bumped.scores,
            nunits,
            ntones,
            qpoint,
            bit_budget,
            available_bits,
            channel_mode,
            channel_flags,
            param_1_0xb56,
            tone_mode,
            prior_tone_counts,
            tone_table_active_quarters,
        )
    } else {
        dba_balance_and_trim(
            spectrum,
            idsfs,
            presence,
            allocations,
            scores,
            nunits,
            ntones,
            qpoint,
            bit_budget,
            available_bits,
            channel_mode,
            channel_flags,
            param_1_0xb56,
            tone_mode,
            prior_tone_counts,
            tone_table_active_quarters,
        )
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct DbaAt3DataParams<'a> {
    pub(crate) spectrum: &'a [f32; 1024],
    pub(crate) initial_nunits: i32,
    pub(crate) available_bits: i32,
    pub(crate) channel_mode: i32,
    pub(crate) prior_tone_counts: &'a [i32; 4],
    pub(crate) channel_flags: i32,
    pub(crate) param_1_0xb56: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbaAt3DataResult {
    pub(crate) residual_spectrum: [f32; 1024],
    pub(crate) residual_idsfs: [u32; 256],
    pub(crate) presence: [i32; 32],
    pub(crate) allocations: [i32; 32],
    pub(crate) nunits: i32,
    pub(crate) ntones: i32,
    pub(crate) coding_layout: i32,
    pub(crate) tone_mode: i32,
    pub(crate) tone_table: DbaToneTable,
    pub(crate) remaining_bits: i32,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn dba_fallback_at3data() -> DbaAt3DataResult {
    DbaAt3DataResult {
        residual_spectrum: [0.0; 1024],
        residual_idsfs: [0; 256],
        presence: [0; 32],
        allocations: [0; 32],
        nunits: 1,
        ntones: 1,
        coding_layout: 0,
        tone_mode: 0,
        tone_table: DbaToneTable {
            banks: [
                DbaToneBank::new(3),
                DbaToneBank {
                    active_quarters: [0; 4],
                    idwl: 0,
                    width: 0,
                    groups: std::array::from_fn(|_| Vec::new()),
                },
            ],
            components: Vec::new(),
        },
        remaining_bits: 0,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn dba_at3data(params: DbaAt3DataParams<'_>) -> DbaAt3DataResult {
    let initial_idsfs = dba_max_abs_prepass(params.spectrum);
    let prelude = dba_allocation_prelude(DbaPreludeParams {
        idsfs: &initial_idsfs,
        initial_nunits: params.initial_nunits,
        available_bits: params.available_bits,
        channel_mode: params.channel_mode,
        prior_tone_counts: params.prior_tone_counts,
    });

    if prelude.high_rate {
        let high = dba_high_rate_tone_extract(params.spectrum, &initial_idsfs, &prelude).unwrap();
        let mut scores = prelude.strong_idsf_counts;
        for band in 0..prelude.nunits as usize {
            scores[band + 1] = high.allocations[band].wrapping_mul(0x155);
        }
        let active_quarters = [
            high.tone_table.banks[0].active_quarters,
            high.tone_table.banks[1].active_quarters,
        ];
        let refined = dba_refine_allocations(
            &high.residual_spectrum,
            &high.residual_idsfs,
            &high.presence,
            &high.allocations,
            &scores,
            &prelude.idsf_sums,
            false,
            prelude.nunits,
            high.ntones,
            prelude.qpoint,
            high.bit_budget,
            params.available_bits,
            params.channel_mode,
            params.channel_flags,
            params.param_1_0xb56,
            2,
            params.prior_tone_counts,
            &active_quarters,
        );
        return DbaAt3DataResult {
            residual_spectrum: high.residual_spectrum.try_into().unwrap(),
            residual_idsfs: refined.idsfs,
            presence: refined.presence,
            allocations: refined.allocations,
            nunits: refined.nunits,
            ntones: refined.ntones,
            coding_layout: 1,
            tone_mode: 2,
            tone_table: high.tone_table,
            remaining_bits: refined.return_value,
        };
    }

    let low = dba_low_rate_allocate(
        params.spectrum,
        &initial_idsfs,
        &prelude,
        params.channel_mode,
    )
    .unwrap();
    let mut scores = prelude.strong_idsf_counts;
    scores[1..=prelude.nunits as usize]
        .copy_from_slice(&low.allocation_scores[..prelude.nunits as usize]);
    let active_quarters = [
        low.tone_table.banks[0].active_quarters,
        low.tone_table.banks[1].active_quarters,
    ];
    let refined = dba_refine_allocations(
        &low.residual_spectrum,
        &low.residual_idsfs,
        &low.presence,
        &low.allocations,
        &scores,
        &prelude.idsf_sums,
        true,
        prelude.nunits,
        low.ntones,
        prelude.qpoint,
        low.bit_budget,
        params.available_bits,
        params.channel_mode,
        params.channel_flags,
        params.param_1_0xb56,
        low.tone_mode,
        params.prior_tone_counts,
        &active_quarters,
    );
    DbaAt3DataResult {
        residual_spectrum: low.residual_spectrum.try_into().unwrap(),
        residual_idsfs: refined.idsfs,
        presence: refined.presence,
        allocations: refined.allocations,
        nunits: refined.nunits,
        ntones: refined.ntones,
        coding_layout: low.coding_layout,
        tone_mode: low.tone_mode,
        tone_table: low.tone_table,
        remaining_bits: refined.return_value,
    }
}

pub fn countbits_nontone_specs_generic(
    idsf_idx: i32,
    idwl: i32,
    n_samples: i32,
    spectrum: &[f32],
) -> i32 {
    let phase = (idwl as u32).wrapping_mul(0x002b_0000);
    let table_index = ((phase >> 23) as i32 + idsf_idx) * 3 - idwl;
    let raw_bits = DBA_SCALE_LOOKUP[table_index as usize].wrapping_sub(phase & 0x7f80_0000);
    let scale = f32::from_bits(raw_bits);

    let mut total_bits: i32 = 6;
    let n_groups = n_samples / 8;

    if idsf_idx == 1 {
        for group in 0..n_groups {
            let base = group * 8;
            let mut idx0: i32 = 0;
            for j in 0..4 {
                let q = dba_magic_round_bits(spectrum[(base + j) as usize], scale) & 1;
                idx0 = (idx0 << 1) | q as i32;
            }
            let mut idx1: i32 = 0;
            for j in 4..8 {
                let q = dba_magic_round_bits(spectrum[(base + j) as usize], scale) & 1;
                idx1 = (idx1 << 1) | q as i32;
            }
            total_bits += DBA_NBITS_WL2_QUAD[idx0 as usize] + DBA_NBITS_WL2_QUAD[idx1 as usize];
        }
    } else {
        let table = dba::dba_hcspec_table(idsf_idx);
        let mask = DBA_HUF_MASK[(idsf_idx - 2) as usize];
        for group in 0..n_groups {
            let base = group * 8;
            for j in 0..8 {
                let q = dba_magic_round_bits(spectrum[(base + j) as usize], scale) & mask;
                total_bits += (table[q as usize] >> 16) as i32;
            }
        }
    }

    total_bits
}
