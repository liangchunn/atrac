use super::huffman::{HuffmanEmitError, emit_symbol};
use super::writer::{BitWriter, BitWriterError};
use crate::tables::huffman::{
    ghpc_freq_a, ghpc_idam_aa, ghpc_idam_ab, ghpc_idam_c, ghpc_idsf_aa, ghpc_idsf_ab, ghpc_idsf_b,
    ghpc_nwavs_a, ghpc_nwavs_b,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackGhaError {
    MissingMode {
        index: usize,
        modes: usize,
    },
    MissingPreviousRow {
        index: usize,
        previous_rows: usize,
    },
    MissingPreviousIndex {
        row: usize,
        index: usize,
        previous_indices: usize,
    },
    MissingPreviousValue {
        row: usize,
        index: usize,
        previous_values: usize,
    },
    Writer(BitWriterError),
    Huffman(HuffmanEmitError),
}

impl From<BitWriterError> for PackGhaError {
    fn from(error: BitWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<HuffmanEmitError> for PackGhaError {
    fn from(error: HuffmanEmitError) -> Self {
        Self::Huffman(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhIdlocRow {
    pub active: bool,
    pub first_flag: u32,
    pub first_location: u32,
    pub second_flag: u32,
    pub second_location: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhNwavsRow {
    pub active: bool,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhFreqRow<'a> {
    pub active: bool,
    pub values: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhIdsfRow<'a> {
    pub active: bool,
    pub values: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhIdamRow<'a> {
    pub active: bool,
    pub values: &'a [u32],
}

pub fn pack_gh_idloc_0_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhIdlocRow],
) -> Result<(), BitWriterError> {
    for row in rows {
        if !row.active {
            continue;
        }

        writer.write_bits(row.first_flag, 1)?;
        if row.first_flag != 0 {
            writer.write_bits(row.first_location, 5)?;
        }

        writer.write_bits(row.second_flag, 1)?;
        if row.second_flag != 0 {
            writer.write_bits(row.second_location, 5)?;
        }
    }

    Ok(())
}

pub fn pack_gh_idloc_1_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

pub fn pack_gh_nwavs_3_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

pub fn pack_gh_idsf_3_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

pub fn pack_gh_idam_3_at5(_writer: &mut BitWriter<'_>) -> Result<(), BitWriterError> {
    Ok(())
}

pub fn pack_gh_idam_0_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhIdamRow<'_>],
) -> Result<(), BitWriterError> {
    for row in rows {
        if !row.active {
            continue;
        }

        for value in row.values {
            writer.write_bits(*value, 4)?;
        }
    }

    Ok(())
}

pub fn pack_gh_idam_1_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhIdamRow<'_>],
) -> Result<(), PackGhaError> {
    for row in rows {
        if !row.active || row.values.is_empty() {
            continue;
        }

        if row.values.len() == 1 {
            emit_symbol(writer, ghpc_idam_aa(), row.values[0] as usize)?;
        } else {
            for value in row.values {
                emit_symbol(writer, ghpc_idam_ab(), *value as usize)?;
            }
        }
    }

    Ok(())
}

pub fn pack_gh_idam_2_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhIdamRow<'_>],
    previous_rows: &[&[u32]],
    previous_indices: &[&[i32]],
) -> Result<(), PackGhaError> {
    for (row_index, row) in rows.iter().enumerate() {
        if !row.active || row.values.is_empty() {
            continue;
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGhaError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;
        let previous_index_row = previous_indices.get(row_index).copied().unwrap_or(&[]);

        for (index, value) in row.values.iter().enumerate() {
            let previous_index = previous_index_row.get(index).copied().ok_or(
                PackGhaError::MissingPreviousIndex {
                    row: row_index,
                    index,
                    previous_indices: previous_index_row.len(),
                },
            )?;
            let symbol = if previous_index < 0 {
                value.wrapping_sub(0x0c)
            } else {
                let previous = previous_row.get(previous_index as usize).ok_or(
                    PackGhaError::MissingPreviousValue {
                        row: row_index,
                        index: previous_index as usize,
                        previous_values: previous_row.len(),
                    },
                )?;
                value.wrapping_sub(*previous)
            } & 7;
            emit_symbol(writer, ghpc_idam_c(), symbol as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gh_nwavs_0_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhNwavsRow],
) -> Result<(), BitWriterError> {
    for row in rows {
        if row.active {
            writer.write_bits(row.value, 4)?;
        }
    }

    Ok(())
}

pub fn pack_gh_nwavs_1_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhNwavsRow],
) -> Result<(), HuffmanEmitError> {
    for row in rows {
        if row.active {
            emit_symbol(writer, ghpc_nwavs_a(), row.value as usize)?;
        }
    }

    Ok(())
}

pub fn pack_gh_nwavs_2_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhNwavsRow],
    previous_values: &[u32],
) -> Result<(), PackGhaError> {
    for (index, row) in rows.iter().enumerate() {
        if !row.active {
            continue;
        }

        let previous = previous_values
            .get(index)
            .ok_or(PackGhaError::MissingPreviousRow {
                index,
                previous_rows: previous_values.len(),
            })?;
        let symbol = row.value.wrapping_sub(*previous) & 7;
        emit_symbol(writer, ghpc_nwavs_b(), symbol as usize)?;
    }

    Ok(())
}

pub fn pack_gh_freq_0_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhFreqRow<'_>],
    modes: &[u32],
) -> Result<(), PackGhaError> {
    for (row_index, row) in rows.iter().enumerate() {
        if !row.active {
            continue;
        }

        let mode = *modes.get(row_index).ok_or(PackGhaError::MissingMode {
            index: row_index,
            modes: modes.len(),
        })?;

        if row.values.len() > 1 {
            writer.write_bits(mode, 1)?;
        }

        match mode {
            0 => {
                for (index, value) in row.values.iter().enumerate() {
                    if index == 0 {
                        writer.write_bits(*value, 10)?;
                    } else {
                        let previous = row.values[index - 1];
                        let (base, nbits) = gh_freq_mode0_base_bits(previous);
                        writer.write_bits(value.wrapping_sub(base), nbits)?;
                    }
                }
            }
            1 => {
                for index in (0..row.values.len()).rev() {
                    if index == row.values.len() - 1 {
                        writer.write_bits(row.values[index], 10)?;
                    } else {
                        let nbits = gh_freq_mode1_bits(row.values[index + 1]);
                        writer.write_bits(row.values[index], nbits)?;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn pack_gh_idsf_0_at5(
    writer: &mut BitWriter<'_>,
    header_mode: u32,
    rows: &[GhIdsfRow<'_>],
) -> Result<(), BitWriterError> {
    for row in rows {
        if !row.active || row.values.is_empty() {
            continue;
        }

        if header_mode == 0 {
            writer.write_bits(row.values[0], 6)?;
        } else {
            for value in row.values {
                writer.write_bits(*value, 6)?;
            }
        }
    }

    Ok(())
}

pub fn pack_gh_idsf_1_at5(
    writer: &mut BitWriter<'_>,
    header_mode: u32,
    rows: &[GhIdsfRow<'_>],
) -> Result<(), PackGhaError> {
    for row in rows {
        if !row.active || row.values.is_empty() {
            continue;
        }

        if header_mode == 0 {
            let symbol = row.values[0].wrapping_sub(0x18) & 0x1f;
            emit_symbol(writer, ghpc_idsf_aa(), symbol as usize)?;
        } else {
            for value in row.values {
                let symbol = value.wrapping_sub(0x14) & 0x1f;
                emit_symbol(writer, ghpc_idsf_ab(), symbol as usize)?;
            }
        }
    }

    Ok(())
}

pub fn pack_gh_idsf_2_at5(
    writer: &mut BitWriter<'_>,
    header_mode: u32,
    rows: &[GhIdsfRow<'_>],
    previous_rows: &[&[u32]],
    previous_indices: &[&[i32]],
) -> Result<(), PackGhaError> {
    for (row_index, row) in rows.iter().enumerate() {
        if !row.active || row.values.is_empty() {
            continue;
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGhaError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;

        if header_mode == 0 {
            let previous = previous_row.first().copied().unwrap_or(0x2c);
            let symbol = row.values[0].wrapping_sub(previous) & 0x1f;
            emit_symbol(writer, ghpc_idsf_b(), symbol as usize)?;
        } else {
            let previous_index_row = previous_indices.get(row_index).copied().unwrap_or(&[]);
            for (index, value) in row.values.iter().enumerate() {
                let previous_index = previous_index_row.get(index).copied().ok_or(
                    PackGhaError::MissingPreviousIndex {
                        row: row_index,
                        index,
                        previous_indices: previous_index_row.len(),
                    },
                )?;
                let symbol = if previous_index < 0 {
                    value.wrapping_sub(0x22)
                } else {
                    let previous = previous_row.get(previous_index as usize).ok_or(
                        PackGhaError::MissingPreviousValue {
                            row: row_index,
                            index: previous_index as usize,
                            previous_values: previous_row.len(),
                        },
                    )?;
                    value.wrapping_sub(*previous)
                } & 0x1f;
                emit_symbol(writer, ghpc_idsf_b(), symbol as usize)?;
            }
        }
    }

    Ok(())
}

pub fn pack_gh_freq_1_at5(
    writer: &mut BitWriter<'_>,
    rows: &[GhFreqRow<'_>],
    previous_rows: &[&[u32]],
) -> Result<(), PackGhaError> {
    for (row_index, row) in rows.iter().enumerate() {
        if !row.active || row.values.is_empty() {
            continue;
        }

        let previous_row =
            previous_rows
                .get(row_index)
                .ok_or(PackGhaError::MissingPreviousRow {
                    index: row_index,
                    previous_rows: previous_rows.len(),
                })?;

        for (index, value) in row.values.iter().enumerate() {
            let symbol = if index < previous_row.len() {
                value.wrapping_sub(previous_row[index])
            } else if let Some(previous) = previous_row.last() {
                value.wrapping_sub(*previous)
            } else {
                *value
            } & 0xff;
            emit_symbol(writer, ghpc_freq_a(), symbol as usize)?;
        }
    }

    Ok(())
}

fn gh_freq_mode0_base_bits(previous: u32) -> (u32, u8) {
    if previous < 0x200 {
        (0, 10)
    } else if previous < 0x300 {
        (0x200, 9)
    } else if previous < 0x380 {
        (0x300, 8)
    } else if previous < 0x3c0 {
        (0x380, 7)
    } else if previous < 0x3e0 {
        (0x3c0, 6)
    } else if previous < 0x3f0 {
        (0x3e0, 5)
    } else if previous < 0x3f8 {
        (0x3f0, 4)
    } else if previous < 0x3fc {
        (0x3f8, 3)
    } else if previous < 0x3fe {
        (0x3fc, 2)
    } else {
        (0x3fe, 1)
    }
}

fn gh_freq_mode1_bits(next: u32) -> u8 {
    if next < 2 {
        1
    } else if next < 4 {
        2
    } else if next < 8 {
        3
    } else if next < 0x10 {
        4
    } else if next < 0x20 {
        5
    } else if next < 0x40 {
        6
    } else if next < 0x80 {
        7
    } else if next < 0x100 {
        8
    } else if next < 0x200 {
        9
    } else {
        10
    }
}
