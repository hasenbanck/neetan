use crate::harness::*;

#[test]
fn format_floppy_shows_complete() {
    let mut machine = boot_hle_with_two_floppies();

    type_string_long(&mut machine, b"FORMAT B:\r");
    // Wait for the confirmation prompt to appear
    machine.run_for(10_000_000);
    // Confirm with Y
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    let complete = [
        0x0046, 0x006F, 0x0072, 0x006D, 0x0061, 0x0074, 0x0020, 0x0063, 0x006F, 0x006D, 0x0070,
        0x006C, 0x0065, 0x0074, 0x0065,
    ]; // "Format complete"
    assert!(
        find_string_in_text_vram(&machine.bus, &complete),
        "FORMAT should show 'Format complete'"
    );
}

#[test]
fn format_floppy_quick() {
    let mut machine = boot_hle_with_two_floppies();

    type_string_long(&mut machine, b"FORMAT B: /Q\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    let complete = [
        0x0046, 0x006F, 0x0072, 0x006D, 0x0061, 0x0074, 0x0020, 0x0063, 0x006F, 0x006D, 0x0070,
        0x006C, 0x0065, 0x0074, 0x0065,
    ]; // "Format complete"
    assert!(
        find_string_in_text_vram(&machine.bus, &complete),
        "FORMAT /Q should show 'Format complete'"
    );

    // Verify the disk is mountable (DIR B: shows directory header)
    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR B:\r");
    run_until_prompt(&mut machine);

    // "Directory of" should appear (volume mounted successfully)
    let directory_of = [
        0x0044, 0x0069, 0x0072, 0x0065, 0x0063, 0x0074, 0x006F, 0x0072, 0x0079, 0x0020, 0x006F,
        0x0066,
    ]; // "Directory of"
    assert!(
        find_string_in_text_vram(&machine.bus, &directory_of),
        "DIR B: after FORMAT /Q should show 'Directory of' (volume mounts successfully)"
    );
}

#[test]
fn format_no_drive_argument() {
    let mut machine = boot_hle_with_floppy();

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"FORMAT\r");
    run_until_prompt(&mut machine);

    let missing = [
        0x0052, 0x0065, 0x0071, 0x0075, 0x0069, 0x0072, 0x0065, 0x0064,
    ]; // "Required"
    assert!(
        find_string_in_text_vram(&machine.bus, &missing),
        "FORMAT with no args should show 'Required parameter missing'"
    );
}

#[test]
fn format_floppy_then_dir() {
    let mut machine = boot_hle_with_two_floppies();

    // Full format B:
    type_string_long(&mut machine, b"FORMAT B:\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // DIR B: should work (volume remounts successfully)
    type_string(&mut machine.bus, b"DIR B:\r");
    run_until_prompt(&mut machine);

    // "Directory of" should appear (volume mounted successfully)
    let directory_of = [
        0x0044, 0x0069, 0x0072, 0x0065, 0x0063, 0x0074, 0x006F, 0x0072, 0x0079, 0x0020, 0x006F,
        0x0066,
    ]; // "Directory of"
    assert!(
        find_string_in_text_vram(&machine.bus, &directory_of),
        "DIR B: after FORMAT should show 'Directory of' (volume remounts)"
    );
}
