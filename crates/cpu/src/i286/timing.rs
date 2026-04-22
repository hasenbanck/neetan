#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TimingBreakdown {
    pub eu_cycles: i32,
    pub au_cycles: i32,
    pub bu_data_pressure: i32,
    pub bu_fetch_stall: i32,
    pub iu_stall: i32,
    pub flush_penalty: i32,
    pub instruction_bytes: i32,
    pub prefetch_bytes_before: i32,
    pub prefetch_bytes_after: i32,
    pub decoded_entries_before: i32,
    pub decoded_entries_after: i32,
    pub refill_bytes: i32,
    startup_fetch_credit: i32,
    flush_at_end: FlushKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FlushKind {
    #[default]
    None,
    ControlTransfer,
    FaultTransfer,
    SetupJump,
}

impl FlushKind {
    const fn penalty(self) -> i32 {
        match self {
            Self::None => 0,
            Self::ControlTransfer => 0,
            Self::FaultTransfer => 1,
            Self::SetupJump => 5,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TimingModel {
    prefetch_bytes: i32,
    decoded_entries: i32,
    pending_flush: FlushKind,
    instruction_open: bool,
    pub current: TimingBreakdown,
    pub last: TimingBreakdown,
}

impl Default for TimingModel {
    fn default() -> Self {
        let mut model = Self {
            prefetch_bytes: 0,
            decoded_entries: 0,
            pending_flush: FlushKind::SetupJump,
            instruction_open: false,
            current: TimingBreakdown::default(),
            last: TimingBreakdown::default(),
        };
        model.reset(FlushKind::SetupJump);
        model
    }
}

impl TimingModel {
    const PREFETCH_QUEUE_CAPACITY: i32 = 8;
    const DECODE_QUEUE_CAPACITY: i32 = 3;
    const FETCH_STALL_PER_BYTE: i32 = 1;
    const DECODE_START_STALL: i32 = 1;
    const PREFETCH_REFILL_CYCLES: i32 = 2;
    const DECODE_REFILL_CYCLES: i32 = 3;

    pub fn reset(&mut self, pending_flush: FlushKind) {
        self.prefetch_bytes = 0;
        self.decoded_entries = 0;
        self.pending_flush = pending_flush;
        self.instruction_open = false;
        self.current = TimingBreakdown::default();
        self.last = TimingBreakdown::default();
    }

    pub fn begin_instruction(&mut self) -> i32 {
        if self.instruction_open {
            return 0;
        }

        self.instruction_open = true;
        self.current = TimingBreakdown {
            prefetch_bytes_before: self.prefetch_bytes,
            decoded_entries_before: self.decoded_entries,
            flush_penalty: self.pending_flush.penalty(),
            ..TimingBreakdown::default()
        };

        if self.prefetch_bytes == 0 {
            self.current.bu_fetch_stall += Self::FETCH_STALL_PER_BYTE;
            self.current.startup_fetch_credit = 1;
        }

        if self.decoded_entries == 0
            && (self.pending_flush == FlushKind::SetupJump || self.prefetch_bytes > 0)
        {
            self.current.iu_stall += Self::DECODE_START_STALL;
        }

        self.pending_flush = FlushKind::None;

        self.current.flush_penalty + self.current.bu_fetch_stall + self.current.iu_stall
    }

    pub fn finish_instruction(&mut self) -> i32 {
        if !self.instruction_open {
            return 0;
        }

        let extra_fetch_bytes = (self.current.instruction_bytes
            - self.current.prefetch_bytes_before
            - self.current.startup_fetch_credit)
            .max(0);
        self.current.bu_fetch_stall += extra_fetch_bytes * Self::FETCH_STALL_PER_BYTE;

        let consumed_decoded_entries = if self.current.decoded_entries_before > 0 {
            self.current.decoded_entries_before - 1
        } else {
            0
        };

        let queue_after_consumption = (self.current.prefetch_bytes_before
            + self.current.startup_fetch_credit
            + extra_fetch_bytes
            - self.current.instruction_bytes)
            .max(0);
        let refill_budget = (self.current.eu_cycles + self.current.au_cycles + 1
            - self.current.bu_data_pressure)
            .max(0);
        let refill_bytes = refill_budget / Self::PREFETCH_REFILL_CYCLES;
        let queue_after_linear =
            (queue_after_consumption + refill_bytes).min(Self::PREFETCH_QUEUE_CAPACITY);

        let decode_budget = (self.current.eu_cycles + self.current.au_cycles + 1).max(0);
        let decoded_entries_added = (decode_budget / Self::DECODE_REFILL_CYCLES)
            .min(queue_after_linear)
            .min(Self::DECODE_QUEUE_CAPACITY);
        let decoded_entries_after_linear =
            (consumed_decoded_entries + decoded_entries_added).min(Self::DECODE_QUEUE_CAPACITY);

        self.current.refill_bytes = refill_bytes;

        if self.current.flush_at_end != FlushKind::None {
            self.prefetch_bytes = 0;
            self.decoded_entries = 0;
            self.pending_flush = self.current.flush_at_end;
        } else {
            self.prefetch_bytes = queue_after_linear;
            self.decoded_entries = decoded_entries_after_linear;
            self.pending_flush = FlushKind::None;
        }

        self.current.prefetch_bytes_after = self.prefetch_bytes;
        self.current.decoded_entries_after = self.decoded_entries;

        let additional_cycles = extra_fetch_bytes * Self::FETCH_STALL_PER_BYTE;
        self.last = self.current;
        self.current = TimingBreakdown::default();
        self.instruction_open = false;
        additional_cycles
    }

    pub fn note_instruction_byte(&mut self) {
        if self.instruction_open {
            self.current.instruction_bytes += 1;
        }
    }

    pub fn note_bus_pressure(&mut self, pressure: i32) {
        if self.instruction_open {
            self.current.bu_data_pressure += pressure;
        }
    }

    pub fn mark_flush(&mut self, flush_kind: FlushKind) {
        if self.instruction_open {
            self.current.flush_at_end = self.current.flush_at_end.max(flush_kind);
        } else {
            self.pending_flush = self.pending_flush.max(flush_kind);
        }
    }
}
