//! T-state cycle Bus Interface Unit for the NEC V20.
//!
//! This module implements the V20's T-state and Ta-state machine.
//! The structure mirrors the 8086 BIU but is configured for V20 bus
//! parameters: a 4-byte prefetch queue, an 8-bit data bus that fetches one
//! byte per m-cycle, and no BHE/byte-lane logic.

use common::Bus;

use super::V30;
use crate::SegReg16;

/// V20 instruction queue capacity (the V30 widens this to 6).
pub const QUEUE_SIZE: usize = 4;
/// V20 prefetch fetch width: the 8-bit bus fetches one byte per m-cycle.
pub const FETCH_SIZE: usize = 1;
/// 20-bit physical address wrap mask (V20 has 1 MiB of address space).
pub const ADDRESS_MASK: u32 = 0xFFFFF;
/// Safety bound on the inner T-state poll loops used by the BIU helpers.
const BIU_LOOP_GUARD: u32 = 64;

/// Main bus cycle T-state.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum TCycle {
    Tinit,
    #[default]
    Ti,
    T1,
    T2,
    T3,
    Tw,
    T4,
}

/// Pipelined address cycle Ta-state.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum TaCycle {
    Tr,
    Ts,
    T0,
    #[default]
    Td,
    Ta,
}

/// Bus cycle status (mapped to S0-S2 pins).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum BusStatus {
    IoRead,
    IoWrite,
    CodeFetch,
    MemRead,
    MemWrite,
    #[default]
    Passive,
}

/// Prefetch state machine.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FetchState {
    #[default]
    Normal,
    PausedFull,
    Delayed(u8),
    Suspended,
}

/// EU bus request pending type.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum BusPendingType {
    #[default]
    None,
    EuEarly,
    EuLate,
}

/// Bus transfer size for a single m-cycle. The V20 always issues byte
/// transfers on the external bus; word EU operations decompose into two
/// byte m-cycles. The variant is kept so that the EU-side helpers can
/// signal "this is part of a 16-bit operand" via [`OperandSize`].
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum TransferSize {
    #[default]
    Byte,
}

/// Operand size spanning one or two m-cycles.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum OperandSize {
    #[default]
    Operand8,
    Operand16,
}

/// Whether a queue read is the first byte of an instruction or a subsequent byte.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QueueType {
    First,
    Subsequent,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum QueueOp {
    #[default]
    Idle,
    First,
    Flush,
    Subsequent,
}

/// Coarse T-state phase exposed in the cycle trace.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[allow(missing_docs)]
pub enum V30BusPhase {
    #[default]
    Ti,
    T1,
    T2,
    T3,
    Tw,
    T4,
}

/// High-level bus status class captured by the cycle trace.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[allow(missing_docs)]
pub enum V30TraceBusStatus {
    #[default]
    Passive,
    Code,
    MemoryRead,
    MemoryWrite,
    IoRead,
    IoWrite,
    Halt,
    InterruptAck,
}

/// Queue operation reported per cycle.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[allow(missing_docs)]
pub enum V30QueueOpTrace {
    #[default]
    Idle,
    First,
    Subsequent,
    Flush,
}

/// Internal T-state captured for diagnostics.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[allow(missing_docs)]
pub enum V30TraceTCycle {
    Tinit,
    #[default]
    Ti,
    T1,
    T2,
    T3,
    Tw,
    T4,
}

/// Internal Ta-state captured for diagnostics.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[allow(missing_docs)]
pub enum V30TraceTaCycle {
    Tr,
    Ts,
    T0,
    #[default]
    Td,
    Ta,
}

/// Internal prefetch state captured for diagnostics.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[allow(missing_docs)]
pub enum V30TraceFetchState {
    #[default]
    Normal,
    PausedFull,
    Delayed,
    Suspended,
}

/// Per-cycle trace entry emitted by the V30 BIU when capture is enabled.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[allow(missing_docs)]
pub struct V30CycleTraceEntry {
    pub cycle: u64,
    pub phase: V30BusPhase,
    pub status: V30TraceBusStatus,
    pub address: u32,
    pub data: u8,
    pub ale: bool,
    pub queue_op: V30QueueOpTrace,
    pub queue_byte: u8,
    pub t_cycle: V30TraceTCycle,
    pub ta_cycle: V30TraceTaCycle,
    pub fetch_state: V30TraceFetchState,
    pub fetch_delay: u8,
    pub bus_status_latch: V30TraceBusStatus,
    pub queue_len: u8,
}

#[allow(dead_code)]
impl V30 {
    #[inline(always)]
    pub(super) fn queue_len(&self) -> usize {
        self.instruction_queue_len
    }

    #[inline(always)]
    pub(super) fn queue_has_room_for_fetch(&self) -> bool {
        self.instruction_queue_len + FETCH_SIZE <= QUEUE_SIZE
    }

    #[inline(always)]
    pub(super) fn queue_pop(&mut self) -> u8 {
        debug_assert!(self.instruction_queue_len > 0, "queue underrun");
        let value = self.instruction_queue[0];
        if self.instruction_queue_len > 1 {
            self.instruction_queue
                .copy_within(1..self.instruction_queue_len, 0);
        }
        self.instruction_queue_len -= 1;
        value
    }

    #[inline(always)]
    pub(super) fn queue_push8(&mut self, byte: u8) -> u16 {
        debug_assert!(self.instruction_queue_len < QUEUE_SIZE, "queue overrun");
        self.instruction_queue[self.instruction_queue_len] = byte;
        self.instruction_queue_len += 1;
        1
    }

    pub(super) fn queue_flush(&mut self) {
        self.instruction_queue_len = 0;
        self.instruction_preload = None;
        self.fetch_state = FetchState::Normal;
        self.queue_op = QueueOp::Flush;
    }

    fn trace_phase_from(t_cycle: TCycle) -> V30BusPhase {
        match t_cycle {
            TCycle::Tinit | TCycle::T1 => V30BusPhase::T1,
            TCycle::T2 => V30BusPhase::T2,
            TCycle::T3 => V30BusPhase::T3,
            TCycle::Tw => V30BusPhase::Tw,
            TCycle::T4 => V30BusPhase::T4,
            TCycle::Ti => V30BusPhase::Ti,
        }
    }

    fn current_trace_phase(&self) -> V30BusPhase {
        Self::trace_phase_from(self.t_cycle)
    }

    fn trace_status_from(&self, status: BusStatus) -> V30TraceBusStatus {
        match status {
            BusStatus::CodeFetch => V30TraceBusStatus::Code,
            BusStatus::MemRead => V30TraceBusStatus::MemoryRead,
            BusStatus::MemWrite => V30TraceBusStatus::MemoryWrite,
            BusStatus::IoRead => V30TraceBusStatus::IoRead,
            BusStatus::IoWrite => V30TraceBusStatus::IoWrite,
            BusStatus::Passive => {
                if self.halted {
                    V30TraceBusStatus::Halt
                } else {
                    V30TraceBusStatus::Passive
                }
            }
        }
    }

    fn current_trace_status(&self) -> V30TraceBusStatus {
        let visible_status = match self.current_trace_phase() {
            V30BusPhase::T1 | V30BusPhase::T2 => self.bus_status_latch,
            V30BusPhase::Ti | V30BusPhase::T3 | V30BusPhase::Tw | V30BusPhase::T4 => {
                BusStatus::Passive
            }
        };
        self.trace_status_from(visible_status)
    }

    fn current_trace_queue_op(&self) -> V30QueueOpTrace {
        match self.last_queue_op {
            QueueOp::First => V30QueueOpTrace::First,
            QueueOp::Subsequent => V30QueueOpTrace::Subsequent,
            QueueOp::Flush => V30QueueOpTrace::Flush,
            QueueOp::Idle => V30QueueOpTrace::Idle,
        }
    }

    fn current_trace_data(&self) -> u8 {
        if self.bus_status_latch != BusStatus::Passive
            && matches!(
                self.current_trace_phase(),
                V30BusPhase::T3 | V30BusPhase::Tw
            )
        {
            self.data_bus as u8
        } else {
            0
        }
    }

    fn trace_t_cycle_from(t_cycle: TCycle) -> V30TraceTCycle {
        match t_cycle {
            TCycle::Tinit => V30TraceTCycle::Tinit,
            TCycle::Ti => V30TraceTCycle::Ti,
            TCycle::T1 => V30TraceTCycle::T1,
            TCycle::T2 => V30TraceTCycle::T2,
            TCycle::T3 => V30TraceTCycle::T3,
            TCycle::Tw => V30TraceTCycle::Tw,
            TCycle::T4 => V30TraceTCycle::T4,
        }
    }

    fn current_trace_t_cycle(&self) -> V30TraceTCycle {
        Self::trace_t_cycle_from(self.t_cycle)
    }

    fn current_trace_ta_cycle(&self) -> V30TraceTaCycle {
        match self.ta_cycle {
            TaCycle::Tr => V30TraceTaCycle::Tr,
            TaCycle::Ts => V30TraceTaCycle::Ts,
            TaCycle::T0 => V30TraceTaCycle::T0,
            TaCycle::Td => V30TraceTaCycle::Td,
            TaCycle::Ta => V30TraceTaCycle::Ta,
        }
    }

    fn current_trace_fetch_state(&self) -> (V30TraceFetchState, u8) {
        match self.fetch_state {
            FetchState::Normal => (V30TraceFetchState::Normal, 0),
            FetchState::PausedFull => (V30TraceFetchState::PausedFull, 0),
            FetchState::Delayed(delay) => (V30TraceFetchState::Delayed, delay),
            FetchState::Suspended => (V30TraceFetchState::Suspended, 0),
        }
    }

    fn push_trace_entry(&mut self) {
        self.push_trace_entry_snapshot(
            self.t_cycle,
            self.bus_status_latch,
            self.address_latch,
            self.data_bus,
        );
    }

    fn push_trace_entry_snapshot(
        &mut self,
        trace_t_cycle: TCycle,
        trace_bus_status_latch: BusStatus,
        trace_address_latch: u32,
        trace_data_bus: u16,
    ) {
        if !self.cycle_trace_enabled {
            return;
        }
        let (fetch_state, fetch_delay) = self.current_trace_fetch_state();
        let phase = Self::trace_phase_from(trace_t_cycle);
        let entry = V30CycleTraceEntry {
            cycle: self.cycle_num,
            phase,
            status: match phase {
                V30BusPhase::T1 | V30BusPhase::T2 => self.trace_status_from(trace_bus_status_latch),
                V30BusPhase::Ti | V30BusPhase::T3 | V30BusPhase::Tw | V30BusPhase::T4 => {
                    self.trace_status_from(BusStatus::Passive)
                }
            },
            address: trace_address_latch & ADDRESS_MASK,
            data: if trace_bus_status_latch != BusStatus::Passive
                && matches!(phase, V30BusPhase::T3 | V30BusPhase::Tw)
            {
                trace_data_bus as u8
            } else {
                0
            },
            ale: trace_t_cycle == TCycle::T1,
            queue_op: self.current_trace_queue_op(),
            queue_byte: self.last_queue_byte,
            t_cycle: Self::trace_t_cycle_from(trace_t_cycle),
            ta_cycle: self.current_trace_ta_cycle(),
            fetch_state,
            fetch_delay,
            bus_status_latch: self.trace_status_from(trace_bus_status_latch),
            queue_len: self.instruction_queue_len as u8,
        };
        self.cycle_trace.push(entry);
    }

    /// Advance the CPU by one T-state. This is the unit of time used by the
    /// T-state BIU. Every EU or BIU operation ultimately reduces to a sequence
    /// of `cycle` calls.
    pub(super) fn cycle(&mut self, bus: &mut impl Bus) {
        let trace_started_from_tinit = self.t_cycle == TCycle::Tinit;
        if self.t_cycle == TCycle::Tinit {
            self.t_cycle = TCycle::T1;
        }
        let trace_tinit_bus_status_latch = self.bus_status_latch;
        let trace_tinit_address_latch = self.address_latch;

        self.cycles_remaining -= 1;
        self.cycle_num = self.cycle_num.wrapping_add(1);

        match self.bus_status_latch {
            BusStatus::Passive => {
                self.transfer_n = 0;
                match self.fetch_state {
                    FetchState::Delayed(0) => {
                        self.fetch_state = FetchState::Normal;
                        self.biu_make_fetch_decision();
                    }
                    FetchState::PausedFull if self.queue_has_room_for_fetch() => {
                        self.biu_make_fetch_decision();
                    }
                    _ => {}
                }
            }
            BusStatus::MemRead
            | BusStatus::MemWrite
            | BusStatus::IoRead
            | BusStatus::IoWrite
            | BusStatus::CodeFetch => match self.t_cycle {
                TCycle::Tinit => unreachable!("Tinit was translated to T1 above"),
                TCycle::Ti => {
                    self.biu_make_fetch_decision();
                }
                TCycle::T1 => {}
                TCycle::T2 => {
                    if self.final_transfer {
                        self.biu_make_fetch_decision();
                    }
                }
                TCycle::T3 | TCycle::Tw => {
                    self.biu_do_bus_transfer(bus);
                }
                TCycle::T4 => {
                    if self.bus_status_latch == BusStatus::CodeFetch {
                        let inc = self.queue_push8(self.data_bus as u8);
                        self.prefetch_ip = self.prefetch_ip.wrapping_add(inc);
                    }
                    if self.final_transfer {
                        self.biu_make_fetch_decision();
                    }
                }
            },
        }

        if let FetchState::Delayed(count) = self.fetch_state
            && self.t_cycle != TCycle::Tw
            && count > 0
        {
            self.fetch_state = FetchState::Delayed(count - 1);
        }

        self.ta_cycle = match self.ta_cycle {
            TaCycle::Tr => TaCycle::Ts,
            TaCycle::Ts => TaCycle::T0,
            TaCycle::T0 => match (self.pl_status, self.bus_pending) {
                (BusStatus::CodeFetch, BusPendingType::None) => {
                    if matches!(self.t_cycle, TCycle::Ti | TCycle::T4)
                        && self.fetch_state != FetchState::Suspended
                    {
                        self.biu_bus_begin_fetch();
                        TaCycle::Td
                    } else {
                        TaCycle::T0
                    }
                }
                (BusStatus::CodeFetch, BusPendingType::EuLate) => {
                    if matches!(self.t_cycle, TCycle::Ti | TCycle::T4) {
                        self.biu_fetch_abort();
                        self.ta_cycle
                    } else {
                        TaCycle::T0
                    }
                }
                _ => {
                    if matches!(self.t_cycle, TCycle::Ti | TCycle::T4) {
                        TaCycle::Td
                    } else {
                        TaCycle::T0
                    }
                }
            },
            TaCycle::Td => TaCycle::Td,
            TaCycle::Ta => TaCycle::Ta,
        };

        self.t_cycle = if trace_started_from_tinit {
            TCycle::T1
        } else {
            match self.t_cycle {
                TCycle::Tinit => TCycle::T1,
                TCycle::Ti => match self.bus_status_latch {
                    BusStatus::Passive => TCycle::Ti,
                    _ => TCycle::T1,
                },
                TCycle::T1 => match self.bus_status_latch {
                    BusStatus::Passive => TCycle::T1,
                    _ => TCycle::T2,
                },
                TCycle::T2 => TCycle::T3,
                TCycle::Tw | TCycle::T3 => {
                    self.biu_bus_end();
                    TCycle::T4
                }
                TCycle::T4 => {
                    self.bus_status_latch = BusStatus::Passive;
                    TCycle::Ti
                }
            }
        };

        self.last_queue_op = self.queue_op;
        self.queue_op = QueueOp::Idle;

        if trace_started_from_tinit {
            self.push_trace_entry_snapshot(
                TCycle::T1,
                trace_tinit_bus_status_latch,
                trace_tinit_address_latch,
                self.data_bus,
            );
        } else {
            self.push_trace_entry();
        }
        self.last_queue_byte = 0;
    }

    #[inline]
    pub(super) fn cycles(&mut self, bus: &mut impl Bus, count: u32) {
        for _ in 0..count {
            self.cycle(bus);
        }
    }

    fn biu_bus_end(&mut self) {}

    fn biu_do_bus_transfer(&mut self, bus: &mut impl Bus) {
        match self.bus_status_latch {
            BusStatus::CodeFetch | BusStatus::MemRead => {
                let byte = bus.read_byte(self.address_latch & ADDRESS_MASK);
                self.data_bus = byte as u16;
            }
            BusStatus::MemWrite => {
                bus.write_byte(self.address_latch & ADDRESS_MASK, self.data_bus as u8);
            }
            BusStatus::IoRead => {
                let port = (self.address_latch & 0xFFFF) as u16;
                let byte = bus.io_read_byte(port);
                self.data_bus = byte as u16;
            }
            BusStatus::IoWrite => {
                let port = (self.address_latch & 0xFFFF) as u16;
                bus.io_write_byte(port, self.data_bus as u8);
            }
            BusStatus::Passive => {}
        }
        self.bus_status = BusStatus::Passive;
    }

    fn biu_address_start(&mut self, new_bus_status: BusStatus) {
        self.ta_cycle = if self.ta_cycle == TaCycle::Ta {
            TaCycle::Ts
        } else {
            TaCycle::Tr
        };
        self.pl_status = new_bus_status;
    }

    fn biu_bus_begin_fetch(&mut self) {
        if self.queue_has_room_for_fetch() {
            self.operand_size = OperandSize::Operand8;
            self.fetch_state = FetchState::Normal;
            self.pl_status = BusStatus::Passive;
            self.bus_status = BusStatus::CodeFetch;
            self.bus_status_latch = BusStatus::CodeFetch;
            self.t_cycle = TCycle::Tinit;
            let cs_base = self.seg_base(SegReg16::CS);
            self.address_latch = cs_base.wrapping_add(self.prefetch_ip as u32) & ADDRESS_MASK;
            self.address_bus = self.address_latch;
            self.transfer_size = TransferSize::Byte;
            self.transfer_n = 1;
            self.final_transfer = true;
        }
    }

    fn biu_fetch_abort(&mut self) {
        self.ta_cycle = TaCycle::Ta;
    }

    pub(super) fn biu_fetch_suspend(&mut self, bus: &mut impl Bus) {
        self.fetch_state = FetchState::Suspended;
        if self.bus_status_latch == BusStatus::CodeFetch {
            self.biu_bus_wait_finish(bus);
        }
        self.ta_cycle = TaCycle::Td;
        self.pl_status = BusStatus::Passive;
    }

    pub(super) fn biu_fetch_resume_immediate_for_eu(&mut self) {
        if self.fetch_state == FetchState::Suspended {
            self.fetch_state = FetchState::Normal;
            self.biu_complete_current_bus_for_eu();
            self.biu_start_code_fetch_for_eu();
        }
    }

    pub(super) fn biu_queue_flush(&mut self, bus: &mut impl Bus) {
        let _ = bus;
        self.queue_flush();
        self.fetch_state = FetchState::Normal;
        self.biu_fetch_start();
    }

    pub(super) fn biu_queue_flush_early(&mut self, bus: &mut impl Bus) {
        let _ = bus;
        self.queue_flush();
        self.fetch_state = FetchState::Normal;
        self.biu_fetch_start();
        if self.pl_status == BusStatus::CodeFetch && self.ta_cycle == TaCycle::Tr {
            self.ta_cycle = TaCycle::Ts;
        }
    }

    fn biu_fetch_start(&mut self) {
        if self.bus_pending == BusPendingType::EuEarly || self.pl_status == BusStatus::CodeFetch {
            return;
        }
        match self.fetch_state {
            FetchState::Delayed(_) => {}
            _ => {
                self.fetch_state = FetchState::Normal;
                self.biu_address_start(BusStatus::CodeFetch);
            }
        }
    }

    fn biu_make_fetch_decision(&mut self) {
        if !self.queue_has_room_for_fetch() {
            self.fetch_state = FetchState::PausedFull;
            return;
        }
        if self.bus_pending == BusPendingType::EuEarly {
            return;
        }
        if self.fetch_state == FetchState::Suspended {
            return;
        }
        if self.ta_cycle == TaCycle::Td {
            self.biu_fetch_start();
        }
    }

    pub(super) fn biu_instruction_entry_queue_len_for_timing(&self) -> usize {
        self.instruction_entry_queue_bytes as usize
    }

    pub(super) fn biu_latch_is_code_fetch(&self) -> bool {
        self.bus_status_latch == BusStatus::CodeFetch
    }

    pub(super) fn biu_prepare_memory_read(&mut self) {
        self.biu_address_start(BusStatus::MemRead);
    }

    pub(super) fn biu_prepare_memory_write(&mut self) {
        self.biu_address_start(BusStatus::MemWrite);
    }

    pub(super) fn biu_ready_memory_read(&mut self) {
        self.pl_status = BusStatus::MemRead;
        self.ta_cycle = TaCycle::Td;
    }

    pub(super) fn biu_ready_memory_write(&mut self) {
        self.pl_status = BusStatus::MemWrite;
        self.ta_cycle = TaCycle::Td;
    }

    pub(super) fn biu_complete_code_fetch_for_eu(&mut self) {
        if self.t_cycle == TCycle::T4 && self.bus_status_latch == BusStatus::CodeFetch {
            let inc = self.queue_push8(self.data_bus as u8);
            self.prefetch_ip = self.prefetch_ip.wrapping_add(inc);
            self.bus_status = BusStatus::Passive;
            self.bus_status_latch = BusStatus::Passive;
            self.t_cycle = TCycle::Ti;
            self.ta_cycle = TaCycle::Td;
            self.pl_status = BusStatus::Passive;
            if !self.queue_has_room_for_fetch() {
                self.fetch_state = FetchState::PausedFull;
            }
        }
    }

    pub(super) fn biu_complete_current_bus_for_eu(&mut self) {
        if self.t_cycle == TCycle::T4 && self.bus_status_latch != BusStatus::CodeFetch {
            self.bus_status = BusStatus::Passive;
            self.bus_status_latch = BusStatus::Passive;
            self.t_cycle = TCycle::Ti;
            self.ta_cycle = TaCycle::Td;
            self.pl_status = BusStatus::Passive;
        }
    }

    pub(super) fn biu_start_code_fetch_for_eu(&mut self) {
        debug_assert_eq!(self.bus_status_latch, BusStatus::Passive);
        debug_assert_eq!(self.t_cycle, TCycle::Ti);

        if self.queue_has_room_for_fetch() {
            self.operand_size = OperandSize::Operand8;
            self.fetch_state = FetchState::Normal;
            self.pl_status = BusStatus::Passive;
            self.bus_status = BusStatus::CodeFetch;
            self.bus_status_latch = BusStatus::CodeFetch;
            self.t_cycle = TCycle::Tinit;
            let cs_base = self.seg_base(SegReg16::CS);
            self.address_latch = cs_base.wrapping_add(self.prefetch_ip as u32) & ADDRESS_MASK;
            self.address_bus = self.address_latch;
            self.transfer_size = TransferSize::Byte;
            self.transfer_n = 1;
            self.final_transfer = true;
        }
    }

    pub(super) fn biu_queue_flush_and_start_code_fetch_for_eu(&mut self) {
        self.queue_flush();
        self.fetch_state = FetchState::Normal;
        self.biu_start_code_fetch_for_eu();
    }

    #[inline]
    pub(super) fn biu_bus_wait_finish(&mut self, bus: &mut impl Bus) {
        if self.bus_status_latch != BusStatus::Passive {
            let mut guard = 0u32;
            while self.t_cycle != TCycle::T4 && guard < BIU_LOOP_GUARD {
                self.cycle(bus);
                guard += 1;
            }
        }
    }

    #[inline]
    fn biu_bus_wait_until_tx(&mut self, bus: &mut impl Bus) {
        if matches!(
            self.bus_status_latch,
            BusStatus::MemRead
                | BusStatus::MemWrite
                | BusStatus::IoRead
                | BusStatus::IoWrite
                | BusStatus::CodeFetch
        ) {
            let mut guard = 0u32;
            while self.t_cycle != TCycle::T3 && guard < BIU_LOOP_GUARD {
                self.cycle(bus);
                guard += 1;
            }
        }
    }

    #[inline]
    fn biu_bus_wait_delay(&mut self, bus: &mut impl Bus) -> bool {
        match self.fetch_state {
            FetchState::Delayed(0) => {
                self.ta_cycle = TaCycle::Td;
                true
            }
            FetchState::Delayed(delay) => {
                self.cycles(bus, delay as u32);
                self.ta_cycle = TaCycle::Ta;
                true
            }
            _ => false,
        }
    }

    #[inline]
    fn biu_bus_wait_address(&mut self, bus: &mut impl Bus) {
        let mut guard = 0u32;
        while !matches!(self.ta_cycle, TaCycle::Td | TaCycle::Ta) && guard < BIU_LOOP_GUARD {
            self.cycle(bus);
            guard += 1;
        }
    }

    fn biu_complete_pending_fetch_before_eu(
        &mut self,
        bus: &mut impl Bus,
        new_bus_status: BusStatus,
    ) -> bool {
        if self.t_cycle != TCycle::Ti
            || self.pl_status != BusStatus::CodeFetch
            || self.bus_status_latch != BusStatus::Passive
        {
            return false;
        }

        let mut guard = 0u32;
        while self.bus_status_latch == BusStatus::Passive
            && self.pl_status == BusStatus::CodeFetch
            && guard < BIU_LOOP_GUARD
        {
            self.cycle(bus);
            guard += 1;
        }

        if self.bus_status_latch != BusStatus::CodeFetch {
            return false;
        }

        self.bus_pending = BusPendingType::EuEarly;
        self.biu_address_start(new_bus_status);
        self.biu_bus_wait_finish(bus);
        self.biu_complete_code_fetch_for_eu();
        true
    }

    fn biu_complete_first_byte_fetch_handoff_before_eu(
        &mut self,
        bus: &mut impl Bus,
        new_bus_status: BusStatus,
    ) -> bool {
        if new_bus_status != BusStatus::MemWrite
            || self.bus_status_latch != BusStatus::CodeFetch
            || !matches!(self.t_cycle, TCycle::T1 | TCycle::T2)
            || self.last_queue_op != QueueOp::First
        {
            return false;
        }

        self.biu_bus_wait_finish(bus);
        if self.t_cycle == TCycle::T4 && self.pl_status == BusStatus::CodeFetch {
            self.cycle(bus);
        }
        if self.bus_status_latch != BusStatus::CodeFetch {
            return false;
        }

        self.bus_pending = BusPendingType::EuEarly;
        self.biu_address_start(new_bus_status);
        self.biu_bus_wait_finish(bus);
        self.biu_complete_code_fetch_for_eu();
        true
    }

    /// Begin an EU bus m-cycle. Handles bus-cycle synchronisation, fetch
    /// aborts, and address cycle setup before starting T1.
    #[allow(clippy::too_many_arguments)]
    fn biu_bus_begin(
        &mut self,
        bus: &mut impl Bus,
        new_bus_status: BusStatus,
        address: u32,
        data: u16,
        op_size: OperandSize,
        first: bool,
    ) {
        debug_assert_ne!(
            new_bus_status,
            BusStatus::CodeFetch,
            "biu_bus_begin cannot start a code fetch"
        );

        let mut fetch_abort = false;
        let cold_first_byte_read_handoff = matches!(
            new_bus_status,
            BusStatus::MemRead | BusStatus::IoRead | BusStatus::IoWrite
        ) && self.bus_status_latch == BusStatus::CodeFetch
            && matches!(self.t_cycle, TCycle::T1 | TCycle::T2)
            && self.last_queue_op == QueueOp::First;

        let eu_address_pipelined = self
            .biu_complete_first_byte_fetch_handoff_before_eu(bus, new_bus_status)
            || self.biu_complete_pending_fetch_before_eu(bus, new_bus_status);

        match self.t_cycle {
            TCycle::Ti if eu_address_pipelined => {}
            TCycle::Ti if self.pl_status == new_bus_status => {}
            TCycle::Ti => {
                self.biu_address_start(new_bus_status);
            }
            TCycle::T1 | TCycle::T2 => {
                self.bus_pending = BusPendingType::EuEarly;
                self.biu_address_start(new_bus_status);
            }
            _ => {
                if self.pl_status == BusStatus::CodeFetch {
                    self.bus_pending = BusPendingType::EuLate;
                    fetch_abort = true;
                } else if !self.final_transfer {
                    self.bus_pending = BusPendingType::EuEarly;
                }
            }
        }

        self.biu_bus_wait_finish(bus);
        if self.bus_pending == BusPendingType::EuEarly
            && self.t_cycle == TCycle::T4
            && self.bus_status_latch == BusStatus::CodeFetch
            && !cold_first_byte_read_handoff
        {
            self.biu_complete_code_fetch_for_eu();
        }
        let was_delay = self.biu_bus_wait_delay(bus);
        self.biu_bus_wait_address(bus);
        if cold_first_byte_read_handoff
            && self.t_cycle == TCycle::Ti
            && self.bus_status_latch == BusStatus::Passive
        {
            self.cycle(bus);
        }

        if was_delay || fetch_abort {
            self.biu_address_start(new_bus_status);
            self.biu_bus_wait_address(bus);
        }

        if self.t_cycle == TCycle::T4
            && self.bus_status_latch != BusStatus::CodeFetch
            && self.bus_pending != BusPendingType::EuEarly
        {
            self.cycle(bus);
        }

        if first {
            match op_size {
                OperandSize::Operand8 => {
                    self.transfer_n = 1;
                    self.final_transfer = true;
                }
                OperandSize::Operand16 => {
                    self.transfer_n = 1;
                    self.final_transfer = false;
                }
            }
        } else {
            self.transfer_n = 2;
            self.final_transfer = true;
        }

        self.bus_pending = BusPendingType::None;
        self.pl_status = BusStatus::Passive;
        self.bus_status = new_bus_status;
        self.bus_status_latch = new_bus_status;
        self.t_cycle = TCycle::Tinit;
        self.address_bus = address;
        self.address_latch = address;
        self.data_bus = data;
        self.transfer_size = TransferSize::Byte;
        self.operand_size = op_size;
    }

    pub(super) fn biu_read_u8_physical(&mut self, bus: &mut impl Bus, address: u32) -> u8 {
        let wrapped_address = address & ADDRESS_MASK;
        self.biu_bus_begin(
            bus,
            BusStatus::MemRead,
            wrapped_address,
            0,
            OperandSize::Operand8,
            true,
        );
        self.biu_bus_wait_finish(bus);
        self.data_bus as u8
    }

    pub(super) fn biu_write_u8_physical(&mut self, bus: &mut impl Bus, address: u32, byte: u8) {
        let wrapped_address = address & ADDRESS_MASK;
        self.biu_bus_begin(
            bus,
            BusStatus::MemWrite,
            wrapped_address,
            byte as u16,
            OperandSize::Operand8,
            true,
        );
        self.biu_bus_wait_until_tx(bus);
    }

    pub(super) fn biu_chain_eu_transfer(&mut self) {
        if matches!(self.t_cycle, TCycle::T3 | TCycle::T4)
            && self.bus_status_latch != BusStatus::CodeFetch
        {
            self.pl_status = BusStatus::Passive;
            self.ta_cycle = TaCycle::Td;
            self.final_transfer = false;
        }
    }

    fn biu_read_word_two_byte(
        &mut self,
        bus: &mut impl Bus,
        status: BusStatus,
        lo_address: u32,
        hi_address: u32,
    ) -> u16 {
        self.biu_bus_begin(bus, status, lo_address, 0, OperandSize::Operand16, true);
        self.biu_bus_wait_finish(bus);
        let lo = self.data_bus & 0x00FF;

        self.biu_bus_begin(bus, status, hi_address, 0, OperandSize::Operand16, false);
        self.biu_bus_wait_finish(bus);
        let hi = (self.data_bus & 0x00FF) << 8;
        lo | hi
    }

    fn biu_write_word_two_byte(
        &mut self,
        bus: &mut impl Bus,
        status: BusStatus,
        lo_address: u32,
        hi_address: u32,
        word: u16,
    ) {
        self.biu_bus_begin(
            bus,
            status,
            lo_address,
            word & 0x00FF,
            OperandSize::Operand16,
            true,
        );
        self.biu_bus_wait_until_tx(bus);

        self.biu_bus_begin(
            bus,
            status,
            hi_address,
            (word >> 8) & 0x00FF,
            OperandSize::Operand16,
            false,
        );
        self.biu_bus_wait_until_tx(bus);
    }

    pub(super) fn biu_read_u16(&mut self, bus: &mut impl Bus, seg: SegReg16, offset: u16) -> u16 {
        let lo_address = self.seg_base(seg).wrapping_add(offset as u32) & ADDRESS_MASK;
        let hi_address = self
            .seg_base(seg)
            .wrapping_add(offset.wrapping_add(1) as u32)
            & ADDRESS_MASK;
        self.biu_read_word_two_byte(bus, BusStatus::MemRead, lo_address, hi_address)
    }

    pub(super) fn biu_read_word_physical_pair(
        &mut self,
        bus: &mut impl Bus,
        lo_address: u32,
        hi_address: u32,
    ) -> u16 {
        self.biu_read_word_two_byte(
            bus,
            BusStatus::MemRead,
            lo_address & ADDRESS_MASK,
            hi_address & ADDRESS_MASK,
        )
    }

    pub(super) fn biu_write_word_physical_pair(
        &mut self,
        bus: &mut impl Bus,
        lo_address: u32,
        hi_address: u32,
        word: u16,
    ) {
        self.biu_write_word_two_byte(
            bus,
            BusStatus::MemWrite,
            lo_address & ADDRESS_MASK,
            hi_address & ADDRESS_MASK,
            word,
        );
    }

    pub(super) fn biu_read_u16_physical(&mut self, bus: &mut impl Bus, address: u32) -> u16 {
        let wrapped_address = address & ADDRESS_MASK;
        let next_wrapped_address = wrapped_address.wrapping_add(1) & ADDRESS_MASK;
        self.biu_read_word_two_byte(
            bus,
            BusStatus::MemRead,
            wrapped_address,
            next_wrapped_address,
        )
    }

    pub(super) fn biu_write_u16(
        &mut self,
        bus: &mut impl Bus,
        seg: SegReg16,
        offset: u16,
        word: u16,
    ) {
        let lo_address = self.seg_base(seg).wrapping_add(offset as u32) & ADDRESS_MASK;
        let hi_address = self
            .seg_base(seg)
            .wrapping_add(offset.wrapping_add(1) as u32)
            & ADDRESS_MASK;
        self.biu_write_word_two_byte(bus, BusStatus::MemWrite, lo_address, hi_address, word);
    }

    pub(super) fn biu_write_u16_physical(&mut self, bus: &mut impl Bus, address: u32, word: u16) {
        let wrapped_address = address & ADDRESS_MASK;
        let next_wrapped_address = wrapped_address.wrapping_add(1) & ADDRESS_MASK;
        self.biu_write_word_two_byte(
            bus,
            BusStatus::MemWrite,
            wrapped_address,
            next_wrapped_address,
            word,
        );
    }

    pub(super) fn biu_io_read_u8(&mut self, bus: &mut impl Bus, port: u16) -> u8 {
        self.biu_bus_begin(
            bus,
            BusStatus::IoRead,
            port as u32,
            0,
            OperandSize::Operand8,
            true,
        );
        self.biu_bus_wait_finish(bus);
        self.data_bus as u8
    }

    pub(super) fn biu_io_write_u8(&mut self, bus: &mut impl Bus, port: u16, byte: u8) {
        self.biu_bus_begin(
            bus,
            BusStatus::IoWrite,
            port as u32,
            byte as u16,
            OperandSize::Operand8,
            true,
        );
        self.biu_bus_wait_until_tx(bus);
    }

    pub(super) fn biu_io_read_u16(&mut self, bus: &mut impl Bus, port: u16) -> u16 {
        let lo_address = port as u32;
        let hi_address = port.wrapping_add(1) as u32;
        self.biu_read_word_two_byte(bus, BusStatus::IoRead, lo_address, hi_address)
    }

    pub(super) fn biu_io_write_u16(&mut self, bus: &mut impl Bus, port: u16, word: u16) {
        let lo_address = port as u32;
        let hi_address = port.wrapping_add(1) as u32;
        self.biu_write_word_two_byte(bus, BusStatus::IoWrite, lo_address, hi_address, word);
    }

    /// Read one instruction byte from the queue, cycling if empty.
    pub(super) fn biu_queue_read(&mut self, bus: &mut impl Bus, dtype: QueueType) -> u8 {
        if let Some(preload) = self.instruction_preload.take() {
            self.last_queue_op = QueueOp::First;
            self.last_queue_byte = preload;
            self.biu_fetch_on_queue_read();
            return preload;
        }

        let byte = if self.queue_len() > 0 {
            let b = self.queue_pop();
            self.biu_fetch_on_queue_read();
            b
        } else {
            self.ensure_fetch_in_flight();
            let mut guard = 0u32;
            while self.queue_len() == 0 && guard < BIU_LOOP_GUARD {
                self.cycle(bus);
                guard += 1;
            }
            if self.queue_len() > 0 {
                self.queue_pop()
            } else {
                0
            }
        };

        self.queue_op = match dtype {
            QueueType::First => QueueOp::First,
            QueueType::Subsequent => QueueOp::Subsequent,
        };
        self.last_queue_byte = byte;
        self.cycle(bus);
        byte
    }

    fn ensure_fetch_in_flight(&mut self) {
        if self.bus_status_latch == BusStatus::Passive
            && self.pl_status != BusStatus::CodeFetch
            && !matches!(self.fetch_state, FetchState::Delayed(_))
        {
            self.biu_fetch_start();
        }
    }

    pub(super) fn biu_fetch_on_queue_read(&mut self) {
        if self.bus_status == BusStatus::Passive && self.queue_has_room_for_fetch() {
            match self.fetch_state {
                FetchState::Suspended => {
                    self.ta_cycle = TaCycle::Td;
                }
                FetchState::PausedFull => {
                    if self.t_cycle == TCycle::Ti {
                        self.ta_cycle = TaCycle::Td;
                    } else {
                        return;
                    }
                }
                _ => {}
            }
            self.biu_fetch_start();
        }
    }

    pub(super) fn biu_fetch_next(&mut self, bus: &mut impl Bus) {
        self.ensure_fetch_in_flight();

        let mut guard = 0u32;
        while self.queue_len() == 0 && self.instruction_preload.is_none() && guard < BIU_LOOP_GUARD
        {
            self.cycle(bus);
            guard += 1;
        }

        if self.instruction_preload.is_none() && self.queue_len() > 0 {
            let byte = self.queue_pop();
            self.instruction_preload = Some(byte);
            self.queue_op = QueueOp::First;
            self.last_queue_byte = byte;
            self.biu_fetch_on_queue_read();
        }
    }
}
