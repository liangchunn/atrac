//! GHA side-data bit estimates.
//!
//! `calc_nbits_for_gh_freq_0_at5` follows native offset `0x0000bbf0`: for each
//! active GHA row it costs the frequency list in forward mode (first value raw
//! at 10 bits, later values classed by the previous frequency's remaining
//! range toward 0x400) and reverse mode (last value raw at 10 bits, earlier
//! values classed by the next frequency's value), writes the cheaper mode into
//! the per-band freq-mode word when the row has at least two values, and adds
//! one mode bit to the row cost.

use crate::gha::synthesis::GhaWaveRecord;
use crate::tables::huffman::{
    HuffmanDescriptor, ghpc_freq_a, ghpc_idam_aa, ghpc_idam_ab, ghpc_idam_c, ghpc_idsf_aa,
    ghpc_idsf_ab, ghpc_idsf_b, ghpc_nwavs_a, ghpc_nwavs_b,
};

const GHA_NBITS_IMPOSSIBLE_AT5: usize = 0x4000;

fn ghpc_length_bits(descriptor: HuffmanDescriptor, index: usize) -> usize {
    descriptor
        .pack_table()
        .entry(index)
        .map(|entry| usize::from(entry.bit_len))
        .unwrap_or(GHA_NBITS_IMPOSSIBLE_AT5)
}

pub const GHA_FREQ_RAW_BITS_AT5: usize = 10;

#[derive(Debug, Clone, Copy)]
pub struct GhaFreqBitRow<'a> {
    pub active: bool,
    pub frequencies: &'a [u32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaFreqBitCount {
    pub total_bits: usize,
    pub reverse_modes: Vec<Option<bool>>,
}

pub fn calc_nbits_for_gh_freq_0_at5(rows: &[GhaFreqBitRow<'_>]) -> GhaFreqBitCount {
    let mut total_bits = 0usize;
    let mut reverse_modes = Vec::with_capacity(rows.len());
    for row in rows {
        if !row.active {
            reverse_modes.push(None);
            continue;
        }

        let count = row.frequencies.len();
        let mut forward_bits = 0usize;
        if count > 0 {
            forward_bits = GHA_FREQ_RAW_BITS_AT5;
            for &previous in &row.frequencies[..count - 1] {
                forward_bits += forward_class_bits(previous);
            }
        }
        if count < 2 {
            total_bits += forward_bits;
            reverse_modes.push(None);
            continue;
        }

        let mut reverse_bits = GHA_FREQ_RAW_BITS_AT5;
        for &next in &row.frequencies[1..count] {
            reverse_bits += reverse_class_bits(next);
        }

        let use_reverse = reverse_bits < forward_bits;
        total_bits += forward_bits.min(reverse_bits) + 1;
        reverse_modes.push(Some(use_reverse));
    }

    GhaFreqBitCount {
        total_bits,
        reverse_modes,
    }
}

/// One channel's per-band row surface as seen by `calc_nbits_for_gha_at5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhaNbitsRow {
    pub nwavs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaSharedRowSwapPlan {
    pub swap_flags: Vec<bool>,
}

/// Native `calc_nbits_for_gha_at5` (0x0000ff40) stereo prologue: for each
/// band, when the channel-0 row is empty and the channel-1 row has waves,
/// swap the full 10-word rows between channels and set the header swap flag
/// at word `0xea + band`; mono clears all swap flags.
pub fn calc_nbits_gha_swap_plan_at5(
    channel_rows: &[&[GhaNbitsRow]],
    band_count: usize,
) -> GhaSharedRowSwapPlan {
    let mut swap_flags = Vec::with_capacity(band_count);
    for band in 0..band_count {
        let swap = channel_rows.len() == 2
            && channel_rows[0][band].nwavs == 0
            && channel_rows[1][band].nwavs > 0;
        swap_flags.push(swap);
    }
    GhaSharedRowSwapPlan { swap_flags }
}

/// Apply the planned swaps to the two channels' 10-word row surfaces.
pub fn calc_nbits_gha_apply_swaps_at5(channel_rows: &mut [Vec<[u32; 10]>; 2], swap_flags: &[bool]) {
    let (first, second) = channel_rows.split_at_mut(1);
    for (band, &swap) in swap_flags.iter().enumerate() {
        if swap {
            std::mem::swap(&mut first[0][band], &mut second[0][band]);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhaHeaderFlagSummary {
    pub any: bool,
    pub mixed: bool,
    pub bits: usize,
}

/// Native header flag summary: word pairs `0xc4/0xc5` (shared flags at
/// `+0x318`) and `0xd6/0xd7` (stereo flags at `+0x360`) plus their bit
/// costs: 1 bit when none set, 2 bits when all set, `band_count + 2` when
/// mixed.
pub fn calc_nbits_gha_flag_summary_at5(flags: &[bool], band_count: usize) -> GhaHeaderFlagSummary {
    let set = flags.iter().take(band_count).filter(|&&flag| flag).count();
    if band_count == 0 || set == 0 {
        GhaHeaderFlagSummary {
            any: false,
            mixed: false,
            bits: 1,
        }
    } else if set == band_count {
        GhaHeaderFlagSummary {
            any: true,
            mixed: false,
            bits: 2,
        }
    } else {
        GhaHeaderFlagSummary {
            any: true,
            mixed: true,
            bits: band_count + 2,
        }
    }
}

/// Native `g_hc_ghpc_nbands` code lengths (library `.rodata`, 4-byte
/// `[code_u16, len_u16]` entries; the same 16-entry table the packer reads as
/// `G_A_GH_NBANDS_PACK` in `src/bitstream/frame.rs`). The GHA header costs
/// `2 + len(nbands - 1)` bits in `calc_nbits_for_gha_at5`. Native reads
/// `g_hc_ghpc_nbands[iVar3 * 4 - 2]` indexed by the arena nbands `iVar3`
/// (`piVar9[2]`), NOT clamped (decompile libatrac.c:6885). The table MUST
/// carry all 16 entries: for `nbands` in 9..=16 the true length is 6, so an
/// 8-entry table clamped at index 7 under-counts the header by 1 bit and lets
/// the fit loop overfill a tight GHA-heavy frame by exactly one bit (the
/// InvalidTailTarget{16383,16384} overshoot on the tonal corpus, docs/12
/// §4.3(b)).
const GH_NBANDS_PACK_LENGTHS_AT5: [usize; 16] = [1, 3, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6];

#[derive(Debug, Clone, Copy)]
pub struct GhaNbitsChannelRow<'a> {
    pub active: bool,
    pub lower_window_flag: bool,
    pub upper_window_flag: bool,
    pub nwavs: usize,
    pub frequencies: &'a [u32],
}

#[derive(Debug, Clone, Copy)]
pub struct GhaNbitsChannel<'a> {
    pub has_previous: bool,
    pub rows: &'a [GhaNbitsChannelRow<'a>],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaNbitsResult {
    pub total_bits: usize,
    pub selectors: Vec<[u32; 5]>,
    pub reverse_modes: Vec<Vec<Option<bool>>>,
}

/// Native `calc_nbits_for_gha_at5` (0x0000ff40) for the `param_3 != 1`
/// caller profile: raw per-family costs with all five dispatch selectors
/// forced to zero. The header-active early-out, stereo row swap, and flag
/// summaries are modeled by the companion helpers; this function assumes
/// swaps were already applied to `channels`.
pub fn calc_nbits_for_gha_raw_at5(
    header_active: bool,
    header_mode: usize,
    band_count: usize,
    channels: &[GhaNbitsChannel<'_>],
    shared_flags: &[bool],
    stereo_flags: &[bool],
    swap_flags: &[bool],
) -> GhaNbitsResult {
    if !header_active {
        return GhaNbitsResult {
            total_bits: 1,
            selectors: Vec::new(),
            reverse_modes: Vec::new(),
        };
    }

    let mut total_bits = 2 + GH_NBANDS_PACK_LENGTHS_AT5[(band_count - 1).min(15)];
    if channels.len() == 2 {
        total_bits += calc_nbits_gha_flag_summary_at5(shared_flags, band_count).bits;
        total_bits += calc_nbits_gha_flag_summary_at5(stereo_flags, band_count).bits;
        total_bits += calc_nbits_gha_flag_summary_at5(swap_flags, band_count).bits;
    }

    let mut selectors = Vec::with_capacity(channels.len());
    let mut reverse_modes = Vec::with_capacity(channels.len());
    for channel in channels {
        let freq_rows: Vec<GhaFreqBitRow<'_>> = channel
            .rows
            .iter()
            .map(|row| GhaFreqBitRow {
                active: row.active,
                frequencies: &row.frequencies[..row.nwavs.min(row.frequencies.len())],
            })
            .collect();
        let freq = calc_nbits_for_gh_freq_0_at5(&freq_rows);

        let mut channel_bits = freq.total_bits;
        for row in channel.rows.iter().take(band_count) {
            if !row.active {
                continue;
            }
            // IDLOC raw: two window flags plus one 5-bit location per flag.
            channel_bits += if row.lower_window_flag { 7 } else { 2 };
            if row.upper_window_flag {
                channel_bits += 5;
            }
            // NWAVS raw: 4 bits per active row.
            channel_bits += 4;
            // IDSF: 6 bits per wave in header mode 1+, one 6-bit value for
            // non-empty rows in mode 0.
            channel_bits += if header_mode != 0 {
                row.nwavs * 6
            } else if row.nwavs > 0 {
                6
            } else {
                0
            };
            // IDAM: 4 bits per wave in header mode 0 only.
            if header_mode == 0 {
                channel_bits += row.nwavs * 4;
            }
            // Per-wave payload: 5 bits per wave.
            channel_bits += row.nwavs * 5;
        }
        // Per-channel header cost.
        channel_bits += match (channel.has_previous, header_mode != 0) {
            (false, true) => 2,
            (false, false) => 3,
            (true, true) => 6,
            (true, false) => 8,
        };

        total_bits += channel_bits;
        selectors.push([0; 5]);
        reverse_modes.push(freq.reverse_modes);
    }

    GhaNbitsResult {
        total_bits,
        selectors,
        reverse_modes,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GhaNbitsSelectorRow<'a> {
    pub window_words: [u32; 4],
    pub nwavs: usize,
    pub records: &'a [GhaWaveRecord],
}

#[derive(Debug, Clone, Copy)]
pub struct GhaNbitsSelectorChannel<'a> {
    pub has_previous: bool,
    pub rows: &'a [GhaNbitsSelectorRow<'a>],
    pub previous_rows: &'a [GhaNbitsSelectorRow<'a>],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaNbitsSelectorResult {
    pub total_bits: usize,
    pub active_flags: Vec<Vec<bool>>,
    pub selectors: Vec<[Option<u32>; 5]>,
    pub reverse_modes: Vec<Vec<Option<bool>>>,
    pub compact_maps: Vec<Vec<i32>>,
}

/// Native `calc_nbits_for_gha_at5` (0x0000ff40) for the production
/// `param_3 == 1` caller profile: per-family candidate costs over the
/// `g_hc_ghpc_*` Huffman tables with dispatch selector writeback. Assumes
/// the stereo row swap has already been applied to `channels`.
pub fn calc_nbits_for_gha_at5(
    header_active: bool,
    header_mode: usize,
    band_count: usize,
    channels: &[GhaNbitsSelectorChannel<'_>],
    shared_flags: &[bool],
    stereo_flags: &[bool],
    swap_flags: &[bool],
) -> GhaNbitsSelectorResult {
    if !header_active {
        return GhaNbitsSelectorResult {
            total_bits: 1,
            active_flags: Vec::new(),
            selectors: Vec::new(),
            reverse_modes: Vec::new(),
            compact_maps: Vec::new(),
        };
    }

    let mut total_bits = 2 + GH_NBANDS_PACK_LENGTHS_AT5[(band_count - 1).min(15)];
    if channels.len() == 2 {
        total_bits += calc_nbits_gha_flag_summary_at5(shared_flags, band_count).bits;
        total_bits += calc_nbits_gha_flag_summary_at5(stereo_flags, band_count).bits;
        total_bits += calc_nbits_gha_flag_summary_at5(swap_flags, band_count).bits;
    }

    let mut active_flags = Vec::with_capacity(channels.len());
    let mut selectors = Vec::with_capacity(channels.len());
    let mut reverse_modes = Vec::with_capacity(channels.len());
    let mut compact_maps = Vec::with_capacity(channels.len());
    for channel in channels {
        let active: Vec<bool> = if channel.has_previous {
            (0..band_count).map(|band| !shared_flags[band]).collect()
        } else {
            vec![true; band_count]
        };

        // IDLOC: raw window flags versus unchanged-from-previous.
        let mut idloc_raw = 0usize;
        for (band, row) in channel.rows.iter().take(band_count).enumerate() {
            if !active[band] {
                continue;
            }
            idloc_raw += if row.window_words[0] != 0 { 7 } else { 2 };
            if row.window_words[1] != 0 {
                idloc_raw += 5;
            }
        }
        let mut idloc_selector = 0u32;
        let mut idloc_bits = idloc_raw;
        if channel.has_previous {
            let mut unchanged = 0usize;
            for band in 0..band_count {
                if active[band]
                    && channel.rows[band].window_words != channel.previous_rows[band].window_words
                {
                    unchanged += GHA_NBITS_IMPOSSIBLE_AT5;
                }
            }
            if unchanged < idloc_raw {
                idloc_selector = 1;
                idloc_bits = unchanged;
            }
        }

        // NWAVS: raw, Huffman A, previous-delta B, unchanged.
        let mut nwavs_costs = [0usize; 4];
        for band in 0..band_count {
            if !active[band] {
                continue;
            }
            let nwavs = channel.rows[band].nwavs;
            nwavs_costs[0] += 4;
            nwavs_costs[1] += if nwavs < 8 {
                ghpc_length_bits(ghpc_nwavs_a(), nwavs)
            } else {
                GHA_NBITS_IMPOSSIBLE_AT5
            };
            if channel.has_previous {
                let delta = nwavs as i32 - channel.previous_rows[band].nwavs as i32;
                nwavs_costs[2] += if (-4..4).contains(&delta) {
                    ghpc_length_bits(ghpc_nwavs_b(), (delta & 7) as usize)
                } else {
                    GHA_NBITS_IMPOSSIBLE_AT5
                };
                if delta != 0 {
                    nwavs_costs[3] += GHA_NBITS_IMPOSSIBLE_AT5;
                }
            }
        }
        let candidate_count = if channel.has_previous { 4 } else { 2 };
        let mut nwavs_selector = 0usize;
        for candidate in 1..candidate_count {
            if nwavs_costs[candidate] < nwavs_costs[nwavs_selector] {
                nwavs_selector = candidate;
            }
        }
        let nwavs_bits = nwavs_costs[nwavs_selector];

        // FREQ: ported forward/reverse coder versus previous-row deltas.
        let freq_storage: Vec<Vec<u32>> = (0..band_count)
            .map(|band| {
                let row = &channel.rows[band];
                row.records
                    .iter()
                    .take(row.nwavs)
                    .map(|record| record.frequency as u32)
                    .collect()
            })
            .collect();
        let freq_rows: Vec<GhaFreqBitRow<'_>> = freq_storage
            .iter()
            .enumerate()
            .map(|(band, frequencies)| GhaFreqBitRow {
                active: active[band],
                frequencies,
            })
            .collect();
        let freq = calc_nbits_for_gh_freq_0_at5(&freq_rows);
        let mut freq_selector = 0u32;
        let mut freq_bits = freq.total_bits;
        if channel.has_previous {
            let mut delta_bits = 0usize;
            for band in 0..band_count {
                if !active[band] {
                    continue;
                }
                let row = &channel.rows[band];
                let previous = &channel.previous_rows[band];
                let mut band_bits = 0usize;
                for (wave, record) in row.records.iter().take(row.nwavs).enumerate() {
                    let delta = if wave < previous.nwavs {
                        record.frequency as i32 - previous.records[wave].frequency as i32
                    } else if previous.nwavs < 1 {
                        record.frequency as i32
                    } else {
                        record.frequency as i32
                            - previous.records[previous.nwavs - 1].frequency as i32
                    };
                    if !(-0x80..0x80).contains(&delta) {
                        band_bits = GHA_NBITS_IMPOSSIBLE_AT5;
                        break;
                    }
                    band_bits += ghpc_length_bits(ghpc_freq_a(), (delta & 0xff) as usize);
                }
                delta_bits += band_bits;
            }
            if delta_bits < freq.total_bits {
                freq_selector = 1;
                freq_bits = delta_bits;
            }
        }

        // Compact previous-index map: nearest previous frequency within 7,
        // else positional fallback while in range, else -1.
        let mut compact_map = Vec::new();
        if channel.has_previous {
            for band in 0..band_count {
                if !active[band] || channel.rows[band].nwavs == 0 {
                    continue;
                }
                let row = &channel.rows[band];
                let previous = &channel.previous_rows[band];
                for (wave, record) in row.records.iter().take(row.nwavs).enumerate() {
                    let mut best_index = 0usize;
                    let mut best_distance = 0x400i32;
                    for (prev_index, prev_record) in
                        previous.records.iter().take(previous.nwavs).enumerate()
                    {
                        let distance =
                            (record.frequency as i32 - prev_record.frequency as i32).abs();
                        if distance < best_distance {
                            best_distance = distance;
                            best_index = prev_index;
                        }
                    }
                    if previous.nwavs > 0 && best_distance <= 7 {
                        compact_map.push(best_index as i32);
                    } else if wave < previous.nwavs {
                        compact_map.push(wave as i32);
                    } else {
                        compact_map.push(-1);
                    }
                }
            }
        }

        // IDSF: raw, first/all Huffman, previous-delta B, unchanged.
        let mut idsf_costs = [0usize; 4];
        let mut map_cursor = 0usize;
        for band in 0..band_count {
            if !active[band] {
                continue;
            }
            let row = &channel.rows[band];
            if header_mode == 0 {
                idsf_costs[0] += if row.nwavs > 0 { 6 } else { 0 };
                if row.nwavs > 0 {
                    let first = row.records[0].scale_index as i32;
                    idsf_costs[1] += if (0x18..0x38).contains(&first) {
                        ghpc_length_bits(ghpc_idsf_aa(), (first - 0x18) as usize)
                    } else {
                        GHA_NBITS_IMPOSSIBLE_AT5
                    };
                    if channel.has_previous {
                        let previous = &channel.previous_rows[band];
                        let base = if previous.nwavs < 1 {
                            0x2c
                        } else {
                            previous.records[0].scale_index as i32
                        };
                        let delta = first - base;
                        idsf_costs[2] += if (-0x10..0x10).contains(&delta) {
                            ghpc_length_bits(ghpc_idsf_b(), (delta & 0x1f) as usize)
                        } else {
                            GHA_NBITS_IMPOSSIBLE_AT5
                        };
                        let unchanged_base = if previous.nwavs < 1 {
                            0x31
                        } else {
                            previous.records[0].scale_index as i32
                        };
                        if first != unchanged_base {
                            idsf_costs[3] += GHA_NBITS_IMPOSSIBLE_AT5;
                        }
                    }
                }
            } else {
                idsf_costs[0] += row.nwavs * 6;

                // Candidate 1 (all-Huffman): native `local_194` accumulates a
                // per-band `local_1a0` whose own loop breaks (setting `0x4000`)
                // when a scale leaves `[0x14, 0x34)`. This band cost is
                // INDEPENDENT of the delta/unchanged candidates below — the
                // three native loops (decompile 7412-7540) are separate, so an
                // out-of-range scale forces this candidate impossible even when
                // an earlier wave's delta already went out of range.
                let mut all_bits = 0usize;
                for record in row.records.iter().take(row.nwavs) {
                    let scale = record.scale_index as i32;
                    if (scale.wrapping_sub(0x14) as u32) >= 0x20 {
                        all_bits = GHA_NBITS_IMPOSSIBLE_AT5;
                        break;
                    }
                    all_bits += ghpc_length_bits(ghpc_idsf_ab(), (scale - 0x14) as usize);
                }
                idsf_costs[1] += all_bits;

                if channel.has_previous {
                    let previous = &channel.previous_rows[band];

                    // Candidate 2 (previous-delta B): native `local_1b0` with
                    // its own per-band break.
                    let mut delta_bits = 0usize;
                    for (wave, record) in row.records.iter().take(row.nwavs).enumerate() {
                        let scale = record.scale_index as i32;
                        let map_index = compact_map[map_cursor + wave];
                        if map_index < 0 {
                            if (scale.wrapping_sub(0x12) as u32) >= 0x20 {
                                delta_bits = GHA_NBITS_IMPOSSIBLE_AT5;
                                break;
                            }
                            delta_bits +=
                                ghpc_length_bits(ghpc_idsf_b(), ((scale - 0x22) & 0x1f) as usize);
                        } else {
                            let prev_scale =
                                previous.records[map_index as usize].scale_index as i32;
                            let delta = scale - prev_scale;
                            if ((delta + 0x10) as u32) >= 0x20 {
                                delta_bits = GHA_NBITS_IMPOSSIBLE_AT5;
                                break;
                            }
                            delta_bits += ghpc_length_bits(ghpc_idsf_b(), (delta & 0x1f) as usize);
                        }
                    }
                    idsf_costs[2] += delta_bits;

                    // Candidate 3 (unchanged-from-previous): native `local_1dc`
                    // ANDs over ALL waves with no early break.
                    let mut unchanged = true;
                    for (wave, record) in row.records.iter().take(row.nwavs).enumerate() {
                        let scale = record.scale_index as i32;
                        let map_index = compact_map[map_cursor + wave];
                        let same = if map_index < 0 {
                            scale == 0x20
                        } else {
                            scale == previous.records[map_index as usize].scale_index as i32
                        };
                        unchanged &= same;
                    }
                    if row.nwavs > 0 && !unchanged {
                        idsf_costs[3] += GHA_NBITS_IMPOSSIBLE_AT5;
                    }
                }
            }
            if channel.has_previous {
                map_cursor += row.nwavs;
            }
        }
        let idsf_candidates = if channel.has_previous { 4 } else { 2 };
        let mut idsf_selector = 0usize;
        for candidate in 1..idsf_candidates {
            if idsf_costs[candidate] < idsf_costs[idsf_selector] {
                idsf_selector = candidate;
            }
        }
        let idsf_bits = idsf_costs[idsf_selector];

        // IDAM: only costed and selected in header mode 0.
        let mut idam_selector = None;
        let mut idam_bits = 0usize;
        if header_mode == 0 {
            let mut idam_costs = [0usize; 4];
            let mut map_cursor = 0usize;
            for band in 0..band_count {
                if !active[band] {
                    continue;
                }
                let row = &channel.rows[band];
                idam_costs[0] += row.nwavs * 4;
                if row.nwavs == 1 {
                    idam_costs[1] +=
                        ghpc_length_bits(ghpc_idam_aa(), row.records[0].amplitude_index);
                } else {
                    for record in row.records.iter().take(row.nwavs) {
                        idam_costs[1] += ghpc_length_bits(ghpc_idam_ab(), record.amplitude_index);
                    }
                }
                if channel.has_previous {
                    let previous = &channel.previous_rows[band];
                    let mut delta_bits = 0usize;
                    let mut unchanged = true;
                    for (wave, record) in row.records.iter().take(row.nwavs).enumerate() {
                        let amplitude = record.amplitude_index as i32;
                        let map_index = compact_map[map_cursor + wave];
                        if map_index < 0 {
                            if !(8..16).contains(&amplitude) {
                                delta_bits = GHA_NBITS_IMPOSSIBLE_AT5;
                                break;
                            }
                            delta_bits +=
                                ghpc_length_bits(ghpc_idam_c(), ((amplitude - 0xc) & 7) as usize);
                            unchanged &= amplitude == 0xe;
                        } else {
                            let prev_amplitude =
                                previous.records[map_index as usize].amplitude_index as i32;
                            let delta = amplitude - prev_amplitude;
                            if !(-4..4).contains(&delta) {
                                delta_bits = GHA_NBITS_IMPOSSIBLE_AT5;
                                break;
                            }
                            delta_bits += ghpc_length_bits(ghpc_idam_c(), (delta & 7) as usize);
                            unchanged &= amplitude == prev_amplitude;
                        }
                    }
                    idam_costs[2] += delta_bits;
                    if row.nwavs > 0 && !unchanged {
                        idam_costs[3] += GHA_NBITS_IMPOSSIBLE_AT5;
                    }
                    map_cursor += row.nwavs;
                }
            }
            let idam_candidates = if channel.has_previous { 4 } else { 2 };
            let mut best = 0usize;
            for candidate in 1..idam_candidates {
                if idam_costs[candidate] < idam_costs[best] {
                    best = candidate;
                }
            }
            idam_selector = Some(best as u32);
            idam_bits = idam_costs[best];
        }

        // Per-wave payload: 5 bits per wave.
        let mut payload_bits = 0usize;
        for band in 0..band_count {
            if active[band] {
                payload_bits += channel.rows[band].nwavs * 5;
            }
        }

        let header_bits = match (channel.has_previous, header_mode != 0) {
            (false, true) => 2,
            (false, false) => 3,
            (true, true) => 6,
            (true, false) => 8,
        };

        total_bits += idloc_bits
            + nwavs_bits
            + freq_bits
            + idsf_bits
            + idam_bits
            + payload_bits
            + header_bits;
        active_flags.push(active);
        selectors.push([
            Some(idloc_selector),
            Some(nwavs_selector as u32),
            Some(freq_selector),
            Some(idsf_selector as u32),
            idam_selector,
        ]);
        reverse_modes.push(freq.reverse_modes);
        compact_maps.push(compact_map);
    }

    GhaNbitsSelectorResult {
        total_bits,
        active_flags,
        selectors,
        reverse_modes,
        compact_maps,
    }
}

fn forward_class_bits(previous: u32) -> usize {
    if previous < 0x200 {
        10
    } else if previous < 0x300 {
        9
    } else if previous < 0x380 {
        8
    } else if previous < 0x3c0 {
        7
    } else if previous < 0x3e0 {
        6
    } else if previous < 0x3f0 {
        5
    } else if previous < 0x3f8 {
        4
    } else if previous < 0x3fc {
        3
    } else if previous < 0x3fe {
        2
    } else {
        1
    }
}

fn reverse_class_bits(next: u32) -> usize {
    if next < 2 {
        1
    } else if next < 4 {
        2
    } else if next < 8 {
        3
    } else if next < 0x10 {
        4
    } else if next < 0x20 {
        5
    } else if next < 0x40 {
        6
    } else if next < 0x80 {
        7
    } else if next < 0x100 {
        8
    } else if next < 0x200 {
        9
    } else {
        10
    }
}
