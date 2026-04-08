use crate::harness::*;

#[test]
fn shell_prompt_visible() {
    let machine = boot_hle();
    assert!(
        find_char_in_text_vram(&machine.bus, 0x003E),
        "prompt '>' should be visible in VRAM after boot"
    );
}

#[test]
fn shell_banner_displayed() {
    let machine = boot_hle();
    let neetan = [0x004E, 0x0065, 0x0065, 0x0074, 0x0061, 0x006E]; // "Neetan"
    assert!(
        find_string_in_text_vram(&machine.bus, &neetan),
        "boot banner 'Neetan' should be visible in VRAM"
    );
}

#[test]
fn shell_ver_command() {
    let mut machine = boot_hle();
    type_string(&mut machine.bus, b"VER\r");
    run_until_prompt(&mut machine);

    // Check for version string fragments in VRAM
    // "6.20" should appear
    let version = [0x0036, 0x002E, 0x0032, 0x0030]; // "6.20"
    assert!(
        find_string_in_text_vram(&machine.bus, &version),
        "VER command should display version '6.20'"
    );
}

#[test]
fn shell_cls_command() {
    let mut machine = boot_hle();
    // Before CLS, banner text should be present
    let neetan = [0x004E, 0x0065, 0x0065, 0x0074, 0x0061, 0x006E];
    assert!(find_string_in_text_vram(&machine.bus, &neetan));

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // After CLS, the banner should be gone (screen was cleared)
    // But a new prompt should be present
    assert!(
        find_char_in_text_vram(&machine.bus, 0x003E),
        "prompt should reappear after CLS"
    );
    // The banner from boot should no longer be visible (screen was cleared,
    // only the new prompt remains)
    assert!(
        !find_string_in_text_vram(&machine.bus, &neetan),
        "boot banner should be gone after CLS"
    );
}

#[test]
fn shell_echo_command() {
    let mut machine = boot_hle();
    type_string(&mut machine.bus, b"ECHO HELLO\r");
    run_until_prompt(&mut machine);

    let hello = [0x0048, 0x0045, 0x004C, 0x004C, 0x004F]; // "HELLO"
    assert!(
        find_string_in_text_vram(&machine.bus, &hello),
        "ECHO HELLO should display 'HELLO' in VRAM"
    );
}

#[test]
fn shell_cd_command() {
    let mut machine = boot_hle();
    type_string(&mut machine.bus, b"CD\r");
    run_until_prompt(&mut machine);

    // Should display current path, which starts with the drive letter and backslash
    let backslash = [0x005C]; // "\"
    assert!(
        find_string_in_text_vram(&machine.bus, &backslash),
        "CD should display current path with backslash"
    );
}

#[test]
fn shell_set_command() {
    let mut machine = boot_hle();
    type_string(&mut machine.bus, b"SET\r");
    run_until_prompt(&mut machine);

    // Should display COMSPEC= from the environment
    let comspec = [0x0043, 0x004F, 0x004D, 0x0053, 0x0050, 0x0045, 0x0043]; // "COMSPEC"
    assert!(
        find_string_in_text_vram(&machine.bus, &comspec),
        "SET should display 'COMSPEC' from environment"
    );
}

#[test]
fn shell_bad_command() {
    let mut machine = boot_hle();
    type_string(&mut machine.bus, b"NOSUCHCMD\r");
    run_until_prompt(&mut machine);

    // Unknown commands should just return to prompt (no crash)
    assert!(
        find_char_in_text_vram(&machine.bus, 0x003E),
        "prompt should be visible after bad command"
    );
}

#[test]
fn shell_line_editing() {
    let mut machine = boot_hle();

    // Type "VEER", move left twice, delete the extra 'E', then press Enter.
    // This should produce "VER" which executes the version command.
    type_string(&mut machine.bus, b"VEER");
    type_special_key(&mut machine.bus, SCAN_LEFT);
    type_special_key(&mut machine.bus, SCAN_LEFT);
    type_special_key(&mut machine.bus, SCAN_DELETE);
    type_string(&mut machine.bus, b"\r");
    run_until_prompt(&mut machine);

    let version = [0x0036, 0x002E, 0x0032, 0x0030]; // "6.20"
    assert!(
        find_string_in_text_vram(&machine.bus, &version),
        "line editing should produce working VER command"
    );
}

#[test]
fn shell_history() {
    let mut machine = boot_hle();

    // Execute VER command.
    type_string(&mut machine.bus, b"VER\r");
    run_until_prompt(&mut machine);

    // Clear screen so we can verify the recalled command produces fresh output.
    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // Press up arrow twice (past CLS to VER), then Enter.
    type_special_key(&mut machine.bus, SCAN_UP);
    type_special_key(&mut machine.bus, SCAN_UP);
    type_string(&mut machine.bus, b"\r");
    run_until_prompt(&mut machine);

    let version = [0x0036, 0x002E, 0x0032, 0x0030]; // "6.20"
    assert!(
        find_string_in_text_vram(&machine.bus, &version),
        "history recall should re-execute VER command"
    );
}
