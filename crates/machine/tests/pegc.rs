use std::{
    path::{Path, PathBuf},
    process::Command,
};

use common::{Bus, MachineModel, PegcSnapshotUpload};
use machine::{NoTracing, Pc9801Bus, Pc9821As};
use spirv::ComposeShader;

const VRAM_B: u32 = 0xA8000;
const VRAM_R: u32 = 0xB0000;
const VRAM_G: u32 = 0xB8000;
const PEGC_VRAM_A: u32 = VRAM_B;
const PEGC_MMIO_BANK_A8: u32 = 0xE0004;
const BANK_SIZE: u32 = 0x8000;
const BYTES_PER_LINE: u32 = 80;
const GRID_ROWS: u32 = 16;
const GRID_COLS: u32 = 16;
const CELL_WIDTH: u32 = 40;
const LINES_PER_ROW_400: u32 = 25;
const CYCLES_PER_STEP: u64 = 200_000;
const STEPS_PER_PATTERN: usize = 400;

fn create_pegc_bus() -> Pc9801Bus<NoTracing> {
    let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9821AS, 48000);
    bus.io_write_byte(0x6A, 0x01); // analog mode (mode2 bit 0)
    bus.io_write_byte(0x6A, 0x07); // mode change permission (mode2 bit 3)
    bus.set_graphics_extension_enabled(true);
    bus
}

fn enable_pegc_packed_pixel(bus: &mut Pc9801Bus<NoTracing>) {
    bus.io_write_byte(0x6A, 0x21); // PEGC 256-color enable
    bus.io_write_byte(0x6A, 0x63); // packed pixel mode
    bus.io_write_byte(0x6A, 0x68); // two-screen mode (640x400)
}

fn hsv_to_grb(i: u8) -> (u8, u8, u8) {
    let h6 = i as u32 * 6;
    let sector = h6 / 256;
    let f = (h6 % 256) as u8;
    let t = f;
    let q = 255 - f;
    match sector {
        0 => (t, 255, 0),
        1 => (255, q, 0),
        2 => (255, 0, t),
        3 => (q, 0, 255),
        4 => (0, t, 255),
        5 => (0, 255, q),
        _ => unreachable!(),
    }
}

fn program_hsv_palette(bus: &mut Pc9801Bus<NoTracing>) {
    for i in 0..=255u8 {
        let (green, red, blue) = hsv_to_grb(i);
        bus.io_write_byte(0xA8, i);
        bus.io_write_byte(0xAA, green);
        bus.io_write_byte(0xAC, red);
        bus.io_write_byte(0xAE, blue);
    }
}

fn set_bank_a8(bus: &mut Pc9801Bus<NoTracing>, bank: u8) {
    bus.write_byte(PEGC_MMIO_BANK_A8, bank);
}

fn fill_pegc_grid(bus: &mut Pc9801Bus<NoTracing>) {
    let mut current_bank: u8 = 0;
    let mut bank_offset: u32 = 0;
    set_bank_a8(bus, 0);

    for row in 0..GRID_ROWS {
        for _line in 0..LINES_PER_ROW_400 {
            for col in 0..GRID_COLS {
                let palette_index = (row * 16 + col) as u8;
                for _pixel in 0..CELL_WIDTH {
                    if bank_offset >= BANK_SIZE {
                        current_bank += 1;
                        set_bank_a8(bus, current_bank);
                        bank_offset = 0;
                    }
                    bus.write_byte(PEGC_VRAM_A + bank_offset, palette_index);
                    bank_offset += 1;
                }
            }
        }
    }
}

#[test]
fn pegc_hsv_palette_all_distinct() {
    let mut colors = std::collections::HashSet::new();
    for i in 0..=255u8 {
        let grb = hsv_to_grb(i);
        assert!(
            colors.insert(grb),
            "palette index {i} produces duplicate color ({}, {}, {})",
            grb.0,
            grb.1,
            grb.2
        );
    }
}

#[test]
fn pegc_hsv_palette_boundary_values() {
    assert_eq!(hsv_to_grb(0), (0, 255, 0)); // pure red
    assert_eq!(hsv_to_grb(43), (255, 253, 0)); // near yellow (sector 1 start)
    assert_eq!(hsv_to_grb(86), (255, 0, 4)); // near green (sector 2 start)
    assert_eq!(hsv_to_grb(128), (255, 0, 255)); // cyan
    assert_eq!(hsv_to_grb(171), (0, 2, 255)); // near blue (sector 4 start)
    assert_eq!(hsv_to_grb(214), (0, 255, 251)); // near magenta (sector 5 start)
    assert_eq!(hsv_to_grb(255), (0, 255, 5)); // near red (wrapping)
}

#[test]
fn pegc_palette_program_and_readback() {
    let mut bus = create_pegc_bus();
    enable_pegc_packed_pixel(&mut bus);
    program_hsv_palette(&mut bus);

    for i in 0..=255u8 {
        let (expected_green, expected_red, expected_blue) = hsv_to_grb(i);

        bus.io_write_byte(0xA8, i);
        let green = bus.io_read_byte(0xAA);
        let red = bus.io_read_byte(0xAC);
        let blue = bus.io_read_byte(0xAE);

        assert_eq!(
            green, expected_green,
            "palette[{i}] green: expected {expected_green}, got {green}"
        );
        assert_eq!(
            red, expected_red,
            "palette[{i}] red: expected {expected_red}, got {red}"
        );
        assert_eq!(
            blue, expected_blue,
            "palette[{i}] blue: expected {expected_blue}, got {blue}"
        );
    }
}

#[test]
fn pegc_packed_pixel_grid_all_pixels() {
    let mut bus = create_pegc_bus();
    enable_pegc_packed_pixel(&mut bus);
    program_hsv_palette(&mut bus);
    fill_pegc_grid(&mut bus);

    let mut current_bank: u8 = 0;
    let mut bank_offset: u32 = 0;
    set_bank_a8(&mut bus, 0);

    for row in 0..GRID_ROWS {
        for line in 0..LINES_PER_ROW_400 {
            for col in 0..GRID_COLS {
                let expected_index = (row * 16 + col) as u8;
                for pixel in 0..CELL_WIDTH {
                    if bank_offset >= BANK_SIZE {
                        current_bank += 1;
                        set_bank_a8(&mut bus, current_bank);
                        bank_offset = 0;
                    }
                    let actual = bus.read_byte(PEGC_VRAM_A + bank_offset);
                    assert_eq!(
                        actual, expected_index,
                        "mismatch at row={row}, line={line}, col={col}, pixel={pixel}: \
                         expected {expected_index}, got {actual}"
                    );
                    bank_offset += 1;
                }
            }
        }
    }
}

#[test]
fn pegc_bank_switching_across_boundary() {
    let mut bus = create_pegc_bus();
    enable_pegc_packed_pixel(&mut bus);

    set_bank_a8(&mut bus, 0);
    bus.write_byte(PEGC_VRAM_A + BANK_SIZE - 1, 0xAA);

    set_bank_a8(&mut bus, 1);
    bus.write_byte(PEGC_VRAM_A, 0xBB);

    set_bank_a8(&mut bus, 0);
    assert_eq!(bus.read_byte(PEGC_VRAM_A + BANK_SIZE - 1), 0xAA);

    set_bank_a8(&mut bus, 1);
    assert_eq!(bus.read_byte(PEGC_VRAM_A), 0xBB);
}

fn debug_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("utils")
        .join("debug")
}

fn ensure_debug_pegc_firmware_exists() {
    let dir = debug_dir();
    let rom_path = dir.join("debug_pegc.rom");
    if rom_path.exists() {
        return;
    }
    let status = Command::new("make")
        .arg("-C")
        .arg(&dir)
        .arg("debug_pegc.rom")
        .status()
        .expect("Failed to run make for debug_pegc.rom");
    assert!(status.success(), "make debug_pegc.rom failed");
}

fn run_steps(machine: &mut Pc9821As, steps: usize) {
    for _ in 0..steps {
        machine.run_for(CYCLES_PER_STEP);
    }
}

fn send_enter(bus: &mut Pc9801Bus) {
    bus.push_keyboard_scancode(0x1C);
    bus.push_keyboard_scancode(0x9C);
}

fn render_snapshot(shader: &ComposeShader, bus: &mut Pc9801Bus) -> spirv::ComposeOutput {
    bus.capture_vsync_snapshot();
    let display = bus.vsync_snapshot();
    let default_pegc = PegcSnapshotUpload::default();
    let pegc = bus.pegc_vsync_snapshot().unwrap_or(&default_pegc);
    shader
        .execute(display, &[], pegc)
        .expect("shader execution failed")
}

fn get_pixel(output: &spirv::ComposeOutput, x: u32, y: u32) -> [u8; 4] {
    let offset = (y * output.width + x) as usize * 4;
    [
        output.framebuffer[offset],
        output.framebuffer[offset + 1],
        output.framebuffer[offset + 2],
        output.framebuffer[offset + 3],
    ]
}

fn expected_16color_rgba(
    snapshot: &common::DisplaySnapshotUpload,
    palette_index: usize,
) -> [u8; 4] {
    let packed = snapshot.palette_rgba[palette_index];
    let r = (packed & 0xFF) as u8;
    let g = ((packed >> 8) & 0xFF) as u8;
    let b = ((packed >> 16) & 0xFF) as u8;
    [r, g, b, 255]
}

fn expected_256color_rgba(pegc: &PegcSnapshotUpload, palette_index: usize) -> [u8; 4] {
    let packed = pegc.palette_rgba_256[palette_index];
    let r = (packed & 0xFF) as u8;
    let g = ((packed >> 8) & 0xFF) as u8;
    let b = ((packed >> 16) & 0xFF) as u8;
    [r, g, b, 255]
}

fn check_shader_16color_quadrant(
    output: &spirv::ComposeOutput,
    snapshot: &common::DisplaySnapshotUpload,
    lines: [u32; 2],
    pixels: [u32; 2],
    palette_index: usize,
    label: &str,
) {
    let [start_line, end_line] = lines;
    let [start_pixel, end_pixel] = pixels;
    let hide_odd = (snapshot.display_flags & 0x04) != 0;
    let expected = expected_16color_rgba(snapshot, palette_index);
    let black = expected_16color_rgba(snapshot, 0);
    for y in start_line..end_line {
        let line_expected = if hide_odd && (y % 2) == 1 {
            black
        } else {
            expected
        };
        for x in start_pixel..end_pixel {
            let actual = get_pixel(output, x, y);
            assert_eq!(
                actual, line_expected,
                "{label} at ({x}, {y}): expected palette {palette_index} {line_expected:?}, got {actual:?}"
            );
        }
    }
}

fn check_shader_256color_grid(
    output: &spirv::ComposeOutput,
    pegc: &PegcSnapshotUpload,
    lines_per_row: u32,
    label: &str,
) {
    for row in 0..GRID_ROWS {
        let y_start = row * lines_per_row;
        let y_end = y_start + lines_per_row;
        for col in 0..GRID_COLS {
            let x_start = col * CELL_WIDTH;
            let x_end = x_start + CELL_WIDTH;
            let palette_index = (row * 16 + col) as usize;
            let expected = expected_256color_rgba(pegc, palette_index);

            for y in y_start..y_end {
                for x in x_start..x_end {
                    let actual = get_pixel(output, x, y);
                    assert_eq!(
                        actual, expected,
                        "{label} palette {palette_index} at ({x}, {y}): expected {expected:?}, got {actual:?}"
                    );
                }
            }
        }
    }
}

fn check_plane_quadrant(
    bus: &Pc9801Bus,
    plane_base: u32,
    lines: [u32; 2],
    cols: [u32; 2],
    expected: u8,
    label: &str,
) {
    let [start_line, end_line] = lines;
    let [start_col, end_col] = cols;
    for line in start_line..end_line {
        for col in start_col..end_col {
            let offset = line * BYTES_PER_LINE + col;
            let actual = bus.read_byte_direct(plane_base + offset);
            assert_eq!(
                actual, expected,
                "{label} mismatch at line {line}, col {col}: expected 0x{expected:02X}, got 0x{actual:02X}"
            );
        }
    }
}

#[test]
fn pegc_firmware_all_patterns() {
    ensure_debug_pegc_firmware_exists();

    let dir = debug_dir();
    let bios_path = dir.join("debug_pegc.rom");

    let bios_data = std::fs::read(&bios_path)
        .unwrap_or_else(|error| panic!("Failed to read {}: {error}", bios_path.display()));

    let mut bus = Pc9801Bus::new(MachineModel::PC9821AS, 48000);
    bus.load_bios_rom(&bios_data);
    bus.set_graphics_extension_enabled(true);
    // TODO: call bus.set_gdc_5mhz_capable() once the GDC clock branch is merged.

    let mut machine = Pc9821As::new(cpu::I386::new(), bus);
    let shader = ComposeShader::from_embedded().expect("failed to load compose shader");

    // Pattern 0: 16-color analog quadrant pattern
    // TL=1 (blue), TR=2 (red), BL=4 (green), BR=7 (white)
    run_steps(&mut machine, STEPS_PER_PATTERN);

    // Verify VRAM: TL = B-plane only
    check_plane_quadrant(&machine.bus, VRAM_B, [0, 200], [0, 40], 0xFF, "P0 TL B");
    check_plane_quadrant(&machine.bus, VRAM_R, [0, 200], [0, 40], 0x00, "P0 TL R");
    check_plane_quadrant(&machine.bus, VRAM_G, [0, 200], [0, 40], 0x00, "P0 TL G");
    // TR = R-plane only
    check_plane_quadrant(&machine.bus, VRAM_B, [0, 200], [40, 80], 0x00, "P0 TR B");
    check_plane_quadrant(&machine.bus, VRAM_R, [0, 200], [40, 80], 0xFF, "P0 TR R");
    check_plane_quadrant(&machine.bus, VRAM_G, [0, 200], [40, 80], 0x00, "P0 TR G");
    // BL = G-plane only
    check_plane_quadrant(&machine.bus, VRAM_B, [200, 400], [0, 40], 0x00, "P0 BL B");
    check_plane_quadrant(&machine.bus, VRAM_R, [200, 400], [0, 40], 0x00, "P0 BL R");
    check_plane_quadrant(&machine.bus, VRAM_G, [200, 400], [0, 40], 0xFF, "P0 BL G");
    // BR = B+R+G planes
    check_plane_quadrant(&machine.bus, VRAM_B, [200, 400], [40, 80], 0xFF, "P0 BR B");
    check_plane_quadrant(&machine.bus, VRAM_R, [200, 400], [40, 80], 0xFF, "P0 BR R");
    check_plane_quadrant(&machine.bus, VRAM_G, [200, 400], [40, 80], 0xFF, "P0 BR G");

    // Shader verification: 16-color quadrants
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_16color_quadrant(&output, snapshot, [0, 200], [0, 320], 1, "P0 TL");
    check_shader_16color_quadrant(&output, snapshot, [0, 200], [320, 640], 2, "P0 TR");
    check_shader_16color_quadrant(&output, snapshot, [200, 400], [0, 320], 4, "P0 BL");
    check_shader_16color_quadrant(&output, snapshot, [200, 400], [320, 640], 7, "P0 BR");

    // Pattern 1: PEGC 256-color 640x400 two-screen mode (16x16 grid, 25 lines/row)
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    let output = render_snapshot(&shader, &mut machine.bus);
    let pegc = machine
        .bus
        .pegc_vsync_snapshot()
        .expect("PEGC snapshot should exist for pattern 1");
    assert_eq!(output.width, 640);
    assert_eq!(output.height, 400);
    check_shader_256color_grid(&output, pegc, 25, "P1");

    // Pattern 2: PEGC 256-color 640x480 one-screen mode (16x16 grid, 30 lines/row)
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    let output = render_snapshot(&shader, &mut machine.bus);
    let pegc = machine
        .bus
        .pegc_vsync_snapshot()
        .expect("PEGC snapshot should exist for pattern 2");
    assert_eq!(output.width, 640);
    assert_eq!(output.height, 480);
    check_shader_256color_grid(&output, pegc, 30, "P2");
}
