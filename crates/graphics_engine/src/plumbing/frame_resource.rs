use crate::{
    descriptors::FrameDescriptorSets,
    plumbing::{Binary, CommandBuffer, Fence, MappedBuffer, Semaphore},
};

/// Resources for a single in-flight frame.
///
/// Groups synchronization primitives, command buffers, and per-frame GPU resources
/// that share the same lifecycle: all resources are consumed before the present
/// operation completes, and the present fence signals when all resources are safe to reuse.
pub(crate) struct FrameResources {
    /// Semaphore signaled when swapchain image is available.
    pub(crate) image_available_semaphore: Semaphore<Binary>,
    /// Fence signaled when presentation completes.
    pub(crate) present_fence: Fence,
    /// Command buffer for this frame's graphics work.
    pub(crate) graphics_command_buffer: CommandBuffer,
    /// Per-frame descriptor resources.
    pub(crate) descriptors: FrameDescriptorSets,
    /// Per-frame upload buffer for CPU->GPU VRAM data transfer.
    pub(crate) upload_buffer: MappedBuffer,
    /// Tracks which descriptor version this frame's descriptors were last written at.
    pub(crate) last_descriptor_version: u64,
    /// Present ID for VK_KHR_present_id2 if supported.
    pub(crate) present_wait_id: u64,
}
