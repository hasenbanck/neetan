use std::path::Path;

use device::floppy::d88::{D88Disk, D88MediaType};

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture")
        .join(name)
}

fn load_fixture(name: &str) -> D88Disk {
    let path = fixture_path(name);
    let data =
        std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    D88Disk::from_bytes(&data).unwrap_or_else(|e| panic!("Failed to parse {name}: {e}"))
}

#[test]
fn parse_blank_2dd() {
    let disk = load_fixture("blank_2DD.d88");
    assert_eq!(disk.media_type, D88MediaType::Disk2DD);
    assert!(!disk.write_protected);
}

#[test]
fn blank_2dd_track_structure() {
    let disk = load_fixture("blank_2DD.d88");

    // 2DD: 80 cylinders × 2 heads = 160 tracks, 16 sectors/track, N=1 (256 bytes).
    for track in 0..160 {
        assert_eq!(
            disk.sector_count(track),
            16,
            "Track {track} should have 16 sectors"
        );
    }

    // First sector: C=0, H=0, R=1, N=1.
    let s = disk.find_sector(0, 0, 1, 1).unwrap();
    assert_eq!(s.data.len(), 256);

    // Blank disk is filled with 0x40 (standard format fill byte).
    assert!(
        s.data.iter().all(|&b| b == 0x40),
        "Blank 2DD disk sectors should be filled with 0x40"
    );
}

#[test]
fn blank_2dd_sector_lookup() {
    let disk = load_fixture("blank_2DD.d88");

    // All 16 sectors on track 0 should be findable (R=1..=16, N=1).
    for r in 1..=16u8 {
        let s = disk.find_sector(0, 0, r, 1);
        assert!(s.is_some(), "Sector R={r} not found on track 0");
    }

    // Nonexistent sector.
    assert!(disk.find_sector(0, 0, 17, 1).is_none());
}

#[test]
fn parse_blank_2hd() {
    let disk = load_fixture("blank_2HD.d88");
    assert_eq!(disk.media_type, D88MediaType::Disk2HD);
    assert!(!disk.write_protected);
}

#[test]
fn blank_2hd_track_structure() {
    let disk = load_fixture("blank_2HD.d88");

    // 2HD: 77 cylinders × 2 heads = 154 tracks, 26 sectors/track, N=0 (128 bytes).
    for track in 0..154 {
        assert_eq!(
            disk.sector_count(track),
            26,
            "Track {track} should have 26 sectors"
        );
    }

    // First sector: C=0, H=0, R=1, N=0.
    let s = disk.find_sector(0, 0, 1, 0).unwrap();
    assert_eq!(s.data.len(), 128);
}

#[test]
fn blank_2hd_sector_lookup() {
    let disk = load_fixture("blank_2HD.d88");

    // 26 sectors per track (R=1..=26, N=0).
    for r in 1..=26u8 {
        let s = disk.find_sector(0, 0, r, 0);
        assert!(s.is_some(), "Sector R={r} not found on track 0");
    }

    // Test a sector on a different cylinder/head.
    // Inner tracks use N=1 (256 bytes) instead of N=0 (128 bytes).
    let s = disk.find_sector(10, 1, 1, 1);
    assert!(s.is_some(), "Sector at C=10 H=1 R=1 N=1 should exist");
}

#[test]
fn sector_at_index_wraps_2hd() {
    let disk = load_fixture("blank_2HD.d88");
    let s0 = disk.sector_at_index(0, 0).unwrap();
    let s26 = disk.sector_at_index(0, 26).unwrap();
    assert_eq!(s0.record, s26.record);
    assert_eq!(s0.cylinder, s26.cylinder);
}
