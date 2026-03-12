//! BIOS HLE (High-Level Emulation) trap controller.
//!
//! Detects a vector-number byte on the BIOS trap port (0x07F0). When a byte
//! is received, the controller latches the pending vector number so the
//! machine loop can dispatch the corresponding Rust-side BIOS handler.

use std::cell::Cell;

/// BIOS HLE trap controller.
#[derive(Default)]
pub struct BiosController {
    hle_pending: bool,
    pending_vector: u8,
    yield_requested: Cell<bool>,
}

impl BiosController {
    /// Creates a new BIOS controller with no pending trap.
    pub fn new() -> Self {
        Self {
            hle_pending: false,
            pending_vector: 0,
            yield_requested: Cell::new(false),
        }
    }

    /// Writes a byte to the trap port (0x07F0).
    ///
    /// The byte is the vector number, which sets `hle_pending`.
    pub fn write_trap_port(&mut self, value: u8) {
        self.pending_vector = value;
        self.hle_pending = true;
        self.yield_requested.set(true);
    }

    /// Returns true if a BIOS HLE trap is pending.
    pub fn hle_pending(&self) -> bool {
        self.hle_pending
    }

    /// Consumes and returns the yield-requested flag.
    ///
    /// This is an auto-clearing signal used by `cpu_should_yield` so that
    /// a stale `hle_pending` from a previous `cpu.run_for()` call does not
    /// cause the CPU to break immediately in the next call.
    pub fn take_yield_requested(&self) -> bool {
        self.yield_requested.replace(false)
    }

    /// Returns the pending interrupt vector number.
    pub fn pending_vector(&self) -> u8 {
        self.pending_vector
    }

    /// Clears the HLE pending flag after execution.
    pub fn clear_hle_pending(&mut self) {
        self.hle_pending = false;
    }
}
