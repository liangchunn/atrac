use super::generated::{
    G_A_SPEC_WL1_A, G_A_SPEC_WL1_B, G_A_SPEC_WL1_C, G_A_SPEC_WL1_D, G_A_SPEC_WL1_E, G_A_SPEC_WL1_F,
    G_A_SPEC_WL1_G, G_A_SPEC_WL1_H, G_A_SPEC_WL1_I, G_A_SPEC_WL1_J, G_A_SPEC_WL1_K, G_A_SPEC_WL2_A,
    G_A_SPEC_WL2_B, G_A_SPEC_WL2_C, G_A_SPEC_WL2_D, G_A_SPEC_WL2_E, G_A_SPEC_WL2_F, G_A_SPEC_WL2_G,
    G_A_SPEC_WL2_H, G_A_SPEC_WL2_I, G_A_SPEC_WL2_J, G_A_SPEC_WL2_K, G_A_SPEC_WL2_L, G_A_SPEC_WL2_M,
    G_A_SPEC_WL2_N, G_A_SPEC_WL3_A, G_A_SPEC_WL3_B, G_A_SPEC_WL3_C, G_A_SPEC_WL3_D, G_A_SPEC_WL3_E,
    G_A_SPEC_WL3_F, G_A_SPEC_WL3_G, G_A_SPEC_WL3_H, G_A_SPEC_WL3_I, G_A_SPEC_WL3_J, G_A_SPEC_WL3_K,
    G_A_SPEC_WL3_L, G_A_SPEC_WL3_M, G_A_SPEC_WL3_N, G_A_SPEC_WL4_A, G_A_SPEC_WL4_B, G_A_SPEC_WL4_C,
    G_A_SPEC_WL4_D, G_A_SPEC_WL4_E, G_A_SPEC_WL4_F, G_A_SPEC_WL4_G, G_A_SPEC_WL4_H, G_A_SPEC_WL4_I,
    G_A_SPEC_WL4_J, G_A_SPEC_WL4_K, G_A_SPEC_WL4_L, G_A_SPEC_WL5_A, G_A_SPEC_WL5_B, G_A_SPEC_WL5_C,
    G_A_SPEC_WL5_D, G_A_SPEC_WL5_E, G_A_SPEC_WL5_F, G_A_SPEC_WL5_G, G_A_SPEC_WL5_H, G_A_SPEC_WL5_I,
    G_A_SPEC_WL5_J, G_A_SPEC_WL5_K, G_A_SPEC_WL5_L, G_A_SPEC_WL6_A, G_A_SPEC_WL6_B, G_A_SPEC_WL6_C,
    G_A_SPEC_WL6_D, G_A_SPEC_WL6_E, G_A_SPEC_WL6_F, G_A_SPEC_WL6_G, G_A_SPEC_WL6_H, G_A_SPEC_WL6_I,
    G_A_SPEC_WL6_J, G_A_SPEC_WL6_K, G_A_SPEC_WL6_L, G_A_SPEC_WL7_A, G_A_SPEC_WL7_B, G_A_SPEC_WL7_C,
    G_A_SPEC_WL7_D, G_A_SPEC_WL7_E, G_A_SPEC_WL7_F, G_A_SPEC_WL7_G, G_A_SPEC_WL7_H, G_A_SPEC_WL7_I,
    G_A_SPEC_WL7_J, G_A_SPEC_WL7_K, G_A_SPEC_WL7_L, G_AAA_HCSPEC,
};
use super::huffman::{HUFFMAN_CODE_ENTRY_BYTES, HuffmanCodeEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralTableInfo {
    pub symbol: &'static str,
    pub word_len: u8,
    pub table: char,
    pub native_addr: u32,
    pub byte_len: usize,
}

impl SpectralTableInfo {
    pub fn generated_pack_table(self) -> Option<SpectralPackTable> {
        spectral_generated_pack_table(self.symbol)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralPackTable {
    symbol: &'static str,
    native_addr: u32,
    bytes: &'static [u8],
}

impl SpectralPackTable {
    pub const fn symbol(self) -> &'static str {
        self.symbol
    }

    pub const fn native_addr(self) -> u32 {
        self.native_addr
    }

    pub const fn bytes(self) -> &'static [u8] {
        self.bytes
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / HUFFMAN_CODE_ENTRY_BYTES
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn entry(self, index: usize) -> Option<HuffmanCodeEntry> {
        let start = index.checked_mul(HUFFMAN_CODE_ENTRY_BYTES)?;
        let raw = self.bytes.get(start..start + HUFFMAN_CODE_ENTRY_BYTES)?;
        Some(HuffmanCodeEntry {
            code: u16::from_le_bytes(raw[0..2].try_into().ok()?),
            bit_len: raw[2],
            reserved: raw[3],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralDescriptorSlot {
    pub selector: char,
    pub word_len: u8,
    pub slot_index: usize,
    pub descriptor_addr: u32,
    pub pack_table_symbol: &'static str,
    pub pack_table_native_addr: u32,
    pub decode_table_symbol: &'static str,
    pub decode_table_native_addr: u32,
}

impl SpectralDescriptorSlot {
    pub fn pack_table(self) -> Option<&'static SpectralTableInfo> {
        SPECTRAL_TABLES
            .iter()
            .find(|entry| entry.symbol == self.pack_table_symbol)
    }

    pub fn metadata(self) -> Option<SpectralDescriptorMetadata> {
        spectral_descriptor_metadata(self.slot_index)
    }

    pub fn file_bytes(self) -> Option<&'static [u8]> {
        spectral_descriptor_slot_bytes(self.slot_index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralDescriptorMetadata {
    pub group_size: u8,
    pub run_length: u8,
    pub grouped_symbol_shift: u8,
    pub signed: bool,
    pub bit_width: u8,
    pub magnitude_mask: u16,
}

impl SpectralDescriptorMetadata {
    pub fn grouped_symbol_count(self, nsps: u8) -> usize {
        usize::from(nsps) >> u32::from(self.grouped_symbol_shift & 0x1f)
    }
}

pub const SPECTRAL_TABLE_COUNT: usize = 87;
pub const SPECTRAL_DESCRIPTOR_MATRIX_ADDR: u32 = 0x000f_4240;
pub const SPECTRAL_DESCRIPTOR_ENTRY_BYTES: usize = 24;
pub const SPECTRAL_DESCRIPTOR_GROUP_SIZE_OFFSET: usize = 0x11;
pub const SPECTRAL_DESCRIPTOR_RUN_LENGTH_OFFSET: usize = 0x12;
pub const SPECTRAL_DESCRIPTOR_GROUP_SHIFT_OFFSET: usize = 0x13;
pub const SPECTRAL_DESCRIPTOR_SIGNED_OFFSET: usize = 0x14;
pub const SPECTRAL_DESCRIPTOR_BIT_WIDTH_OFFSET: usize = 0x15;
pub const SPECTRAL_DESCRIPTOR_MAGNITUDE_MASK_OFFSET: usize = 0x16;
pub const SPECTRAL_DESCRIPTOR_WORD_LEN_COUNT: usize = 7;
pub const SPECTRAL_DESCRIPTOR_SELECTOR_COUNT: usize = 16;
pub const SPECTRAL_DESCRIPTOR_SLOT_COUNT: usize =
    SPECTRAL_DESCRIPTOR_WORD_LEN_COUNT * SPECTRAL_DESCRIPTOR_SELECTOR_COUNT;

macro_rules! spec_slot {
    (
        $selector:literal,
        $word_len:literal,
        $slot_index:literal,
        $pack_table_symbol:literal,
        $pack_table_native_addr:literal,
        $decode_table_symbol:literal,
        $decode_table_native_addr:literal
    ) => {
        SpectralDescriptorSlot {
            selector: $selector,
            word_len: $word_len,
            slot_index: $slot_index,
            descriptor_addr: SPECTRAL_DESCRIPTOR_MATRIX_ADDR
                + (($slot_index as u32) * (SPECTRAL_DESCRIPTOR_ENTRY_BYTES as u32)),
            pack_table_symbol: $pack_table_symbol,
            pack_table_native_addr: $pack_table_native_addr,
            decode_table_symbol: $decode_table_symbol,
            decode_table_native_addr: $decode_table_native_addr,
        }
    };
}

pub const SPECTRAL_TABLES: &[SpectralTableInfo] = &[
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_A",
        word_len: 1,
        table: 'A',
        native_addr: 0x000b9040,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_B",
        word_len: 1,
        table: 'B',
        native_addr: 0x000b8c40,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_C",
        word_len: 1,
        table: 'C',
        native_addr: 0x000b8840,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_D",
        word_len: 1,
        table: 'D',
        native_addr: 0x000b8800,
        byte_len: 64,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_E",
        word_len: 1,
        table: 'E',
        native_addr: 0x000b8400,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_F",
        word_len: 1,
        table: 'F',
        native_addr: 0x000b8000,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_G",
        word_len: 1,
        table: 'G',
        native_addr: 0x000b7c00,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_H",
        word_len: 1,
        table: 'H',
        native_addr: 0x000b7be0,
        byte_len: 16,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_I",
        word_len: 1,
        table: 'I',
        native_addr: 0x000b77e0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_J",
        word_len: 1,
        table: 'J',
        native_addr: 0x000b73e0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl1_K",
        word_len: 1,
        table: 'K',
        native_addr: 0x000b6fe0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_A",
        word_len: 2,
        table: 'A',
        native_addr: 0x000b6d20,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_B",
        word_len: 2,
        table: 'B',
        native_addr: 0x000b6a60,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_C",
        word_len: 2,
        table: 'C',
        native_addr: 0x000b6960,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_D",
        word_len: 2,
        table: 'D',
        native_addr: 0x000b66a0,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_E",
        word_len: 2,
        table: 'E',
        native_addr: 0x000b63e0,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_F",
        word_len: 2,
        table: 'F',
        native_addr: 0x000b62e0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_G",
        word_len: 2,
        table: 'G',
        native_addr: 0x000b61e0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_H",
        word_len: 2,
        table: 'H',
        native_addr: 0x000b5f20,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_I",
        word_len: 2,
        table: 'I',
        native_addr: 0x000b5c60,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_J",
        word_len: 2,
        table: 'J',
        native_addr: 0x000b59a0,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_K",
        word_len: 2,
        table: 'K',
        native_addr: 0x000b56e0,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_L",
        word_len: 2,
        table: 'L',
        native_addr: 0x000b55e0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_M",
        word_len: 2,
        table: 'M',
        native_addr: 0x000b54e0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl2_N",
        word_len: 2,
        table: 'N',
        native_addr: 0x000b5220,
        byte_len: 684,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_A",
        word_len: 3,
        table: 'A',
        native_addr: 0x000b5120,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_B",
        word_len: 3,
        table: 'B',
        native_addr: 0x000b5020,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_C",
        word_len: 3,
        table: 'C',
        native_addr: 0x000b4f20,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_D",
        word_len: 3,
        table: 'D',
        native_addr: 0x000b4b20,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_E",
        word_len: 3,
        table: 'E',
        native_addr: 0x000b4b00,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_F",
        word_len: 3,
        table: 'F',
        native_addr: 0x000b4ae0,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_G",
        word_len: 3,
        table: 'G',
        native_addr: 0x000b46e0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_H",
        word_len: 3,
        table: 'H',
        native_addr: 0x000b45e0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_I",
        word_len: 3,
        table: 'I',
        native_addr: 0x000b44e0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_J",
        word_len: 3,
        table: 'J',
        native_addr: 0x000b40e0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_K",
        word_len: 3,
        table: 'K',
        native_addr: 0x000b3fe0,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_L",
        word_len: 3,
        table: 'L',
        native_addr: 0x000b3fc0,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_M",
        word_len: 3,
        table: 'M',
        native_addr: 0x000b3fa0,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl3_N",
        word_len: 3,
        table: 'N',
        native_addr: 0x000b3ba0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_A",
        word_len: 4,
        table: 'A',
        native_addr: 0x000b3b80,
        byte_len: 24,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_B",
        word_len: 4,
        table: 'B',
        native_addr: 0x000b3780,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_C",
        word_len: 4,
        table: 'C',
        native_addr: 0x000b3758,
        byte_len: 24,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_D",
        word_len: 4,
        table: 'D',
        native_addr: 0x000b3740,
        byte_len: 24,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_E",
        word_len: 4,
        table: 'E',
        native_addr: 0x000b3340,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_F",
        word_len: 4,
        table: 'F',
        native_addr: 0x000b2f40,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_G",
        word_len: 4,
        table: 'G',
        native_addr: 0x000b2b40,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_H",
        word_len: 4,
        table: 'H',
        native_addr: 0x000b2740,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_I",
        word_len: 4,
        table: 'I',
        native_addr: 0x000b2340,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_J",
        word_len: 4,
        table: 'J',
        native_addr: 0x000b1f40,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_K",
        word_len: 4,
        table: 'K',
        native_addr: 0x000b1f20,
        byte_len: 24,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl4_L",
        word_len: 4,
        table: 'L',
        native_addr: 0x000b1ee0,
        byte_len: 64,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_A",
        word_len: 5,
        table: 'A',
        native_addr: 0x000b1ae0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_B",
        word_len: 5,
        table: 'B',
        native_addr: 0x000b16e0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_C",
        word_len: 5,
        table: 'C',
        native_addr: 0x000b16c0,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_D",
        word_len: 5,
        table: 'D',
        native_addr: 0x000b12c0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_E",
        word_len: 5,
        table: 'E',
        native_addr: 0x000b12a0,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_F",
        word_len: 5,
        table: 'F',
        native_addr: 0x000b1280,
        byte_len: 32,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_G",
        word_len: 5,
        table: 'G',
        native_addr: 0x000b1240,
        byte_len: 64,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_H",
        word_len: 5,
        table: 'H',
        native_addr: 0x000b1200,
        byte_len: 64,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_I",
        word_len: 5,
        table: 'I',
        native_addr: 0x000b0e00,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_J",
        word_len: 5,
        table: 'J',
        native_addr: 0x000b0a00,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_K",
        word_len: 5,
        table: 'K',
        native_addr: 0x000b0600,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl5_L",
        word_len: 5,
        table: 'L',
        native_addr: 0x000b0200,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_A",
        word_len: 6,
        table: 'A',
        native_addr: 0x000b01c0,
        byte_len: 64,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_B",
        word_len: 6,
        table: 'B',
        native_addr: 0x000afdc0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_C",
        word_len: 6,
        table: 'C',
        native_addr: 0x000af9c0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_D",
        word_len: 6,
        table: 'D',
        native_addr: 0x000af5c0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_E",
        word_len: 6,
        table: 'E',
        native_addr: 0x000af1c0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_F",
        word_len: 6,
        table: 'F',
        native_addr: 0x000af140,
        byte_len: 128,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_G",
        word_len: 6,
        table: 'G',
        native_addr: 0x000af0c0,
        byte_len: 128,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_H",
        word_len: 6,
        table: 'H',
        native_addr: 0x000aecc0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_I",
        word_len: 6,
        table: 'I',
        native_addr: 0x000ae8c0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_J",
        word_len: 6,
        table: 'J',
        native_addr: 0x000ae4c0,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_K",
        word_len: 6,
        table: 'K',
        native_addr: 0x000ae440,
        byte_len: 128,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl6_L",
        word_len: 6,
        table: 'L',
        native_addr: 0x000ae040,
        byte_len: 1024,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_A",
        word_len: 7,
        table: 'A',
        native_addr: 0x000adf40,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_B",
        word_len: 7,
        table: 'B',
        native_addr: 0x000ade40,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_C",
        word_len: 7,
        table: 'C',
        native_addr: 0x000add40,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_D",
        word_len: 7,
        table: 'D',
        native_addr: 0x000adc40,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_E",
        word_len: 7,
        table: 'E',
        native_addr: 0x000adb40,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_F",
        word_len: 7,
        table: 'F',
        native_addr: 0x000ada40,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_G",
        word_len: 7,
        table: 'G',
        native_addr: 0x000ad940,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_H",
        word_len: 7,
        table: 'H',
        native_addr: 0x000ad840,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_I",
        word_len: 7,
        table: 'I',
        native_addr: 0x000ad740,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_J",
        word_len: 7,
        table: 'J',
        native_addr: 0x000ad640,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_K",
        word_len: 7,
        table: 'K',
        native_addr: 0x000ad540,
        byte_len: 256,
    },
    SpectralTableInfo {
        symbol: "g_a_spec_wl7_L",
        word_len: 7,
        table: 'L',
        native_addr: 0x000ad440,
        byte_len: 256,
    },
];

// Native `g_aaa_hcspec`: 16 selectors by 7 word lengths, 24 bytes per slot.
pub const SPECTRAL_DESCRIPTOR_SLOTS: &[SpectralDescriptorSlot] = &[
    spec_slot!(
        'A',
        1,
        0,
        "g_a_spec_wl1_A",
        0x000b_9040,
        "g_a_dectbl_wl1_A",
        0x000a_c440
    ),
    spec_slot!(
        'A',
        2,
        1,
        "g_a_spec_wl2_A",
        0x000b_6d20,
        "g_a_dectbl_wl2_A",
        0x000a_4be0
    ),
    spec_slot!(
        'A',
        3,
        2,
        "g_a_spec_wl3_A",
        0x000b_5120,
        "g_a_dectbl_wl3_A",
        0x0009_fae0
    ),
    spec_slot!(
        'A',
        4,
        3,
        "g_a_spec_wl4_A",
        0x000b_3b80,
        "g_a_dectbl_wl4_A",
        0x0009_bc60
    ),
    spec_slot!(
        'A',
        5,
        4,
        "g_a_spec_wl5_A",
        0x000b_1ae0,
        "g_a_dectbl_wl5_A",
        0x0009_73c0
    ),
    spec_slot!(
        'A',
        6,
        5,
        "g_a_spec_wl6_A",
        0x000b_01c0,
        "g_a_dectbl_wl6_A",
        0x0009_2e40
    ),
    spec_slot!(
        'A',
        7,
        6,
        "g_a_spec_wl7_A",
        0x000a_df40,
        "g_a_dectbl_wl7_A",
        0x0008_b9c0
    ),
    spec_slot!(
        'B',
        1,
        7,
        "g_a_spec_wl1_B",
        0x000b_8c40,
        "g_a_dectbl_wl1_B",
        0x000a_bc40
    ),
    spec_slot!(
        'B',
        2,
        8,
        "g_a_spec_wl2_B",
        0x000b_6a60,
        "g_a_dectbl_wl2_B",
        0x000a_47e0
    ),
    spec_slot!(
        'B',
        3,
        9,
        "g_a_spec_wl3_B",
        0x000b_5020,
        "g_a_dectbl_wl3_B",
        0x0009_f6e0
    ),
    spec_slot!(
        'B',
        4,
        10,
        "g_a_spec_wl4_B",
        0x000b_3780,
        "g_a_dectbl_wl4_B",
        0x0009_b460
    ),
    spec_slot!(
        'B',
        5,
        11,
        "g_a_spec_wl5_B",
        0x000b_16e0,
        "g_a_dectbl_wl5_B",
        0x0009_6bc0
    ),
    spec_slot!(
        'B',
        6,
        12,
        "g_a_spec_wl6_B",
        0x000a_fdc0,
        "g_a_dectbl_wl6_B",
        0x0009_1e40
    ),
    spec_slot!(
        'B',
        7,
        13,
        "g_a_spec_wl7_B",
        0x000a_de40,
        "g_a_dectbl_wl7_B",
        0x0008_b7c0
    ),
    spec_slot!(
        'C',
        1,
        14,
        "g_a_spec_wl1_C",
        0x000b_8840,
        "g_a_dectbl_wl1_C",
        0x000a_ac40
    ),
    spec_slot!(
        'C',
        2,
        15,
        "g_a_spec_wl2_C",
        0x000b_6960,
        "g_a_dectbl_wl2_C",
        0x000a_45e0
    ),
    spec_slot!(
        'C',
        3,
        16,
        "g_a_spec_wl3_C",
        0x000b_4f20,
        "g_a_dectbl_wl3_C",
        0x0009_f4e0
    ),
    spec_slot!(
        'C',
        4,
        17,
        "g_a_spec_wl4_C",
        0x000b_3758,
        "g_a_dectbl_wl4_C",
        0x0009_b440
    ),
    spec_slot!(
        'C',
        5,
        18,
        "g_a_spec_wl5_C",
        0x000b_16c0,
        "g_a_dectbl_wl5_C",
        0x0009_6ba0
    ),
    spec_slot!(
        'C',
        6,
        19,
        "g_a_spec_wl6_C",
        0x000a_f9c0,
        "g_a_dectbl_wl6_C",
        0x0009_1640
    ),
    spec_slot!(
        'C',
        7,
        20,
        "g_a_spec_wl7_C",
        0x000a_dd40,
        "g_a_dectbl_wl7_C",
        0x0008_b6c0
    ),
    spec_slot!(
        'D',
        1,
        21,
        "g_a_spec_wl1_D",
        0x000b_8800,
        "g_a_dectbl_wl1_D",
        0x000a_ac00
    ),
    spec_slot!(
        'D',
        2,
        22,
        "g_a_spec_wl2_D",
        0x000b_66a0,
        "g_a_dectbl_wl2_D",
        0x000a_3de0
    ),
    spec_slot!(
        'D',
        3,
        23,
        "g_a_spec_wl3_D",
        0x000b_4b20,
        "g_a_dectbl_wl3_D",
        0x0009_e4e0
    ),
    spec_slot!(
        'D',
        4,
        24,
        "g_a_spec_wl4_D",
        0x000b_3740,
        "g_a_dectbl_wl4_D",
        0x0009_b420
    ),
    spec_slot!(
        'D',
        5,
        25,
        "g_a_spec_wl5_D",
        0x000b_12c0,
        "g_a_dectbl_wl5_D",
        0x0009_67a0
    ),
    spec_slot!(
        'D',
        6,
        26,
        "g_a_spec_wl6_D",
        0x000a_f5c0,
        "g_a_dectbl_wl6_D",
        0x0009_0640
    ),
    spec_slot!(
        'D',
        7,
        27,
        "g_a_spec_wl7_D",
        0x000a_dc40,
        "g_a_dectbl_wl7_D",
        0x0008_b5c0
    ),
    spec_slot!(
        'E',
        1,
        28,
        "g_a_spec_wl1_E",
        0x000b_8400,
        "g_a_dectbl_wl1_E",
        0x000a_9c00
    ),
    spec_slot!(
        'E',
        2,
        29,
        "g_a_spec_wl2_E",
        0x000b_63e0,
        "g_a_dectbl_wl2_E",
        0x000a_39e0
    ),
    spec_slot!(
        'E',
        3,
        30,
        "g_a_spec_wl3_E",
        0x000b_4b00,
        "g_a_dectbl_wl3_E",
        0x0009_e4d0
    ),
    spec_slot!(
        'E',
        4,
        31,
        "g_a_spec_wl4_E",
        0x000b_3340,
        "g_a_dectbl_wl4_E",
        0x0009_b020
    ),
    spec_slot!(
        'E',
        5,
        32,
        "g_a_spec_wl5_E",
        0x000b_12a0,
        "g_a_dectbl_wl5_E",
        0x0009_6780
    ),
    spec_slot!(
        'E',
        6,
        33,
        "g_a_spec_wl6_E",
        0x000a_f1c0,
        "g_a_dectbl_wl6_E",
        0x0009_0440
    ),
    spec_slot!(
        'E',
        7,
        34,
        "g_a_spec_wl7_E",
        0x000a_db40,
        "g_a_dectbl_wl7_E",
        0x0008_b3c0
    ),
    spec_slot!(
        'F',
        1,
        35,
        "g_a_spec_wl1_F",
        0x000b_8000,
        "g_a_dectbl_wl1_F",
        0x000a_9400
    ),
    spec_slot!(
        'F',
        2,
        36,
        "g_a_spec_wl2_F",
        0x000b_62e0,
        "g_a_dectbl_wl2_F",
        0x000a_35e0
    ),
    spec_slot!(
        'F',
        3,
        37,
        "g_a_spec_wl3_F",
        0x000b_4ae0,
        "g_a_dectbl_wl3_F",
        0x0009_e4c0
    ),
    spec_slot!(
        'F',
        4,
        38,
        "g_a_spec_wl4_F",
        0x000b_2f40,
        "g_a_dectbl_wl4_F",
        0x0009_ae20
    ),
    spec_slot!(
        'F',
        5,
        39,
        "g_a_spec_wl5_F",
        0x000b_1280,
        "g_a_dectbl_wl5_F",
        0x0009_6740
    ),
    spec_slot!(
        'F',
        6,
        40,
        "g_a_spec_wl6_F",
        0x000a_f140,
        "g_a_dectbl_wl6_F",
        0x0009_03c0
    ),
    spec_slot!(
        'F',
        7,
        41,
        "g_a_spec_wl7_F",
        0x000a_da40,
        "g_a_dectbl_wl7_F",
        0x0008_b1c0
    ),
    spec_slot!(
        'G',
        1,
        42,
        "g_a_spec_wl1_G",
        0x000b_7c00,
        "g_a_dectbl_wl1_G",
        0x000a_8c00
    ),
    spec_slot!(
        'G',
        2,
        43,
        "g_a_spec_wl2_G",
        0x000b_61e0,
        "g_a_dectbl_wl2_G",
        0x000a_34e0
    ),
    spec_slot!(
        'G',
        3,
        44,
        "g_a_spec_wl3_G",
        0x000b_46e0,
        "g_a_dectbl_wl3_G",
        0x0009_dcc0
    ),
    spec_slot!(
        'G',
        4,
        45,
        "g_a_spec_wl4_G",
        0x000b_2b40,
        "g_a_dectbl_wl4_G",
        0x0009_ac20
    ),
    spec_slot!(
        'G',
        5,
        46,
        "g_a_spec_wl5_G",
        0x000b_1240,
        "g_a_dectbl_wl5_G",
        0x0009_66c0
    ),
    spec_slot!(
        'G',
        6,
        47,
        "g_a_spec_wl6_G",
        0x000a_f0c0,
        "g_a_dectbl_wl6_G",
        0x0008_ffc0
    ),
    spec_slot!(
        'G',
        7,
        48,
        "g_a_spec_wl7_G",
        0x000a_d940,
        "g_a_dectbl_wl7_G",
        0x0008_afc0
    ),
    spec_slot!(
        'H',
        1,
        49,
        "g_a_spec_wl1_H",
        0x000b_7be0,
        "g_a_dectbl_wl1_H",
        0x000a_8be0
    ),
    spec_slot!(
        'H',
        2,
        50,
        "g_a_spec_wl2_H",
        0x000b_5f20,
        "g_a_dectbl_wl2_H",
        0x000a_2ce0
    ),
    spec_slot!(
        'H',
        3,
        51,
        "g_a_spec_wl3_H",
        0x000b_45e0,
        "g_a_dectbl_wl3_H",
        0x0009_d8c0
    ),
    spec_slot!(
        'H',
        4,
        52,
        "g_a_spec_wl4_H",
        0x000b_2740,
        "g_a_dectbl_wl4_H",
        0x0009_9c20
    ),
    spec_slot!(
        'H',
        5,
        53,
        "g_a_spec_wl5_H",
        0x000b_1200,
        "g_a_dectbl_wl5_H",
        0x0009_6680
    ),
    spec_slot!(
        'H',
        6,
        54,
        "g_a_spec_wl6_H",
        0x000a_ecc0,
        "g_a_dectbl_wl6_H",
        0x0008_efc0
    ),
    spec_slot!(
        'H',
        7,
        55,
        "g_a_spec_wl7_A",
        0x000a_df40,
        "g_a_dectbl_wl7_A",
        0x0008_b9c0
    ),
    spec_slot!(
        'I',
        1,
        56,
        "g_a_spec_wl1_I",
        0x000b_77e0,
        "g_a_dectbl_wl1_I",
        0x000a_7be0
    ),
    spec_slot!(
        'I',
        2,
        57,
        "g_a_spec_wl2_I",
        0x000b_5c60,
        "g_a_dectbl_wl2_I",
        0x000a_1ce0
    ),
    spec_slot!(
        'I',
        3,
        58,
        "g_a_spec_wl3_I",
        0x000b_44e0,
        "g_a_dectbl_wl3_I",
        0x0009_d6c0
    ),
    spec_slot!(
        'I',
        4,
        59,
        "g_a_spec_wl4_I",
        0x000b_2340,
        "g_a_dectbl_wl4_I",
        0x0009_9420
    ),
    spec_slot!(
        'I',
        5,
        60,
        "g_a_spec_wl5_I",
        0x000b_0e00,
        "g_a_dectbl_wl5_I",
        0x0009_5e80
    ),
    spec_slot!(
        'I',
        6,
        61,
        "g_a_spec_wl6_A",
        0x000b_01c0,
        "g_a_dectbl_wl6_A",
        0x0009_2e40
    ),
    spec_slot!(
        'I',
        7,
        62,
        "g_a_spec_wl7_H",
        0x000a_d840,
        "g_a_dectbl_wl7_H",
        0x0008_adc0
    ),
    spec_slot!(
        'J',
        1,
        63,
        "g_a_spec_wl1_C",
        0x000b_8840,
        "g_a_dectbl_wl1_C",
        0x000a_ac40
    ),
    spec_slot!(
        'J',
        2,
        64,
        "g_a_spec_wl2_J",
        0x000b_59a0,
        "g_a_dectbl_wl2_J",
        0x000a_18e0
    ),
    spec_slot!(
        'J',
        3,
        65,
        "g_a_spec_wl3_B",
        0x000b_5020,
        "g_a_dectbl_wl3_B",
        0x0009_f6e0
    ),
    spec_slot!(
        'J',
        4,
        66,
        "g_a_spec_wl4_J",
        0x000b_1f40,
        "g_a_dectbl_wl4_J",
        0x0009_8420
    ),
    spec_slot!(
        'J',
        5,
        67,
        "g_a_spec_wl5_B",
        0x000b_16e0,
        "g_a_dectbl_wl5_B",
        0x0009_6bc0
    ),
    spec_slot!(
        'J',
        6,
        68,
        "g_a_spec_wl6_I",
        0x000a_e8c0,
        "g_a_dectbl_wl6_I",
        0x0008_dfc0
    ),
    spec_slot!(
        'J',
        7,
        69,
        "g_a_spec_wl7_A",
        0x000a_df40,
        "g_a_dectbl_wl7_A",
        0x0008_b9c0
    ),
    spec_slot!(
        'K',
        1,
        70,
        "g_a_spec_wl1_E",
        0x000b_8400,
        "g_a_dectbl_wl1_E",
        0x000a_9c00
    ),
    spec_slot!(
        'K',
        2,
        71,
        "g_a_spec_wl2_D",
        0x000b_66a0,
        "g_a_dectbl_wl2_D",
        0x000a_3de0
    ),
    spec_slot!(
        'K',
        3,
        72,
        "g_a_spec_wl3_A",
        0x000b_5120,
        "g_a_dectbl_wl3_A",
        0x0009_fae0
    ),
    spec_slot!(
        'K',
        4,
        73,
        "g_a_spec_wl4_E",
        0x000b_3340,
        "g_a_dectbl_wl4_E",
        0x0009_b020
    ),
    spec_slot!(
        'K',
        5,
        74,
        "g_a_spec_wl5_I",
        0x000b_0e00,
        "g_a_dectbl_wl5_I",
        0x0009_5e80
    ),
    spec_slot!(
        'K',
        6,
        75,
        "g_a_spec_wl6_J",
        0x000a_e4c0,
        "g_a_dectbl_wl6_J",
        0x0008_cfc0
    ),
    spec_slot!(
        'K',
        7,
        76,
        "g_a_spec_wl7_A",
        0x000a_df40,
        "g_a_dectbl_wl7_A",
        0x0008_b9c0
    ),
    spec_slot!(
        'L',
        1,
        77,
        "g_a_spec_wl1_F",
        0x000b_8000,
        "g_a_dectbl_wl1_F",
        0x000a_9400
    ),
    spec_slot!(
        'L',
        2,
        78,
        "g_a_spec_wl2_K",
        0x000b_56e0,
        "g_a_dectbl_wl2_K",
        0x000a_10e0
    ),
    spec_slot!(
        'L',
        3,
        79,
        "g_a_spec_wl3_J",
        0x000b_40e0,
        "g_a_dectbl_wl3_J",
        0x0009_c6c0
    ),
    spec_slot!(
        'L',
        4,
        80,
        "g_a_spec_wl4_I",
        0x000b_2340,
        "g_a_dectbl_wl4_I",
        0x0009_9420
    ),
    spec_slot!(
        'L',
        5,
        81,
        "g_a_spec_wl5_J",
        0x000b_0a00,
        "g_a_dectbl_wl5_J",
        0x0009_4e80
    ),
    spec_slot!(
        'L',
        6,
        82,
        "g_a_spec_wl6_J",
        0x000a_e4c0,
        "g_a_dectbl_wl6_J",
        0x0008_cfc0
    ),
    spec_slot!(
        'L',
        7,
        83,
        "g_a_spec_wl7_I",
        0x000a_d740,
        "g_a_dectbl_wl7_I",
        0x0008_a5c0
    ),
    spec_slot!(
        'M',
        1,
        84,
        "g_a_spec_wl1_J",
        0x000b_73e0,
        "g_a_dectbl_wl1_J",
        0x000a_6be0
    ),
    spec_slot!(
        'M',
        2,
        85,
        "g_a_spec_wl2_L",
        0x000b_55e0,
        "g_a_dectbl_wl2_L",
        0x000a_0ee0
    ),
    spec_slot!(
        'M',
        3,
        86,
        "g_a_spec_wl3_K",
        0x000b_3fe0,
        "g_a_dectbl_wl3_K",
        0x0009_c4c0
    ),
    spec_slot!(
        'M',
        4,
        87,
        "g_a_spec_wl4_J",
        0x000b_1f40,
        "g_a_dectbl_wl4_J",
        0x0009_8420
    ),
    spec_slot!(
        'M',
        5,
        88,
        "g_a_spec_wl5_E",
        0x000b_12a0,
        "g_a_dectbl_wl5_E",
        0x0009_6780
    ),
    spec_slot!(
        'M',
        6,
        89,
        "g_a_spec_wl6_B",
        0x000a_fdc0,
        "g_a_dectbl_wl6_B",
        0x0009_1e40
    ),
    spec_slot!(
        'M',
        7,
        90,
        "g_a_spec_wl7_J",
        0x000a_d640,
        "g_a_dectbl_wl7_J",
        0x0008_a3c0
    ),
    spec_slot!(
        'N',
        1,
        91,
        "g_a_spec_wl1_G",
        0x000b_7c00,
        "g_a_dectbl_wl1_G",
        0x000a_8c00
    ),
    spec_slot!(
        'N',
        2,
        92,
        "g_a_spec_wl2_M",
        0x000b_54e0,
        "g_a_dectbl_wl2_M",
        0x000a_0ce0
    ),
    spec_slot!(
        'N',
        3,
        93,
        "g_a_spec_wl3_L",
        0x000b_3fc0,
        "g_a_dectbl_wl3_L",
        0x0009_c4a0
    ),
    spec_slot!(
        'N',
        4,
        94,
        "g_a_spec_wl4_C",
        0x000b_3758,
        "g_a_dectbl_wl4_C",
        0x0009_b440
    ),
    spec_slot!(
        'N',
        5,
        95,
        "g_a_spec_wl5_F",
        0x000b_1280,
        "g_a_dectbl_wl5_F",
        0x0009_6740
    ),
    spec_slot!(
        'N',
        6,
        96,
        "g_a_spec_wl6_K",
        0x000a_e440,
        "g_a_dectbl_wl6_K",
        0x0008_cdc0
    ),
    spec_slot!(
        'N',
        7,
        97,
        "g_a_spec_wl7_H",
        0x000a_d840,
        "g_a_dectbl_wl7_H",
        0x0008_adc0
    ),
    spec_slot!(
        'O',
        1,
        98,
        "g_a_spec_wl1_E",
        0x000b_8400,
        "g_a_dectbl_wl1_E",
        0x000a_9c00
    ),
    spec_slot!(
        'O',
        2,
        99,
        "g_a_spec_wl2_N",
        0x000b_5220,
        "g_a_dectbl_wl2_N",
        0x0009_fce0
    ),
    spec_slot!(
        'O',
        3,
        100,
        "g_a_spec_wl3_M",
        0x000b_3fa0,
        "g_a_dectbl_wl3_M",
        0x0009_c480
    ),
    spec_slot!(
        'O',
        4,
        101,
        "g_a_spec_wl4_K",
        0x000b_1f20,
        "g_a_dectbl_wl4_K",
        0x0009_8400
    ),
    spec_slot!(
        'O',
        5,
        102,
        "g_a_spec_wl5_K",
        0x000b_0600,
        "g_a_dectbl_wl5_K",
        0x0009_3e80
    ),
    spec_slot!(
        'O',
        6,
        103,
        "g_a_spec_wl6_L",
        0x000a_e040,
        "g_a_dectbl_wl6_L",
        0x0008_bdc0
    ),
    spec_slot!(
        'O',
        7,
        104,
        "g_a_spec_wl7_K",
        0x000a_d540,
        "g_a_dectbl_wl7_K",
        0x0008_a1c0
    ),
    spec_slot!(
        'P',
        1,
        105,
        "g_a_spec_wl1_K",
        0x000b_6fe0,
        "g_a_dectbl_wl1_K",
        0x000a_5be0
    ),
    spec_slot!(
        'P',
        2,
        106,
        "g_a_spec_wl2_K",
        0x000b_56e0,
        "g_a_dectbl_wl2_K",
        0x000a_10e0
    ),
    spec_slot!(
        'P',
        3,
        107,
        "g_a_spec_wl3_N",
        0x000b_3ba0,
        "g_a_dectbl_wl3_N",
        0x0009_bc80
    ),
    spec_slot!(
        'P',
        4,
        108,
        "g_a_spec_wl4_L",
        0x000b_1ee0,
        "g_a_dectbl_wl4_L",
        0x0009_83c0
    ),
    spec_slot!(
        'P',
        5,
        109,
        "g_a_spec_wl5_L",
        0x000b_0200,
        "g_a_dectbl_wl5_L",
        0x0009_2e80
    ),
    spec_slot!(
        'P',
        6,
        110,
        "g_a_spec_wl6_G",
        0x000a_f0c0,
        "g_a_dectbl_wl6_G",
        0x0008_ffc0
    ),
    spec_slot!(
        'P',
        7,
        111,
        "g_a_spec_wl7_L",
        0x000a_d440,
        "g_a_dectbl_wl7_L",
        0x0008_99c0
    ),
];

pub fn spectral_table(word_len: u8, table: char) -> Option<&'static SpectralTableInfo> {
    SPECTRAL_TABLES
        .iter()
        .find(|entry| entry.word_len == word_len && entry.table == table)
}

pub fn spectral_tables_for_word_len(
    word_len: u8,
) -> impl Iterator<Item = &'static SpectralTableInfo> {
    SPECTRAL_TABLES
        .iter()
        .filter(move |entry| entry.word_len == word_len)
}

pub fn spectral_generated_pack_table(symbol: &str) -> Option<SpectralPackTable> {
    match symbol {
        "g_a_spec_wl1_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_A",
            native_addr: 0x000b9040,
            bytes: &G_A_SPEC_WL1_A,
        }),
        "g_a_spec_wl1_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_B",
            native_addr: 0x000b8c40,
            bytes: &G_A_SPEC_WL1_B,
        }),
        "g_a_spec_wl1_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_C",
            native_addr: 0x000b8840,
            bytes: &G_A_SPEC_WL1_C,
        }),
        "g_a_spec_wl1_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_D",
            native_addr: 0x000b8800,
            bytes: &G_A_SPEC_WL1_D,
        }),
        "g_a_spec_wl1_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_E",
            native_addr: 0x000b8400,
            bytes: &G_A_SPEC_WL1_E,
        }),
        "g_a_spec_wl1_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_F",
            native_addr: 0x000b8000,
            bytes: &G_A_SPEC_WL1_F,
        }),
        "g_a_spec_wl1_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_G",
            native_addr: 0x000b7c00,
            bytes: &G_A_SPEC_WL1_G,
        }),
        "g_a_spec_wl1_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_H",
            native_addr: 0x000b7be0,
            bytes: &G_A_SPEC_WL1_H,
        }),
        "g_a_spec_wl1_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_I",
            native_addr: 0x000b77e0,
            bytes: &G_A_SPEC_WL1_I,
        }),
        "g_a_spec_wl1_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_J",
            native_addr: 0x000b73e0,
            bytes: &G_A_SPEC_WL1_J,
        }),
        "g_a_spec_wl1_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl1_K",
            native_addr: 0x000b6fe0,
            bytes: &G_A_SPEC_WL1_K,
        }),
        "g_a_spec_wl2_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_A",
            native_addr: 0x000b6d20,
            bytes: &G_A_SPEC_WL2_A,
        }),
        "g_a_spec_wl2_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_B",
            native_addr: 0x000b6a60,
            bytes: &G_A_SPEC_WL2_B,
        }),
        "g_a_spec_wl2_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_C",
            native_addr: 0x000b6960,
            bytes: &G_A_SPEC_WL2_C,
        }),
        "g_a_spec_wl2_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_D",
            native_addr: 0x000b66a0,
            bytes: &G_A_SPEC_WL2_D,
        }),
        "g_a_spec_wl2_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_E",
            native_addr: 0x000b63e0,
            bytes: &G_A_SPEC_WL2_E,
        }),
        "g_a_spec_wl2_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_F",
            native_addr: 0x000b62e0,
            bytes: &G_A_SPEC_WL2_F,
        }),
        "g_a_spec_wl2_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_G",
            native_addr: 0x000b61e0,
            bytes: &G_A_SPEC_WL2_G,
        }),
        "g_a_spec_wl2_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_H",
            native_addr: 0x000b5f20,
            bytes: &G_A_SPEC_WL2_H,
        }),
        "g_a_spec_wl2_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_I",
            native_addr: 0x000b5c60,
            bytes: &G_A_SPEC_WL2_I,
        }),
        "g_a_spec_wl2_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_J",
            native_addr: 0x000b59a0,
            bytes: &G_A_SPEC_WL2_J,
        }),
        "g_a_spec_wl2_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_K",
            native_addr: 0x000b56e0,
            bytes: &G_A_SPEC_WL2_K,
        }),
        "g_a_spec_wl2_L" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_L",
            native_addr: 0x000b55e0,
            bytes: &G_A_SPEC_WL2_L,
        }),
        "g_a_spec_wl2_M" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_M",
            native_addr: 0x000b54e0,
            bytes: &G_A_SPEC_WL2_M,
        }),
        "g_a_spec_wl2_N" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl2_N",
            native_addr: 0x000b5220,
            bytes: &G_A_SPEC_WL2_N,
        }),
        "g_a_spec_wl3_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_A",
            native_addr: 0x000b5120,
            bytes: &G_A_SPEC_WL3_A,
        }),
        "g_a_spec_wl3_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_B",
            native_addr: 0x000b5020,
            bytes: &G_A_SPEC_WL3_B,
        }),
        "g_a_spec_wl3_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_C",
            native_addr: 0x000b4f20,
            bytes: &G_A_SPEC_WL3_C,
        }),
        "g_a_spec_wl3_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_D",
            native_addr: 0x000b4b20,
            bytes: &G_A_SPEC_WL3_D,
        }),
        "g_a_spec_wl3_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_E",
            native_addr: 0x000b4b00,
            bytes: &G_A_SPEC_WL3_E,
        }),
        "g_a_spec_wl3_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_F",
            native_addr: 0x000b4ae0,
            bytes: &G_A_SPEC_WL3_F,
        }),
        "g_a_spec_wl3_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_G",
            native_addr: 0x000b46e0,
            bytes: &G_A_SPEC_WL3_G,
        }),
        "g_a_spec_wl3_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_H",
            native_addr: 0x000b45e0,
            bytes: &G_A_SPEC_WL3_H,
        }),
        "g_a_spec_wl3_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_I",
            native_addr: 0x000b44e0,
            bytes: &G_A_SPEC_WL3_I,
        }),
        "g_a_spec_wl3_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_J",
            native_addr: 0x000b40e0,
            bytes: &G_A_SPEC_WL3_J,
        }),
        "g_a_spec_wl3_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_K",
            native_addr: 0x000b3fe0,
            bytes: &G_A_SPEC_WL3_K,
        }),
        "g_a_spec_wl3_L" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_L",
            native_addr: 0x000b3fc0,
            bytes: &G_A_SPEC_WL3_L,
        }),
        "g_a_spec_wl3_M" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_M",
            native_addr: 0x000b3fa0,
            bytes: &G_A_SPEC_WL3_M,
        }),
        "g_a_spec_wl3_N" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl3_N",
            native_addr: 0x000b3ba0,
            bytes: &G_A_SPEC_WL3_N,
        }),
        "g_a_spec_wl4_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_A",
            native_addr: 0x000b3b80,
            bytes: &G_A_SPEC_WL4_A,
        }),
        "g_a_spec_wl4_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_B",
            native_addr: 0x000b3780,
            bytes: &G_A_SPEC_WL4_B,
        }),
        "g_a_spec_wl4_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_C",
            native_addr: 0x000b3758,
            bytes: &G_A_SPEC_WL4_C,
        }),
        "g_a_spec_wl4_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_D",
            native_addr: 0x000b3740,
            bytes: &G_A_SPEC_WL4_D,
        }),
        "g_a_spec_wl4_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_E",
            native_addr: 0x000b3340,
            bytes: &G_A_SPEC_WL4_E,
        }),
        "g_a_spec_wl4_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_F",
            native_addr: 0x000b2f40,
            bytes: &G_A_SPEC_WL4_F,
        }),
        "g_a_spec_wl4_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_G",
            native_addr: 0x000b2b40,
            bytes: &G_A_SPEC_WL4_G,
        }),
        "g_a_spec_wl4_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_H",
            native_addr: 0x000b2740,
            bytes: &G_A_SPEC_WL4_H,
        }),
        "g_a_spec_wl4_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_I",
            native_addr: 0x000b2340,
            bytes: &G_A_SPEC_WL4_I,
        }),
        "g_a_spec_wl4_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_J",
            native_addr: 0x000b1f40,
            bytes: &G_A_SPEC_WL4_J,
        }),
        "g_a_spec_wl4_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_K",
            native_addr: 0x000b1f20,
            bytes: &G_A_SPEC_WL4_K,
        }),
        "g_a_spec_wl4_L" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl4_L",
            native_addr: 0x000b1ee0,
            bytes: &G_A_SPEC_WL4_L,
        }),
        "g_a_spec_wl5_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_A",
            native_addr: 0x000b1ae0,
            bytes: &G_A_SPEC_WL5_A,
        }),
        "g_a_spec_wl5_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_B",
            native_addr: 0x000b16e0,
            bytes: &G_A_SPEC_WL5_B,
        }),
        "g_a_spec_wl5_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_C",
            native_addr: 0x000b16c0,
            bytes: &G_A_SPEC_WL5_C,
        }),
        "g_a_spec_wl5_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_D",
            native_addr: 0x000b12c0,
            bytes: &G_A_SPEC_WL5_D,
        }),
        "g_a_spec_wl5_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_E",
            native_addr: 0x000b12a0,
            bytes: &G_A_SPEC_WL5_E,
        }),
        "g_a_spec_wl5_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_F",
            native_addr: 0x000b1280,
            bytes: &G_A_SPEC_WL5_F,
        }),
        "g_a_spec_wl5_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_G",
            native_addr: 0x000b1240,
            bytes: &G_A_SPEC_WL5_G,
        }),
        "g_a_spec_wl5_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_H",
            native_addr: 0x000b1200,
            bytes: &G_A_SPEC_WL5_H,
        }),
        "g_a_spec_wl5_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_I",
            native_addr: 0x000b0e00,
            bytes: &G_A_SPEC_WL5_I,
        }),
        "g_a_spec_wl5_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_J",
            native_addr: 0x000b0a00,
            bytes: &G_A_SPEC_WL5_J,
        }),
        "g_a_spec_wl5_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_K",
            native_addr: 0x000b0600,
            bytes: &G_A_SPEC_WL5_K,
        }),
        "g_a_spec_wl5_L" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl5_L",
            native_addr: 0x000b0200,
            bytes: &G_A_SPEC_WL5_L,
        }),
        "g_a_spec_wl6_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_A",
            native_addr: 0x000b01c0,
            bytes: &G_A_SPEC_WL6_A,
        }),
        "g_a_spec_wl6_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_B",
            native_addr: 0x000afdc0,
            bytes: &G_A_SPEC_WL6_B,
        }),
        "g_a_spec_wl6_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_C",
            native_addr: 0x000af9c0,
            bytes: &G_A_SPEC_WL6_C,
        }),
        "g_a_spec_wl6_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_D",
            native_addr: 0x000af5c0,
            bytes: &G_A_SPEC_WL6_D,
        }),
        "g_a_spec_wl6_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_E",
            native_addr: 0x000af1c0,
            bytes: &G_A_SPEC_WL6_E,
        }),
        "g_a_spec_wl6_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_F",
            native_addr: 0x000af140,
            bytes: &G_A_SPEC_WL6_F,
        }),
        "g_a_spec_wl6_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_G",
            native_addr: 0x000af0c0,
            bytes: &G_A_SPEC_WL6_G,
        }),
        "g_a_spec_wl6_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_H",
            native_addr: 0x000aecc0,
            bytes: &G_A_SPEC_WL6_H,
        }),
        "g_a_spec_wl6_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_I",
            native_addr: 0x000ae8c0,
            bytes: &G_A_SPEC_WL6_I,
        }),
        "g_a_spec_wl6_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_J",
            native_addr: 0x000ae4c0,
            bytes: &G_A_SPEC_WL6_J,
        }),
        "g_a_spec_wl6_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_K",
            native_addr: 0x000ae440,
            bytes: &G_A_SPEC_WL6_K,
        }),
        "g_a_spec_wl6_L" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl6_L",
            native_addr: 0x000ae040,
            bytes: &G_A_SPEC_WL6_L,
        }),
        "g_a_spec_wl7_A" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_A",
            native_addr: 0x000adf40,
            bytes: &G_A_SPEC_WL7_A,
        }),
        "g_a_spec_wl7_B" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_B",
            native_addr: 0x000ade40,
            bytes: &G_A_SPEC_WL7_B,
        }),
        "g_a_spec_wl7_C" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_C",
            native_addr: 0x000add40,
            bytes: &G_A_SPEC_WL7_C,
        }),
        "g_a_spec_wl7_D" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_D",
            native_addr: 0x000adc40,
            bytes: &G_A_SPEC_WL7_D,
        }),
        "g_a_spec_wl7_E" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_E",
            native_addr: 0x000adb40,
            bytes: &G_A_SPEC_WL7_E,
        }),
        "g_a_spec_wl7_F" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_F",
            native_addr: 0x000ada40,
            bytes: &G_A_SPEC_WL7_F,
        }),
        "g_a_spec_wl7_G" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_G",
            native_addr: 0x000ad940,
            bytes: &G_A_SPEC_WL7_G,
        }),
        "g_a_spec_wl7_H" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_H",
            native_addr: 0x000ad840,
            bytes: &G_A_SPEC_WL7_H,
        }),
        "g_a_spec_wl7_I" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_I",
            native_addr: 0x000ad740,
            bytes: &G_A_SPEC_WL7_I,
        }),
        "g_a_spec_wl7_J" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_J",
            native_addr: 0x000ad640,
            bytes: &G_A_SPEC_WL7_J,
        }),
        "g_a_spec_wl7_K" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_K",
            native_addr: 0x000ad540,
            bytes: &G_A_SPEC_WL7_K,
        }),
        "g_a_spec_wl7_L" => Some(SpectralPackTable {
            symbol: "g_a_spec_wl7_L",
            native_addr: 0x000ad440,
            bytes: &G_A_SPEC_WL7_L,
        }),
        _ => None,
    }
}

pub fn generated_spectral_pack_table(word_len: u8, table: char) -> Option<SpectralPackTable> {
    spectral_table(word_len, table)?.generated_pack_table()
}

pub fn spectral_descriptor_slot_bytes(slot_index: usize) -> Option<&'static [u8]> {
    let start = slot_index.checked_mul(SPECTRAL_DESCRIPTOR_ENTRY_BYTES)?;
    let end = start.checked_add(SPECTRAL_DESCRIPTOR_ENTRY_BYTES)?;
    G_AAA_HCSPEC.get(start..end)
}

pub fn spectral_descriptor_metadata(slot_index: usize) -> Option<SpectralDescriptorMetadata> {
    let row = spectral_descriptor_slot_bytes(slot_index)?;
    Some(SpectralDescriptorMetadata {
        group_size: row[SPECTRAL_DESCRIPTOR_GROUP_SIZE_OFFSET],
        run_length: row[SPECTRAL_DESCRIPTOR_RUN_LENGTH_OFFSET],
        grouped_symbol_shift: row[SPECTRAL_DESCRIPTOR_GROUP_SHIFT_OFFSET] & 0x1f,
        signed: row[SPECTRAL_DESCRIPTOR_SIGNED_OFFSET] != 0,
        bit_width: row[SPECTRAL_DESCRIPTOR_BIT_WIDTH_OFFSET],
        magnitude_mask: u16::from(row[SPECTRAL_DESCRIPTOR_MAGNITUDE_MASK_OFFSET]),
    })
}

pub fn spectral_descriptor_slot(
    word_len: u8,
    selector: char,
) -> Option<&'static SpectralDescriptorSlot> {
    let selector_index = (selector as u32).checked_sub('A' as u32)? as usize;
    let word_len_index = usize::from(word_len.checked_sub(1)?);
    if selector_index >= SPECTRAL_DESCRIPTOR_SELECTOR_COUNT
        || word_len_index >= SPECTRAL_DESCRIPTOR_WORD_LEN_COUNT
    {
        return None;
    }
    let index = selector_index * SPECTRAL_DESCRIPTOR_WORD_LEN_COUNT + word_len_index;
    let entry = SPECTRAL_DESCRIPTOR_SLOTS.get(index)?;
    (entry.selector == selector && entry.word_len == word_len).then_some(entry)
}

pub fn spectral_descriptor_slots_for_selector(
    selector: char,
) -> impl Iterator<Item = &'static SpectralDescriptorSlot> {
    SPECTRAL_DESCRIPTOR_SLOTS
        .iter()
        .filter(move |entry| entry.selector == selector)
}
