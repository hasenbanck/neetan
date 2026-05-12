//! NEON-accelerated cell-row combine inner loops for the PC-98 software renderer.
//!
//! Native 128-bit pipeline: every stage runs on full Q registers and the loop
//! body processes two 8-pixel cells (16 pixels) per iteration. Palette indices
//! for the digital paths are computed arithmetically from the four plane bytes
//! per cell, so the AVX2-shared `PLANE_INDEX_*` lookup tables are not used.
//! PEGC gathers 16 RGBA palette entries via independent scalar loads and
//! deinterleaves them with `vld4q_u8`. The framebuffer is written with a
//! single `vst4q_u8` (64 bytes interleaved RGBA) per pair of cells.

use core::arch::aarch64::*;

use super::{
    BLACK, CELL_STRIDE, CELLS_PER_ROW, ComposeScratch, DigitalSimdPalette, FIXED_TEXT_LUT,
    PIXELS_PER_CELL, ROW_BYTES,
};

/// 16-byte bit-mask vector covering two 8-pixel cells. The same vector drives
/// per-plane bit extraction and text-overlay mask construction.
const BIT_MASKS: [u8; 16] = [
    0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01, 0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01,
];

#[inline]
unsafe fn broadcast_pair(a: u8, b: u8) -> uint8x16_t {
    unsafe { vcombine_u8(vdup_n_u8(a), vdup_n_u8(b)) }
}

#[inline]
unsafe fn text_rgba_pair(p0: u32, p1: u32) -> (uint8x16_t, uint8x16_t, uint8x16_t, uint8x16_t) {
    unsafe {
        (
            broadcast_pair(p0 as u8, p1 as u8),
            broadcast_pair((p0 >> 8) as u8, (p1 >> 8) as u8),
            broadcast_pair((p0 >> 16) as u8, (p1 >> 16) as u8),
            broadcast_pair((p0 >> 24) as u8, (p1 >> 24) as u8),
        )
    }
}

/// Loop-invariant Q vectors used by every cell pair in the digital paths.
#[derive(Clone, Copy)]
struct IndexEnv {
    bit_masks: uint8x16_t,
    contrib_b: uint8x16_t,
    contrib_r: uint8x16_t,
    contrib_g: uint8x16_t,
    contrib_e: uint8x16_t,
}

#[inline]
unsafe fn index_env() -> IndexEnv {
    unsafe {
        IndexEnv {
            bit_masks: vld1q_u8(BIT_MASKS.as_ptr()),
            contrib_b: vdupq_n_u8(1),
            contrib_r: vdupq_n_u8(2),
            contrib_g: vdupq_n_u8(4),
            contrib_e: vdupq_n_u8(8),
        }
    }
}

#[inline]
unsafe fn compute_indices(
    env: IndexEnv,
    b: uint8x16_t,
    r: uint8x16_t,
    g: uint8x16_t,
    e: uint8x16_t,
) -> uint8x16_t {
    unsafe {
        let b_bits = vandq_u8(vtstq_u8(b, env.bit_masks), env.contrib_b);
        let r_bits = vandq_u8(vtstq_u8(r, env.bit_masks), env.contrib_r);
        let g_bits = vandq_u8(vtstq_u8(g, env.bit_masks), env.contrib_g);
        let e_bits = vandq_u8(vtstq_u8(e, env.bit_masks), env.contrib_e);
        vorrq_u8(vorrq_u8(b_bits, r_bits), vorrq_u8(g_bits, e_bits))
    }
}

pub(super) unsafe fn combine_cells_digital_color_neon(
    row_buf: &mut [u8],
    scratch: &ComposeScratch,
    palette: &DigitalSimdPalette,
) {
    debug_assert_eq!(row_buf.len(), ROW_BYTES);
    debug_assert_eq!(CELLS_PER_ROW % 2, 0);

    unsafe {
        let env = index_env();
        let red_tbl = vld1q_u8(palette.graphics_red.as_ptr());
        let green_tbl = vld1q_u8(palette.graphics_green.as_ptr());
        let blue_tbl = vld1q_u8(palette.graphics_blue.as_ptr());
        let alpha_tbl = vld1q_u8(palette.graphics_alpha.as_ptr());

        let row_ptr = row_buf.as_mut_ptr();
        let mut cell = 0usize;
        while cell < CELLS_PER_ROW {
            let b = broadcast_pair(scratch.graphics_b[cell], scratch.graphics_b[cell + 1]);
            let r = broadcast_pair(scratch.graphics_r[cell], scratch.graphics_r[cell + 1]);
            let g = broadcast_pair(scratch.graphics_g[cell], scratch.graphics_g[cell + 1]);
            let e = broadcast_pair(scratch.graphics_e[cell], scratch.graphics_e[cell + 1]);

            let idx = compute_indices(env, b, r, g, e);

            let red = vqtbl1q_u8(red_tbl, idx);
            let green = vqtbl1q_u8(green_tbl, idx);
            let blue = vqtbl1q_u8(blue_tbl, idx);
            let alpha = vqtbl1q_u8(alpha_tbl, idx);

            let text_bytes = broadcast_pair(scratch.text_byte[cell], scratch.text_byte[cell + 1]);
            let text_mask = vtstq_u8(text_bytes, env.bit_masks);
            let tp0 = *palette
                .text_pixels
                .get_unchecked((scratch.text_color[cell] & 0x07) as usize);
            let tp1 = *palette
                .text_pixels
                .get_unchecked((scratch.text_color[cell + 1] & 0x07) as usize);
            let (t_r, t_g, t_b, t_a) = text_rgba_pair(tp0, tp1);
            // Text has priority over graphics in color digital modes.
            let red = vbslq_u8(text_mask, t_r, red);
            let green = vbslq_u8(text_mask, t_g, green);
            let blue = vbslq_u8(text_mask, t_b, blue);
            let alpha = vbslq_u8(text_mask, t_a, alpha);

            let rgba = uint8x16x4_t(red, green, blue, alpha);
            vst4q_u8(row_ptr.add(cell * CELL_STRIDE), rgba);

            cell += 2;
        }
    }
}

pub(super) unsafe fn combine_cells_digital_mono_neon(
    row_buf: &mut [u8],
    scratch: &ComposeScratch,
    palette: &DigitalSimdPalette,
) {
    debug_assert_eq!(row_buf.len(), ROW_BYTES);

    unsafe {
        // With text disabled the monochrome path is pure graphics and can use
        // channel shuffle tables. With text enabled, graphics-on pixels borrow
        // the current cell text color, so it is cheaper to build a blend mask.
        if palette.text_enabled {
            combine_cells_digital_mono_text_neon(row_buf, scratch, palette);
        } else {
            combine_cells_digital_mono_graphics_neon(row_buf, scratch, palette);
        }
    }
}

#[inline]
unsafe fn combine_cells_digital_mono_text_neon(
    row_buf: &mut [u8],
    scratch: &ComposeScratch,
    palette: &DigitalSimdPalette,
) {
    debug_assert_eq!(CELLS_PER_ROW % 2, 0);

    unsafe {
        let env = index_env();
        let mono_mask_tbl = vld1q_u8(palette.mono_mask.as_ptr());
        let (pz_r, pz_g, pz_b, pz_a) = text_rgba_pair(palette.palette_zero, palette.palette_zero);

        let row_ptr = row_buf.as_mut_ptr();
        let mut cell = 0usize;
        while cell < CELLS_PER_ROW {
            let b = broadcast_pair(scratch.graphics_b[cell], scratch.graphics_b[cell + 1]);
            let r = broadcast_pair(scratch.graphics_r[cell], scratch.graphics_r[cell + 1]);
            let g = broadcast_pair(scratch.graphics_g[cell], scratch.graphics_g[cell + 1]);
            let e = broadcast_pair(scratch.graphics_e[cell], scratch.graphics_e[cell + 1]);

            let idx = compute_indices(env, b, r, g, e);

            // Monochrome mixed mode colors both text pixels and graphics-on pixels
            // with the cell text color. Everything else falls back to palette[0].
            let mono_on = vqtbl1q_u8(mono_mask_tbl, idx);
            let text_bytes = broadcast_pair(scratch.text_byte[cell], scratch.text_byte[cell + 1]);
            let text_mask = vtstq_u8(text_bytes, env.bit_masks);
            let combined = vorrq_u8(mono_on, text_mask);

            let tp0 = *palette
                .text_pixels
                .get_unchecked((scratch.text_color[cell] & 0x07) as usize);
            let tp1 = *palette
                .text_pixels
                .get_unchecked((scratch.text_color[cell + 1] & 0x07) as usize);
            let (t_r, t_g, t_b, t_a) = text_rgba_pair(tp0, tp1);

            let red = vbslq_u8(combined, t_r, pz_r);
            let green = vbslq_u8(combined, t_g, pz_g);
            let blue = vbslq_u8(combined, t_b, pz_b);
            let alpha = vbslq_u8(combined, t_a, pz_a);

            let rgba = uint8x16x4_t(red, green, blue, alpha);
            vst4q_u8(row_ptr.add(cell * CELL_STRIDE), rgba);

            cell += 2;
        }
    }
}

#[inline]
unsafe fn combine_cells_digital_mono_graphics_neon(
    row_buf: &mut [u8],
    scratch: &ComposeScratch,
    palette: &DigitalSimdPalette,
) {
    debug_assert_eq!(CELLS_PER_ROW % 2, 0);

    unsafe {
        let env = index_env();
        let red_tbl = vld1q_u8(palette.mono_red.as_ptr());
        let green_tbl = vld1q_u8(palette.mono_green.as_ptr());
        let blue_tbl = vld1q_u8(palette.mono_blue.as_ptr());
        let alpha_tbl = vld1q_u8(palette.mono_alpha.as_ptr());

        let row_ptr = row_buf.as_mut_ptr();
        let mut cell = 0usize;
        while cell < CELLS_PER_ROW {
            let b = broadcast_pair(scratch.graphics_b[cell], scratch.graphics_b[cell + 1]);
            let r = broadcast_pair(scratch.graphics_r[cell], scratch.graphics_r[cell + 1]);
            let g = broadcast_pair(scratch.graphics_g[cell], scratch.graphics_g[cell + 1]);
            let e = broadcast_pair(scratch.graphics_e[cell], scratch.graphics_e[cell + 1]);

            let idx = compute_indices(env, b, r, g, e);

            // Graphics-only monochrome mode maps the selected indices to fixed
            // white and unselected indices to palette[0].
            let red = vqtbl1q_u8(red_tbl, idx);
            let green = vqtbl1q_u8(green_tbl, idx);
            let blue = vqtbl1q_u8(blue_tbl, idx);
            let alpha = vqtbl1q_u8(alpha_tbl, idx);

            let rgba = uint8x16x4_t(red, green, blue, alpha);
            vst4q_u8(row_ptr.add(cell * CELL_STRIDE), rgba);

            cell += 2;
        }
    }
}

pub(super) unsafe fn combine_cells_pegc_neon(
    row_buf: &mut [u8],
    scratch: &ComposeScratch,
    palette_256: &[u32; 256],
    graphics_enabled: bool,
) {
    debug_assert_eq!(row_buf.len(), ROW_BYTES);
    debug_assert_eq!(CELLS_PER_ROW % 2, 0);

    unsafe {
        let bit_masks = vld1q_u8(BIT_MASKS.as_ptr());
        let black_r = vdupq_n_u8(BLACK as u8);
        let black_g = vdupq_n_u8((BLACK >> 8) as u8);
        let black_b = vdupq_n_u8((BLACK >> 16) as u8);
        let black_a = vdupq_n_u8((BLACK >> 24) as u8);
        let row_ptr = row_buf.as_mut_ptr();

        let mut cell = 0usize;
        while cell < CELLS_PER_ROW {
            let (red, green, blue, alpha) = if graphics_enabled {
                // PEGC uses 8-bit palette indices. Sixteen independent scalar
                // loads gather one RGBA pixel each; `vld4q_u8` then deinterleaves
                // the resulting 64-byte stack buffer into per-channel Q vectors.
                let idx_ptr = scratch.pegc_indices.as_ptr().add(cell * PIXELS_PER_CELL);
                let palette_ptr = palette_256.as_ptr();
                let mut tmp = [0u32; 16];
                let mut pixel = 0usize;
                while pixel < 16 {
                    tmp[pixel] = *palette_ptr.add(*idx_ptr.add(pixel) as usize);
                    pixel += 1;
                }
                let deint = vld4q_u8(tmp.as_ptr() as *const u8);
                (deint.0, deint.1, deint.2, deint.3)
            } else {
                (black_r, black_g, black_b, black_a)
            };

            let text_bytes = broadcast_pair(scratch.text_byte[cell], scratch.text_byte[cell + 1]);
            let text_mask = vtstq_u8(text_bytes, bit_masks);
            let tp0 = FIXED_TEXT_LUT[(scratch.text_color[cell] & 0x07) as usize];
            let tp1 = FIXED_TEXT_LUT[(scratch.text_color[cell + 1] & 0x07) as usize];
            let (t_r, t_g, t_b, t_a) = text_rgba_pair(tp0, tp1);
            // Text overlay keeps using fixed 8-color text values on top of PEGC.
            let red = vbslq_u8(text_mask, t_r, red);
            let green = vbslq_u8(text_mask, t_g, green);
            let blue = vbslq_u8(text_mask, t_b, blue);
            let alpha = vbslq_u8(text_mask, t_a, alpha);

            let rgba = uint8x16x4_t(red, green, blue, alpha);
            vst4q_u8(row_ptr.add(cell * CELL_STRIDE), rgba);

            cell += 2;
        }
    }
}
