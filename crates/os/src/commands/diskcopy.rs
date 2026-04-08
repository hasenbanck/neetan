//! DISKCOPY command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
    filesystem, tables,
};

pub(crate) struct Diskcopy;

impl Command for Diskcopy {
    fn name(&self) -> &'static str {
        "DISKCOPY"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningDiskcopy {
            args: args.to_vec(),
            phase: DiskcopyPhase::Init,
        })
    }
}

struct DiskcopyState {
    src_drive_index: u8,
    src_da_ua: u8,
    dst_drive_index: u8,
    dst_da_ua: u8,
    sectors_per_track: u8,
    sector_size: u16,
    total_tracks: u32,
    current_track: u32,
    same_drive: bool,
    verify: bool,
    disk_buffer: Vec<u8>,
}

const KB_BUF_COUNT: u32 = 0x0528;

enum DiskcopyPhase {
    Init,
    PromptInsertSource(DiskcopyState),
    ReadTracks(DiskcopyState),
    PromptInsertDest(DiskcopyState),
    WriteTracks(DiskcopyState),
    VerifyTracks(DiskcopyState),
    Summary(DiskcopyState),
    PromptAnother(DiskcopyState),
}

struct RunningDiskcopy {
    args: Vec<u8>,
    phase: DiskcopyPhase,
}

impl RunningCommand for RunningDiskcopy {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, DiskcopyPhase::Init);
        match phase {
            DiskcopyPhase::Init => match init_diskcopy(io, disk, &self.args) {
                Ok(diskcopy_state) => {
                    if diskcopy_state.same_drive {
                        let drive_letter = (b'A' + diskcopy_state.src_drive_index) as char;
                        let msg = format!(
                            "Insert SOURCE diskette in drive {}:\r\nPress any key to continue . . .",
                            drive_letter
                        );
                        io.print_msg(msg.as_bytes());
                        self.phase = DiskcopyPhase::PromptInsertSource(diskcopy_state);
                    } else {
                        io.print_msg(b"\r\nReading from source disk . . .\r\n");
                        self.phase = DiskcopyPhase::ReadTracks(diskcopy_state);
                    }
                    StepResult::Continue
                }
                Err(msg) => {
                    io.print_msg(msg);
                    StepResult::Done(1)
                }
            },
            DiskcopyPhase::PromptInsertSource(diskcopy_state) => {
                if io.memory.read_byte(KB_BUF_COUNT) == 0 {
                    self.phase = DiskcopyPhase::PromptInsertSource(diskcopy_state);
                    return StepResult::Continue;
                }
                consume_key(io);
                io.print_msg(b"\r\n\r\nReading from source disk . . .\r\n");
                self.phase = DiskcopyPhase::ReadTracks(diskcopy_state);
                StepResult::Continue
            }
            DiskcopyPhase::ReadTracks(mut diskcopy_state) => {
                if diskcopy_state.current_track >= diskcopy_state.total_tracks {
                    if diskcopy_state.same_drive {
                        let drive_letter = (b'A' + diskcopy_state.dst_drive_index) as char;
                        let msg = format!(
                            "\r\nInsert DESTINATION diskette in drive {}:\r\nPress any key to continue . . .",
                            drive_letter
                        );
                        io.print_msg(msg.as_bytes());
                        diskcopy_state.current_track = 0;
                        self.phase = DiskcopyPhase::PromptInsertDest(diskcopy_state);
                    } else {
                        io.print_msg(b"\r\nWriting to destination disk . . .\r\n");
                        diskcopy_state.current_track = 0;
                        self.phase = DiskcopyPhase::WriteTracks(diskcopy_state);
                    }
                    return StepResult::Continue;
                }

                let lba = diskcopy_state.current_track * diskcopy_state.sectors_per_track as u32;
                match disk.read_sectors(
                    diskcopy_state.src_da_ua,
                    lba,
                    diskcopy_state.sectors_per_track as u32,
                ) {
                    Ok(data) => {
                        diskcopy_state.disk_buffer.extend_from_slice(&data);
                        diskcopy_state.current_track += 1;
                        self.phase = DiskcopyPhase::ReadTracks(diskcopy_state);
                        StepResult::Continue
                    }
                    Err(_) => {
                        io.print_msg(b"Read error on source disk\r\n");
                        StepResult::Done(1)
                    }
                }
            }
            DiskcopyPhase::PromptInsertDest(diskcopy_state) => {
                if io.memory.read_byte(KB_BUF_COUNT) == 0 {
                    self.phase = DiskcopyPhase::PromptInsertDest(diskcopy_state);
                    return StepResult::Continue;
                }
                consume_key(io);
                io.print_msg(b"\r\n\r\nWriting to destination disk . . .\r\n");
                self.phase = DiskcopyPhase::WriteTracks(diskcopy_state);
                StepResult::Continue
            }
            DiskcopyPhase::WriteTracks(mut diskcopy_state) => {
                if diskcopy_state.current_track >= diskcopy_state.total_tracks {
                    if diskcopy_state.verify {
                        io.print_msg(b"\r\nVerifying . . .\r\n");
                        diskcopy_state.current_track = 0;
                        self.phase = DiskcopyPhase::VerifyTracks(diskcopy_state);
                    } else {
                        self.phase = DiskcopyPhase::Summary(diskcopy_state);
                    }
                    return StepResult::Continue;
                }

                let track_size =
                    diskcopy_state.sectors_per_track as usize * diskcopy_state.sector_size as usize;
                let buffer_offset = diskcopy_state.current_track as usize * track_size;
                let track_data =
                    &diskcopy_state.disk_buffer[buffer_offset..buffer_offset + track_size];
                let lba = diskcopy_state.current_track * diskcopy_state.sectors_per_track as u32;

                if disk
                    .write_sectors(diskcopy_state.dst_da_ua, lba, track_data)
                    .is_err()
                {
                    io.print_msg(b"Write error on destination disk\r\n");
                    return StepResult::Done(1);
                }

                diskcopy_state.current_track += 1;
                self.phase = DiskcopyPhase::WriteTracks(diskcopy_state);
                StepResult::Continue
            }
            DiskcopyPhase::VerifyTracks(mut diskcopy_state) => {
                if diskcopy_state.current_track >= diskcopy_state.total_tracks {
                    self.phase = DiskcopyPhase::Summary(diskcopy_state);
                    return StepResult::Continue;
                }

                let track_size =
                    diskcopy_state.sectors_per_track as usize * diskcopy_state.sector_size as usize;
                let buffer_offset = diskcopy_state.current_track as usize * track_size;
                let expected =
                    &diskcopy_state.disk_buffer[buffer_offset..buffer_offset + track_size];
                let lba = diskcopy_state.current_track * diskcopy_state.sectors_per_track as u32;

                match disk.read_sectors(
                    diskcopy_state.dst_da_ua,
                    lba,
                    diskcopy_state.sectors_per_track as u32,
                ) {
                    Ok(readback) => {
                        if readback[..track_size] != expected[..track_size] {
                            io.print_msg(b"Verify error\r\n");
                            return StepResult::Done(1);
                        }
                    }
                    Err(_) => {
                        io.print_msg(b"Verify error\r\n");
                        return StepResult::Done(1);
                    }
                }

                diskcopy_state.current_track += 1;
                self.phase = DiskcopyPhase::VerifyTracks(diskcopy_state);
                StepResult::Continue
            }
            DiskcopyPhase::Summary(diskcopy_state) => {
                io.print_msg(b"\r\nCopy complete.\r\n");

                // Invalidate destination volume cache
                state.fat_volumes[diskcopy_state.dst_drive_index as usize] = None;

                io.print_msg(b"\r\nCopy another diskette (Y/N)?");
                self.phase = DiskcopyPhase::PromptAnother(diskcopy_state);
                StepResult::Continue
            }
            DiskcopyPhase::PromptAnother(mut diskcopy_state) => {
                if io.memory.read_byte(KB_BUF_COUNT) == 0 {
                    self.phase = DiskcopyPhase::PromptAnother(diskcopy_state);
                    return StepResult::Continue;
                }
                let key = consume_key(io);
                io.print_msg(b"\r\n");

                match key.to_ascii_uppercase() {
                    b'Y' => {
                        diskcopy_state.current_track = 0;
                        diskcopy_state.disk_buffer.clear();
                        if diskcopy_state.same_drive {
                            let drive_letter = (b'A' + diskcopy_state.src_drive_index) as char;
                            let msg = format!(
                                "\r\nInsert SOURCE diskette in drive {}:\r\nPress any key to continue . . .",
                                drive_letter
                            );
                            io.print_msg(msg.as_bytes());
                            self.phase = DiskcopyPhase::PromptInsertSource(diskcopy_state);
                        } else {
                            io.print_msg(b"\r\nReading from source disk . . .\r\n");
                            self.phase = DiskcopyPhase::ReadTracks(diskcopy_state);
                        }
                        StepResult::Continue
                    }
                    _ => StepResult::Done(0),
                }
            }
        }
    }
}

fn init_diskcopy(
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    args: &[u8],
) -> Result<DiskcopyState, &'static [u8]> {
    let args = args.trim_ascii();
    if args.is_empty() {
        return Err(b"Required parameter missing\r\n");
    }

    let mut verify = false;
    let mut drive_tokens: Vec<&[u8]> = Vec::new();

    for part in args.split(|&b| b == b' ' || b == b'\t') {
        if part.is_empty() {
            continue;
        }
        if part.len() >= 2 && part[0] == b'/' {
            if part[1].eq_ignore_ascii_case(&b'V') {
                verify = true;
            }
        } else {
            drive_tokens.push(part);
        }
    }

    if drive_tokens.is_empty() {
        return Err(b"Required parameter missing\r\n");
    }

    // Parse source drive
    let (src_drive_opt, _, _) = filesystem::split_path(drive_tokens[0]);
    let src_drive_index = src_drive_opt.ok_or(&b"Invalid drive specification\r\n"[..])?;

    let src_da_ua = io
        .memory
        .read_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_DAUA_TABLE + src_drive_index as u32);
    if src_da_ua == 0 {
        return Err(b"Invalid drive specification\r\n");
    }
    let src_da_type = src_da_ua & 0xF0;
    if src_da_type != 0x90 && src_da_type != 0x70 {
        return Err(b"Invalid drive specification\r\n");
    }

    // Parse destination drive (or same as source)
    let (dst_drive_index, dst_da_ua) = if drive_tokens.len() >= 2 {
        let (dst_drive_opt, _, _) = filesystem::split_path(drive_tokens[1]);
        let dst_idx = dst_drive_opt.ok_or(&b"Invalid drive specification\r\n"[..])?;
        let dst_da = io
            .memory
            .read_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_DAUA_TABLE + dst_idx as u32);
        if dst_da == 0 {
            return Err(b"Invalid drive specification\r\n");
        }
        let dst_da_type = dst_da & 0xF0;
        if dst_da_type != 0x90 && dst_da_type != 0x70 {
            return Err(b"Invalid drive specification\r\n");
        }
        (dst_idx, dst_da)
    } else {
        (src_drive_index, src_da_ua)
    };

    let same_drive = src_drive_index == dst_drive_index;

    // Get source geometry
    let (src_cylinders, src_heads, src_spt) = disk
        .drive_geometry(src_da_ua)
        .ok_or(&b"Invalid drive specification\r\n"[..])?;
    let src_sector_size = disk
        .sector_size(src_da_ua)
        .ok_or(&b"Invalid drive specification\r\n"[..])?;

    // Check destination geometry matches (unless same drive)
    if !same_drive {
        let (dst_cylinders, dst_heads, dst_spt) = disk
            .drive_geometry(dst_da_ua)
            .ok_or(&b"Invalid drive specification\r\n"[..])?;
        let dst_sector_size = disk
            .sector_size(dst_da_ua)
            .ok_or(&b"Invalid drive specification\r\n"[..])?;

        if src_cylinders != dst_cylinders
            || src_heads != dst_heads
            || src_spt != dst_spt
            || src_sector_size != dst_sector_size
        {
            return Err(b"Drive types or diskette types not compatible\r\n");
        }
    }

    let total_tracks = src_cylinders as u32 * src_heads as u32;
    let total_disk_bytes = total_tracks as usize * src_spt as usize * src_sector_size as usize;

    Ok(DiskcopyState {
        src_drive_index,
        src_da_ua,
        dst_drive_index,
        dst_da_ua,
        sectors_per_track: src_spt,
        sector_size: src_sector_size,
        total_tracks,
        current_track: 0,
        same_drive,
        verify,
        disk_buffer: Vec::with_capacity(total_disk_bytes),
    })
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
