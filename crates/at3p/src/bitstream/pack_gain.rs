use super::huffman::{HuffmanEmitError, emit_symbol};
use super::writer::{BitWriter, BitWriterError};
use crate::tables::huffman::{
    gc_idlev_a, gc_idlev_b, gc_idlev_c, gc_idlev_d, gc_idloc_a_atk, gc_idloc_a_rel, gc_idloc_b_atk,
    gc_idloc_b_rel, gc_idloc_c_atk, gc_ngc_a, gc_ngc_b,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackGainError {
    MissingValue {
        index: usize,
        values: usize,
    },
    MissingPreviousValue {
        index: usize,
        previous_values: usize,
    },
    MissingPreviousRow {
        index: usize,
        previous_rows: usize,
    },
    MissingFlag {
        index: usize,
        flags: usize,
    },
    MissingLevel {
        row: usize,
        index: usize,
        levels: usize,
    },
    BitWriter(BitWriterError),
    Huffman(HuffmanEmitError),
}

impl From<HuffmanEmitError> for PackGainError {
    fn from(error: HuffmanEmitError) -> Self {
        Self::Huffman(error)
    }
}

impl From<BitWriterError> for PackGainError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ngc3Fields<'a> {
    pub bit_width: u8,
    pub base: i32,
    pub values: &'a [i32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idlev3Fields<'a> {
    pub bit_width: u8,
    pub base: i32,
    pub rows: &'a [&'a [i32]],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idloc3Fields<'a> {
    pub bit_width: u8,
    pub base: i32,
    pub rows: &'a [&'a [u32]],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdlocRow<'a> {
    pub locations: &'a [u32],
    pub levels: &'a [i32],
}

pub fn pack_gain_ngc_0_at5(
    writer: &mut BitWriter<'_>,
    values: &[u32],
) -> Result<(), BitWriterError> {
    for value in values {
        writer.write_bits(*value, 3)?;
    }
    Ok(())
}

pub fn pack_gain_ngc_1_at5(
    writer: &mut BitWriter<'_>,
    symbols: &[u32],
) -> Result<(), HuffmanEmitError> {
    for symbol in symbols {
        emit_symbol(writer, gc_ngc_a(), *symbol as usize)?;
    }
    Ok(())
}

pub fn pack_gain_ngc_2_at5(
    writer: &mut BitWriter<'_>,
    values: &[u32],
) -> Result<(), PackGainError> {
    let mut previous = *values.first().ok_or(PackGainError::MissingValue {
        index: 0,
        values: values.len(),
    })?;
    emit_symbol(writer, gc_ngc_a(), previous as usize)?;

    for value in &values[1..] {
        let symbol = value.wrapping_sub(previous) & 7;
        emit_symbol(writer, gc_ngc_b(), symbol as usize)?;
        previous = *value;
    }

    Ok(())
}

pub fn pack_gain_ngc_3_at5(
    writer: &mut BitWriter<'_>,
    fields: &Ngc3Fields<'_>,
) -> Result<(), BitWriterError> {
    writer.write_bits(u32::from(fields.bit_width), 2)?;
    writer.write_bits(fields.base as u32, 3)?;

    if fields.bit_width == 0 {
        return Ok(());
    }

    for value in fields.values {
        writer.write_bits(value.wrapping_sub(fields.base) as u32, fields.bit_width)?;
    }

    Ok(())
}

pub fn pack_gain_ngc_4_at5(
    writer: &mut BitWriter<'_>,
    current_values: &[u32],
    previous_values: &[u32],
) -> Result<(), PackGainError> {
    for (index, current) in current_values.iter().enumerate() {
        let previous = previous_values
            .get(index)
            .ok_or(PackGainError::MissingPreviousValue {
                index,
                previous_values: previous_values.len(),
            })?;
        let symbol = current.wrapping_sub(*previous) & 7;
        emit_symbol(writer, gc_ngc_b(), symbol as usize)?;
    }

    Ok(())
}

pub fn pack_gain_ngc_5_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

pub fn pack_gain_idlev_0_at5(
    writer: &mut BitWriter<'_>,
    rows: &[&[u32]],
) -> Result<(), BitWriterError> {
    for row in rows {
        for level in *row {
            writer.write_bits(*level, 4)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idlev_1_at5(
    writer: &mut BitWriter<'_>,
    rows: &[&[u32]],
) -> Result<(), HuffmanEmitError> {
    for row in rows {
        let Some(first) = row.first() else {
            continue;
        };

        emit_symbol(writer, gc_idlev_a(), *first as usize)?;
        for window in row.windows(2) {
            let symbol = window[1].wrapping_sub(window[0]) & 0xf;
            emit_symbol(writer, gc_idlev_b(), symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idlev_2_at5(
    writer: &mut BitWriter<'_>,
    rows: &[&[u32]],
) -> Result<(), HuffmanEmitError> {
    if let Some(row) = rows.first() {
        if let Some(first) = row.first() {
            emit_symbol(writer, gc_idlev_a(), *first as usize)?;
            for window in row.windows(2) {
                let symbol = window[1].wrapping_sub(window[0]) & 0xf;
                emit_symbol(writer, gc_idlev_b(), symbol as usize)?;
            }
        }
    }

    for window in rows.windows(2) {
        let previous_row = window[0];
        let current_row = window[1];
        for (index, current) in current_row.iter().enumerate() {
            let previous = previous_row.get(index).copied().unwrap_or(7);
            let symbol = current.wrapping_sub(previous) & 0xf;
            emit_symbol(writer, gc_idlev_c(), symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idlev_3_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idlev3Fields<'_>,
) -> Result<(), BitWriterError> {
    writer.write_bits(u32::from(fields.bit_width), 2)?;
    writer.write_bits(fields.base as u32, 4)?;

    if fields.bit_width == 0 {
        return Ok(());
    }

    for row in fields.rows {
        for level in *row {
            writer.write_bits(level.wrapping_sub(fields.base) as u32, fields.bit_width)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idlev_4_at5(
    writer: &mut BitWriter<'_>,
    current_rows: &[&[u32]],
    previous_rows: &[&[u32]],
) -> Result<(), PackGainError> {
    for (row_index, current_row) in current_rows.iter().enumerate() {
        if current_row.is_empty() {
            continue;
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGainError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;

        for (index, current) in current_row.iter().enumerate() {
            let previous = previous_row.get(index).copied().unwrap_or(7);
            let symbol = current.wrapping_sub(previous) & 0xf;
            emit_symbol(writer, gc_idlev_d(), symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idlev_5_at5(
    writer: &mut BitWriter<'_>,
    rows: &[&[u32]],
    flags: &[u32],
) -> Result<(), PackGainError> {
    for (row_index, row) in rows.iter().enumerate() {
        if row.is_empty() {
            continue;
        }

        let flag = *flags.get(row_index).ok_or(PackGainError::MissingFlag {
            index: row_index,
            flags: flags.len(),
        })?;
        writer.write_bits(flag, 1)?;

        if flag == 0 {
            continue;
        }

        emit_symbol(writer, gc_idlev_a(), row[0] as usize)?;
        for window in row.windows(2) {
            let symbol = window[1].wrapping_sub(window[0]) & 0xf;
            emit_symbol(writer, gc_idlev_b(), symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idlev_6_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

pub fn pack_gain_idloc_0_at5(
    writer: &mut BitWriter<'_>,
    rows: &[&[u32]],
) -> Result<(), BitWriterError> {
    for row in rows {
        let Some(first) = row.first() else {
            continue;
        };

        writer.write_bits(*first, 5)?;
        for window in row.windows(2) {
            write_idloc_delta(writer, window[0], window[1])?;
        }
    }

    Ok(())
}

pub fn pack_gain_idloc_1_at5(
    writer: &mut BitWriter<'_>,
    rows: &[IdlocRow<'_>],
) -> Result<(), PackGainError> {
    for (row_index, row) in rows.iter().enumerate() {
        let Some(first) = row.locations.first() else {
            continue;
        };
        if row.locations.len() > 1 && row.levels.len() < row.locations.len() {
            return Err(PackGainError::MissingLevel {
                row: row_index,
                index: row.levels.len(),
                levels: row.levels.len(),
            });
        }

        writer.write_bits(*first, 5)?;
        for index in 1..row.locations.len() {
            let descriptor = if row.levels[index].wrapping_sub(row.levels[index - 1]) < 1 {
                gc_idloc_a_atk()
            } else {
                gc_idloc_a_rel()
            };
            let symbol = row.locations[index].wrapping_sub(row.locations[index - 1]);
            emit_symbol(writer, descriptor, symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idloc_2_at5(
    writer: &mut BitWriter<'_>,
    rows: &[IdlocRow<'_>],
) -> Result<(), PackGainError> {
    if let Some(row) = rows.first() {
        pack_gain_idloc_0_at5(writer, &[row.locations])?;
    }

    for row_index in 1..rows.len() {
        let current_row = rows[row_index];
        let Some(first) = current_row.locations.first() else {
            continue;
        };
        if current_row.locations.len() > 1 && current_row.levels.len() < current_row.locations.len()
        {
            return Err(PackGainError::MissingLevel {
                row: row_index,
                index: current_row.levels.len(),
                levels: current_row.levels.len(),
            });
        }

        let previous_row = rows[row_index - 1];
        let first_symbol = previous_row
            .locations
            .first()
            .map_or(*first, |previous| first.wrapping_sub(*previous) & 0x1f);
        emit_symbol(writer, gc_idloc_b_atk(), first_symbol as usize)?;

        for index in 1..current_row.locations.len() {
            let level_delta = current_row.levels[index].wrapping_sub(current_row.levels[index - 1]);
            let attack = level_delta <= 0;
            let (descriptor, symbol) = if index < previous_row.locations.len() {
                let descriptor = if attack {
                    gc_idloc_b_atk()
                } else {
                    gc_idloc_b_rel()
                };
                (
                    descriptor,
                    current_row.locations[index].wrapping_sub(previous_row.locations[index]) & 0x1f,
                )
            } else {
                let descriptor = if attack {
                    gc_idloc_a_atk()
                } else {
                    gc_idloc_a_rel()
                };
                (
                    descriptor,
                    current_row.locations[index].wrapping_sub(current_row.locations[index - 1]),
                )
            };
            emit_symbol(writer, descriptor, symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idloc_3_at5(
    writer: &mut BitWriter<'_>,
    fields: &Idloc3Fields<'_>,
) -> Result<(), BitWriterError> {
    writer.write_bits(u32::from(fields.bit_width.wrapping_sub(1)), 2)?;
    writer.write_bits(fields.base as u32, 5)?;

    if fields.bit_width == 0 {
        return Ok(());
    }

    for row in fields.rows {
        for (index, location) in row.iter().enumerate() {
            let value = location
                .wrapping_sub(index as u32)
                .wrapping_sub(fields.base as u32);
            writer.write_bits(value, fields.bit_width)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idloc_4_at5(
    writer: &mut BitWriter<'_>,
    current_rows: &[IdlocRow<'_>],
    previous_rows: &[&[u32]],
) -> Result<(), PackGainError> {
    for (row_index, current_row) in current_rows.iter().enumerate() {
        let Some(first) = current_row.locations.first() else {
            continue;
        };
        if current_row.locations.len() > 1 && current_row.levels.len() < current_row.locations.len()
        {
            return Err(PackGainError::MissingLevel {
                row: row_index,
                index: current_row.levels.len(),
                levels: current_row.levels.len(),
            });
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGainError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;
        let first_symbol = previous_row
            .first()
            .map_or(*first, |previous| first.wrapping_sub(*previous) & 0x1f);
        emit_symbol(writer, gc_idloc_c_atk(), first_symbol as usize)?;

        for index in 1..current_row.locations.len() {
            let level_delta = current_row.levels[index].wrapping_sub(current_row.levels[index - 1]);
            let attack = level_delta <= 0;

            if index < previous_row.len() {
                let previous_symbol =
                    current_row.locations[index].wrapping_sub(previous_row[index]) & 0x1f;
                if attack {
                    emit_symbol(writer, gc_idloc_c_atk(), previous_symbol as usize)?;
                } else if previous_symbol == 0 {
                    writer.write_bits(0, 1)?;
                } else {
                    writer.write_bits(1, 1)?;
                    write_idloc_delta(
                        writer,
                        current_row.locations[index - 1],
                        current_row.locations[index],
                    )?;
                }
            } else {
                let descriptor = if attack {
                    gc_idloc_a_atk()
                } else {
                    gc_idloc_a_rel()
                };
                let symbol =
                    current_row.locations[index].wrapping_sub(current_row.locations[index - 1]);
                emit_symbol(writer, descriptor, symbol as usize)?;
            }
        }
    }

    Ok(())
}

pub fn pack_gain_idloc_5_at5(
    writer: &mut BitWriter<'_>,
    current_rows: &[IdlocRow<'_>],
    previous_rows: &[&[u32]],
    flags: &[u32],
) -> Result<(), PackGainError> {
    for (row_index, current_row) in current_rows.iter().enumerate() {
        let Some(first) = current_row.locations.first() else {
            continue;
        };
        if current_row.locations.len() > 1 && current_row.levels.len() < current_row.locations.len()
        {
            return Err(PackGainError::MissingLevel {
                row: row_index,
                index: current_row.levels.len(),
                levels: current_row.levels.len(),
            });
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGainError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;

        if current_row.locations.len() <= previous_row.len() {
            let flag = *flags.get(row_index).ok_or(PackGainError::MissingFlag {
                index: row_index,
                flags: flags.len(),
            })?;
            writer.write_bits(flag, 1)?;

            if flag == 0 {
                continue;
            }
        }

        writer.write_bits(*first, 5)?;
        for index in 1..current_row.locations.len() {
            let descriptor =
                if current_row.levels[index].wrapping_sub(current_row.levels[index - 1]) < 1 {
                    gc_idloc_a_atk()
                } else {
                    gc_idloc_a_rel()
                };
            let symbol =
                current_row.locations[index].wrapping_sub(current_row.locations[index - 1]);
            emit_symbol(writer, descriptor, symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gain_idloc_6_at5(
    writer: &mut BitWriter<'_>,
    current_rows: &[&[u32]],
    previous_rows: &[&[u32]],
    flags: &[u32],
) -> Result<(), PackGainError> {
    for (row_index, current_row) in current_rows.iter().enumerate() {
        if current_row.is_empty() {
            continue;
        }

        let flag = *flags.get(row_index).ok_or(PackGainError::MissingFlag {
            index: row_index,
            flags: flags.len(),
        })?;
        if flag == 0 {
            continue;
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGainError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;
        let mut index = previous_row.len();
        if index >= current_row.len() {
            continue;
        }

        if index == 0 {
            writer.write_bits(current_row[0], 5)?;
            index = 1;
        }

        while index < current_row.len() {
            write_idloc_delta(writer, current_row[index - 1], current_row[index])?;
            index += 1;
        }
    }

    Ok(())
}

fn write_idloc_delta(
    writer: &mut BitWriter<'_>,
    previous: u32,
    current: u32,
) -> Result<(), BitWriterError> {
    let Some((value, bits)) = (if previous < 0x0f {
        Some((current, 5))
    } else if previous < 0x17 {
        Some((current.wrapping_sub(previous).wrapping_sub(1), 4))
    } else if previous < 0x1b {
        Some((current.wrapping_sub(previous).wrapping_sub(1), 3))
    } else if previous < 0x1d {
        Some((current.wrapping_sub(previous).wrapping_sub(1), 2))
    } else if previous == 0x1d {
        Some((current.wrapping_sub(0x1e), 1))
    } else {
        None
    }) else {
        return Ok(());
    };

    writer.write_bits(value, bits)
}
