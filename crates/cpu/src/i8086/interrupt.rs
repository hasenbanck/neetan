use super::I8086;
use crate::{PENDING_IRQ, PENDING_NMI, SegReg16};

impl I8086 {
    pub(super) fn raise_divide_error(&mut self, bus: &mut impl common::Bus) {
        if self.rep_active {
            self.ip = self.rep_restart_ip;
        }
        self.rep_active = false;

        let flags_val = self.flags.compress();
        let return_ip = self.ip;
        let return_cs = self.sregs[SegReg16::CS as usize];

        self.clk(bus, 4);

        let dest_ip = self.read_memory_word(bus, 0);
        self.clk(bus, 1);
        let dest_cs = self.read_memory_word(bus, 2);

        self.biu_fetch_suspend(bus);
        self.clk(bus, 3);

        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;

        self.clk(bus, 3);
        self.corr(bus);
        self.clk(bus, 1);

        self.push(bus, return_cs);
        self.sregs[SegReg16::CS as usize] = dest_cs;

        self.clk(bus, 3);
        self.ip = dest_ip;
        self.flush_and_fetch(bus);
        self.clk(bus, 3);
        self.push(bus, return_ip);
    }

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

    pub(super) fn raise_software_interrupt_with_entry_cycles(
        &mut self,
        vector: u8,
        bus: &mut impl common::Bus,
        entry_cycles: i32,
    ) {
        if self.rep_active {
            self.ip = self.rep_restart_ip;
        }
        self.rep_active = false;

        let flags_val = self.flags.compress();
        let return_ip = self.ip;
        let return_cs = self.sregs[SegReg16::CS as usize];

        self.clk(bus, entry_cycles);

        let addr = (vector as u32) * 4;
        let dest_ip = self.read_memory_word(bus, addr);
        self.clk(bus, 1);
        let dest_cs = self.read_memory_word(bus, addr + 2);

        self.biu_fetch_suspend(bus);
        self.clk(bus, 3);

        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;

        self.clk(bus, 6);

        self.push(bus, return_cs);
        self.sregs[SegReg16::CS as usize] = dest_cs;

        self.clk(bus, 4);
        self.ip = dest_ip;
        self.flush_and_fetch(bus);
        self.push(bus, return_ip);
    }

    pub(super) fn raise_software_interrupt(&mut self, vector: u8, bus: &mut impl common::Bus) {
        self.raise_software_interrupt_with_entry_cycles(vector, bus, 3);
    }

    pub(super) fn raise_interrupt(&mut self, vector: u8, bus: &mut impl common::Bus) {
        if self.rep_active {
            self.ip = self.rep_restart_ip;
        }
        self.rep_active = false;
        let flags_val = self.flags.compress();
        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;

        let addr = (vector as u32) * 4;
        let dest_ip = self.read_memory_word(bus, addr);
        let dest_cs = self.read_memory_word(bus, addr + 2);

        let cs = self.sregs[SegReg16::CS as usize];
        self.push(bus, cs);
        self.push(bus, self.ip);

        self.set_cs_ip_and_flush(bus, dest_cs, dest_ip);
    }
}
