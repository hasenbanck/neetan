//! Intel 82441FX host-bridge stub.
//!
//! Responds as the PMC (PCI and Memory Controller) on the 440FX chipset:
//! vendor `0x8086`, device `0x1237`, class host-bridge (`0x060000`), header
//! type `0x00`. The POC only implements the subset of the configuration
//! space that guest BIOSes read during enumeration; DRAM sizing registers
//! and the PAM (Programmable Attribute Map) return their reset defaults.

use crate::bus::pci::PciDevice;

/// Vendor ID: Intel.
const VENDOR_ID: u16 = 0x8086;
/// Device ID: 82441FX PMC.
const DEVICE_ID: u16 = 0x1237;
/// Revision ID (datasheet reports 0x02 on stepping B0).
const REVISION_ID: u8 = 0x02;
/// Class code bytes at offsets 0x09/0x0A/0x0B:
///   programming IF = 0x00, subclass = 0x00 (host bridge), class = 0x06.
const CLASS_PROG_IF: u8 = 0x00;
const CLASS_SUBCLASS: u8 = 0x00;
const CLASS_CODE: u8 = 0x06;
/// Header type 0 (normal function, no multi-function bit set).
const HEADER_TYPE: u8 = 0x00;
/// Bus master + memory space enabled (post-BIOS default on real hardware).
const COMMAND_DEFAULT: u16 = 0x0006;
/// Status: devsel timing = medium, fast back-to-back capable.
const STATUS_DEFAULT: u16 = 0x2280;

/// 82441FX host-bridge PCI device.
pub struct I440fxHostBridge {
    config: [u8; 256],
}

impl Default for I440fxHostBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl I440fxHostBridge {
    /// Builds the bridge with PMC reset-state configuration values.
    pub fn new() -> Self {
        let mut config = [0u8; 256];
        // 0x00-0x01: vendor
        config[0x00] = VENDOR_ID as u8;
        config[0x01] = (VENDOR_ID >> 8) as u8;
        // 0x02-0x03: device
        config[0x02] = DEVICE_ID as u8;
        config[0x03] = (DEVICE_ID >> 8) as u8;
        // 0x04-0x05: command
        config[0x04] = COMMAND_DEFAULT as u8;
        config[0x05] = (COMMAND_DEFAULT >> 8) as u8;
        // 0x06-0x07: status
        config[0x06] = STATUS_DEFAULT as u8;
        config[0x07] = (STATUS_DEFAULT >> 8) as u8;
        // 0x08: revision
        config[0x08] = REVISION_ID;
        // 0x09-0x0B: class code
        config[0x09] = CLASS_PROG_IF;
        config[0x0A] = CLASS_SUBCLASS;
        config[0x0B] = CLASS_CODE;
        // 0x0E: header type
        config[0x0E] = HEADER_TYPE;
        // 0x34: capabilities pointer (0 = no extended capabilities)
        config[0x34] = 0x00;
        // Chipset-specific DRAM row configuration registers (0x59-0x5F) begin
        // zeroed; BIOS probes RAM then writes them. Not modeled here.
        Self { config }
    }
}

impl PciDevice for I440fxHostBridge {
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
        // Vendor/device/class/revision are read-only per PCI spec. The
        // command and status registers accept writes to the mutable bits
        // only; other fields (PAM, DRAM sizing) pass through directly in
        // this stub, since the guest BIOS may want to read back what it
        // wrote during setup.
        match offset {
            0x00..=0x03 | 0x08..=0x0B | 0x0E => { /* read-only */ }
            0x06..=0x07 => {
                // Status bits are "write 1 to clear" sticky. For the POC we
                // accept byte writes as-is; this is close enough for boot.
                self.config[offset as usize] = value;
            }
            _ => {
                self.config[offset as usize] = value;
            }
        }
    }
}
