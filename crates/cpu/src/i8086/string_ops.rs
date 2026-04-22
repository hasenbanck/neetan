use super::{I8086, StepFinishCycle, biu::ADDRESS_MASK};
use crate::{ByteReg, SegReg16, WordReg};

impl I8086 {
    fn direction_delta(&self) -> u16 {
        if self.flags.df { 0xFFFF } else { 1 }
    }

    fn direction_delta_word(&self) -> u16 {
        if self.flags.df { 0xFFFE } else { 2 }
    }

    pub(super) fn movsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & ADDRESS_MASK;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & ADDRESS_MASK;
        let val = self.read_memory_byte(bus, src_addr);
        self.clk(bus, 1);
        self.write_memory_byte(bus, dst_addr, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = if self.seg_prefix {
            self.prefix_seg
        } else {
            SegReg16::DS
        };
        let val = self.read_word_seg(bus, src_seg, si);
        self.clk(bus, 1);
        self.write_word_seg(bus, SegReg16::ES, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & ADDRESS_MASK;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & ADDRESS_MASK;
        self.clk(bus, 1);
        let src = self.read_memory_byte(bus, src_addr);
        self.clk(bus, 2);
        let dst = self.read_memory_byte(bus, dst_addr);
        self.clk(bus, 3);
        self.alu_sub_byte(src, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_seg = if self.seg_prefix {
            self.prefix_seg
        } else {
            SegReg16::DS
        };
        self.clk(bus, 1);
        let src = self.read_word_seg(bus, src_seg, si);
        self.clk(bus, 2);
        let dst = self.read_word_seg(bus, SegReg16::ES, di);
        self.clk(bus, 3);
        self.alu_sub_word(src, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & ADDRESS_MASK;
        self.write_memory_byte(bus, addr, self.regs.byte(ByteReg::AL));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        self.write_word_seg(bus, SegReg16::ES, di, self.regs.word(WordReg::AX));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn lodsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & ADDRESS_MASK;
        let val = self.read_memory_byte(bus, addr);
        self.regs.set_byte(ByteReg::AL, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let seg = if self.seg_prefix {
            self.prefix_seg
        } else {
            SegReg16::DS
        };
        let val = self.read_word_seg(bus, seg, si);
        self.regs.set_word(WordReg::AX, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn scasb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & ADDRESS_MASK;
        self.clk(bus, 1);
        let dst = self.read_memory_byte(bus, addr);
        self.clk(bus, 1);
        self.clk(bus, 3);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn scasw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        self.clk(bus, 1);
        let dst = self.read_word_seg(bus, SegReg16::ES, di);
        self.clk(bus, 1);
        self.clk(bus, 3);
        let aw = self.regs.word(WordReg::AX);
        self.alu_sub_word(aw, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.movsb_body(bus);
        self.clk(bus, 3);
        if self.seg_prefix
            && !self.instruction_entry_queue_full()
            && self.next_instruction_uses_prefetched_high_byte()
        {
            self.clk(bus, 1);
        }
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.movsw_body(bus);
        self.clk(bus, 3);
        if self.seg_prefix
            && !self.instruction_entry_queue_full()
            && self.next_instruction_uses_prefetched_high_byte()
        {
            self.clk(bus, 1);
        }
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.cmpsb_body(bus);
        if !self.seg_prefix
            && self.instruction_entry_queue_full()
            && self.next_instruction_uses_prefetched_high_byte()
        {
            self.clk(bus, 1);
        }
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.cmpsw_body(bus);
        if !self.seg_prefix
            && self.instruction_entry_queue_full()
            && self.next_instruction_uses_prefetched_high_byte()
        {
            self.clk(bus, 1);
        }
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.stosb_body(bus);
        self.clk(bus, 4);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.step_finish_cycle = StepFinishCycle::TerminalWritebackInlineCommit;
        }
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.stosw_body(bus);
        self.clk(bus, 4);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.step_finish_cycle = StepFinishCycle::TerminalWritebackInlineCommit;
        }
    }

    pub(super) fn lodsb(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.lodsb_body(bus);
        self.clk(bus, 4);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.lodsw_body(bus);
        self.clk(bus, 4);
        if !self.seg_prefix && self.instruction_entry_queue_full() {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.scasb_body(bus);
        self.clk(bus, 1);
        if self.seg_prefix && self.next_instruction_uses_prefetched_high_byte() {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        self.clk(bus, 1);
        self.scasw_body(bus);
        self.clk(bus, 1);
        if self.seg_prefix && self.next_instruction_uses_prefetched_high_byte() {
            self.step_finish_cycle = StepFinishCycle::PreloadedOnly;
        }
    }
}
