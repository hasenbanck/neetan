//! Drive trait, DiskIo trait, drive mapping, error types.

pub mod fat;
pub(crate) mod fat_bpb;
pub(crate) mod fat_dir;
pub(crate) mod fat_file;
pub(crate) mod fat_partition;
pub(crate) mod virtual_drive;

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
