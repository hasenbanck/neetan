use std::{
    fmt::{self, Display},
    ptr::NonNull,
};

use jay_ash::{Device, Instance, vk};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct UsageFlags(u8);

impl UsageFlags {
    pub(crate) const FAST_DEVICE_ACCESS: Self = Self(0x01);
    pub(crate) const HOST_ACCESS: Self = Self(0x02);
    pub(crate) const DOWNLOAD: Self = Self(0x04);
    pub(crate) const UPLOAD: Self = Self(0x08);

    pub(crate) const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & 0x0F)
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub(crate) const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub(crate) const fn bits(self) -> u8 {
        self.0
    }
}

impl std::ops::BitOr for UsageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AllocationError {
    OutOfDeviceMemory,
    OutOfHostMemory,
    NoCompatibleMemoryTypes,
    TooManyObjects,
}

impl Display for AllocationError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocationError::OutOfDeviceMemory => fmt.write_str("Device memory exhausted"),
            AllocationError::OutOfHostMemory => fmt.write_str("Host memory exhausted"),
            AllocationError::NoCompatibleMemoryTypes => fmt.write_str(
                "No compatible memory types from requested types support requested usage",
            ),
            AllocationError::TooManyObjects => {
                fmt.write_str("Reached limit on allocated memory objects count")
            }
        }
    }
}

impl std::error::Error for AllocationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MapError {
    OutOfDeviceMemory,
    OutOfHostMemory,
    NonHostVisible,
    MapFailed,
    AlreadyMapped,
}

impl Display for MapError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::OutOfDeviceMemory => fmt.write_str("Device memory exhausted"),
            MapError::OutOfHostMemory => fmt.write_str("Host memory exhausted"),
            MapError::MapFailed => fmt.write_str("Failed to map memory object"),
            MapError::NonHostVisible => fmt.write_str("Impossible to map non-host-visible memory"),
            MapError::AlreadyMapped => fmt.write_str("Block is already mapped"),
        }
    }
}

impl std::error::Error for MapError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Request {
    pub(crate) size: u64,
    pub(crate) align_mask: u64,
    pub(crate) usage: UsageFlags,
    pub(crate) memory_types: u32,
}

struct Relevant;

impl Drop for Relevant {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        eprintln!("Memory block wasn't deallocated");
    }
}

pub(crate) struct MemoryBlock {
    memory: vk::DeviceMemory,
    props: vk::MemoryPropertyFlags,
    size: u64,
    atom_mask: u64,
    mapped: bool,
    relevant: Relevant,
}

unsafe impl Sync for MemoryBlock {}
unsafe impl Send for MemoryBlock {}

impl MemoryBlock {
    #[inline(always)]
    pub(crate) fn memory(&self) -> &vk::DeviceMemory {
        &self.memory
    }

    #[inline(always)]
    pub(crate) fn offset(&self) -> u64 {
        0
    }

    /// Maps a range of the memory block to a host-visible pointer.
    ///
    /// # Safety
    ///
    /// The block must have been allocated from the specified `device`.
    #[inline(always)]
    pub(crate) unsafe fn map(
        &mut self,
        device: &Device,
        offset: u64,
        size: usize,
    ) -> Result<NonNull<u8>, MapError> {
        if !self.props.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
            return Err(MapError::NonHostVisible);
        }

        let size_u64 = size as u64;
        assert!(offset < self.size, "`offset` is out of memory block bounds");
        assert!(
            size_u64 <= self.size - offset,
            "`offset + size` is out of memory block bounds"
        );

        if self.mapped {
            return Err(MapError::AlreadyMapped);
        }
        self.mapped = true;

        let end = align_up(offset + size_u64, self.atom_mask)
            .expect("mapping end doesn't fit device address space");
        let aligned_offset = align_down(offset, self.atom_mask);

        let result = unsafe {
            device
                .map_memory(
                    self.memory,
                    aligned_offset,
                    end - aligned_offset,
                    vk::MemoryMapFlags::empty(),
                )
                .map(|ptr| {
                    NonNull::new(ptr as *mut u8)
                        .expect("Pointer to memory mapping must not be null")
                })
                .map_err(|err| match err {
                    vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => MapError::OutOfDeviceMemory,
                    vk::Result::ERROR_OUT_OF_HOST_MEMORY => MapError::OutOfHostMemory,
                    vk::Result::ERROR_MEMORY_MAP_FAILED => MapError::MapFailed,
                    err => panic!("Unexpected Vulkan error: `{}`", err),
                })
        };

        match result {
            Ok(ptr) => {
                let ptr_offset = (offset - aligned_offset) as isize;
                Ok(unsafe { NonNull::new_unchecked(ptr.as_ptr().offset(ptr_offset)) })
            }
            Err(err) => {
                self.mapped = false;
                Err(err)
            }
        }
    }

    /// Unmaps previously mapped memory.
    ///
    /// # Safety
    ///
    /// The block must have been allocated from the specified `device`.
    #[inline(always)]
    pub(crate) unsafe fn unmap(&mut self, device: &Device) -> bool {
        if !self.mapped {
            return false;
        }
        self.mapped = false;
        unsafe { device.unmap_memory(self.memory) };
        true
    }

    /// Flushes a range of mapped non-coherent memory to make CPU writes visible to the GPU.
    ///
    /// # Safety
    ///
    /// The block must have been allocated from the specified `device`.
    /// The memory must be currently mapped and the range must be within mapped bounds.
    #[inline(always)]
    pub(crate) unsafe fn flush_range(
        &self,
        device: &Device,
        offset: u64,
        size: u64,
    ) -> Result<(), MapError> {
        if self.props.contains(vk::MemoryPropertyFlags::HOST_COHERENT) || size == 0 {
            return Ok(());
        }

        let aligned_offset = align_down(offset, self.atom_mask);
        let end = align_up(offset + size, self.atom_mask).ok_or(MapError::OutOfHostMemory)?;

        unsafe {
            device
                .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(self.memory)
                    .offset(aligned_offset)
                    .size(end - aligned_offset)])
                .map_err(|err| match err {
                    vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => MapError::OutOfDeviceMemory,
                    vk::Result::ERROR_OUT_OF_HOST_MEMORY => MapError::OutOfHostMemory,
                    error => panic!("Unexpected Vulkan error: `{}`", error),
                })
        }
    }

    fn deallocate(self) -> vk::DeviceMemory {
        std::mem::forget(self.relevant);
        self.memory
    }
}

#[derive(Clone, Copy)]
struct MemoryTypeInfo {
    heap: u32,
    props: vk::MemoryPropertyFlags,
}

#[derive(Clone, Copy)]
struct MemoryForOneUsage {
    mask: u32,
    types: [u32; 32],
    types_count: u32,
}

struct MemoryForUsage {
    usages: [MemoryForOneUsage; 64],
}

impl MemoryForUsage {
    fn new(memory_types: &[MemoryTypeInfo]) -> Self {
        assert!(
            memory_types.len() <= 32,
            "Only up to 32 memory types supported"
        );

        let mut mfu = MemoryForUsage {
            usages: [MemoryForOneUsage {
                mask: 0,
                types: [0; 32],
                types_count: 0,
            }; 64],
        };

        for usage in 0..64 {
            mfu.usages[usage as usize] =
                one_usage(UsageFlags::from_bits_truncate(usage), memory_types);
        }

        mfu
    }

    fn mask(&self, usage: UsageFlags) -> u32 {
        self.usages[usage.bits() as usize].mask
    }

    fn types(&self, usage: UsageFlags) -> &[u32] {
        let usage = &self.usages[usage.bits() as usize];
        &usage.types[..usage.types_count as usize]
    }
}

fn one_usage(usage: UsageFlags, memory_types: &[MemoryTypeInfo]) -> MemoryForOneUsage {
    let mut types = [0; 32];
    let mut types_count = 0;

    for (index, mt) in memory_types.iter().enumerate() {
        if compatible(usage, mt.props) {
            types[types_count as usize] = index as u32;
            types_count += 1;
        }
    }

    types[..types_count as usize]
        .sort_unstable_by_key(|&index| reverse_priority(usage, memory_types[index as usize].props));

    let mask = types[..types_count as usize]
        .iter()
        .fold(0u32, |mask, index| mask | 1u32 << index);

    MemoryForOneUsage {
        mask,
        types,
        types_count,
    }
}

fn compatible(usage: UsageFlags, flags: vk::MemoryPropertyFlags) -> bool {
    if flags.contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED)
        || flags.contains(vk::MemoryPropertyFlags::PROTECTED)
    {
        false
    } else if usage.intersects(UsageFlags::HOST_ACCESS | UsageFlags::UPLOAD | UsageFlags::DOWNLOAD)
    {
        flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
    } else {
        true
    }
}

fn reverse_priority(usage: UsageFlags, flags: vk::MemoryPropertyFlags) -> u32 {
    let device_local: bool = flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        ^ (usage.is_empty() || usage.contains(UsageFlags::FAST_DEVICE_ACCESS));

    let host_visible: bool = flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
        ^ usage.intersects(UsageFlags::HOST_ACCESS | UsageFlags::UPLOAD | UsageFlags::DOWNLOAD);

    let host_cached: bool =
        flags.contains(vk::MemoryPropertyFlags::HOST_CACHED) ^ usage.contains(UsageFlags::DOWNLOAD);

    let host_coherent: bool = flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        ^ (usage.intersects(UsageFlags::UPLOAD | UsageFlags::DOWNLOAD));

    device_local as u32 * 8
        + host_visible as u32 * 4
        + host_cached as u32 * 2
        + host_coherent as u32
}

fn with_implicit_usage_flags(usage: UsageFlags) -> UsageFlags {
    if usage.is_empty() {
        UsageFlags::FAST_DEVICE_ACCESS
    } else if usage.intersects(UsageFlags::DOWNLOAD | UsageFlags::UPLOAD) {
        usage | UsageFlags::HOST_ACCESS
    } else {
        usage
    }
}

pub(crate) struct GpuAllocator {
    memory_for_usage: MemoryForUsage,
    memory_types: Box<[MemoryTypeInfo]>,
    memory_heap_sizes: Box<[u64]>,
    max_memory_allocation_size: u64,
    non_coherent_atom_size: u64,
    allocations_remains: u32,
}

impl GpuAllocator {
    /// Creates a new allocator by querying device memory properties.
    ///
    /// # Safety
    ///
    /// `physical_device` must have been queried from the given `instance`.
    pub(crate) unsafe fn new(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self, vk::Result> {
        let limits = unsafe {
            instance
                .get_physical_device_properties(physical_device)
                .limits
        };

        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };

        let memory_types: Box<[MemoryTypeInfo]> = memory_properties.memory_types
            [..memory_properties.memory_type_count as usize]
            .iter()
            .map(|mt| MemoryTypeInfo {
                props: mt.property_flags,
                heap: mt.heap_index,
            })
            .collect();

        let memory_heap_sizes: Box<[u64]> = memory_properties.memory_heaps
            [..memory_properties.memory_heap_count as usize]
            .iter()
            .map(|heap| heap.size)
            .collect();

        let non_coherent_atom_size = limits.non_coherent_atom_size;
        assert!(
            non_coherent_atom_size.is_power_of_two(),
            "`non_coherent_atom_size` must be power of two"
        );

        let memory_for_usage = MemoryForUsage::new(&memory_types);

        Ok(GpuAllocator {
            memory_for_usage,
            memory_types,
            memory_heap_sizes,
            max_memory_allocation_size: u64::MAX,
            non_coherent_atom_size,
            allocations_remains: limits.max_memory_allocation_count,
        })
    }

    /// Allocates a dedicated memory block.
    ///
    /// # Safety
    ///
    /// `device` must be the device associated with the physical device used to create this allocator.
    pub(crate) unsafe fn alloc(
        &mut self,
        device: &Device,
        mut request: Request,
    ) -> Result<MemoryBlock, AllocationError> {
        request.usage = with_implicit_usage_flags(request.usage);

        if request.size > self.max_memory_allocation_size {
            return Err(AllocationError::OutOfDeviceMemory);
        }

        if self.allocations_remains == 0 {
            return Err(AllocationError::TooManyObjects);
        }

        if 0 == self.memory_for_usage.mask(request.usage) & request.memory_types {
            return Err(AllocationError::NoCompatibleMemoryTypes);
        }

        for &index in self.memory_for_usage.types(request.usage) {
            if 0 == request.memory_types & (1 << index) {
                continue;
            }

            let memory_type = &self.memory_types[index as usize];
            let heap_size = self.memory_heap_sizes[memory_type.heap as usize];

            if request.size > heap_size {
                continue;
            }

            let atom_mask = if memory_type
                .props
                .contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
                && !memory_type
                    .props
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
            {
                self.non_coherent_atom_size - 1
            } else {
                0
            };

            let info = vk::MemoryAllocateInfo::default()
                .allocation_size(request.size)
                .memory_type_index(index);

            let result = unsafe {
                device
                    .allocate_memory(&info, None)
                    .map_err(|err| match err {
                        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => {
                            AllocationError::OutOfDeviceMemory
                        }
                        vk::Result::ERROR_OUT_OF_HOST_MEMORY => AllocationError::OutOfHostMemory,
                        vk::Result::ERROR_TOO_MANY_OBJECTS => AllocationError::TooManyObjects,
                        error => panic!("Unexpected Vulkan error: `{error}`"),
                    })
            };

            match result {
                Ok(memory) => {
                    self.allocations_remains -= 1;
                    return Ok(MemoryBlock {
                        memory,
                        props: memory_type.props,
                        size: request.size,
                        atom_mask,
                        mapped: false,
                        relevant: Relevant,
                    });
                }
                Err(AllocationError::OutOfDeviceMemory) => continue,
                Err(error) => return Err(error),
            }
        }

        Err(AllocationError::OutOfDeviceMemory)
    }

    /// Deallocates a memory block previously allocated from this allocator.
    ///
    /// # Safety
    ///
    /// `device` must be the device associated with the physical device used to create this allocator.
    /// `block` must have been allocated by this allocator.
    pub(crate) unsafe fn dealloc(&mut self, device: &Device, block: MemoryBlock) {
        let memory = block.deallocate();
        unsafe { device.free_memory(memory, None) };
        self.allocations_remains += 1;
    }
}

fn align_up(value: u64, align_mask: u64) -> Option<u64> {
    Some(value.checked_add(align_mask)? & !align_mask)
}

fn align_down(value: u64, align_mask: u64) -> u64 {
    value & !align_mask
}
