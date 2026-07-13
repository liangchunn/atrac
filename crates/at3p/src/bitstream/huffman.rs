use crate::bitstream::writer::{BitWriter, BitWriterError};
use crate::tables::huffman::{HuffmanCodeEntry, HuffmanDescriptor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuffmanEmitError {
    MissingSymbol {
        descriptor: &'static str,
        symbol: usize,
    },
    BitWriter(BitWriterError),
}

impl From<BitWriterError> for HuffmanEmitError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HuffmanEmission {
    pub descriptor: &'static str,
    pub table: &'static str,
    pub symbol: usize,
    pub code: u16,
    pub bit_len: u8,
    pub start_bit: usize,
    pub end_bit: usize,
}

pub fn emit_symbol(
    writer: &mut BitWriter<'_>,
    descriptor: HuffmanDescriptor,
    symbol: usize,
) -> Result<HuffmanEmission, HuffmanEmitError> {
    let table = descriptor.pack_table();
    let HuffmanCodeEntry { code, bit_len, .. } =
        table.entry(symbol).ok_or(HuffmanEmitError::MissingSymbol {
            descriptor: descriptor.symbol(),
            symbol,
        })?;

    let start_bit = writer.bit_pos();
    writer.write_bits(code as u32, bit_len)?;
    let end_bit = writer.bit_pos();

    Ok(HuffmanEmission {
        descriptor: descriptor.symbol(),
        table: table.symbol(),
        symbol,
        code,
        bit_len,
        start_bit,
        end_bit,
    })
}
