use super::{ADDRESS_MASK, I286};

impl I286 {
    pub(super) fn extended_0f(&mut self, bus: &mut impl common::Bus) {
        let sub = self.fetch(bus);
        match sub {
            0x00 => self.group_0f00(bus),
            0x01 => self.group_0f01(bus),
            0x02 => self.lar(bus),
            0x03 => self.lsl_instr(bus),
            _ => self.raise_fault(6, bus),
        }
    }

    fn group_0f00(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.msw & 1 == 0 {
            self.raise_fault(6, bus);
            return;
        }
        match (modrm >> 3) & 7 {
            0 => {
                // SLDT - Store LDTR selector
                self.put_rm_word(modrm, self.ldtr, bus);
                self.clk_modrm_prefetch(bus, modrm, 2, 3);
            }
            1 => {
                // STR - Store Task Register selector
                self.put_rm_word(modrm, self.tr, bus);
                self.clk_modrm_prefetch(bus, modrm, 2, 3);
            }
            2 => {
                // LLDT - Load LDTR from GDT descriptor (Bug #4: CPL=0 required)
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
                self.clk_modrm_prefetch(bus, modrm, 17, 19);
            }
            3 => {
                // LTR - Load Task Register from GDT descriptor
                // Bug #4: CPL=0 required
                if self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                let selector = self.get_rm_word(modrm, bus);
                // Bug #13: use 0xFFFC mask
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
                // Bug #6: only accept available TSS (type 1), not busy (type 3)
                if descriptor.rights & 0x10 != 0 || desc_type != 0x01 {
                    self.raise_fault_with_code(13, selector & 0xFFFC, bus);
                    return;
                }
                if descriptor.rights & 0x80 == 0 {
                    self.raise_fault_with_code(11, selector & 0xFFFC, bus);
                    return;
                }
                self.tr = selector;
                self.tr_base = descriptor.base;
                self.tr_limit = descriptor.limit;
                self.tr_rights = descriptor.rights;
                // Bug #5: Mark TSS as busy by setting bit 1 of type field.
                self.tr_rights |= 0x02;
                if let Some(addr) = self.descriptor_addr_checked(selector) {
                    let r = bus.read_byte(addr.wrapping_add(5) & ADDRESS_MASK);
                    bus.write_byte(addr.wrapping_add(5) & ADDRESS_MASK, r | 0x02);
                }
                self.clk_modrm_prefetch(bus, modrm, 17, 19);
            }
            4 => {
                // VERR - Verify segment readable (Bug #11: conforming exemption)
                let selector = self.get_rm_word(modrm, bus);
                let readable = self.verr_accessible(selector, bus);
                self.flags.zero_val = if readable { 0 } else { 1 };
                self.clk_modrm_prefetch(bus, modrm, 14, 16);
            }
            5 => {
                // VERW - Verify segment writable
                let selector = self.get_rm_word(modrm, bus);
                let writable = self.selector_accessible(selector, true, bus);
                self.flags.zero_val = if writable { 0 } else { 1 };
                self.clk_modrm_prefetch(bus, modrm, 14, 16);
            }
            _ => self.raise_fault(6, bus),
        }
    }

    fn group_0f01(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // SGDT - Store Global Descriptor Table Register
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                bus.write_byte(self.ea, self.gdt_limit as u8);
                bus.write_byte(self.seg_addr(1), (self.gdt_limit >> 8) as u8);
                bus.write_byte(self.seg_addr(2), self.gdt_base as u8);
                bus.write_byte(self.seg_addr(3), (self.gdt_base >> 8) as u8);
                bus.write_byte(self.seg_addr(4), (self.gdt_base >> 16) as u8);
                bus.write_byte(self.seg_addr(5), 0xFF);
                self.clk(11);
            }
            1 => {
                // SIDT - Store Interrupt Descriptor Table Register
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                bus.write_byte(self.ea, self.idt_limit as u8);
                bus.write_byte(self.seg_addr(1), (self.idt_limit >> 8) as u8);
                bus.write_byte(self.seg_addr(2), self.idt_base as u8);
                bus.write_byte(self.seg_addr(3), (self.idt_base >> 8) as u8);
                bus.write_byte(self.seg_addr(4), (self.idt_base >> 16) as u8);
                bus.write_byte(self.seg_addr(5), 0xFF);
                self.clk(12);
            }
            2 => {
                // LGDT - Load Global Descriptor Table Register (Bug #4: CPL=0 in PM)
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                if self.is_protected_mode() && self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                let limit =
                    bus.read_byte(self.ea) as u16 | ((bus.read_byte(self.seg_addr(1)) as u16) << 8);
                let base = bus.read_byte(self.seg_addr(2)) as u32
                    | ((bus.read_byte(self.seg_addr(3)) as u32) << 8)
                    | ((bus.read_byte(self.seg_addr(4)) as u32) << 16);
                self.gdt_base = base & ADDRESS_MASK;
                self.gdt_limit = limit;
                self.clk(11);
            }
            3 => {
                // LIDT - Load Interrupt Descriptor Table Register (Bug #4: CPL=0 in PM)
                if modrm >= 0xC0 {
                    self.raise_fault(6, bus);
                    return;
                }
                if self.is_protected_mode() && self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                self.calc_ea(modrm, bus);
                let limit =
                    bus.read_byte(self.ea) as u16 | ((bus.read_byte(self.seg_addr(1)) as u16) << 8);
                let base = bus.read_byte(self.seg_addr(2)) as u32
                    | ((bus.read_byte(self.seg_addr(3)) as u32) << 8)
                    | ((bus.read_byte(self.seg_addr(4)) as u32) << 16);
                self.idt_base = base & ADDRESS_MASK;
                self.idt_limit = limit;
                self.clk(12);
            }
            4 => {
                // SMSW - Store Machine Status Word
                self.put_rm_word(modrm, self.msw, bus);
                self.clk_modrm_prefetch(bus, modrm, 2, 3);
            }
            6 => {
                // LMSW - Load Machine Status Word (Bug #4: CPL=0 in PM)
                // On the 286, LMSW cannot clear PE once set.
                if self.is_protected_mode() && self.cpl() != 0 {
                    self.raise_fault_with_code(13, 0, bus);
                    return;
                }
                let value = self.get_rm_word(modrm, bus);
                let old_pe = self.msw & 1;
                self.msw = value | old_pe;
                self.clk_modrm_prefetch(bus, modrm, 3, 6);
            }
            _ => self.raise_fault(6, bus),
        }
    }

    fn lar(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.msw & 1 == 0 {
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
                (1..=7).contains(&desc_type)
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
                    let reg = self.reg_word(modrm);
                    self.regs.set_word(reg, (rights as u16) << 8);
                    self.flags.zero_val = 0; // ZF=1: valid
                }
            }
        }
        self.clk_modrm_prefetch(bus, modrm, 14, 16);
    }

    fn lsl_instr(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        if self.msw & 1 == 0 {
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
                (1..=3).contains(&desc_type)
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
                    let reg = self.reg_word(modrm);
                    self.regs.set_word(reg, descriptor.limit);
                    self.flags.zero_val = 0; // ZF=1: valid
                }
            }
        }
        self.clk_modrm_prefetch(bus, modrm, 14, 16);
    }

    fn verr_accessible(&self, selector: u16, bus: &mut impl common::Bus) -> bool {
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

        // Conforming code segments: DPL check exemption.
        if !Self::descriptor_is_conforming_code(rights) && dpl < cpl.max(rpl) {
            return false;
        }

        Self::descriptor_is_readable(rights)
    }

    fn selector_accessible(&self, selector: u16, write: bool, bus: &mut impl common::Bus) -> bool {
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
}
