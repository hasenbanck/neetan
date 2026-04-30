//! PC-9801 320KB FDD interface PPI.
//!
//! Early PC-9801 models expose the 320KB floppy subsystem through host
//! interface ports starting at 0x51.
// TODO: FIX me. Not proven to actually work with the official PC-9801F bios.

use common::warn;

/// Snapshot of the 320KB FDD PPI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fdd320PpiState {
    /// Alternating status byte returned by port C reads.
    pub status: u8,
}

/// PC-9801 320KB FDD PPI compatibility shim.
pub struct Fdd320Ppi {
    /// Public state for save-state support.
    pub state: Fdd320PpiState,
}

impl Default for Fdd320Ppi {
    fn default() -> Self {
        Self::new()
    }
}

impl Fdd320Ppi {
    /// Creates a new 320KB FDD PPI in reset state.
    pub fn new() -> Self {
        Self {
            state: Fdd320PpiState { status: 0xFF },
        }
    }

    /// Reads port 0x51.
    pub fn read_port_a(&mut self) -> u8 {
        warn!("PC-9801 320KB FDD interface PPI has only a shim");
        0x00
    }

    /// Reads port 0x55 and advances the handshake shim.
    pub fn read_port_c(&mut self) -> u8 {
        warn!("PC-9801 320KB FDD interface PPI has only a shim");
        self.state.status ^= 0xFF;
        self.state.status
    }
}

#[cfg(test)]
mod tests {
    use super::Fdd320Ppi;

    #[test]
    fn port_c_read_alternates_from_reset() {
        let mut ppi = Fdd320Ppi::new();

        assert_eq!(ppi.read_port_c(), 0x00);
        assert_eq!(ppi.read_port_c(), 0xFF);
        assert_eq!(ppi.read_port_c(), 0x00);
    }

    #[test]
    fn port_a_matches_probe_value() {
        let mut ppi = Fdd320Ppi::new();

        assert_eq!(ppi.read_port_a(), 0x00);
    }
}
