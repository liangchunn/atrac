//! `time2freq_at5` composition pieces.
//!
//! The tonality pre-pass follows the decompiled `time2freq_at5` block at
//! native `0x4c480` (decompile from `decompiled/libatrac.c` line 32774): per
//! channel and band it runs a strided magnitude DFT over band floats
//! `128..`, derives a tonality ratio and band-0 scale word, flags tonal
//! bands against the `sa_tlev_thred_064/096` thresholds selected by the
//! channel bandwidth word, and relaxes the thresholds when a wide-bandwidth
//! channel flags more than three bands.

use crate::dsp::fft::{FftError, dft_v_at5};
use crate::dsp::gain::{GAIN_WINDOW_VALUES, GainPassError, gainc_window_enc_at5};
use crate::dsp::gain::{GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS, GC_SET_POINTS_OUTPUT_GROUPS};
use crate::dsp::mdct::{MDCT_128_OUTPUT_COUNT, MdctError, winormal_mdct_128_ex_at5};
use crate::dsp::set_gainc::{
    SET_GAINC_HISTORY_A_FLOATS, SET_GAINC_HISTORY_B_FLOATS, SET_GAINC_SCRATCH_FLOATS,
    SetGaincError, SetGaincPlane, SetGaincRow, set_gainc_at5,
};
use crate::dsp::sigproc::{
    GAIN_DETECT_BAND_WINDOW_PEAK_OFFSET, GAIN_DETECT_BAND_WINDOW_VALUES,
    GAIN_DETECT_HISTORY_PEAK_VALUES, GAIN_DETECT_PEAK_BINS, GainDetectBandOutcome,
    GainDetectBandStateWritebackFields, GainDetectCandidateListRecord,
    GainDetectCandidateLoopError, GainDetectLeanOutcome, GainDetectScratch, gain_detect_band_at5,
    gain_detect_band_state_writeback_at5, gain_detect_band_with_scratch_at5,
    gain_detect_peak_bins_at5, gain_detect_primary_history_shift_at5,
    gain_detect_prune_markers_at5, gain_detect_secondary_history_shift_at5,
};
use crate::tables::at5::{
    TLEV_THRED_AT5_ENTRIES, sc064_at5_ref, sc128_at5_ref, tlev_thred_064_at5, tlev_thred_096_at5,
};
use crate::tables::at5::{ip064_at5_ref, ip128_at5_ref};
use crate::tables::at5::{rev_at5, wind0_at5, wind1_at5, wind2_at5, wind3_at5};

pub const TIME2FREQ_BANDS_AT5: usize = 16;
pub const TIME2FREQ_TONALITY_INPUT_OFFSET: usize = 128;
const TONALITY_WIDE_BAND_LIMIT: i32 = 0x1a;
const TONALITY_TABLE_SPLIT_BANDWIDTH: i32 = 0x12;
const TONALITY_ACTIVE_LIMIT_DEFAULT: usize = 0xb;
const TONALITY_BAND0_SCALES: [f32; 4] = [8.0, 4.0, 2.0, 1.5];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Time2FreqError {
    BandInputsTooShort {
        needed: usize,
        actual: usize,
    },
    BandDataTooShort {
        band: usize,
        needed: usize,
        actual: usize,
    },
    Fft(FftError),
    Gain(GainPassError),
    Mdct(MdctError),
    Detector(GainDetectCandidateLoopError),
    /// The `mode_cc == 0` `set_gainc_at5` leaf rejected an entry surface.
    SetGainc(SetGaincError),
    /// The `mode_cc == 0` descending `set_gainc_at5` detector dispatch is out
    /// of scope for the 352 target (native gate at decompile `33012`; the 352
    /// path always takes `mode_cc = 1` -> `detect_gainc_data_new_at5`).
    UnportedSetGaincDispatch,
    /// A driver stage was given fewer channels/bands than the parameters
    /// declared.
    ChannelStateTooShort {
        needed: usize,
        actual: usize,
    },
}

impl From<GainDetectCandidateLoopError> for Time2FreqError {
    fn from(error: GainDetectCandidateLoopError) -> Self {
        Self::Detector(error)
    }
}

impl From<SetGaincError> for Time2FreqError {
    fn from(error: SetGaincError) -> Self {
        Self::SetGainc(error)
    }
}

impl From<crate::dsp::sigproc::SigprocError> for Time2FreqError {
    fn from(error: crate::dsp::sigproc::SigprocError) -> Self {
        Self::Detector(GainDetectCandidateLoopError::from(error))
    }
}

impl From<MdctError> for Time2FreqError {
    fn from(error: MdctError) -> Self {
        Self::Mdct(error)
    }
}

impl From<FftError> for Time2FreqError {
    fn from(error: FftError) -> Self {
        Self::Fft(error)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TonalityChannel {
    pub flags: [bool; TIME2FREQ_BANDS_AT5],
    pub tonality: [f32; TIME2FREQ_BANDS_AT5],
    pub scales: [f32; TIME2FREQ_BANDS_AT5],
}

/// Native tonality pre-pass for one channel. `band_inputs` are the 16 band
/// buffers; the DFT reads floats `128..256` per band (stride 1 over 128
/// values for bands 0..2, stride 2 over 64 values for bands 2..).
pub fn time2freq_tonality_channel_at5(
    band_inputs: &[&[f32]],
    bandwidth: i32,
    mode_cc_nonzero: bool,
    prepass_disabled: bool,
) -> Result<TonalityChannel, Time2FreqError> {
    if band_inputs.len() < TIME2FREQ_BANDS_AT5 {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: TIME2FREQ_BANDS_AT5,
            actual: band_inputs.len(),
        });
    }

    let mut result = TonalityChannel {
        flags: [false; TIME2FREQ_BANDS_AT5],
        tonality: [1.0; TIME2FREQ_BANDS_AT5],
        scales: [1.0; TIME2FREQ_BANDS_AT5],
    };

    if prepass_disabled {
        result.scales = [8.0; TIME2FREQ_BANDS_AT5];
        return Ok(result);
    }

    let thresholds = if bandwidth > TONALITY_TABLE_SPLIT_BANDWIDTH {
        tlev_thred_096_at5()
    } else {
        tlev_thred_064_at5()
    };
    debug_assert_eq!(thresholds.len(), TLEV_THRED_AT5_ENTRIES);
    let active_limit = if mode_cc_nonzero {
        1
    } else {
        TONALITY_ACTIVE_LIMIT_DEFAULT
    };

    let ip128 = ip128_at5_ref();
    let sc128 = sc128_at5_ref();
    let ip064 = ip064_at5_ref();
    let sc064 = sc064_at5_ref();

    for band in 0..TIME2FREQ_BANDS_AT5 {
        if band < active_limit || bandwidth > TONALITY_WIDE_BAND_LIMIT {
            let (bins, stride, high_divisor, low_divisor) = if band < 2 {
                (0x40usize, 1usize, 56.0f32, 8.0f32)
            } else {
                (0x20usize, 2usize, 28.0f32, 4.0f32)
            };
            let count = bins * 2;
            let needed = TIME2FREQ_TONALITY_INPUT_OFFSET + (count - 1) * stride + 1;
            let data = band_inputs[band];
            if data.len() < needed {
                return Err(Time2FreqError::BandDataTooShort {
                    band,
                    needed,
                    actual: data.len(),
                });
            }

            let mut magnitudes = [0.0f32; 129];
            if band < 2 {
                dft_v_at5(
                    &data[TIME2FREQ_TONALITY_INPUT_OFFSET..],
                    stride,
                    count,
                    &mut magnitudes,
                    ip128,
                    sc128,
                )?;
            } else {
                dft_v_at5(
                    &data[TIME2FREQ_TONALITY_INPUT_OFFSET..],
                    stride,
                    count,
                    &mut magnitudes,
                    ip064,
                    sc064,
                )?;
            }

            let mut max_value = 0.0f32;
            let mut max_index = 0usize;
            for (bin, value) in magnitudes[..bins].iter().enumerate() {
                if max_value < *value {
                    max_value = *value;
                    max_index = bin;
                }
            }

            let split = bins >> 3;
            let mut low_sum = 0.0f32;
            for value in &magnitudes[..split] {
                low_sum += *value;
            }
            let mut high_sum = 0.0f32;
            for value in &magnitudes[split..bins] {
                high_sum += *value;
            }

            result.tonality[band] = if max_value <= 0.0 {
                1.0
            } else {
                (bins as f32 * max_value) / (low_sum + high_sum)
            };

            if band == 0
                && (high_sum / high_divisor) * 16.0 < low_sum / low_divisor
                && max_index < TONALITY_BAND0_SCALES.len()
            {
                result.scales[0] = TONALITY_BAND0_SCALES[max_index];
            }

            if result.tonality[band] < thresholds[band] {
                result.flags[band] = true;
            }
        }
        if mode_cc_nonzero {
            result.flags[band] = false;
        }
    }

    if bandwidth > TONALITY_WIDE_BAND_LIMIT {
        let tonal_count = result.flags.iter().filter(|&&flag| flag).count();
        if tonal_count > 3 {
            let delta = if tonal_count < 8 {
                1.0f32
            } else if tonal_count < 0xc {
                2.0
            } else {
                3.0
            };
            for band in 0..TIME2FREQ_BANDS_AT5 {
                if result.tonality[band] < thresholds[band] + delta {
                    result.flags[band] = true;
                }
            }
        }
    }

    Ok(result)
}

/// Native stereo tonal-flag reconciliation (`time2freq_at5` `param_6 == 3`
/// block, decompile from line 32952). Part 1: where the two channels'
/// tonal flags differ, copy the higher-tonality channel's flag to the other
/// when the tonality gap is under `1.0` or the band correlation exceeds
/// `20.0` dB. Part 2: from `copy_start_band` upward, copy flags between
/// channels in the direction selected by the per-band config word at
/// `+0x50`. For the 352 target all flags are forced to zero upstream
/// (`mode_cc = 1`), so this block is a traced no-op on the flag surface.
pub fn time2freq_stereo_flag_reconcile_at5(
    flags: &mut [[bool; TIME2FREQ_BANDS_AT5]; 2],
    tonality: &[[f32; TIME2FREQ_BANDS_AT5]; 2],
    correlation_db: &[f32],
    copy_start_band: usize,
    direction_words: &[u32; TIME2FREQ_BANDS_AT5],
) -> Result<(), Time2FreqError> {
    if correlation_db.len() < TIME2FREQ_BANDS_AT5 {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: TIME2FREQ_BANDS_AT5,
            actual: correlation_db.len(),
        });
    }

    for band in 0..TIME2FREQ_BANDS_AT5 {
        if flags[0][band] != flags[1][band] {
            let higher = usize::from(tonality[0][band] <= tonality[1][band]);
            let lower = 1 - higher;
            if tonality[higher][band] - tonality[lower][band] < 1.0 || 20.0 < correlation_db[band] {
                flags[lower][band] = flags[higher][band];
            }
        }
    }

    for band in copy_start_band.min(TIME2FREQ_BANDS_AT5)..TIME2FREQ_BANDS_AT5 {
        if direction_words[band] == 0 {
            flags[1][band] = flags[0][band];
        } else {
            flags[0][band] = flags[1][band];
        }
    }

    Ok(())
}

pub const TIME2FREQ_POINT_WORDS: usize = 15;
pub const TIME2FREQ_MAX_POINTS: usize = 7;

/// `sa_corr_thred` (native .rodata `0xbfa20`, nm-named, 16 f32):
/// `[10, 10, 10, 10, 12, 16 ×11]`. Read by the mode_cc==0 record-harmonization
/// block (decompile 33100) as the per-band correlation-dB threshold selecting
/// the neighbor-gated copy arm.
const SA_CORR_THRED_BITS: [u32; TIME2FREQ_BANDS_AT5] = [
    0x4120_0000, // 10.0
    0x4120_0000,
    0x4120_0000,
    0x4120_0000,
    0x4140_0000, // 12.0
    0x4180_0000, // 16.0
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
    0x4180_0000,
];

/// Detector-arena word offsets (chobj0's `+0x10` arena). `energy_84` is the
/// rolled slot-1 correlation dB row (`detector_words[0x21 + band]` as f32),
/// `rolled2` the twice-rolled row (`[0x11 + band]`); words `[1..17]` hold the
/// mode_cc==0 per-band identity flags (read+written by block 5), word `[0]` the
/// mode-aware band count (`copy_start`).
const DETECTOR_ENERGY84_BASE: usize = 0x21;
const DETECTOR_ROLLED2_BASE: usize = 0x11;

#[inline]
fn detector_energy84(detector_words: &[u32], band: usize) -> f32 {
    f32::from_bits(detector_words[DETECTOR_ENERGY84_BASE + band])
}

#[inline]
fn detector_rolled2(detector_words: &[u32], band: usize) -> f32 {
    f32::from_bits(detector_words[DETECTOR_ROLLED2_BASE + band])
}

/// One channel's persistent + per-call `set_gainc_at5` surfaces for the
/// mode_cc==0 (64/48 kbps) dispatch. `scratch` is the per-(channel,band)
/// 133-float window `[+0x3fc, +0x610)`; `history_a`/`history_b` are the
/// persistent per-band history rows mutated in place by the leaf; `prev_plane`
/// is last frame's post-everything current plane (read by the leaf, its word 18
/// mutated by the ch1 stereo pre-adjust); `cur_plane` is produced this call and
/// becomes next frame's `prev_plane`.
#[derive(Debug, Clone)]
pub struct Time2FreqSetGaincChannel {
    pub scratch: Vec<[f32; SET_GAINC_SCRATCH_FLOATS]>,
    pub history_a: Vec<[f32; SET_GAINC_HISTORY_A_FLOATS]>,
    pub history_b: Vec<[f32; SET_GAINC_HISTORY_B_FLOATS]>,
    pub prev_plane: SetGaincPlane,
    pub cur_plane: SetGaincPlane,
}

/// The shared cross-channel state the mode_cc==0 dispatch threads into
/// `time2freq_at5_with_set_gainc`: the single detector arena (chobj0's `+0x10`
/// arena, read for energy_84/rolled2 and read+written for the identity flags),
/// the header `+0x1c` word (0 at 64/48), the `cfg+0x50` reconcile direction
/// words (all-zero at 64/48), and the per-channel [`Time2FreqSetGaincChannel`]
/// surfaces.
#[derive(Debug)]
pub struct Time2FreqSetGaincState<'a> {
    pub detector_words: &'a mut [u32],
    pub header_1c: u32,
    pub direction_words: [u32; TIME2FREQ_BANDS_AT5],
    pub channels: Vec<Time2FreqSetGaincChannel>,
}

/// Native mode_cc==0 stereo record-harmonization block (decompile `33041..33182`,
/// a distinct function from the `33193` cross-channel harmonization). Runs after
/// the `set_gainc_at5` dispatch loop when `param_7 < 0x18 && param_6 == 3 &&
/// cfg+0xcc == 0`. Operates on FULL 38-word plane rows plus the detector arena:
///
/// * gate loop (33048-33071): per band a bool gate from the two channels' cur
///   level[0]-6 and prev-plane word 18;
/// * threshold (33072-33083): `sel < 0x13 → 20, sel < 0x17 → 30, else 40`;
/// * copy loop (33084-33127): on `thr < energy_84[band]` copy the smaller-count
///   cur row onto the other (ties ch0→ch1); else on the `sa_corr_thred` +
///   neighbor-identity arm the same copy;
/// * identity-flags loop (33128-33157): write `detector_words[band+1]` = whether
///   both channels' cur rows are point-identical;
/// * final loop (33158-33181): where the (previous-frame) identity flag is 0, the
///   gate is set, and `thr*0.6 <= energy_84` and `<= rolled2`, copy the
///   fewer-points cur row onto the other and set the flag.
///
/// Integer + exact-f32 decisions only; the copy loop reads the PREVIOUS frame's
/// identity flags (this call's identity loop rewrites them afterward).
#[allow(clippy::too_many_arguments)]
pub fn time2freq_mode_cc0_record_harmonization_at5(
    cur_ch0: &mut SetGaincPlane,
    cur_ch1: &mut SetGaincPlane,
    prev_ch0: &SetGaincPlane,
    prev_ch1: &SetGaincPlane,
    detector_words: &mut [u32],
    selector: i32,
) {
    // Gate array (native `aiStack_15dc`), band 0..16.
    let mut gate = [false; TIME2FREQ_BANDS_AT5];
    for (band, gate_slot) in gate.iter_mut().enumerate() {
        let a = if cur_ch0[band][0] != 0 {
            cur_ch0[band][8] as i32 - 6
        } else {
            0
        };
        let b = if cur_ch1[band][0] != 0 {
            cur_ch1[band][8] as i32 - 6
        } else {
            0
        };
        let ch1_prev18 = f32::from_bits(prev_ch1[band][0x48 / 4]);
        let ch0_prev18 = f32::from_bits(prev_ch0[band][0x48 / 4]);
        *gate_slot = (a < 1 || a - b < 2) || (ch1_prev18 <= ch0_prev18 && ch1_prev18 < 32768.0);
    }

    // Threshold from the block selector.
    let thr = if selector < 0x13 {
        20.0f32
    } else if selector < 0x17 {
        30.0
    } else {
        40.0
    };

    // Copy loop (33084-33127).
    let copy_start = detector_words[0] as i32;
    for band in 0..TIME2FREQ_BANDS_AT5 {
        if !gate[band] {
            continue;
        }
        let energy = detector_energy84(detector_words, band);
        if thr < energy {
            if (cur_ch0[band][0] as i32) <= (cur_ch1[band][0] as i32) {
                cur_ch1[band] = cur_ch0[band];
            } else {
                cur_ch0[band] = cur_ch1[band];
            }
        } else if f32::from_bits(SA_CORR_THRED_BITS[band]) < energy && copy_start < band as i32 {
            // Neighbor-gated identity flags: bands [band, band+1, band+2] for
            // band < 15; [band-1, band, band+1] for band == 15. Word j holds the
            // previous frame's flag for band j-1.
            let idents_open = if band < 0xf {
                detector_words[band] != 0
                    && detector_words[band + 1] != 0
                    && detector_words[band + 2] != 0
            } else {
                detector_words[band - 1] != 0
                    && detector_words[band] != 0
                    && detector_words[band + 1] != 0
            };
            if idents_open {
                if (cur_ch1[band][0] as i32) < (cur_ch0[band][0] as i32) {
                    cur_ch0[band] = cur_ch1[band];
                } else {
                    cur_ch1[band] = cur_ch0[band];
                }
            }
        }
    }

    // Identity-flags loop (33128-33157): compare the two channels' cur rows.
    for band in 0..TIME2FREQ_BANDS_AT5 {
        let count0 = cur_ch0[band][0] as i32;
        let flag = if count0 == cur_ch1[band][0] as i32 {
            let mut identical = 1u32;
            for i in 0..count0.max(0) as usize {
                if cur_ch0[band][1 + i] != cur_ch1[band][1 + i]
                    || cur_ch0[band][8 + i] != cur_ch1[band][8 + i]
                {
                    identical = 0;
                    break;
                }
            }
            identical
        } else {
            0
        };
        detector_words[band + 1] = flag;
    }

    // Final loop (33158-33181).
    let thr06 = thr * 0.6;
    for band in 0..TIME2FREQ_BANDS_AT5 {
        if detector_words[band + 1] == 0
            && gate[band]
            && thr06 <= detector_energy84(detector_words, band)
            && thr06 <= detector_rolled2(detector_words, band)
        {
            if (cur_ch1[band][0] as i32) < (cur_ch0[band][0] as i32) {
                cur_ch0[band] = cur_ch1[band];
            } else {
                cur_ch1[band] = cur_ch0[band];
            }
            detector_words[band + 1] = 1;
        }
    }
}

fn point_count(record: &[i32; TIME2FREQ_POINT_WORDS]) -> usize {
    record[0].clamp(0, TIME2FREQ_MAX_POINTS as i32) as usize
}

/// Largest inter-point level drop in a record's point prefix: internal
/// drops `level[i] - level[i + 1]` record `location[i]`, and the virtual
/// tail drop `level[last] - 6` records the last location.
fn largest_level_drop(record: &[i32; TIME2FREQ_POINT_WORDS]) -> (i32, i32) {
    let count = point_count(record);
    let mut best_location = -1i32;
    let mut best_drop = 0i32;
    for index in 0..count.saturating_sub(1) {
        let drop = record[8 + index] - record[9 + index];
        if best_drop < drop {
            best_location = record[1 + index];
            best_drop = drop;
        }
    }
    if count > 0 {
        let tail_drop = record[7 + count] - 6;
        if best_drop < tail_drop {
            best_location = record[count];
            best_drop = tail_drop;
        }
    }
    (best_location, best_drop)
}

fn total_level_drop(record: &[i32; TIME2FREQ_POINT_WORDS]) -> i32 {
    let count = point_count(record);
    if count == 0 {
        return 0;
    }
    let mut total = 0i32;
    for index in 0..count - 1 {
        total += record[8 + index] - record[9 + index];
    }
    total + record[7 + count] - 6
}

/// Native `time2freq_at5` band-0 attack injection (decompile
/// `33405..33655`): propagate a strong band-1 level drop into the band-0
/// gain record when bands `2..min(4, verify_band_limit)` corroborate it,
/// then normalize the band-0 point list. Returns whether the channel's
/// records were touched.
pub fn time2freq_band0_attack_injection_at5(
    band_records: &mut [[i32; TIME2FREQ_POINT_WORDS]],
    other_channel_band1: Option<&[i32; TIME2FREQ_POINT_WORDS]>,
    verify_band_limit: usize,
) -> Result<bool, Time2FreqError> {
    if band_records.len() < TIME2FREQ_BANDS_AT5 {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: TIME2FREQ_BANDS_AT5,
            actual: band_records.len(),
        });
    }

    let (best_location, best_drop) = largest_level_drop(&band_records[1]);
    if best_drop < 2 {
        return Ok(false);
    }

    // Bands 2..min(4, limit) must each have a point within one location of
    // the drop; an empty record aborts the whole injection.
    let last_verify_band = verify_band_limit.min(4);
    for band in 2..last_verify_band {
        let record = &band_records[band];
        let count = point_count(record);
        if count == 0 {
            return Ok(false);
        }
        let mut best_distance = 0x20i32;
        for index in 0..count {
            let distance = (best_location - record[1 + index]).abs();
            if distance < best_distance {
                best_distance = distance;
            }
        }
        if best_distance > 1 {
            return Ok(false);
        }
    }

    // Skip when band 0 already carries more than two levels of total drop.
    let count = point_count(&band_records[0]);
    if count > 0 && total_level_drop(&band_records[0]) > 2 {
        return Ok(false);
    }

    {
        let record = &mut band_records[0];
        if count == 0 {
            if let Some(other) = other_channel_band1 {
                if total_level_drop(other) > 1 {
                    record[1] = best_location;
                    record[8] = 7;
                    record[0] = 1;
                }
            }
        } else if count - 1 < 6 && best_location < record[1] {
            for index in (0..count).rev() {
                record[index + 2] = record[index + 1];
                record[index + 9] = record[index + 8];
            }
            record[1] = best_location;
            record[8] = (record[9] + 1).min(0xf);
            record[0] = count as i32 + 1;
        } else if count - 1 < 6 && record[count] < best_location {
            record[count + 1] = best_location;
            record[count + 8] = 7;
            for index in 0..count {
                record[index + 8] = (record[index + 8] + 1).min(0xf);
            }
            record[0] = count as i32 + 1;
        } else if best_location < 8 {
            let mut skip = false;
            if count > 0 {
                for index in 0..count - 1 {
                    if record[8 + index] - record[9 + index] < 0 {
                        skip = true;
                        break;
                    }
                }
                if record[7 + count] < 6 {
                    skip = true;
                }
            }
            if !skip && (record[1] - best_location).abs() < 2 {
                record[8] = (record[8] + 1).min(0xf);
            }
        }
    }

    // Normalization over the band-0 point list.
    normalize_point_prefix_at5(&mut band_records[0]);

    Ok(true)
}

/// Native point-prefix normalization (shared by the band-0 injection and
/// the adjacent-band merge): seven passes collapsing equal-adjacent-level
/// points, dropping a trailing level-6 point, seven passes collapsing
/// duplicate-location points, and zero-filling the remaining slots.
pub fn normalize_point_prefix_at5(record: &mut [i32; TIME2FREQ_POINT_WORDS]) {
    let mut count = point_count(record);
    for _ in 0..7 {
        let mut index = 0usize;
        while count > 1 && index < count - 1 {
            if record[8 + index] == record[9 + index] {
                for shift in index..count - 1 {
                    record[shift + 8] = record[shift + 9];
                    record[shift + 1] = record[shift + 2];
                }
                count -= 1;
                record[0] = count as i32;
            } else {
                index += 1;
            }
        }
    }
    if count > 0 && record[7 + count] == 6 {
        count -= 1;
        record[0] = count as i32;
    }
    for _ in 0..7 {
        let mut index = 0usize;
        while count > 1 && index < count - 1 {
            if record[1 + index] == record[2 + index] {
                for shift in index..count - 1 {
                    record[shift + 8] = record[shift + 9];
                    record[shift + 1] = record[shift + 2];
                }
                count -= 1;
                record[0] = count as i32;
            } else {
                index += 1;
            }
        }
    }
    for slot in count..TIME2FREQ_MAX_POINTS {
        record[slot + 8] = 0;
        record[slot + 1] = 0;
    }
}

/// Native adjacent-band record merge (`time2freq_at5` decompile
/// `33665..33790`): for each band pair `(band, band + 1)` with equal
/// nonzero point counts that are not already identical, when every
/// location and level differs by at most one, merge onto the current band
/// with per-point `min(location)` / `max(level)` and normalize. Returns
/// the bands whose records were merged.
pub fn time2freq_adjacent_band_merge_at5(
    band_records: &mut [[i32; TIME2FREQ_POINT_WORDS]],
    band_limit: usize,
) -> Result<Vec<usize>, Time2FreqError> {
    if band_records.len() < band_limit {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: band_limit,
            actual: band_records.len(),
        });
    }

    let mut merged = Vec::new();
    for band in 0..band_limit.saturating_sub(1) {
        let count = point_count(&band_records[band]);
        let next_count = point_count(&band_records[band + 1]);
        if count == next_count {
            let identical = (0..count).all(|index| {
                band_records[band][1 + index] == band_records[band + 1][1 + index]
                    && band_records[band][8 + index] == band_records[band + 1][8 + index]
            });
            if identical {
                continue;
            }
        }
        if count == 0 || count != next_count {
            continue;
        }
        let within_one = (0..count).all(|index| {
            (band_records[band][1 + index] - band_records[band + 1][1 + index]).abs() <= 1
        }) && (0..count).all(|index| {
            (band_records[band][8 + index] - band_records[band + 1][8 + index]).abs() <= 1
        });
        if !within_one {
            continue;
        }

        for index in 0..count {
            let location = band_records[band][1 + index].min(band_records[band + 1][1 + index]);
            let level = band_records[band][8 + index].max(band_records[band + 1][8 + index]);
            band_records[band][1 + index] = location;
            band_records[band][8 + index] = level;
        }
        normalize_point_prefix_at5(&mut band_records[band]);
        merged.push(band);
    }
    Ok(merged)
}

/// Native per-channel record dedup/compaction used by the cross-channel
/// gain-record harmonization sub-arm B (`time2freq_at5` decompile
/// `33330..33395`, Ghidra `0x4d0e2..0x4d180`, native `0x3d0e2..0x3d180`):
/// seven passes collapsing equal-adjacent *level* slots, a trailing
/// `level == 6` count decrement, seven passes collapsing equal-adjacent
/// *location* slots, a negative-count clamp, and a tail zero-fill.
///
/// This is a DISTINCT traversal from [`normalize_point_prefix_at5`]: native's
/// inner loops here advance the scan index on every step and shrink the active
/// length only on a collapse (the index is not re-checked after a collapse),
/// whereas `normalize_point_prefix_at5` re-checks the same index. Ported
/// faithfully to the decompile rather than reusing the normalizer.
fn time2freq_record_dedup_compaction_at5(record: &mut [i32; TIME2FREQ_POINT_WORDS]) {
    let mut count = record[0];
    if count > 0 {
        // Seven passes collapsing equal-adjacent level slots.
        for _ in 0..7 {
            let mut idx = 0i32;
            let mut bound = count - 1;
            if bound > 0 {
                loop {
                    if record[(idx + 8) as usize] == record[(idx + 9) as usize] {
                        let mut shift = idx;
                        while shift < bound {
                            record[(shift + 8) as usize] = record[(shift + 9) as usize];
                            record[(shift + 1) as usize] = record[(shift + 2) as usize];
                            shift += 1;
                        }
                        count = bound;
                        record[0] = bound;
                        bound -= 1;
                    }
                    idx += 1;
                    if idx >= bound {
                        break;
                    }
                }
            }
        }
        if count > 0 {
            if record[(count + 7) as usize] == 6 {
                count -= 1;
                record[0] = count;
            }
            if count > 0 {
                // Seven passes collapsing equal-adjacent location slots.
                for _ in 0..7 {
                    let mut idx = 0i32;
                    let mut bound = count - 1;
                    if bound > 0 {
                        loop {
                            let mut new_bound = bound;
                            if record[(idx + 1) as usize] == record[(idx + 2) as usize] {
                                let mut shift = idx;
                                while shift < bound {
                                    record[(shift + 8) as usize] = record[(shift + 9) as usize];
                                    record[(shift + 1) as usize] = record[(shift + 2) as usize];
                                    shift += 1;
                                }
                                count = bound;
                                record[0] = bound;
                                new_bound = bound - 1;
                            }
                            idx += 1;
                            bound = new_bound;
                            if idx >= bound {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if count < 0 {
        record[0] = 0;
        count = 0;
    }
    for slot in count..TIME2FREQ_MAX_POINTS as i32 {
        record[(slot + 8) as usize] = 0;
        record[(slot + 1) as usize] = 0;
    }
}

/// Native `time2freq_at5` cross-channel gain-record harmonization (decompile
/// `33193..33404`, Ghidra `0x4cbf0..0x4d180`, native `0x3cbf0..0x3d180`).
///
/// Runs inside the open post-detector harmonization gate for STEREO input only
/// (`param_5 == 2`), BEFORE the per-channel band-0 attack injection and
/// adjacent-band merge (native order: this block at `33193`, band-0 injection
/// at `33404+`). Over bands `0..min(band_limit, 0x10)` (`band_limit ==
/// param_8`) it pairs the two channels' 15-word point-prefix records (word 0 =
/// count, words 1..7 = locations, words 8..14 = levels) as
/// `(larger-count, smaller-count)` — a tie keeps channel 0 as the larger
/// (`if (ch1.count <= ch0.count) larger = ch0`) — and:
///
/// * **Sub-arm A** (counts differ, both nonzero, `33206..33273`): build the
///   subset index map of the smaller record's locations into the larger's
///   (inner scan with bail), require every consecutive matched level-drop delta
///   and both endpoint levels to agree within one, and — when they do and the
///   match count equals `larger.count - 1` — overwrite the smaller record's
///   locations, levels and count with the larger's.
/// * **Sub-arm B** (counts equal, records not identical, both nonzero, every
///   location and level within one, `33291..33399`): equalize both channels per
///   slot to `min(location)` / `max(level)`, then run
///   [`time2freq_record_dedup_compaction_at5`] on each physical channel.
///
/// Integer-only — no x87 float decision. Mutates both channels' records in
/// place. `band_limit` is `param_8`; native caps it at `0x10` (`local_2fc8`).
pub fn time2freq_cross_channel_record_harmonization_at5(
    records_ch0: &mut [[i32; TIME2FREQ_POINT_WORDS]],
    records_ch1: &mut [[i32; TIME2FREQ_POINT_WORDS]],
    band_limit: usize,
) -> Result<(), Time2FreqError> {
    let limit = band_limit.min(TIME2FREQ_BANDS_AT5);
    if records_ch0.len() < limit || records_ch1.len() < limit {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: limit,
            actual: records_ch0.len().min(records_ch1.len()),
        });
    }

    for band in 0..limit {
        // The two physical channels' records for this band; worked on in place.
        let mut rec = [records_ch0[band], records_ch1[band]];

        // Order into (larger, smaller); a tie keeps channel 0 as `larger`.
        let (larger_idx, smaller_idx) = if rec[1][0] <= rec[0][0] {
            (0usize, 1usize)
        } else {
            (1usize, 0usize)
        };
        let larger_count = rec[larger_idx][0];
        let smaller_count = rec[smaller_idx][0];
        // Tracks the smaller record's count (`local_30e4`); the sub-arm A copy
        // raises it to `larger_count`.
        let mut local_30e4 = smaller_count;

        // --- Sub-arm A: counts differ, both nonzero.
        if larger_count != smaller_count && larger_count > 0 && smaller_count > 0 {
            // For each smaller location, the larger index whose location
            // matches it (`aiStack_15fc`); records point-count is bounded by 7.
            let mut match_index = [0i32; TIME2FREQ_MAX_POINTS + 1];
            let mut match_count = 0usize;
            for i_small in 0..smaller_count as usize {
                let mut j = 0i32;
                'inner: loop {
                    while rec[larger_idx][(j + 1) as usize] != rec[smaller_idx][i_small + 1] {
                        j += 1;
                        if j >= larger_count {
                            break 'inner;
                        }
                    }
                    if match_count < match_index.len() {
                        match_index[match_count] = j;
                    }
                    match_count += 1;
                    j += 1;
                    if j >= larger_count {
                        break 'inner;
                    }
                }
            }

            // Consecutive matched level-drop deltas that agree within one.
            let mut close_count = 0usize;
            if match_count >= 1 {
                for k in 0..match_count - 1 {
                    let d_small = rec[smaller_idx][k + 8] - rec[smaller_idx][k + 9];
                    let d_large = rec[larger_idx][(match_index[k] + 8) as usize]
                        - rec[larger_idx][(match_index[k + 1] + 8) as usize];
                    if (d_small - d_large).abs() < 2 {
                        close_count += 1;
                    }
                }
            }

            let endpoint0_close = (rec[larger_idx][8] - rec[smaller_idx][8]).abs() < 2;
            let endpoint_last_close = (rec[larger_idx][(larger_count + 7) as usize]
                - rec[smaller_idx][(local_30e4 + 7) as usize])
                .abs()
                < 2;

            if endpoint0_close
                && match_count > 1
                && endpoint_last_close
                && match_count - 1 == close_count
                && match_count == (larger_count - 1) as usize
            {
                for k in 0..larger_count as usize {
                    rec[smaller_idx][k + 1] = rec[larger_idx][k + 1];
                    rec[smaller_idx][k + 8] = rec[larger_idx][k + 8];
                }
                rec[smaller_idx][0] = larger_count;
                local_30e4 = larger_count;
            }
        }

        // --- Identical check + sub-arm B. `iVar12` is now always `larger_count`.
        let run_sub_arm_b = if larger_count == local_30e4 {
            let count = larger_count as usize;
            let mut identical = true;
            for k in 0..count {
                if rec[larger_idx][k + 1] != rec[smaller_idx][k + 1]
                    || rec[larger_idx][k + 8] != rec[smaller_idx][k + 8]
                {
                    identical = false;
                    break;
                }
            }
            !identical
        } else {
            true
        };

        if run_sub_arm_b && larger_count > 0 && local_30e4 > 0 && larger_count == local_30e4 {
            let count = larger_count as usize;
            let loc_close = (0..count)
                .filter(|&k| (rec[larger_idx][k + 1] - rec[smaller_idx][k + 1]).abs() < 2)
                .count();
            if loc_close == count {
                let level_close = (0..count)
                    .filter(|&k| (rec[larger_idx][k + 8] - rec[smaller_idx][k + 8]).abs() < 2)
                    .count();
                if level_close == count {
                    for k in 0..count {
                        let location = rec[larger_idx][k + 1].min(rec[smaller_idx][k + 1]);
                        let level = rec[larger_idx][k + 8].max(rec[smaller_idx][k + 8]);
                        rec[larger_idx][k + 1] = location;
                        rec[smaller_idx][k + 1] = location;
                        rec[larger_idx][k + 8] = level;
                        rec[smaller_idx][k + 8] = level;
                    }
                    for record in rec.iter_mut() {
                        time2freq_record_dedup_compaction_at5(record);
                    }
                }
            }
        }

        records_ch0[band] = rec[0];
        records_ch1[band] = rec[1];
    }

    Ok(())
}

impl From<GainPassError> for Time2FreqError {
    fn from(error: GainPassError) -> Self {
        Self::Gain(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Time2FreqWindowPass {
    pub pre_peak: f32,
    pub post_peak: f32,
    pub cursor: usize,
}

/// Native per-band gain-window pass (`time2freq_at5` decompile
/// `33792..33860`): when the current or previous record has points, scan
/// the 256-sample absolute pre-peak, build the gain window from the
/// previous (attack) and current (release) point prefixes via
/// `gainc_window_enc_at5`, multiply samples `0..=cursor` in place, and
/// scan the post-window peak. Returns `None` when both records are empty
/// (native leaves the band untouched with zero peaks).
pub fn time2freq_window_pass_at5(
    band_samples: &mut [f32],
    previous_record: &[i32],
    current_record: &[i32],
) -> Result<Option<Time2FreqWindowPass>, Time2FreqError> {
    if band_samples.len() < 256 {
        return Err(Time2FreqError::BandDataTooShort {
            band: 0,
            needed: 256,
            actual: band_samples.len(),
        });
    }
    let previous_count = previous_record.first().copied().unwrap_or(0);
    let current_count = current_record.first().copied().unwrap_or(0);
    if previous_count <= 0 && current_count <= 0 {
        return Ok(None);
    }

    let mut pre_peak = band_samples[0].abs();
    for value in &band_samples[1..256] {
        let magnitude = value.abs();
        if pre_peak < magnitude {
            pre_peak = magnitude;
        }
    }

    let mut window = [0.0f32; GAIN_WINDOW_VALUES];
    let cursor = gainc_window_enc_at5(previous_record, current_record, &mut window)?;
    for index in 0..=cursor.min(255) {
        band_samples[index] *= window[index];
    }

    let mut post_peak = band_samples[0].abs();
    for value in &band_samples[1..256] {
        let magnitude = value.abs();
        if post_peak < magnitude {
            post_peak = magnitude;
        }
    }

    Ok(Some(Time2FreqWindowPass {
        pre_peak,
        post_peak,
        cursor,
    }))
}

/// Native stereo peak equalization (`time2freq_at5` decompile
/// `33864..33889`): when the two channels' pre-window peaks sit within 5%
/// (larger under `smaller * 1.05`), both take the larger; the post-window
/// peaks equalize when within the two-sided 5% window
/// (`larger * 0.95 < other < larger * 1.05`).
pub fn time2freq_stereo_peak_equalize_at5(pre_peaks: &mut [f32; 2], post_peaks: &mut [f32; 2]) {
    let smaller = usize::from(pre_peaks[1] <= pre_peaks[0]);
    let larger = 1 - smaller;
    let larger_value = pre_peaks[larger];
    if larger_value < pre_peaks[smaller] * 1.05 {
        let unified = if larger_value <= pre_peaks[smaller] {
            pre_peaks[smaller]
        } else {
            larger_value
        };
        pre_peaks[0] = unified;
        pre_peaks[1] = unified;
    }

    let smaller = usize::from(post_peaks[1] <= post_peaks[0]);
    let larger = 1 - smaller;
    let larger_value = post_peaks[larger];
    let smaller_value = post_peaks[smaller];
    if smaller_value * 0.95 < larger_value && larger_value < smaller_value * 1.05 {
        let unified = if larger_value <= smaller_value {
            smaller_value
        } else {
            larger_value
        };
        post_peaks[0] = unified;
        post_peaks[1] = unified;
    }
}

/// Find the point whose level the native overshoot correction decrements:
/// the first index with a strict level drop (`level[i] > level[i + 1]`,
/// disassembly `0x3e09c..0x3e0bc`), else the last point when its level
/// exceeds 6 (`0x3e0cb..0x3e0e9`), else `None`.
fn overshoot_target_point(record: &[i32; TIME2FREQ_POINT_WORDS]) -> Option<usize> {
    let count = point_count(record);
    for index in 0..count.saturating_sub(1) {
        if record[8 + index] > record[9 + index] {
            return Some(index);
        }
    }
    if count > 0 && record[7 + count] > 6 {
        return Some(count - 1);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Time2FreqOvershoot {
    pub post_peak: f32,
    pub attempts: usize,
}

/// Native overshoot correction (`time2freq_at5` decompile `33920..34060`):
/// while the windowed peak exceeds `65536.0` or eight times the pre-window
/// peak and the current record still has points, decrement the first
/// strict-drop point's level (or the tail level when above 6), normalize
/// the record, re-window the source samples, and rescan the peak — at
/// most two attempts.
pub fn time2freq_overshoot_correct_at5(
    source_samples: &[f32],
    band_samples: &mut [f32],
    previous_record: &[i32],
    current_record: &mut [i32; TIME2FREQ_POINT_WORDS],
    pre_peak: f32,
    mut post_peak: f32,
) -> Result<Time2FreqOvershoot, Time2FreqError> {
    if source_samples.len() < 256 || band_samples.len() < 256 {
        return Err(Time2FreqError::BandDataTooShort {
            band: 0,
            needed: 256,
            actual: source_samples.len().min(band_samples.len()),
        });
    }

    let mut attempts = 0usize;
    while point_count(current_record) > 0
        && (65536.0 < post_peak || pre_peak * 8.0 < post_peak)
        && attempts < 2
    {
        let Some(target) = overshoot_target_point(current_record) else {
            break;
        };
        current_record[8 + target] -= 1;
        normalize_point_prefix_at5(current_record);

        band_samples[..256].copy_from_slice(&source_samples[..256]);
        let mut window = [0.0f32; GAIN_WINDOW_VALUES];
        let cursor = gainc_window_enc_at5(previous_record, current_record, &mut window)?;
        for index in 0..=cursor.min(255) {
            band_samples[index] *= window[index];
        }

        post_peak = band_samples[0].abs();
        for value in &band_samples[1..256] {
            let magnitude = value.abs();
            if post_peak < magnitude {
                post_peak = magnitude;
            }
        }
        attempts += 1;
    }

    Ok(Time2FreqOvershoot {
        post_peak,
        attempts,
    })
}

/// Compose the native post-harmonization gain-window peak scan, stereo peak
/// equalization, overshoot correction, and post-correction stereo record copy
/// (`time2freq_at5`, decompile 33792..34208).
fn time2freq_record_overshoot_stage_at5(
    channels: &[Time2FreqChannelState],
    previous_records: &[Vec<[i32; TIME2FREQ_POINT_WORDS]>],
    current_records: &mut [Vec<[i32; TIME2FREQ_POINT_WORDS]>],
    band_limit: usize,
) -> Result<(), Time2FreqError> {
    let channel_count = channels.len().min(current_records.len());
    let mut pre_peaks = vec![vec![0.0f32; band_limit]; channel_count];
    let mut post_peaks = vec![vec![0.0f32; band_limit]; channel_count];

    // Native first scans every (band, channel), retaining the pre/post-window
    // peaks, then equalizes the two channels' peak surfaces per band.
    for band in 0..band_limit {
        for ch in 0..channel_count {
            let source = &channels[ch].band_inputs[band];
            if source.len() < 256 {
                return Err(Time2FreqError::BandDataTooShort {
                    band,
                    needed: 256,
                    actual: source.len(),
                });
            }
            let mut samples = [0.0f32; 256];
            samples.copy_from_slice(&source[..256]);
            if let Some(pass) = time2freq_window_pass_at5(
                &mut samples,
                &previous_records[ch][band],
                &current_records[ch][band],
            )? {
                pre_peaks[ch][band] = pass.pre_peak;
                post_peaks[ch][band] = pass.post_peak;
            }
        }
        if channel_count == 2 {
            let mut pre = [pre_peaks[0][band], pre_peaks[1][band]];
            let mut post = [post_peaks[0][band], post_peaks[1][band]];
            time2freq_stereo_peak_equalize_at5(&mut pre, &mut post);
            pre_peaks[0][band] = pre[0];
            pre_peaks[1][band] = pre[1];
            post_peaks[0][band] = post[0];
            post_peaks[1][band] = post[1];
        }
    }

    for band in 0..band_limit {
        let stereo_identical_before = channel_count == 2
            && current_records[0][band][0] == current_records[1][band][0]
            && (0..point_count(&current_records[0][band])).all(|index| {
                current_records[0][band][1 + index] == current_records[1][band][1 + index]
                    && current_records[0][band][8 + index] == current_records[1][band][8 + index]
            });

        for ch in 0..channel_count {
            let source = &channels[ch].band_inputs[band];
            let mut windowed = [0.0f32; 256];
            windowed.copy_from_slice(&source[..256]);
            time2freq_overshoot_correct_at5(
                source,
                &mut windowed,
                &previous_records[ch][band],
                &mut current_records[ch][band],
                pre_peaks[ch][band],
                post_peaks[ch][band],
            )?;
        }

        // When stereo records were identical before the independent correction
        // loops but diverged afterward, native copies the record with the
        // smaller total level drop over the other channel (ties ch0 -> ch1).
        if stereo_identical_before && current_records[0][band] != current_records[1][band] {
            let drop0 = total_level_drop(&current_records[0][band]);
            let drop1 = total_level_drop(&current_records[1][band]);
            if drop1 < drop0 {
                current_records[0][band] = current_records[1][band];
            } else {
                current_records[1][band] = current_records[0][band];
            }
        }
    }
    Ok(())
}

/// Native `time2freq_at5` main MDCT pass (the `0x3d2e1` site, decompile
/// `34246..34280`, fires for every band on every call): copy the 256 band
/// samples, apply the gain window when either record has points, and run
/// `winormal_mdct_128_Ex_at5`. This leaf retains `g_a_wind0_at5` as its
/// explicit legacy/default window; the composed driver selects wind0..wind3
/// from the previous/current tonal flags before calling the private windowed
/// variant. `g_a_rev_at5[band]` supplies the output order.
pub fn time2freq_mdct_pass_at5(
    band_samples: &[f32],
    previous_record: &[i32],
    current_record: &[i32],
    band_index: usize,
    spectrum_out: &mut [f32],
) -> Result<bool, Time2FreqError> {
    time2freq_mdct_pass_with_window_at5(
        band_samples,
        previous_record,
        current_record,
        band_index,
        spectrum_out,
        &wind0_at5(),
    )
}

/// Select the native MDCT window from the previous/current tonal flags.
///
/// Native encoder evidence: the 64 kbps `time2freq_at5` MDCT calls at
/// `0x3d2e1` and `0x3d551` pass `g_a_wind{0,1,2,3}_at5` for flag pairs
/// `(0,0)`, `(0,1)`, `(1,0)`, and `(1,1)` respectively. The same four-way
/// law is visible statically in `backward_transform_at5` (native `0x43f20`,
/// decompile 28127-28130).
pub fn time2freq_mdct_window_at5(
    previous_tonal: bool,
    current_tonal: bool,
) -> [f32; GAIN_WINDOW_VALUES] {
    match (previous_tonal, current_tonal) {
        (false, false) => wind0_at5(),
        (false, true) => wind1_at5(),
        (true, false) => wind2_at5(),
        (true, true) => wind3_at5(),
    }
}

fn time2freq_mdct_pass_with_window_at5(
    band_samples: &[f32],
    previous_record: &[i32],
    current_record: &[i32],
    band_index: usize,
    spectrum_out: &mut [f32],
    mdct_window: &[f32],
) -> Result<bool, Time2FreqError> {
    if band_samples.len() < 256 {
        return Err(Time2FreqError::BandDataTooShort {
            band: band_index,
            needed: 256,
            actual: band_samples.len(),
        });
    }
    if band_index >= TIME2FREQ_BANDS_AT5 {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: TIME2FREQ_BANDS_AT5,
            actual: band_index,
        });
    }

    let mut samples = [0.0f32; 256];
    samples.copy_from_slice(&band_samples[..256]);
    let previous_count = previous_record.first().copied().unwrap_or(0);
    let current_count = current_record.first().copied().unwrap_or(0);
    let mut windowed = false;
    if previous_count > 0 || current_count > 0 {
        let mut window = [0.0f32; GAIN_WINDOW_VALUES];
        let cursor = gainc_window_enc_at5(previous_record, current_record, &mut window)?;
        for index in 0..=cursor.min(255) {
            samples[index] *= window[index];
        }
        windowed = true;
    }

    let output_order = usize::from(rev_at5()[band_index]);
    winormal_mdct_128_ex_at5(&samples, spectrum_out, mdct_window, output_order)?;
    Ok(windowed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Time2FreqDelayedBand {
    Copied,
    Transformed { windowed: bool },
}

/// Native `time2freq_at5` delayed pass (the `0x3d551` site, decompile
/// `34283..34324`): when both gain-record counts are zero, copy the 128 words
/// of the SAME-CALL main-pass spectrum for this band through into the delayed
/// (`spec_a`) surface unchanged; otherwise run the second MDCT over the band
/// samples, windowing only when the function-local gate records carry points
/// (zeroed at entry — never for the traced 352 path).
///
/// Native evidence (`time2freq_at5`, native `0x3c480`, Ghidra `0x4c480`, fn at
/// decompile `32593`; Ghidra addr = native + 0x10000): the delayed loop runs
/// AFTER the main MDCT loop populated `param_2` (the main spectrum table) in the
/// same call. In the copy branch (`34284..34292`) both `local_2f5c` record
/// tables (word 0, the count) are zero and the loop copies 0x80 words
/// `param_2[ch] + band*0x200` -> `param_3[ch] + band*0x200`, i.e.
/// `spec_a[band] = spec_b[band]` for this call's main-pass output. The A/B
/// spectrum tables ARE zeroed per core call at `atx_encode_core` entry
/// (`46062..46071`), but that is ENTRY state only: the copy reads the main table
/// AFTER the main loop wrote it, so the delayed surface aliases the current
/// call's main spectrum, not a cross-frame delayed buffer. The band-b delayed
/// output reads only band b's main output, so per-band interleaving of the two
/// loops is equivalent to running them separately.
pub fn time2freq_delayed_pass_at5(
    band_samples: &[f32],
    main_spectrum: &[f32],
    previous_record: &[i32],
    current_record: &[i32],
    gate_previous_record: &[i32],
    gate_current_record: &[i32],
    band_index: usize,
    delayed_out: &mut [f32],
) -> Result<Time2FreqDelayedBand, Time2FreqError> {
    time2freq_delayed_pass_with_window_at5(
        band_samples,
        main_spectrum,
        previous_record,
        current_record,
        gate_previous_record,
        gate_current_record,
        band_index,
        delayed_out,
        &wind0_at5(),
    )
}

#[allow(clippy::too_many_arguments)]
fn time2freq_delayed_pass_with_window_at5(
    band_samples: &[f32],
    main_spectrum: &[f32],
    previous_record: &[i32],
    current_record: &[i32],
    gate_previous_record: &[i32],
    gate_current_record: &[i32],
    band_index: usize,
    delayed_out: &mut [f32],
    mdct_window: &[f32],
) -> Result<Time2FreqDelayedBand, Time2FreqError> {
    if main_spectrum.len() < MDCT_128_OUTPUT_COUNT || delayed_out.len() < MDCT_128_OUTPUT_COUNT {
        return Err(Time2FreqError::BandDataTooShort {
            band: band_index,
            needed: MDCT_128_OUTPUT_COUNT,
            actual: main_spectrum.len().min(delayed_out.len()),
        });
    }

    let previous_count = previous_record.first().copied().unwrap_or(0);
    let current_count = current_record.first().copied().unwrap_or(0);
    if previous_count <= 0 && current_count <= 0 {
        delayed_out[..MDCT_128_OUTPUT_COUNT]
            .copy_from_slice(&main_spectrum[..MDCT_128_OUTPUT_COUNT]);
        return Ok(Time2FreqDelayedBand::Copied);
    }

    if band_samples.len() < 256 {
        return Err(Time2FreqError::BandDataTooShort {
            band: band_index,
            needed: 256,
            actual: band_samples.len(),
        });
    }
    let mut samples = [0.0f32; 256];
    samples.copy_from_slice(&band_samples[..256]);
    let gate_previous = gate_previous_record.first().copied().unwrap_or(0);
    let gate_current = gate_current_record.first().copied().unwrap_or(0);
    let mut windowed = false;
    if gate_previous > 0 || gate_current > 0 {
        let mut window = [0.0f32; GAIN_WINDOW_VALUES];
        let cursor = gainc_window_enc_at5(gate_previous_record, gate_current_record, &mut window)?;
        for index in 0..=cursor.min(255) {
            samples[index] *= window[index];
        }
        windowed = true;
    }

    let output_order = usize::from(rev_at5()[band_index]);
    winormal_mdct_128_ex_at5(&samples, delayed_out, mdct_window, output_order)?;
    Ok(Time2FreqDelayedBand::Transformed { windowed })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time2FreqBandOutcome {
    pub main_windowed: bool,
    pub delayed: Time2FreqDelayedBand,
}

/// Native gate for the post-detector gain-record harmonization block that
/// contains band-0 attack injection and adjacent-band merge (`time2freq_at5`
/// decompile lines `33183..33191`, Ghidra `0x4eea3..0x4f1b7`, native
/// `0x3eea3..0x3f1b7` for the merge loop).
pub fn time2freq_post_detector_record_harmonization_gate_at5(
    side_0x1c: i32,
    channel_count: usize,
    bandwidth: i32,
) -> bool {
    side_0x1c != 2
        && ((bandwidth < 0x10 && channel_count == 1) || (bandwidth < 0x14 && channel_count == 2))
}

/// Composed post-detector `time2freq_at5` channel pipeline when the native
/// gain-record harmonization block is open: band-0 attack injection,
/// adjacent-band merge, then per band the main MDCT pass into the spectrum
/// and the delayed pass into the delayed buffer. The detector runs upstream
/// (`mode_cc = 1` dispatch); the tonality flags on the traced 352 path are
/// zero and do not feed this pipeline.
#[allow(clippy::too_many_arguments)]
pub fn time2freq_post_detector_channel_at5(
    band_inputs: &[&[f32]],
    current_records: &mut [[i32; TIME2FREQ_POINT_WORDS]],
    previous_records: &[[i32; TIME2FREQ_POINT_WORDS]],
    other_channel_band1: Option<&[i32; TIME2FREQ_POINT_WORDS]>,
    band_limit: usize,
    spectra_out: &mut [f32],
    delayed_out: &mut [f32],
) -> Result<Vec<Time2FreqBandOutcome>, Time2FreqError> {
    let tonal_flags = vec![(false, false); band_limit];
    time2freq_post_detector_channel_impl_at5(
        band_inputs,
        current_records,
        previous_records,
        other_channel_band1,
        band_limit,
        true,
        &tonal_flags,
        spectra_out,
        delayed_out,
    )
}

// Native runs the full main MDCT loop then the delayed loop per channel, but the
// delayed pass for band b reads only band b's just-written main spectrum, so the
// per-band interleave below is equivalent to native's two-loop structure.
#[allow(clippy::too_many_arguments)]
fn time2freq_post_detector_channel_impl_at5(
    band_inputs: &[&[f32]],
    current_records: &mut [[i32; TIME2FREQ_POINT_WORDS]],
    previous_records: &[[i32; TIME2FREQ_POINT_WORDS]],
    other_channel_band1: Option<&[i32; TIME2FREQ_POINT_WORDS]>,
    band_limit: usize,
    record_harmonization_open: bool,
    tonal_flags: &[(bool, bool)],
    spectra_out: &mut [f32],
    delayed_out: &mut [f32],
) -> Result<Vec<Time2FreqBandOutcome>, Time2FreqError> {
    if band_inputs.len() < band_limit
        || current_records.len() < TIME2FREQ_BANDS_AT5
        || previous_records.len() < band_limit
        || tonal_flags.len() < band_limit
    {
        return Err(Time2FreqError::BandInputsTooShort {
            needed: band_limit.max(TIME2FREQ_BANDS_AT5),
            actual: band_inputs
                .len()
                .min(current_records.len())
                .min(previous_records.len())
                .min(tonal_flags.len()),
        });
    }
    if spectra_out.len() < band_limit * MDCT_128_OUTPUT_COUNT
        || delayed_out.len() < band_limit * MDCT_128_OUTPUT_COUNT
    {
        return Err(Time2FreqError::BandDataTooShort {
            band: 0,
            needed: band_limit * MDCT_128_OUTPUT_COUNT,
            actual: spectra_out.len().min(delayed_out.len()),
        });
    }

    if record_harmonization_open {
        time2freq_band0_attack_injection_at5(current_records, other_channel_band1, band_limit)?;
        time2freq_adjacent_band_merge_at5(current_records, band_limit)?;
    }

    let gate_record = [0i32; TIME2FREQ_POINT_WORDS];
    let mut outcomes = Vec::with_capacity(band_limit);
    for band in 0..band_limit {
        let range = band * MDCT_128_OUTPUT_COUNT..(band + 1) * MDCT_128_OUTPUT_COUNT;
        let (current_tonal, previous_tonal) = tonal_flags[band];
        let mdct_window = time2freq_mdct_window_at5(previous_tonal, current_tonal);
        let main_windowed = time2freq_mdct_pass_with_window_at5(
            band_inputs[band],
            &previous_records[band],
            &current_records[band],
            band,
            &mut spectra_out[range.clone()],
            &mdct_window,
        )?;
        // The record-empty delayed branch copies this band's SAME-CALL main
        // spectrum (native `param_2[ch] + band*0x200`, decompile 34286..34292),
        // which the main pass just wrote into `spectra_out[range]`.
        let main_spectrum: [f32; MDCT_128_OUTPUT_COUNT] =
            spectra_out[range.clone()].try_into().unwrap();
        let delayed_band = &mut delayed_out[range];
        let delayed = time2freq_delayed_pass_with_window_at5(
            band_inputs[band],
            &main_spectrum,
            &previous_records[band],
            &current_records[band],
            &gate_record,
            &gate_record,
            band,
            delayed_band,
            &mdct_window,
        )?;
        outcomes.push(Time2FreqBandOutcome {
            main_windowed,
            delayed,
        });
    }
    Ok(outcomes)
}

/// Owned per-band seed surfaces for one detector dispatch
/// (`gain_detect_band_at5`). These mirror the native detector's per-band
/// working state: the 140-float front window (byte `0x600 + 0x1d0..`), the
/// seeded 64-slot spectrum/envelope history, the previous-frame peak/level
/// scalars, and the mutable persistent candidate pool / gc output slab /
/// counts. On the 352 path the detector runs once per `time2freq_at5` call
/// (`mode_cc = 1` dispatch at decompile `33037`).
#[derive(Debug, Clone)]
pub struct Time2FreqDetectorBandSeed {
    pub band_window: Vec<f32>,
    pub spectrum: Vec<f32>,
    pub envelope: Vec<f32>,
    pub prev_max_slot: usize,
    pub prev_peak_slot_plus_32: usize,
    pub prev_level_a: f32,
    pub prev_level_b: f32,
    pub stored_peak_a: f32,
    pub current_bin0_peak: f32,
    pub carried_removed_count: usize,
    pub persistent_records: Vec<GainDetectCandidateListRecord>,
    pub output_records: Vec<u32>,
    pub counts: Vec<i32>,
}

/// Native detector dispatch for one channel (`mode_cc != 0` branch): run the
/// composed `detect_gainc_data_new_at5` per-band chain over each band's seed
/// surfaces and return the resulting 15-word point-prefix records. The
/// compact point words are the decision surface consumed by the injection,
/// merge, gain-window, and MDCT stages (and, downstream, the packer side
/// data).
pub fn time2freq_channel_detect_records_at5(
    seeds: &mut [Time2FreqDetectorBandSeed],
    band_limit: usize,
) -> Result<Vec<[i32; TIME2FREQ_POINT_WORDS]>, Time2FreqError> {
    let (records, _outcomes) = time2freq_channel_detect_at5(seeds, band_limit)?;
    Ok(records)
}

/// Like `time2freq_channel_detect_records_at5` but also returns each band's
/// full `GainDetectBandOutcome`. The outcome carries the fresh front peaks /
/// weights and the merged forward pool (`next_pool_records`) that the
/// cross-frame seed evolution (`time2freq_detector_seed_evolve_at5`) needs.
/// The per-band seed is mutated in place: after the call `seed.counts` holds
/// this call's final `[group0, group1]` gc counts and `seed.output_records`
/// holds the populated gc output slab (group 0 = merged pool, group 1 = fresh
/// upper-group records).
pub fn time2freq_channel_detect_at5(
    seeds: &mut [Time2FreqDetectorBandSeed],
    band_limit: usize,
) -> Result<
    (
        Vec<[i32; TIME2FREQ_POINT_WORDS]>,
        Vec<GainDetectBandOutcome>,
    ),
    Time2FreqError,
> {
    if seeds.len() < band_limit {
        return Err(Time2FreqError::ChannelStateTooShort {
            needed: band_limit,
            actual: seeds.len(),
        });
    }
    let mut records = vec![[0i32; TIME2FREQ_POINT_WORDS]; TIME2FREQ_BANDS_AT5.max(band_limit)];
    let mut outcomes = Vec::with_capacity(band_limit);
    for (band, record) in records.iter_mut().enumerate().take(band_limit) {
        let seed = &mut seeds[band];
        let outcome = gain_detect_band_at5(
            &seed.band_window,
            &seed.spectrum,
            &seed.envelope,
            seed.prev_max_slot,
            seed.prev_peak_slot_plus_32,
            seed.prev_level_a,
            seed.prev_level_b,
            seed.stored_peak_a,
            seed.current_bin0_peak,
            seed.carried_removed_count,
            &mut seed.persistent_records,
            &mut seed.output_records,
            &mut seed.counts,
        )?;
        *record = outcome.compact_point_words;
        outcomes.push(outcome);
    }
    Ok((records, outcomes))
}

fn time2freq_channel_detect_lean_at5(
    seeds: &mut [Time2FreqDetectorBandSeed],
    band_limit: usize,
    scratch: &mut GainDetectScratch,
) -> Result<
    (
        Vec<[i32; TIME2FREQ_POINT_WORDS]>,
        Vec<GainDetectBandOutcome>,
    ),
    Time2FreqError,
> {
    if seeds.len() < band_limit {
        return Err(Time2FreqError::ChannelStateTooShort {
            needed: band_limit,
            actual: seeds.len(),
        });
    }
    let mut records = vec![[0i32; TIME2FREQ_POINT_WORDS]; TIME2FREQ_BANDS_AT5.max(band_limit)];
    let mut prune_blocked = [false; TIME2FREQ_BANDS_AT5];
    for (band, record) in records.iter_mut().enumerate().take(band_limit) {
        let seed = &mut seeds[band];
        let outcome = gain_detect_band_with_scratch_at5(
            &seed.band_window,
            &seed.spectrum,
            &seed.envelope,
            seed.prev_max_slot,
            seed.prev_peak_slot_plus_32,
            seed.prev_level_a,
            seed.prev_level_b,
            seed.stored_peak_a,
            seed.current_bin0_peak,
            seed.carried_removed_count,
            &mut seed.persistent_records,
            &mut seed.output_records,
            &mut seed.counts,
            scratch,
            false,
        )?;
        *record = outcome.compact_point_words;
        prune_blocked[band] = outcome.prune_blocked;
        time2freq_detector_seed_evolve_in_place_at5(seed, &outcome, scratch)?;
    }
    Ok((
        records,
        gain_detect_prune_markers_at5(band_limit, &prune_blocked),
    ))
}

fn time2freq_detector_seed_evolve_in_place_at5(
    seed: &mut Time2FreqDetectorBandSeed,
    outcome: &GainDetectLeanOutcome,
    scratch: &GainDetectScratch,
) -> Result<(), Time2FreqError> {
    if seed.spectrum.len() < GAIN_DETECT_HISTORY_PEAK_VALUES
        || seed.envelope.len() < GAIN_DETECT_HISTORY_PEAK_VALUES
    {
        return Err(Time2FreqError::ChannelStateTooShort {
            needed: GAIN_DETECT_HISTORY_PEAK_VALUES,
            actual: seed.spectrum.len().min(seed.envelope.len()),
        });
    }

    let next_stored_peak_a = seed.spectrum[GAIN_DETECT_PEAK_BINS - 1];
    seed.spectrum
        .copy_within(GAIN_DETECT_PEAK_BINS..GAIN_DETECT_HISTORY_PEAK_VALUES, 0);
    for (destination, peak) in seed.spectrum[GAIN_DETECT_PEAK_BINS..GAIN_DETECT_HISTORY_PEAK_VALUES]
        .iter_mut()
        .zip(outcome.front.peaks.bins())
    {
        *destination = *peak;
    }

    seed.envelope
        .copy_within(GAIN_DETECT_PEAK_BINS..2 * GAIN_DETECT_PEAK_BINS - 1, 0);
    for (destination, weight) in seed.envelope
        [GAIN_DETECT_PEAK_BINS - 1..GAIN_DETECT_HISTORY_PEAK_VALUES - 1]
        .iter_mut()
        .zip(outcome.front.weights.iter())
    {
        *destination = weight.weight();
    }
    seed.envelope[GAIN_DETECT_HISTORY_PEAK_VALUES - 1] = 0.0;

    let writeback = gain_detect_band_state_writeback_at5(GainDetectBandStateWritebackFields {
        prev_peak_slot_plus_32: seed.prev_peak_slot_plus_32,
        current_peak_slot: outcome.front.peaks.max_index(),
        previous_level_bits: seed.prev_level_b.to_bits(),
        current_peak_value_bits: outcome.front.peaks.max_value().to_bits(),
        gain_records_total: 0,
        gain_records_removed: 0,
        list_count_primary: 0,
        list_count_secondary: 0,
        active_chain_count: 0,
        stereo_energy_a_bits: 0,
        stereo_energy_b_bits: 0,
    })
    .map_err(Time2FreqError::from)?;
    seed.prev_max_slot = writeback.prev_max_slot();
    seed.prev_peak_slot_plus_32 = writeback.prev_peak_slot() + GAIN_DETECT_PEAK_BINS;
    seed.prev_level_a = f32::from_bits(writeback.prev_level_a_bits());
    seed.prev_level_b = f32::from_bits(writeback.prev_level_b_bits());
    seed.stored_peak_a = next_stored_peak_a;
    seed.current_bin0_peak = outcome.front.peaks.bins()[0];
    seed.carried_removed_count = outcome.prune_pool2_removed_count;

    let group1_count = seed.counts.get(1).copied().unwrap_or(0);
    seed.counts.clear();
    seed.counts.extend_from_slice(&[group1_count, 0]);

    let slab_words = GC_SET_POINTS_OUTPUT_GROUPS * GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS;
    seed.output_records
        .copy_within(GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS..slab_words, 0);
    seed.output_records[GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS..slab_words].fill(0);

    seed.persistent_records.clear();
    seed.persistent_records
        .extend_from_slice(scratch.next_pool_records(outcome));
    Ok(())
}

/// Evolve one per-band detector seed across a core call, per the native
/// `detect_gainc_data_new_at5` writeback (decompile `32494..32530`). The
/// `post_seed` is the seed state *after* `time2freq_channel_detect_at5` ran it
/// (its `counts` / `output_records` slab hold this call's gc output); `outcome`
/// is that band's returned outcome. `next_band_window` is the fresh detector
/// window (slot-0-base floats `500..640`, post-residual) the next call reads.
///
/// Native evidence (channel block `param_1[ch]`, band `local_37b4`):
/// - spectrum history (`0x1204`, 64 f32): `+32` rolling shift, low half =
///   entry spectrum `[32..64]`, high half = this call's fresh abs peaks
///   (`local_9c`) — `gain_detect_primary_history_shift_at5` (32496..32502).
/// - `stored_peak_a` (`0x2204`): entry spectrum word 31 (`local_120`) (32503).
/// - prev scalars (`0x1104/0x1108/0x1184/0x1188`): `prev_max_slot =
///   prev_peak_slot_plus_32 - 32`, `prev_peak_slot_plus_32 = current_peak_slot
///   + 32`, `prev_level_a = prev_level_b`, `prev_level_b = current_peak_value`
///   — `gain_detect_band_state_writeback_at5` (32504..32507).
/// - envelope history (`0x2244`, 64 f32): entry `[32..63]` prefix, this call's
///   32 window weights at words `31..63`, trailing word 0 —
///   `gain_detect_secondary_history_shift_at5` (32508..32514).
/// - counts chains (`0x3244/0x3248`): `counts_seed(N+1) = [counts_final(N)[1],
///   0]` (0x3248 delays into 0x3244, 0x3248 takes the reset group-1 count)
///   (32515..32516). The removed-count chain `0x32c8` -> next `local_55c[1]`
///   ages the pool-2 removal count into the next call's gate removal seed
///   (`carried_removed_count`, decompile 32518/31509); it is nonzero once the
///   over-seven prune removal branch flags a pool-2 partner (docs/12 §2.2,
///   `removal_partner_events.ndjson`). The `0x32c4`/`0x3344` chains (pool-0
///   removed / pool-1 dup carries) have no gate effect and stay dead here.
/// - candidate pool (`0x3384`, 0x600 int): list A = group 0 = merged forward
///   pool (`next_pool_records`); the gc output slab's group-0 region preseeds
///   from group 1's final words, group-1 region is fresh zeros (32521..32527).
pub fn time2freq_detector_seed_evolve_at5(
    post_seed: &Time2FreqDetectorBandSeed,
    outcome: &GainDetectBandOutcome,
    next_band_window: Vec<f32>,
) -> Result<Time2FreqDetectorBandSeed, Time2FreqError> {
    // Fresh abs peaks (native `local_9c`) and their strict-`<` first-wins
    // argmax slot/value (native `local_37d0`/`local_37d4`).
    let fresh_peak_bits: Vec<u32> = outcome
        .front
        .peaks
        .bins()
        .iter()
        .map(|value| value.to_bits())
        .collect();

    // Spectrum history: `+32` rolling shift.
    let spectrum_bits: Vec<u32> = post_seed.spectrum.iter().map(|v| v.to_bits()).collect();
    let next_spectrum_bits =
        gain_detect_primary_history_shift_at5(&spectrum_bits, &fresh_peak_bits)
            .map_err(Time2FreqError::from)?;
    let next_spectrum: Vec<f32> = next_spectrum_bits
        .iter()
        .map(|b| f32::from_bits(*b))
        .collect();

    // stored_peak_a <- entry spectrum word 31 (native `local_120`, `0x2204`).
    let next_stored_peak_a = post_seed.spectrum[GAIN_DETECT_PEAK_BINS - 1];

    // Envelope history: entry `[32..63]` prefix, fresh window weights at
    // `31..63`, trailing word 0. The verified `..._shift_at5` helper reads its
    // `previous_history_words[31..62]` into the result prefix, so present the
    // entry envelope's `[32..63]` window at that offset.
    let mut envelope_prev = vec![0u32; GAIN_DETECT_HISTORY_PEAK_VALUES];
    for (dest, src) in envelope_prev[GAIN_DETECT_PEAK_BINS - 1..2 * (GAIN_DETECT_PEAK_BINS - 1)]
        .iter_mut()
        .zip(&post_seed.envelope[GAIN_DETECT_PEAK_BINS..2 * GAIN_DETECT_PEAK_BINS - 1])
    {
        *dest = src.to_bits();
    }
    let weight_bits: Vec<u32> = outcome
        .front
        .weights
        .iter()
        .map(|weight| weight.weight().to_bits())
        .collect();
    let next_envelope_bits =
        gain_detect_secondary_history_shift_at5(&envelope_prev, &weight_bits, 0)
            .map_err(Time2FreqError::from)?;
    let next_envelope: Vec<f32> = next_envelope_bits
        .iter()
        .map(|b| f32::from_bits(*b))
        .collect();

    // Prev scalars via the verified band-state writeback mapping.
    let writeback = gain_detect_band_state_writeback_at5(GainDetectBandStateWritebackFields {
        prev_peak_slot_plus_32: post_seed.prev_peak_slot_plus_32,
        current_peak_slot: outcome.front.peaks.max_index(),
        previous_level_bits: post_seed.prev_level_b.to_bits(),
        current_peak_value_bits: outcome.front.peaks.max_value().to_bits(),
        // Count fields do not feed the record decision surface; carry a
        // consistent (dead) mapping for completeness.
        gain_records_total: 0,
        gain_records_removed: 0,
        list_count_primary: 0,
        list_count_secondary: 0,
        active_chain_count: 0,
        stereo_energy_a_bits: 0,
        stereo_energy_b_bits: 0,
    })
    .map_err(Time2FreqError::from)?;
    let next_prev_max_slot = writeback.prev_max_slot();
    let next_prev_peak_slot_plus_32 = writeback.prev_peak_slot() + GAIN_DETECT_PEAK_BINS;
    let next_prev_level_a = f32::from_bits(writeback.prev_level_a_bits());
    let next_prev_level_b = f32::from_bits(writeback.prev_level_b_bits());

    // Counts double-buffer: 0x3248 (group-1 count) delays into 0x3244, and the
    // fresh 0x3248 resets to 0. So next call's group-0 seed count = this call's
    // group-1 count; group-1 seed count = 0.
    let group1_count = post_seed.counts.get(1).copied().unwrap_or(0);
    let next_counts = vec![group1_count, 0];

    // gc output slab preseed: group-0 region <- this call's group-1 region
    // (the stored pool second half), group-1 region <- fresh zeros.
    let slab_words = GC_SET_POINTS_OUTPUT_GROUPS * GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS;
    let mut next_output_records = vec![0u32; slab_words];
    next_output_records[..GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS].copy_from_slice(
        &post_seed.output_records[GC_SET_POINTS_OUTPUT_GROUP_STRIDE_WORDS..slab_words],
    );

    // list A (persistent pool) <- this call's merged forward pool (group 0).
    let next_persistent = outcome.next_pool_records.clone();

    // current_bin0_peak <- next call's fresh front bin-0 peak (native
    // `local_9c[0]`, the level-b numerator). Recompute it from the fresh window
    // so it matches the front the next `gain_detect_band_at5` computes.
    let next_current_bin0_peak = if next_band_window.len() >= GAIN_DETECT_BAND_WINDOW_VALUES {
        gain_detect_peak_bins_at5(&next_band_window[GAIN_DETECT_BAND_WINDOW_PEAK_OFFSET..])
            .map(|peaks| peaks.bins()[0])
            .map_err(Time2FreqError::from)?
    } else {
        0.0
    };

    Ok(Time2FreqDetectorBandSeed {
        band_window: next_band_window,
        spectrum: next_spectrum,
        envelope: next_envelope,
        prev_max_slot: next_prev_max_slot,
        prev_peak_slot_plus_32: next_prev_peak_slot_plus_32,
        prev_level_a: next_prev_level_a,
        prev_level_b: next_prev_level_b,
        stored_peak_a: next_stored_peak_a,
        current_bin0_peak: next_current_bin0_peak,
        // Removed-count aging (`local_55c[2]` -> state `0x32c8` -> next call's
        // `local_55c[1]`, decompile 32518 load 31509): the gate removal seed for
        // the next call is this call's pool-2 removal count. Zero unless the
        // over-seven prune removal branch flagged a pool-2 partner. Falsifies the
        // exercises pool-2 partner removals (docs/12 §2.2).
        carried_removed_count: outcome.prune_pool2_removed_count,
        persistent_records: next_persistent,
        output_records: next_output_records,
        counts: next_counts,
    })
}

/// Owned per-channel input state for the `time2freq_at5` driver. This is a
/// (256 rolled slot-0..1 samples each, as `at5enc_sigproc` hands them over),
/// the previous-frame point records (attack side of the gain window), the
/// per-band detector seed surfaces, and the tonality pre-pass gate flag
/// (channel word `+0x1dc` bit 4).
///
/// There is no separate delayed-input surface: the delayed (`spec_a`) pass
/// copies the current call's main spectrum when its records are empty (see
/// `time2freq_delayed_pass_at5`).
#[derive(Debug, Clone)]
pub struct Time2FreqChannelState {
    pub band_inputs: Vec<Vec<f32>>,
    pub previous_records: Vec<[i32; TIME2FREQ_POINT_WORDS]>,
    pub detector_seeds: Vec<Time2FreqDetectorBandSeed>,
    pub prepass_disabled: bool,
}

/// Frame-level parameters for `time2freq_at5`, matching the native ABI
/// arguments pinned by `time2freq_trace.ndjson` (352 path:
/// `channel_count = 2`, `param6 = 2`, `bandwidth = 30`, `band_limit = 16`,
/// `mode_cc = 1`, detector gate open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time2FreqParams {
    pub channel_count: usize,
    /// `param_6`: stereo mode selector. Value `3` enables the tonal-flag
    /// reconciliation and the post-detector harmonization blocks. The 352
    /// target passes `2`, so both are skipped.
    pub param6: i32,
    /// `param_7`: mode bandwidth (`> 0x12` selects the flat `096` tonality
    /// thresholds; `>= 0x18` disables the stereo harmonization block).
    pub bandwidth: i32,
    /// `param_8`: active band count (16 for the 352 target).
    pub band_limit: usize,
    /// Channel word `+0x4` field `+0xcc`. Nonzero (352: `1`) dispatches
    /// `detect_gainc_data_new_at5`; zero would take the unported
    /// `set_gainc_at5` descending path.
    pub mode_cc: i32,
    /// Channel word `+0x30` field `+0x14 == 0` (352: open) enables the
    /// detector dispatch.
    pub detector_gate_open: bool,
}

/// Decision and spectrum surfaces produced for one channel by
/// `time2freq_at5`.
#[derive(Debug, Clone)]
pub struct Time2FreqChannelOutput {
    /// `band_limit * 128` main MDCT spectrum floats handed to
    /// `init_channel_block_at5`.
    pub spectra: Vec<f32>,
    /// `band_limit * 128` delayed/lookahead MDCT floats.
    pub delayed_out: Vec<f32>,
    /// Final 15-word point-prefix records (detector output after the band-0
    /// injection and adjacent-band merge). These are the gain side-data
    /// decision surface.
    pub final_records: Vec<[i32; TIME2FREQ_POINT_WORDS]>,
    /// Tonality pre-pass result (flags forced to zero on the 352
    /// `mode_cc = 1` path).
    pub tonality: TonalityChannel,
    /// Per-band emit outcomes (main-window applied, delayed copy/transform).
    pub band_outcomes: Vec<Time2FreqBandOutcome>,
    /// Per-band detector outcomes (fresh front peaks/weights + merged forward
    /// pool). Empty when the detector gate is closed. Consumed by the
    /// cross-frame seed evolution (`time2freq_detector_seed_evolve_at5`).
    pub detector_outcomes: Vec<GainDetectBandOutcome>,
    /// mode_cc==0 ONLY: the full 16x38-word post-writeback `cur_plane` rows
    /// (native `*(chobj+0x8)` content). At mode_cc==0 `detector_outcomes` is
    /// empty (the shared-state `set_gainc_at5` dispatch produces plane rows, not
    /// per-band detector outcomes), so the gain-record bridge has no 15-word
    /// prefix source to fold; it consumes these whole rows instead — carrying
    /// the live word-15 tonality flag and float tail words that the 15-word
    /// `final_records` prefix drops. `None` on the mode_cc==1 detector path
    /// (rates 96..352), where `assemble_gain_a_records` keeps its detector-gated
    /// prefix behavior byte-for-byte.
    pub final_plane_rows: Option<Vec<SetGaincRow>>,
}

/// Top-level `time2freq_at5` driver (native `0x3c480`, Ghidra `0x4c480`;
/// decompile `32593..34336`). Composes, in pinned native order over owned
/// per-channel state:
///
/// 1. tonality pre-pass per channel (decompile `32774`,
///    `time2freq_tonality_channel_at5`); on the 352 path `mode_cc = 1`
///    forces every tonal flag to zero;
/// 2. stereo tonal-flag reconciliation — gated on `param6 == 3` (decompile
///    `32952`), skipped for the 352 target (`param6 = 2`);
/// 3. detector dispatch — gated on the detector word (decompile `33011`)
///    and `mode_cc` (decompile `33012`): `mode_cc != 0` runs the composed
///    `gain_detect_band_at5` chain per band; `mode_cc == 0` would take the
///    unported `set_gainc_at5` path;
/// 4. the stereo gain-record harmonization block (decompile `33042`) is
///    gated on `param7 < 0x18 && param6 == 3 && mode_cc == 0` — skipped for
///    the 352 target (`param7 = 30`, `param6 = 2`, `mode_cc = 1`);
/// 5. the post-detector gain-record harmonization block (band-0 injection
///    plus adjacent-band merge) is separately gated by channel count and
///    bandwidth (`param_7 < 0x10 && param_5 == 1` or
///    `param_7 < 0x14 && param_5 == 2`, with side `+0x1c != 2`); it is
///    skipped for the 352 target (`param_5 = 2`, `param_7 = 30`);
/// 6. the always-running per-band main MDCT and delayed pass write the
///    spectrum and delayed buffer.
///
/// The band-0 injection's stereo fallback consults the other channel's
/// band-1 record; the driver snapshots both channels' detector-output band-1
/// records before running either channel's emit so the cross-channel read is
/// order-independent when the block is open. `Time2FreqParams` does not yet
/// expose the side `+0x1c` field; the current 352 path closes the block by
/// bandwidth, so the side value is irrelevant for this target.
pub fn time2freq_at5(
    channels: &mut [Time2FreqChannelState],
    params: &Time2FreqParams,
) -> Result<Vec<Time2FreqChannelOutput>, Time2FreqError> {
    time2freq_at5_impl(channels, params, None, None)
}

/// Encoder-facing detector path that evolves seeds in place and omits the
/// allocation-heavy per-band diagnostic outcomes.
pub(crate) fn time2freq_encode_at5(
    channels: &mut [Time2FreqChannelState],
    params: &Time2FreqParams,
    scratch: &mut GainDetectScratch,
) -> Result<Vec<Time2FreqChannelOutput>, Time2FreqError> {
    time2freq_at5_impl(channels, params, None, Some(scratch))
}

/// Like [`time2freq_at5`] but with the mode_cc==0 (64/48 kbps) `set_gainc_at5`
/// dispatch wired live: `set_gainc.detector_words` is the shared detector arena
/// (read for energy_84/rolled2, read+written for the identity flags) and
/// `set_gainc.channels[ch]` carry the per-channel scratch / persistent history /
/// prev+cur planes. On the `mode_cc == 0` path this replaces the whole
/// `detect_gainc_data_new_at5` chain with the descending per-(band, channel)
/// leaf dispatch, the ch1 stereo pre-adjust (decompile 33013-33026), and the
/// 33042 record-harmonization block; the produced current planes' 15-word point
/// prefixes feed the shared stage-5 emit, and `set_gainc.channels[ch].cur_plane`
/// carries the post-stage-5 plane the caller persists as next frame's
/// `prev_plane`.
pub fn time2freq_at5_with_set_gainc(
    channels: &mut [Time2FreqChannelState],
    params: &Time2FreqParams,
    set_gainc: &mut Time2FreqSetGaincState,
) -> Result<Vec<Time2FreqChannelOutput>, Time2FreqError> {
    time2freq_at5_impl(channels, params, Some(set_gainc), None)
}

/// Seed a fresh current gain-record plane from the tonality pre-pass, matching
/// the native tonality writes into the zeroed cur plane (decompile 32794/32841/
/// 32863): word 15 (`+0x3c`) = tonal flag, word 21 (`+0x54`) = tonality scale,
/// word 25 (`+0x64`) = tonality value. On the prepass-disabled path native
/// leaves word 25 untouched (zero), unlike the enabled path's 1.0 default.
fn seed_set_gainc_cur_plane(
    plane: &mut SetGaincPlane,
    tonality: &TonalityChannel,
    prepass_disabled: bool,
) {
    for (band, row) in plane.iter_mut().enumerate() {
        *row = [0u32; crate::dsp::set_gainc::SET_GAINC_ROW_WORDS];
        row[0x3c / 4] = u32::from(tonality.flags[band]);
        row[0x54 / 4] = tonality.scales[band].to_bits();
        row[0x64 / 4] = if prepass_disabled {
            0
        } else {
            tonality.tonality[band].to_bits()
        };
    }
}

/// The 15-word point-prefix records (words 0..15) of every band row of a
/// gain-record plane. Sized to `max(16, band_limit)` to match the detector
/// path's record vector.
fn plane_point_prefixes(
    plane: &SetGaincPlane,
    band_limit: usize,
) -> Vec<[i32; TIME2FREQ_POINT_WORDS]> {
    let count = TIME2FREQ_BANDS_AT5.max(band_limit);
    (0..count)
        .map(|band| {
            let mut record = [0i32; TIME2FREQ_POINT_WORDS];
            if band < TIME2FREQ_BANDS_AT5 {
                for (word, slot) in record.iter_mut().enumerate() {
                    *slot = plane[band][word] as i32;
                }
            }
            record
        })
        .collect()
}

/// Native mode_cc==0 descending `set_gainc_at5` dispatch (decompile 33013-33034)
/// plus the ch1 stereo pre-adjust (33013-33026): outer band 15..0, inner channel
/// 0..cc. The pre-adjust mutates ch1's prev-plane word 18 in place before ch1's
/// leaf call; the gate mean `fVar9/param_8` is modeled in f64 (a content-level
/// gate, per the (kkk) x87 precedent for register-resident accumulations).
fn run_set_gainc_dispatch(
    set_gainc: &mut Time2FreqSetGaincState,
    params: &Time2FreqParams,
    channel_count: usize,
) -> Result<(), Time2FreqError> {
    let band_limit = params.band_limit;
    // fVar9 = f32 sum of energy_84[0..param_8]; the divide + compare are x87
    // register-resident and content-level, modeled in f64.
    let sum: f64 = (0..band_limit)
        .map(|b| f64::from(detector_energy84(set_gainc.detector_words, b)))
        .sum();
    let mean = if band_limit > 0 {
        sum / band_limit as f64
    } else {
        0.0
    };
    let selector = params.bandwidth;
    let band_count = band_limit as i32;
    let header_1c = set_gainc.header_1c;

    for band in (0..TIME2FREQ_BANDS_AT5).rev() {
        for ch in 0..channel_count {
            if ch == 1 {
                let energy = detector_energy84(set_gainc.detector_words, band);
                if 30.0 < energy && 30.0 < mean {
                    let (left, right) = set_gainc.channels.split_at_mut(1);
                    let a_bits = left[0].prev_plane[band][0x48 / 4];
                    let a = f32::from_bits(a_bits);
                    let ratio = a / f32::from_bits(right[0].prev_plane[band][0x48 / 4]);
                    if 0.5 < ratio && ratio < 2.0 {
                        right[0].prev_plane[band][0x48 / 4] = a_bits;
                    }
                }
            }
            let channel = &mut set_gainc.channels[ch];
            set_gainc_at5(
                band,
                band_count,
                selector,
                channel_count as i32,
                header_1c,
                &channel.scratch[band],
                &mut channel.history_a[band],
                &mut channel.history_b[band],
                &channel.prev_plane,
                &mut channel.cur_plane,
            )?;
        }
    }
    Ok(())
}

fn time2freq_at5_impl(
    channels: &mut [Time2FreqChannelState],
    params: &Time2FreqParams,
    mut set_gainc: Option<&mut Time2FreqSetGaincState>,
    mut gain_scratch: Option<&mut GainDetectScratch>,
) -> Result<Vec<Time2FreqChannelOutput>, Time2FreqError> {
    let cc = params.channel_count;
    if channels.len() < cc {
        return Err(Time2FreqError::ChannelStateTooShort {
            needed: cc,
            actual: channels.len(),
        });
    }

    // Stage 1: tonality pre-pass per channel.
    let mode_cc_nonzero = params.mode_cc != 0;
    let prepass_disabled: Vec<bool> = channels
        .iter()
        .take(cc)
        .map(|c| c.prepass_disabled)
        .collect();
    let mut tonality_channels = Vec::with_capacity(cc);
    for channel in channels.iter().take(cc) {
        let band_inputs: Vec<&[f32]> = channel.band_inputs.iter().map(Vec::as_slice).collect();
        tonality_channels.push(time2freq_tonality_channel_at5(
            &band_inputs,
            params.bandwidth,
            mode_cc_nonzero,
            channel.prepass_disabled,
        )?);
    }

    // Stage 2: stereo tonal-flag reconciliation, gated on `param6 == 3`
    // (decompile 32952). On the mode_cc != 0 path (96-352) every tonal flag is
    // already false, so the reconcile is a guaranteed flag-surface no-op and
    // the correlation/copy_start/direction inputs are irrelevant (pass zeros);
    // on the mode_cc == 0 path (64/48) the live inputs come from the detector
    // arena + cfg+0x50 direction words carried in `set_gainc`.
    if params.param6 == 3 && cc == 2 {
        let mut flags = [tonality_channels[0].flags, tonality_channels[1].flags];
        let tonality = [tonality_channels[0].tonality, tonality_channels[1].tonality];
        let (correlation_db, copy_start, direction_words): (
            Vec<f32>,
            usize,
            [u32; TIME2FREQ_BANDS_AT5],
        ) = match set_gainc.as_deref() {
            Some(sg) => (
                (0..TIME2FREQ_BANDS_AT5)
                    .map(|b| detector_energy84(sg.detector_words, b))
                    .collect(),
                sg.detector_words[0] as usize,
                sg.direction_words,
            ),
            None => (
                vec![0.0f32; TIME2FREQ_BANDS_AT5],
                0,
                [0u32; TIME2FREQ_BANDS_AT5],
            ),
        };
        time2freq_stereo_flag_reconcile_at5(
            &mut flags,
            &tonality,
            &correlation_db,
            copy_start,
            &direction_words,
        )?;
        tonality_channels[0].flags = flags[0];
        tonality_channels[1].flags = flags[1];
    }

    // Stage 3: detector dispatch. `mode_cc != 0` runs the composed
    // `detect_gainc_data_new_at5` chain per channel; `mode_cc == 0` runs the
    // cross-channel `set_gainc_at5` dispatch + 33042 harmonization over the
    // shared `set_gainc` state.
    let mut current_records_per_channel: Vec<Vec<[i32; TIME2FREQ_POINT_WORDS]>>;
    let mut detector_outcomes_per_channel: Vec<Vec<GainDetectBandOutcome>>;
    let mut previous_records_per_channel: Vec<Vec<[i32; TIME2FREQ_POINT_WORDS]>>;
    let mut tonal_window_flags_per_channel = vec![vec![(false, false); params.band_limit]; cc];

    if params.detector_gate_open && params.mode_cc == 0 {
        let sg = set_gainc
            .as_deref_mut()
            .ok_or(Time2FreqError::UnportedSetGaincDispatch)?;
        // Seed the current planes from the (post-reconcile) tonality pre-pass.
        for (ch, tonality) in tonality_channels.iter().take(cc).enumerate() {
            seed_set_gainc_cur_plane(
                &mut sg.channels[ch].cur_plane,
                tonality,
                prepass_disabled[ch],
            );
        }
        run_set_gainc_dispatch(sg, params, cc)?;
        for (ch, channel) in sg.channels.iter().take(cc).enumerate() {
            for band in 0..params.band_limit.min(TIME2FREQ_BANDS_AT5) {
                tonal_window_flags_per_channel[ch][band] = (
                    channel.cur_plane[band][0x3c / 4] != 0,
                    channel.prev_plane[band][0x3c / 4] != 0,
                );
            }
        }
        // 33042 record-harmonization block (`param_7 < 0x18 && param_6 == 3`).
        if params.bandwidth < 0x18 && params.param6 == 3 && cc == 2 {
            let (left, right) = sg.channels.split_at_mut(1);
            time2freq_mode_cc0_record_harmonization_at5(
                &mut left[0].cur_plane,
                &mut right[0].cur_plane,
                &left[0].prev_plane,
                &right[0].prev_plane,
                sg.detector_words,
                params.bandwidth,
            );
        }
        current_records_per_channel = sg
            .channels
            .iter()
            .take(cc)
            .map(|c| plane_point_prefixes(&c.cur_plane, params.band_limit))
            .collect();
        // The stage-5 gain-window/injection previous records are the prev-plane
        // point prefixes (native reads the same plane rows).
        previous_records_per_channel = sg
            .channels
            .iter()
            .take(cc)
            .map(|c| plane_point_prefixes(&c.prev_plane, params.band_limit))
            .collect();
        detector_outcomes_per_channel = (0..cc).map(|_| Vec::new()).collect();
    } else {
        current_records_per_channel = Vec::with_capacity(cc);
        detector_outcomes_per_channel = Vec::with_capacity(cc);
        previous_records_per_channel = Vec::with_capacity(cc);
        for (channel_index, channel) in channels.iter_mut().take(cc).enumerate() {
            for band in 0..params.band_limit.min(TIME2FREQ_BANDS_AT5) {
                tonal_window_flags_per_channel[channel_index][band].0 =
                    tonality_channels[channel_index].flags[band];
            }
            if params.detector_gate_open {
                if let Some(scratch) = gain_scratch.as_deref_mut() {
                    let (records, prune_markers) = time2freq_channel_detect_lean_at5(
                        &mut channel.detector_seeds,
                        params.band_limit,
                        scratch,
                    )?;
                    current_records_per_channel.push(records);
                    detector_outcomes_per_channel.push(prune_markers);
                } else {
                    let (records, outcomes) = time2freq_channel_detect_at5(
                        &mut channel.detector_seeds,
                        params.band_limit,
                    )?;
                    current_records_per_channel.push(records);
                    detector_outcomes_per_channel.push(outcomes);
                }
            } else {
                current_records_per_channel.push(vec![
                    [0i32; TIME2FREQ_POINT_WORDS];
                    TIME2FREQ_BANDS_AT5.max(params.band_limit)
                ]);
                detector_outcomes_per_channel.push(Vec::new());
            }
            previous_records_per_channel.push(channel.previous_records.clone());
        }
    }

    // Stage 5: post-detector emit per channel.
    let record_harmonization_open =
        time2freq_post_detector_record_harmonization_gate_at5(0, cc, params.bandwidth);

    // Cross-channel gain-record harmonization (native `time2freq_at5` decompile
    // `33193..33404`) runs inside the open gate for stereo only, mutating both
    // channels' records in place BEFORE the per-channel band-0 injection and
    // adjacent-band merge. The band-1 snapshot below is taken AFTER this block,
    // matching native's live `local_2f5c[other]+0x98` read.
    if record_harmonization_open && cc == 2 {
        let (head, tail) = current_records_per_channel.split_at_mut(1);
        time2freq_cross_channel_record_harmonization_at5(
            head[0].as_mut_slice(),
            tail[0].as_mut_slice(),
            params.band_limit,
        )?;
    }

    let band1_snapshots: Vec<[i32; TIME2FREQ_POINT_WORDS]> = current_records_per_channel
        .iter()
        .map(|records| records[1])
        .collect();

    if record_harmonization_open {
        for channel_index in 0..cc {
            let other_channel_band1 = if cc > 1 {
                Some(&band1_snapshots[1 - channel_index])
            } else {
                None
            };
            time2freq_band0_attack_injection_at5(
                &mut current_records_per_channel[channel_index],
                other_channel_band1,
                params.band_limit,
            )?;
            time2freq_adjacent_band_merge_at5(
                &mut current_records_per_channel[channel_index],
                params.band_limit,
            )?;
        }
    }

    // Native scans/equalizes the gain-window peaks and corrects overshoot after
    // record harmonization but before either MDCT loop. This stage is usually
    // dormant at the higher rates; it is decision-live on 64/48 tonal
    time2freq_record_overshoot_stage_at5(
        &channels[..cc],
        &previous_records_per_channel,
        &mut current_records_per_channel,
        params.band_limit,
    )?;

    let mut outputs = Vec::with_capacity(cc);
    for channel_index in 0..cc {
        let channel = &channels[channel_index];
        let band_inputs: Vec<&[f32]> = channel.band_inputs.iter().map(Vec::as_slice).collect();
        let other_channel_band1 = if cc > 1 {
            Some(&band1_snapshots[1 - channel_index])
        } else {
            None
        };

        let mut spectra = vec![0.0f32; params.band_limit * MDCT_128_OUTPUT_COUNT];
        let mut delayed_out = vec![0.0f32; params.band_limit * MDCT_128_OUTPUT_COUNT];
        let band_outcomes = time2freq_post_detector_channel_impl_at5(
            &band_inputs,
            &mut current_records_per_channel[channel_index],
            &previous_records_per_channel[channel_index],
            other_channel_band1,
            params.band_limit,
            false,
            &tonal_window_flags_per_channel[channel_index],
            &mut spectra,
            &mut delayed_out,
        )?;

        outputs.push(Time2FreqChannelOutput {
            spectra,
            delayed_out,
            final_records: current_records_per_channel[channel_index].clone(),
            tonality: tonality_channels[channel_index].clone(),
            band_outcomes,
            detector_outcomes: std::mem::take(&mut detector_outcomes_per_channel[channel_index]),
            final_plane_rows: None,
        });
    }

    // On the mode_cc == 0 path, write the post-stage-5 15-word prefixes back into
    // the current planes (words 0..15) so the persisted `prev_plane` carries them
    // to next frame (native works on the same plane rows throughout).
    if params.detector_gate_open && params.mode_cc == 0 {
        if let Some(sg) = set_gainc.as_deref_mut() {
            for (ch, records) in current_records_per_channel.iter().take(cc).enumerate() {
                for (band, record) in records.iter().take(TIME2FREQ_BANDS_AT5).enumerate() {
                    for (word, &value) in record.iter().enumerate() {
                        sg.channels[ch].cur_plane[band][word] = value as u32;
                    }
                }
            }
            // Post-fill each channel output with the full post-writeback plane
            // rows (native `*(chobj+0x8)` content: 16x38 words, live word-15 flag
            // and float tail). The gain-record bridge carries these whole; the
            // 15-word `final_records` prefix drops the tail and is used only by
            // the mode_cc==1 detector path.
            for (ch, output) in outputs.iter_mut().take(cc).enumerate() {
                output.final_plane_rows = Some(sg.channels[ch].cur_plane.to_vec());
            }
        }
    }

    Ok(outputs)
}
