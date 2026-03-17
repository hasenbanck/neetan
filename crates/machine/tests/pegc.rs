use common::{Bus, MachineModel};
use machine::{NoTracing, Pc9801Bus};

const PEGC_VRAM_A: u32 = 0xA8000;
const PEGC_MMIO_BANK_A8: u32 = 0xE0004;
const BANK_SIZE: u32 = 0x8000;
const GRID_ROWS: u32 = 16;
const GRID_COLS: u32 = 16;
const CELL_WIDTH: u32 = 40;
const LINES_PER_ROW_400: u32 = 25;

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
