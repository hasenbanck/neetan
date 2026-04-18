//! Translated KVM vCPU exit reasons.

use kvm_ioctls::VcpuExit;

/// Reason the guest vCPU returned control to the host.
///
/// Borrowing variants hold a reference into the vCPU's `kvm_run` mmap region;
/// the borrow is released as soon as the next call to
/// [`KvmVcpu::run`](crate::KvmVcpu::run) begins.
#[derive(Debug)]
pub enum VmExit<'a> {
    /// Guest executed `in` on the given I/O port.
    ///
    /// The host must write the response value into `data` (little-endian, 1,
    /// 2, or 4 bytes) before the next `run`.
    IoIn {
        /// I/O port number.
        port: u16,
        /// Scratch buffer for the response value.
        data: &'a mut [u8],
    },
    /// Guest executed `out` on the given I/O port.
    IoOut {
        /// I/O port number.
        port: u16,
        /// Little-endian write payload (1, 2, or 4 bytes).
        data: &'a [u8],
    },
    /// Guest performed a memory-mapped I/O read at an address not covered by
    /// a KVM memory slot.
    MmioRead {
        /// Guest physical address.
        address: u64,
        /// Scratch buffer for the response value.
        data: &'a mut [u8],
    },
    /// Guest performed a memory-mapped I/O write at an address not covered by
    /// a KVM memory slot.
    MmioWrite {
        /// Guest physical address.
        address: u64,
        /// Little-endian write payload.
        data: &'a [u8],
    },
    /// Guest executed `hlt`.
    Hlt,
    /// KVM is ready to accept an interrupt injection.
    InterruptWindowOpen,
    /// `KVM_RUN` was interrupted by a signal.
    Interrupted,
    /// Guest triggered a system shutdown (triple fault, poweroff, etc.).
    Shutdown,
    /// Entering the guest failed.
    FailEntry {
        /// Hardware-reported entry failure reason.
        reason: u64,
    },
    /// An internal KVM error was reported.
    InternalError,
    /// An exit reason the wrapper does not handle individually.
    Other,
}

impl<'a> VmExit<'a> {
    pub(crate) fn from_vcpu_exit(exit: VcpuExit<'a>) -> Self {
        match exit {
            VcpuExit::IoIn(port, data) => Self::IoIn { port, data },
            VcpuExit::IoOut(port, data) => Self::IoOut { port, data },
            VcpuExit::MmioRead(address, data) => Self::MmioRead { address, data },
            VcpuExit::MmioWrite(address, data) => Self::MmioWrite { address, data },
            VcpuExit::Hlt => Self::Hlt,
            VcpuExit::IrqWindowOpen => Self::InterruptWindowOpen,
            VcpuExit::Intr => Self::Interrupted,
            VcpuExit::Shutdown => Self::Shutdown,
            VcpuExit::FailEntry(reason, _cpu) => Self::FailEntry { reason },
            VcpuExit::InternalError => Self::InternalError,
            _ => Self::Other,
        }
    }
}
