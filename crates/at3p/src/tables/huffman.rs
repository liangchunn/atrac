use super::generated::{
    G_A_CT_PACK_A, G_A_CT_PACK_B, G_A_CT_PACK_C, G_A_CT_PACK_D, G_A_FREQ_PACK_A, G_A_HC_SF,
    G_A_HC_SF_SG, G_A_HC_WL, G_A_IDAM_PACK_AA, G_A_IDAM_PACK_AB, G_A_IDAM_PACK_C, G_A_IDLEV_PACK_A,
    G_A_IDLEV_PACK_B, G_A_IDLEV_PACK_C, G_A_IDLEV_PACK_D, G_A_IDLOC_PACK_A_ATK,
    G_A_IDLOC_PACK_A_REL, G_A_IDLOC_PACK_B_ATK, G_A_IDLOC_PACK_B_REL, G_A_IDLOC_PACK_C_ATK,
    G_A_IDSF_PACK_AA, G_A_IDSF_PACK_AB, G_A_IDSF_PACK_B, G_A_NGC_PACK_A, G_A_NGC_PACK_B,
    G_A_NWAVS_PACK_A, G_A_NWAVS_PACK_B, G_AA_SFC_PACK, G_AA_SFC_SG_PACK, G_AA_WLC_PACK, G_HC_CT_A,
    G_HC_CT_B, G_HC_CT_C, G_HC_CT_D, G_HC_GC_IDLEV_A, G_HC_GC_IDLEV_B, G_HC_GC_IDLEV_C,
    G_HC_GC_IDLEV_D, G_HC_GC_IDLOC_A_ATK, G_HC_GC_IDLOC_A_REL, G_HC_GC_IDLOC_B_ATK,
    G_HC_GC_IDLOC_B_REL, G_HC_GC_IDLOC_C_ATK, G_HC_GC_NGC_A, G_HC_GC_NGC_B, G_HC_GHPC_FREQ_A,
    G_HC_GHPC_IDAM_AA, G_HC_GHPC_IDAM_AB, G_HC_GHPC_IDAM_C, G_HC_GHPC_IDSF_AA, G_HC_GHPC_IDSF_AB,
    G_HC_GHPC_IDSF_B, G_HC_GHPC_NWAVS_A, G_HC_GHPC_NWAVS_B,
};
use super::view::read_u32_le;

pub const HUFFMAN_DESCRIPTOR_BYTES: usize = 24;
pub const HUFFMAN_CODE_ENTRY_BYTES: usize = 4;

const GC_NGC_A_ADDR: u32 = 0x000b_cf60;
const GC_NGC_A_DECODE_ADDR: u32 = 0x000b_cec0;
const GC_NGC_B_ADDR: u32 = 0x000b_cf40;
const GC_NGC_B_DECODE_ADDR: u32 = 0x000b_ce40;
const GC_IDLEV_A_ADDR: u32 = 0x000b_ce00;
const GC_IDLEV_A_DECODE_ADDR: u32 = 0x000b_ccc0;
const GC_IDLEV_B_ADDR: u32 = 0x000b_cdc0;
const GC_IDLEV_B_DECODE_ADDR: u32 = 0x000b_cac0;
const GC_IDLEV_C_ADDR: u32 = 0x000b_cd80;
const GC_IDLEV_C_DECODE_ADDR: u32 = 0x000b_c8c0;
const GC_IDLEV_D_ADDR: u32 = 0x000b_cd40;
const GC_IDLEV_D_DECODE_ADDR: u32 = 0x000b_c6c0;
const GC_IDLOC_A_ATK_ADDR: u32 = 0x000b_c640;
const GC_IDLOC_A_ATK_DECODE_ADDR: u32 = 0x000b_c340;
const GC_IDLOC_A_REL_ADDR: u32 = 0x000b_c5c0;
const GC_IDLOC_A_REL_DECODE_ADDR: u32 = 0x000b_c180;
const GC_IDLOC_B_ATK_ADDR: u32 = 0x000b_c540;
const GC_IDLOC_B_ATK_DECODE_ADDR: u32 = 0x000b_c300;
const GC_IDLOC_B_REL_ADDR: u32 = 0x000b_c4c0;
const GC_IDLOC_B_REL_DECODE_ADDR: u32 = 0x000b_c140;
const GC_IDLOC_C_ATK_ADDR: u32 = 0x000b_c440;
const GC_IDLOC_C_ATK_DECODE_ADDR: u32 = 0x000b_c280;
const GHPC_NWAVS_A_ADDR: u32 = 0x000b_c0a0;
const GHPC_NWAVS_A_DECODE_ADDR: u32 = 0x000b_c000;
const GHPC_NWAVS_B_ADDR: u32 = 0x000b_c080;
const GHPC_NWAVS_B_DECODE_ADDR: u32 = 0x000b_bfc0;
const GHPC_FREQ_A_ADDR: u32 = 0x000b_bbc0;
const GHPC_FREQ_A_DECODE_ADDR: u32 = 0x000b_b3c0;
const GHPC_IDSF_AA_ADDR: u32 = 0x000b_b340;
const GHPC_IDSF_AA_DECODE_ADDR: u32 = 0x000b_b140;
const GHPC_IDSF_AB_ADDR: u32 = 0x000b_b2c0;
const GHPC_IDSF_AB_DECODE_ADDR: u32 = 0x000b_b040;
const GHPC_IDSF_B_ADDR: u32 = 0x000b_b240;
const GHPC_IDSF_B_DECODE_ADDR: u32 = 0x000b_af40;
const GHPC_IDAM_AA_ADDR: u32 = 0x000b_af00;
const GHPC_IDAM_AA_DECODE_ADDR: u32 = 0x000b_ae20;
const GHPC_IDAM_AB_ADDR: u32 = 0x000b_aec0;
const GHPC_IDAM_AB_DECODE_ADDR: u32 = 0x000b_ada0;
const GHPC_IDAM_C_ADDR: u32 = 0x000b_aea0;
const GHPC_IDAM_C_DECODE_ADDR: u32 = 0x000b_ad80;
const CT_A_ADDR: u32 = 0x000b_d020;
const CT_A_DECODE_ADDR: u32 = 0x000b_cfb0;
const CT_B_ADDR: u32 = 0x000b_d000;
const CT_B_DECODE_ADDR: u32 = 0x000b_cfa0;
const CT_C_ADDR: u32 = 0x000b_cfe0;
const CT_C_DECODE_ADDR: u32 = 0x000b_cf90;
const CT_D_ADDR: u32 = 0x000b_cfc0;
const CT_D_DECODE_ADDR: u32 = 0x000b_cf80;
const WLC_PACK_ADDR: u32 = 0x0008_9940;
const WLC_A_DECODE_ADDR: u32 = 0x0008_9928;
const WLC_B_DECODE_ADDR: u32 = 0x0008_9920;
const WLC_C_DECODE_ADDR: u32 = 0x0008_9900;
const WLC_D_DECODE_ADDR: u32 = 0x0008_98e0;
const WLC_PACK_SLICE_BYTES: usize = 32;
const SFC_PACK_ADDR: u32 = 0x000b_a980;
const SFC_A_DECODE_ADDR: u32 = 0x000b_a780;
const SFC_B_DECODE_ADDR: u32 = 0x000b_a580;
const SFC_C_DECODE_ADDR: u32 = 0x000b_a380;
const SFC_D_DECODE_ADDR: u32 = 0x000b_a180;
const SFC_PACK_SLICE_BYTES: usize = 256;
const SFC_SG_PACK_ADDR: u32 = 0x000b_a080;
const SFC_SG_A_DECODE_ADDR: u32 = 0x000b_a040;
const SFC_SG_B_DECODE_ADDR: u32 = 0x000b_a000;
const SFC_SG_C_DECODE_ADDR: u32 = 0x000b_9f80;
const SFC_SG_D_DECODE_ADDR: u32 = 0x000b_9f00;
const SFC_SG_PACK_SLICE_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HuffmanCodeEntry {
    pub code: u16,
    pub bit_len: u8,
    pub reserved: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct HuffmanCodeTable {
    symbol: &'static str,
    native_addr: u32,
    bytes: &'static [u8],
}

impl HuffmanCodeTable {
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

#[derive(Debug, Clone, Copy)]
pub struct HuffmanDescriptor {
    symbol: &'static str,
    bytes: &'static [u8],
    pack_table: HuffmanCodeTable,
    decode_table_symbol: &'static str,
    decode_table_native_addr: u32,
}

impl HuffmanDescriptor {
    pub const fn symbol(self) -> &'static str {
        self.symbol
    }

    pub const fn bytes(self) -> &'static [u8] {
        self.bytes
    }

    pub const fn pack_table(self) -> HuffmanCodeTable {
        self.pack_table
    }

    pub const fn pack_table_symbol(self) -> &'static str {
        self.pack_table.symbol
    }

    pub const fn pack_table_native_addr(self) -> u32 {
        self.pack_table.native_addr
    }

    pub const fn decode_table_symbol(self) -> &'static str {
        self.decode_table_symbol
    }

    pub const fn decode_table_native_addr(self) -> u32 {
        self.decode_table_native_addr
    }

    pub fn file_pack_pointer_placeholder(self) -> Option<u32> {
        read_u32_le(self.bytes, 0)
    }

    pub fn file_decode_pointer_placeholder(self) -> Option<u32> {
        read_u32_le(self.bytes, 1)
    }

    pub fn config_word(self) -> Option<u32> {
        read_u32_le(self.bytes, 3)
    }

    pub fn max_decode_bits(self) -> u8 {
        self.bytes[0x10]
    }

    pub fn symbol_mask(self) -> u8 {
        self.bytes[0x16]
    }
}

pub const fn gc_ngc_a() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_ngc_A",
        bytes: &G_HC_GC_NGC_A,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_ngc_pack_A",
            native_addr: GC_NGC_A_ADDR,
            bytes: &G_A_NGC_PACK_A,
        },
        decode_table_symbol: "g_a_ngc_dectbl_A",
        decode_table_native_addr: GC_NGC_A_DECODE_ADDR,
    }
}

pub const fn gc_ngc_b() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_ngc_B",
        bytes: &G_HC_GC_NGC_B,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_ngc_pack_B",
            native_addr: GC_NGC_B_ADDR,
            bytes: &G_A_NGC_PACK_B,
        },
        decode_table_symbol: "g_a_ngc_dectbl_B",
        decode_table_native_addr: GC_NGC_B_DECODE_ADDR,
    }
}

pub const fn gc_idlev_a() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idlev_A",
        bytes: &G_HC_GC_IDLEV_A,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idlev_pack_A",
            native_addr: GC_IDLEV_A_ADDR,
            bytes: &G_A_IDLEV_PACK_A,
        },
        decode_table_symbol: "g_a_idlev_dectbl_A",
        decode_table_native_addr: GC_IDLEV_A_DECODE_ADDR,
    }
}

pub const fn gc_idlev_b() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idlev_B",
        bytes: &G_HC_GC_IDLEV_B,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idlev_pack_B",
            native_addr: GC_IDLEV_B_ADDR,
            bytes: &G_A_IDLEV_PACK_B,
        },
        decode_table_symbol: "g_a_idlev_dectbl_B",
        decode_table_native_addr: GC_IDLEV_B_DECODE_ADDR,
    }
}

pub const fn gc_idlev_c() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idlev_C",
        bytes: &G_HC_GC_IDLEV_C,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idlev_pack_C",
            native_addr: GC_IDLEV_C_ADDR,
            bytes: &G_A_IDLEV_PACK_C,
        },
        decode_table_symbol: "g_a_idlev_dectbl_C",
        decode_table_native_addr: GC_IDLEV_C_DECODE_ADDR,
    }
}

pub const fn gc_idlev_d() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idlev_D",
        bytes: &G_HC_GC_IDLEV_D,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idlev_pack_D",
            native_addr: GC_IDLEV_D_ADDR,
            bytes: &G_A_IDLEV_PACK_D,
        },
        decode_table_symbol: "g_a_idlev_dectbl_D",
        decode_table_native_addr: GC_IDLEV_D_DECODE_ADDR,
    }
}

pub const fn gc_idloc_a_atk() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idloc_A_atk",
        bytes: &G_HC_GC_IDLOC_A_ATK,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idloc_pack_A_atk",
            native_addr: GC_IDLOC_A_ATK_ADDR,
            bytes: &G_A_IDLOC_PACK_A_ATK,
        },
        decode_table_symbol: "g_a_idloc_dectbl_A_atk",
        decode_table_native_addr: GC_IDLOC_A_ATK_DECODE_ADDR,
    }
}

pub const fn gc_idloc_a_rel() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idloc_A_rel",
        bytes: &G_HC_GC_IDLOC_A_REL,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idloc_pack_A_rel",
            native_addr: GC_IDLOC_A_REL_ADDR,
            bytes: &G_A_IDLOC_PACK_A_REL,
        },
        decode_table_symbol: "g_a_idloc_dectbl_A_rel",
        decode_table_native_addr: GC_IDLOC_A_REL_DECODE_ADDR,
    }
}

pub const fn gc_idloc_b_atk() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idloc_B_atk",
        bytes: &G_HC_GC_IDLOC_B_ATK,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idloc_pack_B_atk",
            native_addr: GC_IDLOC_B_ATK_ADDR,
            bytes: &G_A_IDLOC_PACK_B_ATK,
        },
        decode_table_symbol: "g_a_idloc_dectbl_B_atk",
        decode_table_native_addr: GC_IDLOC_B_ATK_DECODE_ADDR,
    }
}

pub const fn gc_idloc_b_rel() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idloc_B_rel",
        bytes: &G_HC_GC_IDLOC_B_REL,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idloc_pack_B_rel",
            native_addr: GC_IDLOC_B_REL_ADDR,
            bytes: &G_A_IDLOC_PACK_B_REL,
        },
        decode_table_symbol: "g_a_idloc_dectbl_B_rel",
        decode_table_native_addr: GC_IDLOC_B_REL_DECODE_ADDR,
    }
}

pub const fn gc_idloc_c_atk() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_gc_idloc_C_atk",
        bytes: &G_HC_GC_IDLOC_C_ATK,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idloc_pack_C_atk",
            native_addr: GC_IDLOC_C_ATK_ADDR,
            bytes: &G_A_IDLOC_PACK_C_ATK,
        },
        decode_table_symbol: "g_a_idloc_dectbl_C_atk",
        decode_table_native_addr: GC_IDLOC_C_ATK_DECODE_ADDR,
    }
}

pub const fn ghpc_freq_a() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_freq_A",
        bytes: &G_HC_GHPC_FREQ_A,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_freq_pack_A",
            native_addr: GHPC_FREQ_A_ADDR,
            bytes: &G_A_FREQ_PACK_A,
        },
        decode_table_symbol: "g_a_freq_dectbl_A",
        decode_table_native_addr: GHPC_FREQ_A_DECODE_ADDR,
    }
}

pub const fn ghpc_nwavs_a() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_nwavs_A",
        bytes: &G_HC_GHPC_NWAVS_A,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_nwavs_pack_A",
            native_addr: GHPC_NWAVS_A_ADDR,
            bytes: &G_A_NWAVS_PACK_A,
        },
        decode_table_symbol: "g_a_nwavs_dectbl_A",
        decode_table_native_addr: GHPC_NWAVS_A_DECODE_ADDR,
    }
}

pub const fn ghpc_nwavs_b() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_nwavs_B",
        bytes: &G_HC_GHPC_NWAVS_B,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_nwavs_pack_B",
            native_addr: GHPC_NWAVS_B_ADDR,
            bytes: &G_A_NWAVS_PACK_B,
        },
        decode_table_symbol: "g_a_nwavs_dectbl_B",
        decode_table_native_addr: GHPC_NWAVS_B_DECODE_ADDR,
    }
}

pub const fn ghpc_idsf_aa() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_idsf_AA",
        bytes: &G_HC_GHPC_IDSF_AA,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idsf_pack_AA",
            native_addr: GHPC_IDSF_AA_ADDR,
            bytes: &G_A_IDSF_PACK_AA,
        },
        decode_table_symbol: "g_a_idsf_dectbl_AA",
        decode_table_native_addr: GHPC_IDSF_AA_DECODE_ADDR,
    }
}

pub const fn ghpc_idsf_ab() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_idsf_AB",
        bytes: &G_HC_GHPC_IDSF_AB,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idsf_pack_AB",
            native_addr: GHPC_IDSF_AB_ADDR,
            bytes: &G_A_IDSF_PACK_AB,
        },
        decode_table_symbol: "g_a_idsf_dectbl_AB",
        decode_table_native_addr: GHPC_IDSF_AB_DECODE_ADDR,
    }
}

pub const fn ghpc_idsf_b() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_idsf_B",
        bytes: &G_HC_GHPC_IDSF_B,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idsf_pack_B",
            native_addr: GHPC_IDSF_B_ADDR,
            bytes: &G_A_IDSF_PACK_B,
        },
        decode_table_symbol: "g_a_idsf_dectbl_B",
        decode_table_native_addr: GHPC_IDSF_B_DECODE_ADDR,
    }
}

pub const fn ghpc_idam_aa() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_idam_AA",
        bytes: &G_HC_GHPC_IDAM_AA,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idam_pack_AA",
            native_addr: GHPC_IDAM_AA_ADDR,
            bytes: &G_A_IDAM_PACK_AA,
        },
        decode_table_symbol: "g_a_idam_dectbl_AA",
        decode_table_native_addr: GHPC_IDAM_AA_DECODE_ADDR,
    }
}

pub const fn ghpc_idam_ab() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_idam_AB",
        bytes: &G_HC_GHPC_IDAM_AB,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idam_pack_AB",
            native_addr: GHPC_IDAM_AB_ADDR,
            bytes: &G_A_IDAM_PACK_AB,
        },
        decode_table_symbol: "g_a_idam_dectbl_AB",
        decode_table_native_addr: GHPC_IDAM_AB_DECODE_ADDR,
    }
}

pub const fn ghpc_idam_c() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ghpc_idam_C",
        bytes: &G_HC_GHPC_IDAM_C,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_idam_pack_C",
            native_addr: GHPC_IDAM_C_ADDR,
            bytes: &G_A_IDAM_PACK_C,
        },
        decode_table_symbol: "g_a_idam_dectbl_C",
        decode_table_native_addr: GHPC_IDAM_C_DECODE_ADDR,
    }
}

pub const fn ct_a() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ct_A",
        bytes: &G_HC_CT_A,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_ct_pack_A",
            native_addr: CT_A_ADDR,
            bytes: &G_A_CT_PACK_A,
        },
        decode_table_symbol: "g_a_ct_dectbl_A",
        decode_table_native_addr: CT_A_DECODE_ADDR,
    }
}

pub const fn ct_b() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ct_B",
        bytes: &G_HC_CT_B,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_ct_pack_B",
            native_addr: CT_B_ADDR,
            bytes: &G_A_CT_PACK_B,
        },
        decode_table_symbol: "g_a_ct_dectbl_B",
        decode_table_native_addr: CT_B_DECODE_ADDR,
    }
}

pub const fn ct_c() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ct_C",
        bytes: &G_HC_CT_C,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_ct_pack_C",
            native_addr: CT_C_ADDR,
            bytes: &G_A_CT_PACK_C,
        },
        decode_table_symbol: "g_a_ct_dectbl_C",
        decode_table_native_addr: CT_C_DECODE_ADDR,
    }
}

pub const fn ct_d() -> HuffmanDescriptor {
    HuffmanDescriptor {
        symbol: "g_hc_ct_D",
        bytes: &G_HC_CT_D,
        pack_table: HuffmanCodeTable {
            symbol: "g_a_ct_pack_D",
            native_addr: CT_D_ADDR,
            bytes: &G_A_CT_PACK_D,
        },
        decode_table_symbol: "g_a_ct_dectbl_D",
        decode_table_native_addr: CT_D_DECODE_ADDR,
    }
}

pub fn wlc_a() -> HuffmanDescriptor {
    wlc_descriptor(
        0,
        "g_a_hc_wl_A",
        0x00,
        "g_a_wlc_dectbl_A",
        WLC_A_DECODE_ADDR,
    )
}

pub fn wlc_b() -> HuffmanDescriptor {
    wlc_descriptor(
        1,
        "g_a_hc_wl_B",
        0x20,
        "g_a_wlc_dectbl_B",
        WLC_B_DECODE_ADDR,
    )
}

pub fn wlc_c() -> HuffmanDescriptor {
    wlc_descriptor(
        2,
        "g_a_hc_wl_C",
        0x40,
        "g_a_wlc_dectbl_C",
        WLC_C_DECODE_ADDR,
    )
}

pub fn wlc_d() -> HuffmanDescriptor {
    wlc_descriptor(
        3,
        "g_a_hc_wl_D",
        0x60,
        "g_a_wlc_dectbl_D",
        WLC_D_DECODE_ADDR,
    )
}

pub fn wlc_descriptors() -> [HuffmanDescriptor; 4] {
    [wlc_a(), wlc_b(), wlc_c(), wlc_d()]
}

fn wlc_descriptor(
    index: usize,
    symbol: &'static str,
    pack_offset: usize,
    decode_table_symbol: &'static str,
    decode_table_native_addr: u32,
) -> HuffmanDescriptor {
    let descriptor_start = index * HUFFMAN_DESCRIPTOR_BYTES;
    HuffmanDescriptor {
        symbol,
        bytes: &G_A_HC_WL[descriptor_start..descriptor_start + HUFFMAN_DESCRIPTOR_BYTES],
        pack_table: HuffmanCodeTable {
            symbol: "g_aa_wlc_pack",
            native_addr: WLC_PACK_ADDR + pack_offset as u32,
            bytes: &G_AA_WLC_PACK[pack_offset..pack_offset + WLC_PACK_SLICE_BYTES],
        },
        decode_table_symbol,
        decode_table_native_addr,
    }
}

pub fn sfc_a() -> HuffmanDescriptor {
    sfc_descriptor(
        0,
        "g_a_hc_sf_A",
        0x000,
        "g_a_sfc_dectbl_A",
        SFC_A_DECODE_ADDR,
    )
}

pub fn sfc_b() -> HuffmanDescriptor {
    sfc_descriptor(
        1,
        "g_a_hc_sf_B",
        0x100,
        "g_a_sfc_dectbl_B",
        SFC_B_DECODE_ADDR,
    )
}

pub fn sfc_c() -> HuffmanDescriptor {
    sfc_descriptor(
        2,
        "g_a_hc_sf_C",
        0x200,
        "g_a_sfc_dectbl_C",
        SFC_C_DECODE_ADDR,
    )
}

pub fn sfc_d() -> HuffmanDescriptor {
    sfc_descriptor(
        3,
        "g_a_hc_sf_D",
        0x300,
        "g_a_sfc_dectbl_D",
        SFC_D_DECODE_ADDR,
    )
}

pub fn sfc_descriptors() -> [HuffmanDescriptor; 4] {
    [sfc_a(), sfc_b(), sfc_c(), sfc_d()]
}

fn sfc_descriptor(
    index: usize,
    symbol: &'static str,
    pack_offset: usize,
    decode_table_symbol: &'static str,
    decode_table_native_addr: u32,
) -> HuffmanDescriptor {
    let descriptor_start = index * HUFFMAN_DESCRIPTOR_BYTES;
    HuffmanDescriptor {
        symbol,
        bytes: &G_A_HC_SF[descriptor_start..descriptor_start + HUFFMAN_DESCRIPTOR_BYTES],
        pack_table: HuffmanCodeTable {
            symbol: "g_aa_sfc_pack",
            native_addr: SFC_PACK_ADDR + pack_offset as u32,
            bytes: &G_AA_SFC_PACK[pack_offset..pack_offset + SFC_PACK_SLICE_BYTES],
        },
        decode_table_symbol,
        decode_table_native_addr,
    }
}

pub fn sfc_sg_a() -> HuffmanDescriptor {
    sfc_sg_descriptor(
        0,
        "g_a_hc_sf_sg_A",
        0x00,
        "g_a_sfc_sg_dectbl_A",
        SFC_SG_A_DECODE_ADDR,
    )
}

pub fn sfc_sg_b() -> HuffmanDescriptor {
    sfc_sg_descriptor(
        1,
        "g_a_hc_sf_sg_B",
        0x40,
        "g_a_sfc_sg_dectbl_B",
        SFC_SG_B_DECODE_ADDR,
    )
}

pub fn sfc_sg_c() -> HuffmanDescriptor {
    sfc_sg_descriptor(
        2,
        "g_a_hc_sf_sg_C",
        0x80,
        "g_a_sfc_sg_dectbl_C",
        SFC_SG_C_DECODE_ADDR,
    )
}

pub fn sfc_sg_d() -> HuffmanDescriptor {
    sfc_sg_descriptor(
        3,
        "g_a_hc_sf_sg_D",
        0xc0,
        "g_a_sfc_sg_dectbl_D",
        SFC_SG_D_DECODE_ADDR,
    )
}

pub fn sfc_sg_descriptors() -> [HuffmanDescriptor; 4] {
    [sfc_sg_a(), sfc_sg_b(), sfc_sg_c(), sfc_sg_d()]
}

fn sfc_sg_descriptor(
    index: usize,
    symbol: &'static str,
    pack_offset: usize,
    decode_table_symbol: &'static str,
    decode_table_native_addr: u32,
) -> HuffmanDescriptor {
    let descriptor_start = index * HUFFMAN_DESCRIPTOR_BYTES;
    HuffmanDescriptor {
        symbol,
        bytes: &G_A_HC_SF_SG[descriptor_start..descriptor_start + HUFFMAN_DESCRIPTOR_BYTES],
        pack_table: HuffmanCodeTable {
            symbol: "g_aa_sfc_sg_pack",
            native_addr: SFC_SG_PACK_ADDR + pack_offset as u32,
            bytes: &G_AA_SFC_SG_PACK[pack_offset..pack_offset + SFC_SG_PACK_SLICE_BYTES],
        },
        decode_table_symbol,
        decode_table_native_addr,
    }
}
