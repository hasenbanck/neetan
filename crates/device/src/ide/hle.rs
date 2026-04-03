//! IDE High-Level Emulation (HLE) - controller-specific functions.
//!
//! Shared HLE functions (read, write, init, format, etc.) live in
//! `crate::disk_hle`. This module contains IDE-specific functions:
//! sense (returns 0x0F for IDE) and motor control extensions (D0h/E0h/F0h).

use crate::disk::HddImage;

/// Executes a BIOS sense operation: returns the media type.
/// For SASI-compatible images (256-byte sectors with standard SASI geometry),
/// returns the SASI media type code. Otherwise returns 0x0F (IDE).
pub(super) fn execute_sense(drive_idx: usize, drives: &[Option<HddImage>; 2]) -> u8 {
    let Some(drive) = &drives[drive_idx] else {
        return 0x60;
    };
    if let Some(sense_code) = drive.geometry.sasi_new_sense_type() {
        return sense_code;
    }
    0x0F
}

/// Executes an IDE-specific Check Power Mode (function D0h).
/// Returns 0x00 on success, 0x40 if drive absent.
pub(super) fn execute_check_power_mode(drive_idx: usize, drives: &[Option<HddImage>; 2]) -> u8 {
    if drives[drive_idx].is_some() {
        0x00
    } else {
        0x40
    }
}

/// Executes an IDE-specific Motor ON (function E0h).
/// Returns 0x00 on success, 0x40 if drive absent.
/// No-op in emulation since virtual drives are always ready.
pub(super) fn execute_motor_on(drive_idx: usize, drives: &[Option<HddImage>; 2]) -> u8 {
    if drives[drive_idx].is_some() {
        0x00
    } else {
        0x40
    }
}

/// Executes an IDE-specific Motor OFF (function F0h).
/// Returns 0x00 on success, 0x40 if drive absent.
/// No-op in emulation since virtual drives are always ready.
pub(super) fn execute_motor_off(drive_idx: usize, drives: &[Option<HddImage>; 2]) -> u8 {
    if drives[drive_idx].is_some() {
        0x00
    } else {
        0x40
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{HddFormat, HddGeometry};

    fn make_test_drive() -> HddImage {
        let geometry = HddGeometry {
            cylinders: 20,
            heads: 4,
            sectors_per_track: 17,
            sector_size: 512,
        };
        let data = vec![0u8; geometry.total_bytes() as usize];
        HddImage::from_raw(geometry, HddFormat::Hdi, data)
    }

    fn make_sasi_compat_drive() -> HddImage {
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
    fn sense_returns_ide_type() {
        let drives: [Option<HddImage>; 2] = [Some(make_test_drive()), None];
        assert_eq!(execute_sense(0, &drives), 0x0F);
    }

    #[test]
    fn sense_returns_sasi_type_for_sasi_compat_image() {
        let drives: [Option<HddImage>; 2] = [Some(make_sasi_compat_drive()), None];
        assert_eq!(execute_sense(0, &drives), 0x00);
    }

    #[test]
    fn check_power_mode_with_drive() {
        let drives: [Option<HddImage>; 2] = [Some(make_test_drive()), None];
        assert_eq!(execute_check_power_mode(0, &drives), 0x00);
    }

    #[test]
    fn check_power_mode_without_drive() {
        let drives: [Option<HddImage>; 2] = [None, None];
        assert_eq!(execute_check_power_mode(0, &drives), 0x40);
    }

    #[test]
    fn motor_on_with_drive() {
        let drives: [Option<HddImage>; 2] = [Some(make_test_drive()), None];
        assert_eq!(execute_motor_on(0, &drives), 0x00);
    }

    #[test]
    fn motor_on_without_drive() {
        let drives: [Option<HddImage>; 2] = [None, None];
        assert_eq!(execute_motor_on(0, &drives), 0x40);
    }

    #[test]
    fn motor_off_with_drive() {
        let drives: [Option<HddImage>; 2] = [Some(make_test_drive()), None];
        assert_eq!(execute_motor_off(0, &drives), 0x00);
    }

    #[test]
    fn motor_off_without_drive() {
        let drives: [Option<HddImage>; 2] = [None, None];
        assert_eq!(execute_motor_off(0, &drives), 0x40);
    }
}
