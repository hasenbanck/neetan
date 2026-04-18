//! Linux KVM wrapper for the PC-9821Ra40 machine variant.
//!
//! It exposes a minimal surface on top of `/dev/kvm` ioctls: VM creation,
//! guest memory management, vCPU execution, and IRQ/NMI injection. The
//! higher-level `machine` crate drives it from `Pc9821Ra40::run_for`.
//!
//! All real implementation is gated behind `target_os = "linux"`. Compiling
//! or referencing this crate on non-Linux hosts will surface a clear compile
//! error at use sites.

#![warn(missing_docs)]

mod error;
mod leaked_slice;

pub use error::Error;
pub use leaked_slice::LeakedSlice;

#[cfg(target_os = "linux")]
mod cpuid;
#[cfg(target_os = "linux")]
mod exit;
#[cfg(target_os = "linux")]
mod memory;
#[cfg(target_os = "linux")]
mod system;
#[cfg(target_os = "linux")]
mod timer;
#[cfg(target_os = "linux")]
mod vcpu;
#[cfg(target_os = "linux")]
mod vm;

#[cfg(target_os = "linux")]
pub use cpuid::{CpuidEntries, pentium2_cpuid};
#[cfg(target_os = "linux")]
pub use exit::VmExit;
#[cfg(target_os = "linux")]
pub use memory::{HostMemory, MemorySlotHandle};
#[cfg(target_os = "linux")]
pub use system::KvmSystem;
#[cfg(target_os = "linux")]
pub use timer::BudgetTimer;
#[cfg(target_os = "linux")]
pub use vcpu::{KvmVcpu, Registers, SegmentDescriptor, SegmentRegisters};
#[cfg(target_os = "linux")]
pub use vm::KvmVm;
