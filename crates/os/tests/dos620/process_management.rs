//! Integration tests for process management (EXEC and terminate).

use common::Bus;

use crate::harness::*;

/// EXECs the given filename, reads flags and return code via INT 21h/4Dh,
/// and asserts CF=0 and AX matches `expected_return_ax`.
fn exec_file_and_check_return_code(
    machine: &mut machine::Pc9801Ra,
    filename: &[u8],
    expected_return_ax: u16,
) {
    let base = INJECT_CODE_BASE;
    let seg = INJECT_CODE_SEGMENT;

    write_bytes(&mut machine.bus, base + 0x0200, filename);
    machine.bus.write_byte(base + 0x0220, 0x00);
    machine.bus.write_byte(base + 0x0221, 0x0D);

    let seg_lo = (seg & 0xFF) as u8;
    let seg_hi = (seg >> 8) as u8;
    #[rustfmt::skip]
    let param_block: [u8; 14] = [
        0x00, 0x00,
        0x20, 0x02, seg_lo, seg_hi,
        0x30, 0x02, seg_lo, seg_hi,
        0x40, 0x02, seg_lo, seg_hi,
    ];
    write_bytes(&mut machine.bus, base + 0x0210, &param_block);

    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;
    const SEG_LO: u8 = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    const SEG_HI: u8 = (INJECT_CODE_SEGMENT >> 8) as u8;

    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBA, 0x00, 0x02,               // MOV DX, 0200h (filename)
        0xBB, 0x10, 0x02,               // MOV BX, 0210h (param block)
        0xB8, 0x00, 0x4B,               // MOV AX, 4B00h (EXEC)
        0xCD, 0x21,                     // INT 21h
        // EXEC destroys all registers except SS:SP; reload DS and ES.
        0x9C,                            // PUSHF
        0xB8, SEG_LO, SEG_HI,           // MOV AX, INJECT_CODE_SEGMENT
        0x8E, 0xD8,                     // MOV DS, AX
        0x8E, 0xC0,                     // MOV ES, AX
        0x58,                           // POP AX (saved flags)
        0x89, 0x06, RES_LO, RES_HI,     // MOV [result+0], AX (flags)
        0xB4, 0x4D,                     // MOV AH, 4Dh
        0xCD, 0x21,                     // INT 21h
        0x89, 0x06, RES_LO + 2, RES_HI, // MOV [result+2], AX
        0xFA,                           // CLI
        0xF4,                           // HLT
    ];

    inject_and_run_with_budget(machine, code, INJECT_BUDGET_DISK_IO);

    let flags = result_word(&machine.bus, 0);
    assert_eq!(
        flags & 0x0001,
        0,
        "EXEC should return with CF=0, flags={:#06X}",
        flags
    );

    let return_ax = result_word(&machine.bus, 2);
    assert_eq!(return_ax, expected_return_ax, "unexpected return code");
}

/// EXEC a .COM file on the test floppy and verify the child terminates
/// with the expected return code.
#[test]
fn exec_com_and_get_return_code() {
    let mut machine = boot_hle_with_floppy();

    let base = INJECT_CODE_BASE;
    let seg = INJECT_CODE_SEGMENT;

    // Filename at +0x0200
    write_bytes(&mut machine.bus, base + 0x0200, b"A:\\TEST.COM\x00");

    // Command tail at +0x0220: length=0, CR
    machine.bus.write_byte(base + 0x0220, 0x00);
    machine.bus.write_byte(base + 0x0221, 0x0D);

    // EXEC parameter block at +0x0210
    let seg_lo = (seg & 0xFF) as u8;
    let seg_hi = (seg >> 8) as u8;
    #[rustfmt::skip]
    let param_block: [u8; 14] = [
        0x00, 0x00,                     // env_seg = 0 (inherit)
        0x20, 0x02, seg_lo, seg_hi,     // cmd_tail = seg:0220
        0x30, 0x02, seg_lo, seg_hi,     // fcb1 = seg:0230
        0x40, 0x02, seg_lo, seg_hi,     // fcb2 = seg:0240
    ];
    write_bytes(&mut machine.bus, base + 0x0210, &param_block);

    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;

    // Injected code:
    // 1. EXEC A:\TEST.COM (child exits with code 0x42)
    // 2. Restore DS/ES (EXEC destroys all registers except SS:SP)
    // 3. Save carry flag
    // 4. Get return code via INT 21h/4Dh
    // 5. Get current PSP via INT 21h/62h
    // 6. CLI + HLT
    const SEG_LO: u8 = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    const SEG_HI: u8 = (INJECT_CODE_SEGMENT >> 8) as u8;
    #[rustfmt::skip]
    let code: &[u8] = &[
        // DS and ES already set to INJECT_CODE_SEGMENT by inject_and_run
        0xBA, 0x00, 0x02,               // MOV DX, 0200h (filename)
        0xBB, 0x10, 0x02,               // MOV BX, 0210h (param block)
        0xB8, 0x00, 0x4B,               // MOV AX, 4B00h (EXEC)
        0xCD, 0x21,                     // INT 21h
        // After child terminates, we return here.
        // EXEC destroys all registers except SS:SP; reload DS and ES.
        0x9C,                           // PUSHF (save CF from EXEC result)
        0xB8, SEG_LO, SEG_HI,           // MOV AX, INJECT_CODE_SEGMENT
        0x8E, 0xD8,                     // MOV DS, AX
        0x8E, 0xC0,                     // MOV ES, AX
        0x58,                           // POP AX (get saved flags)
        0x89, 0x06, RES_LO, RES_HI,     // MOV [result+0], AX (flags)
        // Get return code
        0xB4, 0x4D,                     // MOV AH, 4Dh
        0xCD, 0x21,                     // INT 21h
        0x89, 0x06, RES_LO + 2, RES_HI, // MOV [result+2], AX
        // Get current PSP
        0xB4, 0x62,                     // MOV AH, 62h
        0xCD, 0x21,                     // INT 21h
        0x89, 0x1E, RES_LO + 4, RES_HI, // MOV [result+4], BX
        0xFA,                           // CLI
        0xF4,                           // HLT
    ];

    inject_and_run_with_budget(&mut machine, code, INJECT_BUDGET_DISK_IO);

    // CF should be clear (success)
    let flags = result_word(&machine.bus, 0);
    assert_eq!(
        flags & 0x0001,
        0,
        "EXEC should return with CF=0 on success, flags={:#06X}",
        flags
    );

    // Return code = 0x42, termination type = 0x00
    let return_ax = result_word(&machine.bus, 2);
    assert_eq!(
        return_ax, 0x0042,
        "INT 21h/4Dh should return AX=0042h (code=42h, type=00h), got {:#06X}",
        return_ax
    );

    // Current PSP should be restored to parent
    let psp_after = result_word(&machine.bus, 4);
    assert_ne!(psp_after, 0, "current PSP should be restored to parent");
}

/// After EXEC + terminate, the child's MCB should be freed and coalesced.
#[test]
fn terminate_frees_child_memory_and_coalesces() {
    let mut machine = boot_hle_with_floppy();

    let base = INJECT_CODE_BASE;
    let seg = INJECT_CODE_SEGMENT;

    write_bytes(&mut machine.bus, base + 0x0200, b"A:\\TEST.COM\x00");
    machine.bus.write_byte(base + 0x0220, 0x00);
    machine.bus.write_byte(base + 0x0221, 0x0D);

    let seg_lo = (seg & 0xFF) as u8;
    let seg_hi = (seg >> 8) as u8;
    #[rustfmt::skip]
    let param_block: [u8; 14] = [
        0x00, 0x00,
        0x20, 0x02, seg_lo, seg_hi,
        0x30, 0x02, seg_lo, seg_hi,
        0x40, 0x02, seg_lo, seg_hi,
    ];
    write_bytes(&mut machine.bus, base + 0x0210, &param_block);

    // Get first MCB before EXEC
    let sysvars = get_sysvars_address(&mut machine);
    let first_mcb_segment = read_word(&machine.bus, sysvars - 2);
    let blocks_before = walk_mcb_chain(&machine.bus, first_mcb_segment);

    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;

    const SEG_LO2: u8 = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    const SEG_HI2: u8 = (INJECT_CODE_SEGMENT >> 8) as u8;
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBA, 0x00, 0x02,                      // MOV DX, 0200h
        0xBB, 0x10, 0x02,                      // MOV BX, 0210h
        0xB8, 0x00, 0x4B,                      // MOV AX, 4B00h
        0xCD, 0x21,                            // INT 21h
        // Restore DS after EXEC (EXEC destroys all registers except SS:SP)
        0xB8, SEG_LO2, SEG_HI2,                // MOV AX, INJECT_CODE_SEGMENT
        0x8E, 0xD8,                            // MOV DS, AX
        0xC6, 0x06, RES_LO, RES_HI, 0x01,      // MOV BYTE [result], 01h
        0xFA,                                  // CLI
        0xF4,                                  // HLT
    ];

    inject_and_run_with_budget(&mut machine, code, INJECT_BUDGET_DISK_IO);

    assert_eq!(
        result_byte(&machine.bus, 0),
        0x01,
        "EXEC should have returned to parent"
    );

    // Walk MCB chain after terminate
    let blocks_after = walk_mcb_chain(&machine.bus, first_mcb_segment);

    // Free memory should be >= before (child blocks freed and coalesced)
    let free_before: u32 = blocks_before
        .iter()
        .filter(|b| b.owner == 0)
        .map(|b| b.size as u32)
        .sum();
    let free_after: u32 = blocks_after
        .iter()
        .filter(|b| b.owner == 0)
        .map(|b| b.size as u32)
        .sum();
    assert!(
        free_after >= free_before,
        "free memory after terminate ({}) should be >= before ({})",
        free_after,
        free_before
    );

    // MCB chain should end with Z block
    assert_eq!(
        blocks_after.last().map(|b| b.block_type),
        Some(0x5A),
        "MCB chain should end with Z block"
    );
}

/// TSR termination keeps the process MCB resident.
///
/// Replaces TEST.COM on the floppy with a TSR program, then EXECs it.
/// The TSR program keeps 32 paragraphs resident (DX=0x0020).
#[test]
fn tsr_keeps_memory_resident() {
    let mut machine = boot_hle_with_floppy();

    let base = INJECT_CODE_BASE;
    let seg = INJECT_CODE_SEGMENT;

    // Replace the floppy with one containing a TSR .COM program.
    // The TSR .COM keeps 32 paragraphs resident (DX=0x0020).
    let tsr_com: &[u8] = &[
        0xBA, 0x20, 0x00, // MOV DX, 0020h (keep 32 paragraphs)
        0xB8, 0x00, 0x31, // MOV AX, 3100h (TSR, exit code 0)
        0xCD, 0x21, // INT 21h
    ];
    machine.bus.eject_floppy(0);
    let floppy = create_test_floppy_with_program(b"TEST    COM", tsr_com);
    machine.bus.insert_floppy(0, floppy, None);

    // Set up EXEC parameter block
    write_bytes(&mut machine.bus, base + 0x0200, b"A:\\TEST.COM\x00");
    machine.bus.write_byte(base + 0x0220, 0x00);
    machine.bus.write_byte(base + 0x0221, 0x0D);

    let seg_lo = (seg & 0xFF) as u8;
    let seg_hi = (seg >> 8) as u8;
    #[rustfmt::skip]
    let param_block: [u8; 14] = [
        0x00, 0x00,
        0x20, 0x02, seg_lo, seg_hi,
        0x30, 0x02, seg_lo, seg_hi,
        0x40, 0x02, seg_lo, seg_hi,
    ];
    write_bytes(&mut machine.bus, base + 0x0210, &param_block);

    // Get first MCB before EXEC
    let sysvars = get_sysvars_address(&mut machine);
    let first_mcb_segment = read_word(&machine.bus, sysvars - 2);

    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;
    const SEG_LO3: u8 = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    const SEG_HI3: u8 = (INJECT_CODE_SEGMENT >> 8) as u8;

    #[rustfmt::skip]
    let code: &[u8] = &[
        0xBA, 0x00, 0x02,                      // MOV DX, 0200h (filename)
        0xBB, 0x10, 0x02,                      // MOV BX, 0210h (param block)
        0xB8, 0x00, 0x4B,                      // MOV AX, 4B00h (EXEC)
        0xCD, 0x21,                            // INT 21h
        // Restore DS after EXEC
        0xB8, SEG_LO3, SEG_HI3,                // MOV AX, INJECT_CODE_SEGMENT
        0x8E, 0xD8,                            // MOV DS, AX
        0xC6, 0x06, RES_LO, RES_HI, 0x01,      // MOV BYTE [result+0], 01h
        0xFA,                                  // CLI
        0xF4,                                  // HLT
    ];

    inject_and_run_with_budget(&mut machine, code, INJECT_BUDGET_DISK_IO);

    assert_eq!(
        result_byte(&machine.bus, 0),
        0x01,
        "TSR should have returned to parent"
    );

    // Walk MCB chain after TSR
    let blocks_after = walk_mcb_chain(&machine.bus, first_mcb_segment);

    // Find the child's PSP block. The EXEC allocated the largest block,
    // so the child PSP should be the block right after COMMAND.COM.
    // After TSR, the child's MCB should still be owned (not freed) but resized.
    let tsr_block = blocks_after
        .iter()
        .find(|b| b.owner != 0 && b.owner != 0x0008 && b.size <= 0x0021);
    assert!(
        tsr_block.is_some(),
        "TSR block should exist and be resized to ~32 paragraphs"
    );
    let tsr_block = tsr_block.unwrap();
    assert!(
        tsr_block.size <= 0x0021,
        "TSR block should be resized to ~32 paragraphs, got {}",
        tsr_block.size
    );

    // MCB chain should end with Z block
    assert_eq!(
        blocks_after.last().map(|b| b.block_type),
        Some(0x5A),
        "MCB chain should end with Z block"
    );
}

/// Build a minimal MZ EXE that executes `code_bytes` with the given stack size.
/// init_cs and init_ip are relative to the load segment (image base).
fn build_exe(code_bytes: &[u8], init_cs: u16, init_ip: u16, stack_size: u16) -> Vec<u8> {
    let header_paragraphs: u16 = 2; // 32 bytes = 2 paragraphs
    let header_size = (header_paragraphs as usize) * 16;
    let image_size = code_bytes.len() + stack_size as usize;
    let file_size = header_size + image_size;
    let total_pages = file_size.div_ceil(512) as u16;
    let bytes_last_page = (file_size % 512) as u16;
    // SS:SP relative to load segment. Put stack after code.
    let init_ss: u16 = 0;
    let init_sp = (code_bytes.len() as u16) + stack_size;

    let mut exe = vec![0u8; file_size];
    // MZ header
    exe[0] = 0x4D; // 'M'
    exe[1] = 0x5A; // 'Z'
    exe[2..4].copy_from_slice(&bytes_last_page.to_le_bytes());
    exe[4..6].copy_from_slice(&total_pages.to_le_bytes());
    exe[6..8].copy_from_slice(&0u16.to_le_bytes()); // reloc_count = 0
    exe[8..10].copy_from_slice(&header_paragraphs.to_le_bytes());
    exe[10..12].copy_from_slice(&0u16.to_le_bytes()); // min_alloc
    exe[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes()); // max_alloc
    exe[14..16].copy_from_slice(&init_ss.to_le_bytes());
    exe[16..18].copy_from_slice(&init_sp.to_le_bytes());
    // checksum at [18..20] = 0
    exe[20..22].copy_from_slice(&init_ip.to_le_bytes());
    exe[22..24].copy_from_slice(&init_cs.to_le_bytes());
    exe[24..26].copy_from_slice(&(header_size as u16).to_le_bytes()); // reloc_table_offset
    // Code image
    exe[header_size..header_size + code_bytes.len()].copy_from_slice(code_bytes);
    exe
}

/// EXEC a .EXE file and verify the child terminates with the expected return code.
#[test]
fn exec_exe_and_get_return_code() {
    // Build a minimal MZ EXE: MOV AH,4Ch; MOV AL,42h; INT 21h
    let code: &[u8] = &[0xB4, 0x4C, 0xB0, 0x42, 0xCD, 0x21];
    let exe_data = build_exe(code, 0, 0, 256);

    let mut machine = boot_hle_with_floppy();
    machine.bus.eject_floppy(0);
    let floppy = create_test_floppy_with_program(b"TEST    EXE", &exe_data);
    machine.bus.insert_floppy(0, floppy, None);

    exec_file_and_check_return_code(&mut machine, b"A:\\TEST.EXE\x00", 0x0042);
}

/// EXEC a .EXE with a relocation entry and verify it runs correctly.
#[test]
fn exec_exe_with_relocation() {
    // Build an EXE where the code references a segment that needs relocation.
    // The code does: MOV AX, [relocated_seg]:0000 ; then exits with that value.
    //
    // Layout (relative to load segment):
    //   CS:0000 = code
    //   Segment 0x0001:0000 = data word (0x0042)
    //
    // Code:
    //   MOV AX, 0001h        ; segment to be relocated -> becomes load_seg+1
    //   MOV DS, AX
    //   MOV AL, [0000h]      ; read byte from relocated segment
    //   MOV AH, 4Ch
    //   INT 21h
    let code: &[u8] = &[
        0xB8, 0x01, 0x00, // MOV AX, 0001h (to be relocated)
        0x8E, 0xD8, // MOV DS, AX
        0xA0, 0x00, 0x00, // MOV AL, [0000h]
        0xB4, 0x4C, // MOV AH, 4Ch
        0xCD, 0x21, // INT 21h
    ];
    let header_paragraphs: u16 = 2;
    let header_size = (header_paragraphs as usize) * 16;
    // Image: code at offset 0, data at offset 16 (segment 0x0001 relative)
    let image_size = 16 + 16 + 256; // code paragraph + data paragraph + stack
    let file_size = header_size + image_size;
    let total_pages = file_size.div_ceil(512) as u16;
    let bytes_last_page = (file_size % 512) as u16;

    let mut exe = vec![0u8; file_size];
    exe[0] = 0x4D;
    exe[1] = 0x5A;
    exe[2..4].copy_from_slice(&bytes_last_page.to_le_bytes());
    exe[4..6].copy_from_slice(&total_pages.to_le_bytes());
    exe[6..8].copy_from_slice(&1u16.to_le_bytes()); // 1 relocation
    exe[8..10].copy_from_slice(&header_paragraphs.to_le_bytes());
    exe[10..12].copy_from_slice(&0u16.to_le_bytes()); // min_alloc
    exe[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes()); // max_alloc
    exe[14..16].copy_from_slice(&0u16.to_le_bytes()); // init_ss = 0
    exe[16..18].copy_from_slice(&(image_size as u16).to_le_bytes()); // init_sp
    exe[20..22].copy_from_slice(&0u16.to_le_bytes()); // init_ip = 0
    exe[22..24].copy_from_slice(&0u16.to_le_bytes()); // init_cs = 0
    exe[24..26].copy_from_slice(&(header_size as u16).to_le_bytes()); // reloc offset
    // Relocation table at offset 28 (inside header padding after the 28-byte fields)
    exe[24..26].copy_from_slice(&28u16.to_le_bytes()); // reloc_table_offset = 28
    exe[28] = 0x01; // reloc offset low (byte 1 of MOV AX, imm16)
    exe[29] = 0x00; // reloc offset high
    exe[30] = 0x00; // reloc segment low
    exe[31] = 0x00; // reloc segment high

    // Image at file offset 32
    exe[header_size..header_size + code.len()].copy_from_slice(code);
    // Data at image offset 16 (= segment 0x0001 * 16 relative to load base)
    exe[header_size + 16] = 0x42; // The byte the code reads

    let mut machine = boot_hle_with_floppy();
    machine.bus.eject_floppy(0);
    let floppy = create_test_floppy_with_program(b"TEST    EXE", &exe);
    machine.bus.insert_floppy(0, floppy, None);

    exec_file_and_check_return_code(&mut machine, b"A:\\TEST.EXE\x00", 0x0042);
}

struct McbInfo {
    owner: u16,
    size: u16,
    block_type: u8,
}

fn walk_mcb_chain(bus: &machine::Pc9801Bus, first_mcb: u16) -> Vec<McbInfo> {
    let mut blocks = Vec::new();
    let mut current = first_mcb;
    for _ in 0..4096 {
        let addr = (current as u32) << 4;
        let block_type = bus.read_byte_direct(addr);
        if block_type != 0x4D && block_type != 0x5A {
            break;
        }
        let owner = read_word(bus, addr + 1);
        let size = read_word(bus, addr + 3);
        blocks.push(McbInfo {
            owner,
            size,
            block_type,
        });
        if block_type == 0x5A {
            break;
        }
        current = current + size + 1;
    }
    blocks
}
