use super::VX0;
use crate::{PENDING_IRQ, PENDING_NMI, SegReg16};

impl<const MODEL: u8> VX0<MODEL> {
    pub(super) fn check_interrupts(&mut self, bus: &mut impl common::Bus) {
        if self.pending_irq & PENDING_NMI != 0 && self.inhibit_all == 0 {
            self.pending_irq &= !PENDING_NMI;
            bus.acknowledge_nmi();
            self.raise_interrupt(2, bus);
        } else if self.flags.if_flag
            && self.pending_irq & PENDING_IRQ != 0
            && self.no_interrupt == 0
            && self.inhibit_all == 0
        {
            self.pending_irq &= !PENDING_IRQ;
            let vector = bus.acknowledge_irq();
            self.raise_interrupt(vector, bus);
        }
    }

    pub(super) fn raise_software_interrupt(
        &mut self,
        vector: u8,
        bus: &mut impl common::Bus,
        entry_cycles: i32,
        cold_entry_cycles: i32,
        cold_prefetch_count: u8,
    ) {
        self.raise_interrupt_with_entry_timing(
            vector,
            bus,
            entry_cycles,
            true,
            cold_entry_cycles,
            cold_prefetch_count,
        );
    }

    pub(super) fn raise_interrupt(&mut self, vector: u8, bus: &mut impl common::Bus) {
        self.raise_interrupt_with_entry_timing(vector, bus, 0, false, 0, 0);
    }

    pub(super) fn raise_divide_error(&mut self, bus: &mut impl common::Bus) {
        self.raise_divide_error_with_entry_cycles(bus, 1);
    }

    pub(super) fn raise_divide_error_with_entry_cycles(
        &mut self,
        bus: &mut impl common::Bus,
        entry_cycles: i32,
    ) {
        self.raise_divide_error_with_entry_timing(bus, entry_cycles, false);
    }

    pub(super) fn raise_divide_error_with_ready_vector_read(
        &mut self,
        bus: &mut impl common::Bus,
        entry_cycles: i32,
    ) {
        self.raise_divide_error_with_entry_timing(bus, entry_cycles, true);
    }

    pub(super) fn raise_interrupt_with_prepared_stack_writes(
        &mut self,
        vector: u8,
        bus: &mut impl common::Bus,
    ) {
        self.raise_interrupt_with_prepared_stack_writes_timing(vector, bus, false);
    }

    pub(super) fn raise_interrupt_with_ready_vector_read_and_prepared_stack_writes(
        &mut self,
        vector: u8,
        bus: &mut impl common::Bus,
    ) {
        self.raise_interrupt_with_prepared_stack_writes_timing(vector, bus, true);
    }

    fn raise_interrupt_with_prepared_stack_writes_timing(
        &mut self,
        vector: u8,
        bus: &mut impl common::Bus,
        vector_read_ready: bool,
    ) {
        if self.rep_state.active {
            self.ip = self.rep_state.restart_ip;
        }
        self.rep_state.active = false;
        let flags_val = self.flags.compress();
        let return_cs = self.sregs[SegReg16::CS as usize];
        let return_ip = self.ip;

        self.biu_fetch_suspend(bus);
        if vector_read_ready {
            self.biu_ready_memory_read();
        }
        let addr = (vector as u32) * 4;
        let dest_ip = self.read_memory_word(bus, addr);
        self.biu_chain_eu_transfer();
        let dest_cs = self.read_memory_word(bus, addr + 2);

        self.clk(bus, 1);
        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;
        self.flags.mf = true;

        self.clk(bus, 1);
        self.biu_prepare_memory_write();
        self.clk(bus, 2);
        self.push(bus, return_cs);

        self.clk(bus, 1);
        self.biu_prepare_memory_write_from_ts();
        self.push(bus, return_ip);
        self.ip = dest_ip;
        self.sregs[SegReg16::CS as usize] = dest_cs;
        self.flush_and_fetch_early();
    }

    fn raise_divide_error_with_entry_timing(
        &mut self,
        bus: &mut impl common::Bus,
        entry_cycles: i32,
        ready_vector_read: bool,
    ) {
        if self.rep_state.active {
            self.ip = self.rep_state.restart_ip;
        }
        self.rep_state.active = false;
        let flags_val = self.flags.compress();
        let return_cs = self.sregs[SegReg16::CS as usize];
        let return_ip = self.ip;

        self.clk(bus, entry_cycles);
        if ready_vector_read {
            self.biu_fetch_suspend_with_ready_memory_read(bus);
        } else {
            self.biu_fetch_suspend(bus);
        }
        let dest_ip = self.read_memory_word(bus, 0);
        self.biu_chain_eu_transfer();
        let dest_cs = self.read_memory_word(bus, 2);

        self.clk(bus, 1);
        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;
        self.flags.mf = true;

        self.clk(bus, 1);
        self.biu_prepare_memory_write();
        self.clk(bus, 2);
        self.push(bus, return_cs);

        self.clk(bus, 1);
        self.biu_prepare_memory_write_from_ts();
        self.push(bus, return_ip);
        self.ip = dest_ip;
        self.sregs[SegReg16::CS as usize] = dest_cs;
        self.flush_and_fetch_early();
    }

    fn raise_interrupt_with_entry_timing(
        &mut self,
        vector: u8,
        bus: &mut impl common::Bus,
        entry_cycles: i32,
        suspend_before_entry_cycles: bool,
        cold_entry_cycles: i32,
        cold_prefetch_count: u8,
    ) {
        if self.rep_state.active {
            self.ip = self.rep_state.restart_ip;
        }
        self.rep_state.active = false;
        let flags_val = self.flags.compress();
        let return_cs = self.sregs[SegReg16::CS as usize];
        let return_ip = self.ip;

        if suspend_before_entry_cycles {
            let cold_entry = self.biu_instruction_entry_queue_len_for_timing() == 0;
            self.biu_fetch_suspend_after_pending_fetch(bus);
            if cold_entry && cold_prefetch_count > 0 {
                for _ in 0..cold_prefetch_count {
                    self.biu_complete_code_fetch_for_eu();
                    self.biu_start_code_fetch_for_eu();
                    self.biu_fetch_suspend(bus);
                }
            } else {
                let cycles = if cold_entry {
                    cold_entry_cycles
                } else {
                    entry_cycles
                };
                self.clk(bus, cycles);
            }
        } else {
            self.clk(bus, entry_cycles);
            self.biu_fetch_suspend(bus);
        }
        let addr = (vector as u32) * 4;
        if suspend_before_entry_cycles {
            self.biu_ready_memory_read();
        }
        let dest_ip = self.read_memory_word(bus, addr);
        self.biu_chain_eu_transfer();
        let dest_cs = self.read_memory_word(bus, addr + 2);

        self.clk(bus, 1);
        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;
        self.flags.mf = true;

        if suspend_before_entry_cycles {
            self.clk(bus, 1);
            self.biu_prepare_memory_write();
            self.clk(bus, 2);
        } else {
            self.clk(bus, 3);
        }
        self.push(bus, return_cs);

        if suspend_before_entry_cycles {
            self.clk(bus, 1);
            self.biu_prepare_memory_write_from_ts();
        } else {
            self.clk(bus, 1);
        }
        self.push(bus, return_ip);
        self.ip = dest_ip;
        self.sregs[SegReg16::CS as usize] = dest_cs;
        self.flush_and_fetch_early();
    }
}
