//! Roland MT-32 sound module device.
//!
//! Wraps the munt emulation core, managing the render thread internally.
//! The thread is an implementation detail - callers only interact with
//! [`Mt32::new`] (initialization) and [`Mt32::exchange`] (audio synchronization).

use std::{
    path::Path,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

pub use munt_oxide::MuntError;
use munt_oxide::{MuntSharedBuffer, MuntThread};

/// Roland MT-32 sound module.
///
/// Each audio chunk the emulation thread:
/// 1. Waits for the render thread to finish the previous chunk.
/// 2. Mixes the rendered audio into the output.
/// 3. Fills new MIDI data and signals the render thread.
pub struct Mt32 {
    shared: Arc<(Mutex<MuntSharedBuffer>, Condvar)>,
    join_handle: Option<JoinHandle<()>>,
}

impl Mt32 {
    /// Loads MT-32 ROMs from the given directory and starts the render thread.
    pub fn new(rom_directory: &Path) -> Result<Self, MuntError> {
        let (shared, join_handle) = MuntThread::start(rom_directory)?;
        Ok(Self {
            shared,
            join_handle: Some(join_handle),
        })
    }

    /// Waits for the render thread to finish, mixes audio into `output`,
    /// then fills new MIDI data via `fill` and signals the render thread.
    pub fn exchange(&self, volume: f32, output: &mut [f32], fill: impl FnOnce(&mut Vec<u8>)) {
        let (mutex, condvar) = &*self.shared;

        let mut buf = condvar
            .wait_while(mutex.lock().unwrap(), |buf| {
                !buf.render_done && !buf.shutdown
            })
            .unwrap();

        for (out, &sample) in output.iter_mut().zip(buf.audio.iter()) {
            *out += sample * volume;
        }

        fill(&mut buf.midi);
        buf.render_done = false;
        buf.midi_ready = true;
        condvar.notify_one();
    }
}

impl Drop for Mt32 {
    fn drop(&mut self) {
        {
            let (mutex, condvar) = &*self.shared;
            let mut buf = mutex.lock().unwrap();
            buf.shutdown = true;
            condvar.notify_one();
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}
