use super::I286;
use crate::{ByteReg, SegReg16, WordReg};

impl I286 {
    fn direction_delta(&self) -> u16 {
        if self.flags.df { 0xFFFF } else { 1 }
    }

    fn direction_delta_word(&self) -> u16 {
        if self.flags.df { 0xFFFE } else { 2 }
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let val = self.read_byte_seg(bus, src_seg, si);
        self.write_byte_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        self.clk(5);
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let val = self.read_word_seg(bus, src_seg, si);
        self.write_word_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        let penalty = 2 * i32::from(si & 1 == 1) + i32::from(di & 1 == 1);
        self.clk(5 + penalty);
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let src = self.read_byte_seg(bus, src_seg, si);
        let dst = self.read_byte_seg(bus, SegReg16::ES, di);
        self.alu_sub_byte(src, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        self.clk(8);
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let src = self.read_word_seg(bus, src_seg, si);
        let dst = self.read_word_seg(bus, SegReg16::ES, di);
        self.alu_sub_word(src, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        let penalty = 2 * i32::from(si & 1 == 1) + 2 * i32::from(di & 1 == 1);
        self.clk(8 + penalty);
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        self.write_byte_seg(bus, SegReg16::ES, di, self.regs.byte(ByteReg::AL));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        self.clk(3);
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        self.write_word_seg(bus, SegReg16::ES, di, self.regs.word(WordReg::AX));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        let penalty = if di & 1 == 1 { 1 } else { 0 };
        self.clk(3 + penalty);
    }

    pub(super) fn lodsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_byte_seg(bus, seg, si);
        self.regs.set_byte(ByteReg::AL, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        self.clk(5);
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_word_seg(bus, seg, si);
        self.regs.set_word(WordReg::AX, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let penalty = if si & 1 == 1 { 2 } else { 0 };
        self.clk(5 + penalty);
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let dst = self.read_byte_seg(bus, SegReg16::ES, di);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        self.clk(7);
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let dst = self.read_word_seg(bus, SegReg16::ES, di);
        let aw = self.regs.word(WordReg::AX);
        self.alu_sub_word(aw, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        let penalty = if di & 1 == 1 { 2 } else { 0 };
        self.clk(7 + penalty);
    }

    pub(super) fn insb(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let val = self.io_read_byte_timed(bus, port);
        self.write_byte_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        self.clk(5);
    }

    pub(super) fn insw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let val = self.io_read_word_timed(bus, port);
        self.write_word_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
        let penalty = 2 * i32::from(port & 1 == 1) + i32::from(di & 1 == 1);
        self.clk(5 + penalty);
    }

    pub(super) fn outsb(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_byte_seg(bus, seg, si);
        self.io_write_byte_timed(bus, port, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        self.clk(5);
    }

    pub(super) fn outsw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_word_seg(bus, seg, si);
        self.io_write_word_timed(bus, port, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let penalty = 2 * i32::from(port & 1 == 1) + 2 * i32::from(si & 1 == 1);
        self.clk(5 + penalty);
    }
}
