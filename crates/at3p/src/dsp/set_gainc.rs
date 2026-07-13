//! Pure port of the native low-rate gain detector `set_gainc_at5`
//! (native 0x36020, size 14640; decompile `decompiled/libatrac.c` 29190-31150).
//!
//! `time2freq_at5` dispatches this per (descending band 15..0, channel) when
//! `cfg+0xcc == 0` (the 64/48 kbps mode) instead of the 352-path
//! `detect_gainc_data_new_at5` (dispatch gate decompile 33012). This module is
//! UNWIRED: nothing in the shipping encode path calls it yet (docs/13 §5.2
//! slice 2 is the leaf only).
//!
//! Boundary contract (runtime-pinned, docs/13 progress (jjj); oracle schema
//! `atx_set_gainc_io_trace_v1`):
//! - `param_2` scratch: the leaf reads floats `[+0x3fc, +0x610)` only. Here
//!   `scratch[0]` = native `param_2+0x3fc`, so `scratch[1]` = `+0x400` (the
//!   peak-scan base) and `scratch[129..133]` = `+0x600..+0x610`.
//! - `param_5` = previous gain-record plane (`*(chobj+0xc)`), read-only.
//! - `param_6` = current gain-record plane (`*(chobj+0x8)`), zeroed by the
//!   caller at t2f entry except caller-written fields (+0x54, +0x64, and at
//!   band 0 the band-1 row written by the previous call). 16 rows x 0x98 bytes.
//! - Two per-band history rows in the channel object are read at entry and
//!   written back: row A (0x21 floats at `chobj+0x34+band*0x84`) and row B
//!   (0x20 floats at `chobj+0x874+band*0x80`, only when `*(header+0x1c) != 2`).
//!
//! Ghidra-vs-disasm corrections baked into this port (all pinned against the
//! - The decompile DROPS three hidden running-max-index locals kept in the
//!   pairwise flatten loops (native 0x3663b-0x366b2 and 0x366d4-0x3674f):
//!   `[ebp-0x18a4]`/`[ebp-0x18a8]` (primary, unbounded/bounded) and
//!   `[ebp-0x18c8]`/`[ebp-0x18cc]` (half-diff side). They are the regparm
//!   `high_index`/`low_index` arguments of all six attack/release calls.
//! - regparm args (native): attack#1 eax=min(idx_a4,0x20) @0x36af1, attack#2
//!   eax=min(idx_c8,0x20) @0x36d2c, attack#3 eax=min(idx_a4,0x1e) @0x372f9;
//!   release#1/#3 eax=idx_a8 (@0x36f69/0x374f2), release#2 eax=idx_cc
//!   (@0x37113); release other_point_count ecx = attack count `[ebp-0x1898]`
//!   (#1/#3) resp. half-diff attack count `[ebp-0x18d4]` (#2).
//! - attack#2's total/limit/current seeds are COPIES of the primary slots
//!   taken BEFORE attack#1 runs (native 0x36a7c-0x36a94), not fresh zeros.
//! - attack#1's `fractional` argument is 0 exactly on the band-0 threshold
//!   boost path (band==0 && selector<=0x1a && row[+0x64]>=10.0, native
//!   0x36aa6-0x36af0) and a guaranteed-nonzero flag-garbage word otherwise.
//! - the `1 < local_18a4` gate at decompile 30589 reads the two-sided flag
//!   slot `[ebp-0x18a0]` (native 0x37e39), NOT a hidden index.
//! - `(int)ROUND(x)` sites use fldcw 0xc00 truncation (native 0x36918-0x3692d
//!   etc.), i.e. trunc toward zero, matching `trunc_to_i32` in `dsp::gain`.
//! - all float constants are f32 dwords from the pool at native 0xC1C08..
//!   (log2e=0x3fb8aa3b, 1.414=0x3fb4fdf4, 1.65=0x3fd33333, ...).
//! - `sa_coef_B` (native 0xBF980, nm-named) and `sa_distance` (0xBF9C0) are
//!   embedded below from the reference binary bytes.
//!
//! x87 modeling notes (documented per docs/13 (kkk)):
//! - per-step f32 stores in the peak/sum loops are exact f32 chains (native
//!   stores each iteration, e.g. 0x361f8-0x36208);
//! - the bin mean (native 0x386ed-0x386ff) accumulates in st(0); 32 f32 adds
//!   are exact in f64, so f64 accumulation + one f32 rounding is bit-exact;
//! - the merge weighted sums/compares are register-resident natively and are
//!   modeled in f64 (products of two f32 are exact in f64);
//! - `log()` uses Rust/libm f64 ln like the validated `dsp::gain` wrappers.

use crate::dsp::gain::{
    CheckGcCandidate, CheckGcConfig, CheckGcRecord, GainPassError, GainPassPoints, attack_pass_at5,
    check_gc_at5, release_pass_at5,
};
use crate::tables::at5::lngain_at5;

pub const SET_GAINC_ROW_WORDS: usize = 38; // 0x98 bytes
pub const SET_GAINC_BANDS: usize = 16;
pub const SET_GAINC_SCRATCH_FLOATS: usize = 133; // bytes [+0x3fc, +0x610)
pub const SET_GAINC_HISTORY_A_FLOATS: usize = 33;
pub const SET_GAINC_HISTORY_B_FLOATS: usize = 32;

/// One 0x98-byte gain-record row as raw little-endian words.
pub type SetGaincRow = [u32; SET_GAINC_ROW_WORDS];
/// One 16-band gain-record plane (native `*(chobj+0x8)` / `*(chobj+0xc)`).
pub type SetGaincPlane = [SetGaincRow; SET_GAINC_BANDS];

/// `sa_coef_B` (native .rodata 0xBF980, 16 f32): [2.0, 1.9, 1.9, 1.75, 1.65 x12].
const SA_COEF_B_BITS: [u32; 16] = [
    0x4000_0000,
    0x3ff3_3333,
    0x3ff3_3333,
    0x3fe0_0000,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
    0x3fd3_3333,
];

/// `sa_distance` (native .rodata 0xBF9C0, ints indexed by segment code 0..10).
const SA_DISTANCE: [i32; 11] = [6, 5, 5, 0, 5, 5, 5, 0, 5, 5, 5];

/// f32 log2(e) constant bits 0x3fb8aa3b (native pool 0xC1C1C).
const LOG2E_F32: f32 = f32::from_bits(0x3fb8_aa3b);
/// f32 1.414 constant bits 0x3fb4fdf4 (native pool 0xC1C34).
const SQRT2_F32: f32 = f32::from_bits(0x3fb4_fdf4);
/// f32 1.65 constant bits 0x3fd33333 (native pool 0xC1C44).
const COEF_1_65_F32: f32 = f32::from_bits(0x3fd3_3333);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetGaincError {
    Pass(GainPassError),
    BandOutOfRange {
        band: usize,
    },
    PrevRecordCountOutOfRange {
        count: i32,
    },
    SegmentOverflow {
        count: i32,
    },
    DeferredQueueOverflow {
        count: i32,
    },
    /// The undo-selection sort found no candidate (native would write a
    /// used-flag one slot below the array; fail-explicit instead).
    UndoSelectionExhausted,
}

impl From<GainPassError> for SetGaincError {
    fn from(error: GainPassError) -> Self {
        Self::Pass(error)
    }
}

#[inline]
fn row_f32(row: &SetGaincRow, byte: usize) -> f32 {
    f32::from_bits(row[byte / 4])
}

#[inline]
fn row_i32(row: &SetGaincRow, byte: usize) -> i32 {
    row[byte / 4] as i32
}

#[inline]
fn row_set_f32(row: &mut SetGaincRow, byte: usize, value: f32) {
    row[byte / 4] = value.to_bits();
}

#[inline]
fn row_set_i32(row: &mut SetGaincRow, byte: usize, value: i32) {
    row[byte / 4] = value as u32;
}

/// Native `1 << (g & 0x1f)` / reciprocal scale pattern (e.g. decompile 30120-30126).
#[inline]
fn pow2_scale(g: i32) -> f32 {
    let byte = g as u8;
    if g < 0 {
        let shift = byte.wrapping_neg() & 0x1f;
        1.0 / (1i32.wrapping_shl(u32::from(shift)) as f32)
    } else {
        let shift = byte & 0x1f;
        1i32.wrapping_shl(u32::from(shift)) as f32
    }
}

/// Native fldcw-0xc00 fistp: truncation toward zero (disasm 0x36918-0x3692d).
#[inline]
fn trunc_i32(value: f64) -> i32 {
    value.trunc() as i32
}

#[inline]
fn sa_distance(code: i32) -> i32 {
    SA_DISTANCE[code as usize]
}

// NOTE: the decompile displays several of these running-max scans as
// write-backs into the scanned cell (e.g. 29596 `local_13c[0] = fVar2`).
// The disassembly shows ZERO stores to any of those cells (grep over
// [ebp-0x138], [ebp-0x188], [ebp-0x1c8..0x1d0], [ebp-0xb38], [ebp-0xaf8],
// [ebp-0x9e8]): all such maxima are x87-register-resident and every later
// reader of those array cells loads the PRISTINE memory value.

/// 16-entry running-max scan without write-back, native update condition
/// `!(v <= cur)` (decompile 29607-29614).
fn max16_ge(arr: &[f32], base: usize) -> f32 {
    let mut current = arr[base];
    for index in 1..0x10 {
        let value = arr[base + index];
        if !(value <= current) {
            current = value;
        }
    }
    current
}

/// 16-entry running-max scan without write-back, strict `<` update
/// (decompile 29768-29777 / 29829-29838).
fn max16_lt(arr: &[f32], base: usize) -> f32 {
    let mut current = arr[base];
    for index in 1..0x10 {
        let value = arr[base + index];
        if current < value {
            current = value;
        }
    }
    current
}

/// Pairwise flatten loop with the hidden running-max-index outputs Ghidra
/// dropped (native 0x3663b-0x366b2 primary, 0x366d4-0x3674f half-diff side).
///
/// For each pair base `c` in 0,2,..,0x22 over `src[src_base + c]`:
/// the pair max (second element wins only on strict `>`) is written to
/// `dst[c]` and `dst[c+1]`; when it strictly exceeds the running max, the
/// winning element index is recorded unbounded and, when `c <= 0x1f`, bounded.
fn flatten_pairs(dst_src: &mut [f32], dst_base: usize, src_base: usize) -> (i32, i32) {
    let mut run_max = 0.0f32;
    let mut idx_unbounded = 0i32;
    let mut idx_bounded = 0i32;
    let mut c = 0usize;
    while c <= 0x22 {
        let p = dst_src[src_base + c];
        let q = dst_src[src_base + c + 1];
        let (m, idx) = if q > p {
            (q, (c + 1) as i32)
        } else {
            (p, c as i32)
        };
        dst_src[dst_base + c] = m;
        dst_src[dst_base + c + 1] = m;
        if run_max < m {
            run_max = m;
            idx_unbounded = idx;
            if c <= 0x1f {
                idx_bounded = idx;
            }
        }
        c += 2;
    }
    (idx_unbounded, idx_bounded)
}

/// Suffix-cumulate a levels array in place (decompile 30374-30398).
fn suffix_cumulate(levels: &mut [i32; 7], count: i32) {
    let mut acc = 0i32;
    let mut k = count;
    while k >= 1 {
        acc += levels[(k - 1) as usize];
        levels[(k - 1) as usize] = acc;
        k -= 1;
    }
}

/// Build the net gain step curve into `curve[1..0x22]` from ascending attack
/// points and descending release points, then subtract the top entry
/// (decompile 30400-30439; same shape reused in the merge blocks 30064-30102).
#[allow(clippy::too_many_arguments)]
fn build_curve(
    curve: &mut [i32; 40],
    atk_locations: &[i32; 7],
    atk_values: &[i32],
    atk_count: i32,
    rel_locations: &[i32; 7],
    rel_values: &[i32],
    rel_count: i32,
) {
    for slot in curve[1..0x22].iter_mut() {
        *slot = 0;
    }
    let mut pos = 0i32;
    for i in 0..atk_count.max(0) as usize {
        let location = atk_locations[i];
        if pos <= location {
            let value = atk_values[i];
            while pos <= location {
                curve[(pos + 1) as usize] += value;
                pos += 1;
            }
        }
    }
    let mut pos = 0x20i32;
    for i in 0..rel_count.max(0) as usize {
        let location = rel_locations[i];
        if location <= pos {
            let value = rel_values[i];
            while location <= pos {
                curve[(pos + 1) as usize] += value;
                pos -= 1;
            }
        }
    }
    let top = curve[0x21];
    for k in 0..=0x20usize {
        curve[k + 1] -= top;
    }
}

/// Pure `set_gainc_at5` (native 0x36020). `band` is the descending band index
/// (`param_3`), `band_count` is `param_7`, `selector`/`channel_count` are
/// `cfg+0x1e8`/`cfg+0xa8`, `header_1c` is `*(header+0x1c)`.
///
/// The native `param_4` (channel index) is only used to select the channel
/// object whose history rows and planes are passed here directly, so it is
/// not a parameter of the pure function.
#[allow(clippy::too_many_arguments)]
// Slot mirrors (s1850/s1858) keep the native trailing stores even where the
// value is provably dead afterwards; the port stays line-mechanical.
#[allow(unused_assignments)]
pub fn set_gainc_at5(
    band: usize,
    band_count: i32,
    selector: i32,
    channel_count: i32,
    header_1c: u32,
    scratch: &[f32; SET_GAINC_SCRATCH_FLOATS],
    history_a: &mut [f32; SET_GAINC_HISTORY_A_FLOATS],
    history_b: &mut [f32; SET_GAINC_HISTORY_B_FLOATS],
    prev_plane: &SetGaincPlane,
    cur_plane: &mut SetGaincPlane,
) -> Result<(), SetGaincError> {
    if band >= SET_GAINC_BANDS {
        return Err(SetGaincError::BandOutOfRange { band });
    }

    // --- entry flags (decompile 29350-29359; flag slot [ebp-0x18a0]) ---
    let flag2 = (band as i32) < band_count
        && ((channel_count == 2 && (selector.wrapping_sub(0xc) as u32) < 4)
            || (selector < 0x10 && channel_count == 1));
    // bVar27: `*(header+0x1c) != 2`
    let fractional = header_1c != 2;

    // --- stage 1: primary envelope timeline afStack_250 (29361-29421) ---
    // env[1..0x22] = history A (prev peaks + prev lookahead), env[0x21..0x41] =
    // this frame's 32 group peaks, env[0x41] = the +0x600 lookahead (local_14c).
    let mut env = [0.0f32; 0x42];
    env[1..0x22].copy_from_slice(&history_a[..]);
    let mut sum_main = 0.0f32; // local_18e4 (f32 per-step store, native 0x361f8)
    for group in 0..0x20usize {
        let mut peak = 4.0f32;
        for j in 0..4usize {
            let value = scratch[1 + group * 4 + j].abs();
            if peak < value {
                peak = value;
            }
        }
        env[0x21 + group] = peak;
        sum_main = peak + sum_main;
    }
    // +0x600 lookahead peak (29395-29413)
    let mut peek = scratch[129].abs();
    for j in 1..4usize {
        let value = scratch[129 + j].abs();
        if peek < value {
            peek = value;
        }
    }
    env[0x41] = if 4.0 <= peek { peek } else { 4.0 };
    history_a.copy_from_slice(&env[0x21..0x42]);

    // --- stage 2: streak counting over the current peaks (29423-29476) ---
    let prev_row = &prev_plane[band];
    let prev_6c = row_i32(prev_row, 0x6c);
    let prev_68 = row_i32(prev_row, 0x68);
    // Native 0x362e1-0x36330: cur[0x50] = max(env[0x1f], env[0x20]) computed in
    // st registers; env is NOT written (Ghidra shows spurious array stores).
    let cur50 = if env[0x20] > env[0x1f] {
        env[0x20]
    } else {
        env[0x1f]
    };
    row_set_f32(&mut cur_plane[band], 0x50, cur50);
    // Alternation loop (native 0x36334-0x36449): the running reference is an
    // x87 register seeded with max(env[0x20], env[0x21]); env stays pristine.
    let mut transitions = 0i32; // local_18f8
    {
        let mut reference = if env[0x20] > env[0x21] {
            env[0x20]
        } else {
            env[0x21]
        };
        let mut dir_state = 0i32; // iVar18 (edx)
        let mut dir_mode = 0i32; // iVar20 (esi)
        let mut run = 3i32; // local_1a58
        let mut i = 1usize; // iVar13 (ecx)
        while i < 0x20 {
            let value = env[0x21 + i];
            if value <= reference {
                if value * 4.0 <= reference {
                    reference = value;
                    if dir_state != 2 {
                        if dir_mode != 2 {
                            dir_mode = 2;
                            dir_state = 2;
                            // LAB_463af
                            transitions += 1;
                            run = 1;
                            i += 1;
                            continue;
                        }
                        dir_state = 2;
                        // LAB_46756
                        run = 0;
                    }
                }
                // LAB_46432
                run += 1;
                i += 1;
            } else {
                let big = reference * 4.0;
                reference = value;
                if value < big {
                    run += 1;
                    i += 1;
                    continue;
                }
                if dir_state == 1 || run < 3 || dir_mode == 1 {
                    dir_state = 1;
                    // LAB_46756 then LAB_46432
                    run = 1;
                    i += 1;
                    continue;
                }
                dir_mode = 1;
                dir_state = 1;
                // LAB_463af
                transitions += 1;
                run = 1;
                i += 1;
            }
        }
    }
    let streaky = prev_6c > 3 || prev_68 > 3 || transitions > 3; // bVar7

    // --- stage 3: half-difference envelope local_b3c (29478-29530) ---
    let mut b3c = [0.0f32; 0x65];
    let mut sum_diff = 0.0f32; // local_18e8
    if fractional {
        let mut hd = [0.0f32; 0x80];
        for j in 0..0x80usize {
            hd[j] = (scratch[j] - scratch[j + 1]) * 0.5;
        }
        b3c[0x24..0x44].copy_from_slice(&history_b[..]);
        let mut cursor = 0usize;
        for group in 0..0x20usize {
            let mut peak = 4.0f32; // local_18c4 = local_18d4(4.0)
            for _ in 0..4usize {
                let value = hd[cursor].abs();
                cursor += 1;
                if peak < value {
                    peak = value;
                }
            }
            sum_diff += peak;
            b3c[0x44 + group] = peak;
        }
        b3c[0x64] = 4.0; // local_9ac lookahead placeholder
        history_b.copy_from_slice(&b3c[0x44..0x64]);
        row_set_f32(&mut cur_plane[band], 0x90, sum_main);
        row_set_f32(&mut cur_plane[band], 0x94, sum_diff);
        sum_main = row_f32(prev_row, 0x90);
        sum_diff = row_f32(prev_row, 0x94);
    }

    // --- stage 4: primary pairwise flatten + hidden indices (29531-29552) ---
    let mut flat = [0.0f32; 36]; // local_13c
    let (idx_a4, idx_a8) = {
        // native 0x3663b-0x366b2 over env pairs (pfVar1[c], pfVar1[c+1])
        let mut run_max = 0.0f32;
        let mut unbounded = 0i32;
        let mut bounded = 0i32;
        let mut c = 0usize;
        while c <= 0x22 {
            let p = env[1 + c];
            let q = env[2 + c];
            let (m, idx) = if q > p {
                (q, (c + 1) as i32)
            } else {
                (p, c as i32)
            };
            flat[c] = m;
            flat[c + 1] = m;
            if run_max < m {
                run_max = m;
                unbounded = idx;
                if c <= 0x1f {
                    bounded = idx;
                }
            }
            c += 2;
        }
        (unbounded, bounded)
    };

    // --- stage 5: half-diff pairwise flatten (29554-29577) ---
    let (idx_c8, idx_cc) = if fractional {
        flatten_pairs(&mut b3c, 0, 0x24)
    } else {
        (0, 0)
    };

    // --- stage 6: previous-level seed (29579-29592) ---
    let prev_count = row_i32(prev_row, 0);
    if !(0..=7).contains(&prev_count) {
        return Err(SetGaincError::PrevRecordCountOutOfRange { count: prev_count });
    }
    let mut level_max = 6i32;
    for k in 0..prev_count as usize {
        let level = prev_row[8 + k] as i32;
        if level_max < level {
            level_max = level;
        }
    }
    // primary slot set (named by native ebp offsets; roles rotate mid-function)
    let mut s1858 = level_max - 6; // total-level, later release current-level
    let mut s1854; // level limit slot
    let mut s1850 = 0i32; // attack current-level
    let mut s185c; // first-rounding flag

    // --- stage 7: reference peaks + limit (29593-29636) ---
    let prev_74 = row_f32(prev_row, 0x74);
    let mut fv9 = prev_74;
    let lower_max = max16_lt(&flat, 0); // fVar2 (register max; flat[0] pristine)
    let upper_max = max16_ge(&flat, 0x10); // local_1900
    row_set_f32(&mut cur_plane[band], 0x4c, upper_max);
    let overall_max = if upper_max < lower_max {
        lower_max
    } else {
        upper_max
    };
    row_set_f32(&mut cur_plane[band], 0x48, overall_max);
    if lower_max * 1.5 < fv9 {
        fv9 = lower_max;
    } else if fv9 <= lower_max {
        // keep fv9
    } else if lower_max * 1.5 < upper_max {
        fv9 = lower_max;
    }
    // fVar2 (attack reference peak, spilled at native 0x368f7 to [ebp-0x18ac])
    let mut fv2 = fv9;
    if prev_count > 0 {
        fv2 = prev_74;
    }
    let prev_70 = row_f32(prev_row, 0x70);
    let limit_ln = (65536.0f64 / f64::from(prev_70)).ln(); // dVar29
    s185c = row_i32(prev_row, 0x78);
    s1854 = trunc_i32(limit_ln * f64::from(LOG2E_F32));
    let mut thr = row_f32(&cur_plane[band], 0x54) * 1.5; // fVar15

    // --- stage 8 (fractional): half-diff prep BEFORE attack#1 (29637-29677) ---
    let mut b3c_peak = 0.0f32; // local_18c4
    let mut b3c_prev_peak = 0.0f32; // local_18c8
    let mut b3c_upper = 4.0f32; // local_18d4
    let mut s1868 = 0i32;
    let mut s1864 = 0i32;
    let mut s1860 = 0i32;
    let mut s186c = 0i32; // local_186c (b3c first-rounding flag, entry init 0)
    if fractional {
        let mut p84 = row_f32(prev_row, 0x84);
        let b3c_lower = max16_lt(&b3c, 0); // fVar10 (register max)
        b3c_upper = max16_lt(&b3c, 0x10); // local_18d4 (register max)
        let b3c_overall = if b3c_upper < b3c_lower {
            b3c_lower
        } else {
            b3c_upper
        };
        row_set_f32(&mut cur_plane[band], 0x7c, b3c_overall);
        row_set_f32(&mut cur_plane[band], 0x80, b3c_upper);
        if b3c_lower * 1.5 < p84 || (b3c_lower < p84 && b3c_lower * 1.5 < b3c_upper) {
            p84 = b3c_lower;
        }
        b3c_prev_peak = p84;
        if prev_count > 0 {
            p84 = row_f32(prev_row, 0x84);
        }
        b3c_peak = p84;
        // native 0x36a7c-0x36a94: copies of the PRIMARY slots pre-attack#1
        s1868 = s1858;
        s1864 = s1854;
        s1860 = s1850;
    }

    // --- stage 9: band-0 threshold boost (29678-29687, native 0x36aa6) ---
    let cur_64 = row_f32(&cur_plane[band], 0x64);
    let mut a1_fractional = true;
    if band == 0 && selector <= 0x1a && cur_64 >= 10.0 {
        thr *= 1.5;
        a1_fractional = false; // native p14 = 0 on this path only
    }

    // --- attack pass #1 (29688; call 0x36b74) ---
    let mut atk_points = GainPassPoints::default(); // &local_2ac block
    let mut atk_count = attack_pass_at5(
        idx_a4.min(0x20) as usize,
        1,
        0,
        0,
        &mut s185c,
        &mut s1858,
        s1854,
        &mut s1850,
        &env[1..],
        fv2,
        fv9,
        thr,
        &mut atk_points,
        a1_fractional,
    )?;

    // --- band-0 attack glue (29690-29722) ---
    if band == 0 {
        let c54 = row_f32(&cur_plane[0], 0x54);
        if c54 == 1.0 && row_f32(&cur_plane[0], 0x64) <= 15.0 {
            let r1_58 = row_i32(&cur_plane[1], 0x58);
            let r1_40 = row_i32(&cur_plane[1], 0x40);
            let r1_5c = row_i32(&cur_plane[1], 0x5c);
            let r1_60 = row_i32(&cur_plane[1], 0x60);
            let r1_48 = row_f32(&cur_plane[1], 0x48);
            let r0_48 = row_f32(&cur_plane[0], 0x48);
            if r1_58 > 0
                && r1_40 > 1
                && r1_5c < r1_60
                && s1858 < s1854
                && atk_count == 0
                && r1_48 < r0_48
            {
                atk_points.levels[atk_count] = 1;
                atk_points.locations[atk_count] = r1_5c;
                atk_count += 1;
            }
            let mut level_sum = 0i32;
            for k in 0..atk_count {
                level_sum += atk_points.levels[k];
            }
            if ((atk_count as i32).wrapping_sub(1) as u32) < 2
                && r1_48 < r0_48
                && level_sum < 3
                && s1850 < s1854
                && r1_40 > 3
            {
                atk_points.levels[0] += 1;
                if atk_count == 1 {
                    atk_points.locations[0] = r1_5c;
                }
            }
        }
    }

    // --- attack summary stores (29724-29741) ---
    let mut atk_level_sum = 0i32;
    for k in 0..atk_count {
        atk_level_sum += atk_points.levels[k];
    }
    atk_points.reserved = atk_count as i32; // local_2ac
    row_set_i32(&mut cur_plane[band], 0x58, atk_count as i32);
    if atk_count > 0 {
        row_set_i32(&mut cur_plane[band], 0x5c, atk_points.locations[0]);
    }
    row_set_i32(&mut cur_plane[band], 0x40, atk_level_sum);

    // --- attack pass #2, half-diff side (29742-29755; call 0x36d8c) ---
    let mut b3c_atk_points = GainPassPoints::default(); // &local_b9c block
    let mut b3c_atk_count = 0usize; // local_18d8
    if fractional {
        b3c_atk_count = attack_pass_at5(
            idx_c8.min(0x20) as usize,
            1,
            0,
            0,
            &mut s186c,
            &mut s1868,
            s1864,
            &mut s1860,
            &b3c[0x24..],
            b3c_peak,
            b3c_prev_peak,
            thr,
            &mut b3c_atk_points,
            true,
        )?;
        b3c_atk_points.reserved = b3c_atk_count as i32; // local_b9c
        let mut level_sum = 0i32;
        for k in 0..b3c_atk_count {
            level_sum += b3c_atk_points.levels[k];
        }
        row_set_i32(&mut cur_plane[band], 0x88, level_sum);
    }

    // --- release limit setup (29757-29766) ---
    {
        let deficit =
            row_i32(prev_row, 0x40) + row_i32(&cur_plane[band], 0x40) - row_i32(prev_row, 0x44);
        s1854 = if deficit < 0 { deficit + 6 } else { 6 };
        s1858 = 0;
    }

    // --- release reference selection (29767-29804) ---
    let cur_lower_max = max16_lt(&env, 0x21); // fVar15/fVar9 over current peaks[0..0x10)
    let cur_upper_max = max16_lt(&env, 0x31); // local_1a50 (register max)
    let mut rel_sel = if cur_upper_max < cur_lower_max {
        cur_lower_max
    } else {
        cur_upper_max
    };
    if cur_lower_max * 1.75 < rel_sel {
        rel_sel = cur_lower_max;
    }
    // fVar9 (release reference peak, spilled at native 0x36ef4 to [ebp-0x18b0])
    let mut fv9r = rel_sel;
    if upper_max * 1.75 < rel_sel {
        fv9r = upper_max;
    }
    let rel_bvar6 = upper_max * 1.75 < rel_sel; // bVar6
    let mut rel_count = 0usize; // local_18a0
    let mut peak_out = fv9r; // local_1870 (native 0x36f09, unconditional)
    let thr_r = row_f32(&cur_plane[band], 0x54) * 1.75; // fVar15
    let mut rel_points = GainPassPoints::default(); // &local_30c block

    // --- release pass #1 (29805-29816; call 0x36f6f) ---
    if atk_count >= 1 || row_f32(&cur_plane[band], 0x64) <= 15.0 {
        rel_count = release_pass_at5(
            idx_a8 as usize,
            2,
            atk_count,
            0,
            i32::from(rel_bvar6),
            &mut s1858,
            s1854,
            &mut peak_out,
            &flat[..],
            &env[1..],
            fv9r,
            thr_r,
            &mut rel_points,
            fractional,
        )?;
    }

    // --- release pass #2, half-diff side (29817-29876; call 0x37119) ---
    let mut b3c_rel_points = GainPassPoints::default(); // &local_bfc block
    let mut b3c_rel_count = 0usize; // local_18dc
    let mut b3c_first_mode = 0i32; // local_18e0
    let mut b3c_relref = 0.0f32; // local_1874
    if fractional {
        let deficit =
            row_i32(prev_row, 0x88) + row_i32(&cur_plane[band], 0x88) - row_i32(prev_row, 0x8c);
        s1864 = if deficit < 0 { deficit + 6 } else { 6 };
        s1868 = 0;
        let b3c_cur_lower = max16_lt(&b3c, 0x44); // fVar10
        let b3c_cur_upper = max16_lt(&b3c, 0x54); // local_1a50 (register max)
        let mut sel = b3c_cur_upper;
        if sel < b3c_cur_lower {
            sel = b3c_cur_lower;
        }
        if b3c_cur_lower * 1.75 < sel {
            sel = b3c_cur_lower;
        }
        let call;
        if sel <= b3c_upper * 1.75 {
            b3c_first_mode = 0;
            b3c_relref = sel;
            b3c_upper = sel;
            if b3c_atk_count >= 1 {
                call = true;
            } else {
                // LAB_476c0
                b3c_relref = b3c_upper;
                call = row_f32(&cur_plane[band], 0x64) <= 15.0;
            }
        } else {
            b3c_first_mode = 1;
            b3c_relref = b3c_upper;
            if b3c_atk_count > 0 {
                call = true;
            } else {
                // LAB_476c0
                b3c_relref = b3c_upper;
                call = row_f32(&cur_plane[band], 0x64) <= 15.0;
            }
        }
        if call {
            let peak = b3c_relref;
            b3c_rel_count = release_pass_at5(
                idx_cc as usize,
                2,
                b3c_atk_count,
                0,
                b3c_first_mode,
                &mut s1868,
                s1864,
                &mut b3c_relref,
                &b3c[..],
                &b3c[0x24..],
                peak,
                thr_r,
                &mut b3c_rel_points,
                true,
            )?;
        }
        b3c_rel_points.reserved = b3c_rel_count as i32; // local_bfc (29875)
    }

    // --- primary rebound append (29877-29912) ---
    if rel_bvar6 && rel_count > 0 && peak_out * 1.75 < env[0x21] {
        let quotient_ln = (f64::from(env[0x21]) / f64::from(peak_out)).ln();
        let boost;
        let mut residual = 0.0f32; // local_19d4
        if fractional {
            // native 0x371e2: log result x f32 log2e, stored to f32 (local_df0)
            let l2 = (quotient_ln * f64::from(LOG2E_F32)) as f32;
            let bias = if atk_count == 0 || l2 < 2.0 {
                0.5f32
            } else {
                atk_points.fractions[atk_count - 1]
            };
            boost = trunc_i32(f64::from(l2) + f64::from(bias));
            residual = l2 - boost as f32;
        } else {
            boost = trunc_i32(quotient_ln * f64::from(LOG2E_F32) + 0.5);
        }
        if boost > 0 {
            if atk_count < 1 || atk_points.locations[atk_count - 1] != 0x1f {
                if atk_count + rel_count < 7 {
                    atk_points.levels[atk_count] = boost;
                    atk_points.locations[atk_count] = 0x1f;
                    if fractional {
                        atk_points.fractions[atk_count] = residual;
                    }
                    atk_count += 1;
                    atk_points.reserved = atk_count as i32;
                }
            } else {
                atk_points.levels[atk_count - 1] = boost;
                if fractional {
                    atk_points.fractions[atk_count - 1] = residual;
                }
            }
        }
    }

    // --- attack pass #3 + summary recompute (29914-29948; call 0x37356) ---
    let mut ran_recompute = atk_count != 0;
    if atk_count == 0 {
        let mut thr3 = row_f32(&cur_plane[band], 0x54) * 1.5;
        if band == 0 {
            thr3 += thr3;
        }
        atk_count = attack_pass_at5(
            idx_a4.min(0x1e) as usize,
            2,
            0,
            rel_count,
            &mut s185c,
            &mut s1858,
            s1854,
            &mut s1850,
            &flat[..],
            fv2,
            f32::from_bits(0xbf80_0000),
            thr3,
            &mut atk_points,
            fractional,
        )?;
        if atk_count != 0 {
            ran_recompute = true;
        }
    }
    if ran_recompute {
        // LAB_47368
        let mut level_sum = 0i32;
        for k in 0..atk_count {
            level_sum += atk_points.levels[k];
        }
        atk_points.reserved = atk_count as i32;
        row_set_i32(&mut cur_plane[band], 0x58, atk_count as i32);
        row_set_i32(&mut cur_plane[band], 0x5c, atk_points.locations[0]);
        row_set_i32(&mut cur_plane[band], 0x40, level_sum);
        let deficit = level_sum + row_i32(prev_row, 0x40) - row_i32(prev_row, 0x44);
        s1854 = if deficit < 0 { deficit + 6 } else { 6 };
        s1858 = 0;
    }

    // --- release pass #3 over the quad-flattened envelope (29949-29980) ---
    if rel_count == 0 && row_f32(&cur_plane[band], 0x64) <= 15.0 {
        let mut quad = [0.0f32; 36]; // local_39c
        let mut i = 0usize;
        while i <= 0x20 {
            let a = flat[i + 2];
            let b = flat[i];
            let m = if a < b { b } else { a };
            quad[i] = m;
            quad[i + 1] = m;
            quad[i + 2] = m;
            quad[i + 3] = m;
            i += 4;
        }
        let mut thr4 = row_f32(&cur_plane[band], 0x54) * 1.75;
        if band == 0 {
            thr4 += thr4;
        }
        rel_count = release_pass_at5(
            idx_a8 as usize,
            4,
            atk_count,
            0,
            i32::from(rel_bvar6),
            &mut s1858,
            s1854,
            &mut peak_out,
            &quad[..],
            &env[1..],
            fv9r,
            thr4,
            &mut rel_points,
            fractional,
        )?;
    }

    // --- release summary stores (29981-29997) ---
    {
        let mut level_sum = 0i32;
        for k in 0..rel_count {
            level_sum += rel_points.levels[k];
        }
        rel_points.reserved = rel_count as i32; // local_30c
        let last_location = if rel_count < 1 {
            0x1f
        } else {
            rel_points.locations[rel_count - 1]
        };
        row_set_i32(&mut cur_plane[band], 0x60, last_location);
        row_set_i32(&mut cur_plane[band], 0x44, level_sum);
    }

    // --- half-diff rebound + cross-list merges (29998-30371) ---
    let mut curve = [0i32; 40]; // aiStack_b0; curve[1+pos] = net gain at bin pos
    let mut w_atk = atk_count; // local_19d8 (working attack count)
    let mut w_rel = rel_count; // local_19dc
    let mut total_points = 0i32; // Ghidra local_1898 ([ebp-0x1894])
    if fractional {
        // half-diff rebound append (29999-30014)
        if b3c_first_mode != 0 && b3c_rel_count > 0 && b3c_relref * 1.75 < b3c[0x44] {
            let level = trunc_i32(
                (f64::from(b3c[0x44]) / f64::from(b3c_relref)).ln() * f64::from(LOG2E_F32) + 0.5,
            );
            if level > 0 {
                if b3c_atk_count < 1 || b3c_atk_points.locations[b3c_atk_count - 1] != 0x1f {
                    if b3c_atk_count + b3c_rel_count < 7 {
                        b3c_atk_points.levels[b3c_atk_count] = level;
                        b3c_atk_points.locations[b3c_atk_count] = 0x1f;
                        b3c_atk_count += 1;
                        b3c_atk_points.reserved = b3c_atk_count as i32;
                    }
                } else {
                    b3c_atk_points.levels[b3c_atk_count - 1] = level;
                }
            }
        }

        total_points = atk_count as i32 + rel_count as i32; // 30019
        if b3c_atk_count > 0 {
            let mut db0 = [0i32; 40]; // aiStack_db0 rank map (entries [pos+1])
            // attack-side maps ascending (30022-30042)
            {
                let mut ia = 0usize;
                for pos in 0..=0x20i32 {
                    if ia < atk_count && atk_points.locations[ia] == pos {
                        db0[pos as usize + 1] = ia as i32 + 8;
                        ia += 1;
                    } else {
                        db0[pos as usize + 1] = ia as i32;
                    }
                    // local_c8c side map is written but never read in this block
                }
            }
            // suffix level sums (30043-30063)
            let mut dcc = [0i32; 8];
            {
                let mut acc = 0i32;
                let mut k = atk_count as i32;
                while k >= 1 {
                    acc += atk_points.levels[(k - 1) as usize];
                    dcc[(k - 1) as usize] = acc;
                    k -= 1;
                }
            }
            let mut dec = [0i32; 8];
            {
                let mut acc = 0i32;
                let mut k = rel_count as i32;
                while k >= 1 {
                    acc += rel_points.levels[(k - 1) as usize];
                    dec[(k - 1) as usize] = acc;
                    k -= 1;
                }
            }
            // step curve from suffix sums (30064-30102)
            build_curve(
                &mut curve,
                &atk_points.locations,
                &dcc[..7],
                w_atk as i32,
                &rel_points.locations,
                &dec[..7],
                w_rel as i32,
            );
            // half-diff attack insertion loop (30103-30194)
            if (atk_count + rel_count) < 7 {
                if b3c_atk_count > 0 {
                    let mut cur_level = s1850; // local_19c4
                    let mut reserved_next = w_atk; // local_19cc
                    let mut je = 0usize; // local_19e4
                    loop {
                        let location = b3c_atk_points.locations[je];
                        let free_slot = db0[location as usize + 1] < 8
                            && (location < 1 || db0[location as usize] < 8)
                            && (location > 0x1e || db0[location as usize + 2] < 8);
                        if free_slot {
                            if band != 0 || sum_main <= 100000.0 || sum_main <= sum_diff * 10.0 {
                                // weighted average compare (30116-30152), f64 model
                                let mut left = 0.0f64;
                                if location >= 0 {
                                    for bin in 0..=(location as usize) {
                                        left += f64::from(pow2_scale(curve[bin + 1]))
                                            * f64::from(env[bin + 1]);
                                    }
                                }
                                let split = location + 1;
                                let mut right = 0.0f64;
                                for bin in (split as usize)..0x21 {
                                    right += f64::from(pow2_scale(curve[bin + 1]))
                                        * f64::from(env[bin + 1]);
                                }
                                let mut level = b3c_atk_points.levels[je];
                                let scale = f64::from(pow2_scale(level));
                                if scale * (left / f64::from(split))
                                    < (right / f64::from(0x20 - location)) * f64::from(SQRT2_F32)
                                {
                                    let mut next_level = cur_level + level;
                                    if s1854 < next_level {
                                        level = s1854 - cur_level;
                                        next_level = level + cur_level;
                                    }
                                    cur_level = next_level;
                                    if level > 0 {
                                        let rank = db0[location as usize + 1];
                                        if (w_atk as i32) <= rank {
                                            atk_points.levels[w_atk] = level;
                                            atk_points.locations[w_atk] = location;
                                        } else {
                                            let mut t = w_atk;
                                            while (rank as usize) < t {
                                                atk_points.levels[t] = atk_points.levels[t - 1];
                                                atk_points.locations[t] =
                                                    atk_points.locations[t - 1];
                                                t -= 1;
                                            }
                                            atk_points.levels[rank as usize] = level;
                                            atk_points.locations[rank as usize] = location;
                                        }
                                        w_atk += 1;
                                        db0[location as usize + 1] = rank + 8;
                                        let mut p = split;
                                        while p < 0x20 {
                                            db0[p as usize + 1] += 1;
                                            p += 1;
                                        }
                                        reserved_next = w_atk;
                                    }
                                }
                                // LAB_4794c
                                if 6 < w_atk as i32 + rel_count as i32 {
                                    break;
                                }
                            }
                            // gate2 failed: advance without the break check
                        } else if 6 < w_atk as i32 + rel_count as i32 {
                            break;
                        }
                        je += 1;
                        if je >= b3c_atk_count {
                            break;
                        }
                    }
                    atk_points.reserved = reserved_next as i32; // local_2ac
                    s1850 = cur_level;
                }
                atk_count = w_atk; // local_189c = local_19d8
            }

            // half-diff release insertion loop (30196-30360)
            if b3c_rel_count > 0 {
                // release-side maps descending (30200-30218)
                let mut ia = 0usize;
                for pos in (0..=0x20i32).rev() {
                    if ia < w_rel && rel_points.locations[ia] == pos {
                        db0[pos as usize + 1] = ia as i32 + 8;
                        ia += 1;
                    } else {
                        db0[pos as usize + 1] = ia as i32;
                    }
                }
                // suffix sums over the CURRENT working lists (30219-30240)
                let mut dcc2 = [0i32; 8];
                {
                    let mut acc = 0i32;
                    let mut k = w_atk as i32;
                    while k >= 1 {
                        acc += atk_points.levels[(k - 1) as usize];
                        dcc2[(k - 1) as usize] = acc;
                        k -= 1;
                    }
                }
                let mut dec2 = [0i32; 8];
                {
                    let mut acc = 0i32;
                    let mut k = w_rel as i32;
                    while k >= 1 {
                        acc += rel_points.levels[(k - 1) as usize];
                        dec2[(k - 1) as usize] = acc;
                        k -= 1;
                    }
                }
                build_curve(
                    &mut curve,
                    &atk_points.locations,
                    &dcc2[..7],
                    w_atk as i32,
                    &rel_points.locations,
                    &dec2[..7],
                    w_rel as i32,
                );
                if (atk_count as i32 + rel_count as i32) < 7 && b3c_rel_count > 0 {
                    let mut cur_level = s1858; // local_19b4
                    let mut reserved_next = w_rel; // local_19bc
                    let mut je = 0usize; // local_19ec
                    loop {
                        let location = b3c_rel_points.locations[je];
                        let free_slot = db0[location as usize + 1] < 8
                            && (location < 1 || db0[location as usize] < 8)
                            && (location > 0x1e || db0[location as usize + 2] < 8);
                        if free_slot {
                            let mut left = 0.0f64;
                            for bin in 0..(location.max(0) as usize) {
                                left +=
                                    f64::from(pow2_scale(curve[bin + 1])) * f64::from(env[bin + 1]);
                            }
                            if location > 0 {
                                left /= f64::from(location);
                            }
                            let mut right = 0.0f64;
                            for bin in (location.max(0) as usize)..0x21 {
                                right +=
                                    f64::from(pow2_scale(curve[bin + 1])) * f64::from(env[bin + 1]);
                            }
                            let mut level = b3c_rel_points.levels[je];
                            let scale = f64::from(pow2_scale(level));
                            if scale * (right / f64::from(0x21 - location)) < left {
                                let mut next_level = cur_level + level;
                                if s1854 < next_level {
                                    level = s1854 - cur_level;
                                    next_level = level + cur_level;
                                }
                                cur_level = next_level;
                                if level > 0 {
                                    let rank = db0[location as usize + 1];
                                    if (w_rel as i32) <= rank {
                                        rel_points.levels[w_rel] = level;
                                        rel_points.locations[w_rel] = location;
                                    } else {
                                        let mut t = w_rel;
                                        while (rank as usize) < t {
                                            rel_points.levels[t] = rel_points.levels[t - 1];
                                            rel_points.locations[t] = rel_points.locations[t - 1];
                                            t -= 1;
                                        }
                                        rel_points.levels[rank as usize] = level;
                                        rel_points.locations[rank as usize] = location;
                                    }
                                    w_rel += 1;
                                    db0[location as usize + 1] = rank + 8;
                                    let mut p = location;
                                    while p >= 0 {
                                        db0[p as usize + 1] += 1;
                                        p -= 1;
                                    }
                                    reserved_next = w_rel;
                                }
                            }
                        }
                        // do-while condition (30355)
                        if !((w_atk as i32 + w_rel as i32) < 7) {
                            break;
                        }
                        je += 1;
                        if je >= b3c_rel_count {
                            break;
                        }
                    }
                    rel_points.reserved = reserved_next as i32; // local_30c
                    s1858 = cur_level;
                }
                b3c_rel_points.reserved = b3c_rel_count as i32; // 30361
            }
        } else {
            // 30363-30366
            w_atk = atk_points.reserved.max(0) as usize;
            w_rel = rel_points.reserved.max(0) as usize;
        }
    } else {
        // 30368-30371
        w_atk = atk_points.reserved.max(0) as usize;
        w_rel = rel_count;
    }

    // --- suffix-cumulate all four level lists (30372-30398) ---
    suffix_cumulate(&mut atk_points.levels, w_atk as i32);
    suffix_cumulate(&mut rel_points.levels, w_rel as i32);
    if fractional {
        let b9c = b3c_atk_points.reserved;
        suffix_cumulate(&mut b3c_atk_points.levels, b9c);
        let bfc = b3c_rel_points.reserved;
        suffix_cumulate(&mut b3c_rel_points.levels, bfc);
    }

    // --- final gain curve (30400-30439) ---
    build_curve(
        &mut curve,
        &atk_points.locations,
        &atk_points.levels,
        atk_points.reserved,
        &rel_points.locations,
        &rel_points.levels,
        rel_points.reserved,
    );

    // --- half-diff curve local_d1c (30440-30480) ---
    let mut d1c = [0i32; 36];
    if fractional {
        let mut pos = 0i32;
        for j in 0..b3c_atk_points.reserved.max(0) as usize {
            let location = b3c_atk_points.locations[j];
            if pos <= location {
                let value = b3c_atk_points.levels[j];
                while pos <= location {
                    d1c[pos as usize] += value;
                    pos += 1;
                }
            }
        }
        let mut pos = 0x20i32;
        for j in 0..b3c_rel_points.reserved.max(0) as usize {
            let location = b3c_rel_points.locations[j];
            if location <= pos {
                let value = b3c_rel_points.levels[j];
                while location <= pos {
                    d1c[pos as usize] += value;
                    pos -= 1;
                }
            }
        }
        let top = d1c[0x20];
        for slot in d1c[..0x21].iter_mut() {
            *slot -= top;
        }
    }

    // --- segment building over curve steps (30482-30533) ---
    let mut deltas = [0i32; 40]; // aiStack_e90 (boundary deltas at [1+i])
    let mut segments = [[0i32; 7]; 9]; // local_f6c records (7 words each)
    let mut segment_count = 0i32; // local_192c
    segments[0] = [0, 0x40, 0, 0, 0, 0x4080_0000u32 as i32, 0];
    {
        let mut seg = 0usize;
        let mut i = 0usize;
        loop {
            if f32::from_bits(segments[seg][5] as u32) < env[1 + i] {
                segments[seg][5] = env[1 + i].to_bits() as i32;
            }
            let next = i + 1;
            if curve[i + 1] != curve[i + 2] {
                if seg + 1 >= segments.len() {
                    return Err(SetGaincError::SegmentOverflow {
                        count: segment_count,
                    });
                }
                deltas[1 + i] = curve[i + 1] - curve[i + 2];
                segments[seg][1] = next as i32;
                let direction = if curve[i + 1] <= curve[i + 2] { 2 } else { 1 };
                segments[seg][3] = direction;
                segment_count += 1;
                segments[seg + 1][5] = 0x4080_0000u32 as i32;
                segments[seg + 1][0] = next as i32;
                segments[seg + 1][1] = 0x40;
                segments[seg + 1][2] = direction;
                segments[seg + 1][3] = 0;
                seg += 1;
            }
            i = next;
            if i >= 0x20 {
                break;
            }
        }
    }

    // --- trailing flat scan + first +0x74 selection (30534-30588) ---
    let base54 = row_f32(&cur_plane[band], 0x54);
    let coef = f32::from_bits(SA_COEF_B_BITS[band]) * base54; // fVar10 (30536)
    let mut last_boundary = 0i32; // local_1930
    // The trailing max accumulates in a register seeded from env[0x21]; the
    // decompile's `afStack_250[0x21] = ...` stores are spurious (no memory
    // store to [ebp-0x1c8] exists), so env stays pristine for later readers.
    let mut tail_max = env[0x21];
    {
        let mut k = 0x1fi32;
        loop {
            if deltas[1 + k as usize] != 0 {
                last_boundary = k;
                break;
            }
            let value = env[1 + k as usize];
            if !(value <= tail_max) {
                tail_max = value;
            }
            k -= 1;
            if k < 0 {
                break;
            }
        }
    }
    if segment_count < 1 {
        let c4c = row_f32(&cur_plane[band], 0x4c);
        let c48 = row_f32(&cur_plane[band], 0x48);
        if c4c < c48 && c48 < c4c * coef {
            row_set_f32(&mut cur_plane[band], 0x74, c4c);
        } else {
            row_set_f32(&mut cur_plane[band], 0x74, c48);
        }
    } else if deltas[1 + last_boundary as usize] < 1 {
        let mut done = false;
        if last_boundary < 0x10 {
            let scaled = row_f32(&cur_plane[band], 0x4c) * coef;
            if scaled < peak_out {
                let was_less = tail_max < scaled;
                tail_max = peak_out;
                if was_less {
                    let c4c = row_f32(&cur_plane[band], 0x4c);
                    row_set_f32(&mut cur_plane[band], 0x74, c4c);
                } else {
                    row_set_f32(&mut cur_plane[band], 0x74, tail_max);
                }
                done = true;
            }
        }
        if !done {
            row_set_f32(&mut cur_plane[band], 0x74, peak_out);
        }
    } else if last_boundary > 0xf {
        row_set_f32(&mut cur_plane[band], 0x74, tail_max);
    } else {
        let c4c = row_f32(&cur_plane[band], 0x4c);
        if c4c * coef < tail_max {
            row_set_f32(&mut cur_plane[band], 0x74, tail_max);
        } else {
            row_set_f32(&mut cur_plane[band], 0x74, c4c);
        }
    }
    row_set_i32(&mut cur_plane[band], 0x78, 0);

    // --- check_gc scan phase (30589-30846), gated on the two-sided flag ---
    let mut records = [CheckGcRecord::default(); 8]; // local_148c area
    let mut record_count = 0usize; // local_187c
    let mut queue1 = [0i32; 112]; // &local_118c deferred/save queue
    if flag2 && !streaky && segment_count < 7 {
        let mut budget = segment_count; // local_1878 (30590)
        // mode-code assignment (30591-30655)
        for seg in 0..=(segment_count as usize) {
            let start = segments[seg][0];
            let end = segments[seg][1];
            let (code, reference) = match (segments[seg][2], segments[seg][3]) {
                (1, 1) => (5, env[1 + start as usize]),
                (1, 2) => {
                    let head = env[1 + start as usize];
                    let tail = env[end as usize];
                    (6, if tail <= head { tail } else { head })
                }
                (1, _) => (4, env[1 + start as usize]),
                (2, 1) => (9, f32::from_bits(segments[seg][5] as u32)),
                (2, 2) => (10, env[end as usize]),
                (2, _) => (8, fv9r),
                (0, 1) => (1, fv2),
                (0, 2) => (2, env[end as usize]),
                (0, _) => (0, f32::from_bits(segments[seg][5] as u32)),
                _ => continue,
            };
            segments[seg][4] = code;
            segments[seg][6] = reference.to_bits() as i32;
        }
        // bin mean (30656-30662; native 0x386ed accumulates in st(0))
        let mean = {
            let mut acc = 0.0f64;
            for k in 0..0x20usize {
                acc += f64::from(env[1 + k]);
            }
            (acc * 0.03125) as f32
        };
        let guard_peak = row_f32(&cur_plane[band], 0x48);
        let config_for = |code: i32| CheckGcConfig {
            mode: code,
            guard_peak,
            ratio_scale: base54,
        };
        // segment scan (30663-30766)
        'segments: for seg in 0..=(segment_count as usize) {
            let s_start = segments[seg][0];
            let s_end = segments[seg][1];
            let s_code = segments[seg][4];
            let s_ref = f32::from_bits(segments[seg][6] as u32);
            let mut local_budget = budget; // local_19f0
            if budget > 6 || s_end - s_start <= sa_distance(s_code) {
                continue;
            }
            let thr_seg = if segments[seg][3] == 0 {
                base54 + base54
            } else {
                base54 * COEF_1_65_F32
            };
            let mut run_start = s_start; // local_f7c
            let mut run_end = s_start; // local_f78
            let mut run_peak = 4.0f32; // local_f74
            let mut k = s_start;
            while k < s_end {
                let value = env[1 + k as usize];
                if thr_seg * value <= s_ref {
                    run_end += 1;
                    if run_peak < value {
                        run_peak = value;
                    }
                } else {
                    if run_end > 0x20 {
                        break;
                    }
                    if sa_distance(s_code) < run_end - run_start && run_start > 0 {
                        // check_gc call #1 (native 0x38895)
                        check_gc_at5(
                            config_for(s_code),
                            CheckGcCandidate {
                                start: run_start as usize,
                                end: run_end as usize,
                                width_bits: run_peak.to_bits(),
                            },
                            &mut queue1,
                            &env[..],
                            &mut curve[..],
                            &mut deltas[..],
                            &mut records,
                            &mut record_count,
                            &mut budget,
                            mean,
                        )?;
                        local_budget = budget;
                    }
                    run_peak = 4.0;
                    run_start = k + 1;
                    run_end = k + 1;
                    if local_budget > 6 {
                        continue 'segments;
                    }
                }
                k += 1;
            }
            // post-scan tail (30705-30756)
            if local_budget < 7 && sa_distance(s_code) < run_end - run_start {
                if (run_start.wrapping_sub(1) as u32) < 0x20 {
                    // check_gc call #4 (native 0x39859)
                    check_gc_at5(
                        config_for(s_code),
                        CheckGcCandidate {
                            start: run_start as usize,
                            end: run_end as usize,
                            width_bits: run_peak.to_bits(),
                        },
                        &mut queue1,
                        &env[..],
                        &mut curve[..],
                        &mut deltas[..],
                        &mut records,
                        &mut record_count,
                        &mut budget,
                        mean,
                    )?;
                } else {
                    if total_points != 0 || !fractional {
                        continue;
                    }
                    if segment_count < 1 {
                        continue; // goto LAB_49861 (skips the call)
                    }
                    if run_start == 0 && s_end < 0x20 {
                        // whole-head quiet rescan appending to queue1 (30716-30748)
                        run_start = 0;
                        run_end = 0;
                        run_peak = env[1];
                        let mut kk = 0i32; // local_19f4
                        if s_end >= 0 {
                            loop {
                                let value = env[1 + kk as usize];
                                if thr_seg * value <= s_ref {
                                    run_end += 1;
                                    if run_peak < value {
                                        run_peak = value;
                                    }
                                } else {
                                    if sa_distance(s_code) < run_end - run_start {
                                        let count = queue1[0];
                                        if !(0..=0x2e).contains(&count) {
                                            return Err(SetGaincError::DeferredQueueOverflow {
                                                count,
                                            });
                                        }
                                        let slot = count as usize;
                                        queue1[1 + slot] = run_start;
                                        queue1[0x21 + slot] = run_end;
                                        queue1[0x41 + slot] = run_peak.to_bits() as i32;
                                        queue1[0x61 + slot] = s_code;
                                        queue1[0] = count + 1;
                                        local_budget = budget;
                                    }
                                    run_peak = 4.0;
                                    run_start = kk + 1;
                                    run_end = kk + 1;
                                    if local_budget > 6 {
                                        break;
                                    }
                                }
                                kk += 1;
                                if kk > s_end {
                                    break;
                                }
                            }
                        }
                        continue;
                    }
                }
            }
        }

        // deferred-queue processing (30767-30845)
        let queue_len = queue1[0];
        let mut queue2 = [0i32; 112]; // &local_16cc inner deferred block
        let mut qi = 0i32; // local_1934
        while qi < queue_len {
            let mut c_start = queue1[1 + qi as usize]; // local_193c
            let mut c_end = queue1[0x21 + qi as usize]; // local_1940
            let mut c_code = queue1[0x61 + qi as usize]; // local_1944
            let mut c_peak_bits = queue1[0x41 + qi as usize]; // local_1948
            queue2[0] = 0; // local_16cc = 0 (30774)
            let mut tries = 7 - budget; // local_1938
            if tries > 3 {
                tries = 3;
            }
            if tries > 0 && c_start < 0x21 && budget < 7 {
                loop {
                    let try_code = c_code; // local_149c (config +0x10)
                    let dedup_start = c_start; // local_14ac
                    let dedup_end = c_end; // local_14a8
                    let seed_ref = f32::from_bits(c_peak_bits as u32); // local_1494
                    let mut run_peak = 4.0f32; // local_14b4
                    let mut run_start = c_start; // local_14bc
                    let mut run_end = c_start; // local_14b8
                    let mut local_budget = budget; // iVar18
                    let mut hit_budget = false;
                    let mut k = c_start;
                    while k < c_end {
                        let value = env[1 + k as usize];
                        if (base54 + base54) * value <= seed_ref {
                            run_end += 1;
                            if run_peak < value {
                                run_peak = value;
                            }
                        } else {
                            if run_end > 0x20 {
                                break;
                            }
                            if (run_start.wrapping_sub(1) as u32) < 0x20
                                && sa_distance(try_code) < run_end - run_start
                            {
                                // check_gc call #2 (native 0x3906c)
                                check_gc_at5(
                                    config_for(try_code),
                                    CheckGcCandidate {
                                        start: run_start as usize,
                                        end: run_end as usize,
                                        width_bits: run_peak.to_bits(),
                                    },
                                    &mut queue2,
                                    &env[..],
                                    &mut curve[..],
                                    &mut deltas[..],
                                    &mut records,
                                    &mut record_count,
                                    &mut budget,
                                    mean,
                                )?;
                                local_budget = budget;
                            }
                            run_peak = 4.0;
                            run_start = k + 1;
                            run_end = k + 1;
                            if local_budget > 6 {
                                hit_budget = true;
                                break;
                            }
                        }
                        k += 1;
                    }
                    if !hit_budget
                        && local_budget < 7
                        && run_start > 0
                        && run_start < 0x21
                        && sa_distance(try_code) < run_end - run_start
                        && (dedup_start != run_start || dedup_end != run_end)
                    {
                        // check_gc call #3 (native 0x390c5)
                        check_gc_at5(
                            config_for(try_code),
                            CheckGcCandidate {
                                start: run_start as usize,
                                end: run_end as usize,
                                width_bits: run_peak.to_bits(),
                            },
                            &mut queue2,
                            &env[..],
                            &mut curve[..],
                            &mut deltas[..],
                            &mut records,
                            &mut record_count,
                            &mut budget,
                            mean,
                        )?;
                    }
                    // LAB_48ec6: pick the widest deferred sub-candidate
                    let mut found = false;
                    if queue2[0] > 0 {
                        let count = queue2[0];
                        let mut best = 0usize;
                        let mut best_width = queue2[0x21] - queue2[1];
                        for j in 1..count as usize {
                            let width = queue2[0x21 + j] - queue2[1 + j];
                            if best_width < width {
                                best_width = width;
                                best = j;
                            }
                        }
                        c_peak_bits = queue2[0x41 + best];
                        c_start = queue2[1 + best];
                        c_end = queue2[0x21 + best];
                        c_code = queue2[0x61 + best];
                        tries -= 1;
                        queue2[0] = 0;
                        found = true;
                    }
                    if !(tries > 0 && found && c_start < 0x21 && budget < 7) {
                        break;
                    }
                }
            }
            qi += 1;
        }
    }

    // --- over-budget record undo (30848-30930) ---
    let mut diffs = 0i32;
    for k in 0..0x20usize {
        if curve[1 + k] != curve[2 + k] {
            diffs += 1;
        }
    }
    if record_count > 0 && diffs > 7 {
        // ascending (span + ratio) selection order (30858-30891)
        let mut keys = [0.0f32; 8]; // afStack_184c
        let mut used = [0i32; 8]; // aiStack_17cc
        let mut order = [0i32; 8]; // aiStack_174c
        for j in 0..record_count {
            let words = records[j].words();
            keys[j] = words[3] as i32 as f32 + f32::from_bits(words[0]);
            used[j] = 0;
        }
        for slot in order.iter_mut().take(record_count) {
            let mut best = -1i32;
            let mut best_key = 65536.0f32;
            for t in 0..record_count {
                if used[t] == 0 && keys[t] < best_key {
                    best = t as i32;
                    best_key = keys[t];
                }
            }
            *slot = best;
            if best < 0 {
                return Err(SetGaincError::UndoSelectionExhausted);
            }
            used[best as usize] = 1;
        }
        // undo in key order until the boundary count fits (30893-30929)
        let mut oi = 0usize; // local_1a00
        while diffs > 7 {
            if oi >= record_count {
                return Err(SetGaincError::UndoSelectionExhausted);
            }
            let record_index = order[oi];
            if record_index < 0 {
                return Err(SetGaincError::UndoSelectionExhausted);
            }
            let mut words = records[record_index as usize].words();
            let start = words[1] as i32;
            let mut end = words[2] as i32;
            if end > 0x21 {
                end = 0x21;
            }
            let level = words[4] as i32;
            let mut t = start;
            while t < end {
                curve[(t + 1) as usize] -= level;
                t += 1;
            }
            if start > 0 {
                deltas[start as usize] += level;
            }
            if end < 0x21 {
                deltas[end as usize] -= level;
            }
            words[5] = 65536.0f32.to_bits(); // mark undone (30920)
            records[record_index as usize] = CheckGcRecord::from_words(words);
            diffs = 0;
            for k in 0..0x20usize {
                if curve[1 + k] != curve[2 + k] {
                    diffs += 1;
                }
            }
            oi += 1;
        }
    }

    // --- top normalization + clamp (30931-30949) ---
    if curve[0x21] != 0 {
        let top = curve[0x21];
        for k in 0..0x21usize {
            curve[1 + k] -= top;
        }
    }
    for k in 0..0x21usize {
        if curve[1 + k] > 9 {
            curve[1 + k] = 9;
        } else if curve[1 + k] <= -7 {
            curve[1 + k] = -6;
        }
    }
    // (30951-30958 recounts the diffs into the dead budget slot; no effect.)

    // --- +0x74 refinement from surviving records (30959-30983) ---
    if record_count > 0 {
        let mut best74 = row_f32(&cur_plane[band], 0x74);
        for j in 0..record_count {
            let words = records[j].words();
            let end = words[2] as i32;
            let start_minus_1 = words[1] as i32 - 1;
            if end < 0x21 || start_minus_1 < last_boundary {
                continue;
            }
            let width = f32::from_bits(words[5]);
            if width < best74 {
                row_set_i32(&mut cur_plane[band], 0x78, 1);
                last_boundary = start_minus_1;
                best74 = width;
            }
        }
        row_set_f32(&mut cur_plane[band], 0x74, best74);
    }

    // --- +0x70 scaled-peak scan (30984-31007) ---
    {
        let mut scale = pow2_scale(curve[1]);
        row_set_f32(&mut cur_plane[band], 0x70, env[1] * scale);
        let mut k = 1usize;
        loop {
            let g = curve[1 + k];
            if curve[k] != g {
                scale = pow2_scale(g);
            }
            let value = env[1 + k] * scale;
            if row_f32(&cur_plane[band], 0x70) < value {
                row_set_f32(&mut cur_plane[band], 0x70, value);
            }
            if k + 1 > 0x1f {
                break;
            }
            k += 1;
        }
    }

    // --- half-diff +0x84 selection (31008-31078) ---
    if fractional {
        let mut steps = 0i32;
        let mut step_deltas = [0i32; 34]; // local_c8c reuse
        for k in 0..0x20usize {
            if d1c[k] != d1c[k + 1] {
                steps += 1;
                step_deltas[k] = d1c[k] - d1c[k + 1];
            } else {
                step_deltas[k] = 0;
            }
        }
        let coef2 = row_f32(&cur_plane[band], 0x54) * f32::from_bits(SA_COEF_B_BITS[band]);
        let mut last_nonzero = 0i32; // local_1a54
        // Register-resident trailing max seeded from b3c[0x44] (the decompile's
        // `local_b3c[0x44] = ...` stores are spurious; memory stays pristine).
        let mut b3c_tail = b3c[0x44];
        {
            let mut k = 0x1fi32;
            loop {
                if step_deltas[k as usize] != 0 {
                    last_nonzero = k;
                    break;
                }
                let value = b3c[0x24 + k as usize];
                if !(value <= b3c_tail) {
                    b3c_tail = value;
                }
                k -= 1;
                if k < 0 {
                    break;
                }
            }
        }
        let out84;
        if steps < 1 {
            let c80 = row_f32(&cur_plane[band], 0x80);
            let c7c = row_f32(&cur_plane[band], 0x7c);
            if c80 < c7c && !(c80 * coef2 <= c7c) {
                out84 = c80;
            } else {
                out84 = c7c;
            }
        } else if step_deltas[last_nonzero as usize] < 1 {
            let mut value = b3c_relref;
            if last_nonzero <= 0xf {
                let c80 = row_f32(&cur_plane[band], 0x80);
                let scaled = c80 * coef2;
                if scaled < b3c_relref && !(scaled <= b3c_tail) {
                    value = c80;
                }
            }
            out84 = value;
        } else {
            let mut value = b3c_tail;
            if last_nonzero < 0x10 {
                value = row_f32(&cur_plane[band], 0x80);
                let scaled = value * coef2;
                if b3c_tail > scaled {
                    value = b3c_tail;
                }
            }
            out84 = value;
        }
        row_set_f32(&mut cur_plane[band], 0x84, out84);
    }

    // --- record emission or degenerate reset (31080-31143) ---
    let mut emitted = 0i32; // local_1a04
    let mut degenerate = false;
    {
        let lngain = lngain_at5();
        let mut rec = 0usize;
        for k in 0..0x20usize {
            if curve[1 + k] != curve[2 + k] {
                if emitted > 6 {
                    degenerate = true;
                    break;
                }
                cur_plane[band][1 + rec] = k as u32;
                let mut level_id = -1i32; // local_1954
                for j in 0..0x10usize {
                    if i32::from(lngain[j]) <= curve[1 + k] {
                        level_id = j as i32;
                    }
                }
                cur_plane[band][8 + rec] = level_id as u32;
                emitted += 1;
                rec += 1;
            }
        }
    }
    if degenerate {
        // 31086-31126 reset
        cur_plane[band][0] = 0;
        for word in 8..15usize {
            cur_plane[band][word] = 0;
        }
        for word in 1..8usize {
            cur_plane[band][word] = 0;
        }
        let f48 = row_f32(&cur_plane[band], 0x48);
        let f4c = row_f32(&cur_plane[band], 0x4c);
        row_set_i32(&mut cur_plane[band], 0x44, 0);
        row_set_i32(&mut cur_plane[band], 0x40, 0);
        row_set_i32(&mut cur_plane[band], 0x58, 0);
        row_set_i32(&mut cur_plane[band], 0x60, 0);
        row_set_i32(&mut cur_plane[band], 0x5c, 0);
        row_set_i32(&mut cur_plane[band], 0x78, 0);
        let factor = row_f32(&cur_plane[band], 0x54) * f32::from_bits(SA_COEF_B_BITS[band]);
        row_set_f32(&mut cur_plane[band], 0x70, f48);
        if f48 <= f4c || f4c * factor <= f48 {
            cur_plane[band][0x74 / 4] = cur_plane[band][0x48 / 4];
        } else {
            row_set_f32(&mut cur_plane[band], 0x74, f4c);
        }
        let f80 = row_f32(&cur_plane[band], 0x80);
        let f7c = row_f32(&cur_plane[band], 0x7c);
        row_set_i32(&mut cur_plane[band], 0x8c, 0);
        row_set_i32(&mut cur_plane[band], 0x88, 0);
        if f7c <= f80 || f80 * factor <= f7c {
            cur_plane[band][0x84 / 4] = cur_plane[band][0x7c / 4];
        } else {
            row_set_f32(&mut cur_plane[band], 0x84, f80);
        }
    } else {
        cur_plane[band][0] = emitted as u32;
    }

    // --- LAB_4862c final stores (31144-31148) ---
    row_set_f32(&mut cur_plane[band], 0x50, flat[0x1f]);
    row_set_i32(&mut cur_plane[band], 0x68, prev_6c);
    row_set_i32(&mut cur_plane[band], 0x6c, transitions);
    Ok(())
}
