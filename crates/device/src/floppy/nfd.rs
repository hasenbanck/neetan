//! NFD (T98FDDIMAGE) floppy disk image format parser.
//!
//! NFD is a per-sector metadata format from the T98-Next emulator that stores
//! C/H/R/N, MFM flag, FDC status, and PDA disk type alongside raw sector data.
//! Two revisions exist:
//!
//! - **R0**: Fixed 68,112-byte header with a flat 163×26 sector map.
//! - **R1**: Compact header with per-track pointers and variable sector counts.

use std::fmt;

use common::warn;

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

/// Which NFD revision a parsed image came from. Used to select the
/// matching serializer when re-emitting the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfdRevision {
    /// R0 revision (fixed 163 x 26 sector map).
    R0,
    /// R1 revision (per-track headers and offset table).
    R1,
}

/// Parses an NFD disk image (R0 or R1) from raw bytes, returning the
/// parsed disk and which revision the magic indicated.
pub fn from_bytes(data: &[u8]) -> Result<(D88Disk, NfdRevision), NfdError> {
    if data.len() < COMMON_HEADER_SIZE {
        return Err(NfdError::TooSmall);
    }

    if &data[..15] == NFD_R0_MAGIC {
        parse_r0(data).map(|disk| (disk, NfdRevision::R0))
    } else if &data[..15] == NFD_R1_MAGIC {
        parse_r1(data).map(|disk| (disk, NfdRevision::R1))
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
            let sector_data_offset = data_offset;
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
                source_offset: Some(sector_data_offset as u64),
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
            let sector_data_offset = data_offset;
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
                source_offset: Some(sector_data_offset as u64),
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

/// Maps a `D88MediaType` back to the canonical NFD PDA byte. Inverse of
/// `media_type_from_pda`. The lossy direction `0x30 -> Disk2HD -> 0x90`
/// is intentional; both PDAs decode to 2HD on the parse side.
fn pda_from_media_type(media_type: D88MediaType) -> u8 {
    match media_type {
        D88MediaType::Disk2D | D88MediaType::Disk2DD => 0x10,
        D88MediaType::Disk2HD => 0x90,
    }
}

/// R0 header size: common header + flat sector map.
const R0_HEADER_SIZE: usize =
    COMMON_HEADER_SIZE + R0_TRACK_MAX * SECTORS_PER_TRACK * SECTOR_ENTRY_SIZE;

/// R1 track-offset table size (164 tracks x 4 bytes each).
const R1_TRACK_TABLE_SIZE: usize = R1_TRACK_MAX * 4;

/// R1 per-track header size (u16 sector_count + u16 diag_count + 12 reserved bytes).
const R1_TRACK_HEADER_SIZE: usize = 16;

/// Serializes a `D88Disk` into NFD R0 bytes.
///
/// Title/comment bytes (offsets 0x10..0x110) are emitted as zeros; NFD
/// header titles are not preserved across re-emit. Tracks with more than
/// 26 sectors are truncated (R0 cannot represent them) and a warning is
/// logged.
pub fn to_bytes_r0(disk: &D88Disk) -> Vec<u8> {
    let media_pda = pda_from_media_type(disk.media_type);
    let mut warned_truncate = false;

    // Compute total file size: header + sum of sector data sizes.
    let mut data_size: usize = 0;
    for track in 0..R0_TRACK_MAX {
        let count = disk.sector_count(track);
        let emit = count.min(SECTORS_PER_TRACK);
        for slot in 0..emit {
            let Some(sector) = disk.sector_at_index(track, slot) else {
                continue;
            };
            data_size += sector.data.len();
        }
    }

    let mut out = vec![0u8; R0_HEADER_SIZE + data_size];

    // Common header.
    out[..15].copy_from_slice(NFD_R0_MAGIC);
    out[0x110..0x114].copy_from_slice(&(R0_HEADER_SIZE as u32).to_le_bytes());
    out[0x114] = if disk.write_protected { 0x10 } else { 0x00 };

    // Sector map and sector data.
    let mut data_offset = R0_HEADER_SIZE;
    for track in 0..R0_TRACK_MAX {
        let count = disk.sector_count(track);
        if count > SECTORS_PER_TRACK && !warned_truncate {
            warn!(
                "NFD R0 serializer: track {track} has {count} sectors; \
                 R0 supports at most {SECTORS_PER_TRACK} per track, truncating."
            );
            warned_truncate = true;
        }
        let emit = count.min(SECTORS_PER_TRACK);

        for slot in 0..SECTORS_PER_TRACK {
            let entry_offset =
                COMMON_HEADER_SIZE + (track * SECTORS_PER_TRACK + slot) * SECTOR_ENTRY_SIZE;

            if slot < emit {
                let Some(sector) = disk.sector_at_index(track, slot) else {
                    out[entry_offset] = 0xFF;
                    continue;
                };
                out[entry_offset] = sector.cylinder;
                out[entry_offset + 1] = sector.head;
                out[entry_offset + 2] = sector.record;
                out[entry_offset + 3] = sector.size_code;
                // fl_mfm: 0 means MFM (D88 has 0x40 set for MFM).
                out[entry_offset + 4] = if sector.mfm_flag & 0x40 != 0 {
                    0x00
                } else {
                    0xFF
                };
                out[entry_offset + 5] = sector.deleted;
                out[entry_offset + 6] = sector.status;
                out[entry_offset + 10] = media_pda;

                let sector_data_size = sector.data.len();
                out[data_offset..data_offset + sector_data_size].copy_from_slice(&sector.data);
                data_offset += sector_data_size;
            } else {
                out[entry_offset] = 0xFF;
            }
        }
    }

    out
}

/// Serializes a `D88Disk` into NFD R1 bytes.
///
/// Title/comment bytes are emitted as zeros. Per-sector retry copies and
/// diagnostic entries are dropped (count and retry fields are emitted as
/// zero); the parser already discards them at load time, so this matches
/// the lossy in-memory representation.
pub fn to_bytes_r1(disk: &D88Disk) -> Vec<u8> {
    let media_pda = pda_from_media_type(disk.media_type);

    // Determine which tracks are non-empty and reserve their per-track
    // metadata blocks. Layout: common header + track-offset table, then
    // for each non-empty track a (header + sector entries) block, then
    // sector data.
    let mut track_metadata_size: [usize; R1_TRACK_MAX] = [0; R1_TRACK_MAX];
    let mut header_section_size = COMMON_HEADER_SIZE + R1_TRACK_TABLE_SIZE;

    for (track, slot) in track_metadata_size.iter_mut().enumerate() {
        let count = disk.sector_count(track);
        if count == 0 {
            continue;
        }
        let block_size = R1_TRACK_HEADER_SIZE + count * SECTOR_ENTRY_SIZE;
        *slot = block_size;
        header_section_size += block_size;
    }

    // Compute sector-data total size.
    let mut data_size: usize = 0;
    for track in 0..R1_TRACK_MAX {
        for slot in 0..disk.sector_count(track) {
            if let Some(sector) = disk.sector_at_index(track, slot) {
                data_size += sector.data.len();
            }
        }
    }

    let mut out = vec![0u8; header_section_size + data_size];

    // Common header.
    out[..15].copy_from_slice(NFD_R1_MAGIC);
    out[0x110..0x114].copy_from_slice(&(header_section_size as u32).to_le_bytes());
    out[0x114] = if disk.write_protected { 0x10 } else { 0x00 };

    // Compute track absolute offsets and emit per-track metadata blocks.
    let mut metadata_offset = COMMON_HEADER_SIZE + R1_TRACK_TABLE_SIZE;
    let mut data_offset = header_section_size;

    for (track, &block_size) in track_metadata_size.iter().enumerate() {
        let count = disk.sector_count(track);
        let table_entry = COMMON_HEADER_SIZE + track * 4;
        if count == 0 {
            // Track-offset entry stays zero.
            continue;
        }

        out[table_entry..table_entry + 4].copy_from_slice(&(metadata_offset as u32).to_le_bytes());

        // Per-track header: u16 sector_count, u16 diag_count = 0, 12 reserved.
        out[metadata_offset..metadata_offset + 2].copy_from_slice(&(count as u16).to_le_bytes());
        out[metadata_offset + 2..metadata_offset + 4].copy_from_slice(&0u16.to_le_bytes());
        let mut entry_offset = metadata_offset + R1_TRACK_HEADER_SIZE;

        for slot in 0..count {
            let Some(sector) = disk.sector_at_index(track, slot) else {
                entry_offset += SECTOR_ENTRY_SIZE;
                continue;
            };
            out[entry_offset] = sector.cylinder;
            out[entry_offset + 1] = sector.head;
            out[entry_offset + 2] = sector.record;
            out[entry_offset + 3] = sector.size_code;
            out[entry_offset + 4] = if sector.mfm_flag & 0x40 != 0 {
                0x00
            } else {
                0xFF
            };
            out[entry_offset + 5] = sector.deleted;
            out[entry_offset + 6] = sector.status;
            // bytes 7-9: zero
            // byte 10: by_retry (always 0 - retries are not preserved)
            out[entry_offset + 11] = media_pda;
            // bytes 12-15: zero
            entry_offset += SECTOR_ENTRY_SIZE;

            // Append this sector's data after the metadata section.
            let sector_data_size = sector.data.len();
            out[data_offset..data_offset + sector_data_size].copy_from_slice(&sector.data);
            data_offset += sector_data_size;
        }

        metadata_offset += block_size;
    }

    out
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

    /// Builds a minimal NFD R0 image with a single 256-byte sector at
    /// track 0, slot 0 (C=0 H=0 R=1 N=1) carrying a fill pattern.
    fn build_minimal_r0(fill: u8) -> Vec<u8> {
        let header_size = R0_HEADER_SIZE;
        let mut out = vec![0u8; header_size + 256];

        out[..15].copy_from_slice(NFD_R0_MAGIC);
        out[0x110..0x114].copy_from_slice(&(header_size as u32).to_le_bytes());

        // Track 0, slot 0 entry.
        let entry = COMMON_HEADER_SIZE;
        out[entry] = 0; // C
        out[entry + 1] = 0; // H
        out[entry + 2] = 1; // R
        out[entry + 3] = 1; // N (256 bytes)
        // fl_mfm = 0 means MFM is on; matches D88 mfm_flag = 0x40.
        out[entry + 4] = 0;
        out[entry + 10] = 0x90; // PDA = 2HD

        // Fill all other slots with 0xFF (empty).
        for track in 0..R0_TRACK_MAX {
            for slot in 0..SECTORS_PER_TRACK {
                if track == 0 && slot == 0 {
                    continue;
                }
                let off =
                    COMMON_HEADER_SIZE + (track * SECTORS_PER_TRACK + slot) * SECTOR_ENTRY_SIZE;
                out[off] = 0xFF;
            }
        }

        // Sector data after header.
        for byte in &mut out[header_size..] {
            *byte = fill;
        }

        out
    }

    #[test]
    fn r0_roundtrip_unchanged() {
        let original = build_minimal_r0(0xAB);
        let (disk, rev) = from_bytes(&original).unwrap();
        assert_eq!(rev, NfdRevision::R0);
        let serialized = to_bytes_r0(&disk);
        assert_eq!(serialized.len(), original.len());
        // Header (sector map + magic) and data should match for this minimal case.
        assert_eq!(serialized, original);
    }

    #[test]
    fn r0_after_sector_mutation() {
        let original = build_minimal_r0(0xAB);
        let (mut disk, _) = from_bytes(&original).unwrap();

        let sector = disk.find_sector_on_track_index_mut(0, 0, 0, 1, 1).unwrap();
        sector.data.fill(0x77);

        let serialized = to_bytes_r0(&disk);
        let (reparsed, _) = from_bytes(&serialized).unwrap();
        let s = reparsed.find_sector(0, 0, 1, 1).unwrap();
        assert!(s.data.iter().all(|&b| b == 0x77));
    }

    /// Builds a minimal NFD R1 image with two sectors on track 0
    /// (C=0 H=0 R=1, R=2; both 256 bytes; no diag/retry).
    fn build_minimal_r1(fill1: u8, fill2: u8) -> Vec<u8> {
        let track_metadata_size = R1_TRACK_HEADER_SIZE + 2 * SECTOR_ENTRY_SIZE;
        let header_section_size = COMMON_HEADER_SIZE + R1_TRACK_TABLE_SIZE + track_metadata_size;
        let total = header_section_size + 2 * 256;
        let mut out = vec![0u8; total];

        out[..15].copy_from_slice(NFD_R1_MAGIC);
        out[0x110..0x114].copy_from_slice(&(header_section_size as u32).to_le_bytes());

        // Track-offset table: track 0 starts after the table.
        let track_meta_offset = COMMON_HEADER_SIZE + R1_TRACK_TABLE_SIZE;
        out[COMMON_HEADER_SIZE..COMMON_HEADER_SIZE + 4]
            .copy_from_slice(&(track_meta_offset as u32).to_le_bytes());

        // Track header: 2 sectors, 0 diag.
        out[track_meta_offset..track_meta_offset + 2].copy_from_slice(&2u16.to_le_bytes());
        // wDiag = 0 (already zero).

        // Sector entries.
        let entry0 = track_meta_offset + R1_TRACK_HEADER_SIZE;
        out[entry0] = 0;
        out[entry0 + 1] = 0;
        out[entry0 + 2] = 1; // R
        out[entry0 + 3] = 1; // N
        out[entry0 + 11] = 0x90;
        let entry1 = entry0 + SECTOR_ENTRY_SIZE;
        out[entry1] = 0;
        out[entry1 + 1] = 0;
        out[entry1 + 2] = 2; // R
        out[entry1 + 3] = 1;
        out[entry1 + 11] = 0x90;

        // Sector data.
        let data1_offset = header_section_size;
        for byte in &mut out[data1_offset..data1_offset + 256] {
            *byte = fill1;
        }
        let data2_offset = data1_offset + 256;
        for byte in &mut out[data2_offset..data2_offset + 256] {
            *byte = fill2;
        }

        out
    }

    #[test]
    fn r1_roundtrip_unchanged() {
        let original = build_minimal_r1(0xAA, 0xBB);
        let (disk, rev) = from_bytes(&original).unwrap();
        assert_eq!(rev, NfdRevision::R1);
        let serialized = to_bytes_r1(&disk);
        assert_eq!(serialized, original);
    }

    #[test]
    fn r1_after_sector_mutation() {
        let original = build_minimal_r1(0xAA, 0xBB);
        let (mut disk, _) = from_bytes(&original).unwrap();

        let sector = disk.find_sector_on_track_index_mut(0, 0, 0, 2, 1).unwrap();
        sector.data.fill(0x55);

        let serialized = to_bytes_r1(&disk);
        let (reparsed, _) = from_bytes(&serialized).unwrap();

        let s1 = reparsed.find_sector(0, 0, 1, 1).unwrap();
        assert!(s1.data.iter().all(|&b| b == 0xAA));
        let s2 = reparsed.find_sector(0, 0, 2, 1).unwrap();
        assert!(s2.data.iter().all(|&b| b == 0x55));
    }
}
