use crate::tables::at5::{
    ALLOCATION_MCFX_AT5_ENTRIES, ALLOCATION_MM_AT5_ENTRIES, ALLOCATION_TC_AT5_ENTRIES,
    ALLOCATION_WCFX_AT5_ENTRIES, X_AT5_ENTRIES, Y_AT5_ENTRIES, mcfx_hbr_at5, mcfx_lbr_at5,
    mm_032_at5, mm_064_at5, mm_096_at5, mm_256_at5, pcfx_at5, tc_032_at5, tc_064_at5, tc_096_at5,
    tc_256_at5, wcfx_br_s064_m032_at5, wcfx_br_s096_m064_at5, wcfx_br_s128_m096_at5,
    wcfx_br_s256_m128_at5, x_at5, y_at5,
};

pub const ALLOCATION_WORD_LENGTHS_AT5: usize = 32;
pub const ALLOCATION_GROUPS_AT5: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationError {
    UnsupportedChannelCount(usize),
    BandCountTooLarge {
        count: usize,
        max: usize,
    },
    WordLengthRowTooShort {
        row: usize,
        needed: usize,
        actual: usize,
    },
    WordLengthsTooShort {
        needed: usize,
        actual: usize,
    },
    IdsfBitsTooShort {
        needed: usize,
        actual: usize,
    },
    GhaFlagsTooShort {
        needed: usize,
        actual: usize,
    },
    ChannelRowsTooShort {
        needed: usize,
        actual: usize,
    },
    WeightsTooShort {
        needed: usize,
        actual: usize,
    },
    AuxWeightsTooShort {
        needed: usize,
        actual: usize,
    },
    NspecsTooShort {
        needed: usize,
        actual: usize,
    },
    SideFlagsTooShort {
        needed: usize,
        actual: usize,
    },
    MaxWordLengthsTooShort {
        needed: usize,
        actual: usize,
    },
    ActivityTooShort {
        needed: usize,
        actual: usize,
    },
    PrimaryToneActivityTooShort {
        needed: usize,
        actual: usize,
    },
    SecondaryToneActivityTooShort {
        needed: usize,
        actual: usize,
    },
    ToneGroupCountTooLarge {
        count: usize,
        max: usize,
    },
    OutputTooShort {
        needed: usize,
        actual: usize,
    },
    GainLocationOutOfRange {
        value: i32,
    },
    GainLocationDeltaOutOfRange {
        value: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZerothWcfxTableKind {
    S064M032,
    S096M064,
    S128M096,
    S256M128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZerothWcfxSelection {
    pub kind: ZerothWcfxTableKind,
    pub values: [f32; ALLOCATION_WCFX_AT5_ENTRIES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZerothToneTableKind {
    S032,
    S064,
    S096,
    S256,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZerothToneSelection {
    pub kind: ZerothToneTableKind,
    pub tone_thresholds: [f32; ALLOCATION_TC_AT5_ENTRIES],
    pub max_margins: [i32; ALLOCATION_MM_AT5_ENTRIES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothBandShape {
    pub word_length_count: usize,
    pub group_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothActiveBandCounts {
    pub active_band_count: usize,
    pub group_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothSideDataBitSeed {
    pub mode_bits_118: i16,
    pub idwl_bits_11a: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothIdsfBitCount {
    pub idsf_bits_11c: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothGhaChannelFlags {
    pub has_nonzero_band: bool,
    pub trimmed_differs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothGhaBitSeed {
    pub gha_bits: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZerothInactiveZeroingMode {
    FullBandActivity,
    ToneGroupSpans,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondMcfxTableKind {
    LowBitrate,
    HighBitrate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecondMcfxSelection {
    pub kind: SecondMcfxTableKind,
    pub values: [f32; ALLOCATION_MCFX_AT5_ENTRIES],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SecondCandidateSearchState {
    pub step: f32,
    pub step_scale: f32,
    pub last_direction: i32,
    pub iteration: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondCandidateSearchAction {
    Stop,
    Continue,
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SecondCandidateSearchUpdate {
    pub action: SecondCandidateSearchAction,
    pub state: SecondCandidateSearchState,
}

pub fn select_zeroth_wcfx_at5(
    channel_count: usize,
    selector: u32,
) -> Result<ZerothWcfxSelection, AllocationError> {
    validate_channel_count(channel_count)?;

    let kind = if channel_count == 2 {
        if selector < 0x13 {
            ZerothWcfxTableKind::S064M032
        } else if selector < 0x17 {
            ZerothWcfxTableKind::S096M064
        } else if selector < 0x19 {
            ZerothWcfxTableKind::S128M096
        } else {
            ZerothWcfxTableKind::S256M128
        }
    } else if selector < 0x0d {
        ZerothWcfxTableKind::S064M032
    } else if selector < 0x13 {
        ZerothWcfxTableKind::S096M064
    } else if selector < 0x17 {
        ZerothWcfxTableKind::S128M096
    } else {
        ZerothWcfxTableKind::S256M128
    };

    let values = match kind {
        ZerothWcfxTableKind::S064M032 => wcfx_br_s064_m032_at5(),
        ZerothWcfxTableKind::S096M064 => wcfx_br_s096_m064_at5(),
        ZerothWcfxTableKind::S128M096 => wcfx_br_s128_m096_at5(),
        ZerothWcfxTableKind::S256M128 => wcfx_br_s256_m128_at5(),
    };
    Ok(ZerothWcfxSelection { kind, values })
}

pub fn select_zeroth_tone_tables_at5(selector: u32) -> ZerothToneSelection {
    let kind = if selector < 0x0d {
        ZerothToneTableKind::S032
    } else if selector < 0x13 {
        ZerothToneTableKind::S064
    } else if selector < 0x1b {
        ZerothToneTableKind::S096
    } else {
        ZerothToneTableKind::S256
    };

    let (tone_thresholds, max_margins) = match kind {
        ZerothToneTableKind::S032 => (tc_032_at5(), mm_032_at5()),
        ZerothToneTableKind::S064 => (tc_064_at5(), mm_064_at5()),
        ZerothToneTableKind::S096 => (tc_096_at5(), mm_096_at5()),
        ZerothToneTableKind::S256 => (tc_256_at5(), mm_256_at5()),
    };
    ZerothToneSelection {
        kind,
        tone_thresholds,
        max_margins,
    }
}

pub fn select_zeroth_inactive_zeroing_mode_at5(
    object_mode_1c: u32,
    channel_count: usize,
    selector: u32,
) -> Result<ZerothInactiveZeroingMode, AllocationError> {
    validate_channel_count(channel_count)?;

    if object_mode_1c == 2 {
        return Ok(ZerothInactiveZeroingMode::FullBandActivity);
    }

    if (channel_count == 1 && selector > 0x16) || (channel_count == 2 && selector > 0x1a) {
        Ok(ZerothInactiveZeroingMode::ToneGroupSpans)
    } else {
        Ok(ZerothInactiveZeroingMode::FullBandActivity)
    }
}

pub fn select_second_mcfx_at5(
    channel_count: usize,
    selector: u32,
) -> Result<SecondMcfxSelection, AllocationError> {
    validate_channel_count(channel_count)?;

    let kind = if (channel_count == 1 && selector > 0x16) || (channel_count == 2 && selector > 0x1a)
    {
        SecondMcfxTableKind::HighBitrate
    } else {
        SecondMcfxTableKind::LowBitrate
    };
    let values = match kind {
        SecondMcfxTableKind::LowBitrate => mcfx_lbr_at5(),
        SecondMcfxTableKind::HighBitrate => mcfx_hbr_at5(),
    };
    Ok(SecondMcfxSelection { kind, values })
}

pub fn second_positive_step_pcfx_at5() -> [f32; ALLOCATION_MCFX_AT5_ENTRIES] {
    pcfx_at5()
}

pub fn initial_second_candidate_search_state_at5(
    current_bits: i32,
    target_bits: i32,
) -> SecondCandidateSearchState {
    if current_bits < target_bits {
        SecondCandidateSearchState {
            step: 1.0,
            step_scale: 1.0,
            last_direction: 1,
            iteration: 0,
        }
    } else {
        SecondCandidateSearchState {
            step: -1.0,
            step_scale: 1.0,
            last_direction: -1,
            iteration: 0,
        }
    }
}

pub fn advance_second_candidate_search_at5(
    state: SecondCandidateSearchState,
    current_bits: i32,
    target_bits: i32,
    flags_nonzero: bool,
) -> SecondCandidateSearchUpdate {
    let window_factor = if flags_nonzero { 0.5 } else { 0.95 };
    let lower_window = (f64::from(target_bits) * f64::from(window_factor)).trunc() as i32;
    let in_native_window = current_bits <= target_bits && lower_window < current_bits;
    let positive_step_limit = state.step >= 5.0 && current_bits <= target_bits;
    let negative_step_limit = state.step <= -6.0;
    if in_native_window || positive_step_limit || negative_step_limit {
        return SecondCandidateSearchUpdate {
            action: SecondCandidateSearchAction::Stop,
            state,
        };
    }

    let direction = if current_bits < target_bits { 1 } else { -1 };
    let mut next_state = state;
    if state.iteration < 7 {
        if direction != state.last_direction {
            next_state.step_scale *= 0.5;
        }
        let signed_scale = if direction == 1 {
            next_state.step_scale
        } else {
            -next_state.step_scale
        };
        next_state.step += signed_scale;
    }
    next_state.last_direction = direction;
    next_state.iteration = state.iteration + 1;

    let action = if next_state.iteration < 8 {
        SecondCandidateSearchAction::Continue
    } else {
        SecondCandidateSearchAction::Exhausted
    };
    SecondCandidateSearchUpdate {
        action,
        state: next_state,
    }
}

pub fn compute_zeroth_base_weights_at5(
    activity: &[i32],
    scale: f32,
    coefficients: &[f32],
    output: &mut [f32],
    band_count: usize,
) -> Result<(), AllocationError> {
    if activity.len() < band_count {
        return Err(AllocationError::ActivityTooShort {
            needed: band_count,
            actual: activity.len(),
        });
    }
    if coefficients.len() < band_count {
        return Err(AllocationError::WeightsTooShort {
            needed: band_count,
            actual: coefficients.len(),
        });
    }
    if output.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: output.len(),
        });
    }

    for index in 0..band_count {
        output[index] =
            (f64::from(activity[index]) * f64::from(scale) * f64::from(coefficients[index])) as f32;
    }

    Ok(())
}

pub fn apply_zeroth_aux_weight_bonus_at5(
    aux_weights: &[f32],
    output: &mut [f32],
    band_count: usize,
) -> Result<(), AllocationError> {
    if aux_weights.len() < band_count {
        return Err(AllocationError::AuxWeightsTooShort {
            needed: band_count,
            actual: aux_weights.len(),
        });
    }
    if output.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: output.len(),
        });
    }

    for index in 0..band_count {
        let bonus = if aux_weights[index] >= 10.0 {
            2.0
        } else if aux_weights[index] >= 6.0 {
            1.0
        } else if aux_weights[index] >= 3.5 {
            0.5
        } else {
            0.0
        };
        output[index] += bonus;
    }

    Ok(())
}

pub fn compute_second_candidate_weights_at5(
    current_weights: &[f32],
    step: f32,
    coefficients: &[f32],
    output: &mut [f32],
    band_count: usize,
) -> Result<(), AllocationError> {
    if current_weights.len() < band_count {
        return Err(AllocationError::WeightsTooShort {
            needed: band_count,
            actual: current_weights.len(),
        });
    }
    if coefficients.len() < band_count {
        return Err(AllocationError::WeightsTooShort {
            needed: band_count,
            actual: coefficients.len(),
        });
    }
    if output.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: output.len(),
        });
    }

    for index in 0..band_count {
        output[index] = (f64::from(coefficients[index]) * f64::from(step)
            + f64::from(current_weights[index])) as f32;
    }

    Ok(())
}

pub fn mark_second_candidate_changes_at5(
    candidate_word_lengths: &[i32],
    previous_word_lengths: &[i32],
    nspecs: &[i32],
    changed_flags: &mut [i32],
    band_count: usize,
    first_iteration: bool,
) -> Result<(), AllocationError> {
    if candidate_word_lengths.len() < band_count {
        return Err(AllocationError::WordLengthsTooShort {
            needed: band_count,
            actual: candidate_word_lengths.len(),
        });
    }
    if previous_word_lengths.len() < band_count {
        return Err(AllocationError::WordLengthsTooShort {
            needed: band_count,
            actual: previous_word_lengths.len(),
        });
    }
    if first_iteration && nspecs.len() < band_count {
        return Err(AllocationError::NspecsTooShort {
            needed: band_count,
            actual: nspecs.len(),
        });
    }
    if changed_flags.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: changed_flags.len(),
        });
    }

    for index in 0..band_count {
        let changed = candidate_word_lengths[index] != previous_word_lengths[index];
        let live_first_iteration_band =
            first_iteration && candidate_word_lengths[index] > 0 && nspecs[index] > 0;
        changed_flags[index] = i32::from(changed || live_first_iteration_band);
    }

    Ok(())
}

pub fn round_and_clamp_word_lengths_at5(
    weights: &[f32],
    max_word_lengths: &[i16],
    output: &mut [i32],
    band_count: usize,
) -> Result<(), AllocationError> {
    if weights.len() < band_count {
        return Err(AllocationError::WeightsTooShort {
            needed: band_count,
            actual: weights.len(),
        });
    }
    if max_word_lengths.len() < band_count {
        return Err(AllocationError::MaxWordLengthsTooShort {
            needed: band_count,
            actual: max_word_lengths.len(),
        });
    }
    if output.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: output.len(),
        });
    }

    for index in 0..band_count {
        let rounded = (f64::from(weights[index]) + 0.5).trunc() as i32;
        let max_word_length = i32::from(max_word_lengths[index]);
        output[index] = if rounded > max_word_length {
            max_word_length
        } else if rounded <= 0 {
            1
        } else {
            rounded
        };
    }

    Ok(())
}

pub fn zero_inactive_word_lengths_at5(
    activity: &[i32],
    output: &mut [i32],
    band_count: usize,
) -> Result<(), AllocationError> {
    if activity.len() < band_count {
        return Err(AllocationError::ActivityTooShort {
            needed: band_count,
            actual: activity.len(),
        });
    }
    if output.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: output.len(),
        });
    }

    for index in 0..band_count {
        if activity[index] == 0 {
            output[index] = 0;
        }
    }

    Ok(())
}

pub fn zero_tone_span_inactive_word_lengths_at5(
    primary_tone_activity: &[f32],
    secondary_tone_activity: Option<&[f32]>,
    activity: &[i32],
    output: &mut [i32],
    group_count: usize,
) -> Result<(), AllocationError> {
    if group_count >= Y_AT5_ENTRIES {
        return Err(AllocationError::ToneGroupCountTooLarge {
            count: group_count,
            max: Y_AT5_ENTRIES - 1,
        });
    }
    if primary_tone_activity.len() < group_count {
        return Err(AllocationError::PrimaryToneActivityTooShort {
            needed: group_count,
            actual: primary_tone_activity.len(),
        });
    }
    if let Some(secondary_tone_activity) = secondary_tone_activity {
        if secondary_tone_activity.len() < group_count {
            return Err(AllocationError::SecondaryToneActivityTooShort {
                needed: group_count,
                actual: secondary_tone_activity.len(),
            });
        }
    }

    let spans = y_at5();
    let needed_bands = usize::from(spans[group_count]);
    if activity.len() < needed_bands {
        return Err(AllocationError::ActivityTooShort {
            needed: needed_bands,
            actual: activity.len(),
        });
    }
    if output.len() < needed_bands {
        return Err(AllocationError::OutputTooShort {
            needed: needed_bands,
            actual: output.len(),
        });
    }

    for group in 0..group_count {
        let primary_is_zero = primary_tone_activity[group] == 0.0;
        let secondary_is_zero = secondary_tone_activity
            .map(|secondary| secondary[group] == 0.0)
            .unwrap_or(false);
        if primary_is_zero || secondary_is_zero {
            let start = usize::from(spans[group]);
            let end = usize::from(spans[group + 1]);
            zero_inactive_word_lengths_at5(
                &activity[start..end],
                &mut output[start..end],
                end - start,
            )?;
        }
    }

    Ok(())
}

pub fn copy_word_lengths_to_activity_at5(
    word_lengths: &[i32],
    activity: &mut [i32],
) -> Result<(), AllocationError> {
    if word_lengths.len() < ALLOCATION_WORD_LENGTHS_AT5 {
        return Err(AllocationError::WordLengthsTooShort {
            needed: ALLOCATION_WORD_LENGTHS_AT5,
            actual: word_lengths.len(),
        });
    }
    if activity.len() < ALLOCATION_WORD_LENGTHS_AT5 {
        return Err(AllocationError::OutputTooShort {
            needed: ALLOCATION_WORD_LENGTHS_AT5,
            actual: activity.len(),
        });
    }

    activity[..ALLOCATION_WORD_LENGTHS_AT5]
        .copy_from_slice(&word_lengths[..ALLOCATION_WORD_LENGTHS_AT5]);
    Ok(())
}

fn zeroth_band_shape_decision_at5(
    quant_unit_count: usize,
    fallback_word_length_count: usize,
    fallback_group_count: usize,
) -> (bool, ZerothBandShape) {
    // Native uses an unsigned `(count - 29) < 3` comparison, so only
    // 29..=31 select the rounded 32/16 shape. Every other valid count copies
    // the existing shared cfg+0xb4/cfg+0xbc pair.
    let rounds_up = quant_unit_count.wrapping_sub(29) < 3;
    let shape = if rounds_up {
        ZerothBandShape {
            word_length_count: ALLOCATION_WORD_LENGTHS_AT5,
            group_count: ALLOCATION_GROUPS_AT5,
        }
    } else {
        ZerothBandShape {
            word_length_count: fallback_word_length_count,
            group_count: fallback_group_count,
        }
    };
    (rounds_up, shape)
}

/// Compute the shared zeroth band-shape counts without mutating word lengths.
///
/// Native `zeroth_bit_allocation_at5` (native `0x42360`; decompile
/// 36640-36660; disassembly `0x43134..0x43140`, `0x4386b..0x43883`) rounds
/// quant-unit counts 29..=31 to 32/16 and otherwise copies the supplied
/// shared `cfg+0xb4`/`cfg+0xbc` counts. Callers own surface validation; the
/// allocation finalizer below rejects counts above 32 before using this law.
pub fn zeroth_band_shape_counts_at5(
    quant_unit_count: usize,
    fallback_word_length_count: usize,
    fallback_group_count: usize,
) -> ZerothBandShape {
    zeroth_band_shape_decision_at5(
        quant_unit_count,
        fallback_word_length_count,
        fallback_group_count,
    )
    .1
}

pub fn finalize_zeroth_band_shape_at5(
    word_length_rows: &mut [&mut [i32]],
    band_count: usize,
    fallback_word_length_count: usize,
    fallback_group_count: usize,
) -> Result<ZerothBandShape, AllocationError> {
    if band_count > ALLOCATION_WORD_LENGTHS_AT5 {
        return Err(AllocationError::BandCountTooLarge {
            count: band_count,
            max: ALLOCATION_WORD_LENGTHS_AT5,
        });
    }

    let (rounds_up, band_shape) = zeroth_band_shape_decision_at5(
        band_count,
        fallback_word_length_count,
        fallback_group_count,
    );
    if rounds_up {
        for (row_index, row) in word_length_rows.iter_mut().enumerate() {
            if row.len() < ALLOCATION_WORD_LENGTHS_AT5 {
                return Err(AllocationError::WordLengthRowTooShort {
                    row: row_index,
                    needed: ALLOCATION_WORD_LENGTHS_AT5,
                    actual: row.len(),
                });
            }
            for value in &mut row[band_count..ALLOCATION_WORD_LENGTHS_AT5] {
                *value = 0;
            }
        }
    }

    Ok(band_shape)
}

pub fn compute_zeroth_active_band_counts_at5(
    word_length_rows: &[&[i32]],
    channel_count: usize,
    word_length_count: usize,
) -> Result<ZerothActiveBandCounts, AllocationError> {
    validate_channel_count(channel_count)?;
    let max_word_length_count = ALLOCATION_WORD_LENGTHS_AT5.min(X_AT5_ENTRIES - 1);
    if word_length_count > max_word_length_count {
        return Err(AllocationError::BandCountTooLarge {
            count: word_length_count,
            max: max_word_length_count,
        });
    }
    if word_length_rows.len() < channel_count {
        return Err(AllocationError::ChannelRowsTooShort {
            needed: channel_count,
            actual: word_length_rows.len(),
        });
    }
    for (row_index, row) in word_length_rows.iter().take(channel_count).enumerate() {
        if row.len() < word_length_count {
            return Err(AllocationError::WordLengthRowTooShort {
                row: row_index,
                needed: word_length_count,
                actual: row.len(),
            });
        }
    }

    let mut active_band_count = word_length_count;
    while active_band_count > 0 {
        let index = active_band_count - 1;
        let should_trim = if channel_count == 2 {
            word_length_rows[0][index] == 0 && word_length_rows[1][index] == 0
        } else {
            word_length_rows[0][index] == 0
        };
        if !should_trim {
            break;
        }
        active_band_count -= 1;
    }

    let grouped_counts = x_at5();
    Ok(ZerothActiveBandCounts {
        active_band_count,
        group_count: usize::from(grouped_counts[active_band_count]) + 1,
    })
}

pub fn compute_zeroth_side_data_bit_seed_at5(
    channel_count: usize,
    quant_unit_count: u32,
) -> Result<ZerothSideDataBitSeed, AllocationError> {
    validate_channel_count(channel_count)?;

    let idwl_bits = quant_unit_count
        .wrapping_mul(3)
        .wrapping_add(2)
        .wrapping_mul(channel_count as u32);

    Ok(ZerothSideDataBitSeed {
        mode_bits_118: 6,
        idwl_bits_11a: idwl_bits as u16 as i16,
    })
}

pub fn compute_zeroth_enabled_idsf_bit_count_at5(
    channel_count: usize,
    active_band_count: usize,
    idsf_bits: &[i32],
) -> Result<ZerothIdsfBitCount, AllocationError> {
    validate_channel_count(channel_count)?;
    if active_band_count > ALLOCATION_WORD_LENGTHS_AT5 {
        return Err(AllocationError::BandCountTooLarge {
            count: active_band_count,
            max: ALLOCATION_WORD_LENGTHS_AT5,
        });
    }

    let mut bit_count = (channel_count as i32).wrapping_mul(2);
    if active_band_count > 0 {
        if idsf_bits.len() < channel_count {
            return Err(AllocationError::IdsfBitsTooShort {
                needed: channel_count,
                actual: idsf_bits.len(),
            });
        }
        for bits in &idsf_bits[..channel_count] {
            bit_count = bit_count.wrapping_add(*bits);
        }
    }

    Ok(ZerothIdsfBitCount {
        idsf_bits_11c: bit_count as u16 as i16,
    })
}

pub fn compute_zeroth_gha_bit_seed_at5(
    channel_count: usize,
    channel_flags: &[ZerothGhaChannelFlags],
) -> Result<ZerothGhaBitSeed, AllocationError> {
    validate_channel_count(channel_count)?;
    if channel_flags.len() < channel_count {
        return Err(AllocationError::GhaFlagsTooShort {
            needed: channel_count,
            actual: channel_flags.len(),
        });
    }

    let mut bit_count = channel_count as i32;
    for flags in &channel_flags[..channel_count] {
        if flags.has_nonzero_band {
            bit_count += if flags.trimmed_differs { 15 } else { 11 };
        }
    }

    Ok(ZerothGhaBitSeed {
        gha_bits: bit_count as u16 as i16,
    })
}

pub fn apply_zeroth_stereo_cross_zero_at5(
    left_word_lengths: &[i32],
    right_word_lengths: &mut [i32],
    primary_flags: &mut [i16],
    secondary_flags: &mut [i16],
    band_count: usize,
) -> Result<(), AllocationError> {
    if left_word_lengths.len() < band_count {
        return Err(AllocationError::WordLengthsTooShort {
            needed: band_count,
            actual: left_word_lengths.len(),
        });
    }
    if right_word_lengths.len() < band_count {
        return Err(AllocationError::WordLengthsTooShort {
            needed: band_count,
            actual: right_word_lengths.len(),
        });
    }
    if primary_flags.len() < band_count {
        return Err(AllocationError::SideFlagsTooShort {
            needed: band_count,
            actual: primary_flags.len(),
        });
    }
    if secondary_flags.len() < band_count {
        return Err(AllocationError::SideFlagsTooShort {
            needed: band_count,
            actual: secondary_flags.len(),
        });
    }

    for index in 0..band_count {
        if right_word_lengths[index] == 0 {
            if left_word_lengths[index] != 0 {
                secondary_flags[index] = 1;
            }
            primary_flags[index] = 0;
        } else if left_word_lengths[index] == 0 {
            primary_flags[index] = 0;
        }

        if primary_flags[index] == 1 {
            right_word_lengths[index] = 0;
        }
    }

    Ok(())
}

fn validate_channel_count(channel_count: usize) -> Result<(), AllocationError> {
    match channel_count {
        1 | 2 => Ok(()),
        _ => Err(AllocationError::UnsupportedChannelCount(channel_count)),
    }
}

/// Native `zeroth_bit_allocation_at5` entry prologue flag (decompile
/// `LAB_00052437`, native `0x52437..0x52491`): per channel, the flag
/// starts at 1 and clears to 0 if any of the first
/// `min(band_count, 8)` bands has a zero point count (word 0) in its
/// current 0x98-byte gain record (`*(channel + 8) + band * 0x98`).
/// The flag gates the flags-zero weight and zeroing helpers downstream.
pub fn compute_zeroth_gain_record_flags_at5(
    record_point_counts: &[&[u32]],
    band_count: usize,
    channel_count: usize,
) -> Result<Vec<u32>, AllocationError> {
    if !(1..=2).contains(&channel_count) {
        return Err(AllocationError::UnsupportedChannelCount(channel_count));
    }
    if record_point_counts.len() < channel_count {
        return Err(AllocationError::ChannelRowsTooShort {
            needed: channel_count,
            actual: record_point_counts.len(),
        });
    }
    let scanned = band_count.min(8);
    let mut flags = Vec::with_capacity(channel_count);
    for counts in record_point_counts.iter().take(channel_count) {
        if counts.len() < scanned {
            return Err(AllocationError::ChannelRowsTooShort {
                needed: scanned,
                actual: counts.len(),
            });
        }
        let all_nonzero = counts[..scanned].iter().all(|count| *count != 0);
        flags.push(u32::from(all_nonzero));
    }
    Ok(flags)
}

/// Native flagged-path base weights (decompile line 36493, the
/// `header flag & 0x7c != 0` branch of `zeroth_bit_allocation_at5`):
/// `weight[i] = (idsf_activity[i] * scale) / 10.0` with no coefficient
/// table.
pub fn compute_zeroth_flagged_base_weights_at5(
    activity: &[i32],
    scale: f32,
    output: &mut [f32],
    band_count: usize,
) -> Result<(), AllocationError> {
    if activity.len() < band_count {
        return Err(AllocationError::ActivityTooShort {
            needed: band_count,
            actual: activity.len(),
        });
    }
    if output.len() < band_count {
        return Err(AllocationError::OutputTooShort {
            needed: band_count,
            actual: output.len(),
        });
    }
    for index in 0..band_count {
        output[index] = ((f64::from(activity[index]) * f64::from(scale)) / 10.0) as f32;
    }
    Ok(())
}

/// Native flags-zero stepped weight adjustment (decompile line 36455):
/// when the channel's gain-record flag is clear and the selector is
/// above 0xc (stereo) or 10 (mono), the tonality metric at channel
/// `+0x458` picks a constant added to the first eight weights:
/// `<= 2.9` gives -0.75, `<= 3.0` -0.5, `<= 3.1` -0.25, `<= 3.2` 0.0,
/// `<= 3.3` +0.25, else +0.5.
pub fn zeroth_flags_zero_weight_step_at5(metric: f32) -> f32 {
    if metric <= 3.3 {
        if metric <= 3.2 {
            if metric <= 3.1 {
                if metric <= 3.0 {
                    if metric <= 2.9 { -0.75 } else { -0.5 }
                } else {
                    -0.25
                }
            } else {
                0.0
            }
        } else {
            0.25
        }
    } else {
        0.5
    }
}

/// Native low-rate transient boost (decompile line 36529): when the
/// selector sits in `0xb..0x14` (stereo) or equals 9 (mono) and the
/// channel transient byte at `+0x45c` is set, the first eight weights
/// gain +0.5 plus +0.75 more where the aux weight reaches 3.0. Out of
/// the 352 kbps scope (selector 30) but ported from the decompile.
pub fn apply_zeroth_transient_boost_at5(
    aux_weights: &[f32],
    output: &mut [f32],
) -> Result<(), AllocationError> {
    if aux_weights.len() < 8 || output.len() < 8 {
        return Err(AllocationError::OutputTooShort {
            needed: 8,
            actual: aux_weights.len().min(output.len()),
        });
    }
    for index in 0..8 {
        let mut value = output[index] + 0.5;
        if 3.0 <= aux_weights[index] {
            value += 0.75;
        }
        output[index] = value;
    }
    Ok(())
}

/// One band's input to the zeroth quant-table selection.
#[derive(Debug, Clone, Copy)]
pub struct ZerothQuantBandInput<'a> {
    pub spectrum: &'a [f32],
    pub word_length: usize,
    pub idsf: usize,
    pub scale: f32,
    pub count: usize,
}

/// The zeroth pass's per-state quant-table selection outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothQuantTableSelection {
    /// Per band: the picked candidate index (`local_340`, the earliest
    /// strict minimum over the candidate costs), `None` for bands the
    /// native loop skips (`word_length < 1`).
    pub picks: Vec<Option<usize>>,
    /// Per band: the picked minimum cost (0 for skipped bands).
    pub min_costs: Vec<u16>,
    /// Per band: the full 8-candidate cost row the native
    /// `quant_nontone_nspecs_at5` writes to `block+0xb88 + band*0x10`
    /// (`None` for skipped bands — the native writes nothing there, so
    /// the plane keeps its zero-initialized state). Consumed by the
    /// bridge to serialize the `block+0xb08` quant plane cost region.
    pub cost_rows: Vec<Option<[u16; crate::coding::quant_cost::QUANT_COST_CANDIDATES]>>,
    /// The per-state total written to channel `+0x46 + state * 2`
    /// (a 16-bit sum of the minimum costs; the inactive state's slot
    /// takes the native `0x4000` sentinel instead).
    pub state_total: u16,
}

/// Native sentinel stored for candidate states the zeroth pass does not
/// evaluate (`*(channel + 0x46 + state * 2) = 0x4000`).
pub const ZEROTH_QUANT_STATE_SENTINEL: u16 = 0x4000;

/// The zeroth pass's quant-table selection loop (decompile line 36662):
/// for every band with a positive word length,
/// `quant_nontone_nspecs_at5` fills the candidate cost row and the
/// caller scans candidates `1..sa_nencodetbls[selector]` for the
/// earliest strict minimum, storing the pick in the `+0xb08` row and
/// accumulating the minimum into the per-state 16-bit total.
pub fn zeroth_quant_table_selection_at5(
    bands: &[Option<ZerothQuantBandInput<'_>>],
    state: usize,
    candidate_count: usize,
) -> Result<ZerothQuantTableSelection, crate::coding::quant::QuantError> {
    let mut picks = Vec::with_capacity(bands.len());
    let mut min_costs = Vec::with_capacity(bands.len());
    let mut cost_rows = Vec::with_capacity(bands.len());
    let mut state_total: u16 = 0;
    for band in bands {
        let Some(input) = band else {
            picks.push(None);
            min_costs.push(0);
            cost_rows.push(None);
            continue;
        };
        let costs = crate::coding::quant_cost::quant_nontone_costs_at5(
            input.spectrum,
            input.word_length,
            input.idsf,
            input.scale,
            input.count,
            state,
            candidate_count,
        )?;
        let mut best_index = 0usize;
        let mut best = costs[0] as i16;
        for (index, cost) in costs
            .iter()
            .enumerate()
            .take(candidate_count.min(costs.len()))
            .skip(1)
        {
            if (*cost as i16) < best {
                best = *cost as i16;
                best_index = index;
            }
        }
        state_total = state_total.wrapping_add(best as u16);
        picks.push(Some(best_index));
        min_costs.push(best as u16);
        cost_rows.push(Some(costs));
    }
    Ok(ZerothQuantTableSelection {
        picks,
        min_costs,
        cost_rows,
        state_total,
    })
}

/// Candidate bit counts and selection for the zeroth gain-control
/// point-count (`ngc`) side-data mode (decompile line 36800).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothGainNgcSelection {
    /// The four candidate bit counts: raw 3-bit, `ngc_pack_A` Huffman
    /// (the decompile's `g_hc_gc_ngc_A[...]` is the descriptor's
    /// relocated pointer to `g_a_ngc_pack_A`, dereference folded by
    /// Ghidra), delta (`ngc_pack_B` between bands for channel 0,
    /// against the reference channel for channel 1), and the range mode
    /// (`sa_ngc_3[max - min] * count + 5` for channel 0; 0 when every
    /// count equals the reference for channel 1, else the `0x4000`
    /// sentinel).
    pub candidates: [i32; 4],
    /// The minimum candidate's index (channel word `+0x6d25`).
    pub mode: usize,
    /// Channel-0 range mode width (`+0x6d26`) and minimum (`+0x6d27`).
    pub fixed_width: Option<i32>,
    pub fixed_min: Option<i32>,
}

/// Native zeroth `ngc` mode selection over the active bands' gain
/// record point counts. `reference` is channel 0's counts when scoring
/// channel 1 (the native reads `*(channel[10] + 8)` records at the
/// same 0x98 stride); `None` scores channel 0 itself.
pub fn zeroth_gain_ngc_mode_at5(
    point_counts: &[i32],
    reference: Option<&[i32]>,
) -> Result<ZerothGainNgcSelection, AllocationError> {
    use crate::tables::generated::{G_A_NGC_PACK_A, G_A_NGC_PACK_B, SA_NGC_3};

    let count = point_counts.len();
    if let Some(reference_counts) = reference {
        if reference_counts.len() < count {
            return Err(AllocationError::ChannelRowsTooShort {
                needed: count,
                actual: reference_counts.len(),
            });
        }
    }

    let huffman_len =
        |table: &[u8], index: i32| -> i32 { i32::from(table[(index as usize) * 4 + 2]) };

    let raw = count as i32 * 3;
    let mut huffman = 0i32;
    for &value in point_counts {
        huffman += huffman_len(&G_A_NGC_PACK_A, value);
    }

    let mut candidates = [raw, huffman, 0, 0];
    let mut fixed_width = None;
    let mut fixed_min = None;
    match reference {
        None => {
            // Channel 0 delta: first band via ngc_A, then ngc_B over
            // consecutive-band differences masked to 3 bits.
            if count > 0 {
                let mut delta = huffman_len(&G_A_NGC_PACK_A, point_counts[0]);
                for pair in point_counts.windows(2) {
                    delta += huffman_len(&G_A_NGC_PACK_B, (pair[1] - pair[0]) & 7);
                }
                candidates[2] = delta;
            }
            // Range mode: sa_ngc_3[max - min] * count + 5.
            let mut minimum = 7i32;
            let mut maximum = 0i32;
            for &value in point_counts {
                minimum = minimum.min(value);
                maximum = maximum.max(value);
            }
            let width = i32::from(SA_NGC_3[(maximum - minimum) as usize] as i8);
            candidates[3] = width * count as i32 + 5;
            fixed_width = Some(width);
            fixed_min = Some(minimum);
        }
        Some(reference_counts) => {
            // Channel 1 delta: ngc_B against the reference channel for
            // every band.
            let mut delta = 0i32;
            for (value, reference_value) in point_counts.iter().zip(reference_counts) {
                delta += huffman_len(&G_A_NGC_PACK_B, (value - reference_value) & 7);
            }
            candidates[2] = delta;
            // Copy mode: free when identical to the reference,
            // otherwise the 0x4000 sentinel.
            let identical = point_counts
                .iter()
                .zip(reference_counts)
                .all(|(value, reference_value)| value == reference_value);
            candidates[3] = if identical { 0 } else { 0x4000 };
        }
    }

    let mut mode = 0usize;
    let mut best = candidates[0];
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if *candidate < best {
            best = *candidate;
            mode = index;
        }
    }

    Ok(ZerothGainNgcSelection {
        candidates,
        mode,
        fixed_width,
        fixed_min,
    })
}

/// One band's gain-record level row for the zeroth idlev selection:
/// the point count and up to seven level values (record words 8..).
#[derive(Debug, Clone, Copy)]
pub struct ZerothGainLevelBand<'a> {
    pub count: usize,
    pub levels: &'a [i32],
}

/// The zeroth gain level (`idlev`) mode selection for channel 0
/// (decompile line 36903).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothGainLevelSelection {
    /// Raw 4-bit, per-band chain (`idlev_pack_A` first + `pack_B`
    /// in-band deltas), cross-band chain (band 0 as the per-band
    /// chain, later bands via `pack_C` against the previous band's
    /// same-index level, or against 7 beyond its count), and the range
    /// mode (`sa_idlev_3[max - min] * total_count + 6`).
    pub candidates: [i32; 4],
    pub mode: usize,
    /// Range-mode width (`+0x6d29`) and minimum (`+0x6d2a`).
    pub fixed_width: i32,
    pub fixed_min: i32,
}

/// Native channel-0 idlev candidate scoring over the active bands'
/// gain-record levels; the minimum picks the mode word at `+0x6d28`.
pub fn zeroth_gain_idlev_mode_at5(
    bands: &[ZerothGainLevelBand<'_>],
) -> Result<ZerothGainLevelSelection, AllocationError> {
    use crate::tables::generated::{
        G_A_IDLEV_PACK_A, G_A_IDLEV_PACK_B, G_A_IDLEV_PACK_C, SA_IDLEV_3,
    };

    for band in bands {
        if band.levels.len() < band.count {
            return Err(AllocationError::ChannelRowsTooShort {
                needed: band.count,
                actual: band.levels.len(),
            });
        }
    }
    let huffman_len =
        |table: &[u8], index: i32| -> i32 { i32::from(table[(index as usize & 0xf) * 4 + 2]) };

    // Candidate 0: raw four bits per level.
    let raw: i32 = bands.iter().map(|band| band.count as i32 * 4).sum();

    // Candidate 1: per band, pack_A for the first level then pack_B
    // over in-band deltas.
    let mut per_band = 0i32;
    for band in bands {
        if band.count > 0 {
            per_band += i32::from(G_A_IDLEV_PACK_A[(band.levels[0] as usize) * 4 + 2]);
            for pair in band.levels[..band.count].windows(2) {
                per_band += huffman_len(&G_A_IDLEV_PACK_B, pair[1] - pair[0]);
            }
        }
    }

    // Candidate 2: cross-band chain — band 0 like the per-band chain,
    // later bands via pack_C against the previous band's same-index
    // level (or 7 beyond the previous band's count).
    let mut cross = 0i32;
    if let Some(first) = bands.first() {
        if first.count > 0 {
            cross += i32::from(G_A_IDLEV_PACK_A[(first.levels[0] as usize) * 4 + 2]);
            for pair in first.levels[..first.count].windows(2) {
                cross += huffman_len(&G_A_IDLEV_PACK_B, pair[1] - pair[0]);
            }
        }
        for window in bands.windows(2) {
            let previous = &window[0];
            let current = &window[1];
            for index in 0..current.count {
                let reference = if index < previous.count {
                    previous.levels[index]
                } else {
                    7
                };
                cross += huffman_len(&G_A_IDLEV_PACK_C, current.levels[index] - reference);
            }
        }
    }

    // Candidate 3: range mode over every level.
    let mut minimum = 0xf_i32;
    let mut maximum = 0i32;
    for band in bands {
        for &level in &band.levels[..band.count] {
            minimum = minimum.min(level);
            maximum = maximum.max(level);
        }
    }
    // With no levels at all the native indexes sa_idlev_3 with a
    // negative max-min and reads garbage — harmless because the width
    // multiplies a zero total; the port uses 0 instead.
    let width = if maximum < minimum {
        0
    } else {
        let table_index = (maximum - minimum) as usize;
        let low = SA_IDLEV_3[table_index * 2];
        let high = SA_IDLEV_3[table_index * 2 + 1];
        i32::from(u16::from_le_bytes([low, high]) as i16)
    };
    let total_count: i32 = bands.iter().map(|band| band.count as i32).sum();
    let range = width * total_count + 6;

    let candidates = [raw, per_band, cross, range];
    let mut mode = 0usize;
    let mut best = candidates[0];
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if *candidate < best {
            best = *candidate;
            mode = index;
        }
    }

    Ok(ZerothGainLevelSelection {
        candidates,
        mode,
        fixed_width: width,
        fixed_min: minimum,
    })
}

/// The zeroth gain level (`idlev`) mode selection for channel 1
/// (decompile line 37036), scored against channel 0's records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothGainLevelChannel1Selection {
    /// Raw 4-bit, `idlev_pack_D` cross-channel deltas (reference level
    /// at the same index, or 7 beyond the reference band's count),
    /// per-band copy flags (one bit per active band plus the
    /// `pack_A`/`pack_B` chain for mismatched bands), and the full
    /// copy (0 when every band matches, else the `0x4000` sentinel).
    pub candidates: [i32; 4],
    pub mode: usize,
    /// Per-band copy-flag words (`+0x6d2b..`): 0 when the band matches
    /// the reference, 1 otherwise (bands with no points keep the
    /// initial 1 without contributing bits).
    pub copy_flags: Vec<u32>,
}

/// Native channel-1 idlev candidate scoring; the minimum picks the
/// mode word at `+0x6d28`.
pub fn zeroth_gain_idlev_mode_ch1_at5(
    bands: &[ZerothGainLevelBand<'_>],
    reference: &[ZerothGainLevelBand<'_>],
) -> Result<ZerothGainLevelChannel1Selection, AllocationError> {
    use crate::tables::generated::{G_A_IDLEV_PACK_A, G_A_IDLEV_PACK_B, G_A_IDLEV_PACK_D};

    if reference.len() < bands.len() {
        return Err(AllocationError::ChannelRowsTooShort {
            needed: bands.len(),
            actual: reference.len(),
        });
    }
    for band in bands.iter().chain(reference.iter()) {
        if band.levels.len() < band.count {
            return Err(AllocationError::ChannelRowsTooShort {
                needed: band.count,
                actual: band.levels.len(),
            });
        }
    }
    let huffman_len =
        |table: &[u8], index: i32| -> i32 { i32::from(table[(index as usize & 0xf) * 4 + 2]) };

    let raw: i32 = bands.iter().map(|band| band.count as i32 * 4).sum();

    // Candidate 1: pack_D deltas against the reference channel.
    let mut delta = 0i32;
    for (band, reference_band) in bands.iter().zip(reference) {
        for index in 0..band.count {
            let reference_level = if index < reference_band.count {
                reference_band.levels[index]
            } else {
                7
            };
            delta += huffman_len(&G_A_IDLEV_PACK_D, band.levels[index] - reference_level);
        }
    }

    // Candidate 2: one copy-flag bit per active band plus the per-band
    // chain for mismatched bands; the flags land at `+0x6d2b..`.
    let mut flagged = 0i32;
    let mut copy_flags = Vec::with_capacity(bands.len());
    for (band, reference_band) in bands.iter().zip(reference) {
        let mut flag = 1u32;
        if band.count > 0 {
            let matches = (0..band.count).all(|index| {
                if index < reference_band.count {
                    band.levels[index] == reference_band.levels[index]
                } else {
                    band.levels[index] == 7
                }
            });
            flag = u32::from(!matches);
            flagged += 1;
            if flag != 0 {
                flagged += i32::from(G_A_IDLEV_PACK_A[(band.levels[0] as usize) * 4 + 2]);
                for pair in band.levels[..band.count].windows(2) {
                    flagged += huffman_len(&G_A_IDLEV_PACK_B, pair[1] - pair[0]);
                }
            }
        }
        copy_flags.push(flag);
    }

    // Candidate 3: free when every band matches the reference.
    let all_match = bands.iter().zip(reference).all(|(band, reference_band)| {
        (0..band.count).all(|index| {
            if index < reference_band.count {
                band.levels[index] == reference_band.levels[index]
            } else {
                band.levels[index] == 7
            }
        })
    });
    let copy = if all_match { 0 } else { 0x4000 };

    let candidates = [raw, delta, flagged, copy];
    let mut mode = 0usize;
    let mut best = candidates[0];
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if *candidate < best {
            best = *candidate;
            mode = index;
        }
    }

    Ok(ZerothGainLevelChannel1Selection {
        candidates,
        mode,
        copy_flags,
    })
}

/// One band's gain-record row for the zeroth idloc selection: the
/// point count, the locations (record words 1..) and the levels
/// (record words 8..) that steer the attack/release table choices.
#[derive(Debug, Clone, Copy)]
pub struct ZerothGainLocationBand<'a> {
    pub count: usize,
    pub locations: &'a [i32],
    pub levels: &'a [i32],
}

fn zeroth_gain_location_band_check(
    band: &ZerothGainLocationBand<'_>,
) -> Result<(), AllocationError> {
    let needed = band.count;
    let actual = band.locations.len().min(band.levels.len());
    if actual < needed {
        return Err(AllocationError::ChannelRowsTooShort { needed, actual });
    }
    Ok(())
}

/// Native raw location coding: the first point costs a fixed five
/// bits; every later point is coded in `sa_idloc_0[previous
/// location]` bits (the bits needed for a location known to exceed
/// it, so the last location never indexes the table).
fn zeroth_raw_location_cost(band: &ZerothGainLocationBand<'_>) -> Result<i32, AllocationError> {
    use crate::tables::generated::SA_IDLOC_0;

    if band.count == 0 {
        return Ok(0);
    }
    let mut cost = 5i32;
    for &location in &band.locations[..band.count - 1] {
        if !(0..SA_IDLOC_0.len() as i32).contains(&location) {
            return Err(AllocationError::GainLocationOutOfRange { value: location });
        }
        cost += i32::from(SA_IDLOC_0[location as usize] as i8);
    }
    Ok(cost)
}

/// Code length from a 32-entry idloc pack table (stride 4, length at
/// byte 2); the caller decides whether the index was masked.
fn zeroth_idloc_pack_len(table: &[u8], index: i32) -> Result<i32, AllocationError> {
    if !(0..32).contains(&index) {
        return Err(AllocationError::GainLocationDeltaOutOfRange { value: index });
    }
    Ok(i32::from(table[index as usize * 4 + 2]))
}

/// Within-band location delta chain: five bits for the first point,
/// then `idloc_pack_A_rel` (level rising into the point) or
/// `idloc_pack_A_atk` per unmasked consecutive-location delta.
fn zeroth_within_band_location_cost(
    band: &ZerothGainLocationBand<'_>,
) -> Result<i32, AllocationError> {
    use crate::tables::generated::{G_A_IDLOC_PACK_A_ATK, G_A_IDLOC_PACK_A_REL};

    let mut cost = 5i32;
    for index in 1..band.count {
        let table = if band.levels[index - 1] < band.levels[index] {
            &G_A_IDLOC_PACK_A_REL
        } else {
            &G_A_IDLOC_PACK_A_ATK
        };
        cost += zeroth_idloc_pack_len(table, band.locations[index] - band.locations[index - 1])?;
    }
    Ok(cost)
}

/// The zeroth gain location (`idloc`) mode selection for channel 0
/// (decompile line 37172).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothGainLocationSelection {
    /// Raw (five bits then `sa_idloc_0`), within-band delta chain
    /// (`idloc_pack_A_atk`/`_rel` picked by the level slope),
    /// cross-band chain (band 0 raw, later bands via the `_B_` tables
    /// over `& 0x1f`-masked same-index deltas or the `_A_` tables
    /// beyond the previous band's count), and the range mode
    /// (`sa_idloc_3[max - min]` of location minus point index, times
    /// the total count, plus 7).
    pub candidates: [i32; 4],
    pub mode: usize,
    /// Range-mode width (`+0x6d3c`) and minimum (`+0x6d3d`).
    pub fixed_width: i32,
    pub fixed_min: i32,
}

/// Native channel-0 idloc candidate scoring over the active bands'
/// gain-record locations; the minimum picks the mode word at
/// `+0x6d3b`.
pub fn zeroth_gain_idloc_mode_at5(
    bands: &[ZerothGainLocationBand<'_>],
) -> Result<ZerothGainLocationSelection, AllocationError> {
    use crate::tables::generated::{
        G_A_IDLOC_PACK_A_ATK, G_A_IDLOC_PACK_A_REL, G_A_IDLOC_PACK_B_ATK, G_A_IDLOC_PACK_B_REL,
        SA_IDLOC_3,
    };

    for band in bands {
        zeroth_gain_location_band_check(band)?;
    }

    // Candidate 0: raw location coding for every band.
    let mut raw = 0i32;
    for band in bands {
        raw += zeroth_raw_location_cost(band)?;
    }

    // Candidate 1: the within-band delta chain per active band.
    let mut per_band = 0i32;
    for band in bands {
        if band.count > 0 {
            per_band += zeroth_within_band_location_cost(band)?;
        }
    }

    // Candidate 2: cross-band chain — band 0 raw, later bands coded
    // against the previous band with the B tables (masked same-index
    // deltas; B_atk always for the first point) and falling back to
    // the A tables beyond the previous band's count.
    let mut cross = 0i32;
    if let Some(first) = bands.first() {
        cross += zeroth_raw_location_cost(first)?;
        for window in bands.windows(2) {
            let previous = &window[0];
            let current = &window[1];
            if current.count == 0 {
                continue;
            }
            let first_index = if previous.count < 1 {
                current.locations[0]
            } else {
                (current.locations[0] - previous.locations[0]) & 0x1f
            };
            cross += zeroth_idloc_pack_len(&G_A_IDLOC_PACK_B_ATK, first_index)?;
            for index in 1..current.count {
                let rising = current.levels[index - 1] < current.levels[index];
                if index < previous.count {
                    let delta = (current.locations[index] - previous.locations[index]) & 0x1f;
                    let table = if rising {
                        &G_A_IDLOC_PACK_B_REL
                    } else {
                        &G_A_IDLOC_PACK_B_ATK
                    };
                    cross += zeroth_idloc_pack_len(table, delta)?;
                } else {
                    let delta = current.locations[index] - current.locations[index - 1];
                    let table = if rising {
                        &G_A_IDLOC_PACK_A_REL
                    } else {
                        &G_A_IDLOC_PACK_A_ATK
                    };
                    cross += zeroth_idloc_pack_len(table, delta)?;
                }
            }
        }
    }

    // Candidate 3: range mode over location minus point index.
    let mut minimum = 0x1f_i32;
    let mut maximum = 0i32;
    for band in bands {
        for (index, &location) in band.locations[..band.count].iter().enumerate() {
            let value = location - index as i32;
            minimum = minimum.min(value);
            maximum = maximum.max(value);
        }
    }
    // With no points at all the native indexes sa_idloc_3 with a
    // negative max-min and reads garbage — harmless because the width
    // multiplies a zero total; the port uses 0 instead.
    let width = if maximum < minimum {
        0
    } else {
        let table_index = (maximum - minimum) as usize;
        if table_index >= SA_IDLOC_3.len() / 2 {
            return Err(AllocationError::GainLocationDeltaOutOfRange {
                value: maximum - minimum,
            });
        }
        let low = SA_IDLOC_3[table_index * 2];
        let high = SA_IDLOC_3[table_index * 2 + 1];
        i32::from(u16::from_le_bytes([low, high]) as i16)
    };
    let total_count: i32 = bands.iter().map(|band| band.count as i32).sum();
    let range = width * total_count + 7;

    let candidates = [raw, per_band, cross, range];
    let mut mode = 0usize;
    let mut best = candidates[0];
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if *candidate < best {
            best = *candidate;
            mode = index;
        }
    }

    Ok(ZerothGainLocationSelection {
        candidates,
        mode,
        fixed_width: width,
        fixed_min: minimum,
    })
}

/// The zeroth gain location (`idloc`) mode selection for channel 1
/// (decompile line 37341), scored against channel 0's records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothGainLocationChannel1Selection {
    /// Raw, cross-channel chain (`idloc_pack_C_atk` over masked
    /// same-index deltas, a 1-bit-plus-raw escape when the level
    /// rises into the point, and the `_A_` tables beyond the
    /// reference count), per-band copy flags (one flag bit only when
    /// the reference band has enough points, plus the within-band
    /// chain for uncopyable bands), and the full copy (extra points
    /// beyond the reference coded raw; any same-index location
    /// mismatch yields the `0x4000` sentinel).
    pub candidates: [i32; 4],
    pub mode: usize,
    /// Per-band copy-flag words (`+0x6d3e..`): 0 when the band copies
    /// the reference, 1 otherwise (bands with no points keep the
    /// initial 1).
    pub copy_flags: Vec<u32>,
    /// Full-copy progress words (`+0x6d4e..`): the native writes 0 at
    /// each band's start and 1 after it survives, so on the sentinel
    /// path only a prefix is written (trailing 0 for the failing
    /// band, nothing after it).
    pub copy_markers: Vec<u32>,
}

/// Native channel-1 idloc candidate scoring; the minimum picks the
/// mode word at `+0x6d3b`.
pub fn zeroth_gain_idloc_mode_ch1_at5(
    bands: &[ZerothGainLocationBand<'_>],
    reference: &[ZerothGainLocationBand<'_>],
) -> Result<ZerothGainLocationChannel1Selection, AllocationError> {
    use crate::tables::generated::{
        G_A_IDLOC_PACK_A_ATK, G_A_IDLOC_PACK_A_REL, G_A_IDLOC_PACK_C_ATK, SA_IDLOC_0,
    };

    if reference.len() < bands.len() {
        return Err(AllocationError::ChannelRowsTooShort {
            needed: bands.len(),
            actual: reference.len(),
        });
    }
    for band in bands.iter().chain(reference.iter()) {
        zeroth_gain_location_band_check(band)?;
    }

    // Candidate 0: raw location coding for every band.
    let mut raw = 0i32;
    for band in bands {
        raw += zeroth_raw_location_cost(band)?;
    }

    // Candidate 1: cross-channel chain — C_atk over the masked
    // same-index delta for the first point (raw location when the
    // reference band is empty) and for later points with a
    // non-rising level; a rising level costs one flag bit plus the
    // raw sa_idloc_0 escape when the delta is nonzero; points beyond
    // the reference count use the within-band A tables.
    let mut cross = 0i32;
    for (band, reference_band) in bands.iter().zip(reference) {
        if band.count == 0 {
            continue;
        }
        let first_index = if reference_band.count < 1 {
            band.locations[0]
        } else {
            (band.locations[0] - reference_band.locations[0]) & 0x1f
        };
        cross += zeroth_idloc_pack_len(&G_A_IDLOC_PACK_C_ATK, first_index)?;
        for index in 1..band.count {
            let rising = band.levels[index - 1] < band.levels[index];
            if index < reference_band.count {
                let delta = (band.locations[index] - reference_band.locations[index]) & 0x1f;
                if rising {
                    cross += 1;
                    if delta != 0 {
                        let previous = band.locations[index - 1];
                        if !(0..SA_IDLOC_0.len() as i32).contains(&previous) {
                            return Err(AllocationError::GainLocationOutOfRange {
                                value: previous,
                            });
                        }
                        cross += i32::from(SA_IDLOC_0[previous as usize] as i8);
                    }
                } else {
                    cross += zeroth_idloc_pack_len(&G_A_IDLOC_PACK_C_ATK, delta)?;
                }
            } else {
                let delta = band.locations[index] - band.locations[index - 1];
                let table = if rising {
                    &G_A_IDLOC_PACK_A_REL
                } else {
                    &G_A_IDLOC_PACK_A_ATK
                };
                cross += zeroth_idloc_pack_len(table, delta)?;
            }
        }
    }

    // Candidate 2: per-band copy flags — no flag bit when the
    // reference band is too short (the decoder can infer the copy is
    // impossible), otherwise one bit plus the within-band chain for
    // mismatched bands; the flags land at `+0x6d3e..`.
    let mut flagged = 0i32;
    let mut copy_flags = Vec::with_capacity(bands.len());
    for (band, reference_band) in bands.iter().zip(reference) {
        let mut flag = 1u32;
        if band.count > 0 {
            if reference_band.count < band.count {
                flagged += zeroth_within_band_location_cost(band)?;
            } else {
                flagged += 1;
                let matches =
                    band.locations[..band.count] == reference_band.locations[..band.count];
                flag = u32::from(!matches);
                if flag != 0 {
                    flagged += zeroth_within_band_location_cost(band)?;
                }
            }
        }
        copy_flags.push(flag);
    }

    // Candidate 3: full copy — same-index locations must match the
    // reference exactly (else the 0x4000 sentinel kills the
    // candidate); points beyond the reference count are coded raw.
    // The per-band progress words land at `+0x6d4e..`.
    let mut copy_cost = 0i32;
    let mut copy_markers = Vec::with_capacity(bands.len());
    let mut copy_sentinel = false;
    'bands: for (band, reference_band) in bands.iter().zip(reference) {
        copy_markers.push(0u32);
        for index in 0..band.count {
            if index < reference_band.count {
                if band.locations[index] != reference_band.locations[index] {
                    copy_sentinel = true;
                    break 'bands;
                }
            } else if index == 0 {
                copy_cost += 5;
            } else {
                let previous = band.locations[index - 1];
                if !(0..SA_IDLOC_0.len() as i32).contains(&previous) {
                    return Err(AllocationError::GainLocationOutOfRange { value: previous });
                }
                copy_cost += i32::from(SA_IDLOC_0[previous as usize] as i8);
            }
        }
        *copy_markers.last_mut().unwrap() = 1;
    }
    let copy = if copy_sentinel { 0x4000 } else { copy_cost };

    let candidates = [raw, cross, flagged, copy];
    let mut mode = 0usize;
    let mut best = candidates[0];
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if *candidate < best {
            best = *candidate;
            mode = index;
        }
    }

    Ok(ZerothGainLocationChannel1Selection {
        candidates,
        mode,
        copy_flags,
        copy_markers,
    })
}

/// One channel's winning zeroth gain side-data costs (decompile line
/// 36796): the record entry flag at `+0x6d21` gates the whole
/// contribution; the ngc/idlev/idloc bits are the minimum candidates
/// picked into `+0x6d25`/`+0x6d28`/`+0x6d3b`.
#[derive(Debug, Clone, Copy)]
pub struct ZerothGainSideChannelCosts {
    pub active: bool,
    pub ngc_bits: i32,
    pub idlev_bits: i32,
    pub idloc_bits: i32,
}

/// The gain side-data words written at the end of the zeroth tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothGainSideDataTotal {
    /// u16 total at `+0x124` (the native truncating cast).
    pub gain_bits_124: i16,
    /// u16 at `+0x122`, always zeroed alongside (decompile line
    /// 37574).
    pub cleared_122: i16,
}

/// Native gain side-data budget summation (decompile lines 36782 and
/// 37567): the header seed (channel count plus 11/15 per flagged
/// channel, from `compute_zeroth_gha_bit_seed_at5`) plus each active
/// channel's ngc + idlev + idloc winning costs, truncated to the u16
/// at `+0x124`.
pub fn zeroth_gain_side_data_total_at5(
    seed_bits: i32,
    channels: &[ZerothGainSideChannelCosts],
) -> ZerothGainSideDataTotal {
    let mut total = seed_bits;
    for channel in channels {
        if channel.active {
            total = total
                .wrapping_add(channel.ngc_bits)
                .wrapping_add(channel.idlev_bits)
                .wrapping_add(channel.idloc_bits);
        }
    }
    ZerothGainSideDataTotal {
        gain_bits_124: total as u16 as i16,
        cleared_122: 0,
    }
}

/// One activity-summary word pair in the zeroth trailing side-data
/// blocks (decompile lines 37575, 37617, and 37644 share the shape):
/// an "any active" flag, a "partial" flag, and the signalling cost —
/// 1 bit when nothing is active, 2 bits when everything is, or
/// count + 2 for a per-entry bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothActivitySummary {
    pub any_flag: u32,
    pub partial_flag: u32,
    pub bits: i16,
}

/// Native activity summary over `count` flag words.
pub fn zeroth_activity_summary_at5(
    flags: &[i32],
    count: usize,
) -> Result<ZerothActivitySummary, AllocationError> {
    if flags.len() < count {
        return Err(AllocationError::ActivityTooShort {
            needed: count,
            actual: flags.len(),
        });
    }
    let sum: i32 = flags[..count].iter().sum();
    let (any_flag, partial_flag, bits) = if count < 1 || sum == 0 {
        (0, 0, 1)
    } else if sum == count as i32 {
        (1, 0, 2)
    } else {
        (1, 1, (count as i16).wrapping_add(2))
    };
    Ok(ZerothActivitySummary {
        any_flag,
        partial_flag,
        bits,
    })
}

/// The per-channel band-activity signalling block (decompile line
/// 37575): each channel's flags at records `+0x988..` summarize into
/// the words at `+0x980`/`+0x984`, and the costs accumulate into the
/// u16 at `+0x122` (replacing the zero stored by the gain side-data
/// tail).
pub fn zeroth_band_activity_bits_at5(
    channel_activity: &[&[i32]],
    band_count: usize,
) -> Result<(Vec<ZerothActivitySummary>, i16), AllocationError> {
    let mut summaries = Vec::with_capacity(channel_activity.len());
    let mut bits = 0i16;
    for activity in channel_activity {
        let summary = zeroth_activity_summary_at5(activity, band_count)?;
        bits = bits.wrapping_add(summary.bits);
        summaries.push(summary);
    }
    Ok((summaries, bits))
}

/// The tone side-data block into the u16 at `+0x120` (decompile line
/// 37609): zero below tone mode 3, otherwise
/// `(g_a_idspcbands_at5[group_count - 1] * 4 + 4) * channel_count`
/// (the decompile's `g_a_idspcqus_at5[count + 0x1f]` is the folded
/// alias of the next table, confirmed by the GOT pointer at disasm
/// 0x44bee). Stereo then appends primary and secondary tone-activity
/// summaries (flag words `piVar9[0]/[1]` and `piVar9[0x12]/[0x13]`)
/// regardless of the mode gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZerothToneSideBits {
    pub bits_120: i16,
    pub primary: Option<ZerothActivitySummary>,
    pub secondary: Option<ZerothActivitySummary>,
}

pub fn zeroth_tone_side_bits_at5(
    tone_mode_2c: i32,
    tone_group_count_30: i32,
    channel_count: usize,
    primary_activity: &[i32],
    secondary_activity: &[i32],
) -> Result<ZerothToneSideBits, AllocationError> {
    use crate::tables::generated::G_A_IDSPCBANDS_AT5;

    validate_channel_count(channel_count)?;
    let mut bits = 0i16;
    if tone_mode_2c >= 3 {
        let count = tone_group_count_30;
        if !(1..=G_A_IDSPCBANDS_AT5.len() as i32).contains(&count) {
            return Err(AllocationError::ToneGroupCountTooLarge {
                count: count.max(0) as usize,
                max: G_A_IDSPCBANDS_AT5.len(),
            });
        }
        let width = i16::from(G_A_IDSPCBANDS_AT5[count as usize - 1]);
        bits = width
            .wrapping_mul(4)
            .wrapping_add(4)
            .wrapping_mul(channel_count as i16);
    }

    let (primary, secondary) = if channel_count == 2 {
        let groups = tone_group_count_30.max(0) as usize;
        let primary = zeroth_activity_summary_at5(primary_activity, groups)?;
        let secondary = zeroth_activity_summary_at5(secondary_activity, groups)?;
        bits = bits.wrapping_add(primary.bits).wrapping_add(secondary.bits);
        (Some(primary), Some(secondary))
    } else {
        (None, None)
    };

    Ok(ZerothToneSideBits {
        bits_120: bits,
        primary,
        secondary,
    })
}

/// The seven side-data u16 words summed by the zeroth epilogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothSideBitWords {
    pub mode_bits_118: i16,
    pub idwl_bits_11a: i16,
    pub idsf_bits_11c: i16,
    pub idct_bits_11e: i16,
    pub tone_bits_120: i16,
    pub activity_bits_122: i16,
    pub gain_bits_124: i16,
}

/// The zeroth epilogue's stored totals (decompile line 37673).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZerothFinalBitTotals {
    /// `calc_nbits_for_gha_at5` result stored at `+0x126`.
    pub gha_bits_126: i16,
    /// 9 when the tone flag (`piVar9[0x25]`) is set, else 1
    /// (`+0x128`).
    pub header_bits_128: i16,
    /// Sum of the seven side words plus gha and header (`+0x12a`).
    pub base_total_12a: i16,
    /// The base total plus each channel's word-length header cost
    /// (`*(param_1[ch] + 0x46 + *(channel + 0x1074) * 2)`), stored at
    /// `+0x12e` and returned by the native function.
    pub extended_total_12e: i16,
    /// Whether the relax rule fired and rewrote word-length rows.
    pub relaxed: bool,
}

/// Native zeroth epilogue: totals into `+0x126/128/12a/12e`, then the
/// relax rule — when the channel-0 gate word
/// (`*(*(*param_3 + 0x30) + 0x14)`) is zero and the extended total is
/// below 90% of the frame budget, every word-length row whose header
/// word is under 7 has its `band_count` band words forced to 7. The
/// x87 compare multiplies the i32 budget by an f32 0.9 constant
/// (disasm 0x44d04 `fmuls`) with an exact product for realistic
/// budgets, so f64 arithmetic reproduces the decision.
pub fn zeroth_final_bit_totals_at5(
    words: &ZerothSideBitWords,
    gha_bits: i16,
    tone_flag_25: bool,
    channel_extra_bits: &[i16],
    relax_gate_zero: bool,
    frame_bit_budget: i32,
    word_length_rows: &mut [&mut [i16]],
    band_count: usize,
) -> Result<ZerothFinalBitTotals, AllocationError> {
    if channel_extra_bits.len() < word_length_rows.len() {
        return Err(AllocationError::ChannelRowsTooShort {
            needed: word_length_rows.len(),
            actual: channel_extra_bits.len(),
        });
    }
    for row in word_length_rows.iter() {
        if row.len() < band_count + 1 {
            return Err(AllocationError::WordLengthsTooShort {
                needed: band_count + 1,
                actual: row.len(),
            });
        }
    }

    let header_bits_128 = if tone_flag_25 { 9 } else { 1 };
    let base_total_12a = words
        .idwl_bits_11a
        .wrapping_add(words.mode_bits_118)
        .wrapping_add(words.idsf_bits_11c)
        .wrapping_add(words.idct_bits_11e)
        .wrapping_add(words.tone_bits_120)
        .wrapping_add(words.activity_bits_122)
        .wrapping_add(words.gain_bits_124)
        .wrapping_add(gha_bits)
        .wrapping_add(header_bits_128);
    let mut extended_total_12e = base_total_12a;
    for extra in &channel_extra_bits[..word_length_rows.len()] {
        extended_total_12e = extended_total_12e.wrapping_add(*extra);
    }

    let relaxed = relax_gate_zero
        && f64::from(extended_total_12e) < f64::from(frame_bit_budget) * f64::from(0.9f32);
    if relaxed {
        for row in word_length_rows.iter_mut() {
            if row[0] < 7 {
                for band in 0..band_count {
                    row[1 + band] = 7;
                }
            }
        }
    }

    Ok(ZerothFinalBitTotals {
        gha_bits_126: gha_bits,
        header_bits_128,
        base_total_12a,
        extended_total_12e,
        relaxed,
    })
}
