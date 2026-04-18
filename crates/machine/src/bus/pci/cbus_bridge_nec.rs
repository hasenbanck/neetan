//! NEC PCI-Cbus bridge stub.
//!
//! The PC-9821Ra-series exposes the legacy C-bus expansion slots through a
//! bridge device that the guest OS enumerates at PCI bus 0, slot 6 (matching
//! NP21W's `98graphbridge.c`/`cbusbridge.c` layout). Vendor `0x1033` ("NEC"),
//! device `0x0001`, class code `0x068000` ("other bridge").
//!
//! The bridge's presence alone is what matters for PCI enumeration in the
//! POC; the actual C-bus transactions are handled by the existing
//! [`Pc9801Bus`](crate::Pc9801Bus) PIO paths. Configuration-space writes are
//! accepted but have no side effects yet.

use crate::bus::pci::PciDevice;

/// Vendor ID: NEC.
const VENDOR_ID: u16 = 0x1033;
/// Device ID: NEC PCI-Cbus bridge.
const DEVICE_ID: u16 = 0x0001;
/// Revision ID.
const REVISION_ID: u8 = 0x00;
/// Class code bytes at offsets 0x09/0x0A/0x0B:
///   prog IF = 0x00, subclass = 0x80 (other bridge), class = 0x06.
const CLASS_PROG_IF: u8 = 0x00;
const CLASS_SUBCLASS: u8 = 0x80;
const CLASS_CODE: u8 = 0x06;
/// Header type 0.
const HEADER_TYPE: u8 = 0x00;
/// Command register default: bus master + memory + I/O enabled.
const COMMAND_DEFAULT: u16 = 0x0007;
/// Status register default.
const STATUS_DEFAULT: u16 = 0x0280;

/// NEC PCI-Cbus bridge PCI device.
pub struct NecCbusBridge {
    config: [u8; 256],
}

impl Default for NecCbusBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl NecCbusBridge {
    /// Builds the bridge with reset-state configuration values.
    pub fn new() -> Self {
        let mut config = [0u8; 256];
        config[0x00] = VENDOR_ID as u8;
        config[0x01] = (VENDOR_ID >> 8) as u8;
        config[0x02] = DEVICE_ID as u8;
        config[0x03] = (DEVICE_ID >> 8) as u8;
        config[0x04] = COMMAND_DEFAULT as u8;
        config[0x05] = (COMMAND_DEFAULT >> 8) as u8;
        config[0x06] = STATUS_DEFAULT as u8;
        config[0x07] = (STATUS_DEFAULT >> 8) as u8;
        config[0x08] = REVISION_ID;
        config[0x09] = CLASS_PROG_IF;
        config[0x0A] = CLASS_SUBCLASS;
        config[0x0B] = CLASS_CODE;
        config[0x0E] = HEADER_TYPE;
        Self { config }
    }
}

impl PciDevice for NecCbusBridge {
    fn vendor_id(&self) -> u16 {
        VENDOR_ID
    }

    fn device_id(&self) -> u16 {
        DEVICE_ID
    }

    fn read_config_byte(&self, offset: u8) -> u8 {
        self.config[offset as usize]
    }

    fn write_config_byte(&mut self, offset: u8, value: u8) {
        match offset {
            0x00..=0x03 | 0x08..=0x0B | 0x0E => { /* read-only */ }
            _ => {
                self.config[offset as usize] = value;
            }
        }
    }
}
