//! Direct coding-output -> typed frame-syntax bridge.
//!
//! This is the production handoff between the encoder decisions and the
//! bitstream writer.  It deliberately contains no native object offsets and
//! creates no synthetic object-memory windows; the old window serializers in
//! `reference::native_layout` remains the differential oracle only.

use crate::coding::allocation::{
    ZerothGainLevelBand, ZerothGainLocationBand, zeroth_activity_summary_at5,
    zeroth_band_shape_counts_at5, zeroth_gain_idlev_mode_at5, zeroth_gain_idlev_mode_ch1_at5,
    zeroth_gain_idloc_mode_at5, zeroth_gain_idloc_mode_ch1_at5, zeroth_gain_ngc_mode_at5,
};
use crate::coding::bitcount::{IdctBlockState, IdsfBlockState, IdwlBlockState};
use crate::coding::calc_block::{CalcChannelOutput, CalcFrameOutput};
use crate::encoder::cfg_bridge::FrameConfig;
use crate::encoder::frame::{FrameError, ObjectInputs};
use crate::encoder::frontend::FrontendState;
use crate::encoder::packing_prep::{GhaBandData, GhaPackingPrep, PackingPrepError};
use crate::pipeline::syntax::{
    BlockGroupSyntax, BlockHeaderSyntax, ChannelSyntax, FrameSyntax, GainIdlevEncodingSyntax,
    GainIdlevSyntax, GainIdlocEncodingSyntax, GainIdlocSyntax, GainNgcEncodingSyntax,
    GainNgcSyntax, GainPayloadSyntax, GainRowSyntax, GainSyntax, GatedFlagsSyntax,
    GhaChannelSyntax, GhaFreqEncodingSyntax, GhaFreqSyntax, GhaIdsfEncodingSyntax, GhaIdsfSyntax,
    GhaNwavsEncodingSyntax, GhaNwavsSyntax, GhaPayloadSyntax, GhaRecordSyntax, GhaSyntax,
    GhaWaveSyntax, IdctCountSyntax, IdctEncodingSyntax, IdctRowSyntax, IdctSyntax,
    IdsfEncodingSyntax, IdsfSyntax, IdwlEncodingSyntax, IdwlSyntax, SpectralCodebookSyntax,
    SpectralSyntax, SpectralUnitSyntax, StereoSyntax, idspcqu_tail_count_at,
};
use crate::tables::at5::{isps_at5, nsps_at5};

#[derive(Debug, Clone)]
struct GainRowData {
    count: usize,
    locations: Vec<i32>,
    levels: Vec<i32>,
}

/// Build one complete, self-contained typed frame from encoder
/// decisions. `band_index`/`band_count` are the per-frame fallback shape words
/// used by the zeroth 29..=31 rounding law.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_frame_syntax(
    out: &CalcFrameOutput,
    per_frame: &FrameConfig,
    frontend: &FrontendState,
    prep: &GhaPackingPrep,
    objects: &[ObjectInputs],
    frame_bytes: usize,
    band_index: u32,
    band_count: u32,
) -> Result<FrameSyntax, FrameError> {
    let channel_count = objects.len();
    if channel_count == 0 || out.channels.len() < channel_count {
        return Err(FrameError::GhaChannelMissing { channel: 0 });
    }

    let active = per_frame.active_b0 as usize;
    let stereo_units = per_frame.level_groups_c0 as usize;
    let header = build_block_header(channel_count, active, stereo_units, band_index, band_count);

    let gain_rows = objects
        .iter()
        .map(|object| {
            gain_rows_from_records(
                &object.gain_a_records,
                object.init_header.obj_1b490.max(0) as usize,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let channels = (0..channel_count)
        .map(|channel| {
            build_channel_syntax(
                channel,
                &out.channels[channel],
                out,
                &objects[channel],
                &gain_rows,
                objects
                    .first()
                    .map(|object| object.gain_a_records.as_slice()),
                active,
                stereo_units,
                header.quant_header as usize,
                header.gainb_count,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let stereo = (channel_count == 2).then(|| StereoSyntax {
        secondary: gated_from_cfg_group(&per_frame.stereo_group1, stereo_units),
        primary: gated_from_cfg_group(&per_frame.stereo_group2, stereo_units),
    });
    let gha = build_gha_syntax(frontend, prep, channel_count)?;
    let group = BlockGroupSyntax::new(header, channels, stereo, None, gha);
    FrameSyntax::from_parts(frame_bytes, vec![group]).map_err(FrameError::from)
}

fn build_block_header(
    channel_count: usize,
    active: usize,
    stereo_units: usize,
    band_index: u32,
    band_count: u32,
) -> BlockHeaderSyntax {
    // Native derives cfg+0xc4/cfg+0xc8 from the effective cfg+0xb4/cfg+0xbc
    // extent independently of the returned per-frame active cfg+0xb0 count.
    // In particular, extent 29/13 rounds to header shape 32/16 even when the
    // current frame has fewer than 29 active quant units.
    let shape = zeroth_band_shape_counts_at5(
        band_index as usize,
        band_index as usize,
        band_count as usize,
    );
    BlockHeaderSyntax {
        channel_mode: u32::from(channel_count != 1),
        quant_header: shape.word_length_count as u32,
        header_flag: false,
        quant_unit_count: active,
        bandwidth_gate: true,
        stereo_unit_count: stereo_units,
        gainb_count: shape.group_count,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_channel_syntax(
    channel: usize,
    output: &CalcChannelOutput,
    frame: &CalcFrameOutput,
    object: &ObjectInputs,
    gain_rows: &[Vec<GainRowData>],
    previous_gain_records: Option<&[u32]>,
    active: usize,
    stereo_units: usize,
    config_count: usize,
    gainb_count: usize,
) -> Result<ChannelSyntax, FrameError> {
    Ok(ChannelSyntax {
        channel_index: channel as u32,
        // The native object points both channels at object 0. Typed payloads
        // carry every predictor explicitly, so this is identity metadata only.
        previous_channel: Some(0),
        idwl: build_idwl(channel, output, frame, config_count)?,
        idsf: build_idsf(channel, output, frame, active),
        idct: build_idct(channel, output, frame, active),
        spectral: build_spectral(output, active, stereo_units),
        gainb: gated_from_activity(&object.band_activity, gainb_count)?,
        gain: build_gain(channel, object, gain_rows, previous_gain_records)?,
    })
}

fn idwl_record(block: &IdwlBlockState) -> Result<[i32; 5], PackingPrepError> {
    match block.mode {
        0 => Ok(block.selector_fields_14_24),
        1 => Ok(block.selector_fields_28_38),
        2 => Ok(block.selector_fields_3c_4c),
        3 => Ok(block.selector_fields_50_60),
        mode => Err(PackingPrepError::IdwlModeOutOfRange { mode }),
    }
}

fn u32_row(values: &[i32], count: usize) -> Vec<u32> {
    values
        .iter()
        .take(count)
        .map(|value| *value as u32)
        .collect()
}

fn build_idwl(
    channel: usize,
    output: &CalcChannelOutput,
    frame: &CalcFrameOutput,
    config_count: usize,
) -> Result<IdwlSyntax, FrameError> {
    let block = &output.idwl_block;
    let mode = block.mode;
    let record = idwl_record(block)?;
    let selector_a = record[4];
    if !(0..4).contains(&selector_a) {
        return Err(PackingPrepError::IdwlSelectorOutOfRange {
            selector: selector_a,
        }
        .into());
    }
    let selector_b = record[1] as u32;
    let count = record[2] as u32 as usize;
    let mode3_value = record[3] as u32;
    let huffman_selector = record[0] as u32 as usize;
    let plane = &block.word_rows[selector_a as usize];
    let dispatch = (mode & 3) + (((channel as u32) & 1) << 2);
    let encoding = match dispatch {
        0 | 4 => IdwlEncodingSyntax::Raw {
            word_lengths: u32_row(&output.o_1b5f8, config_count),
        },
        1 => IdwlEncodingSyntax::Direct {
            selector_a: selector_a as u32,
            selector_b,
            count,
            mode3_value,
            prefix_count: frame.shared_wlc_window_fields[0] as u32 as usize,
            residual_bits: frame.shared_wlc_window_fields[1] as u8,
            residual_base: frame.shared_wlc_window_fields[2] as u32,
            values: u32_row(plane, count),
        },
        2 => {
            let side_selector = block.selector_fields_3c_4c[1];
            if !(0..4).contains(&side_selector) {
                return Err(PackingPrepError::IdwlSelectorOutOfRange {
                    selector: side_selector,
                }
                .into());
            }
            let symbols = &block.side.rows[side_selector as usize];
            let group_flags = (0..(count >> 1))
                .map(|group| u32::from(symbols[group * 2] == 0 && symbols[group * 2 + 1] == 0))
                .collect();
            IdwlEncodingSyntax::Grouped {
                selector_b,
                count,
                mode3_value,
                subgroup_flag: block.side.subgroup_flag as u32,
                huffman_selector,
                field_3bits: symbols[33] as u32,
                field_4bits: symbols[32] as u32,
                group_flags,
                symbols: u32_row(symbols, count),
            }
        }
        3 | 7 => IdwlEncodingSyntax::Delta {
            selector_a: selector_a as u32,
            selector_b,
            count,
            config_count,
            mode3_value,
            huffman_selector,
            values: u32_row(plane, config_count),
        },
        5 | 6 => IdwlEncodingSyntax::Previous {
            progressive: dispatch == 6,
            selector_b,
            count,
            config_count,
            mode3_value,
            huffman_selector,
            current_values: u32_row(&output.o_1b5f8, config_count),
            previous_values: u32_row(&frame.channels[0].o_1b5f8, config_count),
            tail_flags: u32_row(plane, config_count),
        },
        _ => unreachable!("masked IDWL dispatch"),
    };
    Ok(IdwlSyntax { mode, encoding })
}

fn idsf_values(block: &IdsfBlockState, selector: usize, count: usize) -> Vec<i32> {
    if selector == 3 {
        block.transformed[..count].to_vec()
    } else {
        block.shifted_rows[selector][..count].to_vec()
    }
}

fn build_idsf(
    channel: usize,
    output: &CalcChannelOutput,
    frame: &CalcFrameOutput,
    count: usize,
) -> IdsfSyntax {
    let Some(block) = output.idsf_block.as_ref() else {
        return IdsfSyntax {
            mode: 0,
            encoding: IdsfEncodingSyntax::Raw {
                values: u32_row(&output.o_1b678, count),
            },
        };
    };
    let mode = block.mode;
    let dispatch = (mode & 3) + (((channel as u32) & 1) << 2);
    let encoding = match dispatch {
        0 | 4 => IdsfEncodingSyntax::Raw {
            values: u32_row(&output.o_1b678, count),
        },
        1 => IdsfEncodingSyntax::Direct {
            mode_selector: block.mode_selector,
            field_a: block.compact_base as u32,
            field_b: block.codebook_selector as u32,
            prefix_count: block.start,
            residual_bits: block.count as u8,
            residual_base: block.field_0x1c748,
            count,
            values: idsf_values(block, block.mode_selector, count),
        },
        2 => IdsfEncodingSyntax::Grouped {
            huffman_selector: block.huffman_selector,
            field_a: block.compact_base as u32,
            field_b: block.codebook_selector as u32,
            count,
            symbols: u32_row(&block.transformed, count),
        },
        3 => IdsfEncodingSyntax::Delta {
            mode_selector: block.mode_selector,
            huffman_selector: block.huffman_selector,
            field_a: block.compact_base as u32,
            field_b: block.codebook_selector as u32,
            count,
            values: idsf_values(block, block.mode_selector, count),
        },
        5 | 6 => IdsfEncodingSyntax::Previous {
            progressive: dispatch == 6,
            huffman_selector: block.huffman_selector,
            count,
            current_values: u32_row(&output.o_1b678, count),
            previous_values: u32_row(&frame.channels[0].o_1b678, count),
        },
        7 => IdsfEncodingSyntax::Empty,
        _ => unreachable!("masked IDSF dispatch"),
    };
    IdsfSyntax { mode, encoding }
}

fn build_idct(
    channel: usize,
    output: &CalcChannelOutput,
    frame: &CalcFrameOutput,
    quant_units: usize,
) -> IdctSyntax {
    let block: &IdctBlockState = &output.idct_block;
    let count = if block.split_flag == 0 {
        IdctCountSyntax::FullBand(quant_units)
    } else {
        IdctCountSyntax::Explicit(block.band_count)
    };
    let active = count.active();
    let rows = (0..active)
        .map(|index| IdctRowSyntax {
            mode: block.flags[index],
            value: output.o_1b578[index] as u32,
        })
        .collect();
    let dispatch = (block.mode & 3) + (((channel as u32) & 1) << 2);
    let encoding = match dispatch {
        0 | 4 => IdctEncodingSyntax::Fixed,
        1 | 5 => IdctEncodingSyntax::Huffman,
        2 | 6 => IdctEncodingSyntax::Delta,
        3 => IdctEncodingSyntax::Empty,
        7 => IdctEncodingSyntax::Previous {
            values: u32_row(&frame.channels[0].o_1b578, active),
        },
        _ => unreachable!("masked IDCT dispatch"),
    };
    IdctSyntax {
        bandwidth: output.mode_1074 != 0,
        mode: block.mode,
        bandwidth_mode: 1,
        count,
        rows,
        encoding,
    }
}

fn build_spectral(
    output: &CalcChannelOutput,
    quant_units: usize,
    stereo_units: usize,
) -> SpectralSyntax {
    let nsps = nsps_at5();
    let isps = isps_at5();
    let units = (0..quant_units)
        .filter(|quant_unit| output.o_1b5f8[*quant_unit] > 0)
        .map(|quant_unit| {
            let sample_count = usize::from(nsps[quant_unit]);
            let start = usize::from(isps[quant_unit]);
            SpectralUnitSyntax {
                quant_unit,
                codebook: SpectralCodebookSyntax {
                    bandwidth: output.mode_1074 != 0,
                    selector: output.o_1b578[quant_unit] as usize,
                    word_length: output.o_1b5f8[quant_unit] as usize,
                },
                samples: output.o_1b6f8[start..start + sample_count]
                    .iter()
                    .map(|sample| *sample as u16)
                    .collect(),
            }
        })
        .collect();
    let tail_values = if quant_units <= 2 {
        Vec::new()
    } else {
        idspcqu_tail_count_at(stereo_units + 0x1f)
            .map(|count| {
                output.o_1c6f8[..count]
                    .iter()
                    .map(|value| *value as u8)
                    .collect()
            })
            .unwrap_or_default()
    };
    SpectralSyntax { units, tail_values }
}

fn gain_rows_from_records(records: &[u32], count: usize) -> Result<Vec<GainRowData>, FrameError> {
    let needed = count.saturating_mul(38);
    if records.len() < needed {
        return Err(PackingPrepError::GainRowCountExceedsMax {
            count: needed,
            max: records.len(),
        }
        .into());
    }
    (0..count)
        .map(|row| {
            let base = row * 38;
            let point_count = records[base] as usize;
            if point_count > 7 {
                return Err(PackingPrepError::GainRowCountExceedsMax {
                    count: point_count,
                    max: 7,
                }
                .into());
            }
            Ok(GainRowData {
                count: point_count,
                locations: records[base + 1..base + 1 + point_count]
                    .iter()
                    .map(|value| *value as i32)
                    .collect(),
                levels: records[base + 8..base + 8 + point_count]
                    .iter()
                    .map(|value| *value as i32)
                    .collect(),
            })
        })
        .collect()
}

fn rows_to_syntax(rows: &[GainRowData]) -> Vec<GainRowSyntax> {
    rows.iter()
        .map(|row| GainRowSyntax {
            count: row.count as u32,
            locations: u32_row(&row.locations, row.count),
            levels: u32_row(&row.levels, row.count),
        })
        .collect()
}

fn build_gain(
    channel: usize,
    object: &ObjectInputs,
    all_rows: &[Vec<GainRowData>],
    previous_gain_records: Option<&[u32]>,
) -> Result<GainSyntax, FrameError> {
    if object.init_header.obj_1b484 == 0 {
        return Ok(GainSyntax::Absent);
    }
    let rows = &all_rows[channel];
    let point_counts = rows.iter().map(|row| row.count as i32).collect::<Vec<_>>();
    let level_bands = rows
        .iter()
        .map(|row| ZerothGainLevelBand {
            count: row.count,
            levels: &row.levels,
        })
        .collect::<Vec<_>>();
    let location_bands = rows
        .iter()
        .map(|row| ZerothGainLocationBand {
            count: row.count,
            locations: &row.locations,
            levels: &row.levels,
        })
        .collect::<Vec<_>>();

    let (ngc, idlev, idloc) = if channel == 0 {
        let ngc_pick =
            zeroth_gain_ngc_mode_at5(&point_counts, None).map_err(PackingPrepError::from)?;
        let idlev_pick =
            zeroth_gain_idlev_mode_at5(&level_bands).map_err(PackingPrepError::from)?;
        let idloc_pick =
            zeroth_gain_idloc_mode_at5(&location_bands).map_err(PackingPrepError::from)?;
        let ngc_encoding = match ngc_pick.mode {
            0 => GainNgcEncodingSyntax::Raw,
            1 => GainNgcEncodingSyntax::Huffman,
            2 => GainNgcEncodingSyntax::Delta,
            3 => GainNgcEncodingSyntax::Direct {
                bit_width: ngc_pick.fixed_width.unwrap_or(0) as u8,
                base: ngc_pick.fixed_min.unwrap_or(0),
            },
            _ => unreachable!(),
        };
        let idlev_encoding = match idlev_pick.mode {
            0 => GainIdlevEncodingSyntax::Raw,
            1 => GainIdlevEncodingSyntax::Delta,
            2 => GainIdlevEncodingSyntax::RowDelta,
            3 => GainIdlevEncodingSyntax::Direct {
                bit_width: idlev_pick.fixed_width as u8,
                base: idlev_pick.fixed_min,
            },
            _ => unreachable!(),
        };
        let idloc_encoding = match idloc_pick.mode {
            0 => GainIdlocEncodingSyntax::Raw,
            1 => GainIdlocEncodingSyntax::LevelAdaptive,
            2 => GainIdlocEncodingSyntax::RowAdaptive,
            3 => GainIdlocEncodingSyntax::Direct {
                bit_width: idloc_pick.fixed_width as u8,
                base: idloc_pick.fixed_min,
            },
            _ => unreachable!(),
        };
        (
            GainNgcSyntax {
                mode: ngc_pick.mode as u32,
                encoding: ngc_encoding,
            },
            GainIdlevSyntax {
                mode: idlev_pick.mode as u32,
                encoding: idlev_encoding,
            },
            GainIdlocSyntax {
                mode: idloc_pick.mode as u32,
                encoding: idloc_encoding,
            },
        )
    } else {
        // Native ch1 scores the reference buffer using ch1's row count, not
        // ch0's own gain-header row count; the two can differ during roll-in.
        let previous = gain_rows_from_records(
            previous_gain_records.ok_or(PackingPrepError::GainMissingPreviousRows)?,
            rows.len(),
        )?;
        let previous_counts = previous
            .iter()
            .map(|row| row.count as i32)
            .collect::<Vec<_>>();
        let previous_levels = previous
            .iter()
            .map(|row| ZerothGainLevelBand {
                count: row.count,
                levels: &row.levels,
            })
            .collect::<Vec<_>>();
        let previous_locations = previous
            .iter()
            .map(|row| ZerothGainLocationBand {
                count: row.count,
                locations: &row.locations,
                levels: &row.levels,
            })
            .collect::<Vec<_>>();
        let ngc_pick = zeroth_gain_ngc_mode_at5(&point_counts, Some(&previous_counts))
            .map_err(PackingPrepError::from)?;
        let idlev_pick = zeroth_gain_idlev_mode_ch1_at5(&level_bands, &previous_levels)
            .map_err(PackingPrepError::from)?;
        let idloc_pick = zeroth_gain_idloc_mode_ch1_at5(&location_bands, &previous_locations)
            .map_err(PackingPrepError::from)?;
        let previous_syntax = rows_to_syntax(&previous);
        let ngc_encoding = match ngc_pick.mode {
            0 => GainNgcEncodingSyntax::Raw,
            1 => GainNgcEncodingSyntax::Huffman,
            2 => GainNgcEncodingSyntax::Previous {
                counts: previous_syntax.iter().map(|row| row.count).collect(),
            },
            3 => GainNgcEncodingSyntax::Empty,
            _ => unreachable!(),
        };
        let idlev_encoding = match idlev_pick.mode {
            0 => GainIdlevEncodingSyntax::Raw,
            1 => GainIdlevEncodingSyntax::Previous {
                levels: previous_syntax
                    .iter()
                    .map(|row| row.levels.clone())
                    .collect(),
            },
            2 => GainIdlevEncodingSyntax::Flagged {
                flags: idlev_pick.copy_flags.clone(),
            },
            3 => GainIdlevEncodingSyntax::Empty,
            _ => unreachable!(),
        };
        let idloc_encoding = match idloc_pick.mode {
            0 => GainIdlocEncodingSyntax::Raw,
            1 => GainIdlocEncodingSyntax::Previous {
                locations: previous_syntax
                    .iter()
                    .map(|row| row.locations.clone())
                    .collect(),
            },
            2 => GainIdlocEncodingSyntax::PreviousFlagged {
                locations: previous_syntax
                    .iter()
                    .map(|row| row.locations.clone())
                    .collect(),
                flags: idloc_pick.copy_flags.clone(),
            },
            3 => {
                let mut flags = idloc_pick.copy_markers.clone();
                flags.resize(rows.len(), 0);
                GainIdlocEncodingSyntax::PreviousRawFlagged {
                    locations: previous_syntax
                        .iter()
                        .map(|row| row.locations.clone())
                        .collect(),
                    flags,
                }
            }
            _ => unreachable!(),
        };
        (
            GainNgcSyntax {
                mode: ngc_pick.mode as u32,
                encoding: ngc_encoding,
            },
            GainIdlevSyntax {
                mode: idlev_pick.mode as u32,
                encoding: idlev_encoding,
            },
            GainIdlocSyntax {
                mode: idloc_pick.mode as u32,
                encoding: idloc_encoding,
            },
        )
    };

    Ok(GainSyntax::Present(GainPayloadSyntax {
        band_count: (object.init_header.obj_1b488 != 0)
            .then_some(object.init_header.obj_1b48c as usize),
        rows: rows_to_syntax(rows),
        ngc,
        idlev,
        idloc,
    }))
}

fn gated_from_summary(any: bool, partial: bool, flags: &[bool], count: usize) -> GatedFlagsSyntax {
    if !any {
        GatedFlagsSyntax::Absent
    } else if !partial {
        GatedFlagsSyntax::PresentWithoutFlags
    } else {
        GatedFlagsSyntax::Present {
            flags: flags.iter().copied().take(count).collect(),
        }
    }
}

fn gated_from_cfg_group(group: &(u32, u32, [u32; 16]), count: usize) -> GatedFlagsSyntax {
    let flags = group.2.iter().map(|word| *word != 0).collect::<Vec<_>>();
    gated_from_summary(group.0 != 0, group.1 != 0, &flags, count)
}

fn gated_from_activity(activity: &[i32], count: usize) -> Result<GatedFlagsSyntax, FrameError> {
    let summary = zeroth_activity_summary_at5(activity, count).map_err(PackingPrepError::from)?;
    let flags = activity.iter().map(|word| *word != 0).collect::<Vec<_>>();
    Ok(gated_from_summary(
        summary.any_flag != 0,
        summary.partial_flag != 0,
        &flags,
        count,
    ))
}

fn gha_records(rows: &[GhaBandData], active: &[bool]) -> Vec<GhaRecordSyntax> {
    rows.iter()
        .enumerate()
        .map(|(band, row)| GhaRecordSyntax {
            active: active.get(band).copied().unwrap_or(false),
            first_location: (row.window_words[0] != 0).then_some(row.window_words[2]),
            second_location: (row.window_words[1] != 0).then_some(row.window_words[3]),
            waves: row
                .records
                .iter()
                .map(|wave| GhaWaveSyntax {
                    idsf: wave.scale_index as u32,
                    phase: wave.phase_index as u32,
                    freq: wave.frequency as u32,
                })
                .collect(),
        })
        .collect()
}

fn gha_previous_counts(rows: &[GhaBandData]) -> Vec<u32> {
    rows.iter().map(|row| row.wave_count as u32).collect()
}

fn gha_previous_freq(rows: &[GhaBandData]) -> Vec<Vec<u32>> {
    rows.iter()
        .map(|row| {
            row.records
                .iter()
                .map(|wave| wave.frequency as u32)
                .collect()
        })
        .collect()
}

fn gha_previous_idsf(rows: &[GhaBandData]) -> Vec<Vec<u32>> {
    rows.iter()
        .map(|row| {
            row.records
                .iter()
                .map(|wave| wave.scale_index as u32)
                .collect()
        })
        .collect()
}

fn gha_predictor_indices(compact: &[i32], records: &[GhaRecordSyntax]) -> Vec<Vec<i32>> {
    let mut base = 0usize;
    records
        .iter()
        .map(|record| {
            if !record.active {
                return Vec::new();
            }
            let end = base + record.waves.len();
            let row = compact.get(base..end).unwrap_or(&[]).to_vec();
            base = end;
            row
        })
        .collect()
}

fn build_gha_syntax(
    frontend: &FrontendState,
    prep: &GhaPackingPrep,
    channel_count: usize,
) -> Result<GhaSyntax, FrameError> {
    let arena = frontend.packer_arena(0);
    if arena.header_active == 0 {
        return Ok(GhaSyntax::Absent);
    }
    let band_count = arena.header_band_count as usize;
    let shared = arena
        .shared
        .iter()
        .map(|word| *word != 0)
        .collect::<Vec<_>>();
    let stereo = arena
        .opposite
        .iter()
        .map(|word| *word != 0)
        .collect::<Vec<_>>();
    let stereo_flags = (channel_count == 2).then(|| {
        [
            gated_from_summary(
                prep.summaries.shared.0,
                prep.summaries.shared.1,
                &shared,
                band_count,
            ),
            gated_from_summary(
                prep.summaries.swap.0,
                prep.summaries.swap.1,
                &prep.swap_flags,
                band_count,
            ),
            gated_from_summary(
                prep.summaries.stereo.0,
                prep.summaries.stereo.1,
                &stereo,
                band_count,
            ),
        ]
    });

    let channels = (0..channel_count)
        .map(|channel| {
            let selectors = prep
                .channels
                .get(channel)
                .ok_or(FrameError::GhaChannelMissing { channel })?;
            let rows = prep
                .post_swap_channels
                .get(channel)
                .ok_or(FrameError::GhaChannelMissing { channel })?;
            let records = gha_records(rows, &selectors.active_flags);
            let previous_rows = prep.post_swap_channels.first().unwrap_or(rows);
            let nwavs_encoding = match selectors.nwavs & 3 {
                0 => GhaNwavsEncodingSyntax::Raw,
                1 => GhaNwavsEncodingSyntax::Huffman,
                2 => GhaNwavsEncodingSyntax::Previous {
                    counts: gha_previous_counts(previous_rows),
                },
                3 => GhaNwavsEncodingSyntax::Empty,
                _ => unreachable!(),
            };
            let freq_encoding = if selectors.freq & 1 == 0 {
                GhaFreqEncodingSyntax::Local {
                    // A `None` means the serializer leaves a zero-filled word
                    // untouched; zero is therefore the direct typed value.
                    modes: selectors
                        .freq_modes
                        .iter()
                        .map(|mode| u32::from(mode.unwrap_or(false)))
                        .collect(),
                }
            } else {
                GhaFreqEncodingSyntax::Previous {
                    rows: gha_previous_freq(previous_rows),
                }
            };
            let idsf_encoding = match selectors.idsf & 3 {
                0 => GhaIdsfEncodingSyntax::Raw,
                1 => GhaIdsfEncodingSyntax::Huffman,
                2 => GhaIdsfEncodingSyntax::Previous {
                    rows: gha_previous_idsf(previous_rows),
                    indices: gha_predictor_indices(&selectors.compact_map, &records),
                },
                3 => GhaIdsfEncodingSyntax::Empty,
                _ => unreachable!(),
            };
            Ok(GhaChannelSyntax {
                channel_index: channel as u32,
                records,
                idloc_mode: selectors.idloc,
                nwavs: GhaNwavsSyntax {
                    mode: selectors.nwavs,
                    encoding: nwavs_encoding,
                },
                freq: GhaFreqSyntax {
                    mode: selectors.freq,
                    encoding: freq_encoding,
                },
                idsf: GhaIdsfSyntax {
                    mode: selectors.idsf,
                    encoding: idsf_encoding,
                },
            })
        })
        .collect::<Result<Vec<_>, FrameError>>()?;

    Ok(GhaSyntax::Present(GhaPayloadSyntax {
        header_mode: arena.header_mode,
        band_count,
        stereo_flags,
        channels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_header_rounds_192_extent_independently_of_active_units() {
        let header = build_block_header(2, 0, 1, 29, 13);

        assert_eq!(header.channel_mode, 1);
        assert_eq!(header.quant_header, 32);
        assert_eq!(header.quant_unit_count, 0);
        assert_eq!(header.stereo_unit_count, 1);
        assert_eq!(header.gainb_count, 16);
    }

    #[test]
    fn block_header_keeps_non_rounding_reduced_extent() {
        let header = build_block_header(2, 17, 8, 28, 12);

        assert_eq!(header.quant_header, 28);
        assert_eq!(header.quant_unit_count, 17);
        assert_eq!(header.gainb_count, 12);
    }
}
