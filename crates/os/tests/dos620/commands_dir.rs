use crate::harness::*;

#[test]
fn dir_basic_listing() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR\r");
    run_until_prompt(&mut machine);

    let testfile = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045,
    ]; // "TESTFILE"
    assert!(
        find_string_in_text_vram(&machine.bus, &testfile),
        "DIR should list TESTFILE"
    );

    let command = [0x0043, 0x004F, 0x004D, 0x004D, 0x0041, 0x004E, 0x0044]; // "COMMAND"
    assert!(
        find_string_in_text_vram(&machine.bus, &command),
        "DIR should list COMMAND"
    );
}

#[test]
fn dir_lists_cdrom_root() {
    let mut machine = boot_hle_with_cdrom();
    type_string(&mut machine.bus, b"Q:\r");
    run_until_prompt_ap(&mut machine);

    type_string(&mut machine.bus, b"DIR\r");
    run_until_prompt_ap(&mut machine);

    let readme = [0x0052, 0x0045, 0x0041, 0x0044, 0x004D, 0x0045]; // "README"
    assert!(
        find_string_in_text_vram(&machine.bus, &readme),
        "DIR on Q: should list README.TXT from the synthetic CD-ROM"
    );
}

#[test]
fn dir_wildcard_txt() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR *.TXT\r");
    run_until_prompt(&mut machine);

    let testfile = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045,
    ]; // "TESTFILE"
    assert!(
        find_string_in_text_vram(&machine.bus, &testfile),
        "DIR *.TXT should list TESTFILE"
    );

    let command_com = [
        0x0043, 0x004F, 0x004D, 0x004D, 0x0041, 0x004E, 0x0044, 0x0020, 0x0020, 0x0043, 0x004F,
        0x004D,
    ]; // "COMMAND  COM"
    assert!(
        !find_string_in_text_vram(&machine.bus, &command_com),
        "DIR *.TXT should not list COMMAND.COM"
    );
}

#[test]
fn dir_bare_format() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR /B\r");
    run_until_prompt(&mut machine);

    let testfile_txt = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045, 0x002E, 0x0054, 0x0058,
        0x0054,
    ]; // "TESTFILE.TXT"
    assert!(
        find_string_in_text_vram(&machine.bus, &testfile_txt),
        "DIR /B should show TESTFILE.TXT"
    );
}

#[test]
fn dir_file_count() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR\r");
    run_until_prompt(&mut machine);

    let files = [0x0066, 0x0069, 0x006C, 0x0065, 0x0028, 0x0073, 0x0029]; // "file(s)"
    assert!(
        find_string_in_text_vram(&machine.bus, &files),
        "DIR should show file(s) count in summary"
    );
}

#[test]
fn dir_nonexistent() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"DIR NOFILE.XYZ\r");
    run_until_prompt(&mut machine);

    let not_found = [
        0x004E, 0x006F, 0x0074, 0x0020, 0x0046, 0x006F, 0x0075, 0x006E, 0x0064,
    ]; // "Not Found"
    assert!(
        find_string_in_text_vram(&machine.bus, &not_found),
        "DIR NOFILE.XYZ should show 'Not Found'"
    );
}

#[test]
fn dir_sorted_by_name() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // /ON sorts by name; COMMAND.COM should come before TEST.COM and TESTFILE.TXT
    type_string(&mut machine.bus, b"DIR /ON\r");
    run_until_prompt(&mut machine);

    // Verify COMMAND appears (sorted output should still list all files)
    let command = [0x0043, 0x004F, 0x004D, 0x004D, 0x0041, 0x004E, 0x0044]; // "COMMAND"
    assert!(
        find_string_in_text_vram(&machine.bus, &command),
        "DIR /ON should list COMMAND"
    );

    let testfile = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045,
    ]; // "TESTFILE"
    assert!(
        find_string_in_text_vram(&machine.bus, &testfile),
        "DIR /ON should list TESTFILE"
    );
}

#[test]
fn dir_sorted_by_extension() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR /OE\r");
    run_until_prompt(&mut machine);

    // All files should still appear
    let files = [0x0066, 0x0069, 0x006C, 0x0065, 0x0028, 0x0073, 0x0029]; // "file(s)"
    assert!(
        find_string_in_text_vram(&machine.bus, &files),
        "DIR /OE should show file(s) in summary"
    );
}

#[test]
fn dir_sorted_by_size() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR /OS\r");
    run_until_prompt(&mut machine);

    let files = [0x0066, 0x0069, 0x006C, 0x0065, 0x0028, 0x0073, 0x0029]; // "file(s)"
    assert!(
        find_string_in_text_vram(&machine.bus, &files),
        "DIR /OS should show file(s) in summary"
    );
}

#[test]
fn dir_attr_filter_dirs_only() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // Create a subdirectory first
    type_string(&mut machine.bus, b"MD SUBDIR\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // /AD shows only directories
    type_string(&mut machine.bus, b"DIR /AD\r");
    run_until_prompt(&mut machine);

    let subdir = [0x0053, 0x0055, 0x0042, 0x0044, 0x0049, 0x0052]; // "SUBDIR"
    assert!(
        find_string_in_text_vram(&machine.bus, &subdir),
        "DIR /AD should show SUBDIR"
    );

    // Regular files should NOT appear
    let testfile_txt = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045, 0x0020, 0x0054, 0x0058,
        0x0054,
    ]; // "TESTFILE TXT"
    assert!(
        !find_string_in_text_vram(&machine.bus, &testfile_txt),
        "DIR /AD should not show regular files"
    );
}

#[test]
fn dir_recursive() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // Create subdirectory and copy a file into it
    type_string(&mut machine.bus, b"MD SUB\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY TESTFILE.TXT SUB\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // DIR /S should list files in root and in SUB
    type_string(&mut machine.bus, b"DIR /S\r");
    run_until_prompt(&mut machine);

    // Should show TESTFILE in both root and subdirectory listing
    let testfile = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045,
    ]; // "TESTFILE"
    assert!(
        find_string_in_text_vram(&machine.bus, &testfile),
        "DIR /S should show TESTFILE"
    );

    // Should show the SUB directory marker too
    let sub = [0x0053, 0x0055, 0x0042]; // "SUB"
    assert!(
        find_string_in_text_vram(&machine.bus, &sub),
        "DIR /S should show SUB directory"
    );
}

#[test]
fn dir_in_subdirectory_shows_subdir_content() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // Create a subdirectory and change into it.
    type_string(&mut machine.bus, b"MD SUB\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CD SUB\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"DIR\r");
    run_until_prompt(&mut machine);

    // COMMAND.COM lives in the root directory only. It must NOT appear
    // when listing the subdirectory (regression: resolve_file_path used to
    // always start from root instead of the current directory).
    let command_com = [
        0x0043, 0x004F, 0x004D, 0x004D, 0x0041, 0x004E, 0x0044, 0x0020, 0x0020, 0x0043, 0x004F,
        0x004D,
    ]; // "COMMAND  COM"
    assert!(
        !find_string_in_text_vram(&machine.bus, &command_com),
        "DIR inside subdirectory must not list COMMAND.COM from root"
    );

    // The prompt should confirm we are in A:\SUB.
    let sub_prompt: [u16; 7] = [0x0041, 0x003A, 0x005C, 0x0053, 0x0055, 0x0042, 0x003E]; // "A:\SUB>"
    assert!(
        find_string_in_text_vram(&machine.bus, &sub_prompt),
        "Prompt should show A:\\SUB>"
    );
}
