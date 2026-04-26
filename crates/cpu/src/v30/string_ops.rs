use super::V30;
use crate::{ByteReg, SegReg16, WordReg};

impl V30 {
    fn direction_delta(&self) -> u16 {
        if self.flags.df { 0xFFFF } else { 1 }
    }

    fn direction_delta_word(&self) -> u16 {
        if self.flags.df { 0xFFFE } else { 2 }
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, src_addr);
        self.biu_chain_eu_transfer();
        self.biu_write_u8_physical(bus, dst_addr, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_base = self.default_base(SegReg16::DS);
        let dst_base = self.seg_base(SegReg16::ES);
        let val = self.read_word_seg(bus, src_base, si);
        self.biu_chain_eu_transfer();
        self.write_word_seg(bus, dst_base, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let src = self.biu_read_u8_physical(bus, src_addr);
        self.biu_chain_eu_transfer();
        let dst = self.biu_read_u8_physical(bus, dst_addr);
        self.alu_sub_byte(src, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_base = self.default_base(SegReg16::DS);
        let dst_base = self.seg_base(SegReg16::ES);
        let src = self.read_word_seg(bus, src_base, si);
        self.biu_chain_eu_transfer();
        let dst = self.read_word_seg(bus, dst_base, di);
        self.alu_sub_word(src, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        self.biu_write_u8_physical(bus, addr, self.regs.byte(ByteReg::AL));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        self.stosb_body(bus);
        self.clk(bus, 3);
    }

    pub(super) fn stosw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        self.write_word_seg(bus, base, di, self.regs.word(WordReg::AX));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        self.stosw_body(bus);
        self.clk(bus, 3);
    }

    pub(super) fn lodsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, addr);
        self.regs.set_byte(ByteReg::AL, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsb(&mut self, bus: &mut impl common::Bus) {
        self.lodsb_body(bus);
        self.clk(bus, 3);
    }

    pub(super) fn lodsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let val = self.read_word_seg(bus, base, si);
        self.regs.set_word(WordReg::AX, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        self.lodsw_body(bus);
        self.clk(bus, 2);
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let dst = self.biu_read_u8_physical(bus, addr);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let dst = self.read_word_seg(bus, base, di);
        let aw = self.regs.word(WordReg::AX);
        self.alu_sub_word(aw, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insb_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let val = self.biu_io_read_u8(bus, port);
        self.biu_chain_eu_transfer();
        self.biu_write_u8_physical(bus, addr, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insb(&mut self, bus: &mut impl common::Bus) {
        self.insb_body(bus);
        self.clk(bus, 4);
    }

    pub(super) fn insw_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let val = self.biu_io_read_u16(bus, port);
        self.biu_chain_eu_transfer();
        self.write_word_seg(bus, base, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insw(&mut self, bus: &mut impl common::Bus) {
        self.insw_body(bus);
        self.clk(bus, 4);
    }

    pub(super) fn outsb_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, addr);
        self.biu_chain_eu_transfer();
        self.biu_io_write_u8(bus, port, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn outsb(&mut self, bus: &mut impl common::Bus) {
        self.outsb_body(bus);
        self.clk(bus, 4);
    }

    pub(super) fn outsw_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let val = self.read_word_seg(bus, base, si);
        self.biu_chain_eu_transfer();
        self.biu_io_write_u16(bus, port, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn outsw(&mut self, bus: &mut impl common::Bus) {
        self.outsw_body(bus);
        self.clk(bus, 4);
    }
}
