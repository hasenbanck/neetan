use common::{Context, OptionContext, info};
use sdl3::{
    render::{PixelFormat, Rect, Renderer, ScaleMode, Texture},
    video::Window,
};

use crate::{
    DisplayAspectMode, GraphicsEngine, NATIVE_HEIGHT, NATIVE_WIDTH, RenderInstructions, Result,
    compute_color_target_extent,
};

/// Renderer state that exists only between `on_resume` and `on_destroy_surface`.
struct SdlState {
    texture: Texture,
    renderer: Renderer,
}

/// SDL 2D rendering backend.
pub struct SdlGraphicsEngine {
    aspect_mode: DisplayAspectMode,
    state: Option<SdlState>,
}

impl SdlGraphicsEngine {
    /// Creates a new SDL graphics engine. The renderer itself is created in `on_resume`.
    pub fn new(aspect_mode: DisplayAspectMode) -> Self {
        Self {
            aspect_mode,
            state: None,
        }
    }
}

impl GraphicsEngine for SdlGraphicsEngine {
    fn on_resume(
        &mut self,
        window: &mut Window,
        vsync_enabled: bool,
        _width: u32,
        _height: u32,
    ) -> Result<()> {
        if self.state.is_some() {
            return Ok(());
        }

        let renderer = window
            .create_renderer()
            .context("SDL_CreateRenderer failed")?;
        renderer
            .set_vsync(vsync_enabled)
            .context("SDL_SetRenderVSync failed")?;

        let texture = renderer
            .create_streaming_texture(PixelFormat::Rgba32, NATIVE_WIDTH, NATIVE_HEIGHT)
            .context("SDL_CreateTexture failed")?;
        texture
            .set_scale_mode(ScaleMode::Linear)
            .context("SDL_SetTextureScaleMode failed")?;

        let backend_name = renderer.name().unwrap_or_else(|| "unknown".to_string());
        info!("SDL graphics engine initialized (backend: {backend_name})");

        self.state = Some(SdlState { texture, renderer });

        Ok(())
    }

    fn on_resize(&mut self, _width: u32, _height: u32) -> Result<()> {
        Ok(())
    }

    fn on_destroy_surface(&mut self) {
        self.state = None;
    }

    fn try_wait_for_previous_present(&self, _timeout_ms: u64) -> Result<bool> {
        Ok(true)
    }

    fn render_frame(&mut self, render_instructions: Option<&RenderInstructions>) -> Result<()> {
        let state = self
            .state
            .as_mut()
            .context("SDL renderer not initialized")?;

        let (output_width, output_height) = state
            .renderer
            .output_size()
            .context("SDL_GetRenderOutputSize failed")?;

        if output_width == 0 || output_height == 0 {
            return Ok(());
        }

        state
            .renderer
            .clear_with_color(0, 0, 0, 255)
            .context("SDL_RenderClear failed")?;

        if let Some(instructions) = render_instructions {
            let expected_bytes = (NATIVE_WIDTH * NATIVE_HEIGHT * 4) as usize;
            if instructions.framebuffer.len() != expected_bytes {
                return Ok(());
            }

            state
                .texture
                .update(None, instructions.framebuffer, NATIVE_WIDTH * 4)
                .context("SDL_UpdateTexture failed")?;

            let native_height = instructions.native_height.min(NATIVE_HEIGHT);
            let src = Rect {
                x: 0,
                y: 0,
                w: NATIVE_WIDTH as i32,
                h: native_height as i32,
            };
            let dst = fitted_destination_rect(
                output_width,
                output_height,
                self.aspect_mode.display_aspect_ratio(),
            );

            state
                .renderer
                .render_texture(&state.texture, Some(src), Some(dst))
                .context("SDL_RenderTexture failed")?;
        }

        state
            .renderer
            .present()
            .context("SDL_RenderPresent failed")?;

        Ok(())
    }
}

fn fitted_destination_rect(output_width: u32, output_height: u32, aspect_ratio: f64) -> Rect {
    let (fitted_width, fitted_height) =
        compute_color_target_extent(output_width, output_height, aspect_ratio);
    let offset_x = ((output_width - fitted_width) / 2) as i32;
    let offset_y = ((output_height - fitted_height) / 2) as i32;
    Rect {
        x: offset_x,
        y: offset_y,
        w: fitted_width as i32,
        h: fitted_height as i32,
    }
}
