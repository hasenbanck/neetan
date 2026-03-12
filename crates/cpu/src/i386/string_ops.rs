use super::I386;
use crate::{ByteReg, DwordReg, SegReg32, WordReg};

impl I386 {
    #[inline(always)]
    fn string_index_si(&self) -> u32 {
        if self.address_size_override {
            self.regs.dword(DwordReg::ESI)
        } else {
            self.regs.word(WordReg::SI) as u32
        }
    }

    #[inline(always)]
    fn string_index_di(&self) -> u32 {
        if self.address_size_override {
            self.regs.dword(DwordReg::EDI)
        } else {
            self.regs.word(WordReg::DI) as u32
        }
    }

    #[inline(always)]
    fn string_set_si(&mut self, value: u32) {
        if self.address_size_override {
            self.regs.set_dword(DwordReg::ESI, value);
        } else {
            self.regs.set_word(WordReg::SI, value as u16);
        }
    }

    #[inline(always)]
    fn string_set_di(&mut self, value: u32) {
        if self.address_size_override {
            self.regs.set_dword(DwordReg::EDI, value);
        } else {
            self.regs.set_word(WordReg::DI, value as u16);
        }
    }

    #[inline(always)]
    fn string_advance_si(&mut self, bytes: u32) {
        let si = self.string_index_si();
        let next = if self.flags.df {
            si.wrapping_sub(bytes)
        } else {
            si.wrapping_add(bytes)
        };
        self.string_set_si(next);
    }

    #[inline(always)]
    fn string_advance_di(&mut self, bytes: u32) {
        let di = self.string_index_di();
        let next = if self.flags.df {
            di.wrapping_sub(bytes)
        } else {
            di.wrapping_add(bytes)
        };
        self.string_set_di(next);
    }

    #[inline(always)]
    fn string_addr(&self, base: u32, offset: u32) -> u32 {
        self.string_addr_delta(base, offset, 0)
    }

    #[inline(always)]
    fn string_addr_delta(&self, base: u32, offset: u32, delta: u8) -> u32 {
        let effective_offset = if self.address_size_override {
            offset.wrapping_add(delta as u32)
        } else {
            (offset as u16).wrapping_add(delta as u16) as u32
        };
        base.wrapping_add(effective_offset)
    }

    #[inline(always)]
    fn string_read_word(&mut self, bus: &mut impl common::Bus, base: u32, offset: u32) -> u16 {
        let l0 = self.string_addr_delta(base, offset, 0);
        let l1 = self.string_addr_delta(base, offset, 1);
        let a0 = self.translate_linear(l0, false, bus).unwrap_or(0);
        let a1 = self.translate_linear(l1, false, bus).unwrap_or(0);
        bus.read_byte(a0) as u16 | ((bus.read_byte(a1) as u16) << 8)
    }

    #[inline(always)]
    fn string_write_word(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u32,
        value: u16,
    ) {
        let l0 = self.string_addr_delta(base, offset, 0);
        let l1 = self.string_addr_delta(base, offset, 1);
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
    }

    #[inline(always)]
    fn string_read_dword(&mut self, bus: &mut impl common::Bus, base: u32, offset: u32) -> u32 {
        let l0 = self.string_addr_delta(base, offset, 0);
        let l1 = self.string_addr_delta(base, offset, 1);
        let l2 = self.string_addr_delta(base, offset, 2);
        let l3 = self.string_addr_delta(base, offset, 3);
        let a0 = self.translate_linear(l0, false, bus).unwrap_or(0);
        let a1 = self.translate_linear(l1, false, bus).unwrap_or(0);
        let a2 = self.translate_linear(l2, false, bus).unwrap_or(0);
        let a3 = self.translate_linear(l3, false, bus).unwrap_or(0);
        bus.read_byte(a0) as u32
            | ((bus.read_byte(a1) as u32) << 8)
            | ((bus.read_byte(a2) as u32) << 16)
            | ((bus.read_byte(a3) as u32) << 24)
    }

    #[inline(always)]
    fn string_write_dword(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u32,
        value: u32,
    ) {
        let l0 = self.string_addr_delta(base, offset, 0);
        let l1 = self.string_addr_delta(base, offset, 1);
        let l2 = self.string_addr_delta(base, offset, 2);
        let l3 = self.string_addr_delta(base, offset, 3);
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        let Some(a2) = self.translate_linear(l2, true, bus) else {
            return;
        };
        let Some(a3) = self.translate_linear(l3, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        bus.write_byte(a2, (value >> 16) as u8);
        bus.write_byte(a3, (value >> 24) as u8);
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_linear = self.string_addr(self.default_base(SegReg32::DS), si);
        let dst_linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let src_phys = self.translate_linear(src_linear, false, bus).unwrap_or(0);
        let val = bus.read_byte(src_phys);
        let Some(dst_phys) = self.translate_linear(dst_linear, true, bus) else {
            return;
        };
        bus.write_byte(dst_phys, val);
        self.string_advance_si(1);
        self.string_advance_di(1);
        self.clk(7);
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_base = self.default_base(SegReg32::DS);
        let dst_base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let val = self.string_read_dword(bus, src_base, si);
            self.string_write_dword(bus, dst_base, di, val);
            self.string_advance_si(4);
            self.string_advance_di(4);
        } else {
            let val = self.string_read_word(bus, src_base, si);
            self.string_write_word(bus, dst_base, di, val);
            self.string_advance_si(2);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0 || di & 3 != 0
        } else {
            si & 1 != 0 || di & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(7 + penalty);
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_linear = self.string_addr(self.default_base(SegReg32::DS), si);
        let dst_linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let src_phys = self.translate_linear(src_linear, false, bus).unwrap_or(0);
        let dst_phys = self.translate_linear(dst_linear, false, bus).unwrap_or(0);
        let src = bus.read_byte(src_phys);
        let dst = bus.read_byte(dst_phys);
        self.alu_sub_byte(src, dst);
        self.string_advance_si(1);
        self.string_advance_di(1);
        self.clk(10);
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_base = self.default_base(SegReg32::DS);
        let dst_base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let src = self.string_read_dword(bus, src_base, si);
            let dst = self.string_read_dword(bus, dst_base, di);
            self.alu_sub_dword(src, dst);
            self.string_advance_si(4);
            self.string_advance_di(4);
        } else {
            let src = self.string_read_word(bus, src_base, si);
            let dst = self.string_read_word(bus, dst_base, di);
            self.alu_sub_word(src, dst);
            self.string_advance_si(2);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0 || di & 3 != 0
        } else {
            si & 1 != 0 || di & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(10 + penalty);
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        let linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let al = self.regs.byte(ByteReg::AL);
        let Some(addr) = self.translate_linear(linear, true, bus) else {
            return;
        };
        bus.write_byte(addr, al);
        self.string_advance_di(1);
        self.clk(4);
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        let base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            self.string_write_dword(bus, base, di, self.regs.dword(DwordReg::EAX));
            self.string_advance_di(4);
        } else {
            self.string_write_word(bus, base, di, self.regs.word(WordReg::AX));
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            di & 3 != 0
        } else {
            di & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(4 + penalty);
    }

    pub(super) fn lodsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let linear = self.string_addr(self.default_base(SegReg32::DS), si);
        let addr = self.translate_linear(linear, false, bus).unwrap_or(0);
        let val = bus.read_byte(addr);
        self.regs.set_byte(ByteReg::AL, val);
        self.string_advance_si(1);
        self.clk(5);
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let base = self.default_base(SegReg32::DS);
        if self.operand_size_override {
            let val = self.string_read_dword(bus, base, si);
            self.regs.set_dword(DwordReg::EAX, val);
            self.string_advance_si(4);
        } else {
            let val = self.string_read_word(bus, base, si);
            self.regs.set_word(WordReg::AX, val);
            self.string_advance_si(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0
        } else {
            si & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(5 + penalty);
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        let linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let addr = self.translate_linear(linear, false, bus).unwrap_or(0);
        let dst = bus.read_byte(addr);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        self.string_advance_di(1);
        self.clk(7);
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        let base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let dst = self.string_read_dword(bus, base, di);
            let eax = self.regs.dword(DwordReg::EAX);
            self.alu_sub_dword(eax, dst);
            self.string_advance_di(4);
        } else {
            let dst = self.string_read_word(bus, base, di);
            let aw = self.regs.word(WordReg::AX);
            self.alu_sub_word(aw, dst);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            di & 3 != 0
        } else {
            di & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(7 + penalty);
    }

    pub(super) fn insb(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, bus) {
            return;
        }
        let di = self.string_index_di();
        let linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let val = bus.io_read_byte(port);
        let Some(addr) = self.translate_linear(linear, true, bus) else {
            return;
        };
        bus.write_byte(addr, val);
        self.string_advance_di(1);
        self.clk(15);
    }

    pub(super) fn insw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, bus) {
            return;
        }
        let di = self.string_index_di();
        let base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            if !self.check_io_privilege(port.wrapping_add(2), bus) {
                return;
            }
            let low = bus.io_read_word(port) as u32;
            let high = bus.io_read_word(port.wrapping_add(2)) as u32;
            let val = low | (high << 16);
            self.string_write_dword(bus, base, di, val);
            self.string_advance_di(4);
        } else {
            let val = bus.io_read_word(port);
            self.string_write_word(bus, base, di, val);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            di & 3 != 0
        } else {
            di & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(15 + penalty);
    }

    pub(super) fn outsb(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, bus) {
            return;
        }
        let si = self.string_index_si();
        let linear = self.string_addr(self.default_base(SegReg32::DS), si);
        let addr = self.translate_linear(linear, false, bus).unwrap_or(0);
        let val = bus.read_byte(addr);
        bus.io_write_byte(port, val);
        self.string_advance_si(1);
        self.clk(14);
    }

    pub(super) fn outsw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, bus) {
            return;
        }
        let si = self.string_index_si();
        let base = self.default_base(SegReg32::DS);
        if self.operand_size_override {
            if !self.check_io_privilege(port.wrapping_add(2), bus) {
                return;
            }
            let val = self.string_read_dword(bus, base, si);
            bus.io_write_word(port, val as u16);
            bus.io_write_word(port.wrapping_add(2), (val >> 16) as u16);
            self.string_advance_si(4);
        } else {
            let val = self.string_read_word(bus, base, si);
            bus.io_write_word(port, val);
            self.string_advance_si(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0
        } else {
            si & 1 != 0
        };
        let penalty = if misaligned { 4 } else { 0 };
        self.clk(14 + penalty);
    }
}
