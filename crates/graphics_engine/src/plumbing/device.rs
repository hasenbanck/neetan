use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_char},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use common::{Context as _, bail, info};
use jay_ash::vk;

use super::{
    Queue,
    features::{self, DeviceFeatures, ExtensionSet},
    memory::MemoryBlock,
    utils::{format_api_version, vulkan_debug_callback},
};
use crate::{
    Error, Result,
    plumbing::{Context, Surface},
};

/// GPU resources that can be deferred for cleanup.
pub(crate) enum DeferredResource {
    /// An image with optional view and memory allocation.
    Image {
        handle: vk::Image,
        view: Option<vk::ImageView>,
        framebuffer: Option<vk::Framebuffer>,
        memory: MemoryBlock,
    },
}

/// Configuration for device creation specifying which extensions and features to request.
///
/// This flexible configuration allows users to request arbitrary extensions and features
/// without modifying the engine code. If any requested extension or feature is unavailable,
/// device initialization will fail with an error.
#[derive(Clone)]
pub(crate) struct DeviceConfiguration {
    /// Platform-specific instance extensions (e.g. from SDL3's `SDL_Vulkan_GetInstanceExtensions`).
    pub(crate) platform_extensions: Vec<*const c_char>,

    /// Required device extensions to request.
    ///
    /// If any of these are unavailable, device initialization fails.
    pub(crate) required_device_extensions: Vec<*const c_char>,

    /// Optional device extensions to request.
    pub(crate) optional_device_extensions: Vec<*const c_char>,

    /// Vulkan 1.0 features to request.
    pub(crate) features_1_0: vk::PhysicalDeviceFeatures,
    /// VK_KHR_timeline_semaphore features.
    pub(crate) timeline_semaphore: vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR<'static>,
    /// VK_KHR_present_id2 features.
    pub(crate) present_id2: Option<vk::PhysicalDevicePresentId2FeaturesKHR<'static>>,
    /// VK_KHR_present_wait2 features.
    pub(crate) present_wait2: Option<vk::PhysicalDevicePresentWait2FeaturesKHR<'static>>,
}

impl DeviceConfiguration {
    /// Creates a new DeviceConfiguration with all the required features.
    pub(crate) fn new(platform_extensions: Vec<*const c_char>) -> Self {
        let mut required_device_extensions = vec![
            vk::KHR_SWAPCHAIN_EXTENSION_NAME.as_ptr(),
            vk::KHR_TIMELINE_SEMAPHORE_EXTENSION_NAME.as_ptr(),
        ];

        let optional_device_extensions = vec![
            vk::KHR_PRESENT_ID_2_EXTENSION_NAME.as_ptr(),
            vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME.as_ptr(),
        ];

        if cfg!(target_os = "macos") {
            required_device_extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION_NAME.as_ptr());
        }

        let features_1_0 = vk::PhysicalDeviceFeatures {
            ..Default::default()
        };

        let timeline_semaphore =
            vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR::default().timeline_semaphore(true);

        let present_id2 = vk::PhysicalDevicePresentId2FeaturesKHR::default().present_id2(true);

        let present_wait2 =
            vk::PhysicalDevicePresentWait2FeaturesKHR::default().present_wait2(true);

        Self {
            platform_extensions,
            required_device_extensions,
            optional_device_extensions,
            features_1_0,
            timeline_semaphore,
            present_id2: Some(present_id2),
            present_wait2: Some(present_wait2),
        }
    }
}

impl DeviceConfiguration {
    fn device_extensions(&self) -> Vec<*const c_char> {
        self.required_device_extensions
            .iter()
            .chain(self.optional_device_extensions.iter())
            .copied()
            .collect()
    }

    /// Converts this configuration into a DeviceFeatures struct.
    fn into_device_features(self) -> DeviceFeatures {
        DeviceFeatures {
            features_1_0: self.features_1_0,
            timeline_semaphore: Some(self.timeline_semaphore),
            present_id2: self.present_id2,
            present_wait2: self.present_wait2,
        }
    }
}

pub(crate) struct Device {
    graphics_queue: Queue,
    context: Rc<Context>,
    graveyard: RefCell<Vec<(u64, DeferredResource)>>,
    last_timeline_value: Cell<u64>,
}

impl Device {
    pub(crate) fn new(mut config: DeviceConfiguration) -> Result<Self> {
        static INIT_GUARD: AtomicBool = AtomicBool::new(false);

        if let Err(_error) =
            INIT_GUARD.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {
            panic!("Graphics device initialization more than once");
        }

        let entry = jay_ash::Entry::linked();

        let api_version = vk::make_api_version(0, 1, 0, 0);

        info!(
            "Using Vulkan API Version: {api_version}",
            api_version = format_api_version(api_version)
        );

        let (enable_validation, layer_names) = enable_validation_layers(&entry)?;

        let instance_extensions = create_instance_extensions(&entry, &config.platform_extensions)?;

        let app_info = vk::ApplicationInfo::default()
            .engine_name(c"shinsekai_engine")
            .engine_version(1)
            .application_name(c"shinsekai_app")
            .application_version(1)
            .api_version(api_version);

        let mut instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&instance_extensions.names);

        if cfg!(target_os = "macos") {
            instance_create_info =
                instance_create_info.flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
        }

        let mut debug_create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));

        if enable_validation && !layer_names.is_empty() && instance_extensions.debug_utils {
            instance_create_info = instance_create_info.push_next(&mut debug_create_info);
        }

        let instance = unsafe { entry.create_instance(&instance_create_info, None) }
            .context("Failed to create Vulkan instance")?;

        let (debug_utils_instance, debug_messenger) =
            if enable_validation && !layer_names.is_empty() && instance_extensions.debug_utils {
                let debug_utils_instance =
                    jay_ash::ext::debug_utils::Instance::new(&entry, &instance);
                let messenger = unsafe {
                    debug_utils_instance.create_debug_utils_messenger(&debug_create_info, None)
                }
                .context("Failed to create debug messenger")?;
                (Some(debug_utils_instance), Some(messenger))
            } else {
                (None, None)
            };

        let physical_device_properties2 =
            jay_ash::khr::get_physical_device_properties2::Instance::new(&entry, &instance);

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .context("Failed to enumerate physical devices")?;

        let candidate = select_physical_device(
            &instance,
            &physical_device_properties2,
            &physical_devices,
            &config.required_device_extensions,
            &config.optional_device_extensions,
        )?;

        let device_name =
            unsafe { CStr::from_ptr(candidate.properties.device_name.as_ptr()).to_string_lossy() }
                .into_owned();
        info!("Selected physical device: {device_name}");

        // We do not check for limits, since our requirements are well within the
        // common desktop class limits:
        //
        // maxImageDimension2D = 8192 (we need 640x400 and the device has to support the color target for the viewport size)
        // maxMemoryAllocationCount = 4096 (we only allocate a couple of images and buffers)
        // maxPerStageDescriptorSamplers = 64 (we need at most two)
        // maxPerStageDescriptorStorageBuffers = 4 (we need at most one)
        // maxPerStageDescriptorSampledImages = 16 (we need at most one)
        // maxBoundDescriptorSets = 4 (we need at most two)

        let present_id2_extension_present = candidate.extension_set.present_id2;
        let present_wait2_extension_present = candidate.extension_set.present_wait2;
        let present_id2_feature_available = candidate
            .features
            .present_id2
            .as_ref()
            .is_some_and(|features| features.present_id2 == vk::TRUE);
        let present_wait2_feature_available = candidate
            .features
            .present_wait2
            .as_ref()
            .is_some_and(|features| features.present_wait2 == vk::TRUE);

        let present_wait_disabled_reason =
            if !present_id2_extension_present || !present_wait2_extension_present {
                let mut missing = Vec::new();
                if !present_id2_extension_present {
                    missing.push("VK_KHR_present_id2");
                }
                if !present_wait2_extension_present {
                    missing.push("VK_KHR_present_wait2");
                }
                Some(format!(
                    "optional extension unavailable ({missing})",
                    missing = missing.join(", ")
                ))
            } else if !present_id2_feature_available || !present_wait2_feature_available {
                let mut missing = Vec::new();
                if !present_id2_feature_available {
                    missing.push("present_id2");
                }
                if !present_wait2_feature_available {
                    missing.push("present_wait2");
                }
                Some(format!(
                    "optional extension feature unavailable ({missing})",
                    missing = missing.join(", ")
                ))
            } else {
                None
            };

        if present_wait_disabled_reason.is_some() {
            disable_present_wait2(&mut config);
        }

        if let Some(reason) = present_wait_disabled_reason {
            info!("Disabling present wait: {reason}");
        } else {
            info!("Present wait enabled via VK_KHR_present_id2 and VK_KHR_present_wait2");
        }

        // Build the final extension list as the intersection of the still-requested
        // extensions in `config` and the extensions actually available on the candidate.
        // Required extensions were already validated during candidate evaluation;
        let extension_ptrs: Vec<*const c_char> = config
            .device_extensions()
            .into_iter()
            .filter(|&ptr| {
                let name = unsafe { CStr::from_ptr(ptr) };
                extension_in_available(&candidate.available_extensions, name)
            })
            .collect();

        info!("Device extensions:");
        for &extension_ptr in extension_ptrs.iter() {
            let extension_name = unsafe { CStr::from_ptr(extension_ptr) };
            let name_str = extension_name.to_string_lossy();
            info!("  - {name_str}");
        }

        let enable_present_wait2 = config.present_id2.is_some() && config.present_wait2.is_some();
        let mut requested_features = config.into_device_features();
        features::validate_features(&requested_features, &candidate.features)?;

        let mut features2 = unsafe { features::build_features_chain(&mut requested_features) };

        let graphics_queue_family = candidate.graphics_queue_family;
        let queue_create_info = build_queue_create_info(graphics_queue_family);

        let queue_create_infos = [queue_create_info];
        let mut device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&extension_ptrs);

        // Manually set pNext since features2 already has a chain.
        device_create_info.p_next = &mut features2 as *mut _ as *mut std::ffi::c_void;

        let physical_device = candidate.physical_device;
        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .context("Failed to create logical device")?;

        let debug_utils_device = instance_extensions
            .debug_utils
            .then(|| jay_ash::ext::debug_utils::Device::new(&instance, &device));
        let timeline_semaphore = jay_ash::khr::timeline_semaphore::Device::new(&instance, &device);
        let present_wait2 = if enable_present_wait2 {
            Some(jay_ash::khr::present_wait2::Device::new(&instance, &device))
        } else {
            None
        };

        let allocator = unsafe { super::memory::GpuAllocator::new(&instance, physical_device) }
            .context("Failed to create GPU memory allocator")?;

        let context = Rc::new(Context::new(
            allocator,
            debug_utils_device,
            debug_utils_instance,
            debug_messenger,
            timeline_semaphore,
            present_wait2,
            instance,
            device,
            physical_device,
            entry,
        ));

        let graphics_queue_handle =
            unsafe { context.device().get_device_queue(graphics_queue_family, 0) };

        let graphics_queue = Queue::new(
            context.clone(),
            graphics_queue_handle,
            graphics_queue_family,
            c"graphics_queue",
        );

        Ok(Self {
            graphics_queue,
            context,
            graveyard: RefCell::new(Vec::new()),
            last_timeline_value: Cell::new(0),
        })
    }

    /// Returns a reference to the graphics queue.
    #[inline]
    pub(crate) fn graphics_queue(&self) -> &Queue {
        &self.graphics_queue
    }

    /// Returns a reference to the context.
    #[inline]
    pub(crate) fn context(&self) -> &Rc<Context> {
        &self.context
    }

    /// Creates a new surface from a pre-created `VkSurfaceKHR` handle.
    pub(crate) fn create_surface_from_handle(
        &self,
        surface_handle: vk::SurfaceKHR,
        vsync_enabled: bool,
    ) -> Surface {
        let surface_loader =
            jay_ash::khr::surface::Instance::new(self.context.entry(), self.context.instance());

        Surface::new(
            self.context.clone(),
            surface_handle,
            surface_loader,
            self.graphics_queue.family_index(),
            vsync_enabled,
        )
    }

    /// Defers a resource for cleanup after a delay.
    ///
    /// The resource will be kept alive until `clear_graveyard` is called with
    /// a timeline value at least `removal_delay` ticks after the deferred value.
    pub(crate) fn defer_resource(&self, resource: DeferredResource) {
        let timeline = self.last_timeline_value.get();
        self.graveyard.borrow_mut().push((timeline, resource));
    }

    /// Clears resources older than `removal_delay` timeline ticks.
    ///
    /// `removal_delay` must be >= frame_count because:
    /// - Resources may be in use by graphics commands.
    /// - Graphics completion is only guaranteed when the same slot's present fence is waited on.
    /// - With N frame slots, we must wait for N frames to cycle back and wait on the original slot's fence.
    ///
    /// This should be called once per frame after waiting on the timeline semaphore.
    pub(crate) fn clear_graveyard(&self, timeline_value: u64, removal_delay: u64) {
        self.last_timeline_value.set(timeline_value);

        let mut graveyard = self.graveyard.borrow_mut();
        let mut i = 0;
        while i < graveyard.len() {
            if graveyard[i].0 + removal_delay <= timeline_value {
                let (_, resource) = graveyard.swap_remove(i);
                match resource {
                    DeferredResource::Image {
                        handle,
                        view,
                        framebuffer,
                        memory,
                    } => unsafe {
                        if let Some(framebuffer) = framebuffer {
                            self.context.device().destroy_framebuffer(framebuffer, None);
                        }
                        if let Some(view) = view {
                            self.context.device().destroy_image_view(view, None);
                        }
                        self.context.device().destroy_image(handle, None);
                        self.context
                            .allocator()
                            .dealloc(self.context.device(), memory);
                    },
                }
            } else {
                i += 1;
            }
        }
    }
}

fn disable_present_wait2(config: &mut DeviceConfiguration) {
    config.optional_device_extensions.retain(|&ptr| {
        ptr != vk::KHR_PRESENT_ID_2_EXTENSION_NAME.as_ptr()
            && ptr != vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME.as_ptr()
    });
    config.present_id2 = None;
    config.present_wait2 = None;
}

fn enable_validation_layers(entry: &jay_ash::Entry) -> Result<(bool, Vec<*const c_char>)> {
    static VALIDATION_LAYER_NAME: &CStr = c"VK_LAYER_KHRONOS_validation";
    let enable_validation = cfg!(debug_assertions);

    let mut layer_names = Vec::new();
    if enable_validation {
        let available_layers = unsafe { entry.enumerate_instance_layer_properties() }
            .context("Failed to enumerate instance layers")?;

        let validation_available = available_layers.iter().any(|layer| {
            let name = unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) };
            name == VALIDATION_LAYER_NAME
        });

        if validation_available {
            info!("Enabled Vulkan validation layers");
            layer_names.push(VALIDATION_LAYER_NAME.as_ptr());
        } else {
            info!("Validation layers requested but not available");
        }
    }

    Ok((enable_validation, layer_names))
}

struct InstanceExtensions {
    names: Vec<*const c_char>,
    debug_utils: bool,
}

fn create_instance_extensions(
    entry: &jay_ash::Entry,
    platform_extensions: &[*const c_char],
) -> Result<InstanceExtensions> {
    let available_extensions = unsafe { entry.enumerate_instance_extension_properties(None) }
        .context("Failed to enumerate instance extensions")?;

    let mut extension_names = Vec::new();

    for &extension in platform_extensions {
        push_required_instance_extension(&available_extensions, &mut extension_names, extension)?;
    }

    push_required_instance_extension(
        &available_extensions,
        &mut extension_names,
        jay_ash::khr::get_physical_device_properties2::NAME.as_ptr(),
    )?;
    push_required_instance_extension(
        &available_extensions,
        &mut extension_names,
        jay_ash::khr::get_surface_capabilities2::NAME.as_ptr(),
    )?;

    if cfg!(target_os = "macos") {
        push_required_instance_extension(
            &available_extensions,
            &mut extension_names,
            jay_ash::khr::portability_enumeration::NAME.as_ptr(),
        )?;
    }

    let debug_utils =
        is_instance_extension_available(&available_extensions, jay_ash::ext::debug_utils::NAME);
    if debug_utils {
        push_unique_extension(
            &mut extension_names,
            jay_ash::ext::debug_utils::NAME.as_ptr(),
        );
    }

    info!("Enabled instance extensions:");
    for ext in extension_names.iter() {
        let ext_name = unsafe { CStr::from_ptr(*ext) };
        info!("  - {ext_name}", ext_name = ext_name.to_string_lossy());
    }

    Ok(InstanceExtensions {
        names: extension_names,
        debug_utils,
    })
}

fn push_required_instance_extension(
    available_extensions: &[vk::ExtensionProperties],
    extension_names: &mut Vec<*const c_char>,
    extension: *const c_char,
) -> Result<()> {
    let extension_name = unsafe { CStr::from_ptr(extension) };

    if !is_instance_extension_available(available_extensions, extension_name) {
        bail!(
            "required instance extension {extension_name} not available",
            extension_name = extension_name.to_string_lossy()
        );
    }

    push_unique_extension(extension_names, extension);
    Ok(())
}

fn push_unique_extension(extension_names: &mut Vec<*const c_char>, extension: *const c_char) {
    let extension_name = unsafe { CStr::from_ptr(extension) };

    if extension_names.iter().all(|&existing_extension| {
        let existing_name = unsafe { CStr::from_ptr(existing_extension) };
        existing_name != extension_name
    }) {
        extension_names.push(extension);
    }
}

fn is_instance_extension_available(
    available_extensions: &[vk::ExtensionProperties],
    extension_name: &CStr,
) -> bool {
    available_extensions.iter().any(|extension| {
        let available_name = unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) };
        available_name == extension_name
    })
}

fn build_queue_create_info(graphics_queue_family: u32) -> vk::DeviceQueueCreateInfo<'static> {
    static QUEUE_PRIORITIES: &[f32] = &[1.0f32];

    vk::DeviceQueueCreateInfo::default()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(QUEUE_PRIORITIES)
}

/// A physical device evaluated against the configured requirements, with all
/// per-device queries cached so device creation does not re-issue them.
struct PhysicalDeviceCandidate {
    physical_device: vk::PhysicalDevice,
    properties: vk::PhysicalDeviceProperties,
    graphics_queue_family: u32,
    available_extensions: Vec<vk::ExtensionProperties>,
    extension_set: ExtensionSet,
    features: features::DeviceFeatures,
    score: u32,
}

/// Selects the best suitable physical device, evaluating each candidate exactly once.
fn select_physical_device(
    instance: &jay_ash::Instance,
    physical_device_properties2: &jay_ash::khr::get_physical_device_properties2::Instance,
    devices: &[vk::PhysicalDevice],
    required_device_extensions: &[*const c_char],
    optional_device_extensions: &[*const c_char],
) -> Result<PhysicalDeviceCandidate> {
    if devices.is_empty() {
        bail!("no physical devices found");
    }

    let mut best: Option<PhysicalDeviceCandidate> = None;

    for &device in devices {
        let Some(candidate) = evaluate_physical_device_candidate(
            instance,
            physical_device_properties2,
            device,
            required_device_extensions,
            optional_device_extensions,
        ) else {
            continue;
        };

        match &best {
            Some(current_best) if current_best.score >= candidate.score => {}
            _ => best = Some(candidate),
        }
    }

    best.ok_or_else(|| {
        Error::Message(common::StringError(
            "no suitable physical device found (missing graphics queue, required extensions, or required features)".to_owned(),
        ))
    })
}

/// Evaluates a single physical device. Returns `None` if it does not meet the
/// hard requirements (graphics queue, all required extensions, required feature bits).
fn evaluate_physical_device_candidate(
    instance: &jay_ash::Instance,
    physical_device_properties2: &jay_ash::khr::get_physical_device_properties2::Instance,
    physical_device: vk::PhysicalDevice,
    required_device_extensions: &[*const c_char],
    optional_device_extensions: &[*const c_char],
) -> Option<PhysicalDeviceCandidate> {
    let properties = {
        let mut properties2 = vk::PhysicalDeviceProperties2KHR::default();
        unsafe {
            physical_device_properties2
                .get_physical_device_properties2(physical_device, &mut properties2);
        }
        properties2.properties
    };

    let graphics_queue_family = find_graphics_queue_family(instance, physical_device)?;

    let available_extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device) }.ok()?;

    for &required in required_device_extensions {
        let required_name = unsafe { CStr::from_ptr(required) };
        if !extension_in_available(&available_extensions, required_name) {
            return None;
        }
    }

    let mut enabled_extension_ptrs: Vec<*const c_char> = required_device_extensions.to_vec();
    for &optional in optional_device_extensions {
        let optional_name = unsafe { CStr::from_ptr(optional) };
        if extension_in_available(&available_extensions, optional_name) {
            enabled_extension_ptrs.push(optional);
        }
    }

    let extension_set = ExtensionSet {
        timeline_semaphore: contains_extension(
            &enabled_extension_ptrs,
            vk::KHR_TIMELINE_SEMAPHORE_EXTENSION_NAME,
        ),
        present_id2: contains_extension(
            &enabled_extension_ptrs,
            vk::KHR_PRESENT_ID_2_EXTENSION_NAME,
        ),
        present_wait2: contains_extension(
            &enabled_extension_ptrs,
            vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME,
        ),
    };

    let features = features::query_physical_device_features(
        physical_device_properties2,
        physical_device,
        &extension_set,
    );

    let timeline_semaphore_supported = extension_set.timeline_semaphore
        && features
            .timeline_semaphore
            .as_ref()
            .is_some_and(|f| f.timeline_semaphore == vk::TRUE);

    if !timeline_semaphore_supported {
        return None;
    }

    let mut score = 1u32;
    if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        score += 1000;
    }

    Some(PhysicalDeviceCandidate {
        physical_device,
        properties,
        graphics_queue_family,
        available_extensions,
        extension_set,
        features,
        score,
    })
}

fn extension_in_available(
    available_extensions: &[vk::ExtensionProperties],
    extension_name: &CStr,
) -> bool {
    available_extensions.iter().any(|extension| {
        let available_name = unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) };
        available_name == extension_name
    })
}

fn contains_extension(extensions: &[*const c_char], target: &CStr) -> bool {
    extensions.iter().any(|&ptr| {
        let name = unsafe { CStr::from_ptr(ptr) };
        name == target
    })
}

/// Finds the graphics queue family index for the given physical device.
fn find_graphics_queue_family(
    instance: &jay_ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Option<u32> {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    queue_families
        .iter()
        .enumerate()
        .find(|(_, qf)| qf.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .map(|(index, _)| index as u32)
}
