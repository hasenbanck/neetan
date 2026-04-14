use jay_ash::vk;

use crate::plumbing::RenderingEncoder;

mod blitter;
mod clear;
mod compose;
mod crt;
mod scale;

pub(crate) use blitter::*;
pub(crate) use clear::*;
pub(crate) use compose::{Compose, render_compose_pass};
pub(crate) use crt::{Crt, render_crt_pass};
pub(crate) use scale::{Scale, render_scale_pass};

pub(crate) struct UpscalePushConstants {
    content_height: f32,
    output_size: [f32; 2],
    source_size: [f32; 2],
}

impl UpscalePushConstants {
    pub(crate) fn new(
        output_extent: vk::Extent2D,
        source_width: u32,
        source_height: u32,
        content_height: u32,
    ) -> Self {
        Self {
            content_height: content_height as f32,
            output_size: [output_extent.width as f32, output_extent.height as f32],
            source_size: [source_width as f32, source_height as f32],
        }
    }

    pub(crate) fn to_le_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&self.content_height.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.output_size[0].to_le_bytes());
        bytes[12..16].copy_from_slice(&self.output_size[1].to_le_bytes());
        bytes[16..20].copy_from_slice(&self.source_size[0].to_le_bytes());
        bytes[20..24].copy_from_slice(&self.source_size[1].to_le_bytes());
        bytes
    }
}

pub(crate) trait Renderer {
    type DrawData;

    /// Records the render commands within a dynamic rendering pass.
    fn render(&self, encoder: &RenderingEncoder<'_>, draw_data: Self::DrawData);
}
