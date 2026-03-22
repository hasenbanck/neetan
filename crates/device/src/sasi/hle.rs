//! SASI High-Level Emulation (HLE) - controller-specific functions.
//!
//! Shared HLE functions (read, write, init, format, etc.) live in
//! `crate::disk_hle`. This module contains only the SASI-specific
//! sense implementation.

use crate::disk::HddImage;

/// Executes a BIOS sense operation: returns the SASI media type.
pub(super) fn execute_sense(drive_idx: usize, drives: &[Option<HddImage>; 2]) -> u8 {
    let Some(drive) = &drives[drive_idx] else {
        return 0x60;
    };
    drive.geometry.sasi_media_type().unwrap_or(0x0F)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{HddFormat, HddGeometry};

    fn make_test_drive() -> HddImage {
        let geometry = HddGeometry {
            cylinders: 153,
            heads: 4,
            sectors_per_track: 33,
            sector_size: 256,
        };
        let data = vec![0u8; geometry.total_bytes() as usize];
        HddImage::from_raw(geometry, HddFormat::Thd, data)
    }

    #[test]
    fn sense_returns_media_type() {
        let drives: [Option<HddImage>; 2] = [Some(make_test_drive()), None];
        let media_type = execute_sense(0, &drives);
        assert_eq!(media_type, 0); // 5 MB SASI = type 0
    }
}
