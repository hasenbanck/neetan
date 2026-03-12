use super::V30;
use crate::{PENDING_IRQ, PENDING_NMI, SegReg16};

impl V30 {
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

    pub(super) fn raise_interrupt(&mut self, vector: u8, bus: &mut impl common::Bus) {
        if self.rep_active {
            self.ip = self.rep_restart_ip;
        }
        self.rep_active = false;
        let flags_val = self.flags.compress();
        self.push(bus, flags_val);
        self.flags.tf = false;
        self.flags.if_flag = false;
        self.flags.mf = true;

        let addr = (vector as u32) * 4;
        let dest_ip = bus.read_word(addr);
        let dest_cs = bus.read_word(addr + 2);

        let cs = self.sregs[SegReg16::CS as usize];
        self.push(bus, cs);
        self.push(bus, self.ip);

        self.ip = dest_ip;
        self.sregs[SegReg16::CS as usize] = dest_cs;
    }
}
