pub const STATUS_OK: u32 = 0;
pub const STATUS_ERROR: u32 = 0x8000_0000;

pub const ERROR_OK: u32 = 0;
pub const ERROR_NOT_INITIALIZED: u32 = 0x113;
pub const ERROR_ENCODE_ALREADY_INITIALIZED: u32 = 0x120;
pub const ERROR_WRONG_INITIALIZED_STATE: u32 = 0x122;
pub const ERROR_UNSUPPORTED_TARGET: u32 = 0x10f;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderErrorKind {
    InvalidState,
    UnsupportedTarget,
    CoreNotImplemented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderError {
    pub status: u32,
    pub error_code: u32,
    pub kind: EncoderErrorKind,
}

impl EncoderError {
    pub const fn invalid_state(error_code: u32) -> Self {
        Self {
            status: STATUS_ERROR,
            error_code,
            kind: EncoderErrorKind::InvalidState,
        }
    }

    pub const fn unsupported_target() -> Self {
        Self {
            status: STATUS_ERROR,
            error_code: ERROR_UNSUPPORTED_TARGET,
            kind: EncoderErrorKind::UnsupportedTarget,
        }
    }

    pub const fn core_not_implemented() -> Self {
        Self {
            status: STATUS_ERROR,
            error_code: ERROR_OK,
            kind: EncoderErrorKind::CoreNotImplemented,
        }
    }
}
