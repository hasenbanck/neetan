//! Shader loading from embedded SPIR-V data.

use std::{ffi::CStr, rc::Rc, time::Instant};

use common::{Context as _, debug};

use crate::{
    Result,
    plumbing::{Context, GraphicsPipeline, IntoCString, PipelineConfig},
};

/// Loads pipelines from embedded SPIR-V shader data.
pub(crate) struct PipelineLoader {
    context: Rc<Context>,
}

impl PipelineLoader {
    /// Creates a new pipeline loader.
    pub(crate) fn new(context: Rc<Context>) -> Self {
        Self { context }
    }

    /// Converts raw SPIR-V bytes into a `Vec<u32>`.
    fn load_spirv(spv_data: &[u8]) -> Vec<u32> {
        assert_eq!(spv_data.len() % 4, 0);
        let u32_length = spv_data.len() / 4;

        let mut aligned_data = vec![0u32; u32_length];
        common::cast_u32_slice_as_bytes_mut(&mut aligned_data).copy_from_slice(spv_data);

        aligned_data
    }

    /// Compiles a graphics pipeline from embedded SPIR-V data.
    pub(crate) fn compile_graphics_pipeline(
        &self,
        debug_name: &str,
        spv_data: &[u8],
        vertex_entry: &CStr,
        fragment_entry: &CStr,
        config: &PipelineConfig,
    ) -> Result<GraphicsPipeline> {
        let start = Instant::now();
        let spirv = Self::load_spirv(spv_data);
        let debug_name_cstr = debug_name.to_owned().into_cstring();
        let result = GraphicsPipeline::new(
            Rc::clone(&self.context),
            &debug_name_cstr,
            &spirv,
            vertex_entry,
            fragment_entry,
            config,
        )
        .context("Failed to create graphics pipeline")
        .map_err(crate::Error::from);

        debug!(
            "Pipeline for `{debug_name}` compiled in {elapsed_μs} μs",
            elapsed_μs = start.elapsed().as_micros()
        );

        result
    }
}
