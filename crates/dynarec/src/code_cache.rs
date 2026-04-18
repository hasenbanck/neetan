//! Platform-agnostic executable memory allocator for JIT code.
//!
//! The x86-64 backend emits compiled blocks into a bump-allocated RWX
//! region backed by `mmap` on Unix and `VirtualAlloc` on Windows. The
//! bytecode backend keeps IR in a plain `Vec<IrOp>` and does not use
//! this allocator.

use std::ptr::NonNull;

/// Default size for a single CPU's code cache (16 MiB, per design doc 4.3).
pub const DEFAULT_CACHE_SIZE: usize = 16 * 1024 * 1024;

/// A contiguous region of RWX memory backed by the host's anonymous
/// executable allocation API.
///
/// Allocation is bump-pointer. When full, call [`reset`](Self::reset) to
/// reclaim the whole region and recompile from scratch. No LRU, no free
/// list.
pub struct CodeCache {
    base: NonNull<u8>,
    capacity: usize,
    used: usize,
}

// SAFETY: the raw pointer is the exclusive owner of a private mapping; no
// aliasing invariants cross threads because `CodeCache` is only ever used
// from the owning `I386Jit`.
unsafe impl Send for CodeCache {}

impl CodeCache {
    /// Allocates a new code cache of the requested size, rounded up to a
    /// whole number of host pages.
    pub fn new(size: usize) -> Self {
        let page = host_page_size();
        let capacity = (size + page - 1) & !(page - 1);
        let base = platform::reserve_rwx(capacity);
        Self {
            base,
            capacity,
            used: 0,
        }
    }

    /// Returns the total capacity of the cache in bytes.
    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of bytes currently used.
    #[cfg(test)]
    pub fn used(&self) -> usize {
        self.used
    }

    /// Resets the bump pointer, effectively freeing all allocations.
    /// The underlying memory is retained.
    pub fn reset(&mut self) {
        self.used = 0;
    }

    /// Allocates `size` bytes aligned to `align`. Returns `None` when the
    /// request cannot be satisfied. The returned pointer is valid for
    /// writes and for subsequent execution after writes finish.
    pub fn alloc(&mut self, align: usize, size: usize) -> Option<NonNull<u8>> {
        debug_assert!(align.is_power_of_two());
        let mask = align - 1;
        let start = (self.used + mask) & !mask;
        let end = start.checked_add(size)?;
        if end > self.capacity {
            return None;
        }
        self.used = end;
        // SAFETY: start < capacity and base is a valid allocation.
        let ptr = unsafe { self.base.as_ptr().add(start) };
        NonNull::new(ptr)
    }
}

impl Drop for CodeCache {
    fn drop(&mut self) {
        platform::release_rwx(self.base, self.capacity);
    }
}

fn host_page_size() -> usize {
    platform::page_size()
}

#[cfg(unix)]
mod platform {
    use std::ptr::NonNull;

    const PROT_READ: i32 = 0x1;
    const PROT_WRITE: i32 = 0x2;
    const PROT_EXEC: i32 = 0x4;
    const MAP_PRIVATE: i32 = 0x02;
    #[cfg(target_os = "linux")]
    const MAP_ANONYMOUS: i32 = 0x20;
    #[cfg(not(target_os = "linux"))]
    const MAP_ANONYMOUS: i32 = 0x1000;
    const MAP_FAILED: *mut core::ffi::c_void = !0usize as *mut core::ffi::c_void;
    #[cfg(target_os = "linux")]
    const PAGE_SIZE_NAME: i32 = 30; // _SC_PAGESIZE
    #[cfg(not(target_os = "linux"))]
    const PAGE_SIZE_NAME: i32 = 29;

    unsafe extern "C" {
        fn mmap(
            addr: *mut core::ffi::c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut core::ffi::c_void;
        fn munmap(addr: *mut core::ffi::c_void, len: usize) -> i32;
        fn sysconf(name: i32) -> i64;
    }

    pub fn page_size() -> usize {
        // SAFETY: sysconf with a valid selector is safe.
        let value = unsafe { sysconf(PAGE_SIZE_NAME) };
        if value > 0 { value as usize } else { 4096 }
    }

    pub fn reserve_rwx(capacity: usize) -> NonNull<u8> {
        // SAFETY: standard mmap invocation; arguments are validated by the
        // kernel. We check the return value against MAP_FAILED below.
        let ptr = unsafe {
            mmap(
                core::ptr::null_mut(),
                capacity,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert!(ptr != MAP_FAILED, "dynarec: mmap(RWX) failed");
        NonNull::new(ptr as *mut u8).expect("dynarec: mmap returned null")
    }

    pub fn release_rwx(base: NonNull<u8>, capacity: usize) {
        // SAFETY: base/capacity came from a prior reserve_rwx call.
        unsafe {
            munmap(base.as_ptr().cast(), capacity);
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::ptr::NonNull;

    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_EXECUTE_READWRITE: u32 = 0x40;

    #[repr(C)]
    struct SystemInfo {
        processor_architecture: u16,
        reserved: u16,
        page_size: u32,
        minimum_application_address: *mut core::ffi::c_void,
        maximum_application_address: *mut core::ffi::c_void,
        active_processor_mask: usize,
        number_of_processors: u32,
        processor_type: u32,
        allocation_granularity: u32,
        processor_level: u16,
        processor_revision: u16,
    }

    unsafe extern "system" {
        fn VirtualAlloc(
            addr: *mut core::ffi::c_void,
            size: usize,
            alloc_type: u32,
            protect: u32,
        ) -> *mut core::ffi::c_void;
        fn VirtualFree(addr: *mut core::ffi::c_void, size: usize, free_type: u32) -> i32;
        fn GetSystemInfo(info: *mut SystemInfo);
    }

    pub fn page_size() -> usize {
        // SAFETY: GetSystemInfo fills a POD struct via out-pointer.
        let mut info = core::mem::MaybeUninit::<SystemInfo>::uninit();
        unsafe {
            GetSystemInfo(info.as_mut_ptr());
            info.assume_init().page_size as usize
        }
    }

    pub fn reserve_rwx(capacity: usize) -> NonNull<u8> {
        // SAFETY: VirtualAlloc with valid flags; return is checked.
        let ptr = unsafe {
            VirtualAlloc(
                core::ptr::null_mut(),
                capacity,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            )
        };
        assert!(!ptr.is_null(), "dynarec: VirtualAlloc(RWX) failed");
        NonNull::new(ptr as *mut u8).unwrap()
    }

    pub fn release_rwx(base: NonNull<u8>, _capacity: usize) {
        // SAFETY: base came from a prior VirtualAlloc.
        unsafe {
            VirtualFree(base.as_ptr().cast(), 0, MEM_RELEASE);
        }
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("dynarec: unsupported platform for CodeCache");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_and_reset() {
        let mut cache = CodeCache::new(64 * 1024);
        assert!(cache.capacity() >= 64 * 1024);

        let a = cache.alloc(16, 128).expect("first alloc");
        let b = cache.alloc(16, 256).expect("second alloc");
        assert_ne!(a.as_ptr(), b.as_ptr());
        assert_eq!(cache.used(), 128 + 256);

        // SAFETY: region is RWX, alloc returned a disjoint 128-byte slice.
        unsafe {
            core::ptr::write_bytes(a.as_ptr(), 0xAA, 128);
            core::ptr::write_bytes(b.as_ptr(), 0xBB, 256);
        }

        cache.reset();
        assert_eq!(cache.used(), 0);
        let c = cache.alloc(16, 32).expect("post-reset alloc");
        // After reset the bump pointer starts fresh at offset 0.
        assert_eq!(c.as_ptr(), a.as_ptr());
    }

    #[test]
    fn alloc_respects_alignment() {
        let mut cache = CodeCache::new(4096);
        let a = cache.alloc(1, 3).expect("three bytes at alignment 1");
        assert_eq!(a.as_ptr().align_offset(1), 0);
        let b = cache.alloc(64, 1).expect("aligned to 64");
        assert_eq!(b.as_ptr().align_offset(64), 0);
    }

    #[test]
    fn alloc_fails_when_full() {
        let mut cache = CodeCache::new(4096);
        let capacity = cache.capacity();
        let _ = cache.alloc(1, capacity).expect("alloc whole region");
        assert!(cache.alloc(1, 1).is_none());
    }
}
