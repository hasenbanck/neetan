//! Command pools, command buffers, and command encoders.

use std::{borrow::Cow, ffi::CStr, rc::Rc};

use common::Context as _;
use jay_ash::vk;

use crate::plumbing::{Context, GraphicsPipeline, Queue};

/// A command pool for allocating command buffers.
pub(crate) struct CommandPool {
    pub(crate) command_pool: vk::CommandPool,
    pub(crate) context: Rc<Context>,
}

impl CommandPool {
    /// Creates a new command pool associated with the given queue.
    pub(crate) fn new(context: Rc<Context>, name: &CStr, queue: &Queue) -> crate::Result<Self> {
        let pool_create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue.family_index())
            .flags(
                vk::CommandPoolCreateFlags::TRANSIENT
                    | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            );

        let command_pool = unsafe {
            context
                .device()
                .create_command_pool(&pool_create_info, None)
                .context("Failed to create command pool")?
        };

        context.set_object_name(name, command_pool);

        Ok(Self {
            command_pool,
            context,
        })
    }

    /// Creates a new command buffer from this pool.
    pub(crate) fn create_command_buffer(&self, name: &CStr) -> crate::Result<CommandBuffer> {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let buffers = unsafe {
            self.context
                .device()
                .allocate_command_buffers(&alloc_info)
                .context("Failed to allocate command buffer")?
        };

        self.context.set_object_name(name, buffers[0]);

        Ok(CommandBuffer {
            handle: buffers[0],
            pool: self.command_pool,
            context: Rc::clone(&self.context),
        })
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device()
                .destroy_command_pool(self.command_pool, None);
        }
    }
}

/// A command buffer allocated from a command pool.
pub(crate) struct CommandBuffer {
    handle: vk::CommandBuffer,
    pool: vk::CommandPool,
    context: Rc<Context>,
}

impl CommandBuffer {
    /// Returns the raw Vulkan command buffer handle.
    pub(crate) fn handle(&self) -> vk::CommandBuffer {
        self.handle
    }

    /// Resets the command buffer to initial state.
    pub(crate) fn reset(&self) -> crate::Result<()> {
        unsafe {
            self.context
                .device()
                .reset_command_buffer(self.handle, vk::CommandBufferResetFlags::empty())
                .context("Failed to reset command buffer")?;
        }
        Ok(())
    }

    /// Begins recording commands. Returns an encoder that ends recording on drop.
    pub(crate) fn record(&self) -> crate::Result<CommandEncoder<'_>> {
        CommandEncoder::new(&self.context, self.handle)
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device()
                .free_command_buffers(self.pool, &[self.handle]);
        }
    }
}

/// RAII command encoder that begins recording on creation and ends on drop.
pub(crate) struct CommandEncoder<'a> {
    context: &'a Context,
    buffer: vk::CommandBuffer,
}

impl<'a> CommandEncoder<'a> {
    fn new(context: &'a Context, buffer: vk::CommandBuffer) -> crate::Result<Self> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            context
                .device()
                .begin_command_buffer(buffer, &begin_info)
                .context("Failed to begin command buffer")?;
        }

        Ok(Self { context, buffer })
    }

    /// Begins a debug label region.
    pub(crate) fn begin_debug_label(&self, label: impl Into<Cow<'static, CStr>>, color: [f32; 4]) {
        let label = label.into();
        let label_info = vk::DebugUtilsLabelEXT::default()
            .label_name(&label)
            .color(color);
        if let Some(debug_utils_device) = self.context.debug_utils_device() {
            unsafe {
                debug_utils_device.cmd_begin_debug_utils_label(self.buffer, &label_info);
            }
        }
    }

    /// Ends a debug label region.
    pub(crate) fn end_debug_label(&self) {
        if let Some(debug_utils_device) = self.context.debug_utils_device() {
            unsafe {
                debug_utils_device.cmd_end_debug_utils_label(self.buffer);
            }
        }
    }
}

impl Drop for CommandEncoder<'_> {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device()
                .end_command_buffer(self.buffer)
                .expect("failed to end command buffer");
        }
    }
}

impl<'a> CommandEncoder<'a> {
    /// Begins a render pass. Returns an encoder that ends the render pass on drop.
    pub(crate) fn begin_render_pass(
        &'a self,
        render_pass: vk::RenderPass,
        framebuffer: vk::Framebuffer,
        render_area: vk::Rect2D,
        clear_values: &'a [vk::ClearValue],
    ) -> RenderPassEncoder<'a> {
        let begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
            .render_area(render_area)
            .clear_values(clear_values);

        unsafe {
            self.context.device().cmd_begin_render_pass(
                self.buffer,
                &begin_info,
                vk::SubpassContents::INLINE,
            )
        };

        RenderPassEncoder { encoder: self }
    }

    /// Binds descriptor sets to the graphics pipeline bind point.
    pub(crate) fn bind_descriptor_sets(
        &self,
        pipeline_layout: vk::PipelineLayout,
        descriptor_sets: &[vk::DescriptorSet],
    ) {
        unsafe {
            self.context.device().cmd_bind_descriptor_sets(
                self.buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                0,
                descriptor_sets,
                &[],
            )
        };
    }

    /// Records a buffer-to-image copy of `width x height` pixels into the
    /// color aspect of `dst` (mip 0, layer 0). The image must be in
    /// `TRANSFER_DST_OPTIMAL` layout when this is recorded.
    pub(crate) fn copy_buffer_to_image(
        &mut self,
        src: vk::Buffer,
        dst: vk::Image,
        width: u32,
        height: u32,
    ) {
        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        unsafe {
            self.context.device().cmd_copy_buffer_to_image(
                self.buffer,
                src,
                dst,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&region),
            );
        }
    }

    /// Inserts an image memory barrier for a color image layout transition.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image_barrier(
        &mut self,
        image: vk::Image,
        src_stage: vk::PipelineStageFlags,
        src_access: vk::AccessFlags,
        dst_stage: vk::PipelineStageFlags,
        dst_access: vk::AccessFlags,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(src_access)
            .dst_access_mask(dst_access)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        unsafe {
            self.context.device().cmd_pipeline_barrier(
                self.buffer,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier),
            );
        }
    }
}

/// RAII guard for a render pass scope.
///
/// Calls `cmd_end_render_pass` on drop.
pub(crate) struct RenderPassEncoder<'a> {
    encoder: &'a CommandEncoder<'a>,
}

impl RenderPassEncoder<'_> {
    /// Binds a graphics pipeline.
    pub(crate) fn bind_pipeline(&self, pipeline: &GraphicsPipeline) {
        unsafe {
            self.encoder.context.device().cmd_bind_pipeline(
                self.encoder.buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline(),
            )
        };
    }

    /// Sets the viewport.
    pub(crate) fn set_viewport(&self, viewports: &[vk::Viewport]) {
        unsafe {
            self.encoder
                .context
                .device()
                .cmd_set_viewport(self.encoder.buffer, 0, viewports)
        };
    }

    /// Sets the scissor rectangles.
    pub(crate) fn set_scissor(&self, scissors: &[vk::Rect2D]) {
        unsafe {
            self.encoder
                .context
                .device()
                .cmd_set_scissor(self.encoder.buffer, 0, scissors)
        };
    }

    /// Pushes constants to the command buffer.
    pub(crate) fn push_constants(
        &self,
        pipeline_layout: vk::PipelineLayout,
        stage_flags: vk::ShaderStageFlags,
        offset: u32,
        constants: &[u8],
    ) {
        unsafe {
            self.encoder.context.device().cmd_push_constants(
                self.encoder.buffer,
                pipeline_layout,
                stage_flags,
                offset,
                constants,
            )
        };
    }

    /// Draws primitives.
    pub(crate) fn draw(
        &self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        unsafe {
            self.encoder.context.device().cmd_draw(
                self.encoder.buffer,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            )
        };
    }
}

impl Drop for RenderPassEncoder<'_> {
    fn drop(&mut self) {
        unsafe {
            self.encoder
                .context
                .device()
                .cmd_end_render_pass(self.encoder.buffer)
        };
    }
}
