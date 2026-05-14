//! End-to-end PEGC display tests. Each test boots the `debug_pegc.asm` ROM
//! with a different mode-selector byte and asserts that the software-renderer
//! framebuffer matches the expected color pattern. This validates the full
//! chain: CPU -> bus I/O -> PEGC state -> compose -> framebuffer bytes.

use common::{Bus, Cpu, CpuMode, MachineModel};
use machine::{Pc9801Bus, Pc9821Ap};

const DEBUG_PEGC_ROM: &[u8] = include_bytes!("../utils/debug/debug_pegc.rom");
const FONT_ROM_DATA: &[u8] = include_bytes!("../utils/font/font.rom");

const MODE_BYTE_ADDR: u32 = 0x0500;
const RUN_BUDGET_CYCLES: u64 = 50_000_000;

const FB_WIDTH: u32 = 640;
const FB_HEIGHT_MAX: u32 = 480;

fn run_mode(mode: u8) -> (Vec<u8>, u32) {
    let mut bus = Pc9801Bus::new(MachineModel::PC9821AP, CpuMode::High, 48000);
    bus.set_gdc_clock_5mhz();

    let mut machine = Pc9821Ap::new(cpu::I386::<{ cpu::CPU_MODEL_486 }>::new(), bus);
    machine.bus.load_font_rom(FONT_ROM_DATA);
    machine.bus.load_bios_rom(DEBUG_PEGC_ROM);
    machine.bus.write_byte(MODE_BYTE_ADDR, mode);

    let _ = machine.run_for(RUN_BUDGET_CYCLES);

    assert!(
        machine.cpu.halted(),
        "ROM did not reach HLT for mode {mode}",
    );

    let framebuffer = machine.bus.display_framebuffer().to_vec();
    let height = machine.bus.display_native_height();
    assert_eq!(
        framebuffer.len(),
        (FB_WIDTH * FB_HEIGHT_MAX * 4) as usize,
        "unexpected framebuffer length for mode {mode}",
    );
    (framebuffer, height)
}

fn pixel(framebuffer: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * FB_WIDTH + x) * 4) as usize;
    [framebuffer[i], framebuffer[i + 1], framebuffer[i + 2]]
}

/// HSV-cycle palette, identical to the asm `palette_256` table.
/// Returns (red, green, blue) for use with the framebuffer's R/G/B/A layout.
fn hsv_rgb(index: u8) -> [u8; 3] {
    let h6 = u32::from(index) * 6;
    let sector = h6 / 256;
    let f = (h6 % 256) as u8;
    let t = f;
    let q = 255 - f;
    let (green, red, blue) = match sector {
        0 => (t, 255, 0),
        1 => (255, q, 0),
        2 => (255, 0, t),
        3 => (q, 0, 255),
        4 => (0, t, 255),
        5 => (0, 255, q),
        _ => unreachable!(),
    };
    [red, green, blue]
}

fn assert_pixel(framebuffer: &[u8], x: u32, y: u32, expected: [u8; 3], context: &str) {
    let got = pixel(framebuffer, x, y);
    assert_eq!(
        got, expected,
        "{context}: pixel ({x},{y}) expected RGB {expected:?}, got {got:?}",
    );
}

#[test]
fn pegc_mode_1_packed_256color_640x400() {
    let (framebuffer, height) = run_mode(1);
    assert_eq!(height, 400, "mode 1 should render 400 lines");

    for row in 0..16u32 {
        for col in 0..16u32 {
            let x = col * 40 + 20;
            let y = row * 25 + 12;
            let expected = hsv_rgb((row * 16 + col) as u8);
            assert_pixel(
                &framebuffer,
                x,
                y,
                expected,
                &format!("mode 1 cell ({col},{row})"),
            );
        }
    }
}

#[test]
fn pegc_mode_2_packed_256color_640x480() {
    let (framebuffer, height) = run_mode(2);
    assert_eq!(
        height, 480,
        "mode 2 should render 480 lines (port 09A8h + GDC AL=480)",
    );

    for row in 0..16u32 {
        for col in 0..16u32 {
            let x = col * 40 + 20;
            let y = row * 30 + 15;
            let expected = hsv_rgb((row * 16 + col) as u8);
            assert_pixel(
                &framebuffer,
                x,
                y,
                expected,
                &format!("mode 2 cell ({col},{row})"),
            );
        }
    }
}

#[test]
fn pegc_mode_3_plane_quadrants_640x400() {
    let (framebuffer, height) = run_mode(3);
    assert_eq!(height, 400, "mode 3 should render 400 lines");

    let tl = hsv_rgb(0x33);
    let tr = hsv_rgb(0x66);
    let bl = hsv_rgb(0x99);
    let br = hsv_rgb(0xCC);

    assert_pixel(&framebuffer, 160, 100, tl, "mode 3 TL");
    assert_pixel(&framebuffer, 480, 100, tr, "mode 3 TR");
    assert_pixel(&framebuffer, 160, 300, bl, "mode 3 BL");
    assert_pixel(&framebuffer, 480, 300, br, "mode 3 BR");
}

#[test]
fn pegc_mode_4_plane_quadrants_640x480() {
    let (framebuffer, height) = run_mode(4);
    assert_eq!(
        height, 480,
        "mode 4 should render 480 lines (port 09A8h + GDC AL=480)",
    );

    let tl = hsv_rgb(0x33);
    let tr = hsv_rgb(0x66);
    let bl = hsv_rgb(0x99);
    let br = hsv_rgb(0xCC);

    assert_pixel(&framebuffer, 160, 120, tl, "mode 4 TL");
    assert_pixel(&framebuffer, 480, 120, tr, "mode 4 TR");
    assert_pixel(&framebuffer, 160, 360, bl, "mode 4 BL");
    assert_pixel(&framebuffer, 480, 360, br, "mode 4 BR");
}
