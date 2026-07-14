//! Pure library for the classic ATRAC3 encoder: DSP (QMF, MDCT, gain
//! control), psychoacoustic decisions, quantization, and bitstream packing.
//!
//! WAV decoding and filesystem policy stay outside this crate; the streaming
//! encoder writes its ATRAC3 container to a caller-provided `Write` sink.

pub mod config;
pub mod dsp;
pub mod encoder;
pub mod tables;

pub use config::{Atrac3Profile, ChannelMode, UnsupportedProfile};
