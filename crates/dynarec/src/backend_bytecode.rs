//! Safe bytecode backend that interprets the IR against `I386State`.
//!
//! The backend is the correctness oracle for the native x86-64 backend:
//! everything it does, the interpreter already does. Each IR op mirrors
//! the matching sequence in `crates/cpu/src/i386/{alu, execute, modrm}.rs`.
//! The x86-64 backend uses the same semantic primitives, so the two
//! backends should produce bit-identical guest state on every opcode.

use common::Bus;
use cpu::{ByteReg, DwordReg, I386, I386State, SegReg32, WordReg};

use crate::ir::{
    AddrSize, AluOp, Block, BlockExit, CbwOp, FlagOp, GuestReg, IndirectTarget, IrCond, IrOp,
    LoopCond, MemOperand, RegOperand, RmDest, RmSource, ShiftCount, ShiftOp, Size, UnaryOp,
};

/// Why a block's execution stopped.
#[derive(Debug, Clone, Copy)]
pub enum BlockOutcome {
    /// Block exited normally; control returns to the dispatcher with
    /// `I386State::eip` set to the new EIP.
    Continue,
    /// Block could not start because its static cycle cost exceeds the
    /// remaining budget.
    Cycles,
    /// An unsupported instruction was encountered; the dispatcher must
    /// set EIP to `guest_eip` and single-step the interpreter.
    Fallback {
        /// EIP of the unsupported instruction.
        guest_eip: u32,
    },
    /// A fault was raised during block execution (e.g. #PF via
    /// `translate_linear` returning `None`). `fault_pending` on the CPU
    /// is set; the dispatcher lets the interpreter handle delivery.
    Fault,
    /// Guest `HLT` executed. The dispatcher must mark the CPU halted.
    Halt,
}

/// Runs `block` against the given state.
///
/// On entry the caller has already subtracted `block.cycles` from the
/// CPU's remaining budget (the whole-block charge). Returns the number
/// of IR ops that executed successfully before the block exited; the
/// dispatcher uses this to increment its jit-instrs counter.
pub fn execute_block<const CPU_MODEL: u8, B: Bus>(
    block: &Block,
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
) -> (BlockOutcome, u64) {
    // Clear any residual `fault_pending` left over from the previous
    // block's fault delivery (paging sets it to signal the backend and
    // leaves it set after `raise_fault_with_code` returns). Block
    // entry is the moral equivalent of the interpreter's per-step
    // clear at the top of `execute_one`.
    cpu.clear_fault_pending();
    // Refresh `prev_ip` so a fault raised by the first instruction in
    // this block (page fault in mem_read, V86 #GP in PUSHF, etc.)
    // reports the block-start EIP rather than whatever the last
    // interpreter fallback left behind. Ops past the first still see
    // a stale `prev_ip` — per-instruction refresh would need a
    // dedicated IR marker and is left as future work.
    cpu.jit_refresh_prev_ip();
    let mut executed = 0u64;
    // Track the expected EIP between ops. All in-block IR ops leave
    // EIP untouched except `InterpCall`, which advances EIP from
    // `instr_start_eip` to `instr_end_eip` by running the interpreter
    // for one instruction. A post-op EIP that differs from the
    // expected value means a fault was delivered inline (e.g. #GP
    // from `jit_popf` in V86 with IOPL<3, or #GP from an `IN`
    // dispatched via `InterpCall`). `raise_fault_with_code` clears
    // `fault_pending` once the handler is entered, so we cannot rely
    // on that flag alone; we also need to observe EIP divergence.
    // Bail before `apply_exit` overwrites the handler EIP with the
    // block's terminator target.
    let mut expected_eip = cpu.state.eip();
    for op in &block.ops {
        execute_op(op, cpu, bus, block.stack32);
        if cpu.fault_pending() {
            return (BlockOutcome::Fault, executed);
        }
        match op {
            IrOp::InterpCall { instr_end_eip, .. } => expected_eip = *instr_end_eip,
            IrOp::InstrStart { eip } => expected_eip = *eip,
            _ => {}
        }
        if cpu.state.eip() != expected_eip {
            return (BlockOutcome::Fault, executed);
        }
        executed += 1;
    }

    (apply_exit(block, cpu, bus), executed)
}

pub(crate) fn apply_exit<const CPU_MODEL: u8, B: Bus>(
    block: &Block,
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
) -> BlockOutcome {
    match block.exit {
        BlockExit::Fallthrough { next_eip } => {
            cpu.state.set_eip(next_eip);
            BlockOutcome::Continue
        }
        BlockExit::DirectJump { target_eip } => {
            cpu.state.set_eip(target_eip);
            BlockOutcome::Continue
        }
        BlockExit::ConditionalJump {
            cond,
            taken_eip,
            fallthrough_eip,
        } => {
            let target = if eval_cond(&cpu.state, cond) {
                taken_eip
            } else {
                fallthrough_eip
            };
            cpu.state.set_eip(target);
            BlockOutcome::Continue
        }
        BlockExit::CondLoop {
            cond,
            taken_eip,
            fallthrough_eip,
            addr_size,
        } => {
            let take = eval_loop(&mut cpu.state, cond, addr_size);
            let target = if take { taken_eip } else { fallthrough_eip };
            cpu.state.set_eip(target);
            BlockOutcome::Continue
        }
        BlockExit::DirectCall {
            target_eip,
            return_eip,
            size,
        } => {
            stack_push_value(&mut cpu.state, bus, return_eip, size, block.stack32);
            if cpu.fault_pending() {
                return BlockOutcome::Fault;
            }
            cpu.state.set_eip(target_eip);
            BlockOutcome::Continue
        }
        BlockExit::IndirectCall {
            target,
            return_eip,
            size,
        } => {
            let addr = resolve_indirect(cpu, bus, &target, size);
            if cpu.fault_pending() {
                return BlockOutcome::Fault;
            }
            stack_push_value(&mut cpu.state, bus, return_eip, size, block.stack32);
            if cpu.fault_pending() {
                return BlockOutcome::Fault;
            }
            set_near_eip_from_size(&mut cpu.state, addr, size);
            BlockOutcome::Continue
        }
        BlockExit::IndirectJump { target, size } => {
            let addr = resolve_indirect(cpu, bus, &target, size);
            if cpu.fault_pending() {
                return BlockOutcome::Fault;
            }
            set_near_eip_from_size(&mut cpu.state, addr, size);
            BlockOutcome::Continue
        }
        BlockExit::Return { size } => {
            let eip = stack_pop(&mut cpu.state, bus, size, block.stack32);
            if cpu.fault_pending() {
                return BlockOutcome::Fault;
            }
            set_near_eip_from_size(&mut cpu.state, eip, size);
            BlockOutcome::Continue
        }
        BlockExit::ReturnImm { size, extra_sp } => {
            let eip = stack_pop(&mut cpu.state, bus, size, block.stack32);
            if cpu.fault_pending() {
                return BlockOutcome::Fault;
            }
            if block.stack32 {
                let esp = cpu.state.regs.dword(DwordReg::ESP).wrapping_add(extra_sp);
                cpu.state.regs.set_dword(DwordReg::ESP, esp);
            } else {
                let sp = cpu
                    .state
                    .regs
                    .word(WordReg::SP)
                    .wrapping_add(extra_sp as u16);
                cpu.state.regs.set_word(WordReg::SP, sp);
            }
            set_near_eip_from_size(&mut cpu.state, eip, size);
            BlockOutcome::Continue
        }
        BlockExit::Halt { next_eip } => {
            cpu.state.set_eip(next_eip);
            BlockOutcome::Halt
        }
        BlockExit::Fallback { guest_eip } => BlockOutcome::Fallback { guest_eip },
    }
}

pub(crate) fn execute_op<const CPU_MODEL: u8, B: Bus>(
    op: &IrOp,
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    stack32: bool,
) {
    match op {
        IrOp::Nop => {}
        IrOp::MovImm { dst, imm } => write_reg(&mut cpu.state, dst, *imm),
        IrOp::MovReg { dst, src } => {
            let value = read_reg(&cpu.state, src);
            write_reg(&mut cpu.state, dst, value);
        }
        IrOp::MemMovImm { mem, imm, size } => {
            mem_write(cpu, bus, mem, *imm, *size);
        }
        IrOp::MemRead { dst, mem } => {
            let value = mem_read(cpu, bus, mem, dst.size);
            if cpu.fault_pending() {
                return;
            }
            write_reg(&mut cpu.state, dst, value);
        }
        IrOp::MemWrite { src, mem } => {
            let value = read_reg(&cpu.state, src);
            mem_write(cpu, bus, mem, value, src.size);
        }
        IrOp::Lea { dst, mem } => {
            let offset = compute_offset(&cpu.state, mem);
            write_reg(&mut cpu.state, dst, offset);
        }
        IrOp::Alu { dst, src, op: alu } => {
            let lhs = read_reg(&cpu.state, dst);
            let rhs = read_reg(&cpu.state, src);
            let result = alu_op(*alu, lhs, rhs, dst.size, &mut cpu.state.flags);
            if alu_writes_result(*alu) {
                write_reg(&mut cpu.state, dst, result);
            }
        }
        IrOp::AluImm { dst, imm, op: alu } => {
            let lhs = read_reg(&cpu.state, dst);
            let result = alu_op(*alu, lhs, *imm, dst.size, &mut cpu.state.flags);
            if alu_writes_result(*alu) {
                write_reg(&mut cpu.state, dst, result);
            }
        }
        IrOp::AluRm { dst, mem, op: alu } => {
            let lhs = read_reg(&cpu.state, dst);
            let rhs = mem_read(cpu, bus, mem, dst.size);
            if cpu.fault_pending() {
                return;
            }
            let result = alu_op(*alu, lhs, rhs, dst.size, &mut cpu.state.flags);
            if alu_writes_result(*alu) {
                write_reg(&mut cpu.state, dst, result);
            }
        }
        IrOp::MemAluReg {
            mem,
            src,
            size,
            op: alu,
        } => {
            let lhs = mem_read(cpu, bus, mem, *size);
            if cpu.fault_pending() {
                return;
            }
            let rhs = read_reg(&cpu.state, src);
            let result = alu_op(*alu, lhs, rhs, *size, &mut cpu.state.flags);
            if alu_writes_result(*alu) {
                mem_write(cpu, bus, mem, result, *size);
            }
        }
        IrOp::MemAluImm {
            mem,
            imm,
            size,
            op: alu,
        } => {
            let lhs = mem_read(cpu, bus, mem, *size);
            if cpu.fault_pending() {
                return;
            }
            let result = alu_op(*alu, lhs, *imm, *size, &mut cpu.state.flags);
            if alu_writes_result(*alu) {
                mem_write(cpu, bus, mem, result, *size);
            }
        }
        IrOp::Unary { dst, op: uop } => {
            let value = read_reg(&cpu.state, dst);
            let result = unary_op(*uop, value, dst.size, &mut cpu.state.flags);
            write_reg(&mut cpu.state, dst, result);
        }
        IrOp::MemUnary { mem, size, op: uop } => {
            let value = mem_read(cpu, bus, mem, *size);
            if cpu.fault_pending() {
                return;
            }
            let result = unary_op(*uop, value, *size, &mut cpu.state.flags);
            mem_write(cpu, bus, mem, result, *size);
        }
        IrOp::Shift {
            dst,
            size,
            count,
            op,
        } => execute_shift(cpu, bus, dst, *size, count, *op),
        IrOp::MovZx { dst, src, src_size } => {
            let value = read_rm_source(cpu, bus, src, *src_size);
            if cpu.fault_pending() {
                return;
            }
            write_reg(&mut cpu.state, dst, value);
        }
        IrOp::MovSx { dst, src, src_size } => {
            let raw = read_rm_source(cpu, bus, src, *src_size);
            if cpu.fault_pending() {
                return;
            }
            let value = sign_extend(raw, *src_size);
            write_reg(&mut cpu.state, dst, value);
        }
        IrOp::ImulRegRm { dst, src, size } => {
            let rhs = read_rm_source(cpu, bus, src, *size);
            if cpu.fault_pending() {
                return;
            }
            let lhs = read_reg(&cpu.state, dst);
            let (result, overflow) = imul(lhs, rhs, *size);
            write_reg(&mut cpu.state, dst, result);
            cpu.state.flags.carry_val = overflow as u32;
            cpu.state.flags.overflow_val = overflow as u32;
        }
        IrOp::ImulRegRmImm {
            dst,
            src,
            imm,
            size,
        } => {
            let lhs = read_rm_source(cpu, bus, src, *size);
            if cpu.fault_pending() {
                return;
            }
            let (result, overflow) = imul(lhs, *imm as u32, *size);
            write_reg(&mut cpu.state, dst, result);
            cpu.state.flags.carry_val = overflow as u32;
            cpu.state.flags.overflow_val = overflow as u32;
        }
        IrOp::PushReg { src } => {
            let value = read_reg(&cpu.state, src);
            stack_push_value(&mut cpu.state, bus, value, src.size, stack32);
        }
        IrOp::PushStackPtr { size } => {
            let value = match size {
                Size::Word => u32::from(cpu.state.regs.word(WordReg::SP)),
                Size::Dword => cpu.state.regs.dword(DwordReg::ESP),
                Size::Byte => unreachable!("byte PUSH does not exist on x86"),
            };
            stack_push_value(&mut cpu.state, bus, value, *size, stack32);
        }
        IrOp::PushImm { imm, size } => {
            stack_push_value(&mut cpu.state, bus, *imm, *size, stack32);
        }
        IrOp::PushMem { mem, size } => {
            let value = mem_read(cpu, bus, mem, *size);
            if cpu.fault_pending() {
                return;
            }
            stack_push_value(&mut cpu.state, bus, value, *size, stack32);
        }
        IrOp::PopReg { dst } => {
            let value = stack_pop(&mut cpu.state, bus, dst.size, stack32);
            if cpu.fault_pending() {
                return;
            }
            write_reg(&mut cpu.state, dst, value);
        }
        IrOp::PopMem { mem, size } => {
            let value = stack_pop(&mut cpu.state, bus, *size, stack32);
            if cpu.fault_pending() {
                return;
            }
            mem_write(cpu, bus, mem, value, *size);
        }
        IrOp::XchgReg { a, b } => {
            let va = read_reg(&cpu.state, a);
            let vb = read_reg(&cpu.state, b);
            write_reg(&mut cpu.state, a, vb);
            write_reg(&mut cpu.state, b, va);
        }
        IrOp::SetCc { dst, cond } => {
            let bit = if eval_cond(&cpu.state, *cond) { 1 } else { 0 };
            match dst {
                RmDest::Reg(reg) => write_reg(&mut cpu.state, reg, bit),
                RmDest::Mem { mem, size } => mem_write(cpu, bus, mem, bit, *size),
            }
        }
        IrOp::CbwCwd { op } => execute_cbw_cwd(&mut cpu.state, *op),
        IrOp::Flag(flag_op) => apply_flag_op(*flag_op, &mut cpu.state.flags),
        IrOp::PushAll { size } => execute_push_all(&mut cpu.state, bus, *size, stack32),
        IrOp::PopAll { size } => execute_pop_all(&mut cpu.state, bus, *size, stack32),
        IrOp::PushFlags { size } => execute_push_flags(cpu, bus, *size, stack32),
        IrOp::PopFlags { size } => execute_pop_flags(cpu, bus, *size, stack32),
        IrOp::Sahf => execute_sahf(&mut cpu.state),
        IrOp::Lahf => execute_lahf(&mut cpu.state),
        IrOp::MulAcc { src, size, signed } => {
            let value = read_rm_source(cpu, bus, src, *size);
            if cpu.fault_pending() {
                return;
            }
            cpu.jit_mul(value, size_bytes(*size), *signed);
        }
        IrOp::DivAcc { src, size, signed } => {
            let value = read_rm_source(cpu, bus, src, *size);
            if cpu.fault_pending() {
                return;
            }
            cpu.jit_div(value, size_bytes(*size), *signed, bus);
        }
        IrOp::Bswap { dst } => {
            let value = cpu.state.regs.dword(*dst);
            cpu.state.regs.set_dword(*dst, value.swap_bytes());
        }
        IrOp::InterpCall {
            instr_start_eip, ..
        } => {
            cpu.state.set_eip(*instr_start_eip);
            cpu.step_instruction(bus);
        }
        IrOp::InstrStart { eip } => {
            cpu.state.set_eip(*eip);
            cpu.jit_refresh_prev_ip();
        }
        IrOp::BitScan {
            dst,
            src,
            size,
            reverse,
        } => {
            let value = read_rm_source(cpu, bus, src, *size);
            if cpu.fault_pending() {
                return;
            }
            let masked = value & size.mask();
            if masked == 0 {
                cpu.state.flags.zero_val = 0;
            } else {
                cpu.state.flags.zero_val = 1;
                let index = if *reverse {
                    match size {
                        Size::Word => 15 - (masked as u16).leading_zeros(),
                        Size::Dword => 31 - masked.leading_zeros(),
                        Size::Byte => unreachable!("BSR has no byte form"),
                    }
                } else {
                    match size {
                        Size::Word => (masked as u16).trailing_zeros(),
                        Size::Dword => masked.trailing_zeros(),
                        Size::Byte => unreachable!("BSF has no byte form"),
                    }
                };
                write_reg(&mut cpu.state, dst, index);
            }
        }
    }
}

fn alu_writes_result(op: AluOp) -> bool {
    !matches!(op, AluOp::Cmp | AluOp::Test)
}

fn read_reg(state: &I386State, reg: &RegOperand) -> u32 {
    match reg.reg {
        GuestReg::Gpr(r) => match reg.size {
            Size::Byte => state.regs.byte(byte_reg_low(r)) as u32,
            Size::Word => state.regs.word(word_reg(r)) as u32,
            Size::Dword => state.regs.dword(r),
        },
        GuestReg::ByteHi(r) => state.regs.byte(byte_reg_high(r)) as u32,
    }
}

fn write_reg(state: &mut I386State, reg: &RegOperand, value: u32) {
    match reg.reg {
        GuestReg::Gpr(r) => match reg.size {
            Size::Byte => state.regs.set_byte(byte_reg_low(r), value as u8),
            Size::Word => state.regs.set_word(word_reg(r), value as u16),
            Size::Dword => state.regs.set_dword(r, value),
        },
        GuestReg::ByteHi(r) => state.regs.set_byte(byte_reg_high(r), value as u8),
    }
}

fn byte_reg_low(r: DwordReg) -> ByteReg {
    match r {
        DwordReg::EAX => ByteReg::AL,
        DwordReg::ECX => ByteReg::CL,
        DwordReg::EDX => ByteReg::DL,
        DwordReg::EBX => ByteReg::BL,
        _ => unreachable!("byte low alias only exists for EAX/ECX/EDX/EBX"),
    }
}

fn byte_reg_high(r: DwordReg) -> ByteReg {
    match r {
        DwordReg::EAX => ByteReg::AH,
        DwordReg::ECX => ByteReg::CH,
        DwordReg::EDX => ByteReg::DH,
        DwordReg::EBX => ByteReg::BH,
        _ => unreachable!("byte high alias only exists for EAX/ECX/EDX/EBX"),
    }
}

fn word_reg(r: DwordReg) -> WordReg {
    match r {
        DwordReg::EAX => WordReg::AX,
        DwordReg::ECX => WordReg::CX,
        DwordReg::EDX => WordReg::DX,
        DwordReg::EBX => WordReg::BX,
        DwordReg::ESP => WordReg::SP,
        DwordReg::EBP => WordReg::BP,
        DwordReg::ESI => WordReg::SI,
        DwordReg::EDI => WordReg::DI,
    }
}

fn compute_offset(state: &I386State, mem: &MemOperand) -> u32 {
    let mut offset: u32 = 0;
    if let Some(base) = mem.base {
        offset = offset.wrapping_add(state.regs.dword(base));
    }
    if let Some(index) = mem.index {
        offset = offset.wrapping_add(state.regs.dword(index) << mem.scale);
    }
    offset = offset.wrapping_add(mem.disp as u32);
    match mem.addr_size {
        AddrSize::Addr16 => offset & 0xFFFF,
        AddrSize::Addr32 => offset,
    }
}

fn mem_read<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    mem: &MemOperand,
    size: Size,
) -> u32 {
    let offset = compute_offset(&cpu.state, mem);
    match size {
        Size::Byte => cpu.read_byte_seg(bus, mem.seg, offset) as u32,
        Size::Word => cpu.read_word_seg(bus, mem.seg, offset) as u32,
        Size::Dword => cpu.read_dword_seg(bus, mem.seg, offset),
    }
}

fn mem_write<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    mem: &MemOperand,
    value: u32,
    size: Size,
) {
    let offset = compute_offset(&cpu.state, mem);
    match size {
        Size::Byte => cpu.write_byte_seg(bus, mem.seg, offset, value as u8),
        Size::Word => cpu.write_word_seg(bus, mem.seg, offset, value as u16),
        Size::Dword => cpu.write_dword_seg(bus, mem.seg, offset, value),
    }
}

fn read_rm_source<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    src: &RmSource,
    size: Size,
) -> u32 {
    match src {
        RmSource::Reg(reg) => read_reg(&cpu.state, reg),
        RmSource::Mem { mem, size: msize } => {
            debug_assert_eq!(*msize, size);
            mem_read(cpu, bus, mem, size)
        }
    }
}

fn sign_extend(value: u32, src_size: Size) -> u32 {
    match src_size {
        Size::Byte => value as i8 as i32 as u32,
        Size::Word => value as i16 as i32 as u32,
        Size::Dword => value,
    }
}

fn resolve_indirect<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    target: &IndirectTarget,
    size: Size,
) -> u32 {
    match target {
        IndirectTarget::Reg(r) => {
            let value = cpu.state.regs.dword(*r);
            match size {
                Size::Word => value & 0xFFFF,
                Size::Dword => value,
                Size::Byte => unreachable!("indirect jump cannot have byte width"),
            }
        }
        IndirectTarget::Mem(mem) => mem_read(cpu, bus, mem, size),
    }
}

fn alu_op(op: AluOp, lhs: u32, rhs: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    let lhs_m = lhs & size.mask();
    let rhs_m = rhs & size.mask();
    match op {
        AluOp::Add => add(lhs_m, rhs_m, size, flags),
        AluOp::Sub | AluOp::Cmp => sub(lhs_m, rhs_m, size, flags),
        AluOp::And | AluOp::Test => logic(lhs_m & rhs_m, size, flags),
        AluOp::Or => logic(lhs_m | rhs_m, size, flags),
        AluOp::Xor => logic(lhs_m ^ rhs_m, size, flags),
        AluOp::Adc => {
            let carry = flags.cf_val();
            add_with_carry(lhs_m, rhs_m, carry, size, flags)
        }
        AluOp::Sbb => {
            let borrow = flags.cf_val();
            sub_with_borrow(lhs_m, rhs_m, borrow, size, flags)
        }
    }
}

fn unary_op(op: UnaryOp, value: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    let value_m = value & size.mask();
    match op {
        UnaryOp::Inc => {
            let saved_cf = flags.carry_val;
            let result = add(value_m, 1, size, flags);
            flags.carry_val = saved_cf;
            result
        }
        UnaryOp::Dec => {
            let saved_cf = flags.carry_val;
            let result = sub(value_m, 1, size, flags);
            flags.carry_val = saved_cf;
            result
        }
        UnaryOp::Not => !value_m & size.mask(),
        UnaryOp::Neg => {
            let result = 0u32.wrapping_sub(value_m);
            match size {
                Size::Byte => {
                    flags.carry_val = u32::from(value_m != 0);
                    flags.set_of_sub_byte(result, value_m, 0);
                    flags.set_af(result, value_m, 0);
                    flags.set_szpf_byte(result);
                    result & 0xFF
                }
                Size::Word => {
                    flags.carry_val = u32::from(value_m != 0);
                    flags.set_of_sub_word(result, value_m, 0);
                    flags.set_af(result, value_m, 0);
                    flags.set_szpf_word(result);
                    result & 0xFFFF
                }
                Size::Dword => {
                    flags.carry_val = u32::from(value_m != 0);
                    flags.set_of_sub_dword(result, value_m, 0);
                    flags.set_af(result, value_m, 0);
                    flags.set_szpf_dword(result);
                    result
                }
            }
        }
    }
}

fn add(lhs: u32, rhs: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    match size {
        Size::Byte => {
            let full = lhs.wrapping_add(rhs);
            let result = full & 0xFF;
            flags.set_cf_byte(full);
            flags.set_szpf_byte(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_add_byte(full, rhs, lhs);
            result
        }
        Size::Word => {
            let full = lhs.wrapping_add(rhs);
            let result = full & 0xFFFF;
            flags.set_cf_word(full);
            flags.set_szpf_word(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_add_word(full, rhs, lhs);
            result
        }
        Size::Dword => {
            let full = (lhs as u64).wrapping_add(rhs as u64);
            let result = full as u32;
            flags.set_cf_dword(full);
            flags.set_szpf_dword(result);
            flags.set_af(result, rhs, lhs);
            flags.set_of_add_dword(result, rhs, lhs);
            result
        }
    }
}

fn sub(lhs: u32, rhs: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    match size {
        Size::Byte => {
            let full = lhs.wrapping_sub(rhs);
            let result = full & 0xFF;
            flags.set_cf_byte(full);
            flags.set_szpf_byte(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_sub_byte(full, rhs, lhs);
            result
        }
        Size::Word => {
            let full = lhs.wrapping_sub(rhs);
            let result = full & 0xFFFF;
            flags.set_cf_word(full);
            flags.set_szpf_word(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_sub_word(full, rhs, lhs);
            result
        }
        Size::Dword => {
            let result = lhs.wrapping_sub(rhs);
            flags.carry_val = u32::from(lhs < rhs);
            flags.set_of_sub_dword(result, rhs, lhs);
            flags.set_af(result, rhs, lhs);
            flags.set_szpf_dword(result);
            result
        }
    }
}

fn logic(result_raw: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    let result = result_raw & size.mask();
    flags.carry_val = 0;
    flags.overflow_val = 0;
    flags.aux_val = 0;
    match size {
        Size::Byte => flags.set_szpf_byte(result),
        Size::Word => flags.set_szpf_word(result),
        Size::Dword => flags.set_szpf_dword(result),
    }
    result
}

fn add_with_carry(lhs: u32, rhs: u32, carry: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    match size {
        Size::Byte => {
            let full = lhs.wrapping_add(rhs).wrapping_add(carry);
            let result = full & 0xFF;
            flags.set_cf_byte(full);
            flags.set_szpf_byte(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_add_byte(full, rhs, lhs);
            result
        }
        Size::Word => {
            let full = lhs.wrapping_add(rhs).wrapping_add(carry);
            let result = full & 0xFFFF;
            flags.set_cf_word(full);
            flags.set_szpf_word(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_add_word(full, rhs, lhs);
            result
        }
        Size::Dword => {
            let full = (lhs as u64)
                .wrapping_add(rhs as u64)
                .wrapping_add(carry as u64);
            let result = full as u32;
            flags.set_cf_dword(full);
            flags.set_szpf_dword(result);
            flags.set_af(result, rhs, lhs);
            flags.set_of_add_dword(result, rhs, lhs);
            result
        }
    }
}

fn sub_with_borrow(lhs: u32, rhs: u32, borrow: u32, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    match size {
        Size::Byte => {
            let full = lhs.wrapping_sub(rhs).wrapping_sub(borrow);
            let result = full & 0xFF;
            flags.set_cf_byte(full);
            flags.set_szpf_byte(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_sub_byte(full, rhs, lhs);
            result
        }
        Size::Word => {
            let full = lhs.wrapping_sub(rhs).wrapping_sub(borrow);
            let result = full & 0xFFFF;
            flags.set_cf_word(full);
            flags.set_szpf_word(result);
            flags.set_af(full, rhs, lhs);
            flags.set_of_sub_word(full, rhs, lhs);
            result
        }
        Size::Dword => {
            let result = lhs.wrapping_sub(rhs).wrapping_sub(borrow);
            flags.carry_val = u32::from((lhs as u64) < (rhs as u64 + borrow as u64));
            flags.set_of_sub_dword(result, rhs, lhs);
            flags.set_af(result, rhs, lhs);
            flags.set_szpf_dword(result);
            result
        }
    }
}

fn execute_shift<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    dst: &RmDest,
    size: Size,
    count: &ShiftCount,
    op: ShiftOp,
) {
    let count_val: u8 = match count {
        ShiftCount::Imm(n) => *n,
        ShiftCount::Cl => cpu.state.regs.byte(ByteReg::CL),
    };

    let lhs = match dst {
        RmDest::Reg(reg) => read_reg(&cpu.state, reg),
        RmDest::Mem { mem, size: msize } => {
            debug_assert_eq!(*msize, size);
            let v = mem_read(cpu, bus, mem, size);
            if cpu.fault_pending() {
                return;
            }
            v
        }
    };

    let result = shift_op(op, lhs, count_val, size, &mut cpu.state.flags);

    match dst {
        RmDest::Reg(reg) => write_reg(&mut cpu.state, reg, result),
        RmDest::Mem { mem, size: msize } => mem_write(cpu, bus, mem, result, *msize),
    }
}

fn shift_op(op: ShiftOp, val: u32, count: u8, size: Size, flags: &mut cpu::I386Flags) -> u32 {
    match size {
        Size::Byte => shift_byte(op, val as u8, count, flags) as u32,
        Size::Word => shift_word(op, val as u16, count, flags) as u32,
        Size::Dword => shift_dword(op, val, count, flags),
    }
}

fn shift_byte(op: ShiftOp, val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    match op {
        ShiftOp::Shl => alu_shl_byte(val, count, flags),
        ShiftOp::Shr => alu_shr_byte(val, count, flags),
        ShiftOp::Sar => alu_sar_byte(val, count, flags),
        ShiftOp::Rol => alu_rol_byte(val, count, flags),
        ShiftOp::Ror => alu_ror_byte(val, count, flags),
        ShiftOp::Rcl => alu_rcl_byte(val, count, flags),
        ShiftOp::Rcr => alu_rcr_byte(val, count, flags),
    }
}

fn shift_word(op: ShiftOp, val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    match op {
        ShiftOp::Shl => alu_shl_word(val, count, flags),
        ShiftOp::Shr => alu_shr_word(val, count, flags),
        ShiftOp::Sar => alu_sar_word(val, count, flags),
        ShiftOp::Rol => alu_rol_word(val, count, flags),
        ShiftOp::Ror => alu_ror_word(val, count, flags),
        ShiftOp::Rcl => alu_rcl_word(val, count, flags),
        ShiftOp::Rcr => alu_rcr_word(val, count, flags),
    }
}

fn shift_dword(op: ShiftOp, val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    match op {
        ShiftOp::Shl => alu_shl_dword(val, count, flags),
        ShiftOp::Shr => alu_shr_dword(val, count, flags),
        ShiftOp::Sar => alu_sar_dword(val, count, flags),
        ShiftOp::Rol => alu_rol_dword(val, count, flags),
        ShiftOp::Ror => alu_ror_dword(val, count, flags),
        ShiftOp::Rcl => alu_rcl_dword(val, count, flags),
        ShiftOp::Rcr => alu_rcr_dword(val, count, flags),
    }
}

fn alu_shl_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let result = if count < 8 {
        (val as u32) << count
    } else {
        0u32
    };
    flags.carry_val = if count <= 8 {
        ((val as u32) << (count - 1)) & 0x80
    } else if (count & 7) == 0 {
        ((val as u32) & 1) << 7
    } else {
        0
    };
    flags.overflow_val = (((result >> 7) & 1) ^ flags.cf_val()) * 0x80;
    flags.aux_val = 0;
    flags.set_szpf_byte(result);
    result as u8
}

fn alu_shl_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let result = if count < 16 {
        (val as u32) << count
    } else {
        0u32
    };
    flags.carry_val = if count <= 16 {
        ((val as u32) << (count - 1)) & 0x8000
    } else if (count & 15) == 0 {
        ((val as u32) & 1) << 15
    } else {
        0
    };
    flags.overflow_val = (((result >> 15) & 1) ^ flags.cf_val()) * 0x8000;
    flags.aux_val = 0;
    flags.set_szpf_word(result);
    result as u16
}

fn alu_shl_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let result = val << count;
    flags.carry_val = (val >> (32 - count)) & 1;
    flags.overflow_val = ((result >> 31) ^ flags.carry_val) & 1;
    flags.aux_val = 0;
    flags.set_szpf_dword(result);
    result
}

fn alu_shr_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    flags.overflow_val = if count == 1 { val as u32 & 0x80 } else { 0 };
    let result = if count < 8 {
        flags.carry_val = ((val >> (count - 1)) & 1) as u32;
        val >> count
    } else {
        flags.carry_val = if count == 8 || (count > 8 && (count & 7) == 0) {
            (val >> 7) as u32
        } else {
            0
        };
        0u8
    };
    flags.aux_val = 0;
    flags.set_szpf_byte(result as u32);
    result
}

fn alu_shr_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    flags.overflow_val = if count == 1 { val as u32 & 0x8000 } else { 0 };
    let result = if count < 16 {
        flags.carry_val = ((val >> (count - 1)) & 1) as u32;
        val >> count
    } else {
        flags.carry_val = if count == 16 || (count > 16 && (count & 15) == 0) {
            (val >> 15) as u32
        } else {
            0
        };
        0u16
    };
    flags.aux_val = 0;
    flags.set_szpf_word(result as u32);
    result
}

fn alu_shr_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    flags.overflow_val = if count == 1 { val & 0x8000_0000 } else { 0 };
    flags.carry_val = (val >> (count - 1)) & 1;
    let result = val >> count;
    flags.aux_val = 0;
    flags.set_szpf_dword(result);
    result
}

fn alu_sar_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    flags.overflow_val = 0;
    let signed = val as i8;
    let result = if count < 8 {
        flags.carry_val = ((signed >> (count - 1)) & 1) as u32;
        (signed >> count) as u8
    } else {
        flags.carry_val = if signed < 0 { 1 } else { 0 };
        (signed >> 7) as u8
    };
    flags.aux_val = 0;
    flags.set_szpf_byte(result as u32);
    result
}

fn alu_sar_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    flags.overflow_val = 0;
    let signed = val as i16;
    let result = if count < 16 {
        flags.carry_val = ((signed >> (count - 1)) & 1) as u32;
        (signed >> count) as u16
    } else {
        flags.carry_val = if signed < 0 { 1 } else { 0 };
        (signed >> 15) as u16
    };
    flags.aux_val = 0;
    flags.set_szpf_word(result as u32);
    result
}

fn alu_sar_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    flags.overflow_val = 0;
    let signed = val as i32;
    flags.carry_val = ((signed >> (count - 1)) & 1) as u32;
    let result = (signed >> count) as u32;
    flags.aux_val = 0;
    flags.set_szpf_dword(result);
    result
}

fn alu_rol_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count & 7;
    if count == 0 {
        count = 8;
    }
    let result = val.rotate_left(count as u32);
    flags.carry_val = (result & 1) as u32;
    flags.overflow_val = ((result >> 7) ^ (result & 1)) as u32 * 0x80;
    result
}

fn alu_rol_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count & 15;
    if count == 0 {
        count = 16;
    }
    let result = val.rotate_left(count as u32);
    flags.carry_val = (result & 1) as u32;
    flags.overflow_val = ((result >> 15) ^ (result & 1)) as u32 * 0x8000;
    result
}

fn alu_rol_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let result = val.rotate_left(count as u32);
    flags.carry_val = result & 1;
    flags.overflow_val = ((result >> 31) ^ flags.carry_val) & 1;
    result
}

fn alu_ror_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count & 7;
    if count == 0 {
        count = 8;
    }
    let result = val.rotate_right(count as u32);
    flags.carry_val = ((result >> 7) & 1) as u32;
    flags.overflow_val = ((result ^ (result << 1)) & 0x80) as u32;
    result
}

fn alu_ror_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count & 15;
    if count == 0 {
        count = 16;
    }
    let result = val.rotate_right(count as u32);
    flags.carry_val = ((result >> 15) & 1) as u32;
    flags.overflow_val = ((result ^ (result << 1)) & 0x8000) as u32;
    result
}

fn alu_ror_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let result = val.rotate_right(count as u32);
    flags.carry_val = (result >> 31) & 1;
    flags.overflow_val = ((result >> 31) ^ ((result >> 30) & 1)) & 1;
    result
}

fn alu_rcl_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count % 9;
    if count == 0 {
        count = 9;
    }
    let cf = flags.cf_val();
    let wide = (val as u32) | (cf << 8);
    let rotated = (wide << count) | (wide >> (9 - count));
    let result = rotated as u8;
    flags.carry_val = (rotated >> 8) & 1;
    flags.overflow_val = ((result as u32 >> 7) ^ flags.carry_val) * 0x80;
    result
}

fn alu_rcl_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count % 17;
    if count == 0 {
        count = 17;
    }
    let cf = flags.cf_val();
    let wide = (val as u32) | (cf << 16);
    let rotated = (wide << count) | (wide >> (17 - count));
    let result = rotated as u16;
    flags.carry_val = (rotated >> 16) & 1;
    flags.overflow_val = ((result as u32 >> 15) ^ flags.carry_val) * 0x8000;
    result
}

fn alu_rcl_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let cf = flags.cf_val() as u64;
    let mask = (1u64 << 33) - 1;
    let wide = (val as u64) | (cf << 32);
    let rotated = ((wide << count) | (wide >> (33 - count))) & mask;
    let result = rotated as u32;
    flags.carry_val = ((rotated >> 32) & 1) as u32;
    flags.overflow_val = ((result >> 31) ^ flags.carry_val) & 1;
    result
}

fn alu_rcr_byte(val: u8, count: u8, flags: &mut cpu::I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count % 9;
    if count == 0 {
        count = 9;
    }
    let cf = flags.cf_val();
    let wide = (val as u32) | (cf << 8);
    let result = ((wide >> count) | (wide << (9 - count))) as u8;
    flags.carry_val = (wide >> (count - 1)) & 1;
    flags.overflow_val = ((result ^ (result << 1)) & 0x80) as u32;
    result
}

fn alu_rcr_word(val: u16, count: u8, flags: &mut cpu::I386Flags) -> u16 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let mut count = count % 17;
    if count == 0 {
        count = 17;
    }
    let cf = flags.cf_val();
    let wide = (val as u32) | (cf << 16);
    let result = ((wide >> count) | (wide << (17 - count))) as u16;
    flags.carry_val = (wide >> (count - 1)) & 1;
    flags.overflow_val = (result as u32 ^ ((result as u32) << 1)) & 0x8000;
    result
}

fn alu_rcr_dword(val: u32, count: u8, flags: &mut cpu::I386Flags) -> u32 {
    let count = count & 0x1F;
    if count == 0 {
        return val;
    }
    let cf = flags.cf_val() as u64;
    let mask = (1u64 << 33) - 1;
    let wide = (val as u64) | (cf << 32);
    let rotated = ((wide >> count) | (wide << (33 - count))) & mask;
    let result = rotated as u32;
    flags.carry_val = ((wide >> (count - 1)) & 1) as u32;
    flags.overflow_val = ((result >> 31) ^ ((result >> 30) & 1)) & 1;
    result
}

fn imul(lhs: u32, rhs: u32, size: Size) -> (u32, bool) {
    match size {
        Size::Word => {
            let lhs_s = lhs as i16 as i32;
            let rhs_s = rhs as i16 as i32;
            let result = lhs_s * rhs_s;
            let overflow = !(-0x8000..=0x7FFF).contains(&result);
            (result as u16 as u32, overflow)
        }
        Size::Dword => {
            let lhs_s = lhs as i32 as i64;
            let rhs_s = rhs as i32 as i64;
            let result = lhs_s * rhs_s;
            let overflow = !(i32::MIN as i64..=i32::MAX as i64).contains(&result);
            (result as u32, overflow)
        }
        Size::Byte => unreachable!("IMUL byte is an F6 sub-op, not currently in JIT scope"),
    }
}

fn execute_cbw_cwd(state: &mut I386State, op: CbwOp) {
    match op {
        CbwOp::Cbw => {
            let al = state.regs.byte(ByteReg::AL) as i8;
            state.regs.set_word(WordReg::AX, al as i16 as u16);
        }
        CbwOp::Cwde => {
            let ax = state.regs.word(WordReg::AX) as i16;
            state.regs.set_dword(DwordReg::EAX, ax as i32 as u32);
        }
        CbwOp::Cwd => {
            let ax = state.regs.word(WordReg::AX) as i16;
            let dx = if ax < 0 { 0xFFFFu16 } else { 0 };
            state.regs.set_word(WordReg::DX, dx);
        }
        CbwOp::Cdq => {
            let eax = state.regs.dword(DwordReg::EAX) as i32;
            let edx = if eax < 0 { 0xFFFF_FFFFu32 } else { 0 };
            state.regs.set_dword(DwordReg::EDX, edx);
        }
    }
}

fn apply_flag_op(op: FlagOp, flags: &mut cpu::I386Flags) {
    match op {
        FlagOp::Clc => flags.carry_val = 0,
        FlagOp::Stc => flags.carry_val = 1,
        FlagOp::Cmc => flags.carry_val = if flags.cf() { 0 } else { 1 },
        FlagOp::Cld => flags.df = false,
        FlagOp::Std => flags.df = true,
    }
}

fn size_bytes(size: Size) -> u8 {
    match size {
        Size::Byte => 1,
        Size::Word => 2,
        Size::Dword => 4,
    }
}

fn execute_push_all<B: Bus>(state: &mut I386State, bus: &mut B, size: Size, stack32: bool) {
    if matches!(size, Size::Dword) {
        let snapshot = state.regs.dword(DwordReg::ESP);
        push_dword(state, bus, state.regs.dword(DwordReg::EAX), stack32);
        push_dword(state, bus, state.regs.dword(DwordReg::ECX), stack32);
        push_dword(state, bus, state.regs.dword(DwordReg::EDX), stack32);
        push_dword(state, bus, state.regs.dword(DwordReg::EBX), stack32);
        push_dword(state, bus, snapshot, stack32);
        push_dword(state, bus, state.regs.dword(DwordReg::EBP), stack32);
        push_dword(state, bus, state.regs.dword(DwordReg::ESI), stack32);
        push_dword(state, bus, state.regs.dword(DwordReg::EDI), stack32);
    } else {
        let snapshot = if stack32 {
            state.regs.dword(DwordReg::ESP) as u16
        } else {
            state.regs.word(WordReg::SP)
        };
        push_word(state, bus, state.regs.word(WordReg::AX), stack32);
        push_word(state, bus, state.regs.word(WordReg::CX), stack32);
        push_word(state, bus, state.regs.word(WordReg::DX), stack32);
        push_word(state, bus, state.regs.word(WordReg::BX), stack32);
        push_word(state, bus, snapshot, stack32);
        push_word(state, bus, state.regs.word(WordReg::BP), stack32);
        push_word(state, bus, state.regs.word(WordReg::SI), stack32);
        push_word(state, bus, state.regs.word(WordReg::DI), stack32);
    }
}

fn execute_pop_all<B: Bus>(state: &mut I386State, bus: &mut B, size: Size, stack32: bool) {
    if matches!(size, Size::Dword) {
        let edi = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::EDI, edi);
        let esi = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::ESI, esi);
        let ebp = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::EBP, ebp);
        let popped_esp = pop_dword(state, bus, stack32);
        if !stack32 {
            // POPAD quirk on 386/486 with 16-bit stack: the ESP slot is first
            // written in full, then SP is overwritten with the post-pop value
            // so the popped dword's upper half survives in ESP's upper bits.
            let sp_after = state.regs.word(WordReg::SP);
            let new_esp = (popped_esp & 0xFFFF_0000) | sp_after as u32;
            state.regs.set_dword(DwordReg::ESP, new_esp);
        }
        let ebx = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::EBX, ebx);
        let edx = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::EDX, edx);
        let ecx = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::ECX, ecx);
        let eax = pop_dword(state, bus, stack32);
        state.regs.set_dword(DwordReg::EAX, eax);
    } else {
        let di = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::DI, di);
        let si = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::SI, si);
        let bp = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::BP, bp);
        let _discard = pop_word(state, bus, stack32);
        let bx = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::BX, bx);
        let dx = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::DX, dx);
        let cx = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::CX, cx);
        let ax = pop_word(state, bus, stack32);
        state.regs.set_word(WordReg::AX, ax);
    }
}

fn execute_push_flags<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    size: Size,
    _stack32: bool,
) {
    cpu.jit_pushf(bus, size_bytes(size));
}

fn execute_pop_flags<const CPU_MODEL: u8, B: Bus>(
    cpu: &mut I386<CPU_MODEL>,
    bus: &mut B,
    size: Size,
    _stack32: bool,
) {
    cpu.jit_popf(bus, size_bytes(size));
}

fn execute_sahf(state: &mut I386State) {
    let ah = state.regs.byte(ByteReg::AH);
    state.flags.carry_val = (ah & 0x01) as u32;
    state.flags.parity_val = if ah & 0x04 != 0 { 0 } else { 1 };
    state.flags.aux_val = (ah & 0x10) as u32;
    state.flags.zero_val = if ah & 0x40 != 0 { 0 } else { 1 };
    state.flags.sign_val = if ah & 0x80 != 0 { -1 } else { 0 };
}

fn execute_lahf(state: &mut I386State) {
    let packed = state.flags.compress() as u8;
    state.regs.set_byte(ByteReg::AH, packed);
}

fn eval_cond(state: &I386State, cond: IrCond) -> bool {
    let f = &state.flags;
    match cond {
        IrCond::O => f.of(),
        IrCond::No => !f.of(),
        IrCond::B => f.cf(),
        IrCond::Nb => !f.cf(),
        IrCond::Z => f.zf(),
        IrCond::Nz => !f.zf(),
        IrCond::Be => f.cf() || f.zf(),
        IrCond::A => !f.cf() && !f.zf(),
        IrCond::S => f.sf(),
        IrCond::Ns => !f.sf(),
        IrCond::P => f.pf(),
        IrCond::Np => !f.pf(),
        IrCond::L => f.sf() != f.of(),
        IrCond::Ge => f.sf() == f.of(),
        IrCond::Le => f.zf() || (f.sf() != f.of()),
        IrCond::G => !f.zf() && (f.sf() == f.of()),
    }
}

fn eval_loop(state: &mut I386State, cond: LoopCond, addr_size: AddrSize) -> bool {
    let count_nonzero = match cond {
        LoopCond::Jcxz => match addr_size {
            AddrSize::Addr32 => state.regs.dword(DwordReg::ECX) == 0,
            AddrSize::Addr16 => state.regs.word(WordReg::CX) == 0,
        },
        LoopCond::Loopne | LoopCond::Loope | LoopCond::Loop => {
            let value = match addr_size {
                AddrSize::Addr32 => {
                    let v = state.regs.dword(DwordReg::ECX).wrapping_sub(1);
                    state.regs.set_dword(DwordReg::ECX, v);
                    v
                }
                AddrSize::Addr16 => {
                    let v = state.regs.word(WordReg::CX).wrapping_sub(1);
                    state.regs.set_word(WordReg::CX, v);
                    v as u32
                }
            };
            value != 0
        }
    };

    match cond {
        LoopCond::Jcxz => count_nonzero,
        LoopCond::Loop => count_nonzero,
        LoopCond::Loopne => count_nonzero && !state.flags.zf(),
        LoopCond::Loope => count_nonzero && state.flags.zf(),
    }
}

fn set_near_eip_from_size(state: &mut I386State, eip: u32, size: Size) {
    if matches!(size, Size::Dword) {
        state.set_eip(eip);
    } else {
        state.set_eip(eip & 0xFFFF);
    }
}

fn stack_push_value<B: Bus>(
    state: &mut I386State,
    bus: &mut B,
    value: u32,
    size: Size,
    stack32: bool,
) {
    match size {
        Size::Word => push_word(state, bus, value as u16, stack32),
        Size::Dword => push_dword(state, bus, value, stack32),
        Size::Byte => unreachable!("byte PUSH does not exist on x86"),
    }
}

fn stack_pop<B: Bus>(state: &mut I386State, bus: &mut B, size: Size, stack32: bool) -> u32 {
    match size {
        Size::Word => pop_word(state, bus, stack32) as u32,
        Size::Dword => pop_dword(state, bus, stack32),
        Size::Byte => unreachable!("byte POP does not exist on x86"),
    }
}

fn push_word<B: Bus>(state: &mut I386State, bus: &mut B, value: u16, stack32: bool) {
    let new_sp = if stack32 {
        let esp = state.regs.dword(DwordReg::ESP).wrapping_sub(2);
        state.regs.set_dword(DwordReg::ESP, esp);
        esp
    } else {
        let sp = state.regs.word(WordReg::SP).wrapping_sub(2);
        state.regs.set_word(WordReg::SP, sp);
        sp as u32
    };
    let linear = state.seg_bases[SegReg32::SS as usize].wrapping_add(new_sp);
    bus.write_word(linear, value);
}

fn push_dword<B: Bus>(state: &mut I386State, bus: &mut B, value: u32, stack32: bool) {
    let new_sp = if stack32 {
        let esp = state.regs.dword(DwordReg::ESP).wrapping_sub(4);
        state.regs.set_dword(DwordReg::ESP, esp);
        esp
    } else {
        let sp = state.regs.word(WordReg::SP).wrapping_sub(4);
        state.regs.set_word(WordReg::SP, sp);
        sp as u32
    };
    let linear = state.seg_bases[SegReg32::SS as usize].wrapping_add(new_sp);
    bus.write_dword(linear, value);
}

fn pop_word<B: Bus>(state: &mut I386State, bus: &mut B, stack32: bool) -> u16 {
    let sp = if stack32 {
        state.regs.dword(DwordReg::ESP)
    } else {
        state.regs.word(WordReg::SP) as u32
    };
    let linear = state.seg_bases[SegReg32::SS as usize].wrapping_add(sp);
    let value = bus.read_word(linear);
    if stack32 {
        state.regs.set_dword(DwordReg::ESP, sp.wrapping_add(2));
    } else {
        state
            .regs
            .set_word(WordReg::SP, (sp as u16).wrapping_add(2));
    }
    value
}

fn pop_dword<B: Bus>(state: &mut I386State, bus: &mut B, stack32: bool) -> u32 {
    let sp = if stack32 {
        state.regs.dword(DwordReg::ESP)
    } else {
        state.regs.word(WordReg::SP) as u32
    };
    let linear = state.seg_bases[SegReg32::SS as usize].wrapping_add(sp);
    let value = bus.read_dword(linear);
    if stack32 {
        state.regs.set_dword(DwordReg::ESP, sp.wrapping_add(4));
    } else {
        state
            .regs
            .set_word(WordReg::SP, (sp as u16).wrapping_add(4));
    }
    value
}
