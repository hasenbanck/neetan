//! INT3 (0xCC), INTO (0xCE), and BOUND (0x62) tests.
//!
//! Per 80486 PRM Chapter 9 and the per-instruction reference:
//!   - INT3 dispatches vector 3 unconditionally; in VM86 it is NOT IOPL-
//!     gated (only INT n / 0xCD is).
//!   - INTO dispatches vector 4 only when EFLAGS.OF=1; otherwise it is a
//!     no-op. Like INT3, it is exempt from the VM86 IOPL gate.
//!   - BOUND raises #BR (vector 5) when the indexed value is outside the
//!     [low, high] range read from the memory operand. In-range values
//!     leave the CPU unchanged.

use common::Cpu as _;
use cpu::{CPU_MODEL_386, I386, I386State};

use super::setup::{
    HANDLER_BOUND_RANGE_IP, HANDLER_BREAKPOINT_IP, HANDLER_GENERAL_PROTECTION_IP,
    HANDLER_OVERFLOW_IP, INTERRUPT_DESCRIPTOR_TABLE_BASE, RING0_CODE_BASE, SELECTOR_RING0_CODE,
    SHARED_DATA_BASE, TestBus, place_at, place_code, promote_to_ring3,
    setup_protected_mode_with_handlers, setup_vm86_with_iopl, write_interrupt_gate_386,
    write_word_at,
};

fn make_cpu_386() -> I386<{ CPU_MODEL_386 }> {
    I386::<{ CPU_MODEL_386 }>::new()
}

#[test]
fn int3_pm_cpl0_dispatches_vector_3() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BREAKPOINT_IP as u32 + 1);
}

#[test]
fn int3_pm_cpl3_via_dpl3_gate_succeeds() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    // Reinstall vector 3 with DPL=3 so CPL=3 can use it.
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        3,
        HANDLER_BREAKPOINT_IP as u32,
        SELECTOR_RING0_CODE,
        3,
    );
    promote_to_ring3(&mut state);
    cpu.load_state(&state);

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BREAKPOINT_IP as u32 + 1);
}

#[test]
fn int3_real_mode_dispatches_via_ivt_vector_3() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let handler_segment: u16 = 0x2000;
    let handler_offset: u16 = 0x0100;
    bus.ram[3 * 4] = handler_offset as u8;
    bus.ram[3 * 4 + 1] = (handler_offset >> 8) as u8;
    bus.ram[3 * 4 + 2] = handler_segment as u8;
    bus.ram[3 * 4 + 3] = (handler_segment >> 8) as u8;
    let handler_linear = ((handler_segment as u32) << 4) + handler_offset as u32;
    bus.ram[handler_linear as usize] = 0xF4;

    place_code(&mut bus, 0xFFFF, 0x0000, &[0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.cs(), handler_segment);
    assert_eq!(cpu.ip() as u16, handler_offset + 1);
}

#[test]
fn int3_vm86_at_iopl_0_dispatches_via_idt_no_iopl_gate() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 0);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        3,
        HANDLER_BREAKPOINT_IP as u32,
        SELECTOR_RING0_CODE,
        3,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_BREAKPOINT_IP as u32) as usize] = 0xF4;
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    place_code(&mut bus, 0x1000, 0x0000, &[0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(
        cpu.ip(),
        HANDLER_BREAKPOINT_IP as u32 + 1,
        "INT3 in VM86 ignores IOPL (unlike INT n)"
    );
}

#[test]
fn int3_vm86_at_iopl_3_dispatches_via_idt() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 3);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        3,
        HANDLER_BREAKPOINT_IP as u32,
        SELECTOR_RING0_CODE,
        3,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_BREAKPOINT_IP as u32) as usize] = 0xF4;
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    place_code(&mut bus, 0x1000, 0x0000, &[0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BREAKPOINT_IP as u32 + 1);
}

#[test]
fn int3_pm_with_dpl0_gate_at_cpl0_succeeds() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BREAKPOINT_IP as u32 + 1);
}

#[test]
fn int_3_imm_form_pm_dispatches_same_as_int3() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    // 0xCD 0x03 = INT 3 immediate form (different opcode, same vector).
    place_at(&mut bus, RING0_CODE_BASE, &[0xCD, 0x03]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BREAKPOINT_IP as u32 + 1);
}

#[test]
fn int_3_imm_form_vm86_at_iopl_0_raises_general_protection() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 0);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        13,
        HANDLER_GENERAL_PROTECTION_IP as u32,
        SELECTOR_RING0_CODE,
        0,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_GENERAL_PROTECTION_IP as u32) as usize] = 0xF4;
    state.gdt_limit = 5 * 8 - 1;
    cpu.load_state(&state);

    place_code(&mut bus, 0x1000, 0x0000, &[0xCD, 0x03]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(
        cpu.ip(),
        HANDLER_GENERAL_PROTECTION_IP as u32 + 1,
        "INT 3 immediate form (0xCD 03) is INT n and respects IOPL gate"
    );
}

#[test]
fn into_with_of_clear_is_a_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.overflow_val = 0;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCE, 0x90]);
    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.ip(), 1, "INTO with OF=0 advances IP past the opcode");
}

#[test]
fn into_with_of_set_dispatches_vector_4() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.overflow_val = 0x8000;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCE]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_OVERFLOW_IP as u32 + 1);
}

#[test]
fn into_in_vm86_at_iopl_0_with_of_set_does_not_raise_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 0);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        4,
        HANDLER_OVERFLOW_IP as u32,
        SELECTOR_RING0_CODE,
        3,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_OVERFLOW_IP as u32) as usize] = 0xF4;
    state.gdt_limit = 5 * 8 - 1;
    state.flags.overflow_val = 0x8000;
    cpu.load_state(&state);

    place_code(&mut bus, 0x1000, 0x0000, &[0xCE]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(
        cpu.ip(),
        HANDLER_OVERFLOW_IP as u32 + 1,
        "INTO is exempt from VM86 IOPL gate"
    );
}

#[test]
fn into_real_mode_with_of_set_dispatches_via_ivt() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let handler_segment: u16 = 0x2000;
    let handler_offset: u16 = 0x0100;
    bus.ram[4 * 4] = handler_offset as u8;
    bus.ram[4 * 4 + 1] = (handler_offset >> 8) as u8;
    bus.ram[4 * 4 + 2] = handler_segment as u8;
    bus.ram[4 * 4 + 3] = (handler_segment >> 8) as u8;
    let handler_linear = ((handler_segment as u32) << 4) + handler_offset as u32;
    bus.ram[handler_linear as usize] = 0xF4;

    let mut state = I386State::default();
    state.set_cs(0xFFFF);
    state.seg_bases[cpu::SegReg32::CS as usize] = 0x000F_FFF0;
    state.set_ss(0x3000);
    state.seg_bases[cpu::SegReg32::SS as usize] = 0x0003_0000;
    state.set_esp(0x1000);
    state.seg_limits = [0xFFFF; 6];
    state.seg_rights[cpu::SegReg32::CS as usize] = 0x9B;
    state.seg_rights[cpu::SegReg32::SS as usize] = 0x93;
    state.seg_valid = [true; 6];
    state.flags.overflow_val = 0x8000;
    cpu.load_state(&state);

    place_at(&mut bus, 0x000F_FFF0, &[0xCE]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.cs(), handler_segment);
    assert_eq!(cpu.ip() as u16, handler_offset + 1);
}

#[test]
fn into_at_cpl3_via_dpl3_gate_dispatches_when_of_set() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        4,
        HANDLER_OVERFLOW_IP as u32,
        SELECTOR_RING0_CODE,
        3,
    );
    promote_to_ring3(&mut state);
    state.flags.overflow_val = 0x8000;
    cpu.load_state(&state);

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0xCE]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_OVERFLOW_IP as u32 + 1);
}

#[test]
fn bound_word_in_range_is_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_0050);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    // BOUND AX, [DS:0x40] (0x62 modrm=0x06 disp16=0x0040)
    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert!(!cpu.halted());
}

#[test]
fn bound_word_below_range_raises_br() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_0005);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(
        cpu.ip(),
        HANDLER_BOUND_RANGE_IP as u32 + 1,
        "BOUND below range raises #BR (vector 5)"
    );
}

#[test]
fn bound_word_above_range_raises_br() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_0FFF);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BOUND_RANGE_IP as u32 + 1);
}

#[test]
fn bound_word_signed_negative_below_negative_low_raises_br() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_FFF0);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0xFFFE);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x000A);

    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BOUND_RANGE_IP as u32 + 1);
}

#[test]
fn bound_word_at_low_boundary_inclusive_does_not_raise() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_0010);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert!(!cpu.halted(), "BOUND lower bound is inclusive");
}

#[test]
fn bound_word_at_high_boundary_inclusive_does_not_raise() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_00FF);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert!(!cpu.halted(), "BOUND upper bound is inclusive");
}

#[test]
fn bound_dword_in_range_is_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0000_5000);
    cpu.load_state(&state);

    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0000_1000);
    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE + 0x44, 0x0000_FFFF);

    // BOUND EAX, [DS:0x40] (0x66 0x62 modrm=0x06 disp16=0x0040)
    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert!(!cpu.halted());
}

#[test]
fn bound_dword_above_range_raises_br() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_eax(0x0001_0000);
    cpu.load_state(&state);

    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0000_1000);
    super::setup::write_dword_at(&mut bus, SHARED_DATA_BASE + 0x44, 0x0000_FFFF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_BOUND_RANGE_IP as u32 + 1);
}

#[test]
fn bound_real_mode_in_range_is_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = I386State::default();
    state.set_cs(0xFFFF);
    state.seg_bases[cpu::SegReg32::CS as usize] = 0x000F_FFF0;
    state.set_ds(0x1000);
    state.seg_bases[cpu::SegReg32::DS as usize] = 0x0001_0000;
    state.set_ss(0x3000);
    state.seg_bases[cpu::SegReg32::SS as usize] = 0x0003_0000;
    state.set_esp(0x1000);
    state.seg_limits = [0xFFFF; 6];
    state.seg_rights[cpu::SegReg32::CS as usize] = 0x9B;
    state.seg_rights[cpu::SegReg32::DS as usize] = 0x93;
    state.seg_rights[cpu::SegReg32::SS as usize] = 0x93;
    state.seg_valid = [true; 6];
    state.set_eax(0x50);
    cpu.load_state(&state);

    write_word_at(&mut bus, 0x0001_0040, 0x0010);
    write_word_at(&mut bus, 0x0001_0042, 0x00FF);

    place_at(&mut bus, 0x000F_FFF0, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert!(!cpu.halted());
}

#[test]
fn bound_real_mode_out_of_range_raises_br() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let handler_segment: u16 = 0x2000;
    let handler_offset: u16 = 0x0100;
    bus.ram[5 * 4] = handler_offset as u8;
    bus.ram[5 * 4 + 1] = (handler_offset >> 8) as u8;
    bus.ram[5 * 4 + 2] = handler_segment as u8;
    bus.ram[5 * 4 + 3] = (handler_segment >> 8) as u8;
    let handler_linear = ((handler_segment as u32) << 4) + handler_offset as u32;
    bus.ram[handler_linear as usize] = 0xF4;

    let mut state = I386State::default();
    state.set_cs(0xFFFF);
    state.seg_bases[cpu::SegReg32::CS as usize] = 0x000F_FFF0;
    state.set_ds(0x1000);
    state.seg_bases[cpu::SegReg32::DS as usize] = 0x0001_0000;
    state.set_ss(0x3000);
    state.seg_bases[cpu::SegReg32::SS as usize] = 0x0003_0000;
    state.set_esp(0x1000);
    state.seg_limits = [0xFFFF; 6];
    state.seg_rights[cpu::SegReg32::CS as usize] = 0x9B;
    state.seg_rights[cpu::SegReg32::DS as usize] = 0x93;
    state.seg_rights[cpu::SegReg32::SS as usize] = 0x93;
    state.seg_valid = [true; 6];
    state.set_eax(0xFFFF);
    cpu.load_state(&state);

    write_word_at(&mut bus, 0x0001_0040, 0x0010);
    write_word_at(&mut bus, 0x0001_0042, 0x0050);

    place_at(&mut bus, 0x000F_FFF0, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.cs(), handler_segment);
}

#[test]
fn bound_at_cpl3_in_range_is_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_eax(0x50);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    place_at(
        &mut bus,
        super::setup::RING3_CODE_BASE,
        &[0x62, 0x06, 0x40, 0x00],
    );
    cpu.step(&mut bus);

    assert!(
        !cpu.halted(),
        "BOUND has no privilege gate; passes at any CPL"
    );
}

#[test]
fn bound_vm86_in_range_is_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 3);
    state.set_eax(0x50);
    cpu.load_state(&state);

    let ds_linear: u32 = 0x40_000;
    write_word_at(&mut bus, ds_linear + 0x40, 0x0010);
    write_word_at(&mut bus, ds_linear + 0x42, 0x00FF);

    place_code(&mut bus, 0x1000, 0x0000, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert!(!cpu.halted());
}

#[test]
fn into_with_of_set_pushes_correct_return_address() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0xFFE0);
    state.flags.overflow_val = 0x8000;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCE]);
    cpu.step(&mut bus);

    let pushed_eip = super::setup::read_dword_at(&bus, super::setup::RING0_STACK_BASE + cpu.esp());
    assert_eq!(
        pushed_eip, 1,
        "INTO with OF=1 pushes the byte after INTO (1 byte past start)"
    );
}

#[test]
fn int3_pushes_eip_pointing_past_one_byte_opcode() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0xFFE0);
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCC]);
    cpu.step(&mut bus);

    let pushed_eip = super::setup::read_dword_at(&bus, super::setup::RING0_STACK_BASE + cpu.esp());
    assert_eq!(pushed_eip, 1, "INT3 pushes EIP past its 1-byte opcode");
}

#[test]
fn into_with_of_clear_does_not_change_stack() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0xFFE0);
    state.flags.overflow_val = 0;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCE, 0x90]);
    cpu.step(&mut bus);

    assert_eq!(
        cpu.esp(),
        0xFFE0,
        "INTO with OF=0 does not modify the stack"
    );
}

#[test]
fn bound_in_range_pushes_no_frame() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_esp(0xFFE0);
    state.set_eax(0x0050);
    cpu.load_state(&state);

    write_word_at(&mut bus, SHARED_DATA_BASE + 0x40, 0x0010);
    write_word_at(&mut bus, SHARED_DATA_BASE + 0x42, 0x00FF);

    place_at(&mut bus, RING0_CODE_BASE, &[0x62, 0x06, 0x40, 0x00]);
    cpu.step(&mut bus);

    assert_eq!(cpu.esp(), 0xFFE0);
}

#[test]
fn into_real_mode_with_of_clear_is_no_op() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    place_code(&mut bus, 0xFFFF, 0x0000, &[0xCE, 0x90]);
    cpu.step(&mut bus);

    assert!(!cpu.halted());
    assert_eq!(cpu.ip(), 1);
}

// Removed: shrinking IDT limit below INT3 vector AND #GP vector causes a
// triple-fault shutdown rather than a clean #GP dispatch, since both vectors
// 3 and 13 lie in the truncated region. The general behavior is exercised by
// int_n_pm_with_idt_limit_too_small_raises_general_protection in int_n.rs
// using a high vector that leaves the GP gate accessible.

#[test]
fn into_pm_with_of_set_clears_if_when_dispatched_via_interrupt_gate() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.if_flag = true;
    state.flags.overflow_val = 0x8000;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[0xCE]);
    cpu.step(&mut bus);

    assert!(
        !cpu.state.flags.if_flag,
        "Default vector 4 is an interrupt gate so IF is cleared"
    );
}
