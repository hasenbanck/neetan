use super::I8086;
use crate::{ByteReg, SegReg16, WordReg, build_x86_reg_word_table, build_x86_rm_table};

static MODRM_REG: [u8; 256] = build_x86_reg_word_table();
static MODRM_RM: [u8; 256] = build_x86_rm_table();

impl I8086 {
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

    #[inline(always)]
    fn modrm_decode_costs(&self, modrm: u8) -> (i32, i32, u8) {
        match modrm & 0xC7 {
            0x00 => (4, 0, 0),
            0x01 => (5, 0, 0),
            0x02 => (5, 0, 0),
            0x03 => (4, 0, 0),
            0x04 => (2, 0, 0),
            0x05 => (2, 0, 0),
            0x06 => (0, 1, 2),
            0x07 => (2, 0, 0),
            0x40 => (4, 3, 1),
            0x41 => (5, 3, 1),
            0x42 => (5, 3, 1),
            0x43 => (4, 3, 1),
            0x44 => (2, 3, 1),
            0x45 => (2, 3, 1),
            0x46 => (2, 3, 1),
            0x47 => (2, 3, 1),
            0x80 => (4, 2, 2),
            0x81 => (5, 2, 2),
            0x82 => (5, 2, 2),
            0x83 => (4, 2, 2),
            0x84 => (2, 2, 2),
            0x85 => (2, 2, 2),
            0x86 => (2, 2, 2),
            0x87 => (2, 2, 2),
            _ => (0, 0, 0),
        }
    }

    #[inline(always)]
    pub(super) fn has_disp16_single_register_base(&self, modrm: u8) -> bool {
        matches!(modrm & 0xC7, 0x84..=0x87)
    }

    #[inline(always)]
    pub(super) fn has_disp8_single_register_base(&self, modrm: u8) -> bool {
        matches!(modrm & 0xC7, 0x44..=0x47)
    }

    #[inline(always)]
    pub(super) fn has_disp16_double_register_base(&self, modrm: u8) -> bool {
        matches!(modrm & 0xC7, 0x80..=0x83)
    }

    #[inline(always)]
    pub(super) fn has_disp16_cycle4_double_register_base(&self, modrm: u8) -> bool {
        matches!(modrm & 0xC7, 0x80 | 0x83)
    }

    #[inline(always)]
    pub(super) fn has_mod0_single_register_base(&self, modrm: u8) -> bool {
        matches!(modrm & 0xC7, 0x04..=0x07)
    }

    #[inline(always)]
    pub(super) fn has_single_or_direct_base(&self, modrm: u8) -> bool {
        self.has_mod0_single_register_base(modrm) || self.has_disp16_single_register_base(modrm)
    }

    #[inline(always)]
    pub(super) fn has_simple_disp16_base(&self, modrm: u8) -> bool {
        matches!(modrm & 0xC7, 0x06 | 0x84..=0x87)
    }

    pub(super) fn fetch_modrm(&mut self, bus: &mut impl common::Bus) -> u8 {
        let modrm = self.fetch(bus);
        self.modrm_displacement = 0;
        self.modrm_has_displacement = false;

        if modrm < 0xC0 {
            let (pre_disp_cost, post_disp_cost, displacement_size) = self.modrm_decode_costs(modrm);
            self.clk(bus, 1 + pre_disp_cost);

            match displacement_size {
                1 => {
                    self.modrm_displacement = self.fetch(bus) as i8 as u16;
                    self.modrm_has_displacement = true;
                }
                2 => {
                    self.modrm_displacement = self.fetchword(bus);
                    self.modrm_has_displacement = true;
                }
                _ => {}
            }

            self.clk(bus, post_disp_cost);

            if !self.seg_prefix
                && !self.opcode_started_at_odd_address()
                && self.has_disp16_single_register_base(modrm)
            {
                self.clk(bus, 1);
            }
        }

        modrm
    }

    pub(super) fn calc_ea(&mut self, modrm: u8) {
        debug_assert!(modrm < 0xC0);

        let mode = modrm >> 6;
        let rm = modrm & 7;

        // Mode 0, rm 6 is the only "direct address" form: the displacement
        // word holds the offset and the default segment is DS (not SS, even
        // though rm 6 normally selects BP).
        if mode == 0 && rm == 6 {
            self.set_effective_address(SegReg16::DS, self.modrm_displacement);
            return;
        }

        let base = self.ea_base(rm);
        let displacement = if self.modrm_has_displacement {
            self.modrm_displacement
        } else {
            0
        };
        // BP-based addressing modes (rm 2, 3, and rm 6 with a displacement)
        // default to SS; every other base uses DS.
        let seg = if rm == 2 || rm == 3 || (rm == 6 && mode != 0) {
            SegReg16::SS
        } else {
            SegReg16::DS
        };
        self.set_effective_address(seg, base.wrapping_add(displacement));
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

    #[inline(always)]
    pub(super) fn resolve_rm_address(&mut self, modrm: u8) {
        if modrm < 0xC0 {
            self.calc_ea(modrm);
        }
    }

    #[inline(always)]
    pub(super) fn charge_rm_eadone(&mut self, modrm: u8, bus: &mut impl common::Bus) {
        if modrm < 0xC0 {
            if !self.seg_prefix
                && self.instruction_entry_queue_full()
                && !self.opcode_started_at_odd_address()
                && self.has_disp16_single_register_base(modrm)
            {
                self.clk(bus, 1);
            } else {
                self.clk_eadone(bus);
            }
        }
    }

    pub(super) fn get_rm_byte(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u8 {
        if modrm >= 0xC0 {
            self.regs.byte(self.rm_byte(modrm))
        } else {
            self.calc_ea(modrm);
            let value = self.read_memory_byte(bus, self.ea);
            self.clk_eaload(bus);
            value
        }
    }

    pub(super) fn get_rm_word(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u16 {
        if modrm >= 0xC0 {
            self.regs.word(self.rm_word(modrm))
        } else {
            self.calc_ea(modrm);
            let value = self.seg_read_word(bus);
            self.clk_eaload(bus);
            value
        }
    }

    pub(super) fn putback_rm_byte(&mut self, modrm: u8, value: u8, bus: &mut impl common::Bus) {
        if modrm >= 0xC0 {
            let reg = self.rm_byte(modrm);
            self.regs.set_byte(reg, value);
        } else {
            self.write_memory_byte(bus, self.ea, value);
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
}
