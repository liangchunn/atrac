use crate::gha::power::{PowerCheckError, check_power_level_at5, check_power_level_dual_at5};
use crate::tables::at5::{SIN_AT5_ENTRIES, amtbl_gha, sftbl_gha_at5, sin_at5};

pub const COMPONENT_SAMPLES: usize = 256;
const WINDOW_LOW: f32 = f32::from_bits(0x3e15_f000);
const WINDOW_HALF: f32 = f32::from_bits(0x3f00_0000);
const WINDOW_HIGH: f32 = f32::from_bits(0x3f5a_8400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhaSynthesisError {
    InvalidRange { start: usize, end: usize },
    SampleCountNotMultipleOfFour { sample_count: usize },
    InputTooShort { needed: usize, actual: usize },
    OutputTooShort { needed: usize, actual: usize },
    UnsupportedFrequency { frequency: usize },
    WindowOutOfRange { index: usize },
    ScaleIndexOutOfRange { index: usize },
    AmplitudeIndexOutOfRange { index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentResult {
    pub residual_power: f64,
    pub sin_weight: f32,
    pub cos_weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhaWaveRecord {
    pub scale_index: usize,
    pub amplitude_index: usize,
    pub phase_index: usize,
    pub frequency: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhaSynthesisState<'a> {
    pub lower_window: Option<usize>,
    pub upper_window: Option<usize>,
    pub waves: &'a [GhaWaveRecord],
}

pub fn calc_component_at5(
    samples: &[f32],
    frequency: usize,
    start: usize,
    end: usize,
) -> Result<ComponentResult, GhaSynthesisError> {
    check_component_inputs(samples.len(), frequency, start, end)?;

    let count = end - start;
    let mut residual = [0.0_f32; COMPONENT_SAMPLES];

    if frequency == 0 {
        let mut sum = 0.0_f32;
        for chunk in samples[start..end].chunks_exact(4) {
            sum += chunk[1] + chunk[0] + chunk[2] + chunk[3];
        }

        let mean = sum / (count as f32);
        for (dst, &sample) in residual[start..end].iter_mut().zip(&samples[start..end]) {
            *dst = sample - mean;
        }

        return Ok(ComponentResult {
            residual_power: check_power_level_at5(&residual, &residual, COMPONENT_SAMPLES)?,
            sin_weight: 0.0,
            cos_weight: mean,
        });
    }

    let sin_table = sin_at5();
    let previous_phase = ((start as isize - 1) * frequency as isize).rem_euclid(2048) as usize;
    let end_phase = (frequency * end) & 0x7ff;
    let mut phase = previous_phase;
    let mut sin_basis = [0.0_f32; COMPONENT_SAMPLES + 1];
    let mut cos_basis = [0.0_f32; COMPONENT_SAMPLES];

    for index in (start..end).step_by(4) {
        phase = (phase + frequency) & 0x7ff;
        sin_basis[index + 1] = sin_table[phase];
        cos_basis[index] = sin_table[(phase + 0x200) & 0x7ff];

        phase = (phase + frequency) & 0x7ff;
        sin_basis[index + 2] = sin_table[phase];
        cos_basis[index + 1] = sin_table[(phase + 0x200) & 0x7ff];

        phase = (phase + frequency) & 0x7ff;
        sin_basis[index + 3] = sin_table[phase];
        cos_basis[index + 2] = sin_table[(phase + 0x200) & 0x7ff];

        phase = (phase + frequency) & 0x7ff;
        sin_basis[index + 4] = sin_table[phase];
        cos_basis[index + 3] = sin_table[(phase + 0x200) & 0x7ff];
    }

    let [sin_power, cos_power] = check_power_level_dual_at5(
        &samples[start..],
        &sin_basis[start + 1..],
        &samples[start..],
        &cos_basis[start..],
        count,
    )?;
    let boundary = ((count as f64)
        - ((f64::from(cos_basis[end - 1]) * f64::from(sin_table[end_phase]))
            - (f64::from(sin_table[(previous_phase + 0x200) & 0x7ff])
                * f64::from(sin_basis[start + 1])))
            / f64::from(sin_table[frequency]))
        * 0.5;
    let cross = ((f64::from(sin_basis[end]) * f64::from(sin_table[end_phase]))
        - (f64::from(sin_basis[start + 1]) * f64::from(sin_table[previous_phase])))
        / f64::from(sin_table[frequency])
        * 0.5;
    let complement = (count as f64) - boundary;
    let determinant_recip = 1.0 / (cross * cross - boundary * complement);
    let sin_weight =
        (f64::from(cos_power) * cross - complement * f64::from(sin_power)) * determinant_recip;
    let cos_weight =
        determinant_recip * (f64::from(sin_power) * cross - f64::from(cos_power) * boundary);

    for index in start..end {
        let estimate =
            sin_weight * f64::from(sin_basis[index + 1]) + cos_weight * f64::from(cos_basis[index]);
        residual[index] = (f64::from(samples[index]) - estimate) as f32;
    }

    Ok(ComponentResult {
        residual_power: check_power_level_at5(&residual, &residual, COMPONENT_SAMPLES)?,
        sin_weight: sin_weight as f32,
        cos_weight: cos_weight as f32,
    })
}

pub fn synthesis_wav_at5(
    state: &GhaSynthesisState<'_>,
    output: &mut [f32],
    window_offset: usize,
    count: usize,
    scale_only: bool,
    invert: bool,
    invert_gate: i32,
) -> Result<(), GhaSynthesisError> {
    if output.len() < count {
        return Err(GhaSynthesisError::OutputTooShort {
            needed: count,
            actual: output.len(),
        });
    }
    if window_offset + count > COMPONENT_SAMPLES {
        return Err(GhaSynthesisError::InvalidRange {
            start: window_offset,
            end: window_offset + count,
        });
    }

    let sin_table = sin_at5();
    let scale_table = sftbl_gha_at5();
    let amplitude_table = amtbl_gha();

    output[..count].fill(0.0);

    for wave in state.waves {
        let scale =
            *scale_table
                .get(wave.scale_index)
                .ok_or(GhaSynthesisError::ScaleIndexOutOfRange {
                    index: wave.scale_index,
                })?;
        let amplitude = if scale_only {
            scale
        } else {
            let amplitude = *amplitude_table.get(wave.amplitude_index).ok_or(
                GhaSynthesisError::AmplitudeIndexOutOfRange {
                    index: wave.amplitude_index,
                },
            )?;
            scale * amplitude
        };
        let mut phase = ((wave.phase_index & 0x1f) as i64 * 0x40)
            + (window_offset as i64 - 0x80) * wave.frequency as i64;

        for sample in &mut output[..count] {
            let phase_index = phase.rem_euclid(SIN_AT5_ENTRIES as i64) as usize;
            let next =
                f64::from(amplitude) * f64::from(sin_table[phase_index]) + f64::from(*sample);
            *sample = next as f32;
            phase = phase_index as i64 + wave.frequency as i64;
        }
    }

    if invert && invert_gate == 1 {
        for sample in &mut output[..count] {
            *sample = f32::from_bits(sample.to_bits() ^ 0x8000_0000);
        }
    }

    let window = synthesis_window(state)?;
    for (index, sample) in output[..count].iter_mut().enumerate() {
        *sample = (f64::from(window[window_offset + index]) * f64::from(*sample)) as f32;
    }

    Ok(())
}

fn synthesis_window(
    state: &GhaSynthesisState<'_>,
) -> Result<[f32; COMPONENT_SAMPLES], GhaSynthesisError> {
    let mut window = [1.0_f32; COMPONENT_SAMPLES];

    if let Some(index) = state.lower_window {
        if index + 3 >= COMPONENT_SAMPLES {
            return Err(GhaSynthesisError::WindowOutOfRange { index });
        }
        window[..index].fill(0.0);
        window[index] = 0.0;
        window[index + 1] = WINDOW_LOW;
        window[index + 2] = WINDOW_HALF;
        window[index + 3] = WINDOW_HIGH;
    }

    if let Some(index) = state.upper_window {
        if index < 4 || index > COMPONENT_SAMPLES {
            return Err(GhaSynthesisError::WindowOutOfRange { index });
        }
        window[index - 4] = WINDOW_HIGH;
        window[index - 3] = WINDOW_HALF;
        window[index - 2] = WINDOW_LOW;
        window[index - 1] = 0.0;
        window[index..].fill(0.0);
    }

    Ok(window)
}

fn check_component_inputs(
    sample_len: usize,
    frequency: usize,
    start: usize,
    end: usize,
) -> Result<(), GhaSynthesisError> {
    if start > end || end > COMPONENT_SAMPLES {
        return Err(GhaSynthesisError::InvalidRange { start, end });
    }

    let sample_count = end - start;
    if sample_count % 4 != 0 {
        return Err(GhaSynthesisError::SampleCountNotMultipleOfFour { sample_count });
    }

    if sample_len < COMPONENT_SAMPLES {
        return Err(GhaSynthesisError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: sample_len,
        });
    }

    if frequency >= SIN_AT5_ENTRIES {
        return Err(GhaSynthesisError::UnsupportedFrequency { frequency });
    }
    if frequency > 0 && frequency < SIN_AT5_ENTRIES && sin_at5()[frequency].to_bits() == 0 {
        return Err(GhaSynthesisError::UnsupportedFrequency { frequency });
    }

    Ok(())
}

impl From<PowerCheckError> for GhaSynthesisError {
    fn from(error: PowerCheckError) -> Self {
        match error {
            PowerCheckError::SampleCountNotMultipleOfFour { sample_count } => {
                Self::SampleCountNotMultipleOfFour { sample_count }
            }
            PowerCheckError::InputTooShort { needed, actual } => {
                Self::InputTooShort { needed, actual }
            }
        }
    }
}
