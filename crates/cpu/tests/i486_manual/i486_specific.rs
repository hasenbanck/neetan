//! 486-specific instruction tests derived from the 80486 PRM.
//!
//! Covers BSWAP, CMPXCHG, XADD, INVLPG, WBINVD, INVD plus the 386 #UD
//! coverage for each, the CR0 reserved-bit masking that distinguishes the
//! 386 (PE/MP/EM/TS/ET/PG only) from the 486 (additionally NE/WP/AM/NW/CD),
//! and the 486 #AC alignment-check four-corner matrix.

use common::Cpu as _;

use super::setup::{
    HANDLER_ALIGNMENT_CHECK_IP, HANDLER_GENERAL_PROTECTION_IP, HANDLER_INVALID_OPCODE_IP,
    RING0_CODE_BASE, RING3_CODE_BASE, SHARED_DATA_BASE, TestBus, make_cpu_386, make_cpu_486,
    place_at, promote_to_ring3, setup_protected_mode, setup_protected_mode_with_handlers,
};

const REG_INDEX_EAX: u8 = 0;
const REG_INDEX_ECX: u8 = 1;
const REG_INDEX_EDX: u8 = 2;
const REG_INDEX_EBX: u8 = 3;

const HALT_OPCODE: u8 = 0xF4;

fn modrm_register(reg_index: u8, rm_index: u8) -> u8 {
    0xC0 | ((reg_index & 7) << 3) | (rm_index & 7)
}

fn modrm_memory_disp16(reg_index: u8, displacement: u16, code: &mut Vec<u8>) {
    code.push(0x06 | ((reg_index & 7) << 3));
    code.push(displacement as u8);
    code.push((displacement >> 8) as u8);
}

#[test]
fn bswap_eax_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x1122_3344);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xC8]);

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0x4433_2211);
}

#[test]
fn bswap_ecx_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_ecx(0xAABB_CCDD);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xC9]);

    cpu.step(&mut bus);

    assert_eq!(cpu.ecx(), 0xDDCC_BBAA);
}

#[test]
fn bswap_edx_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_edx(0xDEAD_BEEF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xCA]);

    cpu.step(&mut bus);

    assert_eq!(cpu.edx(), 0xEFBE_ADDE);
}

#[test]
fn bswap_ebx_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_ebx(0x0102_0304);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xCB]);

    cpu.step(&mut bus);

    assert_eq!(cpu.ebx(), 0x0403_0201);
}

#[test]
fn bswap_esp_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0x1234_5678);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xCC]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.esp(), 0x7856_3412);
}

#[test]
fn bswap_ebp_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_ebp(0xCAFE_BABE);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xCD]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.ebp(), 0xBEBA_FECA);
}

#[test]
fn bswap_esi_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esi(0xFEDC_BA98);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xCE]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.esi(), 0x98BA_DCFE);
}

#[test]
fn bswap_edi_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_edi(0x8765_4321);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xCF]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.edi(), 0x2143_6587);
}

#[test]
fn bswap_zero_is_identity_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xC8]);

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0);
}

#[test]
fn bswap_palindrome_is_identity_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0xAA55_55AA);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xC8]);

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0xAA55_55AA);
}

#[test]
fn bswap_does_not_affect_flags_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.carry_val = 1;
    state.flags.zero_val = 0;
    state.flags.sign_val = -1;
    state.flags.overflow_val = 1;
    state.flags.parity_val = 0xAB;
    cpu.load_state(&state);
    cpu.set_eax(0x1122_3344);
    let flags_before = cpu.state.flags.compress();

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xC8]);

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0x4433_2211);
    assert_eq!(cpu.state.flags.compress(), flags_before);
}

#[test]
fn bswap_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x0F, 0xC8]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn cmpxchg_byte_equal_replaces_destination_and_sets_zf_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x42);
    cpu.set_ecx(0x99);

    // CMPXCHG CL, CL: r/m=CL, r=CL, AL=0x42, CL=0x99 (not equal).
    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0xB0, 0xC9]);

    cpu.step(&mut bus);

    assert_eq!(
        cpu.eax() & 0xFF,
        0x99,
        "AL must take destination on mismatch"
    );
    assert!(!cpu.state.flags.zf());
}

#[test]
fn cmpxchg_byte_match_writes_source_to_destination_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x42;
    cpu.set_eax(0x42);
    cpu.set_ecx(0xAB);

    // CMPXCHG [0x100], CL with CL as r and disp16 as r/m.
    let mut code: Vec<u8> = vec![0x0F, 0xB0];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize],
        0xAB
    );
    assert_eq!(cpu.eax() & 0xFF, 0x42, "AL must remain unchanged on match");
}

#[test]
fn cmpxchg_byte_mismatch_loads_destination_into_al_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x77;
    cpu.set_eax(0x42);
    cpu.set_ecx(0xAB);

    let mut code: Vec<u8> = vec![0x0F, 0xB0];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.zf());
    assert_eq!(
        cpu.eax() & 0xFF,
        0x77,
        "AL must take destination on mismatch"
    );
    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize],
        0x77,
        "memory destination must remain unchanged on mismatch"
    );
}

#[test]
fn cmpxchg_word_match_writes_source_to_destination_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x34;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32 + 1) as usize] = 0x12;
    cpu.set_eax(0x1234);
    cpu.set_ecx(0xBEEF);

    let mut code: Vec<u8> = vec![0x0F, 0xB1];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize],
        0xEF
    );
    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32 + 1) as usize],
        0xBE
    );
}

#[test]
fn cmpxchg_word_mismatch_loads_destination_into_ax_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0xAA;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32 + 1) as usize] = 0xBB;
    cpu.set_eax(0x1234);
    cpu.set_ecx(0xBEEF);

    let mut code: Vec<u8> = vec![0x0F, 0xB1];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.zf());
    assert_eq!(cpu.eax() & 0xFFFF, 0xBBAA);
}

#[test]
fn cmpxchg_dword_match_writes_source_to_destination_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    let memory_address = SHARED_DATA_BASE + memory_offset as u32;
    bus.ram[memory_address as usize] = 0x78;
    bus.ram[memory_address as usize + 1] = 0x56;
    bus.ram[memory_address as usize + 2] = 0x34;
    bus.ram[memory_address as usize + 3] = 0x12;
    cpu.set_eax(0x1234_5678);
    cpu.set_ecx(0xCAFE_BABE);

    // 66 0F B1 [modr/m] -- 32-bit operand size CMPXCHG.
    let mut code: Vec<u8> = vec![0x66, 0x0F, 0xB1];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(bus.ram[memory_address as usize], 0xBE);
    assert_eq!(bus.ram[memory_address as usize + 1], 0xBA);
    assert_eq!(bus.ram[memory_address as usize + 2], 0xFE);
    assert_eq!(bus.ram[memory_address as usize + 3], 0xCA);
}

#[test]
fn cmpxchg_dword_mismatch_loads_destination_into_eax_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    let memory_address = SHARED_DATA_BASE + memory_offset as u32;
    bus.ram[memory_address as usize] = 0xEF;
    bus.ram[memory_address as usize + 1] = 0xBE;
    bus.ram[memory_address as usize + 2] = 0xAD;
    bus.ram[memory_address as usize + 3] = 0xDE;
    cpu.set_eax(0x1234_5678);
    cpu.set_ecx(0xCAFE_BABE);

    let mut code: Vec<u8> = vec![0x66, 0x0F, 0xB1];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.zf());
    assert_eq!(cpu.eax(), 0xDEAD_BEEF);
}

#[test]
fn cmpxchg_byte_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0xB0, 0xC9]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn cmpxchg_word_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0xB1, 0xC9]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn cmpxchg_lock_prefix_does_not_fault_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x42);
    cpu.set_ecx(0x42);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x42;

    // LOCK CMPXCHG [0x100], CL.
    let mut code: Vec<u8> = vec![0xF0, 0x0F, 0xB0];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize],
        0x42
    );
}

#[test]
fn xadd_byte_swaps_and_adds_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x10);
    cpu.set_ecx(0x20);

    // XADD AL, CL: r/m=AL, r=CL.  After: AL = 0x10+0x20 = 0x30, CL = 0x10.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC0, modrm_register(REG_INDEX_ECX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFF, 0x30);
    assert_eq!(cpu.ecx() & 0xFF, 0x10);
    assert!(!cpu.state.flags.cf());
}

#[test]
fn xadd_byte_sets_carry_on_overflow_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0xFF);
    cpu.set_ecx(0x01);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC0, modrm_register(REG_INDEX_ECX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFF, 0x00);
    assert_eq!(cpu.ecx() & 0xFF, 0xFF);
    assert!(cpu.state.flags.cf());
    assert!(cpu.state.flags.zf());
}

#[test]
fn xadd_word_swaps_and_adds_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x1000);
    cpu.set_ecx(0x2000);

    // XADD AX, CX (operand size = 16, default in 16-bit code segment).
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC1, modrm_register(REG_INDEX_ECX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFFFF, 0x3000);
    assert_eq!(cpu.ecx() & 0xFFFF, 0x1000);
}

#[test]
fn xadd_word_to_memory_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x34;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32 + 1) as usize] = 0x12;
    cpu.set_ecx(0x4321);

    let mut code: Vec<u8> = vec![0x0F, 0xC1];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert_eq!(
        super::setup::read_word_at(&bus, SHARED_DATA_BASE + memory_offset as u32),
        0x5555
    );
    assert_eq!(cpu.ecx() & 0xFFFF, 0x1234);
}

#[test]
fn xadd_dword_swaps_and_adds_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x0001_0000);
    cpu.set_ecx(0x0002_0000);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x66,
            0x0F,
            0xC1,
            modrm_register(REG_INDEX_ECX, REG_INDEX_EAX),
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0x0003_0000);
    assert_eq!(cpu.ecx(), 0x0001_0000);
}

#[test]
fn xadd_byte_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC0, modrm_register(REG_INDEX_ECX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn xadd_word_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC1, modrm_register(REG_INDEX_ECX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn xadd_aliased_register_doubles_value_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x7);

    // XADD AL, AL: temp = AL+AL = 0x0E; AL = AL = 0x7; AL = 0x0E.
    // Per 80486 PRM the source operand is loaded with the original
    // destination, then the destination receives the sum.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC0, modrm_register(REG_INDEX_EAX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFF, 0x0E);
}

#[test]
fn invlpg_at_ring0_invalidates_tlb_entry_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    super::setup::enable_identity_paging(&mut bus, &mut state);
    cpu.load_state(&state);

    // Force a memory access first to populate the TLB at linear address
    // SHARED_DATA_BASE.
    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0xCD;

    // MOV AL, [0x100] - first access primes the TLB.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0xA0,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
            // INVLPG [0x100]
            0x0F,
            0x01,
            0x3E,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
        ],
    );

    cpu.step(&mut bus);
    // The first instruction (MOV AL, [0x100]) primes the TLB; we then
    // change the PTE to non-present underneath, run INVLPG, and verify
    // the next access through that linear address takes a #PF.
    let memory_linear = SHARED_DATA_BASE + memory_offset as u32;
    super::setup::set_identity_page_flags(&mut bus, memory_linear, 0);

    cpu.step(&mut bus);

    // After INVLPG, perform another access through the same linear address
    // and confirm it now traps via #PF.
    let probe_offset: u16 = 0x101;
    place_at(
        &mut bus,
        RING0_CODE_BASE + cpu.ip(),
        &[0xA0, probe_offset as u8, (probe_offset >> 8) as u8],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(
        cpu.ip(),
        super::setup::HANDLER_PAGE_FAULT_IP as u32 + 1,
        "after INVLPG, the previously cached translation is invalidated"
    );
}

#[test]
fn invlpg_at_ring3_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    place_at(
        &mut bus,
        super::setup::RING3_CODE_BASE,
        &[
            0x0F,
            0x01,
            0x3E,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
        ],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn invlpg_register_form_raises_invalid_opcode_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    // 0F 01 F8 - INVLPG with mod=11 (register form) is not encodable.
    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x01, 0xF8]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn invlpg_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x01, 0x3E, 0x00, 0x01]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn wbinvd_at_ring0_succeeds_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x09, HALT_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted(), "WBINVD followed by HLT must reach HLT");
    assert_eq!(cpu.ip(), 3, "after WBINVD then HLT, IP advances past HLT");
}

#[test]
fn wbinvd_at_ring3_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0x0F, 0x09]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn wbinvd_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x09]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn invd_at_ring0_succeeds_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x08, HALT_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted(), "INVD followed by HLT must reach HLT");
    assert_eq!(cpu.ip(), 3);
}

#[test]
fn invd_at_ring3_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0x0F, 0x08]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn invd_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x08]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

// CR0 reserved-bit masking. Per 80486 PRM Section 9.2.1, the 486 introduces
// the WP, NE, AM, NW, and CD writable bits. The 386 ignores writes to these
// positions; the 486 retains them.

const CR0_BIT_PE: u32 = 1 << 0;
const CR0_BIT_MP: u32 = 1 << 1;
const CR0_BIT_EM: u32 = 1 << 2;
const CR0_BIT_TS: u32 = 1 << 3;
const CR0_BIT_ET: u32 = 1 << 4;
const CR0_BIT_NE: u32 = 1 << 5;
const CR0_BIT_WP: u32 = 1 << 16;
const CR0_BIT_AM: u32 = 1 << 18;
const CR0_BIT_NW: u32 = 1 << 29;
const CR0_BIT_CD: u32 = 1 << 30;
const CR0_BIT_PG: u32 = 1 << 31;

const CR0_MASK_386_WRITABLE: u32 =
    CR0_BIT_PE | CR0_BIT_MP | CR0_BIT_EM | CR0_BIT_TS | CR0_BIT_ET | CR0_BIT_PG;
const CR0_MASK_486_WRITABLE: u32 =
    CR0_MASK_386_WRITABLE | CR0_BIT_NE | CR0_BIT_WP | CR0_BIT_AM | CR0_BIT_NW | CR0_BIT_CD;

#[test]
fn mov_cr0_on_386_drops_486_only_bits() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    // EAX = all ones except PG (kept clear so we don't enable paging).
    cpu.set_eax(!CR0_BIT_PG);

    // MOV CR0, EAX
    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x22, 0xC0]);

    cpu.step(&mut bus);

    let expected = !CR0_BIT_PG & CR0_MASK_386_WRITABLE;
    assert_eq!(cpu.state.cr0, expected);
    assert_eq!(cpu.state.cr0 & CR0_BIT_WP, 0);
    assert_eq!(cpu.state.cr0 & CR0_BIT_NE, 0);
    assert_eq!(cpu.state.cr0 & CR0_BIT_AM, 0);
    assert_eq!(cpu.state.cr0 & CR0_BIT_NW, 0);
    assert_eq!(cpu.state.cr0 & CR0_BIT_CD, 0);
}

#[test]
fn mov_cr0_on_486_keeps_wp_ne_am_nw_cd() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(!CR0_BIT_PG);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x22, 0xC0]);

    cpu.step(&mut bus);

    let expected = !CR0_BIT_PG & CR0_MASK_486_WRITABLE;
    assert_eq!(cpu.state.cr0, expected);
    assert!(cpu.state.cr0 & CR0_BIT_WP != 0);
    assert!(cpu.state.cr0 & CR0_BIT_NE != 0);
    assert!(cpu.state.cr0 & CR0_BIT_AM != 0);
    assert!(cpu.state.cr0 & CR0_BIT_NW != 0);
    assert!(cpu.state.cr0 & CR0_BIT_CD != 0);
}

#[test]
fn mov_cr0_at_ring3_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0x0F, 0x22, 0xC0]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn mov_from_cr0_on_486_returns_full_writable_mask() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 = CR0_BIT_PE | CR0_BIT_ET | CR0_BIT_WP | CR0_BIT_AM;
    cpu.load_state(&state);

    // MOV EAX, CR0
    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x20, 0xC0]);

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), CR0_BIT_PE | CR0_BIT_ET | CR0_BIT_WP | CR0_BIT_AM);
}

#[test]
fn mov_to_cr0_clears_then_sets_pe_on_real_mode_setup_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode(&mut bus, 0xFFFF);
    cpu.load_state(&state);

    cpu.set_eax(CR0_BIT_PE | CR0_BIT_AM);

    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x22, 0xC0]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.cr0, CR0_BIT_PE | CR0_BIT_AM);
    assert_eq!(cpu.state.cr0 & CR0_BIT_PG, 0);
}

#[test]
fn lmsw_cannot_clear_pe_on_486() {
    // Per 80486 PRM 9.2.4: LMSW writes only the low 4 bits, and cannot
    // clear PE once it has been set. Verify the implementation still
    // OR-preserves PE through the LMSW write.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 = CR0_BIT_PE | CR0_BIT_ET;
    cpu.load_state(&state);

    cpu.set_eax(0x0000_0000); // LMSW would write zero into low 4 bits.
    // LMSW AX (0F 01 /6 with mod=11, rm=AX = 0xF0).
    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0x01, 0xF0]);

    cpu.step(&mut bus);

    assert!(cpu.state.cr0 & CR0_BIT_PE != 0, "LMSW must not clear PE");
}

#[test]
fn cmpxchg_does_not_modify_destination_on_mismatch_with_register_target_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    cpu.set_eax(0x42);
    cpu.set_ecx(0x77); // destination
    cpu.set_edx(0xAB); // source

    // CMPXCHG CL, DL: r/m=CL, r=DL.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xB0, modrm_register(REG_INDEX_EDX, REG_INDEX_ECX)],
    );

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.zf());
    assert_eq!(cpu.eax() & 0xFF, 0x77, "AL takes mem on mismatch");
    assert_eq!(cpu.ecx() & 0xFF, 0x77, "CL unchanged on mismatch");
    assert_eq!(cpu.edx() & 0xFF, 0xAB, "DL unchanged");
}

#[test]
fn cmpxchg_does_modify_destination_on_match_with_register_target_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    cpu.set_eax(0x42);
    cpu.set_ecx(0x42);
    cpu.set_edx(0xAB);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xB0, modrm_register(REG_INDEX_EDX, REG_INDEX_ECX)],
    );

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(cpu.eax() & 0xFF, 0x42);
    assert_eq!(cpu.ecx() & 0xFF, 0xAB, "CL takes source on match");
    assert_eq!(cpu.edx() & 0xFF, 0xAB);
}

#[test]
fn cmpxchg_dword_via_eax_full_width_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x1234_5678);
    cpu.set_ecx(0x1234_5678); // destination matches, will be replaced by EBX
    cpu.set_ebx(0xDEAD_BEEF);

    // 66 0F B1 [modr/m: r=EBX, r/m=ECX]
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x66,
            0x0F,
            0xB1,
            modrm_register(REG_INDEX_EBX, REG_INDEX_ECX),
        ],
    );

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(cpu.ecx(), 0xDEAD_BEEF);
    assert_eq!(cpu.eax(), 0x1234_5678);
}

#[test]
fn xadd_byte_to_memory_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x05;
    cpu.set_ecx(0x07);

    let mut code: Vec<u8> = vec![0x0F, 0xC0];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize],
        0x0C
    );
    assert_eq!(cpu.ecx() & 0xFF, 0x05);
}

#[test]
fn xadd_dword_to_memory_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    let memory_address = SHARED_DATA_BASE + memory_offset as u32;
    bus.ram[memory_address as usize] = 0x10;
    bus.ram[memory_address as usize + 1] = 0x00;
    bus.ram[memory_address as usize + 2] = 0x00;
    bus.ram[memory_address as usize + 3] = 0x00;
    cpu.set_ecx(0x0010_0000);

    let mut code: Vec<u8> = vec![0x66, 0x0F, 0xC1];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert_eq!(
        super::setup::read_dword_at(&bus, memory_address),
        0x0010_0010
    );
    assert_eq!(cpu.ecx(), 0x0000_0010);
}

#[test]
fn xadd_dword_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x66,
            0x0F,
            0xC1,
            modrm_register(REG_INDEX_ECX, REG_INDEX_EAX),
        ],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn cmpxchg_dword_raises_invalid_opcode_on_386() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x66,
            0x0F,
            0xB1,
            modrm_register(REG_INDEX_ECX, REG_INDEX_EAX),
        ],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_INVALID_OPCODE_IP as u32 + 1);
}

#[test]
fn wbinvd_in_vm86_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = super::setup::setup_vm86(&mut bus);
    super::setup::install_protected_mode_general_protection_handler(&mut bus);
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[0x0F, 0x09]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn invd_in_vm86_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = super::setup::setup_vm86(&mut bus);
    super::setup::install_protected_mode_general_protection_handler(&mut bus);
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[0x0F, 0x08]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn invlpg_in_vm86_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = super::setup::setup_vm86(&mut bus);
    super::setup::install_protected_mode_general_protection_handler(&mut bus);
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    place_at(
        &mut bus,
        0x0001_0000,
        &[
            0x0F,
            0x01,
            0x3E,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
        ],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn lmsw_at_ring3_raises_general_protection_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0x0F, 0x01, 0xF0]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn xadd_word_zero_plus_zero_sets_zf_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0);
    cpu.set_ecx(0);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[0x0F, 0xC1, modrm_register(REG_INDEX_ECX, REG_INDEX_EAX)],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFFFF, 0);
    assert!(cpu.state.flags.zf());
    assert!(!cpu.state.flags.cf());
}

#[test]
fn cmpxchg_byte_match_with_zero_source_writes_zero_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x100;
    bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize] = 0x42;
    cpu.set_eax(0x42);
    cpu.set_ecx(0x00);

    let mut code: Vec<u8> = vec![0x0F, 0xB0];
    modrm_memory_disp16(REG_INDEX_ECX, memory_offset, &mut code);
    place_at(&mut bus, RING0_CODE_BASE, &code);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.zf());
    assert_eq!(
        bus.ram[(SHARED_DATA_BASE + memory_offset as u32) as usize],
        0x00
    );
}

#[test]
fn bswap_without_operand_size_prefix_swaps_bytes_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0xAABB_CCDD);

    // BSWAP EAX without 0x66 prefix - the implementation always treats
    // the operand as 32-bit since BSWAP has no 16-bit form.
    place_at(&mut bus, RING0_CODE_BASE, &[0x0F, 0xC8]);

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0xDDCC_BBAA);
}

#[test]
fn invd_at_real_mode_succeeds_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    // Real mode: CPL test does not apply. INVD must execute as a no-op.
    let mut state = cpu::I386State::default();
    state.set_cs(0xF000);
    state.seg_bases[cpu::SegReg32::CS as usize] = 0x000F_0000;
    state.seg_limits = [0xFFFF; 6];
    state.seg_rights[cpu::SegReg32::CS as usize] =
        super::setup::RIGHTS_RING0_CODE_READABLE_ACCESSED;
    state.seg_valid = [true, true, true, true, false, false];
    cpu.load_state(&state);

    place_at(&mut bus, 0x000F_0000, &[0x0F, 0x08, HALT_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), 3);
}

#[test]
fn invlpg_does_not_fault_when_address_translates_through_unmapped_page_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    super::setup::enable_identity_paging(&mut bus, &mut state);
    cpu.load_state(&state);

    // INVLPG operates on the linear address regardless of whether it has
    // a TLB entry; absence of a TLB entry is silently a no-op.
    let memory_offset: u16 = 0x200;
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x0F,
            0x01,
            0x3E,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
            HALT_OPCODE,
        ],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(
        cpu.halted(),
        "INVLPG with no TLB entry must continue execution"
    );
    assert_eq!(cpu.ip(), 6, "after INVLPG then HLT, IP advances past HLT");
}

// 80486 PRM 6.3.5: alignment check (#AC, vector 17) requires CR0.AM=1,
// EFLAGS.AC=1, and CPL=3 simultaneously. The four-corner matrix verifies
// that any one missing condition suppresses the fault.

const EFLAGS_AC: u32 = 1 << 18;

const RING3_PROBE_OFFSET_ALIGNED: u16 = 0x100;
const RING3_PROBE_OFFSET_MISALIGNED: u16 = 0x101;
const RING3_PROBE_OFFSET_DWORD_MISALIGNED: u16 = 0x102;

// MOV AX, [disp16] = 0x8B 0x06 disp16-low disp16-high.
fn ring3_mov_ax_from_disp16(displacement: u16) -> [u8; 4] {
    [0x8B, 0x06, displacement as u8, (displacement >> 8) as u8]
}

// MOV EAX, [disp16] = 0x66 0x8B 0x06 disp16-low disp16-high.
fn ring3_mov_eax_from_disp16(displacement: u16) -> [u8; 5] {
    [
        0x66,
        0x8B,
        0x06,
        displacement as u8,
        (displacement >> 8) as u8,
    ]
}

#[test]
fn ac_word_access_misaligned_at_ring3_with_am_and_ac_set_raises_alignment_check_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_ALIGNMENT_CHECK_IP as u32 + 1);
}

#[test]
fn ac_word_access_aligned_at_ring3_with_am_and_ac_set_does_not_fault_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_ALIGNED as u32,
        0xCAFE,
    );
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_ALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0xCAFE);
}

#[test]
fn ac_dword_access_misaligned_at_ring3_with_am_and_ac_set_raises_alignment_check_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_eax_from_disp16(RING3_PROBE_OFFSET_DWORD_MISALIGNED),
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_ALIGNMENT_CHECK_IP as u32 + 1);
}

#[test]
fn ac_dword_access_aligned_at_ring3_with_am_and_ac_set_does_not_fault_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    super::setup::write_dword_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_ALIGNED as u32,
        0xCAFE_BABE,
    );
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_eax_from_disp16(RING3_PROBE_OFFSET_ALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax(), 0xCAFE_BABE);
}

#[test]
fn ac_dword_access_two_byte_aligned_at_ring3_raises_alignment_check_on_486() {
    // 4-byte alignment requires the two low bits of the linear address to
    // be zero. A 2-byte-aligned offset still violates 4-byte alignment.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    let two_byte_aligned_offset: u16 = 0x102;
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_eax_from_disp16(two_byte_aligned_offset),
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_ALIGNMENT_CHECK_IP as u32 + 1);
}

#[test]
fn ac_byte_access_at_ring3_with_am_and_ac_set_does_not_fault_on_486() {
    // Byte accesses are always aligned by definition.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    bus.ram[(SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32) as usize] = 0x77;
    // MOV AL, [disp16] = 0xA0 disp16-low disp16-high.
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &[
            0xA0,
            RING3_PROBE_OFFSET_MISALIGNED as u8,
            (RING3_PROBE_OFFSET_MISALIGNED >> 8) as u8,
        ],
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFF, 0x77);
}

#[test]
fn ac_word_access_at_ring0_with_am_and_ac_set_does_not_fault_on_486() {
    // Even with AM=AC=1 the CPL check must suppress #AC outside ring 3.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32,
        0x1234,
    );
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0x1234);
}

#[test]
fn ac_word_access_at_ring3_without_am_does_not_fault_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32,
        0xBEEF,
    );
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0xBEEF);
}

#[test]
fn ac_word_access_at_ring3_without_eflags_ac_does_not_fault_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32,
        0xC0DE,
    );
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0xC0DE);
}

#[test]
fn ac_misaligned_access_in_real_mode_does_not_fault_on_486() {
    // Real mode CPL is reported as 0; #AC must not fire even with AM=AC=1.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode(&mut bus, 0xFFFF);
    state.cr0 = 0x0010 | CR0_BIT_AM; // PE=0, AM=1; this is real mode.
    state.eflags_upper |= EFLAGS_AC;
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32,
        0x4242,
    );
    cpu.set_ds((SHARED_DATA_BASE >> 4) as u16);
    cpu.state.seg_bases[cpu::SegReg32::DS as usize] = SHARED_DATA_BASE;
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0x4242);
}

#[test]
fn ac_misaligned_word_access_in_vm86_with_am_and_ac_raises_alignment_check_on_486() {
    // VM86 reports CPL=3, so #AC fires when AM and AC are both set.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = super::setup::setup_vm86(&mut bus);
    super::setup::install_protected_mode_general_protection_handler(&mut bus);
    super::setup::write_interrupt_gate_386(
        &mut bus,
        super::setup::INTERRUPT_DESCRIPTOR_TABLE_BASE,
        17,
        HANDLER_ALIGNMENT_CHECK_IP as u32,
        super::setup::SELECTOR_RING0_CODE,
        0,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_ALIGNMENT_CHECK_IP as u32) as usize] = 0xF4;
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    let probe_offset: u16 = 0x101;
    place_at(
        &mut bus,
        0x0001_0000,
        &ring3_mov_ax_from_disp16(probe_offset),
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_ALIGNMENT_CHECK_IP as u32 + 1);
}

#[test]
fn ac_does_not_fire_on_386_even_with_am_and_ac_set() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    // The 386 ignores writes to CR0.AM, but the bit can still be set
    // directly in `state.cr0`. Verify the CPU model gate still suppresses
    // the #AC fault.
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32,
        0xABCD,
    );
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0xABCD);
}

#[test]
fn ac_misaligned_write_at_ring3_raises_alignment_check_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    cpu.set_eax(0x9999);

    // MOV [disp16], AX = 0x89 0x06 disp16-low disp16-high.
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &[
            0x89,
            0x06,
            RING3_PROBE_OFFSET_MISALIGNED as u8,
            (RING3_PROBE_OFFSET_MISALIGNED >> 8) as u8,
        ],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_ALIGNMENT_CHECK_IP as u32 + 1);
}

#[test]
fn ac_misaligned_push_at_ring3_raises_alignment_check_on_486() {
    // Stack accesses also trigger #AC when SP becomes misaligned for the
    // operand size.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.cr0 |= CR0_BIT_AM;
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    state.set_esp(0x0FFF); // odd SP -> push of word writes to misaligned address
    cpu.load_state(&state);

    // PUSH AX = 0x50.
    place_at(&mut bus, RING3_CODE_BASE, &[0x50]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_ALIGNMENT_CHECK_IP as u32 + 1);
}

#[test]
fn popfd_writes_ac_bit_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0xF000);
    cpu.load_state(&state);

    // Pre-place a 32-bit FLAGS value with AC=1 on the stack.
    let pushed: u32 = 0x0000_0002 | EFLAGS_AC;
    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE.wrapping_add(0xF000), pushed);
    cpu.state.seg_bases[cpu::SegReg32::SS as usize] = SHARED_DATA_BASE;

    // POPFD = 0x66 0x9D.
    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x9D]);

    cpu.step(&mut bus);

    assert!(cpu.state.eflags_upper & EFLAGS_AC != 0);
}

#[test]
fn popfd_does_not_write_ac_bit_on_386() {
    // On the 386, AC (bit 18) is reserved and POPFD must not set it.
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0xF000);
    cpu.load_state(&state);

    let pushed: u32 = 0x0000_0002 | EFLAGS_AC;
    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE.wrapping_add(0xF000), pushed);
    cpu.state.seg_bases[cpu::SegReg32::SS as usize] = SHARED_DATA_BASE;

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x9D]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.eflags_upper & EFLAGS_AC, 0);
}

#[test]
fn popfd_clears_ac_bit_when_pushed_value_clears_it_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.eflags_upper |= EFLAGS_AC;
    state.set_esp(0xF000);
    cpu.load_state(&state);

    let pushed: u32 = 0x0000_0002; // AC=0
    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE.wrapping_add(0xF000), pushed);
    cpu.state.seg_bases[cpu::SegReg32::SS as usize] = SHARED_DATA_BASE;

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x9D]);

    cpu.step(&mut bus);

    assert_eq!(cpu.state.eflags_upper & EFLAGS_AC, 0);
}

#[test]
fn pushfd_includes_ac_bit_on_486() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.eflags_upper |= EFLAGS_AC;
    state.set_esp(0xF008);
    cpu.load_state(&state);

    cpu.state.seg_bases[cpu::SegReg32::SS as usize] = SHARED_DATA_BASE;

    // PUSHFD = 0x66 0x9C.
    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x9C]);

    cpu.step(&mut bus);

    let pushed = super::setup::read_dword_at(&bus, SHARED_DATA_BASE + (0xF008 - 4));
    assert!(pushed & EFLAGS_AC != 0);
}

#[test]
fn ac_misaligned_word_at_ring3_does_not_fault_when_ac_already_high_on_disabled_mask_on_486() {
    // Sanity case: if EFLAGS.AC is set but CR0.AM=0, the access is allowed
    // even though the linear address is misaligned. This complements the
    // four-corner matrix.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.eflags_upper |= EFLAGS_AC;
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    super::setup::write_word_at(
        &mut bus,
        SHARED_DATA_BASE + RING3_PROBE_OFFSET_MISALIGNED as u32,
        0xDEAD,
    );
    place_at(
        &mut bus,
        RING3_CODE_BASE,
        &ring3_mov_ax_from_disp16(RING3_PROBE_OFFSET_MISALIGNED),
    );

    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.eax() & 0xFFFF, 0xDEAD);
}
