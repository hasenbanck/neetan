//! MORE command.

use crate::{
    DiskIo, DriveIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    filesystem::{fat_dir, fat_file},
};

pub(crate) struct More;

impl Command for More {
    fn name(&self) -> &'static str {
        "MORE"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningMore {
            args: args.to_vec(),
            phase: MorePhase::Init,
        })
    }
}

const LINES_PER_PAGE: u16 = 24;
const KB_BUF_COUNT: u32 = 0x0528;

struct ReadState {
    data: Vec<u8>,
    offset: usize,
}

enum MorePhase {
    Init,
    Outputting { read: ReadState, lines_shown: u16 },
    WaitKey(ReadState),
}

struct RunningMore {
    args: Vec<u8>,
    phase: MorePhase,
}

impl RunningMore {
    fn do_output(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DriveIo,
        mut read: ReadState,
        mut lines_shown: u16,
    ) -> StepResult {
        while read.offset < read.data.len() {
            let byte = read.data[read.offset];
            io.output_byte(byte);
            if byte == b'\n' {
                lines_shown += 1;
                if lines_shown >= LINES_PER_PAGE {
                    io.print(b"-- More --");
                    read.offset += 1;
                    self.phase = MorePhase::WaitKey(read);
                    return StepResult::Continue;
                }
            }
            read.offset += 1;
        }

        StepResult::Done(0)
    }
}

impl RunningMore {
    fn step_init(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DriveIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();
        if is_help_request(&self.args) || args.is_empty() {
            print_help(io);
            return StepResult::Done(0);
        }

        match init_more(state, io, disk, args) {
            Ok(new_phase) => {
                self.phase = new_phase;
                StepResult::Continue
            }
            Err(msg) => {
                io.print(msg);
                StepResult::Done(1)
            }
        }
    }

    fn step_wait_key(&mut self, read: ReadState, io: &mut IoAccess) -> StepResult {
        if io.memory.read_byte(KB_BUF_COUNT) == 0 {
            self.phase = MorePhase::WaitKey(read);
            return StepResult::Continue;
        }
        consume_key(io);
        io.output_byte(b'\r');
        for _ in 0..40 {
            io.output_byte(b' ');
        }
        io.output_byte(b'\r');

        self.phase = MorePhase::Outputting {
            read,
            lines_shown: 0,
        };
        StepResult::Continue
    }
}

impl RunningCommand for RunningMore {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DriveIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, MorePhase::Init);
        match phase {
            MorePhase::Init => self.step_init(state, io, disk),
            MorePhase::Outputting { read, lines_shown } => {
                self.do_output(state, io, disk, read, lines_shown)
            }
            MorePhase::WaitKey(read) => self.step_wait_key(read, io),
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Displays output one screen at a time.");
    io.println(b"");
    io.println(b"MORE filename");
    io.println(b"");
    io.println(b"  filename  Specifies the file to display.");
}

fn init_more(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    path: &[u8],
) -> Result<MorePhase, &'static [u8]> {
    let (drive_index, dir_cluster, fcb_name) = state
        .resolve_file_path(path, io.memory, disk)
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

    if entry.attribute & fat_dir::ATTR_DIRECTORY != 0 {
        return Err(b"Access denied\r\n");
    }

    if entry.file_size == 0 {
        return Ok(MorePhase::Init);
    }

    let data = fat_file::read_all(vol, &entry, disk).map_err(|_| &b"Read error\r\n"[..])?;

    Ok(MorePhase::Outputting {
        read: ReadState { data, offset: 0 },
        lines_shown: 0,
    })
}

fn consume_key(io: &mut IoAccess) {
    let head = io.memory.read_word(0x0524) as u32;
    let mut new_head = head + 2;
    if new_head >= 0x0522 {
        new_head = 0x0502;
    }
    io.memory.write_word(0x0524, new_head as u16);
    let count = io.memory.read_byte(KB_BUF_COUNT);
    if count > 0 {
        io.memory.write_byte(KB_BUF_COUNT, count - 1);
    }
}
