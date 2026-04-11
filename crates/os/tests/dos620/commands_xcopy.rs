use std::fs;

use crate::{file_copy_harness::*, harness::*};

#[test]
fn xcopy_copies_multicluster_file() {
    let source_bytes = prng_bytes(RANDOM_FILE_SIZE);
    let floppy = create_random_file_floppy(&source_bytes);

    let hdd_path = make_temp_hdd_path("xcopy-repro");
    let hdd = create_empty_hdd(256);
    fs::write(&hdd_path, hdd.to_bytes()).expect("write temp HDD image");

    let mut machine = boot_hle_with_temp_hdd_and_floppy(&hdd_path, floppy);

    type_string_long(&mut machine, b"FORMAT A:\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"XCOPY C:RAND.BIN A:\\\r");
    run_until_prompt(&mut machine);
    machine.bus.flush_hdd(0);

    let copied = [0x0063, 0x006F, 0x0070, 0x0069, 0x0065, 0x0064]; // "copied"
    assert!(
        find_string_in_text_vram(&machine.bus, &copied),
        "XCOPY should report success before the on-disk bytes are checked"
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
        "XCOPY corrupted data: first mismatches at {mismatches:?}"
    );
    assert_eq!(
        copied_bytes, source_bytes,
        "XCOPY should preserve file contents"
    );
}

#[test]
fn xcopy_broken_source_chain_reports_read_error() {
    let broken_name = *b"BROKEN  BIN";
    let floppy = create_broken_chain_floppy_with_name(&broken_name, 4096);

    let hdd_path = make_temp_hdd_path("xcopy-broken-chain");
    let hdd = create_empty_hdd(256);
    fs::write(&hdd_path, hdd.to_bytes()).expect("write temp HDD image");

    let mut machine = boot_hle_with_temp_hdd_and_floppy(&hdd_path, floppy);

    type_string_long(&mut machine, b"FORMAT A:\r");
    machine.run_for(10_000_000);
    type_string(&mut machine.bus, b"Y");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"XCOPY C:BROKEN.BIN A:\\\r");
    run_until_prompt(&mut machine);
    machine.bus.flush_hdd(0);

    let read_error = [
        0x0052, 0x0065, 0x0061, 0x0064, 0x0020, 0x0065, 0x0072, 0x0072, 0x006F, 0x0072,
    ]; // "Read error"
    assert!(
        find_string_in_text_vram(&machine.bus, &read_error),
        "XCOPY should report a read error for a truncated source chain"
    );
    assert!(
        extract_root_file(&hdd_path, &broken_name).is_err(),
        "XCOPY should not create a destination file after a read error"
    );
}
