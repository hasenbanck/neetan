use super::{I386, CPU_MODEL_486};

const TLB_SIZE: usize = 64;
const TLB_MASK: u32 = (TLB_SIZE as u32) - 1;

const PTE_PRESENT: u32 = 0x01;
const PTE_WRITABLE: u32 = 0x02;
const PTE_USER: u32 = 0x04;
const PTE_ACCESSED: u32 = 0x20;
const PTE_DIRTY: u32 = 0x40;

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
    #[inline(always)]
    pub(super) fn is_paging_enabled(&self) -> bool {
        self.cr0 & 0x8000_0001 == 0x8000_0001
    }

    pub(super) fn flush_tlb(&mut self) {
        self.tlb_valid = [false; TLB_SIZE];
    }

    #[inline(always)]
    pub(super) fn translate_linear_probe(&self, linear: u32) -> Option<u32> {
        if !self.is_paging_enabled() {
            return Some(linear & 0x00FF_FFFF);
        }
        let page = linear >> 12;
        let slot = (page & TLB_MASK) as usize;
        if self.tlb_valid[slot] && self.tlb_tag[slot] == page {
            Some(self.tlb_phys[slot] | (linear & 0xFFF))
        } else {
            None
        }
    }

    #[inline(always)]
    pub(super) fn translate_linear(
        &mut self,
        linear: u32,
        write: bool,
        bus: &mut impl common::Bus,
    ) -> Option<u32> {
        if !self.is_paging_enabled() {
            return Some(linear & 0x00FF_FFFF);
        }

        let page = linear >> 12;
        let slot = (page & TLB_MASK) as usize;

        if self.tlb_valid[slot] && self.tlb_tag[slot] == page {
            if write && !self.tlb_writable[slot] {
                return self.page_table_walk(linear, write, bus);
            }
            return Some(self.tlb_phys[slot] | (linear & 0xFFF));
        }

        self.page_table_walk(linear, write, bus)
    }

    fn page_table_walk(
        &mut self,
        linear: u32,
        write: bool,
        bus: &mut impl common::Bus,
    ) -> Option<u32> {
        let dir_index = (linear >> 22) & 0x3FF;
        let table_index = (linear >> 12) & 0x3FF;
        let offset = linear & 0xFFF;

        let pde_addr = (self.cr3 & 0xFFFF_F000) | (dir_index << 2);
        let pde = self.read_dword_phys_raw(bus, pde_addr);

        if pde & PTE_PRESENT == 0 {
            self.raise_page_fault(linear, write, false, bus);
            return None;
        }

        let pte_addr = (pde & 0xFFFF_F000) | (table_index << 2);
        let pte = self.read_dword_phys_raw(bus, pte_addr);

        if pte & PTE_PRESENT == 0 {
            self.raise_page_fault(linear, write, false, bus);
            return None;
        }

        // Permission check: user mode needs U/S=1 on both PDE and PTE.
        // For writes in user mode, both R/W bits must be set.
        // Supervisor (CPL 0-2) can always write regardless of R/W on a 386.
        // On 486, CR0.WP (bit 16) enforces R/W checks even in supervisor mode.
        // System table accesses (IDT, GDT, TSS) during interrupt delivery use
        // supervisor privilege regardless of current CPL.
        let is_user = self.cpl() == 3 && !self.supervisor_override;
        if is_user {
            if pde & PTE_USER == 0 || pte & PTE_USER == 0 {
                self.raise_page_fault(linear, write, true, bus);
                return None;
            }
            if write && (pde & PTE_WRITABLE == 0 || pte & PTE_WRITABLE == 0) {
                self.raise_page_fault(linear, write, true, bus);
                return None;
            }
        } else if write
            && CPU_MODEL == CPU_MODEL_486
            && self.cr0 & 0x0001_0000 != 0
            && (pde & PTE_WRITABLE == 0 || pte & PTE_WRITABLE == 0)
        {
            self.raise_page_fault(linear, write, true, bus);
            return None;
        }

        // Set accessed bit on PDE if not already set.
        if pde & PTE_ACCESSED == 0 {
            self.write_dword_phys_raw(bus, pde_addr, pde | PTE_ACCESSED);
        }

        // Set accessed (and dirty if write) on PTE.
        let mut new_pte = pte | PTE_ACCESSED;
        if write {
            new_pte |= PTE_DIRTY;
        }
        if new_pte != pte {
            self.write_dword_phys_raw(bus, pte_addr, new_pte);
        }

        let physical_page = pte & 0xFFFF_F000;
        let physical = physical_page | offset;

        // Fill TLB.
        let page = linear >> 12;
        let slot = (page & TLB_MASK) as usize;
        self.tlb_valid[slot] = true;
        self.tlb_tag[slot] = page;
        self.tlb_phys[slot] = physical_page;
        // Writable in TLB if both PDE and PTE allow writes (or supervisor without WP).
        let wp_enforced = CPU_MODEL == CPU_MODEL_486 && self.cr0 & 0x0001_0000 != 0;
        self.tlb_writable[slot] = if is_user || wp_enforced {
            pde & PTE_WRITABLE != 0 && pte & PTE_WRITABLE != 0
        } else {
            true
        };

        Some(physical & 0x00FF_FFFF)
    }

    fn raise_page_fault(
        &mut self,
        linear: u32,
        write: bool,
        present: bool,
        bus: &mut impl common::Bus,
    ) {
        self.cr2 = linear;
        let mut error_code: u16 = 0;
        if present {
            error_code |= 1; // P bit
        }
        if write {
            error_code |= 2; // W/R bit
        }
        if self.cpl() == 3 {
            error_code |= 4; // U/S bit
        }
        self.fault_pending = true;
        self.raise_fault_with_code(14, error_code, bus);
    }

    #[inline(always)]
    pub(super) fn read_dword_phys_raw(&self, bus: &mut impl common::Bus, addr: u32) -> u32 {
        let a = addr & 0x00FF_FFFF;
        bus.read_byte(a) as u32
            | ((bus.read_byte(a.wrapping_add(1) & 0x00FF_FFFF) as u32) << 8)
            | ((bus.read_byte(a.wrapping_add(2) & 0x00FF_FFFF) as u32) << 16)
            | ((bus.read_byte(a.wrapping_add(3) & 0x00FF_FFFF) as u32) << 24)
    }

    #[inline(always)]
    pub(super) fn write_dword_phys_raw(&self, bus: &mut impl common::Bus, addr: u32, value: u32) {
        let a = addr & 0x00FF_FFFF;
        bus.write_byte(a, value as u8);
        bus.write_byte(a.wrapping_add(1) & 0x00FF_FFFF, (value >> 8) as u8);
        bus.write_byte(a.wrapping_add(2) & 0x00FF_FFFF, (value >> 16) as u8);
        bus.write_byte(a.wrapping_add(3) & 0x00FF_FFFF, (value >> 24) as u8);
    }
}
