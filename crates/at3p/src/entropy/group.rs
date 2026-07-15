use crate::tables::at5::mask_q_at5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupedSymbol {
    pub value: u16,
    pub nonzero_count: u32,
    pub sign_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupError {
    UnsupportedGroupSize {
        group_size: usize,
    },
    UnsupportedBitWidth {
        bit_width: u8,
    },
    InvalidInputLength {
        helper: &'static str,
        group_size: usize,
        input_len: usize,
        required_multiple: usize,
    },
    ShortExInput {
        input_len: usize,
        symbol_count: usize,
    },
}

pub fn hc_mkgrp_at5(
    input: &[u16],
    group_size: usize,
    bit_width: u8,
    magnitude_mask: u16,
    signed: bool,
) -> Result<Vec<GroupedSymbol>, GroupError> {
    validate_bit_width(bit_width)?;

    if signed {
        hc_mkgrp_at5_signed(input, group_size, bit_width, magnitude_mask)
    } else {
        hc_mkgrp_at5_unsigned(input, group_size, bit_width, magnitude_mask)
    }
}

pub fn hc_mkgrp_ex_at5(
    input_words_le: &[u8],
    symbol_count: usize,
    group_size: usize,
    bit_width: u8,
    magnitude_mask: u16,
) -> Result<Vec<u16>, GroupError> {
    let mut out = vec![0u16; symbol_count];
    let len = hc_mkgrp_ex_core_at5(
        input_words_le,
        symbol_count,
        group_size,
        bit_width,
        magnitude_mask,
        |index, value| out[index] = value,
    )?;
    out.truncate(len);
    Ok(out)
}

/// Allocation-free `hc_mkgrp_Ex_at5` surface. The caller provides room for
/// at least `symbol_count` values, which covers every supported group size.
pub(crate) fn hc_mkgrp_ex_into_at5(
    input_words_le: &[u8],
    symbol_count: usize,
    group_size: usize,
    bit_width: u8,
    magnitude_mask: u16,
    out: &mut [i16],
) -> Result<usize, GroupError> {
    hc_mkgrp_ex_core_at5(
        input_words_le,
        symbol_count,
        group_size,
        bit_width,
        magnitude_mask,
        |index, value| out[index] = value as i16,
    )
}

fn hc_mkgrp_ex_core_at5(
    input_words_le: &[u8],
    symbol_count: usize,
    group_size: usize,
    bit_width: u8,
    magnitude_mask: u16,
    mut write: impl FnMut(usize, u16),
) -> Result<usize, GroupError> {
    validate_bit_width(bit_width)?;

    let required_len = symbol_count
        .checked_mul(2)
        .expect("symbol_count byte length should fit usize");
    if input_words_le.len() < required_len {
        return Err(GroupError::ShortExInput {
            input_len: input_words_le.len(),
            symbol_count,
        });
    }

    match group_size {
        1 => {
            validate_len("hc_mkgrp_Ex_at5", symbol_count, group_size, 4)?;
            for index in 0..symbol_count {
                let start = index * 2;
                let value = u16::from_le_bytes([input_words_le[start], input_words_le[start + 1]]);
                write(index, value & magnitude_mask);
            }
            Ok(symbol_count)
        }
        2 | 4 => {
            let required_multiple = if group_size == 2 { 8 } else { 16 };
            validate_len(
                "hc_mkgrp_Ex_at5",
                symbol_count,
                group_size,
                required_multiple,
            )?;
            let mask = mask_q_at5()[bit_width as usize];
            let shift = u32::from(bit_width & 0x1f);
            let mut output_index = 0usize;

            for symbol_base in (0..symbol_count).step_by(required_multiple) {
                let packed = if group_size == 2 {
                    let first = lane_word_ex(input_words_le, symbol_base, [0, 2, 4, 6]) & mask;
                    let second = lane_word_ex(input_words_le, symbol_base, [1, 3, 5, 7]) & mask;
                    first.wrapping_shl(shift) | second
                } else {
                    let first = lane_word_ex(input_words_le, symbol_base, [0, 4, 8, 12]) & mask;
                    let second = lane_word_ex(input_words_le, symbol_base, [1, 5, 9, 13]) & mask;
                    let third = lane_word_ex(input_words_le, symbol_base, [2, 6, 10, 14]) & mask;
                    let fourth = lane_word_ex(input_words_le, symbol_base, [3, 7, 11, 15]) & mask;
                    (((first.wrapping_shl(shift) | second).wrapping_shl(shift) | third)
                        .wrapping_shl(shift))
                        | fourth
                };
                for byte_index in 0..4 {
                    write(
                        output_index,
                        ((packed >> (24 - byte_index * 8)) & 0xff) as u16,
                    );
                    output_index += 1;
                }
            }
            Ok(output_index)
        }
        _ => Err(GroupError::UnsupportedGroupSize { group_size }),
    }
}

fn hc_mkgrp_at5_unsigned(
    input: &[u16],
    group_size: usize,
    bit_width: u8,
    magnitude_mask: u16,
) -> Result<Vec<GroupedSymbol>, GroupError> {
    match group_size {
        1 => {
            validate_len("hc_mkgrp_at5", input.len(), group_size, 4)?;
            Ok(input
                .iter()
                .map(|value| GroupedSymbol {
                    value: value & magnitude_mask,
                    nonzero_count: 0,
                    sign_bits: 0,
                })
                .collect())
        }
        2 | 4 => {
            let required_multiple = if group_size == 2 { 8 } else { 16 };
            validate_len("hc_mkgrp_at5", input.len(), group_size, required_multiple)?;
            let low_bytes: Vec<u8> = input.iter().map(|value| *value as u8).collect();
            pack_unsigned_low_bytes(&low_bytes, group_size, bit_width)
        }
        _ => Err(GroupError::UnsupportedGroupSize { group_size }),
    }
}

fn hc_mkgrp_at5_signed(
    input: &[u16],
    group_size: usize,
    bit_width: u8,
    magnitude_mask: u16,
) -> Result<Vec<GroupedSymbol>, GroupError> {
    match group_size {
        1 => Ok(input
            .iter()
            .map(|value| {
                let sign_bits = sign_bit(*value);
                GroupedSymbol {
                    value: signed_magnitude(*value, magnitude_mask),
                    nonzero_count: u32::from(*value != 0),
                    sign_bits,
                }
            })
            .collect()),
        2 => {
            validate_len("hc_mkgrp_at5", input.len(), group_size, 2)?;
            Ok(input
                .chunks_exact(2)
                .map(|chunk| pack_signed_group(chunk, bit_width, magnitude_mask))
                .collect())
        }
        4 => {
            validate_len("hc_mkgrp_at5", input.len(), group_size, 4)?;
            Ok(input
                .chunks_exact(4)
                .map(|chunk| pack_signed_group(chunk, bit_width, magnitude_mask))
                .collect())
        }
        _ => Err(GroupError::UnsupportedGroupSize { group_size }),
    }
}

fn pack_unsigned_low_bytes(
    input: &[u8],
    group_size: usize,
    bit_width: u8,
) -> Result<Vec<GroupedSymbol>, GroupError> {
    let mask = mask_q_at5()[bit_width as usize];
    let shift = u32::from(bit_width & 0x1f);
    let mut out = Vec::with_capacity(input.len() / group_size);

    match group_size {
        2 => {
            for chunk in input.chunks_exact(8) {
                let first = lane_word(chunk, [0, 2, 4, 6]) & mask;
                let second = lane_word(chunk, [1, 3, 5, 7]) & mask;
                push_packed_bytes(&mut out, first.wrapping_shl(shift) | second);
            }
        }
        4 => {
            for chunk in input.chunks_exact(16) {
                let first = lane_word(chunk, [0, 4, 8, 12]) & mask;
                let second = lane_word(chunk, [1, 5, 9, 13]) & mask;
                let third = lane_word(chunk, [2, 6, 10, 14]) & mask;
                let fourth = lane_word(chunk, [3, 7, 11, 15]) & mask;
                let packed = (((first.wrapping_shl(shift) | second).wrapping_shl(shift) | third)
                    .wrapping_shl(shift))
                    | fourth;
                push_packed_bytes(&mut out, packed);
            }
        }
        _ => return Err(GroupError::UnsupportedGroupSize { group_size }),
    }

    Ok(out)
}

fn pack_signed_group(chunk: &[u16], bit_width: u8, magnitude_mask: u16) -> GroupedSymbol {
    let mut value = native_signed_shift(signed_magnitude(chunk[0], magnitude_mask), bit_width);
    let mut nonzero_count = u32::from(chunk[0] != 0);
    let mut sign_bits = sign_bit(chunk[0]);

    for (index, sample) in chunk.iter().enumerate().skip(1) {
        if (*sample as i16) < 0 {
            value |= sample.wrapping_neg() & magnitude_mask;
            sign_bits = sign_bits * 2 | 1;
            nonzero_count += 1;
        } else if (*sample & 0x7fff) != 0 {
            value |= *sample;
            sign_bits *= 2;
            nonzero_count += 1;
        }

        if index + 1 < chunk.len() {
            value = native_signed_shift(value, bit_width);
        }
    }

    GroupedSymbol {
        value,
        nonzero_count,
        sign_bits,
    }
}

fn validate_bit_width(bit_width: u8) -> Result<(), GroupError> {
    if usize::from(bit_width) < mask_q_at5().len() {
        Ok(())
    } else {
        Err(GroupError::UnsupportedBitWidth { bit_width })
    }
}

fn validate_len(
    helper: &'static str,
    input_len: usize,
    group_size: usize,
    required_multiple: usize,
) -> Result<(), GroupError> {
    if required_multiple != 0 && input_len % required_multiple == 0 {
        Ok(())
    } else {
        Err(GroupError::InvalidInputLength {
            helper,
            group_size,
            input_len,
            required_multiple,
        })
    }
}

fn lane_word<const N: usize>(chunk: &[u8], indices: [usize; N]) -> u32 {
    indices
        .into_iter()
        .fold(0_u32, |word, index| (word << 8) | u32::from(chunk[index]))
}

fn lane_word_ex<const N: usize>(input_words_le: &[u8], base: usize, indices: [usize; N]) -> u32 {
    indices.into_iter().fold(0_u32, |word, index| {
        (word << 8) | u32::from(input_words_le[(base + index) * 2])
    })
}

fn push_packed_bytes(out: &mut Vec<GroupedSymbol>, packed: u32) {
    out.extend((0..4).map(|index| GroupedSymbol {
        value: ((packed >> (24 - index * 8)) & 0xff) as u16,
        nonzero_count: 0,
        sign_bits: 0,
    }));
}

fn sign_bit(value: u16) -> u32 {
    u32::from((value & 0x8000) != 0)
}

fn signed_magnitude(value: u16, magnitude_mask: u16) -> u16 {
    if (value as i16) < 0 {
        value.wrapping_neg() & magnitude_mask
    } else {
        value
    }
}

fn native_signed_shift(value: u16, bit_width: u8) -> u16 {
    ((i32::from(value as i16)) << u32::from(bit_width & 0x1f)) as u16
}
