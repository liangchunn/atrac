//! Scoped composition of `init_channel_block_at5` (native `0x3f870`,
//! decompile `0x4f870`, 5635 bytes) for the first implementation
//!
//! Native source of truth is the decompiled boundary at Ghidra
//! `0x0004f870` and direct disassembly at native
//! `0x0003f870..0x00040e80`. The `atx_encode_core` call site at native
//! `0x00055df3` pins the ABI: eax = param_1 block/channel-state table,
//! edx = param_2 spectrum A table, ecx = param_3 spectrum B table,
//! stack esp+4 = param_4 object table (gain arrays + `0x1b6xx` planes),
//! esp+8 = param_5 channel count, esp+12 = param_6 encode selector.
//!
//! The pass, per channel:
//!  * classifies gain records (block `+0x44` flag; object
//!    `+0x1b48c`/`+0x1b484`/`+0x1b490`/`+0x1b488` last-nonzero and
//!    duplicate state) from the `obj+0x8`/`obj+0xc` gain arrays;
//!  * zeroes the `+0x1b5f8`/`+0x1b678`/`+0x1b578` planes and initializes
//!    the shared `block[0]+0x1008` struct;
//!  * seeds the block word0 from the selector;
//!  * computes spectral energy ratios (block `+0x44c`/`+0x450`) from
//!    spectrum A and the energy class `+0x42`;
//!  * derives idsf via `set_idsf_from_mdspec_at5` for spectrum B
//!    (`obj+0x1b678`, max `block+0x2cc`) and spectrum A
//!    (gainA `+0x9c8`, max `block+0x34c`);
//!  * writes the per-band mdspec averages (gainA `+0xa48`);
//!  * seeds the word-length rows (block `+0x4c`), aux weight
//!    (block `+0x3cc`), tonality (block `+0x458`), the word0 rows,
//!    block `+0x454`, and the final min/max word clamps.
//!
//! Float model: the x87 energy accumulations are plain f32 (each `fsts`
//! rounds after every add, so `sum += x.abs()` in f32 is bit-exact);
//! the ratio quotients (`fVar7*0.0625` numerator is exact 2^-4, the
//! `fVar8/112.0` denominator is x87-extended) are modeled in f64 with a
//! single f32 round at the native `fstps`. The `+0x4c` word-length seed
//! truncates `(a+b)*0.5+0.5` toward zero (x87 RC=11). The tonality sum
//! is an f64 reduction rounded to f32 then divided.
//!
//! Scope: the selector-`<0xc` 0.89 spectrum scale is never reached at any
//! shipped rate and stays guarded with an explicit `OutOfScope` error when its
//! native gate is active. The `+0x1dc` 0.94 scale (ported in place below) and
//! the `<0x10` high-frequency spectral cut (decompile 34685-34710, ported below
//! as [`init_high_frequency_cut_start_at5`] — first live at a MONO rate at
//! 64 kbps, docs/14 §3.1) run when their native gates open. The `+0x45c`
//! gain-scan transient byte + first-8-row word bump (decompile
//! 35024-35049) is dead at 352 (selector 30) but LIVE at 96 kbps
//! (selector 19); it is ported in place (Block N below), gated to the
//! stereo `0xb..=0x13` / mono `9` selector window. The per-group
//! joint-stereo spectrum SWAP branch (decompile 34711–34766) IS live on
//! below.

use crate::coding::normalize::{NormalizeError, set_idsf_from_mdspec_at5};
use crate::tables::at5::{isps_at5, nsps_at5, y_at5};

const INIT_BANDS_AT5: usize = 32;
const INIT_GAIN_ROWS_AT5: usize = 16;
const INIT_GAIN_ROW_INTS_AT5: usize = 0x26;
const INIT_SPECTRUM_WORDS_AT5: usize = 2048;

#[derive(Debug, PartialEq)]
pub enum InitBlockError {
    Normalize(NormalizeError),
    OutOfScope(&'static str),
    ShapeMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl From<NormalizeError> for InitBlockError {
    fn from(error: NormalizeError) -> Self {
        InitBlockError::Normalize(error)
    }
}

/// One channel's `init_channel_block_at5` entry surface.
#[derive(Debug, Clone)]
pub struct InitChannelState {
    /// Object-side word `*(objentry+0x30)+0x1c` (block A / mono word0).
    pub objside_1c: i32,
    /// Object-side word `*(objentry+0x30)+0x14` (mono word0 override).
    pub objside_14: i32,
    /// Object-side pointer `*(objentry+0x30)`, written to block `+0x100c`.
    pub objside_ptr: u32,
    /// Spectrum B pointer, written to block `+0x1010`.
    pub spec_b_ptr: u32,
    /// `g_a_y_at5` index `*(*(objentry+0x10))` (from object[0] only).
    pub y_index: i32,
    /// Gain array A records (`obj+0x8`), 16 records of 0x26 ints.
    pub gain_a_records: Vec<u32>,
    /// Gain array B records (`obj+0xc`), 16 records of 0x26 ints.
    pub gain_b_records: Vec<u32>,
    /// gainB `+0x9c8` word-length seed input. Init does not write the B-side
    /// (`obj+0xc`) `+0x9c8` this call; it is the PREVIOUS call's gainA `+0x9c8`
    /// (init's A-side `set_idsf_from_mdspec_at5` write, decompile 34879), seen
    /// through the head-of-`at5enc_sigproc` double-buffer swap (`obj+0x8` <->
    /// `obj+0xc`, decompile 42960–42964). Init reads it into the `+0x4c`
    /// word-length seed `ROUND((b_9c8+a_9c8)*0.5+0.5)` (decompile 34946).
    pub b_9c8: Vec<i32>,
    /// gainB `+0xa48` weight input. Init does not write the B-side `+0xa48` this
    /// call; it is the PREVIOUS call's gainA `+0xa48` (init's A-side band-average
    /// write, decompile ~34884–34928), seen through the double-buffer swap. Init
    /// reads it into the `+0x3cc` weight `(a_a48+b_a48)*0.5` (decompile 34960).
    pub b_a48: Vec<f32>,
    /// Spectrum A (param_2), 2048 f32 words.
    pub spectrum_a: Vec<f32>,
    /// Spectrum B (param_3), 2048 f32 words.
    pub spectrum_b: Vec<f32>,
}

/// Frame-level `init_channel_block_at5` inputs.
#[derive(Debug)]
pub struct InitFrameState {
    pub channels: Vec<InitChannelState>,
    /// `param_5` channel count.
    pub channel_count: usize,
    /// `param_6` encode selector.
    pub selector: i32,
    /// Side `+0xb4` quant-unit / band count (`uVar5`).
    pub band_count: usize,
    /// PER-FRAME effective band extent (`cfg+0xb4`, `uVar5`) — the value the
    /// native high-frequency spectral-cut gate reads (decompile 34685). Distinct
    /// from `band_count`: the composed bridge feeds the fixed 32-wide processing
    /// extent through `band_count` while threading THIS call's post-override
    /// per-frame value (`report.sigproc.writeback.band_limit`) here. Drives
    /// [`init_high_frequency_cut_gate_open`] + its isps lookup only; every other
    /// carry the same captured `side +0xb4`.
    pub extent_b4: usize,
    /// Side `+0xbc` gain-record band count.
    pub gain_band_count: usize,
    /// Side `+0x1dc` flags (the 0.94-scale gate).
    pub flags_1dc: u32,
    /// Side `+0xac` sample-rate word (the high-frequency zero gate).
    pub sr_ac: u32,
    /// Side `+0x50` per-group joint-stereo flags.
    pub join_flags_50: Vec<i32>,
}

/// One channel's `init_channel_block_at5` output surface.
#[derive(Debug, Clone, PartialEq)]
pub struct InitChannelOutput {
    pub word0: i16,
    pub word_rows: Vec<i16>,
    pub class_42: i16,
    pub flag_44: i16,
    pub word_lengths_4c: Vec<i32>,
    pub max_b_2cc: Vec<f32>,
    pub max_a_34c: Vec<f32>,
    pub weight_3cc: Vec<f32>,
    pub ratio_44c: f32,
    pub ratio_450: f32,
    pub scaled_454: f32,
    pub tonality_458: f32,
    pub block_100c: u32,
    pub block_1010: u32,
    pub obj_1b48c: i32,
    pub obj_1b484: i32,
    pub obj_1b488: i32,
    pub obj_1b490: i32,
    pub obj_1b5f8: Vec<i32>,
    pub obj_1b578: Vec<i32>,
    pub idsf_1b678: Vec<i32>,
    pub a_9c8: Vec<i32>,
    pub a_a48: Vec<f32>,
    /// Block-N transient byte at `block+0x45c` (decompile 35024-35049).
    /// Set to 1 by the gain-scan producer when any of gainA record 0's
    /// `count` level ids is outside `{5,6,7}`; the byte is 0 otherwise.
    /// Emitted `false` when the native gate is closed (native leaves the
    /// byte unwritten there, but every native consumer reads it under the
    /// same selector window, so `false` is consumer-faithful).
    pub transient_45c: bool,
}

/// The shared `block[0]+0x1008` output surface.
#[derive(Debug, Clone, PartialEq)]
pub struct InitSharedOutput {
    /// `shared[0]` pointer (equals side address + 0x90).
    pub word0_ptr: u32,
    pub word21: i32,
    pub word22: i32,
    pub word23: i32,
    pub word24: i32,
    pub row_94: Vec<u16>,
    pub row_d4: Vec<u16>,
}

/// The side (`*(objentry0+4)`) output surface.
#[derive(Debug, Clone, PartialEq)]
pub struct InitSideOutput {
    pub field_90: u32,
    pub field_94: u32,
    pub field_98: u32,
    pub field_9c: u32,
    pub field_a0: u32,
    pub field_118: u32,
}

#[derive(Debug)]
pub struct InitFrameOutput {
    pub channels: Vec<InitChannelOutput>,
    pub shared: InitSharedOutput,
    pub side: InitSideOutput,
}

fn record_word(records: &[u32], record: usize, word: usize) -> i32 {
    records[record * INIT_GAIN_ROW_INTS_AT5 + word] as i32
}

/// Gain-record spread classifier: for the two leading records, returns
/// true when any record's point spread (`rec[8..8+count]`) exceeds 3.
fn gain_spread_over_3(records: &[u32]) -> bool {
    for record in 0..2 {
        let count = record_word(records, record, 0);
        let mut max = 6;
        let mut min = 6;
        let mut index = 0;
        while index < count {
            let value = record_word(records, record, 8 + index as usize);
            if max < value {
                max = value;
            }
            if value < min {
                min = value;
            }
            index += 1;
        }
        if max - min > 3 {
            return true;
        }
    }
    false
}

/// Native block-B `obj+0x44` gain flag and the `+0x1b48c`/`+0x1b484`/
/// `+0x1b490`/`+0x1b488` last-nonzero/duplicate state.
fn classify_gain_records(
    channel: &InitChannelState,
    gain_band_count: usize,
) -> (i16, i32, i32, i32, i32) {
    // Block-B flag `obj+0x44`.
    let flag_44: i16 = if gain_spread_over_3(&channel.gain_a_records) {
        0
    } else if gain_spread_over_3(&channel.gain_b_records) {
        1
    } else {
        -1
    };

    // Last-nonzero record scan over gain array A, top-down.
    let records = &channel.gain_a_records;
    let top = gain_band_count as i32 - 1;
    let mut last_nonzero = gain_band_count as i32;
    if top >= 0 {
        let mut index = top;
        loop {
            if record_word(records, index as usize, 0) != 0 {
                break;
            }
            last_nonzero = index;
            index -= 1;
            if index < 0 {
                break;
            }
        }
    }
    let obj_1b48c = last_nonzero;
    let obj_1b484 = if obj_1b48c < 1 { 0 } else { 1 };

    // Duplicate-record fold.
    let obj_1b490 = if obj_1b48c < 2 {
        obj_1b48c
    } else {
        let mut index = obj_1b48c - 1;
        let mut equal = true;
        if index > 0 {
            loop {
                let count = record_word(records, index as usize, 0);
                if count != record_word(records, (index - 1) as usize, 0) {
                    break;
                }
                let mut point = 0;
                while point < count {
                    let cur_20 = record_word(records, index as usize, 8 + point as usize);
                    let prev_20 = record_word(records, (index - 1) as usize, 8 + point as usize);
                    let cur_4 = record_word(records, index as usize, 1 + point as usize);
                    let prev_4 = record_word(records, (index - 1) as usize, 1 + point as usize);
                    if cur_20 != prev_20 || cur_4 != prev_4 {
                        equal = false;
                        break;
                    }
                    point += 1;
                }
                if equal {
                    index -= 1;
                }
                if !(equal && index > 0) {
                    break;
                }
            }
        }
        index + 1
    };
    let obj_1b488 = i32::from(obj_1b48c != obj_1b490);

    (flag_44, obj_1b48c, obj_1b484, obj_1b490, obj_1b488)
}

/// f32 running sum of `|spectrum[range]|` (each native `fsts` rounds to
/// f32 after every add, so plain f32 accumulation is bit-exact).
fn f32_abs_sum(spectrum: &[f32], start: usize, end: usize) -> f32 {
    let mut sum = 0.0f32;
    for &value in &spectrum[start..end] {
        sum += (f64::from(value).abs()) as f32;
    }
    sum
}

/// Native block-word0 selector ladder (`0x3fe25` stereo / `0x3fdb0`
/// mono). Returns the base word0 short.
fn selector_word0(selector: i32, stereo: bool) -> i16 {
    if stereo {
        if selector < 0x13 {
            if selector < 0xf {
                if selector < 0xb {
                    if selector < 5 {
                        if selector < 4 { 2 } else { 3 }
                    } else {
                        4
                    }
                } else {
                    5
                }
            } else {
                6
            }
        } else {
            7
        }
    } else if selector < 0xf {
        if selector < 0xd {
            if selector < 7 {
                if selector < 5 {
                    if selector < 4 { 2 } else { 3 }
                } else {
                    4
                }
            } else {
                5
            }
        } else {
            6
        }
    } else {
        7
    }
}

fn require_len(field: &'static str, actual: usize, expected: usize) -> Result<(), InitBlockError> {
    if actual != expected {
        return Err(InitBlockError::ShapeMismatch {
            field,
            expected,
            actual,
        });
    }
    Ok(())
}

/// Native high-frequency spectral-cut gate (`init_channel_block_at5`,
/// decompile 34685: `param_6 < 0x10 && *(cfg+0xac) == 0xac44 && (uVar5 < 0x20
/// && 0x17 < uVar5)`). `selector` is `param_6`, `sr_ac` is the sample-rate word
/// `cfg+0xac` (0xac44 = 44100), `effective_extent` is the PER-FRAME band extent
/// `cfg+0xb4` (`uVar5`). When open, lines `[start..0x800)` of BOTH per-channel
/// spectra are zeroed (see [`init_high_frequency_cut_start_at5`]). First live at
/// a MONO rate at 64 kbps (selector 15, extent 27); natively also live at stereo
/// 48/64 (selector 13/15, extent 26/27). docs/14 §3.1.
pub fn init_high_frequency_cut_gate_open(
    selector: i32,
    sr_ac: u32,
    effective_extent: usize,
) -> bool {
    selector < 0x10 && sr_ac == 0xac44 && (0x18..0x20).contains(&effective_extent)
}

/// Native high-frequency spectral-cut start line (`init_channel_block_at5`,
/// decompile 34685-34710; disassembly 0x3ff4f-0x40088). The first spectral line
/// to zero — lines `[start..0x800)` of both per-channel spectra are set to 0.
/// Only meaningful when [`init_high_frequency_cut_gate_open`] is true (the isps
/// lookup assumes a live `0x18..0x20` extent).
///
/// Native law (decompile 34689):
/// ```text
/// cut = (int)ROUND((float)((int)ROUND((float)isps[uVar5] * 0.010766) * 1000)
///                  / 10.766);
/// if (cut < 0) cut += 0xf;
/// start = (cut >> 4) << 4;   // trunc-div by 16, x16
/// ```
/// Both ROUNDs are x87 `fistpl` with the control word set to TRUNCATE
/// (`mov $0xc,%dh` -> RC=11): i.e. C `(int)` casts, NOT round-to-nearest.
/// Arithmetic is x87 extended precision; the two constants are f32 loads
/// (multiplier bits `0x3c3063e0` ~= 0.010766 at native .rodata 0xc1ccc, divisor
/// bits `0x412c4189` ~= 10.766 at 0xc1cd0). f64 emulation on the exact f32
/// constant values (promoted to f64) with `as i32` truncation is decision-exact
/// at every reachable extent — margins >= 0.15 on every truncation: extent 27 ->
/// trunc(1408*c)=15 -> trunc(15000/d)=1393 -> start 1392; extent 26 -> 13 ->
/// trunc(13000/d)=1207 -> start 1200; extent 24 -> 11 -> trunc(11000/d)=1021 ->
/// start 1008. docs/14 §3.1.
pub fn init_high_frequency_cut_start_at5(effective_extent: usize) -> usize {
    let multiplier = f64::from(f32::from_bits(0x3c30_63e0)); // ~0.010766
    let divisor = f64::from(f32::from_bits(0x412c_4189)); // ~10.766
    let isps = isps_at5();
    let isps_v = f64::from(isps[effective_extent]);
    // Inner truncating fistp: (int)(isps * 0.010766).
    let inner = (isps_v * multiplier) as i32;
    // Outer truncating fistp: (int)((inner * 1000) / 10.766).
    let mut cut = (f64::from(inner) * 1000.0 / divisor) as i32;
    if cut < 0 {
        cut += 0xf;
    }
    ((cut >> 4) << 4) as usize
}

pub fn init_channel_block_frame_at5(
    frame: &mut InitFrameState,
) -> Result<InitFrameOutput, InitBlockError> {
    // Channel-mode gate. Stereo (2) has shipped since docs/13; mono (1) is
    // brought up rate by rate under docs/14. Every mono-specific block below
    // is already ported (word0 ladder `selector_word0(sel, false)`, the mono
    // force-7 override, the selector-9 Block N window, and the joint-stereo
    // SWAP that is natively inside `if (param_5 == 2)`), so widening this gate
    // to accept `channel_count == 1` composes them. Reject 0 and >2 with the
    // existing OutOfScope shape.
    if frame.channel_count == 0 || frame.channel_count > 2 {
        return Err(InitBlockError::OutOfScope(
            "channel_count must be 1 (mono) or 2 (stereo)",
        ));
    }
    // Native band-loop extent `uVar5 = *(uint *)(side + 0xb4)` (decompile
    // line 34394): 32 at full-band rates (320/352), but the reduced band
    // index at lower rates (27 at 96/128, 28 at 160, …). Native processes
    // `uVar5` bands into fixed 32-wide (`0x20`) per-channel storage, leaving
    // the `[uVar5..32]` tail at its initialization value. Accept any extent
    // that fits the 32-wide storage; the per-band scratch/output arrays below
    // are sized to 32 and the loops run to `uv5`, so the untouched tail
    // matches native exactly. At `band_count == 32` this is identical to the
    // previous 32-only path (production feeds 32 via the bridge).
    if frame.band_count == 0 || frame.band_count > INIT_BANDS_AT5 {
        return Err(InitBlockError::OutOfScope(
            "band count (side +0xb4) must fit the 32-wide storage",
        ));
    }
    // Per-rate gain scan bound (`+0x1b48c` seed = `g_a_x_at5[band_index]+1`): 16
    // full-band, 13 at 192 (docs/13 §3.1). The detector gain-record buffers stay
    // 16-wide; `classify_gain_records` scans down from `gain_band_count` over
    // them. Reject only a count that would read past the 16-record buffer.
    if frame.gain_band_count > INIT_GAIN_ROWS_AT5 {
        return Err(InitBlockError::OutOfScope(
            "gain-record band count exceeds the 16-record buffer",
        ));
    }
    if frame.channels.len() != frame.channel_count {
        return Err(InitBlockError::ShapeMismatch {
            field: "channels",
            expected: frame.channel_count,
            actual: frame.channels.len(),
        });
    }

    let selector = frame.selector;
    let uv5 = frame.band_count;
    let stereo = frame.channel_count == 2;

    // Non-live spectrum-scaling gates (blocks F/G/H/I). At 352 kbps
    // none is active; reject explicitly if the native gate opens.
    if (selector < 0xc && stereo) || (selector == 9 && !stereo) {
        return Err(InitBlockError::OutOfScope(
            "0.89-scale branch (selector < 0xc) is out of scope",
        ));
    }
    // Config flag-word (`cfg+0x1dc`) ×0.94 spectrum scaling (decompile
    // 34669–34686, native 0x3f870 / Ghidra 0x4f870). When any of the six
    // native bit patterns holds, both per-channel spectrum buffers
    // (`param_3`/`param_2` == spectrum_b/spectrum_a, 0x800 f32 each) are
    // scaled in place by the exact f32 0.94 BEFORE the joint-stereo swap.
    // Applied below (after the require_len length checks). The selector-gated
    // 0.89126587 branch (decompile 34654–34668) stays fail-explicit — see the
    // `selector < 0xc` gate above.
    let flags = frame.flags_1dc;
    let scale_094 = (flags & 0xc) == 4
        || (flags & 0x18) == 8
        || (flags & 0x30) == 0x10
        || (flags & 0x18) == 0x10
        || (flags & 0x30) == 0x20
        || (flags & 0x60) == 0x40;
    for (index, channel) in frame.channels.iter().enumerate() {
        let _ = index;
        require_len(
            "gain_a_records",
            channel.gain_a_records.len(),
            INIT_GAIN_ROWS_AT5 * INIT_GAIN_ROW_INTS_AT5,
        )?;
        require_len(
            "gain_b_records",
            channel.gain_b_records.len(),
            INIT_GAIN_ROWS_AT5 * INIT_GAIN_ROW_INTS_AT5,
        )?;
        require_len("b_9c8", channel.b_9c8.len(), INIT_BANDS_AT5)?;
        require_len("b_a48", channel.b_a48.len(), INIT_BANDS_AT5)?;
        require_len(
            "spectrum_a",
            channel.spectrum_a.len(),
            INIT_SPECTRUM_WORDS_AT5,
        )?;
        require_len(
            "spectrum_b",
            channel.spectrum_b.len(),
            INIT_SPECTRUM_WORDS_AT5,
        )?;
    }

    // Apply the ×0.94 config-flag spectrum scaling (decompile 34669–34686)
    // over every channel's two 0x800-float spectra, before the joint-stereo
    // swap consumes them. Exact f32 constant.
    if scale_094 {
        for channel in frame.channels.iter_mut() {
            for value in channel.spectrum_a.iter_mut() {
                *value *= 0.94f32;
            }
            for value in channel.spectrum_b.iter_mut() {
                *value *= 0.94f32;
            }
        }
    }

    // High-frequency spectral cut (`init_channel_block_at5` decompile
    // 34685-34710, native 0x3f870 / Ghidra 0x4f870; disassembly
    // 0x3ff4f-0x40088). When the native gate is open (selector < 0x10, sample
    // rate 0xac44, PER-FRAME extent `cfg+0xb4` in 0x18..0x20), lines
    // `[start..0x800)` of BOTH per-channel spectra are zeroed IN PLACE — AFTER
    // the 0.94 scaling and BEFORE the joint-stereo swap (native order:
    // 0.89-gate -> 0.94-scale -> CUT -> swap). All downstream init computation
    // (energy ratios, max/idsf rows, band averages, word-length seeds) then
    // reads the cut spectra, and the caller's norm stage observes the cut
    // through the same buffer. First live at a MONO rate at 64 kbps (selector
    // 15, extent 27 -> start 1392); a live write-watchpoint logged the zero
    // store at native PC 0x40053 (docs/14 §3.1).
    if init_high_frequency_cut_gate_open(selector, frame.sr_ac, frame.extent_b4) {
        let start = init_high_frequency_cut_start_at5(frame.extent_b4);
        for channel in frame.channels.iter_mut() {
            for value in channel.spectrum_a[start..INIT_SPECTRUM_WORDS_AT5].iter_mut() {
                *value = 0.0;
            }
            for value in channel.spectrum_b[start..INIT_SPECTRUM_WORDS_AT5].iter_mut() {
                *value = 0.0;
            }
        }
    }

    // Per-group joint-stereo spectrum SWAP (decompile 34711–34766, native
    // offset 0x3f870 / Ghidra 0x4f870 — Ghidra = native + 0x10000). This
    // branch sits AFTER the high-frequency-zero gate (34685–34710) and
    // BEFORE the energy-ratio loop (34768+), so all downstream init
    // computation (energy ratios, max rows, weights, idsf planes, band
    // averages, word-length seeds) consumes the SWAPPED spectra. Native:
    //
    //   if (param_5 == 2) {                              // stereo only
    //     for (g = 0; g < *(cfg + 0xbc); g++)            // gain band count
    //       if (*(cfg + 0x50 + g*4) == 1) {             // join flag group g
    //         off = g * 0x200;                          // 0x80 f32 words
    //         // spectrum B (param_3) first: exchange ch0<->ch1 group;
    //         // spectrum A (param_2) next: same exchange.
    //       }
    //   }
    //
    // The decompile hand-rolls a three-copy exchange through a `local_21c`
    // scratch (34718–34739 for B, 34740–34761 for A); the net effect is a
    // pure ch0<->ch1 group swap of the 128-float slice [g*128, (g+1)*128)
    // in BOTH spectra (order irrelevant — no float math). The `cfg+0x50`
    // row is the same native memory as the one-call-delayed stereo swap
    // flags (`tone_secondary_words`); the bridge feeds it as `join_flags_50`.
    // In-place mutation of the caller-owned spectra matches native, so the
    // caller observes the swapped surface after init returns.
    if stereo {
        let group_words = INIT_SPECTRUM_WORDS_AT5 / INIT_GAIN_ROWS_AT5; // 0x80 = 128
        for g in 0..frame.gain_band_count {
            if frame.join_flags_50.get(g).copied() != Some(1) {
                continue;
            }
            let start = g * group_words;
            let end = start + group_words;
            let (c0, c1) = frame.channels.split_at_mut(1);
            c0[0].spectrum_b[start..end].swap_with_slice(&mut c1[0].spectrum_b[start..end]);
            c0[0].spectrum_a[start..end].swap_with_slice(&mut c1[0].spectrum_a[start..end]);
        }
    }

    let isps = isps_at5();
    let nsps = nsps_at5();
    let y_table = y_at5();

    // Block A: shared +0x24 and side +0x90 from object-side +0x1c of
    // object[0]; side +0xa0 from the channel count.
    let objside0_1c = frame.channels[0].objside_1c;
    let (side_field_90, shared_word24) = if objside0_1c == 2 {
        (0u32, 0x10i32)
    } else {
        (1u32, 4i32)
    };
    let side = InitSideOutput {
        field_90: side_field_90,
        field_94: 0,
        field_98: 0,
        field_9c: 0,
        field_a0: u32::from(frame.channel_count != 1),
        field_118: 0,
    };

    // Per-channel output accumulators.
    let mut class_42 = [0i16; 2];
    let mut flag_44 = [0i16; 2];
    let mut word0 = [0i16; 2];
    let mut ratio_44c = [0f32; 2];
    let mut ratio_450 = [0f32; 2];
    let mut b1b48c = [0i32; 2];
    let mut b1b484 = [0i32; 2];
    let mut b1b488 = [0i32; 2];
    let mut b1b490 = [0i32; 2];
    let mut max_b_2cc: Vec<Vec<f32>> = Vec::with_capacity(2);
    let mut max_a_34c: Vec<Vec<f32>> = Vec::with_capacity(2);
    let mut idsf_1b678: Vec<Vec<i32>> = Vec::with_capacity(2);
    let mut a_9c8: Vec<Vec<i32>> = Vec::with_capacity(2);
    let mut a_a48: Vec<Vec<f32>> = Vec::with_capacity(2);
    let mut word_lengths_4c: Vec<Vec<i32>> = Vec::with_capacity(2);
    let mut weight_3cc: Vec<Vec<f32>> = Vec::with_capacity(2);
    let mut tonality_458 = [0f32; 2];

    // Block E (mono only, out of scope path): word0 forced to 7 when the
    // object-side +0x14 word of object[0] is nonzero.
    let mono_force7 = !stereo && frame.channels[0].objside_14 != 0;

    // Blocks B..M, per channel.
    for channel in &frame.channels {
        let (f44, l48c, l484, l490, l488) = classify_gain_records(channel, frame.gain_band_count);

        // Block E: block word0 from the selector.
        let idx0 = idsf_1b678.len();
        word0[idx0] = if mono_force7 {
            7
        } else {
            selector_word0(selector, stereo)
        };

        // Block J: energy ratios and class from spectrum A.
        let spec_a = &channel.spectrum_a;
        let sum_lo = f32_abs_sum(spec_a, 0, 0x10);
        let sum_mid = f32_abs_sum(spec_a, 0x10, 0x80);
        let sum_hi = f32_abs_sum(spec_a, 0x80, 0x100);
        let num = sum_lo * 0.0625f32; // exact 2^-4
        let denom_44c = f64::from(sum_mid) / 112.0;
        let r44c = if denom_44c <= 0.0 {
            1.0f32
        } else {
            (f64::from(num) / denom_44c) as f32
        };
        let denom_450 = f64::from(sum_hi) * 0.0078125; // exact 2^-7
        let r450 = if denom_450 <= 0.0 {
            1.0f32
        } else {
            (f64::from(num) / denom_450) as f32
        };

        let mut c42: i16 = if r44c <= 4.0 { 1 } else { 2 };
        if f44 == 0 {
            c42 += 1;
        }
        if r450 <= 8.0 {
            if r450 > 4.0 {
                c42 += 1;
            }
        } else {
            c42 += 2;
        }
        if c42 < 2 && r450 > 1.0 {
            c42 = 2;
        }

        let idx = idsf_1b678.len();
        flag_44[idx] = f44;
        b1b48c[idx] = l48c;
        b1b484[idx] = l484;
        b1b490[idx] = l490;
        b1b488[idx] = l488;
        class_42[idx] = c42;
        ratio_44c[idx] = r44c;
        ratio_450[idx] = r450;

        // Block L: idsf from spectrum B (-> obj+0x1b678, max block+0x2cc)
        // and spectrum A (-> gainA+0x9c8, max block+0x34c). Storage is fixed
        // 32-wide (native `0x20`); only the leading `uv5` bands are written,
        // so the `[uv5..32]` tail stays at the init value (0 / 0.0) exactly as
        // native leaves it.
        let mut idsf_b = vec![0u32; INIT_BANDS_AT5];
        let mut maxb = vec![0f32; INIT_BANDS_AT5];
        set_idsf_from_mdspec_at5(&channel.spectrum_b, &mut idsf_b, &mut maxb, uv5)?;
        let mut idsf_a = vec![0u32; INIT_BANDS_AT5];
        let mut maxa = vec![0f32; INIT_BANDS_AT5];
        set_idsf_from_mdspec_at5(&channel.spectrum_a, &mut idsf_a, &mut maxa, uv5)?;

        // Block L: per-band mdspec average (gainA +0xa48). 32-wide storage; the
        // `[uv5..32]` tail stays at the 1.0 init value (native leaves it).
        let mut avg = vec![1.0f32; INIT_BANDS_AT5];
        let mut band = 0usize;
        while band < uv5 {
            let threshold = maxb[band];
            if !(threshold > 0.0) {
                band += 1;
                continue;
            }
            let start = usize::from(isps[band]);
            let end = usize::from(isps[band + 1]);
            let mut acc = 0f32;
            if start < end {
                acc = f32_abs_sum(&channel.spectrum_b, start, end);
            }
            avg[band] = ((f64::from(nsps[band]) * f64::from(threshold)) / f64::from(acc)) as f32;
            band += 1;
        }

        // Block M: word-length seed (block+0x4c) and aux weight (+0x3cc).
        // 32-wide storage; native writes only the leading `uv5` bands, so the
        // `[uv5..32]` tail stays 0 (word length) / 0.0 (weight) as native
        // leaves the buffer.
        let a9 = idsf_a.iter().map(|&v| v as i32).collect::<Vec<_>>();
        let mut wl = vec![0i32; INIT_BANDS_AT5];
        let mut weight = vec![0f32; INIT_BANDS_AT5];
        for b in 0..uv5 {
            let sum = a9[b] + channel.b_9c8[b];
            wl[b] = (f64::from(sum) * 0.5 + 0.5) as i32;
            weight[b] = ((f64::from(avg[b]) + f64::from(channel.b_a48[b])) * 0.5) as f32;
        }

        // Block M: tonality (block+0x458), mean of the leading weights.
        let ycount = usize::from(y_table[channel.y_index as usize]).min(uv5);
        let mut acc = 0f64;
        for &w in weight.iter().take(ycount) {
            acc += f64::from(w);
        }
        let sf = acc as f32;
        tonality_458[idx] = if ycount > 0 {
            (f64::from(sf) / ycount as f64) as f32
        } else {
            sf
        };

        max_b_2cc.push(maxb);
        max_a_34c.push(maxa);
        idsf_1b678.push(idsf_b.iter().map(|&v| v as i32).collect());
        a_9c8.push(a9);
        a_a48.push(avg);
        word_lengths_4c.push(wl);
        weight_3cc.push(weight);
    }

    // Block K: stereo +0x42 reconcile.
    if stereo {
        let diff = (i32::from(class_42[0]) - i32::from(class_42[1])).abs();
        if diff == 1 {
            let m = class_42[0].max(class_42[1]);
            class_42[0] = m;
            class_42[1] = m;
        }
    }

    // Block N: block +0x454 = (word0*10)/max, where max = max over all
    // channels/bands of the +0x4c word lengths, floored at 1.
    let mut max_wl = 1i32;
    for wl in &word_lengths_4c {
        for &v in wl.iter().take(uv5) {
            if v > max_wl {
                max_wl = v;
            }
        }
    }

    let mut channels_out = Vec::with_capacity(frame.channel_count);
    for (ci, channel) in frame.channels.iter().enumerate() {
        let base = word0[ci];
        let mut rows = vec![0i16; INIT_BANDS_AT5 + 1];
        rows[0] = base;
        for slot in rows.iter_mut().take(INIT_BANDS_AT5 + 1).skip(1) {
            *slot = base;
        }
        // Weight bonus.
        for b in 0..uv5 {
            let w = weight_3cc[ci][b];
            if w >= 6.0 {
                rows[b + 1] += 2;
            } else if w >= 3.5 {
                rows[b + 1] += 1;
            }
        }

        // Block N: gain-scan transient byte (`block+0x45c`) plus the
        // first-8-row word bump (decompile 35024-35049, native
        // 0x40c8c-0x40d16). Native order: aux weight bonus above →
        // this block → class min-clamp → 7-clamp (below). Gate:
        // `(param_5 == 2 && (param_6 - 0xb) < 9) || (param_6 == 9 &&
        // param_5 == 1)` — stereo selector 0xb..=0x13, or mono selector 9.
        // First live at 96 kbps (selector 19); statically dead at 128+
        // (selector >= 23) and at 352. When the gate is closed, native
        // leaves the byte unwritten/stale; every native consumer reads
        // `+0x45c` under the same selector window, so emitting `false`
        // outside the window is consumer-faithful.
        let mut transient_45c = false;
        let gate_open =
            (stereo && (0..=8).contains(&(selector - 0xb))) || (selector == 9 && !stereo);
        if gate_open {
            // gainA (`obj+0x8`) record 0: word 0 = point count, words
            // 8.. = level ids. For each of `count` level ids not in
            // `{5,6,7}` (`(level - 5) as u32 > 2`), set the byte and bump
            // rows[1..=8] by 1 — per qualifying entry, so a call with two
            // qualifying levels bumps each of the first eight rows by 2.
            let count = record_word(&channel.gain_a_records, 0, 0);
            let mut i = 0;
            while i < count {
                let level = record_word(&channel.gain_a_records, 0, 8 + i as usize);
                if (level.wrapping_sub(5) as u32) > 2 {
                    transient_45c = true;
                    for slot in rows.iter_mut().take(9).skip(1) {
                        *slot += 1;
                    }
                }
                i += 1;
            }
        }

        // Min-clamp first 8 to the energy class, then max-clamp all to 7.
        for slot in rows.iter_mut().take(9).skip(1) {
            if *slot < class_42[ci] {
                *slot = class_42[ci];
            }
        }
        for slot in rows.iter_mut().take(uv5 + 1).skip(1) {
            if *slot > 7 {
                *slot = 7;
            }
        }

        let scaled_454 = ((f64::from(i32::from(base) * 10)) / f64::from(max_wl)) as f32;

        channels_out.push(InitChannelOutput {
            word0: base,
            word_rows: rows,
            class_42: class_42[ci],
            flag_44: flag_44[ci],
            word_lengths_4c: word_lengths_4c[ci].clone(),
            max_b_2cc: max_b_2cc[ci].clone(),
            max_a_34c: max_a_34c[ci].clone(),
            weight_3cc: weight_3cc[ci].clone(),
            ratio_44c: ratio_44c[ci],
            ratio_450: ratio_450[ci],
            scaled_454,
            tonality_458: tonality_458[ci],
            block_100c: channel.objside_ptr,
            block_1010: channel.spec_b_ptr,
            obj_1b48c: b1b48c[ci],
            obj_1b484: b1b484[ci],
            obj_1b488: b1b488[ci],
            obj_1b490: b1b490[ci],
            obj_1b5f8: vec![0i32; INIT_BANDS_AT5],
            obj_1b578: vec![0i32; INIT_BANDS_AT5],
            idsf_1b678: idsf_1b678[ci].clone(),
            a_9c8: a_9c8[ci].clone(),
            a_a48: a_a48[ci].clone(),
            transient_45c,
        });
    }

    // Block D: shared struct.
    let shared = InitSharedOutput {
        // shared[0] = side address + 0x90; represented structurally by
        // the test against the captured side address.
        word0_ptr: 0,
        word21: 1,
        word22: 0,
        word23: 1,
        word24: shared_word24,
        row_94: vec![0u16; uv5],
        row_d4: vec![0u16; uv5],
    };

    Ok(InitFrameOutput {
        channels: channels_out,
        shared,
        side,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_word0_stereo_ladder() {
        // Native 0x3fe25 stereo ladder thresholds.
        assert_eq!(selector_word0(3, true), 2);
        assert_eq!(selector_word0(4, true), 3);
        assert_eq!(selector_word0(5, true), 4);
        assert_eq!(selector_word0(0xa, true), 4);
        assert_eq!(selector_word0(0xb, true), 5);
        assert_eq!(selector_word0(0xe, true), 5);
        assert_eq!(selector_word0(0xf, true), 6);
        assert_eq!(selector_word0(0x12, true), 6);
        assert_eq!(selector_word0(0x13, true), 7);
        assert_eq!(selector_word0(30, true), 7);
    }

    #[test]
    fn selector_word0_mono_ladder() {
        // Native 0x3fdb0 mono ladder thresholds.
        assert_eq!(selector_word0(3, false), 2);
        assert_eq!(selector_word0(4, false), 3);
        assert_eq!(selector_word0(6, false), 4);
        assert_eq!(selector_word0(7, false), 5);
        assert_eq!(selector_word0(0xc, false), 5);
        assert_eq!(selector_word0(0xd, false), 6);
        assert_eq!(selector_word0(0xe, false), 6);
        assert_eq!(selector_word0(0xf, false), 7);
    }

    fn record(count: i32, points: &[i32]) -> Vec<u32> {
        let mut record = vec![0u32; INIT_GAIN_ROW_INTS_AT5];
        record[0] = count as u32;
        for (i, &p) in points.iter().enumerate() {
            // point i occupies word 8+i (the +0x20 slot) and word 1+i
            // (the +0x4 slot) in the native record.
            record[8 + i] = p as u32;
            record[1 + i] = p as u32;
        }
        record
    }

    fn records16(first_two: [(i32, Vec<i32>); 2]) -> Vec<u32> {
        let mut out = Vec::new();
        for (count, points) in first_two {
            out.extend(record(count, &points));
        }
        for _ in 2..INIT_GAIN_ROWS_AT5 {
            out.extend(vec![0u32; INIT_GAIN_ROW_INTS_AT5]);
        }
        out
    }

    #[test]
    fn high_frequency_cut_start_law() {
        // Native trunc law (decompile 34689): extent 24/26/27 -> start
        // 1008/1200/1392. isps[27]=1408 -> trunc(1408*0.010766)=15 ->
        // trunc(15000/10.766)=1393 -> (1393>>4)<<4 = 1392.
        assert_eq!(init_high_frequency_cut_start_at5(24), 1008);
        assert_eq!(init_high_frequency_cut_start_at5(26), 1200);
        assert_eq!(init_high_frequency_cut_start_at5(27), 1392);
    }

    #[test]
    fn high_frequency_cut_gate_window() {
        // Gate: selector < 0x10 && sr == 0xac44 && extent in 0x18..0x20.
        // OPEN for the live 64/48 mono/stereo cases.
        assert!(init_high_frequency_cut_gate_open(15, 0xac44, 27));
        assert!(init_high_frequency_cut_gate_open(13, 0xac44, 26));
        assert!(init_high_frequency_cut_gate_open(0, 0xac44, 24));
        // CLOSED: selector >= 0x10 (e.g. 96/128 mono at extent 27/32).
        assert!(!init_high_frequency_cut_gate_open(0x13, 0xac44, 27));
        assert!(!init_high_frequency_cut_gate_open(0x10, 0xac44, 27));
        // CLOSED: full-band extent 32 (0x20 not < 0x20).
        assert!(!init_high_frequency_cut_gate_open(15, 0xac44, 32));
        // CLOSED: extent 23 and below (< 0x18).
        assert!(!init_high_frequency_cut_gate_open(15, 0xac44, 23));
        assert!(!init_high_frequency_cut_gate_open(15, 0xac44, 0));
        // CLOSED: sample rate != 0xac44.
        assert!(!init_high_frequency_cut_gate_open(15, 0xbb80, 27));
    }

    #[test]
    fn gain_spread_gate() {
        // spread <= 3 keeps the record classifiable; > 3 trips.
        assert!(!gain_spread_over_3(&records16([
            (3, vec![6, 7, 9]),
            (0, vec![])
        ])));
        assert!(gain_spread_over_3(&records16([
            (2, vec![6, 11]),
            (0, vec![])
        ])));
        // Empty records collapse to max==min==6 (no spread).
        assert!(!gain_spread_over_3(&records16([(0, vec![]), (0, vec![])])));
    }

    #[test]
    fn classify_gain_flag_prefers_array_a_then_b() {
        let channel = |a: Vec<u32>, b: Vec<u32>| InitChannelState {
            objside_1c: 0,
            objside_14: 0,
            objside_ptr: 0,
            spec_b_ptr: 0,
            y_index: 16,
            gain_a_records: a,
            gain_b_records: b,
            b_9c8: vec![0; INIT_BANDS_AT5],
            b_a48: vec![1.0; INIT_BANDS_AT5],
            spectrum_a: vec![0.0; INIT_SPECTRUM_WORDS_AT5],
            spectrum_b: vec![0.0; INIT_SPECTRUM_WORDS_AT5],
        };
        // Array A trips -> flag 0.
        let ch = channel(
            records16([(2, vec![6, 11]), (0, vec![])]),
            records16([(0, vec![]), (0, vec![])]),
        );
        assert_eq!(classify_gain_records(&ch, 16).0, 0);
        // Array A clean, array B trips -> flag 1.
        let ch = channel(
            records16([(3, vec![6, 7, 8]), (0, vec![])]),
            records16([(2, vec![6, 11]), (0, vec![])]),
        );
        assert_eq!(classify_gain_records(&ch, 16).0, 1);
        // Both clean -> flag -1.
        let ch = channel(
            records16([(3, vec![6, 7, 8]), (0, vec![])]),
            records16([(0, vec![]), (0, vec![])]),
        );
        assert_eq!(classify_gain_records(&ch, 16).0, -1);
    }

    // Config flag-word (`cfg+0x1dc`) ×0.94 spectrum scaling
    // (`init_channel_block_at5`, decompile 34669–34686, native 0x3f870 /
    // Ghidra 0x4f870). When a matching flag pattern holds, both per-channel
    // spectra are scaled in place by the exact f32 0.94 before the joint-stereo
    // swap; otherwise they are untouched. docs/12 §4.3 b-residual.
    fn init_frame_with_flags(flags_1dc: u32) -> InitFrameState {
        // Distinctive, deterministic spectra so a ×0.94 scale is detectable.
        let spectrum = |base: f32| -> Vec<f32> {
            (0..INIT_SPECTRUM_WORDS_AT5)
                .map(|i| base + i as f32 * 0.001)
                .collect()
        };
        let channel = |seed: f32| InitChannelState {
            objside_1c: 0,
            objside_14: 0,
            objside_ptr: 0,
            spec_b_ptr: 0,
            y_index: 16,
            // Clean records (no spread trip) so classification stays simple.
            gain_a_records: records16([(0, vec![]), (0, vec![])]),
            gain_b_records: records16([(0, vec![]), (0, vec![])]),
            b_9c8: vec![0; INIT_BANDS_AT5],
            b_a48: vec![1.0; INIT_BANDS_AT5],
            spectrum_a: spectrum(seed),
            spectrum_b: spectrum(seed + 100.0),
        };
        InitFrameState {
            channels: vec![channel(1.0), channel(2.0)],
            channel_count: 2,
            // Selector 30 (the 352 path): >= 0xc so the 0.89 gate is skipped,
            // >= 0x10 so the HF-zero gate is skipped, and `30 - 0xb = 19` is not
            // in 0..=8 so the +0x45c gate is skipped.
            selector: 30,
            band_count: INIT_BANDS_AT5,
            // Full-band per-frame extent: selector 30 keeps the HF-cut gate
            // closed (0x20 not in 0x18..0x20) regardless.
            extent_b4: INIT_BANDS_AT5,
            gain_band_count: INIT_GAIN_ROWS_AT5,
            flags_1dc,
            sr_ac: 0xac44,
            // No joint-stereo swap: isolate the ×0.94 scaling.
            join_flags_50: vec![0i32; INIT_GAIN_ROWS_AT5],
        }
    }

    fn original_spectrum(base: f32) -> Vec<f32> {
        (0..INIT_SPECTRUM_WORDS_AT5)
            .map(|i| base + i as f32 * 0.001)
            .collect()
    }

    #[test]
    fn init_config_flag_scales_both_spectra_by_0_94() {
        // `flags = 0x40`: `(f & 0x60) == 0x40` -> in the scaling set.
        let mut frame = init_frame_with_flags(0x40);
        init_channel_block_frame_at5(&mut frame).expect("init runs on the 352 path");
        for (ci, seed) in [(0usize, 1.0f32), (1usize, 2.0f32)] {
            let expect_a: Vec<f32> = original_spectrum(seed).iter().map(|v| v * 0.94).collect();
            let expect_b: Vec<f32> = original_spectrum(seed + 100.0)
                .iter()
                .map(|v| v * 0.94)
                .collect();
            assert_eq!(
                frame.channels[ci].spectrum_a, expect_a,
                "ch{ci} spectrum_a scaled"
            );
            assert_eq!(
                frame.channels[ci].spectrum_b, expect_b,
                "ch{ci} spectrum_b scaled"
            );
        }
    }

    #[test]
    fn init_config_flag_zero_leaves_spectra_untouched() {
        let mut frame = init_frame_with_flags(0);
        init_channel_block_frame_at5(&mut frame).expect("init runs on the 352 path");
        for (ci, seed) in [(0usize, 1.0f32), (1usize, 2.0f32)] {
            assert_eq!(
                frame.channels[ci].spectrum_a,
                original_spectrum(seed),
                "ch{ci} spectrum_a"
            );
            assert_eq!(
                frame.channels[ci].spectrum_b,
                original_spectrum(seed + 100.0),
                "ch{ci} spectrum_b"
            );
        }
    }

    #[test]
    fn init_config_flag_pattern_outside_set_leaves_spectra_untouched() {
        // `flags = 0x3c`: `(f&0xc)==0xc` but every scaling condition is false
        // (`&0x18=0x18`, `&0x30=0x30`, `&0x60=0x20`) -> NOT in the set.
        let mut frame = init_frame_with_flags(0x3c);
        init_channel_block_frame_at5(&mut frame).expect("init runs on the 352 path");
        for (ci, seed) in [(0usize, 1.0f32), (1usize, 2.0f32)] {
            assert_eq!(
                frame.channels[ci].spectrum_a,
                original_spectrum(seed),
                "ch{ci} spectrum_a"
            );
            assert_eq!(
                frame.channels[ci].spectrum_b,
                original_spectrum(seed + 100.0),
                "ch{ci} spectrum_b"
            );
        }
    }
}
