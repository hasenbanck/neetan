//! Nuked-SC55 emulation in pure Rust.
//!
//! Provides a render thread that accepts MIDI bytes and produces
//! resampled 48 kHz stereo f32 audio chunks for mixing into the
//! emulator's audio output.

#![forbid(unsafe_code)]

mod context;
mod mcu;
mod mcu_interrupt;
mod mcu_opcodes;
mod mcu_timer;
mod pcm;
mod state;
mod submcu;
mod thread;

use state::Sc55State;
pub use thread::{Sc55Channels, Sc55Error, Sc55SharedBuffer, Sc55Thread};
