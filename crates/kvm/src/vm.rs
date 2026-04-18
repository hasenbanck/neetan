//! KVM VM wrapper.

use kvm_bindings::kvm_userspace_memory_region;
use kvm_ioctls::VmFd;

use crate::{
    error::Error,
    leaked_slice::LeakedSlice,
    memory::{HostMemory, MemorySlotHandle},
    vcpu::KvmVcpu,
};

/// A single KVM virtual machine.
///
/// Owns the VM fd, tracks allocated memory slots, and spawns vCPUs.
pub struct KvmVm {
    vm: VmFd,
    vcpu_mmap_size: usize,
    next_slot: u32,
}

impl KvmVm {
    pub(crate) fn new(vm: VmFd, vcpu_mmap_size: usize) -> Self {
        Self {
            vm,
            vcpu_mmap_size,
            next_slot: 0,
        }
    }

    /// Installs the KVM x86 TSS at the given guest physical address.
    ///
    /// Required on Intel hosts before the first vCPU runs in real mode.
    pub fn set_tss_address(&self, guest_phys_addr: usize) -> Result<(), Error> {
        self.vm.set_tss_address(guest_phys_addr).map_err(Error::Kvm)
    }

    /// Registers a region of host memory as guest RAM at `guest_phys_addr`.
    ///
    /// `host_offset` and `size` must together fit within `host_memory`.
    /// The returned handle identifies the slot for later updates.
    pub fn register_ram_slot(
        &mut self,
        guest_phys_addr: u64,
        host_memory: &mut HostMemory,
        host_offset: usize,
        size: usize,
    ) -> Result<MemorySlotHandle, Error> {
        assert!(
            host_offset
                .checked_add(size)
                .is_some_and(|end| end <= host_memory.len()),
            "register_ram_slot: region out of bounds (offset={host_offset}, size={size}, host_len={})",
            host_memory.len()
        );
        let slot = self.next_slot;
        self.next_slot = self.next_slot.checked_add(1).ok_or(Error::TooManySlots)?;

        // SAFETY: host_memory is held alive by the caller for at least as long
        // as this KvmVm (enforced at the machine crate level: Pc9821Ra40 owns
        // both). We verified offset + size is in bounds above, so the pointer
        // arithmetic lands inside the mapping.
        let userspace_addr = unsafe { host_memory.as_mut_ptr().add(host_offset) } as u64;

        let region = kvm_userspace_memory_region {
            slot,
            flags: 0,
            guest_phys_addr,
            memory_size: size as u64,
            userspace_addr,
        };

        // SAFETY: the memory region points into a live mmap owned by the
        // caller; KVM will read and write those bytes directly from the guest.
        unsafe {
            self.vm.set_user_memory_region(region).map_err(Error::Kvm)?;
        }
        Ok(MemorySlotHandle { slot })
    }

    /// Registers a [`LeakedSlice`] as a guest RAM slot at `guest_phys_addr`.
    ///
    /// Equivalent to [`register_ram_slot`](Self::register_ram_slot) but takes
    /// a `LeakedSlice` directly, matching how the `machine` crate owns its
    /// per-region RAM backings.
    pub fn register_ram_slot_leaked(
        &mut self,
        guest_phys_addr: u64,
        leaked: &mut LeakedSlice,
    ) -> Result<MemorySlotHandle, Error> {
        let slot = self.next_slot;
        self.next_slot = self.next_slot.checked_add(1).ok_or(Error::TooManySlots)?;

        let userspace_addr = leaked.as_mut_ptr() as u64;
        let region = kvm_userspace_memory_region {
            slot,
            flags: 0,
            guest_phys_addr,
            memory_size: leaked.len() as u64,
            userspace_addr,
        };

        // SAFETY: the leaked slice stays valid for its lifetime (the caller
        // keeps it alongside the VM), so KVM can safely read and write
        // those bytes directly from the guest.
        unsafe {
            self.vm.set_user_memory_region(region).map_err(Error::Kvm)?;
        }
        Ok(MemorySlotHandle { slot })
    }

    /// Creates a new vCPU with the given `id` (single-vCPU POC uses 0).
    pub fn create_vcpu(&self, id: u64) -> Result<KvmVcpu, Error> {
        let vcpu = self.vm.create_vcpu(id).map_err(Error::Kvm)?;
        Ok(KvmVcpu::new(vcpu, self.vcpu_mmap_size))
    }
}
