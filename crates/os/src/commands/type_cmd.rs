//! TYPE command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    filesystem::{fat_dir, fat_file},
};

pub(crate) struct TypeCmd;

impl Command for TypeCmd {
    fn name(&self) -> &'static str {
        "TYPE"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningType {
            args: args.to_vec(),
            phase: TypePhase::Init,
        })
    }
}

struct ReadState {
    data: Vec<u8>,
    offset: usize,
}

enum TypePhase {
    Init,
    Outputting(ReadState),
}

struct RunningType {
    args: Vec<u8>,
    phase: TypePhase,
}

impl RunningType {
    fn do_output(
        &mut self,
        _state: &mut OsState,
        io: &mut IoAccess,
        _disk: &mut dyn DiskIo,
        read: ReadState,
    ) -> StepResult {
        let chunk_end = (read.offset + 4096).min(read.data.len());
        for &byte in &read.data[read.offset..chunk_end] {
            io.output_byte(byte);
        }

        if chunk_end >= read.data.len() {
            return StepResult::Done(0);
        }

        self.phase = TypePhase::Outputting(ReadState {
            data: read.data,
            offset: chunk_end,
        });
        StepResult::Continue
    }
}

impl RunningCommand for RunningType {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, TypePhase::Init);
        match phase {
            TypePhase::Init => {
                let args = self.args.trim_ascii();
                if is_help_request(&self.args) || args.is_empty() {
                    print_help(io);
                    return StepResult::Done(0);
                }

                match init_type(state, io, disk, args) {
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
            TypePhase::Outputting(read) => self.do_output(state, io, disk, read),
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Displays the contents of a text file.");
    io.println(b"");
    io.println(b"TYPE filename");
    io.println(b"");
    io.println(b"  filename  Specifies the file to display.");
}

fn init_type(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    path: &[u8],
) -> Result<TypePhase, &'static [u8]> {
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
        return Ok(TypePhase::Init);
    }

    let data = fat_file::read_all(vol, &entry, disk).map_err(|_| &b"Read error\r\n"[..])?;

    Ok(TypePhase::Outputting(ReadState { data, offset: 0 }))
}
