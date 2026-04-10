use crate::harness;

#[test]
fn test_ems_status() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x40,                         // MOV AH, 40h (get status)
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    assert_eq!(ah, 0x00, "EMS status should be 0x00 (OK), got {:#04X}", ah);
}

#[test]
fn test_ems_page_frame_segment() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x41,                         // MOV AH, 41h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    let bx = harness::result_word(&machine.bus, 2);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
    assert_eq!(
        bx, 0xC000,
        "Page frame segment should be 0xC000, got {:#06X}",
        bx
    );
}

#[test]
fn test_ems_unallocated_page_count() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x42,                         // MOV AH, 42h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (free pages)
        0x89, 0x16, 0x04, 0x01,             // MOV [0x0104], DX (total pages)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    let free = harness::result_word(&machine.bus, 2);
    let total = harness::result_word(&machine.bus, 4);
    assert_eq!(ah, 0x00);
    assert!(free > 0, "Free pages should be > 0, got {}", free);
    assert!(total > 0, "Total pages should be > 0, got {}", total);
    assert!(
        free <= total,
        "Free ({}) should be <= total ({})",
        free,
        total
    );
}

#[test]
fn test_ems_version() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x46,                         // MOV AH, 46h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ax = harness::result_word(&machine.bus, 0);
    assert_eq!(
        ax, 0x0040,
        "EMS version should be 0x0040 (4.0), got {:#06X}",
        ax
    );
}

#[test]
fn test_ems_allocate_and_deallocate() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 1 page
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (status)
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (handle)
        // Deallocate
        0x8B, 0x16, 0x02, 0x01,             // MOV DX, [0x0102]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (dealloc status)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let alloc_ah = harness::result_byte(&machine.bus, 1);
    let dealloc_ah = harness::result_byte(&machine.bus, 5);
    assert_eq!(
        alloc_ah, 0x00,
        "Allocate should succeed, got AH={:#04X}",
        alloc_ah
    );
    assert_eq!(
        dealloc_ah, 0x00,
        "Deallocate should succeed, got AH={:#04X}",
        dealloc_ah
    );
}

#[test]
fn test_ems_map_write_read() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 1 page
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (handle)
        // Map logical 0 to physical 0: AH=44h, AL=physical, BX=logical, DX=handle
        0xB8, 0x00, 0x44,                   // MOV AX, 4400h (AH=44, AL=0=phys page)
        0xBB, 0x00, 0x00,                   // MOV BX, 0 (logical page)
        0x8B, 0x16, 0x02, 0x01,             // MOV DX, [0x0102]
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (map status)
        // Write 0xAB to C000:0000
        0xB8, 0x00, 0xC0,                   // MOV AX, C000h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0xAB, // MOV BYTE ES:[0000], ABh
        // Read it back
        0x26, 0xA0, 0x00, 0x00,             // MOV AL, ES:[0000]
        0xA2, 0x06, 0x01,                   // MOV [0x0106], AL
        // Restore DS segment
        0xB8, 0x00, 0x20,                   // MOV AX, 2000h
        0x8E, 0xD8,                         // MOV DS, AX
        // Deallocate
        0x8B, 0x16, 0x02, 0x01,             // MOV DX, [0x0102]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let alloc_ah = harness::result_byte(&machine.bus, 1);
    let map_ah = harness::result_byte(&machine.bus, 5);
    let read_val = harness::result_byte(&machine.bus, 6);
    assert_eq!(alloc_ah, 0x00, "Allocate AH={:#04X}", alloc_ah);
    assert_eq!(map_ah, 0x00, "Map AH={:#04X}", map_ah);
    assert_eq!(
        read_val, 0xAB,
        "Read from page frame should be 0xAB, got {:#04X}",
        read_val
    );
}

#[test]
fn test_ems_map_all_four_pages() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        // Allocate 4 pages
        0xBB, 0x04, 0x00,                   // MOV BX, 4
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x00, 0x01,             // MOV [0x0100], DX (handle)
        // Map logical 0 to physical 0
        0xB8, 0x00, 0x44,                   // MOV AX, 4400h
        0xBB, 0x00, 0x00,                   // MOV BX, 0
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xCD, 0x67,                         // INT 67h
        // Map logical 1 to physical 1
        0xB8, 0x01, 0x44,                   // MOV AX, 4401h
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xCD, 0x67,                         // INT 67h
        // Map logical 2 to physical 2
        0xB8, 0x02, 0x44,                   // MOV AX, 4402h
        0xBB, 0x02, 0x00,                   // MOV BX, 2
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xCD, 0x67,                         // INT 67h
        // Map logical 3 to physical 3
        0xB8, 0x03, 0x44,                   // MOV AX, 4403h
        0xBB, 0x03, 0x00,                   // MOV BX, 3
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xCD, 0x67,                         // INT 67h
        // Write unique bytes to each page frame slot via ES
        0xB8, 0x00, 0xC0,                   // MOV AX, C000h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0x11, // ES:[0000] = 11h (slot 0)
        0xB8, 0x00, 0xC4,                   // MOV AX, C400h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0x22, // ES:[0000] = 22h (slot 1)
        0xB8, 0x00, 0xC8,                   // MOV AX, C800h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0x33, // ES:[0000] = 33h (slot 2)
        0xB8, 0x00, 0xCC,                   // MOV AX, CC00h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0x44, // ES:[0000] = 44h (slot 3)
        // Read them all back; store results in DS segment
        0xB8, 0x00, 0x20,                   // MOV AX, 2000h
        0x8E, 0xD8,                         // MOV DS, AX
        0xB8, 0x00, 0xC0,                   // MOV AX, C000h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xA0, 0x00, 0x00,             // MOV AL, ES:[0000]
        0xA2, 0x02, 0x01,                   // MOV [0x0102], AL
        0xB8, 0x00, 0xC4,                   // MOV AX, C400h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xA0, 0x00, 0x00,             // MOV AL, ES:[0000]
        0xA2, 0x03, 0x01,                   // MOV [0x0103], AL
        0xB8, 0x00, 0xC8,                   // MOV AX, C800h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xA0, 0x00, 0x00,             // MOV AL, ES:[0000]
        0xA2, 0x04, 0x01,                   // MOV [0x0104], AL
        0xB8, 0x00, 0xCC,                   // MOV AX, CC00h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xA0, 0x00, 0x00,             // MOV AL, ES:[0000]
        0xA2, 0x05, 0x01,                   // MOV [0x0105], AL
        // Deallocate
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [0x0100]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, &code);
    assert_eq!(harness::result_byte(&machine.bus, 2), 0x11, "Slot 0");
    assert_eq!(harness::result_byte(&machine.bus, 3), 0x22, "Slot 1");
    assert_eq!(harness::result_byte(&machine.bus, 4), 0x33, "Slot 2");
    assert_eq!(harness::result_byte(&machine.bus, 5), 0x44, "Slot 3");
}

#[test]
fn test_ems_get_handle_count() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate handle 1
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (handle 1)
        // Allocate handle 2
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x08, 0x01,             // MOV [0x0108], DX (handle 2)
        // Get handle count
        0xB4, 0x4B,                         // MOV AH, 4Bh
        0xCD, 0x67,                         // INT 67h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (count)
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (status)
        // Deallocate both
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [0x0106]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [0x0108]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let count = harness::result_word(&machine.bus, 0);
    let ah = harness::result_byte(&machine.bus, 3);
    assert_eq!(ah, 0x00);
    assert!(count >= 2, "Handle count should be >= 2, got {}", count);
}

#[test]
fn test_ems_get_handle_pages() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 3 pages
        0xBB, 0x03, 0x00,                   // MOV BX, 3
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x04, 0x01,             // MOV [0x0104], DX (handle)
        // Get handle pages
        0x8B, 0x16, 0x04, 0x01,             // MOV DX, [handle]
        0xB4, 0x4C,                         // MOV AH, 4Ch
        0xCD, 0x67,                         // INT 67h
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (pages)
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (status)
        // Deallocate
        0x8B, 0x16, 0x04, 0x01,             // MOV DX, [handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let pages = harness::result_word(&machine.bus, 0);
    let ah = harness::result_byte(&machine.bus, 3);
    assert_eq!(ah, 0x00);
    assert_eq!(pages, 3, "Handle should have 3 pages, got {}", pages);
}

#[test]
fn test_ems_save_and_restore() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        // Allocate 2 pages
        0xBB, 0x02, 0x00,                   // MOV BX, 2
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x00, 0x01,             // MOV [0x0100], DX (handle)
        // Map logical 0 to physical 0
        0xB8, 0x00, 0x44,                   // MOV AX, 4400h
        0xBB, 0x00, 0x00,                   // MOV BX, 0
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xCD, 0x67,                         // INT 67h
        // Write 0xAA to page frame
        0xB8, 0x00, 0xC0,                   // MOV AX, C000h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0xAA, // ES:[0000] = AAh
        // Restore DS
        0xB8, 0x00, 0x20,                   // MOV AX, 2000h
        0x8E, 0xD8,                         // MOV DS, AX
        // Save page map (AH=47h)
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xB4, 0x47,                         // MOV AH, 47h
        0xCD, 0x67,                         // INT 67h
        // Map logical 1 to physical 0 (replaces page 0)
        0xB8, 0x00, 0x44,                   // MOV AX, 4400h
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xCD, 0x67,                         // INT 67h
        // Write 0xBB to page frame
        0xB8, 0x00, 0xC0,                   // MOV AX, C000h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xC6, 0x06, 0x00, 0x00, 0xBB, // ES:[0000] = BBh
        // Restore DS
        0xB8, 0x00, 0x20,                   // MOV AX, 2000h
        0x8E, 0xD8,                         // MOV DS, AX
        // Restore page map (AH=48h)
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xB4, 0x48,                         // MOV AH, 48h
        0xCD, 0x67,                         // INT 67h
        // Read page frame byte
        0xB8, 0x00, 0xC0,                   // MOV AX, C000h
        0x8E, 0xC0,                         // MOV ES, AX
        0x26, 0xA0, 0x00, 0x00,             // MOV AL, ES:[0000]
        // Store result before restoring DS (MOV AX clobbers AL)
        0xA2, 0x02, 0x01,                   // MOV [0x0102], AL
        0xB8, 0x00, 0x20,                   // MOV AX, 2000h
        0x8E, 0xD8,                         // MOV DS, AX
        // Deallocate
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, &code);
    let val = harness::result_byte(&machine.bus, 2);
    assert_eq!(
        val, 0xAA,
        "After restore, page frame should have 0xAA, got {:#04X}",
        val
    );
}

#[test]
fn test_ems_reallocate() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 2 pages
        0xBB, 0x02, 0x00,                   // MOV BX, 2
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (handle)
        // Reallocate to 4 pages (AH=51h, BX=new_count, DX=handle)
        0xBB, 0x04, 0x00,                   // MOV BX, 4
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x51,                         // MOV AH, 51h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (realloc status)
        // Get page count
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x4C,                         // MOV AH, 4Ch
        0xCD, 0x67,                         // INT 67h
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (page count)
        // Deallocate
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let realloc_ah = harness::result_byte(&machine.bus, 1);
    let pages = harness::result_word(&machine.bus, 2);
    assert_eq!(realloc_ah, 0x00, "Reallocate AH={:#04X}", realloc_ah);
    assert_eq!(
        pages, 4,
        "After reallocate, pages should be 4, got {}",
        pages
    );
}

#[test]
fn test_ems_handle_name() {
    let mut machine = harness::boot_hle();
    let data_offset = 0x0200u16;
    let name_bytes = b"TESTEMS\0";
    harness::write_bytes(
        &mut machine.bus,
        harness::INJECT_CODE_BASE + data_offset as u32,
        name_bytes,
    );

    #[rustfmt::skip]
    let code: Vec<u8> = vec![
        // Allocate 1 page
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x16, 0x00, 0x01,             // MOV [0x0100], DX (handle)
        // Set handle name: AH=53h, AL=01h, DX=handle, DS:SI=name
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xBE, data_offset as u8, (data_offset >> 8) as u8, // MOV SI, data_offset
        0xB8, 0x01, 0x53,                   // MOV AX, 5301h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (set status)
        // Get handle name: AH=53h, AL=00h, DX=handle, ES:DI=buffer
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xBF, 0x08, 0x01,                   // MOV DI, 0x0108 (result+8)
        0xB8, 0x00, 0x53,                   // MOV AX, 5300h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (get status)
        // Deallocate
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, &code);

    let set_ah = harness::result_byte(&machine.bus, 3);
    let get_ah = harness::result_byte(&machine.bus, 5);
    assert_eq!(set_ah, 0x00, "Set name AH={:#04X}", set_ah);
    assert_eq!(get_ah, 0x00, "Get name AH={:#04X}", get_ah);
    let got_name = harness::read_bytes(&machine.bus, harness::INJECT_RESULT_BASE + 8, 8);
    assert_eq!(&got_name, name_bytes, "Name mismatch");
}

#[test]
fn test_ems_allocate_raw_pages() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // AH=5Ah, AL=00h (allocate raw), BX=0 pages
        0xBB, 0x00, 0x00,                   // MOV BX, 0
        0xB8, 0x00, 0x5A,                   // MOV AX, 5A00h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (handle)
        // Deallocate
        0x8B, 0x16, 0x02, 0x01,             // MOV DX, [handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    assert_eq!(ah, 0x00, "Allocate raw pages AH={:#04X}", ah);
}

#[test]
fn test_xms_version() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x00,                         // MOV AH, 00h (get version)
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (version)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (revision)
        0x89, 0x16, 0x04, 0x01,             // MOV [0x0104], DX (hma_exists)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let version = harness::result_word(&machine.bus, 0);
    let revision = harness::result_word(&machine.bus, 2);
    let hma = harness::result_word(&machine.bus, 4);
    assert_eq!(
        version, 0x0300,
        "XMS version should be 3.00, got {:#06X}",
        version
    );
    assert_eq!(
        revision, 0x0001,
        "XMS revision should be 0001, got {:#06X}",
        revision
    );
    assert_eq!(hma, 1, "HMA should exist (DX=1), got {}", hma);
}

#[test]
fn test_xms_entry_point_via_int2f() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x10, 0x43,                   // MOV AX, 4310h
        0xCD, 0x2F,                         // INT 2Fh
        0x8C, 0x06, 0x00, 0x01,             // MOV [0x0100], ES (entry segment)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (entry offset)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let seg = harness::result_word(&machine.bus, 0);
    let off = harness::result_word(&machine.bus, 2);
    assert_eq!(
        seg, 0x0200,
        "Entry segment should be 0x0200, got {:#06X}",
        seg
    );
    assert_eq!(
        off, 0x0D44,
        "Entry offset should be 0x0D44, got {:#06X}",
        off
    );
}

#[test]
fn test_xms_query_free() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x08,                         // MOV AH, 08h (query free)
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (largest)
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (total)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let largest = harness::result_word(&machine.bus, 0);
    let total = harness::result_word(&machine.bus, 2);
    assert!(
        largest > 0,
        "Largest free block should be > 0, got {}",
        largest
    );
    assert!(total > 0, "Total free should be > 0, got {}", total);
    assert!(
        largest <= total,
        "Largest ({}) should be <= total ({})",
        largest,
        total
    );
}

#[test]
fn test_xms_allocate_and_free() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 64 KB
        0xBA, 0x40, 0x00,                   // MOV DX, 64
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (handle)
        // Free
        0x8B, 0x16, 0x02, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (1=success)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let alloc_ax = harness::result_word(&machine.bus, 0);
    let free_ax = harness::result_word(&machine.bus, 4);
    assert_eq!(alloc_ax, 1, "Allocate AX={}", alloc_ax);
    assert_eq!(free_ax, 1, "Free AX={}", free_ax);
}

#[test]
fn test_xms_allocate_decreases_free() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Query free before
        0xB4, 0x08,                         // MOV AH, 08h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x00, 0x01,             // MOV [0x0100], DX (total before)
        // Allocate 100 KB
        0xBA, 0x64, 0x00,                   // MOV DX, 100
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x04, 0x01,             // MOV [0x0104], DX (handle)
        // Query free after
        0xB4, 0x08,                         // MOV AH, 08h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (total after)
        // Free
        0x8B, 0x16, 0x04, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let before = harness::result_word(&machine.bus, 0);
    let after = harness::result_word(&machine.bus, 2);
    assert!(
        before > after,
        "Free should decrease: before={}, after={}",
        before,
        after
    );
    assert!(
        before - after >= 100,
        "Should decrease by at least 100 KB: before={}, after={}",
        before,
        after
    );
}

#[test]
fn test_xms_lock_and_unlock() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 64 KB
        0xBA, 0x40, 0x00,                   // MOV DX, 64
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x08, 0x01,             // MOV [0x0108], DX (handle)
        // Lock: AH=0Ch, DX=handle
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [handle]
        0xB4, 0x0C,                         // MOV AH, 0Ch
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (addr high)
        0x89, 0x1E, 0x04, 0x01,             // MOV [0x0104], BX (addr low)
        // Unlock: AH=0Dh, DX=handle
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [handle]
        0xB4, 0x0D,                         // MOV AH, 0Dh
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x06, 0x01,                   // MOV [0x0106], AX (1=success)
        // Free
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let lock_ax = harness::result_word(&machine.bus, 0);
    let addr_high = harness::result_word(&machine.bus, 2);
    let addr_low = harness::result_word(&machine.bus, 4);
    let unlock_ax = harness::result_word(&machine.bus, 6);
    let addr = ((addr_high as u32) << 16) | addr_low as u32;
    assert_eq!(lock_ax, 1, "Lock AX={}", lock_ax);
    assert!(
        addr >= 0x100000,
        "Lock address should be >= 1MB, got {:#010X}",
        addr
    );
    assert_eq!(unlock_ax, 1, "Unlock AX={}", unlock_ax);
}

#[test]
fn test_xms_handle_info() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 64 KB
        0xBA, 0x40, 0x00,                   // MOV DX, 64
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (handle)
        // Handle info: AH=0Eh, DX=handle
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x0E,                         // MOV AH, 0Eh
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (BH=lock, BL=free)
        0x89, 0x16, 0x04, 0x01,             // MOV [0x0104], DX (size_kb)
        // Free
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let info_ax = harness::result_word(&machine.bus, 0);
    let bx_val = harness::result_word(&machine.bus, 2);
    let size_kb = harness::result_word(&machine.bus, 4);
    let lock_count = (bx_val >> 8) as u8;
    assert_eq!(info_ax, 1, "Handle info AX={}", info_ax);
    assert_eq!(lock_count, 0, "Lock count should be 0, got {}", lock_count);
    assert_eq!(size_kb, 64, "Size should be 64 KB, got {}", size_kb);
}

#[test]
fn test_xms_reallocate() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 64 KB
        0xBA, 0x40, 0x00,                   // MOV DX, 64
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x08, 0x01,             // MOV [0x0108], DX (handle)
        // Reallocate to 128 KB: AH=0Fh, BX=new_size, DX=handle
        0xBB, 0x80, 0x00,                   // MOV BX, 128
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [handle]
        0xB4, 0x0F,                         // MOV AH, 0Fh
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        // Verify with handle info
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [handle]
        0xB4, 0x0E,                         // MOV AH, 0Eh
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (size_kb)
        // Free
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let realloc_ax = harness::result_word(&machine.bus, 0);
    let size_kb = harness::result_word(&machine.bus, 2);
    assert_eq!(realloc_ax, 1, "Reallocate AX={}", realloc_ax);
    assert_eq!(
        size_kb, 128,
        "Size should be 128 KB after realloc, got {}",
        size_kb
    );
}

#[test]
fn test_xms_request_hma() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBA, 0xFF, 0xFF,                   // MOV DX, FFFFh
        0xB4, 0x01,                         // MOV AH, 01h (request HMA)
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        // Release HMA so other tests are not affected
        0xB4, 0x02,                         // MOV AH, 02h
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ax = harness::result_word(&machine.bus, 0);
    assert_eq!(ax, 1, "Request HMA should succeed, AX={}", ax);
}

#[test]
fn test_xms_request_hma_twice_fails() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // First request
        0xBA, 0xFF, 0xFF,                   // MOV DX, FFFFh
        0xB4, 0x01,                         // MOV AH, 01h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        // Second request (should fail)
        0xBA, 0xFF, 0xFF,                   // MOV DX, FFFFh
        0xB4, 0x01,                         // MOV AH, 01h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX
        0x89, 0x1E, 0x04, 0x01,             // MOV [0x0104], BX (BL=error)
        // Release so we clean up
        0xB4, 0x02,                         // MOV AH, 02h
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let first_ax = harness::result_word(&machine.bus, 0);
    let second_ax = harness::result_word(&machine.bus, 2);
    let bl = harness::result_byte(&machine.bus, 4);
    assert_eq!(first_ax, 1, "First request should succeed, AX={}", first_ax);
    assert_eq!(second_ax, 0, "Second request should fail, AX={}", second_ax);
    assert_eq!(bl, 0x91, "Error code should be 0x91, got {:#04X}", bl);
}

#[test]
fn test_xms_release_hma() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Request HMA
        0xBA, 0xFF, 0xFF,                   // MOV DX, FFFFh
        0xB4, 0x01,                         // MOV AH, 01h
        0xCD, 0xFE,                         // INT FEh
        // Release HMA
        0xB4, 0x02,                         // MOV AH, 02h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ax = harness::result_word(&machine.bus, 0);
    assert_eq!(ax, 1, "Release HMA should succeed, AX={}", ax);
}

#[test]
fn test_hma_query_reflects_allocation() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Query HMA before allocation
        0xB8, 0x01, 0x4A,                   // MOV AX, 4A01h
        0xCD, 0x2F,                         // INT 2Fh
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (free before)
        // Request HMA
        0xBA, 0xFF, 0xFF,                   // MOV DX, FFFFh
        0xB4, 0x01,                         // MOV AH, 01h
        0xCD, 0xFE,                         // INT FEh
        // Query HMA after allocation
        0xB8, 0x01, 0x4A,                   // MOV AX, 4A01h
        0xCD, 0x2F,                         // INT 2Fh
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (free after)
        // Release HMA
        0xB4, 0x02,                         // MOV AH, 02h
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let before = harness::result_word(&machine.bus, 0);
    let after = harness::result_word(&machine.bus, 2);
    assert_eq!(
        before, 0xFFFF,
        "HMA should be free before, got {:#06X}",
        before
    );
    assert_eq!(
        after, 0x0000,
        "HMA should be allocated after, got {:#06X}",
        after
    );
}

#[test]
fn test_umb_allocate() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // UMB Allocate: AH=10h, DX=paragraphs
        0xBA, 0x10, 0x00,                   // MOV DX, 16
        0xB4, 0x10,                         // MOV AH, 10h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (segment)
        0x89, 0x16, 0x04, 0x01,             // MOV [0x0104], DX (actual size)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ax = harness::result_word(&machine.bus, 0);
    let segment = harness::result_word(&machine.bus, 2);
    let size = harness::result_word(&machine.bus, 4);
    assert_eq!(ax, 1, "UMB allocate AX={}", ax);
    assert!(
        segment >= 0xD000,
        "UMB segment should be >= D000, got {:#06X}",
        segment
    );
    assert!(size >= 16, "UMB size should be >= 16, got {}", size);
}

#[test]
fn test_umb_allocate_and_free() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // UMB Allocate
        0xBA, 0x10, 0x00,                   // MOV DX, 16
        0xB4, 0x10,                         // MOV AH, 10h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (segment)
        // UMB Free: AH=11h, DX=segment
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [segment]
        0xB4, 0x11,                         // MOV AH, 11h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (1=success)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let free_ax = harness::result_word(&machine.bus, 2);
    assert_eq!(free_ax, 1, "UMB free AX={}", free_ax);
}

#[test]
fn test_umb_allocate_too_large() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBA, 0xFF, 0xFF,                   // MOV DX, FFFFh
        0xB4, 0x10,                         // MOV AH, 10h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (0=fail)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (BL=error)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ax = harness::result_word(&machine.bus, 0);
    let bl = harness::result_byte(&machine.bus, 2);
    assert_eq!(ax, 0, "Should fail, AX={}", ax);
    assert_eq!(bl, 0xB0, "Error code should be 0xB0, got {:#04X}", bl);
}

#[test]
fn test_umb_reallocate() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 32 paragraphs
        0xBA, 0x20, 0x00,                   // MOV DX, 32
        0xB4, 0x10,                         // MOV AH, 10h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x1E, 0x00, 0x01,             // MOV [0x0100], BX (segment)
        // Reallocate to 16: AH=12h, BX=new_size, DX=segment
        0xBB, 0x10, 0x00,                   // MOV BX, 16
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [segment]
        0xB4, 0x12,                         // MOV AH, 12h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (1=success)
        // Free
        0x8B, 0x16, 0x00, 0x01,             // MOV DX, [segment]
        0xB4, 0x11,                         // MOV AH, 11h
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let realloc_ax = harness::result_word(&machine.bus, 2);
    assert_eq!(realloc_ax, 1, "UMB reallocate AX={}", realloc_ax);
}

#[test]
fn test_ems_and_xms_coexist() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate EMS: 1 page
        0xBB, 0x01, 0x00,                   // MOV BX, 1
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (EMS status)
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (EMS handle)
        // Allocate XMS: 64 KB
        0xBA, 0x40, 0x00,                   // MOV DX, 64
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (XMS status)
        0x89, 0x16, 0x08, 0x01,             // MOV [0x0108], DX (XMS handle)
        // Free XMS
        0x8B, 0x16, 0x08, 0x01,             // MOV DX, [XMS handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        // Free EMS
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [EMS handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (dealloc status)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ems_ah = harness::result_byte(&machine.bus, 1);
    let xms_ax = harness::result_word(&machine.bus, 2);
    let dealloc_ah = harness::result_byte(&machine.bus, 5);
    assert_eq!(
        ems_ah, 0x00,
        "EMS allocate should succeed, AH={:#04X}",
        ems_ah
    );
    assert_eq!(xms_ax, 1, "XMS allocate should succeed, AX={}", xms_ax);
    assert_eq!(
        dealloc_ah, 0x00,
        "EMS deallocate should succeed, AH={:#04X}",
        dealloc_ah
    );
}

#[test]
fn test_ems_and_xms_share_pool() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 700 EMS pages (11200 KB) to nearly exhaust the 12 MB pool
        0xBB, 0xBC, 0x02,                   // MOV BX, 700
        0xB4, 0x43,                         // MOV AH, 43h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (EMS status)
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (EMS handle)
        // Try to allocate 4000 KB via XMS (more than the remaining ~1088 KB)
        0xBA, 0xA0, 0x0F,                   // MOV DX, 4000
        0xB4, 0x09,                         // MOV AH, 09h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x02, 0x01,                   // MOV [0x0102], AX (XMS result: 0=fail)
        // Free EMS
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [EMS handle]
        0xB4, 0x45,                         // MOV AH, 45h
        0xCD, 0x67,                         // INT 67h
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ems_ah = harness::result_byte(&machine.bus, 1);
    let xms_ax = harness::result_word(&machine.bus, 2);
    assert_eq!(
        ems_ah, 0x00,
        "EMS allocate 700 pages should succeed, AH={:#04X}",
        ems_ah
    );
    assert_eq!(
        xms_ax, 0,
        "XMS allocate 4000 KB should fail (pool nearly exhausted by EMS), AX={}",
        xms_ax
    );
}

#[test]
fn test_ems_5b00_get_alternate_map_register_set() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x00, 0x5B,                   // MOV AX, 5B00h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    let bl = harness::result_byte(&machine.bus, 2);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
    assert_eq!(bl, 0x00, "BL should be 0 (software mode), got {:#04X}", bl);
}

#[test]
fn test_ems_5b02_get_alternate_map_save_array_size() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x02, 0x5B,                   // MOV AX, 5B02h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    let size = harness::result_word(&machine.bus, 2);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
    assert!(size > 0, "Save array size should be > 0, got {}", size);
}

#[test]
fn test_ems_5b03_allocate_alternate_map_register_set() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB8, 0x03, 0x5B,                   // MOV AX, 5B03h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    let bl = harness::result_byte(&machine.bus, 2);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
    assert_eq!(bl, 0x00, "BL should be 0 (software mode), got {:#04X}", bl);
}

#[test]
fn test_ems_5b01_set_alternate_map_register_set_invalid() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Try to set alternate register set 1 (not supported)
        0xBB, 0x01, 0x00,                   // MOV BX, 0001h (set 1)
        0xB8, 0x01, 0x5B,                   // MOV AX, 5B01h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    assert_eq!(
        ah, 0x9C,
        "AH should be 0x9C (not supported), got {:#04X}",
        ah
    );
}

#[test]
fn test_ems_5c_prepare_warm_boot() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x5C,                         // MOV AH, 5Ch
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
}

#[test]
fn test_ems_5d00_enable_os_function_set() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // First call: BX:CX = 0, should return access key
        0xBB, 0x00, 0x00,                   // MOV BX, 0
        0xB9, 0x00, 0x00,                   // MOV CX, 0
        0xB8, 0x00, 0x5D,                   // MOV AX, 5D00h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (key high)
        0x89, 0x0E, 0x04, 0x01,             // MOV [0x0104], CX (key low)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    let key_hi = harness::result_word(&machine.bus, 2);
    let key_lo = harness::result_word(&machine.bus, 4);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
    let key = ((key_hi as u32) << 16) | key_lo as u32;
    assert_ne!(key, 0, "Access key should be non-zero");
}

#[test]
fn test_ems_5d01_disable_os_function_set() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Enable first to get key
        0xBB, 0x00, 0x00,                   // MOV BX, 0
        0xB9, 0x00, 0x00,                   // MOV CX, 0
        0xB8, 0x00, 0x5D,                   // MOV AX, 5D00h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (key high)
        0x89, 0x0E, 0x04, 0x01,             // MOV [0x0104], CX (key low)
        // Disable using returned key
        0x8B, 0x1E, 0x02, 0x01,             // MOV BX, [key high]
        0x8B, 0x0E, 0x04, 0x01,             // MOV CX, [key low]
        0xB8, 0x01, 0x5D,                   // MOV AX, 5D01h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
}

#[test]
fn test_ems_5d02_return_os_access_key() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Enable first to get key
        0xBB, 0x00, 0x00,                   // MOV BX, 0
        0xB9, 0x00, 0x00,                   // MOV CX, 0
        0xB8, 0x00, 0x5D,                   // MOV AX, 5D00h
        0xCD, 0x67,                         // INT 67h
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (key high)
        0x89, 0x0E, 0x04, 0x01,             // MOV [0x0104], CX (key low)
        // Return key
        0x8B, 0x1E, 0x02, 0x01,             // MOV BX, [key high]
        0x8B, 0x0E, 0x04, 0x01,             // MOV CX, [key low]
        0xB8, 0x02, 0x5D,                   // MOV AX, 5D02h
        0xCD, 0x67,                         // INT 67h
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ah = harness::result_byte(&machine.bus, 1);
    assert_eq!(ah, 0x00, "AH should be 0 (success), got {:#04X}", ah);
}

#[test]
fn test_xms_88_query_any_free_extended_memory() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x88,                         // MOV AH, 88h
        0xCD, 0xFE,                         // INT FEh
        // Store EAX (largest free KB) as dword at [0x0100]
        0x66, 0xA3, 0x00, 0x01,             // MOV [0x0100], EAX
        // Store EDX (total free KB) as dword at [0x0104]
        0x66, 0x89, 0x16, 0x04, 0x01,       // MOV [0x0104], EDX
        // Store ECX (highest address) as dword at [0x0108]
        0x66, 0x89, 0x0E, 0x08, 0x01,       // MOV [0x0108], ECX
        // Store BL (error code) at [0x010C]
        0x88, 0x1E, 0x0C, 0x01,             // MOV [0x010C], BL
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let largest = harness::result_dword(&machine.bus, 0);
    let total = harness::result_dword(&machine.bus, 4);
    let highest = harness::result_dword(&machine.bus, 8);
    let bl = harness::result_byte(&machine.bus, 0x0C);
    assert_eq!(bl, 0x00, "BL should be 0 (no error), got {:#04X}", bl);
    assert!(
        largest > 0,
        "Largest free block should be > 0, got {}",
        largest
    );
    assert!(total > 0, "Total free should be > 0, got {}", total);
    assert!(
        highest >= 0x100000,
        "Highest address should be >= 1MB, got {:#X}",
        highest
    );
}

#[test]
fn test_xms_89_allocate_any_extended_memory() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Set EDX = 64 (allocate 64 KB)
        0x66, 0xBA, 0x40, 0x00, 0x00, 0x00, // MOV EDX, 64
        0xB4, 0x89,                         // MOV AH, 89h
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        0x89, 0x16, 0x02, 0x01,             // MOV [0x0102], DX (handle)
        // Free the handle
        0x8B, 0x16, 0x02, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x04, 0x01,                   // MOV [0x0104], AX (1=success)
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let alloc_ax = harness::result_word(&machine.bus, 0);
    let handle = harness::result_word(&machine.bus, 2);
    let free_ax = harness::result_word(&machine.bus, 4);
    assert_eq!(alloc_ax, 1, "Allocate should succeed, AX={}", alloc_ax);
    assert!(handle >= 1, "Handle should be >= 1, got {}", handle);
    assert_eq!(free_ax, 1, "Free should succeed, AX={}", free_ax);
}

#[test]
fn test_xms_8e_get_extended_handle_info() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 64 KB via 32-bit function
        0x66, 0xBA, 0x40, 0x00, 0x00, 0x00, // MOV EDX, 64
        0xB4, 0x89,                         // MOV AH, 89h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (handle)
        // Query handle info via 0x8E
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x8E,                         // MOV AH, 8Eh
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        0x89, 0x1E, 0x02, 0x01,             // MOV [0x0102], BX (BH=lock count)
        0x89, 0x0E, 0x04, 0x01,             // MOV [0x0104], CX (free handles)
        0x66, 0x89, 0x16, 0x08, 0x01,       // MOV [0x0108], EDX (size KB, 32-bit)
        // Clean up
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let ax = harness::result_word(&machine.bus, 0);
    let bh = harness::result_byte(&machine.bus, 3);
    let free_handles = harness::result_word(&machine.bus, 4);
    let size_kb = harness::result_dword(&machine.bus, 8);
    assert_eq!(ax, 1, "Query should succeed, AX={}", ax);
    assert_eq!(bh, 0, "Lock count should be 0, got {}", bh);
    assert!(
        free_handles > 0,
        "Free handles should be > 0, got {}",
        free_handles
    );
    assert_eq!(size_kb, 64, "Size should be 64 KB, got {}", size_kb);
}

#[test]
fn test_xms_8f_reallocate_any_extended_memory() {
    let mut machine = harness::boot_hle();
    #[rustfmt::skip]
    let code: &[u8] = &[
        // Allocate 64 KB via 32-bit function
        0x66, 0xBA, 0x40, 0x00, 0x00, 0x00, // MOV EDX, 64
        0xB4, 0x89,                         // MOV AH, 89h
        0xCD, 0xFE,                         // INT FEh
        0x89, 0x16, 0x06, 0x01,             // MOV [0x0106], DX (handle)
        // Reallocate to 128 KB via 0x8F (EBX = new size)
        0x66, 0xBB, 0x80, 0x00, 0x00, 0x00, // MOV EBX, 128
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x8F,                         // MOV AH, 8Fh
        0xCD, 0xFE,                         // INT FEh
        0xA3, 0x00, 0x01,                   // MOV [0x0100], AX (1=success)
        // Verify new size via 0x8E
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x8E,                         // MOV AH, 8Eh
        0xCD, 0xFE,                         // INT FEh
        0x66, 0x89, 0x16, 0x02, 0x01,       // MOV [0x0102], EDX (size KB)
        // Clean up
        0x8B, 0x16, 0x06, 0x01,             // MOV DX, [handle]
        0xB4, 0x0A,                         // MOV AH, 0Ah
        0xCD, 0xFE,                         // INT FEh
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    harness::inject_and_run(&mut machine, code);
    let realloc_ax = harness::result_word(&machine.bus, 0);
    let new_size = harness::result_dword(&machine.bus, 2);
    assert_eq!(
        realloc_ax, 1,
        "Reallocate should succeed, AX={}",
        realloc_ax
    );
    assert_eq!(new_size, 128, "New size should be 128 KB, got {}", new_size);
}
