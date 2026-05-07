//! BIOS HLE handler implementations.
//!
//! Each handler reads/writes CPU registers directly via the `Cpu` trait.
//! The ROM stubs save AX/DX on the stack (clobbered by the trap OUT),
//! write the vector number to the trap port, and IRET. The Rust side
//! restores AX/DX from the stack before dispatching to the handler.

mod bootstrap;
mod cmt_printer;
mod comed;
mod crt;
mod disk;
mod floppy_interrupt;
mod graphics;
mod keyboard;
mod pmode;
mod serial_rs232c;
mod timer;

use common::{Cpu, SegmentRegister, warn};

use super::{
    Pc9801Bus,
    os_adapter::{OsCpuAccess, OsCursorAccess, OsDiskIo, OsMemoryAccess},
};
use crate::{Tracing, memory::Pc9801Memory};

const PIT_CLOCK_8MHZ_LINEAGE: u32 = 1_996_800;
const PAGE_PRESENT: u32 = 0x01;
const PAGE_ACCESSED: u32 = 0x20;
const PAGE_DIRTY: u32 = 0x40;

fn iret_stack_base(cpu: &impl Cpu) -> u32 {
    cpu.segment_base(SegmentRegister::SS)
        .wrapping_add(u32::from(cpu.sp()))
}

fn hle_read_dword(memory: &Pc9801Memory, address: u32) -> u32 {
    memory.read_byte(address) as u32
        | ((memory.read_byte(address + 1) as u32) << 8)
        | ((memory.read_byte(address + 2) as u32) << 16)
        | ((memory.read_byte(address + 3) as u32) << 24)
}

fn hle_write_dword(memory: &mut Pc9801Memory, address: u32, value: u32) {
    memory.write_byte(address, value as u8);
    memory.write_byte(address + 1, (value >> 8) as u8);
    memory.write_byte(address + 2, (value >> 16) as u8);
    memory.write_byte(address + 3, (value >> 24) as u8);
}

fn hle_page_translate_access(
    cr0: u32,
    cr3: u32,
    linear: u32,
    write: bool,
    memory: &mut Pc9801Memory,
) -> u32 {
    if cr0 & 0x8000_0001 != 0x8000_0001 {
        return linear;
    }
    let dir_idx = (linear >> 22) & 0x3FF;
    let tbl_idx = (linear >> 12) & 0x3FF;
    let offset = linear & 0xFFF;
    let pde_addr = (cr3 & 0xFFFFF000) + dir_idx * 4;
    let pde = hle_read_dword(memory, pde_addr);
    if pde & PAGE_PRESENT == 0 {
        return linear;
    }
    let pte_addr = (pde & 0xFFFFF000) + tbl_idx * 4;
    let pte = hle_read_dword(memory, pte_addr);
    if pte & PAGE_PRESENT == 0 {
        return linear;
    }

    let accessed_pde = pde | PAGE_ACCESSED;
    if accessed_pde != pde {
        hle_write_dword(memory, pde_addr, accessed_pde);
    }

    let mut accessed_pte = pte | PAGE_ACCESSED;
    if write {
        accessed_pte |= PAGE_DIRTY;
    }
    if accessed_pte != pte {
        hle_write_dword(memory, pte_addr, accessed_pte);
    }

    (pte & 0xFFFFF000) | offset
}

pub(super) fn hle_page_translate_read(
    cr0: u32,
    cr3: u32,
    linear: u32,
    memory: &mut Pc9801Memory,
) -> u32 {
    hle_page_translate_access(cr0, cr3, linear, false, memory)
}

pub(super) fn hle_page_translate_write(
    cr0: u32,
    cr3: u32,
    linear: u32,
    memory: &mut Pc9801Memory,
) -> u32 {
    hle_page_translate_access(cr0, cr3, linear, true, memory)
}

fn boot_sector_has_signature(data: &[u8]) -> bool {
    data.len() >= 0x400 && data[0x3FE] == 0x55 && data[0x3FF] == 0xAA
}

/// Computes the CGROM byte offset for a Kanji character given JIS row/col.
///
/// The font ROM uses an interleaved layout: each JIS column occupies a 4096-byte block,
/// with rows packed at 16-byte intervals within. Left half at the computed offset,
/// right half at offset + 0x800.
fn cgrom_kanji_offset(jis_row: u8, jis_col: u8) -> u32 {
    let col = (jis_col & 0x7F) as u32;
    let row = (jis_row.wrapping_sub(0x20) & 0x7F) as u32;
    col * 0x1000 + row * 16
}

fn reverse_bits(b: u8) -> u8 {
    let mut v = b;
    v = (v & 0xF0) >> 4 | (v & 0x0F) << 4;
    v = (v & 0xCC) >> 2 | (v & 0x33) << 2;
    (v & 0xAA) >> 1 | (v & 0x55) << 1
}

impl<T: Tracing> Pc9801Bus<T> {
    pub(crate) fn handle_bios_interval_timer_tick(&mut self) {
        if !self.bios_interval_timer_active {
            return;
        }

        let count = self.ram_read_u16(0x058A);
        let new_count = count.wrapping_sub(1);
        self.ram_write_u16(0x058A, new_count);

        if count > 0 && new_count == 0 {
            self.bios_interval_timer_active = false;
        }
    }

    /// Configures paging state used by HLE BIOS routines (SASI, INT 1Fh, etc.).
    /// When paging is active (CR0.PG + CR0.PE), HLE memory accesses translate
    /// linear addresses through the page tables rooted at CR3.
    pub fn set_hle_paging(&mut self, cr0: u32, cr3: u32) {
        self.hle_cr0 = cr0;
        self.hle_cr3 = cr3;
    }

    /// Executes the pending BIOS HLE operation with direct CPU register access.
    pub(crate) fn execute_bios_hle(&mut self, cpu: &mut impl Cpu) {
        let vector = self.bios.pending_vector();
        self.bios.clear_hle_pending();

        // The assembly stub pushes AX and DX before clobbering them with the
        // trap port address and vector number. Restore the caller's original
        // values and adjust SP so the IRET frame sits at SS:SP+0.
        let sp = cpu.sp();
        let ss_base = cpu.segment_base(SegmentRegister::SS);
        let saved_dx = self.read_word_direct(ss_base.wrapping_add(u32::from(sp)));
        let saved_ax = self.read_word_direct(ss_base.wrapping_add(u32::from(sp.wrapping_add(2))));
        cpu.set_dx(saved_dx);
        cpu.set_ax(saved_ax);
        cpu.set_sp(sp.wrapping_add(4));

        self.tracer.trace_bios_hle(vector, cpu.ah(), cpu.al());

        match vector {
            0x08 => self.hle_int08h(cpu),
            0x09 => self.hle_int09h(cpu),
            0x0A => {
                self.pic.write_port0(0, 0x20);
                self.display_control.state.vsync_irq_enabled = true;
            }
            0x0B | 0x0D | 0x0E => self.pic.write_port0(0, 0x20),
            0x0C => self.hle_int0ch(cpu),
            0x10 | 0x11 | 0x14..=0x17 => {
                self.pic.write_port0(1, 0x20);
                self.pic.write_port0(0, 0x20);
            }
            0x12 => self.hle_int12h(cpu),
            0x13 => self.hle_int13h(cpu),
            0x18 => self.hle_int18h(cpu),
            0x19 => self.hle_int19h(cpu),
            0x1A => self.hle_int1ah(cpu),
            0x1B => self.hle_int1bh(cpu),
            0x1C => self.hle_int1ch(cpu),
            0x1F => self.hle_int1fh(cpu),
            0x20..=0x2A | 0x2F | 0x33 | 0x67 | 0xDC | 0xE7 | 0xFE => {
                if let Some(mut neetan_os) = self.os.take() {
                    let mut cpu_access = OsCpuAccess(cpu);
                    let mut mem_access = OsMemoryAccess(&mut self.memory);
                    let mut disk_io = OsDiskIo {
                        floppy: &mut self.floppy,
                        sasi: &mut self.sasi,
                        ide: &mut self.ide,
                    };
                    let mut cursor_access = OsCursorAccess(&mut self.gdc_master.state);
                    neetan_os.dispatch(
                        vector,
                        &mut cpu_access,
                        &mut mem_access,
                        &mut disk_io,
                        &mut cursor_access,
                        &mut self.tracer,
                    );
                    self.os = Some(neetan_os);
                }
            }
            0xD2 => {}
            0xF0 => {
                if std::mem::take(&mut self.needs_full_reinit) {
                    self.initialize_post_boot_state();
                }
                self.hle_bootstrap(cpu);
            }
            0xF1 | 0xF2 => self.hle_bootstrap(cpu),
            0xF3 => self.hle_n88_basic_entry(),
            _ => {}
        }
    }

    fn hle_n88_basic_entry(&mut self) {
        warn!("N88-BASIC ROM entry invoked; this software requires an original ROM to run");
    }

    fn hle_linear_address(&self, cpu: &impl Cpu, seg: SegmentRegister, off: u32) -> u32 {
        cpu.segment_base(seg).wrapping_add(off)
    }

    fn hle_physical_address(
        &mut self,
        cpu: &impl Cpu,
        seg: SegmentRegister,
        off: u32,
        write: bool,
    ) -> u32 {
        let linear = self.hle_linear_address(cpu, seg, off);
        hle_page_translate_access(self.hle_cr0, self.hle_cr3, linear, write, &mut self.memory)
    }

    fn hle_read_byte(&mut self, cpu: &impl Cpu, seg: SegmentRegister, off: u32) -> u8 {
        let phys = self.hle_physical_address(cpu, seg, off, false);
        self.read_byte_direct(phys)
    }

    fn hle_write_byte(&mut self, cpu: &impl Cpu, seg: SegmentRegister, off: u32, value: u8) {
        let phys = self.hle_physical_address(cpu, seg, off, true);
        self.memory.write_byte(phys, value);
    }

    fn set_iret_cf(&mut self, cpu: &impl Cpu, error: bool) {
        let base = iret_stack_base(cpu);
        let flags_addr = base + 0x04;
        let mut flags = self.read_word_direct(flags_addr);
        if error {
            flags |= 0x0001;
        } else {
            flags &= !0x0001;
        }
        self.memory.write_byte(flags_addr, flags as u8);
        self.memory.write_byte(flags_addr + 1, (flags >> 8) as u8);
    }

    fn write_result_ah_cf(&mut self, cpu: &mut impl Cpu, result_ah: u8) {
        cpu.set_ah(result_ah);
        self.set_iret_cf(cpu, result_ah >= 0x20);
    }

    fn write_mem_word(&mut self, addr: u32, value: u16) {
        self.memory.write_byte(addr, value as u8);
        self.memory.write_byte(addr + 1, (value >> 8) as u8);
    }

    fn ram_read_u16(&self, addr: usize) -> u16 {
        u16::from_le_bytes([self.memory.state.ram[addr], self.memory.state.ram[addr + 1]])
    }

    fn ram_write_u16(&mut self, addr: usize, value: u16) {
        let bytes = value.to_le_bytes();
        self.memory.state.ram[addr] = bytes[0];
        self.memory.state.ram[addr + 1] = bytes[1];
    }

    fn fdc_drain_results(fdc: &mut device::upd765a_fdc::Upd765aFdc, dest: &mut [u8]) -> usize {
        let mut count = 0;
        while count < dest.len() {
            let status = fdc.read_status();
            if status & 0xD0 != 0xD0 {
                // Not (RQM | DIO | CB) - no more result bytes.
                break;
            }
            dest[count] = fdc.read_data();
            count += 1;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use common::MachineModel;

    use super::{
        PAGE_ACCESSED, PAGE_DIRTY, hle_page_translate_read, hle_page_translate_write,
        hle_read_dword, hle_write_dword,
    };
    use crate::memory::Pc9801Memory;

    fn test_memory() -> Pc9801Memory {
        Pc9801Memory::new(MachineModel::PC9801RA, 0x400000)
    }

    #[test]
    fn hle_translate_read_sets_accessed_bits() {
        let mut memory = test_memory();
        let cr0 = 0x8000_0001;
        let cr3 = 0x0000_1000;
        let linear = 0x0040_1234;
        let pde_addr = cr3 + 4;
        let pte_addr = 0x0000_2004;

        hle_write_dword(&mut memory, pde_addr, 0x0000_2003);
        hle_write_dword(&mut memory, pte_addr, 0x0000_3003);

        let physical = hle_page_translate_read(cr0, cr3, linear, &mut memory);

        assert_eq!(physical, 0x0000_3234);
        assert_eq!(
            hle_read_dword(&memory, pde_addr),
            0x0000_2003 | PAGE_ACCESSED
        );
        assert_eq!(
            hle_read_dword(&memory, pte_addr),
            0x0000_3003 | PAGE_ACCESSED
        );
    }

    #[test]
    fn hle_translate_write_sets_accessed_and_dirty_bits() {
        let mut memory = test_memory();
        let cr0 = 0x8000_0001;
        let cr3 = 0x0000_1000;
        let linear = 0x0040_1234;
        let pde_addr = cr3 + 4;
        let pte_addr = 0x0000_2004;

        hle_write_dword(&mut memory, pde_addr, 0x0000_2003);
        hle_write_dword(&mut memory, pte_addr, 0x0000_3003);

        let physical = hle_page_translate_write(cr0, cr3, linear, &mut memory);

        assert_eq!(physical, 0x0000_3234);
        assert_eq!(
            hle_read_dword(&memory, pde_addr),
            0x0000_2003 | PAGE_ACCESSED
        );
        assert_eq!(
            hle_read_dword(&memory, pte_addr),
            0x0000_3003 | PAGE_ACCESSED | PAGE_DIRTY
        );
    }

    #[test]
    fn hle_translate_non_present_returns_linear_without_side_effects() {
        let mut memory = test_memory();
        let cr0 = 0x8000_0001;
        let cr3 = 0x0000_1000;
        let linear = 0x0040_1234;
        let pde_addr = cr3 + 4;

        hle_write_dword(&mut memory, pde_addr, 0x0000_2002);

        let physical = hle_page_translate_write(cr0, cr3, linear, &mut memory);

        assert_eq!(physical, linear);
        assert_eq!(hle_read_dword(&memory, pde_addr), 0x0000_2002);
    }

    #[test]
    fn hle_translate_paging_disabled_returns_linear() {
        let mut memory = test_memory();
        let linear = 0x0040_1234;

        let physical = hle_page_translate_write(0, 0x0000_1000, linear, &mut memory);

        assert_eq!(physical, linear);
    }
}
