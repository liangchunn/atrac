//! Typed packing decisions shared by coding orchestration and frame syntax.
//!
//! Unlike the reference native-layout adapter, this module models content and
//! dispatch decisions only. It has no object-memory offsets or byte windows.

use crate::coding::allocation::AllocationError;
use crate::encoder::frontend::FrontendState;
use crate::gha::bitcount::{
    GhaNbitsRow, GhaNbitsSelectorChannel, GhaNbitsSelectorRow, calc_nbits_for_gha_at5,
    calc_nbits_gha_flag_summary_at5, calc_nbits_gha_swap_plan_at5,
};
use crate::gha::synthesis::GhaWaveRecord;

const GHA_MAX_BANDS: usize = 16;
const GHA_MAX_WAVE_TOTAL: usize = 0x30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackingPrepError {
    GainMissingPreviousRows,
    GainRowCountExceedsMax {
        count: usize,
        max: usize,
    },
    GainSelection(AllocationError),
    IdwlModeOutOfRange {
        mode: u32,
    },
    IdwlSelectorOutOfRange {
        selector: i32,
    },
    GhaBandCountExceedsMax {
        band_count: usize,
        max: usize,
    },
    GhaWaveTotalExceedsMax {
        total: usize,
        max: usize,
    },
    GhaUnsupportedChannelCount {
        channel_count: usize,
    },
    GhaHeaderModeZeroUnsupported,
    GhaHasPreviousWithoutReference {
        channel: usize,
    },
    GhaSelectorMissing {
        channel: usize,
        family: &'static str,
    },
}

impl From<AllocationError> for PackingPrepError {
    fn from(error: AllocationError) -> Self {
        Self::GainSelection(error)
    }
}

/// One content row used by GHA packing decisions. The four window words are
/// `[first_present, second_present, first_location, second_location]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaBandData {
    pub window_words: [u32; 4],
    pub wave_count: usize,
    pub records: Vec<GhaWaveRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaChannelSelectors {
    pub idloc: u32,
    pub nwavs: u32,
    pub freq: u32,
    pub idsf: u32,
    pub idam: Option<u32>,
    pub freq_modes: Vec<Option<bool>>,
    pub active_flags: Vec<bool>,
    pub compact_map: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GhaHeaderSummaries {
    pub shared: (bool, bool),
    pub stereo: (bool, bool),
    pub swap: (bool, bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaPackingPrep {
    pub swap_flags: Vec<bool>,
    pub post_swap_channels: Vec<Vec<GhaBandData>>,
    pub channels: Vec<GhaChannelSelectors>,
    pub summaries: GhaHeaderSummaries,
    pub total_bits: usize,
}

fn apply_row_swaps(channels: &mut [Vec<GhaBandData>], swap_flags: &[bool]) {
    if channels.len() != 2 {
        return;
    }
    let (first, second) = channels.split_at_mut(1);
    for (band, &swap) in swap_flags.iter().enumerate() {
        if swap && band < first[0].len() && band < second[0].len() {
            std::mem::swap(&mut first[0][band], &mut second[0][band]);
        }
    }
}

fn compute_gha_packing_prep(
    header_active: bool,
    header_mode: usize,
    band_count: usize,
    channels: &[Vec<GhaBandData>],
    has_previous: &[bool],
    shared_flags: &[bool],
    stereo_flags: &[bool],
) -> Result<GhaPackingPrep, PackingPrepError> {
    let channel_count = channels.len();
    if !(1..=2).contains(&channel_count) {
        return Err(PackingPrepError::GhaUnsupportedChannelCount { channel_count });
    }
    if band_count > GHA_MAX_BANDS {
        return Err(PackingPrepError::GhaBandCountExceedsMax {
            band_count,
            max: GHA_MAX_BANDS,
        });
    }
    let total = channels
        .iter()
        .enumerate()
        .flat_map(|(channel, rows)| {
            rows.iter().enumerate().filter_map(move |(band, row)| {
                (!(channel > 0 && shared_flags.get(band).copied().unwrap_or(false)))
                    .then_some(row.records.len())
            })
        })
        .sum::<usize>();
    if total > GHA_MAX_WAVE_TOTAL {
        return Err(PackingPrepError::GhaWaveTotalExceedsMax {
            total,
            max: GHA_MAX_WAVE_TOTAL,
        });
    }
    if header_active && header_mode == 0 {
        return Err(PackingPrepError::GhaHeaderModeZeroUnsupported);
    }
    for (channel, &previous) in has_previous.iter().enumerate().take(channel_count) {
        if previous && channel == 0 {
            return Err(PackingPrepError::GhaHasPreviousWithoutReference { channel });
        }
    }
    if !header_active {
        return Ok(GhaPackingPrep {
            swap_flags: vec![false; band_count],
            post_swap_channels: channels.to_vec(),
            channels: Vec::new(),
            summaries: GhaHeaderSummaries {
                shared: (false, false),
                stereo: (false, false),
                swap: (false, false),
            },
            total_bits: 1,
        });
    }

    let swap_flags = if channel_count == 2 {
        let rows0 = channels[0]
            .iter()
            .map(|row| GhaNbitsRow {
                nwavs: row.wave_count,
            })
            .collect::<Vec<_>>();
        let rows1 = channels[1]
            .iter()
            .map(|row| GhaNbitsRow {
                nwavs: row.wave_count,
            })
            .collect::<Vec<_>>();
        calc_nbits_gha_swap_plan_at5(&[&rows0, &rows1], band_count).swap_flags
    } else {
        vec![false; band_count]
    };
    let mut post = channels.to_vec();
    apply_row_swaps(&mut post, &swap_flags);

    let selector_rows = post
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|row| GhaNbitsSelectorRow {
                    window_words: row.window_words,
                    nwavs: row.wave_count,
                    records: &row.records,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let selector_channels = (0..channel_count)
        .map(|channel| {
            let has_previous = has_previous.get(channel).copied().unwrap_or(false);
            GhaNbitsSelectorChannel {
                has_previous,
                rows: &selector_rows[channel],
                previous_rows: if has_previous {
                    &selector_rows[channel - 1]
                } else {
                    &[]
                },
            }
        })
        .collect::<Vec<_>>();
    let result = calc_nbits_for_gha_at5(
        header_active,
        header_mode,
        band_count,
        &selector_channels,
        shared_flags,
        stereo_flags,
        &swap_flags,
    );
    let mut per_channel = Vec::with_capacity(channel_count);
    for channel in 0..channel_count {
        let selectors = &result.selectors[channel];
        let require = |value: Option<u32>, family| {
            value.ok_or(PackingPrepError::GhaSelectorMissing { channel, family })
        };
        per_channel.push(GhaChannelSelectors {
            idloc: require(selectors[0], "idloc")?,
            nwavs: require(selectors[1], "nwavs")?,
            freq: require(selectors[2], "freq")?,
            idsf: require(selectors[3], "idsf")?,
            idam: selectors[4],
            freq_modes: result.reverse_modes[channel].clone(),
            active_flags: result.active_flags[channel].clone(),
            compact_map: result.compact_maps[channel].clone(),
        });
    }
    let summarize = |flags: &[bool]| {
        let summary = calc_nbits_gha_flag_summary_at5(flags, band_count);
        (summary.any, summary.mixed)
    };
    let summaries = GhaHeaderSummaries {
        shared: summarize(shared_flags),
        stereo: summarize(stereo_flags),
        swap: summarize(&swap_flags),
    };
    Ok(GhaPackingPrep {
        swap_flags,
        post_swap_channels: post,
        channels: per_channel,
        summaries,
        total_bits: result.total_bits,
    })
}

pub(crate) const GHA_HAS_PREVIOUS: [bool; 2] = [false, true];

pub(crate) fn gha_packing_prep_from_frontend(
    state: &FrontendState,
) -> Result<GhaPackingPrep, PackingPrepError> {
    let channel_count = state.channel_count;
    if !(1..=2).contains(&channel_count) {
        return Err(PackingPrepError::GhaUnsupportedChannelCount { channel_count });
    }
    let header = state.packer_arena(0);
    let band_count = header.header_band_count as usize;
    let channels = (0..channel_count)
        .map(|channel| {
            let arena = state.packer_arena(channel);
            arena
                .rows
                .iter()
                .zip(&arena.records)
                .take(band_count)
                .map(|(words, records)| GhaBandData {
                    window_words: [words[4], words[5], words[6], words[7]],
                    wave_count: words[8] as usize,
                    records: records.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let shared_flags = header
        .shared
        .iter()
        .map(|word| *word != 0)
        .collect::<Vec<_>>();
    let stereo_flags = header
        .opposite
        .iter()
        .map(|word| *word != 0)
        .collect::<Vec<_>>();
    compute_gha_packing_prep(
        header.header_active != 0,
        header.header_mode as usize,
        band_count,
        &channels,
        &GHA_HAS_PREVIOUS[..channel_count],
        &shared_flags,
        &stereo_flags,
    )
}
