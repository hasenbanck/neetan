use jay_ash::vk;

use crate::plumbing::{CommandEncoder, FrameTarget};

/// Clears the frame image.
pub(crate) fn clear_frame_pass(
    encoder: &mut CommandEncoder,
    extent: vk::Extent2D,
    frame: &FrameTarget,
) {
    let clear_values = [vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        },
    }];

    let render_area = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent,
    };

    let _render_pass_encoder = encoder.begin_render_pass(
        frame.render_pass(),
        frame.framebuffer(),
        render_area,
        &clear_values,
    );
}
