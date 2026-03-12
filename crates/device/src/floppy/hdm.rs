//! HDM floppy disk image format parser.
//!
//! HDM is a headerless raw sector format for PC-98 2HD floppies.
//! Fixed geometry: 77 cylinders, 2 heads, 8 sectors/track, 1024 bytes/sector.
//! Total size is always exactly 1,261,568 bytes.

use std::fmt;

use super::d88::{D88Disk, D88MediaType, D88Sector};

const HDM_FILE_SIZE: usize = 1_261_568;
const CYLINDERS: u8 = 77;
const HEADS: u8 = 2;
const SECTORS_PER_TRACK: u8 = 8;
const SECTOR_SIZE: usize = 1024;
const SIZE_CODE: u8 = 3; // 128 << 3 = 1024

/// Error type for HDM parsing.
#[derive(Debug, Clone)]
pub enum HdmError {
    /// Image data is not the expected size.
    InvalidSize {
        /// Actual byte count of the image data.
        actual: usize,
    },
}

impl fmt::Display for HdmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HdmError::InvalidSize { actual } => {
                write!(
                    f,
                    "HDM image size is {actual} bytes, expected exactly {HDM_FILE_SIZE}"
                )
            }
        }
    }
}

/// Parses an HDM disk image from raw bytes.
pub fn from_bytes(data: &[u8]) -> Result<D88Disk, HdmError> {
    if data.len() != HDM_FILE_SIZE {
        return Err(HdmError::InvalidSize { actual: data.len() });
    }

    let total_tracks = CYLINDERS as usize * HEADS as usize;
    let mut track_sectors = Vec::with_capacity(total_tracks);
    let mut offset = 0;

    for cylinder in 0..CYLINDERS {
        for head in 0..HEADS {
            let mut sectors = Vec::with_capacity(SECTORS_PER_TRACK as usize);

            for record in 1..=SECTORS_PER_TRACK {
                let sector_data = data[offset..offset + SECTOR_SIZE].to_vec();
                offset += SECTOR_SIZE;

                sectors.push(D88Sector {
                    cylinder,
                    head,
                    record,
                    size_code: SIZE_CODE,
                    sector_count: SECTORS_PER_TRACK as u16,
                    mfm_flag: 0x00,
                    deleted: 0x00,
                    status: 0x00,
                    reserved: [0u8; 5],
                    data: sector_data,
                });
            }

            track_sectors.push(Some(sectors));
        }
    }

    Ok(D88Disk::from_tracks(
        String::new(),
        false,
        D88MediaType::Disk2HD,
        track_sectors,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_wrong_size() {
        assert!(matches!(
            from_bytes(&[0; 100]),
            Err(HdmError::InvalidSize { actual: 100 })
        ));
        assert!(matches!(
            from_bytes(&vec![0; HDM_FILE_SIZE + 1]),
            Err(HdmError::InvalidSize { .. })
        ));
    }

    #[test]
    fn parse_valid_image() {
        let data = vec![0u8; HDM_FILE_SIZE];
        let disk = from_bytes(&data).unwrap();
        assert_eq!(disk.media_type, D88MediaType::Disk2HD);
        assert!(!disk.write_protected);
    }

    #[test]
    fn track_structure() {
        let data = vec![0u8; HDM_FILE_SIZE];
        let disk = from_bytes(&data).unwrap();

        // 77 cylinders × 2 heads = 154 tracks, each with 8 sectors.
        for track in 0..154 {
            assert_eq!(
                disk.sector_count(track),
                8,
                "Track {track} should have 8 sectors"
            );
        }

        // Beyond track 153 should be empty.
        assert_eq!(disk.sector_count(154), 0);
    }

    #[test]
    fn chrn_lookup() {
        let mut data = vec![0u8; HDM_FILE_SIZE];
        // Write a marker in the first byte of C=0 H=0 R=1.
        data[0] = 0xAA;
        let disk = from_bytes(&data).unwrap();

        let s = disk.find_sector(0, 0, 1, SIZE_CODE).unwrap();
        assert_eq!(s.data[0], 0xAA);
        assert_eq!(s.data.len(), SECTOR_SIZE);

        // All 8 sectors on track 0 should be findable.
        for r in 1..=8u8 {
            assert!(disk.find_sector(0, 0, r, SIZE_CODE).is_some());
        }

        // Nonexistent R=9.
        assert!(disk.find_sector(0, 0, 9, SIZE_CODE).is_none());
    }

    #[test]
    fn second_head_data() {
        let mut data = vec![0u8; HDM_FILE_SIZE];
        // C=0 H=1 starts at offset 8 × 1024 = 8192.
        data[8192] = 0xBB;
        let disk = from_bytes(&data).unwrap();

        let s = disk.find_sector(0, 1, 1, SIZE_CODE).unwrap();
        assert_eq!(s.data[0], 0xBB);
    }

    #[test]
    fn sector_wrapping() {
        let data = vec![0u8; HDM_FILE_SIZE];
        let disk = from_bytes(&data).unwrap();

        let s0 = disk.sector_at_index(0, 0).unwrap();
        let s8 = disk.sector_at_index(0, 8).unwrap();
        assert_eq!(s0.record, s8.record);
    }
}
