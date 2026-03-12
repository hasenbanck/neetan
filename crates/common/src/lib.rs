//! Core library for commonly used functionality and traits.
//!
//! Defines the fundamental [`Bus`] and [`Cpu`] traits that all PC-98 machine
//! model implementations must satisfy. The traits are designed for static
//! dispatch: each concrete machine model wires its specific CPU and bus types
//! together at compile time.

#![warn(missing_docs)]
#![deny(unsafe_code)]

mod display_snapshot;
pub mod error;
mod jis;
pub mod log;
mod stack_vec;

pub use display_snapshot::{DisplaySnapshotUpload, cast_u32_slice_as_bytes_mut};
pub use error::{Context, ContextError, OptionContext, StringError};
pub use jis::{JisChar, char_to_jis, jis_slice_to_string, jis_to_char, str_to_jis};
pub use stack_vec::StackVec;

/// CPU generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuType {
    /// NEC V30 (µPD70116).
    V30,
    /// Intel 80286.
    I286,
    /// Intel 80386.
    I386,
}

/// Number of [`EventKind`] variants.
const EVENT_KIND_COUNT: usize = 13;

/// Trait representing the system bus of an emulated machine.
///
/// The bus is the single point of contact between the CPU and every other
/// subsystem: RAM, VRAM, ROM, and I/O peripherals. All memory and port
/// accesses flow through this trait, allowing the concrete bus implementation
/// to dispatch reads and writes to the appropriate backing store or device
/// handler.
///
/// # Address widths
///
/// Memory addresses are 32 bits wide. Concrete implementations apply the
/// appropriate mask for the emulated CPU generation:
///
/// - V30 / 8086: 20-bit (1 MB address space)
/// - i286: 24-bit (16 MB address space)
/// - i386+: full 32-bit (4 GB address space)
///
/// I/O port addresses are 16 bits wide across all generations.
///
/// # Word access
///
/// The default implementations of multibyte reads and writes compose of
/// individual byte operations in little-endian order. This is correct for
/// memory-mapped I/O and VRAM, where each byte access may trigger distinct
/// side effects. Concrete implementations should override these methods with
/// fast paths for contiguous RAM regions where no per-byte dispatch is needed.
///
/// # Interrupt polling
///
/// The bus exposes interrupt readiness through [`has_irq`](Bus::has_irq) and
/// [`has_nmi`](Bus::has_nmi). The CPU polls these after each instruction and
/// initiates an acknowledgment cycle when appropriate. This models the real
/// hardware flow (CPU checks INTR/NMI pins, then performs an INTA bus cycle)
/// and avoids circular ownership between the CPU and bus.
///
/// # Cycle tracking
///
/// The bus maintains a CPU cycle counter, updated by the CPU after each
/// instruction via [`set_current_cycle`](Bus::set_current_cycle). Peripheral
/// handlers use [`current_cycle`](Bus::current_cycle) for lazy state
/// evaluation — computing elapsed time on access rather than updating on
/// every cycle.
pub trait Bus {
    /// Reads a single byte from the given memory address.
    fn read_byte(&mut self, address: u32) -> u8;

    /// Writes a single byte to the given memory address.
    fn write_byte(&mut self, address: u32, value: u8);

    /// Reads a 16-bit little-endian word from the given memory address.
    ///
    /// The default implementation composes two byte reads. Override this for
    /// fast-path RAM access where the address is known to fall within a
    /// contiguous region.
    fn read_word(&mut self, address: u32) -> u16 {
        let low = self.read_byte(address) as u16;
        let high = self.read_byte(address.wrapping_add(1)) as u16;
        low | (high << 8)
    }

    /// Writes a 16-bit little-endian word to the given memory address.
    ///
    /// The default implementation composes two byte writes. Override this for
    /// fast-path RAM access where the address is known to fall within a
    /// contiguous region.
    fn write_word(&mut self, address: u32, value: u16) {
        self.write_byte(address, value as u8);
        self.write_byte(address.wrapping_add(1), (value >> 8) as u8);
    }

    /// Reads a single byte from the given I/O port.
    fn io_read_byte(&mut self, port: u16) -> u8;

    /// Writes a single byte to the given I/O port.
    fn io_write_byte(&mut self, port: u16, value: u8);

    /// Reads a 16-bit little-endian word from the given I/O port.
    ///
    /// The default implementation composes two byte reads from consecutive
    /// port addresses. Some peripherals treat word-wide port access differently
    /// from two byte accesses; override this method for those cases.
    fn io_read_word(&mut self, port: u16) -> u16 {
        let low = self.io_read_byte(port) as u16;
        let high = self.io_read_byte(port.wrapping_add(1)) as u16;
        low | (high << 8)
    }

    /// Writes a 16-bit little-endian word to the given I/O port.
    ///
    /// The default implementation composes two byte writes to consecutive
    /// port addresses.
    fn io_write_word(&mut self, port: u16, value: u16) {
        self.io_write_byte(port, value as u8);
        self.io_write_byte(port.wrapping_add(1), (value >> 8) as u8);
    }

    /// Returns `true` if a maskable hardware interrupt is pending.
    ///
    /// The CPU calls this after each instruction when the interrupt flag (IF)
    /// is set. If this returns `true`, the CPU will call
    /// [`acknowledge_irq`](Bus::acknowledge_irq) to obtain the interrupt
    /// vector, modeling the real INTA bus cycle.
    fn has_irq(&self) -> bool;

    /// Acknowledges a pending maskable interrupt and returns its vector number.
    ///
    /// This models the INTA bus cycle: the PIC resolves the highest-priority
    /// unmasked interrupt, marks it in-service, and returns its programmed
    /// vector number. The CPU then uses this vector to index the interrupt
    /// vector table.
    ///
    /// Must only be called when [`has_irq`](Bus::has_irq) returns `true`.
    fn acknowledge_irq(&mut self) -> u8;

    /// Returns `true` if a non-maskable interrupt is pending.
    ///
    /// NMIs are edge-triggered and cannot be masked by the CPU's IF flag.
    /// The CPU checks this after each instruction unconditionally.
    fn has_nmi(&self) -> bool;

    /// Acknowledges a pending non-maskable interrupt.
    ///
    /// Clears the non-maskable interrupt (NMI) pending state.
    /// The CPU vectors through interrupt vector 2 after calling this.
    fn acknowledge_nmi(&mut self);

    /// Returns the current CPU cycle count.
    ///
    /// The value represents the number of CPU cycles elapsed since the
    /// start of emulation. It is updated by the CPU after each
    /// instruction via [`set_current_cycle`](Bus::set_current_cycle),
    /// ensuring that I/O port handlers and other peripheral logic see
    /// a cycle-accurate timestamp when invoked during execution.
    ///
    /// Peripherals use this for lazy state evaluation: rather than
    /// updating internal state on every cycle, a peripheral records
    /// the cycle count at its last access and, when next accessed,
    /// fast-forwards its state by the elapsed delta.
    fn current_cycle(&self) -> u64;

    /// Sets the current CPU cycle count.
    ///
    /// The CPU calls this after executing each instruction to keep the
    /// bus's cycle counter synchronized with the CPU's own cycle
    /// accounting. This ensures that any I/O port access or
    /// memory-mapped peripheral triggered during instruction execution
    /// observes the correct timestamp for lazy state evaluation.
    fn set_current_cycle(&mut self, cycle: u64);

    /// Drains accumulated memory wait-state cycles.
    ///
    /// Some memory accesses (e.g. GRCG VRAM operations) impose additional
    /// wait-state penalties beyond the instruction's base cycle count.
    /// The bus accumulates these penalties during memory reads and writes,
    /// and the CPU drains them after each instruction to include the
    /// penalty in the cycle accounting.
    ///
    /// Returns the number of accumulated wait cycles and resets the
    /// internal counter to zero.
    fn drain_wait_cycles(&mut self) -> i64 {
        0
    }

    /// Returns `true` if a CPU reset has been requested by hardware.
    fn reset_pending(&self) -> bool {
        false
    }

    /// Returns `true` if the bus requests the CPU to yield execution.
    ///
    /// Certain HLE (High-Level Emulation) traps need access to CPU register
    /// state that is not available through `io_write_byte`. When this returns
    /// `true`, the CPU breaks out of its execution loop so the machine
    /// loop can service the request with full CPU + bus access.
    fn cpu_should_yield(&self) -> bool {
        false
    }
}

/// Trait representing an emulated CPU.
///
/// Each CPU generation (V30, i286, i386, i486) provides its own implementation
/// of this trait. The CPU is parameterized over a concrete [`Bus`] type through
/// the `run_for` method's generic parameter, enabling static dispatch without
/// requiring the trait itself to carry a type parameter.
///
/// # Execution model
///
/// The CPU executes one instruction at a time inside [`run_for`](Cpu::run_for).
/// After each instruction, the CPU checks the bus for pending interrupts
/// (NMI unconditionally, IRQ when IF is set) and services them before
/// continuing. The method returns when the cycle budget is exhausted or a
/// halt condition is reached.
///
/// # Halt state
///
/// When the CPU executes a HLT instruction, it enters a halted state where
/// no further instructions execute until an interrupt arrives. The
/// [`run_for`](Cpu::run_for) method returns early when halted, reporting the
/// cycles consumed up to and including the HLT. The scheduler can then
/// advance time directly to the next event rather than spinning.
/// [`halted`](Cpu::halted) lets the scheduler query this state.
pub trait Cpu {
    /// Executes instructions until approximately `cycles_to_run` CPU cycles
    /// have been consumed, then returns the actual number of cycles consumed.
    ///
    /// The returned cycle count may exceed `cycles_to_run` because the CPU
    /// finishes the current instruction before checking the budget. It may
    /// also be less than `cycles_to_run` if the CPU enters a halted state.
    ///
    /// The bus is passed by mutable reference for the duration of execution.
    /// All memory reads, I/O port accesses, and interrupt polling flow
    /// through the bus.
    fn run_for(&mut self, cycles_to_run: u64, bus: &mut impl Bus) -> u64;

    /// Resets the CPU to its initial power-on state.
    ///
    /// After reset, the CPU begins execution at the architecture-defined
    /// reset vector (FFFF:0000 for real-mode x86 processors). All registers
    /// are set to their documented power-on values. Any pending interrupt
    /// or halt state is cleared.
    fn reset(&mut self);

    /// Returns `true` if the CPU is in a halted state.
    ///
    /// The CPU enters this state when it executes a HLT instruction and
    /// leaves it when an interrupt (NMI or unmasked IRQ) is received. The
    /// scheduler uses this to skip ahead to the next scheduled event
    /// instead of calling [`run_for`](Cpu::run_for) in a tight loop.
    fn halted(&self) -> bool;

    /// Performs a warm reset for returning from protected mode to real mode.
    ///
    /// On 286+ CPUs, this clears protected mode and sets the CPU to resume
    /// execution at `cs:ip` with `ss:sp`, emulating the ITF ROM's warm reset
    /// sequence (`SS ← [0:406], SP ← [0:404], RETF`).
    ///
    /// The default implementation falls back to a cold reset.
    fn warm_reset(&mut self, _ss: u16, _sp: u16, _cs: u16, _ip: u16) {
        self.reset();
    }

    /// Returns the AX register.
    fn ax(&self) -> u16;

    /// Sets the AX register.
    fn set_ax(&mut self, v: u16);

    /// Returns the BX register.
    fn bx(&self) -> u16;

    /// Sets the BX register.
    fn set_bx(&mut self, v: u16);

    /// Returns the CX register.
    fn cx(&self) -> u16;

    /// Sets the CX register.
    fn set_cx(&mut self, v: u16);

    /// Returns the DX register.
    fn dx(&self) -> u16;

    /// Sets the DX register.
    fn set_dx(&mut self, v: u16);

    /// Returns the current stack pointer (low 16 bits).
    fn sp(&self) -> u16;

    /// Sets the stack pointer (low 16 bits).
    fn set_sp(&mut self, v: u16);

    /// Returns the BP register.
    fn bp(&self) -> u16;

    /// Sets the BP register.
    fn set_bp(&mut self, v: u16);

    /// Returns the SI register.
    fn si(&self) -> u16;

    /// Sets the SI register.
    fn set_si(&mut self, v: u16);

    /// Returns the DI register.
    fn di(&self) -> u16;

    /// Sets the DI register.
    fn set_di(&mut self, v: u16);

    /// Returns the ES segment register.
    fn es(&self) -> u16;

    /// Sets the ES segment register.
    fn set_es(&mut self, v: u16);

    /// Returns the CS segment register.
    fn cs(&self) -> u16;

    /// Sets the CS segment register.
    fn set_cs(&mut self, v: u16);

    /// Returns the current stack segment register.
    fn ss(&self) -> u16;

    /// Sets the SS segment register.
    fn set_ss(&mut self, v: u16);

    /// Returns the DS segment register.
    fn ds(&self) -> u16;

    /// Sets the DS segment register.
    fn set_ds(&mut self, v: u16);

    /// Returns the instruction pointer.
    fn ip(&self) -> u16;

    /// Sets the instruction pointer.
    fn set_ip(&mut self, v: u16);

    /// Returns the FLAGS register (16-bit).
    fn flags(&self) -> u16;

    /// Sets the FLAGS register (16-bit).
    fn set_flags(&mut self, v: u16);

    /// Returns the CPU generation.
    fn cpu_type(&self) -> CpuType;

    /// Returns CR0 (control register 0). Only meaningful for 386+.
    fn cr0(&self) -> u32 {
        0
    }

    /// Returns CR3 (page directory base register). Only meaningful for 386+.
    fn cr3(&self) -> u32 {
        0
    }

    /// Returns the high byte of AX.
    #[inline]
    fn ah(&self) -> u8 {
        (self.ax() >> 8) as u8
    }

    /// Sets the high byte of AX, preserving the low byte.
    #[inline]
    fn set_ah(&mut self, v: u8) {
        self.set_ax((self.ax() & 0x00FF) | (u16::from(v) << 8));
    }

    /// Returns the low byte of AX.
    #[inline]
    fn al(&self) -> u8 {
        self.ax() as u8
    }

    /// Sets the low byte of AX, preserving the high byte.
    #[inline]
    fn set_al(&mut self, v: u8) {
        self.set_ax((self.ax() & 0xFF00) | u16::from(v));
    }

    /// Returns the high byte of BX.
    #[inline]
    fn bh(&self) -> u8 {
        (self.bx() >> 8) as u8
    }

    /// Sets the high byte of BX, preserving the low byte.
    #[inline]
    fn set_bh(&mut self, v: u8) {
        self.set_bx((self.bx() & 0x00FF) | (u16::from(v) << 8));
    }

    /// Returns the low byte of BX.
    #[inline]
    fn bl(&self) -> u8 {
        self.bx() as u8
    }

    /// Sets the low byte of BX, preserving the high byte.
    #[inline]
    fn set_bl(&mut self, v: u8) {
        self.set_bx((self.bx() & 0xFF00) | u16::from(v));
    }

    /// Returns the high byte of CX.
    #[inline]
    fn ch(&self) -> u8 {
        (self.cx() >> 8) as u8
    }

    /// Sets the high byte of CX, preserving the low byte.
    #[inline]
    fn set_ch(&mut self, v: u8) {
        self.set_cx((self.cx() & 0x00FF) | (u16::from(v) << 8));
    }

    /// Returns the low byte of CX.
    #[inline]
    fn cl(&self) -> u8 {
        self.cx() as u8
    }

    /// Sets the low byte of CX, preserving the high byte.
    #[inline]
    fn set_cl(&mut self, v: u8) {
        self.set_cx((self.cx() & 0xFF00) | u16::from(v));
    }

    /// Returns the high byte of DX.
    #[inline]
    fn dh(&self) -> u8 {
        (self.dx() >> 8) as u8
    }

    /// Sets the high byte of DX, preserving the low byte.
    #[inline]
    fn set_dh(&mut self, v: u8) {
        self.set_dx((self.dx() & 0x00FF) | (u16::from(v) << 8));
    }

    /// Returns the low byte of DX.
    #[inline]
    fn dl(&self) -> u8 {
        self.dx() as u8
    }

    /// Sets the low byte of DX, preserving the high byte.
    #[inline]
    fn set_dl(&mut self, v: u8) {
        self.set_dx((self.dx() & 0xFF00) | u16::from(v));
    }
}

/// Abstract machine that can be stepped by a host loop.
pub trait Machine {
    /// Returns the CPU clock frequency in Hz.
    fn cpu_clock_hz(&self) -> f64;

    /// Runs the machine for up to `budget` CPU cycles, returning cycles consumed.
    fn run_for(&mut self, budget: u64) -> u64;

    /// Returns a reference to the display snapshot captured at the last VSYNC.
    fn snapshot_display(&self) -> &DisplaySnapshotUpload;

    /// Injects a PC-98 keyboard scan code.
    fn push_keyboard_scancode(&mut self, code: u8);

    /// Injects mouse movement deltas for the current frame.
    ///
    /// `dx`/`dy` are relative pixel deltas from the host.
    /// Called once per frame before [`run_for`](Machine::run_for).
    fn push_mouse_delta(&mut self, dx: i16, dy: i16);

    /// Updates mouse button state.
    ///
    /// Each parameter: `true` = pressed, `false` = released.
    fn set_mouse_buttons(&mut self, left: bool, right: bool, middle: bool);

    /// Fills `output` with interleaved stereo audio samples (`[L, R, L, R, …]`)
    /// for the current frame, returning the number of `f32` values written
    /// (i.e. `frames × 2`).
    ///
    /// Called once per display frame after [`run_for`](Machine::run_for).
    /// The machine generates samples covering the cycles executed since the
    /// last call, at the given `volume` (0.0–1.0).
    fn generate_audio_samples(&mut self, volume: f32, output: &mut [f32]) -> usize;

    /// Returns `true` if the font ROM was modified since the last call, and clears the flag.
    fn take_font_rom_dirty(&mut self) -> bool;

    /// Returns the font ROM data for GPU upload.
    fn font_rom_data(&self) -> &[u8];

    /// Inserts a floppy disk image into the specified drive (0-based).
    /// Reads the file, auto-detects format, and inserts. Returns a description string on success.
    fn insert_floppy(&mut self, drive: usize, path: &std::path::Path) -> Result<String, String>;

    /// Ejects the floppy disk from the specified drive, flushing any dirty data first.
    fn eject_floppy(&mut self, drive: usize);

    /// Flushes any dirty floppy disk images to their backing files.
    fn flush_floppies(&mut self);

    /// Flushes any dirty hard disk images to their backing files.
    fn flush_hdds(&mut self);

    /// Flushes the printer output file, if attached.
    fn flush_printer(&mut self);
}

/// Kinds of scheduled events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EventKind {
    /// PIT channel 0 reached terminal count.
    #[default]
    PitTimer0,
    /// GDC vertical sync begins (VSYNC blanking interval starts).
    ///
    /// During VSYNC, VRAM/GRCG access wait-state penalties are reduced
    /// because the display controller is not actively scanning the frame
    /// buffer — there is no bus contention between CPU and GDC.
    GdcVsync,
    /// GDC active display period begins (VSYNC blanking interval ends).
    ///
    /// Marks the transition from VSYNC back to the active display period,
    /// restoring full VRAM/GRCG wait-state penalties. Together with
    /// `GdcVsync`, these two events alternate each frame to model the
    /// display/blanking timing split.
    GdcDisplayStart,
    /// FDC execution phase complete (data ready for transfer).
    FdcExecution,
    /// FDC interrupt (raise IRQ after seek/data transfer).
    FdcInterrupt,
    /// GDC slave drawing operation complete (clear DRAWING flag).
    GdcDrawingComplete,
    /// Mouse interface timer tick (raises IRQ13 / INT 15h when unmasked).
    MouseTimer,
    /// YM2203 Timer A overflow.
    FmTimerA,
    /// YM2203 Timer B overflow.
    FmTimerB,
    /// YM2203 Timer A overflow (second board, dual-board config).
    FmTimer2A,
    /// YM2203 Timer B overflow (second board, dual-board config).
    FmTimer2B,
    /// SASI controller execution complete (data ready or command finished).
    SasiExecution,
    /// SASI controller interrupt (raise IRQ 9 after operation).
    SasiInterrupt,
}

impl EventKind {
    const ALL: [EventKind; EVENT_KIND_COUNT] = [
        EventKind::PitTimer0,
        EventKind::GdcVsync,
        EventKind::GdcDisplayStart,
        EventKind::FdcExecution,
        EventKind::FdcInterrupt,
        EventKind::GdcDrawingComplete,
        EventKind::MouseTimer,
        EventKind::FmTimerA,
        EventKind::FmTimerB,
        EventKind::FmTimer2A,
        EventKind::FmTimer2B,
        EventKind::SasiExecution,
        EventKind::SasiInterrupt,
    ];

    const fn from_index(index: usize) -> Self {
        Self::ALL[index]
    }
}

/// Snapshot of a single scheduled event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScheduledEvent {
    /// CPU cycle at which this event fires.
    pub fire_cycle: u64,
    /// The event type.
    pub kind: EventKind,
}

/// Snapshot of the scheduler's pending event queue.
///
/// Uses a flat array indexed by [`EventKind`] discriminant. Each slot holds
/// `Some(fire_cycle)` when an event of that kind is scheduled, or `None`
/// when it is not. At most one event per kind can be active at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerState {
    /// Fire cycle for each event kind, indexed by discriminant.
    pub fire_cycles: [Option<u64>; EVENT_KIND_COUNT],
}

/// Event-driven scheduler for timed peripheral events.
///
/// Internally stores at most one pending event per [`EventKind`] in a flat
/// array, giving O(1) schedule/cancel and O(N) minimum-scan where N is the
/// small, fixed number of event kinds.
pub struct Scheduler {
    /// Embedded state for save/restore.
    pub state: SchedulerState,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    /// Creates a new empty scheduler.
    pub fn new() -> Self {
        Self {
            state: SchedulerState {
                fire_cycles: [None; EVENT_KIND_COUNT],
            },
        }
    }

    /// Schedules an event to fire at `fire_cycle`. Replaces any existing
    /// event of the same kind.
    pub fn schedule(&mut self, kind: EventKind, fire_cycle: u64) {
        self.state.fire_cycles[kind as usize] = Some(fire_cycle);
    }

    /// Cancels any pending event of the given kind.
    pub fn cancel(&mut self, kind: EventKind) {
        self.state.fire_cycles[kind as usize] = None;
    }

    /// Returns the cycle of the earliest scheduled event, if any.
    pub fn next_event_cycle(&self) -> Option<u64> {
        self.state.fire_cycles.iter().filter_map(|&c| c).min()
    }

    /// Removes and returns all events due at or before `current_cycle`.
    pub fn pop_due_events(
        &mut self,
        current_cycle: u64,
    ) -> StackVec<ScheduledEvent, EVENT_KIND_COUNT> {
        let mut due = StackVec::new();
        for (index, slot) in self.state.fire_cycles.iter_mut().enumerate() {
            if let Some(fire_cycle) = *slot
                && fire_cycle <= current_cycle
            {
                due.push(ScheduledEvent {
                    fire_cycle,
                    kind: EventKind::from_index(index),
                });
                *slot = None;
            }
        }
        due.sort_by_key(|e: &ScheduledEvent| e.fire_cycle);
        due
    }
}
