//! Per-rate coding parameters (docs/13 §1.1): the three per-rate words the
//! computed encode pipeline threads in place of the pinned 352 constants — the
//! ATRAC3plus block **selector** (`cfg+0x1e8`), the frame **bit budget**
//! (`cfg+0x1e0`), and the **frame byte** count (`gAtracCodecParam` block_align).
//!
//! docs/13 §3.1 additionally makes the **band extent** per-rate: `band_index`
//! (`handle+0x1a4`, the scale-factor / quant-unit count, `cfg+0xb4`) and its
//! derived `isps` spectral-line count (`handle+0x1b8`) and `band_count`
//! (`g_a_x_at5[band_index] + 1`, the QMF/gain-group count, `cfg+0xbc`). These are
//! **32 / 2048 / 16 (full-band)** at 256/320/352, but reduce at lower rates —
//! **29 / 1664 / 13** at 192 and **28 / 1536 / 12** at 160. The old note that
//! "band_index/isps [is] rate-independent" is now stale.
//!
//! Everything else in the encode config is rate-independent for plain 44.1 kHz
//! stereo (docs/13 §1.1, cross-checked against the committed
//! `zeroth_budget_by_rate.ndjson`): `block_count = 1`, `channels = 2`,
//! `sample_rate = 44100`, `+0xa8/+0xac/…` config words, and the frame schedule
//! (priming 7 calls, flush 8/9). Only the four per-rate words (selector, budget,
//! frame_bytes, band_index) vary at a rate whose coding path is ported.
//!
//! docs/14 §0.2 makes [`CodingParams::for_profile`] **channel-mode-aware** — the
//! same words for the five 44.1 kHz MONO rows (32/48/64/96/128 kbps). Measured
//! calls per run — same frame schedule as stereo, channel-independent): mono
//! `block_count = 1`, `channels = 1`, `sample_rate = 44100`, `mode_a = 1` at all
//! five rates (`+0x190`; the joint/intensity machinery is structurally dead at
//! mono), and the `+0xcc`/`+0xd0` gate word (`handle+0x44`) reads 0 at every
//! mono rate as at stereo, so the plain-threshold path governs. Only the
//! selector-threshold **laws** differ by channel count (see [`CodingParams`]
//! field docs). The stereo path is bit-for-bit unchanged. All five mono rates —
//! 128 kbps (docs/14 §1.3), 96 kbps (docs/14 §2.1), 64 kbps (docs/14 §3.1),
//! 48 kbps (docs/14 §4.1), and 32 kbps (docs/14 §5.1) — are landed and drive
//! these params end-to-end.
//!
//! # Native sources of truth
//!
//! * **Selector ladder** — `atx_init_encode_block` (native `0x5a8a0`; Ghidra
//!   decompile comment `0x6a8a0` = native + 0x10000; decompiled/libatrac.c lines
//!   48395-48455). It computes `bps = sample_rate * ((frame_bits + 7) >> 3) >> 8`
//!   then a descending threshold ladder → the block selector. The stereo ±1/±2
//!   adjustments (decompile 48430-48454) fire only for chan_code 5/7
//!   multichannel / joint containers, NEVER for plain stereo — not ported. Pinned
//!   at all nine rates by `init_words_by_rate.ndjson` (`selector_0x1e8_u32`) and
//!   `zeroth_budget_by_rate.ndjson` (`cfg_1e8_selector_u32`): 13/15/19/23/24/25/
//!   27/29/30 for 48..352.
//!
//! * **Frame bit budget** — `atx_encode_core` (native `0x559f0`; decompile line
//!   46039): `budget = frame_bytes*8 - 2*block_count - 3`, stored to `cfg+0x1e0`
//!   by the sole writer `mov %ecx,0x1e0(%edx)` in `zeroth_bit_allocation_at5`
//!   (native `0x423c0`). Pinned at all nine rates by
//!   `zeroth_budget_by_rate.ndjson` (`budget_ecx_u32`): 2235/3003/4475/5947/7483/
//!   8955/11899/14907/16379 for 48..352 (block_count = 1 for plain stereo, from
//!   the init-words `block_count_u32 = 1`).

use crate::encoder::profile::Atrac3plusProfile;
use crate::tables::at5::{isps_at5, x_at5};

/// Block group count (`atx_state + 0xc`; init-words `block_count_u32 = 1` at
/// every rate). Named for the stereo program where it was pinned, but MONO
/// measures the same value 1 (`block_count_u32 = 1` at all five rates,
/// the mono budget uses it too. The name is kept to avoid a repo-wide rename.
pub const STEREO_BLOCK_COUNT: u32 = 1;

/// Full-band `band_index` (`handle+0x1a4`, `cfg+0xb4`): 32 scale-factor / quant
/// units. The value at 256/320/352; reduced rates take
/// [`atx_band_index_for_bandwidth`].
pub const FULL_BAND_INDEX: u32 = 0x20;

/// The native ATRAC3plus `band_index` (`handle+0x1a4`) for a stereo row's
/// `bandwidth_hz`. Ports the `atx_init_encode_block` scan verbatim (native
/// `0x5a8a0`; Ghidra `0x6a8a0` = native + 0x10000; decompiled/libatrac.c lines
/// 48375-48392):
///
/// ```text
/// target = (bandwidth_hz << 12) / sample_rate;        // integer division
/// i = 0;
/// do { if (target <= g_a_isps_at5[i + 1]) break; i++; } while (i < 0x20);
/// band_index = min(i + 1, 0x20);
/// ```
///
/// `DAT_000cdc62` in the decompile is `g_a_isps_at5 + 2` bytes, i.e. the u16
/// table indexed from `[1..]` — hence the `isps[i + 1]` scan. `band_index` then
/// indexes `g_a_isps_at5` for the spectral-line count (`handle+0x1b8`) and
/// `g_a_x_at5` for the QMF/gain-group `band_count` (`+1`).
///
/// (`band_index_0x1a4_u32`) for all nine rates: 48→26, 64/96/128→27, 160→28,
/// 192→29, 256/320/352→32.
pub fn atx_band_index_for_bandwidth(sample_rate: u32, bandwidth_hz: u32) -> u32 {
    let target = ((bandwidth_hz as u64) << 12) / sample_rate as u64;
    let isps = isps_at5();
    let mut i: usize = 0;
    loop {
        if target <= u64::from(isps[i + 1]) {
            break;
        }
        i += 1;
        if i >= FULL_BAND_INDEX as usize {
            break;
        }
    }
    (i as u32 + 1).min(FULL_BAND_INDEX)
}

/// The native ATRAC3plus block selector for `sample_rate` and `frame_bits`
/// (= `frame_bytes * 8`). Ports the `atx_init_encode_block` ladder verbatim
/// (native `0x5a8a0`, decompile 48395-48455). Plain-stereo only: the
/// multichannel/joint ±1/±2 adjustments are intentionally not applied.
pub fn atx_encode_selector(sample_rate: u32, frame_bits: u32) -> u32 {
    let frame_bytes = (frame_bits + 7) >> 3;
    let bps = ((sample_rate as u64 * frame_bytes as u64) >> 8) as u32;
    if bps >= 380_000 {
        31
    } else if bps >= 350_000 {
        30
    } else if bps >= 300_000 {
        29
    } else if bps >= 250_000 {
        27
    } else if bps >= 180_000 {
        25
    } else if bps >= 150_000 {
        24
    } else if bps >= 120_000 {
        23
    } else if bps >= 110_000 {
        21
    } else if bps >= 100_000 {
        20
    } else if bps >= 90_000 {
        19
    } else if bps >= 86_000 {
        18
    } else if bps >= 76_000 {
        17
    } else if bps >= 70_000 {
        16
    } else if bps >= 60_000 {
        15
    } else if bps >= 55_000 {
        14
    } else if bps >= 45_000 {
        13
    } else if bps >= 35_000 {
        12
    } else if bps >= 30_000 {
        11
    } else if bps >= 26_000 {
        10
    } else if bps >= 22_000 {
        9
    } else if bps >= 19_000 {
        8
    } else if bps >= 15_000 {
        7
    } else if bps >= 13_500 {
        6
    } else if bps >= 11_500 {
        5
    } else if bps >= 9_500 {
        4
    } else if bps >= 7_500 {
        3
    } else if bps >= 5_500 {
        2
    } else if bps > 3_499 {
        1
    } else {
        0
    }
}

/// Frame bit budget for a single-frame stereo block: `frame_bytes*8 -
/// 2*block_count - 3` (`atx_encode_core`, native `0x559f0`, decompile 46039).
pub fn frame_bit_budget(frame_bytes: u32, block_count: u32) -> i32 {
    frame_bytes as i32 * 8 - 2 * block_count as i32 - 3
}

/// The three per-rate words the computed pipeline threads in place of the pinned
/// 352 constants. Derived purely from an [`Atrac3plusProfile`]; carries no runtime
/// trace input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodingParams {
    /// ATRAC3plus block selector (`cfg+0x1e8`; `InitFrameState`/`CalcFrameEntry`
    /// `selector`; the frontend extract param_3 + time2freq bandwidth). Stored as
    /// `i32` to match the frontend/init sites; cast `as u32` for the cfg/zeroth/
    /// calc sites.
    pub selector: i32,
    /// Frame bit budget (`cfg+0x1e0`; zeroth `frame_bit_budget`; calc `budget`).
    pub budget: i32,
    /// Frame byte count (`gAtracCodecParam` block_align; `FramePrepackerState`
    /// `frame_bytes`; the packer tail stuffing target and the RIFF `data` /
    /// `block_align` sizing).
    pub frame_bytes: u32,
    /// The `gAtracCodecParam` row channel count (`handle+0x94`; the frontend
    /// channel-object count, the prepacker `nblk`): **2** for the nine stereo
    /// rows, **1** for the five mono rows. MEASURED `channels_u32` / cmode in
    /// `init_words_by_rate.ndjson` (2) and `init_words_mono_by_rate.ndjson` (1),
    /// docs/14 §0.2/§0.4. Seeds the computed driver's frontend/gain-roll channel
    /// count and the from-scratch prepacker object count in place of the hardcoded
    /// stereo [`FRONTEND_CHANNEL_COUNT`](crate::encoder::frontend::FRONTEND_CHANNEL_COUNT)
    /// / `COMPUTED_NBLK` anchors. (Mono has no ported coding path yet — docs/14
    /// phases 1-5 — so a 1-channel driver is never stepped in any shipping path.)
    pub channels: u32,
    /// The `g_a_encode_setting_atx` row+0x14 `mode_a` config word — the zeroth
    /// `param_5` (`*(handle+0x190)`) joint/intensity-stereo producer gate
    /// (docs/13 §2.3 (r), now stored by the validated profile). `3` for
    /// 48-256 kbps (the
    /// zeroth `param_5 == 3` producer arm is LIVE), `2` for 320/352 kbps (DEAD;
    /// which is why those rates are byte-identical whether or not the producer
    /// is wired). Threaded into the frontend sigproc mode and the coding-bridge
    /// frame function.
    pub mode_a: u32,
    /// The native `band_index` (`handle+0x1a4`, `cfg+0xb4`): the scale-factor /
    /// quant-unit count. **32** (full-band) at 256/320/352, **29** at 192, and
    /// **28** at 160 (docs/13 §3). Derived by [`atx_band_index_for_bandwidth`]
    /// from the stereo row's `bandwidth_hz`. Threaded into the frontend band
    /// limit, the zeroth band count, and the per-frame `cfg+0xb4`/`+0xbc` words.
    pub band_index: u32,
    /// The GHA-enable config word (`cfg+0xd0`; docs/13 §5.1, docs/14 §0.2). Set
    /// by `atx_init_encode_block` (decompile 48468-48485): the multichannel gate
    /// word `handle+0x44` reads 0 at every stereo AND mono rate (§0.2 oracles),
    /// so the plain-threshold branch governs and the store reduces to a single
    /// `selector >` compare whose threshold is channel-mode-dependent — **1-ch
    /// `selector > 0xe`**, **2-ch `selector > 0x12`** (both channel branches of
    /// the +0xd0 store). Threaded into the frontend extract `header_0xd0_enabled`
    /// read (the sine/general vs disabled-fallback dispatch). Equals the pinned
    /// `gha_enable_0xd0_u32` column: stereo 0 at 48/64, 1 at 96-352
    /// (`init_words_by_rate.ndjson`); mono 0 at 32/48 (sel 11/13), 1 at 64/96/128
    /// (sel 15/19/23) (`init_words_mono_by_rate.ndjson`).
    pub gha_enabled: bool,
    /// The low-rate gain-detector mode word (`cfg+0xcc`; docs/13 §5.2, docs/14
    /// §0.2): `false` selects the `mode_cc == 0` descending `set_gainc_at5`
    /// dispatch, `true` the `detect_gainc_data_new_at5` chain. Set by
    /// `atx_init_encode_block` (decompile 48451-48466): with the gate word
    /// `handle+0x44` == 0 (never 2, at both stereo and mono), the store reduces
    /// to `cfg+0xcc = (selector >= threshold) ? 1 : 0` with a channel-mode
    /// threshold — **1-ch `selector >= 0xf`**, **2-ch `selector >= 0x13`**.
    /// Within each channel mode this is numerically identical to [`gha_enabled`]
    /// (`sel > 0xe` ⟺ `sel >= 0xf`; `sel > 0x12` ⟺ `sel >= 0x13`), but the two
    /// stores are written with the distinct decompiled comparators. Threaded into
    /// the frontend `time2freq_at5` `mode_cc` argument. Equals the pinned
    /// `ms_seed_0xcc_u32` column: stereo 0 at 48/64, 1 at 96-352; mono 0 at
    /// 32/48, 1 at 64/96/128 (`init_words_mono_by_rate.ndjson`).
    pub mode_cc: bool,
}

impl CodingParams {
    /// Derive the per-rate coding params from an [`Atrac3plusProfile`], channel-mode
    /// aware (block_count = 1 for both stereo and mono, MEASURED). `channels == 1`
    /// resolves the five mono rows; anything else the nine stereo rows.
    pub fn for_profile(profile: &Atrac3plusProfile) -> Self {
        let frame_bits = profile.frame_bytes() * 8;
        let is_mono = profile.channels() == 1;
        // The validated profile owns the matching `g_a_encode_setting_atx`
        // facts, so this conversion is infallible and has no neutral fallback.
        let mode_a = profile.mode_a();
        let band_index =
            atx_band_index_for_bandwidth(profile.sample_rate(), profile.bandwidth_hz());
        let selector = atx_encode_selector(profile.sample_rate(), frame_bits) as i32;
        // `atx_init_encode_block` +0xd0 / +0xcc store thresholds are
        // channel-mode-dependent (gate word `handle+0x44` == 0 measured at every
        // stereo AND mono rate, so the plain-threshold branch governs):
        //   +0xd0 (gha_enabled): 1-ch `sel > 0xe`   / 2-ch `sel > 0x12`
        //   +0xcc (mode_cc):     1-ch `sel >= 0xf`  / 2-ch `sel >= 0x13`
        // (decompile 48468-48485 / 48451-48466). The mono budget word is NOT
        // pinned here — that is the Phase-1 zeroth-oracle's job (docs/14); only
        // block_count = 1 is measured, giving `frame_bytes*8 - 5`.
        let (gha_gt, cc_ge) = if is_mono { (0x0e, 0x0f) } else { (0x12, 0x13) };
        CodingParams {
            selector,
            budget: frame_bit_budget(profile.frame_bytes(), STEREO_BLOCK_COUNT),
            frame_bytes: profile.frame_bytes(),
            // `handle+0x94` channel count: 2 stereo / 1 mono (MEASURED, docs/14
            // §0.2/§0.4). All five mono values — 128 kbps (docs/14 §1.3), 96 kbps
            // (docs/14 §2.1), 64 kbps (docs/14 §3.1), 48 kbps (docs/14 §4.1), and
            // 32 kbps (docs/14 §5.1) — drive a live 1-channel pipeline.
            channels: u32::from(profile.channels()),
            mode_a,
            band_index,
            gha_enabled: selector > gha_gt,
            mode_cc: selector >= cc_ge,
        }
    }

    /// The QMF / gain-group band count (`g_a_x_at5[band_index] + 1`,
    /// `handle+0x1b48c` seed, `cfg+0xbc`): 16 full-band, 13 at 192, 12 at 160.
    pub fn band_count(&self) -> u32 {
        u32::from(x_at5()[self.band_index as usize]) + 1
    }

    /// The spectral-line count (`g_a_isps_at5[band_index]`, `handle+0x1b8`): 2048
    /// full-band, 1664 at 192, 1536 at 160.
    pub fn isps(&self) -> u32 {
        u32::from(isps_at5()[self.band_index as usize])
    }
}
