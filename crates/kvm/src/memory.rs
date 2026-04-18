//! Host-side guest memory backing.

use memmap2::MmapMut;

use crate::error::Error;

/// A single contiguous anonymous host mmap that backs the guest physical
/// address space.
///
/// The [`KvmVm`](crate::KvmVm) registers regions within this buffer as guest
/// memory slots via `KVM_SET_USER_MEMORY_REGION`. The guest reads and writes
/// those bytes at native speed; host code reads and writes the same bytes
/// via [`as_slice`](Self::as_slice) / [`as_mut_slice`](Self::as_mut_slice).
pub struct HostMemory {
    mmap: MmapMut,
}

impl HostMemory {
    /// Allocates `size` bytes of anonymous, zero-initialized host memory.
    pub fn new(size: usize) -> Result<Self, Error> {
        let mmap = MmapMut::map_anon(size).map_err(Error::Memory)?;
        Ok(Self { mmap })
    }

    /// Total size of the backing buffer in bytes.
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Whether the buffer has zero length.
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    /// Returns a raw mutable pointer to the first byte of the buffer.
    ///
    /// The pointer is valid for the lifetime of `self` and points at writable
    /// memory of [`len`](Self::len) bytes.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    /// Borrows the buffer immutably.
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }

    /// Borrows the buffer mutably.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.mmap
    }
}

/// Handle returned by [`KvmVm::register_ram_slot`](crate::KvmVm::register_ram_slot).
///
/// Identifies a previously registered memory slot so it can be updated or
/// unregistered later. Slot numbers are assigned sequentially starting at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySlotHandle {
    pub(crate) slot: u32,
}

impl MemorySlotHandle {
    /// Returns the KVM slot index.
    pub fn slot(self) -> u32 {
        self.slot
    }
}
