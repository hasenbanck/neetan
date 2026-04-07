use crate::harness;

fn boot_and_get_environment() -> (machine::Pc9801Ra, u32) {
    let mut machine = harness::boot_hle();
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
    let mut machine = harness::boot_hle();
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

                // NEETAN OS HLE sets the program name for all processes
                // including the root COMMAND.COM, for maximum compatibility
                // with programs that read it (spec section 2.6).
                assert_eq!(
                    count, 0x0001,
                    "NEETAN HLE: COMMAND.COM environment should have count=1"
                );

                let pathname = harness::read_string(&machine.bus, env_addr + count_offset + 2, 128);
                let pathname_str = String::from_utf8_lossy(&pathname);
                assert!(
                    pathname_str.contains("COMMAND.COM"),
                    "Program pathname should contain 'COMMAND.COM', got '{}'",
                    pathname_str
                );
                return;
            }
        }
        offset += 1;
    }
    panic!("Could not find double-null terminator in environment block");
}

// TODO: child_process_program_name(): deferred to phase 10.9 (process management).
//       No child processes exist after HLE boot; this test requires EXEC (INT 21h/4Bh).
