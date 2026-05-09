use super::I386;
use crate::{ByteReg, DwordReg, SegReg32, WordReg};

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
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
    fn string_read_word(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u32,
    ) -> Option<u16> {
        let l0 = self.string_addr_delta(base, offset, 0);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFE
        } else {
            l0 & 0xFFF <= 0xFFE && (offset as u16) <= 0xFFFE
        };
        if same_page {
            let a0 = self.translate_linear(l0, false, bus)?;
            return Some(bus.read_word(a0));
        }
        let l1 = self.string_addr_delta(base, offset, 1);
        let a0 = self.translate_linear(l0, false, bus)?;
        let a1 = self.translate_linear(l1, false, bus)?;
        Some(bus.read_byte(a0) as u16 | ((bus.read_byte(a1) as u16) << 8))
    }

    #[inline(always)]
    fn string_write_word(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u32,
        value: u16,
    ) -> bool {
        let l0 = self.string_addr_delta(base, offset, 0);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFE
        } else {
            l0 & 0xFFF <= 0xFFE && (offset as u16) <= 0xFFFE
        };
        if same_page {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return false;
            };
            bus.write_word(a0, value);
            return true;
        }
        let l1 = self.string_addr_delta(base, offset, 1);
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return false;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return false;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        true
    }

    #[inline(always)]
    fn string_read_dword(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u32,
    ) -> Option<u32> {
        let l0 = self.string_addr_delta(base, offset, 0);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFC
        } else {
            l0 & 0xFFF <= 0xFFC && (offset as u16) <= 0xFFFC
        };
        if same_page {
            let a0 = self.translate_linear(l0, false, bus)?;
            return Some(bus.read_dword(a0));
        }
        let l1 = self.string_addr_delta(base, offset, 1);
        let l2 = self.string_addr_delta(base, offset, 2);
        let l3 = self.string_addr_delta(base, offset, 3);
        let a0 = self.translate_linear(l0, false, bus)?;
        let a1 = self.translate_linear(l1, false, bus)?;
        let a2 = self.translate_linear(l2, false, bus)?;
        let a3 = self.translate_linear(l3, false, bus)?;
        Some(
            bus.read_byte(a0) as u32
                | ((bus.read_byte(a1) as u32) << 8)
                | ((bus.read_byte(a2) as u32) << 16)
                | ((bus.read_byte(a3) as u32) << 24),
        )
    }

    #[inline(always)]
    fn string_write_dword(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u32,
        value: u32,
    ) -> bool {
        let l0 = self.string_addr_delta(base, offset, 0);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFC
        } else {
            l0 & 0xFFF <= 0xFFC && (offset as u16) <= 0xFFFC
        };
        if same_page {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return false;
            };
            bus.write_dword(a0, value);
            return true;
        }
        let l1 = self.string_addr_delta(base, offset, 1);
        let l2 = self.string_addr_delta(base, offset, 2);
        let l3 = self.string_addr_delta(base, offset, 3);
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return false;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return false;
        };
        let Some(a2) = self.translate_linear(l2, true, bus) else {
            return false;
        };
        let Some(a3) = self.translate_linear(l3, true, bus) else {
            return false;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        bus.write_byte(a2, (value >> 16) as u8);
        bus.write_byte(a3, (value >> 24) as u8);
        true
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_seg = self.default_seg(SegReg32::DS);
        if !self.check_segment_access(src_seg, si, 1, false, bus) {
            return;
        }
        if !self.check_segment_access(SegReg32::ES, di, 1, true, bus) {
            return;
        }
        let src_linear = self.string_addr(self.seg_base(src_seg), si);
        let dst_linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let Some(src_phys) = self.translate_linear(src_linear, false, bus) else {
            return;
        };
        let val = bus.read_byte(src_phys);
        let Some(dst_phys) = self.translate_linear(dst_linear, true, bus) else {
            return;
        };
        bus.write_byte(dst_phys, val);
        self.string_advance_si(1);
        self.string_advance_di(1);
        self.clk(Self::timing(7, 7));
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_seg = self.default_seg(SegReg32::DS);
        let access_size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_segment_access(src_seg, si, access_size, false, bus) {
            return;
        }
        if !self.check_segment_access(SegReg32::ES, di, access_size, true, bus) {
            return;
        }
        let src_base = self.seg_base(src_seg);
        let dst_base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let Some(val) = self.string_read_dword(bus, src_base, si) else {
                return;
            };
            if !self.string_write_dword(bus, dst_base, di, val) {
                return;
            }
            self.string_advance_si(4);
            self.string_advance_di(4);
        } else {
            let Some(val) = self.string_read_word(bus, src_base, si) else {
                return;
            };
            if !self.string_write_word(bus, dst_base, di, val) {
                return;
            }
            self.string_advance_si(2);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0 || di & 3 != 0
        } else {
            si & 1 != 0 || di & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(7, 7) + penalty);
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_seg = self.default_seg(SegReg32::DS);
        if !self.check_segment_access(src_seg, si, 1, false, bus) {
            return;
        }
        if !self.check_segment_access(SegReg32::ES, di, 1, false, bus) {
            return;
        }
        let src_linear = self.string_addr(self.seg_base(src_seg), si);
        let dst_linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let Some(src_phys) = self.translate_linear(src_linear, false, bus) else {
            return;
        };
        let Some(dst_phys) = self.translate_linear(dst_linear, false, bus) else {
            return;
        };
        let src = bus.read_byte(src_phys);
        let dst = bus.read_byte(dst_phys);
        self.alu_sub_byte(src, dst);
        self.string_advance_si(1);
        self.string_advance_di(1);
        self.clk(Self::timing(10, 8));
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let di = self.string_index_di();
        let src_seg = self.default_seg(SegReg32::DS);
        let access_size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_segment_access(src_seg, si, access_size, false, bus) {
            return;
        }
        if !self.check_segment_access(SegReg32::ES, di, access_size, false, bus) {
            return;
        }
        let src_base = self.seg_base(src_seg);
        let dst_base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let Some(src) = self.string_read_dword(bus, src_base, si) else {
                return;
            };
            let Some(dst) = self.string_read_dword(bus, dst_base, di) else {
                return;
            };
            self.alu_sub_dword(src, dst);
            self.string_advance_si(4);
            self.string_advance_di(4);
        } else {
            let Some(src) = self.string_read_word(bus, src_base, si) else {
                return;
            };
            let Some(dst) = self.string_read_word(bus, dst_base, di) else {
                return;
            };
            self.alu_sub_word(src, dst);
            self.string_advance_si(2);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0 || di & 3 != 0
        } else {
            si & 1 != 0 || di & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(10, 8) + penalty);
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        if !self.check_segment_access(SegReg32::ES, di, 1, true, bus) {
            return;
        }
        let linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let al = self.regs.byte(ByteReg::AL);
        let Some(addr) = self.translate_linear(linear, true, bus) else {
            return;
        };
        bus.write_byte(addr, al);
        self.string_advance_di(1);
        self.clk(Self::timing(4, 5));
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        let access_size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_segment_access(SegReg32::ES, di, access_size, true, bus) {
            return;
        }
        let base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            if !self.string_write_dword(bus, base, di, self.regs.dword(DwordReg::EAX)) {
                return;
            }
            self.string_advance_di(4);
        } else {
            if !self.string_write_word(bus, base, di, self.regs.word(WordReg::AX)) {
                return;
            }
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            di & 3 != 0
        } else {
            di & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(4, 5) + penalty);
    }

    pub(super) fn lodsb(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let src_seg = self.default_seg(SegReg32::DS);
        if !self.check_segment_access(src_seg, si, 1, false, bus) {
            return;
        }
        let linear = self.string_addr(self.seg_base(src_seg), si);
        let Some(addr) = self.translate_linear(linear, false, bus) else {
            return;
        };
        let val = bus.read_byte(addr);
        self.regs.set_byte(ByteReg::AL, val);
        self.string_advance_si(1);
        self.clk(Self::timing(5, 5));
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        let si = self.string_index_si();
        let src_seg = self.default_seg(SegReg32::DS);
        let access_size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_segment_access(src_seg, si, access_size, false, bus) {
            return;
        }
        let base = self.seg_base(src_seg);
        if self.operand_size_override {
            let Some(val) = self.string_read_dword(bus, base, si) else {
                return;
            };
            self.regs.set_dword(DwordReg::EAX, val);
            self.string_advance_si(4);
        } else {
            let Some(val) = self.string_read_word(bus, base, si) else {
                return;
            };
            self.regs.set_word(WordReg::AX, val);
            self.string_advance_si(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0
        } else {
            si & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(5, 5) + penalty);
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        if !self.check_segment_access(SegReg32::ES, di, 1, false, bus) {
            return;
        }
        let linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let Some(addr) = self.translate_linear(linear, false, bus) else {
            return;
        };
        let dst = bus.read_byte(addr);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        self.string_advance_di(1);
        self.clk(Self::timing(7, 6));
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        let di = self.string_index_di();
        let access_size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_segment_access(SegReg32::ES, di, access_size, false, bus) {
            return;
        }
        let base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let Some(dst) = self.string_read_dword(bus, base, di) else {
                return;
            };
            let eax = self.regs.dword(DwordReg::EAX);
            self.alu_sub_dword(eax, dst);
            self.string_advance_di(4);
        } else {
            let Some(dst) = self.string_read_word(bus, base, di) else {
                return;
            };
            let aw = self.regs.word(WordReg::AX);
            self.alu_sub_word(aw, dst);
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            di & 3 != 0
        } else {
            di & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(7, 6) + penalty);
    }

    pub(super) fn insb(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, 1, bus) {
            return;
        }
        let di = self.string_index_di();
        if !self.check_segment_access(SegReg32::ES, di, 1, true, bus) {
            return;
        }
        let linear = self.string_addr(self.seg_base(SegReg32::ES), di);
        let val = bus.io_read_byte(port);
        let Some(addr) = self.translate_linear(linear, true, bus) else {
            return;
        };
        bus.write_byte(addr, val);
        self.string_advance_di(1);
        self.clk(Self::timing(15, 17));
    }

    pub(super) fn insw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_io_privilege(port, size, bus) {
            return;
        }
        let di = self.string_index_di();
        if !self.check_segment_access(SegReg32::ES, di, size as u32, true, bus) {
            return;
        }
        let base = self.seg_base(SegReg32::ES);
        if self.operand_size_override {
            let low = bus.io_read_word(port) as u32;
            let high = bus.io_read_word(port.wrapping_add(2)) as u32;
            let val = low | (high << 16);
            if !self.string_write_dword(bus, base, di, val) {
                return;
            }
            self.string_advance_di(4);
        } else {
            let val = bus.io_read_word(port);
            if !self.string_write_word(bus, base, di, val) {
                return;
            }
            self.string_advance_di(2);
        }
        let misaligned = if self.operand_size_override {
            di & 3 != 0
        } else {
            di & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(15, 17) + penalty);
    }

    pub(super) fn outsb(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        if !self.check_io_privilege(port, 1, bus) {
            return;
        }
        let si = self.string_index_si();
        let src_seg = self.default_seg(SegReg32::DS);
        if !self.check_segment_access(src_seg, si, 1, false, bus) {
            return;
        }
        let linear = self.string_addr(self.seg_base(src_seg), si);
        let Some(addr) = self.translate_linear(linear, false, bus) else {
            return;
        };
        let val = bus.read_byte(addr);
        bus.io_write_byte(port, val);
        self.string_advance_si(1);
        self.clk(Self::timing(14, 17));
    }

    pub(super) fn outsw(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let size = if self.operand_size_override { 4 } else { 2 };
        if !self.check_io_privilege(port, size, bus) {
            return;
        }
        let si = self.string_index_si();
        let src_seg = self.default_seg(SegReg32::DS);
        if !self.check_segment_access(src_seg, si, size as u32, false, bus) {
            return;
        }
        let base = self.seg_base(src_seg);
        if self.operand_size_override {
            let Some(val) = self.string_read_dword(bus, base, si) else {
                return;
            };
            bus.io_write_word(port, val as u16);
            bus.io_write_word(port.wrapping_add(2), (val >> 16) as u16);
            self.string_advance_si(4);
        } else {
            let Some(val) = self.string_read_word(bus, base, si) else {
                return;
            };
            bus.io_write_word(port, val);
            self.string_advance_si(2);
        }
        let misaligned = if self.operand_size_override {
            si & 3 != 0
        } else {
            si & 1 != 0
        };
        let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
        self.clk(Self::timing(14, 17) + penalty);
    }
}
