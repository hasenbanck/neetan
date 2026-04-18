//! Oracle tests: run the same program on both the interpreter
//! (`Pc9821Ap`) and the JIT-backed CPU (`Pc9821ApJit`), then compare
//! the resulting architectural state.
//!
//! Any divergence is a JIT bug.

use common::{Bus, MachineModel};
use cpu::{CPU_MODEL_486, DwordReg, I386, I386State, SegReg32};
use dynarec::{I386Jit, JitBackend, JitStats};
use machine::{Machine, Pc9801Bus};

type InterpMachine = Machine<I386<{ CPU_MODEL_486 }>>;
type JitMachine = Machine<I386Jit<{ CPU_MODEL_486 }>>;

fn interpreter() -> InterpMachine {
    Machine::new(
        I386::<{ CPU_MODEL_486 }>::new(),
        Pc9801Bus::new(MachineModel::PC9821AP, 48000),
    )
}

fn jit() -> JitMachine {
    jit_with_backend(JitBackend::Auto)
}

fn jit_with_backend(backend: JitBackend) -> JitMachine {
    Machine::new(
        I386Jit::<{ CPU_MODEL_486 }>::new_with_backend(backend),
        Pc9801Bus::new(MachineModel::PC9821AP, 48000),
    )
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

fn load_program(machine_interp: &mut InterpMachine, machine_jit: &mut JitMachine, program: &[u8]) {
    place_code(&mut machine_interp.bus, 0x0100, program);
    place_code(&mut machine_jit.bus, 0x0100, program);
    let state = initial_state();
    machine_interp.cpu.load_state(&state);
    machine_jit.cpu.load_state(&state);
}

fn run_both(program: &[u8], cycles: u64) -> (I386State, I386State) {
    let mut interp = interpreter();
    let mut jit_m = jit();
    load_program(&mut interp, &mut jit_m, program);
    interp.run_for(cycles);
    jit_m.run_for(cycles);
    (interp.cpu.state.clone(), jit_m.cpu.state().clone())
}

#[cfg(all(target_arch = "x86_64", unix))]
fn run_jit_backend(program: &[u8], cycles: u64, backend: JitBackend) -> (I386State, JitStats) {
    let mut machine = jit_with_backend(backend);
    place_code(&mut machine.bus, 0x0100, program);
    let state = initial_state();
    machine.cpu.load_state(&state);
    machine.run_for(cycles);
    (machine.cpu.state().clone(), machine.cpu.stats())
}

fn assert_state_eq(interp: &I386State, jit: &I386State, label: &str) {
    // Compare the architecturally-visible GPRs, EIP, segment registers,
    // and eflags. Other fields (seg_bases / seg_rights caches) are
    // derived from the selectors; if the selectors match and the code
    // never reloads segments, the caches must match too.
    for (i, &reg) in [
        DwordReg::EAX,
        DwordReg::ECX,
        DwordReg::EDX,
        DwordReg::EBX,
        DwordReg::ESP,
        DwordReg::EBP,
        DwordReg::ESI,
        DwordReg::EDI,
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            interp.regs.dword(reg),
            jit.regs.dword(reg),
            "{label}: GPR[{i}]={reg:?} mismatch (interp=0x{:08X} jit=0x{:08X})",
            interp.regs.dword(reg),
            jit.regs.dword(reg)
        );
    }
    assert_eq!(interp.eip(), jit.eip(), "{label}: EIP mismatch");
    assert_eq!(
        interp.flags.compress(),
        jit.flags.compress(),
        "{label}: FLAGS mismatch"
    );
    for seg in [
        SegReg32::ES,
        SegReg32::CS,
        SegReg32::SS,
        SegReg32::DS,
        SegReg32::FS,
        SegReg32::GS,
    ] {
        assert_eq!(
            interp.sregs[seg as usize], jit.sregs[seg as usize],
            "{label}: segment {seg:?} mismatch",
        );
    }
}

#[test]
fn mov_add_sequence() {
    // MOV EAX, 0x12345678 ; ADD EAX, 0x11111111 ; HLT
    let program: &[u8] = &[
        0x66, 0xB8, 0x78, 0x56, 0x34, 0x12, // MOV EAX, 0x12345678
        0x66, 0x05, 0x11, 0x11, 0x11, 0x11, // ADD EAX, 0x11111111
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "mov_add_sequence");
    assert_eq!(interp.eax(), 0x23456789);
}

#[test]
fn sub_and_cmp_flags() {
    // MOV AX, 0x0010 ; SUB AX, 0x0011 ; MOV CX, AX ; HLT
    // (in 16-bit real mode)
    let program: &[u8] = &[
        0xB8, 0x10, 0x00, // MOV AX, 0x0010
        0x2D, 0x11, 0x00, // SUB AX, 0x0011
        0x89, 0xC1, // MOV CX, AX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "sub_and_cmp_flags");
    // AX = 0xFFFF (wrap), CX mirrors.
    assert_eq!(interp.eax() & 0xFFFF, 0xFFFF);
    assert_eq!(interp.ecx() & 0xFFFF, 0xFFFF);
    assert!(interp.flags.cf());
    assert!(interp.flags.sf());
}

#[test]
fn inc_dec_loop() {
    // MOV CX, 5
    // loop_top: DEC CX ; JNZ loop_top
    // MOV AX, CX ; HLT
    let program: &[u8] = &[
        0xB9, 0x05, 0x00, // MOV CX, 5
        // loop_top at 0x0103
        0x49, // DEC CX
        0x75, 0xFD, // JNZ loop_top (rel8 = -3)
        0x89, 0xC8, // MOV AX, CX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 500);
    assert_state_eq(&interp, &jit, "inc_dec_loop");
    assert_eq!(interp.ecx() & 0xFFFF, 0);
    assert_eq!(interp.eax() & 0xFFFF, 0);
}

#[test]
fn push_pop_round_trip() {
    // MOV AX, 0x1234 ; PUSH AX ; MOV AX, 0x5678 ; POP AX ; HLT
    let program: &[u8] = &[
        0xB8, 0x34, 0x12, // MOV AX, 0x1234
        0x50, // PUSH AX
        0xB8, 0x78, 0x56, // MOV AX, 0x5678
        0x58, // POP AX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 500);
    assert_state_eq(&interp, &jit, "push_pop_round_trip");
    assert_eq!(interp.eax() & 0xFFFF, 0x1234);
}

#[test]
fn call_ret_round_trip() {
    // MOV AX, 0 ; CALL sub ; HLT
    // sub: MOV AX, 0xBEEF ; RET
    //
    // offsets:
    //   0x0100: B8 00 00        ; MOV AX, 0
    //   0x0103: E8 03 00        ; CALL +3 -> 0x0109
    //   0x0106: F4              ; HLT
    //   0x0107: <pad>
    //   0x0108: <pad>
    //   0x0109: B8 EF BE        ; MOV AX, 0xBEEF
    //   0x010C: C3              ; RET
    let program: &[u8] = &[
        0xB8, 0x00, 0x00, // MOV AX, 0  (0x0100)
        0xE8, 0x03, 0x00, // CALL rel16=+3 (next is 0x0106; +3 = 0x0109)
        0xF4, // HLT                  (0x0106)
        0x90, 0x90, // pad                   (0x0107..0x0109)
        0xB8, 0xEF, 0xBE, // MOV AX, 0xBEEF (0x0109)
        0xC3, // RET                   (0x010C)
    ];
    let (interp, jit) = run_both(program, 500);
    assert_state_eq(&interp, &jit, "call_ret_round_trip");
    assert_eq!(interp.eax() & 0xFFFF, 0xBEEF);
}

#[test]
fn unsupported_instruction_falls_back() {
    // Use REP MOVSB which is outside the supported opcode set (string
    // ops and the REP prefix are not yet decoded). The JIT must emit
    // Fallback and delegate to the interpreter, producing identical
    // state.
    // MOV CX, 4 ; MOV SI, 0x0500 ; MOV DI, 0x0600
    // CLD ; REP MOVSB ; HLT
    //
    // Source data at 0x0500 = 0xAA 0xBB 0xCC 0xDD
    let program: &[u8] = &[
        0xB9, 0x04, 0x00, // MOV CX, 4
        0xBE, 0x00, 0x05, // MOV SI, 0x0500
        0xBF, 0x00, 0x06, // MOV DI, 0x0600
        0xFC, // CLD
        0xF3, 0xA4, // REP MOVSB
        0xF4, // HLT
    ];

    let mut interp = interpreter();
    let mut jit_m = jit();
    load_program(&mut interp, &mut jit_m, program);

    // Seed the source data.
    let src: [u8; 4] = [0xAA, 0xBB, 0xCC, 0xDD];
    for (i, &b) in src.iter().enumerate() {
        interp.bus.write_byte(0x0500 + i as u32, b);
        jit_m.bus.write_byte(0x0500 + i as u32, b);
    }

    interp.run_for(500);
    jit_m.run_for(500);

    assert_state_eq(
        &interp.cpu.state,
        jit_m.cpu.state(),
        "unsupported_instruction_falls_back",
    );

    // Verify the memcpy actually happened on both.
    for i in 0..4 {
        let a = interp.bus.read_byte(0x0600 + i);
        let b = jit_m.bus.read_byte(0x0600 + i);
        assert_eq!(a, b, "destination byte {i} differs");
        assert_eq!(a, src[i as usize], "destination byte {i} wrong");
    }
}

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn bytecode_and_x64_backends_match() {
    let program: &[u8] = &[
        0x66, 0xB8, 0x78, 0x56, 0x34, 0x12, // MOV EAX, 0x12345678
        0x66, 0x05, 0x11, 0x11, 0x11, 0x11, // ADD EAX, 0x11111111
        0x66, 0x50, // PUSH EAX
        0x66, 0x59, // POP ECX
        0xF4, // HLT
    ];

    let (bytecode_state, _) = run_jit_backend(program, 500, JitBackend::Bytecode);
    let (x64_state, _) = run_jit_backend(program, 500, JitBackend::X64);
    assert_state_eq(
        &bytecode_state,
        &x64_state,
        "bytecode_and_x64_backends_match",
    );
}

#[test]
fn pusha_popa_round_trip() {
    // MOV AX,0x1111; MOV CX,0x2222; MOV DX,0x3333; MOV BX,0x4444
    // MOV BP,0x5555; MOV SI,0x6666; MOV DI,0x7777; PUSHA
    // XOR AX,AX; XOR CX,CX; XOR DX,DX; XOR BX,BX
    // XOR BP,BP; XOR SI,SI; XOR DI,DI; POPA; HLT
    let program: &[u8] = &[
        0xB8, 0x11, 0x11, // MOV AX, 0x1111
        0xB9, 0x22, 0x22, // MOV CX, 0x2222
        0xBA, 0x33, 0x33, // MOV DX, 0x3333
        0xBB, 0x44, 0x44, // MOV BX, 0x4444
        0xBD, 0x55, 0x55, // MOV BP, 0x5555
        0xBE, 0x66, 0x66, // MOV SI, 0x6666
        0xBF, 0x77, 0x77, // MOV DI, 0x7777
        0x60, // PUSHA
        0x31, 0xC0, // XOR AX,AX
        0x31, 0xC9, // XOR CX,CX
        0x31, 0xD2, // XOR DX,DX
        0x31, 0xDB, // XOR BX,BX
        0x31, 0xED, // XOR BP,BP
        0x31, 0xF6, // XOR SI,SI
        0x31, 0xFF, // XOR DI,DI
        0x61, // POPA
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 1000);
    assert_state_eq(&interp, &jit, "pusha_popa_round_trip");
    assert_eq!(interp.eax() & 0xFFFF, 0x1111);
    assert_eq!(interp.ecx() & 0xFFFF, 0x2222);
    assert_eq!(interp.edx() & 0xFFFF, 0x3333);
    assert_eq!(interp.ebx() & 0xFFFF, 0x4444);
    assert_eq!(interp.ebp() & 0xFFFF, 0x5555);
    assert_eq!(interp.esi() & 0xFFFF, 0x6666);
    assert_eq!(interp.edi() & 0xFFFF, 0x7777);
}

#[test]
fn pushf_popf_round_trip() {
    // STC (set CF) ; PUSHF ; CLC (clear CF) ; POPF ; HLT
    let program: &[u8] = &[
        0xF9, // STC
        0x9C, // PUSHF
        0xF8, // CLC
        0x9D, // POPF
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "pushf_popf_round_trip");
    // After restoring FLAGS from the stack, CF must be set again.
    assert!(interp.flags.cf());
}

#[test]
fn lahf_sahf_round_trip() {
    // STC ; LAHF (AH <- flags) ; CLC ; SAHF (restore flags) ; HLT
    let program: &[u8] = &[
        0xF9, // STC
        0x9F, // LAHF
        0xF8, // CLC
        0x9E, // SAHF
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "lahf_sahf_round_trip");
    assert!(interp.flags.cf());
}

#[test]
fn leave_restores_bp_sp() {
    // Simulate an ENTER-compatible prologue then LEAVE:
    //   PUSH BP ; MOV BP, SP ; SUB SP, 8 ; <work> ; LEAVE ; HLT
    let program: &[u8] = &[
        0xBD, 0x44, 0x44, // MOV BP, 0x4444 (sentinel)
        0x55, // PUSH BP         (push sentinel)
        0x89, 0xE5, // MOV BP, SP      (establish frame)
        0x83, 0xEC, 0x08, // SUB SP, 8       (allocate locals)
        // <body would go here>
        0xC9, // LEAVE           (SP<-BP, POP BP)
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 500);
    assert_state_eq(&interp, &jit, "leave_restores_bp_sp");
    // Both must see BP restored to the sentinel.
    assert_eq!(interp.ebp() & 0xFFFF, 0x4444);
}

#[test]
fn neg_not_unary() {
    // MOV AX, 0x1234 ; NEG AX ; NOT AX ; HLT
    // After NEG: AX = 0 - 0x1234 = 0xEDCC, CF=1, SF=1
    // After NOT: AX = !0xEDCC = 0x1233
    let program: &[u8] = &[
        0xB8, 0x34, 0x12, // MOV AX, 0x1234
        0xF7, 0xD8, // NEG AX (F7 /3)
        0xF7, 0xD0, // NOT AX (F7 /2)
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "neg_not_unary");
    assert_eq!(interp.eax() & 0xFFFF, 0x1233);
}

#[test]
fn mul_unsigned_16bit() {
    // MOV AX, 0x1234 ; MOV BX, 0x5678 ; MUL BX ; HLT
    // DX:AX = 0x1234 * 0x5678 = 0x0626_0060
    let program: &[u8] = &[
        0xB8, 0x34, 0x12, // MOV AX, 0x1234
        0xBB, 0x78, 0x56, // MOV BX, 0x5678
        0xF7, 0xE3, // MUL BX (F7 /4)
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "mul_unsigned_16bit");
    assert_eq!(interp.eax() & 0xFFFF, 0x0060);
    assert_eq!(interp.edx() & 0xFFFF, 0x0626);
    assert!(interp.flags.cf());
    assert!(interp.flags.of());
}

#[test]
fn imul_signed_16bit() {
    // MOV AX, 0xFFFF (-1) ; MOV BX, 0x0002 ; IMUL BX ; HLT
    // DX:AX = -2 = 0xFFFF_FFFE
    let program: &[u8] = &[
        0xB8, 0xFF, 0xFF, // MOV AX, 0xFFFF
        0xBB, 0x02, 0x00, // MOV BX, 2
        0xF7, 0xEB, // IMUL BX (F7 /5)
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "imul_signed_16bit");
    assert_eq!(interp.eax() & 0xFFFF, 0xFFFE);
    assert_eq!(interp.edx() & 0xFFFF, 0xFFFF);
    // For small signed result sign-extendable into AX, CF/OF should be 0.
    assert!(!interp.flags.cf());
    assert!(!interp.flags.of());
}

#[test]
fn div_unsigned_16bit() {
    // MOV DX, 0 ; MOV AX, 100 ; MOV BX, 7 ; DIV BX ; HLT
    // AX = 14, DX = 2
    let program: &[u8] = &[
        0xBA, 0x00, 0x00, // MOV DX, 0
        0xB8, 0x64, 0x00, // MOV AX, 100
        0xBB, 0x07, 0x00, // MOV BX, 7
        0xF7, 0xF3, // DIV BX (F7 /6)
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "div_unsigned_16bit");
    assert_eq!(interp.eax() & 0xFFFF, 14);
    assert_eq!(interp.edx() & 0xFFFF, 2);
}

#[test]
fn bswap_reverses_byte_order() {
    // MOV EAX, 0x11223344 ; BSWAP EAX ; HLT
    // After BSWAP, EAX = 0x44332211.
    let program: &[u8] = &[
        0x66, 0xB8, 0x44, 0x33, 0x22, 0x11, // MOV EAX, 0x11223344
        0x0F, 0xC8, // BSWAP EAX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "bswap_reverses_byte_order");
    assert_eq!(interp.eax(), 0x44332211);
}

#[test]
fn bsf_finds_lowest_set_bit() {
    // MOV AX, 0x0018 ; BSF CX, AX ; HLT
    // Lowest set bit of 0x0018 (0b11000) is bit 3.
    let program: &[u8] = &[
        0xB8, 0x18, 0x00, // MOV AX, 0x0018
        0x0F, 0xBC, 0xC8, // BSF CX, AX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "bsf_finds_lowest_set_bit");
    assert_eq!(interp.ecx() & 0xFFFF, 3);
    assert!(!interp.flags.zf());
}

#[test]
fn bsr_finds_highest_set_bit() {
    // MOV AX, 0x0018 ; BSR CX, AX ; HLT
    // Highest set bit of 0x0018 (0b11000) is bit 4.
    let program: &[u8] = &[
        0xB8, 0x18, 0x00, // MOV AX, 0x0018
        0x0F, 0xBD, 0xC8, // BSR CX, AX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "bsr_finds_highest_set_bit");
    assert_eq!(interp.ecx() & 0xFFFF, 4);
    assert!(!interp.flags.zf());
}

#[test]
fn bsf_zero_source_sets_zf() {
    // MOV AX, 0 ; BSF CX, AX ; HLT
    // Source is zero; ZF=1, CX undefined (but interpreter preserves).
    let program: &[u8] = &[
        0xB8, 0x00, 0x00, // MOV AX, 0
        0x0F, 0xBC, 0xC8, // BSF CX, AX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "bsf_zero_source_sets_zf");
    assert!(interp.flags.zf());
}

#[test]
fn bt_reg_reg_copies_bit_to_cf() {
    // MOV AX, 0x0008 ; MOV CX, 3 ; BT AX, CX ; HLT
    // Bit 3 of AX is 1 (AX=0b1000), so CF=1.
    let program: &[u8] = &[
        0xB8, 0x08, 0x00, // MOV AX, 0x0008
        0xB9, 0x03, 0x00, // MOV CX, 3
        0x0F, 0xA3, 0xC8, // BT AX, CX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "bt_reg_reg_copies_bit_to_cf");
    assert!(interp.flags.cf());
}

#[test]
fn bts_reg_imm_sets_bit() {
    // MOV AX, 0 ; BTS AX, 5 ; HLT
    // Set bit 5 -> AX=0x0020, prior bit was 0 -> CF=0.
    let program: &[u8] = &[
        0xB8, 0x00, 0x00, // MOV AX, 0
        0x0F, 0xBA, 0xE8, 0x05, // BTS AX, 5
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "bts_reg_imm_sets_bit");
    assert_eq!(interp.eax() & 0xFFFF, 0x0020);
    assert!(!interp.flags.cf());
}

#[test]
fn btr_reg_imm_clears_bit() {
    // MOV AX, 0xFFFF ; BTR AX, 3 ; HLT
    // Clear bit 3 -> AX=0xFFF7, prior bit was 1 -> CF=1.
    let program: &[u8] = &[
        0xB8, 0xFF, 0xFF, // MOV AX, 0xFFFF
        0x0F, 0xBA, 0xF0, 0x03, // BTR AX, 3
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "btr_reg_imm_clears_bit");
    assert_eq!(interp.eax() & 0xFFFF, 0xFFF7);
    assert!(interp.flags.cf());
}

#[test]
fn btc_reg_imm_toggles_bit() {
    // MOV AX, 0x0100 ; BTC AX, 0 ; BTC AX, 8 ; HLT
    // After BTC 0: AX=0x0101 (set), CF=0. After BTC 8: AX=0x0001, CF=1.
    let program: &[u8] = &[
        0xB8, 0x00, 0x01, // MOV AX, 0x0100
        0x0F, 0xBA, 0xF8, 0x00, // BTC AX, 0
        0x0F, 0xBA, 0xF8, 0x08, // BTC AX, 8
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "btc_reg_imm_toggles_bit");
    assert_eq!(interp.eax() & 0xFFFF, 0x0001);
    assert!(interp.flags.cf());
}

#[test]
fn shld_imm_concatenates_and_shifts_left() {
    // MOV AX, 0x1234 ; MOV BX, 0xABCD ; SHLD AX, BX, 4 ; HLT
    // AX = low 16 of (AX<<4 | BX>>12) = low 16 of (0x12340 | 0xA) = 0x234A
    let program: &[u8] = &[
        0xB8, 0x34, 0x12, // MOV AX, 0x1234
        0xBB, 0xCD, 0xAB, // MOV BX, 0xABCD
        0x0F, 0xA4, 0xD8, 0x04, // SHLD AX, BX, 4
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "shld_imm_concatenates_and_shifts_left");
    assert_eq!(interp.eax() & 0xFFFF, 0x234A);
}

#[test]
fn shrd_imm_concatenates_and_shifts_right() {
    // MOV AX, 0x1234 ; MOV BX, 0xABCD ; SHRD AX, BX, 4 ; HLT
    // AX = low 16 of (BX<<12 | AX>>4) = low 16 of (0xDCAB0 | 0x0123) = 0xD123
    let program: &[u8] = &[
        0xB8, 0x34, 0x12, // MOV AX, 0x1234
        0xBB, 0xCD, 0xAB, // MOV BX, 0xABCD
        0x0F, 0xAC, 0xD8, 0x04, // SHRD AX, BX, 4
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "shrd_imm_concatenates_and_shifts_right");
    assert_eq!(interp.eax() & 0xFFFF, 0xD123);
}

#[test]
fn xadd_reg_reg_swaps_and_adds() {
    // MOV AX, 3 ; MOV BX, 5 ; XADD AX, BX ; HLT
    // temp=AX=3; AX = AX+BX = 8; BX = temp = 3. Final: AX=8, BX=3.
    let program: &[u8] = &[
        0xB8, 0x03, 0x00, // MOV AX, 3
        0xBB, 0x05, 0x00, // MOV BX, 5
        0x0F, 0xC1, 0xD8, // XADD AX, BX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "xadd_reg_reg_swaps_and_adds");
    assert_eq!(interp.eax() & 0xFFFF, 8);
    assert_eq!(interp.ebx() & 0xFFFF, 3);
}

#[test]
fn cmpxchg_reg_reg_equal() {
    // MOV AX, 7 ; MOV CX, 7 ; MOV DX, 99 ; CMPXCHG CX, DX ; HLT
    // AL==CL (wait, word form so AX==CX): equal -> CX=DX=99, ZF=1, AX unchanged.
    let program: &[u8] = &[
        0xB8, 0x07, 0x00, // MOV AX, 7
        0xB9, 0x07, 0x00, // MOV CX, 7
        0xBA, 0x63, 0x00, // MOV DX, 99
        0x0F, 0xB1, 0xD1, // CMPXCHG CX, DX
        0xF4, // HLT
    ];
    let (interp, jit) = run_both(program, 200);
    assert_state_eq(&interp, &jit, "cmpxchg_reg_reg_equal");
    assert_eq!(interp.ecx() & 0xFFFF, 99);
    assert_eq!(interp.eax() & 0xFFFF, 7);
    assert!(interp.flags.zf());
}

#[test]
fn xlat_reads_table_byte() {
    // MOV AL, 2 ; MOV BX, 0x0500 ; XLAT ; HLT
    // Reads [DS:0x0502]. We plant a byte there up front.
    let program: &[u8] = &[
        0xB0, 0x02, // MOV AL, 2
        0xBB, 0x00, 0x05, // MOV BX, 0x0500
        0xD7, // XLAT
        0xF4, // HLT
    ];
    let mut interp = interpreter();
    let mut jit_m = jit();
    load_program(&mut interp, &mut jit_m, program);
    interp.bus.write_byte(0x0502, 0xAB);
    jit_m.bus.write_byte(0x0502, 0xAB);
    interp.run_for(500);
    jit_m.run_for(500);
    assert_state_eq(
        &interp.cpu.state,
        jit_m.cpu.state(),
        "xlat_reads_table_byte",
    );
    assert_eq!(interp.cpu.state.eax() & 0xFF, 0xAB);
}

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn x64_backend_executes_compiled_block() {
    let program: &[u8] = &[
        0x66, 0xB8, 0x78, 0x56, 0x34, 0x12, // MOV EAX, 0x12345678
        0x66, 0x05, 0x11, 0x11, 0x11, 0x11, // ADD EAX, 0x11111111
        0xF4, // HLT
    ];

    let (_, stats) = run_jit_backend(program, 200, JitBackend::X64);
    assert!(
        stats.blocks_executed > 0,
        "x64 backend executed zero compiled blocks"
    );
    assert!(
        stats.jit_instrs_executed > 0,
        "x64 backend executed zero compiled IR ops"
    );
}
