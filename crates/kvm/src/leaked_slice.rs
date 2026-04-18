//! Page-aligned host-memory slice usable as a backend for guest RAM.
//!
//! The PC-98 machine bus holds its RAM via [`Pc9801Memory`](crate::HostMemory)
//! backends. Under the KVM backend we want the same bytes to be visible to
//! both the host-side `Pc9801Memory` accessors (for HLE handlers) and to the
//! KVM guest (executing against the same pages via
//! `KVM_SET_USER_MEMORY_REGION`).
//!
//! KVM requires `userspace_addr`, `guest_phys_addr`, and `memory_size` to be
//! page-aligned. `Box::leak` only gives 1-byte alignment, which makes KVM
//! reject the slot with `EINVAL`. This module instead backs each slice with
//! an anonymous `mmap`, which is naturally page-aligned and also gives
//! consistent behavior across dynamic lengths.

#[cfg(target_os = "linux")]
use memmap2::MmapMut;

/// A page-aligned slice of host memory whose backing is freed when the
/// value is dropped.
///
/// Machine code treats this as "a `Box<[u8]>` that happens to be page
/// aligned". The KVM crate passes its raw pointer into
/// `KVM_SET_USER_MEMORY_REGION` so the guest and the host read the same
/// bytes.
///
/// # Ownership
///
/// The caller must not construct two [`LeakedSlice`] values covering the
/// same memory. `&mut self`-method access relies on the pointer being the
/// sole handle to the underlying allocation. Call sites that want to expose
/// the same region to both KVM and a `LeakedSlice` are safe: KVM reads and
/// writes through a separate kernel-side mapping, but the userspace
/// accesses (from `as_mut_slice` / `as_mut_ptr`) flow through this handle.
#[cfg(target_os = "linux")]
pub struct LeakedSlice {
    mmap: MmapMut,
}

#[cfg(target_os = "linux")]
impl LeakedSlice {
    /// Allocates `len` zero-filled, page-aligned bytes via an anonymous
    /// `mmap`.
    pub fn new_zeroed(len: usize) -> Self {
        let mmap = MmapMut::map_anon(len)
            .expect("LeakedSlice::new_zeroed: anonymous mmap failed");
        Self { mmap }
    }

    /// Length of the backing slice in bytes.
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Whether the slice is empty.
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    /// Raw mutable pointer to the start of the slice. Page-aligned.
    ///
    /// Intended for use with `KVM_SET_USER_MEMORY_REGION`, which consumes a
    /// userspace-virtual address. The pointer stays valid until this value
    /// is dropped.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    /// Immutable slice view.
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }

    /// Mutable slice view.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.mmap
    }
}

// Non-Linux targets: stub that panics if instantiated. The `kvm` crate's
// feature gates prevent this from being reachable at runtime, but keeping
// the type present lets the `machine` crate use it in type positions.
#[cfg(not(target_os = "linux"))]
pub struct LeakedSlice {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(not(target_os = "linux"))]
impl LeakedSlice {
    pub fn new_zeroed(_len: usize) -> Self {
        panic!("LeakedSlice is only available on Linux");
    }

    pub fn len(&self) -> usize {
        0
    }

    pub fn is_empty(&self) -> bool {
        true
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        panic!("LeakedSlice is only available on Linux");
    }

    pub fn as_slice(&self) -> &[u8] {
        &[]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        panic!("LeakedSlice is only available on Linux");
    }
}
