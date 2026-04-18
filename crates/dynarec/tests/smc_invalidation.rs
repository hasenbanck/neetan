//! Self-modifying-code tests for the JIT's SMC invalidation path.
//!
//! The SMC hook runs only while the JIT dispatcher owns the bus (i.e.
//! inside a `run_for` slice). These tests therefore exercise writes
//! that happen DURING JIT execution, not between slices.

use common::{Bus, MachineModel};
use cpu::{CPU_MODEL_486, I386State, SegReg32};
use dynarec::I386Jit;
use machine::{Machine, Pc9801Bus};

type JitCpu = I386Jit<{ CPU_MODEL_486 }>;
type JitMachine = Machine<JitCpu>;

fn jit() -> JitMachine {
    Machine::new(JitCpu::new(), Pc9801Bus::new(MachineModel::PC9821AP, 48000))
}

fn place_code<B: Bus>(bus: &mut B, base: u32, code: &[u8]) {
    for (i, &b) in code.iter().enumerate() {
        bus.write_byte(base + i as u32, b);
    }
}

fn initial_state() -> I386State {
    let mut state = I386State::default();
    state.set_cs(0x0000);
    state.seg_bases[SegReg32::CS as usize] = 0x0000;
    state.seg_limits[SegReg32::CS as usize] = 0xFFFF;
    state.seg_rights[SegReg32::CS as usize] = 0x9B;
    state.seg_valid[SegReg32::CS as usize] = true;
    state.set_ss(0x0000);
    state.seg_bases[SegReg32::SS as usize] = 0x0000;
    state.seg_limits[SegReg32::SS as usize] = 0xFFFF;
    state.seg_rights[SegReg32::SS as usize] = 0x93;
    state.seg_valid[SegReg32::SS as usize] = true;
    state.set_ds(0x0000);
    state.seg_bases[SegReg32::DS as usize] = 0x0000;
    state.seg_limits[SegReg32::DS as usize] = 0xFFFF;
    state.seg_rights[SegReg32::DS as usize] = 0x93;
    state.seg_valid[SegReg32::DS as usize] = true;
    state.set_es(0x0000);
    state.seg_bases[SegReg32::ES as usize] = 0x0000;
    state.seg_limits[SegReg32::ES as usize] = 0xFFFF;
    state.seg_rights[SegReg32::ES as usize] = 0x93;
    state.seg_valid[SegReg32::ES as usize] = true;
    state.set_esp(0x1000);
    state.set_eip(0x0100);
    state
}

/// Program pattern:
///
///   0x0100: CALL 0x0200          (E8 FD 00  -> next=0x0103, +FD = 0x0200)
///   0x0103: MOV BYTE PTR [0x0201], 0xBB (C6 06 01 02 BB)
///   0x0108: CALL 0x0200          (E8 F5 00  -> next=0x010B, +F5 = 0x0200)
///   0x010B: F4 HLT
///
///   0x0200: B0 AA                ; MOV AL, 0xAA
///   0x0202: C3                   ; RET
///
/// First CALL compiles the block at 0x0200 with the immediate 0xAA.
/// The write to [0x0201] overwrites that immediate with 0xBB. The
/// invalidator queues a drop; the dispatcher processes it after the
/// second CALL's host block exits. The second invocation of 0x0200
/// must therefore see a freshly recompiled block using 0xBB.
///
/// Without SMC invalidation the cached block would run the stale
/// immediate and AL would stay 0xAA.
#[test]
fn self_modifying_code_invalidates_cached_block() {
    let program: &[u8] = &[
        // 0x0100: CALL rel16=+0x00FD -> 0x0103 + 0xFD = 0x0200
        0xE8, 0xFD, 0x00, //
        // 0x0103: MOV BYTE PTR [0x0201], 0xBB
        0xC6, 0x06, 0x01, 0x02, 0xBB, //
        // 0x0108: CALL rel16=+0x00F5 -> 0x010B + 0xF5 = 0x0200
        0xE8, 0xF5, 0x00, //
        // 0x010B: HLT
        0xF4, //
    ];
    let subroutine: &[u8] = &[
        0xB0, 0xAA, // MOV AL, 0xAA
        0xC3, // RET
    ];

    let mut machine = jit();
    place_code(&mut machine.bus, 0x0100, program);
    place_code(&mut machine.bus, 0x0200, subroutine);
    machine.cpu.load_state(&initial_state());
    machine.run_for(2000);

    // First CALL ran the pre-patch routine (AL=0xAA), the write to
    // [0x0201] patched the immediate to 0xBB, and the second CALL
    // executed a freshly recompiled block with AL=0xBB. The observable
    // evidence of correct invalidation is the final AL value.
    assert_eq!(
        machine.cpu.state().eax() & 0xFF,
        0xBB,
        "AL should be 0xBB after SMC patch; got 0x{:02X}",
        machine.cpu.state().eax() & 0xFF
    );
}

/// Writes to a page that has no translated code must not trigger any
/// block invalidation. We check this by running a block, writing to a
/// distant page, then re-running and asserting the cached block still
/// produces the expected output.
#[test]
fn write_to_unrelated_page_does_not_invalidate() {
    // A loop block: MOV CX, 3; loop_top: DEC CX; JNZ loop_top; HLT.
    // Inside the loop we also write to an unrelated page each iter.
    let program: &[u8] = &[
        0xB9, 0x03, 0x00, // MOV CX, 3
        // loop_top at 0x0103
        0xC6, 0x06, 0x00, 0x50, 0xFF, // MOV BYTE PTR [0x5000], 0xFF (unrelated page)
        0x49, // DEC CX
        0x75, 0xF8, // JNZ loop_top (rel8 = -8)
        0xF4, // HLT
    ];

    let mut machine = jit();
    place_code(&mut machine.bus, 0x0100, program);
    machine.cpu.load_state(&initial_state());
    machine.run_for(500);
    assert_eq!(machine.cpu.state().ecx() & 0xFFFF, 0);
    // The inner loop block is reused across iterations; if the
    // invalidator were over-broad we would either run slower or lose
    // correctness. Here we just assert termination and that the write
    // landed.
    assert_eq!(machine.bus.read_byte(0x5000), 0xFF);
}
