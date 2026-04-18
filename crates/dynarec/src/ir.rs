//! Small typed IR bridging the decoder and the backends.
//!
//! Covers the non-privileged, non-FPU, non-string integer subset of
//! the 386/486 ISA:
//!
//! * ALU (reg/mem/imm), MOV (reg/mem/imm/moffs), LEA
//! * INC/DEC/NOT/NEG (reg and mem)
//! * PUSH/POP (reg/mem/imm), PUSHA/POPA, PUSHF/POPF, LAHF/SAHF
//! * CALL/JMP/RET near (direct and indirect), Jcc, LOOP/JCXZ
//! * Shifts and rotates (C0/C1/D0-D3), SHLD/SHRD
//! * IMUL (2-op and 3-op), one-op MUL/IMUL/DIV/IDIV
//! * MOVZX/MOVSX, SETcc, XCHG reg-reg, flag-only ops, CBW/CWD
//! * BSWAP, BSF, BSR, BT/BTS/BTR/BTC, XADD, CMPXCHG
//! * IN/OUT, XLAT, ENTER/LEAVE
//!
//! Complex-semantics opcodes (BT family on memory, SHLD/SHRD, XADD,
//! CMPXCHG, IN/OUT, XLAT, ENTER) go through [`IrOp::InterpCall`] which
//! single-steps the interpreter mid-block. Anything still unhandled
//! (FPU, string ops, privileged ops, far control transfers,
//! segment-register loads) terminates the block with
//! [`BlockExit::Fallback`] so the dispatcher can single-step the
//! interpreter.

use cpu::{DwordReg, SegReg32};

/// Operand size in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// 8-bit operand.
    Byte,
    /// 16-bit operand.
    Word,
    /// 32-bit operand.
    Dword,
}

impl Size {
    /// Returns a mask covering the operand's value bits.
    pub const fn mask(self) -> u32 {
        match self {
            Size::Byte => 0xFF,
            Size::Word => 0xFFFF,
            Size::Dword => 0xFFFF_FFFF,
        }
    }
}

/// Effective-address size (selects which half of base/index registers
/// participate in the EA and how the final offset is masked).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrSize {
    /// 16-bit addressing: base and index are the low 16 bits; final
    /// offset is masked to 16 bits before adding the segment base.
    Addr16,
    /// 32-bit addressing: full 32-bit registers participate with SIB.
    Addr32,
}

/// Symbolic reference to a guest architectural register.
///
/// Byte-high registers (AH/CH/DH/BH) are encoded with [`GuestReg::ByteHi`];
/// all other widths share the same [`DwordReg`] slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestReg {
    /// Low portion of the named dword register (AL/AX/EAX, etc.).
    Gpr(DwordReg),
    /// High byte of the named dword register (AH/CH/DH/BH).
    ByteHi(DwordReg),
}

/// Condition code for a conditional branch or SETcc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrCond {
    /// OF=1.
    O,
    /// OF=0.
    No,
    /// CF=1 (below / carry).
    B,
    /// CF=0.
    Nb,
    /// ZF=1 (equal / zero).
    Z,
    /// ZF=0.
    Nz,
    /// CF=1 or ZF=1.
    Be,
    /// CF=0 and ZF=0.
    A,
    /// SF=1.
    S,
    /// SF=0.
    Ns,
    /// PF=1.
    P,
    /// PF=0.
    Np,
    /// SF != OF.
    L,
    /// SF = OF.
    Ge,
    /// ZF=1 or SF != OF.
    Le,
    /// ZF=0 and SF = OF.
    G,
}

/// ALU opcode carried with [`IrOp::Alu`] and its variants.
///
/// Flag-update semantics are encoded in the opcode itself (e.g. `Cmp`
/// sets flags like `Sub` but writes no register, while `Test` matches
/// `And` with the same property).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AluOp {
    /// Add.
    Add,
    /// Add with carry (uses CF).
    Adc,
    /// Subtract.
    Sub,
    /// Subtract with borrow (uses CF).
    Sbb,
    /// Bitwise AND.
    And,
    /// Bitwise OR.
    Or,
    /// Bitwise XOR.
    Xor,
    /// Compare: sets SUB flags, discards the difference.
    Cmp,
    /// Test: sets AND flags, discards the result.
    Test,
}

/// Unary ALU ops carried with [`IrOp::Unary`] / [`IrOp::MemUnary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Increment (does not touch CF).
    Inc,
    /// Decrement (does not touch CF).
    Dec,
    /// Bitwise NOT (does not touch flags).
    Not,
    /// Two's-complement negate (sets full ALU flags).
    Neg,
}

/// Flag-only ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagOp {
    /// Clear carry (CLC).
    Clc,
    /// Set carry (STC).
    Stc,
    /// Complement carry (CMC).
    Cmc,
    /// Clear direction (CLD).
    Cld,
    /// Set direction (STD).
    Std,
}

/// Shift / rotate kind carried with [`IrOp::Shift`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftOp {
    /// Rotate left.
    Rol,
    /// Rotate right.
    Ror,
    /// Rotate-through-carry left.
    Rcl,
    /// Rotate-through-carry right.
    Rcr,
    /// Shift left (SHL / SAL).
    Shl,
    /// Shift right (logical).
    Shr,
    /// Arithmetic shift right (SAR).
    Sar,
}

/// Source of a shift / rotate count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftCount {
    /// Immediate count (literal byte from the encoding).
    Imm(u8),
    /// CL register.
    Cl,
}

/// Condition mnemonic for `LOOP` family terminators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopCond {
    /// `LOOPNE`/`LOOPNZ`: take if `ECX != 0 && ZF == 0`.
    Loopne,
    /// `LOOPE`/`LOOPZ`: take if `ECX != 0 && ZF == 1`.
    Loope,
    /// `LOOP`: take if `ECX != 0`.
    Loop,
    /// `JCXZ`/`JECXZ`: take if `ECX == 0` (no decrement).
    Jcxz,
}

/// Kind of `CBW/CWDE/CWD/CDQ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbwOp {
    /// 0x98 with 16-bit operand: AL sign-extended to AX.
    Cbw,
    /// 0x98 with 32-bit operand: AX sign-extended to EAX.
    Cwde,
    /// 0x99 with 16-bit operand: AX sign-extended to DX:AX.
    Cwd,
    /// 0x99 with 32-bit operand: EAX sign-extended to EDX:EAX.
    Cdq,
}

/// A register-register operand pair. Used by instructions where the
/// operand is a GPR or byte alias.
#[derive(Debug, Clone, Copy)]
pub struct RegOperand {
    /// Guest register.
    pub reg: GuestReg,
    /// Width of the access.
    pub size: Size,
}

/// Runtime memory operand. The effective address is computed at
/// execution time from the current register values because the EA
/// depends on base/index registers that may change mid-block.
#[derive(Debug, Clone, Copy)]
pub struct MemOperand {
    /// Segment register whose cached base applies. Resolved at decode
    /// time from the default segment and any segment override prefix.
    pub seg: SegReg32,
    /// Base register, or `None` for pure-displacement addressing
    /// (e.g. `MOV AL, [moffs]` or `[disp32]`).
    pub base: Option<DwordReg>,
    /// Scaled index register (32-bit SIB only).
    pub index: Option<DwordReg>,
    /// Scale shift for the index: 0, 1, 2, or 3 (== *1, *2, *4, *8).
    pub scale: u8,
    /// Sign-extended displacement.
    pub disp: i32,
    /// Address-size mode selecting how base/index are read and the
    /// mask applied to the final offset before segment-base add.
    pub addr_size: AddrSize,
}

/// Source that may be a register or a memory location.
#[derive(Debug, Clone, Copy)]
pub enum RmSource {
    /// Register operand.
    Reg(RegOperand),
    /// Memory operand with explicit width.
    Mem {
        /// Memory operand.
        mem: MemOperand,
        /// Width of the access.
        size: Size,
    },
}

/// Destination that may be a register or a memory location.
#[derive(Debug, Clone, Copy)]
pub enum RmDest {
    /// Register operand (width is on the `RegOperand`).
    Reg(RegOperand),
    /// Memory operand with explicit width.
    Mem {
        /// Memory operand.
        mem: MemOperand,
        /// Width of the access.
        size: Size,
    },
}

/// Target of an indirect JMP / CALL.
#[derive(Debug, Clone, Copy)]
pub enum IndirectTarget {
    /// Target in a register (full width per operand size).
    Reg(DwordReg),
    /// Target loaded from memory.
    Mem(MemOperand),
}

/// A single IR operation.
#[derive(Debug, Clone)]
pub enum IrOp {
    /// `dst = imm` (zero-extended to the operand width).
    MovImm {
        /// Destination register.
        dst: RegOperand,
        /// Immediate value.
        imm: u32,
    },
    /// `dst = src` (register-to-register move with no flag update).
    MovReg {
        /// Destination register.
        dst: RegOperand,
        /// Source register.
        src: RegOperand,
    },
    /// `mem = imm` (MOV r/m, imm with memory destination).
    MemMovImm {
        /// Destination memory operand.
        mem: MemOperand,
        /// Immediate value masked to the operand size.
        imm: u32,
        /// Width of the write.
        size: Size,
    },
    /// Load from memory into a register.
    MemRead {
        /// Destination register.
        dst: RegOperand,
        /// Memory source.
        mem: MemOperand,
    },
    /// Store a register into memory.
    MemWrite {
        /// Source register.
        src: RegOperand,
        /// Memory destination.
        mem: MemOperand,
    },
    /// Load the effective address into a register (no memory access).
    Lea {
        /// Destination register (width in the operand).
        dst: RegOperand,
        /// Address source.
        mem: MemOperand,
    },
    /// ALU op with both operands in registers.
    Alu {
        /// Destination register (also `lhs` unless op is `Cmp`/`Test`).
        dst: RegOperand,
        /// Right operand (source) register.
        src: RegOperand,
        /// ALU opcode.
        op: AluOp,
    },
    /// ALU op with register left and immediate right.
    AluImm {
        /// Destination register (also `lhs` unless op is `Cmp`/`Test`).
        dst: RegOperand,
        /// Immediate right operand.
        imm: u32,
        /// ALU opcode.
        op: AluOp,
    },
    /// ALU op reading memory as source, writing a register destination.
    /// Models opcodes like `ADD reg, [mem]` (0x03, 0x0B, ...). For
    /// `Cmp`/`Test` the destination is not written.
    AluRm {
        /// Destination register (also lhs unless op is `Cmp`/`Test`).
        dst: RegOperand,
        /// Memory right operand.
        mem: MemOperand,
        /// ALU opcode.
        op: AluOp,
    },
    /// ALU op reading a register as source, writing through memory as
    /// destination. Models opcodes like `ADD [mem], reg`. For
    /// `Cmp`/`Test` the destination is not written (TEST memory stores
    /// nothing; CMP memory would likewise just set flags).
    MemAluReg {
        /// Memory destination (also lhs unless op is `Cmp`/`Test`).
        mem: MemOperand,
        /// Register source.
        src: RegOperand,
        /// Width of the access.
        size: Size,
        /// ALU opcode.
        op: AluOp,
    },
    /// ALU op with memory left and immediate right (group 1 memory
    /// form, `ADD [mem], imm` etc.).
    MemAluImm {
        /// Memory destination (also lhs unless op is `Cmp`/`Test`).
        mem: MemOperand,
        /// Immediate right operand (sign-extended for `83`).
        imm: u32,
        /// Width of the access.
        size: Size,
        /// ALU opcode.
        op: AluOp,
    },
    /// Unary op on a register.
    Unary {
        /// Destination register.
        dst: RegOperand,
        /// Unary opcode.
        op: UnaryOp,
    },
    /// Unary op on a memory operand.
    MemUnary {
        /// Memory destination.
        mem: MemOperand,
        /// Width of the access.
        size: Size,
        /// Unary opcode.
        op: UnaryOp,
    },
    /// Shift / rotate.
    Shift {
        /// Destination (register or memory).
        dst: RmDest,
        /// Operand size.
        size: Size,
        /// Count source.
        count: ShiftCount,
        /// Shift opcode.
        op: ShiftOp,
    },
    /// Zero-extend move from a narrower source into a register.
    MovZx {
        /// Destination register (typically 16 or 32 bits).
        dst: RegOperand,
        /// Source register or memory (byte or word).
        src: RmSource,
        /// Source width (Byte or Word).
        src_size: Size,
    },
    /// Sign-extend move from a narrower source into a register.
    MovSx {
        /// Destination register (typically 16 or 32 bits).
        dst: RegOperand,
        /// Source register or memory (byte or word).
        src: RmSource,
        /// Source width (Byte or Word).
        src_size: Size,
    },
    /// Two-operand IMUL (`IMUL r, r/m`). `dst = dst * src`.
    ImulRegRm {
        /// Destination register.
        dst: RegOperand,
        /// Right-hand source (register or memory).
        src: RmSource,
        /// Operand size (Word or Dword).
        size: Size,
    },
    /// Three-operand IMUL (`IMUL r, r/m, imm`). `dst = src * imm`.
    ImulRegRmImm {
        /// Destination register.
        dst: RegOperand,
        /// Multiplicand (register or memory).
        src: RmSource,
        /// Sign-extended immediate multiplier.
        imm: i32,
        /// Operand size (Word or Dword).
        size: Size,
    },
    /// PUSH register (16 or 32 bit), respecting current stack address size.
    PushReg {
        /// Source register.
        src: RegOperand,
    },
    /// PUSH the pre-decrement stack pointer register value.
    ///
    /// `PUSH SP/ESP` must snapshot the source before the stack pointer is
    /// adjusted, so it cannot share the generic register-push lowering.
    PushStackPtr {
        /// Operand size selects `SP` vs `ESP`.
        size: Size,
    },
    /// PUSH immediate (sign-extended to operand size).
    PushImm {
        /// Value to push (already extended).
        imm: u32,
        /// Operand size (Word or Dword).
        size: Size,
    },
    /// PUSH memory operand (FF /6).
    PushMem {
        /// Memory source.
        mem: MemOperand,
        /// Operand size.
        size: Size,
    },
    /// POP into a register.
    PopReg {
        /// Destination register.
        dst: RegOperand,
    },
    /// POP into memory (8F /0).
    PopMem {
        /// Memory destination.
        mem: MemOperand,
        /// Operand size.
        size: Size,
    },
    /// XCHG between two registers.
    XchgReg {
        /// First register.
        a: RegOperand,
        /// Second register.
        b: RegOperand,
    },
    /// SETcc r/m8.
    SetCc {
        /// Destination (reg or mem, byte only).
        dst: RmDest,
        /// Condition code.
        cond: IrCond,
    },
    /// CBW / CWDE / CWD / CDQ.
    CbwCwd {
        /// Which variant.
        op: CbwOp,
    },
    /// Modify a flag directly.
    Flag(FlagOp),
    /// `PUSHA`/`PUSHAD`: push AX/CX/DX/BX/SP-snapshot/BP/SI/DI (or 32-bit
    /// variants), preserving the original SP value in the pushed-SP slot.
    PushAll {
        /// Operand size (`Word` or `Dword`) selects PUSHA vs PUSHAD.
        size: Size,
    },
    /// `POPA`/`POPAD`: pop DI/SI/BP/SP-discard/BX/DX/CX/AX.
    PopAll {
        /// Operand size (`Word` or `Dword`) selects POPA vs POPAD.
        size: Size,
    },
    /// `PUSHF`/`PUSHFD`: push the compressed EFLAGS word/dword.
    PushFlags {
        /// Operand size (`Word` or `Dword`).
        size: Size,
    },
    /// `POPF`/`POPFD`: pop and load EFLAGS with CPL masking.
    PopFlags {
        /// Operand size (`Word` or `Dword`).
        size: Size,
    },
    /// `SAHF`: load SF/ZF/AF/PF/CF from AH.
    Sahf,
    /// `LAHF`: store SF/ZF/AF/PF/CF into AH.
    Lahf,
    /// One-operand `MUL`/`IMUL` using the AL/AX/EAX accumulator and
    /// AH/DX/EDX as the upper-half destination. Sets CF/OF based on
    /// whether the upper half is significant.
    MulAcc {
        /// Multiplicand.
        src: RmSource,
        /// Operand size (`Byte`, `Word`, or `Dword`).
        size: Size,
        /// `true` for `IMUL`, `false` for `MUL`.
        signed: bool,
    },
    /// One-operand `DIV`/`IDIV` using the accumulator pair as dividend.
    /// May raise `#DE` on divide-by-zero or quotient overflow; both
    /// backends must consult `fault_pending` after the helper returns.
    DivAcc {
        /// Divisor.
        src: RmSource,
        /// Operand size (`Byte`, `Word`, or `Dword`).
        size: Size,
        /// `true` for `IDIV`, `false` for `DIV`.
        signed: bool,
    },
    /// `BSWAP r32`: reverse the byte order of a 32-bit register.
    /// Encoded with a 16-bit operand size the behavior is undefined on
    /// real silicon; we match the interpreter and reverse the dword
    /// regardless of operand size.
    Bswap {
        /// Destination GPR (always dword-sized).
        dst: DwordReg,
    },
    /// `BSF r, r/m` or `BSR r, r/m`: find first/last set bit. Sets ZF
    /// based on whether the source was nonzero; writes the bit index
    /// into the destination register when ZF=1.
    BitScan {
        /// Destination register.
        dst: RegOperand,
        /// Source (register or memory).
        src: RmSource,
        /// Operand size (`Word` or `Dword`).
        size: Size,
        /// `true` scans high-to-low (BSR), `false` low-to-high (BSF).
        reverse: bool,
    },
    /// Run a single interpreter step inline within the block.
    ///
    /// Used for instructions whose semantics carry too much prefix /
    /// address-size / segment state to reproduce compactly in IR (bit
    /// tests on memory with sign-extended register indexes, SHLD/SHRD,
    /// XADD, CMPXCHG, port IN/OUT, XLAT, ENTER). The backend sets the
    /// guest EIP to `instr_start_eip` so the interpreter refetches the
    /// full instruction from the original bytes, then invokes
    /// `step_instruction`. Cycle accounting is charged dynamically by
    /// the interpreter; the decoder must *not* include these opcodes
    /// in the block's static `cycles` sum.
    ///
    /// After running, the backend compares `state.eip()` against
    /// `instr_end_eip`. A mismatch means the interpreter delivered a
    /// fault (or otherwise branched) and the remaining block body is
    /// stale, so the backend must bail out. Without this the outer
    /// block exit would overwrite the handler EIP on fall-through (see
    /// `raise_fault_with_code` in `crates/cpu/src/i386/interrupt.rs`,
    /// which clears `fault_pending` once the fault is delivered).
    InterpCall {
        /// EIP of the first byte of the instruction (before any
        /// prefix bytes). The interpreter restarts from here.
        instr_start_eip: u32,
        /// EIP immediately after the last byte of the instruction.
        /// Used as a "no fault, no branch" sentinel by the backends.
        instr_end_eip: u32,
    },
    /// No-op placeholder (for NOP and for padding when a decoded guest
    /// instruction has no IR effect).
    Nop,
    /// Marks the start of a new guest instruction. Updates guest
    /// `ip`/`ip_upper` and `prev_ip`/`prev_ip_upper` to the given EIP
    /// so any subsequent fault raised inside this instruction's ops
    /// (page fault in a memory op, #GP from a segment-limit check,
    /// etc.) reports the correct faulting EIP. Emitted by the decoder
    /// at the start of every instruction within a block.
    InstrStart {
        /// Guest EIP of the instruction's first byte (before prefixes).
        eip: u32,
    },
}

/// How a block terminates. The dispatcher reads this after the backend
/// executes the body to decide what to do next.
#[derive(Debug, Clone, Copy)]
pub enum BlockExit {
    /// Fall through to the next sequential instruction stream EIP.
    ///
    /// Unlike near control transfers in 16-bit mode, this preserves the
    /// full architectural `EIP`, including any carry into the upper half.
    Fallthrough {
        /// Next sequential EIP.
        next_eip: u32,
    },
    /// Unconditional direct jump to a known EIP.
    DirectJump {
        /// Target EIP.
        target_eip: u32,
    },
    /// Conditional branch: taken EIP and fall-through EIP are both known.
    ConditionalJump {
        /// Branch condition.
        cond: IrCond,
        /// Target EIP when taken.
        taken_eip: u32,
        /// Target EIP when not taken.
        fallthrough_eip: u32,
    },
    /// `LOOP`/`LOOPE`/`LOOPNE`/`JCXZ`: decrements ECX (except JCXZ) and
    /// branches based on ECX and optionally ZF.
    CondLoop {
        /// Which LOOP variant.
        cond: LoopCond,
        /// Target EIP when taken.
        taken_eip: u32,
        /// Target EIP when not taken.
        fallthrough_eip: u32,
        /// Address-size selects CX vs ECX.
        addr_size: AddrSize,
    },
    /// Direct near CALL: push return EIP, jump to target.
    DirectCall {
        /// Target EIP.
        target_eip: u32,
        /// Return address (EIP of the instruction after the CALL).
        return_eip: u32,
        /// Operand size (Word or Dword) used for the pushed return addr.
        size: Size,
    },
    /// Indirect near CALL (FF /2): push return EIP, jump to value from
    /// register or memory.
    IndirectCall {
        /// Source of the target.
        target: IndirectTarget,
        /// Return address.
        return_eip: u32,
        /// Operand size.
        size: Size,
    },
    /// Indirect near JMP (FF /4): jump to value from register or memory.
    IndirectJump {
        /// Source of the target.
        target: IndirectTarget,
        /// Operand size.
        size: Size,
    },
    /// Near RET (no imm): pop EIP.
    Return {
        /// Operand size (Word or Dword) of the popped return address.
        size: Size,
    },
    /// Near RET imm16: pop EIP then advance SP by `extra_sp`.
    ReturnImm {
        /// Operand size (Word or Dword) of the popped return address.
        size: Size,
        /// Extra bytes to add to SP/ESP after the pop.
        extra_sp: u32,
    },
    /// `HLT`: advances EIP to the next instruction and enters the halted state.
    Halt {
        /// EIP of the instruction after `HLT`.
        next_eip: u32,
    },
    /// Unsupported instruction: dispatcher must set EIP to `guest_eip`
    /// and single-step the interpreter once.
    Fallback {
        /// EIP of the unsupported instruction.
        guest_eip: u32,
    },
}

/// A decoded block of guest instructions lowered to IR.
#[derive(Debug)]
pub struct Block {
    /// Physical linear address of the first guest byte in the block.
    pub phys_addr: u32,
    /// Total static cycle cost.
    pub cycles: i64,
    /// IR body.
    pub ops: Vec<IrOp>,
    /// Terminator.
    pub exit: BlockExit,
    /// Whether the block was decoded with 32-bit stack addressing (SS B-bit).
    pub stack32: bool,
}

impl Block {
    /// Maximum guest instructions per block.
    pub const MAX_INSTRS: u16 = 32;
}
