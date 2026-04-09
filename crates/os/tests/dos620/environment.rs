use crate::harness;

fn boot_and_get_environment() -> (machine::Pc9801Ra, u32) {
    let mut machine = harness::boot_hle();
    let psp_segment = harness::get_psp_segment(&mut machine);
    let psp_linear = harness::far_to_linear(psp_segment, 0);
    let env_segment = harness::read_word(&machine.bus, psp_linear + 0x2C);
    let env_linear = harness::far_to_linear(env_segment, 0);
    (machine, env_linear)
}

fn read_environment_strings(bus: &machine::Pc9801Bus, env_addr: u32) -> Vec<String> {
    let mut strings = Vec::new();
    let mut offset = 0u32;
    let max_env_size = 32768u32;

    loop {
        if offset >= max_env_size {
            break;
        }
        let byte = harness::read_byte(bus, env_addr + offset);
        if byte == 0 {
            break;
        }
        let s = harness::read_string(bus, env_addr + offset, (max_env_size - offset) as usize);
        offset += s.len() as u32 + 1;
        if let Ok(str_val) = String::from_utf8(s) {
            strings.push(str_val);
        }
    }

    strings
}

#[test]
fn has_comspec_entry() {
    let (machine, env_addr) = boot_and_get_environment();
    let strings = read_environment_strings(&machine.bus, env_addr);
    let has_comspec = strings.iter().any(|s| s.starts_with("COMSPEC="));
    assert!(
        has_comspec,
        "Environment should contain COMSPEC= entry, found: {:?}",
        strings
    );
}

#[test]
fn has_path_entry() {
    let (machine, env_addr) = boot_and_get_environment();
    let strings = read_environment_strings(&machine.bus, env_addr);
    let has_path = strings.iter().any(|s| s.starts_with("PATH="));
    assert!(
        has_path,
        "Environment should contain PATH= entry, found: {:?}",
        strings
    );
}

#[test]
fn double_null_terminated() {
    let (machine, env_addr) = boot_and_get_environment();
    // Walk to find the double-null terminator.
    let mut offset = 0u32;
    let max_size = 32768u32;
    let mut found_double_null = false;

    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                found_double_null = true;
                break;
            }
        }
        offset += 1;
    }

    assert!(
        found_double_null,
        "Environment block should end with a double-null terminator"
    );
}

#[test]
fn program_name_after_terminator() {
    let (machine, env_addr) = boot_and_get_environment();
    // Find the double-null terminator, then check for WORD count + pathname.
    let mut offset = 0u32;
    let max_size = 32768u32;

    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                // Double-null found at offset. After it: WORD count + pathname.
                let count_offset = offset + 2;
                let count = harness::read_word(&machine.bus, env_addr + count_offset);
                assert!(
                    count >= 1,
                    "Program name count after environment should be >= 1, got {}",
                    count
                );

                let pathname = harness::read_string(&machine.bus, env_addr + count_offset + 2, 128);
                assert!(
                    !pathname.is_empty(),
                    "Program pathname after environment should be non-empty"
                );
                return;
            }
        }
        offset += 1;
    }

    panic!("Could not find double-null terminator in environment block");
}

#[test]
fn prompt_entry_if_present() {
    let (machine, env_addr) = boot_and_get_environment();
    let strings = read_environment_strings(&machine.bus, env_addr);
    // PROMPT may or may not be set depending on AUTOEXEC.BAT. If present, verify it has a value.
    if let Some(prompt) = strings.iter().find(|s| s.starts_with("PROMPT=")) {
        assert!(
            prompt.len() > 7,
            "PROMPT= should have a non-empty value, got '{}'",
            prompt
        );
    }
}

#[test]
fn program_pathname_is_command_com() {
    let mut machine = harness::boot_hle();
    let psp_segment = harness::get_psp_segment(&mut machine);
    let psp_linear = harness::far_to_linear(psp_segment, 0);
    let env_segment = harness::read_word(&machine.bus, psp_linear + 0x2C);
    let env_addr = harness::far_to_linear(env_segment, 0);

    let mut offset = 0u32;
    let max_size = 32768u32;

    // Find the double-null terminator.
    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                let count_offset = offset + 2;
                let count = harness::read_word(&machine.bus, env_addr + count_offset);

                // NEETAN OS HLE sets the program name for all processes
                // including the root COMMAND.COM, for maximum compatibility
                // with programs that read it (spec section 2.6).
                assert_eq!(
                    count, 0x0001,
                    "NEETAN HLE: COMMAND.COM environment should have count=1"
                );

                let pathname = harness::read_string(&machine.bus, env_addr + count_offset + 2, 128);
                let pathname_str = String::from_utf8_lossy(&pathname);
                assert!(
                    pathname_str.contains("COMMAND.COM"),
                    "Program pathname should contain 'COMMAND.COM', got '{}'",
                    pathname_str
                );
                return;
            }
        }
        offset += 1;
    }
    panic!("Could not find double-null terminator in environment block");
}

/// EXEC a child .COM process and verify it inherits the parent's environment
/// with a valid program pathname after the double-null terminator.
#[test]
fn child_process_program_name() {
    use common::Bus;
    use harness::*;

    // .COM that reads its own env segment from PSP+0x2C, stores it at 0x2000:0x0180,
    // then terminates.
    #[rustfmt::skip]
    let env_reader_com: &[u8] = &[
        0xA1, 0x2C, 0x00,                  // MOV AX, [002Ch]  ; env_seg from PSP
        0xBB, 0x00, 0x20,                  // MOV BX, 2000h
        0x8E, 0xC3,                        // MOV ES, BX
        0x26, 0xA3, 0x80, 0x01,            // MOV [ES:0180h], AX
        0xB4, 0x4C,                        // MOV AH, 4Ch
        0x30, 0xC0,                        // XOR AL, AL
        0xCD, 0x21,                        // INT 21h
    ];

    let mut machine = boot_hle_with_floppy();
    machine.bus.eject_floppy(0);
    let floppy = create_test_floppy_with_program(b"TEST    COM", env_reader_com);
    machine.bus.insert_floppy(0, floppy, None);

    let base = INJECT_CODE_BASE;
    let seg = INJECT_CODE_SEGMENT;

    // Filename at +0x0200
    write_bytes(&mut machine.bus, base + 0x0200, b"A:\\TEST.COM\x00");

    // Command tail at +0x0220: length=0, CR
    machine.bus.write_byte(base + 0x0220, 0x00);
    machine.bus.write_byte(base + 0x0221, 0x0D);

    // EXEC parameter block at +0x0210 (env_seg=0 -> inherit parent)
    let seg_lo = (seg & 0xFF) as u8;
    let seg_hi = (seg >> 8) as u8;
    #[rustfmt::skip]
    let param_block: [u8; 14] = [
        0x00, 0x00,                         // env_seg = 0 (inherit)
        0x20, 0x02, seg_lo, seg_hi,         // cmd_tail = seg:0220
        0x30, 0x02, seg_lo, seg_hi,         // fcb1 = seg:0230
        0x40, 0x02, seg_lo, seg_hi,         // fcb2 = seg:0240
    ];
    write_bytes(&mut machine.bus, base + 0x0210, &param_block);

    const SEG_LO: u8 = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    const SEG_HI: u8 = (INJECT_CODE_SEGMENT >> 8) as u8;

    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBA, 0x00, 0x02,                  // MOV DX, 0200h (filename)
        0xBB, 0x10, 0x02,                  // MOV BX, 0210h (param block)
        0xB8, 0x00, 0x4B,                  // MOV AX, 4B00h (EXEC)
        0xCD, 0x21,                        // INT 21h
        // Restore DS after EXEC (destroys all regs except SS:SP).
        0xB8, SEG_LO, SEG_HI,              // MOV AX, INJECT_CODE_SEGMENT
        0x8E, 0xD8,                        // MOV DS, AX
        0xFA,                              // CLI
        0xF4,                              // HLT
    ];

    inject_and_run_with_budget(&mut machine, code, INJECT_BUDGET_DISK_IO);

    // The child .COM stored its env segment at 0x2000:0x0180.
    let child_env_seg = harness::read_word(&machine.bus, INJECT_CODE_BASE + 0x0180);
    assert!(
        child_env_seg > 0,
        "Child env segment should be non-zero, got {:#06X}",
        child_env_seg
    );

    let env_addr = harness::far_to_linear(child_env_seg, 0);

    // Walk the environment to find the double-null terminator.
    let mut offset = 0u32;
    let max_size = 32768u32;

    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                // Double-null found. After it: WORD count + pathname.
                let count_offset = offset + 2;
                let count = harness::read_word(&machine.bus, env_addr + count_offset);
                assert!(
                    count >= 1,
                    "Program name count should be >= 1, got {}",
                    count
                );

                let pathname = harness::read_string(&machine.bus, env_addr + count_offset + 2, 128);
                let pathname_str = String::from_utf8_lossy(&pathname);
                assert!(
                    pathname_str.contains("COMMAND.COM"),
                    "Child inherited env should contain 'COMMAND.COM' in program pathname, got '{pathname_str}'",
                );
                return;
            }
        }
        offset += 1;
    }
    panic!("Could not find double-null terminator in child environment block");
}
