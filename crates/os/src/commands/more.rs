//! MORE command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
    filesystem::fat_dir,
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
    drive_index: u8,
    current_cluster: u16,
    cluster_data: Vec<u8>,
    offset: usize,
    remaining: u32,
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
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
        mut read: ReadState,
        mut lines_shown: u16,
    ) -> StepResult {
        let vol = match state.fat_volumes[read.drive_index as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        let cluster_size = vol.bpb.sectors_per_cluster as usize * vol.bpb.bytes_per_sector as usize;
        let end = cluster_size.min(read.remaining as usize + read.offset);

        while read.offset < end && read.remaining > 0 {
            if read.offset < read.cluster_data.len() {
                let byte = read.cluster_data[read.offset];
                io.console.process_byte(io.memory, byte);
                if byte == b'\n' {
                    lines_shown += 1;
                    if lines_shown >= LINES_PER_PAGE {
                        io.print_msg(b"-- More --");
                        read.offset += 1;
                        read.remaining -= 1;
                        self.phase = MorePhase::WaitKey(read);
                        return StepResult::Continue;
                    }
                }
            }
            read.offset += 1;
            read.remaining -= 1;
        }

        if read.remaining == 0 {
            return StepResult::Done(0);
        }

        match vol.next_cluster(read.current_cluster) {
            Some(next) => {
                let next_data = match vol.read_cluster(next, disk) {
                    Ok(d) => d,
                    Err(_) => return StepResult::Done(1),
                };
                self.phase = MorePhase::Outputting {
                    read: ReadState {
                        drive_index: read.drive_index,
                        current_cluster: next,
                        cluster_data: next_data,
                        offset: 0,
                        remaining: read.remaining,
                    },
                    lines_shown,
                };
                StepResult::Continue
            }
            None => StepResult::Done(0),
        }
    }
}

impl RunningCommand for RunningMore {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, MorePhase::Init);
        match phase {
            MorePhase::Init => {
                let args = self.args.trim_ascii();
                if args.is_empty() {
                    io.print_msg(b"Required parameter missing\r\n");
                    return StepResult::Done(1);
                }

                match init_more(state, io, disk, args) {
                    Ok(new_phase) => {
                        self.phase = new_phase;
                        StepResult::Continue
                    }
                    Err(msg) => {
                        io.print_msg(msg);
                        StepResult::Done(1)
                    }
                }
            }
            MorePhase::Outputting { read, lines_shown } => {
                self.do_output(state, io, disk, read, lines_shown)
            }
            MorePhase::WaitKey(read) => {
                if io.memory.read_byte(KB_BUF_COUNT) == 0 {
                    self.phase = MorePhase::WaitKey(read);
                    return StepResult::Continue;
                }
                consume_key(io);
                io.console.process_byte(io.memory, b'\r');
                for _ in 0..40 {
                    io.console.process_byte(io.memory, b' ');
                }
                io.console.process_byte(io.memory, b'\r');

                self.phase = MorePhase::Outputting {
                    read,
                    lines_shown: 0,
                };
                StepResult::Continue
            }
        }
    }
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

    if entry.file_size == 0 || entry.start_cluster < 2 {
        return Ok(MorePhase::Init);
    }

    let cluster_data = vol
        .read_cluster(entry.start_cluster, disk)
        .map_err(|_| &b"Read error\r\n"[..])?;

    Ok(MorePhase::Outputting {
        read: ReadState {
            drive_index,
            current_cluster: entry.start_cluster,
            cluster_data,
            offset: 0,
            remaining: entry.file_size,
        },
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
