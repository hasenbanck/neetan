//! Virtual-8086 mode entry/exit transitions.
//!
//! 80486 PRM Chapter 23 calls out two ways to enter V86 (task switch and
//! IRETD with CPL=0) and two ways to exit (task switch out, exception or
//! interrupt to PL0). Existing tests cover the IRETD entry path in
//! `iret.rs` (iret_pm_at_cpl0_with_eflags_vm_enters_vm86 and
//! iret_pm_at_cpl_nonzero_silently_strips_vm_bit). This file fills the
//! task-switch path and the V86 -> PL0 interrupt push details:
//!   - 23.3 / 23.3.1: task switch into i486 TSS with VM=1 enters V86 with
//!     segment registers loaded as 8086 selectors (cached base = sel<<4).
//!   - 23.3 / 23.3.1: task switch from V86 into i486 TSS with VM=0 exits
//!     V86 and loads protected-mode segment caches.
//!   - 23.3.1: V86 -> V86 task switch keeps VM=1 and reloads segments as
//!     8086 selectors.
//!   - 23.3.2 / Figure 23-3: V86 interrupt to PL0 pushes the V86 EFLAGS
//!     image with VM=1, and the processor clears DS/ES/FS/GS before the
//!     handler runs.

use common::Cpu as _;

use super::setup::{
    GLOBAL_DESCRIPTOR_TABLE_BASE, HANDLER_GENERAL_PROTECTION_IP, INTERRUPT_DESCRIPTOR_TABLE_BASE,
    RIGHTS_TSS_386_AVAILABLE, RIGHTS_TSS_386_BUSY, RING0_CODE_BASE, RING0_STACK_BASE,
    SELECTOR_PRIMARY_TSS, SELECTOR_RING0_CODE, SELECTOR_RING0_DATA, SELECTOR_RING0_STACK,
    SELECTOR_SECONDARY_TSS, SYSTEM_TYPE_TASK_GATE, TASK_STATE_SEGMENT_SECONDARY_BASE,
    TSS_MINIMUM_LIMIT, TestBus, Tss386Image, make_cpu_486, place_at, read_dword_at,
    setup_protected_mode_with_handlers, setup_vm86_with_iopl, write_gate_descriptor,
    write_interrupt_gate_386, write_segment_descriptor_16bit, write_tss_386,
};

/// Vector used by V86 -> task-switch tests. The setup helper does not
/// install an entry here, so each test installs its own task gate.
const VM86_TASK_GATE_VECTOR: u8 = 0x50;

const HLT_OPCODE: u8 = 0xF4;
const EFLAGS_VM_BIT: u32 = 0x0002_0000;
const EFLAGS_RESERVED_ALWAYS_SET: u32 = 0x0000_0002;

fn write_secondary_tss_image(bus: &mut TestBus, image: &Tss386Image) {
    write_tss_386(bus, TASK_STATE_SEGMENT_SECONDARY_BASE, image);
}

#[test]
fn task_switch_into_tss_with_vm_bit_set_enters_v86_and_loads_segments_as_8086_selectors() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_protected_mode_with_handlers(&mut bus);
    cpu.load_state(&state);

    let target_eip: u32 = 0x0000_0100;
    let target_image = Tss386Image {
        eip: target_eip,
        eflags: EFLAGS_VM_BIT | EFLAGS_RESERVED_ALWAYS_SET | (3 << 12),
        cs: 0x1000,
        ss: 0x2000,
        esp: 0x0000_FF00,
        ds: 0x4000,
        es: 0x3000,
        fs: 0x5000,
        gs: 0x6000,
        ..Tss386Image::default()
    };
    write_secondary_tss_image(&mut bus, &target_image);

    // JMP FAR ptr16:16 -> SELECTOR_SECONDARY_TSS triggers a task switch.
    // The shared setup leaves the ring-0 code segment 16-bit, so the encoding
    // is 5 bytes: opcode + 16-bit offset + 16-bit selector.
    place_at(
        &mut bus,
        RING0_CODE_BASE,
        &[
            0xEA,
            0x00,
            0x00,
            SELECTOR_SECONDARY_TSS as u8,
            (SELECTOR_SECONDARY_TSS >> 8) as u8,
        ],
    );

    cpu.step(&mut bus);

    assert_ne!(
        cpu.state.eflags_upper & EFLAGS_VM_BIT,
        0,
        "task switch to TSS with VM=1 must enter V86"
    );
    assert_eq!(cpu.cs(), target_image.cs);
    assert_eq!(cpu.ss(), target_image.ss);
    assert_eq!(cpu.es(), target_image.es);
    assert_eq!(cpu.ds(), target_image.ds);
    assert_eq!(cpu.fs(), target_image.fs);
    assert_eq!(cpu.gs(), target_image.gs);
    assert_eq!(
        cpu.state.seg_bases[cpu::SegReg32::CS as usize],
        (target_image.cs as u32) << 4,
        "V86 caches base = selector*16"
    );
    assert_eq!(
        cpu.state.seg_bases[cpu::SegReg32::DS as usize],
        (target_image.ds as u32) << 4
    );
}

#[test]
fn task_switch_from_v86_into_tss_with_vm_bit_clear_exits_v86_and_loads_protected_segments() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    // Set up two TSSes: a "running" V86 one and a secondary that's a
    // protected-mode return target. The PRM-documented way to leave V86
    // via task switch is INT n through a task gate (PRM 23.3.1), so the
    // test triggers the switch with INT 0x50 -> task gate.
    let state = setup_protected_mode_with_handlers(&mut bus);

    let return_eip: u32 = 0x0000_0200;
    let target_image = Tss386Image {
        eip: return_eip,
        eflags: EFLAGS_RESERVED_ALWAYS_SET,
        cs: SELECTOR_RING0_CODE,
        ss: SELECTOR_RING0_STACK,
        esp: 0x0000_F000,
        ds: SELECTOR_RING0_DATA,
        es: SELECTOR_RING0_DATA,
        ..Tss386Image::default()
    };
    write_secondary_tss_image(&mut bus, &target_image);

    let mut vm86_state = setup_vm86_with_iopl(&mut bus, 3);
    vm86_state.tr = SELECTOR_PRIMARY_TSS;
    vm86_state.tr_base = state.tr_base;
    vm86_state.tr_limit = TSS_MINIMUM_LIMIT;
    vm86_state.tr_rights = RIGHTS_TSS_386_BUSY;
    write_segment_descriptor_16bit(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        SELECTOR_PRIMARY_TSS >> 3,
        state.tr_base,
        TSS_MINIMUM_LIMIT as u16,
        RIGHTS_TSS_386_BUSY,
    );
    write_segment_descriptor_16bit(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        SELECTOR_SECONDARY_TSS >> 3,
        TASK_STATE_SEGMENT_SECONDARY_BASE,
        TSS_MINIMUM_LIMIT as u16,
        RIGHTS_TSS_386_AVAILABLE,
    );
    vm86_state.gdt_limit = 10 * 8 - 1;

    // Task gate at INT 0x50, DPL=3 so V86 software (CPL=3) can invoke it.
    write_gate_descriptor(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        VM86_TASK_GATE_VECTOR as u16,
        0,
        SELECTOR_SECONDARY_TSS,
        0,
        SYSTEM_TYPE_TASK_GATE,
        3,
    );

    cpu.load_state(&vm86_state);

    place_at(&mut bus, 0x0001_0000, &[0xCD, VM86_TASK_GATE_VECTOR]);

    cpu.step(&mut bus);

    assert_eq!(
        cpu.state.eflags_upper & EFLAGS_VM_BIT,
        0,
        "task switch to TSS with VM=0 must exit V86"
    );
    assert_eq!(cpu.cs(), SELECTOR_RING0_CODE);
    assert_eq!(cpu.ip(), return_eip);
}

#[test]
fn task_switch_from_v86_into_v86_tss_keeps_vm_bit_and_reloads_8086_segments() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    // Same scaffolding as the previous test, but the secondary TSS image
    // also has VM=1 so the destination is another V86 task.
    let state = setup_protected_mode_with_handlers(&mut bus);

    let target_image = Tss386Image {
        eip: 0x0000_0050,
        eflags: EFLAGS_VM_BIT | EFLAGS_RESERVED_ALWAYS_SET | (3 << 12),
        cs: 0x7000,
        ss: 0x8000,
        esp: 0x0000_FF00,
        ds: 0x9000,
        es: 0xA000,
        fs: 0xB000,
        gs: 0xC000,
        ..Tss386Image::default()
    };
    write_secondary_tss_image(&mut bus, &target_image);

    let mut vm86_state = setup_vm86_with_iopl(&mut bus, 3);
    vm86_state.tr = SELECTOR_PRIMARY_TSS;
    vm86_state.tr_base = state.tr_base;
    vm86_state.tr_limit = TSS_MINIMUM_LIMIT;
    vm86_state.tr_rights = RIGHTS_TSS_386_BUSY;
    write_segment_descriptor_16bit(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        SELECTOR_PRIMARY_TSS >> 3,
        state.tr_base,
        TSS_MINIMUM_LIMIT as u16,
        RIGHTS_TSS_386_BUSY,
    );
    write_segment_descriptor_16bit(
        &mut bus,
        GLOBAL_DESCRIPTOR_TABLE_BASE,
        SELECTOR_SECONDARY_TSS >> 3,
        TASK_STATE_SEGMENT_SECONDARY_BASE,
        TSS_MINIMUM_LIMIT as u16,
        RIGHTS_TSS_386_AVAILABLE,
    );
    vm86_state.gdt_limit = 10 * 8 - 1;

    write_gate_descriptor(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        VM86_TASK_GATE_VECTOR as u16,
        0,
        SELECTOR_SECONDARY_TSS,
        0,
        SYSTEM_TYPE_TASK_GATE,
        3,
    );

    cpu.load_state(&vm86_state);

    place_at(&mut bus, 0x0001_0000, &[0xCD, VM86_TASK_GATE_VECTOR]);

    cpu.step(&mut bus);

    assert_ne!(
        cpu.state.eflags_upper & EFLAGS_VM_BIT,
        0,
        "task switch from V86 to a V86 TSS keeps VM=1"
    );
    assert_eq!(cpu.cs(), target_image.cs);
    assert_eq!(
        cpu.state.seg_bases[cpu::SegReg32::CS as usize],
        (target_image.cs as u32) << 4
    );
    assert_eq!(
        cpu.state.seg_bases[cpu::SegReg32::DS as usize],
        (target_image.ds as u32) << 4
    );
}

#[test]
fn iret_to_outer_privilege_with_vm_bit_set_in_image_at_cpl_zero_enters_v86() {
    // Already covered by iret.rs::iret_pm_at_cpl0_with_eflags_vm_enters_vm86
    // and the surrounding suite (segment-cache base, EFLAGS, 9-dword pop).
    // Re-asserts the entry-via-IRETD path here as a Chapter 23 anchor for
    // the rest of this file.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.seg_granularity[cpu::SegReg32::SS as usize] = 0x40;
    state.seg_limits[cpu::SegReg32::SS as usize] = 0xFFFF_FFFF;
    cpu.load_state(&state);

    let sp = cpu.esp();
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp, 0x0100);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 4, 0x1000);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 8, 0x0002_3202);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 12, 0xFF00);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 16, 0x2000);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 20, 0x3000);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 24, 0x4000);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 28, 0x5000);
    super::setup::write_dword_at(&mut bus, RING0_STACK_BASE + sp + 32, 0x6000);

    place_at(&mut bus, RING0_CODE_BASE, &[0x66, 0xCF]);

    cpu.step(&mut bus);

    assert_ne!(cpu.state.eflags_upper & EFLAGS_VM_BIT, 0);
    assert_eq!(cpu.cs(), 0x1000);
}

#[test]
fn iret_to_outer_privilege_with_vm_bit_set_at_cpl_nonzero_ignores_vm_bit_or_raises_general_protection()
 {
    // Already covered by iret.rs::iret_pm_at_cpl_nonzero_silently_strips_vm_bit
    // (manual: only CPL=0 IRETD can enter V86; at CPL>0 the bit is silently
    // stripped). Re-asserted here as the Chapter 23 negative path.
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let mut state = setup_protected_mode_with_handlers(&mut bus);
    state.seg_granularity[cpu::SegReg32::SS as usize] = 0x40;
    state.seg_limits[cpu::SegReg32::SS as usize] = 0xFFFF_FFFF;
    super::setup::promote_to_ring3(&mut state);
    state.set_esp(0xFFE0);
    cpu.load_state(&state);

    let sp = cpu.esp();
    super::setup::write_dword_at(&mut bus, super::setup::RING3_STACK_BASE + sp, 0x0000_0100);
    super::setup::write_dword_at(
        &mut bus,
        super::setup::RING3_STACK_BASE + sp + 4,
        super::setup::SELECTOR_RING3_CODE as u32,
    );
    super::setup::write_dword_at(
        &mut bus,
        super::setup::RING3_STACK_BASE + sp + 8,
        EFLAGS_VM_BIT | EFLAGS_RESERVED_ALWAYS_SET,
    );

    place_at(&mut bus, super::setup::RING3_CODE_BASE, &[0x66, 0xCF]);

    cpu.step(&mut bus);

    assert_eq!(
        cpu.state.eflags_upper & EFLAGS_VM_BIT,
        0,
        "CPL>0 IRETD must NOT enter V86"
    );
}

#[test]
fn v86_interrupt_entry_to_ring0_handler_pushes_vm_flag_in_eflags_image_on_stack() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_vm86_with_iopl(&mut bus, 3);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        13,
        HANDLER_GENERAL_PROTECTION_IP as u32,
        SELECTOR_RING0_CODE,
        0,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_GENERAL_PROTECTION_IP as u32) as usize] = HLT_OPCODE;
    cpu.load_state(&state);

    // Trigger #GP via privileged INVD.
    place_at(&mut bus, 0x0001_0000, &[0x0F, 0x08]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());

    // Stack frame on PL0 stack for #GP (which carries an error code):
    // ERROR_CODE, EIP, CS, EFLAGS, ESP, SS, ES, DS, FS, GS (PRM Fig. 23-3
    // "with error code"). EFLAGS sits 12 bytes above the new ESP.
    let pl0_stack_linear =
        cpu.state.seg_bases[cpu::SegReg32::SS as usize] + cpu.state.regs.dword(cpu::DwordReg::ESP);
    let pushed_eflags = read_dword_at(&bus, pl0_stack_linear + 12);
    assert_ne!(
        pushed_eflags & EFLAGS_VM_BIT,
        0,
        "interrupt frame from V86 must record VM=1 in EFLAGS image"
    );
}

#[test]
fn v86_interrupt_entry_to_ring0_handler_clears_ds_es_fs_gs() {
    let mut cpu = make_cpu_486();
    let mut bus = TestBus::new();

    let state = setup_vm86_with_iopl(&mut bus, 3);
    write_interrupt_gate_386(
        &mut bus,
        INTERRUPT_DESCRIPTOR_TABLE_BASE,
        13,
        HANDLER_GENERAL_PROTECTION_IP as u32,
        SELECTOR_RING0_CODE,
        0,
    );
    bus.ram[(RING0_CODE_BASE + HANDLER_GENERAL_PROTECTION_IP as u32) as usize] = HLT_OPCODE;
    cpu.load_state(&state);

    place_at(&mut bus, 0x0001_0000, &[0x0F, 0x08]);

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ds(), 0, "DS cleared on V86 -> PL0 interrupt entry");
    assert_eq!(cpu.es(), 0, "ES cleared on V86 -> PL0 interrupt entry");
    assert_eq!(cpu.fs(), 0, "FS cleared on V86 -> PL0 interrupt entry");
    assert_eq!(cpu.gs(), 0, "GS cleared on V86 -> PL0 interrupt entry");
}
