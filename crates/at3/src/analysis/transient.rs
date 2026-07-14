//! Transient detection and gain curve construction for ATRAC3.
//!
//! Ports the transient-analysis module, adapted to the libatrac
//! constants. This is separate from the libatrac-compatible gain-control
//! schedule-generation path in `analysis::gain`.
//!
//! Key components:
//! - `TransientDetector`: HP-filter + RMS-based transient flagging.
//! - `analyze_gain`: splits a 256-sample half-frame into `max_points`
//!   subframes and computes RMS or peak per subframe.
//! - `calc_curve`: builds `GainCurvePoint`s from the analyzed gain envelope
//!   using plateau detection, median filtering, and boundary scoring.

use crate::analysis::gain::{EXPONENT_OFFSET, GainPoint, MAX_GAIN_POINTS};

const PREV_BUF_SZ: usize = 20;
const FIR_LEN: usize = 21;

fn calculate_rms(input: &[f32]) -> f32 {
    (input.iter().map(|x| x * x).sum::<f32>() / input.len() as f32).sqrt()
}

fn calculate_peak(input: &[f32]) -> f32 {
    input.iter().map(|x| x.abs()).fold(0.0, f32::max)
}

fn get_first_set_bit(x: u32) -> u16 {
    if x == 0 {
        return 0;
    }
    (31 - x.leading_zeros()) as u16
}

/// RMS-based transient detector with a 21-tap high-pass filter.
#[derive(Debug, Clone)]
pub struct TransientDetector {
    short_sz: usize,
    block_sz: usize,
    n_short_blocks: usize,
    hpf_buffer: Vec<f32>,
    last_energy: f32,
    last_transient_pos: u16,
}

impl TransientDetector {
    pub fn new(short_sz: u16, block_sz: u16) -> Self {
        let short_sz = short_sz as usize;
        let block_sz = block_sz as usize;
        Self {
            short_sz,
            block_sz,
            n_short_blocks: block_sz / short_sz,
            hpf_buffer: vec![0.0; block_sz + FIR_LEN],
            last_energy: 0.0,
            last_transient_pos: 0,
        }
    }

    fn hp_filter(&mut self, input: &[f32], out: &mut [f32]) {
        debug_assert_eq!(self.block_sz, input.len());
        debug_assert_eq!(self.block_sz, out.len());
        const FIR_COEF: [f32; 10] = [
            -8.65163e-18 * 2.0,
            -0.00851586 * 2.0,
            -6.74764e-18 * 2.0,
            0.0209036 * 2.0,
            -3.36639e-17 * 2.0,
            -0.0438162 * 2.0,
            -1.54175e-17 * 2.0,
            0.0931738 * 2.0,
            -5.52212e-17 * 2.0,
            -0.313819 * 2.0,
        ];

        self.hpf_buffer[PREV_BUF_SZ..PREV_BUF_SZ + self.block_sz].copy_from_slice(input);
        for (i, y) in out.iter_mut().enumerate() {
            let mut s = self.hpf_buffer[i + 10];
            let mut s2 = 0.0;
            for j in (0..(((FIR_LEN - 1) / 2) - 1)).step_by(2) {
                s += FIR_COEF[j] * (self.hpf_buffer[i + j] + self.hpf_buffer[i + FIR_LEN - j]);
                s2 += FIR_COEF[j + 1]
                    * (self.hpf_buffer[i + j + 1] + self.hpf_buffer[i + FIR_LEN - j - 1]);
            }
            *y = (s + s2) / 2.0;
        }
        self.hpf_buffer[..PREV_BUF_SZ].copy_from_slice(&input[self.block_sz - PREV_BUF_SZ..]);
    }

    /// Returns `true` if a transient was detected in this block.
    pub fn detect(&mut self, buf: &[f32]) -> bool {
        assert_eq!(self.block_sz, buf.len());
        let n_blocks_to_analyze = self.n_short_blocks + 1;
        let mut rms_per_short_block = vec![0.0; n_blocks_to_analyze];
        let mut filtered = vec![0.0; self.block_sz];
        self.hp_filter(buf, &mut filtered);

        let mut trans = false;
        rms_per_short_block[0] = self.last_energy;
        for i in 1..n_blocks_to_analyze {
            rms_per_short_block[i] =
                19.0 * calculate_rms(&filtered[(i - 1) * self.short_sz..i * self.short_sz]).log10();
            if rms_per_short_block[i] - rms_per_short_block[i - 1] > 16.0 {
                trans = true;
                self.last_transient_pos = i as u16;
            }
            if rms_per_short_block[i - 1] - rms_per_short_block[i] > 20.0 {
                trans = true;
                self.last_transient_pos = i as u16;
            }
        }
        self.last_energy = rms_per_short_block[self.n_short_blocks];
        trans
    }

    pub fn last_transient_pos(&self) -> u32 {
        self.last_transient_pos as u32
    }
}

/// Splits `input` into `max_points` equal subframes and computes RMS (or
/// peak) per subframe. Returns the per-subframe energy envelope.
pub fn analyze_gain(input: &[f32], max_points: u32, use_rms: bool) -> Vec<f32> {
    let mut res = Vec::with_capacity(max_points as usize);
    let step = input.len() / max_points as usize;
    if step == 0 {
        return res;
    }

    for pos in (0..input.len()).step_by(step) {
        let end = (pos + step).min(input.len());
        let chunk = &input[pos..end];
        let val = if use_rms {
            calculate_rms(chunk)
        } else {
            calculate_peak(chunk)
        };
        res.push(val);
    }
    res
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GainCurvePoint {
    pub level: u32,
    pub location: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CurveBuilderCtx {
    pub last_level: f32,
    pub last_hpf_energy: f32,
    pub last_target: f32,
}

#[derive(Debug, Clone, Copy)]
struct PlateauResult {
    level: f32,
    max_raw: f32,
    release_at_end: bool,
}

/// Converts a gain ratio to a level index (0..15).
///
/// Uses the decode-side `GAIN_LEVEL` orientation: level 0 = gain 16,
/// level 4 = gain 1, level 15 = gain 1/2048. This matches the codex
/// `relation_to_idx` and is compatible with the libatrac level indexing
/// (the level indices are the same; only the gain table orientation
/// differs between encode and decode sides).
fn relation_to_idx(mut x: f32) -> u16 {
    if x <= 0.5 {
        x = 1.0 / x.max(0.000_488_281_25);
        (EXPONENT_OFFSET as u16) + get_first_set_bit(x as u32)
    } else {
        x = x.min(16.0);
        (EXPONENT_OFFSET as u16) - get_first_set_bit(x as u32)
    }
}

fn median_filter<const RADIUS: usize>(input: &[f32], out: &mut [f32]) {
    assert_eq!(input.len(), out.len());
    let n = input.len() as isize;
    let mut w = vec![0.0; RADIUS * 2 + 1];

    for (i, out_val) in out.iter_mut().enumerate() {
        let lo = (i as isize - RADIUS as isize).max(0);
        let hi = (n - 1).min(i as isize + RADIUS as isize);
        let count = (hi - lo + 1) as usize;
        w[..count].copy_from_slice(&input[lo as usize..=hi as usize]);
        w[..count].sort_by(|a, b| a.total_cmp(b));
        *out_val = w[count / 2];
    }
}

fn find_plateau(input: &[f32], min_contiguous: usize) -> PlateauResult {
    let n = input.len();
    let max_raw = input.iter().copied().fold(0.0, f32::max);
    if n < min_contiguous {
        return PlateauResult {
            level: 0.0,
            max_raw,
            release_at_end: false,
        };
    }

    let mut filtered = vec![0.0; n];
    median_filter::<1>(input, &mut filtered);

    let mut best_level = 0.0;
    let mut best_end = None;
    for j in 0..=n - min_contiguous {
        let min_val = filtered[j..j + min_contiguous]
            .iter()
            .copied()
            .fold(filtered[j], f32::min);
        if min_val > best_level {
            best_level = min_val;
            best_end = Some(j + min_contiguous - 1);
        }
    }

    let Some(mut best_end) = best_end else {
        return PlateauResult {
            level: 0.0,
            max_raw,
            release_at_end: false,
        };
    };

    if best_level < 1.0e-6 {
        return PlateauResult {
            level: 0.0,
            max_raw,
            release_at_end: false,
        };
    }

    while best_end + 1 < n && filtered[best_end + 1] >= best_level {
        best_end += 1;
    }

    let mut release_at_end = false;
    if best_end < n - 1 {
        if input[n - 1] < best_level * 0.1 {
            release_at_end = true;
        } else {
            let any_high_after = input[best_end + 1..].iter().any(|v| *v >= best_level * 0.7);
            release_at_end = !any_high_after && input[n - 1] < best_level * 0.5;
        }
    }

    PlateauResult {
        level: best_level,
        max_raw,
        release_at_end,
    }
}

fn boundary_transient_score(env: &[f32], loc: usize, win: usize) -> f32 {
    assert!(loc > 0 && loc < env.len());
    let left_start = loc.saturating_sub(win);
    let left_max = env[left_start..loc].iter().copied().fold(0.0, f32::max);
    let right_end = env.len().min(loc + win);
    let right_max = env[loc..right_end].iter().copied().fold(0.0, f32::max);

    let attack = (right_max + 1.0e-9) / (left_max + 1.0e-9);
    let release = (left_max + 1.0e-9) / (right_max + 1.0e-9);
    attack.max(release)
}

/// Builds gain curve points from the analyzed gain envelope.
///
/// The first call with a given `CurveBuilderCtx` returns an empty curve
/// (the detector needs one frame of history). Subsequent calls produce
/// up to `MAX_GAIN_POINTS - 1` transition points.
pub fn calc_curve(input: &[f32], ctx: &mut CurveBuilderCtx, min_score: f32) -> Vec<GainCurvePoint> {
    let mut curve = Vec::new();
    if input.is_empty() {
        return curve;
    }

    let plateau = find_plateau(input, 3);
    let use_plateau =
        plateau.level > 1.0e-6 && !plateau.release_at_end && plateau.level >= plateau.max_raw * 0.4;
    let target = if use_plateau {
        plateau.level
    } else {
        *input.last().unwrap()
    };

    let saved_last_level = ctx.last_level;
    ctx.last_level = *input.last().unwrap();
    ctx.last_target = target;

    if target < 1.0e-6 || saved_last_level < 1.0e-6 {
        return curve;
    }

    let n = input.len();
    let mut filtered = vec![0.0; n];
    median_filter::<1>(input, &mut filtered);

    let mut sf_level = vec![0_u16; n];
    for (level, &f) in sf_level.iter_mut().zip(&filtered) {
        *level = relation_to_idx(f / target);
    }

    let mut target_sf = 0;
    for sf in (0..n.saturating_sub(1)).rev() {
        if sf_level[sf] != EXPONENT_OFFSET as u16 {
            target_sf = sf + 1;
            break;
        }
    }

    if target_sf == 0 {
        return curve;
    }

    let mut boundary_score = vec![1.0; n + 1];
    for (loc, score) in boundary_score
        .iter_mut()
        .enumerate()
        .take(target_sf + 1)
        .skip(1)
    {
        *score = boundary_transient_score(&filtered, loc, 3);
    }

    #[derive(Clone)]
    struct Transition {
        loc: usize,
        level: u16,
        delta: i32,
    }

    let mut trans = Vec::new();
    let mut prev = EXPONENT_OFFSET as u16;
    for sf in (0..target_sf).rev() {
        let lev = sf_level[sf];
        if lev != prev {
            let loc = sf + 1;
            let delta = (lev as i32 - prev as i32).abs();
            let score = boundary_score[loc];
            let keep = loc == target_sf || delta >= 2 || score >= min_score;
            if keep {
                trans.push(Transition {
                    loc,
                    level: lev,
                    delta,
                });
                prev = lev;
            }
        }
    }
    trans.reverse();

    if trans.len() > MAX_GAIN_POINTS - 1 {
        trans.sort_by(|a, b| b.delta.cmp(&a.delta).then_with(|| b.loc.cmp(&a.loc)));
        trans.truncate(MAX_GAIN_POINTS - 1);
        trans.sort_by_key(|t| t.loc);
    }

    curve.extend(trans.into_iter().map(|t| GainCurvePoint {
        level: t.level as u32,
        location: t.loc as u32,
    }));
    curve
}

/// Detects transients and builds gain points for one band.
///
/// `band_pcm` is the 256-sample QMF band output for the current frame.
/// `ctx` carries per-band state across frames. Returns gain points suitable
/// for `GainInfo::from_points`.
pub fn detect_transient(band_pcm: &[f32], ctx: &mut CurveBuilderCtx) -> Vec<GainPoint> {
    let gain = analyze_gain(band_pcm, 32, true);
    let curve = calc_curve(&gain, ctx, 2.0);
    curve
        .into_iter()
        .map(|p| GainPoint {
            level: p.level,
            location: p.location,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_gain_rms_correct() {
        let input = vec![1.0f32; 256];
        let res = analyze_gain(&input, 32, true);
        assert_eq!(32, res.len());
        for v in &res {
            assert!((v - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn analyze_gain_peak_correct() {
        let mut input = vec![0.0f32; 256];
        input[64] = 42.0;
        let res = analyze_gain(&input, 32, false);
        assert_eq!(32, res.len());
        assert_eq!(42.0, res[8]);
        assert_eq!(0.0, res[0]);
    }

    #[test]
    fn transient_detector_flags_large_energy_change() {
        let mut detector = TransientDetector::new(32, 256);
        let mut quiet = vec![0.001; 256];
        let _ = detector.detect(&quiet);
        assert!(!detector.detect(&quiet));
        quiet[128..].fill(10.0);
        assert!(detector.detect(&quiet));
        assert!(detector.last_transient_pos() > 0);
    }

    #[test]
    fn calc_curve_skips_first_frame() {
        let gain = [1.0, 1.0, 1.0, 8.0, 8.0, 8.0, 8.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut ctx = CurveBuilderCtx::default();
        assert!(calc_curve(&gain, &mut ctx, 2.0).is_empty());
    }

    #[test]
    fn calc_curve_emits_points_on_second_frame() {
        let gain = [1.0, 1.0, 1.0, 8.0, 8.0, 8.0, 8.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut ctx = CurveBuilderCtx::default();
        let _ = calc_curve(&gain, &mut ctx, 2.0);
        let curve = calc_curve(&gain, &mut ctx, 2.0);
        assert!(curve.len() <= MAX_GAIN_POINTS);
        assert!(curve.iter().all(|p| p.level < 16 && p.location < 32));
        assert!(curve.windows(2).all(|w| w[0].location <= w[1].location));
    }

    #[test]
    fn relation_to_idx_neutral_is_4() {
        assert_eq!(4, relation_to_idx(1.0));
    }

    #[test]
    fn relation_to_idx_amplify_is_below_4() {
        assert_eq!(3, relation_to_idx(2.0));
        assert_eq!(2, relation_to_idx(4.0));
        assert_eq!(1, relation_to_idx(8.0));
        assert_eq!(0, relation_to_idx(16.0));
    }

    #[test]
    fn relation_to_idx_attenuate_is_above_4() {
        assert_eq!(5, relation_to_idx(0.5));
        assert_eq!(6, relation_to_idx(0.25));
        assert_eq!(7, relation_to_idx(0.125));
    }

    #[test]
    fn detect_transient_returns_empty_for_silence() {
        let mut ctx = CurveBuilderCtx::default();
        let pcm = vec![0.0f32; 256];
        let points = detect_transient(&pcm, &mut ctx);
        assert!(points.is_empty());
    }

    #[test]
    fn detect_transient_returns_empty_for_constant_signal() {
        let mut ctx = CurveBuilderCtx::default();
        let pcm = vec![1000.0f32; 256];
        let _ = detect_transient(&pcm, &mut ctx);
        let points = detect_transient(&pcm, &mut ctx);
        assert!(
            points.is_empty(),
            "constant signal should not produce gain points"
        );
    }
}
