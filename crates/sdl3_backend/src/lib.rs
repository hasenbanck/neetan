//! SDL3 rendering backends: GPU API primary, 2D renderer fallback.
#![deny(missing_docs)]

mod device;
mod error;
mod gpu;
mod legacy;
mod pipeline;

pub use error::Error;
pub use gpu::ModernSdlGpuBackend;
pub use legacy::LegacySdlBackend;
use sdl3::video::Window;

/// Native rendering target width in pixels.
pub const PC98_NATIVE_WIDTH: u32 = 640;
/// Native rendering target height in pixels.
pub const PC98_NATIVE_HEIGHT: u32 = 480;
/// Size in bytes of the native framebuffer.
pub const PC98_FRAMEBUFFER_BYTES: u64 = (PC98_NATIVE_WIDTH * PC98_NATIVE_HEIGHT * 4) as u64;

/// Backend result type.
pub type Result<T> = std::result::Result<T, Error>;

/// The instructions to render a frame.
pub struct RenderInstructions<'a> {
    /// 640*480*4 bytes of packed `R, G, B, A` sRGB pixels (little-endian per pixel)
    /// uploaded to the native-resolution sampled image.
    pub framebuffer: &'a [u8],
    /// Active vertical display height (400, or up to 480 in PEGC 480-line mode).
    pub native_height: u32,
    /// Whether the CRT upscale effect is enabled.
    pub crt: bool,
}

/// A backend-neutral interface for the graphics engine.
pub trait GraphicsEngine {
    /// Called when the window is resuming.
    fn on_resume(&mut self, window: &mut Window, vsync_enabled: bool) -> Result<()>;

    /// Called when the rendering surface should be torn down (e.g., Android suspend).
    fn on_destroy_surface(&mut self);

    /// Renders the next frame.
    fn render_frame(
        &mut self,
        window: &Window,
        render_instructions: Option<&RenderInstructions>,
    ) -> Result<()>;

    /// Selects the scaling method applied.
    fn set_scaling(&mut self, scaling: Scaling);
}

/// Scaling method used to scale the native image.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Scaling {
    /// Nearest-neighbour sampling: blocky pixels, no blending.
    Nearest,
    /// Hardware bilinear sampling: smooth but blurry.
    Bilinear,
    /// Pixel-art filter: single hardware bilinear sample
    /// produces crisp pixel art at arbitrary (non-integer) scale.
    Pixelart,
}

/// Display aspect mode for scaling and startup dimensions.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DisplayAspectMode {
    /// Pixel aspect correction: 640x400 is presented as 4:3.
    Aspect4By3,
    /// Square pixels: native 640x400 maps to 1:1 pixel aspect.
    Aspect1By1,
}

impl DisplayAspectMode {
    /// Returns the display aspect ratio (width / height) for this mode.
    pub fn display_aspect_ratio(self) -> f64 {
        match self {
            Self::Aspect4By3 => 4.0 / 3.0,
            Self::Aspect1By1 => 640.0 / 400.0,
        }
    }

    /// Source-size vector passed to the present shader for aspect-ratio fitting.
    ///
    /// For 4:3 mode the display aspect is fixed regardless of content_height
    /// (any line count is stretched to 4:3). For 1:1 (square-pixel) mode the
    /// display aspect tracks the active line count, so the source vector uses
    /// the live content_height.
    pub fn source_size(self, content_height: u32) -> [f32; 2] {
        let content_height = content_height.max(1) as f32;
        match self {
            Self::Aspect4By3 => [PC98_NATIVE_WIDTH as f32, PC98_NATIVE_HEIGHT as f32],
            Self::Aspect1By1 => [PC98_NATIVE_WIDTH as f32, content_height],
        }
    }
}

/// Computes the fitted color-target extent for a given surface size and aspect ratio.
///
/// Picks the largest (width, height) within (surface_width, surface_height) that
/// matches `aspect_ratio` exactly. Used to letterbox/pillarbox the rendered image.
pub fn compute_color_target_extent(
    surface_width: u32,
    surface_height: u32,
    aspect_ratio: f64,
) -> (u32, u32) {
    let surface_aspect = surface_width as f64 / surface_height as f64;
    if surface_aspect > aspect_ratio {
        let height = surface_height;
        let width = (surface_height as f64 * aspect_ratio).round() as u32;
        (width, height)
    } else {
        let width = surface_width;
        let height = (surface_width as f64 / aspect_ratio).round() as u32;
        (width, height)
    }
}
