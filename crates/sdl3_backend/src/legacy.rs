//! Legacy SDL3 rendering backend built on the 2D `SDL_Renderer`.
//!
//! Kept as a fallback for hosts where the SDL3 GPU API is unavailable or
//! the [`SdlGpuBackend`](crate::ModernSdlGpuBackend) fails to initialize. Functionally
//! a simple streaming-texture blit with letterboxing.

use common::{Context, OptionContext, error, info, warn};
use sdl3::{
    info::version as sdl_version,
    render::{PixelFormat, Rect, Renderer, ScaleMode, Texture},
    video::Window,
};

use crate::{
    DisplayAspectMode, GraphicsEngine, PC98_NATIVE_HEIGHT, PC98_NATIVE_WIDTH, RenderInstructions,
    Result, Scaling, compute_color_target_extent,
};

/// Renderer state that exists only between `on_resume` and `on_destroy_surface`.
struct SdlState {
    texture: Texture,
    renderer: Renderer,
}

/// SDL 2D rendering backend.
pub struct LegacySdlBackend {
    aspect_mode: DisplayAspectMode,
    state: Option<SdlState>,
    scaling: Scaling,
    /// `SDL_SCALEMODE_PIXELART` was introduced in SDL 3.4.
    pixelart_supported: bool,
}

impl LegacySdlBackend {
    /// Creates a new SDL graphics engine. The renderer itself is created in `on_resume`.
    pub fn new(aspect_mode: DisplayAspectMode) -> Self {
        let (major, minor, patch) = sdl_version();
        let pixelart_supported = (major, minor) >= (3, 4);
        if !pixelart_supported {
            warn!(
                "SDL {major}.{minor}.{patch} detected (< 3.4); Scaling::Pixelart will use Nearest fallback"
            );
        }

        Self {
            aspect_mode,
            state: None,
            scaling: Scaling::Pixelart,
            pixelart_supported,
        }
    }

    fn sdl_scale_mode(&self, scaling: Scaling) -> ScaleMode {
        match scaling {
            Scaling::Nearest => ScaleMode::Nearest,
            Scaling::Bilinear => ScaleMode::Linear,
            Scaling::Pixelart => {
                if self.pixelart_supported {
                    ScaleMode::Pixelart
                } else {
                    ScaleMode::Nearest
                }
            }
        }
    }
}

impl GraphicsEngine for LegacySdlBackend {
    fn on_resume(&mut self, window: &mut Window, vsync_enabled: bool) -> Result<()> {
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
            .create_streaming_texture(PixelFormat::Rgba32, PC98_NATIVE_WIDTH, PC98_NATIVE_HEIGHT)
            .context("SDL_CreateTexture failed")?;
        texture
            .set_scale_mode(self.sdl_scale_mode(self.scaling))
            .context("SDL_SetTextureScaleMode failed")?;

        info!("SDL Renderer backend initialized");

        self.state = Some(SdlState { texture, renderer });

        Ok(())
    }

    fn on_destroy_surface(&mut self) {
        self.state = None;
    }

    fn render_frame(
        &mut self,
        _window: &Window,
        render_instructions: Option<&RenderInstructions>,
    ) -> Result<()> {
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
            let expected_bytes = (PC98_NATIVE_WIDTH * PC98_NATIVE_HEIGHT * 4) as usize;
            if instructions.framebuffer.len() != expected_bytes {
                return Ok(());
            }

            state
                .texture
                .update(None, instructions.framebuffer, PC98_NATIVE_WIDTH * 4)
                .context("SDL_UpdateTexture failed")?;

            let native_height = instructions.native_height.min(PC98_NATIVE_HEIGHT);
            let src = Rect {
                x: 0,
                y: 0,
                w: PC98_NATIVE_WIDTH as i32,
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

    fn set_scaling(&mut self, scaling: Scaling) {
        self.scaling = scaling;
        let mode = self.sdl_scale_mode(scaling);
        if let Some(state) = self.state.as_ref()
            && let Err(error) = state.texture.set_scale_mode(mode)
        {
            error!("Failed to set SDL texture scale mode: {error}");
        }
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
