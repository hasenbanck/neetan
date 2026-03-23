use common::EventKind;

use crate::{
    Pc9801Bus, Tracing,
    bus::{INTERRUPT_DELAY_CYCLES, bios},
};

impl<T: Tracing> Pc9801Bus<T> {
    pub(super) fn handle_sasi_execution(&mut self) {
        let raise_irq = self.sasi.complete_operation();
        if raise_irq {
            self.scheduler.schedule(
                EventKind::SasiInterrupt,
                self.current_cycle + INTERRUPT_DELAY_CYCLES,
            );
            self.update_next_event_cycle();
        }
    }

    pub(super) fn handle_sasi_interrupt(&mut self) {
        // SASI uses IRQ 9 (slave PIC IRQ 1).
        self.pic.set_irq(9);
        self.tracer.trace_irq_raise(9);
    }

    pub(super) fn process_ide_action(&mut self, action: device::ide::IdeAction) {
        match action {
            device::ide::IdeAction::None => {}
            device::ide::IdeAction::ScheduleCompletion => {
                self.scheduler.schedule(
                    EventKind::IdeExecution,
                    self.current_cycle + INTERRUPT_DELAY_CYCLES,
                );
                self.update_next_event_cycle();
            }
        }
    }

    pub(super) fn handle_ide_execution(&mut self) {
        let raise_irq = self.ide.complete_operation();
        if raise_irq {
            self.scheduler.schedule(
                EventKind::IdeInterrupt,
                self.current_cycle + INTERRUPT_DELAY_CYCLES,
            );
            self.update_next_event_cycle();
        }
    }

    pub(super) fn handle_ide_interrupt(&mut self) {
        // IDE uses IRQ 9 (slave PIC IRQ 1), same as SASI.
        self.pic.set_irq(9);
        self.tracer.trace_irq_raise(9);
    }

    /// Returns `true` if a SASI HLE trap is pending.
    pub fn sasi_hle_pending(&self) -> bool {
        self.sasi.hle_pending()
    }

    /// Returns `true` if an IDE HLE trap is pending.
    pub fn ide_hle_pending(&self) -> bool {
        self.ide.hle_pending()
    }

    /// Returns `true` if a BIOS HLE trap is pending.
    pub fn bios_hle_pending(&self) -> bool {
        self.bios.hle_pending()
    }

    /// Executes the pending SASI HLE operation using the CPU's stack frame.
    ///
    /// The real BIOS pushes DS, SI, DI, ES, BP, DX, CX, BX, AX (9 words)
    /// before dispatching to the SASI ROM entry. The ROM triggers the trap
    /// via `OUT TRAP_PORT, AL`, then pops all registers and IRETs.
    /// The stack frame at SS:SP has:
    /// SP+0x00: AX, SP+0x02: BX, SP+0x04: CX, SP+0x06: DX, SP+0x08: BP,
    /// SP+0x0A: ES, SP+0x0C: DI, SP+0x0E: SI, SP+0x10: DS
    /// After these 9 words, the INT frame follows:
    /// SP+0x12: IP, SP+0x14: CS, SP+0x16: FLAGS
    pub fn execute_sasi_hle(&mut self, ss: u16, sp: u16) {
        let stack_base = (u32::from(ss) << 4).wrapping_add(u32::from(sp));

        let ax = self.read_word_direct(stack_base);
        let bx = self.read_word_direct(stack_base + 0x02);
        let cx = self.read_word_direct(stack_base + 0x04);
        let dx = self.read_word_direct(stack_base + 0x06);
        let bp = self.read_word_direct(stack_base + 0x08);
        let es = self.read_word_direct(stack_base + 0x0A);

        let function_code = (ax >> 8) as u8;
        let drive_select = ax as u8;
        let drive_idx = device::sasi::drive_index(drive_select);
        let function = function_code & 0x0F;

        let result_ah = match function {
            0x03 => {
                let current_lo = self.read_byte_with_access_page(0x055C);
                let current_hi = self.read_byte_with_access_page(0x055D);
                let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                let disk_equip = self.sasi.execute_init(current_equip);
                self.write_byte_with_access_page(0x055C, disk_equip as u8);
                self.write_byte_with_access_page(0x055D, (disk_equip >> 8) as u8);

                // Unmask IRQ 0 (system timer). The real PC-98 BIOS calls
                // INT 1Ch AH=02 during init which unmasks the timer via
                // `pic.imr &= ~PIC_SYSTEMTIMER`.
                self.pic.state.chips[0].imr &= !0x01;
                self.pic.invalidate_irq_cache();

                0x00
            }
            0x04 => match function_code {
                // New Sense: returns geometry in AH/BX/CX/DX.
                0x84 => {
                    let sense_result = self.sasi.execute_sense(drive_idx);
                    if sense_result >= 0x20 {
                        sense_result
                    } else {
                        if let Some(geometry) = self.sasi.drive_geometry(drive_idx) {
                            let write_stack = |bus: &mut Self, offset: u32, value: u16| {
                                let addr = stack_base + offset;
                                bus.memory.write_byte(addr, value as u8);
                                bus.memory.write_byte(addr + 1, (value >> 8) as u8);
                            };
                            write_stack(self, 0x02, geometry.sector_size);
                            write_stack(self, 0x04, geometry.cylinders.saturating_sub(1));
                            let dx_value = ((u16::from(geometry.heads)) << 8)
                                | u16::from(geometry.sectors_per_track);
                            write_stack(self, 0x06, dx_value);
                        }
                        sense_result
                    }
                }
                _ => self.sasi.execute_sense(drive_idx),
            },
            0x05 => {
                let xfer = device::sasi::transfer_size(bx);
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = device::sasi::buffer_address(es, bp);
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &self.memory;
                self.sasi.execute_write(drive_idx, xfer, pos, addr, |a| {
                    let phys = bios::hle_page_translate(cr0, cr3, a, memory);
                    memory.read_byte(phys)
                })
            }
            0x06 => {
                let xfer = device::sasi::transfer_size(bx);
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = device::sasi::buffer_address(es, bp);
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &mut self.memory;
                self.sasi
                    .execute_read(drive_idx, xfer, pos, addr, |a, byte| {
                        let phys = bios::hle_page_translate(cr0, cr3, a, memory);
                        memory.write_byte(phys, byte);
                    })
            }
            0x07 | 0x0F => 0x00, // Retract: no-op
            0x0D => {
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                self.sasi.execute_format(drive_idx, pos)
            }
            0x0E => {
                let mode_set_result = self.sasi.execute_mode_set(drive_idx);
                if mode_set_result == 0x00 {
                    self.apply_sasi_mode_set(drive_idx, function_code);
                }
                mode_set_result
            }
            0x01 => 0x00, // Verify: no-op
            _ => 0x40,    // Unsupported: Equipment Check error
        };

        self.tracer
            .trace_sasi_hle(function_code, drive_select, result_ah, bx, cx, dx, es, bp);

        // Write result AH back to stack (high byte of AX word at stack_base).
        self.memory.write_byte(stack_base + 1, result_ah);

        // Update FLAGS on the stack: set CF on error, clear on success.
        let flags_addr = stack_base + 0x16;
        let mut flags = self.read_word_direct(flags_addr);
        if result_ah >= 0x20 {
            flags |= 0x0001; // Set CF (error)
        } else {
            flags &= !0x0001; // Clear CF (success or informational)
        }
        self.memory.write_byte(flags_addr, flags as u8);
        self.memory.write_byte(flags_addr + 1, (flags >> 8) as u8);

        self.sasi.clear_hle_pending();
    }

    pub(super) fn apply_sasi_mode_set(&mut self, drive_idx: usize, function_code: u8) {
        if let Some((offset, segment)) = self
            .sasi
            .mode_set_parameter_pointer(drive_idx, function_code)
        {
            let pointer_address = match drive_idx {
                0 => Some(0x05E8u32),
                1 => Some(0x05ECu32),
                _ => None,
            };

            if let Some(address) = pointer_address {
                self.write_byte_with_access_page(address, offset as u8);
                self.write_byte_with_access_page(address + 1, (offset >> 8) as u8);
                self.write_byte_with_access_page(address + 2, segment as u8);
                self.write_byte_with_access_page(address + 3, (segment >> 8) as u8);
            }
        }

        if drive_idx <= 1 {
            let flag_bit = 1u8 << drive_idx;
            let mode_is_half_height = function_code & 0x80 != 0;
            let sector_size = self
                .sasi
                .drive_geometry(drive_idx)
                .map(|geometry| geometry.sector_size)
                .unwrap_or(256);
            let mut mode_flags = self.read_byte_with_access_page(0x0481);

            // Full-height mode sets the compatibility bit for 512-byte sectors.
            if !mode_is_half_height && sector_size == 512 {
                mode_flags |= flag_bit;
            } else {
                mode_flags &= !flag_bit;
            }
            self.write_byte_with_access_page(0x0481, mode_flags);
        }
    }

    /// Executes the pending IDE HLE operation using the CPU's stack frame.
    ///
    /// The stack frame layout is identical to SASI HLE:
    /// SP+0x00: AX, SP+0x02: BX, SP+0x04: CX, SP+0x06: DX, SP+0x08: BP,
    /// SP+0x0A: ES, SP+0x0C: DI, SP+0x0E: SI, SP+0x10: DS
    /// After these 9 words, the INT frame follows:
    /// SP+0x12: IP, SP+0x14: CS, SP+0x16: FLAGS
    pub fn execute_ide_hle(&mut self, ss: u16, sp: u16) {
        let stack_base = (u32::from(ss) << 4).wrapping_add(u32::from(sp));

        let ax = self.read_word_direct(stack_base);
        let bx = self.read_word_direct(stack_base + 0x02);
        let cx = self.read_word_direct(stack_base + 0x04);
        let dx = self.read_word_direct(stack_base + 0x06);
        let bp = self.read_word_direct(stack_base + 0x08);
        let es = self.read_word_direct(stack_base + 0x0A);

        let function_code = (ax >> 8) as u8;
        let drive_select = ax as u8;
        let drive_idx = device::ide::drive_index(drive_select);

        let result_ah = match function_code {
            // SASI-compatible functions (lower nibble dispatch).
            0x03 => {
                let current_lo = self.read_byte_with_access_page(0x055C);
                let current_hi = self.read_byte_with_access_page(0x055D);
                let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                let disk_equip = self.ide.execute_init(current_equip);
                self.write_byte_with_access_page(0x055C, disk_equip as u8);
                self.write_byte_with_access_page(0x055D, (disk_equip >> 8) as u8);
                self.pic.state.chips[0].imr &= !0x01;
                self.pic.invalidate_irq_cache();
                0x00
            }
            0x04 | 0x84 => {
                if function_code == 0x84 {
                    let sense_result = self.ide.execute_sense(drive_idx);
                    if sense_result >= 0x20 {
                        sense_result
                    } else {
                        if let Some(geometry) = self.ide.drive_geometry(drive_idx) {
                            let write_stack = |bus: &mut Self, offset: u32, value: u16| {
                                let addr = stack_base + offset;
                                bus.memory.write_byte(addr, value as u8);
                                bus.memory.write_byte(addr + 1, (value >> 8) as u8);
                            };
                            write_stack(self, 0x02, geometry.sector_size);
                            write_stack(self, 0x04, geometry.cylinders.saturating_sub(1));
                            let dx_value = ((u16::from(geometry.heads)) << 8)
                                | u16::from(geometry.sectors_per_track);
                            write_stack(self, 0x06, dx_value);
                        }
                        sense_result
                    }
                } else {
                    self.ide.execute_sense(drive_idx)
                }
            }
            0x05 => {
                let xfer = device::ide::transfer_size(bx);
                let geometry = self.ide.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::ide::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = device::ide::buffer_address(es, bp);
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &self.memory;
                self.ide.execute_write(drive_idx, xfer, pos, addr, |a| {
                    let phys = bios::hle_page_translate(cr0, cr3, a, memory);
                    memory.read_byte(phys)
                })
            }
            0x06 => {
                let xfer = device::ide::transfer_size(bx);
                let geometry = self.ide.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::ide::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = device::ide::buffer_address(es, bp);
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &mut self.memory;
                self.ide
                    .execute_read(drive_idx, xfer, pos, addr, |a, byte| {
                        let phys = bios::hle_page_translate(cr0, cr3, a, memory);
                        memory.write_byte(phys, byte);
                    })
            }
            0x07 | 0x0F => 0x00, // Retract: no-op
            0x0D => {
                let geometry = self.ide.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::ide::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                self.ide.execute_format(drive_idx, pos)
            }
            0x0E => self.ide.execute_mode_set(drive_idx),
            0x01 => 0x00, // Verify: no-op

            // IDE-specific motor control extensions.
            0xD0 => self.ide.execute_check_power_mode(drive_idx),
            0xE0 => self.ide.execute_motor_on(drive_idx),
            0xF0 => self.ide.execute_motor_off(drive_idx),

            _ => 0x40, // Unsupported: Equipment Check error
        };

        // Write result AH back to stack (high byte of AX word at stack_base).
        self.memory.write_byte(stack_base + 1, result_ah);

        // Update FLAGS on the stack: set CF on error, clear on success.
        let flags_addr = stack_base + 0x16;
        let mut flags = self.read_word_direct(flags_addr);
        if result_ah >= 0x20 {
            flags |= 0x0001;
        } else {
            flags &= !0x0001;
        }
        self.memory.write_byte(flags_addr, flags as u8);
        self.memory.write_byte(flags_addr + 1, (flags >> 8) as u8);

        self.ide.clear_hle_pending();
    }
}
