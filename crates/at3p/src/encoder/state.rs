use super::errors::ERROR_OK;
use super::profile::ATRAC3PLUS_352;

pub const ATX_OFFSET_CHANNELS: usize = 0x08;
pub const ATX_OFFSET_BLOCK_COUNT: usize = 0x0c;
pub const ATX_OFFSET_FRAME_BYTES: usize = 0x10;
pub const ATX_OFFSET_ERROR_CODE: usize = 0x14;
pub const ATX_OFFSET_ENCODE_DELAY_REMAINING: usize = 0x18;
pub const ATX_OFFSET_FLUSH_REMAINING: usize = 0x1c;
pub const ATX_OFFSET_INPUT_CHANNELS: usize = 0x20;
pub const ATX_OFFSET_FIRST_BLOCK_ERROR: usize = 0x34;
pub const ATX_FIRST_OUTPUT_CORE_CALL_INDEX: u32 = 7;

pub const ATX_CHANNEL_OFFSET_INDEX: usize = 0x00;
pub const ATX_CHANNEL_OFFSET_SHARED_CONFIG: usize = 0x04;
pub const ATX_CHANNEL_OFFSET_IDCT_BAND_START: usize = 0x1074;
pub const ATX_CHANNEL_OFFSET_IDCT_MODE: usize = 0x1078;
pub const ATX_CHANNEL_OFFSET_IDCT_AUX0: usize = 0x107c;
pub const ATX_CHANNEL_OFFSET_IDCT_AUX1: usize = 0x1080;
pub const ATX_CHANNEL_OFFSET_IDWL_MODE: usize = 0x1c70c;
pub const ATX_CHANNEL_OFFSET_IDWL_AUX0: usize = 0x1c710;
pub const ATX_CHANNEL_OFFSET_IDWL_AUX1: usize = 0x1c714;
pub const ATX_CHANNEL_OFFSET_IDWL_AUX2: usize = 0x1c718;
pub const ATX_CHANNEL_OFFSET_IDWL_SELECTOR_INDEX: usize = 0x1c71c;
pub const ATX_CHANNEL_OFFSET_IDWL_SELECTOR_KIND: usize = 0x1c720;
pub const ATX_CHANNEL_OFFSET_IDWL_START: usize = 0x1c724;
pub const ATX_CHANNEL_OFFSET_IDWL_COUNT: usize = 0x1c728;
pub const ATX_CHANNEL_OFFSET_IDWL_STRIDE: usize = 0x1c72c;
pub const ATX_CHANNEL_OFFSET_IDSF_MODE: usize = 0x1c73c;
pub const ATX_CHANNEL_OFFSET_IDSF_START: usize = 0x1c740;
pub const ATX_CHANNEL_OFFSET_IDSF_COUNT: usize = 0x1c744;
pub const ATX_CHANNEL_OFFSET_IDSF_FIELD_0X1C750: usize = 0x1c750;

pub const ATX_SHARED_CONFIG_OFFSET_FIELD_0X90: usize = 0x90;
pub const ATX_SHARED_CONFIG_OFFSET_BLOCK_HEADER_MODE_BITS: usize = 0xa0;
pub const ATX_SHARED_CONFIG_OFFSET_CHANNEL_COUNT: usize = 0xa8;
pub const ATX_SHARED_CONFIG_OFFSET_SCALE_FACTOR_BAND_COUNT: usize = 0xb0;
pub const ATX_SHARED_CONFIG_OFFSET_QUANT_UNIT_COUNT: usize = 0xc4;
pub const ATX_SHARED_CONFIG_OFFSET_FIELD_0X118: usize = 0x118;
pub const ATX_SHARED_CONFIG_OFFSET_BANDWIDTH_MODE: usize = 0x1e8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtxHandleState {
    pub channels: u32,
    pub block_count: u32,
    pub frame_bytes: u32,
    pub error_code: u32,
    pub encode_delay_remaining: u32,
    pub flush_remaining: u32,
    pub input_channels: u32,
    pub block_errors: Vec<u32>,
}

impl AtxHandleState {
    pub fn target_352_initial() -> Self {
        Self {
            channels: u32::from(ATRAC3PLUS_352.channels),
            block_count: 1,
            frame_bytes: ATRAC3PLUS_352.frame_bytes,
            error_code: ERROR_OK,
            encode_delay_remaining: 7,
            flush_remaining: 9,
            input_channels: u32::from(ATRAC3PLUS_352.channels),
            block_errors: vec![ERROR_OK],
        }
    }

    pub fn target_352_first_output() -> Self {
        let mut state = Self::target_352_initial();
        state.encode_delay_remaining = 0;
        state
    }

    pub fn frame_bit_budget(&self) -> u32 {
        self.frame_bytes * 8 - self.block_count * 2 - 3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtxSharedConfigState {
    pub field_0x90: u32,
    pub block_header_mode_bits: u32,
    pub channel_count: u32,
    pub scale_factor_band_count: u32,
    pub quant_unit_count: u32,
    pub field_0x118: u32,
    pub bandwidth_mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtxChannelState {
    pub channel_index: u32,
    pub shared_config: AtxSharedConfigState,
    pub idct_band_start: u32,
    pub idct_mode: u32,
    pub idct_aux0: u32,
    pub idct_aux1: u32,
    pub idwl_mode: u32,
    pub idwl_aux0: u32,
    pub idwl_aux1: u32,
    pub idwl_aux2: u32,
    pub idwl_selector_index: u32,
    pub idwl_selector_kind: u32,
    pub idwl_start: u32,
    pub idwl_count: u32,
    pub idwl_stride: u32,
    pub idsf_mode: u32,
    pub idsf_start: u32,
    pub idsf_count: u32,
    pub idsf_field_0x1c750: u32,
}

impl AtxChannelState {
    pub fn idwl_dispatch_index(&self) -> u32 {
        packer_dispatch_index(self.idwl_mode, self.channel_index)
    }

    pub fn idsf_dispatch_index(&self) -> u32 {
        packer_dispatch_index(self.idsf_mode, self.channel_index)
    }

    pub fn idct_dispatch_index(&self) -> u32 {
        packer_dispatch_index(self.idct_mode, self.channel_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtxBlockState {
    pub channels: Vec<AtxChannelState>,
}

impl AtxBlockState {
    pub fn target_352_first_output_packer_state() -> Self {
        let shared_config = AtxSharedConfigState {
            field_0x90: 1,
            block_header_mode_bits: 1,
            channel_count: 2,
            scale_factor_band_count: 32,
            quant_unit_count: 32,
            field_0x118: 0,
            bandwidth_mode: 30,
        };

        Self {
            channels: vec![
                AtxChannelState {
                    channel_index: 0,
                    shared_config,
                    idct_band_start: 0,
                    idct_mode: 2,
                    idct_aux0: 32,
                    idct_aux1: 0,
                    idwl_mode: 3,
                    idwl_aux0: 0,
                    idwl_aux1: 0,
                    idwl_aux2: 0,
                    idwl_selector_index: 0,
                    idwl_selector_kind: 1,
                    idwl_start: 0,
                    idwl_count: 32,
                    idwl_stride: 0,
                    idsf_mode: 3,
                    idsf_start: 0,
                    idsf_count: 5,
                    idsf_field_0x1c750: 1,
                },
                AtxChannelState {
                    channel_index: 1,
                    shared_config,
                    idct_band_start: 0,
                    idct_mode: 3,
                    idct_aux0: 31,
                    idct_aux1: 0,
                    idwl_mode: 1,
                    idwl_aux0: 0,
                    idwl_aux1: 0,
                    idwl_aux2: 0,
                    idwl_selector_index: 0,
                    idwl_selector_kind: 0,
                    idwl_start: 0,
                    idwl_count: 32,
                    idwl_stride: 0,
                    idsf_mode: 1,
                    idsf_start: 0,
                    idsf_count: 0,
                    idsf_field_0x1c750: 0,
                },
            ],
        }
    }
}

fn packer_dispatch_index(mode: u32, channel_index: u32) -> u32 {
    (mode & 3) + ((channel_index & 1) * 4)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtxEncoderState {
    pub handle: AtxHandleState,
    pub blocks: Vec<AtxBlockState>,
}

impl AtxEncoderState {
    pub fn target_352_first_output_packer_state() -> Self {
        Self {
            handle: AtxHandleState::target_352_first_output(),
            blocks: vec![AtxBlockState::target_352_first_output_packer_state()],
        }
    }

    pub fn block(&self, index: usize) -> Option<&AtxBlockState> {
        self.blocks.get(index)
    }

    pub fn channel(&self, block_index: usize, channel_index: usize) -> Option<&AtxChannelState> {
        self.block(block_index)
            .and_then(|block| block.channels.get(channel_index))
    }
}
