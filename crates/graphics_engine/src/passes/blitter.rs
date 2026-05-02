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

    let clear_values = [vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        },
    }];

    let render_area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent: surface_extent,
    };

    {
        let render_pass_encoder = encoder.begin_render_pass(
            frame.render_pass(),
            frame.framebuffer(),
            render_area,
            &clear_values,
        );

        render_pass_encoder.set_viewport(&[vk::Viewport {
            x: offset_x as f32,
            y: offset_y as f32,
            width: color_target_extent.width as f32,
            height: color_target_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }]);
        render_pass_encoder.set_scissor(&[vk::Rect2D {
            offset: vk::Offset2D {
                x: offset_x as i32,
                y: offset_y as i32,
            },
            extent: color_target_extent,
        }]);

        let mut push_bytes = [0u8; 8];
        push_bytes[0..4].copy_from_slice(&(offset_x as i32).to_le_bytes());
        push_bytes[4..8].copy_from_slice(&(offset_y as i32).to_le_bytes());
        render_pass_encoder.push_constants(
            pipeline_layout,
            vk::ShaderStageFlags::FRAGMENT,
            0,
            &push_bytes,
        );

        blitter.render(&render_pass_encoder, ());
    }
}
