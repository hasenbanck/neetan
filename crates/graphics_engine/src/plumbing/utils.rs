use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    str::FromStr,
};

use common::{debug, error, info, warn};
use jay_ash::vk;

/// Converts a Vulkan Result error code to a human-readable string.
pub(crate) fn vk_result_to_string(result: vk::Result) -> Cow<'static, str> {
    match result {
        vk::Result::SUCCESS => Cow::Borrowed("Success"),
        vk::Result::NOT_READY => Cow::Borrowed("Not ready"),
        vk::Result::TIMEOUT => Cow::Borrowed("Timeout"),
        vk::Result::EVENT_SET => Cow::Borrowed("Event set"),
        vk::Result::EVENT_RESET => Cow::Borrowed("Event reset"),
        vk::Result::INCOMPLETE => Cow::Borrowed("Incomplete"),
        vk::Result::ERROR_OUT_OF_HOST_MEMORY => Cow::Borrowed("Out of host memory"),
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => Cow::Borrowed("Out of device memory"),
        vk::Result::ERROR_INITIALIZATION_FAILED => Cow::Borrowed("Initialization failed"),
        vk::Result::ERROR_DEVICE_LOST => Cow::Borrowed("Device lost"),
        vk::Result::ERROR_MEMORY_MAP_FAILED => Cow::Borrowed("Memory map failed"),
        vk::Result::ERROR_LAYER_NOT_PRESENT => Cow::Borrowed("Layer not present"),
        vk::Result::ERROR_EXTENSION_NOT_PRESENT => Cow::Borrowed("Extension not present"),
        vk::Result::ERROR_FEATURE_NOT_PRESENT => Cow::Borrowed("Feature not present"),
        vk::Result::ERROR_INCOMPATIBLE_DRIVER => Cow::Borrowed("Incompatible driver"),
        vk::Result::ERROR_TOO_MANY_OBJECTS => Cow::Borrowed("Too many objects"),
        vk::Result::ERROR_FORMAT_NOT_SUPPORTED => Cow::Borrowed("Format not supported"),
        vk::Result::ERROR_FRAGMENTED_POOL => Cow::Borrowed("Fragmented pool"),
        vk::Result::ERROR_UNKNOWN => Cow::Borrowed("Unknown error"),
        vk::Result::ERROR_OUT_OF_POOL_MEMORY => Cow::Borrowed("Out of pool memory"),
        vk::Result::ERROR_INVALID_EXTERNAL_HANDLE => Cow::Borrowed("Invalid external handle"),
        vk::Result::ERROR_FRAGMENTATION => Cow::Borrowed("Fragmentation"),
        vk::Result::ERROR_INVALID_OPAQUE_CAPTURE_ADDRESS => {
            Cow::Borrowed("Invalid opaque capture address")
        }
        vk::Result::ERROR_SURFACE_LOST_KHR => Cow::Borrowed("Surface lost"),
        vk::Result::ERROR_NATIVE_WINDOW_IN_USE_KHR => Cow::Borrowed("Native window in use"),
        vk::Result::SUBOPTIMAL_KHR => Cow::Borrowed("Suboptimal"),
        vk::Result::ERROR_OUT_OF_DATE_KHR => Cow::Borrowed("Out of date"),
        vk::Result::ERROR_INCOMPATIBLE_DISPLAY_KHR => Cow::Borrowed("Incompatible display"),
        vk::Result::ERROR_VALIDATION_FAILED_EXT => Cow::Borrowed("Validation failed"),
        vk::Result::ERROR_INVALID_SHADER_NV => Cow::Borrowed("Invalid shader"),
        vk::Result::ERROR_INVALID_DRM_FORMAT_MODIFIER_PLANE_LAYOUT_EXT => {
            Cow::Borrowed("Invalid DRM format modifier plane layout")
        }
        vk::Result::ERROR_NOT_PERMITTED_EXT => Cow::Borrowed("Not permitted"),
        vk::Result::ERROR_FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT => {
            Cow::Borrowed("Full screen exclusive mode lost")
        }
        vk::Result::THREAD_IDLE_KHR => Cow::Borrowed("Thread idle"),
        vk::Result::THREAD_DONE_KHR => Cow::Borrowed("Thread done"),
        vk::Result::OPERATION_DEFERRED_KHR => Cow::Borrowed("Operation deferred"),
        vk::Result::OPERATION_NOT_DEFERRED_KHR => Cow::Borrowed("Operation not deferred"),
        vk::Result::PIPELINE_COMPILE_REQUIRED_EXT => Cow::Borrowed("Pipeline compile required"),
        _ => Cow::Owned(format!("Unknown Vulkan result code: {result}")),
    }
}

/// Formats a Vulkan API version as a human-readable string (e.g., "1.4.0").
pub(crate) fn format_api_version(version: u32) -> String {
    format!(
        "{major}.{minor}.{patch}",
        major = vk::api_version_major(version),
        minor = vk::api_version_minor(version),
        patch = vk::api_version_patch(version)
    )
}

pub(crate) unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    // Remove noisy layer initialization.
    if message_type == vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
        && message_severity == vk::DebugUtilsMessageSeverityFlagsEXT::INFO
    {
        return vk::FALSE;
    }

    let callback_data = unsafe { *p_callback_data };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        unsafe { CStr::from_ptr(callback_data.p_message).to_string_lossy() }
    };

    match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => {
            error!("{message}")
        }
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => {
            warn!("{message}")
        }
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => {
            info!("{message}")
        }
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => {
            debug!("{message}")
        }
        _ => {
            warn!("{message}");
        }
    }

    vk::FALSE
}

pub(crate) trait IntoCString {
    fn into_cstring(self) -> CString;
}

impl IntoCString for String {
    fn into_cstring(self) -> CString {
        use std::ffi::CString;
        CString::from_str(&self).expect("invalid CString")
    }
}
