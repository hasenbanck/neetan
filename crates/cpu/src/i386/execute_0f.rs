use super::I386;
use crate::{ByteReg, SegReg32};

impl I386 {
    pub(super) fn extended_0f(&mut self, bus: &mut impl common::Bus) {
        let sub = self.fetch(bus);
        match sub {
            0x00 => self.group_0f00(bus),
            0x01 => self.group_0f01(bus),
            0x02 => self.lar(bus),
            0x03 => self.lsl_instr(bus),
            0x06 => self.clts(bus),

            0x20 => self.mov_r32_cr(bus),
            0x21 => self.mov_r32_dr(bus),
            0x22 => self.mov_cr_r32(bus),
            0x23 => self.mov_dr_r32(bus),

            0x80..=0x8F => self.jcc_near(sub & 0x0F, bus),
            0x90..=0x9F => self.setcc(sub & 0x0F, bus),

            0xA0 => self.push_seg(SegReg32::FS, bus),
            0xA1 => self.pop_seg(SegReg32::FS, bus),
            0xA3 => self.bt_reg(bus),
            0xA4 => self.shld_imm(bus),
            0xA5 => self.shld_cl(bus),
            0xA8 => self.push_seg(SegReg32::GS, bus),
            0xA9 => self.pop_seg(SegReg32::GS, bus),
            0xAB => self.bts_reg(bus),
            0xAC => self.shrd_imm(bus),
            0xAD => self.shrd_cl(bus),
            0xAF => self.imul_reg_rm(bus),

            0xB2 => self.lss(bus),
            0xB3 => self.btr_reg(bus),
            0xB4 => self.lfs(bus),
            0xB5 => self.lgs(bus),
            0xB6 => self.movzx_rm8(bus),
            0xB7 => self.movzx_rm16(bus),
            0xBA => self.group_ba(bus),
            0xBB => self.btc_reg(bus),
            0xBC => self.bsf(bus),
            0xBD => self.bsr(bus),
            0xBE => self.movsx_rm8(bus),
            0xBF => self.movsx_rm16(bus),

            _ => self.raise_fault(6, bus),
        }
    }

    #[inline(always)]
    fn cond(&self, cc: u8) -> bool {
        match cc & 0x0F {
            0x0 => self.flags.of(),
            0x1 => !self.flags.of(),
            0x2 => self.flags.cf(),
            0x3 => !self.flags.cf(),
            0x4 => self.flags.zf(),
            0x5 => !self.flags.zf(),
            0x6 => self.flags.cf() || self.flags.zf(),
            0x7 => !self.flags.cf() && !self.flags.zf(),
            0x8 => self.flags.sf(),
            0x9 => !self.flags.sf(),
            0xA => self.flags.pf(),
            0xB => !self.flags.pf(),
            0xC => self.flags.sf() != self.flags.of(),
            0xD => self.flags.sf() == self.flags.of(),
            0xE => self.flags.zf() || (self.flags.sf() != self.flags.of()),
            0xF => !self.flags.zf() && (self.flags.sf() == self.flags.of()),
            _ => unreachable!(),
        }
    }

    fn clts(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        self.cr0 &= !0x0000_0008;
        self.clk(5);
    }

    fn jcc_near(&mut self, cc: u8, bus: &mut impl common::Bus) {
        let condition = self.cond(cc);
        if self.operand_size_override {
            let disp = self.fetchdword(bus) as i32;
            if condition {
                let eip = (self.ip_upper | self.ip as u32).wrapping_add(disp as u32);
                self.ip = eip as u16;
                self.ip_upper = eip & 0xFFFF_0000;
                let m = self.next_instruction_length_approx(bus);
                self.clk(7 + m);
            } else {
                self.clk(3);
            }
        } else {
            let disp = self.fetchword(bus) as i16;
            if condition {
                self.ip = self.ip.wrapping_add(disp as u16);
                self.ip_upper = 0;
                let m = self.next_instruction_length_approx(bus);
                self.clk(7 + m);
            } else {
                self.clk(3);
            }
        }
    }

    fn setcc(&mut self, cc: u8, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let value = if self.cond(cc) { 1 } else { 0 };
        self.put_rm_byte(modrm, value, bus);
        self.clk_modrm(modrm, 4, 5);
    }

    #[inline(always)]
    fn bit_parts(&self, bit_offset: i32, bits_per_unit: u32) -> (u32, i32) {
        let unit_shift = if bits_per_unit == 32 { 5 } else { 4 };
        let bit_index = (bit_offset & (bits_per_unit as i32 - 1)) as u32;
        let unit_index = bit_offset >> unit_shift;
        let byte_delta = unit_index * (bits_per_unit as i32 / 8);
        (bit_index, byte_delta)
    }

    #[inline(always)]
    fn bit_mem_effective_offset(&self, bit_offset: i32, bits_per_unit: u32) -> (u32, u32) {
        let (bit_index, byte_delta) = self.bit_parts(bit_offset, bits_per_unit);
        let offset = if self.address_size_override {
            self.eo32.wrapping_add(byte_delta as u32)
        } else {
            (self.eo32 as u16).wrapping_add(byte_delta as u16) as u32
        };
        (offset, bit_index)
    }

    fn bt_reg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let bit_offset = self.regs.dword(self.reg_dword(modrm));
            if modrm >= 0xC0 {
                let value = self.regs.dword(self.rm_dword(modrm));
                self.flags.carry_val = (value >> (bit_offset & 31)) & 1;
            } else {
                self.calc_ea(modrm, bus);
                let (offset, bit_index) = self.bit_mem_effective_offset(bit_offset as i32, 32);
                let value = self.read_dword_seg(bus, self.ea_seg, offset);
                self.flags.carry_val = (value >> bit_index) & 1;
            }
        } else {
            let bit_offset = self.regs.word(self.reg_word(modrm));
            if modrm >= 0xC0 {
                let value = self.regs.word(self.rm_word(modrm)) as u32;
                self.flags.carry_val = (value >> (bit_offset as u32 & 15)) & 1;
            } else {
                self.calc_ea(modrm, bus);
                let signed_offset = bit_offset as i16 as i32;
                let (offset, bit_index) = self.bit_mem_effective_offset(signed_offset, 16);
                let value = self.read_word_seg(bus, self.ea_seg, offset) as u32;
                self.flags.carry_val = (value >> bit_index) & 1;
            }
        }
        self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
        self.clk_modrm_word(modrm, 3, 12, 2);
    }

    fn bts_reg(&mut self, bus: &mut impl common::Bus) {
        self.bit_modify_reg(bus, false, true, false, 6, 13);
    }

    fn btr_reg(&mut self, bus: &mut impl common::Bus) {
        self.bit_modify_reg(bus, true, false, false, 6, 13);
    }

    fn btc_reg(&mut self, bus: &mut impl common::Bus) {
        self.bit_modify_reg(bus, false, false, true, 6, 13);
    }

    fn bit_modify_reg(
        &mut self,
        bus: &mut impl common::Bus,
        clear: bool,
        set: bool,
        toggle: bool,
        register_cycles: i32,
        memory_cycles: i32,
    ) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let bit_offset = self.regs.dword(self.reg_dword(modrm));
            if modrm >= 0xC0 {
                let mut value = self.regs.dword(self.rm_dword(modrm));
                let bit = 1u32 << (bit_offset & 31);
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                let reg = self.rm_dword(modrm);
                self.regs.set_dword(reg, value);
            } else {
                self.calc_ea(modrm, bus);
                let (offset, bit_index) = self.bit_mem_effective_offset(bit_offset as i32, 32);
                let mut value = self.read_dword_seg(bus, self.ea_seg, offset);
                let bit = 1u32 << bit_index;
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                self.write_dword_seg(bus, self.ea_seg, offset, value);
            }
        } else {
            let bit_offset = self.regs.word(self.reg_word(modrm));
            if modrm >= 0xC0 {
                let mut value = self.regs.word(self.rm_word(modrm));
                let bit = 1u16 << (bit_offset as u32 & 15);
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                let reg = self.rm_word(modrm);
                self.regs.set_word(reg, value);
            } else {
                self.calc_ea(modrm, bus);
                let signed_offset = bit_offset as i16 as i32;
                let (offset, bit_index) = self.bit_mem_effective_offset(signed_offset, 16);
                let mut value = self.read_word_seg(bus, self.ea_seg, offset);
                let bit = 1u16 << bit_index;
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                self.write_word_seg(bus, self.ea_seg, offset, value);
            }
        }
        self.clk_modrm_word(modrm, register_cycles, memory_cycles, 2);
    }

    fn bit_modify_imm(
        &mut self,
        bus: &mut impl common::Bus,
        clear: bool,
        set: bool,
        toggle: bool,
        register_cycles: i32,
        memory_cycles: i32,
    ) {
        let modrm = self.fetch(bus);

        if self.operand_size_override {
            if modrm >= 0xC0 {
                let imm = self.fetch(bus) as u32;
                let mut value = self.regs.dword(self.rm_dword(modrm));
                let bit = 1u32 << (imm & 31);
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                let reg = self.rm_dword(modrm);
                self.regs.set_dword(reg, value);
            } else {
                self.calc_ea(modrm, bus);
                let imm = self.fetch(bus) as u32;
                let bit_index = imm & 31;
                if self.address_size_override {
                    let address = self.ea;
                    let mut value = self.read_dword_linear(bus, address);
                    let bit = 1u32 << bit_index;
                    self.flags.carry_val = u32::from(value & bit != 0);
                    if clear {
                        value &= !bit;
                    }
                    if set {
                        value |= bit;
                    }
                    if toggle {
                        value ^= bit;
                    }
                    self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                    self.write_dword_linear(bus, address, value);
                } else {
                    let mut value = self.seg_read_dword(bus);
                    let bit = 1u32 << bit_index;
                    self.flags.carry_val = u32::from(value & bit != 0);
                    if clear {
                        value &= !bit;
                    }
                    if set {
                        value |= bit;
                    }
                    if toggle {
                        value ^= bit;
                    }
                    self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                    self.seg_write_dword(bus, value);
                }
            }
        } else if modrm >= 0xC0 {
            let imm = self.fetch(bus) as u32;
            let mut value = self.regs.word(self.rm_word(modrm));
            let bit = 1u16 << (imm & 15);
            self.flags.carry_val = u32::from(value & bit != 0);
            if clear {
                value &= !bit;
            }
            if set {
                value |= bit;
            }
            if toggle {
                value ^= bit;
            }
            self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
            let reg = self.rm_word(modrm);
            self.regs.set_word(reg, value);
        } else {
            self.calc_ea(modrm, bus);
            let imm = self.fetch(bus) as u32;
            let bit_index = imm & 15;
            if self.address_size_override {
                let address = self.ea;
                let mut value = self.read_word_linear(bus, address);
                let bit = 1u16 << bit_index;
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                self.write_word_linear(bus, address, value);
            } else {
                let mut value = self.seg_read_word(bus);
                let bit = 1u16 << bit_index;
                self.flags.carry_val = u32::from(value & bit != 0);
                if clear {
                    value &= !bit;
                }
                if set {
                    value |= bit;
                }
                if toggle {
                    value ^= bit;
                }
                self.flags.overflow_val = if self.flags.carry_val != 0 { 0x0800 } else { 0 };
                self.seg_write_word(bus, value);
            }
        }

        self.clk_modrm_word(modrm, register_cycles, memory_cycles, 2);
    }

    fn group_ba(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.ip = self.ip.wrapping_sub(1);
        match (modrm >> 3) & 7 {
            4 => self.bit_modify_imm(bus, false, false, false, 3, 6),
            5 => self.bit_modify_imm(bus, false, true, false, 6, 8),
            6 => self.bit_modify_imm(bus, true, false, false, 6, 8),
            7 => self.bit_modify_imm(bus, false, false, true, 6, 8),
            _ => self.raise_fault(6, bus),
        }
    }

    fn shld_imm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let count = self.fetch(bus) & 0x1F;
            if count != 0 {
                let result = (dst << count) | (src >> (32 - count));
                self.flags.carry_val = (dst >> (32 - count)) & 1;
                self.flags.overflow_val = ((result >> 31) & 1) ^ self.flags.carry_val;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_dword(result);
                self.putback_rm_dword(modrm, result, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm)) as u32;
            let dst = self.get_rm_word(modrm, bus) as u32;
            let count = self.fetch(bus) & 0x1F;
            if count != 0 {
                let result = if count < 16 {
                    (((dst << 16) | src) << count) >> 16
                } else {
                    (src << (count - 16)) | (src >> (32 - count))
                } & 0xFFFF;
                self.flags.carry_val = if count < 16 {
                    (dst >> (16 - count)) & 1
                } else if count == 16 {
                    dst & 1
                } else {
                    (src >> (32 - count)) & 1
                };
                self.flags.overflow_val = ((result >> 15) & 1) ^ self.flags.carry_val;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_word(result);
                self.putback_rm_word(modrm, result as u16, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 2);
        }
    }

    fn shld_cl(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let count = self.regs.byte(ByteReg::CL) & 0x1F;
            if count != 0 {
                let result = (dst << count) | (src >> (32 - count));
                self.flags.carry_val = (dst >> (32 - count)) & 1;
                self.flags.overflow_val = ((result >> 31) & 1) ^ self.flags.carry_val;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_dword(result);
                self.putback_rm_dword(modrm, result, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm)) as u32;
            let dst = self.get_rm_word(modrm, bus) as u32;
            let count = self.regs.byte(ByteReg::CL) & 0x1F;
            if count != 0 {
                let result = if count < 16 {
                    (((dst << 16) | src) << count) >> 16
                } else {
                    (src << (count - 16)) | (src >> (32 - count))
                } & 0xFFFF;
                self.flags.carry_val = if count < 16 {
                    (dst >> (16 - count)) & 1
                } else if count == 16 {
                    dst & 1
                } else {
                    (src >> (32 - count)) & 1
                };
                self.flags.overflow_val = ((result >> 15) & 1) ^ self.flags.carry_val;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_word(result);
                self.putback_rm_word(modrm, result as u16, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 2);
        }
    }

    fn shrd_imm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let count = self.fetch(bus) & 0x1F;
            if count != 0 {
                let result = (dst >> count) | (src << (32 - count));
                self.flags.carry_val = (dst >> (count - 1)) & 1;
                self.flags.overflow_val = ((result >> 31) ^ (result >> 30)) & 1;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_dword(result);
                self.putback_rm_dword(modrm, result, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm)) as u32;
            let dst = self.get_rm_word(modrm, bus) as u32;
            let count = self.fetch(bus) & 0x1F;
            if count != 0 {
                let result = if count < 16 {
                    (dst >> count) | (src << (16 - count))
                } else {
                    (src >> (count - 16)) | (src << (32 - count))
                } & 0xFFFF;
                self.flags.carry_val = if count <= 16 {
                    if count == 16 {
                        (dst >> 15) & 1
                    } else {
                        (dst >> (count - 1)) & 1
                    }
                } else {
                    (src >> (count - 17)) & 1
                };
                self.flags.overflow_val = ((result >> 15) ^ (result >> 14)) & 1;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_word(result);
                self.putback_rm_word(modrm, result as u16, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 2);
        }
    }

    fn shrd_cl(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let count = self.regs.byte(ByteReg::CL) & 0x1F;
            if count != 0 {
                let result = (dst >> count) | (src << (32 - count));
                self.flags.carry_val = (dst >> (count - 1)) & 1;
                self.flags.overflow_val = ((result >> 31) ^ (result >> 30)) & 1;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_dword(result);
                self.putback_rm_dword(modrm, result, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm)) as u32;
            let dst = self.get_rm_word(modrm, bus) as u32;
            let count = self.regs.byte(ByteReg::CL) & 0x1F;
            if count != 0 {
                let result = if count < 16 {
                    (dst >> count) | (src << (16 - count))
                } else {
                    (src >> (count - 16)) | (src << (32 - count))
                } & 0xFFFF;
                self.flags.carry_val = if count <= 16 {
                    if count == 16 {
                        (dst >> 15) & 1
                    } else {
                        (dst >> (count - 1)) & 1
                    }
                } else {
                    (src >> (count - 17)) & 1
                };
                self.flags.overflow_val = ((result >> 15) ^ (result >> 14)) & 1;
                self.flags.aux_val = 0x10;
                self.flags.set_szpf_word(result);
                self.putback_rm_word(modrm, result as u16, bus);
            }
            self.clk_modrm_word(modrm, 3, 7, 2);
        }
    }

    fn imul_reg_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.get_rm_dword(modrm, bus) as i32 as i64;
            let dst = self.regs.dword(self.reg_dword(modrm)) as i32 as i64;
            let result = dst * src;
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result as u32);
            self.flags.carry_val = u32::from(result < i32::MIN as i64 || result > i32::MAX as i64);
            self.flags.overflow_val = self.flags.carry_val;
            self.clk_modrm_word(modrm, 38, 41, 2);
        } else {
            let src = self.get_rm_word(modrm, bus) as i16 as i32;
            let dst = self.regs.word(self.reg_word(modrm)) as i16 as i32;
            let result = dst * src;
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result as u16);
            self.flags.carry_val = u32::from(result < i16::MIN as i32 || result > i16::MAX as i32);
            self.flags.overflow_val = self.flags.carry_val;
            self.clk_modrm_word(modrm, 22, 25, 2);
        }
    }

    fn lss(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            self.raise_fault(6, bus);
            return;
        }

        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let offset = self.seg_read_dword(bus);
            let seg = self.seg_read_word_at(bus, 4);
            if !self.load_segment(SegReg32::SS, seg, bus) {
                return;
            }
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, offset);
        } else {
            let offset = self.seg_read_word(bus);
            let seg = self.seg_read_word_at(bus, 2);
            if !self.load_segment(SegReg32::SS, seg, bus) {
                return;
            }
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, offset);
        }
        self.clk(7);
    }

    fn lfs(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            self.raise_fault(6, bus);
            return;
        }

        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let offset = self.seg_read_dword(bus);
            let seg = self.seg_read_word_at(bus, 4);
            if !self.load_segment(SegReg32::FS, seg, bus) {
                return;
            }
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, offset);
        } else {
            let offset = self.seg_read_word(bus);
            let seg = self.seg_read_word_at(bus, 2);
            if !self.load_segment(SegReg32::FS, seg, bus) {
                return;
            }
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, offset);
        }
        self.clk(7);
    }

    fn lgs(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            self.raise_fault(6, bus);
            return;
        }

        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let offset = self.seg_read_dword(bus);
            let seg = self.seg_read_word_at(bus, 4);
            if !self.load_segment(SegReg32::GS, seg, bus) {
                return;
            }
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, offset);
        } else {
            let offset = self.seg_read_word(bus);
            let seg = self.seg_read_word_at(bus, 2);
            if !self.load_segment(SegReg32::GS, seg, bus) {
                return;
            }
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, offset);
        }
        self.clk(7);
    }

    fn movzx_rm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let value = self.get_rm_byte(modrm, bus) as u32;
        if self.operand_size_override {
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, value);
        } else {
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, value as u16);
        }
        self.clk_modrm(modrm, 3, 6);
    }

    fn movzx_rm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let value = self.get_rm_word(modrm, bus) as u32;
        if self.operand_size_override {
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, value);
        } else {
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, value as u16);
        }
        self.clk_modrm_word(modrm, 3, 6, 1);
    }

    fn movsx_rm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let value = self.get_rm_byte(modrm, bus) as i8 as i32;
        if self.operand_size_override {
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, value as u32);
        } else {
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, value as u16);
        }
        self.clk_modrm(modrm, 3, 6);
    }

    fn movsx_rm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let value = self.get_rm_word(modrm, bus) as i16 as i32;
        if self.operand_size_override {
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, value as u32);
        } else {
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, value as u16);
        }
        self.clk_modrm_word(modrm, 3, 6, 1);
    }

    fn bsf(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let n: u32;
        if self.operand_size_override {
            let value = self.get_rm_dword(modrm, bus);
            if value == 0 {
                self.flags.zero_val = 0;
                n = 32;
            } else {
                self.flags.zero_val = 1;
                let index = value.trailing_zeros();
                let reg = self.reg_dword(modrm);
                self.regs.set_dword(reg, index);
                n = index + 1;
            }
        } else {
            let value = self.get_rm_word(modrm, bus);
            if value == 0 {
                self.flags.zero_val = 0;
                n = 16;
            } else {
                self.flags.zero_val = 1;
                let index = value.trailing_zeros();
                let reg = self.reg_word(modrm);
                self.regs.set_word(reg, index as u16);
                n = index + 1;
            }
        }
        self.clk(10 + 3 * n as i32);
    }

    fn bsr(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let n: u32;
        if self.operand_size_override {
            let value = self.get_rm_dword(modrm, bus);
            if value == 0 {
                self.flags.zero_val = 0;
                n = 32;
            } else {
                self.flags.zero_val = 1;
                let index = 31 - value.leading_zeros();
                let reg = self.reg_dword(modrm);
                self.regs.set_dword(reg, index);
                n = value.leading_zeros() + 1;
            }
        } else {
            let value = self.get_rm_word(modrm, bus);
            if value == 0 {
                self.flags.zero_val = 0;
                n = 16;
            } else {
                self.flags.zero_val = 1;
                let index = 15 - value.leading_zeros();
                let reg = self.reg_word(modrm);
                self.regs.set_word(reg, index as u16);
                n = value.leading_zeros() + 1;
            }
        }
        self.clk(10 + 3 * n as i32);
    }

    fn group_0f00(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if !self.is_protected_mode() {
            self.raise_fault(6, bus);
            return;
        }
        match (modrm >> 3) & 7 {
            0 => {
                // SLDT
                if self.operand_size_override && modrm >= 0xC0 {
                    self.put_rm_dword(modrm, self.ldtr as u32, bus);
                } else {
                    self.put_rm_word(modrm, self.ldtr, bus);
                }
                self.clk_modrm(modrm, 2, 3);
            }
            1 => {
                // STR
                if self.operand_size_override && modrm >= 0xC0 {
                    self.put_rm_dword(modrm, self.tr as u32, bus);
                } else {
                    self.put_rm_word(modrm, self.tr, bus);
                }
                self.clk_modrm(modrm, 2, 3);
            }
            2 => {
                // LLDT
                if self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                let selector = self.get_rm_word(modrm, bus);
                if selector & 0xFFFC == 0 {
                    self.ldtr = selector;
                    self.ldtr_base = 0;
                    self.ldtr_limit = 0;
                } else {
                    if selector & 0x0004 != 0 {
                        self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                        return;
                    }
                    let Some(descriptor) = self.decode_descriptor(selector, bus) else {
                        self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                        return;
                    };
                    let desc_type = descriptor.rights & 0x0F;
                    if descriptor.rights & 0x10 != 0 || desc_type != 0x02 {
                        self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                        return;
                    }
                    if descriptor.rights & 0x80 == 0 {
                        self.raise_fault_with_code(11, selector & 0xFFFC, bus);
                        return;
                    }
                    self.ldtr = selector;
                    self.ldtr_base = descriptor.base;
                    self.ldtr_limit = descriptor.limit;
                }
                self.clk_modrm(modrm, 17, 19);
            }
            3 => {
                // LTR - accepts available 286 TSS (type 1) and available 386 TSS (type 9)
                if self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                let selector = self.get_rm_word(modrm, bus);
                if selector & 0xFFFC == 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                if selector & 0x0004 != 0 {
                    self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                    return;
                }
                let Some(descriptor) = self.decode_descriptor(selector, bus) else {
                    self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                    return;
                };
                let desc_type = descriptor.rights & 0x0F;
                if descriptor.rights & 0x10 != 0 || (desc_type != 0x01 && desc_type != 0x09) {
                    self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                    return;
                }
                if descriptor.rights & 0x80 == 0 {
                    self.raise_fault_with_code(11, selector & 0xFFFC, bus);
                    return;
                }
                let min_limit: u32 = if desc_type == 0x09 { 103 } else { 43 };
                if descriptor.limit < min_limit {
                    self.raise_fault_with_code(10, selector & 0xFFFC, bus);
                    return;
                }
                self.tr = selector;
                self.tr_base = descriptor.base;
                self.tr_limit = descriptor.limit;
                self.tr_rights = descriptor.rights;
                // Mark TSS as busy by setting bit 1 of type field.
                self.tr_rights |= 0x02;
                if let Some(addr) = self.descriptor_addr_checked(selector) {
                    let linear = addr.wrapping_add(5);
                    let phys = self.translate_linear(linear, true, bus).unwrap_or(0);
                    let r = bus.read_byte(phys);
                    bus.write_byte(phys, r | 0x02);
                }
                self.clk_modrm(modrm, 17, 19);
            }
            4 => {
                // VERR
                let selector = self.get_rm_word(modrm, bus);
                let readable = self.verr_accessible(selector, bus);
                self.flags.zero_val = if readable { 0 } else { 1 };
                self.clk_modrm(modrm, 14, 16);
            }
            5 => {
                // VERW
                let selector = self.get_rm_word(modrm, bus);
                let writable = self.selector_accessible(selector, true, bus);
                self.flags.zero_val = if writable { 0 } else { 1 };
                self.clk_modrm(modrm, 14, 16);
            }
            _ => self.raise_fault(6, bus),
        }
    }

    fn group_0f01(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // SGDT - store full 32-bit base on 386
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                let gdt_limit = self.gdt_limit;
                let gdt_base = self.gdt_base;
                self.write_word_linear(bus, self.seg_addr(0), gdt_limit);
                self.write_dword_linear(bus, self.seg_addr(2), gdt_base);
                self.clk(11);
            }
            1 => {
                // SIDT - store full 32-bit base on 386
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                let idt_limit = self.idt_limit;
                let idt_base = self.idt_base;
                self.write_word_linear(bus, self.seg_addr(0), idt_limit);
                self.write_dword_linear(bus, self.seg_addr(2), idt_base);
                self.clk(12);
            }
            2 => {
                // LGDT - load full 32-bit base on 386
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                if self.is_protected_mode() && self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                let limit = self.read_word_linear(bus, self.seg_addr(0));
                let base = self.read_dword_linear(bus, self.seg_addr(2));
                self.gdt_base = base;
                self.gdt_limit = limit;
                self.clk(11);
            }
            3 => {
                // LIDT - load full 32-bit base on 386
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                if self.is_protected_mode() && self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                let limit = self.read_word_linear(bus, self.seg_addr(0));
                let base = self.read_dword_linear(bus, self.seg_addr(2));
                self.idt_base = base;
                self.idt_limit = limit;
                self.clk(12);
            }
            4 => {
                // SMSW - register gets full 32-bit CR0, memory gets 16-bit
                if modrm >= 0xC0 {
                    let cr0 = self.cr0;
                    let reg = self.rm_dword(modrm);
                    self.regs.set_dword(reg, cr0);
                } else {
                    self.put_rm_word(modrm, self.cr0 as u16, bus);
                }
                self.clk_modrm(modrm, 2, 3);
            }
            6 => {
                // LMSW - only writes low 4 bits of CR0 (PE/MP/EM/TS), cannot clear PE
                if self.is_protected_mode() && self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                let value = self.get_rm_word(modrm, bus);
                self.cr0 = (self.cr0 & 0xFFFF_FFF0) | (value as u32 & 0x000F) | (self.cr0 & 1);
                self.clk_modrm(modrm, 10, 13);
            }
            _ => self.raise_fault(6, bus),
        }
    }

    fn lar(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if !self.is_protected_mode() {
            self.raise_fault(6, bus);
            return;
        }
        let selector = self.get_rm_word(modrm, bus);
        self.flags.zero_val = 1; // ZF=0: invalid by default
        if selector & 0xFFFC != 0
            && let Some(descriptor) = self.decode_descriptor(selector, bus)
        {
            let rights = descriptor.rights;
            let desc_type = rights & 0x1F;
            let valid_type = if rights & 0x10 != 0 {
                true
            } else {
                matches!(desc_type, 1..=5 | 9 | 11 | 12)
            };
            if valid_type {
                let cpl = self.cpl();
                let rpl = selector & 3;
                let dpl = Self::descriptor_dpl(rights);
                let priv_ok = if Self::descriptor_is_segment(rights)
                    && Self::descriptor_is_conforming_code(rights)
                {
                    true
                } else {
                    dpl >= cpl.max(rpl)
                };
                if priv_ok {
                    if self.operand_size_override {
                        let reg = self.reg_dword(modrm);
                        let result = ((rights as u32) << 8)
                            | (((descriptor.granularity & 0xF0) as u32) << 16);
                        self.regs.set_dword(reg, result);
                    } else {
                        let reg = self.reg_word(modrm);
                        self.regs.set_word(reg, (rights as u16) << 8);
                    }
                    self.flags.zero_val = 0; // ZF=1: valid
                }
            }
        }
        self.clk_modrm(modrm, 14, 16);
    }

    fn lsl_instr(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if !self.is_protected_mode() {
            self.raise_fault(6, bus);
            return;
        }
        let selector = self.get_rm_word(modrm, bus);
        self.flags.zero_val = 1; // ZF=0: invalid by default
        if selector & 0xFFFC != 0
            && let Some(descriptor) = self.decode_descriptor(selector, bus)
        {
            let rights = descriptor.rights;
            let desc_type = rights & 0x1F;
            let valid_type = if rights & 0x10 != 0 {
                true
            } else {
                matches!(desc_type, 1..=3 | 9 | 11)
            };
            if valid_type {
                let cpl = self.cpl();
                let rpl = selector & 3;
                let dpl = Self::descriptor_dpl(rights);
                let priv_ok = if Self::descriptor_is_segment(rights)
                    && Self::descriptor_is_conforming_code(rights)
                {
                    true
                } else {
                    dpl >= cpl.max(rpl)
                };
                if priv_ok {
                    if self.operand_size_override {
                        let reg = self.reg_dword(modrm);
                        self.regs.set_dword(reg, descriptor.limit);
                    } else {
                        let reg = self.reg_word(modrm);
                        self.regs.set_word(reg, descriptor.limit as u16);
                    }
                    self.flags.zero_val = 0; // ZF=1: valid
                }
            }
        }
        self.clk_modrm(modrm, 14, 16);
    }

    fn verr_accessible(&mut self, selector: u16, bus: &mut impl common::Bus) -> bool {
        if selector & 0xFFFC == 0 {
            return false;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            return false;
        };
        let rights = descriptor.rights;
        if !Self::descriptor_is_segment(rights) {
            return false;
        }
        let cpl = self.cpl();
        let rpl = selector & 3;
        let dpl = Self::descriptor_dpl(rights);
        if !Self::descriptor_is_conforming_code(rights) && dpl < cpl.max(rpl) {
            return false;
        }
        Self::descriptor_is_readable(rights)
    }

    fn selector_accessible(
        &mut self,
        selector: u16,
        write: bool,
        bus: &mut impl common::Bus,
    ) -> bool {
        if selector & 0xFFFC == 0 {
            return false;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            return false;
        };
        let rights = descriptor.rights;
        if !Self::descriptor_is_segment(rights) {
            return false;
        }
        let cpl = self.cpl();
        let rpl = selector & 3;
        let dpl = Self::descriptor_dpl(rights);
        if dpl < cpl.max(rpl) {
            return false;
        }
        if write {
            return Self::descriptor_is_writable(rights);
        }
        Self::descriptor_is_readable(rights)
    }

    /// MOV r32, CRn (0F 20) — read from control register.
    fn mov_r32_cr(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        let modrm = self.fetch(bus);
        let cr_num = (modrm >> 3) & 7;
        let value = match cr_num {
            0 => self.cr0,
            2 => self.cr2,
            3 => self.cr3,
            _ => {
                self.raise_fault(6, bus);
                return;
            }
        };
        let reg = self.rm_dword(modrm);
        self.regs.set_dword(reg, value);
        self.clk(6);
    }

    /// MOV r32, DRn (0F 21) — read from debug register.
    fn mov_r32_dr(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        let modrm = self.fetch(bus);
        let dr_num = (modrm >> 3) & 7;
        let value = match dr_num {
            0 => self.dr0,
            1 => self.dr1,
            2 => self.dr2,
            3 => self.dr3,
            4 => self.dr6,
            5 => self.dr7,
            6 => self.dr6,
            7 => self.dr7,
            _ => unreachable!(),
        };
        let reg = self.rm_dword(modrm);
        self.regs.set_dword(reg, value);
        self.clk(22);
    }

    /// MOV CRn, r32 (0F 22) — write to control register.
    /// On a 386, CR0 writable bits are: PE(0), MP(1), EM(2), TS(3), ET(4), PG(31).
    /// Bits 5-15 and 17-30 are reserved and always read as 0.
    fn mov_cr_r32(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        let modrm = self.fetch(bus);
        let cr_num = (modrm >> 3) & 7;
        let value = self.regs.dword(self.rm_dword(modrm));
        match cr_num {
            0 => {
                let old_cr0 = self.cr0;
                // Mask to 386-valid bits: PG(31) | ET(4) | TS(3) | EM(2) | MP(1) | PE(0).
                self.cr0 = value & 0x8000_001F;
                if (old_cr0 ^ self.cr0) & 0x8000_0001 != 0 {
                    self.flush_tlb();
                    self.prefetch_valid = false;
                }
                self.clk(10);
            }
            2 => {
                self.cr2 = value;
                self.clk(5);
            }
            3 => {
                self.cr3 = value;
                self.flush_tlb();
                self.prefetch_valid = false;
                self.clk(5);
            }
            _ => {
                self.raise_fault(6, bus);
            }
        }
    }

    /// MOV DRn, r32 (0F 23) — write to debug register.
    fn mov_dr_r32(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        let modrm = self.fetch(bus);
        let dr_num = (modrm >> 3) & 7;
        let value = self.regs.dword(self.rm_dword(modrm));
        match dr_num {
            0 => self.dr0 = value,
            1 => self.dr1 = value,
            2 => self.dr2 = value,
            3 => self.dr3 = value,
            4 => self.dr6 = value,
            5 => self.dr7 = value,
            6 => self.dr6 = value,
            7 => self.dr7 = value,
            _ => unreachable!(),
        }
        self.clk(22);
    }
}
