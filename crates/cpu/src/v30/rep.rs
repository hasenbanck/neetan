use super::V30;
use crate::{SegReg16, WordReg};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum RepType {
    RepNc,
    RepC,
    RepNe,
    RepE,
}

impl V30 {
    fn start_rep(&mut self, rep_type: RepType, bus: &mut impl common::Bus) {
        self.rep_restart_ip = self.prev_ip;
        let mut next = self.fetch(bus);

        // Handle segment prefix after REP.
        match next {
            0x26 => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::ES;
                next = self.fetch(bus);
                self.clk(bus, 0);
            }
            0x2E => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::CS;
                next = self.fetch(bus);
                self.clk(bus, 0);
            }
            0x36 => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::SS;
                next = self.fetch(bus);
                self.clk(bus, 0);
            }
            0x3E => {
                self.seg_prefix = true;
                self.prefix_seg = SegReg16::DS;
                next = self.fetch(bus);
                self.clk(bus, 0);
            }
            _ => {}
        }

        let startup = match next {
            0xA4 | 0xA5 => 5, // REP MOVSB/W
            0xA6 | 0xA7 => 5, // REP CMPSB/W
            0xAA | 0xAB => 4, // REP STOSB/W
            0xAC | 0xAD => 5, // REP LODSB/W
            0xAE | 0xAF => 5, // REP SCASB/W
            0x6C | 0x6D => 5, // REP INSB/W
            0x6E | 0x6F => 5, // REP OUTSB/W
            _ => 0,
        };
        self.clk(bus, startup);
        self.do_rep(rep_type, next, bus);
    }

    fn do_rep(&mut self, rep_type: RepType, next: u8, bus: &mut impl common::Bus) {
        let mut count = self.regs.word(WordReg::CX);

        if count == 0 {
            return;
        }

        let is_cmps_scas = matches!(next, 0xA6 | 0xA7 | 0xAE | 0xAF);
        let ends_with_write = matches!(next, 0x6C | 0x6D | 0x6E | 0x6F | 0xA4 | 0xA5 | 0xAA | 0xAB);

        loop {
            match next {
                0x6C => self.insb_body(bus),
                0x6D => self.insw_body(bus),
                0x6E => self.outsb_body(bus),
                0x6F => self.outsw_body(bus),
                0xA4 => self.movsb(bus),
                0xA5 => self.movsw(bus),
                0xA6 => self.cmpsb(bus),
                0xA7 => self.cmpsw(bus),
                0xAA => self.stosb_body(bus),
                0xAB => self.stosw_body(bus),
                0xAC => self.lodsb(bus),
                0xAD => self.lodsw(bus),
                0xAE => self.scasb(bus),
                0xAF => self.scasw(bus),
                _ => {
                    self.dispatch(next, bus);
                    return;
                }
            }

            count -= 1;
            if count == 0 {
                break;
            }

            let terminate = if is_cmps_scas {
                match rep_type {
                    RepType::RepNc => self.flags.cf(),
                    RepType::RepC => !self.flags.cf(),
                    RepType::RepNe => self.flags.zf(),
                    RepType::RepE => !self.flags.zf(),
                }
            } else {
                false
            };

            if terminate {
                break;
            }

            if is_cmps_scas {
                self.clk(bus, 5);
            }

            self.cycles_remaining -= bus.drain_wait_cycles();

            // Update the bus cycle so scheduler events (vsync, PIT, etc.)
            // fire at the correct time during long string operations.
            let consumed = (self.run_budget as i64 - self.cycles_remaining) as u64;
            bus.set_current_cycle(self.run_start_cycle + consumed);

            let interrupt_pending = bus.has_nmi() || (self.flags.if_flag && bus.has_irq());

            if self.cycles_remaining <= 0 || interrupt_pending {
                if ends_with_write {
                    self.biu_bus_wait_finish(bus);
                }

                // Save state for resume.
                self.rep_active = true;
                self.rep_ip = self.ip;
                self.rep_seg_prefix = self.seg_prefix;
                self.rep_prefix_seg = self.prefix_seg;
                self.rep_opcode = next;
                self.rep_type = match rep_type {
                    RepType::RepNe => 1,
                    RepType::RepNc => 2,
                    RepType::RepC => 3,
                    RepType::RepE => 0,
                };
                self.regs.set_word(WordReg::CX, count);
                return;
            }

            if ends_with_write {
                self.biu_chain_eu_transfer();
            }
        }

        if ends_with_write {
            self.biu_bus_wait_finish(bus);
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
            2 => RepType::RepNc,
            3 => RepType::RepC,
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

    pub(super) fn repnc(&mut self, bus: &mut impl common::Bus) {
        self.start_rep(RepType::RepNc, bus);
    }

    pub(super) fn repc(&mut self, bus: &mut impl common::Bus) {
        self.start_rep(RepType::RepC, bus);
    }
}
