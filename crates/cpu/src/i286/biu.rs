//! 80286 bus/front-end foundation.

use std::mem;

use common::Bus;

use super::{ADDRESS_MASK, I286};
use crate::SegReg16;

pub(crate) const MAX_QUEUE_SIZE: usize = 6;
const HLT_OPCODE: u8 = 0xF4;
const BIU_LOOP_GUARD: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BusStatus {
    Code,
    MemoryRead,
    MemoryWrite,
    IoRead,
    IoWrite,
    Halt,
    Passive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueType {
    First,
    Subsequent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BiuRequestKind {
    CodeFetch,
    MemoryRead,
    MemoryWrite,
    IoRead,
    IoWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BiuTransferSize {
    Byte,
    Word,
    QueueRoomByte,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BiuBusRequest {
    kind: BiuRequestKind,
    address: u32,
    size: BiuTransferSize,
    lane: I286BusLane,
    data_bus: u16,
    value: u16,
    data_ready_on_ts: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BiuCompletedBusCycle {
    value: u16,
}

/// Observable 80286 bus phase in the SingleStepTests trace vocabulary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286BusPhase {
    /// Passive or internal cycle.
    #[default]
    Ti,
    /// Bus-cycle start.
    Ts,
    /// Bus-cycle continuation.
    Tc,
}

/// Bus byte lane used by the current 80286 bus cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286BusLane {
    /// No lane is active.
    #[default]
    None,
    /// Low data byte is active.
    LowByte,
    /// High data byte is active.
    HighByte,
    /// Both byte lanes are active.
    Word,
}

/// High-level 80286 bus request class used in diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286PendingBusRequest {
    /// No external bus activity.
    #[default]
    None,
    /// Code fetch.
    CodeFetch,
    /// Memory read.
    MemoryRead,
    /// Memory write.
    MemoryWrite,
    /// I/O read.
    IoRead,
    /// I/O write.
    IoWrite,
    /// HALT marker.
    Halt,
}

/// Address-unit stage used for trace diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286AuStage {
    /// No address calculation is being tracked.
    #[default]
    Idle,
    /// Effective address is ready.
    AddressReady,
}

/// Execution-unit stage used for trace diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286EuStage {
    /// No instruction stage is being tracked.
    #[default]
    Idle,
    /// Instruction is being decoded.
    Decode,
    /// Instruction is executing.
    Execute,
    /// Instruction is halted.
    Halted,
}

/// Front-end flush state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286FlushState {
    /// No flush is pending.
    #[default]
    None,
    /// A control transfer made the next instruction start cold.
    ControlTransfer,
    /// The CPU is halted.
    Halted,
}

/// REP diagnostic state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286RepState {
    /// No REP string operation is active.
    #[default]
    None,
    /// REP string operation is active.
    Iterating,
    /// REP string operation was suspended.
    Suspended,
}

/// Diagnostic configuration for seeding the 286 front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I286WarmStartConfig {
    /// Number of instruction bytes assumed to be already prefetched.
    pub prefetch_bytes_before: u8,
    /// Number of decoded entries assumed by old analysis helpers.
    pub decoded_entries_before: u8,
    /// Flush state at instruction entry.
    pub pending_flush: I286FlushState,
}

/// Snapshot of the 286 timing/front-end state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct I286CycleState {
    /// Bytes currently resident in the 6-byte prefetch queue.
    pub prefetch_queue_fill: u8,
    /// Decoded-queue fill approximation retained for diagnostics.
    pub decoded_queue_fill: u8,
    /// Current bus phase.
    pub bus_phase: I286BusPhase,
    /// Current bus request.
    pub pending_bus_request: I286PendingBusRequest,
    /// Address-unit stage.
    pub au_stage: I286AuStage,
    /// Execution-unit stage.
    pub eu_stage: I286EuStage,
    /// Front-end flush state.
    pub flush_state: I286FlushState,
    /// REP state.
    pub rep_state: I286RepState,
    /// Whether LOCK is active.
    pub lock_active: bool,
}

/// Per-cycle trace entry emitted by the 80286 BIU foundation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I286CycleTraceEntry {
    /// Cycle number within the trace stream.
    pub cycle: u64,
    /// Latched state for this cycle.
    pub state: I286CycleState,
    /// Physical address driven on the bus when known.
    pub address: Option<u32>,
    /// Data bus value when known.
    pub data: Option<u16>,
    /// High-level bus status.
    pub bus_status: I286TraceBusStatus,
    /// Active byte lane for this cycle.
    pub lane: I286BusLane,
    /// Whether BHE# is asserted for this cycle.
    pub bhe_asserted: bool,
}

/// High-level 80286 bus status for trace comparison.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum I286TraceBusStatus {
    /// No external bus activity.
    #[default]
    Passive,
    /// Code fetch cycle.
    Code,
    /// Memory read cycle.
    MemoryRead,
    /// Memory write cycle.
    MemoryWrite,
    /// I/O read cycle.
    IoRead,
    /// I/O write cycle.
    IoWrite,
    /// HALT marker cycle.
    Halt,
}

impl BiuBusRequest {
    fn pending_request(self) -> I286PendingBusRequest {
        match self.kind {
            BiuRequestKind::CodeFetch => I286PendingBusRequest::CodeFetch,
            BiuRequestKind::MemoryRead => I286PendingBusRequest::MemoryRead,
            BiuRequestKind::MemoryWrite => I286PendingBusRequest::MemoryWrite,
            BiuRequestKind::IoRead => I286PendingBusRequest::IoRead,
            BiuRequestKind::IoWrite => I286PendingBusRequest::IoWrite,
        }
    }

    fn trace_status(self) -> I286TraceBusStatus {
        match self.kind {
            BiuRequestKind::CodeFetch => I286TraceBusStatus::Code,
            BiuRequestKind::MemoryRead => I286TraceBusStatus::MemoryRead,
            BiuRequestKind::MemoryWrite => I286TraceBusStatus::MemoryWrite,
            BiuRequestKind::IoRead => I286TraceBusStatus::IoRead,
            BiuRequestKind::IoWrite => I286TraceBusStatus::IoWrite,
        }
    }

    fn is_demand(self) -> bool {
        !matches!(self.kind, BiuRequestKind::CodeFetch)
    }
}

impl I286BusLane {
    pub(crate) fn bhe_asserted(self) -> bool {
        matches!(self, Self::HighByte | Self::Word)
    }
}

impl I286 {
    pub(crate) fn queue_len(&self) -> usize {
        self.instruction_queue_len
    }

    pub(crate) fn front_end_resident_len(&self) -> usize {
        self.instruction_queue_len
            + usize::from(self.instruction_preload.is_some())
            + self.prefetch_spill_queue.len()
    }

    pub(crate) fn biu_queue_byte_for_timing(&self, index: usize) -> Option<u8> {
        let mut queue_index = index;
        if let Some(preload) = self.instruction_preload {
            if queue_index == 0 {
                return Some(preload);
            }
            queue_index -= 1;
        }
        if queue_index < self.instruction_queue_len {
            Some(self.instruction_queue[queue_index])
        } else {
            self.prefetch_spill_queue
                .get(queue_index - self.instruction_queue_len)
                .copied()
        }
    }

    pub(crate) fn queue_has_room(&self) -> bool {
        self.instruction_queue_len < MAX_QUEUE_SIZE
    }

    fn queue_pop(&mut self) -> u8 {
        debug_assert!(self.instruction_queue_len > 0, "286 queue underrun");
        let value = self.instruction_queue[0];
        if self.instruction_queue_len > 1 {
            self.instruction_queue
                .copy_within(1..self.instruction_queue_len, 0);
        }
        self.instruction_queue_len -= 1;
        self.drain_prefetch_spill();
        self.refresh_decoded_queue();
        value
    }

    fn queue_push(&mut self, value: u8) {
        debug_assert!(
            self.instruction_queue_len < MAX_QUEUE_SIZE,
            "286 queue overrun"
        );
        self.instruction_queue[self.instruction_queue_len] = value;
        self.instruction_queue_len += 1;
        self.refresh_decoded_queue();
    }

    fn push_prefetch_byte(&mut self, value: u8) {
        if self.queue_has_room() {
            self.queue_push(value);
        } else {
            self.prefetch_spill_queue.push_back(value);
            self.refresh_decoded_queue();
        }
    }

    fn drain_prefetch_spill(&mut self) {
        while self.queue_has_room() {
            let Some(value) = self.prefetch_spill_queue.pop_front() else {
                break;
            };
            self.queue_push(value);
        }
    }

    fn refresh_decoded_queue(&mut self) {
        self.decoded_queue_len = (self.instruction_queue_len
            + usize::from(self.instruction_preload.is_some()))
        .min(3) as u8;
    }

    pub(crate) fn biu_reset_front_end(&mut self) {
        self.instruction_queue = [0; MAX_QUEUE_SIZE];
        self.instruction_queue_len = 0;
        self.instruction_preload = None;
        self.prefetch_spill_queue.clear();
        self.instruction_entry_queue_bytes = 0;
        self.instruction_entry_decoded_queue_bytes = 0;
        self.instruction_entry_flush_state = I286FlushState::ControlTransfer;
        self.decoded_queue_len = 0;
        self.prefetch_ip = self.ip;
        self.bus_status = BusStatus::Passive;
        self.data_bus = 0xFFFF;
        self.flush_state = I286FlushState::ControlTransfer;
        self.pending_bus_request = None;
        self.active_bus_request = None;
        self.completed_bus_cycle = None;
        self.delay_queue_room_fetch_once = false;
        self.decode_spill_fetch_gap_enabled = false;
        self.decode_spill_fetch_needs_gap = false;
        self.wrapped_queue_room_fetch_delayed = false;
        self.bus_phase = I286BusPhase::Ti;
        self.bus_lane = I286BusLane::None;
        self.bhe_asserted = false;
        self.au_stage = I286AuStage::Idle;
        self.eu_stage = I286EuStage::Idle;
    }

    pub(crate) fn biu_flush_for_control_transfer(&mut self) {
        self.instruction_queue_len = 0;
        self.instruction_preload = None;
        self.prefetch_spill_queue.clear();
        self.decoded_queue_len = 0;
        self.prefetch_ip = self.ip;
        self.flush_state = I286FlushState::ControlTransfer;
        self.delay_queue_room_fetch_once = false;
        self.decode_spill_fetch_gap_enabled = false;
        self.decode_spill_fetch_needs_gap = false;
        if let Some(request) = self.pending_bus_request
            && matches!(request.kind, BiuRequestKind::CodeFetch)
        {
            self.pending_bus_request = None;
        }
        if let Some(request) = self.active_bus_request
            && matches!(request.kind, BiuRequestKind::CodeFetch)
        {
            self.active_bus_request = None;
            self.bus_phase = I286BusPhase::Ti;
            self.bus_lane = I286BusLane::None;
            self.bhe_asserted = false;
        }
    }

    pub(crate) fn biu_latch_instruction_entry(&mut self) {
        self.decode_spill_fetch_gap_enabled = false;
        self.decode_spill_fetch_needs_gap = false;
        self.instruction_entry_queue_bytes = (self.instruction_queue_len
            + usize::from(self.instruction_preload.is_some())
            + self.prefetch_spill_queue.len()) as u8;
        self.instruction_entry_decoded_queue_bytes = self.decoded_queue_len;
        self.instruction_entry_flush_state = self.flush_state;
        self.eu_stage = I286EuStage::Decode;
    }

    pub(crate) fn biu_start_execute(&mut self) {
        self.decode_spill_fetch_gap_enabled = false;
        self.decode_spill_fetch_needs_gap = false;
        self.eu_stage = I286EuStage::Execute;
    }

    pub(crate) fn biu_finish_instruction(&mut self) {
        if !self.halted && !self.shutdown && !self.rep_active {
            self.eu_stage = I286EuStage::Idle;
        }
    }

    pub(crate) fn biu_mark_au_address_ready(&mut self) {
        self.au_stage = I286AuStage::AddressReady;
    }

    pub(crate) fn biu_clear_au_address_ready(&mut self) {
        if self.au_stage == I286AuStage::AddressReady {
            self.au_stage = I286AuStage::Idle;
        }
    }

    pub(crate) fn biu_delay_next_queue_room_fetch(&mut self) {
        self.delay_queue_room_fetch_once = true;
    }

    pub(crate) fn biu_set_decode_spill_fetch_gap(&mut self, enabled: bool) {
        self.decode_spill_fetch_gap_enabled = enabled;
    }

    #[allow(dead_code)]
    pub(crate) fn biu_instruction_entry_queue_len_for_timing(&self) -> usize {
        usize::from(self.instruction_entry_queue_bytes)
    }

    #[allow(dead_code)]
    pub(crate) fn biu_instruction_entry_decoded_queue_len_for_timing(&self) -> usize {
        usize::from(self.instruction_entry_decoded_queue_bytes)
    }

    #[allow(dead_code)]
    pub(crate) fn biu_instruction_entry_flush_state_for_timing(&self) -> I286FlushState {
        self.instruction_entry_flush_state
    }

    #[allow(dead_code)]
    pub(crate) fn instruction_entry_queue_had_current_instruction(&self) -> bool {
        self.instruction_entry_queue_bytes > 0
    }

    pub(crate) fn biu_internal_cycles(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.biu_tick_prefetching();
        }
    }

    pub(crate) fn biu_bus_cycles(&mut self, bus: &mut impl Bus, cycles: u32) {
        for _ in 0..cycles {
            self.biu_tick(bus, true);
        }
    }

    pub(crate) fn biu_no_prefetch_cycle(&mut self, bus: &mut impl Bus) {
        self.biu_tick(bus, false);
    }

    pub(crate) fn biu_queue_read(&mut self, bus: &mut impl Bus, queue_type: QueueType) -> u8 {
        if let Some(preload) = self.instruction_preload.take() {
            self.refresh_decoded_queue();
            if !matches!(queue_type, QueueType::First) || preload != HLT_OPCODE {
                self.tick_after_queue_read(bus);
            }
            return preload;
        }

        let mut guard = 0usize;
        while self.instruction_queue_len == 0 && guard < BIU_LOOP_GUARD {
            self.biu_tick(bus, true);
            guard += 1;
        }
        debug_assert!(self.instruction_queue_len > 0, "286 queue fill timeout");

        let value = self.queue_pop();
        match queue_type {
            QueueType::First => {
                if value != HLT_OPCODE {
                    self.wrapped_queue_room_fetch_delayed = false;
                }
                self.eu_stage = I286EuStage::Decode;
            }
            QueueType::Subsequent => {}
        }
        if !matches!(queue_type, QueueType::First) || value != HLT_OPCODE {
            self.tick_after_queue_read(bus);
        }
        value
    }

    pub(crate) fn biu_queue_read_subsequent_without_tick(&mut self, bus: &mut impl Bus) -> u8 {
        if let Some(preload) = self.instruction_preload.take() {
            self.refresh_decoded_queue();
            return preload;
        }

        if self.instruction_queue_len == 0 {
            self.drain_prefetch_spill();
        }

        let mut guard = 0usize;
        while self.instruction_queue_len == 0 && guard < BIU_LOOP_GUARD {
            self.biu_tick(bus, true);
            guard += 1;
            self.drain_prefetch_spill();
        }
        debug_assert!(self.instruction_queue_len > 0, "286 queue fill timeout");
        self.queue_pop()
    }

    pub(crate) fn biu_tick_after_subsequent_queue_read(&mut self, bus: &mut impl Bus) {
        self.tick_after_queue_read(bus);
    }

    pub(crate) fn biu_fetch_next(&mut self, bus: &mut impl Bus) {
        if self.instruction_preload.is_some() {
            return;
        }
        let mut guard = 0usize;
        while self.instruction_queue_len == 0 && guard < BIU_LOOP_GUARD {
            self.biu_tick(bus, true);
            guard += 1;
        }
        if self.instruction_queue_len > 0 {
            self.instruction_preload = Some(self.queue_pop());
            self.refresh_decoded_queue();
        }
    }

    pub(crate) fn biu_realign_to_current_ip(&mut self) {
        let resident_bytes = self.instruction_queue_len
            + usize::from(self.instruction_preload.is_some())
            + self.prefetch_spill_queue.len();
        let queue_start_ip = self.prefetch_ip.wrapping_sub(resident_bytes as u16);
        if self.ip != queue_start_ip {
            self.biu_flush_for_control_transfer();
        }
    }

    fn tick_after_queue_read(&mut self, bus: &mut impl Bus) {
        if self.queue_has_room() || self.active_bus_request.is_some() {
            self.biu_tick(bus, true);
        }
    }

    fn biu_tick_prefetching(&mut self) {
        self.emit_passive_cycle();
    }

    fn biu_tick(&mut self, bus: &mut impl Bus, allow_prefetch: bool) {
        if let Some(request) = self.active_bus_request.take() {
            self.complete_active_bus_cycle(bus, request);
            return;
        }

        if let Some(request) = self.pending_bus_request.take() {
            self.start_bus_cycle(request);
            return;
        }

        if allow_prefetch
            && self.delay_queue_room_fetch_once
            && (self.instruction_queue_len == MAX_QUEUE_SIZE - 1
                || (self.front_end_resident_len() >= MAX_QUEUE_SIZE
                    && !self.prefetch_spill_queue.is_empty()))
        {
            self.delay_queue_room_fetch_once = false;
            self.emit_passive_cycle();
            return;
        }

        if allow_prefetch {
            if self.decode_spill_fetch_gap_enabled
                && self.front_end_resident_len() >= MAX_QUEUE_SIZE
                && !self.prefetch_spill_queue.is_empty()
                && self.eu_stage == I286EuStage::Decode
            {
                if self.decode_spill_fetch_needs_gap {
                    self.decode_spill_fetch_needs_gap = false;
                    self.emit_passive_cycle();
                    return;
                }
                if let Some(request) = self.make_code_fetch_request() {
                    self.decode_spill_fetch_needs_gap = true;
                    self.start_bus_cycle(request);
                    return;
                }
            } else if self.can_start_code_fetch()
                && let Some(request) = self.make_code_fetch_request()
            {
                if !self.queue_has_room()
                    && !self.prefetch_spill_queue.is_empty()
                    && self.eu_stage == I286EuStage::Decode
                {
                    self.decode_spill_fetch_needs_gap = true;
                }
                self.start_bus_cycle(request);
                return;
            }
        }

        self.emit_passive_cycle();
    }

    fn emit_passive_cycle(&mut self) {
        self.bus_phase = I286BusPhase::Ti;
        self.bus_lane = I286BusLane::None;
        self.bhe_asserted = false;
        self.emit_cycle(
            I286BusPhase::Ti,
            self.pending_request_for_trace(),
            I286TraceBusStatus::Passive,
            None,
            None,
            I286BusLane::None,
        );
    }

    fn make_code_fetch_request(&self) -> Option<BiuBusRequest> {
        if !self.can_start_code_fetch() {
            return None;
        }

        let cs_base = self.seg_bases[SegReg16::CS as usize];
        let address = cs_base.wrapping_add(u32::from(self.prefetch_ip)) & ADDRESS_MASK;
        if address & 1 == 1 {
            Some(BiuBusRequest {
                kind: BiuRequestKind::CodeFetch,
                address,
                size: BiuTransferSize::Byte,
                lane: Self::byte_lane(address),
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            })
        } else if self.instruction_queue_len + 2 > MAX_QUEUE_SIZE {
            Some(BiuBusRequest {
                kind: BiuRequestKind::CodeFetch,
                address,
                size: BiuTransferSize::QueueRoomByte,
                lane: I286BusLane::Word,
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            })
        } else {
            Some(BiuBusRequest {
                kind: BiuRequestKind::CodeFetch,
                address: address & !1,
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            })
        }
    }

    fn can_start_code_fetch(&self) -> bool {
        self.queue_has_room()
            || (!self.prefetch_spill_queue.is_empty() && self.eu_stage == I286EuStage::Decode)
    }

    fn start_bus_cycle(&mut self, request: BiuBusRequest) {
        self.bus_phase = I286BusPhase::Ts;
        self.bus_lane = request.lane;
        self.bhe_asserted = request.lane.bhe_asserted();
        let status = request.trace_status();
        self.bus_status = Self::bus_status_from_trace(status);
        let request_kind = request.pending_request();
        let address = Some(request.address);
        let data = if matches!(
            request.kind,
            BiuRequestKind::MemoryWrite | BiuRequestKind::IoWrite
        ) && !request.data_ready_on_ts
        {
            Some(self.data_bus)
        } else {
            Some(request.data_bus)
        };

        self.active_bus_request = Some(request);
        self.emit_cycle(
            I286BusPhase::Ts,
            request_kind,
            status,
            address,
            data,
            request.lane,
        );
    }

    fn complete_active_bus_cycle(&mut self, bus: &mut impl Bus, request: BiuBusRequest) {
        match request.kind {
            BiuRequestKind::CodeFetch => self.complete_code_fetch(bus, request),
            BiuRequestKind::MemoryRead => {
                let value = self.read_bus_value(bus, request);
                self.data_bus = self.value_on_data_bus(request, value);
                self.completed_bus_cycle = Some(BiuCompletedBusCycle { value });
            }
            BiuRequestKind::MemoryWrite => {
                self.write_bus_value(bus, request);
                self.data_bus = request.data_bus;
                self.completed_bus_cycle = Some(BiuCompletedBusCycle {
                    value: request.value,
                });
            }
            BiuRequestKind::IoRead => {
                let value = self.read_io_value(bus, request);
                self.data_bus = self.value_on_data_bus(request, value);
                self.completed_bus_cycle = Some(BiuCompletedBusCycle { value });
            }
            BiuRequestKind::IoWrite => {
                self.write_io_value(bus, request);
                self.data_bus = request.data_bus;
                self.completed_bus_cycle = Some(BiuCompletedBusCycle {
                    value: request.value,
                });
            }
        }

        self.bus_phase = I286BusPhase::Tc;
        self.bus_lane = request.lane;
        self.bhe_asserted = request.lane.bhe_asserted();
        self.emit_cycle(
            I286BusPhase::Tc,
            self.pending_request_for_trace(),
            I286TraceBusStatus::Passive,
            None,
            Some(self.data_bus),
            request.lane,
        );
        self.bus_phase = I286BusPhase::Ti;
        self.bus_lane = I286BusLane::None;
        self.bhe_asserted = false;
    }

    fn complete_code_fetch(&mut self, bus: &mut impl Bus, request: BiuBusRequest) {
        match request.size {
            BiuTransferSize::Byte => {
                let value = bus.read_byte(request.address);
                self.data_bus = self.value_on_data_bus(request, u16::from(value));
                self.push_prefetch_byte(value);
                self.prefetch_ip = self.prefetch_ip.wrapping_add(1);
            }
            BiuTransferSize::Word => {
                let low = bus.read_byte(request.address) as u16;
                let high = bus.read_byte(request.address.wrapping_add(1) & ADDRESS_MASK) as u16;
                self.data_bus = low | (high << 8);
                self.push_prefetch_byte(low as u8);
                self.push_prefetch_byte(high as u8);
                self.prefetch_ip = self.prefetch_ip.wrapping_add(2);
            }
            BiuTransferSize::QueueRoomByte => {
                let low = bus.read_byte(request.address) as u16;
                let high = bus.read_byte(request.address.wrapping_add(1) & ADDRESS_MASK) as u16;
                self.data_bus = low | (high << 8);
                self.push_prefetch_byte(low as u8);
                self.push_prefetch_byte(high as u8);
                self.prefetch_ip = self.prefetch_ip.wrapping_add(2);
            }
        }
        self.flush_state = I286FlushState::None;
    }

    fn read_bus_value(&mut self, bus: &mut impl Bus, request: BiuBusRequest) -> u16 {
        match request.size {
            BiuTransferSize::Byte => u16::from(bus.read_byte(request.address)),
            BiuTransferSize::Word | BiuTransferSize::QueueRoomByte => {
                let low = bus.read_byte(request.address) as u16;
                let high = bus.read_byte(request.address.wrapping_add(1) & ADDRESS_MASK) as u16;
                low | (high << 8)
            }
        }
    }

    fn write_bus_value(&mut self, bus: &mut impl Bus, request: BiuBusRequest) {
        match request.size {
            BiuTransferSize::Byte => bus.write_byte(request.address, request.value as u8),
            BiuTransferSize::Word | BiuTransferSize::QueueRoomByte => {
                bus.write_byte(request.address, request.value as u8);
                bus.write_byte(
                    request.address.wrapping_add(1) & ADDRESS_MASK,
                    (request.value >> 8) as u8,
                );
            }
        }
    }

    fn read_io_value(&mut self, bus: &mut impl Bus, request: BiuBusRequest) -> u16 {
        match request.size {
            BiuTransferSize::Byte => u16::from(bus.io_read_byte(request.address as u16)),
            BiuTransferSize::Word | BiuTransferSize::QueueRoomByte => {
                bus.io_read_word(request.address as u16)
            }
        }
    }

    fn write_io_value(&mut self, bus: &mut impl Bus, request: BiuBusRequest) {
        match request.size {
            BiuTransferSize::Byte => bus.io_write_byte(request.address as u16, request.value as u8),
            BiuTransferSize::Word | BiuTransferSize::QueueRoomByte => {
                bus.io_write_word(request.address as u16, request.value)
            }
        }
    }

    fn submit_demand_request(
        &mut self,
        bus: &mut impl Bus,
        request: BiuBusRequest,
    ) -> BiuCompletedBusCycle {
        debug_assert!(request.is_demand());
        debug_assert!(self.pending_bus_request.is_none());

        let mut request = request;
        self.submit_demand_request_inner(bus, &mut request, true, false, false)
    }

    fn submit_demand_request_after_prefetch_gap(
        &mut self,
        bus: &mut impl Bus,
        request: BiuBusRequest,
    ) -> BiuCompletedBusCycle {
        debug_assert!(request.is_demand());
        debug_assert!(self.pending_bus_request.is_none());

        let mut request = request;
        self.submit_demand_request_inner(bus, &mut request, true, true, false)
    }

    fn submit_demand_request_after_prefetch_and_au_gap(
        &mut self,
        bus: &mut impl Bus,
        request: BiuBusRequest,
    ) -> BiuCompletedBusCycle {
        debug_assert!(request.is_demand());
        debug_assert!(self.pending_bus_request.is_none());

        let mut request = request;
        self.submit_demand_request_inner(bus, &mut request, true, true, true)
    }

    fn submit_demand_request_without_prefetch(
        &mut self,
        bus: &mut impl Bus,
        request: BiuBusRequest,
    ) -> BiuCompletedBusCycle {
        debug_assert!(request.is_demand());
        debug_assert!(self.pending_bus_request.is_none());

        let mut request = request;
        self.submit_demand_request_inner(bus, &mut request, false, false, false)
    }

    fn submit_demand_request_inner(
        &mut self,
        bus: &mut impl Bus,
        request: &mut BiuBusRequest,
        allow_prefetch: bool,
        emit_after_prefetch_gap: bool,
        emit_au_gap_after_prefetch_gap: bool,
    ) -> BiuCompletedBusCycle {
        let delayed_queue_room_fetch = if allow_prefetch {
            self.prefetch_before_demand(bus)
        } else {
            false
        };
        if self.lock_prefix_active && !self.lock_demand_gap_emitted {
            self.emit_passive_cycle();
            self.lock_demand_gap_emitted = true;
            if emit_au_gap_after_prefetch_gap && self.au_stage == I286AuStage::AddressReady {
                self.emit_passive_cycle();
            }
        } else if emit_after_prefetch_gap
            || (self.au_stage == I286AuStage::AddressReady && !delayed_queue_room_fetch)
        {
            self.emit_passive_cycle();
            if emit_after_prefetch_gap
                && emit_au_gap_after_prefetch_gap
                && self.au_stage == I286AuStage::AddressReady
            {
                self.emit_passive_cycle();
            }
        }
        if matches!(
            request.kind,
            BiuRequestKind::MemoryRead | BiuRequestKind::IoRead
        ) {
            request.data_bus = self.data_bus;
        }

        self.pending_bus_request = Some(*request);
        if matches!(
            request.kind,
            BiuRequestKind::MemoryRead | BiuRequestKind::MemoryWrite
        ) {
            self.au_stage = I286AuStage::Idle;
        }

        let mut guard = 0usize;
        loop {
            self.biu_tick(bus, true);
            if let Some(completed) = self.completed_bus_cycle.take() {
                return completed;
            }
            guard += 1;
            debug_assert!(guard < BIU_LOOP_GUARD, "286 BIU demand request timeout");
        }
    }

    fn prefetch_before_demand(&mut self, bus: &mut impl Bus) -> bool {
        let mut guard = 0usize;
        let mut delayed_queue_room_fetch = false;
        while (self.active_bus_request.is_some() || self.queue_has_room()) && guard < BIU_LOOP_GUARD
        {
            if !delayed_queue_room_fetch
                && self.active_bus_request.is_none()
                && self.instruction_queue_len == MAX_QUEUE_SIZE - 1
                && self.prefetch_ip == 0
            {
                self.emit_passive_cycle();
                self.emit_passive_cycle();
                delayed_queue_room_fetch = true;
                self.wrapped_queue_room_fetch_delayed = true;
                break;
            }
            if !delayed_queue_room_fetch
                && self.au_stage == I286AuStage::AddressReady
                && self.active_bus_request.is_none()
                && self.instruction_queue_len == MAX_QUEUE_SIZE - 1
            {
                self.emit_passive_cycle();
                delayed_queue_room_fetch = true;
                guard += 1;
                continue;
            }
            self.biu_tick(bus, true);
            guard += 1;
        }
        debug_assert!(guard < BIU_LOOP_GUARD, "286 BIU demand prefetch timeout");
        delayed_queue_room_fetch
    }

    fn pending_request_for_trace(&self) -> I286PendingBusRequest {
        if let Some(request) = self.pending_bus_request {
            request.pending_request()
        } else {
            I286PendingBusRequest::None
        }
    }

    fn bus_status_from_trace(status: I286TraceBusStatus) -> BusStatus {
        match status {
            I286TraceBusStatus::Code => BusStatus::Code,
            I286TraceBusStatus::MemoryRead => BusStatus::MemoryRead,
            I286TraceBusStatus::MemoryWrite => BusStatus::MemoryWrite,
            I286TraceBusStatus::IoRead => BusStatus::IoRead,
            I286TraceBusStatus::IoWrite => BusStatus::IoWrite,
            I286TraceBusStatus::Halt => BusStatus::Halt,
            I286TraceBusStatus::Passive => BusStatus::Passive,
        }
    }

    fn value_on_data_bus(&self, request: BiuBusRequest, value: u16) -> u16 {
        match request.size {
            BiuTransferSize::Byte => self.merge_byte_lane(request.address, value as u8),
            BiuTransferSize::Word | BiuTransferSize::QueueRoomByte => value,
        }
    }

    pub(crate) fn biu_read_u8_physical(&mut self, bus: &mut impl Bus, address: u32) -> u8 {
        let address = address & ADDRESS_MASK;
        let request = BiuBusRequest {
            kind: BiuRequestKind::MemoryRead,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus: self.data_bus,
            value: 0,
            data_ready_on_ts: false,
        };
        self.submit_demand_request(bus, request).value as u8
    }

    pub(crate) fn biu_read_u8_physical_without_prefetch(
        &mut self,
        bus: &mut impl Bus,
        address: u32,
    ) -> u8 {
        let address = address & ADDRESS_MASK;
        let request = BiuBusRequest {
            kind: BiuRequestKind::MemoryRead,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus: self.data_bus,
            value: 0,
            data_ready_on_ts: false,
        };
        self.submit_demand_request_without_prefetch(bus, request)
            .value as u8
    }

    pub(crate) fn biu_read_u8_physical_after_prefetch_gap(
        &mut self,
        bus: &mut impl Bus,
        address: u32,
    ) -> u8 {
        let address = address & ADDRESS_MASK;
        let request = BiuBusRequest {
            kind: BiuRequestKind::MemoryRead,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus: self.data_bus,
            value: 0,
            data_ready_on_ts: false,
        };
        self.submit_demand_request_after_prefetch_gap(bus, request)
            .value as u8
    }

    pub(crate) fn biu_read_u8_physical_after_prefetch_and_au_gap(
        &mut self,
        bus: &mut impl Bus,
        address: u32,
    ) -> u8 {
        let address = address & ADDRESS_MASK;
        let request = BiuBusRequest {
            kind: BiuRequestKind::MemoryRead,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus: self.data_bus,
            value: 0,
            data_ready_on_ts: false,
        };
        self.submit_demand_request_after_prefetch_and_au_gap(bus, request)
            .value as u8
    }

    pub(crate) fn biu_write_u8_physical(&mut self, bus: &mut impl Bus, address: u32, value: u8) {
        let address = address & ADDRESS_MASK;
        let data_bus = self.merge_byte_lane(address, value);
        let request = BiuBusRequest {
            kind: BiuRequestKind::MemoryWrite,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus,
            value: u16::from(value),
            data_ready_on_ts: false,
        };
        self.submit_demand_request(bus, request);
    }

    fn biu_write_u8_physical_without_prefetch(
        &mut self,
        bus: &mut impl Bus,
        address: u32,
        value: u8,
        data_ready_on_ts: bool,
    ) {
        let address = address & ADDRESS_MASK;
        let data_bus = self.merge_byte_lane(address, value);
        let request = BiuBusRequest {
            kind: BiuRequestKind::MemoryWrite,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus,
            value: u16::from(value),
            data_ready_on_ts,
        };
        self.submit_demand_request_without_prefetch(bus, request);
    }

    pub(crate) fn biu_read_u16_pair(
        &mut self,
        bus: &mut impl Bus,
        low_address: u32,
        high_address: u32,
    ) -> u16 {
        let low_address = low_address & ADDRESS_MASK;
        let high_address = high_address & ADDRESS_MASK;
        if low_address & 1 == 0 && low_address.wrapping_add(1) & ADDRESS_MASK == high_address {
            let request = BiuBusRequest {
                kind: BiuRequestKind::MemoryRead,
                address: low_address,
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            };
            self.submit_demand_request(bus, request).value
        } else {
            let low = u16::from(self.biu_read_u8_physical(bus, low_address));
            let high = u16::from(self.biu_read_u8_physical(bus, high_address));
            low | (high << 8)
        }
    }

    pub(crate) fn biu_read_u16_pair_after_prefetch_gap(
        &mut self,
        bus: &mut impl Bus,
        low_address: u32,
        high_address: u32,
    ) -> u16 {
        let low_address = low_address & ADDRESS_MASK;
        let high_address = high_address & ADDRESS_MASK;
        if low_address & 1 == 0 && low_address.wrapping_add(1) & ADDRESS_MASK == high_address {
            let request = BiuBusRequest {
                kind: BiuRequestKind::MemoryRead,
                address: low_address,
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            };
            self.submit_demand_request_after_prefetch_gap(bus, request)
                .value
        } else {
            let low = u16::from(self.biu_read_u8_physical_after_prefetch_gap(bus, low_address));
            let high = u16::from(self.biu_read_u8_physical(bus, high_address));
            low | (high << 8)
        }
    }

    pub(crate) fn biu_read_u16_pair_after_prefetch_and_au_gap(
        &mut self,
        bus: &mut impl Bus,
        low_address: u32,
        high_address: u32,
    ) -> u16 {
        let low_address = low_address & ADDRESS_MASK;
        let high_address = high_address & ADDRESS_MASK;
        if low_address & 1 == 0 && low_address.wrapping_add(1) & ADDRESS_MASK == high_address {
            let request = BiuBusRequest {
                kind: BiuRequestKind::MemoryRead,
                address: low_address,
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            };
            self.submit_demand_request_after_prefetch_and_au_gap(bus, request)
                .value
        } else {
            let low =
                u16::from(self.biu_read_u8_physical_after_prefetch_and_au_gap(bus, low_address));
            let high = u16::from(self.biu_read_u8_physical(bus, high_address));
            low | (high << 8)
        }
    }

    pub(crate) fn biu_write_u16_pair(
        &mut self,
        bus: &mut impl Bus,
        low_address: u32,
        high_address: u32,
        value: u16,
    ) {
        let low_address = low_address & ADDRESS_MASK;
        let high_address = high_address & ADDRESS_MASK;
        if low_address & 1 == 0 && low_address.wrapping_add(1) & ADDRESS_MASK == high_address {
            let request = BiuBusRequest {
                kind: BiuRequestKind::MemoryWrite,
                address: low_address,
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: value,
                value,
                data_ready_on_ts: false,
            };
            self.submit_demand_request(bus, request);
        } else {
            self.biu_write_u8_physical(bus, low_address, value as u8);
            self.biu_write_u8_physical_without_prefetch(
                bus,
                high_address,
                (value >> 8) as u8,
                true,
            );
        }
    }

    pub(crate) fn biu_io_read_u8(&mut self, bus: &mut impl Bus, port: u16) -> u8 {
        self.biu_io_read_u8_address(bus, u32::from(port))
    }

    fn biu_io_read_u8_address(&mut self, bus: &mut impl Bus, address: u32) -> u8 {
        let request = BiuBusRequest {
            kind: BiuRequestKind::IoRead,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus: self.data_bus,
            value: 0,
            data_ready_on_ts: false,
        };
        self.submit_demand_request(bus, request).value as u8
    }

    pub(crate) fn biu_io_write_u8(&mut self, bus: &mut impl Bus, port: u16, value: u8) {
        self.biu_io_write_u8_address(bus, u32::from(port), value);
    }

    fn biu_io_write_u8_address(&mut self, bus: &mut impl Bus, address: u32, value: u8) {
        self.biu_io_write_u8_address_with_request(bus, address, value, false, false);
    }

    pub(crate) fn biu_io_write_u8_after_prefetch_gap(
        &mut self,
        bus: &mut impl Bus,
        port: u16,
        value: u8,
    ) {
        self.biu_io_write_u8_address_with_request(bus, u32::from(port), value, true, false);
    }

    fn biu_io_write_u8_address_with_request(
        &mut self,
        bus: &mut impl Bus,
        address: u32,
        value: u8,
        after_prefetch_gap: bool,
        data_ready_on_ts: bool,
    ) {
        let data_bus = self.merge_byte_lane(address, value);
        let request = BiuBusRequest {
            kind: BiuRequestKind::IoWrite,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus,
            value: u16::from(value),
            data_ready_on_ts,
        };
        if after_prefetch_gap {
            self.submit_demand_request_after_prefetch_gap(bus, request);
        } else {
            self.submit_demand_request(bus, request);
        }
    }

    fn biu_io_write_u8_address_ready_on_ts(&mut self, bus: &mut impl Bus, address: u32, value: u8) {
        let data_bus = self.merge_byte_lane(address, value);
        let request = BiuBusRequest {
            kind: BiuRequestKind::IoWrite,
            address,
            size: BiuTransferSize::Byte,
            lane: Self::byte_lane(address),
            data_bus,
            value: u16::from(value),
            data_ready_on_ts: true,
        };
        self.submit_demand_request_without_prefetch(bus, request);
    }

    pub(crate) fn biu_io_read_u16(&mut self, bus: &mut impl Bus, port: u16) -> u16 {
        if port & 1 == 0 {
            let request = BiuBusRequest {
                kind: BiuRequestKind::IoRead,
                address: u32::from(port),
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: self.data_bus,
                value: 0,
                data_ready_on_ts: false,
            };
            self.submit_demand_request(bus, request).value
        } else {
            let low = u16::from(self.biu_io_read_u8(bus, port));
            let high = u16::from(self.biu_io_read_u8_address(bus, u32::from(port) + 1));
            low | (high << 8)
        }
    }

    pub(crate) fn biu_io_write_u16(&mut self, bus: &mut impl Bus, port: u16, value: u16) {
        self.biu_io_write_u16_with_request(bus, port, value, false);
    }

    pub(crate) fn biu_io_write_u16_after_prefetch_gap(
        &mut self,
        bus: &mut impl Bus,
        port: u16,
        value: u16,
    ) {
        self.biu_io_write_u16_with_request(bus, port, value, true);
    }

    fn biu_io_write_u16_with_request(
        &mut self,
        bus: &mut impl Bus,
        port: u16,
        value: u16,
        after_prefetch_gap: bool,
    ) {
        if port & 1 == 0 {
            let request = BiuBusRequest {
                kind: BiuRequestKind::IoWrite,
                address: u32::from(port),
                size: BiuTransferSize::Word,
                lane: I286BusLane::Word,
                data_bus: value,
                value,
                data_ready_on_ts: false,
            };
            if after_prefetch_gap {
                self.submit_demand_request_after_prefetch_gap(bus, request);
            } else {
                self.submit_demand_request(bus, request);
            }
        } else {
            self.biu_io_write_u8_address_with_request(
                bus,
                u32::from(port),
                value as u8,
                after_prefetch_gap,
                false,
            );
            self.biu_io_write_u8_address_ready_on_ts(bus, u32::from(port) + 1, (value >> 8) as u8);
        }
    }

    pub(crate) fn biu_halt_marker(&mut self) {
        self.eu_stage = I286EuStage::Halted;
        self.flush_state = I286FlushState::Halted;
        self.bus_phase = I286BusPhase::Ts;
        self.bus_lane = I286BusLane::None;
        self.bhe_asserted = false;
        self.bus_status = BusStatus::Halt;
        self.emit_cycle(
            I286BusPhase::Ts,
            I286PendingBusRequest::Halt,
            I286TraceBusStatus::Halt,
            None,
            Some(self.data_bus),
            I286BusLane::None,
        );
    }

    pub(crate) fn biu_halt_preamble(&mut self, bus: &mut impl Bus) {
        if self.active_bus_request.is_none()
            && self.prefetch_ip == 0
            && self.instruction_queue_len == MAX_QUEUE_SIZE - 2
            && self.wrapped_queue_room_fetch_delayed
        {
            self.wrapped_queue_room_fetch_delayed = false;
            return;
        }

        let mut guard = 0usize;
        while (self.active_bus_request.is_some()
            || self.front_end_resident_len() < MAX_QUEUE_SIZE - 1)
            && guard < BIU_LOOP_GUARD
        {
            self.biu_tick(bus, true);
            guard += 1;
        }
        debug_assert!(guard < BIU_LOOP_GUARD, "286 BIU halt preamble timeout");
        self.biu_tick(bus, false);
        self.biu_tick(bus, false);
    }

    fn byte_lane(address: u32) -> I286BusLane {
        if address & 1 == 0 {
            I286BusLane::LowByte
        } else {
            I286BusLane::HighByte
        }
    }

    fn merge_byte_lane(&self, address: u32, value: u8) -> u16 {
        if address & 1 == 0 {
            (self.data_bus & 0xFF00) | u16::from(value)
        } else {
            (self.data_bus & 0x00FF) | (u16::from(value) << 8)
        }
    }

    fn emit_cycle(
        &mut self,
        bus_phase: I286BusPhase,
        request: I286PendingBusRequest,
        status: I286TraceBusStatus,
        address: Option<u32>,
        data: Option<u16>,
        lane: I286BusLane,
    ) {
        self.cycles_remaining -= 1;
        self.cycle_counter = self.cycle_counter.wrapping_add(1);
        self.bus_status = Self::bus_status_from_trace(status);

        if self.capture_enabled {
            self.trace.push(I286CycleTraceEntry {
                cycle: self.cycle_counter,
                state: self.current_cycle_state(bus_phase, request),
                address,
                data,
                bus_status: status,
                lane,
                bhe_asserted: lane.bhe_asserted(),
            });
        }
    }

    fn current_cycle_state(
        &self,
        bus_phase: I286BusPhase,
        request: I286PendingBusRequest,
    ) -> I286CycleState {
        I286CycleState {
            prefetch_queue_fill: self.queue_len() as u8
                + u8::from(self.instruction_preload.is_some())
                + self.prefetch_spill_queue.len() as u8,
            decoded_queue_fill: self.decoded_queue_len,
            bus_phase,
            pending_bus_request: request,
            au_stage: self.au_stage,
            eu_stage: if self.halted || self.shutdown {
                I286EuStage::Halted
            } else {
                self.eu_stage
            },
            flush_state: self.flush_state,
            rep_state: if self.rep_active {
                I286RepState::Suspended
            } else {
                I286RepState::None
            },
            lock_active: self.lock_prefix_active,
        }
    }

    /// Enables or disables cycle trace capture.
    pub fn set_cycle_trace_capture(&mut self, capture_enabled: bool) {
        self.capture_enabled = capture_enabled;
        self.trace.clear();
        self.cycle_counter = 0;
    }

    /// Drains the collected cycle trace.
    pub fn drain_cycle_trace(&mut self) -> Vec<I286CycleTraceEntry> {
        mem::take(&mut self.trace)
    }

    /// Installs a prefetched front-end state for verification diagnostics.
    pub fn install_front_end_state(
        &mut self,
        _bus: &mut impl Bus,
        prefetch_bytes: &[u8],
        decoded_entries: u8,
        pending_flush: I286FlushState,
    ) {
        self.instruction_queue = [0; MAX_QUEUE_SIZE];
        let len = prefetch_bytes.len().min(MAX_QUEUE_SIZE);
        self.instruction_queue[..len].copy_from_slice(&prefetch_bytes[..len]);
        self.instruction_queue_len = len;
        self.instruction_preload = None;
        self.prefetch_spill_queue.clear();
        self.decoded_queue_len = decoded_entries.min(3);
        self.prefetch_ip = self.ip.wrapping_add(len as u16);
        self.flush_state = pending_flush;
        self.pending_bus_request = None;
        self.active_bus_request = None;
        self.completed_bus_cycle = None;
        self.delay_queue_room_fetch_once = false;
        self.decode_spill_fetch_gap_enabled = false;
        self.decode_spill_fetch_needs_gap = false;
        self.wrapped_queue_room_fetch_delayed = false;
    }

    /// Installs a warm-start front-end state for diagnostic analysis.
    pub fn install_warm_start(
        &mut self,
        bus: &mut impl Bus,
        config: I286WarmStartConfig,
        instruction_bytes: &[u8],
    ) {
        let len = usize::from(config.prefetch_bytes_before).min(instruction_bytes.len());
        self.install_front_end_state(
            bus,
            &instruction_bytes[..len],
            config.decoded_entries_before,
            config.pending_flush,
        );
    }
}
