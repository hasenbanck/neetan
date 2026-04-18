//! Top-level KVM handle (owns the `/dev/kvm` fd).

use kvm_bindings::CpuId;
use kvm_ioctls::Kvm;

use crate::{error::Error, vm::KvmVm};

/// Expected KVM API version (matches Linux `KVM_API_VERSION`).
const KVM_API_VERSION: i32 = 12;

/// Owns the `/dev/kvm` file descriptor and exposes VM creation.
pub struct KvmSystem {
    kvm: Kvm,
    vcpu_mmap_size: usize,
}

impl KvmSystem {
    /// Opens `/dev/kvm` and verifies the API version.
    pub fn open() -> Result<Self, Error> {
        let kvm = Kvm::new().map_err(Error::Kvm)?;
        let version = kvm.get_api_version();
        if version != KVM_API_VERSION {
            return Err(Error::ApiVersionMismatch { actual: version });
        }
        let vcpu_mmap_size = kvm.get_vcpu_mmap_size().map_err(Error::Kvm)?;
        Ok(Self {
            kvm,
            vcpu_mmap_size,
        })
    }

    /// Creates a new VM with no memory slots and no vCPUs.
    pub fn create_vm(&self) -> Result<KvmVm, Error> {
        let vm = self.kvm.create_vm().map_err(Error::Kvm)?;
        Ok(KvmVm::new(vm, self.vcpu_mmap_size))
    }

    /// Returns the size in bytes of the per-vCPU `kvm_run` mmap region.
    pub fn vcpu_mmap_size(&self) -> usize {
        self.vcpu_mmap_size
    }

    /// Returns the host-supported CPUID leaves, intended to be rewritten via
    /// [`pentium2_cpuid`](crate::pentium2_cpuid) and applied with
    /// [`KvmVcpu::set_cpuid2`](crate::KvmVcpu::set_cpuid2).
    pub fn supported_cpuid(&self) -> Result<CpuId, Error> {
        self.kvm
            .get_supported_cpuid(kvm_bindings::KVM_MAX_CPUID_ENTRIES)
            .map_err(Error::Kvm)
    }
}
