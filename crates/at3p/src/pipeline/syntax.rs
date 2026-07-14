//! Typed ATRAC3plus frame-syntax boundary.
//!
//! This first migration slice names the frame, group, channel, and section
//! control fields consumed by the packer. The retained reference backing is a
//! temporary parity adapter; payload-family fields move out of it section by
//! section before production packing switches to this type.

use crate::bitstream::frame::FrameAssemblyError;
#[cfg(any(test, debug_assertions))]
use crate::bitstream::frame::{BlockGroup, FramePrepackerState, ObjectState};
#[cfg(any(test, debug_assertions))]
use crate::tables::at5::isps_at5;
use crate::tables::at5::nsps_at5;
use crate::tables::generated::{G_A_IDSPCBANDS_AT5, G_A_IDSPCQUS_AT5};
use crate::tables::spectral::SPECTRAL_DESCRIPTOR_SLOTS;

const MAX_QUANT_UNITS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameSyntax {
    frame_bytes: usize,
    groups: Vec<BlockGroupSyntax>,
    #[cfg(any(test, debug_assertions))]
    reference_backing: Option<FramePrepackerState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockGroupSyntax {
    header: BlockHeaderSyntax,
    channels: Vec<ChannelSyntax>,
    stereo: Option<StereoSyntax>,
    post_payload: Option<[u8; 2]>,
    gha: GhaSyntax,
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
    pub gain: GainSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GhaSyntax {
    Absent,
    Present(GhaPayloadSyntax),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaPayloadSyntax {
    pub header_mode: u32,
    pub band_count: usize,
    pub stereo_flags: Option<[GatedFlagsSyntax; 3]>,
    pub channels: Vec<GhaChannelSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaChannelSyntax {
    pub channel_index: u32,
    pub records: Vec<GhaRecordSyntax>,
    pub idloc_mode: u32,
    pub nwavs: GhaNwavsSyntax,
    pub freq: GhaFreqSyntax,
    pub idsf: GhaIdsfSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaRecordSyntax {
    pub active: bool,
    pub first_location: Option<u32>,
    pub second_location: Option<u32>,
    pub waves: Vec<GhaWaveSyntax>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GhaWaveSyntax {
    pub idsf: u32,
    pub phase: u32,
    pub freq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaNwavsSyntax {
    pub mode: u32,
    pub encoding: GhaNwavsEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GhaNwavsEncodingSyntax {
    Raw,
    Huffman,
    Previous { counts: Vec<u32> },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaFreqSyntax {
    pub mode: u32,
    pub encoding: GhaFreqEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GhaFreqEncodingSyntax {
    Local { modes: Vec<u32> },
    Previous { rows: Vec<Vec<u32>> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhaIdsfSyntax {
    pub mode: u32,
    pub encoding: GhaIdsfEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GhaIdsfEncodingSyntax {
    Raw,
    Huffman,
    Previous {
        rows: Vec<Vec<u32>>,
        indices: Vec<Vec<i32>>,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GainSyntax {
    Absent,
    Present(GainPayloadSyntax),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GainPayloadSyntax {
    pub band_count: Option<usize>,
    pub rows: Vec<GainRowSyntax>,
    pub ngc: GainNgcSyntax,
    pub idlev: GainIdlevSyntax,
    pub idloc: GainIdlocSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GainRowSyntax {
    pub count: u32,
    pub locations: Vec<u32>,
    pub levels: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GainNgcSyntax {
    pub mode: u32,
    pub encoding: GainNgcEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GainNgcEncodingSyntax {
    Raw,
    Huffman,
    Delta,
    Direct { bit_width: u8, base: i32 },
    Previous { counts: Vec<u32> },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GainIdlevSyntax {
    pub mode: u32,
    pub encoding: GainIdlevEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GainIdlevEncodingSyntax {
    Raw,
    Delta,
    RowDelta,
    Direct { bit_width: u8, base: i32 },
    Previous { levels: Vec<Vec<u32>> },
    Flagged { flags: Vec<u32> },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GainIdlocSyntax {
    pub mode: u32,
    pub encoding: GainIdlocEncodingSyntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GainIdlocEncodingSyntax {
    Raw,
    LevelAdaptive,
    RowAdaptive,
    Direct {
        bit_width: u8,
        base: i32,
    },
    Previous {
        locations: Vec<Vec<u32>>,
    },
    PreviousFlagged {
        locations: Vec<Vec<u32>>,
        flags: Vec<u32>,
    },
    PreviousRawFlagged {
        locations: Vec<Vec<u32>>,
        flags: Vec<u32>,
    },
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
    InvalidGainPayload {
        group: usize,
        channel: usize,
        detail: &'static str,
    },
    InvalidGhaPayload {
        group: usize,
        detail: &'static str,
    },
    UnsupportedGhaIdam {
        group: usize,
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
    MissingReferenceBacking,
}

impl From<FrameAssemblyError> for FrameSyntaxError {
    fn from(value: FrameAssemblyError) -> Self {
        Self::Reference(value)
    }
}

impl FrameSyntax {
    #[cfg(any(test, debug_assertions))]
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
                        gain: gain_syntax_from_reference(
                            group,
                            object,
                            group_index,
                            channel_index,
                        )?,
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
            let gha = gha_syntax_from_reference(group, source, group_index)?;
            groups.push(BlockGroupSyntax {
                header,
                channels,
                stereo,
                post_payload,
                gha,
            });
        }
        let syntax = Self {
            frame_bytes: reference.frame_bytes,
            groups,
            reference_backing: Some(reference.clone()),
        };
        if syntax.groups.len() != reference.block_count {
            return Err(FrameSyntaxError::GroupCount {
                declared: reference.block_count,
                actual: syntax.groups.len(),
            });
        }
        syntax.validate()?;
        Ok(syntax)
    }

    pub(crate) fn from_parts(
        frame_bytes: usize,
        groups: Vec<BlockGroupSyntax>,
    ) -> Result<Self, FrameSyntaxError> {
        let syntax = Self {
            frame_bytes,
            groups,
            #[cfg(any(test, debug_assertions))]
            reference_backing: None,
        };
        syntax.validate()?;
        Ok(syntax)
    }

    pub(crate) fn validate(&self) -> Result<(), FrameSyntaxError> {
        if self.frame_bytes == 0 {
            return Err(FrameSyntaxError::EmptyFrame);
        }
        for (group_index, syntax) in self.groups.iter().enumerate() {
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
            if let Err(detail) = syntax.gha.validate(syntax.channels.len()) {
                return Err(FrameSyntaxError::InvalidGhaPayload {
                    group: group_index,
                    detail,
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
                if let Err(detail) = channel.gain.validate(channel.channel_index) {
                    return Err(FrameSyntaxError::InvalidGainPayload {
                        group: group_index,
                        channel: channel_index,
                        detail,
                    });
                }
            }
        }
        Ok(())
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn to_reference(&self) -> Result<FramePrepackerState, FrameSyntaxError> {
        self.validate()?;
        self.reference_backing
            .clone()
            .ok_or(FrameSyntaxError::MissingReferenceBacking)
    }

    pub(crate) fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }

    pub(crate) fn groups(&self) -> &[BlockGroupSyntax] {
        &self.groups
    }
}

impl BlockGroupSyntax {
    pub(crate) fn new(
        header: BlockHeaderSyntax,
        channels: Vec<ChannelSyntax>,
        stereo: Option<StereoSyntax>,
        post_payload: Option<[u8; 2]>,
        gha: GhaSyntax,
    ) -> Self {
        Self {
            header,
            channels,
            stereo,
            post_payload,
            gha,
        }
    }

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

    pub(crate) fn gha(&self) -> &GhaSyntax {
        &self.gha
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

    pub(crate) fn gain(&self) -> &GainSyntax {
        &self.gain
    }
}

impl GainSyntax {
    fn validate(&self, channel_index: u32) -> Result<(), &'static str> {
        let Self::Present(payload) = self else {
            return Ok(());
        };
        if !(1..=16).contains(&payload.rows.len()) {
            return Err("row count");
        }
        if payload
            .band_count
            .is_some_and(|count| !(1..=16).contains(&count))
        {
            return Err("band count");
        }
        for row in &payload.rows {
            let points = row.count.min(7) as usize;
            if row.locations.len() != points || row.levels.len() != points {
                return Err("gain-point count");
            }
            if row.count > 7 {
                return Err("gain-point limit");
            }
            if row.locations.iter().any(|value| *value > 31) {
                return Err("gain location width");
            }
            if row.levels.iter().any(|value| *value > 15) {
                return Err("gain level width");
            }
        }
        for (section, mode) in [
            ("NGC", payload.ngc.mode),
            ("IDLEV", payload.idlev.mode),
            ("IDLOC", payload.idloc.mode),
        ] {
            if mode > 3 {
                return Err(section);
            }
        }
        if payload.ngc.encoding.kind()
            != GainNgcEncodingSyntax::kind_for_dispatch(payload.ngc.mode, channel_index)
        {
            return Err("NGC mode/payload agreement");
        }
        if payload.idlev.encoding.kind()
            != GainIdlevEncodingSyntax::kind_for_dispatch(payload.idlev.mode, channel_index)
        {
            return Err("IDLEV mode/payload agreement");
        }
        if payload.idloc.encoding.kind()
            != GainIdlocEncodingSyntax::kind_for_dispatch(payload.idloc.mode, channel_index)
        {
            return Err("IDLOC mode/payload agreement");
        }
        let row_count = payload.rows.len();
        if let GainNgcEncodingSyntax::Previous { counts } = &payload.ngc.encoding
            && counts.len() != row_count
        {
            return Err("NGC predictor count");
        }
        if let GainNgcEncodingSyntax::Direct { bit_width, .. } = &payload.ngc.encoding
            && *bit_width > 3
        {
            return Err("NGC direct width");
        }
        match &payload.idlev.encoding {
            GainIdlevEncodingSyntax::Previous { levels } if levels.len() != row_count => {
                return Err("IDLEV predictor rows");
            }
            GainIdlevEncodingSyntax::Flagged { flags } if flags.len() != row_count => {
                return Err("IDLEV flag count");
            }
            GainIdlevEncodingSyntax::Flagged { flags } if flags.iter().any(|flag| *flag > 1) => {
                return Err("IDLEV flag width");
            }
            GainIdlevEncodingSyntax::Direct { bit_width, .. } if *bit_width > 3 => {
                return Err("IDLEV direct width");
            }
            _ => {}
        }
        match &payload.idloc.encoding {
            GainIdlocEncodingSyntax::Previous { locations } if locations.len() != row_count => {
                return Err("IDLOC predictor rows");
            }
            GainIdlocEncodingSyntax::PreviousFlagged { locations, flags }
            | GainIdlocEncodingSyntax::PreviousRawFlagged { locations, flags }
                if locations.len() != row_count || flags.len() != row_count =>
            {
                return Err("IDLOC predictor/flag rows");
            }
            GainIdlocEncodingSyntax::PreviousFlagged { flags, .. }
            | GainIdlocEncodingSyntax::PreviousRawFlagged { flags, .. }
                if flags.iter().any(|flag| *flag > 1) =>
            {
                return Err("IDLOC flag width");
            }
            GainIdlocEncodingSyntax::Direct { bit_width, .. } if !(1..=4).contains(bit_width) => {
                return Err("IDLOC direct width");
            }
            _ => {}
        }
        Ok(())
    }
}

impl GainNgcEncodingSyntax {
    fn kind_for_dispatch(mode: u32, channel_index: u32) -> &'static str {
        match (mode & 3) + ((channel_index & 1) << 2) {
            0 | 4 => "raw",
            1 | 5 => "huffman",
            2 => "delta",
            3 => "direct",
            6 => "previous",
            7 => "empty",
            _ => unreachable!("masked gain NGC dispatch index"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Huffman => "huffman",
            Self::Delta => "delta",
            Self::Direct { .. } => "direct",
            Self::Previous { .. } => "previous",
            Self::Empty => "empty",
        }
    }
}

impl GainIdlevEncodingSyntax {
    fn kind_for_dispatch(mode: u32, channel_index: u32) -> &'static str {
        match (mode & 3) + ((channel_index & 1) << 2) {
            0 | 4 => "raw",
            1 => "delta",
            2 => "row-delta",
            3 => "direct",
            5 => "previous",
            6 => "flagged",
            7 => "empty",
            _ => unreachable!("masked gain IDLEV dispatch index"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Delta => "delta",
            Self::RowDelta => "row-delta",
            Self::Direct { .. } => "direct",
            Self::Previous { .. } => "previous",
            Self::Flagged { .. } => "flagged",
            Self::Empty => "empty",
        }
    }
}

impl GainIdlocEncodingSyntax {
    fn kind_for_dispatch(mode: u32, channel_index: u32) -> &'static str {
        match (mode & 3) + ((channel_index & 1) << 2) {
            0 | 4 => "raw",
            1 => "level-adaptive",
            2 => "row-adaptive",
            3 => "direct",
            5 => "previous",
            6 => "previous-flagged",
            7 => "previous-raw-flagged",
            _ => unreachable!("masked gain IDLOC dispatch index"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::LevelAdaptive => "level-adaptive",
            Self::RowAdaptive => "row-adaptive",
            Self::Direct { .. } => "direct",
            Self::Previous { .. } => "previous",
            Self::PreviousFlagged { .. } => "previous-flagged",
            Self::PreviousRawFlagged { .. } => "previous-raw-flagged",
        }
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

impl GhaSyntax {
    fn validate(&self, channel_count: usize) -> Result<(), &'static str> {
        let Self::Present(payload) = self else {
            return Ok(());
        };
        if payload.header_mode != 1 {
            return Err("header mode");
        }
        if !(1..=16).contains(&payload.band_count) {
            return Err("band count");
        }
        match (&payload.stereo_flags, channel_count) {
            (Some(flags), 2) => {
                for flag_group in flags {
                    flag_group.validate(payload.band_count)?;
                }
            }
            (None, 1) => {}
            _ => return Err("stereo flag agreement"),
        }
        if payload.channels.len() != channel_count {
            return Err("channel count");
        }
        for channel in &payload.channels {
            if channel.channel_index > 1 {
                return Err("channel index");
            }
            if channel.records.len() != payload.band_count {
                return Err("record count");
            }
            if channel.idloc_mode > 1 || channel.freq.mode > 1 {
                return Err("one-bit mode width");
            }
            if channel.nwavs.mode > 3 || channel.idsf.mode > 3 {
                return Err("two-bit mode width");
            }
            for record in &channel.records {
                if record
                    .first_location
                    .into_iter()
                    .chain(record.second_location)
                    .any(|location| location > 31)
                {
                    return Err("location width");
                }
                if record.waves.len() > 15 {
                    return Err("wave count width");
                }
                for wave in &record.waves {
                    if wave.phase > 31 || wave.freq > 1023 || wave.idsf > 63 {
                        return Err("wave field width");
                    }
                }
            }
            if channel.nwavs.encoding.kind()
                != GhaNwavsEncodingSyntax::kind_for_mode(channel.nwavs.mode)
            {
                return Err("NWAVS mode/payload agreement");
            }
            if channel.freq.encoding.kind()
                != GhaFreqEncodingSyntax::kind_for_mode(channel.freq.mode)
            {
                return Err("FREQ mode/payload agreement");
            }
            if channel.idsf.encoding.kind()
                != GhaIdsfEncodingSyntax::kind_for_mode(channel.idsf.mode)
            {
                return Err("IDSF mode/payload agreement");
            }
            if let GhaNwavsEncodingSyntax::Previous { counts } = &channel.nwavs.encoding
                && counts.len() != payload.band_count
            {
                return Err("NWAVS predictor rows");
            }
            match &channel.freq.encoding {
                GhaFreqEncodingSyntax::Local { modes }
                    if modes.len() != payload.band_count || modes.iter().any(|mode| *mode > 1) =>
                {
                    return Err("FREQ local modes");
                }
                GhaFreqEncodingSyntax::Previous { rows } if rows.len() != payload.band_count => {
                    return Err("FREQ predictor rows");
                }
                _ => {}
            }
            if let GhaIdsfEncodingSyntax::Previous { rows, indices } = &channel.idsf.encoding {
                if rows.len() != payload.band_count || indices.len() != payload.band_count {
                    return Err("IDSF predictor rows");
                }
                for (record, index_row) in channel.records.iter().zip(indices) {
                    if record.active && index_row.len() != record.waves.len() {
                        return Err("IDSF predictor indices");
                    }
                }
            }
        }
        Ok(())
    }
}

impl GhaNwavsEncodingSyntax {
    fn kind_for_mode(mode: u32) -> &'static str {
        ["raw", "huffman", "previous", "empty"][(mode & 3) as usize]
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Huffman => "huffman",
            Self::Previous { .. } => "previous",
            Self::Empty => "empty",
        }
    }
}

impl GhaFreqEncodingSyntax {
    fn kind_for_mode(mode: u32) -> &'static str {
        ["local", "previous"][(mode & 1) as usize]
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Previous { .. } => "previous",
        }
    }
}

impl GhaIdsfEncodingSyntax {
    fn kind_for_mode(mode: u32) -> &'static str {
        ["raw", "huffman", "previous", "empty"][(mode & 3) as usize]
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Huffman => "huffman",
            Self::Previous { .. } => "previous",
            Self::Empty => "empty",
        }
    }
}

#[cfg(any(test, debug_assertions))]
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

#[cfg(any(test, debug_assertions))]
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

#[cfg(any(test, debug_assertions))]
fn gain_syntax_from_reference(
    group: &BlockGroup,
    object: &ObjectState,
    group_index: usize,
    channel_index: usize,
) -> Result<GainSyntax, FrameSyntaxError> {
    if object.u32(0x1b484)? == 0 {
        return Ok(GainSyntax::Absent);
    }

    let row_count = object.u32(0x1b490)? as usize;
    let rows = gain_rows_from_reference(object, row_count)?;
    let previous_rows = || -> Result<Vec<GainRowSyntax>, FrameSyntaxError> {
        let previous = object
            .previous_index
            .and_then(|index| group.objects.get(index))
            .ok_or(FrameSyntaxError::MissingPreviousChannel {
                group: group_index,
                channel: channel_index,
            })?;
        Ok(gain_rows_from_reference(previous, row_count)?)
    };
    let parity = object.channel_index & 1;

    let ngc_mode = object.u32(0x1b494)?;
    let ngc_dispatch = (ngc_mode & 3) + (parity << 2);
    let ngc_encoding = match ngc_dispatch {
        0 | 4 => GainNgcEncodingSyntax::Raw,
        1 | 5 => GainNgcEncodingSyntax::Huffman,
        2 => GainNgcEncodingSyntax::Delta,
        3 => GainNgcEncodingSyntax::Direct {
            bit_width: object.u32(0x1b498)? as u8,
            base: object.u32(0x1b49c)? as i32,
        },
        6 => GainNgcEncodingSyntax::Previous {
            counts: previous_rows()?.into_iter().map(|row| row.count).collect(),
        },
        7 => GainNgcEncodingSyntax::Empty,
        _ => unreachable!("masked gain NGC dispatch index"),
    };

    let idlev_mode = object.u32(0x1b4a0)?;
    let idlev_dispatch = (idlev_mode & 3) + (parity << 2);
    let idlev_encoding = match idlev_dispatch {
        0 | 4 => GainIdlevEncodingSyntax::Raw,
        1 => GainIdlevEncodingSyntax::Delta,
        2 => GainIdlevEncodingSyntax::RowDelta,
        3 => GainIdlevEncodingSyntax::Direct {
            bit_width: object.u32(0x1b4a4)? as u8,
            base: object.u32(0x1b4a8)? as i32,
        },
        5 => GainIdlevEncodingSyntax::Previous {
            levels: previous_rows()?.into_iter().map(|row| row.levels).collect(),
        },
        6 => GainIdlevEncodingSyntax::Flagged {
            flags: object.u32_array(0x1b4ac, row_count)?,
        },
        7 => GainIdlevEncodingSyntax::Empty,
        _ => unreachable!("masked gain IDLEV dispatch index"),
    };

    let idloc_mode = object.u32(0x1b4ec)?;
    let idloc_dispatch = (idloc_mode & 3) + (parity << 2);
    let idloc_encoding = match idloc_dispatch {
        0 | 4 => GainIdlocEncodingSyntax::Raw,
        1 => GainIdlocEncodingSyntax::LevelAdaptive,
        2 => GainIdlocEncodingSyntax::RowAdaptive,
        3 => GainIdlocEncodingSyntax::Direct {
            bit_width: object.u32(0x1b4f0)? as u8,
            base: object.u32(0x1b4f4)? as i32,
        },
        5 => GainIdlocEncodingSyntax::Previous {
            locations: previous_rows()?
                .into_iter()
                .map(|row| row.locations)
                .collect(),
        },
        6 => GainIdlocEncodingSyntax::PreviousFlagged {
            locations: previous_rows()?
                .into_iter()
                .map(|row| row.locations)
                .collect(),
            flags: object.u32_array(0x1b4f8, row_count)?,
        },
        7 => GainIdlocEncodingSyntax::PreviousRawFlagged {
            locations: previous_rows()?
                .into_iter()
                .map(|row| row.locations)
                .collect(),
            flags: object.u32_array(0x1b538, row_count)?,
        },
        _ => unreachable!("masked gain IDLOC dispatch index"),
    };

    let band_count = (object.u32(0x1b488)? != 0)
        .then(|| object.u32(0x1b48c).map(|value| value as usize))
        .transpose()?;
    Ok(GainSyntax::Present(GainPayloadSyntax {
        band_count,
        rows,
        ngc: GainNgcSyntax {
            mode: ngc_mode,
            encoding: ngc_encoding,
        },
        idlev: GainIdlevSyntax {
            mode: idlev_mode,
            encoding: idlev_encoding,
        },
        idloc: GainIdlocSyntax {
            mode: idloc_mode,
            encoding: idloc_encoding,
        },
    }))
}

#[cfg(any(test, debug_assertions))]
fn gain_rows_from_reference(
    object: &ObjectState,
    count: usize,
) -> Result<Vec<GainRowSyntax>, FrameAssemblyError> {
    (0..count)
        .map(|row| {
            let base = row * 0x98;
            let point_count = object.gainb_u32(base)?;
            let points = point_count.min(7) as usize;
            let locations = (0..points)
                .map(|point| object.gainb_u32(base + 0x4 + point * 4))
                .collect::<Result<Vec<_>, _>>()?;
            let levels = (0..points)
                .map(|point| object.gainb_u32(base + 0x20 + point * 4))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(GainRowSyntax {
                count: point_count,
                locations,
                levels,
            })
        })
        .collect()
}

#[cfg(any(test, debug_assertions))]
fn gha_syntax_from_reference(
    group: &BlockGroup,
    source: &ObjectState,
    group_index: usize,
) -> Result<GhaSyntax, FrameSyntaxError> {
    if source.arena_u32(0)? == 0 {
        return Ok(GhaSyntax::Absent);
    }
    let header_mode = source.arena_u32(1)?;
    if header_mode == 0 {
        return Err(FrameSyntaxError::UnsupportedGhaIdam { group: group_index });
    }
    let band_count = source.arena_u32(2)? as usize;
    let stereo_flags = if group.nblk == 2 {
        Some([
            gated_flags_from_arena(source, 0xc4, 0xc5, 0xc6, band_count)?,
            gated_flags_from_arena(source, 0xe8, 0xe9, 0xea, band_count)?,
            gated_flags_from_arena(source, 0xd6, 0xd7, 0xd8, band_count)?,
        ])
    } else {
        None
    };
    let channels = group
        .objects
        .iter()
        .take(group.nblk)
        .enumerate()
        .map(|(channel_position, object)| {
            let active = (0..object.gha_records.len())
                .map(|record| object.u32(0x1c7b0 + record * 4).map(|value| value != 0))
                .collect::<Result<Vec<_>, _>>()?;
            let records = object
                .gha_records
                .iter()
                .enumerate()
                .map(|(record, waves)| {
                    let word = record * 10;
                    Ok(GhaRecordSyntax {
                        active: active[record],
                        first_location: (object.p1_u32(word + 5)? != 0)
                            .then(|| object.p1_u32(word + 7))
                            .transpose()?,
                        second_location: (object.p1_u32(word + 6)? != 0)
                            .then(|| object.p1_u32(word + 8))
                            .transpose()?,
                        waves: waves
                            .iter()
                            .map(|wave| GhaWaveSyntax {
                                idsf: wave.idsf,
                                phase: wave.phase,
                                freq: wave.freq,
                            })
                            .collect(),
                    })
                })
                .collect::<Result<Vec<_>, FrameAssemblyError>>()?;
            let previous = || -> Result<&ObjectState, FrameSyntaxError> {
                object
                    .previous_index
                    .and_then(|index| group.objects.get(index))
                    .ok_or(FrameSyntaxError::MissingPreviousChannel {
                        group: group_index,
                        channel: channel_position,
                    })
            };

            let nwavs_mode = object.u32(0x1c760)?;
            let nwavs_encoding = match nwavs_mode & 3 {
                0 => GhaNwavsEncodingSyntax::Raw,
                1 => GhaNwavsEncodingSyntax::Huffman,
                2 => GhaNwavsEncodingSyntax::Previous {
                    counts: previous()?
                        .gha_records
                        .iter()
                        .map(|waves| waves.len() as u32)
                        .collect(),
                },
                3 => GhaNwavsEncodingSyntax::Empty,
                _ => unreachable!("masked GHA NWAVS mode"),
            };

            let freq_mode = object.u32(0x1c764)?;
            let freq_encoding = if freq_mode & 1 == 0 {
                GhaFreqEncodingSyntax::Local {
                    modes: (0..records.len())
                        .map(|record| object.u32(0x1c770 + record * 4))
                        .collect::<Result<Vec<_>, _>>()?,
                }
            } else {
                GhaFreqEncodingSyntax::Previous {
                    rows: previous()?
                        .gha_records
                        .iter()
                        .map(|waves| waves.iter().map(|wave| wave.freq).collect())
                        .collect(),
                }
            };

            let idsf_mode = object.u32(0x1c768)?;
            let idsf_encoding = match idsf_mode & 3 {
                0 => GhaIdsfEncodingSyntax::Raw,
                1 => GhaIdsfEncodingSyntax::Huffman,
                2 => GhaIdsfEncodingSyntax::Previous {
                    rows: previous()?
                        .gha_records
                        .iter()
                        .map(|waves| waves.iter().map(|wave| wave.idsf).collect())
                        .collect(),
                    indices: gha_predictor_indices_from_reference(object, &active)?,
                },
                3 => GhaIdsfEncodingSyntax::Empty,
                _ => unreachable!("masked GHA IDSF mode"),
            };

            Ok(GhaChannelSyntax {
                channel_index: object.channel_index,
                records,
                idloc_mode: object.u32(0x1c75c)?,
                nwavs: GhaNwavsSyntax {
                    mode: nwavs_mode,
                    encoding: nwavs_encoding,
                },
                freq: GhaFreqSyntax {
                    mode: freq_mode,
                    encoding: freq_encoding,
                },
                idsf: GhaIdsfSyntax {
                    mode: idsf_mode,
                    encoding: idsf_encoding,
                },
            })
        })
        .collect::<Result<Vec<_>, FrameSyntaxError>>()?;
    Ok(GhaSyntax::Present(GhaPayloadSyntax {
        header_mode,
        band_count,
        stereo_flags,
        channels,
    }))
}

#[cfg(any(test, debug_assertions))]
fn gated_flags_from_arena(
    object: &ObjectState,
    present_index: usize,
    flags_present_index: usize,
    flags_index: usize,
    count: usize,
) -> Result<GatedFlagsSyntax, FrameAssemblyError> {
    if object.arena_u32(present_index)? == 0 {
        return Ok(GatedFlagsSyntax::Absent);
    }
    if object.arena_u32(flags_present_index)? == 0 {
        return Ok(GatedFlagsSyntax::PresentWithoutFlags);
    }
    let flags = (0..count)
        .map(|index| {
            object
                .arena_u32(flags_index + index)
                .map(|value| value != 0)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GatedFlagsSyntax::Present { flags })
}

#[cfg(any(test, debug_assertions))]
fn gha_predictor_indices_from_reference(
    object: &ObjectState,
    active: &[bool],
) -> Result<Vec<Vec<i32>>, FrameAssemblyError> {
    let mut rows = Vec::with_capacity(object.gha_records.len());
    let mut base = 0;
    for (record, waves) in object.gha_records.iter().enumerate() {
        if !active[record] {
            rows.push(Vec::new());
            continue;
        }
        rows.push(
            (0..waves.len())
                .map(|wave| {
                    object
                        .cfg_u32(0x11c + (base + wave) * 4)
                        .map(|value| value as i32)
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        base += waves.len();
    }
    Ok(rows)
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
        put_u32(&mut range_b, 0x1b484, 0);
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
        put_u32(&mut gha_arena, 0, 0);
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
    fn owned_syntax_does_not_require_native_layout_backing() {
        let reference = reference_state();
        let adapted = FrameSyntax::from_reference(&reference).unwrap();
        let owned = FrameSyntax::from_parts(adapted.frame_bytes, adapted.groups.clone()).unwrap();

        assert_eq!(owned.groups(), adapted.groups());
        assert_eq!(
            owned.to_reference(),
            Err(FrameSyntaxError::MissingReferenceBacking)
        );
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

    #[test]
    fn gain_dispatch_mappings_cover_every_mode_and_channel_parity() {
        let ngc = [
            "raw", "huffman", "delta", "direct", "raw", "huffman", "previous", "empty",
        ];
        let idlev = [
            "raw",
            "delta",
            "row-delta",
            "direct",
            "raw",
            "previous",
            "flagged",
            "empty",
        ];
        let idloc = [
            "raw",
            "level-adaptive",
            "row-adaptive",
            "direct",
            "raw",
            "previous",
            "previous-flagged",
            "previous-raw-flagged",
        ];
        for channel_index in 0..2 {
            for mode in 0..4 {
                let index = mode + channel_index * 4;
                assert_eq!(
                    GainNgcEncodingSyntax::kind_for_dispatch(mode as u32, channel_index as u32),
                    ngc[index]
                );
                assert_eq!(
                    GainIdlevEncodingSyntax::kind_for_dispatch(mode as u32, channel_index as u32),
                    idlev[index]
                );
                assert_eq!(
                    GainIdlocEncodingSyntax::kind_for_dispatch(mode as u32, channel_index as u32),
                    idloc[index]
                );
            }
        }
    }

    #[test]
    fn gha_mode_mappings_cover_every_payload_family() {
        for (mode, expected) in ["raw", "huffman", "previous", "empty"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(GhaNwavsEncodingSyntax::kind_for_mode(mode as u32), expected);
            assert_eq!(GhaIdsfEncodingSyntax::kind_for_mode(mode as u32), expected);
        }
        assert_eq!(GhaFreqEncodingSyntax::kind_for_mode(0), "local");
        assert_eq!(GhaFreqEncodingSyntax::kind_for_mode(1), "previous");
    }
}
