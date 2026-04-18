//! Implements the CPU emulation.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod i286;
mod i386;
mod v30;

pub use i286::{I286, I286Flags, I286State};
pub use i386::{
    CPU_MODEL_386, CPU_MODEL_486, I386, I386Flags, I386State, TLB_MASK, TLB_SIZE, TlbCache,
};
pub use v30::{V30, V30Flags, V30State};

pub(crate) const PENDING_IRQ: u8 = 0x01;
pub(crate) const PENDING_NMI: u8 = 0x02;

macro_rules! impl_cpu_run_for {
    () => {
        fn run_for(&mut self, cycles_to_run: u64, bus: &mut impl common::Bus) -> u64 {
            let start_cycle = bus.current_cycle();
            self.run_start_cycle = start_cycle;
            self.run_budget = cycles_to_run;
            self.cycles_remaining = cycles_to_run as i64;

            while self.cycles_remaining > 0 {
                if self.halted {
                    // Poll the bus for interrupts that may wake the CPU from HLT.
                    // NMI always wakes; IRQ only wakes when IF is set.
                    if bus.has_nmi() {
                        self.pending_irq |= $crate::PENDING_NMI;
                    }
                    if self.flags.if_flag {
                        if bus.has_irq() {
                            self.pending_irq |= $crate::PENDING_IRQ;
                        }
                    } else {
                        // IF=0: maskable interrupts cannot wake the CPU.
                        // Clear any stale PENDING_IRQ from before HLT.
                        self.pending_irq &= !$crate::PENDING_IRQ;
                    }
                    if self.pending_irq != 0 {
                        self.halted = false;
                    } else {
                        let consumed = (cycles_to_run as i64 - self.cycles_remaining) as u64;
                        bus.set_current_cycle(start_cycle + consumed);
                        return consumed;
                    }
                }

                self.execute_one(bus);
                self.cycles_remaining -= bus.drain_wait_cycles();

                let consumed = cycles_to_run as i64 - self.cycles_remaining;
                bus.set_current_cycle(start_cycle + consumed as u64);

                // Poll the bus for pending interrupts after the cycle update.
                // set_current_cycle may have triggered events (e.g. PIT timer)
                // that raised interrupt lines on the PIC.
                if bus.has_nmi() {
                    self.pending_irq |= $crate::PENDING_NMI;
                }
                if bus.has_irq() {
                    self.pending_irq |= $crate::PENDING_IRQ;
                } else {
                    self.pending_irq &= !$crate::PENDING_IRQ;
                }

                if bus.reset_pending() {
                    break;
                }
                if bus.cpu_should_yield() {
                    break;
                }
            }

            let actual = (cycles_to_run as i64 - self.cycles_remaining) as u64;
            bus.set_current_cycle(start_cycle + actual);
            actual
        }
    };
}

pub(crate) use impl_cpu_run_for;

/// 32-bit general-purpose registers.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwordReg {
    /// Extended accumulator.
    EAX = 0,
    /// Extended count.
    ECX = 1,
    /// Extended data.
    EDX = 2,
    /// Extended base.
    EBX = 3,
    /// Extended stack pointer.
    ESP = 4,
    /// Extended base pointer.
    EBP = 5,
    /// Extended source index.
    ESI = 6,
    /// Extended destination index.
    EDI = 7,
}

impl DwordReg {
    /// Returns the register for the given 3-bit index.
    pub const fn from_index(index: u8) -> Self {
        match index & 7 {
            0 => Self::EAX,
            1 => Self::ECX,
            2 => Self::EDX,
            3 => Self::EBX,
            4 => Self::ESP,
            5 => Self::EBP,
            6 => Self::ESI,
            7 => Self::EDI,
            _ => unreachable!(),
        }
    }
}

/// 16-bit general-purpose registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WordReg {
    /// Accumulator.
    AX = 0,
    /// Count.
    CX = 1,
    /// Data.
    DX = 2,
    /// Base.
    BX = 3,
    /// Stack pointer.
    SP = 4,
    /// Base pointer.
    BP = 5,
    /// Source index.
    SI = 6,
    /// Destination index.
    DI = 7,
}

impl WordReg {
    const fn from_index(index: u8) -> Self {
        match index & 7 {
            0 => Self::AX,
            1 => Self::CX,
            2 => Self::DX,
            3 => Self::BX,
            4 => Self::SP,
            5 => Self::BP,
            6 => Self::SI,
            7 => Self::DI,
            _ => unreachable!(),
        }
    }
}

/// 8-bit general-purpose registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ByteReg {
    /// Accumulator low byte.
    AL = 0,
    /// Count low byte.
    CL = 1,
    /// Data low byte.
    DL = 2,
    /// Base low byte.
    BL = 3,
    /// Accumulator high byte.
    AH = 4,
    /// Count high byte.
    CH = 5,
    /// Data high byte.
    DH = 6,
    /// Base high byte.
    BH = 7,
}

/// i386 segment registers (6 segments including FS/GS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SegReg32 {
    /// Extra segment.
    ES = 0,
    /// Code segment.
    CS = 1,
    /// Stack segment.
    SS = 2,
    /// Data segment.
    DS = 3,
    /// Additional data segment F.
    FS = 4,
    /// Additional data segment G.
    GS = 5,
}

impl SegReg32 {
    /// Returns the segment register for the given 3-bit index.
    pub const fn from_index(index: u8) -> Self {
        match index & 7 {
            0 => Self::ES,
            1 => Self::CS,
            2 => Self::SS,
            3 => Self::DS,
            4 => Self::FS,
            5 => Self::GS,
            _ => unreachable!(),
        }
    }
}

/// 8086/i286/V30 segment registers (4 segments).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SegReg16 {
    /// Extra segment.
    ES = 0,
    /// Code segment.
    CS = 1,
    /// Stack segment.
    SS = 2,
    /// Data segment.
    DS = 3,
}

impl SegReg16 {
    /// Returns the segment register for the given 2-bit index.
    pub const fn from_index(index: u8) -> Self {
        match index & 3 {
            0 => Self::ES,
            1 => Self::CS,
            2 => Self::SS,
            3 => Self::DS,
            _ => unreachable!(),
        }
    }
}

/// 32-bit register file holding eight general-purpose dword registers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct RegisterFile32 {
    d: [u32; 8],
}

impl Default for RegisterFile32 {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisterFile32 {
    /// Creates a new register file with all registers zeroed.
    pub const fn new() -> Self {
        Self { d: [0; 8] }
    }

    /// Reads a 32-bit register.
    #[inline(always)]
    pub const fn dword(&self, reg: DwordReg) -> u32 {
        self.d[reg as usize]
    }

    /// Writes a 32-bit register.
    #[inline(always)]
    pub const fn set_dword(&mut self, reg: DwordReg, value: u32) {
        self.d[reg as usize] = value;
    }

    /// Reads a 16-bit register (low word of a dword register).
    #[inline(always)]
    pub const fn word(&self, reg: WordReg) -> u16 {
        self.d[reg as usize] as u16
    }

    /// Writes a 16-bit register (low word of a dword register), preserving
    /// the upper half.
    #[inline(always)]
    pub const fn set_word(&mut self, reg: WordReg, value: u16) {
        let index = reg as usize;
        self.d[index] = (self.d[index] & 0xFFFF_0000) | value as u32;
    }

    /// Reads an 8-bit register (low or high byte of a word register).
    #[inline(always)]
    pub const fn byte(&self, reg: ByteReg) -> u8 {
        let idx = reg as u8;
        if idx < 4 {
            self.d[idx as usize] as u8
        } else {
            (self.d[(idx - 4) as usize] >> 8) as u8
        }
    }

    /// Writes an 8-bit register (low or high byte of a word register).
    #[inline(always)]
    pub const fn set_byte(&mut self, reg: ByteReg, value: u8) {
        let idx = reg as u8;
        if idx < 4 {
            let i = idx as usize;
            self.d[i] = (self.d[i] & 0xFFFF_FF00) | value as u32;
        } else {
            let i = (idx - 4) as usize;
            self.d[i] = (self.d[i] & 0xFFFF_00FF) | ((value as u32) << 8);
        }
    }
}

/// 16-bit register file holding eight general-purpose word registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterFile16 {
    w: [u16; 8],
}

impl Default for RegisterFile16 {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisterFile16 {
    /// Creates a new register file with all registers zeroed.
    pub const fn new() -> Self {
        Self { w: [0; 8] }
    }

    /// Reads a 16-bit register.
    #[inline(always)]
    pub const fn word(&self, reg: WordReg) -> u16 {
        self.w[reg as usize]
    }

    /// Writes a 16-bit register.
    #[inline(always)]
    pub const fn set_word(&mut self, reg: WordReg, value: u16) {
        self.w[reg as usize] = value;
    }

    /// Reads an 8-bit register (low or high byte of a word register).
    #[inline(always)]
    pub const fn byte(&self, reg: ByteReg) -> u8 {
        let idx = reg as u8;
        if idx < 4 {
            self.w[idx as usize] as u8
        } else {
            (self.w[(idx - 4) as usize] >> 8) as u8
        }
    }

    /// Writes an 8-bit register (low or high byte of a word register).
    #[inline(always)]
    pub const fn set_byte(&mut self, reg: ByteReg, value: u8) {
        let idx = reg as u8;
        if idx < 4 {
            let i = idx as usize;
            self.w[i] = (self.w[i] & 0xFF00) | value as u16;
        } else {
            let i = (idx - 4) as usize;
            self.w[i] = (self.w[i] & 0x00FF) | ((value as u16) << 8);
        }
    }
}

impl ByteReg {
    /// Returns the register for the given 3-bit index.
    pub const fn from_index(index: u8) -> Self {
        match index & 7 {
            0 => Self::AL,
            1 => Self::CL,
            2 => Self::DL,
            3 => Self::BL,
            4 => Self::AH,
            5 => Self::CH,
            6 => Self::DH,
            7 => Self::BH,
            _ => unreachable!(),
        }
    }
}

const fn build_x86_reg_word_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0u16;
    while i < 256 {
        table[i as usize] = ((i >> 3) & 7) as u8;
        i += 1;
    }
    table
}

const fn build_x86_rm_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0u16;
    while i < 256 {
        table[i as usize] = (i & 7) as u8;
        i += 1;
    }
    table
}
