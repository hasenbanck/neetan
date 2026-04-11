//! DOSMOCK command.

use crate::{
    DiskIo, DriveIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    filesystem::{fat_dir, fat_file},
    process::COMMAND_COM_STUB,
    tables,
};

pub(crate) struct Dosmock;

impl Command for Dosmock {
    fn name(&self) -> &'static str {
        "DOSMOCK"
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningDosmock {
            args: args.to_vec(),
        })
    }
}

struct RunningDosmock {
    args: Vec<u8>,
}

impl RunningCommand for RunningDosmock {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DriveIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();
        if is_help_request(&self.args) {
            print_help(io);
            return StepResult::Done(0);
        }

        let Some(drive_index) = parse_drive_arg(args) else {
            print_help(io);
            return StepResult::Done(1);
        };

        match create_dos_mock_files(state, io, disk, drive_index) {
            Ok(()) => {
                io.print(b"DOS mock files created\r\n");
                StepResult::Done(0)
            }
            Err(message) => {
                io.print(message);
                StepResult::Done(1)
            }
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Creates minimal DOS marker files on a drive root.");
    io.println(b"");
    io.println(b"DOSMOCK drive:");
    io.println(b"");
    io.println(b"  drive:  Specifies the target drive root (for example A:).");
}

fn parse_drive_arg(args: &[u8]) -> Option<u8> {
    if args.len() != 2 && args.len() != 3 {
        return None;
    }
    if args[1] != b':' {
        return None;
    }
    if args.len() == 3 && args[2] != b'\\' && args[2] != b'/' {
        return None;
    }
    let drive_letter = args[0].to_ascii_uppercase();
    if !drive_letter.is_ascii_uppercase() {
        return None;
    }
    Some(drive_letter - b'A')
}

fn create_dos_mock_files(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    drive_index: u8,
) -> Result<(), &'static [u8]> {
    if drive_index == 25 {
        return Err(b"Target drive is not writable\r\n");
    }

    let cds_addr = tables::CDS_BASE + (drive_index as u32) * tables::CDS_ENTRY_SIZE;
    let cds_flags = io.memory.read_word(cds_addr + tables::CDS_OFF_FLAGS);
    if cds_flags == 0 {
        return Err(b"Unable to access target drive\r\n");
    }

    if state.mscdex.drive_letter == drive_index && cds_flags & tables::CDS_FLAG_PHYSICAL == 0 {
        return Err(b"Target drive is not writable\r\n");
    }

    if state
        .ensure_volume_mounted(drive_index, io.memory, disk)
        .is_err()
    {
        return Err(b"Unable to access target drive\r\n");
    }

    let (time, date) = state.dos_timestamp_now();

    let volume = state.fat_volumes[drive_index as usize]
        .as_mut()
        .ok_or(&b"Unable to access target drive\r\n"[..])?;

    let io_sys = fat_dir::name_to_fcb(b"IO.SYS");
    let msdos_sys = fat_dir::name_to_fcb(b"MSDOS.SYS");
    let command_com = fat_dir::name_to_fcb(b"COMMAND.COM");

    for name in [io_sys, msdos_sys, command_com] {
        if fat_dir::find_entry(volume, 0, &name, disk)
            .map_err(|_| &b"Unable to access target drive\r\n"[..])?
            .is_some()
        {
            return Err(b"DOS mock files already exist\r\n");
        }
    }

    let mut created = Vec::new();
    let files = [
        (
            io_sys,
            &[][..],
            fat_dir::ATTR_READ_ONLY | fat_dir::ATTR_HIDDEN | fat_dir::ATTR_SYSTEM,
        ),
        (
            msdos_sys,
            &[][..],
            fat_dir::ATTR_READ_ONLY | fat_dir::ATTR_HIDDEN | fat_dir::ATTR_SYSTEM,
        ),
        (
            command_com,
            COMMAND_COM_STUB,
            fat_dir::ATTR_READ_ONLY | fat_dir::ATTR_ARCHIVE,
        ),
    ];

    for (name, content, attributes) in files {
        if fat_file::create_or_replace_file(
            volume,
            0,
            &name,
            content,
            fat_file::FileCreateOptions {
                attributes,
                time,
                date,
            },
            disk,
        )
        .is_err()
        {
            rollback_created_files(volume, disk, &created);
            let _ = volume.flush_fat(disk);
            return Err(b"Unable to create DOS mock files\r\n");
        }
        created.push(name);
    }

    if volume.flush_fat(disk).is_err() {
        rollback_created_files(volume, disk, &created);
        let _ = volume.flush_fat(disk);
        return Err(b"Unable to create DOS mock files\r\n");
    }

    Ok(())
}

fn rollback_created_files(
    volume: &mut crate::filesystem::fat::FatVolume,
    disk: &mut dyn DiskIo,
    created: &[[u8; 11]],
) {
    for name in created {
        if let Ok(Some(entry)) = fat_dir::find_entry(volume, 0, name, disk) {
            if entry.start_cluster >= 2 {
                volume.free_chain(entry.start_cluster);
            }
            let _ = fat_dir::delete_entry(volume, &entry, disk);
        }
    }
}
