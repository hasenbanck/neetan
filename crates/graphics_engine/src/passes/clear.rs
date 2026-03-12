use jay_ash::vk;

use crate::plumbing::{CommandEncoder, FrameTarget};

/// Clears the frame image.
pub(crate) fn clear_frame_pass(
    encoder: &mut CommandEncoder,
    extent: vk::Extent2D,
    frame: &FrameTarget,
) {
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
            extent,
        })
        .layer_count(1)
        .color_attachments(std::slice::from_ref(&color_attachment));

    encoder.begin_rendering(&rendering_info);

    // Transition to PRESENT_SRC_KHR for swapchain present.
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
