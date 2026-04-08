//! REN / RENAME command.

use crate::{
    DiskIo, IoAccess, OsState,
    commands::{Command, RunningCommand, StepResult},
    filesystem::fat_dir,
};

pub(crate) struct Ren;

impl Command for Ren {
    fn name(&self) -> &'static str {
        "REN"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["RENAME"]
    }

    fn start(&self, args: &[u8]) -> Box<dyn RunningCommand> {
        Box::new(RunningRen {
            args: args.to_vec(),
        })
    }
}

struct RunningRen {
    args: Vec<u8>,
}

impl RunningCommand for RunningRen {
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

        // Split into source and dest
        let (source, dest) = match split_two_args(args) {
            Some(pair) => pair,
            None => {
                io.print_msg(b"Required parameter missing\r\n");
                return StepResult::Done(1);
            }
        };

        match rename_files(state, io, disk, source, dest) {
            Ok(()) => StepResult::Done(0),
            Err(msg) => {
                io.print_msg(msg);
                StepResult::Done(1)
            }
        }
    }
}

fn split_two_args(args: &[u8]) -> Option<(&[u8], &[u8])> {
    let args = args.trim_ascii();
    let pos = args.iter().position(|&b| b == b' ' || b == b'\t')?;
    let first = &args[..pos];
    let rest = args[pos + 1..].trim_ascii();
    if rest.is_empty() {
        return None;
    }
    // Second arg may also have trailing spaces
    let end = rest
        .iter()
        .position(|&b| b == b' ' || b == b'\t')
        .unwrap_or(rest.len());
    Some((first, &rest[..end]))
}

fn rename_files(
    state: &mut OsState,
    io: &mut IoAccess,
    disk: &mut dyn DiskIo,
    source: &[u8],
    dest: &[u8],
) -> Result<(), &'static [u8]> {
    let (drive_index, dir_cluster, src_fcb_pattern) = state
        .resolve_file_path(source, io.memory, disk)
        .map_err(|_| &b"File not found\r\n"[..])?;

    if drive_index == 25 {
        return Err(b"Access denied\r\n");
    }

    // Dest is just a filename pattern (no path allowed in REN dest)
    let dest_fcb_template = fat_dir::name_to_fcb(dest);

    let vol = state.fat_volumes[drive_index as usize]
        .as_mut()
        .ok_or(&b"Invalid drive\r\n"[..])?;

    let mut renamed_any = false;
    let mut start_index = 0u16;

    loop {
        let result =
            fat_dir::find_matching(vol, dir_cluster, &src_fcb_pattern, 0, start_index, disk)
                .map_err(|_| &b"File not found\r\n"[..])?;

        match result {
            Some((mut entry, next_index)) => {
                // Build new name by merging source name with dest template
                let new_name = merge_wildcard_name(&entry.name, &dest_fcb_template);

                // Skip if name unchanged
                if new_name != entry.name {
                    // Check if the new name already exists
                    if fat_dir::find_entry(vol, dir_cluster, &new_name, disk)
                        .map_err(|_| &b"Duplicate file name or file not found\r\n"[..])?
                        .is_some()
                    {
                        io.print_msg(b"Duplicate file name or file not found\r\n");
                        start_index = next_index;
                        continue;
                    }

                    entry.name = new_name;
                    fat_dir::update_entry(vol, &entry, disk)
                        .map_err(|_| &b"Access denied\r\n"[..])?;
                }
                renamed_any = true;
                start_index = next_index;
            }
            None => break,
        }
    }

    if !renamed_any {
        return Err(b"File not found\r\n");
    }

    Ok(())
}

/// Merges a source FCB name with a destination template.
/// For each position: if the template has '?', use the source character;
/// otherwise use the template character.
fn merge_wildcard_name(source: &[u8; 11], template: &[u8; 11]) -> [u8; 11] {
    let mut result = [0u8; 11];
    for i in 0..11 {
        result[i] = if template[i] == b'?' {
            source[i]
        } else {
            template[i]
        };
    }
    result
}
