//! RD / RMDIR command.

use crate::{
    DiskIo, DriveIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
    filesystem::{fat::FatVolume, fat_dir},
};

pub(crate) struct Rd;

impl Command for Rd {
    fn name(&self) -> &'static str {
        "RD"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["RMDIR"]
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningRd {
            args: args.to_vec(),
        })
    }
}

struct RunningRd {
    args: Vec<u8>,
}

impl RunningCommand for RunningRd {
    fn step(
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

        match remove_directory(state, io, disk, args) {
            Ok(()) => StepResult::Done(0),
            Err(msg) => {
                io.print(msg);
                StepResult::Done(1)
            }
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Removes a directory.");
    io.println(b"");
    io.println(b"RD path");
    io.println(b"RMDIR path");
    io.println(b"");
    io.println(b"  path  Specifies the directory to remove. The directory must");
    io.println(b"        be empty before it can be removed.");
}

fn remove_directory(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    path: &[u8],
) -> Result<(), &'static [u8]> {
    let (drive_index, parent_cluster, fcb_name) = state
        .resolve_file_path(path, io.memory, disk)
        .map_err(|_| &b"Invalid path\r\n"[..])?;

    if drive_index == 25 {
        return Err(b"Access denied\r\n");
    }

    let vol = state.fat_volumes[drive_index as usize]
        .as_mut()
        .ok_or(&b"Invalid drive\r\n"[..])?;

    // Find the directory entry
    let entry = fat_dir::find_entry(vol, parent_cluster, &fcb_name, disk)
        .map_err(|_| &b"Invalid path\r\n"[..])?
        .ok_or(&b"Invalid path\r\n"[..])?;

    if entry.attribute & fat_dir::ATTR_DIRECTORY == 0 {
        return Err(b"Invalid path\r\n");
    }

    // Check if directory is empty (only "." and ".." allowed)
    if !is_directory_empty(vol, entry.start_cluster, disk).map_err(|_| &b"Invalid path\r\n"[..])? {
        return Err(b"Directory not empty\r\n");
    }

    // Delete the directory entry and free cluster chain
    fat_dir::delete_entry(vol, &entry, disk).map_err(|_| &b"Access denied\r\n"[..])?;
    if entry.start_cluster >= 2 {
        vol.free_chain(entry.start_cluster);
    }
    vol.flush_fat(disk).map_err(|_| &b"Disk error\r\n"[..])?;

    Ok(())
}

/// Returns true if a directory contains only "." and ".." entries.
fn is_directory_empty(
    vol: &FatVolume,
    dir_cluster: u16,
    disk: &mut dyn DiskIo,
) -> Result<bool, u16> {
    let all_pattern = [b'?'; 11];
    let mut start_index = 0u16;

    loop {
        let result = fat_dir::find_matching(
            vol,
            dir_cluster,
            &all_pattern,
            fat_dir::ATTR_HIDDEN | fat_dir::ATTR_SYSTEM | fat_dir::ATTR_DIRECTORY,
            start_index,
            disk,
        )?;

        match result {
            Some((entry, next_index)) => {
                // Skip "." and ".."
                if entry.name == *b".          " || entry.name == *b"..         " {
                    start_index = next_index;
                    continue;
                }
                // Found a non-dot entry - directory is not empty
                return Ok(false);
            }
            None => return Ok(true),
        }
    }
}
