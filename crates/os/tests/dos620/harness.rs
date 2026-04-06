#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use common::{Bus, MachineModel};

/// Mutex to serialize tests that perform disk write operations.
pub static DISK_WRITE_MUTEX: Mutex<()> = Mutex::new(());

static FONT_ROM_DATA: &[u8] = include_bytes!("../../../../utils/font/font.rom");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

const DOS_HDD_IMAGE_NAME: &str = "roms/dos620.hdi";

const MAX_BOOT_CYCLES: u64 = 5_000_000_000;
const BOOT_CHECK_INTERVAL: u64 = 1_000_000;

pub const INJECT_CODE_SEGMENT: u16 = 0x2000;
pub const INJECT_CODE_BASE: u32 = (INJECT_CODE_SEGMENT as u32) << 4;
pub const INJECT_RESULT_OFFSET: u16 = 0x0100;
pub const INJECT_RESULT_BASE: u32 = INJECT_CODE_BASE + INJECT_RESULT_OFFSET as u32;
pub const INJECT_BUDGET: u64 = 50_000_000;
pub const INJECT_BUDGET_DISK_IO: u64 = 500_000_000;

macro_rules! boot_to_dos_prompt {
    ($machine:expr) => {{
        let mut total_cycles = 0u64;
        loop {
            total_cycles += $machine.run_for($crate::harness::BOOT_CHECK_INTERVAL);

            if $crate::harness::dos_prompt_visible(&$machine.bus) {
                break;
            }

            assert!(
                total_cycles < $crate::harness::MAX_BOOT_CYCLES,
                "DOS did not reach prompt within {} cycles",
                $crate::harness::MAX_BOOT_CYCLES
            );
        }
        total_cycles
    }};
}

pub fn dos_prompt_visible(bus: &machine::Pc9801Bus) -> bool {
    let vram = bus.text_vram();
    // PC-98 text VRAM: 2 bytes per character (JIS code, little-endian), 80 columns.
    // Character area: offset 0x0000-0x1FFF, attribute area: 0x2000-0x3FFF.
    // Scan the bottom rows for '>' (0x003E).
    for row in 15..25 {
        for col in 0..80 {
            let offset = (row * 80 + col) * 2;
            if offset + 1 >= vram.len() {
                break;
            }
            let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            if code == 0x003E {
                return true;
            }
        }
    }
    false
}

pub fn create_dos620_machine() -> machine::Pc9801Ra {
    let mut machine = machine::Pc9801Ra::new(
        cpu::I386::new(),
        machine::Pc9801Bus::new(MachineModel::PC9801RA, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    let hdd_path = workspace_root().join(DOS_HDD_IMAGE_NAME);
    let hdd_data = std::fs::read(&hdd_path).unwrap_or_else(|error| {
        panic!(
            "DOS 6.20 HDD image ({}) required for integration tests: {}",
            hdd_path.display(),
            error
        )
    });
    let hdd = device::disk::load_hdd_image(&hdd_path, &hdd_data)
        .expect("valid HDI format for DOS 6.20 image");
    machine.bus.insert_hdd(0, hdd, None);

    machine
}

pub fn boot_dos620() -> machine::Pc9801Ra {
    let mut machine = create_dos620_machine();
    boot_to_dos_prompt!(machine);
    machine
}

pub fn write_bytes(bus: &mut impl Bus, addr: u32, data: &[u8]) {
    for (i, &byte) in data.iter().enumerate() {
        bus.write_byte(addr + i as u32, byte);
    }
}

pub fn inject_and_run(machine: &mut machine::Pc9801Ra, code: &[u8]) {
    inject_and_run_with_budget(machine, code, INJECT_BUDGET);
}

pub fn inject_and_run_with_budget(machine: &mut machine::Pc9801Ra, code: &[u8], budget: u64) {
    write_bytes(&mut machine.bus, INJECT_CODE_BASE, code);

    let mut state = cpu::I386State {
        ip: 0x0000,
        ..Default::default()
    };
    state.set_cs(INJECT_CODE_SEGMENT);
    state.set_ss(INJECT_CODE_SEGMENT);
    state.set_ds(INJECT_CODE_SEGMENT);
    state.set_es(INJECT_CODE_SEGMENT);
    state.set_esp(0xFFFE);
    // Enable interrupts (IF flag) so DOS INT handlers work.
    state.set_eflags(state.eflags() | 0x0200);
    machine.cpu.load_state(&state);

    machine.run_for(budget);
}

/// Clears the InDOS flag so that DOS file I/O functions work correctly.
/// After boot, COMMAND.COM sits inside an INT 21h call (reading input),
/// leaving InDOS=1. When we hijack the CPU, this flag remains set.
/// DOS file I/O checks InDOS for re-entrancy and may hang if it's nonzero.
pub fn reset_indos(machine: &mut machine::Pc9801Ra) {
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x34,                         // MOV AH, 34h (get InDOS address)
        0xCD, 0x21,                         // INT 21h -> ES:BX = InDOS
        0x26, 0xC6, 0x07, 0x00,             // MOV BYTE ES:[BX], 00h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    inject_and_run(machine, code);
}

/// Runs code via the INT 28h DOS idle hook, which is safe for file I/O.
///
/// After boot, COMMAND.COM is inside INT 21h/0Ah (reading keyboard input).
/// DOS calls INT 28h from this idle loop. Inside INT 28h, it is safe to call
/// INT 21h functions with AH >= 0Ch (including all file I/O). This avoids the
/// re-entrancy hang that occurs when directly hijacking the CPU for disk operations.
///
/// Layout at INJECT_CODE_BASE:
///   +0x0000: INT 28h hook stub (saves old vector, runs user code, restores, IRET)
///   +0x0080: user code (the actual test code)
///   +0x0100: result area
///   +0x0200: data area (filenames, buffers)
pub fn inject_and_run_via_int28(machine: &mut machine::Pc9801Ra, code: &[u8], budget: u64) {
    let base = INJECT_CODE_BASE;
    let seg_lo = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    let seg_hi = (INJECT_CODE_SEGMENT >> 8) as u8;

    // Save old INT 28h vector (at IVT 0x00A0).
    let old_int28_off = read_word(&machine.bus, 0x00A0);
    let old_int28_seg = read_word(&machine.bus, 0x00A2);

    // Write user code at +0x0080.
    write_bytes(&mut machine.bus, base + 0x0080, code);

    // Build INT 28h hook stub at +0x0000:
    //   PUSH DS
    //   PUSH ES
    //   MOV AX, INJECT_CODE_SEGMENT
    //   MOV DS, AX
    //   MOV ES, AX
    //   CALL 0x0080              (call user code)
    //   POP ES
    //   POP DS
    //   Restore old INT 28h vector
    //   IRET
    let old_off_lo = (old_int28_off & 0xFF) as u8;
    let old_off_hi = (old_int28_off >> 8) as u8;
    let old_seg_lo = (old_int28_seg & 0xFF) as u8;
    let old_seg_hi = (old_int28_seg >> 8) as u8;
    // CALL rel16: target=0x0080, CALL at +0x09 (3 bytes), IP after=0x0C, rel=0x0080-0x0C=0x0074.
    #[rustfmt::skip]
    let stub: Vec<u8> = vec![
        0x1E,                               // PUSH DS                  ; +0x00
        0x06,                               // PUSH ES                  ; +0x01
        0xB8, seg_lo, seg_hi,               // MOV AX, seg              ; +0x02
        0x8E, 0xD8,                         // MOV DS, AX               ; +0x05
        0x8E, 0xC0,                         // MOV ES, AX               ; +0x07
        0xE8, 0x74, 0x00,                   // CALL 0080h               ; +0x09
        // After user code returns:
        0x07,                               // POP ES                   ; +0x0C
        0x1F,                               // POP DS                   ; +0x0D
        // Restore old INT 28h vector (write to IVT at 0000:00A0)
        0x50,                               // PUSH AX                  ; +0x0E
        0x53,                               // PUSH BX                  ; +0x0F
        0x1E,                               // PUSH DS                  ; +0x10
        0x31, 0xDB,                         // XOR BX, BX               ; +0x11
        0x8E, 0xDB,                         // MOV DS, BX               ; +0x13
        0xBB, 0xA0, 0x00,                   // MOV BX, 00A0h            ; +0x15
        0xC7, 0x07, old_off_lo, old_off_hi, // MOV [BX], old_offset    ; +0x18
        0xC7, 0x47, 0x02, old_seg_lo, old_seg_hi, // MOV [BX+2], old_segment ; +0x1C
        0x1F,                               // POP DS                   ; +0x21
        0x5B,                               // POP BX                   ; +0x22
        0x58,                               // POP AX                   ; +0x23
        0xCF,                               // IRET                     ; +0x24
    ];
    write_bytes(&mut machine.bus, base, &stub);

    // Set INT 28h vector to point to our stub.
    machine.bus.write_byte(0x00A0, 0x00); // offset low
    machine.bus.write_byte(0x00A1, 0x00); // offset high
    machine.bus.write_byte(0x00A2, seg_lo);
    machine.bus.write_byte(0x00A3, seg_hi);

    // Resume the machine. DOS will call INT 28h from the idle loop,
    // which runs our stub, which runs the user code, restores INT 28h, and IRETs.
    machine.run_for(budget);
}

pub fn far_to_linear(segment: u16, offset: u16) -> u32 {
    ((segment as u32) << 4) + offset as u32
}

pub fn read_byte(bus: &machine::Pc9801Bus, addr: u32) -> u8 {
    bus.read_byte_direct(addr)
}

pub fn read_word(bus: &machine::Pc9801Bus, addr: u32) -> u16 {
    let low = bus.read_byte_direct(addr) as u16;
    let high = bus.read_byte_direct(addr + 1) as u16;
    low | (high << 8)
}

pub fn read_dword(bus: &machine::Pc9801Bus, addr: u32) -> u32 {
    read_word(bus, addr) as u32 | ((read_word(bus, addr + 2) as u32) << 16)
}

pub fn read_far_ptr(bus: &machine::Pc9801Bus, addr: u32) -> (u16, u16) {
    let offset = read_word(bus, addr);
    let segment = read_word(bus, addr + 2);
    (segment, offset)
}

pub fn read_bytes(bus: &machine::Pc9801Bus, addr: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| bus.read_byte_direct(addr + i as u32))
        .collect()
}

pub fn read_string(bus: &machine::Pc9801Bus, addr: u32, max_len: usize) -> Vec<u8> {
    let mut result = Vec::new();
    for i in 0..max_len {
        let byte = bus.read_byte_direct(addr + i as u32);
        if byte == 0 {
            break;
        }
        result.push(byte);
    }
    result
}

pub fn read_device_name(bus: &machine::Pc9801Bus, header_addr: u32) -> String {
    // Device header name field is at offset +0x0A, 8 bytes.
    let name_bytes = read_bytes(bus, header_addr + 0x0A, 8);
    String::from_utf8_lossy(&name_bytes).to_string()
}

pub fn result_byte(bus: &machine::Pc9801Bus, offset: u32) -> u8 {
    bus.read_byte_direct(INJECT_RESULT_BASE + offset)
}

pub fn result_word(bus: &machine::Pc9801Bus, offset: u32) -> u16 {
    read_word(bus, INJECT_RESULT_BASE + offset)
}

pub fn result_dword(bus: &machine::Pc9801Bus, offset: u32) -> u32 {
    read_dword(bus, INJECT_RESULT_BASE + offset)
}

pub fn get_sysvars_address(machine: &mut machine::Pc9801Ra) -> u32 {
    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x52,                         // MOV AH, 52h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, RES_LO, RES_HI,         // MOV [result+0], BX
        0x8C, 0x06, RES_LO + 2, RES_HI,     // MOV [result+2], ES
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    inject_and_run(machine, code);

    let offset = result_word(&machine.bus, 0);
    let segment = result_word(&machine.bus, 2);
    far_to_linear(segment, offset)
}

pub fn get_psp_segment(machine: &mut machine::Pc9801Ra) -> u16 {
    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x62,                         // MOV AH, 62h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, RES_LO, RES_HI,        // MOV [result+0], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    inject_and_run(machine, code);

    result_word(&machine.bus, 0)
}

/// Creates free memory by splitting the last MCB (Z block) in the chain.
/// COMMAND.COM owns all remaining memory after boot, so allocation tests
/// need free memory created first. This walks the MCB chain, finds the Z block,
/// shrinks it, and appends a new free Z block of `free_paragraphs` paragraphs.
pub fn create_free_memory(machine: &mut machine::Pc9801Ra, free_paragraphs: u16) {
    let sysvars = get_sysvars_address(machine);
    let first_mcb_segment = read_word(&machine.bus, sysvars - 2);
    let mut mcb_addr = far_to_linear(first_mcb_segment, 0);

    // Walk to the last MCB (Z block).
    for _ in 0..1000 {
        let block_type = read_byte(&machine.bus, mcb_addr);
        let size = read_word(&machine.bus, mcb_addr + 3);

        if block_type == 0x5A {
            // Found the Z block. Split it.
            let current_segment = mcb_addr >> 4;
            assert!(
                size > free_paragraphs + 1,
                "Z block too small to split: size={}, need={}",
                size,
                free_paragraphs + 1
            );

            let new_size = size - free_paragraphs - 1;
            // Change Z to M and shrink.
            machine.bus.write_byte(mcb_addr, 0x4D); // 'M'
            machine
                .bus
                .write_byte(mcb_addr + 3, (new_size & 0xFF) as u8);
            machine.bus.write_byte(mcb_addr + 4, (new_size >> 8) as u8);

            // Create new free Z block after the shrunken block.
            let new_mcb_segment = current_segment + new_size as u32 + 1;
            let new_mcb_addr = new_mcb_segment << 4;
            machine.bus.write_byte(new_mcb_addr, 0x5A); // 'Z'
            machine.bus.write_byte(new_mcb_addr + 1, 0x00); // owner = free
            machine.bus.write_byte(new_mcb_addr + 2, 0x00);
            machine
                .bus
                .write_byte(new_mcb_addr + 3, (free_paragraphs & 0xFF) as u8);
            machine
                .bus
                .write_byte(new_mcb_addr + 4, (free_paragraphs >> 8) as u8);
            // Clear reserved and name fields.
            for i in 5..16 {
                machine.bus.write_byte(new_mcb_addr + i, 0x00);
            }
            return;
        }

        if block_type != 0x4D {
            panic!("Invalid MCB type {:#04X} at {:#010X}", block_type, mcb_addr);
        }

        let next_segment = (mcb_addr >> 4) + size as u32 + 1;
        mcb_addr = next_segment << 4;
    }
    panic!("Could not find Z block in MCB chain");
}

pub fn find_char_in_text_vram(bus: &machine::Pc9801Bus, char_code: u16) -> bool {
    let vram = bus.text_vram();
    for row in 0..25 {
        for col in 0..80 {
            let offset = (row * 80 + col) * 2;
            if offset + 1 >= vram.len() {
                return false;
            }
            let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            if code == char_code {
                return true;
            }
        }
    }
    false
}

pub fn find_string_in_text_vram(bus: &machine::Pc9801Bus, chars: &[u16]) -> bool {
    if chars.is_empty() {
        return true;
    }
    let vram = bus.text_vram();
    let total_chars = 80 * 25;
    for start in 0..total_chars {
        if start + chars.len() > total_chars {
            break;
        }
        let mut matched = true;
        for (i, &expected) in chars.iter().enumerate() {
            let offset = (start + i) * 2;
            if offset + 1 >= vram.len() {
                return false;
            }
            let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            if code != expected {
                matched = false;
                break;
            }
        }
        if matched {
            return true;
        }
    }
    false
}

/// Loads the raw HDD image data, skipping the HDI header (first 32 bytes).
pub fn load_hdd_image_data() -> Vec<u8> {
    let hdd_path = workspace_root().join(DOS_HDD_IMAGE_NAME);
    std::fs::read(&hdd_path).unwrap_or_else(|error| {
        panic!(
            "DOS 6.20 HDD image ({}) required: {}",
            hdd_path.display(),
            error
        )
    })
}
