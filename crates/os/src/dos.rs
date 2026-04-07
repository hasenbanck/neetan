//! INT 21h function dispatcher (AH routing).

use crate::{CpuAccess, MemoryAccess, NeetanOs, tables};

impl NeetanOs {
    /// Dispatches an INT 21h call based on the AH register.
    pub(crate) fn int21h(&mut self, cpu: &mut dyn CpuAccess, _memory: &mut dyn MemoryAccess) {
        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x34 => self.int21h_34h_get_indos(cpu),
            0x52 => self.int21h_52h_get_sysvars(cpu),
            0x62 => self.int21h_62h_get_psp(cpu),
            _ => unimplemented!("INT 21h AH={:#04X}", ah),
        }
    }

    /// AH=34h: Get address of InDOS flag.
    /// Returns ES:BX pointing to the InDOS byte.
    fn int21h_34h_get_indos(&self, cpu: &mut dyn CpuAccess) {
        let segment = (self.indos_addr >> 4) as u16;
        let offset = (self.indos_addr & 0x0F) as u16;
        cpu.set_es(segment);
        cpu.set_bx(offset);
    }

    /// AH=52h: Get List of Lists (SYSVARS pointer).
    /// Returns ES:BX pointing to SYSVARS.
    fn int21h_52h_get_sysvars(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_es(tables::SYSVARS_SEGMENT);
        cpu.set_bx(tables::SYSVARS_OFFSET);
    }

    /// AH=62h: Get PSP address.
    /// Returns BX = segment of current PSP.
    fn int21h_62h_get_psp(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_bx(self.current_psp);
    }
}
