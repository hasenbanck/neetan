use super::{FinishState, VX0, biu::queue_size_for};
use crate::{ByteReg, SegReg16, WordReg};

struct SignedDivideTiming {
    word: bool,
    dividend_negative: bool,
    quotient_bits: u32,
    success: bool,
}

impl<const MODEL: u8> VX0<MODEL> {
    fn clk_unary_rmw_tail(&mut self, bus: &mut impl common::Bus, modrm: u8) {
        if modrm >= 0xC0 {
            let cycles =
                if self.seg_prefix && self.instruction_entry_queue_had_current_instruction() {
                    1
                } else {
                    2
                };
            self.clk(bus, cycles);
        } else {
            self.clk(bus, 1);
        }
    }

    fn prepare_unary_rmw_write(&mut self, bus: &mut impl common::Bus, modrm: u8) {
        if modrm < 0xC0 {
            self.biu_prefetch_before_rmw_write(bus);
            self.clk(bus, 2);
        }
    }

    fn prepare_divide_memory_read(&mut self, bus: &mut impl common::Bus, modrm: u8) {
        let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);
        let delayed_read = entry_queue_len == 0
            || has_word_displacement
            || (self.rep_state.prefix && !self.seg_prefix && mode == 0);

        if self.rep_state.prefix
            && self.seg_prefix
            && entry_queue_len == queue_size_for(MODEL)
            && mode == 1
        {
            self.biu_fetch_suspend(bus);
            self.biu_complete_code_fetch_for_eu();
            self.clk(bus, 2);
            self.biu_ready_memory_read();
        } else if delayed_read {
            self.biu_fetch_suspend(bus);
            self.clk(bus, 2);
            self.biu_ready_memory_read();
        } else if (self.seg_prefix && entry_queue_len == queue_size_for(MODEL) && mode == 1)
            || (self.rep_state.prefix && mode == 1)
            || (self.rep_state.prefix
                && self.seg_prefix
                && entry_queue_len == queue_size_for(MODEL)
                && mode == 0)
        {
            self.complete_next_fallthrough_code_fetch(bus);
            self.biu_ready_memory_read();
        } else if self.seg_prefix && entry_queue_len == queue_size_for(MODEL) && mode == 0 {
            self.biu_prepare_memory_read();
        }
    }

    fn get_rm_byte_divide(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u8 {
        if modrm >= 0xC0 {
            self.regs.byte(self.rm_byte(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.prepare_divide_memory_read(bus, modrm);
            let value = self.biu_read_u8_physical(bus, self.ea);
            self.biu_fetch_resume_immediate_for_eu();
            value
        }
    }

    fn get_rm_word_divide(&mut self, modrm: u8, bus: &mut impl common::Bus) -> u16 {
        if modrm >= 0xC0 {
            self.regs.word(self.rm_word(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.prepare_divide_memory_read(bus, modrm);
            let value = self.seg_read_word(bus);
            self.biu_fetch_resume_immediate_for_eu();
            value
        }
    }

    fn clk_unsigned_divide_success(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        entry_queue_len: usize,
        word: bool,
    ) {
        let mut register_cycles = if word { 26 } else { 20 };
        let memory_cycles = if word { 27 } else { 21 };
        if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix {
            register_cycles -= 1;
        }
        self.clk_modrm(bus, modrm, register_cycles, memory_cycles);
    }

    fn clk_unsigned_divide_error(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        entry_queue_len: usize,
        word: bool,
    ) {
        let register_cycles = if word {
            if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix {
                9
            } else {
                10
            }
        } else if entry_queue_len == queue_size_for(MODEL) {
            11
        } else {
            13
        };
        let memory_cycles = if word { 11 } else { 12 };
        self.clk_modrm(bus, modrm, register_cycles, memory_cycles);
    }

    fn unsigned_byte_divide_error_entry_cycles(&self, modrm: u8, entry_queue_len: usize) -> i32 {
        if modrm >= 0xC0 && entry_queue_len == 0 {
            3
        } else if modrm >= 0xC0 && entry_queue_len == queue_size_for(MODEL) && self.seg_prefix {
            0
        } else {
            1
        }
    }

    fn unsigned_byte_divide_error_needs_ready_vector_read(
        &self,
        modrm: u8,
        entry_queue_len: usize,
    ) -> bool {
        modrm >= 0xC0 && entry_queue_len == 0
    }

    fn unsigned_word_divide_error_entry_cycles(&self, modrm: u8, entry_queue_len: usize) -> i32 {
        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);
        if modrm < 0xC0 && (entry_queue_len != queue_size_for(MODEL) || has_word_displacement) {
            4
        } else {
            1
        }
    }

    fn unsigned_word_divide_error_needs_ready_vector_read(
        &self,
        modrm: u8,
        entry_queue_len: usize,
    ) -> bool {
        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);
        modrm < 0xC0 && (entry_queue_len != queue_size_for(MODEL) || has_word_displacement)
    }

    fn clk_signed_divide(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        entry_queue_len: usize,
        timing: SignedDivideTiming,
    ) {
        let SignedDivideTiming {
            word,
            dividend_negative,
            quotient_bits,
            success,
        } = timing;
        let limit_bits = if word { 16 } else { 8 };
        let mut register_cycles;
        let mut memory_cycles;
        if success || quotient_bits == limit_bits {
            register_cycles = if word { 45 } else { 38 };
            memory_cycles = if word { 46 } else { 39 };
            if dividend_negative {
                register_cycles += 3;
                memory_cycles += 3;
            }
            if !success {
                let overflow_adjustment = if word { 3 } else { 4 };
                register_cycles -= overflow_adjustment;
                memory_cycles -= overflow_adjustment;
            }
        } else {
            register_cycles = if dividend_negative { 22 } else { 19 };
            memory_cycles = if dividend_negative { 23 } else { 20 };
        }

        if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix {
            register_cycles -= 1;
        }
        if modrm >= 0xC0
            && self.rep_state.prefix
            && entry_queue_len == queue_size_for(MODEL)
            && (success || quotient_bits >= limit_bits)
        {
            register_cycles += 1;
        }
        self.clk_modrm(bus, modrm, register_cycles, memory_cycles);
    }

    fn divide_abs_bits(value: i32) -> u32 {
        let magnitude = value.unsigned_abs();
        if magnitude == 0 {
            0
        } else {
            u32::BITS - magnitude.leading_zeros()
        }
    }

    fn complete_one_fallthrough_code_fetch(&mut self, bus: &mut impl common::Bus) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        } else {
            self.biu_complete_current_bus_for_eu();
            self.biu_start_code_fetch_for_eu();
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }
    }

    fn complete_next_fallthrough_code_fetch(&mut self, bus: &mut impl common::Bus) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }
        self.complete_one_fallthrough_code_fetch(bus);
    }

    fn push_return_and_restart_near_call(&mut self, bus: &mut impl common::Bus, target: u16) {
        self.biu_ready_memory_write();
        self.push(bus, self.ip);
        self.biu_bus_wait_finish(bus);
        self.ip = target;
        self.restart_fetch_after_delay(bus, 0);
    }

    /// Group 0x80: ALU r/m8, imm8
    pub(super) fn group_80(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let operation = (modrm >> 3) & 7;
        let dst = self.get_rm_byte_prepared(modrm, bus);
        let src = self.fetch(bus);
        let result = match operation {
            0 => self.alu_add_byte(dst, src),
            1 => self.alu_or_byte(dst, src),
            2 => {
                let cf = self.flags.cf_val();
                self.alu_adc_byte(dst, src, cf)
            }
            3 => {
                let cf = self.flags.cf_val();
                self.alu_sbb_byte(dst, src, cf)
            }
            4 => self.alu_and_byte(dst, src),
            5 => self.alu_sub_byte(dst, src),
            6 => self.alu_xor_byte(dst, src),
            7 => {
                self.alu_sub_byte(dst, src);
                let memory_cycles = self.immediate_cmp_memory_tail(modrm, 1);
                self.clk_immediate_modrm_tail(bus, modrm, memory_cycles);
                return;
            }
            _ => unreachable!(),
        };
        if modrm < 0xC0 {
            self.biu_prefetch_after_immediate_before_rmw_write(bus);
        }
        self.putback_rm_byte(modrm, result, bus);
        self.clk_immediate_modrm_tail(bus, modrm, 1);
    }

    /// Group 0x81: ALU r/m16, imm16
    pub(super) fn group_81(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let operation = (modrm >> 3) & 7;
        let dst = self.get_rm_word_prepared(modrm, bus);
        let src = self.fetchword(bus);
        let result = match operation {
            0 => self.alu_add_word(dst, src),
            1 => self.alu_or_word(dst, src),
            2 => {
                let cf = self.flags.cf_val();
                self.alu_adc_word(dst, src, cf)
            }
            3 => {
                let cf = self.flags.cf_val();
                self.alu_sbb_word(dst, src, cf)
            }
            4 => self.alu_and_word(dst, src),
            5 => self.alu_sub_word(dst, src),
            6 => self.alu_xor_word(dst, src),
            7 => {
                self.alu_sub_word(dst, src);
                let memory_cycles = self.immediate_cmp_memory_tail(modrm, 2);
                self.clk_immediate_modrm_tail(bus, modrm, memory_cycles);
                return;
            }
            _ => unreachable!(),
        };
        if modrm < 0xC0 {
            self.biu_prefetch_after_immediate_before_rmw_write(bus);
        }
        self.putback_rm_word(modrm, result, bus);
        self.clk_immediate_modrm_tail(bus, modrm, 1);
    }

    /// Group 0x82: ALU r/m8, imm8 (same as 0x80)
    pub(super) fn group_82(&mut self, bus: &mut impl common::Bus) {
        self.group_80(bus);
    }

    /// Group 0x83: ALU r/m16, sign-extended imm8
    pub(super) fn group_83(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let operation = (modrm >> 3) & 7;
        let dst = self.get_rm_word_prepared(modrm, bus);
        let src = self.fetch(bus) as i8 as u16;
        let result = match operation {
            0 => self.alu_add_word(dst, src),
            1 => self.alu_or_word(dst, src),
            2 => {
                let cf = self.flags.cf_val();
                self.alu_adc_word(dst, src, cf)
            }
            3 => {
                let cf = self.flags.cf_val();
                self.alu_sbb_word(dst, src, cf)
            }
            4 => self.alu_and_word(dst, src),
            5 => self.alu_sub_word(dst, src),
            6 => self.alu_xor_word(dst, src),
            7 => {
                self.alu_sub_word(dst, src);
                let memory_cycles = self.immediate_cmp_memory_tail(modrm, 1);
                self.clk_immediate_modrm_tail(bus, modrm, memory_cycles);
                return;
            }
            _ => unreachable!(),
        };
        if modrm < 0xC0 {
            self.biu_prefetch_after_immediate_before_rmw_write(bus);
        }
        self.putback_rm_word(modrm, result, bus);
        self.clk_immediate_modrm_tail(bus, modrm, 1);
    }

    fn clk_immediate_modrm_tail(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        memory_cycles: i32,
    ) {
        let register_cycles =
            if self.seg_prefix && self.instruction_entry_queue_had_current_instruction() {
                2
            } else {
                3
            };
        self.clk_modrm(bus, modrm, register_cycles, memory_cycles);
    }

    fn immediate_cmp_memory_tail(&self, modrm: u8, immediate_bytes: u8) -> i32 {
        if modrm >= 0xC0
            || self.biu_instruction_entry_queue_len_for_timing() != queue_size_for(MODEL)
        {
            return 1;
        }

        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);
        if has_word_displacement {
            return 1;
        }

        match immediate_bytes {
            1 if mode <= 1 => 4,
            2 if mode == 0 => 3,
            _ => 1,
        }
    }

    fn clk_test_immediate_byte_tail(&mut self, bus: &mut impl common::Bus, modrm: u8) {
        let register_cycles =
            if self.seg_prefix && self.instruction_entry_queue_had_current_instruction() {
                1
            } else {
                2
            };
        let memory_cycles = if self.test_immediate_full_queue_short_memory(modrm) {
            3
        } else {
            4
        };
        self.clk_modrm(bus, modrm, register_cycles, memory_cycles);
    }

    fn clk_test_immediate_word_tail(&mut self, bus: &mut impl common::Bus, modrm: u8) {
        let memory_cycles = if self.biu_instruction_entry_queue_len_for_timing()
            == queue_size_for(MODEL)
            && !self.seg_prefix
            && (modrm >> 6) == 0
            && (modrm & 7) != 6
        {
            2
        } else {
            3
        };
        self.clk_modrm_word(bus, modrm, 2, memory_cycles);
    }

    fn test_immediate_full_queue_short_memory(&self, modrm: u8) -> bool {
        if self.biu_instruction_entry_queue_len_for_timing() != queue_size_for(MODEL) {
            return false;
        }

        let mode = modrm >> 6;
        let rm = modrm & 7;
        mode == 1 || (mode == 0 && rm != 6)
    }

    /// Group 0xC0: shift/rotate r/m8, imm8
    pub(super) fn group_c0(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_byte_prepared(modrm, bus);
        let count = self.fetch(bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, count),
            1 => self.alu_ror_byte(dst, count),
            2 => self.alu_rcl_byte(dst, count),
            3 => self.alu_rcr_byte(dst, count),
            4 => self.alu_shl_byte(dst, count),
            5 => self.alu_shr_byte(dst, count),
            6 => self.alu_shl_byte(dst, count), // undocumented: same as SHL
            7 => self.alu_sar_byte(dst, count),
            _ => unreachable!(),
        };
        self.putback_shift_rotate_immediate_byte(bus, modrm, result, count);
    }

    /// Group 0xC1: shift/rotate r/m16, imm8
    pub(super) fn group_c1(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word_prepared(modrm, bus);
        let count = self.fetch(bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, count),
            1 => self.alu_ror_word(dst, count),
            2 => self.alu_rcl_word(dst, count),
            3 => self.alu_rcr_word(dst, count),
            4 => self.alu_shl_word(dst, count),
            5 => self.alu_shr_word(dst, count),
            6 => self.alu_shl_word(dst, count),
            7 => self.alu_sar_word(dst, count),
            _ => unreachable!(),
        };
        self.putback_shift_rotate_immediate_word(bus, modrm, result, count);
    }

    fn putback_shift_rotate_immediate_byte(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        result: u8,
        count: u8,
    ) {
        if modrm >= 0xC0 {
            self.putback_rm_byte(modrm, result, bus);
            self.clk(bus, self.shift_rotate_immediate_register_cycles(count));
            return;
        }

        if count == 0 {
            self.clk(bus, Self::SHIFT_ROTATE_IMMEDIATE_ZERO_MEMORY_CYCLES);
            return;
        }

        self.clk(bus, Self::shift_rotate_immediate_memory_cycles(count));
        let fetched_before_write = self.shift_rotate_immediate_prefetch_before_write(bus, count);
        self.biu_fetch_suspend(bus);
        self.shift_rotate_immediate_idle_before_write(bus, count, fetched_before_write);
        self.putback_rm_byte(modrm, result, bus);
        self.clk(bus, 1);
    }

    fn putback_shift_rotate_immediate_word(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        result: u16,
        count: u8,
    ) {
        if modrm >= 0xC0 {
            self.putback_rm_word(modrm, result, bus);
            self.clk(bus, self.shift_rotate_immediate_register_cycles(count));
            return;
        }

        if count == 0 {
            self.clk(bus, Self::SHIFT_ROTATE_IMMEDIATE_ZERO_MEMORY_CYCLES);
            return;
        }

        self.clk(bus, Self::shift_rotate_immediate_memory_cycles(count));
        let fetched_before_write = self.shift_rotate_immediate_prefetch_before_write(bus, count);
        self.biu_fetch_suspend(bus);
        self.shift_rotate_immediate_idle_before_write(bus, count, fetched_before_write);
        self.putback_rm_word(modrm, result, bus);
        self.clk(bus, 1);
    }

    fn shift_rotate_immediate_register_cycles(&self, count: u8) -> i32 {
        let cycles = if count == 0 { 6 } else { 7 + i32::from(count) };
        if self.seg_prefix && self.instruction_entry_queue_had_current_instruction() {
            cycles - 1
        } else {
            cycles
        }
    }

    const SHIFT_ROTATE_IMMEDIATE_ZERO_MEMORY_CYCLES: i32 = 7;

    fn shift_rotate_immediate_memory_cycles(count: u8) -> i32 {
        6 + i32::from(count)
    }

    fn shift_rotate_immediate_prefetch_before_write(
        &mut self,
        bus: &mut impl common::Bus,
        count: u8,
    ) -> bool {
        if matches!(count, 1 | 4 | 5) && self.queue_len() < 3 {
            self.clk(bus, 4);
            true
        } else {
            false
        }
    }

    fn shift_rotate_immediate_idle_before_write(
        &mut self,
        bus: &mut impl common::Bus,
        count: u8,
        fetched_before_write: bool,
    ) {
        let idle_cycles = if fetched_before_write || self.queue_len() == queue_size_for(MODEL) {
            0
        } else {
            match count {
                1 | 5 | 9 => 3,
                3 | 4 | 7 | 8 => 2,
                _ => 0,
            }
        };
        if idle_cycles > 0 {
            self.clk(bus, idle_cycles);
            self.biu_ready_memory_write();
        }
    }

    /// Group 0xD0: shift/rotate r/m8, 1
    pub(super) fn group_d0(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_byte_prepared(modrm, bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, 1),
            1 => self.alu_ror_byte(dst, 1),
            2 => self.alu_rcl_byte(dst, 1),
            3 => self.alu_rcr_byte(dst, 1),
            4 => self.alu_shl_byte(dst, 1),
            5 => self.alu_shr_byte(dst, 1),
            6 => self.alu_shl_byte(dst, 1),
            7 => self.alu_sar_byte(dst, 1),
            _ => unreachable!(),
        };
        if modrm >= 0xC0 {
            self.putback_rm_byte(modrm, result, bus);
            let cycles =
                if self.seg_prefix && self.instruction_entry_queue_had_current_instruction() {
                    3
                } else {
                    4
                };
            self.clk(bus, cycles);
        } else {
            self.clk(bus, 7);
            self.biu_prefetch_before_rmw_write(bus);
            self.putback_rm_byte(modrm, result, bus);
            self.clk(bus, 1);
        }
    }

    /// Group 0xD1: shift/rotate r/m16, 1
    pub(super) fn group_d1(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let dst = self.get_rm_word_prepared(modrm, bus);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, 1),
            1 => self.alu_ror_word(dst, 1),
            2 => self.alu_rcl_word(dst, 1),
            3 => self.alu_rcr_word(dst, 1),
            4 => self.alu_shl_word(dst, 1),
            5 => self.alu_shr_word(dst, 1),
            6 => self.alu_shl_word(dst, 1),
            7 => self.alu_sar_word(dst, 1),
            _ => unreachable!(),
        };
        if modrm >= 0xC0 {
            self.putback_rm_word(modrm, result, bus);
            let cycles =
                if self.seg_prefix && self.instruction_entry_queue_had_current_instruction() {
                    3
                } else {
                    4
                };
            self.clk(bus, cycles);
        } else {
            self.clk(bus, 7);
            self.biu_prefetch_before_rmw_write(bus);
            self.putback_rm_word(modrm, result, bus);
            self.clk(bus, 1);
        }
    }

    /// Group 0xD2: shift/rotate r/m8, CL
    pub(super) fn group_d2(&mut self, bus: &mut impl common::Bus) {
        let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
        let modrm = self.fetch(bus);
        let dst = self.get_shift_rotate_cl_rm_byte(bus, modrm, entry_queue_len);
        let count = self.regs.byte(ByteReg::CL);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_byte(dst, count),
            1 => self.alu_ror_byte(dst, count),
            2 => self.alu_rcl_byte(dst, count),
            3 => self.alu_rcr_byte(dst, count),
            4 => self.alu_shl_byte(dst, count),
            5 => self.alu_shr_byte(dst, count),
            6 => self.alu_shl_byte(dst, count),
            7 => self.alu_sar_byte(dst, count),
            _ => unreachable!(),
        };
        self.putback_shift_rotate_cl_byte(bus, modrm, result, count, entry_queue_len);
    }

    /// Group 0xD3: shift/rotate r/m16, CL
    pub(super) fn group_d3(&mut self, bus: &mut impl common::Bus) {
        let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
        let modrm = self.fetch(bus);
        let dst = self.get_shift_rotate_cl_rm_word(bus, modrm, entry_queue_len);
        let count = self.regs.byte(ByteReg::CL);
        let result = match (modrm >> 3) & 7 {
            0 => self.alu_rol_word(dst, count),
            1 => self.alu_ror_word(dst, count),
            2 => self.alu_rcl_word(dst, count),
            3 => self.alu_rcr_word(dst, count),
            4 => self.alu_shl_word(dst, count),
            5 => self.alu_shr_word(dst, count),
            6 => self.alu_shl_word(dst, count),
            7 => self.alu_sar_word(dst, count),
            _ => unreachable!(),
        };
        self.putback_shift_rotate_cl_word(bus, modrm, result, count, entry_queue_len);
    }

    fn get_shift_rotate_cl_rm_byte(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        entry_queue_len: usize,
    ) -> u8 {
        if modrm >= 0xC0 {
            self.regs.byte(self.rm_byte(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.prepare_shift_rotate_cl_memory_read(bus, modrm, entry_queue_len);
            let value = self.biu_read_u8_physical(bus, self.ea);
            self.biu_fetch_resume_immediate_for_eu();
            value
        }
    }

    fn get_shift_rotate_cl_rm_word(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        entry_queue_len: usize,
    ) -> u16 {
        if modrm >= 0xC0 {
            self.regs.word(self.rm_word(modrm))
        } else {
            self.calc_ea(modrm, bus);
            self.prepare_shift_rotate_cl_memory_read(bus, modrm, entry_queue_len);
            let value = self.seg_read_word(bus);
            self.biu_fetch_resume_immediate_for_eu();
            value
        }
    }

    fn prepare_shift_rotate_cl_memory_read(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        entry_queue_len: usize,
    ) {
        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);

        if entry_queue_len == 0
            || (entry_queue_len == queue_size_for(MODEL) && has_word_displacement)
        {
            self.biu_fetch_suspend(bus);
            self.clk(bus, 2);
            self.biu_ready_memory_read();
        } else if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix && mode == 1 {
            self.biu_complete_code_fetch_and_start_for_eu(bus);
            self.biu_fetch_suspend(bus);
            self.biu_complete_code_fetch_for_eu();
            self.biu_ready_memory_read();
        } else if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix && mode == 0 {
            self.biu_prepare_memory_read();
        }
    }

    fn putback_shift_rotate_cl_byte(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        result: u8,
        count: u8,
        entry_queue_len: usize,
    ) {
        let count_cycles = i32::from(count);
        if modrm >= 0xC0 {
            self.putback_rm_byte(modrm, result, bus);
            let mut cycles = Self::shift_rotate_cl_register_cycles(count);
            if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix {
                cycles -= 1;
            }
            self.clk(bus, cycles);
            return;
        }

        let memory_cycles = if count == 1 && entry_queue_len == queue_size_for(MODEL) {
            11
        } else if count == 1 {
            12
        } else {
            7 + count_cycles
        };
        self.clk(bus, memory_cycles);
        if count == 0 {
            self.clk(bus, 1);
            return;
        }
        self.biu_fetch_suspend(bus);
        if count == 1
            && entry_queue_len == queue_size_for(MODEL)
            && self.queue_len() == queue_size_for(MODEL)
        {
            self.biu_ready_memory_write();
        }
        let write_idle_cycles =
            self.shift_rotate_cl_write_idle_cycles(modrm, count, entry_queue_len);
        if write_idle_cycles > 0 {
            self.clk(bus, write_idle_cycles);
            self.biu_ready_memory_write();
        }
        self.putback_rm_byte(modrm, result, bus);
        self.clk(bus, 1);
    }

    fn putback_shift_rotate_cl_word(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        result: u16,
        count: u8,
        entry_queue_len: usize,
    ) {
        let count_cycles = i32::from(count);
        if modrm >= 0xC0 {
            self.putback_rm_word(modrm, result, bus);
            let mut cycles = Self::shift_rotate_cl_register_cycles(count);
            if entry_queue_len == queue_size_for(MODEL) && self.seg_prefix {
                cycles -= 1;
            }
            self.clk(bus, cycles);
            return;
        }

        let memory_cycles = if count == 1 && entry_queue_len == queue_size_for(MODEL) {
            11
        } else if count == 1 {
            12
        } else {
            7 + count_cycles
        };
        self.clk(bus, memory_cycles);
        if count == 0 {
            self.clk(bus, 1);
            return;
        }
        self.biu_fetch_suspend(bus);
        if count == 1
            && entry_queue_len == queue_size_for(MODEL)
            && self.queue_len() == queue_size_for(MODEL)
        {
            self.biu_ready_memory_write();
        }
        let write_idle_cycles =
            self.shift_rotate_cl_write_idle_cycles(modrm, count, entry_queue_len);
        if write_idle_cycles > 0 {
            self.clk(bus, write_idle_cycles);
            self.biu_ready_memory_write();
        }
        self.putback_rm_word(modrm, result, bus);
        self.clk(bus, 1);
    }

    fn shift_rotate_cl_write_idle_cycles(
        &self,
        modrm: u8,
        count: u8,
        entry_queue_len: usize,
    ) -> i32 {
        if count == 1 {
            return 0;
        }
        if count == 2 {
            return 0;
        }
        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);

        if count == 3 {
            return if entry_queue_len == 0 || has_word_displacement {
                2
            } else {
                3
            };
        }
        if count >= 5 {
            return 3;
        }

        if entry_queue_len == 0 {
            if mode == 0 && rm != 6 {
                if self.queue_len() == queue_size_for(MODEL) {
                    3
                } else {
                    2
                }
            } else if self.queue_len() == queue_size_for(MODEL) {
                3
            } else {
                2
            }
        } else if entry_queue_len == queue_size_for(MODEL) && has_word_displacement {
            if self.queue_len() == queue_size_for(MODEL) {
                3
            } else {
                2
            }
        } else {
            0
        }
    }

    fn shift_rotate_cl_register_cycles(count: u8) -> i32 {
        let count_cycles = i32::from(count);
        if count == 0 { 7 } else { 8 + count_cycles }
    }

    /// Group 0xF6: various byte operations
    pub(super) fn group_f6(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let op = (modrm >> 3) & 7;
        let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
        match op {
            0 | 1 => {
                // TEST r/m8, imm8
                let dst = self.get_rm_byte_prepared(modrm, bus);
                let src = self.fetch(bus);
                self.alu_and_byte(dst, src);
                self.clk_test_immediate_byte_tail(bus, modrm);
            }
            2 => {
                // NOT r/m8
                let dst = self.get_rm_byte_prepared(modrm, bus);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_byte(modrm, !dst, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            3 => {
                // NEG r/m8
                let dst = self.get_rm_byte_prepared(modrm, bus);
                let result = self.alu_neg_byte(dst);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            4 => {
                // MUL r/m8 (unsigned, NEC MULU)
                let src = self.get_rm_byte_prepared(modrm, bus);
                let al = self.regs.byte(ByteReg::AL);
                let result = al as u16 * src as u16;
                let tail_adjustment =
                    Self::group_multiply_tail_adjustment(entry_queue_len, self.seg_prefix, modrm);
                self.regs.set_word(WordReg::AX, result);
                self.flags.carry_val = if result & 0xFF00 != 0 { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm(bus, modrm, 22 - tail_adjustment, 25 - tail_adjustment);
            }
            5 => {
                // IMUL r/m8 (signed, NEC MUL)
                let src = self.get_rm_byte_prepared(modrm, bus) as i8;
                let al = self.regs.byte(ByteReg::AL) as i8;
                let result = al as i16 * src as i16;
                let cycles = Self::signed_multiply_byte_cycles(al, src);
                let tail_adjustment =
                    Self::group_multiply_tail_adjustment(entry_queue_len, self.seg_prefix, modrm);
                self.regs.set_word(WordReg::AX, result as u16);
                let ah = (result >> 8) as i8;
                let al_sign = result as i8;
                self.flags.carry_val = if ah != (al_sign >> 7) { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm(
                    bus,
                    modrm,
                    cycles - tail_adjustment,
                    cycles + 3 - tail_adjustment,
                );
            }
            6 => {
                // DIV r/m8 (unsigned, NEC DIVU)
                let src = self.get_rm_byte_divide(modrm, bus) as u16;
                if src == 0 {
                    self.clk_unsigned_divide_error(bus, modrm, entry_queue_len, false);
                    let entry_cycles =
                        self.unsigned_byte_divide_error_entry_cycles(modrm, entry_queue_len);
                    if self
                        .unsigned_byte_divide_error_needs_ready_vector_read(modrm, entry_queue_len)
                    {
                        self.raise_divide_error_with_ready_vector_read(bus, entry_cycles);
                    } else {
                        self.raise_divide_error_with_entry_cycles(bus, entry_cycles);
                    }
                    return;
                }
                let aw = self.regs.word(WordReg::AX);
                let quotient = aw / src;
                if quotient > 0xFF {
                    self.clk_unsigned_divide_error(bus, modrm, entry_queue_len, false);
                    let entry_cycles =
                        self.unsigned_byte_divide_error_entry_cycles(modrm, entry_queue_len);
                    if self
                        .unsigned_byte_divide_error_needs_ready_vector_read(modrm, entry_queue_len)
                    {
                        self.raise_divide_error_with_ready_vector_read(bus, entry_cycles);
                    } else {
                        self.raise_divide_error_with_entry_cycles(bus, entry_cycles);
                    }
                    return;
                }
                let remainder = aw % src;
                self.regs.set_byte(ByteReg::AL, quotient as u8);
                self.regs.set_byte(ByteReg::AH, remainder as u8);
                self.clk_unsigned_divide_success(bus, modrm, entry_queue_len, false);
            }
            7 => {
                // IDIV r/m8 (signed, NEC DIV)
                let src = self.get_rm_byte_divide(modrm, bus) as i8 as i16;
                let aw = self.regs.word(WordReg::AX) as i16;
                let dividend_negative = aw < 0;
                if src == 0 {
                    self.clk_signed_divide(
                        bus,
                        modrm,
                        entry_queue_len,
                        SignedDivideTiming {
                            word: false,
                            dividend_negative,
                            quotient_bits: 0,
                            success: false,
                        },
                    );
                    self.raise_divide_error(bus);
                    return;
                }
                let quotient = i32::from(aw) / i32::from(src);
                let quotient_bits = Self::divide_abs_bits(quotient);
                if !(-127..=127).contains(&quotient) {
                    self.clk_signed_divide(
                        bus,
                        modrm,
                        entry_queue_len,
                        SignedDivideTiming {
                            word: false,
                            dividend_negative,
                            quotient_bits,
                            success: false,
                        },
                    );
                    self.raise_divide_error(bus);
                    return;
                }
                let remainder = i32::from(aw) % i32::from(src);
                self.regs.set_byte(ByteReg::AL, quotient as u8);
                self.regs.set_byte(ByteReg::AH, remainder as u8);
                self.clk_signed_divide(
                    bus,
                    modrm,
                    entry_queue_len,
                    SignedDivideTiming {
                        word: false,
                        dividend_negative,
                        quotient_bits,
                        success: true,
                    },
                );
            }
            _ => unreachable!(),
        }
    }

    /// Group 0xF7: various word operations
    pub(super) fn group_f7(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let op = (modrm >> 3) & 7;
        let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
        match op {
            0 | 1 => {
                // TEST r/m16, imm16
                let dst = self.get_rm_word_prepared(modrm, bus);
                let src = self.fetchword(bus);
                self.alu_and_word(dst, src);
                self.clk_test_immediate_word_tail(bus, modrm);
            }
            2 => {
                // NOT r/m16
                let dst = self.get_rm_word_prepared(modrm, bus);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_word(modrm, !dst, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            3 => {
                // NEG r/m16
                let dst = self.get_rm_word_prepared(modrm, bus);
                let result = self.alu_neg_word(dst);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_word(modrm, result, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            4 => {
                // MUL r/m16 (unsigned, NEC MULU)
                let src = self.get_rm_word_prepared(modrm, bus);
                let aw = self.regs.word(WordReg::AX);
                let result = aw as u32 * src as u32;
                let tail_adjustment =
                    Self::group_multiply_tail_adjustment(entry_queue_len, self.seg_prefix, modrm);
                self.regs.set_word(WordReg::AX, result as u16);
                self.regs.set_word(WordReg::DX, (result >> 16) as u16);
                self.flags.carry_val = if result & 0xFFFF0000 != 0 { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm_word(bus, modrm, 29 - tail_adjustment, 32 - tail_adjustment);
            }
            5 => {
                // IMUL r/m16 (signed, NEC MUL)
                let src = self.get_rm_word_prepared(modrm, bus) as i16;
                let aw = self.regs.word(WordReg::AX) as i16;
                let result = aw as i32 * src as i32;
                let cycles = Self::signed_multiply_word_cycles(aw, src);
                let tail_adjustment =
                    Self::group_multiply_tail_adjustment(entry_queue_len, self.seg_prefix, modrm);
                self.regs.set_word(WordReg::AX, result as u16);
                self.regs.set_word(WordReg::DX, (result >> 16) as u16);
                let upper = (result >> 16) as i16;
                let lower_sign = result as i16;
                self.flags.carry_val = if upper != (lower_sign >> 15) { 1 } else { 0 };
                self.flags.overflow_val = self.flags.carry_val;
                self.clk_modrm_word(
                    bus,
                    modrm,
                    cycles - tail_adjustment,
                    cycles + 3 - tail_adjustment,
                );
            }
            6 => {
                // DIV r/m16 (unsigned, NEC DIVU)
                let src = self.get_rm_word_divide(modrm, bus) as u32;
                if src == 0 {
                    self.clk_unsigned_divide_error(bus, modrm, entry_queue_len, true);
                    let entry_cycles =
                        self.unsigned_word_divide_error_entry_cycles(modrm, entry_queue_len);
                    if self
                        .unsigned_word_divide_error_needs_ready_vector_read(modrm, entry_queue_len)
                    {
                        self.raise_divide_error_with_ready_vector_read(bus, entry_cycles);
                    } else {
                        self.raise_divide_error_with_entry_cycles(bus, entry_cycles);
                    }
                    return;
                }
                let dw = self.regs.word(WordReg::DX) as u32;
                let aw = self.regs.word(WordReg::AX) as u32;
                let dividend = (dw << 16) | aw;
                let quotient = dividend / src;
                if quotient > 0xFFFF {
                    self.clk_unsigned_divide_error(bus, modrm, entry_queue_len, true);
                    let entry_cycles =
                        self.unsigned_word_divide_error_entry_cycles(modrm, entry_queue_len);
                    if self
                        .unsigned_word_divide_error_needs_ready_vector_read(modrm, entry_queue_len)
                    {
                        self.raise_divide_error_with_ready_vector_read(bus, entry_cycles);
                    } else {
                        self.raise_divide_error_with_entry_cycles(bus, entry_cycles);
                    }
                    return;
                }
                let remainder = dividend % src;
                self.regs.set_word(WordReg::AX, quotient as u16);
                self.regs.set_word(WordReg::DX, remainder as u16);
                self.clk_unsigned_divide_success(bus, modrm, entry_queue_len, true);
            }
            7 => {
                // IDIV r/m16 (signed, NEC DIV)
                let src = self.get_rm_word_divide(modrm, bus) as i16 as i32;
                let dw = self.regs.word(WordReg::DX) as u32;
                let aw = self.regs.word(WordReg::AX) as u32;
                let dividend = ((dw << 16) | aw) as i32;
                let dividend_negative = dividend < 0;
                if src == 0 {
                    self.clk_signed_divide(
                        bus,
                        modrm,
                        entry_queue_len,
                        SignedDivideTiming {
                            word: true,
                            dividend_negative,
                            quotient_bits: 0,
                            success: false,
                        },
                    );
                    self.raise_divide_error(bus);
                    return;
                }
                let Some(quotient) = dividend.checked_div(src) else {
                    self.clk_signed_divide(
                        bus,
                        modrm,
                        entry_queue_len,
                        SignedDivideTiming {
                            word: true,
                            dividend_negative,
                            quotient_bits: 32,
                            success: false,
                        },
                    );
                    self.raise_divide_error(bus);
                    return;
                };
                let quotient_bits = Self::divide_abs_bits(quotient);
                if !(-32767..=32767).contains(&quotient) {
                    self.clk_signed_divide(
                        bus,
                        modrm,
                        entry_queue_len,
                        SignedDivideTiming {
                            word: true,
                            dividend_negative,
                            quotient_bits,
                            success: false,
                        },
                    );
                    self.raise_divide_error(bus);
                    return;
                }
                let remainder = dividend.checked_rem(src).unwrap_or(0);
                self.regs.set_word(WordReg::AX, quotient as u16);
                self.regs.set_word(WordReg::DX, remainder as u16);
                self.clk_signed_divide(
                    bus,
                    modrm,
                    entry_queue_len,
                    SignedDivideTiming {
                        word: true,
                        dividend_negative,
                        quotient_bits,
                        success: true,
                    },
                );
            }
            _ => unreachable!(),
        }
    }

    /// Group 0xFE: INC/DEC r/m8
    pub(super) fn group_fe(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // INC r/m8
                let dst = self.get_rm_byte_prepared(modrm, bus);
                let result = self.alu_inc_byte(dst);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            1 => {
                // DEC r/m8
                let dst = self.get_rm_byte_prepared(modrm, bus);
                let result = self.alu_dec_byte(dst);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_byte(modrm, result, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            _ => {
                self.clk(bus, 2);
            }
        }
    }

    /// Group 0xFF: various word operations
    pub(super) fn group_ff(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        match (modrm >> 3) & 7 {
            0 => {
                // INC r/m16
                let dst = self.get_rm_word_prepared(modrm, bus);
                let result = self.alu_inc_word(dst);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_word(modrm, result, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            1 => {
                // DEC r/m16
                let dst = self.get_rm_word_prepared(modrm, bus);
                let result = self.alu_dec_word(dst);
                self.prepare_unary_rmw_write(bus, modrm);
                self.putback_rm_word(modrm, result, bus);
                self.clk_unary_rmw_tail(bus, modrm);
            }
            2 => {
                // CALL r/m16 (near indirect)
                if modrm >= 0xC0 {
                    let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
                    let has_prefix = self.opcode_start_ip != self.prev_ip;
                    let dst = self.get_rm_word(modrm, bus);
                    if entry_queue_len == queue_size_for(MODEL) && !has_prefix {
                        self.complete_one_fallthrough_code_fetch(bus);
                        self.clk(bus, 2);
                    } else {
                        self.complete_next_fallthrough_code_fetch(bus);
                    }
                    self.push_return_and_restart_near_call(bus, dst);
                } else {
                    let dst = self.get_rm_word_prepared(modrm, bus);
                    self.complete_one_fallthrough_code_fetch(bus);
                    self.clk(bus, 2);
                    self.push_return_and_restart_near_call(bus, dst);
                }
            }
            3 => {
                // CALL m16:16 (far indirect)
                if modrm >= 0xC0 {
                    return;
                }
                let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
                let queue_full_at_entry = entry_queue_len == queue_size_for(MODEL);
                let queue_empty_at_entry = entry_queue_len == 0;
                let addressing_mode = modrm >> 6;
                let rm_field = modrm & 7;
                let word_displacement_pointer_read =
                    addressing_mode == 2 || (addressing_mode == 0 && rm_field == 6);
                let defer_first_pointer_read = queue_full_at_entry
                    && ((!self.seg_prefix && addressing_mode == 0 && rm_field != 6)
                        || (self.seg_prefix && addressing_mode == 1));
                let short_first_pointer_gap =
                    queue_full_at_entry && !self.seg_prefix && addressing_mode == 1;
                let delayed_first_pointer_prepare =
                    (queue_full_at_entry && word_displacement_pointer_read) || queue_empty_at_entry;
                let short_second_pointer_gap = (queue_full_at_entry
                    && ((addressing_mode == 0 && rm_field != 6)
                        || addressing_mode == 1
                        || word_displacement_pointer_read))
                    || queue_empty_at_entry;
                self.calc_ea(modrm, bus);
                if delayed_first_pointer_prepare {
                    self.clk(bus, 1);
                    self.biu_prepare_memory_read();
                } else if !defer_first_pointer_read {
                    self.biu_prepare_memory_read();
                }
                let first_pointer_gap = if short_first_pointer_gap { 1 } else { 2 };
                self.clk(bus, first_pointer_gap);
                let offset = self.seg_read_word(bus);
                self.clk(bus, if short_second_pointer_gap { 3 } else { 4 });
                self.biu_prepare_memory_read();
                self.clk(bus, 2);
                let segment = self.seg_read_word_at(bus, 2);
                self.biu_fetch_suspend(bus);
                self.biu_prepare_memory_write();
                self.clk(bus, 6);
                let cs = self.sregs[SegReg16::CS as usize];
                self.push(bus, cs);
                self.biu_prepare_memory_write();
                self.biu_bus_wait_finish(bus);
                self.clk(bus, 1);
                self.push(bus, self.ip);
                self.ip = offset;
                self.sregs[SegReg16::CS as usize] = segment;
                self.biu_bus_wait_finish(bus);
                self.restart_fetch_after_delay(bus, 0);
            }
            4 => {
                // JMP r/m16 (near indirect)
                let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
                let dst = if modrm >= 0xC0 {
                    self.get_rm_word(modrm, bus)
                } else {
                    self.get_rm_word_prepared(modrm, bus)
                };
                self.ip = dst;
                if modrm >= 0xC0 {
                    if entry_queue_len == queue_size_for(MODEL) && !self.seg_prefix {
                        self.complete_one_fallthrough_code_fetch(bus);
                        self.restart_fetch_after_delay(bus, 0);
                    } else {
                        self.biu_bus_wait_finish(bus);
                        self.biu_complete_code_fetch_for_eu();
                        self.restart_fetch_after_delay(bus, 1);
                    }
                } else {
                    self.complete_one_fallthrough_code_fetch(bus);
                    self.restart_fetch_after_delay(bus, 0);
                }
            }
            5 => {
                // JMP m16:16 (far indirect)
                if modrm >= 0xC0 {
                    return;
                }
                self.calc_ea(modrm, bus);
                self.prepare_modrm_memory_read(bus, modrm);
                let offset = self.seg_read_word(bus);
                self.biu_prefetch_before_pointer_segment_read(bus);
                self.clk(bus, 2);
                let segment = self.seg_read_word_at(bus, 2);
                self.ip = offset;
                self.sregs[SegReg16::CS as usize] = segment;
                self.restart_fetch_after_delay(bus, 3);
            }
            6 | 7 => {
                // PUSH r/m16 (7 is undocumented alias)
                let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
                let hot_unprefixed_register =
                    !self.seg_prefix && self.instruction_entry_queue_had_current_instruction();
                if modrm >= 0xC0 && (modrm & 7) == 4 {
                    if hot_unprefixed_register {
                        self.complete_one_fallthrough_code_fetch(bus);
                        self.biu_fetch_suspend(bus);
                        self.clk(bus, 2);
                    } else {
                        self.complete_next_fallthrough_code_fetch(bus);
                    }
                    self.biu_ready_memory_write();
                    self.push_sp(bus);
                    self.finish_state = FinishState::NoTerminalFetch;
                } else if modrm >= 0xC0 {
                    let value = self.get_rm_word(modrm, bus);
                    if hot_unprefixed_register {
                        self.complete_one_fallthrough_code_fetch(bus);
                        self.biu_fetch_suspend(bus);
                        self.clk(bus, 2);
                    } else {
                        self.complete_next_fallthrough_code_fetch(bus);
                    }
                    self.biu_ready_memory_write();
                    self.push(bus, value);
                    self.clk(bus, 1);
                    self.finish_state = FinishState::NoTerminalFetch;
                } else {
                    let value = self.get_rm_word_prepared(modrm, bus);
                    self.complete_next_fallthrough_code_fetch(bus);
                    let addressing_mode = modrm >> 6;
                    let rm_field = modrm & 7;
                    if entry_queue_len == queue_size_for(MODEL)
                        && !self.seg_prefix
                        && addressing_mode == 0
                        && rm_field != 6
                    {
                        self.biu_fetch_suspend(bus);
                        self.clk(bus, 2);
                    } else if entry_queue_len == queue_size_for(MODEL)
                        && ((!self.seg_prefix && addressing_mode == 1)
                            || (self.seg_prefix && addressing_mode == 0 && rm_field != 6))
                    {
                        self.complete_one_fallthrough_code_fetch(bus);
                    }
                    self.biu_ready_memory_write();
                    self.push(bus, value);
                    self.clk(bus, 1);
                    self.finish_state = FinishState::NoTerminalFetch;
                }
            }
            _ => {
                self.clk(bus, 2);
            }
        }
    }

    #[inline(always)]
    fn signed_multiply_byte_cycles(left: i8, right: i8) -> i32 {
        if left.is_negative() ^ right.is_negative() {
            36
        } else {
            32
        }
    }

    #[inline(always)]
    fn signed_multiply_word_cycles(left: i16, right: i16) -> i32 {
        if left.is_negative() ^ right.is_negative() {
            43
        } else {
            39
        }
    }

    #[inline(always)]
    fn group_multiply_tail_adjustment(entry_queue_len: usize, seg_prefix: bool, modrm: u8) -> i32 {
        if modrm >= 0xC0 {
            return if entry_queue_len == queue_size_for(MODEL) && seg_prefix {
                1
            } else {
                0
            };
        }

        let mode = modrm >> 6;
        let rm = modrm & 7;
        let has_word_displacement = mode == 2 || (mode == 0 && rm == 6);
        let adjustment = if entry_queue_len == 0
            || (has_word_displacement && (!seg_prefix || entry_queue_len == queue_size_for(MODEL)))
        {
            2
        } else {
            0
        };

        if entry_queue_len != queue_size_for(MODEL) || has_word_displacement {
            return adjustment;
        }

        adjustment + 2
    }
}
