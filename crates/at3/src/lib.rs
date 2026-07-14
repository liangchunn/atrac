//! Classic ATRAC3 encoder with a validated profile and streaming lifecycle.
//!
//! Input chunks are deinterleaved signed 16-bit PCM at 44.1 kHz, with one
//! channel for mono profiles and two for stereo profiles. Ask
//! [`Atrac3Encoder::expected_next_chunk_frames`] for each chunk length. The
//! encoder owns priming and tail flushing and writes a RIFF/WAVE ATRAC3 file to
//! the caller's [`std::io::Write`] sink; callers must not add codec delay or
//! padding themselves.
//!
//! # Example
//!
//! ```
//! # fn encode(pcm: &[i16]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
//! let profile = at3::Atrac3Profile::new(66, 1)?;
//! let mut encoder = at3::Atrac3Encoder::new(Vec::new(), profile, pcm.len() as u32)?;
//! let mut offset = 0;
//! while let Some(frames) = encoder.expected_next_chunk_frames() {
//!     encoder.push_pcm(&[&pcm[offset..offset + frames]])?;
//!     offset += frames;
//! }
//! let (bytes, summary) = encoder.finish()?;
//! assert_eq!(summary.file_bytes, bytes.len() as u64);
//! # Ok(bytes)
//! # }
//! ```
//!
//! The internal layer boundaries and parity policy are documented in the
//! workspace's `docs/architecture.md`.

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
