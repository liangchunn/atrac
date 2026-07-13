//! Quantization and tone-extraction constants reproduced from
//! `libatrac.so.1.2.0`.
//!
//! ## Scale-factor table (`g_a_sftbl`)
//!
//! `g_a_sftbl[i] = 2^(i/3 - 5)` for `i` in `0..64`. Index 0 = 0.03125
//! (minimum representable), index 63 = 65536.0. `scfof_id_at3` returns
//! `g_a_sftbl[id]` (or −65536.0 for `id >= 64`); `idscfof_val_at3` /
//! `idscfof_absval_at3` perform the inverse lookup.
//!
//! ## IDWL helper tables
//!
//! - `g_a_wltbl[8]`: IDWL → word-length (bits per mantissa). Index 0 = 0
//!   (no quantization), 1 = 2 bits, 2 = 3, 3 = 3, 4 = 4, 5 = 4, 6 = 5,
//!   7 = 6.
//! - `g_a_nsteps[8]`: IDWL → max quantization step (`2^wlof − 1`). Index
//!   0 = 0, 1 = 1, 2 = 2, 3 = 3, 4 = 4, 5 = 7, 6 = 15, 7 = 31.
//! - `g_a_width[8]`: IDWL → spectral width (number of bins per tone).
//!   `[1, 2, 3, 4, 5, 6, 7, 8]`.
//! - `g_a_itbgrp[16]`: ITB → group index. `[0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3]`.
//!
//! ## Huffman code tables
//!
//! `g_a_hctbl0` and `g_a_hctbl1` are flat arrays of `(code: u32, len: u32)`
//! pairs (160 entries each = 1280 bytes). They are consumed by `init_hctbl`
//! to build the runtime Huffman table: for each IDWL 1..7, `npower =
//! (1 << wlof) ^ ngrp` codes are copied. The two tables correspond to
//! different codebooks (table 0 and table 1), selected per-tone by the
//! `table_idx` field.
//!
//! `g_a_ngrp_for_tone` / `g_a_npks_for_tone` (16 `i32` each) control the
//! Huffman table construction for the tone path. `g_a_ngrp_for_tone[1..8]
//! = [2,1,1,1,1,1,1]` → IDWL 1 uses pair-coding (mode 2), IDWL 2..7 use
//! single-coding (mode 1).

#![allow(clippy::excessive_precision)]

/// Scale-factor table (`g_a_sftbl` at `0xC4940`, 64 `f32`).
///
/// `g_a_sftbl[i] = 2^(i/3 - 5)`. Used by `scfof_id_at3` / `idscfof_val_at3`.
pub const SCALE_FACTOR_TABLE: [f32; 64] = [
    0.03125,
    0.039372533559799194,
    0.0496062827706337,
    0.0625,
    0.07874506711959839,
    0.0992125655412674,
    0.125,
    0.15749013423919678,
    0.1984251310825348,
    0.25,
    0.31498026847839355,
    0.3968502621650696,
    0.5,
    0.6299605369567871,
    0.7937005162239075,
    1.0,
    1.2599210739135742,
    1.5874010324478149,
    2.0,
    2.5198421478271484,
    3.1748020648956299,
    4.0,
    5.0396842956542969,
    6.3496041297912598,
    8.0,
    10.079368591308594,
    12.69920825958252,
    16.0,
    20.158737182617188,
    25.398416519165039,
    32.0,
    40.317474365234375,
    50.796833038330078,
    64.0,
    80.63494873046875,
    101.59366607666016,
    128.0,
    161.2698974609375,
    203.18733215332031,
    256.0,
    322.539794921875,
    406.37466430664062,
    512.0,
    645.07958984375,
    812.74932861328125,
    1024.0,
    1290.1591796875,
    1625.4986572265625,
    2048.0,
    2580.318359375,
    3250.997314453125,
    4096.0,
    5160.63671875,
    6501.99462890625,
    8192.0,
    10321.2734375,
    13003.9892578125,
    16384.0,
    20642.546875,
    26007.978515625,
    32768.0,
    41285.09375,
    52015.95703125,
    65536.0,
];

/// IDWL → word-length (`g_a_wltbl` at `0xC4860`, 8 `i32`).
///
/// `wlof_idwl_at3(idwl)` returns `g_a_wltbl[idwl]` (or −1 for `idwl >= 8`).
pub const WORD_LENGTH_TABLE: [i32; 8] = [0, 2, 3, 3, 4, 4, 5, 6];

/// IDWL → max quantization step (`g_a_nsteps` at `0xC4880`, 8 `i32`).
///
/// `nstepsof_idwl_at3(idwl)` returns `g_a_nsteps[idwl]` (or −1).
/// `g_a_nsteps[i] = 2^g_a_wltbl[i] - 1` (except index 0 which is 0).
pub const NSTEPS_TABLE: [i32; 8] = [0, 1, 2, 3, 4, 7, 15, 31];

/// IDWL → spectral width (`g_a_width` at `0xC48E0`, 8 `i32`).
///
/// `twidof_id_at3(idwl)` returns `g_a_width[idwl]` (or −1). This is the
/// number of spectral coefficients per tone component.
pub const WIDTH_TABLE: [i32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

/// ITB → group index (`g_a_itbgrp` at `0xC48A0`, 16 `i32`).
///
/// `itbgrpof_itb_at3(itb)` returns `g_a_itbgrp[itb]` (or −1). Groups 4
/// consecutive ITBs together: `[0,0,0,0, 1,1,1,1, 2,2,2,2, 3,3,3,3]`.
pub const ITB_GROUP_TABLE: [i32; 16] = [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3];

/// Context array for `translate_to_idwl` — non-attack path (`a_maskh` at `0xC4AA0`).
///
/// 8 `i32` values. Passed as the `ctx` parameter when `bVar4 == false`.
pub const CTX_A_MASKH: [i32; 8] = [30, 30, 30, 30, 30, 30, 30, 30];

/// Context array for `translate_to_idwl` — attack/single-tone path (`a_masks` at `0xC4AC0`).
///
/// 8 `i32` values. Passed as the `ctx` parameter when `bVar4 == true`.
pub const CTX_A_MASKS: [i32; 8] = [60, 60, 60, 60, 60, 60, 60, 60];

/// Initial IDWL ceiling per ITFB group, zeroed by `set_idtf_and_limwl`
/// and then adjusted adaptively by the convergence loop.
pub const ITFB_IDWL_CEILING_INIT: [i32; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

/// CLC bit-length table (`aa_cbitlen` at `0xC4A60`, 16 `i32`).
///
/// Used by `extract_multitone` for bit-cost accounting.
pub const CLC_BIT_LENGTH_TABLE: [i32; 16] = [0, 3, 3, 4, 5, 6, 7, 8, 0, 2, 3, 3, 4, 4, 5, 6];

/// Tone-frequency constants (`s_a_const` at `0xC4BE0`, 13 `f32`).
///
/// `s_a_const[i] = 2^(i/12)` for `i` in `0..13` (one octave of semitone
/// ratios). Used by `tfof_id` (deferred to milestone #7).
pub const TONE_FREQ_CONST: [f32; 13] = [
    1.0, 1.0592537, 1.1220182, 1.1885022, 1.2589254, 1.3335214, 1.4125376, 1.4962357, 1.5848932,
    1.6788039, 1.7782795, 1.8836488, 1.9952621,
];

/// Tone-frequency divisors (`s_a_divide` at `0xC4C20`, 8 `f32`).
///
/// Used by `tfof_id` (deferred to milestone #7). `[1, 3, 5, 7, 9, 15, 31, 63]`.
pub const TONE_FREQ_DIVIDE: [f32; 8] = [1.0, 3.0, 5.0, 7.0, 9.0, 15.0, 31.0, 63.0];

/// Number of Huffman code groups per IDWL for the tone path
/// (`g_a_ngrp_for_tone` at `0xC4D40`, 16 `i32`).
///
/// Only indices 1..8 are used. Index 0 and 8 are padding (0).
pub const NGRP_FOR_TONE: [i32; 16] = [0, 2, 1, 1, 1, 1, 1, 1, 0, 2, 1, 1, 1, 1, 1, 1];

/// Number of peak groups per IDWL for the tone path
/// (`g_a_npks_for_tone` at `0xC4CC0`, 16 `i32`).
pub const NPKS_FOR_TONE: [i32; 16] = [0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1];

/// Number of Huffman code groups per IDWL for the non-tone (spec) path
/// (`g_a_ngrp_for_spec` at `0xC4D00`, 16 `i32`).
pub const NGRP_FOR_SPEC: [i32; 16] = [0, 2, 1, 1, 1, 1, 1, 1, 0, 2, 1, 1, 1, 1, 1, 1];

/// Number of peak groups per IDWL for the non-tone (spec) path
/// (`g_a_npks_for_spec` at `0xC4C80`, 16 `i32`).
pub const NPKS_FOR_SPEC: [i32; 16] = [0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1];

/// Huffman code/length pairs for tone codebook 0 (`g_a_hctbl0` at `0xC4D80`).
///
/// 160 `(code: u32, len: u32)` pairs, consumed by `init_hctbl` in IDWL
/// order 1..7 with counts `[16, 8, 8, 16, 16, 32, 64]`.
pub const HCTBL0_CODES: [(u32, u32); 160] = [
    (0, 1),
    (4, 3),
    (0, 0),
    (5, 3),
    (12, 4),
    (28, 5),
    (0, 0),
    (29, 5),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (13, 4),
    (30, 5),
    (0, 0),
    (31, 5),
    (0, 1),
    (4, 3),
    (6, 3),
    (0, 0),
    (0, 0),
    (0, 0),
    (7, 3),
    (5, 3),
    (0, 1),
    (4, 3),
    (12, 4),
    (14, 4),
    (0, 0),
    (15, 4),
    (13, 4),
    (5, 3),
    (0, 1),
    (4, 3),
    (12, 4),
    (28, 5),
    (30, 5),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (31, 5),
    (29, 5),
    (13, 4),
    (5, 3),
    (0, 2),
    (2, 3),
    (8, 4),
    (10, 4),
    (28, 5),
    (60, 6),
    (62, 6),
    (12, 4),
    (0, 0),
    (13, 4),
    (63, 6),
    (61, 6),
    (29, 5),
    (11, 4),
    (9, 4),
    (3, 3),
    (0, 3),
    (2, 4),
    (4, 4),
    (6, 4),
    (20, 5),
    (22, 5),
    (24, 5),
    (52, 6),
    (54, 6),
    (56, 6),
    (58, 6),
    (120, 7),
    (122, 7),
    (124, 7),
    (126, 7),
    (8, 4),
    (0, 0),
    (9, 4),
    (127, 7),
    (125, 7),
    (123, 7),
    (121, 7),
    (59, 6),
    (57, 6),
    (55, 6),
    (53, 6),
    (25, 5),
    (23, 5),
    (21, 5),
    (7, 4),
    (5, 4),
    (3, 4),
    (0, 3),
    (8, 5),
    (10, 5),
    (12, 5),
    (14, 5),
    (16, 5),
    (36, 6),
    (38, 6),
    (40, 6),
    (42, 6),
    (44, 6),
    (46, 6),
    (48, 6),
    (50, 6),
    (104, 7),
    (106, 7),
    (108, 7),
    (110, 7),
    (112, 7),
    (114, 7),
    (116, 7),
    (236, 8),
    (238, 8),
    (240, 8),
    (242, 8),
    (244, 8),
    (246, 8),
    (248, 8),
    (250, 8),
    (252, 8),
    (254, 8),
    (2, 4),
    (0, 0),
    (3, 4),
    (255, 8),
    (253, 8),
    (251, 8),
    (249, 8),
    (247, 8),
    (245, 8),
    (243, 8),
    (241, 8),
    (239, 8),
    (237, 8),
    (117, 7),
    (115, 7),
    (113, 7),
    (111, 7),
    (109, 7),
    (107, 7),
    (105, 7),
    (51, 6),
    (49, 6),
    (47, 6),
    (45, 6),
    (43, 6),
    (41, 6),
    (39, 6),
    (37, 6),
    (17, 5),
    (15, 5),
    (13, 5),
    (11, 5),
    (9, 5),
];

/// Huffman code/length pairs for tone codebook 1 (`g_a_hctbl1` at `0xC5280`).
///
/// 160 `(code: u32, len: u32)` pairs. Codebook 1 uses flat 4-bit or 6-bit
/// codes (no variable-length optimization).
pub const HCTBL1_CODES: [(u32, u32); 160] = [
    (0, 4),
    (1, 4),
    (0, 0),
    (3, 4),
    (4, 4),
    (5, 4),
    (0, 0),
    (7, 4),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (12, 4),
    (13, 4),
    (0, 0),
    (15, 4),
    (0, 3),
    (1, 3),
    (2, 3),
    (0, 0),
    (0, 0),
    (0, 0),
    (6, 3),
    (7, 3),
    (0, 3),
    (1, 3),
    (2, 3),
    (3, 3),
    (0, 0),
    (5, 3),
    (6, 3),
    (7, 3),
    (0, 4),
    (1, 4),
    (2, 4),
    (3, 4),
    (4, 4),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (12, 4),
    (13, 4),
    (14, 4),
    (15, 4),
    (0, 4),
    (1, 4),
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 4),
    (6, 4),
    (7, 4),
    (0, 0),
    (9, 4),
    (10, 4),
    (11, 4),
    (12, 4),
    (13, 4),
    (14, 4),
    (15, 4),
    (0, 5),
    (1, 5),
    (2, 5),
    (3, 5),
    (4, 5),
    (5, 5),
    (6, 5),
    (7, 5),
    (8, 5),
    (9, 5),
    (10, 5),
    (11, 5),
    (12, 5),
    (13, 5),
    (14, 5),
    (15, 5),
    (0, 0),
    (17, 5),
    (18, 5),
    (19, 5),
    (20, 5),
    (21, 5),
    (22, 5),
    (23, 5),
    (24, 5),
    (25, 5),
    (26, 5),
    (27, 5),
    (28, 5),
    (29, 5),
    (30, 5),
    (31, 5),
    (0, 6),
    (1, 6),
    (2, 6),
    (3, 6),
    (4, 6),
    (5, 6),
    (6, 6),
    (7, 6),
    (8, 6),
    (9, 6),
    (10, 6),
    (11, 6),
    (12, 6),
    (13, 6),
    (14, 6),
    (15, 6),
    (16, 6),
    (17, 6),
    (18, 6),
    (19, 6),
    (20, 6),
    (21, 6),
    (22, 6),
    (23, 6),
    (24, 6),
    (25, 6),
    (26, 6),
    (27, 6),
    (28, 6),
    (29, 6),
    (30, 6),
    (31, 6),
    (0, 0),
    (33, 6),
    (34, 6),
    (35, 6),
    (36, 6),
    (37, 6),
    (38, 6),
    (39, 6),
    (40, 6),
    (41, 6),
    (42, 6),
    (43, 6),
    (44, 6),
    (45, 6),
    (46, 6),
    (47, 6),
    (48, 6),
    (49, 6),
    (50, 6),
    (51, 6),
    (52, 6),
    (53, 6),
    (54, 6),
    (55, 6),
    (56, 6),
    (57, 6),
    (58, 6),
    (59, 6),
    (60, 6),
    (61, 6),
    (62, 6),
    (63, 6),
];

/// Number of Huffman codes per IDWL (1..7), derived from
/// `npower = (1 << wlof) ^ ngrp`.
///
/// `HUFF_COUNTS_PER_IDWL[i-1]` = code count for IDWL `i`.
/// `[16, 8, 8, 16, 16, 32, 64]`.
pub const HUFF_COUNTS_PER_IDWL: [usize; 7] = [16, 8, 8, 16, 16, 32, 64];

/// Quantization table start positions (`g_a_qtstart` at `0xC47C0`, 33 `i32`).
///
/// `ispof_iqt_at3(bfu)` returns `g_a_qtstart[bfu]` for `bfu < 33`, else −1.
/// Maps BFU index → cumulative spectral position. Values are
/// `0, 8, 16, ..., 1024`.
pub const QTSTART_TABLE: [i32; 33] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 288, 320,
    352, 384, 416, 448, 480, 512, 576, 640, 704, 768, 896, 1024,
];

/// Number of spectral samples per BFU (`g_a_nsps1024` at `0xC4740`, 32 `i32`).
///
/// `nsps_inqt_at3(bfu)` returns `g_a_nsps1024[bfu]` for `bfu < 32`, else −1.
/// BFU sizes: 8×8, 8×16, 10×32, 4×64, 3×128.
pub const NSPS1024_TABLE: [i32; 32] = [
    8, 8, 8, 8, 8, 8, 8, 8, 16, 16, 16, 16, 16, 16, 16, 16, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32,
    64, 64, 64, 64, 128, 128,
];
