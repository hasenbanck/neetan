//! MD / MKDIR command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
    filesystem::fat_dir,
};

pub(crate) struct Md;

impl Command for Md {
    fn name(&self) -> &'static str {
        "MD"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["MKDIR"]
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningMd {
            args: args.to_vec(),
        })
    }
}

struct RunningMd {
    args: Vec<u8>,
}

impl RunningCommand for RunningMd {
    fn step(
        &mut self,
        state: &mut OsState,
        io: &mut IoAccess,
        disk: &mut dyn DiskIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();
        if args.is_empty() {
            io.print_msg(b"Required parameter missing\r\n");
            return StepResult::Done(1);
        }

        match create_directory(state, io, disk, args) {
            Ok(()) => StepResult::Done(0),
            Err(msg) => {
                io.print_msg(msg);
                StepResult::Done(1)
            }
        }
    }
}

fn create_directory(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    path: &[u8],
) -> Result<(), &'static [u8]> {
    let (drive_index, parent_cluster, fcb_name) = state
        .resolve_file_path(path, io.memory, disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?;

    if drive_index == 25 {
        return Err(b"Access denied\r\n");
    }

    let vol = state.fat_volumes[drive_index as usize]
        .as_mut()
        .ok_or(&b"Invalid drive\r\n"[..])?;

    // Check if entry already exists
    if fat_dir::find_entry(vol, parent_cluster, &fcb_name, disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?
        .is_some()
    {
        return Err(b"Unable to create directory\r\n");
    }

    // Allocate a cluster for the new directory
    let new_cluster = vol
        .allocate_cluster(0)
        .ok_or(&b"Unable to create directory\r\n"[..])?;

    // Zero-fill the new cluster
    let cluster_size = vol.bpb.sectors_per_cluster as usize * vol.bpb.bytes_per_sector as usize;
    let zeros = vec![0u8; cluster_size];
    vol.write_cluster(new_cluster, &zeros, disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?;

    // Write "." entry (points to self)
    let dot_entry = fat_dir::DirEntry {
        name: *b".          ",
        attribute: fat_dir::ATTR_DIRECTORY,
        time: 0x6000,
        date: 0x1E21,
        start_cluster: new_cluster,
        file_size: 0,
        dir_sector: 0,
        dir_offset: 0,
    };
    fat_dir::create_entry(vol, new_cluster, &dot_entry, disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?;

    // Write ".." entry (points to parent, 0 if parent is root)
    let dotdot_entry = fat_dir::DirEntry {
        name: *b"..         ",
        attribute: fat_dir::ATTR_DIRECTORY,
        time: 0x6000,
        date: 0x1E21,
        start_cluster: parent_cluster,
        file_size: 0,
        dir_sector: 0,
        dir_offset: 0,
    };
    fat_dir::create_entry(vol, new_cluster, &dotdot_entry, disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?;

    // Create directory entry in parent
    let dir_entry = fat_dir::DirEntry {
        name: fcb_name,
        attribute: fat_dir::ATTR_DIRECTORY,
        time: 0x6000,
        date: 0x1E21,
        start_cluster: new_cluster,
        file_size: 0,
        dir_sector: 0,
        dir_offset: 0,
    };
    fat_dir::create_entry(vol, parent_cluster, &dir_entry, disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?;

    vol.flush_fat(disk)
        .map_err(|_| &b"Unable to create directory\r\n"[..])?;

    Ok(())
}
