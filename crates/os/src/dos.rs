//! INT 21h function dispatcher (AH routing).

use common::warn;

use crate::{
    BufferedInputState, CpuAccess, DiskIo, MemoryAccess, NeetanOs, adjust_iret_ip, country, memory,
    set_iret_carry, set_iret_zf, tables,
};

impl NeetanOs {
    /// Dispatches an INT 21h call based on the AH register.
    pub(crate) fn int21h(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        disk: &mut dyn DiskIo,
    ) {
        let indos_addr = self.state.indos_addr;
        let indos = memory.read_byte(indos_addr);
        memory.write_byte(indos_addr, indos.wrapping_add(1));

        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x00 => self.terminate_process(cpu, memory, 0, 0),
            0x01 => self.int21h_01h_keyboard_input_with_echo(cpu, memory),
            0x02 => self.int21h_02h_display_character(cpu, memory),
            0x06 => self.int21h_06h_direct_console_io(cpu, memory),
            0x07 => self.int21h_07h_direct_char_input(cpu, memory),
            0x08 => self.int21h_08h_char_input_no_echo(cpu, memory),
            0x09 => self.int21h_09h_display_string(cpu, memory),
            0x0A => self.int21h_0ah_buffered_input(cpu, memory),
            0x0B => self.int21h_0bh_check_keyboard_status(cpu, memory),
            0x0C => self.int21h_0ch_flush_and_invoke(cpu, memory),
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

        let indos = memory.read_byte(indos_addr);
        memory.write_byte(indos_addr, indos.wrapping_sub(1));
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

    /// Reads one key byte for INT 21h input functions.
    ///
    /// Extended keys (arrows, function keys) have ch=0x00 in the keyboard buffer.
    /// NEC DOS IO.SYS expands these into the programmed escape sequences from
    /// the function key map (INT DCh CL=0x0C/0x0D). This method queues the
    /// escape sequence bytes and returns them one at a time.
    ///
    /// Returns `Some(byte)` if a byte is available, `None` if the keyboard
    /// buffer is empty (and no pending bytes).
    fn read_input_byte(&mut self, memory: &mut dyn MemoryAccess) -> Option<u8> {
        if let Some(byte) = self.state.pending_key_bytes.pop_front() {
            return Some(byte);
        }
        if !tables::key_available(memory) {
            return None;
        }
        let (scan, ch) = tables::read_key(memory);
        if ch == 0x00 {
            // Extended key: look up the escape sequence in the function key map.
            if let Some(seq) = self.lookup_fnkey_sequence(scan) {
                if seq.is_empty() {
                    // No mapping: return raw 0x00 + scan code (legacy fallback).
                    self.state.pending_key_bytes.push_back(scan);
                    return Some(0x00);
                }
                // Queue remaining bytes after the first one.
                for &b in &seq[1..] {
                    self.state.pending_key_bytes.push_back(b);
                }
                return Some(seq[0]);
            }
            // Unknown scan code: return raw 0x00 + scan code.
            self.state.pending_key_bytes.push_back(scan);
            return Some(0x00);
        }
        Some(ch)
    }

    /// Returns true if an input byte is ready (pending bytes or key in buffer).
    fn input_byte_available(&self, memory: &dyn MemoryAccess) -> bool {
        !self.state.pending_key_bytes.is_empty() || tables::key_available(memory)
    }

    /// Looks up the escape sequence for a hardware scan code in the function key map.
    /// Returns the sequence bytes (up to the first NUL), or None if not a mapped key.
    fn lookup_fnkey_sequence(&self, scan: u8) -> Option<Vec<u8>> {
        // Map hardware scan code to fn_key_map offset and slot size.
        // fn_key_map layout (specifier 0x0000):
        //   0-159:   F1-F10 (10 x 16 bytes), scan codes 0x62-0x6B
        //   160-319: Shift+F1-F10 (10 x 16 bytes) -- shifted versions, not mapped by scan
        //   320+:    editing keys (11 x 6 bytes):
        //     0=ROLL UP(0x36), 1=ROLL DOWN(0x37), 2=INS(0x38), 3=DEL(0x39),
        //     4=UP(0x3A), 5=LEFT(0x3B), 6=RIGHT(0x3C), 7=DOWN(0x3D),
        //     8=HOME(0x3E), 9=HELP(0x3F), 10=SHIFT+HOME
        let (offset, max_len) = match scan {
            0x62..=0x6B => {
                let idx = (scan - 0x62) as usize;
                (idx * 16, 15)
            }
            0x36..=0x3F => {
                let idx = (scan - 0x36) as usize;
                (320 + idx * 6, 5)
            }
            _ => return None,
        };

        let map = &self.state.fn_key_map;
        let mut seq = Vec::new();
        for i in 0..max_len {
            let b = map.get(offset + i).copied().unwrap_or(0);
            if b == 0 {
                break;
            }
            seq.push(b);
        }
        Some(seq)
    }

    /// AH=01h: Keyboard input with echo (blocking).
    /// Waits for a key, echoes it, returns AL = character.
    fn int21h_01h_keyboard_input_with_echo(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        match self.read_input_byte(memory) {
            Some(ch) => {
                self.console.process_byte(memory, ch);
                cpu.set_ax((cpu.ax() & 0xFF00) | ch as u16);
            }
            None => adjust_iret_ip(cpu, memory, -2),
        }
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
            match self.read_input_byte(memory) {
                Some(ch) => {
                    cpu.set_ax((cpu.ax() & 0xFF00) | ch as u16);
                    set_iret_zf(cpu, memory, false);
                }
                None => {
                    cpu.set_ax(cpu.ax() & 0xFF00);
                    set_iret_zf(cpu, memory, true);
                }
            }
            return;
        }
        self.console.process_byte(memory, dl);
        cpu.set_ax((cpu.ax() & 0xFF00) | dl as u16);
    }

    /// AH=07h: Direct character input without echo (blocking, no Ctrl+C check).
    /// Waits for a key, returns AL = character.
    fn int21h_07h_direct_char_input(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        match self.read_input_byte(memory) {
            Some(ch) => cpu.set_ax((cpu.ax() & 0xFF00) | ch as u16),
            None => adjust_iret_ip(cpu, memory, -2),
        }
    }

    /// AH=08h: Character input without echo (blocking, with Ctrl+C check).
    /// Waits for a key, returns AL = character.
    fn int21h_08h_char_input_no_echo(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        match self.read_input_byte(memory) {
            Some(ch) => cpu.set_ax((cpu.ax() & 0xFF00) | ch as u16),
            None => adjust_iret_ip(cpu, memory, -2),
        }
    }

    /// AH=0Ah: Buffered keyboard input (blocking, with echo).
    /// DS:DX -> buffer: byte[0]=max chars, byte[1]=actual count, byte[2+]=data.
    fn int21h_0ah_buffered_input(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        if self.state.buffered_input.is_none() {
            let buffer_addr = ((cpu.ds() as u32) << 4) + cpu.dx() as u32;
            let max_chars = memory.read_byte(buffer_addr);
            if max_chars == 0 {
                return;
            }
            self.state.buffered_input = Some(BufferedInputState {
                buffer_addr,
                max_chars,
                current_pos: 0,
            });
        }

        let ch = match self.read_input_byte(memory) {
            Some(ch) => ch,
            None => {
                adjust_iret_ip(cpu, memory, -2);
                return;
            }
        };
        let bi = self.state.buffered_input.as_mut().unwrap();

        match ch {
            0x0D => {
                let addr = bi.buffer_addr;
                let pos = bi.current_pos;
                memory.write_byte(addr + 1, pos);
                memory.write_byte(addr + 2 + pos as u32, 0x0D);
                self.console.process_byte(memory, b'\r');
                self.console.process_byte(memory, b'\n');
                self.state.buffered_input = None;
            }
            0x08 => {
                if let Some(bi) = self.state.buffered_input.as_mut()
                    && bi.current_pos > 0
                {
                    bi.current_pos -= 1;
                    self.console.process_byte(memory, 0x08);
                    self.console.process_byte(memory, b' ');
                    self.console.process_byte(memory, 0x08);
                }
                adjust_iret_ip(cpu, memory, -2);
            }
            _ => {
                let bi = self.state.buffered_input.as_mut().unwrap();
                if bi.current_pos < bi.max_chars.saturating_sub(1) {
                    let addr = bi.buffer_addr + 2 + bi.current_pos as u32;
                    memory.write_byte(addr, ch);
                    bi.current_pos += 1;
                    self.console.process_byte(memory, ch);
                }
                adjust_iret_ip(cpu, memory, -2);
            }
        }
    }

    /// AH=0Bh: Check keyboard status (non-blocking).
    /// Returns AL = FFh if key available, 00h if not.
    fn int21h_0bh_check_keyboard_status(&self, cpu: &mut dyn CpuAccess, memory: &dyn MemoryAccess) {
        let al: u8 = if self.input_byte_available(memory) {
            0xFF
        } else {
            0x00
        };
        cpu.set_ax((cpu.ax() & 0xFF00) | al as u16);
    }

    /// AH=0Ch: Flush input buffer and invoke input function.
    /// AL = function to invoke (01h, 06h, 07h, 08h, or 0Ah).
    fn int21h_0ch_flush_and_invoke(
        &mut self,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
    ) {
        tables::flush_keyboard_buffer(memory);
        self.state.pending_key_bytes.clear();
        let al = (cpu.ax() & 0xFF) as u8;
        match al {
            0x01 => self.int21h_01h_keyboard_input_with_echo(cpu, memory),
            0x06 => self.int21h_06h_direct_console_io(cpu, memory),
            0x07 => self.int21h_07h_direct_char_input(cpu, memory),
            0x08 => self.int21h_08h_char_input_no_echo(cpu, memory),
            0x0A => self.int21h_0ah_buffered_input(cpu, memory),
            _ => {}
        }
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
