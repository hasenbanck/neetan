//! MD / MKDIR command.

use crate::{
    DiskIo, DriveIo, IoAccess, MemoryAccess, OsState,
    commands::{Command, RunningCommand, StepResult, is_help_request},
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
        disk: &mut dyn DriveIo,
    ) -> StepResult {
        let args = self.args.trim_ascii();
        if is_help_request(&self.args) || args.is_empty() {
            print_help(io);
            return StepResult::Done(0);
        }

        match create_directory(state, io.memory, disk, args) {
            Ok(()) => StepResult::Done(0),
            Err(error) => {
                io.print(error_message(error));
                StepResult::Done(1)
            }
        }
    }
}

fn print_help(io: &mut IoAccess) {
    io.println(b"Creates a directory.");
    io.println(b"");
    io.println(b"MD path");
    io.println(b"MKDIR path");
    io.println(b"");
    io.println(b"  path  Specifies the directory to create.");
}

pub(crate) fn create_directory(
    state: &mut OsState,
    memory: &dyn MemoryAccess,
    disk: &mut dyn DiskIo,
    path: &[u8],
) -> Result<(), u16> {
    let (drive_index, parent_cluster, fcb_name) = state.resolve_file_path(path, memory, disk)?;

    if drive_index == 25 {
        return Err(0x0005);
    }

    let (time, date) = state.dos_timestamp_now();

    let vol = state.fat_volumes[drive_index as usize]
        .as_mut()
        .ok_or(0x000Fu16)?;

    if fat_dir::find_entry(vol, parent_cluster, &fcb_name, disk)
        .map_err(|_| 0x001Fu16)?
        .is_some()
    {
        return Err(0x0005);
    }

    let new_cluster = vol.allocate_cluster(0).ok_or(0x0005u16)?;

    let cluster_size = vol.sectors_per_cluster() as usize * vol.bytes_per_sector() as usize;
    let zeros = vec![0u8; cluster_size];
    vol.write_cluster(new_cluster, &zeros, disk).map_err(|_| {
        vol.free_chain(new_cluster);
        0x001Fu16
    })?;

    let dot_entry = fat_dir::DirEntry {
        name: *b".          ",
        attribute: fat_dir::ATTR_DIRECTORY,
        time,
        date,
        start_cluster: new_cluster,
        file_size: 0,
        dir_sector: 0,
        dir_offset: 0,
    };
    if let Err(error) = fat_dir::create_entry(vol, new_cluster, &dot_entry, disk) {
        vol.free_chain(new_cluster);
        let _ = vol.flush_fat(disk);
        return Err(error);
    }

    let dotdot_entry = fat_dir::DirEntry {
        name: *b"..         ",
        attribute: fat_dir::ATTR_DIRECTORY,
        time,
        date,
        start_cluster: parent_cluster,
        file_size: 0,
        dir_sector: 0,
        dir_offset: 0,
    };
    if let Err(error) = fat_dir::create_entry(vol, new_cluster, &dotdot_entry, disk) {
        vol.free_chain(new_cluster);
        let _ = vol.flush_fat(disk);
        return Err(error);
    }

    let dir_entry = fat_dir::DirEntry {
        name: fcb_name,
        attribute: fat_dir::ATTR_DIRECTORY,
        time,
        date,
        start_cluster: new_cluster,
        file_size: 0,
        dir_sector: 0,
        dir_offset: 0,
    };
    if let Err(error) = fat_dir::create_entry(vol, parent_cluster, &dir_entry, disk) {
        vol.free_chain(new_cluster);
        let _ = vol.flush_fat(disk);
        return Err(error);
    }

    vol.flush_fat(disk).map_err(|_| 0x001Fu16)?;

    Ok(())
}

fn error_message(error: u16) -> &'static [u8] {
    match error {
        0x0005 => b"Access denied\r\n",
        0x000Fu16 => b"Invalid drive\r\n",
        _ => b"Unable to create directory\r\n",
    }
}
