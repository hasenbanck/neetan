//! [`common::Cpu`] shim over a KVM vCPU.
//!
//! HLE handlers (`bus/bios.rs`, `bus/pci_bios.rs`, etc.) are written against
//! the [`common::Cpu`] trait and expect a `&mut impl Cpu` that represents the
//! guest's current register state. Under KVM, that state lives in the kernel
//! and is only visible through `KVM_GET_REGS` / `KVM_GET_SREGS` ioctls.
//!
//! [`KvmCpuShim`] bridges the two:
//!
//! 1. [`KvmCpuShim::fetch`] issues one pair of ioctls and caches both
//!    register files.
//! 2. The HLE handler runs against the shim, reading cached registers and
//!    marking the cache dirty on every setter.
//! 3. [`KvmCpuShim::commit`] writes dirty register files back to the vCPU.
//!
//! A debug assertion enforces the fetch → commit round-trip so HLE handlers
//! that mutate state can never forget to push it back to KVM.

use common::{Cpu, CpuType, SegmentRegister};
use kvm::{Error as KvmError, KvmVcpu, Registers, SegmentDescriptor, SegmentRegisters};

/// Cached view of the KVM vCPU register files, with dirty tracking.
///
/// Cheap to build via [`fetch`](Self::fetch) and flush via
/// [`commit`](Self::commit). The caller is responsible for calling `commit`
/// (or explicitly discarding via [`discard`](Self::discard)) before the shim
/// is dropped; in debug builds this is verified by a drop-time assertion.
pub(crate) struct KvmCpuShim {
    regs: Registers,
    sregs: SegmentRegisters,
    regs_dirty: bool,
    sregs_dirty: bool,
    /// Set by `commit` or `discard` to suppress the drop-time assertion.
    released: bool,
}

impl KvmCpuShim {
    /// Reads the current register and sregs state from the KVM vCPU.
    pub(crate) fn fetch(vcpu: &KvmVcpu) -> Result<Self, KvmError> {
        let regs = vcpu.get_regs()?;
        let sregs = vcpu.get_sregs()?;
        Ok(Self {
            regs,
            sregs,
            regs_dirty: false,
            sregs_dirty: false,
            released: false,
        })
    }

    /// Writes any dirty register state back to the KVM vCPU.
    ///
    /// No-op for register files that were not mutated. Marks the shim as
    /// released so [`Drop`] does not assert.
    pub(crate) fn commit(mut self, vcpu: &KvmVcpu) -> Result<(), KvmError> {
        if self.regs_dirty {
            vcpu.set_regs(&self.regs)?;
        }
        if self.sregs_dirty {
            vcpu.set_sregs(&self.sregs)?;
        }
        self.released = true;
        Ok(())
    }

    /// Explicitly discards the shim without writing register state back.
    ///
    /// Used in error paths where committing would propagate garbage.
    #[cfg(test)]
    pub(crate) fn discard(mut self) {
        self.released = true;
    }

    /// Writes `value` into the 16-bit word `[hi..lo]` of `rax`/`rbx`/... while
    /// preserving the upper 48 bits.
    fn set_low_word(value_64: &mut u64, word: u16) {
        *value_64 = (*value_64 & !0xFFFF) | u64::from(word);
    }

    /// Writes `value` into the 32-bit dword of `rax`/`rbx`/... while
    /// preserving the upper 32 bits.
    fn set_low_dword(value_64: &mut u64, dword: u32) {
        *value_64 = (*value_64 & !0xFFFF_FFFF) | u64::from(dword);
    }

    fn segment_field_mut(&mut self, seg: SegmentRegister) -> &mut SegmentDescriptor {
        match seg {
            SegmentRegister::ES => &mut self.sregs.es,
            SegmentRegister::CS => &mut self.sregs.cs,
            SegmentRegister::SS => &mut self.sregs.ss,
            SegmentRegister::DS => &mut self.sregs.ds,
        }
    }

    fn segment_field(&self, seg: SegmentRegister) -> &SegmentDescriptor {
        match seg {
            SegmentRegister::ES => &self.sregs.es,
            SegmentRegister::CS => &self.sregs.cs,
            SegmentRegister::SS => &self.sregs.ss,
            SegmentRegister::DS => &self.sregs.ds,
        }
    }
}

impl Drop for KvmCpuShim {
    fn drop(&mut self) {
        debug_assert!(
            self.released,
            "KvmCpuShim dropped without commit() or discard(); HLE writes would be lost"
        );
    }
}

impl Cpu for KvmCpuShim {
    fn run_for(&mut self, _cycles_to_run: u64, _bus: &mut impl common::Bus) -> u64 {
        unreachable!("KvmCpuShim is HLE-only and must never drive execution")
    }

    fn reset(&mut self) {
        unreachable!("KvmCpuShim does not implement reset; use Pc9821Ra40::cold_reset")
    }

    fn halted(&self) -> bool {
        false
    }

    fn ax(&self) -> u16 {
        self.regs.rax as u16
    }

    fn set_ax(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rax, v);
        self.regs_dirty = true;
    }

    fn bx(&self) -> u16 {
        self.regs.rbx as u16
    }

    fn set_bx(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rbx, v);
        self.regs_dirty = true;
    }

    fn cx(&self) -> u16 {
        self.regs.rcx as u16
    }

    fn set_cx(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rcx, v);
        self.regs_dirty = true;
    }

    fn dx(&self) -> u16 {
        self.regs.rdx as u16
    }

    fn set_dx(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rdx, v);
        self.regs_dirty = true;
    }

    fn sp(&self) -> u16 {
        self.regs.rsp as u16
    }

    fn set_sp(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rsp, v);
        self.regs_dirty = true;
    }

    fn bp(&self) -> u16 {
        self.regs.rbp as u16
    }

    fn set_bp(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rbp, v);
        self.regs_dirty = true;
    }

    fn si(&self) -> u16 {
        self.regs.rsi as u16
    }

    fn set_si(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rsi, v);
        self.regs_dirty = true;
    }

    fn di(&self) -> u16 {
        self.regs.rdi as u16
    }

    fn set_di(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rdi, v);
        self.regs_dirty = true;
    }

    fn es(&self) -> u16 {
        self.sregs.es.selector
    }

    fn set_es(&mut self, v: u16) {
        self.sregs.es.selector = v;
        self.sregs_dirty = true;
    }

    fn cs(&self) -> u16 {
        self.sregs.cs.selector
    }

    fn set_cs(&mut self, v: u16) {
        self.sregs.cs.selector = v;
        self.sregs_dirty = true;
    }

    fn ss(&self) -> u16 {
        self.sregs.ss.selector
    }

    fn set_ss(&mut self, v: u16) {
        self.sregs.ss.selector = v;
        self.sregs_dirty = true;
    }

    fn ds(&self) -> u16 {
        self.sregs.ds.selector
    }

    fn set_ds(&mut self, v: u16) {
        self.sregs.ds.selector = v;
        self.sregs_dirty = true;
    }

    fn ip(&self) -> u16 {
        self.regs.rip as u16
    }

    fn set_ip(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rip, v);
        self.regs_dirty = true;
    }

    fn flags(&self) -> u16 {
        self.regs.rflags as u16
    }

    fn set_flags(&mut self, v: u16) {
        Self::set_low_word(&mut self.regs.rflags, v);
        self.regs_dirty = true;
    }

    fn cpu_type(&self) -> CpuType {
        CpuType::Pentium2
    }

    fn load_segment_real_mode(&mut self, seg: SegmentRegister, selector: u16) {
        let segment = self.segment_field_mut(seg);
        segment.selector = selector;
        segment.base = u64::from(selector) << 4;
        segment.limit = 0xFFFF;
        segment.type_ = 0x03; // data, read/write (code-like segments get set elsewhere)
        segment.present = 1;
        segment.dpl = 0;
        segment.db = 0;
        segment.s = 1;
        segment.l = 0;
        segment.g = 0;
        segment.avl = 0;
        segment.unusable = 0;
        self.sregs_dirty = true;
    }

    fn segment_base(&self, seg: SegmentRegister) -> u32 {
        self.segment_field(seg).base as u32
    }

    fn cr0(&self) -> u32 {
        self.sregs.cr0 as u32
    }

    fn cr3(&self) -> u32 {
        self.sregs.cr3 as u32
    }

    fn eax(&self) -> u32 {
        self.regs.rax as u32
    }

    fn set_eax(&mut self, v: u32) {
        Self::set_low_dword(&mut self.regs.rax, v);
        self.regs_dirty = true;
    }

    fn ebx(&self) -> u32 {
        self.regs.rbx as u32
    }

    fn set_ebx(&mut self, v: u32) {
        Self::set_low_dword(&mut self.regs.rbx, v);
        self.regs_dirty = true;
    }

    fn ecx(&self) -> u32 {
        self.regs.rcx as u32
    }

    fn set_ecx(&mut self, v: u32) {
        Self::set_low_dword(&mut self.regs.rcx, v);
        self.regs_dirty = true;
    }

    fn edx(&self) -> u32 {
        self.regs.rdx as u32
    }

    fn set_edx(&mut self, v: u32) {
        Self::set_low_dword(&mut self.regs.rdx, v);
        self.regs_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use common::{Cpu, CpuType, SegmentRegister};
    use kvm::{Registers, SegmentRegisters};

    use super::KvmCpuShim;

    /// Builds a shim directly from register values for unit testing.
    ///
    /// Bypasses `fetch` so the tests don't need a live KVM vCPU.
    fn shim_from(regs: Registers, sregs: SegmentRegisters) -> KvmCpuShim {
        KvmCpuShim {
            regs,
            sregs,
            regs_dirty: false,
            sregs_dirty: false,
            released: false,
        }
    }

    #[test]
    fn get_set_16bit_preserves_upper_bits() {
        let regs = Registers {
            rax: 0x1122_3344_5566_7788,
            ..Registers::default()
        };
        let mut shim = shim_from(regs, SegmentRegisters::default());
        assert_eq!(shim.ax(), 0x7788);
        shim.set_ax(0xBEEF);
        assert_eq!(shim.regs.rax, 0x1122_3344_5566_BEEF);
        shim.discard();
    }

    #[test]
    fn set_ah_al_preserve_adjacent_bytes() {
        let mut shim = shim_from(Registers::default(), SegmentRegisters::default());
        shim.set_ax(0x1234);
        shim.set_ah(0xCD);
        assert_eq!(shim.ax(), 0xCD34);
        shim.set_al(0xEF);
        assert_eq!(shim.ax(), 0xCDEF);
        shim.discard();
    }

    #[test]
    fn set_edx_preserves_upper_32_bits() {
        let regs = Registers {
            rdx: 0xAAAA_BBBB_CCCC_DDDD,
            ..Registers::default()
        };
        let mut shim = shim_from(regs, SegmentRegisters::default());
        shim.set_edx(0x1234_5678);
        assert_eq!(shim.regs.rdx, 0xAAAA_BBBB_1234_5678);
        shim.discard();
    }

    #[test]
    fn segment_set_updates_selector_only() {
        let mut sregs = SegmentRegisters::default();
        sregs.es.base = 0xFFFF_0000;
        sregs.es.selector = 0xF000;
        let mut shim = shim_from(Registers::default(), sregs);
        shim.set_es(0x1234);
        // Selector updated; base untouched by `set_es`.
        assert_eq!(shim.sregs.es.selector, 0x1234);
        assert_eq!(shim.sregs.es.base, 0xFFFF_0000);
        shim.discard();
    }

    #[test]
    fn load_segment_real_mode_updates_selector_and_base() {
        let mut shim = shim_from(Registers::default(), SegmentRegisters::default());
        shim.load_segment_real_mode(SegmentRegister::DS, 0x1234);
        assert_eq!(shim.sregs.ds.selector, 0x1234);
        assert_eq!(shim.sregs.ds.base, 0x1_2340);
        assert_eq!(shim.sregs.ds.limit, 0xFFFF);
        assert_eq!(shim.sregs.ds.present, 1);
        assert_eq!(shim.segment_base(SegmentRegister::DS), 0x1_2340);
        shim.discard();
    }

    #[test]
    fn cpu_type_reports_pentium2() {
        let shim = shim_from(Registers::default(), SegmentRegisters::default());
        assert_eq!(shim.cpu_type(), CpuType::Pentium2);
        shim.discard();
    }

    #[test]
    fn cr0_cr3_read_through_from_sregs() {
        let sregs = SegmentRegisters {
            cr0: 0x6000_0011,
            cr3: 0x0010_0000,
            ..SegmentRegisters::default()
        };
        let shim = shim_from(Registers::default(), sregs);
        assert_eq!(shim.cr0(), 0x6000_0011);
        assert_eq!(shim.cr3(), 0x0010_0000);
        shim.discard();
    }

    #[test]
    fn dirty_flags_track_reg_mutations() {
        let mut shim = shim_from(Registers::default(), SegmentRegisters::default());
        assert!(!shim.regs_dirty);
        assert!(!shim.sregs_dirty);
        shim.set_bx(1);
        assert!(shim.regs_dirty);
        assert!(!shim.sregs_dirty);
        shim.set_cs(0xF000);
        assert!(shim.sregs_dirty);
        shim.discard();
    }
}
