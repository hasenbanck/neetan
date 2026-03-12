//! Surface and swapchain management for presentation.

use std::rc::Rc;

use common::{Context as _, OptionContext as _, bail, ensure};
use jay_ash::vk;

use super::context::Context;
use crate::{
    Result,
    plumbing::{FrameResources, Queue},
};

/// Result of acquiring a swapchain image.
enum AcquireStatus {
    /// Image acquired successfully.
    Success(u32),
    /// Swapchain is suboptimal but usable. Contains the acquired image index.
    Suboptimal(u32),
    /// Swapchain is out of date and must be recreated.
    OutOfDate,
}

/// An opaque handle for a native surface with integrated swapchain management.
pub(crate) struct Surface {
    handle: vk::SurfaceKHR,
    surface_loader: jay_ash::khr::surface::Instance,
    surface_capabilities2_loader: jay_ash::khr::get_surface_capabilities2::Instance,
    graphics_queue_family: u32,

    swapchain: Option<vk::SwapchainKHR>,
    swapchain_loader: jay_ash::khr::swapchain::Device,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,

    format: vk::SurfaceFormatKHR,
    is_srgb: bool,
    extent: vk::Extent2D,

    /// Present mode for V-Sync ON (always FIFO).
    vsync_on_mode: vk::PresentModeKHR,
    /// Present mode for V-Sync OFF (MAILBOX > IMMEDIATE > None).
    vsync_off_mode: Option<vk::PresentModeKHR>,
    /// Current V-Sync state.
    vsync_enabled: bool,

    /// Whether VK_KHR_present_wait2 is supported.
    supports_present_wait2: bool,

    context: Rc<Context>,
}

impl Surface {
    /// Creates a new surface without initializing the swapchain.
    ///
    /// Call `initialize_swapchain()` after creation to set up the swapchain.
    pub(crate) fn new(
        context: Rc<Context>,
        handle: vk::SurfaceKHR,
        surface_loader: jay_ash::khr::surface::Instance,
        graphics_queue_family: u32,
        vsync_enabled: bool,
    ) -> Self {
        context.set_object_name(c"surface", handle);

        let swapchain_loader =
            jay_ash::khr::swapchain::Device::new(context.instance(), context.device());
        let surface_capabilities2_loader = jay_ash::khr::get_surface_capabilities2::Instance::new(
            context.entry(),
            context.instance(),
        );

        let supports_present_wait2 = context.supports_present_wait2();

        Self {
            handle,
            surface_loader,
            surface_capabilities2_loader,
            graphics_queue_family,
            swapchain: None,
            swapchain_loader,
            images: Vec::new(),
            image_views: Vec::new(),
            format: vk::SurfaceFormatKHR::default(),
            is_srgb: false,
            extent: vk::Extent2D::default(),
            vsync_on_mode: vk::PresentModeKHR::FIFO,
            vsync_off_mode: None,
            vsync_enabled,
            supports_present_wait2,
            context,
        }
    }

    /// Initializes the swapchain with the specified extent.
    ///
    /// If `extent` is None, queries the current surface capabilities for the extent.
    pub(crate) fn initialize_swapchain(
        &mut self,
        preferred_extent: Option<vk::Extent2D>,
    ) -> Result<()> {
        let present_support = unsafe {
            self.surface_loader.get_physical_device_surface_support(
                self.context.physical_device(),
                self.graphics_queue_family,
                self.handle,
            )
        }
        .context("Failed to query present support")?;

        if !present_support {
            bail!("Graphics queue family does not support presentation");
        }

        let surface_info = vk::PhysicalDeviceSurfaceInfo2KHR::default().surface(self.handle);

        let mut surface_capabilities = vk::SurfaceCapabilities2KHR::default();

        unsafe {
            self.surface_capabilities2_loader
                .get_physical_device_surface_capabilities2(
                    self.context.physical_device(),
                    &surface_info,
                    &mut surface_capabilities,
                )
        }
        .context("Failed to query surface capabilities")?;

        let capabilities = surface_capabilities.surface_capabilities;

        let formats = unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(self.context.physical_device(), self.handle)
        }
        .context("Failed to query surface formats")?;

        let present_modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(
                    self.context.physical_device(),
                    self.handle,
                )
        }
        .context("Failed to query present modes")?;

        let (format, is_srgb) = Self::choose_surface_format(&formats)?;

        let (vsync_on_mode, vsync_off_mode) = Self::choose_present_modes(&present_modes)?;

        let extent = Self::choose_extent(&capabilities, preferred_extent);

        // Prefer 2 images for minimal latency, but accept more if the platform requires it.
        let mut image_count = capabilities.min_image_count.max(2);
        if capabilities.max_image_count > 0 {
            image_count = image_count.min(capabilities.max_image_count);
        }

        ensure!(image_count <= 4, "Image count must be smaller than 4");

        let pre_transform = if capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            capabilities.current_transform
        };

        let present_mode = match self.vsync_enabled {
            true => vsync_on_mode,
            false => vsync_off_mode.unwrap_or(vsync_on_mode),
        };

        let mut create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.handle)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(self.swapchain.unwrap_or(vk::SwapchainKHR::null()));

        if self.supports_present_wait2 {
            create_info = create_info.flags(
                vk::SwapchainCreateFlagsKHR::PRESENT_ID_2
                    | vk::SwapchainCreateFlagsKHR::PRESENT_WAIT_2,
            );
        }

        let swapchain = unsafe { self.swapchain_loader.create_swapchain(&create_info, None) }
            .context("Failed to create swapchain")?;

        self.context.set_object_name(c"swapchain", swapchain);

        if let Some(old_swapchain) = self.swapchain.take() {
            self.destroy_swapchain_resources(old_swapchain);
        }

        let images = unsafe { self.swapchain_loader.get_swapchain_images(swapchain) }
            .context("Failed to get swapchain images")?;

        let image_views = Self::create_image_views(&self.context, &images, format.format)?;

        self.swapchain = Some(swapchain);
        self.images = images;
        self.image_views = image_views;
        self.format = format;
        self.is_srgb = is_srgb;
        self.extent = extent;
        self.vsync_on_mode = vsync_on_mode;
        self.vsync_off_mode = vsync_off_mode;

        Ok(())
    }

    /// Recreates the swapchain, typically in response to window resize or out-of-date errors.
    pub(crate) fn recreate(
        &mut self,
        preferred_extent: Option<vk::Extent2D>,
        in_flight_fences: &[vk::Fence],
    ) -> Result<()> {
        self.wait_for_fences(in_flight_fences)
            .context("Failed to wait for in-flight fences")?;

        self.initialize_swapchain(preferred_extent)
    }

    /// Handles a window resize event by immediately recreating the swapchain.
    pub(crate) fn on_resize(
        &mut self,
        width: u32,
        height: u32,
        in_flight_fences: &[vk::Fence],
    ) -> Result<()> {
        let extent = vk::Extent2D { width, height };
        self.recreate(Some(extent), in_flight_fences)
    }

    /// Acquires the next swapchain image with automatic error recovery.
    pub(crate) fn acquire_image(
        &mut self,
        timeout: u64,
        semaphore: vk::Semaphore,
        in_flight_fences: &[vk::Fence],
        frame_resources: &mut [FrameResources],
    ) -> Result<u32> {
        match self.acquire_next_image_internal(timeout, semaphore)? {
            AcquireStatus::Success(index) => Ok(index),
            AcquireStatus::OutOfDate => {
                self.recreate(None, in_flight_fences)?;

                frame_resources
                    .iter_mut()
                    .for_each(|frame_resources| frame_resources.present_wait_id = 0);

                match self.acquire_next_image_internal(timeout, semaphore)? {
                    AcquireStatus::Success(index) | AcquireStatus::Suboptimal(index) => Ok(index),
                    AcquireStatus::OutOfDate => bail!("swapchain still out of date after recreate"),
                }
            }
            AcquireStatus::Suboptimal(index) => Ok(index),
        }
    }

    /// Internal helper to acquire the next swapchain image.
    fn acquire_next_image_internal(
        &self,
        timeout: u64,
        semaphore: vk::Semaphore,
    ) -> Result<AcquireStatus> {
        let swapchain = self
            .swapchain
            .context("swapchain not initialized during acquire")?;

        match unsafe {
            self.swapchain_loader.acquire_next_image(
                swapchain,
                timeout,
                semaphore,
                vk::Fence::null(),
            )
        } {
            Ok((index, false)) => Ok(AcquireStatus::Success(index)),
            Ok((index, true)) => Ok(AcquireStatus::Suboptimal(index)),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(AcquireStatus::OutOfDate),
            Err(vk::Result::ERROR_SURFACE_LOST_KHR) => bail!("surface lost"),
            Err(error) => bail!("failed to acquire next image: {error:?}"),
        }
    }

    /// Presents the swapchain image to the surface.
    pub(crate) fn present(
        &mut self,
        queue: &Queue,
        image_index: u32,
        render_finished_semaphore: vk::Semaphore,
        present_id: &mut u64,
    ) -> Result<()> {
        let swapchain = self
            .swapchain
            .context("swapchain not initialized during present")?;

        let mut present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&render_finished_semaphore))
            .swapchains(std::slice::from_ref(&swapchain))
            .image_indices(std::slice::from_ref(&image_index));

        let present_id_storage: u64;
        let mut present_id2_khr: vk::PresentId2KHR;

        if self.supports_present_wait2 {
            present_id_storage = *present_id;

            present_id2_khr =
                vk::PresentId2KHR::default().present_ids(std::slice::from_ref(&present_id_storage));

            present_info.p_next = &mut present_id2_khr as *mut _ as *mut std::ffi::c_void;
        }

        let queue_handle = queue.lock_handle();

        match unsafe {
            self.swapchain_loader
                .queue_present(*queue_handle, &present_info)
        } {
            Ok(_) => Ok(()),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                *present_id = 0;
                bail!("swapchain out of date during present");
            }
            Err(vk::Result::ERROR_SURFACE_LOST_KHR) => {
                *present_id = 0;
                bail!("surface lost");
            }
            Err(error) => {
                *present_id = 0;
                bail!("failed to present: {error:?}")
            }
        }
    }

    /// Waits for a present operation with the given present ID to complete.
    ///
    /// Returns `true` if the present completed, `false` if it timed out.
    /// Requires VK_KHR_present_wait2 to be supported.
    pub(crate) fn wait_for_present(&self, present_id: u64, timeout: u64) -> Result<bool> {
        if !self.supports_present_wait2 || present_id == 0 {
            return Ok(true);
        }

        let swapchain = self
            .swapchain
            .context("swapchain not initialized during wait_for_present")?;

        let loader = self.context.present_wait2().context(
            "present_wait2 loader not available despite supports_present_wait being true",
        )?;

        let wait_info = vk::PresentWait2InfoKHR::default()
            .present_id(present_id)
            .timeout(timeout);

        match unsafe { loader.wait_for_present2(swapchain, &wait_info) } {
            Ok(_) => Ok(true),
            Err(vk::Result::TIMEOUT) => Ok(false),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                common::debug!("swapchain out of date during wait_for_present2");
                Ok(true)
            }
            Err(vk::Result::ERROR_SURFACE_LOST_KHR) => {
                bail!("surface lost during wait_for_present2")
            }
            Err(error) => {
                bail!("wait_for_present2 failed: {error:?}")
            }
        }
    }

    /// Waits for the given fences to complete.
    fn wait_for_fences(&self, fences: &[vk::Fence]) -> Result<()> {
        Ok(unsafe {
            self.context
                .device()
                .wait_for_fences(fences, true, u64::MAX)
        }
        .context("Failed to wait for fences")?)
    }

    /// Chooses the best surface format from available formats.
    /// Boolean signals if a sRGB format was chosen.
    fn choose_surface_format(
        formats: &[vk::SurfaceFormatKHR],
    ) -> Result<(vk::SurfaceFormatKHR, bool)> {
        if formats.is_empty() {
            bail!("no suitable surface format found");
        }

        // Prefer sRGB non-linear format with BGRA8 or RGBA8.
        for format in formats {
            if format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                && (format.format == vk::Format::B8G8R8A8_SRGB
                    || format.format == vk::Format::R8G8B8A8_SRGB)
            {
                return Ok((*format, true));
            }
        }

        Ok((formats[0], false))
    }

    /// Chooses present modes for V-Sync ON and V-Sync OFF.
    fn choose_present_modes(
        modes: &[vk::PresentModeKHR],
    ) -> Result<(vk::PresentModeKHR, Option<vk::PresentModeKHR>)> {
        if modes.is_empty() {
            bail!("no suitable present mode found");
        }

        // V-Sync ON is always FIFO (guaranteed by Vulkan spec).
        let vsync_on_mode = vk::PresentModeKHR::FIFO;

        // V-Sync OFF prefers MAILBOX (lower latency, no tearing), else IMMEDIATE.
        let vsync_off_mode = if modes.contains(&vk::PresentModeKHR::MAILBOX) {
            Some(vk::PresentModeKHR::MAILBOX)
        } else if modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
            Some(vk::PresentModeKHR::IMMEDIATE)
        } else {
            None
        };

        Ok((vsync_on_mode, vsync_off_mode))
    }

    /// Chooses the swapchain extent based on capabilities and preference.
    fn choose_extent(
        capabilities: &vk::SurfaceCapabilitiesKHR,
        preferred_extent: Option<vk::Extent2D>,
    ) -> vk::Extent2D {
        if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else if let Some(extent) = preferred_extent {
            vk::Extent2D {
                width: extent.width.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: extent.height.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        } else {
            capabilities.min_image_extent
        }
    }

    /// Creates image views for all swapchain images.
    fn create_image_views(
        context: &Rc<Context>,
        images: &[vk::Image],
        format: vk::Format,
    ) -> Result<Vec<vk::ImageView>> {
        images
            .iter()
            .enumerate()
            .map(|(i, &image)| {
                let create_info = vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::IDENTITY,
                        g: vk::ComponentSwizzle::IDENTITY,
                        b: vk::ComponentSwizzle::IDENTITY,
                        a: vk::ComponentSwizzle::IDENTITY,
                    })
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                let view = unsafe { context.device().create_image_view(&create_info, None) }
                    .context("Failed to create image view")?;

                if let Ok(name) = std::ffi::CString::new(format!("swapchain_image_view_{i}")) {
                    context.set_object_name(&name, view);
                }

                Ok(view)
            })
            .collect()
    }

    /// Destroys swapchain resources (image views and swapchain).
    fn destroy_swapchain_resources(&mut self, swapchain: vk::SwapchainKHR) {
        for view in self.image_views.drain(..) {
            unsafe { self.context.device().destroy_image_view(view, None) };
        }

        self.images.clear();

        unsafe { self.swapchain_loader.destroy_swapchain(swapchain, None) };
    }

    /// Returns the current swapchain extent.
    #[inline]
    pub(crate) fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    /// Returns the swapchain surface format.
    #[inline]
    pub(crate) fn format(&self) -> vk::Format {
        self.format.format
    }

    /// Returns a reference to the swapchain images.
    #[inline]
    pub(crate) fn images(&self) -> &[vk::Image] {
        &self.images
    }

    /// Returns a reference to the swapchain image views.
    #[inline]
    pub(crate) fn image_views(&self) -> &[vk::ImageView] {
        &self.image_views
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        if let Some(swapchain) = self.swapchain.take() {
            self.destroy_swapchain_resources(swapchain);
        }

        unsafe { self.surface_loader.destroy_surface(self.handle, None) };
    }
}
