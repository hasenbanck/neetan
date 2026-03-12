use super::V30;

impl V30 {
    #[inline(always)]
    pub(super) fn alu_add_byte(&mut self, dst: u8, src: u8) -> u8 {
        let result = dst as u32 + src as u32;
        self.flags.set_cf_byte(result);
        self.flags.set_of_add_byte(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_add_word(&mut self, dst: u16, src: u16) -> u16 {
        let result = dst as u32 + src as u32;
        self.flags.set_cf_word(result);
        self.flags.set_of_add_word(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_word(result);
        result as u16
    }

    #[inline(always)]
    pub(super) fn alu_adc_byte(&mut self, dst: u8, src: u8, cf: u32) -> u8 {
        let result = dst as u32 + src as u32 + cf;
        self.flags.set_cf_byte(result);
        self.flags.set_of_add_byte(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_adc_word(&mut self, dst: u16, src: u16, cf: u32) -> u16 {
        let result = dst as u32 + src as u32 + cf;
        self.flags.set_cf_word(result);
        self.flags.set_of_add_word(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_word(result);
        result as u16
    }

    #[inline(always)]
    pub(super) fn alu_sbb_byte(&mut self, dst: u8, src: u8, cf: u32) -> u8 {
        let result = (dst as u32).wrapping_sub(src as u32).wrapping_sub(cf);
        self.flags.set_cf_byte(result);
        self.flags.set_of_sub_byte(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_sbb_word(&mut self, dst: u16, src: u16, cf: u32) -> u16 {
        let result = (dst as u32).wrapping_sub(src as u32).wrapping_sub(cf);
        self.flags.set_cf_word(result);
        self.flags.set_of_sub_word(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_word(result);
        result as u16
    }

    #[inline(always)]
    pub(super) fn alu_sub_byte(&mut self, dst: u8, src: u8) -> u8 {
        let result = (dst as u32).wrapping_sub(src as u32);
        self.flags.set_cf_byte(result);
        self.flags.set_of_sub_byte(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_sub_word(&mut self, dst: u16, src: u16) -> u16 {
        let result = (dst as u32).wrapping_sub(src as u32);
        self.flags.set_cf_word(result);
        self.flags.set_of_sub_word(result, src as u32, dst as u32);
        self.flags.set_af(result, src as u32, dst as u32);
        self.flags.set_szpf_word(result);
        result as u16
    }

    #[inline(always)]
    pub(super) fn alu_or_byte(&mut self, dst: u8, src: u8) -> u8 {
        let result = dst | src;
        self.flags.carry_val = 0;
        self.flags.overflow_val = 0;
        self.flags.aux_val = 0;
        self.flags.set_szpf_byte(result as u32);
        result
    }

    #[inline(always)]
    pub(super) fn alu_or_word(&mut self, dst: u16, src: u16) -> u16 {
        let result = dst | src;
        self.flags.carry_val = 0;
        self.flags.overflow_val = 0;
        self.flags.aux_val = 0;
        self.flags.set_szpf_word(result as u32);
        result
    }

    #[inline(always)]
    pub(super) fn alu_and_byte(&mut self, dst: u8, src: u8) -> u8 {
        let result = dst & src;
        self.flags.carry_val = 0;
        self.flags.overflow_val = 0;
        self.flags.aux_val = 0;
        self.flags.set_szpf_byte(result as u32);
        result
    }

    #[inline(always)]
    pub(super) fn alu_and_word(&mut self, dst: u16, src: u16) -> u16 {
        let result = dst & src;
        self.flags.carry_val = 0;
        self.flags.overflow_val = 0;
        self.flags.aux_val = 0;
        self.flags.set_szpf_word(result as u32);
        result
    }

    #[inline(always)]
    pub(super) fn alu_xor_byte(&mut self, dst: u8, src: u8) -> u8 {
        let result = dst ^ src;
        self.flags.carry_val = 0;
        self.flags.overflow_val = 0;
        self.flags.aux_val = 0;
        self.flags.set_szpf_byte(result as u32);
        result
    }

    #[inline(always)]
    pub(super) fn alu_xor_word(&mut self, dst: u16, src: u16) -> u16 {
        let result = dst ^ src;
        self.flags.carry_val = 0;
        self.flags.overflow_val = 0;
        self.flags.aux_val = 0;
        self.flags.set_szpf_word(result as u32);
        result
    }

    #[inline(always)]
    pub(super) fn alu_inc_byte(&mut self, val: u8) -> u8 {
        let result = val as u32 + 1;
        self.flags.set_of_add_byte(result, 1, val as u32);
        self.flags.set_af(result, 1, val as u32);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_inc_word(&mut self, val: u16) -> u16 {
        let result = val as u32 + 1;
        self.flags.set_of_add_word(result, 1, val as u32);
        self.flags.set_af(result, 1, val as u32);
        self.flags.set_szpf_word(result);
        result as u16
    }

    #[inline(always)]
    pub(super) fn alu_dec_byte(&mut self, val: u8) -> u8 {
        let result = (val as u32).wrapping_sub(1);
        self.flags.set_of_sub_byte(result, 1, val as u32);
        self.flags.set_af(result, 1, val as u32);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_dec_word(&mut self, val: u16) -> u16 {
        let result = (val as u32).wrapping_sub(1);
        self.flags.set_of_sub_word(result, 1, val as u32);
        self.flags.set_af(result, 1, val as u32);
        self.flags.set_szpf_word(result);
        result as u16
    }

    #[inline(always)]
    pub(super) fn alu_neg_byte(&mut self, val: u8) -> u8 {
        let result = 0u32.wrapping_sub(val as u32);
        self.flags.carry_val = if val != 0 { 1 } else { 0 };
        self.flags.set_of_sub_byte(result, val as u32, 0);
        self.flags.set_af(result, val as u32, 0);
        self.flags.set_szpf_byte(result);
        result as u8
    }

    #[inline(always)]
    pub(super) fn alu_neg_word(&mut self, val: u16) -> u16 {
        let result = 0u32.wrapping_sub(val as u32);
        self.flags.carry_val = if val != 0 { 1 } else { 0 };
        self.flags.set_of_sub_word(result, val as u32, 0);
        self.flags.set_af(result, val as u32, 0);
        self.flags.set_szpf_word(result);
        result as u16
    }

    pub(super) fn alu_shl_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        let result = if count < 8 {
            (val as u32) << count
        } else {
            0u32
        };
        self.flags.carry_val = if count <= 8 {
            ((val as u32) << (count - 1)) & 0x80
        } else {
            0
        };
        self.flags.overflow_val = (((result >> 7) & 1) ^ self.flags.cf_val()) * 0x80;
        self.flags.aux_val = 0;
        self.flags.set_szpf_byte(result);
        result as u8
    }

    pub(super) fn alu_shl_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        let result = if count < 16 {
            (val as u32) << count
        } else {
            0u32
        };
        self.flags.carry_val = if count <= 16 {
            ((val as u32) << (count - 1)) & 0x8000
        } else {
            0
        };
        self.flags.overflow_val = (((result >> 15) & 1) ^ self.flags.cf_val()) * 0x8000;
        self.flags.aux_val = 0;
        self.flags.set_szpf_word(result);
        result as u16
    }

    pub(super) fn alu_shr_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        self.flags.overflow_val = if count == 1 { val as u32 & 0x80 } else { 0 };
        let result = if count < 8 {
            self.flags.carry_val = ((val >> (count - 1)) & 1) as u32;
            val >> count
        } else {
            self.flags.carry_val = if count == 8 { (val >> 7) as u32 } else { 0 };
            0u8
        };
        self.flags.aux_val = 0;
        self.flags.set_szpf_byte(result as u32);
        result
    }

    pub(super) fn alu_shr_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        self.flags.overflow_val = if count == 1 { val as u32 & 0x8000 } else { 0 };
        let result = if count < 16 {
            self.flags.carry_val = ((val >> (count - 1)) & 1) as u32;
            val >> count
        } else {
            self.flags.carry_val = if count == 16 { (val >> 15) as u32 } else { 0 };
            0u16
        };
        self.flags.aux_val = 0;
        self.flags.set_szpf_word(result as u32);
        result
    }

    pub(super) fn alu_sar_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        self.flags.overflow_val = 0;
        let signed = val as i8;
        let result = if count < 8 {
            self.flags.carry_val = ((signed >> (count - 1)) & 1) as u32;
            (signed >> count) as u8
        } else {
            self.flags.carry_val = if signed < 0 { 1 } else { 0 };
            (signed >> 7) as u8
        };
        self.flags.aux_val = 0;
        self.flags.set_szpf_byte(result as u32);
        result
    }

    pub(super) fn alu_sar_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        self.flags.overflow_val = 0;
        let signed = val as i16;
        let result = if count < 16 {
            self.flags.carry_val = ((signed >> (count - 1)) & 1) as u32;
            (signed >> count) as u16
        } else {
            self.flags.carry_val = if signed < 0 { 1 } else { 0 };
            (signed >> 15) as u16
        };
        self.flags.aux_val = 0;
        self.flags.set_szpf_word(result as u32);
        result
    }

    pub(super) fn alu_rol_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        let count = count & 7;
        let result = if count == 0 {
            self.flags.carry_val = (val & 1) as u32;
            val
        } else {
            let r = val.rotate_left(count as u32);
            self.flags.carry_val = (r & 1) as u32;
            r
        };
        self.flags.overflow_val = ((result >> 7) ^ (result & 1)) as u32 * 0x80;
        result
    }

    pub(super) fn alu_rol_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        let count = count & 15;
        let result = if count == 0 {
            self.flags.carry_val = (val & 1) as u32;
            val
        } else {
            let r = val.rotate_left(count as u32);
            self.flags.carry_val = (r & 1) as u32;
            r
        };
        self.flags.overflow_val = ((result >> 15) ^ (result & 1)) as u32 * 0x8000;
        result
    }

    pub(super) fn alu_ror_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        let count = count & 7;
        let result = if count == 0 {
            self.flags.carry_val = ((val >> 7) & 1) as u32;
            val
        } else {
            let r = val.rotate_right(count as u32);
            self.flags.carry_val = ((r >> 7) & 1) as u32;
            r
        };
        self.flags.overflow_val = ((result ^ (result << 1)) & 0x80) as u32;
        result
    }

    pub(super) fn alu_ror_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        let count = count & 15;
        let result = if count == 0 {
            self.flags.carry_val = ((val >> 15) & 1) as u32;
            val
        } else {
            let r = val.rotate_right(count as u32);
            self.flags.carry_val = ((r >> 15) & 1) as u32;
            r
        };
        self.flags.overflow_val = ((result ^ (result << 1)) & 0x8000) as u32;
        result
    }

    pub(super) fn alu_rcl_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        let count = count % 9;
        if count == 0 {
            self.flags.overflow_val = ((val as u32 >> 7) ^ self.flags.cf_val()) * 0x80;
            return val;
        }
        let cf = self.flags.cf_val();
        let wide = (val as u32) | (cf << 8);
        let rotated = (wide << count) | (wide >> (9 - count));
        let result = rotated as u8;
        self.flags.carry_val = (rotated >> 8) & 1;
        self.flags.overflow_val = ((result as u32 >> 7) ^ self.flags.carry_val) * 0x80;
        result
    }

    pub(super) fn alu_rcl_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        let count = count % 17;
        if count == 0 {
            self.flags.overflow_val = ((val as u32 >> 15) ^ self.flags.cf_val()) * 0x8000;
            return val;
        }
        let cf = self.flags.cf_val();
        let wide = (val as u32) | (cf << 16);
        let rotated = (wide << count) | (wide >> (17 - count));
        let result = rotated as u16;
        self.flags.carry_val = (rotated >> 16) & 1;
        self.flags.overflow_val = ((result as u32 >> 15) ^ self.flags.carry_val) * 0x8000;
        result
    }

    pub(super) fn alu_rcr_byte(&mut self, val: u8, count: u8) -> u8 {
        if count == 0 {
            return val;
        }
        let count = count % 9;
        if count == 0 {
            self.flags.overflow_val = ((val ^ (val << 1)) & 0x80) as u32;
            return val;
        }
        let cf = self.flags.cf_val();
        let wide = (val as u32) | (cf << 8);
        let result = ((wide >> count) | (wide << (9 - count))) as u8;
        self.flags.carry_val = (wide >> (count - 1)) & 1;
        self.flags.overflow_val = ((result ^ (result << 1)) & 0x80) as u32;
        result
    }

    pub(super) fn alu_rcr_word(&mut self, val: u16, count: u8) -> u16 {
        if count == 0 {
            return val;
        }
        let count = count % 17;
        if count == 0 {
            self.flags.overflow_val = (val as u32 ^ ((val as u32) << 1)) & 0x8000;
            return val;
        }
        let cf = self.flags.cf_val();
        let wide = (val as u32) | (cf << 16);
        let result = ((wide >> count) | (wide << (17 - count))) as u16;
        self.flags.carry_val = (wide >> (count - 1)) & 1;
        self.flags.overflow_val = (result as u32 ^ ((result as u32) << 1)) & 0x8000;
        result
    }
}
