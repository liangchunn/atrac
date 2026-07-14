use crate::entropy::huffman::huffman_entry;
use crate::tables::at5::{
    IDCT_FIXBITS_AT5_ENTRIES, idct_fixbits_at5, n2_under128_at5, sg_shape_index_at5,
};
use crate::tables::huffman::{
    HuffmanDescriptor, ct_a, ct_b, ct_c, ct_d, sfc_descriptors, sfc_sg_descriptors, wlc_descriptors,
};

pub const IDCT_BAND_LIMIT_AT5: usize = 32;
pub const IDSF_BAND_LIMIT_AT5: usize = 32;
pub const IDWL_BAND_LIMIT_AT5: usize = 32;
pub const VAR_REBITALLOC_SELECTOR_STRIDE_AT5: usize = 8;

const IDSF_SHIFTED_ROWS_AT5: usize = 3;
const IDSF_COMPACT_GROUP_LIMIT_AT5: usize = 10;
const IDSF_COMPACT_CODEBOOK_ROWS: usize = 64;
const IDSF_COMPACT_CODEBOOK_COLUMNS: usize = 9;
const IDSF_THRESHOLD_AT5: [i32; 6] = [0, 1, 3, 7, 15, 31];
const IDWL_ROW_COUNT_AT5: usize = 4;
const IDWL_MODE_COUNT_AT5: usize = 2;
const IDWL_COEF_ROWS_AT5: usize = 3;
const IDWL_SG_GROUP_LIMIT_AT5: usize = 10;
const IDWL_SG_ROW_WORDS_AT5: usize = 35;
/// Smallest native ATRAC3plus band extent (48 kbps -> 26). The "reduced" IDWL
/// shapes whose group count over-covers the processing word_count only occur at
/// or above this extent; smaller law-consistent pairs (e.g. 2/1) never appear in
/// native cfg and stay typed-rejected.
const IDWL_MIN_BAND_EXTENT_AT5: usize = 26;
const IDWL_SG_CODEBOOK_BASES: usize = 8;
const IDWL_SG_CODEBOOK_SELECTORS: usize = 16;
const IDWL_SG_CODEBOOK_COLUMNS: usize = 9;
const IDWL_COST_LIMIT_AT5: i32 = 0x4000;
const IDWL_THRESHOLD_AT5: [i32; 3] = [0, 1, 3];
const VAR_REBITALLOC_COST_LIMIT_AT5: i32 = 0x4000;

// Native symbol g_aaa_wlc_coef_at5, offset 0x000893a0, size 192 bytes.
const IDWL_WLC_COEF_AT5: [[[i8; IDWL_BAND_LIMIT_AT5]; IDWL_COEF_ROWS_AT5]; IDWL_MODE_COUNT_AT5] = [
    [
        [
            5, 5, 4, 4, 3, 3, 2, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
        [
            5, 5, 5, 4, 4, 4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
        [
            6, 5, 5, 5, 4, 4, 4, 4, 3, 3, 3, 3, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0,
            0, 0, 0,
        ],
    ],
    [
        [
            5, 5, 4, 4, 3, 3, 2, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
        [
            5, 5, 5, 4, 4, 4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
        [
            6, 5, 5, 5, 5, 5, 5, 5, 3, 3, 3, 3, 2, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
    ],
];

// Native symbol g_aaa_wlc_sg_cb_at5, offset 0x00089460, size 1152 bytes.
const IDWL_WLC_SG_CODEBOOK_AT5: [[[i8; IDWL_SG_CODEBOOK_COLUMNS]; IDWL_SG_CODEBOOK_SELECTORS];
    IDWL_SG_CODEBOOK_BASES] = [
    [
        [0, 0, 0, 0, 0, 0, 0, -2, -1],
        [0, 0, 0, 0, 0, 0, 0, -5, -1],
        [0, 0, 0, -7, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, -7, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, -5, 0, 0],
        [0, 0, 0, 0, -5, 0, 0, 0, 0],
        [-7, -7, 0, 0, 0, 0, 0, 0, 0],
        [0, -7, 0, 0, 0, 0, 0, 0, 0],
        [-2, -2, -5, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, -2, -5, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, -2, -5, 0, 0],
        [0, 0, 0, -5, 0, 0, 0, 0, 0],
        [0, -2, -7, -2, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, -2, -5, 0, 0, 0],
        [0, 0, 0, -5, -5, 0, 0, 0, 0],
        [0, 0, 0, -5, -2, 0, 0, 0, 0],
    ],
    [
        [-1, -5, -3, -2, -1, -1, 0, 0, 0],
        [-2, -5, -3, -3, -2, -1, -1, 0, 0],
        [0, -1, -1, -1, 0, 0, 0, 0, 0],
        [-1, -3, 0, 0, 0, 0, 0, 0, 0],
        [-1, -2, 0, 0, 0, 0, 0, 0, 0],
        [-1, -3, -1, 0, 0, 0, 0, 1, 1],
        [-1, -5, -3, -3, -2, -1, 0, 0, 0],
        [-1, -1, -4, -2, -2, -1, -1, 0, 0],
        [-1, -1, -3, -2, -3, -1, -1, -1, 0],
        [-1, -4, -2, -3, -1, 0, 0, 0, 0],
        [0, -1, -2, -2, -1, -1, 0, 0, 0],
        [0, -2, -1, 0, 0, 0, 0, 0, 0],
        [-1, -1, 0, 0, 0, 0, 0, 0, 0],
        [-1, -1, -3, -2, -2, -1, -1, -1, 0],
        [0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, -1, -3, -2, -2, -1, -1, -1, 0],
    ],
    [
        [-1, -2, 0, 1, 1, 1, 1, 1, 1],
        [0, -1, 1, 1, 1, 1, 1, 1, 1],
        [0, -2, 1, 1, 1, 1, 1, 1, 1],
        [0, -2, 0, 1, 1, 1, 1, 1, 1],
        [-1, -1, 0, 1, 1, 1, 1, 1, 1],
        [0, 0, -1, 0, 1, 1, 1, 1, 1],
        [-1, -1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, -1, 1, 1, 1, 1, 1, 1],
        [0, -1, 0, 1, 1, 1, 1, 1, 1],
        [-1, -1, -1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 0, 1, 1, 1, 1, 1],
        [0, 0, 0, 1, 1, 1, 1, 1, 1],
        [0, -1, -1, 1, 1, 1, 1, 1, 1],
        [0, 1, 0, 1, 1, 1, 1, 1, 1],
        [0, -3, -2, 1, 1, 1, 1, 2, 2],
        [-3, -5, -3, 2, 2, 2, 2, 2, 2],
    ],
    [
        [-1, -2, 0, 2, 2, 2, 2, 2, 2],
        [-1, -2, 0, 1, 2, 2, 2, 2, 2],
        [0, -2, 0, 2, 2, 2, 2, 2, 2],
        [-1, 0, 1, 2, 2, 2, 2, 2, 2],
        [0, 0, 1, 2, 2, 2, 2, 2, 2],
        [0, -2, 0, 1, 2, 2, 2, 2, 2],
        [0, -1, 1, 2, 2, 2, 2, 2, 2],
        [-1, -1, 0, 2, 2, 2, 2, 2, 2],
        [-1, -1, 0, 1, 2, 2, 2, 2, 2],
        [-1, -2, -1, 2, 2, 2, 2, 2, 2],
        [0, -1, 0, 2, 2, 2, 2, 2, 2],
        [1, 1, 0, 1, 2, 2, 2, 2, 2],
        [0, 1, 2, 2, 2, 2, 2, 2, 2],
        [1, 0, 0, 1, 2, 2, 2, 2, 2],
        [0, 0, 0, 1, 2, 2, 2, 2, 2],
        [-1, -1, -1, 1, 2, 2, 2, 2, 2],
    ],
    [
        [0, 1, 2, 3, 3, 3, 3, 3, 3],
        [1, 1, 2, 3, 3, 3, 3, 3, 3],
        [-1, 0, 1, 2, 3, 3, 3, 3, 3],
        [0, 0, 2, 3, 3, 3, 3, 3, 3],
        [-1, 0, 1, 3, 3, 3, 3, 3, 3],
        [0, 0, 1, 3, 3, 3, 3, 3, 3],
        [1, 2, 3, 3, 3, 3, 3, 3, 3],
        [1, 2, 2, 3, 3, 3, 3, 3, 3],
        [0, 1, 1, 3, 3, 3, 3, 3, 3],
        [0, 0, 1, 2, 3, 3, 3, 3, 3],
        [-1, 1, 2, 3, 3, 3, 3, 3, 3],
        [-1, 0, 2, 3, 3, 3, 3, 3, 3],
        [2, 2, 3, 3, 3, 3, 3, 3, 3],
        [1, 1, 3, 3, 3, 3, 3, 3, 3],
        [0, 2, 3, 3, 3, 3, 3, 3, 3],
        [0, 1, 1, 2, 3, 3, 3, 3, 3],
    ],
    [
        [0, 1, 2, 3, 4, 4, 4, 4, 4],
        [1, 2, 3, 4, 4, 4, 4, 4, 4],
        [0, 0, 2, 3, 4, 4, 4, 4, 4],
        [1, 1, 2, 4, 4, 4, 4, 4, 4],
        [0, 1, 2, 4, 4, 4, 4, 4, 4],
        [-1, 0, 1, 3, 4, 4, 4, 4, 4],
        [0, 0, 1, 3, 4, 4, 4, 4, 4],
        [1, 1, 2, 3, 4, 4, 4, 4, 4],
        [0, 1, 1, 3, 4, 4, 4, 4, 4],
        [2, 2, 3, 4, 4, 4, 4, 4, 4],
        [1, 1, 3, 4, 4, 4, 4, 4, 4],
        [1, 2, 2, 4, 4, 4, 4, 4, 4],
        [-1, 0, 2, 3, 4, 4, 4, 4, 4],
        [0, 1, 3, 4, 4, 4, 4, 4, 4],
        [1, 2, 2, 3, 4, 4, 4, 4, 4],
        [0, 2, 3, 4, 4, 4, 4, 4, 4],
    ],
    [
        [1, 2, 3, 4, 5, 5, 5, 5, 5],
        [0, 1, 2, 3, 4, 5, 5, 5, 5],
        [0, 1, 2, 3, 5, 5, 5, 5, 5],
        [1, 1, 3, 4, 5, 5, 5, 5, 5],
        [1, 1, 2, 4, 5, 5, 5, 5, 5],
        [1, 2, 2, 4, 5, 5, 5, 5, 5],
        [1, 1, 2, 3, 5, 5, 5, 5, 5],
        [2, 2, 3, 4, 5, 5, 5, 5, 5],
        [0, 1, 2, 4, 5, 5, 5, 5, 5],
        [2, 2, 3, 5, 5, 5, 5, 5, 5],
        [1, 2, 3, 5, 5, 5, 5, 5, 5],
        [0, 1, 3, 4, 5, 5, 5, 5, 5],
        [1, 2, 2, 3, 5, 5, 5, 5, 5],
        [2, 3, 4, 5, 5, 5, 5, 5, 5],
        [0, 2, 3, 4, 5, 5, 5, 5, 5],
        [1, 1, 1, 3, 4, 5, 5, 5, 5],
    ],
    [
        [1, 2, 3, 4, 5, 5, 5, 6, 6],
        [1, 2, 3, 4, 5, 6, 6, 6, 6],
        [2, 3, 4, 5, 6, 6, 6, 6, 6],
        [1, 2, 3, 4, 6, 6, 6, 6, 6],
        [2, 2, 3, 4, 5, 5, 5, 6, 6],
        [1, 2, 3, 4, 5, 5, 6, 6, 6],
        [2, 2, 3, 4, 6, 6, 6, 6, 6],
        [2, 2, 3, 4, 5, 6, 6, 6, 6],
        [2, 2, 4, 5, 6, 6, 6, 6, 6],
        [2, 2, 3, 5, 6, 6, 6, 6, 6],
        [1, 2, 3, 5, 6, 6, 6, 6, 6],
        [2, 3, 3, 5, 6, 6, 6, 6, 6],
        [1, 2, 4, 5, 6, 6, 6, 6, 6],
        [2, 2, 3, 4, 5, 5, 6, 6, 6],
        [2, 3, 3, 4, 6, 6, 6, 6, 6],
        [1, 3, 4, 5, 6, 6, 6, 6, 6],
    ],
];

// Native symbol g_aa_sfc_sg_cb_at5, offset 0x000b9cc0, size 576 bytes.
const IDSF_SFC_SG_CODEBOOK_AT5: [[i8; IDSF_COMPACT_CODEBOOK_COLUMNS]; IDSF_COMPACT_CODEBOOK_ROWS] = [
    [-3, -2, -1, 0, 3, 5, 6, 8, 40],
    [-3, -2, 0, 1, 7, 9, 11, 13, 20],
    [-1, 0, 0, 1, 6, 8, 10, 13, 41],
    [0, 0, 0, 2, 5, 5, 6, 8, 14],
    [0, 0, 0, 2, 6, 7, 8, 11, 47],
    [0, 0, 1, 2, 5, 7, 8, 10, 32],
    [0, 0, 1, 3, 8, 10, 12, 14, 47],
    [0, 0, 2, 4, 9, 10, 12, 14, 40],
    [0, 0, 3, 5, 9, 10, 12, 14, 22],
    [0, 1, 3, 5, 10, 14, 18, 22, 31],
    [0, 2, 5, 6, 10, 10, 10, 12, 46],
    [0, 2, 5, 7, 12, 14, 15, 18, 44],
    [1, 1, 4, 5, 7, 7, 8, 9, 15],
    [1, 2, 2, 2, 4, 5, 7, 9, 26],
    [1, 2, 2, 3, 6, 7, 7, 8, 47],
    [1, 2, 2, 3, 6, 8, 10, 13, 22],
    [1, 3, 4, 7, 13, 17, 21, 24, 41],
    [1, 4, 0, 4, 10, 12, 13, 14, 17],
    [2, 3, 3, 3, 6, 8, 10, 13, 48],
    [2, 3, 3, 4, 9, 12, 14, 17, 47],
    [2, 3, 3, 5, 10, 12, 14, 17, 25],
    [2, 3, 5, 7, 8, 9, 9, 9, 13],
    [2, 3, 5, 9, 16, 21, 25, 28, 33],
    [2, 4, 5, 8, 12, 14, 17, 19, 26],
    [2, 4, 6, 8, 12, 13, 13, 15, 20],
    [2, 4, 7, 12, 20, 26, 30, 32, 35],
    [3, 3, 5, 6, 12, 14, 16, 19, 34],
    [3, 4, 4, 5, 7, 9, 10, 11, 48],
    [3, 4, 5, 6, 8, 9, 10, 11, 16],
    [3, 5, 5, 5, 7, 9, 10, 13, 35],
    [3, 5, 5, 7, 10, 12, 13, 15, 49],
    [3, 5, 7, 7, 8, 7, 9, 12, 21],
    [3, 5, 7, 8, 12, 14, 15, 15, 24],
    [3, 5, 7, 10, 16, 21, 24, 27, 44],
    [3, 5, 8, 14, 21, 26, 28, 29, 42],
    [3, 6, 10, 13, 18, 19, 20, 22, 27],
    [3, 6, 11, 16, 24, 27, 28, 29, 31],
    [4, 5, 4, 3, 4, 6, 8, 11, 18],
    [4, 6, 5, 6, 9, 10, 12, 14, 20],
    [4, 6, 7, 6, 6, 6, 7, 8, 46],
    [4, 6, 7, 9, 13, 16, 18, 20, 48],
    [4, 6, 7, 9, 14, 17, 20, 23, 31],
    [4, 6, 9, 11, 14, 15, 15, 17, 21],
    [4, 8, 13, 20, 27, 32, 35, 36, 38],
    [5, 6, 6, 4, 5, 6, 7, 6, 6],
    [5, 7, 7, 8, 9, 9, 10, 12, 49],
    [5, 8, 9, 9, 10, 11, 12, 13, 42],
    [5, 8, 10, 12, 15, 16, 17, 19, 42],
    [5, 8, 12, 17, 26, 31, 32, 33, 44],
    [5, 9, 13, 16, 20, 22, 23, 23, 35],
    [6, 8, 8, 7, 6, 5, 6, 8, 15],
    [6, 8, 8, 8, 9, 10, 12, 16, 24],
    [6, 8, 8, 9, 10, 10, 11, 11, 13],
    [6, 8, 10, 13, 19, 21, 24, 26, 32],
    [6, 9, 10, 11, 13, 13, 14, 16, 49],
    [7, 9, 9, 10, 13, 14, 16, 19, 27],
    [7, 10, 12, 13, 16, 16, 17, 17, 27],
    [7, 10, 12, 14, 17, 19, 20, 22, 48],
    [8, 9, 10, 9, 10, 11, 11, 11, 19],
    [8, 11, 12, 12, 13, 13, 13, 13, 17],
    [8, 11, 13, 14, 16, 17, 19, 20, 27],
    [8, 12, 17, 22, 26, 28, 29, 30, 33],
    [10, 14, 16, 19, 21, 22, 22, 24, 28],
    [10, 15, 17, 18, 21, 22, 23, 25, 43],
];

#[derive(Debug, Clone, Copy)]
pub struct IdctChannelState<'a> {
    pub mode: u32,
    pub bandwidth_mode: usize,
    pub band_count: usize,
    pub idct_source: &'a [u32],
    pub previous_idct_source: &'a [u32],
}

#[derive(Debug, Clone, Copy)]
pub struct IdsfChannelState<'a> {
    pub mode: u32,
    pub band_count: usize,
    pub group_count: usize,
    pub scale_factors: &'a [i32],
    pub previous_scale_factors: &'a [i32],
}

#[derive(Debug, Clone, Copy)]
pub struct IdwlChannelState<'a> {
    pub mode: u32,
    pub context_kind: u32,
    pub word_count: usize,
    pub group_count: usize,
    pub word_lengths: &'a [i32],
    pub previous_word_lengths: &'a [i32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdsfBlockState {
    pub mode: u32,
    pub start: usize,
    pub count: usize,
    pub field_0x1c748: i32,
    pub huffman_selector: usize,
    pub mode_selector: usize,
    pub codebook_selector: usize,
    pub compact_base: i32,
    pub shifted_rows: [[i32; IDSF_BAND_LIMIT_AT5]; IDSF_SHIFTED_ROWS_AT5],
    pub transformed: [i32; IDSF_BAND_LIMIT_AT5],
}

impl Default for IdsfBlockState {
    fn default() -> Self {
        Self {
            mode: 0,
            start: 0,
            count: 0,
            field_0x1c748: 0,
            huffman_selector: 0,
            mode_selector: 0,
            codebook_selector: 0,
            compact_base: 0,
            shifted_rows: [[0; IDSF_BAND_LIMIT_AT5]; IDSF_SHIFTED_ROWS_AT5],
            transformed: [0; IDSF_BAND_LIMIT_AT5],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdwlSideState {
    pub window_fields: [i32; 3],
    pub subgroup_flag: i32,
    pub compact: [i32; IDWL_SG_GROUP_LIMIT_AT5],
    pub codebook: [i32; IDWL_SG_GROUP_LIMIT_AT5],
    pub rows: [[i32; IDWL_SG_ROW_WORDS_AT5]; IDWL_ROW_COUNT_AT5],
}

impl Default for IdwlSideState {
    fn default() -> Self {
        Self {
            window_fields: [0; 3],
            subgroup_flag: 0,
            compact: [0; IDWL_SG_GROUP_LIMIT_AT5],
            codebook: [0; IDWL_SG_GROUP_LIMIT_AT5],
            rows: [[0; IDWL_SG_ROW_WORDS_AT5]; IDWL_ROW_COUNT_AT5],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdwlBlockState {
    pub mode: u32,
    pub costs: [i32; 4],
    pub selector_fields_14_24: [i32; 5],
    pub selector_fields_28_38: [i32; 5],
    pub selector_fields_3c_4c: [i32; 5],
    pub selector_fields_50_60: [i32; 5],
    pub valid_flags: [i32; IDWL_ROW_COUNT_AT5],
    pub word_rows: [[i32; IDWL_BAND_LIMIT_AT5]; IDWL_ROW_COUNT_AT5],
    pub active_counts: [[i32; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5],
    pub duplicate_indices: [[i32; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5],
    pub tail_counts: [i32; IDWL_ROW_COUNT_AT5],
    pub side: IdwlSideState,
}

impl Default for IdwlBlockState {
    fn default() -> Self {
        Self {
            mode: 0,
            costs: [0; 4],
            selector_fields_14_24: [0; 5],
            selector_fields_28_38: [0; 5],
            selector_fields_3c_4c: [0; 5],
            selector_fields_50_60: [0; 5],
            valid_flags: [0; IDWL_ROW_COUNT_AT5],
            word_rows: [[0; IDWL_BAND_LIMIT_AT5]; IDWL_ROW_COUNT_AT5],
            active_counts: [[0; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5],
            duplicate_indices: [[0; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5],
            tail_counts: [0; IDWL_ROW_COUNT_AT5],
            side: IdwlSideState::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdsfWindowFields {
    start: usize,
    bits: usize,
    base: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdsfMode1Choice {
    bits: i32,
    fields: IdsfWindowFields,
    // Winning mode-1 sub index (0..2 = shifted rows, 3 = compact transformed plane).
    // Native `local_8c[10]`, written in the sub-select loop at decompile 35634-35643;
    // consumed by the mode-1 tail store to object word 0x1c750 (decompile 35834).
    mode_selector: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdsfMode3Choice {
    bits: i32,
    mode_selector: usize,
    huffman_selector: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdwlWindowFields {
    start: usize,
    bits: usize,
    base: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdwlWlcChoice {
    bits: i32,
    selector: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdctBlockState {
    pub mode: u32,
    pub band_count: usize,
    pub split_flag: u32,
    pub flags: [u32; IDCT_BAND_LIMIT_AT5],
    pub aux: [u32; IDCT_BAND_LIMIT_AT5],
    pub previous: [u32; IDCT_BAND_LIMIT_AT5],
}

impl Default for IdctBlockState {
    fn default() -> Self {
        Self {
            mode: 0,
            band_count: 0,
            split_flag: 0,
            flags: [0; IDCT_BAND_LIMIT_AT5],
            aux: [0; IDCT_BAND_LIMIT_AT5],
            previous: [0; IDCT_BAND_LIMIT_AT5],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdctCost {
    mode: u32,
    split_flag: u32,
    bits: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct VarRebitallocInput<'a> {
    pub quant_unit: usize,
    pub channel_index: usize,
    pub channel_count: usize,
    pub old_selector: usize,
    pub selector_count: usize,
    pub current_idct_bits: i32,
    pub source_costs: &'a [i16],
    pub target_costs: &'a [i16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarRebitallocResult {
    pub bit_delta: i32,
    pub word_length: u32,
    pub idct_bits: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitcountError {
    EmptyChannels,
    ChannelBlockCountMismatch {
        channels: usize,
        blocks: usize,
    },
    UnsupportedIdctSelector {
        selector: u32,
        max_supported: u32,
    },
    BandCountTooLarge {
        channel: usize,
        count: usize,
        max: usize,
    },
    BandwidthModeOutOfRange {
        channel: usize,
        mode: usize,
        max: usize,
    },
    IdctSourceTooShort {
        channel: usize,
        needed: usize,
        actual: usize,
    },
    PreviousIdctSourceTooShort {
        channel: usize,
        needed: usize,
        actual: usize,
    },
    IdsfScaleFactorsTooShort {
        needed: usize,
        actual: usize,
    },
    PreviousIdsfScaleFactorsTooShort {
        needed: usize,
        actual: usize,
    },
    IdwlWordLengthsTooShort {
        needed: usize,
        actual: usize,
    },
    PreviousIdwlWordLengthsTooShort {
        needed: usize,
        actual: usize,
    },
    IdwlWordCountTooLarge {
        count: usize,
        max: usize,
    },
    IdwlGroupCountTooLarge {
        count: usize,
        max: usize,
    },
    IdwlGroupShapeOutOfRange {
        word_count: usize,
        group_count: usize,
    },
    UnsupportedIdwlMode {
        mode: u32,
        max_supported: u32,
    },
    UnsupportedIdwlContextKind {
        context_kind: u32,
        max_supported: u32,
    },
    IdwlUpdateIndexOutOfRange {
        index: usize,
        count: usize,
    },
    IdwlCompactBaseOutOfRange {
        value: i32,
        max: usize,
    },
    IdwlCompactDeltaOutOfRange {
        value: i32,
        max: usize,
    },
    CopyWlcBlockCountOutOfRange {
        count: usize,
        source: usize,
        destination: usize,
    },
    CopyWlcBlockIndexOutOfRange {
        index: usize,
        count: usize,
    },
    VarRebitallocChannelCountOutOfRange {
        count: usize,
        max: usize,
    },
    VarRebitallocChannelIndexOutOfRange {
        index: usize,
        count: usize,
    },
    VarRebitallocQuantUnitOutOfRange {
        quant_unit: usize,
        max: usize,
    },
    VarRebitallocSelectorOutOfRange {
        selector: usize,
        max: usize,
    },
    VarRebitallocSelectorCountOutOfRange {
        count: usize,
        max: usize,
    },
    VarRebitallocCostTableTooShort {
        table: &'static str,
        needed: usize,
        actual: usize,
    },
    IdsfGroupCountTooLarge {
        count: usize,
        max: usize,
    },
    IdsfCompactDeltaOutOfRange {
        value: i32,
        max: usize,
    },
    HuffmanSymbolOutOfRange {
        descriptor: &'static str,
        symbol: usize,
    },
}

pub fn calc_nbits_for_idct_at5(
    channels: &[IdctChannelState<'_>],
    blocks: &mut [IdctBlockState],
    selector: u32,
) -> Result<i32, BitcountError> {
    if channels.is_empty() {
        return Err(BitcountError::EmptyChannels);
    }
    if blocks.len() != channels.len() {
        return Err(BitcountError::ChannelBlockCountMismatch {
            channels: channels.len(),
            blocks: blocks.len(),
        });
    }

    if channels[0].band_count == 0 {
        return Ok(0);
    }
    if selector > 1 {
        return Err(BitcountError::UnsupportedIdctSelector {
            selector,
            max_supported: 1,
        });
    }

    validate_idct_inputs(channels)?;

    let fixbits = idct_fixbits_at5();
    let mut bit_count = (channels.len() as i32) * 3 + 1;
    for (channel_index, (channel, block)) in channels.iter().zip(blocks.iter_mut()).enumerate() {
        let band_count = channel.band_count;
        let row_fixbits = i32::from(*fixbits.get(channel.bandwidth_mode).ok_or(
            BitcountError::BandwidthModeOutOfRange {
                channel: channel_index,
                mode: channel.bandwidth_mode,
                max: IDCT_FIXBITS_AT5_ENTRIES - 1,
            },
        )?);

        let active_count = if selector == 0 {
            band_count
        } else {
            idct_selector_active_count(band_count, &block.aux)
        };

        block.band_count = active_count;
        block.flags = [0; IDCT_BAND_LIMIT_AT5];
        for band in 0..band_count {
            block.flags[band] = match (channel.mode, channel.idct_source[band] > 0) {
                (_, true) => 1,
                (0, false) => 0,
                (_, false) if channel.previous_idct_source[band] > 0 => 2,
                _ => 0,
            };
        }

        let selected = if selector == 0 {
            IdctCost {
                mode: 0,
                split_flag: 0,
                bits: fixed_idct_cost(&block.flags, band_count, row_fixbits) + 1,
            }
        } else {
            select_idct_selector_cost(channel, block, row_fixbits)?
        };

        block.mode = selected.mode;
        block.split_flag = selected.split_flag;
        bit_count += selected.bits;
    }

    Ok(bit_count)
}

pub fn calc_nbits_for_idsf_ch_at5(
    channel: &IdsfChannelState<'_>,
    block: &mut IdsfBlockState,
) -> Result<i32, BitcountError> {
    validate_idsf_inputs(channel)?;

    if channel.mode != 0 {
        return calc_nbits_for_idsf_previous_ch_at5(channel, block);
    }

    calc_nbits_for_idsf_fresh_ch_at5(channel, block)
}

pub fn calc_nbits_for_idwl_ch_init_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
) -> Result<i32, BitcountError> {
    validate_idwl_inputs(channel)?;
    build_idwl_word_rows(channel, block);
    rebuild_idwl_activity(channel, block);
    if channel.context_kind != 2 && channel.mode == 0 {
        calc_idwl_sg_at5(channel, block)?;
    }

    init_idwl_common_fields(channel, block);
    match (channel.context_kind, channel.mode) {
        (2, 0) => {
            block.costs[1] = IDWL_COST_LIMIT_AT5;
            block.costs[2] = IDWL_COST_LIMIT_AT5;
            block.costs[3] = calc_nbits_for_idwl_3_at5(channel, block)?;
        }
        (_, 0) => {
            block.costs[1] = calc_nbits_for_idwl_1_at5(channel, block);
            block.costs[2] = calc_nbits_for_idwl_2_at5(channel, block)?;
            block.costs[3] = calc_nbits_for_idwl_3_at5(channel, block)?;
        }
        (_, 1) => {
            block.costs[1] = calc_nbits_for_idwl_4_at5(channel, block)?;
            block.costs[2] = calc_nbits_for_idwl_5_at5(channel, block)?;
            block.costs[3] = calc_nbits_for_idwl_3_at5(channel, block)?;
        }
        _ => unreachable!("validated IDWL mode"),
    }

    select_idwl_cost(block)
}

pub fn calc_nbits_for_idwl_ch_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
    selector_mode: u32,
    word_index: usize,
) -> Result<i32, BitcountError> {
    validate_idwl_inputs(channel)?;
    if word_index >= channel.word_count {
        return Err(BitcountError::IdwlUpdateIndexOutOfRange {
            index: word_index,
            count: channel.word_count,
        });
    }

    if channel.mode == selector_mode {
        update_idwl_word_rows(channel, block, word_index);
        rebuild_idwl_activity(channel, block);
        if channel.context_kind != 2 && channel.mode == 0 {
            calc_idwl_sg_at5(channel, block)?;
        }
    }

    match (channel.context_kind, selector_mode, channel.mode) {
        (_, 0, 0) if channel.context_kind != 2 => {
            block.costs[1] = calc_nbits_for_idwl_1_at5(channel, block);
            block.costs[2] = calc_nbits_for_idwl_2_at5(channel, block)?;
            block.costs[3] = calc_nbits_for_idwl_3_at5(channel, block)?;
        }
        (_, 0, 1) => {
            block.costs[1] = calc_nbits_for_idwl_4_at5(channel, block)?;
            block.costs[2] = calc_nbits_for_idwl_5_at5(channel, block)?;
        }
        (2, 0, 0) => {
            block.costs[3] = calc_nbits_for_idwl_3_at5(channel, block)?;
        }
        (_, 1, 1) => {
            block.costs[1] = calc_nbits_for_idwl_4_at5(channel, block)?;
            block.costs[2] = calc_nbits_for_idwl_5_at5(channel, block)?;
            block.costs[3] = calc_nbits_for_idwl_3_at5(channel, block)?;
        }
        _ => {}
    }

    select_idwl_cost(block)
}

pub fn copy_wlcinfo_at5(
    source: &[IdwlBlockState],
    destination: &mut [IdwlBlockState],
    block_count: usize,
    param4: u32,
    block_index: usize,
) -> Result<(), BitcountError> {
    if block_count == 0 || block_count > source.len() || block_count > destination.len() {
        return Err(BitcountError::CopyWlcBlockCountOutOfRange {
            count: block_count,
            source: source.len(),
            destination: destination.len(),
        });
    }
    if block_index >= block_count {
        return Err(BitcountError::CopyWlcBlockIndexOutOfRange {
            index: block_index,
            count: block_count,
        });
    }

    for index in 0..block_count {
        destination[index].mode = source[index].mode;
    }

    copy_wlc_rows_at5(&source[block_index], &mut destination[block_index]);

    if block_index != 0 {
        copy_wlc_costs_1_to_3_at5(&source[1], &mut destination[1]);
        copy_wlc_fields_28_to_60_at5(&source[1], &mut destination[1]);
        return Ok(());
    }

    if param4 == 2 {
        destination[0].costs[3] = source[0].costs[3];
        destination[0].selector_fields_50_60 = source[0].selector_fields_50_60;
    } else {
        destination[0].side = source[0].side.clone();
        copy_wlc_costs_1_to_3_at5(&source[0], &mut destination[0]);
        copy_wlc_fields_28_to_60_at5(&source[0], &mut destination[0]);
    }

    if block_count == 2 {
        destination[1].costs[1] = source[1].costs[1];
        destination[1].costs[2] = source[1].costs[2];
        destination[1].selector_fields_28_38 = source[1].selector_fields_28_38;
        destination[1].selector_fields_3c_4c = source[1].selector_fields_3c_4c;
    }

    Ok(())
}

pub fn calc_nbits_var_rebitalloc_at5<F>(
    input: VarRebitallocInput<'_>,
    blocks: &mut [IdctBlockState],
    mut calc_idct_bits: F,
) -> Result<VarRebitallocResult, BitcountError>
where
    F: FnMut(&mut [IdctBlockState]) -> Result<i32, BitcountError>,
{
    validate_var_rebitalloc_input(&input, blocks.len())?;

    let cost_base = input.quant_unit * VAR_REBITALLOC_SELECTOR_STRIDE_AT5;
    let old_cost_index = cost_base + input.old_selector;
    let current_idct_bits = i16_wrap_i32(input.current_idct_bits);
    let source_cost = i32::from(input.source_costs[old_cost_index]);
    let target_cost = i32::from(input.target_costs[old_cost_index]);
    let source_total = i16_wrap_i32(current_idct_bits + source_cost);
    let target_total = i16_wrap_i32(current_idct_bits + target_cost);

    let mut best_selector = input.old_selector;
    let mut best_cost = VAR_REBITALLOC_COST_LIMIT_AT5;
    for selector in 0..input.selector_count {
        let cost = i32::from(input.target_costs[cost_base + selector]);
        if cost < best_cost {
            best_selector = selector;
            best_cost = cost;
        }
    }

    if best_selector != input.old_selector {
        let saved_blocks = blocks[..input.channel_count].to_vec();
        blocks[input.channel_index].aux[input.quant_unit] = best_selector as u32;
        let trial_idct_bits = i16_wrap_i32(calc_idct_bits(&mut blocks[..input.channel_count])?);
        let trial_total = i16_wrap_i32(best_cost + trial_idct_bits);
        if trial_total < target_total {
            return Ok(VarRebitallocResult {
                bit_delta: i16_wrap_i32(best_cost + trial_idct_bits - source_total),
                word_length: best_selector as u32,
                idct_bits: trial_idct_bits,
            });
        }
        blocks[..input.channel_count].clone_from_slice(&saved_blocks);
    }

    Ok(VarRebitallocResult {
        bit_delta: i16_wrap_i32(target_total - source_total),
        word_length: input.old_selector as u32,
        idct_bits: current_idct_bits,
    })
}

fn copy_wlc_rows_at5(source: &IdwlBlockState, destination: &mut IdwlBlockState) {
    destination.word_rows = source.word_rows;
    destination.active_counts = source.active_counts;
    destination.duplicate_indices = source.duplicate_indices;
    destination.tail_counts = source.tail_counts;
}

fn copy_wlc_costs_1_to_3_at5(source: &IdwlBlockState, destination: &mut IdwlBlockState) {
    destination.costs[1..4].copy_from_slice(&source.costs[1..4]);
}

fn copy_wlc_fields_28_to_60_at5(source: &IdwlBlockState, destination: &mut IdwlBlockState) {
    destination.selector_fields_28_38 = source.selector_fields_28_38;
    destination.selector_fields_3c_4c = source.selector_fields_3c_4c;
    destination.selector_fields_50_60 = source.selector_fields_50_60;
}

fn validate_var_rebitalloc_input(
    input: &VarRebitallocInput<'_>,
    block_len: usize,
) -> Result<(), BitcountError> {
    if input.channel_count == 0 || input.channel_count > block_len {
        return Err(BitcountError::VarRebitallocChannelCountOutOfRange {
            count: input.channel_count,
            max: block_len,
        });
    }
    if input.channel_index >= input.channel_count {
        return Err(BitcountError::VarRebitallocChannelIndexOutOfRange {
            index: input.channel_index,
            count: input.channel_count,
        });
    }
    if input.quant_unit >= IDCT_BAND_LIMIT_AT5 {
        return Err(BitcountError::VarRebitallocQuantUnitOutOfRange {
            quant_unit: input.quant_unit,
            max: IDCT_BAND_LIMIT_AT5 - 1,
        });
    }
    if input.old_selector >= VAR_REBITALLOC_SELECTOR_STRIDE_AT5 {
        return Err(BitcountError::VarRebitallocSelectorOutOfRange {
            selector: input.old_selector,
            max: VAR_REBITALLOC_SELECTOR_STRIDE_AT5 - 1,
        });
    }
    if input.selector_count == 0 || input.selector_count > VAR_REBITALLOC_SELECTOR_STRIDE_AT5 {
        return Err(BitcountError::VarRebitallocSelectorCountOutOfRange {
            count: input.selector_count,
            max: VAR_REBITALLOC_SELECTOR_STRIDE_AT5,
        });
    }

    let cost_base = input.quant_unit * VAR_REBITALLOC_SELECTOR_STRIDE_AT5;
    let old_needed = cost_base + input.old_selector + 1;
    if input.source_costs.len() < old_needed {
        return Err(BitcountError::VarRebitallocCostTableTooShort {
            table: "source",
            needed: old_needed,
            actual: input.source_costs.len(),
        });
    }

    let target_needed = old_needed.max(cost_base + input.selector_count);
    if input.target_costs.len() < target_needed {
        return Err(BitcountError::VarRebitallocCostTableTooShort {
            table: "target",
            needed: target_needed,
            actual: input.target_costs.len(),
        });
    }

    Ok(())
}

fn i16_wrap_i32(value: i32) -> i32 {
    i32::from(value as i16)
}

fn validate_idwl_inputs(channel: &IdwlChannelState<'_>) -> Result<(), BitcountError> {
    if channel.mode >= IDWL_MODE_COUNT_AT5 as u32 {
        return Err(BitcountError::UnsupportedIdwlMode {
            mode: channel.mode,
            max_supported: (IDWL_MODE_COUNT_AT5 - 1) as u32,
        });
    }
    if channel.context_kind > 2 {
        return Err(BitcountError::UnsupportedIdwlContextKind {
            context_kind: channel.context_kind,
            max_supported: 2,
        });
    }
    if channel.word_count > IDWL_BAND_LIMIT_AT5 {
        return Err(BitcountError::IdwlWordCountTooLarge {
            count: channel.word_count,
            max: IDWL_BAND_LIMIT_AT5,
        });
    }
    if channel.group_count > IDWL_SG_GROUP_LIMIT_AT5 {
        return Err(BitcountError::IdwlGroupCountTooLarge {
            count: channel.group_count,
            max: IDWL_SG_GROUP_LIMIT_AT5,
        });
    }
    if channel.word_lengths.len() < channel.word_count {
        return Err(BitcountError::IdwlWordLengthsTooShort {
            needed: channel.word_count,
            actual: channel.word_lengths.len(),
        });
    }
    if channel.mode != 0 && channel.previous_word_lengths.len() < channel.word_count {
        return Err(BitcountError::PreviousIdwlWordLengthsTooShort {
            needed: channel.word_count,
            actual: channel.previous_word_lengths.len(),
        });
    }

    // Native cfg produces the IDWL (word_count, group_count) pair as
    // (band_extent, g_a_sg_shape_index_at5[band_extent-1] + 1) — the shape law
    // encoded by `idwl_sg_group_count` (and mirrored by cfg_shape_count_b8).
    // Admit exactly the pairs that obey that law; off-law pairs (e.g. 26/10,
    // 32/9) never appear in native cfg and stay typed-rejected.
    //
    // When group_count*3 overruns the processing word_count (the "reduced"
    // shapes — 48 kbps 26/9, 160 kbps 28/10), native calc_idwl_sg_at5
    // (native 0x46510: compact loop decompile 38113-38129, side loop 38223-38241)
    // drives its group loops off cfg+0xb8, NOT clamped to c4, so it reads the
    // 32-wide storage row past the processing count. Those shapes therefore
    // require the full 32-word storage backing and only occur at or above the
    // smallest native band extent (26); smaller law-consistent pairs (2/1, ...)
    // are garbage and keep failing IdwlGroupShapeOutOfRange as before.
    let grouped_bands = channel.group_count.saturating_mul(3);
    let law_shape = channel.group_count == idwl_sg_group_count(channel.word_count);
    if grouped_bands > channel.word_count {
        if !law_shape || channel.word_count < IDWL_MIN_BAND_EXTENT_AT5 {
            return Err(BitcountError::IdwlGroupShapeOutOfRange {
                word_count: channel.word_count,
                group_count: channel.group_count,
            });
        }
        if channel.word_lengths.len() < IDWL_BAND_LIMIT_AT5 {
            return Err(BitcountError::IdwlWordLengthsTooShort {
                needed: IDWL_BAND_LIMIT_AT5,
                actual: channel.word_lengths.len(),
            });
        }
    } else if !law_shape
        || (channel.group_count == IDWL_SG_GROUP_LIMIT_AT5
            && channel.word_count < IDWL_BAND_LIMIT_AT5)
    {
        // grouped_bands <= word_count: historical generic path. Reject off-law
        // pairs (e.g. 32/9) and the unobserved group_count==10 sub-32 extents
        // (30/10, 31/10) whose fixed group-9 tail [27..32) would overrun a row
        // narrower than the 32-word storage.
        return Err(BitcountError::IdwlGroupShapeOutOfRange {
            word_count: channel.word_count,
            group_count: channel.group_count,
        });
    }

    Ok(())
}

fn init_idwl_common_fields(channel: &IdwlChannelState<'_>, block: &mut IdwlBlockState) {
    block.selector_fields_14_24 = [0, 0, channel.word_count as i32, 0, 0];
    block.costs[0] = (channel.word_count as i32) * 3;
}

fn build_idwl_word_rows(channel: &IdwlChannelState<'_>, block: &mut IdwlBlockState) {
    for row in &mut block.word_rows {
        *row = [0; IDWL_BAND_LIMIT_AT5];
    }

    for index in 0..channel.word_count {
        block.word_rows[0][index] = channel.word_lengths[index];
    }
    block.valid_flags[0] = 1;

    for row in 1..IDWL_ROW_COUNT_AT5 {
        let mut valid = 1;
        for index in 0..channel.word_count {
            let value = channel.word_lengths[index]
                - i32::from(IDWL_WLC_COEF_AT5[channel.mode as usize][row - 1][index]);
            block.word_rows[row][index] = value;
            if value < 0 {
                valid = 0;
            }
        }
        block.valid_flags[row] = valid;
    }
}

fn update_idwl_word_rows(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
    word_index: usize,
) {
    block.word_rows[0][word_index] = channel.word_lengths[word_index];
    block.valid_flags[0] = 1;

    for row in 1..IDWL_ROW_COUNT_AT5 {
        block.word_rows[row][word_index] = channel.word_lengths[word_index]
            - i32::from(IDWL_WLC_COEF_AT5[channel.mode as usize][row - 1][word_index]);
        block.valid_flags[row] = if block.word_rows[row][..channel.word_count]
            .iter()
            .any(|value| *value < 0)
        {
            0
        } else {
            1
        };
    }
}

fn rebuild_idwl_activity(channel: &IdwlChannelState<'_>, block: &mut IdwlBlockState) {
    for row in 0..IDWL_ROW_COUNT_AT5 {
        if block.valid_flags[row] == 0 {
            block.tail_counts[row] = 0;
            block.active_counts[row] = [channel.word_count as i32; IDWL_ROW_COUNT_AT5];
            block.duplicate_indices[row] = [-1; IDWL_ROW_COUNT_AT5];
            continue;
        }

        let values = &block.word_rows[row];
        let mut active = [channel.word_count; IDWL_ROW_COUNT_AT5];
        active[1] = trim_trailing_exact(values, channel.word_count, 0);
        let mut tail_count = 0;

        if channel.mode == 0 {
            active[2] = trim_trailing_exact(values, channel.word_count, 1);
            let mut candidate3 = channel.word_count;
            let mut zeros = 0;
            while candidate3 > 0 && values[candidate3 - 1] == 0 {
                candidate3 -= 1;
                zeros += 1;
            }
            if (1..=4).contains(&zeros) {
                tail_count = zeros;
                while candidate3 > 0 && values[candidate3 - 1] == 1 {
                    candidate3 -= 1;
                }
                active[3] = candidate3;
            } else {
                active[3] = channel.word_count;
            }
        } else {
            active[2] = trim_trailing_zero_or_one(values, channel.word_count);
            let mut candidate3 = channel.word_count;
            while candidate3 > 0 && values[candidate3 - 1] == 0 {
                candidate3 -= 1;
            }
            let mut ones = 0;
            while candidate3 > 0 && values[candidate3 - 1] == 1 {
                candidate3 -= 1;
                ones += 1;
            }
            if ones > 2 {
                if ones > 6 {
                    tail_count = 6;
                    candidate3 += ones - 6;
                } else {
                    tail_count = ones;
                }
                active[3] = candidate3;
            } else {
                active[3] = channel.word_count;
            }
        }

        block.tail_counts[row] = tail_count as i32;
        for (candidate, active_count) in active.iter().enumerate() {
            block.active_counts[row][candidate] = *active_count as i32;
            block.duplicate_indices[row][candidate] = -1;
            for previous in 0..candidate {
                if active[candidate] == active[previous] {
                    block.duplicate_indices[row][candidate] = previous as i32;
                    break;
                }
            }
        }
    }
}

fn trim_trailing_exact(values: &[i32; IDWL_BAND_LIMIT_AT5], count: usize, target: i32) -> usize {
    let mut active = count;
    while active > 0 && values[active - 1] == target {
        active -= 1;
    }
    active
}

fn trim_trailing_zero_or_one(values: &[i32; IDWL_BAND_LIMIT_AT5], count: usize) -> usize {
    let mut active = count;
    while active > 0 && (0..=1).contains(&values[active - 1]) {
        active -= 1;
    }
    active
}

fn calc_idwl_sg_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
) -> Result<(), BitcountError> {
    let mut compact = [0; IDWL_SG_GROUP_LIMIT_AT5];
    for (group, compact_value) in compact.iter_mut().enumerate().take(channel.group_count) {
        let start = group * 3;
        *compact_value = round_div3_plus_half(
            channel.word_lengths[start]
                + channel.word_lengths[start + 1]
                + channel.word_lengths[start + 2],
        );
    }
    if channel.group_count == IDWL_SG_GROUP_LIMIT_AT5 {
        compact[9] = round_div5_plus_half(channel.word_lengths[27..32].iter().sum());
    }

    let compact_base = compact[0];
    for value in compact.iter_mut().take(channel.group_count).skip(1) {
        *value = compact_base - *value;
    }
    block.side.compact = compact;

    for row in 0..IDWL_ROW_COUNT_AT5 {
        let duplicate = block.duplicate_indices[0][row];
        if duplicate >= 0 {
            block.side.rows[row] = block.side.rows[duplicate as usize];
            continue;
        }

        let compact_count = idwl_sg_group_count(block.active_counts[0][row] as usize);
        if let Some(previous) = (0..row).find(|previous| {
            block.side.rows[*previous][IDWL_SG_ROW_WORDS_AT5 - 1] == compact_count as i32
        }) {
            block.side.rows[row] = block.side.rows[previous];
            continue;
        }

        let compact_base_index = usize::try_from(compact_base).unwrap_or(IDWL_SG_CODEBOOK_BASES);
        if compact_base_index >= IDWL_SG_CODEBOOK_BASES {
            return Err(BitcountError::IdwlCompactBaseOutOfRange {
                value: compact_base,
                max: IDWL_SG_CODEBOOK_BASES - 1,
            });
        }

        let selector = idwl_sg_codebook_selector(compact_base_index, compact_count, &compact)?;
        block.side.codebook[0] = compact_base;
        for group in 1..IDWL_SG_GROUP_LIMIT_AT5 {
            block.side.codebook[group] =
                i32::from(IDWL_WLC_SG_CODEBOOK_AT5[compact_base_index][selector][group - 1]);
        }
        // Native calc_idwl_sg_at5 (0x46510; decompile 38182-38218) seeds all
        // nine tail slots with raw table deltas, then converts only the live
        // cfg+0xb8 prefix to absolute predictions. Storage past group_count
        // remains raw and is observable through the shared WLC side copy.
        for group in 1..channel.group_count {
            block.side.codebook[group] = compact_base - block.side.codebook[group];
        }

        let side_row = &mut block.side.rows[row];
        for value in side_row.iter_mut() {
            *value = 0;
        }
        // Native calc_idwl_sg_at5 (0x46510) writes the FULL 3-wide word triple
        // for every group 0..cfg+0xb8, storage-backed past the c4 processing
        // count: group 0 unconditionally (decompile 38220-38222) and groups
        // 1..b8 (decompile 38223-38241). The side-row array (35-wide) and
        // word_lengths (validated >= 32-wide for reduced shapes) both span the
        // storage row, so these stay in bounds; indices >= word_count are never
        // packed (pack_idwl reads only 0..config_count) and never replay-compared.
        debug_assert!(channel.word_count >= 3, "native c4 processing count >= 26");
        for index in 0..3 {
            side_row[index] = channel.word_lengths[index] - block.side.codebook[0];
        }
        for group in 1..channel.group_count {
            let start = group * 3;
            for index in start..start + 3 {
                side_row[index] = channel.word_lengths[index] - block.side.codebook[group];
            }
        }
        if channel.group_count == IDWL_SG_GROUP_LIMIT_AT5 && channel.word_count > 27 {
            for index in 27..channel.word_count {
                side_row[index] = channel.word_lengths[index] - block.side.codebook[9];
            }
        }
        for value in side_row.iter_mut().take(channel.word_count) {
            *value = (*value as u32 & 7) as i32;
        }
        side_row[32] = selector as i32;
        side_row[33] = compact_base;
        side_row[34] = compact_count as i32;
    }

    Ok(())
}

fn idwl_sg_group_count(active_count: usize) -> usize {
    if active_count == 0 {
        0
    } else {
        // Native indexes g_a_sg_shape_index_at5[active_count - 1]. Use the
        // generated, binary-pinned table rather than duplicating its bytes.
        usize::from(sg_shape_index_at5()[active_count - 1]) + 1
    }
}

fn idwl_sg_codebook_selector(
    compact_base: usize,
    compact_count: usize,
    compact: &[i32; IDWL_SG_GROUP_LIMIT_AT5],
) -> Result<usize, BitcountError> {
    let n2 = n2_under128_at5();
    let mut best_selector = 0;
    let mut best_cost = idwl_sg_codebook_cost(compact_base, 0, compact_count, compact, &n2)?;
    for selector in 1..IDWL_SG_CODEBOOK_SELECTORS {
        let cost = idwl_sg_codebook_cost(compact_base, selector, compact_count, compact, &n2)?;
        if cost < best_cost {
            best_selector = selector;
            best_cost = cost;
        }
    }
    Ok(best_selector)
}

fn idwl_sg_codebook_cost(
    compact_base: usize,
    selector: usize,
    compact_count: usize,
    compact: &[i32; IDWL_SG_GROUP_LIMIT_AT5],
    n2: &[u16; 128],
) -> Result<i32, BitcountError> {
    let mut cost = 0;
    for group in 1..compact_count {
        let delta = (compact[group]
            - i32::from(IDWL_WLC_SG_CODEBOOK_AT5[compact_base][selector][group - 1]))
        .abs();
        let delta_index = usize::try_from(delta).unwrap_or(usize::MAX);
        let value = n2
            .get(delta_index)
            .ok_or(BitcountError::IdwlCompactDeltaOutOfRange {
                value: delta,
                max: n2.len() - 1,
            })?;
        cost += i32::from(*value);
    }
    Ok(cost)
}

fn calc_nbits_for_idwl_1_at5(channel: &IdwlChannelState<'_>, block: &mut IdwlBlockState) -> i32 {
    let mut adjusted = [[0; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5];
    let mut raw_bits = [[0; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5];
    let mut fields = [[IdwlWindowFields {
        start: 0,
        bits: 0,
        base: 0,
    }; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5];

    for row in 0..IDWL_ROW_COUNT_AT5 {
        if block.valid_flags[row] == 0 {
            continue;
        }
        for candidate in 0..IDWL_ROW_COUNT_AT5 {
            if let Some(duplicate) = duplicate_index(block, row, candidate) {
                raw_bits[row][candidate] = raw_bits[row][duplicate];
                fields[row][candidate] = fields[row][duplicate];
            } else {
                let active = block.active_counts[row][candidate] as usize;
                if active > 0 {
                    let choice = idwl_window_choice(&block.word_rows[row], active);
                    raw_bits[row][candidate] = choice.0 + 10;
                    fields[row][candidate] = choice.1;
                }
            }
            adjusted[row][candidate] = adjust_idwl_candidate_bits(
                raw_bits[row][candidate],
                candidate,
                channel.mode,
                channel.word_count,
                block.active_counts[row][candidate] as usize,
            );
        }
    }

    let (best_row, best_candidate, best_bits) = select_positive_idwl_matrix(&adjusted);
    let best_fields = fields[best_row][best_candidate];
    block.selector_fields_28_38 = [
        0,
        best_candidate as i32,
        block.active_counts[best_row][best_candidate],
        block.tail_counts[best_row],
        best_row as i32,
    ];
    block.side.window_fields = [
        best_fields.start as i32,
        best_fields.bits as i32,
        best_fields.base,
    ];
    best_bits + 4
}

fn idwl_window_choice(
    values: &[i32; IDWL_BAND_LIMIT_AT5],
    count: usize,
) -> (i32, IdwlWindowFields) {
    let mut max_value = 0;
    let mut min_value = 7;
    let mut active = [true; 3];
    let mut starts = [count; 4];
    let mut bases = [0; 4];

    let mut cursor = count;
    while cursor > 0 {
        let index = cursor - 1;
        let value = values[index];
        if max_value < value {
            max_value = value;
        }
        if value < min_value {
            min_value = value;
        }

        for selector in 0..3 {
            if !active[selector] {
                continue;
            }
            if IDWL_THRESHOLD_AT5[selector] < max_value - min_value {
                active[selector] = false;
                starts[selector] = index + 1;
            } else {
                starts[selector] = index;
                bases[selector] = min_value;
            }
        }

        cursor = index;
    }

    let mut best_selector = 3;
    let mut best_bits = (count as i32) * 3;
    for selector in 0..3 {
        let bits = ((count - starts[selector]) * selector + starts[selector] * 3) as i32;
        if bits < best_bits {
            best_bits = bits;
            best_selector = selector;
        }
    }

    (
        best_bits,
        IdwlWindowFields {
            start: starts[best_selector],
            bits: best_selector,
            base: bases[best_selector],
        },
    )
}

fn calc_nbits_for_idwl_2_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
) -> Result<i32, BitcountError> {
    let mut raw_bits = [0; IDWL_ROW_COUNT_AT5];
    let mut selectors = [0; IDWL_ROW_COUNT_AT5];
    let mut adjusted = [0; IDWL_ROW_COUNT_AT5];

    for candidate in 0..IDWL_ROW_COUNT_AT5 {
        if let Some(duplicate) = duplicate_index(block, 0, candidate) {
            raw_bits[candidate] = raw_bits[duplicate];
            selectors[candidate] = selectors[duplicate];
        } else if let Some(choice) = calc_nbits_for_idwl_2_sub_at5(block, candidate)? {
            raw_bits[candidate] = choice.bits;
            selectors[candidate] = choice.selector;
        }

        adjusted[candidate] = adjust_idwl_candidate_bits(
            raw_bits[candidate],
            candidate,
            channel.mode,
            channel.word_count,
            block.active_counts[0][candidate] as usize,
        );
    }

    let mut best_candidate = 0;
    let mut best_bits = IDWL_COST_LIMIT_AT5;
    for (candidate, bits) in adjusted.iter().enumerate() {
        if *bits < best_bits && *bits > 0 {
            best_candidate = candidate;
            best_bits = *bits;
        }
    }

    if best_bits >= IDWL_COST_LIMIT_AT5 {
        return Ok(IDWL_COST_LIMIT_AT5);
    }

    let selector = selectors[best_candidate];
    block.selector_fields_3c_4c[4] = 0;
    if selector < 2 {
        block.selector_fields_3c_4c[0] = selector as i32;
        block.side.subgroup_flag = 0;
    } else {
        block.selector_fields_3c_4c[0] = (selector - 2) as i32;
        block.side.subgroup_flag = 1;
    }
    block.selector_fields_3c_4c[1] = best_candidate as i32;
    block.selector_fields_3c_4c[2] = block.active_counts[0][best_candidate];
    block.selector_fields_3c_4c[3] = block.tail_counts[0];

    Ok(best_bits + 2)
}

fn calc_nbits_for_idwl_2_sub_at5(
    block: &IdwlBlockState,
    candidate: usize,
) -> Result<Option<IdwlWlcChoice>, BitcountError> {
    let active = block.active_counts[0][candidate] as usize;
    if active == 0 {
        return Ok(None);
    }

    let row = &block.side.rows[candidate];
    let descriptors = wlc_descriptors();
    let mut costs = [0; 4];
    for value in row.iter().take(active) {
        if (3..=5).contains(value) {
            return Ok(None);
        }
        let symbol = *value as usize;
        costs[0] += huffman_bit_len(descriptors[0], symbol)?;
        costs[1] += huffman_bit_len(descriptors[1], symbol)?;
    }

    for pair in 0..(active / 2) {
        let start = pair * 2;
        costs[2] += 1;
        costs[3] += 1;
        if row[start] != 0 || row[start + 1] != 0 {
            costs[2] += huffman_bit_len(descriptors[0], row[start] as usize)?;
            costs[2] += huffman_bit_len(descriptors[0], row[start + 1] as usize)?;
            costs[3] += huffman_bit_len(descriptors[1], row[start] as usize)?;
            costs[3] += huffman_bit_len(descriptors[1], row[start + 1] as usize)?;
        }
    }
    for value in row.iter().take(active).skip((active / 2) * 2) {
        costs[2] += huffman_bit_len(descriptors[0], *value as usize)?;
        costs[3] += huffman_bit_len(descriptors[1], *value as usize)?;
    }

    let mut selector = 3;
    let mut bits = costs[3];
    for index in (0..3).rev() {
        if costs[index] < bits {
            selector = index;
            bits = costs[index];
        }
    }
    Ok(Some(IdwlWlcChoice {
        bits: bits + 9,
        selector,
    }))
}

fn calc_nbits_for_idwl_3_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
) -> Result<i32, BitcountError> {
    let mut adjusted = [[0; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5];
    let mut raw_bits = [[0; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5];
    let mut fields = [[[0; 5]; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5];

    for row in 0..IDWL_ROW_COUNT_AT5 {
        if block.valid_flags[row] == 0 {
            continue;
        }
        for candidate in 0..IDWL_ROW_COUNT_AT5 {
            if let Some(duplicate) = duplicate_index(block, row, candidate) {
                raw_bits[row][candidate] = raw_bits[row][duplicate];
                fields[row][candidate] = fields[row][duplicate];
            } else {
                let active = block.active_counts[row][candidate] as usize;
                // Native calc_nbits_for_idwl_3_at5 (native 0x1d6f0; Ghidra
                // 0x2d6f0, decompile 5518-5690) writes the per-candidate record
                // fields UNCONDITIONALLY in the row loop. In the LAB_0001d8eb
                // block the writes at decompile 5672-5676:
                //   [local_278+4]  = local_24c  (candidate-in-block)
                //   [local_278+8]  = iVar6      (active count)
                //   [local_278+0x10] = local_248 (row)
                //   [local_278+0xc]  = param_2+0x2f8 (tail)
                // are OUTSIDE the `if (0 < iVar6)` cost guard (5632-5671). Only
                // field 0 (the selector, `*piVar1`) is written inside the guard:
                // `*piVar1 = 0` at 5631/5661, then overwritten with the chosen
                // selector at 5665. For a count-0 candidate field 0 stays 0
                // (that native slot is uninitialized-stack UB when reached
                // through the guard, but native only reaches the min-select loop
                // for count>0 candidates; the count-0 slot's selector is 0 here).
                // We therefore write fields 1-4 unconditionally and only
                // compute/overwrite the cost + selector when active > 0.
                fields[row][candidate] = [
                    0,
                    candidate as i32,
                    active as i32,
                    block.tail_counts[row],
                    row as i32,
                ];
                if active > 0 {
                    let choice = idwl_progressive_row_cost(&block.word_rows[row], active)?;
                    raw_bits[row][candidate] = choice.bits + 5;
                    fields[row][candidate][0] = choice.selector as i32;
                }
            }
            adjusted[row][candidate] = adjust_idwl_candidate_bits_allow_zero(
                raw_bits[row][candidate],
                candidate,
                channel.mode,
                channel.word_count,
                block.active_counts[row][candidate] as usize,
            );
        }
    }

    let (best_row, best_candidate, best_bits) = select_positive_idwl_matrix(&adjusted);
    // Native calc_nbits_for_idwl_3_at5 (native 0x1d6f0; decompile 5587-5592)
    // writes the winning record from the SELECT indices directly, not from the
    // per-candidate `fields` row: only field 0 (the huffman selector) is read
    // out of the fields array (`aiStack_1ac[local_254 * 5 + local_250 * 0x14]`);
    // field 1 is `local_254` (the winning candidate index), field 2 is the live
    // active count `+0x278[best_row*4 + best_candidate]`, field 3 is
    // `+0x2f8[best_row]` (the row tail), and field 4 is `local_250` (the winning
    // row). This only diverges from `fields[best_row][best_candidate]` when the
    // winner is a DUPLICATE candidate (its `fields` row was copied wholesale
    // from the duplicate source, so it carries the source's candidate index in
    // field 1). Copying field 1 verbatim mis-selects the pack sub-mode
    // (`selector_b`): a mode-3-ch1 candidate-3 winner that duplicates candidate 2
    // would serialize as candidate 2 (tail-flag encoding) while its cost was
    // taken as candidate 3 (+7, mode3-value encoding) — the docs/13 §3.2 slice 5a
    // 160 pack-vs-accounting gap. Rebuild the record from the select indices.
    block.selector_fields_50_60 = [
        fields[best_row][best_candidate][0],
        best_candidate as i32,
        block.active_counts[best_row][best_candidate],
        block.tail_counts[best_row],
        best_row as i32,
    ];
    best_bits
        .checked_add(4)
        .ok_or(BitcountError::IdwlCompactDeltaOutOfRange {
            value: best_bits,
            max: i32::MAX as usize,
        })
}

fn idwl_progressive_row_cost(
    values: &[i32; IDWL_BAND_LIMIT_AT5],
    count: usize,
) -> Result<IdwlWlcChoice, BitcountError> {
    let descriptors = wlc_descriptors();
    let mut costs = [0; 4];
    for index in 1..count {
        let symbol = ((values[index] - values[index - 1]) as u32 & 7) as usize;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol)?;
        }
    }

    let selector = strict_min_index(&costs);
    Ok(IdwlWlcChoice {
        bits: costs[selector],
        selector,
    })
}

fn calc_nbits_for_idwl_4_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
) -> Result<i32, BitcountError> {
    let mut raw_bits = [0; IDWL_ROW_COUNT_AT5];
    let mut selectors = [0; IDWL_ROW_COUNT_AT5];
    let mut adjusted = [0; IDWL_ROW_COUNT_AT5];

    for candidate in 0..IDWL_ROW_COUNT_AT5 {
        if let Some(duplicate) = duplicate_index(block, 0, candidate) {
            raw_bits[candidate] = raw_bits[duplicate];
            selectors[candidate] = selectors[duplicate];
        } else {
            let active = block.active_counts[0][candidate] as usize;
            if active > 0 {
                let choice = idwl_previous_direct_cost(channel, active)?;
                raw_bits[candidate] = choice.bits + 2;
                selectors[candidate] = choice.selector;
            }
        }
        adjusted[candidate] = adjust_idwl_candidate_bits_allow_zero(
            raw_bits[candidate],
            candidate,
            channel.mode,
            channel.word_count,
            block.active_counts[0][candidate] as usize,
        );
    }

    let (candidate, bits) = select_idwl_candidate(&adjusted);
    block.selector_fields_28_38 = [
        selectors[candidate] as i32,
        candidate as i32,
        block.active_counts[0][candidate],
        block.tail_counts[0],
        0,
    ];
    Ok(bits + 2)
}

fn calc_nbits_for_idwl_5_at5(
    channel: &IdwlChannelState<'_>,
    block: &mut IdwlBlockState,
) -> Result<i32, BitcountError> {
    let mut raw_bits = [0; IDWL_ROW_COUNT_AT5];
    let mut selectors = [0; IDWL_ROW_COUNT_AT5];
    let mut adjusted = [0; IDWL_ROW_COUNT_AT5];

    for candidate in 0..IDWL_ROW_COUNT_AT5 {
        if let Some(duplicate) = duplicate_index(block, 0, candidate) {
            raw_bits[candidate] = raw_bits[duplicate];
            selectors[candidate] = selectors[duplicate];
        } else {
            let active = block.active_counts[0][candidate] as usize;
            if active > 0 {
                let choice = idwl_previous_progressive_cost(channel, active)?;
                raw_bits[candidate] = choice.bits + 2;
                selectors[candidate] = choice.selector;
            }
        }
        adjusted[candidate] = adjust_idwl_candidate_bits_allow_zero(
            raw_bits[candidate],
            candidate,
            channel.mode,
            channel.word_count,
            block.active_counts[0][candidate] as usize,
        );
    }

    let (candidate, bits) = select_idwl_candidate(&adjusted);
    block.selector_fields_3c_4c = [
        selectors[candidate] as i32,
        candidate as i32,
        block.active_counts[0][candidate],
        block.tail_counts[0],
        0,
    ];
    Ok(bits + 2)
}

fn idwl_previous_direct_cost(
    channel: &IdwlChannelState<'_>,
    active: usize,
) -> Result<IdwlWlcChoice, BitcountError> {
    let descriptors = wlc_descriptors();
    let mut costs = [0; 4];
    for index in 0..active {
        let symbol = ((channel.word_lengths[index] - channel.previous_word_lengths[index]) as u32
            & 7) as usize;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol)?;
        }
    }

    let selector = strict_min_index(&costs);
    Ok(IdwlWlcChoice {
        bits: costs[selector],
        selector,
    })
}

fn idwl_previous_progressive_cost(
    channel: &IdwlChannelState<'_>,
    active: usize,
) -> Result<IdwlWlcChoice, BitcountError> {
    let descriptors = wlc_descriptors();
    let mut costs = [0; 4];
    if active == 0 {
        return Ok(IdwlWlcChoice {
            bits: 0,
            selector: 0,
        });
    }

    let mut previous_delta =
        (channel.word_lengths[0] - channel.previous_word_lengths[0]) as u32 & 7;
    for (selector, descriptor) in descriptors.iter().enumerate() {
        costs[selector] += huffman_bit_len(*descriptor, previous_delta as usize)?;
    }
    for index in 1..active {
        let delta = (channel.word_lengths[index] - channel.previous_word_lengths[index]) as u32 & 7;
        let symbol = (delta.wrapping_sub(previous_delta) & 7) as usize;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol)?;
        }
        previous_delta = delta;
    }

    let selector = strict_min_index(&costs);
    Ok(IdwlWlcChoice {
        bits: costs[selector],
        selector,
    })
}

fn duplicate_index(block: &IdwlBlockState, row: usize, candidate: usize) -> Option<usize> {
    usize::try_from(block.duplicate_indices[row][candidate]).ok()
}

fn adjust_idwl_candidate_bits(
    raw_bits: i32,
    candidate: usize,
    mode: u32,
    word_count: usize,
    active_count: usize,
) -> i32 {
    if raw_bits < 1 {
        return 0;
    }

    match candidate {
        0 => raw_bits,
        1 => raw_bits + 5,
        2 if mode == 1 => raw_bits + 5 + (word_count - active_count) as i32,
        2 => raw_bits + 5,
        3 => raw_bits + 7,
        _ => raw_bits,
    }
}

fn adjust_idwl_candidate_bits_allow_zero(
    raw_bits: i32,
    candidate: usize,
    mode: u32,
    word_count: usize,
    active_count: usize,
) -> i32 {
    match candidate {
        0 => raw_bits,
        1 => raw_bits + 5,
        2 if mode == 1 => raw_bits + 5 + (word_count - active_count) as i32,
        2 => raw_bits + 5,
        3 => raw_bits + 7,
        _ => raw_bits,
    }
}

fn select_positive_idwl_matrix(
    costs: &[[i32; IDWL_ROW_COUNT_AT5]; IDWL_ROW_COUNT_AT5],
) -> (usize, usize, i32) {
    let mut best_row = 0;
    let mut best_candidate = 0;
    let mut best = costs[0][0];
    for (row, row_costs) in costs.iter().enumerate() {
        for (candidate, cost) in row_costs.iter().enumerate() {
            if *cost < best && *cost > 0 {
                best = *cost;
                best_row = row;
                best_candidate = candidate;
            }
        }
    }
    (best_row, best_candidate, best)
}

fn select_idwl_candidate(costs: &[i32; IDWL_ROW_COUNT_AT5]) -> (usize, i32) {
    let mut best_candidate = 0;
    let mut best = costs[0];
    for (candidate, cost) in costs.iter().enumerate().skip(1) {
        if *cost < best {
            best = *cost;
            best_candidate = candidate;
        }
    }
    (best_candidate, best)
}

fn select_idwl_cost(block: &mut IdwlBlockState) -> Result<i32, BitcountError> {
    let mode = strict_min_index(&block.costs);
    block.mode = mode as u32;
    Ok(block.costs[mode])
}

fn validate_idsf_inputs(channel: &IdsfChannelState<'_>) -> Result<(), BitcountError> {
    if channel.band_count > IDSF_BAND_LIMIT_AT5 {
        return Err(BitcountError::BandCountTooLarge {
            channel: 0,
            count: channel.band_count,
            max: IDSF_BAND_LIMIT_AT5,
        });
    }
    if channel.group_count > IDSF_COMPACT_GROUP_LIMIT_AT5 {
        return Err(BitcountError::IdsfGroupCountTooLarge {
            count: channel.group_count,
            max: IDSF_COMPACT_GROUP_LIMIT_AT5,
        });
    }
    // Native (calc_nbits_for_idsf_ch_at5, native 0x40e80; Ghidra 0x50e80) has NO
    // group-shape gate. On the fresh path (mode 0) it reads the fixed 32-word
    // object row +0x1b678 for all group indices: the compact-group loop touches
    // 0..group_count*3, and when group_count == 10 the group-9 block additionally
    // sums indices 27..31. Everything wrapped and costed stays band_count-scoped,
    // so the only Rust-side requirement here is memory safety: the borrowed slice
    // must cover the native read extent. The previous path (mode != 0) reads only
    // 0..band_count of both current and previous rows.
    let required = if channel.mode == 0 {
        let group_extent = if channel.group_count == 10 {
            32
        } else {
            channel.group_count.saturating_mul(3)
        };
        channel.band_count.max(group_extent)
    } else {
        channel.band_count
    };
    if channel.scale_factors.len() < required {
        return Err(BitcountError::IdsfScaleFactorsTooShort {
            needed: required,
            actual: channel.scale_factors.len(),
        });
    }
    if channel.mode != 0 && channel.previous_scale_factors.len() < channel.band_count {
        return Err(BitcountError::PreviousIdsfScaleFactorsTooShort {
            needed: channel.band_count,
            actual: channel.previous_scale_factors.len(),
        });
    }

    Ok(())
}

fn calc_nbits_for_idsf_previous_ch_at5(
    channel: &IdsfChannelState<'_>,
    block: &mut IdsfBlockState,
) -> Result<i32, BitcountError> {
    let band_count = channel.band_count;
    let mut costs = [0; 4];
    let mut selectors = [0; 4];
    costs[0] = (band_count as i32) * 6;

    let direct = previous_idsf_direct_cost(channel)?;
    costs[1] = direct.0 + 2;
    selectors[1] = direct.1;

    let progressive = previous_idsf_progressive_cost(channel)?;
    costs[2] = progressive.0 + 2;
    selectors[2] = progressive.1;

    costs[3] =
        if channel.scale_factors[..band_count] == channel.previous_scale_factors[..band_count] {
            0
        } else {
            0x4000
        };

    let mode = strict_min_index(&costs);
    block.mode = mode as u32;
    block.huffman_selector = selectors[mode];
    Ok(costs[mode])
}

// Fresh-channel path of calc_nbits_for_idsf_ch_at5, native 0x40e80 (decompile 35076,
// after the previous-mode early return at 35249-35252). The mode-record array
// `local_8c` holds five words per mode; the epilogue (decompile 35833-35836) stores:
//   param_1[0x71cf] = mode                       // 0x1c73c  argmin over local_2c[0..3]
//   param_1[0x71d4] = local_8c[mode*5+5]         // 0x1c750  mode_selector
//   param_1[0x71d3] = local_8c[mode*5+4]         // 0x1c74c  huffman selector
// Per-mode local_8c[mode*5+5]:
//   mode 0 -> local_8c[5]  : never written on the fresh path (stack residue) -> leave 0.
//   mode 1 -> local_8c[10] : mode-1 winning SUB index (sub-select loop 35634-35643);
//              carried here as mode1.mode_selector.
//   mode 2 -> local_8c[0xf]: the CONSTANT 3, stored unconditionally at 35684.
//   mode 3 -> local_8c[0x14]: mode-3 sub winner (35823) -> mode3.mode_selector.
fn calc_nbits_for_idsf_fresh_ch_at5(
    channel: &IdsfChannelState<'_>,
    block: &mut IdsfBlockState,
) -> Result<i32, BitcountError> {
    build_idsf_shifted_rows(channel, block);
    build_idsf_transformed_row(channel, block)?;

    let mode1 = idsf_mode1_choice(channel, block)?;
    block.start = mode1.fields.start;
    block.count = mode1.fields.bits;
    block.field_0x1c748 = mode1.fields.base;

    let mode2 = idsf_mode2_cost(channel, block)?;
    let mode3 = idsf_mode3_choice(channel, block)?;

    let costs = [
        (channel.band_count as i32) * 6,
        mode1.bits,
        mode2.0,
        mode3.bits,
    ];
    let mode = strict_min_index(&costs);
    block.mode = mode as u32;
    match mode {
        // mode 1: store the winning sub index (0x1c750). Native leaves 0x1c74c
        // (huffman selector) as stack garbage on this arm, so do NOT touch it.
        1 => block.mode_selector = mode1.mode_selector,
        // mode 2: native writes the CONSTANT 3 to 0x1c750 (decompile 35684 + tail 35834).
        2 => {
            block.huffman_selector = mode2.1;
            block.mode_selector = 3;
        }
        3 => {
            block.huffman_selector = mode3.huffman_selector;
            block.mode_selector = mode3.mode_selector;
        }
        _ => {}
    }

    Ok(costs[mode])
}

fn previous_idsf_direct_cost(
    channel: &IdsfChannelState<'_>,
) -> Result<(i32, usize), BitcountError> {
    let descriptors = sfc_descriptors();
    let mut costs = [0; 4];
    for index in 0..channel.band_count {
        let symbol = wrapped_sub_i32(
            channel.scale_factors[index],
            channel.previous_scale_factors[index],
            0x3f,
        );
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol)?;
        }
    }

    let selector = strict_min_index(&costs);
    Ok((costs[selector], selector))
}

fn previous_idsf_progressive_cost(
    channel: &IdsfChannelState<'_>,
) -> Result<(i32, usize), BitcountError> {
    let descriptors = sfc_descriptors();
    let mut costs = [0; 4];
    if channel.band_count == 0 {
        return Ok((0, 0));
    }

    let mut previous_delta = wrapped_sub_i32(
        channel.scale_factors[0],
        channel.previous_scale_factors[0],
        0x3f,
    ) as i32;
    for (selector, descriptor) in descriptors.iter().enumerate() {
        costs[selector] += huffman_bit_len(*descriptor, previous_delta as usize)?;
    }

    for index in 1..channel.band_count {
        let delta = wrapped_sub_i32(
            channel.scale_factors[index],
            channel.previous_scale_factors[index],
            0x3f,
        ) as i32;
        let symbol = (delta - previous_delta) as u32 & 0x3f;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol as usize)?;
        }
        previous_delta = delta;
    }

    let selector = strict_min_index(&costs);
    Ok((costs[selector], selector))
}

fn build_idsf_shifted_rows(
    channel: &IdsfChannelState<'_>,
    block: &mut IdsfBlockState,
) -> [bool; 3] {
    let mut valid = [true; 3];
    for index in 0..channel.band_count {
        block.shifted_rows[0][index] = channel.scale_factors[index];
    }

    for row in 1..IDSF_SHIFTED_ROWS_AT5 {
        for index in 0..channel.band_count {
            let value = channel.scale_factors[index] + (index / (row + 1)) as i32;
            block.shifted_rows[row][index] = value;
            if value > 0x3f {
                valid[row] = false;
            }
        }
    }

    valid
}

fn build_idsf_transformed_row(
    channel: &IdsfChannelState<'_>,
    block: &mut IdsfBlockState,
) -> Result<(), BitcountError> {
    let mut compact = [0; IDSF_COMPACT_GROUP_LIMIT_AT5];
    for group in 0..channel.group_count {
        let start = group * 3;
        compact[group] = round_div3_plus_half(
            channel.scale_factors[start]
                + channel.scale_factors[start + 1]
                + channel.scale_factors[start + 2],
        );
    }
    if channel.group_count == 10 {
        compact[9] = round_div5_plus_half(channel.scale_factors[27..32].iter().sum());
    }

    let compact_count = if channel.band_count == 0 {
        0
    } else {
        usize::from(sg_shape_index_at5()[channel.band_count - 1]) + 1
    };
    let first = compact[0];
    block.compact_base = first;
    for value in compact.iter_mut().take(channel.group_count).skip(1) {
        *value = first - *value;
    }

    let n2 = n2_under128_at5();
    let mut best_selector = 0;
    let mut best_cost = compact_codebook_cost(0, compact_count, &compact, &n2)?;
    for selector in 1..IDSF_COMPACT_CODEBOOK_ROWS {
        let cost = compact_codebook_cost(selector, compact_count, &compact, &n2)?;
        if cost < best_cost {
            best_cost = cost;
            best_selector = selector;
        }
    }
    block.codebook_selector = best_selector;

    let mut predicted = [0; IDSF_COMPACT_GROUP_LIMIT_AT5];
    predicted[0] = first;
    for group in 1..channel.group_count {
        predicted[group] = first - i32::from(IDSF_SFC_SG_CODEBOOK_AT5[best_selector][group - 1]);
    }

    for group in 0..channel.group_count {
        let group_start = group * 3;
        for index in group_start..(group_start + 3).min(channel.band_count) {
            block.transformed[index] =
                wrap_idsf_residual(channel.scale_factors[index] - predicted[group]);
        }
    }
    if channel.group_count == 10 {
        for index in 27..32 {
            block.transformed[index] =
                wrap_idsf_residual(channel.scale_factors[index] - predicted[9]);
        }
    }

    Ok(())
}

fn compact_codebook_cost(
    selector: usize,
    compact_count: usize,
    compact: &[i32; IDSF_COMPACT_GROUP_LIMIT_AT5],
    n2: &[u16; 128],
) -> Result<i32, BitcountError> {
    let mut cost = 0;
    for index in 1..compact_count {
        let delta =
            (compact[index] - i32::from(IDSF_SFC_SG_CODEBOOK_AT5[selector][index - 1])).abs();
        let delta = usize::try_from(delta).unwrap_or(usize::MAX);
        let value = n2
            .get(delta)
            .ok_or(BitcountError::IdsfCompactDeltaOutOfRange {
                value: compact[index],
                max: n2.len() - 1,
            })?;
        cost += i32::from(*value);
    }
    Ok(cost)
}

fn idsf_mode1_choice(
    channel: &IdsfChannelState<'_>,
    block: &IdsfBlockState,
) -> Result<IdsfMode1Choice, BitcountError> {
    let valid_rows = idsf_shifted_row_validity(channel, block);
    let mut costs = [0; 4];
    let mut fields = [IdsfWindowFields {
        start: 0,
        bits: 0,
        base: 0,
    }; 4];

    for row in 0..IDSF_SHIFTED_ROWS_AT5 {
        if valid_rows[row] {
            let choice = idsf_window_choice(&block.shifted_rows[row], channel.band_count);
            costs[row] = choice.0 + 0x10;
            fields[row] = choice.1;
        }
    }

    if let Some(choice) = idsf_fixed_transformed_choice(channel, block) {
        costs[3] = choice.bits;
        fields[3] = choice.fields;
    }

    let mut selected = 0;
    let mut best = costs[0];
    for (index, cost) in costs.iter().enumerate().skip(1) {
        if *cost < best && *cost > 0 {
            selected = index;
            best = *cost;
        }
    }

    Ok(IdsfMode1Choice {
        bits: best,
        fields: fields[selected],
        // `selected` is native `local_8c[10]` (sub-select loop, decompile 35634-35643):
        // 0..2 = shifted-row windows, 3 = compact transformed plane.
        mode_selector: selected,
    })
}

fn idsf_shifted_row_validity(
    channel: &IdsfChannelState<'_>,
    block: &IdsfBlockState,
) -> [bool; IDSF_SHIFTED_ROWS_AT5] {
    let mut valid = [true; IDSF_SHIFTED_ROWS_AT5];
    for (row, valid_row) in valid.iter_mut().enumerate().skip(1) {
        *valid_row = block.shifted_rows[row][..channel.band_count]
            .iter()
            .all(|value| *value <= 0x3f);
    }
    valid
}

fn idsf_window_choice(
    values: &[i32; IDSF_BAND_LIMIT_AT5],
    count: usize,
) -> (i32, IdsfWindowFields) {
    let mut max_value = 0;
    let mut min_value = 0x3f;
    let mut active = [true; 6];
    let mut starts = [count; 7];
    let mut bases = [0; 7];

    let mut cursor = count;
    while cursor > 0 {
        let previous_cursor = cursor;
        let index = previous_cursor - 1;
        let value = values[index];
        if max_value < value {
            max_value = value;
        }
        if value < min_value {
            min_value = value;
        }

        for selector in 0..6 {
            if !active[selector] {
                continue;
            }
            if IDSF_THRESHOLD_AT5[selector] < max_value - min_value {
                active[selector] = false;
                starts[selector] = previous_cursor;
            } else {
                starts[selector] = index;
                bases[selector] = min_value;
            }
        }
        cursor = index;
    }

    let mut best_selector = 6;
    let mut best_bits = (count as i32) * 6;
    for selector in 0..6 {
        let bits = ((count - starts[selector]) * selector + starts[selector] * 6) as i32;
        if bits < best_bits {
            best_bits = bits;
            best_selector = selector;
        }
    }

    (
        best_bits,
        IdsfWindowFields {
            start: starts[best_selector],
            bits: best_selector,
            base: bases[best_selector],
        },
    )
}

fn idsf_fixed_transformed_choice(
    channel: &IdsfChannelState<'_>,
    block: &IdsfBlockState,
) -> Option<IdsfMode1Choice> {
    let mut best: Option<(i32, IdsfWindowFields)> = None;
    for start in 0..channel.band_count {
        if block.transformed[..start]
            .iter()
            .any(|value| !idsf_small_residual_for_4bit_field(*value))
        {
            continue;
        }

        let tail = &block.transformed[start..channel.band_count];
        let min_value = *tail.iter().min()?;
        if !idsf_small_residual_for_4bit_field(min_value) {
            continue;
        }
        let max_value = *tail.iter().max()?;
        let bits = match max_value - min_value {
            0 => 0,
            1 => 1,
            2 | 3 => 2,
            4..=7 => 3,
            _ => continue,
        };
        let cost = (start * 4 + (channel.band_count - start) * bits) as i32;
        let fields = IdsfWindowFields {
            start,
            bits,
            base: min_value,
        };
        if best.is_none_or(|(best_cost, _)| cost < best_cost) {
            best = Some((cost, fields));
        }
    }

    best.map(|(bits, fields)| IdsfMode1Choice {
        bits: bits + 0x19,
        fields,
        // Compact transformed plane is native sub 3; the caller only reads bits/fields
        // from this return, but keep the sub index self-consistent.
        mode_selector: 3,
    })
}

fn idsf_mode2_cost(
    channel: &IdsfChannelState<'_>,
    block: &IdsfBlockState,
) -> Result<(i32, usize), BitcountError> {
    let descriptors = sfc_sg_descriptors();
    let mut costs = [0; 4];
    for value in &block.transformed[..channel.band_count] {
        if !(-7..=7).contains(value) {
            return Ok((0x4000, 0));
        }
        let symbol = *value as u32 & 0x0f;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol as usize)?;
        }
    }

    let selector = strict_min_index(&costs);
    Ok((costs[selector] + 0x0e, selector))
}

fn idsf_mode3_choice(
    channel: &IdsfChannelState<'_>,
    block: &IdsfBlockState,
) -> Result<IdsfMode3Choice, BitcountError> {
    let valid_rows = idsf_shifted_row_validity(channel, block);
    let mut costs = [0; 4];
    let mut selectors = [0; 4];

    for row in 0..IDSF_SHIFTED_ROWS_AT5 {
        if valid_rows[row] {
            let choice = idsf_mode3_sfc_row_cost(&block.shifted_rows[row], channel.band_count)?;
            costs[row] = choice.0 + 4;
            selectors[row] = choice.1;
        }
    }

    if let Some(choice) = idsf_mode3_sg_transformed_cost(channel, block)? {
        costs[3] = choice.0 + 0x10;
        selectors[3] = choice.1;
    }

    let mut mode_selector = 0;
    let mut bits = costs[0];
    let mut huffman_selector = selectors[0];
    for index in 1..4 {
        if costs[index] < bits && costs[index] > 0 {
            bits = costs[index];
            mode_selector = index;
            huffman_selector = selectors[index];
        }
    }

    Ok(IdsfMode3Choice {
        bits,
        mode_selector,
        huffman_selector,
    })
}

fn idsf_mode3_sfc_row_cost(
    values: &[i32; IDSF_BAND_LIMIT_AT5],
    count: usize,
) -> Result<(i32, usize), BitcountError> {
    let descriptors = sfc_descriptors();
    let mut costs = [6; 4];
    for index in 1..count {
        let symbol = (values[index] - values[index - 1]) as u32 & 0x3f;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol as usize)?;
        }
    }

    let selector = strict_min_index(&costs);
    Ok((costs[selector], selector))
}

fn idsf_mode3_sg_transformed_cost(
    channel: &IdsfChannelState<'_>,
    block: &IdsfBlockState,
) -> Result<Option<(i32, usize)>, BitcountError> {
    if channel.band_count == 0 || !(-8..=7).contains(&block.transformed[0]) {
        return Ok(None);
    }

    let descriptors = sfc_sg_descriptors();
    let mut costs = [4; 4];
    for index in 1..channel.band_count {
        let delta = (block.transformed[index] - block.transformed[index - 1]) as u32 & 0x3f;
        if delta.wrapping_sub(8) < 0x31 {
            return Ok(None);
        }
        let symbol = delta & 0x0f;
        for (selector, descriptor) in descriptors.iter().enumerate() {
            costs[selector] += huffman_bit_len(*descriptor, symbol as usize)?;
        }
    }

    let selector = strict_min_index(&costs);
    Ok(Some((costs[selector], selector)))
}

fn idsf_small_residual_for_4bit_field(value: i32) -> bool {
    (-7..=8).contains(&value)
}

fn round_div3_plus_half(value: i32) -> i32 {
    (value * 2 + 3) / 6
}

fn round_div5_plus_half(value: i32) -> i32 {
    (value * 2 + 5) / 10
}

fn wrap_idsf_residual(value: i32) -> i32 {
    if value > 0x1f {
        value - 0x40
    } else if value < -0x20 {
        value + 0x40
    } else {
        value
    }
}

fn wrapped_sub_i32(lhs: i32, rhs: i32, mask: u32) -> usize {
    ((lhs - rhs) as u32 & mask) as usize
}

fn strict_min_index(costs: &[i32; 4]) -> usize {
    let mut best = 0;
    let mut best_cost = costs[0];
    for (index, cost) in costs.iter().enumerate().skip(1) {
        if *cost < best_cost {
            best = index;
            best_cost = *cost;
        }
    }
    best
}

fn validate_idct_inputs(channels: &[IdctChannelState<'_>]) -> Result<(), BitcountError> {
    for (channel_index, channel) in channels.iter().enumerate() {
        if channel.band_count > IDCT_BAND_LIMIT_AT5 {
            return Err(BitcountError::BandCountTooLarge {
                channel: channel_index,
                count: channel.band_count,
                max: IDCT_BAND_LIMIT_AT5,
            });
        }
        if channel.bandwidth_mode >= IDCT_FIXBITS_AT5_ENTRIES {
            return Err(BitcountError::BandwidthModeOutOfRange {
                channel: channel_index,
                mode: channel.bandwidth_mode,
                max: IDCT_FIXBITS_AT5_ENTRIES - 1,
            });
        }
        if channel.idct_source.len() < channel.band_count {
            return Err(BitcountError::IdctSourceTooShort {
                channel: channel_index,
                needed: channel.band_count,
                actual: channel.idct_source.len(),
            });
        }
        if channel.mode != 0 && channel.previous_idct_source.len() < channel.band_count {
            return Err(BitcountError::PreviousIdctSourceTooShort {
                channel: channel_index,
                needed: channel.band_count,
                actual: channel.previous_idct_source.len(),
            });
        }
    }

    Ok(())
}

fn idct_selector_active_count(full_count: usize, aux: &[u32; IDCT_BAND_LIMIT_AT5]) -> usize {
    if full_count == 0 {
        return 0;
    }

    let mut index = full_count - 1;
    loop {
        if aux[index] > 0 {
            return index + 1;
        }
        if index == 0 {
            return full_count;
        }
        index -= 1;
    }
}

fn select_idct_selector_cost(
    channel: &IdctChannelState<'_>,
    block: &IdctBlockState,
    row_fixbits: i32,
) -> Result<IdctCost, BitcountError> {
    let full_count = channel.band_count;
    let active_count = block.band_count;
    let mut best = idct_count_choice(
        0,
        fixed_idct_cost(&block.flags, active_count, row_fixbits),
        fixed_idct_cost(&block.flags, full_count, row_fixbits),
    );

    for candidate in [
        idct_mode1_cost(channel, block)?,
        idct_mode2_cost(channel, block)?,
        idct_mode3_or_4_cost(channel, block)?,
    ] {
        if candidate.bits < best.bits {
            best = candidate;
        }
    }

    Ok(best)
}

fn fixed_idct_cost(flags: &[u32; IDCT_BAND_LIMIT_AT5], count: usize, row_fixbits: i32) -> i32 {
    flags[..count]
        .iter()
        .map(|flag| match *flag {
            1 => row_fixbits,
            2 => 1,
            _ => 0,
        })
        .sum()
}

fn idct_count_choice(mode: u32, prefix_bits: i32, full_bits: i32) -> IdctCost {
    if prefix_bits + 5 < full_bits {
        IdctCost {
            mode,
            split_flag: 1,
            bits: prefix_bits + 6,
        }
    } else {
        IdctCost {
            mode,
            split_flag: 0,
            bits: full_bits + 1,
        }
    }
}

fn idct_mode1_cost(
    channel: &IdctChannelState<'_>,
    block: &IdctBlockState,
) -> Result<IdctCost, BitcountError> {
    let descriptor = if channel.bandwidth_mode == 0 {
        ct_a()
    } else {
        ct_b()
    };
    Ok(idct_count_choice(
        1,
        direct_huffman_idct_cost(&block.flags, &block.aux, block.band_count, descriptor)?,
        direct_huffman_idct_cost(&block.flags, &block.aux, channel.band_count, descriptor)?,
    ))
}

fn idct_mode2_cost(
    channel: &IdctChannelState<'_>,
    block: &IdctBlockState,
) -> Result<IdctCost, BitcountError> {
    let (first, delta) = if channel.bandwidth_mode == 0 {
        (ct_a(), ct_a())
    } else {
        (ct_b(), ct_c())
    };
    Ok(idct_count_choice(
        2,
        delta_huffman_idct_cost(&block.flags, &block.aux, block.band_count, first, delta)?,
        delta_huffman_idct_cost(&block.flags, &block.aux, channel.band_count, first, delta)?,
    ))
}

fn idct_mode3_or_4_cost(
    channel: &IdctChannelState<'_>,
    block: &IdctBlockState,
) -> Result<IdctCost, BitcountError> {
    if channel.mode == 0 {
        let has_positive_aux = block.flags[..channel.band_count]
            .iter()
            .zip(block.aux[..channel.band_count].iter())
            .any(|(flag, value)| *flag == 1 && *value > 0);
        return Ok(IdctCost {
            mode: 3,
            split_flag: 0,
            bits: if has_positive_aux { 0x4000 } else { 0 },
        });
    }

    let descriptor = if channel.bandwidth_mode == 0 {
        ct_a()
    } else {
        ct_d()
    };
    Ok(idct_count_choice(
        3,
        previous_huffman_idct_cost(
            &block.flags,
            &block.aux,
            &block.previous,
            block.band_count,
            descriptor,
        )?,
        previous_huffman_idct_cost(
            &block.flags,
            &block.aux,
            &block.previous,
            channel.band_count,
            descriptor,
        )?,
    ))
}

fn direct_huffman_idct_cost(
    flags: &[u32; IDCT_BAND_LIMIT_AT5],
    values: &[u32; IDCT_BAND_LIMIT_AT5],
    count: usize,
    descriptor: HuffmanDescriptor,
) -> Result<i32, BitcountError> {
    let mut bits = 0;
    for index in 0..count {
        match flags[index] {
            1 => bits += huffman_bit_len(descriptor, values[index] as usize)?,
            2 => bits += 1,
            _ => {}
        }
    }
    Ok(bits)
}

fn delta_huffman_idct_cost(
    flags: &[u32; IDCT_BAND_LIMIT_AT5],
    values: &[u32; IDCT_BAND_LIMIT_AT5],
    count: usize,
    first_descriptor: HuffmanDescriptor,
    delta_descriptor: HuffmanDescriptor,
) -> Result<i32, BitcountError> {
    // Native calc_nbits_for_idct_at5 mode-2 block (0x2c0b0, decompile 24060-24101)
    // keys the first descriptor STRICTLY on band index 0 (`if (*piVar6 == 1)`),
    // exactly like pack_idct_2_at5 (0x23d70). A flag-0 or flag-2 band 0 leaves the
    // running previous at 0, so the first flag-1 band at index >= 1 is a DELTA
    // against 0 with the delta descriptor -- not the first descriptor at the raw
    // value. The two-pass caller (prefix over block.band_count, full over
    // channel.band_count) walks the identical running-previous chain over its
    // shared prefix, matching native's single-pass-with-continuation structure.
    let mut bits = 0;
    let mut previous_value = 0_u32;
    let symbol_mask = u32::from(delta_descriptor.symbol_mask());

    for index in 0..count {
        match flags[index] {
            1 => {
                let (descriptor, symbol) = if index == 0 {
                    (first_descriptor, values[index])
                } else {
                    (
                        delta_descriptor,
                        values[index].wrapping_sub(previous_value) & symbol_mask,
                    )
                };
                bits += huffman_bit_len(descriptor, symbol as usize)?;
                previous_value = values[index];
            }
            2 => bits += 1,
            _ => {}
        }
    }

    Ok(bits)
}

fn previous_huffman_idct_cost(
    flags: &[u32; IDCT_BAND_LIMIT_AT5],
    values: &[u32; IDCT_BAND_LIMIT_AT5],
    previous: &[u32; IDCT_BAND_LIMIT_AT5],
    count: usize,
    descriptor: HuffmanDescriptor,
) -> Result<i32, BitcountError> {
    let symbol_mask = u32::from(descriptor.symbol_mask());
    let mut bits = 0;
    for index in 0..count {
        match flags[index] {
            1 => {
                let symbol = values[index].wrapping_sub(previous[index]) & symbol_mask;
                bits += huffman_bit_len(descriptor, symbol as usize)?;
            }
            2 => bits += 1,
            _ => {}
        }
    }
    Ok(bits)
}

fn huffman_bit_len(descriptor: HuffmanDescriptor, symbol: usize) -> Result<i32, BitcountError> {
    huffman_entry(descriptor, symbol)
        .map(|entry| i32::from(entry.bit_len))
        .map_err(|error| BitcountError::HuffmanSymbolOutOfRange {
            descriptor: error.descriptor,
            symbol: error.symbol,
        })
}
