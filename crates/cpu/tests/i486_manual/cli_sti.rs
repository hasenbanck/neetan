//! CLI / STI privilege-level boundary tests.
//!
//! 80486 PRM CLI/STI references. The IF gate behaviour:
//!
//! - Real mode: CLI/STI always succeed.
//! - PM CPL <= IOPL: CLI/STI succeed (clear/set IF).
//! - PM CPL > IOPL: -> #GP(0).
//! - VM86 IOPL=3: CLI/STI succeed (CPL=3, IOPL=3, so 3 <= 3).
//! - VM86 IOPL<3: -> #GP(0) (CPL=3 > IOPL).
//!
//! STI also sets a one-instruction interrupt-blocking window: an externally
//! asserted IRQ that arrived before STI must NOT be delivered between STI
//! and the next instruction; it dispatches only on the boundary AFTER the
//! next instruction.

use common::Cpu as _;
use cpu::{CPU_MODEL_386, I386, I386State};

use super::setup::{
    HANDLER_GENERAL_PROTECTION_IP, INTERRUPT_DESCRIPTOR_TABLE_BASE,
    RIGHTS_RING0_CODE_READABLE_ACCESSED, RIGHTS_RING0_DATA_WRITABLE_ACCESSED, RING0_CODE_BASE,
    RING3_CODE_BASE, SELECTOR_RING0_CODE, TestBus, place_at, promote_to_ring3,
    setup_protected_mode_with_handlers, setup_vm86, setup_vm86_with_iopl, write_interrupt_gate_386,
};

const CLI_OPCODE: u8 = 0xFA;
const STI_OPCODE: u8 = 0xFB;
const NOP_OPCODE: u8 = 0x90;
const HLT_OPCODE: u8 = 0xF4;

fn make_cpu_386() -> I386<{ CPU_MODEL_386 }> {
    I386::<{ CPU_MODEL_386 }>::new()
}

fn install_vm86_gp_handler(bus: &mut TestBus, state: &mut I386State) {
    write_interrupt_gate_386(
        bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        13,
        HANDLER_GENERAL_PROTECTION_IP as u32,
        SELECTOR_RING0_CODE,
        0,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_GENERAL_PROTECTION_IP as u32) as usize] = HLT_OPCODE;
    state.gdt_limit = 5 * 8 - 1;
}

fn make_real_mode_state_at(cs_segment: u16, cs_base: u32) -> I386State {
    let mut state = I386State::default();
    state.set_cs(cs_segment);
    state.seg_bases[cpu::SegReg32::CS as usize] = cs_base;
    state.set_ss(0x2000);
    state.seg_bases[cpu::SegReg32::SS as usize] = 0x0002_0000;
    state.set_esp(0x1000);
    state.seg_limits = [0xFFFF; 6];
    state.seg_rights[cpu::SegReg32::CS as usize] = RIGHTS_RING0_CODE_READABLE_ACCESSED;
    state.seg_rights[cpu::SegReg32::SS as usize] = RIGHTS_RING0_DATA_WRITABLE_ACCESSED;
    state.seg_valid = [true; 6];
    state
}

// CLI tests.

#[test]
fn cli_real_mode_clears_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.if_flag = true;
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[CLI_OPCODE]);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag);
}

#[test]
fn cli_pm_cpl0_clears_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.if_flag = true;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag);
}

#[test]
fn cli_pm_cpl0_iopl0_succeeds() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.iopl = 0;
    state.flags.if_flag = true;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag);
}

#[test]
fn cli_pm_cpl3_iopl3_succeeds() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    state.flags.if_flag = true;
    state.flags.iopl = 3;
    cpu.load_state(&state);

    place_at(&mut bus, RING3_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag);
}

#[test]
fn cli_pm_cpl3_iopl2_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    state.flags.iopl = 2;
    cpu.load_state(&state);

    place_at(&mut bus, RING3_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn cli_pm_cpl3_iopl0_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    state.flags.iopl = 0;
    cpu.load_state(&state);

    place_at(&mut bus, RING3_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn cli_pm_cpl1_iopl0_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_cs(SELECTOR_RING0_CODE | 1);
    state.stored_cpl = 1;
    state.flags.iopl = 0;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn cli_pm_cpl1_iopl1_succeeds() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_cs(SELECTOR_RING0_CODE | 1);
    state.stored_cpl = 1;
    state.flags.iopl = 1;
    state.flags.if_flag = true;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[CLI_OPCODE]);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag, "CPL=IOPL boundary CLI succeeds");
}

#[test]
fn cli_vm86_iopl3_clears_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86(&mut bus);
    state.flags.if_flag = true;
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[CLI_OPCODE],
    );

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag);
}

#[test]
fn cli_vm86_iopl2_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 2);
    install_vm86_gp_handler(&mut bus, &mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[CLI_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn cli_vm86_iopl1_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 1);
    install_vm86_gp_handler(&mut bus, &mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[CLI_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn cli_vm86_iopl0_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 0);
    install_vm86_gp_handler(&mut bus, &mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[CLI_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn cli_does_not_change_unrelated_flags() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.if_flag = true;
    state.flags.df = true;
    state.flags.carry_val = 1;
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[CLI_OPCODE]);

    cpu.step(&mut bus);

    assert!(!cpu.state.flags.if_flag);
    assert!(cpu.state.flags.df);
    assert!(cpu.state.flags.cf());
}

// STI tests.

#[test]
fn sti_real_mode_sets_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let state = make_real_mode_state_at(0x1000, 0x0001_0000);
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[STI_OPCODE]);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.if_flag);
}

#[test]
fn sti_pm_cpl0_sets_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.flags.if_flag = false;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[STI_OPCODE]);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.if_flag);
}

#[test]
fn sti_pm_cpl3_iopl3_sets_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    state.flags.if_flag = false;
    state.flags.iopl = 3;
    cpu.load_state(&state);

    place_at(&mut bus, RING3_CODE_BASE, &[STI_OPCODE]);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.if_flag);
}

#[test]
fn sti_pm_cpl3_iopl2_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    state.flags.iopl = 2;
    cpu.load_state(&state);

    place_at(&mut bus, RING3_CODE_BASE, &[STI_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn sti_pm_cpl3_iopl0_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    state.flags.iopl = 0;
    cpu.load_state(&state);

    place_at(&mut bus, RING3_CODE_BASE, &[STI_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn sti_pm_cpl2_iopl1_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.set_cs(SELECTOR_RING0_CODE | 2);
    state.stored_cpl = 2;
    state.flags.iopl = 1;
    cpu.load_state(&state);

    place_at(&mut bus, RING0_CODE_BASE, &[STI_OPCODE]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn sti_vm86_iopl3_sets_if() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86(&mut bus);
    state.flags.if_flag = false;
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[STI_OPCODE],
    );

    cpu.step(&mut bus);

    assert!(cpu.state.flags.if_flag);
}

#[test]
fn sti_vm86_iopl2_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 2);
    install_vm86_gp_handler(&mut bus, &mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[STI_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn sti_vm86_iopl1_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 1);
    install_vm86_gp_handler(&mut bus, &mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[STI_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn sti_vm86_iopl0_raises_gp() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = setup_vm86_with_iopl(&mut bus, 0);
    install_vm86_gp_handler(&mut bus, &mut state);
    cpu.load_state(&state);

    place_at(
        &mut bus,
        state.seg_bases[cpu::SegReg32::CS as usize],
        &[STI_OPCODE],
    );

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), HANDLER_GENERAL_PROTECTION_IP as u32 + 1);
}

#[test]
fn sti_does_not_change_unrelated_flags() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.df = true;
    state.flags.carry_val = 1;
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[STI_OPCODE]);

    cpu.step(&mut bus);

    assert!(cpu.state.flags.if_flag);
    assert!(cpu.state.flags.df);
    assert!(cpu.state.flags.cf());
}

// STI-blocking window tests.

const TEST_IRQ_VECTOR: u8 = 0x40;
const IRQ_HANDLER_IP: u16 = 0xA000;

fn install_real_mode_irq_handler(bus: &mut TestBus, vector: u8, segment: u16, offset: u16) {
    bus.ram[(vector as usize) * 4] = offset as u8;
    bus.ram[(vector as usize) * 4 + 1] = (offset >> 8) as u8;
    bus.ram[(vector as usize) * 4 + 2] = segment as u8;
    bus.ram[(vector as usize) * 4 + 3] = (segment >> 8) as u8;
    let linear = ((segment as u32) << 4) + offset as u32;
    bus.ram[linear as usize] = HLT_OPCODE;
}

#[test]
fn sti_blocks_irq_for_one_instruction_then_dispatches() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.if_flag = false;
    cpu.load_state(&state);

    install_real_mode_irq_handler(&mut bus, TEST_IRQ_VECTOR, 0x2000, IRQ_HANDLER_IP);
    bus.irq_vector = TEST_IRQ_VECTOR;
    cpu.signal_irq();

    // [STI, NOP, INT3] - INT3 should never be reached because the IRQ
    // dispatches between NOP and INT3.
    place_at(&mut bus, 0x0001_0000, &[STI_OPCODE, NOP_OPCODE, 0xCC]);

    cpu.step(&mut bus); // STI
    assert!(cpu.state.flags.if_flag);
    let ip_after_sti = cpu.ip();
    assert_eq!(ip_after_sti, 1, "post-STI IP advanced past the STI byte");

    cpu.step(&mut bus); // NOP - IRQ must not yet dispatch
    assert!(!cpu.halted(), "IRQ blocked across the STI window");
    assert_eq!(
        cpu.ip(),
        2,
        "post-NOP IP advanced past the NOP byte (no IRQ dispatch yet)"
    );

    cpu.step(&mut bus); // IRQ should dispatch here, landing at handler HLT
    assert!(cpu.halted());
    assert_eq!(cpu.cs(), 0x2000);
    assert_eq!(cpu.ip() as u16, IRQ_HANDLER_IP + 1);
}

#[test]
fn cli_does_not_inhibit_next_instruction_irq() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.if_flag = true;
    cpu.load_state(&state);

    install_real_mode_irq_handler(&mut bus, TEST_IRQ_VECTOR, 0x2000, IRQ_HANDLER_IP);
    // IRQ raised AFTER CLI should remain pending; without IF=1 it cannot
    // dispatch even though no STI window is active.
    place_at(&mut bus, 0x0001_0000, &[CLI_OPCODE, NOP_OPCODE]);

    cpu.step(&mut bus); // CLI
    bus.irq_vector = TEST_IRQ_VECTOR;
    cpu.signal_irq();

    cpu.step(&mut bus); // NOP - CLI cleared IF, no IRQ delivery

    assert!(!cpu.halted(), "CLI clears IF, IRQ stays pending");
    assert_eq!(cpu.ip(), 2);
}

#[test]
fn sti_then_cli_does_not_dispatch_pending_irq() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.if_flag = false;
    cpu.load_state(&state);

    install_real_mode_irq_handler(&mut bus, TEST_IRQ_VECTOR, 0x2000, IRQ_HANDLER_IP);
    bus.irq_vector = TEST_IRQ_VECTOR;
    cpu.signal_irq();

    // STI raises IF and starts blocking window. CLI immediately clears IF.
    // The pending IRQ must NOT dispatch.
    place_at(
        &mut bus,
        0x0001_0000,
        &[STI_OPCODE, CLI_OPCODE, NOP_OPCODE, NOP_OPCODE],
    );

    cpu.step(&mut bus); // STI
    cpu.step(&mut bus); // CLI - blocking window expires here
    cpu.step(&mut bus); // NOP - IF=0 after CLI, IRQ stays pending
    cpu.step(&mut bus); // NOP - same

    assert!(
        !cpu.halted(),
        "CLI cleared IF before STI window opened delivery"
    );
    assert!(!cpu.state.flags.if_flag);
}

#[test]
fn sti_blocking_window_inhibits_only_one_following_instruction() {
    let mut cpu = make_cpu_386();
    let mut bus = TestBus::new();

    let mut state = make_real_mode_state_at(0x1000, 0x0001_0000);
    state.flags.if_flag = false;
    cpu.load_state(&state);

    install_real_mode_irq_handler(&mut bus, TEST_IRQ_VECTOR, 0x2000, IRQ_HANDLER_IP);
    bus.irq_vector = TEST_IRQ_VECTOR;
    cpu.signal_irq();

    // STI, NOP, NOP, INT3. IRQ must dispatch on the boundary BEFORE the second
    // NOP (i.e. after the first NOP completes).
    place_at(
        &mut bus,
        0x0001_0000,
        &[STI_OPCODE, NOP_OPCODE, NOP_OPCODE, 0xCC],
    );

    cpu.step(&mut bus); // STI
    cpu.step(&mut bus); // NOP #1 - blocking still active
    assert!(!cpu.halted());
    cpu.step(&mut bus); // IRQ dispatches here, landing at handler HLT
    assert!(cpu.halted());
    assert_eq!(cpu.ip() as u16, IRQ_HANDLER_IP + 1);
}

// Helper sanity tests.

#[test]
fn cli_sti_helper_install_real_mode_irq_handler_writes_ivt() {
    let mut bus = TestBus::new();
    install_real_mode_irq_handler(&mut bus, TEST_IRQ_VECTOR, 0x2000, 0x0050);
    let base = (TEST_IRQ_VECTOR as usize) * 4;
    assert_eq!(bus.ram[base], 0x50);
    assert_eq!(bus.ram[base + 2], 0x00);
    assert_eq!(bus.ram[base + 3], 0x20);
}

#[test]
fn cli_sti_helper_install_vm86_gp_handler_sets_present_bit() {
    let mut bus = TestBus::new();
    let mut state = setup_vm86_with_iopl(&mut bus, 0);
    install_vm86_gp_handler(&mut bus, &mut state);
    let address = (INTERRUPT_DESCRIPTOR_TABLE_BASE + 13 * 8) as usize;
    assert_eq!(bus.ram[address + 5] & 0x80, 0x80);
}
