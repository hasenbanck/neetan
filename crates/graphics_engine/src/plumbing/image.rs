use std::{ffi::CStr, rc::Rc};

use common::Context as _;
use jay_ash::vk;

use super::memory::{self, MemoryBlock};
use crate::{
    Result,
    layout_transitioner::LayoutTransitioner,
    plumbing::{Context, IntoCString},
};

/// A 2D color target image for rasterization rendering and subsequent sampling.
///
/// This image is created with `COLOR_ATTACHMENT | SAMPLED` usage, suitable for
/// render targets that are written by fragment shaders and later read/sampled.
pub(crate) struct ColorTargetImage {
    handle: vk::Image,
    view: vk::ImageView,
    extent: vk::Extent3D,
    memory_block: Option<MemoryBlock>,
    context: Rc<Context>,
}

impl ColorTargetImage {
    /// Creates a new 2D color target image.
    ///
    /// The image is created with `COLOR_ATTACHMENT | SAMPLED` usage and transitioned
    /// to `SHADER_READ_ONLY_OPTIMAL` layout. Render passes transition to
    /// `COLOR_ATTACHMENT_OPTIMAL` and back via explicit barriers.
    pub(crate) fn new(
        context: Rc<Context>,
        name: &CStr,
        layout_transitioner: &mut LayoutTransitioner,
        format: vk::Format,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let handle = unsafe {
            context
                .device()
                .create_image(&image_create_info, None)
                .context("Failed to create color target image")?
        };

        let memory_requirements = unsafe { context.device().get_image_memory_requirements(handle) };

        let request = memory::Request {
            size: memory_requirements.size,
            align_mask: memory_requirements.alignment - 1,
            usage: memory::UsageFlags::FAST_DEVICE_ACCESS,
            memory_types: memory_requirements.memory_type_bits,
        };

        let memory_block = unsafe {
            match context.allocator().alloc(context.device(), request) {
                Ok(block) => block,
                Err(error) => {
                    context.device().destroy_image(handle, None);
                    return Err(error)
                        .context("Failed to allocate memory for color target image")?;
                }
            }
        };

        unsafe {
            if let Err(error) = context.device().bind_image_memory(
                handle,
                *memory_block.memory(),
                memory_block.offset(),
            ) {
                context.device().destroy_image(handle, None);
                context.allocator().dealloc(context.device(), memory_block);

                return Err(error).context("Failed to bind memory for color target image")?;
            }
        }

        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(handle)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping::default())
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );

        let view = match unsafe { context.device().create_image_view(&view_create_info, None) } {
            Ok(view) => view,
            Err(error) => {
                unsafe {
                    context.device().destroy_image(handle, None);
                    context.allocator().dealloc(context.device(), memory_block);
                }
                return Err(error).context("Failed to create color target image view")?;
            }
        };

        context.set_object_name(name, handle);
        let view_name = format!("{}_view", name.to_string_lossy()).into_cstring();
        context.set_object_name(&view_name, view);

        let initial_image_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

        let subresource_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        if let Err(error) = layout_transitioner.transition_layout(
            handle,
            vk::ImageLayout::UNDEFINED,
            initial_image_layout,
            subresource_range,
        ) {
            unsafe {
                context.device().destroy_image_view(view, None);
                context.device().destroy_image(handle, None);
                context.allocator().dealloc(context.device(), memory_block);
            }
            return Err(error)
                .context("Failed to transition color target image to initial layout")?;
        }

        Ok(Self {
            handle,
            view,
            extent,
            memory_block: Some(memory_block),
            context,
        })
    }

    /// Returns the image view.
    #[inline]
    pub(crate) fn view(&self) -> vk::ImageView {
        self.view
    }

    /// Returns the dimensions of the image.
    #[inline]
    pub(crate) fn extent(&self) -> vk::Extent3D {
        self.extent
    }

    /// Returns the raw image handle.
    #[inline]
    pub(crate) fn handle(&self) -> vk::Image {
        self.handle
    }

    /// Consumes the image and returns its raw parts without cleanup.
    ///
    /// The caller is responsible for eventually destroying the image view,
    /// image, and freeing the memory.
    ///
    /// Returns `(image_handle, view_handle, memory_block)`.
    pub(crate) fn into_raw_parts(mut self) -> (vk::Image, vk::ImageView, MemoryBlock) {
        let handle = self.handle;
        let view = self.view;
        let memory = self
            .memory_block
            .take()
            .expect("color target image memory already taken");
        // Prevent Drop from running.
        std::mem::forget(self);
        (handle, view, memory)
    }
}

impl Drop for ColorTargetImage {
    fn drop(&mut self) {
        unsafe {
            self.context.device().destroy_image_view(self.view, None);
            self.context.device().destroy_image(self.handle, None);

            if let Some(memory_block) = self.memory_block.take() {
                self.context
                    .allocator()
                    .dealloc(self.context.device(), memory_block);
            }
        }
    }
}
