use crate::dsp::scalar::{ScalarError, sub_seq_at5};
use crate::tables::at5::{NSPS_AT5_ENTRIES, SFTBL_AT5_ENTRIES, isps_at5, nsps_at5, sftbl_at5};

pub const NORMALIZED_MDSPEC_CLAMP_BITS: u32 = 0x3f8f_9e00;
pub const NORMALIZED_MDSPEC_CLAMP: f32 = f32::from_bits(NORMALIZED_MDSPEC_CLAMP_BITS);
pub const NORM_CHANNEL_BLOCK_MODE_352: u32 = 2;
const IDSF_SCALE_TABLE_INDEX: usize = 15;

pub struct NormChannelBlockAt5<'a> {
    pub mdspec: &'a mut [f32],
    pub scale_factors: &'a [u32],
    pub mdspec_average: &'a [f32],
    pub normalized_mdspec_average: &'a mut [f32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizeError {
    UnsupportedNormMode {
        mode: u32,
        supported: u32,
    },
    QuantUnitCountTooLarge {
        count: usize,
        max: usize,
    },
    SpectrumTooShort {
        needed: usize,
        actual: usize,
    },
    ScaleFactorsTooShort {
        needed: usize,
        actual: usize,
    },
    ScaleFactorIndexOutOfRange {
        unit: usize,
        index: usize,
        max: usize,
    },
    BandInputTooShort {
        needed: usize,
        actual: usize,
    },
    BandOutputTooShort {
        needed: usize,
        actual: usize,
    },
    /// The channel-difference subtraction (`sub_seq_at5`) rejected its inputs
    /// (e.g. a channel spectrum shorter than `g_a_isps_at5[quant_unit_count]`
    /// difference lines). Wraps the underlying `ScalarError` verbatim.
    DifferenceSpectrum(ScalarError),
}

pub fn normalize_mdspec_at5(
    mdspec: &mut [f32],
    scale_factors: &[u32],
    quant_unit_count: usize,
) -> Result<(), NormalizeError> {
    if quant_unit_count > NSPS_AT5_ENTRIES {
        return Err(NormalizeError::QuantUnitCountTooLarge {
            count: quant_unit_count,
            max: NSPS_AT5_ENTRIES,
        });
    }
    if scale_factors.len() < quant_unit_count {
        return Err(NormalizeError::ScaleFactorsTooShort {
            needed: quant_unit_count,
            actual: scale_factors.len(),
        });
    }

    let isps = isps_at5();
    let needed_samples = usize::from(isps[quant_unit_count]);
    if mdspec.len() < needed_samples {
        return Err(NormalizeError::SpectrumTooShort {
            needed: needed_samples,
            actual: mdspec.len(),
        });
    }

    let scale_table = sftbl_at5();
    for unit in 0..quant_unit_count {
        let scale_index = scale_factors[unit] as usize;
        if scale_index >= SFTBL_AT5_ENTRIES {
            return Err(NormalizeError::ScaleFactorIndexOutOfRange {
                unit,
                index: scale_index,
                max: SFTBL_AT5_ENTRIES - 1,
            });
        }

        let scale = 1.0_f64 / f64::from(scale_table[scale_index]);
        let start = usize::from(isps[unit]);
        let end = usize::from(isps[unit + 1]);
        for sample in &mut mdspec[start..end] {
            *sample = (f64::from(*sample) * scale) as f32;
        }
    }

    Ok(())
}

pub fn clip_normalized_mdspec_at5(
    mdspec: &mut [f32],
    scale_factors: &[u32],
    quant_unit_count: usize,
) -> Result<(), NormalizeError> {
    if quant_unit_count > NSPS_AT5_ENTRIES {
        return Err(NormalizeError::QuantUnitCountTooLarge {
            count: quant_unit_count,
            max: NSPS_AT5_ENTRIES,
        });
    }
    if scale_factors.len() < quant_unit_count {
        return Err(NormalizeError::ScaleFactorsTooShort {
            needed: quant_unit_count,
            actual: scale_factors.len(),
        });
    }

    let isps = isps_at5();
    let needed_samples = usize::from(isps[quant_unit_count]);
    if mdspec.len() < needed_samples {
        return Err(NormalizeError::SpectrumTooShort {
            needed: needed_samples,
            actual: mdspec.len(),
        });
    }

    for unit in 0..quant_unit_count {
        let scale_factor = scale_factors[unit] as usize;
        if scale_factor >= SFTBL_AT5_ENTRIES {
            return Err(NormalizeError::ScaleFactorIndexOutOfRange {
                unit,
                index: scale_factor,
                max: SFTBL_AT5_ENTRIES - 1,
            });
        }
        if scale_factor <= 0x3e {
            continue;
        }

        let start = usize::from(isps[unit]);
        let end = usize::from(isps[unit + 1]);
        for sample in &mut mdspec[start..end] {
            if *sample > NORMALIZED_MDSPEC_CLAMP {
                *sample = NORMALIZED_MDSPEC_CLAMP;
            } else if *sample < -NORMALIZED_MDSPEC_CLAMP {
                *sample = -NORMALIZED_MDSPEC_CLAMP;
            }
        }
    }

    Ok(())
}

pub fn normalize_mdspec_average_at5(
    dst: &mut [f32],
    src: &[f32],
    scale_factors: &[u32],
    quant_unit_count: usize,
) -> Result<(), NormalizeError> {
    if quant_unit_count > NSPS_AT5_ENTRIES {
        return Err(NormalizeError::QuantUnitCountTooLarge {
            count: quant_unit_count,
            max: NSPS_AT5_ENTRIES,
        });
    }
    if src.len() < quant_unit_count {
        return Err(NormalizeError::BandInputTooShort {
            needed: quant_unit_count,
            actual: src.len(),
        });
    }
    if dst.len() < quant_unit_count {
        return Err(NormalizeError::BandOutputTooShort {
            needed: quant_unit_count,
            actual: dst.len(),
        });
    }
    if scale_factors.len() < quant_unit_count {
        return Err(NormalizeError::ScaleFactorsTooShort {
            needed: quant_unit_count,
            actual: scale_factors.len(),
        });
    }

    let scale_table = sftbl_at5();
    for unit in 0..quant_unit_count {
        let scale_factor = scale_factors[unit] as usize;
        if scale_factor >= SFTBL_AT5_ENTRIES {
            return Err(NormalizeError::ScaleFactorIndexOutOfRange {
                unit,
                index: scale_factor,
                max: SFTBL_AT5_ENTRIES - 1,
            });
        }

        dst[unit] = (f64::from(src[unit]) / f64::from(scale_table[scale_factor])) as f32;
    }

    Ok(())
}

pub fn norm_channel_block_at5(
    channels: &mut [NormChannelBlockAt5<'_>],
    quant_unit_count: usize,
    mode: u32,
) -> Result<(), NormalizeError> {
    if mode != NORM_CHANNEL_BLOCK_MODE_352 {
        return Err(NormalizeError::UnsupportedNormMode {
            mode,
            supported: NORM_CHANNEL_BLOCK_MODE_352,
        });
    }

    for channel in channels {
        normalize_mdspec_at5(channel.mdspec, channel.scale_factors, quant_unit_count)?;
        clip_normalized_mdspec_at5(channel.mdspec, channel.scale_factors, quant_unit_count)?;
        normalize_mdspec_average_at5(
            channel.normalized_mdspec_average,
            channel.mdspec_average,
            channel.scale_factors,
            quant_unit_count,
        )?;
    }

    Ok(())
}

pub fn set_idsf_from_mdspec_at5(
    mdspec: &[f32],
    scale_factors: &mut [u32],
    band_max: &mut [f32],
    quant_unit_count: usize,
) -> Result<(), NormalizeError> {
    if quant_unit_count > NSPS_AT5_ENTRIES {
        return Err(NormalizeError::QuantUnitCountTooLarge {
            count: quant_unit_count,
            max: NSPS_AT5_ENTRIES,
        });
    }
    if scale_factors.len() < quant_unit_count {
        return Err(NormalizeError::ScaleFactorsTooShort {
            needed: quant_unit_count,
            actual: scale_factors.len(),
        });
    }
    if band_max.len() < quant_unit_count {
        return Err(NormalizeError::BandOutputTooShort {
            needed: quant_unit_count,
            actual: band_max.len(),
        });
    }

    let isps = isps_at5();
    let needed_samples = usize::from(isps[quant_unit_count]);
    if mdspec.len() < needed_samples {
        return Err(NormalizeError::SpectrumTooShort {
            needed: needed_samples,
            actual: mdspec.len(),
        });
    }

    let nsps = nsps_at5();
    let scale_table = sftbl_at5();
    for unit in 0..quant_unit_count {
        let start = usize::from(isps[unit]);
        let sample_count = usize::from(nsps[unit]);
        let mut max = (f64::from(mdspec[start]).abs()) as f32;
        for sample in &mdspec[start + 1..start + sample_count] {
            let candidate = (f64::from(*sample).abs()) as f32;
            if max < candidate {
                max = candidate;
            }
        }

        band_max[unit] = max;
        let scaled = f64::from(max) * f64::from(scale_table[IDSF_SCALE_TABLE_INDEX]);
        let scale_factor = if scaled >= f64::from(scale_table[0]) {
            search_idsf_scale_factor_at5(&scale_table, scaled)
        } else {
            0
        };
        scale_factors[unit] = scale_factor;
    }

    Ok(())
}

/// Native `norm_channel_block_at5` (native offset `0x0000bef0`; decompile lines
/// ~4762-4785), the `param_5 == 3` branch body (decompile lines 4770-4773):
///
/// ```c
/// if (param_5 == 3) {
///     sub_seq_at5(*param_2, param_2[1], diff, g_a_isps_at5[uc]); // diff = ch0 - ch1 over isps[uc] lines
///     set_idsf_from_mdspec_at5(diff, side+4, band_max, uc);      // side+4 = per-unit IDSF of the difference
/// }
/// ```
///
/// Computes the per-unit IDSF scale-factor row of the channel-difference (side)
/// spectrum `ch0_mdspec - ch1_mdspec`: it subtracts the two channel MDCT spectra
/// over `g_a_isps_at5[quant_unit_count]` difference lines (`sub_seq_at5`, native
/// `0x0000fef0`), then runs `set_idsf_from_mdspec_at5` (native `0x00039950`) on
/// the difference, in the exact composition order of decompile 4770-4773. Returns
/// `(side+4 scale factors, band_max)` — native `side+4` is the shared side object
/// `*(block[0]+0x1008) + 4`; `band_max` is the per-unit band-maximum scratch.
///
/// `param_5 = mode_a` is the native `g_a_encode_setting_atx` row+0x14 config word:
/// `3` for 48-256 kbps (this branch LIVE) and `2` for 320/352 kbps (this branch
/// DEAD), which is why the shipped 352 path never needed it (oracle `gate_proof`:
/// 84 hits at 256, 0 at 352).
///
/// UNWIRED: this is a pure leaf, not composed into the pipeline yet (like the (u)
/// `zeroth_joint_stereo_producer_at5` leaf). It reuses the already-ported,
/// native-verified `sub_seq_at5` and `set_idsf_from_mdspec_at5` leaves verbatim.
pub fn norm_channel_difference_idsf_at5(
    ch0_mdspec: &[f32],
    ch1_mdspec: &[f32],
    quant_unit_count: usize,
) -> Result<(Vec<u32>, Vec<f32>), NormalizeError> {
    if quant_unit_count > NSPS_AT5_ENTRIES {
        return Err(NormalizeError::QuantUnitCountTooLarge {
            count: quant_unit_count,
            max: NSPS_AT5_ENTRIES,
        });
    }

    // g_a_isps_at5[quant_unit_count] difference lines (== 2048 for 32 units).
    // The quant_unit_count bound above keeps this index inside isps_at5().
    let n_lines = usize::from(isps_at5()[quant_unit_count]);

    let mut diff = vec![0.0f32; n_lines];
    sub_seq_at5(ch0_mdspec, ch1_mdspec, &mut diff, n_lines)
        .map_err(NormalizeError::DifferenceSpectrum)?;

    let mut side4 = vec![0u32; quant_unit_count];
    let mut band_max = vec![0.0f32; quant_unit_count];
    set_idsf_from_mdspec_at5(&diff, &mut side4, &mut band_max, quant_unit_count)?;

    Ok((side4, band_max))
}

fn search_idsf_scale_factor_at5(scale_table: &[f32; SFTBL_AT5_ENTRIES], scaled: f64) -> u32 {
    let mut index = 0x20_i32;
    let mut step = 0x10_i32;

    loop {
        let half_step = step >> 1;
        if f64::from(scale_table[index as usize]) <= scaled {
            index += step;
        } else {
            index -= step;
        }
        step = half_step;
        if step == 0 {
            break;
        }
    }

    if index < 0x3f && f64::from(scale_table[index as usize]) <= scaled {
        index += 1;
    }
    index as u32
}
