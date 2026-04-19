use super::{IndexMode, Z80};

impl Z80 {
    pub(crate) fn execute_cb(&mut self, opcode: u8, bus: &mut impl common::Bus) {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;

        match x {
            0 => {
                self.set_q_latch(true);
                let indexed_address = (z == 6).then(|| self.hl());
                let value = self.read_reg8_or_mem(z, IndexMode::HL, indexed_address, bus);
                let result = match y {
                    0 => self.rlc(value),
                    1 => self.rrc(value),
                    2 => self.rl(value),
                    3 => self.rr(value),
                    4 => self.sla(value),
                    5 => self.sra(value),
                    6 => self.sll(value),
                    7 => self.srl(value),
                    _ => unreachable!(),
                };
                if z == 6 {
                    self.clk(1);
                }
                self.write_reg8_or_mem(z, IndexMode::HL, indexed_address, result, bus);
            }
            1 => {
                self.set_q_latch(true);
                let indexed_address = (z == 6).then(|| self.hl());
                let value = self.read_reg8_or_mem(z, IndexMode::HL, indexed_address, bus);
                if z == 6 {
                    self.clk(1);
                    self.bit_test_with_xy(y, value, (self.wz >> 8) as u8);
                } else {
                    self.bit_test(y, value);
                }
            }
            2 => {
                self.set_q_latch(false);
                let indexed_address = (z == 6).then(|| self.hl());
                let value = self.read_reg8_or_mem(z, IndexMode::HL, indexed_address, bus);
                let result = value & !(1 << y);
                if z == 6 {
                    self.clk(1);
                }
                self.write_reg8_or_mem(z, IndexMode::HL, indexed_address, result, bus);
            }
            3 => {
                self.set_q_latch(false);
                let indexed_address = (z == 6).then(|| self.hl());
                let value = self.read_reg8_or_mem(z, IndexMode::HL, indexed_address, bus);
                let result = value | (1 << y);
                if z == 6 {
                    self.clk(1);
                }
                self.write_reg8_or_mem(z, IndexMode::HL, indexed_address, result, bus);
            }
            _ => unreachable!(),
        }
    }

    pub(crate) fn execute_cb_indexed(
        &mut self,
        opcode: u8,
        address: u16,
        bus: &mut impl common::Bus,
    ) {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let value = self.read_byte(bus, address);

        match x {
            0 => {
                self.set_q_latch(true);
                let result = match y {
                    0 => self.rlc(value),
                    1 => self.rrc(value),
                    2 => self.rl(value),
                    3 => self.rr(value),
                    4 => self.sla(value),
                    5 => self.sra(value),
                    6 => self.sll(value),
                    7 => self.srl(value),
                    _ => unreachable!(),
                };
                self.clk(1);
                if z != 6 {
                    self.write_reg8_plain(z, IndexMode::HL, result);
                }
                self.write_byte(bus, address, result);
            }
            1 => {
                self.set_q_latch(true);
                self.clk(1);
                self.bit_test_with_xy(y, value, (self.wz >> 8) as u8);
            }
            2 => {
                self.set_q_latch(false);
                let result = value & !(1 << y);
                self.clk(1);
                if z != 6 {
                    self.write_reg8_plain(z, IndexMode::HL, result);
                }
                self.write_byte(bus, address, result);
            }
            3 => {
                self.set_q_latch(false);
                let result = value | (1 << y);
                self.clk(1);
                if z != 6 {
                    self.write_reg8_plain(z, IndexMode::HL, result);
                }
                self.write_byte(bus, address, result);
            }
            _ => unreachable!(),
        }
    }
}
