use crate::harness;

#[test]
fn ivt_has_valid_int21h_vector() {
    let machine = harness::boot_dos620();
    // INT 21h vector is at IVT offset 0x21 * 4 = 0x0084.
    let (segment, offset) = harness::read_far_ptr(&machine.bus, 0x0084);
    let linear = harness::far_to_linear(segment, offset);
    assert_ne!(
        linear, 0,
        "INT 21h vector should point to a valid DOS handler, got 0000:0000"
    );
    assert!(
        linear < 0xA0000,
        "INT 21h vector should be in conventional memory, got {:#010X}",
        linear
    );
}

#[test]
fn bios_data_area_populated() {
    let machine = harness::boot_dos620();
    // BIOS Data Area starts at 0x0400. Check equipment word area at 0x0500
    // and memory size at 0x0501.
    let equipment_byte = harness::read_byte(&machine.bus, 0x0500);
    // After POST, this should be non-zero (contains display/floppy/memory info).
    // At minimum, the system should have some equipment configured.
    assert_ne!(
        equipment_byte, 0x00,
        "BDA equipment byte at 0x0500 should be non-zero after POST"
    );
}

#[test]
fn dos_data_area_populated() {
    let machine = harness::boot_dos620();
    // DOS data structures start around 0x0600. After DOS boot, this area should
    // contain SYSVARS and other DOS structures -- not all zeros.
    let mut nonzero_count = 0;
    for i in 0..0x100 {
        if harness::read_byte(&machine.bus, 0x0600 + i) != 0 {
            nonzero_count += 1;
        }
    }
    assert!(
        nonzero_count > 10,
        "DOS data area at 0x0600 should contain significant non-zero data, found only {} non-zero bytes",
        nonzero_count
    );
}

#[test]
fn nul_device_header_at_expected_location() {
    let mut machine = harness::boot_dos620();
    // Get SYSVARS via INT 21h/52h. NUL device header starts at SYSVARS+0x22.
    let sysvars = harness::get_sysvars_address(&mut machine);
    let name = harness::read_device_name(&machine.bus, sysvars + 0x22);
    assert_eq!(
        name.trim(),
        "NUL",
        "Device header at SYSVARS+0x22 should be NUL device, got '{}'",
        name
    );
}

#[test]
fn conventional_memory_below_a0000() {
    let mut machine = harness::boot_dos620();
    // PC-98 always has 640KB conventional memory (unlike IBM PC where INT 12h returns KB).
    // Verify the MCB chain stays within conventional memory (below 0xA0000).
    let sysvars = harness::get_sysvars_address(&mut machine);
    let first_mcb_seg = harness::read_word(&machine.bus, sysvars - 2);
    let mut mcb_addr = harness::far_to_linear(first_mcb_seg, 0);

    for _ in 0..1000 {
        let block_type = harness::read_byte(&machine.bus, mcb_addr);
        let size = harness::read_word(&machine.bus, mcb_addr + 3);
        let mcb_seg = mcb_addr >> 4;
        let end_seg = mcb_seg + size as u32 + 1;
        let end_linear = end_seg << 4;

        assert!(
            end_linear <= 0xA0000,
            "MCB chain must stay below 0xA0000, block at seg {:#06X} extends to {:#010X}",
            mcb_seg,
            end_linear
        );

        if block_type == 0x5A {
            break;
        }
        mcb_addr = end_seg << 4;
    }
}

#[test]
fn ivt_int29h_vector_valid() {
    let machine = harness::boot_dos620();
    // INT 29h vector at IVT offset 0x29 * 4 = 0x00A4.
    let (segment, offset) = harness::read_far_ptr(&machine.bus, 0x00A4);
    let linear = harness::far_to_linear(segment, offset);
    assert_ne!(
        linear, 0,
        "INT 29h vector should be non-zero (fast console output handler)"
    );
    assert!(
        linear < 0xA0000,
        "INT 29h vector should be in conventional memory, got {:#010X}",
        linear
    );
}

#[test]
fn ivt_int2fh_vector_valid() {
    let machine = harness::boot_dos620();
    // INT 2Fh vector at IVT offset 0x2F * 4 = 0x00BC.
    let (segment, offset) = harness::read_far_ptr(&machine.bus, 0x00BC);
    let linear = harness::far_to_linear(segment, offset);
    assert_ne!(
        linear, 0,
        "INT 2Fh vector should be non-zero (multiplex interrupt handler)"
    );
    assert!(
        linear < 0xA0000,
        "INT 2Fh vector should be in conventional memory, got {:#010X}",
        linear
    );
}

#[test]
fn ivt_intdch_vector_valid() {
    let machine = harness::boot_dos620();
    // INT DCh vector at IVT offset 0xDC * 4 = 0x0370.
    let (segment, offset) = harness::read_far_ptr(&machine.bus, 0x0370);
    let linear = harness::far_to_linear(segment, offset);
    assert_ne!(
        linear, 0,
        "INT DCh vector should be non-zero (NEC DOS extension handler)"
    );
    assert!(
        linear < 0xA0000,
        "INT DCh vector should be in conventional memory, got {:#010X}",
        linear
    );
}
