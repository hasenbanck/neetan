use crate::harness;

#[test]
fn get_current_drive() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x19,                         // MOV AH, 19h
        0xCD, 0x21,                         // INT 21h
        0xA2, 0x00, 0x01,                   // MOV [0x0100], AL
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let drive = harness::result_byte(&machine.bus, 0);
    assert!(drive < 26, "Current drive should be < 26, got {}", drive);
}

#[test]
fn select_drive_returns_lastdrive() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x0E,                         // MOV AH, 0Eh
        0xB2, 0x00,                         // MOV DL, 00h (select A:)
        0xCD, 0x21,                         // INT 21h
        0xA2, 0x00, 0x01,                   // MOV [0x0100], AL
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let lastdrive = harness::result_byte(&machine.bus, 0);
    assert!(
        (5..=26).contains(&lastdrive),
        "LASTDRIVE count from AH=0Eh should be 5-26, got {}",
        lastdrive
    );
}

#[test]
fn get_set_interrupt_vector() {
    let mut machine = harness::boot_hle();
    // Set INT 60h to a known address, then read it back.
    // AH=25h: AL=vector, DS:DX=handler.
    // AH=35h: AL=vector, returns ES:BX.
    let seg_lo = (harness::INJECT_CODE_SEGMENT & 0xFF) as u8;
    let seg_hi = (harness::INJECT_CODE_SEGMENT >> 8) as u8;
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        // Set INT 60h to 1234:5678
        0xB8, 0x34, 0x12,                   // MOV AX, 1234h (segment for DS)
        0x8E, 0xD8,                         // MOV DS, AX
        0xBA, 0x78, 0x56,                   // MOV DX, 5678h
        0xB4, 0x25,                         // MOV AH, 25h
        0xB0, 0x60,                         // MOV AL, 60h
        0xCD, 0x21,                         // INT 21h
        // Restore DS to our injection segment
        0xB8, seg_lo, seg_hi,               // MOV AX, INJECT_CODE_SEGMENT
        0x8E, 0xD8,                         // MOV DS, AX
        // Get INT 60h back
        0xB4, 0x35,                         // MOV AH, 35h
        0xB0, 0x60,                         // MOV AL, 60h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (offset)
        0x8C, 0x06, 0x02, 0x01,             // MOV [0x0102], ES (segment)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, &code);

    let offset = harness::result_word(&machine.bus, 0);
    let segment = harness::result_word(&machine.bus, 2);
    assert_eq!(
        offset, 0x5678,
        "INT 60h offset should be 0x5678 after set, got {:#06X}",
        offset
    );
    assert_eq!(
        segment, 0x1234,
        "INT 60h segment should be 0x1234 after set, got {:#06X}",
        segment
    );
}

#[test]
fn get_dta_address() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x2F,                         // MOV AH, 2Fh
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX
        0x8C, 0x06, 0x02, 0x01,             // MOV [0x0102], ES
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let offset = harness::result_word(&machine.bus, 0);
    let segment = harness::result_word(&machine.bus, 2);
    // Default DTA is at PSP+0x80. The offset should be 0x0080.
    assert_eq!(
        offset, 0x0080,
        "Default DTA offset should be 0x0080 (PSP+80h), got {:#06X}",
        offset
    );
    assert_ne!(segment, 0x0000, "DTA segment should be non-zero");
}

#[test]
fn get_indos_flag_address() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x34,                         // MOV AH, 34h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX
        0x8C, 0x06, 0x02, 0x01,             // MOV [0x0102], ES
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let offset = harness::result_word(&machine.bus, 0);
    let segment = harness::result_word(&machine.bus, 2);
    let linear = harness::far_to_linear(segment, offset);
    assert!(
        linear > 0 && linear < 0xA0000,
        "InDOS flag address should be in conventional memory, got {:#010X}",
        linear
    );
}

#[test]
fn get_current_directory() {
    let mut machine = harness::boot_hle();
    // AH=47h, DL=0 (current drive). DS:SI = 64-byte buffer.
    let buffer_offset: u16 = harness::INJECT_RESULT_OFFSET + 0x10;
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        0xB4, 0x47,                                        // MOV AH, 47h
        0xB2, 0x00,                                        // MOV DL, 00h (default drive)
        0xBE, buffer_offset as u8, (buffer_offset >> 8) as u8, // MOV SI, buffer_offset
        0xCD, 0x21,                                        // INT 21h
        0xFA,                                              // CLI
        0xF4,                                              // HLT
    ];
    harness::inject_and_run(&mut machine, &code);

    // For root directory, the buffer should start with 0x00 (empty string = root).
    let first_byte = harness::read_byte(&machine.bus, harness::INJECT_RESULT_BASE + 0x10);
    // Root directory returns either empty string or backslash.
    assert!(
        first_byte == 0x00 || first_byte == 0x5C,
        "Current directory for root should be empty (0x00) or backslash (0x5C), got {:#04X}",
        first_byte
    );
}

#[test]
fn allocate_memory() {
    let mut machine = harness::boot_hle();
    // HLE MCB chain already has a free Z block after boot.
    // AH=48h, BX=0x10 (allocate 16 paragraphs = 256 bytes).
    // On success: CF=0, AX=segment. On error: CF=1, BX=largest available.
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (segment or error)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (flags)
        // Free the allocated block immediately.
        0xA1, 0x00, 0x01,                   // MOV AX, [0x0100]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 2);
    let carry_flag = flags & 0x0001;
    assert_eq!(
        carry_flag, 0,
        "Memory allocation (AH=48h) should succeed (CF=0), flags={:#06X}",
        flags
    );

    let segment = harness::result_word(&machine.bus, 0);
    let linear = harness::far_to_linear(segment, 0);
    assert!(
        linear > 0 && linear < 0xA0000,
        "Allocated segment should be in conventional memory, got seg {:#06X} (linear {:#010X})",
        segment,
        linear
    );
}

#[test]
fn resize_memory() {
    let mut machine = harness::boot_hle();

    // Allocate 16 paragraphs, resize to 8, then free.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 16 paragraphs
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (segment)
        // Resize to 8 paragraphs
        0x8E, 0xC0,                         // MOV ES, AX
        0xBB, 0x08, 0x00,                   // MOV BX, 0008h
        0xB4, 0x4A,                         // MOV AH, 4Ah
        0xCD, 0x21,                         // INT 21h
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (flags)
        // Free
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 2);
    let carry_flag = flags & 0x0001;
    assert_eq!(
        carry_flag, 0,
        "Memory resize (AH=4Ah) should succeed (CF=0), flags={:#06X}",
        flags
    );
}

#[test]
fn free_memory() {
    let mut machine = harness::boot_hle();

    // Allocate then free.
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (flags)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 0);
    let carry_flag = flags & 0x0001;
    assert_eq!(
        carry_flag, 0,
        "Memory free (AH=49h) should succeed (CF=0), flags={:#06X}",
        flags
    );
}

#[test]
fn get_psp_via_51h() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x51,                         // MOV AH, 51h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let psp = harness::result_word(&machine.bus, 0);
    let linear = harness::far_to_linear(psp, 0);
    assert!(
        linear > 0 && linear < 0xA0000,
        "PSP from AH=51h should be in conventional memory, got seg {:#06X}",
        psp
    );
}

#[test]
fn get_psp_via_62h() {
    let mut machine = harness::boot_hle();
    // Call both AH=51h and AH=62h and verify they return the same value.
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x51,                         // MOV AH, 51h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (via 51h)
        0xB4, 0x62,                         // MOV AH, 62h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (via 62h)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let psp_51h = harness::result_word(&machine.bus, 0);
    let psp_62h = harness::result_word(&machine.bus, 2);
    assert_eq!(
        psp_51h, psp_62h,
        "AH=51h ({:#06X}) and AH=62h ({:#06X}) should return the same PSP segment",
        psp_51h, psp_62h
    );
}

#[test]
fn get_sysvars_pointer() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x52,                         // MOV AH, 52h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX
        0x8C, 0x06, 0x02, 0x01,             // MOV [0x0102], ES
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let offset = harness::result_word(&machine.bus, 0);
    let segment = harness::result_word(&machine.bus, 2);
    let linear = harness::far_to_linear(segment, offset);
    assert!(
        linear > 0 && linear < 0xA0000,
        "SYSVARS pointer should be in conventional memory, got {:#010X}",
        linear
    );
}

#[test]
fn get_memory_strategy() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x58,                         // MOV AH, 58h
        0xB0, 0x00,                         // MOV AL, 00h (get strategy)
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let strategy = harness::result_word(&machine.bus, 0);
    assert!(
        strategy <= 2,
        "Memory allocation strategy should be 0 (first fit), 1 (best fit), or 2 (last fit), got {}",
        strategy
    );
}

#[test]
fn set_and_get_dta() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Set DTA to DS:0200h (DS = INJECT_CODE_SEGMENT)
        0xBA, 0x00, 0x02,                   // MOV DX, 0200h
        0xB4, 0x1A,                         // MOV AH, 1Ah
        0xCD, 0x21,                         // INT 21h
        // Get DTA back
        0xB4, 0x2F,                         // MOV AH, 2Fh
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX
        0x8C, 0x06, 0x02, 0x01,             // MOV [0x0102], ES
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let offset = harness::result_word(&machine.bus, 0);
    let segment = harness::result_word(&machine.bus, 2);
    assert_eq!(
        offset, 0x0200,
        "DTA offset should be 0x0200 after set, got {:#06X}",
        offset
    );
    assert_eq!(
        segment,
        harness::INJECT_CODE_SEGMENT,
        "DTA segment should be {:#06X} after set, got {:#06X}",
        harness::INJECT_CODE_SEGMENT,
        segment
    );
}

#[test]
fn set_memory_strategy_round_trip() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Get current strategy
        0xB4, 0x58,                         // MOV AH, 58h
        0xB0, 0x00,                         // MOV AL, 00h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (original)
        // Set to best-fit (1)
        0xB4, 0x58,                         // MOV AH, 58h
        0xB0, 0x01,                         // MOV AL, 01h (set)
        0xBB, 0x01, 0x00,                   // MOV BX, 0001h (best fit)
        0xCD, 0x21,                         // INT 21h
        // Get again
        0xB4, 0x58,                         // MOV AH, 58h
        0xB0, 0x00,                         // MOV AL, 00h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (after set)
        // Restore original
        0x8B, 0x1E, 0x00, 0x01,             // MOV BX, [0x0100]
        0xB4, 0x58,                         // MOV AH, 58h
        0xB0, 0x01,                         // MOV AL, 01h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let original = harness::result_word(&machine.bus, 0);
    assert!(
        original <= 2,
        "Original strategy should be 0-2, got {}",
        original
    );

    let after_set = harness::result_word(&machine.bus, 2);
    assert_eq!(
        after_set, 1,
        "Strategy after setting to best-fit should be 1, got {}",
        after_set
    );
}

#[test]
fn get_set_ctrl_break() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Get current Ctrl-Break state
        0xB4, 0x33,                         // MOV AH, 33h
        0xB0, 0x00,                         // MOV AL, 00h (get)
        0xCD, 0x21,                         // INT 21h
        0x88, 0x16, 0x00, 0x01,             // MOV [0x0100], DL (original)
        // Set to ON
        0xB4, 0x33,                         // MOV AH, 33h
        0xB0, 0x01,                         // MOV AL, 01h (set)
        0xB2, 0x01,                         // MOV DL, 01h (ON)
        0xCD, 0x21,                         // INT 21h
        // Get again
        0xB4, 0x33,                         // MOV AH, 33h
        0xB0, 0x00,                         // MOV AL, 00h
        0xCD, 0x21,                         // INT 21h
        0x88, 0x16, 0x01, 0x01,             // MOV [0x0101], DL (after set)
        // Restore original
        0xB4, 0x33,                         // MOV AH, 33h
        0xB0, 0x01,                         // MOV AL, 01h
        0x8A, 0x16, 0x00, 0x01,             // MOV DL, [0x0100]
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let original = harness::result_byte(&machine.bus, 0);
    assert!(
        original <= 1,
        "Ctrl-Break state should be 0 or 1, got {}",
        original
    );

    let after_set = harness::result_byte(&machine.bus, 1);
    assert_eq!(
        after_set, 1,
        "Ctrl-Break state after setting to ON should be 1, got {}",
        after_set
    );
}

#[test]
fn get_switch_character() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x37,                         // MOV AH, 37h
        0xB0, 0x00,                         // MOV AL, 00h (get switch char)
        0xCD, 0x21,                         // INT 21h
        0x88, 0x16, 0x00, 0x01,             // MOV [0x0100], DL (switch char)
        0xA2, 0x01, 0x01,                   // MOV [0x0101], AL (return code)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let switch_char = harness::result_byte(&machine.bus, 0);
    assert!(
        switch_char == 0x2F || switch_char == 0x2D,
        "Switch character should be '/' (0x2F) or '-' (0x2D), got {:#04X}",
        switch_char
    );
}

#[test]
fn set_psp_round_trip() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Get current PSP
        0xB4, 0x62,                         // MOV AH, 62h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (original)
        // Set PSP to 0x1234
        0xBB, 0x34, 0x12,                   // MOV BX, 1234h
        0xB4, 0x50,                         // MOV AH, 50h
        0xCD, 0x21,                         // INT 21h
        // Get again
        0xB4, 0x62,                         // MOV AH, 62h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (after set)
        // Restore original
        0x8B, 0x1E, 0x00, 0x01,             // MOV BX, [0x0100]
        0xB4, 0x50,                         // MOV AH, 50h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let after_set = harness::result_word(&machine.bus, 2);
    assert_eq!(
        after_set, 0x1234,
        "PSP after setting to 0x1234 should be 0x1234, got {:#06X}",
        after_set
    );
}

#[test]
fn get_return_code() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x4D,                         // MOV AH, 4Dh
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (AH=type, AL=code)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let termination_type = harness::result_byte(&machine.bus, 1);
    assert!(
        termination_type <= 3,
        "Termination type (AH) should be 0-3, got {}",
        termination_type
    );
}

#[test]
fn change_directory_to_root() {
    let mut machine = harness::boot_hle();
    let path = b"A:\\\0";
    harness::write_bytes(&mut machine.bus, harness::INJECT_CODE_BASE + 0x200, path);

    let buffer_offset: u16 = harness::INJECT_RESULT_OFFSET + 0x10;
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        // CHDIR to root
        0xBA, 0x00, 0x02,                                        // MOV DX, 0200h
        0xB4, 0x3B,                                              // MOV AH, 3Bh
        0xCD, 0x21,                                              // INT 21h
        0x9C,                                                    // PUSHF
        0x58,                                                    // POP AX
        0xA3, 0x00, 0x01,                                        // MOV [0x0100], AX (flags)
        // Get current directory
        0xB4, 0x47,                                              // MOV AH, 47h
        0xB2, 0x00,                                              // MOV DL, 00h
        0xBE, buffer_offset as u8, (buffer_offset >> 8) as u8,   // MOV SI, buffer_offset
        0xCD, 0x21,                                              // INT 21h
        0xFA,                                                    // CLI
        0xF4,                                                    // HLT
    ];
    harness::inject_and_run(&mut machine, &code);

    let flags = harness::result_word(&machine.bus, 0);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 0,
        "CHDIR to root should succeed (CF=0), flags={:#06X}",
        flags
    );

    let first_byte = harness::read_byte(&machine.bus, harness::INJECT_RESULT_BASE + 0x10);
    assert!(
        first_byte == 0x00 || first_byte == 0x5C,
        "After CHDIR to root, current directory should be empty or '\\', got {:#04X}",
        first_byte
    );
}

#[test]
fn change_directory_with_dot_component() {
    let mut machine = harness::boot_hle_with_floppy();
    harness::type_string(&mut machine.bus, b"A:\r");
    harness::run_until_prompt(&mut machine);
    harness::type_string(&mut machine.bus, b"MD SUBDIR\r");
    harness::run_until_prompt(&mut machine);

    let path = b".\\SUBDIR\0";
    harness::write_bytes(&mut machine.bus, harness::INJECT_CODE_BASE + 0x200, path);

    let buffer_offset: u16 = harness::INJECT_RESULT_OFFSET + 0x10;
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        0xBA, 0x00, 0x02,                                        // MOV DX, 0200h
        0xB4, 0x3B,                                              // MOV AH, 3Bh
        0xCD, 0x21,                                              // INT 21h
        0x9C,                                                    // PUSHF
        0x58,                                                    // POP AX
        0xA3, 0x00, 0x01,                                        // MOV [0x0100], AX (flags)
        0xB4, 0x47,                                              // MOV AH, 47h
        0xB2, 0x00,                                              // MOV DL, 00h
        0xBE, buffer_offset as u8, (buffer_offset >> 8) as u8,   // MOV SI, buffer_offset
        0xCD, 0x21,                                              // INT 21h
        0xFA,                                                    // CLI
        0xF4,                                                    // HLT
    ];
    harness::inject_and_run_with_budget(&mut machine, &code, harness::INJECT_BUDGET_DISK_IO);

    let flags = harness::result_word(&machine.bus, 0);
    assert_eq!(
        flags & 0x0001,
        0,
        "CHDIR with .\\ component should succeed, flags={:#06X}",
        flags
    );

    let cwd = harness::read_string(&machine.bus, harness::INJECT_RESULT_BASE + 0x10, 67);
    assert_eq!(cwd, b"SUBDIR", "Current directory should resolve to SUBDIR");
}

#[test]
fn parse_filename_into_fcb() {
    let mut machine = harness::boot_hle();
    // Place filename at 0x9000:0200, FCB buffer at 0x9000:0220.
    let filename = b" TEST.TXT\0";
    harness::write_bytes(
        &mut machine.bus,
        harness::INJECT_CODE_BASE + 0x200,
        filename,
    );
    // Zero out FCB area.
    let zeros = [0u8; 37];
    harness::write_bytes(&mut machine.bus, harness::INJECT_CODE_BASE + 0x220, &zeros);

    #[rustfmt::skip]
    let code: &[u8] = &[
        // AH=29h, AL=01h (skip leading separators), DS:SI=filename, ES:DI=FCB
        0xBE, 0x00, 0x02,                   // MOV SI, 0200h
        0xBF, 0x20, 0x02,                   // MOV DI, 0220h
        0xB4, 0x29,                         // MOV AH, 29h
        0xB0, 0x01,                         // MOV AL, 01h
        0xCD, 0x21,                         // INT 21h
        0xA2, 0x00, 0x01,                   // MOV [0x0100], AL (result)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let result = harness::result_byte(&machine.bus, 0);
    assert_eq!(
        result, 0x00,
        "Parse filename should return 0 (no wildcards), got {:#04X}",
        result
    );

    // FCB: offset+1..+8 = filename (8 bytes, space-padded), offset+9..+11 = extension (3 bytes).
    let fcb_addr = harness::INJECT_CODE_BASE + 0x220;
    let name = harness::read_bytes(&machine.bus, fcb_addr + 1, 8);
    let ext = harness::read_bytes(&machine.bus, fcb_addr + 9, 3);
    assert_eq!(
        &name,
        b"TEST    ",
        "FCB filename should be 'TEST    ', got {:?}",
        String::from_utf8_lossy(&name)
    );
    assert_eq!(
        &ext,
        b"TXT",
        "FCB extension should be 'TXT', got {:?}",
        String::from_utf8_lossy(&ext)
    );
}

#[test]
fn get_allocation_info() {
    let mut machine = harness::boot_hle_with_floppy();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x1C,                         // MOV AH, 1Ch
        0xB2, 0x00,                         // MOV DL, 00h (default drive)
        0xCD, 0x21,                         // INT 21h
        // AL = sectors/cluster, CX = bytes/sector, DX = total clusters
        // INT 21h/1Ch modifies DS (returns DS:BX -> media descriptor),
        // so we must use ES: prefix to store results in our segment.
        0x26, 0xA2, 0x00, 0x01,             // MOV ES:[0x0100], AL
        0x26, 0x89, 0x0E, 0x02, 0x01,       // MOV ES:[0x0102], CX
        0x26, 0x89, 0x16, 0x04, 0x01,       // MOV ES:[0x0104], DX
        0xC3,                               // RET
    ];
    harness::inject_and_run_via_int28(&mut machine, code, harness::INJECT_BUDGET_DISK_IO);

    let sectors_per_cluster = harness::result_byte(&machine.bus, 0) as u16;
    assert!(
        sectors_per_cluster > 0 && sectors_per_cluster.is_power_of_two(),
        "Sectors per cluster should be a power of 2, got {}",
        sectors_per_cluster
    );

    let bytes_per_sector = harness::result_word(&machine.bus, 2);
    assert!(
        bytes_per_sector == 256 || bytes_per_sector == 512 || bytes_per_sector == 1024,
        "Bytes per sector should be 256, 512, or 1024, got {}",
        bytes_per_sector
    );

    let total_clusters = harness::result_word(&machine.bus, 4);
    assert!(
        total_clusters > 0,
        "Total clusters should be > 0, got {}",
        total_clusters
    );
}

#[test]
fn get_free_disk_space() {
    let mut machine = harness::boot_hle_with_floppy();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x36,                         // MOV AH, 36h
        0xB2, 0x01,                         // MOV DL, 01h (A:)
        0xCD, 0x21,                         // INT 21h
        0x89, 0x06, 0x00, 0x01,             // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX
        0x89, 0x0E, 0x04, 0x01,             // MOV [0x0104], CX
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run_with_budget(&mut machine, code, harness::INJECT_BUDGET_DISK_IO);

    let sectors_per_cluster = harness::result_word(&machine.bus, 0);
    assert_eq!(
        sectors_per_cluster, 1,
        "Test floppy should report 1 sector per cluster, got {}",
        sectors_per_cluster
    );

    let free_clusters = harness::result_word(&machine.bus, 2);
    assert_eq!(
        free_clusters, 1218,
        "Test floppy should report 1218 free clusters, got {}",
        free_clusters
    );

    let bytes_per_sector = harness::result_word(&machine.bus, 4);
    assert_eq!(
        bytes_per_sector, 1024,
        "Test floppy should report 1024 bytes per sector, got {}",
        bytes_per_sector
    );

    let total_clusters = harness::result_word(&machine.bus, 6);
    assert_eq!(
        total_clusters, 1221,
        "Test floppy should report 1221 total clusters, got {}",
        total_clusters
    );
}

#[test]
fn get_free_disk_space_invalid_drive_returns_ffff() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x36,                         // MOV AH, 36h
        0xB2, 0x1B,                         // MOV DL, 1Bh (invalid)
        0xCD, 0x21,                         // INT 21h
        0x89, 0x06, 0x00, 0x01,             // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let ax = harness::result_word(&machine.bus, 0);
    assert_eq!(
        ax, 0xFFFF,
        "Invalid drive should return AX=FFFFh, got {:#06X}",
        ax
    );
}

#[test]
fn get_free_disk_space_virtual_drive_is_empty() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x36,                         // MOV AH, 36h
        0xB2, 0x1A,                         // MOV DL, 1Ah (Z:)
        0xCD, 0x21,                         // INT 21h
        0x89, 0x06, 0x00, 0x01,             // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX
        0x89, 0x0E, 0x04, 0x01,             // MOV [0x0104], CX
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let sectors_per_cluster = harness::result_word(&machine.bus, 0);
    assert_eq!(
        sectors_per_cluster, 1,
        "Virtual Z: drive should report a synthetic 1 sector per cluster, got {}",
        sectors_per_cluster
    );

    let free_clusters = harness::result_word(&machine.bus, 2);
    assert_eq!(
        free_clusters, 0,
        "Virtual Z: drive should report no free clusters, got {}",
        free_clusters
    );

    let bytes_per_sector = harness::result_word(&machine.bus, 4);
    assert_eq!(
        bytes_per_sector, 512,
        "Virtual Z: drive should report a synthetic 512-byte sector, got {}",
        bytes_per_sector
    );

    let total_clusters = harness::result_word(&machine.bus, 6);
    assert_eq!(
        total_clusters, 0,
        "Virtual Z: drive should report zero total clusters, got {}",
        total_clusters
    );
}

#[test]
fn get_extended_country_info() {
    let mut machine = harness::boot_hle();
    let buffer_offset: u16 = harness::INJECT_RESULT_OFFSET + 0x10;
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        0xB4, 0x65,                                              // MOV AH, 65h
        0xB0, 0x01,                                              // MOV AL, 01h (get info)
        0xBB, 0xFF, 0xFF,                                        // MOV BX, FFFFh (current codepage)
        0xBA, 0xFF, 0xFF,                                        // MOV DX, FFFFh (current country)
        0xB9, 0x29, 0x00,                                        // MOV CX, 0029h (buffer size 41)
        0xBF, buffer_offset as u8, (buffer_offset >> 8) as u8,   // MOV DI, buffer_offset
        0x8C, 0xD8,                                              // MOV AX, DS
        0x8E, 0xC0,                                              // MOV ES, AX
        0xB4, 0x65,                                              // MOV AH, 65h
        0xB0, 0x01,                                              // MOV AL, 01h
        0xCD, 0x21,                                              // INT 21h
        0x9C,                                                    // PUSHF
        0x58,                                                    // POP AX
        0xA3, 0x00, 0x01,                                        // MOV [0x0100], AX (flags)
        0x89, 0x0E, 0x02, 0x01,                                  // MOV [0x0102], CX (bytes returned)
        0xFA,                                                    // CLI
        0xF4,                                                    // HLT
    ];
    harness::inject_and_run(&mut machine, &code);

    let flags = harness::result_word(&machine.bus, 0);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 0,
        "Get extended country info should succeed (CF=0), flags={:#06X}",
        flags
    );

    // Buffer: byte 0 = info ID (should be 01h).
    let info_id = harness::read_byte(&machine.bus, harness::INJECT_RESULT_BASE + 0x10);
    assert_eq!(
        info_id, 0x01,
        "Extended country info ID should be 0x01, got {:#04X}",
        info_id
    );

    // Bytes 3-4 (WORD) = country code.
    let country = harness::read_word(&machine.bus, harness::INJECT_RESULT_BASE + 0x10 + 3);
    assert_eq!(
        country, 81,
        "Extended country code should be 81 (Japan), got {}",
        country
    );
}

#[test]
fn allocate_memory_insufficient() {
    let mut machine = harness::boot_hle();
    // Request 0xFFFF paragraphs (more than available).
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBB, 0xFF, 0xFF,                   // MOV BX, FFFFh
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (error code)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (largest available)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (flags)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 4);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 1,
        "Allocation of 0xFFFF paragraphs should fail (CF=1), flags={:#06X}",
        flags
    );

    let error_code = harness::result_word(&machine.bus, 0);
    assert_eq!(
        error_code, 8,
        "Error code should be 8 (insufficient memory), got {}",
        error_code
    );

    let largest = harness::result_word(&machine.bus, 2);
    assert!(
        largest > 0,
        "Largest available block should be > 0, got {}",
        largest
    );
}

#[test]
fn allocate_memory_exact_fit() {
    let mut machine = harness::boot_hle();
    // First query largest available block, then allocate exactly that much.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Try to allocate 0xFFFF to get largest available in BX
        0xBB, 0xFF, 0xFF,                   // MOV BX, FFFFh
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (largest available)
        // Now allocate exactly that amount
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (segment)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (flags)
        // Free
        0xA1, 0x02, 0x01,                   // MOV AX, [0x0102]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 4);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 0,
        "Exact-fit allocation should succeed (CF=0), flags={:#06X}",
        flags
    );

    let segment = harness::result_word(&machine.bus, 2);
    let linear = harness::far_to_linear(segment, 0);
    assert!(
        linear > 0 && linear < 0xA0000,
        "Exact-fit segment should be in conventional memory, got {:#06X}",
        segment
    );
}

#[test]
fn allocate_multiple_blocks() {
    let mut machine = harness::boot_hle();
    // Allocate 3 blocks of 0x10, 0x20, 0x30 paragraphs.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Block A: 0x10 paragraphs
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (seg A)
        // Block B: 0x20 paragraphs
        0xBB, 0x20, 0x00,                   // MOV BX, 0020h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (seg B)
        // Block C: 0x30 paragraphs
        0xBB, 0x30, 0x00,                   // MOV BX, 0030h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (seg C)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x06, 0x01,                   // MOV [0x0106], AX (flags)
        // Free all three
        0xA1, 0x00, 0x01,                   // MOV AX, [0x0100]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49, 0xCD, 0x21,             // AH=49h INT 21h
        0xA1, 0x02, 0x01,                   // MOV AX, [0x0102]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49, 0xCD, 0x21,             // AH=49h INT 21h
        0xA1, 0x04, 0x01,                   // MOV AX, [0x0104]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49, 0xCD, 0x21,             // AH=49h INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 6);
    let carry = flags & 0x0001;
    assert_eq!(carry, 0, "Third allocation should succeed (CF=0)");

    let seg_a = harness::result_word(&machine.bus, 0);
    let seg_b = harness::result_word(&machine.bus, 2);
    let seg_c = harness::result_word(&machine.bus, 4);

    assert_ne!(seg_a, seg_b, "Segments A and B should differ");
    assert_ne!(seg_b, seg_c, "Segments B and C should differ");
    assert!(
        seg_a < seg_b,
        "Segment A ({:#06X}) should be < B ({:#06X})",
        seg_a,
        seg_b
    );
    assert!(
        seg_b < seg_c,
        "Segment B ({:#06X}) should be < C ({:#06X})",
        seg_b,
        seg_c
    );
}

#[test]
fn allocate_after_free_reuses_block() {
    let mut machine = harness::boot_hle();
    // Allocate, free, re-allocate same size. First-fit should return the same segment.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (seg1)
        // Free
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        // Re-allocate 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (seg2)
        // Free
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let seg1 = harness::result_word(&machine.bus, 0);
    let seg2 = harness::result_word(&machine.bus, 2);
    assert_eq!(
        seg1, seg2,
        "Re-allocation should reuse the same block: seg1={:#06X} seg2={:#06X}",
        seg1, seg2
    );
}

#[test]
fn free_invalid_segment() {
    let mut machine = harness::boot_hle();
    // Free a segment that was never allocated (0x0050).
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x50, 0x00,                   // MOV AX, 0050h
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (error code)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (flags)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 2);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 1,
        "Freeing invalid segment should fail (CF=1), flags={:#06X}",
        flags
    );

    let error_code = harness::result_word(&machine.bus, 0);
    assert_eq!(
        error_code, 9,
        "Error code should be 9 (invalid memory block), got {}",
        error_code
    );
}

#[test]
fn free_already_free_block() {
    let mut machine = harness::boot_hle();
    // Allocate, free, then free again.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (segment)
        // Free first time
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        // Free second time (same segment)
        0xA1, 0x00, 0x01,                   // MOV AX, [0x0100]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (error code)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (flags)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 4);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 1,
        "Double-free should fail (CF=1), flags={:#06X}",
        flags
    );

    let error_code = harness::result_word(&machine.bus, 2);
    assert_eq!(
        error_code, 9,
        "Double-free error should be 9 (invalid block), got {}",
        error_code
    );
}

#[test]
fn resize_memory_grow() {
    let mut machine = harness::boot_hle();
    // Allocate a small block then grow it.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (segment)
        // Resize to 0x20
        0x8E, 0xC0,                         // MOV ES, AX
        0xBB, 0x20, 0x00,                   // MOV BX, 0020h
        0xB4, 0x4A,                         // MOV AH, 4Ah
        0xCD, 0x21,                         // INT 21h
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (flags)
        // Free
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 2);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 0,
        "Memory grow (0x10 -> 0x20) should succeed (CF=0), flags={:#06X}",
        flags
    );
}

#[test]
fn resize_memory_grow_fail() {
    let mut machine = harness::boot_hle();
    // Allocate two adjacent blocks, then try to grow the first past the second.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate block A: 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (seg A)
        // Allocate block B: 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (seg B)
        // Try to resize A to 0x30 (should fail, B is in the way)
        0xA1, 0x00, 0x01,                   // MOV AX, [0x0100]
        0x8E, 0xC0,                         // MOV ES, AX
        0xBB, 0x30, 0x00,                   // MOV BX, 0030h
        0xB4, 0x4A,                         // MOV AH, 4Ah
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (error code)
        0x89, 0x1E, 0x06, 0x01,             // MOV [0x0106], BX (max available)
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x08, 0x01,                   // MOV [0x0108], AX (flags)
        // Clean up: free both blocks
        0xA1, 0x00, 0x01,                   // MOV AX, [0x0100]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49, 0xCD, 0x21,             // free A
        0xA1, 0x02, 0x01,                   // MOV AX, [0x0102]
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49, 0xCD, 0x21,             // free B
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 8);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 1,
        "Growing A past B should fail (CF=1), flags={:#06X}",
        flags
    );

    let error_code = harness::result_word(&machine.bus, 4);
    assert_eq!(
        error_code, 8,
        "Error should be 8 (insufficient memory), got {}",
        error_code
    );

    let max_available = harness::result_word(&machine.bus, 6);
    assert_eq!(
        max_available, 0x10,
        "Max available should be current size (0x10), got {:#06X}",
        max_available
    );
}

#[test]
fn resize_memory_to_same_size() {
    let mut machine = harness::boot_hle();
    // Allocate then resize to the same size (no-op).
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (segment)
        // Resize to 0x10 (same)
        0x8E, 0xC0,                         // MOV ES, AX
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x4A,                         // MOV AH, 4Ah
        0xCD, 0x21,                         // INT 21h
        0x9C,                               // PUSHF
        0x58,                               // POP AX
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (flags)
        // Free
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let flags = harness::result_word(&machine.bus, 2);
    let carry = flags & 0x0001;
    assert_eq!(
        carry, 0,
        "Resize to same size should succeed (CF=0), flags={:#06X}",
        flags
    );
}

#[test]
fn mcb_chain_intact_after_alloc_free() {
    let mut machine = harness::boot_hle();
    // Allocate a block, free it, then verify the MCB chain is well-formed.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 0x10
        0xBB, 0x10, 0x00,                   // MOV BX, 0010h
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x21,                         // INT 21h
        // Free
        0x8E, 0xC0,                         // MOV ES, AX
        0xB4, 0x49,                         // MOV AH, 49h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    // Walk MCB chain and verify integrity
    let sysvars = harness::get_sysvars_address(&mut machine);
    let first_mcb_segment = harness::read_word(&machine.bus, sysvars - 2);
    let mut mcb_addr = harness::far_to_linear(first_mcb_segment, 0);
    let mut count = 0u32;

    for _ in 0..1000 {
        let block_type = harness::read_byte(&machine.bus, mcb_addr);
        let size = harness::read_word(&machine.bus, mcb_addr + 3);

        assert!(
            block_type == 0x4D || block_type == 0x5A,
            "MCB #{} at {:#010X} has invalid type {:#04X}",
            count,
            mcb_addr,
            block_type
        );

        count += 1;

        if block_type == 0x5A {
            break;
        }

        let current_segment = mcb_addr >> 4;
        let next_segment = current_segment + size as u32 + 1;
        mcb_addr = next_segment << 4;
    }

    assert!(
        count >= 3,
        "MCB chain should have at least 3 entries, got {}",
        count
    );
}

#[test]
fn free_coalesces_adjacent_blocks() {
    let mut machine = harness::boot_hle();
    // Allocate A, B, C. Free A, C, then B. After freeing B, the three
    // freed blocks should coalesce into one (since A and C were already free).
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate A: 0x10
        0xBB, 0x10, 0x00,
        0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x00, 0x01,                   // [0x0100] = seg A
        // Allocate B: 0x10
        0xBB, 0x10, 0x00,
        0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x02, 0x01,                   // [0x0102] = seg B
        // Allocate C: 0x10
        0xBB, 0x10, 0x00,
        0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x04, 0x01,                   // [0x0104] = seg C
        // Count MCBs before freeing (store at [0x0106])
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let seg_a = harness::result_word(&machine.bus, 0);
    let seg_b = harness::result_word(&machine.bus, 2);
    let seg_c = harness::result_word(&machine.bus, 4);

    // Free A
    #[rustfmt::skip]
    let free_a: Vec<u8> = vec![
        0xB8, (seg_a & 0xFF) as u8, (seg_a >> 8) as u8,
        0x8E, 0xC0,
        0xB4, 0x49, 0xCD, 0x21,
        0xFA, 0xF4,
    ];
    harness::inject_and_run(&mut machine, &free_a);

    // Free C
    #[rustfmt::skip]
    let free_c: Vec<u8> = vec![
        0xB8, (seg_c & 0xFF) as u8, (seg_c >> 8) as u8,
        0x8E, 0xC0,
        0xB4, 0x49, 0xCD, 0x21,
        0xFA, 0xF4,
    ];
    harness::inject_and_run(&mut machine, &free_c);

    // Count MCBs before freeing B
    let sysvars = harness::get_sysvars_address(&mut machine);
    let first_mcb = harness::read_word(&machine.bus, sysvars - 2);
    let count_before = count_mcb_entries(&machine.bus, first_mcb);

    // Free B (should coalesce A+B+C into one free block)
    #[rustfmt::skip]
    let free_b: Vec<u8> = vec![
        0xB8, (seg_b & 0xFF) as u8, (seg_b >> 8) as u8,
        0x8E, 0xC0,
        0xB4, 0x49, 0xCD, 0x21,
        0xFA, 0xF4,
    ];
    harness::inject_and_run(&mut machine, &free_b);

    let count_after = count_mcb_entries(&machine.bus, first_mcb);
    assert!(
        count_after < count_before,
        "MCB chain should have fewer entries after coalescing: before={}, after={}",
        count_before,
        count_after
    );
}

#[test]
fn allocate_best_fit_strategy() {
    let mut machine = harness::boot_hle();
    // Allocate A(0x10), B(0x10), C(0x30), D(0x10). Free A (0x10 hole). Free C (0x30 hole).
    // Set best-fit. Allocate 0x10. Should go into the A hole (smaller), not the C hole.
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate A: 0x10
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x00, 0x01,                   // [0x0100] = seg A
        // Allocate B: 0x10
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x02, 0x01,                   // [0x0102] = seg B
        // Allocate C: 0x30
        0xBB, 0x30, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x04, 0x01,                   // [0x0104] = seg C
        // Allocate D: 0x10
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x06, 0x01,                   // [0x0106] = seg D
        // Free A
        0xA1, 0x00, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        // Free C
        0xA1, 0x04, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        // Set strategy to best-fit (1)
        0xB4, 0x58, 0xB0, 0x01,             // MOV AH, 58h; MOV AL, 01h
        0xBB, 0x01, 0x00,                   // MOV BX, 0001h
        0xCD, 0x21,                         // INT 21h
        // Allocate 0x10 -- should go into the 0x10 hole (A's old spot)
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x08, 0x01,                   // [0x0108] = seg E (best-fit result)
        // Restore strategy to first-fit (0)
        0xB4, 0x58, 0xB0, 0x01,
        0xBB, 0x00, 0x00,
        0xCD, 0x21,
        // Clean up: free E, B, D
        0xA1, 0x08, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        0xA1, 0x02, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        0xA1, 0x06, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        0xFA, 0xF4,
    ];
    harness::inject_and_run(&mut machine, code);

    let seg_a = harness::result_word(&machine.bus, 0);
    let seg_e = harness::result_word(&machine.bus, 8);

    // Best-fit should have chosen A's old hole (0x10 paragraphs) over C's hole (0x30).
    assert_eq!(
        seg_e, seg_a,
        "Best-fit should reuse the smaller hole (A's old segment {:#06X}), got {:#06X}",
        seg_a, seg_e
    );
}

#[test]
fn allocate_last_fit_strategy() {
    let mut machine = harness::boot_hle();
    // Allocate A(0x10), B(0x10), C(0x10). Free A and B to create two equal holes
    // separated by C. Set last-fit. Allocate 0x10. Should NOT pick A (first hole).
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate A: 0x10
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x00, 0x01,                   // [0x0100] = seg A
        // Allocate B: 0x10
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x02, 0x01,                   // [0x0102] = seg B
        // Allocate C: 0x10 (separator so A and B don't coalesce when freed)
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x04, 0x01,                   // [0x0104] = seg C
        // Free A
        0xA1, 0x00, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        // Free B
        0xA1, 0x02, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        // Set last-fit (2)
        0xB4, 0x58, 0xB0, 0x01,
        0xBB, 0x02, 0x00,
        0xCD, 0x21,
        // Allocate 0x10 -- last-fit should NOT pick A's hole (first in chain)
        0xBB, 0x10, 0x00, 0xB4, 0x48, 0xCD, 0x21,
        0xA3, 0x06, 0x01,                   // [0x0106] = seg D (last-fit result)
        // Restore first-fit (0)
        0xB4, 0x58, 0xB0, 0x01,
        0xBB, 0x00, 0x00,
        0xCD, 0x21,
        // Clean up: free D, C
        0xA1, 0x06, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        0xA1, 0x04, 0x01, 0x8E, 0xC0, 0xB4, 0x49, 0xCD, 0x21,
        0xFA, 0xF4,
    ];
    harness::inject_and_run(&mut machine, code);

    let seg_a = harness::result_word(&machine.bus, 0);
    let seg_d = harness::result_word(&machine.bus, 6);

    assert_ne!(
        seg_d, seg_a,
        "Last-fit should NOT pick the first hole (A at {:#06X}), got {:#06X}",
        seg_a, seg_d
    );
    assert!(
        seg_d > seg_a,
        "Last-fit result ({:#06X}) should be at a higher address than A ({:#06X})",
        seg_d,
        seg_a
    );
}

fn count_mcb_entries(bus: &machine::Pc9801Bus, first_mcb_segment: u16) -> u32 {
    let mut count = 0u32;
    let mut mcb_addr = harness::far_to_linear(first_mcb_segment, 0);
    for _ in 0..1000 {
        let block_type = harness::read_byte(bus, mcb_addr);
        if block_type != 0x4D && block_type != 0x5A {
            break;
        }
        count += 1;
        if block_type == 0x5A {
            break;
        }
        let size = harness::read_word(bus, mcb_addr + 3);
        let current_segment = mcb_addr >> 4;
        mcb_addr = (current_segment + size as u32 + 1) << 4;
    }
    count
}
