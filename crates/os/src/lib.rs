//! NEETAN OS - HLE DOS implementation for PC-98.
//!
//! Provides a high-level emulation of MS-DOS 6.20 compatible OS services
//! for the PC-9801 series. The `NeetanOs` struct receives DOS interrupt
//! dispatch calls from the machine bus and delegates to per-interrupt
//! handler modules.

mod cdrom;
mod commands;
mod config;
mod console;
mod console_esc;
mod country;
mod dos;
mod filesystem;
mod interrupt;
mod ioctl;
mod memory;
mod process;
mod shell;
mod state;
pub mod tables;

/// CPU register access for the OS.
///
/// Implemented by the machine crate's bridge adapter, wrapping `common::Cpu`.
pub trait CpuAccess {
    /// Returns the AX register.
    fn ax(&self) -> u16;
    /// Sets the AX register.
    fn set_ax(&mut self, value: u16);
    /// Returns the BX register.
    fn bx(&self) -> u16;
    /// Sets the BX register.
    fn set_bx(&mut self, value: u16);
    /// Returns the CX register.
    fn cx(&self) -> u16;
    /// Sets the CX register.
    fn set_cx(&mut self, value: u16);
    /// Returns the DX register.
    fn dx(&self) -> u16;
    /// Sets the DX register.
    fn set_dx(&mut self, value: u16);
    /// Returns the SI register.
    fn si(&self) -> u16;
    /// Sets the SI register.
    fn set_si(&mut self, value: u16);
    /// Returns the DI register.
    fn di(&self) -> u16;
    /// Sets the DI register.
    fn set_di(&mut self, value: u16);
    /// Returns the DS segment register.
    fn ds(&self) -> u16;
    /// Sets the DS segment register.
    fn set_ds(&mut self, value: u16);
    /// Returns the ES segment register.
    fn es(&self) -> u16;
    /// Sets the ES segment register.
    fn set_es(&mut self, value: u16);
    /// Returns the SS segment register.
    fn ss(&self) -> u16;
    /// Returns the SP register.
    fn sp(&self) -> u16;
    /// Sets the SP register.
    fn set_sp(&mut self, value: u16);
    /// Returns the CS segment register.
    fn cs(&self) -> u16;
    /// Sets the carry flag in the IRET frame.
    fn set_carry(&mut self, carry: bool);
}

/// Emulated memory access for the OS.
///
/// Implemented by the machine crate's bridge adapter, wrapping `Pc9801Memory`.
pub trait MemoryAccess {
    /// Reads a byte from the given linear address.
    fn read_byte(&self, address: u32) -> u8;
    /// Writes a byte to the given linear address.
    fn write_byte(&mut self, address: u32, value: u8);
    /// Reads a 16-bit word (little-endian) from the given linear address.
    fn read_word(&self, address: u32) -> u16;
    /// Writes a 16-bit word (little-endian) to the given linear address.
    fn write_word(&mut self, address: u32, value: u16);
    /// Bulk read from emulated RAM into a host buffer.
    fn read_block(&self, address: u32, buf: &mut [u8]);
    /// Bulk write from a host buffer into emulated RAM.
    fn write_block(&mut self, address: u32, data: &[u8]);
}

/// Disk I/O for the filesystem layer.
///
/// Abstracts access to floppy and hard disk images through the machine bus.
pub trait DiskIo {
    /// Read sectors from a physical drive.
    /// `drive_da`: device address (0x90 for FD0, 0x80 for HDD0, etc.)
    /// `lba`: logical block address (0-based)
    /// `count`: number of sectors to read
    fn read_sectors(&mut self, drive_da: u8, lba: u32, count: u32) -> Result<Vec<u8>, u8>;
    /// Write sectors to a physical drive.
    fn write_sectors(&mut self, drive_da: u8, lba: u32, data: &[u8]) -> Result<(), u8>;
    /// Get the sector size for a drive (typically 512 for HDD, 512 or 1024 for FDD).
    fn sector_size(&self, drive_da: u8) -> Option<u16>;
    /// Get total sector count for a drive.
    fn total_sectors(&self, drive_da: u8) -> Option<u32>;
}

/// Console I/O for commands and the shell.
///
/// Abstracts keyboard input and text output through the machine's display.
pub trait ConsoleIo {
    /// Write a character to the console at the current cursor position.
    fn write_char(&mut self, ch: u8);
    /// Write a string to the console.
    fn write_str(&mut self, s: &[u8]);
    /// Read a character from the keyboard buffer (blocking).
    fn read_char(&mut self) -> u8;
    /// Check if a character is available in the keyboard buffer.
    fn char_available(&self) -> bool;
    /// Read a scan code + character pair (for special keys like arrows).
    fn read_key(&mut self) -> (u8, u8);
    /// Get current cursor position.
    fn cursor_position(&self) -> (u8, u8);
    /// Set cursor position.
    fn set_cursor_position(&mut self, row: u8, col: u8);
    /// Scroll the screen up by one line.
    fn scroll_up(&mut self);
    /// Clear the screen.
    fn clear_screen(&mut self);
    /// Get the screen dimensions.
    fn screen_size(&self) -> (u8, u8);
}

/// Information about a discovered drive for CDS/DPB/DAUA population.
struct DriveInfo {
    /// 0-based drive index (0=A, 1=B, ..., 25=Z).
    drive_index: u8,
    /// Device address (0x90=1MB FDD0, 0x80=HDD0, 0x00=virtual).
    da_ua: u8,
    /// True for the virtual Z: drive.
    is_virtual: bool,
}

/// The NEETAN OS HLE DOS instance.
///
/// Holds all DOS state: memory management, file handles, process info, etc.
/// Created when no bootable media is found, then called via `dispatch()` on
/// each DOS interrupt.
pub struct NeetanOs {
    /// Linear address of SYSVARS (List of Lists) in emulated RAM.
    sysvars_base: u32,
    /// Linear address of the InDOS flag byte.
    indos_addr: u32,
    /// Boot drive number (1=A, 2=B, ...). Default is 1 (A:).
    boot_drive: u8,
    /// DOS version reported to programs: (major, minor) = (6, 20).
    version: (u8, u8),
    /// Segment of the current PSP (set during boot and EXEC).
    current_psp: u16,
    /// Current default drive (0-based: 0=A, 1=B, ...).
    current_drive: u8,
    /// DTA (Disk Transfer Area) segment.
    dta_segment: u16,
    /// DTA (Disk Transfer Area) offset.
    dta_offset: u16,
    /// Ctrl-Break check state (false=off, true=on).
    ctrl_break: bool,
    /// Switch character (default 0x2F = '/').
    switch_char: u8,
    /// Memory allocation strategy (0=first fit, 1=best fit, 2=last fit).
    allocation_strategy: u16,
    /// Exit code from last terminated child process.
    last_return_code: u8,
    /// Termination type of last child (0=normal, 1=ctrl-C, 2=critical error, 3=TSR).
    last_termination_type: u8,
    /// Linear address of DBCS lead byte table in emulated RAM.
    dbcs_table_addr: u32,
}

impl Default for NeetanOs {
    fn default() -> Self {
        Self::new()
    }
}

impl NeetanOs {
    /// Creates a new NeetanOs instance.
    pub fn new() -> Self {
        Self {
            sysvars_base: tables::SYSVARS_BASE,
            indos_addr: tables::INDOS_FLAG_ADDR,
            boot_drive: 1,
            version: (6, 20),
            current_psp: 0,
            current_drive: 0,
            dta_segment: 0,
            dta_offset: 0x0080,
            ctrl_break: false,
            switch_char: 0x2F,
            allocation_strategy: 0,
            last_return_code: 0,
            last_termination_type: 0,
            dbcs_table_addr: 0,
        }
    }

    /// Performs the DOS boot sequence: writes data structures into emulated RAM,
    /// mounts drives, parses CONFIG.SYS, and creates the COMMAND.COM process.
    pub fn boot(
        &mut self,
        _cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        _disk: &mut dyn DiskIo,
        _console: &mut dyn ConsoleIo,
    ) {
        self.write_dos_data_structures(memory);
        self.write_iosys_work_area(memory);
        let drives = Self::discover_drives(memory);
        self.write_drive_structures(memory, &drives);
        self.write_command_com_process(memory);
    }

    /// Dispatches a DOS/OS interrupt to the appropriate handler.
    ///
    /// `vector`: interrupt number (0x20-0x2F, 0x33, 0xDC).
    /// Returns `true` if the interrupt was handled, `false` if the vector
    /// should fall through to the default IRET behavior.
    pub fn dispatch(
        &mut self,
        vector: u8,
        cpu: &mut dyn CpuAccess,
        memory: &mut dyn MemoryAccess,
        _disk: &mut dyn DiskIo,
        _console: &mut dyn ConsoleIo,
    ) -> bool {
        match vector {
            0x20 => {
                self.int20h(cpu, memory);
                true
            }
            0x21 => {
                self.int21h(cpu, memory);
                true
            }
            0x22 => false,
            0x23 => false,
            0x24 => false,
            0x25 => false,
            0x26 => false,
            0x27 => false,
            0x28 => {
                self.int28h(cpu, memory);
                true
            }
            0x29 => {
                self.int29h(cpu, memory);
                true
            }
            0x2A => true, // Critical section stubs: no-op
            0x2F => {
                self.int2fh(cpu, memory);
                true
            }
            0x33 => false,
            0xDC => {
                self.intdch(cpu, memory);
                true
            }
            _ => false,
        }
    }

    /// Writes SYSVARS, device chain, SFT, CDS, DPB, buffers, MCB into emulated RAM.
    fn write_dos_data_structures(&self, mem: &mut dyn MemoryAccess) {
        use tables::*;

        // Zero the DOS data area.
        let zeros = vec![0u8; (FIRST_MCB_ADDR + 16 - DOS_DATA_BASE) as usize];
        mem.write_block(DOS_DATA_BASE, &zeros);

        // SYSVARS -2: first MCB segment
        mem.write_word(SYSVARS_BASE - 2, FIRST_MCB_SEGMENT);

        // SYSVARS fields
        let (dpb_seg, dpb_off) = dos_data_far(DPB_OFFSET);
        write_far_ptr(mem, SYSVARS_BASE + SYSVARS_OFF_DPB_PTR, dpb_seg, dpb_off);

        let (sft_seg, sft_off) = dos_data_far(SFT_OFFSET);
        write_far_ptr(mem, SYSVARS_BASE + SYSVARS_OFF_SFT_PTR, sft_seg, sft_off);

        let (clock_seg, clock_off) = dos_data_far(DEV_CLOCK_OFFSET);
        write_far_ptr(
            mem,
            SYSVARS_BASE + SYSVARS_OFF_CLOCK_PTR,
            clock_seg,
            clock_off,
        );

        let (con_seg, con_off) = dos_data_far(DEV_CON_OFFSET);
        write_far_ptr(mem, SYSVARS_BASE + SYSVARS_OFF_CON_PTR, con_seg, con_off);

        // MAX_SECTOR is set later by write_drive_structures().

        let (buf_seg, buf_off) = dos_data_far(DISK_BUFFER_OFFSET);
        write_far_ptr(mem, SYSVARS_BASE + SYSVARS_OFF_BUFFER_PTR, buf_seg, buf_off);

        let (cds_seg, cds_off) = dos_data_far(CDS_OFFSET);
        write_far_ptr(mem, SYSVARS_BASE + SYSVARS_OFF_CDS_PTR, cds_seg, cds_off);

        let (fcb_seg, fcb_off) = dos_data_far(FCB_SFT_OFFSET);
        write_far_ptr(
            mem,
            SYSVARS_BASE + SYSVARS_OFF_FCB_SFT_PTR,
            fcb_seg,
            fcb_off,
        );

        mem.write_word(SYSVARS_BASE + SYSVARS_OFF_PROT_FCBS, 0);
        // BLOCK_DEVS is set later by write_drive_structures().
        mem.write_byte(SYSVARS_BASE + SYSVARS_OFF_LASTDRIVE, 26);

        // JOIN drives = 0
        mem.write_word(SYSVARS_BASE + SYSVARS_OFF_JOIN_DRIVES, 0);
        // SETVER list = NULL
        write_far_ptr(mem, SYSVARS_BASE + SYSVARS_OFF_SETVER_PTR, 0, 0);
        // BUFFERS
        mem.write_word(SYSVARS_BASE + SYSVARS_OFF_BUFFERS, 5);
        mem.write_word(SYSVARS_BASE + SYSVARS_OFF_LOOKAHEAD, 0);
        mem.write_byte(SYSVARS_BASE + SYSVARS_OFF_BOOT_DRIVE, self.boot_drive);
        mem.write_byte(SYSVARS_BASE + SYSVARS_OFF_386_FLAG, 0x01);
        mem.write_word(SYSVARS_BASE + SYSVARS_OFF_EXT_MEM, 0);

        // Device chain
        self.write_device_chain(mem);

        // SFT
        self.write_sft(mem);

        // CDS and DPB are populated by write_drive_structures().

        // Disk buffer header + one buffer
        self.write_disk_buffer(mem);

        // InDOS flag and critical error flag
        mem.write_byte(INDOS_FLAG_ADDR, 0x00);
        mem.write_byte(CRITICAL_ERROR_FLAG_ADDR, 0x00);

        // DBCS lead byte table (Shift-JIS ranges)
        mem.write_byte(DBCS_TABLE_ADDR, 0x81);
        mem.write_byte(DBCS_TABLE_ADDR + 1, 0x9F);
        mem.write_byte(DBCS_TABLE_ADDR + 2, 0xE0);
        mem.write_byte(DBCS_TABLE_ADDR + 3, 0xFC);
        mem.write_byte(DBCS_TABLE_ADDR + 4, 0x00);
        mem.write_byte(DBCS_TABLE_ADDR + 5, 0x00);

        // FCB-SFT header (no entries)
        write_far_ptr(mem, FCB_SFT_BASE, 0xFFFF, 0xFFFF);
        mem.write_word(FCB_SFT_BASE + 4, 0);
    }

    /// Writes the device header chain: NUL -> CON -> $AID#NEC -> MS$KANJI.
    /// CLOCK is separate (not in chain, only referenced by SYSVARS+0x08).
    fn write_device_chain(&self, mem: &mut dyn MemoryAccess) {
        use tables::*;

        let base = DOS_DATA_BASE;

        // NUL (embedded at SYSVARS+0x22) -> CON
        write_device_header(
            mem,
            base + DEV_NUL_OFFSET as u32,
            DOS_DATA_SEGMENT,
            DEV_CON_OFFSET,
            DEVATTR_CHAR | DEVATTR_NUL,
            b"NUL     ",
        );

        // CON -> $AID#NEC
        write_device_header(
            mem,
            base + DEV_CON_OFFSET as u32,
            DOS_DATA_SEGMENT,
            DEV_AID_NEC_OFFSET,
            DEVATTR_CHAR | DEVATTR_STDIN | DEVATTR_STDOUT | DEVATTR_SPECIAL,
            b"CON     ",
        );

        // CLOCK (not in chain)
        write_device_header(
            mem,
            base + DEV_CLOCK_OFFSET as u32,
            0xFFFF,
            0xFFFF,
            DEVATTR_CHAR | DEVATTR_CLOCK,
            b"CLOCK   ",
        );

        // $AID#NEC -> MS$KANJI
        write_device_header(
            mem,
            base + DEV_AID_NEC_OFFSET as u32,
            DOS_DATA_SEGMENT,
            DEV_MS_KANJI_OFFSET,
            DEVATTR_CHAR,
            b"$AID#NEC",
        );

        // MS$KANJI (end of chain)
        write_device_header(
            mem,
            base + DEV_MS_KANJI_OFFSET as u32,
            0xFFFF,
            0xFFFF,
            DEVATTR_CHAR,
            b"MS$KANJI",
        );
    }

    /// Writes the SFT header and 5 standard file entries.
    fn write_sft(&self, mem: &mut dyn MemoryAccess) {
        use tables::*;

        // SFT header: next = FFFF:FFFF, count = 5
        write_far_ptr(mem, SFT_BASE, 0xFFFF, 0xFFFF);
        mem.write_word(SFT_BASE + 4, 5);

        let entries_base = SFT_BASE + SFT_HEADER_SIZE;

        // CON device pointer (far ptr)
        let con_addr = DOS_DATA_BASE + DEV_CON_OFFSET as u32;
        let con_seg = DOS_DATA_SEGMENT;
        let con_off = DEV_CON_OFFSET;

        // Standard handles: stdin(0), stdout(1), stderr(2) -> CON
        for i in 0..3u32 {
            let entry = entries_base + i * SFT_ENTRY_SIZE;
            mem.write_word(entry + SFT_ENT_REF_COUNT, 1);
            mem.write_word(entry + SFT_ENT_OPEN_MODE, 0x0002); // read/write
            mem.write_byte(entry + SFT_ENT_FILE_ATTR, 0x00);
            mem.write_word(
                entry + SFT_ENT_DEV_INFO,
                SFT_DEVINFO_CHAR | SFT_DEVINFO_SPECIAL | SFT_DEVINFO_STDIN | SFT_DEVINFO_STDOUT,
            );
            write_far_ptr(mem, entry + SFT_ENT_DEV_PTR, con_seg, con_off);
            mem.write_block(entry + SFT_ENT_NAME, b"CON        ");
        }

        // Handle 3: AUX (stub)
        {
            let entry = entries_base + 3 * SFT_ENTRY_SIZE;
            mem.write_word(entry + SFT_ENT_REF_COUNT, 1);
            mem.write_word(entry + SFT_ENT_OPEN_MODE, 0x0002);
            mem.write_byte(entry + SFT_ENT_FILE_ATTR, 0x00);
            mem.write_word(entry + SFT_ENT_DEV_INFO, SFT_DEVINFO_CHAR);
            // Point to NUL device as a safe fallback
            let nul_off = DEV_NUL_OFFSET;
            write_far_ptr(mem, entry + SFT_ENT_DEV_PTR, DOS_DATA_SEGMENT, nul_off);
            mem.write_block(entry + SFT_ENT_NAME, b"AUX        ");
        }

        // Handle 4: PRN (stub)
        {
            let entry = entries_base + 4 * SFT_ENTRY_SIZE;
            mem.write_word(entry + SFT_ENT_REF_COUNT, 1);
            mem.write_word(entry + SFT_ENT_OPEN_MODE, 0x0002);
            mem.write_byte(entry + SFT_ENT_FILE_ATTR, 0x00);
            mem.write_word(entry + SFT_ENT_DEV_INFO, SFT_DEVINFO_CHAR);
            let nul_off = DEV_NUL_OFFSET;
            write_far_ptr(mem, entry + SFT_ENT_DEV_PTR, DOS_DATA_SEGMENT, nul_off);
            mem.write_block(entry + SFT_ENT_NAME, b"PRN        ");
        }

        // Suppress unused variable warning.
        let _ = con_addr;
    }

    /// Reads the BDA DISK_EQUIP word to discover equipped drives and assigns
    /// drive letters following the PC-98 floppy-first convention.
    fn discover_drives(mem: &dyn MemoryAccess) -> Vec<DriveInfo> {
        use tables::*;

        let disk_equip = mem.read_word(BDA_DISK_EQUIP);
        let mut drives = Vec::new();
        let mut next_index: u8 = 0;

        // 1MB FDD units (bits 0-3 of disk_equip).
        for unit in 0..4u8 {
            if disk_equip & (1 << unit) != 0 {
                drives.push(DriveInfo {
                    drive_index: next_index,
                    da_ua: 0x90 + unit,
                    is_virtual: false,
                });
                next_index += 1;
            }
        }

        // 640KB FDD units (bits 12-15 of disk_equip).
        for unit in 0..4u8 {
            if disk_equip & (1 << (12 + unit)) != 0 {
                drives.push(DriveInfo {
                    drive_index: next_index,
                    da_ua: 0x70 + unit,
                    is_virtual: false,
                });
                next_index += 1;
            }
        }

        // HDD units (bits 8-11 of disk_equip).
        for unit in 0..4u8 {
            if disk_equip & (1 << (8 + unit)) != 0 {
                drives.push(DriveInfo {
                    drive_index: next_index,
                    da_ua: 0x80 + unit,
                    is_virtual: false,
                });
                next_index += 1;
            }
        }

        // Z: virtual drive is always present.
        drives.push(DriveInfo {
            drive_index: 25,
            da_ua: 0x00,
            is_virtual: true,
        });

        drives
    }

    /// Populates CDS entries, DPB chain, DA/UA mapping tables, and SYSVARS
    /// counters based on the discovered drives.
    fn write_drive_structures(&self, mem: &mut dyn MemoryAccess, drives: &[DriveInfo]) {
        use tables::*;

        let mut max_sector_size: u16 = 512;

        for (chain_index, drive) in drives.iter().enumerate() {
            // DA/UA mapping table (16 bytes at 0060:006Ch, drives A:-P:).
            if drive.drive_index < 16 {
                mem.write_byte(
                    IOSYS_BASE + IOSYS_OFF_DAUA_TABLE + drive.drive_index as u32,
                    drive.da_ua,
                );
            }

            // Extended DA/UA table (52 bytes at 0060:2C86h, 2 bytes per drive).
            let ext_offset = IOSYS_BASE + IOSYS_OFF_EXT_DAUA_TABLE + (drive.drive_index as u32) * 2;
            mem.write_byte(ext_offset, 0x00); // attribute
            mem.write_byte(ext_offset + 1, drive.da_ua);

            // DPB entry.
            let dpb_addr = DPB_BASE + (chain_index as u32) * DPB_ENTRY_SIZE;
            self.write_dpb_for_drive(mem, dpb_addr, drive);

            // Track max sector size across all DPBs.
            let bytes_per_sector = mem.read_word(dpb_addr + DPB_OFF_BYTES_PER_SECTOR);
            if bytes_per_sector > max_sector_size {
                max_sector_size = bytes_per_sector;
            }

            // Chain to next DPB or terminate.
            if chain_index + 1 < drives.len() {
                let next_addr = DPB_BASE + ((chain_index + 1) as u32) * DPB_ENTRY_SIZE;
                let next_off = DPB_OFFSET + ((chain_index + 1) as u16) * (DPB_ENTRY_SIZE as u16);
                write_far_ptr(mem, dpb_addr + DPB_OFF_NEXT_DPB, DOS_DATA_SEGMENT, next_off);
                let _ = next_addr;
            } else {
                write_far_ptr(mem, dpb_addr + DPB_OFF_NEXT_DPB, 0xFFFF, 0xFFFF);
            }

            // CDS entry.
            let cds_addr = CDS_BASE + (drive.drive_index as u32) * CDS_ENTRY_SIZE;
            let drive_letter = b'A' + drive.drive_index;

            // Path: "X:\"
            mem.write_byte(cds_addr + CDS_OFF_PATH, drive_letter);
            mem.write_byte(cds_addr + CDS_OFF_PATH + 1, b':');
            mem.write_byte(cds_addr + CDS_OFF_PATH + 2, b'\\');

            // Flags.
            let flags = if drive.is_virtual {
                CDS_FLAG_NETWORK
            } else {
                CDS_FLAG_PHYSICAL
            };
            mem.write_word(cds_addr + CDS_OFF_FLAGS, flags);

            // DPB pointer.
            let dpb_off = DPB_OFFSET + (chain_index as u16) * (DPB_ENTRY_SIZE as u16);
            write_far_ptr(mem, cds_addr + CDS_OFF_DPB_PTR, DOS_DATA_SEGMENT, dpb_off);

            // Backslash offset (points past "X:").
            mem.write_word(cds_addr + CDS_OFF_BACKSLASH_OFFSET, 2);
        }

        // Update SYSVARS.
        mem.write_byte(SYSVARS_BASE + SYSVARS_OFF_BLOCK_DEVS, drives.len() as u8);
        mem.write_word(SYSVARS_BASE + SYSVARS_OFF_MAX_SECTOR, max_sector_size);
    }

    /// Writes a single DPB entry with geometry appropriate for the drive type.
    fn write_dpb_for_drive(&self, mem: &mut dyn MemoryAccess, dpb_addr: u32, drive: &DriveInfo) {
        use tables::*;

        mem.write_byte(dpb_addr + DPB_OFF_DRIVE_NUM, drive.drive_index);

        // Determine unit number from DA/UA low nibble.
        let unit_num = drive.da_ua & 0x0F;
        mem.write_byte(dpb_addr + DPB_OFF_UNIT_NUM, unit_num);

        if drive.is_virtual {
            // Virtual Z: drive: minimal geometry.
            mem.write_word(dpb_addr + DPB_OFF_BYTES_PER_SECTOR, 512);
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_MASK, 0); // 1 sector/cluster - 1
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_SHIFT, 0);
            mem.write_word(dpb_addr + DPB_OFF_RESERVED_SECTORS, 1);
            mem.write_byte(dpb_addr + DPB_OFF_NUM_FATS, 1);
            mem.write_word(dpb_addr + DPB_OFF_ROOT_ENTRIES, 16);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_DATA_SECTOR, 3);
            mem.write_word(dpb_addr + DPB_OFF_MAX_CLUSTER, 2);
            mem.write_word(dpb_addr + DPB_OFF_SECTORS_PER_FAT, 1);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_ROOT_SECTOR, 2);
            mem.write_byte(dpb_addr + DPB_OFF_MEDIA_DESC, 0xF8);
            mem.write_byte(dpb_addr + DPB_OFF_ACCESS_FLAG, 0x00);
        } else if drive.da_ua & 0xF0 == 0x70 {
            // 640KB FDD (2DD): 512 bytes/sector.
            mem.write_word(dpb_addr + DPB_OFF_BYTES_PER_SECTOR, 512);
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_MASK, 1); // 2 sectors/cluster - 1
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_SHIFT, 1);
            mem.write_word(dpb_addr + DPB_OFF_RESERVED_SECTORS, 1);
            mem.write_byte(dpb_addr + DPB_OFF_NUM_FATS, 2);
            mem.write_word(dpb_addr + DPB_OFF_ROOT_ENTRIES, 112);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_DATA_SECTOR, 14);
            mem.write_word(dpb_addr + DPB_OFF_MAX_CLUSTER, 1231);
            mem.write_word(dpb_addr + DPB_OFF_SECTORS_PER_FAT, 3);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_ROOT_SECTOR, 7);
            mem.write_byte(dpb_addr + DPB_OFF_MEDIA_DESC, 0xFE);
            mem.write_byte(dpb_addr + DPB_OFF_ACCESS_FLAG, 0xFF);
        } else if drive.da_ua & 0xF0 == 0x80 {
            // HDD: 512 bytes/sector, default geometry.
            mem.write_word(dpb_addr + DPB_OFF_BYTES_PER_SECTOR, 512);
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_MASK, 3); // 4 sectors/cluster - 1
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_SHIFT, 2);
            mem.write_word(dpb_addr + DPB_OFF_RESERVED_SECTORS, 1);
            mem.write_byte(dpb_addr + DPB_OFF_NUM_FATS, 2);
            mem.write_word(dpb_addr + DPB_OFF_ROOT_ENTRIES, 512);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_DATA_SECTOR, 69);
            mem.write_word(dpb_addr + DPB_OFF_MAX_CLUSTER, 4080);
            mem.write_word(dpb_addr + DPB_OFF_SECTORS_PER_FAT, 8);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_ROOT_SECTOR, 17);
            mem.write_byte(dpb_addr + DPB_OFF_MEDIA_DESC, 0xF8);
            mem.write_byte(dpb_addr + DPB_OFF_ACCESS_FLAG, 0xFF);
        } else if drive.da_ua & 0xF0 == 0x90 {
            // 1MB FDD (2HD): 1024 bytes/sector, PC-98 standard.
            mem.write_word(dpb_addr + DPB_OFF_BYTES_PER_SECTOR, 1024);
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_MASK, 0); // 1 sector/cluster - 1
            mem.write_byte(dpb_addr + DPB_OFF_CLUSTER_SHIFT, 0);
            mem.write_word(dpb_addr + DPB_OFF_RESERVED_SECTORS, 1);
            mem.write_byte(dpb_addr + DPB_OFF_NUM_FATS, 2);
            mem.write_word(dpb_addr + DPB_OFF_ROOT_ENTRIES, 192);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_DATA_SECTOR, 11);
            mem.write_word(dpb_addr + DPB_OFF_MAX_CLUSTER, 1223);
            mem.write_word(dpb_addr + DPB_OFF_SECTORS_PER_FAT, 2);
            mem.write_word(dpb_addr + DPB_OFF_FIRST_ROOT_SECTOR, 5);
            mem.write_byte(dpb_addr + DPB_OFF_MEDIA_DESC, 0xFE);
            mem.write_byte(dpb_addr + DPB_OFF_ACCESS_FLAG, 0xFF);
        } else {
            panic!(
                "Unknown device type DA/UA {:#04X} for drive {}",
                drive.da_ua,
                (b'A' + drive.drive_index) as char
            );
        }

        // Device header pointer -> NUL device (placeholder).
        write_far_ptr(
            mem,
            dpb_addr + DPB_OFF_DEVICE_PTR,
            DOS_DATA_SEGMENT,
            DEV_NUL_OFFSET,
        );
    }

    /// Writes a minimal disk buffer header with one empty 512-byte buffer.
    fn write_disk_buffer(&self, mem: &mut dyn MemoryAccess) {
        use tables::*;

        // Buffer header: next = FFFF:FFFF, drive = 0xFF (none), rest = 0
        write_far_ptr(mem, DISK_BUFFER_BASE, 0xFFFF, 0xFFFF);
        mem.write_byte(DISK_BUFFER_BASE + 4, 0xFF); // drive
        // Remaining header bytes and buffer data are already zero from memset.
    }

    /// Creates the MCB chain, environment block, PSP, and COMMAND.COM code stub.
    fn write_command_com_process(&mut self, mem: &mut dyn MemoryAccess) {
        use tables::*;

        memory::write_initial_mcb_chain(mem);
        process::write_environment_block(mem, ENV_SEGMENT);
        process::write_psp(
            mem,
            PSP_SEGMENT,
            PSP_SEGMENT,
            ENV_SEGMENT,
            MEMORY_TOP_SEGMENT,
        );
        process::write_command_com_stub(mem, PSP_SEGMENT);
        self.current_psp = PSP_SEGMENT;
        self.current_drive = self.boot_drive.saturating_sub(1);
        self.dta_segment = PSP_SEGMENT;
        self.dta_offset = 0x0080;
        self.dbcs_table_addr = tables::DBCS_TABLE_ADDR;
    }

    /// Populates the IO.SYS work area at segment 0060h.
    fn write_iosys_work_area(&self, mem: &mut dyn MemoryAccess) {
        use tables::*;

        let base = IOSYS_BASE;

        // MS-DOS product number (0x0100+ range for MS-DOS 5.0+, 0x0102 = NEC MS DOS 6.20)
        mem.write_word(base + IOSYS_OFF_PRODUCT_NUMBER, 0x0102);
        mem.write_byte(base + IOSYS_OFF_INTERNAL_REVISION, 0x00);

        mem.write_byte(base + IOSYS_OFF_EMM_BANK_FLAG, 0x00);
        mem.write_byte(base + IOSYS_OFF_EXT_MEM_128K, 0x00);
        mem.write_byte(base + IOSYS_OFF_FD_DUPLICATE, 0x00);

        // RS-232C default: 9600 baud (1001), 8N1 (11=8bit, 0=no parity, 01=1stop)
        // Bits: 0000_1001_0100_1100 = 0x094C
        mem.write_word(base + IOSYS_OFF_AUX_PROTOCOL, 0x094C);

        // DA/UA mapping table: populated by write_drive_structures().

        // Kanji/graph mode
        mem.write_byte(base + IOSYS_OFF_KANJI_MODE, 0x01); // Shift-JIS kanji mode
        mem.write_byte(base + IOSYS_OFF_GRAPH_CHAR, 0x20);
        mem.write_byte(base + IOSYS_OFF_SHIFT_FN_CHAR, 0x20);

        mem.write_byte(base + IOSYS_OFF_STOP_REENTRY, 0x00);
        mem.write_byte(base + IOSYS_OFF_INTDC_FLAG, 0x00);
        mem.write_byte(base + IOSYS_OFF_SPECIAL_INPUT, 0x00);
        mem.write_byte(base + IOSYS_OFF_PRINTER_ECHO, 0x00);
        mem.write_byte(base + IOSYS_OFF_SOFTKEY_FLAGS, 0x00);

        // Display / cursor state
        mem.write_byte(base + IOSYS_OFF_CURSOR_Y, 0x00);
        mem.write_byte(base + IOSYS_OFF_FNKEY_DISPLAY, 0x01); // show function keys
        mem.write_byte(base + IOSYS_OFF_SCROLL_LOWER, 24); // bottom row
        mem.write_byte(base + IOSYS_OFF_SCREEN_LINES, 0x01); // 25-line mode
        mem.write_byte(base + IOSYS_OFF_CLEAR_ATTR, 0xE1); // white-on-black
        mem.write_byte(base + IOSYS_OFF_KANJI_HI_FLAG, 0x00);
        mem.write_byte(base + IOSYS_OFF_KANJI_HI_BYTE, 0x00);
        mem.write_byte(base + IOSYS_OFF_LINE_WRAP, 0x00); // wrap at column 80
        mem.write_byte(base + IOSYS_OFF_SCROLL_SPEED, 0x00); // normal
        mem.write_byte(base + IOSYS_OFF_CLEAR_CHAR, 0x20); // space
        mem.write_byte(base + IOSYS_OFF_CURSOR_VISIBLE, 0x01); // shown
        mem.write_byte(base + IOSYS_OFF_CURSOR_X, 0x00);
        mem.write_byte(base + IOSYS_OFF_DISPLAY_ATTR, 0xE1); // white-on-black
        mem.write_byte(base + IOSYS_OFF_SCROLL_UPPER, 0x00);
        mem.write_word(base + IOSYS_OFF_SCROLL_WAIT, 0x0001); // normal speed

        // Saved cursor
        mem.write_byte(base + IOSYS_OFF_SAVED_CURSOR_Y, 0x00);
        mem.write_byte(base + IOSYS_OFF_SAVED_CURSOR_X, 0x00);
        mem.write_byte(base + IOSYS_OFF_SAVED_CURSOR_ATTR, 0xE1);

        mem.write_byte(base + IOSYS_OFF_LAST_DRIVE_UNIT, 0x00);
        mem.write_byte(base + IOSYS_OFF_FD_DUPLICATE2, 0x00);
        mem.write_word(base + IOSYS_OFF_EXT_ATTR_DISPLAY, 0x0000);
        mem.write_word(base + IOSYS_OFF_EXT_ATTR_CLEAR, 0x0000);

        mem.write_word(base + IOSYS_OFF_EXT_ATTR_MODE, 0x0000); // PC mode
        mem.write_word(base + IOSYS_OFF_TEXT_MODE, 0x0000); // 25-line gapped

        // DA/UA pointer -> points to the DA/UA table itself
        tables::write_far_ptr(
            mem,
            base + IOSYS_OFF_DAUA_PTR,
            IOSYS_SEGMENT,
            IOSYS_OFF_DAUA_TABLE as u16,
        );
    }
}
