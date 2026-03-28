//! µPD765A Floppy Disk Controller for the PC-98.
//!
//! The PC-98 has two FDC interfaces: 1MB (ports 0x90/0x92/0x94) and
//! 640KB (ports 0xC8/0xCA/0xCC). Each is an independent µPD765A.
//!
//! The FDC communicates with the bus via [`FdcAction`] return values
//! from [`Upd765aFdc::write_data`]. The bus is responsible for disk
//! image lookups, DMA transfers, and scheduling interrupts.

use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use common::error;

use crate::floppy::{FloppyImage, d88::D88MediaType};

/// FDC command processing phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdcPhase {
    /// Waiting for a command byte.
    Idle,
    /// Collecting parameter bytes for the current command.
    Command,
    /// Executing a data transfer command (bus handles the transfer).
    Execution,
    /// Returning result bytes to the host.
    Result,
}

/// Actions the bus must take after an FDC write_data call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdcAction {
    /// No bus action needed.
    None,
    /// Schedule a seek/recalibrate interrupt after a delay.
    ScheduleSeekInterrupt,
    /// Start a READ DATA transfer: bus should look up sector and DMA.
    StartReadData,
    /// Start a READ ID: bus should provide sector ID at current rotation.
    StartReadId,
    /// Start a WRITE DATA transfer: bus should DMA from memory and write to disk.
    StartWriteData,
    /// Start a FORMAT TRACK (WRITE ID) transfer: bus should read CHRN from DMA and format.
    StartFormatTrack,
}

/// The active FDC command during execution phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdcCommand {
    /// No active command.
    None,
    /// READ DATA (0x06) or READ DELETED DATA (0x0C).
    ReadData,
    /// READ ID (0x0A).
    ReadId,
    /// WRITE DATA (0x05) or WRITE DELETED DATA (0x09).
    WriteData,
    /// FORMAT TRACK / WRITE ID (0x0D).
    FormatTrack,
}

/// Snapshot of the µPD765A FDC state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upd765aFdcState {
    /// Current command processing phase.
    pub phase: FdcPhase,
    /// Main status register (MSR).
    pub status: u8,
    /// External circuit control register.
    pub control: u8,
    /// Previous control register value (for edge detection).
    pub prev_control: u8,
    /// Current command byte (full byte, including flags).
    pub command_byte: u8,
    /// Current command index (low 5 bits).
    pub command: u8,
    /// Active command type during execution.
    pub active_command: FdcCommand,
    /// Parameter buffer.
    pub params: [u8; 9],
    /// Number of parameter bytes expected for current command.
    pub params_expected: u8,
    /// Number of parameter bytes received so far.
    pub params_received: u8,
    /// Result buffer.
    pub result: [u8; 7],
    /// Number of valid result bytes.
    pub result_count: u8,
    /// Current read index into result buffer.
    pub result_index: u8,
    /// Pending ST0 per drive (set by Recalibrate/Seek, consumed by Sense Interrupt Status).
    pub drive_st0: [u8; 4],
    /// Current cylinder (track) per drive.
    pub drive_cylinder: [u8; 4],
    /// Interrupt pending - set when a command completes and needs to notify the CPU.
    pub interrupt_pending: bool,
    /// MT (Multi-Track) flag from command byte.
    pub mt: bool,
    /// MF (MFM Mode) flag from command byte.
    pub mf: bool,
    /// SK (Skip Deleted) flag from command byte.
    pub sk: bool,
    /// Cylinder from command parameters.
    pub c: u8,
    /// Head from command parameters.
    pub h: u8,
    /// Record (sector number) from command parameters.
    pub r: u8,
    /// Size code from command parameters (sector size = 128 << n).
    pub n: u8,
    /// End of track (last sector number to process).
    pub eot: u8,
    /// Gap length.
    pub gpl: u8,
    /// Data length (used when N=0).
    pub dtl: u8,
    /// Head/drive select byte (params[0] for data commands).
    pub hd_us: u8,
    /// Current rotational sector counter for READ ID.
    pub crcn: u8,
    /// Specify SRT (Step Rate Time).
    pub srt: u8,
    /// Specify HUT (Head Unload Time).
    pub hut: u8,
    /// Specify HLT (Head Load Time).
    pub hlt: u8,
    /// Specify ND (Non-DMA mode).
    pub nd: bool,
    /// Terminal count - set by the bus when DMA TC fires during a data transfer.
    pub tc: bool,
    /// Bitmask of equipped drives (bit per drive 0-3).
    pub drive_equipped: u8,
    /// Bitmask of drives that have a disk inserted (bit per drive 0-3).
    pub drive_has_disk: u8,
    /// Bitmask of drives that have a write-protected disk (bit per drive 0-3).
    pub drive_write_protected: u8,
}

/// µPD765A FDC controller.
pub struct Upd765aFdc {
    /// Embedded state for save/restore.
    pub state: Upd765aFdcState,
}

impl Deref for Upd765aFdc {
    type Target = Upd765aFdcState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for Upd765aFdc {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for Upd765aFdc {
    fn default() -> Self {
        Self::new()
    }
}

/// MSR bit 7: RQM (Request for Master) - host may transfer data.
const MSR_RQM: u8 = 0x80;

/// MSR bit 6: DIO (Data Input/Output) - 1 = FDC->host (result), 0 = host->FDC (command/params).
const MSR_DIO: u8 = 0x40;

/// MSR bit 5: NDM (Non-DMA Mode) - FDC is in non-DMA mode data transfer.
const _MSR_NDM: u8 = 0x20;

/// MSR bit 4: CB (Controller Busy) - command in progress.
const MSR_CB: u8 = 0x10;

/// MSR bits 3-0: D3B-D0B (Drive Busy) - per-drive seek-in-progress flags.
const _MSR_DB: u8 = 0x0F;

/// ST0 bits 7-6: IC (Interrupt Code) - 01 = abnormal termination.
pub const ST0_ABNORMAL_TERMINATION: u8 = 0x40;

/// ST0 bits 7-6: IC (Interrupt Code) - 10 = invalid command.
pub const ST0_INVALID_COMMAND: u8 = 0x80;

/// ST0 bit 5: SE (Seek End) - seek or recalibrate completed.
pub const ST0_SEEK_END: u8 = 0x20;

/// ST0 bit 3: NR (Not Ready) - drive not ready.
pub const ST0_NOT_READY: u8 = 0x08;

/// ST1 bit 0: MA (Missing Address Mark) - address mark not found.
pub const ST1_MISSING_ADDRESS_MARK: u8 = 0x01;

/// ST1 bit 1: NW (Not Writable) - write-protected disk.
pub const ST1_NOT_WRITABLE: u8 = 0x02;

/// ST3 bit 5: RY (Ready) - drive is ready.
const ST3_READY: u8 = 0x20;

/// ST3 bit 4: T0 (Track 0) - head is at track 0.
const ST3_TRACK_0: u8 = 0x10;

/// ST3 bit 6: WP (Write Protected) - disk is write-protected.
const ST3_WRITE_PROTECT: u8 = 0x40;

/// ST3 bit 3: TS (Two Side) - drive is double-sided.
const ST3_TWO_SIDE: u8 = 0x08;

/// Command byte bit 7: MT (Multi-Track) flag.
const CMD_FLAG_MT: u8 = 0x80;

/// Command byte bit 6: MF (MFM Mode) flag.
const CMD_FLAG_MF: u8 = 0x40;

/// Command byte bit 5: SK (Skip Deleted Data) flag.
const CMD_FLAG_SK: u8 = 0x20;

/// Mask for command index (low 5 bits of command byte).
const CMD_INDEX_MASK: u8 = 0x1F;

/// READ DIAGNOSTIC command index.
const CMD_READ_DIAGNOSTIC: u8 = 0x02;

/// SPECIFY command index.
const CMD_SPECIFY: u8 = 0x03;

/// SENSE DRIVE STATUS command index.
const CMD_SENSE_DRIVE_STATUS: u8 = 0x04;

/// WRITE DATA command index.
const CMD_WRITE_DATA: u8 = 0x05;

/// READ DATA command index.
const CMD_READ_DATA: u8 = 0x06;

/// RECALIBRATE command index.
const CMD_RECALIBRATE: u8 = 0x07;

/// SENSE INTERRUPT STATUS command index.
const CMD_SENSE_INTERRUPT_STATUS: u8 = 0x08;

/// WRITE DELETED DATA command index.
const CMD_WRITE_DELETED_DATA: u8 = 0x09;

/// READ ID command index.
const CMD_READ_ID: u8 = 0x0A;

/// READ DELETED DATA command index.
const CMD_READ_DELETED_DATA: u8 = 0x0C;

/// WRITE ID (FORMAT TRACK) command index.
const CMD_WRITE_ID: u8 = 0x0D;

/// SEEK command index.
const CMD_SEEK: u8 = 0x0F;

/// SCAN EQUAL command index.
const CMD_SCAN_EQUAL: u8 = 0x11;

/// SCAN LOW OR EQUAL command index.
const CMD_SCAN_LOW_OR_EQUAL: u8 = 0x19;

/// SCAN HIGH OR EQUAL command index.
const CMD_SCAN_HIGH_OR_EQUAL: u8 = 0x1D;

/// Mask for drive number (US bits 1-0) from HD/US parameter byte.
const HD_US_DRIVE_MASK: u8 = 0x03;

/// Mask for head select (HD bit 2) from HD/US parameter byte.
const HD_US_HEAD_SHIFT: u8 = 2;

/// Control register bit 7: RST (Reset) - triggers FDC reset on rising edge.
/// Ref: undoc98 `io_fdd.txt`
const CTRL_RESET: u8 = 0x80;

/// Control register bit 6: FRY (Forced Ready) - force drive ready signal active.
/// Ref: undoc98 `io_fdd.txt`
const CTRL_FORCED_READY: u8 = 0x40;

/// Default drive equipment bitmask: 2 built-in drives equipped (bits 0-1).
const DEFAULT_DRIVE_EQUIPPED: u8 = 0x03;

/// Parameter count per command index (low 5 bits of command byte).
const CMD_PARAMS: [u8; 32] = [
    0, 0, 8, 2, 1, 8, 8, 1, 0, 8, 1, 0, 8, 5, 0, 2, 0, 8, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 8, 0, 0,
];

impl Upd765aFdc {
    /// Creates a new FDC in idle state with RQM set.
    /// 2 built-in drives (0 and 1) are equipped by default.
    pub fn new() -> Self {
        Self {
            state: Upd765aFdcState {
                phase: FdcPhase::Idle,
                status: MSR_RQM,
                control: 0,
                prev_control: 0,
                command_byte: 0,
                command: 0,
                active_command: FdcCommand::None,
                params: [0; 9],
                params_expected: 0,
                params_received: 0,
                result: [0; 7],
                result_count: 0,
                result_index: 0,
                drive_st0: [0; 4],
                drive_cylinder: [0; 4],
                interrupt_pending: false,
                mt: false,
                mf: false,
                sk: false,
                c: 0,
                h: 0,
                r: 0,
                n: 0,
                eot: 0,
                gpl: 0,
                dtl: 0,
                hd_us: 0,
                crcn: 0,
                srt: 0,
                hut: 0,
                hlt: 0,
                nd: false,
                tc: false,
                drive_equipped: DEFAULT_DRIVE_EQUIPPED,
                drive_has_disk: 0,
                drive_write_protected: 0,
            },
        }
    }

    /// Returns and clears the interrupt pending flag.
    pub fn take_interrupt_pending(&mut self) -> bool {
        std::mem::replace(&mut self.state.interrupt_pending, false)
    }

    /// Reads the main status register (MSR).
    pub fn read_status(&self) -> u8 {
        self.state.status
    }

    /// Reads the data register (FIFO).
    pub fn read_data(&mut self) -> u8 {
        if self.state.phase != FdcPhase::Result {
            return 0xFF;
        }

        let index = self.state.result_index as usize;
        let value = self.state.result[index];
        self.state.result_index += 1;

        if self.state.result_index >= self.state.result_count {
            self.state.phase = FdcPhase::Idle;
            self.state.status = MSR_RQM;
        }

        value
    }

    /// Writes the data register (command/parameter bytes).
    /// Returns an [`FdcAction`] indicating what the bus should do.
    pub fn write_data(&mut self, value: u8) -> FdcAction {
        match self.state.phase {
            FdcPhase::Idle => {
                let cmd_index = (value & CMD_INDEX_MASK) as usize;
                self.state.command_byte = value;
                self.state.command = value & CMD_INDEX_MASK;
                self.state.params_received = 0;
                self.state.params_expected = CMD_PARAMS[cmd_index];

                // Extract flags from high bits.
                self.state.mt = value & CMD_FLAG_MT != 0;
                self.state.mf = value & CMD_FLAG_MF != 0;
                self.state.sk = value & CMD_FLAG_SK != 0;

                if self.state.params_expected == 0 {
                    self.execute_command()
                } else {
                    self.state.phase = FdcPhase::Command;
                    self.state.status = MSR_RQM | MSR_CB;
                    FdcAction::None
                }
            }
            FdcPhase::Command => {
                let index = self.state.params_received as usize;
                self.state.params[index] = value;
                self.state.params_received += 1;

                if self.state.params_received >= self.state.params_expected {
                    self.execute_command()
                } else {
                    FdcAction::None
                }
            }
            FdcPhase::Result | FdcPhase::Execution => FdcAction::None,
        }
    }

    /// Writes the external circuit control register.
    pub fn write_control(&mut self, value: u8) {
        self.state.prev_control = self.state.control;
        self.state.control = value;

        // Rising edge of RST bit triggers reset.
        if value & CTRL_RESET != 0 && self.state.prev_control & CTRL_RESET == 0 {
            self.reset();
        }
    }

    /// Called by the bus after looking up sector data for READ DATA.
    /// `data` is the sector content, `d88_status` is the D88 sector status byte.
    pub fn provide_sector_data(&mut self, data: &[u8], d88_status: u8) {
        // The bus handles DMA transfer. We just need the status for the result phase.
        // d88_status of 0 = normal, non-zero flags error conditions.
        let _ = data;
        let _ = d88_status;
    }

    /// Called by the bus with READ ID result. Sets up result bytes.
    pub fn provide_read_id(&mut self, c: u8, h: u8, r: u8, n: u8) {
        self.state.c = c;
        self.state.h = h;
        self.state.r = r;
        self.state.n = n;
    }

    /// Called by the bus when DMA terminal count fires.
    pub fn signal_terminal_count(&mut self) {
        self.state.tc = true;
    }

    /// Completes a data command successfully, filling the 7-byte result buffer.
    pub fn complete_success(&mut self) {
        let drive = self.state.hd_us & HD_US_DRIVE_MASK;
        let head = (self.state.hd_us >> HD_US_HEAD_SHIFT) & 0x01;
        // ST0: normal termination (IC=00), head, drive.
        self.state.result[0] = (head << HD_US_HEAD_SHIFT) | drive;
        self.state.result[1] = 0x00; // ST1
        self.state.result[2] = 0x00; // ST2
        self.state.result[3] = self.state.c;
        self.state.result[4] = self.state.h;
        self.state.result[5] = self.state.r;
        self.state.result[6] = self.state.n;
        self.state.interrupt_pending = true;
        self.enter_result(7);
    }

    /// Completes a data command with error, filling the 7-byte result buffer.
    /// `st0_extra`, `st1`, `st2` are OR'd into the corresponding status bytes.
    pub fn complete_error(&mut self, st0_extra: u8, st1: u8, st2: u8) {
        let drive = self.state.hd_us & HD_US_DRIVE_MASK;
        let head = (self.state.hd_us >> HD_US_HEAD_SHIFT) & 0x01;
        // ST0: abnormal termination (IC=01) | extra flags | head/drive.
        self.state.result[0] =
            ST0_ABNORMAL_TERMINATION | st0_extra | (head << HD_US_HEAD_SHIFT) | drive;
        self.state.result[1] = st1;
        self.state.result[2] = st2;
        self.state.result[3] = self.state.c;
        self.state.result[4] = self.state.h;
        self.state.result[5] = self.state.r;
        self.state.result[6] = self.state.n;
        self.state.interrupt_pending = true;
        self.enter_result(7);
    }

    /// Advances C/H/R to the next sector for the result phase.
    /// Returns `true` if the command should end (EOT reached without MT continuation).
    pub fn advance_sector(&mut self) -> bool {
        if self.state.r == self.state.eot {
            self.state.r = 1;
            if self.state.mt {
                self.state.h ^= 1;
                if self.state.h == 1 {
                    // Flipped to head 1 - continue reading other side.
                    return false;
                }
                // Flipped back to head 0 - both heads done.
            }
            self.state.c += 1;
            return true;
        }
        self.state.r += 1;
        false
    }

    /// Returns the drive number from the current command parameters.
    pub fn current_drive(&self) -> usize {
        (self.state.hd_us & HD_US_DRIVE_MASK) as usize
    }

    /// Returns the track index for the current command (cylinder*2 + head).
    pub fn current_track_index(&self) -> usize {
        let cylinder = self.state.drive_cylinder[self.current_drive()] as usize;
        let head = ((self.state.hd_us >> HD_US_HEAD_SHIFT) & 0x01) as usize;
        cylinder * 2 + head
    }

    /// Resets the FDC to idle state.
    fn reset(&mut self) {
        self.state.phase = FdcPhase::Idle;
        self.state.status = MSR_RQM;
        self.state.command = 0;
        self.state.command_byte = 0;
        self.state.active_command = FdcCommand::None;
        self.state.params_received = 0;
        self.state.params_expected = 0;
        self.state.result_count = 0;
        self.state.result_index = 0;
        self.state.drive_st0 = [0; 4];
        self.state.interrupt_pending = false;
        // Keep drive_cylinder - track positions survive reset.
    }

    fn execute_command(&mut self) -> FdcAction {
        match self.state.command {
            // Specify: store timing params, no result phase.
            CMD_SPECIFY => {
                self.state.srt = (self.state.params[0] >> 4) & 0x0F;
                self.state.hut = self.state.params[0] & 0x0F;
                self.state.hlt = (self.state.params[1] >> 1) & 0x7F;
                self.state.nd = self.state.params[1] & 0x01 != 0;
                self.state.phase = FdcPhase::Idle;
                self.state.status = MSR_RQM;
                FdcAction::None
            }

            // Sense Drive Status: return ST3.
            CMD_SENSE_DRIVE_STATUS => {
                let drive = (self.state.params[0] & HD_US_DRIVE_MASK) as usize;
                let head = (self.state.params[0] >> HD_US_HEAD_SHIFT) & 0x01;
                let track0 = if self.state.drive_cylinder[drive] == 0 {
                    ST3_TRACK_0
                } else {
                    0
                };
                let equipped = self.state.drive_equipped & (1 << drive) != 0;
                let has_disk = self.state.drive_has_disk & (1 << drive) != 0;
                // Ready: set if drive is equipped and either FRY (forced ready)
                // is set in the control register, or a disk is actually present.
                let ready = if equipped && (self.state.control & CTRL_FORCED_READY != 0 || has_disk)
                {
                    ST3_READY
                } else {
                    0x00
                };
                let two_side = if equipped { ST3_TWO_SIDE } else { 0 };
                let write_protect = if self.state.drive_write_protected & (1 << drive) != 0 {
                    ST3_WRITE_PROTECT
                } else {
                    0
                };
                self.state.result[0] = (head << HD_US_HEAD_SHIFT)
                    | (drive as u8)
                    | track0
                    | ready
                    | two_side
                    | write_protect;
                self.enter_result(1);
                FdcAction::None
            }

            // READ DATA / READ DELETED DATA.
            CMD_READ_DATA | CMD_READ_DELETED_DATA => {
                self.extract_data_params();
                self.state.active_command = FdcCommand::ReadData;
                self.state.tc = false;
                self.state.phase = FdcPhase::Execution;
                // MSR: CB set, RQM cleared during execution.
                self.state.status = MSR_CB;
                FdcAction::StartReadData
            }

            // Recalibrate: seek to track 0.
            CMD_RECALIBRATE => {
                let drive = (self.state.params[0] & HD_US_DRIVE_MASK) as usize;
                self.state.drive_cylinder[drive] = 0;
                // ST0: Seek End | drive number.
                self.state.drive_st0[drive] = ST0_SEEK_END | (drive as u8);
                self.state.interrupt_pending = true;
                self.state.phase = FdcPhase::Idle;
                self.state.status = MSR_RQM;
                FdcAction::ScheduleSeekInterrupt
            }

            // Sense Interrupt Status: return pending ST0 + PCN.
            CMD_SENSE_INTERRUPT_STATUS => {
                if let Some(drive) = self.pending_interrupt_drive() {
                    self.state.result[0] = self.state.drive_st0[drive];
                    self.state.result[1] = self.state.drive_cylinder[drive];
                    self.state.drive_st0[drive] = 0;
                    self.enter_result(2);
                } else {
                    // No pending interrupt - return invalid command status.
                    self.state.result[0] = ST0_INVALID_COMMAND;
                    self.enter_result(1);
                }
                FdcAction::None
            }

            // READ ID.
            CMD_READ_ID => {
                self.state.hd_us = self.state.params[0];
                self.state.active_command = FdcCommand::ReadId;
                self.state.phase = FdcPhase::Execution;
                self.state.status = MSR_CB;
                FdcAction::StartReadId
            }

            // Seek: move to target cylinder.
            CMD_SEEK => {
                let drive = (self.state.params[0] & HD_US_DRIVE_MASK) as usize;
                let target = self.state.params[1];
                self.state.drive_cylinder[drive] = target;
                // ST0: Seek End | drive number.
                self.state.drive_st0[drive] = ST0_SEEK_END | (drive as u8);
                self.state.interrupt_pending = true;
                self.state.phase = FdcPhase::Idle;
                self.state.status = MSR_RQM;
                FdcAction::ScheduleSeekInterrupt
            }

            // WRITE DATA / WRITE DELETED DATA.
            CMD_WRITE_DATA | CMD_WRITE_DELETED_DATA => {
                self.extract_data_params();
                self.state.active_command = FdcCommand::WriteData;
                self.state.tc = false;
                self.state.phase = FdcPhase::Execution;
                self.state.status = MSR_CB;
                FdcAction::StartWriteData
            }

            // FORMAT TRACK (WRITE ID).
            CMD_WRITE_ID => {
                self.state.hd_us = self.state.params[0];
                self.state.n = self.state.params[1];
                self.state.eot = self.state.params[2]; // SC (sector count)
                self.state.gpl = self.state.params[3];
                self.state.dtl = self.state.params[4]; // D (fill byte)
                self.state.active_command = FdcCommand::FormatTrack;
                self.state.tc = false;
                self.state.phase = FdcPhase::Execution;
                self.state.status = MSR_CB;
                FdcAction::StartFormatTrack
            }

            // Remaining data transfer commands - fail with "not ready".
            CMD_READ_DIAGNOSTIC
            | CMD_SCAN_EQUAL
            | CMD_SCAN_LOW_OR_EQUAL
            | CMD_SCAN_HIGH_OR_EQUAL => {
                self.state.hd_us = self.state.params[0];
                self.extract_data_params();
                self.complete_error(ST0_NOT_READY, 0x00, 0x00);
                FdcAction::None
            }

            // Unknown/unimplemented command: return invalid command status.
            _ => {
                self.state.result[0] = ST0_INVALID_COMMAND;
                self.enter_result(1);
                FdcAction::None
            }
        }
    }

    /// Extracts C/H/R/N/EOT/GPL/DTL from data command parameters.
    fn extract_data_params(&mut self) {
        self.state.hd_us = self.state.params[0];
        self.state.c = self.state.params[1];
        self.state.h = self.state.params[2];
        self.state.r = self.state.params[3];
        self.state.n = self.state.params[4];
        self.state.eot = self.state.params[5];
        self.state.gpl = self.state.params[6];
        self.state.dtl = self.state.params[7];
    }

    fn enter_result(&mut self, count: u8) {
        self.state.phase = FdcPhase::Result;
        self.state.result_count = count;
        self.state.result_index = 0;
        self.state.status = MSR_RQM | MSR_DIO | MSR_CB;
    }

    fn pending_interrupt_drive(&self) -> Option<usize> {
        self.state.drive_st0.iter().position(|&st0| st0 != 0)
    }
}

/// FDC 1MB interface IRQ line number (IRQ 11).
const FDC_IRQ_1MB: u8 = 11;

/// FDC 640KB interface IRQ line number (IRQ 10).
const FDC_IRQ_640K: u8 = 10;

/// Default port 0xBE value: PORT EXC = 1 (1MB), FDD EXC = 1 (500 kbps).
const FDC_MEDIA_DEFAULT: u8 = 0x03;

/// PC-98 floppy controller managing both FDC interfaces and up to 4 drives.
///
/// The PC-98 has two independent µPD765A FDCs:
/// - 1MB interface (ports 0x90/0x92/0x94, IRQ 11, DMA ch 2) for 2HD disks
/// - 640KB interface (ports 0xC8/0xCA/0xCC, IRQ 10, DMA ch 3) for 2DD/2D disks
///
/// Port 0xBE controls which interface is active. The controller holds both FDC
/// instances and the shared floppy drive storage (up to 4 drives).
pub struct FloppyController {
    /// 1MB FDC (ports 0x90/0x92/0x94).
    fdc_1mb: Upd765aFdc,
    /// 640KB FDC (ports 0xC8/0xCA/0xCC).
    fdc_640k: Upd765aFdc,
    /// Which FDC (0=1MB, 1=640K) is currently executing a command.
    active_interface: u8,
    /// Floppy disk images (up to 4 drives, shared between both FDCs).
    drives: [Option<FloppyImage>; 4],
    /// File paths for floppy disk images (for write-back).
    drive_paths: [Option<PathBuf>; 4],
    /// Dirty flags per drive (set when a write modifies sector data).
    drive_dirty: [bool; 4],
    /// Dual-mode FDC interface control register (port 0xBE).
    fdc_media: u8,
}

impl Default for FloppyController {
    fn default() -> Self {
        Self::new()
    }
}

impl FloppyController {
    /// Creates a new floppy controller with both FDCs in idle state.
    pub fn new() -> Self {
        Self {
            fdc_1mb: Upd765aFdc::new(),
            fdc_640k: Upd765aFdc::new(),
            active_interface: 0,
            drives: [None, None, None, None],
            drive_paths: [None, None, None, None],
            drive_dirty: [false, false, false, false],
            fdc_media: FDC_MEDIA_DEFAULT,
        }
    }

    /// Inserts a floppy disk image into the specified drive (0-3).
    pub fn insert_drive(&mut self, drive: usize, image: FloppyImage, path: Option<PathBuf>) {
        let mask = 1u8 << drive;
        if image.write_protected {
            self.fdc_1mb.state.drive_write_protected |= mask;
            self.fdc_640k.state.drive_write_protected |= mask;
        } else {
            self.fdc_1mb.state.drive_write_protected &= !mask;
            self.fdc_640k.state.drive_write_protected &= !mask;
        }
        self.drives[drive] = Some(image);
        self.drive_paths[drive] = path;
        self.drive_dirty[drive] = false;
        self.fdc_1mb.state.drive_has_disk |= mask;
        self.fdc_640k.state.drive_has_disk |= mask;
    }

    /// Ejects the floppy disk from the specified drive, flushing if dirty.
    pub fn eject_drive(&mut self, drive: usize) {
        self.flush_drive(drive);
        self.drives[drive] = None;
        self.drive_paths[drive] = None;
        self.drive_dirty[drive] = false;
        let mask = 1u8 << drive;
        self.fdc_1mb.state.drive_has_disk &= !mask;
        self.fdc_640k.state.drive_has_disk &= !mask;
        self.fdc_1mb.state.drive_write_protected &= !mask;
        self.fdc_640k.state.drive_write_protected &= !mask;
    }

    /// Writes the floppy image back to its file if it has been modified.
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
                            "Failed to rename temp floppy image for drive {drive} to {}: {err}",
                            path.display()
                        );
                        let _ = std::fs::remove_file(&tmp_path);
                    }
                },
                Err(err) => {
                    error!(
                        "Failed to write floppy image for drive {drive} to {}: {err}",
                        path.display()
                    );
                }
            }
        }
    }

    /// Flushes all dirty floppy images to disk.
    pub fn flush_all_drives(&mut self) {
        for drive in 0..4 {
            self.flush_drive(drive);
        }
    }

    /// Returns a reference to the disk image in the given drive, if present.
    pub fn drive(&self, index: usize) -> Option<&FloppyImage> {
        self.drives[index].as_ref()
    }

    /// Returns whether the disk in the given drive has been modified.
    pub fn is_drive_dirty(&self, index: usize) -> bool {
        self.drive_dirty[index]
    }

    /// Marks a drive as dirty (modified).
    pub fn mark_dirty(&mut self, drive: usize) {
        self.drive_dirty[drive] = true;
    }

    /// Returns a reference to the 1MB FDC.
    pub fn fdc_1mb(&self) -> &Upd765aFdc {
        &self.fdc_1mb
    }

    /// Returns a mutable reference to the 1MB FDC.
    pub fn fdc_1mb_mut(&mut self) -> &mut Upd765aFdc {
        &mut self.fdc_1mb
    }

    /// Returns a reference to the 640KB FDC.
    pub fn fdc_640k(&self) -> &Upd765aFdc {
        &self.fdc_640k
    }

    /// Returns a mutable reference to the 640KB FDC.
    pub fn fdc_640k_mut(&mut self) -> &mut Upd765aFdc {
        &mut self.fdc_640k
    }

    /// Returns a reference to the FDC for the currently active interface.
    pub fn active_fdc(&self) -> &Upd765aFdc {
        if self.active_interface == 0 {
            &self.fdc_1mb
        } else {
            &self.fdc_640k
        }
    }

    /// Returns a mutable reference to the FDC for the currently active interface.
    pub fn active_fdc_mut(&mut self) -> &mut Upd765aFdc {
        if self.active_interface == 0 {
            &mut self.fdc_1mb
        } else {
            &mut self.fdc_640k
        }
    }

    /// Sets which FDC interface (0=1MB, 1=640K) is active for the current command.
    pub fn set_active_interface(&mut self, interface: u8) {
        self.active_interface = interface;
    }

    /// Returns the IRQ line for the currently active FDC interface.
    pub fn irq_line(&self) -> u8 {
        if self.active_interface == 0 {
            FDC_IRQ_1MB
        } else {
            FDC_IRQ_640K
        }
    }

    /// Returns the DMA channel for the currently active FDC interface.
    pub fn dma_channel(&self) -> usize {
        if self.active_interface == 0 { 2 } else { 3 }
    }

    /// Returns the current port 0xBE register value.
    pub fn fdc_media(&self) -> u8 {
        self.fdc_media
    }

    /// Writes the port 0xBE register.
    pub fn set_fdc_media(&mut self, value: u8) {
        self.fdc_media = value;
    }

    /// Returns the effective port 0xBE value with bits 0-1 adjusted for the
    /// media type of the disk in drive 0. On real hardware, the floppy drive's
    /// density detection mechanism overrides the software-set data rate when
    /// the inserted disk doesn't match. A 2DD disk forces both PORT EXC (bit 0)
    /// and FDD EXC (bit 1) low, routing accesses to the 640KB FDC at 250 kbps.
    pub fn effective_fdc_media(&self) -> u8 {
        let mut value = self.fdc_media;
        if let Some(image) = &self.drives[0]
            && image.media_type != D88MediaType::Disk2HD
        {
            value &= !0x03;
        }
        value
    }

    /// Returns whether PORT EXC is set (1MB interface active).
    pub fn port_exc_is_1mb(&self) -> bool {
        self.effective_fdc_media() & 0x01 != 0
    }

    /// Checks whether the FDC interface data rate and recording density match
    /// the disk in the specified drive.
    pub fn density_matches(&self, drive: usize) -> bool {
        let track_index = self.active_fdc().current_track_index();
        let Some(image) = &self.drives[drive] else {
            return true;
        };
        let Some(sector) = image.sector_at_index(track_index, 0) else {
            return true;
        };

        let fdc_expects_2hd = self.effective_fdc_media() & 0x02 != 0;
        let disk_is_2hd = image.media_type == D88MediaType::Disk2HD;
        if fdc_expects_2hd != disk_is_2hd {
            return false;
        }

        let fdc_mf = self.active_fdc().state.mf;
        let sector_is_mfm = sector.mfm_flag & 0x40 == 0;
        fdc_mf == sector_is_mfm
    }

    /// Returns whether a drive has a disk inserted.
    pub fn has_drive(&self, drive: usize) -> bool {
        self.drives[drive].is_some()
    }

    /// Returns whether the disk in the specified drive is write-protected.
    pub fn is_write_protected(&self, drive: usize) -> bool {
        self.drives[drive]
            .as_ref()
            .is_some_and(|d| d.write_protected)
    }

    /// Returns the size code (N) of the first sector on track 0 of the specified drive.
    pub fn boot_sector_size_code(&self, drive: usize) -> Option<u8> {
        self.drives[drive]
            .as_ref()
            .and_then(|disk| disk.sector_at_index(0, 0))
            .map(|s| s.size_code)
    }

    /// Reads sector data from the specified drive by C/H/R/N near the given track index.
    pub fn read_sector_data(
        &self,
        drive: usize,
        track_index: usize,
        c: u8,
        h: u8,
        r: u8,
        n: u8,
    ) -> Option<&[u8]> {
        self.drives[drive]
            .as_ref()
            .and_then(|disk| disk.find_sector_near_track_index(track_index, c, h, r, n))
            .map(|s| s.data.as_slice())
    }

    /// Writes sector data to the specified drive by C/H/R/N near the given track index.
    /// Returns `true` if the sector was found and written. Marks the drive dirty on success.
    #[allow(clippy::too_many_arguments)]
    pub fn write_sector_data(
        &mut self,
        drive: usize,
        track_index: usize,
        c: u8,
        h: u8,
        r: u8,
        n: u8,
        data: &[u8],
    ) -> bool {
        if let Some(disk) = self.drives[drive].as_mut()
            && let Some(sector) = disk.find_sector_near_track_index_mut(track_index, c, h, r, n)
        {
            let copy_len = data.len().min(sector.data.len());
            sector.data[..copy_len].copy_from_slice(&data[..copy_len]);
            self.drive_dirty[drive] = true;
            true
        } else {
            false
        }
    }

    /// Formats a track on the specified drive. Replaces the track's sectors
    /// with new ones described by `chrn` entries, filled with `fill_byte`.
    pub fn format_track(
        &mut self,
        drive: usize,
        track_index: usize,
        chrn: &[(u8, u8, u8, u8)],
        data_n: u8,
        fill_byte: u8,
    ) {
        if let Some(disk) = self.drives[drive].as_mut() {
            disk.format_track(track_index, chrn, data_n, fill_byte);
            self.drive_dirty[drive] = true;
        }
    }

    /// Returns the sector ID (C, H, R, N) at the given rotational index on a track.
    pub fn read_id_at_index(
        &self,
        drive: usize,
        track_index: usize,
        sector_index: usize,
    ) -> Option<(u8, u8, u8, u8)> {
        self.drives[drive].as_ref().and_then(|disk| {
            disk.sector_at_index(track_index, sector_index)
                .map(|s| (s.cylinder, s.head, s.record, s.size_code))
        })
    }

    /// Returns the number of sectors on a track for the specified drive.
    pub fn sector_count(&self, drive: usize, track_index: usize) -> usize {
        self.drives[drive]
            .as_ref()
            .map(|disk| disk.sector_count(track_index))
            .unwrap_or(0)
    }

    /// Sets all FDC state to match the PC-98 post-ITF boot state.
    pub fn initialize_boot_state(&mut self, pit_clock_hz: u32) {
        self.fdc_1mb.state.status = 0x80;
        self.fdc_1mb.state.control = 0x18;
        self.fdc_1mb.state.prev_control = 0xC8;
        self.fdc_1mb.state.command = 0x07;
        self.fdc_1mb.state.params[0] = 0x03;
        self.fdc_1mb.state.drive_st0 = [0x20, 0x21, 0x22, 0x23];
        if pit_clock_hz == 1_996_800 {
            self.fdc_1mb.state.srt = 12;
            self.fdc_1mb.state.hut = 15;
            self.fdc_1mb.state.hlt = 18;
        } else {
            self.fdc_1mb.state.srt = 11;
            self.fdc_1mb.state.hut = 10;
            self.fdc_1mb.state.hlt = 25;
        }
        self.fdc_1mb.state.tc = true;

        self.fdc_640k.state.status = 0x80;
        self.fdc_640k.state.control = 0x48;
        self.fdc_640k.state.prev_control = 0xC8;
        self.fdc_640k.state.command = 0x07;
        self.fdc_640k.state.params[0] = 0x03;
        self.fdc_640k.state.drive_st0 = [0x20, 0x21, 0x22, 0x23];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let fdc = Upd765aFdc::new();
        assert_eq!(fdc.read_status(), MSR_RQM);
        assert_eq!(fdc.state.phase, FdcPhase::Idle);
    }

    #[test]
    fn specify_stores_params() {
        let mut fdc = Upd765aFdc::new();
        // Specify: command 0x03, params: SRT/HUT=0xCF, HLT/ND=0x02
        let action = fdc.write_data(0x03);
        assert_eq!(action, FdcAction::None);
        let action = fdc.write_data(0xCF);
        assert_eq!(action, FdcAction::None);
        let action = fdc.write_data(0x02);
        assert_eq!(action, FdcAction::None);

        assert_eq!(fdc.state.srt, 0x0C);
        assert_eq!(fdc.state.hut, 0x0F);
        assert_eq!(fdc.state.hlt, 0x01);
        assert!(!fdc.state.nd);
        assert_eq!(fdc.state.phase, FdcPhase::Idle);
    }

    #[test]
    fn recalibrate_returns_schedule_seek() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_cylinder[0] = 10;

        let action = fdc.write_data(0x07); // Recalibrate
        assert_eq!(action, FdcAction::None);
        let action = fdc.write_data(0x00); // Drive 0
        assert_eq!(action, FdcAction::ScheduleSeekInterrupt);

        assert_eq!(fdc.state.drive_cylinder[0], 0);
        assert_eq!(fdc.state.drive_st0[0], 0x20);
        assert_eq!(fdc.state.phase, FdcPhase::Idle);
    }

    #[test]
    fn seek_returns_schedule_seek() {
        let mut fdc = Upd765aFdc::new();
        let action = fdc.write_data(0x0F); // Seek
        assert_eq!(action, FdcAction::None);
        fdc.write_data(0x01); // Drive 1
        let action = fdc.write_data(42); // Track 42
        assert_eq!(action, FdcAction::ScheduleSeekInterrupt);

        assert_eq!(fdc.state.drive_cylinder[1], 42);
        assert_eq!(fdc.state.drive_st0[1], 0x21);
    }

    #[test]
    fn sense_interrupt_after_recalibrate() {
        let mut fdc = Upd765aFdc::new();
        fdc.write_data(0x07);
        fdc.write_data(0x02); // Drive 2

        // Now Sense Interrupt Status.
        let action = fdc.write_data(0x08);
        assert_eq!(action, FdcAction::None);
        assert_eq!(fdc.state.phase, FdcPhase::Result);

        let st0 = fdc.read_data();
        assert_eq!(st0, 0x22); // Seek End | drive 2
        let pcn = fdc.read_data();
        assert_eq!(pcn, 0); // Track 0 after recalibrate
        assert_eq!(fdc.state.phase, FdcPhase::Idle);
    }

    #[test]
    fn read_data_returns_start_read_data() {
        let mut fdc = Upd765aFdc::new();
        // READ DATA: 0x46 = MT=0, MF=1, SK=0, cmd=0x06
        let action = fdc.write_data(0x46);
        assert_eq!(action, FdcAction::None);
        // Params: HD/US, C, H, R, N, EOT, GPL, DTL
        for &byte in &[0x00, 0x00, 0x00, 0x01, 0x03, 0x08, 0x1B, 0xFF] {
            fdc.write_data(byte);
        }
        // Last param should trigger execution.
        assert_eq!(fdc.state.phase, FdcPhase::Execution);
        assert_eq!(fdc.state.active_command, FdcCommand::ReadData);
        assert_eq!(fdc.state.c, 0x00);
        assert_eq!(fdc.state.r, 0x01);
        assert_eq!(fdc.state.n, 0x03);
        assert_eq!(fdc.state.eot, 0x08);
        assert!(fdc.state.mf);
        assert!(!fdc.state.mt);
    }

    #[test]
    fn read_id_returns_start_read_id() {
        let mut fdc = Upd765aFdc::new();
        // READ ID: 0x4A = MF=1, cmd=0x0A
        let action = fdc.write_data(0x4A);
        assert_eq!(action, FdcAction::None);
        let action = fdc.write_data(0x00); // HD/US
        assert_eq!(action, FdcAction::StartReadId);
        assert_eq!(fdc.state.phase, FdcPhase::Execution);
        assert_eq!(fdc.state.active_command, FdcCommand::ReadId);
    }

    #[test]
    fn write_data_returns_start_write_data() {
        let mut fdc = Upd765aFdc::new();
        // WRITE DATA: 0x45 = MT=0, MF=1, SK=0, cmd=0x05
        let action = fdc.write_data(0x45);
        assert_eq!(action, FdcAction::None);
        // Params: HD/US, C, H, R, N, EOT, GPL, DTL
        for &byte in &[0x00, 0x00, 0x00, 0x01, 0x03, 0x08, 0x1B, 0xFF] {
            fdc.write_data(byte);
        }
        assert_eq!(fdc.state.phase, FdcPhase::Execution);
        assert_eq!(fdc.state.active_command, FdcCommand::WriteData);
        assert_eq!(fdc.state.c, 0x00);
        assert_eq!(fdc.state.r, 0x01);
        assert_eq!(fdc.state.n, 0x03);
        assert_eq!(fdc.state.eot, 0x08);
        assert!(fdc.state.mf);
        assert!(!fdc.state.mt);
    }

    #[test]
    fn complete_success_fills_result() {
        let mut fdc = Upd765aFdc::new();
        // Simulate a READ DATA that entered execution.
        fdc.state.phase = FdcPhase::Execution;
        fdc.state.hd_us = 0x00;
        fdc.state.c = 0;
        fdc.state.h = 0;
        fdc.state.r = 1;
        fdc.state.n = 3;

        fdc.complete_success();

        assert_eq!(fdc.state.phase, FdcPhase::Result);
        assert!(fdc.state.interrupt_pending);
        // Read 7 result bytes.
        let st0 = fdc.read_data();
        assert_eq!(st0, 0x00); // Normal termination, head 0, drive 0
        let st1 = fdc.read_data();
        assert_eq!(st1, 0x00);
        let st2 = fdc.read_data();
        assert_eq!(st2, 0x00);
        let c = fdc.read_data();
        assert_eq!(c, 0);
        let h = fdc.read_data();
        assert_eq!(h, 0);
        let r = fdc.read_data();
        assert_eq!(r, 1);
        let n = fdc.read_data();
        assert_eq!(n, 3);
    }

    #[test]
    fn complete_error_sets_abnormal() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.phase = FdcPhase::Execution;
        fdc.state.hd_us = 0x01; // Drive 1
        fdc.state.c = 5;
        fdc.state.h = 0;
        fdc.state.r = 3;
        fdc.state.n = 2;

        fdc.complete_error(0x08, 0x01, 0x00); // NR, MA

        assert_eq!(fdc.state.phase, FdcPhase::Result);
        let st0 = fdc.read_data();
        assert_eq!(st0, 0x49); // 0x40 (IC=01) | 0x08 (NR) | 0x01 (drive 1)
    }

    #[test]
    fn sense_drive_status_equipped() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_equipped = 0x03; // Drives 0 and 1 equipped.
        fdc.state.control = 0x40; // Forced ready.
        fdc.write_data(0x04); // Sense Drive Status
        fdc.write_data(0x00); // Drive 0

        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "Drive 0 should be ready");
        assert_eq!(st3 & 0x10, 0x10, "Drive 0 should be at track 0");
        assert_eq!(st3 & 0x08, 0x08, "Drive 0 should report two-side");
    }

    #[test]
    fn sense_drive_status_equipped_no_disk_not_ready() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_equipped = 0x01; // Drive 0 equipped.
        // drive_has_disk = 0 (no disk), control = 0 (no FRY).
        fdc.write_data(0x04); // Sense Drive Status
        fdc.write_data(0x00); // Drive 0

        let st3 = fdc.read_data();
        assert_eq!(
            st3 & 0x20,
            0x00,
            "equipped but no disk and no FRY should NOT be ready"
        );
        assert_eq!(st3 & 0x10, 0x10, "should be at track 0");
        assert_eq!(st3 & 0x08, 0x08, "should report two-side even without disk");
    }

    #[test]
    fn sense_drive_status_equipped_with_disk_ready() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_equipped = 0x01; // Drive 0 equipped.
        fdc.state.drive_has_disk = 0x01; // Disk inserted in drive 0.
        // control = 0 (no FRY).
        fdc.write_data(0x04); // Sense Drive Status
        fdc.write_data(0x00); // Drive 0

        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "equipped with disk should be ready");
        assert_eq!(st3 & 0x08, 0x08, "should report two-side");
    }

    #[test]
    fn sense_drive_status_forced_ready_no_disk() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_equipped = 0x01; // Drive 0 equipped.
        // drive_has_disk = 0 (no disk).
        fdc.state.control = CTRL_FORCED_READY; // FRY set.
        fdc.write_data(0x04); // Sense Drive Status
        fdc.write_data(0x00); // Drive 0

        let st3 = fdc.read_data();
        assert_eq!(st3 & 0x20, 0x20, "FRY should override missing disk");
        assert_eq!(st3 & 0x08, 0x08, "should report two-side");
    }

    #[test]
    fn sense_drive_status_not_equipped() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_equipped = 0x00; // No drives equipped.
        fdc.write_data(0x04); // Sense Drive Status
        fdc.write_data(0x00); // Drive 0

        let st3 = fdc.read_data();
        assert_eq!(
            st3 & 0x08,
            0x00,
            "unequipped drive should not report two-side"
        );
        assert_eq!(st3 & 0x20, 0x00, "unequipped drive should not be ready");
    }

    #[test]
    fn write_control_reset() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_st0[0] = 0x20;

        // Rising edge of bit 7 triggers reset.
        fdc.write_control(0x80);
        assert_eq!(fdc.state.drive_st0[0], 0);
        assert_eq!(fdc.state.phase, FdcPhase::Idle);
    }

    #[test]
    fn current_track_index() {
        let mut fdc = Upd765aFdc::new();
        fdc.state.drive_cylinder[2] = 10;
        fdc.state.hd_us = 0x06; // head=1, drive=2
        assert_eq!(fdc.current_track_index(), 10 * 2 + 1);
    }
}
