use crate::dsp::scalar::{ScalarError, sub_seq_at5_in_place_a};
use crate::gha::power::{PowerCheckError, check_power_level_at5};
use crate::gha::synthesis::{
    COMPONENT_SAMPLES, GhaSynthesisError, GhaSynthesisState, GhaWaveRecord, synthesis_wav_at5,
};
use crate::tables::at5::{WIN_AT5_UPPER_HALF_INDEX, win_at5_ref};

const MAX_EXTRACT_BANDS_AT5: usize = 16;
const MAX_EXTRACT_NWAVS_AT5: usize = 0x30;
/// The window-detector source length (`0x180`) that `analysis_general_at5`
/// reads from each `param_2` band buffer, so every band buffer the driver
/// consumes must be at least this long.
const EXTRACT_GENERAL_SOURCE_SAMPLES_AT5: usize = 0x180;
const EXTRACT_GHA_RECORD_STRIDE_BYTES_AT5: u32 = 0x10;
const EXTRACT_GHA_RECORD_ARENA_OFFSET_AT5: u32 = 0x0c;
pub const EXTRACT_GHA_HEADER_WORD_COUNT_AT5: usize = 3;
pub const EXTRACT_GHA_ROW_WORD_COUNT_AT5: usize = 10;
pub const EXTRACT_GHA_RESIDUAL_SAMPLES_AT5: usize = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhaExtractError {
    UnsupportedChannelCount {
        channel_count: usize,
    },
    PreviousChannelCountMismatch {
        current: usize,
        previous: usize,
    },
    UnsupportedBandCount {
        band_count: usize,
    },
    InvalidSelectedBandCount {
        selected_band_count: usize,
        band_count: usize,
    },
    EnergyTableTooShort {
        channel: usize,
        needed: usize,
        actual: usize,
    },
    ChannelModeFlagsTooShort {
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
    InvalidSelectedBandIndex {
        band: usize,
        group_count: usize,
    },
    InvalidWritebackChannel {
        channel_index: usize,
        channel_count: usize,
    },
    RowsTooShort {
        needed: usize,
        actual: usize,
    },
    InputTooShort {
        needed: usize,
        actual: usize,
    },
    Power(PowerCheckError),
    Synthesis(GhaSynthesisError),
    Scalar(ScalarError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaRow {
    pub active: bool,
    pub nwavs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaChannelSummary {
    pub mode: usize,
    pub band_count: usize,
    pub active_row_count: usize,
    pub total_active_nwavs: usize,
    pub max_active_nwavs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaChannelModeFlags {
    pub initial_mode: usize,
    pub flag_0x28: bool,
    pub flag_0x30: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaHeader {
    pub mode: usize,
    pub scheduler_group_count: usize,
    pub mode_mask: usize,
    pub sets_global_mode_flag: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaSelectedBandEntry {
    pub channel_index: usize,
    pub selected_index: usize,
    pub caller_band_index: usize,
    pub matrix_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractGhaGeneralCallShape {
    pub channel_count: usize,
    pub group_count: usize,
    pub profile_selector: usize,
    pub selected_band_order: Vec<usize>,
    pub selected_band_entries: Vec<ExtractGhaSelectedBandEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaWaveLimit {
    pub effective_total_nwavs: usize,
    pub clears_rows: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractGhaResidualSource {
    Current,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaResidualSynthesisCall {
    pub band_index: usize,
    pub channel_index: usize,
    pub source: ExtractGhaResidualSource,
}

pub fn extract_ghwave_channel_summary_at5(
    mode: usize,
    band_count: usize,
    rows: &[ExtractGhaRow],
) -> Result<ExtractGhaChannelSummary, GhaExtractError> {
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if rows.len() < band_count {
        return Err(GhaExtractError::RowsTooShort {
            needed: band_count,
            actual: rows.len(),
        });
    }

    let mut active_row_count = 0usize;
    let mut total_active_nwavs = 0usize;
    let mut max_active_nwavs = 0usize;
    for row in rows.iter().take(band_count) {
        if row.active {
            active_row_count += 1;
            total_active_nwavs += row.nwavs;
            max_active_nwavs = max_active_nwavs.max(row.nwavs);
        }
    }

    Ok(ExtractGhaChannelSummary {
        mode,
        band_count,
        active_row_count,
        total_active_nwavs,
        max_active_nwavs,
    })
}

pub fn extract_ghwave_band_energy_at5(samples: &[f32]) -> Result<f32, GhaExtractError> {
    if samples.len() < COMPONENT_SAMPLES {
        return Err(GhaExtractError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: samples.len(),
        });
    }

    Ok((check_power_level_at5(
        &samples[..COMPONENT_SAMPLES],
        &samples[..COMPONENT_SAMPLES],
        COMPONENT_SAMPLES,
    )? * 0.003_906_25) as f32)
}

pub fn extract_ghwave_selected_band_order_at5(
    energy_tables: &[&[f32]],
    band_count: usize,
    selected_band_count: usize,
) -> Result<Vec<usize>, GhaExtractError> {
    if !(1..=2).contains(&energy_tables.len()) {
        return Err(GhaExtractError::UnsupportedChannelCount {
            channel_count: energy_tables.len(),
        });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if selected_band_count > band_count {
        return Err(GhaExtractError::InvalidSelectedBandCount {
            selected_band_count,
            band_count,
        });
    }
    for (channel, table) in energy_tables.iter().enumerate() {
        if table.len() < band_count {
            return Err(GhaExtractError::EnergyTableTooShort {
                channel,
                needed: band_count,
                actual: table.len(),
            });
        }
    }

    let mut summed_energy = vec![0.0f32; band_count];
    for table in energy_tables {
        for (total, energy) in summed_energy
            .iter_mut()
            .zip(table.iter().copied().take(band_count))
        {
            *total += energy;
        }
    }

    let mut order: Vec<usize> = (0..band_count).collect();
    shell_sort_energy_descending_at5(&mut summed_energy, &mut order);
    Ok(order
        .into_iter()
        .filter(|&band| band < selected_band_count)
        .collect())
}

pub fn extract_ghwave_header_at5(
    channel_flags: &[ExtractGhaChannelModeFlags],
    band_count: usize,
    channel_count: usize,
    threshold: u32,
    allow_compact_groups: bool,
) -> Result<ExtractGhaHeader, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if channel_flags.len() < channel_count {
        return Err(GhaExtractError::ChannelModeFlagsTooShort {
            needed: channel_count,
            actual: channel_flags.len(),
        });
    }

    let mode_mask = channel_flags
        .iter()
        .take(channel_count)
        .fold(0usize, |mask, flags| {
            mask | extract_ghwave_channel_mode_at5(*flags)
        });

    let (mode, mut scheduler_group_count, sets_global_mode_flag) = if mode_mask == 1 {
        (1, band_count, true)
    } else if mode_mask == 2 {
        (1, band_count, false)
    } else if !allow_compact_groups {
        (0, 1, false)
    } else {
        let group_count = if (threshold < 0x13 && channel_count == 1)
            || (threshold < 0x19 && channel_count == 2)
        {
            1
        } else {
            2
        };
        (1, group_count, false)
    };

    if mode_mask != 1 && mode_mask != 2 {
        scheduler_group_count = scheduler_group_count.min(band_count);
    }

    Ok(ExtractGhaHeader {
        mode,
        scheduler_group_count,
        mode_mask,
        sets_global_mode_flag,
    })
}

pub fn extract_ghwave_general_call_shape_at5(
    header: ExtractGhaHeader,
    source_band_count: usize,
    channel_count: usize,
    selected_band_order: &[usize],
    profile_selector: usize,
    allow_compact_groups: bool,
) -> Result<Option<ExtractGhaGeneralCallShape>, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if source_band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount {
            band_count: source_band_count,
        });
    }
    if !(allow_compact_groups && header.mode_mask == 3) {
        return Ok(None);
    }

    let group_count = header.scheduler_group_count;
    if group_count > source_band_count {
        return Err(GhaExtractError::InvalidSelectedBandCount {
            selected_band_count: group_count,
            band_count: source_band_count,
        });
    }
    if selected_band_order.len() < group_count {
        return Err(GhaExtractError::SelectedBandOrderTooShort {
            needed: group_count,
            actual: selected_band_order.len(),
        });
    }
    for &band in selected_band_order.iter().take(group_count) {
        if band >= group_count {
            return Err(GhaExtractError::InvalidSelectedBandIndex { band, group_count });
        }
    }

    let selected_band_order = selected_band_order[..group_count].to_vec();
    let mut selected_band_entries = Vec::with_capacity(channel_count * group_count);
    for channel_index in 0..channel_count {
        for (selected_index, &caller_band_index) in selected_band_order.iter().enumerate() {
            selected_band_entries.push(ExtractGhaSelectedBandEntry {
                channel_index,
                selected_index,
                caller_band_index,
                matrix_index: channel_index * source_band_count + caller_band_index,
            });
        }
    }

    Ok(Some(ExtractGhaGeneralCallShape {
        channel_count,
        group_count,
        profile_selector,
        selected_band_order,
        selected_band_entries,
    }))
}

pub fn extract_ghwave_wave_limit_at5(
    channel_rows: &[&[ExtractGhaRow]],
    band_count: usize,
    shared_flags: &[bool],
) -> Result<ExtractGhaWaveLimit, GhaExtractError> {
    let channel_count = channel_rows.len();
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if channel_count > 1 && shared_flags.len() < band_count {
        return Err(GhaExtractError::SharedFlagsTooShort {
            needed: band_count,
            actual: shared_flags.len(),
        });
    }
    for rows in channel_rows {
        if rows.len() < band_count {
            return Err(GhaExtractError::RowsTooShort {
                needed: band_count,
                actual: rows.len(),
            });
        }
    }

    let mut effective_total_nwavs = 0usize;
    for (channel, rows) in channel_rows.iter().enumerate() {
        for band in 0..band_count {
            if channel == 0 || !shared_flags[band] {
                effective_total_nwavs += rows[band].nwavs;
            }
        }
    }

    Ok(ExtractGhaWaveLimit {
        effective_total_nwavs,
        clears_rows: effective_total_nwavs > MAX_EXTRACT_NWAVS_AT5,
    })
}

pub fn extract_ghwave_init_row_words_at5(row_words: &mut [[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]]) {
    for row in row_words {
        reset_row_words_at5(row);
    }
}

pub fn extract_ghwave_clear_overflow_row_words_at5(
    row_words: &mut [[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]],
) {
    for row in row_words {
        reset_row_words_at5(row);
    }
}

fn reset_row_words_at5(row: &mut [u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]) {
    row[0] = 0;
    row[1] = 0;
    row[4] = 0;
    row[5] = 0;
    row[6] = 0;
    row[7] = 0x20;
    row[8] = 0;
}

pub fn extract_ghwave_record_pointer_plan_at5(header_ptr: u32, wave_counts: &[usize]) -> Vec<u32> {
    let mut cumulative_waves = 0u32;
    let mut pointers = Vec::with_capacity(wave_counts.len());
    for &count in wave_counts {
        pointers.push(
            header_ptr
                .wrapping_add(EXTRACT_GHA_RECORD_ARENA_OFFSET_AT5)
                .wrapping_add(cumulative_waves.wrapping_mul(EXTRACT_GHA_RECORD_STRIDE_BYTES_AT5)),
        );
        cumulative_waves = cumulative_waves.wrapping_add(count as u32);
    }
    pointers
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractGhaRowWriteback {
    pub band_index: usize,
    pub channel_index: Option<usize>,
    pub row_words: [u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5],
}

pub fn extract_ghwave_write_row_words_at5(
    channel_rows: &mut [Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    band_count: usize,
    writebacks: &[ExtractGhaRowWriteback],
) -> Result<(), GhaExtractError> {
    let channel_count = channel_rows.len();
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    for rows in channel_rows.iter() {
        if rows.len() < band_count {
            return Err(GhaExtractError::RowsTooShort {
                needed: band_count,
                actual: rows.len(),
            });
        }
    }
    for writeback in writebacks {
        if writeback.band_index >= band_count {
            return Err(GhaExtractError::InvalidSelectedBandIndex {
                band: writeback.band_index,
                group_count: band_count,
            });
        }
        if let Some(channel_index) = writeback.channel_index {
            if channel_index >= channel_count {
                return Err(GhaExtractError::InvalidWritebackChannel {
                    channel_index,
                    channel_count,
                });
            }
        }
    }

    for writeback in writebacks {
        match writeback.channel_index {
            Some(channel_index) => {
                channel_rows[channel_index][writeback.band_index] = writeback.row_words;
            }
            None => {
                channel_rows[0][writeback.band_index] = writeback.row_words;
                if channel_count == 2 {
                    channel_rows[1][writeback.band_index] = writeback.row_words;
                }
            }
        }
    }

    Ok(())
}

pub fn extract_ghwave_reset_header_words_at5(
    header_words: &mut [u32; EXTRACT_GHA_HEADER_WORD_COUNT_AT5],
) {
    header_words[0] = 0;
    header_words[2] = 1;
}

pub fn extract_ghwave_write_header_words_at5(
    header: ExtractGhaHeader,
    active: bool,
) -> [u32; EXTRACT_GHA_HEADER_WORD_COUNT_AT5] {
    [
        u32::from(active),
        header.mode as u32,
        header.scheduler_group_count as u32,
    ]
}

pub fn extract_ghwave_residual_synthesis_plan_at5(
    current_rows: &[&[ExtractGhaRow]],
    previous_rows: &[&[ExtractGhaRow]],
    band_count: usize,
) -> Result<Vec<ExtractGhaResidualSynthesisCall>, GhaExtractError> {
    let channel_count = current_rows.len();
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if previous_rows.len() != channel_count {
        return Err(GhaExtractError::PreviousChannelCountMismatch {
            current: channel_count,
            previous: previous_rows.len(),
        });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    for rows in current_rows.iter().chain(previous_rows.iter()) {
        if rows.len() < band_count {
            return Err(GhaExtractError::RowsTooShort {
                needed: band_count,
                actual: rows.len(),
            });
        }
    }

    let mut calls = Vec::new();
    for band_index in 0..band_count {
        for channel_index in 0..channel_count {
            if current_rows[channel_index][band_index].nwavs > 0
                || previous_rows[channel_index][band_index].nwavs > 0
            {
                calls.push(ExtractGhaResidualSynthesisCall {
                    band_index,
                    channel_index,
                    source: ExtractGhaResidualSource::Previous,
                });
                calls.push(ExtractGhaResidualSynthesisCall {
                    band_index,
                    channel_index,
                    source: ExtractGhaResidualSource::Current,
                });
            }
        }
    }

    Ok(calls)
}

#[derive(Debug, Clone, Copy)]
pub struct ExtractGhaResidualRowInput<'a> {
    pub row_words: [u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5],
    pub records: &'a [GhaWaveRecord],
    pub scale_only_mode: u32,
    pub invert_flag: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractGhaResidualBuffers {
    pub delayed_raw: [f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5],
    pub current_raw: [f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5],
    pub delayed_windowed: [f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5],
    pub current_windowed: [f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5],
    pub sum: [f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5],
    pub windows_both: bool,
    pub delayed_windowed_applied: bool,
    pub current_windowed_applied: bool,
}

pub fn extract_ghwave_residual_at5(
    delayed: &ExtractGhaResidualRowInput<'_>,
    current: &ExtractGhaResidualRowInput<'_>,
    channel_index: usize,
    band_samples: &mut [f32],
) -> Result<ExtractGhaResidualBuffers, GhaExtractError> {
    if band_samples.len() < EXTRACT_GHA_RESIDUAL_SAMPLES_AT5 {
        return Err(GhaExtractError::InputTooShort {
            needed: EXTRACT_GHA_RESIDUAL_SAMPLES_AT5,
            actual: band_samples.len(),
        });
    }

    let mut delayed_buffer = [0.0f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5];
    let mut current_buffer = [0.0f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5];
    synthesis_wav_at5(
        &residual_synthesis_state_at5(delayed),
        &mut delayed_buffer,
        EXTRACT_GHA_RESIDUAL_SAMPLES_AT5,
        EXTRACT_GHA_RESIDUAL_SAMPLES_AT5,
        delayed.scale_only_mode != 0,
        delayed.invert_flag != 0,
        channel_index as i32,
    )?;
    synthesis_wav_at5(
        &residual_synthesis_state_at5(current),
        &mut current_buffer,
        0,
        EXTRACT_GHA_RESIDUAL_SAMPLES_AT5,
        current.scale_only_mode != 0,
        current.invert_flag != 0,
        channel_index as i32,
    )?;
    let delayed_raw = delayed_buffer;
    let current_raw = current_buffer;

    let delayed_wave_count = delayed.row_words[8] as i32;
    let current_wave_count = current.row_words[8] as i32;
    let windows_both = delayed_wave_count >= 1
        && current_wave_count >= 1
        && current.row_words[2] as i32 <= delayed.row_words[3] as i32 - 0x80;
    let delayed_windowed_applied =
        windows_both || (delayed_wave_count > 0 && delayed.row_words[1] == 0);
    let current_windowed_applied =
        windows_both || (current_wave_count > 0 && current.row_words[0] == 0);

    let window = win_at5_ref();
    if delayed_windowed_applied {
        for (index, sample) in delayed_buffer.iter_mut().enumerate() {
            *sample *= window[WIN_AT5_UPPER_HALF_INDEX + index];
        }
    }
    if current_windowed_applied {
        for (index, sample) in current_buffer.iter_mut().enumerate() {
            *sample *= window[index];
        }
    }

    let mut sum = [0.0f32; EXTRACT_GHA_RESIDUAL_SAMPLES_AT5];
    for (index, total) in sum.iter_mut().enumerate() {
        *total = current_buffer[index] + delayed_buffer[index];
    }

    sub_seq_at5_in_place_a(band_samples, &sum, EXTRACT_GHA_RESIDUAL_SAMPLES_AT5)?;

    Ok(ExtractGhaResidualBuffers {
        delayed_raw,
        current_raw,
        delayed_windowed: delayed_buffer,
        current_windowed: current_buffer,
        sum,
        windows_both,
        delayed_windowed_applied,
        current_windowed_applied,
    })
}

fn residual_synthesis_state_at5<'a>(
    input: &ExtractGhaResidualRowInput<'a>,
) -> GhaSynthesisState<'a> {
    GhaSynthesisState {
        lower_window: (input.row_words[0] != 0).then_some(input.row_words[2] as usize),
        upper_window: (input.row_words[1] != 0).then_some(input.row_words[3] as usize),
        waves: &input.records[..(input.row_words[8] as usize).min(input.records.len())],
    }
}

fn extract_ghwave_channel_mode_at5(flags: ExtractGhaChannelModeFlags) -> usize {
    if flags.initial_mode == 0 {
        0
    } else if flags.flag_0x28 {
        2
    } else if flags.flag_0x30 {
        1
    } else {
        3
    }
}

fn shell_sort_energy_descending_at5(energy: &mut [f32], order: &mut [usize]) {
    let count = energy.len();
    let mut gap = 1usize;
    if count > 0 {
        while gap <= count {
            gap = gap * 3 + 1;
        }
    }

    loop {
        gap /= 3;
        if gap == 0 {
            break;
        }

        for index in gap..count {
            let mut scan = index as isize - gap as isize;
            let current_energy = energy[index];
            while scan >= 0 && energy[scan as usize] < current_energy {
                let dst = scan as usize + gap;
                energy[dst] = energy[scan as usize];
                order.swap(dst, scan as usize);
                scan -= gap as isize;
            }
            energy[(scan + gap as isize) as usize] = current_energy;
        }
    }
}

impl From<PowerCheckError> for GhaExtractError {
    fn from(error: PowerCheckError) -> Self {
        Self::Power(error)
    }
}

impl From<GhaSynthesisError> for GhaExtractError {
    fn from(error: GhaSynthesisError) -> Self {
        Self::Synthesis(error)
    }
}

impl From<ScalarError> for GhaExtractError {
    fn from(error: ScalarError) -> Self {
        Self::Scalar(error)
    }
}

/// Per-channel mode-flag decision at the top of `extract_ghwave_at5`
/// (decompile from line 41688): after the per-band energy scan and the
/// descending Shell sort, the two flag words at `local_1c84 + 0x30` and
/// `+ 0x28` classify the channel spectrum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhaChannelModeDecision {
    /// Peaked spectrum (`min * 4 <= max`) with the header `+0xd0` enable
    /// word zero: the native code short-circuits to the general path
    /// with header mode 3 (`local_1c9c = 3`, header words 1/2 = 0/1).
    DisabledFallback,
    /// Both flag words written: `flag_0x30` is the top-2-dominant word
    /// (the two strongest bands hold at least `total * 0.99999`),
    /// `flag_0x28` the flat word (`min * 4 > max`); `initial_mode` is
    /// the 3 the later flag-zeroing loop stores for every channel,
    /// matching the `ExtractGhaChannelModeFlags` consumed by
    /// `extract_ghwave_header_at5`.
    Flags(ExtractGhaChannelModeFlags),
    /// `total <= 0.0`: the native code leaves both stack flag words
    /// unwritten for this channel.
    Silent,
}

/// Native flag decision for one channel's per-band energies. The energy
/// total accumulates in band order (matching the scan loop), the sort is
/// the strict descending Shell sort, and the comparisons follow the
/// decompile exactly: `e[second] + e[first] <= total * 0.99999` selects
/// the top-2-dominant flag; otherwise `e[last] * 4.0 <= e[first]`
/// separates the peaked case (fallback when `header_0xd0_enabled` is
/// false, both flags zero when true) from the flat case.
pub fn extract_ghwave_channel_mode_flags_at5(
    energies: &[f32],
    band_count: usize,
    header_0xd0_enabled: bool,
) -> Result<GhaChannelModeDecision, GhaExtractError> {
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if energies.len() < band_count {
        return Err(GhaExtractError::EnergyTableTooShort {
            channel: 0,
            needed: band_count,
            actual: energies.len(),
        });
    }
    let mut total = 0.0f32;
    for energy in &energies[..band_count] {
        total += *energy;
    }
    if !(0.0 < total) {
        return Ok(GhaChannelModeDecision::Silent);
    }

    let mut sorted = energies[..band_count].to_vec();
    let mut order: Vec<usize> = (0..band_count).collect();
    shell_sort_energy_descending_at5(&mut sorted, &mut order);
    let first = energies[order[0]];
    let second = energies[order[1.min(band_count - 1)]];
    let last = energies[order[band_count - 1]];

    if second + first <= total * 0.99999 {
        if last * 4.0 <= first {
            if !header_0xd0_enabled {
                Ok(GhaChannelModeDecision::DisabledFallback)
            } else {
                Ok(GhaChannelModeDecision::Flags(ExtractGhaChannelModeFlags {
                    initial_mode: 3,
                    flag_0x28: false,
                    flag_0x30: false,
                }))
            }
        } else {
            Ok(GhaChannelModeDecision::Flags(ExtractGhaChannelModeFlags {
                initial_mode: 3,
                flag_0x28: true,
                flag_0x30: false,
            }))
        }
    } else {
        Ok(GhaChannelModeDecision::Flags(ExtractGhaChannelModeFlags {
            initial_mode: 3,
            flag_0x28: false,
            flag_0x30: true,
        }))
    }
}

/// Native stereo shared/opposite gates (decompile line 41722): when both
/// channels' previous-frame rows agree on words 4..=7, the correlation
/// dB for the band selects the gate pair written to the secondary
/// struct words `+0x318` (shared) and `+0x360` (opposite):
/// `db >= 20.0` gives (1, 0), `-11.0 <= db < 20.0` gives (0, 0), and
/// `db < -11.0` (or NaN) gives (1, 1). Rows that differ give (0, 0).
/// Mono clears both rows.
pub type PreviousGhaRows<'a> = (
    &'a [[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]],
    &'a [[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]],
);

pub fn extract_ghwave_stereo_share_gates_at5(
    correlation_db: &[f32],
    previous_rows: Option<PreviousGhaRows<'_>>,
    band_count: usize,
) -> Result<(Vec<u32>, Vec<u32>), GhaExtractError> {
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    let Some((rows_a, rows_b)) = previous_rows else {
        return Ok((vec![0; band_count], vec![0; band_count]));
    };
    if rows_a.len() < band_count || rows_b.len() < band_count {
        return Err(GhaExtractError::RowsTooShort {
            needed: band_count,
            actual: rows_a.len().min(rows_b.len()),
        });
    }
    if correlation_db.len() < band_count {
        return Err(GhaExtractError::SharedFlagsTooShort {
            needed: band_count,
            actual: correlation_db.len(),
        });
    }

    let mut shared = vec![0u32; band_count];
    let mut opposite = vec![0u32; band_count];
    for band in 0..band_count {
        let equal = (4..=7).all(|word| rows_a[band][word] == rows_b[band][word]);
        if equal {
            let db = correlation_db[band];
            if 20.0 <= db {
                shared[band] = 1;
            } else if -11.0 <= db {
                // both stay zero
            } else {
                shared[band] = 1;
                opposite[band] = 1;
            }
        }
    }
    Ok((shared, opposite))
}

/// Native scan-profile selection (decompile line 41765): bit 1 of the
/// shifted header flag byte (`header + 0x1dc`) picks the DFT peak scan
/// threshold and group budget — clear gives `(30.0, 8)`, set gives
/// `(6.0, 0x10)`.
pub fn extract_ghwave_scan_profile_at5(header_flag_word: u32) -> (f32, usize) {
    if header_flag_word & 2 == 0 {
        (30.0, 8)
    } else {
        (6.0, 0x10)
    }
}

/// One band's DFT peak scan in the `extract_ghwave_at5` mode-flag
/// detection loops (decompile line 41797): `dft_x_at5` over the 256
/// sample window yields 129 magnitude bins; the peak is the strictly
/// positive argmax (native `-1` when every bin is `<= 0`), the ratio is
/// `max * 129.0 / sum` (left at 0 only when the bin sum is exactly
/// zero), and the magnitude is the peak bin's value (0 without a peak).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhaBandPeakScan {
    pub peak_index: Option<usize>,
    pub ratio: f32,
    pub peak_magnitude: f32,
}

fn peak_scan_from_bins(bins: &[f32; 129]) -> GhaBandPeakScan {
    let mut peak_index = None;
    let mut best = 0.0f32;
    for (index, &value) in bins.iter().enumerate() {
        if best < value {
            peak_index = Some(index);
            best = value;
        }
    }

    let mut max = bins[0];
    let mut sum = bins[0];
    for &value in &bins[1..] {
        if max < value {
            max = value;
        }
        sum += value;
    }
    let ratio = if sum == 0.0 { 0.0 } else { (max * 129.0) / sum };
    let peak_magnitude = peak_index.map_or(0.0, |index| bins[index]);
    GhaBandPeakScan {
        peak_index,
        ratio,
        peak_magnitude,
    }
}

pub fn extract_ghwave_band_peak_scan_at5(
    samples: &[f32],
) -> Result<GhaBandPeakScan, GhaExtractError> {
    if samples.len() < COMPONENT_SAMPLES {
        return Err(GhaExtractError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: samples.len(),
        });
    }
    let mut bins = [0.0f32; 129];
    let ip_table = crate::tables::at5::ip256_at5();
    let sc_table = crate::tables::at5::sc256_at5();
    crate::dsp::fft::dft_x_at5(
        &samples[..COMPONENT_SAMPLES],
        COMPONENT_SAMPLES,
        &mut bins,
        &ip_table,
        &sc_table,
    )
    .map_err(|_| GhaExtractError::InputTooShort {
        needed: COMPONENT_SAMPLES,
        actual: samples.len(),
    })?;
    Ok(peak_scan_from_bins(&bins))
}

/// Shared-band variant (native `mix_seq_at5`/`invmix_seq_at5` +
/// `dft_x_at5` at decompile line 41843): the two channel windows are
/// mixed (or inverse-mixed when the opposite flag is set) before the
/// scan; the native code stores the result into both channels' tables.
pub fn extract_ghwave_mixed_band_peak_scan_at5(
    channel_a: &[f32],
    channel_b: &[f32],
    invert: bool,
) -> Result<GhaBandPeakScan, GhaExtractError> {
    if channel_a.len() < COMPONENT_SAMPLES || channel_b.len() < COMPONENT_SAMPLES {
        return Err(GhaExtractError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: channel_a.len().min(channel_b.len()),
        });
    }
    let mut mixed = [0.0f32; COMPONENT_SAMPLES];
    let result = if invert {
        crate::dsp::scalar::invmix_seq_at5(channel_a, channel_b, &mut mixed, COMPONENT_SAMPLES)
    } else {
        crate::dsp::scalar::mix_seq_at5(channel_a, channel_b, &mut mixed, COMPONENT_SAMPLES)
    };
    result.map_err(|_| GhaExtractError::InputTooShort {
        needed: COMPONENT_SAMPLES,
        actual: channel_a.len().min(channel_b.len()),
    })?;
    extract_ghwave_band_peak_scan_at5(&mixed)
}

/// Final per-channel mode flags plus the scan side tables produced by
/// the detection loops.
#[derive(Debug, Clone, PartialEq)]
pub struct GhaModeDetectionOutcome {
    /// Final flag words per channel; `initial_mode` is 0 when the native
    /// code cleared the `+0x18` word (low energy or a `-1` peak).
    pub channel_flags: Vec<ExtractGhaChannelModeFlags>,
    /// The `local_29c` peak-index tables (native `-1` initialization
    /// maps to `None`).
    pub peak_indices: Vec<Vec<Option<usize>>>,
    /// Which bands were scanned (`aiStack_7dc`).
    pub scanned: Vec<Vec<bool>>,
    /// Which bands used the mixed sequence (`auStack_79c`).
    pub mixed_used: Vec<bool>,
}

/// The `extract_ghwave_at5` mode-flag detection loops (decompile line
/// 41780): per channel, when the energy total reaches 1.0 and a flag
/// from the initial decision survives, the bands are visited in that
/// channel's descending energy order and lazily scanned (own window, or
/// the mixed window when the stereo shared gate is set — the mixed scan
/// fills both channels' tables). The dominant flag (`flag_0x30`) needs
/// the first two bands' ratios above the profile threshold and, when
/// header flag bit 1 is clear, the first two peak indices within the
/// profile budget of the strongest band's peak. The flat flag
/// (`flag_0x28`) needs every band's ratio above the threshold and its
/// peak magnitude within 24 dB of the strongest band's. A `-1` peak
/// clears the surviving flag and the channel's `initial_mode`; totals
/// below 1.0 clear everything.
#[allow(clippy::too_many_arguments)]
pub fn extract_ghwave_mode_detection_at5(
    band_windows: &[Vec<&[f32]>],
    energies: &[&[f32]],
    decisions: &[GhaChannelModeDecision],
    shared: &[u32],
    opposite: &[u32],
    header_flag_word: u32,
    band_count: usize,
    channel_count: usize,
) -> Result<GhaModeDetectionOutcome, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if band_windows.len() < channel_count
        || energies.len() < channel_count
        || decisions.len() < channel_count
        || shared.len() < band_count
        || opposite.len() < band_count
        || band_windows
            .iter()
            .take(channel_count)
            .any(|windows| windows.len() < band_count)
        || energies
            .iter()
            .take(channel_count)
            .any(|table| table.len() < band_count)
    {
        return Err(GhaExtractError::EnergyTableTooShort {
            channel: 0,
            needed: band_count,
            actual: 0,
        });
    }

    let (threshold, peak_budget) = extract_ghwave_scan_profile_at5(header_flag_word);
    let distance_gate_enabled = header_flag_word & 2 == 0;

    let mut channel_flags = vec![
        ExtractGhaChannelModeFlags {
            initial_mode: 3,
            flag_0x28: false,
            flag_0x30: false,
        };
        channel_count
    ];
    let mut peak_indices = vec![vec![None; band_count]; channel_count];
    let mut scanned = vec![vec![false; band_count]; channel_count];
    let mut mixed_used = vec![false; band_count];
    let mut scans = vec![
        vec![
            GhaBandPeakScan {
                peak_index: None,
                ratio: 0.0,
                peak_magnitude: 0.0,
            };
            band_count
        ];
        channel_count
    ];

    for channel in 0..channel_count {
        // The initial decision's flag words; Silent leaves them unset
        // and the low-energy gate below clears everything anyway.
        let mut flags = match decisions[channel] {
            GhaChannelModeDecision::Flags(flags) => flags,
            GhaChannelModeDecision::Silent => ExtractGhaChannelModeFlags {
                initial_mode: 3,
                flag_0x28: false,
                flag_0x30: false,
            },
            GhaChannelModeDecision::DisabledFallback => {
                return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
            }
        };

        let mut total = 0.0f32;
        for energy in &energies[channel][..band_count] {
            total += *energy;
        }
        // Per-channel descending energy order (the scan-order table).
        let mut sorted = energies[channel][..band_count].to_vec();
        let mut order: Vec<usize> = (0..band_count).collect();
        shell_sort_energy_descending_at5(&mut sorted, &mut order);

        if !(1.0 <= total) {
            flags.flag_0x28 = false;
            flags.flag_0x30 = false;
            flags.initial_mode = 0;
            channel_flags[channel] = flags;
            continue;
        }

        let dominant = flags.flag_0x30;
        let flat = !dominant && flags.flag_0x28;
        if dominant || flat {
            let mut visited = 0usize;
            while visited < band_count {
                let band = order[visited];
                if !scanned[channel][band] {
                    if shared[band] == 0 || channel_count < 2 {
                        let scan = extract_ghwave_band_peak_scan_at5(band_windows[channel][band])?;
                        peak_indices[channel][band] = scan.peak_index;
                        scans[channel][band] = scan;
                        scanned[channel][band] = true;
                    } else {
                        let scan = extract_ghwave_mixed_band_peak_scan_at5(
                            band_windows[0][band],
                            band_windows[1][band],
                            opposite[band] != 0,
                        )?;
                        for other in 0..channel_count {
                            peak_indices[other][band] = scan.peak_index;
                            scans[other][band] = scan;
                            scanned[other][band] = true;
                        }
                        mixed_used[band] = true;
                    }
                }

                let scan = scans[channel][band];
                if peak_indices[channel][band].is_none() {
                    if dominant {
                        flags.flag_0x30 = false;
                    } else {
                        flags.flag_0x28 = false;
                    }
                    flags.initial_mode = 0;
                    break;
                }
                if dominant {
                    if visited > 1 || threshold < scan.ratio {
                        if distance_gate_enabled && visited < 2 {
                            let strongest = order[0];
                            let peak = peak_indices[channel][band].unwrap() as i32;
                            let strongest_peak =
                                peak_indices[channel][strongest].unwrap_or(usize::MAX) as i32;
                            if (peak - strongest_peak).abs() > peak_budget as i32 {
                                flags.flag_0x30 = false;
                            }
                        }
                        visited += 1;
                    } else {
                        flags.flag_0x30 = false;
                    }
                    if !flags.flag_0x30 {
                        break;
                    }
                } else {
                    if threshold < scan.ratio {
                        let strongest = order[0];
                        let strongest_magnitude = scans[channel][strongest].peak_magnitude;
                        let quotient = scan.peak_magnitude / strongest_magnitude;
                        let db = if 0.0 < quotient {
                            (f64::from(quotient).ln() as f32) * 8.685889f32
                        } else {
                            -160.0f32
                        };
                        if 24.0 < f64::from(db).abs() {
                            flags.flag_0x28 = false;
                        }
                        visited += 1;
                    } else {
                        flags.flag_0x28 = false;
                    }
                    if !flags.flag_0x28 {
                        break;
                    }
                }
            }
        }
        channel_flags[channel] = flags;
    }

    Ok(GhaModeDetectionOutcome {
        channel_flags,
        peak_indices,
        scanned,
        mixed_used,
    })
}

/// Disabled-`0xd0` fallback outcome: the residual synthesis pairs to
/// run and the cleared header active word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaDisabledFallbackPlan {
    pub calls: Vec<ExtractGhaResidualSynthesisCall>,
    /// The native writes `*header = 0` before returning.
    pub header_active_word: u32,
}

/// Disabled-`0xd0` fallback path (decompile `LAB_0005cb5c`, line 42183):
/// when the header enable word `+0xd0` is zero and the mode mask is not
/// 1 or 2, `extract_ghwave_at5` skips analysis entirely. Per band and
/// channel, a previous row with waves triggers the same
/// previous-then-current residual synthesis, crossfade windowing, and
/// `sub_seq_at5` subtraction as the post-writeback loop
/// (`extract_ghwave_residual_at5` executes each pair; the current rows
/// were init-cleared this frame, so their synthesis contributes
/// silence), and the current header's active word is cleared before the
/// early return.
pub fn extract_ghwave_disabled_fallback_plan_at5(
    previous_rows: &[&[ExtractGhaRow]],
    band_count: usize,
) -> Result<GhaDisabledFallbackPlan, GhaExtractError> {
    let channel_count = previous_rows.len();
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    for rows in previous_rows {
        if rows.len() < band_count {
            return Err(GhaExtractError::RowsTooShort {
                needed: band_count,
                actual: rows.len(),
            });
        }
    }

    let mut calls = Vec::new();
    for band_index in 0..band_count {
        for channel_index in 0..channel_count {
            if previous_rows[channel_index][band_index].nwavs > 0 {
                calls.push(ExtractGhaResidualSynthesisCall {
                    band_index,
                    channel_index,
                    source: ExtractGhaResidualSource::Previous,
                });
                calls.push(ExtractGhaResidualSynthesisCall {
                    band_index,
                    channel_index,
                    source: ExtractGhaResidualSource::Current,
                });
            }
        }
    }

    Ok(GhaDisabledFallbackPlan {
        calls,
        header_active_word: 0,
    })
}

/// Per-channel wave-count allocation for the sine dispatch path
/// (decompile `LAB_0005cfc7`, line 42322).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaSineWaveAllocation {
    /// `local_1b4c` count tables, `counts[channel][band]`.
    pub counts: Vec<Vec<i32>>,
    /// The remaining `local_1d64` budget after the per-band decrements.
    pub remaining_budget: i32,
}

/// Native sine-path wave budget: `param_3 < 0xb` gives 0, `0xb..=0xc`
/// gives 0xc, larger gives 0x30.
pub fn extract_ghwave_sine_wave_budget_at5(param_3: i32) -> i32 {
    if param_3 < 0xb {
        0
    } else if param_3 < 0xd {
        0xc
    } else {
        0x30
    }
}

/// The sine dispatch wave-count allocation (`extract_ghwave_at5` for
/// header modes 1/2, decompile line 42270): per band below the header
/// band count, the channel energies sum and convert to dB when at
/// least 1.0 (`ln * 8.685889`, else 0); a non-positive dB total zeroes
/// every count. Otherwise each band's raw count is
/// `floorf(budget * db / total + 0.5)` clamped up to 1, and the bands
/// are visited in the selected energy order: the count clamps to the
/// remaining budget, splits across channels (mono: all to channel 0;
/// stereo unshared: `ch0 = c - (c >> 1)`, `ch1 = clamped - ch0`;
/// shared: all to channel 0), then the mode clamp applies — mode 1
/// uses `param_3`-dependent limits (`< 0xd`: rank budget 3, limit 3,
/// sub-limit 0; `< 0xf`: rank budget 3, limit 8, sub-limit 0; else
/// rank budget 0x10, limit 0xf, sub-limit 0xf), with counts below 2
/// clamped to the sub-limit inside the rank budget and everything
/// clamped to the sub-limit beyond it; mode 2 clamps every count to 3.
/// The budget decrements by the allocated channel counts per band.
pub fn extract_ghwave_sine_wave_allocation_at5(
    energies: &[&[f32]],
    selected_order: &[usize],
    shared: &[u32],
    mode_mask: u32,
    param_3: i32,
    channel_count: usize,
    header_band_count: usize,
) -> Result<GhaSineWaveAllocation, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if header_band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount {
            band_count: header_band_count,
        });
    }
    if !(1..=2).contains(&mode_mask) {
        return Err(GhaExtractError::UnsupportedBandCount {
            band_count: mode_mask as usize,
        });
    }
    if selected_order.len() < header_band_count
        || shared.len() < MAX_EXTRACT_BANDS_AT5.min(header_band_count)
        || energies.len() < channel_count
        || energies
            .iter()
            .take(channel_count)
            .any(|table| table.len() < header_band_count)
        || selected_order[..header_band_count]
            .iter()
            .any(|band| *band >= header_band_count)
    {
        return Err(GhaExtractError::SelectedBandOrderTooShort {
            needed: header_band_count,
            actual: selected_order.len(),
        });
    }

    let mut budget = extract_ghwave_sine_wave_budget_at5(param_3);
    let mut counts = vec![vec![0i32; header_band_count.max(1)]; channel_count];

    // Summed energy -> per-band dB (only bands at or above 1.0).
    let mut db = vec![0.0f32; header_band_count];
    let mut total = 0.0f32;
    for band in 0..header_band_count {
        let mut energy = 0.0f32;
        for table in energies.iter().take(channel_count) {
            energy += table[band];
        }
        if 1.0 <= energy {
            db[band] = if 0.0 < energy {
                (f64::from(energy).ln() as f32) * 8.685889f32
            } else {
                -160.0f32
            };
        }
        total += db[band];
    }

    if total <= 0.0 {
        return Ok(GhaSineWaveAllocation {
            counts,
            remaining_budget: budget,
        });
    }

    let mut raw = vec![0i32; header_band_count];
    for band in 0..header_band_count {
        let rounded = ((budget as f32) * db[band] / total + 0.5).floor();
        raw[band] = if (rounded as i32) < 1 {
            1
        } else {
            rounded as i32
        };
    }

    for (rank, &band) in selected_order[..header_band_count].iter().enumerate() {
        let mut clamped = raw[band];
        if budget < clamped {
            raw[band] = budget;
            clamped = budget;
        }
        if channel_count == 1 {
            counts[0][band] = clamped;
        } else if shared[band] == 0 {
            let first = clamped - (clamped >> 1);
            counts[0][band] = first;
            counts[1][band] = raw[band] - first;
        } else {
            counts[0][band] = clamped;
            counts[1][band] = 0;
        }

        let mut decrement = true;
        if mode_mask == 1 {
            let (rank_budget, limit, sub_limit) = if param_3 < 0xd {
                (3, 3, 0)
            } else if param_3 < 0xf {
                (3, 8, 0)
            } else {
                (0x10, 0xf, 0xf)
            };
            if rank < rank_budget {
                for channel_counts in counts.iter_mut().take(channel_count) {
                    let value = channel_counts[band];
                    let bound = if value < 2 { sub_limit } else { limit };
                    if bound < value {
                        channel_counts[band] = bound;
                    }
                }
            } else {
                for channel_counts in counts.iter_mut().take(channel_count) {
                    if sub_limit < channel_counts[band] {
                        channel_counts[band] = sub_limit;
                    }
                }
            }
        } else {
            // mode_mask == 2: every count clamps to 3; the native skips
            // the budget decrement entirely when there are no channels,
            // which cannot happen here.
            for channel_counts in counts.iter_mut().take(channel_count) {
                if 3 < channel_counts[band] {
                    channel_counts[band] = 3;
                }
            }
            decrement = true;
        }
        if decrement {
            for channel_counts in counts.iter().take(channel_count) {
                budget -= channel_counts[band];
            }
        }
    }

    Ok(GhaSineWaveAllocation {
        counts,
        remaining_budget: budget,
    })
}

/// Row words 0..3 computed by the sine dispatch region setup
/// (decompile `LAB_0005d220`, line 42450): the analysis window inside
/// the 256-sample band window plus the start/end presence flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhaSineRegionWords {
    /// Row word 0.
    pub start_flag: u32,
    /// Row word 1.
    pub end_flag: u32,
    /// Row word 2: analysis start sample.
    pub start: i32,
    /// Row word 3: analysis end sample (`+4`, capped at 0x100).
    pub end: i32,
}

/// Native region-word setup from the current and previous row words
/// 4..=7 (gain-window presence flags and locations): the start comes
/// from the current row's word 6 (`* 4 + 0x80`) when its word-4 flag is
/// set and word 7 exceeds word 6, else from the previous row's word 6
/// (`<< 2`) when its word-4 flag is set, else 0; the end comes from the
/// previous row's word 7 (`* 4`) when its word-5 flag is set and that
/// end does not precede the start, else from the current row's word 7
/// (`* 4 + 0x80`) when its word-5 flag is set, else 0x100. The end then
/// advances by 4, capped at 0x100.
pub fn extract_ghwave_sine_region_words_at5(
    current_words: &[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5],
    previous_words: &[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5],
) -> GhaSineRegionWords {
    let (start, start_flag) =
        if current_words[4] == 0 || current_words[7] as i32 <= current_words[6] as i32 {
            if previous_words[4] != 0 {
                (((previous_words[6] as i32) << 2), 1)
            } else {
                (0, 0)
            }
        } else {
            (current_words[6] as i32 * 4 + 0x80, 1)
        };

    let previous_end = previous_words[7] as i32 * 4;
    let (mut end, end_flag) = if previous_words[5] == 0 || previous_end < start {
        if current_words[5] != 0 {
            (current_words[7] as i32 * 4 + 0x80, 1)
        } else {
            (0x100, 0)
        }
    } else {
        (previous_end, 1)
    };

    if end + 4 < 0x101 {
        end += 4;
    } else {
        end = 0x100;
    }

    GhaSineRegionWords {
        start_flag,
        end_flag,
        start,
        end,
    }
}

/// The sine dispatch initial-bin scan (decompile line 42497, gated on
/// the header `+0xd0` enable): the band window's `start..end` region is
/// copied into a zeroed 256-sample buffer, `dft_x_at5` produces 129
/// magnitude bins, and the strictly positive argmax becomes the coarse
/// bin stored in the channel's peak table (native `-1` → `None`). This
/// is the unweighted form of the already-ported
/// `analysis_general_initial_bin_at5`.
pub fn extract_ghwave_sine_initial_bin_at5(
    samples: &[f32],
    start: usize,
    end: usize,
) -> Result<Option<usize>, GhaExtractError> {
    let weights = [1.0f32; 129];
    crate::gha::analysis::analysis_general_initial_bin_at5(samples, start, end, &weights).map_err(
        |_| GhaExtractError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: samples.len(),
        },
    )
}

/// One sine dispatch call's outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct GhaSineDispatchBandResult {
    pub band_index: usize,
    pub channel_index: usize,
    pub mixed: bool,
    pub region: GhaSineRegionWords,
    pub initial_bin: Option<usize>,
    /// Record-arena slot of this row's first wave (native row word 9 is
    /// `header + 0xc + 0x10 *` this index).
    pub record_start_index: usize,
    pub wave_count: usize,
}

/// The executed sine dispatch (decompile line 42430, header modes
/// 1/2).
#[derive(Debug, Clone, PartialEq)]
pub struct GhaSineDispatchOutcome {
    pub calls: Vec<GhaSineDispatchBandResult>,
    /// Updated current row words per channel and band (words 0..3
    /// region, word 8 wave count, word 9 record-arena pointer
    /// `record_arena_header + 0xc + 0x10*record_start_index`; shared
    /// bands copy channel 0's full row to channel 1).
    pub rows: Vec<Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>>,
    /// Records appended in arena order.
    pub records: Vec<GhaWaveRecord>,
    /// Updated coarse peak tables (`local_29c`).
    pub peak_indices: Vec<Vec<Option<usize>>>,
    pub cumulative_waves: usize,
}

/// Executes the sine dispatch for header modes 1/2 (decompile line
/// 42430): bands are visited in the selected energy order; unshared
/// bands run per channel with a positive allocated count, shared bands
/// mix (or inverse-mix) the two windows once through channel 0. Each
/// call derives the analysis region from the current and previous row
/// words 4..=7, rescans the coarse bin over the zero-padded region when
/// the `+0xd0` enable is set (updating the peak table), plants the
/// record-arena POINTER in row word 9 (`record_arena_header + 0xc +
/// 0x10*cumulative`, wrapping i32 — decompile 42530/42622, matching the
/// general path `analysis.rs:698`) for every VISITED band BEFORE the
/// sub, runs `analysis_sine_at5_sub` with the allocated count, and
/// stores the extracted wave count in row word 8. Shared bands then copy
/// the full 10-word row to channel 1 (both rows alias the same record
/// pointer; decompile 42625–42641).
#[allow(clippy::too_many_arguments)]
pub fn extract_ghwave_sine_dispatch_at5(
    band_windows: &[Vec<&[f32]>],
    selected_order: &[usize],
    counts: &[Vec<i32>],
    shared: &[u32],
    opposite: &[u32],
    current_rows: &[Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    previous_rows: &[Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    peak_indices: &[Vec<Option<usize>>],
    header_0xd0_enabled: bool,
    channel_count: usize,
    header_band_count: usize,
    record_arena_header: i32,
) -> Result<GhaSineDispatchOutcome, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if header_band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount {
            band_count: header_band_count,
        });
    }
    if selected_order.len() < header_band_count
        || shared.len() < header_band_count
        || opposite.len() < header_band_count
        || band_windows.len() < channel_count
        || counts.len() < channel_count
        || current_rows.len() < channel_count
        || previous_rows.len() < channel_count
        || peak_indices.len() < channel_count
    {
        return Err(GhaExtractError::SelectedBandOrderTooShort {
            needed: header_band_count,
            actual: selected_order.len(),
        });
    }
    for channel in 0..channel_count {
        if band_windows[channel].len() < header_band_count
            || counts[channel].len() < header_band_count
            || current_rows[channel].len() < header_band_count
            || previous_rows[channel].len() < header_band_count
            || peak_indices[channel].len() < header_band_count
        {
            return Err(GhaExtractError::RowsTooShort {
                needed: header_band_count,
                actual: current_rows[channel].len(),
            });
        }
    }

    let mut rows: Vec<Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>> = current_rows
        .iter()
        .take(channel_count)
        .map(|channel_rows| channel_rows[..header_band_count].to_vec())
        .collect();
    let mut peaks: Vec<Vec<Option<usize>>> = peak_indices
        .iter()
        .take(channel_count)
        .map(|table| table[..header_band_count].to_vec())
        .collect();
    let mut records = Vec::new();
    let mut calls = Vec::new();
    let mut cumulative = 0usize;

    let run_call = |channel: usize,
                    band: usize,
                    samples: &[f32],
                    mixed: bool,
                    rows: &mut Vec<Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>>,
                    peaks: &mut Vec<Vec<Option<usize>>>,
                    records: &mut Vec<GhaWaveRecord>,
                    cumulative: &mut usize,
                    count: i32|
     -> Result<GhaSineDispatchBandResult, GhaExtractError> {
        let region = extract_ghwave_sine_region_words_at5(
            &rows[channel][band],
            &previous_rows[channel][band],
        );
        rows[channel][band][0] = region.start_flag;
        rows[channel][band][1] = region.end_flag;
        rows[channel][band][2] = region.start as u32;
        rows[channel][band][3] = region.end as u32;

        let start = region.start.max(0) as usize;
        let end = (region.end.max(0) as usize).min(COMPONENT_SAMPLES);
        let initial_bin = if header_0xd0_enabled {
            let bin = extract_ghwave_sine_initial_bin_at5(samples, start, end.max(start))?;
            peaks[channel][band] = bin;
            bin
        } else {
            // `+0xd0 == 0`: the dispatch-time peak scan (decompile 42497 unshared
            // / 42588 shared, both gated on `*(cfg+0xd0) != 0`, windowed to
            // start..end) is skipped. The peak table instead carries the value the
            // front detection seeded UNCONDITIONALLY: a FULL-WINDOW DFT argmax over
            // the whole 0x100 band window (decompile 41863 shared-mixed / 41949
            // unshared, `dft_x_at5(window, 0x100, ...)` with NO start..end mask),
            // strongest strictly-positive bin. That full-window bin — not the
            // start..end windowed one — is what `analysis_sine_at5_sub` gets as its
            // initial coarse bin under disabled, which is why the sine arm stays
            // live under `+0xd0 == 0` (docs/13 §5.1 evidence 3).
            let bin = extract_ghwave_sine_initial_bin_at5(samples, 0, COMPONENT_SAMPLES)?;
            peaks[channel][band] = bin;
            bin
        };

        let record_start_index = *cumulative;
        // Native plants the arena POINTER (not a plain record index) in row
        // word 9, for every visited band, BEFORE the sub runs (decompile
        // 42530 unshared / 42622 shared: `local_1d4c*0x10 + 0xc + iVar15`).
        // Wrapping i32 arithmetic mirrors the general path (analysis.rs:698).
        let record_pointer_word = record_arena_header
            .wrapping_add(0xc)
            .wrapping_add((record_start_index as i32).wrapping_mul(0x10));
        rows[channel][band][9] = record_pointer_word as u32;

        let max_waves = count.max(0) as usize;
        let mut call_records = vec![
            GhaWaveRecord {
                scale_index: 0,
                amplitude_index: 0,
                phase_index: 0,
                frequency: 0,
            };
            max_waves.min(16)
        ];
        let wave_count = crate::gha::analysis::analysis_sine_at5_sub(
            samples,
            &mut call_records,
            start,
            end.max(start),
            initial_bin,
            max_waves.min(16),
        )
        .map_err(|_| GhaExtractError::InputTooShort {
            needed: COMPONENT_SAMPLES,
            actual: samples.len(),
        })?;
        rows[channel][band][8] = wave_count as u32;
        records.extend_from_slice(&call_records[..wave_count]);
        *cumulative += wave_count;

        Ok(GhaSineDispatchBandResult {
            band_index: band,
            channel_index: channel,
            mixed,
            region,
            initial_bin,
            record_start_index,
            wave_count,
        })
    };

    for &band in &selected_order[..header_band_count] {
        if shared[band] == 0 || channel_count < 2 {
            for channel in 0..channel_count {
                if counts[channel][band] > 0 {
                    let call = run_call(
                        channel,
                        band,
                        band_windows[channel][band],
                        false,
                        &mut rows,
                        &mut peaks,
                        &mut records,
                        &mut cumulative,
                        counts[channel][band],
                    )?;
                    calls.push(call);
                }
            }
        } else if counts[0][band] > 0 {
            let mut mixed = [0.0f32; COMPONENT_SAMPLES];
            let mix_result = if opposite[band] != 0 {
                crate::dsp::scalar::invmix_seq_at5(
                    band_windows[0][band],
                    band_windows[1][band],
                    &mut mixed,
                    COMPONENT_SAMPLES,
                )
            } else {
                crate::dsp::scalar::mix_seq_at5(
                    band_windows[0][band],
                    band_windows[1][band],
                    &mut mixed,
                    COMPONENT_SAMPLES,
                )
            };
            mix_result.map_err(|_| GhaExtractError::InputTooShort {
                needed: COMPONENT_SAMPLES,
                actual: band_windows[0][band].len().min(band_windows[1][band].len()),
            })?;
            let call = run_call(
                0,
                band,
                &mixed,
                true,
                &mut rows,
                &mut peaks,
                &mut records,
                &mut cumulative,
                counts[0][band],
            )?;
            // The native copies the full 10-word row to channel 1; both
            // rows reference the same record pointer.
            rows[1][band] = rows[0][band];
            calls.push(call);
        }
    }

    Ok(GhaSineDispatchOutcome {
        calls,
        rows,
        records,
        peak_indices: peaks,
        cumulative_waves: cumulative,
    })
}

/// Dispatch directive chosen by the front half of `extract_ghwave_at5`.
#[derive(Debug, Clone, PartialEq)]
pub enum GhaExtractDispatch {
    /// The low-bitrate fast path (`param_3 < 0xc` stereo or `< 10`
    /// mono): mode mask 3 with header words `[.., 0, min(bands, 1)]`,
    /// straight to the general dispatch.
    FastPath,
    /// Header `+0xd0` disabled with a general/empty mask: residual-only
    /// fallback, header active word cleared, early return.
    DisabledFallback(GhaDisabledFallbackPlan),
    /// Mode mask 3 with the enable set: the compact general scheduler
    /// runs over the selected order.
    General,
    /// Mode mask 0 with the enable set: no analysis; control falls to
    /// the post-dispatch wave-limit/residual/writeback stages.
    NoAnalysis,
    /// Mode mask 1 or 2: the sine dispatch runs with this allocation.
    Sine(GhaSineWaveAllocation),
}

/// Everything the front half computes before the dispatch executes.
#[derive(Debug, Clone, PartialEq)]
pub struct GhaExtractFrontOutcome {
    pub dispatch: GhaExtractDispatch,
    pub header: ExtractGhaHeader,
    pub energies: Vec<Vec<f32>>,
    pub shared: Vec<u32>,
    pub opposite: Vec<u32>,
    pub selected_order: Vec<usize>,
    pub detection: Option<GhaModeDetectionOutcome>,
}

/// The front half of `extract_ghwave_at5` (decompile line 41560): the
/// low-bitrate fast path, the per-channel energy scan, the mode-flag
/// decision (with the disabled-`+0xd0` early fallback), the stereo
/// share gates over the band-window correlation, the DFT detection
/// loops, the header decision, the selected-band compaction, and the
/// dispatch selection with the sine wave allocation when the mask is 1
/// or 2. Row initialization, dispatch execution, wave limit, residual
/// synthesis, and header writeback remain the caller's ported stages.
#[allow(clippy::too_many_arguments)]
pub fn extract_ghwave_front_at5(
    band_windows: &[Vec<&[f32]>],
    previous_rows: &[Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    param_3: i32,
    band_count: usize,
    channel_count: usize,
    header_flag_word: u32,
    header_0xd0_enabled: bool,
) -> Result<GhaExtractFrontOutcome, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if band_windows.len() < channel_count || previous_rows.len() < channel_count {
        return Err(GhaExtractError::RowsTooShort {
            needed: channel_count,
            actual: band_windows.len().min(previous_rows.len()),
        });
    }
    for channel in 0..channel_count {
        if band_windows[channel].len() < band_count || previous_rows[channel].len() < band_count {
            return Err(GhaExtractError::RowsTooShort {
                needed: band_count,
                actual: band_windows[channel]
                    .len()
                    .min(previous_rows[channel].len()),
            });
        }
    }

    // Low-bitrate fast path: mask 3, header words 1/2 = 0 / min(bands, 1).
    if (param_3 < 0xc && channel_count == 2) || (param_3 < 10 && channel_count == 1) {
        let header = ExtractGhaHeader {
            mode: 0,
            scheduler_group_count: band_count.min(1),
            mode_mask: 3,
            sets_global_mode_flag: false,
        };
        return Ok(GhaExtractFrontOutcome {
            dispatch: GhaExtractDispatch::FastPath,
            header,
            energies: Vec::new(),
            shared: vec![0; band_count],
            opposite: vec![0; band_count],
            selected_order: (0..header.scheduler_group_count).collect(),
            detection: None,
        });
    }

    // Per-channel energy scan.
    let mut energies = vec![vec![0.0f32; band_count]; channel_count];
    for channel in 0..channel_count {
        for band in 0..band_count {
            energies[channel][band] = extract_ghwave_band_energy_at5(band_windows[channel][band])?;
        }
    }

    // Per-channel mode-flag decision; the disabled fallback aborts to
    // the residual-only path with header words 0 / 1.
    let mut decisions = Vec::with_capacity(channel_count);
    for channel_energies in energies.iter().take(channel_count) {
        let decision = extract_ghwave_channel_mode_flags_at5(
            channel_energies,
            band_count,
            header_0xd0_enabled,
        )?;
        if decision == GhaChannelModeDecision::DisabledFallback {
            let previous_row_views: Vec<Vec<ExtractGhaRow>> = previous_rows
                .iter()
                .take(channel_count)
                .map(|rows| {
                    rows[..band_count]
                        .iter()
                        .map(|words| ExtractGhaRow {
                            active: words[0] != 0,
                            nwavs: words[8] as usize,
                        })
                        .collect()
                })
                .collect();
            let row_refs: Vec<&[ExtractGhaRow]> =
                previous_row_views.iter().map(Vec::as_slice).collect();
            let plan = extract_ghwave_disabled_fallback_plan_at5(&row_refs, band_count)?;
            let header = ExtractGhaHeader {
                mode: 0,
                scheduler_group_count: 1,
                mode_mask: 3,
                sets_global_mode_flag: false,
            };
            return Ok(GhaExtractFrontOutcome {
                dispatch: GhaExtractDispatch::DisabledFallback(plan),
                header,
                energies,
                shared: vec![0; band_count],
                opposite: vec![0; band_count],
                selected_order: Vec::new(),
                detection: None,
            });
        }
        decisions.push(decision);
    }

    // Stereo share gates from the band-window correlation.
    let (shared, opposite) = if channel_count == 2 {
        let a_windows: Vec<&[f32]> = band_windows[0][..band_count].to_vec();
        let b_windows: Vec<&[f32]> = band_windows[1][..band_count].to_vec();
        let correlation = crate::gha::power::check_channel_correlation_at5(
            &a_windows,
            &b_windows,
            COMPONENT_SAMPLES,
            band_count,
        )?;
        let rows_a: Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]> =
            previous_rows[0][..band_count].to_vec();
        let rows_b: Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]> =
            previous_rows[1][..band_count].to_vec();
        extract_ghwave_stereo_share_gates_at5(
            &correlation.db,
            Some((&rows_a, &rows_b)),
            band_count,
        )?
    } else {
        (vec![0; band_count], vec![0; band_count])
    };

    // Detection loops.
    let energy_refs: Vec<&[f32]> = energies.iter().map(Vec::as_slice).collect();
    let detection = extract_ghwave_mode_detection_at5(
        band_windows,
        &energy_refs,
        &decisions,
        &shared,
        &opposite,
        header_flag_word,
        band_count,
        channel_count,
    )?;

    // Header decision over the final flags, then band compaction.
    let header = extract_ghwave_header_at5(
        &detection.channel_flags,
        band_count,
        channel_count,
        param_3 as u32,
        header_0xd0_enabled,
    )?;
    let selected_order = extract_ghwave_selected_band_order_at5(
        &energy_refs,
        band_count,
        header.scheduler_group_count,
    )?;

    let dispatch = match header.mode_mask {
        1 | 2 => GhaExtractDispatch::Sine(extract_ghwave_sine_wave_allocation_at5(
            &energy_refs,
            &selected_order,
            &shared,
            header.mode_mask as u32,
            param_3,
            channel_count,
            header.scheduler_group_count,
        )?),
        3 if header_0xd0_enabled => GhaExtractDispatch::General,
        _ if header_0xd0_enabled => GhaExtractDispatch::NoAnalysis,
        _ => {
            let previous_row_views: Vec<Vec<ExtractGhaRow>> = previous_rows
                .iter()
                .take(channel_count)
                .map(|rows| {
                    rows[..band_count]
                        .iter()
                        .map(|words| ExtractGhaRow {
                            active: words[0] != 0,
                            nwavs: words[8] as usize,
                        })
                        .collect()
                })
                .collect();
            let row_refs: Vec<&[ExtractGhaRow]> =
                previous_row_views.iter().map(Vec::as_slice).collect();
            GhaExtractDispatch::DisabledFallback(extract_ghwave_disabled_fallback_plan_at5(
                &row_refs, band_count,
            )?)
        }
    };

    Ok(GhaExtractFrontOutcome {
        dispatch,
        header,
        energies,
        shared,
        opposite,
        selected_order,
        detection: Some(detection),
    })
}

/// Post-dispatch tail outcome of `extract_ghwave_at5`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaExtractTailOutcome {
    /// Whether the 0x30 wave limit tripped and the rows were re-initialized.
    pub overflow_cleared: bool,
    /// The effective wave total that was checked against the limit.
    pub effective_total_nwavs: usize,
    /// Residual synthesis pairs (either-row gate, band-major order).
    pub residual_plan: Vec<ExtractGhaResidualSynthesisCall>,
    /// Final channel-0 header words `[active, mode, band_count]`; the
    /// native sets the active word to 1 at the very end of the normal
    /// path.
    pub header_words: [u32; EXTRACT_GHA_HEADER_WORD_COUNT_AT5],
    /// On overflow, every channel's header takes the reset words
    /// (`active = 0`, `band_count = 1`) before channel 0's active word
    /// is set again by the final write.
    pub reset_channel_headers: bool,
}

/// The post-dispatch tail of `extract_ghwave_at5` (decompile
/// `LAB_0005d35e`, line 42566): the raw wave total accumulates over the
/// current rows (skipping secondary-channel rows of shared bands); a
/// total above 0x30 re-initializes every channel's rows (preserving
/// words 2, 3, and 9) and resets the per-channel header words; the
/// residual loop then plans a previous+current synthesis pair for every
/// row where either buffer has waves; finally channel 0's header takes
/// `[1, mode, band_count]`.
pub fn extract_ghwave_tail_at5(
    rows: &mut [Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    previous_rows: &[Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    shared: &[u32],
    header: ExtractGhaHeader,
    channel_count: usize,
    band_count: usize,
) -> Result<GhaExtractTailOutcome, GhaExtractError> {
    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if rows.len() < channel_count
        || previous_rows.len() < channel_count
        || shared.len() < band_count
        || rows
            .iter()
            .take(channel_count)
            .chain(previous_rows.iter().take(channel_count))
            .any(|channel_rows| channel_rows.len() < band_count)
    {
        return Err(GhaExtractError::RowsTooShort {
            needed: band_count,
            actual: rows.len().min(previous_rows.len()),
        });
    }

    let row_views: Vec<Vec<ExtractGhaRow>> = rows
        .iter()
        .take(channel_count)
        .map(|channel_rows| {
            channel_rows[..band_count]
                .iter()
                .map(|words| ExtractGhaRow {
                    active: words[0] != 0,
                    nwavs: words[8] as usize,
                })
                .collect()
        })
        .collect();
    let row_refs: Vec<&[ExtractGhaRow]> = row_views.iter().map(Vec::as_slice).collect();
    let shared_flags: Vec<bool> = shared[..band_count].iter().map(|flag| *flag != 0).collect();
    let limit = extract_ghwave_wave_limit_at5(&row_refs, band_count, &shared_flags)?;

    if limit.clears_rows {
        for channel_rows in rows.iter_mut().take(channel_count) {
            for row in channel_rows[..band_count].iter_mut() {
                extract_ghwave_clear_overflow_row_words_at5(std::slice::from_mut(row));
            }
        }
    }

    // Residual plan over the (possibly cleared) current rows and the
    // previous rows.
    let cleared_views: Vec<Vec<ExtractGhaRow>> = rows
        .iter()
        .take(channel_count)
        .map(|channel_rows| {
            channel_rows[..band_count]
                .iter()
                .map(|words| ExtractGhaRow {
                    active: words[0] != 0,
                    nwavs: words[8] as usize,
                })
                .collect()
        })
        .collect();
    let previous_views: Vec<Vec<ExtractGhaRow>> = previous_rows
        .iter()
        .take(channel_count)
        .map(|channel_rows| {
            channel_rows[..band_count]
                .iter()
                .map(|words| ExtractGhaRow {
                    active: words[0] != 0,
                    nwavs: words[8] as usize,
                })
                .collect()
        })
        .collect();
    let cleared_refs: Vec<&[ExtractGhaRow]> = cleared_views.iter().map(Vec::as_slice).collect();
    let previous_refs: Vec<&[ExtractGhaRow]> = previous_views.iter().map(Vec::as_slice).collect();
    let residual_plan =
        extract_ghwave_residual_synthesis_plan_at5(&cleared_refs, &previous_refs, band_count)?;

    let header_words = extract_ghwave_write_header_words_at5(header, true);

    Ok(GhaExtractTailOutcome {
        overflow_cleared: limit.clears_rows,
        effective_total_nwavs: limit.effective_total_nwavs,
        residual_plan,
        header_words,
        reset_channel_headers: limit.clears_rows,
    })
}

/// Which dispatch arm the whole-boundary driver executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhaExtractDispatchKind {
    General,
    Sine,
    NoAnalysis,
    DisabledFallback,
}

/// Whole-boundary inputs for the composed `extract_ghwave_at5` driver.
///
/// These are the surfaces the native boundary consumes (decompile line
/// 41560 onward): the `param_2` band-buffer matrix, the delayed
/// (`*(obj+0x20)+4`) state rows and their arena records, the header
/// enable/mode/flag scalars, and `param_3..param_6`. The fresh output
/// rows land in a driver-owned `*(obj+0x24)+4`-shaped block whose header
/// (`record_arena_header`) anchors the record pointers.
#[derive(Debug, Clone)]
pub struct GhaExtractInput {
    pub channel_count: usize,
    pub band_count: usize,
    /// `param_3` threshold scalar (`ecx`, 30 for the 352 kbps encode).
    pub param_3: i32,
    /// `param_6` profile selector (stack arg, 0 for the 352 kbps encode).
    pub profile_selector: usize,
    /// Config header `+0x1dc` word: bit 1 picks the scan profile.
    pub header_flag_word: u32,
    /// Config header `+0xd0` enable word (`!= 0`).
    pub header_0xd0_enabled: bool,
    /// `param_2[ch][band]`, each at least `0x180` f32 (the residual pass
    /// subtracts into `[0, 0x80)` in place; the returned buffers carry
    /// the residual).
    pub band_windows: Vec<Vec<Vec<f32>>>,
    /// Delayed state rows `*(obj+0x20)+4` `[ch][band]` (10 words): the
    /// delayed window words (4..7), share-gate comparison words, and the
    /// residual delayed-row source.
    pub delayed_rows: Vec<Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>>,
    /// Delayed-row arena records `[ch][band]` (previous frame's waves)
    /// used by the residual delayed synthesis.
    pub delayed_records: Vec<Vec<Vec<GhaWaveRecord>>>,
    /// The delayed header mode (residual `scale_only` for the delayed row).
    pub delayed_header_mode: u32,
    /// Delayed arena `+0x360` inverse-mix flags, one per band. Native residual
    /// synthesis reads these for the delayed rows and the fresh front's flags
    /// for the current rows (decompile 42722-42725). `None` is retained only
    /// it preserves their former same-flag replay behavior.
    pub delayed_opposite: Option<Vec<u32>>,
    /// Native record-arena header address (`*(*(obj+0x24))`); anchors row
    /// word 9. Heap-relative, taken from the trace.
    pub record_arena_header: i32,
}

/// Whole-boundary outputs of the composed `extract_ghwave_at5` driver.
#[derive(Debug, Clone, PartialEq)]
pub struct GhaExtractOutput {
    pub dispatch_kind: GhaExtractDispatchKind,
    pub header: ExtractGhaHeader,
    /// Final channel-0 header words `[active, mode, band_count]`.
    pub header_words: [u32; EXTRACT_GHA_HEADER_WORD_COUNT_AT5],
    /// Final output rows `*(obj+0x24)+4` `[ch][band]` (10 words).
    pub output_rows: Vec<Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>>,
    /// The waves written per output row `[ch][band]`.
    pub row_records: Vec<Vec<Vec<GhaWaveRecord>>>,
    /// The record arena in native dispatch (write) order.
    pub arena_records: Vec<GhaWaveRecord>,
    /// The residual-subtracted `param_2` band buffers `[ch][band]`.
    pub band_windows: Vec<Vec<Vec<f32>>>,
    /// The tail outcome (wave limit, overflow, residual plan, header).
    pub tail: Option<GhaExtractTailOutcome>,
    /// The share-gate outputs (`+0x318` shared, `+0x360` opposite/stereo).
    pub shared: Vec<u32>,
    pub opposite: Vec<u32>,
    pub selected_order: Vec<usize>,
}

/// The composed live `extract_ghwave_at5` boundary (decompile
/// `0x0004c930`, line 41560): sequences the detection front
/// (`extract_ghwave_front_at5`) → dispatch execution → row embedding →
/// post-dispatch tail (`extract_ghwave_tail_at5`) → residual
/// synthesis/subtraction, computing the general scheduler call inputs
/// from the boundary surfaces rather than a scheduler trace.
///
/// The general (mask-3) arm builds the `GeneralSchedulerCallInput` list
/// from the caller state — `samples`/`source` are the `param_2` band
/// buffer at `[channel_for_source * band_count + group]` viewed as 256
/// and 384 f32 (native offset 40677 shares the base pointer),
/// `delayed_window_words` are the delayed state row words 4..7,
/// `shared_channel_samples` are both channels' 256-sample windows for a
/// shared call, and `initial_records` are scratch (the arena slot is
/// overwritten) — then runs `analysis_general_at5_compact_scheduler` and
/// embeds the returned rows.
pub fn extract_ghwave_at5(input: GhaExtractInput) -> Result<GhaExtractOutput, GhaExtractError> {
    let GhaExtractInput {
        channel_count,
        band_count,
        param_3,
        // `param_6` is captured for completeness but the general scheduler
        // consumes `param_3` as its threshold/profile (see below).
        profile_selector: _,
        header_flag_word,
        header_0xd0_enabled,
        mut band_windows,
        delayed_rows,
        delayed_records,
        delayed_header_mode,
        delayed_opposite,
        record_arena_header,
    } = input;

    if !(1..=2).contains(&channel_count) {
        return Err(GhaExtractError::UnsupportedChannelCount { channel_count });
    }
    if band_count > MAX_EXTRACT_BANDS_AT5 {
        return Err(GhaExtractError::UnsupportedBandCount { band_count });
    }
    if band_windows.len() < channel_count
        || delayed_rows.len() < channel_count
        || delayed_records.len() < channel_count
    {
        return Err(GhaExtractError::RowsTooShort {
            needed: channel_count,
            actual: band_windows.len().min(delayed_rows.len()),
        });
    }
    for channel in 0..channel_count {
        if band_windows[channel].len() < band_count
            || delayed_rows[channel].len() < band_count
            || delayed_records[channel].len() < band_count
        {
            return Err(GhaExtractError::RowsTooShort {
                needed: band_count,
                actual: band_windows[channel].len(),
            });
        }
        for band in 0..band_count {
            if band_windows[channel][band].len() < EXTRACT_GENERAL_SOURCE_SAMPLES_AT5 {
                return Err(GhaExtractError::InputTooShort {
                    needed: EXTRACT_GENERAL_SOURCE_SAMPLES_AT5,
                    actual: band_windows[channel][band].len(),
                });
            }
        }
    }

    // Fresh output rows (`*(obj+0x24)+4`).
    let mut output_rows =
        vec![vec![[0u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]; band_count]; channel_count];
    for rows in &mut output_rows {
        extract_ghwave_init_row_words_at5(rows);
    }
    let mut row_records: Vec<Vec<Vec<GhaWaveRecord>>> =
        vec![vec![Vec::new(); band_count]; channel_count];
    let mut arena_records = Vec::new();

    // Front detection + dispatch. The scheduler call inputs borrow
    // `band_windows` immutably; the residual pass mutates it afterward, so
    // the borrows are confined to this block.
    let (front_dispatch, header, shared, opposite, selected_order) = {
        let window_refs: Vec<Vec<&[f32]>> = band_windows
            .iter()
            .take(channel_count)
            .map(|channel| channel.iter().map(Vec::as_slice).collect())
            .collect();
        let front = extract_ghwave_front_at5(
            &window_refs,
            &delayed_rows,
            param_3,
            band_count,
            channel_count,
            header_flag_word,
            header_0xd0_enabled,
        )?;
        let GhaExtractFrontOutcome {
            dispatch,
            header,
            energies,
            shared,
            opposite,
            selected_order,
            detection: _,
        } = front;

        let kind = match &dispatch {
            GhaExtractDispatch::General | GhaExtractDispatch::FastPath => {
                let group_count = header.scheduler_group_count;
                if energies.len() < channel_count {
                    // The fast path returns no energies; it never triggers
                    // at 352 kbps and is not composed live here.
                    return Err(GhaExtractError::EnergyTableTooShort {
                        channel: 0,
                        needed: channel_count,
                        actual: energies.len(),
                    });
                }
                let energy_refs: Vec<&[f32]> = energies.iter().map(Vec::as_slice).collect();
                let shared_bools: Vec<bool> = shared[..group_count]
                    .iter()
                    .map(|flag| *flag != 0)
                    .collect();
                let stereo_bools: Vec<bool> = opposite[..group_count]
                    .iter()
                    .map(|flag| *flag != 0)
                    .collect();

                // `analysis_general_at5` uses `param_3` (the `ecx` threshold,
                // 30 for the 352 kbps encode) as both the wave-budget
                // threshold and the profile selector it threads into
                // `analysis_general_at5_sub` (decompile 42250 passes the
                // register `param_3` through; the traced sub calls carry
                // `profile = 30`). The extract stack `param_6` selector is
                // not what the scheduler consumes.
                let scheduler_profile = param_3.max(0) as usize;
                let budgets = crate::gha::analysis::analysis_general_wave_budgets_at5(
                    &energy_refs,
                    group_count,
                    &shared_bools,
                    &selected_order,
                    param_3,
                )
                .map_err(GhaExtractError::from)?;
                let plan = crate::gha::analysis::analysis_general_dispatch_plan_at5(
                    &budgets,
                    &selected_order,
                    &shared_bools,
                    group_count,
                )
                .map_err(GhaExtractError::from)?;

                let scratch: Vec<Vec<GhaWaveRecord>> = plan
                    .iter()
                    .map(|call| {
                        vec![
                            GhaWaveRecord {
                                scale_index: 0,
                                amplitude_index: 0,
                                phase_index: 0,
                                frequency: 0,
                            };
                            call.max_waves
                        ]
                    })
                    .collect();
                let call_inputs: Vec<crate::gha::analysis::GeneralSchedulerCallInput<'_>> = plan
                    .iter()
                    .zip(scratch.iter())
                    .map(|(call, records)| {
                        let group = call.group_index;
                        let source_channel = call.channel_index.unwrap_or(0);
                        let delayed = &delayed_rows[source_channel][group];
                        let delayed_window_words = [
                            delayed[4] as i32,
                            delayed[5] as i32,
                            delayed[6] as i32,
                            delayed[7] as i32,
                        ];
                        let shared_channel_samples = if call.channel_index.is_none() {
                            Some((
                                &band_windows[0][group][..COMPONENT_SAMPLES],
                                &band_windows[1][group][..COMPONENT_SAMPLES],
                            ))
                        } else {
                            None
                        };
                        crate::gha::analysis::GeneralSchedulerCallInput {
                            group_index: group,
                            channel_index: call.channel_index,
                            samples: &band_windows[source_channel][group][..COMPONENT_SAMPLES],
                            source: &band_windows[source_channel][group]
                                [..EXTRACT_GENERAL_SOURCE_SAMPLES_AT5],
                            delayed_window_words,
                            shared_channel_samples,
                            initial_records: records,
                        }
                    })
                    .collect();

                let mut states =
                    vec![vec![[0i32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]; group_count]; channel_count];
                let result = crate::gha::analysis::analysis_general_at5_compact_scheduler(
                    &energy_refs,
                    &selected_order,
                    &shared_bools,
                    &stereo_bools,
                    &mut states,
                    &call_inputs,
                    group_count,
                    scheduler_profile,
                    record_arena_header,
                )
                .map_err(GhaExtractError::from)?;

                let mut writebacks = Vec::with_capacity(result.calls.len());
                for output in &result.calls {
                    let mut row_words = [0u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5];
                    for (word, state_word) in row_words.iter_mut().zip(output.state.iter()) {
                        *word = *state_word as u32;
                    }
                    writebacks.push(ExtractGhaRowWriteback {
                        band_index: output.group_index,
                        channel_index: output.channel_index,
                        row_words,
                    });

                    let count = (output.state[8].max(0) as usize).min(output.records.len());
                    let recs = output.records[..count].to_vec();
                    match output.channel_index {
                        Some(channel) => row_records[channel][output.group_index] = recs.clone(),
                        None => {
                            row_records[0][output.group_index] = recs.clone();
                            if channel_count == 2 {
                                row_records[1][output.group_index] = recs.clone();
                            }
                        }
                    }
                    arena_records.extend(recs);
                }
                drop(scratch);
                extract_ghwave_write_row_words_at5(&mut output_rows, band_count, &writebacks)?;
                GhaExtractDispatchKind::General
            }
            GhaExtractDispatch::Sine(allocation) => {
                let window_refs_ch: Vec<Vec<&[f32]>> = band_windows
                    .iter()
                    .take(channel_count)
                    .map(|channel| channel.iter().map(Vec::as_slice).collect())
                    .collect();
                let peak_indices: Vec<Vec<Option<usize>>> =
                    vec![vec![None; band_count]; channel_count];
                let outcome = extract_ghwave_sine_dispatch_at5(
                    &window_refs_ch,
                    &selected_order,
                    &allocation.counts,
                    &shared,
                    &opposite,
                    &output_rows,
                    &delayed_rows,
                    &peak_indices,
                    header_0xd0_enabled,
                    channel_count,
                    header.scheduler_group_count,
                    record_arena_header,
                )?;
                for (channel, channel_rows) in outcome.rows.iter().take(channel_count).enumerate() {
                    for (band, row) in channel_rows.iter().take(band_count).enumerate() {
                        output_rows[channel][band] = *row;
                    }
                }
                // Populate per-row records from each dispatch call, mirroring
                // the general arm. Native writes the extracted waves directly
                // into the arena at the row-9 pointer (decompile 42532/42624);
                // a shared/mixed call assigns to channel 0 and clones to
                // channel 1 (the full 10-word row copy at 42625–42641 aliases
                // the same record pointer). An unshared call assigns to its
                // own channel. Without this, `run_residual_pass_at5` never
                // subtracts the extracted tone from the band windows.
                for call in &outcome.calls {
                    let recs = outcome.records
                        [call.record_start_index..call.record_start_index + call.wave_count]
                        .to_vec();
                    if call.mixed {
                        row_records[0][call.band_index] = recs.clone();
                        if channel_count == 2 {
                            row_records[1][call.band_index] = recs;
                        }
                    } else {
                        row_records[call.channel_index][call.band_index] = recs;
                    }
                }
                arena_records = outcome.records.clone();
                GhaExtractDispatchKind::Sine
            }
            GhaExtractDispatch::DisabledFallback(_) => GhaExtractDispatchKind::DisabledFallback,
            GhaExtractDispatch::NoAnalysis => GhaExtractDispatchKind::NoAnalysis,
        };

        (kind, header, shared, opposite, selected_order)
    };

    // The disabled fallback skips the tail; the residual runs over the
    // delayed rows against silence and the header active word is cleared.
    if front_dispatch == GhaExtractDispatchKind::DisabledFallback {
        run_residual_pass_at5(
            &mut band_windows,
            &output_rows,
            &row_records,
            &delayed_rows,
            &delayed_records,
            &opposite,
            delayed_opposite.as_deref(),
            header.mode as u32,
            delayed_header_mode,
            channel_count,
            band_count,
        )?;
        return Ok(GhaExtractOutput {
            dispatch_kind: front_dispatch,
            header,
            header_words: [0, header.mode as u32, header.scheduler_group_count as u32],
            output_rows,
            row_records,
            arena_records,
            band_windows,
            tail: None,
            shared,
            opposite,
            selected_order,
        });
    }

    // Post-dispatch tail: wave limit / overflow clear / residual plan /
    // header writeback.
    let tail = extract_ghwave_tail_at5(
        &mut output_rows,
        &delayed_rows,
        &shared,
        header,
        channel_count,
        band_count,
    )?;

    // If the overflow clear fired, the output rows carry no waves; drop
    // the collected records so the arena matches the cleared rows.
    if tail.overflow_cleared {
        for channel_rows in &mut row_records {
            for records in channel_rows {
                records.clear();
            }
        }
        arena_records.clear();
    }

    run_residual_pass_at5(
        &mut band_windows,
        &output_rows,
        &row_records,
        &delayed_rows,
        &delayed_records,
        &opposite,
        delayed_opposite.as_deref(),
        header.mode as u32,
        delayed_header_mode,
        channel_count,
        band_count,
    )?;

    Ok(GhaExtractOutput {
        dispatch_kind: front_dispatch,
        header,
        header_words: tail.header_words,
        output_rows,
        row_records,
        arena_records,
        band_windows,
        tail: Some(tail),
        shared,
        opposite,
        selected_order,
    })
}

/// Executes the post-dispatch residual synthesis/subtraction over every
/// row where the current output row or the delayed row carries waves
/// (band-major, channel-inner), mutating the `param_2` band buffers in
/// place (decompile `LAB_0005d35e` residual loop, line 42709).
#[allow(clippy::too_many_arguments)]
fn run_residual_pass_at5(
    band_windows: &mut [Vec<Vec<f32>>],
    output_rows: &[Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    row_records: &[Vec<Vec<GhaWaveRecord>>],
    delayed_rows: &[Vec<[u32; EXTRACT_GHA_ROW_WORD_COUNT_AT5]>],
    delayed_records: &[Vec<Vec<GhaWaveRecord>>],
    opposite: &[u32],
    delayed_opposite: Option<&[u32]>,
    current_scale_only_mode: u32,
    delayed_scale_only_mode: u32,
    channel_count: usize,
    band_count: usize,
) -> Result<(), GhaExtractError> {
    for band in 0..band_count {
        for channel in 0..channel_count {
            let current_waves = output_rows[channel][band][8] > 0;
            let delayed_waves = delayed_rows[channel][band][8] > 0;
            if !current_waves && !delayed_waves {
                continue;
            }
            let current_invert_flag = if band < opposite.len() {
                opposite[band]
            } else {
                0
            };
            let delayed_invert_flag = delayed_opposite
                .and_then(|flags| flags.get(band))
                .copied()
                .unwrap_or(current_invert_flag);
            let delayed_input = ExtractGhaResidualRowInput {
                row_words: delayed_rows[channel][band],
                records: &delayed_records[channel][band],
                scale_only_mode: delayed_scale_only_mode,
                invert_flag: delayed_invert_flag,
            };
            let current_input = ExtractGhaResidualRowInput {
                row_words: output_rows[channel][band],
                records: &row_records[channel][band],
                scale_only_mode: current_scale_only_mode,
                invert_flag: current_invert_flag,
            };
            extract_ghwave_residual_at5(
                &delayed_input,
                &current_input,
                channel,
                &mut band_windows[channel][band],
            )?;
        }
    }
    Ok(())
}

impl From<crate::gha::analysis::GhaAnalysisError> for GhaExtractError {
    fn from(error: crate::gha::analysis::GhaAnalysisError) -> Self {
        // The scheduler surfaces map onto the extract error space by
        // shape; the driver only reaches these on malformed inputs.
        match error {
            crate::gha::analysis::GhaAnalysisError::Power(power) => GhaExtractError::Power(power),
            crate::gha::analysis::GhaAnalysisError::Scalar(scalar) => {
                GhaExtractError::Scalar(scalar)
            }
            crate::gha::analysis::GhaAnalysisError::Component(synth) => {
                GhaExtractError::Synthesis(synth)
            }
            _ => GhaExtractError::UnsupportedBandCount {
                band_count: usize::MAX,
            },
        }
    }
}
