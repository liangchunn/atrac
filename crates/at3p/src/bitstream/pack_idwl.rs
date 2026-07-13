use super::huffman::{HuffmanEmitError, emit_symbol};
use super::writer::{BitWriter, BitWriterError};
use crate::tables::huffman::wlc_descriptors;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idwl1Fields<'a> {
    pub channel_flag: u32,
    pub selector_a: u32,
    pub selector_b: u32,
    pub count: usize,
    pub mode3_value: u32,
    pub prefix_count: usize,
    pub residual_bits: u8,
    pub residual_base: u32,
    pub values: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idwl4Fields<'a> {
    pub channel_flag: u32,
    pub selector_b: u32,
    pub count: usize,
    pub config_count: usize,
    pub mode3_value: u32,
    pub huffman_selector: usize,
    pub current_values: &'a [u32],
    pub previous_values: &'a [u32],
    pub tail_flags: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idwl3Fields<'a> {
    pub channel_flag: u32,
    pub selector_a: u32,
    pub selector_b: u32,
    pub count: usize,
    pub config_count: usize,
    pub mode3_value: u32,
    pub huffman_selector: usize,
    pub values: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idwl2Fields<'a> {
    pub channel_flag: u32,
    pub selector_b: u32,
    pub count: usize,
    pub mode3_value: u32,
    pub subgroup_flag: u32,
    pub huffman_selector: usize,
    pub field_3bits: u32,
    pub field_4bits: u32,
    pub group_flags: &'a [u32],
    pub symbols: &'a [u32],
}

pub type Idwl5Fields<'a> = Idwl4Fields<'a>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackIdwlError {
    MissingValue {
        index: usize,
        values: usize,
    },
    MissingPreviousValue {
        index: usize,
        previous_values: usize,
    },
    MissingTailFlag {
        index: usize,
        tail_flags: usize,
    },
    MissingGroupFlag {
        index: usize,
        group_flags: usize,
    },
    UnsupportedWlcSelector {
        selector: usize,
    },
    Huffman(HuffmanEmitError),
    BitWriter(BitWriterError),
}

impl From<BitWriterError> for PackIdwlError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}

impl From<HuffmanEmitError> for PackIdwlError {
    fn from(error: HuffmanEmitError) -> Self {
        Self::Huffman(error)
    }
}

pub fn pack_idwl_0_at5(
    writer: &mut BitWriter<'_>,
    word_lengths: &[u32],
) -> Result<(), BitWriterError> {
    for word_length in word_lengths {
        writer.write_bits(*word_length, 3)?;
    }
    Ok(())
}

pub fn pack_idwl_1_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idwl1Fields<'_>,
) -> Result<(), PackIdwlError> {
    writer.write_bits(fields.selector_a, 2)?;
    writer.write_bits(fields.selector_b, 2)?;

    if fields.selector_b != 0 {
        writer.write_bits(fields.count as u32, 5)?;
        if fields.selector_b == 3 {
            let mode3_bias = if fields.channel_flag == 0 { 1 } else { 3 };
            writer.write_bits(fields.mode3_value.wrapping_sub(mode3_bias), 2)?;
        }
    }

    if fields.count != 0 {
        writer.write_bits(fields.prefix_count as u32, 5)?;
        writer.write_bits(u32::from(fields.residual_bits), 2)?;
        writer.write_bits(fields.residual_base, 3)?;

        for index in 0..fields.prefix_count {
            let value = fields
                .values
                .get(index)
                .ok_or(PackIdwlError::MissingValue {
                    index,
                    values: fields.values.len(),
                })?;
            writer.write_bits(*value, 3)?;
        }

        if fields.residual_bits != 0 {
            for index in fields.prefix_count..fields.count {
                let value = fields
                    .values
                    .get(index)
                    .ok_or(PackIdwlError::MissingValue {
                        index,
                        values: fields.values.len(),
                    })?;
                writer.write_bits(
                    value.wrapping_sub(fields.residual_base),
                    fields.residual_bits,
                )?;
            }
        }
    }

    Ok(())
}

pub fn pack_idwl_2_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idwl2Fields<'_>,
) -> Result<(), PackIdwlError> {
    writer.write_bits(fields.selector_b, 2)?;

    if fields.selector_b != 0 {
        writer.write_bits(fields.count as u32, 5)?;
        if fields.selector_b == 3 {
            let mode3_bias = if fields.channel_flag == 0 { 1 } else { 3 };
            writer.write_bits(fields.mode3_value.wrapping_sub(mode3_bias), 2)?;
        }
    }

    if fields.count != 0 {
        writer.write_bits(fields.subgroup_flag, 1)?;
        writer.write_bits(fields.huffman_selector as u32, 1)?;
        writer.write_bits(fields.field_3bits, 3)?;
        writer.write_bits(fields.field_4bits, 4)?;

        let descriptor = wlc_descriptors()
            .get(fields.huffman_selector)
            .copied()
            .ok_or(PackIdwlError::UnsupportedWlcSelector {
                selector: fields.huffman_selector,
            })?;

        if fields.subgroup_flag == 0 {
            for index in 0..fields.count {
                let symbol = idwl_symbol(fields.symbols, index)?;
                emit_symbol(writer, descriptor, symbol as usize)?;
            }
        } else {
            for (group_index, group_flag) in fields.group_flags.iter().enumerate() {
                writer.write_bits(*group_flag, 1)?;

                if *group_flag == 0 {
                    for index in (group_index * 2)..(group_index * 2 + 2) {
                        let symbol = idwl_symbol(fields.symbols, index)?;
                        emit_symbol(writer, descriptor, symbol as usize)?;
                    }
                }
            }

            for index in (fields.group_flags.len() * 2)..fields.count {
                let symbol = idwl_symbol(fields.symbols, index)?;
                emit_symbol(writer, descriptor, symbol as usize)?;
            }
        }
    }

    Ok(())
}

pub fn pack_idwl_3_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idwl3Fields<'_>,
) -> Result<(), PackIdwlError> {
    writer.write_bits(fields.selector_a, 2)?;
    writer.write_bits(fields.selector_b, 2)?;

    if fields.selector_b != 0 {
        writer.write_bits(fields.count as u32, 5)?;
        if fields.selector_b == 3 {
            let mode3_bias = if fields.channel_flag == 0 { 1 } else { 3 };
            writer.write_bits(fields.mode3_value.wrapping_sub(mode3_bias), 2)?;
        }
    }

    if fields.count != 0 {
        writer.write_bits(fields.huffman_selector as u32, 2)?;
        let descriptor = wlc_descriptors()
            .get(fields.huffman_selector)
            .copied()
            .ok_or(PackIdwlError::UnsupportedWlcSelector {
                selector: fields.huffman_selector,
            })?;

        let first = fields.values.first().ok_or(PackIdwlError::MissingValue {
            index: 0,
            values: fields.values.len(),
        })?;
        writer.write_bits(*first, 3)?;

        for index in 1..fields.count {
            let current = fields
                .values
                .get(index)
                .ok_or(PackIdwlError::MissingValue {
                    index,
                    values: fields.values.len(),
                })?;
            let previous = fields
                .values
                .get(index - 1)
                .ok_or(PackIdwlError::MissingValue {
                    index: index - 1,
                    values: fields.values.len(),
                })?;
            let symbol = current.wrapping_sub(*previous) & 7;
            emit_symbol(writer, descriptor, symbol as usize)?;
        }
    }

    if fields.selector_b == 2 && fields.channel_flag == 1 && fields.config_count > fields.count {
        for index in fields.count..fields.config_count {
            let tail_flag = fields
                .values
                .get(index)
                .ok_or(PackIdwlError::MissingTailFlag {
                    index,
                    tail_flags: fields.values.len(),
                })?;
            writer.write_bits(*tail_flag, 1)?;
        }
    }

    Ok(())
}

pub fn pack_idwl_4_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idwl4Fields<'_>,
) -> Result<(), PackIdwlError> {
    writer.write_bits(fields.selector_b, 2)?;

    if fields.selector_b != 0 {
        writer.write_bits(fields.count as u32, 5)?;
        if fields.selector_b == 3 {
            let mode3_bias = if fields.channel_flag == 0 { 1 } else { 3 };
            writer.write_bits(fields.mode3_value.wrapping_sub(mode3_bias), 2)?;
        }
    }

    if fields.count != 0 {
        writer.write_bits(fields.huffman_selector as u32, 2)?;
        let descriptor = wlc_descriptors()
            .get(fields.huffman_selector)
            .copied()
            .ok_or(PackIdwlError::UnsupportedWlcSelector {
                selector: fields.huffman_selector,
            })?;

        for index in 0..fields.count {
            let current = fields
                .current_values
                .get(index)
                .ok_or(PackIdwlError::MissingValue {
                    index,
                    values: fields.current_values.len(),
                })?;
            let previous =
                fields
                    .previous_values
                    .get(index)
                    .ok_or(PackIdwlError::MissingPreviousValue {
                        index,
                        previous_values: fields.previous_values.len(),
                    })?;
            let symbol = current.wrapping_sub(*previous) & 7;
            emit_symbol(writer, descriptor, symbol as usize)?;
        }
    }

    if fields.selector_b == 2 && fields.config_count > fields.count {
        for index in fields.count..fields.config_count {
            let tail_flag = fields
                .tail_flags
                .get(index)
                .ok_or(PackIdwlError::MissingTailFlag {
                    index,
                    tail_flags: fields.tail_flags.len(),
                })?;
            writer.write_bits(*tail_flag, 1)?;
        }
    }

    Ok(())
}

pub fn pack_idwl_5_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idwl5Fields<'_>,
) -> Result<(), PackIdwlError> {
    writer.write_bits(fields.selector_b, 2)?;

    if fields.selector_b != 0 {
        writer.write_bits(fields.count as u32, 5)?;
        if fields.selector_b == 3 {
            let mode3_bias = if fields.channel_flag == 0 { 1 } else { 3 };
            writer.write_bits(fields.mode3_value.wrapping_sub(mode3_bias), 2)?;
        }
    }

    if fields.count != 0 {
        writer.write_bits(fields.huffman_selector as u32, 2)?;
        let descriptor = wlc_descriptors()
            .get(fields.huffman_selector)
            .copied()
            .ok_or(PackIdwlError::UnsupportedWlcSelector {
                selector: fields.huffman_selector,
            })?;

        let mut previous_delta = 0;
        for index in 0..fields.count {
            let current = fields
                .current_values
                .get(index)
                .ok_or(PackIdwlError::MissingValue {
                    index,
                    values: fields.current_values.len(),
                })?;
            let previous =
                fields
                    .previous_values
                    .get(index)
                    .ok_or(PackIdwlError::MissingPreviousValue {
                        index,
                        previous_values: fields.previous_values.len(),
                    })?;
            let delta = current.wrapping_sub(*previous) & 7;
            let symbol = if index == 0 {
                delta
            } else {
                delta.wrapping_sub(previous_delta) & 7
            };
            emit_symbol(writer, descriptor, symbol as usize)?;
            previous_delta = delta;
        }
    }

    if fields.selector_b == 2 && fields.config_count > fields.count {
        for index in fields.count..fields.config_count {
            let tail_flag = fields
                .tail_flags
                .get(index)
                .ok_or(PackIdwlError::MissingTailFlag {
                    index,
                    tail_flags: fields.tail_flags.len(),
                })?;
            writer.write_bits(*tail_flag, 1)?;
        }
    }

    Ok(())
}

fn idwl_symbol(symbols: &[u32], index: usize) -> Result<u32, PackIdwlError> {
    symbols
        .get(index)
        .copied()
        .ok_or(PackIdwlError::MissingValue {
            index,
            values: symbols.len(),
        })
}
