//! Bitstream packing shared by the classic ATRAC3 encoder cores.
//!
//! Ports the 6 functions from `libatrac.so.1.2.0` that serialize the
//! encoder state into the ATRAC3 bitstream format.

use crate::analysis::gain::GainInfo;
use crate::core::coding::quant::{
    HuffEntry, HuffTableSet, huffbits, ispof_iqt_at3, nsps_inqt_at3, twidof_id_at3, wlof_idwl_at3,
};
use crate::tables::ITB_GROUP_TABLE;

/// `pack_store_from_msb` (`0x6a018`): Writes a value MSB-first into a byte
/// buffer at a given bit position.
///
/// - `value`: the value to write (lower `num_bits` bits are used)
/// - `num_bits`: number of bits to write (0 = no-op)
/// - `buffer`: target byte buffer
/// - `bit_pos`: cumulative bit counter (updated in place)
pub fn pack_store_from_msb(value: u32, num_bits: u32, buffer: &mut [u8], bit_pos: &mut u32) {
    if num_bits == 0 {
        return;
    }
    let mut bits_left = num_bits;
    loop {
        let byte_idx = (*bit_pos >> 3) as usize;
        let bit_off_in_byte = *bit_pos & 7;
        let remaining_in_byte = 8 - bit_off_in_byte;

        if bits_left < remaining_in_byte {
            let shift = remaining_in_byte - bits_left;
            let mask = (0xFFu32 >> bit_off_in_byte) & (0xFFu32 << shift);
            if byte_idx < buffer.len() {
                buffer[byte_idx] |= ((value << shift) & mask) as u8;
            }
            *bit_pos += bits_left;
            break;
        } else {
            bits_left -= remaining_in_byte;
            let mask = 0xFFu32 >> bit_off_in_byte;
            if byte_idx < buffer.len() {
                buffer[byte_idx] |= (mask & (value >> bits_left)) as u8;
            }
            *bit_pos += remaining_in_byte;
            if bits_left == 0 {
                break;
            }
        }
    }
}

/// `itbgrpof_itb_at3` (`0x656b8`): ITB index → ITB group index.
///
/// Returns `ITB_GROUP_TABLE[itb]` for `itb < 16`, else −1.
#[inline]
pub fn itbgrpof_itb_at3(itb: u32) -> i32 {
    if itb < 16 {
        ITB_GROUP_TABLE[itb as usize]
    } else {
        -1
    }
}

/// `pack_specs` (`0x7901c`): Writes Huffman-coded mantissas into the
/// bitstream.
///
/// Helper called by `pack_mddata_at3` for both tone and spectral mantissas.
/// Uses the code/length pairs from `HuffEntry::codes` and writes each code
/// via `pack_store_from_msb`.
///
/// Returns `Err(-1)` on invalid coding mode, `Ok(())` otherwise.
pub fn pack_specs(
    entry: &HuffEntry,
    mantissas: &[i32],
    mantissa_count: usize,
    buffer: &mut [u8],
    bit_pos: &mut u32,
) -> Result<(), i32> {
    let count = mantissa_count.min(mantissas.len());
    match entry.ngrp {
        1 => {
            for &m in mantissas.iter().take(count) {
                let idx = (m as u32 & entry.mask) as usize;
                if idx < entry.codes.len() {
                    let (code, len) = entry.codes[idx];
                    pack_store_from_msb(code, len, buffer, bit_pos);
                }
            }
        }
        2 => {
            let mut i = 0;
            while i + 1 < count {
                let hi = mantissas[i] as u32 & entry.mask;
                let lo = mantissas[i + 1] as u32 & entry.mask;
                let idx = ((hi << (entry.wlof as u32)) | lo) as usize;
                if idx < entry.codes.len() {
                    let (code, len) = entry.codes[idx];
                    pack_store_from_msb(code, len, buffer, bit_pos);
                }
                i += 2;
            }
        }
        _ => return Err(-1),
    }
    Ok(())
}

/// `nbits_for_spectrum` (`0x65b2c`): Predicts the bit cost for packing
/// spectral (non-tone) BFU data.
///
/// Returns total bit count, or −0x8000 on error.
pub fn nbits_for_spectrum(
    bfu_count: i32,
    table_idx: i32,
    idwl_array: &[i32],
    spectral_data: &[i32],
    spec_huff: &HuffTableSet,
) -> i32 {
    if bfu_count < 0 {
        return -0x8000;
    }
    let mut total = bfu_count * 3 + 6;

    let bfu_num = bfu_count as usize;

    // Per-BFU header bits
    for &idwl in idwl_array.iter().take(bfu_num) {
        if idwl >= 1 {
            total += 6;
        } else if idwl < 0 {
            return -0x8000;
        }
    }

    // Spectral Huffman bits
    let Ok(table_ix) = usize::try_from(table_idx) else {
        return -0x8000;
    };
    if table_ix >= spec_huff.tables.len() {
        return -0x8000;
    }

    for (i, &idwl) in idwl_array.iter().enumerate().take(bfu_num) {
        if idwl > 0 {
            let pos = ispof_iqt_at3(i as u32);
            let nsps = nsps_inqt_at3(i as u32);
            if pos < 0 || nsps < 0 {
                return -0x8000;
            }
            let spec_start = pos as usize;
            let spec_end = (spec_start + nsps as usize).min(spectral_data.len());
            let entry = spec_huff.entry(table_ix, idwl);
            let bits = huffbits(entry, &spectral_data[spec_start..spec_end], nsps as usize);
            if bits == -0x8000 {
                return -0x8000;
            }
            total += bits;
        }
    }

    total
}

/// Data for a single tone component used in `nbits_for_component`.
pub struct ToneComponentNbits {
    /// Starting spectral position of this tone.
    pub position: i32,
    /// Quantized mantissa values.
    pub mantissas: Vec<i32>,
}

/// Data for a single tone group used in `nbits_for_component`.
pub struct ToneGroupNbits {
    /// Word-length index (IDWL).
    pub idwl: i32,
    /// Huffman table selector (0 or 1).
    pub table_idx: i32,
    /// Per-ITB-group mask: `template[i]` != 0 means this group has tones.
    pub has_tone: [i32; 4],
    /// Per-BFU tone count within the group.
    pub per_bfu_tone_count: Vec<i32>,
    /// Tone components, ordered by BFU then by component within BFU.
    pub components: Vec<ToneComponentNbits>,
}

/// `nbits_for_component` (`0x6595c`): Predicts the bit cost for packing
/// tone components into the bitstream.
///
/// Returns total bit count, or −0x8000 on error.
pub fn nbits_for_component(
    bfu_count: i32,
    tone_group_count: i32,
    coding_mode: i32,
    tone_groups: &[ToneGroupNbits],
    tone_huff: &HuffTableSet,
) -> i32 {
    if tone_group_count < 0 {
        return -0x8000;
    }

    let mut total = if tone_group_count >= 1 { 7 } else { 5 };

    if tone_group_count == 0 {
        return total;
    }

    for group in tone_groups.iter().take(tone_group_count as usize) {
        // 1 bit per BFU (band flags)
        total += bfu_count;
        // Group header bits
        total += 6;

        if coding_mode == 3 {
            return -0x8000;
        }

        if twidof_id_at3(group.idwl as u32) == -1 {
            return -0x8000;
        }

        let Ok(table_ix) = usize::try_from(group.table_idx) else {
            return -0x8000;
        };
        if table_ix >= tone_huff.tables.len() {
            return -0x8000;
        }

        let entry = tone_huff.entry(table_ix, group.idwl);
        let width = twidof_id_at3(group.idwl as u32);

        let mut comp_idx = 0usize;
        // C loops while local_1c < bfu_count * 4. itbgrpof_itb_at3 handles
        // indices 0..15, so clamp the range to valid ITB indices.
        for bfu in 0..(bfu_count.saturating_mul(4).min(16)) as usize {
            let itb_group = itbgrpof_itb_at3(bfu as u32);
            if itb_group == -1 {
                continue;
            }
            let grp = itb_group as usize;
            if grp < group.has_tone.len() && group.has_tone[grp] != 0 {
                // Component header
                total += 3;

                let tone_cnt = if bfu < group.per_bfu_tone_count.len() {
                    group.per_bfu_tone_count[bfu]
                } else {
                    0
                };

                for _ in 0..tone_cnt {
                    if comp_idx >= group.components.len() {
                        return -0x8000;
                    }
                    let comp = &group.components[comp_idx];

                    let mut w = width;
                    if w + comp.position > 0x400 {
                        w = 0x400 - comp.position;
                    }
                    if w <= 0 {
                        comp_idx += 1;
                        total += 12;
                        continue;
                    }

                    let bits = huffbits(entry, &comp.mantissas, w as usize);
                    if bits == -0x8000 {
                        return -0x8000;
                    }
                    total += 12 + bits;
                    comp_idx += 1;
                }
            }
        }
    }

    total
}

/// `nbits_for_packdata` (`0x658b4`): Wrapper summing the 4 sub-functions.
///
/// Calls `nbits_for_component`, `nbits_for_spectrum`, `nbits_for_sheader`,
/// and `nbits_for_adjust`, summing their results.
///
/// Returns total bit count, or −0x8000 on error.
#[allow(clippy::too_many_arguments)]
pub fn nbits_for_packdata(
    bfu_count: i32,
    spectral_bfu_count: i32,
    tone_group_count: i32,
    coding_mode: i32,
    table_idx: i32,
    idwl_array: &[i32],
    spectral_data: &[i32],
    tone_groups: &[ToneGroupNbits],
    tone_huff: &HuffTableSet,
    spec_huff: &HuffTableSet,
    joint_stereo: bool,
    adjust_count: i32,
    adjust_per_bfu: &[i32],
) -> i32 {
    let comp_bits = nbits_for_component(
        bfu_count,
        tone_group_count,
        coding_mode,
        tone_groups,
        tone_huff,
    );
    if comp_bits == -0x8000 {
        return -0x8000;
    }
    let spec_bits = nbits_for_spectrum(
        spectral_bfu_count,
        table_idx,
        idwl_array,
        spectral_data,
        spec_huff,
    );
    if spec_bits == -0x8000 {
        return -0x8000;
    }
    let shdr = crate::core::coding::quant::nbits_for_sheader(joint_stereo);
    let adjust = crate::core::coding::quant::nbits_for_adjust(adjust_count, adjust_per_bfu);
    shdr + adjust + comp_bits + spec_bits
}

/// Data for one tone component in pack_mddata_at3.
#[derive(Clone)]
pub struct PackToneComponent {
    /// Word-length index (IDWL, quant_idx equivalent).
    pub idwl: i32,
    /// Scale factor index (IDSF).
    pub idsf: i32,
    /// Coded length identifier.
    pub coded_len: i32,
    /// Huffman table selector (0 or 1).
    pub table_idx: i32,
    /// Quantized mantissa values.
    pub mantissas: Vec<i32>,
    /// Starting spectral position.
    pub position: i32,
}

/// Group of tone components with shared idwl and coded_len.
#[derive(Clone)]
pub struct PackToneGroup {
    pub idwl: i32,
    pub coded_len: i32,
    pub table_idx: i32,
    pub bfu_flags: Vec<i32>,
    pub itb_components: Vec<Vec<PackToneComponent>>,
}

fn tone_itb_index(position: i32) -> Option<usize> {
    if position < 0 {
        return None;
    }
    let itb = (position / 0x40) as usize;
    if itb < 16 { Some(itb) } else { None }
}

fn tone_itb_group(position: i32, bfu_count: usize) -> Option<usize> {
    let itb = tone_itb_index(position)?;
    let group = itbgrpof_itb_at3(itb as u32);
    if group < 0 {
        return None;
    }
    let group = group as usize;
    if group < bfu_count { Some(group) } else { None }
}

/// Group tone components by (idwl, coded_len) for per-group packing.
///
/// Ensures at least 2 groups are produced when components exist and the
/// single group has more than 7 components (required by the binary decoder).
fn build_tone_groups(
    components: &[PackToneComponent],
    tone_component_count: usize,
    bfu_count: i32,
) -> Vec<PackToneGroup> {
    let count = tone_component_count.min(components.len());
    let bfu_count = bfu_count.max(0) as usize;
    if count == 0 || bfu_count == 0 {
        return Vec::new();
    }

    use std::collections::BTreeMap;
    type GroupEntry = (i32, Vec<PackToneComponent>);
    let mut map: BTreeMap<(i32, i32), GroupEntry> = BTreeMap::new();

    for comp in components.iter().take(count) {
        if tone_itb_group(comp.position, bfu_count).is_none() {
            continue;
        }
        let key = (comp.idwl, comp.coded_len);
        let entry = map
            .entry(key)
            .or_insert_with(|| (comp.table_idx, Vec::new()));
        entry.1.push(comp.clone());
    }

    let mut result = Vec::new();
    let itb_limit = bfu_count.saturating_mul(4).min(16);
    for ((idwl, coded_len), (table_idx, components)) in map {
        let mut pending = components;
        while !pending.is_empty() {
            let mut next_pending = Vec::new();
            let mut itb_counts = [0usize; 16];
            let mut bfu_flags = vec![0i32; bfu_count];
            let mut itb_components = vec![Vec::new(); itb_limit];
            for comp in pending {
                let Some(itb) = tone_itb_index(comp.position) else {
                    continue;
                };
                let Some(itb_group) = tone_itb_group(comp.position, bfu_count) else {
                    continue;
                };
                if itb_counts[itb] < 7 {
                    itb_counts[itb] += 1;
                    bfu_flags[itb_group] = 1;
                    itb_components[itb].push(comp);
                } else {
                    next_pending.push(comp);
                }
            }
            if itb_components.iter().all(Vec::is_empty) {
                break;
            }
            result.push(PackToneGroup {
                idwl,
                coded_len,
                table_idx,
                bfu_flags,
                itb_components,
            });
            pending = next_pending;
        }
    }

    result
}

/// `pack_mddata_at3` (`0x68b04`): Main bitstream packer for one channel.
///
/// Writes headers, tone component data, and spectral data into a byte buffer
/// using `pack_store_from_msb` and `pack_specs`.
///
/// Returns the number of bits written, or −1 on error or if bit budget is
/// exceeded.
///
/// Parameters:
/// - `packing_enabled`: state[0x1860] — if == 1, skip VLC packing (returns −1)
/// - `bfu_count`: state[1] — for gain-control header (param_1[1] in C, byte offset 4)
/// - `spectral_bfu_count`: state[0] — for spectral section (*param_1 in C, byte offset 0)
/// - `coding_mode`: state[2] — coding mode, error if == 3
/// - `table_idx`: state[3] — Huffman table selector
/// - `_tone_group_count`: state[0x44] — number of tone groups (replaced by actual group count)
/// - `tone_component_count`: number of tone components
///
/// Note: The C code uses param_1[1] (offset 4) for the gain-control header and
/// *param_1 (offset 0) for the spectral header. These are distinct fields
/// and may hold different values in some encoder states.
#[allow(clippy::too_many_arguments)]
pub fn pack_mddata_at3(
    packing_enabled: i32,
    bfu_count: i32,
    spectral_bfu_count: i32,
    coding_mode: i32,
    table_idx: i32,
    _tone_group_count: i32,
    tone_component_count: i32,
    gain_control: &[GainInfo],
    tone_components: &[PackToneComponent],
    explicit_tone_groups: Option<&[PackToneGroup]>,
    idwl_array: &[i32],
    scale_factors: &[i32],
    spectral_data: &[i32],
    tone_huff: &HuffTableSet,
    spec_huff: &HuffTableSet,
    buffer: &mut [u8],
    _bit_budget: i32,
    channel_index: usize,
    joint_stereo: bool,
) -> i32 {
    let mut bit_pos: u32 = 0;

    if packing_enabled != 1 {
        // Frame header
        if joint_stereo && channel_index == 1 {
            // JS side-channel header. The Sony decoder expects this specific
            // format when joint_stereo=1 is set in the WAV header.
            // Format: 0 (1b) + 7 (3b) + 3,3,3,3 (4x2b) + 3 (2b) = 14 bits,
            // replacing the 6-bit 0x28 sync word. The 8-bit difference
            // matches nbits_for_sheader(true) - nbits_for_sheader(false).
            pack_store_from_msb(0, 1, buffer, &mut bit_pos);
            pack_store_from_msb(7, 3, buffer, &mut bit_pos);
            for _ in 0..4 {
                pack_store_from_msb(3, 2, buffer, &mut bit_pos);
            }
            pack_store_from_msb(3, 2, buffer, &mut bit_pos);
        } else {
            pack_store_from_msb(0x28, 6, buffer, &mut bit_pos);
        }
        pack_store_from_msb((bfu_count - 1) as u32, 2, buffer, &mut bit_pos);

        // Gain-control adjustment header (C lines 56471-56493).
        // The three bfu_count entries are 0x40-byte GainInfo structs at
        // state+0x10, state+0x50, state+0x90. nbits_for_adjust counts the
        // same records as bfu_count * 3 + sum(count) * 9.
        for i in 0..bfu_count as usize {
            let info = gain_control.get(i);
            let point_count = if let Some(info) = info {
                info.count.clamp(0, 7)
            } else {
                0
            };
            pack_store_from_msb(point_count as u32, 3, buffer, &mut bit_pos);
            if let Some(info) = info {
                for t in 0..point_count as usize {
                    pack_store_from_msb(info.level[t] as u32, 4, buffer, &mut bit_pos);
                    pack_store_from_msb(info.location[t] as u32, 5, buffer, &mut bit_pos);
                }
            }
        }

        // Tone groups: group components by (idwl, coded_len), then write
        // per-group headers and per-BFU/ITB component data.
        // The C code iterates tone groups from state+0x114, writing group-level
        // bfu_flags + coded_len + idwl, then per-ITB component data.
        let built_groups;
        let groups = if let Some(groups) = explicit_tone_groups {
            groups
        } else {
            built_groups =
                build_tone_groups(tone_components, tone_component_count as usize, bfu_count);
            &built_groups
        };
        let actual_tone_group_count = groups.len() as i32;

        // Tone group info
        pack_store_from_msb(actual_tone_group_count as u32, 5, buffer, &mut bit_pos);
        if actual_tone_group_count > 0 {
            pack_store_from_msb(1, 2, buffer, &mut bit_pos);
        }

        if actual_tone_group_count > 0 {
            for group in groups {
                // Per-BFU presence flags (1 bit each)
                for bfu in 0..bfu_count as usize {
                    let present = if bfu < group.bfu_flags.len() {
                        group.bfu_flags[bfu]
                    } else {
                        0
                    };
                    pack_store_from_msb(present as u32, 1, buffer, &mut bit_pos);
                }

                // Group header: coded_len (3 bits), idwl (3 bits)
                pack_store_from_msb(group.coded_len as u32, 3, buffer, &mut bit_pos);
                pack_store_from_msb(group.idwl as u32, 3, buffer, &mut bit_pos);

                if coding_mode == 3 {
                    return -1;
                }

                if wlof_idwl_at3(group.idwl as u32) == -1 {
                    return -1;
                }
                let width = twidof_id_at3(group.coded_len as u32);
                if width == -1 {
                    return -1;
                }

                let Ok(table_ix) = usize::try_from(group.table_idx) else {
                    return -1;
                };
                if table_ix >= tone_huff.tables.len() {
                    return -1;
                }

                let entry = tone_huff.entry(table_ix, group.idwl);

                // Per-ITB component data within this group.
                // Each ITB within an active group can have at most 7 components
                // (3-bit field). Components are distributed across ITBs with
                // max 7 per slot.
                {
                    for itb_idx in 0..(bfu_count.saturating_mul(4).min(16)) as usize {
                        let itb_group = itbgrpof_itb_at3(itb_idx as u32);
                        if itb_group == -1 {
                            continue;
                        }
                        let ig = itb_group as usize;
                        if ig >= group.bfu_flags.len() || group.bfu_flags[ig] == 0 {
                            continue;
                        }

                        let all_itb_comps = group
                            .itb_components
                            .get(itb_idx)
                            .map(Vec::as_slice)
                            .unwrap_or_default();
                        let this_count = all_itb_comps.len();

                        pack_store_from_msb(this_count as u32, 3, buffer, &mut bit_pos);

                        for comp in all_itb_comps {
                            let pos = comp.position;
                            let mut effective_width = width;
                            if pos + effective_width > 0x400 {
                                effective_width = 0x400 - pos;
                            }
                            if effective_width > 0 {
                                pack_store_from_msb(comp.idsf as u32, 6, buffer, &mut bit_pos);
                                pack_store_from_msb(
                                    comp.position.rem_euclid(0x40) as u32,
                                    6,
                                    buffer,
                                    &mut bit_pos,
                                );

                                if pack_specs(
                                    entry,
                                    &comp.mantissas,
                                    effective_width as usize,
                                    buffer,
                                    &mut bit_pos,
                                )
                                .is_err()
                                {
                                    return -1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Spectral header — uses spectral_bfu_count (state[0] / *param_1 in C)
        pack_store_from_msb((spectral_bfu_count - 1) as u32, 5, buffer, &mut bit_pos);
        pack_store_from_msb(table_idx as u32, 1, buffer, &mut bit_pos);

        // Per-BFU IDWL values
        let spec_bfu_n = spectral_bfu_count as usize;
        let idwl_at = |i: usize| idwl_array.get(i).copied().unwrap_or(0);
        for i in 0..spec_bfu_n {
            pack_store_from_msb(idwl_at(i) as u32, 3, buffer, &mut bit_pos);
        }

        // Per-BFU scale factors
        for i in 0..spec_bfu_n {
            if idwl_at(i) > 0 {
                let sf = scale_factors.get(i).copied().unwrap_or(0);
                pack_store_from_msb(sf as u32, 6, buffer, &mut bit_pos);
            }
        }

        // Spectral mantissas via pack_specs
        let Ok(table_ix) = usize::try_from(table_idx) else {
            return -1;
        };
        if table_ix >= spec_huff.tables.len() {
            return -1;
        }

        for i in 0..spec_bfu_n {
            let idwl = idwl_at(i);
            if idwl > 0 {
                let wlof2 = wlof_idwl_at3(idwl as u32);
                if wlof2 > 0 {
                    let pos = ispof_iqt_at3(i as u32);
                    if pos < 0 {
                        return -1;
                    }
                    let nsps = nsps_inqt_at3(i as u32);
                    if nsps < 0 {
                        return -1;
                    }
                    let spec_start = pos as usize;
                    let spec_end = (spec_start + nsps as usize).min(spectral_data.len());
                    let entry = spec_huff.entry(table_ix, idwl);
                    if pack_specs(
                        entry,
                        &spectral_data[spec_start..spec_end],
                        nsps as usize,
                        buffer,
                        &mut bit_pos,
                    )
                    .is_err()
                    {
                        return -1;
                    }
                }
            }
        }

        // Return actual packed bit count (may exceed budget for quiet frames
        // where only the subheader is written).
        return bit_pos as i32;
    }

    -1
}

/// `put_chsunit_at3` (`0x69200`): Writes one channel's sound unit into the
/// bitstream.
///
/// Thin wrapper around `pack_mddata_at3` that manages byte offset tracking.
///
/// Returns the bit count on success, or −1 on error.
#[allow(clippy::too_many_arguments)]
pub fn put_chsunit_at3(
    joint_stereo: i32,
    back_ptr_mode: i32,
    back_ptr_flush_mode: i32,
    back_ptr_byte_offset: &mut i32,
    bfu_byte_count: i32,
    packing_enabled: i32,
    bfu_count: i32,
    spectral_bfu_count: i32,
    coding_mode: i32,
    table_idx: i32,
    tone_group_count: i32,
    tone_component_count: i32,
    gain_control: &[GainInfo],
    tone_components: &[PackToneComponent],
    idwl_array: &[i32],
    scale_factors: &[i32],
    spectral_data: &[i32],
    tone_huff: &HuffTableSet,
    spec_huff: &HuffTableSet,
    buffer: &mut [u8],
    bit_budget: i32,
) -> i32 {
    if joint_stereo == 1 && back_ptr_mode == 0 {
        return -1;
    }

    // The C code passes param_3 + back_ptr[0x18] as the buffer to
    // pack_mddata_at3, writing at an offset into the channel output.
    let offset = (*back_ptr_byte_offset) as usize;
    let out_buf = if offset < buffer.len() {
        &mut buffer[offset..]
    } else {
        return -1;
    };

    let result = pack_mddata_at3(
        packing_enabled,
        bfu_count,
        spectral_bfu_count,
        coding_mode,
        table_idx,
        tone_group_count,
        tone_component_count,
        gain_control,
        tone_components,
        None,
        idwl_array,
        scale_factors,
        spectral_data,
        tone_huff,
        spec_huff,
        out_buf,
        bit_budget,
        0,
        joint_stereo != 0,
    );

    if result == -1 {
        return -1;
    }

    if back_ptr_mode != 1 && back_ptr_flush_mode != 2 {
        *back_ptr_byte_offset += bfu_byte_count;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_store_single_byte() {
        let mut buf = [0u8; 4];
        let mut bit_pos = 0u32;
        pack_store_from_msb(0xAB, 8, &mut buf, &mut bit_pos);
        assert_eq!(bit_pos, 8);
        assert_eq!(buf[0], 0xAB);
    }

    #[test]
    fn pack_store_msb_first() {
        let mut buf = [0u8; 4];
        let mut bit_pos = 0u32;
        pack_store_from_msb(0xABC, 12, &mut buf, &mut bit_pos);
        assert_eq!(bit_pos, 12);
        assert_eq!(buf[0], 0xAB);
        assert_eq!(buf[1], 0xC0);
    }

    #[test]
    fn pack_store_bit_aligned() {
        let mut buf = [0u8; 4];
        let mut bit_pos = 0u32;
        pack_store_from_msb(0x1, 1, &mut buf, &mut bit_pos);
        assert_eq!(bit_pos, 1);
        assert_eq!(buf[0], 0x80);

        pack_store_from_msb(0x1, 1, &mut buf, &mut bit_pos);
        assert_eq!(bit_pos, 2);
        assert_eq!(buf[0], 0xC0);
    }

    #[test]
    fn pack_store_crosses_byte_boundary() {
        let mut buf = [0u8; 4];
        let mut bit_pos = 4u32;
        pack_store_from_msb(0xFF, 8, &mut buf, &mut bit_pos);
        assert_eq!(bit_pos, 12);
        assert_eq!(buf[0] & 0x0F, 0x0F);
        assert_eq!(buf[1] >> 4, 0x0F);
    }

    #[test]
    fn pack_store_zero_bits_noop() {
        let mut buf = [0xFFu8; 4];
        let mut bit_pos = 0u32;
        pack_store_from_msb(0, 0, &mut buf, &mut bit_pos);
        assert_eq!(bit_pos, 0);
        assert_eq!(buf[0], 0xFF);
    }

    #[test]
    fn itbgrpof_is_grouped() {
        assert_eq!(itbgrpof_itb_at3(0), 0);
        assert_eq!(itbgrpof_itb_at3(3), 0);
        assert_eq!(itbgrpof_itb_at3(4), 1);
        assert_eq!(itbgrpof_itb_at3(7), 1);
        assert_eq!(itbgrpof_itb_at3(8), 2);
        assert_eq!(itbgrpof_itb_at3(12), 3);
        assert_eq!(itbgrpof_itb_at3(15), 3);
        assert_eq!(itbgrpof_itb_at3(16), -1);
    }

    #[test]
    fn nbits_for_spectrum_basic() {
        let huff = HuffTableSet::build_spec();
        let idwl = [0i32; 32];
        let specs = [0i32; 1024];
        let bits = nbits_for_spectrum(32, 0, &idwl, &specs, &huff);
        assert!(bits >= 0);
    }

    #[test]
    fn nbits_for_component_no_groups() {
        let huff = HuffTableSet::build_tone();
        let bits = nbits_for_component(0, 0, 0, &[], &huff);
        assert_eq!(bits, 5);
    }

    #[test]
    fn nbits_for_component_negative_group_count() {
        let huff = HuffTableSet::build_tone();
        let bits = nbits_for_component(0, -1, 0, &[], &huff);
        assert_eq!(bits, -0x8000);
    }
}
