//! Implements the Intel 80286 emulation.
//!
//! Following references were used to write the emulator:
//!
//! - Intel Corporation, "80286 Programmer's Reference Manual".
//! - MAME Intel i286 emulator (`devices/cpu/i86/i286.cpp`).

mod alu;
mod biu;
mod execute;
mod execute_0f;
mod execute_group;
mod flags;
mod interrupt;
mod modrm;
mod rep;
mod state;
mod string_ops;

use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

use biu::{BiuBusRequest, BiuCompletedBusCycle, BusStatus, MAX_QUEUE_SIZE, QueueType};
pub use biu::{
    I286AuStage, I286BusLane, I286BusPhase, I286CycleState, I286CycleTraceEntry, I286EuStage,
    I286FlushState, I286PendingBusRequest, I286RepState, I286TraceBusStatus, I286WarmStartConfig,
};
use common::Cpu as _;
pub use flags::I286Flags;
pub use state::I286State;

use crate::{SegReg16, WordReg};

pub(crate) const ADDRESS_MASK: u32 = 0x00FF_FFFF;

#[derive(Clone, Copy)]
struct SegmentDescriptor {
    base: u32,
    limit: u16,
    rights: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum TaskType {
    Iret,
    Jmp,
    Call,
}

/// Intel 80286 CPU emulator.
pub struct I286 {
    /// Embedded state for save/restore.
    pub state: I286State,

    prev_ip: u16,
    seg_prefix: bool,
    prefix_seg: SegReg16,
    prefix_count_current: u8,

    halted: bool,
    pending_irq: u8,
    no_interrupt: u8,
    inhibit_all: u8,

    rep_ip: u16,
    rep_restart_ip: u16,
    rep_seg_prefix: bool,
    rep_prefix_seg: SegReg16,
    rep_opcode: u8,
    rep_type: u8,
    rep_active: bool,

    cycles_remaining: i64,
    run_start_cycle: u64,
    run_budget: u64,

    ea: u32,
    eo: u16,
    ea_seg: SegReg16,
    lock_prefix_active: bool,
    lock_demand_gap_emitted: bool,

    trap_level: u8,
    shutdown: bool,

    instruction_queue: [u8; MAX_QUEUE_SIZE],
    instruction_queue_len: usize,
    instruction_preload: Option<u8>,
    prefetch_spill_queue: VecDeque<u8>,
    instruction_entry_queue_bytes: u8,
    instruction_entry_decoded_queue_bytes: u8,
    instruction_entry_flush_state: I286FlushState,
    decoded_queue_len: u8,
    prefetch_ip: u16,
    bus_status: BusStatus,
    data_bus: u16,
    flush_state: I286FlushState,
    pending_bus_request: Option<BiuBusRequest>,
    active_bus_request: Option<BiuBusRequest>,
    completed_bus_cycle: Option<BiuCompletedBusCycle>,
    delay_queue_room_fetch_once: bool,
    decode_spill_fetch_gap_enabled: bool,
    decode_spill_fetch_needs_gap: bool,
    wrapped_queue_room_fetch_delayed: bool,
    bus_phase: I286BusPhase,
    bus_lane: I286BusLane,
    bhe_asserted: bool,
    au_stage: I286AuStage,
    eu_stage: I286EuStage,
    capture_enabled: bool,
    cycle_counter: u64,
    trace: Vec<I286CycleTraceEntry>,
}

impl Deref for I286 {
    type Target = I286State;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for I286 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for I286 {
    fn default() -> Self {
        Self::new()
    }
}

impl I286 {
    /// Creates a new I286 CPU in its reset state.
    pub fn new() -> Self {
        let mut cpu = Self {
            state: I286State::default(),
            prev_ip: 0,
            seg_prefix: false,
            prefix_seg: SegReg16::DS,
            prefix_count_current: 0,
            halted: false,
            pending_irq: 0,
            no_interrupt: 0,
            inhibit_all: 0,
            rep_ip: 0,
            rep_restart_ip: 0,
            rep_seg_prefix: false,
            rep_prefix_seg: SegReg16::DS,
            rep_opcode: 0,
            rep_type: 0,
            rep_active: false,
            cycles_remaining: 0,
            run_start_cycle: 0,
            run_budget: 0,
            ea: 0,
            eo: 0,
            ea_seg: SegReg16::DS,
            lock_prefix_active: false,
            lock_demand_gap_emitted: false,
            trap_level: 0,
            shutdown: false,
            instruction_queue: [0; MAX_QUEUE_SIZE],
            instruction_queue_len: 0,
            instruction_preload: None,
            prefetch_spill_queue: VecDeque::new(),
            instruction_entry_queue_bytes: 0,
            instruction_entry_decoded_queue_bytes: 0,
            instruction_entry_flush_state: I286FlushState::ControlTransfer,
            decoded_queue_len: 0,
            prefetch_ip: 0,
            bus_status: BusStatus::Passive,
            data_bus: 0xFFFF,
            flush_state: I286FlushState::ControlTransfer,
            pending_bus_request: None,
            active_bus_request: None,
            completed_bus_cycle: None,
            delay_queue_room_fetch_once: false,
            decode_spill_fetch_gap_enabled: false,
            decode_spill_fetch_needs_gap: false,
            wrapped_queue_room_fetch_delayed: false,
            bus_phase: I286BusPhase::Ti,
            bus_lane: I286BusLane::None,
            bhe_asserted: false,
            au_stage: I286AuStage::Idle,
            eu_stage: I286EuStage::Idle,
            capture_enabled: false,
            cycle_counter: 0,
            trace: Vec::new(),
        };
        cpu.reset();
        cpu
    }

    #[inline(always)]
    fn clk(&mut self, cycles: i32) {
        if cycles > 0 {
            self.biu_internal_cycles(cycles as u32);
        }
    }

    #[inline(always)]
    fn clk_bus(&mut self, bus: &mut impl common::Bus, cycles: i32) {
        if cycles > 0 {
            self.biu_bus_cycles(bus, cycles as u32);
        }
    }

    #[inline(always)]
    fn clk_modrm(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        reg_cycles: i32,
        mem_cycles: i32,
    ) {
        if modrm >= 0xC0 {
            self.clk_bus(bus, reg_cycles);
        } else {
            self.clk(mem_cycles);
        }
    }

    #[inline(always)]
    fn clk_modrm_word(
        &mut self,
        bus: &mut impl common::Bus,
        modrm: u8,
        reg_cycles: i32,
        mem_cycles: i32,
        word_accesses: i32,
    ) {
        if modrm >= 0xC0 {
            self.clk_bus(bus, reg_cycles);
        } else {
            let penalty = if self.ea & 1 == 1 {
                4 * word_accesses
            } else {
                0
            };
            self.clk(mem_cycles + penalty);
        }
    }

    #[inline(always)]
    fn sp_penalty(&self, word_accesses: i32) -> i32 {
        if self.regs.word(WordReg::SP) & 1 == 1 {
            4 * word_accesses
        } else {
            0
        }
    }

    #[inline(always)]
    fn fetch(&mut self, bus: &mut impl common::Bus) -> u8 {
        let value = self.biu_queue_read(bus, QueueType::Subsequent);
        self.ip = self.ip.wrapping_add(1);
        value
    }

    #[inline(always)]
    fn fetch_first(&mut self, bus: &mut impl common::Bus) -> u8 {
        self.biu_latch_instruction_entry();
        let value = self.biu_queue_read(bus, QueueType::First);
        self.ip = self.ip.wrapping_add(1);
        value
    }

    #[inline(always)]
    fn fetchword(&mut self, bus: &mut impl common::Bus) -> u16 {
        let low = self.fetch(bus) as u16;
        let high = self.fetch(bus) as u16;
        low | (high << 8)
    }

    #[inline(always)]
    fn fetchword_without_tick(&mut self, bus: &mut impl common::Bus) -> u16 {
        let low = self.biu_queue_read_subsequent_without_tick(bus) as u16;
        self.ip = self.ip.wrapping_add(1);
        let high = self.biu_queue_read_subsequent_without_tick(bus) as u16;
        self.ip = self.ip.wrapping_add(1);
        low | (high << 8)
    }

    #[inline(always)]
    fn is_prefix_opcode(opcode: u8) -> bool {
        matches!(opcode, 0x26 | 0x2E | 0x36 | 0x3E | 0xF0)
    }

    fn is_direct_accumulator_memory_opcode(opcode: u8) -> bool {
        matches!(opcode, 0xA0..=0xA3)
    }

    fn update_prefixed_decode_spill_fetch_gap(&mut self) {
        let next_byte = self.biu_queue_byte_for_timing(0);
        let next_is_lea_mode1 = next_byte == Some(0x8D)
            && self
                .biu_queue_byte_for_timing(1)
                .is_some_and(|modrm| modrm >> 6 == 1);
        let next_is_direct_accumulator_memory =
            next_byte.is_some_and(Self::is_direct_accumulator_memory_opcode);
        let next_is_prefix = next_byte.is_some_and(Self::is_prefix_opcode);
        let next_prefix_is_before_direct_accumulator_memory = next_is_prefix
            && self
                .biu_queue_byte_for_timing(1)
                .is_some_and(Self::is_direct_accumulator_memory_opcode);
        let enabled = (matches!(self.prefix_count_current, 4..=6)
            && (next_is_lea_mode1
                || (self.prefix_count_current == 5 && next_is_direct_accumulator_memory)
                || (self.prefix_count_current == 5
                    && next_is_prefix
                    && !next_prefix_is_before_direct_accumulator_memory)
                || (self.prefix_count_current >= 6 && !next_is_direct_accumulator_memory)));
        self.biu_set_decode_spill_fetch_gap(enabled);
    }

    fn fetch_after_prefix_delay(&mut self, bus: &mut impl common::Bus) -> u8 {
        if self.prefix_count_current >= 3 && self.biu_queue_byte_for_timing(0) == Some(0xD7) {
            if self.prefix_count_current == 3 {
                self.biu_no_prefetch_cycle(bus);
            }
            let opcode = self.biu_queue_read_subsequent_without_tick(bus);
            self.ip = self.ip.wrapping_add(1);
            return opcode;
        }

        if self.prefix_count_current >= 4
            && self
                .biu_queue_byte_for_timing(0)
                .is_some_and(Self::is_prefix_opcode)
            && self.biu_queue_byte_for_timing(1) == Some(0xD7)
        {
            let opcode = self.biu_queue_read_subsequent_without_tick(bus);
            self.ip = self.ip.wrapping_add(1);
            return opcode;
        }

        let next_byte = self.biu_queue_byte_for_timing(0);
        let next_is_bound = next_byte == Some(0x62);
        let next_is_direct_accumulator_memory =
            next_byte.is_some_and(Self::is_direct_accumulator_memory_opcode);
        let prefix_delay_cycles = if (self.prefix_count_current >= 4 && next_is_bound)
            || (self.prefix_count_current >= 6 && next_is_direct_accumulator_memory)
        {
            0
        } else if self.prefix_count_current == 5 && next_is_direct_accumulator_memory {
            1
        } else {
            2
        };
        self.clk_bus(bus, prefix_delay_cycles);
        if self.prefix_count_current >= 4 {
            let opcode = self.biu_queue_read_subsequent_without_tick(bus);
            self.ip = self.ip.wrapping_add(1);
            let skip_tick_after_opcode = self.prefix_count_current >= 5
                && (Self::is_direct_accumulator_memory_opcode(opcode)
                    || (Self::is_prefix_opcode(opcode)
                        && self
                            .biu_queue_byte_for_timing(0)
                            .is_some_and(Self::is_direct_accumulator_memory_opcode)))
                || (self.prefix_count_current == 4 && opcode == 0x62);
            let skip_d7_prefix_tick =
                Self::is_prefix_opcode(opcode) && self.biu_queue_byte_for_timing(0) == Some(0xD7);
            if !(skip_tick_after_opcode || skip_d7_prefix_tick) {
                self.biu_tick_after_subsequent_queue_read(bus);
            }
            opcode
        } else {
            self.fetch(bus)
        }
    }

    #[inline(always)]
    fn default_seg(&self, seg: SegReg16) -> SegReg16 {
        if self.seg_prefix && matches!(seg, SegReg16::DS | SegReg16::SS) {
            self.prefix_seg
        } else {
            seg
        }
    }

    #[inline(always)]
    fn default_base(&self, seg: SegReg16) -> u32 {
        self.seg_bases[self.default_seg(seg) as usize]
    }

    #[inline(always)]
    fn seg_base(&self, seg: SegReg16) -> u32 {
        self.seg_bases[seg as usize]
    }

    /// Computes the physical address for a byte at `eo + delta`, wrapping
    /// the offset within the 16-bit segment boundary.
    #[inline(always)]
    fn seg_addr(&self, delta: u16) -> u32 {
        self.seg_base(self.ea_seg)
            .wrapping_add(self.eo.wrapping_add(delta) as u32)
            & ADDRESS_MASK
    }

    #[inline(always)]
    fn cpl(&self) -> u16 {
        self.sregs[SegReg16::CS as usize] & 3
    }

    #[inline(always)]
    fn is_protected_mode(&self) -> bool {
        self.msw & 1 != 0
    }

    #[inline(always)]
    fn segment_error_code(selector: u16) -> u16 {
        selector & 0xFFFC
    }

    fn set_real_segment_cache(&mut self, seg: SegReg16, selector: u16) {
        self.seg_bases[seg as usize] = (selector as u32) << 4;
        self.seg_limits[seg as usize] = 0xFFFF;
        self.seg_rights[seg as usize] = if seg == SegReg16::CS { 0x9B } else { 0x93 };
        self.seg_valid[seg as usize] = true;
    }

    fn set_loaded_segment_cache(
        &mut self,
        seg: SegReg16,
        selector: u16,
        descriptor: SegmentDescriptor,
    ) {
        self.sregs[seg as usize] = selector;
        self.seg_bases[seg as usize] = descriptor.base;
        self.seg_limits[seg as usize] = descriptor.limit;
        self.seg_rights[seg as usize] = descriptor.rights;
        self.seg_valid[seg as usize] = true;
    }

    fn set_null_segment(&mut self, seg: SegReg16, selector: u16) {
        self.sregs[seg as usize] = selector;
        self.seg_bases[seg as usize] = 0;
        self.seg_limits[seg as usize] = 0;
        self.seg_rights[seg as usize] = 0;
        self.seg_valid[seg as usize] = false;
    }

    fn decode_descriptor(
        &mut self,
        selector: u16,
        bus: &mut impl common::Bus,
    ) -> Option<SegmentDescriptor> {
        let addr = self.descriptor_addr_checked(selector)?;
        Some(self.read_descriptor_at(bus, addr))
    }

    fn descriptor_dpl(rights: u8) -> u16 {
        ((rights >> 5) & 0x03) as u16
    }

    fn descriptor_is_segment(rights: u8) -> bool {
        rights & 0x10 != 0
    }

    fn descriptor_is_code(rights: u8) -> bool {
        rights & 0x08 != 0
    }

    fn descriptor_is_conforming_code(rights: u8) -> bool {
        Self::descriptor_is_code(rights) && rights & 0x04 != 0
    }

    fn descriptor_is_readable(rights: u8) -> bool {
        !Self::descriptor_is_code(rights) || rights & 0x02 != 0
    }

    fn descriptor_is_writable(rights: u8) -> bool {
        !Self::descriptor_is_code(rights) && rights & 0x02 != 0
    }

    fn descriptor_is_expand_down(rights: u8) -> bool {
        !Self::descriptor_is_code(rights) && rights & 0x04 != 0
    }

    fn descriptor_present(rights: u8) -> bool {
        rights & 0x80 != 0
    }

    fn raise_segment_not_present(
        &mut self,
        seg: SegReg16,
        selector: u16,
        bus: &mut impl common::Bus,
    ) {
        let vector = if seg == SegReg16::SS { 12 } else { 11 };
        self.raise_fault_with_code(vector, Self::segment_error_code(selector), bus);
    }

    fn raise_segment_protection(
        &mut self,
        seg: SegReg16,
        selector: u16,
        bus: &mut impl common::Bus,
    ) {
        let vector = if seg == SegReg16::SS { 12 } else { 13 };
        self.raise_fault_with_code(vector, Self::segment_error_code(selector), bus);
    }

    fn load_protected_segment(
        &mut self,
        seg: SegReg16,
        selector: u16,
        bus: &mut impl common::Bus,
    ) -> bool {
        if matches!(seg, SegReg16::DS | SegReg16::ES) && selector & 0xFFFC == 0 {
            self.set_null_segment(seg, selector);
            return true;
        }
        if selector & 0xFFFC == 0 {
            self.raise_segment_protection(seg, selector, bus);
            return false;
        }

        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_segment_protection(seg, selector, bus);
            return false;
        };
        let rights = descriptor.rights;
        let cpl = self.cpl();
        let rpl = selector & 0x0003;
        let dpl = Self::descriptor_dpl(rights);

        // Bug #14: Check type and privilege BEFORE present.
        match seg {
            SegReg16::CS => {
                if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_code(rights) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if Self::descriptor_is_conforming_code(rights) {
                    if dpl > cpl {
                        self.raise_segment_protection(seg, selector, bus);
                        return false;
                    }
                } else if dpl != cpl || rpl > cpl {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                // Present check for CS -> #NP.
                if !Self::descriptor_present(rights) {
                    self.raise_segment_not_present(seg, selector, bus);
                    return false;
                }
                // Bug #10: Set accessed bit.
                self.set_accessed_bit(selector, bus);
                if Self::descriptor_is_conforming_code(rights) {
                    let adjusted = (selector & !3) | cpl;
                    self.set_loaded_segment_cache(seg, adjusted, descriptor);
                } else {
                    let adjusted = (selector & !3) | dpl;
                    self.set_loaded_segment_cache(seg, adjusted, descriptor);
                }
                return true;
            }
            SegReg16::SS => {
                if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_writable(rights) {
                    self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                    return false;
                }
                if dpl != cpl || rpl != cpl {
                    self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                    return false;
                }
                // Present check for SS -> #SS (vector 12).
                if !Self::descriptor_present(rights) {
                    self.raise_fault_with_code(12, Self::segment_error_code(selector), bus);
                    return false;
                }
            }
            SegReg16::DS | SegReg16::ES => {
                if !Self::descriptor_is_segment(rights) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if !Self::descriptor_is_readable(rights) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if !Self::descriptor_is_conforming_code(rights) && dpl < cpl.max(rpl) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                // Present check for DS/ES -> #NP.
                if !Self::descriptor_present(rights) {
                    self.raise_segment_not_present(seg, selector, bus);
                    return false;
                }
            }
        }

        // Bug #10: Set accessed bit.
        self.set_accessed_bit(selector, bus);
        self.set_loaded_segment_cache(seg, selector, descriptor);
        true
    }

    fn load_cs_for_return(
        &mut self,
        selector: u16,
        new_ip: u16,
        bus: &mut impl common::Bus,
    ) -> bool {
        if selector & 0xFFFC == 0 {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        };
        let rights = descriptor.rights;
        let cpl = self.cpl();
        let rpl = selector & 0x0003;
        let dpl = Self::descriptor_dpl(rights);

        if rpl < cpl {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_code(rights) {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if Self::descriptor_is_conforming_code(rights) {
            if dpl > rpl {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                return false;
            }
        } else if dpl != rpl {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
            return false;
        }
        if new_ip > descriptor.limit {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        self.set_accessed_bit(selector, bus);
        let adjusted = (selector & !3) | rpl;
        self.set_loaded_segment_cache(SegReg16::CS, adjusted, descriptor);
        true
    }

    fn check_segment_access(
        &mut self,
        seg: SegReg16,
        offset: u16,
        size: u16,
        write: bool,
        bus: &mut impl common::Bus,
    ) -> bool {
        if !self.is_protected_mode() {
            return true;
        }

        if !self.seg_valid[seg as usize] {
            let vector = if seg == SegReg16::SS { 12 } else { 13 };
            self.raise_fault_with_code(vector, 0, bus);
            return false;
        }

        let rights = self.seg_rights[seg as usize];
        let end = offset as u32 + size.saturating_sub(1) as u32;
        let limit = self.seg_limits[seg as usize] as u32;
        if Self::descriptor_is_expand_down(rights) {
            if offset as u32 <= limit || end > 0xFFFF {
                self.raise_fault_with_code(if seg == SegReg16::SS { 12 } else { 13 }, 0, bus);
                return false;
            }
        } else if end > limit {
            self.raise_fault_with_code(if seg == SegReg16::SS { 12 } else { 13 }, 0, bus);
            return false;
        }

        if write {
            if !Self::descriptor_is_writable(rights) {
                let vector = if seg == SegReg16::SS { 12 } else { 13 };
                self.raise_fault_with_code(vector, 0, bus);
                return false;
            }
        } else if !Self::descriptor_is_readable(rights) {
            let vector = if seg == SegReg16::SS { 12 } else { 13 };
            self.raise_fault_with_code(vector, 0, bus);
            return false;
        }

        true
    }

    #[inline(always)]
    fn seg_read_byte_at(&mut self, bus: &mut impl common::Bus, delta: u16) -> u8 {
        let offset = self.eo.wrapping_add(delta);
        if !self.check_segment_access(self.ea_seg, offset, 1, false, bus) {
            return 0;
        }
        self.biu_read_u8_physical(bus, self.seg_addr(delta))
    }

    #[inline(always)]
    fn seg_write_byte_at(&mut self, bus: &mut impl common::Bus, delta: u16, value: u8) {
        let offset = self.eo.wrapping_add(delta);
        if !self.check_segment_access(self.ea_seg, offset, 1, true, bus) {
            return;
        }
        self.biu_write_u8_physical(bus, self.seg_addr(delta), value);
    }

    /// Reads a word from memory at `ea + delta`, wrapping the offset
    /// within the segment boundary (offset 0xFFFF wraps to 0x0000).
    #[inline(always)]
    fn seg_read_word_at(&mut self, bus: &mut impl common::Bus, delta: u16) -> u16 {
        let offset = self.eo.wrapping_add(delta);
        if !self.check_segment_access(self.ea_seg, offset, 2, false, bus) {
            return 0;
        }
        let low_address = self.seg_addr(delta);
        let high_address = self.seg_addr(delta.wrapping_add(1));
        self.biu_read_u16_pair(bus, low_address, high_address)
    }

    #[inline(always)]
    fn seg_read_word_at_after_prefetch_gap(
        &mut self,
        bus: &mut impl common::Bus,
        delta: u16,
    ) -> u16 {
        let offset = self.eo.wrapping_add(delta);
        if !self.check_segment_access(self.ea_seg, offset, 2, false, bus) {
            return 0;
        }
        let low_address = self.seg_addr(delta);
        let high_address = self.seg_addr(delta.wrapping_add(1));
        self.biu_read_u16_pair_after_prefetch_gap(bus, low_address, high_address)
    }

    #[inline(always)]
    fn seg_read_word_at_after_prefetch_and_au_gap(
        &mut self,
        bus: &mut impl common::Bus,
        delta: u16,
    ) -> u16 {
        let offset = self.eo.wrapping_add(delta);
        if !self.check_segment_access(self.ea_seg, offset, 2, false, bus) {
            return 0;
        }
        let low_address = self.seg_addr(delta);
        let high_address = self.seg_addr(delta.wrapping_add(1));
        self.biu_read_u16_pair_after_prefetch_and_au_gap(bus, low_address, high_address)
    }

    /// Reads a word from memory at the current EA, wrapping the offset
    /// within the segment boundary (offset 0xFFFF wraps to 0x0000).
    #[inline(always)]
    fn seg_read_word(&mut self, bus: &mut impl common::Bus) -> u16 {
        self.seg_read_word_at(bus, 0)
    }

    /// Writes a word to memory at the current EA, wrapping the offset
    /// within the segment boundary (offset 0xFFFF wraps to 0x0000).
    #[inline(always)]
    fn seg_write_word(&mut self, bus: &mut impl common::Bus, value: u16) {
        if !self.check_segment_access(self.ea_seg, self.eo, 2, true, bus) {
            return;
        }
        self.biu_write_u16_pair(bus, self.ea, self.seg_addr(1), value);
    }

    /// Reads a byte from `seg:offset`, wrapping the offset within 16 bits.
    #[inline(always)]
    fn read_byte_seg(&mut self, bus: &mut impl common::Bus, seg: SegReg16, offset: u16) -> u8 {
        if !self.check_segment_access(seg, offset, 1, false, bus) {
            return 0;
        }
        let base = self.seg_base(seg);
        self.biu_read_u8_physical(bus, base.wrapping_add(offset as u32) & ADDRESS_MASK)
    }

    #[inline(always)]
    fn read_byte_seg_without_prefetch(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg16,
        offset: u16,
    ) -> u8 {
        if !self.check_segment_access(seg, offset, 1, false, bus) {
            return 0;
        }
        let base = self.seg_base(seg);
        self.biu_read_u8_physical_without_prefetch(
            bus,
            base.wrapping_add(offset as u32) & ADDRESS_MASK,
        )
    }

    #[inline(always)]
    fn read_byte_seg_after_prefetch_gap(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg16,
        offset: u16,
    ) -> u8 {
        if !self.check_segment_access(seg, offset, 1, false, bus) {
            return 0;
        }
        let base = self.seg_base(seg);
        self.biu_read_u8_physical_after_prefetch_gap(
            bus,
            base.wrapping_add(offset as u32) & ADDRESS_MASK,
        )
    }

    /// Writes a byte to `seg:offset`, wrapping the offset within 16 bits.
    #[inline(always)]
    fn write_byte_seg(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg16,
        offset: u16,
        value: u8,
    ) {
        if !self.check_segment_access(seg, offset, 1, true, bus) {
            return;
        }
        let base = self.seg_base(seg);
        self.biu_write_u8_physical(bus, base.wrapping_add(offset as u32) & ADDRESS_MASK, value);
    }

    /// Reads a word from `seg:offset`, wrapping the offset within 16 bits.
    #[inline(always)]
    fn read_word_seg(&mut self, bus: &mut impl common::Bus, seg: SegReg16, offset: u16) -> u16 {
        if !self.check_segment_access(seg, offset, 2, false, bus) {
            return 0;
        }
        let base = self.seg_base(seg);
        let low_address = base.wrapping_add(offset as u32) & ADDRESS_MASK;
        let high_address = base.wrapping_add(offset.wrapping_add(1) as u32) & ADDRESS_MASK;
        self.biu_read_u16_pair(bus, low_address, high_address)
    }

    /// Writes a word to `seg:offset`, wrapping the offset within 16 bits.
    #[inline(always)]
    fn write_word_seg(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg16,
        offset: u16,
        value: u16,
    ) {
        if !self.check_segment_access(seg, offset, 2, true, bus) {
            return;
        }
        let base = self.seg_base(seg);
        let low_address = base.wrapping_add(offset as u32) & ADDRESS_MASK;
        let high_address = base.wrapping_add(offset.wrapping_add(1) as u32) & ADDRESS_MASK;
        self.biu_write_u16_pair(bus, low_address, high_address, value);
    }

    fn push(&mut self, bus: &mut impl common::Bus, value: u16) {
        let sp = self.regs.word(WordReg::SP).wrapping_sub(2);
        if !self.check_segment_access(SegReg16::SS, sp, 2, true, bus) {
            return;
        }
        self.regs.set_word(WordReg::SP, sp);
        let base = self.seg_base(SegReg16::SS);
        let low_address = base.wrapping_add(sp as u32) & ADDRESS_MASK;
        let high_address = base.wrapping_add(sp.wrapping_add(1) as u32) & ADDRESS_MASK;
        self.biu_write_u16_pair(bus, low_address, high_address, value);
    }

    fn pop(&mut self, bus: &mut impl common::Bus) -> u16 {
        let sp = self.regs.word(WordReg::SP);
        if !self.check_segment_access(SegReg16::SS, sp, 2, false, bus) {
            return 0;
        }
        let base = self.seg_base(SegReg16::SS);
        let low_address = base.wrapping_add(sp as u32) & ADDRESS_MASK;
        let high_address = base.wrapping_add(sp.wrapping_add(1) as u32) & ADDRESS_MASK;
        let value = self.biu_read_u16_pair(bus, low_address, high_address);
        self.regs.set_word(WordReg::SP, sp.wrapping_add(2));
        value
    }

    fn load_segment(&mut self, seg: SegReg16, selector: u16, bus: &mut impl common::Bus) -> bool {
        if !self.is_protected_mode() {
            self.sregs[seg as usize] = selector;
            self.set_real_segment_cache(seg, selector);
            return true;
        }
        self.load_protected_segment(seg, selector, bus)
    }

    /// Returns the physical address of the descriptor for `selector`, provided
    /// the selector is non-null and falls within the table limit.
    fn descriptor_addr_checked(&self, selector: u16) -> Option<u32> {
        if selector & 0xFFFC == 0 {
            return None;
        }
        let (table_base, table_limit) = if selector & 4 != 0 {
            (self.ldtr_base, self.ldtr_limit)
        } else {
            (self.gdt_base, self.gdt_limit)
        };
        let index = (selector & !7) as u32;
        if index.wrapping_add(7) > table_limit as u32 {
            return None;
        }
        Some(table_base.wrapping_add(index))
    }

    fn set_accessed_bit(&mut self, selector: u16, bus: &mut impl common::Bus) {
        if let Some(addr) = self.descriptor_addr_checked(selector) {
            let rights = self.biu_read_u8_physical(bus, addr.wrapping_add(5) & ADDRESS_MASK);
            if rights & 0x01 == 0 {
                self.biu_write_u8_physical(bus, addr.wrapping_add(5) & ADDRESS_MASK, rights | 0x01);
            }
        }
    }

    fn invalidate_segment_if_needed(&mut self, seg: SegReg16, new_cpl: u16) {
        if !self.seg_valid[seg as usize] {
            return;
        }
        let rights = self.seg_rights[seg as usize];
        if !Self::descriptor_is_segment(rights) {
            self.set_null_segment(seg, 0);
            return;
        }
        if Self::descriptor_is_conforming_code(rights) {
            return;
        }
        let dpl = Self::descriptor_dpl(rights);
        if dpl < new_cpl {
            self.set_null_segment(seg, 0);
        }
    }

    fn read_word_phys(&mut self, bus: &mut impl common::Bus, addr: u32) -> u16 {
        self.biu_read_u16_pair(
            bus,
            addr & ADDRESS_MASK,
            addr.wrapping_add(1) & ADDRESS_MASK,
        )
    }

    fn write_word_phys(&mut self, bus: &mut impl common::Bus, addr: u32, value: u16) {
        self.biu_write_u16_pair(
            bus,
            addr & ADDRESS_MASK,
            addr.wrapping_add(1) & ADDRESS_MASK,
            value,
        );
    }

    fn read_descriptor_at(&mut self, bus: &mut impl common::Bus, addr: u32) -> SegmentDescriptor {
        let limit = self.read_word_phys(bus, addr);
        let base = u32::from(self.biu_read_u8_physical(bus, addr.wrapping_add(2) & ADDRESS_MASK))
            | (u32::from(self.biu_read_u8_physical(bus, addr.wrapping_add(3) & ADDRESS_MASK)) << 8)
            | (u32::from(self.biu_read_u8_physical(bus, addr.wrapping_add(4) & ADDRESS_MASK))
                << 16);
        let rights = self.biu_read_u8_physical(bus, addr.wrapping_add(5) & ADDRESS_MASK);
        SegmentDescriptor {
            base,
            limit,
            rights,
        }
    }

    fn switch_task(&mut self, ntask: u16, task_type: TaskType, bus: &mut impl common::Bus) {
        if ntask & 0x0004 != 0 {
            self.raise_fault_with_code(10, Self::segment_error_code(ntask), bus);
            return;
        }

        let Some(naddr) = self.descriptor_addr_checked(ntask) else {
            self.raise_fault_with_code(10, Self::segment_error_code(ntask), bus);
            return;
        };

        let ndesc = self.read_descriptor_at(bus, naddr);

        let r = ndesc.rights;
        if Self::descriptor_is_segment(r) || (r & 0x0D) != 0x01 {
            self.raise_fault_with_code(13, Self::segment_error_code(ntask), bus);
            return;
        }
        if task_type == TaskType::Iret {
            if r & 0x02 == 0 {
                self.raise_fault_with_code(13, Self::segment_error_code(ntask), bus);
                return;
            }
        } else if r & 0x02 != 0 {
            self.raise_fault_with_code(13, Self::segment_error_code(ntask), bus);
            return;
        }

        if !Self::descriptor_present(r) {
            self.raise_fault_with_code(11, Self::segment_error_code(ntask), bus);
            return;
        }

        if ndesc.limit < 43 {
            self.raise_fault_with_code(10, Self::segment_error_code(ntask), bus);
            return;
        }

        let mut flags = self.flags.compress();

        if task_type == TaskType::Call {
            self.write_word_phys(bus, ndesc.base, self.tr);
        }

        if task_type == TaskType::Iret {
            flags &= !0x4000;
        }

        // Save current state to old TSS.
        let old_base = self.tr_base;
        self.write_word_phys(bus, old_base.wrapping_add(14), self.ip);
        self.write_word_phys(bus, old_base.wrapping_add(16), flags);
        self.write_word_phys(bus, old_base.wrapping_add(18), self.regs.word(WordReg::AX));
        self.write_word_phys(bus, old_base.wrapping_add(20), self.regs.word(WordReg::CX));
        self.write_word_phys(bus, old_base.wrapping_add(22), self.regs.word(WordReg::DX));
        self.write_word_phys(bus, old_base.wrapping_add(24), self.regs.word(WordReg::BX));
        self.write_word_phys(bus, old_base.wrapping_add(26), self.regs.word(WordReg::SP));
        self.write_word_phys(bus, old_base.wrapping_add(28), self.regs.word(WordReg::BP));
        self.write_word_phys(bus, old_base.wrapping_add(30), self.regs.word(WordReg::SI));
        self.write_word_phys(bus, old_base.wrapping_add(32), self.regs.word(WordReg::DI));
        self.write_word_phys(
            bus,
            old_base.wrapping_add(34),
            self.sregs[SegReg16::ES as usize],
        );
        self.write_word_phys(
            bus,
            old_base.wrapping_add(36),
            self.sregs[SegReg16::CS as usize],
        );
        self.write_word_phys(
            bus,
            old_base.wrapping_add(38),
            self.sregs[SegReg16::SS as usize],
        );
        self.write_word_phys(
            bus,
            old_base.wrapping_add(40),
            self.sregs[SegReg16::DS as usize],
        );

        // Read all fields from new TSS.
        let new_base = ndesc.base;
        let ntss_ip = self.read_word_phys(bus, new_base.wrapping_add(14));
        let ntss_flags = self.read_word_phys(bus, new_base.wrapping_add(16));
        let ntss_ax = self.read_word_phys(bus, new_base.wrapping_add(18));
        let ntss_cx = self.read_word_phys(bus, new_base.wrapping_add(20));
        let ntss_dx = self.read_word_phys(bus, new_base.wrapping_add(22));
        let ntss_bx = self.read_word_phys(bus, new_base.wrapping_add(24));
        let ntss_sp = self.read_word_phys(bus, new_base.wrapping_add(26));
        let ntss_bp = self.read_word_phys(bus, new_base.wrapping_add(28));
        let ntss_si = self.read_word_phys(bus, new_base.wrapping_add(30));
        let ntss_di = self.read_word_phys(bus, new_base.wrapping_add(32));
        let ntss_es = self.read_word_phys(bus, new_base.wrapping_add(34));
        let ntss_cs = self.read_word_phys(bus, new_base.wrapping_add(36));
        let ntss_ss = self.read_word_phys(bus, new_base.wrapping_add(38));
        let ntss_ds = self.read_word_phys(bus, new_base.wrapping_add(40));
        let ntss_ldt = self.read_word_phys(bus, new_base.wrapping_add(42));

        // Mark old TSS idle (JMP/IRET).
        if task_type != TaskType::Call
            && let Some(oaddr) = self.descriptor_addr_checked(self.tr)
        {
            let old_rights = self.biu_read_u8_physical(bus, oaddr.wrapping_add(5) & ADDRESS_MASK);
            self.biu_write_u8_physical(
                bus,
                oaddr.wrapping_add(5) & ADDRESS_MASK,
                old_rights & !0x02,
            );
        }

        // Mark new TSS busy (CALL/JMP).
        if task_type != TaskType::Iret {
            let new_rights = self.biu_read_u8_physical(bus, naddr.wrapping_add(5) & ADDRESS_MASK);
            self.biu_write_u8_physical(
                bus,
                naddr.wrapping_add(5) & ADDRESS_MASK,
                new_rights | 0x02,
            );
        }

        // Update TR.
        self.tr = ntask;
        self.tr_limit = ndesc.limit;
        self.tr_base = ndesc.base;
        self.tr_rights = self.biu_read_u8_physical(bus, naddr.wrapping_add(5) & ADDRESS_MASK);

        // Load registers from new TSS.
        self.flags.expand(ntss_flags);
        self.regs.set_word(WordReg::AX, ntss_ax);
        self.regs.set_word(WordReg::CX, ntss_cx);
        self.regs.set_word(WordReg::DX, ntss_dx);
        self.regs.set_word(WordReg::BX, ntss_bx);
        self.regs.set_word(WordReg::SP, ntss_sp);
        self.regs.set_word(WordReg::BP, ntss_bp);
        self.regs.set_word(WordReg::SI, ntss_si);
        self.regs.set_word(WordReg::DI, ntss_di);

        // Load LDT from new TSS.
        if ntss_ldt & 0x0004 != 0 {
            self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
            return;
        }
        if ntss_ldt & 0xFFFC != 0 {
            let Some(ldtaddr) = self.descriptor_addr_checked(ntss_ldt) else {
                self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
                return;
            };
            let ldt_desc = self.read_descriptor_at(bus, ldtaddr);
            let lr = ldt_desc.rights;
            if Self::descriptor_is_segment(lr) || (lr & 0x0F) != 0x02 {
                self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
                return;
            }
            if !Self::descriptor_present(lr) {
                self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
                return;
            }
            self.ldtr = ntss_ldt;
            self.ldtr_base = ldt_desc.base;
            self.ldtr_limit = ldt_desc.limit;
        } else {
            self.ldtr = 0;
            self.ldtr_base = 0;
            self.ldtr_limit = 0;
        }

        if task_type == TaskType::Call {
            self.flags.nt = true;
        }

        self.msw |= 8;

        // Load segment registers from new TSS. SS first (uses new CS RPL as CPL).
        let new_cpl = ntss_cs & 3;
        self.load_task_data_segment(SegReg16::SS, ntss_ss, new_cpl, bus);
        self.load_task_code_segment(ntss_cs, ntss_ip, bus);
        let cpl = self.cpl();
        self.load_task_data_segment(SegReg16::ES, ntss_es, cpl, bus);
        self.load_task_data_segment(SegReg16::DS, ntss_ds, cpl, bus);
    }

    fn load_task_data_segment(
        &mut self,
        seg: SegReg16,
        selector: u16,
        required_cpl: u16,
        bus: &mut impl common::Bus,
    ) {
        if selector & 0xFFFC == 0 {
            self.set_null_segment(seg, selector);
            return;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        };
        let rights = descriptor.rights;
        if seg == SegReg16::SS {
            if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_writable(rights) {
                self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                return;
            }
            let dpl = Self::descriptor_dpl(rights);
            let rpl = selector & 3;
            if dpl != required_cpl || rpl != required_cpl {
                self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                return;
            }
        } else {
            if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_readable(rights) {
                self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                return;
            }
            if !Self::descriptor_is_conforming_code(rights) {
                let dpl = Self::descriptor_dpl(rights);
                let rpl = selector & 3;
                if dpl < required_cpl.max(rpl) {
                    self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                    return;
                }
            }
        }
        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        }
        self.set_accessed_bit(selector, bus);
        self.set_loaded_segment_cache(seg, selector, descriptor);
    }

    fn load_task_code_segment(&mut self, selector: u16, offset: u16, bus: &mut impl common::Bus) {
        if selector & 0xFFFC == 0 {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        };
        let rights = descriptor.rights;
        if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_code(rights) {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        }
        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
            return;
        }
        if offset > descriptor.limit {
            self.raise_fault_with_code(10, 0, bus);
            return;
        }
        self.set_accessed_bit(selector, bus);
        let cpl = selector & 3;
        let adjusted = (selector & !3) | cpl;
        self.set_loaded_segment_cache(SegReg16::CS, adjusted, descriptor);
        self.ip = offset;
    }

    fn code_descriptor(
        &mut self,
        selector: u16,
        offset: u16,
        gate: TaskType,
        old_cs: u16,
        old_ip: u16,
        bus: &mut impl common::Bus,
    ) -> bool {
        let Some(addr) = self.descriptor_addr_checked(selector) else {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        };

        let desc = self.read_descriptor_at(bus, addr);
        let r = desc.rights;
        let cpl = self.cpl();
        let rpl = selector & 3;

        if Self::descriptor_is_segment(r) {
            if !Self::descriptor_is_code(r) {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                return false;
            }
            if Self::descriptor_is_conforming_code(r) {
                if Self::descriptor_dpl(r) > cpl {
                    self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                    return false;
                }
            } else if rpl > cpl || Self::descriptor_dpl(r) != cpl {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                return false;
            }
            if !Self::descriptor_present(r) {
                self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
                return false;
            }
            if offset > desc.limit {
                self.raise_fault_with_code(13, 0, bus);
                return false;
            }
            self.set_accessed_bit(selector, bus);
            let adjusted = (selector & !3) | cpl;
            self.set_loaded_segment_cache(SegReg16::CS, adjusted, desc);
            self.ip = offset;
            if gate == TaskType::Call {
                self.push(bus, old_cs);
                self.push(bus, old_ip);
            }
            return true;
        }

        // System descriptor: gate DPL must be >= max(CPL, RPL).
        let dpl = Self::descriptor_dpl(r);
        if dpl < cpl.max(rpl) {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if !Self::descriptor_present(r) {
            self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
            return false;
        }

        let gate_type = r & 0x0F;
        match gate_type {
            4 => {
                // Call gate.
                let gate_offset = self.read_word_phys(bus, addr);
                let gate_selector = self.read_word_phys(bus, addr.wrapping_add(2));
                let gate_count = self.read_word_phys(bus, addr.wrapping_add(4)) & 0x1F;

                let Some(target_addr) = self.descriptor_addr_checked(gate_selector) else {
                    self.raise_fault_with_code(13, Self::segment_error_code(gate_selector), bus);
                    return false;
                };
                let target_desc = self.read_descriptor_at(bus, target_addr);
                let tr = target_desc.rights;
                if !Self::descriptor_is_code(tr) || !Self::descriptor_is_segment(tr) {
                    self.raise_fault_with_code(13, Self::segment_error_code(gate_selector), bus);
                    return false;
                }
                let target_dpl = Self::descriptor_dpl(tr);
                if target_dpl > cpl {
                    self.raise_fault_with_code(13, Self::segment_error_code(gate_selector), bus);
                    return false;
                }
                if !Self::descriptor_present(tr) {
                    self.raise_fault_with_code(11, Self::segment_error_code(gate_selector), bus);
                    return false;
                }
                if gate_offset > target_desc.limit {
                    self.raise_fault_with_code(13, 0, bus);
                    return false;
                }

                if !Self::descriptor_is_conforming_code(tr) && target_dpl < cpl {
                    // Inter-privilege call via call gate.
                    if gate == TaskType::Jmp {
                        self.raise_fault_with_code(
                            13,
                            Self::segment_error_code(gate_selector),
                            bus,
                        );
                        return false;
                    }

                    let tss_sp_offset = 2 + target_dpl * 4;
                    let tss_ss_offset = 4 + target_dpl * 4;
                    let tss_sp =
                        self.read_word_phys(bus, self.tr_base.wrapping_add(tss_sp_offset as u32));
                    let tss_ss =
                        self.read_word_phys(bus, self.tr_base.wrapping_add(tss_ss_offset as u32));

                    let saved_ss = self.sregs[SegReg16::SS as usize];
                    let saved_sp = self.regs.word(WordReg::SP);
                    let old_ss_base = self.seg_base(SegReg16::SS);

                    // Load new SS with target DPL as required privilege.
                    self.load_task_data_segment(SegReg16::SS, tss_ss, target_dpl, bus);
                    self.regs.set_word(WordReg::SP, tss_sp);

                    self.push(bus, saved_ss);
                    self.push(bus, saved_sp);
                    for i in (0..gate_count).rev() {
                        let param = self.read_word_phys(
                            bus,
                            old_ss_base.wrapping_add(saved_sp.wrapping_add(i * 2) as u32),
                        );
                        self.push(bus, param);
                    }
                }

                self.set_accessed_bit(gate_selector, bus);
                let adjusted = (gate_selector & !3) | target_dpl;
                self.set_loaded_segment_cache(SegReg16::CS, adjusted, target_desc);
                self.ip = gate_offset;
                if gate == TaskType::Call {
                    self.push(bus, old_cs);
                    self.push(bus, old_ip);
                }
                true
            }
            5 => {
                // Task gate: extract TSS selector and switch.
                let task_selector = self.read_word_phys(bus, addr.wrapping_add(2));
                self.switch_task(task_selector, gate, bus);
                let flags_val = self.flags.compress();
                let cpl = self.cpl();
                self.flags.load_flags(flags_val, cpl, true);
                true
            }
            1 => {
                // Idle TSS descriptor: direct task switch.
                self.switch_task(selector, gate, bus);
                let flags_val = self.flags.compress();
                let cpl = self.cpl();
                self.flags.load_flags(flags_val, cpl, true);
                true
            }
            3 => {
                // Busy TSS: #GP.
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                false
            }
            _ => {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                false
            }
        }
    }

    fn execute_one(&mut self, bus: &mut impl common::Bus) {
        self.prev_ip = self.ip;

        if self.pending_irq != 0 {
            self.check_interrupts(bus);
            self.biu_realign_to_current_ip();
        }
        if self.no_interrupt > 0 {
            self.no_interrupt -= 1;
        }
        if self.inhibit_all > 0 {
            self.inhibit_all -= 1;
        }

        self.seg_prefix = false;
        self.prefix_count_current = 0;
        self.lock_prefix_active = false;
        self.lock_demand_gap_emitted = false;

        if self.rep_active {
            self.continue_rep(bus);
        } else {
            let mut opcode = self.fetch_first(bus);
            loop {
                match opcode {
                    0x26 => {
                        self.prefix_count_current = self.prefix_count_current.saturating_add(1);
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::ES;
                        self.update_prefixed_decode_spill_fetch_gap();
                        opcode = self.fetch_after_prefix_delay(bus);
                    }
                    0x2E => {
                        self.prefix_count_current = self.prefix_count_current.saturating_add(1);
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::CS;
                        self.update_prefixed_decode_spill_fetch_gap();
                        opcode = self.fetch_after_prefix_delay(bus);
                    }
                    0x36 => {
                        self.prefix_count_current = self.prefix_count_current.saturating_add(1);
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::SS;
                        self.update_prefixed_decode_spill_fetch_gap();
                        opcode = self.fetch_after_prefix_delay(bus);
                    }
                    0x3E => {
                        self.prefix_count_current = self.prefix_count_current.saturating_add(1);
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg16::DS;
                        self.update_prefixed_decode_spill_fetch_gap();
                        opcode = self.fetch_after_prefix_delay(bus);
                    }
                    0xF0 => {
                        self.prefix_count_current = self.prefix_count_current.saturating_add(1);
                        self.lock_prefix_active = true;
                        self.update_prefixed_decode_spill_fetch_gap();
                        opcode = self.fetch_after_prefix_delay(bus);
                    }
                    _ => {
                        self.biu_start_execute();
                        self.dispatch(opcode, bus);
                        break;
                    }
                }
            }
        }

        if !self.halted && !self.shutdown && !self.rep_active {
            self.biu_realign_to_current_ip();
            self.biu_fetch_next(bus);
            self.biu_finish_instruction();
        }
    }

    /// Executes exactly one logical instruction (should only be used in tests).
    ///
    /// Sets `cycles_remaining` to `i64::MAX` so that REP-prefixed string
    /// instructions run to completion in a single call instead of pausing
    /// mid-loop when the cycle budget runs out. This is necessary because
    /// `execute_one` resumes a paused REP via `continue_rep`, which would
    /// otherwise split a single logical instruction across multiple calls.
    pub fn step(&mut self, bus: &mut impl common::Bus) {
        self.cycles_remaining = i64::MAX;
        self.execute_one(bus);
    }

    /// Returns the number of cycles consumed by the last `step()` call.
    pub fn cycles_consumed(&self) -> u64 {
        (i64::MAX - self.cycles_remaining) as u64
    }

    /// Returns the last computed effective address (for alignment checks).
    pub fn last_ea(&self) -> u32 {
        self.ea
    }

    /// Signals a maskable interrupt request (IRQ).
    pub fn signal_irq(&mut self) {
        self.pending_irq |= crate::PENDING_IRQ;
    }

    /// Signals a non-maskable interrupt (NMI).
    pub fn signal_nmi(&mut self) {
        self.pending_irq |= crate::PENDING_NMI;
    }
}

impl common::Cpu for I286 {
    crate::impl_cpu_run_for!();

    fn reset(&mut self) {
        // GP registers (AX, BX, CX, DX, SP, BP, SI, DI) are preserved across
        // reset on real 286 hardware. Intel documents them as "undefined", but
        // the register file is not cleared by the RESET signal. The PC-98 VX
        // BIOS relies on SP surviving the warm reset triggered via port 0xF0
        // after testing extended memory in protected mode.

        // Reset segment registers and their caches.
        self.sregs = [0; 4];
        self.set_real_segment_cache(SegReg16::ES, 0);
        self.set_real_segment_cache(SegReg16::CS, 0);
        self.set_real_segment_cache(SegReg16::SS, 0);
        self.set_real_segment_cache(SegReg16::DS, 0);
        self.sregs[SegReg16::CS as usize] = 0xFFFF;
        self.seg_bases[SegReg16::CS as usize] = 0xFFFF0;

        // Reset control registers.
        self.msw = 0xFFF0;
        self.ip = 0;
        self.flags = I286Flags::default();

        // Reset descriptor table registers.
        self.idt_base = 0;
        self.idt_limit = 0x03FF;
        self.gdt_base = 0;
        self.gdt_limit = 0;
        self.ldtr = 0;
        self.ldtr_base = 0;
        self.ldtr_limit = 0;
        self.tr = 0;
        self.tr_base = 0;
        self.tr_limit = 0;
        self.tr_rights = 0;

        // Reset runtime state.
        self.prev_ip = 0;
        self.halted = false;
        self.pending_irq = 0;
        self.no_interrupt = 0;
        self.inhibit_all = 0;
        self.rep_active = false;
        self.rep_restart_ip = 0;
        self.rep_type = 0;
        self.seg_prefix = false;
        self.ea = 0;
        self.eo = 0;
        self.ea_seg = SegReg16::DS;
        self.lock_prefix_active = false;
        self.lock_demand_gap_emitted = false;
        self.trap_level = 0;
        self.shutdown = false;
        self.biu_reset_front_end();
        self.trace.clear();
        self.cycle_counter = 0;
    }

    fn halted(&self) -> bool {
        self.halted || self.shutdown
    }

    fn warm_reset(&mut self, ss: u16, sp: u16, cs: u16, ip: u16) {
        self.reset();
        self.sregs[SegReg16::SS as usize] = ss;
        self.set_real_segment_cache(SegReg16::SS, ss);
        self.state.set_sp(sp);
        self.sregs[SegReg16::CS as usize] = cs;
        self.set_real_segment_cache(SegReg16::CS, cs);
        self.ip = ip;
        self.biu_reset_front_end();
    }

    fn ax(&self) -> u16 {
        self.state.ax()
    }

    fn set_ax(&mut self, v: u16) {
        self.state.set_ax(v);
    }

    fn bx(&self) -> u16 {
        self.state.bx()
    }

    fn set_bx(&mut self, v: u16) {
        self.state.set_bx(v);
    }

    fn cx(&self) -> u16 {
        self.state.cx()
    }

    fn set_cx(&mut self, v: u16) {
        self.state.set_cx(v);
    }

    fn dx(&self) -> u16 {
        self.state.dx()
    }

    fn set_dx(&mut self, v: u16) {
        self.state.set_dx(v);
    }

    fn sp(&self) -> u16 {
        self.state.sp()
    }

    fn set_sp(&mut self, v: u16) {
        self.state.set_sp(v);
    }

    fn bp(&self) -> u16 {
        self.state.bp()
    }

    fn set_bp(&mut self, v: u16) {
        self.state.set_bp(v);
    }

    fn si(&self) -> u16 {
        self.state.si()
    }

    fn set_si(&mut self, v: u16) {
        self.state.set_si(v);
    }

    fn di(&self) -> u16 {
        self.state.di()
    }

    fn set_di(&mut self, v: u16) {
        self.state.set_di(v);
    }

    fn es(&self) -> u16 {
        self.state.es()
    }

    fn set_es(&mut self, v: u16) {
        self.state.set_es(v);
    }

    fn cs(&self) -> u16 {
        self.state.cs()
    }

    fn set_cs(&mut self, v: u16) {
        self.state.set_cs(v);
        self.biu_reset_front_end();
    }

    fn ss(&self) -> u16 {
        self.state.ss()
    }

    fn set_ss(&mut self, v: u16) {
        self.state.set_ss(v);
    }

    fn ds(&self) -> u16 {
        self.state.ds()
    }

    fn set_ds(&mut self, v: u16) {
        self.state.set_ds(v);
    }

    fn ip(&self) -> u16 {
        self.state.ip
    }

    fn set_ip(&mut self, v: u16) {
        self.state.ip = v;
        self.biu_reset_front_end();
    }

    fn flags(&self) -> u16 {
        self.state.compressed_flags()
    }

    fn set_flags(&mut self, v: u16) {
        self.state.set_compressed_flags(v);
    }

    fn cpu_type(&self) -> common::CpuType {
        common::CpuType::I286
    }

    fn load_segment_real_mode(&mut self, seg: common::SegmentRegister, selector: u16) {
        let seg16 = match seg {
            common::SegmentRegister::ES => SegReg16::ES,
            common::SegmentRegister::CS => SegReg16::CS,
            common::SegmentRegister::SS => SegReg16::SS,
            common::SegmentRegister::DS => SegReg16::DS,
        };
        self.state.sregs[seg16 as usize] = selector;
        self.set_real_segment_cache(seg16, selector);
    }

    fn segment_base(&self, seg: common::SegmentRegister) -> u32 {
        let seg16 = match seg {
            common::SegmentRegister::ES => SegReg16::ES,
            common::SegmentRegister::CS => SegReg16::CS,
            common::SegmentRegister::SS => SegReg16::SS,
            common::SegmentRegister::DS => SegReg16::DS,
        };
        self.state.seg_bases[seg16 as usize]
    }
}
