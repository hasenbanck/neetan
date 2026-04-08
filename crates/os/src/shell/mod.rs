//! Shell state machine, command parsing, I/O redirection.

pub mod batch;
pub mod history;

use history::History;

use crate::{
    DiskIo, IoAccess, MemoryAccess, OsState,
    commands::{self, Command, RunningCommand, StepResult},
    tables,
};

const KB_BUF_START: u32 = 0x0502;
const KB_BUF_END: u32 = 0x0522;
const KB_BUF_HEAD: u32 = 0x0524;
const KB_BUF_COUNT: u32 = 0x0528;

const SCAN_INSERT: u8 = 0x37;
const SCAN_DELETE: u8 = 0x38;
const SCAN_UP: u8 = 0x39;
const SCAN_LEFT: u8 = 0x3A;
const SCAN_RIGHT: u8 = 0x3B;
const SCAN_DOWN: u8 = 0x3C;
const SCAN_HOME: u8 = 0x3D;
const SCAN_END: u8 = 0x3E;

pub(crate) struct LineEditor {
    buffer: Vec<u8>,
    cursor: usize,
    prompt_col: u8,
    insert_mode: bool,
    saved_line: Option<Vec<u8>>,
}

impl LineEditor {
    fn new(prompt_col: u8) -> Self {
        Self {
            buffer: Vec::with_capacity(128),
            cursor: 0,
            prompt_col,
            insert_mode: true,
            saved_line: None,
        }
    }
}

pub(crate) enum ShellPhase {
    ShowPrompt,
    ReadingInput(LineEditor),
    ExecutingCommand(Box<dyn RunningCommand>),
    WaitingForChild,
    ExecutingBatch(batch::BatchState),
}

pub(crate) struct Shell {
    phase: ShellPhase,
    history: History,
    commands: Vec<Box<dyn Command>>,
    echo_on: bool,
    last_exit_code: u8,
    boot_banner_shown: bool,
}

impl Shell {
    pub(crate) fn new() -> Self {
        let commands: Vec<Box<dyn Command>> = vec![
            Box::new(commands::cls::Cls),
            Box::new(commands::ver::Ver),
            Box::new(commands::echo::Echo),
            Box::new(commands::rem::Rem),
            Box::new(commands::cd::Cd),
            Box::new(commands::set::Set),
            Box::new(commands::copy::Copy),
            Box::new(commands::date::Date),
            Box::new(commands::del::Del),
            Box::new(commands::dir::Dir),
            Box::new(commands::diskcopy::Diskcopy),
            Box::new(commands::format::Format),
            Box::new(commands::md::Md),
            Box::new(commands::more::More),
            Box::new(commands::rd::Rd),
            Box::new(commands::time::Time),
            Box::new(commands::type_cmd::TypeCmd),
            Box::new(commands::xcopy::Xcopy),
        ];

        Self {
            phase: ShellPhase::ShowPrompt,
            history: History::new(),
            commands,
            echo_on: true,
            last_exit_code: 0,
            boot_banner_shown: false,
        }
    }

    pub(crate) fn step(&mut self, state: &mut OsState, io: &mut IoAccess, disk: &mut dyn DiskIo) {
        let phase = std::mem::replace(&mut self.phase, ShellPhase::ShowPrompt);
        self.phase = match phase {
            ShellPhase::ShowPrompt => {
                if !self.boot_banner_shown {
                    let (major, minor) = state.version;
                    let msg = format!("Neetan OS Version {}.{}\r\n\r\n", major, minor);
                    for &byte in msg.as_bytes() {
                        io.console.process_byte(io.memory, byte);
                    }
                    self.boot_banner_shown = true;
                }
                render_prompt(state, io);
                let prompt_col = io.console.cursor_col(io.memory);
                ShellPhase::ReadingInput(LineEditor::new(prompt_col))
            }
            ShellPhase::ReadingInput(mut editor) => {
                if !key_available(io.memory) {
                    ShellPhase::ReadingInput(editor)
                } else {
                    let (scan, ch) = read_key(io.memory);
                    match ch {
                        0x0D => {
                            io.console.process_byte(io.memory, b'\r');
                            io.console.process_byte(io.memory, b'\n');
                            let line = editor.buffer.clone();
                            if !line.trim_ascii().is_empty() {
                                self.history.push(line.clone());
                            }
                            self.history.reset_position();
                            self.dispatch_command(state, &line)
                        }
                        0x08 => {
                            if editor.cursor > 0 {
                                editor.cursor -= 1;
                                editor.buffer.remove(editor.cursor);
                                redraw_from_cursor(&editor, io);
                            }
                            ShellPhase::ReadingInput(editor)
                        }
                        0x00 => {
                            match scan {
                                SCAN_LEFT => {
                                    if editor.cursor > 0 {
                                        editor.cursor -= 1;
                                        let row = io.console.cursor_row(io.memory);
                                        let col = editor.prompt_col + editor.cursor as u8;
                                        io.console.set_cursor(io.memory, row, col);
                                    }
                                }
                                SCAN_RIGHT => {
                                    if editor.cursor < editor.buffer.len() {
                                        editor.cursor += 1;
                                        let row = io.console.cursor_row(io.memory);
                                        let col = editor.prompt_col + editor.cursor as u8;
                                        io.console.set_cursor(io.memory, row, col);
                                    }
                                }
                                SCAN_HOME => {
                                    editor.cursor = 0;
                                    let row = io.console.cursor_row(io.memory);
                                    io.console.set_cursor(io.memory, row, editor.prompt_col);
                                }
                                SCAN_END => {
                                    editor.cursor = editor.buffer.len();
                                    let row = io.console.cursor_row(io.memory);
                                    let col = editor.prompt_col + editor.cursor as u8;
                                    io.console.set_cursor(io.memory, row, col);
                                }
                                SCAN_INSERT => {
                                    editor.insert_mode = !editor.insert_mode;
                                }
                                SCAN_DELETE => {
                                    if editor.cursor < editor.buffer.len() {
                                        editor.buffer.remove(editor.cursor);
                                        redraw_from_cursor(&editor, io);
                                    }
                                }
                                SCAN_UP => {
                                    if self.history.at_end() && !self.history.is_empty() {
                                        editor.saved_line = Some(editor.buffer.clone());
                                    }
                                    if let Some(entry) = self.history.navigate_up() {
                                        let entry = entry.to_vec();
                                        replace_line(&mut editor, entry, io);
                                    }
                                }
                                SCAN_DOWN => {
                                    if !self.history.at_end() {
                                        match self.history.navigate_down() {
                                            Some(entry) => {
                                                let entry = entry.to_vec();
                                                replace_line(&mut editor, entry, io);
                                            }
                                            None => {
                                                let restored =
                                                    editor.saved_line.take().unwrap_or_default();
                                                replace_line(&mut editor, restored, io);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            ShellPhase::ReadingInput(editor)
                        }
                        ch if ch >= 0x20 => {
                            if editor.buffer.len() < 127 {
                                if editor.insert_mode || editor.cursor >= editor.buffer.len() {
                                    editor.buffer.insert(editor.cursor, ch);
                                    editor.cursor += 1;
                                    if editor.cursor == editor.buffer.len() {
                                        io.console.process_byte(io.memory, ch);
                                    } else {
                                        redraw_from(&editor, editor.cursor - 1, io);
                                    }
                                } else {
                                    editor.buffer[editor.cursor] = ch;
                                    editor.cursor += 1;
                                    io.console.process_byte(io.memory, ch);
                                }
                            }
                            ShellPhase::ReadingInput(editor)
                        }
                        _ => ShellPhase::ReadingInput(editor),
                    }
                }
            }
            ShellPhase::ExecutingCommand(mut cmd) => match cmd.step(state, io, disk) {
                StepResult::Continue => ShellPhase::ExecutingCommand(cmd),
                StepResult::Done(code) => {
                    self.last_exit_code = code;
                    ShellPhase::ShowPrompt
                }
            },
            ShellPhase::WaitingForChild => {
                unimplemented!("WaitingForChild shell phase")
            }
            ShellPhase::ExecutingBatch(_) => {
                unimplemented!("ExecutingBatch shell phase")
            }
        };
    }

    fn dispatch_command(&mut self, _state: &mut OsState, line: &[u8]) -> ShellPhase {
        let trimmed = line.trim_ascii();
        if trimmed.is_empty() {
            return ShellPhase::ShowPrompt;
        }

        // Split into command name and arguments
        let (cmd_name, args) = split_command(trimmed);
        let cmd_upper: Vec<u8> = cmd_name.iter().map(|b| b.to_ascii_uppercase()).collect();

        // Handle ECHO ON/OFF specially (affects shell state)
        if cmd_upper == b"ECHO" {
            let args_trimmed = args.trim_ascii();
            let args_upper: Vec<u8> = args_trimmed
                .iter()
                .map(|b| b.to_ascii_uppercase())
                .collect();
            if args_upper == b"ON" {
                self.echo_on = true;
                return ShellPhase::ShowPrompt;
            }
            if args_upper == b"OFF" {
                self.echo_on = false;
                return ShellPhase::ShowPrompt;
            }
        }

        // Special case: ECHO. (dot immediately after ECHO, no space)
        if cmd_upper.starts_with(b"ECHO.")
            && let Some(cmd) = self.find_command(b"ECHO")
        {
            let running = cmd.start(b"");
            return ShellPhase::ExecutingCommand(running);
        }

        // Look up in command registry
        if let Some(cmd) = self.find_command(&cmd_upper) {
            let running = cmd.start(args);
            return ShellPhase::ExecutingCommand(running);
        }

        // Command not found -- print error
        ShellPhase::ShowPrompt
    }

    fn find_command(&self, name: &[u8]) -> Option<&dyn Command> {
        for cmd in &self.commands {
            let cmd_name: Vec<u8> = cmd.name().bytes().collect();
            if cmd_name == name {
                return Some(cmd.as_ref());
            }
            for alias in cmd.aliases() {
                let alias_bytes: Vec<u8> = alias.bytes().collect();
                if alias_bytes == name {
                    return Some(cmd.as_ref());
                }
            }
        }
        None
    }
}

fn redraw_from_cursor(editor: &LineEditor, io: &mut IoAccess) {
    redraw_from(editor, editor.cursor, io);
}

fn redraw_from(editor: &LineEditor, from: usize, io: &mut IoAccess) {
    let row = io.console.cursor_row(io.memory);
    io.console
        .set_cursor(io.memory, row, editor.prompt_col + from as u8);
    for &byte in &editor.buffer[from..] {
        io.console.process_byte(io.memory, byte);
    }
    io.console.process_byte(io.memory, b' ');
    io.console
        .set_cursor(io.memory, row, editor.prompt_col + editor.cursor as u8);
}

fn replace_line(editor: &mut LineEditor, new_buffer: Vec<u8>, io: &mut IoAccess) {
    let row = io.console.cursor_row(io.memory);
    io.console.set_cursor(io.memory, row, editor.prompt_col);
    io.console.clear_line_from_cursor(io.memory);
    for &byte in &new_buffer {
        io.console.process_byte(io.memory, byte);
    }
    editor.buffer = new_buffer;
    editor.cursor = editor.buffer.len();
}

fn split_command(line: &[u8]) -> (&[u8], &[u8]) {
    if let Some(pos) = line.iter().position(|&b| b == b' ' || b == b'\t') {
        (&line[..pos], &line[pos + 1..])
    } else {
        (line, &[])
    }
}

fn key_available(memory: &dyn MemoryAccess) -> bool {
    memory.read_byte(KB_BUF_COUNT) > 0
}

fn read_key(memory: &mut dyn MemoryAccess) -> (u8, u8) {
    let head = memory.read_word(KB_BUF_HEAD) as u32;
    let ch = memory.read_byte(head);
    let scan = memory.read_byte(head + 1);

    let mut new_head = head + 2;
    if new_head >= KB_BUF_END {
        new_head = KB_BUF_START;
    }
    memory.write_word(KB_BUF_HEAD, new_head as u16);

    let count = memory.read_byte(KB_BUF_COUNT);
    if count > 0 {
        memory.write_byte(KB_BUF_COUNT, count - 1);
    }

    (scan, ch)
}

fn render_prompt(state: &OsState, io: &mut IoAccess) {
    let prompt_value = read_env_var(state, io.memory, b"PROMPT");
    let prompt = prompt_value.unwrap_or_else(|| b"$P$G".to_vec());

    let mut i = 0;
    while i < prompt.len() {
        if prompt[i] == b'$' && i + 1 < prompt.len() {
            i += 1;
            match prompt[i].to_ascii_uppercase() {
                b'P' => {
                    let cds_addr =
                        tables::CDS_BASE + (state.current_drive as u32) * tables::CDS_ENTRY_SIZE;
                    for j in 0..67u32 {
                        let byte = io.memory.read_byte(cds_addr + tables::CDS_OFF_PATH + j);
                        if byte == 0 {
                            break;
                        }
                        io.console.process_byte(io.memory, byte);
                    }
                }
                b'G' => {
                    io.console.process_byte(io.memory, b'>');
                }
                b'L' => {
                    io.console.process_byte(io.memory, b'<');
                }
                b'E' => {
                    io.console.process_byte(io.memory, 0x1B);
                }
                b'H' => {
                    io.console.process_byte(io.memory, 0x08);
                }
                b'_' => {
                    io.console.process_byte(io.memory, b'\r');
                    io.console.process_byte(io.memory, b'\n');
                }
                b'$' => {
                    io.console.process_byte(io.memory, b'$');
                }
                b'D' => {
                    for &byte in b"1995-01-01" {
                        io.console.process_byte(io.memory, byte);
                    }
                }
                b'T' => {
                    for &byte in b"00:00:00" {
                        io.console.process_byte(io.memory, byte);
                    }
                }
                b'N' => {
                    io.console
                        .process_byte(io.memory, b'A' + state.current_drive);
                }
                b'V' => {
                    let (major, minor) = state.version;
                    let msg = format!("Neetan OS Version {}.{}", major, minor);
                    for &byte in msg.as_bytes() {
                        io.console.process_byte(io.memory, byte);
                    }
                }
                _ => {
                    io.console.process_byte(io.memory, b'$');
                    io.console.process_byte(io.memory, prompt[i]);
                }
            }
        } else {
            io.console.process_byte(io.memory, prompt[i]);
        }
        i += 1;
    }
}

fn read_env_var(state: &OsState, memory: &dyn MemoryAccess, var_name: &[u8]) -> Option<Vec<u8>> {
    let psp_base = (state.current_psp as u32) << 4;
    let env_seg = memory.read_word(psp_base + tables::PSP_OFF_ENV_SEG);
    let base = (env_seg as u32) << 4;
    let upper_name: Vec<u8> = var_name.iter().map(|b| b.to_ascii_uppercase()).collect();

    let mut offset = 0u32;
    loop {
        let byte = memory.read_byte(base + offset);
        if byte == 0 {
            return None;
        }
        let start = offset;
        while memory.read_byte(base + offset) != 0 {
            offset += 1;
        }

        // Check if this entry's name matches
        if let Some(eq_pos) = (start..offset).position(|i| memory.read_byte(base + i) == b'=') {
            let mut name_matches = eq_pos == upper_name.len();
            if name_matches {
                for (j, &expected) in upper_name.iter().enumerate() {
                    let actual = memory.read_byte(base + start + j as u32);
                    if actual.to_ascii_uppercase() != expected {
                        name_matches = false;
                        break;
                    }
                }
            }
            if name_matches {
                let value_start = start + eq_pos as u32 + 1;
                let mut value = Vec::new();
                for i in value_start..offset {
                    value.push(memory.read_byte(base + i));
                }
                return Some(value);
            }
        }

        offset += 1;
    }
}
