//! INT 2Fh: DOS Multiplex Interrupt.
//!
//! Dispatched by AH register. Provides installation checks for resident
//! services (Windows, XMS, DOSKEY, HMA).

use common::warn;

use crate::{CpuAccess, MemoryAccess, NeetanOs};

impl NeetanOs {
    /// Dispatches an INT 2Fh call based on the AH register.
    pub(crate) fn int2fh(&self, cpu: &mut dyn CpuAccess, _memory: &mut dyn MemoryAccess) {
        // TODO: We don't handle this correctly. Both AL and AH needs to be tested.
        //       See: https://www.stanislavs.org/helppc/int_2f.html
        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x16 => self.int2fh_16h_windows_check(cpu),
            0x43 => self.int2fh_43h_xms_check(cpu),
            0x48 => self.int2fh_48h_doskey_check(cpu),
            0x4A => self.int2fh_4ah_hma_query(cpu),
            _ => warn!("INT 2Fh AH={ah:#04X} in unimplemented"),
        }
    }

    /// AH=16h: Windows enhanced mode check.
    /// Returns AL=00h (no Windows running).
    fn int2fh_16h_windows_check(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_ax(cpu.ax() & 0xFF00);
    }

    /// AH=43h: XMS driver installation check.
    /// Returns AL=00h (no XMS driver installed).
    fn int2fh_43h_xms_check(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_ax(cpu.ax() & 0xFF00);
    }

    /// AH=48h: DOSKEY installation check.
    /// Returns AL=00h (DOSKEY not installed).
    fn int2fh_48h_doskey_check(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_ax(cpu.ax() & 0xFF00);
    }

    /// AH=4Ah, AL=01h: HMA (High Memory Area) query.
    /// Returns BX=0000h (no HMA free space).
    fn int2fh_4ah_hma_query(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_bx(0x0000);
    }
}
