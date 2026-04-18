use std::{marker::PhantomData, mem::offset_of, ptr};

use common::Bus;
use cpu::{DwordReg, I386, I386Flags, I386State, SegReg32, TlbCache};

use crate::{
    backend_bytecode::BlockOutcome,
    code_cache::CodeCache,
    ir::{
        AddrSize, AluOp, Block, BlockExit, CbwOp, FlagOp, GuestReg, IndirectTarget, IrCond, IrOp,
        LoopCond, MemOperand, RegOperand, RmDest, RmSource, ShiftCount, ShiftOp, Size, UnaryOp,
    },
};

type NativeEntry = unsafe extern "C" fn(*mut JitRuntimeCtx) -> u32;

#[repr(u32)]
enum NativeOutcome {
    Continue = 0,
    Cycles = 1,
    Fallback = 2,
    Fault = 3,
    Halt = 4,
}

pub(crate) struct CompiledBlock {
    entry: NativeEntry,
}

pub(crate) struct CompilerScratch {
    code: Vec<u8>,
    fault_exits: Vec<usize>,
}

impl CompilerScratch {
    pub(crate) fn new() -> Self {
        Self {
            code: Vec::new(),
            fault_exits: Vec::new(),
        }
    }
}

#[repr(C)]
struct JitRuntimeCtx {
    cpu: *mut (),
    state: *mut I386State,
    bus: *mut (),
    helpers: *const JitHelperTable,
    cycles_remaining: i64,
    exit_eip: u32,
}

#[repr(C)]
struct JitHelperTable {
    check_segment_access: unsafe extern "C" fn(*mut (), *mut (), u32, u32, u32, u32) -> i64,
    read_byte_seg: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
    read_word_seg: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
    read_dword_seg: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
    write_byte_seg: unsafe extern "C" fn(*mut (), *mut (), u32, u32, u32) -> i64,
    write_word_seg: unsafe extern "C" fn(*mut (), *mut (), u32, u32, u32) -> i64,
    write_dword_seg: unsafe extern "C" fn(*mut (), *mut (), u32, u32, u32) -> i64,
    push_word: unsafe extern "C" fn(*mut (), *mut (), u32, bool) -> i64,
    push_dword: unsafe extern "C" fn(*mut (), *mut (), u32, bool) -> i64,
    pop_word: unsafe extern "C" fn(*mut (), *mut (), bool) -> i64,
    pop_dword: unsafe extern "C" fn(*mut (), *mut (), bool) -> i64,
    shift_byte_cl: unsafe extern "C" fn(*mut I386State, u32, u32, u32) -> u32,
    pusha: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    popa: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    pushf: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    popf: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    sahf: unsafe extern "C" fn(*mut I386State),
    lahf: unsafe extern "C" fn(*mut I386State),
    neg: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> u32,
    mul_acc: unsafe extern "C" fn(*mut (), *mut (), u32, u32, u32),
    div_acc: unsafe extern "C" fn(*mut (), *mut (), u32, u32, u32) -> i64,
    bit_scan: unsafe extern "C" fn(*mut I386State, u32, u32, u32) -> u32,
    interp_one: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
    instr_start: unsafe extern "C" fn(*mut (), u32),
    read_byte_phys: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    read_word_phys: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    read_dword_phys: unsafe extern "C" fn(*mut (), *mut (), u32) -> i64,
    write_byte_phys: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
    write_word_phys: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
    write_dword_phys: unsafe extern "C" fn(*mut (), *mut (), u32, u32) -> i64,
}

const CTX_CPU_OFFSET: i32 = offset_of!(JitRuntimeCtx, cpu) as i32;
const CTX_STATE_OFFSET: i32 = offset_of!(JitRuntimeCtx, state) as i32;
const CTX_BUS_OFFSET: i32 = offset_of!(JitRuntimeCtx, bus) as i32;
const CTX_HELPERS_OFFSET: i32 = offset_of!(JitRuntimeCtx, helpers) as i32;
const CTX_CYCLES_OFFSET: i32 = offset_of!(JitRuntimeCtx, cycles_remaining) as i32;
const CTX_EXIT_EIP_OFFSET: i32 = offset_of!(JitRuntimeCtx, exit_eip) as i32;

const HELP_CHECK_SEGMENT_ACCESS_OFFSET: i32 =
    offset_of!(JitHelperTable, check_segment_access) as i32;
const HELP_READ_BYTE_OFFSET: i32 = offset_of!(JitHelperTable, read_byte_seg) as i32;
const HELP_READ_WORD_OFFSET: i32 = offset_of!(JitHelperTable, read_word_seg) as i32;
const HELP_READ_DWORD_OFFSET: i32 = offset_of!(JitHelperTable, read_dword_seg) as i32;
const HELP_WRITE_BYTE_OFFSET: i32 = offset_of!(JitHelperTable, write_byte_seg) as i32;
const HELP_WRITE_WORD_OFFSET: i32 = offset_of!(JitHelperTable, write_word_seg) as i32;
const HELP_WRITE_DWORD_OFFSET: i32 = offset_of!(JitHelperTable, write_dword_seg) as i32;
const HELP_PUSH_WORD_OFFSET: i32 = offset_of!(JitHelperTable, push_word) as i32;
const HELP_PUSH_DWORD_OFFSET: i32 = offset_of!(JitHelperTable, push_dword) as i32;
const HELP_POP_WORD_OFFSET: i32 = offset_of!(JitHelperTable, pop_word) as i32;
const HELP_POP_DWORD_OFFSET: i32 = offset_of!(JitHelperTable, pop_dword) as i32;
const HELP_SHIFT_BYTE_CL_OFFSET: i32 = offset_of!(JitHelperTable, shift_byte_cl) as i32;
const HELP_PUSHA_OFFSET: i32 = offset_of!(JitHelperTable, pusha) as i32;
const HELP_POPA_OFFSET: i32 = offset_of!(JitHelperTable, popa) as i32;
const HELP_PUSHF_OFFSET: i32 = offset_of!(JitHelperTable, pushf) as i32;
const HELP_POPF_OFFSET: i32 = offset_of!(JitHelperTable, popf) as i32;
const HELP_SAHF_OFFSET: i32 = offset_of!(JitHelperTable, sahf) as i32;
const HELP_LAHF_OFFSET: i32 = offset_of!(JitHelperTable, lahf) as i32;
const HELP_NEG_OFFSET: i32 = offset_of!(JitHelperTable, neg) as i32;
const HELP_MUL_ACC_OFFSET: i32 = offset_of!(JitHelperTable, mul_acc) as i32;
const HELP_DIV_ACC_OFFSET: i32 = offset_of!(JitHelperTable, div_acc) as i32;
const HELP_BIT_SCAN_OFFSET: i32 = offset_of!(JitHelperTable, bit_scan) as i32;
const HELP_INTERP_ONE_OFFSET: i32 = offset_of!(JitHelperTable, interp_one) as i32;
const HELP_INSTR_START_OFFSET: i32 = offset_of!(JitHelperTable, instr_start) as i32;
const HELP_READ_BYTE_PHYS_OFFSET: i32 = offset_of!(JitHelperTable, read_byte_phys) as i32;
const HELP_READ_WORD_PHYS_OFFSET: i32 = offset_of!(JitHelperTable, read_word_phys) as i32;
const HELP_READ_DWORD_PHYS_OFFSET: i32 = offset_of!(JitHelperTable, read_dword_phys) as i32;
const HELP_WRITE_BYTE_PHYS_OFFSET: i32 = offset_of!(JitHelperTable, write_byte_phys) as i32;
const HELP_WRITE_WORD_PHYS_OFFSET: i32 = offset_of!(JitHelperTable, write_word_phys) as i32;
const HELP_WRITE_DWORD_PHYS_OFFSET: i32 = offset_of!(JitHelperTable, write_dword_phys) as i32;

const REGS_OFFSET: i32 = offset_of!(I386State, regs) as i32;
const IP_OFFSET: i32 = offset_of!(I386State, ip) as i32;
const IP_UPPER_OFFSET: i32 = offset_of!(I386State, ip_upper) as i32;
const FLAGS_OFFSET: i32 = offset_of!(I386State, flags) as i32;
const FLAG_SIGN_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, sign_val) as i32;
const FLAG_ZERO_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, zero_val) as i32;
const FLAG_CARRY_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, carry_val) as i32;
const FLAG_OVERFLOW_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, overflow_val) as i32;
const FLAG_AUX_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, aux_val) as i32;
const FLAG_PARITY_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, parity_val) as i32;
const FLAG_DF_OFFSET: i32 = FLAGS_OFFSET + offset_of!(I386Flags, df) as i32;

const SEG_BASES_OFFSET: i32 = offset_of!(I386State, seg_bases) as i32;
const CR0_OFFSET: i32 = offset_of!(I386State, cr0) as i32;
const TLB_OFFSET: i32 = offset_of!(I386State, tlb) as i32;
const TLB_VALID_OFFSET: i32 = TLB_OFFSET + offset_of!(TlbCache, valid) as i32;
const TLB_TAG_OFFSET: i32 = TLB_OFFSET + offset_of!(TlbCache, tag) as i32;
const TLB_PHYS_OFFSET: i32 = TLB_OFFSET + offset_of!(TlbCache, phys) as i32;
const TLB_WRITABLE_OFFSET: i32 = TLB_OFFSET + offset_of!(TlbCache, writable) as i32;
const TLB_DIRTY_OFFSET: i32 = TLB_OFFSET + offset_of!(TlbCache, dirty) as i32;

const REG_SLOT_SIZE: i32 = core::mem::size_of::<u32>() as i32;

/// Per-CPU reusable scratch for the x64 backend dispatcher.
///
/// Owns the helper vtable (on the heap, so its address is stable and
/// its pointer can safely be stored in `runtime.helpers` at
/// construction) and a reusable `JitRuntimeCtx`. References to the
/// guest `I386` and `Bus` are rebound on every `execute_block` call:
/// they never outlive the single entry-point invocation, and nothing
/// else in `RunState` aliases them, so the bound is strictly narrower
/// than the surrounding `run_for` loop.
pub(crate) struct RunState<const CPU_MODEL: u8, B: Bus> {
    /// Heap-stable helper vtable. `runtime.helpers` holds a raw pointer
    /// into this box; the box keeps the table's address invariant under
    /// moves of `RunState`, and dropping the box frees the vtable last.
    #[allow(dead_code)]
    helpers: Box<JitHelperTable>,
    runtime: JitRuntimeCtx,
    _marker: PhantomData<fn(&mut I386<CPU_MODEL>, &mut B)>,
}

impl<const CPU_MODEL: u8, B: Bus> RunState<CPU_MODEL, B> {
    pub(crate) fn new() -> Self {
        let helpers = Box::new(JitHelperTable {
            check_segment_access: helper_check_segment_access::<CPU_MODEL, B>,
            read_byte_seg: helper_read_byte_seg::<CPU_MODEL, B>,
            read_word_seg: helper_read_word_seg::<CPU_MODEL, B>,
            read_dword_seg: helper_read_dword_seg::<CPU_MODEL, B>,
            write_byte_seg: helper_write_byte_seg::<CPU_MODEL, B>,
            write_word_seg: helper_write_word_seg::<CPU_MODEL, B>,
            write_dword_seg: helper_write_dword_seg::<CPU_MODEL, B>,
            push_word: helper_push_word::<CPU_MODEL, B>,
            push_dword: helper_push_dword::<CPU_MODEL, B>,
            pop_word: helper_pop_word::<CPU_MODEL, B>,
            pop_dword: helper_pop_dword::<CPU_MODEL, B>,
            shift_byte_cl: helper_shift_byte_cl,
            pusha: helper_pusha::<CPU_MODEL, B>,
            popa: helper_popa::<CPU_MODEL, B>,
            pushf: helper_pushf::<CPU_MODEL, B>,
            popf: helper_popf::<CPU_MODEL, B>,
            sahf: helper_sahf,
            lahf: helper_lahf,
            neg: helper_neg::<CPU_MODEL>,
            mul_acc: helper_mul_acc::<CPU_MODEL>,
            div_acc: helper_div_acc::<CPU_MODEL, B>,
            bit_scan: helper_bit_scan,
            interp_one: helper_interp_one::<CPU_MODEL, B>,
            instr_start: helper_instr_start::<CPU_MODEL>,
            read_byte_phys: helper_read_byte_phys::<B>,
            read_word_phys: helper_read_word_phys::<B>,
            read_dword_phys: helper_read_dword_phys::<B>,
            write_byte_phys: helper_write_byte_phys::<B>,
            write_word_phys: helper_write_word_phys::<B>,
            write_dword_phys: helper_write_dword_phys::<B>,
        });
        let helpers_ptr: *const JitHelperTable = &*helpers;
        let runtime = JitRuntimeCtx {
            cpu: ptr::null_mut(),
            state: ptr::null_mut(),
            bus: ptr::null_mut(),
            helpers: helpers_ptr,
            cycles_remaining: 0,
            exit_eip: 0,
        };
        Self {
            helpers,
            runtime,
            _marker: PhantomData,
        }
    }

    pub(crate) fn execute_block(
        &mut self,
        cpu: &mut I386<CPU_MODEL>,
        bus: &mut B,
        compiled: &CompiledBlock,
        block: &Block,
        cycles_remaining: i64,
    ) -> (BlockOutcome, u64, i64) {
        // Mirror `backend_bytecode::execute_block`: clear any residual
        // `fault_pending` from the previous block's delivery, and
        // refresh `prev_ip` so a first-instruction fault reports the
        // block-start EIP. `IrOp::InstrStart` at each instruction
        // boundary refreshes per-instruction; this handles the prelude
        // before the first `InstrStart` runs.
        cpu.clear_fault_pending();
        cpu.jit_refresh_prev_ip();

        self.runtime.cpu = cpu as *mut I386<CPU_MODEL> as *mut ();
        self.runtime.state = &mut cpu.state as *mut I386State;
        self.runtime.bus = bus as *mut B as *mut ();
        self.runtime.cycles_remaining = cycles_remaining;
        self.runtime.exit_eip = 0;

        // SAFETY: `compiled.entry` expects a valid pointer to `JitRuntimeCtx`.
        let outcome = unsafe { (compiled.entry)(&mut self.runtime) };

        let block_outcome = match outcome {
            x if x == NativeOutcome::Continue as u32 => BlockOutcome::Continue,
            x if x == NativeOutcome::Cycles as u32 => BlockOutcome::Cycles,
            x if x == NativeOutcome::Fallback as u32 => BlockOutcome::Fallback {
                guest_eip: self.runtime.exit_eip,
            },
            x if x == NativeOutcome::Fault as u32 => BlockOutcome::Fault,
            x if x == NativeOutcome::Halt as u32 => BlockOutcome::Halt,
            _ => unreachable!("x64 dynarec: invalid block outcome"),
        };

        let executed = match block_outcome {
            BlockOutcome::Continue | BlockOutcome::Fallback { .. } | BlockOutcome::Halt => {
                block.ops.len() as u64
            }
            BlockOutcome::Cycles | BlockOutcome::Fault => 0,
        };

        let cycles = self.runtime.cycles_remaining;
        self.runtime.cpu = ptr::null_mut();
        self.runtime.state = ptr::null_mut();
        self.runtime.bus = ptr::null_mut();
        (block_outcome, executed, cycles)
    }
}

pub(crate) fn compile_block(
    cache: &mut CodeCache,
    block: &Block,
    scratch: &mut CompilerScratch,
) -> Option<CompiledBlock> {
    scratch.code.clear();
    scratch.fault_exits.clear();

    let mut emitter = Emitter::new(&mut scratch.code);
    emitter.push_r64(GpReg::Rbp);
    emitter.push_r64(GpReg::Rbx);
    emitter.push_r64(GpReg::R12);
    emitter.push_r64(GpReg::R13);
    emitter.push_r64(GpReg::R14);
    emitter.push_r64(GpReg::R15);
    emitter.sub_rsp_imm8(8);
    emitter.mov_m64_r64(GpReg::Rsp, 0, GpReg::Rdi);

    emitter.mov_r64_m64(GpReg::Rbp, GpReg::Rdi, CTX_STATE_OFFSET);
    emitter.mov_r64_m64(GpReg::Rbx, GpReg::Rdi, CTX_CPU_OFFSET);
    emitter.mov_r64_m64(GpReg::R12, GpReg::Rdi, CTX_BUS_OFFSET);
    emitter.mov_r64_m64(GpReg::R13, GpReg::Rdi, CTX_HELPERS_OFFSET);
    emitter.mov_r64_m64(GpReg::R14, GpReg::Rdi, CTX_CYCLES_OFFSET);
    emitter.sub_r64_imm32(GpReg::R14, block.cycles as i32);
    let cycles_exit = emitter.jcc_rel32_placeholder(ConditionCode::L);

    let mut context = LoweringCtx {
        emitter: &mut emitter,
        block,
        fault_exits: &mut scratch.fault_exits,
    };

    for op in &block.ops {
        lower_op(&mut context, op);
    }
    lower_exit(&mut context, block);

    let cycles_label = context.emitter.position();
    emit_return_with_outcome(context.emitter, NativeOutcome::Cycles as u32);
    let fault_label = context.emitter.position();
    emit_return_with_outcome(context.emitter, NativeOutcome::Fault as u32);

    context
        .emitter
        .patch_rel32_branches(&[cycles_exit], cycles_label);
    context
        .emitter
        .patch_rel32_branches(context.fault_exits, fault_label);

    let entry_ptr = cache.alloc(16, scratch.code.len())?;
    // SAFETY: `entry_ptr` points to a writable allocation returned by the
    // code cache, and `bytes.len()` bytes are available there.
    unsafe {
        ptr::copy_nonoverlapping(
            scratch.code.as_ptr(),
            entry_ptr.as_ptr(),
            scratch.code.len(),
        );
    }

    Some(CompiledBlock {
        // SAFETY: the code cache now contains a valid function body with the
        // `NativeEntry` ABI emitted below.
        entry: unsafe { std::mem::transmute::<*mut u8, NativeEntry>(entry_ptr.as_ptr()) },
    })
}

// The helpers below are declared `extern "C"` and are called from the
// emitted x64 code. A panic that unwinds through a helper would cross
// the native-code frame boundary, which is UB. We rely on Rust's
// default abort-on-panic behavior for `extern "C"` functions (stable
// since Rust 1.81). If a future edition ever changes this default, the
// helpers must either be wrapped in `std::panic::catch_unwind` or
// declared `extern "C-unwind"` with matching cleanup in the emitted
// code.
unsafe extern "C" fn helper_read_byte_seg<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let value = cpu.read_byte_seg(bus, seg_reg(seg), offset) as u32;
    if cpu.fault_pending() {
        -1
    } else {
        i64::from(value)
    }
}

unsafe extern "C" fn helper_check_segment_access<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
    size: u32,
    write: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    if cpu.jit_check_segment_access(bus, seg_reg(seg), offset, size as u8, write != 0) {
        0
    } else {
        -1
    }
}

unsafe extern "C" fn helper_read_word_seg<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let value = cpu.read_word_seg(bus, seg_reg(seg), offset) as u32;
    if cpu.fault_pending() {
        -1
    } else {
        i64::from(value)
    }
}

unsafe extern "C" fn helper_read_dword_seg<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let value = cpu.read_dword_seg(bus, seg_reg(seg), offset);
    if cpu.fault_pending() {
        -1
    } else {
        i64::from(value)
    }
}

unsafe extern "C" fn helper_write_byte_seg<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
    value: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.write_byte_seg(bus, seg_reg(seg), offset, value as u8);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_write_word_seg<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
    value: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.write_word_seg(bus, seg_reg(seg), offset, value as u16);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_write_dword_seg<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    seg: u32,
    offset: u32,
    value: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.write_dword_seg(bus, seg_reg(seg), offset, value);
    if cpu.fault_pending() { -1 } else { 0 }
}

/// Fast-path helpers invoked after the inline TLB probe. These take a
/// pre-translated physical address and dispatch straight through the
/// bus, skipping `translate_linear` entirely.
unsafe extern "C" fn helper_read_byte_phys<B: Bus>(_cpu: *mut (), bus: *mut (), phys: u32) -> i64 {
    let bus = unsafe { &mut *(bus as *mut B) };
    i64::from(bus.read_byte(phys))
}

unsafe extern "C" fn helper_read_word_phys<B: Bus>(_cpu: *mut (), bus: *mut (), phys: u32) -> i64 {
    let bus = unsafe { &mut *(bus as *mut B) };
    i64::from(bus.read_word(phys))
}

unsafe extern "C" fn helper_read_dword_phys<B: Bus>(_cpu: *mut (), bus: *mut (), phys: u32) -> i64 {
    let bus = unsafe { &mut *(bus as *mut B) };
    i64::from(bus.read_dword(phys))
}

unsafe extern "C" fn helper_write_byte_phys<B: Bus>(
    _cpu: *mut (),
    bus: *mut (),
    phys: u32,
    value: u32,
) -> i64 {
    let bus = unsafe { &mut *(bus as *mut B) };
    bus.write_byte(phys, value as u8);
    0
}

unsafe extern "C" fn helper_write_word_phys<B: Bus>(
    _cpu: *mut (),
    bus: *mut (),
    phys: u32,
    value: u32,
) -> i64 {
    let bus = unsafe { &mut *(bus as *mut B) };
    bus.write_word(phys, value as u16);
    0
}

unsafe extern "C" fn helper_write_dword_phys<B: Bus>(
    _cpu: *mut (),
    bus: *mut (),
    phys: u32,
    value: u32,
) -> i64 {
    let bus = unsafe { &mut *(bus as *mut B) };
    bus.write_dword(phys, value);
    0
}

unsafe extern "C" fn helper_push_word<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    value: u32,
    stack32: bool,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let new_sp = if stack32 {
        let esp = cpu.state.regs.dword(DwordReg::ESP).wrapping_sub(2);
        cpu.state.regs.set_dword(DwordReg::ESP, esp);
        esp
    } else {
        let sp = (cpu.state.regs.dword(DwordReg::ESP) as u16).wrapping_sub(2);
        let current = cpu.state.regs.dword(DwordReg::ESP);
        cpu.state
            .regs
            .set_dword(DwordReg::ESP, (current & 0xFFFF_0000) | u32::from(sp));
        u32::from(sp)
    };
    cpu.write_word_seg(bus, SegReg32::SS, new_sp, value as u16);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_push_dword<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    value: u32,
    stack32: bool,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let new_sp = if stack32 {
        let esp = cpu.state.regs.dword(DwordReg::ESP).wrapping_sub(4);
        cpu.state.regs.set_dword(DwordReg::ESP, esp);
        esp
    } else {
        let sp = (cpu.state.regs.dword(DwordReg::ESP) as u16).wrapping_sub(4);
        let current = cpu.state.regs.dword(DwordReg::ESP);
        cpu.state
            .regs
            .set_dword(DwordReg::ESP, (current & 0xFFFF_0000) | u32::from(sp));
        u32::from(sp)
    };
    cpu.write_dword_seg(bus, SegReg32::SS, new_sp, value);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_pop_word<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    stack32: bool,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let sp = if stack32 {
        cpu.state.regs.dword(DwordReg::ESP)
    } else {
        u32::from(cpu.state.regs.dword(DwordReg::ESP) as u16)
    };
    let value = cpu.read_word_seg(bus, SegReg32::SS, sp);
    if cpu.fault_pending() {
        return -1;
    }
    if stack32 {
        cpu.state.regs.set_dword(DwordReg::ESP, sp.wrapping_add(2));
    } else {
        let new_sp = (sp as u16).wrapping_add(2);
        let current = cpu.state.regs.dword(DwordReg::ESP);
        cpu.state
            .regs
            .set_dword(DwordReg::ESP, (current & 0xFFFF_0000) | u32::from(new_sp));
    }
    i64::from(value as u32)
}

unsafe extern "C" fn helper_pop_dword<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    stack32: bool,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let sp = if stack32 {
        cpu.state.regs.dword(DwordReg::ESP)
    } else {
        u32::from(cpu.state.regs.dword(DwordReg::ESP) as u16)
    };
    let value = cpu.read_dword_seg(bus, SegReg32::SS, sp);
    if cpu.fault_pending() {
        return -1;
    }
    if stack32 {
        cpu.state.regs.set_dword(DwordReg::ESP, sp.wrapping_add(4));
    } else {
        let new_sp = (sp as u16).wrapping_add(4);
        let current = cpu.state.regs.dword(DwordReg::ESP);
        cpu.state
            .regs
            .set_dword(DwordReg::ESP, (current & 0xFFFF_0000) | u32::from(new_sp));
    }
    i64::from(value)
}

unsafe extern "C" fn helper_pusha<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    size: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.jit_pusha(bus, size as u8);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_popa<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    size: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.jit_popa(bus, size as u8);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_pushf<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    size: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.jit_pushf(bus, size as u8);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_popf<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    size: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.jit_popf(bus, size as u8);
    if cpu.fault_pending() { -1 } else { 0 }
}

unsafe extern "C" fn helper_sahf(state: *mut I386State) {
    let state = unsafe { &mut *state };
    let ah = state.regs.byte(cpu::ByteReg::AH);
    state.flags.carry_val = (ah & 0x01) as u32;
    state.flags.parity_val = if ah & 0x04 != 0 { 0 } else { 1 };
    state.flags.aux_val = (ah & 0x10) as u32;
    state.flags.zero_val = if ah & 0x40 != 0 { 0 } else { 1 };
    state.flags.sign_val = if ah & 0x80 != 0 { -1 } else { 0 };
}

unsafe extern "C" fn helper_lahf(state: *mut I386State) {
    let state = unsafe { &mut *state };
    let packed = state.flags.compress() as u8;
    state.regs.set_byte(cpu::ByteReg::AH, packed);
}

unsafe extern "C" fn helper_neg<const CPU_MODEL: u8>(
    cpu: *mut (),
    _bus: *mut (),
    value: u32,
    size: u32,
) -> u32 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    cpu.jit_neg(value, size as u8)
}

unsafe extern "C" fn helper_mul_acc<const CPU_MODEL: u8>(
    cpu: *mut (),
    _bus: *mut (),
    value: u32,
    size: u32,
    signed: u32,
) {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    cpu.jit_mul(value, size as u8, signed != 0);
}

unsafe extern "C" fn helper_div_acc<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    value: u32,
    size: u32,
    signed: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    let pre_eip = cpu.state.eip();
    cpu.jit_div(value, size as u8, signed != 0, bus);
    if cpu.fault_pending() || cpu.state.eip() != pre_eip {
        -1
    } else {
        0
    }
}

/// Runs a single interpreter step starting at `instr_start_eip`.
/// Restores EIP first so the interpreter re-fetches prefix, opcode,
/// modrm and any operand bytes from the original guest memory. Used
/// for complex opcodes whose semantics are too state-heavy to lower
/// to IR directly (bit tests with register bit-index into memory,
/// SHLD/SHRD, XADD, CMPXCHG, IN/OUT, XLAT, ENTER). Cycle accounting
/// is done by the interpreter's own `clk` calls; the JIT decoder
/// must not add static cycles for these opcodes.
///
/// Returns `-1` when the interpreter raised a fault or otherwise
/// branched (EIP no longer points at `instr_end_eip`), so the x64
/// lowering can exit the block with `BlockOutcome::Fault`. A plain
/// fault-pending check is insufficient because
/// `raise_fault_with_code` clears `fault_pending` once the handler is
/// successfully entered, leaving EIP at the handler while reporting
/// "no fault" back to the caller (see
/// `crates/cpu/src/i386/interrupt.rs`).
unsafe extern "C" fn helper_interp_one<const CPU_MODEL: u8, B: Bus>(
    cpu: *mut (),
    bus: *mut (),
    instr_start_eip: u32,
    instr_end_eip: u32,
) -> i64 {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    let bus = unsafe { &mut *(bus as *mut B) };
    cpu.state.set_eip(instr_start_eip);
    cpu.step_instruction(bus);
    if cpu.fault_pending() || cpu.state.eip() != instr_end_eip {
        -1
    } else {
        0
    }
}

/// Updates guest `ip`/`ip_upper`/`prev_ip`/`prev_ip_upper` to the
/// instruction's start EIP. Emitted at every `InstrStart` marker so
/// a subsequent fault raised inside the instruction reports the
/// correct faulting EIP (see `I386::jit_refresh_prev_ip` in
/// `crates/cpu/src/i386.rs`).
unsafe extern "C" fn helper_instr_start<const CPU_MODEL: u8>(cpu: *mut (), eip: u32) {
    let cpu = unsafe { &mut *(cpu as *mut I386<CPU_MODEL>) };
    cpu.state.set_eip(eip);
    cpu.jit_refresh_prev_ip();
}

/// Returns the bit index of the first/last set bit in `value` under
/// operand `size`, or leaves the destination untouched when `value`
/// is zero. Updates ZF to reflect whether `value` was nonzero.
///
/// Encoding of `flags`:
///   bit 0: `reverse` (0=BSF, 1=BSR)
///
/// Returns `0xFFFF_FFFF` when ZF=0 (source was zero) so the caller can
/// skip the destination write; otherwise returns the bit index to
/// store.
unsafe extern "C" fn helper_bit_scan(
    state: *mut I386State,
    value: u32,
    size: u32,
    flags: u32,
) -> u32 {
    let state = unsafe { &mut *state };
    let reverse = flags & 1 != 0;
    let masked = match size {
        2 => value & 0xFFFF,
        4 => value,
        _ => unreachable!("BSF/BSR byte form is undefined"),
    };
    if masked == 0 {
        state.flags.zero_val = 0;
        0xFFFF_FFFF
    } else {
        state.flags.zero_val = 1;
        if reverse {
            match size {
                2 => 15 - (masked as u16).leading_zeros(),
                4 => 31 - masked.leading_zeros(),
                _ => unreachable!(),
            }
        } else {
            match size {
                2 => (masked as u16).trailing_zeros(),
                4 => masked.trailing_zeros(),
                _ => unreachable!(),
            }
        }
    }
}

unsafe extern "C" fn helper_shift_byte_cl(
    state: *mut I386State,
    value: u32,
    count: u32,
    op: u32,
) -> u32 {
    let state = unsafe { &mut *state };
    let value = value as u8;
    let count = count as u8;
    match op {
        4 => helper_shl_byte(value, count, &mut state.flags) as u32,
        5 => helper_shr_byte(value, count, &mut state.flags) as u32,
        7 => helper_sar_byte(value, count, &mut state.flags) as u32,
        _ => unreachable!("x64 dynarec: invalid shift byte helper op {op}"),
    }
}

fn helper_shl_byte(value: u8, count: u8, flags: &mut I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return value;
    }
    let result = if count < 8 {
        (value as u32) << count
    } else {
        0
    };
    flags.carry_val = if count <= 8 {
        ((value as u32) << (count - 1)) & 0x80
    } else if (count & 7) == 0 {
        ((value as u32) & 1) << 7
    } else {
        0
    };
    flags.overflow_val = (((result >> 7) & 1) ^ flags.cf_val()) * 0x80;
    flags.aux_val = 0;
    flags.set_szpf_byte(result);
    result as u8
}

fn helper_shr_byte(value: u8, count: u8, flags: &mut I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return value;
    }
    flags.overflow_val = if count == 1 { value as u32 & 0x80 } else { 0 };
    let result = if count < 8 {
        flags.carry_val = ((value >> (count - 1)) & 1) as u32;
        value >> count
    } else {
        flags.carry_val = if count == 8 || (count > 8 && (count & 7) == 0) {
            (value >> 7) as u32
        } else {
            0
        };
        0
    };
    flags.aux_val = 0;
    flags.set_szpf_byte(result as u32);
    result
}

fn helper_sar_byte(value: u8, count: u8, flags: &mut I386Flags) -> u8 {
    let count = count & 0x1F;
    if count == 0 {
        return value;
    }
    flags.overflow_val = 0;
    let signed = value as i8;
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

fn seg_reg(seg: u32) -> SegReg32 {
    match seg {
        0 => SegReg32::ES,
        1 => SegReg32::CS,
        2 => SegReg32::SS,
        3 => SegReg32::DS,
        4 => SegReg32::FS,
        5 => SegReg32::GS,
        _ => unreachable!("x64 dynarec: invalid segment register {seg}"),
    }
}

struct LoweringCtx<'a, 'b> {
    emitter: &'a mut Emitter<'b>,
    block: &'a Block,
    fault_exits: &'a mut Vec<usize>,
}

#[derive(Clone, Copy)]
struct BranchList {
    offsets: [usize; 2],
    len: u8,
}

impl BranchList {
    fn new() -> Self {
        Self {
            offsets: [0; 2],
            len: 0,
        }
    }

    fn one(offset: usize) -> Self {
        let mut branches = Self::new();
        branches.push(offset);
        branches
    }

    fn push(&mut self, offset: usize) {
        debug_assert!((self.len as usize) < self.offsets.len());
        self.offsets[self.len as usize] = offset;
        self.len += 1;
    }

    fn as_slice(&self) -> &[usize] {
        &self.offsets[..self.len as usize]
    }
}

fn lower_op(context: &mut LoweringCtx<'_, '_>, op: &IrOp) {
    match op {
        IrOp::Nop => {}
        IrOp::MovImm { dst, imm } => {
            context.emitter.mov_r32_imm32(GpReg::Rax, *imm);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::MovReg { dst, src } => {
            load_guest_reg(context.emitter, GpReg::Rax, src);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::MemMovImm { mem, imm, size } => {
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            context.emitter.mov_r32_imm32(GpReg::R8, *imm);
            call_mem_write(context, *size);
        }
        IrOp::MemRead { dst, mem } => {
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_read(context, dst.size);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::MemWrite { src, mem } => {
            load_guest_reg(context.emitter, GpReg::R8, src);
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_write(context, src.size);
        }
        IrOp::Lea { dst, mem } => {
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            store_guest_reg(context.emitter, dst, GpReg::Rcx);
        }
        IrOp::Alu { dst, src, op } => lower_alu_rr(context, dst, src, *op),
        IrOp::AluImm { dst, imm, op } => lower_alu_imm(context, dst, *imm, *op),
        IrOp::AluRm { dst, mem, op } => lower_alu_mem(context, dst, mem, *op),
        IrOp::MemAluReg { mem, src, size, op } => {
            load_guest_reg(context.emitter, GpReg::R10, src);
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_read(context, *size);
            context
                .emitter
                .mov_r32_r32(GpReg::Rcx, GpReg::R10, Size::Dword);
            emit_alu(
                context.emitter,
                *op,
                *size,
                GpReg::Rax,
                Some(GpReg::Rcx),
                None,
            );
            if alu_writes_result(*op) {
                context
                    .emitter
                    .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                load_seg_arg(context.emitter, mem.seg);
                call_mem_write(context, *size);
            }
        }
        IrOp::MemAluImm { mem, imm, size, op } => {
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_read(context, *size);
            emit_alu(context.emitter, *op, *size, GpReg::Rax, None, Some(*imm));
            if alu_writes_result(*op) {
                context
                    .emitter
                    .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                load_seg_arg(context.emitter, mem.seg);
                call_mem_write(context, *size);
            }
        }
        IrOp::Unary { dst, op } => match op {
            UnaryOp::Not => {
                load_guest_reg(context.emitter, GpReg::Rax, dst);
                emit_not_value(context.emitter, dst.size, GpReg::Rax);
                store_guest_reg(context.emitter, dst, GpReg::Rax);
            }
            UnaryOp::Neg => {
                load_guest_reg(context.emitter, GpReg::Rax, dst);
                emit_call_neg_helper(context, dst.size, GpReg::Rax);
                store_guest_reg(context.emitter, dst, GpReg::Rax);
            }
            _ => {
                load_guest_reg(context.emitter, GpReg::Rax, dst);
                emit_unary(context.emitter, *op, dst.size, GpReg::Rax);
                store_guest_reg(context.emitter, dst, GpReg::Rax);
            }
        },
        IrOp::MemUnary { mem, size, op } => match op {
            UnaryOp::Not => {
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                call_mem_read(context, *size);
                emit_not_value(context.emitter, *size, GpReg::Rax);
                context
                    .emitter
                    .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                load_seg_arg(context.emitter, mem.seg);
                call_mem_write(context, *size);
            }
            UnaryOp::Neg => {
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                call_mem_read(context, *size);
                emit_call_neg_helper(context, *size, GpReg::Rax);
                context
                    .emitter
                    .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                load_seg_arg(context.emitter, mem.seg);
                call_mem_write(context, *size);
            }
            _ => {
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                call_mem_read(context, *size);
                emit_unary(context.emitter, *op, *size, GpReg::Rax);
                context
                    .emitter
                    .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
                emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                load_seg_arg(context.emitter, mem.seg);
                call_mem_write(context, *size);
            }
        },
        IrOp::Shift {
            dst,
            size,
            count,
            op,
        } => lower_shift(context, dst, *size, count, *op),
        IrOp::MovZx { dst, src, src_size } => {
            load_rm_source(context, GpReg::Rax, src, *src_size);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::MovSx { dst, src, src_size } => {
            load_rm_source(context, GpReg::Rax, src, *src_size);
            sign_extend_in_place(context.emitter, GpReg::Rax, *src_size);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::ImulRegRm { dst, src, size } => {
            load_rm_source(context, GpReg::Rcx, src, *size);
            load_guest_reg(context.emitter, GpReg::Rax, dst);
            emit_imul(context.emitter, *size);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::ImulRegRmImm {
            dst,
            src,
            imm,
            size,
        } => {
            load_rm_source(context, GpReg::Rax, src, *size);
            context.emitter.mov_r32_imm32(GpReg::Rcx, *imm as u32);
            emit_imul(context.emitter, *size);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::PushReg { src } => {
            load_guest_reg(context.emitter, GpReg::Rdx, src);
            call_push(context, src.size);
        }
        IrOp::PushStackPtr { size } => {
            let src = RegOperand {
                reg: GuestReg::Gpr(DwordReg::ESP),
                size: *size,
            };
            load_guest_reg(context.emitter, GpReg::Rdx, &src);
            call_push(context, *size);
        }
        IrOp::PushImm { imm, size } => {
            context.emitter.mov_r32_imm32(GpReg::Rdx, *imm);
            call_push(context, *size);
        }
        IrOp::PushMem { mem, size } => {
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_read(context, *size);
            context
                .emitter
                .mov_r32_r32(GpReg::Rdx, GpReg::Rax, Size::Dword);
            call_push(context, *size);
        }
        IrOp::PopReg { dst } => {
            call_pop(context, dst.size);
            store_guest_reg(context.emitter, dst, GpReg::Rax);
        }
        IrOp::PopMem { mem, size } => {
            call_pop(context, *size);
            context
                .emitter
                .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_write(context, *size);
        }
        IrOp::XchgReg { a, b } => {
            load_guest_reg(context.emitter, GpReg::R10, a);
            load_guest_reg(context.emitter, GpReg::R11, b);
            store_guest_reg(context.emitter, a, GpReg::R11);
            store_guest_reg(context.emitter, b, GpReg::R10);
        }
        IrOp::SetCc { dst, cond } => {
            emit_cond_to_reg(context.emitter, *cond, GpReg::Rax);
            match dst {
                RmDest::Reg(reg) => store_guest_reg(context.emitter, reg, GpReg::Rax),
                RmDest::Mem { mem, size } => {
                    context
                        .emitter
                        .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
                    emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
                    call_mem_write(context, *size);
                }
            }
        }
        IrOp::CbwCwd { op } => emit_cbw_cwd(context.emitter, *op),
        IrOp::Flag(op) => emit_flag_op(context.emitter, *op),
        IrOp::PushAll { size } => call_pusha_popa(context, HELP_PUSHA_OFFSET, *size),
        IrOp::PopAll { size } => call_pusha_popa(context, HELP_POPA_OFFSET, *size),
        IrOp::PushFlags { size } => call_pushf_popf(context, HELP_PUSHF_OFFSET, *size),
        IrOp::PopFlags { size } => call_pushf_popf(context, HELP_POPF_OFFSET, *size),
        IrOp::Sahf => call_state_only_helper(context, HELP_SAHF_OFFSET),
        IrOp::Lahf => call_state_only_helper(context, HELP_LAHF_OFFSET),
        IrOp::MulAcc { src, size, signed } => {
            load_rm_source(context, GpReg::Rax, src, *size);
            call_mul_acc_helper(context, GpReg::Rax, *size, *signed);
        }
        IrOp::DivAcc { src, size, signed } => {
            load_rm_source(context, GpReg::Rax, src, *size);
            call_div_acc_helper(context, GpReg::Rax, *size, *signed);
        }
        IrOp::Bswap { dst } => emit_bswap_reg(context.emitter, *dst),
        IrOp::BitScan {
            dst,
            src,
            size,
            reverse,
        } => {
            load_rm_source(context, GpReg::Rax, src, *size);
            emit_call_bit_scan_helper(context, GpReg::Rax, *size, *reverse, dst);
        }
        IrOp::InterpCall {
            instr_start_eip,
            instr_end_eip,
        } => {
            context.emitter.mov_r32_imm32(GpReg::Rdx, *instr_start_eip);
            context.emitter.mov_r32_imm32(GpReg::Rcx, *instr_end_eip);
            call_helper(context.emitter, HELP_INTERP_ONE_OFFSET);
            context.emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
            context
                .fault_exits
                .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
        }
        IrOp::InstrStart { eip } => {
            // SysV: helper takes (cpu=rdi, eip=rsi). Load rdi from the
            // pinned cpu pointer in rbx, and rsi with the EIP. Bypass
            // `call_helper`, which wires rsi to the bus pointer.
            context.emitter.mov_r64_r64(GpReg::Rdi, GpReg::Rbx);
            context.emitter.mov_r32_imm32(GpReg::Rsi, *eip);
            context
                .emitter
                .mov_r64_m64(GpReg::Rax, GpReg::R13, HELP_INSTR_START_OFFSET);
            context.emitter.call_r64(GpReg::Rax);
        }
    }
}

fn lower_exit(context: &mut LoweringCtx<'_, '_>, block: &Block) {
    match block.exit {
        BlockExit::Fallthrough { next_eip } => {
            write_full_eip(context.emitter, next_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::DirectJump { target_eip } => {
            write_full_eip(context.emitter, target_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::ConditionalJump {
            cond,
            taken_eip,
            fallthrough_eip,
        } => {
            let taken = emit_branch_cond(context.emitter, cond);
            write_full_eip(context.emitter, fallthrough_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
            let taken_label = context.emitter.position();
            write_full_eip(context.emitter, taken_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
            context
                .emitter
                .patch_rel32_branches(taken.as_slice(), taken_label);
        }
        BlockExit::CondLoop {
            cond,
            taken_eip,
            fallthrough_eip,
            addr_size,
        } => lower_cond_loop_exit(context, cond, taken_eip, fallthrough_eip, addr_size),
        BlockExit::DirectCall {
            target_eip,
            return_eip,
            size,
        } => {
            context.emitter.mov_r32_imm32(GpReg::Rdx, return_eip);
            call_push(context, size);
            write_full_eip(context.emitter, target_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::IndirectCall {
            target,
            return_eip,
            size,
        } => {
            lower_indirect_target(context, target, size, GpReg::R10);
            context.emitter.mov_m64_r64(GpReg::Rsp, 8, GpReg::R10);
            context.emitter.mov_r32_imm32(GpReg::Rdx, return_eip);
            call_push(context, size);
            context.emitter.mov_r64_m64(GpReg::R10, GpReg::Rsp, 8);
            write_near_eip_reg_size(context.emitter, GpReg::R10, size);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::IndirectJump { target, size } => {
            lower_indirect_target(context, target, size, GpReg::Rax);
            write_near_eip_reg_size(context.emitter, GpReg::Rax, size);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::Return { size } => {
            call_pop(context, size);
            write_near_eip_reg_size(context.emitter, GpReg::Rax, size);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::ReturnImm { size, extra_sp } => {
            call_pop(context, size);
            if block.stack32 {
                context.emitter.add_m32_imm32(
                    GpReg::Rbp,
                    gpr_offset(DwordReg::ESP),
                    extra_sp as i32,
                );
            } else {
                context
                    .emitter
                    .mov_r32_m32(GpReg::Rdx, GpReg::Rbp, gpr_offset(DwordReg::ESP));
                context.emitter.and_r32_imm32(GpReg::Rdx, 0xFFFF_0000);
                context
                    .emitter
                    .mov_r32_m32(GpReg::Rcx, GpReg::Rbp, gpr_offset(DwordReg::ESP));
                context.emitter.and_r32_imm32(GpReg::Rcx, 0x0000_FFFF);
                context.emitter.add_r32_imm32(GpReg::Rcx, extra_sp as i32);
                context.emitter.and_r32_imm32(GpReg::Rcx, 0x0000_FFFF);
                context.emitter.or_r32_r32(GpReg::Rdx, GpReg::Rcx);
                context
                    .emitter
                    .mov_m32_r32(GpReg::Rbp, gpr_offset(DwordReg::ESP), GpReg::Rdx);
            }
            write_near_eip_reg_size(context.emitter, GpReg::Rax, size);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
        }
        BlockExit::Halt { next_eip } => {
            write_full_eip(context.emitter, next_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Halt as u32);
        }
        BlockExit::Fallback { guest_eip } => {
            context.emitter.mov_r64_m64(GpReg::Rax, GpReg::Rsp, 0);
            context
                .emitter
                .mov_m32_imm32(GpReg::Rax, CTX_EXIT_EIP_OFFSET, guest_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Fallback as u32);
        }
    }
}

fn lower_alu_rr(context: &mut LoweringCtx<'_, '_>, dst: &RegOperand, src: &RegOperand, op: AluOp) {
    load_guest_reg(context.emitter, GpReg::Rax, dst);
    load_guest_reg(context.emitter, GpReg::Rcx, src);
    emit_alu(
        context.emitter,
        op,
        dst.size,
        GpReg::Rax,
        Some(GpReg::Rcx),
        None,
    );
    if alu_writes_result(op) {
        store_guest_reg(context.emitter, dst, GpReg::Rax);
    }
}

fn lower_alu_imm(context: &mut LoweringCtx<'_, '_>, dst: &RegOperand, imm: u32, op: AluOp) {
    load_guest_reg(context.emitter, GpReg::Rax, dst);
    emit_alu(context.emitter, op, dst.size, GpReg::Rax, None, Some(imm));
    if alu_writes_result(op) {
        store_guest_reg(context.emitter, dst, GpReg::Rax);
    }
}

fn lower_alu_mem(context: &mut LoweringCtx<'_, '_>, dst: &RegOperand, mem: &MemOperand, op: AluOp) {
    emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
    call_mem_read(context, dst.size);
    context
        .emitter
        .mov_r32_r32(GpReg::Rcx, GpReg::Rax, Size::Dword);
    load_guest_reg(context.emitter, GpReg::Rax, dst);
    emit_alu(
        context.emitter,
        op,
        dst.size,
        GpReg::Rax,
        Some(GpReg::Rcx),
        None,
    );
    if alu_writes_result(op) {
        store_guest_reg(context.emitter, dst, GpReg::Rax);
    }
}

fn lower_shift(
    context: &mut LoweringCtx<'_, '_>,
    dst: &RmDest,
    size: Size,
    count: &ShiftCount,
    op: ShiftOp,
) {
    let use_shift_byte_cl_helper = matches!(size, Size::Byte)
        && matches!(count, ShiftCount::Cl)
        && matches!(op, ShiftOp::Shl | ShiftOp::Shr | ShiftOp::Sar);
    let after_shift = context.emitter.new_label();
    match dst {
        RmDest::Reg(reg) => {
            load_guest_reg(context.emitter, GpReg::Rax, reg);
            load_shift_count(context.emitter, count);
            if use_shift_byte_cl_helper {
                emit_shift_byte_cl_helper(context.emitter, op);
            } else {
                emit_shift(context.emitter, op, size, after_shift);
            }
            store_guest_reg(context.emitter, reg, GpReg::Rax);
        }
        RmDest::Mem { mem, size: msize } => {
            debug_assert_eq!(*msize, size);
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_read(context, size);
            load_shift_count(context.emitter, count);
            if use_shift_byte_cl_helper {
                emit_shift_byte_cl_helper(context.emitter, op);
            } else {
                emit_shift(context.emitter, op, size, after_shift);
            }
            context
                .emitter
                .mov_r32_r32(GpReg::R8, GpReg::Rax, Size::Dword);
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            load_seg_arg(context.emitter, mem.seg);
            call_mem_write(context, size);
        }
    }
    context.emitter.place_label(after_shift);
}

fn emit_shift_byte_cl_helper(emitter: &mut Emitter<'_>, op: ShiftOp) {
    emitter.mov_r64_r64(GpReg::Rdi, GpReg::Rbp);
    emitter.mov_r64_r64(GpReg::Rsi, GpReg::Rax);
    emitter.mov_r32_r32(GpReg::Rdx, GpReg::Rcx, Size::Dword);
    emitter.mov_r32_imm32(GpReg::Rcx, u32::from(shift_ext(op)));
    emitter.mov_r64_m64(GpReg::Rax, GpReg::R13, HELP_SHIFT_BYTE_CL_OFFSET);
    emitter.call_r64(GpReg::Rax);
}

fn load_rm_source(context: &mut LoweringCtx<'_, '_>, dst: GpReg, src: &RmSource, size: Size) {
    match src {
        RmSource::Reg(reg) => load_guest_reg(context.emitter, dst, reg),
        RmSource::Mem { mem, size: msize } => {
            debug_assert_eq!(*msize, size);
            emit_compute_mem_offset(context.emitter, mem, GpReg::Rcx);
            call_mem_read(context, size);
            if dst != GpReg::Rax {
                context.emitter.mov_r32_r32(dst, GpReg::Rax, Size::Dword);
            }
        }
    }
}

fn lower_indirect_target(
    context: &mut LoweringCtx<'_, '_>,
    target: IndirectTarget,
    size: Size,
    dst: GpReg,
) {
    match target {
        IndirectTarget::Reg(reg) => {
            let operand = RegOperand {
                reg: GuestReg::Gpr(reg),
                size,
            };
            load_guest_reg(context.emitter, dst, &operand);
        }
        IndirectTarget::Mem(mem) => {
            emit_compute_mem_offset(context.emitter, &mem, GpReg::Rcx);
            call_mem_read(context, size);
            if dst != GpReg::Rax {
                context.emitter.mov_r32_r32(dst, GpReg::Rax, Size::Dword);
            }
        }
    }
}

fn load_shift_count(emitter: &mut Emitter<'_>, count: &ShiftCount) {
    match count {
        ShiftCount::Imm(value) => emitter.mov_r32_imm32(GpReg::Rcx, u32::from(*value)),
        ShiftCount::Cl => {
            let cl = RegOperand {
                reg: GuestReg::Gpr(DwordReg::ECX),
                size: Size::Byte,
            };
            load_guest_reg(emitter, GpReg::Rcx, &cl);
        }
    }
}

fn emit_shift(emitter: &mut Emitter<'_>, op: ShiftOp, size: Size, done_label: Label) {
    emitter.and_r32_imm32(GpReg::Rcx, 0x1F);
    emitter.test_r32_r32(GpReg::Rcx, GpReg::Rcx, Size::Dword);
    let zero = emitter.jcc_rel32_placeholder(ConditionCode::Z);
    if matches!(op, ShiftOp::Rcl | ShiftOp::Rcr) {
        load_guest_cf_to_host(emitter);
    }
    emitter.group2_cl(op, size, GpReg::Rax);
    match op {
        ShiftOp::Shl | ShiftOp::Shr | ShiftOp::Sar => {
            save_result_and_flags(emitter, size, GpReg::Rax, true, true, true);
        }
        ShiftOp::Rol | ShiftOp::Ror | ShiftOp::Rcl | ShiftOp::Rcr => {
            save_rotate_flags(emitter, GpReg::Rax);
        }
    }
    let done_jump = emitter.jmp_rel32_placeholder();
    emitter.place_label(done_label);
    emitter.patch_rel32_branches(&[zero, done_jump], emitter.position());
}

fn emit_alu(
    emitter: &mut Emitter<'_>,
    op: AluOp,
    size: Size,
    dst: GpReg,
    rhs_reg: Option<GpReg>,
    rhs_imm: Option<u32>,
) {
    match op {
        AluOp::Add => {
            apply_binop(emitter, BinOp::Add, size, dst, rhs_reg, rhs_imm);
            save_result_and_flags(emitter, size, dst, true, true, true);
        }
        AluOp::Sub | AluOp::Cmp => {
            apply_binop(emitter, BinOp::Sub, size, dst, rhs_reg, rhs_imm);
            save_result_and_flags(emitter, size, dst, true, true, true);
        }
        AluOp::And => {
            apply_binop(emitter, BinOp::And, size, dst, rhs_reg, rhs_imm);
            save_logic_flags(emitter, size, dst);
        }
        AluOp::Or => {
            apply_binop(emitter, BinOp::Or, size, dst, rhs_reg, rhs_imm);
            save_logic_flags(emitter, size, dst);
        }
        AluOp::Xor => {
            apply_binop(emitter, BinOp::Xor, size, dst, rhs_reg, rhs_imm);
            save_logic_flags(emitter, size, dst);
        }
        AluOp::Test => {
            emitter.mov_r32_r32(GpReg::R10, dst, Size::Dword);
            apply_binop(emitter, BinOp::And, size, GpReg::R10, rhs_reg, rhs_imm);
            save_logic_flags(emitter, size, GpReg::R10);
        }
        AluOp::Adc => {
            load_guest_cf_to_host(emitter);
            apply_binop(emitter, BinOp::Adc, size, dst, rhs_reg, rhs_imm);
            save_result_and_flags(emitter, size, dst, true, true, true);
        }
        AluOp::Sbb => {
            load_guest_cf_to_host(emitter);
            apply_binop(emitter, BinOp::Sbb, size, dst, rhs_reg, rhs_imm);
            save_result_and_flags(emitter, size, dst, true, true, true);
        }
    }
}

fn emit_unary(emitter: &mut Emitter<'_>, op: UnaryOp, size: Size, dst: GpReg) {
    match op {
        UnaryOp::Inc => {
            emitter.unary_inc(size, dst);
            save_result_and_flags(emitter, size, dst, false, true, true);
        }
        UnaryOp::Dec => {
            emitter.unary_dec(size, dst);
            save_result_and_flags(emitter, size, dst, false, true, true);
        }
        UnaryOp::Not | UnaryOp::Neg => {
            // NOT and NEG with full lazy-flag semantics are handled by
            // call_unary_helper; we should never reach here for them.
            let _ = (emitter, size, dst);
            unreachable!("NOT/NEG must use call_unary_helper, not emit_unary");
        }
    }
}

fn emit_imul(emitter: &mut Emitter<'_>, size: Size) {
    emitter.imul_reg_reg(size, GpReg::Rax, GpReg::Rcx);
    emitter.mov_r32_r32(GpReg::R10, GpReg::Rax, Size::Dword);
    emitter.setcc_r8(ConditionCode::O, GpReg::R11);
    emitter.movzx_r32_r8(GpReg::R11, GpReg::R11);
    store_guest_reg_from_reg(emitter, FLAG_CARRY_OFFSET, GpReg::R11);
    store_guest_reg_from_reg(emitter, FLAG_OVERFLOW_OFFSET, GpReg::R11);
}

fn emit_cbw_cwd(emitter: &mut Emitter<'_>, op: CbwOp) {
    match op {
        CbwOp::Cbw => {
            let eax_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EAX),
                size: Size::Dword,
            };
            load_guest_reg(emitter, GpReg::Rdx, &eax_reg);
            emitter.mov_r32_r32(GpReg::Rax, GpReg::Rdx, Size::Dword);
            emitter.shl_r32_imm8(GpReg::Rax, 24);
            emitter.sar_r32_imm8(GpReg::Rax, 24);
            emitter.and_r32_imm32(GpReg::Rdx, 0xFFFF_0000);
            emitter.and_r32_imm32(GpReg::Rax, 0x0000_FFFF);
            emitter.or_r32_r32(GpReg::Rdx, GpReg::Rax);
            emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(DwordReg::EAX), GpReg::Rdx);
        }
        CbwOp::Cwde => {
            let eax_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EAX),
                size: Size::Word,
            };
            load_guest_reg(emitter, GpReg::Rax, &eax_reg);
            sign_extend_in_place(emitter, GpReg::Rax, Size::Word);
            emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(DwordReg::EAX), GpReg::Rax);
        }
        CbwOp::Cwd => {
            let eax_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EAX),
                size: Size::Word,
            };
            load_guest_reg(emitter, GpReg::Rax, &eax_reg);
            sign_extend_in_place(emitter, GpReg::Rax, Size::Word);
            emitter.shr_r32_imm8(GpReg::Rax, 31);
            emitter.neg_r32(GpReg::Rax);
            let edx_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EDX),
                size: Size::Word,
            };
            store_guest_reg(emitter, &edx_reg, GpReg::Rax);
        }
        CbwOp::Cdq => {
            let eax_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::EAX),
                size: Size::Dword,
            };
            load_guest_reg(emitter, GpReg::Rax, &eax_reg);
            emitter.sar_r32_imm8(GpReg::Rax, 31);
            emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(DwordReg::EDX), GpReg::Rax);
        }
    }
}

fn emit_flag_op(emitter: &mut Emitter<'_>, op: FlagOp) {
    match op {
        FlagOp::Clc => emitter.mov_m32_imm32(GpReg::Rbp, FLAG_CARRY_OFFSET, 0),
        FlagOp::Stc => emitter.mov_m32_imm32(GpReg::Rbp, FLAG_CARRY_OFFSET, 1),
        FlagOp::Cmc => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_CARRY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            emitter.setcc_r8(ConditionCode::Z, GpReg::Rax);
            emitter.movzx_r32_r8(GpReg::Rax, GpReg::Rax);
            emitter.mov_m32_r32(GpReg::Rbp, FLAG_CARRY_OFFSET, GpReg::Rax);
        }
        FlagOp::Cld => emitter.mov_m8_imm8(GpReg::Rbp, FLAG_DF_OFFSET, 0),
        FlagOp::Std => emitter.mov_m8_imm8(GpReg::Rbp, FLAG_DF_OFFSET, 1),
    }
}

fn emit_cond_to_reg(emitter: &mut Emitter<'_>, cond: IrCond, dst: GpReg) {
    let true_jumps = emit_branch_cond(emitter, cond);
    emitter.mov_r32_imm32(dst, 0);
    let done = emitter.jmp_rel32_placeholder();
    let true_label = emitter.position();
    emitter.mov_r32_imm32(dst, 1);
    let done_label = emitter.position();
    emitter.patch_rel32_branches(true_jumps.as_slice(), true_label);
    emitter.patch_rel32_branches(&[done], done_label);
}

fn emit_branch_cond(emitter: &mut Emitter<'_>, cond: IrCond) -> BranchList {
    match cond {
        IrCond::O => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_OVERFLOW_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Nz))
        }
        IrCond::No => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_OVERFLOW_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Z))
        }
        IrCond::B => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_CARRY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Nz))
        }
        IrCond::Nb => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_CARRY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Z))
        }
        IrCond::Z => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_ZERO_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Z))
        }
        IrCond::Nz => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_ZERO_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Nz))
        }
        IrCond::Be => {
            let mut jumps = BranchList::new();
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_CARRY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Nz));
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_ZERO_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Z));
            jumps
        }
        IrCond::A => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_CARRY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            let fail_cf = emitter.jcc_rel32_placeholder(ConditionCode::Nz);
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_ZERO_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            let taken = emitter.jcc_rel32_placeholder(ConditionCode::Nz);
            let fail_label = emitter.position();
            emitter.patch_rel32_branches(&[fail_cf], fail_label);
            BranchList::one(taken)
        }
        IrCond::S => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_SIGN_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::S))
        }
        IrCond::Ns => {
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_SIGN_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Ns))
        }
        IrCond::P => {
            emitter.movzx_r32_m8(GpReg::Rax, GpReg::Rbp, FLAG_PARITY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Byte);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::P))
        }
        IrCond::Np => {
            emitter.movzx_r32_m8(GpReg::Rax, GpReg::Rbp, FLAG_PARITY_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Byte);
            BranchList::one(emitter.jcc_rel32_placeholder(ConditionCode::Np))
        }
        IrCond::L | IrCond::Ge | IrCond::Le | IrCond::G => emit_complex_compare_cond(emitter, cond),
    }
}

fn emit_complex_compare_cond(emitter: &mut Emitter<'_>, cond: IrCond) -> BranchList {
    emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_SIGN_OFFSET);
    emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
    emitter.setcc_r8(ConditionCode::S, GpReg::Rax);
    emitter.movzx_r32_r8(GpReg::Rax, GpReg::Rax);
    emitter.mov_r32_m32(GpReg::Rcx, GpReg::Rbp, FLAG_OVERFLOW_OFFSET);
    emitter.test_r32_r32(GpReg::Rcx, GpReg::Rcx, Size::Dword);
    emitter.setcc_r8(ConditionCode::Nz, GpReg::Rcx);
    emitter.movzx_r32_r8(GpReg::Rcx, GpReg::Rcx);
    emitter.cmp_r32_r32(GpReg::Rax, GpReg::Rcx, Size::Dword);
    let mut jumps = BranchList::new();
    match cond {
        IrCond::L => jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Nz)),
        IrCond::Ge => jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Z)),
        IrCond::Le => {
            jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Nz));
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_ZERO_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Z));
        }
        IrCond::G => {
            let fail_compare = emitter.jcc_rel32_placeholder(ConditionCode::Nz);
            emitter.mov_r32_m32(GpReg::Rax, GpReg::Rbp, FLAG_ZERO_OFFSET);
            emitter.test_r32_r32(GpReg::Rax, GpReg::Rax, Size::Dword);
            jumps.push(emitter.jcc_rel32_placeholder(ConditionCode::Nz));
            let fail_label = emitter.position();
            emitter.patch_rel32_branches(&[fail_compare], fail_label);
        }
        _ => unreachable!(),
    }
    jumps
}

fn lower_cond_loop_exit(
    context: &mut LoweringCtx<'_, '_>,
    cond: LoopCond,
    taken_eip: u32,
    fallthrough_eip: u32,
    addr_size: AddrSize,
) {
    match cond {
        LoopCond::Jcxz => {
            let count_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::ECX),
                size: match addr_size {
                    AddrSize::Addr16 => Size::Word,
                    AddrSize::Addr32 => Size::Dword,
                },
            };
            load_guest_reg(context.emitter, GpReg::Rax, &count_reg);
            context
                .emitter
                .test_r32_r32(GpReg::Rax, GpReg::Rax, count_reg.size);
            let taken = context.emitter.jcc_rel32_placeholder(ConditionCode::Z);
            write_full_eip(context.emitter, fallthrough_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
            let taken_label = context.emitter.position();
            write_full_eip(context.emitter, taken_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
            context.emitter.patch_rel32_branches(&[taken], taken_label);
        }
        LoopCond::Loop | LoopCond::Loopne | LoopCond::Loope => {
            let count_reg = RegOperand {
                reg: GuestReg::Gpr(DwordReg::ECX),
                size: match addr_size {
                    AddrSize::Addr16 => Size::Word,
                    AddrSize::Addr32 => Size::Dword,
                },
            };
            load_guest_reg(context.emitter, GpReg::Rax, &count_reg);
            context.emitter.sub_r32_imm32(GpReg::Rax, 1);
            store_guest_reg(context.emitter, &count_reg, GpReg::Rax);
            context
                .emitter
                .test_r32_r32(GpReg::Rax, GpReg::Rax, count_reg.size);
            let count_zero = context.emitter.jcc_rel32_placeholder(ConditionCode::Z);
            let taken_jumps = match cond {
                LoopCond::Loop => BranchList::one(context.emitter.jmp_rel32_placeholder()),
                LoopCond::Loopne => emit_branch_cond(context.emitter, IrCond::Nz),
                LoopCond::Loope => emit_branch_cond(context.emitter, IrCond::Z),
                LoopCond::Jcxz => unreachable!(),
            };
            let fallthrough_label = context.emitter.position();
            write_full_eip(context.emitter, fallthrough_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
            let taken_label = context.emitter.position();
            write_full_eip(context.emitter, taken_eip);
            emit_return_with_outcome(context.emitter, NativeOutcome::Continue as u32);
            context
                .emitter
                .patch_rel32_branches(taken_jumps.as_slice(), taken_label);
            context
                .emitter
                .patch_rel32_branches(&[count_zero], fallthrough_label);
        }
    }
}

fn alu_writes_result(op: AluOp) -> bool {
    !matches!(op, AluOp::Cmp | AluOp::Test)
}

fn apply_binop(
    emitter: &mut Emitter<'_>,
    op: BinOp,
    size: Size,
    dst: GpReg,
    rhs_reg: Option<GpReg>,
    rhs_imm: Option<u32>,
) {
    if let Some(reg) = rhs_reg {
        emitter.binop_reg_reg(op, size, dst, reg);
    } else if let Some(imm) = rhs_imm {
        emitter.binop_reg_imm(op, size, dst, imm as i32);
    } else {
        unreachable!();
    }
}

fn sign_extend_in_place(emitter: &mut Emitter<'_>, reg: GpReg, size: Size) {
    match size {
        Size::Byte => {
            emitter.shl_r32_imm8(reg, 24);
            emitter.sar_r32_imm8(reg, 24);
        }
        Size::Word => {
            emitter.shl_r32_imm8(reg, 16);
            emitter.sar_r32_imm8(reg, 16);
        }
        Size::Dword => {}
    }
}

fn save_logic_flags(emitter: &mut Emitter<'_>, size: Size, result_reg: GpReg) {
    emitter.mov_r32_r32(GpReg::R10, result_reg, Size::Dword);
    write_sign_zero_parity(emitter, size, GpReg::R10);
    emitter.mov_m32_imm32(GpReg::Rbp, FLAG_CARRY_OFFSET, 0);
    emitter.mov_m32_imm32(GpReg::Rbp, FLAG_OVERFLOW_OFFSET, 0);
    emitter.mov_m32_imm32(GpReg::Rbp, FLAG_AUX_OFFSET, 0);
    if result_reg != GpReg::R10 {
        emitter.mov_r32_r32(result_reg, GpReg::R10, Size::Dword);
    }
}

fn save_result_and_flags(
    emitter: &mut Emitter<'_>,
    size: Size,
    result_reg: GpReg,
    write_carry: bool,
    write_aux: bool,
    write_overflow: bool,
) {
    emitter.mov_r32_r32(GpReg::R10, result_reg, Size::Dword);
    emitter.setcc_r8(ConditionCode::O, GpReg::R11);
    emitter.movzx_r32_r8(GpReg::R11, GpReg::R11);
    emitter.lahf();
    emitter.movzx_r32_r8_high(GpReg::Rdx);
    write_sign_zero_parity(emitter, size, GpReg::R10);
    if write_carry {
        emitter.mov_r32_r32(GpReg::Rax, GpReg::Rdx, Size::Dword);
        emitter.and_r32_imm32(GpReg::Rax, 1);
        emitter.mov_m32_r32(GpReg::Rbp, FLAG_CARRY_OFFSET, GpReg::Rax);
    }
    if write_aux {
        emitter.mov_r32_r32(GpReg::Rax, GpReg::Rdx, Size::Dword);
        emitter.shr_r32_imm8(GpReg::Rax, 4);
        emitter.and_r32_imm32(GpReg::Rax, 1);
        emitter.mov_m32_r32(GpReg::Rbp, FLAG_AUX_OFFSET, GpReg::Rax);
    }
    if write_overflow {
        emitter.mov_m32_r32(GpReg::Rbp, FLAG_OVERFLOW_OFFSET, GpReg::R11);
    }
    if result_reg != GpReg::R10 {
        emitter.mov_r32_r32(result_reg, GpReg::R10, Size::Dword);
    }
}

fn save_rotate_flags(emitter: &mut Emitter<'_>, result_reg: GpReg) {
    emitter.mov_r32_r32(GpReg::R10, result_reg, Size::Dword);
    emitter.setcc_r8(ConditionCode::O, GpReg::R11);
    emitter.movzx_r32_r8(GpReg::R11, GpReg::R11);
    emitter.setcc_r8(ConditionCode::B, GpReg::Rax);
    emitter.movzx_r32_r8(GpReg::Rax, GpReg::Rax);
    emitter.mov_m32_r32(GpReg::Rbp, FLAG_CARRY_OFFSET, GpReg::Rax);
    emitter.mov_m32_r32(GpReg::Rbp, FLAG_OVERFLOW_OFFSET, GpReg::R11);
    if result_reg != GpReg::R10 {
        emitter.mov_r32_r32(result_reg, GpReg::R10, Size::Dword);
    }
}

fn write_sign_zero_parity(emitter: &mut Emitter<'_>, size: Size, result_reg: GpReg) {
    emitter.mov_r32_r32(GpReg::Rax, result_reg, Size::Dword);
    sign_extend_in_place(emitter, GpReg::Rax, size);
    emitter.mov_m32_r32(GpReg::Rbp, FLAG_SIGN_OFFSET, GpReg::Rax);

    emitter.mov_r32_r32(GpReg::Rax, result_reg, Size::Dword);
    match size {
        Size::Byte => emitter.and_r32_imm32(GpReg::Rax, 0xFF),
        Size::Word => emitter.and_r32_imm32(GpReg::Rax, 0xFFFF),
        Size::Dword => {}
    }
    emitter.mov_m32_r32(GpReg::Rbp, FLAG_ZERO_OFFSET, GpReg::Rax);
    emitter.mov_m32_r32(GpReg::Rbp, FLAG_PARITY_OFFSET, GpReg::Rax);
}

fn load_guest_cf_to_host(emitter: &mut Emitter<'_>) {
    emitter.mov_r32_m32(GpReg::Rdx, GpReg::Rbp, FLAG_CARRY_OFFSET);
    emitter.test_r32_r32(GpReg::Rdx, GpReg::Rdx, Size::Dword);
    emitter.setcc_r8(ConditionCode::Nz, GpReg::Rdx);
    emitter.movzx_r32_r8(GpReg::Rdx, GpReg::Rdx);
    emitter.bt_r32_imm8(GpReg::Rdx, 0);
}

fn call_mem_read(context: &mut LoweringCtx<'_, '_>, size: Size) {
    emit_mem_access_with_tlb_probe(context, size, AccessKind::Read);
}

fn call_mem_write(context: &mut LoweringCtx<'_, '_>, size: Size) {
    emit_mem_access_with_tlb_probe(context, size, AccessKind::Write);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    Read,
    Write,
}

/// Emits the inline TLB probe and dispatches to the appropriate helper.
///
/// On entry: rcx = offset, rdx = seg_const (as u32), r8 = value (for
/// writes). On exit (reads): rax = loaded value, or fault-exit branched
/// to on fault.
///
/// The probe:
///   1. linear = seg_bases[rdx] + rcx
///   2. if paging off (CR0.PG|PE != both set): phys = linear, fast path.
///   3. page = linear >> 12, slot = page & 0x3F
///   4. check tlb.valid[slot]; if zero, slow path
///   5. check tlb.tag[slot] == page; if ne, slow path
///   6. for writes: check tlb.writable[slot] and tlb.dirty[slot]; if
///      either zero, slow path
///   7. phys = tlb.phys[slot] | (linear & 0xFFF); fast path
///
/// Fast path calls the `*_phys` helper with (cpu, bus, phys, [value]).
/// Slow path calls the existing `*_seg` helper with
/// (cpu, bus, seg, offset, [value]). Both return i64 where a negative
/// result signals a fault.
fn emit_mem_access_with_tlb_probe(context: &mut LoweringCtx<'_, '_>, size: Size, kind: AccessKind) {
    let emitter = &mut *context.emitter;

    match kind {
        AccessKind::Read => {
            emitter.push_r64(GpReg::Rcx);
            emitter.push_r64(GpReg::Rdx);
        }
        AccessKind::Write => {
            emitter.push_r64(GpReg::Rcx);
            emitter.push_r64(GpReg::Rdx);
            emitter.push_r64(GpReg::R8);
            emitter.push_r64(GpReg::R15);
        }
    }
    emitter.mov_r32_imm32(GpReg::R8, size_to_u32(size));
    emitter.mov_r32_imm32(
        GpReg::R9,
        if matches!(kind, AccessKind::Write) {
            1
        } else {
            0
        },
    );
    call_helper(emitter, HELP_CHECK_SEGMENT_ACCESS_OFFSET);
    match kind {
        AccessKind::Read => {
            emitter.pop_r64(GpReg::Rdx);
            emitter.pop_r64(GpReg::Rcx);
        }
        AccessKind::Write => {
            emitter.pop_r64(GpReg::R15);
            emitter.pop_r64(GpReg::R8);
            emitter.pop_r64(GpReg::Rdx);
            emitter.pop_r64(GpReg::Rcx);
        }
    }
    emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(emitter.jcc_rel32_placeholder(ConditionCode::S));

    // The probe must not clobber registers that IR lowering reserves
    // across the call: callers use r10 (MemAluReg keeps src in r10)
    // and r11 is caller-saved but is reused by some paths. Use r15 for
    // linear/phys (design-doc scratch) and r9 for page/cr0/tlb_phys
    // scratch. rax is used as the slot index for SIB addressing.

    // r15d = seg_bases[rdx] + rcx (linear)
    emitter.mov_r32_m32_sib(GpReg::R15, GpReg::Rbp, GpReg::Rdx, 2, SEG_BASES_OFFSET);
    emitter.add_r32_r32(GpReg::R15, GpReg::Rcx, Size::Dword);

    let mut slow_branches: Vec<usize> = Vec::new();
    match size {
        Size::Byte => {}
        Size::Word => {
            emitter.mov_r32_r32(GpReg::R9, GpReg::R15, Size::Dword);
            emitter.and_r32_imm32(GpReg::R9, 0xFFF);
            emitter.cmp_r32_imm32(GpReg::R9, 0xFFE);
            slow_branches.push(emitter.jcc_rel32_placeholder(ConditionCode::A));
        }
        Size::Dword => {
            emitter.mov_r32_r32(GpReg::R9, GpReg::R15, Size::Dword);
            emitter.and_r32_imm32(GpReg::R9, 0xFFF);
            emitter.cmp_r32_imm32(GpReg::R9, 0xFFC);
            slow_branches.push(emitter.jcc_rel32_placeholder(ConditionCode::A));
        }
    }

    // Check paging enabled: cr0 & 0x80000001 == 0x80000001
    emitter.mov_r32_m32(GpReg::R9, GpReg::Rbp, CR0_OFFSET);
    emitter.and_r32_imm32(GpReg::R9, 0x8000_0001u32);
    emitter.cmp_r32_imm32(GpReg::R9, 0x8000_0001u32 as i32);
    let paging_on = emitter.jcc_rel32_placeholder(ConditionCode::Z);

    // Paging off fast path: phys = linear (already in r15)
    let paging_off_ready = emitter.jmp_rel32_placeholder();

    // Paging on path: TLB probe
    let tlb_probe_start = emitter.position();
    emitter.patch_rel32_branches(&[paging_on], tlb_probe_start);

    // r9d = page = linear >> 12
    emitter.mov_r32_r32(GpReg::R9, GpReg::R15, Size::Dword);
    emitter.shr_r32_imm8(GpReg::R9, 12);
    // rax = slot = page & 0x3F (we keep this in rax for the SIB index)
    emitter.mov_r32_r32(GpReg::Rax, GpReg::R9, Size::Dword);
    emitter.and_r32_imm32(GpReg::Rax, 0x3F);

    // Check tlb.valid[slot] != 0
    emitter.cmp_m8_imm8_sib(GpReg::Rbp, GpReg::Rax, TLB_VALID_OFFSET, 0);
    slow_branches.push(emitter.jcc_rel32_placeholder(ConditionCode::Z));

    // Check tlb.tag[slot] == page
    emitter.cmp_m32_r32_sib(GpReg::Rbp, GpReg::Rax, 2, TLB_TAG_OFFSET, GpReg::R9);
    slow_branches.push(emitter.jcc_rel32_placeholder(ConditionCode::Nz));

    if matches!(kind, AccessKind::Write) {
        // Writes require writable AND dirty TLB bits.
        emitter.cmp_m8_imm8_sib(GpReg::Rbp, GpReg::Rax, TLB_WRITABLE_OFFSET, 0);
        slow_branches.push(emitter.jcc_rel32_placeholder(ConditionCode::Z));
        emitter.cmp_m8_imm8_sib(GpReg::Rbp, GpReg::Rax, TLB_DIRTY_OFFSET, 0);
        slow_branches.push(emitter.jcc_rel32_placeholder(ConditionCode::Z));
    }

    // phys = tlb.phys[slot] | (linear & 0xFFF)
    emitter.mov_r32_m32_sib(GpReg::R9, GpReg::Rbp, GpReg::Rax, 2, TLB_PHYS_OFFSET);
    emitter.and_r32_imm32(GpReg::R15, 0xFFF);
    emitter.or_r32_r32(GpReg::R15, GpReg::R9);

    // Fast-path (paging off OR TLB hit): r15 = phys, dispatch *_phys.
    let fast_path_start = emitter.position();
    emitter.patch_rel32_branches(&[paging_off_ready], fast_path_start);

    // Set up call: rdi = cpu, rsi = bus, rdx = phys (r15), [rcx = value (r8) for writes]
    emitter.mov_r64_r64(GpReg::Rdi, GpReg::Rbx);
    emitter.mov_r64_r64(GpReg::Rsi, GpReg::R12);
    emitter.mov_r32_r32(GpReg::Rdx, GpReg::R15, Size::Dword);
    if matches!(kind, AccessKind::Write) {
        emitter.mov_r32_r32(GpReg::Rcx, GpReg::R8, Size::Dword);
    }
    let phys_helper_offset = match (kind, size) {
        (AccessKind::Read, Size::Byte) => HELP_READ_BYTE_PHYS_OFFSET,
        (AccessKind::Read, Size::Word) => HELP_READ_WORD_PHYS_OFFSET,
        (AccessKind::Read, Size::Dword) => HELP_READ_DWORD_PHYS_OFFSET,
        (AccessKind::Write, Size::Byte) => HELP_WRITE_BYTE_PHYS_OFFSET,
        (AccessKind::Write, Size::Word) => HELP_WRITE_WORD_PHYS_OFFSET,
        (AccessKind::Write, Size::Dword) => HELP_WRITE_DWORD_PHYS_OFFSET,
    };
    emitter.mov_r64_m64(GpReg::Rax, GpReg::R13, phys_helper_offset);
    emitter.call_r64(GpReg::Rax);
    let after_fast = emitter.jmp_rel32_placeholder();

    // Slow path label: fall through to the existing *_seg helper with
    // rdx = seg (preserved), rcx = offset (preserved), r8 = value for
    // writes (preserved). call_helper re-loads rdi and rsi.
    let slow_path_label = emitter.position();
    emitter.patch_rel32_branches(&slow_branches, slow_path_label);
    let seg_helper_offset = match kind {
        AccessKind::Read => helper_read_offset(size),
        AccessKind::Write => helper_write_offset(size),
    };
    call_helper(emitter, seg_helper_offset);

    // Converge: both paths end here. Test for fault.
    let done_label = emitter.position();
    emitter.patch_rel32_branches(&[after_fast], done_label);
    emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
}

fn call_push(context: &mut LoweringCtx<'_, '_>, size: Size) {
    context
        .emitter
        .mov_r32_imm32(GpReg::Rcx, u32::from(context.block.stack32 as u8));
    call_helper(context.emitter, helper_push_offset(size));
    context.emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
}

fn call_pop(context: &mut LoweringCtx<'_, '_>, size: Size) {
    context
        .emitter
        .mov_r32_imm32(GpReg::Rdx, u32::from(context.block.stack32 as u8));
    call_helper(context.emitter, helper_pop_offset(size));
    context.emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
}

fn call_helper(emitter: &mut Emitter<'_>, helper_offset: i32) {
    emitter.mov_r64_r64(GpReg::Rdi, GpReg::Rbx);
    emitter.mov_r64_r64(GpReg::Rsi, GpReg::R12);
    emitter.mov_r64_m64(GpReg::Rax, GpReg::R13, helper_offset);
    emitter.call_r64(GpReg::Rax);
}

fn call_pusha_popa(context: &mut LoweringCtx<'_, '_>, helper_offset: i32, size: Size) {
    let size_bytes: u32 = match size {
        Size::Word => 2,
        Size::Dword => 4,
        Size::Byte => unreachable!("PUSHA/POPA have no byte form"),
    };
    context.emitter.mov_r32_imm32(GpReg::Rdx, size_bytes);
    call_helper(context.emitter, helper_offset);
    context.emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
}

fn call_pushf_popf(context: &mut LoweringCtx<'_, '_>, helper_offset: i32, size: Size) {
    let size_bytes: u32 = match size {
        Size::Word => 2,
        Size::Dword => 4,
        Size::Byte => unreachable!("PUSHF/POPF have no byte form"),
    };
    context.emitter.mov_r32_imm32(GpReg::Rdx, size_bytes);
    call_helper(context.emitter, helper_offset);
    context.emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
}

fn call_state_only_helper(context: &mut LoweringCtx<'_, '_>, helper_offset: i32) {
    let emitter = &mut *context.emitter;
    emitter.mov_r64_r64(GpReg::Rdi, GpReg::Rbp);
    emitter.mov_r64_m64(GpReg::Rax, GpReg::R13, helper_offset);
    emitter.call_r64(GpReg::Rax);
}

fn emit_not_value(emitter: &mut Emitter<'_>, size: Size, dst: GpReg) {
    emitter.not_r32(dst);
    match size {
        Size::Byte => emitter.and_r32_imm32(dst, 0xFF),
        Size::Word => emitter.and_r32_imm32(dst, 0xFFFF),
        Size::Dword => {}
    }
}

fn size_to_u32(size: Size) -> u32 {
    match size {
        Size::Byte => 1,
        Size::Word => 2,
        Size::Dword => 4,
    }
}

fn emit_call_neg_helper(context: &mut LoweringCtx<'_, '_>, size: Size, value_reg: GpReg) {
    // Helper signature: (cpu, bus, value, size) -> u32
    let emitter = &mut *context.emitter;
    if value_reg != GpReg::Rdx {
        emitter.mov_r32_r32(GpReg::Rdx, value_reg, Size::Dword);
    }
    emitter.mov_r32_imm32(GpReg::Rcx, size_to_u32(size));
    call_helper(emitter, HELP_NEG_OFFSET);
    // Result in rax; copy into value_reg if caller expects it there.
    if value_reg != GpReg::Rax {
        context
            .emitter
            .mov_r32_r32(value_reg, GpReg::Rax, Size::Dword);
    }
}

fn call_mul_acc_helper(
    context: &mut LoweringCtx<'_, '_>,
    value_reg: GpReg,
    size: Size,
    signed: bool,
) {
    // Helper signature: (cpu, bus, value, size, signed)
    let emitter = &mut *context.emitter;
    if value_reg != GpReg::Rdx {
        emitter.mov_r32_r32(GpReg::Rdx, value_reg, Size::Dword);
    }
    emitter.mov_r32_imm32(GpReg::Rcx, size_to_u32(size));
    emitter.mov_r32_imm32(GpReg::R8, u32::from(signed));
    call_helper(emitter, HELP_MUL_ACC_OFFSET);
}

fn emit_bswap_reg(emitter: &mut Emitter<'_>, dst: DwordReg) {
    // Load the current guest register into rax, byte-swap via host
    // BSWAP, and store back. BSWAP has no flag side effects.
    let reg_operand = RegOperand {
        reg: GuestReg::Gpr(dst),
        size: Size::Dword,
    };
    load_guest_reg(emitter, GpReg::Rax, &reg_operand);
    emitter.bswap_r32(GpReg::Rax);
    store_guest_reg(emitter, &reg_operand, GpReg::Rax);
}

fn emit_call_bit_scan_helper(
    context: &mut LoweringCtx<'_, '_>,
    value_reg: GpReg,
    size: Size,
    reverse: bool,
    dst: &RegOperand,
) {
    // Helper signature: (state, value, size, flags) -> u32
    // Flags.bit0 = reverse. Return value = 0xFFFF_FFFF on zero source
    // (ZF cleared, skip destination write); otherwise the bit index.
    let emitter = &mut *context.emitter;
    if value_reg != GpReg::Rsi {
        emitter.mov_r32_r32(GpReg::Rsi, value_reg, Size::Dword);
    }
    emitter.mov_r32_imm32(GpReg::Rdx, size_to_u32(size));
    emitter.mov_r32_imm32(GpReg::Rcx, u32::from(reverse));
    // Call pattern differs from the CPU helpers: this helper only
    // wants the state pointer in rdi.
    emitter.mov_r64_r64(GpReg::Rdi, GpReg::Rbp);
    emitter.mov_r64_m64(GpReg::Rax, GpReg::R13, HELP_BIT_SCAN_OFFSET);
    emitter.call_r64(GpReg::Rax);
    // If the helper returned 0xFFFF_FFFF, the source was zero and we
    // must NOT write the destination (intel leaves it undefined; the
    // interpreter preserves it).
    emitter.cmp_r32_imm32(GpReg::Rax, -1i32);
    let zero_source = emitter.jcc_rel32_placeholder(ConditionCode::Z);
    store_guest_reg(emitter, dst, GpReg::Rax);
    let done_label = emitter.position();
    emitter.patch_rel32_branches(&[zero_source], done_label);
}

fn call_div_acc_helper(
    context: &mut LoweringCtx<'_, '_>,
    value_reg: GpReg,
    size: Size,
    signed: bool,
) {
    // Helper signature: (cpu, bus, value, size, signed) -> i64 (-1 on fault)
    let emitter = &mut *context.emitter;
    if value_reg != GpReg::Rdx {
        emitter.mov_r32_r32(GpReg::Rdx, value_reg, Size::Dword);
    }
    emitter.mov_r32_imm32(GpReg::Rcx, size_to_u32(size));
    emitter.mov_r32_imm32(GpReg::R8, u32::from(signed));
    call_helper(emitter, HELP_DIV_ACC_OFFSET);
    context.emitter.test_r64_r64(GpReg::Rax, GpReg::Rax);
    context
        .fault_exits
        .push(context.emitter.jcc_rel32_placeholder(ConditionCode::S));
}

fn helper_read_offset(size: Size) -> i32 {
    match size {
        Size::Byte => HELP_READ_BYTE_OFFSET,
        Size::Word => HELP_READ_WORD_OFFSET,
        Size::Dword => HELP_READ_DWORD_OFFSET,
    }
}

fn helper_write_offset(size: Size) -> i32 {
    match size {
        Size::Byte => HELP_WRITE_BYTE_OFFSET,
        Size::Word => HELP_WRITE_WORD_OFFSET,
        Size::Dword => HELP_WRITE_DWORD_OFFSET,
    }
}

fn helper_push_offset(size: Size) -> i32 {
    match size {
        Size::Word => HELP_PUSH_WORD_OFFSET,
        Size::Dword => HELP_PUSH_DWORD_OFFSET,
        Size::Byte => unreachable!(),
    }
}

fn helper_pop_offset(size: Size) -> i32 {
    match size {
        Size::Word => HELP_POP_WORD_OFFSET,
        Size::Dword => HELP_POP_DWORD_OFFSET,
        Size::Byte => unreachable!(),
    }
}

fn load_guest_reg(emitter: &mut Emitter<'_>, dst: GpReg, reg: &RegOperand) {
    match reg.reg {
        GuestReg::Gpr(gpr) => match reg.size {
            Size::Byte => emitter.movzx_r32_m8(dst, GpReg::Rbp, gpr_offset(gpr)),
            Size::Word => emitter.movzx_r32_m16(dst, GpReg::Rbp, gpr_offset(gpr)),
            Size::Dword => emitter.mov_r32_m32(dst, GpReg::Rbp, gpr_offset(gpr)),
        },
        GuestReg::ByteHi(gpr) => {
            emitter.mov_r32_m32(dst, GpReg::Rbp, gpr_offset(gpr));
            emitter.shr_r32_imm8(dst, 8);
            emitter.and_r32_imm32(dst, 0xFF);
        }
    }
}

fn store_guest_reg(emitter: &mut Emitter<'_>, reg: &RegOperand, src: GpReg) {
    match reg.reg {
        GuestReg::Gpr(gpr) => match reg.size {
            Size::Dword => emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(gpr), src),
            Size::Word => {
                emitter.mov_r32_m32(GpReg::Rdx, GpReg::Rbp, gpr_offset(gpr));
                emitter.and_r32_imm32(GpReg::Rdx, 0xFFFF_0000);
                emitter.mov_r32_r32(GpReg::Rax, src, Size::Dword);
                emitter.and_r32_imm32(GpReg::Rax, 0x0000_FFFF);
                emitter.or_r32_r32(GpReg::Rdx, GpReg::Rax);
                emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(gpr), GpReg::Rdx);
            }
            Size::Byte => {
                emitter.mov_r32_m32(GpReg::Rdx, GpReg::Rbp, gpr_offset(gpr));
                emitter.and_r32_imm32(GpReg::Rdx, 0xFFFF_FF00);
                emitter.mov_r32_r32(GpReg::Rax, src, Size::Dword);
                emitter.and_r32_imm32(GpReg::Rax, 0x0000_00FF);
                emitter.or_r32_r32(GpReg::Rdx, GpReg::Rax);
                emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(gpr), GpReg::Rdx);
            }
        },
        GuestReg::ByteHi(gpr) => {
            emitter.mov_r32_m32(GpReg::Rdx, GpReg::Rbp, gpr_offset(gpr));
            emitter.and_r32_imm32(GpReg::Rdx, 0xFFFF_00FF);
            emitter.mov_r32_r32(GpReg::Rax, src, Size::Dword);
            emitter.and_r32_imm32(GpReg::Rax, 0x0000_00FF);
            emitter.shl_r32_imm8(GpReg::Rax, 8);
            emitter.or_r32_r32(GpReg::Rdx, GpReg::Rax);
            emitter.mov_m32_r32(GpReg::Rbp, gpr_offset(gpr), GpReg::Rdx);
        }
    }
}

fn store_guest_reg_from_reg(emitter: &mut Emitter<'_>, offset: i32, src: GpReg) {
    emitter.mov_m32_r32(GpReg::Rbp, offset, src);
}

fn emit_compute_mem_offset(emitter: &mut Emitter<'_>, mem: &MemOperand, dst: GpReg) {
    emitter.mov_r32_imm32(dst, mem.disp as u32);
    if let Some(base) = mem.base {
        let base_op = RegOperand {
            reg: GuestReg::Gpr(base),
            size: match mem.addr_size {
                AddrSize::Addr16 => Size::Word,
                AddrSize::Addr32 => Size::Dword,
            },
        };
        load_guest_reg(emitter, GpReg::Rax, &base_op);
        emitter.add_r32_r32(dst, GpReg::Rax, Size::Dword);
    }
    if let Some(index) = mem.index {
        let index_op = RegOperand {
            reg: GuestReg::Gpr(index),
            size: match mem.addr_size {
                AddrSize::Addr16 => Size::Word,
                AddrSize::Addr32 => Size::Dword,
            },
        };
        load_guest_reg(emitter, GpReg::Rax, &index_op);
        if mem.scale != 0 {
            emitter.shl_r32_imm8(GpReg::Rax, mem.scale);
        }
        emitter.add_r32_r32(dst, GpReg::Rax, Size::Dword);
    }
    if matches!(mem.addr_size, AddrSize::Addr16) {
        emitter.and_r32_imm32(dst, 0xFFFF);
    }
    load_seg_arg(emitter, mem.seg);
}

fn load_seg_arg(emitter: &mut Emitter<'_>, seg: SegReg32) {
    emitter.mov_r32_imm32(GpReg::Rdx, seg as u32);
}

fn write_full_eip(emitter: &mut Emitter<'_>, eip: u32) {
    emitter.mov_m32_imm32(GpReg::Rbp, IP_UPPER_OFFSET, eip & 0xFFFF_0000);
    emitter.mov_m16_imm16(GpReg::Rbp, IP_OFFSET, eip as u16);
}

fn write_near_eip_reg_size(emitter: &mut Emitter<'_>, reg: GpReg, size: Size) {
    if matches!(size, Size::Word) {
        emitter.mov_m32_imm32(GpReg::Rbp, IP_UPPER_OFFSET, 0);
        emitter.mov_m16_r16(GpReg::Rbp, IP_OFFSET, reg);
    } else {
        emitter.mov_r32_r32(GpReg::Rdx, reg, Size::Dword);
        emitter.and_r32_imm32(GpReg::Rdx, 0xFFFF_0000);
        emitter.mov_m32_r32(GpReg::Rbp, IP_UPPER_OFFSET, GpReg::Rdx);
        emitter.mov_m16_r16(GpReg::Rbp, IP_OFFSET, reg);
    }
}

fn emit_return_with_outcome(emitter: &mut Emitter<'_>, outcome: u32) {
    emitter.mov_r64_m64(GpReg::Rax, GpReg::Rsp, 0);
    emitter.mov_m64_r64(GpReg::Rax, CTX_CYCLES_OFFSET, GpReg::R14);
    emitter.add_rsp_imm8(8);
    emitter.pop_r64(GpReg::R15);
    emitter.pop_r64(GpReg::R14);
    emitter.pop_r64(GpReg::R13);
    emitter.pop_r64(GpReg::R12);
    emitter.pop_r64(GpReg::Rbx);
    emitter.pop_r64(GpReg::Rbp);
    emitter.mov_r32_imm32(GpReg::Rax, outcome);
    emitter.ret();
}

fn gpr_offset(reg: DwordReg) -> i32 {
    REGS_OFFSET + (reg as i32 * REG_SLOT_SIZE)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GpReg {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl GpReg {
    fn low3(self) -> u8 {
        (self as u8) & 7
    }

    fn ext(self) -> bool {
        (self as u8) & 8 != 0
    }
}

#[derive(Clone, Copy)]
enum BinOp {
    Add,
    Or,
    Adc,
    Sbb,
    And,
    Sub,
    Xor,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum ConditionCode {
    O = 0x0,
    No = 0x1,
    B = 0x2,
    Nb = 0x3,
    Z = 0x4,
    Nz = 0x5,
    Be = 0x6,
    A = 0x7,
    S = 0x8,
    Ns = 0x9,
    P = 0xA,
    Np = 0xB,
    L = 0xC,
    Ge = 0xD,
    Le = 0xE,
    G = 0xF,
}

type Label = usize;

struct Emitter<'a> {
    bytes: &'a mut Vec<u8>,
}

impl<'a> Emitter<'a> {
    fn new(bytes: &'a mut Vec<u8>) -> Self {
        Self { bytes }
    }

    fn position(&self) -> usize {
        self.bytes.len()
    }

    fn new_label(&self) -> Label {
        self.position()
    }

    fn place_label(&mut self, _label: Label) {}

    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn rex(&mut self, w: bool, r: bool, x: bool, b: bool) {
        let rex = 0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8);
        if rex != 0x40 {
            self.bytes.push(rex);
        }
    }

    fn modrm(&mut self, mode: u8, reg: u8, rm: u8) {
        self.bytes.push((mode << 6) | ((reg & 7) << 3) | (rm & 7));
    }

    fn sib(&mut self, scale: u8, index: u8, base: u8) {
        self.bytes
            .push((scale << 6) | ((index & 7) << 3) | (base & 7));
    }

    fn mov_r64_m64(&mut self, dst: GpReg, base: GpReg, disp: i32) {
        self.rex(true, dst.ext(), false, base.ext());
        self.bytes.push(0x8B);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, dst.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, dst.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn mov_r32_m32(&mut self, dst: GpReg, base: GpReg, disp: i32) {
        self.rex(false, dst.ext(), false, base.ext());
        self.bytes.push(0x8B);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, dst.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, dst.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn movzx_r32_m8(&mut self, dst: GpReg, base: GpReg, disp: i32) {
        self.rex(false, dst.ext(), false, base.ext());
        self.emit(&[0x0F, 0xB6]);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, dst.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, dst.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn movzx_r32_m16(&mut self, dst: GpReg, base: GpReg, disp: i32) {
        self.rex(false, dst.ext(), false, base.ext());
        self.emit(&[0x0F, 0xB7]);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, dst.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, dst.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn mov_m64_r64(&mut self, base: GpReg, disp: i32, src: GpReg) {
        self.rex(true, src.ext(), false, base.ext());
        self.bytes.push(0x89);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, src.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, src.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn mov_m32_r32(&mut self, base: GpReg, disp: i32, src: GpReg) {
        self.rex(false, src.ext(), false, base.ext());
        self.bytes.push(0x89);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, src.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, src.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn mov_m32_imm32(&mut self, base: GpReg, disp: i32, imm: u32) {
        self.rex(false, false, false, base.ext());
        self.bytes.push(0xC7);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, 0, 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, 0, base.low3());
        }
        self.emit(&disp.to_le_bytes());
        self.emit(&imm.to_le_bytes());
    }

    fn mov_m16_imm16(&mut self, base: GpReg, disp: i32, imm: u16) {
        self.bytes.push(0x66);
        self.rex(false, false, false, base.ext());
        self.bytes.push(0xC7);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, 0, 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, 0, base.low3());
        }
        self.emit(&disp.to_le_bytes());
        self.emit(&imm.to_le_bytes());
    }

    fn mov_m16_r16(&mut self, base: GpReg, disp: i32, src: GpReg) {
        self.bytes.push(0x66);
        self.rex(false, src.ext(), false, base.ext());
        self.bytes.push(0x89);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, src.low3(), 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, src.low3(), base.low3());
        }
        self.emit(&disp.to_le_bytes());
    }

    fn mov_m8_imm8(&mut self, base: GpReg, disp: i32, imm: u8) {
        self.rex(false, false, false, base.ext());
        self.bytes.push(0xC6);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, 0, 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, 0, base.low3());
        }
        self.emit(&disp.to_le_bytes());
        self.bytes.push(imm);
    }

    fn mov_r32_imm32(&mut self, dst: GpReg, imm: u32) {
        self.rex(false, false, false, dst.ext());
        self.bytes.push(0xB8 + dst.low3());
        self.emit(&imm.to_le_bytes());
    }

    /// `mov dst32, [base + index * (1 << scale_log2) + disp32]`
    fn mov_r32_m32_sib(
        &mut self,
        dst: GpReg,
        base: GpReg,
        index: GpReg,
        scale_log2: u8,
        disp: i32,
    ) {
        debug_assert!(scale_log2 <= 3);
        debug_assert!(!matches!(index, GpReg::Rsp));
        self.rex(false, dst.ext(), index.ext(), base.ext());
        self.bytes.push(0x8B);
        self.modrm(0b10, dst.low3(), 0b100);
        self.sib(scale_log2, index.low3(), base.low3());
        self.emit(&disp.to_le_bytes());
    }

    /// `cmp [base + index * (1 << scale_log2) + disp32], src32`
    fn cmp_m32_r32_sib(
        &mut self,
        base: GpReg,
        index: GpReg,
        scale_log2: u8,
        disp: i32,
        src: GpReg,
    ) {
        debug_assert!(scale_log2 <= 3);
        debug_assert!(!matches!(index, GpReg::Rsp));
        self.rex(false, src.ext(), index.ext(), base.ext());
        self.bytes.push(0x39); // CMP r/m32, r32
        self.modrm(0b10, src.low3(), 0b100);
        self.sib(scale_log2, index.low3(), base.low3());
        self.emit(&disp.to_le_bytes());
    }

    /// `cmp byte [base + index + disp32], imm8`
    fn cmp_m8_imm8_sib(&mut self, base: GpReg, index: GpReg, disp: i32, imm: u8) {
        debug_assert!(!matches!(index, GpReg::Rsp));
        self.rex(false, false, index.ext(), base.ext());
        self.bytes.push(0x80); // CMP r/m8, imm8
        self.modrm(0b10, 7, 0b100);
        self.sib(0, index.low3(), base.low3());
        self.emit(&disp.to_le_bytes());
        self.bytes.push(imm);
    }

    fn mov_r64_r64(&mut self, dst: GpReg, src: GpReg) {
        self.rex(true, src.ext(), false, dst.ext());
        self.bytes.push(0x89);
        self.modrm(0b11, src.low3(), dst.low3());
    }

    fn mov_r32_r32(&mut self, dst: GpReg, src: GpReg, size: Size) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        self.rex(false, src.ext(), false, dst.ext());
        self.bytes.push(0x89);
        self.modrm(0b11, src.low3(), dst.low3());
    }

    fn movzx_r32_r8(&mut self, dst: GpReg, src: GpReg) {
        self.rex(false, dst.ext(), false, src.ext());
        self.emit(&[0x0F, 0xB6]);
        self.modrm(0b11, dst.low3(), src.low3());
    }

    fn movzx_r32_r8_high(&mut self, dst: GpReg) {
        self.emit(&[0x0F, 0xB6, 0xC4 | (dst.low3() << 3)]);
    }

    fn setcc_r8(&mut self, cc: ConditionCode, dst: GpReg) {
        self.rex(false, false, false, dst.ext());
        self.emit(&[0x0F, 0x90 + cc as u8]);
        self.modrm(0b11, 0, dst.low3());
    }

    fn lahf(&mut self) {
        self.emit(&[0x9F]);
    }

    fn call_r64(&mut self, reg: GpReg) {
        self.rex(false, false, false, reg.ext());
        self.emit(&[0xFF]);
        self.modrm(0b11, 2, reg.low3());
    }

    fn ret(&mut self) {
        self.bytes.push(0xC3);
    }

    fn push_r64(&mut self, reg: GpReg) {
        self.rex(false, false, false, reg.ext());
        self.bytes.push(0x50 + reg.low3());
    }

    fn pop_r64(&mut self, reg: GpReg) {
        self.rex(false, false, false, reg.ext());
        self.bytes.push(0x58 + reg.low3());
    }

    fn sub_rsp_imm8(&mut self, imm: u8) {
        self.emit(&[0x48, 0x83, 0xEC, imm]);
    }

    fn add_rsp_imm8(&mut self, imm: u8) {
        self.emit(&[0x48, 0x83, 0xC4, imm]);
    }

    fn sub_r64_imm32(&mut self, reg: GpReg, imm: i32) {
        self.rex(true, false, false, reg.ext());
        self.emit(&[0x81]);
        self.modrm(0b11, 5, reg.low3());
        self.emit(&imm.to_le_bytes());
    }

    fn add_m32_imm32(&mut self, base: GpReg, disp: i32, imm: i32) {
        self.rex(false, false, false, base.ext());
        self.emit(&[0x81]);
        if matches!(base, GpReg::Rsp | GpReg::R12) {
            self.modrm(0b10, 0, 0b100);
            self.sib(0, 0b100, base.low3());
        } else {
            self.modrm(0b10, 0, base.low3());
        }
        self.emit(&disp.to_le_bytes());
        self.emit(&imm.to_le_bytes());
    }

    fn add_r32_imm32(&mut self, reg: GpReg, imm: i32) {
        self.binop_reg_imm(BinOp::Add, Size::Dword, reg, imm);
    }

    fn sub_r32_imm32(&mut self, reg: GpReg, imm: i32) {
        self.binop_reg_imm(BinOp::Sub, Size::Dword, reg, imm);
    }

    fn add_r32_r32(&mut self, dst: GpReg, src: GpReg, size: Size) {
        self.binop_reg_reg(BinOp::Add, size, dst, src);
    }

    fn and_r32_imm32(&mut self, reg: GpReg, imm: u32) {
        self.binop_reg_imm(BinOp::And, Size::Dword, reg, imm as i32);
    }

    fn or_r32_r32(&mut self, dst: GpReg, src: GpReg) {
        self.binop_reg_reg(BinOp::Or, Size::Dword, dst, src);
    }

    fn shr_r32_imm8(&mut self, reg: GpReg, imm: u8) {
        self.group2_imm8(ShiftOp::Shr, Size::Dword, reg, imm);
    }

    fn shl_r32_imm8(&mut self, reg: GpReg, imm: u8) {
        self.group2_imm8(ShiftOp::Shl, Size::Dword, reg, imm);
    }

    fn sar_r32_imm8(&mut self, reg: GpReg, imm: u8) {
        self.group2_imm8(ShiftOp::Sar, Size::Dword, reg, imm);
    }

    fn neg_r32(&mut self, reg: GpReg) {
        self.rex(false, false, false, reg.ext());
        self.emit(&[0xF7]);
        self.modrm(0b11, 3, reg.low3());
    }

    fn not_r32(&mut self, reg: GpReg) {
        self.rex(false, false, false, reg.ext());
        self.emit(&[0xF7]);
        self.modrm(0b11, 2, reg.low3());
    }

    fn bswap_r32(&mut self, reg: GpReg) {
        self.rex(false, false, false, reg.ext());
        self.emit(&[0x0F, 0xC8 | reg.low3()]);
    }

    fn cmp_r32_imm32(&mut self, reg: GpReg, imm: i32) {
        // CMP r/m32, imm32: REX.W=0, 0x81 /7
        self.rex(false, false, false, reg.ext());
        self.emit(&[0x81]);
        self.modrm(0b11, 7, reg.low3());
        self.bytes.extend_from_slice(&imm.to_le_bytes());
    }

    fn bt_r32_imm8(&mut self, reg: GpReg, imm: u8) {
        self.rex(false, false, false, reg.ext());
        self.emit(&[0x0F, 0xBA]);
        self.modrm(0b11, 4, reg.low3());
        self.bytes.push(imm);
    }

    fn binop_reg_reg(&mut self, op: BinOp, size: Size, dst: GpReg, src: GpReg) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        let opcode = match op {
            BinOp::Add => 0x01,
            BinOp::Or => 0x09,
            BinOp::Adc => 0x11,
            BinOp::Sbb => 0x19,
            BinOp::And => 0x21,
            BinOp::Sub => 0x29,
            BinOp::Xor => 0x31,
        };
        self.rex(false, src.ext(), false, dst.ext());
        self.bytes.push(opcode_for_size(opcode, size));
        self.modrm(0b11, src.low3(), dst.low3());
    }

    fn binop_reg_imm(&mut self, op: BinOp, size: Size, reg: GpReg, imm: i32) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        let ext = match op {
            BinOp::Add => 0,
            BinOp::Or => 1,
            BinOp::Adc => 2,
            BinOp::Sbb => 3,
            BinOp::And => 4,
            BinOp::Sub => 5,
            BinOp::Xor => 6,
        };
        self.rex(false, false, false, reg.ext());
        self.emit(&[match size {
            Size::Byte => 0x80,
            Size::Word | Size::Dword => 0x81,
        }]);
        self.modrm(0b11, ext, reg.low3());
        match size {
            Size::Byte => self.bytes.push(imm as u8),
            Size::Word => self.emit(&(imm as i16).to_le_bytes()),
            Size::Dword => self.emit(&imm.to_le_bytes()),
        }
    }

    fn test_r32_r32(&mut self, lhs: GpReg, rhs: GpReg, size: Size) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        self.rex(false, rhs.ext(), false, lhs.ext());
        self.emit(&[opcode_for_size(0x85, size)]);
        self.modrm(0b11, rhs.low3(), lhs.low3());
    }

    fn test_r64_r64(&mut self, lhs: GpReg, rhs: GpReg) {
        self.rex(true, rhs.ext(), false, lhs.ext());
        self.emit(&[0x85]);
        self.modrm(0b11, rhs.low3(), lhs.low3());
    }

    fn cmp_r32_r32(&mut self, lhs: GpReg, rhs: GpReg, size: Size) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        self.rex(false, rhs.ext(), false, lhs.ext());
        self.emit(&[opcode_for_size(0x39, size)]);
        self.modrm(0b11, rhs.low3(), lhs.low3());
    }

    fn unary_inc(&mut self, size: Size, reg: GpReg) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        self.rex(false, false, false, reg.ext());
        self.emit(&[match size {
            Size::Byte => 0xFE,
            Size::Word | Size::Dword => 0xFF,
        }]);
        self.modrm(0b11, 0, reg.low3());
    }

    fn unary_dec(&mut self, size: Size, reg: GpReg) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        self.rex(false, false, false, reg.ext());
        self.emit(&[match size {
            Size::Byte => 0xFE,
            Size::Word | Size::Dword => 0xFF,
        }]);
        self.modrm(0b11, 1, reg.low3());
    }

    fn group2_cl(&mut self, op: ShiftOp, size: Size, reg: GpReg) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        let opcode = match size {
            Size::Byte => 0xD2,
            Size::Word | Size::Dword => 0xD3,
        };
        self.rex(false, false, false, reg.ext());
        self.emit(&[opcode]);
        self.modrm(0b11, shift_ext(op), reg.low3());
    }

    fn group2_imm8(&mut self, op: ShiftOp, size: Size, reg: GpReg, imm: u8) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        let opcode = match size {
            Size::Byte => 0xC0,
            Size::Word | Size::Dword => 0xC1,
        };
        self.rex(false, false, false, reg.ext());
        self.emit(&[opcode]);
        self.modrm(0b11, shift_ext(op), reg.low3());
        self.bytes.push(imm);
    }

    fn imul_reg_reg(&mut self, size: Size, dst: GpReg, src: GpReg) {
        if matches!(size, Size::Word) {
            self.bytes.push(0x66);
        }
        self.rex(false, dst.ext(), false, src.ext());
        self.emit(&[0x0F, 0xAF]);
        self.modrm(0b11, dst.low3(), src.low3());
    }

    fn jcc_rel32_placeholder(&mut self, cc: ConditionCode) -> usize {
        self.emit(&[0x0F, 0x80 + cc as u8]);
        let position = self.bytes.len();
        self.emit(&0i32.to_le_bytes());
        position
    }

    fn jmp_rel32_placeholder(&mut self) -> usize {
        self.bytes.push(0xE9);
        let position = self.bytes.len();
        self.emit(&0i32.to_le_bytes());
        position
    }

    fn patch_rel32_branches(&mut self, positions: &[usize], target: usize) {
        for &position in positions {
            let next = position + 4;
            let relative = i32::try_from(target as isize - next as isize)
                .expect("x64 dynarec: relative branch out of range");
            self.bytes[position..position + 4].copy_from_slice(&relative.to_le_bytes());
        }
    }
}

fn opcode_for_size(base: u8, size: Size) -> u8 {
    match size {
        Size::Byte => base.saturating_sub(1),
        Size::Word | Size::Dword => base,
    }
}

fn shift_ext(op: ShiftOp) -> u8 {
    match op {
        ShiftOp::Rol => 0,
        ShiftOp::Ror => 1,
        ShiftOp::Rcl => 2,
        ShiftOp::Rcr => 3,
        ShiftOp::Shl => 4,
        ShiftOp::Shr => 5,
        ShiftOp::Sar => 7,
    }
}
