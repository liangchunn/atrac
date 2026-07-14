//! QMF analysis for the classic ATRAC3 encoder.
//!
//! Reproduces the behaviour of `libatrac.so.1.2.0`:
//! - `qmf` leaf at `0x6a0ac`: one 48-tap windowed split, stride 2.
//! - `bandsplit_at3` at `0x6b2a0`: three cascaded `qmf` splits producing four
//!   256-sample subbands from one 1024-sample channel block.
//!
//! The `qmf` leaf keeps a 46-sample history (the last 46 input samples of the
//! previous call) per stage. `bandsplit_at3` keeps three independent histories
//! (at state offsets `+0x5000`, `+0x50b8`, `+0x5170`), one per cascade stage.
//!
//! For each output sample `k` in `0..N/2`:
//! ```text
//! lo = sum_{j odd}  buf[2k + j] * QMF_WINDOW[j]
//! hi = sum_{j even} buf[2k + j] * QMF_WINDOW[j]
//! lower[k] = (lo + hi) as f32
//! upper[k] = (lo - hi) as f32
//! ```
//! where `buf = [history (46 f32)][input (N f32)]`. Default builds use `f64`
//! accumulation as the empirically closest fast path. `bit-perfect` builds use
//! the software x87 scalar path and the binary's reverse grouped tap order.

use crate::tables::QMF_WINDOW;

#[cfg(feature = "bit-perfect")]
use crate::dsp::x87::{RoundingMode, X87Control, X87Real as Ext80};

const HISTORY: usize = 46;
const TAPS: usize = 48;

/// One 48-tap QMF analysis split with persistent 46-sample history.
#[derive(Clone)]
pub struct QmfSplit {
    history: Vec<f32>,
}

impl QmfSplit {
    pub fn new() -> Self {
        Self {
            history: vec![0.0; HISTORY],
        }
    }

    /// Splits `input` (N samples) into `lower` and `upper` (N/2 samples each).
    pub fn analysis(&mut self, input: &[f32], lower: &mut [f32], upper: &mut [f32]) {
        let n = input.len();
        let half = n / 2;
        debug_assert_eq!(lower.len(), half);
        debug_assert_eq!(upper.len(), half);
        debug_assert!(n >= 2 && n.is_multiple_of(2));

        let mut buf = vec![0.0_f32; HISTORY + n];
        buf[..HISTORY].copy_from_slice(&self.history);
        buf[HISTORY..].copy_from_slice(input);

        for k in 0..half {
            let base = 2 * k;
            let (lo, hi) = qmf_outputs(&buf, base);
            lower[k] = lo;
            upper[k] = hi;
        }

        self.history.copy_from_slice(&input[n - HISTORY..]);
    }
}

#[cfg(feature = "bit-perfect")]
fn qmf_outputs(buf: &[f32], base: usize) -> (f32, f32) {
    const CONTROL: X87Control = X87Control {
        rounding: RoundingMode::NearestEven,
    };

    let mut lo = Ext80::zero(false);
    let mut hi = Ext80::zero(false);
    let mut j = TAPS - 1;
    loop {
        lo = x87_mul_add(lo, buf[base + j], QMF_WINDOW[j], CONTROL);
        lo = x87_mul_add(lo, buf[base + j - 2], QMF_WINDOW[j - 2], CONTROL);
        hi = x87_mul_add(hi, buf[base + j - 1], QMF_WINDOW[j - 1], CONTROL);
        hi = x87_mul_add(hi, buf[base + j - 3], QMF_WINDOW[j - 3], CONTROL);
        if j == 3 {
            break;
        }
        j -= 4;
    }

    (
        lo.fadd(hi, CONTROL).to_f32(RoundingMode::NearestEven),
        lo.fsub(hi, CONTROL).to_f32(RoundingMode::NearestEven),
    )
}

#[cfg(feature = "bit-perfect")]
fn x87_mul_add(acc: Ext80, sample: f32, coeff: f32, control: X87Control) -> Ext80 {
    let product = Ext80::from_f32_exact(sample).fmul(Ext80::from_f32_exact(coeff), control);
    acc.fadd(product, control)
}

#[cfg(not(feature = "bit-perfect"))]
fn qmf_outputs(buf: &[f32], base: usize) -> (f32, f32) {
    let mut lo = 0.0_f64;
    let mut hi = 0.0_f64;
    let mut j = 0;
    while j < TAPS {
        hi += (buf[base + j] as f64) * (QMF_WINDOW[j] as f64);
        lo += (buf[base + j + 1] as f64) * (QMF_WINDOW[j + 1] as f64);
        j += 2;
    }
    ((lo + hi) as f32, (lo - hi) as f32)
}

impl Default for QmfSplit {
    fn default() -> Self {
        Self::new()
    }
}

/// Number of PCM samples per ATRAC3 sound unit (per channel).
pub const NUM_SAMPLES: usize = 1024;
/// Number of samples per QMF subband output.
pub const BAND_SAMPLES: usize = 256;
/// Number of QMF subbands.
pub const BAND_COUNT: usize = 4;

/// Cascaded 3-stage QMF analysis filter bank for one channel, matching
/// `bandsplit_at3`.
///
/// Stage 1 splits 1024 samples into two 512-sample halves. Stage 2 splits the
/// lower half into sub0 (lowest band) and sub1. Stage 3 splits the upper half
/// into sub3 and sub2 (note the index swap, matching `bandsplit_at3`'s third
/// `qmf` call which writes `lower -> sub3`, `upper -> sub2`).
pub struct Atrac3AnalysisFilterBank {
    stage1: QmfSplit,
    stage2: QmfSplit,
    stage3: QmfSplit,
    buf_low: Vec<f32>,
    buf_up: Vec<f32>,
}

impl Atrac3AnalysisFilterBank {
    pub fn new() -> Self {
        Self {
            stage1: QmfSplit::new(),
            stage2: QmfSplit::new(),
            stage3: QmfSplit::new(),
            buf_low: vec![0.0; NUM_SAMPLES / 2],
            buf_up: vec![0.0; NUM_SAMPLES / 2],
        }
    }

    /// Analyses one 1024-sample channel block into four 256-sample subbands.
    ///
    /// `bands[0]` is the lowest-frequency band, `bands[3]` the highest.
    pub fn analysis(&mut self, pcm: &[f32], bands: &mut [&mut [f32]; BAND_COUNT]) {
        assert_eq!(pcm.len(), NUM_SAMPLES);
        for b in bands.iter() {
            assert_eq!(b.len(), BAND_SAMPLES);
        }

        self.stage1
            .analysis(pcm, &mut self.buf_low, &mut self.buf_up);

        let [sub0, sub1, sub2, sub3] = bands;
        self.stage2.analysis(&self.buf_low, sub0, sub1);
        self.stage3.analysis(&self.buf_up, sub3, sub2);
    }
}

impl Default for Atrac3AnalysisFilterBank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_of_silence_is_silence() {
        let mut q = QmfSplit::new();
        let input = vec![0.0; 512];
        let mut lo = vec![0.0; 256];
        let mut hi = vec![0.0; 256];
        q.analysis(&input, &mut lo, &mut hi);
        assert!(lo.iter().all(|x| *x == 0.0));
        assert!(hi.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn filter_bank_produces_four_finite_bands() {
        let mut fb = Atrac3AnalysisFilterBank::new();
        let pcm: Vec<f32> = (0..NUM_SAMPLES)
            .map(|i| (2.0 * std::f32::consts::PI * 997.0 * i as f32 / 44_100.0).sin() * 4096.0)
            .collect();
        let mut b0 = vec![0.0; BAND_SAMPLES];
        let mut b1 = vec![0.0; BAND_SAMPLES];
        let mut b2 = vec![0.0; BAND_SAMPLES];
        let mut b3 = vec![0.0; BAND_SAMPLES];
        let mut bands = [
            b0.as_mut_slice(),
            b1.as_mut_slice(),
            b2.as_mut_slice(),
            b3.as_mut_slice(),
        ];
        fb.analysis(&pcm, &mut bands);
        for b in &bands {
            assert!(b.iter().all(|x| x.is_finite()));
        }
    }
}
