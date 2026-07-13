//! Gain control for the classic ATRAC3 encoder.
//!
//! Reproduces the behaviour of `libatrac.so.1.2.0`:
//! - `gaincontrol_at3` (`0x6b504`): 4-band loop calling `set_gainc` then
//!   `adjust_gainc`.
//! - `gainc_window` (`0x692f8`): computes 512 per-sample scale values from
//!   two `GainInfo` structs (current + next), then `forward_transform_at3`
//!   divides the original samples by these scales to produce the MDCT input.
//! - `lngainof_id_at3` (`0x657c8`): level index → exponent (`i − 4`).
//!
//! ## Level mapping
//!
//! Level index `0..15` maps to exponent `level_index − 4` via `LNGAIN`.
//! The linear gain is `2^exponent`. The encode side uses ascending gains
//! (`2^(i−4)`, `0.0625 … 2048.0`); the decode side uses descending gains
//! (`2^(4−i)`, `16.0 … 0.000488`). The two are reciprocals.
//!
//! ## GainInfo struct
//!
//! The 64-byte (`0x40`) per-band gain metadata struct, matching the memory
//! layout used by `libatrac.so`:
//!
//! ```text
//! offset  field        type
//! 0x00    count        int32  (0..7)
//! 0x04    location[7]  int32  (units of 8 samples within a 256-sample half)
//! 0x20    level[8]     int32  (level index 0..15)
//! ```

use crate::tables::{GAIN_INTERPOLATION_DECODE, GAIN_LEVEL_DECODE, LNGAIN_EXPONENTS};

/// Number of QMF subbands.
pub const BAND_COUNT: usize = 4;
/// Samples per QMF subband per frame.
pub const BAND_SAMPLES: usize = 256;
/// MDCT block size (overlap + current).
pub const MDCT_SIZE: usize = 512;
/// Number of 8-sample blocks per 256-sample half.
pub const BLOCKS_PER_HALF: usize = 32;
/// Max gain points per band (libatrac returns −1 if count > 7).
pub const MAX_GAIN_POINTS: usize = 7;
/// Location scale: 8 samples per location unit.
pub const LOC_SCALE: u32 = 3;
/// Location size: 8 samples.
pub const LOC_SZ: usize = 8;
/// Exponent offset (codex `EXPONENT_OFFSET`): level index 4 → exponent 0 →
/// gain 1.0.
pub const EXPONENT_OFFSET: i32 = 4;

/// One gain control point: a level index and a location.
///
/// `level` is a level index `0..15` (maps to exponent `level − 4` via
/// `LNGAIN`). `location` is in units of `LOC_SZ = 8` samples within a
/// 256-sample half-frame.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GainPoint {
    pub level: u32,
    pub location: u32,
}

/// Per-band gain metadata matching the 64-byte `GainInfo` struct in
/// `libatrac.so`.
///
/// Fields use `i32` to match the binary's memory layout (the struct is
/// passed by value on the stack in `gainc_window`).
#[derive(Debug, Clone)]
pub struct GainInfo {
    pub count: i32,
    pub location: [i32; 7],
    pub level: [i32; 8],
}

impl GainInfo {
    pub fn new() -> Self {
        Self {
            count: 0,
            location: [0; 7],
            level: [0; 8],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count <= 0
    }

    pub fn len(&self) -> usize {
        self.count.clamp(0, MAX_GAIN_POINTS as i32) as usize
    }

    /// Returns the gain points as `(level, location)` pairs.
    pub fn points(&self) -> Vec<GainPoint> {
        (0..self.len())
            .map(|i| GainPoint {
                level: self.level[i] as u32,
                location: self.location[i] as u32,
            })
            .collect()
    }

    /// Builds a `GainInfo` from a slice of `GainPoint`s (max 7).
    pub fn from_points(points: &[GainPoint]) -> Self {
        let count = points.len().min(MAX_GAIN_POINTS) as i32;
        let mut info = Self::new();
        info.count = count;
        for (i, p) in points.iter().take(MAX_GAIN_POINTS).enumerate() {
            info.location[i] = p.location as i32;
            info.level[i] = p.level as i32;
        }
        info
    }

    pub fn peak_history(&self) -> f32 {
        f32::from_bits(self.level[7] as u32)
    }

    pub fn set_peak_history(&mut self, peak: f32) {
        self.level[7] = peak.to_bits() as i32;
    }
}

impl Default for GainInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Container for 4 bands of `GainInfo` (current + next schedules).
#[derive(Debug, Clone)]
pub struct SubbandInfo {
    pub current: [GainInfo; BAND_COUNT],
    pub next: [GainInfo; BAND_COUNT],
}

impl SubbandInfo {
    pub fn new() -> Self {
        Self {
            current: core::array::from_fn(|_| GainInfo::new()),
            next: core::array::from_fn(|_| GainInfo::new()),
        }
    }

    pub fn reset(&mut self) {
        self.current = core::array::from_fn(|_| GainInfo::new());
        self.next = core::array::from_fn(|_| GainInfo::new());
    }

    pub fn is_band_empty(&self, band: usize) -> bool {
        self.current[band].is_empty() && self.next[band].is_empty()
    }
}

impl Default for SubbandInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a level index to its linear gain value (encode-side, ascending).
///
/// Equivalent to `lngainof_id_at3` followed by `2^exponent`:
/// `GAIN_LEVEL_ENCODE[level] = 2^(level - 4)`.
#[inline]
pub fn level_to_gain_encode(level: u32) -> f32 {
    if level >= 16 {
        return 0.0;
    }
    let exp = LNGAIN_EXPONENTS[level as usize];
    (2.0_f32).powi(exp)
}

/// Converts a level index to its linear gain value (decode-side, descending).
///
/// `GAIN_LEVEL_DECODE[level] = 2^(4 - level)`. Matches codex `GAIN_LEVEL`.
#[inline]
pub fn level_to_gain_decode(level: u32) -> f32 {
    if level >= 16 {
        return 0.0;
    }
    GAIN_LEVEL_DECODE[level as usize]
}

/// Converts a linear gain exponent to its level index.
///
/// Equivalent to `idof_lngain_at3`: linear scan of `LNGAIN_EXPONENTS`.
/// Returns `None` if the exponent is not in the table.
#[inline]
pub fn exponent_to_level(exponent: i32) -> Option<u32> {
    LNGAIN_EXPONENTS
        .iter()
        .position(|&e| e == exponent)
        .map(|i| i as u32)
}

/// Gain processor implementing `gainc_window` (`0x692f8`) scale computation.
///
/// The processor builds a 64-entry per-block exponent array from the current
/// and next `GainInfo` schedules, then converts it to 512 per-sample scale
/// values. The caller (forward transform) divides the original samples by
/// these scales to produce the gain-modulated MDCT input.
pub struct GainProcessor;

impl GainProcessor {
    /// Reimplementation of libatrac `gaincontrol_at3` (`0x6b504`).
    pub fn gaincontrol_at3(
        bands: [&[f32]; BAND_COUNT],
        current: &[GainInfo; BAND_COUNT],
        next: &mut [GainInfo; BAND_COUNT],
    ) -> bool {
        for band in 0..BAND_COUNT {
            if !Self::set_gainc_with_size(bands[band], MDCT_SIZE, &current[band], &mut next[band]) {
                return false;
            }
        }

        Self::adjust_gainc(&bands, MDCT_SIZE, current, next)
    }

    /// Reimplementation of libatrac `set_gainc` (`0x6a190`).
    pub fn set_gainc(samples: &[f32], previous: &GainInfo, output: &mut GainInfo) -> bool {
        Self::set_gainc_with_size(samples, samples.len(), previous, output)
    }

    pub fn set_gainc_with_size(
        samples: &[f32],
        size: usize,
        previous: &GainInfo,
        output: &mut GainInfo,
    ) -> bool {
        if size == 0 {
            *output = GainInfo::new();
            return true;
        }

        let block = size.div_ceil(64);
        let half = size / 2;
        let mut envelope = [0.0f32; 64];
        let mut pos = half;
        for out in &mut envelope {
            let mut peak = 0.0f32;
            for j in 0..block {
                let idx = pos + j;
                if idx < samples.len() {
                    peak = peak.max(samples[idx].abs());
                }
            }
            *out = peak;
            pos = pos.saturating_add(block);
        }

        let mut tail_peak = 0.0f32;
        let tail_start = half.saturating_sub(block * 4);
        for &s in samples.iter().take(half).skip(tail_start) {
            tail_peak = tail_peak.max(s.abs());
        }

        let mut max_prev_exp = 0i32;
        for i in 0..previous.len() {
            let level = previous.level[i];
            if !(0..16).contains(&level) {
                return false;
            }
            max_prev_exp = max_prev_exp.max(LNGAIN_EXPONENTS[level as usize]);
        }

        let mut score = [0.0f32; 40];
        let mut selected = [false; 40];
        let mut candidate_count = 0usize;

        let mut running_peak = previous.peak_history();
        for i in 0..32 {
            running_peak = running_peak.max(envelope[i]);
            let next = envelope[i + 1];
            if next > 10.0 && running_peak * 1.5 < next {
                let ratio = if running_peak <= 4.0 {
                    next * 0.25
                } else {
                    next / running_peak
                };
                score[i] = ratio;
                candidate_count += 1;
            }
        }

        let mut group_peak = [0.0f32; 16];
        for (g, dst) in group_peak.iter_mut().enumerate() {
            let start = g * 4;
            *dst = envelope[start..start + 4]
                .iter()
                .copied()
                .fold(0.0, f32::max);
        }

        let mut release_running = group_peak[8..16].iter().copied().fold(0.0, f32::max);
        for g in (0..8).rev() {
            release_running = release_running.max(group_peak[g]);
            let prev = if g == 0 { tail_peak } else { group_peak[g - 1] };
            if prev > 10.0 && release_running * 1.85 < prev {
                let ratio = if release_running <= 4.0 {
                    prev * 0.25
                } else {
                    prev / release_running
                };
                score[32 + g] = ratio;
                candidate_count += 1;
            }
        }

        let keep = candidate_count.min(MAX_GAIN_POINTS);
        for _ in 0..keep {
            let mut best_idx = 0usize;
            let mut best = 0.0f32;
            for (i, &v) in score.iter().enumerate() {
                if best < v {
                    best = v;
                    best_idx = i;
                }
            }
            score[best_idx] = 0.0;
            selected[best_idx] = true;
        }

        let mut attack_locs = [0i32; 7];
        let mut attack_delta = [0i32; 7];
        let mut attack_count = 0usize;
        let mut exp_budget = max_prev_exp;
        running_peak = previous.peak_history();
        for i in 0..32 {
            running_peak = running_peak.max(envelope[i]);
            let next = envelope[i + 1];
            if next > 10.0 && running_peak * 1.5 < next && selected[i] {
                let ratio = if running_peak <= 4.0 {
                    next * 0.25
                } else {
                    next / running_peak
                };
                let mut delta = log2_delta(ratio);
                if delta < 0 {
                    delta = 0;
                } else {
                    if exp_budget + delta > 10 {
                        delta = 10 - exp_budget;
                    }
                    exp_budget += delta;
                }
                if delta > 0 && attack_count < 7 {
                    attack_locs[attack_count] = i as i32;
                    attack_delta[attack_count] = delta;
                    attack_count += 1;
                }
                if attack_count > 6 {
                    break;
                }
            }
        }

        let release_limit = exp_budget.min(4);
        let mut release_locs = [0i32; 7];
        let mut release_delta = [0i32; 7];
        let mut release_count = 0usize;
        if attack_count > 0 || previous.count > 0 {
            let mut release_acc = 0i32;
            let mut f1 = group_peak[8..16].iter().copied().fold(0.0, f32::max);
            for g in (0..8).rev() {
                f1 = f1.max(group_peak[g]);
                let prev = if g == 0 { tail_peak } else { group_peak[g - 1] };
                if prev > 10.0 && f1 * 1.85 < prev && selected[32 + g] {
                    let ratio = if f1 <= 4.0 { prev * 0.25 } else { prev / f1 };
                    let mut delta = log2_delta(ratio);
                    if delta < 0 {
                        delta = 0;
                    } else {
                        if release_acc + delta > release_limit {
                            delta = release_limit - release_acc;
                        }
                        release_acc += delta;
                    }
                    if delta > 0 && attack_count < 7 && release_count < 7 {
                        release_locs[release_count] = if g == 0 { 1 } else { (g * 4) as i32 };
                        release_delta[release_count] = delta;
                        release_count += 1;
                    }
                    if attack_count + release_count > 6 {
                        break;
                    }
                }
            }
        }

        let mut current_max = envelope[0..32].iter().copied().fold(0.0, f32::max);
        if !current_max.is_finite() {
            current_max = 0.0;
        }

        let mut last_release_cum = 0i32;
        for i in (0..release_count).rev() {
            last_release_cum += release_delta[i];
            last_release_cum = last_release_cum.min(-LNGAIN_EXPONENTS[0]);
            release_delta[i] = last_release_cum;
        }

        let mut attack_cum = 0i32;
        let attack_cap = LNGAIN_EXPONENTS[15] + last_release_cum;
        for i in (0..attack_count).rev() {
            attack_cum += attack_delta[i];
            attack_cum = attack_cum.min(attack_cap);
            attack_delta[i] = attack_cum;
        }

        let mut curve = [0i32; 33];
        let mut i = 0i32;
        for n in 0..attack_count {
            while i <= attack_locs[n] && (i as usize) < curve.len() {
                curve[i as usize] += attack_delta[n];
                i += 1;
            }
        }

        i = 32;
        for n in 0..release_count {
            while i >= release_locs[n] && i >= 0 {
                curve[i as usize] += release_delta[n];
                i -= 1;
            }
        }

        let base = curve[32];
        let mut next = GainInfo::new();
        next.set_peak_history(current_max);
        let mut count = 0usize;
        for loc in 0..32 {
            if curve[loc] != curve[loc + 1] {
                if count >= MAX_GAIN_POINTS {
                    next.count = (count + 1) as i32;
                    *output = next;
                    return false;
                }
                next.location[count] = loc as i32;
                let exp = curve[loc] - base;
                let Some(level) = exponent_to_level(exp) else {
                    return false;
                };
                next.level[count] = level as i32;
                count += 1;
            }
        }
        next.count = count as i32;
        while next.count > 0 && next.level[next.count as usize - 1] == EXPONENT_OFFSET {
            next.count -= 1;
        }
        *output = next;
        output.count <= MAX_GAIN_POINTS as i32
    }

    /// Reimplementation of libatrac `adjust_gainc` (`0x6b050`).
    pub fn adjust_gainc(
        bands: &[&[f32]],
        size: usize,
        current: &[GainInfo],
        next: &mut [GainInfo],
    ) -> bool {
        let band_count = next.len().min(current.len()).min(bands.len());
        if band_count <= 1 {
            return true;
        }

        let mut spread = [0i32; BAND_COUNT];
        for band in 0..band_count.min(BAND_COUNT) {
            let info = &next[band];
            if info.count < 1 {
                spread[band] = 0;
                continue;
            }

            let count = info.len();
            let mut min_level = EXPONENT_OFFSET;
            let mut min_idx = 0usize;
            for i in 0..count {
                if info.level[i] < min_level {
                    min_level = info.level[i];
                    min_idx = i;
                }
            }

            let mut max_before_min = 0i32;
            for i in 0..=min_idx {
                max_before_min = max_before_min.max(info.level[i]);
            }
            spread[band] = max_before_min - min_level;
        }

        if spread[1] > 1 && current[0].count == 0 && next[0].count == 0 {
            let location = next[1].location[0];
            let block = size / 64;
            let sample_limit = (location + 1).max(0) as usize * block;
            let mut allow = current[0].peak_history() <= 16384.0;
            let start = size / 2;
            for i in 0..sample_limit {
                let idx = start + i;
                if idx >= bands[0].len() {
                    break;
                }
                if bands[0][idx].abs() > 16384.0 {
                    allow = false;
                    break;
                }
            }

            if allow {
                let Some(level) = exponent_to_level(1) else {
                    return false;
                };
                next[0].count = 1;
                next[0].level[0] = level as i32;
                next[0].location[0] = location;
            }
        }

        true
    }

    /// Computes 512 per-sample scale values from the current and next gain
    /// schedules, matching `gainc_window` (`0x692f8`).
    ///
    /// `scales` receives 512 `f32` values. The first 256 correspond to the
    /// overlap half, the second 256 to the current half.
    ///
    /// Returns `false` if any level index is invalid (matching the −1 return
    /// of `gainc_window`).
    pub fn compute_scales(
        current: &GainInfo,
        next: &GainInfo,
        scales: &mut [f32; MDCT_SIZE],
    ) -> bool {
        let mut block_exp = [0i32; 2 * BLOCKS_PER_HALF];

        if !Self::fill_block_exponents(&mut block_exp, next, true) {
            return false;
        }
        if !Self::fill_block_exponents(&mut block_exp, current, false) {
            return false;
        }

        Self::block_exponents_to_scales(&block_exp, scales);
        true
    }

    /// Fills the 64-entry block exponent array from a `GainInfo` schedule.
    ///
    /// When `is_next` is true (first pass), locations are offset by +32
    /// (matching `gainc_window`'s `add edx, 0x20`). When false (second pass),
    /// exponents are ADDED to existing values (matching `add [buf], eax`).
    ///
    /// Returns `false` if any level index is invalid.
    fn fill_block_exponents(
        block_exp: &mut [i32; 2 * BLOCKS_PER_HALF],
        info: &GainInfo,
        is_next: bool,
    ) -> bool {
        let count = info.len();
        let mut pos = 0usize;

        for i in 0..count {
            let level = info.level[i] as u32;
            if level >= 16 {
                return false;
            }
            let exp = LNGAIN_EXPONENTS[level as usize];

            let location = info.location[i] as usize;
            let end = if is_next {
                location + BLOCKS_PER_HALF
            } else {
                location
            };

            if pos > end {
                continue;
            }

            while pos <= end && pos < block_exp.len() {
                if is_next {
                    block_exp[pos] = exp;
                } else {
                    block_exp[pos] += exp;
                }
                pos += 1;
            }
        }
        true
    }
    /// Converts 64 block exponents to 512 per-sample scale values, matching
    /// the third loop of `gainc_window`.
    ///
    /// The loop iterates block 63→0. For each 8-sample block:
    /// - `cur_exp = block_exp[block_idx]`, `neg_exp = -cur_exp`
    /// - If `neg_exp == prev_exp` (flat region): all 8 samples get
    ///   `scale = 2^neg_exp` (the decode-side gain).
    /// - Otherwise (interpolation): sample `s` (0..7) gets
    ///   `scale = pow(2, ((LOC_SZ-s)*neg_exp + s*prev_exp) / LOC_SZ)`.
    /// - After each block: `prev_exp = neg_exp`.
    fn block_exponents_to_scales(
        block_exp: &[i32; 2 * BLOCKS_PER_HALF],
        scales: &mut [f32; MDCT_SIZE],
    ) {
        let mut prev_exp = 0i32;
        let loc_sz = LOC_SZ as f64;

        for block_idx in (0..2 * BLOCKS_PER_HALF).rev() {
            let cur_exp = block_exp[block_idx];
            let neg_exp = -cur_exp;

            if neg_exp == prev_exp {
                let scale = (2.0_f64).powi(neg_exp) as f32;
                for s in 0..LOC_SZ {
                    let sample_idx = block_idx * LOC_SZ + s;
                    if sample_idx < MDCT_SIZE {
                        scales[sample_idx] = scale;
                    }
                }
            } else {
                let neg_f = neg_exp as f64;
                let prev_f = prev_exp as f64;

                for s in 0..LOC_SZ {
                    let exponent = ((LOC_SZ - s) as f64 * neg_f + s as f64 * prev_f) / loc_sz;
                    let scale = (2.0_f64).powf(exponent);
                    let sample_idx = block_idx * LOC_SZ + s;
                    if sample_idx < MDCT_SIZE {
                        scales[sample_idx] = scale as f32;
                    }
                }
            }

            prev_exp = neg_exp;
        }
    }

    /// Modulates a 512-sample MDCT block by dividing the original samples
    /// by the computed gain scales.
    ///
    /// `overlap` (256 samples) and `current` (256 samples) are divided by
    /// `scales[0..256]` and `scales[256..512]` respectively, producing the
    /// gain-compensated MDCT input in `block`.
    pub fn modulate(
        overlap: &[f32; BAND_SAMPLES],
        current: &[f32; BAND_SAMPLES],
        scales: &[f32; MDCT_SIZE],
        block: &mut [f32; MDCT_SIZE],
    ) {
        for i in 0..BAND_SAMPLES {
            let s0 = scales[i];
            let s1 = scales[BAND_SAMPLES + i];
            block[i] = if s0 != 0.0 {
                overlap[i] / s0
            } else {
                overlap[i]
            };
            block[BAND_SAMPLES + i] = if s1 != 0.0 {
                current[i] / s1
            } else {
                current[i]
            };
        }
    }

    /// Demodulates gain-modulated samples (decode-side), matching the codex
    /// `GainProcessor::demodulate` algorithm.
    ///
    /// Uses the decode-side `GAIN_LEVEL_DECODE` and `GAIN_INTERPOLATION_DECODE`
    /// tables. `out[pos] = (cur[pos] * scale + prev[pos]) * level`.
    pub fn demodulate(
        gi_now: &[GainPoint],
        gi_next: &[GainPoint],
        out: &mut [f32],
        cur: &[f32],
        prev: &[f32],
    ) {
        let half = MDCT_SIZE / 2;
        assert!(out.len() >= half && cur.len() >= half && prev.len() >= half);

        let mut pos = 0usize;
        let scale = gi_next
            .first()
            .map(|p| GAIN_LEVEL_DECODE[p.level as usize])
            .unwrap_or(1.0);

        for (i, point) in gi_now.iter().enumerate() {
            let last_pos = (point.location << LOC_SCALE) as usize;
            let mut level = GAIN_LEVEL_DECODE[point.level as usize];
            let next_level = gi_now
                .get(i + 1)
                .map(|p| p.level as i32)
                .unwrap_or(EXPONENT_OFFSET);
            let inc_pos = next_level - point.level as i32 + 15;
            let gain_inc = GAIN_INTERPOLATION_DECODE[(inc_pos as usize).min(7)];

            while pos < last_pos {
                out[pos] = (cur[pos] * scale + prev[pos]) * level;
                pos += 1;
            }
            while pos < last_pos + LOC_SZ {
                out[pos] = (cur[pos] * scale + prev[pos]) * level;
                level *= gain_inc;
                pos += 1;
            }
        }

        while pos < half {
            out[pos] = cur[pos] * scale + prev[pos];
            pos += 1;
        }
    }
}

impl Default for GainProcessor {
    fn default() -> Self {
        Self
    }
}

fn log2_delta(ratio: f32) -> i32 {
    let safe = if ratio <= 1.0e-20 { 1.0e-20 } else { ratio };
    ((safe as f64).log10() / 2.0f64.log10() + 0.5).trunc() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_info_empty_by_default() {
        let info = GainInfo::new();
        assert!(info.is_empty());
        assert_eq!(info.len(), 0);
        assert!(info.points().is_empty());
    }

    #[test]
    fn gain_info_from_points_roundtrip() {
        let points = vec![
            GainPoint {
                level: 3,
                location: 5,
            },
            GainPoint {
                level: 7,
                location: 10,
            },
        ];
        let info = GainInfo::from_points(&points);
        assert_eq!(info.count, 2);
        assert_eq!(info.len(), 2);
        assert_eq!(info.points(), points);
    }

    #[test]
    fn gain_info_caps_at_max_points() {
        let points: Vec<_> = (0..10)
            .map(|i| GainPoint {
                level: i % 16,
                location: i,
            })
            .collect();
        let info = GainInfo::from_points(&points);
        assert_eq!(info.count, MAX_GAIN_POINTS as i32);
        assert_eq!(info.len(), MAX_GAIN_POINTS);
    }

    #[test]
    fn level_to_gain_encode_matches_table() {
        for level in 0..16 {
            let expected = (2.0_f32).powi(level as i32 - 4);
            let got = level_to_gain_encode(level);
            assert!(
                (got - expected).abs() < 1e-5,
                "level {level}: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn level_to_gain_decode_matches_table() {
        for level in 0..16 {
            let expected = (2.0_f32).powi(4 - level as i32);
            let got = level_to_gain_decode(level);
            assert!(
                (got - expected).abs() < 1e-5,
                "level {level}: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn exponent_to_level_is_inverse() {
        for (level, &exp) in LNGAIN_EXPONENTS.iter().enumerate() {
            assert_eq!(exponent_to_level(exp), Some(level as u32));
        }
        assert_eq!(exponent_to_level(100), None);
    }

    #[test]
    fn empty_gain_info_produces_unit_scales() {
        let current = GainInfo::new();
        let next = GainInfo::new();
        let mut scales = [0.0f32; MDCT_SIZE];
        assert!(GainProcessor::compute_scales(&current, &next, &mut scales));
        for (i, &s) in scales.iter().enumerate() {
            assert!(
                (s - 1.0).abs() < 1e-5,
                "scales[{i}] = {s}, expected 1.0 for empty gain"
            );
        }
    }

    #[test]
    fn flat_region_scale_for_zero_exponent() {
        let mut next = GainInfo::new();
        next.count = 1;
        next.level[0] = 4;
        next.location[0] = 0;

        let current = GainInfo::new();
        let mut scales = [0.0f32; MDCT_SIZE];
        assert!(GainProcessor::compute_scales(&current, &next, &mut scales));

        for (i, &s) in scales.iter().enumerate() {
            assert!(
                s.is_finite() && s > 0.0,
                "scale[{i}] = {s} should be finite and positive"
            );
        }
    }

    #[test]
    fn compute_scales_produces_finite_positive_values() {
        let mut next = GainInfo::new();
        next.count = 2;
        next.level[0] = 8;
        next.location[0] = 0;
        next.level[1] = 2;
        next.location[1] = 16;

        let current = GainInfo::new();
        let mut scales = [0.0f32; MDCT_SIZE];
        assert!(GainProcessor::compute_scales(&current, &next, &mut scales));

        for (i, &s) in scales.iter().enumerate() {
            assert!(s.is_finite(), "scale[{i}] is not finite: {s}");
            assert!(s > 0.0, "scale[{i}] is not positive: {s}");
        }
    }

    #[test]
    fn modulate_divides_by_scales() {
        let mut next = GainInfo::new();
        next.count = 1;
        next.level[0] = 8;
        next.location[0] = 0;

        let current = GainInfo::new();
        let mut scales = [0.0f32; MDCT_SIZE];
        GainProcessor::compute_scales(&current, &next, &mut scales);

        let overlap = [100.0f32; BAND_SAMPLES];
        let current_in = [200.0f32; BAND_SAMPLES];
        let mut block = [0.0f32; MDCT_SIZE];
        GainProcessor::modulate(&overlap, &current_in, &scales, &mut block);

        for i in 0..BAND_SAMPLES {
            let expected = overlap[i] / scales[i];
            assert!(
                (block[i] - expected).abs() < 1e-3,
                "overlap modulate[{i}] = {}, expected {}",
                block[i],
                expected
            );
        }
    }

    #[test]
    fn subband_info_resets_to_empty() {
        let mut si = SubbandInfo::new();
        si.current[0].count = 3;
        si.next[2].count = 1;
        si.reset();
        for b in 0..BAND_COUNT {
            assert!(si.current[b].is_empty());
            assert!(si.next[b].is_empty());
        }
    }

    #[test]
    fn demodulate_empty_gain_is_overlap_addition() {
        let cur = vec![2.0f32; BAND_SAMPLES];
        let prev = vec![3.0f32; BAND_SAMPLES];
        let mut out = vec![0.0f32; BAND_SAMPLES];
        GainProcessor::demodulate(&[], &[], &mut out, &cur, &prev);
        for (i, &v) in out.iter().enumerate() {
            assert!((v - 5.0).abs() < 1e-5, "out[{i}] = {v}");
        }
    }

    #[test]
    fn compute_scales_returns_false_for_invalid_level() {
        let mut next = GainInfo::new();
        next.count = 1;
        next.level[0] = 20;
        next.location[0] = 0;
        let current = GainInfo::new();
        let mut scales = [0.0f32; MDCT_SIZE];
        assert!(!GainProcessor::compute_scales(&current, &next, &mut scales));
    }
}
