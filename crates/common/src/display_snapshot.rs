#[cfg(not(target_endian = "little"))]
compile_error!("DisplaySnapshotUpload requires a little-endian target");

/// Typed display snapshot uploaded from CPU emulation to the GPU compose pass.
///
/// The binary layout of this struct is stable and shared with shader code.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DisplaySnapshotUpload {
    /// Palette entries packed as 0xAA_BB_GG_RR.
    pub palette_rgba: [u32; 16],
    /// Display flags (bit 0 = display active, bit 1 = blink visible, bit 2 = hide odd rasters,
    /// bit 3 = 16-color mode, bit 4 = text display enabled, bit 5 = graphics display enabled,
    /// bit 6 = global display enable).
    pub display_flags: u32,
    /// Text pitch from the master GDC.
    pub gdc_text_pitch: u32,
    /// Four packed text scroll descriptors: low 16 bits = start address, high 16 bits = line count.
    pub gdc_scroll_start_line: [u32; 4],
    /// Four packed graphics scroll descriptors: low 16 bits = start address, high 16 bits = line count.
    pub gdc_graphics_scroll: [u32; 4],
    /// Graphics GDC pitch (words per row, typically 40).
    pub gdc_graphics_pitch: u32,
    /// Video mode register (port 0x68 value).
    pub video_mode: u32,
    /// Graphics GDC lines per character row (CSRFORM line repeat factor).
    pub gdc_graphics_lines_per_row: u32,
    /// Graphics GDC display zoom factor (0-15, rendered as zoom+1).
    pub gdc_graphics_zoom_display: u32,
    /// Interlace mode from GDC SYNC command (0x00=non-interlace, 0x08=repeat, 0x09=on).
    pub gdc_interlace_mode: u32,
    /// Bitmask of graphics color indices that are "on" in monochrome mode.
    pub graphics_monochrome_mask: u32,
    /// KAC-mode-derived mask used for kanji high-byte detection in compose.
    pub gdc_text_kanji_high_mask: u32,
    /// CRTC PL (low 16) and BL (high 16).
    pub crtc_pl_bl: u32,
    /// CRTC CL (low 16) and SSL (high 16).
    pub crtc_cl_ssl: u32,
    /// CRTC SUR (low 16) and SDR (high 16).
    pub crtc_sur_sdr: u32,
    /// Text cursor: bit 31 = visible, bits 0-17 = EAD address from master GDC.
    pub text_cursor: u32,
    /// Graphics GDC active display lines (AL from SYNC command, 0-1023).
    pub gdc_graphics_al: u32,
    /// Reserved header words so text VRAM starts at byte offset 0x100.
    pub reserved_header_words: [u32; 26],
    /// Text VRAM bytes as 32-bit little-endian words.
    pub text_vram_words: [u32; 0x4000 / 4],
    /// Graphics VRAM B-plane (32 KB) as 32-bit little-endian words.
    pub graphics_b_plane: [u32; 0x8000 / 4],
    /// Graphics VRAM R-plane (32 KB) as 32-bit little-endian words.
    pub graphics_r_plane: [u32; 0x8000 / 4],
    /// Graphics VRAM G-plane (32 KB) as 32-bit little-endian words.
    pub graphics_g_plane: [u32; 0x8000 / 4],
    /// Graphics VRAM E-plane (32 KB) as 32-bit little-endian words.
    pub graphics_e_plane: [u32; 0x8000 / 4],
}

impl DisplaySnapshotUpload {
    /// Total byte size of the upload payload.
    pub const BYTE_SIZE: usize = 147_712;

    /// Returns the raw byte representation of this struct.
    ///
    /// # Safety justification
    ///
    /// Sound because `Self` is `#[repr(C)]`, composed entirely of `u32` (valid for
    /// any bit pattern), and every byte of the struct is initialized. The returned
    /// slice borrows `self` so the lifetime is correct, and the size is exact.
    #[allow(unsafe_code)]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, size_of::<Self>()) }
    }

    /// Returns a zero-initialized instance.
    ///
    /// # Safety justification
    ///
    /// Sound because every field is `u32` (or `[u32; N]`), and zero is a valid
    /// value for `u32`. There are no padding bytes to worry about because the
    /// struct is `#[repr(C)]` with only `u32`-aligned `u32` fields.
    #[allow(unsafe_code)]
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

/// Reinterprets a `&mut [u32]` slice as `&mut [u8]`.
///
/// # Safety justification
///
/// Sound because `u32` is valid for any bit pattern, so writing arbitrary bytes
/// into the returned slice can never produce an invalid `u32`. The pointer is
/// already suitably aligned (u8 has no alignment requirement), and the byte
/// length is exact (`len * 4`). The exclusive borrow is forwarded so no aliasing
/// occurs.
#[allow(unsafe_code)]
pub fn cast_u32_slice_as_bytes_mut(slice: &mut [u32]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), size_of_val(slice)) }
}

impl Default for DisplaySnapshotUpload {
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Display flags bit 7: PEGC 256-color mode active.
pub const DISPLAY_FLAG_PEGC_256_COLOR: u32 = 0x80;

const _: [(); DisplaySnapshotUpload::BYTE_SIZE] = [(); size_of::<DisplaySnapshotUpload>()];

/// PEGC snapshot uploaded to a separate GPU buffer when 256-color mode is active.
///
/// Contains the 256-entry palette and the full 512 KB extended VRAM.
/// Bound at descriptor binding 4 in the compose shader.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PegcSnapshotUpload {
    /// 256-color palette entries packed as 0xAA_BB_GG_RR.
    pub palette_rgba_256: [u32; 256],
    /// Flags: bit 0 = packed pixel mode, bit 1 = 1-screen (480-line) mode, bit 2 = display page 1.
    pub pegc_flags: u32,
    /// Reserved for alignment.
    pub reserved: [u32; 3],
    /// Full 512 KB PEGC VRAM as 32-bit little-endian words.
    pub pegc_vram: [u32; 0x80000 / 4],
}

impl PegcSnapshotUpload {
    /// Total byte size of the upload payload.
    pub const BYTE_SIZE: usize = 256 * 4 + 4 + 3 * 4 + 0x80000;

    /// Returns the raw byte representation of this struct.
    ///
    /// # Safety justification
    ///
    /// Sound because `Self` is `#[repr(C)]`, composed entirely of `u32` (valid for
    /// any bit pattern), and every byte of the struct is initialized.
    #[allow(unsafe_code)]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, size_of::<Self>()) }
    }

    /// Returns a zero-initialized instance.
    ///
    /// # Safety justification
    ///
    /// Sound because every field is `u32` (or `[u32; N]`), and zero is a valid value for `u32`.
    #[allow(unsafe_code)]
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl Default for PegcSnapshotUpload {
    fn default() -> Self {
        Self::zeroed()
    }
}

const _: [(); PegcSnapshotUpload::BYTE_SIZE] = [(); size_of::<PegcSnapshotUpload>()];

#[cfg(test)]
mod tests {
    use std::mem::{offset_of, size_of};

    use super::{DisplaySnapshotUpload, PegcSnapshotUpload};

    #[test]
    fn display_snapshot_layout_matches_expected_offsets() {
        assert_eq!(
            size_of::<DisplaySnapshotUpload>(),
            DisplaySnapshotUpload::BYTE_SIZE
        );
        assert_eq!(offset_of!(DisplaySnapshotUpload, palette_rgba), 0x000);
        assert_eq!(offset_of!(DisplaySnapshotUpload, display_flags), 0x040);
        assert_eq!(offset_of!(DisplaySnapshotUpload, gdc_text_pitch), 0x044);
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, gdc_scroll_start_line),
            0x048
        );
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, gdc_graphics_scroll),
            0x058
        );
        assert_eq!(offset_of!(DisplaySnapshotUpload, gdc_graphics_pitch), 0x068);
        assert_eq!(offset_of!(DisplaySnapshotUpload, video_mode), 0x06C);
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, gdc_graphics_lines_per_row),
            0x070
        );
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, gdc_graphics_zoom_display),
            0x074
        );
        assert_eq!(offset_of!(DisplaySnapshotUpload, gdc_interlace_mode), 0x078);
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, graphics_monochrome_mask),
            0x07C
        );
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, gdc_text_kanji_high_mask),
            0x080
        );
        assert_eq!(offset_of!(DisplaySnapshotUpload, crtc_pl_bl), 0x084);
        assert_eq!(offset_of!(DisplaySnapshotUpload, crtc_cl_ssl), 0x088);
        assert_eq!(offset_of!(DisplaySnapshotUpload, crtc_sur_sdr), 0x08C);
        assert_eq!(offset_of!(DisplaySnapshotUpload, text_cursor), 0x090);
        assert_eq!(offset_of!(DisplaySnapshotUpload, gdc_graphics_al), 0x094);
        assert_eq!(
            offset_of!(DisplaySnapshotUpload, reserved_header_words),
            0x098
        );
        assert_eq!(offset_of!(DisplaySnapshotUpload, text_vram_words), 0x100);
        assert_eq!(offset_of!(DisplaySnapshotUpload, graphics_b_plane), 0x4100);
        assert_eq!(offset_of!(DisplaySnapshotUpload, graphics_r_plane), 0xC100);
        assert_eq!(offset_of!(DisplaySnapshotUpload, graphics_g_plane), 0x14100);
        assert_eq!(offset_of!(DisplaySnapshotUpload, graphics_e_plane), 0x1C100);
    }

    #[test]
    fn pegc_snapshot_layout_matches_expected_offsets() {
        assert_eq!(
            size_of::<PegcSnapshotUpload>(),
            PegcSnapshotUpload::BYTE_SIZE
        );
        assert_eq!(offset_of!(PegcSnapshotUpload, palette_rgba_256), 0x000);
        assert_eq!(offset_of!(PegcSnapshotUpload, pegc_flags), 0x400);
        assert_eq!(offset_of!(PegcSnapshotUpload, reserved), 0x404);
        assert_eq!(offset_of!(PegcSnapshotUpload, pegc_vram), 0x410);
    }
}
