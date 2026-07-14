use crate::tables::huffman::{HuffmanCodeEntry, HuffmanDescriptor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HuffmanLookupError {
    pub descriptor: &'static str,
    pub symbol: usize,
}

pub fn huffman_entry(
    descriptor: HuffmanDescriptor,
    symbol: usize,
) -> Result<HuffmanCodeEntry, HuffmanLookupError> {
    descriptor
        .pack_table()
        .entry(symbol)
        .ok_or(HuffmanLookupError {
            descriptor: descriptor.symbol(),
            symbol,
        })
}
