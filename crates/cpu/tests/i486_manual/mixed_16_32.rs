//! Mixing 16-bit and 32-bit code (80486 PRM Chapter 24).
//!
//! Validates the four prefix-vs-D-bit corners (24.2), B-bit stack pointer
//! semantics (24.1), and cross-width control transfer through call gates
//! (24.4). The setup helper installs a 16-bit code segment alongside the
//! 32-bit one and a 16-bit stack segment alongside the 32-bit stack.

use common::Cpu as _;

use super::setup::{
    GLOBAL_DESCRIPTOR_TABLE_BASE, RIGHTS_RING0_DATA_WRITABLE_ACCESSED, RING0_CODE_16BIT_BASE,
    RING0_CODE_BASE, RING0_STACK_16BIT_BASE, RING0_STACK_BASE, SELECTOR_RING0_CODE,
    SELECTOR_RING0_CODE_16BIT, SELECTOR_RING0_DATA_16BIT_STACK, SHARED_DATA_BASE, TestBus,
    make_cpu_486, place_at, read_dword_at, read_word_at, setup_protected_mode_mixed_widths,
    write_call_gate_286, write_call_gate_386, write_dword_at, write_segment_descriptor,
    write_word_at,
};

const HLT_OPCODE: u8 = 0xF4;

const CALL_GATE_TO_32BIT_SLOT: u16 = 12;
const CALL_GATE_TO_16BIT_SLOT: u16 = 13;
const SELECTOR_CALL_GATE_TO_32BIT: u16 = CALL_GATE_TO_32BIT_SLOT << 3;
const SELECTOR_CALL_GATE_TO_16BIT: u16 = CALL_GATE_TO_16BIT_SLOT << 3;

fn switch_cpu_state_to_16bit_code(state: &mut cpu::I386State) {
    state.set_cs(SELECTOR_RING0_CODE_16BIT);
    state.seg_bases[cpu::SegReg32::CS as usize] = RING0_CODE_16BIT_BASE;
    state.seg_granularity[cpu::SegReg32::CS as usize] = 0;
}

fn switch_cpu_state_to_16bit_stack(state: &mut cpu::I386State) {
    state.set_ss(SELECTOR_RING0_DATA_16BIT_STACK);
    state.seg_bases[cpu::SegReg32::SS as usize] = RING0_STACK_16BIT_BASE;
    state.seg_granularity[cpu::SegReg32::SS as usize] = 0;
    state.set_esp(0x0000_F000);
}

#[test]
fn prefix_66h_in_16bit_code_promotes_mov_to_32bit_operand() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);
    cpu.set_eax(0x1122_3344);

    // 66 A3 disp16 = MOV ds:[disp16], EAX (32-bit operand under 16-bit
    // address default; 66H promotes operand size).
    let memory_offset: u16 = 0x0040;
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE,
        &[
            0x66,
            0xA3,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    let stored = read_dword_at(&bus, SHARED_DATA_BASE + memory_offset as u32);
    assert_eq!(stored, 0x1122_3344);
}

#[test]
fn prefix_66h_in_16bit_code_promotes_add_to_32bit_operand() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);
    cpu.set_eax(0x0000_0001);
    cpu.set_ebx(0x1234_5678);

    // 66 01 D8 = ADD EAX, EBX.
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE,
        &[0x66, 0x01, 0xD8, HLT_OPCODE],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0x1234_5679);
}

#[test]
fn prefix_66h_in_32bit_code_demotes_mov_to_16bit_operand() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0xFFFF_BEEF);

    // 66 A3 disp32 = MOV ds:[disp32], AX (16-bit operand). 32-bit code
    // segment uses 32-bit addressing by default.
    let memory_offset: u32 = 0x0000_0040;
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x66,
            0xA3,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
            (memory_offset >> 16) as u8,
            (memory_offset >> 24) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(read_word_at(&bus, SHARED_DATA_BASE + memory_offset), 0xBEEF);
    assert_eq!(read_word_at(&bus, SHARED_DATA_BASE + memory_offset + 2), 0);
}

#[test]
fn prefix_66h_in_32bit_code_demotes_add_to_16bit_operand() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x1234_FFFF);
    cpu.set_ebx(0x5678_0001);

    // 66 01 D8 = ADD AX, BX.
    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x01, 0xD8, HLT_OPCODE]);

    cpu.step(&mut bus);

    // Low 16 bits: 0xFFFF + 0x0001 = 0x0000 (carry out, top half unaffected).
    assert_eq!(cpu.eax() & 0xFFFF, 0x0000);
    assert_eq!(cpu.eax() & 0xFFFF_0000, 0x1234_0000);
}

#[test]
fn prefix_67h_in_16bit_code_uses_32bit_effective_address() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    state.seg_limits[cpu::SegReg32::DS as usize] = 0x000F_FFFF;
    cpu.load_state(&state);

    let memory_address = 0x0001_2345u32;
    write_word_at(&mut bus, SHARED_DATA_BASE + memory_address, 0xABCD);

    // 67 A1 disp32 = MOV AX, ds:[disp32] with 32-bit addressing.
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE,
        &[
            0x67,
            0xA1,
            memory_address as u8,
            (memory_address >> 8) as u8,
            (memory_address >> 16) as u8,
            (memory_address >> 24) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFFFF, 0xABCD);
}

#[test]
fn prefix_67h_in_32bit_code_uses_16bit_effective_address() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);
    cpu.set_ebx(0x0040);

    let memory_offset: u16 = 0x0040;
    write_word_at(&mut bus, SHARED_DATA_BASE + memory_offset as u32, 0x1234);

    // 67 A1 disp16 = MOV AX, ds:[disp16] with 16-bit addressing override.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x67,
            0xA1,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFFFF, 0x1234);
}

#[test]
fn combined_66h_67h_in_16bit_code_uses_32bit_operand_and_32bit_address() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    state.seg_limits[cpu::SegReg32::DS as usize] = 0x000F_FFFF;
    cpu.load_state(&state);

    let memory_address = 0x0001_0080u32;
    write_dword_at(&mut bus, SHARED_DATA_BASE + memory_address, 0xCAFE_BABE);

    // 66 67 A1 disp32 = MOV EAX, ds:[disp32] with both prefixes.
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE,
        &[
            0x66,
            0x67,
            0xA1,
            memory_address as u8,
            (memory_address >> 8) as u8,
            (memory_address >> 16) as u8,
            (memory_address >> 24) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0xCAFE_BABE);
}

#[test]
fn combined_66h_67h_in_32bit_code_uses_16bit_operand_and_16bit_address() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);

    let memory_offset: u16 = 0x0080;
    write_word_at(&mut bus, SHARED_DATA_BASE + memory_offset as u32, 0xBEEF);

    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x66,
            0x67,
            0xA1,
            memory_offset as u8,
            (memory_offset >> 8) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFFFF, 0xBEEF);
}

#[test]
fn implicit_stack_push_uses_sp_when_ss_b_bit_clear() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_stack(&mut state);
    cpu.load_state(&state);
    cpu.set_eax(0x1234_5678);

    // PUSH AX = 0x50 (16-bit operand because no 66H, default 32-bit code).
    // We're in 32-bit code but want to verify SP semantics. Use 66H to
    // force 16-bit operand, so PUSH writes 2 bytes at SS:SP.
    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x50, HLT_OPCODE]);

    cpu.step(&mut bus);

    // B-bit clear: only SP (low 16 bits of ESP) decrements; ESP upper bits
    // must stay at 0.
    assert_eq!(cpu.esp() & 0xFFFF, 0xEFFE);
    assert_eq!(cpu.esp() & 0xFFFF_0000, 0);
    let pushed = read_word_at(&bus, RING0_STACK_16BIT_BASE + (cpu.esp() & 0xFFFF));
    assert_eq!(pushed, 0x5678);
}

#[test]
fn implicit_stack_push_uses_esp_when_ss_b_bit_set() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);
    cpu.set_eax(0x1234_5678);
    cpu.set_esp(0x0001_0000);

    // PUSH EAX = 0x50 (32-bit operand by default in 32-bit code).
    place_at(&mut bus, RING0_CODE_BASE, &[0x50, HLT_OPCODE]);

    cpu.step(&mut bus);

    // B-bit set: full ESP decrements by 4.
    assert_eq!(cpu.esp(), 0x0000_FFFC);
    let pushed = read_dword_at(&bus, RING0_STACK_BASE + cpu.esp());
    assert_eq!(pushed, 0x1234_5678);
}

#[test]
fn far_call_from_16bit_code_through_386_call_gate_pushes_dword_frame_on_inner_stack() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);

    // Install a 386 call gate at slot 12 pointing at a 32-bit code target.
    let gate_target_offset: u32 = 0x0000_0080;
    write_call_gate_386(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        CALL_GATE_TO_32BIT_SLOT,
        gate_target_offset,
        SELECTOR_RING0_CODE,
        0,
        0,
    );
    cpu.state.gdt_limit = (CALL_GATE_TO_16BIT_SLOT + 1) * 8 - 1;

    // CALL FAR mem16:32 form (0x9A) using ptr16:16 — the 386 gate ignores
    // the immediate offset/selector pair, but the encoded operands need to
    // be 16-bit because we are in a 16-bit code segment.
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE,
        &[
            0x9A,
            0x00,
            0x00,
            SELECTOR_CALL_GATE_TO_32BIT as u8,
            (SELECTOR_CALL_GATE_TO_32BIT >> 8) as u8,
        ],
    );
    place_at(
        &mut bus,
        RING0_CODE_BASE + gate_target_offset,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);

    // Same-privilege CALL through 386 gate pushes a dword return frame
    // (CS, EIP) regardless of caller's D-bit.
    let return_eip = read_dword_at(&bus, RING0_STACK_BASE + cpu.esp());
    let return_cs = read_dword_at(&bus, RING0_STACK_BASE + cpu.esp() + 4);
    assert_eq!(return_cs as u16, SELECTOR_RING0_CODE_16BIT);
    assert_eq!(return_eip, 5, "EIP after 5-byte CALL FAR");
    assert_eq!(cpu.cs(), SELECTOR_RING0_CODE);
    assert_eq!(cpu.ip(), gate_target_offset);
}

#[test]
fn far_call_from_32bit_code_through_286_call_gate_pushes_word_frame_on_inner_stack() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);

    // 286 call gate at slot 13 pointing at a 16-bit code target.
    let gate_target_offset: u32 = 0x0000_0090;
    write_call_gate_286(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        CALL_GATE_TO_16BIT_SLOT,
        gate_target_offset,
        SELECTOR_RING0_CODE_16BIT,
        0,
        0,
    );
    cpu.state.gdt_limit = (CALL_GATE_TO_16BIT_SLOT + 1) * 8 - 1;

    // CALL FAR ptr16:32 in 32-bit code segment.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0x9A,
            0x00,
            0x00,
            0x00,
            0x00,
            SELECTOR_CALL_GATE_TO_16BIT as u8,
            (SELECTOR_CALL_GATE_TO_16BIT >> 8) as u8,
        ],
    );
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE + gate_target_offset,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);

    // 286 gate pushes a word return frame (CS, IP).
    let return_ip = read_word_at(&bus, RING0_STACK_BASE + cpu.esp());
    let return_cs = read_word_at(&bus, RING0_STACK_BASE + cpu.esp() + 2);
    assert_eq!(return_cs, SELECTOR_RING0_CODE);
    assert_eq!(return_ip, 7, "IP after 7-byte CALL FAR ptr16:32");
    assert_eq!(cpu.cs(), SELECTOR_RING0_CODE_16BIT);
    assert_eq!(cpu.ip(), gate_target_offset);
}

#[test]
fn ret_in_16bit_code_pops_two_bytes() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);

    let return_offset: u16 = 0x0040;
    let initial_esp = cpu.esp();
    write_word_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);

    // RET (near, 16-bit operand) = 0xC3.
    place_at(&mut bus, RING0_CODE_16BIT_BASE, &[0xC3]);
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE + return_offset as u32,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), return_offset as u32 + 1);
    assert_eq!(cpu.esp(), initial_esp + 2);
}

#[test]
fn ret_with_66h_prefix_in_16bit_code_pops_four_bytes() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);

    let return_offset: u32 = 0x0050;
    let initial_esp = cpu.esp();
    write_dword_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);

    place_at(&mut bus, RING0_CODE_16BIT_BASE, &[0x66, 0xC3]);
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE + return_offset,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), return_offset + 1);
    assert_eq!(cpu.esp(), initial_esp + 4);
}

#[test]
fn ret_in_32bit_code_pops_four_bytes() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);

    let return_offset: u32 = 0x0040;
    let initial_esp = cpu.esp();
    write_dword_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);

    place_at(&mut bus, RING0_CODE_BASE, &[0xC3]);
    place_at(&mut bus, RING0_CODE_BASE + return_offset, &[HLT_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), return_offset + 1);
    assert_eq!(cpu.esp(), initial_esp + 4);
}

#[test]
fn ret_with_66h_prefix_in_32bit_code_pops_two_bytes() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);

    let return_offset: u16 = 0x0050;
    let initial_esp = cpu.esp();
    write_word_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0xC3]);
    place_at(
        &mut bus,
        RING0_CODE_BASE + return_offset as u32,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), return_offset as u32 + 1);
    assert_eq!(cpu.esp(), initial_esp + 2);
}

#[test]
fn iret_in_16bit_code_pops_three_words() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);

    let return_offset: u16 = 0x0040;
    let initial_esp = cpu.esp();
    write_word_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);
    write_word_at(
        &mut bus,
        RING0_STACK_BASE + initial_esp + 2,
        SELECTOR_RING0_CODE_16BIT,
    );
    write_word_at(&mut bus, RING0_STACK_BASE + initial_esp + 4, 0x0202);

    place_at(&mut bus, RING0_CODE_16BIT_BASE, &[0xCF]);
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE + return_offset as u32,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.esp(), initial_esp + 6);
}

#[test]
fn iret_with_66h_prefix_in_16bit_code_pops_three_dwords() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);

    let return_offset: u32 = 0x0040;
    let initial_esp = cpu.esp();
    write_dword_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);
    write_dword_at(
        &mut bus,
        RING0_STACK_BASE + initial_esp + 4,
        SELECTOR_RING0_CODE_16BIT as u32,
    );
    write_dword_at(&mut bus, RING0_STACK_BASE + initial_esp + 8, 0x0000_0202);

    place_at(&mut bus, RING0_CODE_16BIT_BASE, &[0x66, 0xCF]);
    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE + return_offset,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.esp(), initial_esp + 12);
}

#[test]
fn iret_in_32bit_code_pops_three_dwords() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);

    let return_offset: u32 = 0x0040;
    let initial_esp = cpu.esp();
    write_dword_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);
    write_dword_at(
        &mut bus,
        RING0_STACK_BASE + initial_esp + 4,
        SELECTOR_RING0_CODE as u32,
    );
    write_dword_at(&mut bus, RING0_STACK_BASE + initial_esp + 8, 0x0000_0202);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCF]);
    place_at(&mut bus, RING0_CODE_BASE + return_offset, &[HLT_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.esp(), initial_esp + 12);
}

#[test]
fn iret_with_66h_prefix_in_32bit_code_pops_three_words() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);

    let return_offset: u16 = 0x0040;
    let initial_esp = cpu.esp();
    write_word_at(&mut bus, RING0_STACK_BASE + initial_esp, return_offset);
    write_word_at(
        &mut bus,
        RING0_STACK_BASE + initial_esp + 2,
        SELECTOR_RING0_CODE,
    );
    write_word_at(&mut bus, RING0_STACK_BASE + initial_esp + 4, 0x0202);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0xCF]);
    place_at(
        &mut bus,
        RING0_CODE_BASE + return_offset as u32,
        &[HLT_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.esp(), initial_esp + 6);
}

#[test]
fn expand_up_ss_with_b_bit_clear_uses_16bit_stack_pointer_and_64k_limit() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    switch_cpu_state_to_16bit_stack(&mut state);
    state.set_esp(0xDEAD_F000);
    cpu.load_state(&state);

    // PUSH AX (66 50 in 32-bit code segment).
    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x50, HLT_OPCODE]);
    cpu.set_eax(0xCAFE_BABE);

    cpu.step(&mut bus);

    // Only the low 16 bits of ESP changed; the upper bits are preserved
    // verbatim because the B-bit is clear.
    assert_eq!(cpu.esp() & 0xFFFF, 0xEFFE);
    assert_eq!(cpu.esp() & 0xFFFF_0000, 0xDEAD_0000);
}

#[test]
fn expand_up_ss_with_b_bit_set_uses_32bit_stack_pointer() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_mixed_widths(&mut bus);
    cpu.load_state(&state);
    cpu.set_esp(0x0010_0000);

    place_at(&mut bus, RING0_CODE_BASE, &[0x50, HLT_OPCODE]);

    cpu.step(&mut bus);

    // Full 32-bit ESP decrement.
    assert_eq!(cpu.esp(), 0x000F_FFFC);
}

#[test]
fn prefix_67h_in_16bit_code_reads_data_above_64k_offset() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    // Replace the standard ring-0 data segment with one whose limit
    // extends above 64 KiB so the test can reach beyond a 16-bit offset.
    let mut state = setup_protected_mode_mixed_widths(&mut bus);
    write_segment_descriptor(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        2, // SELECTOR_RING0_DATA slot
        SHARED_DATA_BASE,
        0x000F_FFFF,
        RIGHTS_RING0_DATA_WRITABLE_ACCESSED,
        0,
    );
    state.seg_limits[cpu::SegReg32::DS as usize] = 0x000F_FFFF;
    switch_cpu_state_to_16bit_code(&mut state);
    cpu.load_state(&state);

    let memory_address = 0x0001_2000u32; // > 64 KiB
    write_word_at(&mut bus, SHARED_DATA_BASE + memory_address, 0x55AA);

    place_at(
        &mut bus,
        RING0_CODE_16BIT_BASE,
        &[
            0x67,
            0xA1,
            memory_address as u8,
            (memory_address >> 8) as u8,
            (memory_address >> 16) as u8,
            (memory_address >> 24) as u8,
            HLT_OPCODE,
        ],
    );

    cpu.step(&mut bus);

    assert_eq!(cpu.eax() & 0xFFFF, 0x55AA);
}
