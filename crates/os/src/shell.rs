//! Shell state machine, command parsing, I/O redirection.

pub mod batch;
pub mod history;

use std::collections::VecDeque;

use history::History;

use crate::{
    DiskIo, IoAccess, MemoryAccess, OsState,
    commands::{self, Command, RunningCommand, StepResult},
    filesystem::{fat, fat_dir},
    tables,
};

pub(crate) enum RedirectSpec {
    Overwrite(Vec<u8>),
    Append(Vec<u8>),
}

struct ParsedCommand {
    command: Vec<u8>,
    output_redirect: Option<RedirectSpec>,
    input_file: Option<Vec<u8>>,
}

struct PendingCommand {
    parsed: ParsedCommand,
}

/// An external program (.COM or .EXE) to be EXECed from the shell.
pub(crate) struct PendingExec {
    pub path: Vec<u8>,
    pub args: Vec<u8>,
}

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
    ExecutingBatch(Box<batch::BatchState>),
}

pub(crate) struct Shell {
    pub(crate) phase: ShellPhase,
    history: History,
    commands: Vec<Box<dyn Command>>,
    pub(crate) echo_on: bool,
    pub(crate) last_exit_code: u8,
    boot_banner_shown: bool,
    pending_drive_change: Option<u8>,
    pending_commands: VecDeque<PendingCommand>,
    current_redirect: Option<RedirectSpec>,
    redirect_buffer: Option<Vec<u8>>,
    pipe_input: Option<Vec<u8>>,
    /// COMMAND.COM PSP segment, used to detect child process termination.
    pub(crate) command_com_psp: u16,
    /// External program pending EXEC (set by dispatch, consumed by int21h_ffh_shell_step).
    pub(crate) pending_exec: Option<PendingExec>,
}

impl Shell {
    fn build_commands() -> Vec<Box<dyn Command>> {
        vec![
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
            Box::new(commands::ren::Ren),
            Box::new(commands::time::Time),
            Box::new(commands::type_cmd::TypeCmd),
            Box::new(commands::xcopy::Xcopy),
        ]
    }

    pub(crate) fn new(command_com_psp: u16) -> Self {
        Self {
            phase: ShellPhase::ShowPrompt,
            history: History::new(),
            commands: Self::build_commands(),
            echo_on: true,
            last_exit_code: 0,
            boot_banner_shown: false,
            pending_drive_change: None,
            pending_commands: VecDeque::new(),
            current_redirect: None,
            redirect_buffer: None,
            pipe_input: None,
            command_com_psp,
            pending_exec: None,
        }
    }

    pub(crate) fn new_with_autoexec(
        command_com_psp: u16,
        lines: Vec<Vec<u8>>,
        bat_path: Vec<u8>,
    ) -> Self {
        let params: [Vec<u8>; 10] = Default::default();
        let bat_state = batch::BatchState::new(lines, params, bat_path, true);
        Self {
            phase: ShellPhase::ExecutingBatch(Box::new(bat_state)),
            history: History::new(),
            commands: Self::build_commands(),
            echo_on: true,
            last_exit_code: 0,
            boot_banner_shown: true,
            pending_drive_change: None,
            pending_commands: VecDeque::new(),
            current_redirect: None,
            redirect_buffer: None,
            pipe_input: None,
            command_com_psp,
            pending_exec: None,
        }
    }

    pub(crate) fn step(&mut self, state: &mut OsState, io: &mut IoAccess, disk: &mut dyn DiskIo) {
        let phase = std::mem::replace(&mut self.phase, ShellPhase::ShowPrompt);
        self.phase = match phase {
            ShellPhase::ShowPrompt => {
                if !self.boot_banner_shown {
                    let (major, minor) = state.version;
                    let msg = format!("Neetan OS Version {}.{}\r\n\r\n", major, minor);
                    io.print(msg.as_bytes());
                    self.boot_banner_shown = true;
                }
                if let Some(drive) = self.pending_drive_change.take() {
                    state.current_drive = drive;
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
                            self.dispatch_command(state, io, disk, &line)
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
            ShellPhase::ExecutingCommand(mut cmd) => {
                if self.redirect_buffer.is_some() {
                    io.redirect_output = self.redirect_buffer.take();
                }
                if self.pipe_input.is_some() {
                    io.redirect_input = self
                        .pipe_input
                        .take()
                        .map(|data| crate::RedirectInput { data, position: 0 });
                }
                match cmd.step(state, io, disk) {
                    StepResult::Continue => {
                        self.redirect_buffer = io.redirect_output.take();
                        self.pipe_input = io.redirect_input.take().map(|ri| {
                            let mut d = ri.data;
                            d.drain(..ri.position);
                            d
                        });
                        ShellPhase::ExecutingCommand(cmd)
                    }
                    StepResult::Done(code) => {
                        self.last_exit_code = code;
                        let output_data = io.redirect_output.take();
                        if let Some(spec) = self.current_redirect.take()
                            && let Some(data) = &output_data
                        {
                            write_redirect_to_file(state, io, disk, data, &spec);
                        }
                        if let Some(next) = self.pending_commands.pop_front() {
                            self.setup_and_dispatch(next, output_data, state, io, disk)
                        } else {
                            ShellPhase::ShowPrompt
                        }
                    }
                }
            }
            ShellPhase::WaitingForChild => {
                // The child process runs via CPU execution. When it terminates
                // (INT 21h AH=4Ch), terminate_process() restores COMMAND.COM's
                // PSP and IRET frame. We detect completion by checking if
                // current_psp has returned to COMMAND.COM's PSP.
                if state.current_psp == self.command_com_psp {
                    self.last_exit_code = state.last_return_code;
                    ShellPhase::ShowPrompt
                } else {
                    ShellPhase::WaitingForChild
                }
            }
            ShellPhase::ExecutingBatch(mut batch) => {
                match batch.step_batch(self, state, io, disk) {
                    batch::BatchStepResult::Continue => ShellPhase::ExecutingBatch(batch),
                    batch::BatchStepResult::Finished => ShellPhase::ShowPrompt,
                }
            }
        };
    }

    fn dispatch_command(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
        line: &[u8],
    ) -> ShellPhase {
        let trimmed = line.trim_ascii();
        if trimmed.is_empty() {
            return ShellPhase::ShowPrompt;
        }

        // Split on sequence separator (ASCII 0x14) first
        let sequences = split_on_sequence(trimmed);
        if sequences.len() > 1 {
            let mut seq_iter = sequences.into_iter();
            let first = seq_iter.next().unwrap();
            for seg in seq_iter {
                self.pending_commands.push_back(PendingCommand {
                    parsed: ParsedCommand {
                        command: seg,
                        output_redirect: None,
                        input_file: None,
                    },
                });
            }
            return self.dispatch_single(state, io, disk, &first);
        }

        // Split on pipes
        let pipes = split_on_pipes(trimmed);
        if pipes.len() > 1 {
            let mut pipe_iter = pipes.into_iter();
            let first = pipe_iter.next().unwrap();
            for seg in pipe_iter {
                let parsed = parse_redirections(&seg);
                self.pending_commands.push_back(PendingCommand { parsed });
            }
            // First pipe stage: redirect output to buffer
            let parsed = parse_redirections(&first);
            self.current_redirect = None;
            self.redirect_buffer = Some(Vec::new());
            return self.dispatch_parsed(state, io, disk, &parsed.command);
        }

        self.dispatch_single(state, io, disk, trimmed)
    }

    fn dispatch_single(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
        segment: &[u8],
    ) -> ShellPhase {
        let parsed = parse_redirections(segment);

        // Set up output redirection
        if parsed.output_redirect.is_some() {
            self.current_redirect = parsed.output_redirect;
            self.redirect_buffer = Some(Vec::new());
        }

        // Set up input redirection
        if let Some(ref filename) = parsed.input_file {
            match read_file_data(state, io, disk, filename) {
                Ok(data) => {
                    self.pipe_input = Some(data);
                }
                Err(msg) => {
                    io.print(msg);
                    self.current_redirect = None;
                    self.redirect_buffer = None;
                    return ShellPhase::ShowPrompt;
                }
            }
        }

        self.dispatch_parsed(state, io, disk, &parsed.command)
    }

    pub(crate) fn dispatch_parsed(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
        command: &[u8],
    ) -> ShellPhase {
        let trimmed = command.trim_ascii();
        if trimmed.is_empty() {
            return ShellPhase::ShowPrompt;
        }

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

        // Handle drive change: single letter followed by colon (e.g. "A:")
        if cmd_upper.len() == 2 && cmd_upper[1] == b':' && cmd_upper[0].is_ascii_uppercase() {
            let drive_index = cmd_upper[0] - b'A';
            let cds_addr = tables::CDS_BASE + (drive_index as u32) * tables::CDS_ENTRY_SIZE;
            let cds_flags = io.memory.read_word(cds_addr + tables::CDS_OFF_FLAGS);
            if cds_flags == 0 {
                io.println(b"Invalid drive");
                self.last_exit_code = 1;
                return ShellPhase::ShowPrompt;
            }
            // For physical drives, verify media is accessible.
            if drive_index != 25
                && state
                    .ensure_volume_mounted(drive_index, io.memory, disk)
                    .is_err()
            {
                let msg = [
                    b"No media in drive ".as_slice(),
                    &[b'A' + drive_index],
                    b"\r\n",
                ]
                .concat();
                io.print(&msg);
                self.last_exit_code = 1;
                return ShellPhase::ShowPrompt;
            }
            self.pending_drive_change = Some(drive_index);
            return ShellPhase::ShowPrompt;
        }

        // Look up in command registry
        if let Some(cmd) = self.find_command(&cmd_upper) {
            let running = cmd.start(args);
            return ShellPhase::ExecutingCommand(running);
        }

        // Try to find .BAT file (skip if command already has .COM/.EXE extension,
        // otherwise name_to_fcb would truncate e.g. "INSTALL.EXE.BAT" to match "INSTALL.EXE")
        let has_exe_ext =
            cmd_upper.len() > 4 && (cmd_upper.ends_with(b".COM") || cmd_upper.ends_with(b".EXE"));
        let mut bat_name = cmd_upper.clone();
        bat_name.extend_from_slice(b".BAT");
        if !has_exe_ext
            && let Ok((drive_index, dir_cluster, fcb_name)) =
                state.resolve_file_path(&bat_name, io.memory, disk)
            && drive_index != 25
            && let Some(vol) = state.fat_volumes[drive_index as usize].as_ref()
            && let Ok(Some(entry)) = fat_dir::find_entry(vol, dir_cluster, &fcb_name, disk)
            && entry.attribute & fat_dir::ATTR_DIRECTORY == 0
        {
            match batch::load_bat_file(vol, &entry, disk) {
                Ok(lines) => {
                    let params = parse_bat_params(args);
                    let bat_state =
                        batch::BatchState::new(lines, params, bat_name.clone(), self.echo_on);
                    return ShellPhase::ExecutingBatch(Box::new(bat_state));
                }
                Err(_) => {
                    io.println(b"Error reading batch file");
                    return ShellPhase::ShowPrompt;
                }
            }
        }

        // Try external program (.COM or .EXE)
        if let Some(full_path) = find_external_program(trimmed, &cmd_upper, state, io.memory, disk)
        {
            self.pending_exec = Some(PendingExec {
                path: full_path,
                args: args.to_vec(),
            });
            return ShellPhase::WaitingForChild;
        }

        io.println(b"Bad command or file name");
        self.last_exit_code = 1;

        ShellPhase::ShowPrompt
    }

    fn setup_and_dispatch(
        &mut self,
        pending: PendingCommand,
        pipe_data: Option<Vec<u8>>,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> ShellPhase {
        // Set up output redirection for this command
        if pending.parsed.output_redirect.is_some() {
            self.current_redirect = pending.parsed.output_redirect;
            self.redirect_buffer = Some(Vec::new());
        } else if !self.pending_commands.is_empty() {
            // More pipe stages follow: capture output
            self.current_redirect = None;
            self.redirect_buffer = Some(Vec::new());
        } else {
            self.current_redirect = None;
            self.redirect_buffer = None;
        }

        // Set up input: pipe data from previous command, or input file redirect
        if let Some(data) = pipe_data {
            self.pipe_input = Some(data);
        } else if let Some(ref filename) = pending.parsed.input_file {
            match read_file_data(state, io, disk, filename) {
                Ok(data) => {
                    self.pipe_input = Some(data);
                }
                Err(msg) => {
                    io.print(msg);
                    self.current_redirect = None;
                    self.redirect_buffer = None;
                    return ShellPhase::ShowPrompt;
                }
            }
        }

        self.dispatch_parsed(state, io, disk, &pending.parsed.command)
    }

    fn find_command(&self, name: &[u8]) -> Option<&dyn Command> {
        for cmd in &self.commands {
            if cmd.name().as_bytes() == name {
                return Some(cmd.as_ref());
            }
            for alias in cmd.aliases() {
                if alias.as_bytes() == name {
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
    tables::key_available(memory)
}

fn read_key(memory: &mut dyn MemoryAccess) -> (u8, u8) {
    tables::read_key(memory)
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

pub(crate) fn read_env_var(
    state: &OsState,
    memory: &dyn MemoryAccess,
    var_name: &[u8],
) -> Option<Vec<u8>> {
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

fn split_on_sequence(line: &[u8]) -> Vec<Vec<u8>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for &byte in line {
        if byte == 0x14 {
            if !current.is_empty() {
                segments.push(current);
                current = Vec::new();
            }
        } else {
            current.push(byte);
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn split_on_pipes(line: &[u8]) -> Vec<Vec<u8>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for &byte in line {
        if byte == b'|' {
            segments.push(current);
            current = Vec::new();
        } else {
            current.push(byte);
        }
    }
    segments.push(current);
    segments
}

fn parse_redirections(segment: &[u8]) -> ParsedCommand {
    let mut command = Vec::new();
    let mut output_redirect = None;
    let mut input_file = None;

    let mut i = 0;
    while i < segment.len() {
        if segment[i] == b'>' {
            i += 1;
            let append = i < segment.len() && segment[i] == b'>';
            if append {
                i += 1;
            }
            // Skip whitespace after >
            while i < segment.len() && (segment[i] == b' ' || segment[i] == b'\t') {
                i += 1;
            }
            // Read filename
            let mut filename = Vec::new();
            while i < segment.len()
                && segment[i] != b' '
                && segment[i] != b'\t'
                && segment[i] != b'>'
                && segment[i] != b'<'
                && segment[i] != b'|'
            {
                filename.push(segment[i]);
                i += 1;
            }
            if !filename.is_empty() {
                output_redirect = if append {
                    Some(RedirectSpec::Append(filename))
                } else {
                    Some(RedirectSpec::Overwrite(filename))
                };
            }
        } else if segment[i] == b'<' {
            i += 1;
            while i < segment.len() && (segment[i] == b' ' || segment[i] == b'\t') {
                i += 1;
            }
            let mut filename = Vec::new();
            while i < segment.len()
                && segment[i] != b' '
                && segment[i] != b'\t'
                && segment[i] != b'>'
                && segment[i] != b'<'
                && segment[i] != b'|'
            {
                filename.push(segment[i]);
                i += 1;
            }
            if !filename.is_empty() {
                input_file = Some(filename);
            }
        } else {
            command.push(segment[i]);
            i += 1;
        }
    }

    ParsedCommand {
        command,
        output_redirect,
        input_file,
    }
}

/// Searches for an external program (.COM or .EXE) matching the command name.
///
/// Search order: if the command already has an extension (.COM/.EXE), use as-is.
/// Otherwise, try .COM then .EXE in: current directory, then each PATH directory.
fn find_external_program(
    original_cmd: &[u8],
    cmd_upper: &[u8],
    state: &mut OsState,
    memory: &dyn crate::MemoryAccess,
    disk: &mut dyn DiskIo,
) -> Option<Vec<u8>> {
    let has_extension =
        cmd_upper.len() > 4 && (cmd_upper.ends_with(b".COM") || cmd_upper.ends_with(b".EXE"));

    // If the command contains a path separator or drive letter, try it directly
    let has_path = original_cmd.contains(&b'\\') || original_cmd.contains(&b'/');
    let has_drive = original_cmd.len() >= 2 && original_cmd[1] == b':';

    if has_path || has_drive {
        // Direct path: try as given (with extension search if needed)
        return try_find_program(original_cmd, has_extension, state, memory, disk);
    }

    // Search current directory first
    if let Some(path) = try_find_program(original_cmd, has_extension, state, memory, disk) {
        return Some(path);
    }

    // Search PATH directories
    let path_value = read_env_var(state, memory, b"PATH")?;
    for dir in path_value.split(|&b| b == b';') {
        let dir = dir.trim_ascii();
        if dir.is_empty() {
            continue;
        }
        let mut full_path = dir.to_vec();
        if !full_path.ends_with(b"\\") {
            full_path.push(b'\\');
        }
        full_path.extend_from_slice(original_cmd);
        if let Some(path) = try_find_program(&full_path, has_extension, state, memory, disk) {
            return Some(path);
        }
    }

    None
}

/// Tries to find a program file at the given path, optionally appending .COM/.EXE.
fn try_find_program(
    path: &[u8],
    has_extension: bool,
    state: &mut OsState,
    memory: &dyn crate::MemoryAccess,
    disk: &mut dyn DiskIo,
) -> Option<Vec<u8>> {
    if has_extension {
        if file_exists_on_disk(path, state, memory, disk) {
            return Some(path.to_vec());
        }
    } else {
        // Try .COM first, then .EXE
        let mut com_path = path.to_vec();
        com_path.extend_from_slice(b".COM");
        if file_exists_on_disk(&com_path, state, memory, disk) {
            return Some(com_path);
        }
        let mut exe_path = path.to_vec();
        exe_path.extend_from_slice(b".EXE");
        if file_exists_on_disk(&exe_path, state, memory, disk) {
            return Some(exe_path);
        }
    }
    None
}

/// Checks if a file exists on a real (non-virtual) drive.
fn file_exists_on_disk(
    path: &[u8],
    state: &mut OsState,
    memory: &dyn crate::MemoryAccess,
    disk: &mut dyn DiskIo,
) -> bool {
    let (drive_index, dir_cluster, fcb_name) = match state.resolve_file_path(path, memory, disk) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if drive_index == 25 {
        return false;
    }
    let vol = match state.fat_volumes[drive_index as usize].as_ref() {
        Some(v) => v,
        None => return false,
    };
    matches!(
        fat_dir::find_entry(vol, dir_cluster, &fcb_name, disk),
        Ok(Some(e)) if e.attribute & fat_dir::ATTR_DIRECTORY == 0
    )
}

fn parse_bat_params(args: &[u8]) -> [Vec<u8>; 10] {
    let mut params: [Vec<u8>; 10] = Default::default();
    let mut idx = 1usize; // %1 is first argument, %0 is filled by caller
    let trimmed = args.trim_ascii();
    if trimmed.is_empty() {
        return params;
    }
    let mut i = 0;
    while i < trimmed.len() && idx < 10 {
        // Skip whitespace
        while i < trimmed.len() && (trimmed[i] == b' ' || trimmed[i] == b'\t') {
            i += 1;
        }
        if i >= trimmed.len() {
            break;
        }
        let start = i;
        while i < trimmed.len() && trimmed[i] != b' ' && trimmed[i] != b'\t' {
            i += 1;
        }
        params[idx] = trimmed[start..i].to_vec();
        idx += 1;
    }
    params
}

fn write_redirect_to_file(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    data: &[u8],
    spec: &RedirectSpec,
) {
    let filename = match spec {
        RedirectSpec::Overwrite(f) | RedirectSpec::Append(f) => f,
    };
    let is_append = matches!(spec, RedirectSpec::Append(_));

    let (drive_index, dir_cluster, fcb_name) =
        match state.resolve_file_path(filename, io.memory, disk) {
            Ok(r) => r,
            Err(_) => {
                io.console.process_byte(io.memory, b'\r');
                io.console.process_byte(io.memory, b'\n');
                for &byte in b"File creation error" {
                    io.console.process_byte(io.memory, byte);
                }
                io.console.process_byte(io.memory, b'\r');
                io.console.process_byte(io.memory, b'\n');
                return;
            }
        };

    if drive_index == 25 {
        return; // Z: is read-only
    }

    let vol = match state.fat_volumes[drive_index as usize].as_mut() {
        Some(v) => v,
        None => return,
    };

    if is_append {
        // Append mode: find existing file, walk to end of chain, append data
        if let Ok(Some(existing)) = fat_dir::find_entry(vol, dir_cluster, &fcb_name, disk) {
            append_to_existing_file(vol, &existing, data, disk);
        } else {
            create_new_file_with_data(vol, dir_cluster, &fcb_name, data, disk);
        }
    } else {
        // Overwrite mode: delete existing, create new
        if let Ok(Some(existing)) = fat_dir::find_entry(vol, dir_cluster, &fcb_name, disk) {
            if existing.start_cluster >= 2 {
                vol.free_chain(existing.start_cluster);
            }
            let _ = fat_dir::delete_entry(vol, &existing, disk);
        }
        create_new_file_with_data(vol, dir_cluster, &fcb_name, data, disk);
    }

    let _ = vol.flush_fat(disk);
}

fn create_new_file_with_data(
    vol: &mut fat::FatVolume,
    dir_cluster: u16,
    fcb_name: &[u8; 11],
    data: &[u8],
    disk: &mut dyn DiskIo,
) {
    let cluster_size = vol.bpb.sectors_per_cluster as usize * vol.bpb.bytes_per_sector as usize;
    let mut first_cluster: u16 = 0;
    let mut last_cluster: u16 = 0;
    let mut offset = 0;

    while offset < data.len() {
        let end = (offset + cluster_size).min(data.len());
        let mut cluster_data = vec![0u8; cluster_size];
        cluster_data[..end - offset].copy_from_slice(&data[offset..end]);

        let new_cluster = match vol.allocate_cluster(last_cluster) {
            Some(c) => c,
            None => return,
        };
        if first_cluster == 0 {
            first_cluster = new_cluster;
        }
        let _ = vol.write_cluster(new_cluster, &cluster_data, disk);
        last_cluster = new_cluster;
        offset = end;
    }

    let new_entry = fat_dir::DirEntry {
        name: *fcb_name,
        attribute: 0x20, // archive
        time: 0x6000,    // 12:00:00
        date: 0x1E21,    // 1995-01-01
        start_cluster: first_cluster,
        file_size: data.len() as u32,
        dir_sector: 0,
        dir_offset: 0,
    };
    let _ = fat_dir::create_entry(vol, dir_cluster, &new_entry, disk);
}

fn append_to_existing_file(
    vol: &mut fat::FatVolume,
    entry: &fat_dir::DirEntry,
    data: &[u8],
    disk: &mut dyn DiskIo,
) {
    let cluster_size = vol.bpb.sectors_per_cluster as usize * vol.bpb.bytes_per_sector as usize;
    let old_size = entry.file_size as usize;

    // Find last cluster and its fill level
    let mut last_cluster = entry.start_cluster;
    let mut remaining_in_chain = old_size;

    if last_cluster < 2 {
        // Empty file: allocate first cluster
        let new_cluster = match vol.allocate_cluster(0) {
            Some(c) => c,
            None => return,
        };
        let mut cluster_data = vec![0u8; cluster_size];
        let write_len = data.len().min(cluster_size);
        cluster_data[..write_len].copy_from_slice(&data[..write_len]);
        let _ = vol.write_cluster(new_cluster, &cluster_data, disk);

        let mut updated = entry.clone();
        updated.start_cluster = new_cluster;
        updated.file_size = data.len() as u32;

        // Write remaining data if any
        let mut offset = write_len;
        let mut prev = new_cluster;
        while offset < data.len() {
            let end = (offset + cluster_size).min(data.len());
            let mut cd = vec![0u8; cluster_size];
            cd[..end - offset].copy_from_slice(&data[offset..end]);
            let nc = match vol.allocate_cluster(prev) {
                Some(c) => c,
                None => break,
            };
            let _ = vol.write_cluster(nc, &cd, disk);
            prev = nc;
            offset = end;
        }
        updated.file_size = (old_size + data.len()) as u32;
        let _ = fat_dir::update_entry(vol, &updated, disk);
        return;
    }

    // Walk to last cluster
    while remaining_in_chain > cluster_size {
        if let Some(next) = vol.next_cluster(last_cluster) {
            last_cluster = next;
            remaining_in_chain -= cluster_size;
        } else {
            break;
        }
    }

    let used_in_last = remaining_in_chain % cluster_size;
    let free_in_last = if used_in_last == 0 && old_size > 0 {
        0
    } else {
        cluster_size - used_in_last
    };

    let mut offset = 0;

    // Fill remaining space in last cluster
    if free_in_last > 0 && !data.is_empty() {
        let mut existing_data = match vol.read_cluster(last_cluster, disk) {
            Ok(d) => d,
            Err(_) => return,
        };
        let write_len = data.len().min(free_in_last);
        existing_data[used_in_last..used_in_last + write_len].copy_from_slice(&data[..write_len]);
        let _ = vol.write_cluster(last_cluster, &existing_data, disk);
        offset = write_len;
    }

    // Allocate new clusters for remaining data
    while offset < data.len() {
        let end = (offset + cluster_size).min(data.len());
        let mut cluster_data = vec![0u8; cluster_size];
        cluster_data[..end - offset].copy_from_slice(&data[offset..end]);
        let new_cluster = match vol.allocate_cluster(last_cluster) {
            Some(c) => c,
            None => break,
        };
        let _ = vol.write_cluster(new_cluster, &cluster_data, disk);
        last_cluster = new_cluster;
        offset = end;
    }

    let mut updated = entry.clone();
    updated.file_size = (old_size + data.len()) as u32;
    let _ = fat_dir::update_entry(vol, &updated, disk);
}

fn read_file_data(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    filename: &[u8],
) -> Result<Vec<u8>, &'static [u8]> {
    let (drive_index, dir_cluster, fcb_name) = state
        .resolve_file_path(filename, io.memory, disk)
        .map_err(|_| &b"File not found\r\n"[..])?;

    if drive_index == 25 {
        return Err(b"Access denied\r\n");
    }

    let vol = state.fat_volumes[drive_index as usize]
        .as_ref()
        .ok_or(&b"Invalid drive\r\n"[..])?;

    let entry = fat_dir::find_entry(vol, dir_cluster, &fcb_name, disk)
        .map_err(|_| &b"File not found\r\n"[..])?
        .ok_or(&b"File not found\r\n"[..])?;

    if entry.file_size == 0 || entry.start_cluster < 2 {
        return Ok(Vec::new());
    }

    let mut data = Vec::with_capacity(entry.file_size as usize);
    let mut cluster = entry.start_cluster;
    let mut remaining = entry.file_size as usize;

    loop {
        let cluster_data = vol
            .read_cluster(cluster, disk)
            .map_err(|_| &b"Read error\r\n"[..])?;
        let take = remaining.min(cluster_data.len());
        data.extend_from_slice(&cluster_data[..take]);
        remaining -= take;
        if remaining == 0 {
            break;
        }
        cluster = match vol.next_cluster(cluster) {
            Some(c) => c,
            None => break,
        };
    }

    Ok(data)
}
