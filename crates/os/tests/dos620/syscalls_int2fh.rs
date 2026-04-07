use crate::harness;

#[test]
fn windows_not_running() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x00, 0x16,                   // MOV AX, 1600h
        0xCD, 0x2F,                         // INT 2Fh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let al = harness::result_byte(&machine.bus, 0);
    assert_eq!(
        al, 0x00,
        "INT 2Fh/1600h AL should be 0x00 (no Windows), got {:#04X}",
        al
    );
}

#[test]
fn xms_check() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x00, 0x43,                   // MOV AX, 4300h
        0xCD, 0x2F,                         // INT 2Fh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let al = harness::result_byte(&machine.bus, 0);
    // TODO: Once a XMS memory manager is provided, we must test for it's presence here.
    // assert!(al, 0x80);
    assert_eq!(
        al, 0x00,
        "XMS check: AL should be 0x00 (not installed), got {:#04X}",
        al
    );
}

#[test]
fn doskey_check() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x00, 0x48,                   // MOV AX, 4800h
        0xCD, 0x2F,                         // INT 2Fh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let al = harness::result_byte(&machine.bus, 0);
    assert_eq!(
        al, 0x00,
        "DOSKEY check: AL should be 0x00 (not installed), got {:#04X}",
        al
    );
}

#[test]
fn hma_query() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x01, 0x4A,                   // MOV AX, 4A01h
        0xCD, 0x2F,                         // INT 2Fh
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let bx = harness::result_word(&machine.bus, 0);
    assert_eq!(
        bx, 0x0000,
        "HMA free space should be 0x0000 (no HMA), got {:#06X}",
        bx
    );
}
