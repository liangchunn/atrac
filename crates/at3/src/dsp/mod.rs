//! DSP building blocks for the classic ATRAC3 encoder.
//!
//! Module stubs will be filled in by later TODOs (QMF analysis, MDCT,
//! gain control, tone extraction, quantization, bitstream packing).

pub mod dba;
pub mod dba_pack;
pub mod encode;
pub mod gain;
pub mod mdct;
pub mod pack;
pub mod qmf;
pub mod quant;
pub mod tone;
pub mod transient;
