//! COPY command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    filesystem::{fat_dir, fat_file::FatFileCursor},
};

pub(crate) struct Copy;

impl Command for Copy {
    fn name(&self) -> &'static str {
        "COPY"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningCopy {
            args: args.to_vec(),
            phase: CopyPhase::Init,
        })
    }
}

struct CopyState {
    // Source files (may be multiple for concatenation)
    sources: Vec<SourceSpec>,
    current_source: usize,
    src_search_index: u16,
    dst_path: Vec<u8>,
    dst_is_dir: bool,
    dst_drive: u8,
    dst_dir_cluster: u16,
    files_copied: u32,
    verify: bool,
    overwrite_all: bool,
    concatenating: bool,
}

struct SourceSpec {
    drive: u8,
    dir_cluster: u16,
    pattern: [u8; 11],
}

struct FileCopyState {
    src_drive: u8,
    src_cursor: FatFileCursor,
    src_entry: fat_dir::DirEntry,
    dst_drive: u8,
    dst_dir_cluster: u16,
    dst_fcb_name: [u8; 11],
    dst_first_cluster: u16,
    dst_last_cluster: u16,
    dst_total_written: u32,
}

const KB_BUF_COUNT: u32 = 0x0528;

enum CopyPhase {
    Init,
    FindNext(CopyState),
    ConfirmOverwrite(CopyState, FileCopyState),
    ReadChunk(CopyState, FileCopyState),
    WriteChunk(CopyState, FileCopyState, Vec<u8>),
    VerifyChunk(CopyState, FileCopyState, u16, Vec<u8>),
    FinishFile(CopyState, FileCopyState),
    // Concatenation: append next source to same dest
    ConcatNextSource(CopyState, FileCopyState),
    ConcatRead(CopyState, FileCopyState),
    ConcatWrite(CopyState, FileCopyState, Vec<u8>),
    Summary(u32),
}

struct RunningCopy {
    args: Vec<u8>,
    phase: CopyPhase,
}

fn destination_fcb_name(dst_path: &[u8]) -> [u8; 11] {
    let (_, components, _) = crate::filesystem::split_path(dst_path);
    let name = components.last().copied().unwrap_or(dst_path);
    fat_dir::name_to_fcb(name)
}

impl RunningCopy {
    fn step_init(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        if is_help_request(&self.args) || self.args.trim_ascii().is_empty() {
            print_help(io);
            return StepResult::Done(0);
        }
        match init_copy(state, io, disk, &self.args) {
            Ok(copy_state) => {
                self.phase = CopyPhase::FindNext(copy_state);
                StepResult::Continue
            }
            Err(msg) => {
                io.print(msg);
                StepResult::Done(1)
            }
        }
    }

    fn step_find_next(
        &mut self,
        mut copy_state: CopyState,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        if copy_state.current_source >= copy_state.sources.len() {
            if copy_state.files_copied == 0 {
                io.println(b"File not found");
                return StepResult::Done(1);
            }
            self.phase = CopyPhase::Summary(copy_state.files_copied);
            return StepResult::Continue;
        }

        let src = &copy_state.sources[copy_state.current_source];
        let vol = match state.fat_volumes[src.drive as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        let result = fat_dir::find_matching(
            vol,
            src.dir_cluster,
            &src.pattern,
            0,
            copy_state.src_search_index,
            disk,
        );

        match result {
            Ok(Some((entry, next_index))) => {
                if entry.attribute & fat_dir::ATTR_DIRECTORY != 0 {
                    copy_state.src_search_index = next_index;
                    self.phase = CopyPhase::FindNext(copy_state);
                    return StepResult::Continue;
                }

                copy_state.src_search_index = next_index;

                let dst_fcb_name = if copy_state.dst_is_dir {
                    entry.name
                } else {
                    destination_fcb_name(&copy_state.dst_path)
                };

                let display_name = fat_dir::fcb_to_display_name(&entry.name);
                for &byte in &display_name {
                    io.output_byte(byte);
                }
                io.println(b"");

                let file_state = FileCopyState {
                    src_drive: copy_state.sources[copy_state.current_source].drive,
                    src_cursor: FatFileCursor::new(&entry),
                    src_entry: entry,
                    dst_drive: copy_state.dst_drive,
                    dst_dir_cluster: copy_state.dst_dir_cluster,
                    dst_fcb_name,
                    dst_first_cluster: 0,
                    dst_last_cluster: 0,
                    dst_total_written: 0,
                };

                // Check if dest exists and /Y not set
                if !copy_state.overwrite_all {
                    let dst_vol = match state.fat_volumes[file_state.dst_drive as usize].as_ref() {
                        Some(v) => v,
                        None => return StepResult::Done(1),
                    };
                    if fat_dir::find_entry(
                        dst_vol,
                        file_state.dst_dir_cluster,
                        &file_state.dst_fcb_name,
                        disk,
                    )
                    .ok()
                    .flatten()
                    .is_some()
                    {
                        io.print(b"Overwrite (Yes/No/All)?");
                        self.phase = CopyPhase::ConfirmOverwrite(copy_state, file_state);
                        return StepResult::Continue;
                    }
                }

                if file_state.src_entry.file_size == 0 {
                    self.phase = CopyPhase::FinishFile(copy_state, file_state);
                } else {
                    self.phase = CopyPhase::ReadChunk(copy_state, file_state);
                }
                StepResult::Continue
            }
            Ok(None) => {
                // Move to next source spec (for non-concatenation multi-source)
                copy_state.current_source += 1;
                copy_state.src_search_index = 0;
                if copy_state.current_source < copy_state.sources.len() && !copy_state.concatenating
                {
                    self.phase = CopyPhase::FindNext(copy_state);
                } else if copy_state.files_copied == 0 {
                    io.println(b"File not found");
                    return StepResult::Done(1);
                } else {
                    self.phase = CopyPhase::Summary(copy_state.files_copied);
                }
                StepResult::Continue
            }
            Err(_) => {
                io.println(b"File not found");
                StepResult::Done(1)
            }
        }
    }

    fn step_confirm_overwrite(
        &mut self,
        mut copy_state: CopyState,
        file_state: FileCopyState,
        io: &mut IoAccess,
    ) -> StepResult {
        if io.memory.read_byte(KB_BUF_COUNT) == 0 {
            self.phase = CopyPhase::ConfirmOverwrite(copy_state, file_state);
            return StepResult::Continue;
        }
        let key = consume_key(io);
        io.output_byte(b'\r');
        io.output_byte(b'\n');

        match key.to_ascii_uppercase() {
            b'Y' => {
                if file_state.src_entry.file_size == 0 {
                    self.phase = CopyPhase::FinishFile(copy_state, file_state);
                } else {
                    self.phase = CopyPhase::ReadChunk(copy_state, file_state);
                }
            }
            b'A' => {
                copy_state.overwrite_all = true;
                if file_state.src_entry.file_size == 0 {
                    self.phase = CopyPhase::FinishFile(copy_state, file_state);
                } else {
                    self.phase = CopyPhase::ReadChunk(copy_state, file_state);
                }
            }
            _ => {
                // Skip this file
                self.phase = CopyPhase::FindNext(copy_state);
            }
        }
        StepResult::Continue
    }

    fn step_read_chunk(
        &mut self,
        copy_state: CopyState,
        mut file_state: FileCopyState,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        if file_state.src_cursor.remaining() == 0 {
            if copy_state.concatenating {
                self.phase = CopyPhase::ConcatNextSource(copy_state, file_state);
            } else {
                self.phase = CopyPhase::FinishFile(copy_state, file_state);
            }
            return StepResult::Continue;
        }

        let dst_cluster_size = match state.fat_volumes[file_state.dst_drive as usize].as_ref() {
            Some(v) => v.bpb.cluster_size() as usize,
            None => return StepResult::Done(1),
        };
        let src_vol = match state.fat_volumes[file_state.src_drive as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };
        let write_data = match file_state
            .src_cursor
            .read_chunk(src_vol, disk, dst_cluster_size)
        {
            Ok(data) => data,
            Err(_) => {
                io.println(b"Read error");
                return StepResult::Done(1);
            }
        };

        if write_data.is_empty() {
            if copy_state.concatenating {
                self.phase = CopyPhase::ConcatNextSource(copy_state, file_state);
            } else {
                self.phase = CopyPhase::FinishFile(copy_state, file_state);
            }
            return StepResult::Continue;
        }

        self.phase = CopyPhase::WriteChunk(copy_state, file_state, write_data);
        StepResult::Continue
    }

    fn step_write_chunk(
        &mut self,
        copy_state: CopyState,
        mut file_state: FileCopyState,
        data: Vec<u8>,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let vol = match state.fat_volumes[file_state.dst_drive as usize].as_mut() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        let new_cluster = match vol.allocate_cluster(file_state.dst_last_cluster) {
            Some(c) => c,
            None => {
                io.println(b"Insufficient disk space");
                return StepResult::Done(1);
            }
        };

        if file_state.dst_first_cluster == 0 {
            file_state.dst_first_cluster = new_cluster;
        }
        file_state.dst_last_cluster = new_cluster;

        let bytes_to_write = data.len();
        let cluster_size = vol.bpb.cluster_size() as usize;
        let mut write_data = data.clone();
        write_data.resize(cluster_size, 0);

        if vol.write_cluster(new_cluster, &write_data, disk).is_err() {
            io.println(b"Write error");
            return StepResult::Done(1);
        }

        file_state.dst_total_written += bytes_to_write as u32;

        if copy_state.verify {
            self.phase = CopyPhase::VerifyChunk(copy_state, file_state, new_cluster, data);
        } else {
            self.phase = CopyPhase::ReadChunk(copy_state, file_state);
        }
        StepResult::Continue
    }

    #[allow(clippy::too_many_arguments)]
    fn step_verify_chunk(
        &mut self,
        copy_state: CopyState,
        file_state: FileCopyState,
        written_cluster: u16,
        original_data: Vec<u8>,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        // /V: re-read the written cluster and compare
        let vol = match state.fat_volumes[file_state.dst_drive as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        let readback = match vol.read_cluster(written_cluster, disk) {
            Ok(d) => d,
            Err(_) => {
                io.println(b"Verify error");
                return StepResult::Done(1);
            }
        };

        let compare_len = original_data.len();
        if readback[..compare_len] != original_data[..compare_len] {
            io.println(b"Verify error");
            return StepResult::Done(1);
        }

        self.phase = CopyPhase::ReadChunk(copy_state, file_state);
        StepResult::Continue
    }

    fn step_finish_file(
        &mut self,
        mut copy_state: CopyState,
        file_state: FileCopyState,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let vol = match state.fat_volumes[file_state.dst_drive as usize].as_mut() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        // Remove existing dest if present
        if let Ok(Some(existing)) = fat_dir::find_entry(
            vol,
            file_state.dst_dir_cluster,
            &file_state.dst_fcb_name,
            disk,
        ) {
            if existing.start_cluster >= 2 {
                vol.free_chain(existing.start_cluster);
            }
            let _ = fat_dir::delete_entry(vol, &existing, disk);
        }

        let new_entry = fat_dir::DirEntry {
            name: file_state.dst_fcb_name,
            attribute: file_state.src_entry.attribute & 0x27,
            time: file_state.src_entry.time,
            date: file_state.src_entry.date,
            start_cluster: file_state.dst_first_cluster,
            file_size: file_state.dst_total_written,
            dir_sector: 0,
            dir_offset: 0,
        };

        if fat_dir::create_entry(vol, file_state.dst_dir_cluster, &new_entry, disk).is_err() {
            io.println(b"Unable to create destination");
            return StepResult::Done(1);
        }

        let _ = vol.flush_fat(disk);
        copy_state.files_copied += 1;

        self.phase = CopyPhase::FindNext(copy_state);
        StepResult::Continue
    }

    fn step_concat_next_source(
        &mut self,
        mut copy_state: CopyState,
        mut file_state: FileCopyState,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        // Move to next source in the concatenation list
        copy_state.current_source += 1;
        if copy_state.current_source >= copy_state.sources.len() {
            // All sources consumed, finish the destination file
            self.phase = CopyPhase::FinishFile(copy_state, file_state);
            return StepResult::Continue;
        }

        let src = &copy_state.sources[copy_state.current_source];
        let vol = match state.fat_volumes[src.drive as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        // Find the file for this source (exact match, no wildcard iteration)
        let entry = match fat_dir::find_matching(vol, src.dir_cluster, &src.pattern, 0, 0, disk) {
            Ok(Some((e, _))) => e,
            _ => {
                // Skip missing concat sources
                self.phase = CopyPhase::ConcatNextSource(copy_state, file_state);
                return StepResult::Continue;
            }
        };

        let display_name = fat_dir::fcb_to_display_name(&entry.name);
        for &byte in &display_name {
            io.output_byte(byte);
        }
        io.println(b"");

        file_state.src_drive = src.drive;
        file_state.src_cursor = FatFileCursor::new(&entry);

        if entry.file_size == 0 {
            self.phase = CopyPhase::ConcatNextSource(copy_state, file_state);
        } else {
            self.phase = CopyPhase::ConcatRead(copy_state, file_state);
        }
        StepResult::Continue
    }

    fn step_concat_read(
        &mut self,
        copy_state: CopyState,
        mut file_state: FileCopyState,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        if file_state.src_cursor.remaining() == 0 {
            self.phase = CopyPhase::ConcatNextSource(copy_state, file_state);
            return StepResult::Continue;
        }

        let dst_cluster_size = match state.fat_volumes[file_state.dst_drive as usize].as_ref() {
            Some(v) => v.bpb.cluster_size() as usize,
            None => return StepResult::Done(1),
        };
        let src_vol = match state.fat_volumes[file_state.src_drive as usize].as_ref() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };
        let write_data = match file_state
            .src_cursor
            .read_chunk(src_vol, disk, dst_cluster_size)
        {
            Ok(data) => data,
            Err(_) => {
                io.println(b"Read error");
                return StepResult::Done(1);
            }
        };

        if write_data.is_empty() {
            self.phase = CopyPhase::ConcatNextSource(copy_state, file_state);
            return StepResult::Continue;
        }

        self.phase = CopyPhase::ConcatWrite(copy_state, file_state, write_data);
        StepResult::Continue
    }

    fn step_concat_write(
        &mut self,
        copy_state: CopyState,
        mut file_state: FileCopyState,
        data: Vec<u8>,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let vol = match state.fat_volumes[file_state.dst_drive as usize].as_mut() {
            Some(v) => v,
            None => return StepResult::Done(1),
        };

        let new_cluster = match vol.allocate_cluster(file_state.dst_last_cluster) {
            Some(c) => c,
            None => {
                io.println(b"Insufficient disk space");
                return StepResult::Done(1);
            }
        };

        if file_state.dst_first_cluster == 0 {
            file_state.dst_first_cluster = new_cluster;
        }
        file_state.dst_last_cluster = new_cluster;

        let bytes_to_write = data.len();
        let cluster_size = vol.bpb.cluster_size() as usize;
        let mut write_data = data;
        write_data.resize(cluster_size, 0);

        if vol.write_cluster(new_cluster, &write_data, disk).is_err() {
            io.println(b"Write error");
            return StepResult::Done(1);
        }

        file_state.dst_total_written += bytes_to_write as u32;

        self.phase = CopyPhase::ConcatRead(copy_state, file_state);
        StepResult::Continue
    }
}

impl RunningCommand for RunningCopy {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, CopyPhase::Init);
        match phase {
            CopyPhase::Init => self.step_init(state, io, disk),
            CopyPhase::FindNext(cs) => self.step_find_next(cs, state, io, disk),
            CopyPhase::ConfirmOverwrite(cs, fs) => self.step_confirm_overwrite(cs, fs, io),
            CopyPhase::ReadChunk(cs, fs) => self.step_read_chunk(cs, fs, state, io, disk),
            CopyPhase::WriteChunk(cs, fs, data) => {
                self.step_write_chunk(cs, fs, data, state, io, disk)
            }
            CopyPhase::VerifyChunk(cs, fs, cluster, data) => {
                self.step_verify_chunk(cs, fs, cluster, data, state, io, disk)
            }
            CopyPhase::FinishFile(cs, fs) => self.step_finish_file(cs, fs, state, io, disk),
            CopyPhase::ConcatNextSource(cs, fs) => {
                self.step_concat_next_source(cs, fs, state, io, disk)
            }
            CopyPhase::ConcatRead(cs, fs) => self.step_concat_read(cs, fs, state, io, disk),
            CopyPhase::ConcatWrite(cs, fs, data) => {
                self.step_concat_write(cs, fs, data, state, io, disk)
            }
            CopyPhase::Summary(count) => {
                let msg = format!("     {:>4} file(s) copied\r\n", count);
                io.print(msg.as_bytes());
                StepResult::Done(0)
            }
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Copies one or more files to another location.");
    io.println(b"");
    io.println(b"COPY [/V] [/Y] source destination");
    io.println(b"COPY [/V] [/Y] source1+source2[+...] destination");
    io.println(b"");
    io.println(b"  /V  Verifies that new files are written correctly.");
    io.println(b"  /Y  Overwrites existing files without prompting.");
}

fn init_copy(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    args: &[u8],
) -> Result<CopyState, &'static [u8]> {
    let args = args.trim_ascii();
    if args.is_empty() {
        return Err(b"Required parameter missing\r\n");
    }

    let mut verify = false;
    let mut overwrite_all = false;
    let mut tokens: Vec<&[u8]> = Vec::new();

    for part in args.split(|&b| b == b' ' || b == b'\t') {
        if part.is_empty() {
            continue;
        }
        if part.len() >= 2 && part[0] == b'/' {
            match part[1].to_ascii_uppercase() {
                b'V' => verify = true,
                b'Y' => overwrite_all = true,
                _ => {}
            }
        } else {
            tokens.push(part);
        }
    }

    if tokens.is_empty() {
        return Err(b"Required parameter missing\r\n");
    }

    // Check for concatenation: if first token contains '+', split into multiple sources
    let concatenating = tokens[0].contains(&b'+');

    if concatenating {
        // COPY A+B+C DEST
        if tokens.len() < 2 {
            return Err(b"Required parameter missing\r\n");
        }
        let source_part = tokens[0];
        let dest = tokens[1];

        let mut sources = Vec::new();
        for src_name in source_part.split(|&b| b == b'+') {
            if src_name.is_empty() {
                continue;
            }
            let (drive, dir_cluster, pattern) = state
                .resolve_file_path(src_name, io.memory, disk)
                .map_err(|_| &b"File not found\r\n"[..])?;
            sources.push(SourceSpec {
                drive,
                dir_cluster,
                pattern,
            });
        }

        if sources.is_empty() {
            return Err(b"Required parameter missing\r\n");
        }

        let (dst_drive, dst_dir_cluster, dst_is_dir) =
            match state.resolve_dir_path(dest, io.memory, disk) {
                Ok((drive, cluster)) => (drive, cluster, true),
                Err(_) => {
                    let (drive, dir_cluster, _fcb) = state
                        .resolve_file_path(dest, io.memory, disk)
                        .map_err(|_| &b"Invalid destination\r\n"[..])?;
                    (drive, dir_cluster, false)
                }
            };

        if dst_drive == 25 {
            return Err(b"Access denied\r\n");
        }

        Ok(CopyState {
            sources,
            current_source: 0,
            src_search_index: 0,
            dst_path: dest.to_vec(),
            dst_is_dir,
            dst_drive,
            dst_dir_cluster,
            files_copied: 0,
            verify,
            overwrite_all,
            concatenating: true,
        })
    } else {
        // Normal COPY SRC DEST
        if tokens.len() < 2 {
            return Err(b"Required parameter missing\r\n");
        }

        let source = tokens[0];
        let dest = tokens[1];

        let (src_drive, src_dir_cluster, src_pattern) = state
            .resolve_file_path(source, io.memory, disk)
            .map_err(|_| &b"File not found\r\n"[..])?;

        if src_drive == 25 {
            return Err(b"Access denied\r\n");
        }

        let (dst_drive, dst_dir_cluster, dst_is_dir) =
            match state.resolve_dir_path(dest, io.memory, disk) {
                Ok((drive, cluster)) => (drive, cluster, true),
                Err(_) => {
                    let (drive, dir_cluster, _fcb) = state
                        .resolve_file_path(dest, io.memory, disk)
                        .map_err(|_| &b"Invalid destination\r\n"[..])?;
                    (drive, dir_cluster, false)
                }
            };

        if dst_drive == 25 {
            return Err(b"Access denied\r\n");
        }

        Ok(CopyState {
            sources: vec![SourceSpec {
                drive: src_drive,
                dir_cluster: src_dir_cluster,
                pattern: src_pattern,
            }],
            current_source: 0,
            src_search_index: 0,
            dst_path: dest.to_vec(),
            dst_is_dir,
            dst_drive,
            dst_dir_cluster,
            files_copied: 0,
            verify,
            overwrite_all,
            concatenating: false,
        })
    }
}

fn consume_key(io: &mut IoAccess) -> u8 {
    let head = io.memory.read_word(0x0524) as u32;
    let ch = io.memory.read_byte(head);
    let mut new_head = head + 2;
    if new_head >= 0x0522 {
        new_head = 0x0502;
    }
    io.memory.write_word(0x0524, new_head as u16);
    let count = io.memory.read_byte(KB_BUF_COUNT);
    if count > 0 {
        io.memory.write_byte(KB_BUF_COUNT, count - 1);
    }
    ch
}
