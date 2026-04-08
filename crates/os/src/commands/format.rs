//! FORMAT command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
    filesystem, tables,
};

pub(crate) struct Format;

impl Command for Format {
    fn name(&self) -> &'static str {
        "FORMAT"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningFormat {
            args: args.to_vec(),
            phase: FormatPhase::Init,
        })
    }
}

struct FormatState {
    drive_index: u8,
    da_ua: u8,
    heads: u8,
    sectors_per_track: u8,
    sector_size: u16,
    total_sectors: u32,
    total_tracks: u32,
    current_track: u32,
    quick: bool,
    verify: bool,
}

struct BpbParams {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entry_count: u16,
    media_descriptor: u8,
    sectors_per_fat: u16,
}

fn floppy_bpb_params(da_type: u8) -> BpbParams {
    match da_type {
        0x90 => BpbParams {
            bytes_per_sector: 1024,
            sectors_per_cluster: 1,
            reserved_sectors: 1,
            num_fats: 2,
            root_entry_count: 192,
            media_descriptor: 0xFE,
            sectors_per_fat: 2,
        },
        0x70 => BpbParams {
            bytes_per_sector: 512,
            sectors_per_cluster: 2,
            reserved_sectors: 1,
            num_fats: 2,
            root_entry_count: 112,
            media_descriptor: 0xFE,
            sectors_per_fat: 3,
        },
        _ => panic!("unsupported floppy DA type {:#04X}", da_type),
    }
}

const KB_BUF_COUNT: u32 = 0x0528;

enum FormatPhase {
    Init,
    Confirm(FormatState),
    FormatTrack(FormatState),
    WriteBootSector(FormatState),
    WriteFat(FormatState),
    WriteRootDir(FormatState),
    VerifyTrack(FormatState),
    Summary(FormatState),
}

struct RunningFormat {
    args: Vec<u8>,
    phase: FormatPhase,
}

impl RunningCommand for RunningFormat {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let phase = std::mem::replace(&mut self.phase, FormatPhase::Init);
        match phase {
            FormatPhase::Init => match init_format(state, io, disk, &self.args) {
                Ok(format_state) => {
                    let drive_letter = (b'A' + format_state.drive_index) as char;
                    let msg = format!(
                        "\r\nWARNING, ALL DATA ON DRIVE {}: WILL BE LOST!\r\nProceed with Format (Y/N)?",
                        drive_letter
                    );
                    io.print_msg(msg.as_bytes());
                    self.phase = FormatPhase::Confirm(format_state);
                    StepResult::Continue
                }
                Err(msg) => {
                    io.print_msg(msg);
                    StepResult::Done(1)
                }
            },
            FormatPhase::Confirm(format_state) => {
                if io.memory.read_byte(KB_BUF_COUNT) == 0 {
                    self.phase = FormatPhase::Confirm(format_state);
                    return StepResult::Continue;
                }
                let key = consume_key(io);
                io.print_msg(b"\r\n");

                match key.to_ascii_uppercase() {
                    b'Y' => {
                        if format_state.quick {
                            self.phase = FormatPhase::WriteBootSector(format_state);
                        } else {
                            self.phase = FormatPhase::FormatTrack(format_state);
                        }
                        StepResult::Continue
                    }
                    _ => {
                        io.print_msg(b"Format terminated.\r\n");
                        StepResult::Done(0)
                    }
                }
            }
            FormatPhase::FormatTrack(mut format_state) => {
                if format_state.current_track >= format_state.total_tracks {
                    self.phase = FormatPhase::WriteBootSector(format_state);
                    return StepResult::Continue;
                }

                let track_size =
                    format_state.sectors_per_track as usize * format_state.sector_size as usize;
                let fill_data = vec![0xF6u8; track_size];
                let lba = format_state.current_track * format_state.sectors_per_track as u32;

                if disk
                    .write_sectors(format_state.da_ua, lba, &fill_data)
                    .is_err()
                {
                    io.print_msg(b"Write error during format\r\n");
                    return StepResult::Done(1);
                }

                format_state.current_track += 1;
                self.phase = FormatPhase::FormatTrack(format_state);
                StepResult::Continue
            }
            FormatPhase::WriteBootSector(format_state) => {
                let da_type = format_state.da_ua & 0xF0;
                let bpb = floppy_bpb_params(da_type);

                let mut boot = vec![0u8; format_state.sector_size as usize];
                boot[0] = 0xEB;
                boot[1] = 0x3C;
                boot[2] = 0x90;
                boot[3..11].copy_from_slice(b"NEETAN  ");
                boot[11..13].copy_from_slice(&bpb.bytes_per_sector.to_le_bytes());
                boot[13] = bpb.sectors_per_cluster;
                boot[14..16].copy_from_slice(&bpb.reserved_sectors.to_le_bytes());
                boot[16] = bpb.num_fats;
                boot[17..19].copy_from_slice(&bpb.root_entry_count.to_le_bytes());
                let total_16 = format_state.total_sectors.min(0xFFFF) as u16;
                boot[19..21].copy_from_slice(&total_16.to_le_bytes());
                boot[21] = bpb.media_descriptor;
                boot[22..24].copy_from_slice(&bpb.sectors_per_fat.to_le_bytes());
                boot[24..26]
                    .copy_from_slice(&(format_state.sectors_per_track as u16).to_le_bytes());
                boot[26..28].copy_from_slice(&(format_state.heads as u16).to_le_bytes());

                if disk.write_sectors(format_state.da_ua, 0, &boot).is_err() {
                    io.print_msg(b"Write error\r\n");
                    return StepResult::Done(1);
                }

                self.phase = FormatPhase::WriteFat(format_state);
                StepResult::Continue
            }
            FormatPhase::WriteFat(format_state) => {
                let da_type = format_state.da_ua & 0xF0;
                let bpb = floppy_bpb_params(da_type);

                let fat_size = bpb.sectors_per_fat as usize * format_state.sector_size as usize;
                let mut fat_data = vec![0u8; fat_size];
                // FAT12 reserved entries: media descriptor + 0xFF fill
                fat_data[0] = bpb.media_descriptor;
                fat_data[1] = 0xFF;
                fat_data[2] = 0xFF;

                // Write both FAT copies
                for fat_idx in 0..bpb.num_fats as u32 {
                    let fat_lba =
                        bpb.reserved_sectors as u32 + fat_idx * bpb.sectors_per_fat as u32;
                    if disk
                        .write_sectors(format_state.da_ua, fat_lba, &fat_data)
                        .is_err()
                    {
                        io.print_msg(b"Write error\r\n");
                        return StepResult::Done(1);
                    }
                }

                self.phase = FormatPhase::WriteRootDir(format_state);
                StepResult::Continue
            }
            FormatPhase::WriteRootDir(mut format_state) => {
                let da_type = format_state.da_ua & 0xF0;
                let bpb = floppy_bpb_params(da_type);

                let root_dir_sectors =
                    (bpb.root_entry_count as u32 * 32).div_ceil(format_state.sector_size as u32);
                let root_lba =
                    bpb.reserved_sectors as u32 + bpb.num_fats as u32 * bpb.sectors_per_fat as u32;
                let root_size = root_dir_sectors as usize * format_state.sector_size as usize;
                let root_data = vec![0u8; root_size];

                if disk
                    .write_sectors(format_state.da_ua, root_lba, &root_data)
                    .is_err()
                {
                    io.print_msg(b"Write error\r\n");
                    return StepResult::Done(1);
                }

                if format_state.verify {
                    format_state.current_track = 0;
                    self.phase = FormatPhase::VerifyTrack(format_state);
                } else {
                    self.phase = FormatPhase::Summary(format_state);
                }
                StepResult::Continue
            }
            FormatPhase::VerifyTrack(mut format_state) => {
                if format_state.current_track >= format_state.total_tracks {
                    self.phase = FormatPhase::Summary(format_state);
                    return StepResult::Continue;
                }

                let lba = format_state.current_track * format_state.sectors_per_track as u32;
                if disk
                    .read_sectors(
                        format_state.da_ua,
                        lba,
                        format_state.sectors_per_track as u32,
                    )
                    .is_err()
                {
                    io.print_msg(b"Verify error\r\n");
                    return StepResult::Done(1);
                }

                format_state.current_track += 1;
                self.phase = FormatPhase::VerifyTrack(format_state);
                StepResult::Continue
            }
            FormatPhase::Summary(format_state) => {
                let da_type = format_state.da_ua & 0xF0;
                let bpb = floppy_bpb_params(da_type);

                let total_bytes =
                    format_state.total_sectors as u64 * format_state.sector_size as u64;

                let root_dir_sectors =
                    (bpb.root_entry_count as u32 * 32).div_ceil(format_state.sector_size as u32);
                let system_sectors = bpb.reserved_sectors as u32
                    + bpb.num_fats as u32 * bpb.sectors_per_fat as u32
                    + root_dir_sectors;
                let data_sectors = format_state.total_sectors.saturating_sub(system_sectors);
                let available_bytes = data_sectors as u64 * format_state.sector_size as u64;

                io.print_msg(b"Format complete.\r\n\r\n");
                let msg = format!("  {:>12} bytes total disk space\r\n", total_bytes);
                io.print_msg(msg.as_bytes());
                let msg = format!("  {:>12} bytes available on disk\r\n", available_bytes);
                io.print_msg(msg.as_bytes());

                // Invalidate cached volume so next access re-mounts from fresh disk
                state.fat_volumes[format_state.drive_index as usize] = None;

                StepResult::Done(0)
            }
        }
    }
}

fn init_format(
    _state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    args: &[u8],
) -> Result<FormatState, &'static [u8]> {
    let args = args.trim_ascii();
    if args.is_empty() {
        return Err(b"Required parameter missing\r\n");
    }

    let mut quick = false;
    let mut unconditional = false;
    let mut verify = false;
    let mut drive_token: Option<&[u8]> = None;

    for part in args.split(|&b| b == b' ' || b == b'\t') {
        if part.is_empty() {
            continue;
        }
        if part.len() >= 2 && part[0] == b'/' {
            match part[1].to_ascii_uppercase() {
                b'Q' => quick = true,
                b'U' => unconditional = true,
                b'V' => verify = true,
                _ => {}
            }
        } else {
            drive_token = Some(part);
        }
    }

    let drive_token = drive_token.ok_or(&b"Required parameter missing\r\n"[..])?;

    // Extract drive letter
    let (drive_opt, _, _) = filesystem::split_path(drive_token);
    let drive_index = drive_opt.ok_or(&b"Invalid drive specification\r\n"[..])?;

    // Look up DA/UA
    let da_ua = io
        .memory
        .read_byte(tables::IOSYS_BASE + tables::IOSYS_OFF_DAUA_TABLE + drive_index as u32);
    if da_ua == 0 {
        return Err(b"Invalid drive specification\r\n");
    }

    let da_type = da_ua & 0xF0;
    if da_type != 0x90 && da_type != 0x70 {
        return Err(b"Invalid drive specification\r\n");
    }

    let (cylinders, heads, sectors_per_track) = disk
        .drive_geometry(da_ua)
        .ok_or(&b"Invalid drive specification\r\n"[..])?;
    let sector_size = disk
        .sector_size(da_ua)
        .ok_or(&b"Invalid drive specification\r\n"[..])?;

    let total_tracks = cylinders as u32 * heads as u32;
    let total_sectors = total_tracks * sectors_per_track as u32;

    // Suppress "unconditional" unused warning -- /U is accepted but full format is the default
    let _ = unconditional;

    Ok(FormatState {
        drive_index,
        da_ua,
        heads,
        sectors_per_track,
        sector_size,
        total_sectors,
        total_tracks,
        current_track: 0,
        quick,
        verify,
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
