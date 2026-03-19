//! Implements the NEC V30 (μPD70116) emulation.
//!
//! Following references were used to write the emulator:
//!
//! - NEC Electronics Inc., "V20/V30 Users Manual", October 1986.
//! - MAME NEC V20/V30/V33 emulator (`devices/cpu/nec/`), by Bryan McPhail,
//!   Oliver Bergmann, Fabrice Frances, and David Hedley.

mod alu;
mod execute;
mod execute_0f;
mod execute_group;
mod flags;
mod interrupt;
mod modrm;
mod rep;
mod state;
mod string_ops;

use std::ops::{Deref, DerefMut};

use common::Cpu as _;
pub use flags::V30Flags;
pub use state::V30State;

use crate::{SegReg16, WordReg};

/// NEC V30 (µPD70116) CPU emulator.
pub struct V30 {
    /// Embedded state for save/restore.
    pub state: V30State,

    prev_ip: u16,
    seg_prefix: bool,
    prefix_seg: SegReg16,

    halted: bool,
    pending_irq: u8,
    no_interrupt: u8,
    inhibit_all: u8,

    rep_ip: u16,
    rep_restart_ip: u16,
    rep_seg_prefix: bool,
    rep_prefix_seg: SegReg16,
    rep_opcode: u8,
    rep_type: u8,
    rep_active: bool,

    cycles_remaining: i64,
    run_start_cycle: u64,
    run_budget: u64,

    ea: u32,
    eo: u16,
}

impl Deref for V30 {
    type Target = V30State;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for V30 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for V30 {
    fn default() -> Self {
        Self::new()
    }
}

impl V30 {
    /// Creates a new V30 CPU in its reset state.
    pub fn new() -> Self {
        let mut cpu = Self {
            state: V30State::default(),
            prev_ip: 0,
            seg_prefix: false,
            prefix_seg: SegReg16::DS,
            halted: false,
            pending_irq: 0,
            no_interrupt: 0,
            inhibit_all: 0,
            rep_ip: 0,
            rep_restart_ip: 0,
            rep_seg_prefix: false,
            rep_prefix_seg: SegReg16::DS,
            rep_opcode: 0,
            rep_type: 0,
            rep_active: false,
            cycles_remaining: 0,
            run_start_cycle: 0,
            run_budget: 0,
            ea: 0,
            eo: 0,
        };
        cpu.reset();
        cpu
    }

    #[inline(always)]
    fn clk(&mut self, cycles: i32) {
        self.cycles_remaining -= cycles as i64;
    }

    #[inline(always)]
    fn clk_modrm(&mut self, modrm: u8, reg_cycles: i32, mem_cycles: i32) {
        if modrm >= 0xC0 {
            self.clk(reg_cycles);
        } else {
            self.clk(mem_cycles);
        }
    }

    #[inline(always)]
    fn clk_modrm_word(&mut self, modrm: u8, reg_cycles: i32, mem_cycles: i32, word_accesses: i32) {
        if modrm >= 0xC0 {
            self.clk(reg_cycles);
        } else {
            let penalty = if self.ea & 1 == 1 {
                4 * word_accesses
            } else {
                0
            };
            self.clk(mem_cycles + penalty);
        }
    }

    #[inline(always)]
    fn sp_penalty(&self, word_accesses: i32) -> i32 {
        if self.regs.word(WordReg::SP) & 1 == 1 {
            4 * word_accesses
        } else {
            0
        }
    }

    #[inline(always)]
    fn fetch(&mut self, bus: &mut impl common::Bus) -> u8 {
        let addr = ((self.sregs[SegReg16::CS as usize] as u32) << 4).wrapping_add(self.ip as u32)
            & 0xFFFFF;
        self.ip = self.ip.wrapping_add(1);
        bus.read_byte(addr)
    }

    #[inline(always)]
    fn fetchword(&mut self, bus: &mut impl common::Bus) -> u16 {
        let low = self.fetch(bus) as u16;
        let high = self.fetch(bus) as u16;
        low | (high << 8)
    }

    #[inline(always)]
    fn default_base(&self, seg: SegReg16) -> u32 {
        if self.seg_prefix && matches!(seg, SegReg16::DS | SegReg16::SS) {
            (self.sregs[self.prefix_seg as usize] as u32) << 4
        } else {
            (self.sregs[seg as usize] as u32) << 4
        }
    }

    #[inline(always)]
    fn seg_base(&self, seg: SegReg16) -> u32 {
        (self.sregs[seg as usize] as u32) << 4
    }

    /// Computes the physical address for a byte at `eo + delta`, wrapping
    /// the offset within the 16-bit segment boundary.
    #[inline(always)]
    fn seg_addr(&self, delta: u16) -> u32 {
        let seg_base = self.ea.wrapping_sub(self.eo as u32);
        seg_base.wrapping_add(self.eo.wrapping_add(delta) as u32) & 0xFFFFF
    }

    /// Reads a word from memory at `ea + delta`, wrapping the offset
    /// within the segment boundary (offset 0xFFFF wraps to 0x0000).
    #[inline(always)]
    fn seg_read_word_at(&self, bus: &mut impl common::Bus, delta: u16) -> u16 {
        let low = bus.read_byte(self.seg_addr(delta)) as u16;
        let high = bus.read_byte(self.seg_addr(delta.wrapping_add(1))) as u16;
        low | (high << 8)
    }

    /// Reads a word from memory at the current EA, wrapping the offset
    /// within the segment boundary (offset 0xFFFF wraps to 0x0000).
    #[inline(always)]
    fn seg_read_word(&self, bus: &mut impl common::Bus) -> u16 {
        self.seg_read_word_at(bus, 0)
    }

    /// Writes a word to memory at the current EA, wrapping the offset
    /// within the segment boundary (offset 0xFFFF wraps to 0x0000).
    #[inline(always)]
    fn seg_write_word(&self, bus: &mut impl common::Bus, value: u16) {
        bus.write_byte(self.ea, value as u8);
        bus.write_byte(self.seg_addr(1), (value >> 8) as u8);
    }

    /// Reads a word from `base:offset`, wrapping the offset within 16 bits.
    #[inline(always)]
    fn read_word_seg(&self, bus: &mut impl common::Bus, base: u32, offset: u16) -> u16 {
        let lo = bus.read_byte(base.wrapping_add(offset as u32) & 0xFFFFF) as u16;
        let hi = bus.read_byte(base.wrapping_add(offset.wrapping_add(1) as u32) & 0xFFFFF) as u16;
        lo | (hi << 8)
    }

    /// Writes a word to `base:offset`, wrapping the offset within 16 bits.
    #[inline(always)]
    fn write_word_seg(&self, bus: &mut impl common::Bus, base: u32, offset: u16, value: u16) {
        bus.write_byte(base.wrapping_add(offset as u32) & 0xFFFFF, value as u8);
        bus.write_byte(
            base.wrapping_add(offset.wrapping_add(1) as u32) & 0xFFFFF,
            (value >> 8) as u8,
        );
    }

    fn push(&mut self, bus: &mut impl common::Bus, value: u16) {
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        self.regs.set_word(WordReg::SP, sp);
        let base = self.seg_base(SegReg16::SS);
        bus.write_byte(base.wrapping_add(sp as u32) & 0xFFFFF, value as u8);
        bus.write_byte(
            base.wrapping_add(sp.wrapping_add(1) as u32) & 0xFFFFF,
            (value >> 8) as u8,
        );
    }

    fn pop(&mut self, bus: &mut impl common::Bus) -> u16 {
        let sp = self.regs.word(WordReg::SP);
        let base = self.seg_base(SegReg16::SS);
        let low = bus.read_byte(base.wrapping_add(sp as u32) & 0xFFFFF) as u16;
        let high = bus.read_byte(base.wrapping_add(sp.wrapping_add(1) as u32) & 0xFFFFF) as u16;
        self.regs.set_word(WordReg::SP, sp.wrapping_add(2));
        low | (high << 8)
    }

    fn execute_one(&mut self, bus: &mut impl common::Bus) {
        self.prev_ip = self.ip;

        if self.pending_irq != 0 {
            self.check_interrupts(bus);
        }
        if self.no_interrupt > 0 {
            self.no_interrupt -= 1;
        }
        if self.inhibit_all > 0 {
            self.inhibit_all -= 1;
        }

        self.seg_prefix = false;

        if self.rep_active {
            self.continue_rep(bus);
        } else {
            let mut opcode = self.fetch(bus);
            loop {
                match opcode {
                    0x26 => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::ES;
                        self.clk(2);
                        opcode = self.fetch(bus);
                    }
                    0x2E => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::CS;
                        self.clk(2);
                        opcode = self.fetch(bus);
                    }
                    0x36 => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::SS;
                        self.clk(2);
                        opcode = self.fetch(bus);
                    }
                    0x3E => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::DS;
                        self.clk(2);
                        opcode = self.fetch(bus);
                    }
                    0xF0 => {
                        self.inhibit_all = 1;
                        self.clk(2);
                        opcode = self.fetch(bus);
                    }
                    _ => {
                        self.dispatch(opcode, bus);
                        break;
                    }
                }
            }
        }
    }

    /// Executes exactly one logical instruction (should only be used in tests).
    ///
    /// Sets `cycles_remaining` to `i64::MAX` so that REP-prefixed string
    /// instructions run to completion in a single call instead of pausing
    /// mid-loop when the cycle budget runs out. This is necessary because
    /// `execute_one` resumes a paused REP via `continue_rep`, which would
    /// otherwise split a single logical instruction across multiple calls.
    pub fn step(&mut self, bus: &mut impl common::Bus) {
        self.cycles_remaining = i64::MAX;
        self.execute_one(bus);
    }

    /// Returns the number of cycles consumed by the last `step()` call.
    pub fn cycles_consumed(&self) -> u64 {
        (i64::MAX - self.cycles_remaining) as u64
    }

    /// Returns the last computed effective address (for alignment checks).
    pub fn last_ea(&self) -> u32 {
        self.ea
    }

    /// Signals a maskable interrupt request (IRQ).
    pub fn signal_irq(&mut self) {
        self.pending_irq |= crate::PENDING_IRQ;
    }

    /// Signals a non-maskable interrupt (NMI).
    pub fn signal_nmi(&mut self) {
        self.pending_irq |= crate::PENDING_NMI;
    }
}

impl common::Cpu for V30 {
    crate::impl_cpu_run_for!();

    fn reset(&mut self) {
        self.state = V30State::default();
        self.sregs[SegReg16::CS as usize] = 0xFFFF;
        self.prev_ip = 0;
        self.halted = false;
        self.pending_irq = 0;
        self.no_interrupt = 0;
        self.inhibit_all = 0;
        self.rep_active = false;
        self.rep_restart_ip = 0;
        self.seg_prefix = false;
        self.ea = 0;
        self.eo = 0;
    }

    fn halted(&self) -> bool {
        self.halted
    }

    fn ax(&self) -> u16 {
        self.state.ax()
    }

    fn set_ax(&mut self, v: u16) {
        self.state.set_ax(v);
    }

    fn bx(&self) -> u16 {
        self.state.bx()
    }

    fn set_bx(&mut self, v: u16) {
        self.state.set_bx(v);
    }

    fn cx(&self) -> u16 {
        self.state.cx()
    }

    fn set_cx(&mut self, v: u16) {
        self.state.set_cx(v);
    }

    fn dx(&self) -> u16 {
        self.state.dx()
    }

    fn set_dx(&mut self, v: u16) {
        self.state.set_dx(v);
    }

    fn sp(&self) -> u16 {
        self.state.sp()
    }

    fn set_sp(&mut self, v: u16) {
        self.state.set_sp(v);
    }

    fn bp(&self) -> u16 {
        self.state.bp()
    }

    fn set_bp(&mut self, v: u16) {
        self.state.set_bp(v);
    }

    fn si(&self) -> u16 {
        self.state.si()
    }

    fn set_si(&mut self, v: u16) {
        self.state.set_si(v);
    }

    fn di(&self) -> u16 {
        self.state.di()
    }

    fn set_di(&mut self, v: u16) {
        self.state.set_di(v);
    }

    fn es(&self) -> u16 {
        self.state.es()
    }

    fn set_es(&mut self, v: u16) {
        self.state.set_es(v);
    }

    fn cs(&self) -> u16 {
        self.state.cs()
    }

    fn set_cs(&mut self, v: u16) {
        self.state.set_cs(v);
    }

    fn ss(&self) -> u16 {
        self.state.ss()
    }

    fn set_ss(&mut self, v: u16) {
        self.state.set_ss(v);
    }

    fn ds(&self) -> u16 {
        self.state.ds()
    }

    fn set_ds(&mut self, v: u16) {
        self.state.set_ds(v);
    }

    fn ip(&self) -> u16 {
        self.state.ip
    }

    fn set_ip(&mut self, v: u16) {
        self.state.ip = v;
    }

    fn flags(&self) -> u16 {
        self.state.compressed_flags()
    }

    fn set_flags(&mut self, v: u16) {
        self.state.set_compressed_flags(v);
    }

    fn cpu_type(&self) -> common::CpuType {
        common::CpuType::V30
    }
}
