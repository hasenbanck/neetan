use crate::{file_copy_harness::*, harness::*};

fn digest_hex(data: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    let mut digest = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut digest);

    let mut result = String::with_capacity(64);
    for byte in digest {
        result.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        result.push(char::from(b"0123456789abcdef"[(byte & 0x0F) as usize]));
    }
    result
}

fn text_codes(text: &str) -> Vec<u16> {
    text.bytes().map(u16::from).collect()
}

#[test]
fn b3sum_help_on_empty_args() {
    let mut machine = boot_hle();

    type_string(&mut machine.bus, b"B3SUM\r");
    run_until_prompt(&mut machine);

    assert!(
        find_row_containing(&machine.bus, "Computes BLAKE3 hashes of files.").is_some(),
        "B3SUM without arguments should print help text"
    );
}

#[test]
fn b3sum_help_on_help_flag() {
    let mut machine = boot_hle();

    type_string_long(&mut machine, b"B3SUM /?\r");
    run_until_prompt(&mut machine);

    assert!(
        find_row_containing(&machine.bus, "B3SUM [drive:][path]filename [...]").is_some(),
        "B3SUM /? should print usage text"
    );
}

#[test]
fn b3sum_hashes_single_file() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"B3SUM TESTFILE.TXT\r");
    run_until_prompt(&mut machine);

    let expected_line = format!("{}  TESTFILE.TXT", digest_hex(TEST_FILE_CONTENT));
    assert!(
        find_row_containing(&machine.bus, &expected_line).is_some(),
        "B3SUM should print the expected digest for TESTFILE.TXT"
    );
}

#[test]
fn b3sum_expands_wildcards_with_directory_prefix() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"MD HASHDIR\r");
    run_until_prompt(&mut machine);
    type_string_long(&mut machine, b"COPY TESTFILE.TXT HASHDIR\\ONE.TXT\r");
    run_until_prompt(&mut machine);
    type_string_long(&mut machine, b"COPY TESTFILE.TXT HASHDIR\\TWO.TXT\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);
    type_string_long(&mut machine, b"B3SUM HASHDIR\\*.TXT\r");
    run_until_prompt(&mut machine);

    let expected_digest = digest_hex(TEST_FILE_CONTENT);
    let one_line = format!("{expected_digest}  HASHDIR\\ONE.TXT");
    let two_line = format!("{expected_digest}  HASHDIR\\TWO.TXT");

    assert!(
        find_string_in_text_vram(&machine.bus, &text_codes(&one_line)),
        "B3SUM should print HASHDIR\\ONE.TXT"
    );
    assert!(
        find_string_in_text_vram(&machine.bus, &text_codes(&two_line)),
        "B3SUM should print HASHDIR\\TWO.TXT"
    );
}

#[test]
fn b3sum_stops_after_first_error() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"B3SUM TESTFILE.TXT NOFILE.TXT COMMAND.COM\r");
    run_until_prompt(&mut machine);

    let testfile_line = format!("{}  TESTFILE.TXT", digest_hex(TEST_FILE_CONTENT));
    let command_digest = digest_hex(TEST_COMMAND_COM);

    assert!(
        find_row_containing(&machine.bus, &testfile_line).is_some(),
        "B3SUM should hash the first valid file before the error"
    );
    assert!(
        find_row_containing(&machine.bus, "File not found").is_some(),
        "B3SUM should report the first missing file"
    );
    assert!(
        find_row_containing(&machine.bus, &command_digest).is_none(),
        "B3SUM should stop after the first error"
    );
}

#[test]
fn b3sum_rejects_directories() {
    let mut machine = boot_hle_with_floppy();
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"MD SUBDIR\r");
    run_until_prompt(&mut machine);
    type_string_long(&mut machine, b"B3SUM SUBDIR\r");
    run_until_prompt(&mut machine);

    assert!(
        find_row_containing(&machine.bus, "Access denied").is_some(),
        "B3SUM should reject directory arguments"
    );
}

#[test]
fn b3sum_reports_broken_cluster_chains() {
    let broken_name = *b"BROKEN  BIN";
    let floppy = create_broken_chain_floppy_with_name(&broken_name, 4096);
    let mut machine = boot_hle_with_floppy_image(floppy);

    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);
    type_string_long(&mut machine, b"B3SUM BROKEN.BIN\r");
    run_until_prompt(&mut machine);

    assert!(
        find_row_containing(&machine.bus, "Read error").is_some(),
        "B3SUM should fail on truncated cluster chains"
    );
}
