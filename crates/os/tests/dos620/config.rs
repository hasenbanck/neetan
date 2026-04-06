use crate::harness;

#[test]
fn default_files_count() {
    let mut machine = harness::boot_dos620();
    let sysvars = harness::get_sysvars_address(&mut machine);

    // Walk the SFT chain from SYSVARS+0x04 and count total file entries.
    let (seg, off) = harness::read_far_ptr(&machine.bus, sysvars + 0x04);
    let mut sft_addr = harness::far_to_linear(seg, off);
    let mut total_entries = 0u32;

    for _ in 0..20 {
        if sft_addr == 0 || sft_addr >= 0xA0000 {
            break;
        }

        // SFT header: 4 bytes next pointer, 2 bytes entry count.
        let entry_count = harness::read_word(&machine.bus, sft_addr + 4);
        total_entries += entry_count as u32;

        let (next_seg, next_off) = harness::read_far_ptr(&machine.bus, sft_addr);
        if next_seg == 0xFFFF && next_off == 0xFFFF {
            break;
        }
        sft_addr = harness::far_to_linear(next_seg, next_off);
    }

    assert!(
        total_entries >= 20,
        "Total SFT entries should be >= 20 (default FILES=20), got {}",
        total_entries
    );
}

#[test]
fn default_buffers() {
    let mut machine = harness::boot_dos620();
    let sysvars = harness::get_sysvars_address(&mut machine);
    let buffers = harness::read_word(&machine.bus, sysvars + 0x3F);
    assert!(
        (1..=99).contains(&buffers),
        "BUFFERS should be between 1 and 99, got {}",
        buffers
    );
}

#[test]
fn default_lastdrive() {
    let mut machine = harness::boot_dos620();
    let sysvars = harness::get_sysvars_address(&mut machine);
    let lastdrive = harness::read_byte(&machine.bus, sysvars + 0x21);
    assert!(
        (5..=26).contains(&lastdrive),
        "LASTDRIVE should be between 5 and 26, got {}",
        lastdrive
    );
}
