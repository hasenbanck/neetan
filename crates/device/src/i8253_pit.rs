//! i8253 Programmable Interval Timer for the PC-98.
//!
//! On the PC-98, all GATE pins are hardwired HIGH (active), so modes 1 and 5
//! (which require a rising GATE edge to start) are effectively unusable.
//! No GATE handling is needed. See `undoc98/io_tcu.txt`.
//!
//! The BIOS programs channel 0 in mode 3 (square wave) for the 100 Hz
//! interval timer. Modes 2 and 3 are treated identically here because the
//! PC-98 system timer only observes the output-low edge, not the duty cycle.

use std::ops::{Deref, DerefMut};

use common::{EventKind, Scheduler};

/// Control word mask: read/load mode (bits 5:4).
pub const PIT_CTRL_RL: u8 = 0x30;
/// RL field: low byte only (01b).
pub const PIT_RL_L: u8 = 0x10;
/// RL field: high byte only (10b).
pub const PIT_RL_H: u8 = 0x20;
/// RL field: low byte then high byte (11b).
pub const PIT_RL_ALL: u8 = 0x30;
/// Internal flag: control word written but counter not yet loaded.
pub const PIT_STAT_CMD: u8 = 0x40;

/// Channel flag: read toggle (alternates low/high byte on read).
pub const PIT_FLAG_R: u8 = 0x01;
/// Channel flag: write toggle (alternates low/high byte on write).
pub const PIT_FLAG_W: u8 = 0x02;
/// Channel flag: latch extend (second byte of latched word pending).
pub const PIT_FLAG_L: u8 = 0x04;
/// Channel flag: counter value latched.
pub const PIT_FLAG_C: u8 = 0x10;
/// Channel flag: interrupt pending (output not yet asserted).
pub const PIT_FLAG_I: u8 = 0x20;

/// Channel 0 control word.
/// Bits: SC=01, RL=01 (LSB only), M=011 (mode 3, square wave), BCD=0.
/// Stored as `0x56 & 0x3F` = 0x16 (SC bits stripped for the ctrl register).
const CH0_CTRL_WORD: u8 = 0x56;

/// Beep counter reload value for 8 MHz-series machines.
/// 2.4576 MHz / 998 ~ 2.463 kHz (approximately 2 kHz beep).
const BEEP_COUNTER_8MHZ: u16 = 998;

/// Beep counter reload value for 5/10 MHz-series machines.
/// 1.9968 MHz / 1229 ~ 1.625 kHz (approximately 2 kHz beep).
const BEEP_COUNTER_5_10MHZ: u16 = 1229;

/// Channel 2 control word written by BIOS: 0xB6.
/// Bits: SC=10 (ch2), RL=11 (LSB then MSB), M=011 (mode 3, square wave), BCD=0.
/// Stored as `0xB6 & 0x3F` = 0x36 (strip SC bits for the ctrl register).
const CH2_CTRL_WORD: u8 = 0xB6;

/// Snapshot of a single PIT channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I8253PitChannelState {
    /// Control word.
    pub ctrl: u8,
    /// Channel flags.
    pub flag: u8,
    /// Programmed reload value.
    pub value: u16,
    /// Latched counter snapshot.
    pub latch: u16,
    /// CPU cycle when counter was last loaded.
    pub last_load_cycle: u64,
}

/// Snapshot of the i8253 PIT (all 3 channels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I8253PitState {
    /// The three channel snapshots.
    pub channels: [I8253PitChannelState; 3],
}

/// i8253 PIT with 3 channels.
pub struct I8253Pit {
    /// Embedded state for save/restore.
    pub state: I8253PitState,
}

impl Deref for I8253Pit {
    type Target = I8253PitState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for I8253Pit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for I8253Pit {
    fn default() -> Self {
        Self::new(true)
    }
}

impl I8253Pit {
    /// Creates a new PIT in the PC-98 boot default state.
    ///
    /// `is_8mhz_lineage`: selects the beep counter reload value.
    ///   - `true`  -> 998  (8 MHz-series, PIT clock 2.4576 MHz)
    ///   - `false` -> 1229 (5/10 MHz-series, PIT clock 1.9968 MHz)
    pub fn new(is_8mhz_lineage: bool) -> Self {
        let beep_counter = if is_8mhz_lineage {
            BEEP_COUNTER_8MHZ
        } else {
            BEEP_COUNTER_5_10MHZ
        };

        Self {
            state: I8253PitState {
                channels: [
                    // Channel 0: interval timer (100 Hz system tick).
                    // BIOS programs mode 3 (square wave) via control word 0x56.
                    I8253PitChannelState {
                        ctrl: CH0_CTRL_WORD & 0x3F,
                        flag: PIT_FLAG_I,
                        value: 0,
                        latch: 0,
                        last_load_cycle: 0,
                    },
                    // Channel 1: beep tone generator.
                    // Counter value is lineage-dependent; ctrl/flag zeroed.
                    I8253PitChannelState {
                        ctrl: 0,
                        flag: 0,
                        value: beep_counter,
                        latch: 0,
                        last_load_cycle: 0,
                    },
                    // Channel 2: RS-232C baud rate generator.
                    // BIOS programs mode 3 (square wave) via control word 0xB6.
                    I8253PitChannelState {
                        ctrl: CH2_CTRL_WORD & 0x3F,
                        flag: 0,
                        value: 0,
                        latch: 0,
                        last_load_cycle: 0,
                    },
                ],
            },
        }
    }

    /// Creates a new PIT with all registers zeroed.
    pub fn new_zeroed() -> Self {
        Self {
            state: I8253PitState {
                channels: [
                    I8253PitChannelState {
                        ctrl: 0,
                        flag: 0,
                        value: 0,
                        latch: 0,
                        last_load_cycle: 0,
                    },
                    I8253PitChannelState {
                        ctrl: 0,
                        flag: 0,
                        value: 0,
                        latch: 0,
                        last_load_cycle: 0,
                    },
                    I8253PitChannelState {
                        ctrl: 0,
                        flag: 0,
                        value: 0,
                        latch: 0,
                        last_load_cycle: 0,
                    },
                ],
            },
        }
    }

    /// Writes the control word for a channel.
    /// If RL=00, latches the counter. Otherwise, sets the mode/access.
    pub fn write_control(
        &mut self,
        channel: usize,
        value: u8,
        current_cycle: u64,
        cpu_clock_hz: u32,
        pit_clock_hz: u32,
    ) {
        let ch = &mut self.channels[channel];
        if value & PIT_CTRL_RL != 0 {
            // Mode set
            ch.ctrl = (value & 0x3F) | PIT_STAT_CMD;
            ch.flag &= !(PIT_FLAG_R | PIT_FLAG_W | PIT_FLAG_L | PIT_FLAG_C);
        } else {
            // Latch command: snapshot current count
            let count = get_count(ch, current_cycle, cpu_clock_hz, pit_clock_hz);
            ch.flag |= PIT_FLAG_C;
            ch.flag &= !PIT_FLAG_L;
            ch.latch = count;
        }
    }

    /// Writes a byte to a channel's counter register.
    /// Returns `true` if the caller should skip event scheduling
    /// (word mode first byte, or mode 1 inhibit).
    pub fn write_counter(&mut self, channel: usize, value: u8) -> bool {
        let ch = &mut self.channels[channel];

        match ch.ctrl & PIT_CTRL_RL {
            PIT_RL_L => {
                ch.value = value as u16;
            }
            PIT_RL_H => {
                ch.value = (value as u16) << 8;
            }
            PIT_RL_ALL => {
                ch.flag ^= PIT_FLAG_W;
                if ch.flag & PIT_FLAG_W != 0 {
                    // First byte (LSB)
                    ch.value = (ch.value & 0xFF00) | value as u16;
                    return true;
                }
                // Second byte (MSB)
                ch.value = (ch.value & 0x00FF) | ((value as u16) << 8);
            }
            _ => {}
        }

        ch.ctrl &= !PIT_STAT_CMD;

        // Mode 1 with I flag: don't restart
        if (ch.ctrl & 0x06) == 0x02 && (ch.flag & PIT_FLAG_I != 0) {
            return true;
        }

        false
    }

    /// Reads a byte from a channel's counter register.
    pub fn read_counter(
        &mut self,
        channel: usize,
        current_cycle: u64,
        cpu_clock_hz: u32,
        pit_clock_hz: u32,
    ) -> u8 {
        let ch = &mut self.channels[channel];
        let rl = ch.ctrl & PIT_CTRL_RL;

        let word = if ch.flag & (PIT_FLAG_C | PIT_FLAG_L) != 0 {
            ch.flag &= !PIT_FLAG_C;
            if rl == PIT_RL_ALL {
                ch.flag ^= PIT_FLAG_L;
            }
            ch.latch
        } else {
            get_count(ch, current_cycle, cpu_clock_hz, pit_clock_hz)
        };

        match rl {
            PIT_RL_L => word as u8,
            PIT_RL_H => (word >> 8) as u8,
            _ => {
                let result = if ch.flag & PIT_FLAG_R == 0 {
                    word as u8
                } else {
                    (word >> 8) as u8
                };
                ch.flag ^= PIT_FLAG_R;
                result
            }
        }
    }

    /// Computes the current count for a channel.
    pub fn get_count(
        &self,
        channel: usize,
        current_cycle: u64,
        cpu_clock_hz: u32,
        pit_clock_hz: u32,
    ) -> u16 {
        get_count(
            &self.channels[channel],
            current_cycle,
            cpu_clock_hz,
            pit_clock_hz,
        )
    }

    /// Schedules the next timer 0 event based on the current reload value.
    pub fn schedule_timer0(
        &self,
        scheduler: &mut Scheduler,
        cpu_clock_hz: u32,
        pit_clock_hz: u32,
        current_cycle: u64,
    ) {
        let ch = &self.channels[0];
        let reload = if ch.value == 0 {
            0x10000u64
        } else {
            ch.value as u64
        };
        let cpu_cycles = reload * cpu_clock_hz as u64 / pit_clock_hz as u64;
        scheduler.schedule(EventKind::PitTimer0, current_cycle + cpu_cycles);
    }

    /// Handles the timer 0 event. Returns `true` if an IRQ should be raised.
    pub fn on_timer0_event(
        &mut self,
        scheduler: &mut Scheduler,
        cpu_clock_hz: u32,
        pit_clock_hz: u32,
        current_cycle: u64,
    ) -> bool {
        let ch = &mut self.channels[0];
        let raise_irq = ch.flag & PIT_FLAG_I != 0;
        if raise_irq {
            ch.flag &= !PIT_FLAG_I;
        }
        // Always reschedule ch0 so get_count() keeps working, but only
        // re-arm the interrupt flag for periodic modes (2 and 3).
        //
        // NP21W's systimer() does the same: mode 2 re-arms PIT_FLAG_I and
        // reschedules with the programmed count; all other modes reschedule
        // without arming the interrupt. We extend this to mode 3 as well
        // because the PC-98 ITF programs ch0 in mode 3 (periodic square
        // wave) for the system interval timer.
        let mode = (ch.ctrl >> 1) & 7;
        if mode == 2 || mode == 3 {
            ch.flag |= PIT_FLAG_I;
        }
        ch.last_load_cycle = current_cycle;
        self.schedule_timer0(scheduler, cpu_clock_hz, pit_clock_hz, current_cycle);
        raise_irq
    }
}

/// Computes the current count value for a channel via lazy evaluation.
fn get_count(
    ch: &I8253PitChannelState,
    current_cycle: u64,
    cpu_clock_hz: u32,
    pit_clock_hz: u32,
) -> u16 {
    let count_period = if ch.value == 0 {
        0x10000u64
    } else {
        ch.value as u64
    };

    let elapsed_cpu = current_cycle.saturating_sub(ch.last_load_cycle);
    if elapsed_cpu == 0 {
        return ch.value;
    }

    let elapsed_pit = elapsed_cpu * pit_clock_hz as u64 / cpu_clock_hz as u64;

    let mode = (ch.ctrl >> 1) & 7;
    match mode {
        2 | 3 => {
            // Periodic modes: counter wraps at reload value
            let pos = elapsed_pit % count_period;
            if pos == 0 {
                ch.value
            } else {
                (count_period - pos) as u16
            }
        }
        _ => {
            // One-shot / other modes: count down to 0
            if elapsed_pit >= count_period {
                0
            } else {
                (count_period - elapsed_pit) as u16
            }
        }
    }
}
