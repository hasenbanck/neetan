//! Core arithmetic operations: add, sub, mul, div, sqrt, round_to_int.

use crate::{ExceptionFlags, Fp80, Precision, RoundingMode};

impl Fp80 {
    /// Compute `self + other`. Respects precision control and rounding mode.
    pub fn add(
        self,
        other: Fp80,
        rc: RoundingMode,
        pc: Precision,
        ef: &mut ExceptionFlags,
    ) -> Fp80 {
        // NaN handling
        if self.is_nan() || other.is_nan() {
            return Fp80::propagate_nan(self, other, ef);
        }

        // Denormal operand flag
        if self.is_denormal()
            || self.is_pseudo_denormal()
            || other.is_denormal()
            || other.is_pseudo_denormal()
        {
            ef.denormal = true;
        }

        // Infinity handling
        if self.is_infinity() {
            if other.is_infinity() {
                if self.sign() != other.sign() {
                    ef.invalid = true;
                    return Fp80::INDEFINITE;
                }
                return self;
            }
            return self;
        }
        if other.is_infinity() {
            return other;
        }

        // Zero handling
        if self.is_zero() && other.is_zero() {
            if self.sign() == other.sign() {
                return self;
            }
            return if rc == RoundingMode::Down {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }

        // Extract components
        let a_sign = self.sign();
        let b_sign = other.sign();
        let mut a_exp = self.true_exponent();
        let mut b_exp = other.true_exponent();
        let mut a_sig = self.significand() as u128;
        let mut b_sig = other.significand() as u128;

        // Normalize denormals
        if self.exponent() == 0 && a_sig != 0 {
            let shift = (a_sig as u64).leading_zeros();
            a_sig <<= shift;
            a_exp -= shift as i32;
        }
        if other.exponent() == 0 && b_sig != 0 {
            let shift = (b_sig as u64).leading_zeros();
            b_sig <<= shift;
            b_exp -= shift as i32;
        }

        // Widen to u128 with J-bit at bit 126 (bit 127 free for carry)
        a_sig <<= 63;
        b_sig <<= 63;

        // Align exponents
        if a_exp > b_exp {
            let shift = (a_exp - b_exp) as u32;
            b_sig = shift_right_sticky(b_sig, shift);
        } else if b_exp > a_exp {
            let shift = (b_exp - a_exp) as u32;
            a_sig = shift_right_sticky(a_sig, shift);
            a_exp = b_exp;
        }

        let result_exp;
        let result_sig;
        let result_sign;

        if a_sign == b_sign {
            // Same sign: add magnitudes
            result_sign = a_sign;
            result_sig = a_sig + b_sig;
            result_exp = a_exp;
        } else {
            // Different signs: subtract magnitudes
            if a_sig > b_sig {
                result_sign = a_sign;
                result_sig = a_sig - b_sig;
                result_exp = a_exp;
            } else if b_sig > a_sig {
                result_sign = b_sign;
                result_sig = b_sig - a_sig;
                result_exp = a_exp;
            } else {
                // Exact cancellation
                return if rc == RoundingMode::Down {
                    Fp80::NEG_ZERO
                } else {
                    Fp80::ZERO
                };
            }
        }

        if result_sig == 0 {
            return if rc == RoundingMode::Down {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        // Normalize: shift so that the leading 1 is at bit 127
        let leading = result_sig.leading_zeros();
        let normalized_sig = result_sig << leading;
        // Intermediate has J-bit at 126, round_and_pack expects J-bit at 127
        let rp_exp = result_exp + 1 - leading as i32;

        Fp80::round_and_pack(result_sign, rp_exp, normalized_sig, rc, pc, ef)
    }

    /// Compute `self − other`. Equivalent to `add(self, negate(other), rc, pc)`.
    pub fn sub(
        self,
        other: Fp80,
        rc: RoundingMode,
        pc: Precision,
        ef: &mut ExceptionFlags,
    ) -> Fp80 {
        self.add(other.negate(), rc, pc, ef)
    }

    /// Compute `self × other`. Respects precision control and rounding mode.
    pub fn mul(
        self,
        other: Fp80,
        rc: RoundingMode,
        pc: Precision,
        ef: &mut ExceptionFlags,
    ) -> Fp80 {
        let result_sign = self.sign() ^ other.sign();

        // NaN handling
        if self.is_nan() || other.is_nan() {
            return Fp80::propagate_nan(self, other, ef);
        }

        // Denormal operand flag
        if self.is_denormal()
            || self.is_pseudo_denormal()
            || other.is_denormal()
            || other.is_pseudo_denormal()
        {
            ef.denormal = true;
        }

        // Infinity handling
        if self.is_infinity() {
            if other.is_zero() {
                ef.invalid = true;
                return Fp80::INDEFINITE;
            }
            return if result_sign {
                Fp80::NEG_INFINITY
            } else {
                Fp80::INFINITY
            };
        }
        if other.is_infinity() {
            if self.is_zero() {
                ef.invalid = true;
                return Fp80::INDEFINITE;
            }
            return if result_sign {
                Fp80::NEG_INFINITY
            } else {
                Fp80::INFINITY
            };
        }

        // Zero handling
        if self.is_zero() || other.is_zero() {
            return if result_sign {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        // Extract and normalize
        let mut a_exp = self.true_exponent();
        let mut a_sig = self.significand();
        let mut b_exp = other.true_exponent();
        let mut b_sig = other.significand();

        if self.exponent() == 0 {
            let (ns, shift) = Fp80::normalize_significand(a_sig);
            a_sig = ns;
            a_exp -= shift as i32;
        }
        if other.exponent() == 0 {
            let (ns, shift) = Fp80::normalize_significand(b_sig);
            b_sig = ns;
            b_exp -= shift as i32;
        }

        // 64×64 → 128-bit product. Product at bit 126 (min) to 127 (max).
        let product = (a_sig as u128) * (b_sig as u128);

        if product == 0 {
            return if result_sign {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        let leading = product.leading_zeros();
        let normalized_sig = product << leading;
        // Product naturally at bit 126 (same as add intermediate).
        let rp_exp = a_exp + b_exp + 1 - leading as i32;

        Fp80::round_and_pack(result_sign, rp_exp, normalized_sig, rc, pc, ef)
    }

    /// Compute `self / other`. Respects precision control and rounding mode.
    pub fn div(
        self,
        other: Fp80,
        rc: RoundingMode,
        pc: Precision,
        ef: &mut ExceptionFlags,
    ) -> Fp80 {
        let result_sign = self.sign() ^ other.sign();

        // NaN handling
        if self.is_nan() || other.is_nan() {
            return Fp80::propagate_nan(self, other, ef);
        }

        // Denormal operand flag
        if self.is_denormal()
            || self.is_pseudo_denormal()
            || other.is_denormal()
            || other.is_pseudo_denormal()
        {
            ef.denormal = true;
        }

        // Infinity / Infinity = Indefinite + IE
        if self.is_infinity() && other.is_infinity() {
            ef.invalid = true;
            return Fp80::INDEFINITE;
        }

        // 0 / 0 = Indefinite + IE
        if self.is_zero() && other.is_zero() {
            ef.invalid = true;
            return Fp80::INDEFINITE;
        }

        // Infinity / finite = Infinity
        if self.is_infinity() {
            return if result_sign {
                Fp80::NEG_INFINITY
            } else {
                Fp80::INFINITY
            };
        }

        // finite / Infinity = 0
        if other.is_infinity() {
            return if result_sign {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        // nonzero / 0 = Infinity + ZE
        if other.is_zero() {
            ef.zero_divide = true;
            return if result_sign {
                Fp80::NEG_INFINITY
            } else {
                Fp80::INFINITY
            };
        }

        // 0 / nonzero = 0
        if self.is_zero() {
            return if result_sign {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        // Extract and normalize
        let mut a_exp = self.true_exponent();
        let mut a_sig = self.significand();
        let mut b_exp = other.true_exponent();
        let mut b_sig = other.significand();

        if self.exponent() == 0 {
            let (ns, shift) = Fp80::normalize_significand(a_sig);
            a_sig = ns;
            a_exp -= shift as i32;
        }
        if other.exponent() == 0 {
            let (ns, shift) = Fp80::normalize_significand(b_sig);
            b_sig = ns;
            b_exp -= shift as i32;
        }

        // (sig_a << 64) / sig_b → quotient at ~bit 64
        let dividend = (a_sig as u128) << 64;
        let divisor = b_sig as u128;
        let quotient = dividend / divisor;
        let remainder = dividend % divisor;

        if quotient == 0 {
            return if result_sign {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        let leading = quotient.leading_zeros();
        // Set sticky AFTER normalization so it lands in the discarded bits
        let normalized_sig = if remainder != 0 {
            (quotient << leading) | 1
        } else {
            quotient << leading
        };
        let rp_exp = a_exp - b_exp + 63 - leading as i32;

        Fp80::round_and_pack(result_sign, rp_exp, normalized_sig, rc, pc, ef)
    }

    /// Compute `√self`. Respects precision control and rounding mode.
    pub fn sqrt(self, rc: RoundingMode, pc: Precision, ef: &mut ExceptionFlags) -> Fp80 {
        // NaN handling
        if self.is_signaling_nan() {
            ef.invalid = true;
            return self.quieten();
        }
        if self.is_quiet_nan() {
            return self;
        }

        // Denormal sets DE
        if self.is_denormal() || self.is_pseudo_denormal() {
            ef.denormal = true;
        }

        // sqrt(+0) = +0, sqrt(-0) = -0
        if self.is_zero() {
            return self;
        }

        // sqrt(+inf) = +inf
        if self.is_infinity() && !self.sign() {
            return Fp80::INFINITY;
        }

        // sqrt(negative) or sqrt(-inf) = Indefinite + IE
        if self.sign() {
            ef.invalid = true;
            return Fp80::INDEFINITE;
        }

        let mut exp = self.true_exponent();
        let mut sig = self.significand();

        if self.exponent() == 0 {
            let (ns, shift) = Fp80::normalize_significand(sig);
            sig = ns;
            exp -= shift as i32;
        }

        // P = exp - 63 (value = sig * 2^P). Need (P - K) even for integer sqrt.
        let p = exp - 63;
        let (sig_wide, k) = if p % 2 == 0 {
            ((sig as u128) << 64, 64i32) // K=64, P-K even
        } else {
            ((sig as u128) << 63, 63i32) // K=63, P-K even
        };

        let (root, exact) = sqrt_u128_with_remainder(sig_wide);

        if root == 0 {
            return Fp80::ZERO;
        }

        let leading = root.leading_zeros();
        let normalized_sig = if exact {
            root << leading
        } else {
            (root << leading) | 1
        };
        // rp_exp = (P - K)/2 - leading + 127
        let rp_exp = (p - k) / 2 - leading as i32 + 127;

        Fp80::round_and_pack(false, rp_exp, normalized_sig, rc, pc, ef)
    }

    /// Round to integer value, returned as Fp80 (FRNDINT).
    /// Does not respect precision control - always uses full 64-bit significand.
    pub fn round_to_int(self, rc: RoundingMode, ef: &mut ExceptionFlags) -> Fp80 {
        // NaN handling
        if self.is_signaling_nan() {
            ef.invalid = true;
            return self.quieten();
        }
        if self.is_quiet_nan() {
            return self;
        }

        // Infinity and zero returned unchanged
        if self.is_infinity() || self.is_zero() {
            return self;
        }

        let sign = self.sign();
        let exp = self.true_exponent();
        let sig = self.significand();

        // If exponent >= 63, the value is already an integer (all fraction bits are integer bits)
        if exp >= 63 {
            return self;
        }

        // If exponent < 0, the value is between -1 and 1 (exclusive)
        if exp < 0 {
            ef.precision = true;
            let round_up = match rc {
                RoundingMode::NearestEven => {
                    if exp == -1 {
                        // Value is in [0.5, 1.0) - check if exactly 0.5 (round to even → 0)
                        // or > 0.5 (round up to 1)
                        // sig with exp=-1 means the value is sig * 2^(-1) / 2^63 = sig / 2^64
                        // Value = sig * 2^(exp-63). For exp=-1, value = sig * 2^(-64).
                        // With J-bit set: sig >= 2^63, so value >= 0.5.
                        // Exactly 0.5: sig = 2^63 (0x8000_0000_0000_0000).
                        sig > 0x8000_0000_0000_0000
                    } else {
                        false
                    }
                }
                RoundingMode::Up => !sign,
                RoundingMode::Down => sign,
                RoundingMode::Zero => false,
            };
            return if round_up {
                if sign {
                    Fp80::from_bits(0xBFFF, 0x8000_0000_0000_0000) // -1.0
                } else {
                    Fp80::ONE
                }
            } else if sign {
                Fp80::NEG_ZERO
            } else {
                Fp80::ZERO
            };
        }

        // exp is in [0, 62]. The integer part occupies bits 63 down to (63-exp),
        // and the fraction occupies bits (62-exp) down to 0.
        let fraction_bits = (63 - exp) as u32;
        let fraction_mask = (1u64 << fraction_bits) - 1;
        let fraction = sig & fraction_mask;

        if fraction == 0 {
            return self; // already an integer
        }

        ef.precision = true;
        let integer_part = sig & !fraction_mask;

        let round_up = match rc {
            RoundingMode::NearestEven => {
                let halfway = 1u64 << (fraction_bits - 1);
                if fraction > halfway {
                    true
                } else if fraction == halfway {
                    // Tie: round to even - check the LSB of the integer part
                    (integer_part >> fraction_bits) & 1 != 0
                } else {
                    false
                }
            }
            RoundingMode::Up => !sign,
            RoundingMode::Down => sign,
            RoundingMode::Zero => false,
        };

        let result_sig = if round_up {
            let increment = 1u64 << fraction_bits;
            let new_sig = integer_part.wrapping_add(increment);
            if new_sig < integer_part {
                // Overflow of the significand - carry into exponent
                // This means we go from e.g. 0xFFFF... to 0x10000...
                // which is 1.0 * 2^(exp+1)
                let new_exp = (self.exponent() + 1) & 0x7FFF;
                let se = if sign { 0x8000 | new_exp } else { new_exp };
                return Fp80::from_bits(se, 0x8000_0000_0000_0000);
            }
            new_sig
        } else {
            integer_part
        };

        Fp80::from_bits(self.sign_exponent, result_sig)
    }
}

/// Integer square root of a u128 value, returning (root, exact).
/// The root is such that root^2 <= val < (root+1)^2.
/// `exact` is true if root^2 == val.
fn shift_right_sticky(val: u128, shift: u32) -> u128 {
    if shift == 0 {
        return val;
    }
    if shift >= 128 {
        return if val != 0 { 1 } else { 0 };
    }
    let lost = val & ((1u128 << shift) - 1);
    let shifted = val >> shift;
    if lost != 0 { shifted | 1 } else { shifted }
}

fn sqrt_u128_with_remainder(val: u128) -> (u128, bool) {
    if val == 0 {
        return (0, true);
    }

    // Start with a good initial estimate using leading zeros
    let bits = 128 - val.leading_zeros();
    let mut root = 1u128 << bits.div_ceil(2);

    // Newton's method
    loop {
        let next = (root + val / root) >> 1;
        if next >= root {
            break;
        }
        root = next;
    }

    // Verify and adjust
    let sq = root.checked_mul(root);
    match sq {
        Some(sq) if sq == val => (root, true),
        Some(sq) if sq > val => {
            root -= 1;
            let sq = root * root;
            (root, sq == val)
        }
        _ => (root, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEGATIVE_ONE: Fp80 = Fp80::from_bits(0xBFFF, 0x8000_0000_0000_0000);
    const TWO: Fp80 = Fp80::from_bits(0x4000, 0x8000_0000_0000_0000);
    const THREE: Fp80 = Fp80::from_bits(0x4000, 0xC000_0000_0000_0000);
    const FOUR: Fp80 = Fp80::from_bits(0x4001, 0x8000_0000_0000_0000);
    const SIX: Fp80 = Fp80::from_bits(0x4001, 0xC000_0000_0000_0000);
    const ONE_AND_HALF: Fp80 = Fp80::from_bits(0x3FFF, 0xC000_0000_0000_0000);
    const TWO_AND_HALF: Fp80 = Fp80::from_bits(0x4000, 0xA000_0000_0000_0000);
    const NEGATIVE_ONE_AND_HALF: Fp80 = Fp80::from_bits(0xBFFF, 0xC000_0000_0000_0000);
    const NEGATIVE_TWO: Fp80 = Fp80::from_bits(0xC000, 0x8000_0000_0000_0000);
    const POSITIVE_SNAN: Fp80 = Fp80::from_bits(0x7FFF, 0x8000_0000_0000_0001);
    const POSITIVE_QNAN: Fp80 = Fp80::from_bits(0x7FFF, 0xC000_0000_0000_0001);
    const POSITIVE_DENORMAL: Fp80 = Fp80::from_bits(0x0000, 0x0000_0000_0000_0001);
    const LARGEST_NORMAL: Fp80 = Fp80::from_bits(0x7FFE, 0xFFFF_FFFF_FFFF_FFFF);
    const HALF: Fp80 = Fp80::from_bits(0x3FFE, 0x8000_0000_0000_0000);

    fn no_exceptions() -> ExceptionFlags {
        ExceptionFlags::default()
    }

    #[test]
    fn test_add_basic() {
        let mut ef = no_exceptions();
        let result = Fp80::ONE.add(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, TWO);
        assert_eq!(ef, no_exceptions());

        let mut ef = no_exceptions();
        let result = Fp80::ONE.add(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ONE);

        let mut ef = no_exceptions();
        let result = Fp80::ZERO.add(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);
    }

    #[test]
    fn test_add_edge_cases() {
        // +inf + -inf = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.add(
            Fp80::NEG_INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // -inf + +inf = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_INFINITY.add(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // +inf + +inf = +inf.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.add(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);
        assert_eq!(ef, no_exceptions());

        // -inf + -inf = -inf.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_INFINITY.add(
            Fp80::NEG_INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_INFINITY);

        // inf + finite = inf.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.add(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);

        // +0 + -0 = +0 (default rounding).
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.add(
            Fp80::NEG_ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);
        assert!(!result.sign());

        // +0 + -0 = -0 when RC=Down.
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.add(
            Fp80::NEG_ZERO,
            RoundingMode::Down,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_ZERO);

        // x + (-x) = +0 (default rounding).
        let mut ef = no_exceptions();
        let result = Fp80::ONE.add(
            NEGATIVE_ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);
        assert!(!result.sign());

        // x + (-x) = -0 when RC=Down.
        let mut ef = no_exceptions();
        let result = Fp80::ONE.add(
            NEGATIVE_ONE,
            RoundingMode::Down,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_ZERO);

        // SNaN propagation: SNaN + ONE -> QNaN + IE.
        let mut ef = no_exceptions();
        let result = POSITIVE_SNAN.add(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(result.is_quiet_nan());
        assert!(ef.invalid);

        // QNaN propagation: QNaN + ONE -> QNaN, no IE.
        let mut ef = no_exceptions();
        let result = POSITIVE_QNAN.add(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(result.is_quiet_nan());
        assert!(!ef.invalid);
    }

    #[test]
    fn test_sub_basic() {
        let mut ef = no_exceptions();
        let result = Fp80::ONE.sub(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);
        assert!(!result.sign());

        let mut ef = no_exceptions();
        let result = TWO.sub(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ONE);
    }

    #[test]
    fn test_sub_edge_cases() {
        // +inf - +inf = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.sub(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // +inf - -inf = +inf.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.sub(
            Fp80::NEG_INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);

        // x - x = +0 (or -0 if RC=Down).
        let mut ef = no_exceptions();
        let result = Fp80::ONE.sub(Fp80::ONE, RoundingMode::Down, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::NEG_ZERO);

        // NaN propagation.
        let mut ef = no_exceptions();
        let result = POSITIVE_SNAN.sub(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(result.is_quiet_nan());
        assert!(ef.invalid);
    }

    #[test]
    fn test_mul_basic() {
        let mut ef = no_exceptions();
        let result = TWO.mul(
            THREE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, SIX);
        assert_eq!(ef, no_exceptions());

        // 1 * x = x.
        let mut ef = no_exceptions();
        let result = Fp80::ONE.mul(
            THREE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, THREE);

        // 0 * x = 0.
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.mul(
            THREE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);
    }

    #[test]
    fn test_mul_edge_cases() {
        // inf * 0 = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.mul(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // 0 * inf = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.mul(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // inf * inf = +inf (same sign).
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.mul(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);

        // +inf * -inf = -inf (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.mul(
            Fp80::NEG_INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_INFINITY);

        // Sign of result = XOR of operand signs.
        let mut ef = no_exceptions();
        let result = NEGATIVE_ONE.mul(
            NEGATIVE_ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ONE); // neg * neg = pos

        // -1 * 0 = -0 (sign XOR).
        let mut ef = no_exceptions();
        let result = NEGATIVE_ONE.mul(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_ZERO);

        // NaN propagation.
        let mut ef = no_exceptions();
        let result = POSITIVE_SNAN.mul(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(result.is_quiet_nan());
        assert!(ef.invalid);
    }

    #[test]
    fn test_div_basic() {
        let mut ef = no_exceptions();
        let result = SIX.div(TWO, RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, THREE);
        assert_eq!(ef, no_exceptions());

        // x / 1 = x.
        let mut ef = no_exceptions();
        let result = THREE.div(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, THREE);
    }

    #[test]
    fn test_div_edge_cases() {
        // 0 / 0 = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.div(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // inf / inf = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.div(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // nonzero / 0 = +inf + ZE (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::ONE.div(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);
        assert!(ef.zero_divide);

        // negative / 0 = -inf + ZE.
        let mut ef = no_exceptions();
        let result = NEGATIVE_ONE.div(
            Fp80::ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_INFINITY);
        assert!(ef.zero_divide);

        // 0 / nonzero = 0 (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.div(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);

        // -0 / positive = -0.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_ZERO.div(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_ZERO);

        // inf / finite = inf (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.div(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);

        // finite / inf = 0 (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::ONE.div(
            Fp80::INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);

        // NaN propagation.
        let mut ef = no_exceptions();
        let result = POSITIVE_SNAN.div(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(result.is_quiet_nan());
        assert!(ef.invalid);
    }

    #[test]
    fn test_sqrt_basic() {
        let mut ef = no_exceptions();
        let result = FOUR.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, TWO);
        assert_eq!(ef, no_exceptions());

        let mut ef = no_exceptions();
        let result = Fp80::ONE.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::ONE);
    }

    #[test]
    fn test_sqrt_edge_cases() {
        // sqrt(+0) = +0.
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::ZERO);

        // sqrt(-0) = -0.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_ZERO.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::NEG_ZERO);

        // sqrt(+inf) = +inf.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::INFINITY);

        // sqrt(-inf) = Indefinite + IE.
        let mut ef = no_exceptions();
        let result =
            Fp80::NEG_INFINITY.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // sqrt(negative) = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = NEGATIVE_ONE.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);

        // sqrt(SNaN) = QNaN + IE.
        let mut ef = no_exceptions();
        let result = POSITIVE_SNAN.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert!(result.is_quiet_nan());
        assert!(ef.invalid);

        // sqrt(QNaN) = QNaN, no IE.
        let mut ef = no_exceptions();
        let result = POSITIVE_QNAN.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert!(result.is_quiet_nan());
        assert!(!ef.invalid);
    }

    #[test]
    fn test_round_to_int_basic() {
        // Already integer: unchanged.
        let mut ef = no_exceptions();
        let result = Fp80::ONE.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert_eq!(result, Fp80::ONE);
        assert_eq!(ef, no_exceptions());

        // 1.5 NearestEven -> 2.0 (round to even).
        let mut ef = no_exceptions();
        let result = ONE_AND_HALF.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert_eq!(result, TWO);
        assert!(ef.precision);

        // 2.5 NearestEven -> 2.0 (round to even, not 3).
        let mut ef = no_exceptions();
        let result = TWO_AND_HALF.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert_eq!(result, TWO);
        assert!(ef.precision);
    }

    #[test]
    fn test_round_to_int_edge_cases() {
        // All 4 rounding modes on 1.5.
        let mut ef = no_exceptions();
        assert_eq!(ONE_AND_HALF.round_to_int(RoundingMode::Up, &mut ef), TWO);
        assert!(ef.precision);

        let mut ef = no_exceptions();
        assert_eq!(
            ONE_AND_HALF.round_to_int(RoundingMode::Down, &mut ef),
            Fp80::ONE
        );
        assert!(ef.precision);

        let mut ef = no_exceptions();
        assert_eq!(
            ONE_AND_HALF.round_to_int(RoundingMode::Zero, &mut ef),
            Fp80::ONE
        );
        assert!(ef.precision);

        // Negative: -1.5 rounding modes.
        let mut ef = no_exceptions();
        assert_eq!(
            NEGATIVE_ONE_AND_HALF.round_to_int(RoundingMode::NearestEven, &mut ef),
            NEGATIVE_TWO
        );
        assert!(ef.precision);

        let mut ef = no_exceptions();
        assert_eq!(
            NEGATIVE_ONE_AND_HALF.round_to_int(RoundingMode::Up, &mut ef),
            NEGATIVE_ONE
        );
        assert!(ef.precision);

        let mut ef = no_exceptions();
        assert_eq!(
            NEGATIVE_ONE_AND_HALF.round_to_int(RoundingMode::Down, &mut ef),
            NEGATIVE_TWO
        );
        assert!(ef.precision);

        let mut ef = no_exceptions();
        assert_eq!(
            NEGATIVE_ONE_AND_HALF.round_to_int(RoundingMode::Zero, &mut ef),
            NEGATIVE_ONE
        );
        assert!(ef.precision);

        // NaN returned unchanged (SNaN raises IE).
        let mut ef = no_exceptions();
        let result = POSITIVE_SNAN.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert!(result.is_nan());
        assert!(ef.invalid);

        // Infinity returned unchanged.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert_eq!(result, Fp80::INFINITY);
        assert_eq!(ef, no_exceptions());

        // Zero returned unchanged.
        let mut ef = no_exceptions();
        let result = Fp80::ZERO.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert_eq!(result, Fp80::ZERO);

        // QNaN returned unchanged, no IE.
        let mut ef = no_exceptions();
        let result = POSITIVE_QNAN.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert!(result.is_quiet_nan());
        assert!(!ef.invalid);

        // -0 preserved.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_ZERO.round_to_int(RoundingMode::NearestEven, &mut ef);
        assert_eq!(result, Fp80::NEG_ZERO);
        assert_eq!(ef, no_exceptions());
    }

    #[test]
    fn test_add_same_sign_zeros() {
        // -0 + -0 = -0.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_ZERO.add(
            Fp80::NEG_ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_ZERO);
    }

    #[test]
    fn test_add_denormal_sets_de() {
        // Denormal operand sets DE flag.
        let mut ef = no_exceptions();
        let _result = POSITIVE_DENORMAL.add(
            Fp80::ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(ef.denormal);
    }

    #[test]
    fn test_add_overflow() {
        // Two large normals that overflow to infinity.
        let mut ef = no_exceptions();
        let result = LARGEST_NORMAL.add(
            LARGEST_NORMAL,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);
        assert!(ef.overflow);
    }

    #[test]
    fn test_add_precision_loss() {
        // 1.0 + denormal: result is rounded, PE set.
        let mut ef = no_exceptions();
        let _result = Fp80::ONE.add(
            POSITIVE_DENORMAL,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(ef.precision);
    }

    #[test]
    fn test_add_precision_control() {
        // Single precision: significand rounded to 24 bits.
        let mut ef = no_exceptions();
        let result = Fp80::ONE.add(HALF, RoundingMode::NearestEven, Precision::Single, &mut ef);
        assert_eq!(result, ONE_AND_HALF);

        // Double precision: significand rounded to 53 bits.
        let mut ef = no_exceptions();
        let result = Fp80::ONE.add(HALF, RoundingMode::NearestEven, Precision::Double, &mut ef);
        assert_eq!(result, ONE_AND_HALF);
    }

    #[test]
    fn test_sub_neg_inf_minus_neg_inf() {
        // -inf - (-inf) = Indefinite + IE.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_INFINITY.sub(
            Fp80::NEG_INFINITY,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INDEFINITE);
        assert!(ef.invalid);
    }

    #[test]
    fn test_mul_inf_finite_sign_xor() {
        // +inf * +finite = +inf.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.mul(
            THREE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);

        // +inf * -finite = -inf.
        let mut ef = no_exceptions();
        let result = Fp80::INFINITY.mul(
            NEGATIVE_ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_INFINITY);

        // -inf * +finite = -inf.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_INFINITY.mul(
            THREE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_INFINITY);

        // -inf * -finite = +inf.
        let mut ef = no_exceptions();
        let result = Fp80::NEG_INFINITY.mul(
            NEGATIVE_ONE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::INFINITY);
    }

    #[test]
    fn test_mul_neg_zero_sign_xor() {
        // -0 * -0 = +0 (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::NEG_ZERO.mul(
            Fp80::NEG_ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::ZERO);
        assert!(!result.sign());
    }

    #[test]
    fn test_div_precision_loss() {
        // 1 / 3 is not exact -> PE set.
        let mut ef = no_exceptions();
        let _result = Fp80::ONE.div(
            THREE,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert!(ef.precision);
    }

    #[test]
    fn test_div_positive_over_neg_zero() {
        // positive / -0 = -inf + ZE (sign = XOR).
        let mut ef = no_exceptions();
        let result = Fp80::ONE.div(
            Fp80::NEG_ZERO,
            RoundingMode::NearestEven,
            Precision::Extended,
            &mut ef,
        );
        assert_eq!(result, Fp80::NEG_INFINITY);
        assert!(ef.zero_divide);
    }

    #[test]
    fn test_sqrt_precision_loss() {
        // sqrt(2) is irrational -> PE set.
        let mut ef = no_exceptions();
        let _result = TWO.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert!(ef.precision);
    }

    #[test]
    fn test_sqrt_denormal_sets_de() {
        // sqrt(denormal) sets DE flag.
        let mut ef = no_exceptions();
        let _result =
            POSITIVE_DENORMAL.sqrt(RoundingMode::NearestEven, Precision::Extended, &mut ef);
        assert!(ef.denormal);
    }
}
