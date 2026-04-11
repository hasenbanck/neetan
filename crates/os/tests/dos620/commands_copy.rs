use std::fs;

use crate::{file_copy_harness::*, harness::*};

#[test]
fn copy_single_file() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY TESTFILE.TXT COPIED.TXT\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"TYPE COPIED.TXT\r");
    run_until_prompt(&mut machine);

    let hello = [
        0x0048, 0x0045, 0x004C, 0x004C, 0x004F, 0x0020, 0x0057, 0x004F, 0x0052, 0x004C, 0x0044,
    ]; // "HELLO WORLD"
    assert!(
        find_string_in_text_vram(&machine.bus, &hello),
        "TYPE of copied file should show 'HELLO WORLD'"
    );
}

#[test]
fn copy_file_count() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY TESTFILE.TXT COPY2.TXT\r");
    run_until_prompt(&mut machine);

    let copied = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied),
        "COPY should show 'copied' message"
    );
}

#[test]
fn copy_nonexistent_source() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY NOFILE.TXT DEST.TXT\r");
    run_until_prompt(&mut machine);

    let not_found = [
        0x006E, 0x006F, 0x0074, 0x0020, 0x0066, 0x006F, 0x0075, 0x006E, 0x0064,
    ]; // "not found"
    assert!(
        find_string_in_text_vram(&machine.bus, &not_found),
        "COPY of nonexistent file should show error"
    );
}

#[test]
fn copy_with_verify() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // COPY /V should copy and verify
    type_string_long(&mut machine, b"COPY /V TESTFILE.TXT VERIFIED.TXT\r");
    run_until_prompt(&mut machine);

    let copied = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied),
        "COPY /V should show 'copied' message"
    );

    // Verify the file content is correct
    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"TYPE VERIFIED.TXT\r");
    run_until_prompt(&mut machine);

    let hello = [
        0x0048, 0x0045, 0x004C, 0x004C, 0x004F, 0x0020, 0x0057, 0x004F, 0x0052, 0x004C, 0x0044,
    ]; // "HELLO WORLD"
    assert!(
        find_string_in_text_vram(&machine.bus, &hello),
        "Verified copy should contain 'HELLO WORLD'"
    );
}

#[test]
fn copy_overwrite_with_y_flag() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // First copy
    type_string_long(&mut machine, b"COPY TESTFILE.TXT DEST.TXT\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // Copy again with /Y -- should overwrite without prompting
    type_string_long(&mut machine, b"COPY /Y TESTFILE.TXT DEST.TXT\r");
    run_until_prompt(&mut machine);

    let copied = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied),
        "COPY /Y should overwrite and show 'copied'"
    );
}

#[test]
fn copy_concatenation() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // Create a second file
    type_string_long(&mut machine, b"COPY TESTFILE.TXT PART2.TXT\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // Concatenate: COPY TESTFILE.TXT+PART2.TXT JOINED.TXT
    type_string_long(&mut machine, b"COPY TESTFILE.TXT+PART2.TXT JOINED.TXT\r");
    run_until_prompt(&mut machine);

    let copied = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied),
        "COPY with + concatenation should show 'copied'"
    );

    // Verify the joined file exists via DIR
    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);
    type_string(&mut machine.bus, b"DIR\r");
    run_until_prompt(&mut machine);

    let joined = [0x004A, 0x004F, 0x0049, 0x004E, 0x0045, 0x0044]; // "JOINED"
    assert!(
        find_string_in_text_vram(&machine.bus, &joined),
        "JOINED.TXT should appear in DIR after concatenation"
    );
}

#[test]
fn xcopy_recursive() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    // Create source structure: SRCDIR with a file
    type_string(&mut machine.bus, b"MD SRCDIR\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY TESTFILE.TXT SRCDIR\r");
    run_until_prompt(&mut machine);

    // Create dest dir
    type_string(&mut machine.bus, b"MD DSTDIR\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    // XCOPY /S SRCDIR DSTDIR should copy files recursively
    type_string_long(&mut machine, b"XCOPY SRCDIR DSTDIR\r");
    run_until_prompt(&mut machine);

    // Should show "File(s) copied"
    let copied_msg = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied_msg),
        "XCOPY should show 'copied' message"
    );

    // Verify file is in DSTDIR
    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);
    type_string(&mut machine.bus, b"DIR DSTDIR\r");
    run_until_prompt(&mut machine);

    let testfile = [
        0x0054, 0x0045, 0x0053, 0x0054, 0x0046, 0x0049, 0x004C, 0x0045,
    ]; // "TESTFILE"
    assert!(
        find_string_in_text_vram(&machine.bus, &testfile),
        "XCOPY should have copied TESTFILE.TXT into DSTDIR"
    );
}

#[test]
fn copy_multicluster_file_from_floppy_to_formatted_hdd_without_corruption() {
    let source_bytes = prng_bytes(RANDOM_FILE_SIZE);
    let floppy = create_random_file_floppy(&source_bytes);

    let hdd_path = make_temp_hdd_path("copy-repro");
    let hdd = create_empty_hdd(256);
    fs::write(&hdd_path, hdd.to_bytes()).expect("write temp HDD image");

    let mut machine = boot_hle_with_temp_hdd_and_floppy(&hdd_path, floppy);

    type_string_long(&mut machine, b"FORMAT A:\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY /V C:RAND.BIN A:\\\r");
    run_until_prompt(&mut machine);
    machine.bus.flush_hdd(0);

    let copied = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied),
        "COPY should report success before the on-disk bytes are checked"
    );

    let copied_bytes = extract_root_file(&hdd_path, &RANDOM_FILE_FCB)
        .expect("destination file should be readable");
    let mismatches = mismatch_offsets(&source_bytes, &copied_bytes, 8);

    assert_eq!(
        copied_bytes.len(),
        source_bytes.len(),
        "copied size mismatch"
    );
    assert!(
        mismatches.is_empty(),
        "COPY corrupted data: first mismatches at {mismatches:?}"
    );
    assert_eq!(
        copied_bytes, source_bytes,
        "COPY should preserve file contents"
    );
}

#[test]
fn copy_explicit_destination_path_uses_basename() {
    let source_bytes = prng_bytes(RANDOM_FILE_SIZE);
    let source_name = *b"COPYONE BIN";
    let destination_name = *b"TARGET  BIN";
    let floppy = create_random_file_floppy_with_name(&source_name, &source_bytes);

    let hdd_path = make_temp_hdd_path("copy-destination-path");
    let hdd = create_empty_hdd(256);
    fs::write(&hdd_path, hdd.to_bytes()).expect("write temp HDD image");

    let mut machine = boot_hle_with_temp_hdd_and_floppy(&hdd_path, floppy);

    type_string_long(&mut machine, b"FORMAT A:\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY /V C:COPYONE.BIN A:\\TARGET.BIN\r");
    run_until_prompt(&mut machine);
    machine.bus.flush_hdd(0);

    let copied_bytes = extract_root_file(&hdd_path, &destination_name)
        .expect("destination file should be created with the explicit basename");
    let mismatches = mismatch_offsets(&source_bytes, &copied_bytes, 8);

    assert_eq!(
        copied_bytes.len(),
        source_bytes.len(),
        "copy with explicit destination path should preserve file size"
    );
    assert!(
        mismatches.is_empty(),
        "copy with explicit destination path mismatches at {mismatches:?}"
    );
    assert_eq!(
        copied_bytes, source_bytes,
        "copy with explicit destination path should preserve file contents"
    );
}

#[test]
fn copy_broken_source_chain_reports_read_error() {
    let broken_name = *b"BROKEN  BIN";
    let floppy = create_broken_chain_floppy_with_name(&broken_name, 4096);

    let hdd_path = make_temp_hdd_path("copy-broken-chain");
    let hdd = create_empty_hdd(256);
    fs::write(&hdd_path, hdd.to_bytes()).expect("write temp HDD image");

    let mut machine = boot_hle_with_temp_hdd_and_floppy(&hdd_path, floppy);

    type_string_long(&mut machine, b"FORMAT A:\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"COPY C:BROKEN.BIN A:\\BROKEN.BIN\r");
    run_until_prompt(&mut machine);
    machine.bus.flush_hdd(0);

    let read_error = [
        0x0052, 0x0065, 0x0061, 0x0064, 0x0020, 0x0065, 0x0072, 0x0072, 0x006F, 0x0072,
    ]; // "Read error"
    assert!(
        find_string_in_text_vram(&machine.bus, &read_error),
        "COPY should report a read error for a truncated source chain"
    );
    assert!(
        extract_root_file(&hdd_path, &broken_name).is_err(),
        "COPY should not create a destination file after a read error"
    );
}
