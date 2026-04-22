use super::I8086;
use crate::{SegReg16, WordReg};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum RepType {
    RepNe,
    RepE,
}

impl I8086 {
    fn start_rep(&mut self, rep_type: RepType, bus: &mut impl common::Bus) {
        self.rep_restart_ip = self.prev_ip;
        self.clk(bus, 1);
        let next = self.fetch(bus);
        self.start_rep_fetched(rep_type, next, bus);
    }

    pub(super) fn start_rep_with_opcode(
        &mut self,
        rep_type: RepType,
        opcode: u8,
        bus: &mut impl common::Bus,
    ) {
        self.rep_restart_ip = self.prev_ip;
        self.start_rep_fetched(rep_type, opcode, bus);
    }

    fn start_rep_fetched(&mut self, rep_type: RepType, mut next: u8, bus: &mut impl common::Bus) {
        let count = self.regs.word(WordReg::CX);

        // Handle segment prefix after REP.
        match next {
            0x26 => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::ES;
                self.clk(bus, 1);
                next = self.fetch(bus);
            }
            0x2E => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::CS;
                self.clk(bus, 1);
                next = self.fetch(bus);
            }
            0x36 => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::SS;
                self.clk(bus, 1);
                next = self.fetch(bus);
            }
            0x3E => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::DS;
                self.clk(bus, 1);
                next = self.fetch(bus);
            }
            _ => {}
        }

        let startup = if matches!(next, 0xA4..=0xAF) {
            if count == 0 { 5 } else { 8 }
        } else {
            0
        };
        self.clk(bus, startup);
        self.do_rep(rep_type, next, bus);
    }

    fn do_rep(&mut self, rep_type: RepType, next: u8, bus: &mut impl common::Bus) {
        // For non-string opcodes the REP/REPNE prefix does not iterate; it just
        // sets the microcode F1 flag that MUL/IMUL/DIV/IDIV inspect to negate
        // their operands. The instruction still executes exactly once regardless
        // of CX, so dispatch it here before the string-loop bookkeeping.
        if !matches!(next, 0xA4..=0xAF) {
            self.rep_prefix = true;
            self.dispatch(next, bus);
            self.rep_prefix = false;
            return;
        }

        let mut count = self.regs.word(WordReg::CX);

        if count == 0 {
            return;
        }

        let is_cmps_scas = matches!(next, 0xA6 | 0xA7 | 0xAE | 0xAF);

        loop {
            match next {
                0xA4 => {
                    self.movsb_body(bus);
                    self.clk(bus, 2);
                }
                0xA5 => {
                    self.movsw_body(bus);
                    self.clk(bus, 2);
                }
                0xA6 => {
                    self.cmpsb_body(bus);
                }
                0xA7 => {
                    self.cmpsw_body(bus);
                }
                0xAA => {
                    self.stosb_body(bus);
                    self.clk(bus, 2);
                }
                0xAB => {
                    self.stosw_body(bus);
                    self.clk(bus, 2);
                }
                0xAC => {
                    self.lodsb_body(bus);
                    self.clk(bus, 3);
                }
                0xAD => {
                    self.lodsw_body(bus);
                    self.clk(bus, 3);
                }
                0xAE => {
                    self.scasb_body(bus);
                }
                0xAF => {
                    self.scasw_body(bus);
                }
                _ => {}
            };

            match next {
                0xA4..=0xAF => {}
                _ => {
                    self.rep_prefix = true;
                    self.dispatch(next, bus);
                    self.rep_prefix = false;
                    return;
                }
            }

            match next {
                0xA4 | 0xA5 => {
                    count -= 1;
                    self.clk(bus, 2);
                    if count == 0 {
                        break;
                    }
                    self.clk(bus, 1);
                }
                0xAA | 0xAB => {
                    self.clk(bus, 1);
                    count -= 1;
                    self.clk(bus, 1);
                    if count == 0 {
                        break;
                    }
                    self.clk(bus, 1);
                }
                0xAC | 0xAD => {
                    self.clk(bus, 2);
                    count -= 1;
                    self.clk(bus, 1);
                    if count == 0 {
                        break;
                    }
                    self.clk(bus, 1);
                }
                0xA6 | 0xA7 | 0xAE | 0xAF => {
                    count -= 1;
                    self.clk(bus, 1);
                    let terminate = if is_cmps_scas {
                        match rep_type {
                            RepType::RepNe => self.flags.zf(),
                            RepType::RepE => !self.flags.zf(),
                        }
                    } else {
                        false
                    };

                    if terminate {
                        self.clk(bus, 1);
                        break;
                    }

                    self.clk(bus, 2);
                    if count == 0 {
                        break;
                    }
                    self.clk(bus, 1);
                }
                _ => unreachable!(),
            }

            self.cycles_remaining -= bus.drain_wait_cycles();

            let consumed = (self.run_budget as i64 - self.cycles_remaining) as u64;
            bus.set_current_cycle(self.run_start_cycle + consumed);

            let interrupt_pending = bus.has_nmi() || (self.flags.if_flag && bus.has_irq());

            if self.cycles_remaining <= 0 || interrupt_pending {
                // Save state for resume.
                self.rep_active = true;
                self.rep_ip = self.ip;
                self.rep_seg_prefix = self.seg_prefix;
                self.rep_prefix_seg = self.prefix_seg;
                self.rep_opcode = next;
                self.rep_type = match rep_type {
                    RepType::RepNe => 1,
                    RepType::RepE => 0,
                };
                self.regs.set_word(WordReg::CX, count);
                return;
            }
        }

        self.regs.set_word(WordReg::CX, count);
        self.seg_prefix = false;
    }

    pub(super) fn continue_rep(&mut self, bus: &mut impl common::Bus) {
        self.ip = self.rep_ip;
        self.seg_prefix = self.rep_seg_prefix;
        self.prefix_seg = self.rep_prefix_seg;
        let next = self.rep_opcode;
        self.rep_active = false;
        let rep_type = match self.rep_type {
            1 => RepType::RepNe,
            _ => RepType::RepE,
        };
        self.do_rep(rep_type, next, bus);
    }

    pub(super) fn repne(&mut self, bus: &mut impl common::Bus) {
        self.start_rep(RepType::RepNe, bus);
    }

    pub(super) fn repe(&mut self, bus: &mut impl common::Bus) {
        self.start_rep(RepType::RepE, bus);
    }
}
