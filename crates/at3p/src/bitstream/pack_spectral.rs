use super::group::{GroupError, GroupedSymbol, hc_mkgrp_at5};
use super::writer::{BitWriter, BitWriterError};
use crate::tables::spectral::{
    SpectralDescriptorMetadata, SpectralDescriptorSlot, SpectralPackTable,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackSpectralError {
    MissingSymbol {
        table: &'static str,
        symbol: usize,
    },
    InvalidGroupedLength {
        grouped_symbols: usize,
        run_length: usize,
    },
    InvalidSignBitCount {
        symbol_index: usize,
        bit_count: u32,
    },
    MissingDescriptorMetadata {
        slot_index: usize,
    },
    MissingGeneratedPackTable {
        table: &'static str,
    },
    ShortInput {
        input_len: usize,
        nsps: usize,
    },
    GroupedCountMismatch {
        slot_index: usize,
        expected: usize,
        actual: usize,
    },
    Group(GroupError),
    BitWriter(BitWriterError),
}

impl From<BitWriterError> for PackSpectralError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}

impl From<GroupError> for PackSpectralError {
    fn from(error: GroupError) -> Self {
        Self::Group(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralEmissionSummary {
    pub table: &'static str,
    pub run_length: usize,
    pub signed: bool,
    pub grouped_symbols: usize,
    pub huffman_symbols: usize,
    pub presence_bits: usize,
    pub zero_runs: usize,
    pub nonzero_runs: usize,
    pub sign_bits: usize,
    pub start_bit: usize,
    pub end_bit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralUnitEmission {
    pub descriptor_slot_index: usize,
    pub selector: char,
    pub word_len: u8,
    pub metadata: SpectralDescriptorMetadata,
    pub emission: SpectralEmissionSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralDescriptorUnitInput<'a> {
    pub descriptor: &'a SpectralDescriptorSlot,
    pub input: &'a [u16],
    pub nsps: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralSequenceEmission {
    pub units: usize,
    pub grouped_symbols: usize,
    pub huffman_symbols: usize,
    pub presence_bits: usize,
    pub zero_runs: usize,
    pub nonzero_runs: usize,
    pub sign_bits: usize,
    pub start_bit: usize,
    pub end_bit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectralTailEmission {
    pub values: usize,
    pub start_bit: usize,
    pub end_bit: usize,
}

pub fn pack_spectral_descriptor_units(
    writer: &mut BitWriter<'_>,
    units: &[SpectralDescriptorUnitInput<'_>],
) -> Result<SpectralSequenceEmission, PackSpectralError> {
    let mut summary = SpectralSequenceEmission {
        units: 0,
        grouped_symbols: 0,
        huffman_symbols: 0,
        presence_bits: 0,
        zero_runs: 0,
        nonzero_runs: 0,
        sign_bits: 0,
        start_bit: writer.bit_pos(),
        end_bit: writer.bit_pos(),
    };

    for unit in units {
        let emitted =
            pack_spectral_descriptor_unit(writer, unit.descriptor, unit.input, unit.nsps)?;
        let emission = emitted.emission;
        summary.units += 1;
        summary.grouped_symbols += emission.grouped_symbols;
        summary.huffman_symbols += emission.huffman_symbols;
        summary.presence_bits += emission.presence_bits;
        summary.zero_runs += emission.zero_runs;
        summary.nonzero_runs += emission.nonzero_runs;
        summary.sign_bits += emission.sign_bits;
        summary.end_bit = emission.end_bit;
    }

    Ok(summary)
}

pub fn pack_spectral_idspcqu_tail(
    writer: &mut BitWriter<'_>,
    values: &[u8],
) -> Result<SpectralTailEmission, PackSpectralError> {
    let start_bit = writer.bit_pos();
    for value in values {
        writer.write_bits(u32::from(*value), 4)?;
    }
    Ok(SpectralTailEmission {
        values: values.len(),
        start_bit,
        end_bit: writer.bit_pos(),
    })
}

pub fn pack_spectral_descriptor_unit(
    writer: &mut BitWriter<'_>,
    descriptor: &SpectralDescriptorSlot,
    input: &[u16],
    nsps: u8,
) -> Result<SpectralUnitEmission, PackSpectralError> {
    let metadata = descriptor
        .metadata()
        .ok_or(PackSpectralError::MissingDescriptorMetadata {
            slot_index: descriptor.slot_index,
        })?;
    let pack_table = descriptor
        .pack_table()
        .and_then(|table| table.generated_pack_table())
        .ok_or(PackSpectralError::MissingGeneratedPackTable {
            table: descriptor.pack_table_symbol,
        })?;

    let symbol_count = usize::from(nsps);
    let input = input
        .get(..symbol_count)
        .ok_or(PackSpectralError::ShortInput {
            input_len: input.len(),
            nsps: symbol_count,
        })?;
    let grouped = hc_mkgrp_at5(
        input,
        usize::from(metadata.group_size),
        metadata.bit_width,
        metadata.magnitude_mask,
        metadata.signed,
    )?;
    let expected = metadata.grouped_symbol_count(nsps);
    if grouped.len() != expected {
        return Err(PackSpectralError::GroupedCountMismatch {
            slot_index: descriptor.slot_index,
            expected,
            actual: grouped.len(),
        });
    }

    let emission = pack_spectral_grouped_symbols(
        writer,
        pack_table,
        &grouped,
        usize::from(metadata.run_length),
        metadata.signed,
    )?;

    Ok(SpectralUnitEmission {
        descriptor_slot_index: descriptor.slot_index,
        selector: descriptor.selector,
        word_len: descriptor.word_len,
        metadata,
        emission,
    })
}

pub fn pack_spectral_grouped_symbols(
    writer: &mut BitWriter<'_>,
    table: SpectralPackTable,
    grouped: &[GroupedSymbol],
    run_length: usize,
    signed: bool,
) -> Result<SpectralEmissionSummary, PackSpectralError> {
    let mut summary = SpectralEmissionSummary {
        table: table.symbol(),
        run_length,
        signed,
        grouped_symbols: grouped.len(),
        huffman_symbols: 0,
        presence_bits: 0,
        zero_runs: 0,
        nonzero_runs: 0,
        sign_bits: 0,
        start_bit: writer.bit_pos(),
        end_bit: writer.bit_pos(),
    };

    if run_length == 0 {
        return Ok(summary);
    }

    if run_length == 1 {
        for (index, symbol) in grouped.iter().enumerate() {
            emit_grouped_symbol(writer, table, *symbol, index, signed, &mut summary)?;
        }
    } else {
        if grouped.len() % run_length != 0 {
            return Err(PackSpectralError::InvalidGroupedLength {
                grouped_symbols: grouped.len(),
                run_length,
            });
        }

        for (run_index, run) in grouped.chunks_exact(run_length).enumerate() {
            let run_has_nonzero = run.iter().any(|symbol| symbol.value != 0);
            writer.write_bits(u32::from(run_has_nonzero), 1)?;
            summary.presence_bits += 1;
            if run_has_nonzero {
                summary.nonzero_runs += 1;
                let symbol_base = run_index * run_length;
                for (offset, symbol) in run.iter().enumerate() {
                    emit_grouped_symbol(
                        writer,
                        table,
                        *symbol,
                        symbol_base + offset,
                        signed,
                        &mut summary,
                    )?;
                }
            } else {
                summary.zero_runs += 1;
            }
        }
    }

    summary.end_bit = writer.bit_pos();
    Ok(summary)
}

fn emit_grouped_symbol(
    writer: &mut BitWriter<'_>,
    table: SpectralPackTable,
    symbol: GroupedSymbol,
    symbol_index: usize,
    signed: bool,
    summary: &mut SpectralEmissionSummary,
) -> Result<(), PackSpectralError> {
    if signed && symbol.nonzero_count > 32 {
        return Err(PackSpectralError::InvalidSignBitCount {
            symbol_index,
            bit_count: symbol.nonzero_count,
        });
    }

    let symbol_value = usize::from(symbol.value);
    let entry = table
        .entry(symbol_value)
        .ok_or(PackSpectralError::MissingSymbol {
            table: table.symbol(),
            symbol: symbol_value,
        })?;

    writer.write_bits(u32::from(entry.code), entry.bit_len)?;
    summary.huffman_symbols += 1;

    if signed && symbol.nonzero_count != 0 {
        let bit_count = symbol.nonzero_count as u8;
        writer.write_bits(symbol.sign_bits, bit_count)?;
        summary.sign_bits += usize::from(bit_count);
    }

    Ok(())
}
