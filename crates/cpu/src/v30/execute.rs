use super::{
    V30,
    biu::{ADDRESS_MASK, QUEUE_SIZE},
};
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
            0x40 => self.inc_word_reg(WordReg::AX, bus),
            0x41 => self.inc_word_reg(WordReg::CX, bus),
            0x42 => self.inc_word_reg(WordReg::DX, bus),
            0x43 => self.inc_word_reg(WordReg::BX, bus),
            0x44 => self.inc_word_reg(WordReg::SP, bus),
            0x45 => self.inc_word_reg(WordReg::BP, bus),
            0x46 => self.inc_word_reg(WordReg::SI, bus),
            0x47 => self.inc_word_reg(WordReg::DI, bus),

            // DEC word registers
            0x48 => self.dec_word_reg(WordReg::AX, bus),
            0x49 => self.dec_word_reg(WordReg::CX, bus),
            0x4A => self.dec_word_reg(WordReg::DX, bus),
            0x4B => self.dec_word_reg(WordReg::BX, bus),
            0x4C => self.dec_word_reg(WordReg::SP, bus),
            0x4D => self.dec_word_reg(WordReg::BP, bus),
            0x4E => self.dec_word_reg(WordReg::SI, bus),
            0x4F => self.dec_word_reg(WordReg::DI, bus),

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
            0x63 => self.undefined_63(bus),
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
                self.clk(bus, 1);
            }
            0x6D => {
                self.insw(bus);
                self.clk(bus, 1);
            }
            0x6E => {
                self.outsb(bus);
                self.clk(bus, -1);
            }
            0x6F => {
                self.outsw(bus);
                self.clk(bus, -1);
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
            0x90 => self.clk(bus, 2),
            0x91 => self.xchg_aw(WordReg::CX, bus),
            0x92 => self.xchg_aw(WordReg::DX, bus),
            0x93 => self.xchg_aw(WordReg::BX, bus),
            0x94 => self.xchg_aw(WordReg::SP, bus),
            0x95 => self.xchg_aw(WordReg::BP, bus),
            0x96 => self.xchg_aw(WordReg::SI, bus),
            0x97 => self.xchg_aw(WordReg::DI, bus),

            // CBW, CWD
            0x98 => self.cbw(bus),
            0x99 => self.cwd(bus),

            // CALL far, WAIT
            0x9A => self.call_far(bus),
            0x9B => self.clk(bus, 2), // WAIT
            0x9C => self.pushf(bus),
            0x9D => self.popf(bus),
            0x9E => self.sahf(bus),
            0x9F => self.lahf(bus),

            // MOV AL/AX, [addr] and [addr], AL/AX
            0xA0 => self.mov_al_moffs(bus),
            0xA1 => self.mov_aw_moffs(bus),
            0xA2 => self.mov_moffs_al(bus),
            0xA3 => self.mov_moffs_aw(bus),

            // String ops
            0xA4 => {
                self.movsb(bus);
                self.clk(bus, 1);
            }
            0xA5 => {
                self.movsw(bus);
                self.clk(bus, 1);
            }
            0xA6 => {
                self.cmpsb(bus);
                self.clk(bus, -1);
            }
            0xA7 => {
                self.cmpsw(bus);
                self.clk(bus, -1);
            }

            // TEST AL/AX, imm
            0xA8 => self.test_al_imm8(bus),
            0xA9 => self.test_aw_imm16(bus),

            // STOS, LODS, SCAS
            0xAA => self.stosb(bus),
            0xAB => self.stosw(bus),
            0xAC => {
                self.lodsb(bus);
                self.clk(bus, 1);
            }
            0xAD => {
                self.lodsw(bus);
                self.clk(bus, 1);
            }
            0xAE => {
                self.scasb(bus);
                self.clk(bus, -1);
            }
            0xAF => {
                self.scasw(bus);
                self.clk(bus, -1);
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
            0xD6 => self.xlat_d6(bus),

            // XLAT
            0xD7 => self.xlat_d7(bus),

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
            0xF4 => self.hlt(bus),

            // CMC
            0xF5 => self.cmc(bus),

            // Group 3 byte/word
            0xF6 => self.group_f6(bus),
            0xF7 => self.group_f7(bus),

            // CLC, STC, CLI, STI, CLD, STD
            0xF8 => self.clc(bus),
            0xF9 => self.stc(bus),
            0xFA => self.cli(bus),
            0xFB => self.sti(bus),
            0xFC => self.cld(bus),
            0xFD => self.std(bus),

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
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn add_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_add_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn add_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_add_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn add_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_add_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn add_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_add_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn add_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_add_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn or_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn or_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_or_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn or_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn or_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_or_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn or_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_or_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn or_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_or_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn adc_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn adc_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn adc_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn adc_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn adc_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn adc_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn sbb_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn sbb_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn sbb_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn sbb_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn sbb_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn sbb_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn and_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn and_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_and_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn and_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn and_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_and_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn and_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_and_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn and_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_and_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn sub_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn sub_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_sub_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn sub_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn sub_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_sub_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn sub_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_sub_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn sub_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_sub_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn xor_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn xor_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_xor_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn xor_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, 1, 7);
    }

    fn xor_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_xor_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, 1, 7);
    }

    fn xor_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_xor_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn xor_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_xor_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
        self.clk(bus, 1);
    }

    fn cmp_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(bus, modrm, 1, 6);
    }

    fn cmp_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        self.alu_sub_word(dst, src);
        self.clk_modrm_word(bus, modrm, 1, 6);
    }

    fn cmp_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(bus, modrm, 1, 6);
    }

    fn cmp_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        self.alu_sub_word(dst, src);
        self.clk_modrm_word(bus, modrm, 1, 6);
    }

    fn cmp_ald8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(dst, src);
        self.clk_accumulator_immediate_byte_tail(bus);
    }

    fn cmp_axd16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        self.alu_sub_word(dst, src);
        self.clk(bus, 1);
    }

    fn inc_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.regs.word(reg);
        let result = self.alu_inc_word(val);
        self.regs.set_word(reg, result);
        self.clk(bus, 1);
    }

    fn dec_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.regs.word(reg);
        let result = self.alu_dec_word(val);
        self.regs.set_word(reg, result);
        self.clk(bus, 1);
    }

    fn push_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.regs.word(reg);
        self.push(bus, val);
        self.clk(bus, 1);
    }

    pub(super) fn push_sp(&mut self, bus: &mut impl common::Bus) {
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        self.regs.set_word(WordReg::SP, sp);
        let base = self.seg_base(SegReg16::SS);
        self.write_word_seg(bus, base, sp, sp);
        self.clk(bus, 1);
    }

    fn pop_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.pop(bus);
        self.regs.set_word(reg, val);
        self.clk(bus, 0);
    }

    fn push_seg(&mut self, seg: SegReg16, bus: &mut impl common::Bus) {
        let val = self.sregs[seg as usize];
        self.push(bus, val);
        self.clk(bus, 1);
    }

    fn pop_seg(&mut self, seg: SegReg16, bus: &mut impl common::Bus) {
        let val = self.pop(bus);
        self.sregs[seg as usize] = val;
        self.clk(bus, 0);
    }

    fn pusha(&mut self, bus: &mut impl common::Bus) {
        let sp = self.regs.word(WordReg::SP);
        let aw = self.regs.word(WordReg::AX);
        self.push(bus, aw);
        self.clk(bus, 1);
        let cw = self.regs.word(WordReg::CX);
        self.push(bus, cw);
        self.clk(bus, 1);
        let dw = self.regs.word(WordReg::DX);
        self.push(bus, dw);
        self.clk(bus, 1);
        let bw = self.regs.word(WordReg::BX);
        self.push(bus, bw);
        self.clk(bus, 1);
        self.push(bus, sp);
        self.clk(bus, 1);
        let bp = self.regs.word(WordReg::BP);
        self.push(bus, bp);
        self.clk(bus, 1);
        let ix = self.regs.word(WordReg::SI);
        self.push(bus, ix);
        self.clk(bus, 1);
        let iy = self.regs.word(WordReg::DI);
        self.push(bus, iy);
        self.clk(bus, 1);
    }

    fn popa(&mut self, bus: &mut impl common::Bus) {
        let iy = self.pop(bus);
        self.regs.set_word(WordReg::DI, iy);
        self.biu_chain_eu_transfer();
        let ix = self.pop(bus);
        self.regs.set_word(WordReg::SI, ix);
        self.biu_chain_eu_transfer();
        let bp = self.pop(bus);
        self.regs.set_word(WordReg::BP, bp);
        let sp = self.regs.word(WordReg::SP).wrapping_add(2);
        self.regs.set_word(WordReg::SP, sp);
        self.biu_chain_eu_transfer();
        let bw = self.pop(bus);
        self.regs.set_word(WordReg::BX, bw);
        self.biu_chain_eu_transfer();
        let dw = self.pop(bus);
        self.regs.set_word(WordReg::DX, dw);
        self.biu_chain_eu_transfer();
        let cw = self.pop(bus);
        self.regs.set_word(WordReg::CX, cw);
        self.biu_chain_eu_transfer();
        let aw = self.pop(bus);
        self.regs.set_word(WordReg::AX, aw);
        self.clk(bus, 0);
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
        self.clk(bus, 4);
        self.biu_complete_code_fetch_for_eu();
        self.biu_ready_memory_read();
        let high = self.seg_read_word_at(bus, 2) as i16;
        if val < low || val > high {
            self.clk(bus, 5);
            self.raise_interrupt(5, bus);
        } else {
            self.clk(bus, ea_pen);
        }
    }

    fn push_imm16(&mut self, bus: &mut impl common::Bus) {
        let val = self.fetchword(bus);
        self.push(bus, val);
        self.clk(bus, 1);
    }

    fn push_imm8(&mut self, bus: &mut impl common::Bus) {
        let val = self.fetch(bus) as i8 as u16;
        self.push(bus, val);
        self.clk(bus, 1);
    }

    fn imul_r16w_imm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.get_rm_word_imul_imm(modrm, bus) as i16;
        let imm = self.fetchword(bus) as i16;
        let result = src as i32 * imm as i32;
        let cycles = Self::imul_imm16_cycles(src, imm);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result as u16);
        self.flags.carry_val = if !(-0x8000..=0x7FFF).contains(&result) {
            1
        } else {
            0
        };
        self.flags.overflow_val = self.flags.carry_val;
        self.clk_modrm_word(bus, modrm, cycles, cycles + 1);
    }

    fn imul_r16w_imm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.get_rm_word_imul_imm(modrm, bus) as i16;
        let imm = self.fetch(bus) as i8;
        let result = src as i32 * imm as i32;
        let cycles = Self::imul_imm8_cycles(src, imm);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result as u16);
        self.flags.carry_val = if !(-0x8000..=0x7FFF).contains(&result) {
            1
        } else {
            0
        };
        self.flags.overflow_val = self.flags.carry_val;
        self.clk_modrm_word(bus, modrm, cycles, cycles + 1);
    }

    fn get_rm_word_imul_imm(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u16 {
        if modrm >= 0xC0 {
            self.regs.word(self.rm_word(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.clk(bus, 1);
            self.seg_read_word(bus)
        }
    }

    #[inline(always)]
    fn imul_imm16_cycles(src: i16, imm: i16) -> i32 {
        if src.is_negative() ^ imm.is_negative() {
            40
        } else {
            36
        }
    }

    #[inline(always)]
    fn imul_imm8_cycles(src: i16, imm: i8) -> i32 {
        if src.is_negative() ^ imm.is_negative() {
            41
        } else {
            37
        }
    }

    fn jcc(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.ip = self.ip.wrapping_add(disp as u16);
            self.clk(bus, 7);
        } else {
            self.clk(bus, 3);
        }
    }

    fn jcc_swapped(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.ip = self.ip.wrapping_add(disp as u16);
            self.clk(bus, 7);
        } else {
            self.clk(bus, 3);
        }
    }

    fn test_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_and_byte(dst, src);
        self.clk_modrm(bus, modrm, 1, 6);
    }

    fn test_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        self.alu_and_word(dst, src);
        self.clk_modrm_word(bus, modrm, 1, 6);
    }

    fn test_al_imm8(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_and_byte(dst, src);
        self.clk(bus, 2);
    }

    fn test_aw_imm16(&mut self, bus: &mut impl common::Bus) {
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        self.alu_and_word(dst, src);
        self.clk(bus, 1);
    }

    fn xchg_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let reg = self.reg_byte(modrm);
        let reg_val = self.regs.byte(reg);
        let rm_val = self.get_rm_byte(modrm, bus);
        self.regs.set_byte(reg, rm_val);
        self.putback_rm_byte(modrm, reg_val, bus);
        self.clk_modrm(bus, modrm, 3, 5);
    }

    fn xchg_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let reg = self.reg_word(modrm);
        let reg_val = self.regs.word(reg);
        let rm_val = self.get_rm_word(modrm, bus);
        self.regs.set_word(reg, rm_val);
        self.putback_rm_word(modrm, reg_val, bus);
        self.clk_modrm_word(bus, modrm, 3, 5);
    }

    fn xchg_aw(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let aw = self.regs.word(WordReg::AX);
        let val = self.regs.word(reg);
        self.regs.set_word(WordReg::AX, val);
        self.regs.set_word(reg, aw);
        self.clk(bus, 2);
    }

    fn mov_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.regs.byte(self.reg_byte(modrm));
        self.put_rm_byte(modrm, val, bus);
        self.clk_modrm(bus, modrm, 1, 3);
    }

    fn mov_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.regs.word(self.reg_word(modrm));
        self.put_rm_word(modrm, val, bus);
        self.clk_modrm_word(bus, modrm, 1, 3);
    }

    fn mov_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_byte(modrm, bus);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, val);
        self.clk_modrm(bus, modrm, 1, 5);
    }

    fn mov_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_word(modrm, bus);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, val);
        self.clk_modrm_word(bus, modrm, 1, 5);
    }

    fn mov_rm_sreg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let seg = SegReg16::from_index((modrm >> 3) & 3);
        let val = self.sregs[seg as usize];
        self.put_rm_word(modrm, val, bus);
        self.clk_modrm_word(bus, modrm, 1, 3);
    }

    fn mov_sreg_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.get_rm_word(modrm, bus);
        let seg = SegReg16::from_index((modrm >> 3) & 3);
        self.sregs[seg as usize] = val;
        self.inhibit_all = 1;
        self.clk_modrm_word(bus, modrm, 1, 5);
    }

    fn lea(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.calc_ea(modrm, bus);
        let reg = self.reg_word(modrm);
        let val = self.eo;
        self.regs.set_word(reg, val);
        self.clk_ea_done(bus);
    }

    fn pop_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let val = self.pop(bus);
        self.put_rm_word(modrm, val, bus);
        if modrm >= 0xC0 {
            self.clk(bus, 5);
        } else {
            let ea_pen = if self.ea & 1 == 1 { 4 } else { 0 };
            self.clk(bus, 5 + ea_pen);
        }
    }

    fn cbw(&mut self, bus: &mut impl common::Bus) {
        let al = self.regs.byte(ByteReg::AL) as i8 as i16 as u16;
        self.regs.set_word(WordReg::AX, al);
        self.clk(bus, 1);
    }

    fn cwd(&mut self, bus: &mut impl common::Bus) {
        let aw = self.regs.word(WordReg::AX) as i16;
        self.regs
            .set_word(WordReg::DX, if aw < 0 { 0xFFFF } else { 0 });
        self.clk(bus, 4);
    }

    fn call_far(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let segment = self.fetchword(bus);
        let cs = self.sregs[SegReg16::CS as usize];
        self.push(bus, cs);
        self.push(bus, self.ip);
        self.ip = offset;
        self.sregs[SegReg16::CS as usize] = segment;
        self.clk(bus, 13);
    }

    fn call_near(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetchword(bus);
        if self.seg_prefix && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.clk(bus, 2);
            self.biu_ready_memory_write();
        } else {
            self.biu_fetch_suspend(bus);
            self.biu_complete_code_fetch_for_eu();
        }
        self.push(bus, self.ip);
        self.biu_bus_wait_finish(bus);
        self.ip = self.ip.wrapping_add(disp);
        self.restart_fetch_after_delay(bus, 0);
    }

    fn jmp_near(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetchword(bus);
        self.ip = self.ip.wrapping_add(disp);
        if self.seg_prefix && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.restart_fetch_after_delay(bus, 0);
        } else {
            self.restart_fetch_after_delay(bus, 2);
        }
    }

    fn jmp_far(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let segment = self.fetchword(bus);
        self.ip = offset;
        self.sregs[SegReg16::CS as usize] = segment;
        self.restart_fetch_after_delay(bus, 1);
    }

    fn jmp_short(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        self.ip = self.ip.wrapping_add(disp);
        if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            if self.seg_prefix {
                self.restart_fetch_after_delay(bus, 3);
            } else {
                self.clk(bus, 4);
                self.biu_complete_code_fetch_for_eu();
                self.restart_fetch_after_delay(bus, 1);
            }
        } else {
            self.restart_fetch_after_delay(bus, 2);
        }
    }

    fn ret_near(&mut self, bus: &mut impl common::Bus) {
        if self.seg_prefix && self.biu_instruction_entry_queue_len_for_timing() == 0 {
            self.clk(bus, 2);
            self.biu_complete_code_fetch_for_eu();
            self.biu_fetch_suspend(bus);
            self.clk(bus, 2);
            self.biu_ready_memory_read();
        }
        let ip = self.pop(bus);
        self.ip = ip;
        self.restart_fetch_after_delay(bus, 2);
    }

    fn ret_near_imm(&mut self, bus: &mut impl common::Bus) {
        let imm = self.fetchword(bus);
        if self.seg_prefix && self.biu_instruction_entry_queue_len_for_timing() > 0 {
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.biu_ready_memory_read();
        }
        let ip = self.pop(bus);
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        self.ip = ip;
        self.restart_fetch_after_delay(bus, 2);
    }

    fn ret_far(&mut self, bus: &mut impl common::Bus) {
        let ip = self.pop(bus);
        self.biu_chain_eu_transfer();
        let cs = self.pop(bus);
        self.ip = ip;
        self.sregs[SegReg16::CS as usize] = cs;
        self.restart_fetch_after_delay(bus, 3);
    }

    fn ret_far_imm(&mut self, bus: &mut impl common::Bus) {
        let imm = self.fetchword(bus);
        let ip = self.pop(bus);
        self.biu_chain_eu_transfer();
        let cs = self.pop(bus);
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        self.ip = ip;
        self.sregs[SegReg16::CS as usize] = cs;
        self.restart_fetch_after_delay(bus, 3);
    }

    fn pushf(&mut self, bus: &mut impl common::Bus) {
        let flags_val = self.flags.compress();
        self.push(bus, flags_val);
        self.clk(bus, 1);
    }

    fn popf(&mut self, bus: &mut impl common::Bus) {
        let val = self.pop(bus);
        let mf = self.flags.mf;
        self.flags.expand(val);
        self.flags.mf = mf;
        self.clk(bus, 0);
    }

    fn sahf(&mut self, bus: &mut impl common::Bus) {
        let ah = self.regs.byte(ByteReg::AH);
        self.flags.carry_val = (ah & 0x01) as u32;
        self.flags.parity_val = if ah & 0x04 != 0 { 0 } else { 1 };
        self.flags.aux_val = (ah & 0x10) as u32;
        self.flags.zero_val = if ah & 0x40 != 0 { 0 } else { 1 };
        self.flags.sign_val = if ah & 0x80 != 0 { -1 } else { 0 };
        self.clk(bus, 2);
    }

    fn lahf(&mut self, bus: &mut impl common::Bus) {
        let flags_val = self.flags.compress() as u8;
        self.regs.set_byte(ByteReg::AH, flags_val);
        self.clk(bus, 1);
    }

    fn mov_al_moffs(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let addr = self.default_base(SegReg16::DS).wrapping_add(offset as u32) & 0xFFFFF;
        if self.direct_moffs_prefetch_before_memory(bus) {
            self.biu_ready_memory_read();
        }
        let val = self.biu_read_u8_physical(bus, addr);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(bus, 0);
    }

    fn mov_aw_moffs(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let base = self.default_base(SegReg16::DS);
        self.eo = offset;
        self.ea = base.wrapping_add(offset as u32) & 0xFFFFF;
        if self.direct_moffs_prefetch_before_memory(bus) {
            self.biu_ready_memory_read();
        }
        let val = self.seg_read_word(bus);
        self.regs.set_word(WordReg::AX, val);
        self.clk(bus, 0);
    }

    fn mov_moffs_al(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let addr = self.default_base(SegReg16::DS).wrapping_add(offset as u32) & 0xFFFFF;
        if self.direct_moffs_prefetch_before_memory(bus) {
            self.biu_ready_memory_write();
        }
        self.biu_write_u8_physical(bus, addr, self.regs.byte(ByteReg::AL));
        self.clk(bus, 1);
    }

    fn mov_moffs_aw(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let base = self.default_base(SegReg16::DS);
        self.eo = offset;
        self.ea = base.wrapping_add(offset as u32) & 0xFFFFF;
        if self.direct_moffs_prefetch_before_memory(bus) {
            self.biu_ready_memory_write();
        }
        self.seg_write_word(bus, self.regs.word(WordReg::AX));
        self.clk(bus, 1);
    }

    fn direct_moffs_prefetch_before_memory(&mut self, bus: &mut impl common::Bus) -> bool {
        if !(self.seg_prefix && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE) {
            return false;
        }

        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }

        if self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
        }
        true
    }

    fn mov_byte_reg_imm(&mut self, reg: ByteReg, bus: &mut impl common::Bus) {
        let val = self.fetch(bus);
        self.regs.set_byte(reg, val);
        self.clk(bus, 2);
    }

    fn mov_word_reg_imm(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.fetchword(bus);
        self.regs.set_word(reg, val);
        self.clk(bus, 1);
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
            self.biu_write_u8_physical(bus, self.ea, val);
        }
        self.clk_modrm(bus, modrm, 2, 3);
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
        self.clk_modrm_word(bus, modrm, 2, 3);
    }

    fn les(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.calc_ea(modrm, bus);
        let offset = self.seg_read_word(bus);
        let segment = self.seg_read_word_at(bus, 2);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.sregs[SegReg16::ES as usize] = segment;
        self.clk(bus, 3);
    }

    fn lds(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        self.calc_ea(modrm, bus);
        let offset = self.seg_read_word(bus);
        let segment = self.seg_read_word_at(bus, 2);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.sregs[SegReg16::DS as usize] = segment;
        self.clk(bus, 3);
    }

    fn enter(&mut self, bus: &mut impl common::Bus) {
        let alloc = self.fetchword(bus);
        let bp = self.regs.word(WordReg::BP);
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        self.regs.set_word(WordReg::SP, sp);
        let base = self.seg_base(SegReg16::SS);
        self.write_memory_byte(bus, base.wrapping_add(sp as u32) & ADDRESS_MASK, bp as u8);
        let level = self.fetch(bus);
        self.biu_chain_eu_transfer();
        self.write_memory_byte(
            bus,
            base.wrapping_add(sp.wrapping_add(1) as u32) & ADDRESS_MASK,
            (bp >> 8) as u8,
        );
        let frame_ptr = self.regs.word(WordReg::SP);
        if level > 0 {
            self.clk(bus, 2);
            for _ in 1..level {
                let bp_val = self.regs.word(WordReg::BP).wrapping_sub(2);
                self.regs.set_word(WordReg::BP, bp_val);
                let base = self.seg_base(SegReg16::SS);
                let val = self.read_word_seg(bus, base, bp_val);
                self.biu_chain_eu_transfer();
                self.push(bus, val);
                self.biu_chain_eu_transfer();
            }
            self.push(bus, frame_ptr);
        }
        self.regs.set_word(WordReg::BP, frame_ptr);
        let sp = self.regs.word(WordReg::SP).wrapping_sub(alloc);
        self.regs.set_word(WordReg::SP, sp);
        self.clk(bus, 1);
    }

    fn leave(&mut self, bus: &mut impl common::Bus) {
        let bp = self.regs.word(WordReg::BP);
        self.regs.set_word(WordReg::SP, bp);
        let val = self.pop(bus);
        self.regs.set_word(WordReg::BP, val);
        self.clk(bus, 0);
    }

    fn int3(&mut self, bus: &mut impl common::Bus) {
        self.raise_software_interrupt(3, bus, 5);
    }

    fn int_imm(&mut self, bus: &mut impl common::Bus) {
        let vector = self.fetch(bus);
        self.raise_software_interrupt(vector, bus, 6);
    }

    fn into(&mut self, bus: &mut impl common::Bus) {
        if self.flags.of() {
            self.raise_software_interrupt(4, bus, 6);
        } else {
            self.clk(bus, 4);
        }
    }

    fn iret(&mut self, bus: &mut impl common::Bus) {
        let ip = self.pop(bus);
        self.biu_chain_eu_transfer();
        let cs = self.pop(bus);
        self.biu_chain_eu_transfer();
        let flags_val = self.pop(bus);
        let mf = self.flags.mf;
        self.ip = ip;
        self.sregs[SegReg16::CS as usize] = cs;
        self.flags.expand(flags_val);
        self.flags.mf = mf;
        self.restart_fetch_after_delay(bus, 0);
    }

    fn loopne(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let counter = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, counter);
        if counter != 0 && !self.flags.zf() {
            self.ip = self.ip.wrapping_add(disp);
            self.loop_branch_restart(bus, true);
        } else {
            self.loop_not_taken_tail(bus);
        }
    }

    fn loope(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let counter = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, counter);
        if counter != 0 && self.flags.zf() {
            self.ip = self.ip.wrapping_add(disp);
            self.loop_branch_restart(bus, true);
        } else {
            self.loop_not_taken_tail(bus);
        }
    }

    fn loop_(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let counter = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, counter);
        if counter != 0 {
            self.ip = self.ip.wrapping_add(disp);
            self.loop_branch_restart(bus, false);
        } else {
            self.loop_not_taken_tail(bus);
        }
    }

    fn jcxz(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        if self.regs.word(WordReg::CX) == 0 {
            self.ip = self.ip.wrapping_add(disp);
            self.loop_branch_restart(bus, true);
        } else {
            self.loop_not_taken_tail(bus);
        }
    }

    fn loop_not_taken_tail(&mut self, bus: &mut impl common::Bus) {
        let entry_queue_was_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
        let cycles = if self.seg_prefix && entry_queue_was_full {
            2
        } else {
            3
        };
        self.clk(bus, cycles);
    }

    fn loop_branch_restart(&mut self, bus: &mut impl common::Bus, conditional: bool) {
        let entry_queue_was_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
        let (prefetch_cycles, restart_delay) =
            match (conditional, self.seg_prefix, entry_queue_was_full) {
                (false, false, true) => (8, 0),
                (false, true, true) => (5, 2),
                (false, _, false) => (6, 1),
                (true, false, true) => (8, 1),
                (true, true, true) => (5, 3),
                (true, _, false) => (6, 0),
            };

        self.clk(bus, prefetch_cycles);
        self.biu_complete_code_fetch_for_eu();
        self.biu_fetch_suspend(bus);
        self.clk(bus, restart_delay);
        self.prefetch_ip = self.ip;
        self.biu_queue_flush_and_start_code_fetch_for_eu();
    }

    fn in_al_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.biu_io_read_u8(bus, port);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(bus, 0);
    }

    fn in_aw_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.biu_io_read_u16(bus, port);
        self.regs.set_word(WordReg::AX, val);
        self.clk(bus, 0);
    }

    fn out_imm_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.regs.byte(ByteReg::AL);
        self.biu_io_write_u8(bus, port, val);
        self.clk(bus, 1);
    }

    fn out_imm_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.regs.word(WordReg::AX);
        self.biu_io_write_u16(bus, port, val);
        self.clk(bus, 1);
    }

    fn in_al_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.biu_io_read_u8(bus, port);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(bus, 0);
    }

    fn in_aw_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.biu_io_read_u16(bus, port);
        self.regs.set_word(WordReg::AX, val);
        self.clk(bus, 0);
    }

    fn out_dw_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.regs.byte(ByteReg::AL);
        self.biu_io_write_u8(bus, port, val);
        self.clk(bus, 1);
    }

    fn out_dw_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.regs.word(WordReg::AX);
        self.biu_io_write_u16(bus, port, val);
        self.clk(bus, 1);
    }

    fn xlat_d6(&mut self, bus: &mut impl common::Bus) {
        let delay_before_read = if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            if self.seg_prefix { 19 } else { 20 }
        } else {
            20
        };
        self.xlat(bus, delay_before_read, true, false);
    }

    fn xlat_d7(&mut self, bus: &mut impl common::Bus) {
        let (delay_before_read, ready_memory_read, suspend_fetch_before_delay) = if self.seg_prefix
        {
            if self.queue_len() >= 2 {
                (2, true, true)
            } else {
                (4, false, false)
            }
        } else if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            (0, false, false)
        } else {
            (4, false, false)
        };
        self.xlat(
            bus,
            delay_before_read,
            ready_memory_read,
            suspend_fetch_before_delay,
        );
    }

    fn xlat(
        &mut self,
        bus: &mut impl common::Bus,
        delay_before_read: i32,
        ready_memory_read: bool,
        suspend_fetch_before_delay: bool,
    ) {
        if suspend_fetch_before_delay {
            self.biu_fetch_suspend(bus);
        }
        self.clk(bus, delay_before_read);
        if ready_memory_read {
            self.biu_ready_memory_read();
        }
        let al = self.regs.byte(ByteReg::AL) as u16;
        let bw = self.regs.word(WordReg::BX);
        let addr = self
            .default_base(SegReg16::DS)
            .wrapping_add(bw.wrapping_add(al) as u32)
            & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, addr);
        self.regs.set_byte(ByteReg::AL, val);
    }

    fn daa(&mut self, bus: &mut impl common::Bus) {
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
        self.clk(bus, 2);
    }

    fn das(&mut self, bus: &mut impl common::Bus) {
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
        self.clk(bus, 2);
    }

    fn aaa(&mut self, bus: &mut impl common::Bus) {
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
        self.clk(bus, 6);
    }

    fn aas(&mut self, bus: &mut impl common::Bus) {
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
        self.clk(bus, 6);
    }

    fn aam(&mut self, bus: &mut impl common::Bus) {
        let base = self.fetch(bus);
        let cycles = if self.biu_instruction_entry_queue_len_for_timing() == 0 {
            12
        } else {
            13
        };
        if base == 0 {
            self.regs.set_byte(ByteReg::AH, 0xFF);
            let val = self.regs.byte(ByteReg::AL) as u32;
            self.flags.set_szpf_byte(val);
            self.clk(bus, cycles);
            return;
        }
        let al = self.regs.byte(ByteReg::AL);
        self.regs.set_byte(ByteReg::AH, al / base);
        self.regs.set_byte(ByteReg::AL, al % base);
        let val = self.regs.byte(ByteReg::AL) as u32;
        self.flags.set_szpf_byte(val);
        self.clk(bus, cycles);
    }

    fn aad(&mut self, bus: &mut impl common::Bus) {
        let _base = self.fetch(bus);
        let al = self.regs.byte(ByteReg::AL);
        let ah = self.regs.byte(ByteReg::AH);
        let result = al.wrapping_add(ah.wrapping_mul(10));
        self.regs.set_byte(ByteReg::AL, result);
        self.regs.set_byte(ByteReg::AH, 0);
        self.flags.set_szpf_byte(result as u32);
        let cycles = if self.biu_instruction_entry_queue_len_for_timing() == 0 {
            5
        } else {
            6
        };
        self.clk(bus, cycles);
    }

    fn fpu_escape(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fpu_fetch_modrm(bus);
        if modrm < 0xC0 {
            self.calc_ea(modrm, bus);
            let instruction_len = self.ip.wrapping_sub(self.prev_ip) as usize;
            let prefixed_full_entry =
                self.seg_prefix && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
            let prefetch_before_read = prefixed_full_entry && instruction_len == 4;
            let suspend_fetch_before_read =
                self.queue_len() == 0 || (prefixed_full_entry && instruction_len == 3);

            if prefetch_before_read {
                self.biu_complete_code_fetch_for_eu();
                if self.queue_has_room_for_fetch() {
                    self.biu_start_code_fetch_for_eu();
                    self.clk(bus, 4);
                    self.biu_complete_code_fetch_for_eu();
                }
                self.biu_ready_memory_read();
            } else if suspend_fetch_before_read {
                self.biu_fetch_suspend(bus);
                self.biu_complete_code_fetch_for_eu();
                self.biu_prepare_memory_read();
                self.clk(bus, 2);
                self.biu_ready_memory_read();
            }
            let _ = self.seg_read_word(bus);
            if suspend_fetch_before_read {
                self.biu_fetch_resume_immediate_for_eu();
            }
            self.clk(bus, 1);
        } else if !self.instruction_entry_queue_had_current_instruction() {
            self.clk(bus, 2);
        }
    }

    fn fpu_fetch_modrm(&mut self, bus: &mut impl common::Bus) -> u8 {
        if self.seg_prefix
            && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
            && self.queue_len() > 0
            && self.instruction_queue[0] >= 0xC0
        {
            let modrm = self.queue_pop();
            self.biu_fetch_on_queue_read();
            self.ip = self.ip.wrapping_add(1);
            modrm
        } else {
            self.fetch(bus)
        }
    }

    fn clc(&mut self, bus: &mut impl common::Bus) {
        self.flags.carry_val = 0;
        self.clk(bus, 1);
    }

    fn stc(&mut self, bus: &mut impl common::Bus) {
        self.flags.carry_val = 1;
        self.clk(bus, 1);
    }

    fn cli(&mut self, bus: &mut impl common::Bus) {
        self.flags.if_flag = false;
        self.clk(bus, 1);
    }

    fn sti(&mut self, bus: &mut impl common::Bus) {
        self.flags.if_flag = true;
        self.no_interrupt = 1;
        self.clk(bus, 1);
    }

    fn cld(&mut self, bus: &mut impl common::Bus) {
        self.flags.df = false;
        self.clk(bus, 1);
    }

    fn std(&mut self, bus: &mut impl common::Bus) {
        self.flags.df = true;
        self.clk(bus, 1);
    }

    fn cmc(&mut self, bus: &mut impl common::Bus) {
        self.flags.carry_val = if self.flags.cf() { 0 } else { 1 };
        self.clk(bus, 1);
    }

    fn hlt(&mut self, bus: &mut impl common::Bus) {
        self.halted = true;
        self.clk(bus, 2);
    }

    fn invalid(&mut self, bus: &mut impl common::Bus) {
        // sic! This is the correct way to handle invalid opcodes.
        // This is confirmed by the SST test data.
        let modrm = self.fetch(bus);
        self.get_rm_word(modrm, bus);
        self.clk(bus, 2);
    }

    fn undefined_63(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            let _ = self.regs.word(self.rm_word(modrm));
        } else {
            self.calc_ea(modrm, bus);
            let delayed_memory_read = self.undefined_63_pre_memory_delay();
            if self.undefined_63_prefetch_before_memory() {
                if self.biu_latch_is_code_fetch() {
                    self.biu_fetch_suspend(bus);
                    self.biu_complete_code_fetch_for_eu();
                }
                if self.queue_has_room_for_fetch() {
                    self.biu_start_code_fetch_for_eu();
                    self.clk(bus, 4);
                    self.biu_complete_code_fetch_for_eu();
                }
                self.biu_ready_memory_read();
            } else if delayed_memory_read {
                if self.biu_latch_is_code_fetch() {
                    self.biu_fetch_suspend(bus);
                    self.biu_complete_code_fetch_for_eu();
                }
                self.biu_prepare_memory_read();
                self.clk(bus, 2);
                self.biu_ready_memory_read();
            }
            let _ = self.seg_read_word(bus);
            if delayed_memory_read {
                self.biu_fetch_resume_immediate_for_eu();
            }
        }
        self.clk(bus, 49);
    }

    fn undefined_63_prefetch_before_memory(&self) -> bool {
        let opcode_len = self.ip.wrapping_sub(self.opcode_start_ip);
        self.seg_prefix
            && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
            && opcode_len == 3
    }

    fn undefined_63_pre_memory_delay(&self) -> bool {
        let opcode_len = self.ip.wrapping_sub(self.opcode_start_ip);
        self.biu_instruction_entry_queue_len_for_timing() < QUEUE_SIZE
            || opcode_len >= 4
            || self.undefined_63_prefixed_full_queue_no_displacement()
    }

    fn undefined_63_prefixed_full_queue_no_displacement(&self) -> bool {
        let opcode_len = self.ip.wrapping_sub(self.opcode_start_ip);
        self.seg_prefix
            && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
            && opcode_len == 2
    }
}
