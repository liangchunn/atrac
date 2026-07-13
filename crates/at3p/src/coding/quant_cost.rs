//! Candidate-cost descriptors for `quant_nontone_nspecs_at5`
//! (decompile `_L552`, native `0xc150`).
//!
//! The descriptor array is `g_aaa_hcspec` (native `0xf4240`), indexed
//! from `base - 0x18` with `state * 0x540 + candidate * 0xa8 +
//! word_length * 0x18`. Each 0x18-byte descriptor holds a relocated
//! pointer to the candidate's spectral Huffman code table (resolved
//! here from `readelf -r` into the generated table constants; all 112
//! addends are zero) plus control bytes at `+0x11..+0x14`: the
//! grouped-buffer selector, the accumulation mode, the count shift,
//! and the nonzero-count seed flag.

use crate::tables::generated::{
    G_A_SPEC_WL1_A, G_A_SPEC_WL1_B, G_A_SPEC_WL1_C, G_A_SPEC_WL1_D, G_A_SPEC_WL1_E, G_A_SPEC_WL1_F,
    G_A_SPEC_WL1_G, G_A_SPEC_WL1_H, G_A_SPEC_WL1_I, G_A_SPEC_WL1_J, G_A_SPEC_WL1_K, G_A_SPEC_WL2_A,
    G_A_SPEC_WL2_B, G_A_SPEC_WL2_C, G_A_SPEC_WL2_D, G_A_SPEC_WL2_E, G_A_SPEC_WL2_F, G_A_SPEC_WL2_G,
    G_A_SPEC_WL2_H, G_A_SPEC_WL2_I, G_A_SPEC_WL2_J, G_A_SPEC_WL2_K, G_A_SPEC_WL2_L, G_A_SPEC_WL2_M,
    G_A_SPEC_WL2_N, G_A_SPEC_WL3_A, G_A_SPEC_WL3_B, G_A_SPEC_WL3_C, G_A_SPEC_WL3_D, G_A_SPEC_WL3_E,
    G_A_SPEC_WL3_F, G_A_SPEC_WL3_G, G_A_SPEC_WL3_H, G_A_SPEC_WL3_I, G_A_SPEC_WL3_J, G_A_SPEC_WL3_K,
    G_A_SPEC_WL3_L, G_A_SPEC_WL3_M, G_A_SPEC_WL3_N, G_A_SPEC_WL4_A, G_A_SPEC_WL4_B, G_A_SPEC_WL4_C,
    G_A_SPEC_WL4_D, G_A_SPEC_WL4_E, G_A_SPEC_WL4_F, G_A_SPEC_WL4_G, G_A_SPEC_WL4_H, G_A_SPEC_WL4_I,
    G_A_SPEC_WL4_J, G_A_SPEC_WL4_K, G_A_SPEC_WL4_L, G_A_SPEC_WL5_A, G_A_SPEC_WL5_B, G_A_SPEC_WL5_C,
    G_A_SPEC_WL5_D, G_A_SPEC_WL5_E, G_A_SPEC_WL5_F, G_A_SPEC_WL5_G, G_A_SPEC_WL5_H, G_A_SPEC_WL5_I,
    G_A_SPEC_WL5_J, G_A_SPEC_WL5_K, G_A_SPEC_WL5_L, G_A_SPEC_WL6_A, G_A_SPEC_WL6_B, G_A_SPEC_WL6_C,
    G_A_SPEC_WL6_D, G_A_SPEC_WL6_E, G_A_SPEC_WL6_F, G_A_SPEC_WL6_G, G_A_SPEC_WL6_H, G_A_SPEC_WL6_I,
    G_A_SPEC_WL6_J, G_A_SPEC_WL6_K, G_A_SPEC_WL6_L, G_A_SPEC_WL7_A, G_A_SPEC_WL7_B, G_A_SPEC_WL7_C,
    G_A_SPEC_WL7_D, G_A_SPEC_WL7_E, G_A_SPEC_WL7_F, G_A_SPEC_WL7_G, G_A_SPEC_WL7_H, G_A_SPEC_WL7_I,
    G_A_SPEC_WL7_J, G_A_SPEC_WL7_K, G_A_SPEC_WL7_L, G_AAA_HCSPEC,
};

pub const QUANT_COST_STATES: usize = 2;
pub const QUANT_COST_CANDIDATES: usize = 8;
pub const QUANT_COST_WORD_LENGTHS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantCostDescriptor {
    pub table: &'static [u8],
    /// Byte offset of symbol 0's entry inside `table` (nonzero for the
    /// signed-domain windows where raw quantized symbols index
    /// negatively).
    pub table_zero_offset: usize,
    pub buffer_selector: u8,
    pub mode: u8,
    pub count_shift: u8,
    pub seed_nonzero: bool,
}

fn descriptor_table(
    state: usize,
    candidate: usize,
    word_length: usize,
) -> Option<(&'static [u8], usize)> {
    match (state, candidate, word_length) {
        (0, 0, 1) => Some((&G_A_SPEC_WL1_A, 0)),
        (0, 0, 2) => Some((&G_A_SPEC_WL2_A, 0)),
        (0, 0, 3) => Some((&G_A_SPEC_WL3_A, 0)),
        (0, 0, 4) => Some((&G_A_SPEC_WL4_A, 0)),
        (0, 0, 5) => Some((&G_A_SPEC_WL5_A, 0)),
        (0, 0, 6) => Some((&G_A_SPEC_WL6_A, 0)),
        (0, 0, 7) => Some((&G_A_SPEC_WL7_A, 0)),
        (0, 1, 1) => Some((&G_A_SPEC_WL1_B, 0)),
        (0, 1, 2) => Some((&G_A_SPEC_WL2_B, 0)),
        (0, 1, 3) => Some((&G_A_SPEC_WL3_B, 0)),
        (0, 1, 4) => Some((&G_A_SPEC_WL4_B, 0)),
        (0, 1, 5) => Some((&G_A_SPEC_WL5_B, 0)),
        (0, 1, 6) => Some((&G_A_SPEC_WL6_B, 0)),
        (0, 1, 7) => Some((&G_A_SPEC_WL7_B, 0)),
        (0, 2, 1) => Some((&G_A_SPEC_WL1_C, 0)),
        (0, 2, 2) => Some((&G_A_SPEC_WL2_C, 0)),
        (0, 2, 3) => Some((&G_A_SPEC_WL3_C, 0)),
        (0, 2, 4) => Some((&G_A_SPEC_WL4_C, 0)),
        (0, 2, 5) => Some((&G_A_SPEC_WL5_C, 0)),
        (0, 2, 6) => Some((&G_A_SPEC_WL6_C, 0)),
        (0, 2, 7) => Some((&G_A_SPEC_WL7_C, 0)),
        (0, 3, 1) => Some((&G_A_SPEC_WL1_D, 0)),
        (0, 3, 2) => Some((&G_A_SPEC_WL2_D, 0)),
        (0, 3, 3) => Some((&G_A_SPEC_WL3_D, 0)),
        (0, 3, 4) => Some((&G_A_SPEC_WL4_D, 0)),
        (0, 3, 5) => Some((&G_A_SPEC_WL5_D, 0)),
        (0, 3, 6) => Some((&G_A_SPEC_WL6_D, 0)),
        (0, 3, 7) => Some((&G_A_SPEC_WL7_D, 0)),
        (0, 4, 1) => Some((&G_A_SPEC_WL1_E, 0)),
        (0, 4, 2) => Some((&G_A_SPEC_WL2_E, 0)),
        (0, 4, 3) => Some((&G_A_SPEC_WL3_E, 0)),
        (0, 4, 4) => Some((&G_A_SPEC_WL4_E, 0)),
        (0, 4, 5) => Some((&G_A_SPEC_WL5_E, 0)),
        (0, 4, 6) => Some((&G_A_SPEC_WL6_E, 0)),
        (0, 4, 7) => Some((&G_A_SPEC_WL7_E, 0)),
        (0, 5, 1) => Some((&G_A_SPEC_WL1_F, 0)),
        (0, 5, 2) => Some((&G_A_SPEC_WL2_F, 0)),
        (0, 5, 3) => Some((&G_A_SPEC_WL3_F, 0)),
        (0, 5, 4) => Some((&G_A_SPEC_WL4_F, 0)),
        (0, 5, 5) => Some((&G_A_SPEC_WL5_F, 0)),
        (0, 5, 6) => Some((&G_A_SPEC_WL6_F, 0)),
        (0, 5, 7) => Some((&G_A_SPEC_WL7_F, 0)),
        (0, 6, 1) => Some((&G_A_SPEC_WL1_G, 0)),
        (0, 6, 2) => Some((&G_A_SPEC_WL2_G, 0)),
        (0, 6, 3) => Some((&G_A_SPEC_WL3_G, 0)),
        (0, 6, 4) => Some((&G_A_SPEC_WL4_G, 0)),
        (0, 6, 5) => Some((&G_A_SPEC_WL5_G, 0)),
        (0, 6, 6) => Some((&G_A_SPEC_WL6_G, 0)),
        (0, 6, 7) => Some((&G_A_SPEC_WL7_G, 0)),
        (0, 7, 1) => Some((&G_A_SPEC_WL1_H, 0)),
        (0, 7, 2) => Some((&G_A_SPEC_WL2_H, 0)),
        (0, 7, 3) => Some((&G_A_SPEC_WL3_H, 0)),
        (0, 7, 4) => Some((&G_A_SPEC_WL4_H, 0)),
        (0, 7, 5) => Some((&G_A_SPEC_WL5_H, 0)),
        (0, 7, 6) => Some((&G_A_SPEC_WL6_H, 0)),
        (0, 7, 7) => Some((&G_A_SPEC_WL7_A, 0)),
        (1, 0, 1) => Some((&G_A_SPEC_WL1_I, 0)),
        (1, 0, 2) => Some((&G_A_SPEC_WL2_I, 0)),
        (1, 0, 3) => Some((&G_A_SPEC_WL3_I, 0)),
        (1, 0, 4) => Some((&G_A_SPEC_WL4_I, 0)),
        (1, 0, 5) => Some((&G_A_SPEC_WL5_I, 0)),
        (1, 0, 6) => Some((&G_A_SPEC_WL6_A, 0)),
        (1, 0, 7) => Some((&G_A_SPEC_WL7_H, 0)),
        (1, 1, 1) => Some((&G_A_SPEC_WL1_C, 0)),
        (1, 1, 2) => Some((&G_A_SPEC_WL2_J, 0)),
        (1, 1, 3) => Some((&G_A_SPEC_WL3_B, 0)),
        (1, 1, 4) => Some((&G_A_SPEC_WL4_J, 0)),
        (1, 1, 5) => Some((&G_A_SPEC_WL5_B, 0)),
        (1, 1, 6) => Some((&G_A_SPEC_WL6_I, 0)),
        (1, 1, 7) => Some((&G_A_SPEC_WL7_A, 0)),
        (1, 2, 1) => Some((&G_A_SPEC_WL1_E, 0)),
        (1, 2, 2) => Some((&G_A_SPEC_WL2_D, 0)),
        (1, 2, 3) => Some((&G_A_SPEC_WL3_A, 0)),
        (1, 2, 4) => Some((&G_A_SPEC_WL4_E, 0)),
        (1, 2, 5) => Some((&G_A_SPEC_WL5_I, 0)),
        (1, 2, 6) => Some((&G_A_SPEC_WL6_J, 0)),
        (1, 2, 7) => Some((&G_A_SPEC_WL7_A, 0)),
        (1, 3, 1) => Some((&G_A_SPEC_WL1_F, 0)),
        (1, 3, 2) => Some((&G_A_SPEC_WL2_K, 0)),
        (1, 3, 3) => Some((&G_A_SPEC_WL3_J, 0)),
        (1, 3, 4) => Some((&G_A_SPEC_WL4_I, 0)),
        (1, 3, 5) => Some((&G_A_SPEC_WL5_J, 0)),
        (1, 3, 6) => Some((&G_A_SPEC_WL6_J, 0)),
        (1, 3, 7) => Some((&G_A_SPEC_WL7_I, 0)),
        (1, 4, 1) => Some((&G_A_SPEC_WL1_J, 0)),
        (1, 4, 2) => Some((&G_A_SPEC_WL2_L, 0)),
        (1, 4, 3) => Some((&G_A_SPEC_WL3_K, 0)),
        (1, 4, 4) => Some((&G_A_SPEC_WL4_J, 0)),
        (1, 4, 5) => Some((&G_A_SPEC_WL5_E, 0)),
        (1, 4, 6) => Some((&G_A_SPEC_WL6_B, 0)),
        (1, 4, 7) => Some((&G_A_SPEC_WL7_J, 0)),
        (1, 5, 1) => Some((&G_A_SPEC_WL1_G, 0)),
        (1, 5, 2) => Some((&G_A_SPEC_WL2_M, 0)),
        (1, 5, 3) => Some((&G_A_SPEC_WL3_L, 0)),
        (1, 5, 4) => Some((&G_A_SPEC_WL4_C, 0)),
        (1, 5, 5) => Some((&G_A_SPEC_WL5_F, 0)),
        (1, 5, 6) => Some((&G_A_SPEC_WL6_K, 0)),
        (1, 5, 7) => Some((&G_A_SPEC_WL7_H, 0)),
        (1, 6, 1) => Some((&G_A_SPEC_WL1_E, 0)),
        (1, 6, 2) => Some((&G_A_SPEC_WL2_N, 0)),
        (1, 6, 3) => Some((&G_A_SPEC_WL3_M, 0)),
        (1, 6, 4) => Some((&G_A_SPEC_WL4_K, 0)),
        (1, 6, 5) => Some((&G_A_SPEC_WL5_K, 0)),
        (1, 6, 6) => Some((&G_A_SPEC_WL6_L, 0)),
        (1, 6, 7) => Some((&G_A_SPEC_WL7_K, 0)),
        (1, 7, 1) => Some((&G_A_SPEC_WL1_K, 0)),
        (1, 7, 2) => Some((&G_A_SPEC_WL2_K, 0)),
        (1, 7, 3) => Some((&G_A_SPEC_WL3_N, 0)),
        (1, 7, 4) => Some((&G_A_SPEC_WL4_L, 0)),
        (1, 7, 5) => Some((&G_A_SPEC_WL5_L, 0)),
        (1, 7, 6) => Some((&G_A_SPEC_WL6_G, 0)),
        (1, 7, 7) => Some((&G_A_SPEC_WL7_L, 0)),
        _ => None,
    }
}

/// Fetch the descriptor for `(state, candidate, word_length)`; control
/// bytes come from the generated `g_aaa_hcspec` bytes at the native
/// offsets, the code-table pointer from the relocation-derived match.
pub fn quant_cost_descriptor_at5(
    state: usize,
    candidate: usize,
    word_length: usize,
) -> Option<QuantCostDescriptor> {
    if state >= QUANT_COST_STATES
        || candidate >= QUANT_COST_CANDIDATES
        || word_length == 0
        || word_length > QUANT_COST_WORD_LENGTHS
    {
        return None;
    }
    let (table, table_zero_offset) = descriptor_table(state, candidate, word_length)?;
    // g_aaa_hcspec starts at native 0xf4240; descriptors index from
    // 0xf4228, so subtract the 0x18 bias.
    let offset = state * 0x540 + candidate * 0xa8 + word_length * 0x18 - 0x18;
    Some(QuantCostDescriptor {
        table,
        table_zero_offset,
        buffer_selector: G_AAA_HCSPEC[offset + 0x11],
        mode: G_AAA_HCSPEC[offset + 0x12],
        count_shift: G_AAA_HCSPEC[offset + 0x13],
        seed_nonzero: G_AAA_HCSPEC[offset + 0x14] != 0,
    })
}

/// Code length for a symbol: byte at `table + 2 + symbol * 4` (each
/// code-table entry is 4 bytes with the bit length at `+2`).
fn code_length(descriptor: &QuantCostDescriptor, symbol: i16) -> u16 {
    let index = (descriptor.table_zero_offset as isize + 2 + symbol as isize * 4) as usize;
    u16::from(descriptor.table[index])
}

/// The three cost-accumulation modes over a grouped symbol buffer
/// (decompile `_L552` loop bodies). `iterations = count >> count_shift`
/// symbols are consumed four at a time.
pub fn quant_cost_accumulate_at5(
    descriptor: &QuantCostDescriptor,
    symbols: &[i16],
    count: usize,
    nonzero_count: u16,
) -> u16 {
    let iterations = count >> (descriptor.count_shift & 0x1f);
    let mut cost: u16 = if descriptor.seed_nonzero {
        nonzero_count
    } else {
        0
    };
    if descriptor.mode & 4 != 0 {
        let mut index = 0usize;
        while index < iterations {
            cost = cost.wrapping_add(1);
            let (s0, s1, s2, s3) = (
                symbols[index],
                symbols[index + 1],
                symbols[index + 2],
                symbols[index + 3],
            );
            if s0 != 0 || s1 != 0 || s2 != 0 || s3 != 0 {
                cost = cost
                    .wrapping_add(code_length(descriptor, s0))
                    .wrapping_add(code_length(descriptor, s1))
                    .wrapping_add(code_length(descriptor, s2))
                    .wrapping_add(code_length(descriptor, s3));
            }
            index += 4;
        }
    } else if descriptor.mode & 2 != 0 {
        let mut index = 0usize;
        while index < iterations {
            let (s0, s1, s2, s3) = (
                symbols[index],
                symbols[index + 1],
                symbols[index + 2],
                symbols[index + 3],
            );
            if s0 == 0 && s1 == 0 {
                cost = cost.wrapping_add(2);
            } else {
                cost = cost
                    .wrapping_add(code_length(descriptor, s0))
                    .wrapping_add(code_length(descriptor, s1))
                    .wrapping_add(2);
            }
            if s2 != 0 || s3 != 0 {
                cost = cost
                    .wrapping_add(code_length(descriptor, s2))
                    .wrapping_add(code_length(descriptor, s3));
            }
            index += 4;
        }
    } else if descriptor.mode & 1 != 0 {
        let mut index = 0usize;
        while index < iterations {
            cost = cost
                .wrapping_add(code_length(descriptor, symbols[index]))
                .wrapping_add(code_length(descriptor, symbols[index + 1]))
                .wrapping_add(code_length(descriptor, symbols[index + 2]))
                .wrapping_add(code_length(descriptor, symbols[index + 3]));
            index += 4;
        }
    }
    cost
}

use crate::bitstream::group::hc_mkgrp_ex_at5;
use crate::coding::quant::{QuantError, quant_at5};
use crate::tables::generated::{SAA_MASK, SAA_WL};

/// The composed `quant_nontone_nspecs_at5` cost surface for one band
/// (native `0xc150`): quantize the spectrum via `QUANT_at5`, fold
/// absolute values with a nonzero count, group the quantized and
/// absolute symbol streams through `hc_mkgrp_Ex_at5` (quantized domain
/// uses `saa_wl`/`saa_mask` row 0, absolute domain row 1), and
/// accumulate each candidate descriptor's cost. Returns the eight
/// candidate cost shorts (the caller's `+0xb88` row).
#[allow(clippy::too_many_arguments)]
pub fn quant_nontone_costs_at5(
    spectrum: &[f32],
    word_length: usize,
    idsf: usize,
    threshold_scale: f32,
    count: usize,
    state: usize,
    candidate_count: usize,
) -> Result<[u16; QUANT_COST_CANDIDATES], QuantError> {
    let mut quantized = vec![0i16; count];
    quant_at5(
        spectrum,
        &mut quantized,
        word_length,
        idsf,
        threshold_scale,
        count,
    )?;

    let mut absolute = vec![0i16; count];
    let mut nonzero: u16 = 0;
    for (dst, &value) in absolute.iter_mut().zip(&quantized) {
        *dst = value.wrapping_abs();
        nonzero += u16::from(value != 0);
    }

    let to_bytes = |symbols: &[i16]| -> Vec<u8> {
        symbols
            .iter()
            .flat_map(|value| (*value as u16).to_le_bytes())
            .collect()
    };
    let quant_bytes = to_bytes(&quantized);
    let abs_bytes = to_bytes(&absolute);

    let group = |bytes: &[u8], group_size: usize, row: usize| -> Vec<i16> {
        let bit_width = SAA_WL[row * 8 + word_length];
        let mask = u16::from(SAA_MASK[row * 8 + word_length]);
        hc_mkgrp_ex_at5(bytes, count, group_size, bit_width, mask)
            .map(|values| values.into_iter().map(|value| value as i16).collect())
            .unwrap_or_default()
    };

    let mut costs = [0u16; QUANT_COST_CANDIDATES];
    let mut buffers: [Option<Vec<i16>>; 6] = [None, None, None, None, None, None];
    for candidate in 0..candidate_count.min(QUANT_COST_CANDIDATES) {
        let Some(descriptor) = quant_cost_descriptor_at5(state, candidate, word_length) else {
            continue;
        };
        // Buffer cache slots: quant raw/grp2/grp4 then abs raw/grp2/grp4.
        let domain = usize::from(descriptor.seed_nonzero);
        let slot = domain * 3
            + match descriptor.buffer_selector {
                1 => 0,
                2 => 1,
                _ => 2,
            };
        if buffers[slot].is_none() {
            buffers[slot] = Some(match (domain, descriptor.buffer_selector) {
                (0, 1) => group(&quant_bytes, 1, 0),
                (0, 2) => group(&quant_bytes, 2, 0),
                (0, _) => group(&quant_bytes, 4, 0),
                (1, 1) => absolute.clone(),
                (1, 2) => group(&abs_bytes, 2, 1),
                (_, _) => group(&abs_bytes, 4, 1),
            });
        }
        let symbols = buffers[slot].as_ref().expect("buffer just prepared");
        costs[candidate] = quant_cost_accumulate_at5(&descriptor, symbols, count, nonzero);
    }
    Ok(costs)
}
