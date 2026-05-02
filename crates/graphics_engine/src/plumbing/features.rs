//! Vulkan physical device feature management for Vulkan 1.0 plus extensions.

use common::{bail, error, info, warn};
use jay_ash::vk;

use crate::{Result, plumbing::extensions::ExtensionSet};

/// Configuration for which device features to request.
///
/// Features are organized by Vulkan 1.0 core and by extension.
/// Only features that are both requested AND available will be enabled.
#[derive(Default, Clone)]
pub(crate) struct DeviceFeatures {
    /// Vulkan 1.0 core features.
    pub(crate) features_1_0: vk::PhysicalDeviceFeatures,
    /// VK_KHR_timeline_semaphore features.
    pub(crate) timeline_semaphore: Option<vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR<'static>>,
    /// VK_KHR_present_id2 features.
    pub(crate) present_id2: Option<vk::PhysicalDevicePresentId2FeaturesKHR<'static>>,
    /// VK_KHR_present_wait2 features.
    pub(crate) present_wait2: Option<vk::PhysicalDevicePresentWait2FeaturesKHR<'static>>,
}

/// Macro to check and log device features.
macro_rules! impl_feature_check {
    (
        $requested:expr, $available:expr, $requested_list:expr, $available_list:expr;
        {
            $($name:ident)*
        }
    ) => {
        $(
            if $available.$name == vk::TRUE {
                $available_list.push(stringify!($name));
            }
            if $requested.$name == vk::TRUE {
                $requested_list.push(stringify!($name));
            }
        )*
    };
}

/// Macro to collect requested device features when the containing extension is unavailable.
macro_rules! impl_requested_feature_check {
    (
        $requested:expr, $requested_list:expr;
        {
            $($name:ident)*
        }
    ) => {
        $(
            if $requested.$name == vk::TRUE {
                $requested_list.push(stringify!($name));
            }
        )*
    };
}

/// Initializes extension features based on ExtensionSet flags.
macro_rules! init_extension_features {
    ($extension_set:expr; $($field:ident => $type:ty),* $(,)?) => {
        $(
            let mut $field = if $extension_set.$field {
                Some(<$type>::default())
            } else {
                None
            };
        )*
    };
}

/// Links extension features into the Vulkan p_next chain.
macro_rules! chain_extension_features {
    ($features2:expr; $($ext_var:expr),* $(,)?) => {
        $(
            if let Some(ref mut ext_features) = $ext_var {
                ext_features.p_next = $features2.p_next;
                $features2.p_next = ext_features as *mut _ as *mut std::ffi::c_void;
            }
        )*
    };
}

/// Validates and logs extension features with error/warning handling.
macro_rules! validate_extension_features {
    (
        $requested:expr, $available:expr, $missing_features:expr;
        $(
            $field:ident: $ext_name:expr, $is_error:expr => { $($feature:ident)* }
        ),* $(,)?
    ) => {
        $(
            {
                let mut requested_list: Vec<&str> = Vec::new();
                let mut available_list: Vec<&str> = Vec::new();

                if let (Some(requested_ext), Some(available_ext)) =
                    (&$requested.$field, &$available.$field)
                {
                    impl_feature_check!(
                        requested_ext, available_ext, requested_list, available_list;
                        { $($feature)* }
                    );
                } else if let Some(requested_ext) = &$requested.$field {
                    impl_requested_feature_check!(
                        requested_ext, requested_list;
                        { $($feature)* }
                    );
                }

                log_and_validate_features(
                    &requested_list,
                    &available_list,
                    $ext_name,
                    &mut $missing_features,
                    $is_error,
                );
            }
        )*
    };
}

/// Queries all available physical device features.
pub(crate) fn query_physical_device_features(
    physical_device_properties2: &jay_ash::khr::get_physical_device_properties2::Instance,
    physical_device: vk::PhysicalDevice,
    extension_set: &ExtensionSet,
) -> DeviceFeatures {
    init_extension_features! {
        extension_set;
        timeline_semaphore => vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR,
        present_id2 => vk::PhysicalDevicePresentId2FeaturesKHR,
        present_wait2 => vk::PhysicalDevicePresentWait2FeaturesKHR,
    }

    let mut features2 = vk::PhysicalDeviceFeatures2KHR::default();

    chain_extension_features! {
        features2;
        timeline_semaphore,
        present_id2,
        present_wait2,
    }

    unsafe {
        physical_device_properties2.get_physical_device_features2(physical_device, &mut features2);
    }

    DeviceFeatures {
        features_1_0: features2.features,
        timeline_semaphore,
        present_id2,
        present_wait2,
    }
}

/// Validates that requested features are available and prepares them for device creation.
pub(crate) fn validate_features(
    requested: &DeviceFeatures,
    available: &DeviceFeatures,
) -> Result<()> {
    let mut missing_features = false;

    let mut requested_1_0_list: Vec<&str> = Vec::new();
    let mut available_1_0_list: Vec<&str> = Vec::new();

    impl_feature_check!(
        requested.features_1_0, available.features_1_0, requested_1_0_list, available_1_0_list;
        {
            robust_buffer_access full_draw_index_uint32 image_cube_array independent_blend
            geometry_shader tessellation_shader sample_rate_shading dual_src_blend logic_op
            multi_draw_indirect draw_indirect_first_instance depth_clamp depth_bias_clamp
            fill_mode_non_solid depth_bounds wide_lines large_points alpha_to_one multi_viewport
            sampler_anisotropy texture_compression_etc2 texture_compression_astc_ldr
            texture_compression_bc occlusion_query_precise pipeline_statistics_query
            vertex_pipeline_stores_and_atomics fragment_stores_and_atomics
            shader_tessellation_and_geometry_point_size shader_image_gather_extended
            shader_storage_image_extended_formats shader_storage_image_multisample
            shader_storage_image_read_without_format shader_storage_image_write_without_format
            shader_uniform_buffer_array_dynamic_indexing shader_sampled_image_array_dynamic_indexing
            shader_storage_buffer_array_dynamic_indexing shader_storage_image_array_dynamic_indexing
            shader_clip_distance shader_cull_distance shader_float64 shader_int64 shader_int16
            shader_resource_residency shader_resource_min_lod sparse_binding sparse_residency_buffer
            sparse_residency_image2_d sparse_residency_image3_d sparse_residency2_samples
            sparse_residency4_samples sparse_residency8_samples sparse_residency16_samples
            sparse_residency_aliased variable_multisample_rate inherited_queries
        }
    );

    log_and_validate_features(
        &requested_1_0_list,
        &available_1_0_list,
        "Vulkan 1.0",
        &mut missing_features,
        true,
    );

    validate_extension_features! {
        requested, available, missing_features;
        timeline_semaphore: "VK_KHR_timeline_semaphore", true => { timeline_semaphore },
        present_wait2: "VK_KHR_present_wait2", false => { present_wait2 },
        present_id2: "VK_KHR_present_id2", false => { present_id2 },
    }

    if missing_features {
        bail!("one or more requested device features are not available");
    }

    Ok(())
}

fn log_and_validate_features(
    requested_list: &[&str],
    available_list: &[&str],
    feature_set_name: &str,
    missing_features: &mut bool,
    missing_is_error: bool,
) {
    if requested_list.is_empty() {
        return;
    }

    info!("{feature_set_name} features:");
    for feature in requested_list.iter() {
        if available_list.contains(feature) {
            info!("  - {feature}");
        } else if missing_is_error {
            error!("  - {feature} (requested but not available)");
            *missing_features = true;
        } else {
            warn!("  - {feature} (requested but not available)");
        }
    }
}

/// Builds a `vk::PhysicalDeviceFeatures2KHR` with the full pNext chain for device creation.
pub(crate) unsafe fn build_features_chain(
    requested: &mut DeviceFeatures,
) -> vk::PhysicalDeviceFeatures2KHR<'static> {
    let mut features2 = vk::PhysicalDeviceFeatures2KHR {
        features: requested.features_1_0,
        ..Default::default()
    };

    chain_extension_features! {
        features2;
        requested.timeline_semaphore,
        requested.present_id2,
        requested.present_wait2,
    }

    features2
}
