//! Roland MT-32 emulation in pure Rust.
//!
//! Provides a render thread that accepts MIDI bytes and produces
//! resampled 48 kHz stereo f32 audio chunks for mixing into the
//! emulator's audio output.

#![forbid(unsafe_code)]

mod analog;
mod breverb;
mod context;
mod enumerations;
mod la32_float_wave_generator;
mod la32_ramp;
mod memory_region;
mod midi_event_queue;
mod midi_stream_parser;
mod part;
mod partial;
mod partial_manager;
mod poly;
mod rom_info;
mod sha1;
mod state;
mod structures;
mod synth;
mod tables;
mod thread;
mod tva;
mod tvf;
mod tvp;

pub use thread::{MuntChannels, MuntError, MuntSharedBuffer, MuntThread};
