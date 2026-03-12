use std::{ffi::CStr, ptr::NonNull, rc::Rc};

use common::Context as _;
use jay_ash::vk;

use super::memory::{self, MemoryBlock};
use crate::{Result, plumbing::Context};

/// A persistently mapped host-visible GPU buffer.
///
/// Allocates with `HOST_ACCESS | FAST_DEVICE_ACCESS | DEVICE_ADDRESS`,
/// falling back to host-visible heaps.
///
/// The entire buffer is mapped on construction and remains mapped until
/// drop.
///
/// This is optimized for modern GPUs with Resizable BAR enabled and
/// integrated graphic cards with shared memory architectures.
pub(crate) struct MappedBuffer {
    raw: vk::Buffer,
    memory_block: Option<MemoryBlock>,
    mapped_ptr: NonNull<u8>,
    byte_size: u64,
    context: Rc<Context>,
}

impl MappedBuffer {
    pub(crate) fn new(
        context: Rc<Context>,
        name: &CStr,
        usage: vk::BufferUsageFlags,
        byte_size: u64,
        min_alignment: Option<u64>,
    ) -> Result<Self> {
        let byte_size = byte_size.max(1);
        let buffer_usage = usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(byte_size)
            .usage(buffer_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let raw = unsafe {
            context
                .device()
                .create_buffer(&buffer_create_info, None)
                .context("Failed to create mapped buffer")?
        };

        let memory_requirements = unsafe { context.device().get_buffer_memory_requirements(raw) };
        let alloc_usage = memory::UsageFlags::HOST_ACCESS | memory::UsageFlags::FAST_DEVICE_ACCESS;

        let effective_alignment = min_alignment.map_or(memory_requirements.alignment, |align| {
            memory_requirements.alignment.max(align)
        });

        let request = memory::Request {
            size: memory_requirements.size,
            align_mask: effective_alignment - 1,
            usage: alloc_usage,
            memory_types: memory_requirements.memory_type_bits,
        };

        let mut memory_block = match unsafe { context.allocator().alloc(context.device(), request) }
        {
            Ok(block) => block,
            Err(error) => {
                unsafe { context.device().destroy_buffer(raw, None) };
                return Err(error).context("Failed to allocate mapped buffer memory")?;
            }
        };

        if let Err(error) = unsafe {
            context
                .device()
                .bind_buffer_memory(raw, *memory_block.memory(), memory_block.offset())
        } {
            unsafe {
                context.device().destroy_buffer(raw, None);
                context.allocator().dealloc(context.device(), memory_block);
            }
            return Err(error).context("Failed to bind mapped buffer memory")?;
        }

        let mapped_ptr = match unsafe { memory_block.map(context.device(), 0, byte_size as usize) }
        {
            Ok(ptr) => ptr,
            Err(error) => {
                unsafe {
                    context.device().destroy_buffer(raw, None);
                    context.allocator().dealloc(context.device(), memory_block);
                }
                common::bail!("Failed to map buffer: {error}");
            }
        };

        context.set_object_name(name, raw);

        Ok(Self {
            raw,
            memory_block: Some(memory_block),
            mapped_ptr,
            byte_size,
            context,
        })
    }

    /// Returns a mutable slice into the mapped buffer at the given byte offset and length.
    ///
    /// # Panics
    ///
    /// Panics if `offset + len` exceeds the buffer size.
    pub(crate) fn as_mut_slice_at(&mut self, offset: u64, len: usize) -> &mut [u8] {
        assert!(
            offset + len as u64 <= self.byte_size,
            "MappedBuffer write out of bounds: offset={offset}, len={len}, size={}",
            self.byte_size
        );
        unsafe {
            let ptr = self.mapped_ptr.as_ptr().add(offset as usize);
            std::slice::from_raw_parts_mut(ptr, len)
        }
    }

    /// Flushes a range of mapped memory to make CPU writes visible to the GPU.
    pub(crate) fn flush(&self, offset: u64, size: u64) {
        if let Some(ref memory_block) = self.memory_block {
            unsafe {
                let _ = memory_block.flush_range(self.context.device(), offset, size);
            }
        }
    }

    #[inline]
    pub(crate) fn raw(&self) -> vk::Buffer {
        self.raw
    }

    #[inline]
    pub(crate) fn byte_size(&self) -> u64 {
        self.byte_size
    }
}

impl Drop for MappedBuffer {
    fn drop(&mut self) {
        unsafe {
            if let Some(ref mut memory_block) = self.memory_block {
                memory_block.unmap(self.context.device());
            }
            self.context.device().destroy_buffer(self.raw, None);
            if let Some(memory_block) = self.memory_block.take() {
                self.context
                    .allocator()
                    .dealloc(self.context.device(), memory_block);
            }
        }
    }
}
