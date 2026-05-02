#[allow(clippy::module_inception)]
mod crt;

use jay_ash::vk;

pub(crate) use self::crt::Crt;
use crate::{
    passes::{Renderer, UpscalePushConstants},
    plumbing::{ColorTargetImage, CommandEncoder},
};

/// Renders the CRT pass (Stage 2): native-resolution to window-resolution color target.
pub(crate) fn render_crt_pass(
    encoder: &mut CommandEncoder,
    color_target: &ColorTargetImage,
    render_pass: vk::RenderPass,
    crt: &Crt,
    native_height: u32,
    pipeline_layout: vk::PipelineLayout,
) {
    let extent = color_target.extent();

    encoder.image_barrier(
        color_target.handle(),
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::SHADER_READ,
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    );

    let clear_values = [vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 1.0],
        },
    }];

    let render_area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent: vk::Extent2D {
            width: extent.width,
            height: extent.height,
        },
    };

    {
        let render_pass_encoder = encoder.begin_render_pass(
            render_pass,
            color_target.framebuffer(),
            render_area,
            &clear_values,
        );

        render_pass_encoder.set_viewport(&[vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }]);
        render_pass_encoder.set_scissor(&[vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
        }]);

        let push_constants = UpscalePushConstants::new(
            vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
            640,
            native_height,
            native_height,
        );
        render_pass_encoder.push_constants(
            pipeline_layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            &push_constants.to_le_bytes(),
        );

        crt.render(&render_pass_encoder, ());
    }

    encoder.image_barrier(
        color_target.handle(),
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::SHADER_READ,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    );
}
