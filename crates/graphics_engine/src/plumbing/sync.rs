//! Synchronization primitives for Vulkan.

use std::{ffi::CStr, marker::PhantomData, rc::Rc};

use jay_ash::vk;

use super::context::Context;

pub(crate) struct Binary;
pub(crate) struct Timeline;

/// Binary or timeline semaphore for GPU-GPU and GPU-CPU synchronization.
pub(crate) struct Semaphore<T> {
    handle: vk::Semaphore,
    context: Rc<Context>,
    _marker: PhantomData<T>,
}

impl Semaphore<Binary> {
    /// Creates a new binary semaphore.
    pub(crate) fn new_binary(context: Rc<Context>, name: &CStr) -> Result<Self, vk::Result> {
        let create_info = vk::SemaphoreCreateInfo::default();
        let handle = unsafe { context.device().create_semaphore(&create_info, None)? };
        context.set_object_name(name, handle);
        Ok(Self {
            handle,
            context,
            _marker: PhantomData,
        })
    }
}

impl Semaphore<Timeline> {
    /// Creates a new timeline semaphore with an initial value.
    pub(crate) fn new_timeline(
        context: Rc<Context>,
        name: &CStr,
        initial_value: u64,
    ) -> Result<Self, vk::Result> {
        let mut type_create_info = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);

        let create_info = vk::SemaphoreCreateInfo::default().push_next(&mut type_create_info);
        let handle = unsafe { context.device().create_semaphore(&create_info, None)? };
        context.set_object_name(name, handle);

        Ok(Self {
            handle,
            context,
            _marker: PhantomData,
        })
    }
}

impl<T> Semaphore<T> {
    /// Returns the raw Vulkan semaphore handle.
    #[inline]
    pub(crate) fn handle(&self) -> vk::Semaphore {
        self.handle
    }

    /// Gets the current value of a timeline semaphore.
    pub(crate) fn get_value(&self) -> Result<u64, vk::Result> {
        unsafe {
            self.context
                .device()
                .get_semaphore_counter_value(self.handle)
        }
    }
}

impl<T> Drop for Semaphore<T> {
    fn drop(&mut self) {
        unsafe {
            self.context.device().destroy_semaphore(self.handle, None);
        }
    }
}

/// Fence for GPU-CPU synchronization.
pub(crate) struct Fence {
    handle: vk::Fence,
    context: Rc<Context>,
}

impl Fence {
    /// Creates a new fence.
    pub(crate) fn new(
        context: Rc<Context>,
        name: &CStr,
        signaled: bool,
    ) -> Result<Self, vk::Result> {
        let flags = if signaled {
            vk::FenceCreateFlags::SIGNALED
        } else {
            vk::FenceCreateFlags::empty()
        };

        let create_info = vk::FenceCreateInfo::default().flags(flags);
        let handle = unsafe { context.device().create_fence(&create_info, None)? };
        context.set_object_name(name, handle);

        Ok(Self { handle, context })
    }

    /// Returns the raw Vulkan fence handle.
    #[inline]
    pub(crate) fn handle(&self) -> vk::Fence {
        self.handle
    }

    /// Waits for the fence to be signaled.
    pub(crate) fn wait(&self, timeout: u64) -> Result<(), vk::Result> {
        unsafe {
            self.context
                .device()
                .wait_for_fences(&[self.handle], true, timeout)
        }
    }

    /// Resets the fence to the unsignaled state.
    pub(crate) fn reset(&self) -> Result<(), vk::Result> {
        unsafe { self.context.device().reset_fences(&[self.handle]) }
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        unsafe {
            self.context.device().destroy_fence(self.handle, None);
        }
    }
}
