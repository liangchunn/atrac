use super::generated::{
    ANA_COEF, C_II2, C_II4, C_II8, C_II16, C_IV16, G_A_COEF_C_AT5, G_A_COEF_S_AT5, G_A_FQF_AT5,
    G_A_IDCT_FIXBITS_AT5, G_A_IDSPCBANDS_AT5, G_A_IDSPCQUS_AT5, G_A_IFQF_AT5, G_A_IP016_AT5,
    G_A_IP032_AT5, G_A_IP064_AT5, G_A_IP128_AT5, G_A_IP256_AT5, G_A_ISPS_AT5, G_A_LNGAIN_AT5,
    G_A_MASK_Q_AT5, G_A_MATRIX_AT5, G_A_N2_UNDER128_AT5, G_A_NSPS_AT5, G_A_NSTEPS_AT5, G_A_REV_AT5,
    G_A_RNDTBL_AT5, G_A_SC_AT5, G_A_SC016_AT5, G_A_SC032_AT5, G_A_SC064_AT5, G_A_SC128_AT5,
    G_A_SC256_AT5, G_A_SFTBL_AT5, G_A_SFTBL_GHA_AT5, G_A_SG_SHAPE_INDEX_AT5, G_A_SPCLEV_AT5,
    G_A_WIND0_AT5, G_A_WIND1_AT5, G_A_WIND2_AT5, G_A_WIND3_AT5, G_A_X_AT5, G_A_Y_AT5,
    G_AA_TTVAL_AT5, SA_ADJSCL_STARTQU, SA_ADJUST_IQT_0TH_MONO, SA_ADJUST_IQT_0TH_STEREO,
    SA_AMTBL_GHA, SA_HALF_HANNWIN, SA_INTENSITY_BAND_44KHZ, SA_INTENSITY_BAND_48KHZ,
    SA_LIMIT_IDTF_MONO, SA_LIMIT_IDTF_STEREO, SA_MCFX_HBR, SA_MCFX_LBR, SA_MM_032, SA_MM_064,
    SA_MM_096, SA_MM_256, SA_NENCODETBLS, SA_PCFX, SA_POS_WEIGHT, SA_SINTBL, SA_SINTBL0,
    SA_SINTBL1, SA_SINTBL2, SA_SINTBL3, SA_SPC_FLOOR, SA_SPC_STARTQU, SA_TC_032, SA_TC_064,
    SA_TC_096, SA_TC_256, SA_TLEV_THRED_064, SA_TLEV_THRED_096, SA_WCFX_BR_S064_M032,
    SA_WCFX_BR_S096_M064, SA_WCFX_BR_S128_M096, SA_WCFX_BR_S256_M128, SAA_IDTF_MONO,
    SAA_IDTF_STEREO,
};
use super::view::{f32_table, i16_table, i32_table, u16_table, u32_table};
use std::sync::LazyLock;

pub const SFTBL_AT5_ENTRIES: usize = 64;
pub const SFTBL_GHA_AT5_ENTRIES: usize = 64;
pub const AMTBL_GHA_ENTRIES: usize = 16;
pub const Y_AT5_ENTRIES: usize = 17;
pub const X_AT5_ENTRIES: usize = 33;
pub const SG_SHAPE_INDEX_AT5_ENTRIES: usize = 32;
pub const ISPS_AT5_ENTRIES: usize = 33;
pub const NSPS_AT5_ENTRIES: usize = 32;
pub const NSTEPS_AT5_ENTRIES: usize = 8;
pub const IFQF_AT5_ENTRIES: usize = 8;
pub const SPCLEV_AT5_ENTRIES: usize = 16;
pub const RNDTBL_AT5_ENTRIES: usize = 1024;
pub const ADJSCL_STARTQU_AT5_ENTRIES: usize = 32;
pub const IDSPCBANDS_AT5_ENTRIES: usize = 16;
pub const MASK_Q_AT5_ENTRIES: usize = 7;
pub const IDCT_FIXBITS_AT5_ENTRIES: usize = 2;
pub const IDSPCQUS_AT5_ENTRIES: usize = 32;
pub const N2_UNDER128_AT5_ENTRIES: usize = 128;
pub const COEF_C_AT5_ENTRIES: usize = 128;
pub const COEF_S_AT5_ENTRIES: usize = 128;
pub const MATRIX_AT5_ENTRIES: usize = 128;
pub const WIND_AT5_ENTRIES: usize = 256;
pub const REV_AT5_ENTRIES: usize = 16;
pub const IP016_AT5_ENTRIES: usize = 2;
pub const IP032_AT5_ENTRIES: usize = 2;
pub const IP064_AT5_ENTRIES: usize = 4;
pub const IP128_AT5_ENTRIES: usize = 4;
pub const IP256_AT5_ENTRIES: usize = 8;
pub const SC016_AT5_ENTRIES: usize = 8;
pub const SC032_AT5_ENTRIES: usize = 16;
pub const SC064_AT5_ENTRIES: usize = 32;
pub const SC128_AT5_ENTRIES: usize = 64;
pub const SC256_AT5_ENTRIES: usize = 128;
pub const SC_AT5_ENTRIES: usize = 128;
pub const LNGAIN_AT5_ENTRIES: usize = 16;
pub const HALF_HANNWIN_AT5_ENTRIES: usize = 8;
pub const SIN_AT5_ENTRIES: usize = 2048;
pub const TLEV_THRED_AT5_ENTRIES: usize = 16;
pub const WIN_AT5_ENTRIES: usize = 256;
pub const WIN_AT5_UPPER_HALF_INDEX: usize = 128;
pub const ALLOCATION_WCFX_AT5_ENTRIES: usize = 32;
pub const ALLOCATION_MM_AT5_ENTRIES: usize = 32;
pub const ALLOCATION_TC_AT5_ENTRIES: usize = 16;
pub const NENCODETBLS_AT5_ENTRIES: usize = 2;
pub const ALLOCATION_MCFX_AT5_ENTRIES: usize = 32;
pub const LIMIT_IDTF_STEREO_ENTRIES: usize = 32;
pub const ADJUST_IQT_0TH_STEREO_ENTRIES: usize = 32;
pub const SAA_IDTF_STEREO_ENTRIES: usize = 1024;
pub const LIMIT_IDTF_MONO_ENTRIES: usize = 32;
pub const ADJUST_IQT_0TH_MONO_ENTRIES: usize = 32;
pub const SAA_IDTF_MONO_ENTRIES: usize = 1024;
pub const SPC_FLOOR_ENTRIES: usize = 32;
pub const POS_WEIGHT_ENTRIES: usize = 8;
pub const SPC_STARTQU_ENTRIES: usize = 32;
pub const INTENSITY_BAND_ENTRIES: usize = 32;
pub const SINTBL_AT5_ENTRIES: usize = 65;
pub const SINTBL0_AT5_ENTRIES: usize = 9;
pub const SINTBL1_AT5_ENTRIES: usize = 17;
pub const SINTBL2_AT5_ENTRIES: usize = 33;
pub const SINTBL3_AT5_ENTRIES: usize = 65;

pub fn sftbl_at5() -> [f32; SFTBL_AT5_ENTRIES] {
    f32_table(&G_A_SFTBL_AT5).expect("generated g_a_sftbl_at5 length should be 64 f32s")
}

pub fn sftbl_gha_at5() -> [f32; SFTBL_GHA_AT5_ENTRIES] {
    f32_table(&G_A_SFTBL_GHA_AT5).expect("generated g_a_sftbl_gha_at5 length should be 64 f32s")
}

pub fn amtbl_gha() -> [f32; AMTBL_GHA_ENTRIES] {
    f32_table(&SA_AMTBL_GHA).expect("generated sa_amtbl_gha length should be 16 f32s")
}

pub fn y_at5() -> [u8; Y_AT5_ENTRIES] {
    G_A_Y_AT5
}

pub fn x_at5() -> [u8; X_AT5_ENTRIES] {
    G_A_X_AT5
}

pub fn sg_shape_index_at5() -> [u8; SG_SHAPE_INDEX_AT5_ENTRIES] {
    G_A_SG_SHAPE_INDEX_AT5
}

pub const FQF_AT5_ENTRIES: usize = 8;
pub const TTVAL_AT5_ENTRIES: usize = 128;

pub fn fqf_at5() -> [f32; FQF_AT5_ENTRIES] {
    f32_table(&G_A_FQF_AT5).expect("generated g_a_fqf_at5 length should be 8 f32s")
}

pub fn ttval_at5() -> [f32; TTVAL_AT5_ENTRIES] {
    f32_table(&G_AA_TTVAL_AT5).expect("generated g_aa_ttval_at5 length should be 128 f32s")
}

pub const PQF_ANA_COEF_ENTRIES: usize = 640;

pub fn pqf_ana_coef_at5() -> [f32; PQF_ANA_COEF_ENTRIES] {
    f32_table(&ANA_COEF).expect("generated ana_coef length should be 640 f32s")
}

pub fn pqf_c_iv16_at5() -> [f32; 16] {
    f32_table(&C_IV16).expect("generated c_iv16 length should be 16 f32s")
}

pub fn pqf_c_ii16_at5() -> [f32; 8] {
    f32_table(&C_II16).expect("generated c_ii16 length should be 8 f32s")
}

pub fn pqf_c_ii8_at5() -> [f32; 8] {
    f32_table(&C_II8).expect("generated c_ii8 length should be 8 f32s")
}

pub fn pqf_c_ii4_at5() -> [f32; 4] {
    f32_table(&C_II4).expect("generated c_ii4 length should be 4 f32s")
}

pub fn pqf_c_ii2_at5() -> [f32; 4] {
    f32_table(&C_II2).expect("generated c_ii2 length should be 4 f32s")
}

pub fn isps_at5() -> [u16; ISPS_AT5_ENTRIES] {
    u16_table(&G_A_ISPS_AT5).expect("generated g_a_isps_at5 length should be 33 u16s")
}

pub fn nsps_at5() -> [u8; NSPS_AT5_ENTRIES] {
    G_A_NSPS_AT5
}

/// Native `g_a_nsteps_at5` (`0x000bdbe8`): per-word-length quantizer
/// step counts consumed by the eighth allocation pass threshold.
pub fn nsteps_at5() -> [u8; NSTEPS_AT5_ENTRIES] {
    G_A_NSTEPS_AT5
}

pub fn mask_q_at5() -> [u32; MASK_Q_AT5_ENTRIES] {
    u32_table(&G_A_MASK_Q_AT5).expect("generated g_a_mask_q_at5 length should be 7 u32s")
}

pub fn idct_fixbits_at5() -> [u8; IDCT_FIXBITS_AT5_ENTRIES] {
    G_A_IDCT_FIXBITS_AT5
}

pub fn idspcqus_at5() -> [u8; IDSPCQUS_AT5_ENTRIES] {
    G_A_IDSPCQUS_AT5
}

pub fn n2_under128_at5() -> [u16; N2_UNDER128_AT5_ENTRIES] {
    u16_table(&G_A_N2_UNDER128_AT5)
        .expect("generated g_a_n2_under128_at5 length should be 128 u16s")
}

pub fn coef_c_at5() -> [f32; COEF_C_AT5_ENTRIES] {
    f32_table(&G_A_COEF_C_AT5).expect("generated g_a_coef_c_at5 length should be 128 f32s")
}

pub fn coef_s_at5() -> [f32; COEF_S_AT5_ENTRIES] {
    f32_table(&G_A_COEF_S_AT5).expect("generated g_a_coef_s_at5 length should be 128 f32s")
}

pub fn matrix_at5() -> [u16; MATRIX_AT5_ENTRIES] {
    u16_table(&G_A_MATRIX_AT5).expect("generated g_a_matrix_at5 length should be 128 u16s")
}

pub fn wind0_at5() -> [f32; WIND_AT5_ENTRIES] {
    f32_table(&G_A_WIND0_AT5).expect("generated g_a_wind0_at5 length should be 256 f32s")
}

pub fn wind1_at5() -> [f32; WIND_AT5_ENTRIES] {
    f32_table(&G_A_WIND1_AT5).expect("generated g_a_wind1_at5 length should be 256 f32s")
}

pub fn wind2_at5() -> [f32; WIND_AT5_ENTRIES] {
    f32_table(&G_A_WIND2_AT5).expect("generated g_a_wind2_at5 length should be 256 f32s")
}

pub fn wind3_at5() -> [f32; WIND_AT5_ENTRIES] {
    f32_table(&G_A_WIND3_AT5).expect("generated g_a_wind3_at5 length should be 256 f32s")
}

pub fn rev_at5() -> [u8; REV_AT5_ENTRIES] {
    G_A_REV_AT5
}

pub fn ip016_at5() -> [u32; IP016_AT5_ENTRIES] {
    *ip016_at5_ref()
}

pub fn ip032_at5() -> [u32; IP032_AT5_ENTRIES] {
    *ip032_at5_ref()
}

pub fn ip064_at5() -> [u32; IP064_AT5_ENTRIES] {
    *ip064_at5_ref()
}

pub fn ip128_at5() -> [u32; IP128_AT5_ENTRIES] {
    *ip128_at5_ref()
}

pub fn ip256_at5() -> [u32; IP256_AT5_ENTRIES] {
    *ip256_at5_ref()
}

pub fn sc016_at5() -> [f32; SC016_AT5_ENTRIES] {
    *sc016_at5_ref()
}

pub fn sc032_at5() -> [f32; SC032_AT5_ENTRIES] {
    *sc032_at5_ref()
}

pub fn sc064_at5() -> [f32; SC064_AT5_ENTRIES] {
    *sc064_at5_ref()
}

pub fn sc128_at5() -> [f32; SC128_AT5_ENTRIES] {
    *sc128_at5_ref()
}

pub fn sc256_at5() -> [f32; SC256_AT5_ENTRIES] {
    *sc256_at5_ref()
}

pub fn sc_at5() -> [f32; SC_AT5_ENTRIES] {
    f32_table(&G_A_SC_AT5).expect("generated g_a_sc_at5 length should be 128 f32s")
}

pub fn lngain_at5() -> [i16; LNGAIN_AT5_ENTRIES] {
    i16_table(&G_A_LNGAIN_AT5).expect("generated g_a_lngain_at5 length should be 16 i16s")
}

/// Native `g_a_ifqf_at5` (`0x000bd360`): inverse quantization factors
/// per word length, consumed by the scalefactor-adjust energy model.
pub fn ifqf_at5() -> [f32; IFQF_AT5_ENTRIES] {
    f32_table(&G_A_IFQF_AT5).expect("generated g_a_ifqf_at5 length should be 8 f32s")
}

/// Native `g_a_spclev_at5` (`0x000bd3a0`): spectral noise levels
/// indexed by the per-group `+0x1c6f8` words in `pwc_qu_at5`.
pub fn spclev_at5() -> [f32; SPCLEV_AT5_ENTRIES] {
    f32_table(&G_A_SPCLEV_AT5).expect("generated g_a_spclev_at5 length should be 16 f32s")
}

/// Native `g_a_rndtbl_at5` (`0x000b9440`): the 1024-entry i16 dither
/// table `pwc_qu_at5` scales by `2^-15` into its noise scratch.
pub fn rndtbl_at5() -> [i16; RNDTBL_AT5_ENTRIES] {
    i16_table(&G_A_RNDTBL_AT5).expect("generated g_a_rndtbl_at5 length should be 1024 i16s")
}

/// Native `sa_adjscl_startqu` (`0x000c0ba0`): first adjusted band per
/// encode selector in `adjust_scalefactors_at5`.
pub fn adjscl_startqu_at5() -> [u8; ADJSCL_STARTQU_AT5_ENTRIES] {
    SA_ADJSCL_STARTQU
}

/// Native `g_a_idspcbands_at5` (`0x000b9c60`): per-group index into
/// the `+0x1c6f8` spectral level words used by `pwc_qu_at5`.
pub fn idspcbands_at5() -> [u8; IDSPCBANDS_AT5_ENTRIES] {
    G_A_IDSPCBANDS_AT5
}

pub fn half_hannwin_at5() -> [f32; HALF_HANNWIN_AT5_ENTRIES] {
    f32_table(&SA_HALF_HANNWIN).expect("generated sa_half_hannwin length should be 8 f32s")
}

pub fn wcfx_br_s064_m032_at5() -> [f32; ALLOCATION_WCFX_AT5_ENTRIES] {
    f32_table(&SA_WCFX_BR_S064_M032)
        .expect("generated sa_wcfx_br_s064_m032 length should be 32 f32s")
}

pub fn wcfx_br_s096_m064_at5() -> [f32; ALLOCATION_WCFX_AT5_ENTRIES] {
    f32_table(&SA_WCFX_BR_S096_M064)
        .expect("generated sa_wcfx_br_s096_m064 length should be 32 f32s")
}

pub fn wcfx_br_s128_m096_at5() -> [f32; ALLOCATION_WCFX_AT5_ENTRIES] {
    f32_table(&SA_WCFX_BR_S128_M096)
        .expect("generated sa_wcfx_br_s128_m096 length should be 32 f32s")
}

pub fn wcfx_br_s256_m128_at5() -> [f32; ALLOCATION_WCFX_AT5_ENTRIES] {
    f32_table(&SA_WCFX_BR_S256_M128)
        .expect("generated sa_wcfx_br_s256_m128 length should be 32 f32s")
}

pub fn mm_032_at5() -> [i32; ALLOCATION_MM_AT5_ENTRIES] {
    i32_table(&SA_MM_032).expect("generated sa_mm_032 length should be 32 i32s")
}

pub fn mm_064_at5() -> [i32; ALLOCATION_MM_AT5_ENTRIES] {
    i32_table(&SA_MM_064).expect("generated sa_mm_064 length should be 32 i32s")
}

pub fn mm_096_at5() -> [i32; ALLOCATION_MM_AT5_ENTRIES] {
    i32_table(&SA_MM_096).expect("generated sa_mm_096 length should be 32 i32s")
}

pub fn mm_256_at5() -> [i32; ALLOCATION_MM_AT5_ENTRIES] {
    i32_table(&SA_MM_256).expect("generated sa_mm_256 length should be 32 i32s")
}

/// Native `sa_intensity_band_44kHz` (`0x000bf420`): AT5 `at5enc_sigproc`
/// mode-3 band_count per block selector at 44.1 kHz (decompile 43239-43246,
/// indexed by `cfg+0x1e8`). 32 x i32.
pub fn intensity_band_44khz() -> [i32; INTENSITY_BAND_ENTRIES] {
    i32_table(&SA_INTENSITY_BAND_44KHZ)
        .expect("generated sa_intensity_band_44kHz length should be 32 i32s")
}

/// Native `sa_intensity_band_48kHz` (`0x000bf4a0`): the 48 kHz sibling of
/// [`intensity_band_44khz`] (decompile 43241-43242). 32 x i32.
pub fn intensity_band_48khz() -> [i32; INTENSITY_BAND_ENTRIES] {
    i32_table(&SA_INTENSITY_BAND_48KHZ)
        .expect("generated sa_intensity_band_48kHz length should be 32 i32s")
}

/// Native `sa_sintbl` (`0x000bf520`): the mode-3 intensity-stereo per-band
/// spectrum rotation sine table (decompile 43346-43360, native
/// `at5enc_sigproc` cold region). The head half reads `[0..0x40]` ascending,
/// the tail half reads `[0x40]` down to `[1]` descending. 65 x f32.
pub fn sintbl_at5() -> [f32; SINTBL_AT5_ENTRIES] {
    f32_table(&SA_SINTBL).expect("generated sa_sintbl length should be 65 f32s")
}

/// Native `sa_sintbl0` (`0x000bf860`): the shortest `power_reconst_at5`
/// transition table (length 8, index `0..=8`), selected when the weight ratio
/// exceeds 16 (decompile 4498/4526, leaf native `0x1b970`). 9 x f32.
pub fn sintbl0_at5() -> [f32; SINTBL0_AT5_ENTRIES] {
    f32_table(&SA_SINTBL0).expect("generated sa_sintbl0 length should be 9 f32s")
}

/// Native `sa_sintbl1` (`0x000bf800`): the `power_reconst_at5` transition table
/// for `8 < ratio <= 16` (length 0x10, index `0..=0x10`; decompile 4560). 17 x
/// f32.
pub fn sintbl1_at5() -> [f32; SINTBL1_AT5_ENTRIES] {
    f32_table(&SA_SINTBL1).expect("generated sa_sintbl1 length should be 17 f32s")
}

/// Native `sa_sintbl2` (`0x000bf760`): the `power_reconst_at5` transition table
/// for `4 < ratio <= 8` (length 0x20, index `0..=0x20`; decompile 4547). 33 x
/// f32.
pub fn sintbl2_at5() -> [f32; SINTBL2_AT5_ENTRIES] {
    f32_table(&SA_SINTBL2).expect("generated sa_sintbl2 length should be 33 f32s")
}

/// Native `sa_sintbl3` (`0x000bf640`): the longest `power_reconst_at5`
/// transition table for `ratio <= 4` (length 0x40, index `0..=0x40`; decompile
/// 4552/4556). Bit-identical to [`sintbl_at5`]. 65 x f32.
pub fn sintbl3_at5() -> [f32; SINTBL3_AT5_ENTRIES] {
    f32_table(&SA_SINTBL3).expect("generated sa_sintbl3 length should be 65 f32s")
}

pub fn tc_032_at5() -> [f32; ALLOCATION_TC_AT5_ENTRIES] {
    f32_table(&SA_TC_032).expect("generated sa_tc_032 length should be 16 f32s")
}

pub fn tc_064_at5() -> [f32; ALLOCATION_TC_AT5_ENTRIES] {
    f32_table(&SA_TC_064).expect("generated sa_tc_064 length should be 16 f32s")
}

pub fn tc_096_at5() -> [f32; ALLOCATION_TC_AT5_ENTRIES] {
    f32_table(&SA_TC_096).expect("generated sa_tc_096 length should be 16 f32s")
}

pub fn tc_256_at5() -> [f32; ALLOCATION_TC_AT5_ENTRIES] {
    f32_table(&SA_TC_256).expect("generated sa_tc_256 length should be 16 f32s")
}

pub fn nencodetbls_at5() -> [i32; NENCODETBLS_AT5_ENTRIES] {
    i32_table(&SA_NENCODETBLS).expect("generated sa_nencodetbls length should be 2 i32s")
}

/// Native `sa_limit_idtf_stereo` (`0x000c0540`): calc shared `+0x114`
/// idtf clamp-round limit per encode selector (stereo).
pub fn limit_idtf_stereo() -> [u8; LIMIT_IDTF_STEREO_ENTRIES] {
    SA_LIMIT_IDTF_STEREO
}

/// Native `sa_adjust_iqt_0th_stereo` (`0x000c0560`): calc shared `+0x116`
/// idtf start band per encode selector (stereo).
pub fn adjust_iqt_0th_stereo() -> [u8; ADJUST_IQT_0TH_STEREO_ENTRIES] {
    SA_ADJUST_IQT_0TH_STEREO
}

/// Native `saa_idtf_stereo` (`0x000c0580`): calc `+0xcc` idsf-quant seed
/// table indexed `[selector * 0x20 + band]` (stereo, 32 x 32 bytes).
pub fn saa_idtf_stereo() -> [u8; SAA_IDTF_STEREO_ENTRIES] {
    SAA_IDTF_STEREO
}

/// Native `sa_limit_idtf_mono` (`0x000c0100`): calc shared `+0x114` idtf
/// clamp-round limit per encode selector (mono). `calc_channel_block_at5`
/// reads this in the `param_4 == 1` else-arm (decompile 43975-43979).
pub fn limit_idtf_mono() -> [u8; LIMIT_IDTF_MONO_ENTRIES] {
    SA_LIMIT_IDTF_MONO
}

/// Native `sa_adjust_iqt_0th_mono` (`0x000c0120`): calc shared `+0x116` idtf
/// start band per encode selector (mono). `calc_channel_block_at5` reads this
/// in the `param_4 == 1` else-arm (decompile 43975-43979).
pub fn adjust_iqt_0th_mono() -> [u8; ADJUST_IQT_0TH_MONO_ENTRIES] {
    SA_ADJUST_IQT_0TH_MONO
}

/// Native `saa_idtf_mono` (`0x000c0140`, saa base GOT-0x33478): calc `+0xcc`
/// idsf-quant seed table indexed `[selector * 0x20 + band]` (mono, 32 x 32
/// bytes). Read by `calc_channel_block_at5` in the `param_4 == 1` else-arm
/// (decompile 43976).
pub fn saa_idtf_mono() -> [u8; SAA_IDTF_MONO_ENTRIES] {
    SAA_IDTF_MONO
}

/// Native `sa_spc_floor` (`0x000c0b20`): calc spectral-level position
/// weight floor band per band.
pub fn spc_floor() -> [u8; SPC_FLOOR_ENTRIES] {
    SA_SPC_FLOOR
}

/// Native `sa_pos_weight` (`0x000c0b40`): calc spectral-level position
/// weight, read as 8 little-endian `u32`s.
pub fn pos_weight() -> [u32; POS_WEIGHT_ENTRIES] {
    u32_table(&SA_POS_WEIGHT).expect("generated sa_pos_weight length should be 8 u32s")
}

/// Native `sa_spc_startqu` (`0x000c0b60`): calc spectral-level start
/// quant unit per encode selector.
pub fn spc_startqu() -> [u8; SPC_STARTQU_ENTRIES] {
    SA_SPC_STARTQU
}

pub fn pcfx_at5() -> [f32; ALLOCATION_MCFX_AT5_ENTRIES] {
    f32_table(&SA_PCFX).expect("generated sa_pcfx length should be 32 f32s")
}

pub fn mcfx_lbr_at5() -> [f32; ALLOCATION_MCFX_AT5_ENTRIES] {
    f32_table(&SA_MCFX_LBR).expect("generated sa_mcfx_lbr length should be 32 f32s")
}

pub fn mcfx_hbr_at5() -> [f32; ALLOCATION_MCFX_AT5_ENTRIES] {
    f32_table(&SA_MCFX_HBR).expect("generated sa_mcfx_hbr length should be 32 f32s")
}

pub fn tlev_thred_064_at5() -> [f32; TLEV_THRED_AT5_ENTRIES] {
    f32_table(&SA_TLEV_THRED_064).expect("generated sa_tlev_thred_064 length should be 16 f32s")
}

pub fn tlev_thred_096_at5() -> [f32; TLEV_THRED_AT5_ENTRIES] {
    f32_table(&SA_TLEV_THRED_096).expect("generated sa_tlev_thred_096 length should be 16 f32s")
}

#[allow(clippy::approx_constant)]
fn build_sin_at5() -> [f32; SIN_AT5_ENTRIES] {
    std::array::from_fn(|index| {
        ((index as f64) * 6.283185307179586_f64 * 0.00048828125_f64).sin() as f32
    })
}

#[allow(clippy::approx_constant)]
fn build_win_at5() -> [f32; WIN_AT5_ENTRIES] {
    std::array::from_fn(|index| {
        let cosine = ((index as f64) * 0.00390625_f64 * 6.283185307179586_f64).cos() as f32;
        ((1.0_f64 - f64::from(cosine)) * 0.5_f64) as f32
    })
}

static SIN_AT5: LazyLock<[f32; SIN_AT5_ENTRIES]> = LazyLock::new(build_sin_at5);
static WIN_AT5: LazyLock<[f32; WIN_AT5_ENTRIES]> = LazyLock::new(build_win_at5);
static IP016_AT5: LazyLock<[u32; IP016_AT5_ENTRIES]> = LazyLock::new(|| {
    u32_table(&G_A_IP016_AT5).expect("generated g_a_ip016_at5 length should be 2 u32s")
});
static IP032_AT5: LazyLock<[u32; IP032_AT5_ENTRIES]> = LazyLock::new(|| {
    u32_table(&G_A_IP032_AT5).expect("generated g_a_ip032_at5 length should be 2 u32s")
});
static IP064_AT5: LazyLock<[u32; IP064_AT5_ENTRIES]> = LazyLock::new(|| {
    u32_table(&G_A_IP064_AT5).expect("generated g_a_ip064_at5 length should be 4 u32s")
});
static IP128_AT5: LazyLock<[u32; IP128_AT5_ENTRIES]> = LazyLock::new(|| {
    u32_table(&G_A_IP128_AT5).expect("generated g_a_ip128_at5 length should be 4 u32s")
});
static IP256_AT5: LazyLock<[u32; IP256_AT5_ENTRIES]> = LazyLock::new(|| {
    u32_table(&G_A_IP256_AT5).expect("generated g_a_ip256_at5 length should be 8 u32s")
});
static SC016_AT5: LazyLock<[f32; SC016_AT5_ENTRIES]> = LazyLock::new(|| {
    f32_table(&G_A_SC016_AT5).expect("generated g_a_sc016_at5 length should be 8 f32s")
});
static SC032_AT5: LazyLock<[f32; SC032_AT5_ENTRIES]> = LazyLock::new(|| {
    f32_table(&G_A_SC032_AT5).expect("generated g_a_sc032_at5 length should be 16 f32s")
});
static SC064_AT5: LazyLock<[f32; SC064_AT5_ENTRIES]> = LazyLock::new(|| {
    f32_table(&G_A_SC064_AT5).expect("generated g_a_sc064_at5 length should be 32 f32s")
});
static SC128_AT5: LazyLock<[f32; SC128_AT5_ENTRIES]> = LazyLock::new(|| {
    f32_table(&G_A_SC128_AT5).expect("generated g_a_sc128_at5 length should be 64 f32s")
});
static SC256_AT5: LazyLock<[f32; SC256_AT5_ENTRIES]> = LazyLock::new(|| {
    f32_table(&G_A_SC256_AT5).expect("generated g_a_sc256_at5 length should be 128 f32s")
});

pub(crate) fn ip016_at5_ref() -> &'static [u32; IP016_AT5_ENTRIES] {
    &IP016_AT5
}

pub(crate) fn ip032_at5_ref() -> &'static [u32; IP032_AT5_ENTRIES] {
    &IP032_AT5
}

pub(crate) fn ip064_at5_ref() -> &'static [u32; IP064_AT5_ENTRIES] {
    &IP064_AT5
}

pub(crate) fn ip128_at5_ref() -> &'static [u32; IP128_AT5_ENTRIES] {
    &IP128_AT5
}

pub(crate) fn ip256_at5_ref() -> &'static [u32; IP256_AT5_ENTRIES] {
    &IP256_AT5
}

pub(crate) fn sc016_at5_ref() -> &'static [f32; SC016_AT5_ENTRIES] {
    &SC016_AT5
}

pub(crate) fn sc032_at5_ref() -> &'static [f32; SC032_AT5_ENTRIES] {
    &SC032_AT5
}

pub(crate) fn sc064_at5_ref() -> &'static [f32; SC064_AT5_ENTRIES] {
    &SC064_AT5
}

pub(crate) fn sc128_at5_ref() -> &'static [f32; SC128_AT5_ENTRIES] {
    &SC128_AT5
}

pub(crate) fn sc256_at5_ref() -> &'static [f32; SC256_AT5_ENTRIES] {
    &SC256_AT5
}

/// Compatibility accessor for callers that own the generated table.
pub fn sin_at5() -> [f32; SIN_AT5_ENTRIES] {
    *sin_at5_ref()
}

/// Borrow the process-wide sine table without copying or recomputing it.
pub(crate) fn sin_at5_ref() -> &'static [f32; SIN_AT5_ENTRIES] {
    &SIN_AT5
}

/// Compatibility accessor for callers that own the generated table.
pub fn win_at5() -> [f32; WIN_AT5_ENTRIES] {
    *win_at5_ref()
}

/// Borrow the process-wide window table without copying or recomputing it.
pub(crate) fn win_at5_ref() -> &'static [f32; WIN_AT5_ENTRIES] {
    &WIN_AT5
}

pub fn idspcqu_tail_count(table_index: usize) -> Option<usize> {
    let value = *G_A_IDSPCQUS_AT5.get(table_index)?;
    if value == 0xff {
        None
    } else {
        Some(usize::from(value) + 1)
    }
}

#[cfg(test)]
mod generated_table_tests {
    use super::*;

    #[test]
    fn lazy_trigonometric_tables_preserve_generated_bits_and_identity() {
        let sin = sin_at5_ref();
        let window = win_at5_ref();

        assert!(std::ptr::eq(sin, sin_at5_ref()));
        assert!(std::ptr::eq(window, win_at5_ref()));

        for (cached, generated) in sin.iter().zip(build_sin_at5()) {
            assert_eq!(cached.to_bits(), generated.to_bits());
        }
        for (cached, generated) in window.iter().zip(build_win_at5()) {
            assert_eq!(cached.to_bits(), generated.to_bits());
        }
    }

    #[test]
    fn lazy_fft_tables_preserve_generated_bits_and_identity() {
        macro_rules! check_ip {
            ($borrowed:expr, $owned:expr, $raw:expr, $len:expr) => {{
                let borrowed = $borrowed;
                assert!(std::ptr::eq(borrowed, $borrowed));
                assert_eq!(*borrowed, $owned);
                assert_eq!(*borrowed, u32_table::<$len>($raw).unwrap());
            }};
        }
        macro_rules! check_sc {
            ($borrowed:expr, $owned:expr, $raw:expr, $len:expr) => {{
                let borrowed = $borrowed;
                assert!(std::ptr::eq(borrowed, $borrowed));
                let owned = $owned;
                let generated = f32_table::<$len>($raw).unwrap();
                assert_eq!(
                    borrowed
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    owned
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    borrowed
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    generated
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>()
                );
            }};
        }

        check_ip!(ip016_at5_ref(), ip016_at5(), &G_A_IP016_AT5, 2);
        check_ip!(ip032_at5_ref(), ip032_at5(), &G_A_IP032_AT5, 2);
        check_ip!(ip064_at5_ref(), ip064_at5(), &G_A_IP064_AT5, 4);
        check_ip!(ip128_at5_ref(), ip128_at5(), &G_A_IP128_AT5, 4);
        check_ip!(ip256_at5_ref(), ip256_at5(), &G_A_IP256_AT5, 8);
        check_sc!(sc016_at5_ref(), sc016_at5(), &G_A_SC016_AT5, 8);
        check_sc!(sc032_at5_ref(), sc032_at5(), &G_A_SC032_AT5, 16);
        check_sc!(sc064_at5_ref(), sc064_at5(), &G_A_SC064_AT5, 32);
        check_sc!(sc128_at5_ref(), sc128_at5(), &G_A_SC128_AT5, 64);
        check_sc!(sc256_at5_ref(), sc256_at5(), &G_A_SC256_AT5, 128);
    }
}
