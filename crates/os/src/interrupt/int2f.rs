//! INT 2Fh: DOS Multiplex Interrupt.
//!
//! Dispatched by AH register. Provides installation checks for resident
//! services (Windows, XMS, DOSKEY, HMA).

use common::warn;

use crate::{CdromIo, CpuAccess, MemoryAccess, NeetanOs};

impl NeetanOs {
    /// Dispatches an INT 2Fh call based on the AH register.
    pub(crate) fn int2fh(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        cdrom: &mut dyn CdromIo,
    ) {
        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x15 => self.int2fh_15h_mscdex(cpu, memory, cdrom),
            0x16 => self.int2fh_16h_windows_check(cpu),
            0x43 => self.int2fh_43h_xms_check(cpu),
            0x48 => self.int2fh_48h_doskey_check(cpu),
            0x4A => self.int2fh_4ah_hma_query(cpu),
            _ => warn!("INT 2Fh AH={ah:#04X} is unimplemented"),
        }
    }

    /// AH=15h: MSCDEX CD-ROM interface.
    fn int2fh_15h_mscdex(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        cdrom: &mut dyn CdromIo,
    ) {
        let al = cpu.ax() as u8;
        match al {
            0x00 => {
                // Installation check.
                if cdrom.cdrom_present() {
                    cpu.set_bx(1); // 1 CD-ROM drive.
                    cpu.set_cx(u16::from(self.state.mscdex.drive_letter));
                } else {
                    cpu.set_bx(0);
                }
            }
            0x0B => {
                // CD-ROM drive check.
                let drive = cpu.cx() as u8;
                if cdrom.cdrom_present() && drive == self.state.mscdex.drive_letter {
                    cpu.set_ax(cpu.ax() | 0x00FF); // Non-zero AL = is CD-ROM.
                    cpu.set_bx(0xADAD);
                } else {
                    cpu.set_ax(cpu.ax() & 0xFF00); // AL=0 = not CD-ROM.
                }
            }
            0x0C => {
                // MSCDEX version.
                cpu.set_bx(0x020A); // Version 2.10.
            }
            0x0D => {
                // Get CD-ROM drive letters.
                if cdrom.cdrom_present() {
                    let buffer_addr = (cpu.es() as u32) << 4 | cpu.bx() as u32;
                    memory.write_byte(buffer_addr, self.state.mscdex.drive_letter);
                }
            }
            0x10 => {
                // Send device driver request.
                let es = cpu.es() as u32;
                let bx = cpu.bx() as u32;
                let request_addr = (es << 4) | bx;
                if cdrom.cdrom_present() {
                    self.handle_device_request(memory, cdrom, request_addr);
                }
            }
            _ => {
                warn!("INT 2Fh AX=15{al:02X}h is unimplemented");
            }
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
