#language slang 2026

import screen_space;
import transform;

struct DisplaySnapshotUpload {
    uint palette_rgba[16];
    uint display_flags;
    uint gdc_text_pitch;
    uint gdc_scroll_start_line[4];
    uint gdc_graphics_scroll[4];
    uint gdc_graphics_pitch;
    uint video_mode;
    uint gdc_graphics_lines_per_row;
    uint gdc_graphics_zoom_display;
    uint gdc_interlace_mode;
    uint graphics_monochrome_mask;
    uint gdc_text_kanji_high_mask;
    uint crtc_pl_bl;
    uint crtc_cl_ssl;
    uint crtc_sur_sdr;
    uint text_cursor;
    uint reserved_header_words[27];
    uint text_vram_words[4096];
    uint graphics_b_plane[8192];
    uint graphics_r_plane[8192];
    uint graphics_g_plane[8192];
    uint graphics_e_plane[8192];
};

[[vk::binding(2, 1)]] StructuredBuffer<DisplaySnapshotUpload> upload_buffer;

struct FontRomData {
    uint font_rom_words[134144];
};

[[vk::binding(3, 1)]] StructuredBuffer<FontRomData> font_rom_buffer;

[[shader("vertex")]]
func vs_main(uint vertex_index : SV_VulkanVertexID) -> FullscreenVertex {
    return FullscreenVertex::new(vertex_index);
}

[[shader("pixel")]]
func fs_main(FullscreenVertex input) -> float4 {
    let pixel = int2(input.position.xy);
    let x = uint(pixel.x);
    let y = uint(pixel.y);

    if (x >= 640 || y >= 400) {
        return float4(0.0, 0.0, 0.0, 1.0);
    }

    let display_enabled_global = (upload_buffer[0].display_flags & 0x40) != 0;
    if (!display_enabled_global) {
        return float4(0.0, 0.0, 0.0, 1.0);
    }

    let graphics_enabled = (upload_buffer[0].display_flags & 0x20) != 0;
    let graphics_color = graphics_enabled ? compute_graphics_pixel(x, y) : 0u;

    let text_enabled = (upload_buffer[0].display_flags & 0x10) != 0;
    var text_on: bool;
    var text_color_index: uint;
    if (text_enabled) {
        compute_text_pixel(x, y, text_on, text_color_index);
    } else {
        text_on = false;
        text_color_index = 0;
    }

    let is_monochrome = (upload_buffer[0].video_mode & 0x02) != 0;

    var color_index: uint;
    if (is_monochrome && text_enabled && graphics_enabled) {
        let graphics_on = ((upload_buffer[0].graphics_monochrome_mask >> graphics_color) & 1) != 0;
        if (text_on) {
            color_index = text_color_index;
        } else if (graphics_on) {
            color_index = text_color_index;
        } else {
            color_index = 0;
        }
    } else {
        if (text_on) {
            color_index = text_color_index;
        } else {
            color_index = graphics_color;
        }
    }

    let packed = upload_buffer[0].palette_rgba[color_index];
    return color::unpack_srgb(packed);
}

[ForceInline]
func compute_graphics_pixel(x: uint, y: uint) -> uint {
    let hide_odd = (upload_buffer[0].display_flags & 0x04) != 0;
    if (hide_odd && (y % 2) == 1) {
        return 0;
    }

    let lr = max(upload_buffer[0].gdc_graphics_lines_per_row, 1u);
    let zoom = upload_buffer[0].gdc_graphics_zoom_display + 1;
    let effective_y = y / (lr * zoom);

    let pitch = upload_buffer[0].gdc_graphics_pitch;

    let byte_offset = compute_graphics_address(x, effective_y, pitch);
    let bit_index = 7 - (x % 8);

    let b = read_plane_bit(upload_buffer[0].graphics_b_plane, byte_offset, bit_index);
    let r = read_plane_bit(upload_buffer[0].graphics_r_plane, byte_offset, bit_index);
    let g = read_plane_bit(upload_buffer[0].graphics_g_plane, byte_offset, bit_index);
    let is_16_color = (upload_buffer[0].display_flags & 0x08) != 0;
    let e = is_16_color ? read_plane_bit(upload_buffer[0].graphics_e_plane, byte_offset, bit_index) : 0u;

    return b | (r << 1) | (g << 2) | (e << 3);
}

[ForceInline]
func compute_graphics_address(x: uint, y: uint, pitch: uint) -> uint {
    var scanline = y;
    for (uint i = 0; i < 4; i++) {
        let scroll = upload_buffer[0].gdc_graphics_scroll[i];
        let start_address = unpack_low_u16(scroll);
        let line_count = unpack_high_u16(scroll);
        if (line_count == 0) {
            continue;
        }
        if (scanline < line_count) {
            return ((start_address * 2) + (scanline * pitch * 2) + (x / 8)) & 0x7FFF;
        }
        scanline -= line_count;
    }
    return 0;
}

[ForceInline]
func read_plane_bit(plane: uint[8192], byte_offset: uint, bit_index: uint) -> uint {
    let word_index = byte_offset / 4;
    let byte_position = byte_offset % 4;
    let word = plane[word_index];
    let the_byte = (word >> (byte_position * 8)) & 0xFF;
    return (the_byte >> bit_index) & 1;
}

func compute_text_pixel(x: uint, y: uint, out pixel_on: bool, out color_index: uint) {
    let pitch = upload_buffer[0].gdc_text_pitch;
    let video_mode = upload_buffer[0].video_mode;
    let width40_mode = (video_mode & 0x04) != 0;
    let fontsel_8x16 = (video_mode & 0x08) != 0;
    let attr_semigraphics_mode = (video_mode & 0x01) != 0;
    let kanji_high_mask = upload_buffer[0].gdc_text_kanji_high_mask;

    // CRTC register extraction (all registers are 5-bit, 0-31).
    let crtc_pl_bl = upload_buffer[0].crtc_pl_bl;
    let pl = crtc_pl_bl & 0x1F;
    let bl = (crtc_pl_bl >> 16) & 0x1F;
    let crtc_cl_ssl = upload_buffer[0].crtc_cl_ssl;
    let cl = min(crtc_cl_ssl & 0x1F, 16u);
    let ssl = (crtc_cl_ssl >> 16) & 0x1F;
    let crtc_sur_sdr = upload_buffer[0].crtc_sur_sdr;
    let sur = crtc_sur_sdr & 0x1F;
    let _sdr = (crtc_sur_sdr >> 16) & 0x1F;

    // PL/BL: compute topline, lines, and scanlines per character row.
    var topline: int;
    var lines: int;
    let bl_plus_1 = int(bl) + 1;
    if (pl >= 16) {
        topline = int(pl) - 32;
        lines = bl_plus_1;
    } else {
        topline = int(pl);
        lines = bl_plus_1 - topline;
        if (lines <= 0) {
            lines += 32;
        }
    }
    let scanlines_per_row = uint(max(lines - topline, 1));

    // Walk text scroll partitions to find the active scroll area.
    // GDC line_count is already in scanlines (not character rows).
    var scanline = y;
    var scroll_start = 0u;
    for (uint i = 0; i < 4; i++) {
        let scroll = upload_buffer[0].gdc_scroll_start_line[i];
        let start_address = unpack_low_u16(scroll);
        let line_count = unpack_high_u16(scroll);
        if (line_count == 0) {
            continue;
        }
        if (scanline < line_count) {
            scroll_start = start_address;
            break;
        }
        scanline -= line_count;
    }

    // Map scanline to (row, glyph_y) accounting for SSL and SUR/SDR.
    var row: uint;
    var glyph_y: uint;

    if (sur == 0 && ssl > 0) {
        // SSL offsets the first character row (smooth scroll).
        let first_row_height = scanlines_per_row - ssl;
        if (scanline < first_row_height) {
            row = 0;
            glyph_y = scanline + ssl;
        } else {
            let adjusted = scanline - first_row_height;
            row = adjusted / scanlines_per_row + 1;
            glyph_y = adjusted % scanlines_per_row;
        }
    } else if (sur != 0) {
        // Upper fixed zone (32 - SUR rows) + scroll zone with SSL.
        let upper_rows = 32u - sur;
        let upper_zone_scanlines = upper_rows * scanlines_per_row;
        if (scanline < upper_zone_scanlines) {
            row = scanline / scanlines_per_row;
            glyph_y = scanline % scanlines_per_row;
        } else {
            let zone_scanline = scanline - upper_zone_scanlines;
            if (ssl > 0) {
                let first_scroll_row_height = scanlines_per_row - ssl;
                if (zone_scanline < first_scroll_row_height) {
                    row = upper_rows;
                    glyph_y = zone_scanline + ssl;
                } else {
                    let adjusted = zone_scanline - first_scroll_row_height;
                    row = upper_rows + adjusted / scanlines_per_row + 1;
                    glyph_y = adjusted % scanlines_per_row;
                }
            } else {
                row = upper_rows + zone_scanline / scanlines_per_row;
                glyph_y = zone_scanline % scanlines_per_row;
            }
        }
    } else {
        row = scanline / scanlines_per_row;
        glyph_y = scanline % scanlines_per_row;
    }

    // Compute font_line (signed) from PL-derived topline + glyph_y.
    // Font ROM lookup uses (font_line & 0x0F). Lines outside [0, CL) render blank.
    let font_line = topline + int(glyph_y);
    let font_rom_line = uint(font_line) & 0x0F;
    let font_visible = (font_line >= 0) && (font_line < int(cl));

    let glyph_x = width40_mode ? ((x / 2) % 8) : (x % 8);
    let memory_col = width40_mode ? ((x / 16) * 2) : (x / 8);

    let char_byte_offset = (scroll_start + row * pitch + memory_col) * 2;
    let attr_byte_offset = char_byte_offset + 0x2000;

    let char_low = read_text_vram_byte(char_byte_offset);
    let char_high = read_text_vram_byte(char_byte_offset + 1);
    let attr_byte = read_text_vram_byte(attr_byte_offset);

    let semigraphics = attr_semigraphics_mode && ((attr_byte & 0x10) != 0);
    let is_kanji = (char_high & kanji_high_mask) != 0;
    let character_step = width40_mode ? 2u : 1u;

    var previous_char_low = 0u;
    var previous_char_high = 0u;
    var previous_is_kanji = false;
    var previous_has_right_half = false;
    if (memory_col >= character_step) {
        let previous_byte_offset = (scroll_start + row * pitch + memory_col - character_step) * 2;
        previous_char_low = read_text_vram_byte(previous_byte_offset);
        previous_char_high = read_text_vram_byte(previous_byte_offset + 1);
        previous_is_kanji = (previous_char_high & kanji_high_mask) != 0;

        if (previous_is_kanji) {
            let previous_row_code = previous_char_low & 0x7F;
            let previous_is_trail_code = (previous_char_low & 0x80) != 0;
            let previous_is_gaiji = (previous_row_code == 0x56) || (previous_row_code == 0x57);
            previous_has_right_half =
                !previous_is_trail_code &&
                !previous_is_gaiji &&
                ((previous_row_code < 0x09) || (previous_row_code >= 0x0C));
        }
    }

    let is_right_half = previous_has_right_half;
    var on: bool;

    if (!font_visible) {
        // Outside CL range or negative font_line: render blank (background only).
        on = false;
    } else if (is_right_half || is_kanji) {
        // Kanji (double-byte): 16x16 glyph from the kanji font ROM.
        var kanji_low: uint;
        var kanji_high: uint;
        if (is_right_half) {
            kanji_low = previous_char_low;
            kanji_high = previous_char_high;
        } else {
            kanji_low = char_low;
            kanji_high = char_high;
        }

        let kanji_code = (kanji_high << 8) | kanji_low;
        var font_addr: uint = ((kanji_code & 0x7F7F) << 4) + font_rom_line;
        if (is_right_half) {
            font_addr += 0x800;
        }

        let font_byte = read_font_rom_byte(font_addr);
        on = ((font_byte >> (7u - glyph_x)) & 1u) != 0;
    } else if (fontsel_8x16) {
        // 8x16 mode: ANK16 at 0x80000, chargraph16 at 0x81000.
        var font_base: uint = 0x80000 + char_low * 16 + font_rom_line;
        if (semigraphics) {
            font_base += 0x1000;
        }
        let font_byte = read_font_rom_byte(font_base);
        on = ((font_byte >> (7u - glyph_x)) & 1u) != 0;
    } else {
        // 6x8 mode: ANK8 at 0x82000, chargraph8 at 0x82000+8.
        // Line number is halved (display rows -> font rows).
        let half_line = font_rom_line / 2;
        var font_base: uint = 0x82000 + char_low * 16 + half_line;
        if (semigraphics) {
            font_base += 8;
        }
        let font_byte = read_font_rom_byte(font_base);
        on = ((font_byte >> (7u - glyph_x)) & 1u) != 0;
    }

    let is_secret    = (attr_byte & 0x01) == 0;
    let is_blink     = (attr_byte & 0x02) != 0;
    let is_reverse   = (attr_byte & 0x04) != 0;
    let is_underline = (attr_byte & 0x08) != 0;
    let is_vline     = !attr_semigraphics_mode && ((attr_byte & 0x10) != 0);
    color_index      = (attr_byte >> 5) & 0x07;

    if (is_secret) {
        on = false;
    }

    let blink_visible = (upload_buffer[0].display_flags & 0x02) != 0;
    if (is_blink && !blink_visible) {
        on = false;
    }

    if (is_reverse) {
        on = !on;
    }

    // Text cursor overlay: XOR between cursor_top and cursor_bottom scanlines.
    // Applied before underline/vline so those overlays cannot be punched through.
    // For kanji characters, the cursor spans both the left and right halves.
    let text_cursor = upload_buffer[0].text_cursor;
    let cursor_visible = (text_cursor & 0x80000000) != 0;
    if (cursor_visible) {
        let cursor_addr = text_cursor & 0x3FFFF;
        let char_addr = scroll_start + row * pitch + memory_col;
        let cursor_hit = (char_addr == cursor_addr)
            || (is_right_half && memory_col >= character_step
                && (scroll_start + row * pitch + memory_col - character_step) == cursor_addr);
        if (cursor_hit) {
            let cursor_top = int((text_cursor >> 18) & 0x1F);
            let cursor_bottom = int((text_cursor >> 23) & 0x1F);
            if (font_line >= cursor_top && font_line <= cursor_bottom) {
                on = !on;
            }
        }
    }

    // Underline: draws pixels 4-7 of the underlined character, plus pixels 0-3
    // of the next character (bleed). Uses the receiving character's color.
    let at_underline_line = (font_line + 1) == lines;
    if (at_underline_line) {
        if (is_underline && glyph_x >= 4u) {
            on = true;
        }
        if (glyph_x < 4u && memory_col >= character_step) {
            let prev_attr_offset = (scroll_start + row * pitch + memory_col - character_step) * 2u + 0x2000u;
            let prev_attr = read_text_vram_byte(prev_attr_offset);
            if ((prev_attr & 0x08) != 0) {
                on = true;
            }
        }
    }

    // Vertical line at pixel 4 (not pixel 7).
    if (is_vline && glyph_x == 4u) {
        on = true;
    }

    pixel_on = on;
}

[ForceInline]
func unpack_low_u16(uint packed_value) -> uint {
    return packed_value & 0xFFFF;
}

[ForceInline]
func unpack_high_u16(uint packed_value) -> uint {
    return (packed_value >> 16) & 0xFFFF;
}

[ForceInline]
func read_text_vram_byte(uint byte_offset) -> uint {
    let word_index = byte_offset / 4;
    let byte_position = byte_offset % 4;
    let word = upload_buffer[0].text_vram_words[word_index];
    return (word >> (byte_position * 8)) & 0xFF;
}

[ForceInline]
func read_font_rom_byte(uint byte_offset) -> uint {
    let word_index = byte_offset / 4;
    let byte_position = byte_offset % 4;
    let word = font_rom_buffer[0].font_rom_words[word_index];
    return (word >> (byte_position * 8)) & 0xFF;
}
