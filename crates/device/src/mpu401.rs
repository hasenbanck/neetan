//! Roland MPU-401 MIDI interface (C-Bus, default base 0xE0D0).
//!
//! Port 0xE0D0 (R/W): MIDI data register.
//! Port 0xE0D2 (R/W): status (read) / command (write).
//!
//! Only UART mode is functionally supported. Intelligent mode is not supported.

use std::ops::{Deref, DerefMut};

use common::warn;

const ACK: u8 = 0xFE;
const CMD_RESET: u8 = 0xFF;
const CMD_ENTER_UART: u8 = 0x3F;

/// MPU-401 operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mpu401Mode {
    /// Power-on default. Not functionally emulated.
    Intelligent,
    /// Transparent MIDI passthrough.
    Uart,
}

/// Serializable MPU-401 state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mpu401State {
    /// Current operating mode.
    pub mode: Mpu401Mode,
    /// Pending response byte (ACK) waiting to be read from the data port.
    pub pending_response: Option<u8>,
}

/// Roland MPU-401 MIDI interface device.
pub struct Mpu401 {
    /// Embedded state for save/restore.
    pub state: Mpu401State,
}

impl Deref for Mpu401 {
    type Target = Mpu401State;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for Mpu401 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for Mpu401 {
    fn default() -> Self {
        Self::new()
    }
}

impl Mpu401 {
    /// Creates a new MPU-401 in intelligent mode (power-on default).
    pub fn new() -> Self {
        Self {
            state: Mpu401State {
                mode: Mpu401Mode::Intelligent,
                pending_response: None,
            },
        }
    }

    /// Reads the status register (port 0xE0D2).
    ///
    /// Bit 6 (DSR): always 0 - ready to accept writes.
    /// Bit 7 (DRR): 0 if data is available to read, 1 if empty.
    pub fn read_status(&self) -> u8 {
        if self.pending_response.is_some() {
            0x00
        } else {
            0x80
        }
    }

    /// Writes the command register (port 0xE0D2).
    pub fn write_command(&mut self, value: u8) {
        match value {
            CMD_RESET => {
                self.mode = Mpu401Mode::Intelligent;
                self.pending_response = Some(ACK);
            }
            CMD_ENTER_UART => {
                self.mode = Mpu401Mode::Uart;
                self.pending_response = Some(ACK);
            }
            _ => {
                if self.mode == Mpu401Mode::Intelligent {
                    warn!("MPU-401: unhandled intelligent-mode command: {value:#04X}");
                }
            }
        }
    }

    /// Reads the data register (port 0xE0D0).
    pub fn read_data(&mut self) -> u8 {
        if let Some(response) = self.pending_response.take() {
            return response;
        }
        if self.mode == Mpu401Mode::Uart {
            warn!("MPU-401: UART data read (no device connected)");
        }
        0xFF
    }

    /// Writes the data register (port 0xE0D0).
    pub fn write_data(&mut self, value: u8) {
        match self.mode {
            Mpu401Mode::Uart => {
                warn!("MPU-401: UART data write: {value:#04X}");
            }
            Mpu401Mode::Intelligent => {
                warn!("MPU-401: intelligent-mode data write: {value:#04X}");
            }
        }
    }
}
