#[allow(clippy::module_inception)]
mod blitter;

use jay_ash::vk;

pub(crate) use self::blitter::Blitter;
use crate::{
    passes::Renderer,
    plumbing::{CommandEncoder, FrameTarget},
};

/// Composites the color_target to the swapchain frame.
pub(crate) fn render_blitter_pass(
    encoder: &mut CommandEncoder,
    surface_extent: vk::Extent2D,
    color_target_extent: vk::Extent2D,
    frame: &FrameTarget,
    blitter: &Blitter,
    pipeline_layout: vk::PipelineLayout,
) {
    let offset_x = surface_extent
        .width
        .saturating_sub(color_target_extent.width)
        / 2;
    let offset_y = surface_extent
        .height
        .saturating_sub(color_target_extent.height)
        / 2;

    // Transition from UNDEFINED to GENERAL.
    // Use COLOR_ATTACHMENT_OUTPUT as srcStageMask to synchronize with the
    // semaphore wait from vkAcquireNextImageKHR.
    encoder.image_barrier(
        frame.image(),
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::NONE,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::GENERAL,
    );

    let color_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(frame.view())
        .image_layout(vk::ImageLayout::GENERAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        });

    let rendering_info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: surface_extent,
        })
        .layer_count(1)
        .color_attachments(std::slice::from_ref(&color_attachment));

    {
        let rendering_encoder = encoder.begin_rendering(&rendering_info);

        rendering_encoder.set_viewport(&[vk::Viewport {
            x: offset_x as f32,
            y: offset_y as f32,
            width: color_target_extent.width as f32,
            height: color_target_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }]);
        rendering_encoder.set_scissor(&[vk::Rect2D {
            offset: vk::Offset2D {
                x: offset_x as i32,
                y: offset_y as i32,
            },
            extent: color_target_extent,
        }]);

        let mut push_bytes = [0u8; 8];
        push_bytes[0..4].copy_from_slice(&(offset_x as i32).to_le_bytes());
        push_bytes[4..8].copy_from_slice(&(offset_y as i32).to_le_bytes());
        rendering_encoder.push_constants(
            pipeline_layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            &push_bytes,
        );

        blitter.render(&rendering_encoder, ());
    }

    // Transition to PRESENT_SRC_KHR for swapchain present (still required).
    encoder.image_barrier(
        frame.image(),
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::NONE,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::PRESENT_SRC_KHR,
    );
}
