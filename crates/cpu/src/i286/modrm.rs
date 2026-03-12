use super::I286;
use crate::{ByteReg, SegReg16, WordReg, build_x86_reg_word_table, build_x86_rm_table};

static MODRM_REG: [u8; 256] = build_x86_reg_word_table();
static MODRM_RM: [u8; 256] = build_x86_rm_table();

impl I286 {
    #[inline(always)]
    pub(super) fn reg_word(&self, modrm: u8) -> WordReg {
        WordReg::from_index(MODRM_REG[modrm as usize])
    }

    #[inline(always)]
    pub(super) fn reg_byte(&self, modrm: u8) -> ByteReg {
        ByteReg::from_index(MODRM_REG[modrm as usize])
    }

    #[inline(always)]
    pub(super) fn rm_word(&self, modrm: u8) -> WordReg {
        WordReg::from_index(MODRM_RM[modrm as usize])
    }

    #[inline(always)]
    pub(super) fn rm_byte(&self, modrm: u8) -> ByteReg {
        ByteReg::from_index(MODRM_RM[modrm as usize])
    }

    pub(super) fn calc_ea(&mut self, modrm: u8, bus: &mut impl common::Bus) {
        let mode = modrm >> 6;
        let rm = modrm & 7;

        match mode {
            0 => match rm {
                0 => {
                    self.eo = self
                        .regs
                        .word(WordReg::BX)
                        .wrapping_add(self.regs.word(WordReg::SI));
                    self.ea =
                        self.default_base(SegReg16::DS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::DS);
                }
                1 => {
                    self.eo = self
                        .regs
                        .word(WordReg::BX)
                        .wrapping_add(self.regs.word(WordReg::DI));
                    self.ea =
                        self.default_base(SegReg16::DS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::DS);
                }
                2 => {
                    self.eo = self
                        .regs
                        .word(WordReg::BP)
                        .wrapping_add(self.regs.word(WordReg::SI));
                    self.ea =
                        self.default_base(SegReg16::SS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::SS);
                }
                3 => {
                    self.eo = self
                        .regs
                        .word(WordReg::BP)
                        .wrapping_add(self.regs.word(WordReg::DI));
                    self.ea =
                        self.default_base(SegReg16::SS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::SS);
                }
                4 => {
                    self.eo = self.regs.word(WordReg::SI);
                    self.ea =
                        self.default_base(SegReg16::DS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::DS);
                }
                5 => {
                    self.eo = self.regs.word(WordReg::DI);
                    self.ea =
                        self.default_base(SegReg16::DS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::DS);
                }
                6 => {
                    self.eo = self.fetchword(bus);
                    self.ea =
                        self.default_base(SegReg16::DS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::DS);
                }
                7 => {
                    self.eo = self.regs.word(WordReg::BX);
                    self.ea =
                        self.default_base(SegReg16::DS).wrapping_add(self.eo as u32) & 0xFFFFFF;
                    self.ea_seg = self.default_seg(SegReg16::DS);
                }
                _ => unreachable!(),
            },
            1 => {
                let disp = self.fetch(bus) as i8 as u16;
                let seg = if rm == 2 || rm == 3 || rm == 6 {
                    SegReg16::SS
                } else {
                    SegReg16::DS
                };
                self.eo = self.ea_base(rm).wrapping_add(disp);
                self.ea = self.default_base(seg).wrapping_add(self.eo as u32) & 0xFFFFFF;
                self.ea_seg = self.default_seg(seg);
                if rm <= 3 {
                    self.clk(1);
                }
            }
            2 => {
                let disp = self.fetchword(bus);
                let seg = if rm == 2 || rm == 3 || rm == 6 {
                    SegReg16::SS
                } else {
                    SegReg16::DS
                };
                self.eo = self.ea_base(rm).wrapping_add(disp);
                self.ea = self.default_base(seg).wrapping_add(self.eo as u32) & 0xFFFFFF;
                self.ea_seg = self.default_seg(seg);
                if rm <= 3 {
                    self.clk(1);
                }
            }
            _ => unreachable!(),
        }
    }

    #[inline(always)]
    fn ea_base(&self, rm: u8) -> u16 {
        match rm {
            0 => self
                .regs
                .word(WordReg::BX)
                .wrapping_add(self.regs.word(WordReg::SI)),
            1 => self
                .regs
                .word(WordReg::BX)
                .wrapping_add(self.regs.word(WordReg::DI)),
            2 => self
                .regs
                .word(WordReg::BP)
                .wrapping_add(self.regs.word(WordReg::SI)),
            3 => self
                .regs
                .word(WordReg::BP)
                .wrapping_add(self.regs.word(WordReg::DI)),
            4 => self.regs.word(WordReg::SI),
            5 => self.regs.word(WordReg::DI),
            6 => self.regs.word(WordReg::BP),
            7 => self.regs.word(WordReg::BX),
            _ => unreachable!(),
        }
    }

    pub(super) fn get_rm_byte(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u8 {
        if modrm >= 0xC0 {
            self.regs.byte(self.rm_byte(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.seg_read_byte_at(bus, 0)
        }
    }

    pub(super) fn get_rm_word(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u16 {
        if modrm >= 0xC0 {
            self.regs.word(self.rm_word(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.seg_read_word(bus)
        }
    }

    pub(super) fn putback_rm_byte(&mut self, modrm: u8, value: u8, bus: &mut impl common::Bus) {
        if modrm >= 0xC0 {
            let reg = self.rm_byte(modrm);
            self.regs.set_byte(reg, value);
        } else {
            self.seg_write_byte_at(bus, 0, value);
        }
    }

    pub(super) fn putback_rm_word(&mut self, modrm: u8, value: u16, bus: &mut impl common::Bus) {
        if modrm >= 0xC0 {
            let reg = self.rm_word(modrm);
            self.regs.set_word(reg, value);
        } else {
            self.seg_write_word(bus, value);
        }
    }

    pub(super) fn put_rm_byte(&mut self, modrm: u8, value: u8, bus: &mut impl common::Bus) {
        if modrm >= 0xC0 {
            let reg = self.rm_byte(modrm);
            self.regs.set_byte(reg, value);
        } else {
            self.calc_ea(modrm, bus);
            self.seg_write_byte_at(bus, 0, value);
        }
    }

    pub(super) fn put_rm_word(&mut self, modrm: u8, value: u16, bus: &mut impl common::Bus) {
        if modrm >= 0xC0 {
            let reg = self.rm_word(modrm);
            self.regs.set_word(reg, value);
        } else {
            self.calc_ea(modrm, bus);
            self.seg_write_word(bus, value);
        }
    }
}
