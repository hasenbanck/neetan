use common::Context as _;
use jay_ash::vk;

use crate::{
    passes::Renderer,
    pipeline_loader::PipelineLoader,
    plumbing::{
        GraphicsPipeline, PipelineBlendState, PipelineConfig, PipelineMultisampleState,
        RenderPassEncoder,
    },
};

static CRT_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/shaders_compiled/passes/crt/crt.spv"
));

pub(crate) struct Crt {
    pipeline: GraphicsPipeline,
}

impl Crt {
    pub(crate) fn new(
        pipeline_loader: &PipelineLoader,
        render_pass: vk::RenderPass,
        pipeline_layout: vk::PipelineLayout,
    ) -> crate::Result<Self> {
        let pipeline = pipeline_loader
            .compile_graphics_pipeline(
                "crt",
                CRT_SPV,
                c"vs_main",
                c"fs_main",
                &PipelineConfig {
                    render_pass,
                    subpass: 0,
                    blend_state: PipelineBlendState::default(),
                    multisample_state: PipelineMultisampleState::default(),
                    specialization_info: None,
                    vertex_input: None,
                    pipeline_layout,
                },
            )
            .context("Can't load crt pipeline")?;

        Ok(Self { pipeline })
    }
}

impl Renderer for Crt {
    type DrawData = ();

    fn render(&self, encoder: &RenderPassEncoder<'_>, _draw_data: Self::DrawData) {
        encoder.bind_pipeline(&self.pipeline);
        encoder.draw(3, 1, 0, 0);
    }
}
