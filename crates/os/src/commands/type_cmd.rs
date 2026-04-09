//! TYPE command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    filesystem::fat_dir,
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
    drive_index: u8,
    current_cluster: u16,
    cluster_data: Vec<u8>,
    offset: usize,
    remaining: u32,
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
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
        read: ReadState,
    ) -> StepResult {
        let vol = match state.fat_volumes[read.drive_index as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        let cluster_size = vol.bpb.sectors_per_cluster as usize * vol.bpb.bytes_per_sector as usize;
        let bytes_in_cluster = cluster_size.min(read.remaining as usize);
        let end = read.offset + (bytes_in_cluster - read.offset).min(bytes_in_cluster);

        for i in read.offset..end {
            if i < read.cluster_data.len() {
                io.output_byte(read.cluster_data[i]);
            }
        }

        let new_remaining = read.remaining - (end - read.offset) as u32;
        if new_remaining == 0 {
            return StepResult::Done(0);
        }

        match vol.next_cluster(read.current_cluster) {
            Some(next) => {
                let next_data = match vol.read_cluster(next, disk) {
                    Ok(d) => d,
                    Err(_) => return StepResult::Done(1),
                };
                self.phase = TypePhase::Outputting(ReadState {
                    drive_index: read.drive_index,
                    current_cluster: next,
                    cluster_data: next_data,
                    offset: 0,
                    remaining: new_remaining,
                });
                StepResult::Continue
            }
            None => StepResult::Done(0),
        }
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

    if entry.file_size == 0 || entry.start_cluster < 2 {
        return Ok(TypePhase::Init);
    }

    let cluster_data = vol
        .read_cluster(entry.start_cluster, disk)
        .map_err(|_| &b"Read error\r\n"[..])?;

    Ok(TypePhase::Outputting(ReadState {
        drive_index,
        current_cluster: entry.start_cluster,
        cluster_data,
        offset: 0,
        remaining: entry.file_size,
    }))
}
