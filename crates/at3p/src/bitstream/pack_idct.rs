use super::huffman::{HuffmanEmitError, emit_symbol};
use super::writer::{BitWriter, BitWriterError};
use crate::tables::at5::idct_fixbits_at5;
use crate::tables::huffman::{ct_a, ct_b, ct_c, ct_d};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idct0Count {
    FullBandCount(usize),
    ExplicitCount(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idct0Row {
    pub mode: u32,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackIdctError {
    UnsupportedBandwidthMode {
        bandwidth_mode: usize,
    },
    MissingRow {
        index: usize,
        rows: usize,
    },
    MissingPreviousValue {
        index: usize,
        previous_values: usize,
    },
    Huffman(HuffmanEmitError),
    BitWriter(BitWriterError),
}

impl From<BitWriterError> for PackIdctError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}

impl From<HuffmanEmitError> for PackIdctError {
    fn from(error: HuffmanEmitError) -> Self {
        Self::Huffman(error)
    }
}

pub fn pack_idct_0_at5(
    writer: &mut BitWriter<'_>,
    count: Idct0Count,
    bandwidth_mode: usize,
    rows: &[Idct0Row],
) -> Result<(), PackIdctError> {
    let fixbits = *idct_fixbits_at5()
        .get(bandwidth_mode)
        .ok_or(PackIdctError::UnsupportedBandwidthMode { bandwidth_mode })?;

    let active_count = match count {
        Idct0Count::FullBandCount(count) => {
            writer.write_bits(0, 1)?;
            count
        }
        Idct0Count::ExplicitCount(count) => {
            writer.write_bits(1, 1)?;
            writer.write_bits(count as u32, 5)?;
            count
        }
    };

    for index in 0..active_count {
        let row = rows.get(index).ok_or(PackIdctError::MissingRow {
            index,
            rows: rows.len(),
        })?;
        match row.mode {
            1 => writer.write_bits(row.value, fixbits)?,
            2 => writer.write_bits(row.value, 1)?,
            _ => {}
        }
    }

    Ok(())
}

pub fn pack_idct_1_at5(
    writer: &mut BitWriter<'_>,
    count: Idct0Count,
    bandwidth_mode: usize,
    rows: &[Idct0Row],
) -> Result<(), PackIdctError> {
    let descriptor = if bandwidth_mode == 0 { ct_a() } else { ct_b() };
    let active_count = write_idct_count(writer, count)?;

    for index in 0..active_count {
        let row = rows.get(index).ok_or(PackIdctError::MissingRow {
            index,
            rows: rows.len(),
        })?;
        match row.mode {
            1 => {
                emit_symbol(writer, descriptor, row.value as usize)?;
            }
            2 => writer.write_bits(row.value, 1)?,
            _ => {}
        }
    }

    Ok(())
}

pub fn pack_idct_2_at5(
    writer: &mut BitWriter<'_>,
    count: Idct0Count,
    bandwidth_mode: usize,
    rows: &[Idct0Row],
) -> Result<(), PackIdctError> {
    let first_descriptor = if bandwidth_mode == 0 { ct_a() } else { ct_b() };
    let delta_descriptor = if bandwidth_mode == 0 { ct_a() } else { ct_c() };
    let symbol_mask = u32::from(delta_descriptor.symbol_mask());
    let active_count = write_idct_count(writer, count)?;
    let mut previous_value = 0;

    for index in 0..active_count {
        let row = rows.get(index).ok_or(PackIdctError::MissingRow {
            index,
            rows: rows.len(),
        })?;
        match row.mode {
            1 => {
                let (descriptor, symbol) = if index == 0 {
                    (first_descriptor, row.value)
                } else {
                    (
                        delta_descriptor,
                        row.value.wrapping_sub(previous_value) & symbol_mask,
                    )
                };
                emit_symbol(writer, descriptor, symbol as usize)?;
                previous_value = row.value;
            }
            2 => writer.write_bits(row.value, 1)?,
            _ => {}
        }
    }

    Ok(())
}

pub fn pack_idct_4_at5(
    writer: &mut BitWriter<'_>,
    count: Idct0Count,
    bandwidth_mode: usize,
    rows: &[Idct0Row],
    previous_values: &[u32],
) -> Result<(), PackIdctError> {
    let descriptor = if bandwidth_mode == 0 { ct_a() } else { ct_d() };
    let symbol_mask = u32::from(descriptor.symbol_mask());
    let active_count = write_idct_count(writer, count)?;

    for index in 0..active_count {
        let row = rows.get(index).ok_or(PackIdctError::MissingRow {
            index,
            rows: rows.len(),
        })?;
        match row.mode {
            1 => {
                let previous =
                    previous_values
                        .get(index)
                        .ok_or(PackIdctError::MissingPreviousValue {
                            index,
                            previous_values: previous_values.len(),
                        })?;
                let symbol = row.value.wrapping_sub(*previous) & symbol_mask;
                emit_symbol(writer, descriptor, symbol as usize)?;
            }
            2 => writer.write_bits(row.value, 1)?,
            _ => {}
        }
    }

    Ok(())
}

pub fn pack_idct_3_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

fn write_idct_count(
    writer: &mut BitWriter<'_>,
    count: Idct0Count,
) -> Result<usize, BitWriterError> {
    match count {
        Idct0Count::FullBandCount(count) => {
            writer.write_bits(0, 1)?;
            Ok(count)
        }
        Idct0Count::ExplicitCount(count) => {
            writer.write_bits(1, 1)?;
            writer.write_bits(count as u32, 5)?;
            Ok(count)
        }
    }
}
