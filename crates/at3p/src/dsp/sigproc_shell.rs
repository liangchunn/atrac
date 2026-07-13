//! `at5enc_sigproc` shell pieces.
//!
//! The state-rotation prologue follows the decompiled `at5enc_sigproc` at
//! native `0x4f2b0` (decompile from `decompiled/libatrac.c` line 42931):
//! shift the detector-history word blocks, zero the freed tails, swap each
//! channel's current/previous record scratch pointers, rotate the five-slot
//! pointer ring, and shift the config flag word.

pub const SIGPROC_DETECTOR_STRUCT_WORDS: usize = 0x200;
pub const SIGPROC_CHANNEL_RING_SLOTS: usize = 5;

/// Canonical detector-arena length once the mode-3 intensity-stereo spectral
/// block (docs/13 §3.1) is in play. The block's own pan/weight rows and the
/// power-reconstruction stores reach detector byte `0x844 + 0x40 = 0x884`
/// (word `0x220`, last written index `0x220`), so the arena must hold words
/// `0..=0x220`, i.e. `0x221` words. Words `0x200..0x221` are the mode-3
/// weight-row extension of the arena, past the `0x200`-word region the head-of-
/// frame history rotation (`sigproc_rotate_detector_history_at5`) walks; the
/// rotation never touches them (they roll via the block's own pan/weight
/// rolls).
pub const SIGPROC_DETECTOR_ARENA_WORDS: usize = 0x221;

/// Per-channel sigproc scratch layout (`*(param_2[1] + channel * 4)` in the
/// decompile): sixteen `0x1200`-byte band blocks, each nine rolling
/// 128-float slots, followed at `+0x12000` by the 384-float PQF analysis
/// delay line (`add $0x12000, %edx` at native `0x4f657`).
pub const SIGPROC_BAND_COUNT: usize = 16;
pub const SIGPROC_BAND_SLOTS: usize = 9;
pub const SIGPROC_BAND_SLOT_FLOATS: usize = 0x80;
pub const SIGPROC_BAND_BLOCK_BYTES: u32 = 0x1200;
pub const SIGPROC_BAND_SLOT_BYTES: u32 = 0x200;
pub const SIGPROC_PQF_DELAY_OFFSET: u32 = 0x12000;
pub const SIGPROC_PQF_DELAY_FLOATS: usize = 0x180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigprocShellError {
    DetectorStructTooShort {
        needed: usize,
        actual: usize,
    },
    BandBlockWrongLength {
        needed: usize,
        actual: usize,
    },
    SlotIndexOutOfRange {
        slot: usize,
    },
    BandIndexOutOfRange {
        band: usize,
    },
    BandLimitOutOfRange {
        band_limit: i32,
    },
    /// The mode-3 `band_count` (`detector_words[0]`) exceeded the 16-band
    /// spectrum; never happens at any in-scope rate (fail-explicit rule).
    IntensityBandCountOutOfRange {
        band_count: usize,
    },
    /// Mode-3 intensity stereo is native stereo-only (`param_3 == 2`).
    IntensityChannelCount {
        channel_count: usize,
    },
    /// The detector arena is shorter than [`SIGPROC_DETECTOR_ARENA_WORDS`], so
    /// the mode-3 pan/weight rows would index out of bounds.
    DetectorArenaTooShort {
        needed: usize,
        actual: usize,
    },
    /// A `power_reconst_at5` source/destination window was shorter than the
    /// requested count.
    PowerReconstWindowTooShort {
        needed: usize,
        actual: usize,
    },
}

/// Native detector-history rotation over the `*(channel_block) + 0x10`
/// word struct: six `memmove` block shifts (each destination one row of 16
/// words below its source) followed by zeroing the freed 16-word tails.
pub fn sigproc_rotate_detector_history_at5(words: &mut [u32]) -> Result<(), SigprocShellError> {
    if words.len() < SIGPROC_DETECTOR_STRUCT_WORDS {
        return Err(SigprocShellError::DetectorStructTooShort {
            needed: SIGPROC_DETECTOR_STRUCT_WORDS,
            actual: words.len(),
        });
    }

    words.copy_within(0x21..0x21 + 0x10, 0x11);
    words.copy_within(0x31..0x31 + 0x30, 0x21);
    words.copy_within(0x71..0x71 + 0x30, 0x61);
    words.copy_within(0xb1..0xb1 + 0x30, 0xa1);
    words.copy_within(0xf1..0xf1 + 0x30, 0xe1);
    words.copy_within(0x131..0x131 + 0x60, 0x121);
    for index in 0..0x10 {
        words[0x51 + index] = 0;
        words[0x91 + index] = 0;
        words[0xd1 + index] = 0;
        words[0x111 + index] = 0;
        words[0x181 + index] = 0;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigprocChannelPointers {
    pub current_records: u32,
    pub previous_records: u32,
    pub ring: [u32; SIGPROC_CHANNEL_RING_SLOTS],
}

/// Native per-channel pointer rotation: swap the current/previous record
/// scratch pointers (channel words `+0x8`/`+0xc`) and rotate the five-slot
/// pointer ring at `+0x14..+0x24` left by one (the old head becomes the
/// tail).
pub fn sigproc_rotate_channel_pointers_at5(pointers: &mut SigprocChannelPointers) {
    std::mem::swap(
        &mut pointers.current_records,
        &mut pointers.previous_records,
    );
    pointers.ring.rotate_left(1);
}

/// Native config flag-shift word: `flags = (flags * 2) & 0x7e`.
pub fn sigproc_shift_flag_word_at5(flags: u32) -> u32 {
    flags.wrapping_mul(2) & 0x7e
}

/// Native band-slot pointer plan (decompile line 42977): the shell builds a
/// nine-purpose pointer matrix over each channel's scratch where entry
/// `(slot, band)` points at
/// `channel_scratch + slot * 0x200 + band * 0x1200`. Slot 0 is the oldest
/// 128-float subband row, slot 8 receives the frame's fresh PQF output.
pub fn sigproc_band_slot_pointer_at5(
    channel_scratch: u32,
    slot: usize,
    band: usize,
) -> Result<u32, SigprocShellError> {
    if slot >= SIGPROC_BAND_SLOTS {
        return Err(SigprocShellError::SlotIndexOutOfRange { slot });
    }
    if band >= SIGPROC_BAND_COUNT {
        return Err(SigprocShellError::BandIndexOutOfRange { band });
    }
    Ok(channel_scratch
        .wrapping_add((slot as u32).wrapping_mul(SIGPROC_BAND_SLOT_BYTES))
        .wrapping_add((band as u32).wrapping_mul(SIGPROC_BAND_BLOCK_BYTES)))
}

/// Header/channel words written by the shell's band-limit epilogue
/// (native `0x50557..0x505ef`), captured just before the `time2freq_at5`
/// call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigprocBandLimitWriteback {
    /// `param_5` after the flag override; header words `0x2c` and `0x2d`.
    pub band_limit: i32,
    /// `g_a_x_at5[band_limit] + 1`; header word `0x2f`, every channel's
    /// `+0x1b48c` word, and the band-count argument of `time2freq_at5`.
    pub band_count: u32,
    /// `param_3`; header word `0x2a`.
    pub header_channel_count: u32,
    /// `g_a_sg_shape_index_at5[band_limit - 1] + 1`, or 0 when
    /// `band_limit < 1`; header word `0x2e`.
    pub header_shape_count: u32,
}

/// Native band-limit epilogue: `testb $0x7c` on the low byte of the
/// header flag word (`header + 0x1dc`) forces `band_limit = 0x20`, then
/// the band count `g_a_x_at5[band_limit] + 1` fans out to every channel
/// object and the header words `0x2a/0x2c/0x2d/0x2e/0x2f`.
pub fn sigproc_band_limit_writeback_at5(
    flag_word: u32,
    channel_count: u32,
    band_limit: i32,
) -> Result<SigprocBandLimitWriteback, SigprocShellError> {
    let mut band_limit = band_limit;
    if flag_word as u8 & 0x7c != 0 {
        band_limit = 0x20;
    }
    if !(0..=0x20).contains(&band_limit) {
        return Err(SigprocShellError::BandLimitOutOfRange { band_limit });
    }
    let band_count = u32::from(crate::tables::at5::x_at5()[band_limit as usize]) + 1;
    let header_shape_count = if band_limit < 1 {
        0
    } else {
        u32::from(crate::tables::at5::sg_shape_index_at5()[band_limit as usize - 1]) + 1
    };
    Ok(SigprocBandLimitWriteback {
        band_limit,
        band_count,
        header_channel_count: channel_count,
        header_shape_count,
    })
}

/// Native `at5enc_sigproc` mode-3 band_count (decompile `0x5f2b0`,
/// `decompiled/libatrac.c` lines 43239-43246): under `param_4 == 3` the shell
/// writes `detector_words[0] = sa_intensity_band_{sr}kHz[selector]`, indexed by
/// the block selector `iVar6 = cfg[0x7a]` (`cfg+0x1e8`, always `0..32`). 48 kHz
/// takes the `_48kHz` sibling (`cfg+0xac == 48000`), else the 44.1 kHz table.
/// The non-mode-3 branch instead writes the stereo default `0x10` (line 43497).
pub fn sigproc_intensity_band_count(selector: usize, sample_rate: u32) -> u32 {
    let table = if sample_rate == 48_000 {
        crate::tables::at5::intensity_band_48khz()
    } else {
        crate::tables::at5::intensity_band_44khz()
    };
    table[selector] as u32
}

/// Native stereo swap-flag update (decompile line 43686, native
/// `0x5128f..0x51343`), run only when `param_3 == 2` after
/// `check_channel_correlation_at5` refreshed detector rows `0x31`
/// (clamped difference dB), `0x71` (channel-a power), and `0xb1`
/// (channel-b power) over 0x100 samples of each band's slot-1 window.
///
/// Per band: the previous frame's decision (detector row `0xe1`, i.e.
/// last frame's row `0xf1` after the history rotation) relaxes the
/// power threshold from `8.0 *` to `4.0 *`; a strictly negative
/// difference dB forces the new flag to zero (zero and NaN still take
/// the comparison branch); otherwise the flag is
/// `a_power * threshold < b_power`. The new decision lands in detector
/// row `0xf1`, while the header swap words `0x14..` receive row `0xe1`
/// — downstream consumers see the previous frame's decision.
pub fn sigproc_stereo_swap_update_at5(
    difference_db: &[f32],
    a_power: &[f32],
    b_power: &[f32],
    previous_swap: &[u32],
    band_count: usize,
) -> Result<Vec<u32>, SigprocShellError> {
    let shortest = difference_db
        .len()
        .min(a_power.len())
        .min(b_power.len())
        .min(previous_swap.len());
    if shortest < band_count {
        return Err(SigprocShellError::BandIndexOutOfRange { band: shortest });
    }
    let mut new_swap = Vec::with_capacity(band_count);
    for band in 0..band_count {
        let threshold = if previous_swap[band] == 0 {
            8.0f32
        } else {
            4.0f32
        };
        let scaled = a_power[band] * threshold;
        let db = difference_db[band];
        // Native sign test: only a strictly negative, non-NaN dB takes
        // the zero branch.
        if db < 0.0 {
            new_swap.push(0);
        } else {
            new_swap.push(u32::from(scaled < b_power[band]));
        }
    }
    Ok(new_swap)
}

/// Native stereo per-band dB metric (decompile line 43197, the
/// `param_3 == 2` block right after the PQF loop): for every band the
/// shell runs `sub_seq_at5` + `check_power_level_tripl_at5` over 0x80
/// samples of each channel's slot-6 window and stores the clamped
/// difference dB — the identical ratio/log/negate/60-cap chain already
/// ported as `check_channel_correlation_at5` — into detector words
/// `0x181..0x191` (`detector + 0x604`), the row the history rotation
/// zeroes at entry and shifts into the `0x121..0x181` six-frame history
/// next frame.
pub fn sigproc_stereo_band_db_at5(
    a_slot6_bands: &[&[f32]],
    b_slot6_bands: &[&[f32]],
    band_count: usize,
) -> Result<Vec<f32>, crate::gha::power::PowerCheckError> {
    let correlation = crate::gha::power::check_channel_correlation_at5(
        a_slot6_bands,
        b_slot6_bands,
        SIGPROC_BAND_SLOT_FLOATS,
        band_count,
    )?;
    Ok(correlation.db)
}

/// Native per-band history roll (the `memmove(dst, dst + 0x200, 0x1000)`
/// plus `rep stos` zero at native `0x4f5c8..0x4f601`): shift slots `1..9`
/// of a band's nine-slot block down one row and zero the freed newest
/// slot before the PQF analysis writes into it.
pub fn sigproc_shift_band_slots_at5(block: &mut [f32]) -> Result<(), SigprocShellError> {
    let needed = SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS;
    if block.len() != needed {
        return Err(SigprocShellError::BandBlockWrongLength {
            needed,
            actual: block.len(),
        });
    }
    block.copy_within(SIGPROC_BAND_SLOT_FLOATS.., 0);
    let tail = (SIGPROC_BAND_SLOTS - 1) * SIGPROC_BAND_SLOT_FLOATS;
    for value in &mut block[tail..] {
        *value = 0.0;
    }
    Ok(())
}

/// Detector rows used by the composed shell pass (word indices into the
/// 0x200-word detector struct; float rows hold f32 bits).
pub const SIGPROC_DETECTOR_ROW_CORRELATION_DB: usize = 0x31;
pub const SIGPROC_DETECTOR_ROW_A_POWER: usize = 0x71;
pub const SIGPROC_DETECTOR_ROW_B_POWER: usize = 0xb1;
pub const SIGPROC_DETECTOR_ROW_PREVIOUS_SWAP: usize = 0xe1;
pub const SIGPROC_DETECTOR_ROW_NEW_SWAP: usize = 0xf1;
pub const SIGPROC_DETECTOR_ROW_STEREO_DB: usize = 0x181;

/// Mode-3 intensity-stereo pan rows (decompile 43272-43387, native cold
/// region `0x50630..0x51047`). Row `0x1d1` receives the newly computed pan
/// (native `piVar10 + 0x1d1`); the four older rows form the pan history that
/// the block's own roll shifts each frame (`0x191 <- 0x1a1 <- 0x1b1 <- 0x1c1
/// <- 0x1d1`). All hold f32 bits.
pub const SIGPROC_DETECTOR_ROW_PAN_H4: usize = 0x191;
pub const SIGPROC_DETECTOR_ROW_PAN_H3: usize = 0x1a1;
pub const SIGPROC_DETECTOR_ROW_PAN_H2: usize = 0x1b1;
pub const SIGPROC_DETECTOR_ROW_PAN_H1: usize = 0x1c1;
pub const SIGPROC_DETECTOR_ROW_PAN_NEW: usize = 0x1d1;

/// Mode-3 intensity-stereo power-reconstruction weight rows (decompile
/// 43388-43494). Native detector bytes `0x784`/`0x7c4` (prev weights, ch0/ch1)
/// and `0x804`/`0x844` (cur weights, ch0/ch1); each row is 16 f32. The block's
/// own roll copies cur -> prev then writes the fresh weights into cur.
pub const SIGPROC_DETECTOR_ROW_PREV_WEIGHT_CH0: usize = 0x1e1;
pub const SIGPROC_DETECTOR_ROW_PREV_WEIGHT_CH1: usize = 0x1f1;
pub const SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH0: usize = 0x201;
pub const SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH1: usize = 0x211;

/// The one-call-delayed a/b power history rows the zeroth pass reads as the
/// tone-activity floats (`zeroth_bit_allocation_at5`, native `0x42360`,
/// decompile 36575..36607: primary `*(*(obj+0x10)) + 0x184 + i*4`, byte
/// `0x184` == word `0x61`; secondary `+0x284` == word `0xa1`, read only when
/// `channel_count == 2`).
///
/// Fresh a/b power lands one row above (`SIGPROC_DETECTOR_ROW_A_POWER` `0x71` /
/// `SIGPROC_DETECTOR_ROW_B_POWER` `0xb1`) at the tail of each
/// `at5enc_sigproc` (`check_channel_correlation_at5`, native call site
/// `0x51285`); the head-of-frame history shift
/// (`sigproc_rotate_detector_history_at5`: `copy_within(0x71.., 0x61)` /
/// `copy_within(0xb1.., 0xa1)`) then rolls it down one row, so the zeroth of
/// core call `N` sees the a/b power computed at the tail of call `N-1`. After
/// `frontend_core_call_at5` for call `N` returns, rows `0x61`/`0xa1` hold
/// exactly that surface.
pub const SIGPROC_DETECTOR_ROW_A_POWER_HISTORY0: usize = 0x61;
pub const SIGPROC_DETECTOR_ROW_B_POWER_HISTORY0: usize = 0xa1;

/// Mutable state threaded through one `at5enc_sigproc` frame, mirroring
/// the native structures the shell touches: the shared detector struct,
/// each channel's record scratch pointers, band-slot blocks
/// (`band * 1152 + slot * 128 + index` floats per channel), PQF delay
/// lines, the header flag word, and the header swap words `0x14..`.
#[derive(Debug, Clone)]
pub struct SigprocFrameState {
    pub detector_words: Vec<u32>,
    pub channel_pointers: Vec<SigprocChannelPointers>,
    pub band_blocks: Vec<Vec<f32>>,
    pub pqf_delay: Vec<Vec<f32>>,
    pub header_flag_word: u32,
    pub header_swap_words: Vec<u32>,
}

/// Shell parameters for one frame: `param_3` (channel count), `param_4`
/// (mode), `param_5` (band limit), and the GHA gate
/// `param_2[6] != 0 && param_2[5] == 0`.
#[derive(Debug, Clone, Copy)]
pub struct SigprocFrameParams {
    pub channel_count: usize,
    pub mode: u32,
    pub band_limit: i32,
    /// The block selector `iVar6 = cfg[0x7a]` (`cfg+0x1e8`); mirrors
    /// [`crate::encoder::frontend::FrontendState::selector`]. Only consumed on
    /// the mode-3 `detector_words[0]` band_count path
    /// ([`sigproc_intensity_band_count`]); inert for the stereo default `0x10`
    /// (mode 2) branch.
    pub selector: i32,
    pub gha_gate_open: bool,
}

/// Values the shell hands to `time2freq_at5` and leaves in the header,
/// plus the flag telling the caller whether `extract_ghwave_at5` runs
/// this frame (native order: between the stereo dB metric and the
/// band-limit writeback; GHA does not mutate any surface the later
/// shell stages read).
#[derive(Debug, Clone)]
pub struct SigprocFrameReport {
    pub writeback: SigprocBandLimitWriteback,
    pub gha_should_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigprocFrameError {
    Shell(SigprocShellError),
    Pqf(crate::dsp::pqf::PqfError),
    Power,
    StateShape,
}

impl From<SigprocShellError> for SigprocFrameError {
    fn from(error: SigprocShellError) -> Self {
        SigprocFrameError::Shell(error)
    }
}

impl From<crate::dsp::pqf::PqfError> for SigprocFrameError {
    fn from(error: crate::dsp::pqf::PqfError) -> Self {
        SigprocFrameError::Pqf(error)
    }
}

fn detector_row_write_f32(words: &mut [u32], row: usize, values: &[f32]) {
    for (index, value) in values.iter().enumerate() {
        words[row + index] = value.to_bits();
    }
}

fn band_windows(blocks: &[f32], first_slot: usize, slot_count: usize) -> Vec<&[f32]> {
    (0..SIGPROC_BAND_COUNT)
        .map(|band| {
            let start = band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS
                + first_slot * SIGPROC_BAND_SLOT_FLOATS;
            &blocks[start..start + slot_count * SIGPROC_BAND_SLOT_FLOATS]
        })
        .collect()
}

/// Samples each band contributes to `time2freq_at5`.
pub const SIGPROC_TIME2FREQ_INPUT_FLOATS: usize = 2 * SIGPROC_BAND_SLOT_FLOATS;

/// The `time2freq_at5` input windows the shell hands over (native call
/// at `0x50623`): register arg `eax` is the shell's slot pointer matrix,
/// whose purpose-0 row per channel holds each band's slot-0 pointer, and
/// `time2freq_at5` reads 256 samples per band — the rolled slots 0..1.
///
/// The rest of the native call surface, pinned live by the
/// `time2freq_args` checkpoint in the shell trace: `edx`/`ecx` are the
/// delayed-in/delayed-out tables the shell's caller passed in its own
/// entry `ecx`/`edx` (pure pass-throughs the shell never touches), and
/// the stack carries the channel-object table, `param_3` (channel
/// count), `param_4` (mode), header word `0x7a`, the band count from
/// the band-limit writeback, and `param_2[4]`.
pub fn sigproc_time2freq_input_windows_at5(
    state: &SigprocFrameState,
    channel: usize,
) -> Result<Vec<&[f32]>, SigprocFrameError> {
    let blocks = state
        .band_blocks
        .get(channel)
        .ok_or(SigprocFrameError::StateShape)?;
    if blocks.len() != SIGPROC_BAND_COUNT * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS {
        return Err(SigprocFrameError::StateShape);
    }
    Ok(band_windows(blocks, 0, 2))
}

/// Byte offset (in floats) of a band's `slot` window within a channel's
/// 16-band nine-slot block (`band * 1152 + slot * 128`).
const fn band_slot_offset(band: usize, slot: usize) -> usize {
    band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS + slot * SIGPROC_BAND_SLOT_FLOATS
}

/// Native `power_reconst_at5` (leaf `0x1b970`, decompile 4463-4576): rescale
/// `count` (always `0x80`) samples of `src` into `dst` with a smooth
/// weight-transition envelope built from two sine-table ramps.
///
/// The three `weights` form two transition pairs — head `(w0, w1)` and tail
/// `(w1, w2)`. Each pair selects a transition length and sine table from the
/// max/min weight ratio (a degenerate `<= 0` or NaN weight forces ratio
/// `32.0`): `ratio > 16 -> (8, sa_sintbl0)`, `8 < ratio <= 16 ->
/// (0x10, sa_sintbl1)`, `4 < ratio <= 8 -> (0x20, sa_sintbl2)`,
/// `ratio <= 4 -> (0x40, sa_sintbl3)`. With `f5 = w1 - w0`, `f4 = w1 + w0` the
/// output is a head ramp, a constant-gain middle, and a tail ramp reading the
/// tail table from `[tail_len]` down to `[1]`.
pub fn power_reconst_at5(
    weights: [f32; 3],
    src: &[f32],
    dst: &mut [f32],
    count: usize,
) -> Result<(), SigprocShellError> {
    if src.len() < count || dst.len() < count {
        return Err(SigprocShellError::PowerReconstWindowTooShort {
            needed: count,
            actual: src.len().min(dst.len()),
        });
    }

    let sintbl0 = crate::tables::at5::sintbl0_at5();
    let sintbl1 = crate::tables::at5::sintbl1_at5();
    let sintbl2 = crate::tables::at5::sintbl2_at5();
    let sintbl3 = crate::tables::at5::sintbl3_at5();
    // `w_a`/`w_b` degenerate (<=0 or NaN) forces ratio 32.0; else max/min.
    let select = |w_a: f32, w_b: f32| -> (usize, &[f32]) {
        let ratio = if !(w_a > 0.0) || !(w_b > 0.0) {
            32.0f32
        } else if w_b <= w_a {
            w_a / w_b
        } else {
            w_b / w_a
        };
        if ratio > 16.0 {
            (8, &sintbl0[..])
        } else if ratio > 8.0 {
            (0x10, &sintbl1[..])
        } else if ratio > 4.0 {
            (0x20, &sintbl2[..])
        } else {
            (0x40, &sintbl3[..])
        }
    };

    let (w0, w1, w2) = (weights[0], weights[1], weights[2]);
    let (head_len, head_tbl) = select(w0, w1);
    let (tail_len, tail_tbl) = select(w1, w2);

    // head_len + tail_len <= count always (max 0x40 + 0x40 = 0x80).
    let f5 = w1 - w0;
    let f4 = w1 + w0;
    for i in 0..head_len {
        dst[i] = (f5 * head_tbl[i] + f4) * src[i];
    }
    let gain = f5 * head_tbl[head_len] + f4;
    let mid_end = count - tail_len;
    for i in head_len..mid_end {
        dst[i] = src[i] * gain;
    }
    let t5 = w1 - w2;
    let t4 = w1 + w2;
    for i in mid_end..count {
        // count - i runs tail_len down to 1 across the tail.
        let tv = tail_tbl[count - i];
        dst[i] = (t5 * tv + t4) * src[i];
    }
    Ok(())
}

/// Native `at5enc_sigproc` mode-3 intensity-stereo spectral block (decompile
/// 43247-43494, native cold region `0x50630..0x51047`). Runs under
/// `param_4 == 3`, immediately after the `detector_words[0]` band_count write
/// and before the GHA gate; native stereo-only (`param_3 == 2`).
///
/// Stages: (1) build the per-band intensity weights and the `start` band;
/// (2) for `start..16` compute each band's pan from the |ch0|/|ch1|/|ch0-ch1|
/// sums over the 256-float slot-7++slot-8 window and store it into pan row
/// `0x1d1` (f32 store at native `0x50f69`, clamped to `<= 0.125`); (3) rotate
/// each band's slot-7 spectrum with the sine-table crossfade over the three
/// newest pan frames; (4) roll the pan-history rows `0x191<-0x1a1<-0x1b1<-
/// 0x1c1<-0x1d1` (always); (5) for `band_count..16` reconstruct each channel's
/// slot-6 spectrum from the `sum67` (ch0+ch1) power ratios via
/// [`power_reconst_at5`] (skipped when stereo dB row `0x181[b] <= -11`, both
/// getting weight `0.25`); (6) roll the weight rows `prev<-cur`, `cur<-new`
/// (always). Slot-8 windows are untouched. Power-reconst call args pinned by
/// disassembly `0x50e69..0x50eb9` (EAX triple, EDX `sum67`, ECX channel slot-6,
/// stack count `0x80`). Landed in docs/13 §3.1.
pub fn sigproc_intensity_stereo_block_at5(
    detector_words: &mut [u32],
    band_blocks: &mut [Vec<f32>],
    channel_count: usize,
    band_count: usize,
) -> Result<(), SigprocFrameError> {
    if channel_count != 2 {
        return Err(SigprocShellError::IntensityChannelCount { channel_count }.into());
    }
    if band_count > 15 {
        return Err(SigprocShellError::IntensityBandCountOutOfRange { band_count }.into());
    }
    if detector_words.len() < SIGPROC_DETECTOR_ARENA_WORDS {
        return Err(SigprocShellError::DetectorArenaTooShort {
            needed: SIGPROC_DETECTOR_ARENA_WORDS,
            actual: detector_words.len(),
        }
        .into());
    }
    let block_floats = SIGPROC_BAND_COUNT * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS;
    if band_blocks.len() < 2
        || band_blocks[0].len() != block_floats
        || band_blocks[1].len() != block_floats
    {
        return Err(SigprocFrameError::StateShape);
    }

    // Stage 1: intensity weights (43247-43268). All 1.0, then a decaying
    // ramp from index band_count down to 0, flooring values < 0.01 to 0.
    let mut weights = [1.0f32; SIGPROC_BAND_COUNT];
    if band_count > 0 {
        let mut fv = 1.0f32;
        let mut i = band_count;
        loop {
            if fv < 0.01 {
                fv = 0.0;
            }
            weights[i] = fv;
            if i == 0 {
                break;
            }
            i -= 1;
            fv *= 0.5;
        }
    }
    // First index whose weight is >= 0.01 (weights[15] is always 1.0, so the
    // scan always terminates below 16 at every in-scope band_count).
    let mut start = 0usize;
    while weights[start] < 0.01 && start + 1 < SIGPROC_BAND_COUNT {
        start += 1;
    }

    if start < SIGPROC_BAND_COUNT {
        // Stage 2: pan per band over the 256-float slot-7++slot-8 window.
        for b in start..SIGPROC_BAND_COUNT {
            let base = band_slot_offset(b, 7);
            let win = 2 * SIGPROC_BAND_SLOT_FLOATS;
            let ch0 = &band_blocks[0][base..base + win];
            let ch1 = &band_blocks[1][base..base + win];
            let mut s0 = 0.0f32;
            let mut s1 = 0.0f32;
            let mut sd = 0.0f32;
            for t in 0..win {
                s0 += ch0[t].abs();
                s1 += ch1[t].abs();
                sd += (ch0[t] - ch1[t]).abs();
            }
            let r = if s0 == 0.0 && s1 == 0.0 {
                if sd == 0.0 { 1.0f32 } else { 0.0f32 }
            } else {
                sd / (s0 + s1)
            };
            let mut pan = if r <= 0.9999695f32 {
                if r >= 3.0517578e-05f32 {
                    let arg = (0.5f32 - r) * 10.0f32;
                    let at = f64::from(arg).atan();
                    (f64::from(at as f32) * 0.3640598f64 + 0.5f64) as f32
                } else {
                    1.0f32
                }
            } else {
                0.0f32
            };
            if pan > 0.125f32 {
                pan = 0.125f32;
            }
            detector_words[SIGPROC_DETECTOR_ROW_PAN_NEW + b] = pan.to_bits();
        }

        // Stage 3: per-band slot-7 spectrum rotation (43323-43361).
        let sintbl = crate::tables::at5::sintbl_at5();
        let (b0, b1) = band_blocks.split_at_mut(1);
        let ch0_block = &mut b0[0];
        let ch1_block = &mut b1[0];
        for b in start..SIGPROC_BAND_COUNT {
            let wh = weights[b] * 0.5;
            let fa = wh * f32::from_bits(detector_words[SIGPROC_DETECTOR_ROW_PAN_H2 + b]);
            let fb = wh * f32::from_bits(detector_words[SIGPROC_DETECTOR_ROW_PAN_H1 + b]);
            let fc = wh * f32::from_bits(detector_words[SIGPROC_DETECTOR_ROW_PAN_NEW + b]);
            let base = band_slot_offset(b, 7);
            for t in 0..0x40usize {
                let f = (fb - fa) * sintbl[t] + (fa + fb);
                let c0 = ch0_block[base + t];
                let c1 = ch1_block[base + t];
                ch0_block[base + t] = f * c1 + (1.0 - f) * c0;
                ch1_block[base + t] = (1.0 - f) * c1 + f * c0;
            }
            for t in 0x40usize..0x80 {
                let f = (fb - fc) * sintbl[0x40 - (t - 0x40)] + (fc + fb);
                let c0 = ch0_block[base + t];
                let c1 = ch1_block[base + t];
                ch0_block[base + t] = f * c1 + (1.0 - f) * c0;
                ch1_block[base + t] = (1.0 - f) * c1 + f * c0;
            }
        }
    }

    // Stage 4: pan-history roll (always), raw u32 copies.
    detector_words.copy_within(
        SIGPROC_DETECTOR_ROW_PAN_H3..SIGPROC_DETECTOR_ROW_PAN_H3 + SIGPROC_BAND_COUNT,
        SIGPROC_DETECTOR_ROW_PAN_H4,
    );
    detector_words.copy_within(
        SIGPROC_DETECTOR_ROW_PAN_H2..SIGPROC_DETECTOR_ROW_PAN_H2 + SIGPROC_BAND_COUNT,
        SIGPROC_DETECTOR_ROW_PAN_H3,
    );
    detector_words.copy_within(
        SIGPROC_DETECTOR_ROW_PAN_H1..SIGPROC_DETECTOR_ROW_PAN_H1 + SIGPROC_BAND_COUNT,
        SIGPROC_DETECTOR_ROW_PAN_H2,
    );
    detector_words.copy_within(
        SIGPROC_DETECTOR_ROW_PAN_NEW..SIGPROC_DETECTOR_ROW_PAN_NEW + SIGPROC_BAND_COUNT,
        SIGPROC_DETECTOR_ROW_PAN_H1,
    );

    // Stage 5: per-band power reconstruction (43388-43474) over band_count..16.
    // new_w defaults to 0.25 for every band (native's default-0.25 loop plus
    // the -11 dB and psum<=0 branches); band_count..16 may overwrite it.
    let mut new_w = [[0.25f32; SIGPROC_BAND_COUNT]; 2];
    let sum_len = 2 * SIGPROC_BAND_SLOT_FLOATS;
    for b in band_count..SIGPROC_BAND_COUNT {
        let db = f32::from_bits(detector_words[SIGPROC_DETECTOR_ROW_STEREO_DB + b]);
        if db <= -11.0 {
            // No reconstruction; both channels keep weight 0.25.
            continue;
        }
        let s7_off = band_slot_offset(b, 7);
        let s6_off = band_slot_offset(b, 6);

        // p_ch: power of each channel's post-rotation slot-7 window.
        let p = {
            let ch0_s7 = &band_blocks[0][s7_off..s7_off + SIGPROC_BAND_SLOT_FLOATS];
            let ch1_s7 = &band_blocks[1][s7_off..s7_off + SIGPROC_BAND_SLOT_FLOATS];
            crate::gha::power::check_power_level_dual_at5(
                ch0_s7,
                ch0_s7,
                ch1_s7,
                ch1_s7,
                SIGPROC_BAND_SLOT_FLOATS,
            )
            .map_err(|_| SigprocFrameError::Power)?
        };

        // sum67 = ch0 + ch1 over the 256 floats from the slot-6 base
        // (slots 6..7, post-rotation).
        let mut sum67 = vec![0.0f32; sum_len];
        {
            let ch0_s6 = &band_blocks[0][s6_off..s6_off + sum_len];
            let ch1_s6 = &band_blocks[1][s6_off..s6_off + sum_len];
            crate::dsp::scalar::add_seq_at5(ch0_s6, ch1_s6, &mut sum67, sum_len)
                .map_err(|_| SigprocFrameError::Power)?;
        }
        let psum = crate::gha::power::check_power_level_at5(
            &sum67[SIGPROC_BAND_SLOT_FLOATS..sum_len],
            &sum67[SIGPROC_BAND_SLOT_FLOATS..sum_len],
            SIGPROC_BAND_SLOT_FLOATS,
        )
        .map_err(|_| SigprocFrameError::Power)?
            * 0.25f64;

        for ch in 0..2usize {
            new_w[ch][b] = if !(psum > 0.0) {
                0.25f32
            } else {
                ((f64::from(p[ch]) / psum).sqrt() * 0.25f64) as f32
            };
        }

        // power_reconst per channel: triple = [prev, cur, new_w], src =
        // sum67[0..0x80] (slot-6 half), dst = that channel's slot-6 window.
        for ch in 0..2usize {
            let (prev_row, cur_row) = if ch == 0 {
                (
                    SIGPROC_DETECTOR_ROW_PREV_WEIGHT_CH0,
                    SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH0,
                )
            } else {
                (
                    SIGPROC_DETECTOR_ROW_PREV_WEIGHT_CH1,
                    SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH1,
                )
            };
            let triple = [
                f32::from_bits(detector_words[prev_row + b]),
                f32::from_bits(detector_words[cur_row + b]),
                new_w[ch][b],
            ];
            let dst = &mut band_blocks[ch][s6_off..s6_off + SIGPROC_BAND_SLOT_FLOATS];
            power_reconst_at5(
                triple,
                &sum67[..SIGPROC_BAND_SLOT_FLOATS],
                dst,
                SIGPROC_BAND_SLOT_FLOATS,
            )
            .map_err(SigprocFrameError::Shell)?;
        }
    }

    // Stage 6: weight-row roll (always): prev <- cur, then cur <- new_w.
    detector_words.copy_within(
        SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH0
            ..SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH0 + SIGPROC_BAND_COUNT,
        SIGPROC_DETECTOR_ROW_PREV_WEIGHT_CH0,
    );
    detector_words.copy_within(
        SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH1
            ..SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH1 + SIGPROC_BAND_COUNT,
        SIGPROC_DETECTOR_ROW_PREV_WEIGHT_CH1,
    );
    for b in 0..SIGPROC_BAND_COUNT {
        detector_words[SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH0 + b] = new_w[0][b].to_bits();
        detector_words[SIGPROC_DETECTOR_ROW_CUR_WEIGHT_CH1 + b] = new_w[1][b].to_bits();
    }

    Ok(())
}

/// One `at5enc_sigproc` frame over the ported shell stages, in native
/// order: detector-history rotation, channel pointer rotation, flag
/// shift, per-band slot roll, PQF analysis into slot 8, the stereo
/// band dB metric into detector row `0x181`, the `detector_word0`
/// band_count write (mode-3 → `sa_intensity_band_{sr}kHz[selector]`, else
/// the stereo default `0x10`), the GHA gate, the band-limit header
/// writeback, and the stereo correlation + swap-flag decision over the
/// slot-1 windows. `inputs` holds each channel's 2048 fresh samples.
///
/// Under `param_4 == 3` the band_count write is followed by the full native
/// intensity-stereo spectral block ([`sigproc_intensity_stereo_block_at5`],
/// decompile 43247-43494), landed in docs/13 §3.1; it needs a widened detector
/// arena ([`SIGPROC_DETECTOR_ARENA_WORDS`]).
pub fn sigproc_frame_at5(
    state: &mut SigprocFrameState,
    inputs: &[&[f32]],
    params: &SigprocFrameParams,
) -> Result<SigprocFrameReport, SigprocFrameError> {
    let channels = params.channel_count;
    let block_floats = SIGPROC_BAND_COUNT * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS;
    if state.channel_pointers.len() < channels
        || state.band_blocks.len() < channels
        || state.pqf_delay.len() < channels
        || inputs.len() < channels
        || state.header_swap_words.len() < SIGPROC_BAND_COUNT
        || state
            .band_blocks
            .iter()
            .take(channels)
            .any(|blocks| blocks.len() != block_floats)
    {
        return Err(SigprocFrameError::StateShape);
    }

    // Prologue: history rotation, pointer rotation, flag shift.
    sigproc_rotate_detector_history_at5(&mut state.detector_words)?;
    for pointers in state.channel_pointers.iter_mut().take(channels) {
        sigproc_rotate_channel_pointers_at5(pointers);
    }
    state.header_flag_word = sigproc_shift_flag_word_at5(state.header_flag_word);

    // Per channel: roll every band's nine-slot block, then run the PQF
    // analysis and scatter the fresh 128 samples into slot 8.
    for channel in 0..channels {
        let blocks = &mut state.band_blocks[channel];
        for band in 0..SIGPROC_BAND_COUNT {
            let start = band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS;
            sigproc_shift_band_slots_at5(
                &mut blocks[start..start + SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS],
            )?;
        }
        let result = crate::dsp::pqf::pqf_analysis_at5(&state.pqf_delay[channel], inputs[channel])?;
        for (band, samples) in result.subbands.iter().enumerate() {
            let start = band * SIGPROC_BAND_SLOTS * SIGPROC_BAND_SLOT_FLOATS
                + (SIGPROC_BAND_SLOTS - 1) * SIGPROC_BAND_SLOT_FLOATS;
            blocks[start..start + SIGPROC_BAND_SLOT_FLOATS].copy_from_slice(samples);
        }
        state.pqf_delay[channel] = result.delay;
    }

    // Stereo band dB metric over the rolled slot-6 windows.
    if channels == 2 {
        let a_windows = band_windows(&state.band_blocks[0], 6, 1);
        let b_windows = band_windows(&state.band_blocks[1], 6, 1);
        let db = sigproc_stereo_band_db_at5(&a_windows, &b_windows, SIGPROC_BAND_COUNT)
            .map_err(|_| SigprocFrameError::Power)?;
        detector_row_write_f32(
            &mut state.detector_words,
            SIGPROC_DETECTOR_ROW_STEREO_DB,
            &db,
        );
    }

    // band_count at objside +0x10 word 0 (at5enc_sigproc 43239-43246 /
    // 43496-43498): mode-3 → sa_intensity_band_{sr}kHz[selector]; else the
    // stereo default 0x10. Mode-3 then runs the full intensity-stereo spectral
    // block (decompile 43247-43494) — the pan coefficient banks 0x191..0x1e1,
    // the per-band slot-7 rotation, and the slot-6 power reconstruction — which
    // landed here in docs/13 §3.1 (previously registered out-of-scope).
    if params.mode == 3 {
        // 44.1 kHz is the only in-scope sample rate (48 kHz is rejected
        // upstream); the 48 kHz branch is a faithful decompile port pinned by a
        // unit test but never reached on the shipping path.
        let band_count = sigproc_intensity_band_count(params.selector as usize, 44_100);
        state.detector_words[0] = band_count;
        sigproc_intensity_stereo_block_at5(
            &mut state.detector_words,
            &mut state.band_blocks,
            channels,
            band_count as usize,
        )?;
    } else {
        state.detector_words[0] = 0x10;
    }

    let gha_should_run = params.gha_gate_open;

    // Band-limit epilogue.
    let writeback = sigproc_band_limit_writeback_at5(
        state.header_flag_word,
        channels as u32,
        params.band_limit,
    )?;

    // Stereo correlation over the slot-1 windows (0x100 samples) plus
    // the swap-flag decision; header swap words take the previous
    // frame's decision.
    if channels == 2 {
        let band_count = writeback.band_count as usize;
        let a_windows = band_windows(&state.band_blocks[0], 1, 2);
        let b_windows = band_windows(&state.band_blocks[1], 1, 2);
        let correlation = crate::gha::power::check_channel_correlation_at5(
            &a_windows,
            &b_windows,
            2 * SIGPROC_BAND_SLOT_FLOATS,
            band_count,
        )
        .map_err(|_| SigprocFrameError::Power)?;
        detector_row_write_f32(
            &mut state.detector_words,
            SIGPROC_DETECTOR_ROW_CORRELATION_DB,
            &correlation.db,
        );
        detector_row_write_f32(
            &mut state.detector_words,
            SIGPROC_DETECTOR_ROW_A_POWER,
            &correlation.a_power,
        );
        detector_row_write_f32(
            &mut state.detector_words,
            SIGPROC_DETECTOR_ROW_B_POWER,
            &correlation.b_power,
        );
        let previous_swap: Vec<u32> = state.detector_words
            [SIGPROC_DETECTOR_ROW_PREVIOUS_SWAP..SIGPROC_DETECTOR_ROW_PREVIOUS_SWAP + band_count]
            .to_vec();
        let new_swap = sigproc_stereo_swap_update_at5(
            &correlation.db,
            &correlation.a_power,
            &correlation.b_power,
            &previous_swap,
            band_count,
        )?;
        state.detector_words
            [SIGPROC_DETECTOR_ROW_NEW_SWAP..SIGPROC_DETECTOR_ROW_NEW_SWAP + band_count]
            .copy_from_slice(&new_swap);
        state.header_swap_words[..band_count].copy_from_slice(&previous_swap);
    }

    Ok(SigprocFrameReport {
        writeback,
        gha_should_run,
    })
}
