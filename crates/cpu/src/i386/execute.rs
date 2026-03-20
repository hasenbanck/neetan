use super::{CPU_MODEL_386, CPU_MODEL_486, I386};
use crate::{ByteReg, DwordReg, SegReg32, WordReg};

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
    pub(super) fn dispatch(&mut self, opcode: u8, bus: &mut impl common::Bus) {
        if self.fault_pending {
            return;
        }
        match opcode {
            // ADD
            0x00 => self.add_br8(bus),
            0x01 => self.add_wr16(bus),
            0x02 => self.add_r8b(bus),
            0x03 => self.add_r16w(bus),
            0x04 => self.add_ald8(bus),
            0x05 => self.add_axd16(bus),
            0x06 => self.push_seg(SegReg32::ES, bus),
            0x07 => self.pop_seg(SegReg32::ES, bus),

            // OR
            0x08 => self.or_br8(bus),
            0x09 => self.or_wr16(bus),
            0x0A => self.or_r8b(bus),
            0x0B => self.or_r16w(bus),
            0x0C => self.or_ald8(bus),
            0x0D => self.or_axd16(bus),
            0x0E => self.push_seg(SegReg32::CS, bus),
            0x0F => self.extended_0f(bus),

            // ADC
            0x10 => self.adc_br8(bus),
            0x11 => self.adc_wr16(bus),
            0x12 => self.adc_r8b(bus),
            0x13 => self.adc_r16w(bus),
            0x14 => self.adc_ald8(bus),
            0x15 => self.adc_axd16(bus),
            0x16 => self.push_seg(SegReg32::SS, bus),
            0x17 => {
                self.pop_seg(SegReg32::SS, bus);
                self.inhibit_all = 1;
            }

            // SBB
            0x18 => self.sbb_br8(bus),
            0x19 => self.sbb_wr16(bus),
            0x1A => self.sbb_r8b(bus),
            0x1B => self.sbb_r16w(bus),
            0x1C => self.sbb_ald8(bus),
            0x1D => self.sbb_axd16(bus),
            0x1E => self.push_seg(SegReg32::DS, bus),
            0x1F => self.pop_seg(SegReg32::DS, bus),

            // AND
            0x20 => self.and_br8(bus),
            0x21 => self.and_wr16(bus),
            0x22 => self.and_r8b(bus),
            0x23 => self.and_r16w(bus),
            0x24 => self.and_ald8(bus),
            0x25 => self.and_axd16(bus),
            0x26 => self.invalid(bus),
            0x27 => self.daa(bus),

            // SUB
            0x28 => self.sub_br8(bus),
            0x29 => self.sub_wr16(bus),
            0x2A => self.sub_r8b(bus),
            0x2B => self.sub_r16w(bus),
            0x2C => self.sub_ald8(bus),
            0x2D => self.sub_axd16(bus),
            0x2E => self.invalid(bus),
            0x2F => self.das(bus),

            // XOR
            0x30 => self.xor_br8(bus),
            0x31 => self.xor_wr16(bus),
            0x32 => self.xor_r8b(bus),
            0x33 => self.xor_r16w(bus),
            0x34 => self.xor_ald8(bus),
            0x35 => self.xor_axd16(bus),
            0x36 => self.invalid(bus),
            0x37 => self.aaa(bus),

            // CMP
            0x38 => self.cmp_br8(bus),
            0x39 => self.cmp_wr16(bus),
            0x3A => self.cmp_r8b(bus),
            0x3B => self.cmp_r16w(bus),
            0x3C => self.cmp_ald8(bus),
            0x3D => self.cmp_axd16(bus),
            0x3E => self.invalid(bus),
            0x3F => self.aas(bus),

            // INC word registers
            0x40 => self.inc_word_reg(WordReg::AX),
            0x41 => self.inc_word_reg(WordReg::CX),
            0x42 => self.inc_word_reg(WordReg::DX),
            0x43 => self.inc_word_reg(WordReg::BX),
            0x44 => self.inc_word_reg(WordReg::SP),
            0x45 => self.inc_word_reg(WordReg::BP),
            0x46 => self.inc_word_reg(WordReg::SI),
            0x47 => self.inc_word_reg(WordReg::DI),

            // DEC word registers
            0x48 => self.dec_word_reg(WordReg::AX),
            0x49 => self.dec_word_reg(WordReg::CX),
            0x4A => self.dec_word_reg(WordReg::DX),
            0x4B => self.dec_word_reg(WordReg::BX),
            0x4C => self.dec_word_reg(WordReg::SP),
            0x4D => self.dec_word_reg(WordReg::BP),
            0x4E => self.dec_word_reg(WordReg::SI),
            0x4F => self.dec_word_reg(WordReg::DI),

            // PUSH word registers
            0x50 => self.push_word_reg(WordReg::AX, bus),
            0x51 => self.push_word_reg(WordReg::CX, bus),
            0x52 => self.push_word_reg(WordReg::DX, bus),
            0x53 => self.push_word_reg(WordReg::BX, bus),
            0x54 => self.push_sp(bus),
            0x55 => self.push_word_reg(WordReg::BP, bus),
            0x56 => self.push_word_reg(WordReg::SI, bus),
            0x57 => self.push_word_reg(WordReg::DI, bus),

            // POP word registers
            0x58 => self.pop_word_reg(WordReg::AX, bus),
            0x59 => self.pop_word_reg(WordReg::CX, bus),
            0x5A => self.pop_word_reg(WordReg::DX, bus),
            0x5B => self.pop_word_reg(WordReg::BX, bus),
            0x5C => self.pop_word_reg(WordReg::SP, bus),
            0x5D => self.pop_word_reg(WordReg::BP, bus),
            0x5E => self.pop_word_reg(WordReg::SI, bus),
            0x5F => self.pop_word_reg(WordReg::DI, bus),

            // 80186 instructions
            0x60 => self.pusha(bus),
            0x61 => self.popa(bus),
            0x62 => self.bound(bus),
            0x63 => self.arpl(bus),
            0x64 => self.invalid(bus),
            0x65 => self.invalid(bus),
            0x66 => self.invalid(bus),
            0x67 => self.invalid(bus),
            0x68 => self.push_imm16(bus),
            0x69 => self.imul_r16w_imm16(bus),
            0x6A => self.push_imm8(bus),
            0x6B => self.imul_r16w_imm8(bus),
            0x6C => self.insb(bus),
            0x6D => self.insw(bus),
            0x6E => self.outsb(bus),
            0x6F => self.outsw(bus),

            // Jcc (short jumps)
            0x70 => self.jcc(bus, self.flags.of()),
            0x71 => self.jcc(bus, !self.flags.of()),
            0x72 => self.jcc(bus, self.flags.cf()),
            0x73 => self.jcc(bus, !self.flags.cf()),
            0x74 => self.jcc(bus, self.flags.zf()),
            0x75 => self.jcc(bus, !self.flags.zf()),
            0x76 => self.jcc(bus, self.flags.cf() || self.flags.zf()),
            0x77 => self.jcc_swapped(bus, !self.flags.cf() && !self.flags.zf()),
            0x78 => self.jcc(bus, self.flags.sf()),
            0x79 => self.jcc(bus, !self.flags.sf()),
            0x7A => self.jcc(bus, self.flags.pf()),
            0x7B => self.jcc(bus, !self.flags.pf()),
            0x7C => self.jcc(bus, self.flags.sf() != self.flags.of()),
            0x7D => self.jcc_swapped(bus, self.flags.sf() == self.flags.of()),
            0x7E => self.jcc(bus, self.flags.zf() || (self.flags.sf() != self.flags.of())),
            0x7F => self.jcc_swapped(
                bus,
                !self.flags.zf() && (self.flags.sf() == self.flags.of()),
            ),

            // Group 1
            0x80 => self.group_80(bus),
            0x81 => self.group_81(bus),
            0x82 => self.group_82(bus),
            0x83 => self.group_83(bus),

            // TEST
            0x84 => self.test_br8(bus),
            0x85 => self.test_wr16(bus),

            // XCHG
            0x86 => self.xchg_br8(bus),
            0x87 => self.xchg_wr16(bus),

            // MOV r/m, reg
            0x88 => self.mov_br8(bus),
            0x89 => self.mov_wr16(bus),
            0x8A => self.mov_r8b(bus),
            0x8B => self.mov_r16w(bus),

            // MOV r/m, sreg / LEA / MOV sreg, r/m
            0x8C => self.mov_rm_sreg(bus),
            0x8D => self.lea(bus),
            0x8E => self.mov_sreg_rm(bus),
            0x8F => self.pop_rm(bus),

            // XCHG AX, reg / NOP
            0x90 => self.clk(Self::timing(3, 1)),
            0x91 => self.xchg_aw(WordReg::CX),
            0x92 => self.xchg_aw(WordReg::DX),
            0x93 => self.xchg_aw(WordReg::BX),
            0x94 => self.xchg_aw(WordReg::SP),
            0x95 => self.xchg_aw(WordReg::BP),
            0x96 => self.xchg_aw(WordReg::SI),
            0x97 => self.xchg_aw(WordReg::DI),

            // CBW, CWD
            0x98 => self.cbw(),
            0x99 => self.cwd(),

            // CALL far, WAIT
            0x9A => self.call_far(bus),
            0x9B => self.fpu_wait(bus),
            0x9C => self.pushf(bus),
            0x9D => self.popf(bus),
            0x9E => self.sahf(),
            0x9F => self.lahf(),

            // MOV AL/AX, [addr] and [addr], AL/AX
            0xA0 => self.mov_al_moffs(bus),
            0xA1 => self.mov_aw_moffs(bus),
            0xA2 => self.mov_moffs_al(bus),
            0xA3 => self.mov_moffs_aw(bus),

            // String ops
            0xA4 => self.movsb(bus),
            0xA5 => self.movsw(bus),
            0xA6 => self.cmpsb(bus),
            0xA7 => self.cmpsw(bus),

            // TEST AL/AX, imm
            0xA8 => self.test_al_imm8(bus),
            0xA9 => self.test_aw_imm16(bus),

            // STOS, LODS, SCAS
            0xAA => self.stosb(bus),
            0xAB => self.stosw(bus),
            0xAC => self.lodsb(bus),
            0xAD => self.lodsw(bus),
            0xAE => self.scasb(bus),
            0xAF => self.scasw(bus),

            // MOV byte reg, imm8
            0xB0 => self.mov_byte_reg_imm(ByteReg::AL, bus),
            0xB1 => self.mov_byte_reg_imm(ByteReg::CL, bus),
            0xB2 => self.mov_byte_reg_imm(ByteReg::DL, bus),
            0xB3 => self.mov_byte_reg_imm(ByteReg::BL, bus),
            0xB4 => self.mov_byte_reg_imm(ByteReg::AH, bus),
            0xB5 => self.mov_byte_reg_imm(ByteReg::CH, bus),
            0xB6 => self.mov_byte_reg_imm(ByteReg::DH, bus),
            0xB7 => self.mov_byte_reg_imm(ByteReg::BH, bus),

            // MOV word reg, imm16
            0xB8 => self.mov_word_reg_imm(WordReg::AX, bus),
            0xB9 => self.mov_word_reg_imm(WordReg::CX, bus),
            0xBA => self.mov_word_reg_imm(WordReg::DX, bus),
            0xBB => self.mov_word_reg_imm(WordReg::BX, bus),
            0xBC => self.mov_word_reg_imm(WordReg::SP, bus),
            0xBD => self.mov_word_reg_imm(WordReg::BP, bus),
            0xBE => self.mov_word_reg_imm(WordReg::SI, bus),
            0xBF => self.mov_word_reg_imm(WordReg::DI, bus),

            // Shift/rotate groups
            0xC0 => self.group_c0(bus),
            0xC1 => self.group_c1(bus),

            // RET near imm16, RET near
            0xC2 => self.ret_near_imm(bus),
            0xC3 => self.ret_near(bus),

            // LES, LDS
            0xC4 => self.les(bus),
            0xC5 => self.lds(bus),

            // MOV r/m, imm
            0xC6 => self.mov_rm_imm8(bus),
            0xC7 => self.mov_rm_imm16(bus),

            // ENTER, LEAVE
            0xC8 => self.enter(bus),
            0xC9 => self.leave(bus),

            // RET far imm16, RET far
            0xCA => self.ret_far_imm(bus),
            0xCB => self.ret_far(bus),

            // INT 3, INT imm8, INTO, IRET
            0xCC => self.int3(bus),
            0xCD => self.int_imm(bus),
            0xCE => self.into(bus),
            0xCF => self.iret(bus),

            // Shift/rotate groups
            0xD0 => self.group_d0(bus),
            0xD1 => self.group_d1(bus),
            0xD2 => self.group_d2(bus),
            0xD3 => self.group_d3(bus),

            // AAM, AAD
            0xD4 => self.aam(bus),
            0xD5 => self.aad(bus),

            // undocumented SALC
            0xD6 => self.salc(),

            // XLAT
            0xD7 => self.xlat(bus),

            // FPU escape
            0xD8..=0xDF => self.fpu_escape(opcode, bus),

            // LOOPNE, LOOPE, LOOP, JCXZ
            0xE0 => self.loopne(bus),
            0xE1 => self.loope(bus),
            0xE2 => self.loop_(bus),
            0xE3 => self.jcxz(bus),

            // IN, OUT
            0xE4 => self.in_al_imm(bus),
            0xE5 => self.in_aw_imm(bus),
            0xE6 => self.out_imm_al(bus),
            0xE7 => self.out_imm_aw(bus),

            // CALL near, JMP near, JMP far, JMP short
            0xE8 => self.call_near(bus),
            0xE9 => self.jmp_near(bus),
            0xEA => self.jmp_far(bus),
            0xEB => self.jmp_short(bus),

            // IN, OUT (DX port)
            0xEC => self.in_al_dw(bus),
            0xED => self.in_aw_dw(bus),
            0xEE => self.out_dw_al(bus),
            0xEF => self.out_dw_aw(bus),

            0xF0 => self.invalid(bus),
            0xF1 => self.invalid(bus),

            // REPNE, REPE
            0xF2 => self.repne(bus),
            0xF3 => self.repe(bus),

            // HLT
            0xF4 => self.hlt(bus),

            // CMC
            0xF5 => self.cmc(),

            // Group 3 byte/word
            0xF6 => self.group_f6(bus),
            0xF7 => self.group_f7(bus),

            // CLC, STC, CLI, STI, CLD, STD
            0xF8 => self.clc(),
            0xF9 => self.stc(),
            0xFA => self.cli(bus),
            0xFB => self.sti(bus),
            0xFC => self.cld(),
            0xFD => self.std(),

            // Group 4/5
            0xFE => self.group_fe(bus),
            0xFF => self.group_ff(bus),
        }
    }

    fn add_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_add_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn add_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_add_dword(dst, src);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_add_word(dst, src);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn add_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_add_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn add_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_add_dword(dst, src);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_add_word(dst, src);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn add_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_add_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn add_axd16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_add_dword(dst, src);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_add_word(dst, src);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn or_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn or_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_or_dword(dst, src);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_or_word(dst, src);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn or_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn or_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_or_dword(dst, src);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_or_word(dst, src);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn or_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_or_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn or_axd16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_or_dword(dst, src);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_or_word(dst, src);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn adc_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn adc_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let cf = self.flags.cf_val();
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_adc_dword(dst, src, cf);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_adc_word(dst, src, cf);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn adc_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn adc_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let cf = self.flags.cf_val();
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_adc_dword(dst, src, cf);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_adc_word(dst, src, cf);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn adc_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn adc_axd16(&mut self, bus: &mut impl common::Bus) {
        let cf = self.flags.cf_val();
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_adc_dword(dst, src, cf);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_adc_word(dst, src, cf);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn sbb_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn sbb_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let cf = self.flags.cf_val();
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_sbb_dword(dst, src, cf);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_sbb_word(dst, src, cf);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn sbb_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn sbb_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let cf = self.flags.cf_val();
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_sbb_dword(dst, src, cf);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_sbb_word(dst, src, cf);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn sbb_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn sbb_axd16(&mut self, bus: &mut impl common::Bus) {
        let cf = self.flags.cf_val();
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_sbb_dword(dst, src, cf);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_sbb_word(dst, src, cf);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn and_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn and_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_and_dword(dst, src);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_and_word(dst, src);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn and_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn and_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_and_dword(dst, src);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_and_word(dst, src);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn and_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_and_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn and_axd16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_and_dword(dst, src);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_and_word(dst, src);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn sub_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn sub_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_sub_dword(dst, src);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_sub_word(dst, src);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn sub_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn sub_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_sub_dword(dst, src);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_sub_word(dst, src);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn sub_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_sub_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn sub_axd16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_sub_dword(dst, src);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_sub_word(dst, src);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn xor_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(7, 3));
    }

    fn xor_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            let result = self.alu_xor_dword(dst, src);
            self.putback_rm_dword(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 4);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            let result = self.alu_xor_word(dst, src);
            self.putback_rm_word(modrm, result, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(7, 3), 2);
        }
    }

    fn xor_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn xor_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            let result = self.alu_xor_dword(dst, src);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            let result = self.alu_xor_word(dst, src);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn xor_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_xor_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(Self::timing(2, 1));
    }

    fn xor_axd16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            let result = self.alu_xor_dword(dst, src);
            self.regs.set_dword(DwordReg::EAX, result);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            let result = self.alu_xor_word(dst, src);
            self.regs.set_word(WordReg::AX, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn cmp_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(5, 2));
    }

    fn cmp_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            self.alu_sub_dword(dst, src);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(5, 2), 2);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            self.alu_sub_word(dst, src);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(5, 2), 1);
        }
    }

    fn cmp_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(6, 2));
    }

    fn cmp_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let dst = self.regs.dword(self.reg_dword(modrm));
            let src = self.get_rm_dword(modrm, bus);
            self.alu_sub_dword(dst, src);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 2);
        } else {
            let dst = self.regs.word(self.reg_word(modrm));
            let src = self.get_rm_word(modrm, bus);
            self.alu_sub_word(dst, src);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(6, 2), 1);
        }
    }

    fn cmp_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(dst, src);
        self.clk(Self::timing(2, 1));
    }

    fn cmp_axd16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            self.alu_sub_dword(dst, src);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            self.alu_sub_word(dst, src);
        }
        self.clk(Self::timing(2, 1));
    }

    fn inc_word_reg(&mut self, reg: WordReg) {
        if self.operand_size_override {
            let dreg = DwordReg::from_index(reg as u8);
            let val = self.regs.dword(dreg);
            let result = self.alu_inc_dword(val);
            self.regs.set_dword(dreg, result);
        } else {
            let val = self.regs.word(reg);
            let result = self.alu_inc_word(val);
            self.regs.set_word(reg, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn dec_word_reg(&mut self, reg: WordReg) {
        if self.operand_size_override {
            let dreg = DwordReg::from_index(reg as u8);
            let val = self.regs.dword(dreg);
            let result = self.alu_dec_dword(val);
            self.regs.set_dword(dreg, result);
        } else {
            let val = self.regs.word(reg);
            let result = self.alu_dec_word(val);
            self.regs.set_word(reg, result);
        }
        self.clk(Self::timing(2, 1));
    }

    fn push_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let dreg = DwordReg::from_index(reg as u8);
            let val = self.regs.dword(dreg);
            self.push_dword(bus, val);
        } else {
            let val = self.regs.word(reg);
            self.push(bus, val);
        }
        self.clk(Self::timing(2, 1) + penalty);
    }

    pub(super) fn push_sp(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let esp = self.regs.dword(DwordReg::ESP);
            self.push_dword(bus, esp);
        } else {
            let sp = self.regs.word(WordReg::SP);
            self.push(bus, sp);
        }
        self.clk(Self::timing(2, 1) + penalty);
    }

    fn pop_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let dreg = DwordReg::from_index(reg as u8);
            let val = self.pop_dword(bus);
            self.regs.set_dword(dreg, val);
        } else {
            let val = self.pop(bus);
            self.regs.set_word(reg, val);
        }
        self.clk(Self::timing(4, 4) + penalty);
    }

    pub(super) fn push_seg(&mut self, seg: SegReg32, bus: &mut impl common::Bus) {
        let val = self.sregs[seg as usize];
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            if self.use_esp() {
                let esp = self.regs.dword(DwordReg::ESP).wrapping_sub(4);
                self.regs.set_dword(DwordReg::ESP, esp);
                let base = self.seg_base(SegReg32::SS);
                let l0 = base.wrapping_add(esp);
                let Some(a0) = self.translate_linear(l0, true, bus) else {
                    return;
                };
                let Some(a1) = self.translate_linear(l0.wrapping_add(1), true, bus) else {
                    return;
                };
                let Some(a2) = self.translate_linear(l0.wrapping_add(2), true, bus) else {
                    return;
                };
                let Some(a3) = self.translate_linear(l0.wrapping_add(3), true, bus) else {
                    return;
                };
                bus.write_byte(a0, val as u8);
                bus.write_byte(a1, (val >> 8) as u8);
                bus.write_byte(a2, 0);
                bus.write_byte(a3, 0);
            } else {
                let sp = self.regs.word(WordReg::SP).wrapping_sub(4);
                self.regs.set_word(WordReg::SP, sp);
                let base = self.seg_base(SegReg32::SS);
                let l0 = base.wrapping_add(sp as u32);
                let Some(a0) = self.translate_linear(l0, true, bus) else {
                    return;
                };
                let Some(a1) = self.translate_linear(l0.wrapping_add(1), true, bus) else {
                    return;
                };
                let Some(a2) = self.translate_linear(l0.wrapping_add(2), true, bus) else {
                    return;
                };
                let Some(a3) = self.translate_linear(l0.wrapping_add(3), true, bus) else {
                    return;
                };
                bus.write_byte(a0, val as u8);
                bus.write_byte(a1, (val >> 8) as u8);
                bus.write_byte(a2, 0);
                bus.write_byte(a3, 0);
            }
        } else {
            self.push(bus, val);
        }
        self.clk(Self::timing(2, 3) + penalty);
    }

    pub(super) fn pop_seg(&mut self, seg: SegReg32, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        let val = if self.operand_size_override {
            if self.use_esp() {
                let esp = self.regs.dword(DwordReg::ESP);
                let base = self.seg_base(SegReg32::SS);
                let l0 = base.wrapping_add(esp);
                let a0 = self.translate_linear(l0, false, bus).unwrap_or(0);
                let a1 = self
                    .translate_linear(l0.wrapping_add(1), false, bus)
                    .unwrap_or(0);
                let low = bus.read_byte(a0) as u16;
                let high = bus.read_byte(a1) as u16;
                self.regs.set_dword(DwordReg::ESP, esp.wrapping_add(4));
                low | (high << 8)
            } else {
                let sp = self.regs.word(WordReg::SP);
                let base = self.seg_base(SegReg32::SS);
                let l0 = base.wrapping_add(sp as u32);
                let a0 = self.translate_linear(l0, false, bus).unwrap_or(0);
                let a1 = self
                    .translate_linear(l0.wrapping_add(1), false, bus)
                    .unwrap_or(0);
                let low = bus.read_byte(a0) as u16;
                let high = bus.read_byte(a1) as u16;
                self.regs.set_word(WordReg::SP, sp.wrapping_add(4));
                low | (high << 8)
            }
        } else {
            self.pop(bus)
        };
        if !self.load_segment(seg, val, bus) {
            return;
        }
        self.clk(Self::timing(7, 3) + penalty);
    }

    fn pusha(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let esp = self.regs.dword(DwordReg::ESP);
            self.push_dword(bus, self.regs.dword(DwordReg::EAX));
            self.push_dword(bus, self.regs.dword(DwordReg::ECX));
            self.push_dword(bus, self.regs.dword(DwordReg::EDX));
            self.push_dword(bus, self.regs.dword(DwordReg::EBX));
            self.push_dword(bus, esp);
            self.push_dword(bus, self.regs.dword(DwordReg::EBP));
            self.push_dword(bus, self.regs.dword(DwordReg::ESI));
            self.push_dword(bus, self.regs.dword(DwordReg::EDI));
        } else {
            let sp = self.regs.word(WordReg::SP);
            let aw = self.regs.word(WordReg::AX);
            self.push(bus, aw);
            let cw = self.regs.word(WordReg::CX);
            self.push(bus, cw);
            let dw = self.regs.word(WordReg::DX);
            self.push(bus, dw);
            let bw = self.regs.word(WordReg::BX);
            self.push(bus, bw);
            self.push(bus, sp);
            let bp = self.regs.word(WordReg::BP);
            self.push(bus, bp);
            let ix = self.regs.word(WordReg::SI);
            self.push(bus, ix);
            let iy = self.regs.word(WordReg::DI);
            self.push(bus, iy);
        }
        self.clk(Self::timing(18, 11) + penalty);
    }

    fn popa(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let edi = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::EDI, edi);
            let esi = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::ESI, esi);
            let ebp = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::EBP, ebp);
            let popped_esp = self.pop_dword(bus);
            let ebx = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::EBX, ebx);
            let edx = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::EDX, edx);
            let ecx = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::ECX, ecx);
            let eax = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::EAX, eax);
            let esp_low = self.regs.word(WordReg::SP) as u32;
            let esp = (popped_esp & 0xFFFF_0000) | esp_low;
            self.regs.set_dword(DwordReg::ESP, esp);
        } else {
            let iy = self.pop(bus);
            self.regs.set_word(WordReg::DI, iy);
            let ix = self.pop(bus);
            self.regs.set_word(WordReg::SI, ix);
            let bp = self.pop(bus);
            self.regs.set_word(WordReg::BP, bp);
            let _discard = self.pop(bus);
            let bw = self.pop(bus);
            self.regs.set_word(WordReg::BX, bw);
            let dw = self.pop(bus);
            self.regs.set_word(WordReg::DX, dw);
            let cw = self.pop(bus);
            self.regs.set_word(WordReg::CX, cw);
            let aw = self.pop(bus);
            self.regs.set_word(WordReg::AX, aw);
        }
        self.clk(Self::timing(24, 9) + penalty);
    }

    fn bound(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            return;
        }
        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let val = self.regs.dword(self.reg_dword(modrm)) as i32;
            let ea_pen = if self.ea & 3 != 0 { 8 } else { 0 };
            let low = self.seg_read_dword(bus) as i32;
            let high = self.seg_read_dword_at(bus, 4) as i32;
            if val < low || val > high {
                let sp_pen = self.sp_penalty();
                self.raise_interrupt(5, bus);
                self.clk(Self::timing(56, 7) + ea_pen + sp_pen);
            } else {
                self.clk(Self::timing(10, 7) + ea_pen);
            }
        } else {
            let val = self.regs.word(self.reg_word(modrm)) as i16;
            let ea_pen = if self.ea & 1 == 1 { 8 } else { 0 };
            let low = self.seg_read_word(bus) as i16;
            let high = self.seg_read_word_at(bus, 2) as i16;
            if val < low || val > high {
                let sp_pen = self.sp_penalty();
                self.raise_interrupt(5, bus);
                self.clk(Self::timing(56, 7) + ea_pen + sp_pen);
            } else {
                self.clk(Self::timing(10, 7) + ea_pen);
            }
        }
    }

    fn arpl(&mut self, bus: &mut impl common::Bus) {
        if !self.is_protected_mode() {
            self.raise_fault(6, bus);
            return;
        }

        let modrm = self.fetch(bus);
        let dst = self.get_rm_word(modrm, bus);
        let src_rpl = self.regs.word(self.reg_word(modrm)) & 3;
        let dst_rpl = dst & 3;
        if dst_rpl < src_rpl {
            let result = (dst & !3) | src_rpl;
            self.putback_rm_word(modrm, result, bus);
            self.flags.zero_val = 0; // ZF=1
        } else {
            self.flags.zero_val = 1; // ZF=0
        }
        self.clk_modrm(modrm, Self::timing(10, 9), Self::timing(11, 9));
    }

    fn push_imm16(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let val = self.fetchdword(bus);
            self.push_dword(bus, val);
        } else {
            let val = self.fetchword(bus);
            self.push(bus, val);
        }
        self.clk(Self::timing(2, 1) + penalty);
    }

    fn push_imm8(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let val = self.fetch(bus) as i8 as i32 as u32;
            self.push_dword(bus, val);
        } else {
            let val = self.fetch(bus) as i8 as u16;
            self.push(bus, val);
        }
        self.clk(Self::timing(2, 1) + penalty);
    }

    fn imul_r16w_imm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.get_rm_dword(modrm, bus) as i32 as i64;
            let imm = self.fetchdword(bus) as i32 as i64;
            let result = src * imm;
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result as u32);
            self.flags.carry_val = if result < i32::MIN as i64 || result > i32::MAX as i64 {
                1
            } else {
                0
            };
        } else {
            let src = self.get_rm_word(modrm, bus) as i16 as i32;
            let imm = self.fetchword(bus) as i16 as i32;
            let result = src * imm;
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result as u16);
            self.flags.carry_val = if !(-0x8000..=0x7FFF).contains(&result) {
                1
            } else {
                0
            };
        };
        self.flags.overflow_val = self.flags.carry_val;
        if self.operand_size_override {
            self.clk_modrm_word(modrm, Self::timing(38, 13), Self::timing(41, 13), 2);
        } else {
            self.clk_modrm_word(modrm, Self::timing(22, 13), Self::timing(25, 13), 1);
        }
    }

    fn imul_r16w_imm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.get_rm_dword(modrm, bus) as i32 as i64;
            let imm = self.fetch(bus) as i8 as i64;
            let result = src * imm;
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, result as u32);
            self.flags.carry_val = if result < i32::MIN as i64 || result > i32::MAX as i64 {
                1
            } else {
                0
            };
        } else {
            let src = self.get_rm_word(modrm, bus) as i16 as i32;
            let imm = self.fetch(bus) as i8 as i32;
            let result = src * imm;
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, result as u16);
            self.flags.carry_val = if !(-0x8000..=0x7FFF).contains(&result) {
                1
            } else {
                0
            };
        };
        self.flags.overflow_val = self.flags.carry_val;
        if self.operand_size_override {
            self.clk_modrm_word(modrm, Self::timing(14, 13), Self::timing(17, 13), 2);
        } else {
            self.clk_modrm_word(modrm, Self::timing(14, 13), Self::timing(17, 13), 1);
        }
    }

    fn jcc(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.apply_branch_disp8(disp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(7 + m);
                }
                CPU_MODEL_486 => self.clk(3),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            self.clk(Self::timing(3, 1));
        }
    }

    fn jcc_swapped(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.apply_branch_disp8(disp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(7 + m);
                }
                CPU_MODEL_486 => self.clk(3),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            self.clk(Self::timing(3, 1));
        }
    }

    fn test_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_and_byte(dst, src);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(5, 2));
    }

    fn test_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let src = self.regs.dword(self.reg_dword(modrm));
            let dst = self.get_rm_dword(modrm, bus);
            self.alu_and_dword(dst, src);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(5, 2), 2);
        } else {
            let src = self.regs.word(self.reg_word(modrm));
            let dst = self.get_rm_word(modrm, bus);
            self.alu_and_word(dst, src);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(5, 2), 1);
        }
    }

    fn test_al_imm8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_and_byte(dst, src);
        self.clk(Self::timing(2, 1));
    }

    fn test_aw_imm16(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let src = self.fetchdword(bus);
            let dst = self.regs.dword(DwordReg::EAX);
            self.alu_and_dword(dst, src);
        } else {
            let src = self.fetchword(bus);
            let dst = self.regs.word(WordReg::AX);
            self.alu_and_word(dst, src);
        }
        self.clk(Self::timing(2, 1));
    }

    fn xchg_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let reg = self.reg_byte(modrm);
        let reg_val = self.regs.byte(reg);
        let rm_val = self.get_rm_byte(modrm, bus);
        self.regs.set_byte(reg, rm_val);
        self.putback_rm_byte(modrm, reg_val, bus);
        self.clk_modrm(modrm, Self::timing(3, 3), Self::timing(5, 5));
    }

    fn xchg_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let reg = self.reg_dword(modrm);
            let reg_val = self.regs.dword(reg);
            let rm_val = self.get_rm_dword(modrm, bus);
            self.regs.set_dword(reg, rm_val);
            self.putback_rm_dword(modrm, reg_val, bus);
            self.clk_modrm_word(modrm, Self::timing(3, 3), Self::timing(5, 5), 4);
        } else {
            let reg = self.reg_word(modrm);
            let reg_val = self.regs.word(reg);
            let rm_val = self.get_rm_word(modrm, bus);
            self.regs.set_word(reg, rm_val);
            self.putback_rm_word(modrm, reg_val, bus);
            self.clk_modrm_word(modrm, Self::timing(3, 3), Self::timing(5, 5), 2);
        }
    }

    fn xchg_aw(&mut self, reg: WordReg) {
        if self.operand_size_override {
            let dreg = DwordReg::from_index(reg as u8);
            let eax = self.regs.dword(DwordReg::EAX);
            let val = self.regs.dword(dreg);
            self.regs.set_dword(DwordReg::EAX, val);
            self.regs.set_dword(dreg, eax);
        } else {
            let aw = self.regs.word(WordReg::AX);
            let val = self.regs.word(reg);
            self.regs.set_word(WordReg::AX, val);
            self.regs.set_word(reg, aw);
        }
        self.clk(Self::timing(3, 3));
    }

    fn mov_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.regs.byte(self.reg_byte(modrm));
        self.put_rm_byte(modrm, val, bus);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(2, 1));
    }

    fn mov_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let val = self.regs.dword(self.reg_dword(modrm));
            self.put_rm_dword(modrm, val, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(2, 1), 2);
        } else {
            let val = self.regs.word(self.reg_word(modrm));
            self.put_rm_word(modrm, val, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(2, 1), 1);
        }
    }

    fn mov_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_byte(modrm, bus);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, val);
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(4, 1));
    }

    fn mov_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            let val = self.get_rm_dword(modrm, bus);
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, val);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(4, 1), 2);
        } else {
            let val = self.get_rm_word(modrm, bus);
            let reg = self.reg_word(modrm);
            self.regs.set_word(reg, val);
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(4, 1), 1);
        }
    }

    fn mov_rm_sreg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let seg_index = (modrm >> 3) & 7;
        if seg_index > 5 {
            self.raise_fault(6, bus);
            return;
        }
        let seg = SegReg32::from_index(seg_index);
        let val = self.sregs[seg as usize];
        if self.operand_size_override && modrm >= 0xC0 {
            let reg = self.rm_dword(modrm);
            self.regs.set_dword(reg, val as u32);
            self.clk_modrm_word(modrm, Self::timing(2, 3), Self::timing(2, 3), 2);
        } else {
            self.put_rm_word(modrm, val, bus);
            self.clk_modrm_word(modrm, Self::timing(2, 3), Self::timing(2, 3), 1);
        }
    }

    fn mov_sreg_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let seg_index = (modrm >> 3) & 7;
        if seg_index > 5 || seg_index == 1 {
            self.raise_fault(6, bus);
            return;
        }
        let val = if self.operand_size_override && modrm >= 0xC0 {
            self.get_rm_dword(modrm, bus) as u16
        } else {
            self.get_rm_word(modrm, bus)
        };
        let seg = SegReg32::from_index(seg_index);
        if !self.load_segment(seg, val, bus) {
            return;
        }
        if seg == SegReg32::SS {
            self.inhibit_all = 1;
        }
        self.clk_modrm_word(modrm, Self::timing(2, 3), Self::timing(5, 9), 1);
    }

    fn lea(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            self.invalid(bus);
            return;
        }
        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let reg = self.reg_dword(modrm);
            let val = self.eo32;
            self.regs.set_dword(reg, val);
        } else {
            let reg = self.reg_word(modrm);
            let val = self.eo;
            self.regs.set_word(reg, val);
        }
        self.clk(Self::timing(2, 1));
    }

    fn pop_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let sp_pen = self.sp_penalty();
        if self.operand_size_override {
            let val = self.pop_dword(bus);
            self.put_rm_dword(modrm, val, bus);
        } else {
            let val = self.pop(bus);
            self.put_rm_word(modrm, val, bus);
        }
        if modrm >= 0xC0 {
            self.clk(Self::timing(4, 4) + sp_pen);
        } else {
            let ea_pen = if self.operand_size_override {
                if self.ea & 3 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                }
            } else if self.ea & 1 != 0 {
                Self::timing(4, 3)
            } else {
                0
            };
            self.clk(Self::timing(5, 5) + sp_pen + ea_pen);
        }
    }

    fn cbw(&mut self) {
        if self.operand_size_override {
            let ax = self.regs.word(WordReg::AX) as i16 as i32 as u32;
            self.regs.set_dword(DwordReg::EAX, ax);
        } else {
            let al = self.regs.byte(ByteReg::AL) as i8 as i16 as u16;
            self.regs.set_word(WordReg::AX, al);
        }
        self.clk(Self::timing(3, 3));
    }

    fn cwd(&mut self) {
        if self.operand_size_override {
            let eax = self.regs.dword(DwordReg::EAX) as i32;
            self.regs
                .set_dword(DwordReg::EDX, if eax < 0 { 0xFFFF_FFFF } else { 0 });
        } else {
            let aw = self.regs.word(WordReg::AX) as i16;
            self.regs
                .set_word(WordReg::DX, if aw < 0 { 0xFFFF } else { 0 });
        }
        self.clk(Self::timing(2, 3));
    }

    fn call_far(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let penalty = self.sp_penalty();
            let offset = self.fetchdword(bus);
            let segment = self.fetchword(bus);
            let cs = self.sregs[SegReg32::CS as usize];
            let eip = self.ip_upper | self.ip as u32;
            if self.is_protected_mode() && !self.is_virtual_mode() {
                self.code_descriptor(segment, offset, super::TaskType::Call, cs, eip, bus);
            } else {
                self.push_dword(bus, cs as u32);
                self.push_dword(bus, eip);
                if !self.load_segment(SegReg32::CS, segment, bus) {
                    return;
                }
                self.ip = offset as u16;
                self.ip_upper = offset & 0xFFFF_0000;
            }
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(17 + m + penalty);
                }
                CPU_MODEL_486 => self.clk(18 + penalty),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            let penalty = self.sp_penalty();
            let offset = self.fetchword(bus);
            let segment = self.fetchword(bus);
            let cs = self.sregs[SegReg32::CS as usize];
            let ip = self.ip;
            if self.is_protected_mode() && !self.is_virtual_mode() {
                self.code_descriptor(
                    segment,
                    offset as u32,
                    super::TaskType::Call,
                    cs,
                    ip as u32,
                    bus,
                );
            } else {
                self.push(bus, cs);
                self.push(bus, ip);
                if !self.load_segment(SegReg32::CS, segment, bus) {
                    return;
                }
                self.ip = offset;
                self.ip_upper = 0;
            }
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(17 + m + penalty);
                }
                CPU_MODEL_486 => self.clk(18 + penalty),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        }
    }

    fn call_near(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let penalty = self.sp_penalty();
            let disp = self.fetchdword(bus) as i32;
            let return_eip = self.ip_upper | self.ip as u32;
            self.push_dword(bus, return_eip);
            let target = return_eip.wrapping_add(disp as u32);
            self.ip = target as u16;
            self.ip_upper = target & 0xFFFF_0000;
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(7 + m + penalty);
                }
                CPU_MODEL_486 => self.clk(3 + penalty),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            let penalty = self.sp_penalty();
            let disp = self.fetchword(bus);
            self.push(bus, self.ip);
            self.ip = self.ip.wrapping_add(disp);
            self.ip_upper = 0;
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(7 + m + penalty);
                }
                CPU_MODEL_486 => self.clk(3 + penalty),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        }
    }

    fn jmp_near(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let disp = self.fetchdword(bus) as i32;
            let eip = self.ip_upper | self.ip as u32;
            let target = eip.wrapping_add(disp as u32);
            self.ip = target as u16;
            self.ip_upper = target & 0xFFFF_0000;
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(7 + m);
                }
                CPU_MODEL_486 => self.clk(3),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            let disp = self.fetchword(bus);
            self.ip = self.ip.wrapping_add(disp);
            self.ip_upper = 0;
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(7 + m);
                }
                CPU_MODEL_486 => self.clk(3),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        }
    }

    fn jmp_far(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let offset = self.fetchdword(bus);
            let segment = self.fetchword(bus);
            if self.is_protected_mode() && !self.is_virtual_mode() {
                self.code_descriptor(segment, offset, super::TaskType::Jmp, 0, 0, bus);
            } else {
                if !self.load_segment(SegReg32::CS, segment, bus) {
                    return;
                }
                self.ip = offset as u16;
                self.ip_upper = offset & 0xFFFF_0000;
            }
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(12 + m);
                }
                CPU_MODEL_486 => self.clk(17),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            let offset = self.fetchword(bus);
            let segment = self.fetchword(bus);
            if self.is_protected_mode() && !self.is_virtual_mode() {
                self.code_descriptor(segment, offset as u32, super::TaskType::Jmp, 0, 0, bus);
            } else {
                if !self.load_segment(SegReg32::CS, segment, bus) {
                    return;
                }
                self.ip = offset;
                self.ip_upper = 0;
            }
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(12 + m);
                }
                CPU_MODEL_486 => self.clk(17),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        }
    }

    fn jmp_short(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8;
        self.apply_branch_disp8(disp);
        match CPU_MODEL {
            CPU_MODEL_386 => {
                let m = self.next_instruction_length_approx(bus);
                self.clk(7 + m);
            }
            CPU_MODEL_486 => self.clk(3),
            _ => {
                unreachable!("Unhandled CPU_MODEL")
            }
        }
    }

    fn ret_near(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            let eip = self.pop_dword(bus);
            self.ip = eip as u16;
            self.ip_upper = eip & 0xFFFF_0000;
        } else {
            self.ip = self.pop(bus);
            self.ip_upper = 0;
        }
        match CPU_MODEL {
            CPU_MODEL_386 => {
                let m = self.next_instruction_length_approx(bus);
                self.clk(10 + m + penalty);
            }
            CPU_MODEL_486 => self.clk(5 + penalty),
            _ => {
                unreachable!("Unhandled CPU_MODEL")
            }
        }
    }

    fn ret_near_imm(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        let imm = self.fetchword(bus);
        if self.operand_size_override {
            let eip = self.pop_dword(bus);
            self.ip = eip as u16;
            self.ip_upper = eip & 0xFFFF_0000;
        } else {
            self.ip = self.pop(bus);
            self.ip_upper = 0;
        }
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        match CPU_MODEL {
            CPU_MODEL_386 => {
                let m = self.next_instruction_length_approx(bus);
                self.clk(10 + m + penalty);
            }
            CPU_MODEL_486 => self.clk(5 + penalty),
            _ => {
                unreachable!("Unhandled CPU_MODEL")
            }
        }
    }

    fn ret_far(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();

        if !self.is_protected_mode() || self.is_virtual_mode() {
            if self.operand_size_override {
                let eip = self.pop_dword(bus);
                let cs = self.pop_dword(bus) as u16;
                if !self.load_segment(SegReg32::CS, cs, bus) {
                    return;
                }
                self.ip = eip as u16;
                self.ip_upper = eip & 0xFFFF_0000;
            } else {
                let ip = self.pop(bus);
                let cs = self.pop(bus);
                if !self.load_segment(SegReg32::CS, cs, bus) {
                    return;
                }
                self.ip = ip;
                self.ip_upper = 0;
            }
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(18 + m + penalty);
                }
                CPU_MODEL_486 => self.clk(13 + penalty),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
            return;
        }

        // Protected mode far return.
        let sp = if self.use_esp() {
            self.regs.dword(DwordReg::ESP)
        } else {
            self.regs.word(WordReg::SP) as u32
        };
        let ss_base = self.seg_base(SegReg32::SS);

        if self.operand_size_override {
            let new_eip = self.read_dword_linear(bus, ss_base.wrapping_add(sp));
            let new_cs =
                self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(4))) as u16;

            let new_rpl = new_cs & 3;
            let old_cpl = self.cpl();

            if new_rpl > old_cpl {
                let new_esp = self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(8)));
                let new_ss =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(12))) as u16;

                let saved_cs = self.save_cs_state();
                if !self.load_cs_for_return(new_cs, new_eip, bus) {
                    return;
                }
                self.ip = new_eip as u16;
                self.ip_upper = new_eip & 0xFFFF_0000;

                if !self.load_segment(SegReg32::SS, new_ss, bus) {
                    self.restore_cs_state(&saved_cs);
                    return;
                }
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, new_esp);
                } else {
                    self.regs.set_word(WordReg::SP, new_esp as u16);
                }

                let new_cpl = self.cpl();
                self.invalidate_segment_if_needed(SegReg32::DS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::ES, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::FS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::GS, new_cpl);
            } else {
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, sp.wrapping_add(8));
                } else {
                    self.regs.set_word(WordReg::SP, sp.wrapping_add(8) as u16);
                }
                if !self.load_cs_for_return(new_cs, new_eip, bus) {
                    return;
                }
                self.ip = new_eip as u16;
                self.ip_upper = new_eip & 0xFFFF_0000;
            }
        } else {
            let new_ip = self.read_word_linear(bus, ss_base.wrapping_add(sp));
            let new_cs = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(2)));

            let new_rpl = new_cs & 3;
            let old_cpl = self.cpl();

            if new_rpl > old_cpl {
                let new_sp = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(4)));
                let new_ss = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(6)));

                let saved_cs = self.save_cs_state();
                if !self.load_cs_for_return(new_cs, new_ip as u32, bus) {
                    return;
                }
                self.ip = new_ip;
                self.ip_upper = 0;

                if !self.load_segment(SegReg32::SS, new_ss, bus) {
                    self.restore_cs_state(&saved_cs);
                    return;
                }
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, new_sp as u32);
                } else {
                    self.regs.set_word(WordReg::SP, new_sp);
                }

                let new_cpl = self.cpl();
                self.invalidate_segment_if_needed(SegReg32::DS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::ES, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::FS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::GS, new_cpl);
            } else {
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, sp.wrapping_add(4));
                } else {
                    self.regs.set_word(WordReg::SP, sp.wrapping_add(4) as u16);
                }
                if !self.load_cs_for_return(new_cs, new_ip as u32, bus) {
                    return;
                }
                self.ip = new_ip;
                self.ip_upper = 0;
            }
        }

        match CPU_MODEL {
            CPU_MODEL_386 => {
                let m = self.next_instruction_length_approx(bus);
                self.clk(18 + m + penalty);
            }
            CPU_MODEL_486 => self.clk(13 + penalty),
            _ => {
                unreachable!("Unhandled CPU_MODEL")
            }
        }
    }

    fn ret_far_imm(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        let imm = self.fetchword(bus);

        if !self.is_protected_mode() || self.is_virtual_mode() {
            if self.operand_size_override {
                let eip = self.pop_dword(bus);
                let cs = self.pop_dword(bus) as u16;
                if !self.load_segment(SegReg32::CS, cs, bus) {
                    return;
                }
                self.ip = eip as u16;
                self.ip_upper = eip & 0xFFFF_0000;
            } else {
                let ip = self.pop(bus);
                let cs = self.pop(bus);
                if !self.load_segment(SegReg32::CS, cs, bus) {
                    return;
                }
                self.ip = ip;
                self.ip_upper = 0;
            }
            let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
            self.regs.set_word(WordReg::SP, sp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(18 + m + penalty);
                }
                CPU_MODEL_486 => self.clk(14 + penalty),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
            return;
        }

        // Protected mode far return with immediate.
        let sp = if self.use_esp() {
            self.regs.dword(DwordReg::ESP)
        } else {
            self.regs.word(WordReg::SP) as u32
        };
        let ss_base = self.seg_base(SegReg32::SS);
        let imm32 = imm as u32;

        if self.operand_size_override {
            let new_eip = self.read_dword_linear(bus, ss_base.wrapping_add(sp));
            let new_cs =
                self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(4))) as u16;

            let new_rpl = new_cs & 3;
            let old_cpl = self.cpl();

            if new_rpl > old_cpl {
                let sp_ss_base = sp.wrapping_add(8).wrapping_add(imm32);
                let new_esp = self.read_dword_linear(bus, ss_base.wrapping_add(sp_ss_base));
                let new_ss = self
                    .read_dword_linear(bus, ss_base.wrapping_add(sp_ss_base.wrapping_add(4)))
                    as u16;

                let saved_cs = self.save_cs_state();
                if !self.load_cs_for_return(new_cs, new_eip, bus) {
                    return;
                }
                self.ip = new_eip as u16;
                self.ip_upper = new_eip & 0xFFFF_0000;

                if !self.load_segment(SegReg32::SS, new_ss, bus) {
                    self.restore_cs_state(&saved_cs);
                    return;
                }
                let adj_esp = new_esp.wrapping_add(imm32);
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, adj_esp);
                } else {
                    self.regs.set_word(WordReg::SP, adj_esp as u16);
                }

                let new_cpl = self.cpl();
                self.invalidate_segment_if_needed(SegReg32::DS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::ES, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::FS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::GS, new_cpl);
            } else {
                let new_sp_val = sp.wrapping_add(8).wrapping_add(imm32);
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, new_sp_val);
                } else {
                    self.regs.set_word(WordReg::SP, new_sp_val as u16);
                }
                if !self.load_cs_for_return(new_cs, new_eip, bus) {
                    return;
                }
                self.ip = new_eip as u16;
                self.ip_upper = new_eip & 0xFFFF_0000;
            }
        } else {
            let new_ip = self.read_word_linear(bus, ss_base.wrapping_add(sp));
            let new_cs = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(2)));

            let new_rpl = new_cs & 3;
            let old_cpl = self.cpl();

            if new_rpl > old_cpl {
                let sp_ss_base = sp.wrapping_add(4).wrapping_add(imm32);
                let new_sp = self.read_word_linear(bus, ss_base.wrapping_add(sp_ss_base));
                let new_ss =
                    self.read_word_linear(bus, ss_base.wrapping_add(sp_ss_base.wrapping_add(2)));

                let saved_cs = self.save_cs_state();
                if !self.load_cs_for_return(new_cs, new_ip as u32, bus) {
                    return;
                }
                self.ip = new_ip;
                self.ip_upper = 0;

                if !self.load_segment(SegReg32::SS, new_ss, bus) {
                    self.restore_cs_state(&saved_cs);
                    return;
                }
                let adj_sp = new_sp.wrapping_add(imm);
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, adj_sp as u32);
                } else {
                    self.regs.set_word(WordReg::SP, adj_sp);
                }

                let new_cpl = self.cpl();
                self.invalidate_segment_if_needed(SegReg32::DS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::ES, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::FS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::GS, new_cpl);
            } else {
                let new_sp_val = sp.wrapping_add(4).wrapping_add(imm32);
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, new_sp_val);
                } else {
                    self.regs.set_word(WordReg::SP, new_sp_val as u16);
                }
                if !self.load_cs_for_return(new_cs, new_ip as u32, bus) {
                    return;
                }
                self.ip = new_ip;
                self.ip_upper = 0;
            }
        }

        match CPU_MODEL {
            CPU_MODEL_386 => {
                let m = self.next_instruction_length_approx(bus);
                self.clk(18 + m + penalty);
            }
            CPU_MODEL_486 => self.clk(14 + penalty),
            _ => {
                unreachable!("Unhandled CPU_MODEL")
            }
        }
    }

    fn pushf(&mut self, bus: &mut impl common::Bus) {
        if self.is_virtual_mode() && self.flags.iopl < 3 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        let penalty = self.sp_penalty();
        if self.operand_size_override {
            // PUSHFD: bits 18-31 always push as 0, RF (bit 16) cleared.
            // Only VM (bit 17) is included from upper bits.
            let flags_val = (self.eflags_upper & 0x0002_0000) | self.flags.compress() as u32;
            self.push_dword(bus, flags_val);
        } else {
            let flags_val = self.flags.compress();
            self.push(bus, flags_val);
        }
        let base = match CPU_MODEL {
            CPU_MODEL_386 => 4,
            CPU_MODEL_486 => {
                if self.is_protected_mode() && !self.is_virtual_mode() {
                    3
                } else {
                    4
                }
            }
            _ => unreachable!("Unhandled CPU_MODEL"),
        };
        self.clk(base + penalty);
    }

    fn popf(&mut self, bus: &mut impl common::Bus) {
        if self.is_virtual_mode() && self.flags.iopl < 3 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        let penalty = self.sp_penalty();
        let cpl = self.cpl();
        let pm = self.is_protected_mode();
        if self.operand_size_override {
            let val = self.pop_dword(bus);
            self.flags.load_flags(val as u16, cpl, pm);
            // Bits 18-31 are reserved and not writable via POPFD.
            // VM (bit 17) is not modifiable via POPFD (only IRET at CPL=0).
            // RF (bit 16) is always cleared by POPFD.
            self.eflags_upper &= !0x0001_0000;
        } else {
            let val = self.pop(bus);
            self.flags.load_flags(val, cpl, pm);
        }
        let base = match CPU_MODEL {
            CPU_MODEL_386 => 5,
            CPU_MODEL_486 => {
                if self.is_protected_mode() && !self.is_virtual_mode() {
                    6
                } else {
                    9
                }
            }
            _ => unreachable!("Unhandled CPU_MODEL"),
        };
        self.clk(base + penalty);
    }

    fn sahf(&mut self) {
        let ah = self.regs.byte(ByteReg::AH);
        self.flags.carry_val = (ah & 0x01) as u32;
        self.flags.parity_val = if ah & 0x04 != 0 { 0 } else { 1 };
        self.flags.aux_val = (ah & 0x10) as u32;
        self.flags.zero_val = if ah & 0x40 != 0 { 0 } else { 1 };
        self.flags.sign_val = if ah & 0x80 != 0 { -1 } else { 0 };
        self.clk(Self::timing(3, 2));
    }

    fn lahf(&mut self) {
        let flags_val = self.flags.compress() as u8;
        self.regs.set_byte(ByteReg::AH, flags_val);
        self.clk(Self::timing(2, 3));
    }

    fn mov_al_moffs(&mut self, bus: &mut impl common::Bus) {
        let seg = self.default_seg(SegReg32::DS);
        if self.address_size_override {
            let offset = self.fetchdword(bus);
            let linear = self.seg_bases[seg as usize].wrapping_add(offset);
            let addr = self.translate_linear(linear, false, bus).unwrap_or(0);
            let val = bus.read_byte(addr);
            self.regs.set_byte(ByteReg::AL, val);
        } else {
            let offset = self.fetchword(bus) as u32;
            let val = self.read_byte_seg(bus, seg, offset);
            self.regs.set_byte(ByteReg::AL, val);
        }
        self.clk(Self::timing(4, 1));
    }

    fn mov_aw_moffs(&mut self, bus: &mut impl common::Bus) {
        if self.address_size_override {
            let offset = self.fetchdword(bus);
            let linear = self.default_base(SegReg32::DS).wrapping_add(offset);
            if self.operand_size_override {
                let val = self.read_dword_linear(bus, linear);
                self.regs.set_dword(DwordReg::EAX, val);
                let penalty = if linear & 3 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(4, 1) + penalty);
            } else {
                let val = self.read_word_linear(bus, linear);
                self.regs.set_word(WordReg::AX, val);
                let penalty = if linear & 1 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(4, 1) + penalty);
            }
        } else {
            let offset = self.fetchword(bus);
            let seg = self.default_seg(SegReg32::DS);
            let base = self.seg_base(seg);
            self.ea_seg = seg;
            self.eo = offset;
            self.eo32 = offset as u32;
            self.ea = base.wrapping_add(offset as u32);
            if self.operand_size_override {
                let val = self.seg_read_dword(bus);
                self.regs.set_dword(DwordReg::EAX, val);
                let penalty = if self.ea & 3 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(4, 1) + penalty);
            } else {
                let val = self.seg_read_word(bus);
                self.regs.set_word(WordReg::AX, val);
                let penalty = if self.ea & 1 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(4, 1) + penalty);
            }
        }
    }

    fn mov_moffs_al(&mut self, bus: &mut impl common::Bus) {
        let seg = self.default_seg(SegReg32::DS);
        let al = self.regs.byte(ByteReg::AL);
        if self.address_size_override {
            let offset = self.fetchdword(bus);
            let linear = self.seg_bases[seg as usize].wrapping_add(offset);
            let Some(addr) = self.translate_linear(linear, true, bus) else {
                return;
            };
            bus.write_byte(addr, al);
        } else {
            let offset = self.fetchword(bus) as u32;
            self.write_byte_seg(bus, seg, offset, al);
        }
        self.clk(Self::timing(2, 1));
    }

    fn mov_moffs_aw(&mut self, bus: &mut impl common::Bus) {
        if self.address_size_override {
            let offset = self.fetchdword(bus);
            let linear = self.default_base(SegReg32::DS).wrapping_add(offset);
            if self.operand_size_override {
                let val = self.regs.dword(DwordReg::EAX);
                self.write_dword_linear(bus, linear, val);
                let penalty = if linear & 3 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(2, 1) + penalty);
            } else {
                let val = self.regs.word(WordReg::AX);
                self.write_word_linear(bus, linear, val);
                let penalty = if linear & 1 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(2, 1) + penalty);
            }
        } else {
            let offset = self.fetchword(bus);
            let seg = self.default_seg(SegReg32::DS);
            let base = self.seg_base(seg);
            self.ea_seg = seg;
            self.eo = offset;
            self.eo32 = offset as u32;
            self.ea = base.wrapping_add(offset as u32);
            if self.operand_size_override {
                self.seg_write_dword(bus, self.regs.dword(DwordReg::EAX));
                let penalty = if self.ea & 3 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(2, 1) + penalty);
            } else {
                self.seg_write_word(bus, self.regs.word(WordReg::AX));
                let penalty = if self.ea & 1 != 0 {
                    Self::timing(4, 3)
                } else {
                    0
                };
                self.clk(Self::timing(2, 1) + penalty);
            }
        }
    }

    fn mov_byte_reg_imm(&mut self, reg: ByteReg, bus: &mut impl common::Bus) {
        let val = self.fetch(bus);
        self.regs.set_byte(reg, val);
        self.clk(Self::timing(2, 1));
    }

    fn mov_word_reg_imm(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            let val = self.fetchdword(bus);
            self.regs.set_dword(DwordReg::from_index(reg as u8), val);
        } else {
            let val = self.fetchword(bus);
            self.regs.set_word(reg, val);
        }
        self.clk(Self::timing(2, 1));
    }

    fn mov_rm_imm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            let val = self.fetch(bus);
            let reg = self.rm_byte(modrm);
            self.regs.set_byte(reg, val);
        } else {
            self.calc_ea(modrm, bus);
            let val = self.fetch(bus);
            let Some(addr) = self.translate_linear(self.ea, true, bus) else {
                return;
            };
            bus.write_byte(addr, val);
        }
        self.clk_modrm(modrm, Self::timing(2, 1), Self::timing(2, 1));
    }

    fn mov_rm_imm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.operand_size_override {
            if modrm >= 0xC0 {
                let val = self.fetchdword(bus);
                let reg = self.rm_dword(modrm);
                self.regs.set_dword(reg, val);
            } else {
                self.calc_ea(modrm, bus);
                let val = self.fetchdword(bus);
                self.seg_write_dword(bus, val);
            }
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(2, 1), 2);
        } else {
            if modrm >= 0xC0 {
                let val = self.fetchword(bus);
                let reg = self.rm_word(modrm);
                self.regs.set_word(reg, val);
            } else {
                self.calc_ea(modrm, bus);
                let val = self.fetchword(bus);
                self.seg_write_word(bus, val);
            }
            self.clk_modrm_word(modrm, Self::timing(2, 1), Self::timing(2, 1), 1);
        }
    }

    fn les(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            self.invalid(bus);
            return;
        }
        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let offset = self.seg_read_dword(bus);
            let segment = self.seg_read_word_at(bus, 4);
            if !self.load_segment(SegReg32::ES, segment, bus) {
                return;
            }
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, offset);
            self.clk(Self::timing(7, 6));
            return;
        }
        let offset = self.seg_read_word(bus);
        let segment = self.seg_read_word_at(bus, 2);
        if !self.load_segment(SegReg32::ES, segment, bus) {
            return;
        }
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.clk(Self::timing(7, 6));
    }

    fn lds(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            self.invalid(bus);
            return;
        }
        self.calc_ea(modrm, bus);
        if self.operand_size_override {
            let offset = self.seg_read_dword(bus);
            let segment = self.seg_read_word_at(bus, 4);
            if !self.load_segment(SegReg32::DS, segment, bus) {
                return;
            }
            let reg = self.reg_dword(modrm);
            self.regs.set_dword(reg, offset);
            self.clk(Self::timing(7, 6));
            return;
        }
        let offset = self.seg_read_word(bus);
        let segment = self.seg_read_word_at(bus, 2);
        if !self.load_segment(SegReg32::DS, segment, bus) {
            return;
        }
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.clk(Self::timing(7, 6));
    }

    fn enter(&mut self, bus: &mut impl common::Bus) {
        let alloc = self.fetchword(bus);
        let level = self.fetch(bus) & 0x1F;
        if self.operand_size_override {
            let sp_pen = self.sp_penalty();
            let ebp_val = self.regs.dword(DwordReg::EBP);
            self.push_dword(bus, ebp_val);
            if self.use_esp() {
                let frame_ptr = self.regs.dword(DwordReg::ESP);
                if level > 0 {
                    for _ in 1..level {
                        let ebp = self.regs.dword(DwordReg::EBP).wrapping_sub(4);
                        self.regs.set_dword(DwordReg::EBP, ebp);
                        let val = self.read_dword_seg(bus, SegReg32::SS, ebp);
                        self.push_dword(bus, val);
                    }
                    self.push_dword(bus, frame_ptr);
                }
                self.regs.set_dword(DwordReg::EBP, frame_ptr);
                let esp = self.regs.dword(DwordReg::ESP).wrapping_sub(alloc as u32);
                self.regs.set_dword(DwordReg::ESP, esp);
            } else {
                let frame_ptr = self.regs.word(WordReg::SP);
                if level > 0 {
                    for _ in 1..level {
                        let bp = self.regs.word(WordReg::BP).wrapping_sub(4);
                        self.regs.set_word(WordReg::BP, bp);
                        let val = self.read_dword_seg(bus, SegReg32::SS, bp as u32);
                        self.push_dword(bus, val);
                    }
                    self.push_dword(bus, frame_ptr as u32);
                }
                self.regs.set_dword(DwordReg::EBP, frame_ptr as u32);
                let sp = self.regs.word(WordReg::SP).wrapping_sub(alloc);
                self.regs.set_word(WordReg::SP, sp);
            }
            if level == 0 {
                self.clk(Self::timing(10, 14) + sp_pen);
            } else if level == 1 {
                self.clk(Self::timing(12, 17) + sp_pen);
            } else {
                let l = level as i32;
                self.clk(Self::timing(15 + 4 * (l - 1), 17 + 3 * l) + sp_pen);
            }
        } else {
            let sp_pen = self.sp_penalty();
            let bp = self.regs.word(WordReg::BP);
            self.push(bus, bp);
            let frame_ptr = self.regs.word(WordReg::SP);
            if level > 0 {
                for _ in 1..level {
                    let bp_val = self.regs.word(WordReg::BP).wrapping_sub(2);
                    self.regs.set_word(WordReg::BP, bp_val);
                    let val = self.read_word_seg(bus, SegReg32::SS, bp_val as u32);
                    self.push(bus, val);
                }
                self.push(bus, frame_ptr);
            }
            self.regs.set_word(WordReg::BP, frame_ptr);
            let sp = self.regs.word(WordReg::SP).wrapping_sub(alloc);
            self.regs.set_word(WordReg::SP, sp);
            if level == 0 {
                self.clk(Self::timing(10, 14) + sp_pen);
            } else if level == 1 {
                self.clk(Self::timing(12, 17) + sp_pen);
            } else {
                let l = level as i32;
                self.clk(Self::timing(15 + 4 * (l - 1), 17 + 3 * l) + sp_pen);
            }
        }
    }

    fn leave(&mut self, bus: &mut impl common::Bus) {
        if self.operand_size_override {
            if self.use_esp() {
                let ebp = self.regs.dword(DwordReg::EBP);
                self.regs.set_dword(DwordReg::ESP, ebp);
            } else {
                let bp = self.regs.word(WordReg::BP);
                self.regs.set_word(WordReg::SP, bp);
            }
            let penalty = self.sp_penalty();
            let val = self.pop_dword(bus);
            self.regs.set_dword(DwordReg::EBP, val);
            self.clk(Self::timing(4, 5) + penalty);
        } else {
            let bp = self.regs.word(WordReg::BP);
            self.regs.set_word(WordReg::SP, bp);
            let penalty = self.sp_penalty();
            let val = self.pop(bus);
            self.regs.set_word(WordReg::BP, val);
            self.clk(Self::timing(4, 5) + penalty);
        }
    }

    fn int3(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        self.raise_software_interrupt(3, false, bus);
        self.clk(Self::timing(33, 26) + penalty);
    }

    fn int_imm(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();
        let vector = self.fetch(bus);
        self.raise_software_interrupt(vector, true, bus);
        self.clk(Self::timing(37, 30) + penalty);
    }

    fn into(&mut self, bus: &mut impl common::Bus) {
        if self.flags.of() {
            let penalty = self.sp_penalty();
            self.raise_software_interrupt(4, false, bus);
            self.clk(Self::timing(35, 28) + penalty);
        } else {
            self.clk(Self::timing(3, 3));
        }
    }

    fn iret(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty();

        if !self.is_protected_mode() {
            if self.operand_size_override {
                let eip = self.pop_dword(bus);
                let cs = self.pop_dword(bus) as u16;
                let eflags = self.pop_dword(bus);
                if !self.load_segment(SegReg32::CS, cs, bus) {
                    return;
                }
                self.ip = eip as u16;
                self.ip_upper = eip & 0xFFFF_0000;
                self.flags.load_flags(eflags as u16, 0, false);
            } else {
                let ip = self.pop(bus);
                let cs = self.pop(bus);
                let flags_val = self.pop(bus);
                if !self.load_segment(SegReg32::CS, cs, bus) {
                    return;
                }
                self.ip = ip;
                self.ip_upper = 0;
                self.flags.load_flags(flags_val, 0, false);
            }
            self.clk(Self::timing(22, 15) + penalty);
            return;
        }

        if self.is_virtual_mode() {
            if self.flags.iopl < 3 {
                self.raise_fault_with_code(13, 0, bus);
                return;
            }

            if self.operand_size_override {
                let new_eip = self.pop_dword(bus);
                let new_cs = self.pop_dword(bus) as u16;
                let new_eflags = self.pop_dword(bus);

                self.sregs[SegReg32::CS as usize] = new_cs;
                self.set_real_segment_cache(SegReg32::CS, new_cs);
                self.ip = new_eip as u16;
                self.ip_upper = 0;
                self.flags.load_flags(new_eflags as u16, 3, true);
                self.eflags_upper = (new_eflags & 0x00FF_0000) | 0x0002_0000;
            } else {
                let new_ip = self.pop(bus);
                let new_cs = self.pop(bus);
                let new_flags = self.pop(bus);

                self.sregs[SegReg32::CS as usize] = new_cs;
                self.set_real_segment_cache(SegReg32::CS, new_cs);
                self.ip = new_ip;
                self.ip_upper = 0;
                self.flags.load_flags(new_flags, 3, true);
                self.eflags_upper = 0x0002_0000;
            }

            self.clk(Self::timing(22, 15) + penalty);
            return;
        }

        // Protected mode IRET.
        if self.flags.nt {
            // Task return via back-link in current TSS.
            let backlink = self.read_word_linear(bus, self.tr_base);
            self.switch_task(backlink, super::TaskType::Iret, bus);
            let flags_val = self.flags.compress();
            let cpl = self.cpl();
            self.flags.load_flags(flags_val, cpl, true);
            self.clk(Self::timing(22, 15) + penalty);
            return;
        }

        let old_cpl = self.cpl();

        let sp = if self.use_esp() {
            self.regs.dword(DwordReg::ESP)
        } else {
            self.regs.word(WordReg::SP) as u32
        };
        let ss_base = self.seg_base(SegReg32::SS);

        if self.operand_size_override {
            let new_eip = self.read_dword_linear(bus, ss_base.wrapping_add(sp));
            let new_cs =
                self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(4))) as u16;
            let new_eflags = self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(8)));

            // IRET from CPL0 to virtual-8086 mode.
            // Stack frame: EIP, CS, EFLAGS, ESP, SS, ES, DS, FS, GS.
            if old_cpl == 0 && (new_eflags & 0x0002_0000) != 0 {
                let new_esp =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(12)));
                let new_ss =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(16))) as u16;
                let new_es =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(20))) as u16;
                let new_ds =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(24))) as u16;
                let new_fs =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(28))) as u16;
                let new_gs =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(32))) as u16;

                self.flags.load_flags(new_eflags as u16, old_cpl, true);
                self.eflags_upper = (new_eflags & 0x00FF_0000) | 0x0002_0000;

                self.sregs[SegReg32::CS as usize] = new_cs;
                self.set_real_segment_cache(SegReg32::CS, new_cs);
                self.ip = new_eip as u16;
                self.ip_upper = 0;

                self.sregs[SegReg32::SS as usize] = new_ss;
                self.set_real_segment_cache(SegReg32::SS, new_ss);
                self.regs.set_dword(DwordReg::ESP, new_esp);

                self.sregs[SegReg32::ES as usize] = new_es;
                self.set_real_segment_cache(SegReg32::ES, new_es);
                self.sregs[SegReg32::DS as usize] = new_ds;
                self.set_real_segment_cache(SegReg32::DS, new_ds);
                self.sregs[SegReg32::FS as usize] = new_fs;
                self.set_real_segment_cache(SegReg32::FS, new_fs);
                self.sregs[SegReg32::GS as usize] = new_gs;
                self.set_real_segment_cache(SegReg32::GS, new_gs);

                self.clk(Self::timing(60, 15) + penalty);
                return;
            }

            let new_rpl = new_cs & 3;

            if new_rpl > old_cpl {
                let new_esp =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(12)));
                let new_ss =
                    self.read_dword_linear(bus, ss_base.wrapping_add(sp.wrapping_add(16))) as u16;

                let saved_cs = self.save_cs_state();
                if !self.load_cs_for_return(new_cs, new_eip, bus) {
                    return;
                }
                self.ip = new_eip as u16;
                self.ip_upper = new_eip & 0xFFFF_0000;
                self.flags.load_flags(new_eflags as u16, old_cpl, true);
                if old_cpl == 0 {
                    self.eflags_upper = new_eflags & 0x00FF_0000;
                } else {
                    self.eflags_upper = new_eflags & 0x00FD_0000;
                }

                if !self.load_segment(SegReg32::SS, new_ss, bus) {
                    self.restore_cs_state(&saved_cs);
                    return;
                }
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, new_esp);
                } else {
                    self.regs.set_word(WordReg::SP, new_esp as u16);
                }

                let new_cpl = self.cpl();
                self.invalidate_segment_if_needed(SegReg32::DS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::ES, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::FS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::GS, new_cpl);
            } else {
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, sp.wrapping_add(12));
                } else {
                    self.regs.set_word(WordReg::SP, sp.wrapping_add(12) as u16);
                }
                if !self.load_cs_for_return(new_cs, new_eip, bus) {
                    return;
                }
                self.ip = new_eip as u16;
                self.ip_upper = new_eip & 0xFFFF_0000;
                self.flags.load_flags(new_eflags as u16, old_cpl, true);
                if old_cpl == 0 {
                    self.eflags_upper = new_eflags & 0x00FF_0000;
                } else {
                    self.eflags_upper = new_eflags & 0x00FD_0000;
                }
            }
        } else {
            let new_ip = self.read_word_linear(bus, ss_base.wrapping_add(sp));
            let new_cs = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(2)));
            let new_flags = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(4)));

            let new_rpl = new_cs & 3;

            if new_rpl > old_cpl {
                let new_sp = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(6)));
                let new_ss = self.read_word_linear(bus, ss_base.wrapping_add(sp.wrapping_add(8)));

                let saved_cs = self.save_cs_state();
                if !self.load_cs_for_return(new_cs, new_ip as u32, bus) {
                    return;
                }
                self.ip = new_ip;
                self.ip_upper = 0;
                self.flags.load_flags(new_flags, old_cpl, true);

                if !self.load_segment(SegReg32::SS, new_ss, bus) {
                    self.restore_cs_state(&saved_cs);
                    return;
                }
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, new_sp as u32);
                } else {
                    self.regs.set_word(WordReg::SP, new_sp);
                }

                let new_cpl = self.cpl();
                self.invalidate_segment_if_needed(SegReg32::DS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::ES, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::FS, new_cpl);
                self.invalidate_segment_if_needed(SegReg32::GS, new_cpl);
            } else {
                if self.use_esp() {
                    self.regs.set_dword(DwordReg::ESP, sp.wrapping_add(6));
                } else {
                    self.regs.set_word(WordReg::SP, sp.wrapping_add(6) as u16);
                }
                if !self.load_cs_for_return(new_cs, new_ip as u32, bus) {
                    return;
                }
                self.ip = new_ip;
                self.ip_upper = 0;
                self.flags.load_flags(new_flags, old_cpl, true);
            }
        }

        self.clk(Self::timing(22, 15) + penalty);
    }

    fn loopne(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8;
        let count = if self.address_size_override {
            let value = self.regs.dword(DwordReg::ECX).wrapping_sub(1);
            self.regs.set_dword(DwordReg::ECX, value);
            value
        } else {
            let value = self.regs.word(WordReg::CX).wrapping_sub(1);
            self.regs.set_word(WordReg::CX, value);
            value as u32
        };
        if count != 0 && !self.flags.zf() {
            self.apply_branch_disp8(disp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(11 + m);
                }
                CPU_MODEL_486 => self.clk(9),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            self.clk(Self::timing(5, 6));
        }
    }

    fn loope(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8;
        let count = if self.address_size_override {
            let value = self.regs.dword(DwordReg::ECX).wrapping_sub(1);
            self.regs.set_dword(DwordReg::ECX, value);
            value
        } else {
            let value = self.regs.word(WordReg::CX).wrapping_sub(1);
            self.regs.set_word(WordReg::CX, value);
            value as u32
        };
        if count != 0 && self.flags.zf() {
            self.apply_branch_disp8(disp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(11 + m);
                }
                CPU_MODEL_486 => self.clk(9),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            self.clk(Self::timing(5, 6));
        }
    }

    fn loop_(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8;
        let count = if self.address_size_override {
            let value = self.regs.dword(DwordReg::ECX).wrapping_sub(1);
            self.regs.set_dword(DwordReg::ECX, value);
            value
        } else {
            let value = self.regs.word(WordReg::CX).wrapping_sub(1);
            self.regs.set_word(WordReg::CX, value);
            value as u32
        };
        if count != 0 {
            self.apply_branch_disp8(disp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(11 + m);
                }
                CPU_MODEL_486 => self.clk(7),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            self.clk(Self::timing(5, 6));
        }
    }

    fn jcxz(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8;
        let is_zero = if self.address_size_override {
            self.regs.dword(DwordReg::ECX) == 0
        } else {
            self.regs.word(WordReg::CX) == 0
        };
        if is_zero {
            self.apply_branch_disp8(disp);
            match CPU_MODEL {
                CPU_MODEL_386 => {
                    let m = self.next_instruction_length_approx(bus);
                    self.clk(9 + m);
                }
                CPU_MODEL_486 => self.clk(8),
                _ => {
                    unreachable!("Unhandled CPU_MODEL")
                }
            }
        } else {
            self.clk(Self::timing(5, 5));
        }
    }

    fn in_al_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        if !self.check_io_privilege(port, 1, bus) {
            return;
        }
        let val = bus.io_read_byte(port);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(Self::timing(12, 14));
    }

    fn in_aw_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_io_privilege(port, size, bus) {
            return;
        }
        if self.operand_size_override {
            let low = bus.io_read_word(port) as u32;
            let high = bus.io_read_word(port.wrapping_add(2)) as u32;
            self.regs.set_dword(DwordReg::EAX, low | (high << 16));
        } else {
            let val = bus.io_read_word(port);
            self.regs.set_word(WordReg::AX, val);
        }
        self.clk(Self::timing(12, 14));
    }

    fn out_imm_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        if !self.check_io_privilege(port, 1, bus) {
            return;
        }
        let val = self.regs.byte(ByteReg::AL);
        bus.io_write_byte(port, val);
        self.clk(Self::timing(10, 16));
    }

    fn out_imm_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_io_privilege(port, size, bus) {
            return;
        }
        if self.operand_size_override {
            let val = self.regs.dword(DwordReg::EAX);
            bus.io_write_word(port, val as u16);
            bus.io_write_word(port.wrapping_add(2), (val >> 16) as u16);
        } else {
            let val = self.regs.word(WordReg::AX);
            bus.io_write_word(port, val);
        }
        self.clk(Self::timing(10, 16));
    }

    fn in_al_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, 1, bus) {
            return;
        }
        let val = bus.io_read_byte(port);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(Self::timing(13, 14));
    }

    fn in_aw_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_io_privilege(port, size, bus) {
            return;
        }
        if self.operand_size_override {
            let low = bus.io_read_word(port) as u32;
            let high = bus.io_read_word(port.wrapping_add(2)) as u32;
            self.regs.set_dword(DwordReg::EAX, low | (high << 16));
        } else {
            let val = bus.io_read_word(port);
            self.regs.set_word(WordReg::AX, val);
        }
        self.clk(Self::timing(13, 14));
    }

    fn out_dw_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, 1, bus) {
            return;
        }
        let val = self.regs.byte(ByteReg::AL);
        bus.io_write_byte(port, val);
        self.clk(Self::timing(11, 16));
    }

    fn out_dw_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_io_privilege(port, size, bus) {
            return;
        }
        if self.operand_size_override {
            let val = self.regs.dword(DwordReg::EAX);
            bus.io_write_word(port, val as u16);
            bus.io_write_word(port.wrapping_add(2), (val >> 16) as u16);
        } else {
            let val = self.regs.word(WordReg::AX);
            bus.io_write_word(port, val);
        }
        self.clk(Self::timing(11, 16));
    }

    fn xlat(&mut self, bus: &mut impl common::Bus) {
        let seg = self.default_seg(SegReg32::DS);
        let offset = if self.address_size_override {
            self.regs
                .dword(DwordReg::EBX)
                .wrapping_add(self.regs.byte(ByteReg::AL) as u32)
        } else {
            self.regs
                .word(WordReg::BX)
                .wrapping_add(self.regs.byte(ByteReg::AL) as u16) as u32
        };
        let val = self.read_byte_seg(bus, seg, offset);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(Self::timing(5, 4));
    }

    fn daa(&mut self, _bus: &mut impl common::Bus) {
        let old_al = self.regs.byte(ByteReg::AL);
        let old_cf = self.flags.cf();
        let old_af = self.flags.af();
        let mut al = old_al;
        let mut carry = old_cf;

        if (old_al & 0x0F) > 9 || old_af {
            al = al.wrapping_add(6);
            carry = old_cf || al < old_al;
            self.flags.aux_val = 1;
        } else {
            self.flags.aux_val = 0;
        }
        if old_al > 0x99 || old_cf {
            al = al.wrapping_add(0x60);
            carry = true;
        }
        self.flags.carry_val = u32::from(carry);
        self.regs.set_byte(ByteReg::AL, al);
        self.flags.set_szpf_byte(al as u32);
        self.clk(Self::timing(4, 2));
    }

    fn das(&mut self, _bus: &mut impl common::Bus) {
        let old_al = self.regs.byte(ByteReg::AL);
        let old_cf = self.flags.cf();
        let old_af = self.flags.af();
        let mut al = old_al;
        let mut carry = old_cf;

        if (old_al & 0x0F) > 9 || old_af {
            al = al.wrapping_sub(6);
            carry = old_cf || old_al < 6;
            self.flags.aux_val = 1;
        } else {
            self.flags.aux_val = 0;
        }
        if old_al > 0x99 || old_cf {
            al = al.wrapping_sub(0x60);
            carry = true;
        }
        self.flags.carry_val = u32::from(carry);
        self.regs.set_byte(ByteReg::AL, al);
        self.flags.set_szpf_byte(al as u32);
        self.clk(Self::timing(4, 2));
    }

    fn aaa(&mut self, _bus: &mut impl common::Bus) {
        if (self.regs.byte(ByteReg::AL) & 0x0F) > 9 || self.flags.af() {
            let ax = self.regs.word(WordReg::AX).wrapping_add(0x0106);
            self.regs.set_word(WordReg::AX, ax);
            let val = self.regs.byte(ByteReg::AL) & 0x0F;
            self.regs.set_byte(ByteReg::AL, val);
            self.flags.aux_val = 1;
            self.flags.carry_val = 1;
        } else {
            let al = self.regs.byte(ByteReg::AL) & 0x0F;
            self.regs.set_byte(ByteReg::AL, al);
            self.flags.aux_val = 0;
            self.flags.carry_val = 0;
        }
        self.clk(Self::timing(4, 3));
    }

    fn aas(&mut self, _bus: &mut impl common::Bus) {
        if (self.regs.byte(ByteReg::AL) & 0x0F) > 9 || self.flags.af() {
            let ax = self.regs.word(WordReg::AX).wrapping_sub(0x0106);
            self.regs.set_word(WordReg::AX, ax);
            let val = self.regs.byte(ByteReg::AL) & 0x0F;
            self.regs.set_byte(ByteReg::AL, val);
            self.flags.aux_val = 1;
            self.flags.carry_val = 1;
        } else {
            let al = self.regs.byte(ByteReg::AL) & 0x0F;
            self.regs.set_byte(ByteReg::AL, al);
            self.flags.aux_val = 0;
            self.flags.carry_val = 0;
        }
        self.clk(Self::timing(4, 3));
    }

    fn aam(&mut self, bus: &mut impl common::Bus) {
        let base = self.fetch(bus);
        if base == 0 {
            self.regs.set_byte(ByteReg::AH, 0xFF);
            let val = self.regs.byte(ByteReg::AL) as u32;
            self.flags.set_szpf_byte(val);
            self.clk(Self::timing(17, 15));
            return;
        }
        let al = self.regs.byte(ByteReg::AL);
        self.regs.set_byte(ByteReg::AH, al / base);
        self.regs.set_byte(ByteReg::AL, al % base);
        let val = self.regs.byte(ByteReg::AL) as u32;
        self.flags.set_szpf_byte(val);
        self.clk(Self::timing(17, 15));
    }

    fn aad(&mut self, bus: &mut impl common::Bus) {
        let base = self.fetch(bus);
        let al = self.regs.byte(ByteReg::AL);
        let ah = self.regs.byte(ByteReg::AH);
        let result = al.wrapping_add(ah.wrapping_mul(base));
        self.regs.set_byte(ByteReg::AL, result);
        self.regs.set_byte(ByteReg::AH, 0);
        self.flags.set_szpf_byte(result as u32);
        self.clk(Self::timing(19, 14));
    }

    fn salc(&mut self) {
        let val = if self.flags.cf() { 0xFF } else { 0x00 };
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(Self::timing(2, 2));
    }

    fn clc(&mut self) {
        self.flags.carry_val = 0;
        self.clk(Self::timing(2, 2));
    }

    fn stc(&mut self) {
        self.flags.carry_val = 1;
        self.clk(Self::timing(2, 2));
    }

    fn cli(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() > u16::from(self.flags.iopl) {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        self.flags.if_flag = false;
        self.clk(Self::timing(3, 5));
    }

    fn sti(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() > u16::from(self.flags.iopl) {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        self.flags.if_flag = true;
        self.no_interrupt = 1;
        self.clk(Self::timing(3, 5));
    }

    fn cld(&mut self) {
        self.flags.df = false;
        self.clk(Self::timing(2, 2));
    }

    fn std(&mut self) {
        self.flags.df = true;
        self.clk(Self::timing(2, 2));
    }

    fn cmc(&mut self) {
        self.flags.carry_val = if self.flags.cf() { 0 } else { 1 };
        self.clk(Self::timing(2, 2));
    }

    fn hlt(&mut self, bus: &mut impl common::Bus) {
        if self.is_protected_mode() && self.cpl() != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        self.halted = true;
        self.clk(Self::timing(5, 4));
    }

    fn invalid(&mut self, bus: &mut impl common::Bus) {
        // #UD (Invalid Opcode, INT 6): introduced with the 80286.
        // The fault pushes CS:IP pointing to the faulting opcode.
        self.raise_fault(6, bus);
    }
}
