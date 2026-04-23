use super::I286;
use crate::{ByteReg, SegReg16, WordReg};

/// Per-opcode internal timing for string instructions after visible bus
/// transfers have been emitted by the instruction body. The 286 Programmer's
/// Reference Manual publishes total clocks, but the timing EFSM already
/// charges the external memory and I/O cycles directly. These fields model
/// only the remaining EU/REP bookkeeping cycles.
#[derive(Clone, Copy)]
pub(super) struct I286StringTiming {
    /// Internal clocks charged when the instruction runs without a REP prefix.
    pub non_rep_base_cycles: i32,
    /// Passive startup clocks charged before the first REP iteration.
    pub rep_startup_cycles: u8,
    /// Passive startup clocks charged when REP starts with CX=0.
    pub rep_zero_count_startup_cycles: u8,
    /// Internal clocks charged for each iteration inside a REP loop. REP
    /// startup is applied separately in `start_rep`.
    pub rep_iter_base_cycles: i32,
    /// Passive clocks charged when a REP loop terminates after an iteration.
    pub rep_complete_cycles: u8,
    /// REP-only cycle adjustment when DI is odd.
    pub rep_odd_di_word_adjustment_cycles: i32,
}

pub(super) const fn string_timing(opcode: u8) -> I286StringTiming {
    match opcode {
        0x6C => I286StringTiming {
            non_rep_base_cycles: 5,
            rep_startup_cycles: 7,
            rep_zero_count_startup_cycles: 7,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 0,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0x6D => I286StringTiming {
            non_rep_base_cycles: 5,
            rep_startup_cycles: 7,
            rep_zero_count_startup_cycles: 7,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 0,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0x6E => I286StringTiming {
            non_rep_base_cycles: 1,
            rep_startup_cycles: 7,
            rep_zero_count_startup_cycles: 7,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 0,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0x6F => I286StringTiming {
            non_rep_base_cycles: 1,
            rep_startup_cycles: 7,
            rep_zero_count_startup_cycles: 7,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 0,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xA4 => I286StringTiming {
            non_rep_base_cycles: 1,
            rep_startup_cycles: 5,
            rep_zero_count_startup_cycles: 8,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 3,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xA5 => I286StringTiming {
            non_rep_base_cycles: 1,
            rep_startup_cycles: 5,
            rep_zero_count_startup_cycles: 8,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 3,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xA6 => I286StringTiming {
            non_rep_base_cycles: 4,
            rep_startup_cycles: 4,
            rep_zero_count_startup_cycles: 6,
            rep_iter_base_cycles: 5,
            rep_complete_cycles: 2,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xA7 => I286StringTiming {
            non_rep_base_cycles: 4,
            rep_startup_cycles: 4,
            rep_zero_count_startup_cycles: 6,
            rep_iter_base_cycles: 5,
            rep_complete_cycles: 2,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xAA => I286StringTiming {
            non_rep_base_cycles: 1,
            rep_startup_cycles: 5,
            rep_zero_count_startup_cycles: 5,
            rep_iter_base_cycles: 1,
            rep_complete_cycles: 1,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xAB => I286StringTiming {
            non_rep_base_cycles: 1,
            rep_startup_cycles: 5,
            rep_zero_count_startup_cycles: 5,
            rep_iter_base_cycles: 1,
            rep_complete_cycles: 1,
            rep_odd_di_word_adjustment_cycles: -1,
        },
        0xAC => I286StringTiming {
            non_rep_base_cycles: 3,
            rep_startup_cycles: 4,
            rep_zero_count_startup_cycles: 6,
            rep_iter_base_cycles: 2,
            rep_complete_cycles: 2,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xAD => I286StringTiming {
            non_rep_base_cycles: 3,
            rep_startup_cycles: 4,
            rep_zero_count_startup_cycles: 6,
            rep_iter_base_cycles: 2,
            rep_complete_cycles: 2,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xAE => I286StringTiming {
            non_rep_base_cycles: 5,
            rep_startup_cycles: 4,
            rep_zero_count_startup_cycles: 6,
            rep_iter_base_cycles: 6,
            rep_complete_cycles: 2,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        0xAF => I286StringTiming {
            non_rep_base_cycles: 5,
            rep_startup_cycles: 4,
            rep_zero_count_startup_cycles: 6,
            rep_iter_base_cycles: 6,
            rep_complete_cycles: 2,
            rep_odd_di_word_adjustment_cycles: 0,
        },
        _ => I286StringTiming {
            non_rep_base_cycles: 0,
            rep_startup_cycles: 0,
            rep_zero_count_startup_cycles: 0,
            rep_iter_base_cycles: 0,
            rep_complete_cycles: 0,
            rep_odd_di_word_adjustment_cycles: 0,
        },
    }
}

pub(super) fn rep_string_odd_di_adjustment(timing: I286StringTiming, di_before: u16) -> i32 {
    if di_before & 1 == 1 {
        timing.rep_odd_di_word_adjustment_cycles
    } else {
        0
    }
}

pub(super) fn rep_string_complete_cycles(timing: I286StringTiming, di_after: u16) -> u8 {
    let odd_adjustment = rep_string_odd_di_adjustment(timing, di_after);
    (i32::from(timing.rep_complete_cycles) + odd_adjustment).max(0) as u8
}

#[inline(always)]
fn string_writeback_tail_cycles(base_cycles: i32, word_pointer_before: u16) -> i32 {
    if word_pointer_before & 1 == 1 {
        base_cycles.saturating_sub(1)
    } else {
        base_cycles
    }
}

impl I286 {
    fn direction_delta(&self) -> u16 {
        if self.flags.df { 0xFFFF } else { 1 }
    }

    fn direction_delta_word(&self) -> u16 {
        if self.flags.df { 0xFFFE } else { 2 }
    }

    pub(super) fn movsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let val = self.read_byte_seg(bus, src_seg, si);
        self.write_byte_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        self.movsb_body(bus);
        let timing = string_timing(0xA4);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn movsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let val = self.read_word_seg(bus, src_seg, si);
        self.write_word_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        self.movsw_body(bus);
        let timing = string_timing(0xA5);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn cmpsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let dst = self.read_byte_seg(bus, SegReg16::ES, di);
        let src = self.read_byte_seg(bus, src_seg, si);
        self.alu_sub_byte(src, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        self.cmpsb_body(bus);
        let timing = string_timing(0xA6);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn cmpsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = self.default_seg(SegReg16::DS);
        let dst = self.read_word_seg(bus, SegReg16::ES, di);
        let src = self.read_word_seg(bus, src_seg, si);
        self.alu_sub_word(src, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        self.cmpsw_body(bus);
        let timing = string_timing(0xA7);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn stosb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        self.write_byte_seg(bus, SegReg16::ES, di, self.regs.byte(ByteReg::AL));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        self.stosb_body(bus);
        let timing = string_timing(0xAA);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn stosw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        self.write_word_seg(bus, SegReg16::ES, di, self.regs.word(WordReg::AX));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        let di_before = self.regs.word(WordReg::DI);
        self.stosw_body(bus);
        let timing = string_timing(0xAB);
        let tail_cycles = string_writeback_tail_cycles(timing.non_rep_base_cycles, di_before);
        self.clk(tail_cycles);
    }

    pub(super) fn lodsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_byte_seg(bus, seg, si);
        self.regs.set_byte(ByteReg::AL, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsb(&mut self, bus: &mut impl common::Bus) {
        self.lodsb_body(bus);
        let timing = string_timing(0xAC);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn lodsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_word_seg(bus, seg, si);
        self.regs.set_word(WordReg::AX, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        self.lodsw_body(bus);
        let timing = string_timing(0xAD);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn scasb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let dst = self.read_byte_seg(bus, SegReg16::ES, di);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        self.scasb_body(bus);
        let timing = string_timing(0xAE);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn scasw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let dst = self.read_word_seg(bus, SegReg16::ES, di);
        let aw = self.regs.word(WordReg::AX);
        self.alu_sub_word(aw, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        self.scasw_body(bus);
        let timing = string_timing(0xAF);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn insb_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let val = self.read_io_byte(bus, port);
        self.write_byte_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insb(&mut self, bus: &mut impl common::Bus) {
        self.insb_body(bus);
        let timing = string_timing(0x6C);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn insw_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let val = self.read_io_word(bus, port);
        self.write_word_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insw(&mut self, bus: &mut impl common::Bus) {
        self.insw_body(bus);
        let timing = string_timing(0x6D);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn outsb_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_byte_seg(bus, seg, si);
        self.write_io_byte(bus, port, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn outsb(&mut self, bus: &mut impl common::Bus) {
        self.outsb_body(bus);
        let timing = string_timing(0x6E);
        self.clk(timing.non_rep_base_cycles);
    }

    pub(super) fn outsw_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let seg = self.default_seg(SegReg16::DS);
        let val = self.read_word_seg(bus, seg, si);
        self.write_io_word(bus, port, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn outsw(&mut self, bus: &mut impl common::Bus) {
        self.outsw_body(bus);
        let timing = string_timing(0x6F);
        self.clk(timing.non_rep_base_cycles);
    }
}
