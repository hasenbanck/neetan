//! Queue abstraction for graphics operations.

use std::{
    cell::{RefCell, RefMut},
    ffi::CStr,
    rc::Rc,
};

use jay_ash::{prelude::VkResult, vk};

use super::{Context, command::CommandPool};

/// A generalized queue.
pub(crate) struct Queue {
    handle: Rc<RefCell<vk::Queue>>,
    family_index: u32,
    context: Rc<Context>,
}

impl Clone for Queue {
    fn clone(&self) -> Self {
        Self {
            handle: Rc::clone(&self.handle),
            family_index: self.family_index,
            context: Rc::clone(&self.context),
        }
    }
}

impl Queue {
    /// Creates a new queue.
    pub(crate) fn new(
        context: Rc<Context>,
        handle: vk::Queue,
        family_index: u32,
        name: &CStr,
    ) -> Self {
        context.set_object_name(name, handle);
        Self {
            handle: Rc::new(RefCell::new(handle)),
            family_index,
            context,
        }
    }
}

impl Queue {
    /// Locks and returns the raw Vulkan queue handle.
    #[inline]
    pub(crate) fn lock_handle(&self) -> RefMut<'_, vk::Queue> {
        self.handle.borrow_mut()
    }

    /// Returns the queue family index.
    #[inline]
    pub(crate) fn family_index(&self) -> u32 {
        self.family_index
    }

    /// Creates a command pool for this queue.
    pub(crate) fn create_command_pool(&self, name: &CStr) -> crate::Result<CommandPool> {
        CommandPool::new(Rc::clone(&self.context), name, self)
    }

    /// Submits command buffers to the queue using synchronization2.
    pub(crate) fn submit(&self, submits: &[vk::SubmitInfo2], fence: vk::Fence) -> VkResult<()> {
        let handle = self.lock_handle();
        unsafe { self.context.device().queue_submit2(*handle, submits, fence) }
    }
}
