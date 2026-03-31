use std::path::{Path, PathBuf};

use common::{Bus, MachineModel, PegcSnapshotUpload};
use machine::{NoTracing, Pc9801Bus, Pc9801Vm};
use spirv::ComposeShader;

const CYCLES_PER_STEP: u64 = 200_000;
const STEPS_PER_PATTERN: usize = 200;

const VRAM_B: u32 = 0xA8000;
const VRAM_R: u32 = 0xB0000;
const VRAM_G: u32 = 0xB8000;
const VRAM_E: u32 = 0xE0000;

const BYTES_PER_LINE: u32 = 80;
const SCREEN_HEIGHT: u32 = 400;

fn debug_firmware_directory() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("utils/debug/")
}

fn run_steps(machine: &mut Pc9801Vm, steps: usize) {
    for _ in 0..steps {
        machine.run_for(CYCLES_PER_STEP);
    }
}

fn send_enter(bus: &mut Pc9801Bus) {
    bus.push_keyboard_scancode(0x1C); // Enter make
    bus.push_keyboard_scancode(0x9C); // Enter break
}

fn read_vram(bus: &Pc9801Bus, plane_base: u32, offset: u32) -> u8 {
    bus.read_byte_direct(plane_base + offset)
}

fn vram_offset(line: u32, col_byte: u32) -> u32 {
    line * BYTES_PER_LINE + col_byte
}

fn check_plane_full(bus: &Pc9801Bus, plane_base: u32, expected: u8, plane_name: &str) {
    for line in 0..SCREEN_HEIGHT {
        for col in 0..BYTES_PER_LINE {
            let offset = vram_offset(line, col);
            let actual = read_vram(bus, plane_base, offset);
            assert_eq!(
                actual, expected,
                "{plane_name} mismatch at line {line}, col byte {col}: expected 0x{expected:02X}, got 0x{actual:02X}"
            );
        }
    }
}

fn render_snapshot(shader: &ComposeShader, bus: &mut Pc9801Bus) -> spirv::ComposeOutput {
    bus.capture_vsync_snapshot();
    let display = bus.vsync_snapshot();
    let pegc = PegcSnapshotUpload::default();
    shader
        .execute(display, &[], &pegc)
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

fn expected_palette_rgba(
    snapshot: &common::DisplaySnapshotUpload,
    palette_index: usize,
) -> [u8; 4] {
    let packed = snapshot.palette_rgba[palette_index];
    let r = (packed & 0xFF) as u8;
    let g = ((packed >> 8) & 0xFF) as u8;
    let b = ((packed >> 16) & 0xFF) as u8;
    [r, g, b, 255]
}

fn check_shader_solid_band(
    output: &spirv::ComposeOutput,
    snapshot: &common::DisplaySnapshotUpload,
    start_line: u32,
    end_line: u32,
    palette_index: usize,
    label: &str,
) {
    let hide_odd = (snapshot.display_flags & 0x04) != 0;
    let expected = expected_palette_rgba(snapshot, palette_index);
    let black = expected_palette_rgba(snapshot, 0);
    for y in start_line..end_line {
        let line_expected = if hide_odd && (y % 2) == 1 {
            black
        } else {
            expected
        };
        for x in 0..640 {
            let actual = get_pixel(output, x, y);
            assert_eq!(
                actual, line_expected,
                "{label} at ({x}, {y}): expected palette {palette_index} {line_expected:?}, got {actual:?}"
            );
        }
    }
}

fn check_shader_tile_band(
    output: &spirv::ComposeOutput,
    snapshot: &common::DisplaySnapshotUpload,
    start_line: u32,
    end_line: u32,
    tiles: [u8; 4],
    label: &str,
) {
    let [tile_b, tile_r, tile_g, tile_e] = tiles;
    let hide_odd = (snapshot.display_flags & 0x04) != 0;
    let black = expected_palette_rgba(snapshot, 0);
    for y in start_line..end_line {
        for x in 0..640u32 {
            if hide_odd && (y % 2) == 1 {
                let actual = get_pixel(output, x, y);
                assert_eq!(
                    actual, black,
                    "{label} at ({x}, {y}): expected black (hide_odd), got {actual:?}"
                );
                continue;
            }
            let bit = 7 - (x % 8);
            let b = (tile_b >> bit) & 1;
            let r = (tile_r >> bit) & 1;
            let g = (tile_g >> bit) & 1;
            let e = (tile_e >> bit) & 1;
            let palette_index = (b | (r << 1) | (g << 2) | (e << 3)) as usize;
            let expected = expected_palette_rgba(snapshot, palette_index);
            let actual = get_pixel(output, x, y);
            assert_eq!(
                actual, expected,
                "{label} at ({x}, {y}): expected palette {palette_index} {expected:?}, got {actual:?}"
            );
        }
    }
}

fn check_plane_lines(
    bus: &Pc9801Bus,
    plane_base: u32,
    start_line: u32,
    end_line: u32,
    expected: u8,
    plane_name: &str,
) {
    for line in start_line..end_line {
        for col in 0..BYTES_PER_LINE {
            let offset = vram_offset(line, col);
            let actual = read_vram(bus, plane_base, offset);
            assert_eq!(
                actual, expected,
                "{plane_name} mismatch at line {line}, col byte {col}: expected 0x{expected:02X}, got 0x{actual:02X}"
            );
        }
    }
}

#[test]
fn grcg_firmware_all_patterns() {
    let debug_firmware_directory = debug_firmware_directory();
    let bios_path = debug_firmware_directory.join("debug_grcg.rom");

    let bios_data = std::fs::read(&bios_path)
        .unwrap_or_else(|error| panic!("Failed to read {}: {error}", bios_path.display()));

    let mut bus = Pc9801Bus::new(MachineModel::PC9801VM, 48000);
    bus.load_bios_rom(&bios_data);

    let mut machine = Pc9801Vm::new(cpu::V30::new(), bus);

    // Pattern 0: Solid White (Direct Write) - all 4 planes = 0xFF
    run_steps(&mut machine, STEPS_PER_PATTERN);

    check_plane_full(&machine.bus, VRAM_B, 0xFF, "P0 B-plane");
    check_plane_full(&machine.bus, VRAM_R, 0xFF, "P0 R-plane");
    check_plane_full(&machine.bus, VRAM_G, 0xFF, "P0 G-plane");
    check_plane_full(&machine.bus, VRAM_E, 0xFF, "P0 E-plane");

    let shader = ComposeShader::from_embedded().expect("failed to load compose shader");

    // Shader verification: pattern 0 - all planes 0xFF = palette index 15
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_solid_band(&output, snapshot, 0, 400, 15, "P0");

    // Pattern 1: Individual Planes (Direct Write)
    // Lines 0-99: B only, 100-199: R only, 200-299: G only, 300-399: E only
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    check_plane_lines(&machine.bus, VRAM_B, 0, 100, 0xFF, "P1 B lines 0-99");
    check_plane_lines(&machine.bus, VRAM_B, 100, 400, 0x00, "P1 B lines 100-399");
    check_plane_lines(&machine.bus, VRAM_R, 0, 100, 0x00, "P1 R lines 0-99");
    check_plane_lines(&machine.bus, VRAM_R, 100, 200, 0xFF, "P1 R lines 100-199");
    check_plane_lines(&machine.bus, VRAM_R, 200, 400, 0x00, "P1 R lines 200-399");
    check_plane_lines(&machine.bus, VRAM_G, 0, 200, 0x00, "P1 G lines 0-199");
    check_plane_lines(&machine.bus, VRAM_G, 200, 300, 0xFF, "P1 G lines 200-299");
    check_plane_lines(&machine.bus, VRAM_G, 300, 400, 0x00, "P1 G lines 300-399");
    check_plane_lines(&machine.bus, VRAM_E, 0, 300, 0x00, "P1 E lines 0-299");
    check_plane_lines(&machine.bus, VRAM_E, 300, 400, 0xFF, "P1 E lines 300-399");

    // Shader verification: pattern 1 - individual planes in bands
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_solid_band(&output, snapshot, 0, 100, 1, "P1 B-only");
    check_shader_solid_band(&output, snapshot, 100, 200, 2, "P1 R-only");
    check_shader_solid_band(&output, snapshot, 200, 300, 4, "P1 G-only");
    check_shader_solid_band(&output, snapshot, 300, 400, 8, "P1 E-only");

    // Pattern 2: TDW Full Fill - all planes
    // Mode 0x80, tiles: B=0xAA, R=0x55, G=0xF0, E=0x0F
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    check_plane_full(&machine.bus, VRAM_B, 0xAA, "P2 B-plane");
    check_plane_full(&machine.bus, VRAM_R, 0x55, "P2 R-plane");
    check_plane_full(&machine.bus, VRAM_G, 0xF0, "P2 G-plane");
    check_plane_full(&machine.bus, VRAM_E, 0x0F, "P2 E-plane");

    // Shader verification: pattern 2 - TDW tile fill B=0xAA, R=0x55, G=0xF0, E=0x0F
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_tile_band(&output, snapshot, 0, 400, [0xAA, 0x55, 0xF0, 0x0F], "P2");

    // Pattern 3: TDW Selective Plane Enable
    // Pre-fill=0xFF. Mode 0x8A (TDW, R+E disabled). Tiles: B=0x00, G=0x00.
    // Expected: B=0x00 (tile), R=0xFF (unchanged), G=0x00 (tile), E=0xFF (unchanged)
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    check_plane_full(&machine.bus, VRAM_B, 0x00, "P3 B-plane");
    check_plane_full(&machine.bus, VRAM_R, 0xFF, "P3 R-plane (unchanged)");
    check_plane_full(&machine.bus, VRAM_G, 0x00, "P3 G-plane");
    check_plane_full(&machine.bus, VRAM_E, 0xFF, "P3 E-plane (unchanged)");

    // Shader verification: pattern 3 - B=0x00, R=0xFF, G=0x00, E=0xFF = palette index 10
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_solid_band(&output, snapshot, 0, 400, 10, "P3");

    // Pattern 4: TCR (Tile Compare Read)
    // Band 0 (lines 0-99):   B=0xFF, R=0x00, G=0xFF, E=0x00
    // Band 1 (lines 100-199): all 0xFF
    // Band 2 (lines 200-299): all 0x00
    // Band 3 (lines 300-399): B=0xAA, R=0x55, G=0xAA, E=0x55
    // Tiles: B=0xFF, R=0x00, G=0xFF, E=0x00
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    // Verify VRAM bands
    check_plane_lines(&machine.bus, VRAM_B, 0, 100, 0xFF, "P4 B lines 0-99");
    check_plane_lines(&machine.bus, VRAM_R, 0, 100, 0x00, "P4 R lines 0-99");
    check_plane_lines(&machine.bus, VRAM_G, 0, 100, 0xFF, "P4 G lines 0-99");
    check_plane_lines(&machine.bus, VRAM_E, 0, 100, 0x00, "P4 E lines 0-99");

    check_plane_lines(&machine.bus, VRAM_B, 100, 200, 0xFF, "P4 B lines 100-199");
    check_plane_lines(&machine.bus, VRAM_R, 100, 200, 0xFF, "P4 R lines 100-199");
    check_plane_lines(&machine.bus, VRAM_G, 100, 200, 0xFF, "P4 G lines 100-199");
    check_plane_lines(&machine.bus, VRAM_E, 100, 200, 0xFF, "P4 E lines 100-199");

    check_plane_lines(&machine.bus, VRAM_B, 200, 300, 0x00, "P4 B lines 200-299");
    check_plane_lines(&machine.bus, VRAM_R, 200, 300, 0x00, "P4 R lines 200-299");
    check_plane_lines(&machine.bus, VRAM_G, 200, 300, 0x00, "P4 G lines 200-299");
    check_plane_lines(&machine.bus, VRAM_E, 200, 300, 0x00, "P4 E lines 200-299");

    check_plane_lines(&machine.bus, VRAM_B, 300, 400, 0xAA, "P4 B lines 300-399");
    check_plane_lines(&machine.bus, VRAM_R, 300, 400, 0x55, "P4 R lines 300-399");
    check_plane_lines(&machine.bus, VRAM_G, 300, 400, 0xAA, "P4 G lines 300-399");
    check_plane_lines(&machine.bus, VRAM_E, 300, 400, 0x55, "P4 E lines 300-399");

    // TCR results stored in RAM at physical addresses 0x500-0x503
    // Line 50 (band 0): all match tiles -> 0xFF
    // Line 150 (band 1): R,E mismatch -> 0x00
    // Line 250 (band 2): B,G mismatch -> 0x00
    // Line 350 (band 3): partial match -> 0xAA
    let tcr0 = machine.bus.read_byte_direct(0x0500);
    let tcr1 = machine.bus.read_byte_direct(0x0501);
    let tcr2 = machine.bus.read_byte_direct(0x0502);
    let tcr3 = machine.bus.read_byte_direct(0x0503);

    assert_eq!(
        tcr0, 0xFF,
        "P4 TCR line 50: expected 0xFF (all match), got 0x{tcr0:02X}"
    );
    assert_eq!(
        tcr1, 0x00,
        "P4 TCR line 150: expected 0x00 (R,E mismatch), got 0x{tcr1:02X}"
    );
    assert_eq!(
        tcr2, 0x00,
        "P4 TCR line 250: expected 0x00 (B,G mismatch), got 0x{tcr2:02X}"
    );
    assert_eq!(
        tcr3, 0xAA,
        "P4 TCR line 350: expected 0xAA (partial match), got 0x{tcr3:02X}"
    );

    // Shader verification: pattern 4 - four VRAM bands
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    // Band 0: B=0xFF, R=0x00, G=0xFF, E=0x00 = solid index 5
    check_shader_solid_band(&output, snapshot, 0, 100, 5, "P4 band 0");
    // Band 1: all 0xFF = solid index 15
    check_shader_solid_band(&output, snapshot, 100, 200, 15, "P4 band 1");
    // Band 2: all 0x00 = solid index 0
    check_shader_solid_band(&output, snapshot, 200, 300, 0, "P4 band 2");
    // Band 3: B=0xAA, R=0x55, G=0xAA, E=0x55
    check_shader_tile_band(
        &output,
        snapshot,
        300,
        400,
        [0xAA, 0x55, 0xAA, 0x55],
        "P4 band 3",
    );

    // Pattern 5: RMW Mode - all planes
    // Pre-fill=0x55. Mode 0xC0. Tiles: B=0xFF, R=0x00, G=0xAA, E=0x55. CPU=0xF0.
    // new = (0xF0 & tile) | (0x0F & 0x55)
    //   B: 0xF0 | 0x05 = 0xF5
    //   R: 0x00 | 0x05 = 0x05
    //   G: 0xA0 | 0x05 = 0xA5
    //   E: 0x50 | 0x05 = 0x55
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    check_plane_full(&machine.bus, VRAM_B, 0xF5, "P5 B-plane");
    check_plane_full(&machine.bus, VRAM_R, 0x05, "P5 R-plane");
    check_plane_full(&machine.bus, VRAM_G, 0xA5, "P5 G-plane");
    check_plane_full(&machine.bus, VRAM_E, 0x55, "P5 E-plane");

    // Shader verification: pattern 5 - RMW result B=0xF5, R=0x05, G=0xA5, E=0x55
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_tile_band(&output, snapshot, 0, 400, [0xF5, 0x05, 0xA5, 0x55], "P5");

    // Pattern 6: RMW Selective Plane Enable
    // Pre-fill=0xAA. Mode 0xCC (RMW, G+E disabled). Tiles: B=0xFF, R=0xFF. CPU=0xFF.
    // Expected: B=0xFF, R=0xFF, G=0xAA (unchanged), E=0xAA (unchanged)
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    check_plane_full(&machine.bus, VRAM_B, 0xFF, "P6 B-plane");
    check_plane_full(&machine.bus, VRAM_R, 0xFF, "P6 R-plane");
    check_plane_full(&machine.bus, VRAM_G, 0xAA, "P6 G-plane (unchanged)");
    check_plane_full(&machine.bus, VRAM_E, 0xAA, "P6 E-plane (unchanged)");

    // Shader verification: pattern 6 - B=0xFF, R=0xFF, G=0xAA, E=0xAA
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    check_shader_tile_band(&output, snapshot, 0, 400, [0xFF, 0xFF, 0xAA, 0xAA], "P6");

    // Pattern 7: Word-Width GRCG Operations
    // Top (lines 0-199): TDW, tiles B=0x33, R=0xCC, G=0x55, E=0xAA
    // Bottom (lines 200-399): RMW, pre-fill=0xFF, cpu=0xAA
    //   new = (0xAA & tile) | (0x55 & 0xFF):
    //     B: 0xA0 | 0x55 = 0xF5
    //     R: 0x0A | 0x55 = 0x5F
    //     G: 0xAA | 0x55 = 0xFF
    //     E: 0x00 | 0x55 = 0x55
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    // Top half: TDW tile values
    check_plane_lines(&machine.bus, VRAM_B, 0, 200, 0x33, "P7 B lines 0-199");
    check_plane_lines(&machine.bus, VRAM_R, 0, 200, 0xCC, "P7 R lines 0-199");
    check_plane_lines(&machine.bus, VRAM_G, 0, 200, 0x55, "P7 G lines 0-199");
    check_plane_lines(&machine.bus, VRAM_E, 0, 200, 0xAA, "P7 E lines 0-199");

    // Bottom half: RMW results
    check_plane_lines(&machine.bus, VRAM_B, 200, 400, 0xF5, "P7 B lines 200-399");
    check_plane_lines(&machine.bus, VRAM_R, 200, 400, 0x5F, "P7 R lines 200-399");
    check_plane_lines(&machine.bus, VRAM_G, 200, 400, 0xFF, "P7 G lines 200-399");
    check_plane_lines(&machine.bus, VRAM_E, 200, 400, 0x55, "P7 E lines 200-399");

    // Shader verification: pattern 7 - two halves
    let output = render_snapshot(&shader, &mut machine.bus);
    let snapshot = machine.bus.vsync_snapshot();
    // Top: TDW tiles B=0x33, R=0xCC, G=0x55, E=0xAA
    check_shader_tile_band(
        &output,
        snapshot,
        0,
        200,
        [0x33, 0xCC, 0x55, 0xAA],
        "P7 top",
    );
    // Bottom: RMW result B=0xF5, R=0x5F, G=0xFF, E=0x55
    check_shader_tile_band(
        &output,
        snapshot,
        200,
        400,
        [0xF5, 0x5F, 0xFF, 0x55],
        "P7 bottom",
    );

    // Pattern 8: Mode1 Monochrome test
    // Lines 0-319: G+E planes (index 12, mono ON). Lines 320-399: B-plane (index 1, mono OFF).
    // Text attributes: 5 bands of colored attributes.
    send_enter(&mut machine.bus);
    run_steps(&mut machine, STEPS_PER_PATTERN);

    // Verify graphics VRAM
    check_plane_lines(&machine.bus, VRAM_G, 0, 320, 0xFF, "P8 G lines 0-319");
    check_plane_lines(&machine.bus, VRAM_G, 320, 400, 0x00, "P8 G lines 320-399");
    check_plane_lines(&machine.bus, VRAM_E, 0, 320, 0xFF, "P8 E lines 0-319");
    check_plane_lines(&machine.bus, VRAM_E, 320, 400, 0x00, "P8 E lines 320-399");
    check_plane_lines(&machine.bus, VRAM_B, 0, 320, 0x00, "P8 B lines 0-319");
    check_plane_lines(&machine.bus, VRAM_B, 320, 400, 0xFF, "P8 B lines 320-399");
    check_plane_lines(&machine.bus, VRAM_R, 0, 400, 0x00, "P8 R full");

    // Verify text attribute colors via direct memory reads
    // Text attr at A000:2000, each entry is 2 bytes (low=attr, high=0)
    let text_attr_base: u32 = 0xA2000;
    // Row 0 col 0 (band 0, white): attr = 0xE1
    let attr0 = machine.bus.read_byte_direct(text_attr_base);
    assert_eq!(
        attr0, 0xE1,
        "P8 text attr row 0: expected 0xE1, got 0x{attr0:02X}"
    );
    // Row 5 col 0 (band 1, red): attr = 0x41
    let attr5 = machine.bus.read_byte_direct(text_attr_base + 5 * 80 * 2);
    assert_eq!(
        attr5, 0x41,
        "P8 text attr row 5: expected 0x41, got 0x{attr5:02X}"
    );
    // Row 10 col 0 (band 2, green): attr = 0x81
    let attr10 = machine.bus.read_byte_direct(text_attr_base + 10 * 80 * 2);
    assert_eq!(
        attr10, 0x81,
        "P8 text attr row 10: expected 0x81, got 0x{attr10:02X}"
    );
    // Row 15 col 0 (band 3, cyan): attr = 0xA1
    let attr15 = machine.bus.read_byte_direct(text_attr_base + 15 * 80 * 2);
    assert_eq!(
        attr15, 0xA1,
        "P8 text attr row 15: expected 0xA1, got 0x{attr15:02X}"
    );
    // Row 20 col 0 (band 4, yellow): attr = 0xC1
    let attr20 = machine.bus.read_byte_direct(text_attr_base + 20 * 80 * 2);
    assert_eq!(
        attr20, 0xC1,
        "P8 text attr row 20: expected 0xC1, got 0x{attr20:02X}"
    );

    // Verify display snapshot monochrome mask
    // In analog mode with default palette, green channel bit 3 set for indices 12-15 only
    // (indices 4-7 have green=0x7 which lacks bit 3)
    machine.bus.capture_vsync_snapshot();
    assert_eq!(
        machine.bus.vsync_snapshot().graphics_monochrome_mask,
        0x0000F000,
        "P8 monochrome mask: expected 0xF000 (analog default), got 0x{:08X}",
        machine.bus.vsync_snapshot().graphics_monochrome_mask
    );
}

fn setup_grcg_bus() -> Pc9801Bus<NoTracing> {
    let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
    bus.io_write_byte(0x6A, 0x01); // analog mode for E-plane access
    bus
}

fn enable_grcg_tdw(bus: &mut Pc9801Bus<NoTracing>, tiles: [u8; 4]) {
    bus.io_write_byte(0x7C, 0x80); // TDW mode, all planes enabled
    for tile in tiles {
        bus.io_write_byte(0x7E, tile);
    }
}

fn enable_grcg_rmw(bus: &mut Pc9801Bus<NoTracing>, tiles: [u8; 4]) {
    bus.io_write_byte(0x7C, 0xC0); // RMW mode, all planes enabled
    for tile in tiles {
        bus.io_write_byte(0x7E, tile);
    }
}

fn disable_grcg_mode(bus: &mut Pc9801Bus<NoTracing>) {
    bus.io_write_byte(0x7C, 0x00);
}

fn read_plane_byte(bus: &Pc9801Bus<NoTracing>, plane_base: u32, offset: u32) -> u8 {
    bus.read_byte_direct(plane_base + offset)
}

fn read_all_planes_byte(bus: &Pc9801Bus<NoTracing>, offset: u32) -> [u8; 4] {
    [
        bus.read_byte_direct(VRAM_B + offset),
        bus.read_byte_direct(VRAM_R + offset),
        bus.read_byte_direct(VRAM_G + offset),
        bus.read_byte_direct(VRAM_E + offset),
    ]
}

fn prefill_all_planes_byte(bus: &mut Pc9801Bus<NoTracing>, offset: u32, values: [u8; 4]) {
    let bases = [VRAM_B, VRAM_R, VRAM_G, VRAM_E];
    for (i, &base) in bases.iter().enumerate() {
        bus.write_byte(base + offset, values[i]);
    }
}

#[test]
fn grcg_tdw_byte_write_all_planes() {
    let mut bus = setup_grcg_bus();
    enable_grcg_tdw(&mut bus, [0xAA, 0x55, 0xF0, 0x0F]);

    // Write any byte - CPU data is ignored in TDW, all planes get tile values.
    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(read_all_planes_byte(&bus, 0), [0xAA, 0x55, 0xF0, 0x0F]);
}

#[test]
fn grcg_tdw_word_write_all_planes() {
    let mut bus = setup_grcg_bus();
    enable_grcg_tdw(&mut bus, [0x33, 0xCC, 0x55, 0xAA]);

    bus.write_word(VRAM_B, 0xFFFF);

    disable_grcg_mode(&mut bus);

    // Both bytes should get tile value.
    let bases = [VRAM_B, VRAM_R, VRAM_G, VRAM_E];
    let tiles = [0x33, 0xCC, 0x55, 0xAA];
    for (i, &base) in bases.iter().enumerate() {
        assert_eq!(bus.read_byte_direct(base), tiles[i], "plane {i} low byte");
        assert_eq!(
            bus.read_byte_direct(base + 1),
            tiles[i],
            "plane {i} high byte"
        );
    }
}

#[test]
fn grcg_tcr_byte_read_all_match() {
    let mut bus = setup_grcg_bus();

    // Pre-fill VRAM to match tiles.
    prefill_all_planes_byte(&mut bus, 0, [0xAA, 0x55, 0xF0, 0x0F]);

    // Enable GRCG TCR mode (0x80 = TDW/TCR, reads are TCR).
    enable_grcg_tdw(&mut bus, [0xAA, 0x55, 0xF0, 0x0F]);

    let result = bus.read_byte(VRAM_B);
    assert_eq!(result, 0xFF, "all bits should match");
}

#[test]
fn grcg_tcr_byte_read_no_match() {
    let mut bus = setup_grcg_bus();

    // VRAM is opposite of tiles.
    prefill_all_planes_byte(&mut bus, 0, [0x55, 0xAA, 0x0F, 0xF0]);

    enable_grcg_tdw(&mut bus, [0xAA, 0x55, 0xF0, 0x0F]);

    let result = bus.read_byte(VRAM_B);
    assert_eq!(result, 0x00, "no bits should match");
}

#[test]
fn grcg_tcr_byte_read_partial_match() {
    let mut bus = setup_grcg_bus();

    // VRAM: B=0xAA matches tile, R=0xFF doesn't fully match tile 0x55.
    prefill_all_planes_byte(&mut bus, 0, [0xAA, 0xFF, 0xF0, 0x0F]);

    enable_grcg_tdw(&mut bus, [0xAA, 0x55, 0xF0, 0x0F]);

    let result = bus.read_byte(VRAM_B);
    // R-plane mismatch: !(0xFF ^ 0x55) = !(0xAA) = 0x55. AND with 0xFF from others = 0x55.
    assert_eq!(result, 0x55, "partial match from R-plane mismatch");
}

#[test]
fn grcg_tcr_word_read_all_match() {
    let mut bus = setup_grcg_bus();

    // Pre-fill 2 bytes per plane.
    let bases = [VRAM_B, VRAM_R, VRAM_G, VRAM_E];
    let tiles = [0xAA_u8, 0x55, 0xF0, 0x0F];
    for (i, &base) in bases.iter().enumerate() {
        bus.write_byte(base, tiles[i]);
        bus.write_byte(base + 1, tiles[i]);
    }

    enable_grcg_tdw(&mut bus, tiles);

    let result = bus.read_word(VRAM_B);
    assert_eq!(result, 0xFFFF, "both bytes fully match");
}

#[test]
fn grcg_rmw_byte_write() {
    let mut bus = setup_grcg_bus();

    // Pre-fill with 0x55.
    prefill_all_planes_byte(&mut bus, 0, [0x55; 4]);

    enable_grcg_rmw(&mut bus, [0xFF, 0x00, 0xAA, 0x55]);

    // CPU writes 0xF0: new = (0xF0 & tile) | (~0xF0 & current)
    //   B: (0xF0 & 0xFF) | (0x0F & 0x55) = 0xF0 | 0x05 = 0xF5
    //   R: (0xF0 & 0x00) | (0x0F & 0x55) = 0x00 | 0x05 = 0x05
    //   G: (0xF0 & 0xAA) | (0x0F & 0x55) = 0xA0 | 0x05 = 0xA5
    //   E: (0xF0 & 0x55) | (0x0F & 0x55) = 0x50 | 0x05 = 0x55
    bus.write_byte(VRAM_B, 0xF0);

    disable_grcg_mode(&mut bus);

    assert_eq!(read_all_planes_byte(&bus, 0), [0xF5, 0x05, 0xA5, 0x55]);
}

#[test]
fn grcg_rmw_word_write() {
    let mut bus = setup_grcg_bus();

    let bases = [VRAM_B, VRAM_R, VRAM_G, VRAM_E];
    for &base in &bases {
        bus.write_byte(base, 0xFF);
        bus.write_byte(base + 1, 0xFF);
    }

    enable_grcg_rmw(&mut bus, [0xFF, 0x00, 0xAA, 0x55]);

    // CPU writes 0xAAAA: low=0xAA, high=0xAA
    // For each byte: new = (cpu & tile) | (~cpu & current)
    //   B: (0xAA & 0xFF) | (0x55 & 0xFF) = 0xAA | 0x55 = 0xFF
    //   R: (0xAA & 0x00) | (0x55 & 0xFF) = 0x00 | 0x55 = 0x55
    //   G: (0xAA & 0xAA) | (0x55 & 0xFF) = 0xAA | 0x55 = 0xFF
    //   E: (0xAA & 0x55) | (0x55 & 0xFF) = 0x00 | 0x55 = 0x55
    bus.write_word(VRAM_B, 0xAAAA);

    disable_grcg_mode(&mut bus);

    for (i, &base) in bases.iter().enumerate() {
        let expected = [0xFF, 0x55, 0xFF, 0x55];
        assert_eq!(
            bus.read_byte_direct(base),
            expected[i],
            "plane {i} low byte"
        );
        assert_eq!(
            bus.read_byte_direct(base + 1),
            expected[i],
            "plane {i} high byte"
        );
    }
}

#[test]
fn grcg_rmw_read_returns_raw_vram() {
    let mut bus = setup_grcg_bus();

    // Pre-fill with distinctive data.
    prefill_all_planes_byte(&mut bus, 0, [0xAB, 0xCD, 0xEF, 0x12]);

    enable_grcg_rmw(&mut bus, [0xFF, 0xFF, 0xFF, 0xFF]);

    // In RMW mode, reads should bypass GRCG and return normal VRAM data.
    let result = bus.read_byte(VRAM_B);
    assert_eq!(result, 0xAB, "RMW read should return B-plane data");

    let result = bus.read_byte(VRAM_R);
    assert_eq!(result, 0xCD, "RMW read should return R-plane data");

    let result = bus.read_byte(VRAM_G);
    assert_eq!(result, 0xEF, "RMW read should return G-plane data");

    let result = bus.read_byte(VRAM_E);
    assert_eq!(result, 0x12, "RMW read should return E-plane data");
}

#[test]
fn grcg_rmw_read_word_returns_raw_vram() {
    let mut bus = setup_grcg_bus();

    let bases = [VRAM_B, VRAM_R, VRAM_G, VRAM_E];
    let values: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
    for (i, &base) in bases.iter().enumerate() {
        bus.write_byte(base, values[i]);
        bus.write_byte(base + 1, values[i]);
    }

    enable_grcg_rmw(&mut bus, [0xFF; 4]);

    let result = bus.read_word(VRAM_B);
    assert_eq!(result, 0x1212, "RMW word read B-plane");

    let result = bus.read_word(VRAM_R);
    assert_eq!(result, 0x3434, "RMW word read R-plane");
}

#[test]
fn grcg_tdw_selective_plane_enable() {
    let mut bus = setup_grcg_bus();

    // Pre-fill all planes with 0xFF.
    prefill_all_planes_byte(&mut bus, 0, [0xFF; 4]);

    // TDW with planes 1 (R) and 3 (E) disabled: mode = 0x8A (bits 1,3 set).
    bus.io_write_byte(0x7C, 0x8A);
    bus.io_write_byte(0x7E, 0x00); // B tile
    bus.io_write_byte(0x7E, 0x00); // R tile (disabled)
    bus.io_write_byte(0x7E, 0x00); // G tile
    bus.io_write_byte(0x7E, 0x00); // E tile (disabled)

    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    // B and G should get tile (0x00), R and E unchanged (0xFF).
    assert_eq!(read_plane_byte(&bus, VRAM_B, 0), 0x00, "B written");
    assert_eq!(read_plane_byte(&bus, VRAM_R, 0), 0xFF, "R unchanged");
    assert_eq!(read_plane_byte(&bus, VRAM_G, 0), 0x00, "G written");
    assert_eq!(read_plane_byte(&bus, VRAM_E, 0), 0xFF, "E unchanged");
}

#[test]
fn grcg_rmw_selective_plane_enable() {
    let mut bus = setup_grcg_bus();

    // Pre-fill all planes with 0xAA.
    prefill_all_planes_byte(&mut bus, 0, [0xAA; 4]);

    // RMW with planes 0 (B) and 2 (G) disabled: mode = 0xC5 (bits 0,2 set).
    bus.io_write_byte(0x7C, 0xC5);
    bus.io_write_byte(0x7E, 0xFF); // B tile (disabled)
    bus.io_write_byte(0x7E, 0xFF); // R tile
    bus.io_write_byte(0x7E, 0xFF); // G tile (disabled)
    bus.io_write_byte(0x7E, 0xFF); // E tile

    // CPU writes 0xFF: new = (0xFF & tile) | (0x00 & current) = tile
    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    // B and G unchanged (0xAA), R and E get tile (0xFF).
    assert_eq!(read_plane_byte(&bus, VRAM_B, 0), 0xAA, "B unchanged");
    assert_eq!(read_plane_byte(&bus, VRAM_R, 0), 0xFF, "R written");
    assert_eq!(read_plane_byte(&bus, VRAM_G, 0), 0xAA, "G unchanged");
    assert_eq!(read_plane_byte(&bus, VRAM_E, 0), 0xFF, "E written");
}

#[test]
fn grcg_mode_switch_tdw_to_rmw() {
    let mut bus = setup_grcg_bus();

    // Start in TDW mode.
    enable_grcg_tdw(&mut bus, [0xAA, 0x55, 0xF0, 0x0F]);
    bus.write_byte(VRAM_B, 0xFF);

    // Verify TDW result.
    disable_grcg_mode(&mut bus);
    assert_eq!(read_all_planes_byte(&bus, 0), [0xAA, 0x55, 0xF0, 0x0F]);

    // Switch to RMW mode with different tiles.
    enable_grcg_rmw(&mut bus, [0xFF, 0xFF, 0xFF, 0xFF]);

    // CPU writes 0xF0: new = (0xF0 & 0xFF) | (0x0F & current)
    //   B: 0xF0 | (0x0F & 0xAA) = 0xF0 | 0x0A = 0xFA
    //   R: 0xF0 | (0x0F & 0x55) = 0xF0 | 0x05 = 0xF5
    //   G: 0xF0 | (0x0F & 0xF0) = 0xF0 | 0x00 = 0xF0
    //   E: 0xF0 | (0x0F & 0x0F) = 0xF0 | 0x0F = 0xFF
    bus.write_byte(VRAM_B, 0xF0);

    disable_grcg_mode(&mut bus);
    assert_eq!(read_all_planes_byte(&bus, 0), [0xFA, 0xF5, 0xF0, 0xFF]);
}

#[test]
fn grcg_tcr_selective_plane_compare() {
    let mut bus = setup_grcg_bus();

    // Set VRAM: B=0xAA, R=0x00 (mismatch), G=0xF0, E=0x0F.
    prefill_all_planes_byte(&mut bus, 0, [0xAA, 0x00, 0xF0, 0x0F]);

    // TCR with R-plane disabled (bit 1 set): mode=0x82.
    bus.io_write_byte(0x7C, 0x82);
    bus.io_write_byte(0x7E, 0xAA); // B tile
    bus.io_write_byte(0x7E, 0xFF); // R tile (disabled - won't be compared)
    bus.io_write_byte(0x7E, 0xF0); // G tile
    bus.io_write_byte(0x7E, 0x0F); // E tile

    // TCR should skip R-plane comparison.
    // B: !(0xAA ^ 0xAA) = 0xFF
    // G: !(0xF0 ^ 0xF0) = 0xFF
    // E: !(0x0F ^ 0x0F) = 0xFF
    // Result = 0xFF & 0xFF & 0xFF = 0xFF (R skipped).
    let result = bus.read_byte(VRAM_B);
    assert_eq!(
        result, 0xFF,
        "TCR with R-plane disabled should be all match"
    );
}

#[test]
fn grcg_tdw_sequential_writes() {
    let mut bus = setup_grcg_bus();
    enable_grcg_tdw(&mut bus, [0x11, 0x22, 0x33, 0x44]);

    // Write to multiple consecutive offsets.
    bus.write_byte(VRAM_B, 0xFF);
    bus.write_byte(VRAM_B + 1, 0xFF);
    bus.write_byte(VRAM_B + 2, 0xFF);

    disable_grcg_mode(&mut bus);

    // All 3 bytes at each plane should get tile values.
    for offset in 0..3 {
        let planes = read_all_planes_byte(&bus, offset);
        assert_eq!(planes, [0x11, 0x22, 0x33, 0x44], "offset {offset}");
    }
}

#[test]
fn grcg_tdw_via_e_plane_address() {
    let mut bus = setup_grcg_bus();
    enable_grcg_tdw(&mut bus, [0xDE, 0xAD, 0xBE, 0xEF]);

    // Write via E-plane address - GRCG should still write all 4 planes at the offset.
    bus.write_byte(VRAM_E, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(read_all_planes_byte(&bus, 0), [0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn grcg_tcr_via_e_plane_address() {
    let mut bus = setup_grcg_bus();

    prefill_all_planes_byte(&mut bus, 0, [0xAA, 0x55, 0xF0, 0x0F]);

    enable_grcg_tdw(&mut bus, [0xAA, 0x55, 0xF0, 0x0F]);

    // TCR read via E-plane address - should compare all 4 planes at offset 0.
    let result = bus.read_byte(VRAM_E);
    assert_eq!(result, 0xFF, "TCR via E-plane address, all match");
}

#[test]
fn grcg_rmw_all_zero_preserves_vram() {
    let mut bus = setup_grcg_bus();

    prefill_all_planes_byte(&mut bus, 0, [0xAB, 0xCD, 0xEF, 0x12]);

    enable_grcg_rmw(&mut bus, [0x55, 0xAA, 0x33, 0xCC]);

    // RMW write value=0x00: new = (0x00 & tile) | (0xFF & current) = current.
    bus.write_byte(VRAM_B, 0x00);

    disable_grcg_mode(&mut bus);

    assert_eq!(
        read_all_planes_byte(&bus, 0),
        [0xAB, 0xCD, 0xEF, 0x12],
        "VRAM unchanged with RMW write=0x00"
    );
}

#[test]
fn grcg_rmw_all_one_writes_tile() {
    let mut bus = setup_grcg_bus();

    prefill_all_planes_byte(&mut bus, 0, [0xAB, 0xCD, 0xEF, 0x12]);

    enable_grcg_rmw(&mut bus, [0x55, 0xAA, 0x33, 0xCC]);

    // RMW write value=0xFF: new = (0xFF & tile) | (0x00 & current) = tile.
    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(
        read_all_planes_byte(&bus, 0),
        [0x55, 0xAA, 0x33, 0xCC],
        "VRAM should be tile values with RMW write=0xFF"
    );
}

#[test]
fn grcg_tcr_partial_plane_combinations() {
    let mut bus = setup_grcg_bus();

    // Set VRAM: B=0xAA, R=0x55, G=0xF0, E=0x0F.
    prefill_all_planes_byte(&mut bus, 0, [0xAA, 0x55, 0xF0, 0x0F]);

    // TCR with only B+G enabled (R,E disabled): mode=0x8A (bits 1,3 set).
    bus.io_write_byte(0x7C, 0x8A);
    bus.io_write_byte(0x7E, 0xAA); // B tile (matches)
    bus.io_write_byte(0x7E, 0xFF); // R tile (disabled)
    bus.io_write_byte(0x7E, 0xF0); // G tile (matches)
    bus.io_write_byte(0x7E, 0xFF); // E tile (disabled)

    let result = bus.read_byte(VRAM_B);
    assert_eq!(result, 0xFF, "B+G match, R+E disabled -> all match");

    disable_grcg_mode(&mut bus);

    // TCR with only R+E enabled (B,G disabled): mode=0x85 (bits 0,2 set).
    bus.io_write_byte(0x7C, 0x85);
    bus.io_write_byte(0x7E, 0xFF); // B tile (disabled)
    bus.io_write_byte(0x7E, 0x55); // R tile (matches)
    bus.io_write_byte(0x7E, 0xFF); // G tile (disabled)
    bus.io_write_byte(0x7E, 0x0F); // E tile (matches)

    let result = bus.read_byte(VRAM_B);
    assert_eq!(result, 0xFF, "R+E match, B+G disabled -> all match");

    disable_grcg_mode(&mut bus);

    // Single plane (B only): mode=0x8E (bits 1,2,3 set).
    bus.io_write_byte(0x7C, 0x8E);
    bus.io_write_byte(0x7E, 0xAA); // B tile (matches)
    bus.io_write_byte(0x7E, 0x00); // R tile (disabled)
    bus.io_write_byte(0x7E, 0x00); // G tile (disabled)
    bus.io_write_byte(0x7E, 0x00); // E tile (disabled)

    let result = bus.read_byte(VRAM_B);
    assert_eq!(result, 0xFF, "B only match -> all match");
}

#[test]
fn grcg_mode_switch_resets_tile_index() {
    let mut bus = setup_grcg_bus();

    // Write 2 tiles in TDW mode.
    bus.io_write_byte(0x7C, 0x80);
    bus.io_write_byte(0x7E, 0x11); // tile[0]
    bus.io_write_byte(0x7E, 0x22); // tile[1]

    // Switch mode (write to 0x7C resets tile_index).
    bus.io_write_byte(0x7C, 0x80);
    bus.io_write_byte(0x7E, 0xAA); // tile[0] overwritten

    // Write remaining tiles.
    bus.io_write_byte(0x7E, 0xBB); // tile[1]
    bus.io_write_byte(0x7E, 0xCC); // tile[2]
    bus.io_write_byte(0x7E, 0xDD); // tile[3]

    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(
        read_all_planes_byte(&bus, 0),
        [0xAA, 0xBB, 0xCC, 0xDD],
        "tile[0] should be overwritten after mode switch"
    );
}

#[test]
fn grcg_tile_cycling_wraps_after_four() {
    let mut bus = setup_grcg_bus();

    bus.io_write_byte(0x7C, 0x80);
    // Write 5 tile values - 5th should wrap and overwrite tile[0].
    bus.io_write_byte(0x7E, 0x11); // tile[0]
    bus.io_write_byte(0x7E, 0x22); // tile[1]
    bus.io_write_byte(0x7E, 0x33); // tile[2]
    bus.io_write_byte(0x7E, 0x44); // tile[3]
    bus.io_write_byte(0x7E, 0xFF); // tile[0] overwritten

    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(
        read_all_planes_byte(&bus, 0),
        [0xFF, 0x22, 0x33, 0x44],
        "5th write should wrap to tile[0]"
    );
}

#[test]
fn grcg_tdw_word_various_offsets() {
    let mut bus = setup_grcg_bus();
    enable_grcg_tdw(&mut bus, [0x11, 0x22, 0x33, 0x44]);

    // Write at offset 0.
    bus.write_word(VRAM_B, 0xFFFF);
    // Write at offset 80 (next line).
    bus.write_word(VRAM_B + 80, 0xFFFF);
    // Write at offset 160 (2 lines down).
    bus.write_word(VRAM_B + 160, 0xFFFF);

    disable_grcg_mode(&mut bus);

    let bases = [VRAM_B, VRAM_R, VRAM_G, VRAM_E];
    let tiles = [0x11u8, 0x22, 0x33, 0x44];
    for offset in [0u32, 80, 160] {
        for (i, &base) in bases.iter().enumerate() {
            assert_eq!(
                bus.read_byte_direct(base + offset),
                tiles[i],
                "plane {i} offset {offset} lo"
            );
            assert_eq!(
                bus.read_byte_direct(base + offset + 1),
                tiles[i],
                "plane {i} offset {offset} hi"
            );
        }
    }
}

#[test]
fn grcg_e_plane_disabled_skips_write() {
    let mut bus = setup_grcg_bus();

    // Pre-fill E-plane with 0xAA while extension is enabled.
    prefill_all_planes_byte(&mut bus, 0, [0x00, 0x00, 0x00, 0xAA]);

    // Disable the graphics extension board.
    bus.set_graphics_extension_enabled(false);

    // TDW: all planes enabled, but graphics_extension is off -> E-plane skipped.
    enable_grcg_tdw(&mut bus, [0xFF, 0xFF, 0xFF, 0xFF]);
    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(read_plane_byte(&bus, VRAM_B, 0), 0xFF, "B written");
    assert_eq!(read_plane_byte(&bus, VRAM_R, 0), 0xFF, "R written");
    assert_eq!(read_plane_byte(&bus, VRAM_G, 0), 0xFF, "G written");

    // Re-enable extension to verify E-plane is unchanged.
    bus.set_graphics_extension_enabled(true);
    assert_eq!(
        read_plane_byte(&bus, VRAM_E, 0),
        0xAA,
        "E unchanged (extension was disabled during TDW)"
    );

    // RMW test: prefill, disable extension, write, verify.
    prefill_all_planes_byte(&mut bus, 0, [0x00, 0x00, 0x00, 0xBB]);
    bus.set_graphics_extension_enabled(false);

    enable_grcg_rmw(&mut bus, [0xFF, 0xFF, 0xFF, 0xFF]);
    bus.write_byte(VRAM_B, 0xFF);

    disable_grcg_mode(&mut bus);

    assert_eq!(read_plane_byte(&bus, VRAM_B, 0), 0xFF, "RMW B");
    assert_eq!(read_plane_byte(&bus, VRAM_R, 0), 0xFF, "RMW R");
    assert_eq!(read_plane_byte(&bus, VRAM_G, 0), 0xFF, "RMW G");

    bus.set_graphics_extension_enabled(true);
    assert_eq!(
        read_plane_byte(&bus, VRAM_E, 0),
        0xBB,
        "E unchanged (extension was disabled during RMW)"
    );
}
