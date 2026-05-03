//! Render pass and framebuffer helpers.

use std::{ffi::CStr, rc::Rc};

use common::Context as _;
use jay_ash::vk;

use super::{Context, IntoCString};

/// A single-color-attachment render pass.
pub(crate) struct RenderPass {
    handle: vk::RenderPass,
    format: vk::Format,
    context: Rc<Context>,
}

impl RenderPass {
    /// Creates a render pass for the offscreen color target.
    pub(crate) fn new_color_target(
        context: Rc<Context>,
        name: &CStr,
        format: vk::Format,
    ) -> crate::Result<Self> {
        Self::new(
            context,
            name,
            format,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            &[],
        )
    }

    /// Creates a render pass for swapchain images.
    pub(crate) fn new_swapchain(
        context: Rc<Context>,
        name: &CStr,
        format: vk::Format,
    ) -> crate::Result<Self> {
        let dependencies = [
            vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE),
            vk::SubpassDependency::default()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::empty()),
        ];

        Self::new(
            context,
            name,
            format,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::PRESENT_SRC_KHR,
            &dependencies,
        )
    }

    fn new(
        context: Rc<Context>,
        name: &CStr,
        format: vk::Format,
        initial_layout: vk::ImageLayout,
        final_layout: vk::ImageLayout,
        dependencies: &[vk::SubpassDependency],
    ) -> crate::Result<Self> {
        let attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(initial_layout)
            .final_layout(final_layout);
        let attachments = [attachment];

        let color_attachment = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let color_attachments = [color_attachment];

        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachments);
        let subpasses = [subpass];

        let create_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(dependencies);

        let handle = unsafe {
            context
                .device()
                .create_render_pass(&create_info, None)
                .context("Failed to create render pass")?
        };

        context.set_object_name(name, handle);

        Ok(Self {
            handle,
            format,
            context,
        })
    }

    /// Returns the raw render pass handle.
    pub(crate) fn handle(&self) -> vk::RenderPass {
        self.handle
    }

    /// Returns the color attachment format.
    pub(crate) fn format(&self) -> vk::Format {
        self.format
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.context.device().destroy_render_pass(self.handle, None);
        }
    }
}

/// Creates a single-attachment framebuffer.
pub(crate) fn create_framebuffer(
    context: &Rc<Context>,
    name: &CStr,
    render_pass: vk::RenderPass,
    view: vk::ImageView,
    extent: vk::Extent2D,
) -> crate::Result<vk::Framebuffer> {
    let attachments = [view];
    let create_info = vk::FramebufferCreateInfo::default()
        .render_pass(render_pass)
        .attachments(&attachments)
        .width(extent.width)
        .height(extent.height)
        .layers(1);

    let framebuffer = unsafe {
        context
            .device()
            .create_framebuffer(&create_info, None)
            .context("Failed to create framebuffer")?
    };

    context.set_object_name(name, framebuffer);

    Ok(framebuffer)
}

/// Creates a framebuffer name derived from an image name.
pub(crate) fn framebuffer_name(name: &CStr) -> std::ffi::CString {
    format!("{}_framebuffer", name.to_string_lossy()).into_cstring()
}
