use super::{CPU_MODEL_486, I386};

/// Number of TLB entries (direct-mapped by page number).
pub const TLB_SIZE: usize = 64;
/// Mask that selects the TLB slot index from a page number.
pub const TLB_MASK: u32 = (TLB_SIZE as u32) - 1;

const PTE_PRESENT: u32 = 0x01;
const PTE_WRITABLE: u32 = 0x02;
const PTE_USER: u32 = 0x04;
const PTE_ACCESSED: u32 = 0x20;
const PTE_DIRTY: u32 = 0x40;

/// Direct-mapped TLB used by [`I386::translate_linear`].
///
/// Stored inline on [`I386State`](super::state::I386State) so the dynarec
/// backend can reach the arrays through stable offsets from the state
/// pointer it already holds in a host register.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TlbCache {
    /// Whether the slot carries a valid translation.
    pub valid: [bool; TLB_SIZE],
    /// Page number (linear >> 12) tagging each slot.
    pub tag: [u32; TLB_SIZE],
    /// Physical page base (PTE & 0xFFFF_F000) per slot.
    pub phys: [u32; TLB_SIZE],
    /// Whether the slot is writable under current CPL/WP rules.
    pub writable: [bool; TLB_SIZE],
    /// Whether the slot's PTE.Dirty has already been set.
    pub dirty: [bool; TLB_SIZE],
}

impl Default for TlbCache {
    fn default() -> Self {
        Self {
            valid: [false; TLB_SIZE],
            tag: [0; TLB_SIZE],
            phys: [0; TLB_SIZE],
            writable: [false; TLB_SIZE],
            dirty: [false; TLB_SIZE],
        }
    }
}

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
    #[inline(always)]
    pub(super) fn is_paging_enabled(&self) -> bool {
        self.cr0 & 0x8000_0001 == 0x8000_0001
    }

    pub(super) fn flush_tlb(&mut self) {
        self.state.tlb.valid = [false; TLB_SIZE];
        self.state.tlb.dirty = [false; TLB_SIZE];
        self.fetch_page_valid = false;
    }

    /// Translates a linear address to a physical address, consulting the TLB
    /// and walking the page tables on miss. Returns `None` on page fault;
    /// the caller should inspect `fault_pending` to detect the fault.
    #[inline(always)]
    pub fn translate_linear(
        &mut self,
        linear: u32,
        write: bool,
        bus: &mut impl common::Bus,
    ) -> Option<u32> {
        if !self.is_paging_enabled() {
            return Some(linear);
        }

        let page = linear >> 12;
        let slot = (page & TLB_MASK) as usize;

        if self.state.tlb.valid[slot] && self.state.tlb.tag[slot] == page {
            if write && (!self.state.tlb.writable[slot] || !self.state.tlb.dirty[slot]) {
                return self.page_table_walk(linear, write, bus);
            }
            return Some(self.state.tlb.phys[slot] | (linear & 0xFFF));
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
        self.state.tlb.valid[slot] = true;
        self.state.tlb.tag[slot] = page;
        self.state.tlb.phys[slot] = physical_page;
        self.state.tlb.dirty[slot] = write;
        // Writable in TLB if both PDE and PTE allow writes (or supervisor without WP).
        let wp_enforced = CPU_MODEL == CPU_MODEL_486 && self.cr0 & 0x0001_0000 != 0;
        self.state.tlb.writable[slot] = if is_user || wp_enforced {
            pde & PTE_WRITABLE != 0 && pte & PTE_WRITABLE != 0
        } else {
            true
        };

        Some(physical)
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
        if addr & 0xFFF <= 0xFFC {
            return bus.read_dword(addr);
        }
        bus.read_byte(addr) as u32
            | ((bus.read_byte(addr.wrapping_add(1)) as u32) << 8)
            | ((bus.read_byte(addr.wrapping_add(2)) as u32) << 16)
            | ((bus.read_byte(addr.wrapping_add(3)) as u32) << 24)
    }

    #[inline(always)]
    pub(super) fn write_dword_phys_raw(&self, bus: &mut impl common::Bus, addr: u32, value: u32) {
        if addr & 0xFFF <= 0xFFC {
            bus.write_dword(addr, value);
            return;
        }
        bus.write_byte(addr, value as u8);
        bus.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
        bus.write_byte(addr.wrapping_add(2), (value >> 16) as u8);
        bus.write_byte(addr.wrapping_add(3), (value >> 24) as u8);
    }
}
