#[allow(clippy::module_inception)]
mod compose;

use jay_ash::vk;

pub(crate) use self::compose::Compose;
use crate::{
    passes::Renderer,
    plumbing::{ColorTargetImage, CommandEncoder},
};

/// Renders the compose pass (Stage 1): text VRAM to native-resolution target (640×480).
pub(crate) fn render_compose_pass(
    encoder: &mut CommandEncoder,
    native_target: &ColorTargetImage,
    compose: &Compose,
) {
    let extent = native_target.extent();

    encoder.image_barrier(
        native_target.handle(),
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    );

    let color_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(native_target.view())
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        });

    let rendering_info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
        })
        .layer_count(1)
        .color_attachments(std::slice::from_ref(&color_attachment));

    {
        let rendering_encoder = encoder.begin_rendering(&rendering_info);

        rendering_encoder.set_viewport(&[vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }]);
        rendering_encoder.set_scissor(&[vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
        }]);

        compose.render(&rendering_encoder, ());
    }

    encoder.image_barrier(
        native_target.handle(),
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    );
}
