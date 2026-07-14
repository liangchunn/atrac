//! Typed ATRAC3plus frame-syntax boundary.
//!
//! This first migration slice names the frame, group, channel, and section
//! control fields consumed by the packer. The retained reference backing is a
//! temporary parity adapter; payload-family fields move out of it section by
//! section before production packing switches to this type.

use crate::bitstream::frame::{FrameAssemblyError, FramePrepackerState};

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
    pub post_payload_gate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChannelSyntax {
    pub channel_index: u32,
    pub previous_channel: Option<usize>,
    pub idwl_mode: u32,
    pub idsf_mode: u32,
    pub idct: IdctSyntax,
    pub gain_present: bool,
    pub gha_present: bool,
    pub gha_idam_enabled: bool,
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
                post_payload_gate: source.cfg_u32(0x94)? != 0,
            };
            let channels = group
                .objects
                .iter()
                .take(group.nblk)
                .enumerate()
                .map(|(channel_index, object)| {
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
                    Ok(ChannelSyntax {
                        channel_index: object.channel_index,
                        previous_channel: object.previous_index,
                        idwl_mode: object.u32(0x1c70c)?,
                        idsf_mode: object.u32(0x1c73c)?,
                        idct: IdctSyntax {
                            bandwidth: object.u32(0x1074)? != 0,
                            mode,
                            bandwidth_mode: object.cfg_u32(0x90)? as usize,
                            count,
                            rows,
                            encoding,
                        },
                        gain_present: object.u32(0x1b484)? != 0,
                        gha_present: object.arena_u32(0)? != 0,
                        gha_idam_enabled: object.arena_u32(1)? == 0,
                    })
                })
                .collect::<Result<Vec<_>, FrameSyntaxError>>()?;
            groups.push(BlockGroupSyntax { header, channels });
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
            if syntax.header.quant_unit_count > MAX_QUANT_UNITS {
                return Err(FrameSyntaxError::InvalidQuantUnitCount {
                    group: group_index,
                    count: syntax.header.quant_unit_count,
                });
            }
            for (channel_index, channel) in syntax.channels.iter().enumerate() {
                for (section, mode) in [
                    ("idwl", channel.idwl_mode),
                    ("idsf", channel.idsf_mode),
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
}

impl ChannelSyntax {
    pub(crate) fn idct(&self) -> &IdctSyntax {
        &self.idct
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
}
