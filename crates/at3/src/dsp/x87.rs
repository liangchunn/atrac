use std::cmp::Ordering;

const SIGN_MASK: u16 = 0x8000;
const EXP_MASK: u16 = 0x7fff;
const EXP_INF_NAN: u16 = 0x7fff;
const BIAS: i32 = 16383;
const MIN_EXP: i32 = -16382;
const MAX_EXP: i32 = 16383;
const INTEGER_BIT: u64 = 1u64 << 63;
const QUIET_BIT: u64 = 1u64 << 62;
const EXTRA_BITS: u32 = 63;
const TARGET_TOP: u32 = 63 + EXTRA_BITS;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ext80 {
    pub sign_exp: u16,
    pub signif: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ext80Class {
    Zero,
    Subnormal,
    Normal,
    Infinite,
    QuietNaN,
    SignalingNaN,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundingMode {
    NearestEven,
    Down,
    Up,
    TowardZero,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct X87Control {
    pub rounding: RoundingMode,
}

impl Default for X87Control {
    fn default() -> Self {
        Self {
            rounding: RoundingMode::NearestEven,
        }
    }
}

#[cfg(feature = "bit-perfect")]
pub type X87Real = Ext80;

#[cfg(not(feature = "bit-perfect"))]
pub type X87Real = f64;

impl Ext80 {
    pub const fn zero(sign: bool) -> Self {
        Self {
            sign_exp: if sign { SIGN_MASK } else { 0 },
            signif: 0,
        }
    }

    const fn infinity(sign: bool) -> Self {
        Self {
            sign_exp: (if sign { SIGN_MASK } else { 0 }) | EXP_INF_NAN,
            signif: INTEGER_BIT,
        }
    }

    const fn quiet_nan() -> Self {
        Self {
            sign_exp: EXP_INF_NAN,
            signif: INTEGER_BIT | QUIET_BIT,
        }
    }

    fn sign(self) -> bool {
        (self.sign_exp & SIGN_MASK) != 0
    }

    fn exponent_bits(self) -> u16 {
        self.sign_exp & EXP_MASK
    }

    fn class(self) -> Ext80Class {
        let exp = self.exponent_bits();
        if exp == 0 {
            if self.signif == 0 {
                Ext80Class::Zero
            } else {
                Ext80Class::Subnormal
            }
        } else if exp == EXP_INF_NAN {
            if self.signif == INTEGER_BIT {
                Ext80Class::Infinite
            } else if (self.signif & QUIET_BIT) != 0 {
                Ext80Class::QuietNaN
            } else {
                Ext80Class::SignalingNaN
            }
        } else if (self.signif & INTEGER_BIT) == 0 {
            Ext80Class::Unsupported
        } else {
            Ext80Class::Normal
        }
    }

    pub const fn from_f32_exact(value: f32) -> Self {
        let bits = value.to_bits();
        let sign = (bits >> 31) != 0;
        let exp = ((bits >> 23) & 0xff) as i32;
        let frac = bits & 0x7f_ffff;
        if exp == 0xff {
            if frac == 0 {
                return Self::infinity(sign);
            }
            return Self {
                sign_exp: (if sign { SIGN_MASK } else { 0 }) | EXP_INF_NAN,
                signif: INTEGER_BIT | QUIET_BIT | ((frac as u64) << (62 - 22)),
            };
        }
        if exp == 0 {
            if frac == 0 {
                return Self::zero(sign);
            }
            let top = 31 - frac.leading_zeros() as i32;
            let exponent = top - 149;
            let signif = (frac as u64) << (63 - top);
            return Self::pack_finite(sign, exponent, signif);
        }
        let exponent = exp - 127;
        let signif = ((1u64 << 23) | frac as u64) << (63 - 23);
        Self::pack_finite(sign, exponent, signif)
    }

    pub fn from_i32_exact(value: i32) -> Self {
        if value == 0 {
            return Self::zero(false);
        }
        let sign = value < 0;
        let magnitude = value.unsigned_abs() as u64;
        let top = 63 - magnitude.leading_zeros() as i32;
        let signif = magnitude << (63 - top);
        Self::pack_finite(sign, top, signif)
    }

    pub fn to_f32(self, rounding: RoundingMode) -> f32 {
        f32::from_bits(self.to_f32_bits(rounding))
    }

    pub fn to_i32_trunc(self) -> Option<i32> {
        if self.class() == Ext80Class::Zero {
            return Some(0);
        }
        let (sign, exp, signif) = self.finite_parts()?;
        if exp < 0 {
            return Some(0);
        }
        let magnitude = if exp >= 63 {
            u128::from(signif) << (exp - 63).min(64)
        } else {
            u128::from(signif >> (63 - exp))
        };
        if sign {
            if magnitude >= 2_147_483_648u128 {
                Some(i32::MIN)
            } else {
                Some(-(magnitude as i32))
            }
        } else if magnitude > i32::MAX as u128 {
            Some(i32::MAX)
        } else {
            Some(magnitude as i32)
        }
    }

    pub fn fneg(self) -> Self {
        Self {
            sign_exp: self.sign_exp ^ SIGN_MASK,
            signif: self.signif,
        }
    }

    pub fn fabs(self) -> Self {
        Self {
            sign_exp: self.sign_exp & EXP_MASK,
            signif: self.signif,
        }
    }

    pub fn fadd(self, rhs: Self, control: X87Control) -> Self {
        match (self.class(), rhs.class()) {
            (Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported, _)
            | (_, Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported) => {
                Self::quiet_nan()
            }
            (Ext80Class::Infinite, Ext80Class::Infinite) if self.sign() != rhs.sign() => {
                Self::quiet_nan()
            }
            (Ext80Class::Infinite, _) => self,
            (_, Ext80Class::Infinite) => rhs,
            (Ext80Class::Zero, Ext80Class::Zero) => {
                if self.sign() == rhs.sign() {
                    self
                } else {
                    Self::zero(control.rounding == RoundingMode::Down)
                }
            }
            (Ext80Class::Zero, _) => rhs,
            (_, Ext80Class::Zero) => self,
            _ => {
                let (sa, ea, siga) = self.finite_parts().unwrap();
                let (sb, eb, sigb) = rhs.finite_parts().unwrap();
                let exp = ea.max(eb);
                let aa = aligned_ext(siga, exp - ea);
                let bb = aligned_ext(sigb, exp - eb);
                let (sign, ext) = if sa == sb {
                    (sa, aa.wrapping_add(bb))
                } else if aa > bb {
                    (sa, aa - bb)
                } else if bb > aa {
                    (sb, bb - aa)
                } else {
                    return Self::zero(control.rounding == RoundingMode::Down);
                };
                round_ext(sign, exp, ext, control.rounding)
            }
        }
    }

    pub fn fsub(self, rhs: Self, control: X87Control) -> Self {
        self.fadd(rhs.fneg(), control)
    }

    pub fn fmul(self, rhs: Self, control: X87Control) -> Self {
        match (self.class(), rhs.class()) {
            (Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported, _)
            | (_, Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported) => {
                Self::quiet_nan()
            }
            (Ext80Class::Zero, Ext80Class::Infinite) | (Ext80Class::Infinite, Ext80Class::Zero) => {
                Self::quiet_nan()
            }
            (Ext80Class::Infinite, _) | (_, Ext80Class::Infinite) => {
                Self::infinity(self.sign() ^ rhs.sign())
            }
            (Ext80Class::Zero, _) | (_, Ext80Class::Zero) => Self::zero(self.sign() ^ rhs.sign()),
            _ => {
                let (sa, ea, siga) = self.finite_parts().unwrap();
                let (sb, eb, sigb) = rhs.finite_parts().unwrap();
                let sign = sa ^ sb;
                let mut exp = ea + eb;
                let mut ext = u128::from(siga) * u128::from(sigb);
                if (ext & (1u128 << 127)) != 0 {
                    ext = shift_right_jam(ext, 1);
                    exp += 1;
                }
                round_ext(sign, exp, ext, control.rounding)
            }
        }
    }

    pub fn fdiv(self, rhs: Self, control: X87Control) -> Self {
        match (self.class(), rhs.class()) {
            (Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported, _)
            | (_, Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported) => {
                Self::quiet_nan()
            }
            (Ext80Class::Zero, Ext80Class::Zero) | (Ext80Class::Infinite, Ext80Class::Infinite) => {
                Self::quiet_nan()
            }
            (_, Ext80Class::Zero) => Self::infinity(self.sign() ^ rhs.sign()),
            (Ext80Class::Zero, _) => Self::zero(self.sign() ^ rhs.sign()),
            (Ext80Class::Infinite, _) => Self::infinity(self.sign() ^ rhs.sign()),
            (_, Ext80Class::Infinite) => Self::zero(self.sign() ^ rhs.sign()),
            _ => {
                let (sa, ea, siga) = self.finite_parts().unwrap();
                let (sb, eb, sigb) = rhs.finite_parts().unwrap();
                let sign = sa ^ sb;
                let mut exp = ea - eb;
                let den = u128::from(sigb);
                let mut rem = u128::from(siga);
                if rem < den {
                    rem <<= 1;
                    exp -= 1;
                }
                let mut ext = 0u128;
                for pos in (1..=TARGET_TOP).rev() {
                    if rem >= den {
                        ext |= 1u128 << pos;
                        rem -= den;
                    }
                    if pos > 1 {
                        rem <<= 1;
                    }
                }
                if rem != 0 {
                    ext |= 1;
                }
                round_ext(sign, exp, ext, control.rounding)
            }
        }
    }

    pub fn compare(self, rhs: Self) -> Option<Ordering> {
        if matches!(
            self.class(),
            Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported
        ) || matches!(
            rhs.class(),
            Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported
        ) {
            return None;
        }
        if self.class() == Ext80Class::Zero && rhs.class() == Ext80Class::Zero {
            return Some(Ordering::Equal);
        }
        if self.sign() != rhs.sign() {
            return Some(if self.sign() {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let sign = self.sign();
        let lhs = self.compare_magnitude(rhs);
        Some(if sign { lhs.reverse() } else { lhs })
    }

    fn compare_magnitude(self, rhs: Self) -> Ordering {
        match (self.class(), rhs.class()) {
            (Ext80Class::Infinite, Ext80Class::Infinite) => Ordering::Equal,
            (Ext80Class::Infinite, _) => Ordering::Greater,
            (_, Ext80Class::Infinite) => Ordering::Less,
            _ => {
                let (_, ea, siga) = self.finite_parts().unwrap_or((false, MIN_EXP, 0));
                let (_, eb, sigb) = rhs.finite_parts().unwrap_or((false, MIN_EXP, 0));
                ea.cmp(&eb).then_with(|| siga.cmp(&sigb))
            }
        }
    }

    fn finite_parts(self) -> Option<(bool, i32, u64)> {
        let sign = self.sign();
        match self.class() {
            Ext80Class::Normal => Some((sign, self.exponent_bits() as i32 - BIAS, self.signif)),
            Ext80Class::Subnormal => {
                let top = 63 - self.signif.leading_zeros() as i32;
                let shift = 63 - top;
                Some((sign, MIN_EXP - shift, self.signif << shift))
            }
            _ => None,
        }
    }

    const fn pack_finite(sign: bool, exp: i32, signif: u64) -> Self {
        if signif == 0 {
            return Self::zero(sign);
        }
        if exp > MAX_EXP {
            return Self::infinity(sign);
        }
        let sign_bits = if sign { SIGN_MASK } else { 0 };
        if exp < MIN_EXP {
            let shift = (MIN_EXP - exp) as u32;
            let sub = if shift >= 64 { 0 } else { signif >> shift };
            return Self {
                sign_exp: sign_bits,
                signif: sub,
            };
        }
        Self {
            sign_exp: sign_bits | ((exp + BIAS) as u16),
            signif,
        }
    }

    fn to_f32_bits(self, rounding: RoundingMode) -> u32 {
        let sign = u32::from(self.sign()) << 31;
        match self.class() {
            Ext80Class::Zero => sign,
            Ext80Class::Infinite => sign | 0x7f80_0000,
            Ext80Class::QuietNaN | Ext80Class::SignalingNaN | Ext80Class::Unsupported => {
                sign | 0x7fc0_0000
            }
            Ext80Class::Normal | Ext80Class::Subnormal => {
                let (_, exp, signif) = self.finite_parts().unwrap();
                sign | round_to_f32_bits(self.sign(), exp, signif, rounding)
            }
        }
    }
}

fn round_to_f32_bits(sign: bool, mut exp: i32, signif: u64, mode: RoundingMode) -> u32 {
    const EXP_BITS: u32 = 8;
    const FRAC_BITS: u32 = 23;
    const PRECISION: u32 = FRAC_BITS + 1;
    const BIAS: i32 = 127;
    const MIN_EXP: i32 = 1 - BIAS;
    const MAX_EXP: i32 = BIAS;

    if exp > MAX_EXP {
        return ((1u32 << EXP_BITS) - 1) << FRAC_BITS;
    }
    if exp >= MIN_EXP {
        let shift = 64 - PRECISION;
        let main = u128::from(signif >> shift);
        let lost = u128::from(signif & ((1u64 << shift) - 1));
        let mut rounded = main + u128::from(should_increment(sign, main, lost, shift, mode));
        if rounded == (1u128 << PRECISION) {
            rounded >>= 1;
            exp += 1;
            if exp > MAX_EXP {
                return ((1u32 << EXP_BITS) - 1) << FRAC_BITS;
            }
        }
        let exp_field = (exp + BIAS) as u32;
        return (exp_field << FRAC_BITS) | ((rounded as u32) & ((1u32 << FRAC_BITS) - 1));
    }
    let unit_power = exp - 63 - MIN_EXP + FRAC_BITS as i32;
    let (main, lost, shift) = if unit_power >= 0 {
        (u128::from(signif) << unit_power.min(63), 0u128, 0)
    } else {
        let shift = (-unit_power) as u32;
        if shift >= 64 {
            (0, u128::from(signif), shift)
        } else {
            (
                u128::from(signif >> shift),
                u128::from(signif & ((1u64 << shift) - 1)),
                shift,
            )
        }
    };
    let mut rounded = main + u128::from(should_increment(sign, main, lost, shift, mode));
    let normal_threshold = 1u128 << FRAC_BITS;
    if rounded >= normal_threshold {
        rounded -= normal_threshold;
        return (1u32 << FRAC_BITS) | rounded as u32;
    }
    rounded as u32
}

fn aligned_ext(signif: u64, exp_diff: i32) -> u128 {
    let ext = u128::from(signif) << EXTRA_BITS;
    shift_right_jam(ext, exp_diff as u32)
}

fn round_ext(sign: bool, mut exp: i32, mut ext: u128, mode: RoundingMode) -> Ext80 {
    let Some(normalized) = normalize_ext(exp, ext) else {
        return Ext80::zero(sign);
    };
    (exp, ext) = normalized;
    let main = ext >> EXTRA_BITS;
    let lost = ext & ((1u128 << EXTRA_BITS) - 1);
    let mut rounded = main + u128::from(should_increment(sign, main, lost, EXTRA_BITS, mode));
    if rounded == (1u128 << 64) {
        rounded >>= 1;
        exp += 1;
    }
    Ext80::pack_finite(sign, exp, rounded as u64)
}

fn normalize_ext(mut exp: i32, mut ext: u128) -> Option<(i32, u128)> {
    if ext == 0 {
        return None;
    }
    if (ext & (1u128 << 127)) != 0 {
        ext = shift_right_jam(ext, 1);
        exp += 1;
    }
    if ext < (1u128 << TARGET_TOP) {
        let top = 127 - ext.leading_zeros();
        let shift = TARGET_TOP - top;
        let shift_exp = shift as i32;
        if exp - shift_exp < MIN_EXP - 128 {
            return None;
        }
        ext <<= shift;
        exp -= shift_exp;
    }
    Some((exp, ext))
}

fn should_increment(sign: bool, main: u128, lost: u128, shift: u32, mode: RoundingMode) -> bool {
    if lost == 0 || shift == 0 {
        return false;
    }
    match mode {
        RoundingMode::NearestEven => should_increment_nearest(main, lost, shift),
        RoundingMode::Down => sign,
        RoundingMode::Up => !sign,
        RoundingMode::TowardZero => false,
    }
}

fn should_increment_nearest(main: u128, lost: u128, shift: u32) -> bool {
    if lost == 0 || shift == 0 {
        return false;
    }
    let guard = 1u128 << (shift - 1);
    let rest = lost & (guard - 1);
    (lost & guard) != 0 && (rest != 0 || (main & 1) != 0)
}

fn shift_right_jam(value: u128, shift: u32) -> u128 {
    if shift == 0 {
        value
    } else if shift >= 128 {
        u128::from(value != 0)
    } else {
        let shifted = value >> shift;
        let mask = (1u128 << shift) - 1;
        shifted | u128::from((value & mask) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: X87Control = X87Control {
        rounding: RoundingMode::NearestEven,
    };

    #[test]
    fn exact_basic_encodings() {
        assert_eq!(Ext80::zero(false).sign_exp, 0x0000);
        assert_eq!(Ext80::zero(false).signif, 0);
        assert_eq!(Ext80::zero(true).sign_exp, 0x8000);
        assert_eq!(Ext80::from_f32_exact(1.0).sign_exp, 0x3fff);
        assert_eq!(Ext80::from_f32_exact(1.0).signif, 0x8000_0000_0000_0000);
        assert_eq!(Ext80::from_f32_exact(-1.0).sign_exp, 0xbfff);
        assert_eq!(Ext80::from_f32_exact(2.0).sign_exp, 0x4000);
        assert_eq!(Ext80::from_f32_exact(0.5).sign_exp, 0x3ffe);
    }

    #[test]
    fn f32_roundtrip_representative_values() {
        let values = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            0.5,
            2.0,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::from_bits(0x007f_ffff),
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        for value in values {
            let got = Ext80::from_f32_exact(value).to_f32(RoundingMode::NearestEven);
            assert_eq!(got.to_bits(), value.to_bits(), "value {value:?}");
        }
        assert!(
            Ext80::from_f32_exact(f32::NAN)
                .to_f32(RoundingMode::NearestEven)
                .is_nan()
        );
    }

    #[test]
    fn integer_roundtrip_and_truncation() {
        for value in [-32768, -257, -1, 0, 1, 257, 32767, i32::MAX, i32::MIN] {
            assert_eq!(Ext80::from_i32_exact(value).to_i32_trunc(), Some(value));
        }
        let one_half = Ext80::from_f32_exact(1.5);
        assert_eq!(one_half.to_i32_trunc(), Some(1));
        assert_eq!(one_half.fneg().to_i32_trunc(), Some(-1));
    }

    #[test]
    fn add_sub_mul_div_simple_values() {
        let a = Ext80::from_f32_exact(1.25);
        let b = Ext80::from_f32_exact(0.5);
        assert_eq!(a.fadd(b, C).to_f32(RoundingMode::NearestEven), 1.75);
        assert_eq!(a.fsub(b, C).to_f32(RoundingMode::NearestEven), 0.75);
        assert_eq!(a.fmul(b, C).to_f32(RoundingMode::NearestEven), 0.625);
        assert_eq!(a.fdiv(b, C).to_f32(RoundingMode::NearestEven), 2.5);
    }

    #[test]
    fn cancellation_and_signed_zero() {
        let a = Ext80::from_f32_exact(1024.0);
        let b = Ext80::from_f32_exact(-1024.0);
        assert_eq!(a.fadd(b, C), Ext80::zero(false));
        let down = X87Control {
            rounding: RoundingMode::Down,
        };
        assert_eq!(a.fadd(b, down), Ext80::zero(true));
    }

    #[test]
    fn nearest_even_tie_to_f32() {
        let half_ulp =
            Ext80::from_f32_exact(1.0).fadd(Ext80::from_f32_exact(f32::from_bits(0x3380_0000)), C);
        assert_eq!(
            half_ulp.to_f32(RoundingMode::NearestEven).to_bits(),
            1.0f32.to_bits()
        );
        assert_eq!(
            half_ulp.to_f32(RoundingMode::Up).to_bits(),
            f32::from_bits(0x3f80_0001).to_bits()
        );
    }

    #[test]
    fn overflow_underflow_and_specials() {
        let huge = Ext80::pack_finite(false, MAX_EXP, u64::MAX);
        assert!(huge.fmul(Ext80::from_f32_exact(2.0), C).class() == Ext80Class::Infinite);
        let tiny = Ext80::pack_finite(false, MIN_EXP - 200, INTEGER_BIT);
        assert_eq!(tiny.class(), Ext80Class::Zero);
        assert!(
            Ext80::infinity(false)
                .fadd(Ext80::infinity(true), C)
                .to_f32(RoundingMode::NearestEven)
                .is_nan()
        );
        assert!(
            Ext80::zero(false)
                .fmul(Ext80::infinity(false), C)
                .to_f32(RoundingMode::NearestEven)
                .is_nan()
        );
    }

    #[test]
    fn comparisons() {
        let neg = Ext80::from_f32_exact(-2.0);
        let one = Ext80::from_f32_exact(1.0);
        let two = Ext80::from_f32_exact(2.0);
        assert_eq!(neg.compare(one), Some(Ordering::Less));
        assert_eq!(two.compare(one), Some(Ordering::Greater));
        assert_eq!(
            Ext80::zero(true).compare(Ext80::zero(false)),
            Some(Ordering::Equal)
        );
        assert_eq!(Ext80::quiet_nan().compare(one), None);
    }
}
