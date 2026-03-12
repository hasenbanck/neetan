use common::Bus;
use machine::{Pc9801Ra, Pc9801Vm, Pc9801Vx};

use super::{
    create_machine_ra, create_machine_vm, create_machine_vx, read_ivt_vector, read_ram_u16,
    write_bytes,
};

// LIO handlers use DS:0x1006 as a saved-BX register location, so we must place
// test code above the LIO internal work area (which extends to ~0x10CE).
const TEST_CODE: u32 = 0x1100;

const RESULT: u32 = 0x2700;
const PARAMS: u32 = 0x2800;
const PARAMS2: u32 = 0x2840;
const PARAMS3: u32 = 0x2880;
const BUFFER: u32 = 0x2900;
const WORK_AREA: u32 = 0x2A00;
const LIO_BUDGET: u64 = 20_000_000;

// VRAM plane bases (bus addresses).
const VRAM_B: u32 = 0xA8000;
const VRAM_R: u32 = 0xB0000;
const VRAM_G: u32 = 0xB8000;

// LIO work area addresses in DS:0 segment (databook p.142).
const LIO_SCRNMODE: usize = 0x0620;
const LIO_FGCOLOR: usize = 0x0623;
const LIO_BGCOLOR: usize = 0x0624;
const LIO_PALETTE: usize = 0x0626;
const LIO_VIEWX1: usize = 0x062E;
const LIO_VIEWY1: usize = 0x0630;
const LIO_VIEWX2: usize = 0x0632;
const LIO_VIEWY2: usize = 0x0634;
const LIO_PALMODE: usize = 0x0A08;

fn boot_lio_vm() -> Pc9801Vm {
    let mut machine = create_machine_vm();
    boot_to_halt!(machine);
    machine
}

fn boot_lio_vx() -> Pc9801Vx {
    let mut machine = create_machine_vx();
    boot_to_halt!(machine);
    machine
}

fn boot_lio_ra() -> Pc9801Ra {
    let mut machine = create_machine_ra();
    boot_to_halt!(machine);
    machine
}

fn run_lio_vm(machine: &mut Pc9801Vm, code: &[u8]) {
    write_bytes(&mut machine.bus, TEST_CODE, code);
    machine.cpu.load_state(&{
        let mut s = cpu::V30State {
            ip: TEST_CODE as u16,
            ..Default::default()
        };
        s.set_sp(0x4000);
        s
    });
    machine.run_for(LIO_BUDGET);
}

fn run_lio_vx(machine: &mut Pc9801Vx, code: &[u8]) {
    write_bytes(&mut machine.bus, TEST_CODE, code);
    machine.cpu.load_state(&{
        let mut s = cpu::I286State {
            ip: TEST_CODE as u16,
            ..Default::default()
        };
        s.set_sp(0x4000);
        s
    });
    machine.run_for(LIO_BUDGET);
}

fn run_lio_ra(machine: &mut Pc9801Ra, code: &[u8]) {
    write_bytes(&mut machine.bus, TEST_CODE, code);
    machine.cpu.load_state(&{
        let mut s = cpu::I386State {
            ip: TEST_CODE as u16,
            ..Default::default()
        };
        s.set_esp(0x4000);
        s
    });
    machine.run_for(LIO_BUDGET);
}

/// GINIT only: XOR AX,AX; MOV DS,AX; INT 0xA0; MOV [RESULT],AX; HLT
#[rustfmt::skip]
fn make_ginit_call() -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT first, then single LIO call with BX=PARAMS.
#[rustfmt::skip]
fn make_ginit_then_lio(int_num: u8) -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,  // MOV BX, PARAMS
        0xCD, int_num,          // INT int_num
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT, then GPSET with AH=mode on input.
#[rustfmt::skip]
fn make_ginit_then_gpset(ah_mode: u8) -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,  // MOV BX, PARAMS
        0xB4, ah_mode,          // MOV AH, mode
        0xCD, 0xA6,             // INT 0xA6 (GPSET)
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT, GPSET at PARAMS (AH=01), then another LIO call at PARAMS2.
#[rustfmt::skip]
fn make_ginit_gpset_then_lio(int_num: u8) -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,  // MOV BX, PARAMS
        0xB4, 0x01,             // MOV AH, 0x01 (fg mode)
        0xCD, 0xA6,             // INT 0xA6 (GPSET)
        0xBB, (PARAMS2 & 0xFF) as u8, (PARAMS2 >> 8) as u8,  // MOV BX, PARAMS2
        0xCD, int_num,          // INT int_num
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT, then GLINE (INT 0xA7) at PARAMS, then another LIO call at PARAMS2.
#[rustfmt::skip]
fn make_ginit_gline_then_lio(int_num: u8) -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,  // MOV BX, PARAMS
        0xCD, 0xA7,             // INT 0xA7 (GLINE)
        0xBB, (PARAMS2 & 0xFF) as u8, (PARAMS2 >> 8) as u8,  // MOV BX, PARAMS2
        0xCD, int_num,          // INT int_num
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// Stop graphics first, then GINIT should re-enable it.
#[rustfmt::skip]
fn make_gstop_then_ginit() -> Vec<u8> {
    vec![
        0xB4, 0x41, 0xCD, 0x18,    // INT 18h AH=41h (graphics stop)
        0x31, 0xC0,                 // XOR AX, AX
        0x8E, 0xD8,                 // MOV DS, AX
        0xCD, 0xA0,                 // INT 0xA0 (GINIT)
        0xF4,                       // HLT
    ]
}

/// GINIT → GPSET(0,0,pal=7) → GGET 8x8 at (0,0)
#[rustfmt::skip]
fn make_ginit_gpset_gget() -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        // GPSET at PARAMS: (0,0,pal=7), AH=01
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,
        0xB4, 0x01,             // MOV AH, 0x01
        0xCD, 0xA6,             // INT 0xA6 (GPSET)
        // GGET at PARAMS2
        0xBB, (PARAMS2 & 0xFF) as u8, (PARAMS2 >> 8) as u8,
        0xCD, 0xAB,             // INT 0xAB (GGET)
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT → GPSET(0,0) → GGET(0,0,7,7) → GPUT1(100,100)
/// Uses three parameter blocks: PARAMS for GPSET, PARAMS2 for GGET, PARAMS3 for GPUT1.
#[rustfmt::skip]
fn make_ginit_gpset_gget_gput1() -> Vec<u8> {
    let p = PARAMS as u16;
    let p2 = PARAMS2 as u16;
    let p3 = PARAMS3 as u16;
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        // GPSET(0,0,pal=7) AH=01
        0xBB, (p & 0xFF) as u8, (p >> 8) as u8,
        0xB4, 0x01,
        0xCD, 0xA6,
        // GGET(0,0,7,7) → BUFFER
        0xBB, (p2 & 0xFF) as u8, (p2 >> 8) as u8,
        0xCD, 0xAB,
        // GPUT1(100,100) from BUFFER
        0xBB, (p3 & 0xFF) as u8, (p3 >> 8) as u8,
        0xCD, 0xAC,
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT → GPSET(200,150,pal=5) → GPOINT2(200,150)
#[rustfmt::skip]
fn make_ginit_gpset_gpoint2() -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0x8E, 0xC0,             // MOV ES, AX  (GPOINT2 needs ES=DS)
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        // GPSET at PARAMS, AH=01
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,
        0xB4, 0x01,
        0xCD, 0xA6,
        // GPOINT2 at PARAMS2
        0xBB, (PARAMS2 & 0xFF) as u8, (PARAMS2 >> 8) as u8,
        0xCD, 0xAF,
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GINIT → GVIEW(100,100,500,150) → GPOINT2(50,50) → should return AL=FF (outside viewport)
#[rustfmt::skip]
fn make_ginit_gview_gpoint2_oob() -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0x8E, 0xC0,             // MOV ES, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        // GVIEW at PARAMS
        0xBB, (PARAMS & 0xFF) as u8, (PARAMS >> 8) as u8,
        0xCD, 0xA2,
        // GPOINT2 at PARAMS2
        0xBB, (PARAMS2 & 0xFF) as u8, (PARAMS2 >> 8) as u8,
        0xCD, 0xAF,
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// GCOPY uses register-based parameters, not BX→param block.
/// AX=X, BX=Y, CL=dx, CH=direction, DI=storage_offset, ES=storage_segment
#[rustfmt::skip]
fn make_ginit_then_gcopy(x: u16, y: u16, dx: u8, direction: u8) -> Vec<u8> {
    vec![
        0x31, 0xC0,             // XOR AX, AX
        0x8E, 0xD8,             // MOV DS, AX
        0x8E, 0xC0,             // MOV ES, AX
        0xCD, 0xA0,             // INT 0xA0 (GINIT)
        // Set up registers for GCOPY
        0xB8, (x & 0xFF) as u8, (x >> 8) as u8,    // MOV AX, x
        0xBB, (y & 0xFF) as u8, (y >> 8) as u8,    // MOV BX, y
        0xB1, dx,                                    // MOV CL, dx
        0xB5, direction,                             // MOV CH, direction
        0xBF, (BUFFER & 0xFF) as u8, ((BUFFER >> 8) & 0xFF) as u8,  // MOV DI, BUFFER
        0xCD, 0xCE,             // INT 0xCE (GCOPY)
        0xA3, (RESULT & 0xFF) as u8, (RESULT >> 8) as u8, // MOV [RESULT], AX
        0xF4,                   // HLT
    ]
}

/// Read a 3-bit pixel color from VRAM B/R/G planes at (x, y).
fn read_pixel(bus: &mut impl Bus, x: u16, y: u16) -> u8 {
    let byte_offset = (y as u32) * 80 + (x as u32) / 8;
    let bit_mask = 0x80u8 >> (x & 7);
    let b = (bus.read_byte(VRAM_B + byte_offset) & bit_mask != 0) as u8;
    let r = (bus.read_byte(VRAM_R + byte_offset) & bit_mask != 0) as u8;
    let g = (bus.read_byte(VRAM_G + byte_offset) & bit_mask != 0) as u8;
    b | (r << 1) | (g << 2)
}

/// Read a pixel directly from saved graphics VRAM state, bypassing the bus.
/// This avoids GRCG/EGC interference that can affect bus reads after LIO operations.
fn read_pixel_from_state(state: &machine::MachineState, x: u16, y: u16) -> u8 {
    let byte_offset = (y as usize) * 80 + (x as usize) / 8;
    let bit_mask = 0x80u8 >> (x & 7);
    let b = (state.memory.graphics_vram[byte_offset] & bit_mask != 0) as u8;
    let r = (state.memory.graphics_vram[0x8000 + byte_offset] & bit_mask != 0) as u8;
    let g = (state.memory.graphics_vram[0x10000 + byte_offset] & bit_mask != 0) as u8;
    b | (r << 1) | (g << 2)
}

fn any_pixel_set(bus: &mut impl Bus, width: u16, height: u16) -> bool {
    for y in 0..height {
        for x in 0..width {
            if read_pixel(bus, x, y) != 0 {
                return true;
            }
        }
    }
    false
}

/// Write GPSET parameters: X(word), Y(word), palette(byte).
fn write_gpset_params(bus: &mut impl Bus, base: u32, x: u16, y: u16, palette: u8) {
    write_bytes(
        bus,
        base,
        &[
            (x & 0xFF) as u8,
            (x >> 8) as u8,
            (y & 0xFF) as u8,
            (y >> 8) as u8,
            palette,
        ],
    );
}

/// Write GLINE parameters: X1, Y1, X2, Y2, palette, draw_code, style_switch, style.
#[allow(clippy::too_many_arguments)]
fn write_gline_params(
    bus: &mut impl Bus,
    base: u32,
    x1: u16,
    y1: u16,
    x2: u16,
    y2: u16,
    palette: u8,
    draw_code: u8,
    style_switch: u8,
    style: u16,
) {
    write_bytes(
        bus,
        base,
        &[
            (x1 & 0xFF) as u8,
            (x1 >> 8) as u8,
            (y1 & 0xFF) as u8,
            (y1 >> 8) as u8,
            (x2 & 0xFF) as u8,
            (x2 >> 8) as u8,
            (y2 & 0xFF) as u8,
            (y2 >> 8) as u8,
            palette,
            draw_code,
            style_switch,
            (style & 0xFF) as u8,
            (style >> 8) as u8,
        ],
    );
}

/// Write GVIEW parameters: X1, Y1, X2, Y2, fill_color, border_color.
#[allow(clippy::too_many_arguments)]
fn write_gview_params(
    bus: &mut impl Bus,
    base: u32,
    x1: u16,
    y1: u16,
    x2: u16,
    y2: u16,
    fill_color: u8,
    border_color: u8,
) {
    write_bytes(
        bus,
        base,
        &[
            (x1 & 0xFF) as u8,
            (x1 >> 8) as u8,
            (y1 & 0xFF) as u8,
            (y1 >> 8) as u8,
            (x2 & 0xFF) as u8,
            (x2 >> 8) as u8,
            (y2 & 0xFF) as u8,
            (y2 >> 8) as u8,
            fill_color,
            border_color,
        ],
    );
}

/// Write GCIRCLE parameters: CX, CY, RX, RY, palette, flags (+ start/end for arcs).
#[allow(clippy::too_many_arguments)]
fn write_gcircle_params(
    bus: &mut impl Bus,
    base: u32,
    cx: u16,
    cy: u16,
    rx: u16,
    ry: u16,
    palette: u8,
    flags: u8,
) {
    write_bytes(
        bus,
        base,
        &[
            (cx & 0xFF) as u8,
            (cx >> 8) as u8,
            (cy & 0xFF) as u8,
            (cy >> 8) as u8,
            (rx & 0xFF) as u8,
            (rx >> 8) as u8,
            (ry & 0xFF) as u8,
            (ry >> 8) as u8,
            palette,
            flags,
            // SX, SY, EX, EY = 0 (full circle)
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
    );
}

/// Write GPAINT1 parameters: X, Y, fill_pal, boundary_pal, work_end, work_start.
#[allow(clippy::too_many_arguments)]
fn write_gpaint1_params(
    bus: &mut impl Bus,
    base: u32,
    x: u16,
    y: u16,
    fill_pal: u8,
    boundary_pal: u8,
    work_end: u16,
    work_start: u16,
) {
    write_bytes(
        bus,
        base,
        &[
            (x & 0xFF) as u8,
            (x >> 8) as u8,
            (y & 0xFF) as u8,
            (y >> 8) as u8,
            fill_pal,
            boundary_pal,
            (work_end & 0xFF) as u8,
            (work_end >> 8) as u8,
            (work_start & 0xFF) as u8,
            (work_start >> 8) as u8,
        ],
    );
}

/// Write GGET parameters: X1, Y1, X2, Y2, storage_offset, storage_segment, storage_length.
#[allow(clippy::too_many_arguments)]
fn write_gget_params(
    bus: &mut impl Bus,
    base: u32,
    x1: u16,
    y1: u16,
    x2: u16,
    y2: u16,
    storage_offset: u16,
    storage_segment: u16,
    storage_length: u16,
) {
    write_bytes(
        bus,
        base,
        &[
            (x1 & 0xFF) as u8,
            (x1 >> 8) as u8,
            (y1 & 0xFF) as u8,
            (y1 >> 8) as u8,
            (x2 & 0xFF) as u8,
            (x2 >> 8) as u8,
            (y2 & 0xFF) as u8,
            (y2 >> 8) as u8,
            (storage_offset & 0xFF) as u8,
            (storage_offset >> 8) as u8,
            (storage_segment & 0xFF) as u8,
            (storage_segment >> 8) as u8,
            (storage_length & 0xFF) as u8,
            (storage_length >> 8) as u8,
        ],
    );
}

/// Write GPUT1 parameters: X, Y, storage_offset, storage_segment, storage_length,
/// draw_mode, color_switch, fg_color, bg_color.
#[allow(clippy::too_many_arguments)]
fn write_gput1_params(
    bus: &mut impl Bus,
    base: u32,
    x: u16,
    y: u16,
    storage_offset: u16,
    storage_segment: u16,
    storage_length: u16,
    draw_mode: u8,
    color_switch: u8,
    fg_color: u8,
    bg_color: u8,
) {
    write_bytes(
        bus,
        base,
        &[
            (x & 0xFF) as u8,
            (x >> 8) as u8,
            (y & 0xFF) as u8,
            (y >> 8) as u8,
            (storage_offset & 0xFF) as u8,
            (storage_offset >> 8) as u8,
            (storage_segment & 0xFF) as u8,
            (storage_segment >> 8) as u8,
            (storage_length & 0xFF) as u8,
            (storage_length >> 8) as u8,
            draw_mode,
            color_switch,
            fg_color,
            bg_color,
        ],
    );
}

fn write_gpaint2_params(bus: &mut impl Bus, tile_addr: u16, work_start: u16, work_end: u16) {
    write_bytes(bus, BUFFER, &[0xFF, 0xFF, 0xFF]);
    #[rustfmt::skip]
    let params: Vec<u8> = vec![
        0x40, 0x01,             // +0  X=320
        0x64, 0x00,             // +2  Y=100
        0x00,                   // +4  unused
        0x03,                   // +5  tile_len=3 (must be multiple of 3)
        (tile_addr & 0xFF) as u8, (tile_addr >> 8) as u8,  // +6  tile offset
        0x00, 0x00,             // +8  tile segment
        0x07,                   // +10 boundary_color
        0x00, 0x00, 0x00, 0x00, 0x00, // +11-15 padding
        (work_end & 0xFF) as u8, (work_end >> 8) as u8,    // +16 work_end
        (work_start & 0xFF) as u8, (work_start >> 8) as u8, // +18 work_start
    ];
    write_bytes(bus, PARAMS, &params);
}

// ============================================================================
// LIO ROM & IVT Vector Setup
// ============================================================================

#[test]
fn lio_rom_present_vm() {
    let mut machine = boot_lio_vm();
    assert_ne!(
        machine.bus.read_byte(0xF9900),
        0x00,
        "LIO ROM should be present at F9900h"
    );
}

#[test]
fn lio_rom_present_vx() {
    let mut machine = boot_lio_vx();
    assert_ne!(
        machine.bus.read_byte(0xF9900),
        0x00,
        "LIO ROM should be present at F9900h"
    );
}

#[test]
fn lio_rom_present_ra() {
    let mut machine = boot_lio_ra();
    assert_ne!(
        machine.bus.read_byte(0xF9900),
        0x00,
        "LIO ROM should be present at F9900h"
    );
}

#[test]
fn lio_ivt_vectors_set_vm() {
    let machine = boot_lio_vm();
    let state = machine.save_state();
    let (segment, _offset) = read_ivt_vector(&state.memory.ram, 0xA0);
    assert_eq!(
        segment, 0xF990,
        "INT A0h (GINIT) should point to LIO ROM segment F990h (got {segment:#06X})"
    );
}

#[test]
fn lio_ivt_vectors_set_vx() {
    let machine = boot_lio_vx();
    let state = machine.save_state();
    let (segment, _offset) = read_ivt_vector(&state.memory.ram, 0xA0);
    assert_eq!(
        segment, 0xF990,
        "INT A0h (GINIT) should point to LIO ROM segment F990h (got {segment:#06X})"
    );
}

#[test]
fn lio_ivt_vectors_set_ra() {
    let machine = boot_lio_ra();
    let state = machine.save_state();
    let (segment, _offset) = read_ivt_vector(&state.memory.ram, 0xA0);
    assert_eq!(
        segment, 0xF990,
        "INT A0h (GINIT) should point to LIO ROM segment F990h (got {segment:#06X})"
    );
}

// ============================================================================
// GINIT (INT 0xA0) — databook p.142
// ============================================================================

#[test]
fn ginit_returns_success_vm() {
    let mut machine = boot_lio_vm();
    run_lio_vm(&mut machine, &make_ginit_call());
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GINIT should return AH=00h (success)");
}

#[test]
fn ginit_returns_success_vx() {
    let mut machine = boot_lio_vx();
    run_lio_vx(&mut machine, &make_ginit_call());
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GINIT should return AH=00h (success)");
}

#[test]
fn ginit_returns_success_ra() {
    let mut machine = boot_lio_ra();
    run_lio_ra(&mut machine, &make_ginit_call());
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GINIT should return AH=00h (success)");
}

fn assert_ginit_work_area_defaults(state: &machine::MachineState) {
    assert_eq!(
        state.memory.ram[LIO_FGCOLOR], 7,
        "GINIT fgcolor should be 7"
    );
    assert_eq!(
        state.memory.ram[LIO_BGCOLOR], 0,
        "GINIT bgcolor should be 0"
    );
    assert_eq!(
        &state.memory.ram[LIO_PALETTE..LIO_PALETTE + 8],
        &[0, 1, 2, 3, 4, 5, 6, 7],
        "GINIT palette should be identity"
    );
    assert_eq!(
        read_ram_u16(&state.memory.ram, LIO_VIEWX2),
        639,
        "GINIT viewx2 should be 639"
    );
    assert_eq!(
        read_ram_u16(&state.memory.ram, LIO_VIEWY2),
        199,
        "GINIT viewy2 should be 199"
    );
}

#[test]
fn ginit_work_area_defaults_vm() {
    let mut machine = boot_lio_vm();
    run_lio_vm(&mut machine, &make_ginit_call());
    assert_ginit_work_area_defaults(&machine.save_state());
}

#[test]
fn ginit_work_area_defaults_vx() {
    let mut machine = boot_lio_vx();
    run_lio_vx(&mut machine, &make_ginit_call());
    assert_ginit_work_area_defaults(&machine.save_state());
}

#[test]
fn ginit_work_area_defaults_ra() {
    let mut machine = boot_lio_ra();
    run_lio_ra(&mut machine, &make_ginit_call());
    assert_ginit_work_area_defaults(&machine.save_state());
}

#[test]
fn ginit_enables_graphics_vm() {
    let mut machine = boot_lio_vm();
    run_lio_vm(&mut machine, &make_gstop_then_ginit());
    assert!(
        machine.save_state().gdc_slave.display_enabled,
        "GINIT should enable graphics display"
    );
}

#[test]
fn ginit_enables_graphics_vx() {
    let mut machine = boot_lio_vx();
    run_lio_vx(&mut machine, &make_gstop_then_ginit());
    assert!(
        machine.save_state().gdc_slave.display_enabled,
        "GINIT should enable graphics display"
    );
}

#[test]
fn ginit_enables_graphics_ra() {
    let mut machine = boot_lio_ra();
    run_lio_ra(&mut machine, &make_gstop_then_ginit());
    assert!(
        machine.save_state().gdc_slave.display_enabled,
        "GINIT should enable graphics display"
    );
}

// ============================================================================
// GSCREEN (INT 0xA1) — databook p.144
// ============================================================================

#[test]
fn gscreen_set_mode3_vm() {
    // mode=3, switch=FF, active=FF, display=FF
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0x03, 0xFF, 0xFF, 0xFF]);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA1));
    assert_eq!(
        machine.save_state().memory.ram[LIO_SCRNMODE],
        3,
        "GSCREEN mode=3 should set work area"
    );
}

#[test]
fn gscreen_set_mode3_vx() {
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0x03, 0xFF, 0xFF, 0xFF]);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA1));
    assert_eq!(
        machine.save_state().memory.ram[LIO_SCRNMODE],
        3,
        "GSCREEN mode=3 should set work area"
    );
}

#[test]
fn gscreen_set_mode3_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0x03, 0xFF, 0xFF, 0xFF]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA1));
    assert_eq!(
        machine.save_state().memory.ram[LIO_SCRNMODE],
        3,
        "GSCREEN mode=3 should set work area"
    );
}

#[test]
fn gscreen_display_hide_vm() {
    // mode=FF, switch=02 (hide), active=FF, display=FF
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0xFF, 0x02, 0xFF, 0xFF]);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA1));
    assert!(
        !machine.save_state().gdc_slave.display_enabled,
        "GSCREEN switch=02 should hide graphics"
    );
}

#[test]
fn gscreen_display_hide_vx() {
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0xFF, 0x02, 0xFF, 0xFF]);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA1));
    assert!(
        !machine.save_state().gdc_slave.display_enabled,
        "GSCREEN switch=02 should hide graphics"
    );
}

#[test]
fn gscreen_display_hide_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0xFF, 0x02, 0xFF, 0xFF]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA1));
    assert!(
        !machine.save_state().gdc_slave.display_enabled,
        "GSCREEN switch=02 should hide graphics"
    );
}

// ============================================================================
// GVIEW (INT 0xA2) — databook p.149
// ============================================================================

fn assert_gview_viewport(state: &machine::MachineState) {
    assert_eq!(read_ram_u16(&state.memory.ram, LIO_VIEWX1), 100);
    assert_eq!(read_ram_u16(&state.memory.ram, LIO_VIEWY1), 50);
    assert_eq!(read_ram_u16(&state.memory.ram, LIO_VIEWX2), 500);
    assert_eq!(read_ram_u16(&state.memory.ram, LIO_VIEWY2), 150);
}

#[test]
fn gview_sets_viewport_vm() {
    let mut machine = boot_lio_vm();
    write_gview_params(&mut machine.bus, PARAMS, 100, 50, 500, 150, 0xFF, 0xFF);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA2));
    assert_gview_viewport(&machine.save_state());
}

#[test]
fn gview_sets_viewport_vx() {
    let mut machine = boot_lio_vx();
    write_gview_params(&mut machine.bus, PARAMS, 100, 50, 500, 150, 0xFF, 0xFF);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA2));
    assert_gview_viewport(&machine.save_state());
}

#[test]
fn gview_sets_viewport_ra() {
    let mut machine = boot_lio_ra();
    write_gview_params(&mut machine.bus, PARAMS, 100, 50, 500, 150, 0xFF, 0xFF);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA2));
    assert_gview_viewport(&machine.save_state());
}

#[test]
fn gview_invalid_returns_error_vm() {
    // x1=500 > x2=100 → should return AH=05h
    let mut machine = boot_lio_vm();
    write_gview_params(&mut machine.bus, PARAMS, 500, 0, 100, 199, 0xFF, 0xFF);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA2));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(
        ah, 0x05,
        "GVIEW with x1>x2 should return AH=05h (illegal call)"
    );
}

#[test]
fn gview_invalid_returns_error_vx() {
    let mut machine = boot_lio_vx();
    write_gview_params(&mut machine.bus, PARAMS, 500, 0, 100, 199, 0xFF, 0xFF);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA2));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(
        ah, 0x05,
        "GVIEW with x1>x2 should return AH=05h (illegal call)"
    );
}

#[test]
fn gview_invalid_returns_error_ra() {
    let mut machine = boot_lio_ra();
    write_gview_params(&mut machine.bus, PARAMS, 500, 0, 100, 199, 0xFF, 0xFF);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA2));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(
        ah, 0x05,
        "GVIEW with x1>x2 should return AH=05h (illegal call)"
    );
}

#[test]
fn gview_fill_color_vm() {
    let mut machine = boot_lio_vm();
    // fill=3 (palette 3 = B+R), border=FF (no border)
    write_gview_params(&mut machine.bus, PARAMS, 0, 0, 639, 199, 3, 0xFF);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA2));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GVIEW fill should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 320, 100),
        0,
        "GVIEW fill=3 should write non-zero pixels inside viewport"
    );
}

#[test]
fn gview_fill_color_vx() {
    let mut machine = boot_lio_vx();
    write_gview_params(&mut machine.bus, PARAMS, 0, 0, 639, 199, 3, 0xFF);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA2));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GVIEW fill should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 320, 100),
        0,
        "GVIEW fill=3 should write non-zero pixels inside viewport"
    );
}

#[test]
fn gview_fill_color_ra() {
    let mut machine = boot_lio_ra();
    write_gview_params(&mut machine.bus, PARAMS, 0, 0, 639, 199, 3, 0xFF);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA2));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GVIEW fill should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 320, 100),
        0,
        "GVIEW fill=3 should write non-zero pixels inside viewport"
    );
}

// ============================================================================
// GCOLOR1 (INT 0xA3) — databook p.151
// ============================================================================

#[test]
fn gcolor1_sets_fgbg_vm() {
    // unused=0, bg=3, bd=FF, fg=5, palmode=FF
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0x03, 0xFF, 0x05, 0xFF]);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA3));
    let state = machine.save_state();
    assert_eq!(
        state.memory.ram[LIO_FGCOLOR], 5,
        "GCOLOR1 should set fgcolor=5"
    );
    assert_eq!(
        state.memory.ram[LIO_BGCOLOR], 3,
        "GCOLOR1 should set bgcolor=3"
    );
}

#[test]
fn gcolor1_sets_fgbg_vx() {
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0x03, 0xFF, 0x05, 0xFF]);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA3));
    let state = machine.save_state();
    assert_eq!(
        state.memory.ram[LIO_FGCOLOR], 5,
        "GCOLOR1 should set fgcolor=5"
    );
    assert_eq!(
        state.memory.ram[LIO_BGCOLOR], 3,
        "GCOLOR1 should set bgcolor=3"
    );
}

#[test]
fn gcolor1_sets_fgbg_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0x03, 0xFF, 0x05, 0xFF]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA3));
    let state = machine.save_state();
    assert_eq!(
        state.memory.ram[LIO_FGCOLOR], 5,
        "GCOLOR1 should set fgcolor=5"
    );
    assert_eq!(
        state.memory.ram[LIO_BGCOLOR], 3,
        "GCOLOR1 should set bgcolor=3"
    );
}

#[test]
fn gcolor1_palmode_digital_vm() {
    // VM BIOS GCOLOR1 doesn't handle palette mode (databook p.151: palette mode
    // is only needed in extended graphics mode, which VM doesn't support per p.136).
    // Verify that palmode is unchanged after GCOLOR1 call.
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0xFF, 0xFF, 0xFF, 0x00]);
    let palmode_before = machine.bus.read_byte(LIO_PALMODE as u32);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA3));
    assert_eq!(
        machine.save_state().memory.ram[LIO_PALMODE],
        palmode_before,
        "VM GCOLOR1 should not modify palmode (no extended graphics support)"
    );
}

#[test]
fn gcolor1_palmode_digital_vx() {
    // VX BIOS GCOLOR1 doesn't handle palette mode either (same as VM).
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0xFF, 0xFF, 0xFF, 0x00]);
    let palmode_before = machine.bus.read_byte(LIO_PALMODE as u32);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA3));
    assert_eq!(
        machine.save_state().memory.ram[LIO_PALMODE],
        palmode_before,
        "VX GCOLOR1 should not modify palmode (no extended graphics support)"
    );
}

#[test]
fn gcolor1_palmode_digital_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0xFF, 0xFF, 0xFF, 0x00]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA3));
    assert_eq!(
        machine.save_state().memory.ram[LIO_PALMODE],
        0,
        "GCOLOR1 palmode=0 should set digital mode"
    );
}

#[test]
fn gcolor1_bdcolor_vm() {
    // unused=0, bg=FF, bd=2, fg=FF, palmode=FF
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0xFF, 0x02, 0xFF, 0xFF]);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA3));
    // BIOS computes border_color << 4 and stores to [0x641].
    assert_eq!(
        machine.save_state().memory.ram[0x641],
        0x20,
        "GCOLOR1 should set border color value to 0x20 (palette 2 << 4)"
    );
}

#[test]
fn gcolor1_bdcolor_vx() {
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0xFF, 0x02, 0xFF, 0xFF]);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA3));
    assert_eq!(
        machine.save_state().memory.ram[0x641],
        0x20,
        "GCOLOR1 should set border color value to 0x20 (palette 2 << 4)"
    );
}

#[test]
fn gcolor1_bdcolor_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0xFF, 0x02, 0xFF, 0xFF]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA3));
    assert_eq!(
        machine.save_state().memory.ram[0x641],
        0x20,
        "GCOLOR1 should set border color value to 0x20 (palette 2 << 4)"
    );
}

// ============================================================================
// GCOLOR2 (INT 0xA4) — databook p.152
// ============================================================================

#[test]
fn gcolor2_digital_palette_vm() {
    // pal=1, color_code=5
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0x01, 0x05]);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA4));
    assert_eq!(
        machine.save_state().memory.ram[LIO_PALETTE + 1],
        5,
        "GCOLOR2 should set palette[1]=5"
    );
}

#[test]
fn gcolor2_digital_palette_vx() {
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0x01, 0x05]);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA4));
    assert_eq!(
        machine.save_state().memory.ram[LIO_PALETTE + 1],
        5,
        "GCOLOR2 should set palette[1]=5"
    );
}

#[test]
fn gcolor2_digital_palette_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0x01, 0x05]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA4));
    assert_eq!(
        machine.save_state().memory.ram[LIO_PALETTE + 1],
        5,
        "GCOLOR2 should set palette[1]=5"
    );
}

// ============================================================================
// GCLS (INT 0xA5) — databook p.153
// ============================================================================

#[test]
fn gcls_clears_viewport_vm() {
    // GINIT → GPSET pixel at (100,100) pal=7 → GCLS → pixel should be 0
    let mut machine = boot_lio_vm();
    write_gpset_params(&mut machine.bus, PARAMS, 100, 100, 7);
    run_lio_vm(&mut machine, &make_ginit_gpset_then_lio(0xA5));
    assert_eq!(
        read_pixel(&mut machine.bus, 100, 100),
        0,
        "GCLS should clear pixel at (100,100) to bgcolor=0"
    );
}

#[test]
fn gcls_clears_viewport_vx() {
    let mut machine = boot_lio_vx();
    write_gpset_params(&mut machine.bus, PARAMS, 100, 100, 7);
    run_lio_vx(&mut machine, &make_ginit_gpset_then_lio(0xA5));
    assert_eq!(
        read_pixel(&mut machine.bus, 100, 100),
        0,
        "GCLS should clear pixel at (100,100) to bgcolor=0"
    );
}

#[test]
fn gcls_clears_viewport_ra() {
    let mut machine = boot_lio_ra();
    write_gpset_params(&mut machine.bus, PARAMS, 100, 100, 7);
    run_lio_ra(&mut machine, &make_ginit_gpset_then_lio(0xA5));
    assert_eq!(
        read_pixel(&mut machine.bus, 100, 100),
        0,
        "GCLS should clear pixel at (100,100) to bgcolor=0"
    );
}

// ============================================================================
// GPSET (INT 0xA6) — databook p.153
// ============================================================================

#[test]
fn gpset_sets_pixel_vm() {
    let mut machine = boot_lio_vm();
    write_gpset_params(&mut machine.bus, PARAMS, 200, 150, 5);
    run_lio_vm(&mut machine, &make_ginit_then_gpset(0x01));
    assert_eq!(
        read_pixel(&mut machine.bus, 200, 150),
        5,
        "GPSET should set pixel at (200,150) to palette 5"
    );
}

#[test]
fn gpset_sets_pixel_vx() {
    let mut machine = boot_lio_vx();
    write_gpset_params(&mut machine.bus, PARAMS, 200, 150, 5);
    run_lio_vx(&mut machine, &make_ginit_then_gpset(0x01));
    assert_eq!(
        read_pixel(&mut machine.bus, 200, 150),
        5,
        "GPSET should set pixel at (200,150) to palette 5"
    );
}

#[test]
fn gpset_sets_pixel_ra() {
    let mut machine = boot_lio_ra();
    write_gpset_params(&mut machine.bus, PARAMS, 200, 150, 5);
    run_lio_ra(&mut machine, &make_ginit_then_gpset(0x01));
    assert_eq!(
        read_pixel(&mut machine.bus, 200, 150),
        5,
        "GPSET should set pixel at (200,150) to palette 5"
    );
}

// ============================================================================
// GLINE (INT 0xA7) — databook p.154
// ============================================================================

#[test]
fn gline_horizontal_vm() {
    let mut machine = boot_lio_vm();
    write_gline_params(&mut machine.bus, PARAMS, 0, 0, 79, 0, 7, 0x00, 0x00, 0xFFFF);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA7));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GLINE should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 0, 0),
        0,
        "GLINE start pixel should be drawn"
    );
    assert_ne!(
        read_pixel(&mut machine.bus, 79, 0),
        0,
        "GLINE end pixel should be drawn"
    );
}

#[test]
fn gline_horizontal_vx() {
    let mut machine = boot_lio_vx();
    write_gline_params(&mut machine.bus, PARAMS, 0, 0, 79, 0, 7, 0x00, 0x00, 0xFFFF);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA7));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GLINE should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 0, 0),
        0,
        "GLINE start pixel should be drawn"
    );
    assert_ne!(
        read_pixel(&mut machine.bus, 79, 0),
        0,
        "GLINE end pixel should be drawn"
    );
}

#[test]
fn gline_horizontal_ra() {
    let mut machine = boot_lio_ra();
    write_gline_params(&mut machine.bus, PARAMS, 0, 0, 79, 0, 7, 0x00, 0x00, 0xFFFF);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA7));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GLINE should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 0, 0),
        0,
        "GLINE start pixel should be drawn"
    );
    assert_ne!(
        read_pixel(&mut machine.bus, 79, 0),
        0,
        "GLINE end pixel should be drawn"
    );
}

fn assert_gline_box_outline(state: &machine::MachineState) {
    let ah = state.memory.ram[RESULT as usize + 1];
    assert_eq!(ah, 0x00, "GLINE box outline should return success");
    assert_ne!(
        read_pixel_from_state(state, 30, 10),
        0,
        "Top edge midpoint should be drawn"
    );
    assert_ne!(
        read_pixel_from_state(state, 30, 50),
        0,
        "Bottom edge midpoint should be drawn"
    );
    assert_ne!(
        read_pixel_from_state(state, 10, 30),
        0,
        "Left edge midpoint should be drawn"
    );
    assert_ne!(
        read_pixel_from_state(state, 50, 30),
        0,
        "Right edge midpoint should be drawn"
    );
    assert_eq!(
        read_pixel_from_state(state, 30, 30),
        0,
        "Box interior should be empty"
    );
}

#[test]
fn gline_box_outline_vm() {
    let mut machine = boot_lio_vm();
    write_gline_params(
        &mut machine.bus,
        PARAMS,
        10,
        10,
        50,
        50,
        7,
        0x01,
        0x00,
        0xFFFF,
    );
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA7));
    assert_gline_box_outline(&machine.save_state());
}

#[test]
fn gline_box_outline_vx() {
    let mut machine = boot_lio_vx();
    write_gline_params(
        &mut machine.bus,
        PARAMS,
        10,
        10,
        50,
        50,
        7,
        0x01,
        0x00,
        0xFFFF,
    );
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA7));
    assert_gline_box_outline(&machine.save_state());
}

#[test]
fn gline_box_outline_ra() {
    let mut machine = boot_lio_ra();
    write_gline_params(
        &mut machine.bus,
        PARAMS,
        10,
        10,
        50,
        50,
        7,
        0x01,
        0x00,
        0xFFFF,
    );
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA7));
    assert_gline_box_outline(&machine.save_state());
}

#[test]
fn gline_filled_box_vm() {
    let mut machine = boot_lio_vm();
    write_gline_params(
        &mut machine.bus,
        PARAMS,
        10,
        10,
        50,
        50,
        7,
        0x02,
        0x00,
        0xFFFF,
    );
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA7));
    assert_ne!(
        read_pixel(&mut machine.bus, 30, 30),
        0,
        "Filled box interior should be drawn"
    );
}

#[test]
fn gline_filled_box_vx() {
    let mut machine = boot_lio_vx();
    write_gline_params(
        &mut machine.bus,
        PARAMS,
        10,
        10,
        50,
        50,
        7,
        0x02,
        0x00,
        0xFFFF,
    );
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA7));
    assert_ne!(
        read_pixel(&mut machine.bus, 30, 30),
        0,
        "Filled box interior should be drawn"
    );
}

#[test]
fn gline_filled_box_ra() {
    let mut machine = boot_lio_ra();
    write_gline_params(
        &mut machine.bus,
        PARAMS,
        10,
        10,
        50,
        50,
        7,
        0x02,
        0x00,
        0xFFFF,
    );
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA7));
    assert_ne!(
        read_pixel(&mut machine.bus, 30, 30),
        0,
        "Filled box interior should be drawn"
    );
}

// ============================================================================
// GCIRCLE (INT 0xA8) — databook p.156-157
// ============================================================================

#[test]
fn gcircle_basic_vm() {
    let mut machine = boot_lio_vm();
    write_gcircle_params(&mut machine.bus, PARAMS, 320, 100, 50, 50, 7, 0x00);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA8));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GCIRCLE should return AH=00h");
    // Check rightmost point of circle at (370, 100)
    assert_ne!(
        read_pixel(&mut machine.bus, 370, 100),
        0,
        "Circle right edge should be drawn"
    );
}

#[test]
fn gcircle_basic_vx() {
    let mut machine = boot_lio_vx();
    write_gcircle_params(&mut machine.bus, PARAMS, 320, 100, 50, 50, 7, 0x00);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA8));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GCIRCLE should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 370, 100),
        0,
        "Circle right edge should be drawn"
    );
}

#[test]
fn gcircle_basic_ra() {
    let mut machine = boot_lio_ra();
    write_gcircle_params(&mut machine.bus, PARAMS, 320, 100, 50, 50, 7, 0x00);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA8));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GCIRCLE should return AH=00h");
    assert_ne!(
        read_pixel(&mut machine.bus, 370, 100),
        0,
        "Circle right edge should be drawn"
    );
}

// ============================================================================
// GPAINT1 (INT 0xA9) — databook p.159
// ============================================================================

#[test]
fn gpaint1_returns_success_vm() {
    let mut machine = boot_lio_vm();
    let work_start = WORK_AREA as u16;
    let work_end = work_start + 512;
    write_gpaint1_params(
        &mut machine.bus,
        PARAMS,
        320,
        100,
        7,
        7,
        work_end,
        work_start,
    );
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xA9));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GPAINT1 should return AH=00h (success)");
}

#[test]
fn gpaint1_returns_success_vx() {
    let mut machine = boot_lio_vx();
    let work_start = WORK_AREA as u16;
    let work_end = work_start + 512;
    write_gpaint1_params(
        &mut machine.bus,
        PARAMS,
        320,
        100,
        7,
        7,
        work_end,
        work_start,
    );
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xA9));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GPAINT1 should return AH=00h (success)");
}

#[test]
fn gpaint1_returns_success_ra() {
    let mut machine = boot_lio_ra();
    let work_start = WORK_AREA as u16;
    let work_end = work_start + 512;
    write_gpaint1_params(
        &mut machine.bus,
        PARAMS,
        320,
        100,
        7,
        7,
        work_end,
        work_start,
    );
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xA9));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GPAINT1 should return AH=00h (success)");
}

fn setup_gpaint1_flood_fill(bus: &mut impl Bus) {
    write_gline_params(bus, PARAMS, 10, 10, 50, 50, 7, 0x01, 0x00, 0xFFFF);
    let work_start = WORK_AREA as u16;
    let work_end = work_start + 512;
    write_gpaint1_params(bus, PARAMS2, 30, 30, 5, 7, work_end, work_start);
}

#[test]
fn gpaint1_flood_fill_vm() {
    // GINIT → GLINE box outline → GPAINT1 inside box
    let mut machine = boot_lio_vm();
    setup_gpaint1_flood_fill(&mut machine.bus);
    run_lio_vm(&mut machine, &make_ginit_gline_then_lio(0xA9));
    assert_ne!(
        read_pixel(&mut machine.bus, 30, 30),
        0,
        "GPAINT1 should fill interior of box outline"
    );
}

#[test]
fn gpaint1_flood_fill_vx() {
    let mut machine = boot_lio_vx();
    setup_gpaint1_flood_fill(&mut machine.bus);
    run_lio_vx(&mut machine, &make_ginit_gline_then_lio(0xA9));
    assert_ne!(
        read_pixel(&mut machine.bus, 30, 30),
        0,
        "GPAINT1 should fill interior of box outline"
    );
}

#[test]
fn gpaint1_flood_fill_ra() {
    let mut machine = boot_lio_ra();
    setup_gpaint1_flood_fill(&mut machine.bus);
    run_lio_ra(&mut machine, &make_ginit_gline_then_lio(0xA9));
    assert_ne!(
        read_pixel(&mut machine.bus, 30, 30),
        0,
        "GPAINT1 should fill interior of box outline"
    );
}

// ============================================================================
// GPAINT2 (INT 0xAA) — databook p.160-161
// ============================================================================

#[test]
fn gpaint2_returns_success_vm() {
    let mut machine = boot_lio_vm();
    write_gpaint2_params(
        &mut machine.bus,
        BUFFER as u16,
        WORK_AREA as u16,
        WORK_AREA as u16 + 512,
    );
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xAA));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert!(
        ah == 0x00 || ah == 0x07,
        "GPAINT2 should return AH=00h or 07h (got {ah:#04X})"
    );
}

#[test]
fn gpaint2_returns_success_vx() {
    let mut machine = boot_lio_vx();
    write_gpaint2_params(
        &mut machine.bus,
        BUFFER as u16,
        WORK_AREA as u16,
        WORK_AREA as u16 + 512,
    );
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xAA));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert!(
        ah == 0x00 || ah == 0x07,
        "GPAINT2 should return AH=00h or 07h (got {ah:#04X})"
    );
}

#[test]
fn gpaint2_returns_success_ra() {
    let mut machine = boot_lio_ra();
    write_gpaint2_params(
        &mut machine.bus,
        BUFFER as u16,
        WORK_AREA as u16,
        WORK_AREA as u16 + 512,
    );
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xAA));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert!(
        ah == 0x00 || ah == 0x07,
        "GPAINT2 should return AH=00h or 07h (got {ah:#04X})"
    );
}

// ============================================================================
// GGET (INT 0xAB) — databook p.164-165
// ============================================================================

fn setup_gget_params(bus: &mut impl Bus) {
    write_gpset_params(bus, PARAMS, 0, 0, 7);
    write_gget_params(bus, PARAMS2, 0, 0, 7, 7, BUFFER as u16, 0, 256);
}

#[test]
fn gget_captures_region_vm() {
    let mut machine = boot_lio_vm();
    setup_gget_params(&mut machine.bus);
    run_lio_vm(&mut machine, &make_ginit_gpset_gget());
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GGET should return AH=00h");
    assert_eq!(
        machine.bus.read_word(BUFFER),
        8,
        "GGET buffer header width should be 8"
    );
    assert_eq!(
        machine.bus.read_word(BUFFER + 2),
        8,
        "GGET buffer header height should be 8"
    );
}

#[test]
fn gget_captures_region_vx() {
    let mut machine = boot_lio_vx();
    setup_gget_params(&mut machine.bus);
    run_lio_vx(&mut machine, &make_ginit_gpset_gget());
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GGET should return AH=00h");
    assert_eq!(
        machine.bus.read_word(BUFFER),
        8,
        "GGET buffer header width should be 8"
    );
    assert_eq!(
        machine.bus.read_word(BUFFER + 2),
        8,
        "GGET buffer header height should be 8"
    );
}

#[test]
fn gget_captures_region_ra() {
    let mut machine = boot_lio_ra();
    setup_gget_params(&mut machine.bus);
    run_lio_ra(&mut machine, &make_ginit_gpset_gget());
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GGET should return AH=00h");
    assert_eq!(
        machine.bus.read_word(BUFFER),
        8,
        "GGET buffer header width should be 8"
    );
    assert_eq!(
        machine.bus.read_word(BUFFER + 2),
        8,
        "GGET buffer header height should be 8"
    );
}

// ============================================================================
// GPUT1 (INT 0xAC) — databook p.169-170
// ============================================================================

fn setup_gput1_params(bus: &mut impl Bus) {
    write_gpset_params(bus, PARAMS, 0, 0, 7);
    write_gget_params(bus, PARAMS2, 0, 0, 7, 7, BUFFER as u16, 0, 256);
    write_gput1_params(
        bus,
        PARAMS3,
        100,
        100,
        BUFFER as u16,
        0,
        256,
        0x00,
        0x00,
        7,
        0,
    );
}

#[test]
fn gput1_writes_region_vm() {
    let mut machine = boot_lio_vm();
    setup_gput1_params(&mut machine.bus);
    run_lio_vm(&mut machine, &make_ginit_gpset_gget_gput1());
    assert_ne!(
        read_pixel_from_state(&machine.save_state(), 100, 100),
        0,
        "GPUT1 should write captured region at (100,100)"
    );
}

#[test]
fn gput1_writes_region_vx() {
    let mut machine = boot_lio_vx();
    setup_gput1_params(&mut machine.bus);
    run_lio_vx(&mut machine, &make_ginit_gpset_gget_gput1());
    assert_ne!(
        read_pixel_from_state(&machine.save_state(), 100, 100),
        0,
        "GPUT1 should write captured region at (100,100)"
    );
}

#[test]
fn gput1_writes_region_ra() {
    let mut machine = boot_lio_ra();
    setup_gput1_params(&mut machine.bus);
    run_lio_ra(&mut machine, &make_ginit_gpset_gget_gput1());
    assert_ne!(
        read_pixel_from_state(&machine.save_state(), 100, 100),
        0,
        "GPUT1 should write captured region at (100,100)"
    );
}

// ============================================================================
// GPUT2 (INT 0xAD) — databook p.171
// ============================================================================

#[test]
fn gput2_renders_char_vm() {
    let mut machine = boot_lio_vm();
    // X=0, Y=0, JIS=0x0041 ('A'), mode=00 (PSET), switch=01 (color), fg=7, bg=0
    write_bytes(
        &mut machine.bus,
        PARAMS,
        &[0x00, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x01, 0x07, 0x00],
    );
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xAD));
    assert!(
        any_pixel_set(&mut machine.bus, 8, 16),
        "GPUT2 should render character 'A' with non-zero pixels"
    );
}

#[test]
fn gput2_renders_char_vx() {
    let mut machine = boot_lio_vx();
    write_bytes(
        &mut machine.bus,
        PARAMS,
        &[0x00, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x01, 0x07, 0x00],
    );
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xAD));
    assert!(
        any_pixel_set(&mut machine.bus, 8, 16),
        "GPUT2 should render character 'A' with non-zero pixels"
    );
}

#[test]
fn gput2_renders_char_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(
        &mut machine.bus,
        PARAMS,
        &[0x00, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x01, 0x07, 0x00],
    );
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xAD));
    assert!(
        any_pixel_set(&mut machine.bus, 8, 16),
        "GPUT2 should render character 'A' with non-zero pixels"
    );
}

// ============================================================================
// GROLL (INT 0xAE) — databook p.172
// ============================================================================

#[test]
fn groll_returns_success_vm() {
    // VM format: +0 unused(byte), +1 dot_count(word) = 0
    let mut machine = boot_lio_vm();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0x00, 0x00]);
    run_lio_vm(&mut machine, &make_ginit_then_lio(0xAE));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GROLL should return AH=00h");
}

#[test]
fn groll_returns_success_vx() {
    // VX/RA format: +0 dy(word)=0, +2 dx(word)=0, +4 clear_flag(byte)=0
    let mut machine = boot_lio_vx();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0x00, 0x00, 0x00, 0x00]);
    run_lio_vx(&mut machine, &make_ginit_then_lio(0xAE));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GROLL should return AH=00h");
}

#[test]
fn groll_returns_success_ra() {
    let mut machine = boot_lio_ra();
    write_bytes(&mut machine.bus, PARAMS, &[0x00, 0x00, 0x00, 0x00, 0x00]);
    run_lio_ra(&mut machine, &make_ginit_then_lio(0xAE));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GROLL should return AH=00h");
}

#[test]
fn groll_scroll_effect_vm() {
    // GINIT → GPSET at (320,0) → GROLL scroll up by 1
    let mut machine = boot_lio_vm();
    write_gpset_params(&mut machine.bus, PARAMS, 320, 0, 7);
    write_bytes(&mut machine.bus, PARAMS2, &[0x01, 0x00, 0x00, 0x00, 0x00]);
    run_lio_vm(&mut machine, &make_ginit_gpset_then_lio(0xAE));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GROLL scroll should return AH=00h");
}

#[test]
fn groll_scroll_effect_vx() {
    let mut machine = boot_lio_vx();
    write_gpset_params(&mut machine.bus, PARAMS, 320, 0, 7);
    // VX/RA format: dy=1 (up), dx=0, clear=0
    write_bytes(&mut machine.bus, PARAMS2, &[0x01, 0x00, 0x00, 0x00, 0x00]);
    run_lio_vx(&mut machine, &make_ginit_gpset_then_lio(0xAE));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GROLL scroll should return AH=00h");
}

#[test]
fn groll_scroll_effect_ra() {
    let mut machine = boot_lio_ra();
    write_gpset_params(&mut machine.bus, PARAMS, 320, 0, 7);
    write_bytes(&mut machine.bus, PARAMS2, &[0x01, 0x00, 0x00, 0x00, 0x00]);
    run_lio_ra(&mut machine, &make_ginit_gpset_then_lio(0xAE));
    let ah = (machine.bus.read_word(RESULT) >> 8) as u8;
    assert_eq!(ah, 0x00, "GROLL scroll should return AH=00h");
}

// ============================================================================
// GPOINT2 (INT 0xAF) — databook p.173
// ============================================================================

fn setup_gpoint2_params(bus: &mut impl Bus) {
    write_gpset_params(bus, PARAMS, 200, 150, 5);
    // GPOINT2 params: X=200, Y=150
    write_bytes(bus, PARAMS2, &[0xC8, 0x00, 0x96, 0x00]);
}

#[test]
fn gpoint2_reads_pixel_vm() {
    let mut machine = boot_lio_vm();
    setup_gpoint2_params(&mut machine.bus);
    run_lio_vm(&mut machine, &make_ginit_gpset_gpoint2());
    let al = (machine.bus.read_word(RESULT) & 0xFF) as u8;
    assert_eq!(
        al, 5,
        "GPOINT2 should return AL=5 (palette of pixel at 200,150)"
    );
}

#[test]
fn gpoint2_reads_pixel_vx() {
    let mut machine = boot_lio_vx();
    setup_gpoint2_params(&mut machine.bus);
    run_lio_vx(&mut machine, &make_ginit_gpset_gpoint2());
    let al = (machine.bus.read_word(RESULT) & 0xFF) as u8;
    assert_eq!(
        al, 5,
        "GPOINT2 should return AL=5 (palette of pixel at 200,150)"
    );
}

#[test]
fn gpoint2_reads_pixel_ra() {
    let mut machine = boot_lio_ra();
    setup_gpoint2_params(&mut machine.bus);
    run_lio_ra(&mut machine, &make_ginit_gpset_gpoint2());
    let al = (machine.bus.read_word(RESULT) & 0xFF) as u8;
    assert_eq!(
        al, 5,
        "GPOINT2 should return AL=5 (palette of pixel at 200,150)"
    );
}

fn setup_gpoint2_oob_params(bus: &mut impl Bus) {
    write_gview_params(bus, PARAMS, 100, 100, 500, 150, 0xFF, 0xFF);
    // Point outside viewport: (50, 50)
    write_bytes(bus, PARAMS2, &[0x32, 0x00, 0x32, 0x00]);
}

#[test]
fn gpoint2_oob_returns_ff_vm() {
    let mut machine = boot_lio_vm();
    setup_gpoint2_oob_params(&mut machine.bus);
    run_lio_vm(&mut machine, &make_ginit_gview_gpoint2_oob());
    let al = (machine.bus.read_word(RESULT) & 0xFF) as u8;
    assert_eq!(al, 0xFF, "GPOINT2 outside viewport should return AL=FFh");
}

#[test]
fn gpoint2_oob_returns_ff_vx() {
    let mut machine = boot_lio_vx();
    setup_gpoint2_oob_params(&mut machine.bus);
    run_lio_vx(&mut machine, &make_ginit_gview_gpoint2_oob());
    let al = (machine.bus.read_word(RESULT) & 0xFF) as u8;
    assert_eq!(al, 0xFF, "GPOINT2 outside viewport should return AL=FFh");
}

#[test]
fn gpoint2_oob_returns_ff_ra() {
    let mut machine = boot_lio_ra();
    setup_gpoint2_oob_params(&mut machine.bus);
    run_lio_ra(&mut machine, &make_ginit_gview_gpoint2_oob());
    let al = (machine.bus.read_word(RESULT) & 0xFF) as u8;
    assert_eq!(al, 0xFF, "GPOINT2 outside viewport should return AL=FFh");
}

// ============================================================================
// GCOPY (INT 0xCE) — databook p.174
// ============================================================================

#[test]
fn gcopy_returns_success_vm() {
    let mut machine = boot_lio_vm();
    run_lio_vm(&mut machine, &make_ginit_then_gcopy(0, 0, 8, 0x02));
}

#[test]
fn gcopy_returns_success_vx() {
    let mut machine = boot_lio_vx();
    run_lio_vx(&mut machine, &make_ginit_then_gcopy(0, 0, 8, 0x02));
}

#[test]
fn gcopy_returns_success_ra() {
    let mut machine = boot_lio_ra();
    run_lio_ra(&mut machine, &make_ginit_then_gcopy(0, 0, 8, 0x02));
}
