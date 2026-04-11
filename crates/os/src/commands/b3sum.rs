//! B3SUM command.

use blake3::Hasher;

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    dos,
    filesystem::{fat_dir, fat_file::FatFileCursor},
};

pub(crate) struct B3sum;

impl Command for B3sum {
    fn name(&self) -> &'static str {
        "B3SUM"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningB3sum {
            args: args.to_vec(),
            phase: B3sumPhase::Init,
        })
    }
}

struct B3sumState {
    arguments: Vec<ArgumentState>,
    current_argument: usize,
}

struct ArgumentState {
    display_path: Vec<u8>,
    wildcard_prefix: Vec<u8>,
    drive_index: u8,
    dir_cluster: u16,
    pattern: [u8; 11],
    has_wildcard: bool,
    search_index: u16,
    matched_any: bool,
}

struct FileHashState {
    drive_index: u8,
    cursor: FatFileCursor,
    display_path: Vec<u8>,
    hasher: Hasher,
}

enum B3sumPhase {
    Init,
    FindNext(B3sumState),
    Hashing(B3sumState, Box<FileHashState>),
}

struct RunningB3sum {
    args: Vec<u8>,
    phase: B3sumPhase,
}

impl RunningB3sum {
    fn step_init(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();
        if args.is_empty() || is_help_request(args) {
            print_help(io);
            return StepResult::Done(0);
        }

        match init_b3sum_state(state, io, disk, args) {
            Ok(b3sum_state) => {
                self.phase = B3sumPhase::FindNext(b3sum_state);
                StepResult::Continue
            }
            Err(message) => {
                io.print(message);
                StepResult::Done(1)
            }
        }
    }

    fn step_find_next(
        &mut self,
        mut b3sum_state: B3sumState,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        if b3sum_state.current_argument >= b3sum_state.arguments.len() {
            return StepResult::Done(0);
        }

        let argument = &mut b3sum_state.arguments[b3sum_state.current_argument];
        if argument.drive_index == 25 {
            io.println(b"Access denied");
            return StepResult::Done(1);
        }

        let volume = match state.fat_volumes[argument.drive_index as usize].as_ref() {
            Some(volume) => volume,
            None => {
                io.println(b"Invalid drive");
                return StepResult::Done(1);
            }
        };

        match fat_dir::find_matching(
            volume,
            argument.dir_cluster,
            &argument.pattern,
            0,
            argument.search_index,
            disk,
        ) {
            Ok(Some((entry, next_index))) => {
                argument.search_index = next_index;

                if entry.attribute & fat_dir::ATTR_DIRECTORY != 0 {
                    io.println(b"Access denied");
                    return StepResult::Done(1);
                }

                let display_path = if argument.has_wildcard {
                    let mut display_path = argument.wildcard_prefix.clone();
                    display_path.extend_from_slice(&fat_dir::fcb_to_display_name(&entry.name));
                    display_path
                } else {
                    argument.display_path.clone()
                };

                let drive_index = argument.drive_index;
                let has_wildcard = argument.has_wildcard;
                let hasher = Hasher::new();

                if entry.file_size == 0 {
                    argument.matched_any = true;
                    print_digest(io, &hasher, &display_path);
                    if !has_wildcard {
                        b3sum_state.current_argument += 1;
                    }
                    self.phase = B3sumPhase::FindNext(b3sum_state);
                    return StepResult::Continue;
                }

                if entry.start_cluster < 2 {
                    io.println(b"Read error");
                    return StepResult::Done(1);
                }

                self.phase = B3sumPhase::Hashing(
                    b3sum_state,
                    Box::new(FileHashState {
                        drive_index,
                        cursor: FatFileCursor::new(&entry),
                        display_path,
                        hasher,
                    }),
                );
                StepResult::Continue
            }
            Ok(None) => {
                if !argument.matched_any {
                    io.println(b"File not found");
                    return StepResult::Done(1);
                }

                b3sum_state.current_argument += 1;
                self.phase = B3sumPhase::FindNext(b3sum_state);
                StepResult::Continue
            }
            Err(_) => {
                io.println(b"File not found");
                StepResult::Done(1)
            }
        }
    }

    fn step_hashing(
        &mut self,
        mut b3sum_state: B3sumState,
        mut file_hash_state: Box<FileHashState>,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let volume = match state.fat_volumes[file_hash_state.drive_index as usize].as_ref() {
            Some(volume) => volume,
            None => {
                io.println(b"Invalid drive");
                return StepResult::Done(1);
            }
        };

        let cluster_data = match file_hash_state.cursor.read_chunk(
            volume,
            disk,
            volume.bpb.cluster_size() as usize,
        ) {
            Ok(cluster_data) => cluster_data,
            Err(_) => {
                io.println(b"Read error");
                return StepResult::Done(1);
            }
        };

        if cluster_data.is_empty() {
            let current_argument = &mut b3sum_state.arguments[b3sum_state.current_argument];
            current_argument.matched_any = true;
            print_digest(io, &file_hash_state.hasher, &file_hash_state.display_path);
            if !current_argument.has_wildcard {
                b3sum_state.current_argument += 1;
            }
            self.phase = B3sumPhase::FindNext(b3sum_state);
            return StepResult::Continue;
        }

        file_hash_state.hasher.update(&cluster_data);
        self.phase = B3sumPhase::Hashing(b3sum_state, file_hash_state);
        StepResult::Continue
    }
}

impl RunningCommand for RunningB3sum {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, B3sumPhase::Init);
        match phase {
            B3sumPhase::Init => self.step_init(state, io, disk),
            B3sumPhase::FindNext(b3sum_state) => self.step_find_next(b3sum_state, state, io, disk),
            B3sumPhase::Hashing(b3sum_state, file_hash_state) => {
                self.step_hashing(b3sum_state, file_hash_state, state, io, disk)
            }
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Computes BLAKE3 hashes of files.");
    io.println(b"");
    io.println(b"B3SUM [drive:][path]filename [...]");
    io.println(b"");
    io.println(b"  filename  Specifies one or more files to hash.");
}

fn init_b3sum_state(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    args: &[u8],
) -> Result<B3sumState, &'static [u8]> {
    let mut arguments = Vec::new();

    for part in args.split(|&byte| byte == b' ' || byte == b'\t') {
        if part.is_empty() {
            continue;
        }

        let normalized_path = dos::normalize_path(part);
        let has_wildcard = normalized_path.contains(&b'*') || normalized_path.contains(&b'?');
        let (drive_index, dir_cluster, pattern) = state
            .resolve_file_path(&normalized_path, io.memory, disk)
            .map_err(|_| &b"File not found\r\n"[..])?;

        arguments.push(ArgumentState {
            display_path: normalized_path.clone(),
            wildcard_prefix: wildcard_display_prefix(&normalized_path),
            drive_index,
            dir_cluster,
            pattern,
            has_wildcard,
            search_index: 0,
            matched_any: false,
        });
    }

    if arguments.is_empty() {
        return Err(b"Required parameter missing\r\n");
    }

    Ok(B3sumState {
        arguments,
        current_argument: 0,
    })
}

fn wildcard_display_prefix(path: &[u8]) -> Vec<u8> {
    if let Some(position) = path.iter().rposition(|&byte| byte == b'\\') {
        return path[..=position].to_vec();
    }
    if path.len() >= 2 && path[1] == b':' {
        return path[..2].to_vec();
    }
    Vec::new()
}

fn print_digest(io: &mut IoAccess, hasher: &Hasher, display_path: &[u8]) {
    let mut digest = [0u8; 32];
    let mut line = Vec::with_capacity(64 + 2 + display_path.len() + 2);
    hasher.finalize(&mut digest);
    push_digest_hex(&mut line, &digest);
    line.extend_from_slice(b"  ");
    line.extend_from_slice(display_path);
    line.extend_from_slice(b"\r\n");
    io.print(&line);
}

fn push_digest_hex(out: &mut Vec<u8>, digest: &[u8; 32]) {
    for &byte in digest {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0F));
    }
}

fn nibble_to_hex(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'a' + (nibble - 10),
    }
}
