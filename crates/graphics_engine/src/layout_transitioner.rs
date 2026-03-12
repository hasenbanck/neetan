//! Image layout transition helper.

use std::{ffi::CStr, rc::Rc};

use common::Context as _;
use jay_ash::vk;

use crate::{
    Result,
    plumbing::{CommandBuffer, CommandPool, Context, Fence, Queue},
};

/// Submits image layout transitions on the graphics queue.
pub(crate) struct LayoutTransitioner {
    /// The graphics queue.
    graphics_queue: Queue,
    /// Command buffer for transfer operations.
    command_buffer: CommandBuffer,
    /// Fence for submission synchronization.
    fence: Fence,
    /// Command pool for transfer operations.
    _command_pool: CommandPool,
    /// Reference to the Vulkan context.
    context: Rc<Context>,
}

impl LayoutTransitioner {
    /// Creates a new LayoutTransitioner.
    ///
    /// # Arguments
    ///
    /// * `context` - The Vulkan context.
    /// * `name` - Debug name prefix for internal resources.
    /// * `graphics_queue` - The graphics queue to use for layout transitions.
    pub(crate) fn new(context: Rc<Context>, name: &CStr, graphics_queue: &Queue) -> Result<Self> {
        let graphics_queue = graphics_queue.clone();

        let command_pool = CommandPool::new(Rc::clone(&context), name, &graphics_queue)
            .context("Failed to create command pool for layout transitioner")?;

        let command_buffer = command_pool
            .create_command_buffer(name)
            .context("Failed to create command buffer for layout transitioner")?;

        let fence = Fence::new(Rc::clone(&context), name, false)
            .context("Failed to create fence for layout transitioner")?;

        Ok(Self {
            context,
            graphics_queue,
            command_buffer,
            fence,
            _command_pool: command_pool,
        })
    }

    /// Transitions an image from one layout to another using a command buffer.
    pub(crate) fn transition_layout(
        &mut self,
        handle: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<()> {
        self.command_buffer.reset()?;
        let encoder = self.command_buffer.record()?;

        self.record_image_barrier(
            self.command_buffer.handle(),
            handle,
            old_layout,
            new_layout,
            subresource_range,
        );

        drop(encoder);

        let command_buffer_info =
            vk::CommandBufferSubmitInfo::default().command_buffer(self.command_buffer.handle());

        let command_buffers = [command_buffer_info];

        let submit_info = vk::SubmitInfo2::default().command_buffer_infos(&command_buffers);

        let submits = [submit_info];

        self.graphics_queue
            .submit(&submits, self.fence.handle())
            .context("Failed to submit layout transition")?;

        self.fence
            .wait(u64::MAX)
            .context("Failed to wait for layout transition fence")?;

        self.fence
            .reset()
            .context("Failed to reset layout transition fence")?;

        Ok(())
    }

    /// Records an image layout barrier to an active command buffer.
    fn record_image_barrier(
        &self,
        command_buffer: vk::CommandBuffer,
        handle: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) {
        let (src_stage, src_access, dst_stage, dst_access) =
            layout_transition_masks(old_layout, new_layout);

        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(handle)
            .subresource_range(subresource_range);

        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));

        unsafe {
            self.context
                .device()
                .cmd_pipeline_barrier2(command_buffer, &dependency_info);
        }
    }
}

/// Determines stage and access masks for layout transitions on the transfer queue.
fn layout_transition_masks(
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> (
    vk::PipelineStageFlags2,
    vk::AccessFlags2,
    vk::PipelineStageFlags2,
    vk::AccessFlags2,
) {
    let (src_stage, src_access) = match old_layout {
        vk::ImageLayout::UNDEFINED => {
            (vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::NONE)
        }
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => (
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
        // For all other layouts, use generic masks.
        _ => (
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
        ),
    };

    let (dst_stage, dst_access) = match new_layout {
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => (
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
        // For all other layouts (GENERAL, SHADER_READ_ONLY_OPTIMAL, etc.),
        // use generic masks safe.
        _ => (
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::MEMORY_READ,
        ),
    };

    (src_stage, src_access, dst_stage, dst_access)
}
