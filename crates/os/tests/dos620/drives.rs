use crate::harness;

const IOSYS_BASE: u32 = 0x0600;
const HDI_HEADER_SIZE: usize = 32;

fn hdi_sector_size(data: &[u8]) -> usize {
    u32::from_le_bytes([data[0x10], data[0x11], data[0x12], data[0x13]]) as usize
}

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
fn hdd_partition_table_ipl_signature() {
    let data = harness::load_hdd_image_data();
    let sector_size = hdi_sector_size(&data);
    assert!(
        data.len() > HDI_HEADER_SIZE + sector_size,
        "HDI image too small for partition table"
    );
    // HDI header is 32 bytes, then disk data starts.
    let sector0 = &data[HDI_HEADER_SIZE..HDI_HEADER_SIZE + sector_size];

    // "IPL1" signature at offset 0x04.
    assert_eq!(
        &sector0[0x04..0x08],
        b"IPL1",
        "HDD sector 0 should have 'IPL1' signature at offset 0x04"
    );

    // Boot signature 0xAA55 at last two bytes of sector 0.
    let sig_offset = sector_size - 2;
    let boot_sig = u16::from_le_bytes([sector0[sig_offset], sector0[sig_offset + 1]]);
    assert_eq!(
        boot_sig, 0xAA55,
        "HDD sector 0 should have 0xAA55 at offset {:#06X}, got {:#06X}",
        sig_offset, boot_sig
    );
}

#[test]
fn hdd_partition_table_entries() {
    let data = harness::load_hdd_image_data();
    let sector_size = hdi_sector_size(&data);
    let partition_table_start = HDI_HEADER_SIZE + sector_size;
    assert!(
        data.len() > partition_table_start + sector_size,
        "HDI image too small for partition table sector 1"
    );
    // Partition entries start at sector 1, each 32 bytes.
    // With 256-byte sectors, only 8 entries fit per sector; scan up to 512 bytes (2 sectors).
    let partition_data_len = 512.min(data.len() - partition_table_start);
    let partition_data = &data[partition_table_start..partition_table_start + partition_data_len];

    let mut found_active_dos = false;
    let num_entries = partition_data_len / 32;
    for entry_idx in 0..num_entries {
        let entry = &partition_data[entry_idx * 32..(entry_idx + 1) * 32];
        let mid = entry[0x00];
        let sid = entry[0x01];

        // Active partition: sid bit 7 set. DOS partition: mid in 0x20-0x2F range.
        let is_active = sid & 0x80 != 0;
        let is_dos = (0x20..=0x2F).contains(&(mid & 0x7F));
        if is_active && is_dos {
            found_active_dos = true;
        }
    }

    assert!(
        found_active_dos,
        "HDD partition table should contain at least one active DOS partition"
    );
}

#[test]
fn partition_name() {
    let data = harness::load_hdd_image_data();
    let sector_size = hdi_sector_size(&data);
    let partition_table_start = HDI_HEADER_SIZE + sector_size;
    assert!(
        data.len() > partition_table_start + sector_size,
        "HDI image too small for partition table"
    );
    let partition_data_len = 512.min(data.len() - partition_table_start);
    let partition_data = &data[partition_table_start..partition_table_start + partition_data_len];

    let mut found_named = false;
    let num_entries = partition_data_len / 32;
    for entry_idx in 0..num_entries {
        let entry = &partition_data[entry_idx * 32..(entry_idx + 1) * 32];
        let mid = entry[0x00];
        let sid = entry[0x01];

        let is_active = sid & 0x80 != 0;
        let is_dos = (0x20..=0x2F).contains(&(mid & 0x7F));
        if is_active && is_dos {
            let name = &entry[0x10..0x20];
            let name_str = String::from_utf8_lossy(name);
            let trimmed = name_str.trim_end_matches('\0').trim();
            if !trimmed.is_empty() {
                found_named = true;
            }
        }
    }

    assert!(
        found_named,
        "At least one active DOS partition should have a non-empty name"
    );
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
