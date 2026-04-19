use super::{IndexMode, Z80};

impl Z80 {
    pub(crate) fn increment_r(&mut self) {
        self.r = (self.r_high & 0x80) | ((self.r + 1) & 0x7F);
        self.r_high &= 0x80;
        self.r &= 0x7F;
    }

    pub(crate) fn af(&self) -> u16 {
        self.state.af()
    }

    pub(crate) fn bc(&self) -> u16 {
        self.state.bc()
    }

    pub(crate) fn de(&self) -> u16 {
        self.state.de()
    }

    pub(crate) fn hl(&self) -> u16 {
        self.state.hl()
    }

    pub(crate) fn set_af(&mut self, value: u16) {
        self.state.set_af(value);
    }

    pub(crate) fn set_bc(&mut self, value: u16) {
        self.state.set_bc(value);
    }

    pub(crate) fn set_de(&mut self, value: u16) {
        self.state.set_de(value);
    }

    pub(crate) fn set_hl(&mut self, value: u16) {
        self.state.set_hl(value);
    }

    pub(crate) fn current_hl(&self, mode: IndexMode) -> u16 {
        match mode {
            IndexMode::HL => self.hl(),
            IndexMode::IX => self.ix,
            IndexMode::IY => self.iy,
        }
    }

    pub(crate) fn set_current_hl(&mut self, mode: IndexMode, value: u16) {
        match mode {
            IndexMode::HL => self.set_hl(value),
            IndexMode::IX => self.ix = value,
            IndexMode::IY => self.iy = value,
        }
    }

    pub(crate) fn read_pair(&self, pair: u8, mode: IndexMode) -> u16 {
        match pair & 3 {
            0 => self.bc(),
            1 => self.de(),
            2 => self.current_hl(mode),
            3 => self.sp,
            _ => unreachable!(),
        }
    }

    pub(crate) fn write_pair(&mut self, pair: u8, mode: IndexMode, value: u16) {
        match pair & 3 {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_current_hl(mode, value),
            3 => self.sp = value,
            _ => unreachable!(),
        }
    }

    pub(crate) fn read_pair2(&self, pair: u8, mode: IndexMode) -> u16 {
        match pair & 3 {
            0 => self.bc(),
            1 => self.de(),
            2 => self.current_hl(mode),
            3 => self.af(),
            _ => unreachable!(),
        }
    }

    pub(crate) fn write_pair2(&mut self, pair: u8, mode: IndexMode, value: u16) {
        match pair & 3 {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_current_hl(mode, value),
            3 => self.set_af(value),
            _ => unreachable!(),
        }
    }

    pub(crate) fn read_reg8_plain(&self, reg: u8, mode: IndexMode) -> u8 {
        match reg & 7 {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => match mode {
                IndexMode::HL => self.h,
                IndexMode::IX => (self.ix >> 8) as u8,
                IndexMode::IY => (self.iy >> 8) as u8,
            },
            5 => match mode {
                IndexMode::HL => self.l,
                IndexMode::IX => self.ix as u8,
                IndexMode::IY => self.iy as u8,
            },
            7 => self.a,
            _ => unreachable!(),
        }
    }

    pub(crate) fn write_reg8_plain(&mut self, reg: u8, mode: IndexMode, value: u8) {
        match reg & 7 {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => match mode {
                IndexMode::HL => self.h = value,
                IndexMode::IX => self.ix = (u16::from(value) << 8) | u16::from(self.ix as u8),
                IndexMode::IY => self.iy = (u16::from(value) << 8) | u16::from(self.iy as u8),
            },
            5 => match mode {
                IndexMode::HL => self.l = value,
                IndexMode::IX => {
                    self.ix = (self.ix & 0xFF00) | u16::from(value);
                }
                IndexMode::IY => {
                    self.iy = (self.iy & 0xFF00) | u16::from(value);
                }
            },
            7 => self.a = value,
            _ => {}
        }
    }

    pub(crate) fn fetch_indexed_addr(
        &mut self,
        mode: IndexMode,
        bus: &mut impl common::Bus,
    ) -> u16 {
        match mode {
            IndexMode::HL => self.hl(),
            IndexMode::IX | IndexMode::IY => {
                let displacement = self.fetch_u8(bus) as i8;
                let address = self
                    .current_hl(mode)
                    .wrapping_add_signed(i16::from(displacement));
                self.wz = address;
                self.clk(5);
                address
            }
        }
    }

    pub(crate) fn read_condition(&self, condition: u8) -> bool {
        match condition & 7 {
            0 => !self.flags.zero(),
            1 => self.flags.zero(),
            2 => !self.flags.carry(),
            3 => self.flags.carry(),
            4 => !self.flags.parity_overflow(),
            5 => self.flags.parity_overflow(),
            6 => !self.flags.sign(),
            7 => self.flags.sign(),
            _ => unreachable!(),
        }
    }

    pub(crate) fn exchange_de_with_hl(&mut self, mode: IndexMode) {
        let current = self.current_hl(mode);
        self.set_current_hl(mode, self.de());
        self.set_de(current);
    }

    pub(crate) fn exchange_af(&mut self) {
        let current = self.af();
        self.set_af(self.af_alt);
        self.af_alt = current;
    }

    pub(crate) fn exchange_shadow(&mut self) {
        let current_bc = self.bc();
        let current_de = self.de();
        let current_hl = self.hl();
        self.set_bc(self.bc_alt);
        self.set_de(self.de_alt);
        self.set_hl(self.hl_alt);
        self.bc_alt = current_bc;
        self.de_alt = current_de;
        self.hl_alt = current_hl;
    }
}
