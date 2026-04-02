//! Roland MPU-PC98II (MPU-401 compatible MIDI interface, C-Bus, default base 0xE0D0).
//!
//! Port 0xE0D0 (R/W): MIDI data register.
//! Port 0xE0D2 (R/W): status (read) / command (write).
//!
//! UART mode is functionally supported. Intelligent mode implements the WSD
//! (Want to Send Data) state machine for forwarding MIDI messages.

use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

const ACK: u8 = 0xFE;
const CMD_RESET: u8 = 0xFF;
const CMD_ENTER_UART: u8 = 0x3F;

const VERSION_MAJOR: u8 = 0x01;
const VERSION_MINOR: u8 = 0x00;
const DEFAULT_TEMPO: u8 = 100;

/// MIDI short message length indexed by `status >> 4`.
const MIDI_MESSAGE_LENGTH: [u8; 16] = [
    0, 0, 0, 0, 0, 0, 0, 0, // 0x00-0x7F: not status bytes
    3, 3, 3, 3, 2, 2, 3, 1, // 0x80-0xFF: NoteOff/On/AT/CC/PC/CP/PB/System
];

/// MPU-PC98II operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpuPc98iiMode {
    /// Power-on default. WSD state machine routes MIDI data.
    Intelligent,
    /// Transparent MIDI passthrough.
    Uart,
}

/// Intelligent-mode command phase.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandPhase {
    /// No active command expecting data port writes.
    Idle,
    /// WSD (0xD0-0xD7): first data byte determines status vs running status.
    ShortInit,
    /// WSD: collecting remaining data bytes of a short MIDI message.
    ShortCollect,
    /// WSD System (0xDF): collecting a system exclusive or system common message.
    Long,
    /// Follow-byte command (0xE0-0xEF subset): consume one parameter byte.
    FollowByte,
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
    /// Current intelligent-mode command phase.
    command_phase: CommandPhase,
    /// Running MIDI status for WSD short messages.
    running_status: u8,
    /// Short message collection buffer (max 3 bytes).
    message_buffer: [u8; 3],
    /// Current position in `message_buffer`.
    message_position: u8,
    /// Expected total byte count for the current short message.
    message_expected: u8,
    /// SysEx accumulation buffer for WSD System (0xDF) commands.
    sysex_buffer: Vec<u8>,
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
            command_phase: CommandPhase::Idle,
            running_status: 0,
            message_buffer: [0; 3],
            message_position: 0,
            message_expected: 0,
            sysex_buffer: Vec::new(),
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
    ///
    /// In intelligent mode every command receives an ACK, matching real MPU-401
    /// firmware behavior. The returned phase is saved in `command_phase` to
    /// control how subsequent data port writes are processed.
    ///
    /// In UART mode only 0xFF (reset) is recognized; all other commands are
    /// silently ignored.
    pub fn write_command(&mut self, value: u8) {
        if self.mode == MpuPc98iiMode::Uart {
            if value == CMD_RESET {
                self.mode = MpuPc98iiMode::Intelligent;
                self.response_queue.push_back(ACK);
            }
            return;
        }
        self.response_queue.push_back(ACK);
        match value {
            CMD_RESET => {
                self.send_all_notes_off();
                self.command_phase = CommandPhase::Idle;
                self.running_status = 0;
                self.message_position = 0;
                self.message_expected = 0;
                self.sysex_buffer.clear();
            }
            CMD_ENTER_UART => {
                self.send_all_notes_off();
                self.mode = MpuPc98iiMode::Uart;
                self.command_phase = CommandPhase::Idle;
            }
            _ => {
                match value {
                    0xA0..=0xA7 => self.response_queue.push_back(0x00),
                    0xAB => self.response_queue.push_back(0x00),
                    0xAC => self.response_queue.push_back(VERSION_MAJOR),
                    0xAD => self.response_queue.push_back(VERSION_MINOR),
                    0xAF => self.response_queue.push_back(DEFAULT_TEMPO),
                    _ => {}
                }
                self.command_phase = match value {
                    0xD0..=0xD7 => CommandPhase::ShortInit,
                    0xDF => CommandPhase::Long,
                    0xE0 | 0xE1 | 0xE2 | 0xE4 | 0xE6 | 0xE7 | 0xEC..=0xEF => {
                        CommandPhase::FollowByte
                    }
                    _ => CommandPhase::Idle,
                };
            }
        }
    }

    /// Reads the data register (port 0xE0D0).
    pub fn read_data(&mut self) -> u8 {
        self.response_queue.pop_front().unwrap_or(0xFF)
    }

    /// Writes the data register (port 0xE0D0).
    ///
    /// In UART mode bytes pass through directly. In intelligent mode the
    /// command phase state machine determines whether a byte is MIDI data
    /// (WSD short/long message) or protocol overhead to discard.
    pub fn write_data(&mut self, value: u8) {
        if self.mode == MpuPc98iiMode::Uart {
            self.midi_buffer.push(value);
            return;
        }
        match self.command_phase {
            CommandPhase::Idle => {}
            CommandPhase::ShortInit => self.write_data_short_init(value),
            CommandPhase::ShortCollect => self.write_data_short_collect(value),
            CommandPhase::Long => self.write_data_long(value),
            CommandPhase::FollowByte => {
                self.command_phase = CommandPhase::Idle;
            }
        }
    }

    /// Processes the first data byte of a WSD short message.
    /// Determines message length from the status byte or running status.
    fn write_data_short_init(&mut self, value: u8) {
        if value & 0x80 != 0 {
            if value & 0xF0 != 0xF0 {
                self.running_status = value;
            }
            self.message_position = 0;
            self.message_expected = MIDI_MESSAGE_LENGTH[(value >> 4) as usize];
        } else {
            self.message_buffer[0] = self.running_status;
            self.message_position = 1;
            self.message_expected = MIDI_MESSAGE_LENGTH[(self.running_status >> 4) as usize];
        }
        if self.message_expected == 0 {
            self.command_phase = CommandPhase::Idle;
            return;
        }
        self.message_buffer[self.message_position as usize] = value;
        self.message_position += 1;
        if self.message_position >= self.message_expected {
            self.flush_short_message();
        } else {
            self.command_phase = CommandPhase::ShortCollect;
        }
    }

    /// Collects subsequent data bytes of a WSD short message.
    fn write_data_short_collect(&mut self, value: u8) {
        if (self.message_position as usize) < self.message_buffer.len() {
            self.message_buffer[self.message_position as usize] = value;
        }
        self.message_position += 1;
        if self.message_position >= self.message_expected {
            self.flush_short_message();
        }
    }

    /// Forwards a completed short message to the MIDI buffer and resets the phase.
    fn flush_short_message(&mut self) {
        let length = self.message_expected as usize;
        self.midi_buffer
            .extend_from_slice(&self.message_buffer[..length]);
        self.command_phase = CommandPhase::Idle;
    }

    /// Processes data bytes for a WSD System (0xDF) long message. SysEx (0xF0)
    /// collects until 0xF7; system common messages complete based on their type.
    fn write_data_long(&mut self, value: u8) {
        self.sysex_buffer.push(value);
        let first_byte = self.sysex_buffer[0];
        let length = self.sysex_buffer.len();
        let complete = match first_byte {
            0xF0 => value == 0xF7,
            0xF2 | 0xF3 => length >= 3,
            _ => true,
        };
        if complete {
            if first_byte == 0xF0 {
                self.midi_buffer.extend_from_slice(&self.sysex_buffer);
            }
            self.sysex_buffer.clear();
            self.command_phase = CommandPhase::Idle;
        }
    }

    /// Sends MIDI CC#123 (All Notes Off) on all 16 channels.
    fn send_all_notes_off(&mut self) {
        for channel in 0..16u8 {
            self.midi_buffer.push(0xB0 | channel);
            self.midi_buffer.push(0x7B);
            self.midi_buffer.push(0x00);
        }
    }

    /// Appends all buffered MIDI bytes into `target` and clears the internal buffer.
    pub fn flush_midi_into(&mut self, target: &mut Vec<u8>) {
        target.extend_from_slice(&self.midi_buffer);
        self.midi_buffer.clear();
    }
}
