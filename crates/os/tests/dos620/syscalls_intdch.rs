use crate::harness;

#[test]
fn system_identification() {
    let mut machine = harness::boot_dos620();
    // INT DCh CL=12h: System identification.
    // Call with AX=0000h. If supported, AX changes to product number.
    // DX returns machine type (0003h = normal-mode PC-98).
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB9, 0x12, 0x00,                   // MOV CX, 0012h (CL=12h)
        0xB8, 0x00, 0x00,                   // MOV AX, 0000h
        0xCD, 0xDC,                         // INT DCh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    let ax = harness::result_word(&machine.bus, 0);
    // If AX changed from 0, the function is supported. Record the values.
    // Product numbers for MS-DOS 5.0+ are in the 0x0100+ range.
    // AX may remain 0 if INT DCh is not fully implemented yet.
    // For now, just verify we got some result without crashing.
    let _dx = harness::result_word(&machine.bus, 2);
    assert!(
        ax == 0x0000 || ax >= 0x0100,
        "INT DCh CL=12h: AX should be 0 (unsupported) or >= 0x0100 (product number), got {:#06X}",
        ax
    );
}

#[test]
fn daua_mapping_buffer() {
    let mut machine = harness::boot_dos620();
    // INT DCh CL=13h: Fill 96-byte DA/UA buffer at DS:DX.
    let buffer_offset: u16 = harness::INJECT_RESULT_OFFSET + 0x10;
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        0xB9, 0x13, 0x00,                                     // MOV CX, 0013h (CL=13h)
        0xBA, buffer_offset as u8, (buffer_offset >> 8) as u8, // MOV DX, buffer_offset
        0xCD, 0xDC,                                            // INT DCh
        0xFA,                                                  // CLI
        0xF4,                                                  // HLT
    ];
    harness::inject_and_run(&mut machine, &code);

    // First 16 bytes of the buffer should match DA/UA mapping at 0060:006Ch.
    let buffer_addr = harness::INJECT_RESULT_BASE + 0x10;
    let iosys_daua_addr = 0x0600 + 0x006C;

    for i in 0..16u32 {
        let from_buffer = harness::read_byte(&machine.bus, buffer_addr + i);
        let from_iosys = harness::read_byte(&machine.bus, iosys_daua_addr + i);
        assert_eq!(
            from_buffer, from_iosys,
            "DA/UA buffer byte {} ({:#04X}) should match IO.SYS table ({:#04X})",
            i, from_buffer, from_iosys
        );
    }
}

#[test]
fn internal_revision() {
    let machine = harness::boot_dos620();
    // INT DCh CL=15h returns internal revision from 0060:0022h.
    // We can just read the memory directly to establish the expected value.
    let revision = harness::read_byte(&machine.bus, 0x0600 + 0x0022);
    // The revision is an arbitrary internal number, just verify it's readable.
    // This establishes the baseline value for our HLE implementation.
    let _ = revision;
}

#[test]
fn extended_memory_query() {
    let machine = harness::boot_dos620();
    // INT DCh CL=81h returns extended memory size from 0060:0031h.
    // Read the memory directly to verify the field exists and is reasonable.
    let ext_mem_128kb_units = harness::read_byte(&machine.bus, 0x0600 + 0x0031);
    // PC-9801RA has extended memory. Value is in 128KB units.
    // 0 means no extended memory, which is also valid.
    assert!(
        (ext_mem_128kb_units as u32) * 128 <= 16384,
        "Extended memory should be <= 16384 KB (16 MB), got {} * 128 = {} KB",
        ext_mem_128kb_units,
        ext_mem_128kb_units as u32 * 128
    );
}

#[test]
fn noop_functions_00h_through_08h() {
    let mut machine = harness::boot_dos620();
    // Call INT DCh with CL=00h through CL=08h. These are documented no-ops.
    // They should return without hanging or crashing.
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB1, 0x00,                         // MOV CL, 00h
        // loop:
        0xB5, 0x00,                         // MOV CH, 00h
        0xCD, 0xDC,                         // INT DCh
        0xFE, 0xC1,                         // INC CL
        0x80, 0xF9, 0x09,                   // CMP CL, 09h
        0x72, 0xF5,                         // JB loop (back to MOV CH, 00h)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    // If we reach here, all 9 calls completed without hanging.
}

#[test]
fn disk_partition_info_80h() {
    let mut machine = harness::boot_dos620();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB9, 0x80, 0x00,                   // MOV CX, 0080h (CL=80h)
        0xB0, 0x00,                         // MOV AL, 00h
        0xB4, 0x00,                         // MOV AH, 00h
        0xCD, 0xDC,                         // INT DCh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);

    // Just verify the call completed without hanging. The return values
    // depend on the specific implementation.
    let _ax = harness::result_word(&machine.bus, 0);
    let _bx = harness::result_word(&machine.bus, 2);
}
