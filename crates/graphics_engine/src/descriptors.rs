use std::{ffi::CStr, rc::Rc};

use common::Context as _;
use jay_ash::vk;

use crate::{
    Result,
    plumbing::{ColorTargetImage, CommandEncoder, Context, MappedBuffer},
};

pub(crate) struct DescriptorResources {
    nearest_sampler: vk::Sampler,
    linear_sampler: vk::Sampler,
    set_layout_0: vk::DescriptorSetLayout,
    set_layout_1: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    context: Rc<Context>,
}

impl DescriptorResources {
    pub(crate) fn new(context: &Rc<Context>) -> Result<Self> {
        let device = context.device();

        let nearest_sampler = unsafe {
            device
                .create_sampler(&nearest_sampler_create_info(), None)
                .context("Failed to create nearest sampler")?
        };

        let linear_sampler = unsafe {
            device
                .create_sampler(&linear_sampler_create_info(), None)
                .context("Failed to create linear sampler")?
        };

        // Set 0: Immutable samplers (binding 0 = nearest, binding 1 = linear).
        let nearest_samplers = [nearest_sampler];
        let linear_samplers = [linear_sampler];
        let sampler_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .immutable_samplers(&nearest_samplers),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .immutable_samplers(&linear_samplers),
        ];

        let set_layout_0_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&sampler_bindings);

        let set_layout_0 = unsafe {
            device
                .create_descriptor_set_layout(&set_layout_0_info, None)
                .context("Failed to create sampler set layout")?
        };

        // Set 1: Two SAMPLED_IMAGE bindings + two STORAGE_BUFFER bindings.
        let resource_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let set_layout_1_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&resource_bindings);

        let set_layout_1 = unsafe {
            device
                .create_descriptor_set_layout(&set_layout_1_info, None)
                .context("Failed to create resource set layout")?
        };

        let push_constant_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(8); // int2 = 2 × i32 = 8 bytes

        let set_layouts = [set_layout_0, set_layout_1];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
            .push_constant_ranges(std::slice::from_ref(&push_constant_range));

        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .context("Failed to create pipeline layout")?
        };

        Ok(Self {
            nearest_sampler,
            linear_sampler,
            set_layout_0,
            set_layout_1,
            pipeline_layout,
            context: Rc::clone(context),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_stale_descriptors(
        &self,
        frame_sets: &mut FrameDescriptorSets,
        descriptor_version: &mut u64,
        current_version: u64,
        color_target: &ColorTargetImage,
        native_target: &ColorTargetImage,
        upload_buffer: &MappedBuffer,
        font_rom_buffer: &MappedBuffer,
    ) {
        if *descriptor_version >= current_version {
            return;
        }

        let image_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

        let color_target_info = vk::DescriptorImageInfo::default()
            .image_view(color_target.view())
            .image_layout(image_layout);

        let native_target_info = vk::DescriptorImageInfo::default()
            .image_view(native_target.view())
            .image_layout(image_layout);

        let upload_buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(upload_buffer.raw())
            .offset(0)
            .range(upload_buffer.byte_size());

        let font_rom_buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(font_rom_buffer.raw())
            .offset(0)
            .range(font_rom_buffer.byte_size());

        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(frame_sets.resource_set())
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&color_target_info)),
            vk::WriteDescriptorSet::default()
                .dst_set(frame_sets.resource_set())
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&native_target_info)),
            vk::WriteDescriptorSet::default()
                .dst_set(frame_sets.resource_set())
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&upload_buffer_info)),
            vk::WriteDescriptorSet::default()
                .dst_set(frame_sets.resource_set())
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&font_rom_buffer_info)),
        ];

        unsafe {
            self.context().device().update_descriptor_sets(&writes, &[]);
        };

        *descriptor_version = current_version;
    }

    pub(crate) fn bind_descriptors(
        &self,
        encoder: &CommandEncoder,
        frame_sets: &FrameDescriptorSets,
    ) {
        encoder.bind_descriptor_sets(
            self.pipeline_layout,
            &[frame_sets.sampler_set(), frame_sets.resource_set()],
        );
    }

    pub(crate) fn pipeline_layout(&self) -> vk::PipelineLayout {
        self.pipeline_layout
    }

    pub(crate) fn set_layout_0(&self) -> vk::DescriptorSetLayout {
        self.set_layout_0
    }

    pub(crate) fn set_layout_1(&self) -> vk::DescriptorSetLayout {
        self.set_layout_1
    }

    pub(crate) fn context(&self) -> &Context {
        &self.context
    }
}

impl Drop for DescriptorResources {
    fn drop(&mut self) {
        unsafe {
            let device = self.context.device();
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_set_layout(self.set_layout_1, None);
            device.destroy_descriptor_set_layout(self.set_layout_0, None);
            device.destroy_sampler(self.linear_sampler, None);
            device.destroy_sampler(self.nearest_sampler, None);
        }
    }
}

pub(crate) struct FrameDescriptorSets {
    pool: vk::DescriptorPool,
    sampler_set: vk::DescriptorSet,
    resource_set: vk::DescriptorSet,
    context: Rc<Context>,
}

impl FrameDescriptorSets {
    pub(crate) fn new(
        context: Rc<Context>,
        name: &CStr,
        resources: &DescriptorResources,
    ) -> Result<Self> {
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(2),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(2),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(2)
            .pool_sizes(&pool_sizes);

        let pool = unsafe {
            context
                .device()
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create descriptor pool")?
        };

        context.set_object_name(name, pool);

        let set_layouts = [resources.set_layout_0(), resources.set_layout_1()];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&set_layouts);

        let sets = unsafe {
            context
                .device()
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate descriptor sets")?
        };

        Ok(Self {
            pool,
            sampler_set: sets[0],
            resource_set: sets[1],
            context,
        })
    }

    pub(crate) fn sampler_set(&self) -> vk::DescriptorSet {
        self.sampler_set
    }

    pub(crate) fn resource_set(&self) -> vk::DescriptorSet {
        self.resource_set
    }
}

impl Drop for FrameDescriptorSets {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device()
                .destroy_descriptor_pool(self.pool, None);
        }
    }
}

/// `vk::SamplerCreateInfo` for a nearest (point) sampler.
pub(crate) fn nearest_sampler_create_info() -> vk::SamplerCreateInfo<'static> {
    vk::SamplerCreateInfo::default()
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .min_lod(0.0)
        .max_lod(vk::LOD_CLAMP_NONE)
}

/// `vk::SamplerCreateInfo` for a linear sampler.
pub(crate) fn linear_sampler_create_info() -> vk::SamplerCreateInfo<'static> {
    vk::SamplerCreateInfo::default()
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .min_lod(0.0)
        .max_lod(vk::LOD_CLAMP_NONE)
}
