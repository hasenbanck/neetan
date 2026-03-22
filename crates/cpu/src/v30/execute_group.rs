use super::V30;
use crate::{ByteReg, SegReg16, WordReg};

impl V30 {
    /// Group 0x80: ALU r/m8, imm8
    pub(super) fn group_80(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let src = self.fetch(bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_add_byte(dst, src),
            1 => self.alu_or_byte(dst, src),
            2 => {
                let cf = self.flags.cf_val();
                self.alu_adc_byte(dst, src, cf)
            }
            3 => {
                let cf = self.flags.cf_val();
                self.alu_sbb_byte(dst, src, cf)
            }
            4 => self.alu_and_byte(dst, src),
            5 => self.alu_sub_byte(dst, src),
            6 => self.alu_xor_byte(dst, src),
            7 => {
                self.alu_sub_byte(dst, src);
                self.clk_modrm(modrm, 3, 7);
                return;
            }
            _ => unreachable!(),
        };
        if (modrm >> 3) & 7 != 7 {
            self.putback_rm_byte(modrm, result, bus);
        }
        self.clk_modrm(modrm, 3, 7);
    }

    /// Group 0x81: ALU r/m16, imm16
    pub(super) fn group_81(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word(modrm, bus);
        let src = self.fetchword(bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_add_word(dst, src),
            1 => self.alu_or_word(dst, src),
            2 => {
                let cf = self.flags.cf_val();
                self.alu_adc_word(dst, src, cf)
            }
            3 => {
                let cf = self.flags.cf_val();
                self.alu_sbb_word(dst, src, cf)
            }
            4 => self.alu_and_word(dst, src),
            5 => self.alu_sub_word(dst, src),
            6 => self.alu_xor_word(dst, src),
            7 => {
                self.alu_sub_word(dst, src);
                self.clk_modrm_word(modrm, 3, 7, 1);
                return;
            }
            _ => unreachable!(),
        };
        if (modrm >> 3) & 7 != 7 {
            self.putback_rm_word(modrm, result, bus);
        }
        self.clk_modrm_word(modrm, 3, 7, 2);
    }

    /// Group 0x82: ALU r/m8, imm8 (same as 0x80)
    pub(super) fn group_82(&mut self, bus: &mut impl common::Bus) {
        self.group_80(bus);
    }

    /// Group 0x83: ALU r/m16, sign-extended imm8
    pub(super) fn group_83(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word(modrm, bus);
        let src = self.fetch(bus) as i8 as u16;
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_add_word(dst, src),
            1 => self.alu_or_word(dst, src),
            2 => {
                let cf = self.flags.cf_val();
                self.alu_adc_word(dst, src, cf)
            }
            3 => {
                let cf = self.flags.cf_val();
                self.alu_sbb_word(dst, src, cf)
            }
            4 => self.alu_and_word(dst, src),
            5 => self.alu_sub_word(dst, src),
            6 => self.alu_xor_word(dst, src),
            7 => {
                self.alu_sub_word(dst, src);
                self.clk_modrm_word(modrm, 3, 7, 1);
                return;
            }
            _ => unreachable!(),
        };
        if (modrm >> 3) & 7 != 7 {
            self.putback_rm_word(modrm, result, bus);
        }
        self.clk_modrm_word(modrm, 3, 7, 2);
    }

    /// Group 0xC0: shift/rotate r/m8, imm8
    pub(super) fn group_c0(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let count = self.fetch(bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, count),
            1 => self.alu_ror_byte(dst, count),
            2 => self.alu_rcl_byte(dst, count),
            3 => self.alu_rcr_byte(dst, count),
            4 => self.alu_shl_byte(dst, count),
            5 => self.alu_shr_byte(dst, count),
            6 => self.alu_shl_byte(dst, count), // undocumented: same as SHL
            7 => self.alu_sar_byte(dst, count),
            _ => unreachable!(),
        };
        self.putback_rm_byte(modrm, result, bus);
        let n = count as i32;
        self.clk_modrm(modrm, 5 + n, 8 + n);
    }

    /// Group 0xC1: shift/rotate r/m16, imm8
    pub(super) fn group_c1(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word(modrm, bus);
        let count = self.fetch(bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, count),
            1 => self.alu_ror_word(dst, count),
            2 => self.alu_rcl_word(dst, count),
            3 => self.alu_rcr_word(dst, count),
            4 => self.alu_shl_word(dst, count),
            5 => self.alu_shr_word(dst, count),
            6 => self.alu_shl_word(dst, count),
            7 => self.alu_sar_word(dst, count),
            _ => unreachable!(),
        };
        self.putback_rm_word(modrm, result, bus);
        let n = count as i32;
        self.clk_modrm_word(modrm, 5 + n, 8 + n, 2);
    }

    /// Group 0xD0: shift/rotate r/m8, 1
    pub(super) fn group_d0(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, 1),
            1 => self.alu_ror_byte(dst, 1),
            2 => self.alu_rcl_byte(dst, 1),
            3 => self.alu_rcr_byte(dst, 1),
            4 => self.alu_shl_byte(dst, 1),
            5 => self.alu_shr_byte(dst, 1),
            6 => self.alu_shl_byte(dst, 1),
            7 => self.alu_sar_byte(dst, 1),
            _ => unreachable!(),
        };
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    /// Group 0xD1: shift/rotate r/m16, 1
    pub(super) fn group_d1(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word(modrm, bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, 1),
            1 => self.alu_ror_word(dst, 1),
            2 => self.alu_rcl_word(dst, 1),
            3 => self.alu_rcr_word(dst, 1),
            4 => self.alu_shl_word(dst, 1),
            5 => self.alu_shr_word(dst, 1),
            6 => self.alu_shl_word(dst, 1),
            7 => self.alu_sar_word(dst, 1),
            _ => unreachable!(),
        };
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    /// Group 0xD2: shift/rotate r/m8, CL
    pub(super) fn group_d2(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let count = self.regs.byte(ByteReg::CL);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, count),
            1 => self.alu_ror_byte(dst, count),
            2 => self.alu_rcl_byte(dst, count),
            3 => self.alu_rcr_byte(dst, count),
            4 => self.alu_shl_byte(dst, count),
            5 => self.alu_shr_byte(dst, count),
            6 => self.alu_shl_byte(dst, count),
            7 => self.alu_sar_byte(dst, count),
            _ => unreachable!(),
        };
        self.putback_rm_byte(modrm, result, bus);
        let n = count as i32;
        self.clk_modrm(modrm, 5 + n, 8 + n);
    }

    /// Group 0xD3: shift/rotate r/m16, CL
    pub(super) fn group_d3(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word(modrm, bus);
        let count = self.regs.byte(ByteReg::CL);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, count),
            1 => self.alu_ror_word(dst, count),
            2 => self.alu_rcl_word(dst, count),
            3 => self.alu_rcr_word(dst, count),
            4 => self.alu_shl_word(dst, count),
            5 => self.alu_shr_word(dst, count),
            6 => self.alu_shl_word(dst, count),
            7 => self.alu_sar_word(dst, count),
            _ => unreachable!(),
        };
        self.putback_rm_word(modrm, result, bus);
        let n = count as i32;
        self.clk_modrm_word(modrm, 5 + n, 8 + n, 2);
    }

    /// Group 0xF6: various byte operations
    pub(super) fn group_f6(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let op = (modrm >> 3) & 7;
        match op {
            0 | 1 => {
                // TEST r/m8, imm8
                let dst = self.get_rm_byte(modrm, bus);
                let src = self.fetch(bus);
                self.alu_and_byte(dst, src);
                self.clk_modrm(modrm, 2, 6);
            }
            2 => {
                // NOT r/m8
                let dst = self.get_rm_byte(modrm, bus);
                self.putback_rm_byte(modrm, !dst, bus);
                self.clk_modrm(modrm, 2, 7);
            }
            3 => {
                // NEG r/m8
                let dst = self.get_rm_byte(modrm, bus);
                let result = self.alu_neg_byte(dst);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_modrm(modrm, 2, 7);
            }
            4 => {
                // MUL r/m8 (unsigned, NEC MULU)
                let src = self.get_rm_byte(modrm, bus);
                let al = self.regs.byte(ByteReg::AL);
                let result = al as u16 * src as u16;
                self.regs.set_word(WordReg::AX, result);
                self.flags.carry_val = if result & 0xFF00 != 0 { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm(modrm, 13, 16);
            }
            5 => {
                // IMUL r/m8 (signed, NEC MUL)
                let src = self.get_rm_byte(modrm, bus) as i8 as i16;
                let al = self.regs.byte(ByteReg::AL) as i8 as i16;
                let result = al * src;
                self.regs.set_word(WordReg::AX, result as u16);
                let ah = (result >> 8) as i8;
                let al_sign = result as i8;
                self.flags.carry_val = if ah != (al_sign >> 7) { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm(modrm, 13, 16);
            }
            6 => {
                // DIV r/m8 (unsigned, NEC DIVU)
                let src = self.get_rm_byte(modrm, bus) as u16;
                if src == 0 {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let aw = self.regs.word(WordReg::AX);
                let quotient = aw / src;
                if quotient > 0xFF {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let remainder = aw % src;
                self.regs.set_byte(ByteReg::AL, quotient as u8);
                self.regs.set_byte(ByteReg::AH, remainder as u8);
                self.clk_modrm(modrm, 14, 17);
            }
            7 => {
                // IDIV r/m8 (signed, NEC DIV)
                let src = self.get_rm_byte(modrm, bus) as i8 as i16;
                if src == 0 {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let aw = self.regs.word(WordReg::AX) as i16;
                let Some(quotient) = aw.checked_div(src) else {
                    self.raise_interrupt(0, bus);
                    return;
                };
                if !(-127..=127).contains(&quotient) {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let remainder = aw.checked_rem(src).unwrap_or(0);
                self.regs.set_byte(ByteReg::AL, quotient as u8);
                self.regs.set_byte(ByteReg::AH, remainder as u8);
                self.clk_modrm(modrm, 17, 20);
            }
            _ => unreachable!(),
        }
    }

    /// Group 0xF7: various word operations
    pub(super) fn group_f7(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let op = (modrm >> 3) & 7;
        match op {
            0 | 1 => {
                // TEST r/m16, imm16
                let dst = self.get_rm_word(modrm, bus);
                let src = self.fetchword(bus);
                self.alu_and_word(dst, src);
                self.clk_modrm_word(modrm, 2, 6, 1);
            }
            2 => {
                // NOT r/m16
                let dst = self.get_rm_word(modrm, bus);
                self.putback_rm_word(modrm, !dst, bus);
                self.clk_modrm_word(modrm, 2, 7, 2);
            }
            3 => {
                // NEG r/m16
                let dst = self.get_rm_word(modrm, bus);
                let result = self.alu_neg_word(dst);
                self.putback_rm_word(modrm, result, bus);
                self.clk_modrm_word(modrm, 2, 7, 2);
            }
            4 => {
                // MUL r/m16 (unsigned, NEC MULU)
                let src = self.get_rm_word(modrm, bus);
                let aw = self.regs.word(WordReg::AX);
                let result = aw as u32 * src as u32;
                self.regs.set_word(WordReg::AX, result as u16);
                self.regs.set_word(WordReg::DX, (result >> 16) as u16);
                self.flags.carry_val = if result & 0xFFFF0000 != 0 { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm_word(modrm, 21, 24, 1);
            }
            5 => {
                // IMUL r/m16 (signed, NEC MUL)
                let src = self.get_rm_word(modrm, bus) as i16 as i32;
                let aw = self.regs.word(WordReg::AX) as i16 as i32;
                let result = aw * src;
                self.regs.set_word(WordReg::AX, result as u16);
                self.regs.set_word(WordReg::DX, (result >> 16) as u16);
                let upper = (result >> 16) as i16;
                let lower_sign = result as i16;
                self.flags.carry_val = if upper != (lower_sign >> 15) { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm_word(modrm, 21, 24, 1);
            }
            6 => {
                // DIV r/m16 (unsigned, NEC DIVU)
                let src = self.get_rm_word(modrm, bus) as u32;
                if src == 0 {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let dw = self.regs.word(WordReg::DX) as u32;
                let aw = self.regs.word(WordReg::AX) as u32;
                let dividend = (dw << 16) | aw;
                let quotient = dividend / src;
                if quotient > 0xFFFF {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let remainder = dividend % src;
                self.regs.set_word(WordReg::AX, quotient as u16);
                self.regs.set_word(WordReg::DX, remainder as u16);
                self.clk_modrm_word(modrm, 22, 25, 1);
            }
            7 => {
                // IDIV r/m16 (signed, NEC DIV)
                let src = self.get_rm_word(modrm, bus) as i16 as i32;
                if src == 0 {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let dw = self.regs.word(WordReg::DX) as u32;
                let aw = self.regs.word(WordReg::AX) as u32;
                let dividend = ((dw << 16) | aw) as i32;
                let Some(quotient) = dividend.checked_div(src) else {
                    self.raise_interrupt(0, bus);
                    return;
                };
                if !(-32767..=32767).contains(&quotient) {
                    self.raise_interrupt(0, bus);
                    return;
                }
                let remainder = dividend.checked_rem(src).unwrap_or(0);
                self.regs.set_word(WordReg::AX, quotient as u16);
                self.regs.set_word(WordReg::DX, remainder as u16);
                self.clk_modrm_word(modrm, 25, 28, 1);
            }
            _ => unreachable!(),
        }
    }

    /// Group 0xFE: INC/DEC r/m8
    pub(super) fn group_fe(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // INC r/m8
                let dst = self.get_rm_byte(modrm, bus);
                let result = self.alu_inc_byte(dst);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_modrm(modrm, 2, 7);
            }
            1 => {
                // DEC r/m8
                let dst = self.get_rm_byte(modrm, bus);
                let result = self.alu_dec_byte(dst);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_modrm(modrm, 2, 7);
            }
            _ => {
                self.clk(2);
            }
        }
    }

    /// Group 0xFF: various word operations
    pub(super) fn group_ff(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // INC r/m16
                let dst = self.get_rm_word(modrm, bus);
                let result = self.alu_inc_word(dst);
                self.putback_rm_word(modrm, result, bus);
                self.clk_modrm_word(modrm, 2, 7, 2);
            }
            1 => {
                // DEC r/m16
                let dst = self.get_rm_word(modrm, bus);
                let result = self.alu_dec_word(dst);
                self.putback_rm_word(modrm, result, bus);
                self.clk_modrm_word(modrm, 2, 7, 2);
            }
            2 => {
                // CALL r/m16 (near indirect)
                let sp_pen = self.sp_penalty(1);
                let dst = self.get_rm_word(modrm, bus);
                self.push(bus, self.ip);
                self.ip = dst;
                if modrm >= 0xC0 {
                    self.clk(7 + sp_pen);
                } else {
                    let ea_pen = if self.ea & 1 == 1 { 4 } else { 0 };
                    self.clk(11 + sp_pen + ea_pen);
                }
            }
            3 => {
                // CALL m16:16 (far indirect)
                if modrm >= 0xC0 {
                    return;
                }
                let sp_pen = self.sp_penalty(2);
                self.calc_ea(modrm, bus);
                let offset = self.seg_read_word(bus);
                let segment = self.seg_read_word_at(bus, 2);
                let cs = self.sregs[SegReg16::CS as usize];
                self.push(bus, cs);
                self.push(bus, self.ip);
                self.ip = offset;
                self.sregs[SegReg16::CS as usize] = segment;
                let ea_pen = if self.ea & 1 == 1 { 8 } else { 0 };
                self.clk(16 + sp_pen + ea_pen);
            }
            4 => {
                // JMP r/m16 (near indirect)
                let dst = self.get_rm_word(modrm, bus);
                self.ip = dst;
                self.clk_modrm_word(modrm, 7, 11, 1);
            }
            5 => {
                // JMP m16:16 (far indirect)
                if modrm >= 0xC0 {
                    return;
                }
                self.calc_ea(modrm, bus);
                let offset = self.seg_read_word(bus);
                let segment = self.seg_read_word_at(bus, 2);
                self.ip = offset;
                self.sregs[SegReg16::CS as usize] = segment;
                let penalty = if self.ea & 1 == 1 { 8 } else { 0 };
                self.clk(11 + penalty);
            }
            6 | 7 => {
                // PUSH r/m16 (7 is undocumented alias)
                if modrm >= 0xC0 && (modrm & 7) == 4 {
                    self.push_sp(bus);
                } else {
                    let sp_pen = self.sp_penalty(1);
                    let val = self.get_rm_word(modrm, bus);
                    self.push(bus, val);
                    if modrm >= 0xC0 {
                        self.clk(3 + sp_pen);
                    } else {
                        let ea_pen = if self.ea & 1 == 1 { 4 } else { 0 };
                        self.clk(5 + sp_pen + ea_pen);
                    }
                }
            }
            _ => {
                self.clk(2);
            }
        }
    }
}
