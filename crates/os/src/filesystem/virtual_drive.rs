//! Z: drive implementation.
//!
//! Read-only virtual filesystem listing built-in commands as .COM files.
//! The actual command implementations are registered in step 10.10.
//! The EXEC shortcut (bypassing .COM loading) is implemented in step 10.9.

use crate::filesystem::fat_dir;

/// A virtual directory entry representing a built-in command.
pub(crate) struct VirtualEntry {
    pub name: [u8; 11],
    pub display_name: &'static [u8],
    pub attribute: u8,
    pub file_size: u32,
    pub time: u16,
    pub date: u16,
}

/// The virtual Z: drive filesystem.
pub(crate) struct VirtualDrive {
    entries: Vec<VirtualEntry>,
}

impl VirtualDrive {
    pub fn new() -> Self {
        // Date: 1995-01-01, Time: 00:00
        // DOS date: ((year-1980)<<9) | (month<<5) | day = (15<<9)|(1<<5)|1 = 0x1E21
        // DOS time: 0x0000
        let date = 0x1E21u16;
        let time = 0x0000u16;
        let attr = fat_dir::ATTR_READ_ONLY | fat_dir::ATTR_ARCHIVE;

        let commands: &[&[u8]] = &[
            b"DIR",
            b"COPY",
            b"DEL",
            b"MD",
            b"RD",
            b"TYPE",
            b"MORE",
            b"FORMAT",
            b"DISKCOPY",
            b"XCOPY",
            b"DATE",
            b"TIME",
        ];

        let entries = commands
            .iter()
            .map(|cmd| {
                let mut full_name = cmd.to_vec();
                full_name.extend_from_slice(b".COM");
                let name = fat_dir::name_to_fcb(&full_name);
                VirtualEntry {
                    name,
                    display_name: cmd,
                    attribute: attr,
                    file_size: 1,
                    time,
                    date,
                }
            })
            .collect();

        Self { entries }
    }

    /// Finds an entry by exact FCB name.
    pub fn find_entry(&self, name: &[u8; 11]) -> Option<&VirtualEntry> {
        self.entries.iter().find(|e| e.name == *name)
    }

    /// Finds the next entry matching the pattern and attribute mask.
    pub fn find_matching(
        &self,
        pattern: &[u8; 11],
        attr_mask: u8,
        start_index: u16,
    ) -> Option<(&VirtualEntry, u16)> {
        let _ = attr_mask;
        for (i, entry) in self.entries.iter().enumerate() {
            if (i as u16) < start_index {
                continue;
            }
            if fat_dir::matches_pattern(&entry.name, pattern) {
                return Some((entry, i as u16 + 1));
            }
        }
        None
    }

    /// Returns the number of registered commands.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}
