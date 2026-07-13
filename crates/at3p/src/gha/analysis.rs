use crate::dsp::fft::{FftError, dft_x_at5};
use crate::dsp::scalar::{ScalarError, invmix_seq_at5, mix_seq_at5};
use crate::gha::power::{PowerCheckError, check_power_level_at5};
use crate::gha::synthesis::{
    COMPONENT_SAMPLES, ComponentResult, GhaSynthesisError, GhaWaveRecord, calc_component_at5,
};
use crate::tables::at5::{
    SIN_AT5_ENTRIES, amtbl_gha, ip256_at5, sc256_at5, sftbl_gha_at5, sin_at5,
};

const TWO_PI_AT5: f32 = f32::from_bits(0x40c9_0fdb);
const PHASE_UNITS: f32 = f32::from_bits(0x4500_0000);
const GHA_SCALE_PREBIAS: f32 = 0.916_992_2;
const GHA_SCALE_MINIMUM: f32 = 0.594_604_5;
const GHA_GENERAL_AMPLITUDE_BIAS: f32 = f32::from_bits(0x3d07_5c00);
const GHA_GENERAL_AMPLITUDE_ZERO_WIDTH: f32 = f32::from_bits(0x3d87_5c00);
const MAX_SINE_WAVES_AT5: usize = 16;
const GENERAL_WINDOW_SOURCE_SAMPLES: usize = 0x180;
pub const GENERAL_STATE_WORDS: usize = 10;
pub type GeneralStateWords = [i32; GENERAL_STATE_WORDS];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhaAnalysisError {
    InvalidCoarseBin {
        coarse_bin: usize,
    },
    DftWeightsTooShort {
        needed: usize,
        actual: usize,
    },
    EnergyTableTooShort {
        channel: usize,
        needed: usize,
        actual: usize,
    },
    BudgetTableTooShort {
        channel: usize,
        needed: usize,
        actual: usize,
    },
    SelectedBandOrderTooShort {
        needed: usize,
        actual: usize,
    },
    SharedFlagsTooShort {
        needed: usize,
        actual: usize,
    },
    StereoFlagsTooShort {
        needed: usize,
        actual: usize,
    },
    StateTableTooShort {
        channel: usize,
        needed: usize,
        actual: usize,
    },
    SchedulerCallCountMismatch {
        expected: usize,
        actual: usize,
    },
    SchedulerCallLabelMismatch {
        call_index: usize,
        expected_group: usize,
        actual_group: usize,
        expected_channel: Option<usize>,
        actual_channel: Option<usize>,
    },
    MissingSharedChannelSamples {
        call_index: usize,
        group: usize,
    },
    OutputTooShort {
        needed: usize,
        actual: usize,
    },
    UnsupportedGeneralChannelCount {
        channel_count: usize,
    },
    UnsupportedGeneralGroupCount {
        group_count: usize,
    },
    UnsupportedGeneralMode {
        mode: usize,
    },
    InvalidGeneralGroupIndex {
        group: usize,
        group_count: usize,
    },
    UnsupportedSineWaveCount {
        max_waves: usize,
    },
    Component(GhaSynthesisError),
    Fft(FftError),
    Power(PowerCheckError),
    Scalar(ScalarError),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FineAnalysisResult {
    pub amplitude: f32,
    pub phase: u32,
    pub frequency: usize,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    frequency: usize,
    residual_power: f32,
    component: ComponentResult,
    source_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneralDispatchCall {
    pub group_index: usize,
    pub channel_index: Option<usize>,
    pub max_waves: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct GeneralSchedulerCallInput<'a> {
    pub group_index: usize,
    pub channel_index: Option<usize>,
    pub samples: &'a [f32],
    pub source: &'a [f32],
    pub delayed_window_words: [i32; 4],
    pub shared_channel_samples: Option<(&'a [f32], &'a [f32])>,
    pub initial_records: &'a [GhaWaveRecord],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralSchedulerCallOutput {
    pub group_index: usize,
    pub channel_index: Option<usize>,
    pub initial_bin: Option<usize>,
    pub state: GeneralStateWords,
    pub records: Vec<GhaWaveRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralSchedulerResult {
    pub frequency_indices: Vec<Vec<Option<usize>>>,
    pub calls: Vec<GeneralSchedulerCallOutput>,
}

pub fn fine_analysis_at5(
    samples: &[f32],
    coarse_bin: usize,
    start: usize,
    end: usize,
) -> Result<FineAnalysisResult, GhaAnalysisError> {
    if coarse_bin > 128 {
        return Err(GhaAnalysisError::InvalidCoarseBin { coarse_bin });
    }

    let center = coarse_bin * 8;
    let lower = center.saturating_sub(4);
    let upper = (center + 4).min(0x400);
    let mut candidates = Vec::new();
    for (source_index, frequency) in (lower..upper).step_by(2).enumerate() {
        let component = calc_component_at5(samples, frequency, start, end)?;
        candidates.push(Candidate {
            frequency,
            residual_power: component.residual_power as f32,
            component,
            source_index,
        });
    }

    if candidates.len() < 2 {
        return Err(GhaAnalysisError::InvalidCoarseBin { coarse_bin });
    }

    shell_sort_descending_by_residual(&mut candidates);

    let best_even = candidates[candidates.len() - 1];
    let second_best = candidates[candidates.len() - 2];
    let first_refine_frequency = if second_best.source_index <= best_even.source_index {
        best_even.frequency.saturating_sub(1)
    } else {
        best_even.frequency + 1
    };
    let mut refined_frequency = first_refine_frequency;
    let mut refined_component = calc_component_at5(samples, refined_frequency, start, end)?;
    let mut refined_residual = refined_component.residual_power as f32;

    if best_even.source_index == candidates.len() - 1 {
        let right_frequency = best_even.frequency + 1;
        let right_component = calc_component_at5(samples, right_frequency, start, end)?;
        if right_component.residual_power < f64::from(refined_residual) {
            refined_frequency = right_frequency;
            refined_component = right_component;
            refined_residual = right_component.residual_power as f32;
        }
    }

    let (frequency, component) = if best_even.residual_power <= refined_residual {
        (best_even.frequency, best_even.component)
    } else {
        (refined_frequency, refined_component)
    };
    let amplitude = ((f64::from(component.sin_weight) * f64::from(component.sin_weight))
        + (f64::from(component.cos_weight) * f64::from(component.cos_weight)))
    .sqrt() as f32;
    let angle = f64::from(component.cos_weight).atan2(f64::from(component.sin_weight)) as f32;
    let phase_units = ((angle / TWO_PI_AT5) * PHASE_UNITS + 0.5).floor() as i32;
    let phase = ((phase_units + (frequency as i32 * 0x80)) & 0x7ff) as u32;

    Ok(FineAnalysisResult {
        amplitude,
        phase,
        frequency,
    })
}

pub fn analysis_sine_at5_sub(
    samples: &[f32],
    records: &mut [GhaWaveRecord],
    start: usize,
    end: usize,
    initial_coarse_bin: Option<usize>,
    max_waves: usize,
) -> Result<usize, GhaAnalysisError> {
    validate_sine_inputs(samples.len(), start, end)?;
    if max_waves == 0 || initial_coarse_bin.is_none() {
        return Ok(0);
    }
    if max_waves > MAX_SINE_WAVES_AT5 {
        return Err(GhaAnalysisError::UnsupportedSineWaveCount { max_waves });
    }
    if records.len() < max_waves {
        return Err(GhaAnalysisError::OutputTooShort {
            needed: max_waves,
            actual: records.len(),
        });
    }

    let count = end - start;
    let mut local = [0.0_f32; COMPONENT_SAMPLES];
    local[start..end].copy_from_slice(&samples[start..end]);

    let scale_table = sftbl_gha_at5();
    let mut previous_power = check_power_level_at5(&local[start..], &local[start..], count)? as f32;
    let mut accepted = 0usize;
    let mut active = true;
    let mut coarse_bin = initial_coarse_bin.unwrap();
    let ip_table = ip256_at5();
    let sc_table = sc256_at5();

    while accepted < max_waves && active {
        if accepted > 0 {
            let mut dft_bins = [0.0_f32; 129];
            dft_x_at5(
                &local,
                COMPONENT_SAMPLES,
                &mut dft_bins,
                &ip_table,
                &sc_table,
            )?;
            match strongest_positive_dft_bin_at5(&dft_bins) {
                Some(bin) => coarse_bin = bin,
                None => break,
            }
        }

        let fine = fine_analysis_at5(&local, coarse_bin, start, end)?;
        let scale_index = quantize_gha_scale_at5(fine.amplitude, &scale_table);
        let phase_index = quantize_phase_index_at5(fine.phase);
        let frequency = fine.frequency;

        records[accepted].scale_index = scale_index;
        records[accepted].phase_index = phase_index;
        records[accepted].frequency = frequency;

        subtract_quantized_sine_at5(
            &mut local,
            start,
            end,
            frequency,
            phase_index,
            scale_table[scale_index],
        );

        let new_power = check_power_level_at5(&local[start..], &local[start..], count)?;
        if new_power <= f64::from(previous_power) {
            previous_power = new_power as f32;
            accepted += 1;
        } else {
            active = false;
        }
    }

    records[..accepted].sort_by_key(|record| record.frequency);
    Ok(accepted)
}

pub fn analysis_general_at5_sub(
    samples: &[f32],
    records: &mut [GhaWaveRecord],
    start: usize,
    end: usize,
    mode: usize,
    initial_coarse_bin: Option<usize>,
    dft_weights: &[f32],
    max_waves: usize,
    channel_count: usize,
    profile_selector: usize,
) -> Result<usize, GhaAnalysisError> {
    validate_sine_inputs(samples.len(), start, end)?;
    if mode != 0 && mode != 1 {
        return Err(GhaAnalysisError::UnsupportedGeneralMode { mode });
    }
    if max_waves == 0 || initial_coarse_bin.is_none() {
        return Ok(0);
    }
    if max_waves > MAX_SINE_WAVES_AT5 {
        return Err(GhaAnalysisError::UnsupportedSineWaveCount { max_waves });
    }
    if records.len() < max_waves {
        return Err(GhaAnalysisError::OutputTooShort {
            needed: max_waves,
            actual: records.len(),
        });
    }
    if dft_weights.len() < 129 {
        return Err(GhaAnalysisError::DftWeightsTooShort {
            needed: 129,
            actual: dft_weights.len(),
        });
    }

    let count = end - start;
    let mut local = [0.0_f32; COMPONENT_SAMPLES];
    local[start..end].copy_from_slice(&samples[start..end]);
    let window_scale = general_window_scale_at5(count);
    for sample in &mut local {
        *sample *= window_scale;
    }

    let band_peaks = general_band_peaks_at5(samples);
    let dominant_band = strongest_band_at5(&band_peaks);
    let scale_table = sftbl_gha_at5();
    let amplitude_table = amtbl_gha();
    let mut previous_power = check_power_level_at5(&local[start..], &local[start..], count)? as f32;
    let mut accepted = 0usize;
    let mut active = true;
    let mut coarse_bin = initial_coarse_bin.unwrap();
    let ip_table = ip256_at5();
    let sc_table = sc256_at5();
    let ratio_limit = general_power_ratio_limit_at5(channel_count, profile_selector);
    let mut raw_amplitudes = [0.0_f32; MAX_SINE_WAVES_AT5];
    let mut raw_phases = [0_u32; MAX_SINE_WAVES_AT5];
    let mut raw_frequencies = [0_usize; MAX_SINE_WAVES_AT5];

    while accepted < max_waves && active {
        if accepted > 0 {
            let mut dft_bins = [0.0_f32; 129];
            dft_x_at5(
                &local,
                COMPONENT_SAMPLES,
                &mut dft_bins,
                &ip_table,
                &sc_table,
            )?;
            for (bin, &weight) in dft_bins.iter_mut().zip(&dft_weights[..129]) {
                *bin *= weight;
            }
            match strongest_positive_dft_bin_at5(&dft_bins) {
                Some(bin) => coarse_bin = bin,
                None => break,
            }
        }

        let fine = fine_analysis_at5(&local, coarse_bin, start, end)?;
        raw_amplitudes[accepted] = fine.amplitude;
        raw_phases[accepted] = fine.phase;
        raw_frequencies[accepted] = fine.frequency;

        if mode == 1 {
            let scale_index = quantize_gha_scale_at5(fine.amplitude, &scale_table);
            let phase_index = quantize_phase_index_at5(fine.phase);

            records[accepted].scale_index = scale_index;
            records[accepted].phase_index = phase_index;
            records[accepted].frequency = fine.frequency;

            subtract_quantized_sine_at5(
                &mut local,
                start,
                end,
                fine.frequency,
                phase_index,
                scale_table[scale_index],
            );
        } else {
            subtract_sine_at5(
                &mut local,
                start,
                end,
                fine.frequency,
                fine.phase as usize,
                fine.amplitude,
            );
        }

        if !general_band_growth_allowed_at5(&local, &band_peaks, dominant_band) {
            active = false;
            continue;
        }

        let new_power = check_power_level_at5(&local[start..], &local[start..], count)?;
        let stop_after_accept = f64::from(ratio_limit) < new_power / f64::from(previous_power);
        if new_power <= f64::from(previous_power) {
            previous_power = new_power as f32;
            accepted += 1;
            if stop_after_accept {
                active = false;
            }
        } else {
            active = false;
        }
    }

    if mode == 0 {
        let output_count = finalize_general_mode0_at5(
            records,
            accepted,
            &raw_amplitudes,
            &raw_phases,
            &raw_frequencies,
            &scale_table,
            &amplitude_table,
        );
        Ok(output_count)
    } else {
        records[..accepted].sort_by_key(|record| record.frequency);
        Ok(accepted)
    }
}

pub fn analysis_general_wave_budgets_at5(
    energy_tables: &[&[f32]],
    group_count: usize,
    shared_flags: &[bool],
    selected_band_order: &[usize],
    threshold: i32,
) -> Result<Vec<Vec<usize>>, GhaAnalysisError> {
    let channel_count = energy_tables.len();
    if channel_count == 0 || channel_count > 2 {
        return Err(GhaAnalysisError::UnsupportedGeneralChannelCount { channel_count });
    }
    if group_count > MAX_SINE_WAVES_AT5 {
        return Err(GhaAnalysisError::UnsupportedGeneralGroupCount { group_count });
    }
    if shared_flags.len() < group_count {
        return Err(GhaAnalysisError::SharedFlagsTooShort {
            needed: group_count,
            actual: shared_flags.len(),
        });
    }
    if selected_band_order.len() < group_count {
        return Err(GhaAnalysisError::SelectedBandOrderTooShort {
            needed: group_count,
            actual: selected_band_order.len(),
        });
    }
    for (channel, table) in energy_tables.iter().enumerate() {
        if table.len() < group_count {
            return Err(GhaAnalysisError::EnergyTableTooShort {
                channel,
                needed: group_count,
                actual: table.len(),
            });
        }
    }

    let mut budgets = vec![vec![0_usize; group_count]; channel_count];
    if group_count == 0 {
        return Ok(budgets);
    }

    let total_budget = general_wave_budget_limit_at5(threshold);
    let mut db_weights = [0.0_f32; MAX_SINE_WAVES_AT5];
    let mut db_sum = 0.0_f32;
    for group in 0..group_count {
        let energy = energy_tables
            .iter()
            .fold(0.0_f32, |sum, table| sum + table[group]);
        let db = if energy >= 1.0 && energy > 0.0 {
            ((energy as f64).ln() as f32) * 8.685_889
        } else {
            0.0
        };
        db_weights[group] = db;
        db_sum += db;
    }

    if db_sum <= 0.0 {
        return Ok(budgets);
    }

    let mut group_budgets = [0_i32; MAX_SINE_WAVES_AT5];
    let distributable = total_budget - (group_count as i32 * 4) - 4;
    for group in 0..group_count {
        let rounded = (((distributable as f32 * db_weights[group]) / db_sum) + 0.5).floor() as i32;
        let mut budget = if rounded + 4 < 4 { 4 } else { rounded + 4 };
        if group < 2 {
            budget += 2;
        }
        group_budgets[group] = (budget - (budget >> 31)) & !1;
    }

    let assigned: i32 = group_budgets[..group_count].iter().sum();
    if 1 < total_budget - assigned {
        group_budgets[0] += total_budget - assigned;
    }

    let mut remaining = total_budget;
    for &group in &selected_band_order[..group_count] {
        if group >= group_count {
            return Err(GhaAnalysisError::InvalidGeneralGroupIndex { group, group_count });
        }

        let mut group_budget = group_budgets[group];
        if remaining < group_budget {
            group_budget = remaining;
        }

        if channel_count == 1 {
            budgets[0][group] = group_budget.max(0) as usize;
        } else if shared_flags[group] {
            budgets[0][group] = (group_budget - (group_budget >> 1)).max(0) as usize;
            budgets[1][group] = 0;
        } else {
            let first = group_budget - (group_budget >> 1);
            budgets[0][group] = first.max(0) as usize;
            budgets[1][group] = (group_budget - first).max(0) as usize;
        }

        for channel_budgets in &mut budgets {
            if channel_budgets[group] > 0x0f {
                channel_budgets[group] = 0x0f;
            }
        }

        remaining -= budgets
            .iter()
            .map(|channel_budgets| channel_budgets[group] as i32)
            .sum::<i32>();
    }

    Ok(budgets)
}

pub fn analysis_general_dispatch_plan_at5(
    budgets: &[Vec<usize>],
    selected_band_order: &[usize],
    shared_flags: &[bool],
    group_count: usize,
) -> Result<Vec<GeneralDispatchCall>, GhaAnalysisError> {
    let channel_count = budgets.len();
    if channel_count == 0 || channel_count > 2 {
        return Err(GhaAnalysisError::UnsupportedGeneralChannelCount { channel_count });
    }
    if group_count > MAX_SINE_WAVES_AT5 {
        return Err(GhaAnalysisError::UnsupportedGeneralGroupCount { group_count });
    }
    if selected_band_order.len() < group_count {
        return Err(GhaAnalysisError::SelectedBandOrderTooShort {
            needed: group_count,
            actual: selected_band_order.len(),
        });
    }
    if shared_flags.len() < group_count {
        return Err(GhaAnalysisError::SharedFlagsTooShort {
            needed: group_count,
            actual: shared_flags.len(),
        });
    }
    for (channel, channel_budgets) in budgets.iter().enumerate() {
        if channel_budgets.len() < group_count {
            return Err(GhaAnalysisError::BudgetTableTooShort {
                channel,
                needed: group_count,
                actual: channel_budgets.len(),
            });
        }
    }

    let mut calls = Vec::new();
    for &group in &selected_band_order[..group_count] {
        if group >= group_count {
            return Err(GhaAnalysisError::InvalidGeneralGroupIndex { group, group_count });
        }
        if channel_count == 2 && shared_flags[group] {
            let max_waves = budgets[0][group];
            if max_waves > 0 {
                calls.push(GeneralDispatchCall {
                    group_index: group,
                    channel_index: None,
                    max_waves,
                });
            }
        } else {
            for (channel, channel_budgets) in budgets.iter().enumerate() {
                let max_waves = channel_budgets[group];
                if max_waves > 0 {
                    calls.push(GeneralDispatchCall {
                        group_index: group,
                        channel_index: Some(channel),
                        max_waves,
                    });
                }
            }
        }
    }

    Ok(calls)
}

pub fn analysis_general_at5_compact_scheduler(
    energy_tables: &[&[f32]],
    selected_band_order: &[usize],
    shared_flags: &[bool],
    stereo_flags: &[bool],
    states: &mut [Vec<GeneralStateWords>],
    calls: &[GeneralSchedulerCallInput<'_>],
    group_count: usize,
    profile_selector: usize,
    record_arena_header: i32,
) -> Result<GeneralSchedulerResult, GhaAnalysisError> {
    let channel_count = energy_tables.len();
    if channel_count == 0 || channel_count > 2 {
        return Err(GhaAnalysisError::UnsupportedGeneralChannelCount { channel_count });
    }
    if stereo_flags.len() < group_count {
        return Err(GhaAnalysisError::StereoFlagsTooShort {
            needed: group_count,
            actual: stereo_flags.len(),
        });
    }
    if states.len() < channel_count {
        return Err(GhaAnalysisError::StateTableTooShort {
            channel: states.len(),
            needed: channel_count,
            actual: states.len(),
        });
    }
    for (channel, channel_states) in states.iter().enumerate().take(channel_count) {
        if channel_states.len() < group_count {
            return Err(GhaAnalysisError::StateTableTooShort {
                channel,
                needed: group_count,
                actual: channel_states.len(),
            });
        }
    }

    let budgets = analysis_general_wave_budgets_at5(
        energy_tables,
        group_count,
        shared_flags,
        selected_band_order,
        profile_selector as i32,
    )?;
    let plan = analysis_general_dispatch_plan_at5(
        &budgets,
        selected_band_order,
        shared_flags,
        group_count,
    )?;
    if calls.len() != plan.len() {
        return Err(GhaAnalysisError::SchedulerCallCountMismatch {
            expected: plan.len(),
            actual: calls.len(),
        });
    }

    let mut frequency_indices = vec![vec![None; group_count]; channel_count];
    let mut outputs = Vec::with_capacity(plan.len());
    let mut cumulative_waves = 0i32;
    for (call_index, (expected, call)) in plan.iter().zip(calls).enumerate() {
        if call.group_index != expected.group_index || call.channel_index != expected.channel_index
        {
            return Err(GhaAnalysisError::SchedulerCallLabelMismatch {
                call_index,
                expected_group: expected.group_index,
                actual_group: call.group_index,
                expected_channel: expected.channel_index,
                actual_channel: call.channel_index,
            });
        }

        let group = expected.group_index;
        let state_channel = expected.channel_index.unwrap_or(0);
        let detector_words = analysis_general_window_words_at5(call.source)?;
        let record_pointer_word = record_arena_header
            .wrapping_add(0xc)
            .wrapping_add(cumulative_waves.wrapping_mul(0x10));
        let mut state = states[state_channel][group];
        analysis_general_write_entry_state_at5(
            &mut state,
            detector_words,
            call.delayed_window_words,
            record_pointer_word,
        );
        let start = state[2] as usize;
        let end = state[3] as usize;

        let (initial_bin, count, records) = if expected.channel_index.is_none() {
            let (channel_a, channel_b) = call
                .shared_channel_samples
                .ok_or(GhaAnalysisError::MissingSharedChannelSamples { call_index, group })?;
            let (samples, weights) = analysis_general_shared_weights_at5(
                channel_a,
                channel_b,
                start,
                end,
                group,
                stereo_flags[group],
            )?;
            let initial_bin = analysis_general_initial_bin_at5(&samples, start, end, &weights)?;
            let mut records = call.initial_records.to_vec();
            let count = analysis_general_at5_sub(
                &samples,
                &mut records,
                start,
                end,
                1,
                initial_bin,
                &weights,
                expected.max_waves,
                channel_count,
                profile_selector,
            )?;
            (initial_bin, count, records)
        } else {
            let weights = analysis_general_tilt_weights_at5(call.samples, start, end, group)?;
            let initial_bin = analysis_general_initial_bin_at5(call.samples, start, end, &weights)?;
            let mut records = call.initial_records.to_vec();
            let count = analysis_general_at5_sub(
                call.samples,
                &mut records,
                start,
                end,
                1,
                initial_bin,
                &weights,
                expected.max_waves,
                channel_count,
                profile_selector,
            )?;
            (initial_bin, count, records)
        };

        frequency_indices[state_channel][group] = initial_bin;
        analysis_general_write_output_count_at5(&mut state, count)?;
        cumulative_waves = cumulative_waves.wrapping_add(count as i32);
        states[state_channel][group] = state;
        if expected.channel_index.is_none() && channel_count == 2 {
            let mut target = states[1][group];
            analysis_general_copy_shared_state_at5(state, &mut target);
            states[1][group] = target;
        }

        outputs.push(GeneralSchedulerCallOutput {
            group_index: group,
            channel_index: expected.channel_index,
            initial_bin,
            state,
            records,
        });
    }

    Ok(GeneralSchedulerResult {
        frequency_indices,
        calls: outputs,
    })
}

pub fn analysis_general_initial_bin_at5(
    samples: &[f32],
    start: usize,
    end: usize,
    dft_weights: &[f32],
) -> Result<Option<usize>, GhaAnalysisError> {
    validate_sine_inputs(samples.len(), start, end)?;
    if dft_weights.len() < 129 {
        return Err(GhaAnalysisError::DftWeightsTooShort {
            needed: 129,
            actual: dft_weights.len(),
        });
    }

    let mut local = [0.0_f32; COMPONENT_SAMPLES];
    local[start..end].copy_from_slice(&samples[start..end]);

    let mut dft_bins = [0.0_f32; 129];
    let ip_table = ip256_at5();
    let sc_table = sc256_at5();
    dft_x_at5(
        &local,
        COMPONENT_SAMPLES,
        &mut dft_bins,
        &ip_table,
        &sc_table,
    )?;
    for (bin, &weight) in dft_bins.iter_mut().zip(&dft_weights[..129]) {
        *bin *= weight;
    }

    Ok(strongest_positive_dft_bin_at5(&dft_bins))
}

pub fn analysis_general_tilt_weights_at5(
    samples: &[f32],
    start: usize,
    end: usize,
    group_index: usize,
) -> Result<[f32; 129], GhaAnalysisError> {
    validate_sine_inputs(samples.len(), start, end)?;

    let mut weights = [1.0_f32; 129];
    if group_index != 0 {
        return Ok(weights);
    }

    let mut local = [0.0_f32; COMPONENT_SAMPLES];
    local[start..end].copy_from_slice(&samples[start..end]);

    let mut dft_bins = [0.0_f32; 129];
    let ip_table = ip256_at5();
    let sc_table = sc256_at5();
    dft_x_at5(
        &local,
        COMPONENT_SAMPLES,
        &mut dft_bins,
        &ip_table,
        &sc_table,
    )?;

    let low = dft_bins[..0x40].iter().sum::<f32>();
    let high = dft_bins[0x40..0x80].iter().sum::<f32>();
    let mut ratio = 0.0_f32;
    if low > 0.0 && high > 0.0 {
        ratio = low / high;
    }
    if ratio > 16.0 {
        for weight in &mut weights[0x40..=0x80] {
            *weight = 0.0;
        }
    }

    Ok(weights)
}

pub fn analysis_general_shared_weights_at5(
    channel_a: &[f32],
    channel_b: &[f32],
    start: usize,
    end: usize,
    group_index: usize,
    invert: bool,
) -> Result<([f32; COMPONENT_SAMPLES], [f32; 129]), GhaAnalysisError> {
    validate_sine_inputs(channel_a.len(), start, end)?;
    if channel_b.len() < COMPONENT_SAMPLES {
        return Err(GhaSynthesisError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: channel_b.len(),
        }
        .into());
    }

    let mut mixed = [0.0_f32; COMPONENT_SAMPLES];
    if invert {
        invmix_seq_at5(channel_a, channel_b, &mut mixed, COMPONENT_SAMPLES)?;
    } else {
        mix_seq_at5(channel_a, channel_b, &mut mixed, COMPONENT_SAMPLES)?;
    }

    let ip_table = ip256_at5();
    let sc_table = sc256_at5();
    let mut local = [0.0_f32; COMPONENT_SAMPLES];
    local[start..end].copy_from_slice(&mixed[start..end]);
    let mut mixed_bins = [0.0_f32; 129];
    dft_x_at5(
        &local,
        COMPONENT_SAMPLES,
        &mut mixed_bins,
        &ip_table,
        &sc_table,
    )?;

    let mut channel_a_bins = [0.0_f32; 129];
    dft_x_at5(
        channel_a,
        COMPONENT_SAMPLES,
        &mut channel_a_bins,
        &ip_table,
        &sc_table,
    )?;
    let mut channel_b_bins = [0.0_f32; 129];
    dft_x_at5(
        channel_b,
        COMPONENT_SAMPLES,
        &mut channel_b_bins,
        &ip_table,
        &sc_table,
    )?;

    let mut weights = [0.0_f32; 129];
    for index in 0..129 {
        let denominator = channel_b_bins[index];
        if denominator > 0.0 {
            let ratio = channel_a_bins[index] / denominator;
            if ratio > 0.25 && ratio < 4.0 {
                weights[index] = 1.0;
            }
        }
    }

    if group_index == 0 {
        let low = mixed_bins[..0x40].iter().sum::<f32>();
        let high = mixed_bins[0x40..0x80].iter().sum::<f32>();
        let mut ratio = 0.0_f32;
        if low > 0.0 && high > 0.0 {
            ratio = low / high;
        }
        if ratio > 16.0 {
            for weight in &mut weights[0x40..=0x80] {
                *weight = 0.0;
            }
        }
    }

    Ok((mixed, weights))
}

pub fn analysis_general_window_words_at5(source: &[f32]) -> Result<[i32; 4], GhaAnalysisError> {
    if source.len() < GENERAL_WINDOW_SOURCE_SAMPLES {
        return Err(GhaSynthesisError::InputTooShort {
            needed: GENERAL_WINDOW_SOURCE_SAMPLES,
            actual: source.len(),
        }
        .into());
    }

    let peak_second_half = max_abs_at5(&source[0x40..0x80]);
    let peak_current_tail = max_abs_at5(&source[0x78..0x80]);
    let mut peak_next_first_half = max_abs_at5(&source[0x100..0x140]);
    let peak_next_head = max_abs_at5(&source[0x100..0x104]);

    let mut quad_peaks = [0.0_f32; 0x20];
    for (index, peak) in quad_peaks.iter_mut().enumerate() {
        let start = 0x80 + index * 4;
        *peak = max_abs_at5(&source[start..start + 4]);
    }

    let mut pair_peaks = [0.0_f32; 0x21];
    for index in (0..0x20).step_by(2) {
        let peak = quad_peaks[index].max(quad_peaks[index + 1]);
        pair_peaks[index + 1] = peak;
        pair_peaks[index + 2] = peak;
    }

    let mut strongest_index = 0usize;
    let mut strongest_peak = 0.0_f32;
    for (index, &peak) in quad_peaks.iter().enumerate() {
        if strongest_peak < peak {
            strongest_index = index;
            strongest_peak = peak;
        }
    }

    let mut lower_limit = strongest_index;
    if strongest_peak < peak_next_first_half {
        lower_limit = 0x20;
    }

    let mut ratios = [0.0_f32; 0x20];
    let mut running_peak = peak_second_half;
    for index in 0..0x20 {
        let peak = quad_peaks[index];
        if running_peak < peak {
            running_peak = peak;
        }
        if index < 0x1f {
            if running_peak * 4.0 < quad_peaks[index + 1] && strongest_peak > 0.0 {
                ratios[index] = peak / strongest_peak;
            }
        } else if peak_next_head > running_peak * 4.0 && strongest_peak > 0.0 {
            ratios[index] = peak / peak_next_head;
        }
    }

    let mut lower_active = 0;
    let mut lower_index = 0usize;
    for (index, &ratio) in ratios[..lower_limit].iter().enumerate() {
        if ratio > 0.0 {
            lower_active = 1;
            lower_index = index;
        }
    }
    if lower_active != 0 {
        if lower_index < 0x1e {
            lower_index += 2;
        } else if lower_index < 0x1f {
            lower_index += 1;
        }
    }

    let mut upper_limit = strongest_index;
    if strongest_peak < peak_second_half {
        upper_limit = 0;
    }

    ratios.fill(0.0);
    if upper_limit < 0x20 {
        for index in (upper_limit..0x20).rev() {
            let peak = pair_peaks[index + 1];
            if peak_next_first_half < peak {
                peak_next_first_half = peak;
            }
            if index < 1 {
                if peak_next_first_half + peak_next_first_half < peak_current_tail {
                    ratios[index] = peak / peak_current_tail;
                }
            } else if peak_next_first_half + peak_next_first_half < pair_peaks[index]
                && strongest_peak > 0.0
            {
                ratios[index] = peak / strongest_peak;
            }
        }
    }

    let mut upper_active = 0;
    let mut upper_index = 0x1fusize;
    if upper_limit < 0x20 {
        for index in (upper_limit..0x20).rev() {
            if ratios[index] > 0.0 {
                upper_active = 1;
                upper_index = index;
            }
        }
        if upper_active == 1 && upper_index > 0x1d {
            upper_index = 0x1f;
        }
    }

    let lower_word = if lower_active == 0 {
        -1
    } else {
        lower_index as i32
    };
    let upper_word = if upper_active == 0 {
        0x20
    } else {
        upper_index as i32
    };

    Ok([lower_active, upper_active, lower_word, upper_word])
}

pub fn analysis_general_active_window_words_at5(
    detector_words: [i32; 4],
    delayed_words: [i32; 4],
) -> [i32; 4] {
    let (lower_active, start) = if detector_words[0] == 0 || detector_words[3] <= detector_words[2]
    {
        if delayed_words[0] != 0 {
            (1, delayed_words[2] << 2)
        } else {
            (0, 0)
        }
    } else {
        (1, detector_words[2] * 4 + 0x80)
    };

    let (upper_active, mut end) = if delayed_words[1] == 0 || delayed_words[3] * 4 < start {
        if detector_words[1] != 0 {
            (1, detector_words[3] * 4 + 0x80)
        } else {
            (0, 0x100)
        }
    } else {
        (1, delayed_words[3] * 4)
    };
    if end + 4 < 0x101 {
        end += 4;
    } else {
        end = 0x100;
    }

    [lower_active, upper_active, start, end]
}

pub fn analysis_general_write_entry_state_at5(
    state: &mut GeneralStateWords,
    detector_words: [i32; 4],
    delayed_words: [i32; 4],
    record_pointer_word: i32,
) {
    let active_words = analysis_general_active_window_words_at5(detector_words, delayed_words);
    state[..4].copy_from_slice(&active_words);
    state[4..8].copy_from_slice(&detector_words);
    state[9] = record_pointer_word;
}

pub fn analysis_general_write_output_count_at5(
    state: &mut GeneralStateWords,
    output_count: usize,
) -> Result<(), GhaAnalysisError> {
    if output_count > MAX_SINE_WAVES_AT5 {
        return Err(GhaAnalysisError::UnsupportedSineWaveCount {
            max_waves: output_count,
        });
    }

    state[8] = output_count as i32;
    Ok(())
}

pub fn analysis_general_copy_shared_state_at5(
    source: GeneralStateWords,
    target: &mut GeneralStateWords,
) {
    *target = source;
}

fn shell_sort_descending_by_residual(candidates: &mut [Candidate]) {
    let mut gap = 1;
    while gap <= candidates.len() {
        gap = gap * 3 + 1;
    }

    loop {
        gap /= 3;
        if gap < 1 {
            break;
        }

        for index in gap..candidates.len() {
            let value = candidates[index];
            let mut cursor = index;
            while cursor >= gap && candidates[cursor - gap].residual_power < value.residual_power {
                candidates[cursor] = candidates[cursor - gap];
                cursor -= gap;
            }
            candidates[cursor] = value;
        }
    }
}

fn validate_sine_inputs(
    sample_len: usize,
    start: usize,
    end: usize,
) -> Result<(), GhaAnalysisError> {
    if start > end || end > COMPONENT_SAMPLES {
        return Err(GhaSynthesisError::InvalidRange { start, end }.into());
    }
    let sample_count = end - start;
    if sample_count % 4 != 0 {
        return Err(GhaSynthesisError::SampleCountNotMultipleOfFour { sample_count }.into());
    }
    if sample_len < COMPONENT_SAMPLES {
        return Err(GhaSynthesisError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: sample_len,
        }
        .into());
    }
    Ok(())
}

fn quantize_gha_scale_at5(amplitude: f32, scale_table: &[f32]) -> usize {
    let target = amplitude * GHA_SCALE_PREBIAS;
    if target < GHA_SCALE_MINIMUM {
        return 0;
    }

    let mut index = 0x20isize;
    let mut step = 0x10isize;
    loop {
        while scale_table[index as usize] <= target {
            index += step;
            step >>= 1;
            if step == 0 {
                break;
            }
        }
        if step == 0 {
            break;
        }
        index -= step;
        step >>= 1;
        if step == 0 {
            break;
        }
    }

    let mut index = index as usize;
    if index < 0x3f && scale_table[index] < target {
        index += 1;
    }
    index
}

fn quantize_phase_index_at5(phase: u32) -> usize {
    (((phase as f32 * 0.015_625 + 0.5).floor() as i32) & 0x1f) as usize
}

fn quantize_general_amplitude_index_at5(
    amplitude: f32,
    scale: f32,
    amplitude_table: &[f32],
) -> usize {
    let target = amplitude / scale - GHA_GENERAL_AMPLITUDE_BIAS;
    if target < 0.0 {
        return usize::MAX;
    }
    if target < GHA_GENERAL_AMPLITUDE_ZERO_WIDTH {
        return 0;
    }

    let mut index = 8isize;
    let mut step = 4isize;
    loop {
        while target < amplitude_table[index as usize] {
            index -= step;
            step >>= 1;
            if step == 0 {
                break;
            }
        }
        if step == 0 {
            break;
        }
        index += step;
        step >>= 1;
        if step == 0 {
            break;
        }
    }

    let mut index = index as usize;
    if index < 0x0f && amplitude_table[index] < target {
        index += 1;
    }
    index
}

fn finalize_general_mode0_at5(
    records: &mut [GhaWaveRecord],
    accepted: usize,
    raw_amplitudes: &[f32; MAX_SINE_WAVES_AT5],
    raw_phases: &[u32; MAX_SINE_WAVES_AT5],
    raw_frequencies: &[usize; MAX_SINE_WAVES_AT5],
    scale_table: &[f32],
    amplitude_table: &[f32],
) -> usize {
    if accepted == 0 {
        return 0;
    }

    for index in 0..accepted {
        records[index].frequency = raw_frequencies[index];
    }

    let max_amplitude = raw_amplitudes[..accepted]
        .iter()
        .map(|amplitude| amplitude.abs())
        .fold(0.0_f32, f32::max);
    let scale_index = quantize_gha_scale_at5(max_amplitude, scale_table);
    let scale = scale_table[scale_index];

    for index in 0..accepted {
        records[index].scale_index = scale_index;
        records[index].amplitude_index =
            quantize_general_amplitude_index_at5(raw_amplitudes[index], scale, amplitude_table);
        records[index].phase_index = quantize_phase_index_at5(raw_phases[index]);
        records[index].frequency = raw_frequencies[index];
    }

    let empty = GhaWaveRecord {
        scale_index: 0,
        amplitude_index: 0,
        phase_index: 0,
        frequency: 0,
    };
    let mut kept = [empty; MAX_SINE_WAVES_AT5];
    let mut kept_count = 0usize;
    for record in records[..accepted].iter().copied() {
        let amplitude_gate = record.amplitude_index.wrapping_add(1);
        if amplitude_gate.wrapping_mul(record.scale_index) != 0 {
            kept[kept_count] = record;
            kept_count += 1;
        }
    }

    if kept_count > 0 {
        kept[..kept_count].sort_by_key(|record| record.frequency);
        records[..kept_count].copy_from_slice(&kept[..kept_count]);
    }

    kept_count
}

fn general_window_scale_at5(count: usize) -> f32 {
    match count {
        256 => 1.0,
        224..=255 => 0.9,
        192..=223 => 0.8,
        160..=191 => 0.7,
        128..=159 => 0.6,
        _ => 0.5,
    }
}

fn general_band_peaks_at5(samples: &[f32]) -> [f32; 8] {
    let mut peaks = [0.0_f32; 8];
    for (band, peak) in peaks.iter_mut().enumerate() {
        let start = band * 32;
        let end = start + 32;
        for &sample in &samples[start..end] {
            let magnitude = sample.abs();
            if *peak < magnitude {
                *peak = magnitude;
            }
        }
    }
    peaks
}

fn max_abs_at5(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max)
}

fn strongest_band_at5(peaks: &[f32; 8]) -> usize {
    let mut best_index = 0usize;
    let mut best_value = 0.0_f32;
    for (index, &value) in peaks.iter().enumerate() {
        if best_value < value {
            best_index = index;
            best_value = value;
        }
    }
    best_index
}

fn general_band_growth_allowed_at5(
    samples: &[f32; COMPONENT_SAMPLES],
    original_peaks: &[f32; 8],
    dominant_band: usize,
) -> bool {
    for band in 0..dominant_band {
        let start = band * 32;
        let end = start + 32;
        let mut residual_peak = 0.0_f32;
        for &sample in &samples[start..end] {
            let magnitude = sample.abs();
            if residual_peak < magnitude {
                residual_peak = magnitude;
            }
        }
        if original_peaks[band] * 1.75 < residual_peak {
            return false;
        }
    }
    true
}

fn general_power_ratio_limit_at5(channel_count: usize, profile_selector: usize) -> f32 {
    let threshold = if channel_count == 2 { 0x16 } else { 0x12 };
    if profile_selector <= threshold {
        0.3
    } else {
        0.9
    }
}

fn general_wave_budget_limit_at5(threshold: i32) -> i32 {
    if threshold < 5 {
        3
    } else if threshold <= 10 {
        6
    } else if threshold <= 12 {
        12
    } else if threshold <= 14 {
        24
    } else {
        48
    }
}

fn strongest_positive_dft_bin_at5(bins: &[f32; 129]) -> Option<usize> {
    let mut best_index = None;
    let mut best_value = 0.0_f32;
    for (index, &value) in bins.iter().enumerate() {
        if value > best_value {
            best_index = Some(index);
            best_value = value;
        }
    }
    best_index
}

fn subtract_quantized_sine_at5(
    samples: &mut [f32; COMPONENT_SAMPLES],
    start: usize,
    end: usize,
    frequency: usize,
    phase_index: usize,
    scale: f32,
) {
    subtract_sine_at5(samples, start, end, frequency, phase_index * 0x40, scale);
}

fn subtract_sine_at5(
    samples: &mut [f32; COMPONENT_SAMPLES],
    start: usize,
    end: usize,
    frequency: usize,
    phase_units: usize,
    scale: f32,
) {
    let mut phase = (start as i64 - 0x81) * frequency as i64 + phase_units as i64;
    let sin_table = sin_at5();
    for sample in &mut samples[start..end] {
        let phase_index = (phase + frequency as i64).rem_euclid(SIN_AT5_ENTRIES as i64) as usize;
        *sample =
            (f64::from(*sample) - f64::from(scale) * f64::from(sin_table[phase_index])) as f32;
        phase = phase_index as i64;
    }
}

impl From<GhaSynthesisError> for GhaAnalysisError {
    fn from(error: GhaSynthesisError) -> Self {
        Self::Component(error)
    }
}

impl From<FftError> for GhaAnalysisError {
    fn from(error: FftError) -> Self {
        Self::Fft(error)
    }
}

impl From<PowerCheckError> for GhaAnalysisError {
    fn from(error: PowerCheckError) -> Self {
        Self::Power(error)
    }
}

impl From<ScalarError> for GhaAnalysisError {
    fn from(error: ScalarError) -> Self {
        Self::Scalar(error)
    }
}
