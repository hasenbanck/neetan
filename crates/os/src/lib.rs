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
mod tables;

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

/// The NEETAN OS HLE DOS instance.
///
/// Holds all DOS state: memory management, file handles, process info, etc.
/// Created when no bootable media is found, then called via `dispatch()` on
/// each DOS interrupt.
pub struct NeetanOs {
    // Fields will be populated in later implementation steps.
}

impl Default for NeetanOs {
    fn default() -> Self {
        Self::new()
    }
}

impl NeetanOs {
    /// Creates a new NeetanOs instance.
    pub fn new() -> Self {
        Self {}
    }

    /// Performs the DOS boot sequence: writes data structures into emulated RAM,
    /// mounts drives, parses CONFIG.SYS, and creates the COMMAND.COM process.
    pub fn boot(
        &mut self,
        _cpu: &mut dyn CpuAccess,
        _memory: &mut dyn MemoryAccess,
        _disk: &mut dyn DiskIo,
        _console: &mut dyn ConsoleIo,
    ) {
        unimplemented!("NeetanOs::boot()")
    }

    /// Dispatches a DOS/OS interrupt to the appropriate handler.
    ///
    /// `vector`: interrupt number (0x20-0x2F, 0x33, 0xDC).
    /// Returns `true` if the interrupt was handled, `false` if the vector
    /// should fall through to the default IRET behavior.
    pub fn dispatch(
        &mut self,
        vector: u8,
        _cpu: &mut dyn CpuAccess,
        _memory: &mut dyn MemoryAccess,
        _disk: &mut dyn DiskIo,
        _console: &mut dyn ConsoleIo,
    ) -> bool {
        match vector {
            0x20 => false,
            0x21 => false,
            0x22 => false,
            0x23 => false,
            0x24 => false,
            0x25 => false,
            0x26 => false,
            0x27 => false,
            0x28 => false,
            0x29 => false,
            0x2A => false,
            0x2F => false,
            0x33 => false,
            0xDC => false,
            _ => false,
        }
    }
}
