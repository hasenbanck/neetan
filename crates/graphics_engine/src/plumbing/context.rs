use std::{
    cell::{RefCell, RefMut},
    ffi::CStr,
};

use common::error;
use jay_ash::vk;

use super::memory::GpuAllocator;
use crate::plumbing::utils::vk_result_to_string;

/// The internal context.
pub(crate) struct Context {
    allocator: RefCell<GpuAllocator>,
    debug_utils_device: jay_ash::ext::debug_utils::Device,
    debug_utils_instance: Option<jay_ash::ext::debug_utils::Instance>,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    present_wait2: Option<jay_ash::khr::present_wait2::Device>,
    device: jay_ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: jay_ash::Instance,
    entry: jay_ash::Entry,
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);

            if let (Some(debug_utils), Some(messenger)) =
                (&self.debug_utils_instance, self.debug_messenger)
            {
                debug_utils.destroy_debug_utils_messenger(messenger, None);
            }

            self.instance.destroy_instance(None);
        };
    }
}

impl Context {
    /// Creates a new context.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        allocator: GpuAllocator,
        debug_utils_device: jay_ash::ext::debug_utils::Device,
        debug_utils_instance: Option<jay_ash::ext::debug_utils::Instance>,
        debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
        present_wait2: Option<jay_ash::khr::present_wait2::Device>,
        instance: jay_ash::Instance,
        device: jay_ash::Device,
        physical_device: vk::PhysicalDevice,
        entry: jay_ash::Entry,
    ) -> Self {
        Self {
            allocator: RefCell::new(allocator),
            debug_utils_device,
            debug_utils_instance,
            debug_messenger,
            present_wait2,
            device,
            physical_device,
            instance,
            entry,
        }
    }

    /// Sets a debug name for an object.
    pub(crate) fn set_object_name(&self, name: &CStr, object: impl vk::Handle) {
        let info = vk::DebugUtilsObjectNameInfoEXT::default()
            .object_name(name)
            .object_handle(object);

        if let Err(error) = unsafe { self.debug_utils_device.set_debug_utils_object_name(&info) } {
            error!(
                "Can't set object name: {error}",
                error = vk_result_to_string(error)
            );
        };
    }

    /// Returns a guard to the GPU allocator.
    #[inline(always)]
    pub(crate) fn allocator(&self) -> RefMut<'_, GpuAllocator> {
        self.allocator.borrow_mut()
    }

    /// Returns whether the VK_KHR_present_wait2 extension is available.
    #[inline(always)]
    pub(crate) fn supports_present_wait2(&self) -> bool {
        self.present_wait2.is_some()
    }

    /// Returns a reference to the VK_KHR_present_wait2 extension loader.
    ///
    /// Returns `None` if VK_KHR_present_wait2 is not available.
    #[inline(always)]
    pub(crate) fn present_wait2(&self) -> Option<&jay_ash::khr::present_wait2::Device> {
        self.present_wait2.as_ref()
    }

    /// Returns a reference to the Vulkan device.
    #[inline(always)]
    pub(crate) fn device(&self) -> &jay_ash::Device {
        &self.device
    }

    /// Returns a reference to the physical device.
    #[inline(always)]
    pub(crate) fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    /// Returns a reference to the Vulkan instance.
    #[inline(always)]
    pub(crate) fn instance(&self) -> &jay_ash::Instance {
        &self.instance
    }

    /// Returns a reference to the Vulkan entry point.
    #[inline(always)]
    pub(crate) fn entry(&self) -> &jay_ash::Entry {
        &self.entry
    }

    /// Returns a reference to the debug utils device extension loader.
    #[inline(always)]
    pub(crate) fn debug_utils_device(&self) -> &jay_ash::ext::debug_utils::Device {
        &self.debug_utils_device
    }
}
