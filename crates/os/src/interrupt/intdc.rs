//! INT DCh: NEC DOS Extension (IO.SYS replacement).
//!
//! Dispatched by the CL register with subfunctions in AX. This is a NEC-only
//! interrupt with no IBM PC equivalent. NEETAN OS provides this handler since
//! it replaces IO.SYS.

use common::warn;

use crate::{CpuAccess, MemoryAccess, NeetanOs, tables};

impl NeetanOs {
    /// Dispatches an INT DCh call based on the CL register.
    pub(crate) fn intdch(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let cl = (cpu.cx() & 0xFF) as u8;
        match cl {
            0x00..=0x08 => {}
            0x0C => self.intdch_0ch_read_fnkey_map(cpu, memory),
            0x0D => self.intdch_0dh_write_fnkey_map(cpu, memory),
            0x10 => self.intdch_10h_console(cpu, memory),
            0x12 => self.intdch_12h_system_identification(cpu, memory),
            0x13 => self.intdch_13h_daua_mapping_buffer(cpu, memory),
            0x15 => self.intdch_15h_internal_revision(cpu, memory),
            0x80 => self.intdch_80h_disk_partition_info(cpu, memory),
            0x81 => self.intdch_81h_extended_memory_query(cpu, memory),
            _ => warn!("INT DCh CL={cl:#04X} is unimplemented"),
        }
    }

    /// CL=10h: Console display subfunctions (dispatched by AH).
    fn intdch_10h_console(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x00 => {
                // Single character output.
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.process_byte(memory, dl);
            }
            0x01 => {
                // String display. DS:DX = string, BX = length.
                let addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;
                let length = cpu.bx() as u32;
                for i in 0..length {
                    let byte = memory.read_byte(addr + i);
                    self.console.process_byte(memory, byte);
                }
            }
            0x02 => {
                // Set attribute.
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.set_attribute(memory, dl);
            }
            0x03 => {
                // Cursor positioning. DH = row, DL = column.
                let dh = (cpu.dx() >> 8) as u8;
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.set_cursor_position(memory, dh, dl);
            }
            0x04 => {
                // Cursor down 1 line (with scroll at bottom).
                self.console.linefeed(memory);
            }
            0x05 => {
                // Cursor up 1 line (with scroll at top).
                self.console.reverse_linefeed(memory);
            }
            0x06 => {
                // Cursor up N lines (clamp, no scroll).
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.cursor_up(memory, dl.max(1));
            }
            0x07 => {
                // Cursor down N lines (clamp, no scroll).
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.cursor_down(memory, dl.max(1));
            }
            0x08 => {
                // Cursor right N columns (clamp).
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.cursor_right(memory, dl.max(1));
            }
            0x09 => {
                // Cursor left N columns (clamp).
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.cursor_left(memory, dl.max(1));
            }
            0x0A => {
                // Erase in display.
                let dl = (cpu.dx() & 0xFF) as u8;
                if dl == 2 {
                    self.console.clear_screen(memory);
                }
            }
            0x0B => {
                // Erase in line.
                let dl = (cpu.dx() & 0xFF) as u8;
                match dl {
                    0 => self.console.clear_line_from_cursor(memory),
                    1 => self.console.clear_line_to_cursor(memory),
                    2 => self.console.clear_line(memory),
                    _ => {}
                }
            }
            0x0C => {
                // Insert lines (scroll down).
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.scroll_down(memory, dl.max(1));
            }
            0x0D => {
                // Delete lines (scroll up).
                let dl = (cpu.dx() & 0xFF) as u8;
                self.console.scroll_up(memory, dl.max(1));
            }
            0x0E => {
                // Kanji/graph mode switching.
                let dl = (cpu.dx() & 0xFF) as u8;
                match dl {
                    0 => {
                        memory.write_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_KANJI_MODE, 0x01);
                        memory.write_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_GRAPH_CHAR, 0x20);
                    }
                    3 => {
                        memory.write_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_KANJI_MODE, 0x00);
                        memory.write_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_GRAPH_CHAR, 0x67);
                    }
                    _ => {}
                }
            }
            _ => warn!("INT DCh CL=10h AH={ah:#04X} is unimplemented"),
        }
    }

    /// CL=12h: System identification.
    /// Returns AX = product number from 0060:0020h, DX = machine type.
    fn intdch_12h_system_identification(&self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        let product = memory.read_word(tables::IOSYS_BASE + tables::IOSYS_OFF_PRODUCT_NUMBER);
        cpu.set_ax(product);
        cpu.set_dx(0x0003); // normal-mode PC-98
    }

    /// CL=13h: Fill 96-byte DA/UA mapping buffer at DS:DX.
    fn intdch_13h_daua_mapping_buffer(&self, cpu: &dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let buffer_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;
        let base = tables::IOSYS_BASE;

        // +00h-0Fh: 16 bytes from legacy DA/UA table (0060:006Ch).
        for i in 0..16u32 {
            let byte = memory.read_byte(base + tables::IOSYS_OFF_DAUA_TABLE + i);
            memory.write_byte(buffer_addr + i, byte);
        }

        // +10h-19h: 10 bytes reserved (zero).
        for i in 0x10..0x1Au32 {
            memory.write_byte(buffer_addr + i, 0x00);
        }

        // +1Ah-4Dh: 52 bytes from extended DA/UA table (0060:2C86h).
        for i in 0..tables::IOSYS_EXT_DAUA_TABLE_SIZE {
            let byte = memory.read_byte(base + tables::IOSYS_OFF_EXT_DAUA_TABLE + i);
            memory.write_byte(buffer_addr + 0x1A + i, byte);
        }

        // +4Eh: FD logical drive duplicate flag (0060:0038h).
        let fd_dup = memory.read_byte(base + tables::IOSYS_OFF_FD_DUPLICATE);
        memory.write_byte(buffer_addr + 0x4E, fd_dup);

        // +4Fh: FD logical drive duplicate flag (0060:013Bh).
        let fd_dup2 = memory.read_byte(base + tables::IOSYS_OFF_FD_DUPLICATE2);
        memory.write_byte(buffer_addr + 0x4F, fd_dup2);

        // +50h: Last accessed drive number (0060:0136h).
        let last_drive = memory.read_byte(base + tables::IOSYS_OFF_LAST_DRIVE_UNIT);
        memory.write_byte(buffer_addr + 0x50, last_drive);

        // +51h-5Fh: 15 bytes reserved (zero).
        for i in 0x51..0x60u32 {
            memory.write_byte(buffer_addr + i, 0x00);
        }
    }

    /// CL=15h: Internal IO.SYS revision.
    /// Returns AL = revision byte from 0060:0022h.
    fn intdch_15h_internal_revision(&self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        let revision = memory.read_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_INTERNAL_REVISION);
        cpu.set_ax((cpu.ax() & 0xFF00) | revision as u16);
    }

    /// CL=80h: Disk/partition information.
    /// AL = drive number (0=A, 1=B, ...). Returns basic partition info.
    fn intdch_80h_disk_partition_info(&self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        let drive = (cpu.ax() & 0xFF) as u8;
        let daua = if (drive as u32) < 16 {
            memory.read_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_DAUA_TABLE + drive as u32)
        } else {
            0x00
        };

        if daua == 0x00 {
            cpu.set_ax(0x0002); // invalid drive
            cpu.set_bx(0x0000);
        } else if daua & 0xF0 == 0x80 {
            // HDD: one partition per unit in HLE mode.
            cpu.set_ax(0x0000);
            cpu.set_bx(0x0001);
        } else {
            // FDD or other non-partitioned device.
            cpu.set_ax(0x0000);
            cpu.set_bx(0x0000);
        }
    }

    /// CL=81h: Extended memory query.
    /// Returns AL = extended memory size in 128KB units from 0060:0031h.
    fn intdch_81h_extended_memory_query(&self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        let ext_mem = memory.read_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_EXT_MEM_128K);
        cpu.set_ax((cpu.ax() & 0xFF00) | ext_mem as u16);
    }

    /// CL=0Ch: Read programmable function key mapping.
    /// AX = key specifier, DS:DX = destination buffer.
    fn intdch_0ch_read_fnkey_map(&self, cpu: &dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let key_specifier = cpu.ax();
        let buffer_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;

        let (src_offset, length) = match key_specifier {
            0x0000 => (0, 786),
            0x0001..=0x000A => {
                let idx = (key_specifier - 1) as usize;
                (idx * 16, 16)
            }
            0x000B..=0x0014 => {
                let idx = (key_specifier - 0x000B) as usize;
                (160 + idx * 16, 16)
            }
            0x0015..=0x001F => {
                let idx = (key_specifier - 0x0015) as usize;
                (320 + idx * 6, 6)
            }
            _ => return,
        };

        for i in 0..length {
            let byte = self
                .state
                .fn_key_map
                .get(src_offset + i)
                .copied()
                .unwrap_or(0);
            memory.write_byte(buffer_addr + i as u32, byte);
        }
    }

    /// CL=0Dh: Write programmable function key mapping.
    /// AX = key specifier, DS:DX = source buffer.
    fn intdch_0dh_write_fnkey_map(&mut self, cpu: &dyn CpuAccess, memory: &dyn MemoryAccess) {
        let key_specifier = cpu.ax();
        let buffer_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;

        let (dst_offset, length) = match key_specifier {
            0x0000 => (0, 786),
            0x0001..=0x000A => {
                let idx = (key_specifier - 1) as usize;
                (idx * 16, 16)
            }
            0x000B..=0x0014 => {
                let idx = (key_specifier - 0x000B) as usize;
                (160 + idx * 16, 16)
            }
            0x0015..=0x001F => {
                let idx = (key_specifier - 0x0015) as usize;
                (320 + idx * 6, 6)
            }
            _ => return,
        };

        for i in 0..length {
            let byte = memory.read_byte(buffer_addr + i as u32);
            if let Some(dest) = self.state.fn_key_map.get_mut(dst_offset + i) {
                *dest = byte;
            }
        }
    }
}
