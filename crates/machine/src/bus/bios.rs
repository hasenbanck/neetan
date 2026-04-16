//! BIOS HLE handler implementations.
//!
//! Each handler reads/writes CPU registers directly via the `Cpu` trait.
//! The ROM stubs save AX/DX on the stack (clobbered by the trap OUT),
//! write the vector number to the trap port, and IRET. The Rust side
//! restores AX/DX from the stack before dispatching to the handler.

use common::{Cpu, MachineModel};
use device::{floppy::D88MediaType, i8253_pit::PIT_FLAG_I, upd7220_gdc::GdcScrollPartition};

use super::{
    BootDevice, Pc9801Bus,
    os_adapter::{OsCpuAccess, OsDiskIo, OsMemoryAccess},
};
use crate::{Tracing, memory::Pc9801Memory};

const PIT_CLOCK_8MHZ_LINEAGE: u32 = 1_996_800;

fn iret_stack_base(cpu: &impl Cpu) -> u32 {
    cpu.segment_base(common::SegmentRegister::SS)
        .wrapping_add(u32::from(cpu.sp()))
}

pub(super) fn hle_page_translate(cr0: u32, cr3: u32, linear: u32, memory: &Pc9801Memory) -> u32 {
    if cr0 & 0x8000_0001 != 0x8000_0001 {
        return linear;
    }
    let dir_idx = (linear >> 22) & 0x3FF;
    let tbl_idx = (linear >> 12) & 0x3FF;
    let offset = linear & 0xFFF;
    let pde_addr = (cr3 & 0xFFFFF000) + dir_idx * 4;
    let pde = memory.read_byte(pde_addr) as u32
        | ((memory.read_byte(pde_addr + 1) as u32) << 8)
        | ((memory.read_byte(pde_addr + 2) as u32) << 16)
        | ((memory.read_byte(pde_addr + 3) as u32) << 24);
    if pde & 1 == 0 {
        return linear;
    }
    let pte_addr = (pde & 0xFFFFF000) + tbl_idx * 4;
    let pte = memory.read_byte(pte_addr) as u32
        | ((memory.read_byte(pte_addr + 1) as u32) << 8)
        | ((memory.read_byte(pte_addr + 2) as u32) << 16)
        | ((memory.read_byte(pte_addr + 3) as u32) << 24);
    if pte & 1 == 0 {
        return linear;
    }
    (pte & 0xFFFFF000) | offset
}

impl<T: Tracing> Pc9801Bus<T> {
    pub(crate) fn handle_bios_interval_timer_tick(&mut self) {
        if !self.bios_interval_timer_active {
            return;
        }

        let count = self.ram_read_u16(0x058A);
        let new_count = count.wrapping_sub(1);
        self.ram_write_u16(0x058A, new_count);

        if count > 0 && new_count == 0 {
            self.bios_interval_timer_active = false;
        }
    }

    fn hle_linear_address(&self, cpu: &impl Cpu, seg: common::SegmentRegister, off: u32) -> u32 {
        cpu.segment_base(seg).wrapping_add(off)
    }

    fn hle_physical_address(&self, cpu: &impl Cpu, seg: common::SegmentRegister, off: u32) -> u32 {
        let linear = self.hle_linear_address(cpu, seg, off);
        hle_page_translate(self.hle_cr0, self.hle_cr3, linear, &self.memory)
    }

    fn hle_read_byte(&self, cpu: &impl Cpu, seg: common::SegmentRegister, off: u32) -> u8 {
        let phys = self.hle_physical_address(cpu, seg, off);
        self.read_byte_direct(phys)
    }

    fn hle_write_byte(
        &mut self,
        cpu: &impl Cpu,
        seg: common::SegmentRegister,
        off: u32,
        value: u8,
    ) {
        let phys = self.hle_physical_address(cpu, seg, off);
        self.memory.write_byte(phys, value);
    }

    /// Configures paging state used by HLE BIOS routines (SASI, INT 1Fh, etc.).
    /// When paging is active (CR0.PG + CR0.PE), HLE memory accesses translate
    /// linear addresses through the page tables rooted at CR3.
    pub fn set_hle_paging(&mut self, cr0: u32, cr3: u32) {
        self.hle_cr0 = cr0;
        self.hle_cr3 = cr3;
    }

    /// Executes the pending BIOS HLE operation with direct CPU register access.
    pub(crate) fn execute_bios_hle(&mut self, cpu: &mut impl Cpu) {
        let vector = self.bios.pending_vector();
        self.bios.clear_hle_pending();

        // The assembly stub pushes AX and DX before clobbering them with the
        // trap port address and vector number. Restore the caller's original
        // values and adjust SP so the IRET frame sits at SS:SP+0.
        let sp = cpu.sp();
        let ss_base = cpu.segment_base(common::SegmentRegister::SS);
        let saved_dx = self.read_word_direct(ss_base.wrapping_add(u32::from(sp)));
        let saved_ax = self.read_word_direct(ss_base.wrapping_add(u32::from(sp.wrapping_add(2))));
        cpu.set_dx(saved_dx);
        cpu.set_ax(saved_ax);
        cpu.set_sp(sp.wrapping_add(4));

        self.tracer.trace_bios_hle(vector, cpu.ah(), cpu.al());

        match vector {
            0x08 => self.hle_int08h(cpu),
            0x09 => self.hle_int09h(cpu),
            0x0A => {
                self.pic.write_port0(0, 0x20);
                self.display_control.state.vsync_irq_enabled = true;
            }
            0x0B | 0x0D | 0x0E => self.pic.write_port0(0, 0x20),
            0x0C => self.hle_int0ch(cpu),
            0x10 | 0x11 | 0x14..=0x17 => {
                self.pic.write_port0(1, 0x20);
                self.pic.write_port0(0, 0x20);
            }
            0x12 => self.hle_int12h(cpu),
            0x13 => self.hle_int13h(cpu),
            0x18 => self.hle_int18h(cpu),
            0x19 => self.hle_int19h(cpu),
            0x1A => self.hle_int1ah(cpu),
            0x1B => self.hle_int1bh(cpu),
            0x1C => self.hle_int1ch(cpu),
            0x1F => self.hle_int1fh(cpu),
            0x20..=0x2A | 0x2F | 0x33 | 0x67 | 0xDC | 0xFE => {
                if let Some(mut neetan_os) = self.os.take() {
                    let mut cpu_access = OsCpuAccess(cpu);
                    let mut mem_access = OsMemoryAccess(&mut self.memory);
                    let mut disk_io = OsDiskIo {
                        floppy: &mut self.floppy,
                        sasi: &mut self.sasi,
                        ide: &mut self.ide,
                    };
                    neetan_os.dispatch(
                        vector,
                        &mut cpu_access,
                        &mut mem_access,
                        &mut disk_io,
                        &mut self.tracer,
                    );
                    self.os = Some(neetan_os);
                    self.sync_cursor();
                }
            }
            0xD2 => {}
            0xF0 => {
                if std::mem::take(&mut self.needs_full_reinit) {
                    self.initialize_post_boot_state();
                }
                self.hle_bootstrap(cpu);
            }
            0xF1 | 0xF2 => self.hle_bootstrap(cpu),
            _ => {}
        }
    }

    /// Sync HLE OS software cursor to GDC hardware cursor.
    fn sync_cursor(&mut self) {
        let iosys = os::tables::IOSYS_BASE as usize;
        let cursor_y = self.memory.state.ram[iosys + os::tables::IOSYS_OFF_CURSOR_Y as usize];
        let cursor_x = self.memory.state.ram[iosys + os::tables::IOSYS_OFF_CURSOR_X as usize];
        self.gdc_master.state.ead = cursor_y as u32 * 80 + cursor_x as u32;
        let cursor_visible =
            self.memory.state.ram[iosys + os::tables::IOSYS_OFF_CURSOR_VISIBLE as usize];
        self.gdc_master.state.cursor_display = cursor_visible != 0;
    }

    pub(super) fn set_iret_cf(&mut self, cpu: &impl Cpu, error: bool) {
        let base = iret_stack_base(cpu);
        let flags_addr = base + 0x04;
        let mut flags = self.read_word_direct(flags_addr);
        if error {
            flags |= 0x0001;
        } else {
            flags &= !0x0001;
        }
        self.memory.write_byte(flags_addr, flags as u8);
        self.memory.write_byte(flags_addr + 1, (flags >> 8) as u8);
    }

    fn write_result_ah_cf(&mut self, cpu: &mut impl Cpu, result_ah: u8) {
        cpu.set_ah(result_ah);
        self.set_iret_cf(cpu, result_ah >= 0x20);
    }

    fn write_mem_word(&mut self, addr: u32, value: u16) {
        self.memory.write_byte(addr, value as u8);
        self.memory.write_byte(addr + 1, (value >> 8) as u8);
    }

    fn ram_read_u16(&self, addr: usize) -> u16 {
        u16::from_le_bytes([self.memory.state.ram[addr], self.memory.state.ram[addr + 1]])
    }

    fn ram_write_u16(&mut self, addr: usize, value: u16) {
        let bytes = value.to_le_bytes();
        self.memory.state.ram[addr] = bytes[0];
        self.memory.state.ram[addr + 1] = bytes[1];
    }

    fn hle_int08h(&mut self, cpu: &mut impl Cpu) {
        if !self.bios_interval_timer_active {
            self.pic.write_port0(0, 0x20);
            return;
        }

        // IRET frame layout at SS:SP: [IP +0] [CS +2] [FLAGS +4].
        let iret_base = iret_stack_base(cpu);

        let count = self.ram_read_u16(0x058A);
        let new_count = count.wrapping_sub(1);
        self.ram_write_u16(0x058A, new_count);

        if new_count == 0 && count > 0 {
            self.bios_interval_timer_active = false;
            // Timer expired (decremented from 1 to 0).
            // Mask IRQ 0 in master PIC. The real BIOS masks the timer
            // before calling the callback, preventing re-entrant timer
            // interrupts. The game's callback (or subsequent code) will
            // call INT 1CH AH=02H to restart the interval timer.
            self.pic.state.chips[0].imr |= 0x01;
            self.pic.invalidate_irq_cache();

            // Send EOI before the callback (matching real BIOS order).
            self.pic.write_port0(0, 0x20);

            // Fire the user callback by chaining the IRET frame.
            // The real BIOS invokes `INT 07H` which pushes FLAGS and
            // clears IF, so the callback runs with interrupts disabled.
            let callback_offset = self.ram_read_u16(0x001C);
            let callback_segment = self.ram_read_u16(0x001E);

            if callback_offset != 0 || callback_segment != 0 {
                let orig_flags = self.read_word_direct(iret_base + 0x04);
                let callback_base = iret_base.wrapping_sub(6);
                self.write_mem_word(callback_base, callback_offset);
                self.write_mem_word(callback_base + 0x02, callback_segment);
                self.write_mem_word(callback_base + 0x04, orig_flags & !0x0200);
                cpu.set_sp(cpu.sp().wrapping_sub(6));
            }
            return;
        }

        // Non-zero result: send EOI then reload PIT counter via INT 1CH AH=03H.
        // The real BIOS issues `MOV AH,03H; INT 1CH` so software hooks on INT 1CH
        // can intercept. We call the handler directly since the IVT points to our stub.
        self.pic.write_port0(0, 0x20);
        if new_count != 0 {
            self.int1ch_continue_interval_timer();
        }
    }

    fn hle_int09h(&mut self, cpu: &mut impl Cpu) {
        let (raw_code, clear_irq, retrigger_irq) = self.keyboard.read_data();
        if clear_irq {
            self.pic.clear_irq(1);
        }
        if retrigger_irq {
            self.pic.set_irq(1);
        }

        let key_code = raw_code & 0x7F;
        let is_release = raw_code & 0x80 != 0;
        let group = (key_code >> 3) as usize;
        let bit = 1u8 << (key_code & 0x07);

        if is_release {
            // Key release: clear key status bit.
            self.memory.state.ram[0x052A + group] &= !bit;

            // Update shift state for modifier key releases.
            if (0x70..0x75).contains(&key_code) || key_code == 0x7D {
                if key_code == 0x70 || key_code == 0x7D {
                    self.memory.state.ram[0x053A] &= !0x01;
                } else {
                    self.memory.state.ram[0x053A] &= !bit;
                }
                self.update_shift_key();
            }
        } else {
            // Key press: set key status bit.
            self.memory.state.ram[0x052A + group] |= bit;

            // Update shift state for modifier key presses.
            if key_code == 0x70 || key_code == 0x7D {
                self.memory.state.ram[0x053A] |= 0x01;
                self.update_shift_key();
            } else if (0x71..0x75).contains(&key_code) {
                self.memory.state.ram[0x053A] |= bit;
                self.update_shift_key();
            } else {
                // Non-modifier key press: translate and buffer.
                let count = self.memory.state.ram[0x0528];
                if count < 0x10 {
                    let code = self.translate_key(key_code);
                    if code != 0xFFFF {
                        self.memory.state.ram[0x0528] = count + 1;
                        let tail = self.ram_read_u16(0x0526) as usize;
                        self.memory.state.ram[tail] = code as u8;
                        self.memory.state.ram[tail + 1] = (code >> 8) as u8;
                        let mut new_tail = tail + 2;
                        if new_tail >= 0x0522 {
                            new_tail = 0x0502;
                        }
                        self.ram_write_u16(0x0526, new_tail as u16);
                    }
                }
            }
        }

        // Send EOI to master PIC.
        self.pic.write_port0(0, 0x20);

        // COPY key (0x60) -> INT 06H, STOP key (0x61) -> INT 05H.
        // The real BIOS dispatches these after EOI via the assembly wrapper.
        if !is_release {
            let int_vector = match key_code {
                0x60 => Some(0x06u8),
                0x61 => Some(0x05u8),
                _ => None,
            };
            if let Some(vector) = int_vector {
                let iret_base = iret_stack_base(cpu);
                let callback_offset = self.ram_read_u16((vector as usize) * 4);
                let callback_segment = self.ram_read_u16((vector as usize) * 4 + 2);
                if callback_offset != 0 || callback_segment != 0 {
                    let orig_ip = self.read_word_direct(iret_base);
                    let orig_cs = self.read_word_direct(iret_base + 0x02);
                    let orig_flags = self.read_word_direct(iret_base + 0x04);
                    self.write_mem_word(iret_base + 0x06, orig_ip);
                    self.write_mem_word(iret_base + 0x08, orig_cs);
                    self.write_mem_word(iret_base + 0x0A, orig_flags);
                    self.write_mem_word(iret_base, callback_offset);
                    self.write_mem_word(iret_base + 0x02, callback_segment);
                    self.write_mem_word(iret_base + 0x04, orig_flags & !0x0200);
                }
            }
        }
    }

    fn update_shift_key(&mut self) {
        let shift_sts = self.memory.state.ram[0x053A];

        let base = if shift_sts & 0x10 != 0 {
            7u16
        } else if shift_sts & 0x08 != 0 {
            6u16
        } else {
            let mut b = (shift_sts & 0x07) as u16;
            if b >= 6 {
                b -= 2;
            }
            b
        };
        let table_offset = 0x0B28 + base * 0x60;
        self.ram_write_u16(0x0522, table_offset);
    }

    fn translate_key(&self, key_code: u8) -> u16 {
        let table_offset = self.ram_read_u16(0x0522) as u32;
        let table_base = 0xFD800 + table_offset;

        if key_code <= 0x51 {
            if key_code == 0x51 || key_code == 0x35 || key_code == 0x3E {
                let val = self.read_byte_direct(table_base + key_code as u32);
                if val == 0xFF {
                    return 0xFFFF;
                }
                (val as u16) << 8
            } else {
                let val = self.read_byte_direct(table_base + key_code as u32);
                if val == 0xFF {
                    return 0xFFFF;
                }
                val as u16 + ((key_code as u16) << 8)
            }
        } else if key_code < 0x60 {
            if key_code == 0x5E { 0xAE00 } else { 0xFFFF }
        } else if (0x62..0x70).contains(&key_code) {
            let val = self.read_byte_direct(table_base + (key_code - 0x0C) as u32);
            if val == 0xFF {
                return 0xFFFF;
            }
            (val as u16) << 8
        } else {
            0xFFFF
        }
    }

    fn hle_int0ch(&mut self, _cpu: &mut impl Cpu) {
        let (data, clear_irq, retrigger_irq) = self.serial.read_data();
        if clear_irq {
            self.pic.clear_irq(4);
        }
        if retrigger_irq {
            self.pic.set_irq(4);
        }
        // Include RS-232C signal line status (CI, CS) from sysport port B.
        let status =
            (self.serial.read_status() & 0xFC) | (self.system_ppi.read_rs232c_status() & 0x03);

        // Read the RS-232C buffer control block pointer from BDA.
        let buf_offset = self.ram_read_u16(0x0556);
        let buf_segment = self.ram_read_u16(0x0558);
        let buf_base = (u32::from(buf_segment) << 4).wrapping_add(u32::from(buf_offset));

        if buf_base != 0 {
            let mut flag = self.read_byte_direct(buf_base + 0x02);
            let mut data = data;

            if flag & 0x40 == 0 {
                // SI/SO character set conversion (JIS 7-bit encoding).
                let rs_s_flag = self.memory.state.ram[0x055B];
                if rs_s_flag & 0x80 != 0 {
                    if data >= 0x20 {
                        if rs_s_flag & 0x10 != 0 {
                            data |= 0x80;
                        } else {
                            data &= 0x7F;
                        }
                    } else if data == 0x0E {
                        // SO: set shift-out flag, send EOI, return without buffering.
                        self.memory.state.ram[0x055B] |= 0x10;
                        self.pic.write_port0(0, 0x20);
                        return;
                    } else if data == 0x0F {
                        // SI: clear shift-out flag, send EOI, return without buffering.
                        self.memory.state.ram[0x055B] &= !0x10;
                        self.pic.write_port0(0, 0x20);
                        return;
                    }
                }

                // DEL code handling: convert DEL to NUL if configured.
                if self.memory.state.ram[0x05C1] & 0x01 != 0
                    && (data & 0x7F) == 0x7F
                    && self.memory.state.text_vram[0x3FEA] & 0x80 != 0
                {
                    data = 0;
                }
                // Buffer not full: store data.
                let r_putp = self.read_word_direct(buf_base + 0x10);
                let r_tailp = self.read_word_direct(buf_base + 0x0C);

                // Store (data << 8) | status at put pointer.
                let entry = u16::from(status) | (u16::from(data) << 8);
                let put_addr = (u32::from(buf_segment) << 4).wrapping_add(u32::from(r_putp));
                self.memory.write_byte(put_addr, entry as u8);
                self.memory.write_byte(put_addr + 1, (entry >> 8) as u8);

                // Advance put pointer with wrap.
                let mut new_putp = r_putp + 2;
                if new_putp >= r_tailp {
                    let r_headp = self.read_word_direct(buf_base + 0x0A);
                    new_putp = r_headp;
                }
                self.memory.write_byte(buf_base + 0x10, new_putp as u8);
                self.memory
                    .write_byte(buf_base + 0x11, (new_putp >> 8) as u8);

                // Increment counter.
                let r_cnt = self.read_word_direct(buf_base + 0x0E);
                let new_cnt = r_cnt + 1;
                self.memory.write_byte(buf_base + 0x0E, new_cnt as u8);
                self.memory
                    .write_byte(buf_base + 0x0F, (new_cnt >> 8) as u8);

                // Check for buffer full (put pointer caught up to get pointer).
                let r_getp = self.read_word_direct(buf_base + 0x12);
                if new_putp == r_getp {
                    flag |= 0x40; // RFLAG_BFULL
                }

                // XON/XOFF flow control: send XOFF if threshold reached.
                // RFLAG_XON=0x10, RFLAG_XOFF=0x08.
                if (flag & 0x18) == 0x10 {
                    let r_xon = self.read_word_direct(buf_base + 0x08);
                    if new_cnt >= r_xon {
                        self.serial.write_data(0x13); // XOFF
                        flag |= 0x08; // RFLAG_XOFF
                    }
                }
            } else {
                // Buffer full: set overflow flag (RFLAG_BOVF=0x20) in R_CMD (offset 0x03).
                let r_cmd = self.read_byte_direct(buf_base + 0x03);
                self.memory.write_byte(buf_base + 0x03, r_cmd | 0x20);
            }

            // Set interrupt flag (RINT_INT=0x80) in R_INT.
            let r_int = self.read_byte_direct(buf_base);
            self.memory.write_byte(buf_base, r_int | 0x80);

            // Write back updated flag.
            self.memory.write_byte(buf_base + 0x02, flag);
        }

        // Send EOI to master PIC.
        self.pic.write_port0(0, 0x20);
    }

    fn fdc_drain_results(fdc: &mut device::upd765a_fdc::Upd765aFdc, dest: &mut [u8]) -> usize {
        let mut count = 0;
        while count < dest.len() {
            let status = fdc.read_status();
            if status & 0xD0 != 0xD0 {
                // Not (RQM | DIO | CB) - no more result bytes.
                break;
            }
            dest[count] = fdc.read_data();
            count += 1;
        }
        count
    }

    fn hle_int12h(&mut self, _cpu: &mut impl Cpu) {
        // ISR-aware EOI: always EOI slave, only EOI master if slave ISR is clear.
        self.pic.write_port0(1, 0x20);
        if self.pic.state.chips[1].isr == 0 {
            self.pic.write_port0(0, 0x20);
        }

        // Loop to drain all pending FDC results.
        loop {
            let fdc = self.floppy.fdc_640k_mut();
            let status = fdc.read_status();

            // If FDC is not busy (CB clear), issue SENSE INTERRUPT STATUS.
            if status & 0x10 == 0 {
                if status & 0xC0 != 0x80 {
                    return;
                }
                fdc.write_data(0x08);
            }

            // Read first result byte (ST0).
            let fdc_status = self.floppy.fdc_640k_mut().read_status();
            if fdc_status & 0xD0 != 0xD0 {
                return;
            }
            let st0 = self.floppy.fdc_640k_mut().read_data();

            if st0 == 0x80 {
                if self.memory.state.ram[0x05D7] > 0 {
                    self.memory.state.ram[0x05D7] -= 1;
                }
                return;
            }

            let drive = (st0 & 0x03) as usize;
            let flag_bit = 0x10u8 << drive;

            let result_base = if st0 & 0xA0 != 0 {
                0x05D8 + drive * 2
            } else {
                0x05D0
            };

            self.memory.state.ram[result_base] = st0;
            let mut buf = [0u8; 7];
            let extra = Self::fdc_drain_results(self.floppy.fdc_640k_mut(), &mut buf);
            self.memory.state.ram[result_base + 1..result_base + 1 + extra]
                .copy_from_slice(&buf[..extra]);

            self.memory.state.ram[0x055F] |= flag_bit;
        }
    }

    fn hle_int13h(&mut self, _cpu: &mut impl Cpu) {
        // ISR-aware EOI: always EOI slave, only EOI master if slave ISR is clear.
        self.pic.write_port0(1, 0x20);
        if self.pic.state.chips[1].isr == 0 {
            self.pic.write_port0(0, 0x20);
        }

        // Loop to drain all pending FDC results.
        loop {
            let fdc = self.floppy.fdc_1mb_mut();
            let status = fdc.read_status();

            if status & 0x10 == 0 {
                if status & 0xC0 != 0x80 {
                    break;
                }
                fdc.write_data(0x08);
            }

            let fdc_status = self.floppy.fdc_1mb_mut().read_status();
            if fdc_status & 0xD0 != 0xD0 {
                break;
            }
            let st0 = self.floppy.fdc_1mb_mut().read_data();

            if st0 == 0x80 {
                break;
            }

            let drive = (st0 & 0x03) as usize;
            let flag_bit = 1u8 << drive;

            let result_base = 0x0564 + drive * 8;
            self.memory.state.ram[result_base] = st0;

            let mut buf = [0u8; 7];
            let extra = Self::fdc_drain_results(self.floppy.fdc_1mb_mut(), &mut buf);
            self.memory.state.ram[result_base + 1..result_base + 1 + extra]
                .copy_from_slice(&buf[..extra]);

            self.memory.state.ram[0x055E] |= flag_bit;
        }

        // Motor-off timer: decrement counter, mark drives for motor-off at zero.
        if self.memory.state.ram[0x0480] & 0x10 != 0 && self.memory.state.ram[0x0485] > 0 {
            self.memory.state.ram[0x0485] -= 1;
            if self.memory.state.ram[0x0485] == 0 {
                self.memory.state.ram[0x05A4] |= 0x0F;
            }
        }
    }

    fn hle_int18h(&mut self, cpu: &mut impl Cpu) {
        match cpu.ah() {
            0x00 => self.int18h_key_read(cpu),
            0x01 => self.int18h_buffer_sense(cpu),
            0x02 => self.int18h_shift_status(cpu),
            0x03 => self.int18h_kb_init(cpu),
            0x04 => self.int18h_key_state_sense(cpu),
            0x05 => self.int18h_key_code_read(cpu),
            0x0A => self.int18h_crt_mode_set(cpu),
            0x0B => self.int18h_crt_mode_sense(cpu),
            0x0C => self.int18h_text_display_start(),
            0x0D => self.int18h_text_display_stop(),
            0x0E => self.int18h_single_display_area(cpu),
            0x0F => self.int18h_multi_display_area(cpu),
            0x10 => self.int18h_cursor_blink(cpu),
            0x11 => self.int18h_cursor_display_start(),
            0x12 => self.int18h_cursor_display_stop(),
            0x13 => self.int18h_cursor_position_set(cpu),
            0x14 => self.int18h_font_pattern_read(cpu),
            0x16 => self.int18h_text_vram_init(cpu),
            0x17 => self.int18h_beep_on(),
            0x18 => self.int18h_beep_off(),
            0x1A => self.int18h_user_char_define(cpu),
            0x1B => self.int18h_kcg_access_mode(cpu),
            0x40 => self.int18h_graphics_display_start(),
            0x41 => self.int18h_graphics_display_stop(),
            0x42 => self.int18h_display_area_set(cpu),
            0x43 => self.int18h_palette_set(cpu),
            0x45 => self.int18h_pattern_fill(cpu),
            0x46 => self.int18h_pattern_read(cpu),
            0x47 | 0x48 => self.int18h_vector_draw(cpu),
            0x49 => self.int18h_graphic_char(cpu),
            0x4A => self.int18h_draw_mode_set(cpu),
            _ => {}
        }
    }

    fn int18h_key_read(&mut self, cpu: &mut impl Cpu) {
        let count = self.memory.state.ram[0x0528];
        if count == 0 {
            // Block until a key is available by rewinding the caller's return IP
            // in the IRET frame to re-execute the INT 18H instruction (2 bytes: CD 18).
            let base = iret_stack_base(cpu);
            let caller_ip = self.read_word_direct(base);
            self.write_mem_word(base, caller_ip.wrapping_sub(2));

            // Ensure IF is set in the IRET flags so hardware interrupts (especially
            // IRQ 1 keyboard) can fire during the wait loop.
            let flags = self.read_word_direct(base + 4);
            self.write_mem_word(base + 4, flags | 0x0200);

            // Burn enough cycles so the timeslice ends and the timer interrupt can fire.
            self.pending_wait_cycles += 2000;
            return;
        }

        let head = self.ram_read_u16(0x0524) as usize;
        let entry = self.ram_read_u16(head);

        // Advance head with wrap.
        let mut new_head = head + 2;
        if new_head >= 0x0522 {
            new_head = 0x0502;
        }
        self.ram_write_u16(0x0524, new_head as u16);
        self.memory.state.ram[0x0528] = count - 1;

        cpu.set_ax(entry);
    }

    fn int18h_buffer_sense(&mut self, cpu: &mut impl Cpu) {
        let count = self.memory.state.ram[0x0528];
        if count == 0 {
            cpu.set_bh(0x00);
            return;
        }

        // Peek at head entry without removing.
        let head = self.ram_read_u16(0x0524) as usize;
        let entry = self.ram_read_u16(head);

        cpu.set_ax(entry);
        cpu.set_bh(0x01);
    }

    fn int18h_shift_status(&mut self, cpu: &mut impl Cpu) {
        let shift = self.memory.state.ram[0x053A];
        cpu.set_al(shift);
    }

    fn int18h_kb_init(&mut self, _cpu: &mut impl Cpu) {
        // Clear keyboard buffer and key status area.
        self.memory.state.ram[0x0502..0x0522].fill(0);
        self.memory.state.ram[0x0528..0x053B].fill(0);
        self.ram_write_u16(0x0522, 0x0B28); // KB_SHIFT_TBL
        self.ram_write_u16(0x0524, 0x0502); // KB_BUF_HEAD
        self.ram_write_u16(0x0526, 0x0502); // KB_BUF_TAIL
        self.ram_write_u16(0x05C6, 0x0B28); // KB_CODE_OFF
        self.ram_write_u16(0x05C8, 0xFD80); // KB_CODE_SEG
    }

    fn int18h_key_state_sense(&mut self, cpu: &mut impl Cpu) {
        let group = cpu.al() as usize;
        let value = if group < 16 {
            self.memory.state.ram[0x052A + group]
        } else {
            0
        };
        cpu.set_ah(value);
    }

    fn int18h_key_code_read(&mut self, cpu: &mut impl Cpu) {
        let count = self.memory.state.ram[0x0528];
        if count == 0 {
            cpu.set_bh(0x00);
            return;
        }

        let head = self.ram_read_u16(0x0524) as usize;
        let entry = self.ram_read_u16(head);

        // Consume from buffer (unlike AH=01h which peeks).
        let mut new_head = head + 2;
        if new_head >= 0x0522 {
            new_head = 0x0502;
        }
        self.ram_write_u16(0x0524, new_head as u16);
        self.memory.state.ram[0x0528] = count - 1;

        cpu.set_ax(entry);
        cpu.set_bh(0x01);
    }

    fn int18h_crt_mode_set(&mut self, cpu: &mut impl Cpu) {
        let mode = cpu.al();

        // Clear mode1 bits: atr_sel(0x01), column_width(0x04), font_sel(0x08), KAC(0x20).
        self.display_control.state.video_mode &= !0x2D;

        // Store mode in CRT_STS_FLAG.
        self.memory.state.ram[0x053C] = mode;

        // 400-line text mode when CRTT is set (DIP SW 1-1 = normal mode).
        // Raster/line parameters: 200-line uses index 0/1, 400-line uses index 2/3.
        //                  raster, pl,   bl,   cl
        // 200-20:          0x09,   0x1F, 0x08, 0x08
        // 200-25:          0x07,   0x00, 0x07, 0x08
        // 400-20:          0x13,   0x1E, 0x11, 0x10
        // 400-25:          0x0F,   0x00, 0x0F, 0x10
        let is_hires = self.system_ppi.state.crtt;
        if is_hires {
            self.memory.state.ram[0x053C] |= 0x80;
            self.display_control.state.video_mode |= 0x08; // font_sel
        }

        if mode & 0x02 != 0 {
            self.display_control.state.video_mode |= 0x04; // column_width (40 columns)
        }
        if mode & 0x04 != 0 {
            self.display_control.state.video_mode |= 0x01; // atr_sel
        }
        if mode & 0x08 != 0 {
            self.display_control.state.video_mode |= 0x20; // KAC dot access mode
        }

        // CRTC parameters: (raster, pl, bl, cl)
        //   200-25: (0x07, 0x00, 0x07, 0x08)
        //   200-20: (0x09, 0x1F, 0x08, 0x08)
        //   400-25: (0x0F, 0x00, 0x0F, 0x10)
        //   400-20: (0x13, 0x1E, 0x11, 0x10)
        let is_20_line = mode & 0x01 != 0;
        let (raster, pl, bl, cl) = match (is_hires, is_20_line) {
            (false, false) => (0x07u8, 0x00u8, 0x07u8, 0x08u8),
            (false, true) => (0x09, 0x1F, 0x08, 0x08),
            (true, false) => (0x0F, 0x00, 0x0F, 0x10),
            (true, true) => (0x13, 0x1E, 0x11, 0x10),
        };
        self.memory.state.ram[0x053B] = raster;

        // Update master GDC text lines_per_row from raster count.
        self.gdc_master.state.lines_per_row = (raster & 0x1F) + 1;

        // Update CRTC registers.
        self.crtc.state.regs[0] = pl;
        self.crtc.state.regs[1] = bl;
        self.crtc.state.regs[2] = cl;
        self.crtc.state.regs[3] = 0; // SSL = 0

        // Reset cursor blink after mode change.
        self.int18h_cursor_blink_inner(0);
    }

    fn int18h_crt_mode_sense(&mut self, cpu: &mut impl Cpu) {
        let mode = self.memory.state.ram[0x053C];
        cpu.set_al(mode);
    }

    fn int18h_text_display_start(&mut self) {
        self.gdc_master.state.display_enabled = true;
    }

    fn int18h_text_display_stop(&mut self) {
        self.gdc_master.state.display_enabled = false;
    }

    fn int18h_single_display_area(&mut self, cpu: &mut impl Cpu) {
        let vram_word_addr = cpu.dx() / 2;
        self.gdc_master.state.scroll[0].start_address = u32::from(vram_word_addr);

        // Set line count: 200 lines * raster, doubled for hi-res.
        let crt_sts_flag = self.memory.state.ram[0x053C];
        let mut raster: u16 = 200 << 4;
        if crt_sts_flag & 0x80 != 0 {
            raster <<= 1;
        }
        self.gdc_master.state.scroll[0].line_count = raster;

        // Update BDA fields.
        self.ram_write_u16(0x0548, vram_word_addr);
        self.ram_write_u16(0x054A, raster);
    }

    fn int18h_multi_display_area(&mut self, cpu: &mut impl Cpu) {
        let table_seg = cpu.bx();
        let mut table_off = cpu.cx();
        let start_area = cpu.dh() as usize;
        let count = cpu.dl() as usize;

        // Store table pointer and area info in BDA.
        self.ram_write_u16(0x053E, table_off);
        self.ram_write_u16(0x0540, table_seg);
        self.memory.state.ram[0x0547] = cpu.dh();
        self.memory.state.ram[0x053D] = cpu.dl();

        let crt_sts_flag = self.memory.state.ram[0x053C];
        let raster: u32 = if crt_sts_flag & 0x01 == 0 {
            // 25-line mode.
            8 << 4
        } else {
            // 20-line mode.
            16 << 4
        };
        let raster = if crt_sts_flag & 0x80 != 0 {
            raster << 1
        } else {
            raster
        };

        let seg = u32::from(table_seg);

        for i in 0..count {
            let area_index = start_area + i;
            if area_index >= 4 {
                break;
            }
            let entry_addr = (seg << 4).wrapping_add(u32::from(table_off));
            let addr_word = self.read_word_direct(entry_addr) >> 1;
            let lines = self.read_word_direct(entry_addr + 2) as u32 * raster;

            self.gdc_master.state.scroll[area_index].start_address = u32::from(addr_word);
            self.gdc_master.state.scroll[area_index].line_count = lines as u16;
            table_off = table_off.wrapping_add(4);
        }
    }

    fn int18h_cursor_blink(&mut self, cpu: &mut impl Cpu) {
        self.int18h_cursor_blink_inner(cpu.al() & 1);
    }

    fn int18h_cursor_blink_inner(&mut self, curdel: u8) {
        let sts = self.memory.state.ram[0x053C];
        self.memory.state.ram[0x053C] = sts & !0x40;

        // Determine cursor form index from mode.
        // 200-25=0, 200-20=1, 400-25=2, 400-20=3
        let mut pos = sts & 0x01;
        if sts & 0x80 != 0 {
            pos += 2;
        }

        self.memory.state.ram[0x053D] = curdel << 5;
        self.gdc_master.state.cursor_blink_rate = curdel << 5;
        self.gdc_master.state.cursor_blink = curdel != 0;

        // Set cursor form: raster and cursor lines per mode.
        let raster = self.memory.state.ram[0x053B];
        self.gdc_master.state.lines_per_row = (raster & 0x1F) + 1;
        let cursor_bottom = [0x07u8, 0x09, 0x0F, 0x13][pos as usize];
        self.gdc_master.state.cursor_top = 0;
        self.gdc_master.state.cursor_bottom = cursor_bottom;
    }

    fn int18h_cursor_display_start(&mut self) {
        self.gdc_master.state.cursor_display = true;
    }

    fn int18h_cursor_display_stop(&mut self) {
        self.gdc_master.state.cursor_display = false;
    }

    fn int18h_cursor_position_set(&mut self, cpu: &mut impl Cpu) {
        let word_addr = cpu.dx() / 2;
        self.gdc_master.state.ead = u32::from(word_addr);
    }

    fn int18h_font_pattern_read(&mut self, cpu: &mut impl Cpu) {
        let char_code = cpu.dx();
        let dest_seg = cpu.bx();
        let dest_off = cpu.cx();
        let dest_base = (u32::from(dest_seg) << 4).wrapping_add(u32::from(dest_off));

        let high_byte = (char_code >> 8) as u8;
        match high_byte {
            0x00 => {
                // 8x8 font, header 0x0101, 8 bytes from fontrom + 0x82000.
                self.write_mem_word(dest_base, 0x0101);
                let font_base = 0x82000 + (char_code as u8 as usize) * 16;
                for i in 0..8 {
                    let byte = self.memory.font_read(font_base + i);
                    self.memory.write_byte(dest_base + 2 + i as u32, byte);
                }
            }
            0x29..=0x2B => {
                // 8x16 half-width kanji subset, header 0x0102.
                self.write_mem_word(dest_base, 0x0102);
                let font_offset = cgrom_kanji_offset(high_byte, char_code as u8) as usize;
                for i in 0..16 {
                    let byte = self.memory.font_read(font_offset + i);
                    self.memory.write_byte(dest_base + 2 + i as u32, byte);
                }
            }
            0x80 => {
                // 8x16 ANK, header 0x0102, 16 bytes from fontrom + 0x80000.
                self.write_mem_word(dest_base, 0x0102);
                let font_base = 0x80000 + (char_code as u8 as usize) * 16;
                for i in 0..16 {
                    let byte = self.memory.font_read(font_base + i);
                    self.memory.write_byte(dest_base + 2 + i as u32, byte);
                }
            }
            _ => {
                // 16x16 kanji, header 0x0202.
                self.write_mem_word(dest_base, 0x0202);
                let jis_col = char_code as u8;
                let font_offset = cgrom_kanji_offset(high_byte, jis_col) as usize;
                for i in 0..16 {
                    let left = self.memory.font_read(font_offset + i);
                    let right = self.memory.font_read(font_offset + 0x800 + i);
                    self.memory.write_byte(dest_base + 2 + (i as u32) * 2, left);
                    self.memory
                        .write_byte(dest_base + 2 + (i as u32) * 2 + 1, right);
                }
            }
        }
    }

    fn int18h_text_vram_init(&mut self, cpu: &mut impl Cpu) {
        let char_byte = cpu.dl();
        let attr_byte = cpu.dh();

        // Fill character plane with char_byte at even offsets.
        for i in (0..0x2000).step_by(2) {
            self.memory.state.text_vram[i] = char_byte;
            self.memory.state.text_vram[i + 1] = 0x00;
        }
        // Fill attribute plane with attr_byte at even offsets (up to 0x3FE0).
        for i in (0x2000..0x3FE0).step_by(2) {
            self.memory.state.text_vram[i] = attr_byte;
        }
    }

    fn int18h_beep_on(&mut self) {
        self.beeper.state.buzzer_enabled = true;
    }

    fn int18h_beep_off(&mut self) {
        self.beeper.state.buzzer_enabled = false;
    }

    fn int18h_user_char_define(&mut self, cpu: &mut impl Cpu) {
        let char_code = cpu.dx();
        let jis_row = (char_code >> 8) as u8;
        let jis_col = char_code as u8;

        // Only rows 0x76/0x77 are user-definable.
        if (jis_row & 0x7E) != 0x76 {
            return;
        }

        let src_seg = cpu.bx();
        let src_off = cpu.cx();
        let src_base = (u32::from(src_seg) << 4).wrapping_add(u32::from(src_off));

        // Skip 2-byte size header, read 32 bytes of interleaved font data.
        // Input format: [left0, right0, left1, right1, ..., left15, right15].
        let font_offset = cgrom_kanji_offset(jis_row, jis_col) as usize;

        for i in 0..16 {
            let left = self.read_byte_direct(src_base + 2 + (i as u32) * 2);
            let right = self.read_byte_direct(src_base + 2 + (i as u32) * 2 + 1);
            self.memory.font_write(font_offset + i, left);
            self.memory.font_write(font_offset + 0x800 + i, right);
        }
    }

    fn int18h_kcg_access_mode(&mut self, cpu: &mut impl Cpu) {
        match cpu.al() {
            0 => {
                self.memory.state.ram[0x053C] &= !0x08;
                self.display_control.state.video_mode &= !0x20; // code access mode
            }
            1 => {
                self.memory.state.ram[0x053C] |= 0x08;
                self.display_control.state.video_mode |= 0x20; // dot access mode
            }
            _ => {}
        }
    }

    fn int18h_graphics_display_start(&mut self) {
        self.gdc_slave.state.display_enabled = true;
        self.memory.state.ram[0x054C] |= 0x80;
    }

    fn int18h_graphics_display_stop(&mut self) {
        self.gdc_slave.state.display_enabled = false;
        self.memory.state.ram[0x054C] &= 0x7F;
    }

    fn int18h_display_area_set(&mut self, cpu: &mut impl Cpu) {
        const GDC_SLAVE_SYNC: [[u8; 8]; 6] = [
            [0x02, 0x26, 0x03, 0x11, 0x86, 0x0F, 0xC8, 0x94], // 15-L
            [0x02, 0x4E, 0x4B, 0x0C, 0x83, 0x06, 0xE0, 0x95], // 31-H
            [0x02, 0x26, 0x03, 0x11, 0x83, 0x07, 0x90, 0x65], // 24-L
            [0x02, 0x4E, 0x07, 0x25, 0x87, 0x07, 0x90, 0x65], // 24-M
            [0x02, 0x26, 0x41, 0x0C, 0x83, 0x0D, 0x90, 0x89], // 31-L
            [0x02, 0x4E, 0x47, 0x0C, 0x87, 0x0D, 0x90, 0x89], // 31-M
        ];

        let mode = cpu.ch();
        let modenum = [3u8, 1, 0, 2];
        let crtmode = modenum[(mode >> 6) as usize];

        // Zero scroll parameters for slave GDC (first partition).
        // Line count 0 maps to 1024 in the µPD7220 (10-bit wrap).
        self.gdc_slave.state.scroll[0] = GdcScrollPartition {
            start_address: 0x0000,
            line_count: 0x400,
            im: false,
            wd: false,
        };

        let prxdupd = self.memory.state.ram[0x054D];
        if crtmode == 2 {
            // 400-line ALL mode.
            if (prxdupd & 0x24) == 0x20 {
                self.memory.state.ram[0x054D] ^= 4;
                self.gdc_slave.load_sync_params(&GDC_SLAVE_SYNC[3]);
                self.gdc_slave.state.pitch = 80;
                self.memory.state.ram[0x054D] |= 0x08;
                self.display_control.state.mode2 |= 0x0600;
                self.apply_gdc_dot_clock();
            }
        } else {
            if (prxdupd & 0x24) == 0x24 {
                self.memory.state.ram[0x054D] ^= 4;
                // Select sync table based on PRXCRT bit 6.
                let sync_idx = if self.memory.state.ram[0x054C] & 0x40 != 0 {
                    2
                } else {
                    0
                };
                self.gdc_slave.load_sync_params(&GDC_SLAVE_SYNC[sync_idx]);
                self.gdc_slave.state.pitch = 40;
                self.memory.state.ram[0x054D] |= 0x08;
                self.display_control.state.mode2 &= !0x0600;
                self.apply_gdc_dot_clock();
            }
            if crtmode & 1 != 0 {
                // UPPER: set scroll start to page 1.
                self.gdc_slave.state.scroll[0].start_address = 200 * 40;
            }
        }

        if self.memory.state.ram[0x054D] & 4 != 0 {
            self.gdc_slave.state.scroll[0].line_count = 0x400;
        }

        // Determine 400-line vs 200-line display mode.
        let prxcrt = self.memory.state.ram[0x054C];
        if crtmode == 2 || (prxcrt & 0x40) == 0 {
            // 400-line mode: clear hide_odd_rasters, lines_per_row = 1.
            self.display_control.state.video_mode &= !0x10;
            self.gdc_slave.state.lines_per_row = 1;
        } else {
            // 200-line mode: set hide_odd_rasters, lines_per_row = 2.
            self.display_control.state.video_mode |= 0x10;
            self.gdc_slave.state.lines_per_row = 2;
        }

        // Display page selection.
        if crtmode != 3 {
            self.display_control.state.display_page = (mode >> 4) & 1;
        }

        // Graphics mode and EGC extended mode.
        if mode & 0x20 == 0 {
            self.display_control.state.video_mode &= !0x02;
            self.display_control.state.mode2 &= !0x0004;
        } else {
            self.display_control.state.video_mode |= 0x02;
            self.display_control.state.mode2 |= 0x0004;
        }

        // Store crtmode in CRT_BIOS (0x0597).
        self.memory.state.ram[0x0597] = (self.memory.state.ram[0x0597] & 0xFC) | (crtmode & 0x03);
    }

    fn int18h_palette_set(&mut self, cpu: &mut impl Cpu) {
        let src_seg = cpu.ds();
        let src_off = cpu.bx();
        let src_base = (u32::from(src_seg) << 4).wrapping_add(u32::from(src_off));

        // Read 4 GBCPC bytes and write to digital palette ports.
        // NP21W computes degpal values then writes to ports. Port 0xA8..0xAE
        // map to digital[0..3], with reversed indexing vs NP21W's degpal.
        let mut col = [0u8; 4];
        for (i, col_byte) in col.iter_mut().enumerate() {
            *col_byte = self.read_byte_direct(src_base + 4 + i as u32);
        }
        self.palette.state.digital[0] = ((col[2] & 0x0F) << 4) | (col[0] & 0x0F);
        self.palette.state.digital[1] = ((col[3] & 0x0F) << 4) | (col[1] & 0x0F);
        self.palette.state.digital[2] = (col[2] & 0xF0) | (col[0] >> 4);
        self.palette.state.digital[3] = (col[3] & 0xF0) | (col[1] >> 4);
    }

    fn int18h_pattern_fill(&mut self, cpu: &mut impl Cpu) {
        let src_seg = cpu.ds();
        let src_off = cpu.bx();
        let ucw_base = (u32::from(src_seg) << 4).wrapping_add(u32::from(src_off));
        let ch = cpu.ch();

        let gbon_ptn = self.read_byte_direct(ucw_base); // GBON_PTN
        let gbdotu = self.read_byte_direct(ucw_base + 2); // GBDOTU
        let x = self.read_word_direct(ucw_base + 0x08) as u32; // GBSX1
        let mut y = self.read_word_direct(ucw_base + 0x0A) as u32; // GBSY1
        let length = self.read_word_direct(ucw_base + 0x0C) as u32; // GBLNG1
        let pat_addr = self.read_word_direct(ucw_base + 0x0E); // GBWDPA

        // 200-line page offset.
        if (ch & 0xC0) == 0x40 {
            y += 200;
        }

        let all_planes = (ch & 0x30) == 0x30;
        let ds = u32::from(src_seg);

        let mut i = 0u32;
        loop {
            let pat_byte = self.read_byte_direct((ds << 4).wrapping_add(u32::from(pat_addr) + i));
            let remaining = length - i * 8;
            let bits = if remaining < 8 { remaining } else { 8 };
            let mask = if bits < 8 {
                0xFF_u8 << (8 - bits)
            } else {
                0xFF
            };
            let pat = reverse_bits(pat_byte & mask);

            let px = x + i * 8;
            let word_addr = (y * 40 + (px >> 4)) as usize;
            let bit_offset = px & 0x0F;

            if all_planes {
                for plane in 0..3u8 {
                    let ope = if gbon_ptn & (1 << plane) != 0 { 0 } else { 1 };
                    let plane_base = (plane as usize) * 0x4000;
                    self.gdc_pset_byte(plane_base + word_addr, bit_offset, pat, ope);
                }
            } else {
                let ope = gbdotu & 3;
                let plane_sel = ((ch & 0x30) >> 4) as usize;
                let plane_base = plane_sel * 0x4000;
                self.gdc_pset_byte(plane_base + word_addr, bit_offset, pat, ope);
            }

            i += 1;
            if i * 8 >= length {
                break;
            }
        }

        // Save operation mode in PRXDUPD bits 0-1.
        let ope = gbdotu & 3;
        self.memory.state.ram[0x054D] = (self.memory.state.ram[0x054D] & !0x03) | ope;
    }

    fn gdc_pset_byte(&mut self, word_addr: usize, bit_offset: u32, pat: u8, ope: u8) {
        for bit in 0..8u32 {
            let pixel_bit = (pat >> (7 - bit)) & 1;
            let target_bit = bit_offset + bit;
            let addr = word_addr + (target_bit >> 4) as usize;
            let bit_in_word = (target_bit & 0x0F) as u8;

            if addr >= 0x4000 {
                continue;
            }
            let byte_idx = addr * 2 + (bit_in_word >> 3) as usize;
            let bit_mask = 0x80 >> (bit_in_word & 7);

            if byte_idx >= self.memory.state.graphics_vram.len() {
                continue;
            }

            match ope {
                // SET: set bit if pattern bit is 1.
                0 if pixel_bit != 0 => {
                    self.memory.state.graphics_vram[byte_idx] |= bit_mask;
                }
                // CLEAR: clear bit if pattern bit is 1.
                1 if pixel_bit != 0 => {
                    self.memory.state.graphics_vram[byte_idx] &= !bit_mask;
                }
                // COMPLEMENT: toggle bit if pattern bit is 1.
                2 if pixel_bit != 0 => {
                    self.memory.state.graphics_vram[byte_idx] ^= bit_mask;
                }
                _ => {}
            }
        }
    }

    fn int18h_pattern_read(&mut self, cpu: &mut impl Cpu) {
        // Read pattern from graphics VRAM into ES:0 output buffer.
        let ucw_seg = cpu.ds();
        let ucw_off = cpu.bx();
        let ucw_base = (u32::from(ucw_seg) << 4).wrapping_add(u32::from(ucw_off));
        let out_base = u32::from(cpu.es()) << 4;

        let x = self.read_word_direct(ucw_base + 0x08); // GBSX1
        let y = self.read_word_direct(ucw_base + 0x0A); // GBSY1
        let lines = self.read_word_direct(ucw_base + 0x0C); // GBLNG1

        let pitch_bytes = 80u16;
        let word_x = x / 16;
        let mut out_offset = 0u32;

        for dy in 0..lines {
            let row = y + dy;
            if row >= 400 {
                break;
            }
            let byte_offset = u32::from(row) * u32::from(pitch_bytes) + u32::from(word_x) * 2;
            // Read from B-plane (offset 0x0000 in graphics VRAM).
            let b_lo = self.memory.state.graphics_vram[byte_offset as usize];
            let b_hi = self.memory.state.graphics_vram[byte_offset as usize + 1];
            self.memory.write_byte(out_base + out_offset, b_lo);
            self.memory.write_byte(out_base + out_offset + 1, b_hi);
            out_offset += 2;
        }
    }

    fn int18h_vector_draw(&mut self, cpu: &mut impl Cpu) {
        let src_seg = cpu.ds();
        let src_off = cpu.bx();
        let ucw_base = (u32::from(src_seg) << 4).wrapping_add(u32::from(src_off));
        let ch = cpu.ch();

        let gbon_ptn = self.read_byte_direct(ucw_base); // GBON_PTN
        let gbdotu = self.read_byte_direct(ucw_base + 2); // GBDOTU
        let x1 = self.read_word_direct(ucw_base + 0x08) as i32; // GBSX1
        let mut y1 = self.read_word_direct(ucw_base + 0x0A) as i32; // GBSY1
        let x2 = self.read_word_direct(ucw_base + 0x16) as i32; // GBSX2
        let mut y2 = self.read_word_direct(ucw_base + 0x18) as i32; // GBSY2
        let gbdtyp = self.read_byte_direct(ucw_base + 0x28); // GBDTYP

        if (ch & 0xC0) == 0x40 {
            y1 += 200;
            y2 += 200;
        }

        // Line style pattern from GBMDOTI (bit-reversed).
        let pat_hi = reverse_bits(self.read_byte_direct(ucw_base + 0x20));
        let pat_lo = reverse_bits(self.read_byte_direct(ucw_base + 0x21));
        let pattern = ((pat_hi as u16) << 8) | pat_lo as u16;

        let ope = gbdotu & 3;

        match gbdtyp {
            0x01 => self.draw_line(x1, y1, x2, y2, pattern, gbon_ptn, ope, ch),
            0x00 | 0x02 => self.draw_rect(x1, y1, x2, y2, pattern, gbon_ptn, ope, ch),
            _ => {
                let radius = self.read_word_direct(ucw_base + 0x1C) as i32; // GBCIR
                self.draw_circle(x1, y1, radius, pattern, gbon_ptn, ope, ch);
            }
        }

        // Save operation mode in PRXDUPD bits 0-1.
        self.memory.state.ram[0x054D] = (self.memory.state.ram[0x054D] & !0x03) | ope;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_line(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        pattern: u16,
        gbon_ptn: u8,
        ope: u8,
        ch: u8,
    ) {
        let dx = (x2 - x1).abs();
        let dy = -(y2 - y1).abs();
        let sx: i32 = if x1 < x2 { 1 } else { -1 };
        let sy: i32 = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x1;
        let mut y = y1;
        let mut pat_idx = 0u32;

        loop {
            if (0..640).contains(&x) && (0..400).contains(&y) {
                let bit_in_pattern = (pattern >> (15 - (pat_idx & 15))) & 1;
                if bit_in_pattern != 0 {
                    self.plot_pixel_ope(x as u16, y as u16, gbon_ptn, ope, ch);
                }
            }
            pat_idx += 1;

            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_rect(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        pattern: u16,
        gbon_ptn: u8,
        ope: u8,
        ch: u8,
    ) {
        self.draw_line(x1, y1, x2, y1, pattern, gbon_ptn, ope, ch);
        self.draw_line(x2, y1, x2, y2, pattern, gbon_ptn, ope, ch);
        self.draw_line(x2, y2, x1, y2, pattern, gbon_ptn, ope, ch);
        self.draw_line(x1, y2, x1, y1, pattern, gbon_ptn, ope, ch);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_circle(
        &mut self,
        cx: i32,
        cy: i32,
        radius: i32,
        pattern: u16,
        gbon_ptn: u8,
        ope: u8,
        ch: u8,
    ) {
        let mut x = radius;
        let mut y_pos = 0i32;
        let mut d = 1 - radius;
        let mut pat_idx = 0u32;

        while x >= y_pos {
            let points = [
                (cx + x, cy + y_pos),
                (cx - x, cy + y_pos),
                (cx + x, cy - y_pos),
                (cx - x, cy - y_pos),
                (cx + y_pos, cy + x),
                (cx - y_pos, cy + x),
                (cx + y_pos, cy - x),
                (cx - y_pos, cy - x),
            ];

            for &(px, py) in &points {
                if (0..640).contains(&px) && (0..400).contains(&py) {
                    let bit_in_pattern = (pattern >> (15 - (pat_idx & 15))) & 1;
                    if bit_in_pattern != 0 {
                        self.plot_pixel_ope(px as u16, py as u16, gbon_ptn, ope, ch);
                    }
                }
            }
            pat_idx += 1;

            y_pos += 1;
            if d <= 0 {
                d += 2 * y_pos + 1;
            } else {
                x -= 1;
                d += 2 * (y_pos - x) + 1;
            }
        }
    }

    fn int18h_graphic_char(&mut self, cpu: &mut impl Cpu) {
        let src_seg = cpu.ds();
        let src_off = cpu.bx();
        let ucw_base = (u32::from(src_seg) << 4).wrapping_add(u32::from(src_off));
        let ch = cpu.ch();

        let gbon_ptn = self.read_byte_direct(ucw_base); // GBON_PTN
        let gbdotu = self.read_byte_direct(ucw_base + 2); // GBDOTU
        let x = self.read_word_direct(ucw_base + 0x08) as u32; // GBSX1
        let mut y = self.read_word_direct(ucw_base + 0x0A) as u32; // GBSY1
        let gblng1 = self.read_word_direct(ucw_base + 0x0C); // GBLNG1
        let gblng2 = self.read_word_direct(ucw_base + 0x1E); // GBLNG2

        if (ch & 0xC0) == 0x40 {
            y += 200;
        }

        // Read 8 bytes of GBMDOTI pattern and bit-reverse each.
        let mut pat = [0u8; 8];
        for (i, pat_byte) in pat.iter_mut().enumerate() {
            *pat_byte = reverse_bits(self.read_byte_direct(ucw_base + 0x20 + i as u32));
        }

        // Height (DC+1 scan lines) and width from GBLNG1/GBLNG2.
        let height = if gblng1 != 0 { gblng2 } else { 8 } as u32;

        let all_planes = (ch & 0x30) == 0x30;

        for dy in 0..height {
            let row = y + dy;
            if row >= 400 {
                break;
            }
            let pat_byte = pat[(dy & 7) as usize];
            let word_addr = (row * 40 + (x >> 4)) as usize;
            let bit_offset = x & 0x0F;

            if all_planes {
                for plane in 0..3u8 {
                    let ope = if gbon_ptn & (1 << plane) != 0 { 0 } else { 1 };
                    let plane_base = (plane as usize) * 0x4000;
                    self.gdc_pset_byte(plane_base + word_addr, bit_offset, pat_byte, ope);
                }
            } else {
                let ope = gbdotu & 3;
                let plane_sel = ((ch & 0x30) >> 4) as usize;
                let plane_base = plane_sel * 0x4000;
                self.gdc_pset_byte(plane_base + word_addr, bit_offset, pat_byte, ope);
            }
        }
    }

    fn int18h_draw_mode_set(&mut self, cpu: &mut impl Cpu) {
        if self.memory.state.ram[0x054C] & 0x01 != 0 {
            return;
        }
        let mode = cpu.ch();
        self.gdc_slave.set_sync_mode_byte(mode);
        if mode & 0x10 != 0 {
            self.memory.state.ram[0x054D] &= !0x08;
        } else {
            self.memory.state.ram[0x054D] |= 0x08;
        }
    }

    fn plot_pixel_ope(&mut self, x: u16, y: u16, gbon_ptn: u8, ope: u8, ch: u8) {
        let word_x = x / 16;
        let bit = 15 - (x & 15);
        let byte_offset = u32::from(y) * 80 + u32::from(word_x) * 2;

        let (mask, byte_idx) = if bit >= 8 {
            (1u8 << (bit - 8), 1usize)
        } else {
            (1u8 << bit, 0usize)
        };

        let all_planes = (ch & 0x30) == 0x30;

        if all_planes {
            for plane in 0..3u8 {
                let plane_ope = if gbon_ptn & (1 << plane) != 0 { 0 } else { 1 };
                let idx = (plane as usize) * 0x8000 + byte_offset as usize + byte_idx;
                if idx < self.memory.state.graphics_vram.len() {
                    match plane_ope {
                        0 => self.memory.state.graphics_vram[idx] |= mask,
                        1 => self.memory.state.graphics_vram[idx] &= !mask,
                        2 => self.memory.state.graphics_vram[idx] ^= mask,
                        _ => {}
                    }
                }
            }
        } else {
            let plane_sel = ((ch & 0x30) >> 4) as usize;
            let idx = plane_sel * 0x8000 + byte_offset as usize + byte_idx;
            if idx < self.memory.state.graphics_vram.len() {
                match ope {
                    0 => self.memory.state.graphics_vram[idx] |= mask,
                    1 => self.memory.state.graphics_vram[idx] &= !mask,
                    2 => self.memory.state.graphics_vram[idx] ^= mask,
                    _ => {}
                }
            }
        }
    }

    fn hle_int19h(&mut self, cpu: &mut impl Cpu) {
        match cpu.ah() {
            0x00 | 0x01 => {
                if cpu.ah() == 0x01 {
                    self.int19h_init_flow(cpu);
                } else {
                    self.int19h_init(cpu);
                }
            }
            0x02..=0x06 => {
                if !self.int19h_is_initialized() {
                    cpu.set_ah(0x01);
                    return;
                }
                match cpu.ah() {
                    0x02 => self.int19h_rx_count(cpu),
                    0x03 => self.int19h_send(cpu),
                    0x04 => {
                        if self.int19h_receive(cpu) {
                            return;
                        }
                    }
                    0x05 => self.int19h_cmd_output(cpu),
                    0x06 => self.int19h_status(cpu),
                    _ => unreachable!(),
                }
                // BOVF check at function exit: if buffer overflow occurred,
                // clear the flag and return AH=2.
                let buf_base = self.int19h_get_buf_base();
                let flag = self.read_byte_direct(buf_base + 0x02);
                if flag & 0x20 != 0 {
                    self.memory.write_byte(buf_base + 0x02, flag & !0x20);
                    cpu.set_ah(0x02);
                }
            }
            _ => {}
        }
    }

    fn int19h_get_buf_base(&self) -> u32 {
        let offset = self.ram_read_u16(0x0556);
        let segment = self.ram_read_u16(0x0558);
        (u32::from(segment) << 4).wrapping_add(u32::from(offset))
    }

    fn int19h_is_initialized(&self) -> bool {
        let buf_base = self.int19h_get_buf_base();
        if buf_base == 0 {
            return false;
        }
        let flag = self.read_byte_direct(buf_base + 0x02);
        flag & 0x80 != 0
    }

    fn int19h_init(&mut self, cpu: &mut impl Cpu) {
        let buf_seg = cpu.es();
        let buf_off = cpu.di();
        let buf_base = (u32::from(buf_seg) << 4).wrapping_add(u32::from(buf_off));
        let buf_size = cpu.dx();

        // Program PIT channel 2 for RS-232C baud rate generation.
        #[rustfmt::skip]
        const RS_SPEED: [u16; 20] = [
            // 8MHz lineage (PIT clock ≈ 1.9968 MHz)
            0x0680, 0x0340, 0x01A0, 0x00D0,
            0x0068, 0x0034, 0x001A, 0x000D,
            // 10MHz lineage (PIT clock ≈ 2.4576 MHz)
            0x0800, 0x0400, 0x0200, 0x0100,
            0x0080, 0x0040, 0x0020, 0x0010,
            0x0008, 0x0004, 0x0002, 0x0001,
        ];
        let mut speed = cpu.al();
        if speed >= 8 {
            speed = 4; // Default to 1200 bps.
        }
        let is_8mhz = self.memory.state.ram[0x0501] & 0x80 != 0;
        let speed_idx = if is_8mhz {
            speed as usize
        } else {
            speed as usize + 8
        };
        let divisor = RS_SPEED[speed_idx];
        self.pit.write_control(
            2,
            0xB6,
            self.current_cycle,
            self.clocks.cpu_clock_hz,
            self.clocks.pit_clock_hz,
        );
        self.pit.write_counter(2, divisor as u8);
        self.pit.write_counter(2, (divisor >> 8) as u8);

        // Store buffer pointer in BDA.
        self.ram_write_u16(0x0556, buf_off);
        self.ram_write_u16(0x0558, buf_seg);

        // Initialize control block fields.
        let data_start = buf_off + 0x14; // After RSBIOS header
        let data_end = data_start + buf_size;

        // R_INT (offset 0x00) and R_BFLG (offset 0x01): clear stale state.
        self.memory.write_byte(buf_base, 0x00);
        self.memory.write_byte(buf_base + 0x01, 0x00);

        // R_FLAG (offset 0x02): set AH<<4, only add RFLAG_INIT if IR bit clear.
        let cmd = cpu.cl();
        let mut flag = cpu.ah() << 4;
        // Clear sysport RS-232C interrupt source bits (bits 0-2).
        self.system_ppi.state.port_c &= !0x07;
        if cmd & 0x40 == 0 {
            // IR bit clear: mark initialized.
            flag |= 0x80;
            if cmd & 0x04 != 0 {
                // RXE set: enable RxRDY interrupt via sysport and unmask IRQ 4.
                self.system_ppi.state.port_c |= 0x01;
                self.pic.state.chips[0].imr &= !0x10;
                self.pic.invalidate_irq_cache();
            }
        }
        self.memory.write_byte(buf_base + 0x02, flag);

        // R_CMD (offset 0x03).
        self.memory.write_byte(buf_base + 0x03, cmd);
        // R_STIME (offset 0x04) = BH, default 0x04 if zero.
        let stime = if cpu.bh() == 0 { 0x04 } else { cpu.bh() };
        self.memory.write_byte(buf_base + 0x04, stime);
        // R_RTIME (offset 0x05) = BL, default 0x40 if zero.
        let rtime = if cpu.bx() as u8 == 0 {
            0x40
        } else {
            cpu.bx() as u8
        };
        self.memory.write_byte(buf_base + 0x05, rtime);

        // R_XOFF (offset 0x06) = buf_size / 8.
        let xoff = buf_size / 8;
        self.memory.write_byte(buf_base + 0x06, xoff as u8);
        self.memory.write_byte(buf_base + 0x07, (xoff >> 8) as u8);
        // R_XON (offset 0x08) = XOFF + buf_size / 4.
        let xon = xoff + buf_size / 4;
        self.memory.write_byte(buf_base + 0x08, xon as u8);
        self.memory.write_byte(buf_base + 0x09, (xon >> 8) as u8);

        // R_HEADP (offset 0x0A).
        self.memory.write_byte(buf_base + 0x0A, data_start as u8);
        self.memory
            .write_byte(buf_base + 0x0B, (data_start >> 8) as u8);
        // R_TAILP (offset 0x0C).
        self.memory.write_byte(buf_base + 0x0C, data_end as u8);
        self.memory
            .write_byte(buf_base + 0x0D, (data_end >> 8) as u8);
        // R_CNT (offset 0x0E).
        self.memory.write_byte(buf_base + 0x0E, 0x00);
        self.memory.write_byte(buf_base + 0x0F, 0x00);
        // R_PUTP (offset 0x10).
        self.memory.write_byte(buf_base + 0x10, data_start as u8);
        self.memory
            .write_byte(buf_base + 0x11, (data_start >> 8) as u8);
        // R_GETP (offset 0x12).
        self.memory.write_byte(buf_base + 0x12, data_start as u8);
        self.memory
            .write_byte(buf_base + 0x13, (data_start >> 8) as u8);

        // Program the serial UART command register.
        self.serial.write_command(cmd);

        cpu.set_ah(0x00);
    }

    fn int19h_init_flow(&mut self, cpu: &mut impl Cpu) {
        // Same as AH=00h but with flow control flag.
        self.int19h_init(cpu);

        let buf_base = self.int19h_get_buf_base();
        // Set RFLAG_XON bit (bit 4).
        let flag = self.read_byte_direct(buf_base + 0x02);
        self.memory.write_byte(buf_base + 0x02, flag | 0x10);
    }

    fn int19h_rx_count(&mut self, cpu: &mut impl Cpu) {
        let buf_base = self.int19h_get_buf_base();
        let count = self.read_word_direct(buf_base + 0x0E);
        cpu.set_cx(count);
        cpu.set_ah(0x00);
    }

    fn int19h_send(&mut self, cpu: &mut impl Cpu) {
        // Write data byte to serial data port (port 0x30).
        self.serial.write_data(cpu.al());
        cpu.set_ah(0x00);
    }

    fn int19h_receive(&mut self, cpu: &mut impl Cpu) -> bool {
        let buf_base = self.int19h_get_buf_base();
        let buf_seg = self.ram_read_u16(0x0558);
        let count = self.read_word_direct(buf_base + 0x0E);

        if count == 0 {
            cpu.set_ah(0x03);
            return false;
        }

        let r_getp = self.read_word_direct(buf_base + 0x12);
        let get_addr = (u32::from(buf_seg) << 4).wrapping_add(u32::from(r_getp));
        let entry = self.read_word_direct(get_addr);

        // Advance get pointer with wrap.
        let r_tailp = self.read_word_direct(buf_base + 0x0C);
        let r_headp = self.read_word_direct(buf_base + 0x0A);
        let mut new_getp = r_getp + 2;
        if new_getp >= r_tailp {
            new_getp = r_headp;
        }

        let new_count = count - 1;
        self.memory.write_byte(buf_base + 0x0E, new_count as u8);
        self.memory
            .write_byte(buf_base + 0x0F, (new_count >> 8) as u8);
        self.memory.write_byte(buf_base + 0x12, new_getp as u8);
        self.memory
            .write_byte(buf_base + 0x13, (new_getp >> 8) as u8);

        // XON flow control: send XON if XOFF was active and count dropped below threshold.
        let flag = self.read_byte_direct(buf_base + 0x02);
        if (flag & 0x08) != 0 && new_count < self.read_word_direct(buf_base + 0x06) {
            self.serial.write_data(0x11); // XON
            self.memory
                .write_byte(buf_base + 0x02, (flag & !0x08) & !0x20);
        } else {
            self.memory.write_byte(buf_base + 0x02, flag & !0x20);
        }

        // Return full entry word in CX (CL=status/error, CH=data).
        cpu.set_cx(entry);
        cpu.set_ah(0x00);
        true
    }

    fn int19h_cmd_output(&mut self, cpu: &mut impl Cpu) {
        let buf_base = self.int19h_get_buf_base();
        let cmd = cpu.al();

        self.serial.write_command(cmd);

        let flag = self.read_byte_direct(buf_base + 0x02);
        if cmd & 0x40 != 0 {
            // IR (internal reset): clear INIT flag, disable RxRDY, mask IRQ 4.
            self.memory.write_byte(buf_base + 0x02, flag & !0x80);
            self.system_ppi.state.port_c &= !0x01;
            self.pic.state.chips[0].imr |= 0x10;
        } else if cmd & 0x04 == 0 {
            // RXE clear: disable RxRDY, mask IRQ 4.
            self.system_ppi.state.port_c &= !0x01;
            self.pic.state.chips[0].imr |= 0x10;
        } else {
            // RXE set: enable RxRDY, unmask IRQ 4.
            self.system_ppi.state.port_c |= 0x01;
            self.pic.state.chips[0].imr &= !0x10;
        }
        self.pic.invalidate_irq_cache();

        self.memory.write_byte(buf_base + 0x03, cmd);
        cpu.set_ah(0x00);
    }

    fn int19h_status(&mut self, cpu: &mut impl Cpu) {
        cpu.set_ch(self.serial.read_status());
        cpu.set_cl(self.system_ppi.read_rs232c_status() | self.rtc.cdat());
        cpu.set_ah(0x00);
    }

    fn hle_int1ah(&mut self, cpu: &mut impl Cpu) {
        let result_ah = match cpu.ah() {
            // CMT functions (0x00–0x05): stubs.
            // VM/VX have no CMT hardware - AH=04h returns 0x00, AH=05h returns 0x27.
            // RA returns 0x02 for both (unsupported device).
            0x00 | 0x01 => 0x00,
            0x02 | 0x03 => 0x00,
            0x04 if self.clocks.pit_clock_hz == PIT_CLOCK_8MHZ_LINEAGE => 0x02,
            0x04 => 0x00,
            0x05 if self.clocks.pit_clock_hz == PIT_CLOCK_8MHZ_LINEAGE => 0x02,
            0x05 => 0x27,
            // Printer functions.
            0x10 => {
                self.system_ppi.write_control(0x0D);
                self.printer.write_control(0x82);
                self.printer.write_control(0x0F);
                self.system_ppi.write_control(0x0C);
                u8::from(self.printer.is_ready())
            }
            0x11 if self.printer.is_ready() => {
                self.printer.write_data(cpu.al());
                let old_c = self.printer.read_port_c();
                self.printer.write_port_c(old_c | 0x80);
                self.printer.write_port_c(old_c & !0x80);
                0x01
            }
            0x11 => 0x00,
            0x12 if self.printer.is_ready() => 0x01,
            0x12 => 0x00,
            0x30 => {
                let count = cpu.cx();
                if count == 0 {
                    0x00
                } else {
                    let src_seg = cpu.es();
                    let mut src_off = cpu.bx();
                    let src_base = (u32::from(src_seg) << 4).wrapping_add(u32::from(src_off));
                    let mut remaining = count;
                    for i in 0..count {
                        if !self.printer.is_ready() {
                            cpu.set_ah(0x00);
                            cpu.set_cx(remaining);
                            cpu.set_bx(src_off);
                            return;
                        }
                        let byte = self.read_byte_direct(src_base + u32::from(i));
                        self.printer.write_data(byte);
                        let old_c = self.printer.read_port_c();
                        self.printer.write_port_c(old_c | 0x80);
                        self.printer.write_port_c(old_c & !0x80);
                        src_off = src_off.wrapping_add(1);
                        remaining -= 1;
                    }
                    cpu.set_cx(0x0000);
                    cpu.set_bx(src_off);
                    0x00
                }
            }
            _ => 0x00,
        };

        cpu.set_ah(result_ah);
    }

    fn hle_int1bh(&mut self, cpu: &mut impl Cpu) {
        let function = cpu.ah() & 0x0F;
        let da = cpu.al();
        let devtype = da & 0xF0;

        match devtype {
            // SASI/IDE: DA high nibble 0x80 or 0x00.
            0x80 | 0x00 => self.int1bh_hdd(cpu, function),
            // FDD: all known DA high nibbles for 1MB, 640KB, and 320KB FDCs.
            0x90 | 0x10 | 0x30 | 0xB0 | 0x70 | 0xF0 | 0x50 => self.int1bh_fdd(cpu, function),
            _ => self.write_result_ah_cf(cpu, 0x40),
        }
    }

    fn int1bh_fdd(&mut self, cpu: &mut impl Cpu, function: u8) {
        let drive = (cpu.al() & 0x03) as usize;
        let ah = cpu.ah();
        self.tracer.trace_int1bh_fdd_params(
            cpu.ah(),
            cpu.al(),
            cpu.cl(),
            cpu.dh(),
            cpu.dl(),
            cpu.ch(),
        );

        match function {
            0x00 => {
                if ah & 0x10 != 0 {
                    self.fdd_seek_cylinder[drive] = cpu.cl();
                }
                self.write_result_ah_cf(cpu, 0x00);
            }
            0x03 => {
                // Initialize FDD: update DISK_EQUIP.
                let da = cpu.al();
                let devtype = da & 0xF0;
                let is_1mb = matches!(devtype, 0x90 | 0x30 | 0xB0 | 0x10);
                let equip = if is_1mb {
                    u16::from(self.floppy.fdc_1mb().state.drive_equipped & 0x0F)
                } else {
                    u16::from(self.floppy.fdc_640k().state.drive_equipped & 0x0F)
                };
                let mut disk_equip = self.ram_read_u16(0x055C);
                if is_1mb {
                    disk_equip = (disk_equip & 0xFFF0) | equip;
                } else {
                    disk_equip = (disk_equip & 0x0FFF) | (equip << 12);
                }
                self.ram_write_u16(0x055C, disk_equip);
                self.write_result_ah_cf(cpu, 0x00);
            }
            0x04 => {
                // Sense: return drive status.
                if !self.floppy.has_drive(drive) {
                    self.write_result_ah_cf(cpu, 0x60);
                } else {
                    let mut result = 0x00u8;
                    if self.floppy.is_write_protected(drive) {
                        result |= 0x10;
                    }
                    // Bit 0 = disk present.
                    result |= 0x01;
                    // Report dual-mode drive (1MB/640KB) for extended sense.
                    if (cpu.ax() & 0x8F40) == 0x8400 {
                        result |= 0x08;
                    }
                    self.write_result_ah_cf(cpu, result);
                }
            }
            0x05 => {
                // Write sectors.
                let c = cpu.cl();
                let h = cpu.dh();
                let r = cpu.dl();
                let n_val = cpu.ch();
                let sector_size = 128usize << n_val;
                let buf_seg = cpu.es();
                let buf_off = cpu.bp();
                let buf_addr = (u32::from(buf_seg) << 4).wrapping_add(u32::from(buf_off));
                let transfer_bytes = cpu.bx() as usize;
                let size = if transfer_bytes > 0 {
                    transfer_bytes
                } else {
                    sector_size
                };
                let sector_count = size / sector_size;
                if !self.floppy.has_drive(drive) {
                    self.tracer.trace_int1bh_fdd_write(
                        drive,
                        c,
                        h,
                        r,
                        n_val,
                        sector_count,
                        buf_addr,
                        0x60,
                    );
                    self.write_result_ah_cf(cpu, 0x60);
                    return;
                }
                if self.floppy.is_write_protected(drive) {
                    self.tracer.trace_int1bh_fdd_write(
                        drive,
                        c,
                        h,
                        r,
                        n_val,
                        sector_count,
                        buf_addr,
                        0x70,
                    );
                    self.write_result_ah_cf(cpu, 0x70);
                    return;
                }
                let mut h = h;
                let mut hd = (h ^ (cpu.al() >> 2)) & 1;
                // Segment wrap check.
                if (buf_addr & 0xFFFF) > ((buf_addr + size as u32 - 1) & 0xFFFF) {
                    self.tracer.trace_int1bh_fdd_write(
                        drive,
                        c,
                        h,
                        r,
                        n_val,
                        sector_count,
                        buf_addr,
                        0x20,
                    );
                    self.write_result_ah_cf(cpu, 0x20);
                    return;
                }
                if ah & 0x10 != 0 {
                    self.fdd_seek_cylinder[drive] = c;
                }
                let multi_track = ah & 0x80 != 0;
                let mut track_index = (self.fdd_seek_cylinder[drive] as usize) * 2 + hd as usize;

                let mut offset = 0u32;
                let mut current_r = r;
                for _ in 0..sector_count {
                    let mut data = vec![0u8; sector_size];
                    for (j, data_byte) in data.iter_mut().enumerate() {
                        *data_byte = self.read_byte_direct(buf_addr + offset + j as u32);
                    }
                    if !self.floppy.write_sector_data(
                        drive,
                        track_index,
                        c,
                        h,
                        current_r,
                        n_val,
                        &data,
                    ) {
                        if multi_track && hd == 0 {
                            hd = 1;
                            h = 1;
                            track_index = (self.fdd_seek_cylinder[drive] as usize) * 2 + 1;
                            current_r = 1;
                            if !self.floppy.write_sector_data(
                                drive,
                                track_index,
                                c,
                                h,
                                current_r,
                                n_val,
                                &data,
                            ) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    offset += sector_size as u32;
                    current_r += 1;
                }
                // If no sector was written at all, the starting sector
                // didn't exist - return 0xE0. Otherwise the FDC reached
                // EOT and reports success for whatever was transferred.
                let result = if offset == 0 { 0xE0 } else { 0x00 };
                self.tracer.trace_int1bh_fdd_write(
                    drive,
                    c,
                    h,
                    r,
                    n_val,
                    sector_count,
                    buf_addr,
                    result,
                );
                self.write_result_ah_cf(cpu, result);
            }
            0x02 | 0x06 => {
                // Read sectors (0x06 = normal read, 0x02 = diagnostic read).
                if !self.floppy.has_drive(drive) {
                    self.write_result_ah_cf(cpu, 0x60);
                    return;
                }
                let is_diagnostic = function == 0x02;
                let c = cpu.cl();
                let mut h = cpu.dh();
                let mut hd = (h ^ (cpu.al() >> 2)) & 1;
                let r = cpu.dl();
                let n_val = cpu.ch();
                let sector_size = 128usize << n_val;
                let buf_seg = cpu.es();
                let buf_off = cpu.bp();
                let buf_addr = (u32::from(buf_seg) << 4).wrapping_add(u32::from(buf_off));
                let transfer_bytes = cpu.bx() as usize;
                let size = if transfer_bytes > 0 {
                    transfer_bytes
                } else {
                    sector_size
                };
                // Segment wrap check.
                if (buf_addr & 0xFFFF) > ((buf_addr + size as u32 - 1) & 0xFFFF) {
                    let err = if is_diagnostic { 0x00 } else { 0x20 };
                    self.write_result_ah_cf(cpu, err);
                    return;
                }
                if ah & 0x10 != 0 {
                    self.fdd_seek_cylinder[drive] = c;
                }
                let sector_count = size / sector_size;
                let multi_track = ah & 0x80 != 0;
                let mut track_index = (self.fdd_seek_cylinder[drive] as usize) * 2 + hd as usize;

                let mut offset = 0u32;
                let mut current_r = r;
                for _ in 0..sector_count {
                    if let Some(data) =
                        self.floppy
                            .read_sector_data(drive, track_index, c, h, current_r, n_val)
                    {
                        if !is_diagnostic {
                            for (j, &byte) in data.iter().enumerate() {
                                self.memory.write_byte(buf_addr + offset + j as u32, byte);
                            }
                        }
                        offset += data.len() as u32;
                    } else if multi_track && hd == 0 {
                        // Head switch: try head 1 on same cylinder.
                        hd = 1;
                        h = 1;
                        track_index = (self.fdd_seek_cylinder[drive] as usize) * 2 + 1;
                        current_r = 1;
                        if let Some(data) =
                            self.floppy
                                .read_sector_data(drive, track_index, c, h, current_r, n_val)
                        {
                            if !is_diagnostic {
                                for (j, &byte) in data.iter().enumerate() {
                                    self.memory.write_byte(buf_addr + offset + j as u32, byte);
                                }
                            }
                            offset += data.len() as u32;
                        } else {
                            self.tracer.trace_int1bh_fdd_read(
                                drive,
                                c,
                                h,
                                current_r,
                                n_val,
                                sector_count,
                                buf_addr,
                                0xE0,
                            );
                            // Diagnostic read returns 0x00 on error.
                            let err = if is_diagnostic { 0x00 } else { 0xE0 };
                            self.write_result_ah_cf(cpu, err);
                            return;
                        }
                    } else {
                        self.tracer.trace_int1bh_fdd_read(
                            drive,
                            c,
                            h,
                            current_r,
                            n_val,
                            sector_count,
                            buf_addr,
                            0xE0,
                        );
                        let err = if is_diagnostic { 0x00 } else { 0xE0 };
                        self.write_result_ah_cf(cpu, err);
                        return;
                    }
                    current_r += 1;
                }
                self.tracer.trace_int1bh_fdd_read(
                    drive,
                    c,
                    h,
                    r,
                    n_val,
                    sector_count,
                    buf_addr,
                    0x00,
                );
                self.write_result_ah_cf(cpu, 0x00);
            }
            0x07 => {
                // Recalibrate: no-op (always succeeds).
                self.write_result_ah_cf(cpu, 0x00);
            }
            0x0A => {
                // Read ID: return geometry of first sector on current track.
                if !self.floppy.has_drive(drive) {
                    self.write_result_ah_cf(cpu, 0x60);
                } else {
                    let c = cpu.cl();
                    let h = cpu.dh();
                    let track_index = c as usize * 2 + h as usize;
                    if let Some(disk) = self.floppy.drive(drive)
                        && let Some(sector) = disk.sector_at_index(track_index, 0)
                    {
                        cpu.set_cl(sector.cylinder);
                        cpu.set_dh(sector.head);
                        cpu.set_dl(sector.record);
                        cpu.set_ch(sector.size_code);
                        self.write_result_ah_cf(cpu, 0x00);
                    } else {
                        self.write_result_ah_cf(cpu, 0xE0);
                    }
                }
            }
            0x01 => {
                // Verify: no-op.
                if !self.floppy.has_drive(drive) {
                    self.write_result_ah_cf(cpu, 0x60);
                    return;
                }
                self.write_result_ah_cf(cpu, 0x00);
            }
            0x0D => {
                // Format track.
                if !self.floppy.has_drive(drive) {
                    self.write_result_ah_cf(cpu, 0x60);
                    return;
                }
                if self.floppy.is_write_protected(drive) {
                    self.write_result_ah_cf(cpu, 0x70);
                    return;
                }
                if ah & 0x10 != 0 {
                    self.fdd_seek_cylinder[drive] = cpu.cl();
                }
                let h = cpu.dh();
                let hd = (h ^ (cpu.al() >> 2)) & 1;
                let n_val = cpu.ch();
                let fill_byte = cpu.dl();
                let buf_seg = cpu.es();
                let buf_off = cpu.bp();
                let buf_addr = (u32::from(buf_seg) << 4).wrapping_add(u32::from(buf_off));
                let buf_size = cpu.bx() as usize;
                let sector_count = buf_size / 4;
                let track_index = (self.fdd_seek_cylinder[drive] as usize) * 2 + hd as usize;

                let mut chrn = Vec::with_capacity(sector_count);
                for i in 0..sector_count {
                    let base = buf_addr + (i as u32) * 4;
                    let c = self.read_byte_direct(base);
                    let h = self.read_byte_direct(base + 1);
                    let r = self.read_byte_direct(base + 2);
                    let n = self.read_byte_direct(base + 3);
                    chrn.push((c, h, r, n));
                }
                self.floppy
                    .format_track(drive, track_index, &chrn, n_val, fill_byte);
                self.write_result_ah_cf(cpu, 0x00);
            }
            0x0E => {
                // Set density.
                self.write_result_ah_cf(cpu, 0x00);
            }
            _ => {
                self.write_result_ah_cf(cpu, 0x40);
            }
        }
    }

    fn int1bh_hdd(&mut self, cpu: &mut impl Cpu, function: u8) {
        if self.machine_model.has_ide() {
            self.int1bh_ide(cpu, function);
        } else {
            self.int1bh_sasi(cpu, function);
        }
    }

    fn int1bh_sasi(&mut self, cpu: &mut impl Cpu, function: u8) {
        let ax = cpu.ax();
        let bx = cpu.bx();
        let cx = cpu.cx();
        let dx = cpu.dx();
        let bp = cpu.bp();
        let es = cpu.es();

        let function_code = (ax >> 8) as u8;
        let drive_select = ax as u8;
        let drive_idx = device::sasi::drive_index(drive_select);

        let result_ah = match function {
            0x03 => {
                let current_lo = self.read_byte_direct(0x055C);
                let current_hi = self.read_byte_direct(0x055D);
                let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                let disk_equip = self.sasi.execute_init(current_equip);
                self.memory.write_byte(0x055C, disk_equip as u8);
                self.memory.write_byte(0x055D, (disk_equip >> 8) as u8);
                self.pic.state.chips[0].imr &= !0x01;
                self.pic.invalidate_irq_cache();
                0x00
            }
            0x04 => match function_code {
                0x84 => {
                    let sense_result = self.sasi.execute_sense(drive_idx);
                    if sense_result >= 0x20 {
                        sense_result
                    } else {
                        if let Some(geometry) = self.sasi.drive_geometry(drive_idx) {
                            cpu.set_bx(geometry.sector_size);
                            cpu.set_cx(geometry.cylinders.saturating_sub(1));
                            let dx_value = ((u16::from(geometry.heads)) << 8)
                                | u16::from(geometry.sectors_per_track);
                            cpu.set_dx(dx_value);
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
                let addr = self.hle_linear_address(cpu, common::SegmentRegister::ES, u32::from(bp));
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &self.memory;
                self.sasi.execute_write(drive_idx, xfer, pos, addr, |a| {
                    let phys = hle_page_translate(cr0, cr3, a, memory);
                    memory.read_byte(phys)
                })
            }
            0x06 => {
                let xfer = device::sasi::transfer_size(bx);
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = self.hle_linear_address(cpu, common::SegmentRegister::ES, u32::from(bp));
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &mut self.memory;
                self.sasi
                    .execute_read(drive_idx, xfer, pos, addr, |a, byte| {
                        let phys = hle_page_translate(cr0, cr3, a, memory);
                        memory.write_byte(phys, byte);
                    })
            }
            0x07 | 0x0F => 0x00,
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
            0x01 => 0x00,
            _ => 0x40,
        };

        self.tracer
            .trace_sasi_hle(function_code, drive_select, result_ah, bx, cx, dx, es, bp);

        self.write_result_ah_cf(cpu, result_ah);
    }

    fn int1bh_ide(&mut self, cpu: &mut impl Cpu, function: u8) {
        let ax = cpu.ax();
        let bx = cpu.bx();
        let cx = cpu.cx();
        let dx = cpu.dx();
        let bp = cpu.bp();
        let es = cpu.es();

        let function_code = (ax >> 8) as u8;
        let drive_select = ax as u8;
        let drive_idx = device::ide::drive_index(drive_select);
        // In compatibility mode the CD-ROM is always at unit 1 (DA/UA=0x81).
        // If unit 1 has no HDD but a CD-ROM is present, treat it as the CD-ROM unit.
        let is_cdrom_unit = drive_idx == 1 && self.ide.has_cdrom() && !self.ide.has_hdd(1);

        // IDE-specific extensions use the full unmasked function code.
        let result_ah = match function_code {
            0xD0 => self.ide.execute_check_power_mode(drive_idx),
            0xE0 => self.ide.execute_motor_on(drive_idx),
            0xF0 => self.ide.execute_motor_off(drive_idx),
            _ => match function {
                0x03 => {
                    let current_lo = self.read_byte_direct(0x055C);
                    let current_hi = self.read_byte_direct(0x055D);
                    let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                    let disk_equip = self.ide.execute_init(current_equip);
                    self.memory.write_byte(0x055C, disk_equip as u8);
                    self.memory.write_byte(0x055D, (disk_equip >> 8) as u8);
                    self.pic.state.chips[0].imr &= !0x01;
                    self.pic.invalidate_irq_cache();
                    0x00
                }
                0x04 => match function_code {
                    0x84 => {
                        if is_cdrom_unit {
                            self.ide.execute_cdrom_sense()
                        } else {
                            let sense_result = self.ide.execute_sense(drive_idx);
                            if sense_result >= 0x20 {
                                sense_result
                            } else {
                                if let Some(geometry) = self.ide.drive_geometry(drive_idx) {
                                    cpu.set_bx(geometry.sector_size);
                                    cpu.set_cx(geometry.cylinders.saturating_sub(1));
                                    let dx_value = ((u16::from(geometry.heads)) << 8)
                                        | u16::from(geometry.sectors_per_track);
                                    cpu.set_dx(dx_value);
                                }
                                sense_result
                            }
                        }
                    }
                    _ => {
                        if is_cdrom_unit {
                            self.ide.execute_cdrom_sense()
                        } else {
                            self.ide.execute_sense(drive_idx)
                        }
                    }
                },
                0x05 => {
                    let xfer = device::ide::transfer_size(bx);
                    let geometry = self.ide.drive_geometry(drive_idx);
                    let pos = geometry
                        .map(|g| device::ide::sector_position(drive_select, cx, dx, &g))
                        .unwrap_or(0);
                    let addr =
                        self.hle_linear_address(cpu, common::SegmentRegister::ES, u32::from(bp));
                    let cr0 = self.hle_cr0;
                    let cr3 = self.hle_cr3;
                    let memory = &self.memory;
                    self.ide.execute_write(drive_idx, xfer, pos, addr, |a| {
                        let phys = hle_page_translate(cr0, cr3, a, memory);
                        memory.read_byte(phys)
                    })
                }
                0x06 => {
                    let xfer = device::ide::transfer_size(bx);
                    let geometry = self.ide.drive_geometry(drive_idx);
                    let pos = geometry
                        .map(|g| device::ide::sector_position(drive_select, cx, dx, &g))
                        .unwrap_or(0);
                    let addr =
                        self.hle_linear_address(cpu, common::SegmentRegister::ES, u32::from(bp));
                    let cr0 = self.hle_cr0;
                    let cr3 = self.hle_cr3;
                    let memory = &mut self.memory;
                    self.ide
                        .execute_read(drive_idx, xfer, pos, addr, |a, byte| {
                            let phys = hle_page_translate(cr0, cr3, a, memory);
                            memory.write_byte(phys, byte);
                        })
                }
                0x07 | 0x0F => 0x00,
                0x0D => {
                    let geometry = self.ide.drive_geometry(drive_idx);
                    let pos = geometry
                        .map(|g| device::ide::sector_position(drive_select, cx, dx, &g))
                        .unwrap_or(0);
                    self.ide.execute_format(drive_idx, pos)
                }
                0x0E => self.ide.execute_mode_set(drive_idx),
                0x01 => 0x00,
                _ => 0x40,
            },
        };

        self.tracer
            .trace_sasi_hle(function_code, drive_select, result_ah, bx, cx, dx, es, bp);

        self.write_result_ah_cf(cpu, result_ah);
    }

    fn hle_int1ch(&mut self, cpu: &mut impl Cpu) {
        match cpu.ah() {
            0x00 => self.int1ch_get_datetime(cpu),
            0x01 => self.int1ch_set_datetime(cpu),
            0x02 => self.int1ch_set_interval_timer(cpu),
            0x03 => self.int1ch_continue_interval_timer(),
            _ => {}
        }
    }

    fn int1ch_get_datetime(&mut self, cpu: &mut impl Cpu) {
        let time = (self.host_local_time_fn)();
        for (i, &byte) in time.iter().enumerate() {
            self.hle_write_byte(
                cpu,
                common::SegmentRegister::ES,
                u32::from(cpu.bx()) + i as u32,
                byte,
            );
        }
    }

    fn int1ch_set_datetime(&mut self, cpu: &mut impl Cpu) {
        let mut buf = [0u8; 6];
        for (i, buf_byte) in buf.iter_mut().enumerate() {
            *buf_byte = self.hle_read_byte(
                cpu,
                common::SegmentRegister::ES,
                u32::from(cpu.bx()) + i as u32,
            );
        }

        // Store year in Memory Switch 8 (text VRAM offset 0x3FFE).
        self.memory.state.text_vram[0x3FFE] = buf[0];

        // Write the 6-byte BCD time into the RTC register.
        self.rtc.state.reg[2..8].copy_from_slice(&buf);
    }

    fn int1ch_set_interval_timer(&mut self, cpu: &mut impl Cpu) {
        let callback_seg = cpu.es();
        let callback_off = cpu.bx();
        let count = cpu.cx();

        // Store callback address in IVT vector 0x07 (at 0x001C).
        self.ram_write_u16(0x001C, callback_off);
        self.ram_write_u16(0x001E, callback_seg);

        // Store counter at CA_TIM_CNT (0x058A).
        self.ram_write_u16(0x058A, count);
        self.bios_interval_timer_active = true;

        // Program PIT channel 0 for 10ms interval timer.
        let divider: u16 = if self.clocks.pit_clock_hz == PIT_CLOCK_8MHZ_LINEAGE {
            0x4E00
        } else {
            0x6000
        };

        self.pit.write_control(
            0,
            0x36,
            self.current_cycle,
            self.clocks.cpu_clock_hz,
            self.clocks.pit_clock_hz,
        );
        self.pit.write_counter(0, divider as u8);
        self.pit.write_counter(0, (divider >> 8) as u8);
        self.pit.state.channels[0].last_load_cycle = self.current_cycle;
        self.pic.clear_irq(0);
        self.pit.state.channels[0].flag |= PIT_FLAG_I;
        self.pit.schedule_timer0(
            &mut self.scheduler,
            self.clocks.cpu_clock_hz,
            self.clocks.pit_clock_hz,
            self.current_cycle,
        );
        self.update_next_event_cycle();

        // Unmask IRQ 0 (system timer) in master PIC.
        self.pic.state.chips[0].imr &= !0x01;
        self.pic.invalidate_irq_cache();
    }

    fn int1ch_continue_interval_timer(&mut self) {
        // Reload PIT ch0 counter and unmask IRQ 0.
        // The real BIOS calls this on every non-expired INT 08H tick.
        let divider: u16 = if self.clocks.pit_clock_hz == PIT_CLOCK_8MHZ_LINEAGE {
            0x4E00
        } else {
            0x6000
        };
        self.pit.write_counter(0, divider as u8);
        self.pit.write_counter(0, (divider >> 8) as u8);
        self.pit.state.channels[0].last_load_cycle = self.current_cycle;
        self.pic.clear_irq(0);
        self.pit.state.channels[0].flag |= PIT_FLAG_I;
        self.pit.schedule_timer0(
            &mut self.scheduler,
            self.clocks.cpu_clock_hz,
            self.clocks.pit_clock_hz,
            self.current_cycle,
        );
        self.update_next_event_cycle();
        self.pic.state.chips[0].imr &= !0x01;
        self.pic.invalidate_irq_cache();
    }

    fn hle_int1fh(&mut self, cpu: &mut impl Cpu) {
        let ah = cpu.ah();

        // Bit 7 must be set for valid extended functions.
        // On VM (expansion ROM handler), CF is cleared even for AH < 0x80.
        // On VX/RA, the handler returns without modifying flags.
        if ah & 0x80 == 0 {
            if self.machine_model == MachineModel::PC9801VM {
                self.set_iret_cf(cpu, false);
            }
            return;
        }

        match ah {
            0xCC => {
                // PCI BIOS check: no PCI on VM-era machines. Set CF = 1.
                self.set_iret_cf(cpu, true);
            }
            0x90 => {
                // Protected-mode memory copy (286/386 only).
                if self.machine_model == MachineModel::PC9801VM {
                    self.set_iret_cf(cpu, false);
                    return;
                }

                let desc_seg = cpu.es();
                let desc_off = cpu.bx();
                let desc_base = (u32::from(desc_seg) << 4).wrapping_add(u32::from(desc_off));

                // Read source descriptor (at desc_base + 0x10).
                let src_base_addr = self.parse_gdt_descriptor_base(desc_base + 0x10);

                // Read destination descriptor (at desc_base + 0x18).
                let dst_base_addr = self.parse_gdt_descriptor_base(desc_base + 0x18);

                let mut src_off = u32::from(cpu.si());
                let mut dst_off = u32::from(cpu.di());

                // CX=0 means 65536 bytes.
                let remaining = if cpu.cx() == 0 {
                    0x10000u32
                } else {
                    cpu.cx() as u32
                };

                // The real BIOS enters protected mode (which ignores A20)
                // and disables A20 when returning to real mode.
                self.a20_enabled = true;
                for _ in 0..remaining {
                    let byte = self.read_byte_direct(src_base_addr.wrapping_add(src_off));
                    self.memory
                        .write_byte(dst_base_addr.wrapping_add(dst_off), byte);
                    src_off = (src_off + 1) & 0xFFFF;
                    dst_off = (dst_off + 1) & 0xFFFF;
                }
                self.a20_enabled = false;

                self.set_iret_cf(cpu, false);
            }
            _ => {
                // AH with bit 4 clear (0x80-0x8F excl. handled above): clear CF.
                // AH with bit 4 set (0x91-0x9F, 0xB0-0xBF, etc.): leave CF unchanged.
                if ah & 0x10 == 0 {
                    self.set_iret_cf(cpu, false);
                }
            }
        }
    }

    fn parse_gdt_descriptor_base(&self, addr: u32) -> u32 {
        let base_lo = self.read_byte_direct(addr + 2) as u32;
        let base_mid = self.read_byte_direct(addr + 3) as u32;
        let base_hi = self.read_byte_direct(addr + 4) as u32;
        base_lo | (base_mid << 8) | (base_hi << 16)
    }

    fn try_boot_fdd(&mut self, cpu: &mut impl Cpu, drive: usize) -> bool {
        if self.floppy.has_drive(drive)
            && let Some(n) = self.floppy.boot_sector_size_code(drive)
            && let Some(data) = self.floppy.read_sector_data(drive, 0, 0, 0, 1, n)
        {
            let boot_data: Vec<u8> = data.to_vec();
            let is_2hd = self
                .floppy
                .drive(drive)
                .is_some_and(|d| d.media_type == D88MediaType::Disk2HD);
            let da_base: usize = if is_2hd { 0x90 } else { 0x70 };
            if !is_2hd {
                let equipped = self.floppy.fdc_1mb().state.drive_equipped & 0x0F;
                self.memory.state.ram[0x055C] &= !equipped;
                self.memory.state.ram[0x055D] |= equipped << 4;
                self.memory.state.ram[0x0494] = (equipped & 0x03) << 6;
            }
            if self.try_boot_from_data(cpu, &boot_data, (da_base | drive) as u8) {
                return true;
            }
        }
        false
    }

    fn try_boot_from_data(
        &mut self,
        cpu: &mut impl Cpu,
        boot_data: &[u8],
        boot_device: u8,
    ) -> bool {
        if !boot_data.iter().any(|&b| b != 0) {
            return false;
        }
        for (i, &byte) in boot_data.iter().enumerate() {
            self.memory.write_byte(0x1FC00 + i as u32, byte);
        }
        self.memory.state.ram[0x0584] = boot_device;
        let iret_base = iret_stack_base(cpu);
        self.write_mem_word(iret_base, 0x0000); // IP
        self.write_mem_word(iret_base + 2, 0x1FC0); // CS
        self.write_mem_word(iret_base + 4, 0x0002); // FLAGS (reserved bit 1 set)
        true
    }

    fn hle_bootstrap(&mut self, cpu: &mut impl Cpu) {
        // The HLE dispatch uses IRET to transfer control to the boot sector.
        // We rewrite the IRET frame (6 bytes: IP, CS, FLAGS) at the current
        // SP to redirect execution. SP is left at its current value (set to
        // 0x7C00 by the ITF entry stub). After IRET, SP becomes SP+6.
        //
        // The stack must NOT be placed near 0x0600 because boot sectors
        // commonly load program data to segment 0060:0000 (linear 0x0600),
        // which would overwrite return addresses on the stack.

        // Initialize SASI drives before boot attempt (equivalent of INT 1Bh AH=03).
        for hdd in 0..2usize {
            if self.sasi.drive_geometry(hdd).is_some() {
                let current_lo = self.memory.state.ram[0x055C];
                let current_hi = self.memory.state.ram[0x055D];
                let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                let disk_equip = self.sasi.execute_init(current_equip);
                self.memory.state.ram[0x055C] = disk_equip as u8;
                self.memory.state.ram[0x055D] = (disk_equip >> 8) as u8;
                break;
            }
        }

        // Initialize IDE drives before boot attempt (equivalent of INT 1Bh AH=03).
        for hdd in 0..2usize {
            if self.ide.drive_geometry(hdd).is_some() {
                let current_lo = self.memory.state.ram[0x055C];
                let current_hi = self.memory.state.ram[0x055D];
                let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                let disk_equip = self.ide.execute_init(current_equip);
                self.memory.state.ram[0x055C] = disk_equip as u8;
                self.memory.state.ram[0x055D] = (disk_equip >> 8) as u8;
                break;
            }
        }

        // Set IDE device connection flags and BIOS work area.
        if self.machine_model.has_ide() {
            let hdd_flags = self.ide.compute_connection_flags();
            // 0x05BA: HDD-only connection flags (compatibility mode - no CD-ROM bit).
            self.memory.state.ram[0x05BA] = hdd_flags;
            // 0x05BB: same HDD-only backup used by the WinNT4.0 workaround.
            self.memory.state.ram[0x05BB] = hdd_flags;
            // ROM 0xF8E80+0x10 (offset 0x10E90): all connected device bits including CD-ROM.
            // Bit 2 set when CD-ROM present on channel 1 master - read by NECCD.SYS to detect the drive.
            let all_device_flags = hdd_flags | if self.ide.has_cdrom() { 0x04 } else { 0x00 };
            self.memory.set_rom_byte(0x10E90, all_device_flags);
            // ROM 0xF8E80+0x11 (offset 0x10E91): clear bit 7 for 17KB NECCDD.SYS compatibility.
            let flags_byte = self.memory.rom_byte(0x10E91);
            self.memory.set_rom_byte(0x10E91, flags_byte & !0x80);
        }

        // Install IDE expansion ROM's IRQ 9 handler and presence markers.
        // The real BIOS scans expansion ROMs and calls their init code.
        // Our ide.rom's init entry installs the IRQ handler at D800:008D
        // and sets ROM presence markers at 0x04B0/0x04B8.
        if self.ide.rom_installed() {
            // IVT[0x11] = D800:008D (IDE IRQ 9 handler in expansion ROM)
            self.memory.state.ram[0x0044] = 0x8D; // offset low
            self.memory.state.ram[0x0045] = 0x00; // offset high
            self.memory.state.ram[0x0046] = 0x00; // segment low (D800)
            self.memory.state.ram[0x0047] = 0xD8; // segment high
            // ROM presence markers (segment >> 8)
            self.memory.state.ram[0x04B0] = 0xD8;
            self.memory.state.ram[0x04B8] = 0xD8;
            // Unmask IRQ 9 (slave PIC IR1) so IDE/ATAPI interrupts can fire.
            self.pic.state.chips[1].imr &= !0x02;
            self.pic.invalidate_irq_cache();
        }

        // Boot device selection.
        match self.boot_device {
            BootDevice::Auto => {
                // FDD 0-1 -> CD-ROM (IDE only) -> SASI HDD 0-1 -> IDE HDD 0-1.
                for drive in 0..4usize {
                    if self.try_boot_fdd(cpu, drive) {
                        return;
                    }
                }
                if self.machine_model.has_ide()
                    && let Some(boot_data) = self.ide.read_cdrom_boot_sector()
                    && boot_sector_has_signature(&boot_data)
                    && self.try_boot_from_data(cpu, &boot_data, 0x82)
                {
                    let lo = self.memory.state.ram[0x055C];
                    let hi = self.memory.state.ram[0x055D];
                    let disk_equip = u16::from(lo) | (u16::from(hi) << 8) | 0x0400;
                    self.memory.state.ram[0x055C] = disk_equip as u8;
                    self.memory.state.ram[0x055D] = (disk_equip >> 8) as u8;
                    return;
                }
                for hdd in 0..2usize {
                    if let Some(data) = self.sasi.read_boot_sector(hdd)
                        && self.try_boot_from_data(cpu, &data, 0x80 | hdd as u8)
                    {
                        return;
                    }
                }
                for hdd in 0..2usize {
                    if let Some(data) = self.ide.read_boot_sector(hdd)
                        && self.try_boot_from_data(cpu, &data, 0x80 | hdd as u8)
                    {
                        return;
                    }
                }
            }
            BootDevice::Fdd1 => {
                if self.try_boot_fdd(cpu, 0) {
                    return;
                }
            }
            BootDevice::Fdd2 => {
                if self.try_boot_fdd(cpu, 1) {
                    return;
                }
            }
            BootDevice::Hdd1 => {
                if let Some(data) = self.sasi.read_boot_sector(0)
                    && self.try_boot_from_data(cpu, &data, 0x80)
                {
                    return;
                }
                if let Some(data) = self.ide.read_boot_sector(0)
                    && self.try_boot_from_data(cpu, &data, 0x80)
                {
                    return;
                }
            }
            BootDevice::Hdd2 => {
                if let Some(data) = self.sasi.read_boot_sector(1)
                    && self.try_boot_from_data(cpu, &data, 0x81)
                {
                    return;
                }
                if let Some(data) = self.ide.read_boot_sector(1)
                    && self.try_boot_from_data(cpu, &data, 0x81)
                {
                    return;
                }
            }
            BootDevice::Os => {}
        }

        // No bootable device found (or Os selected): activate NEETAN OS HLE DOS.
        let mut neetan_os = os::NeetanOs::new();
        neetan_os.set_host_local_time_fn(self.host_local_time_fn);
        neetan_os.set_ems_enabled(self.ems_enabled);
        neetan_os.set_xms_enabled(self.xms_enabled);
        neetan_os.set_xms_32_enabled(self.xms_32_enabled);

        {
            let mut cpu_access = OsCpuAccess(cpu);
            let mut mem_access = OsMemoryAccess(&mut self.memory);
            let mut disk_io = OsDiskIo {
                floppy: &mut self.floppy,
                sasi: &mut self.sasi,
                ide: &mut self.ide,
            };
            neetan_os.boot(
                &mut cpu_access,
                &mut mem_access,
                &mut disk_io,
                &mut self.tracer,
            );
        }

        // Enable GDC hardware cursor for HLE OS.
        self.gdc_master.state.cursor_display = true;

        // Redirect IRET to COMMAND.COM's entry point (PSP:0100h).
        // The stub at PSP:0100h sets up its own stack inside COMMAND.COM's
        // MCB allocation before entering the shell loop, so child program
        // allocations cannot overwrite the parent's IRET frame.
        let psp_segment = neetan_os.command_com_psp();
        self.os = Some(neetan_os);
        let iret_base = iret_stack_base(cpu);
        self.write_mem_word(iret_base, 0x0100); // IP = entry point
        self.write_mem_word(iret_base + 2, psp_segment); // CS = PSP segment
        self.write_mem_word(iret_base + 4, 0x0202); // FLAGS (IF set)
    }
}

fn boot_sector_has_signature(data: &[u8]) -> bool {
    data.len() >= 0x400 && data[0x3FE] == 0x55 && data[0x3FF] == 0xAA
}

/// Computes the CGROM byte offset for a Kanji character given JIS row/col.
///
/// The font ROM uses an interleaved layout: each JIS column occupies a 4096-byte block,
/// with rows packed at 16-byte intervals within. Left half at the computed offset,
/// right half at offset + 0x800.
fn cgrom_kanji_offset(jis_row: u8, jis_col: u8) -> u32 {
    let col = (jis_col & 0x7F) as u32;
    let row = (jis_row.wrapping_sub(0x20) & 0x7F) as u32;
    col * 0x1000 + row * 16
}

fn reverse_bits(b: u8) -> u8 {
    let mut v = b;
    v = (v & 0xF0) >> 4 | (v & 0x0F) << 4;
    v = (v & 0xCC) >> 2 | (v & 0x33) << 2;
    (v & 0xAA) >> 1 | (v & 0x55) << 1
}
