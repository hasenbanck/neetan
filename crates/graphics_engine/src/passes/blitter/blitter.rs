use common::Context as _;
use jay_ash::vk;

use crate::{
    passes::Renderer,
    pipeline_loader::PipelineLoader,
    plumbing::{
        GraphicsPipeline, PipelineBlendState, PipelineConfig, PipelineMultisampleState,
        RenderingEncoder,
    },
};

static BLITTER_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/shaders_compiled/passes/blitter/blitter.spv"
));
static BLITTER_SRGB_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/shaders_compiled/passes/blitter/blitter_srgb.spv"
));

pub(crate) struct Blitter {
    pipeline: GraphicsPipeline,
    color_target_image_format: vk::Format,
}

impl Blitter {
    pub(crate) fn new(
        pipeline_loader: &PipelineLoader,
        color_target_image_format: vk::Format,
        pipeline_layout: vk::PipelineLayout,
    ) -> crate::Result<Self> {
        let (shader_name, spv_data) = match color_target_image_format {
            vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB => {
                ("blitter_srgb", BLITTER_SRGB_SPV)
            }
            _ => ("blitter", BLITTER_SPV),
        };

        let pipeline = pipeline_loader
            .compile_graphics_pipeline(
                shader_name,
                spv_data,
                c"vs_main",
                c"fs_main",
                &PipelineConfig {
                    color_formats: vec![color_target_image_format],
                    depth_format: None,
                    blend_state: PipelineBlendState::default(),
                    multisample_state: PipelineMultisampleState::default(),
                    specialization_info: None,
                    vertex_input: None,
                    pipeline_layout,
                },
            )
            .context("Can't load blitter pipeline")?;

        Ok(Self {
            pipeline,
            color_target_image_format,
        })
    }

    pub(crate) fn color_target_image_format(&self) -> vk::Format {
        self.color_target_image_format
    }
}

impl Renderer for Blitter {
    type DrawData = ();

    fn render(&self, encoder: &RenderingEncoder<'_>, _draw_data: Self::DrawData) {
        encoder.bind_pipeline(&self.pipeline);
        encoder.draw(3, 1, 0, 0);
    }
}
