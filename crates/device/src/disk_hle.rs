//! Shared High-Level Emulation (HLE) functions for disk controllers.
//!
//! Both SASI and IDE use the same INT 1Bh BIOS interface for disk I/O.
//! This module provides the common HLE functions used by both controllers.

use crate::disk::{HddGeometry, HddImage};

/// Computes the sector position from register values.
///
/// If `drive_select` bit 7 is set (drives are always 0x80+), uses CHS
/// addressing where CX = cylinder, DH = head, DL = sector:
///   `LBA = (CX * heads + DH) * sectors_per_track + DL`
///
/// Otherwise, uses direct LBA addressing: `(DL<<16 | CX) & 0x1FFFFF`.
pub fn sector_position(drive_select: u8, cx: u16, dx: u16, geometry: &HddGeometry) -> u32 {
    if drive_select & 0x80 != 0 {
        let cylinder = cx as u32;
        let head = (dx >> 8) as u32;
        let sector = (dx & 0xFF) as u32;
        (cylinder * geometry.heads as u32 + head) * geometry.sectors_per_track as u32 + sector
    } else {
        let pos = ((dx as u32 & 0xFF) << 16) | cx as u32;
        pos & 0x1F_FFFF
    }
}

/// Computes the transfer size from BX. 0 means 65536 bytes.
pub fn transfer_size(bx: u16) -> u32 {
    if bx == 0 { 0x10000 } else { bx as u32 }
}

/// Computes the buffer address from ES:BP (linear = ES*16 + BP).
pub fn buffer_address(es: u16, bp: u16) -> u32 {
    (es as u32) * 16 + bp as u32
}

/// Computes the drive index (0 or 1) from the drive select byte (AL).
pub fn drive_index(drive_select: u8) -> usize {
    (drive_select & 0x03) as usize
}

/// Executes a BIOS read operation: reads sectors from the HDD image
/// and writes them to the caller's memory buffer via the provided closure.
///
/// Returns the status code (0x00 on success).
pub fn execute_read(
    drive_idx: usize,
    xfer_size: u32,
    sector_pos: u32,
    buf_addr: u32,
    drives: &[Option<HddImage>; 2],
    mut write_byte: impl FnMut(u32, u8),
) -> u8 {
    let Some(drive) = &drives[drive_idx] else {
        return 0x60;
    };

    let mut remaining = xfer_size;
    let mut pos = sector_pos;
    let mut addr = buf_addr;
    let sector_size = drive.geometry.sector_size as u32;

    while remaining > 0 {
        let read_size = remaining.min(sector_size);

        let Some(sector_data) = drive.read_sector(pos) else {
            return 0xD0;
        };

        for &byte in &sector_data[..read_size as usize] {
            write_byte(addr, byte);
            addr += 1;
        }

        remaining -= read_size;
        pos += 1;
    }

    0x00
}

/// Executes a BIOS write operation: reads from the caller's memory buffer
/// and writes sectors to the HDD image.
///
/// The const generic `SECTOR_SIZE` determines the stack buffer size:
/// SASI callers use `256`, IDE callers use `512`.
pub fn execute_write<const SECTOR_SIZE: usize>(
    drive_idx: usize,
    xfer_size: u32,
    sector_pos: u32,
    buf_addr: u32,
    drives: &mut [Option<HddImage>; 2],
    read_byte: impl Fn(u32) -> u8,
) -> u8 {
    let Some(drive) = &mut drives[drive_idx] else {
        return 0x60;
    };

    let mut remaining = xfer_size;
    let mut pos = sector_pos;
    let mut addr = buf_addr;
    let sector_size = drive.geometry.sector_size as usize;

    while remaining > 0 {
        let write_size = (remaining as usize).min(sector_size);
        let mut buffer = [0u8; SECTOR_SIZE];

        for byte in buffer.iter_mut().take(write_size) {
            *byte = read_byte(addr);
            addr += 1;
        }

        if !drive.write_sector(pos, &buffer[..sector_size]) {
            return 0x70;
        }

        remaining -= write_size as u32;
        pos += 1;
    }

    0x00
}

/// Executes a BIOS init operation: returns the disk equipment word
/// indicating which drives are present. Preserves non-disk bits from
/// the current equipment word using mask 0xF0FF.
pub fn execute_init(drives: &[Option<HddImage>; 2], current_equip: u16) -> u16 {
    let mut disk_equip = current_equip & 0xF0FF;
    for (i, drive) in drives.iter().enumerate() {
        if drive.is_some() {
            disk_equip |= 0x0100 << i;
        }
    }
    disk_equip
}

/// Executes a BIOS format operation on a track.
pub fn execute_format(drive_idx: usize, sector_pos: u32, drives: &mut [Option<HddImage>; 2]) -> u8 {
    let Some(drive) = &mut drives[drive_idx] else {
        return 0x60;
    };
    if drive.format_track(sector_pos) {
        0x00
    } else {
        0xD0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::{HddFormat, HddGeometry};

    fn make_sasi_drive() -> HddImage {
        let geometry = HddGeometry {
            cylinders: 153,
            heads: 4,
            sectors_per_track: 33,
            sector_size: 256,
        };
        let total = geometry.total_bytes() as usize;
        let mut data = vec![0u8; total];
        for lba in 0..geometry.total_sectors() {
            let offset = lba as usize * 256;
            data[offset] = (lba >> 8) as u8;
            data[offset + 1] = lba as u8;
        }
        HddImage::from_raw(geometry, HddFormat::Thd, data)
    }

    fn make_ide_drive() -> HddImage {
        let geometry = HddGeometry {
            cylinders: 20,
            heads: 4,
            sectors_per_track: 17,
            sector_size: 512,
        };
        let total = geometry.total_bytes() as usize;
        let mut data = vec![0u8; total];
        for lba in 0..geometry.total_sectors() {
            let offset = lba as usize * 512;
            data[offset] = (lba >> 8) as u8;
            data[offset + 1] = lba as u8;
        }
        HddImage::from_raw(geometry, HddFormat::Hdi, data)
    }

    fn sasi_geometry() -> HddGeometry {
        HddGeometry {
            cylinders: 153,
            heads: 4,
            sectors_per_track: 33,
            sector_size: 256,
        }
    }

    fn ide_geometry() -> HddGeometry {
        HddGeometry {
            cylinders: 20,
            heads: 4,
            sectors_per_track: 17,
            sector_size: 512,
        }
    }

    #[test]
    fn sector_position_chs_sasi() {
        let geometry = sasi_geometry();
        // CHS(2, 1, 3) = (2 * 4 + 1) * 33 + 3 = 9 * 33 + 3 = 300
        assert_eq!(sector_position(0x80, 0x0002, 0x0103, &geometry), 300);
    }

    #[test]
    fn sector_position_chs_ide() {
        let geometry = ide_geometry();
        // CHS(2, 1, 3) = (2 * 4 + 1) * 17 + 3 = 9 * 17 + 3 = 156
        assert_eq!(sector_position(0x80, 0x0002, 0x0103, &geometry), 156);
    }

    #[test]
    fn sector_position_lba() {
        let geometry = sasi_geometry();
        assert_eq!(sector_position(0x00, 0x0042, 0x0000, &geometry), 0x42);
    }

    #[test]
    fn sector_position_lba_high_byte() {
        let geometry = sasi_geometry();
        assert_eq!(sector_position(0x01, 0x1234, 0x0005, &geometry), 0x51234);
    }

    #[test]
    fn transfer_size_zero_means_64k() {
        assert_eq!(transfer_size(0), 0x10000);
        assert_eq!(transfer_size(0x0100), 256);
        assert_eq!(transfer_size(0x0200), 512);
    }

    #[test]
    fn buffer_address_computation() {
        assert_eq!(buffer_address(0x1FC0, 0x0000), 0x1FC00);
        assert_eq!(buffer_address(0x2000, 0x0100), 0x20100);
    }

    #[test]
    fn drive_index_extraction() {
        assert_eq!(drive_index(0x80), 0);
        assert_eq!(drive_index(0x81), 1);
        assert_eq!(drive_index(0x00), 0);
        assert_eq!(drive_index(0x01), 1);
    }

    #[test]
    fn read_sasi_sector() {
        let drives: [Option<HddImage>; 2] = [Some(make_sasi_drive()), None];
        let geometry = sasi_geometry();
        let pos = sector_position(0x80, 0x0000, 0x0005, &geometry);
        let addr = buffer_address(0x2000, 0x0000);

        let mut writes = Vec::new();
        let status = execute_read(0, 256, pos, addr, &drives, |a, b| {
            writes.push((a, b));
        });
        assert_eq!(status, 0x00);
        assert_eq!(writes.len(), 256);
        assert_eq!(writes[0], (0x20000, 0x00));
        assert_eq!(writes[1], (0x20001, 0x05));
    }

    #[test]
    fn read_ide_sector() {
        let drives: [Option<HddImage>; 2] = [Some(make_ide_drive()), None];
        let geometry = ide_geometry();
        let pos = sector_position(0x80, 0x0000, 0x0005, &geometry);
        let addr = buffer_address(0x2000, 0x0000);

        let mut writes = Vec::new();
        let status = execute_read(0, 512, pos, addr, &drives, |a, b| {
            writes.push((a, b));
        });
        assert_eq!(status, 0x00);
        assert_eq!(writes.len(), 512);
        assert_eq!(writes[0], (0x20000, 0x00));
        assert_eq!(writes[1], (0x20001, 0x05));
    }

    #[test]
    fn read_no_drive_returns_error() {
        let drives: [Option<HddImage>; 2] = [None, None];
        let status = execute_read(0, 1, 0, 0x20000, &drives, |_, _| {});
        assert_eq!(status, 0x60);
    }

    #[test]
    fn write_sasi_sector() {
        let mut drives: [Option<HddImage>; 2] = [Some(make_sasi_drive()), None];
        let geometry = sasi_geometry();
        let pos = sector_position(0x80, 0x0000, 0x000A, &geometry);
        let addr = buffer_address(0x2000, 0x0000);

        let status = execute_write::<256>(0, 256, pos, addr, &mut drives, |_addr| 0xBB);
        assert_eq!(status, 0x00);

        let sector = drives[0].as_ref().unwrap().read_sector(10).unwrap();
        assert!(sector.iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn write_ide_sector() {
        let mut drives: [Option<HddImage>; 2] = [Some(make_ide_drive()), None];
        let geometry = ide_geometry();
        let pos = sector_position(0x80, 0x0000, 0x000A, &geometry);
        let addr = buffer_address(0x2000, 0x0000);

        let status = execute_write::<512>(0, 512, pos, addr, &mut drives, |_addr| 0xBB);
        assert_eq!(status, 0x00);

        let sector = drives[0].as_ref().unwrap().read_sector(10).unwrap();
        assert!(sector.iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn init_detects_drives() {
        let drives: [Option<HddImage>; 2] = [Some(make_sasi_drive()), None];
        let equip = execute_init(&drives, 0x0000);
        assert_eq!(equip, 0x0100);

        let both: [Option<HddImage>; 2] = [Some(make_sasi_drive()), Some(make_sasi_drive())];
        let equip = execute_init(&both, 0x0000);
        assert_eq!(equip, 0x0300);

        let none: [Option<HddImage>; 2] = [None, None];
        let equip = execute_init(&none, 0x0000);
        assert_eq!(equip, 0x0000);
    }

    #[test]
    fn init_preserves_non_disk_bits() {
        let drives: [Option<HddImage>; 2] = [Some(make_sasi_drive()), None];
        let equip = execute_init(&drives, 0x8040);
        assert_eq!(equip, 0x8140);
    }

    #[test]
    fn format_fills_with_e5() {
        let mut drives: [Option<HddImage>; 2] = [Some(make_sasi_drive()), None];
        let status = execute_format(0, 0, &mut drives);
        assert_eq!(status, 0x00);

        let sector = drives[0].as_ref().unwrap().read_sector(0).unwrap();
        assert!(sector.iter().all(|&b| b == 0xE5));
    }
}
