//! Scalar quantization leaves (`QUANT_at5` at native `0x39ab0`,
//! decompile line 31226).

use crate::tables::at5::{FQF_AT5_ENTRIES, TTVAL_AT5_ENTRIES, fqf_at5, ttval_at5};

/// The magic constant `12582912.0` (`2^23 + 2^22`): adding it to a
/// small float leaves the round-to-nearest-even integer in the low
/// mantissa bits of the f32 representation.
const QUANT_MAGIC_AT5: f64 = 12_582_912.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantError {
    WordLengthOutOfRange { idwl: usize },
    ScaleFactorOutOfRange { idsf: usize },
    CountNotLaneAligned { count: usize },
    InputTooShort { needed: usize, actual: usize },
    OutputTooShort { needed: usize, actual: usize },
}

/// Native `QUANT_at5`: quantize `count` spectral samples at word-length
/// index `idwl` and scale-factor index `idsf`. Each output is the low
/// 16 bits of the f32 representation of `x * g_a_fqf_at5[idwl] + 2^23
/// + 2^22` (the magic-number round-to-nearest-even), masked to zero
/// unless the f32 difference `threshold - |x|` has its sign bit set
/// (i.e. `|x|` strictly exceeds the threshold
/// `g_aa_ttval_at5[idwl * 16 + idsf] * threshold_scale`; the native
/// mask is the arithmetic shift of the f32 bit pattern, so a `-0.0`
/// difference also keeps the value). `count` must be a multiple of 4
/// (the native loop writes four lanes per iteration).
pub fn quant_at5(
    spectrum: &[f32],
    output: &mut [i16],
    idwl: usize,
    idsf: usize,
    threshold_scale: f32,
    count: usize,
) -> Result<(), QuantError> {
    if idwl >= FQF_AT5_ENTRIES {
        return Err(QuantError::WordLengthOutOfRange { idwl });
    }
    if idwl * 16 + idsf >= TTVAL_AT5_ENTRIES {
        return Err(QuantError::ScaleFactorOutOfRange { idsf });
    }
    if count % 4 != 0 {
        return Err(QuantError::CountNotLaneAligned { count });
    }
    if spectrum.len() < count {
        return Err(QuantError::InputTooShort {
            needed: count,
            actual: spectrum.len(),
        });
    }
    if output.len() < count {
        return Err(QuantError::OutputTooShort {
            needed: count,
            actual: output.len(),
        });
    }

    let fqf = f64::from(fqf_at5()[idwl]);
    let threshold = f64::from(ttval_at5()[idwl * 16 + idsf] * threshold_scale);

    for index in 0..count {
        let sample = f64::from(spectrum[index]);
        let rounded_bits = ((sample * fqf + QUANT_MAGIC_AT5) as f32).to_bits();
        let difference = (threshold - sample.abs()) as f32;
        let keep = difference.to_bits() & 0x8000_0000 != 0;
        output[index] = if keep { rounded_bits as u16 as i16 } else { 0 };
    }

    Ok(())
}
