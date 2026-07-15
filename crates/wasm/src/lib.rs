//! Browser-oriented WebAssembly bindings for the ATRAC3 encoders.

use std::fmt;

use at3::{ATRAC3_PROFILES, Atrac3Encoder as CoreAtrac3Encoder};
use at3p::{
    ATRAC3PLUS_MONO_PROFILES, ATRAC3PLUS_STEREO_PROFILES,
    Atrac3plusEncoder as CoreAtrac3plusEncoder,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = atrac3Bitrates)]
pub fn atrac3_bitrates(channels: u16) -> Vec<u32> {
    ATRAC3_PROFILES
        .iter()
        .filter(|profile| profile.channels() == channels)
        .map(|profile| profile.bitrate_kbps())
        .collect()
}

#[wasm_bindgen(js_name = atrac3plusBitrates)]
pub fn atrac3plus_bitrates(channels: u16) -> Vec<u32> {
    ATRAC3PLUS_MONO_PROFILES
        .iter()
        .chain(ATRAC3PLUS_STEREO_PROFILES.iter())
        .filter(|profile| profile.channels() == channels)
        .map(|profile| profile.bitrate_kbps())
        .collect()
}

#[wasm_bindgen(js_name = EncodeProgress)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmEncodeProgress {
    phase: EncodePhase,
    completed_steps: u32,
    total_steps: u32,
    completed_output_frames: u32,
    total_output_frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncodePhase {
    Preparing,
    Encoding,
    Flushing,
}

impl Default for WasmEncodeProgress {
    fn default() -> Self {
        Self {
            phase: EncodePhase::Preparing,
            completed_steps: 0,
            total_steps: 0,
            completed_output_frames: 0,
            total_output_frames: 0,
        }
    }
}

#[wasm_bindgen(js_class = EncodeProgress)]
impl WasmEncodeProgress {
    #[wasm_bindgen(getter)]
    pub fn phase(&self) -> String {
        match self.phase {
            EncodePhase::Preparing => "preparing",
            EncodePhase::Encoding => "encoding",
            EncodePhase::Flushing => "flushing",
        }
        .to_owned()
    }

    #[wasm_bindgen(getter, js_name = completedSteps)]
    pub fn completed_steps(&self) -> u32 {
        self.completed_steps
    }

    #[wasm_bindgen(getter, js_name = totalSteps)]
    pub fn total_steps(&self) -> u32 {
        self.total_steps
    }

    #[wasm_bindgen(getter, js_name = completedOutputFrames)]
    pub fn completed_output_frames(&self) -> u32 {
        self.completed_output_frames
    }

    #[wasm_bindgen(getter, js_name = totalOutputFrames)]
    pub fn total_output_frames(&self) -> u32 {
        self.total_output_frames
    }
}

#[wasm_bindgen(js_name = Atrac3Encoder)]
pub struct WasmAtrac3Encoder {
    encoder: Option<CoreAtrac3Encoder<Vec<u8>>>,
    scratch: PcmScratch,
    progress: WasmEncodeProgress,
}

#[wasm_bindgen(js_class = Atrac3Encoder)]
impl WasmAtrac3Encoder {
    #[wasm_bindgen(constructor)]
    pub fn new(
        bitrate_kbps: u32,
        channels: u16,
        input_sample_frames: u32,
    ) -> Result<WasmAtrac3Encoder, JsError> {
        let profile = at3::Atrac3Profile::new(bitrate_kbps, channels)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let encoder = CoreAtrac3Encoder::new(Vec::new(), profile, input_sample_frames)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Self {
            encoder: Some(encoder),
            scratch: PcmScratch::new(channels as usize, at3::PCM_BLOCK_FRAMES),
            progress: WasmEncodeProgress::default(),
        })
    }

    #[wasm_bindgen(js_name = expectedNextChunkFrames)]
    pub fn expected_next_chunk_frames(&self) -> u32 {
        self.encoder
            .as_ref()
            .and_then(CoreAtrac3Encoder::expected_next_chunk_frames)
            .unwrap_or(0) as u32
    }

    #[wasm_bindgen(js_name = pushPcm)]
    pub fn push_pcm(&mut self, pcm_le_bytes: &[u8]) -> Result<WasmEncodeProgress, JsError> {
        let encoder = self
            .encoder
            .as_mut()
            .ok_or_else(|| JsError::new("ATRAC3 encoder is already finished"))?;
        let expected_frames = encoder
            .expected_next_chunk_frames()
            .ok_or_else(|| JsError::new("ATRAC3 PCM input is already complete"))?;
        self.scratch
            .decode(pcm_le_bytes, expected_frames)
            .map_err(|error| JsError::new(&error.to_string()))?;

        let mut latest = None;
        let result = match self.scratch.channels() {
            [mono] => encoder.push_pcm_with_progress(&[mono], |progress| {
                latest = Some(progress.into());
            }),
            [left, right] => encoder.push_pcm_with_progress(&[left, right], |progress| {
                latest = Some(progress.into());
            }),
            _ => unreachable!("validated ATRAC3 channel count"),
        };
        result.map_err(|error| JsError::new(&error.to_string()))?;
        if let Some(progress) = latest {
            self.progress = progress;
        }
        Ok(self.progress)
    }

    #[wasm_bindgen(js_name = currentProgress)]
    pub fn current_progress(&self) -> WasmEncodeProgress {
        self.progress
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, JsError> {
        let encoder = self
            .encoder
            .take()
            .ok_or_else(|| JsError::new("ATRAC3 encoder is already finished"))?;
        let mut latest = None;
        let (bytes, _) = encoder
            .finish_with_progress(|progress| latest = Some(progress.into()))
            .map_err(|error| JsError::new(&error.to_string()))?;
        if let Some(progress) = latest {
            self.progress = progress;
        }
        Ok(bytes)
    }
}

#[wasm_bindgen(js_name = Atrac3plusEncoder)]
pub struct WasmAtrac3plusEncoder {
    encoder: Option<CoreAtrac3plusEncoder<Vec<u8>>>,
    scratch: PcmScratch,
    progress: WasmEncodeProgress,
}

#[wasm_bindgen(js_class = Atrac3plusEncoder)]
impl WasmAtrac3plusEncoder {
    #[wasm_bindgen(constructor)]
    pub fn new(
        bitrate_kbps: u32,
        channels: u16,
        input_sample_frames: u32,
    ) -> Result<WasmAtrac3plusEncoder, JsError> {
        let profile = at3p::Atrac3plusProfile::new(bitrate_kbps, channels)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let encoder = CoreAtrac3plusEncoder::new(Vec::new(), &profile, input_sample_frames)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Self {
            encoder: Some(encoder),
            scratch: PcmScratch::new(channels as usize, at3p::PCM_BLOCK_FRAMES),
            progress: WasmEncodeProgress::default(),
        })
    }

    #[wasm_bindgen(js_name = expectedNextChunkFrames)]
    pub fn expected_next_chunk_frames(&self) -> u32 {
        self.encoder
            .as_ref()
            .and_then(CoreAtrac3plusEncoder::expected_next_chunk_frames)
            .unwrap_or(0) as u32
    }

    #[wasm_bindgen(js_name = pushPcm)]
    pub fn push_pcm(&mut self, pcm_le_bytes: &[u8]) -> Result<WasmEncodeProgress, JsError> {
        let encoder = self
            .encoder
            .as_mut()
            .ok_or_else(|| JsError::new("ATRAC3plus encoder is already finished"))?;
        let expected_frames = encoder
            .expected_next_chunk_frames()
            .ok_or_else(|| JsError::new("ATRAC3plus PCM input is already complete"))?;
        self.scratch
            .decode(pcm_le_bytes, expected_frames)
            .map_err(|error| JsError::new(&error.to_string()))?;

        let mut latest = None;
        let result = match self.scratch.channels() {
            [mono] => encoder.push_pcm_with_progress(&[mono], |progress| {
                latest = Some(progress.into());
            }),
            [left, right] => encoder.push_pcm_with_progress(&[left, right], |progress| {
                latest = Some(progress.into());
            }),
            _ => unreachable!("validated ATRAC3plus channel count"),
        };
        result.map_err(|error| JsError::new(&error.to_string()))?;
        if let Some(progress) = latest {
            self.progress = progress;
        }
        Ok(self.progress)
    }

    #[wasm_bindgen(js_name = currentProgress)]
    pub fn current_progress(&self) -> WasmEncodeProgress {
        self.progress
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, JsError> {
        let encoder = self
            .encoder
            .take()
            .ok_or_else(|| JsError::new("ATRAC3plus encoder is already finished"))?;
        let mut latest = None;
        let (bytes, _) = encoder
            .finish_with_progress(|progress| latest = Some(progress.into()))
            .map_err(|error| JsError::new(&error.to_string()))?;
        if let Some(progress) = latest {
            self.progress = progress;
        }
        Ok(bytes)
    }
}

impl From<at3::EncodeProgress> for WasmEncodeProgress {
    fn from(progress: at3::EncodeProgress) -> Self {
        Self {
            phase: match progress.phase {
                at3::EncodePhase::Encoding => EncodePhase::Encoding,
                at3::EncodePhase::Flushing => EncodePhase::Flushing,
            },
            completed_steps: progress.completed_steps,
            total_steps: progress.total_steps,
            completed_output_frames: progress.completed_output_frames,
            total_output_frames: progress.total_output_frames,
        }
    }
}

impl From<at3p::EncodeProgress> for WasmEncodeProgress {
    fn from(progress: at3p::EncodeProgress) -> Self {
        Self {
            phase: match progress.phase {
                at3p::EncodePhase::Encoding => EncodePhase::Encoding,
                at3p::EncodePhase::Flushing => EncodePhase::Flushing,
            },
            completed_steps: progress.completed_steps,
            total_steps: progress.total_steps,
            completed_output_frames: progress.completed_output_frames,
            total_output_frames: progress.total_output_frames,
        }
    }
}

#[derive(Debug)]
struct PcmScratch {
    channels: Vec<Vec<i16>>,
}

impl PcmScratch {
    fn new(channel_count: usize, block_frames: usize) -> Self {
        Self {
            channels: (0..channel_count)
                .map(|_| Vec::with_capacity(block_frames))
                .collect(),
        }
    }

    fn decode(&mut self, bytes: &[u8], frames: usize) -> Result<(), PcmDecodeError> {
        let channel_count = self.channels.len();
        let expected = frames
            .checked_mul(channel_count)
            .and_then(|samples| samples.checked_mul(size_of::<i16>()))
            .ok_or(PcmDecodeError::SizeOverflow)?;
        if bytes.len() != expected {
            return Err(PcmDecodeError::WrongByteLength {
                expected,
                actual: bytes.len(),
            });
        }
        for channel in &mut self.channels {
            channel.clear();
        }
        for frame in bytes.chunks_exact(channel_count * size_of::<i16>()) {
            for (channel_index, channel) in self.channels.iter_mut().enumerate() {
                let offset = channel_index * size_of::<i16>();
                channel.push(i16::from_le_bytes([frame[offset], frame[offset + 1]]));
            }
        }
        Ok(())
    }

    fn channels(&self) -> &[Vec<i16>] {
        &self.channels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PcmDecodeError {
    SizeOverflow,
    WrongByteLength { expected: usize, actual: usize },
}

impl fmt::Display for PcmDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeOverflow => write!(formatter, "PCM chunk byte length overflowed"),
            Self::WrongByteLength { expected, actual } => write!(
                formatter,
                "PCM chunk has {actual} bytes; expected exactly {expected}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_lists_follow_codec_profiles() {
        assert_eq!(atrac3_bitrates(1), [52, 66]);
        assert_eq!(atrac3_bitrates(2), [66, 105, 132]);
        assert_eq!(atrac3plus_bitrates(1), [32, 48, 64, 96, 128]);
        assert_eq!(
            atrac3plus_bitrates(2),
            [48, 64, 96, 128, 160, 192, 256, 320, 352]
        );
        assert!(atrac3_bitrates(3).is_empty());
        assert!(atrac3plus_bitrates(3).is_empty());
    }

    #[test]
    fn pcm_chunks_are_deinterleaved_as_little_endian() {
        let mut scratch = PcmScratch::new(2, 2);
        scratch
            .decode(&[0x34, 0x12, 0xcc, 0xff, 0x00, 0x80, 0xff, 0x7f], 2)
            .unwrap();
        assert_eq!(
            scratch.channels(),
            &[vec![0x1234, i16::MIN], vec![-52, i16::MAX]]
        );
    }

    #[test]
    fn pcm_chunks_require_the_exact_expected_size() {
        let mut scratch = PcmScratch::new(2, 2);
        assert_eq!(
            scratch.decode(&[0; 6], 2),
            Err(PcmDecodeError::WrongByteLength {
                expected: 8,
                actual: 6,
            })
        );
    }

    fn generated_pcm_bytes(channels: usize, frames: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(channels * frames * 2);
        for frame in 0..frames {
            for channel in 0..channels {
                let sample = ((frame as i32 * 43 + channel as i32 * 997) % 50_003 - 25_001) as i16;
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn atrac3_binding_matches_the_native_stream() {
        let frames = 2049usize;
        let bytes = generated_pcm_bytes(2, frames);
        let mut binding = WasmAtrac3Encoder::new(132, 2, frames as u32).unwrap();
        let mut offset = 0;
        while let Some(chunk_frames) = binding
            .encoder
            .as_ref()
            .and_then(CoreAtrac3Encoder::expected_next_chunk_frames)
        {
            let chunk_bytes = chunk_frames * 4;
            binding
                .push_pcm(&bytes[offset..offset + chunk_bytes])
                .unwrap();
            offset += chunk_bytes;
        }
        let actual = binding.finish().unwrap();

        let profile = at3::Atrac3Profile::new(132, 2).unwrap();
        let mut native = CoreAtrac3Encoder::new(Vec::new(), profile, frames as u32).unwrap();
        let mut scratch = PcmScratch::new(2, at3::PCM_BLOCK_FRAMES);
        let mut offset = 0;
        while let Some(chunk_frames) = native.expected_next_chunk_frames() {
            let chunk_bytes = chunk_frames * 4;
            scratch
                .decode(&bytes[offset..offset + chunk_bytes], chunk_frames)
                .unwrap();
            native
                .push_pcm(&[&scratch.channels()[0], &scratch.channels()[1]])
                .unwrap();
            offset += chunk_bytes;
        }
        let expected = native.finish().unwrap().0;
        assert_eq!(actual, expected);
        assert_eq!(binding.expected_next_chunk_frames(), 0);
    }

    #[test]
    fn atrac3plus_binding_matches_the_native_stream() {
        let frames = 6145usize;
        let bytes = generated_pcm_bytes(2, frames);
        let mut binding = WasmAtrac3plusEncoder::new(352, 2, frames as u32).unwrap();
        let mut offset = 0;
        while let Some(chunk_frames) = binding
            .encoder
            .as_ref()
            .and_then(CoreAtrac3plusEncoder::expected_next_chunk_frames)
        {
            let chunk_bytes = chunk_frames * 4;
            binding
                .push_pcm(&bytes[offset..offset + chunk_bytes])
                .unwrap();
            offset += chunk_bytes;
        }
        let actual = binding.finish().unwrap();

        let profile = at3p::Atrac3plusProfile::new(352, 2).unwrap();
        let mut native = CoreAtrac3plusEncoder::new(Vec::new(), &profile, frames as u32).unwrap();
        let mut scratch = PcmScratch::new(2, at3p::PCM_BLOCK_FRAMES);
        let mut offset = 0;
        while let Some(chunk_frames) = native.expected_next_chunk_frames() {
            let chunk_bytes = chunk_frames * 4;
            scratch
                .decode(&bytes[offset..offset + chunk_bytes], chunk_frames)
                .unwrap();
            native
                .push_pcm(&[&scratch.channels()[0], &scratch.channels()[1]])
                .unwrap();
            offset += chunk_bytes;
        }
        let expected = native.finish().unwrap().0;
        assert_eq!(actual, expected);
        assert_eq!(binding.expected_next_chunk_frames(), 0);
    }
}
