//! NFD (T98FDDIMAGE) floppy disk image format parser.
//!
//! NFD is a per-sector metadata format from the T98-Next emulator that stores
//! C/H/R/N, MFM flag, FDC status, and PDA disk type alongside raw sector data.
//! Two revisions exist:
//!
//! - **R0**: Fixed 68,112-byte header with a flat 163×26 sector map.
//! - **R1**: Compact header with per-track pointers and variable sector counts.

use std::fmt;

use super::d88::{D88Disk, D88MediaType, D88Sector};

const NFD_R0_MAGIC: &[u8; 15] = b"T98FDDIMAGE.R0\0";
const NFD_R1_MAGIC: &[u8; 15] = b"T98FDDIMAGE.R1\0";

const COMMON_HEADER_SIZE: usize = 0x120;
const R0_TRACK_MAX: usize = 163;
const R1_TRACK_MAX: usize = 164;
const SECTORS_PER_TRACK: usize = 26;
const SECTOR_ENTRY_SIZE: usize = 16;
const DIAG_ENTRY_SIZE: usize = 16;

/// Error type for NFD parsing.
#[derive(Debug, Clone)]
pub enum NfdError {
    /// Image data too small for common header.
    TooSmall,
    /// Not a valid NFD R0 or R1 image.
    InvalidMagic,
    /// Header size from dwHeadSize exceeds file length.
    HeaderTruncated {
        /// Header size declared in the file.
        header_size: u32,
        /// Actual byte count of the image data.
        actual: usize,
    },
    /// Sector data runs past end of file.
    DataTruncated {
        /// Track index where truncation was detected.
        track: usize,
        /// Byte offset within the image.
        offset: usize,
    },
    /// R1 track pointer out of bounds.
    InvalidTrackOffset {
        /// Track index with the invalid pointer.
        track: usize,
        /// The invalid offset value.
        offset: u32,
    },
    /// Unrecognized PDA byte value.
    UnknownPda(u8),
}

impl fmt::Display for NfdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NfdError::TooSmall => write!(f, "NFD image too small for header"),
            NfdError::InvalidMagic => write!(f, "not a valid NFD R0 or R1 image"),
            NfdError::HeaderTruncated {
                header_size,
                actual,
            } => {
                write!(
                    f,
                    "NFD header truncated: dwHeadSize={header_size}, file is {actual} bytes"
                )
            }
            NfdError::DataTruncated { track, offset } => {
                write!(
                    f,
                    "NFD sector data truncated at track {track}, offset {offset}"
                )
            }
            NfdError::InvalidTrackOffset { track, offset } => {
                write!(f, "NFD R1 track {track} offset {offset:#X} out of bounds")
            }
            NfdError::UnknownPda(pda) => write!(f, "unknown NFD PDA byte: {pda:#04X}"),
        }
    }
}

fn media_type_from_pda(pda: u8, size_code: u8) -> Result<D88MediaType, NfdError> {
    match pda {
        0x10 => Ok(D88MediaType::Disk2DD),
        0x30 | 0x90 => Ok(D88MediaType::Disk2HD),
        0x00 => {
            if size_code <= 1 {
                Ok(D88MediaType::Disk2HD)
            } else {
                Ok(D88MediaType::Disk2DD)
            }
        }
        _ => Err(NfdError::UnknownPda(pda)),
    }
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Parses an NFD disk image (R0 or R1) from raw bytes.
pub fn from_bytes(data: &[u8]) -> Result<D88Disk, NfdError> {
    if data.len() < COMMON_HEADER_SIZE {
        return Err(NfdError::TooSmall);
    }

    if &data[..15] == NFD_R0_MAGIC {
        parse_r0(data)
    } else if &data[..15] == NFD_R1_MAGIC {
        parse_r1(data)
    } else {
        Err(NfdError::InvalidMagic)
    }
}

fn parse_r0(data: &[u8]) -> Result<D88Disk, NfdError> {
    let head_size = read_u32_le(data, 0x110);
    let write_protected = data[0x114] != 0;

    if (head_size as usize) > data.len() {
        return Err(NfdError::HeaderTruncated {
            header_size: head_size,
            actual: data.len(),
        });
    }

    let sector_map_start = COMMON_HEADER_SIZE;
    let sector_map_size = R0_TRACK_MAX * SECTORS_PER_TRACK * SECTOR_ENTRY_SIZE;

    if data.len() < sector_map_start + sector_map_size {
        return Err(NfdError::HeaderTruncated {
            header_size: head_size,
            actual: data.len(),
        });
    }

    // Count valid sectors per track for sector_count field.
    let mut valid_counts = [0u16; R0_TRACK_MAX];
    for (track_idx, count) in valid_counts.iter_mut().enumerate() {
        for slot in 0..SECTORS_PER_TRACK {
            let entry_offset =
                sector_map_start + (track_idx * SECTORS_PER_TRACK + slot) * SECTOR_ENTRY_SIZE;
            if data[entry_offset] != 0xFF {
                *count += 1;
            }
        }
    }

    // Detect media type from first valid sector.
    let mut media_type = D88MediaType::Disk2HD;
    for track_idx in 0..R0_TRACK_MAX {
        let mut found = false;
        for slot in 0..SECTORS_PER_TRACK {
            let entry_offset =
                sector_map_start + (track_idx * SECTORS_PER_TRACK + slot) * SECTOR_ENTRY_SIZE;
            if data[entry_offset] != 0xFF {
                let size_code = data[entry_offset + 3];
                let pda = data[entry_offset + 10];
                media_type = media_type_from_pda(pda, size_code)?;
                found = true;
                break;
            }
        }
        if found {
            break;
        }
    }

    // Parse sectors and collect data.
    let mut data_offset = head_size as usize;
    let mut track_sectors: Vec<Option<Vec<D88Sector>>> = vec![None; R0_TRACK_MAX];

    for track_idx in 0..R0_TRACK_MAX {
        let mut sectors = Vec::new();

        for slot in 0..SECTORS_PER_TRACK {
            let entry_offset =
                sector_map_start + (track_idx * SECTORS_PER_TRACK + slot) * SECTOR_ENTRY_SIZE;
            let c = data[entry_offset];

            if c == 0xFF {
                continue;
            }

            let h = data[entry_offset + 1];
            let r = data[entry_offset + 2];
            let n = data[entry_offset + 3];
            let fl_mfm = data[entry_offset + 4];
            let fl_ddam = data[entry_offset + 5];
            let by_status = data[entry_offset + 6];

            let sector_size = 128usize << n;

            if data_offset + sector_size > data.len() {
                return Err(NfdError::DataTruncated {
                    track: track_idx,
                    offset: data_offset,
                });
            }

            let sector_data = data[data_offset..data_offset + sector_size].to_vec();
            data_offset += sector_size;

            sectors.push(D88Sector {
                cylinder: c,
                head: h,
                record: r,
                size_code: n,
                sector_count: valid_counts[track_idx],
                mfm_flag: if fl_mfm == 0 { 0x40 } else { 0x00 },
                deleted: fl_ddam,
                status: by_status,
                reserved: [0u8; 5],
                data: sector_data,
            });
        }

        if !sectors.is_empty() {
            track_sectors[track_idx] = Some(sectors);
        }
    }

    Ok(D88Disk::from_tracks(
        String::new(),
        write_protected,
        media_type,
        track_sectors,
    ))
}

fn parse_r1(data: &[u8]) -> Result<D88Disk, NfdError> {
    let head_size = read_u32_le(data, 0x110);
    let write_protected = data[0x114] != 0;

    if (head_size as usize) > data.len() {
        return Err(NfdError::HeaderTruncated {
            header_size: head_size,
            actual: data.len(),
        });
    }

    let track_head_start = COMMON_HEADER_SIZE;
    let track_head_end = track_head_start + R1_TRACK_MAX * 4;

    if data.len() < track_head_end {
        return Err(NfdError::HeaderTruncated {
            header_size: head_size,
            actual: data.len(),
        });
    }

    // Read track offset table.
    let mut track_offsets = [0u32; R1_TRACK_MAX];
    for (i, offset) in track_offsets.iter_mut().enumerate() {
        *offset = read_u32_le(data, track_head_start + i * 4);
    }

    let mut data_offset = head_size as usize;
    let mut media_type = D88MediaType::Disk2HD;
    let mut media_type_detected = false;
    let mut track_sectors: Vec<Option<Vec<D88Sector>>> = vec![None; R1_TRACK_MAX];

    for track_idx in 0..R1_TRACK_MAX {
        let track_offset = track_offsets[track_idx];
        if track_offset == 0 {
            continue;
        }

        let track_meta = track_offset as usize;
        if track_meta + 16 > data.len() {
            return Err(NfdError::InvalidTrackOffset {
                track: track_idx,
                offset: track_offset,
            });
        }

        let sector_count = read_u16_le(data, track_meta) as usize;
        let diag_count = read_u16_le(data, track_meta + 2) as usize;

        let entries_start = track_meta + 16;
        let entries_end = entries_start + sector_count * SECTOR_ENTRY_SIZE;

        if entries_end > data.len() {
            return Err(NfdError::InvalidTrackOffset {
                track: track_idx,
                offset: track_offset,
            });
        }

        let mut sectors = Vec::with_capacity(sector_count);

        for i in 0..sector_count {
            let entry_offset = entries_start + i * SECTOR_ENTRY_SIZE;
            let c = data[entry_offset];
            let h = data[entry_offset + 1];
            let r = data[entry_offset + 2];
            let n = data[entry_offset + 3];
            let fl_mfm = data[entry_offset + 4];
            let fl_ddam = data[entry_offset + 5];
            let by_status = data[entry_offset + 6];
            let by_retry = data[entry_offset + 10];

            let sector_size = 128usize << n;
            let total_size = sector_size * (1 + by_retry as usize);

            if data_offset + total_size > data.len() {
                return Err(NfdError::DataTruncated {
                    track: track_idx,
                    offset: data_offset,
                });
            }

            if !media_type_detected {
                let pda = data[entry_offset + 11];
                media_type = media_type_from_pda(pda, n)?;
                media_type_detected = true;
            }

            let sector_data = data[data_offset..data_offset + sector_size].to_vec();
            data_offset += total_size;

            sectors.push(D88Sector {
                cylinder: c,
                head: h,
                record: r,
                size_code: n,
                sector_count: sector_count as u16,
                mfm_flag: if fl_mfm == 0 { 0x40 } else { 0x00 },
                deleted: fl_ddam,
                status: by_status,
                reserved: [0u8; 5],
                data: sector_data,
            });
        }

        // Skip diagnostic entries and their data.
        let diag_entries_start = entries_end;
        let diag_entries_end = diag_entries_start + diag_count * DIAG_ENTRY_SIZE;

        if diag_entries_end > data.len() {
            return Err(NfdError::InvalidTrackOffset {
                track: track_idx,
                offset: track_offset,
            });
        }

        for i in 0..diag_count {
            let diag_offset = diag_entries_start + i * DIAG_ENTRY_SIZE;
            let by_retry = data[diag_offset + 9];
            let dw_data_len = read_u32_le(data, diag_offset + 10) as usize;
            let total_diag_size = dw_data_len * (1 + by_retry as usize);

            if data_offset + total_diag_size > data.len() {
                return Err(NfdError::DataTruncated {
                    track: track_idx,
                    offset: data_offset,
                });
            }

            data_offset += total_diag_size;
        }

        if !sectors.is_empty() {
            track_sectors[track_idx] = Some(sectors);
        }
    }

    Ok(D88Disk::from_tracks(
        String::new(),
        write_protected,
        media_type,
        track_sectors,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_too_small() {
        assert!(matches!(from_bytes(&[0; 100]), Err(NfdError::TooSmall)));
    }

    #[test]
    fn reject_invalid_magic() {
        let data = vec![0u8; COMMON_HEADER_SIZE];
        assert!(matches!(from_bytes(&data), Err(NfdError::InvalidMagic)));
    }

    #[test]
    fn reject_truncated_header() {
        let mut data = vec![0u8; COMMON_HEADER_SIZE];
        data[..15].copy_from_slice(NFD_R0_MAGIC);
        // Set dwHeadSize larger than file.
        data[0x110..0x114].copy_from_slice(&0x00FF_FFFFu32.to_le_bytes());
        assert!(matches!(
            from_bytes(&data),
            Err(NfdError::HeaderTruncated { .. })
        ));
    }
}
