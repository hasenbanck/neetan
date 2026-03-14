//! PC-98 IDE (ATA) hard disk controller.
//!
//! Consolidates the IDE HLE (High-Level Emulation) and LLE (Low-Level
//! Emulation) into a single controller module. The HLE intercepts INT 1Bh
//! via an expansion ROM stub at 0xD8000, while the LLE handles direct
//! hardware register access at ports 0x0640-0x064E and 0x074C-0x074E.

mod hle;
mod lle;

use std::{cell::Cell, path::PathBuf};

use common::error;
pub use lle::{IdeAction, IdePhase};

use crate::disk::{HddGeometry, HddImage};
pub use crate::disk_hle::{buffer_address, drive_index, sector_position, transfer_size};

/// Size of the expansion ROM window mapped at 0xD8000.
pub const ROM_SIZE: usize = 8192;

/// 8192-byte expansion ROM image.
static ROM_IMAGE: &[u8; ROM_SIZE] = include_bytes!("../../../utils/ide/ide.rom");

/// PC-98 IDE (ATA) hard disk controller.
#[derive(Debug)]
pub struct IdeController {
    lle_controller: lle::Controller,
    hle_pending: bool,
    yield_requested: Cell<bool>,
    rom: Option<Box<[u8; ROM_SIZE]>>,
    drives: [Option<HddImage>; 2],
    drive_paths: [Option<PathBuf>; 2],
    drive_dirty: [bool; 2],
    work_area_mapped: bool,
}

impl Default for IdeController {
    fn default() -> Self {
        Self::new()
    }
}

impl IdeController {
    /// Creates a new idle IDE controller.
    pub fn new() -> Self {
        Self {
            lle_controller: lle::Controller::new(),
            hle_pending: false,
            yield_requested: Cell::new(false),
            rom: None,
            drives: [None, None],
            drive_paths: [None, None],
            drive_dirty: [false, false],
            work_area_mapped: false,
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
                            "Failed to rename temp HDD image for IDE drive {drive} to {}: {err}",
                            path.display()
                        );
                        let _ = std::fs::remove_file(&tmp_path);
                    }
                },
                Err(err) => {
                    error!(
                        "Failed to write HDD image for IDE drive {drive} to {}: {err}",
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
        let status = crate::disk_hle::execute_write::<512>(
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

    /// Executes a BIOS sense: returns the IDE media type.
    pub fn execute_sense(&self, drive_idx: usize) -> u8 {
        hle::execute_sense(drive_idx, &self.drives)
    }

    /// Executes a BIOS init: returns the disk equipment word.
    /// Preserves non-IDE bits from `current_equip`.
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

    /// Executes a BIOS mode set (function 0x0E): returns 0x00 if the drive
    /// exists, 0x60 otherwise.
    pub fn execute_mode_set(&self, drive_idx: usize) -> u8 {
        if self.drives[drive_idx].is_some() {
            0x00
        } else {
            0x60
        }
    }

    /// Executes IDE Check Power Mode (function D0h).
    pub fn execute_check_power_mode(&self, drive_idx: usize) -> u8 {
        hle::execute_check_power_mode(drive_idx, &self.drives)
    }

    /// Executes IDE Motor ON (function E0h).
    pub fn execute_motor_on(&self, drive_idx: usize) -> u8 {
        hle::execute_motor_on(drive_idx, &self.drives)
    }

    /// Executes IDE Motor OFF (function F0h).
    pub fn execute_motor_off(&self, drive_idx: usize) -> u8 {
        hle::execute_motor_off(drive_idx, &self.drives)
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

    /// Writes a byte to the trap port (0x07EE).
    /// Any single byte triggers the HLE trap.
    pub fn write_trap_port(&mut self, _value: u8) {
        self.hle_pending = true;
        self.yield_requested.set(true);
    }

    /// Returns true if an IDE HLE trap is pending.
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

    /// Reads the 16-bit data register (port 0x0640).
    pub fn read_data_word(&mut self) -> u16 {
        self.lle_controller.read_data_word(&self.drives)
    }

    /// Writes the 16-bit data register (port 0x0640).
    pub fn write_data_word(&mut self, value: u16) -> IdeAction {
        let action = self.lle_controller.write_data_word(value, &mut self.drives);
        if matches!(action, IdeAction::ScheduleCompletion) {
            let selected = self.lle_controller.selected_drive();
            self.drive_dirty[selected] = true;
        }
        action
    }

    /// Reads the error register (port 0x0642). Clears ERR bit in status.
    pub fn read_error(&mut self) -> u8 {
        self.lle_controller.read_error()
    }

    /// Reads the sector count register (port 0x0644).
    pub fn read_sector_count(&self) -> u8 {
        self.lle_controller.read_sector_count()
    }

    /// Reads the sector number register (port 0x0646).
    pub fn read_sector_number(&self) -> u8 {
        self.lle_controller.read_sector_number()
    }

    /// Reads the cylinder low register (port 0x0648).
    pub fn read_cylinder_low(&self) -> u8 {
        self.lle_controller.read_cylinder_low()
    }

    /// Reads the cylinder high register (port 0x064A).
    pub fn read_cylinder_high(&self) -> u8 {
        self.lle_controller.read_cylinder_high()
    }

    /// Reads the device/head register (port 0x064C).
    pub fn read_device_head(&self) -> u8 {
        self.lle_controller.read_device_head()
    }

    /// Reads the status register (port 0x064E). Clears pending interrupt.
    pub fn read_status(&mut self) -> u8 {
        self.lle_controller.read_status()
    }

    /// Reads the alternate status register (port 0x074C). Does NOT clear interrupt.
    pub fn read_alt_status(&self) -> u8 {
        self.lle_controller.read_alt_status()
    }

    /// Writes the features register (port 0x0642).
    pub fn write_features(&mut self, value: u8) {
        self.lle_controller.write_features(value);
    }

    /// Writes the sector count register (port 0x0644).
    pub fn write_sector_count(&mut self, value: u8) {
        self.lle_controller.write_sector_count(value);
    }

    /// Writes the sector number register (port 0x0646).
    pub fn write_sector_number(&mut self, value: u8) {
        self.lle_controller.write_sector_number(value);
    }

    /// Writes the cylinder low register (port 0x0648).
    pub fn write_cylinder_low(&mut self, value: u8) {
        self.lle_controller.write_cylinder_low(value);
    }

    /// Writes the cylinder high register (port 0x064A).
    pub fn write_cylinder_high(&mut self, value: u8) {
        self.lle_controller.write_cylinder_high(value);
    }

    /// Writes the device/head register (port 0x064C).
    pub fn write_device_head(&mut self, value: u8) {
        self.lle_controller.write_device_head(value);
    }

    /// Writes the command register (port 0x064E).
    pub fn write_command(&mut self, value: u8) -> IdeAction {
        self.lle_controller.write_command(value, &self.drives)
    }

    /// Writes the device control register (port 0x074C).
    pub fn write_device_control(&mut self, value: u8) {
        self.lle_controller.write_device_control(value);
    }

    /// Reads the bank select register.
    /// Clears the interrupt pending flag (bit 7) after reading.
    pub fn read_bank(&mut self, index: usize) -> u8 {
        self.lle_controller.read_bank(index)
    }

    /// Writes the bank select register.
    pub fn write_bank(&mut self, index: usize, value: u8) {
        self.lle_controller.write_bank(index, value);
    }

    /// Reads the IDE presence detection register (port 0x0433).
    pub fn read_presence(&self) -> u8 {
        self.lle_controller.read_presence(&self.drives)
    }

    /// Reads the additional status register (port 0x0435).
    pub fn read_additional_status(&self) -> u8 {
        self.lle_controller.read_additional_status()
    }

    /// Reads the digital input register (port 0x074E).
    pub fn read_digital_input(&self) -> u8 {
        self.lle_controller.read_digital_input()
    }

    /// Called when the scheduled completion event fires.
    /// Returns true if an interrupt should be raised.
    pub fn complete_operation(&mut self) -> bool {
        self.lle_controller.complete_operation()
    }

    /// Reads the work area mapping port (0x1E8E).
    /// Returns 0x81 if DA000-DBFFF is mapped, 0x80 otherwise.
    pub fn read_work_area_port(&self) -> u8 {
        if self.work_area_mapped { 0x81 } else { 0x80 }
    }

    /// Writes the work area mapping port (0x1E8E).
    /// 0x81 maps RAM at DA000-DBFFF, 0x00/0x80 unmaps it.
    pub fn write_work_area_port(&mut self, value: u8) {
        self.work_area_mapped = value == 0x81;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{HddFormat, HddGeometry};

    fn make_test_drive() -> HddImage {
        let geometry = HddGeometry {
            cylinders: 20,
            heads: 4,
            sectors_per_track: 17,
            sector_size: 512,
        };
        let data = vec![0u8; geometry.total_bytes() as usize];
        HddImage::from_raw(geometry, HddFormat::Hdi, data)
    }

    #[test]
    fn trap_triggers_on_single_byte() {
        let mut ide = IdeController::new();
        assert!(!ide.hle_pending());
        ide.write_trap_port(0x00);
        assert!(ide.hle_pending());
        assert!(ide.take_yield_requested());
        assert!(!ide.take_yield_requested());
    }

    #[test]
    fn rom_image_has_correct_signature() {
        let mut ide = IdeController::new();
        assert!(!ide.rom_installed());

        ide.insert_drive(0, make_test_drive(), None);

        assert!(ide.rom_installed());
        // Expansion ROM signature at offset 9.
        assert_eq!(ide.read_rom_byte(9), 0x55);
        assert_eq!(ide.read_rom_byte(10), 0xAA);
        // ROM size code: 0x10 (= 16 * 512 = 8192 bytes).
        assert_eq!(ide.read_rom_byte(11), 0x10);
    }

    #[test]
    fn work_area_port_read_write() {
        let mut ide = IdeController::new();
        assert_eq!(ide.read_work_area_port(), 0x80);

        ide.write_work_area_port(0x81);
        assert_eq!(ide.read_work_area_port(), 0x81);

        ide.write_work_area_port(0x80);
        assert_eq!(ide.read_work_area_port(), 0x80);

        ide.write_work_area_port(0x00);
        assert_eq!(ide.read_work_area_port(), 0x80);
    }
}
