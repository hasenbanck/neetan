//! SASI Low-Level Emulation (LLE).
//!
//! Emulates the PC-9801-27 SASI interface board at the hardware register
//! level. Two I/O ports expose the SASI protocol:
//! - Port 0x80: Data register (command/data read/write)
//! - Port 0x82: Status/control register
//!
//! Uses DMA channel 0 for data transfers and IRQ 9 (slave PIC IRQ 1,
//! INT 0x11) for completion interrupts.
//!
//! Software that talks directly to the SASI hardware ports (bypassing the
//! BIOS) uses this path. The LLE controller implements the full SASI command
//! protocol as a state machine: Free -> Command -> Read/Write -> Status ->
//! Message -> Free.

use crate::disk::HddImage;

/// SASI controller phase (state machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SasiPhase {
    /// Bus idle, waiting for device selection.
    Free,
    /// Receiving 6-byte command.
    Command,
    /// Processing vendor command 0xC2 (accepts 10 bytes then completes).
    VendorC2,
    /// Returning 4-byte sense data.
    Sense,
    /// Transferring sector data from disk to host (DMA read).
    Read,
    /// Transferring sector data from host to disk (DMA write).
    Write,
    /// Returning status byte.
    Status,
    /// Returning message byte (final phase before returning to Free).
    Message,
}

/// Actions the bus must perform after a SASI controller method call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SasiAction {
    /// No action needed.
    None,
    /// Schedule a completion event (status phase + optional interrupt) after
    /// a delay. The bus should schedule `EventKind::SasiExecution`.
    ScheduleCompletion,
    /// DMA is now ready for transfer. The bus should check the DMA channel.
    DmaReady,
    /// Format track: the SasiController wrapper should write 0xE5 to the
    /// current sector's track, then schedule completion.
    FormatTrack,
}

/// Output Control Register bit masks (port 0x82 write).
const OCR_INTE: u8 = 0x01;
const OCR_DMAE: u8 = 0x02;
const OCR_RST: u8 = 0x08;
const OCR_NRDSW: u8 = 0x40;

/// Input Status Register bit masks (port 0x82 read, NRDSW=1).
const ISR_INT: u8 = 0x01;
const ISR_IXO: u8 = 0x04;
const ISR_CXD: u8 = 0x08;
const ISR_MSG: u8 = 0x10;
const ISR_BSY: u8 = 0x20;
const ISR_REQ: u8 = 0x80;

/// SASI hard disk controller state.
#[derive(Debug)]
pub(super) struct Controller {
    phase: SasiPhase,
    command: [u8; 6],
    command_position: u8,
    unit: u8,
    sector: u32,
    blocks_remaining: u8,
    data_buffer: [u8; 256],
    data_position: usize,
    data_size: usize,
    sense_data: [u8; 4],
    sense_position: u8,
    vendor_c2_position: u8,
    status: u8,
    error_code: u8,
    output_control: u8,
    interrupt_pending: u8,
    /// Saved (unit, sector) for PIO writes that need flushing.
    pending_pio_write: Option<(u8, u32)>,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    /// Creates a new SASI controller in the idle state.
    pub(super) fn new() -> Self {
        Self {
            phase: SasiPhase::Free,
            command: [0; 6],
            command_position: 0,
            unit: 0,
            sector: 0,
            blocks_remaining: 0,
            data_buffer: [0; 256],
            data_position: 0,
            data_size: 0,
            sense_data: [0; 4],
            sense_position: 0,
            vendor_c2_position: 0,
            status: 0,
            error_code: 0,
            output_control: 0,
            interrupt_pending: 0,
            pending_pio_write: None,
        }
    }

    /// Returns the current phase.
    pub(super) fn phase(&self) -> SasiPhase {
        self.phase
    }

    /// Returns true if interrupts are enabled (INTE bit set).
    pub(super) fn interrupts_enabled(&self) -> bool {
        self.output_control & OCR_INTE != 0
    }

    /// Returns true if DMA is enabled (DMAE bit set).
    pub(super) fn dma_enabled(&self) -> bool {
        self.output_control & OCR_DMAE != 0
    }

    /// Returns whether DMA should be active (DMAE set and in read/write phase).
    pub(super) fn dma_ready(&self) -> bool {
        self.dma_enabled() && (self.phase == SasiPhase::Read || self.phase == SasiPhase::Write)
    }

    /// Returns the currently selected unit (drive) number (0 or 1).
    pub(super) fn current_unit(&self) -> u8 {
        self.unit
    }

    /// Returns the current sector address.
    pub(super) fn current_sector(&self) -> u32 {
        self.sector
    }

    /// Handles a write to port 0x80 (data register).
    pub(super) fn write_data(&mut self, value: u8, drives: &[Option<HddImage>; 2]) -> SasiAction {
        match self.phase {
            SasiPhase::Free => {
                if value == 1 {
                    self.phase = SasiPhase::Command;
                    self.command_position = 0;
                }
                SasiAction::None
            }
            SasiPhase::Command => {
                self.command[self.command_position as usize] = value;
                self.command_position += 1;
                if self.command_position >= 6 {
                    self.execute_command(drives)
                } else {
                    SasiAction::None
                }
            }
            SasiPhase::VendorC2 => {
                self.vendor_c2_position += 1;
                if self.vendor_c2_position >= 10 {
                    self.set_completion(0x00);
                    SasiAction::ScheduleCompletion
                } else {
                    SasiAction::None
                }
            }
            SasiPhase::Write => {
                self.data_buffer[self.data_position] = value;
                self.data_position += 1;
                if self.data_position >= self.data_size {
                    self.handle_write_complete(drives)
                } else {
                    SasiAction::None
                }
            }
            _ => SasiAction::None,
        }
    }

    /// Handles a read from port 0x80 (data register).
    pub(super) fn read_data(&mut self, drives: &[Option<HddImage>; 2]) -> u8 {
        match self.phase {
            SasiPhase::Read => self.read_data_byte(drives),
            SasiPhase::Status => {
                let ret = if self.error_code == 0 {
                    self.status
                } else {
                    0x02
                };
                self.phase = SasiPhase::Message;
                ret
            }
            SasiPhase::Message => {
                self.phase = SasiPhase::Free;
                0
            }
            SasiPhase::Sense => {
                let ret = self.sense_data[self.sense_position as usize];
                self.sense_position += 1;
                if self.sense_position >= 4 {
                    self.set_completion(0x00);
                    self.phase = SasiPhase::Status;
                    self.interrupt_pending = ISR_INT;
                }
                ret
            }
            _ => 0,
        }
    }

    /// Handles a write to port 0x82 (output control register).
    pub(super) fn write_control(&mut self, value: u8) -> SasiAction {
        let old = self.output_control;
        self.output_control = value;

        // RST falling edge (1->0) resets the controller.
        if (old & OCR_RST) != 0 && (value & OCR_RST) == 0 {
            self.phase = SasiPhase::Free;
        }

        if self.dma_ready() {
            SasiAction::DmaReady
        } else {
            SasiAction::None
        }
    }

    /// Handles a read from port 0x82 (input status register).
    pub(super) fn read_status(&mut self, drives: &[Option<HddImage>; 2]) -> u8 {
        if self.output_control & OCR_NRDSW != 0 {
            self.read_bus_signals()
        } else {
            self.read_capacity_indicators(drives)
        }
    }

    /// Called by the bus when the scheduled completion event fires.
    /// Transitions to Status phase and optionally raises an interrupt.
    /// Returns true if an interrupt should be raised.
    pub(super) fn complete_operation(&mut self) -> bool {
        self.phase = SasiPhase::Status;
        self.interrupt_pending = ISR_INT;
        self.interrupts_enabled()
    }

    /// Reads one byte from the sector buffer during DMA read.
    /// Called by the DMA controller for each byte transfer.
    pub(super) fn dma_read_byte(&mut self, drives: &[Option<HddImage>; 2]) -> (u8, SasiAction) {
        if self.phase != SasiPhase::Read {
            return (0, SasiAction::None);
        }
        let byte = self.read_data_byte(drives);
        let action = if self.phase != SasiPhase::Read {
            // Phase changed - either completed or errored, schedule completion.
            SasiAction::ScheduleCompletion
        } else {
            SasiAction::None
        };
        (byte, action)
    }

    /// Writes one byte to the sector buffer during DMA write.
    /// Called by the DMA controller for each byte transfer.
    pub(super) fn dma_write_byte(
        &mut self,
        value: u8,
        drives: &mut [Option<HddImage>; 2],
    ) -> SasiAction {
        if self.phase != SasiPhase::Write {
            return SasiAction::None;
        }
        self.data_buffer[self.data_position] = value;
        self.data_position += 1;
        if self.data_position >= self.data_size {
            self.handle_write_complete_mut(drives)
        } else {
            SasiAction::None
        }
    }

    fn execute_command(&mut self, drives: &[Option<HddImage>; 2]) -> SasiAction {
        self.unit = (self.command[1] >> 5) & 1;
        let drive_present = drives[self.unit as usize].is_some();

        match self.command[0] {
            0x00 => {
                // Test Drive Ready
                if drive_present {
                    self.status = 0x00;
                    self.set_completion(0x00);
                } else {
                    self.status = 0x02;
                    self.set_completion(0x7F);
                }
                SasiAction::ScheduleCompletion
            }
            0x01 => {
                // Recalibrate
                if drive_present {
                    self.sector = 0;
                    self.status = 0x00;
                    self.set_completion(0x00);
                } else {
                    self.status = 0x02;
                    self.set_completion(0x7F);
                }
                SasiAction::ScheduleCompletion
            }
            0x03 => {
                // Request Sense
                self.phase = SasiPhase::Sense;
                self.sense_position = 0;
                self.sense_data[0] = self.error_code;
                self.sense_data[1] = (self.unit << 5) | ((self.sector >> 16) as u8 & 0x1F);
                self.sense_data[2] = (self.sector >> 8) as u8;
                self.sense_data[3] = self.sector as u8;
                self.error_code = 0x00;
                self.status = 0x00;
                SasiAction::None
            }
            0x04 => {
                // Format Drive
                self.sector = 0;
                self.status = 0;
                self.set_completion(0x0F);
                SasiAction::ScheduleCompletion
            }
            0x06 => {
                // Format Track
                self.parse_sector_address();
                self.status = 0;
                if let Some(drive) = &drives[self.unit as usize]
                    && self.sector < drive.geometry.total_sectors()
                {
                    self.set_completion(0x00);
                    return SasiAction::FormatTrack;
                }
                self.set_completion(0x0F);
                SasiAction::ScheduleCompletion
            }
            0x08 => {
                // Read Data
                self.parse_sector_address();
                self.blocks_remaining = self.command[4];
                self.status = 0;
                if self.blocks_remaining != 0 && self.seek_read(drives) {
                    self.phase = SasiPhase::Read;
                    if self.dma_ready() {
                        SasiAction::DmaReady
                    } else {
                        SasiAction::None
                    }
                } else {
                    self.set_completion(0x0F);
                    SasiAction::ScheduleCompletion
                }
            }
            0x0A => {
                // Write Data
                self.parse_sector_address();
                self.blocks_remaining = self.command[4];
                self.status = 0;
                if self.blocks_remaining != 0 && self.seek_read(drives) {
                    self.phase = SasiPhase::Write;
                    if self.dma_ready() {
                        SasiAction::DmaReady
                    } else {
                        SasiAction::None
                    }
                } else {
                    self.set_completion(0x0F);
                    SasiAction::ScheduleCompletion
                }
            }
            0x0B => {
                // Seek
                self.parse_sector_address();
                self.blocks_remaining = self.command[4];
                self.status = 0x00;
                self.set_completion(0x00);
                SasiAction::ScheduleCompletion
            }
            0xC2 => {
                // Vendor-specific
                self.phase = SasiPhase::VendorC2;
                self.vendor_c2_position = 0;
                self.status = 0x00;
                SasiAction::None
            }
            _ => {
                self.set_completion(0x00);
                SasiAction::ScheduleCompletion
            }
        }
    }

    fn parse_sector_address(&mut self) {
        self.sector = ((self.command[1] & 0x1F) as u32) << 16
            | (self.command[2] as u32) << 8
            | self.command[3] as u32;
    }

    fn set_completion(&mut self, error_code: u8) {
        self.error_code = error_code;
    }

    fn seek_read(&mut self, drives: &[Option<HddImage>; 2]) -> bool {
        self.data_position = 0;
        self.data_size = 0;

        let Some(drive) = &drives[self.unit as usize] else {
            return false;
        };
        if drive.geometry.sector_size != 256 {
            return false;
        }
        let Some(sector_data) = drive.read_sector(self.sector) else {
            return false;
        };
        self.data_buffer[..256].copy_from_slice(sector_data);
        self.data_size = 256;
        true
    }

    fn read_data_byte(&mut self, drives: &[Option<HddImage>; 2]) -> u8 {
        if self.phase != SasiPhase::Read {
            return 0;
        }
        let ret = self.data_buffer[self.data_position];
        self.data_position += 1;
        if self.data_position >= self.data_size {
            self.blocks_remaining -= 1;
            if self.blocks_remaining == 0 {
                self.set_completion(0x00);
                self.phase = SasiPhase::Status;
                self.interrupt_pending = ISR_INT;
            } else {
                self.sector += 1;
                if !self.seek_read(drives) {
                    self.set_completion(0x0F);
                    self.phase = SasiPhase::Status;
                    self.interrupt_pending = ISR_INT;
                }
            }
        }
        ret
    }

    fn handle_write_complete(&mut self, drives: &[Option<HddImage>; 2]) -> SasiAction {
        let drive_ok = drives[self.unit as usize].is_some();
        if !drive_ok {
            self.set_completion(0x0F);
            return SasiAction::ScheduleCompletion;
        }
        // Save the current unit/sector so the wrapper can flush
        // the buffer to disk via pending_write_data().
        self.pending_pio_write = Some((self.unit, self.sector));
        self.blocks_remaining -= 1;
        if self.blocks_remaining == 0 {
            self.set_completion(0x00);
            SasiAction::ScheduleCompletion
        } else {
            self.sector += 1;
            self.data_position = 0;
            SasiAction::None
        }
    }

    fn handle_write_complete_mut(&mut self, drives: &mut [Option<HddImage>; 2]) -> SasiAction {
        let unit = self.unit as usize;
        let Some(drive) = &mut drives[unit] else {
            self.set_completion(0x0F);
            return SasiAction::ScheduleCompletion;
        };

        if !drive.write_sector(self.sector, &self.data_buffer[..self.data_size]) {
            self.set_completion(0x0F);
            return SasiAction::ScheduleCompletion;
        }

        self.blocks_remaining -= 1;
        if self.blocks_remaining == 0 {
            self.set_completion(0x00);
            SasiAction::ScheduleCompletion
        } else {
            self.sector += 1;
            self.data_position = 0;
            // For write, we keep data_size at 256 and wait for next sector buffer fill.
            SasiAction::None
        }
    }

    /// Returns the sector data buffer that needs to be written to the HDD image.
    /// Called by the SasiController after a port-0x80-based write completes a sector buffer.
    pub(super) fn pending_write_data(&mut self) -> Option<(u8, u32, &[u8])> {
        let (unit, sector) = self.pending_pio_write.take()?;
        Some((unit, sector, &self.data_buffer[..self.data_size]))
    }

    fn read_bus_signals(&mut self) -> u8 {
        let mut ret = self.interrupt_pending;
        self.interrupt_pending = 0;

        if self.phase != SasiPhase::Free {
            ret |= ISR_BSY | ISR_REQ;
            match self.phase {
                SasiPhase::Command => {
                    ret |= ISR_CXD;
                }
                SasiPhase::Sense | SasiPhase::Read => {
                    ret |= ISR_IXO;
                }
                SasiPhase::Status => {
                    ret |= ISR_CXD | ISR_IXO;
                }
                SasiPhase::Message => {
                    ret |= ISR_MSG | ISR_CXD | ISR_IXO;
                }
                _ => {}
            }
        }
        ret
    }

    fn read_capacity_indicators(&self, drives: &[Option<HddImage>; 2]) -> u8 {
        let mut ret = 0u8;

        // Drive 0 (SASI-1): bits 3-5
        if let Some(drive) = &drives[0] {
            ret |= (drive.geometry.sasi_media_type().unwrap_or(7) & 7) << 3;
        } else {
            ret |= 7 << 3;
        }

        // Drive 1 (SASI-2): bits 0-2
        if let Some(drive) = &drives[1] {
            ret |= drive.geometry.sasi_media_type().unwrap_or(7) & 7;
        } else {
            ret |= 7;
        }

        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{HddFormat, HddGeometry, HddImage};

    fn make_test_drive() -> HddImage {
        // 5 MB SASI: 153 cylinders, 4 heads, 33 sectors, 256 bytes/sector
        let geometry = HddGeometry {
            cylinders: 153,
            heads: 4,
            sectors_per_track: 33,
            sector_size: 256,
        };
        let total = geometry.total_bytes() as usize;
        let mut data = vec![0u8; total];
        // Fill each sector's first two bytes with LBA high/low.
        for lba in 0..geometry.total_sectors() {
            let offset = lba as usize * 256;
            data[offset] = (lba >> 8) as u8;
            data[offset + 1] = lba as u8;
        }
        HddImage::from_raw(geometry, HddFormat::Thd, data)
    }

    fn make_drives(drive0: Option<HddImage>) -> [Option<HddImage>; 2] {
        [drive0, None]
    }

    #[test]
    fn initial_state_is_free() {
        let controller = Controller::new();
        assert_eq!(controller.phase(), SasiPhase::Free);
        assert!(!controller.interrupts_enabled());
        assert!(!controller.dma_enabled());
    }

    #[test]
    fn select_transitions_to_command_phase() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        let action = controller.write_data(1, &drives);
        assert_eq!(action, SasiAction::None);
        assert_eq!(controller.phase(), SasiPhase::Command);
    }

    #[test]
    fn select_with_wrong_value_stays_free() {
        let mut controller = Controller::new();
        let drives = make_drives(None);

        controller.write_data(0, &drives);
        assert_eq!(controller.phase(), SasiPhase::Free);

        controller.write_data(2, &drives);
        assert_eq!(controller.phase(), SasiPhase::Free);
    }

    #[test]
    fn test_drive_ready_with_drive() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        // Select
        controller.write_data(1, &drives);
        // Send Test Drive Ready command: 00 00 00 00 00 00
        for &byte in &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }

        // Should schedule completion with no error.
        controller.complete_operation();
        assert_eq!(controller.phase(), SasiPhase::Status);

        // Read status - should be 0x00 (success).
        let status = controller.read_data(&drives);
        assert_eq!(status, 0x00);
        assert_eq!(controller.phase(), SasiPhase::Message);

        // Read message - returns to Free.
        controller.read_data(&drives);
        assert_eq!(controller.phase(), SasiPhase::Free);
    }

    #[test]
    fn test_drive_ready_without_drive() {
        let mut controller = Controller::new();
        let drives = make_drives(None);

        controller.write_data(1, &drives);
        for &byte in &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }

        controller.complete_operation();
        // Status should be 0x02 (check condition) since no drive.
        let status = controller.read_data(&drives);
        assert_eq!(status, 0x02);
    }

    #[test]
    fn request_sense_returns_error_info() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        // First do a Test Drive Ready (success).
        controller.write_data(1, &drives);
        for &byte in &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }
        controller.complete_operation();
        controller.read_data(&drives); // status
        controller.read_data(&drives); // message

        // Now issue Request Sense.
        controller.write_data(1, &drives);
        for &byte in &[0x03, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }

        assert_eq!(controller.phase(), SasiPhase::Sense);

        // Read 4 sense bytes.
        let s0 = controller.read_data(&drives);
        let s1 = controller.read_data(&drives);
        let s2 = controller.read_data(&drives);
        let _s3 = controller.read_data(&drives);

        // Error code should be 0 (no error from previous success).
        assert_eq!(s0, 0x00);
        // Unit 0 in bits 5-6.
        assert_eq!(s1 & 0x60, 0x00);
        assert_eq!(s2, 0x00);
    }

    #[test]
    fn read_data_command_fills_buffer() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        // Select.
        controller.write_data(1, &drives);
        // Read Data: cmd=0x08, unit 0, sector 0, 1 block.
        for &byte in &[0x08, 0x00, 0x00, 0x00, 0x01, 0x00] {
            controller.write_data(byte, &drives);
        }

        assert_eq!(controller.phase(), SasiPhase::Read);

        // Read 256 bytes.
        let mut sector = vec![0u8; 256];
        for byte in sector.iter_mut() {
            *byte = controller.read_data(&drives);
        }

        // First two bytes should be LBA 0: 0x00, 0x00.
        assert_eq!(sector[0], 0x00);
        assert_eq!(sector[1], 0x00);

        // After reading all bytes, should be in Status phase.
        assert_eq!(controller.phase(), SasiPhase::Status);
    }

    #[test]
    fn read_sector_at_nonzero_lba() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        controller.write_data(1, &drives);
        // Read LBA 0x000042 (66), 1 block.
        for &byte in &[0x08, 0x00, 0x00, 0x42, 0x01, 0x00] {
            controller.write_data(byte, &drives);
        }

        assert_eq!(controller.phase(), SasiPhase::Read);

        let first = controller.read_data(&drives);
        let second = controller.read_data(&drives);
        assert_eq!(first, 0x00);
        assert_eq!(second, 0x42);
    }

    #[test]
    fn read_multiple_blocks() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        controller.write_data(1, &drives);
        // Read 2 blocks starting at LBA 0.
        for &byte in &[0x08, 0x00, 0x00, 0x00, 0x02, 0x00] {
            controller.write_data(byte, &drives);
        }

        // Read first sector.
        for _ in 0..256 {
            controller.read_data(&drives);
        }
        // Should still be in Read phase (second sector).
        assert_eq!(controller.phase(), SasiPhase::Read);

        // Read second sector.
        let first = controller.read_data(&drives);
        let second = controller.read_data(&drives);
        assert_eq!(first, 0x00);
        assert_eq!(second, 0x01); // LBA 1

        for _ in 2..256 {
            controller.read_data(&drives);
        }
        // Now should be in Status.
        assert_eq!(controller.phase(), SasiPhase::Status);
    }

    #[test]
    fn write_data_command() {
        let mut controller = Controller::new();
        let mut drives: [Option<HddImage>; 2] = [Some(make_test_drive()), None];

        controller.write_data(1, &drives);
        // Write 1 block at LBA 5.
        for &byte in &[0x0A, 0x00, 0x00, 0x05, 0x01, 0x00] {
            controller.write_data(byte, &drives);
        }

        assert_eq!(controller.phase(), SasiPhase::Write);

        // Write 256 bytes of 0xAA via DMA path.
        for _ in 0..256 {
            controller.dma_write_byte(0xAA, &mut drives);
        }

        // Verify the write was committed.
        let sector = drives[0].as_ref().unwrap().read_sector(5).unwrap();
        assert!(sector.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn reset_via_control_register() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        // Enter command phase.
        controller.write_data(1, &drives);
        assert_eq!(controller.phase(), SasiPhase::Command);

        // Set RST bit.
        controller.write_control(OCR_RST);
        assert_eq!(controller.phase(), SasiPhase::Command);

        // Clear RST bit (falling edge triggers reset).
        controller.write_control(0);
        assert_eq!(controller.phase(), SasiPhase::Free);
    }

    #[test]
    fn capacity_indicators_with_5mb_drive() {
        let controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        // Read without NRDSW - should return capacity indicators.
        let mut ctrl = controller;
        ctrl.output_control = 0; // NRDSW = 0
        let status = ctrl.read_status(&drives);

        // Drive 0 is 5MB SASI type 0, bits 3-5 = 0.
        assert_eq!((status >> 3) & 7, 0);
        // Drive 1 not present, bits 0-2 = 7.
        assert_eq!(status & 7, 7);
    }

    #[test]
    fn capacity_indicators_no_drives() {
        let controller = Controller::new();
        let drives: [Option<HddImage>; 2] = [None, None];

        let mut ctrl = controller;
        ctrl.output_control = 0;
        let status = ctrl.read_status(&drives);
        assert_eq!(status, 0x3F); // Both drives absent: 7<<3 | 7 = 0x3F.
    }

    #[test]
    fn bus_signals_in_command_phase() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        controller.write_data(1, &drives);
        controller.output_control = OCR_NRDSW;
        let status = controller.read_status(&drives);
        // BSY + REQ + CXD.
        assert_ne!(status & ISR_BSY, 0);
        assert_ne!(status & ISR_REQ, 0);
        assert_ne!(status & ISR_CXD, 0);
    }

    #[test]
    fn recalibrate_resets_sector() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        // First read at LBA 5.
        controller.write_data(1, &drives);
        for &byte in &[0x08, 0x00, 0x00, 0x05, 0x01, 0x00] {
            controller.write_data(byte, &drives);
        }
        for _ in 0..256 {
            controller.read_data(&drives);
        }
        controller.read_data(&drives); // status
        controller.read_data(&drives); // message

        // Recalibrate.
        controller.write_data(1, &drives);
        for &byte in &[0x01, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }

        // Recalibrate should set sector to 0.
        assert_eq!(controller.sector, 0);
    }

    #[test]
    fn vendor_c2_command() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        controller.write_data(1, &drives);
        for &byte in &[0xC2, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }

        assert_eq!(controller.phase(), SasiPhase::VendorC2);

        // Send 10 bytes.
        for i in 0..9 {
            let action = controller.write_data(0x00, &drives);
            assert_eq!(action, SasiAction::None, "byte {i}");
        }
        let action = controller.write_data(0x00, &drives);
        assert_eq!(action, SasiAction::ScheduleCompletion);
    }

    #[test]
    fn interrupt_pending_flag() {
        let mut controller = Controller::new();
        let drives = make_drives(Some(make_test_drive()));

        controller.write_data(1, &drives);
        for &byte in &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00] {
            controller.write_data(byte, &drives);
        }

        // Enable interrupts.
        controller.write_control(OCR_INTE | OCR_NRDSW);

        let should_irq = controller.complete_operation();
        assert!(should_irq);

        // Read status register - interrupt pending should be set.
        let status = controller.read_status(&drives);
        assert_ne!(status & ISR_INT, 0);

        // Reading clears the interrupt pending.
        let status2 = controller.read_status(&drives);
        assert_eq!(status2 & ISR_INT, 0);
    }
}
