//! CPU-side software renderer for the PC-98 display.
//!
//! Composes a 640x480 sRGB framebuffer that the graphics engine uploads to
//! a sampled image. Owns its own font ROM and framebuffer; `render` does not
//! allocate.

use std::{
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
};

mod compose;
mod text_normalizer;

/// Total byte size of the PC-98 text VRAM image (16 KiB).
///
/// Bytes 0x0000-0x1FFF hold character codes (low and high byte interleaved
/// per cell); bytes 0x2000-0x3FFF hold attribute bytes with the same cell
/// layout.
pub const TEXT_VRAM_BYTES: usize = 0x4000;

/// Display flag bit 7: PEGC 256-color mode active.
pub const DISPLAY_FLAG_PEGC_256_COLOR: u32 = 0x80;

/// Number of normalized text cells (covers a maximum 80x52 logical text plane).
const TEXT_CELL_COUNT: usize = 4096;

const FONT_ROM_BUFFER_SIZE: usize = 0x83000;

/// CPU-side renderer for the PC-98 display compose pass.
pub struct SoftwareRenderer {
    /// Embedded state for save/restore.
    pub state: SoftwareRendererState,
    /// Per-row scratch reused every frame; not part of save/restore.
    scratch: Box<compose::ComposeScratch>,
    /// Cached AVX2 detection result; queried once at construction time.
    has_avx2: bool,
}

/// Persistent buffers owned by the renderer.
pub struct SoftwareRendererState {
    /// Internal copy of the CGROM/font ROM used for text rasterization.
    pub font_rom: Box<[u8]>,
    /// 640x480 framebuffer, 4 bytes per pixel as `R, G, B, A` little-endian.
    pub framebuffer: Box<[u8]>,
    /// Per-cell normalized text descriptors (scratch reused across frames).
    pub text_cells: Box<[u32; TEXT_CELL_COUNT]>,
}

/// Per-frame inputs to the software renderer.
pub struct RenderInputs<'a> {
    /// Full TVRAM image: 0x0000-0x1FFF character bytes, 0x2000-0x3FFF attribute bytes.
    pub text_vram: &'a [u8; TEXT_VRAM_BYTES],
    /// GDC text pitch in cells per row (typically 80).
    pub gdc_text_pitch: u32,
    /// Four packed text scroll descriptors: low 16 bits = start address, high 16 bits = line count.
    pub gdc_scroll_start_line: [u32; 4],
    /// Video mode register (port 0x68 value).
    pub video_mode: u32,
    /// CRTC PL (low 16) and BL (high 16).
    pub crtc_pl_bl: u32,
    /// CRTC CL (low 16) and SSL (high 16).
    pub crtc_cl_ssl: u32,
    /// CRTC SUR (low 16) and SDR (high 16).
    pub crtc_sur_sdr: u32,
    /// Mask applied to `char_high` to detect kanji. 0xFF for code-access mode,
    /// 0x00 when KAC dot-access mode is selected (which disables kanji decoding).
    pub kanji_high_mask: u8,
    /// True when port 0x68 bit 0 selects semigraphics for attr bit 4.
    pub attr_semigraphics_mode: bool,
    /// True when port 0x68 bit 3 selects 7x13/8x16 mode.
    pub fontsel_8x16: bool,
    /// True when the current frame's blink phase makes blink-attributed cells visible.
    pub blink_visible: bool,
    /// True when the cursor is currently visible.
    pub cursor_visible: bool,
    /// EAD address of the cursor cell.
    pub cursor_addr: u32,
    /// First scanline of the cursor (CSRFORM cursor_top, 0-31).
    pub cursor_top: u32,
    /// Last scanline of the cursor (CSRFORM cursor_bottom, 0-31).
    pub cursor_bottom: u32,

    /// Graphics VRAM B-plane (32 KB) for the active display page.
    pub graphics_b_plane: &'a [u8],
    /// Graphics VRAM R-plane (32 KB) for the active display page.
    pub graphics_r_plane: &'a [u8],
    /// Graphics VRAM G-plane (32 KB) for the active display page.
    pub graphics_g_plane: &'a [u8],
    /// Graphics VRAM E-plane (32 KB) for the active display page.
    pub graphics_e_plane: &'a [u8],
    /// Graphics GDC byte pitch.
    pub gdc_graphics_pitch: u32,
    /// Four packed graphics scroll descriptors.
    pub gdc_graphics_scroll: [u32; 4],
    /// Graphics GDC line repeat factor (CSRFORM).
    pub gdc_graphics_lines_per_row: u32,
    /// Graphics GDC display zoom factor (0-15, rendered as zoom+1).
    pub gdc_graphics_zoom_display: u32,
    /// Graphics GDC active display lines (AL from SYNC command).
    pub gdc_graphics_al: u32,
    /// Bitmask of graphics color indices that are "on" in monochrome mode.
    pub graphics_monochrome_mask: u32,

    /// 16-entry palette packed as `0xAA_BB_GG_RR`.
    pub palette_rgba: [u32; 16],
    /// Display flags (see `DISPLAY_FLAG_*` constants).
    pub display_flags: u32,

    /// PEGC 256-color inputs, present when `DISPLAY_FLAG_PEGC_256_COLOR` is set.
    pub pegc: Option<PegcRenderInputs<'a>>,
}

/// Extra inputs required for PEGC 256-color rendering.
pub struct PegcRenderInputs<'a> {
    /// 256-entry palette packed as `0xAA_BB_GG_RR`.
    pub palette_rgba_256: [u32; 256],
    /// Flags: bit 0 = packed pixel mode, bit 1 = 1-screen (480-line) mode, bit 2 = display page 1.
    pub pegc_flags: u32,
    /// Full 512 KB PEGC VRAM as raw bytes.
    pub vram: &'a [u8],
}

impl SoftwareRenderer {
    /// Native compose-pass output width in pixels.
    pub const WIDTH: usize = 640;
    /// Native compose-pass output height in pixels.
    pub const HEIGHT: usize = 480;
    /// Number of pixels in the framebuffer.
    pub const PIXEL_COUNT: usize = Self::WIDTH * Self::HEIGHT;
    /// Bytes per pixel (`R, G, B, A`).
    pub const PIXEL_BYTES: usize = 4;
    /// Total framebuffer byte size.
    pub const FRAMEBUFFER_BYTES: usize = Self::PIXEL_COUNT * Self::PIXEL_BYTES;

    /// Creates a new renderer with a copy of the supplied font ROM data.
    pub fn new(font_rom_data: &[u8]) -> Self {
        let mut font_rom = vec![0u8; FONT_ROM_BUFFER_SIZE].into_boxed_slice();
        copy_font_rom(&mut font_rom, font_rom_data);
        let framebuffer = vec![0u8; Self::FRAMEBUFFER_BYTES].into_boxed_slice();
        let text_cells: Box<[u32; TEXT_CELL_COUNT]> = vec![0u32; TEXT_CELL_COUNT]
            .into_boxed_slice()
            .try_into()
            .expect("TEXT_CELL_COUNT u32 values");
        Self {
            state: SoftwareRendererState {
                font_rom,
                framebuffer,
                text_cells,
            },
            scratch: compose::ComposeScratch::new(),
            has_avx2: detect_avx2(),
        }
    }

    /// Replaces the font ROM data used by future renders.
    pub fn update_font_rom(&mut self, font_rom_data: &[u8]) {
        copy_font_rom(&mut self.state.font_rom, font_rom_data);
    }

    /// Enables or disables the AVX2 compose dispatch. Intended for parity
    /// testing of the scalar fallback against the SIMD path; production
    /// callers should leave the renderer at its default (AVX2 if detected).
    pub fn set_avx2_enabled(&mut self, enabled: bool) {
        self.has_avx2 = enabled && detect_avx2();
    }

    /// Renders one frame into the internal framebuffer.
    pub fn render(&mut self, inputs: &RenderInputs<'_>) {
        text_normalizer::normalize_text_plane(
            &text_normalizer::TextNormalizerInputs {
                text_vram: inputs.text_vram,
                pitch: inputs.gdc_text_pitch,
                kanji_high_mask: inputs.kanji_high_mask,
                attr_semigraphics_mode: inputs.attr_semigraphics_mode,
                fontsel_8x16: inputs.fontsel_8x16,
                blink_visible: inputs.blink_visible,
                cursor_visible: inputs.cursor_visible,
                cursor_addr: inputs.cursor_addr,
            },
            &mut self.state.text_cells,
        );
        compose::compose(
            &self.state.font_rom,
            &self.state.text_cells,
            inputs,
            &mut self.state.framebuffer,
            &mut self.scratch,
            self.has_avx2,
        );
    }

    /// Returns the internal framebuffer as packed `R, G, B, A` bytes (little-endian per pixel).
    pub fn framebuffer(&self) -> &[u8] {
        &self.state.framebuffer
    }

    /// Computes the active display height (400, or up to 480 for PEGC 480-line mode).
    pub fn native_height(inputs: &RenderInputs<'_>) -> u32 {
        let is_pegc_256_color = (inputs.display_flags & DISPLAY_FLAG_PEGC_256_COLOR) != 0;
        if is_pegc_256_color && inputs.gdc_graphics_al > 400 {
            inputs.gdc_graphics_al.min(Self::HEIGHT as u32)
        } else {
            400
        }
    }

    /// Writes a 640x480 framebuffer to a binary PPM (P6) file.
    pub fn write_ppm(path: impl AsRef<Path>, framebuffer: &[u8]) -> io::Result<()> {
        if framebuffer.len() != Self::FRAMEBUFFER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "software renderer PPM output requires exactly 640x480 RGBA bytes",
            ));
        }

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(b"P6\n640 480\n255\n")?;

        for chunk in framebuffer.chunks_exact(Self::PIXEL_BYTES) {
            writer.write_all(&chunk[0..3])?;
        }

        writer.flush()
    }
}

fn detect_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn copy_font_rom(font_rom: &mut [u8], font_rom_data: &[u8]) {
    font_rom.fill(0);
    let copy_len = font_rom.len().min(font_rom_data.len());
    font_rom[..copy_len].copy_from_slice(&font_rom_data[..copy_len]);
}
