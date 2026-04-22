use super::{
    I8086, StepFinishCycle,
    biu::{ADDRESS_MASK, FetchState},
};
use crate::{ByteReg, SegReg16, WordReg};

impl I8086 {
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
            0x0F => self.pop_cs(bus),

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
            0x26 => self.segment_prefix(bus, SegReg16::ES),
            0x27 => self.daa(bus),

            // SUB
            0x28 => self.sub_br8(bus),
            0x29 => self.sub_wr16(bus),
            0x2A => self.sub_r8b(bus),
            0x2B => self.sub_r16w(bus),
            0x2C => self.sub_ald8(bus),
            0x2D => self.sub_axd16(bus),
            0x2E => self.segment_prefix(bus, SegReg16::CS),
            0x2F => self.das(bus),

            // XOR
            0x30 => self.xor_br8(bus),
            0x31 => self.xor_wr16(bus),
            0x32 => self.xor_r8b(bus),
            0x33 => self.xor_r16w(bus),
            0x34 => self.xor_ald8(bus),
            0x35 => self.xor_axd16(bus),
            0x36 => self.segment_prefix(bus, SegReg16::SS),
            0x37 => self.aaa(bus),

            // CMP
            0x38 => self.cmp_br8(bus),
            0x39 => self.cmp_wr16(bus),
            0x3A => self.cmp_r8b(bus),
            0x3B => self.cmp_r16w(bus),
            0x3C => self.cmp_ald8(bus),
            0x3D => self.cmp_axd16(bus),
            0x3E => self.segment_prefix(bus, SegReg16::DS),
            0x3F => self.aas(bus),

            // INC word registers
            0x40 => self.inc_word_reg(bus, WordReg::AX),
            0x41 => self.inc_word_reg(bus, WordReg::CX),
            0x42 => self.inc_word_reg(bus, WordReg::DX),
            0x43 => self.inc_word_reg(bus, WordReg::BX),
            0x44 => self.inc_word_reg(bus, WordReg::SP),
            0x45 => self.inc_word_reg(bus, WordReg::BP),
            0x46 => self.inc_word_reg(bus, WordReg::SI),
            0x47 => self.inc_word_reg(bus, WordReg::DI),

            // DEC word registers
            0x48 => self.dec_word_reg(bus, WordReg::AX),
            0x49 => self.dec_word_reg(bus, WordReg::CX),
            0x4A => self.dec_word_reg(bus, WordReg::DX),
            0x4B => self.dec_word_reg(bus, WordReg::BX),
            0x4C => self.dec_word_reg(bus, WordReg::SP),
            0x4D => self.dec_word_reg(bus, WordReg::BP),
            0x4E => self.dec_word_reg(bus, WordReg::SI),
            0x4F => self.dec_word_reg(bus, WordReg::DI),

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

            // Jcc aliases on 8086
            0x60 => self.jcc(bus, self.flags.of()),
            0x61 => self.jcc(bus, !self.flags.of()),
            0x62 => self.jcc(bus, self.flags.cf()),
            0x63 => self.jcc(bus, !self.flags.cf()),
            0x64 => self.jcc(bus, self.flags.zf()),
            0x65 => self.jcc(bus, !self.flags.zf()),
            0x66 => self.jcc(bus, self.flags.cf() || self.flags.zf()),
            0x67 => self.jcc_swapped(bus, !self.flags.cf() && !self.flags.zf()),
            0x68 => self.jcc(bus, self.flags.sf()),
            0x69 => self.jcc(bus, !self.flags.sf()),
            0x6A => self.jcc(bus, self.flags.pf()),
            0x6B => self.jcc(bus, !self.flags.pf()),
            0x6C => self.jcc(bus, self.flags.sf() != self.flags.of()),
            0x6D => self.jcc_swapped(bus, self.flags.sf() == self.flags.of()),
            0x6E => self.jcc(bus, self.flags.zf() || (self.flags.sf() != self.flags.of())),
            0x6F => self.jcc_swapped(
                bus,
                !self.flags.zf() && (self.flags.sf() == self.flags.of()),
            ),

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
            0x90 => self.clk(bus, self.queue_resident_eu_cycles(3, 0)),
            0x91 => self.xchg_aw(bus, WordReg::CX),
            0x92 => self.xchg_aw(bus, WordReg::DX),
            0x93 => self.xchg_aw(bus, WordReg::BX),
            0x94 => self.xchg_aw(bus, WordReg::SP),
            0x95 => self.xchg_aw(bus, WordReg::BP),
            0x96 => self.xchg_aw(bus, WordReg::SI),
            0x97 => self.xchg_aw(bus, WordReg::DI),

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

            // RET near aliases on 8086
            0xC0 => self.ret_near_imm(bus),
            0xC1 => self.ret_near(bus),

            // RET near imm16, RET near
            0xC2 => self.ret_near_imm(bus),
            0xC3 => self.ret_near(bus),

            // LES, LDS
            0xC4 => self.les(bus),
            0xC5 => self.lds(bus),

            // MOV r/m, imm
            0xC6 => self.mov_rm_imm8(bus),
            0xC7 => self.mov_rm_imm16(bus),

            // RET far aliases on 8086
            0xC8 => self.ret_far_imm(bus),
            0xC9 => self.ret_far(bus),

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

            // SALC / SETALC
            0xD6 => self.salc(bus),

            // XLAT
            0xD7 => self.xlat(bus),

            // FPU escape (NOP on I8086)
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

            0xF0 => self.lock_prefix(bus),
            0xF1 => self.lock_prefix(bus),

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
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_add_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn add_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_add_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn add_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_add_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn add_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_add_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn add_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_add_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn add_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_add_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
    }

    fn or_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn or_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_or_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn or_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_or_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn or_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_or_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn or_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_or_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn or_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_or_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
    }

    fn adc_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn adc_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn adc_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn adc_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn adc_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn adc_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let cf = self.flags.cf_val();
        let result = self.alu_adc_word(dst, src, cf);
        self.regs.set_word(WordReg::AX, result);
    }

    fn sbb_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn sbb_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn sbb_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn sbb_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn sbb_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_byte(dst, src, cf);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn sbb_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let cf = self.flags.cf_val();
        let result = self.alu_sbb_word(dst, src, cf);
        self.regs.set_word(WordReg::AX, result);
    }

    fn and_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn and_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_and_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn and_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_and_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn and_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_and_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn and_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_and_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn and_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_and_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
    }

    fn sub_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn sub_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_sub_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn sub_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_sub_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn sub_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_sub_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn sub_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_sub_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn sub_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_sub_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
    }

    fn xor_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        self.putback_rm_byte(modrm, result, bus);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn xor_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        let result = self.alu_xor_word(dst, src);
        self.putback_rm_word(modrm, result, bus);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles + 2);
    }

    fn xor_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        let result = self.alu_xor_byte(dst, src);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, result);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn xor_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        let result = self.alu_xor_word(dst, src);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, result);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn xor_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        let result = self.alu_xor_byte(dst, src);
        self.regs.set_byte(ByteReg::AL, result);
        self.clk(bus, 1);
    }

    fn xor_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        let result = self.alu_xor_word(dst, src);
        self.regs.set_word(WordReg::AX, result);
    }

    fn cmp_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn cmp_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        self.alu_sub_word(dst, src);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn cmp_r8b(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.byte(self.reg_byte(modrm));
        let src = self.get_rm_byte(modrm, bus);
        self.alu_sub_byte(dst, src);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn cmp_r16w(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let dst = self.regs.word(self.reg_word(modrm));
        let src = self.get_rm_word(modrm, bus);
        self.alu_sub_word(dst, src);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn cmp_ald8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(dst, src);
        self.clk(bus, 1);
    }

    fn cmp_axd16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        self.alu_sub_word(dst, src);
    }

    fn inc_word_reg(&mut self, bus: &mut impl common::Bus, reg: WordReg) {
        let val = self.regs.word(reg);
        let result = self.alu_inc_word(val);
        self.regs.set_word(reg, result);
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn dec_word_reg(&mut self, bus: &mut impl common::Bus, reg: WordReg) {
        let val = self.regs.word(reg);
        let result = self.alu_dec_word(val);
        self.regs.set_word(reg, result);
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn push_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.regs.word(reg);
        self.push(bus, val);
        self.clk(bus, 3 + self.odd_queue_start_penalty());
    }

    pub(super) fn push_sp(&mut self, bus: &mut impl common::Bus) {
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        self.regs.set_word(WordReg::SP, sp);
        self.write_word_seg(bus, SegReg16::SS, sp, sp);
        self.clk(bus, 3 + self.odd_queue_start_penalty());
    }

    fn pop_word_reg(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.pop(bus);
        self.regs.set_word(reg, val);
        self.clk(bus, 0);
    }

    fn push_seg(&mut self, seg: SegReg16, bus: &mut impl common::Bus) {
        let val = self.sregs[seg as usize];
        self.push(bus, val);
        self.clk(bus, 3 + self.odd_queue_start_penalty());
    }

    fn pop_seg(&mut self, seg: SegReg16, bus: &mut impl common::Bus) {
        let val = self.pop(bus);
        self.sregs[seg as usize] = val;
        self.clk(bus, 0);
    }

    fn pop_cs(&mut self, bus: &mut impl common::Bus) {
        self.pop_seg(SegReg16::CS, bus);
    }

    fn jcc(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.set_ip_and_flush(bus, self.ip.wrapping_add(disp as u16));
            let total_cycles = if self.opcode_started_at_odd_address() {
                17
            } else {
                19
            };
            self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 1));
        } else {
            self.clk(bus, self.queue_resident_eu_cycles(4, 1));
        }
    }

    fn jcc_swapped(&mut self, bus: &mut impl common::Bus, condition: bool) {
        let disp = self.fetch(bus) as i8;
        if condition {
            self.set_ip_and_flush(bus, self.ip.wrapping_add(disp as u16));
            let total_cycles = if self.opcode_started_at_odd_address() {
                17
            } else {
                19
            };
            self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 1));
        } else {
            self.clk(bus, self.queue_resident_eu_cycles(4, 1));
        }
    }

    fn test_br8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.byte(self.reg_byte(modrm));
        let dst = self.get_rm_byte(modrm, bus);
        self.alu_and_byte(dst, src);
        self.clk_modrm(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn test_wr16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let prefix_cycles = 0;
        let modrm = self.fetch_modrm(bus);
        let src = self.regs.word(self.reg_word(modrm));
        let dst = self.get_rm_word(modrm, bus);
        self.alu_and_word(dst, src);
        self.clk_modrm_word(bus, modrm, prefix_cycles, prefix_cycles);
    }

    fn test_al_imm8(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetch(bus);
        let dst = self.regs.byte(ByteReg::AL);
        self.alu_and_byte(dst, src);
        self.clk(bus, 1);
    }

    fn test_aw_imm16(&mut self, bus: &mut impl common::Bus) {
        self.charge_seg_prefix_cycle(bus);
        let src = self.fetchword(bus);
        let dst = self.regs.word(WordReg::AX);
        self.alu_and_word(dst, src);
    }

    fn xchg_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let reg = self.reg_byte(modrm);
        let reg_val = self.regs.byte(reg);
        let rm_val = self.get_rm_byte(modrm, bus);
        self.regs.set_byte(reg, rm_val);
        self.putback_rm_byte(modrm, reg_val, bus);
        self.clk_modrm(bus, modrm, 1, 3);
    }

    fn xchg_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let reg = self.reg_word(modrm);
        let reg_val = self.regs.word(reg);
        let rm_val = self.get_rm_word(modrm, bus);
        self.regs.set_word(reg, rm_val);
        self.putback_rm_word(modrm, reg_val, bus);
        self.clk_modrm_word(bus, modrm, 1, 3);
    }

    fn xchg_aw(&mut self, bus: &mut impl common::Bus, reg: WordReg) {
        let aw = self.regs.word(WordReg::AX);
        let val = self.regs.word(reg);
        self.regs.set_word(WordReg::AX, val);
        self.regs.set_word(reg, aw);
        self.clk(bus, self.queue_resident_eu_cycles(3, 0));
    }

    fn mov_br8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let val = self.regs.byte(self.reg_byte(modrm));
        if modrm >= 0xC0 {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            self.putback_rm_byte(modrm, val, bus);
        } else {
            self.resolve_rm_address(modrm);
            self.charge_rm_eadone(modrm, bus);
            self.putback_rm_byte(modrm, val, bus);
            if self.mov_rm_reg_store_uses_terminal_writeback_rni(modrm) {
                self.finish_on_terminal_writeback(modrm);
            } else {
                self.finish_on_terminal_writeback_with_fetch(modrm);
            }
        }
        self.clk_modrm(bus, modrm, 0, 1);
    }

    fn mov_wr16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let val = self.regs.word(self.reg_word(modrm));
        if modrm >= 0xC0 {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            self.putback_rm_word(modrm, val, bus);
        } else {
            self.resolve_rm_address(modrm);
            self.charge_rm_eadone(modrm, bus);
            self.putback_rm_word(modrm, val, bus);
            if self.mov_rm_reg_store_uses_terminal_writeback_rni(modrm) {
                self.finish_on_terminal_writeback(modrm);
            } else {
                self.finish_on_terminal_writeback_with_fetch(modrm);
            }
        }
        self.clk_modrm_word(bus, modrm, 0, 0);
    }

    fn mov_r8b(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let val = self.get_rm_byte(modrm, bus);
        let reg = self.reg_byte(modrm);
        self.regs.set_byte(reg, val);
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        self.clk_modrm(bus, modrm, 0, 0);
    }

    fn mov_r16w(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let val = self.get_rm_word(modrm, bus);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, val);
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        self.clk_modrm_word(bus, modrm, 0, 0);
    }

    fn mov_rm_sreg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let seg = SegReg16::from_index((modrm >> 3) & 3);
        let val = self.sregs[seg as usize];
        if modrm >= 0xC0 {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            self.putback_rm_word(modrm, val, bus);
        } else {
            self.resolve_rm_address(modrm);
            self.charge_rm_eadone(modrm, bus);
            self.putback_rm_word(modrm, val, bus);
            if self.mov_rm_sreg_uses_inline_commit(modrm) {
                self.finish_on_terminal_writeback_inline_commit(modrm);
            } else if self.mov_rm_sreg_uses_terminal_writeback_with_fetch(modrm) {
                self.finish_on_terminal_writeback_with_fetch(modrm);
            }
        }
        self.clk_modrm_word(bus, modrm, 0, 0);
    }

    fn mov_sreg_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        let val = self.get_rm_word(modrm, bus);
        let seg = SegReg16::from_index((modrm >> 3) & 3);
        self.sregs[seg as usize] = val;
        self.inhibit_all = 1;
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        self.clk_modrm_word(bus, modrm, 0, 0);
    }

    fn lea(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        self.calc_ea(modrm);
        self.charge_rm_eadone(modrm, bus);
        let reg = self.reg_word(modrm);
        let val = self.eo;
        self.regs.set_word(reg, val);
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
    }

    fn pop_rm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        if modrm < 0xC0 {
            self.resolve_rm_address(modrm);
            self.charge_rm_eadone(modrm, bus);
        }
        self.clk(bus, 1);
        let val = self.pop(bus);
        self.clk(bus, 1);
        if modrm >= 0xC0 {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        } else {
            self.clk(bus, 1);
        }
        self.putback_rm_word(modrm, val, bus);
        self.finish_on_terminal_writeback(modrm);
    }

    fn cbw(&mut self, bus: &mut impl common::Bus) {
        let al = self.regs.byte(ByteReg::AL) as i8 as i16 as u16;
        self.regs.set_word(WordReg::AX, al);
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn cwd(&mut self, bus: &mut impl common::Bus) {
        let aw = self.regs.word(WordReg::AX) as i16;
        self.regs
            .set_word(WordReg::DX, if aw < 0 { 0xFFFF } else { 0 });
        self.clk(
            bus,
            self.queue_resident_eu_cycles(if aw < 0 { 6 } else { 5 }, 0),
        );
    }

    fn call_far(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let segment = self.fetchword(bus);
        if self.far_immediate_call_uses_drained_queue_handoff() {
            self.clk(bus, 1);
            self.farcall_routine(bus, segment, offset, true, true);
        } else if self.far_immediate_jump_uses_preloaded_finish() {
            self.farcall_routine(bus, segment, offset, true, true);
        } else {
            self.farcall_routine(bus, segment, offset, true, false);
        }
    }

    fn call_near(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetchword(bus);
        let new_ip = self.ip.wrapping_add(disp);

        self.biu_fetch_suspend(bus);
        self.clk(bus, 2);
        self.corr(bus);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.nearcall_routine(bus, new_ip, false);
        } else {
            self.nearcall_routine(bus, new_ip, true);
        }
    }

    fn jmp_near(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetchword(bus);
        self.set_ip_and_flush(bus, self.ip.wrapping_add(disp));
        let total_cycles = 15 + self.opcode_start_penalty_2();
        self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 2));
    }

    fn jmp_far(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        let segment = self.fetchword(bus);
        if self.far_immediate_jump_uses_preloaded_finish() {
            self.sregs[SegReg16::CS as usize] = segment;
            self.ip = offset;
            self.flush_and_fetch(bus);
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        } else {
            self.set_cs_ip_and_flush(bus, segment, offset);
        }
        let total_cycles = 15 + self.opcode_even_penalty_2();
        self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 4));
    }

    fn jmp_short(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        self.set_ip_and_flush(bus, self.ip.wrapping_add(disp));
        let total_cycles = 15 + self.opcode_start_penalty_2();
        self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 1));
    }

    fn ret_near(&mut self, bus: &mut impl common::Bus) {
        let ip = self.pop(bus);
        self.biu_fetch_suspend(bus);
        self.clk(bus, 1);
        self.ip = ip;
        self.flush_and_fetch(bus);
        self.clk(bus, 2);
    }

    fn ret_near_imm(&mut self, bus: &mut impl common::Bus) {
        let imm = self.fetchword(bus);
        self.farret_routine(bus, false);
        self.clk(bus, 1);
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        if self.instruction_entry_queue_full() {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }

    fn ret_far(&mut self, bus: &mut impl common::Bus) {
        self.farret_routine(bus, true);
    }

    fn ret_far_imm(&mut self, bus: &mut impl common::Bus) {
        let imm = self.fetchword(bus);
        self.farret_routine(bus, true);
        self.clk(bus, 1);
        let sp = self.regs.word(WordReg::SP).wrapping_add(imm);
        self.regs.set_word(WordReg::SP, sp);
        if self.instruction_entry_queue_full() {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }

    fn pushf(&mut self, bus: &mut impl common::Bus) {
        let flags_val = self.flags.compress();
        self.push(bus, flags_val);
        self.clk(bus, 3 + self.odd_queue_start_penalty());
    }

    fn popf(&mut self, bus: &mut impl common::Bus) {
        let val = self.pop(bus);
        self.flags.expand(val);
    }

    fn sahf(&mut self, bus: &mut impl common::Bus) {
        let ah = self.regs.byte(ByteReg::AH);
        self.flags.carry_val = (ah & 0x01) as u32;
        self.flags.parity_val = if ah & 0x04 != 0 { 0 } else { 1 };
        self.flags.aux_val = (ah & 0x10) as u32;
        self.flags.zero_val = if ah & 0x40 != 0 { 0 } else { 1 };
        self.flags.sign_val = if ah & 0x80 != 0 { -1 } else { 0 };
        self.clk(bus, self.queue_resident_eu_cycles(4, 0));
    }

    fn lahf(&mut self, bus: &mut impl common::Bus) {
        let flags_val = self.flags.compress() as u8;
        self.regs.set_byte(ByteReg::AH, flags_val);
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn mov_al_moffs(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        self.set_effective_address(SegReg16::DS, offset);
        let val = self.read_memory_byte(bus, self.ea);
        self.regs.set_byte(ByteReg::AL, val);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.clk(bus, 1);
        }
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
    }

    fn mov_aw_moffs(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        self.set_effective_address(SegReg16::DS, offset);
        let val = self.seg_read_word(bus);
        self.regs.set_word(WordReg::AX, val);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.clk(bus, 1);
        }
        self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
    }

    fn mov_moffs_al(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        self.set_effective_address(SegReg16::DS, offset);
        self.clk(bus, 1);
        self.write_memory_byte(bus, self.ea, self.regs.byte(ByteReg::AL));
        if self.instruction_entry_queue_bytes >= 5 {
            self.clk(bus, 1);
            if !self.seg_prefix && self.instruction_entry_queue_full() {
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            }
        }
    }

    fn mov_moffs_aw(&mut self, bus: &mut impl common::Bus) {
        let offset = self.fetchword(bus);
        self.set_effective_address(SegReg16::DS, offset);
        self.clk(bus, 1);
        self.seg_write_word(bus, self.regs.word(WordReg::AX));
        if self.instruction_entry_queue_bytes >= 5 {
            self.clk(bus, 1);
            if !self.seg_prefix && self.instruction_entry_queue_full() {
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            }
        }
    }

    fn mov_byte_reg_imm(&mut self, reg: ByteReg, bus: &mut impl common::Bus) {
        let val = self.fetch(bus);
        self.regs.set_byte(reg, val);
        self.clk(bus, self.queue_resident_eu_cycles(4, 1));
    }

    fn mov_word_reg_imm(&mut self, reg: WordReg, bus: &mut impl common::Bus) {
        let val = self.fetchword(bus);
        self.regs.set_word(reg, val);
        self.clk(bus, self.queue_resident_eu_cycles(4, 2));
    }

    fn mov_rm_imm8(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        if modrm >= 0xC0 {
            let val = self.fetch(bus);
            let reg = self.rm_byte(modrm);
            self.regs.set_byte(reg, val);
        } else {
            self.resolve_rm_address(modrm);
            self.charge_rm_eadone(modrm, bus);
            let val = self.fetch(bus);
            self.clk(bus, 1);
            self.putback_rm_byte(modrm, val, bus);
            if !self.seg_prefix
                && self.instruction_entry_queue_full()
                && !self.opcode_started_at_odd_address()
                && self.has_simple_disp16_base(modrm)
            {
                self.finish_on_terminal_writeback_inline_commit(modrm);
            } else if (self.fetch_state == FetchState::Normal
                && !self.has_disp16_double_register_base(modrm))
                || (self.fetch_state == FetchState::PausedFull
                    && self.seg_prefix
                    && modrm & 0xC7 == 0x06)
            {
                self.finish_on_terminal_writeback_with_fetch(modrm);
            }
        }
    }

    fn mov_rm_imm16(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        if modrm >= 0xC0 {
            let val = self.fetchword(bus);
            let reg = self.rm_word(modrm);
            self.regs.set_word(reg, val);
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        } else {
            self.resolve_rm_address(modrm);
            self.charge_rm_eadone(modrm, bus);
            let val = self.fetchword(bus);
            self.putback_rm_word(modrm, val, bus);
            if !self.seg_prefix
                && self.instruction_entry_queue_full()
                && !self.opcode_started_at_odd_address()
                && self.has_simple_disp16_base(modrm)
            {
                self.finish_on_terminal_writeback_inline_commit(modrm);
            } else if self.has_disp16_double_register_base(modrm) {
                if self.ip & 1 == 1
                    || (self.seg_prefix && self.has_disp16_cycle4_double_register_base(modrm))
                {
                    self.finish_on_terminal_writeback(modrm);
                } else {
                    self.finish_on_terminal_writeback_with_fetch(modrm);
                }
            } else if self.has_single_or_direct_base(modrm) {
                if !(self.seg_prefix && self.ip & 1 == 0) && self.queue_len() >= 4 {
                    self.finish_on_terminal_writeback(modrm);
                } else {
                    self.finish_on_terminal_writeback_with_fetch(modrm);
                }
            } else if self.queue_len() >= 4 {
                self.finish_on_terminal_writeback(modrm);
            } else {
                self.finish_on_terminal_writeback_with_fetch(modrm);
            }
        }
    }

    fn les(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        self.calc_ea(modrm);
        let offset = self.seg_read_word(bus);
        self.clk_eaload(bus);
        let segment = self.seg_read_word_at(bus, 2);
        self.clk(bus, 1);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.sregs[SegReg16::ES as usize] = segment;
    }

    fn lds(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        self.calc_ea(modrm);
        let offset = self.seg_read_word(bus);
        self.clk_eaload(bus);
        let segment = self.seg_read_word_at(bus, 2);
        self.clk(bus, 1);
        let reg = self.reg_word(modrm);
        self.regs.set_word(reg, offset);
        self.sregs[SegReg16::DS as usize] = segment;
    }

    fn int3(&mut self, bus: &mut impl common::Bus) {
        self.raise_software_interrupt(3, bus);
    }

    fn int_imm(&mut self, bus: &mut impl common::Bus) {
        let vector = self.fetch_interrupt_vector(bus);
        self.raise_software_interrupt_with_entry_cycles(vector, bus, 2);
    }

    fn into(&mut self, bus: &mut impl common::Bus) {
        if self.flags.of() {
            let preloaded_finish = !self.instruction_entry_queue_full();
            self.clk(bus, 1);
            self.raise_software_interrupt(4, bus);
            if preloaded_finish {
                self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
            }
        } else {
            self.clk(bus, 2);
        }
    }

    fn iret(&mut self, bus: &mut impl common::Bus) {
        self.farret_routine(bus, true);
        let flags_val = self.pop(bus);
        self.flags.expand(flags_val);
        self.clk(bus, 1);
    }

    fn loopne(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let cw = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, cw);
        if cw != 0 && !self.flags.zf() {
            self.set_ip_and_flush(bus, self.ip.wrapping_add(disp));
            let total_cycles = if self.seg_prefix || self.opcode_started_at_odd_address() {
                18
            } else {
                21
            };
            self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 1));
        } else {
            self.clk(bus, self.queue_resident_eu_cycles(6, 1));
        }
    }

    fn loope(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let cw = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, cw);
        if cw != 0 && self.flags.zf() {
            self.set_ip_and_flush(bus, self.ip.wrapping_add(disp));
            let total_cycles = if self.seg_prefix || self.opcode_started_at_odd_address() {
                18
            } else {
                21
            };
            self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 1));
        } else {
            self.clk(bus, self.queue_resident_eu_cycles(6, 1));
        }
    }

    fn loop_(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        let cw = self.regs.word(WordReg::CX).wrapping_sub(1);
        self.regs.set_word(WordReg::CX, cw);
        if cw != 0 {
            self.set_ip_and_flush(bus, self.ip.wrapping_add(disp));
            self.clk(bus, self.queue_resident_eu_cycles(17, 1));
        } else {
            self.clk(bus, self.queue_resident_eu_cycles(5, 1));
        }
    }

    fn jcxz(&mut self, bus: &mut impl common::Bus) {
        let disp = self.fetch(bus) as i8 as u16;
        if self.regs.word(WordReg::CX) == 0 {
            self.set_ip_and_flush(bus, self.ip.wrapping_add(disp));
            let total_cycles = if self.seg_prefix || self.opcode_started_at_odd_address() {
                18
            } else {
                21
            };
            self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 1));
        } else {
            self.clk(bus, self.queue_resident_eu_cycles(6, 1));
        }
    }

    fn in_al_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.read_io_byte(bus, port);
        self.clk(bus, 1);
        self.regs.set_byte(ByteReg::AL, val);
    }

    fn in_aw_imm(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.read_io_word(bus, port);
        self.clk(bus, 1);
        self.regs.set_word(WordReg::AX, val);
    }

    fn out_imm_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.regs.byte(ByteReg::AL);
        self.clk(bus, 2);
        self.write_io_byte(bus, port, val);
        self.clk(bus, self.odd_opcode_start_penalty());
    }

    fn out_imm_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.fetch(bus) as u16;
        let val = self.regs.word(WordReg::AX);
        self.clk(bus, 2);
        self.write_io_word(bus, port, val);
        self.clk(bus, self.odd_opcode_start_penalty());
    }

    fn in_al_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.read_io_byte(bus, port);
        self.regs.set_byte(ByteReg::AL, val);
    }

    fn in_aw_dw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.read_io_word(bus, port);
        self.regs.set_word(WordReg::AX, val);
    }

    fn out_dw_al(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.regs.byte(ByteReg::AL);
        self.clk(bus, 1);
        self.write_io_byte(bus, port, val);
        self.clk(bus, self.odd_opcode_start_penalty());
    }

    fn out_dw_aw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let val = self.regs.word(WordReg::AX);
        self.clk(bus, 1);
        self.write_io_word(bus, port, val);
        self.clk(bus, self.odd_opcode_start_penalty());
    }

    fn xlat(&mut self, bus: &mut impl common::Bus) {
        let al = self.regs.byte(ByteReg::AL) as u16;
        let bw = self.regs.word(WordReg::BX);
        let addr = self
            .default_base(SegReg16::DS)
            .wrapping_add(bw.wrapping_add(al) as u32)
            & ADDRESS_MASK;
        self.clk(bus, 3);
        let val = self.read_memory_byte(bus, addr);
        self.regs.set_byte(ByteReg::AL, val);
        self.clk(bus, self.odd_opcode_start_penalty());
    }

    fn salc(&mut self, bus: &mut impl common::Bus) {
        let carry = self.flags.cf();
        let value = if carry { 0xFF } else { 0x00 };
        self.regs.set_byte(ByteReg::AL, value);
        let total_cycles = if carry { 4 } else { 3 };
        self.clk(bus, self.queue_resident_eu_cycles(total_cycles, 0));
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
        let adjust = (self.regs.byte(ByteReg::AL) & 0x0F) > 9 || self.flags.af();
        if adjust {
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
        self.clk(bus, if adjust { 6 } else { 7 });
    }

    fn aas(&mut self, bus: &mut impl common::Bus) {
        let adjust = (self.regs.byte(ByteReg::AL) & 0x0F) > 9 || self.flags.af();
        if adjust {
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
        self.clk(bus, if adjust { 6 } else { 7 });
    }

    fn aam(&mut self, bus: &mut impl common::Bus) {
        let base = self.fetch(bus);
        if base == 0 {
            self.flags.set_szpf_byte(0);
            self.clk(bus, 8);
            self.raise_divide_error(bus);
            return;
        }
        let al = self.regs.byte(ByteReg::AL);
        let quotient = al / base;
        let remainder = al % base;
        self.regs.set_byte(ByteReg::AH, quotient);
        self.regs.set_byte(ByteReg::AL, remainder);
        let val = self.regs.byte(ByteReg::AL) as u32;
        self.flags.set_szpf_byte(val);
        let cycles = 77 + quotient.count_ones() as i32 + 2 * (quotient & 1) as i32;
        self.clk(bus, self.queue_resident_eu_cycles(cycles, 1));
    }

    fn aad(&mut self, bus: &mut impl common::Bus) {
        let base = self.fetch(bus);
        let al = self.regs.byte(ByteReg::AL);
        let ah = self.regs.byte(ByteReg::AH);
        let result = al.wrapping_add(ah.wrapping_mul(base));
        self.regs.set_byte(ByteReg::AL, result);
        self.regs.set_byte(ByteReg::AH, 0);
        self.flags.set_szpf_byte(result as u32);
        let cycles = 59 + base.count_ones() as i32;
        self.clk(bus, self.queue_resident_eu_cycles(cycles, 1));
    }

    fn fpu_escape(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch_modrm(bus);
        if modrm < 0xC0 {
            self.calc_ea(modrm);
            let _ = self.seg_read_word(bus);
            self.clk_eaload(bus);
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        } else {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }

    fn clc(&mut self, bus: &mut impl common::Bus) {
        self.flags.carry_val = 0;
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn stc(&mut self, bus: &mut impl common::Bus) {
        self.flags.carry_val = 1;
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn cli(&mut self, bus: &mut impl common::Bus) {
        self.flags.if_flag = false;
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn sti(&mut self, bus: &mut impl common::Bus) {
        self.flags.if_flag = true;
        self.no_interrupt = 1;
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn cld(&mut self, bus: &mut impl common::Bus) {
        self.flags.df = false;
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn std(&mut self, bus: &mut impl common::Bus) {
        self.flags.df = true;
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn cmc(&mut self, bus: &mut impl common::Bus) {
        self.flags.carry_val = if self.flags.cf() { 0 } else { 1 };
        self.clk(bus, self.queue_resident_eu_cycles(2, 0));
    }

    fn hlt(&mut self, bus: &mut impl common::Bus) {
        self.halted = true;
        self.clk(bus, 2);
    }

    fn segment_prefix(&mut self, bus: &mut impl common::Bus, segment: SegReg16) {
        self.seg_prefix = true;
        self.prefix_seg = segment;
        self.clk(bus, 1);
        let opcode = self.fetch(bus);
        self.dispatch(opcode, bus);
    }

    fn lock_prefix(&mut self, bus: &mut impl common::Bus) {
        self.inhibit_all = 1;
        self.clk(bus, 1);
        let opcode = self.fetch(bus);
        self.dispatch(opcode, bus);
    }
}
