//! INT 21h function dispatcher (AH routing).

use common::warn;

use crate::{CpuAccess, DiskIo, MemoryAccess, NeetanOs, country, memory, set_iret_carry, tables};

impl NeetanOs {
    /// Dispatches an INT 21h call based on the AH register.
    pub(crate) fn int21h(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        disk: &mut dyn DiskIo,
    ) {
        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x00 => self.terminate_process(cpu, memory, 0, 0),
            0x02 => self.int21h_02h_display_character(cpu, memory),
            0x06 => self.int21h_06h_direct_console_io(cpu, memory),
            0x07 => unimplemented!("INT 21h AH=07h: direct character input without echo"),
            0x08 => unimplemented!("INT 21h AH=08h: character input without echo"),
            0x09 => self.int21h_09h_display_string(cpu, memory),
            0x0A => unimplemented!("INT 21h AH=0Ah: buffered keyboard input"),
            0x0C => unimplemented!("INT 21h AH=0Ch: flush input buffer and invoke input"),
            0x0D => self.int21h_0dh_disk_reset(disk),
            0x0E => self.int21h_0eh_select_drive(cpu, memory),
            0x19 => self.int21h_19h_get_current_drive(cpu),
            0x1A => self.int21h_1ah_set_dta(cpu),
            0x1C => self.int21h_1ch_get_alloc_info(cpu, memory),
            0x25 => self.int21h_25h_set_interrupt_vector(cpu, memory),
            0x29 => self.int21h_29h_parse_filename(cpu, memory),
            0x2F => self.int21h_2fh_get_dta(cpu),
            0x30 => self.int21h_30h_get_version(cpu),
            0x33 => self.int21h_33h_extended(cpu),
            0x34 => self.int21h_34h_get_indos(cpu),
            0x35 => self.int21h_35h_get_interrupt_vector(cpu, memory),
            0x37 => self.int21h_37h_switch_char(cpu),
            0x38 => self.int21h_38h_get_country_info(cpu, memory),
            0x3B => self.int21h_3bh_chdir(cpu, memory),
            0x3C => self.int21h_3ch_create_file(cpu, memory, disk),
            0x3D => self.int21h_3dh_open_file(cpu, memory, disk),
            0x3E => self.int21h_3eh_close_handle(cpu, memory, disk),
            0x3F => self.int21h_3fh_read(cpu, memory, disk),
            0x40 => self.int21h_40h_write(cpu, memory, disk),
            0x41 => self.int21h_41h_delete_file(cpu, memory, disk),
            0x42 => self.int21h_42h_lseek(cpu, memory),
            0x43 => self.int21h_43h_get_set_attributes(cpu, memory, disk),
            0x44 => self.int21h_44h_ioctl(cpu, memory, disk),
            0x45 => self.int21h_45h_dup_handle(cpu, memory),
            0x47 => self.int21h_47h_get_current_directory(cpu, memory),
            0x48 => self.int21h_48h_allocate(cpu, memory),
            0x49 => self.int21h_49h_free(cpu, memory),
            0x4A => self.int21h_4ah_resize(cpu, memory),
            0x31 => self.int21h_31h_tsr(cpu, memory),
            0x4B => self.int21h_4bh_exec(cpu, memory, disk),
            0x4C => self.int21h_4ch_terminate(cpu, memory),
            0x4D => self.int21h_4dh_get_return_code(cpu),
            0x4E => self.int21h_4eh_find_first(cpu, memory, disk),
            0x4F => self.int21h_4fh_find_next(cpu, memory, disk),
            0x50 => self.int21h_50h_set_psp(cpu),
            0x51 => self.int21h_51h_get_psp(cpu),
            0x52 => self.int21h_52h_get_sysvars(cpu),
            0x56 => self.int21h_56h_rename(cpu, memory, disk),
            0x57 => self.int21h_57h_get_set_datetime(cpu, memory),
            0x58 => self.int21h_58h_allocation_strategy(cpu, memory),
            0x5D => self.int21h_5dh_server_call(cpu, memory),
            0x62 => self.int21h_62h_get_psp(cpu),
            0x63 => self.int21h_63h_get_dbcs_table(cpu),
            0x65 => self.int21h_65h_get_extended_country_info(cpu, memory),
            0xFF => self.int21h_ffh_shell_step(cpu, memory, disk),
            _ => warn!("INT 21h AH={ah:#04X} is unimplemented"),
        }
    }

    /// AH=02h: Display character.
    /// DL = character to display.
    /// Returns AL = last character output.
    fn int21h_02h_display_character(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        let dl = (cpu.dx() & 0xFF) as u8;
        self.console.process_byte(memory, dl);
        cpu.set_ax((cpu.ax() & 0xFF00) | dl as u16);
    }

    /// AH=06h: Direct console I/O.
    /// DL = character to output (if DL != FFh).
    /// DL = FFh: input request (returns ZF=1 if no char, ZF=0 + AL=char if available).
    fn int21h_06h_direct_console_io(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        let dl = (cpu.dx() & 0xFF) as u8;
        if dl == 0xFF {
            unimplemented!("INT 21h AH=06h DL=FFh: console input not yet implemented");
        }
        self.console.process_byte(memory, dl);
        cpu.set_ax((cpu.ax() & 0xFF00) | dl as u16);
    }

    /// AH=09h: Display string.
    /// DS:DX = pointer to '$'-terminated string.
    /// Returns AL = 0x24 ('$').
    fn int21h_09h_display_string(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        let mut addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;
        for _ in 0..0xFFFFu32 {
            let byte = memory.read_byte(addr);
            if byte == b'$' {
                break;
            }
            self.console.process_byte(memory, byte);
            addr += 1;
        }
        cpu.set_ax((cpu.ax() & 0xFF00) | 0x24);
    }

    /// AH=0Eh: Select default drive.
    /// DL = new default drive (0=A, 1=B, ...).
    /// Returns AL = number of logical drives (LASTDRIVE).
    fn int21h_0eh_select_drive(&mut self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        self.state.current_drive = cpu.dx() as u8;
        let lastdrive = memory.read_byte(self.state.sysvars_base + tables::SYSVARS_OFF_LASTDRIVE);
        cpu.set_ax((cpu.ax() & 0xFF00) | lastdrive as u16);
    }

    /// AH=19h: Get current default drive.
    /// Returns AL = current drive (0=A, 1=B, ...).
    fn int21h_19h_get_current_drive(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_ax((cpu.ax() & 0xFF00) | self.state.current_drive as u16);
    }

    /// AH=1Ah: Set Disk Transfer Area address.
    /// DS:DX = new DTA address.
    fn int21h_1ah_set_dta(&mut self, cpu: &dyn CpuAccess) {
        self.state.dta_segment = cpu.ds();
        self.state.dta_offset = cpu.dx();
    }

    /// AH=25h: Set interrupt vector.
    /// AL = interrupt number, DS:DX = new handler address.
    fn int21h_25h_set_interrupt_vector(&self, cpu: &dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let vector = (cpu.ax() & 0xFF) as u32;
        let ivt_addr = vector * 4;
        memory.write_word(ivt_addr, cpu.dx());
        memory.write_word(ivt_addr + 2, cpu.ds());
    }

    /// AH=2Fh: Get DTA address.
    /// Returns ES:BX = current DTA address.
    fn int21h_2fh_get_dta(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_es(self.state.dta_segment);
        cpu.set_bx(self.state.dta_offset);
    }

    /// AH=30h: Get DOS version number.
    /// Returns AL=major (6), AH=minor (20), BH=OEM, BL=0.
    fn int21h_30h_get_version(&self, cpu: &mut dyn CpuAccess) {
        let (major, minor) = self.state.version;
        cpu.set_ax((minor as u16) << 8 | major as u16);
        // BH=OEM serial number (0x00 = IBM/NEC compatible), BL=0x00
        cpu.set_bx(0x0000);
    }

    /// AH=33h: Extended functions.
    /// AL=00h: Get Ctrl-Break check state -> DL.
    /// AL=01h: Set Ctrl-Break check state <- DL.
    /// AL=06h: Get true DOS version -> BL=major, BH=minor.
    fn int21h_33h_extended(&mut self, cpu: &mut dyn CpuAccess) {
        let al = (cpu.ax() & 0xFF) as u8;
        match al {
            0x00 => {
                cpu.set_dx((cpu.dx() & 0xFF00) | self.state.ctrl_break as u16);
            }
            0x01 => {
                self.state.ctrl_break = (cpu.dx() & 0x00FF) != 0;
            }
            0x06 => {
                let (major, minor) = self.state.version;
                cpu.set_bx((minor as u16) << 8 | major as u16);
                // DL=revision (0), DH=version flags (0)
                cpu.set_dx(0x0000);
            }
            _ => {}
        }
    }

    /// AH=34h: Get address of InDOS flag.
    /// Returns ES:BX pointing to the InDOS byte.
    fn int21h_34h_get_indos(&self, cpu: &mut dyn CpuAccess) {
        let segment = (self.state.indos_addr >> 4) as u16;
        let offset = (self.state.indos_addr & 0x0F) as u16;
        cpu.set_es(segment);
        cpu.set_bx(offset);
    }

    /// AH=35h: Get interrupt vector.
    /// AL = interrupt number.
    /// Returns ES:BX = handler address.
    fn int21h_35h_get_interrupt_vector(&self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        let vector = (cpu.ax() & 0xFF) as u32;
        let ivt_addr = vector * 4;
        let offset = memory.read_word(ivt_addr);
        let segment = memory.read_word(ivt_addr + 2);
        cpu.set_es(segment);
        cpu.set_bx(offset);
    }

    /// AH=37h: Get/set switch character (undocumented).
    /// AL=00h: Get -> DL = switch char, AL = 0.
    /// AL=01h: Set <- DL, AL = 0.
    fn int21h_37h_switch_char(&mut self, cpu: &mut dyn CpuAccess) {
        let al = (cpu.ax() & 0xFF) as u8;
        match al {
            0x00 => {
                cpu.set_dx((cpu.dx() & 0xFF00) | self.state.switch_char as u16);
                cpu.set_ax(cpu.ax() & 0xFF00);
            }
            0x01 => {
                self.state.switch_char = (cpu.dx() & 0xFF) as u8;
                cpu.set_ax(cpu.ax() & 0xFF00);
            }
            _ => {
                // Unknown subfunction: return AL=FFh
                cpu.set_ax((cpu.ax() & 0xFF00) | 0xFF);
            }
        }
    }

    /// AH=38h: Get country-dependent information.
    /// AL=00h: Get current country info. DS:DX = 34-byte buffer. BX = country code on return.
    fn int21h_38h_get_country_info(&self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let buffer_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;
        country::write_country_info(memory, buffer_addr);
        cpu.set_bx(country::COUNTRY_CODE);
        set_iret_carry(cpu, memory, false);
    }

    /// AH=3Bh: Change current directory (CHDIR).
    /// DS:DX = ASCIIZ pathname.
    fn int21h_3bh_chdir(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let path_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;

        let mut path_bytes = Vec::new();
        for i in 0..80u32 {
            let byte = memory.read_byte(path_addr + i);
            if byte == 0 {
                break;
            }
            path_bytes.push(byte);
        }

        match self.state.change_directory(memory, &path_bytes) {
            Ok(()) => {
                set_iret_carry(cpu, memory, false);
            }
            Err(error_code) => {
                cpu.set_ax(error_code);
                set_iret_carry(cpu, memory, true);
            }
        }
    }

    /// AH=47h: Get current directory.
    /// DL = drive (0=default, 1=A, 2=B, ...).
    /// DS:SI = 64-byte buffer for path (without leading backslash).
    fn int21h_47h_get_current_directory(
        &self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        let dl = (cpu.dx() & 0xFF) as u8;
        let drive_index = if dl == 0 {
            self.state.current_drive
        } else {
            dl - 1
        };

        if drive_index >= 26 {
            cpu.set_ax(0x000F); // invalid drive
            set_iret_carry(cpu, memory, true);
            return;
        }

        let cds_addr = tables::CDS_BASE + (drive_index as u32) * tables::CDS_ENTRY_SIZE;
        let cds_flags = memory.read_word(cds_addr + tables::CDS_OFF_FLAGS);
        if cds_flags == 0 {
            cpu.set_ax(0x000F);
            set_iret_carry(cpu, memory, true);
            return;
        }

        // Read CDS path
        let mut path = Vec::new();
        for i in 0..67u32 {
            let byte = memory.read_byte(cds_addr + tables::CDS_OFF_PATH + i);
            if byte == 0 {
                break;
            }
            path.push(byte);
        }

        // Copy everything after "X:\" to the buffer
        let buffer_addr = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
        let skip = if path.len() >= 3 && path[1] == b':' && path[2] == b'\\' {
            3
        } else if path.len() >= 2 && path[1] == b':' {
            2
        } else {
            0
        };

        let remaining = &path[skip..];
        for (i, &byte) in remaining.iter().enumerate() {
            memory.write_byte(buffer_addr + i as u32, byte);
        }
        memory.write_byte(buffer_addr + remaining.len() as u32, 0x00);

        set_iret_carry(cpu, memory, false);
    }

    /// AH=48h: Allocate memory block.
    /// BX = number of paragraphs requested.
    /// Success: CF=0, AX = segment of allocated block.
    /// Failure: CF=1, AX = 8 (insufficient memory), BX = largest available.
    fn int21h_48h_allocate(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let paragraphs = cpu.bx();
        let first_mcb = memory.read_word(self.state.sysvars_base - 2);
        match memory::allocate(
            memory,
            first_mcb,
            paragraphs,
            self.state.current_psp,
            self.state.allocation_strategy,
        ) {
            Ok(segment) => {
                cpu.set_ax(segment);
                set_iret_carry(cpu, memory, false);
            }
            Err((error_code, largest)) => {
                cpu.set_ax(error_code as u16);
                cpu.set_bx(largest);
                set_iret_carry(cpu, memory, true);
            }
        }
    }

    /// AH=49h: Free memory block.
    /// ES = segment of block to free.
    /// Success: CF=0.
    /// Failure: CF=1, AX = error code.
    fn int21h_49h_free(&self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let data_segment = cpu.es();
        let first_mcb = memory.read_word(self.state.sysvars_base - 2);
        match memory::free(memory, first_mcb, data_segment) {
            Ok(()) => {
                set_iret_carry(cpu, memory, false);
            }
            Err(error_code) => {
                cpu.set_ax(error_code as u16);
                set_iret_carry(cpu, memory, true);
            }
        }
    }

    /// AH=4Ah: Resize memory block (SETBLOCK).
    /// ES = segment of block, BX = new size in paragraphs.
    /// Success: CF=0.
    /// Failure: CF=1, AX = error code, BX = max available paragraphs.
    fn int21h_4ah_resize(&self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let data_segment = cpu.es();
        let new_paragraphs = cpu.bx();
        let first_mcb = memory.read_word(self.state.sysvars_base - 2);
        match memory::resize(memory, first_mcb, data_segment, new_paragraphs) {
            Ok(()) => {
                set_iret_carry(cpu, memory, false);
            }
            Err((error_code, max_available)) => {
                cpu.set_ax(error_code as u16);
                cpu.set_bx(max_available);
                set_iret_carry(cpu, memory, true);
            }
        }
    }

    /// AH=31h: Terminate and Stay Resident.
    /// AL = return code, DX = paragraphs to keep resident.
    fn int21h_31h_tsr(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let return_code = (cpu.ax() & 0xFF) as u8;
        let keep_paragraphs = cpu.dx();
        self.terminate_process_tsr(cpu, memory, return_code, keep_paragraphs);
    }

    /// AH=4Ch: Terminate process with return code.
    /// AL = return code.
    fn int21h_4ch_terminate(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let return_code = (cpu.ax() & 0xFF) as u8;
        self.terminate_process(cpu, memory, return_code, 0);
    }

    /// AH=4Dh: Get return code of child process.
    /// Returns AL = exit code, AH = termination type (0-3).
    fn int21h_4dh_get_return_code(&mut self, cpu: &mut dyn CpuAccess) {
        cpu.set_ax(
            (self.state.last_termination_type as u16) << 8 | self.state.last_return_code as u16,
        );
        // Clear after reading (one-shot)
        self.state.last_return_code = 0;
        self.state.last_termination_type = 0;
    }

    /// AH=50h: Set current PSP address (undocumented).
    /// BX = new PSP segment.
    fn int21h_50h_set_psp(&mut self, cpu: &dyn CpuAccess) {
        self.state.current_psp = cpu.bx();
    }

    /// AH=51h: Get current PSP address (undocumented).
    /// Returns BX = segment of current PSP.
    fn int21h_51h_get_psp(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_bx(self.state.current_psp);
    }

    /// AH=52h: Get List of Lists (SYSVARS pointer).
    /// Returns ES:BX pointing to SYSVARS.
    fn int21h_52h_get_sysvars(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_es(tables::SYSVARS_SEGMENT);
        cpu.set_bx(tables::SYSVARS_OFFSET);
    }

    /// AH=58h: Get/set memory allocation strategy.
    /// AL=00h: Get -> AX = strategy (0=first fit, 1=best fit, 2=last fit).
    /// AL=01h: Set <- BX = strategy.
    fn int21h_58h_allocation_strategy(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        let al = (cpu.ax() & 0xFF) as u8;
        match al {
            0x00 => {
                cpu.set_ax(self.state.allocation_strategy);
                set_iret_carry(cpu, memory, false);
            }
            0x01 => {
                self.state.allocation_strategy = cpu.bx();
                set_iret_carry(cpu, memory, false);
            }
            _ => {
                cpu.set_ax(0x0001); // invalid function
                set_iret_carry(cpu, memory, true);
            }
        }
    }

    /// AH=62h: Get PSP address.
    /// Returns BX = segment of current PSP.
    fn int21h_62h_get_psp(&self, cpu: &mut dyn CpuAccess) {
        cpu.set_bx(self.state.current_psp);
    }

    /// AH=63h: Get lead byte table (DBCS double-byte support).
    /// AL=00h: Returns DS:SI pointing to DBCS lead byte table.
    fn int21h_63h_get_dbcs_table(&self, cpu: &mut dyn CpuAccess) {
        let segment = (self.state.dbcs_table_addr >> 4) as u16;
        let offset = (self.state.dbcs_table_addr & 0x0F) as u16;
        cpu.set_ds(segment);
        cpu.set_si(offset);
    }

    /// AH=65h: Get extended country information.
    /// AL=01h: Get extended country info. ES:DI = buffer, CX = buffer size.
    /// AL=07h: Get DBCS table info. ES:DI = buffer, CX = buffer size.
    fn int21h_65h_get_extended_country_info(
        &self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        let al = (cpu.ax() & 0xFF) as u8;
        let buffer_addr = ((cpu.es() as u32) << 4) + cpu.di() as u32;
        let max_bytes = cpu.cx();

        match al {
            0x01 => {
                let written = country::write_extended_country_info(memory, buffer_addr, max_bytes);
                if written > 0 {
                    cpu.set_cx(written);
                    set_iret_carry(cpu, memory, false);
                } else {
                    cpu.set_ax(0x0001);
                    set_iret_carry(cpu, memory, true);
                }
            }
            0x07 => {
                let written = country::write_extended_dbcs_info(memory, buffer_addr, max_bytes);
                if written > 0 {
                    cpu.set_cx(written);
                    set_iret_carry(cpu, memory, false);
                } else {
                    cpu.set_ax(0x0001);
                    set_iret_carry(cpu, memory, true);
                }
            }
            _ => {
                cpu.set_ax(0x0001); // invalid function
                set_iret_carry(cpu, memory, true);
            }
        }
    }
}

/// Normalizes a DOS path by resolving `.` and `..` components.
/// Input/output is a byte vector like `A:\FOO\BAR\..\BAZ`.
pub(crate) fn normalize_path(path: &[u8]) -> Vec<u8> {
    // Find the root prefix (e.g. "A:\")
    let root_len = if path.len() >= 3 && path[1] == b':' && path[2] == b'\\' {
        3
    } else if path.len() >= 2 && path[1] == b':' {
        2
    } else {
        0
    };

    let prefix = &path[..root_len];
    let rest = &path[root_len..];

    let mut components: Vec<&[u8]> = Vec::new();
    for part in rest.split(|&b| b == b'\\') {
        if part.is_empty() || part == b"." {
            continue;
        } else if part == b".." {
            components.pop();
        } else {
            components.push(part);
        }
    }

    let mut result = Vec::from(prefix);
    for (i, component) in components.iter().enumerate() {
        if i > 0 {
            result.push(b'\\');
        }
        result.extend_from_slice(component);
    }

    // Ensure at least "X:\"
    if result.len() == 2 && result[1] == b':' {
        result.push(b'\\');
    }

    result
}
