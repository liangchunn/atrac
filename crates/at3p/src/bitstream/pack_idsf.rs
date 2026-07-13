use super::huffman::{HuffmanEmitError, emit_symbol};
use super::writer::{BitWriter, BitWriterError};
use crate::tables::huffman::{sfc_descriptors, sfc_sg_descriptors};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idsf1Fields<'a> {
    pub mode_selector: usize,
    pub field_0x1c758: u32,
    pub field_0x1c754: u32,
    pub prefix_count: usize,
    pub residual_bits: u8,
    pub residual_base: i32,
    pub count: usize,
    pub values: &'a [i32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idsf2Fields<'a> {
    pub huffman_selector: usize,
    pub field_0x1c758: u32,
    pub field_0x1c754: u32,
    pub count: usize,
    pub symbols: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idsf3Fields<'a> {
    pub mode_selector: usize,
    pub huffman_selector: usize,
    pub field_0x1c758: u32,
    pub field_0x1c754: u32,
    pub count: usize,
    pub values: &'a [i32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idsf4Fields<'a> {
    pub huffman_selector: usize,
    pub count: usize,
    pub current_values: &'a [u32],
    pub previous_values: &'a [u32],
}

pub type Idsf5Fields<'a> = Idsf4Fields<'a>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackIdsfError {
    MissingValue {
        index: usize,
        values: usize,
    },
    MissingPreviousValue {
        index: usize,
        previous_values: usize,
    },
    UnsupportedSfcSelector {
        selector: usize,
    },
    UnsupportedSfcSubgroupSelector {
        selector: usize,
    },
    UnsupportedCompactMode {
        mode: usize,
    },
    Huffman(HuffmanEmitError),
    BitWriter(BitWriterError),
}

impl From<BitWriterError> for PackIdsfError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}

impl From<HuffmanEmitError> for PackIdsfError {
    fn from(error: HuffmanEmitError) -> Self {
        Self::Huffman(error)
    }
}

pub fn pack_idsf_0_at5(
    writer: &mut BitWriter<'_>,
    scale_factors: &[u32],
) -> Result<(), BitWriterError> {
    for scale_factor in scale_factors {
        writer.write_bits(*scale_factor, 6)?;
    }
    Ok(())
}

pub fn pack_idsf_1_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idsf1Fields<'_>,
) -> Result<(), PackIdsfError> {
    writer.write_bits(fields.mode_selector as u32, 2)?;

    if fields.mode_selector == 3 {
        writer.write_bits(fields.field_0x1c758, 6)?;
        writer.write_bits(fields.field_0x1c754, 6)?;
        writer.write_bits(fields.prefix_count as u32, 5)?;
        writer.write_bits(u32::from(fields.residual_bits), 2)?;
        writer.write_bits(fields.residual_base.wrapping_add(7) as u32, 4)?;

        for index in 0..fields.prefix_count {
            let value = idsf1_value(fields, index)?;
            writer.write_bits(value.wrapping_add(7) as u32, 4)?;
        }
    } else {
        if fields.mode_selector > 3 {
            return Err(PackIdsfError::UnsupportedCompactMode {
                mode: fields.mode_selector,
            });
        }

        writer.write_bits(fields.prefix_count as u32, 5)?;
        writer.write_bits(u32::from(fields.residual_bits), 3)?;
        writer.write_bits(fields.residual_base as u32, 6)?;

        for index in 0..fields.prefix_count {
            let value = idsf1_value(fields, index)?;
            writer.write_bits(value as u32, 6)?;
        }
    }

    if fields.residual_bits != 0 && fields.prefix_count < fields.count {
        for index in fields.prefix_count..fields.count {
            let value = idsf1_value(fields, index)?;
            writer.write_bits(
                value.wrapping_sub(fields.residual_base) as u32,
                fields.residual_bits,
            )?;
        }
    }

    Ok(())
}

pub fn pack_idsf_2_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idsf2Fields<'_>,
) -> Result<(), PackIdsfError> {
    writer.write_bits(fields.huffman_selector as u32, 2)?;
    writer.write_bits(fields.field_0x1c758, 6)?;
    writer.write_bits(fields.field_0x1c754, 6)?;

    if fields.count != 0 {
        let descriptor = sfc_sg_descriptors()
            .get(fields.huffman_selector)
            .copied()
            .ok_or(PackIdsfError::UnsupportedSfcSubgroupSelector {
                selector: fields.huffman_selector,
            })?;

        for index in 0..fields.count {
            let symbol = fields
                .symbols
                .get(index)
                .ok_or(PackIdsfError::MissingValue {
                    index,
                    values: fields.symbols.len(),
                })?;
            emit_symbol(writer, descriptor, (*symbol & 0x0f) as usize)?;
        }
    }

    Ok(())
}

pub fn pack_idsf_3_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idsf3Fields<'_>,
) -> Result<(), PackIdsfError> {
    writer.write_bits(fields.mode_selector as u32, 2)?;
    writer.write_bits(fields.huffman_selector as u32, 2)?;

    if fields.mode_selector > 3 {
        return Err(PackIdsfError::UnsupportedCompactMode {
            mode: fields.mode_selector,
        });
    }

    let first = idsf3_value(fields, 0)?;
    if fields.mode_selector == 3 {
        writer.write_bits(fields.field_0x1c758, 6)?;
        writer.write_bits(fields.field_0x1c754, 6)?;
        writer.write_bits(first.wrapping_add(8) as u32, 4)?;

        if fields.count > 1 {
            let descriptor = sfc_sg_descriptors()
                .get(fields.huffman_selector)
                .copied()
                .ok_or(PackIdsfError::UnsupportedSfcSubgroupSelector {
                    selector: fields.huffman_selector,
                })?;

            for index in 1..fields.count {
                let current = idsf3_value(fields, index)?;
                let previous = idsf3_value(fields, index - 1)?;
                let symbol = current.wrapping_sub(previous) as u32 & 0x0f;
                emit_symbol(writer, descriptor, symbol as usize)?;
            }
        }
    } else {
        writer.write_bits(first as u32, 6)?;

        if fields.count > 1 {
            let descriptor = sfc_descriptors()
                .get(fields.huffman_selector)
                .copied()
                .ok_or(PackIdsfError::UnsupportedSfcSelector {
                    selector: fields.huffman_selector,
                })?;

            for index in 1..fields.count {
                let current = idsf3_value(fields, index)?;
                let previous = idsf3_value(fields, index - 1)?;
                let symbol = current.wrapping_sub(previous) as u32 & 0x3f;
                emit_symbol(writer, descriptor, symbol as usize)?;
            }
        }
    }

    Ok(())
}

pub fn pack_idsf_4_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idsf4Fields<'_>,
) -> Result<(), PackIdsfError> {
    writer.write_bits(fields.huffman_selector as u32, 2)?;

    if fields.count != 0 {
        let descriptor = sfc_descriptors()
            .get(fields.huffman_selector)
            .copied()
            .ok_or(PackIdsfError::UnsupportedSfcSelector {
                selector: fields.huffman_selector,
            })?;

        for index in 0..fields.count {
            let symbol = idsf_previous_delta(fields, index)?;
            emit_symbol(writer, descriptor, symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_idsf_5_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idsf5Fields<'_>,
) -> Result<(), PackIdsfError> {
    writer.write_bits(fields.huffman_selector as u32, 2)?;

    let descriptor = sfc_descriptors()
        .get(fields.huffman_selector)
        .copied()
        .ok_or(PackIdsfError::UnsupportedSfcSelector {
            selector: fields.huffman_selector,
        })?;

    let mut previous_delta = idsf_previous_delta(fields, 0)?;
    emit_symbol(writer, descriptor, previous_delta as usize)?;

    for index in 1..fields.count {
        let delta = idsf_previous_delta(fields, index)?;
        let symbol = delta.wrapping_sub(previous_delta) & 0x3f;
        emit_symbol(writer, descriptor, symbol as usize)?;
        previous_delta = delta;
    }

    Ok(())
}

pub fn pack_idsf_6_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

fn idsf_previous_delta(fields: &Idsf4Fields<'_>, index: usize) -> Result<u32, PackIdsfError> {
    let current = fields
        .current_values
        .get(index)
        .ok_or(PackIdsfError::MissingValue {
            index,
            values: fields.current_values.len(),
        })?;
    let previous =
        fields
            .previous_values
            .get(index)
            .ok_or(PackIdsfError::MissingPreviousValue {
                index,
                previous_values: fields.previous_values.len(),
            })?;
    Ok(current.wrapping_sub(*previous) & 0x3f)
}

fn idsf1_value(fields: &Idsf1Fields<'_>, index: usize) -> Result<i32, PackIdsfError> {
    fields
        .values
        .get(index)
        .copied()
        .ok_or(PackIdsfError::MissingValue {
            index,
            values: fields.values.len(),
        })
}

fn idsf3_value(fields: &Idsf3Fields<'_>, index: usize) -> Result<i32, PackIdsfError> {
    fields
        .values
        .get(index)
        .copied()
        .ok_or(PackIdsfError::MissingValue {
            index,
            values: fields.values.len(),
        })
}
