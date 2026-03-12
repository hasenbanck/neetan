//! Render frame structure for presenting to the swapchain.

use jay_ash::vk;

/// A frame acquired from the swapchain, ready to be used as a rendering target.
///
/// Contains only swapchain-specific data. Synchronization primitives are
/// owned by `FrameResources` and accessed via `current_frame_index`.
#[derive(Copy, Clone)]
pub(crate) struct FrameTarget {
    /// The swapchain image index for this frame.
    image_index: u32,
    /// The image view for rendering to this frame.
    image_view: vk::ImageView,
    /// The raw swapchain image handle.
    image: vk::Image,
}

impl FrameTarget {
    /// Creates a new render frame with swapchain handles.
    pub(crate) fn new(image_index: u32, image_view: vk::ImageView, image: vk::Image) -> Self {
        Self {
            image_index,
            image_view,
            image,
        }
    }

    /// Returns the swapchain image index for this frame.
    pub(crate) fn image_index(&self) -> u32 {
        self.image_index
    }

    /// Returns the image view for rendering to this frame.
    pub(crate) fn view(&self) -> vk::ImageView {
        self.image_view
    }

    /// Returns the raw swapchain image handle.
    pub(crate) fn image(&self) -> vk::Image {
        self.image
    }
}
