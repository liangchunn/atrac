//! Gain-control constants reproduced from `libatrac.so.1.2.0`.
//!
//! ## Table orientation
//!
//! The library stores two gain tables:
//!
//! - `gaintable` (`0xC3160`, 24 `f32`): **encode-side** ascending levels
//!   `[2^(i-4) for i in 0..16]` followed by 8 interpolation values. This is
//!   the inverse of the codex `GAIN_LEVEL`.
//! - `gaintableR.2` (`0xC4380`, 24 `f32`): **decode-side** descending levels
//!   `[2^(4-i) for i in 0..16]` (matches codex `GAIN_LEVEL`) followed by 8
//!   interpolation values (matches codex `GAIN_INTERPOLATION[15..23]`).
//!
//! The level-index → exponent mapping is stored separately as `LNGAIN`
//! (`0xC4900`, 16 `int32`): `[−4, −3, …, 11]`, i.e. `exponent = level_index − 4`.
//! `lngainof_id_at3` (`0x657c8`) is a simple table lookup; `idof_lngain_at3`
//! (`0x657f4`) is the inverse linear scan.

#![allow(clippy::excessive_precision)]

/// Level index → fixed-point exponent (`LNGAIN` at `0xC4900`).
///
/// `LNGAIN_EXPONENTS[i] = i - 4` for `i` in `0..16`. The actual linear gain
/// is `2^LNGAIN_EXPONENTS[i]`.
pub const LNGAIN_EXPONENTS: [i32; 16] = [-4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

/// Encode-side gain levels (`gaintable[0..16]` at `0xC3160`).
///
/// `GAIN_LEVEL_ENCODE[i] = 2^(i-4)` — ascending from `0.0625` to `2048.0`.
/// This is the reciprocal of `GAIN_LEVEL_DECODE`.
pub const GAIN_LEVEL_ENCODE: [f32; 16] = [
    0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0,
    2048.0,
];

/// Decode-side gain levels (`gaintableR.2[0..16]` at `0xC4380`).
///
/// `GAIN_LEVEL_DECODE[i] = 2^(4-i)` — descending from `16.0` to `0.000488…`.
/// Matches the codex `at3::data::GAIN_LEVEL`.
pub const GAIN_LEVEL_DECODE: [f32; 16] = [
    16.0,
    8.0,
    4.0,
    2.0,
    1.0,
    0.5,
    0.25,
    0.125,
    0.0625,
    0.03125,
    0.015625,
    0.0078125,
    0.00390625,
    0.001953125,
    0.0009765625,
    0.00048828125,
];

/// Decode-side interpolation values (`gaintableR.2[16..24]` at `0xC4380`).
///
/// `GAIN_INTERPOLATION_DECODE[i] = 2^(-i/8)` for `i` in `0..8`. Matches the
/// codex `GAIN_INTERPOLATION[15..23]`.
pub const GAIN_INTERPOLATION_DECODE: [f32; 8] = [
    1.0,
    0.9170040488243103,
    0.8408964276313782,
    0.7711054086685181,
    0.7071067690849304,
    0.6484197974205017,
    0.5946035385131836,
    0.5452538728713989,
];

/// Encode-side interpolation values (`gaintable[16..24]` at `0xC3160`).
///
/// These are used by the encoder's gain analysis and do not have a simple
/// closed-form relationship to the decode-side interpolation values. Stored
/// verbatim from the dumped table.
pub const GAIN_INTERPOLATION_ENCODE: [f32; 8] = [
    0.0625,
    0.06142211705446243,
    0.060600221157073975,
    0.06005748733878136,
    0.05981917306780815,
    0.05991283804178238,
    0.060368526726961136,
    0.061219003051519394,
];
