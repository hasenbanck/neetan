use super::I386;
use crate::{DwordReg, SegReg32, WordReg};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum RepType {
    RepNe,
    RepE,
}

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
    #[inline(always)]
    fn rep_count(&self) -> u32 {
        if self.address_size_override {
            self.regs.dword(DwordReg::ECX)
        } else {
            self.regs.word(WordReg::CX) as u32
        }
    }

    #[inline(always)]
    fn set_rep_count(&mut self, count: u32) {
        if self.address_size_override {
            self.regs.set_dword(DwordReg::ECX, count);
        } else {
            self.regs.set_word(WordReg::CX, count as u16);
        }
    }

    fn start_rep(&mut self, rep_type: RepType, bus: &mut impl common::Bus) {
        self.clk(Self::timing(0, 1));
        self.rep_restart_ip = self.prev_ip;
        let mut next = self.fetch(bus);

        // Handle any number of prefixes between REP and opcode.
        loop {
            match next {
                0x26 => {
                    self.seg_prefix = true;
                    self.prefix_seg = SegReg32::ES;
                    next = self.fetch(bus);
                }
                0x2E => {
                    self.seg_prefix = true;
                    self.prefix_seg = SegReg32::CS;
                    next = self.fetch(bus);
                }
                0x36 => {
                    self.seg_prefix = true;
                    self.prefix_seg = SegReg32::SS;
                    next = self.fetch(bus);
                }
                0x3E => {
                    self.seg_prefix = true;
                    self.prefix_seg = SegReg32::DS;
                    next = self.fetch(bus);
                }
                0x64 => {
                    self.seg_prefix = true;
                    self.prefix_seg = SegReg32::FS;
                    next = self.fetch(bus);
                }
                0x65 => {
                    self.seg_prefix = true;
                    self.prefix_seg = SegReg32::GS;
                    next = self.fetch(bus);
                }
                0x66 => {
                    self.operand_size_override = !self.code_segment_32bit();
                    next = self.fetch(bus);
                }
                0x67 => {
                    self.address_size_override = !self.code_segment_32bit();
                    next = self.fetch(bus);
                }
                0xF0 => {
                    next = self.fetch(bus);
                }
                _ => break,
            }
        }

        let startup = match next {
            0xA4 | 0xA5 => Self::timing(5, 11),  // REP MOVSB/W
            0xA6 | 0xA7 => Self::timing(5, 6),   // REP CMPSB/W
            0xAA | 0xAB => Self::timing(5, 6),   // REP STOSB/W
            0xAC | 0xAD => Self::timing(5, 6),   // REP LODSB/W
            0xAE | 0xAF => Self::timing(5, 6),   // REP SCASB/W
            0x6C | 0x6D => Self::timing(13, 15), // REP INSB/W
            0x6E | 0x6F => Self::timing(5, 16),  // REP OUTSB/W
            _ => 2,
        };
        self.clk(startup);
        self.do_rep(rep_type, next, bus);
    }

    fn do_rep(&mut self, rep_type: RepType, next: u8, bus: &mut impl common::Bus) {
        let mut count = self.rep_count();

        if count == 0 {
            self.rep_completed = true;
            return;
        }

        let is_cmps_scas = matches!(next, 0xA6 | 0xA7 | 0xAE | 0xAF);

        loop {
            match next {
                0x6C => self.insb(bus),
                0x6D => self.insw(bus),
                0x6E => self.outsb(bus),
                0x6F => self.outsw(bus),
                0xA4 => self.movsb(bus),
                0xA5 => self.movsw(bus),
                0xA6 => self.cmpsb(bus),
                0xA7 => self.cmpsw(bus),
                0xAA => self.stosb(bus),
                0xAB => self.stosw(bus),
                0xAC => self.lodsb(bus),
                0xAD => self.lodsw(bus),
                0xAE => self.scasb(bus),
                0xAF => self.scasw(bus),
                _ => {
                    self.dispatch(next, bus);
                    return;
                }
            }
            if self.fault_pending {
                self.set_rep_count(count);
                return;
            }
            let per_iteration_adjust = match next {
                0xA4 | 0xA5 => Self::timing(-3, -4),
                0xA6 | 0xA7 => Self::timing(-1, -1),
                0xAA | 0xAB => Self::timing(1, -1),
                0xAE | 0xAF => Self::timing(1, -1),
                0x6C | 0x6D => Self::timing(-9, -3),
                0x6E | 0x6F => Self::timing(-2, -4),
                _ => 0,
            };
            if per_iteration_adjust != 0 {
                self.clk(per_iteration_adjust);
            }

            count -= 1;
            if count == 0 {
                break;
            }

            let terminate = if is_cmps_scas {
                match rep_type {
                    RepType::RepNe => self.flags.zf(),
                    RepType::RepE => !self.flags.zf(),
                }
            } else {
                false
            };

            if terminate {
                break;
            }

            self.cycles_remaining -= bus.drain_wait_cycles();

            // Update the bus cycle so scheduler events (vsync, PIT, etc.)
            // fire at the correct time during long string operations.
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
                self.rep_operand_size_override = self.operand_size_override;
                self.rep_address_size_override = self.address_size_override;
                self.set_rep_count(count);
                return;
            }
        }

        self.set_rep_count(count);
        self.seg_prefix = false;
        self.rep_completed = true;
    }

    pub(super) fn continue_rep(&mut self, bus: &mut impl common::Bus) {
        self.ip = self.rep_ip;
        self.seg_prefix = self.rep_seg_prefix;
        self.prefix_seg = self.rep_prefix_seg;
        self.operand_size_override = self.rep_operand_size_override;
        self.address_size_override = self.rep_address_size_override;
        let next = self.rep_opcode;
        let rep_type = if self.rep_type == 1 {
            RepType::RepNe
        } else {
            RepType::RepE
        };
        self.rep_active = false;
        self.do_rep(rep_type, next, bus);
    }

    pub(super) fn repne(&mut self, bus: &mut impl common::Bus) {
        self.start_rep(RepType::RepNe, bus);
    }

    pub(super) fn repe(&mut self, bus: &mut impl common::Bus) {
        self.start_rep(RepType::RepE, bus);
    }
}
