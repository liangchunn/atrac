use super::atx_config::{decode_target_codec_info, serialize_config};
use super::errors::{
    ERROR_ENCODE_ALREADY_INITIALIZED, ERROR_NOT_INITIALIZED, ERROR_OK,
    ERROR_WRONG_INITIALIZED_STATE, EncoderError,
};
use super::profile::ATRAC3PLUS_352;

const NATIVE_DEFAULT_CODEC_INFO: u32 = 0x0100_282e;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferRequest {
    pub input_unit_bytes: u32,
    pub input_buffer_bytes: u32,
    pub frame_bytes: u32,
    pub output_buffer_bytes: u32,
}

pub const TARGET_BUFFER_REQUEST: BufferRequest = BufferRequest {
    input_unit_bytes: 8192,
    input_buffer_bytes: 8192,
    frame_bytes: 2048,
    output_buffer_bytes: 8194,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicEncoderHandle {
    codec_info: u32,
    codec_info_set: bool,
    encode_algorithm: u32,
    encode_initialized: bool,
    last_error: u32,
    config_bytes: Option<[u8; 2]>,
    /// Per-rate frame bytes decoded from the codec_info at `init_encode`. Feeds
    /// `buffer_request` (native `handle+0x3c`, rate-dependent field).
    frame_bytes: Option<u32>,
    /// Channel count decoded from the codec_info's channel-mode field at
    /// `init_encode` (2 stereo / 1 mono, native `handle+0x94`). Feeds the
    /// channel-dependent `buffer_request` input size (docs/14 §0.1).
    input_channels: Option<u32>,
}

impl Default for PublicEncoderHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl PublicEncoderHandle {
    pub fn new() -> Self {
        Self {
            codec_info: NATIVE_DEFAULT_CODEC_INFO,
            codec_info_set: false,
            encode_algorithm: 0,
            encode_initialized: false,
            last_error: ERROR_OK,
            config_bytes: None,
            frame_bytes: None,
            input_channels: None,
        }
    }

    pub fn codec_info(&self) -> u32 {
        self.codec_info
    }

    pub fn encode_algorithm(&self) -> u32 {
        self.encode_algorithm
    }

    pub fn is_encode_initialized(&self) -> bool {
        self.encode_initialized
    }

    pub fn last_error(&self) -> u32 {
        self.last_error
    }

    pub fn config_bytes(&self) -> Option<[u8; 2]> {
        self.config_bytes
    }

    pub fn set_codec_info(&mut self, codec_info: u32) -> Result<(), EncoderError> {
        if self.encode_initialized {
            return self.fail(ERROR_WRONG_INITIALIZED_STATE);
        }
        self.last_error = ERROR_OK;
        self.codec_info = codec_info;
        self.codec_info_set = true;
        Ok(())
    }

    pub fn set_encode_algorithm(&mut self, encode_algorithm: u32) -> Result<(), EncoderError> {
        if self.encode_initialized {
            return self.fail(ERROR_ENCODE_ALREADY_INITIALIZED);
        }
        self.last_error = ERROR_OK;
        self.encode_algorithm = encode_algorithm;
        Ok(())
    }

    pub fn init_encode(&mut self) -> Result<(), EncoderError> {
        if self.encode_initialized {
            return self.fail(ERROR_WRONG_INITIALIZED_STATE);
        }
        if !self.codec_info_set || self.encode_algorithm != ATRAC3PLUS_352.encode_algorithm() {
            return self.unsupported_target();
        }

        // Decode the codec_info's per-rate frame bytes and channel mode (must
        // exact-row-match one of the nine stereo OR five mono rows) and
        // serialize the config bytes from it — no longer hardcoded to the 352
        // frame size / stereo channel mode, so all nine stereo AND five mono
        // codec_info words init successfully with config bytes = the low two
        // big-endian bytes (e.g. `[0x28, nn]` stereo, `[0x24, nn]` mono).
        let decoded = decode_target_codec_info(self.codec_info)
            .map_err(|_| self.set_unsupported_target_error())?;
        let config = serialize_config(
            decoded.sample_rate,
            decoded.channel_mode,
            decoded.frame_bytes,
        )
        .map_err(|_| self.set_unsupported_target_error())?;

        self.last_error = ERROR_OK;
        self.config_bytes = Some(config);
        self.frame_bytes = Some(decoded.frame_bytes);
        self.input_channels = Some(u32::from(decoded.channel_mode));
        self.encode_initialized = true;
        Ok(())
    }

    pub fn buffer_request(&mut self) -> Result<BufferRequest, EncoderError> {
        self.require_encode_initialized()?;
        self.last_error = ERROR_OK;
        // Native `atrac_get_buffer_request` ATRAC3plus branch (libatrac.c
        // 1301-1366, native 0x17640): input_unit = input_buffer =
        // handle_channels(+0x94) * bytes_per_sample(+0x2c == 2) * 0x800, and
        // output_buffer = literal 0x2002 = 8194 (channel- and rate-independent);
        // only frame_bytes (handle+0x3c) varies per rate. Channel-dependent
        // buffer_request_mono_by_rate.ndjson). `frame_bytes`/`input_channels`
        // are Some once `require_encode_initialized` passes.
        let input_bytes = self.input_channels.unwrap_or(2) * 2 * 0x800;
        Ok(BufferRequest {
            input_unit_bytes: input_bytes,
            input_buffer_bytes: input_bytes,
            frame_bytes: self.frame_bytes.unwrap_or(ATRAC3PLUS_352.frame_bytes()),
            output_buffer_bytes: 8194,
        })
    }

    pub fn encode(&mut self) -> Result<(), EncoderError> {
        self.require_encode_initialized()?;
        Err(EncoderError::core_not_implemented())
    }

    pub fn flush_encode(&mut self) -> Result<(), EncoderError> {
        self.require_encode_initialized()?;
        Err(EncoderError::core_not_implemented())
    }

    pub fn free_encode(&mut self) -> Result<(), EncoderError> {
        self.require_encode_initialized()?;
        self.last_error = ERROR_OK;
        self.encode_initialized = false;
        self.config_bytes = None;
        self.frame_bytes = None;
        self.input_channels = None;
        Ok(())
    }

    fn require_encode_initialized(&mut self) -> Result<(), EncoderError> {
        if self.encode_initialized {
            Ok(())
        } else {
            self.fail(ERROR_NOT_INITIALIZED)
        }
    }

    fn fail<T>(&mut self, error_code: u32) -> Result<T, EncoderError> {
        self.last_error = error_code;
        Err(EncoderError::invalid_state(error_code))
    }

    fn unsupported_target<T>(&mut self) -> Result<T, EncoderError> {
        Err(self.set_unsupported_target_error())
    }

    fn set_unsupported_target_error(&mut self) -> EncoderError {
        let error = EncoderError::unsupported_target();
        self.last_error = error.error_code;
        error
    }
}
