use super::{IndexMode, Z80, Z80Flags};

impl Z80 {
    pub(crate) fn read_reg8_or_mem(
        &mut self,
        reg: u8,
        mode: IndexMode,
        indexed_address: Option<u16>,
        bus: &mut impl common::Bus,
    ) -> u8 {
        if reg & 7 == 6 {
            let address = if mode == IndexMode::HL {
                self.hl()
            } else {
                indexed_address.expect("indexed memory operand missing displacement")
            };
            self.read_byte(bus, address)
        } else {
            let register_mode = if indexed_address.is_some() {
                IndexMode::HL
            } else {
                mode
            };
            self.read_reg8_plain(reg, register_mode)
        }
    }

    pub(crate) fn write_reg8_or_mem(
        &mut self,
        reg: u8,
        mode: IndexMode,
        indexed_address: Option<u16>,
        value: u8,
        bus: &mut impl common::Bus,
    ) {
        if reg & 7 == 6 {
            let address = if mode == IndexMode::HL {
                self.hl()
            } else {
                indexed_address.expect("indexed memory operand missing displacement")
            };
            self.write_byte(bus, address, value);
        } else {
            let register_mode = if indexed_address.is_some() {
                IndexMode::HL
            } else {
                mode
            };
            self.write_reg8_plain(reg, register_mode, value);
        }
    }

    fn add_hl_rr(&mut self, mode: IndexMode, pair: u8) {
        self.set_q_latch(true);
        let left = self.current_hl(mode);
        let right = self.read_pair(pair, mode);
        let preserve =
            self.flags.compress() & (Z80Flags::SIGN | Z80Flags::ZERO | Z80Flags::PARITY_OVERFLOW);
        let result = left.wrapping_add(right);
        self.wz = left.wrapping_add(1);
        self.flags.expand(preserve);
        self.flags.set_subtract(false);
        self.flags
            .set_half_carry(((left ^ right ^ result) & 0x1000) != 0);
        self.flags
            .set_carry(u32::from(left) + u32::from(right) > 0xFFFF);
        self.set_xy((result >> 8) as u8);
        self.set_current_hl(mode, result);
        self.clk(7);
    }

    fn daa(&mut self) {
        self.set_q_latch(true);
        let before = self.a;
        if self.flags.carry() || self.a > 0x99 {
            self.a = if self.flags.subtract() {
                self.a.wrapping_sub(0x60)
            } else {
                self.a.wrapping_add(0x60)
            };
            self.flags.set_carry(true);
        }
        if self.flags.half_carry() || (self.a & 0x0F) > 0x09 {
            self.a = if self.flags.subtract() {
                self.a.wrapping_sub(0x06)
            } else {
                self.a.wrapping_add(0x06)
            };
        }
        self.set_parity(self.a);
        self.set_xysz(self.a);
        let half_carry = ((self.a ^ before) & 0x10) != 0;
        self.flags.set_half_carry(half_carry);
    }

    fn rotate_a_left_circular(&mut self) {
        self.set_q_latch(true);
        let preserve =
            self.flags.compress() & (Z80Flags::SIGN | Z80Flags::ZERO | Z80Flags::PARITY_OVERFLOW);
        let carry = self.a & 0x80 != 0;
        self.a = self.a.rotate_left(1);
        self.flags.expand(preserve);
        self.flags.set_carry(carry);
        self.flags.set_subtract(false);
        self.flags.set_half_carry(false);
        self.set_xy(self.a);
    }

    fn rotate_a_right_circular(&mut self) {
        self.set_q_latch(true);
        let preserve =
            self.flags.compress() & (Z80Flags::SIGN | Z80Flags::ZERO | Z80Flags::PARITY_OVERFLOW);
        let carry = self.a & 1 != 0;
        self.a = self.a.rotate_right(1);
        self.flags.expand(preserve);
        self.flags.set_carry(carry);
        self.flags.set_subtract(false);
        self.flags.set_half_carry(false);
        self.set_xy(self.a);
    }

    fn rotate_a_left_through_carry(&mut self) {
        self.set_q_latch(true);
        let preserve =
            self.flags.compress() & (Z80Flags::SIGN | Z80Flags::ZERO | Z80Flags::PARITY_OVERFLOW);
        let carry = self.a & 0x80 != 0;
        self.a = (self.a << 1) | u8::from(self.flags.carry());
        self.flags.expand(preserve);
        self.flags.set_carry(carry);
        self.flags.set_subtract(false);
        self.flags.set_half_carry(false);
        self.set_xy(self.a);
    }

    fn rotate_a_right_through_carry(&mut self) {
        self.set_q_latch(true);
        let preserve =
            self.flags.compress() & (Z80Flags::SIGN | Z80Flags::ZERO | Z80Flags::PARITY_OVERFLOW);
        let carry = self.a & 1 != 0;
        self.a = (u8::from(self.flags.carry()) << 7) | (self.a >> 1);
        self.flags.expand(preserve);
        self.flags.set_carry(carry);
        self.flags.set_subtract(false);
        self.flags.set_half_carry(false);
        self.set_xy(self.a);
    }

    fn cpl(&mut self) {
        self.set_q_latch(true);
        self.a ^= 0xFF;
        self.flags.set_subtract(true);
        self.flags.set_half_carry(true);
        self.set_xy(self.a);
    }

    fn scf(&mut self, xy_prefixed: bool) {
        if self.q != 0 && !xy_prefixed {
            self.flags.set_xy(0);
        }
        self.flags.set_carry(true);
        self.flags.set_subtract(false);
        self.flags.set_half_carry(false);
        let xy = (self.flags.compress() | self.a) & (Z80Flags::X | Z80Flags::Y);
        let flags = self.flags.compress();
        self.flags
            .expand((flags & !(Z80Flags::X | Z80Flags::Y)) | xy);
        self.set_q_latch(true);
    }

    fn ccf(&mut self, xy_prefixed: bool) {
        if self.q != 0 && !xy_prefixed {
            self.flags.set_xy(0);
        }
        let carry = self.flags.carry();
        self.flags.set_half_carry(carry);
        self.flags.set_carry(!carry);
        self.flags.set_subtract(false);
        let xy = (self.flags.compress() | self.a) & (Z80Flags::X | Z80Flags::Y);
        let flags = self.flags.compress();
        self.flags
            .expand((flags & !(Z80Flags::X | Z80Flags::Y)) | xy);
        self.set_q_latch(true);
    }

    fn jr(&mut self, bus: &mut impl common::Bus) {
        self.set_q_latch(false);
        let displacement = self.fetch_u8(bus) as i8;
        self.clk(5);
        self.wz = self.pc.wrapping_add_signed(i16::from(displacement));
        self.pc = self.wz;
    }

    fn jr_conditional(&mut self, condition: u8, bus: &mut impl common::Bus) {
        self.set_q_latch(false);
        let displacement = self.fetch_u8(bus) as i8;
        if !self.read_condition(condition) {
            return;
        }
        self.clk(5);
        self.wz = self.pc.wrapping_add_signed(i16::from(displacement));
        self.pc = self.wz;
    }

    fn djnz(&mut self, bus: &mut impl common::Bus) {
        self.set_q_latch(false);
        self.clk(1);
        let displacement = self.fetch_u8(bus) as i8;
        self.b = self.b.wrapping_sub(1);
        if self.b == 0 {
            return;
        }
        self.clk(5);
        self.wz = self.pc.wrapping_add_signed(i16::from(displacement));
        self.pc = self.wz;
    }

    pub(crate) fn execute_base(
        &mut self,
        opcode: u8,
        mode: IndexMode,
        xy_prefixed: bool,
        bus: &mut impl common::Bus,
    ) {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let p = y >> 1;
        let q = y & 1;

        match x {
            0 => match z {
                0 => match y {
                    0 => self.set_q_latch(false),
                    1 => {
                        self.set_q_latch(false);
                        self.exchange_af();
                    }
                    2 => self.djnz(bus),
                    3 => self.jr(bus),
                    4..=7 => self.jr_conditional(y - 4, bus),
                    _ => unreachable!(),
                },
                1 => {
                    if q == 0 {
                        self.set_q_latch(false);
                        let value = self.fetch_u16(bus);
                        self.write_pair(p, mode, value);
                    } else {
                        self.add_hl_rr(mode, p);
                    }
                }
                2 => {
                    self.set_q_latch(false);
                    match (q, p) {
                        (0, 0) => {
                            let address = self.bc();
                            self.write_byte(bus, address, self.a);
                            self.wz = (u16::from(self.a) << 8) | address.wrapping_add(1) & 0x00FF;
                        }
                        (0, 1) => {
                            let address = self.de();
                            self.write_byte(bus, address, self.a);
                            self.wz = (u16::from(self.a) << 8) | address.wrapping_add(1) & 0x00FF;
                        }
                        (0, 2) => {
                            let address = self.fetch_u16(bus);
                            let value = self.current_hl(mode);
                            self.write_byte(bus, address, value as u8);
                            self.write_byte(bus, address.wrapping_add(1), (value >> 8) as u8);
                            self.wz = address.wrapping_add(1);
                        }
                        (0, 3) => {
                            let address = self.fetch_u16(bus);
                            self.write_byte(bus, address, self.a);
                            self.wz = (u16::from(self.a) << 8) | (address.wrapping_add(1) & 0x00FF);
                        }
                        (1, 0) => {
                            let address = self.bc();
                            self.a = self.read_byte(bus, address);
                            self.wz = address.wrapping_add(1);
                        }
                        (1, 1) => {
                            let address = self.de();
                            self.a = self.read_byte(bus, address);
                            self.wz = address.wrapping_add(1);
                        }
                        (1, 2) => {
                            let address = self.fetch_u16(bus);
                            let low = self.read_byte(bus, address);
                            let high = self.read_byte(bus, address.wrapping_add(1));
                            self.set_current_hl(mode, u16::from(low) | (u16::from(high) << 8));
                            self.wz = address.wrapping_add(1);
                        }
                        (1, 3) => {
                            let address = self.fetch_u16(bus);
                            self.a = self.read_byte(bus, address);
                            self.wz = address.wrapping_add(1);
                        }
                        _ => unreachable!(),
                    }
                }
                3 => {
                    self.set_q_latch(false);
                    if q == 0 {
                        let value = self.read_pair(p, mode).wrapping_add(1);
                        self.write_pair(p, mode, value);
                    } else {
                        let value = self.read_pair(p, mode).wrapping_sub(1);
                        self.write_pair(p, mode, value);
                    }
                    self.clk(2);
                }
                4 => {
                    self.set_q_latch(true);
                    let indexed_address = (mode != IndexMode::HL && y == 6)
                        .then(|| self.fetch_indexed_addr(mode, bus));
                    let value = self.read_reg8_or_mem(y, mode, indexed_address, bus);
                    let result = self.inc8(value);
                    if y == 6 {
                        self.clk(1);
                    }
                    self.write_reg8_or_mem(y, mode, indexed_address, result, bus);
                }
                5 => {
                    self.set_q_latch(true);
                    let indexed_address = (mode != IndexMode::HL && y == 6)
                        .then(|| self.fetch_indexed_addr(mode, bus));
                    let value = self.read_reg8_or_mem(y, mode, indexed_address, bus);
                    let result = self.dec8(value);
                    if y == 6 {
                        self.clk(1);
                    }
                    self.write_reg8_or_mem(y, mode, indexed_address, result, bus);
                }
                6 => {
                    self.set_q_latch(false);
                    let indexed_address = if mode != IndexMode::HL && y == 6 {
                        let displacement = self.fetch_u8(bus) as i8;
                        let address = self
                            .current_hl(mode)
                            .wrapping_add_signed(i16::from(displacement));
                        self.wz = address;
                        Some(address)
                    } else {
                        None
                    };
                    let value = self.fetch_u8(bus);
                    if indexed_address.is_some() {
                        self.clk(2);
                    }
                    self.write_reg8_or_mem(y, mode, indexed_address, value, bus);
                }
                7 => match y {
                    0 => self.rotate_a_left_circular(),
                    1 => self.rotate_a_right_circular(),
                    2 => self.rotate_a_left_through_carry(),
                    3 => self.rotate_a_right_through_carry(),
                    4 => self.daa(),
                    5 => self.cpl(),
                    6 => self.scf(xy_prefixed),
                    7 => self.ccf(xy_prefixed),
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            1 => {
                if y == 6 && z == 6 {
                    self.set_q_latch(false);
                    self.halted = true;
                } else {
                    self.set_q_latch(false);
                    let indexed_address = (mode != IndexMode::HL && (y == 6 || z == 6))
                        .then(|| self.fetch_indexed_addr(mode, bus));
                    let value = self.read_reg8_or_mem(z, mode, indexed_address, bus);
                    self.write_reg8_or_mem(y, mode, indexed_address, value, bus);
                }
            }
            2 => {
                self.set_q_latch(true);
                let indexed_address =
                    (mode != IndexMode::HL && z == 6).then(|| self.fetch_indexed_addr(mode, bus));
                let value = self.read_reg8_or_mem(z, mode, indexed_address, bus);
                match y {
                    0 => self.a = self.add8(self.a, value, false),
                    1 => self.a = self.add8(self.a, value, self.flags.carry()),
                    2 => self.a = self.sub8(self.a, value, false),
                    3 => self.a = self.sub8(self.a, value, self.flags.carry()),
                    4 => self.a = self.and8(self.a, value),
                    5 => self.a = self.xor8(self.a, value),
                    6 => self.a = self.or8(self.a, value),
                    7 => self.cp8(self.a, value),
                    _ => unreachable!(),
                }
            }
            3 => match z {
                0 => {
                    self.set_q_latch(false);
                    self.clk(1);
                    if self.read_condition(y) {
                        self.wz = self.pop(bus);
                        self.pc = self.wz;
                    }
                }
                1 => {
                    if q == 0 {
                        self.set_q_latch(false);
                        let value = self.pop(bus);
                        self.write_pair2(p, mode, value);
                    } else {
                        match p {
                            0 => {
                                self.set_q_latch(false);
                                self.wz = self.pop(bus);
                                self.pc = self.wz;
                            }
                            1 => {
                                self.set_q_latch(false);
                                self.exchange_shadow();
                            }
                            2 => {
                                self.set_q_latch(false);
                                self.pc = self.current_hl(mode);
                            }
                            3 => {
                                self.set_q_latch(false);
                                self.clk(2);
                                self.sp = self.current_hl(mode);
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                2 => {
                    self.set_q_latch(false);
                    let address = self.fetch_u16(bus);
                    self.wz = address;
                    if self.read_condition(y) {
                        self.pc = address;
                    }
                }
                3 => match y {
                    0 => {
                        self.set_q_latch(false);
                        let address = self.fetch_u16(bus);
                        self.wz = address;
                        self.pc = address;
                    }
                    1 => unreachable!("CB prefix handled before execute_base"),
                    2 => {
                        self.set_q_latch(false);
                        let low = self.fetch_u8(bus);
                        self.wz = (u16::from(self.a) << 8) | u16::from(low);
                        self.write_port(bus, self.wz, self.a);
                        self.wz = (self.wz & 0xFF00) | (self.wz.wrapping_add(1) & 0x00FF);
                    }
                    3 => {
                        self.set_q_latch(false);
                        let low = self.fetch_u8(bus);
                        self.wz = (u16::from(self.a) << 8) | u16::from(low);
                        self.a = self.read_port(bus, self.wz);
                        self.wz = self.wz.wrapping_add(1);
                    }
                    4 => {
                        self.set_q_latch(false);
                        let current = self.current_hl(mode);
                        let low = self.read_byte(bus, self.sp);
                        let high = self.read_byte(bus, self.sp.wrapping_add(1));
                        self.wz = u16::from(low) | (u16::from(high) << 8);
                        self.clk(1);
                        self.write_byte(bus, self.sp.wrapping_add(1), (current >> 8) as u8);
                        self.write_byte(bus, self.sp, current as u8);
                        self.clk(2);
                        self.set_current_hl(mode, self.wz);
                    }
                    5 => {
                        self.set_q_latch(false);
                        self.exchange_de_with_hl(IndexMode::HL);
                    }
                    6 => {
                        self.set_q_latch(false);
                        self.iff1 = false;
                        self.iff2 = false;
                    }
                    7 => {
                        self.set_q_latch(false);
                        self.iff1 = true;
                        self.iff2 = true;
                        self.ei = 1;
                    }
                    _ => unreachable!(),
                },
                4 => {
                    self.set_q_latch(false);
                    let address = self.fetch_u16(bus);
                    self.wz = address;
                    if self.read_condition(y) {
                        self.clk(1);
                        self.push(bus, self.pc);
                        self.pc = address;
                    }
                }
                5 => {
                    if q == 0 {
                        self.set_q_latch(false);
                        self.clk(1);
                        let value = self.read_pair2(p, mode);
                        self.push(bus, value);
                    } else {
                        match p {
                            0 => {
                                self.set_q_latch(false);
                                let address = self.fetch_u16(bus);
                                self.wz = address;
                                self.clk(1);
                                self.push(bus, self.pc);
                                self.pc = address;
                            }
                            1 | 3 => unreachable!("DD/FD prefixes handled before execute_base"),
                            2 => unreachable!("ED prefix handled before execute_base"),
                            _ => unreachable!(),
                        }
                    }
                }
                6 => {
                    let immediate = self.fetch_u8(bus);
                    self.set_q_latch(true);
                    match y {
                        0 => self.a = self.add8(self.a, immediate, false),
                        1 => self.a = self.add8(self.a, immediate, self.flags.carry()),
                        2 => self.a = self.sub8(self.a, immediate, false),
                        3 => self.a = self.sub8(self.a, immediate, self.flags.carry()),
                        4 => self.a = self.and8(self.a, immediate),
                        5 => self.a = self.xor8(self.a, immediate),
                        6 => self.a = self.or8(self.a, immediate),
                        7 => self.cp8(self.a, immediate),
                        _ => unreachable!(),
                    }
                }
                7 => {
                    self.set_q_latch(false);
                    self.clk(1);
                    self.push(bus, self.pc);
                    self.wz = u16::from(y) << 3;
                    self.pc = self.wz;
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}
