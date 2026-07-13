//! Tone / transient extraction for the classic ATRAC3 encoder.
//!
//! Reproduces the behaviour of `libatrac.so.1.2.0`:
//! - `is_attack` (`0x67b0c`): checks if 16 tone-template counts indicate an
//!   attack (transient) — returns `true` if `max − min ≥ 3`.
//! - `single_tone_check` (`0x684b4`): scans the 256-bin per-bin descriptor
//!   table to find the global-max bin, then checks neighbor bins to decide
//!   tone strength (0 = none, 1 = weak, 2 = strong).
//! - `extract_tone_specs` (`0x690d4`): subtracts a tone template from the
//!   residual spectrum.
//! - `quant_tone_specs` (`0x67e40`): quantizes tone mantissas and returns
//!   the Huffman bit count (+ 12).
//! - `extract_single_tones` (`0x68670`): extracts single-tone components.
//! - `extract_multitone` (`0x68118`): extracts multi-tone components.
//!
//! ## Per-bin descriptor layout (0x48 = 72 bytes each, 256 entries)
//!
//! Each descriptor describes one spectral bin:
//! ```text
//! offset  field           type
//! 0x00    peak_value      int32  (max |spec| in this bin's group)
//! 0x04    idsf            int32  (scale-factor index)
//! 0x08..0x48 tone_template int32[16] (tone allocation counts per sub-block)
//! ```
//!
//! ## Tone component layout (0x38 = 56 bytes each)
//!
//! ```text
//! offset  field       type
//! 0x00    position    int32  (spectral position, <<2)
//! 0x04    idsf        int32  (scale-factor index, set by quant_tone_specs)
//! 0x08..0x28 mantissas int32[8] (quantized tone mantissas)
//! 0x28    width_id    int32  (3 → twidof_id → width=4)
//! 0x2c    idwl        int32  (7 → group type for huffbits)
//! 0x30    table_idx   int32  (0 or 1 → huff codebook selection)
//! 0x34    padding     int32
//! ```
//!
//! ## Tone group layout (0x21c = 540 bytes each)
//!
//! ```text
//! offset  field           type
//! 0x00    group_type      int32  (7)
//! 0x04    width_id        int32  (3)
//! 0x08    table_idx       int32  (1)
//! 0x0c..0x4c tone_template int32[16] (per-sub-block allocation counts)
//! 0x4c..0x21c tone_components int32[~112] (pointers/refs to tone entries)
//! ```

use crate::dsp::quant::{
    HuffTableSet, abs_max, huffbits, idscfof_absval_at3, idscfof_val_at3, nstepsof_idwl_at3,
    quant_at3, scfof_id_at3, twidof_id_at3,
};

fn read_i32_le(bytes: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn write_i32_le(buf: &mut [u8], off: usize, val: i32) {
    buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
}

/// Number of spectral bins (per QMF band group, 256 per band × 4 bands).
pub const NUM_BINS: usize = 256;
/// Per-bin descriptor size in bytes.
pub const BIN_DESCRIPTOR_SIZE: usize = 0x48;
/// Tone component size in bytes.
pub const TONE_COMPONENT_SIZE: usize = 0x38;
/// Tone group size in bytes.
pub const TONE_GROUP_SIZE: usize = 0x21c;
/// Max tone components per frame (from state field +0x4278, capped at 0x40).
pub const MAX_TONE_COMPONENTS: usize = 0x40;
/// Max tone groups per frame (from state field +0x110).
pub const MAX_TONE_GROUPS: usize = 31;

/// Per-bin spectral descriptor (72 bytes in libatrac).
#[derive(Debug, Clone, Copy, Default)]
pub struct BinDescriptor {
    pub peak_value: i32,
    pub idsf: i32,
    pub tone_template: [i32; 16],
}

impl BinDescriptor {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= BIN_DESCRIPTOR_SIZE);
        let peak_value = read_i32_le(bytes, 0x00);
        let idsf = read_i32_le(bytes, 0x04);
        let mut tone_template = [0i32; 16];
        for (i, slot) in tone_template.iter_mut().enumerate() {
            *slot = read_i32_le(bytes, 0x08 + i * 4);
        }
        Self {
            peak_value,
            idsf,
            tone_template,
        }
    }
}

/// Tone component (56 bytes in libatrac).
#[derive(Debug, Clone, Copy, Default)]
pub struct ToneComponent {
    pub position: i32,
    pub idsf: i32,
    pub mantissas: [i32; 8],
    pub width_id: i32,
    pub idwl: i32,
    pub table_idx: i32,
}

impl ToneComponent {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= TONE_COMPONENT_SIZE);
        let position = read_i32_le(bytes, 0x00);
        let idsf = read_i32_le(bytes, 0x04);
        let mut mantissas = [0i32; 8];
        for (i, slot) in mantissas.iter_mut().enumerate() {
            *slot = read_i32_le(bytes, 0x08 + i * 4);
        }
        let width_id = read_i32_le(bytes, 0x28);
        let idwl = read_i32_le(bytes, 0x2c);
        let table_idx = read_i32_le(bytes, 0x30);
        Self {
            position,
            idsf,
            mantissas,
            width_id,
            idwl,
            table_idx,
        }
    }

    pub fn to_bytes(&self) -> [u8; TONE_COMPONENT_SIZE] {
        let mut buf = [0u8; TONE_COMPONENT_SIZE];
        write_i32_le(&mut buf, 0x00, self.position);
        write_i32_le(&mut buf, 0x04, self.idsf);
        for (i, &m) in self.mantissas.iter().enumerate() {
            write_i32_le(&mut buf, 0x08 + i * 4, m);
        }
        write_i32_le(&mut buf, 0x28, self.width_id);
        write_i32_le(&mut buf, 0x2c, self.idwl);
        write_i32_le(&mut buf, 0x30, self.table_idx);
        buf
    }
}

/// `is_attack` (`0x67b0c`): checks if 16 tone-template counts indicate an
/// attack (transient).
///
/// The initial min and max are both 4 (a sentinel). The function iterates
/// over `count` values, updating min/max. Returns `true` if
/// `max − min ≥ 3`.
///
/// In the encoder, the 16 values are the tone-template counts from one
/// per-bin descriptor row. A large spread means a spectral attack.
pub fn is_attack(values: &[i32]) -> bool {
    let mut min_val = 4i32;
    let mut max_val = 4i32;
    for &v in values {
        if max_val < v {
            max_val = v;
        }
        if v < min_val {
            min_val = v;
        }
    }
    max_val - min_val >= 3
}

/// `single_tone_check` (`0x684b4`): scans the 256-bin per-bin descriptor
/// table to find the global-max peak bin, then checks neighbor bins.
///
/// Returns:
/// - 0: no single tone
/// - 1: weak single tone (peak > 0x14 but <= 0x23)
/// - 2: strong single tone (peak > 0x23 and neighbor_max < threshold)
pub fn single_tone_check(descriptors: &[BinDescriptor]) -> i32 {
    let global_max_bin = find_global_max_bin(descriptors);
    let neighbor_max = scan_neighbors(descriptors, global_max_bin);
    let peak = descriptors[global_max_bin].idsf;

    let threshold = if peak - 0x11 < 9 { 9 } else { peak - 0x11 };

    if neighbor_max < threshold {
        if peak > 0x23 {
            2
        } else if peak > 0x14 {
            1
        } else {
            0
        }
    } else {
        0
    }
}

fn find_global_max_bin(descriptors: &[BinDescriptor]) -> usize {
    let mut best_bin = 0usize;
    let mut best_peak = descriptors[0].idsf;
    for (i, d) in descriptors.iter().enumerate().take(NUM_BINS) {
        if d.idsf > best_peak {
            best_peak = d.idsf;
            best_bin = i;
        }
    }
    best_bin
}

fn scan_neighbors(descriptors: &[BinDescriptor], global_max_bin: usize) -> i32 {
    let group = global_max_bin as i32 / 0x40;
    let remainder = (global_max_bin % 0x40) as i32;

    let neighbor0 = group; // first of two "neighbor" groups (boundary-side)
    let neighbor1 = if remainder < 0x20 {
        group - 1
    } else {
        group + 1
    };

    let mut max_neighbor = 0i32;

    // Walk all 4 groups (group offsets 0, 0x40, 0x80, 0xc0)
    for g in 0..4 {
        let start = g * 0x40;
        if g == (group as usize) {
            // Max's own group: skip window [0, remainder+2], scan rest
            let skip_until = (remainder + 3).min(0x40);
            for d in &descriptors[start + skip_until as usize..start + 0x40] {
                if max_neighbor < d.idsf {
                    max_neighbor = d.idsf;
                }
            }
        } else {
            let is_neighbor =
                (g == neighbor1 as usize) || (g == neighbor0 as usize && g != group as usize);

            if is_neighbor {
                // Neighbor group: scan before the mirror position
                // "before" region: 0 .. (0x3f - remainder) - 2
                let before_end = (0x3f - remainder - 2).clamp(0, 0x40);
                for d in &descriptors[start..start + before_end as usize] {
                    if max_neighbor < d.idsf {
                        max_neighbor = d.idsf;
                    }
                }
                let after_start = (0x3f - remainder + 3).clamp(0, 0x40);
                for d in &descriptors[start + after_start as usize..start + 0x40] {
                    if max_neighbor < d.idsf {
                        max_neighbor = d.idsf;
                    }
                }
            } else {
                // Non-neighbor group: scan all 64 descriptors
                for d in &descriptors[start..start + 0x40] {
                    if max_neighbor < d.idsf {
                        max_neighbor = d.idsf;
                    }
                }
            }
        }
    }
    max_neighbor
}

/// `extract_tone_specs` (`0x690d4`): subtracts a tone template from the
/// residual spectrum.
///
/// - `residual`: the spectral residual (f32 array, modified in place)
/// - `position`: starting spectral position (0..1023)
/// - `idsf`: scale-factor index
/// - `idwl_for_nsteps`: IDWL (for `nstepsof_idwl_at3`)
/// - `idwl_for_twid`: IDWL (for `twidof_id_at3`)
/// - `tone_template`: the tone template values (int array)
///
/// Returns 0 on success, −1 if any lookup fails.
pub fn extract_tone_specs(
    residual: &mut [f32],
    position: u32,
    idsf: u32,
    idwl_for_nsteps: u32,
    idwl_for_twid: u32,
    tone_template: &[i32],
) -> i32 {
    let nsteps = nstepsof_idwl_at3(idwl_for_nsteps);
    if nsteps == -1 {
        return -1;
    }

    let sf = scfof_id_at3(idsf);
    if sf <= 0.0 {
        return -1;
    }

    let width = twidof_id_at3(idwl_for_twid);
    if width == -1 {
        return -1;
    }

    let scale = sf / (nsteps as f64 + 0.5);
    for i in 0..width as usize {
        let pos = position as usize + i;
        if pos < residual.len() && pos < 1024 {
            let template_val = tone_template.get(i).copied().unwrap_or(0) as f64;
            residual[pos] = (residual[pos] as f64 - scale * template_val) as f32;
        }
    }
    0
}

/// `quant_tone_specs` (`0x67e40`): quantizes tone mantissas and returns
/// Huffman bit count + 12.
///
/// - `specs`: the spectral data (f32 array)
/// - `component`: the tone component (modified: idsf and mantissas set)
/// - `huff_tables`: the Huffman table set
///
/// Returns the bit count + 12, or `−0x8000` on failure.
pub fn quant_tone_specs(
    specs: &[f32],
    component: &mut ToneComponent,
    huff_tables: &HuffTableSet,
) -> i32 {
    let width = twidof_id_at3(component.width_id as u32);
    if width == -1 {
        return -0x8000;
    }

    let start = component.position as usize;
    let slice = if start + width as usize <= specs.len() {
        &specs[start..start + width as usize]
    } else {
        return -0x8000;
    };

    let max_abs = abs_max(slice);
    let idsf = idscfof_val_at3(max_abs as f32);
    component.idsf = idsf;

    let sf = scfof_id_at3(idsf as u32);
    if sf <= 0.0 {
        return -0x8000;
    }

    let nsteps = nstepsof_idwl_at3(component.idwl as u32);
    if nsteps == -1 {
        return -0x8000;
    }

    for (i, &val) in slice.iter().enumerate().take(width as usize) {
        component.mantissas[i] = quant_at3(val, sf as f32, nsteps);
    }

    let entry = huff_tables.entry(component.table_idx as usize, component.idwl);
    let bits = huffbits(entry, &component.mantissas, width as usize);
    if bits == -0x8000 {
        return -0x8000;
    }
    bits + 0xc
}

/// `extract_single_tones` (`0x68670`): extracts single-tone components
/// around the global-max peak bin.
///
/// Parameters match the binary's cdecl layout:
/// - `bit_budget`: remaining bits available for encoding
/// - `stc_result`: `single_tone_check` return value (1 = weak, 2 = strong;
///   used as the outer loop count)
/// - `window_radius`: ±N bins around each target position
/// - `max_peak_bin`: the global max bin index (0..255)
/// - `group_span`: number of valid groups (3 or 4, set by caller based on
///   peak bin position)
/// - `max_descriptors`: max descriptor count (256)
/// - `specs`: the spectral data (1024 f32, modified in place by
///   `extract_tone_specs` via `quant_tone_specs`)
/// - `descriptors`: the 256×0x48 descriptor table (IDSF at offset +4,
///   modified in place)
/// - `huff_tables`: the tone-path Huffman table set
///
/// Returns the total bit cost of all extracted tones, or `−0x8000` on error.
#[allow(clippy::too_many_arguments)]
pub fn extract_single_tones(
    bit_budget: i32,
    stc_result: i32,
    window_radius: i32,
    max_peak_bin: i32,
    group_span: i32,
    max_descriptors: i32,
    specs: &mut [f32],
    descriptors: &mut [BinDescriptor],
    huff_tables: &HuffTableSet,
    tone_components_out: &mut Vec<ToneComponent>,
) -> i32 {
    let group = max_peak_bin / 0x40;
    let remainder = max_peak_bin % 0x40;
    let neighbor_group = if remainder < 0x20 {
        group - 1
    } else {
        group + 1
    };

    let mirror = 0x3f - remainder;
    let mut total_bits = (group_span + 6) * stc_result;
    let mut keep_going = true;

    // Per-group state arrays (mirror the binary's tone group template)
    // group_flags[i]: 0 = unused, 1 = used (for group indices 0..group_span-1)
    // sub_group_counts[i]: number of tones per 16-bin subgroup (0..7)
    let max_sg = 16usize;

    // Outer loop: stc_result tone groups
    for _tone_group in 0..stc_result {
        let mut group_flags = vec![0i32; group_span as usize];
        let mut sub_group_counts = vec![0i32; max_sg];

        if keep_going {
            // Two candidate positions: the peak itself and its neighbor
            let candidates = [(group, remainder), (neighbor_group, mirror)];

            for &(g, r) in &candidates {
                if g < 0 || g >= group_span {
                    continue;
                }

                let gidx = g as usize;

                if group_flags[gidx] == 0 {
                    group_flags[gidx] = 1;
                    total_bits += 0xc;
                }

                if total_bits >= bit_budget {
                    continue;
                }

                let position = g * 0x40 + r;
                let start = (position - window_radius).max(0);
                let end = (position + window_radius).min(max_descriptors - 1);

                if start > end {
                    continue;
                }

                for pos in start..=end {
                    let sub_group = if pos >= 0 {
                        (pos as u32 >> 4) as usize
                    } else {
                        ((pos + 0xf) as u32 >> 4) as usize
                    };

                    if sub_group >= max_sg || sub_group_counts[sub_group] >= 7 {
                        continue;
                    }

                    let p = pos as usize;
                    if p >= descriptors.len() {
                        continue;
                    }

                    let desc_idsf = descriptors[p].idsf;
                    let mut component = ToneComponent {
                        position: pos << 2,
                        idsf: desc_idsf,
                        mantissas: [0; 8],
                        width_id: 3,
                        idwl: 7,
                        table_idx: 1,
                    };

                    let bit_cost = quant_tone_specs(specs, &mut component, huff_tables);
                    if bit_cost == -0x8000 {
                        return -0x8000;
                    }

                    if bit_budget < bit_cost + total_bits {
                        keep_going = false;
                    } else {
                        sub_group_counts[sub_group] += 1;

                        let ets_result = extract_tone_specs(
                            specs,
                            (pos << 2) as u32,
                            component.idsf as u32,
                            7,
                            3,
                            &component.mantissas,
                        );
                        if ets_result == -1 {
                            return -0x8000;
                        }

                        total_bits += bit_cost;

                        tone_components_out.push(component);

                        let slice_start = (pos << 2) as usize;
                        let slice_end = (slice_start + 4).min(specs.len());
                        let peak = abs_max(&specs[slice_start..slice_end]);
                        let new_idsf = idscfof_absval_at3(peak as f32);
                        descriptors[p].idsf = new_idsf;
                    }
                }

                if !keep_going {
                    break;
                }
            }
        }
    }

    total_bits
}

/// `extract_multitone` (`0x68118`): extracts multi-tone components from all
/// bins exceeding a threshold.
///
/// Parameters match the binary's cdecl layout:
/// - `bit_budget`: remaining bits available for encoding
/// - `desc_count`: number of descriptors to process (from `ispof_iqt_at3` / 4)
/// - `group_span`: group span (3 or 4)
/// - `threshold`: average IDSF threshold — only bins with
///   `IDSF > threshold + 32.0` are candidates
/// - `specs`: spectral data (1024 f32, modified in place)
/// - `descriptors_a`: descriptor table A (IDSF updated with new values)
/// - `descriptors_b`: descriptor table B (IDSF delta added: old - new)
/// - `huff_tables`: tone-path Huffman table set
///
/// Runs a 2-iteration retry loop. Returns the total bit cost, or -0x8000 on
/// error.
#[allow(clippy::too_many_arguments)]
pub fn extract_multitone(
    bit_budget: i32,
    desc_count: i32,
    group_span: i32,
    threshold: f32,
    specs: &mut [f32],
    descriptors_a: &mut [BinDescriptor],
    descriptors_b: &mut [BinDescriptor],
    huff_tables: &HuffTableSet,
    tone_components_out: &mut Vec<ToneComponent>,
) -> i32 {
    extract_multitone_with_groups(
        bit_budget,
        desc_count,
        group_span,
        threshold,
        specs,
        descriptors_a,
        descriptors_b,
        huff_tables,
        tone_components_out,
    )
    .0
}

#[allow(clippy::too_many_arguments)]
pub fn extract_multitone_with_groups(
    bit_budget: i32,
    desc_count: i32,
    group_span: i32,
    threshold: f32,
    specs: &mut [f32],
    descriptors_a: &mut [BinDescriptor],
    descriptors_b: &mut [BinDescriptor],
    huff_tables: &HuffTableSet,
    tone_components_out: &mut Vec<ToneComponent>,
) -> (i32, i32) {
    let mut total_bits = 0i32;
    let mut group_count = 0i32;
    let mut stop_flag = false;
    for _retry_pass in 0..2 {
        if stop_flag {
            continue;
        }

        // Phase 1: count candidates per 16-bin subgroup
        let mut subgroup_counts = [0i32; 16];
        let mut candidate_count = 0i32;
        for (i, d) in descriptors_b.iter().take(desc_count as usize).enumerate() {
            let idsf = d.idsf;
            if idsf as f32 > threshold + 32.0
                && tone_components_out.len() + (candidate_count as usize) < MAX_TONE_COMPONENTS
            {
                let sg = i >> 4;
                if sg < 16 && subgroup_counts[sg] < 7 {
                    subgroup_counts[sg] += 1;
                    candidate_count += 1;
                }
            }
        }

        if candidate_count <= 0 {
            continue;
        }

        // Phase 2: initial budget check
        total_bits += 6 + group_span;
        if bit_budget < total_bits {
            stop_flag = true;
            continue;
        }
        group_count += 1;

        // Phase 3: allocate tone group and extract tones
        let mut quad_flags = [0i32; 4];
        let mut sg_counts = [0i32; 16];

        for i in 0..desc_count as usize {
            let idsf = descriptors_b[i].idsf;
            if idsf as f32 <= threshold + 32.0 {
                continue;
            }

            if tone_components_out.len() >= MAX_TONE_COMPONENTS {
                continue;
            }

            let sg = i >> 4;
            if sg >= 16 || sg_counts[sg] >= 7 {
                continue;
            }

            let quad = sg >> 2;
            if quad >= 4 {
                continue;
            }

            // Quad-level flag: first tone in this quad costs extra 12 bits
            if quad_flags[quad] == 0 {
                if bit_budget < total_bits + 0xc {
                    stop_flag = true;
                    break;
                }
                quad_flags[quad] = 1;
                total_bits += 0xc;
            }

            // CLC overhead: aa_cbitlen[group_type(7) + table_idx(1)*8] * 4
            let clc_idx = 7 + 8;
            let clc_overhead = crate::tables::CLC_BIT_LENGTH_TABLE[clc_idx] * 4;
            if bit_budget < total_bits + 0xc + clc_overhead {
                stop_flag = true;
                break;
            }

            let desc_idsf = descriptors_a[i].idsf;
            let mut component = ToneComponent {
                position: (i << 2) as i32,
                idsf: desc_idsf,
                mantissas: [0; 8],
                width_id: 3,
                idwl: 7,
                table_idx: 1,
            };

            let bit_cost = quant_tone_specs(specs, &mut component, huff_tables);
            if bit_cost == -0x8000 {
                return (-0x8000, group_count);
            }

            total_bits += bit_cost;

            tone_components_out.push(component);

            let ets_result = extract_tone_specs(
                specs,
                (i << 2) as u32,
                component.idsf as u32,
                7,
                3,
                &component.mantissas,
            );
            if ets_result == -1 {
                return (-0x8000, group_count);
            }

            // Update IDSF from residual
            let slice_start = i << 2;
            let slice_end = (slice_start + 4).min(specs.len());
            let peak = abs_max(&specs[slice_start..slice_end]);
            let new_idsf = idscfof_absval_at3(peak as f32);

            // descriptor_b: add delta (new - old) for the is_attack path
            if i < descriptors_b.len() {
                descriptors_b[i].idsf += new_idsf - descriptors_a[i].idsf;
            }
            // descriptor_a: set to new value
            if i < descriptors_a.len() {
                descriptors_a[i].idsf = new_idsf;
            }

            sg_counts[sg] += 1;
        }
    }

    if tone_components_out.is_empty() {
        group_count = 0;
    }

    (total_bits, group_count)
}

/// `set_cuidsf_from_spec` (`0x68014`): populates descriptor-table IDSF
/// from spectral data.
pub fn set_cuidsf_from_spec(specs: &[f32], descriptors: &mut [BinDescriptor], count: i32) {
    let mut s = 0usize;
    for i in 0..count as usize {
        if i >= descriptors.len() {
            break;
        }
        let end = (s + 4).min(specs.len());
        let peak = abs_max(&specs[s..end]) as f32;
        descriptors[i].idsf = idscfof_absval_at3(peak);
        s += 4;
    }
}

/// `set_quidsf_from_cuidsf` (`0x67f5c`): propagates per-bin IDSF to
/// per-BFU quantized IDSF.
pub fn set_quidsf_from_cuidsf(descriptors: &[BinDescriptor], idsf_out: &mut [i32], count: i32) {
    use crate::dsp::quant::ispof_iqt_at3;

    let mut i = 0i32;
    while i < count {
        let qt = ispof_iqt_at3(i as u32);
        let qt_div = if qt < 0 { (qt + 3) >> 2 } else { qt >> 2 };
        idsf_out[i as usize] = descriptors[qt_div as usize].idsf;

        let mut cur_div = qt_div + 1;
        let nqt_div = {
            let nqt = ispof_iqt_at3((i + 1) as u32);
            if nqt < 0 { (nqt + 3) >> 2 } else { nqt >> 2 }
        };

        while cur_div < nqt_div {
            let desc_idx = cur_div as usize;
            if desc_idx < descriptors.len() && idsf_out[i as usize] < descriptors[desc_idx].idsf {
                idsf_out[i as usize] = descriptors[desc_idx].idsf;
            }
            cur_div += 1;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_desc(idsf: i32) -> BinDescriptor {
        BinDescriptor {
            peak_value: 0,
            idsf,
            tone_template: [0; 16],
        }
    }

    #[test]
    fn is_attack_returns_false_for_uniform_values() {
        let values = [4i32; 16];
        assert!(!is_attack(&values));
    }

    #[test]
    fn is_attack_returns_true_for_large_spread() {
        let mut values = [0i32; 16];
        values[5] = 10;
        assert!(is_attack(&values));
    }

    #[test]
    fn is_attack_threshold_is_three() {
        let mut values = [4i32; 16];
        values[0] = 7;
        assert!(is_attack(&values));
        values[0] = 6;
        assert!(!is_attack(&values));
    }

    #[test]
    fn single_tone_check_returns_zero_for_flat() {
        let descriptors = vec![make_desc(0); NUM_BINS];
        assert_eq!(single_tone_check(&descriptors), 0);
    }

    #[test]
    fn single_tone_check_returns_two_for_strong_peak() {
        let mut descriptors = vec![make_desc(0); NUM_BINS];
        descriptors[128].idsf = 0x30;
        assert_eq!(single_tone_check(&descriptors), 2);
    }

    #[test]
    fn single_tone_check_returns_one_for_medium_peak() {
        let mut descriptors = vec![make_desc(0); NUM_BINS];
        descriptors[128].idsf = 0x18;
        assert_eq!(single_tone_check(&descriptors), 1);
    }

    #[test]
    fn extract_tone_specs_subtracts_template() {
        let mut residual = vec![100.0f32; 1024];
        let template = vec![1i32; 4];
        let result = extract_tone_specs(&mut residual, 10, 15, 7, 3, &template);
        assert_eq!(result, 0);
        let sf = scfof_id_at3(15);
        let nsteps = nstepsof_idwl_at3(7);
        let scale = sf / (nsteps as f64 + 0.5);
        for i in 0..4 {
            let expected = 100.0 - (scale * 1.0) as f32;
            assert!(
                (residual[10 + i] - expected).abs() < 1e-4,
                "residual[{}] = {} expected {}",
                10 + i,
                residual[10 + i],
                expected
            );
        }
    }

    #[test]
    fn extract_tone_specs_returns_neg1_for_invalid_idwl() {
        let mut residual = vec![100.0f32; 1024];
        let template = vec![1i32; 4];
        assert_eq!(
            extract_tone_specs(&mut residual, 10, 15, 8, 3, &template),
            -1
        );
    }

    #[test]
    fn quant_tone_specs_produces_valid_mantissas() {
        let huff_tables = HuffTableSet::build_tone();
        let mut component = ToneComponent {
            position: 0,
            idsf: 0,
            mantissas: [0; 8],
            width_id: 3,
            idwl: 7,
            table_idx: 0,
        };
        let specs = vec![1.0f32; 1024];
        let bits = quant_tone_specs(&specs, &mut component, &huff_tables);
        assert!(bits != -0x8000, "should succeed");
        assert!(component.idsf > 0, "idsf should be set");
        let width = twidof_id_at3(3) as usize;
        for i in 0..width {
            assert!(component.mantissas[i] >= -31 && component.mantissas[i] <= 31);
        }
    }

    #[test]
    fn tone_component_roundtrips_bytes() {
        let component = ToneComponent {
            position: 42,
            idsf: 15,
            mantissas: [1, 2, 3, 4, 5, 6, 7, 8],
            width_id: 3,
            idwl: 7,
            table_idx: 1,
        };
        let bytes = component.to_bytes();
        let back = ToneComponent::from_bytes(&bytes);
        assert_eq!(back.position, 42);
        assert_eq!(back.idsf, 15);
        assert_eq!(back.mantissas, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(back.width_id, 3);
        assert_eq!(back.idwl, 7);
        assert_eq!(back.table_idx, 1);
    }

    #[test]
    fn bin_descriptor_from_bytes_parses_correctly() {
        let mut bytes = vec![0u8; BIN_DESCRIPTOR_SIZE];
        bytes[0..4].copy_from_slice(&123i32.to_le_bytes());
        bytes[4..8].copy_from_slice(&45i32.to_le_bytes());
        bytes[8..12].copy_from_slice(&1i32.to_le_bytes());
        let desc = BinDescriptor::from_bytes(&bytes);
        assert_eq!(desc.peak_value, 123);
        assert_eq!(desc.idsf, 45);
        assert_eq!(desc.tone_template[0], 1);
        assert_eq!(desc.tone_template[1], 0);
    }
}
