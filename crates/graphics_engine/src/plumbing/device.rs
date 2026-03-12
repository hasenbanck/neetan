use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_char},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use common::{Context as _, bail, info};
use jay_ash::vk;

use super::{
    Queue, extensions,
    features::{self, DeviceFeatures},
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

    /// Device extensions to request.
    ///
    /// The engine automatically adds required extensions like VK_KHR_swapchain
    /// and VK_KHR_synchronization2. Only available extensions will be enabled.
    pub(crate) extensions: Vec<*const c_char>,

    /// Vulkan 1.0 features to request.
    pub(crate) features_1_0: vk::PhysicalDeviceFeatures,
    /// Vulkan 1.1 features to request.
    pub(crate) features_1_1: vk::PhysicalDeviceVulkan11Features<'static>,
    /// Vulkan 1.2 features to request.
    pub(crate) features_1_2: vk::PhysicalDeviceVulkan12Features<'static>,
    /// Vulkan 1.3 features to request.
    pub(crate) features_1_3: vk::PhysicalDeviceVulkan13Features<'static>,
    /// Vulkan 1.4 features to request.
    pub(crate) features_1_4: vk::PhysicalDeviceVulkan14Features<'static>,
    /// VK_KHR_present_id2 features.
    pub(crate) present_id2: Option<vk::PhysicalDevicePresentId2FeaturesKHR<'static>>,
    /// VK_KHR_present_wait2 features.
    pub(crate) present_wait2: Option<vk::PhysicalDevicePresentWait2FeaturesKHR<'static>>,
}

impl DeviceConfiguration {
    /// Creates a new DeviceConfiguration with all the required features.
    /// Aims to use all best practises for modern Vulkan 1.4.
    pub(crate) fn new(platform_extensions: Vec<*const c_char>) -> Self {
        let mut extensions = vec![
            // Standard swap chain extension (always supported).
            vk::KHR_SWAPCHAIN_EXTENSION_NAME.as_ptr(),
            // Present wait for better frame pacing (optional).
            //
            // Allows waiting for previous frame presentation to complete, enabling
            // more consistent frame timing and reduced input latency.
            vk::KHR_PRESENT_ID_2_EXTENSION_NAME.as_ptr(),
            vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME.as_ptr(),
        ];

        if cfg!(target_os = "macos") {
            extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION_NAME.as_ptr());
        }

        let features_1_0 = vk::PhysicalDeviceFeatures {
            ..Default::default()
        };

        let features_1_1 = vk::PhysicalDeviceVulkan11Features {
            ..Default::default()
        };

        let features_1_2 = vk::PhysicalDeviceVulkan12Features {
            // Required support since Vulkan 1.4.
            scalar_block_layout: vk::TRUE,
            timeline_semaphore: vk::TRUE,
            ..Default::default()
        };

        let features_1_3 = vk::PhysicalDeviceVulkan13Features {
            dynamic_rendering: vk::TRUE,
            synchronization2: vk::TRUE,
            ..Default::default()
        };

        let features_1_4 = vk::PhysicalDeviceVulkan14Features {
            // Removes the need to create shader modules.
            maintenance5: vk::TRUE,
            ..Default::default()
        };

        let present_id2 = vk::PhysicalDevicePresentId2FeaturesKHR::default().present_id2(true);

        let present_wait2 =
            vk::PhysicalDevicePresentWait2FeaturesKHR::default().present_wait2(true);

        Self {
            platform_extensions,
            extensions,
            features_1_0,
            features_1_1,
            features_1_2,
            features_1_3,
            features_1_4,
            present_id2: Some(present_id2),
            present_wait2: Some(present_wait2),
        }
    }
}

impl DeviceConfiguration {
    /// Converts this configuration into a DeviceFeatures struct.
    fn into_device_features(self) -> DeviceFeatures {
        DeviceFeatures {
            features_1_0: self.features_1_0,
            features_1_1: self.features_1_1,
            features_1_2: self.features_1_2,
            features_1_3: self.features_1_3,
            features_1_4: self.features_1_4,
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

        let entry = load_vulkan_entry().context("Failed to load Vulkan entry")?;

        let api_version = unsafe { entry.try_enumerate_instance_version() }
            .context("Failed to enumerate instance version")?
            .unwrap_or(vk::make_api_version(0, 1, 0, 0));

        if api_version < vk::make_api_version(0, 1, 4, 0) {
            bail!("Unsupported Vulkan API version (requires 1.4+)");
        }

        let api_version = vk::make_api_version(0, 1, 4, 0);

        info!(
            "Using Vulkan API Version: {api_version}",
            api_version = format_api_version(api_version)
        );

        let (enable_validation, layer_names) = enable_validation_layers(&entry)?;

        let extension_names = create_instance_extensions(&config.platform_extensions);

        let app_info = vk::ApplicationInfo::default()
            .engine_name(c"shinsekai_engine")
            .engine_version(1)
            .application_name(c"shinsekai_app")
            .application_version(1)
            .api_version(api_version);

        let mut instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&extension_names);

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

        if enable_validation && !layer_names.is_empty() {
            instance_create_info = instance_create_info.push_next(&mut debug_create_info);
        }

        let instance = unsafe { entry.create_instance(&instance_create_info, None) }
            .context("Failed to create Vulkan instance")?;

        let (debug_utils_instance, debug_messenger) = if enable_validation
            && !layer_names.is_empty()
        {
            let debug_utils_instance = jay_ash::ext::debug_utils::Instance::new(&entry, &instance);
            let messenger = unsafe {
                debug_utils_instance.create_debug_utils_messenger(&debug_create_info, None)
            }
            .context("Failed to create debug messenger")?;
            (Some(debug_utils_instance), Some(messenger))
        } else {
            (None, None)
        };

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .context("Failed to enumerate physical devices")?;

        let physical_device = select_physical_device(&instance, &physical_devices)?;

        let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_name =
            unsafe { CStr::from_ptr(device_properties.device_name.as_ptr()).to_string_lossy() };
        info!("Selected physical device: {device_name}");

        // We do not need to check for limits, since our requirements are well within the
        // requirements for Vulkan 1.4 support:
        //
        // maxImageDimension2D = 8192 (we need 640x400 and the device has to support the color target for the viewport size)
        // maxMemoryAllocationCount = 4096 (we only allocate a couple of images and buffers)
        // maxPerStageDescriptorSamplers = 64 (we need at most two)
        // maxPerStageDescriptorStorageBuffers = 4 (we need at most one)
        // maxPerStageDescriptorSampledImages = 16 (we need at most one)
        // maxBoundDescriptorSets = 4 (we need at most two)

        let (mut extension_ptrs, extension_set) = extensions::build_extension_list(
            &instance,
            physical_device,
            config.extensions.clone(),
        )?;

        let available_features =
            features::query_physical_device_features(&instance, physical_device, &extension_set);

        // Check present_id2 and present_wait2 availability.
        let present_id2_available = extensions::is_extension_available(
            &instance,
            physical_device,
            vk::KHR_PRESENT_ID_2_EXTENSION_NAME,
        );
        let present_wait2_available = extensions::is_extension_available(
            &instance,
            physical_device,
            vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME,
        );

        if !present_id2_available || !present_wait2_available {
            if !present_id2_available {
                info!("VK_KHR_present_id2 not available on this device");
            }
            if !present_wait2_available {
                info!("VK_KHR_present_wait2 not available on this device");
            }
            extension_ptrs.retain(|&ptr| {
                ptr != vk::KHR_PRESENT_ID_2_EXTENSION_NAME.as_ptr()
                    && ptr != vk::KHR_PRESENT_WAIT_2_EXTENSION_NAME.as_ptr()
            });
            config.present_id2 = None;
            config.present_wait2 = None;
        } else {
            info!("VK_KHR_present_id2 and VK_KHR_present_wait2 are available on this device");
        }

        info!("Device extensions:");
        for &extension_ptr in extension_ptrs.iter() {
            let extension_name = unsafe { CStr::from_ptr(extension_ptr) };
            let name_str = extension_name.to_string_lossy();
            info!("  - {name_str}");
        }

        let mut requested_features = config.into_device_features();
        features::validate_features(&requested_features, &available_features)?;

        let mut features2 = unsafe { features::build_features_chain(&mut requested_features) };

        let (graphics_queue_family, queue_create_info) =
            query_graphics_queue_family(&instance, physical_device);

        let queue_create_infos = [queue_create_info];
        let mut device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&extension_ptrs);

        // Manually set pNext since features2 already has a chain.
        device_create_info.p_next = &mut features2 as *mut _ as *mut std::ffi::c_void;

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .context("Failed to create logical device")?;

        let debug_utils_device = jay_ash::ext::debug_utils::Device::new(&instance, &device);
        let present_wait2 = if present_id2_available && present_wait2_available {
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
                        memory,
                    } => unsafe {
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

fn create_instance_extensions(platform_extensions: &[*const c_char]) -> Vec<*const c_char> {
    let mut extension_names = Vec::from(platform_extensions);

    extension_names.push(jay_ash::khr::get_surface_capabilities2::NAME.as_ptr());
    extension_names.push(jay_ash::ext::debug_utils::NAME.as_ptr());

    if cfg!(target_os = "macos") {
        extension_names.push(jay_ash::khr::portability_enumeration::NAME.as_ptr());
    }

    info!("Enabled instance extensions:");
    for ext in extension_names.iter() {
        let ext_name = unsafe { CStr::from_ptr(*ext) };
        info!("  - {ext_name}", ext_name = ext_name.to_string_lossy());
    }

    extension_names
}

fn query_graphics_queue_family(
    instance: &jay_ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> (u32, vk::DeviceQueueCreateInfo<'static>) {
    let graphics_queue_family = find_graphics_queue_family(instance, physical_device).unwrap();

    static QUEUE_PRIORITIES: &[f32] = &[1.0f32];

    let queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(QUEUE_PRIORITIES);

    (graphics_queue_family, queue_create_info)
}

/// Scores a physical device based on its type and capabilities.
/// Higher score is better. Returns 0 if device is unsuitable.
fn score_device(instance: &jay_ash::Instance, physical_device: vk::PhysicalDevice) -> u32 {
    let properties = unsafe { instance.get_physical_device_properties(physical_device) };

    if find_graphics_queue_family(instance, physical_device).is_none() {
        return 0;
    }

    let mut score = 1;

    if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        score += 1000;
    }

    score
}

/// Selects the best physical device from the available devices.
fn select_physical_device(
    instance: &jay_ash::Instance,
    devices: &[vk::PhysicalDevice],
) -> Result<vk::PhysicalDevice> {
    if devices.is_empty() {
        bail!("no physical devices found");
    }

    let mut best_device = None;
    let mut best_score = 0;

    for &device in devices {
        let score = score_device(instance, device);
        if score > best_score {
            best_score = score;
            best_device = Some(device);
        }
    }

    best_device.ok_or_else(|| {
        Error::Message(common::StringError(
            "no suitable physical device found (missing required queue families)".to_owned(),
        ))
    })
}

fn load_vulkan_entry() -> Result<jay_ash::Entry> {
    let entry = jay_ash::Entry::linked();
    Ok(entry)
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
