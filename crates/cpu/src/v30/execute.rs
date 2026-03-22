use super::V30;
use crate::{ByteReg, SegReg16, WordReg};

impl V30 {
    pub(super) fn dispatch(&mut self, opcode: u8, bus: &mut impl common::Bus) {
        match opcode {
            // ADD
            0x00 => self.add_br8(bus),
            0x01 => self.add_wr16(bus),
            0x02 => self.add_r8b(bus),
            0x03 => self.add_r16w(bus),
            0x04 => self.add_ald8(bus),
            0x05 => self.add_axd16(bus),
            0x06 => self.push_seg(SegReg16::ES, bus),
            0x07 => {
                self.pop_seg(SegReg16::ES, bus);
                self.inhibit_all = 1;
            }

            // OR
            0x08 => self.or_br8(bus),
            0x09 => self.or_wr16(bus),
            0x0A => self.or_r8b(bus),
            0x0B => self.or_r16w(bus),
            0x0C => self.or_ald8(bus),
            0x0D => self.or_axd16(bus),
            0x0E => self.push_seg(SegReg16::CS, bus),
            0x0F => self.extended_0f(bus),

            // ADC
            0x10 => self.adc_br8(bus),
            0x11 => self.adc_wr16(bus),
            0x12 => self.adc_r8b(bus),
            0x13 => self.adc_r16w(bus),
            0x14 => self.adc_ald8(bus),
            0x15 => self.adc_axd16(bus),
            0x16 => self.push_seg(SegReg16::SS, bus),
            0x17 => {
                self.pop_seg(SegReg16::SS, bus);
                self.inhibit_all = 1;
            }

            // SBB
            0x18 => self.sbb_br8(bus),
            0x19 => self.sbb_wr16(bus),
            0x1A => self.sbb_r8b(bus),
            0x1B => self.sbb_r16w(bus),
            0x1C => self.sbb_ald8(bus),
            0x1D => self.sbb_axd16(bus),
            0x1E => self.push_seg(SegReg16::DS, bus),
            0x1F => {
                self.pop_seg(SegReg16::DS, bus);
                self.inhibit_all = 1;
            }

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
            0x63 => self.invalid(bus),
            0x64 => self.repnc(bus),
            0x65 => self.repc(bus),
            0x66 => self.invalid(bus),
            0x67 => self.invalid(bus),
            0x68 => self.push_imm16(bus),
            0x69 => self.imul_r16w_imm16(bus),
            0x6A => self.push_imm8(bus),
            0x6B => self.imul_r16w_imm8(bus),
            0x6C => {
                self.insb(bus);
                self.clk(1);
            }
            0x6D => {
                self.insw(bus);
                self.clk(1);
            }
            0x6E => {
                self.outsb(bus);
                self.clk(-1);
            }
            0x6F => {
                self.outsw(bus);
                self.clk(-1);
            }

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
            0x90 => self.clk(3),
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
            0x9B => self.clk(2), // WAIT
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
            0xA4 => {
                self.movsb(bus);
                self.clk(1);
            }
            0xA5 => {
                self.movsw(bus);
                self.clk(1);
            }
            0xA6 => {
                self.cmpsb(bus);
                self.clk(-1);
            }
            0xA7 => {
                self.cmpsw(bus);
                self.clk(-1);
            }

            // TEST AL/AX, imm
            0xA8 => self.test_al_imm8(bus),
            0xA9 => self.test_aw_imm16(bus),

            // STOS, LODS, SCAS
            0xAA => self.stosb(bus),
            0xAB => self.stosw(bus),
            0xAC => {
                self.lodsb(bus);
                self.clk(1);
            }
            0xAD => {
                self.lodsw(bus);
                self.clk(1);
            }
            0xAE => {
                self.scasb(bus);
                self.clk(-1);
            }
            0xAF => {
                self.scasw(bus);
                self.clk(-1);
            }

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

            // V30: D6 = XLAT (not SALC)
            0xD6 => self.xlat(bus),

            // XLAT
            0xD7 => self.xlat(bus),

            // FPU escape (NOP on V30)
            0xD8..=0xDF => self.fpu_escape(bus),

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
            0xF4 => self.hlt(),

            // CMC
            0xF5 => self.cmc(),

            // Group 3 byte/word
            0xF6 => self.group_f6(bus),
            0xF7 => self.group_f7(bus),

            // CLC, STC, CLI, STI, CLD, STD
            0xF8 => self.clc(),
            0xF9 => self.stc(),
            0xFA => self.cli(),
            0xFB => self.sti(),
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
        self.clk_modrm(modrm, 2, 7);
    }

    fn add_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_add_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn add_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_add_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn add_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_add_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn add_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_add_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn add_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_add_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn or_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    fn or_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_or_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn or_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn or_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_or_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn or_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_or_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn or_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_or_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn adc_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    fn adc_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn adc_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn adc_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn adc_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn adc_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn sbb_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    fn sbb_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn sbb_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn sbb_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn sbb_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn sbb_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn and_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    fn and_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_and_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn and_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn and_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_and_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn and_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_and_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn and_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_and_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn sub_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    fn sub_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_sub_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn sub_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn sub_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_sub_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn sub_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_sub_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn sub_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_sub_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn xor_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(modrm, 2, 7);
    }

    fn xor_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_xor_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(modrm, 2, 7, 2);
    }

    fn xor_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(modrm, 2, 7);
    }

    fn xor_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_xor_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(modrm, 2, 7, 1);
    }

    fn xor_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_xor_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(3);
    }

    fn xor_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_xor_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(3);
    }

    fn cmp_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(modrm, 2, 6);
    }

    fn cmp_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        self.alu_sub_word(dst, src);
        self.clk_modrm_word(modrm, 2, 6, 1);
    }

    fn cmp_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(modrm, 2, 6);
    }

    fn cmp_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        self.alu_sub_word(dst, src);
        self.clk_modrm_word(modrm, 2, 6, 1);
    }

    fn cmp_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(dst, src);
        self.clk(3);
    }

    fn cmp_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        self.alu_sub_word(dst, src);
        self.clk(3);
    }

    fn inc_word_reg(&mut self, reg: WordReg) {
        let val = self.regs.word(reg);
        let result = self.alu_inc_word(val);
        self.regs.set_word(reg, result);
        self.clk(2);
    }

    fn dec_word_reg(&mut self, reg: WordReg) {
        let val = self.regs.word(reg);
        let result = self.alu_dec_word(val);
        self.regs.set_word(reg, result);
        self.clk(2);
    }

    fn push_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.regs.word(reg);
        self.push(bus, val);
        self.clk(3 + penalty);
    }

    pub(super) fn push_sp(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        self.regs.set_word(WordReg::SP, sp);
        let base = self.seg_base(SegReg16::SS);
        bus.write_byte(base.wrapping_add(sp as u32) & 0xFFFFF, sp as u8);
        bus.write_byte(
            base.wrapping_add(sp.wrapping_add(1) as u32) & 0xFFFFF,
            (sp >> 8) as u8,
        );
        self.clk(3 + penalty);
    }

    fn pop_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.pop(bus);
        self.regs.set_word(reg, val);
        self.clk(5 + penalty);
    }

    fn push_seg(&mut self, seg: SegReg16, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.sregs[seg as usize];
        self.push(bus, val);
        self.clk(3 + penalty);
    }

    fn pop_seg(&mut self, seg: SegReg16, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.pop(bus);
        self.sregs[seg as usize] = val;
        self.clk(5 + penalty);
    }

    fn pusha(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(8);
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
        self.clk(17 + penalty);
    }

    fn popa(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(8);
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
        self.clk(19 + penalty);
    }

    fn bound(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.regs.word(self.reg_word(modrm)) as i16;
        if modrm >= 0xC0 {
            return;
        }
        self.calc_ea(modrm, bus);
        let ea_pen = if self.ea & 1 == 1 { 8 } else { 0 };
        let low = self.seg_read_word(bus) as i16;
        let high = self.seg_read_word_at(bus, 2) as i16;
        if val < low || val > high {
            let sp_pen = self.sp_penalty(3);
            self.raise_interrupt(5, bus);
            self.clk(33 + ea_pen + sp_pen);
        } else {
            self.clk(13 + ea_pen);
        }
    }

    fn push_imm16(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.fetchword(bus);
        self.push(bus, val);
        self.clk(3 + penalty);
    }

    fn push_imm8(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.fetch(bus) as i8 as u16;
        self.push(bus, val);
        self.clk(3 + penalty);
    }

    fn imul_r16w_imm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
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
        self.flags.overflow_val = self.flags.carry_val;
        self.clk_modrm_word(modrm, 21, 24, 1);
    }

    fn imul_r16w_imm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
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
        self.flags.overflow_val = self.flags.carry_val;
        self.clk_modrm_word(modrm, 21, 24, 1);
    }

    fn jcc(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.ip = self.ip.wrapping_add(disp as u16);
            self.clk(7);
        } else {
            self.clk(3);
        }
    }

    fn jcc_swapped(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.ip = self.ip.wrapping_add(disp as u16);
            self.clk(7);
        } else {
            self.clk(3);
        }
    }

    fn test_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_and_byte(dst, src);
        self.clk_modrm(modrm, 2, 6);
    }

    fn test_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        self.alu_and_word(dst, src);
        self.clk_modrm_word(modrm, 2, 6, 1);
    }

    fn test_al_imm8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_and_byte(dst, src);
        self.clk(3);
    }

    fn test_aw_imm16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        self.alu_and_word(dst, src);
        self.clk(3);
    }

    fn xchg_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let reg = self.reg_byte(modrm);
        let reg_val = self.regs.byte(reg);
        let rm_val = self.get_rm_byte(modrm, bus);
        self.regs.set_byte(reg, rm_val);
        self.putback_rm_byte(modrm, reg_val, bus);
        self.clk_modrm(modrm, 3, 5);
    }

    fn xchg_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let reg = self.reg_word(modrm);
        let reg_val = self.regs.word(reg);
        let rm_val = self.get_rm_word(modrm, bus);
        self.regs.set_word(reg, rm_val);
        self.putback_rm_word(modrm, reg_val, bus);
        self.clk_modrm_word(modrm, 3, 5, 2);
    }

    fn xchg_aw(&mut self, reg: WordReg) {
        let aw = self.regs.word(WordReg::AX);
        let val = self.regs.word(reg);
        self.regs.set_word(WordReg::AX, val);
        self.regs.set_word(reg, aw);
        self.clk(3);
    }

    fn mov_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.regs.byte(self.reg_byte(modrm));
        self.put_rm_byte(modrm, val, bus);
        self.clk_modrm(modrm, 2, 3);
    }

    fn mov_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.regs.word(self.reg_word(modrm));
        self.put_rm_word(modrm, val, bus);
        self.clk_modrm_word(modrm, 2, 3, 1);
    }

    fn mov_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_byte(modrm, bus);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, val);
        self.clk_modrm(modrm, 2, 5);
    }

    fn mov_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_word(modrm, bus);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, val);
        self.clk_modrm_word(modrm, 2, 5, 1);
    }

    fn mov_rm_sreg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let seg = SegReg16::from_index((modrm >> 3) & 3);
        let val = self.sregs[seg as usize];
        self.put_rm_word(modrm, val, bus);
        self.clk_modrm_word(modrm, 2, 3, 1);
    }

    fn mov_sreg_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_word(modrm, bus);
        let seg = SegReg16::from_index((modrm >> 3) & 3);
        self.sregs[seg as usize] = val;
        self.inhibit_all = 1;
        self.clk_modrm_word(modrm, 2, 5, 1);
    }

    fn lea(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.calc_ea(modrm, bus);
        let reg = self.reg_word(modrm);
        let val = self.eo;
        self.regs.set_word(reg, val);
        self.clk(3);
    }

    fn pop_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let sp_pen = self.sp_penalty(1);
        let val = self.pop(bus);
        self.put_rm_word(modrm, val, bus);
        if modrm >= 0xC0 {
            self.clk(5 + sp_pen);
        } else {
            let ea_pen = if self.ea & 1 == 1 { 4 } else { 0 };
            self.clk(5 + sp_pen + ea_pen);
        }
    }

    fn cbw(&mut self) {
        let al = self.regs.byte(ByteReg::AL) as i8 as i16 as u16;
        self.regs.set_word(WordReg::AX, al);
        self.clk(2);
    }

    fn cwd(&mut self) {
        let aw = self.regs.word(WordReg::AX) as i16;
        self.regs
            .set_word(WordReg::DX, if aw < 0 { 0xFFFF } else { 0 });
        self.clk(2);
    }

    fn call_far(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(2);
        let offset = self.fetchword(bus);
        let segment = self.fetchword(bus);
        let cs = self.sregs[SegReg16::CS as usize];
        self.push(bus, cs);
        self.push(bus, self.ip);
        self.ip = offset;
        self.sregs[SegReg16::CS as usize] = segment;
        self.clk(13 + penalty);
    }

    fn call_near(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let disp = self.fetchword(bus);
        self.push(bus, self.ip);
        self.ip = self.ip.wrapping_add(disp);
        self.clk(7 + penalty);
    }

    fn jmp_near(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetchword(bus);
        self.ip = self.ip.wrapping_add(disp);
        self.clk(7);
    }

    fn jmp_far(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let segment = self.fetchword(bus);
        self.ip = offset;
        self.sregs[SegReg16::CS as usize] = segment;
        self.clk(11);
    }

    fn jmp_short(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        self.ip = self.ip.wrapping_add(disp);
        self.clk(7);
    }

    fn ret_near(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        self.ip = self.pop(bus);
        self.clk(11 + penalty);
    }

    fn ret_near_imm(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let imm = self.fetchword(bus);
        self.ip = self.pop(bus);
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        self.clk(11 + penalty);
    }

    fn ret_far(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(2);
        self.ip = self.pop(bus);
        self.sregs[SegReg16::CS as usize] = self.pop(bus);
        self.clk(15 + penalty);
    }

    fn ret_far_imm(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(2);
        let imm = self.fetchword(bus);
        self.ip = self.pop(bus);
        self.sregs[SegReg16::CS as usize] = self.pop(bus);
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        self.clk(15 + penalty);
    }

    fn pushf(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let flags_val = self.flags.compress();
        self.push(bus, flags_val);
        self.clk(3 + penalty);
    }

    fn popf(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(1);
        let val = self.pop(bus);
        let mf = self.flags.mf;
        self.flags.expand(val);
        self.flags.mf = mf;
        self.clk(5 + penalty);
    }

    fn sahf(&mut self) {
        let ah = self.regs.byte(ByteReg::AH);
        self.flags.carry_val = (ah & 0x01) as u32;
        self.flags.parity_val = if ah & 0x04 != 0 { 0 } else { 1 };
        self.flags.aux_val = (ah & 0x10) as u32;
        self.flags.zero_val = if ah & 0x40 != 0 { 0 } else { 1 };
        self.flags.sign_val = if ah & 0x80 != 0 { -1 } else { 0 };
        self.clk(2);
    }

    fn lahf(&mut self) {
        let flags_val = self.flags.compress() as u8;
        self.regs.set_byte(ByteReg::AH, flags_val);
        self.clk(2);
    }

    fn mov_al_moffs(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let addr = self.default_base(SegReg16::DS).wrapping_add(offset as u32) & 0xFFFFF;
        let val = bus.read_byte(addr);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(5);
    }

    fn mov_aw_moffs(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let base = self.default_base(SegReg16::DS);
        self.eo = offset;
        self.ea = base.wrapping_add(offset as u32) & 0xFFFFF;
        let val = self.seg_read_word(bus);
        self.regs.set_word(WordReg::AX, val);
        let penalty = if self.ea & 1 == 1 { 4 } else { 0 };
        self.clk(5 + penalty);
    }

    fn mov_moffs_al(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let addr = self.default_base(SegReg16::DS).wrapping_add(offset as u32) & 0xFFFFF;
        bus.write_byte(addr, self.regs.byte(ByteReg::AL));
        self.clk(3);
    }

    fn mov_moffs_aw(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let base = self.default_base(SegReg16::DS);
        self.eo = offset;
        self.ea = base.wrapping_add(offset as u32) & 0xFFFFF;
        self.seg_write_word(bus, self.regs.word(WordReg::AX));
        let penalty = if self.ea & 1 == 1 { 4 } else { 0 };
        self.clk(3 + penalty);
    }

    fn mov_byte_reg_imm(&mut self, reg: ByteReg, bus: &mut impl common::Bus) {
        let val = self.fetch(bus);
        self.regs.set_byte(reg, val);
        self.clk(2);
    }

    fn mov_word_reg_imm(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.fetchword(bus);
        self.regs.set_word(reg, val);
        self.clk(2);
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
            bus.write_byte(self.ea, val);
        }
        self.clk_modrm(modrm, 2, 3);
    }

    fn mov_rm_imm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            let val = self.fetchword(bus);
            let reg = self.rm_word(modrm);
            self.regs.set_word(reg, val);
        } else {
            self.calc_ea(modrm, bus);
            let val = self.fetchword(bus);
            self.seg_write_word(bus, val);
        }
        self.clk_modrm_word(modrm, 2, 3, 1);
    }

    fn les(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.calc_ea(modrm, bus);
        let offset = self.seg_read_word(bus);
        let segment = self.seg_read_word_at(bus, 2);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.sregs[SegReg16::ES as usize] = segment;
        let penalty = if self.ea & 1 == 1 { 8 } else { 0 };
        self.clk(3 + penalty);
    }

    fn lds(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.calc_ea(modrm, bus);
        let offset = self.seg_read_word(bus);
        let segment = self.seg_read_word_at(bus, 2);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.sregs[SegReg16::DS as usize] = segment;
        let penalty = if self.ea & 1 == 1 { 8 } else { 0 };
        self.clk(3 + penalty);
    }

    fn enter(&mut self, bus: &mut impl common::Bus) {
        let alloc = self.fetchword(bus);
        let level = self.fetch(bus);
        let sp_pen = self.sp_penalty(if level == 0 { 1 } else { 2 * level as i32 - 1 });
        let bp = self.regs.word(WordReg::BP);
        self.push(bus, bp);
        let frame_ptr = self.regs.word(WordReg::SP);
        if level > 0 {
            for _ in 1..level {
                let bp_val = self.regs.word(WordReg::BP).wrapping_sub(2);
                self.regs.set_word(WordReg::BP, bp_val);
                let base = self.seg_base(SegReg16::SS);
                let val = self.read_word_seg(bus, base, bp_val);
                self.push(bus, val);
            }
            self.push(bus, frame_ptr);
        }
        self.regs.set_word(WordReg::BP, frame_ptr);
        let sp = self.regs.word(WordReg::SP).wrapping_sub(alloc);
        self.regs.set_word(WordReg::SP, sp);
        if level == 0 {
            self.clk(11 + sp_pen);
        } else if level == 1 {
            self.clk(15 + sp_pen);
        } else {
            self.clk(12 + 4 * level as i32 + sp_pen);
        }
    }

    fn leave(&mut self, bus: &mut impl common::Bus) {
        let bp = self.regs.word(WordReg::BP);
        self.regs.set_word(WordReg::SP, bp);
        let penalty = self.sp_penalty(1);
        let val = self.pop(bus);
        self.regs.set_word(WordReg::BP, val);
        self.clk(5 + penalty);
    }

    fn int3(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(3);
        self.raise_interrupt(3, bus);
        self.clk(23 + penalty);
    }

    fn int_imm(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(3);
        let vector = self.fetch(bus);
        self.raise_interrupt(vector, bus);
        self.clk(23 + penalty);
    }

    fn into(&mut self, bus: &mut impl common::Bus) {
        if self.flags.of() {
            let penalty = self.sp_penalty(3);
            self.raise_interrupt(4, bus);
            self.clk(24 + penalty);
        } else {
            self.clk(4);
        }
    }

    fn iret(&mut self, bus: &mut impl common::Bus) {
        let penalty = self.sp_penalty(3);
        self.ip = self.pop(bus);
        self.sregs[SegReg16::CS as usize] = self.pop(bus);
        let flags_val = self.pop(bus);
        let mf = self.flags.mf;
        self.flags.expand(flags_val);
        self.flags.mf = mf;
        self.clk(31 + penalty);
    }

    fn loopne(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let cw = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, cw);
        if cw != 0 && !self.flags.zf() {
            self.ip = self.ip.wrapping_add(disp);
            self.clk(8);
        } else {
            self.clk(4);
        }
    }

    fn loope(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let cw = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, cw);
        if cw != 0 && self.flags.zf() {
            self.ip = self.ip.wrapping_add(disp);
            self.clk(8);
        } else {
            self.clk(4);
        }
    }

    fn loop_(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let cw = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, cw);
        if cw != 0 {
            self.ip = self.ip.wrapping_add(disp);
            self.clk(8);
        } else {
            self.clk(4);
        }
    }

    fn jcxz(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        if self.regs.word(WordReg::CX) == 0 {
            self.ip = self.ip.wrapping_add(disp);
            self.clk(8);
        } else {
            self.clk(4);
        }
    }

    fn in_al_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = bus.io_read_byte(port);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(5);
    }

    fn in_aw_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = bus.io_read_word(port);
        self.regs.set_word(WordReg::AX, val);
        let port_penalty = if port & 1 == 1 { 4 } else { 0 };
        self.clk(5 + port_penalty);
    }

    fn out_imm_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.regs.byte(ByteReg::AL);
        bus.io_write_byte(port, val);
        self.clk(3);
    }

    fn out_imm_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.regs.word(WordReg::AX);
        bus.io_write_word(port, val);
        let port_penalty = if port & 1 == 1 { 4 } else { 0 };
        self.clk(3 + port_penalty);
    }

    fn in_al_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = bus.io_read_byte(port);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(5);
    }

    fn in_aw_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = bus.io_read_word(port);
        self.regs.set_word(WordReg::AX, val);
        let port_penalty = if port & 1 == 1 { 4 } else { 0 };
        self.clk(5 + port_penalty);
    }

    fn out_dw_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.regs.byte(ByteReg::AL);
        bus.io_write_byte(port, val);
        self.clk(3);
    }

    fn out_dw_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.regs.word(WordReg::AX);
        bus.io_write_word(port, val);
        let port_penalty = if port & 1 == 1 { 4 } else { 0 };
        self.clk(3 + port_penalty);
    }

    fn xlat(&mut self, bus: &mut impl common::Bus) {
        let al = self.regs.byte(ByteReg::AL) as u16;
        let bw = self.regs.word(WordReg::BX);
        let addr = self
            .default_base(SegReg16::DS)
            .wrapping_add(bw.wrapping_add(al) as u32)
            & 0xFFFFF;
        let val = bus.read_byte(addr);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(5);
    }

    fn daa(&mut self, _bus: &mut impl common::Bus) {
        let old_al = self.regs.byte(ByteReg::AL);
        let old_cf = self.flags.cf();
        let old_af = self.flags.af();
        let mut al = old_al;
        if (old_al & 0x0F) > 9 || old_af {
            al = al.wrapping_add(6);
            self.flags.aux_val = 1;
        } else {
            self.flags.aux_val = 0;
        }
        let threshold = if old_af { 0x9F } else { 0x99 };
        if old_al > threshold || old_cf {
            al = al.wrapping_add(0x60);
            self.flags.carry_val = 1;
        }
        self.regs.set_byte(ByteReg::AL, al);
        self.flags.set_szpf_byte(al as u32);
        self.clk(3);
    }

    fn das(&mut self, _bus: &mut impl common::Bus) {
        let old_al = self.regs.byte(ByteReg::AL);
        let old_cf = self.flags.cf();
        let old_af = self.flags.af();
        let mut al = old_al;
        if (old_al & 0x0F) > 9 || old_af {
            al = al.wrapping_sub(6);
            self.flags.aux_val = 1;
        } else {
            self.flags.aux_val = 0;
        }
        let threshold = if old_af { 0x9F } else { 0x99 };
        if old_al > threshold || old_cf {
            al = al.wrapping_sub(0x60);
            self.flags.carry_val = 1;
        }
        self.regs.set_byte(ByteReg::AL, al);
        self.flags.set_szpf_byte(al as u32);
        self.clk(3);
    }

    fn aaa(&mut self, _bus: &mut impl common::Bus) {
        if (self.regs.byte(ByteReg::AL) & 0x0F) > 9 || self.flags.af() {
            let al = self.regs.byte(ByteReg::AL).wrapping_add(6);
            self.regs.set_byte(ByteReg::AL, al & 0x0F);
            let ah = self.regs.byte(ByteReg::AH).wrapping_add(1);
            self.regs.set_byte(ByteReg::AH, ah);
            self.flags.aux_val = 1;
            self.flags.carry_val = 1;
        } else {
            let al = self.regs.byte(ByteReg::AL) & 0x0F;
            self.regs.set_byte(ByteReg::AL, al);
            self.flags.aux_val = 0;
            self.flags.carry_val = 0;
        }
        self.clk(3);
    }

    fn aas(&mut self, _bus: &mut impl common::Bus) {
        if (self.regs.byte(ByteReg::AL) & 0x0F) > 9 || self.flags.af() {
            let al = self.regs.byte(ByteReg::AL).wrapping_sub(6);
            self.regs.set_byte(ByteReg::AL, al & 0x0F);
            let ah = self.regs.byte(ByteReg::AH).wrapping_sub(1);
            self.regs.set_byte(ByteReg::AH, ah);
            self.flags.aux_val = 1;
            self.flags.carry_val = 1;
        } else {
            let al = self.regs.byte(ByteReg::AL) & 0x0F;
            self.regs.set_byte(ByteReg::AL, al);
            self.flags.aux_val = 0;
            self.flags.carry_val = 0;
        }
        self.clk(3);
    }

    fn aam(&mut self, bus: &mut impl common::Bus) {
        let base = self.fetch(bus);
        if base == 0 {
            self.regs.set_byte(ByteReg::AH, 0xFF);
            let val = self.regs.byte(ByteReg::AL) as u32;
            self.flags.set_szpf_byte(val);
            self.clk(16);
            return;
        }
        let al = self.regs.byte(ByteReg::AL);
        self.regs.set_byte(ByteReg::AH, al / base);
        self.regs.set_byte(ByteReg::AL, al % base);
        let val = self.regs.byte(ByteReg::AL) as u32;
        self.flags.set_szpf_byte(val);
        self.clk(16);
    }

    fn aad(&mut self, bus: &mut impl common::Bus) {
        let _base = self.fetch(bus);
        let al = self.regs.byte(ByteReg::AL);
        let ah = self.regs.byte(ByteReg::AH);
        let result = al.wrapping_add(ah.wrapping_mul(10));
        self.regs.set_byte(ByteReg::AL, result);
        self.regs.set_byte(ByteReg::AH, 0);
        self.flags.set_szpf_byte(result as u32);
        self.clk(14);
    }

    fn fpu_escape(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm < 0xC0 {
            self.calc_ea(modrm, bus);
        }
        self.clk(2);
    }

    fn clc(&mut self) {
        self.flags.carry_val = 0;
        self.clk(2);
    }

    fn stc(&mut self) {
        self.flags.carry_val = 1;
        self.clk(2);
    }

    fn cli(&mut self) {
        self.flags.if_flag = false;
        self.clk(2);
    }

    fn sti(&mut self) {
        self.flags.if_flag = true;
        self.no_interrupt = 1;
        self.clk(2);
    }

    fn cld(&mut self) {
        self.flags.df = false;
        self.clk(2);
    }

    fn std(&mut self) {
        self.flags.df = true;
        self.clk(2);
    }

    fn cmc(&mut self) {
        self.flags.carry_val = if self.flags.cf() { 0 } else { 1 };
        self.clk(2);
    }

    fn hlt(&mut self) {
        self.halted = true;
        self.clk(2);
    }

    fn invalid(&mut self, bus: &mut impl common::Bus) {
        // sic! This is the correct way to handle invalid opcodes.
        // This is confirmed by the SST test data.
        let modrm = self.fetch(bus);
        self.get_rm_word(modrm, bus);
        self.clk(2);
    }
}
