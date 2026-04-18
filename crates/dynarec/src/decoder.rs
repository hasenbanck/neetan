//! Instruction decoder for the JIT front end.
//!
//! Decodes guest instructions straight from physical linear memory and
//! lowers them into IR. Three outcomes per opcode:
//!
//! * Integer-subset opcodes lower to dedicated `IrOp` variants.
//! * Complex opcodes whose semantics carry too much prefix / address
//!   / segment state (BT family, SHLD/SHRD, XADD, CMPXCHG, IN/OUT,
//!   XLAT, ENTER) lower to [`IrOp::InterpCall`] so the interpreter
//!   runs a single instruction mid-block without splitting it.
//! * Anything still unhandled (FPU, string ops, privileged ops, far
//!   control transfers, segment-register loads) terminates the block
//!   with [`BlockExit::Fallback`] so the dispatcher can single-step
//!   the interpreter.
//!
//! The decoder deliberately mirrors the interpreter's prefix / operand-size
//! / address-size conventions (see `crates/cpu/src/i386/modrm.rs` for the
//! effective-address reference) so that fallback transitions are seamless.

use common::Bus;
use cpu::{DwordReg, I386, SegReg32};

use crate::ir::{
    AddrSize, AluOp, Block, BlockExit, CbwOp, FlagOp, GuestReg, IndirectTarget, IrCond, IrOp,
    LoopCond, MemOperand, RegOperand, RmDest, RmSource, ShiftCount, ShiftOp, Size, UnaryOp,
};

/// Maximum guest bytes we allow a block to cover. This is a safety cap;
/// the hard constraint is that blocks live inside a single 4 KiB
/// physical page (enforced by `crosses_page` below), so the cap is
/// never actually reached in practice.
const MAX_BLOCK_BYTES: u32 = 4096;

/// Returns `true` when the linear address `phys` lies in a different
/// 4 KiB page than `start_phys`. The decoder must stop compiling a
/// block as soon as this becomes true, because page-crossings are not
/// guaranteed to map to contiguous physical memory in the guest.
fn crosses_page(start_phys: u32, phys: u32) -> bool {
    (start_phys & !0xFFF) != (phys & !0xFFF)
}

/// Decodes a block of guest instructions starting at `start_phys`
/// (physical linear address) / `start_eip` (guest logical EIP).
pub fn decode_block<const CPU_MODEL: u8, B: Bus>(
    cpu: &I386<CPU_MODEL>,
    bus: &mut B,
    start_phys: u32,
    start_eip: u32,
    mut ops: Vec<IrOp>,
) -> Block {
    let code32 = cpu.is_code_segment_32bit();
    let stack32 = cpu.is_stack_segment_32bit();
    ops.clear();
    let mut ctx = DecodeCtx {
        phys: start_phys,
        eip: start_eip,
        code32,
        stack32,
        operand_size_override: false,
        address_size_override: false,
        seg_override: None,
        lock: false,
        bytes_emitted: 0,
        instrs_emitted: 0,
        cycles: 0,
        ops,
    };

    loop {
        let at_cap = ctx.instrs_emitted >= Block::MAX_INSTRS
            || ctx.bytes_emitted >= MAX_BLOCK_BYTES
            || crosses_page(start_phys, ctx.phys);
        if at_cap {
            break Block {
                phys_addr: start_phys,
                cycles: ctx.cycles,
                ops: ctx.ops,
                exit: BlockExit::Fallthrough { next_eip: ctx.eip },
                stack32,
            };
        }

        let instr_start_eip = ctx.eip;
        let instr_start_phys = ctx.phys;
        let instr_start_bytes = ctx.bytes_emitted;
        let instr_start_ops = ctx.ops.len();
        // Emit an InstrStart marker so the backend can refresh guest
        // `ip`/`prev_ip` to this instruction's EIP before running its
        // ops. Without this, a fault raised by a later-in-block op
        // would push the block's first-instruction EIP as the return
        // address, and the handler would miscompare against the
        // expected faulting EIP.
        ctx.ops.push(IrOp::InstrStart {
            eip: instr_start_eip,
        });
        match decode_one(cpu, bus, &mut ctx, instr_start_eip) {
            DecodeResult::Continued => {
                if crosses_page(start_phys, ctx.phys.wrapping_sub(1)) {
                    ctx.phys = instr_start_phys;
                    ctx.eip = instr_start_eip;
                    ctx.bytes_emitted = instr_start_bytes;
                    ctx.ops.truncate(instr_start_ops);
                    break Block {
                        phys_addr: start_phys,
                        cycles: ctx.cycles,
                        ops: ctx.ops,
                        exit: BlockExit::Fallback {
                            guest_eip: instr_start_eip,
                        },
                        stack32,
                    };
                }
                ctx.operand_size_override = false;
                ctx.address_size_override = false;
                ctx.seg_override = None;
                ctx.lock = false;
                ctx.instrs_emitted += 1;
            }
            DecodeResult::Terminated(exit) => {
                break Block {
                    phys_addr: start_phys,
                    cycles: ctx.cycles,
                    ops: ctx.ops,
                    exit,
                    stack32,
                };
            }
        }
    }
}

struct DecodeCtx {
    phys: u32,
    eip: u32,
    code32: bool,
    stack32: bool,
    operand_size_override: bool,
    address_size_override: bool,
    seg_override: Option<SegReg32>,
    lock: bool,
    bytes_emitted: u32,
    instrs_emitted: u16,
    cycles: i64,
    ops: Vec<IrOp>,
}

impl DecodeCtx {
    fn operand_size(&self) -> Size {
        if self.code32 ^ self.operand_size_override {
            Size::Dword
        } else {
            Size::Word
        }
    }

    fn addr_size(&self) -> AddrSize {
        if self.code32 ^ self.address_size_override {
            AddrSize::Addr32
        } else {
            AddrSize::Addr16
        }
    }

    fn push_pop_size(&self) -> Size {
        self.operand_size()
    }

    fn apply_seg_override(&self, default: SegReg32) -> SegReg32 {
        self.seg_override.unwrap_or(default)
    }
}

enum DecodeResult {
    Continued,
    Terminated(BlockExit),
}

fn fetch_byte<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx) -> u8 {
    let byte = bus.read_byte(ctx.phys);
    ctx.phys = ctx.phys.wrapping_add(1);
    ctx.eip = ctx.eip.wrapping_add(1);
    ctx.bytes_emitted += 1;
    byte
}

fn fetch_u16<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx) -> u16 {
    let lo = fetch_byte(bus, ctx) as u16;
    let hi = fetch_byte(bus, ctx) as u16;
    lo | (hi << 8)
}

fn fetch_u32<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx) -> u32 {
    let lo = fetch_u16(bus, ctx) as u32;
    let hi = fetch_u16(bus, ctx) as u32;
    lo | (hi << 16)
}

fn fallback(ctx: &DecodeCtx, instr_start_eip: u32) -> DecodeResult {
    let _ = ctx;
    DecodeResult::Terminated(BlockExit::Fallback {
        guest_eip: instr_start_eip,
    })
}

fn near_eip_for_size(eip: u32, size: Size) -> u32 {
    match size {
        Size::Word => eip & 0xFFFF,
        Size::Dword => eip,
        Size::Byte => unreachable!("byte-sized near control transfer"),
    }
}

fn near_eip_for_code_size(eip: u32, code32: bool) -> u32 {
    if code32 { eip } else { eip & 0xFFFF }
}

fn gpr_reg(idx: u8, size: Size) -> RegOperand {
    let dword = DwordReg::from_index(idx);
    match size {
        Size::Byte => {
            if idx < 4 {
                RegOperand {
                    reg: GuestReg::Gpr(dword),
                    size: Size::Byte,
                }
            } else {
                let high_of = DwordReg::from_index(idx - 4);
                RegOperand {
                    reg: GuestReg::ByteHi(high_of),
                    size: Size::Byte,
                }
            }
        }
        Size::Word | Size::Dword => RegOperand {
            reg: GuestReg::Gpr(dword),
            size,
        },
    }
}

/// Decodes the ModR/M memory operand following `modrm`. Returns
/// `Some(MemOperand)` for valid memory references, or `None` if the
/// addressing mode requires a capability we don't model (currently:
/// never — the decoder should always produce a valid operand for
/// `modrm < 0xC0`).
fn decode_mem_operand<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, modrm: u8) -> MemOperand {
    match ctx.addr_size() {
        AddrSize::Addr16 => decode_mem_operand_16(bus, ctx, modrm),
        AddrSize::Addr32 => decode_mem_operand_32(bus, ctx, modrm),
    }
}

fn decode_mem_operand_16<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, modrm: u8) -> MemOperand {
    let mode = modrm >> 6;
    let rm = modrm & 7;
    let (base, index, default_seg_bp_based) = match rm {
        0 => (Some(DwordReg::EBX), Some(DwordReg::ESI), false),
        1 => (Some(DwordReg::EBX), Some(DwordReg::EDI), false),
        2 => (Some(DwordReg::EBP), Some(DwordReg::ESI), true),
        3 => (Some(DwordReg::EBP), Some(DwordReg::EDI), true),
        4 => (Some(DwordReg::ESI), None, false),
        5 => (Some(DwordReg::EDI), None, false),
        6 => {
            if mode == 0 {
                (None, None, false) // [disp16]
            } else {
                (Some(DwordReg::EBP), None, true) // [BP+disp]
            }
        }
        7 => (Some(DwordReg::EBX), None, false),
        _ => unreachable!(),
    };

    let disp = match mode {
        0 => {
            if rm == 6 {
                fetch_u16(bus, ctx) as i16 as i32
            } else {
                0
            }
        }
        1 => fetch_byte(bus, ctx) as i8 as i32,
        2 => fetch_u16(bus, ctx) as i16 as i32,
        _ => unreachable!("mode 3 is the register form; caller dispatches"),
    };

    let default_seg = if default_seg_bp_based {
        SegReg32::SS
    } else {
        SegReg32::DS
    };

    MemOperand {
        seg: ctx.apply_seg_override(default_seg),
        base,
        index,
        scale: 0,
        disp,
        addr_size: AddrSize::Addr16,
    }
}

fn decode_mem_operand_32<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, modrm: u8) -> MemOperand {
    let mode = modrm >> 6;
    let rm = modrm & 7;

    let mut base: Option<DwordReg> = None;
    let mut index: Option<DwordReg> = None;
    let mut scale: u8 = 0;
    let mut disp: i32 = 0;
    let default_seg;

    if rm == 4 {
        let sib = fetch_byte(bus, ctx);
        let sib_scale = (sib >> 6) & 0x3;
        let sib_index = (sib >> 3) & 0x7;
        let sib_base = sib & 0x7;
        let scaled_base_no_index =
            sib_index == 4 && sib_scale != 0 && !(mode == 0 && sib_base == 5);

        if sib_index != 4 {
            index = Some(DwordReg::from_index(sib_index));
            scale = sib_scale;
        } else if scaled_base_no_index {
            // Undefined 80386 SIB row: no index, nonzero scale; hardware
            // scales the base register as if it were the index.
            index = Some(DwordReg::from_index(sib_base));
            scale = sib_scale;
        }

        if mode == 0 && sib_base == 5 {
            disp = fetch_u32(bus, ctx) as i32;
            default_seg = SegReg32::DS;
        } else if scaled_base_no_index {
            // base register already consumed as the scaled index.
            default_seg = if sib_base == 4 || sib_base == 5 {
                SegReg32::SS
            } else {
                SegReg32::DS
            };
        } else {
            base = Some(DwordReg::from_index(sib_base));
            default_seg = if sib_base == 4 || sib_base == 5 {
                SegReg32::SS
            } else {
                SegReg32::DS
            };
        }
    } else if mode == 0 && rm == 5 {
        disp = fetch_u32(bus, ctx) as i32;
        default_seg = SegReg32::DS;
    } else {
        base = Some(DwordReg::from_index(rm));
        default_seg = if rm == 4 || rm == 5 {
            SegReg32::SS
        } else {
            SegReg32::DS
        };
    }

    match mode {
        0 => {}
        1 => disp = disp.wrapping_add(fetch_byte(bus, ctx) as i8 as i32),
        2 => disp = disp.wrapping_add(fetch_u32(bus, ctx) as i32),
        _ => unreachable!(),
    }

    MemOperand {
        seg: ctx.apply_seg_override(default_seg),
        base,
        index,
        scale,
        disp,
        addr_size: AddrSize::Addr32,
    }
}

fn decode_one<const CPU_MODEL: u8, B: Bus>(
    cpu: &I386<CPU_MODEL>,
    bus: &mut B,
    ctx: &mut DecodeCtx,
    instr_start_eip: u32,
) -> DecodeResult {
    let _ = cpu;
    let opcode = fetch_byte(bus, ctx);
    // LOCK prefix may make the instruction faulting (#UD on non-
    // lockable opcodes) or require atomic semantics we do not model.
    // Delegate the whole instruction to the interpreter.
    if ctx.lock {
        return fallback(ctx, instr_start_eip);
    }
    match opcode {
        0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 => {
            match opcode {
                0x26 => ctx.seg_override = Some(SegReg32::ES),
                0x2E => ctx.seg_override = Some(SegReg32::CS),
                0x36 => ctx.seg_override = Some(SegReg32::SS),
                0x3E => ctx.seg_override = Some(SegReg32::DS),
                0x64 => ctx.seg_override = Some(SegReg32::FS),
                0x65 => ctx.seg_override = Some(SegReg32::GS),
                0x66 => ctx.operand_size_override = true,
                0x67 => ctx.address_size_override = true,
                0xF0 => ctx.lock = true,
                0xF2 | 0xF3 => return fallback(ctx, instr_start_eip),
                _ => unreachable!(),
            }
            ctx.cycles += 1;
            decode_one(cpu, bus, ctx, instr_start_eip)
        }

        // ALU r/m, reg / reg, r/m / AL, imm / AX/EAX, imm for
        // ADD/OR/ADC/SBB/AND/SUB/XOR/CMP. 0x00..0x3D excluding the
        // segment push/pop (0x06/0x07/0x0E/0x16/0x17/0x1E/0x1F) and
        // the BCD/AAA/DAA opcodes (0x27/0x2F/0x37/0x3F).
        op @ (0x00 | 0x01 | 0x02 | 0x03 | 0x08 | 0x09 | 0x0A | 0x0B | 0x10 | 0x11 | 0x12 | 0x13
        | 0x18 | 0x19 | 0x1A | 0x1B | 0x20 | 0x21 | 0x22 | 0x23 | 0x28 | 0x29 | 0x2A
        | 0x2B | 0x30 | 0x31 | 0x32 | 0x33 | 0x38 | 0x39 | 0x3A | 0x3B) => {
            decode_alu_rm_form(bus, ctx, op);
            DecodeResult::Continued
        }
        op @ (0x04 | 0x05 | 0x0C | 0x0D | 0x14 | 0x15 | 0x1C | 0x1D | 0x24 | 0x25 | 0x2C | 0x2D
        | 0x34 | 0x35 | 0x3C | 0x3D) => {
            decode_alu_acc_imm(bus, ctx, op);
            DecodeResult::Continued
        }

        // INC r32 / r16
        0x40..=0x47 => {
            let size = ctx.operand_size();
            let dst = gpr_reg(opcode - 0x40, size);
            ctx.ops.push(IrOp::Unary {
                dst,
                op: UnaryOp::Inc,
            });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // DEC r32 / r16
        0x48..=0x4F => {
            let size = ctx.operand_size();
            let dst = gpr_reg(opcode - 0x48, size);
            ctx.ops.push(IrOp::Unary {
                dst,
                op: UnaryOp::Dec,
            });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // PUSH SP/ESP
        0x54 => {
            let size = ctx.push_pop_size();
            ctx.ops.push(IrOp::PushStackPtr { size });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // PUSH r32 / r16
        0x50..=0x57 => {
            let size = ctx.push_pop_size();
            let src = gpr_reg(opcode - 0x50, size);
            ctx.ops.push(IrOp::PushReg { src });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // POP r32 / r16
        0x58..=0x5F => {
            let size = ctx.push_pop_size();
            let dst = gpr_reg(opcode - 0x58, size);
            ctx.ops.push(IrOp::PopReg { dst });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // PUSHA / PUSHAD
        0x60 => {
            let size = ctx.push_pop_size();
            ctx.ops.push(IrOp::PushAll { size });
            ctx.cycles += 11;
            DecodeResult::Continued
        }

        // POPA / POPAD
        0x61 => {
            let size = ctx.push_pop_size();
            ctx.ops.push(IrOp::PopAll { size });
            ctx.cycles += 9;
            DecodeResult::Continued
        }

        // PUSH imm16/32
        0x68 => {
            let size = ctx.operand_size();
            let imm = match size {
                Size::Word => fetch_u16(bus, ctx) as u32,
                Size::Dword => fetch_u32(bus, ctx),
                Size::Byte => unreachable!(),
            };
            ctx.ops.push(IrOp::PushImm { imm, size });
            ctx.cycles += 1;
            DecodeResult::Continued
        }
        // PUSH imm8 (sign-extended to operand size)
        0x6A => {
            let size = ctx.operand_size();
            let imm = fetch_byte(bus, ctx) as i8 as i32 as u32;
            let imm = match size {
                Size::Word => imm & 0xFFFF,
                _ => imm,
            };
            ctx.ops.push(IrOp::PushImm { imm, size });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // IMUL r16/32, r/m, imm16/32
        0x69 => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, size);
            let imm = match size {
                Size::Word => fetch_u16(bus, ctx) as i16 as i32,
                Size::Dword => fetch_u32(bus, ctx) as i32,
                Size::Byte => unreachable!(),
            };
            ctx.ops.push(IrOp::ImulRegRmImm {
                dst,
                src,
                imm,
                size,
            });
            ctx.cycles += 13;
            DecodeResult::Continued
        }
        // IMUL r16/32, r/m, imm8 (sign-extended)
        0x6B => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, size);
            let imm = fetch_byte(bus, ctx) as i8 as i32;
            ctx.ops.push(IrOp::ImulRegRmImm {
                dst,
                src,
                imm,
                size,
            });
            ctx.cycles += 13;
            DecodeResult::Continued
        }

        // Short Jcc (0x70-0x7F)
        0x70..=0x7F => {
            let disp = fetch_byte(bus, ctx) as i8 as i32;
            let taken = near_eip_for_code_size(ctx.eip.wrapping_add(disp as u32), ctx.code32);
            let fallthrough = near_eip_for_code_size(ctx.eip, ctx.code32);
            ctx.cycles += 3;
            DecodeResult::Terminated(BlockExit::ConditionalJump {
                cond: jcc_cond(opcode - 0x70),
                taken_eip: taken,
                fallthrough_eip: fallthrough,
            })
        }

        // Group 1: ALU r/m, imm (80/81/82/83)
        0x80..=0x83 => {
            decode_group1(bus, ctx, opcode);
            DecodeResult::Continued
        }

        // TEST r/m, reg
        0x84 | 0x85 => {
            let byte_form = opcode & 0x01 == 0;
            let size = if byte_form {
                Size::Byte
            } else {
                ctx.operand_size()
            };
            let modrm = fetch_byte(bus, ctx);
            let reg_idx = (modrm >> 3) & 7;
            let reg = gpr_reg(reg_idx, size);
            if modrm >= 0xC0 {
                let dst = gpr_reg(modrm & 7, size);
                ctx.ops.push(IrOp::Alu {
                    dst,
                    src: reg,
                    op: AluOp::Test,
                });
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                ctx.ops.push(IrOp::MemAluReg {
                    mem,
                    src: reg,
                    size,
                    op: AluOp::Test,
                });
            }
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // XCHG r/m, reg — memory form has implicit LOCK; delegate to interpreter.
        0x86 | 0x87 => {
            let modrm = fetch_byte(bus, ctx);
            if modrm < 0xC0 {
                let _ = decode_mem_operand(bus, ctx, modrm);
                ctx.ops.push(IrOp::InterpCall {
                    instr_start_eip,
                    instr_end_eip: ctx.eip,
                });
                return DecodeResult::Continued;
            }
            let byte_form = opcode & 0x01 == 0;
            let size = if byte_form {
                Size::Byte
            } else {
                ctx.operand_size()
            };
            let a = gpr_reg((modrm >> 3) & 7, size);
            let b = gpr_reg(modrm & 7, size);
            ctx.ops.push(IrOp::XchgReg { a, b });
            ctx.cycles += 3;
            DecodeResult::Continued
        }

        // MOV r/m, reg / reg, r/m
        0x88..=0x8B => {
            let byte_form = opcode & 0x01 == 0;
            let size = if byte_form {
                Size::Byte
            } else {
                ctx.operand_size()
            };
            let modrm = fetch_byte(bus, ctx);
            let reg = gpr_reg((modrm >> 3) & 7, size);
            let reg_to_rm = opcode & 0x02 == 0;
            if modrm >= 0xC0 {
                let rm = gpr_reg(modrm & 7, size);
                let (dst, src) = if reg_to_rm { (rm, reg) } else { (reg, rm) };
                ctx.ops.push(IrOp::MovReg { dst, src });
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                if reg_to_rm {
                    ctx.ops.push(IrOp::MemWrite { src: reg, mem });
                } else {
                    ctx.ops.push(IrOp::MemRead { dst: reg, mem });
                }
            }
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // LEA r, m
        0x8D => {
            let modrm = fetch_byte(bus, ctx);
            if modrm >= 0xC0 {
                // LEA with register operand raises #UD on a real 386;
                // let the interpreter deliver it.
                return fallback(ctx, instr_start_eip);
            }
            let size = ctx.operand_size();
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let mem = decode_mem_operand(bus, ctx, modrm);
            ctx.ops.push(IrOp::Lea { dst, mem });
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // POP r/m (only /0 is defined)
        0x8F => {
            let modrm = fetch_byte(bus, ctx);
            let reg_field = (modrm >> 3) & 7;
            if reg_field != 0 {
                return fallback(ctx, instr_start_eip);
            }
            let size = ctx.push_pop_size();
            if modrm >= 0xC0 {
                let dst = gpr_reg(modrm & 7, size);
                ctx.ops.push(IrOp::PopReg { dst });
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                ctx.ops.push(IrOp::PopMem { mem, size });
            }
            ctx.cycles += 5;
            DecodeResult::Continued
        }

        // NOP
        0x90 => {
            ctx.ops.push(IrOp::Nop);
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // XCHG AX, r16/r32 (0x91-0x97) - swap with the accumulator.
        0x91..=0x97 => {
            let size = ctx.operand_size();
            let a = gpr_reg(0, size); // A(X/E) = EAX
            let b = gpr_reg(opcode - 0x90, size);
            ctx.ops.push(IrOp::XchgReg { a, b });
            ctx.cycles += 3;
            DecodeResult::Continued
        }

        // CBW / CWDE
        0x98 => {
            let size = ctx.operand_size();
            let op = match size {
                Size::Word => CbwOp::Cbw,
                Size::Dword => CbwOp::Cwde,
                Size::Byte => unreachable!(),
            };
            ctx.ops.push(IrOp::CbwCwd { op });
            ctx.cycles += 3;
            DecodeResult::Continued
        }

        // CWD / CDQ
        0x99 => {
            let size = ctx.operand_size();
            let op = match size {
                Size::Word => CbwOp::Cwd,
                Size::Dword => CbwOp::Cdq,
                Size::Byte => unreachable!(),
            };
            ctx.ops.push(IrOp::CbwCwd { op });
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // PUSHF / PUSHFD
        0x9C => {
            let size = ctx.push_pop_size();
            ctx.ops.push(IrOp::PushFlags { size });
            ctx.cycles += 4;
            DecodeResult::Continued
        }

        // POPF / POPFD
        0x9D => {
            let size = ctx.push_pop_size();
            ctx.ops.push(IrOp::PopFlags { size });
            ctx.cycles += 6;
            DecodeResult::Continued
        }

        // SAHF
        0x9E => {
            ctx.ops.push(IrOp::Sahf);
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // LAHF
        0x9F => {
            ctx.ops.push(IrOp::Lahf);
            ctx.cycles += 3;
            DecodeResult::Continued
        }

        // MOV AL/AX/EAX, moffs / MOV moffs, AL/AX/EAX
        0xA0..=0xA3 => {
            let byte_form = opcode & 0x01 == 0;
            let size = if byte_form {
                Size::Byte
            } else {
                ctx.operand_size()
            };
            let addr_size = ctx.addr_size();
            let disp = match addr_size {
                AddrSize::Addr16 => fetch_u16(bus, ctx) as i16 as i32,
                AddrSize::Addr32 => fetch_u32(bus, ctx) as i32,
            };
            let mem = MemOperand {
                seg: ctx.apply_seg_override(SegReg32::DS),
                base: None,
                index: None,
                scale: 0,
                disp,
                addr_size,
            };
            let reg = match size {
                Size::Byte => RegOperand {
                    reg: GuestReg::Gpr(DwordReg::EAX),
                    size: Size::Byte,
                },
                _ => RegOperand {
                    reg: GuestReg::Gpr(DwordReg::EAX),
                    size,
                },
            };
            if opcode & 0x02 == 0 {
                // 0xA0 / 0xA1: load AL/AX/EAX from moffs.
                ctx.ops.push(IrOp::MemRead { dst: reg, mem });
            } else {
                // 0xA2 / 0xA3: store AL/AX/EAX to moffs.
                ctx.ops.push(IrOp::MemWrite { src: reg, mem });
            }
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // TEST AL/AX/EAX, imm
        0xA8 => {
            let imm = fetch_byte(bus, ctx) as u32;
            let dst = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EAX),
                size: Size::Byte,
            };
            ctx.ops.push(IrOp::AluImm {
                dst,
                imm,
                op: AluOp::Test,
            });
            ctx.cycles += 2;
            DecodeResult::Continued
        }
        0xA9 => {
            let size = ctx.operand_size();
            let imm = match size {
                Size::Word => fetch_u16(bus, ctx) as u32,
                Size::Dword => fetch_u32(bus, ctx),
                Size::Byte => unreachable!(),
            };
            let dst = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EAX),
                size,
            };
            ctx.ops.push(IrOp::AluImm {
                dst,
                imm,
                op: AluOp::Test,
            });
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // MOV r8, imm8
        0xB0..=0xB7 => {
            let dst = gpr_reg(opcode - 0xB0, Size::Byte);
            let imm = fetch_byte(bus, ctx) as u32;
            ctx.ops.push(IrOp::MovImm { dst, imm });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // MOV r16/r32, imm
        0xB8..=0xBF => {
            let size = ctx.operand_size();
            let dst = gpr_reg(opcode - 0xB8, size);
            let imm = match size {
                Size::Word => fetch_u16(bus, ctx) as u32,
                Size::Dword => fetch_u32(bus, ctx),
                Size::Byte => unreachable!(),
            };
            ctx.ops.push(IrOp::MovImm { dst, imm });
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // Shift/rotate r/m, imm8 / r/m, 1 / r/m, CL (groups C0/C1/D0-D3)
        0xC0 => {
            decode_shift_group(
                bus,
                ctx,
                Size::Byte,
                ShiftCount::Imm(0),
                /*is_imm=*/ true,
            );
            DecodeResult::Continued
        }
        0xC1 => {
            let size = ctx.operand_size();
            decode_shift_group(bus, ctx, size, ShiftCount::Imm(0), true);
            DecodeResult::Continued
        }
        0xD0 => {
            decode_shift_group(bus, ctx, Size::Byte, ShiftCount::Imm(1), false);
            DecodeResult::Continued
        }
        0xD1 => {
            let size = ctx.operand_size();
            decode_shift_group(bus, ctx, size, ShiftCount::Imm(1), false);
            DecodeResult::Continued
        }
        0xD2 => {
            decode_shift_group(bus, ctx, Size::Byte, ShiftCount::Cl, false);
            DecodeResult::Continued
        }
        0xD3 => {
            let size = ctx.operand_size();
            decode_shift_group(bus, ctx, size, ShiftCount::Cl, false);
            DecodeResult::Continued
        }

        // RET near
        0xC3 => {
            let size = ctx.push_pop_size();
            ctx.cycles += 5;
            DecodeResult::Terminated(BlockExit::Return { size })
        }
        // RET near imm16
        0xC2 => {
            let extra_sp = fetch_u16(bus, ctx) as u32;
            let size = ctx.push_pop_size();
            ctx.cycles += 5;
            DecodeResult::Terminated(BlockExit::ReturnImm { size, extra_sp })
        }

        // MOV r/m8, imm8
        0xC6 => {
            let modrm = fetch_byte(bus, ctx);
            let reg_field = (modrm >> 3) & 7;
            if reg_field != 0 {
                return fallback(ctx, instr_start_eip);
            }
            let rm = if modrm >= 0xC0 {
                None
            } else {
                Some(decode_mem_operand(bus, ctx, modrm))
            };
            let imm = fetch_byte(bus, ctx) as u32;
            if let Some(mem) = rm {
                ctx.ops.push(IrOp::MemMovImm {
                    mem,
                    imm,
                    size: Size::Byte,
                });
            } else {
                let dst = gpr_reg(modrm & 7, Size::Byte);
                ctx.ops.push(IrOp::MovImm { dst, imm });
            }
            ctx.cycles += 1;
            DecodeResult::Continued
        }
        // MOV r/m16/32, imm
        0xC7 => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let reg_field = (modrm >> 3) & 7;
            if reg_field != 0 {
                return fallback(ctx, instr_start_eip);
            }
            let rm = if modrm >= 0xC0 {
                None
            } else {
                Some(decode_mem_operand(bus, ctx, modrm))
            };
            let imm = match size {
                Size::Word => fetch_u16(bus, ctx) as u32,
                Size::Dword => fetch_u32(bus, ctx),
                Size::Byte => unreachable!(),
            };
            if let Some(mem) = rm {
                ctx.ops.push(IrOp::MemMovImm { mem, imm, size });
            } else {
                let dst = gpr_reg(modrm & 7, size);
                ctx.ops.push(IrOp::MovImm { dst, imm });
            }
            ctx.cycles += 1;
            DecodeResult::Continued
        }

        // ENTER imm16, imm8 — delegate (nested-level semantics + stack frame setup).
        0xC8 => {
            let _frame_size = fetch_u16(bus, ctx);
            let _nest_level = fetch_byte(bus, ctx);
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }

        // LEAVE
        0xC9 => {
            let stack_size = if ctx.stack32 { Size::Dword } else { Size::Word };
            ctx.ops.push(IrOp::MovReg {
                dst: RegOperand {
                    reg: GuestReg::Gpr(DwordReg::ESP),
                    size: stack_size,
                },
                src: RegOperand {
                    reg: GuestReg::Gpr(DwordReg::EBP),
                    size: stack_size,
                },
            });
            let size = ctx.push_pop_size();
            ctx.ops.push(IrOp::PopReg {
                dst: RegOperand {
                    reg: GuestReg::Gpr(DwordReg::EBP),
                    size,
                },
            });
            ctx.cycles += 4;
            DecodeResult::Continued
        }

        // IN acc, imm8 / OUT imm8, acc: port-access instructions. Dispatch
        // through the interpreter because the bus side effects may flip
        // memory banks or change device state that the JIT does not track.
        0xE4..=0xE7 => {
            let _port = fetch_byte(bus, ctx);
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }

        // XLAT / XLATB: AL <- [DS:BX + AL] (addr16) or [DS:EBX + AL] (addr32).
        0xD7 => {
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }

        // LOOPNE / LOOPE / LOOP / JCXZ
        0xE0..=0xE3 => {
            let disp = fetch_byte(bus, ctx) as i8 as i32;
            let taken = near_eip_for_code_size(ctx.eip.wrapping_add(disp as u32), ctx.code32);
            let fallthrough = near_eip_for_code_size(ctx.eip, ctx.code32);
            let cond = match opcode {
                0xE0 => LoopCond::Loopne,
                0xE1 => LoopCond::Loope,
                0xE2 => LoopCond::Loop,
                0xE3 => LoopCond::Jcxz,
                _ => unreachable!(),
            };
            ctx.cycles += 6;
            DecodeResult::Terminated(BlockExit::CondLoop {
                cond,
                taken_eip: taken,
                fallthrough_eip: fallthrough,
                addr_size: ctx.addr_size(),
            })
        }

        // CALL rel
        0xE8 => {
            let size = ctx.operand_size();
            let disp = match size {
                Size::Word => fetch_u16(bus, ctx) as i16 as i32,
                Size::Dword => fetch_u32(bus, ctx) as i32,
                Size::Byte => unreachable!(),
            };
            let return_eip = near_eip_for_size(ctx.eip, size);
            let target = near_eip_for_size(ctx.eip.wrapping_add(disp as u32), size);
            ctx.cycles += 3;
            DecodeResult::Terminated(BlockExit::DirectCall {
                target_eip: target,
                return_eip,
                size,
            })
        }

        // JMP rel near
        0xE9 => {
            let size = ctx.operand_size();
            let disp = match size {
                Size::Word => fetch_u16(bus, ctx) as i16 as i32,
                Size::Dword => fetch_u32(bus, ctx) as i32,
                Size::Byte => unreachable!(),
            };
            let target = near_eip_for_size(ctx.eip.wrapping_add(disp as u32), size);
            ctx.cycles += 3;
            DecodeResult::Terminated(BlockExit::DirectJump { target_eip: target })
        }

        // IN acc, DX / OUT DX, acc: port-access instructions.
        0xEC..=0xEF => {
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }

        // JMP rel8
        0xEB => {
            let disp = fetch_byte(bus, ctx) as i8 as i32;
            let target = near_eip_for_code_size(ctx.eip.wrapping_add(disp as u32), ctx.code32);
            ctx.cycles += 3;
            DecodeResult::Terminated(BlockExit::DirectJump { target_eip: target })
        }

        // HLT
        0xF4 => {
            if !cpu.jit_can_halt() {
                return fallback(ctx, instr_start_eip);
            }
            ctx.cycles += 5;
            DecodeResult::Terminated(BlockExit::Halt { next_eip: ctx.eip })
        }

        // Flag ops
        0xF5 => {
            ctx.ops.push(IrOp::Flag(FlagOp::Cmc));
            ctx.cycles += 2;
            DecodeResult::Continued
        }
        0xF8 => {
            ctx.ops.push(IrOp::Flag(FlagOp::Clc));
            ctx.cycles += 2;
            DecodeResult::Continued
        }
        0xF9 => {
            ctx.ops.push(IrOp::Flag(FlagOp::Stc));
            ctx.cycles += 2;
            DecodeResult::Continued
        }
        0xFC => {
            ctx.ops.push(IrOp::Flag(FlagOp::Cld));
            ctx.cycles += 2;
            DecodeResult::Continued
        }
        0xFD => {
            ctx.ops.push(IrOp::Flag(FlagOp::Std));
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // Group 3 byte form: TEST/NOT/NEG/MUL/IMUL/DIV/IDIV r/m8
        0xF6 => {
            decode_group_f6_f7(bus, ctx, Size::Byte);
            DecodeResult::Continued
        }
        // Group 3 word/dword form: TEST/NOT/NEG/MUL/IMUL/DIV/IDIV r/m16/32
        0xF7 => {
            let size = ctx.operand_size();
            decode_group_f6_f7(bus, ctx, size);
            DecodeResult::Continued
        }

        // Group FE: INC/DEC r/m8
        0xFE => {
            let modrm = fetch_byte(bus, ctx);
            let sub = (modrm >> 3) & 7;
            let op = match sub {
                0 => UnaryOp::Inc,
                1 => UnaryOp::Dec,
                _ => return fallback(ctx, instr_start_eip),
            };
            if modrm >= 0xC0 {
                let dst = gpr_reg(modrm & 7, Size::Byte);
                ctx.ops.push(IrOp::Unary { dst, op });
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                ctx.ops.push(IrOp::MemUnary {
                    mem,
                    size: Size::Byte,
                    op,
                });
            }
            ctx.cycles += 2;
            DecodeResult::Continued
        }

        // Group FF: INC/DEC r/m16/32, CALL/JMP near/far indirect, PUSH r/m
        0xFF => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let sub = (modrm >> 3) & 7;
            match sub {
                0 | 1 => {
                    let unary = if sub == 0 { UnaryOp::Inc } else { UnaryOp::Dec };
                    if modrm >= 0xC0 {
                        let dst = gpr_reg(modrm & 7, size);
                        ctx.ops.push(IrOp::Unary { dst, op: unary });
                    } else {
                        let mem = decode_mem_operand(bus, ctx, modrm);
                        ctx.ops.push(IrOp::MemUnary {
                            mem,
                            size,
                            op: unary,
                        });
                    }
                    ctx.cycles += 2;
                    DecodeResult::Continued
                }
                2 => {
                    // CALL r/m near indirect. The return address is EIP
                    // after the full instruction, so we must capture
                    // ctx.eip AFTER decoding the memory operand's disp.
                    let target = if modrm >= 0xC0 {
                        IndirectTarget::Reg(DwordReg::from_index(modrm & 7))
                    } else {
                        IndirectTarget::Mem(decode_mem_operand(bus, ctx, modrm))
                    };
                    let return_eip = ctx.eip;
                    ctx.cycles += 5;
                    DecodeResult::Terminated(BlockExit::IndirectCall {
                        target,
                        return_eip,
                        size,
                    })
                }
                4 => {
                    // JMP r/m near indirect
                    let target = if modrm >= 0xC0 {
                        IndirectTarget::Reg(DwordReg::from_index(modrm & 7))
                    } else {
                        IndirectTarget::Mem(decode_mem_operand(bus, ctx, modrm))
                    };
                    ctx.cycles += 5;
                    DecodeResult::Terminated(BlockExit::IndirectJump { target, size })
                }
                6 => {
                    // PUSH r/m
                    if modrm >= 0xC0 {
                        if (modrm & 7) == 4 {
                            ctx.ops.push(IrOp::PushStackPtr { size });
                        } else {
                            let src = gpr_reg(modrm & 7, size);
                            ctx.ops.push(IrOp::PushReg { src });
                        }
                    } else {
                        let mem = decode_mem_operand(bus, ctx, modrm);
                        ctx.ops.push(IrOp::PushMem { mem, size });
                    }
                    ctx.cycles += 2;
                    DecodeResult::Continued
                }
                3 | 5 => fallback(ctx, instr_start_eip), // far CALL/JMP
                _ => fallback(ctx, instr_start_eip),
            }
        }

        // 0F prefix secondary opcodes
        0x0F => decode_0f(bus, ctx, instr_start_eip),

        // Anything else: fall back.
        _ => fallback(ctx, instr_start_eip),
    }
}

fn decode_rm_source<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, modrm: u8, size: Size) -> RmSource {
    if modrm >= 0xC0 {
        RmSource::Reg(gpr_reg(modrm & 7, size))
    } else {
        let mem = decode_mem_operand(bus, ctx, modrm);
        RmSource::Mem { mem, size }
    }
}

fn decode_alu_rm_form<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, op: u8) {
    let alu = alu_op_from_opcode_base(op & 0x38);
    let byte_form = op & 0x01 == 0;
    let size = if byte_form {
        Size::Byte
    } else {
        ctx.operand_size()
    };
    let modrm = fetch_byte(bus, ctx);
    let reg = gpr_reg((modrm >> 3) & 7, size);
    let reg_is_src = op & 0x02 == 0; // opcode bit 1 = 0 means r/m = dst, reg = src
    if modrm >= 0xC0 {
        let rm = gpr_reg(modrm & 7, size);
        let (dst, src) = if reg_is_src { (rm, reg) } else { (reg, rm) };
        ctx.ops.push(IrOp::Alu { dst, src, op: alu });
    } else {
        let mem = decode_mem_operand(bus, ctx, modrm);
        if reg_is_src {
            // r/m <- r/m OP reg
            ctx.ops.push(IrOp::MemAluReg {
                mem,
                src: reg,
                size,
                op: alu,
            });
        } else {
            // reg <- reg OP r/m
            ctx.ops.push(IrOp::AluRm {
                dst: reg,
                mem,
                op: alu,
            });
        }
    }
    ctx.cycles += 2;
}

fn decode_alu_acc_imm<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, op: u8) {
    let alu = alu_op_from_opcode_base(op & 0x38);
    let byte_form = op & 0x01 == 0;
    let size = if byte_form {
        Size::Byte
    } else {
        ctx.operand_size()
    };
    let imm = match size {
        Size::Byte => fetch_byte(bus, ctx) as u32,
        Size::Word => fetch_u16(bus, ctx) as u32,
        Size::Dword => fetch_u32(bus, ctx),
    };
    let dst = match size {
        Size::Byte => RegOperand {
            reg: GuestReg::Gpr(DwordReg::EAX),
            size: Size::Byte,
        },
        _ => RegOperand {
            reg: GuestReg::Gpr(DwordReg::EAX),
            size,
        },
    };
    ctx.ops.push(IrOp::AluImm { dst, imm, op: alu });
    ctx.cycles += 2;
}

fn alu_op_from_opcode_base(base: u8) -> AluOp {
    match base {
        0x00 => AluOp::Add,
        0x08 => AluOp::Or,
        0x10 => AluOp::Adc,
        0x18 => AluOp::Sbb,
        0x20 => AluOp::And,
        0x28 => AluOp::Sub,
        0x30 => AluOp::Xor,
        0x38 => AluOp::Cmp,
        _ => unreachable!(),
    }
}

fn decode_group1<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, opcode: u8) {
    let modrm = fetch_byte(bus, ctx);
    let sub = (modrm >> 3) & 7;
    let alu = match sub {
        0 => AluOp::Add,
        1 => AluOp::Or,
        2 => AluOp::Adc,
        3 => AluOp::Sbb,
        4 => AluOp::And,
        5 => AluOp::Sub,
        6 => AluOp::Xor,
        7 => AluOp::Cmp,
        _ => unreachable!(),
    };
    let size = match opcode {
        0x80 | 0x82 => Size::Byte,
        0x81 | 0x83 => ctx.operand_size(),
        _ => unreachable!(),
    };
    // Memory operand (including its displacement) must be decoded
    // BEFORE the immediate: encoding is opcode, modrm, [sib], [disp], imm.
    let rm = if modrm >= 0xC0 {
        None
    } else {
        Some(decode_mem_operand(bus, ctx, modrm))
    };
    let imm = match opcode {
        0x80 | 0x82 => fetch_byte(bus, ctx) as u32,
        0x81 => match size {
            Size::Word => fetch_u16(bus, ctx) as u32,
            Size::Dword => fetch_u32(bus, ctx),
            Size::Byte => unreachable!(),
        },
        0x83 => fetch_byte(bus, ctx) as i8 as i32 as u32,
        _ => unreachable!(),
    };
    let imm = match size {
        Size::Byte => imm & 0xFF,
        Size::Word => imm & 0xFFFF,
        Size::Dword => imm,
    };
    if let Some(mem) = rm {
        ctx.ops.push(IrOp::MemAluImm {
            mem,
            imm,
            size,
            op: alu,
        });
    } else {
        let dst = gpr_reg(modrm & 7, size);
        ctx.ops.push(IrOp::AluImm { dst, imm, op: alu });
    }
    ctx.cycles += 2;
}

fn decode_group_f6_f7<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, size: Size) {
    let modrm = fetch_byte(bus, ctx);
    let sub = (modrm >> 3) & 7;
    match sub {
        0 | 1 => {
            // TEST r/m, imm (both sub values 0 and 1 decode identically)
            let imm = match size {
                Size::Byte => fetch_byte(bus, ctx) as u32,
                Size::Word => fetch_u16(bus, ctx) as u32,
                Size::Dword => fetch_u32(bus, ctx),
            };
            if modrm >= 0xC0 {
                let dst = gpr_reg(modrm & 7, size);
                ctx.ops.push(IrOp::AluImm {
                    dst,
                    imm,
                    op: AluOp::Test,
                });
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                ctx.ops.push(IrOp::MemAluImm {
                    mem,
                    imm,
                    size,
                    op: AluOp::Test,
                });
            }
            ctx.cycles += 2;
        }
        2 | 3 => {
            let op = if sub == 2 { UnaryOp::Not } else { UnaryOp::Neg };
            if modrm >= 0xC0 {
                let dst = gpr_reg(modrm & 7, size);
                ctx.ops.push(IrOp::Unary { dst, op });
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                ctx.ops.push(IrOp::MemUnary { mem, size, op });
            }
            ctx.cycles += 3;
        }
        4..=7 => {
            let signed = sub == 5 || sub == 7;
            let is_div = sub == 6 || sub == 7;
            let src = decode_rm_source(bus, ctx, modrm, size);
            if is_div {
                ctx.ops.push(IrOp::DivAcc { src, size, signed });
                ctx.cycles += 20;
            } else {
                ctx.ops.push(IrOp::MulAcc { src, size, signed });
                ctx.cycles += 13;
            }
        }
        _ => unreachable!(),
    }
}

fn decode_shift_group<B: Bus>(
    bus: &mut B,
    ctx: &mut DecodeCtx,
    size: Size,
    count: ShiftCount,
    is_imm: bool,
) {
    let modrm = fetch_byte(bus, ctx);
    let sub = (modrm >> 3) & 7;
    // 0: ROL, 1: ROR, 2: RCL, 3: RCR, 4: SHL, 5: SHR, 6: SHL (undoc), 7: SAR
    let op = match sub {
        0 => ShiftOp::Rol,
        1 => ShiftOp::Ror,
        2 => ShiftOp::Rcl,
        3 => ShiftOp::Rcr,
        4 | 6 => ShiftOp::Shl,
        5 => ShiftOp::Shr,
        7 => ShiftOp::Sar,
        _ => unreachable!(),
    };
    // Decode memory operand (including displacement) BEFORE fetching
    // the shift count imm8 -- encoding is opcode, modrm, [sib], [disp], imm.
    let dst = if modrm >= 0xC0 {
        RmDest::Reg(gpr_reg(modrm & 7, size))
    } else {
        let mem = decode_mem_operand(bus, ctx, modrm);
        RmDest::Mem { mem, size }
    };
    let count = if is_imm {
        ShiftCount::Imm(fetch_byte(bus, ctx))
    } else {
        count
    };
    ctx.ops.push(IrOp::Shift {
        dst,
        size,
        count,
        op,
    });
    ctx.cycles += 3;
}

fn decode_0f<B: Bus>(bus: &mut B, ctx: &mut DecodeCtx, instr_start_eip: u32) -> DecodeResult {
    let second = fetch_byte(bus, ctx);
    match second {
        // Long Jcc (0x0F 0x80-0x8F)
        0x80..=0x8F => {
            let size = ctx.operand_size();
            let disp = match size {
                Size::Word => fetch_u16(bus, ctx) as i16 as i32,
                Size::Dword => fetch_u32(bus, ctx) as i32,
                Size::Byte => unreachable!(),
            };
            let taken = near_eip_for_size(ctx.eip.wrapping_add(disp as u32), size);
            let fallthrough = near_eip_for_size(ctx.eip, size);
            ctx.cycles += 3;
            DecodeResult::Terminated(BlockExit::ConditionalJump {
                cond: jcc_cond(second - 0x80),
                taken_eip: taken,
                fallthrough_eip: fallthrough,
            })
        }
        // SETcc r/m8 (0x0F 0x90-0x9F)
        0x90..=0x9F => {
            let modrm = fetch_byte(bus, ctx);
            let cond = jcc_cond(second - 0x90);
            let dst = if modrm >= 0xC0 {
                RmDest::Reg(gpr_reg(modrm & 7, Size::Byte))
            } else {
                let mem = decode_mem_operand(bus, ctx, modrm);
                RmDest::Mem {
                    mem,
                    size: Size::Byte,
                }
            };
            ctx.ops.push(IrOp::SetCc { dst, cond });
            ctx.cycles += 4;
            DecodeResult::Continued
        }
        // IMUL r, r/m (0x0F 0xAF)
        0xAF => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, size);
            ctx.ops.push(IrOp::ImulRegRm { dst, src, size });
            ctx.cycles += 13;
            DecodeResult::Continued
        }
        // MOVZX r, r/m8
        0xB6 => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, Size::Byte);
            ctx.ops.push(IrOp::MovZx {
                dst,
                src,
                src_size: Size::Byte,
            });
            ctx.cycles += 3;
            DecodeResult::Continued
        }
        // MOVZX r, r/m16
        0xB7 => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, Size::Word);
            ctx.ops.push(IrOp::MovZx {
                dst,
                src,
                src_size: Size::Word,
            });
            ctx.cycles += 3;
            DecodeResult::Continued
        }
        // MOVSX r, r/m8
        0xBE => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, Size::Byte);
            ctx.ops.push(IrOp::MovSx {
                dst,
                src,
                src_size: Size::Byte,
            });
            ctx.cycles += 3;
            DecodeResult::Continued
        }
        // MOVSX r, r/m16
        0xBF => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, Size::Word);
            ctx.ops.push(IrOp::MovSx {
                dst,
                src,
                src_size: Size::Word,
            });
            ctx.cycles += 3;
            DecodeResult::Continued
        }
        // BSF r16/32, r/m16/32
        0xBC => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, size);
            ctx.ops.push(IrOp::BitScan {
                dst,
                src,
                size,
                reverse: false,
            });
            ctx.cycles += 7;
            DecodeResult::Continued
        }
        // BSR r16/32, r/m16/32
        0xBD => {
            let size = ctx.operand_size();
            let modrm = fetch_byte(bus, ctx);
            let dst = gpr_reg((modrm >> 3) & 7, size);
            let src = decode_rm_source(bus, ctx, modrm, size);
            ctx.ops.push(IrOp::BitScan {
                dst,
                src,
                size,
                reverse: true,
            });
            ctx.cycles += 7;
            DecodeResult::Continued
        }
        // BSWAP r32 (0x0F 0xC8..0xCF, one per register index)
        0xC8..=0xCF => {
            let dst = DwordReg::from_index(second - 0xC8);
            ctx.ops.push(IrOp::Bswap { dst });
            ctx.cycles += 1;
            DecodeResult::Continued
        }
        // BT/BTS/BTR/BTC r/m, r (0x0F A3/AB/B3/BB).
        0xA3 | 0xAB | 0xB3 | 0xBB => {
            let modrm = fetch_byte(bus, ctx);
            if modrm < 0xC0 {
                let _ = decode_mem_operand(bus, ctx, modrm);
            }
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }
        // Group 0x0F 0xBA: BT/BTS/BTR/BTC r/m, imm8 at sub-op 4..7.
        0xBA => {
            let modrm = fetch_byte(bus, ctx);
            let sub = (modrm >> 3) & 7;
            if !(4..=7).contains(&sub) {
                return fallback(ctx, instr_start_eip);
            }
            if modrm < 0xC0 {
                let _ = decode_mem_operand(bus, ctx, modrm);
            }
            let _imm = fetch_byte(bus, ctx);
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }
        // SHLD imm (0x0F A4) / SHLD CL (0x0F A5)
        // SHRD imm (0x0F AC) / SHRD CL (0x0F AD)
        0xA4 | 0xA5 | 0xAC | 0xAD => {
            let modrm = fetch_byte(bus, ctx);
            if modrm < 0xC0 {
                let _ = decode_mem_operand(bus, ctx, modrm);
            }
            // imm form consumes one more byte for the shift count.
            if matches!(second, 0xA4 | 0xAC) {
                let _count = fetch_byte(bus, ctx);
            }
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }
        // CMPXCHG r/m, r (0x0F B0 byte, 0x0F B1 word/dword). 486+.
        0xB0 | 0xB1 => {
            let modrm = fetch_byte(bus, ctx);
            if modrm < 0xC0 {
                let _ = decode_mem_operand(bus, ctx, modrm);
            }
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }
        // XADD r/m, r (0x0F C0 byte, 0x0F C1 word/dword). 486+.
        0xC0 | 0xC1 => {
            let modrm = fetch_byte(bus, ctx);
            if modrm < 0xC0 {
                let _ = decode_mem_operand(bus, ctx, modrm);
            }
            ctx.ops.push(IrOp::InterpCall {
                instr_start_eip,
                instr_end_eip: ctx.eip,
            });
            DecodeResult::Continued
        }
        _ => fallback(ctx, instr_start_eip),
    }
}

fn jcc_cond(cc: u8) -> IrCond {
    match cc & 0x0F {
        0x0 => IrCond::O,
        0x1 => IrCond::No,
        0x2 => IrCond::B,
        0x3 => IrCond::Nb,
        0x4 => IrCond::Z,
        0x5 => IrCond::Nz,
        0x6 => IrCond::Be,
        0x7 => IrCond::A,
        0x8 => IrCond::S,
        0x9 => IrCond::Ns,
        0xA => IrCond::P,
        0xB => IrCond::Np,
        0xC => IrCond::L,
        0xD => IrCond::Ge,
        0xE => IrCond::Le,
        0xF => IrCond::G,
        _ => unreachable!(),
    }
}
