//! Graphics pipeline wrappers for traditional Vulkan pipelines with dynamic rendering.

use std::{ffi::CStr, rc::Rc};

use jay_ash::vk;

use super::{IntoCString, context::Context};

/// Blend state configuration for a pipeline.
#[derive(Clone)]
pub(crate) struct PipelineBlendState {
    /// Whether blending is enabled.
    pub enabled: bool,
    /// Source color blend factor.
    pub src_color_blend_factor: vk::BlendFactor,
    /// Destination color blend factor.
    pub dst_color_blend_factor: vk::BlendFactor,
    /// Color blend operation.
    pub color_blend_op: vk::BlendOp,
    /// Source alpha blend factor.
    pub src_alpha_blend_factor: vk::BlendFactor,
    /// Destination alpha blend factor.
    pub dst_alpha_blend_factor: vk::BlendFactor,
    /// Alpha blend operation.
    pub alpha_blend_op: vk::BlendOp,
    /// Color write mask.
    pub write_mask: vk::ColorComponentFlags,
}

impl Default for PipelineBlendState {
    fn default() -> Self {
        Self {
            enabled: false,
            src_color_blend_factor: vk::BlendFactor::ONE,
            dst_color_blend_factor: vk::BlendFactor::ZERO,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ONE,
            dst_alpha_blend_factor: vk::BlendFactor::ZERO,
            alpha_blend_op: vk::BlendOp::ADD,
            write_mask: vk::ColorComponentFlags::RGBA,
        }
    }
}

/// Multisample state configuration for a pipeline.
#[derive(Clone)]
pub(crate) struct PipelineMultisampleState {
    /// Sample count.
    pub sample_count: vk::SampleCountFlags,
    /// Sample mask.
    pub sample_mask: u32,
    /// Whether alpha to coverage is enabled.
    pub alpha_to_coverage: bool,
}

impl Default for PipelineMultisampleState {
    fn default() -> Self {
        Self {
            sample_count: vk::SampleCountFlags::TYPE_1,
            sample_mask: 0xFFFFFFFF,
            alpha_to_coverage: false,
        }
    }
}

/// Pipeline configuration for creation.
pub(crate) struct PipelineConfig<'a> {
    /// Color attachment formats for dynamic rendering.
    pub color_formats: Vec<vk::Format>,
    /// Depth attachment format for dynamic rendering.
    pub depth_format: Option<vk::Format>,
    /// Blend state.
    pub blend_state: PipelineBlendState,
    /// Multisample state.
    pub multisample_state: PipelineMultisampleState,
    /// Specialization constants.
    pub specialization_info: Option<&'a vk::SpecializationInfo<'a>>,
    /// Vertex input layout. When `None`, the pipeline uses no vertex buffers.
    pub vertex_input: Option<vk::PipelineVertexInputStateCreateInfo<'a>>,
    /// Pipeline layout for descriptor sets.
    pub pipeline_layout: vk::PipelineLayout,
}

/// Graphics pipeline wrapping vertex and fragment shaders.
///
/// Uses dynamic rendering with extensive dynamic state. Most state is set
/// via command buffer commands; only blend state and multisample state are
/// baked into the pipeline.
pub(crate) struct GraphicsPipeline {
    pipeline: vk::Pipeline,
    context: Rc<Context>,
}

impl GraphicsPipeline {
    /// Creates a graphics pipeline from SPIR-V with separate vertex and fragment entry points.
    ///
    /// # Dynamic State
    ///
    /// The following states are dynamic (set via command buffer):
    /// - Viewport with count
    /// - Scissor with count
    /// - Cull mode
    /// - Front face
    /// - Depth test enable/write enable/compare op
    /// - Stencil test enable
    /// - Depth bias enable
    /// - Primitive topology
    /// - Primitive restart enable
    /// - Rasterizer discard enable
    /// - Vertex input
    ///
    /// # Baked State
    ///
    /// The following states are baked into the pipeline:
    /// - Blend enable/equation/write mask
    /// - Polygon mode
    /// - Sample count/mask
    /// - Alpha to coverage enable
    #[allow(clippy::too_many_lines)]
    pub(crate) fn new(
        context: Rc<Context>,
        name: &CStr,
        spirv: &[u32],
        vertex_entry: &CStr,
        fragment_entry: &CStr,
        config: &PipelineConfig,
    ) -> Result<Self, vk::Result> {
        let device = context.device();

        let mut vertex_module_info = vk::ShaderModuleCreateInfo::default().code(spirv);
        let mut fragment_module_info = vk::ShaderModuleCreateInfo::default().code(spirv);

        let vertex_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .name(vertex_entry)
            .push_next(&mut vertex_module_info);

        let mut fragment_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .name(fragment_entry);

        if let Some(spec_info) = config.specialization_info {
            fragment_stage = fragment_stage.specialization_info(spec_info);
        }

        let fragment_stage = fragment_stage.push_next(&mut fragment_module_info);

        let stages = [vertex_stage, fragment_stage];

        let default_vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let vertex_input_state = config
            .vertex_input
            .as_ref()
            .unwrap_or(&default_vertex_input);

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default();

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false)
            .line_width(1.0);

        let sample_maks = &[config.multisample_state.sample_mask];
        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(config.multisample_state.sample_count)
            .sample_shading_enable(false)
            .sample_mask(sample_maks)
            .alpha_to_coverage_enable(config.multisample_state.alpha_to_coverage)
            .alpha_to_one_enable(false);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(false)
            .depth_write_enable(false)
            .depth_compare_op(vk::CompareOp::ALWAYS)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(config.blend_state.enabled)
            .src_color_blend_factor(config.blend_state.src_color_blend_factor)
            .dst_color_blend_factor(config.blend_state.dst_color_blend_factor)
            .color_blend_op(config.blend_state.color_blend_op)
            .src_alpha_blend_factor(config.blend_state.src_alpha_blend_factor)
            .dst_alpha_blend_factor(config.blend_state.dst_alpha_blend_factor)
            .alpha_blend_op(config.blend_state.alpha_blend_op)
            .color_write_mask(config.blend_state.write_mask);

        let color_blend_attachments = vec![color_blend_attachment; config.color_formats.len()];

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        // Activate all Vulkan 1.3 core dynamic states.
        let dynamic_states = [
            vk::DynamicState::LINE_WIDTH,
            vk::DynamicState::DEPTH_BIAS,
            vk::DynamicState::BLEND_CONSTANTS,
            vk::DynamicState::DEPTH_BOUNDS,
            vk::DynamicState::STENCIL_COMPARE_MASK,
            vk::DynamicState::STENCIL_WRITE_MASK,
            vk::DynamicState::STENCIL_REFERENCE,
            vk::DynamicState::CULL_MODE,
            vk::DynamicState::FRONT_FACE,
            vk::DynamicState::PRIMITIVE_TOPOLOGY,
            vk::DynamicState::VIEWPORT_WITH_COUNT,
            vk::DynamicState::SCISSOR_WITH_COUNT,
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_COMPARE_OP,
            vk::DynamicState::DEPTH_BOUNDS_TEST_ENABLE,
            vk::DynamicState::STENCIL_TEST_ENABLE,
            vk::DynamicState::STENCIL_OP,
            vk::DynamicState::RASTERIZER_DISCARD_ENABLE,
            vk::DynamicState::DEPTH_BIAS_ENABLE,
            vk::DynamicState::PRIMITIVE_RESTART_ENABLE,
        ];

        let dynamic_state_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let mut pipeline_rendering_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&config.color_formats);

        if let Some(depth_format) = config.depth_format {
            pipeline_rendering_info = pipeline_rendering_info.depth_attachment_format(depth_format);
        }

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .layout(config.pipeline_layout)
            .vertex_input_state(vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .depth_stencil_state(&depth_stencil_state)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state_info)
            .push_next(&mut pipeline_rendering_info);

        let pipeline = match unsafe {
            device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[pipeline_create_info],
                None,
            )
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, err)) => {
                return Err(err);
            }
        };

        let name_str = name.to_string_lossy();
        let pipeline_name = format!("{name_str}_pipeline").into_cstring();
        context.set_object_name(&pipeline_name, pipeline);

        Ok(Self { pipeline, context })
    }

    /// Returns the Vulkan pipeline handle.
    #[inline]
    pub(crate) fn pipeline(&self) -> vk::Pipeline {
        self.pipeline
    }
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.context.device().destroy_pipeline(self.pipeline, None);
        }
    }
}
