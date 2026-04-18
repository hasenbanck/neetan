//! `I386Jit`: the block-driven dispatcher that implements `common::Cpu`.
//!
//! This mirrors the shape of the interpreter's `impl_cpu_run_for!` macro
//! body (crates/cpu/src/lib.rs:17-81), substituting the single-step
//! `execute_one` call with block lookup/compile/execute.

use std::{cell::RefCell, collections::hash_map::Entry, rc::Rc};

use common::{Bus, CodeInvalidator, Cpu, SegmentRegister};
use cpu::{CPU_MODEL_486, I386, I386State};

#[cfg(all(target_arch = "x86_64", unix))]
use crate::backend_x64;
use crate::{
    backend_bytecode::{self, BlockOutcome},
    block_map::{BlockMap, CachedBlock},
    code_cache::{CodeCache, DEFAULT_CACHE_SIZE},
    decoder,
    ir::IrOp,
};

/// Callback handed to the bus for SMC notifications.
///
/// Records pending invalidations into a shared buffer that the JIT
/// dispatcher drains between blocks. This indirection keeps the
/// currently-executing block's backing storage alive: bus writes from
/// inside a block only append to the buffer, they never mutate the
/// [`BlockMap`] while the dispatcher is still executing.
///
/// The invalidator also stays registered across [`Cpu::run_for`]
/// boundaries so that callers (notably tests) that write code through
/// the bus between slices still have those writes tracked. The
/// [`I386Jit::run_for`] entry drains the accumulated buffer before
/// dispatching the first block.
struct BlockMapInvalidator {
    pending: Rc<RefCell<Vec<(u32, u32)>>>,
}

impl CodeInvalidator for BlockMapInvalidator {
    fn invalidate_range(&mut self, phys_start: u32, phys_end: u32) {
        self.pending.borrow_mut().push((phys_start, phys_end));
    }
}

/// Preferred backend for [`I386Jit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitBackend {
    /// Use the best native backend available on the current host,
    /// otherwise fall back to the bytecode backend.
    Auto,
    /// Force the portable bytecode backend.
    Bytecode,
    /// Prefer the native x86-64 backend. On unsupported hosts this
    /// falls back to [`JitBackend::Bytecode`].
    X64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectedBackend {
    Bytecode,
    #[cfg(all(target_arch = "x86_64", unix))]
    X64,
}

/// A JIT-backed 80386+ CPU core. Wraps an interpreter instance that
/// holds the architectural state; dispatches to compiled blocks when
/// possible and falls back to the interpreter for anything the decoder
/// does not cover (FPU, string ops, privileged ops, far control
/// transfers, segment-register loads).
pub struct I386Jit<const CPU_MODEL: u8 = { CPU_MODEL_486 }> {
    inner: I386<CPU_MODEL>,
    blocks: BlockMap,
    recycled_ops: Vec<Vec<IrOp>>,
    backend: SelectedBackend,
    code_cache: CodeCache,
    #[cfg(all(target_arch = "x86_64", unix))]
    compiler_scratch: backend_x64::CompilerScratch,
    stats: JitStats,
    /// Pending SMC invalidations accumulated by the bus invalidator
    /// between block exits. Drained after every block so a write that
    /// targets code never overlaps the dispatcher's in-flight block
    /// state.
    ///
    /// Shared via `Rc<RefCell<_>>` with a [`BlockMapInvalidator`] handed
    /// to the bus: the JIT drains the vec between blocks while the bus
    /// appends to it during RAM writes. The two accesses are
    /// temporally disjoint; `RefCell` enforces that at runtime.
    pending_invalidations: Rc<RefCell<Vec<(u32, u32)>>>,
    /// Cached control registers, used to detect paging mode changes that
    /// must force a full cache flush (CR3 reload, CR0.PG/PE toggle,
    /// CR4.PSE/PGE toggle, etc.).
    prev_cr0: u32,
    prev_cr3: u32,
}

/// Per-`run_for` slice instrumentation counters. Reset at the start of
/// each call to `run_for` so callers can measure what fraction of work
/// was carried by the JIT versus the interpreter fallback.
#[derive(Debug, Default, Clone, Copy)]
pub struct JitStats {
    /// Number of compiled blocks that finished (reached their exit
    /// terminator). Blocks that aborted via Fault or Fallback are not
    /// counted here.
    pub blocks_executed: u64,
    /// Total number of IR ops run through the selected backend.
    pub jit_instrs_executed: u64,
    /// Number of single-instruction interpreter fallbacks invoked.
    pub fallback_instrs: u64,
}

impl<const CPU_MODEL: u8> Default for I386Jit<CPU_MODEL> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CPU_MODEL: u8> I386Jit<CPU_MODEL> {
    /// Creates a new JIT-backed CPU in its reset state.
    pub fn new() -> Self {
        Self::new_with_backend(JitBackend::Auto)
    }

    /// Creates a new JIT-backed CPU with the requested backend
    /// preference.
    pub fn new_with_backend(backend: JitBackend) -> Self {
        Self {
            inner: I386::<CPU_MODEL>::new(),
            blocks: BlockMap::new(),
            recycled_ops: Vec::new(),
            backend: Self::resolve_backend(backend),
            code_cache: CodeCache::new(DEFAULT_CACHE_SIZE),
            #[cfg(all(target_arch = "x86_64", unix))]
            compiler_scratch: backend_x64::CompilerScratch::new(),
            stats: JitStats::default(),
            pending_invalidations: Rc::new(RefCell::new(Vec::new())),
            prev_cr0: 0,
            prev_cr3: 0,
        }
    }

    /// Returns the backend selected for this CPU instance.
    pub fn backend(&self) -> JitBackend {
        match self.backend {
            SelectedBackend::Bytecode => JitBackend::Bytecode,
            #[cfg(all(target_arch = "x86_64", unix))]
            SelectedBackend::X64 => JitBackend::X64,
        }
    }

    /// Returns the stats recorded during the most recent `run_for`
    /// slice. Counters reset at the start of each slice.
    pub fn stats(&self) -> JitStats {
        self.stats
    }

    /// Returns a reference to the embedded interpreter. Test and debug
    /// only; the JIT owns the canonical state.
    pub fn inner(&self) -> &I386<CPU_MODEL> {
        &self.inner
    }

    /// Returns a mutable reference to the embedded interpreter state.
    pub fn inner_mut(&mut self) -> &mut I386<CPU_MODEL> {
        &mut self.inner
    }

    /// Returns the embedded `I386State` (architectural guest state).
    pub fn state(&self) -> &I386State {
        &self.inner.state
    }

    /// Returns a mutable reference to the embedded `I386State`.
    pub fn state_mut(&mut self) -> &mut I386State {
        &mut self.inner.state
    }

    /// Delegates to the interpreter's `load_state`.
    pub fn load_state(&mut self, state: &I386State) {
        self.inner.load_state(state);
    }

    /// Signals a maskable interrupt request to the embedded core.
    pub fn signal_irq(&mut self) {
        self.inner.signal_irq();
    }

    /// Signals a non-maskable interrupt to the embedded core.
    pub fn signal_nmi(&mut self) {
        self.inner.signal_nmi();
    }

    fn resolve_backend(backend: JitBackend) -> SelectedBackend {
        #[cfg(all(target_arch = "x86_64", unix))]
        {
            match backend {
                JitBackend::Auto | JitBackend::X64 => SelectedBackend::X64,
                JitBackend::Bytecode => SelectedBackend::Bytecode,
            }
        }

        #[cfg(not(all(target_arch = "x86_64", unix)))]
        {
            let _ = backend;
            SelectedBackend::Bytecode
        }
    }

    fn flush_caches(&mut self) {
        self.blocks.flush_into(&mut self.recycled_ops);
        self.code_cache.reset();
    }

    /// Drains the SMC invalidation queue built up during block execution
    /// and drops every translated block whose start page was written.
    ///
    /// When at least one block was invalidated, we also reset the native
    /// code cache and re-compile on demand: entries in the bump cache
    /// can outlive their block maps, so dropping the IR without flushing
    /// would leave stale `CompiledBlock::entry` pointers.
    fn process_pending_invalidations(&mut self) {
        let mut any_invalidated = false;
        let pending: Vec<(u32, u32)> = self.pending_invalidations.borrow_mut().drain(..).collect();
        for (phys_start, phys_end) in pending {
            if self
                .blocks
                .invalidate_range(phys_start, phys_end, &mut self.recycled_ops)
            {
                any_invalidated = true;
            }
        }
        if any_invalidated {
            self.code_cache.reset();
            #[cfg(all(target_arch = "x86_64", unix))]
            self.blocks.clear_native();
        }
    }

    /// Returns a block for `phys`, decoding and inserting one if the
    /// map has no entry yet. The caller can then reborrow `&self.blocks`
    /// to obtain a stable `&CachedBlock` for dispatch; the `Box`
    /// indirection keeps the block's address invariant across future
    /// map operations that do not mutate this specific entry.
    fn ensure_block<B: Bus>(&mut self, bus: &mut B, phys: u32) {
        let inner = &self.inner;
        let recycled_ops = &mut self.recycled_ops;
        if let Entry::Vacant(entry) = self.blocks.entry(phys) {
            let eip = inner.state.eip();
            let block = decoder::decode_block(
                inner,
                bus,
                phys,
                eip,
                recycled_ops.pop().unwrap_or_default(),
            );
            debug_assert_eq!(block.phys_addr, phys);
            entry.insert(Box::new(CachedBlock::new(block)));
            self.blocks.register_block_page(phys);
        }
    }

    fn run_block_bytecode<B: Bus>(&mut self, bus: &mut B) -> RunStep {
        let Some(phys) = self.inner.current_eip_phys(bus) else {
            // Page fault during code fetch. Let the interpreter deliver
            // the exception.
            self.inner.step_instruction(bus);
            self.stats.fallback_instrs += 1;
            return RunStep::Advanced;
        };

        self.ensure_block(bus, phys);

        // Charge the block's static cycle cost up front.
        let before_cycles = self.inner.cycles_remaining();
        let block_cycles = self
            .blocks
            .get(phys)
            .expect("bytecode dynarec: block missing after ensure_block")
            .block
            .cycles;
        self.inner
            .set_cycles_remaining(before_cycles - block_cycles);
        // Reborrow against `self.blocks` for the duration of the
        // call. `execute_block` mutates `self.inner` and `bus`; neither
        // path reaches back into `self.blocks` mid-call (bus writes
        // only append to `pending_invalidations`). Rust's borrow
        // checker splits the disjoint fields.
        let block_ref = &self
            .blocks
            .get(phys)
            .expect("bytecode dynarec: block missing after ensure_block")
            .block;
        let (outcome, executed) = backend_bytecode::execute_block(block_ref, &mut self.inner, bus);
        self.stats.jit_instrs_executed += executed;

        match outcome {
            BlockOutcome::Continue => {
                self.stats.blocks_executed += 1;
                RunStep::Advanced
            }
            BlockOutcome::Cycles => RunStep::Stop,
            BlockOutcome::Fallback { guest_eip } => {
                // TODO: refund unused cycle budget for the cap-exhausted
                // portion of the block. We charge the full static cost
                // at block entry, but when a Fallback fires only the
                // prefix up to the fallback op actually ran. Correctness
                // is unaffected; this refinement is a pure accounting
                // improvement for closer cycle-accurate emulation.
                self.inner.state.set_eip(guest_eip);
                if bus.cpu_should_yield() {
                    // An InterpCall inside the block body (OUT to a BIOS
                    // HLE trap port, for example) asked the CPU to yield.
                    // The machine loop must handle the HLE trap before
                    // the IRET-ish fallback instruction runs, otherwise
                    // the stubbed stack frame is consumed by the wrong
                    // party. Return to the dispatcher without stepping.
                    RunStep::Stop
                } else {
                    self.inner.step_instruction(bus);
                    self.stats.fallback_instrs += 1;
                    RunStep::Advanced
                }
            }
            BlockOutcome::Fault => {
                // `fault_pending` is set on the CPU; any pending work
                // has already been handled inside `I386::raise_*`. The
                // dispatcher simply continues.
                RunStep::Advanced
            }
            BlockOutcome::Halt => {
                self.stats.blocks_executed += 1;
                self.inner.set_halted_flag(true);
                RunStep::Advanced
            }
        }
    }

    #[cfg(all(target_arch = "x86_64", unix))]
    fn run_block_x64<B: Bus>(
        &mut self,
        bus: &mut B,
        run_state: &mut backend_x64::RunState<CPU_MODEL, B>,
    ) -> RunStep {
        let Some(phys) = self.inner.current_eip_phys(bus) else {
            self.inner.step_instruction(bus);
            self.stats.fallback_instrs += 1;
            return RunStep::Advanced;
        };

        self.ensure_block(bus, phys);
        let before_cycles = self.inner.cycles_remaining();

        if self
            .blocks
            .get(phys)
            .expect("x64 dynarec: block missing after ensure_block")
            .native
            .is_none()
        {
            let compile_result = backend_x64::compile_block(
                &mut self.code_cache,
                &self
                    .blocks
                    .get(phys)
                    .expect("x64 dynarec: block missing after ensure_block")
                    .block,
                &mut self.compiler_scratch,
            );
            let compiled = match compile_result {
                Some(compiled) => compiled,
                None => {
                    self.code_cache.reset();
                    self.blocks.clear_native();
                    backend_x64::compile_block(
                        &mut self.code_cache,
                        &self
                            .blocks
                            .get(phys)
                            .expect("x64 dynarec: block missing after ensure_block")
                            .block,
                        &mut self.compiler_scratch,
                    )
                    .expect("x64 dynarec: empty cache could not fit compiled block")
                }
            };
            self.blocks
                .get_mut(phys)
                .expect("x64 dynarec: block missing after ensure_block")
                .native = Some(compiled);
        }

        let cached = self
            .blocks
            .get(phys)
            .expect("x64 dynarec: block missing after ensure_block");
        let compiled = cached
            .native
            .as_ref()
            .expect("x64 dynarec: compiled block missing after insertion");
        let (outcome, executed, cycles_remaining) =
            run_state.execute_block(&mut self.inner, bus, compiled, &cached.block, before_cycles);
        self.inner.set_cycles_remaining(cycles_remaining);
        self.stats.jit_instrs_executed += executed;

        match outcome {
            BlockOutcome::Continue => {
                self.stats.blocks_executed += 1;
                RunStep::Advanced
            }
            BlockOutcome::Cycles => RunStep::Stop,
            BlockOutcome::Fallback { guest_eip } => {
                self.inner.state.set_eip(guest_eip);
                if bus.cpu_should_yield() {
                    // See run_block_bytecode: a yield set during an
                    // in-block InterpCall (OUT to an HLE trap port)
                    // must not be followed by a fallback step.
                    RunStep::Stop
                } else {
                    self.inner.step_instruction(bus);
                    self.stats.fallback_instrs += 1;
                    RunStep::Advanced
                }
            }
            BlockOutcome::Fault => RunStep::Advanced,
            BlockOutcome::Halt => {
                self.stats.blocks_executed += 1;
                self.inner.set_halted_flag(true);
                RunStep::Advanced
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RunStep {
    Advanced,
    Stop,
}

impl<const CPU_MODEL: u8> Cpu for I386Jit<CPU_MODEL> {
    fn run_for(&mut self, cycles_to_run: u64, bus: &mut impl Bus) -> u64 {
        let start_cycle = bus.current_cycle();
        self.inner.set_run_start_cycle(start_cycle);
        self.inner.set_run_budget(cycles_to_run);
        self.inner.set_cycles_remaining(cycles_to_run as i64);

        // Detect control-register changes since the last slice. Any
        // change to CR0 (PE/PG/WP) or CR3 (page directory base) means
        // the linear-to-physical mapping may have shifted and every
        // translated block keyed on physical addresses is suspect.
        let current_cr0 = self.inner.cr0();
        let current_cr3 = self.inner.cr3();
        if current_cr0 != self.prev_cr0 || current_cr3 != self.prev_cr3 {
            self.flush_caches();
        }
        self.prev_cr0 = current_cr0;
        self.prev_cr3 = current_cr3;

        self.stats = JitStats::default();

        // Drain any SMC writes that landed between slices so the block
        // map reflects them before we start dispatching. The invalidator
        // stays registered across `run_for` boundaries, so callers that
        // poke RAM through the bus outside the JIT dispatcher (e.g.
        // test harnesses writing new code between slices) still see
        // their writes invalidate cached blocks.
        if !self.pending_invalidations.borrow().is_empty() {
            self.process_pending_invalidations();
        }

        // Re-register the SMC invalidator with a cloned Rc so the bus
        // can append from helper callbacks while the JIT drains between
        // blocks. RefCell enforces the non-overlapping-access contract
        // at runtime.
        let invalidator = BlockMapInvalidator {
            pending: Rc::clone(&self.pending_invalidations),
        };
        bus.register_code_invalidator(Box::new(invalidator));

        #[cfg(all(target_arch = "x86_64", unix))]
        let mut native_run_state = matches!(self.backend, SelectedBackend::X64)
            .then(backend_x64::RunState::<CPU_MODEL, _>::new);

        while self.inner.cycles_remaining() > 0 {
            if self.inner.halted_flag() {
                if bus.has_nmi() {
                    self.inner.signal_nmi();
                    self.inner.set_halted_flag(false);
                } else if self.inner.state.flags.if_flag && bus.has_irq() {
                    self.inner.signal_irq();
                    self.inner.set_halted_flag(false);
                } else {
                    let consumed = cycles_to_run as i64 - self.inner.cycles_remaining();
                    bus.set_current_cycle(start_cycle + consumed as u64);
                    return consumed as u64;
                }
            }

            // Deliver any pending NMI/IRQ before entering the next block,
            // mirroring the interpreter's per-instruction prologue (see
            // `execute_one`): first `check_interrupts`, then tick the
            // STI/POPF inhibit window. Without this the JIT would silently
            // drop IRQs that arrive while the CPU is running native
            // blocks or halted, and the post-STI window would never
            // close when HLT is handled directly by [`BlockExit::Halt`].
            if self.inner.has_pending_interrupt() {
                self.inner.check_interrupts(bus);
            }
            self.inner.tick_interrupt_window();

            match self.backend {
                SelectedBackend::Bytecode => match self.run_block_bytecode(bus) {
                    RunStep::Advanced => {}
                    RunStep::Stop => break,
                },
                #[cfg(all(target_arch = "x86_64", unix))]
                SelectedBackend::X64 => {
                    let run_state = native_run_state
                        .as_mut()
                        .expect("x64 dynarec: missing per-run native state");
                    match self.run_block_x64(bus, run_state) {
                        RunStep::Advanced => {}
                        RunStep::Stop => break,
                    }
                }
            }

            // Apply SMC invalidations that arrived during the block.
            // Deferred processing keeps any dispatcher-held references
            // into the block map valid: the map is only mutated between
            // blocks, never mid-execution.
            if !self.pending_invalidations.borrow().is_empty() {
                self.process_pending_invalidations();
            }

            // A CR3 reload inside a block body (MOV CR3, reg in the
            // interpreter fallback path) renders every physical-keyed
            // block suspect. Detect and flush.
            let new_cr0 = self.inner.cr0();
            let new_cr3 = self.inner.cr3();
            if new_cr0 != self.prev_cr0 || new_cr3 != self.prev_cr3 {
                self.flush_caches();
                self.prev_cr0 = new_cr0;
                self.prev_cr3 = new_cr3;
            }

            self.inner
                .set_cycles_remaining(self.inner.cycles_remaining() - bus.drain_wait_cycles());

            let consumed = cycles_to_run as i64 - self.inner.cycles_remaining();
            bus.set_current_cycle(start_cycle + consumed as u64);

            if bus.has_nmi() {
                self.inner.signal_nmi();
            }
            if bus.has_irq() {
                self.inner.signal_irq();
            }

            if bus.reset_pending() {
                break;
            }
            if bus.cpu_should_yield() {
                break;
            }
        }

        let actual = (cycles_to_run as i64 - self.inner.cycles_remaining()) as u64;
        bus.set_current_cycle(start_cycle + actual);
        // Leave the invalidator registered so writes the next test slice
        // performs on the bus (outside a `run_for`) are tracked too.
        actual
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.flush_caches();
        self.pending_invalidations.borrow_mut().clear();
        self.prev_cr0 = self.inner.cr0();
        self.prev_cr3 = self.inner.cr3();
    }

    fn halted(&self) -> bool {
        self.inner.halted()
    }

    fn warm_reset(&mut self, ss: u16, sp: u16, cs: u16, ip: u16) {
        self.inner.warm_reset(ss, sp, cs, ip);
        self.flush_caches();
        self.pending_invalidations.borrow_mut().clear();
        self.prev_cr0 = self.inner.cr0();
        self.prev_cr3 = self.inner.cr3();
    }

    fn ax(&self) -> u16 {
        self.inner.ax()
    }
    fn set_ax(&mut self, v: u16) {
        self.inner.set_ax(v)
    }
    fn bx(&self) -> u16 {
        self.inner.bx()
    }
    fn set_bx(&mut self, v: u16) {
        self.inner.set_bx(v)
    }
    fn cx(&self) -> u16 {
        self.inner.cx()
    }
    fn set_cx(&mut self, v: u16) {
        self.inner.set_cx(v)
    }
    fn dx(&self) -> u16 {
        self.inner.dx()
    }
    fn set_dx(&mut self, v: u16) {
        self.inner.set_dx(v)
    }
    fn sp(&self) -> u16 {
        self.inner.sp()
    }
    fn set_sp(&mut self, v: u16) {
        self.inner.set_sp(v)
    }
    fn bp(&self) -> u16 {
        self.inner.bp()
    }
    fn set_bp(&mut self, v: u16) {
        self.inner.set_bp(v)
    }
    fn si(&self) -> u16 {
        self.inner.si()
    }
    fn set_si(&mut self, v: u16) {
        self.inner.set_si(v)
    }
    fn di(&self) -> u16 {
        self.inner.di()
    }
    fn set_di(&mut self, v: u16) {
        self.inner.set_di(v)
    }
    fn es(&self) -> u16 {
        self.inner.es()
    }
    fn set_es(&mut self, v: u16) {
        self.inner.set_es(v)
    }
    fn cs(&self) -> u16 {
        self.inner.cs()
    }
    fn set_cs(&mut self, v: u16) {
        self.inner.set_cs(v)
    }
    fn ss(&self) -> u16 {
        self.inner.ss()
    }
    fn set_ss(&mut self, v: u16) {
        self.inner.set_ss(v)
    }
    fn ds(&self) -> u16 {
        self.inner.ds()
    }
    fn set_ds(&mut self, v: u16) {
        self.inner.set_ds(v)
    }
    fn ip(&self) -> u16 {
        self.inner.ip() as u16
    }
    fn set_ip(&mut self, v: u16) {
        self.inner.set_ip(v)
    }
    fn flags(&self) -> u16 {
        self.inner.flags()
    }
    fn set_flags(&mut self, v: u16) {
        self.inner.set_flags(v)
    }
    fn cpu_type(&self) -> common::CpuType {
        self.inner.cpu_type()
    }
    fn cr0(&self) -> u32 {
        self.inner.cr0()
    }
    fn cr3(&self) -> u32 {
        self.inner.cr3()
    }
    fn load_segment_real_mode(&mut self, seg: SegmentRegister, selector: u16) {
        self.inner.load_segment_real_mode(seg, selector)
    }
    fn segment_base(&self, seg: SegmentRegister) -> u32 {
        self.inner.segment_base(seg)
    }
}
