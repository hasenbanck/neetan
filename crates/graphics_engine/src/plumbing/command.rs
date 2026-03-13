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

    /// Inserts a pipeline barrier using synchronization2.
    pub(crate) fn pipeline_barrier2(&self, dependency_info: &vk::DependencyInfo) {
        unsafe {
            self.context
                .device()
                .cmd_pipeline_barrier2(self.buffer, dependency_info)
        };
    }

    /// Begins a debug label region.
    pub(crate) fn begin_debug_label(&self, label: impl Into<Cow<'static, CStr>>, color: [f32; 4]) {
        let label = label.into();
        let label_info = vk::DebugUtilsLabelEXT::default()
            .label_name(&label)
            .color(color);
        unsafe {
            self.context
                .debug_utils_device()
                .cmd_begin_debug_utils_label(self.buffer, &label_info)
        };
    }

    /// Ends a debug label region.
    pub(crate) fn end_debug_label(&self) {
        unsafe {
            self.context
                .debug_utils_device()
                .cmd_end_debug_utils_label(self.buffer)
        };
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
    /// Begins dynamic rendering. Returns an encoder that ends rendering on drop.
    pub(crate) fn begin_rendering(
        &'a self,
        rendering_info: &vk::RenderingInfo,
    ) -> RenderingEncoder<'a> {
        unsafe {
            self.context
                .device()
                .cmd_begin_rendering(self.buffer, rendering_info)
        };
        RenderingEncoder { encoder: self }
    }

    /// Sets whether rasterizer discard is enabled.
    pub(crate) fn set_rasterizer_discard_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_rasterizer_discard_enable(self.buffer, enable)
        };
    }

    /// Sets the cull mode.
    pub(crate) fn set_cull_mode(&self, cull_mode: vk::CullModeFlags) {
        unsafe {
            self.context
                .device()
                .cmd_set_cull_mode(self.buffer, cull_mode)
        };
    }

    /// Sets whether depth testing is enabled.
    pub(crate) fn set_depth_test_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_depth_test_enable(self.buffer, enable)
        };
    }

    /// Sets whether depth writing is enabled.
    pub(crate) fn set_depth_write_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_depth_write_enable(self.buffer, enable)
        };
    }

    /// Sets the front face orientation.
    pub(crate) fn set_front_face(&self, front_face: vk::FrontFace) {
        unsafe {
            self.context
                .device()
                .cmd_set_front_face(self.buffer, front_face)
        };
    }

    /// Sets whether stencil testing is enabled.
    pub(crate) fn set_stencil_test_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_stencil_test_enable(self.buffer, enable)
        };
    }

    /// Sets whether depth bias is enabled.
    pub(crate) fn set_depth_bias_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_depth_bias_enable(self.buffer, enable)
        };
    }

    /// Sets the primitive topology.
    pub(crate) fn set_primitive_topology(&self, topology: vk::PrimitiveTopology) {
        unsafe {
            self.context
                .device()
                .cmd_set_primitive_topology(self.buffer, topology)
        };
    }

    /// Sets whether primitive restart is enabled.
    pub(crate) fn set_primitive_restart_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_primitive_restart_enable(self.buffer, enable)
        };
    }

    /// Sets the depth compare operation.
    pub(crate) fn set_depth_compare_op(&self, op: vk::CompareOp) {
        unsafe {
            self.context
                .device()
                .cmd_set_depth_compare_op(self.buffer, op)
        };
    }

    /// Sets the line width for line rasterization.
    pub(crate) fn set_line_width(&self, line_width: f32) {
        unsafe {
            self.context
                .device()
                .cmd_set_line_width(self.buffer, line_width)
        };
    }

    /// Sets depth bias parameters.
    pub(crate) fn set_depth_bias(&self, constant_factor: f32, clamp: f32, slope_factor: f32) {
        unsafe {
            self.context.device().cmd_set_depth_bias(
                self.buffer,
                constant_factor,
                clamp,
                slope_factor,
            )
        };
    }

    /// Sets the blend constant color.
    pub(crate) fn set_blend_constants(&self, blend_constants: [f32; 4]) {
        unsafe {
            self.context
                .device()
                .cmd_set_blend_constants(self.buffer, &blend_constants)
        };
    }

    /// Sets the depth bounds test range.
    pub(crate) fn set_depth_bounds(&self, min: f32, max: f32) {
        unsafe {
            self.context
                .device()
                .cmd_set_depth_bounds(self.buffer, min, max)
        };
    }

    /// Sets whether depth bounds testing is enabled.
    pub(crate) fn set_depth_bounds_test_enable(&self, enable: bool) {
        unsafe {
            self.context
                .device()
                .cmd_set_depth_bounds_test_enable(self.buffer, enable)
        };
    }

    /// Sets the stencil compare mask.
    pub(crate) fn set_stencil_compare_mask(
        &self,
        face_mask: vk::StencilFaceFlags,
        compare_mask: u32,
    ) {
        unsafe {
            self.context
                .device()
                .cmd_set_stencil_compare_mask(self.buffer, face_mask, compare_mask)
        };
    }

    /// Sets the stencil write mask.
    pub(crate) fn set_stencil_write_mask(&self, face_mask: vk::StencilFaceFlags, write_mask: u32) {
        unsafe {
            self.context
                .device()
                .cmd_set_stencil_write_mask(self.buffer, face_mask, write_mask)
        };
    }

    /// Sets the stencil reference value.
    pub(crate) fn set_stencil_reference(&self, face_mask: vk::StencilFaceFlags, reference: u32) {
        unsafe {
            self.context
                .device()
                .cmd_set_stencil_reference(self.buffer, face_mask, reference)
        };
    }

    /// Sets the stencil test operations.
    pub(crate) fn set_stencil_op(
        &self,
        face_mask: vk::StencilFaceFlags,
        fail_op: vk::StencilOp,
        pass_op: vk::StencilOp,
        depth_fail_op: vk::StencilOp,
        compare_op: vk::CompareOp,
    ) {
        unsafe {
            self.context.device().cmd_set_stencil_op(
                self.buffer,
                face_mask,
                fail_op,
                pass_op,
                depth_fail_op,
                compare_op,
            )
        };
    }

    /// Sets Vulkan 1.3 core dynamic states to their default values.
    ///
    /// Call this once after creating the encoder, before any render passes.
    /// Blend state, polygon mode, and multisample state are baked into pipelines.
    pub(crate) fn set_default_dynamic_state(&mut self) {
        self.set_depth_test_enable(false);
        self.set_depth_compare_op(vk::CompareOp::ALWAYS);
        self.set_depth_write_enable(false);
        self.set_depth_bias_enable(false);
        self.set_depth_bias(0.0, 0.0, 0.0);
        self.set_depth_bounds(0.0, 1.0);
        self.set_depth_bounds_test_enable(false);
        self.set_stencil_test_enable(false);
        self.set_stencil_compare_mask(vk::StencilFaceFlags::FRONT_AND_BACK, 0xFF);
        self.set_stencil_write_mask(vk::StencilFaceFlags::FRONT_AND_BACK, 0xFF);
        self.set_stencil_reference(vk::StencilFaceFlags::FRONT_AND_BACK, 0x00);
        self.set_stencil_op(
            vk::StencilFaceFlags::FRONT_AND_BACK,
            vk::StencilOp::KEEP,
            vk::StencilOp::KEEP,
            vk::StencilOp::KEEP,
            vk::CompareOp::ALWAYS,
        );
        self.set_rasterizer_discard_enable(false);
        self.set_cull_mode(vk::CullModeFlags::NONE);
        self.set_front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        self.set_primitive_topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        self.set_primitive_restart_enable(false);
        self.set_blend_constants([0.0, 0.0, 0.0, 0.0]);
        self.set_line_width(1.0);
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

    /// Inserts an image memory barrier for a color image layout transition.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image_barrier(
        &mut self,
        image: vk::Image,
        src_stage: vk::PipelineStageFlags2,
        src_access: vk::AccessFlags2,
        dst_stage: vk::PipelineStageFlags2,
        dst_access: vk::AccessFlags2,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
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
        self.pipeline_barrier2(
            &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier)),
        );
    }
}

/// RAII guard for dynamic rendering scope.
///
/// Calls `cmd_end_rendering` on drop.
pub(crate) struct RenderingEncoder<'a> {
    encoder: &'a CommandEncoder<'a>,
}

impl RenderingEncoder<'_> {
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

    /// Sets the viewport with count (required for shader objects).
    pub(crate) fn set_viewport(&self, viewports: &[vk::Viewport]) {
        unsafe {
            self.encoder
                .context
                .device()
                .cmd_set_viewport_with_count(self.encoder.buffer, viewports)
        };
    }

    /// Sets the scissor rectangles with count (required for shader objects).
    pub(crate) fn set_scissor(&self, scissors: &[vk::Rect2D]) {
        unsafe {
            self.encoder
                .context
                .device()
                .cmd_set_scissor_with_count(self.encoder.buffer, scissors)
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

impl Drop for RenderingEncoder<'_> {
    fn drop(&mut self) {
        unsafe {
            self.encoder
                .context
                .device()
                .cmd_end_rendering(self.encoder.buffer)
        };
    }
}
