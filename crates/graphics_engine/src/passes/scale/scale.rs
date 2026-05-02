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

static SCALE_SPV: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/shaders_compiled/passes/scale/scale.spv"
));

pub(crate) struct Scale {
    pipeline: GraphicsPipeline,
}

impl Scale {
    pub(crate) fn new(
        pipeline_loader: &PipelineLoader,
        render_pass: vk::RenderPass,
        pipeline_layout: vk::PipelineLayout,
    ) -> crate::Result<Self> {
        let pipeline = pipeline_loader
            .compile_graphics_pipeline(
                "scale",
                SCALE_SPV,
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
            .context("Can't load scale pipeline")?;

        Ok(Self { pipeline })
    }
}

impl Renderer for Scale {
    type DrawData = ();

    fn render(&self, encoder: &RenderPassEncoder<'_>, _draw_data: Self::DrawData) {
        encoder.bind_pipeline(&self.pipeline);
        encoder.draw(3, 1, 0, 0);
    }
}
