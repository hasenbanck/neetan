use super::{V30, biu::QUEUE_SIZE};
use crate::{ByteReg, SegReg16, WordReg};

#[derive(Clone, Copy)]
struct Bcd4sTiming {
    cold_start: bool,
}

#[derive(Clone, Copy)]
enum Bit1Op {
    Test,
    Clear,
    Set,
    Not,
}

#[derive(Clone, Copy)]
enum Bit1Source {
    Cl,
    Immediate,
}

const BITFIELD_MIN_PREFETCH_QUEUE_LEN: usize = 2;
const BITFIELD_FULL_PREFETCH_QUEUE_TARGET: usize = QUEUE_SIZE - 1;

impl V30 {
    pub(super) fn extended_0f(&mut self, bus: &mut impl common::Bus) {
        let sub = self.fetch(bus);
        match sub {
            // TEST1 r/m8, CL
            0x10 => {
                self.bit1_byte(bus, Bit1Op::Test, Bit1Source::Cl);
            }
            // TEST1 r/m16, CL
            0x11 => {
                self.bit1_word(bus, Bit1Op::Test, Bit1Source::Cl);
            }
            // CLR1 r/m8, CL
            0x12 => {
                self.bit1_byte(bus, Bit1Op::Clear, Bit1Source::Cl);
            }
            // CLR1 r/m16, CL
            0x13 => {
                self.bit1_word(bus, Bit1Op::Clear, Bit1Source::Cl);
            }
            // SET1 r/m8, CL
            0x14 => {
                self.bit1_byte(bus, Bit1Op::Set, Bit1Source::Cl);
            }
            // SET1 r/m16, CL
            0x15 => {
                self.bit1_word(bus, Bit1Op::Set, Bit1Source::Cl);
            }
            // NOT1 r/m8, CL
            0x16 => {
                self.bit1_byte(bus, Bit1Op::Not, Bit1Source::Cl);
            }
            // NOT1 r/m16, CL
            0x17 => {
                self.bit1_word(bus, Bit1Op::Not, Bit1Source::Cl);
            }
            // TEST1 r/m8, imm3
            0x18 => {
                self.bit1_byte(bus, Bit1Op::Test, Bit1Source::Immediate);
            }
            // TEST1 r/m16, imm4
            0x19 => {
                self.bit1_word(bus, Bit1Op::Test, Bit1Source::Immediate);
            }
            // CLR1 r/m8, imm3
            0x1A => {
                self.bit1_byte(bus, Bit1Op::Clear, Bit1Source::Immediate);
            }
            // CLR1 r/m16, imm4
            0x1B => {
                self.bit1_word(bus, Bit1Op::Clear, Bit1Source::Immediate);
            }
            // SET1 r/m8, imm3
            0x1C => {
                self.bit1_byte(bus, Bit1Op::Set, Bit1Source::Immediate);
            }
            // SET1 r/m16, imm4
            0x1D => {
                self.bit1_word(bus, Bit1Op::Set, Bit1Source::Immediate);
            }
            // NOT1 r/m8, imm3
            0x1E => {
                self.bit1_byte(bus, Bit1Op::Not, Bit1Source::Immediate);
            }
            // NOT1 r/m16, imm4
            0x1F => {
                self.bit1_word(bus, Bit1Op::Not, Bit1Source::Immediate);
            }
            // ADD4S
            0x20 => {
                self.add4s(bus);
            }
            // SUB4S
            0x22 => {
                self.sub4s(bus);
            }
            // CMP4S
            0x26 => {
                self.cmp4s(bus);
            }
            // ROL4: rotate nibbles left between AL and operand
            0x28 => {
                self.rol4(bus);
            }
            // ROR4: rotate nibbles right between AL and operand
            0x2A => {
                self.ror4(bus);
            }
            // INS reg1, reg2
            0x31 => {
                self.ins_reg(bus);
            }
            // EXT reg1, reg2
            0x33 => {
                self.ext_reg(bus);
            }
            // INS reg, imm4
            0x39 => {
                self.ins_imm(bus);
            }
            // EXT reg, imm4
            0x3B => {
                self.ext_imm(bus);
            }
            // BRKEM
            0xFF => {
                let vector = self.fetch(bus);
                self.raise_interrupt(vector, bus);
                self.clk(bus, 38);
            }
            _ => {
                // Unknown 0F sub-opcode
                self.clk(bus, 2);
            }
        }
    }

    fn bcd4s_startup(&mut self, bus: &mut impl common::Bus) -> Bcd4sTiming {
        let code_fetch_active = self.biu_latch_is_code_fetch();
        if self.seg_prefix && code_fetch_active && self.queue_len() > 0 {
            self.clk(bus, 5);
            self.biu_complete_code_fetch_for_eu();
            self.clk(bus, 2);
            self.biu_ready_memory_read();
            return Bcd4sTiming { cold_start: false };
        }

        self.clk(bus, if code_fetch_active { 6 } else { 8 });
        self.biu_complete_code_fetch_for_eu();
        self.biu_ready_memory_read();
        Bcd4sTiming {
            cold_start: code_fetch_active,
        }
    }

    fn bcd4s_after_source_read(&mut self, bus: &mut impl common::Bus) {
        if self.queue_has_room_for_fetch() {
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.biu_ready_memory_read();
        } else {
            self.biu_prepare_memory_read();
            self.clk(bus, 3);
        }
    }

    fn bcd4s_before_write(&mut self, bus: &mut impl common::Bus) {
        if self.queue_has_room_for_fetch() {
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.clk(bus, 2);
            self.biu_ready_memory_write();
        } else {
            self.biu_prepare_memory_write();
            self.clk(bus, 5);
        }
    }

    fn bcd4s_after_write(
        &mut self,
        bus: &mut impl common::Bus,
        final_iteration: bool,
        first_iteration: bool,
        timing: Bcd4sTiming,
        final_carry: bool,
    ) {
        if !final_iteration && first_iteration && timing.cold_start {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_current_bus_for_eu();
            self.biu_ready_memory_read();
            return;
        }

        if !final_iteration {
            self.biu_prepare_memory_read();
        }
        self.biu_bus_wait_finish(bus);
        if final_iteration {
            let mut tail = if timing.cold_start && first_iteration {
                3
            } else {
                4
            };
            if !final_carry {
                tail += 1;
            }
            self.clk(bus, tail);
        } else {
            self.clk(bus, 2);
        }
    }

    fn bcd4s_after_compare(
        &mut self,
        bus: &mut impl common::Bus,
        final_iteration: bool,
        final_carry: bool,
    ) {
        if final_iteration {
            let tail = if final_carry { 9 } else { 10 };
            if self.queue_has_room_for_fetch() {
                self.clk(bus, 4);
                self.biu_complete_code_fetch_for_eu();
                self.clk(bus, tail);
            } else {
                self.clk(bus, tail + 4);
            }
        } else if self.queue_has_room_for_fetch() {
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.clk(bus, 6);
            self.biu_ready_memory_read();
        } else {
            self.biu_prepare_memory_read();
            self.clk(bus, 10);
        }
    }

    fn bit1_prefetch_target(&self) -> usize {
        let entry_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
        let instruction_len = self.ip.wrapping_sub(self.prev_ip) as usize;
        if entry_full && instruction_len <= QUEUE_SIZE {
            2
        } else {
            1
        }
    }

    fn bit1_prepare_read(&mut self, bus: &mut impl common::Bus, target_queue_len: usize) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }

        let mut fetched = false;
        while self.queue_len() < target_queue_len && self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            fetched = true;
        }

        if !fetched {
            self.clk(bus, 2);
        }
        self.biu_ready_memory_read();
    }

    fn bit1_prefetch_before_write(&mut self, bus: &mut impl common::Bus, target_queue_len: usize) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }

        while self.queue_len() < target_queue_len && self.queue_has_room_for_fetch() {
            self.bitfield_code_fetch_after_gap(bus, 0);
        }
        self.bitfield_complete_current_phase();
    }

    fn bit1_register_tail(&mut self, bus: &mut impl common::Bus, op: Bit1Op, source: Bit1Source) {
        let cycles = match (op, source) {
            (Bit1Op::Test, Bit1Source::Cl) => {
                if self.seg_prefix
                    && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
                {
                    2
                } else {
                    3
                }
            }
            (Bit1Op::Clear, Bit1Source::Cl) => {
                if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
                    && !self.seg_prefix
                {
                    4
                } else {
                    3
                }
            }
            (Bit1Op::Set, Bit1Source::Cl) => {
                if self.seg_prefix
                    && self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
                {
                    2
                } else {
                    3
                }
            }
            (Bit1Op::Not, Bit1Source::Cl) | (_, Bit1Source::Immediate) => 3,
        };
        self.clk(bus, cycles);
    }

    fn bit1_finish_test_memory(
        &mut self,
        bus: &mut impl common::Bus,
        source: Bit1Source,
        target_queue_len: usize,
    ) {
        let cycles = match source {
            Bit1Source::Cl => 3,
            Bit1Source::Immediate => {
                if target_queue_len >= 2 {
                    2
                } else {
                    4
                }
            }
        };
        self.clk(bus, cycles);
    }

    fn bit1_write_prefetch_target(
        target_queue_len: usize,
        op: Bit1Op,
        source: Bit1Source,
    ) -> usize {
        let extra = match (op, source) {
            (Bit1Op::Clear, Bit1Source::Cl) => 2,
            (Bit1Op::Set | Bit1Op::Not, Bit1Source::Immediate) => 0,
            _ => 1,
        };
        (target_queue_len + extra).min(QUEUE_SIZE)
    }

    fn bit1_write_gap(op: Bit1Op, source: Bit1Source) -> i32 {
        match (op, source) {
            (Bit1Op::Clear, _) => 0,
            (_, Bit1Source::Cl) | (Bit1Op::Set | Bit1Op::Not, Bit1Source::Immediate) => 2,
            _ => 0,
        }
    }

    fn bit1_fetch_byte_bit(&mut self, bus: &mut impl common::Bus, source: Bit1Source) -> u8 {
        match source {
            Bit1Source::Cl => self.regs.byte(ByteReg::CL) & 0x07,
            Bit1Source::Immediate => self.fetch(bus) & 0x07,
        }
    }

    fn bit1_fetch_word_bit(&mut self, bus: &mut impl common::Bus, source: Bit1Source) -> u8 {
        match source {
            Bit1Source::Cl => self.regs.byte(ByteReg::CL) & 0x0F,
            Bit1Source::Immediate => self.fetch(bus) & 0x0F,
        }
    }

    fn bit1_apply_byte(&mut self, value: u8, bit: u8, op: Bit1Op) -> u8 {
        match op {
            Bit1Op::Test => {
                self.flags.set_szpf_byte((value & (1 << bit)) as u32);
                self.flags.carry_val = 0;
                self.flags.overflow_val = 0;
                self.flags.aux_val = 0;
                value
            }
            Bit1Op::Clear => value & !(1 << bit),
            Bit1Op::Set => value | (1 << bit),
            Bit1Op::Not => value ^ (1 << bit),
        }
    }

    fn bit1_apply_word(&mut self, value: u16, bit: u8, op: Bit1Op) -> u16 {
        match op {
            Bit1Op::Test => {
                self.flags.set_szpf_word((value & (1 << bit)) as u32);
                self.flags.carry_val = 0;
                self.flags.overflow_val = 0;
                self.flags.aux_val = 0;
                value
            }
            Bit1Op::Clear => value & !(1 << bit),
            Bit1Op::Set => value | (1 << bit),
            Bit1Op::Not => value ^ (1 << bit),
        }
    }

    fn bit1_byte(&mut self, bus: &mut impl common::Bus, op: Bit1Op, source: Bit1Source) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            let reg = self.rm_byte(modrm);
            let value = self.regs.byte(reg);
            let bit = self.bit1_fetch_byte_bit(bus, source);
            let result = self.bit1_apply_byte(value, bit, op);
            if !matches!(op, Bit1Op::Test) {
                self.regs.set_byte(reg, result);
            }
            self.bit1_register_tail(bus, op, source);
            return;
        }

        self.calc_ea(modrm, bus);
        let target_queue_len = self.bit1_prefetch_target();
        self.bit1_prepare_read(bus, target_queue_len);
        let value = self.biu_read_u8_physical(bus, self.ea);
        let bit = self.bit1_fetch_byte_bit(bus, source);
        let result = self.bit1_apply_byte(value, bit, op);

        if matches!(op, Bit1Op::Test) {
            self.bit1_finish_test_memory(bus, source, target_queue_len);
            return;
        }

        self.bit1_prefetch_before_write(
            bus,
            Self::bit1_write_prefetch_target(target_queue_len, op, source),
        );
        self.clk(bus, Self::bit1_write_gap(op, source));
        self.biu_ready_memory_write();
        self.biu_write_u8_physical(bus, self.ea, result);
        self.biu_bus_wait_finish(bus);
    }

    fn bit1_word(&mut self, bus: &mut impl common::Bus, op: Bit1Op, source: Bit1Source) {
        let modrm = self.fetch(bus);
        if modrm >= 0xC0 {
            let reg = self.rm_word(modrm);
            let value = self.regs.word(reg);
            let bit = self.bit1_fetch_word_bit(bus, source);
            let result = self.bit1_apply_word(value, bit, op);
            if !matches!(op, Bit1Op::Test) {
                self.regs.set_word(reg, result);
            }
            self.bit1_register_tail(bus, op, source);
            return;
        }

        self.calc_ea(modrm, bus);
        let target_queue_len = self.bit1_prefetch_target();
        self.bit1_prepare_read(bus, target_queue_len);
        let value = self.seg_read_word(bus);
        let bit = self.bit1_fetch_word_bit(bus, source);
        let result = self.bit1_apply_word(value, bit, op);

        if matches!(op, Bit1Op::Test) {
            self.bit1_finish_test_memory(bus, source, target_queue_len);
            return;
        }

        self.bit1_prefetch_before_write(
            bus,
            Self::bit1_write_prefetch_target(target_queue_len, op, source),
        );
        self.clk(bus, Self::bit1_write_gap(op, source));
        self.biu_ready_memory_write();
        self.seg_write_word(bus, result);
        self.biu_bus_wait_finish(bus);
    }

    fn bitfield_prefetch_before_memory(
        &mut self,
        bus: &mut impl common::Bus,
        target_queue_len: usize,
    ) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }
        while self.queue_len() < target_queue_len && self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
        }
        self.biu_ready_memory_read();
    }

    fn bitfield_prefetch_between_memory_reads(&mut self, bus: &mut impl common::Bus) {
        if self.queue_has_room_for_fetch() {
            self.biu_complete_current_bus_for_eu();
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            self.clk(bus, 2);
            self.biu_ready_memory_read();
        }
    }

    fn bitfield_complete_current_phase(&mut self) {
        self.biu_complete_code_fetch_for_eu();
        self.biu_complete_current_bus_for_eu();
    }

    fn bitfield_code_fetch_after_gap(&mut self, bus: &mut impl common::Bus, gap: i32) {
        self.bitfield_complete_current_phase();
        self.clk(bus, gap);
        if self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
        }
    }

    fn bitfield_read_word_after_gap(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u16,
        gap: i32,
    ) -> u16 {
        self.bitfield_complete_current_phase();
        self.clk(bus, gap);
        self.biu_ready_memory_read();
        self.read_word_seg(bus, base, offset)
    }

    fn bitfield_write_word_after_gap(
        &mut self,
        bus: &mut impl common::Bus,
        base: u32,
        offset: u16,
        value: u16,
        gap: i32,
    ) {
        self.bitfield_complete_current_phase();
        self.clk(bus, gap);
        self.biu_ready_memory_write();
        self.write_word_seg(bus, base, offset, value);
        self.biu_bus_wait_finish(bus);
    }

    fn bitfield_register_count_prefetch_target(&self) -> usize {
        if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            BITFIELD_FULL_PREFETCH_QUEUE_TARGET
        } else {
            BITFIELD_MIN_PREFETCH_QUEUE_LEN
        }
    }

    fn bitfield_crosses_word(bit_offset: u8, bit_count: u8) -> bool {
        u16::from(bit_offset) + u16::from(bit_count) > 16
    }

    fn bitfield_ins_prefetch_before_memory(
        &mut self,
        bus: &mut impl common::Bus,
        immediate_count: bool,
        zero_offset: bool,
    ) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }

        if zero_offset {
            while self.queue_len() < QUEUE_SIZE && self.queue_has_room_for_fetch() {
                self.biu_start_code_fetch_for_eu();
                self.clk(bus, 4);
                self.biu_complete_code_fetch_for_eu();
            }
            let entry_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
            if immediate_count {
                self.clk(
                    bus,
                    if entry_full && self.seg_prefix {
                        3
                    } else if entry_full {
                        5
                    } else {
                        2
                    },
                );
            } else if entry_full {
                self.clk(bus, if self.seg_prefix { 7 } else { 9 });
            } else {
                self.clk(bus, 5);
            }
        } else if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            let target_queue_len = if self.seg_prefix && immediate_count {
                0
            } else if self.seg_prefix || immediate_count {
                BITFIELD_MIN_PREFETCH_QUEUE_LEN
            } else {
                BITFIELD_FULL_PREFETCH_QUEUE_TARGET
            };
            while self.queue_len() < target_queue_len && self.queue_has_room_for_fetch() {
                self.biu_start_code_fetch_for_eu();
                self.clk(bus, 4);
                self.biu_complete_code_fetch_for_eu();
            }
            if self.seg_prefix {
                self.clk(bus, 2);
            }
        } else if !immediate_count && self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
        }

        self.biu_ready_memory_read();
    }

    fn bitfield_ins_prefetch_before_write(&mut self, bus: &mut impl common::Bus) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }

        while self.queue_len() < QUEUE_SIZE && self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
        }
    }

    fn bitfield_insert_duplicate_gap(bit_offset: u8, bit_count: u8) -> i32 {
        Self::bitfield_insert_zero_gap(u16::from(bit_count)) + i32::from(bit_offset) * 2
    }

    fn bitfield_insert_zero_gap(bit_count: u16) -> i32 {
        if bit_count <= 4 {
            i32::from(bit_count * 2 + 8)
        } else {
            i32::from(bit_count * 3 + 4)
        }
    }

    fn bitfield_insert_prefixed_or_cold(&self) -> bool {
        self.seg_prefix || self.biu_instruction_entry_queue_len_for_timing() < QUEUE_SIZE
    }

    fn bitfield_insert_register_code_count(&self) -> usize {
        if self.bitfield_insert_prefixed_or_cold() {
            2
        } else {
            1
        }
    }

    fn bitfield_insert_register_first_gap(&self, bit_offset: u8) -> i32 {
        if self.bitfield_insert_prefixed_or_cold() {
            i32::from(bit_offset) + 9
        } else {
            i32::from(bit_offset) + 13
        }
    }

    fn bitfield_insert_register_write_gap(&self) -> i32 {
        if self.bitfield_insert_prefixed_or_cold() {
            29
        } else {
            33
        }
    }

    fn bitfield_insert_immediate_middle_gap(&self, bit_offset: u8) -> i32 {
        if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE && !self.seg_prefix {
            (i32::from(bit_offset) + 1).max(3)
        } else if bit_offset <= 3 {
            0
        } else if bit_offset <= 6 {
            3
        } else {
            i32::from(bit_offset) - 3
        }
    }

    fn bitfield_insert_immediate_final_prefetch_gap(&self, bit_offset: u8) -> i32 {
        if bit_offset <= 3 {
            i32::from(bit_offset) + 5
        } else if bit_offset <= 6 {
            i32::from(bit_offset) + 2
        } else {
            8
        }
    }

    fn bitfield_insert_exact_word_write_gap(&self, immediate_count: bool) -> i32 {
        if immediate_count {
            if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
                if self.seg_prefix { 23 } else { 25 }
            } else {
                21
            }
        } else if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE {
            if self.seg_prefix { 27 } else { 29 }
        } else {
            25
        }
    }

    fn bitfield_insert_immediate_zero_write_gap(&self, bit_count: u16) -> i32 {
        let gap = Self::bitfield_insert_zero_gap(bit_count);
        if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE || bit_count <= 4 {
            gap
        } else {
            gap - 1
        }
    }

    fn bitfield_insert_immediate_duplicate_final_gap(&self, bit_offset: u8) -> i32 {
        if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE && !self.seg_prefix {
            (i32::from(bit_offset) + 6).min(8)
        } else {
            self.bitfield_insert_immediate_final_prefetch_gap(bit_offset)
        }
    }

    fn bitfield_insert_immediate_write_gap(&self, bit_offset: u8, bit_count: u8) -> i32 {
        let total_bits = u16::from(bit_offset) + u16::from(bit_count);
        let entry_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
        let short_overflow_credit = match bit_offset {
            1 if total_bits <= 17 && entry_full && !self.seg_prefix => 1,
            1 if total_bits <= 17 => 2,
            2 if !entry_full || self.seg_prefix => 1,
            4 if !entry_full || self.seg_prefix => 2,
            5 if !entry_full || self.seg_prefix => 1,
            _ => 0,
        };
        28 - i32::from(bit_offset) - short_overflow_credit
    }

    fn bitfield_insert_post_read_prefetch(
        &mut self,
        bus: &mut impl common::Bus,
        immediate_count: bool,
        bit_offset: u8,
        final_gap: i32,
    ) {
        if immediate_count {
            let full_unprefixed =
                self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE && !self.seg_prefix;
            self.bitfield_code_fetch_after_gap(bus, 0);
            if full_unprefixed {
                self.bitfield_code_fetch_after_gap(
                    bus,
                    self.bitfield_insert_immediate_middle_gap(bit_offset),
                );
            } else {
                self.bitfield_code_fetch_after_gap(bus, 0);
                self.bitfield_code_fetch_after_gap(
                    bus,
                    self.bitfield_insert_immediate_middle_gap(bit_offset),
                );
            }
            self.bitfield_complete_current_phase();
            self.clk(bus, final_gap);
        } else {
            let code_count = self.bitfield_insert_register_code_count();
            for _ in 0..code_count {
                self.bitfield_code_fetch_after_gap(bus, 0);
            }
            self.bitfield_complete_current_phase();
            self.clk(bus, final_gap);
        }
    }

    fn bitfield_ext_tail(
        &self,
        immediate_count: bool,
        bit_offset: u8,
        bit_count: u8,
        crosses_word: bool,
    ) -> i32 {
        let total_bits = bit_offset + bit_count;
        let entry_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
        let base = if crosses_word {
            if immediate_count || !entry_full {
                7
            } else {
                11
            }
        } else if immediate_count || !entry_full {
            21
        } else {
            25
        };
        let terminal_word_credit = i32::from(!crosses_word && total_bits >= 16);
        let visible_tail = base + i32::from(bit_offset) + terminal_word_credit;
        let code_fetch_credit = if crosses_word {
            if immediate_count || !entry_full { 4 } else { 0 }
        } else if immediate_count || !entry_full {
            8
        } else {
            4
        };
        visible_tail + code_fetch_credit
    }

    fn bitfield_tail(&mut self, bus: &mut impl common::Bus, cycles: i32) {
        self.clk(bus, cycles);
    }

    fn nibble_rotate_prefetch_target(&self) -> usize {
        let entry_full = self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE;
        let instruction_len = self.ip.wrapping_sub(self.prev_ip) as usize;
        if entry_full && instruction_len <= QUEUE_SIZE {
            2
        } else {
            1
        }
    }

    fn nibble_rotate_prepare_read(&mut self, bus: &mut impl common::Bus, target_queue_len: usize) {
        if self.biu_latch_is_code_fetch() {
            self.biu_bus_wait_finish(bus);
            self.biu_complete_code_fetch_for_eu();
        }

        let mut fetched = false;
        while self.queue_len() < target_queue_len && self.queue_has_room_for_fetch() {
            self.biu_start_code_fetch_for_eu();
            self.clk(bus, 4);
            self.biu_complete_code_fetch_for_eu();
            fetched = true;
        }

        if !fetched {
            self.clk(bus, 2);
        }
        self.biu_ready_memory_read();
    }

    fn nibble_rotate_prefetch_before_write(&mut self, bus: &mut impl common::Bus) {
        while self.queue_len() < QUEUE_SIZE && self.queue_has_room_for_fetch() {
            self.bitfield_code_fetch_after_gap(bus, 0);
        }
        self.bitfield_complete_current_phase();
    }

    fn nibble_rotate_write_gap(target_queue_len: usize, ror4: bool) -> i32 {
        let queue_gap = if target_queue_len >= 2 { 4 } else { 0 };
        let rotate_gap = if ror4 { 4 } else { 0 };
        queue_gap + rotate_gap
    }

    fn rol4(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let al = self.regs.byte(ByteReg::AL);
        if modrm >= 0xC0 {
            let reg = self.rm_byte(modrm);
            let tmp = self.regs.byte(reg);
            let new_al = ((al & 0x0F) << 4) | (tmp >> 4);
            let new_op = ((tmp & 0x0F) << 4) | (al & 0x0F);
            self.regs.set_byte(reg, new_op);
            self.regs.set_byte(ByteReg::AL, new_al);
            let cycles = if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
                && !self.seg_prefix
            {
                13
            } else {
                12
            };
            self.clk(bus, cycles);
            return;
        }

        self.calc_ea(modrm, bus);
        let target_queue_len = self.nibble_rotate_prefetch_target();
        self.nibble_rotate_prepare_read(bus, target_queue_len);
        let tmp = self.biu_read_u8_physical(bus, self.ea);
        let new_al = ((al & 0x0F) << 4) | (tmp >> 4);
        let new_op = ((tmp & 0x0F) << 4) | (al & 0x0F);
        self.regs.set_byte(ByteReg::AL, new_al);
        self.nibble_rotate_prefetch_before_write(bus);
        self.clk(bus, Self::nibble_rotate_write_gap(target_queue_len, false));
        self.biu_ready_memory_write();
        self.biu_write_u8_physical(bus, self.ea, new_op);
        self.biu_bus_wait_finish(bus);
    }

    fn ror4(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let al = self.regs.byte(ByteReg::AL);
        if modrm >= 0xC0 {
            let reg = self.rm_byte(modrm);
            let tmp = self.regs.byte(reg);
            let new_op = ((al & 0x0F) << 4) | (tmp >> 4);
            self.regs.set_byte(reg, new_op);
            self.regs.set_byte(ByteReg::AL, tmp);
            let cycles = if self.biu_instruction_entry_queue_len_for_timing() == QUEUE_SIZE
                && !self.seg_prefix
            {
                17
            } else {
                16
            };
            self.clk(bus, cycles);
            return;
        }

        self.calc_ea(modrm, bus);
        let target_queue_len = self.nibble_rotate_prefetch_target();
        self.nibble_rotate_prepare_read(bus, target_queue_len);
        let tmp = self.biu_read_u8_physical(bus, self.ea);
        let new_op = ((al & 0x0F) << 4) | (tmp >> 4);
        self.regs.set_byte(ByteReg::AL, tmp);
        self.nibble_rotate_prefetch_before_write(bus);
        self.clk(bus, Self::nibble_rotate_write_gap(target_queue_len, true));
        self.biu_ready_memory_write();
        self.biu_write_u8_physical(bus, self.ea, new_op);
        self.biu_bus_wait_finish(bus);
    }

    fn add4s(&mut self, bus: &mut impl common::Bus) {
        let count = self.regs.byte(ByteReg::CL).div_ceil(2);
        let timing = self.bcd4s_startup(bus);
        let mut carry = 0u16;
        let mut zero = 0u32;
        let si_base = self.regs.word(WordReg::SI);
        let di_base = self.regs.word(WordReg::DI);
        let src_seg = self.default_base(SegReg16::DS);
        let dst_seg = self.seg_base(SegReg16::ES);
        for i in 0..count {
            let first_iteration = i == 0;
            let final_iteration = i + 1 == count;
            let si = si_base.wrapping_add(i as u16);
            let di = di_base.wrapping_add(i as u16);
            let src_addr = src_seg.wrapping_add(si as u32) & 0xFFFFF;
            let dst_addr = dst_seg.wrapping_add(di as u32) & 0xFFFFF;
            let src = self.biu_read_u8_physical(bus, src_addr) as u16;
            self.bcd4s_after_source_read(bus);
            let dst = self.biu_read_u8_physical(bus, dst_addr) as u16;
            let total = src + dst + carry;
            let old_al = (total & 0xFF) as u8;
            let old_cf = total > 0xFF;
            let old_af = (dst & 0xF) + (src & 0xF) + carry > 0xF;
            let mut al = old_al;
            if (old_al & 0x0F) > 9 || old_af {
                al = al.wrapping_add(6);
            }
            let threshold = if old_af { 0x9F } else { 0x99 };
            if old_al > threshold || old_cf {
                al = al.wrapping_add(0x60);
                carry = 1;
            } else {
                carry = 0;
            }
            let final_carry = carry != 0;
            self.bcd4s_before_write(bus);
            self.biu_write_u8_physical(bus, dst_addr, al);
            if al != 0 {
                zero = 1;
            }
            self.bcd4s_after_write(bus, final_iteration, first_iteration, timing, final_carry);
        }
        self.flags.carry_val = carry as u32;
        self.flags.zero_val = zero;
    }

    fn sub4s(&mut self, bus: &mut impl common::Bus) {
        let count = self.regs.byte(ByteReg::CL).div_ceil(2);
        let timing = self.bcd4s_startup(bus);
        let mut carry = 0i16;
        let mut zero = 0u32;
        let si_base = self.regs.word(WordReg::SI);
        let di_base = self.regs.word(WordReg::DI);
        let src_seg = self.default_base(SegReg16::DS);
        let dst_seg = self.seg_base(SegReg16::ES);
        for i in 0..count {
            let first_iteration = i == 0;
            let final_iteration = i + 1 == count;
            let si = si_base.wrapping_add(i as u16);
            let di = di_base.wrapping_add(i as u16);
            let src_addr = src_seg.wrapping_add(si as u32) & 0xFFFFF;
            let dst_addr = dst_seg.wrapping_add(di as u32) & 0xFFFFF;
            let src = self.biu_read_u8_physical(bus, src_addr) as i16;
            self.bcd4s_after_source_read(bus);
            let dst = self.biu_read_u8_physical(bus, dst_addr) as i16;
            let total = dst - src - carry;
            let old_al = (total & 0xFF) as u8;
            let old_cf = total < 0;
            let old_af = (dst & 0xF) - (src & 0xF) - carry < 0;
            let mut al = old_al;
            if (old_al & 0x0F) > 9 || old_af {
                al = al.wrapping_sub(6);
            }
            let threshold = if old_af { 0x9F } else { 0x99 };
            if old_al > threshold || old_cf {
                al = al.wrapping_sub(0x60);
                carry = 1;
            } else {
                carry = 0;
            }
            let final_carry = carry != 0;
            self.bcd4s_before_write(bus);
            self.biu_write_u8_physical(bus, dst_addr, al);
            if al != 0 {
                zero = 1;
            }
            self.bcd4s_after_write(bus, final_iteration, first_iteration, timing, final_carry);
        }
        self.flags.carry_val = carry as u32;
        self.flags.zero_val = zero;
    }

    fn cmp4s(&mut self, bus: &mut impl common::Bus) {
        let count = self.regs.byte(ByteReg::CL).div_ceil(2);
        let _timing = self.bcd4s_startup(bus);
        let mut carry = 0i16;
        let mut zero = 0u32;
        let si_base = self.regs.word(WordReg::SI);
        let di_base = self.regs.word(WordReg::DI);
        let src_seg = self.default_base(SegReg16::DS);
        let dst_seg = self.seg_base(SegReg16::ES);
        for i in 0..count {
            let final_iteration = i + 1 == count;
            let si = si_base.wrapping_add(i as u16);
            let di = di_base.wrapping_add(i as u16);
            let src_addr = src_seg.wrapping_add(si as u32) & 0xFFFFF;
            let dst_addr = dst_seg.wrapping_add(di as u32) & 0xFFFFF;
            let src = self.biu_read_u8_physical(bus, src_addr) as i16;
            self.bcd4s_after_source_read(bus);
            let dst = self.biu_read_u8_physical(bus, dst_addr) as i16;
            let total = dst - src - carry;
            let old_al = (total & 0xFF) as u8;
            let old_cf = total < 0;
            let old_af = (dst & 0xF) - (src & 0xF) - carry < 0;
            let mut al = old_al;
            if (old_al & 0x0F) > 9 || old_af {
                al = al.wrapping_sub(6);
            }
            let threshold = if old_af { 0x9F } else { 0x99 };
            if old_al > threshold || old_cf {
                al = al.wrapping_sub(0x60);
                carry = 1;
            } else {
                carry = 0;
            }
            let final_carry = carry != 0;
            if al != 0 {
                zero = 1;
            }
            self.bcd4s_after_compare(bus, final_iteration, final_carry);
        }
        self.flags.carry_val = carry as u32;
        self.flags.zero_val = zero;
    }

    fn ins_reg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(0xC0 | (modrm & 7), bus) & 0x0F;
        let bit_count = (self.get_rm_byte(0xC0 | ((modrm >> 3) & 7), bus) & 0x0F) + 1;
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        let iy = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let crosses_word = Self::bitfield_crosses_word(bit_offset, bit_count);
        let exact_word = total_bits == 16;
        let zero_offset = bit_offset == 0;
        let mask = ((1u32 << bit_count) - 1) as u16;
        if zero_offset && exact_word {
            self.bitfield_ins_prefetch_before_write(bus);
            let write_gap = self.bitfield_insert_exact_word_write_gap(false);
            self.bitfield_write_word_after_gap(
                bus,
                base,
                iy,
                self.regs.word(WordReg::AX),
                write_gap,
            );
            self.regs.set_word(WordReg::DI, iy.wrapping_add(2));
            return;
        }
        self.bitfield_ins_prefetch_before_memory(bus, false, zero_offset);
        let mut word = self.read_word_seg(bus, base, iy);
        word =
            (word & !(mask << bit_offset)) | ((self.regs.word(WordReg::AX) & mask) << bit_offset);
        if crosses_word {
            self.bitfield_insert_post_read_prefetch(
                bus,
                false,
                bit_offset,
                self.bitfield_insert_register_write_gap(),
            );
            self.bitfield_write_word_after_gap(bus, base, iy, word, 0);
            let iy2 = iy.wrapping_add(2);
            let overflow_bits = total_bits - 16;
            let overflow_mask = ((1u32 << overflow_bits) - 1) as u16;
            let mut word2 = self.bitfield_read_word_after_gap(bus, base, iy2, 0);
            word2 = (word2 & !overflow_mask)
                | ((self.regs.word(WordReg::AX) >> (16 - bit_offset)) & overflow_mask);
            let high_gap = Self::bitfield_insert_zero_gap(overflow_bits as u16);
            self.bitfield_write_word_after_gap(bus, base, iy2, word2, high_gap);
        } else if exact_word {
            self.bitfield_insert_post_read_prefetch(
                bus,
                false,
                bit_offset,
                self.bitfield_insert_register_write_gap(),
            );
            self.bitfield_write_word_after_gap(bus, base, iy, word, 0);
        } else if zero_offset {
            let write_gap = Self::bitfield_insert_zero_gap(total_bits as u16);
            self.bitfield_write_word_after_gap(bus, base, iy, word, write_gap);
        } else {
            self.bitfield_insert_post_read_prefetch(
                bus,
                false,
                bit_offset,
                self.bitfield_insert_register_first_gap(bit_offset),
            );
            word = self.bitfield_read_word_after_gap(bus, base, iy, 0);
            word = (word & !(mask << bit_offset))
                | ((self.regs.word(WordReg::AX) & mask) << bit_offset);
            let write_gap = Self::bitfield_insert_duplicate_gap(bit_offset, bit_count);
            self.bitfield_write_word_after_gap(bus, base, iy, word, write_gap);
        }
        if total_bits >= 16 {
            self.regs.set_word(WordReg::DI, iy.wrapping_add(2));
        }
    }

    fn ext_reg(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(0xC0 | (modrm & 7), bus) & 0x0F;
        let bit_count = (self.get_rm_byte(0xC0 | ((modrm >> 3) & 7), bus) & 0x0F) + 1;
        let ix = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let target_queue_len = self.bitfield_register_count_prefetch_target();
        self.bitfield_prefetch_before_memory(bus, target_queue_len);
        let mut result = self.read_word_seg(bus, base, ix) >> bit_offset;
        let crosses_word = Self::bitfield_crosses_word(bit_offset, bit_count);
        if crosses_word {
            self.bitfield_prefetch_between_memory_reads(bus);
            result |= self.read_word_seg(bus, base, ix.wrapping_add(2)) << (16 - bit_offset);
        }
        let mask = ((1u32 << bit_count) - 1) as u16;
        self.regs.set_word(WordReg::AX, result & mask);
        if total_bits >= 16 {
            self.regs.set_word(WordReg::SI, ix.wrapping_add(2));
        }
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        let tail = self.bitfield_ext_tail(false, bit_offset, bit_count, crosses_word);
        self.bitfield_tail(bus, tail);
    }

    fn ins_imm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(modrm, bus) & 0x0F;
        let bit_count = (self.fetch(bus) & 0x0F) + 1;
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        let iy = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let total_bits = bit_offset as u32 + bit_count as u32;
        let crosses_word = Self::bitfield_crosses_word(bit_offset, bit_count);
        let exact_word = total_bits == 16;
        let zero_offset = bit_offset == 0;
        let mask = ((1u32 << bit_count) - 1) as u16;
        if zero_offset && exact_word {
            self.bitfield_ins_prefetch_before_write(bus);
            let write_gap = self.bitfield_insert_exact_word_write_gap(true);
            self.bitfield_write_word_after_gap(
                bus,
                base,
                iy,
                self.regs.word(WordReg::AX),
                write_gap,
            );
            self.regs.set_word(WordReg::DI, iy.wrapping_add(2));
            return;
        }
        self.bitfield_ins_prefetch_before_memory(bus, true, zero_offset);
        let mut word = self.read_word_seg(bus, base, iy);
        word =
            (word & !(mask << bit_offset)) | ((self.regs.word(WordReg::AX) & mask) << bit_offset);
        if crosses_word {
            let write_gap = self.bitfield_insert_immediate_write_gap(bit_offset, bit_count);
            self.bitfield_insert_post_read_prefetch(bus, true, bit_offset, write_gap);
            self.bitfield_write_word_after_gap(bus, base, iy, word, 0);
            let iy2 = iy.wrapping_add(2);
            let overflow_bits = total_bits - 16;
            let overflow_mask = ((1u32 << overflow_bits) - 1) as u16;
            let mut word2 = self.bitfield_read_word_after_gap(bus, base, iy2, 0);
            word2 = (word2 & !overflow_mask)
                | ((self.regs.word(WordReg::AX) >> (16 - bit_offset)) & overflow_mask);
            let high_gap = Self::bitfield_insert_zero_gap(overflow_bits as u16);
            self.bitfield_write_word_after_gap(bus, base, iy2, word2, high_gap);
        } else if exact_word {
            let write_gap = self.bitfield_insert_immediate_write_gap(bit_offset, bit_count);
            self.bitfield_insert_post_read_prefetch(bus, true, bit_offset, write_gap);
            self.bitfield_write_word_after_gap(bus, base, iy, word, 0);
        } else if zero_offset {
            let write_gap = self.bitfield_insert_immediate_zero_write_gap(total_bits as u16);
            self.bitfield_write_word_after_gap(bus, base, iy, word, write_gap);
        } else {
            self.bitfield_insert_post_read_prefetch(
                bus,
                true,
                bit_offset,
                self.bitfield_insert_immediate_duplicate_final_gap(bit_offset),
            );
            word = self.bitfield_read_word_after_gap(bus, base, iy, 0);
            word = (word & !(mask << bit_offset))
                | ((self.regs.word(WordReg::AX) & mask) << bit_offset);
            let write_gap = Self::bitfield_insert_duplicate_gap(bit_offset, bit_count);
            self.bitfield_write_word_after_gap(bus, base, iy, word, write_gap);
        }
        if total_bits >= 16 {
            self.regs.set_word(WordReg::DI, iy.wrapping_add(2));
        }
    }

    fn ext_imm(&mut self, bus: &mut impl common::Bus) {
        let modrm = self.fetch(bus);
        let bit_offset = self.get_rm_byte(modrm, bus) & 0x0F;
        let bit_count = (self.fetch(bus) & 0x0F) + 1;
        let ix = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let total_bits = bit_offset as u32 + bit_count as u32;
        self.bitfield_prefetch_before_memory(bus, BITFIELD_MIN_PREFETCH_QUEUE_LEN);
        let mut result = self.read_word_seg(bus, base, ix) >> bit_offset;
        let crosses_word = Self::bitfield_crosses_word(bit_offset, bit_count);
        if crosses_word {
            self.bitfield_prefetch_between_memory_reads(bus);
            result |= self.read_word_seg(bus, base, ix.wrapping_add(2)) << (16 - bit_offset);
        }
        let mask = ((1u32 << bit_count) - 1) as u16;
        self.regs.set_word(WordReg::AX, result & mask);
        if total_bits >= 16 {
            self.regs.set_word(WordReg::SI, ix.wrapping_add(2));
        }
        let new_offset = (bit_offset + bit_count) & 0x0F;
        self.putback_rm_byte(modrm, new_offset, bus);
        let tail = self.bitfield_ext_tail(true, bit_offset, bit_count, crosses_word);
        self.bitfield_tail(bus, tail);
    }
}
