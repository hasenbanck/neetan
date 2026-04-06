//! Adapter structs bridging `os` crate traits to emulator internals.
//!
//! The `os` crate defines `CpuAccess`, `MemoryAccess`, `DiskIo`, and
//! `ConsoleIo` traits. These adapters wrap the concrete emulator types
//! (`common::Cpu`, `Pc9801Memory`) to implement those traits.

use common::Cpu;

use crate::memory::Pc9801Memory;

pub(super) struct OsCpuAccess<'a, C: Cpu>(pub &'a mut C);

impl<C: Cpu> os::CpuAccess for OsCpuAccess<'_, C> {
    fn ax(&self) -> u16 {
        self.0.ax()
    }

    fn set_ax(&mut self, value: u16) {
        self.0.set_ax(value);
    }

    fn bx(&self) -> u16 {
        self.0.bx()
    }

    fn set_bx(&mut self, value: u16) {
        self.0.set_bx(value);
    }

    fn cx(&self) -> u16 {
        self.0.cx()
    }

    fn set_cx(&mut self, value: u16) {
        self.0.set_cx(value);
    }

    fn dx(&self) -> u16 {
        self.0.dx()
    }

    fn set_dx(&mut self, value: u16) {
        self.0.set_dx(value);
    }

    fn si(&self) -> u16 {
        self.0.si()
    }

    fn set_si(&mut self, value: u16) {
        self.0.set_si(value);
    }

    fn di(&self) -> u16 {
        self.0.di()
    }

    fn set_di(&mut self, value: u16) {
        self.0.set_di(value);
    }

    fn ds(&self) -> u16 {
        self.0.ds()
    }

    fn set_ds(&mut self, value: u16) {
        self.0.set_ds(value);
    }

    fn es(&self) -> u16 {
        self.0.es()
    }

    fn set_es(&mut self, value: u16) {
        self.0.set_es(value);
    }

    fn ss(&self) -> u16 {
        self.0.ss()
    }

    fn sp(&self) -> u16 {
        self.0.sp()
    }

    fn set_sp(&mut self, value: u16) {
        self.0.set_sp(value);
    }

    fn cs(&self) -> u16 {
        self.0.cs()
    }

    fn set_carry(&mut self, carry: bool) {
        let mut flags = self.0.flags();
        if carry {
            flags |= 0x0001;
        } else {
            flags &= !0x0001;
        }
        self.0.set_flags(flags);
    }
}

pub(super) struct OsMemoryAccess<'a>(pub &'a mut Pc9801Memory);

impl os::MemoryAccess for OsMemoryAccess<'_> {
    fn read_byte(&self, address: u32) -> u8 {
        self.0.read_byte(address)
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        self.0.write_byte(address, value);
    }

    fn read_word(&self, address: u32) -> u16 {
        let lo = self.0.read_byte(address) as u16;
        let hi = self.0.read_byte(address + 1) as u16;
        lo | (hi << 8)
    }

    fn write_word(&mut self, address: u32, value: u16) {
        self.0.write_byte(address, value as u8);
        self.0.write_byte(address + 1, (value >> 8) as u8);
    }

    fn read_block(&self, address: u32, buf: &mut [u8]) {
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = self.0.read_byte(address + i as u32);
        }
    }

    fn write_block(&mut self, address: u32, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.0.write_byte(address + i as u32, byte);
        }
    }
}

pub(super) struct OsDiskIo;

impl os::DiskIo for OsDiskIo {
    fn read_sectors(&mut self, _drive_da: u8, _lba: u32, _count: u32) -> Result<Vec<u8>, u8> {
        unimplemented!("OsDiskIo::read_sectors()")
    }

    fn write_sectors(&mut self, _drive_da: u8, _lba: u32, _data: &[u8]) -> Result<(), u8> {
        unimplemented!("OsDiskIo::write_sectors()")
    }

    fn sector_size(&self, _drive_da: u8) -> Option<u16> {
        unimplemented!("OsDiskIo::sector_size()")
    }

    fn total_sectors(&self, _drive_da: u8) -> Option<u32> {
        unimplemented!("OsDiskIo::total_sectors()")
    }
}

pub(super) struct OsConsoleIo;

impl os::ConsoleIo for OsConsoleIo {
    fn write_char(&mut self, _ch: u8) {
        unimplemented!("OsConsoleIo::write_char()")
    }

    fn write_str(&mut self, _s: &[u8]) {
        unimplemented!("OsConsoleIo::write_str()")
    }

    fn read_char(&mut self) -> u8 {
        unimplemented!("OsConsoleIo::read_char()")
    }

    fn char_available(&self) -> bool {
        unimplemented!("OsConsoleIo::char_available()")
    }

    fn read_key(&mut self) -> (u8, u8) {
        unimplemented!("OsConsoleIo::read_key()")
    }

    fn cursor_position(&self) -> (u8, u8) {
        unimplemented!("OsConsoleIo::cursor_position()")
    }

    fn set_cursor_position(&mut self, _row: u8, _col: u8) {
        unimplemented!("OsConsoleIo::set_cursor_position()")
    }

    fn scroll_up(&mut self) {
        unimplemented!("OsConsoleIo::scroll_up()")
    }

    fn clear_screen(&mut self) {
        unimplemented!("OsConsoleIo::clear_screen()")
    }

    fn screen_size(&self) -> (u8, u8) {
        unimplemented!("OsConsoleIo::screen_size()")
    }
}
