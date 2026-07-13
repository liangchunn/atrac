#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackDispatchEntry {
    pub index: usize,
    pub native_offset: u32,
}

pub const PACK_IDWL_TABLE_NATIVE_OFFSET: u32 = 0x000f_3020;
pub const PACK_IDSF_TABLE_NATIVE_OFFSET: u32 = 0x000f_3060;
pub const PACK_GAIN_IDLOC_TABLE_NATIVE_OFFSET: u32 = 0x000f_3120;
pub const PACK_GAIN_IDLEV_TABLE_NATIVE_OFFSET: u32 = 0x000f_3160;
pub const PACK_GAIN_NGC_TABLE_NATIVE_OFFSET: u32 = 0x000f_31a0;
pub const PACK_IDCT_TABLE_NATIVE_OFFSET: u32 = 0x000f_31e0;

pub const PACK_IDWL_TABLE: [u32; 8] = [
    0x0001_fd40,
    0x0001_f540,
    0x0001_e9b0,
    0x0001_e190,
    0x0001_fd40,
    0x0001_db90,
    0x0001_d430,
    0x0001_e190,
];

pub const PACK_IDSF_TABLE: [u32; 8] = [
    0x0001_d330,
    0x0001_c780,
    0x0001_c420,
    0x0001_bcc0,
    0x0001_d330,
    0x0001_bab0,
    0x0001_b7a0,
    0x0001_3970,
];

pub const PACK_GAIN_IDLOC_TABLE: [u32; 8] = [
    0x0001_6760,
    0x0001_71f0,
    0x0001_8180,
    0x0001_7e90,
    0x0001_6760,
    0x0001_7440,
    0x0001_6c20,
    0x0001_6260,
];

pub const PACK_GAIN_IDLEV_TABLE: [u32; 8] = [
    0x0001_60e0,
    0x0001_5e50,
    0x0001_5a60,
    0x0001_5760,
    0x0001_60e0,
    0x0001_55a0,
    0x0001_5260,
    0x0001_33f0,
];

pub const PACK_GAIN_NGC_TABLE: [u32; 8] = [
    0x0001_5160,
    0x0001_5030,
    0x0001_4e20,
    0x0001_4b80,
    0x0001_5160,
    0x0001_5030,
    0x0001_4a10,
    0x0001_3390,
];

pub const PACK_IDCT_TABLE: [u32; 8] = [
    0x0001_4680,
    0x0001_42e0,
    0x0001_3d70,
    0x0001_3350,
    0x0001_4680,
    0x0001_42e0,
    0x0001_3d70,
    0x0001_39c0,
];

pub fn side_data_dispatch_index(mode_low_bits: u32, channel_parity: u32) -> usize {
    ((mode_low_bits & 3) + ((channel_parity & 1) << 2)) as usize
}

pub fn pack_idwl_entry(mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    entry(&PACK_IDWL_TABLE, mode_low_bits, channel_parity)
}

pub fn pack_idsf_entry(mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    entry(&PACK_IDSF_TABLE, mode_low_bits, channel_parity)
}

pub fn pack_gain_ngc_entry(mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    entry(&PACK_GAIN_NGC_TABLE, mode_low_bits, channel_parity)
}

pub fn pack_gain_idlev_entry(mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    entry(&PACK_GAIN_IDLEV_TABLE, mode_low_bits, channel_parity)
}

pub fn pack_gain_idloc_entry(mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    entry(&PACK_GAIN_IDLOC_TABLE, mode_low_bits, channel_parity)
}

pub fn pack_idct_entry(mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    entry(&PACK_IDCT_TABLE, mode_low_bits, channel_parity)
}

fn entry(table: &[u32; 8], mode_low_bits: u32, channel_parity: u32) -> PackDispatchEntry {
    let index = side_data_dispatch_index(mode_low_bits, channel_parity);
    PackDispatchEntry {
        index,
        native_offset: table[index],
    }
}
