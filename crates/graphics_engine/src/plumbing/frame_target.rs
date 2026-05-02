//! Render frame structure for presenting to the swapchain.

use jay_ash::vk;

/// A frame acquired from the swapchain, ready to be used as a rendering target.
///
/// Contains only swapchain-specific data. Synchronization primitives are owned
/// by `FrameResources` and accessed via `current_frame_index`.
#[derive(Copy, Clone)]
pub(crate) struct FrameTarget {
    /// The swapchain image index for this frame.
    image_index: u32,
    /// The framebuffer for rendering to this frame.
    framebuffer: vk::Framebuffer,
    /// The render pass compatible with this frame.
    render_pass: vk::RenderPass,
}

impl FrameTarget {
    /// Creates a new render frame with swapchain handles.
    pub(crate) fn new(
        image_index: u32,
        framebuffer: vk::Framebuffer,
        render_pass: vk::RenderPass,
    ) -> Self {
        Self {
            image_index,
            framebuffer,
            render_pass,
        }
    }

    /// Returns the swapchain image index for this frame.
    pub(crate) fn image_index(&self) -> u32 {
        self.image_index
    }

    /// Returns the framebuffer for rendering to this frame.
    pub(crate) fn framebuffer(&self) -> vk::Framebuffer {
        self.framebuffer
    }

    /// Returns the render pass compatible with this frame.
    pub(crate) fn render_pass(&self) -> vk::RenderPass {
        self.render_pass
    }
}
