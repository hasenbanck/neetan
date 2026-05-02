//! The graphics engine.
#![deny(missing_docs)]

mod descriptors;
mod errors;
mod instructions;
mod layout_transitioner;
mod passes;
mod pipeline_loader;
mod plumbing;
mod resources;

use std::{
    ffi::{CString, c_char},
    rc::Rc,
};

use common::{Context, OptionContext, StackVec, bail, error, info};
pub use errors::Error;
pub use instructions::RenderInstructions;
use jay_ash::vk;

use crate::{
    descriptors::{DescriptorResources, FrameDescriptorSets},
    layout_transitioner::LayoutTransitioner,
    passes::{
        Blitter, Crt, Scale, clear_frame_pass, render_blitter_pass, render_crt_pass,
        render_scale_pass,
    },
    pipeline_loader::PipelineLoader,
    plumbing::{
        Binary, CommandPool, Device, DeviceConfiguration, Fence, FrameResources, FrameTarget,
        IntoCString, MappedBuffer, RenderPass, Semaphore, Surface, Timeline,
    },
    resources::Resources,
};

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;

const INITIAL_WINDOW_WIDTH: u32 = 1280;
const INITIAL_WINDOW_HEIGHT_4_BY_3: u32 = 960;
const INITIAL_WINDOW_HEIGHT_1_BY_1: u32 = 800;
const NATIVE_WIDTH: u32 = 640;
const NATIVE_HEIGHT: u32 = 480;
const FRAMEBUFFER_BYTES: u64 = (NATIVE_WIDTH * NATIVE_HEIGHT * 4) as u64;

/// Display aspect mode for scaling and startup dimensions.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DisplayAspectMode {
    /// Pixel aspect correction: 640x400 is presented as 4:3.
    Aspect4By3,
    /// Square pixels: native 640x400 maps to 1:1 pixel aspect.
    Aspect1By1,
}

impl DisplayAspectMode {
    fn startup_extent(self) -> (u32, u32) {
        match self {
            Self::Aspect4By3 => (INITIAL_WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT_4_BY_3),
            Self::Aspect1By1 => (INITIAL_WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT_1_BY_1),
        }
    }

    fn display_aspect_ratio(self) -> f64 {
        match self {
            Self::Aspect4By3 => 4.0 / 3.0,
            Self::Aspect1By1 => 640.0 / 400.0,
        }
    }
}

fn compute_color_target_extent(
    surface_width: u32,
    surface_height: u32,
    aspect_ratio: f64,
) -> (u32, u32) {
    let surface_aspect = surface_width as f64 / surface_height as f64;
    if surface_aspect > aspect_ratio {
        let height = surface_height;
        let width = (surface_height as f64 * aspect_ratio).round() as u32;
        (width, height)
    } else {
        let width = surface_width;
        let height = (surface_width as f64 / aspect_ratio).round() as u32;
        (width, height)
    }
}

/// The graphics engine of the game.
pub struct GraphicsEngine {
    /// The global descriptor resources.
    descriptor_resources: DescriptorResources,
    /// General resources of the engine.
    resources: Resources,
    /// Layout transitioner for image layout transitions.
    layout_transitioner: LayoutTransitioner,
    /// Scales the native-resolution image to the window resolution.
    scale: Scale,
    /// Applies CRT scaling from the native-resolution image to the window resolution.
    crt: Crt,
    /// Copies the color target to the swapchain.
    blitter: Option<Blitter>,
    /// Render pass for offscreen color target rendering.
    color_render_pass: RenderPass,
    /// Pipeline loader for creating graphics pipelines.
    pipeline_loader: PipelineLoader,
    /// Pool of frame resources, one per frame slot.
    frame_resources: Option<Vec<FrameResources>>,
    /// Semaphores signaled when rendering is complete, one per swapchain image.
    /// Indexed by swapchain image index (not frame slot) because the presentation
    /// engine holds onto the semaphore until the image is re-acquired.
    render_finished_semaphores: Option<Vec<Semaphore<Binary>>>,
    /// Number of frame resource slots (matches swapchain image count).
    frame_count: usize,
    /// Present ID used for frame pacing,
    global_present_id: u64,
    /// Index of the current frame in the frame_resources array.
    current_frame_index: usize,
    /// Command pool for allocating graphics command buffers.
    graphics_command_pool: Option<CommandPool>,
    /// Graphics queue timeline for graveyard ordering.
    frame_timeline: Rc<Semaphore<Timeline>>,
    /// Current frame timeline value.
    frame_timeline_value: u64,
    /// Surface and swapchain abstraction.
    surface: Option<Surface>,
    /// Vulkan device abstraction.
    device: Device,
    /// Display aspect mode for computing fitted color target extent.
    display_aspect_mode: DisplayAspectMode,
}

impl GraphicsEngine {
    /// Creates a new graphics engine.
    pub fn new(
        platform_extension_names: &[String],
        display_aspect_mode: DisplayAspectMode,
    ) -> Result<Self> {
        let (initial_width, initial_height) = display_aspect_mode.startup_extent();

        let platform_extension_cstrings: Vec<CString> = platform_extension_names
            .iter()
            .map(|name| CString::new(name.as_str()).unwrap())
            .collect();
        let platform_extensions: Vec<*const c_char> = platform_extension_cstrings
            .iter()
            .map(|cstr| cstr.as_ptr())
            .collect();

        let configuration = DeviceConfiguration::new(platform_extensions);
        let device = Device::new(configuration)?;

        let frame_timeline =
            Semaphore::new_timeline(Rc::clone(device.context()), c"frame_timeline", 0)
                .context("Failed to create timeline semaphore")?;
        let frame_timeline = Rc::new(frame_timeline);

        let pipeline_loader = PipelineLoader::new(Rc::clone(device.context()));

        let mut layout_transitioner = LayoutTransitioner::new(
            Rc::clone(device.context()),
            c"layout_transitioner",
            device.graphics_queue(),
        )
        .context("Can't create layout transitioner")?;

        let descriptor_resources = DescriptorResources::new(device.context())
            .context("Can't create descriptor resources")?;

        let color_render_pass = RenderPass::new_color_target(
            Rc::clone(device.context()),
            c"color_target_render_pass",
            vk::Format::R8G8B8A8_SRGB,
        )
        .context("Can't create color target render pass")?;

        let (color_width, color_height) = compute_color_target_extent(
            initial_width,
            initial_height,
            display_aspect_mode.display_aspect_ratio(),
        );
        let resources = Resources::new(
            &device,
            &mut layout_transitioner,
            color_render_pass.handle(),
            color_width,
            color_height,
        )
        .context("Can't initialize resources")?;

        let scale = Scale::new(
            &pipeline_loader,
            color_render_pass.handle(),
            descriptor_resources.pipeline_layout(),
        )
        .context("Can't create scale pipeline")?;

        let crt = Crt::new(
            &pipeline_loader,
            color_render_pass.handle(),
            descriptor_resources.pipeline_layout(),
        )
        .context("Can't create crt pipeline")?;

        info!("Graphics engine initialized");

        let engine = Self {
            descriptor_resources,
            resources,
            layout_transitioner,
            scale,
            crt,
            blitter: None,
            color_render_pass,
            pipeline_loader,
            frame_resources: None,
            render_finished_semaphores: None,
            frame_count: 0,
            global_present_id: 1,
            current_frame_index: 0,
            graphics_command_pool: None,
            frame_timeline,
            frame_timeline_value: 0,
            surface: None,
            device,
            display_aspect_mode,
        };

        Ok(engine)
    }

    /// Returns the raw `VkInstance` handle for interop with external libraries.
    pub fn raw_instance_handle(&self) -> vk::Instance {
        self.device.context().instance().handle()
    }

    /// Called when the window is resuming.
    pub fn on_resume(
        &mut self,
        surface_handle: vk::SurfaceKHR,
        vsync_enabled: bool,
        width: u32,
        height: u32,
    ) {
        if self.surface.is_none() {
            let mut surface = self
                .device
                .create_surface_from_handle(surface_handle, vsync_enabled);

            let preferred_extent = vk::Extent2D { width, height };
            if let Err(error) = surface.initialize_swapchain(Some(preferred_extent)) {
                error!("Failed to initialize swapchain: {error}");
                return;
            }

            let surface_format = surface.format();
            let frame_count = surface.images().len();

            self.surface = Some(surface);

            if let Err(error) = self.initialize_frame_resources(surface_format, frame_count) {
                error!("Failed to initialize frame resources: {error}");
                self.surface = None;
            }
        }
    }

    /// Initializes frame resources for rendering.
    ///
    /// Creates one set of frame resources per swapchain image.
    fn initialize_frame_resources(
        &mut self,
        surface_format: vk::Format,
        frame_count: usize,
    ) -> Result<()> {
        let context = self.device.context().clone();

        let graphics_command_pool = self
            .device
            .graphics_queue()
            .create_command_pool(c"graphics_command_pool")
            .context("Failed to create graphics command pool")?;

        let surface_render_pass = self
            .surface
            .as_ref()
            .context("Surface not initialized")?
            .render_pass();

        if self
            .blitter
            .as_ref()
            .is_none_or(|blitter| !blitter.is_compatible(surface_format, surface_render_pass))
        {
            self.blitter = Some(
                Blitter::new(
                    &self.pipeline_loader,
                    surface_format,
                    surface_render_pass,
                    self.descriptor_resources.pipeline_layout(),
                )
                .context("Can't create blitter")?,
            );
        }

        let frame_resources = (0..frame_count)
            .map(|i| {
                let image_available_semaphore = Semaphore::new_binary(
                    context.clone(),
                    &format!("image_available_semaphore_{i}").into_cstring(),
                )
                .context("Failed to create image available semaphore")?;
                let present_fence = Fence::new(
                    context.clone(),
                    &format!("present_fence_{i}").into_cstring(),
                    true,
                )
                .context("Failed to create present fence")?;
                let graphics_command_buffer = graphics_command_pool
                    .create_command_buffer(&format!("graphics_command_buffer_{i}").into_cstring())
                    .context("Failed to create graphics command buffer")?;

                let mut descriptors = FrameDescriptorSets::new(
                    Rc::clone(&context),
                    &format!("descriptors_{i}").into_cstring(),
                    &self.descriptor_resources,
                )
                .context("Failed to create per-frame descriptors")?;

                let framebuffer_staging = MappedBuffer::new(
                    Rc::clone(&context),
                    &format!("framebuffer_staging_{i}").into_cstring(),
                    vk::BufferUsageFlags::TRANSFER_SRC,
                    FRAMEBUFFER_BYTES,
                    None,
                )
                .context("Failed to create per-frame framebuffer staging buffer")?;

                let mut last_descriptor_version = 0u64;
                self.descriptor_resources.write_stale_descriptors(
                    &mut descriptors,
                    &mut last_descriptor_version,
                    self.resources.descriptor_version(),
                    self.resources.color_target(),
                    self.resources.native_target(),
                );

                Ok(FrameResources {
                    image_available_semaphore,
                    present_fence,
                    graphics_command_buffer,
                    descriptors,
                    framebuffer_staging,
                    last_descriptor_version,
                    present_wait_id: 0,
                })
            })
            .collect::<Result<Vec<FrameResources>>>()?;

        // One render_finished_semaphore per swapchain image, indexed by image index.
        // The presentation engine holds onto this semaphore until the image is re-acquired,
        // so it must be tied to the image, not the frame slot.
        let render_finished_semaphores = (0..frame_count)
            .map(|i| {
                Semaphore::new_binary(
                    context.clone(),
                    &format!("render_finished_semaphore_{i}").into_cstring(),
                )
                .context("Failed to create render finished semaphore")
                .map_err(Error::from)
            })
            .collect::<Result<Vec<Semaphore<Binary>>>>()?;

        self.graphics_command_pool = Some(graphics_command_pool);
        self.frame_resources = Some(frame_resources);
        self.render_finished_semaphores = Some(render_finished_semaphores);
        self.frame_count = frame_count;
        self.current_frame_index = 0;

        info!("Frame resources initialized ({frame_count} frames)");

        Ok(())
    }

    /// Tries to wait for the previous frame's presentation to complete.
    ///
    /// Returns `true` if the previous present completed (or no wait needed),
    /// `false` if it timed out. Uses a short timeout to avoid blocking the
    /// emulation loop.
    pub fn try_wait_for_previous_present(&self, timeout_ms: u64) -> Result<bool> {
        let frame_resources = match self.frame_resources.as_ref() {
            Some(r) => r,
            None => return Ok(true),
        };

        let surface = match self.surface.as_ref() {
            Some(s) => s,
            None => return Ok(true),
        };

        let previous_frame_index =
            (self.current_frame_index + self.frame_count - 1) % self.frame_count;
        let wait_id = frame_resources[previous_frame_index].present_wait_id;

        let timeout_ns = timeout_ms.saturating_mul(1_000_000);
        surface.wait_for_present(wait_id, timeout_ns)
    }

    /// Renders the next frame.
    pub fn render_frame(&mut self, render_instructions: Option<&RenderInstructions>) -> Result<()> {
        let frame = self.acquire_frame()?;

        self.clear_graveyard()?;

        let extent = self
            .surface
            .as_ref()
            .context("Surface not initialized")?
            .extent();

        if extent.width == 0 || extent.height == 0 {
            return Ok(());
        }

        let frame_index = self.current_frame_index;

        {
            let frame_resources = self
                .frame_resources
                .as_mut()
                .context("Frame resources not initialized")?;
            let frame_resources = &mut frame_resources[frame_index];

            // Reset command buffer for this frame.
            frame_resources
                .graphics_command_buffer
                .reset()
                .context("Failed to reset graphics command buffer")?;

            match render_instructions {
                None => {
                    let mut encoder = frame_resources
                        .graphics_command_buffer
                        .record()
                        .context("Failed to create command encoder")?;

                    clear_frame_pass(&mut encoder, extent, &frame);
                }
                Some(render_instructions) => {
                    // Copy the composed framebuffer into the per-frame staging buffer.
                    let framebuffer = render_instructions.framebuffer;
                    debug_assert_eq!(framebuffer.len(), FRAMEBUFFER_BYTES as usize);
                    {
                        let dst = frame_resources
                            .framebuffer_staging
                            .as_mut_slice_at(0, framebuffer.len());
                        dst.copy_from_slice(framebuffer);
                    }
                    frame_resources
                        .framebuffer_staging
                        .flush(0, framebuffer.len() as u64);

                    self.descriptor_resources.write_stale_descriptors(
                        &mut frame_resources.descriptors,
                        &mut frame_resources.last_descriptor_version,
                        self.resources.descriptor_version(),
                        self.resources.color_target(),
                        self.resources.native_target(),
                    );

                    // Render phase
                    let mut encoder = frame_resources
                        .graphics_command_buffer
                        .record()
                        .context("Failed to create command encoder")?;

                    {
                        encoder.begin_debug_label(c"Setup Phase", [0.5, 0.5, 0.5, 1.0]);

                        self.descriptor_resources
                            .bind_descriptors(&encoder, &frame_resources.descriptors);

                        encoder.end_debug_label();
                    }

                    encoder.begin_debug_label(c"Render Phase", [0.0, 0.5, 1.0, 1.0]);

                    // Stage 1 - Image transfer: Copy the composed framebuffer into native target image.
                    {
                        encoder.begin_debug_label(c"Framebuffer Copy", [1.0, 0.0, 0.0, 1.0]);
                        let native_target = self.resources.native_target();
                        encoder.image_barrier(
                            native_target.handle(),
                            vk::PipelineStageFlags::FRAGMENT_SHADER,
                            vk::AccessFlags::SHADER_READ,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_WRITE,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        );
                        encoder.copy_buffer_to_image(
                            frame_resources.framebuffer_staging.raw(),
                            native_target.handle(),
                            NATIVE_WIDTH,
                            NATIVE_HEIGHT,
                        );
                        encoder.image_barrier(
                            native_target.handle(),
                            vk::PipelineStageFlags::TRANSFER,
                            vk::AccessFlags::TRANSFER_WRITE,
                            vk::PipelineStageFlags::FRAGMENT_SHADER,
                            vk::AccessFlags::SHADER_READ,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        );
                        encoder.end_debug_label();
                    }

                    // Stage 2 - Upscale: read native_target, write to color_target (window res).
                    {
                        let native_height = render_instructions.native_height;

                        if render_instructions.crt {
                            encoder.begin_debug_label(c"CRT Pass", [0.0, 1.0, 0.0, 1.0]);
                            render_crt_pass(
                                &mut encoder,
                                self.resources.color_target(),
                                self.color_render_pass.handle(),
                                &self.crt,
                                native_height,
                                self.descriptor_resources.pipeline_layout(),
                            );
                        } else {
                            encoder.begin_debug_label(c"Scale Pass", [0.0, 1.0, 0.0, 1.0]);
                            render_scale_pass(
                                &mut encoder,
                                self.resources.color_target(),
                                self.color_render_pass.handle(),
                                &self.scale,
                                native_height,
                                self.descriptor_resources.pipeline_layout(),
                            );
                        }
                        encoder.end_debug_label();
                    }

                    // Stage 3 - Blit: read color_target, write to swapchain.
                    {
                        encoder.begin_debug_label(c"Blitter Pass", [0.5, 0.5, 1.0, 1.0]);
                        let ext = self.resources.color_target().extent();
                        let color_target_extent = vk::Extent2D {
                            width: ext.width,
                            height: ext.height,
                        };
                        render_blitter_pass(
                            &mut encoder,
                            extent,
                            color_target_extent,
                            &frame,
                            self.blitter.as_ref().context("Blitter not initialized")?,
                            self.descriptor_resources.pipeline_layout(),
                        );
                        encoder.end_debug_label();
                    }

                    encoder.end_debug_label(); // End Render Phase
                }
            }
        }

        let render_finished_semaphore = self
            .render_finished_semaphores
            .as_ref()
            .context("Render finished semaphores not initialized")?[frame.image_index() as usize]
            .handle();

        self.submit_to_graphics_queue(frame_index, render_finished_semaphore)?;
        self.present_frame(frame, render_finished_semaphore)?;

        Ok(())
    }

    fn submit_to_graphics_queue(
        &mut self,
        frame_index: usize,
        render_finished_semaphore: vk::Semaphore,
    ) -> Result<()> {
        let resources = &self
            .frame_resources
            .as_ref()
            .context("Frame resources not initialized")?[frame_index];

        let wait_semaphores = [resources.image_available_semaphore.handle()];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];

        self.frame_timeline_value += 1;

        let signal_semaphores = [render_finished_semaphore, self.frame_timeline.handle()];
        let wait_values = [0];
        let signal_values = [0, self.frame_timeline_value];

        let mut timeline_info = vk::TimelineSemaphoreSubmitInfoKHR::default()
            .wait_semaphore_values(&wait_values)
            .signal_semaphore_values(&signal_values);

        let command_buffers = [resources.graphics_command_buffer.handle()];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)
            .push_next(&mut timeline_info);

        self.device
            .graphics_queue()
            .submit(
                std::slice::from_ref(&submit_info),
                resources.present_fence.handle(),
            )
            .context("Failed to submit command buffer")?;

        Ok(())
    }

    /// Clears old, unused resources.
    fn clear_graveyard(&mut self) -> Result<()> {
        let timeline_value = self
            .frame_timeline
            .get_value()
            .context("Failed to get frame timeline value")?;
        let removal_delay = self.frame_count as u64;
        self.device.clear_graveyard(timeline_value, removal_delay);
        Ok(())
    }

    /// Acquires the next frame for rendering.
    ///
    /// This waits on the current frame's present fence to ensure resources are not in use,
    /// then acquires the next swapchain image using the current frame's semaphores.
    fn acquire_frame(&mut self) -> Result<FrameTarget> {
        let fences = self.collect_present_fences();

        let Some(frame_resources) = self.frame_resources.as_mut() else {
            bail!("Frame resources not initialized");
        };

        let resources = &frame_resources[self.current_frame_index];

        // Wait for THIS frame's present fence to ensure resources are not in use.
        // This also ensures the command buffer from this frame slot has completed.
        resources
            .present_fence
            .wait(u64::MAX)
            .context("Failed to wait on present fence")?;

        let surface = self.surface.as_mut().context("Surface not initialized")?;

        let image_available_semaphore = resources.image_available_semaphore.handle();
        let image_index = surface.acquire_image(
            u64::MAX,
            image_available_semaphore,
            fences.as_ref(),
            frame_resources,
        )?;

        // Reset the fence only after image acquisition succeeds. This avoids a
        // deadlock where recreate() waits on an unsignaled fence that no GPU
        // submission will ever signal.
        frame_resources[self.current_frame_index]
            .present_fence
            .reset()
            .context("Failed to reset present fence")?;

        let framebuffer = surface.framebuffers()[image_index as usize];
        let render_pass = surface.render_pass();

        Ok(FrameTarget::new(image_index, framebuffer, render_pass))
    }

    /// Presents a frame to the swapchain.
    ///
    /// This should be called after the user has submitted their rendering commands.
    /// The `render_finished_semaphore` should have been signaled by the rendering submission.
    fn present_frame(
        &mut self,
        frame: FrameTarget,
        render_finished_semaphore: vk::Semaphore,
    ) -> Result<()> {
        let Some(frame_resources) = self.frame_resources.as_mut() else {
            bail!("Frame resources not initialized");
        };

        self.global_present_id = self.global_present_id.wrapping_add(1);

        let resources = &mut frame_resources[self.current_frame_index];
        resources.present_wait_id = self.global_present_id;

        let surface = self.surface.as_mut().context("Surface not initialized")?;

        surface.present(
            self.device.graphics_queue(),
            frame.image_index(),
            render_finished_semaphore,
            &mut resources.present_wait_id,
        )?;

        self.current_frame_index = (self.current_frame_index + 1) % self.frame_count;

        Ok(())
    }

    /// Collects all present fence handles from frame resources.
    fn collect_present_fences(&self) -> StackVec<vk::Fence, 4> {
        match &self.frame_resources {
            Some(resources) => resources
                .iter()
                .map(|resource| resource.present_fence.handle())
                .collect(),
            None => StackVec::new(),
        }
    }

    /// Handles window resize by immediately recreating the swapchain.
    pub fn on_resize(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }

        let fences = self.collect_present_fences();

        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };

        if let Some(frame_resources) = self.frame_resources.as_mut() {
            frame_resources
                .iter_mut()
                .for_each(|frame_resources| frame_resources.present_wait_id = 0)
        }

        surface.on_resize(width, height, &fences)?;
        let surface_format = surface.format();
        let surface_render_pass = surface.render_pass();

        if self
            .blitter
            .as_ref()
            .is_none_or(|blitter| !blitter.is_compatible(surface_format, surface_render_pass))
        {
            self.blitter = Some(Blitter::new(
                &self.pipeline_loader,
                surface_format,
                surface_render_pass,
                self.descriptor_resources.pipeline_layout(),
            )?);
        }

        let (color_width, color_height) = compute_color_target_extent(
            width,
            height,
            self.display_aspect_mode.display_aspect_ratio(),
        );

        self.resources.on_resize(
            &self.device,
            &mut self.layout_transitioner,
            self.color_render_pass.handle(),
            color_width,
            color_height,
        );

        Ok(())
    }

    /// Called when the window is suspending.
    pub fn on_destroy_surface(&mut self) {
        // Android devices are expected to drop their surface view.
        if cfg!(target_os = "android") {
            self.surface = None;
        }
    }
}

impl Drop for GraphicsEngine {
    fn drop(&mut self) {
        // Wait for all GPU operations to complete before dropping resources.
        // This prevents validation errors from destroying resources still in use.
        let _ = unsafe { self.device.context().device().device_wait_idle() };
        // Flush all deferred resources from the graveyard to properly deallocate GPU memory.
        self.device.clear_graveyard(u64::MAX, 0);
    }
}
