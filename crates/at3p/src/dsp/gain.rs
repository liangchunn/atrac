pub const GAIN_PASS_BINS: usize = 33;
pub const GAIN_PASS_POINTS: usize = 7;
pub const CHECK_GC_STORAGE_SLOTS: usize = 35;
pub const CHECK_GC_MAX_END: usize = 0x40;
pub const CHECK_GC_BASE_OFFSET: usize = 1;
pub const CHECK_GC_DEFERRED_SLOTS: usize = 112;
pub const CHECK_GC_RECORDS: usize = 8;
pub const CHECK_GC_RECORD_WORDS: usize = 6;
pub const GC_SET_POINTS_ARRAY_VALUES: usize = 40;
pub const GC_SET_POINTS_SOURCE_DEST_WORDS: usize = 12;
pub const GC_SET_POINTS_BOUNDS_WORDS: usize = 8;
pub const GC_SET_POINTS_OUTPUT_GROUPS: usize = 2;
pub const GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS: usize = 0x300;
pub const GC_SET_POINTS_OUTPUT_RECORD_WORDS: usize = 12;
pub const GAIN_WINDOW_POINT_WORDS: usize = 15;
pub const GAIN_WINDOW_POINTS: usize = 7;
pub const GAIN_WINDOW_LEVEL_SLOTS: usize = 64;
pub const GAIN_WINDOW_VALUES: usize = 256;

#[allow(clippy::approx_constant)]
const LOG2_E_AT5: f32 = 1.442695;
const GC_LOG_BIAS_8_AT5: f32 = f32::from_bits(0x3d978d9e);
const GC_LOG_BIAS_16_AT5: f32 = f32::from_bits(0x3e4544c0);
const GC_LOG_BIAS_32_AT5: f32 = f32::from_bits(0x3ea4d3c2);
const GC_LOG_BIAS_BOUND_AT5: f32 = f32::from_bits(0x3ed47fcc);
const GAIN_WINDOW_LNGAIN_AT5: [i32; 16] = [-6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
const GAIN_WINDOW_ITP_AT5: [[u32; 3]; 15] = [
    [0x3f183800, 0x3f350400, 0x3f574400],
    [0x3eb50400, 0x3f000000, 0x3f350400],
    [0x3e574800, 0x3eb50400, 0x3f183800],
    [0x3e000000, 0x3e800000, 0x3f000000],
    [0x3d983000, 0x3e350800, 0x3ed74400],
    [0x3d350000, 0x3e000000, 0x3eb50400],
    [0x3cd74000, 0x3db50000, 0x3e983800],
    [0x3c800000, 0x3d800000, 0x3e800000],
    [0x3c180000, 0x3d350000, 0x3e574800],
    [0x3bb50000, 0x3d000000, 0x3e350800],
    [0x3b580000, 0x3cb50000, 0x3e183800],
    [0x3b000000, 0x3c800000, 0x3e000000],
    [0x3a980000, 0x3c350000, 0x3dd74000],
    [0x3a380000, 0x3c000000, 0x3db50000],
    [0x39d00000, 0x3bb50000, 0x3d983000],
];

#[derive(Debug, Clone, PartialEq)]
pub struct GainPassPoints {
    pub reserved: i32,
    pub locations: [i32; GAIN_PASS_POINTS],
    pub levels: [i32; GAIN_PASS_POINTS],
    pub fractions: [f32; GAIN_PASS_POINTS],
}

impl Default for GainPassPoints {
    fn default() -> Self {
        Self {
            reserved: 0,
            locations: [0; GAIN_PASS_POINTS],
            levels: [0; GAIN_PASS_POINTS],
            fractions: [0.0; GAIN_PASS_POINTS],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckGcCandidate {
    pub start: usize,
    pub end: usize,
    pub width_bits: u32,
}

impl CheckGcCandidate {
    pub fn width(self) -> f32 {
        f32::from_bits(self.width_bits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckGcConfig {
    pub mode: i32,
    pub guard_peak: f32,
    pub ratio_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckGcRecord {
    words: [u32; CHECK_GC_RECORD_WORDS],
}

impl CheckGcRecord {
    pub fn from_words(words: [u32; CHECK_GC_RECORD_WORDS]) -> Self {
        Self { words }
    }

    pub fn words(self) -> [u32; CHECK_GC_RECORD_WORDS] {
        self.words
    }
}

impl Default for CheckGcRecord {
    fn default() -> Self {
        Self {
            words: [0; CHECK_GC_RECORD_WORDS],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcSetPointWords {
    words: [u32; GC_SET_POINTS_SOURCE_DEST_WORDS],
    index: i32,
}

impl GcSetPointWords {
    pub fn from_words(words: [u32; GC_SET_POINTS_SOURCE_DEST_WORDS], index: i32) -> Self {
        Self { words, index }
    }

    pub fn words(self) -> [u32; GC_SET_POINTS_SOURCE_DEST_WORDS] {
        self.words
    }

    pub fn index(self) -> i32 {
        self.index
    }

    fn word_f32(self, index: usize) -> f32 {
        f32::from_bits(self.words[index])
    }

    fn word_i32(self, index: usize) -> i32 {
        self.words[index] as i32
    }

    fn set_f32(&mut self, index: usize, value: f32) {
        self.words[index] = value.to_bits();
    }

    fn set_i32(&mut self, index: usize, value: i32) {
        self.words[index] = value as u32;
    }
}

impl Default for GcSetPointWords {
    fn default() -> Self {
        Self {
            words: [0; GC_SET_POINTS_SOURCE_DEST_WORDS],
            index: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GainPassError {
    StepZero,
    IndexOutOfRange {
        name: &'static str,
        value: usize,
        max: usize,
    },
    EnergyTooShort {
        needed: usize,
        actual: usize,
    },
    CompareTooShort {
        needed: usize,
        actual: usize,
    },
    PointCountOutOfRange {
        point_count: usize,
        max: usize,
    },
    StorageTooShort {
        name: &'static str,
        needed: usize,
        actual: usize,
    },
    CountOutOfRange {
        name: &'static str,
        value: usize,
        max: usize,
    },
    InvalidRange {
        start: usize,
        end: usize,
    },
}

pub fn attack_pass_at5(
    high_index: usize,
    step: usize,
    mut point_count: usize,
    other_point_count: usize,
    first_rounding_flag: &mut i32,
    total_level: &mut i32,
    level_limit: i32,
    current_level: &mut i32,
    energy: &[f32],
    mut peak: f32,
    mut previous_peak: f32,
    threshold: f32,
    points: &mut GainPassPoints,
    fractional: bool,
) -> Result<usize, GainPassError> {
    check_step(step)?;
    check_index("high_index", high_index)?;
    check_point_count(point_count)?;
    check_energy(energy, attack_energy_needed(high_index, step))?;

    let mut index = 0usize;
    while index < high_index {
        if other_point_count + point_count > 5 {
            return Ok(point_count);
        }

        let current_energy = energy[index];
        if peak < current_energy {
            peak = current_energy;
        }
        if point_count == 0 && previous_peak >= 0.0 {
            if previous_peak < current_energy {
                previous_peak = current_energy;
            }
            if index > 0x19 && previous_peak < peak {
                peak = previous_peak;
            }
        }

        index += step;
        let candidate_energy = energy[index];
        if threshold * peak < candidate_energy {
            let ratio = if peak <= 4.0 {
                f64::from(candidate_energy) * 0.25
            } else {
                f64::from(candidate_energy) / f64::from(peak)
            };
            let mut residual = 0.0f32;
            let level = gain_level_from_ratio(
                ratio,
                point_count,
                *first_rounding_flag,
                points,
                fractional,
                &mut residual,
            );
            if !fractional && *first_rounding_flag != 0 {
                *first_rounding_flag = 0;
            }

            if level > 0 {
                let old_level = *current_level;
                let requested_level = old_level + level;
                let stored_level = if level_limit < requested_level {
                    *current_level = level_limit;
                    level_limit - old_level
                } else {
                    *current_level = requested_level;
                    level
                };

                *total_level += stored_level;
                points.levels[point_count] = stored_level;
                points.locations[point_count] = index as i32 - 1;
                if fractional {
                    points.fractions[point_count] = residual;
                }
                point_count += 1;
            }
        }
    }

    Ok(point_count)
}

pub fn release_pass_at5(
    low_index: usize,
    step: usize,
    other_point_count: usize,
    mut point_count: usize,
    first_bin_mode: i32,
    current_level: &mut i32,
    level_limit: i32,
    peak_out: &mut f32,
    energy: &[f32],
    compare: &[f32],
    mut peak: f32,
    threshold: f32,
    points: &mut GainPassPoints,
    fractional: bool,
) -> Result<usize, GainPassError> {
    check_step(step)?;
    check_index("low_index", low_index)?;
    check_point_count(point_count)?;
    check_energy(energy, GAIN_PASS_BINS)?;
    check_compare(compare, GAIN_PASS_BINS)?;

    let low_index = low_index as isize;
    let step = step as isize;
    let mut index = 0x20isize;
    while low_index < index {
        let index_usize = index as usize;
        let tail_energy = energy[index_usize];
        let mut updated_peak = peak;
        if peak < tail_energy {
            updated_peak = tail_energy;
            if index_usize == 0x20 {
                updated_peak = peak;
                if first_bin_mode == 0 {
                    updated_peak = energy[0x20];
                }
            }
        }
        peak = updated_peak;

        let candidate_energy = energy[index_usize - 1];
        if threshold * peak < candidate_energy {
            let ratio = if peak <= 4.0 {
                f64::from(candidate_energy) * 0.25
            } else {
                f64::from(candidate_energy) / f64::from(peak)
            };
            let mut residual = 0.0f32;
            let level =
                gain_level_from_ratio(ratio, point_count, 0, points, fractional, &mut residual);

            if level > 0 {
                let old_level = *current_level;
                let requested_level = old_level + level;
                let stored_level = if level_limit < requested_level {
                    *current_level = level_limit;
                    level_limit - old_level
                } else {
                    *current_level = requested_level;
                    level
                };

                points.levels[point_count] = stored_level;
                if fractional {
                    points.fractions[point_count] = residual;
                }
                if point_count == 0 {
                    *peak_out = peak;
                }

                let location = release_location(index, step, energy, compare);
                points.locations[point_count] = location as i32;
                point_count += 1;

                if other_point_count + point_count > 6 {
                    return Ok(point_count);
                }
            }
        }

        index -= step;
    }

    Ok(point_count)
}

pub fn gc_set_points_at5(
    spectrum: &[f32],
    envelope: &[f32],
    bounds_words: &[u32],
    source: &GcSetPointWords,
    dest: &mut GcSetPointWords,
    output_records: &mut [u32],
    counts: &mut [i32],
) -> Result<i32, GainPassError> {
    check_storage("spectrum", spectrum.len(), GC_SET_POINTS_ARRAY_VALUES)?;
    check_storage("envelope", envelope.len(), GC_SET_POINTS_ARRAY_VALUES)?;
    check_storage(
        "bounds_words",
        bounds_words.len(),
        GC_SET_POINTS_BOUNDS_WORDS,
    )?;
    check_storage(
        "output_records",
        output_records.len(),
        GC_SET_POINTS_OUTPUT_GROUPS * GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS,
    )?;
    check_storage("counts", counts.len(), GC_SET_POINTS_OUTPUT_GROUPS)?;

    let source_index = gc_set_points_index("source_index", source.index(), spectrum.len())?;
    let dest_index = dest.index();
    let dest_index_usize = gc_set_points_index("dest_index", dest_index, spectrum.len())?;
    let lower = dest.word_i32(6);
    let upper = dest.word_i32(7);
    let lower_bound = bounds_words[6] as i32;
    let upper_bound = bounds_words[7] as i32;

    for word in 2..=5 {
        dest.words[word] = source.words[word];
    }

    let mut result = 0;
    let mut clamped = false;

    if lower + 4 < dest_index {
        dest.set_i32(8, (dest_index != lower + 1) as i32);
    } else if lower == lower_bound {
        dest.set_i32(8, (dest_index != lower + 1) as i32);
    } else {
        dest.set_i32(8, 0);
        result = dest_index - lower - 1;
    }

    if dest_index + 4 < upper || upper == upper_bound {
        dest.set_i32(9, (upper != dest_index + 1) as i32);
    } else {
        dest.set_i32(9, 0);
        result += upper - dest_index - 1;
    }

    let upper_group = (upper - 1 > 0x1f) as usize;
    let lower_group = (lower + 1 > 0x1f) as usize;
    let dest_value = spectrum[dest_index_usize];
    let mut envelope_scale = 1.0f32;
    let mut level = 0i32;

    if spectrum[source_index] > 0.0 {
        if dest_value > 0.0 {
            if lower != lower_bound {
                let lower_env_index =
                    gc_set_points_index("lower_envelope_index", lower + 1, envelope.len())?;
                envelope_scale = envelope[lower_env_index];
            }
            if upper != upper_bound {
                let upper_env_index =
                    gc_set_points_index("upper_envelope_index", upper, envelope.len())?;
                let upper_scale = envelope[upper_env_index];
                if envelope_scale < upper_scale {
                    envelope_scale = upper_scale;
                }
            }

            let scaled_value = dest_value * envelope_scale;
            if scaled_value <= source.word_f32(0) {
                let log_level = (f64::from(source.word_f32(0)) / f64::from(scaled_value)).ln()
                    * f64::from(LOG2_E_AT5);
                let mut log_level = log_level as f32;
                if lower == lower_bound {
                    log_level += GC_LOG_BIAS_BOUND_AT5;
                } else if upper == upper_bound {
                    log_level += GC_LOG_BIAS_16_AT5;
                } else {
                    log_level += gc_set_points_span_log_bias(upper - lower - 1);
                }
                level = trunc_to_i32(f64::from(log_level));
            }
        }

        if upper != upper_bound {
            let slot = upper_group * 2 + 2;
            let existing = dest.word_i32(slot);
            if existing + level > 9 {
                clamped = true;
                level = 9 - existing;
            }
        }
        if lower != lower_bound {
            let slot = lower_group * 2 + 3;
            let existing = dest.word_i32(slot);
            if existing + level > 6 {
                clamped = true;
                level = 6 - existing;
            }
        }
    }

    let should_emit = if lower == lower_bound || upper == upper_bound {
        level != 0
    } else {
        level >= gc_set_points_min_level(upper - lower - 1)
    };

    if should_emit {
        let scaled_level = level * dest.word_i32(1);
        dest.set_i32(11, source.word_i32(11) + 1);

        let mut upper_record = None;
        if upper != upper_bound {
            let record_offset = gc_set_points_record_offset(counts, upper_group)?;
            output_records[record_offset + 5] = 0;
            output_records[record_offset + 10] = 0;
            output_records[record_offset + 11] = dest.word_i32(11) as u32;
            output_records[record_offset] = gc_set_points_mod32(upper - 1) as u32;
            output_records[record_offset + 6] = scaled_level as u32;
            output_records[record_offset + 1] = level as u32;
            output_records[record_offset + 7] = 0;

            let slot = upper_group * 2 + 2;
            dest.set_i32(slot, dest.word_i32(slot) + level);
            counts[upper_group] += 1;
            upper_record = Some((record_offset, upper_group));
        }

        if lower != lower_bound {
            let record_offset = gc_set_points_record_offset(counts, lower_group)?;
            output_records[record_offset + 5] = 0;
            output_records[record_offset + 10] = 0;
            output_records[record_offset + 11] = dest.word_i32(11) as u32;
            output_records[record_offset] = gc_set_points_mod32(lower + 1) as u32;
            output_records[record_offset + 6] = scaled_level as u32;
            output_records[record_offset + 1] = (-level) as u32;

            if let Some((upper_record_offset, upper_record_group)) = upper_record {
                output_records[record_offset + 7] = 1;
                output_records[record_offset + 8] =
                    (upper_record_group as i32 - lower_group as i32) as u32;
                output_records[record_offset + 9] = (counts[upper_record_group] - 1) as u32;
                output_records[upper_record_offset + 7] = 1;
                output_records[upper_record_offset + 8] =
                    (lower_group as i32 - upper_record_group as i32) as u32;
                output_records[upper_record_offset + 9] = counts[lower_group] as u32;
            } else {
                output_records[record_offset + 7] = 0;
            }

            let slot = lower_group * 2 + 3;
            dest.set_i32(slot, dest.word_i32(slot) + level);
            counts[lower_group] += 1;
        }

        dest.set_f32(0, dest_value);
    } else {
        dest.set_i32(11, source.word_i32(11));
        if clamped {
            dest.set_f32(0, dest_value);
        } else {
            dest.set_f32(
                0,
                dest_value + (source.word_f32(0) - dest_value) / envelope_scale,
            );
        }
    }

    Ok(result)
}

pub fn gainc_window_enc_at5(
    attack_words: &[i32],
    release_words: &[i32],
    window: &mut [f32],
) -> Result<usize, GainPassError> {
    check_storage("attack_words", attack_words.len(), GAIN_WINDOW_POINT_WORDS)?;
    check_storage(
        "release_words",
        release_words.len(),
        GAIN_WINDOW_POINT_WORDS,
    )?;
    check_storage("window", window.len(), GAIN_WINDOW_VALUES)?;

    let mut levels = [0i32; GAIN_WINDOW_LEVEL_SLOTS];
    fill_gain_window_release_levels(release_words, &mut levels)?;
    add_gain_window_attack_levels(attack_words, &mut levels)?;

    let mut previous_level = 0;
    let mut scale = gain_window_scale(levels[GAIN_WINDOW_LEVEL_SLOTS - 1]);
    let mut output_index = GAIN_WINDOW_VALUES - 1;
    let mut first_changed_index = None;

    for slot in (0..GAIN_WINDOW_LEVEL_SLOTS).rev() {
        let level = levels[slot];
        if level == previous_level {
            for offset in 0..4 {
                window[output_index - offset] = scale;
            }
        } else {
            if first_changed_index.is_none() {
                first_changed_index = Some(output_index);
            }

            let row_index = (level - previous_level).unsigned_abs() as usize - 1;
            if row_index >= GAIN_WINDOW_ITP_AT5.len() {
                return Err(GainPassError::CountOutOfRange {
                    name: "gain_window_delta",
                    value: row_index + 1,
                    max: GAIN_WINDOW_ITP_AT5.len(),
                });
            }
            let row = gain_window_itp_row(row_index);

            if level < previous_level {
                let base = gain_window_scale(previous_level);
                window[output_index] = base * row[2];
                window[output_index - 1] = base * row[1];
                window[output_index - 2] = base * row[0];
            } else {
                let base = gain_window_scale(level);
                window[output_index] = base * row[0];
                window[output_index - 1] = base * row[1];
                window[output_index - 2] = base * row[2];
            }

            scale = gain_window_scale(level);
            window[output_index - 3] = scale;
            previous_level = level;
        }
        output_index = output_index.saturating_sub(4);
    }

    Ok(first_changed_index.unwrap_or(0xff))
}

pub fn check_gc_at5(
    config: CheckGcConfig,
    candidate: CheckGcCandidate,
    deferred: &mut [i32],
    power: &[f32],
    gain: &mut [i32],
    delta: &mut [i32],
    records: &mut [CheckGcRecord],
    record_count: &mut usize,
    budget_count: &mut i32,
    threshold: f32,
) -> Result<(), GainPassError> {
    check_gc_candidate(candidate)?;
    check_storage("power", power.len(), CHECK_GC_STORAGE_SLOTS)?;
    check_storage("gain", gain.len(), CHECK_GC_STORAGE_SLOTS)?;
    check_storage("delta", delta.len(), CHECK_GC_STORAGE_SLOTS)?;
    check_storage("deferred", deferred.len(), CHECK_GC_DEFERRED_SLOTS)?;
    check_storage("records", records.len(), CHECK_GC_RECORDS)?;
    check_count(
        "record_count",
        *record_count,
        records.len().saturating_sub(1),
    )?;

    let start = candidate.start;
    let end = candidate.end;
    let width = candidate.width();

    let left = check_gc_scale_from_shift(gain[check_gc_slot(start as isize - 1)])
        * power[check_gc_slot(start as isize - 1)]
        / (check_gc_scale_from_shift(gain[check_gc_slot(start as isize)]) * width);

    let right = if end < GAIN_PASS_BINS {
        check_gc_scale_from_shift(gain[check_gc_slot(end as isize)])
            * power[check_gc_slot(end as isize)]
            / (check_gc_scale_from_shift(gain[check_gc_slot(end as isize - 1)]) * width)
    } else {
        power[check_gc_slot(end as isize)] / width
    };

    let ratio = (if left < right { left } else { right }) / config.ratio_scale;
    let mut level = check_gc_level_from_ratio(ratio);
    if config.mode == 0 && width + width > threshold {
        level = 0;
    } else if level == 1 && width < 5.0 && config.guard_peak > 100.0 {
        level = 0;
    }

    let mut deferred_budget = *budget_count;
    if level > 0 {
        let mut next_budget = *budget_count;
        if delta[check_gc_slot(start as isize - 1)] == 0 {
            next_budget += 1;
        }
        if end < GAIN_PASS_BINS && delta[check_gc_slot(end as isize - 1)] == 0 {
            next_budget += 1;
        }

        if next_budget < 8 {
            let span = end - start;
            records[*record_count] = CheckGcRecord::from_words([
                ratio.to_bits(),
                start as u32,
                end as u32,
                span as u32,
                level as u32,
                candidate.width_bits,
            ]);
            *record_count += 1;

            if end < GAIN_PASS_BINS {
                for index in start..end {
                    gain[check_gc_slot(index as isize)] += level;
                }
                delta[check_gc_slot(start as isize - 1)] =
                    gain[check_gc_slot(start as isize - 1)] - gain[check_gc_slot(start as isize)];
                delta[check_gc_slot(end as isize - 1)] =
                    gain[check_gc_slot(end as isize - 1)] - gain[check_gc_slot(end as isize)];
            } else {
                for index in start..GAIN_PASS_BINS {
                    gain[check_gc_slot(index as isize)] += level;
                }
                delta[check_gc_slot(start as isize - 1)] =
                    gain[check_gc_slot(start as isize - 1)] - gain[check_gc_slot(start as isize)];
            }

            *budget_count = next_budget;
            deferred_budget = next_budget;
        }
    }

    if start < 0x20 && deferred_budget < 7 && end - start > 7 {
        let deferred_count = deferred[0];
        if deferred_count < 0 {
            return Err(GainPassError::CountOutOfRange {
                name: "deferred_count",
                value: deferred_count as usize,
                max: 6,
            });
        }
        let deferred_count = deferred_count as usize;
        check_count("deferred_count", deferred_count, 6)?;
        deferred[deferred_count + 1] = start as i32;
        deferred[deferred_count + 0x21] = end as i32;
        deferred[deferred_count + 0x41] = candidate.width_bits as i32;
        deferred[deferred_count + 0x61] = if level < 1 { config.mode } else { 9 };
        deferred[0] = deferred_count as i32 + 1;
    }

    Ok(())
}

fn gain_level_from_ratio(
    ratio: f64,
    point_count: usize,
    first_rounding_flag: i32,
    points: &GainPassPoints,
    fractional: bool,
    residual: &mut f32,
) -> i32 {
    if fractional {
        let log2_value = (ratio.ln() * f64::from(LOG2_E_AT5)) as f32;
        let bias = if point_count == 0 || log2_value < 2.0 {
            0.5
        } else {
            points.fractions[point_count - 1]
        };
        let level = trunc_to_i32(f64::from(log2_value) + f64::from(bias));
        *residual = log2_value - level as f32;
        level
    } else if first_rounding_flag != 0 {
        trunc_to_i32(ratio.ln() * f64::from(LOG2_E_AT5))
    } else {
        trunc_to_i32(ratio.ln() * f64::from(LOG2_E_AT5) + 0.5)
    }
}

fn release_location(index: isize, step: isize, energy: &[f32], compare: &[f32]) -> usize {
    let lower = index - step;
    let mut location = index as usize;

    if lower < index - 1 {
        let current_energy = energy[index as usize];
        let mut cursor = index - 1;
        if compare[cursor as usize] <= current_energy {
            loop {
                let next = cursor - 1;
                location = cursor as usize;
                if next <= lower {
                    break;
                }
                cursor = next;
                if compare[cursor as usize] > current_energy {
                    break;
                }
            }
        }
    }

    location
}

fn trunc_to_i32(value: f64) -> i32 {
    value.trunc() as i32
}

fn check_gc_level_from_ratio(ratio: f32) -> i32 {
    trunc_to_i32(f64::from(ratio).ln() * f64::from(LOG2_E_AT5))
}

fn check_gc_scale_from_shift(value: i32) -> f32 {
    let amount = if value < 0 {
        value.wrapping_neg()
    } else {
        value
    } & 0x1f;
    let scale = 1i32.wrapping_shl(amount as u32) as f32;
    if value < 0 { 1.0 / scale } else { scale }
}

fn check_gc_slot(index: isize) -> usize {
    (index + CHECK_GC_BASE_OFFSET as isize) as usize
}

fn fill_gain_window_release_levels(
    release_words: &[i32],
    levels: &mut [i32; GAIN_WINDOW_LEVEL_SLOTS],
) -> Result<(), GainPassError> {
    let count = gain_window_point_count("release_count", release_words[0])?;
    let mut cursor = 0usize;
    for point in 0..count {
        let level = gain_window_level_id(release_words[8 + point])?;
        let end = gain_window_index("release_location", release_words[1 + point], 31)? + 0x20;
        while cursor <= end {
            levels[cursor] = level;
            cursor += 1;
        }
    }
    Ok(())
}

fn add_gain_window_attack_levels(
    attack_words: &[i32],
    levels: &mut [i32; GAIN_WINDOW_LEVEL_SLOTS],
) -> Result<(), GainPassError> {
    let count = gain_window_point_count("attack_count", attack_words[0])?;
    let mut cursor = 0usize;
    for point in 0..count {
        let level = gain_window_level_id(attack_words[8 + point])?;
        let end = gain_window_index("attack_location", attack_words[1 + point], 63)?;
        while cursor <= end {
            levels[cursor] += level;
            cursor += 1;
        }
    }
    Ok(())
}

fn gain_window_point_count(name: &'static str, count: i32) -> Result<usize, GainPassError> {
    if count < 0 {
        return Err(GainPassError::CountOutOfRange {
            name,
            value: count as usize,
            max: GAIN_WINDOW_POINTS,
        });
    }
    let count = count as usize;
    check_count(name, count, GAIN_WINDOW_POINTS)?;
    Ok(count)
}

fn gain_window_level_id(level_id: i32) -> Result<i32, GainPassError> {
    if level_id < 0 {
        return Err(GainPassError::IndexOutOfRange {
            name: "gain_window_level_id",
            value: level_id as usize,
            max: GAIN_WINDOW_LNGAIN_AT5.len() - 1,
        });
    }
    let level_id = level_id as usize;
    if level_id >= GAIN_WINDOW_LNGAIN_AT5.len() {
        Err(GainPassError::IndexOutOfRange {
            name: "gain_window_level_id",
            value: level_id,
            max: GAIN_WINDOW_LNGAIN_AT5.len() - 1,
        })
    } else {
        Ok(GAIN_WINDOW_LNGAIN_AT5[level_id])
    }
}

fn gain_window_index(name: &'static str, value: i32, max: usize) -> Result<usize, GainPassError> {
    if value < 0 {
        return Err(GainPassError::IndexOutOfRange {
            name,
            value: value as usize,
            max,
        });
    }
    let value = value as usize;
    if value > max {
        Err(GainPassError::IndexOutOfRange { name, value, max })
    } else {
        Ok(value)
    }
}

fn gain_window_itp_row(index: usize) -> [f32; 3] {
    let row = GAIN_WINDOW_ITP_AT5[index];
    [
        f32::from_bits(row[0]),
        f32::from_bits(row[1]),
        f32::from_bits(row[2]),
    ]
}

fn gain_window_scale(level: i32) -> f32 {
    let amount = if level < 0 {
        level.wrapping_neg()
    } else {
        level
    } & 0x1f;
    let scale = 1i32.wrapping_shl(amount as u32) as f32;
    if level < 0 { 1.0 / scale } else { scale }
}

fn gc_set_points_index(
    name: &'static str,
    value: i32,
    actual: usize,
) -> Result<usize, GainPassError> {
    if value < 0 {
        Err(GainPassError::IndexOutOfRange {
            name,
            value: value as usize,
            max: actual.saturating_sub(1),
        })
    } else {
        let value = value as usize;
        if value >= actual {
            Err(GainPassError::IndexOutOfRange {
                name,
                value,
                max: actual.saturating_sub(1),
            })
        } else {
            Ok(value)
        }
    }
}

fn gc_set_points_record_offset(counts: &[i32], group: usize) -> Result<usize, GainPassError> {
    let count = counts[group];
    if count < 0 {
        return Err(GainPassError::CountOutOfRange {
            name: "gc_set_points_record_count",
            value: count as usize,
            max: GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS / GC_SET_POINTS_OUTPUT_RECORD_WORDS - 1,
        });
    }

    let count = count as usize;
    let records_per_group =
        GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS / GC_SET_POINTS_OUTPUT_RECORD_WORDS;
    check_count("gc_set_points_record_count", count, records_per_group - 1)?;

    Ok(group * GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS + count * GC_SET_POINTS_OUTPUT_RECORD_WORDS)
}

fn gc_set_points_mod32(value: i32) -> i32 {
    let mask_source = if value < 0 { value + 31 } else { value };
    value - (mask_source & !0x1f)
}

fn gc_set_points_span_log_bias(span_minus_one: i32) -> f32 {
    let span = span_minus_one as u32;
    if span >= 32 {
        GC_LOG_BIAS_32_AT5
    } else if span >= 16 {
        GC_LOG_BIAS_16_AT5
    } else if span >= 8 {
        GC_LOG_BIAS_8_AT5
    } else {
        0.0
    }
}

fn gc_set_points_min_level(span_minus_one: i32) -> i32 {
    let span = span_minus_one as u32;
    if span >= 12 {
        1
    } else if span >= 8 {
        2
    } else if span >= 6 {
        3
    } else {
        4
    }
}

fn check_gc_candidate(candidate: CheckGcCandidate) -> Result<(), GainPassError> {
    if candidate.start > GAIN_PASS_BINS {
        return Err(GainPassError::IndexOutOfRange {
            name: "check_gc_start",
            value: candidate.start,
            max: GAIN_PASS_BINS,
        });
    }
    // `set_gainc_at5` (native 0x36020) scans segments whose sentinel end is
    // 0x40 (decompile 30502: `local_f6c[1] = 0x40`), so a fully-quiet tail run
    // legally reaches `end` up to 0x40; the native callee's `end >= 0x21`
    // branch (0x45050, decompile 28741-28743) only reads `power[end]`, which
    // stays inside that caller's 0x42-float envelope. Callers passing such
    // candidates must supply a `power` slice covering `end + 1`.
    if candidate.end > CHECK_GC_MAX_END {
        return Err(GainPassError::IndexOutOfRange {
            name: "check_gc_end",
            value: candidate.end,
            max: CHECK_GC_MAX_END,
        });
    }
    if candidate.end < candidate.start {
        return Err(GainPassError::InvalidRange {
            start: candidate.start,
            end: candidate.end,
        });
    }
    Ok(())
}

fn check_storage(name: &'static str, actual: usize, needed: usize) -> Result<(), GainPassError> {
    if actual < needed {
        Err(GainPassError::StorageTooShort {
            name,
            needed,
            actual,
        })
    } else {
        Ok(())
    }
}

fn check_count(name: &'static str, value: usize, max: usize) -> Result<(), GainPassError> {
    if value > max {
        Err(GainPassError::CountOutOfRange { name, value, max })
    } else {
        Ok(())
    }
}

fn check_step(step: usize) -> Result<(), GainPassError> {
    if step == 0 {
        Err(GainPassError::StepZero)
    } else if step > GAIN_PASS_BINS - 1 {
        Err(GainPassError::IndexOutOfRange {
            name: "step",
            value: step,
            max: GAIN_PASS_BINS - 1,
        })
    } else {
        Ok(())
    }
}

fn attack_energy_needed(high_index: usize, step: usize) -> usize {
    if high_index == 0 {
        0
    } else {
        let last_loop_index = ((high_index - 1) / step) * step;
        last_loop_index + step + 1
    }
}

fn check_index(name: &'static str, value: usize) -> Result<(), GainPassError> {
    if value > GAIN_PASS_BINS - 1 {
        Err(GainPassError::IndexOutOfRange {
            name,
            value,
            max: GAIN_PASS_BINS - 1,
        })
    } else {
        Ok(())
    }
}

fn check_energy(energy: &[f32], needed: usize) -> Result<(), GainPassError> {
    if energy.len() < needed {
        Err(GainPassError::EnergyTooShort {
            needed,
            actual: energy.len(),
        })
    } else {
        Ok(())
    }
}

fn check_compare(compare: &[f32], needed: usize) -> Result<(), GainPassError> {
    if compare.len() < needed {
        Err(GainPassError::CompareTooShort {
            needed,
            actual: compare.len(),
        })
    } else {
        Ok(())
    }
}

fn check_point_count(point_count: usize) -> Result<(), GainPassError> {
    if point_count >= GAIN_PASS_POINTS {
        Err(GainPassError::PointCountOutOfRange {
            point_count,
            max: GAIN_PASS_POINTS - 1,
        })
    } else {
        Ok(())
    }
}
