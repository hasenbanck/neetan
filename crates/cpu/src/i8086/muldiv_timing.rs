//! Timing helpers for 8086 multiply and divide instructions.
//!
//! The microcode-derived timing model in this file was heavily influenced by
//! the 8088 core implementation in MartyPC, which is licensed under the
//! permissive MIT license.

use super::I8086;

pub(super) enum DivisionTiming {
    Complete { cycles: i32, carry: bool },
    Exception(i32),
}

impl DivisionTiming {
    pub(super) fn cycles(&self) -> i32 {
        match self {
            Self::Complete { cycles, .. } | Self::Exception(cycles) => *cycles,
        }
    }
}

fn neg(value: u16, bits: u32) -> (u16, bool) {
    let mask = if bits == 8 { 0x00FF } else { 0xFFFF };
    let value = value & mask;
    ((!value).wrapping_add(1) & mask, value != 0)
}

fn add(value: u16, other: u16, bits: u32) -> (u16, bool) {
    let mask = if bits == 8 { 0x00FF } else { 0xFFFF };
    let value = value & mask;
    let other = other & mask;
    let sum = value as u32 + other as u32;
    ((sum as u16) & mask, sum > mask as u32)
}

fn sub(value: u16, other: u16, bits: u32) -> (u16, bool) {
    let mask = if bits == 8 { 0x00FF } else { 0xFFFF };
    let value = value & mask;
    let other = other & mask;
    let (result, borrow) = value.overflowing_sub(other);
    (result & mask, borrow)
}

fn rcl(value: u16, carry: bool, bits: u32) -> (u16, bool) {
    let mask = if bits == 8 { 0x00FF } else { 0xFFFF };
    let carry_out = value & (1 << (bits - 1)) != 0;
    let result = ((value << 1) | u16::from(carry)) & mask;
    (result, carry_out)
}

fn rcr(value: u16, carry: bool, bits: u32) -> (u16, bool) {
    let mask = if bits == 8 { 0x00FF } else { 0xFFFF };
    let carry_out = value & 1 != 0;
    let result = (((u16::from(carry)) << (bits - 1)) | (value >> 1)) & mask;
    (result, carry_out)
}

fn sign_bit(value: u16, bits: u32) -> bool {
    value & (1 << (bits - 1)) != 0
}

fn cor_negate_timing(
    bits: u32,
    mut tmpa: u16,
    mut tmpb: u16,
    mut tmpc: u16,
    mut negate_flag: bool,
    skip: bool,
) -> (u16, u16, u16, bool, bool, i32) {
    let mut cycles = 0;

    if !skip {
        // 0x1b6: SIGMA->tmpc | NEG tmpc
        let (sigma, carry) = neg(tmpc, bits);
        tmpc = sigma;

        if carry {
            // 0x1b6, 0x1b7, 0x1b8, MC_JUMP, 0x1ba
            let mask = if bits == 8 { 0x00FF } else { 0xFFFF };
            tmpa = (!tmpa) & mask;
            cycles += 5;
        } else {
            // 0x1b6, 0x1b7, 0x1b8, 0x1b9, 0x1ba
            tmpa = neg(tmpa, bits).0;
            cycles += 5;
        }
        negate_flag = !negate_flag;
    }

    // 0x1bb: LRCY tmpb | 0x1bc: SIGMA->tmpb NEG tmpb | 0x1bd: NCY 11
    let carry = sign_bit(tmpb, bits);
    let (sigma, _) = neg(tmpb, bits);
    cycles += 3;

    if !carry {
        // MC_JUMP, 0x1bf, MC_RTN
        cycles += 3;
    } else {
        // 0x1be, MC_RTN
        tmpb = sigma;
        negate_flag = !negate_flag;
        cycles += 2;
    }

    (tmpa, tmpb, tmpc, carry, negate_flag, cycles)
}

fn corx_timing(bits: u32, tmpb: u16, mut tmpc: u16, mut carry: bool) -> (u16, u16, i32) {
    let mut cycles = 2;
    let (sigma, next_carry) = rcr(tmpc, carry, bits);
    let mut tmpa = 0;
    tmpc = sigma;
    carry = next_carry;

    let mut internal_counter = bits - 1;
    loop {
        cycles += 1;
        if carry {
            let (sigma, next_carry) = add(tmpa, tmpb, bits);
            tmpa = sigma;
            carry = next_carry;
            cycles += 2;
        } else {
            cycles += 1;
        }

        let (sigma, next_carry) = rcr(tmpa, carry, bits);
        tmpa = sigma;
        let (sigma, next_carry_2) = rcr(tmpc, next_carry, bits);
        tmpc = sigma;
        carry = next_carry_2;
        cycles += 3;

        if internal_counter == 0 {
            break;
        }

        internal_counter -= 1;
        cycles += 1;
    }

    cycles += 2;
    (tmpa, tmpc, cycles)
}

fn pre_idiv_timing(
    bits: u32,
    tmpa: u16,
    tmpb: u16,
    tmpc: u16,
    negate_flag: bool,
) -> (u16, u16, u16, bool, i32) {
    // 0x1b4: SIGMA->. (rcl dividend MSB) | 0x1b5: NCY 7
    let mut cycles = 2;
    let carry = sign_bit(tmpa, bits);
    if !carry {
        // Positive dividend: MC_JUMP into NEGATE @ line 7 (skip=true)
        let (tmpa, tmpb, tmpc, _, negate_flag, extra) =
            cor_negate_timing(bits, tmpa, tmpb, tmpc, negate_flag, true);
        cycles += 1 + extra;
        (tmpa, tmpb, tmpc, negate_flag, cycles)
    } else {
        // Negative dividend: fall into NEGATE (skip=false)
        let (tmpa, tmpb, tmpc, _, negate_flag, extra) =
            cor_negate_timing(bits, tmpa, tmpb, tmpc, negate_flag, false);
        cycles += extra;
        (tmpa, tmpb, tmpc, negate_flag, cycles)
    }
}

fn post_idiv_timing(
    bits: u32,
    mut tmpa: u16,
    tmpb: u16,
    tmpc: u16,
    carry: bool,
    negate_flag: bool,
) -> Result<((u16, u16), i32), i32> {
    // 0x1c4: NCY INT0
    let mut cycles = 1;
    if !carry {
        // Division exception (CORD flagged it via carry=false): MC_JUMP to INT0
        return Err(cycles + 1);
    }

    // 0x1c5: LRCY tmpb | 0x1c6: SIGMA->. NEG tmpa | 0x1c7: NCY 5
    let tmpb_sign = sign_bit(tmpb, bits);
    let neg_tmpa = neg(tmpa, bits).0;
    cycles += 3;
    if !tmpb_sign {
        // Divisor positive: MC_JUMP to 5
        cycles += 1;
    } else {
        // Divisor negative: 0x1c8 SIGMA->tmpa (negate remainder)
        tmpa = neg_tmpa;
        cycles += 1;
    }

    // 0x1c9: INC tmpc | 0x1ca: F1 8
    let mut sigma = tmpc.wrapping_add(1);
    cycles += 2;
    if !negate_flag {
        // 0x1cb: COM tmpc (ones-complement)
        sigma = !tmpc;
        cycles += 1;
    } else {
        // MC_JUMP to 0x1cc
        cycles += 1;
    }

    // 0x1cc: CCOF | MC_RTN
    cycles += 2;
    Ok(((tmpa, sigma), cycles))
}

fn cord_timing(bits: u32, mut tmpa: u16, tmpb: u16, mut tmpc: u16) -> DivisionTiming {
    // 0x188: SUBT tmpa | 0x189: MAXC | 0x18a: NCY INT0
    let mut cycles = 3;
    let (_, mut carry) = sub(tmpa, tmpb, bits);
    if !carry {
        // Divide overflow on first compare: MC_JUMP to INT0
        return DivisionTiming::Exception(cycles + 1);
    }

    let mut internal_counter = bits;
    while internal_counter > 0 {
        // 0x18b, 0x18c: RCLY tmpc, 0x18d: RCLY tmpa, 0x18e: NCY 8
        let (sigma, next_carry) = rcl(tmpc, carry, bits);
        tmpc = sigma;
        let (sigma, next_carry_2) = rcl(tmpa, next_carry, bits);
        tmpa = sigma;
        carry = next_carry_2;
        cycles += 4;

        if carry {
            // Path A (MSB of tmpa was set): divisor fits unconditionally.
            // MC_JUMP, 0x195, 0x196: commit subtraction
            cycles += 3;
            carry = false;
            tmpa = sub(tmpa, tmpb, bits).0;
            internal_counter -= 1;
            if internal_counter > 0 {
                // MC_JUMP back to 0x18b
                cycles += 1;
                continue;
            }
            // Final iteration: 0x197, MC_JUMP to 0x192 trailer
            cycles += 2;
        } else {
            // 0x18f: SUBT tmpa (trial subtract) | 0x190: NCY 14
            let (sigma, next_carry) = sub(tmpa, tmpb, bits);
            carry = next_carry;
            cycles += 2;
            if !carry {
                // Path B1 (trial subtract succeeded): commit.
                // MC_JUMP, 0x196
                tmpa = sigma;
                cycles += 2;
                internal_counter -= 1;
                if internal_counter > 0 {
                    // MC_JUMP back to 0x18b
                    cycles += 1;
                    continue;
                }
                // Final iteration: 0x197, MC_JUMP to 0x192 trailer
                cycles += 2;
            } else {
                // Path B2 (trial failed): do not commit.
                // 0x191
                cycles += 1;
                internal_counter -= 1;
                if internal_counter > 0 {
                    // MC_JUMP back to 0x18b
                    cycles += 1;
                    continue;
                }
                // Final iteration: fall straight into 0x192 trailer (no extra)
            }
        }
    }

    // 0x192: RCLY tmpc | 0x193 | 0x194: CCOF | MC_RTN
    let (sigma, next_carry) = rcl(tmpc, carry, bits);
    tmpc = sigma;
    let (_, carry) = rcl(tmpc, next_carry, bits);
    DivisionTiming::Complete {
        cycles: cycles + 4,
        carry,
    }
}

impl I8086 {
    pub(super) fn mul8_timing(
        &self,
        al: u8,
        operand: u8,
        signed: bool,
        mut negate_flag: bool,
    ) -> i32 {
        let mut cycles = 2;
        let mut tmpc = al as u16;
        let mut carry = sign_bit(tmpc, 8);
        let mut tmpb = operand as u16;

        if signed {
            let (sigma, _) = neg(tmpc, 8);
            cycles += 3;
            if carry {
                tmpc = sigma;
                negate_flag = !negate_flag;
                cycles += 3;
            } else {
                cycles += 1;
            }

            let (_, new_tmpb, new_tmpc, new_carry, new_negate_flag, extra) =
                cor_negate_timing(8, 0, tmpb, tmpc, negate_flag, true);
            tmpb = new_tmpb;
            tmpc = new_tmpc;
            carry = new_carry;
            negate_flag = new_negate_flag;
            cycles += extra;
        }

        cycles += 2;
        let (mut tmpa, mut tmpc, extra) = corx_timing(8, tmpb, tmpc, carry);
        cycles += extra;
        cycles += 1;

        if negate_flag {
            let (new_tmpa, _, new_tmpc, _, _, extra) =
                cor_negate_timing(8, tmpa, tmpb, tmpc, negate_flag, false);
            tmpa = new_tmpa;
            tmpc = new_tmpc;
            cycles += 1 + extra;
        }

        cycles += 1;
        if signed {
            cycles += 1;
            let carry = sign_bit(tmpc, 8);
            let (sigma, _) = add(tmpa, 0, 8);
            let sigma = sigma.wrapping_add(u16::from(carry)) & 0x00FF;
            cycles += 3;
            if sigma == 0 {
                cycles += 4;
            } else {
                cycles += 3;
            }
            cycles += 2;
        } else {
            cycles += 6;
            if tmpa == 0 {
                cycles += 4;
            } else {
                cycles += 3;
            }
        }

        cycles
    }

    pub(super) fn mul16_timing(
        &self,
        ax: u16,
        operand: u16,
        signed: bool,
        mut negate_flag: bool,
    ) -> i32 {
        let mut cycles = 2;
        let mut tmpc = ax;
        let mut carry = sign_bit(tmpc, 16);
        let mut tmpb = operand;

        if signed {
            let (sigma, _) = neg(tmpc, 16);
            cycles += 3;
            if carry {
                tmpc = sigma;
                negate_flag = !negate_flag;
                cycles += 3;
            } else {
                cycles += 1;
            }

            let (_, new_tmpb, new_tmpc, new_carry, new_negate_flag, extra) =
                cor_negate_timing(16, 0, tmpb, tmpc, negate_flag, true);
            tmpb = new_tmpb;
            tmpc = new_tmpc;
            carry = new_carry;
            negate_flag = new_negate_flag;
            cycles += extra;
        }

        cycles += 2;
        let (mut tmpa, mut tmpc, extra) = corx_timing(16, tmpb, tmpc, carry);
        cycles += extra;
        cycles += 1;

        if negate_flag {
            let (new_tmpa, _, new_tmpc, _, _, extra) =
                cor_negate_timing(16, tmpa, tmpb, tmpc, negate_flag, false);
            tmpa = new_tmpa;
            tmpc = new_tmpc;
            cycles += 1 + extra;
        }

        cycles += 1;
        if signed {
            cycles += 1;
            let carry = sign_bit(tmpc, 16);
            let (sigma, _) = add(tmpa, 0, 16);
            let sigma = sigma.wrapping_add(u16::from(carry));
            cycles += 3;
            if sigma == 0 {
                cycles += 4;
            } else {
                cycles += 3;
            }
            cycles += 2;
        } else {
            cycles += 6;
            if tmpa == 0 {
                cycles += 4;
            } else {
                cycles += 3;
            }
        }

        cycles
    }

    pub(super) fn div8_timing(
        &self,
        dividend: u16,
        divisor: u8,
        signed: bool,
        mut negate_flag: bool,
    ) -> DivisionTiming {
        // 0x160: A->tmpc | 0x161: M->tmpb | 0x162: X0 PREIDIV
        let mut cycles = 3;
        let mut tmpa = dividend >> 8;
        let mut tmpc = dividend & 0x00FF;
        let mut tmpb = divisor as u16;

        if signed {
            // MC_JUMP into PREIDIV
            let (new_tmpa, new_tmpb, new_tmpc, new_negate_flag, extra) =
                pre_idiv_timing(8, tmpa, tmpb, tmpc, negate_flag);
            tmpa = new_tmpa;
            tmpb = new_tmpb;
            tmpc = new_tmpc;
            negate_flag = new_negate_flag;
            cycles += 1 + extra;
        }

        // 0x163: UNC CORD | MC_JUMP
        cycles += 2;
        match cord_timing(8, tmpa, tmpb, tmpc) {
            // CORD signalled divide overflow already: no post-CORD cycles.
            DivisionTiming::Exception(extra) => DivisionTiming::Exception(cycles + extra),
            DivisionTiming::Complete {
                cycles: extra,
                carry,
            } => {
                cycles += extra;
                // 0x164: COM1 tmpc | 0x165: X->tmpb X0 POSTDIV
                cycles += 2;
                if signed {
                    // MC_JUMP into POSTIDIV
                    cycles += 1;
                    match post_idiv_timing(8, tmpa, dividend >> 8, tmpc, carry, negate_flag) {
                        Ok((_, extra)) => DivisionTiming::Complete {
                            cycles: cycles + extra,
                            carry,
                        },
                        Err(extra) => DivisionTiming::Exception(cycles + extra),
                    }
                } else {
                    DivisionTiming::Complete { cycles, carry }
                }
            }
        }
    }

    pub(super) fn div16_timing(
        &self,
        dividend: u32,
        divisor: u16,
        signed: bool,
        mut negate_flag: bool,
    ) -> DivisionTiming {
        // 0x168: DE->tmpa | 0x169: A->tmpc | 0x16a: M->tmpb X0 PREIDIV
        let mut cycles = 3;
        let mut tmpa = (dividend >> 16) as u16;
        let mut tmpc = dividend as u16;
        let mut tmpb = divisor;

        if signed {
            // MC_JUMP into PREIDIV
            let (new_tmpa, new_tmpb, new_tmpc, new_negate_flag, extra) =
                pre_idiv_timing(16, tmpa, tmpb, tmpc, negate_flag);
            tmpa = new_tmpa;
            tmpb = new_tmpb;
            tmpc = new_tmpc;
            negate_flag = new_negate_flag;
            cycles += 1 + extra;
        }

        // 0x16b: UNC CORD | MC_JUMP
        cycles += 2;
        match cord_timing(16, tmpa, tmpb, tmpc) {
            // CORD signalled divide overflow already: no post-CORD cycles.
            DivisionTiming::Exception(extra) => DivisionTiming::Exception(cycles + extra),
            DivisionTiming::Complete {
                cycles: extra,
                carry,
            } => {
                cycles += extra;
                // 0x16c: COM1 tmpc | 0x16d: DE->tmpb X0 POSTDIV
                cycles += 2;
                if signed {
                    // MC_JUMP into POSTIDIV
                    cycles += 1;
                    match post_idiv_timing(
                        16,
                        tmpa,
                        (dividend >> 16) as u16,
                        tmpc,
                        carry,
                        negate_flag,
                    ) {
                        Ok((_, extra)) => DivisionTiming::Complete {
                            cycles: cycles + extra,
                            carry,
                        },
                        Err(extra) => DivisionTiming::Exception(cycles + extra),
                    }
                } else {
                    DivisionTiming::Complete { cycles, carry }
                }
            }
        }
    }
}
