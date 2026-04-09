//! SET command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    tables,
};

pub(crate) struct Set;

impl Command for Set {
    fn name(&self) -> &'static str {
        "SET"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningSet {
            args: args.to_vec(),
        })
    }
}

struct RunningSet {
    args: Vec<u8>,
}

impl RunningCommand for RunningSet {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();

        if is_help_request(&self.args) {
            print_help(io);
            return StepResult::Done(0);
        }

        if args.is_empty() {
            dump_environment(state, io);
            return StepResult::Done(0);
        }

        // SET VAR=VALUE
        if let Some(eq_pos) = args.iter().position(|&b| b == b'=') {
            let var_name = &args[..eq_pos];
            let value = &args[eq_pos + 1..];
            set_env_var(state, io, var_name, value);
        } else {
            // SET VAR (no =) - print matching vars
            dump_matching_vars(state, io, args);
        }

        StepResult::Done(0)
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Displays, sets, or removes environment variables.");
    io.println(b"");
    io.println(b"SET [variable=[value]]");
    io.println(b"");
    io.println(b"  variable  Specifies the environment variable name.");
    io.println(b"  value     Specifies the value to assign. If omitted, the");
    io.println(b"            variable is removed.");
    io.println(b"");
    io.println(b"Type SET without parameters to display all variables.");
}

fn env_base(state: &OsState, memory: &dyn crate::MemoryAccess) -> u32 {
    let psp_base = (state.current_psp as u32) << 4;
    let env_seg = memory.read_word(psp_base + tables::PSP_OFF_ENV_SEG);
    (env_seg as u32) << 4
}

fn dump_environment(state: &OsState, io: &mut IoAccess) {
    let base = env_base(state, io.memory);
    let mut offset = 0u32;

    loop {
        let byte = io.memory.read_byte(base + offset);
        if byte == 0 {
            break;
        }
        // Read one NUL-terminated string
        let start = offset;
        while io.memory.read_byte(base + offset) != 0 {
            offset += 1;
        }
        // Print the string
        for i in start..offset {
            let byte = io.memory.read_byte(base + i);
            io.output_byte(byte);
        }
        io.output_byte(b'\r');
        io.output_byte(b'\n');
        offset += 1; // skip NUL
    }
}

fn dump_matching_vars(state: &OsState, io: &mut IoAccess, prefix: &[u8]) {
    let base = env_base(state, io.memory);
    let mut offset = 0u32;
    let upper_prefix: Vec<u8> = prefix.iter().map(|b| b.to_ascii_uppercase()).collect();

    loop {
        let byte = io.memory.read_byte(base + offset);
        if byte == 0 {
            break;
        }
        let start = offset;
        while io.memory.read_byte(base + offset) != 0 {
            offset += 1;
        }
        // Check if this var name starts with the prefix (case-insensitive)
        let mut matches = true;
        for (i, &p) in upper_prefix.iter().enumerate() {
            let var_byte = io.memory.read_byte(base + start + i as u32);
            if var_byte.to_ascii_uppercase() != p {
                matches = false;
                break;
            }
        }
        if matches {
            for i in start..offset {
                let byte = io.memory.read_byte(base + i);
                io.output_byte(byte);
            }
            io.output_byte(b'\r');
            io.output_byte(b'\n');
        }
        offset += 1;
    }
}

fn set_env_var(state: &OsState, io: &mut IoAccess, var_name: &[u8], value: &[u8]) {
    let base = env_base(state, io.memory);
    let upper_name: Vec<u8> = var_name.iter().map(|b| b.to_ascii_uppercase()).collect();

    // Read entire environment block into a host buffer
    let mut env_data = Vec::new();
    let mut offset = 0u32;
    loop {
        let byte = io.memory.read_byte(base + offset);
        if byte == 0 {
            // Check for double-NUL (end of environment)
            let next = io.memory.read_byte(base + offset + 1);
            env_data.push(0);
            if next == 0 {
                break;
            }
        } else {
            env_data.push(byte);
        }
        offset += 1;
    }

    // Parse into individual strings
    let mut vars: Vec<Vec<u8>> = Vec::new();
    let mut found = false;
    for entry in env_data.split(|&b| b == 0) {
        if entry.is_empty() {
            continue;
        }
        // Check if this entry's name matches
        if let Some(eq_pos) = entry.iter().position(|&b| b == b'=') {
            let name = &entry[..eq_pos];
            let name_upper: Vec<u8> = name.iter().map(|b| b.to_ascii_uppercase()).collect();
            if name_upper == upper_name {
                found = true;
                if !value.is_empty() {
                    // Replace value
                    let mut new_entry = upper_name.clone();
                    new_entry.push(b'=');
                    new_entry.extend_from_slice(value);
                    vars.push(new_entry);
                }
                // If value is empty, we remove the variable (don't add it)
                continue;
            }
        }
        vars.push(entry.to_vec());
    }

    if !found && !value.is_empty() {
        // Add new variable
        let mut new_entry = upper_name;
        new_entry.push(b'=');
        new_entry.extend_from_slice(value);
        vars.push(new_entry);
    }

    // Write back to environment block
    let mut write_offset = 0u32;
    for var in &vars {
        for &byte in var {
            io.memory.write_byte(base + write_offset, byte);
            write_offset += 1;
        }
        io.memory.write_byte(base + write_offset, 0);
        write_offset += 1;
    }
    // Double-NUL terminator
    io.memory.write_byte(base + write_offset, 0);
}
