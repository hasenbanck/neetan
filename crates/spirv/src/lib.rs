//! TODO Add crate documentation and rename crate to shader-interpreter, which clearly shows what the
//!      creates does.

#![forbid(unsafe_code)]

mod error;
mod interpreter;
mod module;
mod parser;
mod value;

use common::{DISPLAY_FLAG_PEGC_256_COLOR, DisplaySnapshotUpload, PegcSnapshotUpload};
pub use error::Error;
use interpreter::Interpreter;
use module::ExecutionModel;
use value::Value;

const WIDTH: u32 = 640;

/// Size of the font ROM GPU buffer in bytes (matches `FONT_ROM_BUFFER_SIZE` in graphics_engine
/// and the `FontRomData.font_rom_words` array length in the compose shader).
const FONT_ROM_BUFFER_SIZE: usize = 0x83000;

// TODO: This will break each compilation. Expose the SPV properly by exposing it with a #[test] guarded function in the graphics crate.
//       This dependency to the graphics crate also makes sure this SPV is properly creted.
static COMPOSE_SPV: &[u8] = include_bytes!(
    "../../../target/release/build/graphics_engine-c867b2053ff8a920/out/shaders_compiled/passes/compose/compose.spv"
);

pub struct ComposeOutput {
    pub framebuffer: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Parsed compose shader module, reusable across multiple executions.
pub struct ComposeShader {
    module: module::Module,
    function_index: usize,
    interface_ids: Vec<u32>,
}

impl ComposeShader {
    /// Parse a compose.spv SPIR-V module for reuse.
    pub fn new(spirv_bytes: &[u8]) -> Result<Self, Error> {
        let module = parser::parse(spirv_bytes)?;

        let fs_main_index = module
            .entry_points
            .iter()
            .position(|ep| ep.execution_model == ExecutionModel::Fragment && ep.name == "fs_main")
            .ok_or_else(|| Error::new("fs_main entry point not found"))?;

        let fs_function_id = module.entry_points[fs_main_index].function_id;
        let function_index = module
            .functions
            .iter()
            .position(|f| f.result_id == fs_function_id)
            .ok_or_else(|| Error::new("fs_main function not found"))?;

        let interface_ids = module.entry_points[fs_main_index].interface_ids.clone();

        Ok(Self {
            module,
            function_index,
            interface_ids,
        })
    }

    /// Create from the embedded compose.spv.
    pub fn from_embedded() -> Result<Self, Error> {
        Self::new(COMPOSE_SPV)
    }

    /// Execute the fragment shader, producing a 640xN RGBA8 sRGB framebuffer.
    ///
    /// The native height is 400 lines normally, or up to 480 in PEGC 256-color mode
    /// when `gdc_graphics_al > 400`.
    pub fn execute(
        &self,
        display_snapshot: &DisplaySnapshotUpload,
        font_rom_data: &[u8],
        pegc_snapshot: &PegcSnapshotUpload,
    ) -> Result<ComposeOutput, Error> {
        let is_pegc = (display_snapshot.display_flags & DISPLAY_FLAG_PEGC_256_COLOR) != 0;
        let gdc_al = display_snapshot.gdc_graphics_al;
        let native_height = if is_pegc && gdc_al > 400 {
            gdc_al.min(480)
        } else {
            400
        };

        let display_buffer = bytes_to_u32_vec(display_snapshot.as_bytes());
        let font_rom_buffer = bytes_to_u32_vec_padded(font_rom_data, FONT_ROM_BUFFER_SIZE / 4);
        let pegc_buffer = bytes_to_u32_vec(pegc_snapshot.as_bytes());

        let mut framebuffer = vec![0u8; (WIDTH * native_height * 4) as usize];
        let mut interpreter = Interpreter::new(
            &self.module,
            &self.interface_ids,
            &display_buffer,
            &font_rom_buffer,
            &pegc_buffer,
        );

        for y in 0..native_height {
            for x in 0..WIDTH {
                let frag_coord = Value::Vector(vec![
                    Value::F32(x as f32 + 0.5),
                    Value::F32(y as f32 + 0.5),
                    Value::F32(0.0),
                    Value::F32(1.0),
                ]);

                let result = interpreter.execute_fragment(self.function_index, frag_coord)?;
                let rgba8 = linear_float4_to_srgb_rgba8(&result);
                let offset = (y * WIDTH + x) as usize * 4;
                framebuffer[offset..offset + 4].copy_from_slice(&rgba8);
            }
        }

        Ok(ComposeOutput {
            framebuffer,
            width: WIDTH,
            height: native_height,
        })
    }
}

/// Convenience wrapper that parses and executes in one step.
pub fn execute_compose_shader(
    spirv_bytes: &[u8],
    display_snapshot: &DisplaySnapshotUpload,
    font_rom_data: &[u8],
    pegc_snapshot: &PegcSnapshotUpload,
) -> Result<ComposeOutput, Error> {
    ComposeShader::new(spirv_bytes)?.execute(display_snapshot, font_rom_data, pegc_snapshot)
}

fn bytes_to_u32_vec(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn bytes_to_u32_vec_padded(bytes: &[u8], min_words: usize) -> Vec<u32> {
    let mut words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    words.resize(words.len().max(min_words), 0);
    words
}

fn linear_to_srgb_component(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn linear_float4_to_srgb_rgba8(color: &Value) -> [u8; 4] {
    let components = color.as_vector();
    let r = linear_to_srgb_component(components[0].as_f32()).clamp(0.0, 1.0);
    let g = linear_to_srgb_component(components[1].as_f32()).clamp(0.0, 1.0);
    let b = linear_to_srgb_component(components[2].as_f32()).clamp(0.0, 1.0);
    let a = components[3].as_f32().clamp(0.0, 1.0);
    [
        (r * 255.0 + 0.5) as u8,
        (g * 255.0 + 0.5) as u8,
        (b * 255.0 + 0.5) as u8,
        (a * 255.0 + 0.5) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compose_spv() {
        let shader = ComposeShader::from_embedded().expect("failed to parse compose.spv");
        assert_eq!(shader.module.entry_points.len(), 2);
        assert_eq!(shader.module.entry_points[0].name, "vs_main");
        assert_eq!(shader.module.entry_points[1].name, "fs_main");
        assert_eq!(shader.module.functions.len(), 2);

        let fs_main = &shader.module.functions[shader.function_index];
        let total_phis: usize = fs_main
            .blocks
            .iter()
            .map(|b| b.phi_instructions.len())
            .sum();
        assert_eq!(
            total_phis, 129,
            "expected 129 OpPhi instructions in fs_main"
        );
    }

    #[test]
    fn execute_display_disabled() {
        let shader = ComposeShader::from_embedded().expect("failed to parse");
        let display = Box::new(DisplaySnapshotUpload::default());
        let pegc = Box::new(PegcSnapshotUpload::default());
        let font_rom = vec![0u8; FONT_ROM_BUFFER_SIZE];

        let output = shader
            .execute(&display, &font_rom, &pegc)
            .expect("shader execution failed");

        assert_eq!(output.width, 640);
        assert_eq!(output.height, 400);
        assert_eq!(output.framebuffer.len(), 640 * 400 * 4);

        // With display disabled (display_flags = 0), all pixels should be black (0,0,0,255).
        for (x, y) in [(0, 0), (320, 200), (639, 399)] {
            let offset = (y * 640 + x) as usize * 4;
            let pixel = &output.framebuffer[offset..offset + 4];
            assert_eq!(
                pixel,
                [0, 0, 0, 255],
                "pixel ({x}, {y}): expected black, got {pixel:?}"
            );
        }
    }
}
