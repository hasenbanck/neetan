use super::{I8086, flags::I8086Flags};
use crate::{ByteReg, RegisterFile16, SegReg16, WordReg};

/// Snapshot of all I8086 CPU registers and flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct I8086State {
    /// General-purpose register file.
    pub regs: RegisterFile16,
    /// Segment registers: ES, CS, SS, DS.
    pub sregs: [u16; 4],
    /// Instruction pointer.
    pub ip: u16,
    /// CPU flags.
    pub flags: I8086Flags,
}

impl I8086State {
    /// Returns the AX register.
    pub fn ax(&self) -> u16 {
        self.regs.word(WordReg::AX)
    }

    /// Sets the AX register.
    pub fn set_ax(&mut self, v: u16) {
        self.regs.set_word(WordReg::AX, v);
    }

    /// Returns the CX register.
    pub fn cx(&self) -> u16 {
        self.regs.word(WordReg::CX)
    }

    /// Sets the CX register.
    pub fn set_cx(&mut self, v: u16) {
        self.regs.set_word(WordReg::CX, v);
    }

    /// Returns the DX register.
    pub fn dx(&self) -> u16 {
        self.regs.word(WordReg::DX)
    }

    /// Sets the DX register.
    pub fn set_dx(&mut self, v: u16) {
        self.regs.set_word(WordReg::DX, v);
    }

    /// Returns the BX register.
    pub fn bx(&self) -> u16 {
        self.regs.word(WordReg::BX)
    }

    /// Sets the BX register.
    pub fn set_bx(&mut self, v: u16) {
        self.regs.set_word(WordReg::BX, v);
    }

    /// Returns the SP register.
    pub fn sp(&self) -> u16 {
        self.regs.word(WordReg::SP)
    }

    /// Sets the SP register.
    pub fn set_sp(&mut self, v: u16) {
        self.regs.set_word(WordReg::SP, v);
    }

    /// Returns the BP register.
    pub fn bp(&self) -> u16 {
        self.regs.word(WordReg::BP)
    }

    /// Sets the BP register.
    pub fn set_bp(&mut self, v: u16) {
        self.regs.set_word(WordReg::BP, v);
    }

    /// Returns the SI register.
    pub fn si(&self) -> u16 {
        self.regs.word(WordReg::SI)
    }

    /// Sets the SI register.
    pub fn set_si(&mut self, v: u16) {
        self.regs.set_word(WordReg::SI, v);
    }

    /// Returns the DI register.
    pub fn di(&self) -> u16 {
        self.regs.word(WordReg::DI)
    }

    /// Sets the DI register.
    pub fn set_di(&mut self, v: u16) {
        self.regs.set_word(WordReg::DI, v);
    }

    /// Returns the ES segment register.
    pub fn es(&self) -> u16 {
        self.sregs[SegReg16::ES as usize]
    }

    /// Sets the ES segment register.
    pub fn set_es(&mut self, v: u16) {
        self.sregs[SegReg16::ES as usize] = v;
    }

    /// Returns the CS segment register.
    pub fn cs(&self) -> u16 {
        self.sregs[SegReg16::CS as usize]
    }

    /// Sets the CS segment register.
    pub fn set_cs(&mut self, v: u16) {
        self.sregs[SegReg16::CS as usize] = v;
    }

    /// Returns the SS segment register.
    pub fn ss(&self) -> u16 {
        self.sregs[SegReg16::SS as usize]
    }

    /// Sets the SS segment register.
    pub fn set_ss(&mut self, v: u16) {
        self.sregs[SegReg16::SS as usize] = v;
    }

    /// Returns the DS segment register.
    pub fn ds(&self) -> u16 {
        self.sregs[SegReg16::DS as usize]
    }

    /// Sets the DS segment register.
    pub fn set_ds(&mut self, v: u16) {
        self.sregs[SegReg16::DS as usize] = v;
    }

    /// Returns the compressed flags register value.
    pub fn compressed_flags(&self) -> u16 {
        self.flags.compress()
    }

    /// Sets all flags from a compressed flags value.
    pub fn set_compressed_flags(&mut self, v: u16) {
        self.flags.expand(v);
    }
}

impl I8086 {
    /// Loads CPU state from a snapshot, resetting runtime flags.
    pub fn load_state(&mut self, state: &I8086State) {
        self.state = state.clone();
        self.halted = false;
        self.pending_irq = 0;
        self.no_interrupt = 0;
        self.inhibit_all = 0;
        self.rep_active = false;
        self.rep_restart_ip = 0;
        self.rep_prefix = false;
        self.seg_prefix = false;
        self.flush_prefetch_queue();
        self.reset_instruction_timing();
    }

    /// Returns the AL register value.
    pub fn al(&self) -> u8 {
        self.regs.byte(ByteReg::AL)
    }

    /// Returns the AH register value.
    pub fn ah(&self) -> u8 {
        self.regs.byte(ByteReg::AH)
    }

    /// Returns the CL register value.
    pub fn cl(&self) -> u8 {
        self.regs.byte(ByteReg::CL)
    }

    /// Returns the instruction pointer.
    pub fn ip(&self) -> u16 {
        self.ip
    }

    /// Returns the compressed flags register value.
    pub fn flags_register(&self) -> u16 {
        self.flags.compress()
    }
}
