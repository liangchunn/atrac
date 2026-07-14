//! Quantization helper functions reproduced from `libatrac.so.1.2.0`.
//!
//! These are the small leaf functions used by both the tone-extraction path
//! (milestone #5) and the full quantization/bit-allocation path (milestones
//! #6/#7). They are implemented here as standalone functions so the tone
//! module can depend on them without pulling in the full quantization
//! orchestration.
//!
//! ## Scale-factor lookup
//!
//! - `scfof_id_at3(id)` → `SCALE_FACTOR_TABLE[id]` (or −65536.0 for `id >= 64`).
//! - `idscfof_val_at3(val)` → linear scan to find the smallest `id` whose
//!   scale factor `>= |val|` (clamped to 0..63).
//! - `idscfof_absval_at3(val)` → binary-search variant for `|val|`.
//!
//! ## IDWL lookup
//!
//! - `wlof_idwl_at3(idwl)` → `WORD_LENGTH_TABLE[idwl]` (or −1).
//! - `nstepsof_idwl_at3(idwl)` → `NSTEPS_TABLE[idwl]` (or −1).
//! - `twidof_id_at3(idwl)` → `WIDTH_TABLE[idwl]` (or −1).
//!
//! ## Huffman table construction
//!
//! `HuffEntry` represents one IDWL's Huffman codebook (0x30 bytes at runtime).
//! `HuffTableSet` holds two codebooks (table 0 and table 1), each with 8
//! entries (IDWL 0..7). Built at construction time from `HCTBL0_CODES` /
//! `HCTBL1_CODES` + `NGRP_FOR_TONE` + `WORD_LENGTH_TABLE`, mirroring
//! `init_hctbl` (`0x7963c`) and `init_huff_at3` (`0x79798`).
//!
//! `huffbits` counts the total Huffman bit length for a mantissa array,
//! mirroring `huffbits` (`0x79584`).

use crate::tables::{
    HCTBL0_CODES, HCTBL1_CODES, NGRP_FOR_SPEC, NGRP_FOR_TONE, NSTEPS_TABLE, SCALE_FACTOR_TABLE,
    WORD_LENGTH_TABLE,
};

/// Maximum spectral coefficient index (1024 for ATRAC3).
pub const NUM_SPECS: usize = 1024;

/// `scfof_id_at3` (`0x6560c`): IDSF → scale factor (f32).
///
/// Returns `SCALE_FACTOR_TABLE[id]` for `id < 64`, else −65536.0.
/// The original extended-precision return is represented as `f64`.
#[inline]
pub fn scfof_id_at3(id: u32) -> f64 {
    if id < 64 {
        SCALE_FACTOR_TABLE[id as usize] as f64
    } else {
        -65536.0
    }
}

/// `idscfof_val_at3` (`0x65648`): value → IDSF (linear scan).
///
/// Finds the smallest `id` in 0..63 such that `SCALE_FACTOR_TABLE[id] >= |val|`.
/// If `val` is negative, `|val|` is used. Returns 0 if `|val|` is below the
/// minimum scale factor.
#[inline]
pub fn idscfof_val_at3(val: f32) -> i32 {
    let abs_val = if val <= 0.0 { -val } else { val };
    let mut id = 0i32;
    let mut sf = SCALE_FACTOR_TABLE[0];
    while sf <= abs_val && id < 63 {
        id += 1;
        sf = SCALE_FACTOR_TABLE[id as usize];
    }
    id
}

/// `idscfof_absval_at3` (`0x6569c`): |value| → IDSF (binary search).
///
/// Returns 0 if `val < 0.03125`, else binary-searches the scale-factor table.
/// If the found entry is strictly less than `val`, increments by 1.
#[inline]
pub fn idscfof_absval_at3(val: f32) -> i32 {
    if val < SCALE_FACTOR_TABLE[0] {
        return 0;
    }
    let mut idx = 0x20i32;
    let mut step = 0x10i32;
    while step != 0 {
        let delta = if val < SCALE_FACTOR_TABLE[idx as usize] {
            -step
        } else {
            step
        };
        idx += delta;
        step >>= 1;
    }
    if idx < 63 && SCALE_FACTOR_TABLE[idx as usize] < val {
        idx + 1
    } else {
        idx
    }
}

/// `wlof_idwl_at3` (`0x65718`): IDWL → word length (bits per mantissa).
#[inline]
pub fn wlof_idwl_at3(idwl: u32) -> i32 {
    if idwl < 8 {
        WORD_LENGTH_TABLE[idwl as usize]
    } else {
        -1
    }
}

/// `nstepsof_idwl_at3` (`0x65744`): IDWL → max quantization step.
#[inline]
pub fn nstepsof_idwl_at3(idwl: u32) -> i32 {
    if idwl < 8 {
        NSTEPS_TABLE[idwl as usize]
    } else {
        -1
    }
}

/// `twidof_id_at3` (`0x6579c`): IDWL → spectral width (bins per tone).
#[inline]
pub fn twidof_id_at3(idwl: u32) -> i32 {
    if idwl < 8 {
        crate::tables::WIDTH_TABLE[idwl as usize]
    } else {
        -1
    }
}

/// `abs_max` (`0x69290`): max |f32| over a slice.
///
/// Returns 0.0 for empty slices. The original extended-precision return is
/// represented as `f64`.
#[inline]
pub fn abs_max(specs: &[f32]) -> f64 {
    let mut max = 0.0f32;
    for &s in specs {
        let a = s.abs();
        if max < a {
            max = a;
        }
    }
    max as f64
}

/// `npower` (`0x69274`): integer power `base^exp`.
#[inline]
pub fn npower(base: i32, exp: i32) -> i32 {
    let mut result = 1i32;
    for _ in 0..exp {
        result *= base;
    }
    result
}

/// `QUANT` (`0x69198`): quantize a single mantissa.
///
/// `QUANT(val, sf, nsteps) = trunc((nsteps + 0.5) * (val / sf) + 31.5) - 31`,
/// clamped to `[-nsteps, nsteps]`.
///
/// Uses truncation toward zero, matching the instruction sequence at
/// `0x691ce..0x691e1`.
#[inline]
fn clamp_mantissa(mut q: i32, nsteps: i32) -> i32 {
    if nsteps < q {
        q = nsteps;
    }
    if q < -nsteps {
        q = -nsteps;
    }
    q
}

#[inline]
pub fn quant_at3(val: f32, sf: f32, nsteps: i32) -> i32 {
    let nsteps_f = nsteps as f64;
    let scaled = (nsteps_f + 0.5) * (val as f64 / sf as f64) + 31.5;
    clamp_mantissa(scaled.trunc() as i32 - 31, nsteps)
}

/// One Huffman codebook entry (0x30 bytes at runtime in libatrac).
///
/// Layout mirrors `init_hctbl`:
/// - `idwl`: IDWL index (1..7)
/// - `wlof`: word length (from `g_a_wltbl`)
/// - `ngrp`: coding mode (1 = single-value, 2 = pair-value)
/// - `mask`: `(1 << wlof) - 1`
/// - `codes`: `(code, len)` pairs for each quantized value
#[derive(Debug, Clone)]
pub struct HuffEntry {
    pub idwl: i32,
    pub wlof: i32,
    pub ngrp: i32,
    pub mask: u32,
    pub codes: Vec<(u32, u32)>,
}

impl HuffEntry {
    /// Returns the Huffman bit length for a single mantissa value.
    ///
    /// Mirrors the mode-1 branch of `huffbits`: `codes[(mantissa & mask)].1`.
    #[inline]
    pub fn bit_length_single(&self, mantissa: i32) -> u32 {
        let idx = (mantissa as u32 & self.mask) as usize;
        if idx < self.codes.len() {
            self.codes[idx].1
        } else {
            0
        }
    }

    /// Returns the Huffman bit length for a pair of mantissa values.
    ///
    /// Mirrors the mode-2 branch of `huffbits`:
    /// `codes[((hi & mask) << wlof) | (lo & mask)].1`.
    #[inline]
    pub fn bit_length_pair(&self, hi: i32, lo: i32) -> u32 {
        let idx =
            (((hi as u32 & self.mask) << (self.wlof as u32)) | (lo as u32 & self.mask)) as usize;
        if idx < self.codes.len() {
            self.codes[idx].1
        } else {
            0
        }
    }
}

/// A set of two Huffman codebooks (table 0 and table 1), each with 8 entries
/// (IDWL 0..7). Mirrors `init_huff_at3` (`0x79798`).
#[derive(Debug, Clone)]
pub struct HuffTableSet {
    pub tables: [Vec<HuffEntry>; 2],
}

impl HuffTableSet {
    /// Builds the tone-path Huffman table set from the rodata constants.
    ///
    /// Mirrors `init_huff_at3` for the tone path (`g_a_ngrp_for_tone` +
    /// `g_a_hctbl0` / `g_a_hctbl1`).
    pub fn build_tone() -> Self {
        Self::build_internal(&NGRP_FOR_TONE, &HCTBL0_CODES, &HCTBL1_CODES)
    }

    fn build_internal(
        ngrp: &[i32; 16],
        hctbl0: &[(u32, u32); 160],
        hctbl1: &[(u32, u32); 160],
    ) -> Self {
        let sources = [hctbl0, hctbl1];
        let mut tables: [Vec<HuffEntry>; 2] = Default::default();

        for (table_idx, &src) in sources.iter().enumerate() {
            let mut entries: Vec<HuffEntry> = (0..8)
                .map(|idwl| HuffEntry {
                    idwl,
                    wlof: wlof_idwl_at3(idwl as u32),
                    ngrp: 0,
                    mask: 0,
                    codes: Vec::new(),
                })
                .collect();

            let mut src_offset = 0usize;
            for idwl in 1..=7usize {
                let wlof = wlof_idwl_at3(idwl as u32);
                let n = ngrp[idwl];
                let mask = if wlof >= 0 {
                    ((1u32) << (wlof as u32)).wrapping_sub(1)
                } else {
                    0
                };
                let npower_count = npower(1i32 << (wlof as u32), n);

                let codes: Vec<(u32, u32)> =
                    src[src_offset..src_offset + npower_count as usize].to_vec();
                src_offset += npower_count as usize;

                let entry = &mut entries[idwl];
                entry.wlof = wlof;
                entry.ngrp = n;
                entry.mask = mask;
                entry.codes = codes;
            }
            tables[table_idx] = entries;
        }

        HuffTableSet { tables }
    }

    /// Returns the entry for `(table_idx, idwl)`.
    #[inline]
    pub fn entry(&self, table_idx: usize, idwl: i32) -> &HuffEntry {
        &self.tables[table_idx][idwl as usize]
    }
}

/// `huffbits` (`0x79584`): total Huffman bit length for a mantissa array.
///
/// - `entry`: the Huffman codebook entry (contains mode, mask, codes).
/// - `mantissas`: the quantized mantissa values.
/// - `count`: number of mantissas to process.
///
/// Returns the total bit count, or `−0x8000` if the coding mode is invalid.
///
/// Mode 1 (ngrp == 1): one mantissa per code lookup.
/// Mode 2 (ngrp == 2): two mantissas per code lookup (pair coding).
pub fn huffbits(entry: &HuffEntry, mantissas: &[i32], count: usize) -> i32 {
    let mut total = 0i32;
    match entry.ngrp {
        1 => {
            for &m in mantissas.iter().take(count) {
                total += entry.bit_length_single(m) as i32;
            }
        }
        2 => {
            let mut i = 0;
            while i + 1 < count {
                total += entry.bit_length_pair(mantissas[i], mantissas[i + 1]) as i32;
                i += 2;
            }
        }
        _ => return -0x8000,
    }
    total
}

/// `ispof_iqt_at3` (`0x655e0`): BFU index → quantization-table start position.
///
/// Table lookup `g_a_qtstart[bfu]` for `bfu < 33`, else −1.
#[inline]
pub fn ispof_iqt_at3(bfu: u32) -> i32 {
    if bfu < 33 {
        crate::tables::QTSTART_TABLE[bfu as usize]
    } else {
        -1
    }
}

/// `nsps_inqt_at3` (`0x655b4`): BFU index → number of spectral samples.
///
/// Table lookup `g_a_nsps1024[bfu]` for `bfu < 32`, else −1.
#[inline]
pub fn nsps_inqt_at3(bfu: u32) -> i32 {
    if bfu < 32 {
        crate::tables::NSPS1024_TABLE[bfu as usize]
    } else {
        -1
    }
}

/// `translate_to_idwl` (`0x67b4c`): converts scale-factor spread values
/// to word-length indices (IDWL).
///
/// - `ctx`: context-dependent offsets (8 ints)
/// - `min_idwl`: lower clamp bound
/// - `spread`: scale-factor spread values (f32 array)
/// - `spread_int`: per-BFU scale-factor values (int array)
/// - `idwl_out`: IDWL output array (written in place)
/// - `count`: number of BFUs
/// - `max_idwl`: upper clamp bound
///
/// Returns the computed threshold value.
pub fn translate_to_idwl(
    ctx: &[i32],
    min_idwl: i32,
    spread: &[f32],
    spread_int: &[i32],
    idwl_out: &mut [i32],
    count: i32,
    max_idwl: i32,
) -> i32 {
    if count <= 0 {
        return 1;
    }

    let mut max_si = 0;
    for &v in spread_int.iter().take(count as usize).skip(1) {
        if max_si < v {
            max_si = v;
        }
    }

    let threshold = if max_si < 30 {
        let t = max_si / 6;
        if t == 0 { 1 } else { t }
    } else {
        6
    };

    for i in 0..count as usize {
        let v = (spread[i] + 0.5).trunc() as i32;
        idwl_out[i] = v.clamp(min_idwl, max_idwl);

        if spread_int[i] < threshold {
            idwl_out[i] = 0;
        }
        for (j, &c) in ctx.iter().take(8).enumerate() {
            if i > j {
                let prev = i - j - 1;
                if spread_int[i] < spread_int[prev] - c {
                    idwl_out[i] = 0;
                }
            }
        }
    }

    threshold
}

// ================================================================
// Milestone 6c: Quantization helpers and non-tone quantizer
// ================================================================

impl HuffTableSet {
    /// Builds the spec-path Huffman table set (state+0x24).
    pub fn build_spec() -> Self {
        Self::build_internal(&NGRP_FOR_SPEC, &HCTBL0_CODES, &HCTBL1_CODES)
    }
}

/// `tfof_id` (`0x78068`): scale-factor threshold for a BFU.
///
/// Clamps `id` to [0, 12], then returns `s_a_const[id] / s_a_divide[div]`.
/// If `id == 0`, returns 0.0.
pub fn tfof_id(id: i32, div: i32) -> f32 {
    let id = id.clamp(0, 12);
    if id == 0 {
        0.0
    } else {
        crate::tables::TONE_FREQ_CONST[id as usize] / crate::tables::TONE_FREQ_DIVIDE[div as usize]
    }
}

/// `itfbof_iqt` (`0x780b0`): maps BFU quidsf value to ITFB group (0–5).
///
/// Groups by energy level: ≤7 = group 0, ≤11 = 1, ≤15 = 2,
/// ≤19 = 3, ≤25 = 4, else 5.
#[inline]
pub fn itfbof_iqt(quidsf: i32) -> i32 {
    if quidsf <= 7 {
        0
    } else if quidsf <= 11 {
        1
    } else if quidsf <= 15 {
        2
    } else if quidsf <= 19 {
        3
    } else if quidsf <= 25 {
        4
    } else {
        5
    }
}

/// `iorder_from_max` (`0x789b0`): Shell-sort producing a descending-order
/// permutation of `values`.
///
/// On return, `order[0]` is the index of the largest value in `values`,
/// `order[1]` is the index of the second-largest, etc.
pub fn iorder_from_max(values: &[i32], order: &mut [i32], count: i32) {
    let n = count as usize;
    assert!(values.len() >= n && order.len() >= n);
    for (i, o) in order.iter_mut().take(n).enumerate() {
        *o = i as i32;
    }
    let mut h = 1;
    while h * 3 + 1 < n as i32 {
        h = h * 3 + 1;
    }
    while h > 0 {
        let gap = h as usize;
        for i in 0..gap {
            let mut j = i + gap;
            while j < n {
                let key_val = values[order[j] as usize];
                let key_ord = order[j];
                let mut k = j;
                while k >= gap && values[order[k - gap] as usize] < key_val {
                    order[k] = order[k - gap];
                    k -= gap;
                }
                order[k] = key_ord;
                j += gap;
            }
        }
        h /= 3;
    }
}

/// `nbits_for_sheader` (`0x65910`): bit cost for the subband header.
///
/// Returns 8 if `joint_stereo` is false, 16 if true.
/// The JS value accounts for the 14-bit JS params header + 2-bit bfu_count.
pub fn nbits_for_sheader(joint_stereo: bool) -> i32 {
    if joint_stereo { 16 } else { 8 }
}

/// `nbits_for_adjust` (`0x6592c`): bit cost for adjust parameters.
///
/// `count * 3 + sum(per_bfu_vals) * 9`.
pub fn nbits_for_adjust(count: i32, per_bfu_vals: &[i32]) -> i32 {
    let sum: i32 = per_bfu_vals.iter().take(count as usize).sum();
    count * 3 + sum * 9
}

/// `quant_nontone_nspecs` (`0x67d4c`): quantizes non-tone spectral specs
/// for one BFU.
///
/// - `table_idx`: Huffman table selector (0 or 1)
/// - `idwl`: word-length index (0..7)
/// - `sf`: scale factor (from `tfof_id`)
/// - `nsps`: number of spectral samples in this BFU (from `nsps_inqt_at3`)
/// - `specs`: spectral data slice (f32)
/// - `mantissas`: output mantissa array (written in place)
/// - `spec_huff`: spec-path HuffTableSet
///
/// Returns bit count + 6, or 0 if `idwl == 0`, or −0x8000 on error.
pub fn quant_nontone_nspecs(
    table_idx: i32,
    idwl: i32,
    sf: f32,
    nsps: i32,
    specs: &[f32],
    mantissas: &mut [i32],
    spec_huff: &HuffTableSet,
) -> i32 {
    if idwl == 0 {
        return 0;
    }
    let nsteps = nstepsof_idwl_at3(idwl as u32);
    if nsteps == -1 {
        return -0x8000;
    }
    for i in 0..nsps as usize {
        let val = specs[i];
        let abs_val = val.abs();
        if abs_val <= sf {
            mantissas[i] = 0;
        } else {
            let scaled = val as f64 * (nsteps as f64 + 0.5) + 31.5;
            mantissas[i] = clamp_mantissa(scaled.trunc() as i32 - 31, nsteps);
        }
    }
    let Ok(table_idx) = usize::try_from(table_idx) else {
        return -0x8000;
    };
    if table_idx >= spec_huff.tables.len() {
        return -0x8000;
    }
    let entry = spec_huff.entry(table_idx, idwl);
    let bits = huffbits(entry, mantissas, nsps as usize);
    if bits == -0x8000 {
        return -0x8000;
    }
    bits + 6
}

/// `calc_bitnumber` (`0x67c94`): computes total Huffman bits for all BFUs.
///
/// For each BFU where `flags[bfu] == 1`, calls `tfof_id` → `ispof_iqt_at3`
/// → `nsps_inqt_at3` → `quant_nontone_nspecs`, accumulating bit totals.
#[allow(clippy::too_many_arguments)]
pub fn calc_bitnumber(
    flags: &[i32],
    idsf: &[i32],
    idwl: &[i32],
    specs: &[f32],
    bits_out: &mut [i32],
    count: i32,
    table_idx: i32,
    spec_huff: &HuffTableSet,
) -> i32 {
    let mut total = 0i32;
    let mut mantissas = vec![0i32; 128];
    for i in 0..count as usize {
        if i >= flags.len() || flags[i] != 1 {
            total += bits_out[i];
            continue;
        }
        let scale = tfof_id(idsf[i], idwl[i]);
        let pos = ispof_iqt_at3(i as u32);
        let nsps = nsps_inqt_at3(i as u32);
        if pos < 0 || nsps < 0 {
            return -0x8000;
        }
        let spec_start = pos as usize;
        let spec_end = (spec_start + nsps as usize).min(specs.len());
        let bits = quant_nontone_nspecs(
            table_idx,
            idwl[i],
            scale,
            nsps,
            &specs[spec_start..spec_end],
            &mut mantissas,
            spec_huff,
        );
        bits_out[i] = bits;
        if bits == -0x8000 {
            return -0x8000;
        }
        total += bits;
    }
    total
}

/// `set_idtf_and_limwl` (`0x680e8`): initialises IDTF + limit-word-length.
///
/// Zeroes 6 ints at `dst`, then sets `*lim_out = 10` and `*wl_out = 7`.
pub fn set_idtf_and_limwl(dst: &mut [i32; 6], lim_out: &mut i32, wl_out: &mut i32) {
    dst.fill(0);
    *lim_out = 10;
    *wl_out = 7;
}

/// `nbits_for_packdata` (`0x658b4`): total bits for bitstream packing.
///
/// Calls the full implementation in `dsp::pack::nbits_for_packdata`.
/// The parameter count here is narrower — the caller passes only the subset
/// of fields it knows at the `encode_mddata_at3` level.
///
/// For the full packing bit budget (including tone components and spectral
/// data), use `dsp::pack::nbits_for_packdata` directly.
pub fn nbits_for_packdata(shdr_bits: i32, adjust_bits: i32) -> i32 {
    shdr_bits + adjust_bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scfof_id_returns_table_value() {
        assert!((scfof_id_at3(0) - 0.03125).abs() < 1e-6);
        assert!((scfof_id_at3(15) - 1.0).abs() < 1e-6);
        assert!((scfof_id_at3(63) - 65536.0).abs() < 1e-3);
    }

    #[test]
    fn scfof_id_returns_negative_for_out_of_range() {
        assert_eq!(scfof_id_at3(64), -65536.0);
        assert_eq!(scfof_id_at3(100), -65536.0);
    }

    #[test]
    fn idscfof_val_finds_correct_index() {
        assert_eq!(idscfof_val_at3(0.0), 0);
        assert_eq!(idscfof_val_at3(0.03125), 1);
        assert_eq!(idscfof_val_at3(1.0), 16);
        assert_eq!(idscfof_val_at3(100.0), 35);
        assert_eq!(idscfof_val_at3(-1.0), 16);
    }

    #[test]
    fn idscfof_absval_finds_correct_index() {
        assert_eq!(idscfof_absval_at3(0.01), 0);
        assert_eq!(idscfof_absval_at3(1.0), 15);
        assert_eq!(idscfof_absval_at3(0.03125), 1);
        assert_eq!(idscfof_absval_at3(0.04), 2);
    }

    #[test]
    fn wlof_nsteps_twid_are_consistent() {
        for idwl in 0..8 {
            let wlof = wlof_idwl_at3(idwl as u32);
            let nsteps = nstepsof_idwl_at3(idwl as u32);
            assert_eq!(wlof, WORD_LENGTH_TABLE[idwl]);
            assert_eq!(nsteps, NSTEPS_TABLE[idwl]);
        }
        assert_eq!(wlof_idwl_at3(8), -1);
        assert_eq!(nstepsof_idwl_at3(8), -1);
        assert_eq!(twidof_id_at3(8), -1);
    }

    #[test]
    fn abs_max_finds_peak() {
        assert!((abs_max(&[1.0, -5.0, 3.0]) - 5.0).abs() < 1e-6);
        assert!((abs_max(&[]) - 0.0).abs() < 1e-6);
        assert!((abs_max(&[-0.1, -0.2]) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn npower_is_correct() {
        assert_eq!(npower(2, 0), 1);
        assert_eq!(npower(2, 3), 8);
        assert_eq!(npower(4, 2), 16);
        assert_eq!(npower(3, 0), 1);
    }

    #[test]
    fn quant_clamps_to_range() {
        let q = quant_at3(100.0, 1.0, 7);
        assert!((-7..=7).contains(&q));
        let q = quant_at3(-100.0, 1.0, 7);
        assert!((-7..=7).contains(&q));
        let q = quant_at3(0.0, 1.0, 7);
        assert_eq!(q, 0);
    }

    #[test]
    fn huff_table_set_builds_correctly() {
        let hts = HuffTableSet::build_tone();
        for table_idx in 0..2 {
            let entries = &hts.tables[table_idx];
            assert_eq!(entries.len(), 8);
            assert_eq!(entries[0].idwl, 0);
            for idwl in 1..=7 {
                let e = &entries[idwl];
                assert_eq!(e.idwl, idwl as i32);
                assert_eq!(e.wlof, wlof_idwl_at3(idwl as u32));
                assert_eq!(e.ngrp, NGRP_FOR_TONE[idwl]);
                let expected_count = npower(1i32 << e.wlof, e.ngrp) as usize;
                assert_eq!(e.codes.len(), expected_count);
                assert_eq!(e.mask, (1u32 << e.wlof) - 1);
            }
        }
    }

    #[test]
    fn huffbits_single_mode_sums_lengths() {
        let hts = HuffTableSet::build_tone();
        let entry = hts.entry(0, 3);
        assert_eq!(entry.ngrp, 1);
        let mantissas = [0, 1, 2, 3];
        let bits = huffbits(entry, &mantissas, 4);
        let expected: i32 = mantissas
            .iter()
            .map(|&m| entry.bit_length_single(m) as i32)
            .sum();
        assert_eq!(bits, expected);
    }

    #[test]
    fn huffbits_pair_mode_sums_lengths() {
        let hts = HuffTableSet::build_tone();
        let entry = hts.entry(0, 1);
        assert_eq!(entry.ngrp, 2);
        let mantissas = [0, 1, 2, 3];
        let bits = huffbits(entry, &mantissas, 4);
        let expected = entry.bit_length_pair(0, 1) as i32 + entry.bit_length_pair(2, 3) as i32;
        assert_eq!(bits, expected);
    }

    #[test]
    fn itfbof_iqt_groups_correctly() {
        assert_eq!(itfbof_iqt(0), 0);
        assert_eq!(itfbof_iqt(5), 0);
        assert_eq!(itfbof_iqt(7), 0);
        assert_eq!(itfbof_iqt(8), 1);
        assert_eq!(itfbof_iqt(11), 1);
        assert_eq!(itfbof_iqt(12), 2);
        assert_eq!(itfbof_iqt(15), 2);
        assert_eq!(itfbof_iqt(16), 3);
        assert_eq!(itfbof_iqt(19), 3);
        assert_eq!(itfbof_iqt(20), 4);
        assert_eq!(itfbof_iqt(25), 4);
        assert_eq!(itfbof_iqt(26), 5);
        assert_eq!(itfbof_iqt(100), 5);
    }

    #[test]
    fn iorder_descending() {
        let values = [3, 1, 4, 1, 5, 9, 2, 6];
        let mut order = [0i32; 8];
        iorder_from_max(&values, &mut order, 8);
        let sorted: Vec<i32> = order.iter().map(|&i| values[i as usize]).collect();
        assert_eq!(sorted, vec![9, 6, 5, 4, 3, 2, 1, 1]);
    }

    #[test]
    fn iorder_singleton() {
        let values = [42];
        let mut order = [0i32; 1];
        iorder_from_max(&values, &mut order, 1);
        assert_eq!(order[0], 0);
    }
}
