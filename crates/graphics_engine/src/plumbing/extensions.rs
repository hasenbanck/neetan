//! Vulkan device extension management.

use std::ffi::CStr;

use common::{Context, bail};
use jay_ash::vk;

use crate::Result;

/// Tracks which optional device extensions are enabled.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExtensionSet {
    /// Whether VK_EXT_descriptor_heap is enabled.
    pub(crate) descriptor_heap: bool,
    /// Whether VK_KHR_present_id2 is enabled.
    pub(crate) present_id2: bool,
    /// Whether VK_KHR_present_wait2 is enabled.
    pub(crate) present_wait2: bool,
}

/// Builds the list of device extensions to enable.
//
/// # Arguments
///
/// * `instance` - The Vulkan instance.
/// * `physical_device` - The physical device to query extensions for.
/// * `extensions` - Requested extensions.
///
/// # Returns
///
/// A tuple of (extension name pointers, ExtensionSet) indicating which extensions were enabled.
///
/// # Errors
///
/// Returns an error if any requested extension is not available.
pub(crate) fn build_extension_list(
    instance: &jay_ash::Instance,
    physical_device: vk::PhysicalDevice,
    extensions: Vec<*const i8>,
) -> Result<(Vec<*const i8>, ExtensionSet)> {
    let available_extensions = unsafe {
        instance
            .enumerate_device_extension_properties(physical_device)
            .context("Failed to enumerate device extensions")?
    };

    validate_extensions(&available_extensions, &extensions)?;

    let extension_set = ExtensionSet {
        descriptor_heap: extension_in_list(&extensions, vk::EXT_DESCRIPTOR_HEAP_EXTENSION_NAME),
        present_id2: extension_in_list(&extensions, vk::KHR_PRESENT_ID_2_EXTENSION_NAME),
        present_wait2: extension_in_list(&extensions, vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME),
    };

    Ok((extensions, extension_set))
}

/// Validates that all requested extensions are available on the device.
///
/// Returns an error if any requested extension is not available.
fn validate_extensions(
    available_extensions: &[vk::ExtensionProperties],
    requested_extensions: &[*const i8],
) -> Result<()> {
    for &ext_ptr in requested_extensions {
        let extension_name = unsafe { CStr::from_ptr(ext_ptr) };
        let found = available_extensions.iter().any(|ext| {
            let available_name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            available_name == extension_name
        });

        if !found {
            bail!(
                "required extension {extension_name} not available",
                extension_name = extension_name.to_string_lossy()
            );
        }
    }

    Ok(())
}

/// Checks if an extension is in the extension list.
fn extension_in_list(extensions: &[*const i8], extension_name: &CStr) -> bool {
    extensions.iter().any(|&ext_ptr| {
        let name = unsafe { CStr::from_ptr(ext_ptr) };
        name == extension_name
    })
}

/// Checks if a device extension is available on the physical device.
pub(crate) fn is_extension_available(
    instance: &jay_ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension_name: &CStr,
) -> bool {
    let available = unsafe {
        instance
            .enumerate_device_extension_properties(physical_device)
            .unwrap_or_default()
    };

    available.iter().any(|ext| {
        let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
        name == extension_name
    })
}
