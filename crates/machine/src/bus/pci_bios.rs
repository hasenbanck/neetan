//! PCI BIOS 2.10 HLE (INT 1Ah, AH = 0xB1).
//!
//! Implements the real-mode PCI BIOS entry point used by PC-9821Ra-class
//! boot code and DOS utilities to enumerate PCI devices before the OS takes
//! over. Calls route through the shared [`PciBus`](super::pci::PciBus) on
//! [`Pc9801Bus`](crate::Pc9801Bus). On non-Ra40 machines the bus is empty
//! and the handler returns `FUNC_NOT_SUPPORTED` with CF=1, preserving the
//! existing "PCI BIOS absent" behavior.
//!
//! Reference: PCI BIOS Specification, Revision 2.1, §3.

use common::Cpu;

use crate::{Pc9801Bus, Tracing};

/// Status: the requested function completed without error.
const SUCCESSFUL: u8 = 0x00;
/// Status: the PCI BIOS does not support this function.
const FUNC_NOT_SUPPORTED: u8 = 0x81;
/// Status: vendor ID `0xFFFF` is reserved for the FIND operations.
const BAD_VENDOR_ID: u8 = 0x83;
/// Status: no device matched the FIND call.
const DEVICE_NOT_FOUND: u8 = 0x86;
/// Status: register number outside `0..=255` or not aligned to the access size.
const BAD_REGISTER_NUMBER: u8 = 0x87;

/// `AL` sub-function codes under `AH = 0xB1`.
const PCI_BIOS_PRESENT: u8 = 0x01;
const FIND_PCI_DEVICE: u8 = 0x02;
const FIND_PCI_CLASS_CODE: u8 = 0x03;
const READ_CONFIG_BYTE: u8 = 0x08;
const READ_CONFIG_WORD: u8 = 0x09;
const READ_CONFIG_DWORD: u8 = 0x0A;
const WRITE_CONFIG_BYTE: u8 = 0x0B;
const WRITE_CONFIG_WORD: u8 = 0x0C;
const WRITE_CONFIG_DWORD: u8 = 0x0D;

/// `EDX` return value for [`PCI_BIOS_PRESENT`]: `'PCI '` in little-endian.
const PCI_BIOS_SIGNATURE: u32 = 0x2049_4350;
/// Major version reported by [`PCI_BIOS_PRESENT`].
const PCI_BIOS_MAJOR: u8 = 0x02;
/// Minor version reported by [`PCI_BIOS_PRESENT`] (2.10 = BCD-like 0x10).
const PCI_BIOS_MINOR: u8 = 0x10;
/// Mechanism bitmap: bit 0 = Mechanism #1 supported.
const PCI_MECHANISM_1: u8 = 0x01;
/// Highest bus number present (bus 0 only for the POC).
const PCI_LAST_BUS: u8 = 0x00;

impl<T: Tracing> Pc9801Bus<T> {
    /// Dispatches an `INT 1Ah, AH = 0xB1` call.
    ///
    /// On machines without PCI the handler short-circuits to
    /// `FUNC_NOT_SUPPORTED`, leaving AH set to 0x81 and CF raised.
    pub(super) fn hle_pci_bios(&mut self, cpu: &mut impl Cpu) {
        if self.pci.is_empty() {
            self.pci_bios_set_status(cpu, FUNC_NOT_SUPPORTED);
            return;
        }
        match cpu.al() {
            PCI_BIOS_PRESENT => self.pci_bios_present(cpu),
            FIND_PCI_DEVICE => self.pci_bios_find_device(cpu),
            FIND_PCI_CLASS_CODE => self.pci_bios_find_class(cpu),
            READ_CONFIG_BYTE => self.pci_bios_read_config_byte(cpu),
            READ_CONFIG_WORD => self.pci_bios_read_config_word(cpu),
            READ_CONFIG_DWORD => self.pci_bios_read_config_dword(cpu),
            WRITE_CONFIG_BYTE => self.pci_bios_write_config_byte(cpu),
            WRITE_CONFIG_WORD => self.pci_bios_write_config_word(cpu),
            WRITE_CONFIG_DWORD => self.pci_bios_write_config_dword(cpu),
            _ => self.pci_bios_set_status(cpu, FUNC_NOT_SUPPORTED),
        }
    }

    fn pci_bios_set_status(&mut self, cpu: &mut impl Cpu, status: u8) {
        cpu.set_ah(status);
        self.set_iret_cf(cpu, status != SUCCESSFUL);
    }

    fn pci_bios_present(&mut self, cpu: &mut impl Cpu) {
        cpu.set_al(PCI_MECHANISM_1);
        cpu.set_bh(PCI_BIOS_MAJOR);
        cpu.set_bl(PCI_BIOS_MINOR);
        cpu.set_cl(PCI_LAST_BUS);
        cpu.set_edx(PCI_BIOS_SIGNATURE);
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }

    fn pci_bios_find_device(&mut self, cpu: &mut impl Cpu) {
        let vendor = cpu.dx();
        let device = cpu.cx();
        let index = cpu.si();
        if vendor == 0xFFFF {
            self.pci_bios_set_status(cpu, BAD_VENDOR_ID);
            return;
        }
        match self.pci.find_device(vendor, device, index) {
            Some((bus, dev, func)) => {
                cpu.set_bh(bus);
                cpu.set_bl(encode_dev_func(dev, func));
                self.pci_bios_set_status(cpu, SUCCESSFUL);
            }
            None => self.pci_bios_set_status(cpu, DEVICE_NOT_FOUND),
        }
    }

    fn pci_bios_find_class(&mut self, cpu: &mut impl Cpu) {
        let class_code = cpu.ecx();
        let index = cpu.si();
        match self.pci.find_class(class_code, index) {
            Some((bus, dev, func)) => {
                cpu.set_bh(bus);
                cpu.set_bl(encode_dev_func(dev, func));
                self.pci_bios_set_status(cpu, SUCCESSFUL);
            }
            None => self.pci_bios_set_status(cpu, DEVICE_NOT_FOUND),
        }
    }

    fn pci_bios_read_config_byte(&mut self, cpu: &mut impl Cpu) {
        let (bus, dev, func) = decode_bus_dev_func(cpu);
        let offset = cpu.di();
        if offset > 0xFF {
            self.pci_bios_set_status(cpu, BAD_REGISTER_NUMBER);
            return;
        }
        let value = self.pci.read_config_byte(bus, dev, func, offset as u8);
        cpu.set_cl(value);
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }

    fn pci_bios_read_config_word(&mut self, cpu: &mut impl Cpu) {
        let (bus, dev, func) = decode_bus_dev_func(cpu);
        let offset = cpu.di();
        if offset > 0xFF || offset & 1 != 0 {
            self.pci_bios_set_status(cpu, BAD_REGISTER_NUMBER);
            return;
        }
        let value = self.pci.read_config_word(bus, dev, func, offset as u8);
        cpu.set_cx(value);
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }

    fn pci_bios_read_config_dword(&mut self, cpu: &mut impl Cpu) {
        let (bus, dev, func) = decode_bus_dev_func(cpu);
        let offset = cpu.di();
        if offset > 0xFF || offset & 3 != 0 {
            self.pci_bios_set_status(cpu, BAD_REGISTER_NUMBER);
            return;
        }
        let value = self.pci.read_config_dword(bus, dev, func, offset as u8);
        cpu.set_ecx(value);
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }

    fn pci_bios_write_config_byte(&mut self, cpu: &mut impl Cpu) {
        let (bus, dev, func) = decode_bus_dev_func(cpu);
        let offset = cpu.di();
        if offset > 0xFF {
            self.pci_bios_set_status(cpu, BAD_REGISTER_NUMBER);
            return;
        }
        self.pci
            .write_config_byte(bus, dev, func, offset as u8, cpu.cl());
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }

    fn pci_bios_write_config_word(&mut self, cpu: &mut impl Cpu) {
        let (bus, dev, func) = decode_bus_dev_func(cpu);
        let offset = cpu.di();
        if offset > 0xFF || offset & 1 != 0 {
            self.pci_bios_set_status(cpu, BAD_REGISTER_NUMBER);
            return;
        }
        self.pci
            .write_config_word(bus, dev, func, offset as u8, cpu.cx());
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }

    fn pci_bios_write_config_dword(&mut self, cpu: &mut impl Cpu) {
        let (bus, dev, func) = decode_bus_dev_func(cpu);
        let offset = cpu.di();
        if offset > 0xFF || offset & 3 != 0 {
            self.pci_bios_set_status(cpu, BAD_REGISTER_NUMBER);
            return;
        }
        self.pci
            .write_config_dword(bus, dev, func, offset as u8, cpu.ecx());
        self.pci_bios_set_status(cpu, SUCCESSFUL);
    }
}

/// Packs a PCI device/function pair into `BL` per PCI BIOS §3.3.
fn encode_dev_func(dev: u8, func: u8) -> u8 {
    ((dev & 0x1F) << 3) | (func & 0x07)
}

/// Unpacks `(bus, dev, func)` from the canonical `BH`/`BL` registers used
/// by every PCI BIOS config-space call.
fn decode_bus_dev_func(cpu: &impl Cpu) -> (u8, u8, u8) {
    let bus = cpu.bh();
    let bl = cpu.bl();
    let dev = (bl >> 3) & 0x1F;
    let func = bl & 0x07;
    (bus, dev, func)
}

#[cfg(test)]
mod tests {
    use common::MachineModel;
    use cpu::{CPU_MODEL_486, I386};

    use crate::{
        Pc9801Bus,
        bus::pci::{I440fxHostBridge, NecCbusBridge},
    };

    /// Cbus-bridge slot used by the Ra40 convention (matches `bus/init.rs`).
    const NEC_CBUS_BRIDGE_SLOT: u8 = 6;

    /// Builds a machine whose PciBus is populated with the I440FX host
    /// bridge and the NEC PCI-Cbus bridge, matching the Ra40 layout.
    /// We use a PC-9821AP bus as the host for these tests because the
    /// Ra40-specific construction path routes through the (as-yet unbuilt)
    /// KVM backend; attaching the two devices manually produces an
    /// equivalent PCI surface for HLE testing purposes.
    fn bus_with_ra40_pci() -> (Pc9801Bus, I386<{ CPU_MODEL_486 }>) {
        let mut bus: Pc9801Bus = Pc9801Bus::new(MachineModel::PC9821AP, 48000);
        bus.pci.attach(0, 0, Box::new(I440fxHostBridge::new()));
        bus.pci
            .attach(NEC_CBUS_BRIDGE_SLOT, 0, Box::new(NecCbusBridge::new()));
        let cpu = I386::<{ CPU_MODEL_486 }>::new();
        (bus, cpu)
    }

    #[test]
    fn pci_bios_present_returns_signature_and_version() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x01);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x00, "AH should be SUCCESSFUL (0x00)");
        assert_eq!(cpu.al(), 0x01, "AL should report Mechanism #1");
        assert_eq!(cpu.bh(), 0x02, "BH should report major version 2");
        assert_eq!(cpu.bl(), 0x10, "BL should report minor version 2.10");
        assert_eq!(cpu.cl(), 0x00, "CL should report last bus = 0");
        // 'PCI ' in little-endian: P=0x50, C=0x43, I=0x49, ' '=0x20
        assert_eq!(cpu.edx(), 0x2049_4350, "EDX should hold the PCI signature");
    }

    #[test]
    fn find_device_returns_cbus_bridge_slot() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x02);
        cpu.set_dx(0x1033); // NEC vendor
        cpu.set_cx(0x0001); // Cbus bridge device
        cpu.set_si(0);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x00, "find-device should succeed");
        assert_eq!(cpu.bh(), 0, "bus should be 0");
        assert_eq!(
            cpu.bl(),
            NEC_CBUS_BRIDGE_SLOT << 3,
            "dev/func should encode slot 6, func 0"
        );
    }

    #[test]
    fn find_device_with_reserved_vendor_returns_bad_vendor() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x02);
        cpu.set_dx(0xFFFF); // reserved
        cpu.set_cx(0x1234);
        cpu.set_si(0);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x83, "reserved vendor should yield BAD_VENDOR_ID");
    }

    #[test]
    fn find_device_returns_not_found_when_absent() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x02);
        cpu.set_dx(0x1234);
        cpu.set_cx(0x5678);
        cpu.set_si(0);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(
            cpu.ah(),
            0x86,
            "missing device should yield DEVICE_NOT_FOUND"
        );
    }

    #[test]
    fn read_config_word_returns_host_bridge_vendor() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x09);
        cpu.set_bh(0); // bus 0
        cpu.set_bl(0); // dev 0, func 0
        cpu.set_di(0); // offset 0 = vendor ID
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x00, "read-config-word should succeed");
        assert_eq!(cpu.cx(), 0x8086, "vendor ID should be Intel (82441FX)");
    }

    #[test]
    fn read_config_dword_returns_vendor_device_pair() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x0A);
        cpu.set_bh(0);
        cpu.set_bl(0);
        cpu.set_di(0);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x00);
        // Low 16 bits = vendor (0x8086), high 16 bits = device (0x1237)
        assert_eq!(cpu.ecx(), 0x1237_8086);
    }

    #[test]
    fn read_config_word_with_misaligned_offset_fails() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x09);
        cpu.set_bh(0);
        cpu.set_bl(0);
        cpu.set_di(1); // misaligned
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(
            cpu.ah(),
            0x87,
            "misaligned word read should yield BAD_REGISTER_NUMBER"
        );
    }

    #[test]
    fn write_config_byte_persists_on_device() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x0B);
        cpu.set_bh(0);
        cpu.set_bl(0); // host bridge
        cpu.set_di(0x0C); // cache line size (writable)
        cpu.set_cl(0x20);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x00, "write-config-byte should succeed");
        // Read it back via the PciBus directly.
        assert_eq!(bus.pci.read_config_byte(0, 0, 0, 0x0C), 0x20);
    }

    #[test]
    fn find_class_returns_host_bridge() {
        let (mut bus, mut cpu) = bus_with_ra40_pci();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x03);
        cpu.set_ecx(0x0006_0000); // host bridge class code
        cpu.set_si(0);
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(cpu.ah(), 0x00, "find-class should succeed");
        assert_eq!(cpu.bh(), 0, "bus = 0");
        assert_eq!(cpu.bl(), 0, "dev = 0, func = 0");
    }

    /// Non-PCI machines (every existing variant) must see
    /// `FUNC_NOT_SUPPORTED` even when the guest invokes a valid PCI BIOS
    /// sub-function. This is the "no behavior change for existing machines"
    /// guarantee from the POC plan.
    #[test]
    fn pci_bios_present_on_non_pci_machine_returns_func_not_supported() {
        let mut bus: Pc9801Bus = Pc9801Bus::new(MachineModel::PC9801VM, 48000);
        let mut cpu = cpu::V30::new();
        use common::Cpu;
        cpu.set_ah(0xB1);
        cpu.set_al(0x01);
        assert!(bus.pci.is_empty(), "VM should have no PCI devices");
        bus.hle_pci_bios(&mut cpu);
        assert_eq!(
            cpu.ah(),
            0x81,
            "non-PCI machine should return FUNC_NOT_SUPPORTED"
        );
    }
}
