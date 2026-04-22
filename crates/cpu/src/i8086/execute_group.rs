use super::{I8086, StepFinishCycle};
use crate::{ByteReg, SegReg16, WordReg};

impl I8086 {
    fn rm_default_segment(&self, modrm: u8) -> SegReg16 {
        let mode = modrm >> 6;
        let rm = modrm & 7;
        if mode == 0 && rm == 6 {
            SegReg16::DS
        } else if rm == 2 || rm == 3 || (rm == 6 && mode != 0) {
            SegReg16::SS
        } else {
            SegReg16::DS
        }
    }

    fn get_rm_widened_from_byte(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u16 {
        if modrm >= 0xC0 {
            match self.rm_byte(modrm) {
                ByteReg::AL => self.regs.word(WordReg::AX),
                ByteReg::CL => self.regs.word(WordReg::CX),
                ByteReg::DL => self.regs.word(WordReg::DX),
                ByteReg::BL => self.regs.word(WordReg::BX),
                ByteReg::AH => self.regs.word(WordReg::AX).swap_bytes(),
                ByteReg::CH => self.regs.word(WordReg::CX).swap_bytes(),
                ByteReg::DH => self.regs.word(WordReg::DX).swap_bytes(),
                ByteReg::BH => self.regs.word(WordReg::BX).swap_bytes(),
            }
        } else {
            self.calc_ea(modrm);
            let value = self.read_memory_byte(bus, self.ea);
            self.clk_eaload(bus);
            u16::from(value) | 0xFF00
        }
    }

    fn invalid_far_register_segment(&mut self, bus: &mut impl common::Bus) -> u16 {
        self.read_word_seg(bus, self.default_segment(SegReg16::DS), 0x0004)
    }

    fn invalid_far_register_segment_from_byte(&mut self, bus: &mut impl common::Bus) -> u16 {
        let address = self.seg_base(SegReg16::DS).wrapping_add(0x0004);
        u16::from(self.read_memory_byte(bus, address)) | 0xFF00
    }

    fn get_rm_far_ptr_from_byte_pair(
        &mut self,
        modrm: u8,
        bus: &mut impl common::Bus,
    ) -> (u16, u16) {
        debug_assert!(modrm < 0xC0);
        self.calc_ea(modrm);
        let offset = u16::from(self.read_memory_byte(bus, self.ea)) | 0xFF00;
        self.clk_eaload(bus);
        let segment_address = self
            .seg_base(self.rm_default_segment(modrm))
            .wrapping_add(self.eo as u32);
        let segment = u16::from(self.read_memory_byte(bus, segment_address)) | 0xFF00;
        (offset, segment)
    }

    fn invalid_far_register_ip_handoff(&self) -> u16 {
        if self.instruction_entry_queue_bytes == 0 {
            self.prev_ip
        } else {
            self.prev_ip.wrapping_sub(4)
        }
    }

    fn push_byte_value(&mut self, bus: &mut impl common::Bus, value: u8) {
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        self.regs.set_word(WordReg::SP, sp);
        let address = self.seg_base(SegReg16::SS).wrapping_add(sp as u32);
        self.write_memory_byte(bus, address, value);
    }

    fn nearcall_byte_routine(
        &mut self,
        bus: &mut impl common::Bus,
        new_ip: u16,
        early_fetch: bool,
    ) {
        let return_ip = self.ip;
        self.clk(bus, 1);
        self.ip = new_ip;
        if early_fetch {
            self.flush_and_fetch_early(bus);
        } else {
            self.flush_and_fetch(bus);
        }
        self.clk(bus, 3);
        self.push_byte_value(bus, return_ip as u8);
    }

    fn farcall_byte_routine(
        &mut self,
        bus: &mut impl common::Bus,
        new_cs: u16,
        new_ip: u16,
        jump: bool,
        early_fetch: bool,
    ) {
        if jump {
            self.clk(bus, 1);
        }
        self.biu_fetch_suspend(bus);
        self.clk(bus, 2);
        self.corr(bus);
        self.clk(bus, 1);
        let return_cs = self.sregs[SegReg16::CS as usize];
        self.push_byte_value(bus, return_cs as u8);
        self.sregs[SegReg16::CS as usize] = new_cs;
        self.clk(bus, 2);
        self.nearcall_byte_routine(bus, new_ip, early_fetch);
    }

    /// Group 0x80: ALU r/m8, imm8
    pub(super) fn group_80(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let src = self.fetch(bus);
        let op = (modrm >> 3) & 7;
        let result = match op {
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
                dst
            }
            _ => unreachable!(),
        };
        if op != 7 {
            if modrm < 0xC0 {
                self.clk(bus, 1);
            }
            self.putback_rm_byte(modrm, result, bus);
        }
        self.clk_modrm(bus, modrm, 0, 1);
    }

    /// Group 0x81: ALU r/m16, imm16
    pub(super) fn group_81(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        if modrm >= 0xC0 {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
        let dst = self.get_rm_word(modrm, bus);
        let src = self.fetchword(bus);
        let op = (modrm >> 3) & 7;
        let result = match op {
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
                dst
            }
            _ => unreachable!(),
        };
        if op != 7 {
            self.putback_rm_word(modrm, result, bus);
            if modrm < 0xC0 && self.next_instruction_uses_prefetched_high_byte() {
                self.finish_on_terminal_writeback_inline_commit(modrm);
            }
        }
        if op == 7 {
            self.clk_modrm_word(bus, modrm, 0, 0);
        } else {
            self.clk_modrm_word(bus, modrm, 0, 2);
        }
    }

    /// Group 0x82: ALU r/m8, imm8 (same as 0x80)
    pub(super) fn group_82(&mut self, bus: &mut impl common::Bus) {
        self.group_80(bus);
    }

    /// Group 0x83: ALU r/m16, sign-extended imm8
    pub(super) fn group_83(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let dst = self.get_rm_word(modrm, bus);
        let src = self.fetch(bus) as i8 as u16;
        let op = (modrm >> 3) & 7;
        let result = match op {
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
                dst
            }
            _ => unreachable!(),
        };
        if op != 7 {
            if modrm < 0xC0 {
                self.clk(bus, 1);
            }
            self.putback_rm_word(modrm, result, bus);
        }
        self.clk_modrm_word(bus, modrm, 0, 1);
    }

    /// Group 0xD0: shift/rotate r/m8, 1
    pub(super) fn group_d0(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, 1),
            1 => self.alu_ror_byte(dst, 1),
            2 => self.alu_rcl_byte(dst, 1),
            3 => self.alu_rcr_byte(dst, 1),
            4 => self.alu_shl_byte(dst, 1),
            5 => self.alu_shr_byte(dst, 1),
            6 => 0xFF,
            7 => self.alu_sar_byte(dst, 1),
            _ => unreachable!(),
        };
        self.putback_rm_byte(modrm, result, bus);
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        self.clk_modrm(bus, modrm, 0, 2);
    }

    /// Group 0xD1: shift/rotate r/m16, 1
    pub(super) fn group_d1(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let dst = self.get_rm_word(modrm, bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, 1),
            1 => self.alu_ror_word(dst, 1),
            2 => self.alu_rcl_word(dst, 1),
            3 => self.alu_rcr_word(dst, 1),
            4 => self.alu_shl_word(dst, 1),
            5 => self.alu_shr_word(dst, 1),
            6 => 0xFFFF,
            7 => self.alu_sar_word(dst, 1),
            _ => unreachable!(),
        };
        self.putback_rm_word(modrm, result, bus);
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        self.clk_modrm_word(bus, modrm, 0, 2);
    }

    /// Group 0xD2: shift/rotate r/m8, CL
    pub(super) fn group_d2(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let dst = self.get_rm_byte(modrm, bus);
        let count = self.regs.byte(ByteReg::CL);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, count),
            1 => self.alu_ror_byte(dst, count),
            2 => self.alu_rcl_byte(dst, count),
            3 => self.alu_rcr_byte(dst, count),
            4 => self.alu_shl_byte(dst, count),
            5 => self.alu_shr_byte(dst, count),
            6 => {
                if count == 0 {
                    dst
                } else {
                    0xFF
                }
            }
            7 => self.alu_sar_byte(dst, count),
            _ => unreachable!(),
        };
        self.putback_rm_byte(modrm, result, bus);
        let n = count as i32;
        self.clk_modrm(bus, modrm, 5 + 4 * n, 6 + 4 * n);
    }

    /// Group 0xD3: shift/rotate r/m16, CL
    pub(super) fn group_d3(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let dst = self.get_rm_word(modrm, bus);
        let count = self.regs.byte(ByteReg::CL);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, count),
            1 => self.alu_ror_word(dst, count),
            2 => self.alu_rcl_word(dst, count),
            3 => self.alu_rcr_word(dst, count),
            4 => self.alu_shl_word(dst, count),
            5 => self.alu_shr_word(dst, count),
            6 => {
                if count == 0 {
                    dst
                } else {
                    0xFFFF
                }
            }
            7 => self.alu_sar_word(dst, count),
            _ => unreachable!(),
        };
        self.putback_rm_word(modrm, result, bus);
        let n = count as i32;
        self.clk_modrm_word(bus, modrm, 5 + 4 * n, 6 + 4 * n);
    }

    /// Group 0xF6: various byte operations
    pub(super) fn group_f6(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let op = (modrm >> 3) & 7;
        match op {
            0 | 1 => {
                // TEST r/m8, imm8
                let dst = self.get_rm_byte(modrm, bus);
                let src = self.fetch(bus);
                self.alu_and_byte(dst, src);
                self.clk_modrm(bus, modrm, 1, 1);
            }
            2 => {
                // NOT r/m8
                let dst = self.get_rm_byte(modrm, bus);
                self.putback_rm_byte(modrm, !dst, bus);
                self.clk_modrm(bus, modrm, 0, 1);
            }
            3 => {
                // NEG r/m8
                let dst = self.get_rm_byte(modrm, bus);
                let result = self.alu_neg_byte(dst);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_modrm(bus, modrm, 0, 1);
            }
            4 => {
                // MUL r/m8 (unsigned, NEC MULU)
                let src = self.get_rm_byte(modrm, bus);
                let al = self.regs.byte(ByteReg::AL);
                let result = al as u16 * src as u16;
                self.regs.set_word(WordReg::AX, result);
                self.flags.carry_val = if result & 0xFF00 != 0 { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                let cycles = self.mul8_timing(al, src, false, false);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
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
                let cycles = self.mul8_timing(al as u8, src as u8, true, self.rep_prefix);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
            }
            6 => {
                // DIV r/m8 (unsigned, NEC DIVU)
                let src = self.get_rm_byte(modrm, bus) as u16;
                let timing = self.div8_timing(self.regs.word(WordReg::AX), src as u8, false, false);
                if src == 0 {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let aw = self.regs.word(WordReg::AX);
                let quotient = aw / src;
                if quotient > 0xFF {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let remainder = aw % src;
                self.regs.set_byte(ByteReg::AL, quotient as u8);
                self.regs.set_byte(ByteReg::AH, remainder as u8);
                let cycles = timing.cycles();
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
            }
            7 => {
                // IDIV r/m8 (signed, NEC DIV)
                let src = self.get_rm_byte(modrm, bus) as i8 as i16;
                let timing = self.div8_timing(
                    self.regs.word(WordReg::AX),
                    src as u8,
                    true,
                    self.rep_prefix,
                );
                if src == 0 {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let aw = self.regs.word(WordReg::AX) as i16;
                let Some(quotient) = aw.checked_div(src) else {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                };
                let quotient = if self.rep_prefix {
                    quotient.wrapping_neg()
                } else {
                    quotient
                };
                if !(-127..=127).contains(&quotient) {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let remainder = aw.checked_rem(src).unwrap_or(0);
                self.regs.set_byte(ByteReg::AL, quotient as u8);
                self.regs.set_byte(ByteReg::AH, remainder as u8);
                let cycles = timing.cycles();
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
            }
            _ => unreachable!(),
        }
    }

    /// Group 0xF7: various word operations
    pub(super) fn group_f7(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let op = (modrm >> 3) & 7;
        match op {
            0 | 1 => {
                // TEST r/m16, imm16
                let dst = self.get_rm_word(modrm, bus);
                let src = self.fetchword(bus);
                self.alu_and_word(dst, src);
                if modrm >= 0xC0 {
                    self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                }
                self.clk_modrm_word(bus, modrm, 1, 0);
            }
            2 => {
                // NOT r/m16
                let dst = self.get_rm_word(modrm, bus);
                self.putback_rm_word(modrm, !dst, bus);
                self.clk_modrm_word(bus, modrm, 0, 1);
            }
            3 => {
                // NEG r/m16
                let dst = self.get_rm_word(modrm, bus);
                let result = self.alu_neg_word(dst);
                self.putback_rm_word(modrm, result, bus);
                self.clk_modrm_word(bus, modrm, 0, 1);
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
                let cycles = self.mul16_timing(aw, src, false, false);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
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
                let cycles = self.mul16_timing(aw as u16, src as u16, true, self.rep_prefix);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
            }
            6 => {
                // DIV r/m16 (unsigned, NEC DIVU)
                let src = self.get_rm_word(modrm, bus) as u32;
                let dividend = ((self.regs.word(WordReg::DX) as u32) << 16)
                    | self.regs.word(WordReg::AX) as u32;
                let timing = self.div16_timing(dividend, src as u16, false, false);
                if src == 0 {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let dw = self.regs.word(WordReg::DX) as u32;
                let aw = self.regs.word(WordReg::AX) as u32;
                let dividend = (dw << 16) | aw;
                let quotient = dividend / src;
                if quotient > 0xFFFF {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let remainder = dividend % src;
                self.regs.set_word(WordReg::AX, quotient as u16);
                self.regs.set_word(WordReg::DX, remainder as u16);
                let cycles = timing.cycles();
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
            }
            7 => {
                // IDIV r/m16 (signed, NEC DIV)
                let src = self.get_rm_word(modrm, bus) as i16 as i32;
                let dividend = ((self.regs.word(WordReg::DX) as u32) << 16)
                    | self.regs.word(WordReg::AX) as u32;
                let timing = self.div16_timing(dividend, src as u16, true, self.rep_prefix);
                if src == 0 {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let dw = self.regs.word(WordReg::DX) as u32;
                let aw = self.regs.word(WordReg::AX) as u32;
                let dividend = ((dw << 16) | aw) as i32;
                let Some(quotient) = dividend.checked_div(src) else {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                };
                let quotient = if self.rep_prefix {
                    quotient.wrapping_neg()
                } else {
                    quotient
                };
                if !(-32768..=32767).contains(&quotient) {
                    let cycles = timing.cycles();
                    self.clk_muldiv_modrm(bus, modrm, cycles);
                    self.raise_divide_error(bus);
                    return;
                }
                let remainder = dividend.checked_rem(src).unwrap_or(0);
                self.regs.set_word(WordReg::AX, quotient as u16);
                self.regs.set_word(WordReg::DX, remainder as u16);
                let cycles = timing.cycles();
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                self.clk_muldiv_modrm(bus, modrm, cycles);
            }
            _ => unreachable!(),
        }
    }

    /// Group 0xFE: INC/DEC r/m8
    pub(super) fn group_fe(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // INC r/m8
                let dst = self.get_rm_byte(modrm, bus);
                let result = self.alu_inc_byte(dst);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_modrm(bus, modrm, 0, 1);
            }
            1 => {
                // DEC r/m8
                let dst = self.get_rm_byte(modrm, bus);
                let result = self.alu_dec_byte(dst);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_modrm(bus, modrm, 0, 1);
            }
            2 => {
                // Undefined on paper, but executes as CALL with the 8-bit
                // operand widened.
                let dst = self.get_rm_widened_from_byte(modrm, bus);
                if modrm >= 0xC0 {
                    self.clk(bus, 1);
                }
                self.biu_fetch_suspend(bus);
                self.clk(bus, 2);
                self.corr(bus);
                self.nearcall_byte_routine(bus, dst, true);
            }
            3 => {
                // Undefined on paper, but executes as far CALL through an
                // invalid 8-bit form.
                let (offset, segment) = if modrm >= 0xC0 {
                    self.clk(bus, 1);
                    (
                        self.invalid_far_register_ip_handoff(),
                        self.invalid_far_register_segment_from_byte(bus),
                    )
                } else {
                    self.get_rm_far_ptr_from_byte_pair(modrm, bus)
                };
                self.farcall_byte_routine(bus, segment, offset, true, false);
            }
            4 => {
                // Undefined on paper, but executes as JMP with the 8-bit
                // operand widened.
                let dst = self.get_rm_widened_from_byte(modrm, bus);
                if modrm >= 0xC0 {
                    self.clk(bus, 1);
                }
                self.biu_fetch_suspend(bus);
                self.clk(bus, 1);
                self.ip = dst;
                self.flush_and_fetch(bus);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            }
            5 => {
                // Undefined on paper, but executes as far JMP through an
                // invalid 8-bit form.
                let (offset, segment) = if modrm >= 0xC0 {
                    self.clk(bus, 1);
                    (
                        self.invalid_far_register_ip_handoff(),
                        self.invalid_far_register_segment_from_byte(bus),
                    )
                } else {
                    self.get_rm_far_ptr_from_byte_pair(modrm, bus)
                };
                self.biu_fetch_suspend(bus);
                self.clk(bus, 1);
                self.sregs[SegReg16::CS as usize] = segment;
                self.ip = offset;
                self.flush_and_fetch(bus);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            }
            6 | 7 => {
                // Undefined on paper, but executes as PUSH byte.
                let value = self.get_rm_byte(modrm, bus);
                self.push_byte_value(bus, value);
                if modrm >= 0xC0 {
                    if self.seg_prefix || !self.instruction_entry_queue_full() {
                        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                    }
                    self.clk(bus, 4);
                } else {
                    self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                    self.clk(bus, 3);
                }
            }
            _ => {
                self.clk(bus, 2);
            }
        }
    }

    /// Group 0xFF: various word operations
    pub(super) fn group_ff(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // INC r/m16
                let dst = self.get_rm_word(modrm, bus);
                let result = self.alu_inc_word(dst);
                self.putback_rm_word(modrm, result, bus);
                self.clk_modrm_word(bus, modrm, 0, 1);
            }
            1 => {
                // DEC r/m16
                let dst = self.get_rm_word(modrm, bus);
                let result = self.alu_dec_word(dst);
                self.putback_rm_word(modrm, result, bus);
                self.clk_modrm_word(bus, modrm, 0, 1);
            }
            2 => {
                // CALL r/m16 (near indirect)
                let dst = self.get_rm_word(modrm, bus);
                if modrm >= 0xC0 {
                    self.clk(bus, 1);
                }
                self.biu_fetch_suspend(bus);
                self.clk(bus, 2);
                self.corr(bus);
                self.nearcall_routine(bus, dst, true);
            }
            3 => {
                // CALL m16:16 (far indirect)
                if modrm >= 0xC0 {
                    self.clk(bus, 1);
                    let segment = self.invalid_far_register_segment(bus);
                    let offset = self.invalid_far_register_ip_handoff();
                    self.farcall_routine(bus, segment, offset, true, false);
                    return;
                }
                let offset = self.get_rm_word(modrm, bus);
                let segment = self.seg_read_word_at(bus, 2);
                if self.far_indirect_transfer_uses_preloaded_handoff(modrm) {
                    self.clk(bus, 1);
                }
                self.farcall_routine(bus, segment, offset, true, false);
            }
            4 => {
                // JMP r/m16 (near indirect)
                let dst = self.get_rm_word(modrm, bus);
                if modrm >= 0xC0 {
                    self.clk(bus, 1);
                }
                self.biu_fetch_suspend(bus);
                self.clk(bus, 1);
                self.ip = dst;
                self.flush_and_fetch(bus);
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            }
            5 => {
                // JMP m16:16 (far indirect)
                if modrm >= 0xC0 {
                    self.clk(bus, 1);
                    let segment = self.invalid_far_register_segment(bus);
                    self.biu_fetch_suspend(bus);
                    self.clk(bus, 1);
                    self.sregs[SegReg16::CS as usize] = segment;
                    self.ip = self.invalid_far_register_ip_handoff();
                    self.flush_and_fetch(bus);
                    self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                    return;
                }
                let offset = self.get_rm_word(modrm, bus);
                self.biu_fetch_suspend(bus);
                self.clk(bus, 1);
                let segment = self.seg_read_word_at(bus, 2);
                let uses_preloaded_handoff =
                    self.far_indirect_transfer_uses_preloaded_handoff(modrm);
                self.sregs[SegReg16::CS as usize] = segment;
                self.ip = offset;
                self.flush_and_fetch(bus);
                if uses_preloaded_handoff {
                    self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                }
            }
            6 | 7 => {
                // PUSH r/m16 (7 is undocumented alias)
                if modrm >= 0xC0 && (modrm & 7) == 4 {
                    let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
                    self.regs.set_word(WordReg::SP, sp);
                    self.write_word_seg(bus, SegReg16::SS, sp, sp);
                } else {
                    let val = self.get_rm_word(modrm, bus);
                    self.push(bus, val);
                }
                if modrm >= 0xC0 {
                    if self.seg_prefix || !self.instruction_entry_queue_full() {
                        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                    }
                    self.clk(bus, 4);
                } else {
                    self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
                    self.clk(bus, 3);
                }
            }
            _ => {
                self.clk(bus, 2);
            }
        }
    }
}
