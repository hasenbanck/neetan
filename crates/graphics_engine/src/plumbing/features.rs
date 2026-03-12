//! Vulkan physical device feature management for Vulkan 1.0-1.4.

use common::{bail, error, info, warn};
use jay_ash::vk;

use crate::{Result, plumbing::extensions::ExtensionSet};

/// Configuration for which device features to request.
///
/// Features are organized by Vulkan version (1.0-1.4) and by extension.
/// Only features that are both requested AND available will be enabled.
#[derive(Default, Clone)]
pub(crate) struct DeviceFeatures {
    /// Vulkan 1.0 core features.
    pub(crate) features_1_0: vk::PhysicalDeviceFeatures,
    /// Vulkan 1.1 features.
    pub(crate) features_1_1: vk::PhysicalDeviceVulkan11Features<'static>,
    /// Vulkan 1.2 features.
    pub(crate) features_1_2: vk::PhysicalDeviceVulkan12Features<'static>,
    /// Vulkan 1.3 features.
    pub(crate) features_1_3: vk::PhysicalDeviceVulkan13Features<'static>,
    /// Vulkan 1.4 features.
    pub(crate) features_1_4: vk::PhysicalDeviceVulkan14Features<'static>,
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
    instance: &jay_ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension_set: &ExtensionSet,
) -> DeviceFeatures {
    let mut features_1_1 = vk::PhysicalDeviceVulkan11Features::default();
    let mut features_1_2 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features_1_3 = vk::PhysicalDeviceVulkan13Features::default();
    let mut features_1_4 = vk::PhysicalDeviceVulkan14Features::default();

    init_extension_features! {
        extension_set;
        descriptor_heap => vk::PhysicalDeviceDescriptorHeapFeaturesEXT,
        present_id2 => vk::PhysicalDevicePresentId2FeaturesKHR,
        present_wait2 => vk::PhysicalDevicePresentWait2FeaturesKHR,
    }

    let mut features2 = vk::PhysicalDeviceFeatures2::default();

    features_1_1.p_next = std::ptr::null_mut();
    features_1_2.p_next = &mut features_1_1 as *mut _ as *mut std::ffi::c_void;
    features_1_3.p_next = &mut features_1_2 as *mut _ as *mut std::ffi::c_void;
    features_1_4.p_next = &mut features_1_3 as *mut _ as *mut std::ffi::c_void;
    features2.p_next = &mut features_1_4 as *mut _ as *mut std::ffi::c_void;

    chain_extension_features! {
        features2;
        descriptor_heap,
        present_id2,
        present_wait2,
    }

    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }

    DeviceFeatures {
        features_1_0: features2.features,
        features_1_1,
        features_1_2,
        features_1_3,
        features_1_4,
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

    let mut requested_1_1_list: Vec<&str> = Vec::new();
    let mut available_1_1_list: Vec<&str> = Vec::new();

    impl_feature_check!(
        requested.features_1_1, available.features_1_1, requested_1_1_list, available_1_1_list;
        {
            storage_buffer16_bit_access uniform_and_storage_buffer16_bit_access
            storage_push_constant16 storage_input_output16 multiview multiview_geometry_shader
            multiview_tessellation_shader variable_pointers_storage_buffer variable_pointers
            protected_memory sampler_ycbcr_conversion shader_draw_parameters
        }
    );

    let mut requested_1_2_list: Vec<&str> = Vec::new();
    let mut available_1_2_list: Vec<&str> = Vec::new();

    impl_feature_check!(
        requested.features_1_2, available.features_1_2, requested_1_2_list, available_1_2_list;
        {
            sampler_mirror_clamp_to_edge draw_indirect_count storage_buffer8_bit_access
            uniform_and_storage_buffer8_bit_access storage_push_constant8 shader_buffer_int64_atomics
            shader_shared_int64_atomics shader_float16 shader_int8 descriptor_indexing
            shader_input_attachment_array_dynamic_indexing shader_uniform_texel_buffer_array_dynamic_indexing
            shader_storage_texel_buffer_array_dynamic_indexing shader_uniform_buffer_array_non_uniform_indexing
            shader_sampled_image_array_non_uniform_indexing shader_storage_buffer_array_non_uniform_indexing
            shader_storage_image_array_non_uniform_indexing shader_input_attachment_array_non_uniform_indexing
            shader_uniform_texel_buffer_array_non_uniform_indexing shader_storage_texel_buffer_array_non_uniform_indexing
            descriptor_binding_uniform_buffer_update_after_bind descriptor_binding_sampled_image_update_after_bind
            descriptor_binding_storage_image_update_after_bind descriptor_binding_storage_buffer_update_after_bind
            descriptor_binding_uniform_texel_buffer_update_after_bind descriptor_binding_storage_texel_buffer_update_after_bind
            descriptor_binding_update_unused_while_pending descriptor_binding_partially_bound
            descriptor_binding_variable_descriptor_count runtime_descriptor_array sampler_filter_minmax
            scalar_block_layout imageless_framebuffer uniform_buffer_standard_layout
            shader_subgroup_extended_types separate_depth_stencil_layouts host_query_reset
            timeline_semaphore buffer_device_address buffer_device_address_capture_replay
            buffer_device_address_multi_device vulkan_memory_model vulkan_memory_model_device_scope
            vulkan_memory_model_availability_visibility_chains shader_output_viewport_index
            shader_output_layer subgroup_broadcast_dynamic_id
        }
    );

    let mut requested_1_3_list: Vec<&str> = Vec::new();
    let mut available_1_3_list: Vec<&str> = Vec::new();

    impl_feature_check!(
        requested.features_1_3, available.features_1_3, requested_1_3_list, available_1_3_list;
        {
            robust_image_access inline_uniform_block descriptor_binding_inline_uniform_block_update_after_bind
            pipeline_creation_cache_control private_data shader_demote_to_helper_invocation
            shader_terminate_invocation subgroup_size_control compute_full_subgroups synchronization2
            texture_compression_astc_hdr shader_zero_initialize_workgroup_memory dynamic_rendering
            shader_integer_dot_product maintenance4
        }
    );

    let mut requested_1_4_list: Vec<&str> = Vec::new();
    let mut available_1_4_list: Vec<&str> = Vec::new();

    impl_feature_check!(
        requested.features_1_4, available.features_1_4, requested_1_4_list, available_1_4_list;
        {
            global_priority_query shader_subgroup_rotate shader_subgroup_rotate_clustered
            shader_float_controls2 shader_expect_assume rectangular_lines bresenham_lines
            smooth_lines stippled_rectangular_lines stippled_bresenham_lines stippled_smooth_lines
            vertex_attribute_instance_rate_divisor vertex_attribute_instance_rate_zero_divisor
            index_type_uint8 dynamic_rendering_local_read maintenance5 maintenance6
            pipeline_protected_access pipeline_robustness host_image_copy push_descriptor
        }
    );

    log_and_validate_features(
        &requested_1_0_list,
        &available_1_0_list,
        "Vulkan 1.0",
        &mut missing_features,
        true,
    );

    log_and_validate_features(
        &requested_1_1_list,
        &available_1_1_list,
        "Vulkan 1.1",
        &mut missing_features,
        true,
    );

    log_and_validate_features(
        &requested_1_2_list,
        &available_1_2_list,
        "Vulkan 1.2",
        &mut missing_features,
        true,
    );

    log_and_validate_features(
        &requested_1_3_list,
        &available_1_3_list,
        "Vulkan 1.3",
        &mut missing_features,
        true,
    );

    log_and_validate_features(
        &requested_1_4_list,
        &available_1_4_list,
        "Vulkan 1.4",
        &mut missing_features,
        true,
    );

    validate_extension_features! {
        requested, available, missing_features;
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

/// Builds a `vk::PhysicalDeviceFeatures2` with the full pNext chain for device creation.
pub(crate) unsafe fn build_features_chain(
    requested: &mut DeviceFeatures,
) -> vk::PhysicalDeviceFeatures2<'static> {
    let mut features2 = vk::PhysicalDeviceFeatures2 {
        features: requested.features_1_0,
        ..Default::default()
    };

    requested.features_1_1.p_next = std::ptr::null_mut();
    requested.features_1_2.p_next = &mut requested.features_1_1 as *mut _ as *mut std::ffi::c_void;
    requested.features_1_3.p_next = &mut requested.features_1_2 as *mut _ as *mut std::ffi::c_void;
    requested.features_1_4.p_next = &mut requested.features_1_3 as *mut _ as *mut std::ffi::c_void;
    features2.p_next = &mut requested.features_1_4 as *mut _ as *mut std::ffi::c_void;

    chain_extension_features! {
        features2;
        requested.present_id2,
        requested.present_wait2,
    }

    features2
}
