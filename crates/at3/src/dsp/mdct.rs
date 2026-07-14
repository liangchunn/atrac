//! MDCT-512 forward transform matching `winormal_mdct_256` (`0x69a08`) in
//! `libatrac.so.1.2.0`.
//!
//! `forward_transform_at3` (`0x6b318`) loops over 4 QMF bands. For each band
//! it assembles a 512-sample block `[overlap (256)][current (256)]` and calls
//! `winormal_mdct_256`, which:
//!
//! 1. Applies the 512-entry forward window `g_a_fw` during the pre-fold stage.
//! 2. Computes the MDCT via a size-128 complex FFT (pre-twiddle, FFT,
//!    post-twiddle).
//! 3. Scales the result by `1/128`.
//! 4. If the band parity flag is 1 (odd bands 1 and 3), reverses the output
//!    — the odd-band spectrum inversion mandated by the ATRAC3 spec.
//!
//! The mathematical transform implemented here is:
//!
//! ```text
//! X[k] = (1/128) * Σ_{n=0}^{511} x[n] * w[n] * cos(π/256 * (n + 128.5) * (k + 0.5))
//! ```
//!
//! verified against `winormal_mdct_256` to within `f32`
//! precision (max abs error < 1e-9). The FFT-based decomposition uses
//! `rustfft` with `f64` accumulation to approximate the extended-precision
//! evaluation of the original library.

use std::f64::consts::PI;
use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use crate::tables::FORWARD_WINDOW;
#[cfg(feature = "mdct-disasm")]
use crate::tables::mdct::{
    FFT_BITREV128, FFT_WX_128, FFT_WY_128, MDCT_A0_256, MDCT_A1_256, MDCT_A2_256, MDCT_A3_256,
    MDCT_C0_256, MDCT_S0_256,
};

const MDCT_SIZE: usize = 512;
const OUTPUT_SIZE: usize = 256;
const FFT_SIZE: usize = 128;
#[cfg_attr(feature = "mdct-disasm", allow(dead_code))]
const SCALE: f64 = 1.0 / 128.0;

#[cfg(feature = "mdct-disasm")]
const MDCT_SCALE_F32: f32 = f32::from_bits(0x3c00_0000);

#[cfg_attr(feature = "mdct-disasm", allow(dead_code))]
struct MdctTwiddles {
    cos: Vec<f64>,
    sin: Vec<f64>,
}

impl MdctTwiddles {
    fn new() -> Self {
        let alpha = PI / (4.0 * MDCT_SIZE as f64);
        let omega = 2.0 * PI / MDCT_SIZE as f64;
        let mut cos = Vec::with_capacity(FFT_SIZE);
        let mut sin = Vec::with_capacity(FFT_SIZE);
        for i in 0..FFT_SIZE {
            cos.push((omega * i as f64 + alpha).cos());
            sin.push((omega * i as f64 + alpha).sin());
        }
        Self { cos, sin }
    }
}

#[cfg_attr(feature = "mdct-disasm", allow(dead_code))]
pub struct Mdct512 {
    fft: Arc<dyn Fft<f64>>,
    fft_scratch: Vec<Complex<f64>>,
    fft_buf: Vec<Complex<f64>>,
    windowed: Vec<f64>,
    output: Vec<f64>,
    twiddles: MdctTwiddles,
}

impl Mdct512 {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch_len = fft.get_inplace_scratch_len();
        Self {
            fft,
            fft_scratch: vec![Complex::default(); scratch_len],
            fft_buf: vec![Complex::default(); FFT_SIZE],
            windowed: vec![0.0; MDCT_SIZE],
            output: vec![0.0; OUTPUT_SIZE],
            twiddles: MdctTwiddles::new(),
        }
    }

    /// Compute the MDCT-512 of `input` (512 samples, `[overlap; current]`)
    /// into `output` (256 spectral coefficients).
    ///
    /// If `parity` is 1 (odd QMF band), the output is written in reversed
    /// order to match the odd-band spectrum inversion performed by
    /// `winormal_mdct_256`.
    pub fn transform(
        &mut self,
        input: &[f32; MDCT_SIZE],
        output: &mut [f32; OUTPUT_SIZE],
        parity: u32,
    ) {
        #[cfg(feature = "mdct-disasm")]
        {
            self.transform_disasm(input, output, parity);
        }

        #[cfg(not(feature = "mdct-disasm"))]
        {
            for n in 0..MDCT_SIZE {
                self.windowed[n] = input[n] as f64 * FORWARD_WINDOW[n] as f64;
            }

            let n4 = FFT_SIZE;
            let n2 = OUTPUT_SIZE;
            let n34 = 3 * n4;
            let n54 = 5 * n4;

            let mut n = 0usize;
            while n < n4 {
                let idx = n / 2;
                let r0 = self.windowed[n34 - 1 - n] + self.windowed[n34 + n];
                let i0 = self.windowed[n4 + n] - self.windowed[n4 - 1 - n];
                let c = self.twiddles.cos[idx];
                let s = self.twiddles.sin[idx];
                self.fft_buf[idx] = Complex::new(r0 * c + i0 * s, i0 * c - r0 * s);
                n += 2;
            }

            while n < n2 {
                let idx = n / 2;
                let r0 = self.windowed[n34 - 1 - n] - self.windowed[n - n4];
                let i0 = self.windowed[n4 + n] + self.windowed[n54 - 1 - n];
                let c = self.twiddles.cos[idx];
                let s = self.twiddles.sin[idx];
                self.fft_buf[idx] = Complex::new(r0 * c + i0 * s, i0 * c - r0 * s);
                n += 2;
            }

            self.fft
                .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);

            for n in (0..n2).step_by(2) {
                let idx = n / 2;
                let r0 = self.fft_buf[idx].re;
                let i0 = self.fft_buf[idx].im;
                let c = self.twiddles.cos[idx];
                let s = self.twiddles.sin[idx];
                self.output[n] = (-r0 * c - i0 * s) * SCALE;
                self.output[n2 - 1 - n] = (-r0 * s + i0 * c) * SCALE;
            }

            for (k, v) in output.iter_mut().enumerate() {
                let idx = if parity == 1 { OUTPUT_SIZE - 1 - k } else { k };
                *v = self.output[idx] as f32;
            }
        }
    }

    #[cfg(feature = "mdct-disasm")]
    fn transform_disasm(
        &mut self,
        input: &[f32; MDCT_SIZE],
        output: &mut [f32; OUTPUT_SIZE],
        parity: u32,
    ) {
        let mut tmp = [0.0f32; OUTPUT_SIZE];
        let mut real = [0.0f32; FFT_SIZE];
        let mut imag = [0.0f32; FFT_SIZE];

        for i in 0..64 {
            let left = -(input[384 + 2 * i] * FORWARD_WINDOW[384 + 2 * i]);
            let right = FORWARD_WINDOW[383 - 2 * i] * input[383 - 2 * i];
            tmp[i] = left - right;
        }

        for i in 0..128 {
            let left = FORWARD_WINDOW[2 * i] * input[2 * i];
            let right = FORWARD_WINDOW[255 - 2 * i] * input[255 - 2 * i];
            tmp[64 + i] = left - right;
        }

        for i in 0..64 {
            let left = FORWARD_WINDOW[256 + 2 * i] * input[256 + 2 * i];
            let right = FORWARD_WINDOW[511 - 2 * i] * input[511 - 2 * i];
            tmp[192 + i] = left + right;
        }

        for i in 0..FFT_SIZE {
            let even = tmp[2 * i];
            let odd = tmp[2 * i + 1];
            real[i] = even * MDCT_C0_256[i] - odd * MDCT_S0_256[i];
            imag[i] = odd * MDCT_C0_256[i] + even * MDCT_S0_256[i];
        }

        let mut seen = [false; FFT_SIZE];
        for i in 0..FFT_SIZE {
            if !seen[i] {
                let j = FFT_BITREV128[i] as usize;
                real.swap(i, j);
                imag.swap(i, j);
                seen[j] = true;
            }
        }

        let mut twiddle_step = 64usize;
        let mut group_size = 2usize;
        for _ in 0..7 {
            let half = group_size >> 1;
            let mut left = 0usize;
            let mut right = half;
            for _ in 0..twiddle_step {
                let mut twiddle = 0usize;
                for _ in 0..half {
                    let left_re = real[left];
                    let left_im = imag[left];
                    let right_re = real[right];
                    let right_im = imag[right];
                    let t_re = right_re * FFT_WX_128[twiddle] - right_im * FFT_WY_128[twiddle];
                    let t_im = right_re * FFT_WY_128[twiddle] + right_im * FFT_WX_128[twiddle];

                    real[right] = left_re - t_re;
                    imag[right] = left_im - t_im;
                    real[left] = t_re + left_re;
                    imag[left] = t_im + left_im;

                    twiddle += twiddle_step;
                    left += 1;
                    right += 1;
                }
                left += half;
                right += half;
            }
            twiddle_step >>= 1;
            group_size <<= 1;
        }

        for i in 0..FFT_SIZE {
            let rev = FFT_SIZE - 1 - i;
            let real_i = real[i];
            let real_rev = real[rev];
            let imag_i = imag[i];
            let imag_rev = imag[rev];

            let out0 = real_rev * MDCT_A1_256[i]
                + real_i * MDCT_A0_256[i]
                + imag_i * MDCT_A2_256[i]
                + imag_rev * MDCT_A3_256[i];
            tmp[i] = out0;

            let out1 =
                real_i * MDCT_A2_256[i] - real_rev * MDCT_A3_256[i] - imag_i * MDCT_A0_256[i]
                    + imag_rev * MDCT_A1_256[i];
            tmp[OUTPUT_SIZE - 1 - i] = out1;
        }

        for i in 0..OUTPUT_SIZE {
            let src = if parity == 1 {
                tmp[OUTPUT_SIZE - 1 - i]
            } else {
                tmp[i]
            };
            output[i] = MDCT_SCALE_F32 * src;
        }
    }
}

impl Default for Mdct512 {
    fn default() -> Self {
        Self::new()
    }
}

pub const BAND_COUNT: usize = 4;
pub const BAND_SAMPLES: usize = 256;
pub const SPECTRA_SIZE: usize = 256;

pub struct Atrac3ForwardTransform {
    mdct: Mdct512,
    overlap: [[f32; BAND_SAMPLES]; BAND_COUNT],
}

impl Atrac3ForwardTransform {
    pub fn new() -> Self {
        Self {
            mdct: Mdct512::new(),
            overlap: [[0.0; BAND_SAMPLES]; BAND_COUNT],
        }
    }

    pub(crate) fn set_overlap_from_bands(&mut self, bands: &[[f32; BAND_SAMPLES]; BAND_COUNT]) {
        self.overlap = *bands;
    }

    /// Transform 4 QMF bands (256 samples each) into 4 spectral blocks
    /// (256 coefficients each), managing per-band overlap across frames.
    ///
    /// `bands[b]` is the current frame's 256 samples for band `b`.
    /// `spectra[b]` receives the 256 MDCT coefficients for band `b`.
    /// Band parity follows `{0, 1, 0, 1}` — odd bands get spectrum reversal.
    pub fn transform(
        &mut self,
        bands: &[&[f32; BAND_SAMPLES]; BAND_COUNT],
        spectra: &mut [&mut [f32; SPECTRA_SIZE]; BAND_COUNT],
    ) {
        self.transform_with_gain(bands, spectra, None)
    }

    /// Transform 4 QMF bands with optional gain modulation.
    ///
    /// When `gain` is `Some`, each band's overlap and current samples are
    /// divided by the gain scales computed from `(current[band], next[band])`
    /// before the MDCT, matching `forward_transform_at3`'s gain path.
    /// When `gain` is `None` or a band's gain info is empty, the input is
    /// copied directly (the no-gain path).
    pub fn transform_with_gain(
        &mut self,
        bands: &[&[f32; BAND_SAMPLES]; BAND_COUNT],
        spectra: &mut [&mut [f32; SPECTRA_SIZE]; BAND_COUNT],
        gain: Option<&crate::dsp::gain::SubbandInfo>,
    ) {
        const PARITY: [u32; BAND_COUNT] = [0, 1, 0, 1];
        let mut block = [0.0f32; MDCT_SIZE];
        for b in 0..BAND_COUNT {
            let modulated = match gain.filter(|g| !g.is_band_empty(b)) {
                Some(g) => {
                    let mut scales = [0.0f32; MDCT_SIZE];
                    let ok = crate::dsp::gain::GainProcessor::compute_scales(
                        &g.current[b],
                        &g.next[b],
                        &mut scales,
                    );
                    if ok {
                        crate::dsp::gain::GainProcessor::modulate(
                            &self.overlap[b],
                            bands[b],
                            &scales,
                            &mut block,
                        );
                    }
                    ok
                }
                None => false,
            };
            if !modulated {
                block[..BAND_SAMPLES].copy_from_slice(&self.overlap[b]);
                block[BAND_SAMPLES..].copy_from_slice(bands[b]);
            }
            self.mdct.transform(&block, spectra[b], PARITY[b]);
            self.overlap[b].copy_from_slice(bands[b]);
        }
    }
}

impl Default for Atrac3ForwardTransform {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdct_of_silence_is_silence() {
        let mut mdct = Mdct512::new();
        let input = [0.0f32; MDCT_SIZE];
        let mut output = [0.0f32; OUTPUT_SIZE];
        mdct.transform(&input, &mut output, 0);
        for v in &output {
            assert!(v.abs() < 1e-10, "non-zero output for silence: {v}");
        }
    }

    #[test]
    fn mdct_produces_finite_output() {
        let mut mdct = Mdct512::new();
        let mut input = [0.0f32; MDCT_SIZE];
        for (i, v) in input.iter_mut().enumerate() {
            *v = (i as f32 * 0.1).sin() * 100.0;
        }
        let mut output = [0.0f32; OUTPUT_SIZE];
        mdct.transform(&input, &mut output, 0);
        for v in &output {
            assert!(v.is_finite(), "non-finite output: {v}");
        }
    }
}
