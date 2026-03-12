use super::V30;
use crate::{ByteReg, SegReg16, WordReg};

impl V30 {
    pub(super) fn extended_0f(&mut self, bus: &mut impl common::Bus) {
        let sub = self.fetch(bus);
        match sub {
            // TEST1 r/m8, CL
            0x10 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0x7;
                self.flags.set_szpf_byte((tmp & (1 << bit)) as u32);
                self.flags.carry_val = 0;
                self.flags.overflow_val = 0;
                self.flags.aux_val = 0;
                self.clk_modrm(modrm, 3, 8);
            }
            // TEST1 r/m16, CL
            0x11 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0xF;
                self.flags.set_szpf_word((tmp & (1 << bit)) as u32);
                self.flags.carry_val = 0;
                self.flags.overflow_val = 0;
                self.flags.aux_val = 0;
                self.clk_modrm_word(modrm, 3, 8, 1);
            }
            // CLR1 r/m8, CL
            0x12 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0x7;
                self.putback_rm_byte(modrm, tmp & !(1 << bit), bus);
                self.clk_modrm(modrm, 5, 14);
            }
            // CLR1 r/m16, CL
            0x13 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0xF;
                self.putback_rm_word(modrm, tmp & !(1 << bit), bus);
                self.clk_modrm_word(modrm, 5, 14, 2);
            }
            // SET1 r/m8, CL
            0x14 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0x7;
                self.putback_rm_byte(modrm, tmp | (1 << bit), bus);
                self.clk_modrm(modrm, 4, 13);
            }
            // SET1 r/m16, CL
            0x15 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0xF;
                self.putback_rm_word(modrm, tmp | (1 << bit), bus);
                self.clk_modrm_word(modrm, 4, 13, 2);
            }
            // NOT1 r/m8, CL
            0x16 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0x7;
                self.putback_rm_byte(modrm, tmp ^ (1 << bit), bus);
                self.clk_modrm(modrm, 4, 13);
            }
            // NOT1 r/m16, CL
            0x17 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.regs.byte(ByteReg::CL) & 0xF;
                self.putback_rm_word(modrm, tmp ^ (1 << bit), bus);
                self.clk_modrm_word(modrm, 4, 13, 2);
            }
            // TEST1 r/m8, imm3
            0x18 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.fetch(bus) & 0x7;
                self.flags.set_szpf_byte((tmp & (1 << bit)) as u32);
                self.flags.carry_val = 0;
                self.flags.overflow_val = 0;
                self.flags.aux_val = 0;
                self.clk_modrm(modrm, 4, 9);
            }
            // TEST1 r/m16, imm4
            0x19 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.fetch(bus) & 0xF;
                self.flags.set_szpf_word((tmp & (1 << bit)) as u32);
                self.flags.carry_val = 0;
                self.flags.overflow_val = 0;
                self.flags.aux_val = 0;
                self.clk_modrm_word(modrm, 4, 9, 1);
            }
            // CLR1 r/m8, imm3
            0x1A => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.fetch(bus) & 0x7;
                self.putback_rm_byte(modrm, tmp & !(1 << bit), bus);
                self.clk_modrm(modrm, 6, 15);
            }
            // CLR1 r/m16, imm4
            0x1B => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.fetch(bus) & 0xF;
                self.putback_rm_word(modrm, tmp & !(1 << bit), bus);
                self.clk_modrm_word(modrm, 6, 15, 2);
            }
            // SET1 r/m8, imm3
            0x1C => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.fetch(bus) & 0x7;
                self.putback_rm_byte(modrm, tmp | (1 << bit), bus);
                self.clk_modrm(modrm, 5, 14);
            }
            // SET1 r/m16, imm4
            0x1D => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.fetch(bus) & 0xF;
                self.putback_rm_word(modrm, tmp | (1 << bit), bus);
                self.clk_modrm_word(modrm, 5, 14, 2);
            }
            // NOT1 r/m8, imm3
            0x1E => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let bit = self.fetch(bus) & 0x7;
                self.putback_rm_byte(modrm, tmp ^ (1 << bit), bus);
                self.clk_modrm(modrm, 5, 14);
            }
            // NOT1 r/m16, imm4
            0x1F => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_word(modrm, bus);
                let bit = self.fetch(bus) & 0xF;
                self.putback_rm_word(modrm, tmp ^ (1 << bit), bus);
                self.clk_modrm_word(modrm, 5, 14, 2);
            }
            // ADD4S
            0x20 => {
                let count = self.regs.byte(ByteReg::CL).div_ceil(2) as i32;
                self.add4s(bus);
                self.clk(7 + 19 * count);
            }
            // SUB4S
            0x22 => {
                let count = self.regs.byte(ByteReg::CL).div_ceil(2) as i32;
                self.sub4s(bus);
                self.clk(7 + 19 * count);
            }
            // CMP4S
            0x26 => {
                let count = self.regs.byte(ByteReg::CL).div_ceil(2) as i32;
                self.cmp4s(bus);
                self.clk(7 + 19 * count);
            }
            // ROL4: rotate nibbles left between AL and operand
            0x28 => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let al = self.regs.byte(ByteReg::AL);
                let new_al = ((al & 0x0F) << 4) | (tmp >> 4);
                let new_op = ((tmp & 0x0F) << 4) | (al & 0x0F);
                self.putback_rm_byte(modrm, new_op, bus);
                self.regs.set_byte(ByteReg::AL, new_al);
                self.clk_modrm(modrm, 25, 28);
            }
            // ROR4: rotate nibbles right between AL and operand
            0x2A => {
                let modrm = self.fetch(bus);
                let tmp = self.get_rm_byte(modrm, bus);
                let al = self.regs.byte(ByteReg::AL);
                let new_op = ((al & 0x0F) << 4) | (tmp >> 4);
                self.putback_rm_byte(modrm, new_op, bus);
                self.regs.set_byte(ByteReg::AL, tmp);
                self.clk_modrm(modrm, 29, 33);
            }
            // INS reg1, reg2
            0x31 => {
                self.ins_reg(bus);
            }
            // EXT reg1, reg2
            0x33 => {
                self.ext_reg(bus);
            }
            // INS reg, imm4
            0x39 => {
                self.ins_imm(bus);
            }
            // EXT reg, imm4
            0x3B => {
                self.ext_imm(bus);
            }
            // BRKEM
            0xFF => {
                let penalty = self.sp_penalty(3);
                let vector = self.fetch(bus);
                self.raise_interrupt(vector, bus);
                self.clk(38 + penalty);
            }
            _ => {
                // Unknown 0F sub-opcode
                self.clk(2);
            }
        }
    }

    fn add4s(&mut self, bus: &mut impl common::Bus) {
        let count = self.regs.byte(ByteReg::CL).div_ceil(2);
        let mut carry = 0u16;
        let mut zero = 0u32;
        let si_base = self.regs.word(WordReg::SI);
        let di_base = self.regs.word(WordReg::DI);
        let src_seg = self.default_base(SegReg16::DS);
        let dst_seg = self.seg_base(SegReg16::ES);
        for i in 0..count {
            let si = si_base.wrapping_add(i as u16);
            let di = di_base.wrapping_add(i as u16);
            let src_addr = src_seg.wrapping_add(si as u32) & 0xFFFFF;
            let dst_addr = dst_seg.wrapping_add(di as u32) & 0xFFFFF;
            let src = bus.read_byte(src_addr) as u16;
            let dst = bus.read_byte(dst_addr) as u16;
            let total = src + dst + carry;
            let old_al = (total & 0xFF) as u8;
            let old_cf = total > 0xFF;
            let old_af = (dst & 0xF) + (src & 0xF) + carry > 0xF;
            let mut al = old_al;
            if (old_al & 0x0F) > 9 || old_af {
                al = al.wrapping_add(6);
            }
            let threshold = if old_af { 0x9F } else { 0x99 };
            if old_al > threshold || old_cf {
                al = al.wrapping_add(0x60);
                carry = 1;
            } else {
                carry = 0;
            }
            bus.write_byte(dst_addr, al);
            if al != 0 {
                zero = 1;
            }
        }
        self.flags.carry_val = carry as u32;
        self.flags.zero_val = zero;
    }

    fn sub4s(&mut self, bus: &mut impl common::Bus) {
        let count = self.regs.byte(ByteReg::CL).div_ceil(2);
        let mut carry = 0i16;
        let mut zero = 0u32;
        let si_base = self.regs.word(WordReg::SI);
        let di_base = self.regs.word(WordReg::DI);
        let src_seg = self.default_base(SegReg16::DS);
        let dst_seg = self.seg_base(SegReg16::ES);
        for i in 0..count {
            let si = si_base.wrapping_add(i as u16);
            let di = di_base.wrapping_add(i as u16);
            let src_addr = src_seg.wrapping_add(si as u32) & 0xFFFFF;
            let dst_addr = dst_seg.wrapping_add(di as u32) & 0xFFFFF;
            let src = bus.read_byte(src_addr) as i16;
            let dst = bus.read_byte(dst_addr) as i16;
            let total = dst - src - carry;
            let old_al = (total & 0xFF) as u8;
            let old_cf = total < 0;
            let old_af = (dst & 0xF) - (src & 0xF) - carry < 0;
            let mut al = old_al;
            if (old_al & 0x0F) > 9 || old_af {
                al = al.wrapping_sub(6);
            }
            let threshold = if old_af { 0x9F } else { 0x99 };
            if old_al > threshold || old_cf {
                al = al.wrapping_sub(0x60);
                carry = 1;
            } else {
                carry = 0;
            }
            bus.write_byte(dst_addr, al);
            if al != 0 {
                zero = 1;
            }
        }
        self.flags.carry_val = carry as u32;
        self.flags.zero_val = zero;
    }

    fn cmp4s(&mut self, bus: &mut impl common::Bus) {
        let count = self.regs.byte(ByteReg::CL).div_ceil(2);
        let mut carry = 0i16;
        let mut zero = 0u32;
        let si_base = self.regs.word(WordReg::SI);
        let di_base = self.regs.word(WordReg::DI);
        let src_seg = self.default_base(SegReg16::DS);
        let dst_seg = self.seg_base(SegReg16::ES);
        for i in 0..count {
            let si = si_base.wrapping_add(i as u16);
            let di = di_base.wrapping_add(i as u16);
            let src_addr = src_seg.wrapping_add(si as u32) & 0xFFFFF;
            let dst_addr = dst_seg.wrapping_add(di as u32) & 0xFFFFF;
            let src = bus.read_byte(src_addr) as i16;
            let dst = bus.read_byte(dst_addr) as i16;
            let total = dst - src - carry;
            let old_al = (total & 0xFF) as u8;
            let old_cf = total < 0;
            let old_af = (dst & 0xF) - (src & 0xF) - carry < 0;
            let mut al = old_al;
            if (old_al & 0x0F) > 9 || old_af {
                al = al.wrapping_sub(6);
            }
            let threshold = if old_af { 0x9F } else { 0x99 };
            if old_al > threshold || old_cf {
                al = al.wrapping_sub(0x60);
                carry = 1;
            } else {
                carry = 0;
            }
            if al != 0 {
                zero = 1;
            }
        }
        self.flags.carry_val = carry as u32;
        self.flags.zero_val = zero;
    }

    fn ins_reg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(0xC0 | (modrm & 7), bus) & 0x0F;
        let bit_count = (self.get_rm_byte(0xC0 | ((modrm >> 3) & 7), bus) & 0x0F) + 1;
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        let iy = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let mask = ((1u32 << bit_count) - 1) as u16;
        let mut word = self.read_word_seg(bus, base, iy);
        word =
            (word & !(mask << bit_offset)) | ((self.regs.word(WordReg::AX) & mask) << bit_offset);
        self.write_word_seg(bus, base, iy, word);
        if total_bits > 16 {
            let iy2 = iy.wrapping_add(2);
            let overflow_bits = total_bits - 16;
            let overflow_mask = ((1u32 << overflow_bits) - 1) as u16;
            let mut word2 = self.read_word_seg(bus, base, iy2);
            word2 = (word2 & !overflow_mask)
                | ((self.regs.word(WordReg::AX) >> (16 - bit_offset)) & overflow_mask);
            self.write_word_seg(bus, base, iy2, word2);
        }
        if total_bits >= 16 {
            self.regs.set_word(WordReg::DI, iy.wrapping_add(2));
        }
        self.clk(113);
    }

    fn ext_reg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(0xC0 | (modrm & 7), bus) & 0x0F;
        let bit_count = (self.get_rm_byte(0xC0 | ((modrm >> 3) & 7), bus) & 0x0F) + 1;
        let ix = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let mut result = self.read_word_seg(bus, base, ix) >> bit_offset;
        if total_bits > 16 {
            result |= self.read_word_seg(bus, base, ix.wrapping_add(2)) << (16 - bit_offset);
        }
        let mask = ((1u32 << bit_count) - 1) as u16;
        self.regs.set_word(WordReg::AX, result & mask);
        if total_bits >= 16 {
            self.regs.set_word(WordReg::SI, ix.wrapping_add(2));
        }
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        self.clk(59);
    }

    fn ins_imm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(modrm, bus) & 0x0F;
        let bit_count = (self.fetch(bus) & 0x0F) + 1;
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        let iy = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let mask = ((1u32 << bit_count) - 1) as u16;
        let mut word = self.read_word_seg(bus, base, iy);
        word =
            (word & !(mask << bit_offset)) | ((self.regs.word(WordReg::AX) & mask) << bit_offset);
        self.write_word_seg(bus, base, iy, word);
        if total_bits > 16 {
            let iy2 = iy.wrapping_add(2);
            let overflow_bits = total_bits - 16;
            let overflow_mask = ((1u32 << overflow_bits) - 1) as u16;
            let mut word2 = self.read_word_seg(bus, base, iy2);
            word2 = (word2 & !overflow_mask)
                | ((self.regs.word(WordReg::AX) >> (16 - bit_offset)) & overflow_mask);
            self.write_word_seg(bus, base, iy2, word2);
        }
        if total_bits >= 16 {
            self.regs.set_word(WordReg::DI, iy.wrapping_add(2));
        }
        self.clk(103);
    }

    fn ext_imm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(modrm, bus) & 0x0F;
        let bit_count = (self.fetch(bus) & 0x0F) + 1;
        let ix = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let mut result = self.read_word_seg(bus, base, ix) >> bit_offset;
        if total_bits > 16 {
            result |= self.read_word_seg(bus, base, ix.wrapping_add(2)) << (16 - bit_offset);
        }
        let mask = ((1u32 << bit_count) - 1) as u16;
        self.regs.set_word(WordReg::AX, result & mask);
        if total_bits >= 16 {
            self.regs.set_word(WordReg::SI, ix.wrapping_add(2));
        }
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        self.clk(52);
    }
}
