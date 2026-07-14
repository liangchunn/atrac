//! Pure library for the classic ATRAC3 encoder: DSP (QMF, MDCT, gain
//! control), psychoacoustic decisions, quantization, and bitstream packing.
//!
//! WAV decoding and filesystem policy stay outside this crate; the streaming
//! encoder writes its ATRAC3 container to a caller-provided `Write` sink.

#[allow(dead_code)]
mod analysis;
mod config;
#[allow(dead_code)]
mod core;
mod encoder;
#[allow(dead_code, unused_imports)]
mod tables;

pub use config::{Atrac3Profile, ChannelMode, UnsupportedProfile};
pub use encoder::stream::{
    Atrac3StreamEncoder as Atrac3Encoder, Atrac3StreamError as EncodeError,
    Atrac3StreamSummary as EncodeSummary, EncodePhase, EncodeProgress, PCM_BLOCK_FRAMES,
    WriteStage,
};
