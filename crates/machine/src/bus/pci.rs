//! PCI subsystem for PC-9821Ra-class machines.
//!
//! Implements PCI Configuration Mechanism #1 (I/O ports 0xCF8/0xCFC) and a
//! minimal device model. Only bus 0 is supported (the PC-98 PCI topology is
//! single-bus: host bridge at slot 0, NEC PCI-Cbus bridge at slot 6, optional
//! PCI graphics card at other slots).
//!
//! The [`PciBus`] is always present on every machine variant but starts
//! empty. Non-Ra40 machines never populate it, so `is_empty()` returns true
//! and PCI ports return the PCI-spec open-bus value `0xFFFF_FFFF`.
//!
//! Reference: PCI Local Bus Specification, Revision 2.1, section 3.2.2.3.2.

mod cbus_bridge_nec;
mod host_bridge_i440fx;

pub use cbus_bridge_nec::NecCbusBridge;
pub use host_bridge_i440fx::I440fxHostBridge;

/// Number of PCI device slots on bus 0.
const PCI_SLOT_COUNT: usize = 32;
/// Number of functions per PCI slot.
const PCI_FUNCTION_COUNT: usize = 8;
/// PCI Configuration Mechanism #1 address-latch port (0xCF8-0xCFB).
const CONFIG_ADDRESS_PORT: u16 = 0x0CF8;
/// PCI Configuration Mechanism #1 data port (0xCFC-0xCFF).
const CONFIG_DATA_PORT: u16 = 0x0CFC;
/// Enable bit for the address latch (bit 31).
const CONFIG_ADDRESS_ENABLE: u32 = 0x8000_0000;
/// Value returned by PCI for accesses to unpopulated (device, function) slots.
const PCI_OPEN_BUS: u32 = 0xFFFF_FFFF;

/// Minimal PCI device interface for configuration-space-only stubs.
///
/// The POC only needs devices to expose their 256-byte configuration space.
/// BAR decoding, interrupts, and DMA are all out of scope.
pub trait PciDevice: Send {
    /// 16-bit vendor identifier (configuration offset 0x00-0x01).
    fn vendor_id(&self) -> u16;

    /// 16-bit device identifier (configuration offset 0x02-0x03).
    fn device_id(&self) -> u16;

    /// Reads one byte of configuration space at `offset` (0-255).
    fn read_config_byte(&self, offset: u8) -> u8;

    /// Writes one byte of configuration space at `offset` (0-255). Devices
    /// may silently ignore writes to read-only fields.
    fn write_config_byte(&mut self, offset: u8, value: u8);

    /// Reads two bytes of configuration space. Default implementation
    /// composes from [`read_config_byte`]; devices may override for
    /// side-effectful registers.
    fn read_config_word(&self, offset: u8) -> u16 {
        let low = u16::from(self.read_config_byte(offset));
        let high = u16::from(self.read_config_byte(offset.wrapping_add(1)));
        low | (high << 8)
    }

    /// Writes two bytes of configuration space. Default implementation
    /// decomposes into byte writes.
    fn write_config_word(&mut self, offset: u8, value: u16) {
        self.write_config_byte(offset, value as u8);
        self.write_config_byte(offset.wrapping_add(1), (value >> 8) as u8);
    }

    /// Reads four bytes of configuration space.
    fn read_config_dword(&self, offset: u8) -> u32 {
        let low = u32::from(self.read_config_word(offset));
        let high = u32::from(self.read_config_word(offset.wrapping_add(2)));
        low | (high << 16)
    }

    /// Writes four bytes of configuration space.
    fn write_config_dword(&mut self, offset: u8, value: u32) {
        self.write_config_word(offset, value as u16);
        self.write_config_word(offset.wrapping_add(2), (value >> 16) as u16);
    }
}

/// PCI bus 0 with up to 32 devices × 8 functions.
pub struct PciBus {
    devices: [[Option<Box<dyn PciDevice>>; PCI_FUNCTION_COUNT]; PCI_SLOT_COUNT],
    /// Last value written to `0xCF8` (the configuration address latch).
    address: u32,
    /// Running count of attached devices, for cheap `is_empty` checks.
    attached: u32,
}

impl Default for PciBus {
    fn default() -> Self {
        Self::new()
    }
}

impl PciBus {
    /// Creates an empty PCI bus with no attached devices.
    pub fn new() -> Self {
        Self {
            devices: std::array::from_fn(|_| std::array::from_fn(|_| None)),
            address: 0,
            attached: 0,
        }
    }

    /// Whether any device is attached. Non-Ra40 machines leave the bus empty
    /// and I/O dispatchers use this to bypass PCI handling entirely.
    pub fn is_empty(&self) -> bool {
        self.attached == 0
    }

    /// Attaches `device` at bus 0, `(dev, func)`.
    ///
    /// Panics if the slot is already occupied or if `dev >= 32` /
    /// `func >= 8`.
    pub fn attach(&mut self, dev: u8, func: u8, device: Box<dyn PciDevice>) {
        let dev = dev as usize;
        let func = func as usize;
        assert!(dev < PCI_SLOT_COUNT, "PCI device number out of range");
        assert!(
            func < PCI_FUNCTION_COUNT,
            "PCI function number out of range"
        );
        assert!(
            self.devices[dev][func].is_none(),
            "PCI slot {dev}.{func} already occupied"
        );
        self.devices[dev][func] = Some(device);
        self.attached += 1;
    }

    /// Translates the current `0xCF8` latch into (bus, dev, func, dword-register).
    ///
    /// Returns `None` if the enable bit is clear or the bus number is non-zero
    /// (only bus 0 is populated).
    fn decoded_address(&self) -> Option<DecodedAddress> {
        if self.address & CONFIG_ADDRESS_ENABLE == 0 {
            return None;
        }
        let bus = ((self.address >> 16) & 0xFF) as u8;
        if bus != 0 {
            return None;
        }
        let dev = ((self.address >> 11) & 0x1F) as u8;
        let func = ((self.address >> 8) & 0x07) as u8;
        let register = (self.address & 0xFC) as u8;
        Some(DecodedAddress {
            dev,
            func,
            register,
        })
    }

    fn device(&self, dev: u8, func: u8) -> Option<&(dyn PciDevice + 'static)> {
        match &self.devices[dev as usize][func as usize] {
            Some(boxed) => Some(boxed.as_ref()),
            None => None,
        }
    }

    fn device_mut(&mut self, dev: u8, func: u8) -> Option<&mut (dyn PciDevice + 'static)> {
        match &mut self.devices[dev as usize][func as usize] {
            Some(boxed) => Some(boxed.as_mut()),
            None => None,
        }
    }

    /// Byte-sized read from `0xCF8-0xCFF`.
    pub fn io_read_byte(&self, port: u16) -> u8 {
        match port {
            0x0CF8..=0x0CFB => {
                let shift = (port - CONFIG_ADDRESS_PORT) * 8;
                (self.address >> shift) as u8
            }
            0x0CFC..=0x0CFF => {
                let byte_offset = (port - CONFIG_DATA_PORT) as u8;
                match self.decoded_address() {
                    Some(decoded) => match self.device(decoded.dev, decoded.func) {
                        Some(device) => device.read_config_byte(decoded.register | byte_offset),
                        None => 0xFF,
                    },
                    None => 0xFF,
                }
            }
            _ => 0xFF,
        }
    }

    /// Byte-sized write to `0xCF8-0xCFF`.
    pub fn io_write_byte(&mut self, port: u16, value: u8) {
        match port {
            0x0CF8..=0x0CFB => {
                let shift = (port - CONFIG_ADDRESS_PORT) * 8;
                let mask = !(0xFFu32 << shift);
                self.address = (self.address & mask) | (u32::from(value) << shift);
            }
            0x0CFC..=0x0CFF => {
                let byte_offset = (port - CONFIG_DATA_PORT) as u8;
                if let Some(decoded) = self.decoded_address()
                    && let Some(device) = self.device_mut(decoded.dev, decoded.func)
                {
                    device.write_config_byte(decoded.register | byte_offset, value);
                }
            }
            _ => {}
        }
    }

    /// Word-sized read. Falls back to byte composition unless the port is
    /// `0xCFC` (data register, aligned) in which case the device sees a word.
    pub fn io_read_word(&self, port: u16) -> u16 {
        match port {
            0x0CFC | 0x0CFE => {
                let byte_offset = (port - CONFIG_DATA_PORT) as u8;
                match self.decoded_address() {
                    Some(decoded) => match self.device(decoded.dev, decoded.func) {
                        Some(device) => device.read_config_word(decoded.register | byte_offset),
                        None => 0xFFFF,
                    },
                    None => 0xFFFF,
                }
            }
            _ => {
                let low = u16::from(self.io_read_byte(port));
                let high = u16::from(self.io_read_byte(port.wrapping_add(1)));
                low | (high << 8)
            }
        }
    }

    /// Word-sized write.
    pub fn io_write_word(&mut self, port: u16, value: u16) {
        match port {
            0x0CFC | 0x0CFE => {
                let byte_offset = (port - CONFIG_DATA_PORT) as u8;
                if let Some(decoded) = self.decoded_address()
                    && let Some(device) = self.device_mut(decoded.dev, decoded.func)
                {
                    device.write_config_word(decoded.register | byte_offset, value);
                }
            }
            _ => {
                self.io_write_byte(port, value as u8);
                self.io_write_byte(port.wrapping_add(1), (value >> 8) as u8);
            }
        }
    }

    /// Dword-sized read. Always DWORD-aligned per PCI spec.
    pub fn io_read_dword(&self, port: u16) -> u32 {
        match port {
            0x0CF8 => self.address,
            0x0CFC => match self.decoded_address() {
                Some(decoded) => match self.device(decoded.dev, decoded.func) {
                    Some(device) => device.read_config_dword(decoded.register),
                    None => PCI_OPEN_BUS,
                },
                None => PCI_OPEN_BUS,
            },
            _ => {
                let low = u32::from(self.io_read_word(port));
                let high = u32::from(self.io_read_word(port.wrapping_add(2)));
                low | (high << 16)
            }
        }
    }

    /// Dword-sized write.
    pub fn io_write_dword(&mut self, port: u16, value: u32) {
        match port {
            0x0CF8 => self.address = value,
            0x0CFC => {
                if let Some(decoded) = self.decoded_address()
                    && let Some(device) = self.device_mut(decoded.dev, decoded.func)
                {
                    device.write_config_dword(decoded.register, value);
                }
            }
            _ => {
                self.io_write_word(port, value as u16);
                self.io_write_word(port.wrapping_add(2), (value >> 16) as u16);
            }
        }
    }

    /// Searches the bus for a device matching `(vendor, device)`. Returns the
    /// `index`-th match as `(bus, dev, func)`, where `bus` is always 0.
    ///
    /// Used by the PCI BIOS HLE [`FIND_PCI_DEVICE`](super::pci_bios) call.
    pub fn find_device(&self, vendor: u16, device: u16, index: u16) -> Option<(u8, u8, u8)> {
        let mut remaining = index;
        for (dev, functions) in self.devices.iter().enumerate() {
            for (func, entry) in functions.iter().enumerate() {
                if let Some(entry) = entry
                    && entry.vendor_id() == vendor
                    && entry.device_id() == device
                {
                    if remaining == 0 {
                        return Some((0, dev as u8, func as u8));
                    }
                    remaining -= 1;
                }
            }
        }
        None
    }

    /// Searches the bus for a device with the given 24-bit class code (`class
    /// << 16 | subclass << 8 | prog_if`). Used by
    /// [`FIND_PCI_CLASS_CODE`](super::pci_bios).
    pub fn find_class(&self, class_code: u32, index: u16) -> Option<(u8, u8, u8)> {
        let mut remaining = index;
        for (dev, functions) in self.devices.iter().enumerate() {
            for (func, entry) in functions.iter().enumerate() {
                if let Some(entry) = entry {
                    let prog_if = u32::from(entry.read_config_byte(0x09));
                    let subclass = u32::from(entry.read_config_byte(0x0A));
                    let class = u32::from(entry.read_config_byte(0x0B));
                    let encoded = (class << 16) | (subclass << 8) | prog_if;
                    if encoded == class_code {
                        if remaining == 0 {
                            return Some((0, dev as u8, func as u8));
                        }
                        remaining -= 1;
                    }
                }
            }
        }
        None
    }

    /// Raw byte-granular config-space read by `(bus, dev, func, offset)`.
    pub fn read_config_byte(&self, bus: u8, dev: u8, func: u8, offset: u8) -> u8 {
        if bus != 0 || dev >= PCI_SLOT_COUNT as u8 || func >= PCI_FUNCTION_COUNT as u8 {
            return 0xFF;
        }
        match self.device(dev, func) {
            Some(device) => device.read_config_byte(offset),
            None => 0xFF,
        }
    }

    /// Raw word-granular config-space read.
    pub fn read_config_word(&self, bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
        if bus != 0 || dev >= PCI_SLOT_COUNT as u8 || func >= PCI_FUNCTION_COUNT as u8 {
            return 0xFFFF;
        }
        match self.device(dev, func) {
            Some(device) => device.read_config_word(offset),
            None => 0xFFFF,
        }
    }

    /// Raw dword-granular config-space read.
    pub fn read_config_dword(&self, bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
        if bus != 0 || dev >= PCI_SLOT_COUNT as u8 || func >= PCI_FUNCTION_COUNT as u8 {
            return PCI_OPEN_BUS;
        }
        match self.device(dev, func) {
            Some(device) => device.read_config_dword(offset),
            None => PCI_OPEN_BUS,
        }
    }

    /// Raw byte-granular config-space write.
    pub fn write_config_byte(&mut self, bus: u8, dev: u8, func: u8, offset: u8, value: u8) {
        if bus != 0 || dev >= PCI_SLOT_COUNT as u8 || func >= PCI_FUNCTION_COUNT as u8 {
            return;
        }
        if let Some(device) = self.device_mut(dev, func) {
            device.write_config_byte(offset, value);
        }
    }

    /// Raw word-granular config-space write.
    pub fn write_config_word(&mut self, bus: u8, dev: u8, func: u8, offset: u8, value: u16) {
        if bus != 0 || dev >= PCI_SLOT_COUNT as u8 || func >= PCI_FUNCTION_COUNT as u8 {
            return;
        }
        if let Some(device) = self.device_mut(dev, func) {
            device.write_config_word(offset, value);
        }
    }

    /// Raw dword-granular config-space write.
    pub fn write_config_dword(&mut self, bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
        if bus != 0 || dev >= PCI_SLOT_COUNT as u8 || func >= PCI_FUNCTION_COUNT as u8 {
            return;
        }
        if let Some(device) = self.device_mut(dev, func) {
            device.write_config_dword(offset, value);
        }
    }
}

struct DecodedAddress {
    dev: u8,
    func: u8,
    register: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDevice {
        vendor: u16,
        device: u16,
        class_code: u32,
        data: [u8; 256],
    }

    impl MockDevice {
        fn new(vendor: u16, device: u16, class_code: u32) -> Self {
            let mut data = [0u8; 256];
            data[0] = vendor as u8;
            data[1] = (vendor >> 8) as u8;
            data[2] = device as u8;
            data[3] = (device >> 8) as u8;
            data[0x09] = class_code as u8;
            data[0x0A] = (class_code >> 8) as u8;
            data[0x0B] = (class_code >> 16) as u8;
            Self {
                vendor,
                device,
                class_code,
                data,
            }
        }
    }

    impl PciDevice for MockDevice {
        fn vendor_id(&self) -> u16 {
            self.vendor
        }
        fn device_id(&self) -> u16 {
            self.device
        }
        fn read_config_byte(&self, offset: u8) -> u8 {
            self.data[offset as usize]
        }
        fn write_config_byte(&mut self, offset: u8, value: u8) {
            self.data[offset as usize] = value;
        }
    }

    fn bus_with_mock() -> PciBus {
        let mut bus = PciBus::new();
        bus.attach(0, 0, Box::new(MockDevice::new(0x8086, 0x1237, 0x060000)));
        bus.attach(6, 0, Box::new(MockDevice::new(0x1033, 0x0001, 0x068000)));
        bus
    }

    #[test]
    fn empty_bus_reads_return_open_bus() {
        let bus = PciBus::new();
        assert!(bus.is_empty());
        assert_eq!(bus.io_read_dword(0x0CFC), PCI_OPEN_BUS);
        assert_eq!(bus.io_read_word(0x0CFC), 0xFFFF);
        assert_eq!(bus.io_read_byte(0x0CFC), 0xFF);
    }

    #[test]
    fn address_latch_roundtrips_via_dword() {
        let mut bus = PciBus::new();
        bus.io_write_dword(0x0CF8, 0x8000_1234);
        assert_eq!(bus.io_read_dword(0x0CF8), 0x8000_1234);
    }

    #[test]
    fn address_latch_roundtrips_via_byte_writes() {
        let mut bus = PciBus::new();
        bus.io_write_byte(0x0CF8, 0x78);
        bus.io_write_byte(0x0CF9, 0x56);
        bus.io_write_byte(0x0CFA, 0x00);
        bus.io_write_byte(0x0CFB, 0x80);
        assert_eq!(bus.io_read_dword(0x0CF8), 0x8000_5678);
    }

    #[test]
    fn disabled_address_returns_open_bus_on_data_port() {
        let bus = bus_with_mock();
        // Enable bit clear -> data port reads open bus.
        assert_eq!(bus.io_read_dword(0x0CFC), PCI_OPEN_BUS);
    }

    #[test]
    fn dword_read_returns_vendor_device_at_offset_0() {
        let mut bus = bus_with_mock();
        // Select device 0:0.0, register 0.
        bus.io_write_dword(0x0CF8, 0x8000_0000);
        let value = bus.io_read_dword(0x0CFC);
        // Low word = vendor (0x8086), high word = device (0x1237).
        assert_eq!(value & 0xFFFF, 0x8086);
        assert_eq!((value >> 16) & 0xFFFF, 0x1237);
    }

    #[test]
    fn byte_read_from_data_port_returns_correct_byte() {
        let mut bus = bus_with_mock();
        bus.io_write_dword(0x0CF8, 0x8000_0000);
        assert_eq!(bus.io_read_byte(0x0CFC), 0x86); // vendor low
        assert_eq!(bus.io_read_byte(0x0CFD), 0x80); // vendor high
        assert_eq!(bus.io_read_byte(0x0CFE), 0x37); // device low
        assert_eq!(bus.io_read_byte(0x0CFF), 0x12); // device high
    }

    #[test]
    fn word_read_returns_device_id() {
        let mut bus = bus_with_mock();
        bus.io_write_dword(0x0CF8, 0x8000_0000);
        assert_eq!(bus.io_read_word(0x0CFE), 0x1237);
    }

    #[test]
    fn unpopulated_slot_returns_ffff() {
        let mut bus = bus_with_mock();
        // Slot 1 is empty.
        bus.io_write_dword(0x0CF8, 0x8000_0800);
        assert_eq!(bus.io_read_dword(0x0CFC), PCI_OPEN_BUS);
    }

    #[test]
    fn find_device_matches_cbus_bridge_at_slot_6() {
        let bus = bus_with_mock();
        assert_eq!(bus.find_device(0x1033, 0x0001, 0), Some((0, 6, 0)));
        assert_eq!(bus.find_device(0x1033, 0x0001, 1), None);
    }

    #[test]
    fn find_device_finds_host_bridge() {
        let bus = bus_with_mock();
        assert_eq!(bus.find_device(0x8086, 0x1237, 0), Some((0, 0, 0)));
    }

    #[test]
    fn find_device_returns_none_on_miss() {
        let bus = bus_with_mock();
        assert_eq!(bus.find_device(0xFFFF, 0xFFFF, 0), None);
    }

    #[test]
    fn find_class_matches_host_bridge() {
        let bus = bus_with_mock();
        assert_eq!(bus.find_class(0x060000, 0), Some((0, 0, 0)));
    }

    #[test]
    fn find_class_matches_cbus_bridge() {
        let bus = bus_with_mock();
        assert_eq!(bus.find_class(0x068000, 0), Some((0, 6, 0)));
    }

    #[test]
    fn writes_to_config_space_persist() {
        let mut bus = bus_with_mock();
        bus.write_config_byte(0, 0, 0, 0x0C, 0x40);
        assert_eq!(bus.read_config_byte(0, 0, 0, 0x0C), 0x40);
    }

    #[test]
    fn cross_bus_config_access_returns_open_bus() {
        let bus = bus_with_mock();
        assert_eq!(bus.read_config_dword(1, 0, 0, 0x00), PCI_OPEN_BUS);
    }
}
