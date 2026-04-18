//! KVM vCPU wrapper.

use std::os::unix::io::AsRawFd;

use kvm_bindings::{Msrs, kvm_interrupt, kvm_msr_entry, kvm_regs, kvm_sregs};
use kvm_ioctls::VcpuFd;

use crate::{cpuid::CpuidEntries, error::Error, exit::VmExit};

/// General-purpose register state (re-exported from `kvm_bindings`).
pub type Registers = kvm_regs;

/// Segment/control register state (re-exported from `kvm_bindings`).
pub type SegmentRegisters = kvm_sregs;

/// Per-segment descriptor-cache state (re-exported from `kvm_bindings`).
///
/// Exposed so the `machine` crate can manipulate `CS`/`DS`/`ES`/`SS`/`FS`/
/// `GS` fields of a [`SegmentRegisters`] value without having to pull in
/// `kvm_bindings` directly.
pub type SegmentDescriptor = kvm_bindings::kvm_segment;

/// Pre-computed `KVM_INTERRUPT` ioctl number for x86.
///
/// `kvm-ioctls` 0.24 does not wrap `KVM_INTERRUPT`, so we invoke it directly
/// via `libc::ioctl`. Layout follows Linux's `_IOW(KVMIO, 0x86, kvm_interrupt)`:
/// `(dir=1 << 30) | (size=4 << 16) | (type=0xAE << 8) | (nr=0x86)` = `0x4004_AE86`.
const KVM_INTERRUPT_IOCTL: libc::c_ulong = 0x4004_AE86;

const _: () = {
    // Sanity: the kvm_interrupt struct size must match the size encoded in
    // the ioctl number (upper 14 bits = size).
    assert!(std::mem::size_of::<kvm_interrupt>() == 4);
};

/// A single KVM vCPU.
pub struct KvmVcpu {
    vcpu: VcpuFd,
    _vcpu_mmap_size: usize,
}

impl KvmVcpu {
    pub(crate) fn new(vcpu: VcpuFd, vcpu_mmap_size: usize) -> Self {
        Self {
            vcpu,
            _vcpu_mmap_size: vcpu_mmap_size,
        }
    }

    /// Enters the guest and runs until the next vmexit.
    ///
    /// Translates `EINTR` (caused by the budget [`BudgetTimer`](crate::BudgetTimer))
    /// into [`VmExit::Interrupted`] instead of an error.
    pub fn run(&mut self) -> Result<VmExit<'_>, Error> {
        match self.vcpu.run() {
            Ok(exit) => Ok(VmExit::from_vcpu_exit(exit)),
            Err(error) if error.errno() == libc::EINTR => Ok(VmExit::Interrupted),
            Err(error) => Err(Error::Kvm(error)),
        }
    }

    /// Reads the general-purpose register file.
    pub fn get_regs(&self) -> Result<Registers, Error> {
        self.vcpu.get_regs().map_err(Error::Kvm)
    }

    /// Writes the general-purpose register file.
    pub fn set_regs(&self, regs: &Registers) -> Result<(), Error> {
        self.vcpu.set_regs(regs).map_err(Error::Kvm)
    }

    /// Reads the segment/control register file.
    pub fn get_sregs(&self) -> Result<SegmentRegisters, Error> {
        self.vcpu.get_sregs().map_err(Error::Kvm)
    }

    /// Writes the segment/control register file.
    pub fn set_sregs(&self, sregs: &SegmentRegisters) -> Result<(), Error> {
        self.vcpu.set_sregs(sregs).map_err(Error::Kvm)
    }

    /// Installs a rewritten CPUID view of the vCPU.
    pub fn set_cpuid2(&self, cpuid: &CpuidEntries) -> Result<(), Error> {
        self.vcpu.set_cpuid2(cpuid.as_raw()).map_err(Error::Kvm)
    }

    /// Writes a list of MSR index/value pairs into the vCPU.
    pub fn set_msrs(&self, msrs: &[(u32, u64)]) -> Result<(), Error> {
        let entries: Vec<kvm_msr_entry> = msrs
            .iter()
            .map(|&(index, data)| kvm_msr_entry {
                index,
                data,
                ..Default::default()
            })
            .collect();
        let fam = Msrs::from_entries(&entries).map_err(|_| Error::MsrListTooLong)?;
        self.vcpu.set_msrs(&fam).map_err(Error::Kvm)?;
        Ok(())
    }

    /// Injects a maskable interrupt with the given vector.
    ///
    /// The caller must first check [`ready_for_interrupt_injection`] is
    /// `true`, otherwise KVM returns `EAGAIN`.
    pub fn inject_irq(&self, vector: u8) -> Result<(), Error> {
        let interrupt = kvm_interrupt {
            irq: u32::from(vector),
        };
        // SAFETY: KVM_INTERRUPT takes a pointer to a `kvm_interrupt` struct
        // (4 bytes) for the lifetime of the ioctl call. We pass a pointer to
        // a local that lives through the call.
        let ret = unsafe {
            libc::ioctl(
                self.vcpu.as_raw_fd(),
                KVM_INTERRUPT_IOCTL,
                &interrupt as *const kvm_interrupt,
            )
        };
        if ret < 0 {
            Err(Error::Os(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    /// Injects a non-maskable interrupt.
    pub fn inject_nmi(&self) -> Result<(), Error> {
        self.vcpu.nmi().map_err(Error::Kvm)
    }

    /// Returns whether the guest is currently in a state where a maskable
    /// interrupt can be delivered (IF=1, no STI shadow, not mid-instruction).
    ///
    /// Reads the `ready_for_interrupt_injection` flag from `kvm_run`. Must be
    /// called after [`run`](Self::run) returns.
    pub fn ready_for_interrupt_injection(&mut self) -> bool {
        self.vcpu.get_kvm_run().ready_for_interrupt_injection != 0
    }

    /// Requests an `IrqWindowOpen` exit the moment the guest becomes ready to
    /// accept an interrupt. KVM clears the bit automatically once the window
    /// opens and an `IrqWindowOpen` exit is delivered.
    pub fn request_interrupt_window(&mut self) {
        self.vcpu.get_kvm_run().request_interrupt_window = 1;
    }

    /// Signals `KVM_RUN` to return as soon as possible without injecting an
    /// interrupt. Complements [`BudgetTimer`](crate::BudgetTimer) for callers
    /// that want synchronous preemption.
    pub fn set_immediate_exit(&mut self, value: bool) {
        self.vcpu.set_kvm_immediate_exit(u8::from(value));
    }

    /// Borrows the underlying `kvm_ioctls::VcpuFd` for advanced operations
    /// not yet wrapped (FPU state, xsave, LAPIC, ...). Intended for future
    /// extensions.
    pub fn as_raw(&self) -> &VcpuFd {
        &self.vcpu
    }
}
