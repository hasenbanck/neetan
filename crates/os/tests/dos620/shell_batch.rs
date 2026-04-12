use crate::harness::*;

fn boot_with_bat(bat_content: &[u8]) -> machine::Pc9801Ra {
    let floppy = create_test_floppy_with_program(b"TEST    BAT", bat_content);
    boot_hle_with_floppy_image(floppy)
}

fn current_shell_psp(machine: &mut machine::Pc9801Ra) -> u16 {
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x62,                         // MOV AH, 62h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0100h], BX
        0xC3,                               // RET
    ];
    inject_and_run_via_int28(machine, code, INJECT_BUDGET_DISK_IO);
    result_word(&machine.bus, 0)
}

#[test]
fn batch_echo() {
    let mut machine = boot_with_bat(b"ECHO BATCH OUTPUT\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let output = [
        0x0042, 0x0041, 0x0054, 0x0043, 0x0048, 0x0020, 0x004F, 0x0055, 0x0054, 0x0050, 0x0055,
        0x0054,
    ]; // "BATCH OUTPUT"
    assert!(
        find_string_in_text_vram(&machine.bus, &output),
        "batch file should display 'BATCH OUTPUT'"
    );
}

#[test]
fn batch_multi_line() {
    let mut machine = boot_with_bat(b"ECHO ALPHA\r\nECHO BETA\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let alpha = [0x0041, 0x004C, 0x0050, 0x0048, 0x0041]; // "ALPHA"
    let beta = [0x0042, 0x0045, 0x0054, 0x0041]; // "BETA"
    assert!(
        find_string_in_text_vram(&machine.bus, &alpha),
        "batch should display 'ALPHA'"
    );
    assert!(
        find_string_in_text_vram(&machine.bus, &beta),
        "batch should display 'BETA'"
    );
}

#[test]
fn batch_echo_off() {
    let mut machine = boot_with_bat(b"@ECHO OFF\r\nECHO QUIET\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let quiet = [0x0051, 0x0055, 0x0049, 0x0045, 0x0054]; // "QUIET"
    assert!(
        find_string_in_text_vram(&machine.bus, &quiet),
        "batch should display 'QUIET'"
    );
}

#[test]
fn batch_goto() {
    let mut machine = boot_with_bat(b"GOTO SKIP\r\nECHO BAD\r\n:SKIP\r\nECHO GOOD\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let good = [0x0047, 0x004F, 0x004F, 0x0044]; // "GOOD"
    let bad = [0x0042, 0x0041, 0x0044]; // "BAD"
    assert!(
        find_string_in_text_vram(&machine.bus, &good),
        "GOTO should skip to label and display 'GOOD'"
    );
    assert!(
        !find_string_in_text_vram(&machine.bus, &bad),
        "'BAD' should not be displayed after GOTO"
    );
}

#[test]
fn batch_if_exist() {
    let mut machine = boot_with_bat(b"IF EXIST TESTFILE.TXT ECHO FOUND\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let found = [0x0046, 0x004F, 0x0055, 0x004E, 0x0044]; // "FOUND"
    assert!(
        find_string_in_text_vram(&machine.bus, &found),
        "IF EXIST should detect TESTFILE.TXT and display 'FOUND'"
    );
}

#[test]
fn batch_if_not_exist() {
    let mut machine = boot_with_bat(b"IF NOT EXIST NOFILE.TXT ECHO MISSING\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let missing = [0x004D, 0x0049, 0x0053, 0x0053, 0x0049, 0x004E, 0x0047]; // "MISSING"
    assert!(
        find_string_in_text_vram(&machine.bus, &missing),
        "IF NOT EXIST should display 'MISSING' for nonexistent file"
    );
}

#[test]
fn batch_if_errorlevel() {
    let mut machine = boot_with_bat(b"NOSUCHCMD\r\nIF ERRORLEVEL 1 ECHO ERROR\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let error = [0x0045, 0x0052, 0x0052, 0x004F, 0x0052]; // "ERROR"
    assert!(
        find_string_in_text_vram(&machine.bus, &error),
        "IF ERRORLEVEL should detect non-zero exit code and display 'ERROR'"
    );
}

#[test]
fn batch_if_errorlevel_after_external_program() {
    let floppy_a = create_test_floppy_with_program(
        b"TEST    BAT",
        b"B:\\RUNME\r\nIF ERRORLEVEL 66 ECHO EXITOK\r\n",
    );
    let floppy_b = create_test_floppy_with_program(b"RUNME   COM", TEST_COM_PROGRAM);
    let mut machine = boot_hle_with_two_floppy_images(floppy_a, floppy_b);
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let exitok = [0x0045, 0x0058, 0x0049, 0x0054, 0x004F, 0x004B]; // "EXITOK"
    assert!(
        find_string_in_text_vram(&machine.bus, &exitok),
        "batch should wait for external programs before evaluating following lines"
    );
}

#[test]
fn batch_params() {
    let mut machine = boot_with_bat(b"ECHO %1 %2\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string_long(&mut machine, b"TEST FOO BAR\r");
    run_until_prompt(&mut machine);

    let foo = [0x0046, 0x004F, 0x004F]; // "FOO"
    let bar = [0x0042, 0x0041, 0x0052]; // "BAR"
    assert!(
        find_string_in_text_vram(&machine.bus, &foo),
        "batch params should substitute 'FOO'"
    );
    assert!(
        find_string_in_text_vram(&machine.bus, &bar),
        "batch params should substitute 'BAR'"
    );
}

#[test]
fn batch_env_var() {
    let mut machine = boot_with_bat(b"ECHO %COMSPEC%\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    // COMSPEC should contain "COMMAND"
    let command = [0x0043, 0x004F, 0x004D, 0x004D, 0x0041, 0x004E, 0x0044]; // "COMMAND"
    assert!(
        find_string_in_text_vram(&machine.bus, &command),
        "batch %COMSPEC% should display COMMAND.COM path"
    );
}

#[test]
fn batch_rem() {
    let mut machine = boot_with_bat(b"REM This is a comment\r\nECHO VISIBLE\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let visible = [0x0056, 0x0049, 0x0053, 0x0049, 0x0042, 0x004C, 0x0045]; // "VISIBLE"
    assert!(
        find_string_in_text_vram(&machine.bus, &visible),
        "batch should display 'VISIBLE' after REM"
    );
}

#[test]
fn batch_pause() {
    let mut machine = boot_with_bat(b"ECHO BEFORE\r\nPAUSE\r\nECHO AFTER\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");

    // Run until "Press any key" appears
    let press = [0x0050, 0x0072, 0x0065, 0x0073, 0x0073]; // "Press"
    let max_cycles: u64 = 500_000_000;
    let check_interval: u64 = 100_000;
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(check_interval);
        if find_string_in_text_vram(&machine.bus, &press) {
            break;
        }
        assert!(
            total_cycles < max_cycles,
            "PAUSE should display 'Press any key'"
        );
    }

    // Send a keypress
    type_string(&mut machine.bus, b" ");
    run_until_prompt(&mut machine);

    let after = [0x0041, 0x0046, 0x0054, 0x0045, 0x0052]; // "AFTER"
    assert!(
        find_string_in_text_vram(&machine.bus, &after),
        "batch should continue after PAUSE and display 'AFTER'"
    );
}

#[test]
fn batch_drive_change_applies_before_following_lines() {
    let floppy_a = create_test_floppy_with_program(
        b"SWITCH  BAT",
        b"B:\r\nIF EXIST TARGET.TXT ECHO FOUND\r\n",
    );
    let floppy_b = create_test_floppy_with_program(b"TARGET  TXT", b"B-ONLY\r\n");
    let mut machine = boot_hle_with_two_floppy_images(floppy_a, floppy_b);

    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"SWITCH\r");
    run_until_prompt(&mut machine);

    let found = [0x0046, 0x004F, 0x0055, 0x004E, 0x0044]; // "FOUND"
    assert!(
        find_string_in_text_vram(&machine.bus, &found),
        "batch drive change should affect relative paths used by following lines"
    );
}

#[test]
fn batch_command_c_resumes_parent_batch() {
    let mut machine = boot_with_bat(b"COMMAND /C ECHO CHILD\r\nECHO PARENT\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let child = [0x0043, 0x0048, 0x0049, 0x004C, 0x0044]; // "CHILD"
    let parent = [0x0050, 0x0041, 0x0052, 0x0045, 0x004E, 0x0054]; // "PARENT"
    assert!(
        find_string_in_text_vram(&machine.bus, &child),
        "COMMAND /C inside a batch file should execute the child command"
    );
    assert!(
        find_string_in_text_vram(&machine.bus, &parent),
        "batch execution should resume after COMMAND /C returns"
    );
}

#[test]
fn batch_command_c_preserves_external_program_errorlevel() {
    let mut machine = boot_with_bat(b"COMMAND /C A:\\TEST.COM\r\nIF ERRORLEVEL 66 ECHO EXITOK\r\n");
    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let exitok = [0x0045, 0x0058, 0x0049, 0x0054, 0x004F, 0x004B]; // "EXITOK"
    assert!(
        find_string_in_text_vram(&machine.bus, &exitok),
        "COMMAND /C should preserve the external program exit code for the parent batch"
    );
}

#[test]
fn batch_command_c_bomb_reports_oom_and_returns_to_root_shell() {
    let mut machine = boot_with_bat(b"COMMAND /C TEST\r\n");
    let root_psp = current_shell_psp(&mut machine);

    type_string(&mut machine.bus, b"A:\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"CLS\r");
    run_until_prompt(&mut machine);

    type_string(&mut machine.bus, b"TEST\r");
    run_until_prompt(&mut machine);

    let oom = [
        0x0045, 0x0072, 0x0072, 0x006F, 0x0072, 0x0020, 0x006C, 0x006F, 0x0061, 0x0064, 0x0069,
        0x006E, 0x0067, 0x0020, 0x0070, 0x0072, 0x006F, 0x0067, 0x0072, 0x0061, 0x006D, 0x0020,
        0x0028, 0x0038, 0x0029,
    ]; // "Error loading program (8)"
    assert!(
        find_string_in_text_vram(&machine.bus, &oom),
        "Recursive COMMAND /C should stop with insufficient-memory error"
    );

    let restored_psp = current_shell_psp(&mut machine);
    assert_eq!(
        restored_psp, root_psp,
        "COMMAND /C bomb should unwind back to the root shell after OOM"
    );

    type_string(&mut machine.bus, b"VER\r");
    run_until_prompt(&mut machine);

    let version = [0x0036, 0x002E, 0x0032, 0x0030]; // "6.20"
    assert!(
        find_string_in_text_vram(&machine.bus, &version),
        "Root shell should still accept commands after recursive COMMAND /C OOM"
    );
}
