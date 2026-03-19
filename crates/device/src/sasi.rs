//! PC-9801-27 SASI hard disk controller.
//!
//! Consolidates the SASI HLE (High-Level Emulation) and LLE (Low-Level
//! Emulation) into a single controller module. The HLE intercepts INT 1Bh
//! via an expansion ROM stub at 0xD7000, while the LLE handles direct
//! hardware register access at ports 0x80/0x82.

mod hle;
mod lle;

use std::{cell::Cell, path::PathBuf};

use common::error;
pub use lle::{SasiAction, SasiPhase};

use crate::disk::{HddGeometry, HddImage};
pub use crate::disk_hle::{buffer_address, drive_index, sector_position, transfer_size};

/// Size of the expansion ROM window mapped at 0xD7000.
pub const ROM_SIZE: usize = 4096;

/// 4096-byte expansion ROM image.
static ROM_IMAGE: &[u8; ROM_SIZE] = include_bytes!("../../../utils/sasi/sasi.rom");

/// Offset of drive-0 full-height parameter table inside the expansion ROM.
const PARAM_TABLE_D0_FULL_OFFSET: usize = 0x0200;
/// Offset of drive-0 half-height parameter table inside the expansion ROM.
const PARAM_TABLE_D0_HALF_OFFSET: usize = 0x0220;
/// Offset of drive-1 full-height parameter table inside the expansion ROM.
const PARAM_TABLE_D1_FULL_OFFSET: usize = 0x0240;
/// Offset of drive-1 half-height parameter table inside the expansion ROM.
const PARAM_TABLE_D1_HALF_OFFSET: usize = 0x0260;
/// Parameter table size in bytes.
const PARAM_TABLE_SIZE: usize = 0x20;

/// PC-9801-27 SASI hard disk controller.
#[derive(Debug)]
pub struct SasiController {
    lle_controller: lle::Controller,
    hle_pending: bool,
    yield_requested: Cell<bool>,
    rom: Option<Box<[u8; ROM_SIZE]>>,
    drives: [Option<HddImage>; 2],
    drive_paths: [Option<PathBuf>; 2],
    drive_dirty: [bool; 2],
}

impl Default for SasiController {
    fn default() -> Self {
        Self::new()
    }
}

impl SasiController {
    /// Creates a new idle SASI controller.
    pub fn new() -> Self {
        Self {
            lle_controller: lle::Controller::new(),
            hle_pending: false,
            yield_requested: Cell::new(false),
            rom: None,
            drives: [None, None],
            drive_paths: [None, None],
            drive_dirty: [false, false],
        }
    }

    /// Inserts a hard disk image into the specified drive (0-1).
    /// Installs the expansion ROM on the first insertion.
    pub fn insert_drive(&mut self, drive: usize, image: HddImage, path: Option<PathBuf>) {
        self.drives[drive] = Some(image);
        self.drive_paths[drive] = path;
        self.drive_dirty[drive] = false;
        if self.rom.is_none() {
            self.rom = Some(Box::new(*ROM_IMAGE));
        }
        self.refresh_parameter_tables(drive);
    }

    /// Writes the HDD image back to its file if it has been modified.
    pub fn flush_drive(&mut self, drive: usize) {
        if !self.drive_dirty[drive] {
            return;
        }
        if let (Some(image), Some(path)) = (&self.drives[drive], &self.drive_paths[drive]) {
            let data = image.to_bytes();
            let tmp_path = path.with_extension("tmp");
            match std::fs::write(&tmp_path, &data) {
                Ok(()) => match std::fs::rename(&tmp_path, path) {
                    Ok(()) => {
                        self.drive_dirty[drive] = false;
                    }
                    Err(err) => {
                        error!(
                            "Failed to rename temp HDD image for drive {drive} to {}: {err}",
                            path.display()
                        );
                        let _ = std::fs::remove_file(&tmp_path);
                    }
                },
                Err(err) => {
                    error!(
                        "Failed to write HDD image for drive {drive} to {}: {err}",
                        path.display()
                    );
                }
            }
        }
    }

    /// Flushes all dirty HDD images to disk.
    pub fn flush_all_drives(&mut self) {
        for drive in 0..2 {
            self.flush_drive(drive);
        }
    }

    /// Executes a BIOS read: reads sectors and writes to memory via closure.
    pub fn execute_read(
        &self,
        drive_idx: usize,
        xfer_size: u32,
        sector_pos: u32,
        buf_addr: u32,
        write_byte: impl FnMut(u32, u8),
    ) -> u8 {
        crate::disk_hle::execute_read(
            drive_idx,
            xfer_size,
            sector_pos,
            buf_addr,
            &self.drives,
            write_byte,
        )
    }

    /// Reads the boot sector (LBA 0, 1024 bytes) from the specified drive
    /// into a local buffer and returns it.
    pub fn read_boot_sector(&self, drive_idx: usize) -> Option<Vec<u8>> {
        let geometry = self.drive_geometry(drive_idx)?;
        let pos = crate::disk_hle::sector_position(0x80 | drive_idx as u8, 0, 0, &geometry);
        let mut buf = vec![0u8; 0x0400];
        let mut offset = 0usize;
        let result = self.execute_read(drive_idx, 0x0400, pos, 0, |_addr, byte| {
            buf[offset] = byte;
            offset += 1;
        });
        if result < 0x20 { Some(buf) } else { None }
    }

    /// Executes a BIOS write: reads from memory via closure and writes sectors.
    /// Marks the drive dirty on success.
    pub fn execute_write(
        &mut self,
        drive_idx: usize,
        xfer_size: u32,
        sector_pos: u32,
        buf_addr: u32,
        read_byte: impl Fn(u32) -> u8,
    ) -> u8 {
        let status = crate::disk_hle::execute_write::<256>(
            drive_idx,
            xfer_size,
            sector_pos,
            buf_addr,
            &mut self.drives,
            read_byte,
        );
        if status == 0x00 {
            self.drive_dirty[drive_idx] = true;
        }
        status
    }

    /// Executes a BIOS sense: returns the SASI media type.
    pub fn execute_sense(&self, drive_idx: usize) -> u8 {
        hle::execute_sense(drive_idx, &self.drives)
    }

    /// Executes a BIOS init: returns the disk equipment word.
    /// Preserves non-SASI bits from `current_equip`.
    pub fn execute_init(&self, current_equip: u16) -> u16 {
        crate::disk_hle::execute_init(&self.drives, current_equip)
    }

    /// Executes a BIOS format on a track. Marks the drive dirty on success.
    pub fn execute_format(&mut self, drive_idx: usize, sector_pos: u32) -> u8 {
        let status = crate::disk_hle::execute_format(drive_idx, sector_pos, &mut self.drives);
        if status == 0x00 {
            self.drive_dirty[drive_idx] = true;
        }
        status
    }

    /// Executes a BIOS mode set (function 0x0E): selects drive mode
    /// (half-height / full-height). Returns 0x00 if the drive exists, 0x60 otherwise.
    pub fn execute_mode_set(&self, drive_idx: usize) -> u8 {
        if self.drives[drive_idx].is_some() {
            0x00
        } else {
            0x60
        }
    }

    /// Returns the HDD parameter table pointer `(offset, segment)` selected by
    /// the mode-set call for the specified drive.
    pub fn mode_set_parameter_pointer(
        &self,
        drive_idx: usize,
        function_code: u8,
    ) -> Option<(u16, u16)> {
        if self.drives.get(drive_idx)?.is_none() {
            return None;
        }

        let half_height_mode = function_code & 0x80 != 0;
        let offset = match (drive_idx, half_height_mode) {
            (0, false) => PARAM_TABLE_D0_FULL_OFFSET,
            (0, true) => PARAM_TABLE_D0_HALF_OFFSET,
            (1, false) => PARAM_TABLE_D1_FULL_OFFSET,
            (1, true) => PARAM_TABLE_D1_HALF_OFFSET,
            _ => return None,
        };
        Some((offset as u16, 0xD700))
    }

    /// Returns true if the expansion ROM is installed.
    pub fn rom_installed(&self) -> bool {
        self.rom.is_some()
    }

    /// Returns the geometry of the selected drive, if present.
    pub fn drive_geometry(&self, drive: usize) -> Option<HddGeometry> {
        self.drives.get(drive)?.as_ref().map(|drive| drive.geometry)
    }

    /// Reads a byte from the expansion ROM at the given offset.
    pub fn read_rom_byte(&self, offset: usize) -> u8 {
        self.rom.as_ref().map_or(0xFF, |rom| rom[offset])
    }

    /// Writes a byte to the trap port (0x07EF).
    /// Any single byte triggers the HLE trap.
    pub fn write_trap_port(&mut self, _value: u8) {
        self.hle_pending = true;
        self.yield_requested.set(true);
    }

    /// Returns true if a SASI HLE trap is pending.
    pub fn hle_pending(&self) -> bool {
        self.hle_pending
    }

    /// Clears the HLE pending flag after execution.
    pub fn clear_hle_pending(&mut self) {
        self.hle_pending = false;
    }

    /// Returns and clears the yield-requested flag.
    pub fn take_yield_requested(&self) -> bool {
        self.yield_requested.replace(false)
    }

    /// Reads from port 0x80 (data register).
    pub fn read_data(&mut self) -> u8 {
        self.lle_controller.read_data(&self.drives)
    }

    /// Writes to port 0x80 (data register).
    /// Handles pending sector writes internally.
    pub fn write_data(&mut self, value: u8) -> SasiAction {
        let action = self.lle_controller.write_data(value, &self.drives);
        if let Some((unit, sector, data)) = self.lle_controller.pending_write_data() {
            if let Some(drive) = &mut self.drives[unit as usize] {
                drive.write_sector(sector, data);
            }
            self.drive_dirty[unit as usize] = true;
        }
        if action == SasiAction::FormatTrack {
            self.do_format_track();
            return SasiAction::ScheduleCompletion;
        }
        action
    }

    /// Performs a format track operation on the currently selected drive.
    fn do_format_track(&mut self) {
        let unit = self.lle_controller.current_unit() as usize;
        let sector = self.lle_controller.current_sector();
        if let Some(drive) = &mut self.drives[unit] {
            drive.format_track(sector);
            self.drive_dirty[unit] = true;
        }
    }

    /// Reads from port 0x82 (status register).
    pub fn read_status(&mut self) -> u8 {
        self.lle_controller.read_status(&self.drives)
    }

    /// Writes to port 0x82 (control register).
    pub fn write_control(&mut self, value: u8) -> SasiAction {
        self.lle_controller.write_control(value)
    }

    /// Returns the current controller phase.
    pub fn phase(&self) -> SasiPhase {
        self.lle_controller.phase()
    }

    /// Returns the currently selected drive number.
    pub fn current_unit(&self) -> u8 {
        self.lle_controller.current_unit()
    }

    /// Returns whether DMA transfer is active.
    pub fn dma_ready(&self) -> bool {
        self.lle_controller.dma_ready()
    }

    /// DMA read: transfers one byte from disk to host.
    pub fn dma_read_byte(&mut self) -> (u8, SasiAction) {
        self.lle_controller.dma_read_byte(&self.drives)
    }

    /// DMA write: transfers one byte from host to disk.
    /// Marks the drive dirty when a sector write completes.
    pub fn dma_write_byte(&mut self, value: u8) -> SasiAction {
        let action = self.lle_controller.dma_write_byte(value, &mut self.drives);
        if matches!(action, SasiAction::ScheduleCompletion) {
            self.drive_dirty[self.lle_controller.current_unit() as usize] = true;
        }
        action
    }

    /// Called when the scheduled completion event fires.
    /// Returns true if an interrupt should be raised.
    pub fn complete_operation(&mut self) -> bool {
        self.lle_controller.complete_operation()
    }

    fn refresh_parameter_tables(&mut self, drive: usize) {
        let Some(rom) = self.rom.as_mut() else {
            return;
        };
        let Some(image) = &self.drives[drive] else {
            return;
        };

        let full_offset = match drive {
            0 => PARAM_TABLE_D0_FULL_OFFSET,
            1 => PARAM_TABLE_D1_FULL_OFFSET,
            _ => return,
        };
        let half_offset = full_offset + PARAM_TABLE_SIZE;

        Self::write_parameter_table(rom.as_mut_slice(), full_offset, image);
        Self::write_parameter_table(rom.as_mut_slice(), half_offset, image);
    }

    fn write_parameter_table(rom: &mut [u8], offset: usize, image: &HddImage) {
        if offset + PARAM_TABLE_SIZE > rom.len() {
            return;
        }

        let table = &mut rom[offset..offset + PARAM_TABLE_SIZE];
        table.fill(0x00);

        let geometry = image.geometry;
        let cylinders_minus_one = geometry.cylinders.saturating_sub(1);
        let max_head_address = geometry.heads.saturating_sub(1);
        let legacy_sense = geometry.sasi_legacy_sense_type().unwrap_or(0x0F);
        let new_sense = geometry.sasi_new_sense_type().unwrap_or(0x0F);

        // SASI parameter table fields used by software that inspects BIOS data.
        table[0x03] = max_head_address;
        table[0x04] = (cylinders_minus_one >> 8) as u8;
        table[0x05] = cylinders_minus_one as u8;
        table[0x0D] = max_head_address;
        table[0x0E] = (cylinders_minus_one >> 8) as u8;
        table[0x0F] = cylinders_minus_one as u8;
        table[0x14] = legacy_sense;
        table[0x18] = geometry.heads;
        table[0x19] = 0x00;
        table[0x1C] = new_sense;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{HddFormat, HddGeometry};

    fn make_test_drive() -> HddImage {
        let geometry = HddGeometry {
            cylinders: 153,
            heads: 4,
            sectors_per_track: 33,
            sector_size: 256,
        };
        let data = vec![0u8; geometry.total_bytes() as usize];
        HddImage::from_raw(geometry, HddFormat::Thd, data)
    }

    #[test]
    fn trap_triggers_on_single_byte() {
        let mut sasi = SasiController::new();
        assert!(!sasi.hle_pending());
        sasi.write_trap_port(0x00);
        assert!(sasi.hle_pending());
        assert!(sasi.take_yield_requested());
        assert!(!sasi.take_yield_requested());
    }

    #[test]
    fn rom_image_has_correct_signature() {
        let mut sasi = SasiController::new();
        // ROM not installed until a drive is inserted.
        assert!(!sasi.rom_installed());

        sasi.insert_drive(0, make_test_drive(), None);

        assert!(sasi.rom_installed());
        // Expansion ROM signature at offset 9.
        assert_eq!(sasi.read_rom_byte(9), 0x55);
        assert_eq!(sasi.read_rom_byte(10), 0xAA);
        // ROM size code: 2 (= 2 * 512 = 1024 bytes).
        assert_eq!(sasi.read_rom_byte(11), 0x02);
    }
}
