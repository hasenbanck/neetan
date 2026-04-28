use super::{FinishState, VX0, biu::queue_size_for};
use crate::{ByteReg, SegReg16, WordReg};

impl<const MODEL: u8> VX0<MODEL> {
    fn ready_string_memory_bus(&mut self, write: bool) {
        if write {
            self.biu_ready_memory_write();
        } else {
            self.biu_ready_memory_read();
        }
    }

    fn prepare_standalone_string_first_bus(&mut self, bus: &mut impl common::Bus, write: bool) {
        let entry_queue_len = self.biu_instruction_entry_queue_len_for_timing();
        let has_prefix = self.opcode_start_ip != self.prev_ip;

        if entry_queue_len == 0 {
            if self.biu_latch_is_code_fetch() {
                self.biu_bus_wait_finish(bus);
                self.biu_complete_code_fetch_for_eu();
            }
            if self.queue_has_room_for_fetch() {
                self.biu_start_code_fetch_for_eu();
                self.biu_bus_wait_finish(bus);
                self.biu_complete_code_fetch_for_eu();
            }
            self.ready_string_memory_bus(write);
        } else if has_prefix && entry_queue_len == queue_size_for(MODEL) {
            if self.biu_latch_is_code_fetch() {
                self.biu_bus_wait_finish(bus);
                self.biu_complete_code_fetch_for_eu();
            }
            self.clk(bus, 2);
            self.ready_string_memory_bus(write);
        }
    }

    fn clk_standalone_ins_prefix_gap(&mut self, bus: &mut impl common::Bus) {
        let has_prefix = self.opcode_start_ip != self.prev_ip;
        if has_prefix && self.queue_len() == 0 {
            if self.biu_latch_is_code_fetch() {
                self.biu_bus_wait_finish(bus);
                self.biu_complete_code_fetch_for_eu();
            }
            self.clk(bus, 2);
            self.biu_ready_io_read();
        }
    }

    fn clk_standalone_outs_prefix_gap(&mut self, bus: &mut impl common::Bus) {
        let has_prefix = self.opcode_start_ip != self.prev_ip;
        if has_prefix && self.queue_len() == 0 {
            if self.biu_latch_is_code_fetch() {
                self.biu_bus_wait_finish(bus);
                self.biu_complete_code_fetch_for_eu();
            }
            self.clk(bus, 2);
            self.biu_ready_memory_read();
        }
    }

    fn direction_delta(&self) -> u16 {
        if self.flags.df { 0xFFFF } else { 1 }
    }

    fn direction_delta_word(&self) -> u16 {
        if self.flags.df { 0xFFFE } else { 2 }
    }

    pub(super) fn movsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, src_addr);
        self.biu_chain_eu_transfer();
        self.biu_write_u8_physical(bus, dst_addr, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsb(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, false);
        self.movsb_body(bus);
    }

    pub(super) fn movsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_base = self.default_base(SegReg16::DS);
        let dst_base = self.seg_base(SegReg16::ES);
        let val = self.read_word_seg(bus, src_base, si);
        self.biu_chain_eu_transfer();
        self.write_word_seg(bus, dst_base, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn movsw(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, false);
        self.movsw_body(bus);
    }

    pub(super) fn cmpsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let src = self.biu_read_u8_physical(bus, src_addr);
        self.biu_chain_eu_transfer();
        let dst = self.biu_read_u8_physical(bus, dst_addr);
        self.alu_sub_byte(src, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsb(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, false);
        self.cmpsb_body(bus);
    }

    pub(super) fn rep_cmpsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let dst_addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let dst = self.biu_read_u8_physical(bus, dst_addr);
        self.biu_chain_eu_transfer();
        let src = self.biu_read_u8_physical(bus, src_addr);
        self.alu_sub_byte(src, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_base = self.default_base(SegReg16::DS);
        let dst_base = self.seg_base(SegReg16::ES);
        let src = self.read_word_seg(bus, src_base, si);
        self.biu_chain_eu_transfer();
        let dst = self.read_word_seg(bus, dst_base, di);
        self.alu_sub_word(src, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn cmpsw(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, false);
        self.cmpsw_body(bus);
    }

    pub(super) fn rep_cmpsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let di = self.regs.word(WordReg::DI);
        let src_base = self.default_base(SegReg16::DS);
        let dst_base = self.seg_base(SegReg16::ES);
        let dst = self.read_word_seg(bus, dst_base, di);
        self.biu_chain_eu_transfer();
        let src = self.read_word_seg(bus, src_base, si);
        self.alu_sub_word(src, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        self.biu_write_u8_physical(bus, addr, self.regs.byte(ByteReg::AL));
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosb(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, true);
        self.stosb_body(bus);
        self.biu_bus_wait_finish(bus);
        self.finish_state = FinishState::NoTerminalFetch;
    }

    pub(super) fn stosw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        self.write_word_seg(bus, base, di, self.regs.word(WordReg::AX));
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn stosw(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, true);
        self.stosw_body(bus);
        self.biu_bus_wait_finish(bus);
        self.finish_state = FinishState::NoTerminalFetch;
    }

    pub(super) fn lodsb_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, addr);
        self.regs.set_byte(ByteReg::AL, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsw_body(&mut self, bus: &mut impl common::Bus) {
        let si = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let val = self.read_word_seg(bus, base, si);
        self.regs.set_word(WordReg::AX, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn lodsw(&mut self, bus: &mut impl common::Bus) {
        self.lodsw_body(bus);
        self.clk(bus, 2);
    }

    pub(super) fn scasb_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let dst = self.biu_read_u8_physical(bus, addr);
        let al = self.regs.byte(ByteReg::AL);
        self.alu_sub_byte(al, dst);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn scasb(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, false);
        self.scasb_body(bus);
        self.clk(bus, 1);
    }

    pub(super) fn scasw_body(&mut self, bus: &mut impl common::Bus) {
        let di = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let dst = self.read_word_seg(bus, base, di);
        let aw = self.regs.word(WordReg::AX);
        self.alu_sub_word(aw, dst);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn scasw(&mut self, bus: &mut impl common::Bus) {
        self.prepare_standalone_string_first_bus(bus, false);
        self.scasw_body(bus);
        self.clk(bus, 1);
    }

    pub(super) fn insb_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let addr = self.seg_base(SegReg16::ES).wrapping_add(di as u32) & 0xFFFFF;
        let val = self.biu_io_read_u8(bus, port);
        self.biu_chain_eu_transfer();
        self.biu_write_u8_physical(bus, addr, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insb(&mut self, bus: &mut impl common::Bus) {
        self.clk_standalone_ins_prefix_gap(bus);
        self.insb_body(bus);
        self.clk(bus, 1);
        self.finish_state = FinishState::NoTerminalFetch;
    }

    pub(super) fn insw_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let di = self.regs.word(WordReg::DI);
        let base = self.seg_base(SegReg16::ES);
        let val = self.biu_io_read_u16(bus, port);
        self.biu_chain_eu_transfer();
        self.write_word_seg(bus, base, di, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::DI, di.wrapping_add(delta));
    }

    pub(super) fn insw(&mut self, bus: &mut impl common::Bus) {
        self.clk_standalone_ins_prefix_gap(bus);
        self.insw_body(bus);
        self.clk(bus, 1);
        self.finish_state = FinishState::NoTerminalFetch;
    }

    pub(super) fn outsb_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let addr = self.default_base(SegReg16::DS).wrapping_add(si as u32) & 0xFFFFF;
        let val = self.biu_read_u8_physical(bus, addr);
        self.biu_chain_eu_transfer();
        self.biu_io_write_u8(bus, port, val);
        let delta = self.direction_delta();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn outsb(&mut self, bus: &mut impl common::Bus) {
        self.clk_standalone_outs_prefix_gap(bus);
        self.outsb_body(bus);
        self.clk(bus, 1);
        self.finish_state = FinishState::NoTerminalFetch;
    }

    pub(super) fn outsw_body(&mut self, bus: &mut impl common::Bus) {
        let port = self.regs.word(WordReg::DX);
        let si = self.regs.word(WordReg::SI);
        let base = self.default_base(SegReg16::DS);
        let val = self.read_word_seg(bus, base, si);
        self.biu_chain_eu_transfer();
        self.biu_io_write_u16(bus, port, val);
        let delta = self.direction_delta_word();
        self.regs.set_word(WordReg::SI, si.wrapping_add(delta));
    }

    pub(super) fn outsw(&mut self, bus: &mut impl common::Bus) {
        self.clk_standalone_outs_prefix_gap(bus);
        self.outsw_body(bus);
        self.clk(bus, 1);
        self.finish_state = FinishState::NoTerminalFetch;
    }
}
