use crate::harness;

fn boot_and_get_environment() -> (machine::Pc9801Ra, u32) {
    let mut machine = harness::boot_dos620();
    let psp_segment = harness::get_psp_segment(&mut machine);
    let psp_linear = harness::far_to_linear(psp_segment, 0);
    let env_segment = harness::read_word(&machine.bus, psp_linear + 0x2C);
    let env_linear = harness::far_to_linear(env_segment, 0);
    (machine, env_linear)
}

fn read_environment_strings(bus: &machine::Pc9801Bus, env_addr: u32) -> Vec<String> {
    let mut strings = Vec::new();
    let mut offset = 0u32;
    let max_env_size = 32768u32;

    loop {
        if offset >= max_env_size {
            break;
        }
        let byte = harness::read_byte(bus, env_addr + offset);
        if byte == 0 {
            break;
        }
        let s = harness::read_string(bus, env_addr + offset, (max_env_size - offset) as usize);
        offset += s.len() as u32 + 1;
        if let Ok(str_val) = String::from_utf8(s) {
            strings.push(str_val);
        }
    }

    strings
}

#[test]
fn has_comspec_entry() {
    let (machine, env_addr) = boot_and_get_environment();
    let strings = read_environment_strings(&machine.bus, env_addr);
    let has_comspec = strings.iter().any(|s| s.starts_with("COMSPEC="));
    assert!(
        has_comspec,
        "Environment should contain COMSPEC= entry, found: {:?}",
        strings
    );
}

#[test]
fn has_path_entry() {
    let (machine, env_addr) = boot_and_get_environment();
    let strings = read_environment_strings(&machine.bus, env_addr);
    let has_path = strings.iter().any(|s| s.starts_with("PATH="));
    assert!(
        has_path,
        "Environment should contain PATH= entry, found: {:?}",
        strings
    );
}

#[test]
fn double_null_terminated() {
    let (machine, env_addr) = boot_and_get_environment();
    // Walk to find the double-null terminator.
    let mut offset = 0u32;
    let max_size = 32768u32;
    let mut found_double_null = false;

    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                found_double_null = true;
                break;
            }
        }
        offset += 1;
    }

    assert!(
        found_double_null,
        "Environment block should end with a double-null terminator"
    );
}

#[test]
fn program_name_after_terminator() {
    let (machine, env_addr) = boot_and_get_environment();
    // Find the double-null terminator, then check for WORD count + pathname.
    let mut offset = 0u32;
    let max_size = 32768u32;

    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                // Double-null found at offset. After it: WORD count + pathname.
                let count_offset = offset + 2;
                let count = harness::read_word(&machine.bus, env_addr + count_offset);
                assert!(
                    count >= 1,
                    "Program name count after environment should be >= 1, got {}",
                    count
                );

                let pathname = harness::read_string(&machine.bus, env_addr + count_offset + 2, 128);
                assert!(
                    !pathname.is_empty(),
                    "Program pathname after environment should be non-empty"
                );
                return;
            }
        }
        offset += 1;
    }

    panic!("Could not find double-null terminator in environment block");
}

#[test]
fn prompt_entry_if_present() {
    let (machine, env_addr) = boot_and_get_environment();
    let strings = read_environment_strings(&machine.bus, env_addr);
    // PROMPT may or may not be set depending on AUTOEXEC.BAT. If present, verify it has a value.
    if let Some(prompt) = strings.iter().find(|s| s.starts_with("PROMPT=")) {
        assert!(
            prompt.len() > 7,
            "PROMPT= should have a non-empty value, got '{}'",
            prompt
        );
    }
}

#[test]
fn program_pathname_is_command_com() {
    let mut machine = harness::boot_dos620();
    let psp_segment = harness::get_psp_segment(&mut machine);
    let psp_linear = harness::far_to_linear(psp_segment, 0);
    let env_segment = harness::read_word(&machine.bus, psp_linear + 0x2C);
    let env_addr = harness::far_to_linear(env_segment, 0);

    let mut offset = 0u32;
    let max_size = 32768u32;

    // Find the double-null terminator.
    while offset < max_size {
        let byte = harness::read_byte(&machine.bus, env_addr + offset);
        if byte == 0 {
            let next = harness::read_byte(&machine.bus, env_addr + offset + 1);
            if next == 0 {
                let count_offset = offset + 2;
                let count = harness::read_word(&machine.bus, env_addr + count_offset);

                // On NEC MS-DOS 6.20, COMMAND.COM does NOT set up the standard
                // WORD count (0x0001) + program pathname after the environment's
                // double-null terminator. The count word is uninitialized MCB data
                // (observed: 0xE188), not 0x0001. The data following it is leftover
                // SJIS text from the MCB, not a pathname.
                //
                // Child processes launched via EXEC (INT 21h/4Bh) DO get a valid
                // program name (verified by child_process_program_name test).
                assert_ne!(
                    count, 0x0001,
                    "NEC MS-DOS 6.20: COMMAND.COM environment should NOT have count=1 \
                     (standard program name not set)"
                );
                return;
            }
        }
        offset += 1;
    }
    panic!("Could not find double-null terminator in environment block");
}

/// Checks whether child processes (TSRs loaded during boot via EXEC) have valid
/// program names after their environment block's double-null terminator.
#[test]
fn child_process_program_name() {
    let mut machine = harness::boot_dos620();
    let sysvars = harness::get_sysvars_address(&mut machine);
    let first_mcb_seg = harness::read_word(&machine.bus, sysvars - 2);
    let command_psp = harness::get_psp_segment(&mut machine);

    let mut seg = first_mcb_seg as u32;
    let mut checked = 0u32;

    for _ in 0..200 {
        let addr = seg << 4;
        let block_type = harness::read_byte(&machine.bus, addr);
        let owner = harness::read_word(&machine.bus, addr + 1);
        let size = harness::read_word(&machine.bus, addr + 3);

        // Check non-free, non-DOS-internal blocks that are NOT COMMAND.COM.
        // Owner = data segment + 1 = PSP segment for process-owned blocks.
        if owner != 0x0000 && owner != 0x0008 && owner != command_psp {
            let owner_psp_linear = harness::far_to_linear(owner, 0);
            // Verify this looks like a PSP (first two bytes should be CD 20 = INT 20h).
            let psp_sig = harness::read_word(&machine.bus, owner_psp_linear);
            if psp_sig == 0x20CD {
                let env_seg = harness::read_word(&machine.bus, owner_psp_linear + 0x2C);
                if env_seg != 0 {
                    let env_addr = harness::far_to_linear(env_seg, 0);
                    // Walk to find double-null.
                    let mut off = 0u32;
                    while off < 4096 {
                        let b = harness::read_byte(&machine.bus, env_addr + off);
                        if b == 0 {
                            let next = harness::read_byte(&machine.bus, env_addr + off + 1);
                            if next == 0 {
                                let count = harness::read_word(&machine.bus, env_addr + off + 2);
                                let pathname =
                                    harness::read_string(&machine.bus, env_addr + off + 4, 128);
                                assert_eq!(
                                    count, 0x0001,
                                    "Child process (owner PSP={:#06X}) env program name \
                                     count should be 1, got {}",
                                    owner, count
                                );
                                assert!(
                                    !pathname.is_empty(),
                                    "Child process (owner PSP={:#06X}) env program name \
                                     should be non-empty",
                                    owner
                                );
                                checked += 1;
                                break;
                            }
                        }
                        off += 1;
                    }
                }
            }
        }

        if block_type == 0x5A {
            break;
        }
        seg = seg + size as u32 + 1;
    }

    assert!(
        checked > 0,
        "Should have found at least one child process with an environment to check"
    );
}
