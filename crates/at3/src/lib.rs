//! Pure library for the classic ATRAC3 encoder: DSP (QMF, MDCT, gain
//! control), psychoacoustic decisions, quantization, and bitstream packing.
//!
//! This crate is intentionally free of I/O and external tooling concerns.

pub mod dsp;
pub mod tables;
