use crate::plumbing::RenderingEncoder;

mod blitter;
mod clear;
mod compose;
mod scale;

pub(crate) use blitter::*;
pub(crate) use clear::*;
pub(crate) use compose::{Compose, render_compose_pass};
pub(crate) use scale::{Scale, render_scale_pass};

pub(crate) trait Renderer {
    type DrawData;

    /// Records the render commands within a dynamic rendering pass.
    fn render(&self, encoder: &RenderingEncoder<'_>, draw_data: Self::DrawData);
}
