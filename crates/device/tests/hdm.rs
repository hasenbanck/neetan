use std::path::Path;

use device::floppy::{
    d88::{D88Disk, D88MediaType},
    hdm,
};

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture")
        .join(name)
}

fn load_fixture(name: &str) -> D88Disk {
    let path = fixture_path(name);
    let data = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("Failed to read {}: {error}", path.display()));
    hdm::from_bytes(&data).unwrap_or_else(|error| panic!("Failed to parse {name}: {error}"))
}

#[test]
fn parse_blank_hdm() {
    let disk = load_fixture("blank.hdm");
    assert_eq!(disk.media_type, D88MediaType::Disk2HD);
    assert!(!disk.write_protected);
}

#[test]
fn blank_hdm_track_structure() {
    let disk = load_fixture("blank.hdm");

    // 77 cylinders × 2 heads = 154 tracks, 8 sectors/track.
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
fn blank_hdm_sector_lookup() {
    let disk = load_fixture("blank.hdm");

    // All 8 sectors on track 0 (R=1..=8, N=3).
    for r in 1..=8u8 {
        let s = disk.find_sector(0, 0, r, 3);
        assert!(s.is_some(), "Sector R={r} not found on track 0");
        assert_eq!(s.unwrap().data.len(), 1024);
    }

    // Nonexistent R=9.
    assert!(disk.find_sector(0, 0, 9, 3).is_none());
}

#[test]
fn blank_hdm_second_head() {
    let disk = load_fixture("blank.hdm");
    let s = disk.find_sector(0, 1, 1, 3);
    assert!(s.is_some(), "Sector at C=0 H=1 R=1 N=3 should exist");
}

#[test]
fn sector_at_index_wraps_hdm() {
    let disk = load_fixture("blank.hdm");
    let s0 = disk.sector_at_index(0, 0).unwrap();
    let s8 = disk.sector_at_index(0, 8).unwrap();
    assert_eq!(s0.record, s8.record);
    assert_eq!(s0.cylinder, s8.cylinder);
}
