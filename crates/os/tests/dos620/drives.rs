use crate::harness;

const IOSYS_BASE: u32 = 0x0600;

#[test]
fn daua_floppy_assignment() {
    let machine = harness::boot_hle();
    // DA/UA table at 0060:006Ch, 16 bytes for A:-P:.
    // HLE boots with 2 built-in 1MB FDD drives: A:=0x90, B:=0x91.
    let first_drive_daua = harness::read_byte(&machine.bus, IOSYS_BASE + 0x006C);
    assert_eq!(
        first_drive_daua, 0x90,
        "Boot drive A: DA/UA should be 0x90 (1MB FDD unit 0), got {:#04X}",
        first_drive_daua
    );
}

#[test]
fn dpb_chain_matches_drives() {
    let mut machine = harness::boot_hle();
    let sysvars = harness::get_sysvars_address(&mut machine);
    let (seg, off) = harness::read_far_ptr(&machine.bus, sysvars);
    let mut dpb_addr = harness::far_to_linear(seg, off);

    let mut drive_numbers = Vec::new();
    for _ in 0..26 {
        if dpb_addr == 0 || dpb_addr >= 0xA0000 {
            break;
        }

        let drive_num = harness::read_byte(&machine.bus, dpb_addr);
        drive_numbers.push(drive_num);

        // Next DPB pointer is at offset +0x19 in the DPB (DOS 4.0+ format).
        // For DOS 3.x it might be at a different offset. Try +0x19 first.
        let (next_seg, next_off) = harness::read_far_ptr(&machine.bus, dpb_addr + 0x19);
        if next_seg == 0xFFFF && next_off == 0xFFFF {
            break;
        }
        dpb_addr = harness::far_to_linear(next_seg, next_off);
    }

    assert!(
        !drive_numbers.is_empty(),
        "DPB chain should contain at least one entry"
    );

    for &drive in &drive_numbers {
        assert!(drive < 26, "DPB drive number should be < 26, got {}", drive);
    }
}

#[test]
fn sysvars_max_sector_size() {
    let mut machine = harness::boot_hle();
    let sysvars = harness::get_sysvars_address(&mut machine);
    let max_sector = harness::read_word(&machine.bus, sysvars + 0x10);
    // HDD uses 512-byte sectors, so max should be at least 512.
    assert!(
        max_sector >= 512,
        "SYSVARS max bytes per sector should be >= 512, got {}",
        max_sector
    );
}
