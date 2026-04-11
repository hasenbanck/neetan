//! Drive trait, DiskIo trait, drive mapping, error types.

use crate::{DriveIo, OsState};

pub mod fat;
pub(crate) mod fat_bpb;
pub(crate) mod fat_dir;
pub(crate) mod fat_file;
pub(crate) mod fat_partition;
pub(crate) mod iso9660;
pub(crate) mod virtual_drive;

#[derive(Debug, Clone)]
pub(crate) enum ReadDirectory {
    Fat(u16),
    Iso(iso9660::IsoDirectory),
}

#[derive(Debug, Clone)]
pub(crate) struct ReadFilePath {
    pub drive_index: u8,
    pub directory: ReadDirectory,
    pub name: [u8; 11],
}

#[derive(Debug, Clone)]
pub(crate) struct ReadDirPath {
    pub drive_index: u8,
    pub directory: ReadDirectory,
}

#[derive(Debug, Clone)]
pub(crate) struct ReadDirEntry {
    pub name: [u8; 11],
    pub attribute: u8,
    pub time: u16,
    pub date: u16,
    pub file_size: u32,
    pub source: ReadDirEntrySource,
}

#[derive(Debug, Clone)]
pub(crate) enum ReadDirEntrySource {
    Fat(fat_dir::DirEntry),
    Iso(iso9660::IsoDirEntry),
}

impl ReadDirEntry {
    pub(crate) fn from_fat(entry: fat_dir::DirEntry) -> Self {
        Self {
            name: entry.name,
            attribute: entry.attribute,
            time: entry.time,
            date: entry.date,
            file_size: entry.file_size,
            source: ReadDirEntrySource::Fat(entry),
        }
    }

    pub(crate) fn from_iso(entry: iso9660::IsoDirEntry) -> Self {
        Self {
            name: entry.name,
            attribute: entry.attribute,
            time: entry.time,
            date: entry.date,
            file_size: entry.file_size,
            source: ReadDirEntrySource::Iso(entry),
        }
    }
}

pub(crate) fn find_read_entry(
    state: &OsState,
    path: &ReadFilePath,
    device: &mut dyn DriveIo,
) -> Result<Option<ReadDirEntry>, u16> {
    match &path.directory {
        ReadDirectory::Fat(dir_cluster) => {
            let volume = state.fat_volumes[path.drive_index as usize]
                .as_ref()
                .ok_or(0x000Fu16)?;
            let entry = fat_dir::find_entry(volume, *dir_cluster, &path.name, device)?;
            Ok(entry.map(ReadDirEntry::from_fat))
        }
        ReadDirectory::Iso(directory) => {
            let volume = iso9660::IsoVolume::mount(device)?;
            let entry = iso9660::find_entry(&volume, directory, &path.name, device)?;
            Ok(entry.map(ReadDirEntry::from_iso))
        }
    }
}

pub(crate) fn find_matching_read_entry(
    state: &OsState,
    drive_index: u8,
    directory: &ReadDirectory,
    pattern: &[u8; 11],
    attr_mask: u8,
    start_index: u16,
    device: &mut dyn DriveIo,
) -> Result<Option<(ReadDirEntry, u16)>, u16> {
    match directory {
        ReadDirectory::Fat(dir_cluster) => {
            let volume = state.fat_volumes[drive_index as usize]
                .as_ref()
                .ok_or(0x000Fu16)?;
            let entry = fat_dir::find_matching(
                volume,
                *dir_cluster,
                pattern,
                attr_mask,
                start_index,
                device,
            )?;
            Ok(entry.map(|(entry, next_index)| (ReadDirEntry::from_fat(entry), next_index)))
        }
        ReadDirectory::Iso(directory) => {
            let volume = iso9660::IsoVolume::mount(device)?;
            let entry = iso9660::find_matching(
                &volume,
                directory,
                pattern,
                attr_mask,
                start_index,
                device,
            )?;
            Ok(entry.map(|(entry, next_index)| (ReadDirEntry::from_iso(entry), next_index)))
        }
    }
}

pub(crate) fn read_entry_all(
    state: &OsState,
    drive_index: u8,
    entry: &ReadDirEntry,
    device: &mut dyn DriveIo,
) -> Result<Vec<u8>, u16> {
    match &entry.source {
        ReadDirEntrySource::Fat(entry) => {
            let volume = state.fat_volumes[drive_index as usize]
                .as_ref()
                .ok_or(0x000Fu16)?;
            fat_file::read_all(volume, entry, device)
        }
        ReadDirEntrySource::Iso(entry) => iso9660::read_all(entry, device),
    }
}

/// Returns true if the byte is a DBCS (Shift-JIS) lead byte.
pub(crate) fn is_dbcs_lead_byte(b: u8) -> bool {
    (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b)
}

/// Splits a DOS path into components, handling SJIS double-byte characters
/// where 0x5C can appear as a trail byte and must not be treated as backslash.
/// Returns (drive_index, components, is_absolute).
/// `drive_index` is None if no drive letter prefix.
pub(crate) fn split_path(path: &[u8]) -> (Option<u8>, Vec<&[u8]>, bool) {
    if path.is_empty() {
        return (None, Vec::new(), false);
    }

    let mut pos = 0;
    let mut drive_index = None;

    // Check for drive letter prefix "X:"
    if path.len() >= 2 && path[1] == b':' {
        let letter = path[0].to_ascii_uppercase();
        if letter.is_ascii_uppercase() {
            drive_index = Some(letter - b'A');
            pos = 2;
        }
    }

    // Check if path is absolute (starts with backslash after optional drive)
    let is_absolute = pos < path.len() && path[pos] == b'\\';

    // Split remaining path on backslash, respecting DBCS
    let mut components = Vec::new();
    let mut comp_start = pos;
    let mut i = pos;

    while i < path.len() {
        if path[i] == 0 {
            break;
        }
        if is_dbcs_lead_byte(path[i]) && i + 1 < path.len() {
            i += 2;
            continue;
        }
        if path[i] == b'\\' {
            if i > comp_start {
                components.push(&path[comp_start..i]);
            }
            i += 1;
            comp_start = i;
            continue;
        }
        i += 1;
    }
    // Last component
    if i > comp_start {
        components.push(&path[comp_start..i]);
    }

    (drive_index, components, is_absolute)
}
