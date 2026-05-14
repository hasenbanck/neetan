//! CPU-side normalization of the PC-98 text plane.
//!
//! Text rendering on the PC-98 is partly stateful: kanji characters span two
//! cells with a leading + trailing JIS byte, gaiji codes alternate between two
//! ROM banks, and the underline attribute "bleeds" into the right edge of the
//! next cell on the same row. We walk the 80x25 text plane on the CPU once
//! per VSYNC and produce a self-contained packed `u32` per cell that the
//! compose loop can read without any neighbor lookups.

use super::{TEXT_CELL_COUNT, TEXT_VRAM_BYTES};

/// Width in bits of the font ROM offset field within a packed cell.
const FONT_OFFSET_BITS: u32 = 20;
/// Mask covering the font ROM offset field.
const FONT_OFFSET_MASK: u32 = (1 << FONT_OFFSET_BITS) - 1;
/// Bit position of the foreground color field.
const COLOR_SHIFT: u32 = 20;
/// Bit position of the `blank` flag.
const FLAG_BLANK: u32 = 1 << 23;
/// Bit position of the `reverse` flag.
const FLAG_REVERSE: u32 = 1 << 24;
/// Bit position of the `underline_this` flag.
const FLAG_UNDERLINE_THIS: u32 = 1 << 25;
/// Bit position of the `underline_left` flag.
const FLAG_UNDERLINE_LEFT: u32 = 1 << 26;
/// Bit position of the `vertical_line` flag.
const FLAG_VLINE: u32 = 1 << 27;
/// Bit position of the `cursor_cell` flag.
const FLAG_CURSOR_CELL: u32 = 1 << 28;

/// Address mask for the cursor EAD field.
const CURSOR_ADDRESS_MASK: u32 = 0x3FFFF;

/// Half of the kanji ROM (left half size in bytes). Adding this to a left-half
/// font offset selects the right half of the same glyph.
const KANJI_RIGHT_HALF_OFFSET: u32 = 0x800;

/// Offset added to the kanji lead font offset when the cell carries the
/// semigraphics attribute (attr bit 4 with ATTRSEL on).
const KANJI_SEMIGRAPHICS_OFFSET: u32 = 8;

/// Base offset of the 8x16 ANK font in the font ROM.
const ANK_8X16_BASE: u32 = 0x80000;
/// Base offset of the 6x8 ANK font in the font ROM.
const ANK_6X8_BASE: u32 = 0x82000;
/// Bytes per ANK glyph in the font ROM.
const ANK_GLYPH_STRIDE: u32 = 16;
/// Offset added for the semigraphics variant in 8x16 mode.
const ANK_8X16_SEMIGFX_OFFSET: u32 = 0x1000;
/// Offset added for the semigraphics variant in 6x8 mode.
const ANK_6X8_SEMIGFX_OFFSET: u32 = 8;

/// Inputs that vary per VSYNC and drive the normalization pass.
pub(super) struct TextNormalizerInputs<'a> {
    /// Full TVRAM image: 0x0000-0x1FFF character bytes, 0x2000-0x3FFF attribute bytes.
    pub text_vram: &'a [u8; TEXT_VRAM_BYTES],
    /// GDC text pitch in cells per row (typically 80).
    pub pitch: u32,
    /// Mask applied to `char_high` to detect kanji. 0xFF for code-access mode,
    /// 0x00 when KAC dot-access mode is selected (which disables kanji decoding).
    pub kanji_high_mask: u8,
    /// True when port 0x68 bit 0 selects semigraphics for attr bit 4
    /// (false = vertical line).
    pub attr_semigraphics_mode: bool,
    /// True when port 0x68 bit 3 selects 7x13/8x16 mode (false = 6x8 mode).
    pub fontsel_8x16: bool,
    /// True when the current frame's blink phase makes blink-attributed cells visible.
    pub blink_visible: bool,
    /// True when the cursor is currently visible.
    pub cursor_visible: bool,
    /// EAD address of the cursor cell.
    pub cursor_addr: u32,
}

/// Normalizes the text plane into self-contained packed cells.
///
/// Walks each TVRAM row left-to-right, carrying forward the per-row state
/// needed to flatten kanji lead/trail pairs, gaiji bank pairing, underline
/// bleed, and cursor inheritance. Cells beyond `pitch * rows` for the active
/// pitch are zeroed.
pub(super) fn normalize_text_plane(
    inputs: &TextNormalizerInputs<'_>,
    out_cells: &mut [u32; TEXT_CELL_COUNT],
) {
    out_cells.fill(0);

    let pitch = inputs.pitch as usize;
    if pitch == 0 {
        return;
    }

    let max_rows = TEXT_CELL_COUNT / pitch;
    let cursor_addr = inputs.cursor_addr & CURSOR_ADDRESS_MASK;

    for row in 0..max_rows {
        let row_base = row * pitch;
        normalize_row(inputs, row_base, pitch, cursor_addr, out_cells);
    }
}

fn normalize_row(
    inputs: &TextNormalizerInputs<'_>,
    row_base: usize,
    pitch: usize,
    cursor_addr: u32,
    out_cells: &mut [u32; TEXT_CELL_COUNT],
) {
    let mut kanji_pending = false;
    let mut kanji_pending_offset: u32 = 0;
    let mut kanji_pending_was_cursor = false;
    let mut prev_was_gaiji_lead = false;
    let mut prev_gaiji_jis: u16 = 0;
    let mut prev_gaiji_was_cursor = false;
    let mut prev_underline = false;

    for col in 0..pitch {
        let cell_idx = row_base + col;
        if cell_idx >= TEXT_CELL_COUNT {
            return;
        }

        let char_offset = cell_idx * 2;
        let char_low = inputs.text_vram[char_offset];
        let char_high = inputs.text_vram[char_offset + 1];
        let attr = inputs.text_vram[char_offset + 0x2000];

        let is_secret = (attr & 0x01) == 0;
        let is_blink = (attr & 0x02) != 0;
        let is_reverse = (attr & 0x04) != 0;
        let is_underline = (attr & 0x08) != 0;
        let attr_bit4 = (attr & 0x10) != 0;
        let is_vline = !inputs.attr_semigraphics_mode && attr_bit4;
        let is_semigfx = inputs.attr_semigraphics_mode && attr_bit4;
        let color = u32::from((attr >> 5) & 0x07);

        let cell_addr_matches_cursor = inputs.cursor_visible && cursor_addr == cell_idx as u32;

        let is_kanji = (char_high & inputs.kanji_high_mask) != 0;
        let row_code = char_low & 0x7F;
        let is_gaiji = is_kanji && (row_code == 0x56 || row_code == 0x57);
        let kanji_jis: u16 = ((u16::from(char_high) << 8) | u16::from(char_low)) & 0x7F7F;
        let lead_offset = u32::from(kanji_jis) << 4;
        let kanji_lead_offset = if is_semigfx {
            lead_offset + KANJI_SEMIGRAPHICS_OFFSET
        } else {
            lead_offset
        };

        let mut cursor_cell = cell_addr_matches_cursor;

        let mut next_kanji_pending = false;
        let mut next_kanji_pending_offset: u32 = 0;
        let mut next_kanji_pending_was_cursor = false;
        let mut next_prev_was_gaiji_lead = false;
        let mut next_prev_gaiji_jis: u16 = 0;
        let mut next_prev_gaiji_was_cursor = false;

        let font_offset = if kanji_pending {
            cursor_cell = cursor_cell || kanji_pending_was_cursor;
            kanji_pending_offset + KANJI_RIGHT_HALF_OFFSET
        } else if is_gaiji {
            if prev_was_gaiji_lead {
                if prev_gaiji_jis == kanji_jis {
                    cursor_cell = cursor_cell || prev_gaiji_was_cursor;
                }
                kanji_lead_offset + KANJI_RIGHT_HALF_OFFSET
            } else {
                next_prev_was_gaiji_lead = true;
                next_prev_gaiji_jis = kanji_jis;
                next_prev_gaiji_was_cursor = cell_addr_matches_cursor;
                kanji_lead_offset
            }
        } else if is_kanji {
            if !(0x09..0x0C).contains(&row_code) {
                next_kanji_pending = true;
                next_kanji_pending_offset = kanji_lead_offset;
                next_kanji_pending_was_cursor = cell_addr_matches_cursor;
            }
            kanji_lead_offset
        } else if inputs.fontsel_8x16 {
            ANK_8X16_BASE
                + u32::from(char_low) * ANK_GLYPH_STRIDE
                + if is_semigfx {
                    ANK_8X16_SEMIGFX_OFFSET
                } else {
                    0
                }
        } else {
            ANK_6X8_BASE
                + u32::from(char_low) * ANK_GLYPH_STRIDE
                + if is_semigfx {
                    ANK_6X8_SEMIGFX_OFFSET
                } else {
                    0
                }
        };

        let blank = is_secret || (is_blink && !inputs.blink_visible);
        let underline_left = prev_underline;

        let mut packed = font_offset & FONT_OFFSET_MASK;
        packed |= color << COLOR_SHIFT;
        if blank {
            packed |= FLAG_BLANK;
        }
        if is_reverse {
            packed |= FLAG_REVERSE;
        }
        if is_underline {
            packed |= FLAG_UNDERLINE_THIS;
        }
        if underline_left {
            packed |= FLAG_UNDERLINE_LEFT;
        }
        if is_vline {
            packed |= FLAG_VLINE;
        }
        if cursor_cell {
            packed |= FLAG_CURSOR_CELL;
        }

        out_cells[cell_idx] = packed;

        kanji_pending = next_kanji_pending;
        kanji_pending_offset = next_kanji_pending_offset;
        kanji_pending_was_cursor = next_kanji_pending_was_cursor;
        prev_was_gaiji_lead = next_prev_was_gaiji_lead;
        prev_gaiji_jis = next_prev_gaiji_jis;
        prev_gaiji_was_cursor = next_prev_gaiji_was_cursor;
        prev_underline = is_underline;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_vram() -> Box<[u8; TEXT_VRAM_BYTES]> {
        vec![0u8; TEXT_VRAM_BYTES]
            .into_boxed_slice()
            .try_into()
            .unwrap()
    }

    fn default_inputs<'a>(text_vram: &'a [u8; TEXT_VRAM_BYTES]) -> TextNormalizerInputs<'a> {
        TextNormalizerInputs {
            text_vram,
            pitch: 80,
            kanji_high_mask: 0xFF,
            attr_semigraphics_mode: false,
            fontsel_8x16: true,
            blink_visible: true,
            cursor_visible: false,
            cursor_addr: 0,
        }
    }

    fn write_cell(vram: &mut [u8; TEXT_VRAM_BYTES], cell_idx: usize, low: u8, high: u8, attr: u8) {
        vram[cell_idx * 2] = low;
        vram[cell_idx * 2 + 1] = high;
        vram[cell_idx * 2 + 0x2000] = attr;
    }

    fn cell_font_offset(packed: u32) -> u32 {
        packed & FONT_OFFSET_MASK
    }

    fn cell_color(packed: u32) -> u32 {
        (packed >> COLOR_SHIFT) & 0x07
    }

    fn cell_has(packed: u32, flag: u32) -> bool {
        (packed & flag) != 0
    }

    #[test]
    fn ank_8x16_encodes_base_and_color() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 0, b'A', 0x00, 0xE1);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        let cell = out[0];
        assert_eq!(cell_font_offset(cell), ANK_8X16_BASE + (b'A' as u32) * 16);
        assert_eq!(cell_color(cell), 0x07);
        assert!(!cell_has(cell, FLAG_BLANK));
    }

    #[test]
    fn ank_6x8_encodes_base() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 5, b'Z', 0x00, 0x21);

        let mut inputs = default_inputs(&vram);
        inputs.fontsel_8x16 = false;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert_eq!(cell_font_offset(out[5]), ANK_6X8_BASE + (b'Z' as u32) * 16);
        assert_eq!(cell_color(out[5]), 0x01);
    }

    #[test]
    fn semigraphics_offset_added() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 1, 0x80, 0x00, 0x11);

        let mut inputs = default_inputs(&vram);
        inputs.attr_semigraphics_mode = true;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert_eq!(
            cell_font_offset(out[1]),
            ANK_8X16_BASE + 0x80 * 16 + ANK_8X16_SEMIGFX_OFFSET
        );
        assert!(!cell_has(out[1], FLAG_VLINE));
    }

    #[test]
    fn vline_set_when_attr_semigraphics_disabled() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 2, b'B', 0x00, 0x11);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[2], FLAG_VLINE));
    }

    #[test]
    fn secret_bit_blanks_cell() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 0, b'X', 0x00, 0x00);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[0], FLAG_BLANK));
    }

    #[test]
    fn blink_invisible_blanks_cell() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 0, b'X', 0x00, 0x03);

        let mut inputs = default_inputs(&vram);
        inputs.blink_visible = false;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[0], FLAG_BLANK));

        inputs.blink_visible = true;
        normalize_text_plane(&inputs, &mut out);
        assert!(!cell_has(out[0], FLAG_BLANK));
    }

    #[test]
    fn reverse_attribute_encoded() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 0, b'X', 0x00, 0x05);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[0], FLAG_REVERSE));
    }

    #[test]
    fn underline_left_propagates_within_row() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 0, b'A', 0x00, 0x09);
        write_cell(&mut vram, 1, b'B', 0x00, 0x01);
        write_cell(&mut vram, 2, b'C', 0x00, 0x01);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[0], FLAG_UNDERLINE_THIS));
        assert!(!cell_has(out[0], FLAG_UNDERLINE_LEFT));
        assert!(!cell_has(out[1], FLAG_UNDERLINE_THIS));
        assert!(cell_has(out[1], FLAG_UNDERLINE_LEFT));
        assert!(!cell_has(out[2], FLAG_UNDERLINE_LEFT));
    }

    #[test]
    fn underline_left_resets_at_row_boundary() {
        let mut vram = empty_vram();
        let last_col = 79;
        write_cell(&mut vram, last_col, b'A', 0x00, 0x09);
        write_cell(&mut vram, 80, b'B', 0x00, 0x01);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[last_col], FLAG_UNDERLINE_THIS));
        assert!(!cell_has(out[80], FLAG_UNDERLINE_LEFT));
    }

    #[test]
    fn kanji_lead_marks_trail_with_right_half_offset() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 10, 0x21, 0x21, 0x01);
        write_cell(&mut vram, 11, 0x00, 0x00, 0x01);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        let lead_offset = (0x2121u32 & 0x7F7F) << 4;
        assert_eq!(cell_font_offset(out[10]), lead_offset);
        assert_eq!(
            cell_font_offset(out[11]),
            lead_offset + KANJI_RIGHT_HALF_OFFSET
        );
    }

    #[test]
    fn half_width_kana_lead_codes_do_not_pair() {
        let mut vram = empty_vram();
        for (i, row_code) in [0x09u8, 0x0A, 0x0B].iter().enumerate() {
            let cell = 20 + i * 2;
            write_cell(&mut vram, cell, *row_code, 0xA1, 0x01);
            write_cell(&mut vram, cell + 1, b'X', 0x00, 0x01);
        }

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        for (i, _) in [0x09u8, 0x0A, 0x0B].iter().enumerate() {
            let following_cell = 20 + i * 2 + 1;
            assert_eq!(
                cell_font_offset(out[following_cell]),
                ANK_8X16_BASE + (b'X' as u32) * 16,
                "cell after half-width kana lead {i} should be ANK, not kanji trail"
            );
        }
    }

    #[test]
    fn gaiji_pair_uses_left_then_right_half() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 30, 0x56, 0xA1, 0x01);
        write_cell(&mut vram, 31, 0x56, 0xA1, 0x01);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        let lead_offset = (0xA156u32 & 0x7F7F) << 4;
        assert_eq!(cell_font_offset(out[30]), lead_offset);
        assert_eq!(
            cell_font_offset(out[31]),
            lead_offset + KANJI_RIGHT_HALF_OFFSET
        );
    }

    #[test]
    fn cursor_flag_set_on_matching_cell() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 7, b'A', 0x00, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.cursor_visible = true;
        inputs.cursor_addr = 7;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[7], FLAG_CURSOR_CELL));
        assert!(!cell_has(out[6], FLAG_CURSOR_CELL));
        assert!(!cell_has(out[8], FLAG_CURSOR_CELL));
    }

    #[test]
    fn cursor_inherits_to_kanji_right_half() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 12, 0x21, 0x21, 0x01);
        write_cell(&mut vram, 13, 0x00, 0x00, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.cursor_visible = true;
        inputs.cursor_addr = 12;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[12], FLAG_CURSOR_CELL));
        assert!(cell_has(out[13], FLAG_CURSOR_CELL));
    }

    #[test]
    fn cursor_invisible_clears_flag() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 7, b'A', 0x00, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.cursor_visible = false;
        inputs.cursor_addr = 7;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(!cell_has(out[7], FLAG_CURSOR_CELL));
    }

    #[test]
    fn kac_dot_access_disables_kanji_decoding() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 0, 0x21, 0x21, 0x01);
        write_cell(&mut vram, 1, 0x00, 0x00, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.kanji_high_mask = 0x00;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert_eq!(cell_font_offset(out[0]), ANK_8X16_BASE + 0x21 * 16);
        assert_eq!(cell_font_offset(out[1]), ANK_8X16_BASE);
    }

    #[test]
    fn gaiji_mismatched_pair_emits_right_half() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 40, 0x56, 0xA1, 0x01);
        write_cell(&mut vram, 41, 0x56, 0xA2, 0x01);

        let inputs = default_inputs(&vram);
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        let lead_offset_a = (0xA156u32 & 0x7F7F) << 4;
        let lead_offset_b = (0xA256u32 & 0x7F7F) << 4;
        assert_eq!(cell_font_offset(out[40]), lead_offset_a);
        assert_eq!(
            cell_font_offset(out[41]),
            lead_offset_b + KANJI_RIGHT_HALF_OFFSET,
            "second gaiji uses its OWN right half regardless of JIS match"
        );
    }

    #[test]
    fn gaiji_mismatched_pair_does_not_inherit_cursor() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 50, 0x56, 0xA1, 0x01);
        write_cell(&mut vram, 51, 0x56, 0xA2, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.cursor_visible = true;
        inputs.cursor_addr = 50;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(cell_has(out[50], FLAG_CURSOR_CELL));
        assert!(
            !cell_has(out[51], FLAG_CURSOR_CELL),
            "cursor must not inherit when JIS codes differ"
        );
    }

    #[test]
    fn kanji_lead_with_semigraphics_adds_eight() {
        let mut vram = empty_vram();
        // Kanji at JIS 0x2121 with attr bit 4 set.
        write_cell(&mut vram, 60, 0x21, 0x21, 0x11);
        write_cell(&mut vram, 61, 0x00, 0x00, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.attr_semigraphics_mode = true;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        let lead_offset = (0x2121u32 & 0x7F7F) << 4;
        assert_eq!(cell_font_offset(out[60]), lead_offset + 8);
    }

    #[test]
    fn kanji_trail_inherits_semigraphics_offset() {
        let mut vram = empty_vram();
        write_cell(&mut vram, 70, 0x21, 0x21, 0x11);
        write_cell(&mut vram, 71, 0x00, 0x00, 0x01);

        let mut inputs = default_inputs(&vram);
        inputs.attr_semigraphics_mode = true;
        let mut out = [0u32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        let lead_offset = (0x2121u32 & 0x7F7F) << 4;
        assert_eq!(
            cell_font_offset(out[71]),
            lead_offset + 8 + KANJI_RIGHT_HALF_OFFSET,
            "trail must inherit the lead's +8 semigraphics offset"
        );
    }

    #[test]
    fn pitch_zero_produces_zero_cells() {
        let vram = empty_vram();
        let mut inputs = default_inputs(&vram);
        inputs.pitch = 0;
        let mut out = [0xDEAD_BEEFu32; TEXT_CELL_COUNT];
        normalize_text_plane(&inputs, &mut out);

        assert!(out.iter().all(|&c| c == 0));
    }
}
