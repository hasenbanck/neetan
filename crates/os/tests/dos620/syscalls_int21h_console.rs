use crate::harness;

#[test]
fn display_character_02h() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x02,                         // MOV AH, 02h
        0xB2, 0x58,                         // MOV DL, 58h ('X')
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    assert!(
        harness::find_char_in_text_vram(&machine.bus, 0x0058),
        "Text VRAM should contain character 'X' (0x0058) after INT 21h/02h"
    );
}

#[test]
fn display_string_09h() {
    let mut machine = harness::boot_hle();
    // Place '$'-terminated string at 0x9000:0200.
    let string = b"TEST$";
    harness::write_bytes(&mut machine.bus, harness::INJECT_CODE_BASE + 0x200, string);

    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x09,                         // MOV AH, 09h
        0xBA, 0x00, 0x02,                   // MOV DX, 0200h
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let chars: Vec<u16> = b"TEST".iter().map(|&c| c as u16).collect();
    assert!(
        harness::find_string_in_text_vram(&machine.bus, &chars),
        "Text VRAM should contain consecutive 'T','E','S','T' after INT 21h/09h"
    );
}

#[test]
fn direct_console_output_06h() {
    let mut machine = harness::boot_hle();
    // Use a rare character to avoid false matches from the DOS prompt.
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x06,                         // MOV AH, 06h
        0xB2, 0x7E,                         // MOV DL, 7Eh ('~')
        0xCD, 0x21,                         // INT 21h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    assert!(
        harness::find_char_in_text_vram(&machine.bus, 0x007E),
        "Text VRAM should contain character '~' (0x007E) after INT 21h/06h"
    );
}

#[test]
fn fast_console_output_int29h() {
    let mut machine = harness::boot_hle();
    // Use a rare character to avoid false matches.
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB0, 0x7C,                         // MOV AL, 7Ch ('|')
        0xCD, 0x29,                         // INT 29h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    assert!(
        harness::find_char_in_text_vram(&machine.bus, 0x007C),
        "Text VRAM should contain character '|' (0x007C) after INT 29h"
    );
}
