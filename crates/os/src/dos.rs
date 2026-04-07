//! INT 21h function dispatcher (AH routing).

use crate::{CpuAccess, MemoryAccess, NeetanOs, tables};

impl NeetanOs {
    /// Dispatches an INT 21h call based on the AH register.
    pub(crate) fn int21h(&mut self, cpu: &mut dyn CpuAccess, _memory: &mut dyn MemoryAccess) {
        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x52 => self.int21h_52h_get_sysvars(cpu),
            _ => unimplemented!("INT 21h AH={:#04X}", ah),
        }
    }

    /// AH=52h: Get List of Lists (SYSVARS pointer).
    /// Returns ES:BX pointing to SYSVARS.
    fn int21h_52h_get_sysvars(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_es(tables::SYSVARS_SEGMENT);
        cpu.set_bx(tables::SYSVARS_OFFSET);
    }
}
