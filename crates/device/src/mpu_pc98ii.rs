//! Roland MPU-PC98II (MPU-401 compatible MIDI interface, C-Bus, default base 0xE0D0).
//!
//! Port 0xE0D0 (R/W): MIDI data register.
//! Port 0xE0D2 (R/W): status (read) / command (write).
//!
//! UART mode is functionally supported. Intelligent mode supports reset, version queries,
//! and a subset of commands.

use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

use common::warn;

const ACK: u8 = 0xFE;
const CMD_RESET: u8 = 0xFF;
const CMD_ENTER_UART: u8 = 0x3F;
const CMD_REQ_VERSION_MAJOR: u8 = 0xAC;
const CMD_REQ_VERSION_MINOR: u8 = 0xAD;

const MPU_PC98II_VERSION_MAJOR: u8 = 0x01;
const MPU_PC98II_VERSION_MINOR: u8 = 0x00;

/// MPU-PC98II operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpuPc98iiMode {
    /// Power-on default. Not functionally emulated.
    Intelligent,
    /// Transparent MIDI passthrough.
    Uart,
}

/// Serializable MPU-PC98II state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpuPc98iiState {
    /// Current operating mode.
    pub mode: MpuPc98iiMode,
    /// Response FIFO: queued bytes waiting to be read from the data port.
    pub response_queue: VecDeque<u8>,
}

/// Roland MPU-PC98II MIDI interface device.
pub struct MpuPc98ii {
    /// Embedded state for save/restore.
    pub state: MpuPc98iiState,
    /// MIDI bytes buffered during the current audio chunk (transient, not serialized).
    midi_buffer: Vec<u8>,
}

impl Deref for MpuPc98ii {
    type Target = MpuPc98iiState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for MpuPc98ii {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for MpuPc98ii {
    fn default() -> Self {
        Self::new()
    }
}

impl MpuPc98ii {
    /// Creates a new MPU-PC98II in intelligent mode (power-on default).
    pub fn new() -> Self {
        Self {
            state: MpuPc98iiState {
                mode: MpuPc98iiMode::Intelligent,
                response_queue: VecDeque::new(),
            },
            midi_buffer: Vec::new(),
        }
    }

    /// Reads the status register (port 0xE0D2).
    ///
    /// Bit 6 (DSR): always 0 - ready to accept writes.
    /// Bit 7 (DRR): 0 if data is available to read, 1 if empty.
    pub fn read_status(&self) -> u8 {
        if !self.response_queue.is_empty() {
            0x00
        } else {
            0x80
        }
    }

    /// Writes the command register (port 0xE0D2).
    pub fn write_command(&mut self, value: u8) {
        match value {
            CMD_RESET => {
                self.mode = MpuPc98iiMode::Intelligent;
                self.response_queue.clear();
                self.response_queue.push_back(ACK);
            }
            CMD_ENTER_UART => {
                self.mode = MpuPc98iiMode::Uart;
                self.response_queue.push_back(ACK);
            }
            CMD_REQ_VERSION_MAJOR => {
                self.response_queue.push_back(ACK);
                self.response_queue.push_back(MPU_PC98II_VERSION_MAJOR);
            }
            CMD_REQ_VERSION_MINOR => {
                self.response_queue.push_back(ACK);
                self.response_queue.push_back(MPU_PC98II_VERSION_MINOR);
            }
            0x86..=0x8F => {
                self.response_queue.push_back(ACK);
            }
            _ => {
                if self.mode == MpuPc98iiMode::Intelligent {
                    warn!("MPU-PC98II: unhandled intelligent-mode command: {value:#04X}");
                }
            }
        }
    }

    /// Reads the data register (port 0xE0D0).
    pub fn read_data(&mut self) -> u8 {
        if let Some(response) = self.response_queue.pop_front() {
            return response;
        }
        if self.mode == MpuPc98iiMode::Uart {
            warn!("MPU-PC98II: UART data read (no device connected)");
        }
        0xFF
    }

    /// Writes the data register (port 0xE0D0).
    pub fn write_data(&mut self, value: u8) {
        match self.mode {
            MpuPc98iiMode::Uart => {
                self.midi_buffer.push(value);
            }
            MpuPc98iiMode::Intelligent => {
                warn!("MPU-PC98II: intelligent-mode data write: {value:#04X}");
            }
        }
    }

    /// Appends all buffered MIDI bytes into `target` and clears the internal buffer.
    pub fn flush_midi_into(&mut self, target: &mut Vec<u8>) {
        target.extend_from_slice(&self.midi_buffer);
        self.midi_buffer.clear();
    }
}
