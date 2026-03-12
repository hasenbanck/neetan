use std::rc::Rc;

use common::Context as _;
use jay_ash::vk;

use crate::{
    layout_transitioner::LayoutTransitioner,
    plumbing::{ColorTargetImage, DeferredResource, Device},
};

pub(crate) struct Resources {
    color_target: ColorTargetImage,
    native_target: ColorTargetImage,
    descriptor_version: u64,
}

impl Resources {
    pub(crate) fn new(
        device: &Device,
        layout_transitioner: &mut LayoutTransitioner,
        width: u32,
        height: u32,
    ) -> crate::Result<Self> {
        let context = Rc::clone(device.context());

        let color_target = ColorTargetImage::new(
            Rc::clone(&context),
            c"color_target",
            layout_transitioner,
            vk::Format::R8G8B8A8_SRGB,
            width,
            height,
        )
        .context("Can't create color target")?;

        let native_target = ColorTargetImage::new(
            Rc::clone(&context),
            c"native_target",
            layout_transitioner,
            vk::Format::R8G8B8A8_SRGB,
            640,
            400,
        )
        .context("Can't create native-resolution target")?;

        Ok(Resources {
            color_target,
            native_target,
            descriptor_version: 1,
        })
    }

    /// Must be called when the window is resized to re-create resolution dependent resources like render targets.
    pub(crate) fn on_resize(
        &mut self,
        device: &Device,
        layout_transitioner: &mut LayoutTransitioner,
        width: u32,
        height: u32,
    ) {
        let context = Rc::clone(device.context());

        // Defer old color_target for cleanup.
        let (old_handle, old_view, old_memory) = std::mem::replace(
            &mut self.color_target,
            ColorTargetImage::new(
                Rc::clone(&context),
                c"color_target",
                layout_transitioner,
                vk::Format::R8G8B8A8_SRGB,
                width,
                height,
            )
            .expect("failed to create color target"),
        )
        .into_raw_parts();

        device.defer_resource(DeferredResource::Image {
            handle: old_handle,
            view: Some(old_view),
            memory: old_memory,
        });

        self.descriptor_version += 1;
    }

    /// Returns a reference to the color target image.
    pub(crate) fn color_target(&self) -> &ColorTargetImage {
        &self.color_target
    }

    /// Returns a reference to the native-resolution target image (640x400).
    pub(crate) fn native_target(&self) -> &ColorTargetImage {
        &self.native_target
    }

    /// Returns the current descriptor version for staleness tracking.
    pub(crate) fn descriptor_version(&self) -> u64 {
        self.descriptor_version
    }
}
