//! Re-derived ATRAC3 constant tables (QMF, MDCT, gain, Huffman, quantization).
//!
//! Constants here are reproduced from `libatrac.so.1.2.0` by
//! `atrac3-re-tools dump-tables` and cross-referenced against the reference
//! implementation in `reference/atracdenc-codex/`. They are the authoritative
//! source for the Rust encoder.

pub mod dba;
pub mod gain;
pub mod mdct;
pub mod qmf;
pub mod quant;

pub use gain::{
    GAIN_INTERPOLATION_DECODE, GAIN_INTERPOLATION_ENCODE, GAIN_LEVEL_DECODE, GAIN_LEVEL_ENCODE,
    LNGAIN_EXPONENTS,
};
pub use mdct::FORWARD_WINDOW;
pub use qmf::QMF_WINDOW;
pub use quant::{
    CLC_BIT_LENGTH_TABLE, CTX_A_MASKH, CTX_A_MASKS, HCTBL0_CODES, HCTBL1_CODES,
    HUFF_COUNTS_PER_IDWL, ITB_GROUP_TABLE, ITFB_IDWL_CEILING_INIT, NGRP_FOR_SPEC, NGRP_FOR_TONE,
    NPKS_FOR_SPEC, NPKS_FOR_TONE, NSPS1024_TABLE, NSTEPS_TABLE, QTSTART_TABLE, SCALE_FACTOR_TABLE,
    TONE_FREQ_CONST, TONE_FREQ_DIVIDE, WIDTH_TABLE, WORD_LENGTH_TABLE,
};
