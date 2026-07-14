//! Typed ATRAC3plus frame-syntax boundary.
//!
//! This first migration slice names the frame, group, channel, and section
//! control fields consumed by the packer. The retained reference backing is a
//! temporary parity adapter; payload-family fields move out of it section by
//! section before production packing switches to this type.

use crate::bitstream::frame::{FrameAssemblyError, FramePrepackerState};
use crate::tables::at5::{isps_at5, nsps_at5};
use crate::tables::generated::{G_A_IDSPCBANDS_AT5, G_A_IDSPCQUS_AT5};
use crate::tables::spectral::SPECTRAL_DESCRIPTOR_SLOTS;

const MAX_QUANT_UNITS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameSyntax {
    frame_bytes: usize,
    groups: Vec<BlockGroupSyntax>,
    reference_backing: FramePrepackerState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockGroupSyntax {
    header: BlockHeaderSyntax,
    channels: Vec<ChannelSyntax>,
    stereo: Option<StereoSyntax>,
    post_payload: Option<[u8; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlockHeaderSyntax {
    pub channel_mode: u32,
    pub quant_header: u32,
    pub header_flag: bool,
    pub quant_unit_count: usize,
    pub bandwidth_gate: bool,
    pub stereo_unit_count: usize,
    pub gainb_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChannelSyntax {
    pub channel_index: u32,
    pub previous_channel: Option<usize>,
    pub idwl: IdwlSyntax,
    pub idsf: IdsfSyntax,
    pub idct: IdctSyntax,
    pub spectral: SpectralSyntax,
    pub gainb: GatedFlagsSyntax,
    pub gain_present: bool,
    pub gha_present: bool,
    pub gha_idam_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StereoSyntax {
    pub secondary: GatedFlagsSyntax,
    pub primary: GatedFlagsSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GatedFlagsSyntax {
    Absent,
    PresentWithoutFlags,
    Present { flags: Vec<bool> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpectralSyntax {
    pub units: Vec<SpectralUnitSyntax>,
    pub tail_values: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpectralUnitSyntax {
    pub quant_unit: usize,
    pub codebook: SpectralCodebookSyntax,
    pub samples: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpectralCodebookSyntax {
    pub bandwidth: bool,
    pub selector: usize,
    pub word_length: usize,
}

impl SpectralCodebookSyntax {
    pub(crate) fn slot_index(self) -> usize {
        usize::from(self.bandwidth) * 56 + self.selector * 7 + (self.word_length - 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdsfSyntax {
    pub mode: u32,
    pub encoding: IdsfEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdsfEncodingSyntax {
    Raw {
        values: Vec<u32>,
    },
    Direct {
        mode_selector: usize,
        field_a: u32,
        field_b: u32,
        prefix_count: usize,
        residual_bits: u8,
        residual_base: i32,
        count: usize,
        values: Vec<i32>,
    },
    Grouped {
        huffman_selector: usize,
        field_a: u32,
        field_b: u32,
        count: usize,
        symbols: Vec<u32>,
    },
    Delta {
        mode_selector: usize,
        huffman_selector: usize,
        field_a: u32,
        field_b: u32,
        count: usize,
        values: Vec<i32>,
    },
    Previous {
        progressive: bool,
        huffman_selector: usize,
        count: usize,
        current_values: Vec<u32>,
        previous_values: Vec<u32>,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdwlSyntax {
    pub mode: u32,
    pub encoding: IdwlEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdwlEncodingSyntax {
    Raw {
        word_lengths: Vec<u32>,
    },
    Direct {
        selector_a: u32,
        selector_b: u32,
        count: usize,
        mode3_value: u32,
        prefix_count: usize,
        residual_bits: u8,
        residual_base: u32,
        values: Vec<u32>,
    },
    Grouped {
        selector_b: u32,
        count: usize,
        mode3_value: u32,
        subgroup_flag: u32,
        huffman_selector: usize,
        field_3bits: u32,
        field_4bits: u32,
        group_flags: Vec<u32>,
        symbols: Vec<u32>,
    },
    Delta {
        selector_a: u32,
        selector_b: u32,
        count: usize,
        config_count: usize,
        mode3_value: u32,
        huffman_selector: usize,
        values: Vec<u32>,
    },
    Previous {
        progressive: bool,
        selector_b: u32,
        count: usize,
        config_count: usize,
        mode3_value: u32,
        huffman_selector: usize,
        current_values: Vec<u32>,
        previous_values: Vec<u32>,
        tail_flags: Vec<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdctSyntax {
    pub bandwidth: bool,
    pub mode: u32,
    pub bandwidth_mode: usize,
    pub count: IdctCountSyntax,
    pub rows: Vec<IdctRowSyntax>,
    pub encoding: IdctEncodingSyntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdctCountSyntax {
    FullBand(usize),
    Explicit(usize),
}

impl IdctCountSyntax {
    pub(crate) fn active(self) -> usize {
        match self {
            Self::FullBand(count) | Self::Explicit(count) => count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdctRowSyntax {
    pub mode: u32,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdctEncodingSyntax {
    Fixed,
    Huffman,
    Delta,
    Empty,
    Previous { values: Vec<u32> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameSyntaxError {
    Reference(FrameAssemblyError),
    GroupCount {
        declared: usize,
        actual: usize,
    },
    ChannelCount {
        group: usize,
        declared: usize,
        actual: usize,
    },
    UnsupportedChannelCount {
        group: usize,
        channels: usize,
    },
    InvalidHeader {
        group: usize,
        detail: &'static str,
    },
    InvalidQuantUnitCount {
        group: usize,
        count: usize,
    },
    InvalidMode {
        group: usize,
        channel: usize,
        section: &'static str,
        mode: u32,
    },
    InvalidIdwlEncoding {
        group: usize,
        channel: usize,
        expected: &'static str,
        actual: &'static str,
    },
    InvalidIdwlPayload {
        group: usize,
        channel: usize,
        detail: &'static str,
    },
    InvalidIdsfEncoding {
        group: usize,
        channel: usize,
        expected: &'static str,
        actual: &'static str,
    },
    InvalidIdsfPayload {
        group: usize,
        channel: usize,
        detail: &'static str,
    },
    InvalidSpectralPayload {
        group: usize,
        channel: usize,
        detail: &'static str,
    },
    UnsupportedSpectralRemap {
        group: usize,
    },
    InvalidStereoPayload {
        group: usize,
        detail: &'static str,
    },
    InvalidGainbPayload {
        group: usize,
        channel: usize,
        detail: &'static str,
    },
    InvalidPostPayload {
        group: usize,
        value: u8,
    },
    InvalidPreviousChannel {
        group: usize,
        channel: usize,
        previous: usize,
    },
    InvalidExplicitIdctCount {
        group: usize,
        channel: usize,
        count: usize,
        quant_units: usize,
    },
    InvalidFullBandIdctCount {
        group: usize,
        channel: usize,
        count: usize,
        quant_units: usize,
    },
    InvalidIdctBandwidthMode {
        group: usize,
        channel: usize,
        mode: usize,
    },
    InvalidIdctEncoding {
        group: usize,
        channel: usize,
        expected: &'static str,
        actual: &'static str,
    },
    MissingPreviousChannel {
        group: usize,
        channel: usize,
    },
    InvalidIdctRows {
        group: usize,
        channel: usize,
        expected: usize,
        actual: usize,
    },
    InvalidPreviousIdctValues {
        group: usize,
        channel: usize,
        expected: usize,
        actual: usize,
    },
    EmptyFrame,
}

impl From<FrameAssemblyError> for FrameSyntaxError {
    fn from(value: FrameAssemblyError) -> Self {
        Self::Reference(value)
    }
}

impl FrameSyntax {
    pub(crate) fn from_reference(
        reference: &FramePrepackerState,
    ) -> Result<Self, FrameSyntaxError> {
        let mut groups = Vec::with_capacity(reference.groups.len());
        for (group_index, group) in reference.groups.iter().enumerate() {
            let source = group
                .objects
                .first()
                .ok_or(FrameSyntaxError::ChannelCount {
                    group: groups.len(),
                    declared: group.nblk,
                    actual: 0,
                })?;
            let header = BlockHeaderSyntax {
                channel_mode: source.cfg_u32(0xa0)?,
                quant_header: source.cfg_u32(0xc4)?,
                header_flag: source.cfg_u32(0x118)? != 0,
                quant_unit_count: source.cfg_u32(0xb0)? as usize,
                bandwidth_gate: source.cfg_u32(0x90)? != 0,
                stereo_unit_count: source.cfg_u32(0xc0)? as usize,
                gainb_count: source.cfg_u32(0xc8)? as usize,
            };
            let channels = group
                .objects
                .iter()
                .take(group.nblk)
                .enumerate()
                .map(|(channel_index, object)| {
                    let idwl_mode = object.u32(0x1c70c)?;
                    let idwl_dispatch = (idwl_mode & 3) + ((object.channel_index & 1) << 2);
                    let idwl_config_count = object.cfg_u32(0xc4)? as usize;
                    let idwl_count = object.u32(0x1c728)? as usize;
                    let idwl_encoding = match idwl_dispatch {
                        0 | 4 => IdwlEncodingSyntax::Raw {
                            word_lengths: object.u32_array(0x1b5f8, idwl_config_count)?,
                        },
                        1 => IdwlEncodingSyntax::Direct {
                            selector_a: object.u32(0x1c71c)?,
                            selector_b: object.u32(0x1c724)?,
                            count: idwl_count,
                            mode3_value: object.u32(0x1c72c)?,
                            prefix_count: object.u32(0x1c710)? as usize,
                            residual_bits: object.u32(0x1c714)? as u8,
                            residual_base: object.u32(0x1c718)?,
                            values: object.u32_array(0x1c7f0, idwl_count)?,
                        },
                        2 => {
                            let group_count = object.cfg_u32(0xd4)? as usize;
                            let group_flags = (0..group_count)
                                .map(|index| object.cfg_u32(0xd8 + index * 4))
                                .collect::<Result<Vec<_>, FrameAssemblyError>>()?;
                            IdwlEncodingSyntax::Grouped {
                                selector_b: object.u32(0x1c724)?,
                                count: idwl_count,
                                mode3_value: object.u32(0x1c72c)?,
                                subgroup_flag: object.u32(0x1c738)?,
                                huffman_selector: object.u32(0x1c720)? as usize,
                                field_3bits: object.u32(0x1c734)?,
                                field_4bits: object.u32(0x1c730)?,
                                group_flags,
                                symbols: object.u32_array(0x1c870, idwl_count)?,
                            }
                        }
                        3 | 7 => IdwlEncodingSyntax::Delta {
                            selector_a: object.u32(0x1c71c)?,
                            selector_b: object.u32(0x1c724)?,
                            count: idwl_count,
                            config_count: idwl_config_count,
                            mode3_value: object.u32(0x1c72c)?,
                            huffman_selector: object.u32(0x1c720)? as usize,
                            values: object.u32_array(0x1c7f0, idwl_config_count)?,
                        },
                        5 | 6 => {
                            let previous = object
                                .previous_index
                                .and_then(|index| group.objects.get(index))
                                .ok_or(FrameSyntaxError::MissingPreviousChannel {
                                    group: group_index,
                                    channel: channel_index,
                                })?;
                            IdwlEncodingSyntax::Previous {
                                progressive: idwl_dispatch == 6,
                                selector_b: object.u32(0x1c724)?,
                                count: idwl_count,
                                config_count: idwl_config_count,
                                mode3_value: object.u32(0x1c72c)?,
                                huffman_selector: object.u32(0x1c720)? as usize,
                                current_values: object.u32_array(0x1b5f8, idwl_config_count)?,
                                previous_values: previous.u32_array(0x1b5f8, idwl_config_count)?,
                                tail_flags: object.u32_array(0x1c7f0, idwl_config_count)?,
                            }
                        }
                        _ => unreachable!("masked IDWL dispatch index"),
                    };
                    let idsf_mode = object.u32(0x1c73c)?;
                    let idsf_dispatch = (idsf_mode & 3) + ((object.channel_index & 1) << 2);
                    let idsf_count = header.quant_unit_count;
                    let idsf_encoding = match idsf_dispatch {
                        0 | 4 => IdsfEncodingSyntax::Raw {
                            values: object.u32_array(0x1b678, idsf_count)?,
                        },
                        1 => {
                            let mode_selector = (object.u32(0x1c750)? & 3) as usize;
                            IdsfEncodingSyntax::Direct {
                                mode_selector,
                                field_a: object.u32(0x1c758)?,
                                field_b: object.u32(0x1c754)?,
                                prefix_count: object.u32(0x1c740)? as usize,
                                residual_bits: object.u32(0x1c744)? as u8,
                                residual_base: object.u32(0x1c748)? as i32,
                                count: idsf_count,
                                values: object
                                    .i32_array(0x1c8f0 + mode_selector * 0x80, idsf_count)?,
                            }
                        }
                        2 => IdsfEncodingSyntax::Grouped {
                            huffman_selector: (object.u32(0x1c74c)? & 3) as usize,
                            field_a: object.u32(0x1c758)?,
                            field_b: object.u32(0x1c754)?,
                            count: idsf_count,
                            symbols: object.u32_array(0x1ca70, idsf_count)?,
                        },
                        3 => {
                            let mode_selector = (object.u32(0x1c750)? & 3) as usize;
                            IdsfEncodingSyntax::Delta {
                                mode_selector,
                                huffman_selector: (object.u32(0x1c74c)? & 3) as usize,
                                field_a: object.u32(0x1c758)?,
                                field_b: object.u32(0x1c754)?,
                                count: idsf_count,
                                values: object
                                    .i32_array(0x1c8f0 + mode_selector * 0x80, idsf_count)?,
                            }
                        }
                        5 | 6 => {
                            let previous = object
                                .previous_index
                                .and_then(|index| group.objects.get(index))
                                .ok_or(FrameSyntaxError::MissingPreviousChannel {
                                    group: group_index,
                                    channel: channel_index,
                                })?;
                            IdsfEncodingSyntax::Previous {
                                progressive: idsf_dispatch == 6,
                                huffman_selector: (object.u32(0x1c74c)? & 3) as usize,
                                count: idsf_count,
                                current_values: object.u32_array(0x1b678, idsf_count)?,
                                previous_values: previous.u32_array(0x1b678, idsf_count)?,
                            }
                        }
                        7 => {
                            if object.previous_index.is_none() {
                                return Err(FrameSyntaxError::MissingPreviousChannel {
                                    group: group_index,
                                    channel: channel_index,
                                });
                            }
                            IdsfEncodingSyntax::Empty
                        }
                        _ => unreachable!("masked IDSF dispatch index"),
                    };
                    let mode = object.u32(0x1078)?;
                    let count = if object.u32(0x1080)? == 0 {
                        IdctCountSyntax::FullBand(header.quant_unit_count)
                    } else {
                        IdctCountSyntax::Explicit(object.u32(0x107c)? as usize)
                    };
                    let active = count.active();
                    let rows = (0..active)
                        .map(|index| {
                            Ok(IdctRowSyntax {
                                mode: object.u32(0x1084 + index * 4)?,
                                value: object.u32(0x1b578 + index * 4)?,
                            })
                        })
                        .collect::<Result<Vec<_>, FrameAssemblyError>>()?;
                    let dispatch = (mode & 3) + ((object.channel_index & 1) << 2);
                    let encoding = match dispatch {
                        0 | 4 => IdctEncodingSyntax::Fixed,
                        1 | 5 => IdctEncodingSyntax::Huffman,
                        2 | 6 => IdctEncodingSyntax::Delta,
                        3 => IdctEncodingSyntax::Empty,
                        7 => {
                            let previous = object
                                .previous_index
                                .and_then(|index| group.objects.get(index))
                                .ok_or(FrameSyntaxError::MissingPreviousChannel {
                                    group: group_index,
                                    channel: channel_index,
                                })?;
                            let values = (0..active)
                                .map(|index| previous.u32(0x1b578 + index * 4))
                                .collect::<Result<Vec<_>, FrameAssemblyError>>()?;
                            IdctEncodingSyntax::Previous { values }
                        }
                        _ => unreachable!("masked IDCT dispatch index"),
                    };
                    if !header.bandwidth_gate {
                        return Err(FrameSyntaxError::UnsupportedSpectralRemap {
                            group: group_index,
                        });
                    }
                    let nsps = nsps_at5();
                    let isps = isps_at5();
                    let mut spectral_units = Vec::new();
                    for quant_unit in 0..header.quant_unit_count {
                        let word_length = object.u32(0x1b5f8 + quant_unit * 4)? as i32;
                        if word_length <= 0 {
                            continue;
                        }
                        let sample_count = usize::from(nsps[quant_unit]);
                        spectral_units.push(SpectralUnitSyntax {
                            quant_unit,
                            codebook: SpectralCodebookSyntax {
                                bandwidth: object.u32(0x1074)? != 0,
                                selector: object.u32(0x1b578 + quant_unit * 4)? as usize,
                                word_length: word_length as usize,
                            },
                            samples: object.u16_array(
                                0x1b6f8 + usize::from(isps[quant_unit]) * 2,
                                sample_count,
                            )?,
                        });
                    }
                    let tail_values = if header.quant_unit_count <= 2 {
                        Vec::new()
                    } else {
                        idspcqu_tail_count_at(header.stereo_unit_count + 0x1f)
                            .map(|count| {
                                object
                                    .u32_array(0x1c6f8, count)
                                    .map(|words| words.into_iter().map(|word| word as u8).collect())
                            })
                            .transpose()?
                            .unwrap_or_default()
                    };
                    Ok(ChannelSyntax {
                        channel_index: object.channel_index,
                        previous_channel: object.previous_index,
                        idwl: IdwlSyntax {
                            mode: idwl_mode,
                            encoding: idwl_encoding,
                        },
                        idsf: IdsfSyntax {
                            mode: idsf_mode,
                            encoding: idsf_encoding,
                        },
                        idct: IdctSyntax {
                            bandwidth: object.u32(0x1074)? != 0,
                            mode,
                            bandwidth_mode: object.cfg_u32(0x90)? as usize,
                            count,
                            rows,
                            encoding,
                        },
                        spectral: SpectralSyntax {
                            units: spectral_units,
                            tail_values,
                        },
                        gainb: gated_flags_from_gainb(object, header.gainb_count)?,
                        gain_present: object.u32(0x1b484)? != 0,
                        gha_present: object.arena_u32(0)? != 0,
                        gha_idam_enabled: object.arena_u32(1)? == 0,
                    })
                })
                .collect::<Result<Vec<_>, FrameSyntaxError>>()?;
            let stereo = if group.nblk == 2 {
                Some(StereoSyntax {
                    secondary: gated_flags_from_cfg(
                        source,
                        0x48,
                        0x4c,
                        0x50,
                        header.stereo_unit_count,
                    )?,
                    primary: gated_flags_from_cfg(
                        source,
                        0x00,
                        0x04,
                        0x08,
                        header.stereo_unit_count,
                    )?,
                })
            } else {
                None
            };
            let post_payload = if source.cfg_u32(0x94)? == 0 {
                None
            } else {
                Some([source.cfg_u32(0x98)? as u8, source.cfg_u32(0x9c)? as u8])
            };
            groups.push(BlockGroupSyntax {
                header,
                channels,
                stereo,
                post_payload,
            });
        }
        let syntax = Self {
            frame_bytes: reference.frame_bytes,
            groups,
            reference_backing: reference.clone(),
        };
        syntax.validate()?;
        Ok(syntax)
    }

    pub(crate) fn validate(&self) -> Result<(), FrameSyntaxError> {
        if self.frame_bytes == 0 {
            return Err(FrameSyntaxError::EmptyFrame);
        }
        if self.groups.len() != self.reference_backing.block_count {
            return Err(FrameSyntaxError::GroupCount {
                declared: self.reference_backing.block_count,
                actual: self.groups.len(),
            });
        }
        for (group_index, (syntax, reference)) in self
            .groups
            .iter()
            .zip(&self.reference_backing.groups)
            .enumerate()
        {
            if syntax.channels.len() != reference.nblk {
                return Err(FrameSyntaxError::ChannelCount {
                    group: group_index,
                    declared: reference.nblk,
                    actual: syntax.channels.len(),
                });
            }
            if !(1..=2).contains(&syntax.channels.len()) {
                return Err(FrameSyntaxError::UnsupportedChannelCount {
                    group: group_index,
                    channels: syntax.channels.len(),
                });
            }
            if syntax.header.channel_mode > 3 {
                return Err(FrameSyntaxError::InvalidHeader {
                    group: group_index,
                    detail: "channel mode",
                });
            }
            if !(1..=32).contains(&(syntax.header.quant_header as usize)) {
                return Err(FrameSyntaxError::InvalidHeader {
                    group: group_index,
                    detail: "quant header",
                });
            }
            if syntax.header.stereo_unit_count > MAX_QUANT_UNITS {
                return Err(FrameSyntaxError::InvalidHeader {
                    group: group_index,
                    detail: "stereo-unit count",
                });
            }
            if syntax.header.gainb_count > MAX_QUANT_UNITS {
                return Err(FrameSyntaxError::InvalidHeader {
                    group: group_index,
                    detail: "gainB count",
                });
            }
            if syntax.header.quant_unit_count > MAX_QUANT_UNITS {
                return Err(FrameSyntaxError::InvalidQuantUnitCount {
                    group: group_index,
                    count: syntax.header.quant_unit_count,
                });
            }
            match (&syntax.stereo, syntax.channels.len()) {
                (Some(stereo), 2) => {
                    stereo
                        .secondary
                        .validate(syntax.header.stereo_unit_count)
                        .and_then(|_| stereo.primary.validate(syntax.header.stereo_unit_count))
                        .map_err(|detail| FrameSyntaxError::InvalidStereoPayload {
                            group: group_index,
                            detail,
                        })?;
                }
                (None, 1) => {}
                _ => {
                    return Err(FrameSyntaxError::InvalidStereoPayload {
                        group: group_index,
                        detail: "channel-mode agreement",
                    });
                }
            }
            if let Some(words) = syntax.post_payload
                && let Some(value) = words.into_iter().find(|value| *value > 0x0f)
            {
                return Err(FrameSyntaxError::InvalidPostPayload {
                    group: group_index,
                    value,
                });
            }
            for (channel_index, channel) in syntax.channels.iter().enumerate() {
                for (section, mode) in [
                    ("idwl", channel.idwl.mode),
                    ("idsf", channel.idsf.mode),
                    ("idct", channel.idct.mode),
                ] {
                    if mode > 3 {
                        return Err(FrameSyntaxError::InvalidMode {
                            group: group_index,
                            channel: channel_index,
                            section,
                            mode,
                        });
                    }
                }
                let expected_idwl =
                    IdwlEncodingSyntax::kind_for_dispatch(channel.idwl.mode, channel.channel_index);
                if channel.idwl.encoding.kind() != expected_idwl {
                    return Err(FrameSyntaxError::InvalidIdwlEncoding {
                        group: group_index,
                        channel: channel_index,
                        expected: expected_idwl,
                        actual: channel.idwl.encoding.kind(),
                    });
                }
                if let Err(detail) = channel
                    .idwl
                    .encoding
                    .validate(syntax.header.quant_header as usize)
                {
                    return Err(FrameSyntaxError::InvalidIdwlPayload {
                        group: group_index,
                        channel: channel_index,
                        detail,
                    });
                }
                let expected_idsf =
                    IdsfEncodingSyntax::kind_for_dispatch(channel.idsf.mode, channel.channel_index);
                if channel.idsf.encoding.kind() != expected_idsf {
                    return Err(FrameSyntaxError::InvalidIdsfEncoding {
                        group: group_index,
                        channel: channel_index,
                        expected: expected_idsf,
                        actual: channel.idsf.encoding.kind(),
                    });
                }
                if let Err(detail) = channel
                    .idsf
                    .encoding
                    .validate(syntax.header.quant_unit_count)
                {
                    return Err(FrameSyntaxError::InvalidIdsfPayload {
                        group: group_index,
                        channel: channel_index,
                        detail,
                    });
                }
                if let Some(previous) = channel.previous_channel
                    && previous >= syntax.channels.len()
                {
                    return Err(FrameSyntaxError::InvalidPreviousChannel {
                        group: group_index,
                        channel: channel_index,
                        previous,
                    });
                }
                if let IdctCountSyntax::Explicit(count) = channel.idct.count
                    && count > syntax.header.quant_unit_count
                {
                    return Err(FrameSyntaxError::InvalidExplicitIdctCount {
                        group: group_index,
                        channel: channel_index,
                        count,
                        quant_units: syntax.header.quant_unit_count,
                    });
                }
                if let IdctCountSyntax::FullBand(count) = channel.idct.count
                    && count != syntax.header.quant_unit_count
                {
                    return Err(FrameSyntaxError::InvalidFullBandIdctCount {
                        group: group_index,
                        channel: channel_index,
                        count,
                        quant_units: syntax.header.quant_unit_count,
                    });
                }
                if channel.idct.bandwidth_mode > 1 {
                    return Err(FrameSyntaxError::InvalidIdctBandwidthMode {
                        group: group_index,
                        channel: channel_index,
                        mode: channel.idct.bandwidth_mode,
                    });
                }
                let expected_encoding =
                    IdctEncodingSyntax::for_dispatch(channel.idct.mode, channel.channel_index);
                if channel.idct.encoding.kind() != expected_encoding.kind() {
                    return Err(FrameSyntaxError::InvalidIdctEncoding {
                        group: group_index,
                        channel: channel_index,
                        expected: expected_encoding.kind(),
                        actual: channel.idct.encoding.kind(),
                    });
                }
                let active = channel.idct.count.active();
                if channel.idct.rows.len() != active {
                    return Err(FrameSyntaxError::InvalidIdctRows {
                        group: group_index,
                        channel: channel_index,
                        expected: active,
                        actual: channel.idct.rows.len(),
                    });
                }
                if let IdctEncodingSyntax::Previous { values } = &channel.idct.encoding
                    && values.len() != active
                {
                    return Err(FrameSyntaxError::InvalidPreviousIdctValues {
                        group: group_index,
                        channel: channel_index,
                        expected: active,
                        actual: values.len(),
                    });
                }
                if let Err(detail) = channel.spectral.validate(
                    syntax.header.quant_unit_count,
                    syntax.header.stereo_unit_count,
                ) {
                    return Err(FrameSyntaxError::InvalidSpectralPayload {
                        group: group_index,
                        channel: channel_index,
                        detail,
                    });
                }
                if let Err(detail) = channel.gainb.validate(syntax.header.gainb_count) {
                    return Err(FrameSyntaxError::InvalidGainbPayload {
                        group: group_index,
                        channel: channel_index,
                        detail,
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn to_reference(&self) -> Result<FramePrepackerState, FrameSyntaxError> {
        self.validate()?;
        Ok(self.reference_backing.clone())
    }

    pub(crate) fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }

    pub(crate) fn groups(&self) -> &[BlockGroupSyntax] {
        &self.groups
    }
}

impl BlockGroupSyntax {
    pub(crate) fn header(&self) -> BlockHeaderSyntax {
        self.header
    }

    pub(crate) fn channels(&self) -> &[ChannelSyntax] {
        &self.channels
    }

    pub(crate) fn stereo(&self) -> Option<&StereoSyntax> {
        self.stereo.as_ref()
    }

    pub(crate) fn post_payload(&self) -> Option<[u8; 2]> {
        self.post_payload
    }
}

impl ChannelSyntax {
    pub(crate) fn idwl(&self) -> &IdwlSyntax {
        &self.idwl
    }

    pub(crate) fn idct(&self) -> &IdctSyntax {
        &self.idct
    }

    pub(crate) fn idsf(&self) -> &IdsfSyntax {
        &self.idsf
    }

    pub(crate) fn spectral(&self) -> &SpectralSyntax {
        &self.spectral
    }

    pub(crate) fn gainb(&self) -> &GatedFlagsSyntax {
        &self.gainb
    }
}

impl GatedFlagsSyntax {
    fn validate(&self, expected_flags: usize) -> Result<(), &'static str> {
        match self {
            Self::Absent | Self::PresentWithoutFlags => Ok(()),
            Self::Present { flags } if flags.len() == expected_flags => Ok(()),
            Self::Present { .. } => Err("flag count"),
        }
    }
}

fn gated_flags_from_cfg(
    object: &crate::bitstream::frame::ObjectState,
    present_offset: usize,
    flags_present_offset: usize,
    flags_offset: usize,
    count: usize,
) -> Result<GatedFlagsSyntax, FrameAssemblyError> {
    if object.cfg_u32(present_offset)? == 0 {
        return Ok(GatedFlagsSyntax::Absent);
    }
    if object.cfg_u32(flags_present_offset)? == 0 {
        return Ok(GatedFlagsSyntax::PresentWithoutFlags);
    }
    let flags = (0..count)
        .map(|index| {
            object
                .cfg_u32(flags_offset + index * 4)
                .map(|value| value != 0)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GatedFlagsSyntax::Present { flags })
}

fn gated_flags_from_gainb(
    object: &crate::bitstream::frame::ObjectState,
    count: usize,
) -> Result<GatedFlagsSyntax, FrameAssemblyError> {
    if object.gainb_u32(0x980)? == 0 {
        return Ok(GatedFlagsSyntax::Absent);
    }
    if object.gainb_u32(0x984)? == 0 {
        return Ok(GatedFlagsSyntax::PresentWithoutFlags);
    }
    let flags = (0..count)
        .map(|index| object.gainb_u32(0x988 + index * 4).map(|value| value != 0))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GatedFlagsSyntax::Present { flags })
}

impl SpectralSyntax {
    fn validate(
        &self,
        quant_unit_count: usize,
        stereo_unit_count: usize,
    ) -> Result<(), &'static str> {
        let nsps = nsps_at5();
        let mut previous_quant_unit = None;
        for unit in &self.units {
            if unit.quant_unit >= quant_unit_count {
                return Err("quant-unit index");
            }
            if previous_quant_unit.is_some_and(|previous| previous >= unit.quant_unit) {
                return Err("quant-unit ordering");
            }
            previous_quant_unit = Some(unit.quant_unit);
            if unit.codebook.selector > 7 {
                return Err("codebook selector");
            }
            if !(1..=7).contains(&unit.codebook.word_length) {
                return Err("word length");
            }
            if SPECTRAL_DESCRIPTOR_SLOTS
                .get(unit.codebook.slot_index())
                .is_none()
            {
                return Err("codebook slot");
            }
            if unit.samples.len() != usize::from(nsps[unit.quant_unit]) {
                return Err("sample count");
            }
        }
        let expected_tail = if quant_unit_count <= 2 {
            0
        } else {
            idspcqu_tail_count_at(stereo_unit_count + 0x1f).unwrap_or(0)
        };
        (self.tail_values.len() == expected_tail)
            .then_some(())
            .ok_or("tail count")
    }
}

/// Interpret the IDSPCQU extent table, including its native contiguous-table
/// spill into the following IDSPCBANDS bytes and the `0xff` absent sentinel.
pub(crate) fn idspcqu_tail_count_at(index: usize) -> Option<usize> {
    let value = if index < G_A_IDSPCQUS_AT5.len() {
        G_A_IDSPCQUS_AT5[index]
    } else {
        *G_A_IDSPCBANDS_AT5.get(index - G_A_IDSPCQUS_AT5.len())?
    };
    (value != 0xff).then_some(usize::from(value) + 1)
}

impl IdwlEncodingSyntax {
    fn kind_for_dispatch(mode: u32, channel_index: u32) -> &'static str {
        match (mode & 3) + ((channel_index & 1) << 2) {
            0 | 4 => "raw",
            1 => "direct",
            2 => "grouped",
            3 | 7 => "delta",
            5 => "previous",
            6 => "progressive-previous",
            _ => unreachable!("masked IDWL dispatch index"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw { .. } => "raw",
            Self::Direct { .. } => "direct",
            Self::Grouped { .. } => "grouped",
            Self::Delta { .. } => "delta",
            Self::Previous {
                progressive: false, ..
            } => "previous",
            Self::Previous {
                progressive: true, ..
            } => "progressive-previous",
        }
    }

    fn validate(&self, config_count: usize) -> Result<(), &'static str> {
        match self {
            Self::Raw { word_lengths } => (word_lengths.len() == config_count)
                .then_some(())
                .ok_or("raw word-length count"),
            Self::Direct {
                count,
                prefix_count,
                residual_bits,
                values,
                ..
            } => {
                if *count > config_count {
                    return Err("direct active count");
                }
                if *prefix_count > *count {
                    return Err("direct prefix count");
                }
                if *residual_bits > 3 {
                    return Err("direct residual width");
                }
                (values.len() == *count)
                    .then_some(())
                    .ok_or("direct value count")
            }
            Self::Grouped {
                count,
                huffman_selector,
                group_flags,
                symbols,
                ..
            } => {
                if *count > config_count {
                    return Err("grouped active count");
                }
                if *huffman_selector > 1 {
                    return Err("grouped Huffman selector");
                }
                if group_flags.len() * 2 > *count {
                    return Err("grouped flag count");
                }
                (symbols.len() == *count)
                    .then_some(())
                    .ok_or("grouped symbol count")
            }
            Self::Delta {
                count,
                config_count: payload_config_count,
                huffman_selector,
                values,
                ..
            } => {
                if *payload_config_count != config_count {
                    return Err("delta config count");
                }
                if *count > config_count {
                    return Err("delta active count");
                }
                if *huffman_selector > 3 {
                    return Err("delta Huffman selector");
                }
                (values.len() == config_count)
                    .then_some(())
                    .ok_or("delta value count")
            }
            Self::Previous {
                count,
                config_count: payload_config_count,
                huffman_selector,
                current_values,
                previous_values,
                tail_flags,
                ..
            } => {
                if *payload_config_count != config_count {
                    return Err("previous config count");
                }
                if *count > config_count {
                    return Err("previous active count");
                }
                if *huffman_selector > 3 {
                    return Err("previous Huffman selector");
                }
                if current_values.len() != config_count {
                    return Err("previous current-value count");
                }
                if previous_values.len() != config_count {
                    return Err("previous predictor count");
                }
                (tail_flags.len() == config_count)
                    .then_some(())
                    .ok_or("previous tail-flag count")
            }
        }
    }
}

impl IdsfEncodingSyntax {
    fn kind_for_dispatch(mode: u32, channel_index: u32) -> &'static str {
        match (mode & 3) + ((channel_index & 1) << 2) {
            0 | 4 => "raw",
            1 => "direct",
            2 => "grouped",
            3 => "delta",
            5 => "previous",
            6 => "progressive-previous",
            7 => "empty",
            _ => unreachable!("masked IDSF dispatch index"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw { .. } => "raw",
            Self::Direct { .. } => "direct",
            Self::Grouped { .. } => "grouped",
            Self::Delta { .. } => "delta",
            Self::Previous {
                progressive: false, ..
            } => "previous",
            Self::Previous {
                progressive: true, ..
            } => "progressive-previous",
            Self::Empty => "empty",
        }
    }

    fn validate(&self, quant_unit_count: usize) -> Result<(), &'static str> {
        match self {
            Self::Raw { values } => (values.len() == quant_unit_count)
                .then_some(())
                .ok_or("raw value count"),
            Self::Direct {
                mode_selector,
                prefix_count,
                residual_bits,
                count,
                values,
                ..
            } => {
                if *count != quant_unit_count {
                    return Err("direct quant-unit count");
                }
                if *mode_selector > 3 {
                    return Err("direct compact selector");
                }
                if *prefix_count > *count {
                    return Err("direct prefix count");
                }
                let max_residual_bits = if *mode_selector == 3 { 3 } else { 7 };
                if *residual_bits > max_residual_bits {
                    return Err("direct residual width");
                }
                (values.len() == *count)
                    .then_some(())
                    .ok_or("direct value count")
            }
            Self::Grouped {
                huffman_selector,
                count,
                symbols,
                ..
            } => {
                if *count != quant_unit_count {
                    return Err("grouped quant-unit count");
                }
                if *huffman_selector > 3 {
                    return Err("grouped Huffman selector");
                }
                (symbols.len() == *count)
                    .then_some(())
                    .ok_or("grouped symbol count")
            }
            Self::Delta {
                mode_selector,
                huffman_selector,
                count,
                values,
                ..
            } => {
                if *count != quant_unit_count {
                    return Err("delta quant-unit count");
                }
                if *mode_selector > 3 {
                    return Err("delta compact selector");
                }
                if *huffman_selector > 3 {
                    return Err("delta Huffman selector");
                }
                (values.len() == *count)
                    .then_some(())
                    .ok_or("delta value count")
            }
            Self::Previous {
                huffman_selector,
                count,
                current_values,
                previous_values,
                ..
            } => {
                if *count != quant_unit_count {
                    return Err("previous quant-unit count");
                }
                if *huffman_selector > 3 {
                    return Err("previous Huffman selector");
                }
                if current_values.len() != *count {
                    return Err("previous current-value count");
                }
                (previous_values.len() == *count)
                    .then_some(())
                    .ok_or("previous predictor count")
            }
            Self::Empty => Ok(()),
        }
    }
}

impl IdctEncodingSyntax {
    fn for_dispatch(mode: u32, channel_index: u32) -> Self {
        match (mode & 3) + ((channel_index & 1) << 2) {
            0 | 4 => Self::Fixed,
            1 | 5 => Self::Huffman,
            2 | 6 => Self::Delta,
            3 => Self::Empty,
            7 => Self::Previous { values: Vec::new() },
            _ => unreachable!("masked IDCT dispatch index"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Huffman => "huffman",
            Self::Delta => "delta",
            Self::Empty => "empty",
            Self::Previous { .. } => "previous",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitstream::frame::{BlockGroup, ObjectState, ObjectWindow};

    fn put_u32(window: &mut ObjectWindow, offset: usize, value: u32) {
        let start = offset - window.mem_offset;
        window.bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn reference_state() -> FramePrepackerState {
        let mut range_a = ObjectWindow::new(0, vec![0; 0x1110]);
        put_u32(&mut range_a, 0x1074, 1);
        put_u32(&mut range_a, 0x1078, 2);
        put_u32(&mut range_a, 0x107c, 12);
        put_u32(&mut range_a, 0x1080, 1);

        let mut range_b = ObjectWindow::new(0x1b480, vec![0; 0x1780]);
        put_u32(&mut range_b, 0x1b484, 1);
        put_u32(&mut range_b, 0x1c70c, 2);
        put_u32(&mut range_b, 0x1c73c, 1);

        let mut cfg = ObjectWindow::new(0, vec![0; 0x400]);
        for (offset, value) in [
            (0x90, 1),
            (0x94, 1),
            (0xa0, 2),
            (0xb0, 16),
            (0xc0, 16),
            (0xc4, 16),
            (0xc8, 16),
            (0x118, 1),
        ] {
            put_u32(&mut cfg, offset, value);
        }

        let mut gha_arena = ObjectWindow::new(0, vec![0; 8]);
        put_u32(&mut gha_arena, 0, 1);
        put_u32(&mut gha_arena, 4, 0);

        let object = ObjectState {
            channel_index: 0,
            range_a,
            range_b,
            cfg,
            previous_index: None,
            gainb: ObjectWindow::new(0, vec![0; 0xb00]),
            gha_arena,
            gha_p1: ObjectWindow::new(0, Vec::new()),
            gha_records: Vec::new(),
        };
        FramePrepackerState {
            frame_bytes: 2048,
            block_count: 1,
            groups: vec![BlockGroup {
                nblk: 1,
                objects: vec![object],
            }],
        }
    }

    #[test]
    fn reference_round_trip_preserves_the_complete_adapter_state() {
        let reference = reference_state();
        let syntax = FrameSyntax::from_reference(&reference).unwrap();
        assert_eq!(syntax.frame_bytes(), 2048);
        assert_eq!(syntax.groups()[0].header().quant_unit_count, 16);
        assert_eq!(
            syntax.groups()[0].channels()[0].idct.count,
            IdctCountSyntax::Explicit(12)
        );
        assert_eq!(syntax.to_reference().unwrap(), reference);
    }

    #[test]
    fn declared_group_count_is_validated() {
        let mut reference = reference_state();
        reference.block_count = 2;
        assert!(matches!(
            FrameSyntax::from_reference(&reference),
            Err(FrameSyntaxError::GroupCount {
                declared: 2,
                actual: 1
            })
        ));
    }

    #[test]
    fn idct_mode_requires_the_matching_typed_payload() {
        let reference = reference_state();
        let mut syntax = FrameSyntax::from_reference(&reference).unwrap();
        syntax.groups[0].channels[0].idct.encoding = IdctEncodingSyntax::Fixed;

        assert_eq!(
            syntax.validate(),
            Err(FrameSyntaxError::InvalidIdctEncoding {
                group: 0,
                channel: 0,
                expected: "delta",
                actual: "fixed",
            })
        );
    }

    #[test]
    fn idct_dispatch_mapping_covers_every_mode_and_channel_parity() {
        let expected = [
            "fixed", "huffman", "delta", "empty", "fixed", "huffman", "delta", "previous",
        ];
        for channel_index in 0..2 {
            for mode in 0..4 {
                let index = mode + channel_index * 4;
                assert_eq!(
                    IdctEncodingSyntax::for_dispatch(mode as u32, channel_index as u32).kind(),
                    expected[index]
                );
            }
        }
    }

    #[test]
    fn idwl_dispatch_mapping_covers_every_mode_and_channel_parity() {
        let expected = [
            "raw",
            "direct",
            "grouped",
            "delta",
            "raw",
            "previous",
            "progressive-previous",
            "delta",
        ];
        for channel_index in 0..2 {
            for mode in 0..4 {
                let index = mode + channel_index * 4;
                assert_eq!(
                    IdwlEncodingSyntax::kind_for_dispatch(mode as u32, channel_index as u32),
                    expected[index]
                );
            }
        }
    }

    #[test]
    fn idsf_dispatch_mapping_covers_every_mode_and_channel_parity() {
        let expected = [
            "raw",
            "direct",
            "grouped",
            "delta",
            "raw",
            "previous",
            "progressive-previous",
            "empty",
        ];
        for channel_index in 0..2 {
            for mode in 0..4 {
                let index = mode + channel_index * 4;
                assert_eq!(
                    IdsfEncodingSyntax::kind_for_dispatch(mode as u32, channel_index as u32),
                    expected[index]
                );
            }
        }
    }

    #[test]
    fn spectral_codebook_coordinates_cover_both_bandwidth_states() {
        for bandwidth in [false, true] {
            for selector in 0..8 {
                for word_length in 1..=7 {
                    let codebook = SpectralCodebookSyntax {
                        bandwidth,
                        selector,
                        word_length,
                    };
                    let slot = &SPECTRAL_DESCRIPTOR_SLOTS[codebook.slot_index()];
                    assert_eq!(slot.word_len as usize, word_length);
                    assert!(slot.metadata().is_some());
                }
            }
        }
    }
}
