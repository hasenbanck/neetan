use super::I386;
use crate::{PENDING_IRQ, PENDING_NMI, SegReg32};

const INTGATE_286: u8 = 6;
const TRAPGATE_286: u8 = 7;
const INTGATE_386: u8 = 14;
const TRAPGATE_386: u8 = 15;
const TASKGATE: u8 = 5;

enum DoubleFaultResult {
    Normal,
    DoubleFault,
    Shutdown,
}

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
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

    fn interrupt_with_return_eip(
        &mut self,
        vector: u8,
        return_eip: u32,
        error_code: Option<u16>,
        is_software_int: bool,
        is_external: bool,
        bus: &mut impl common::Bus,
    ) {
        self.rep_active = false;
        if !self.is_protected_mode() {
            let flags_val = self.flags.compress();
            self.push(bus, flags_val);
            self.flags.tf = false;
            self.flags.if_flag = false;

            let cs = self.sregs[SegReg32::CS as usize];
            self.push(bus, cs);
            self.push(bus, return_eip as u16);

            let addr = (vector as u32) * 4;
            let dest_ip = bus.read_word(addr);
            let dest_cs = bus.read_word(addr + 2);
            if !self.load_segment(SegReg32::CS, dest_cs, bus) {
                return;
            }
            self.ip = dest_ip;
            self.ip_upper = 0;
        } else {
            self.supervisor_override = true;
            self.interrupt_protected(
                vector,
                return_eip,
                error_code,
                is_software_int,
                is_external,
                bus,
            );
            self.supervisor_override = false;
        }
    }

    fn interrupt_protected(
        &mut self,
        vector: u8,
        return_eip: u32,
        error_code: Option<u16>,
        is_software_int: bool,
        is_external: bool,
        bus: &mut impl common::Bus,
    ) {
        let ext = is_external as u16;
        let gate_offset = (vector as u32) * 8;
        if gate_offset + 7 > self.idt_limit as u32 {
            self.raise_fault_with_code(13, gate_offset as u16 + 2 + ext, bus);
            return;
        }

        let gate_addr = self.idt_base.wrapping_add(gate_offset);
        let w0 = self.read_word_linear(bus, gate_addr);
        let w1 = self.read_word_linear(bus, gate_addr.wrapping_add(2));
        let w2 = self.read_word_linear(bus, gate_addr.wrapping_add(4));
        let w3 = self.read_word_linear(bus, gate_addr.wrapping_add(6));

        let gate_selector = w1;
        let rights_byte = (w2 >> 8) as u8;
        let gate_type = rights_byte & 0x1F;
        let gate_dpl = ((rights_byte >> 5) & 0x03) as u16;
        let gate_present = rights_byte & 0x80 != 0;

        let cpl = self.cpl();

        if is_software_int && gate_dpl < cpl {
            self.raise_fault_with_code(13, gate_offset as u16 + 2 + ext, bus);
            return;
        }

        if !gate_present {
            self.raise_fault_with_code(11, gate_offset as u16 + 2 + ext, bus);
            return;
        }

        let (gate_ip, is_386_gate) = match gate_type {
            INTGATE_386 | TRAPGATE_386 => ((w3 as u32) << 16 | w0 as u32, true),
            INTGATE_286 | TRAPGATE_286 => (w0 as u32, false),
            TASKGATE => {
                let task_selector = gate_selector;
                self.switch_task(task_selector, super::TaskType::Call, bus);
                let flags_val = self.flags.compress();
                let new_cpl = self.cpl();
                self.flags.load_flags(flags_val, new_cpl, true);
                if let Some(code) = error_code {
                    let is_386_tss = (self.tr_rights & 0x0F) >= 0x09;
                    if is_386_tss {
                        self.push_dword(bus, code as u32);
                    } else {
                        self.push(bus, code);
                    }
                }
                return;
            }
            _ => {
                self.raise_fault_with_code(13, gate_offset as u16 + 2 + ext, bus);
                return;
            }
        };

        self.dispatch_int_trap_gate(
            gate_ip,
            gate_selector,
            gate_type,
            is_386_gate,
            return_eip,
            error_code,
            ext,
            bus,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_int_trap_gate(
        &mut self,
        gate_ip: u32,
        gate_selector: u16,
        gate_type: u8,
        is_386_gate: bool,
        return_eip: u32,
        error_code: Option<u16>,
        ext: u16,
        bus: &mut impl common::Bus,
    ) {
        let Some(descriptor) = self.decode_descriptor(gate_selector, bus) else {
            self.raise_fault_with_code(13, Self::segment_error_code(gate_selector) + ext, bus);
            return;
        };

        let rights = descriptor.rights;
        if !Self::descriptor_is_code(rights) || !Self::descriptor_is_segment(rights) {
            self.raise_fault_with_code(13, Self::segment_error_code(gate_selector) + ext, bus);
            return;
        }

        let target_dpl = Self::descriptor_dpl(rights);
        let from_vm86 = self.is_virtual_mode();
        let cpl = self.cpl();

        if target_dpl > cpl {
            self.raise_fault_with_code(13, Self::segment_error_code(gate_selector) + ext, bus);
            return;
        }

        if from_vm86 && (Self::descriptor_is_conforming_code(rights) || target_dpl != 0) {
            self.raise_fault_with_code(13, Self::segment_error_code(gate_selector) + ext, bus);
            return;
        }

        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(11, Self::segment_error_code(gate_selector) + ext, bus);
            return;
        }

        if gate_ip > descriptor.limit {
            self.raise_fault_with_code(13, ext, bus);
            return;
        }

        if Self::descriptor_is_conforming_code(rights) {
            // Conforming code: DPL <= CPL is sufficient, treat as same-privilege.
        } else if target_dpl < cpl {
            // Inter-privilege interrupt: switch stacks from TSS.
            let new_dpl = target_dpl;
            let tss_type = self.tr_rights & 0x0F;
            let is_386_tss = tss_type == 9 || tss_type == 11;

            let (new_esp, new_ss) = if is_386_tss {
                let tss_esp_offset = 4 + new_dpl as u32 * 8;
                let tss_ss_offset = 8 + new_dpl as u32 * 8;
                if tss_ss_offset + 1 > self.tr_limit {
                    self.raise_fault_with_code(10, Self::segment_error_code(self.tr), bus);
                    return;
                }
                let esp = self.read_dword_linear(bus, self.tr_base.wrapping_add(tss_esp_offset));
                let ss = self.read_word_linear(bus, self.tr_base.wrapping_add(tss_ss_offset));
                (esp, ss)
            } else {
                let tss_sp_offset = 2 + new_dpl as u32 * 4;
                let tss_ss_offset = 4 + new_dpl as u32 * 4;
                if tss_ss_offset + 1 > self.tr_limit {
                    self.raise_fault_with_code(10, Self::segment_error_code(self.tr), bus);
                    return;
                }
                let sp = self.read_word_linear(bus, self.tr_base.wrapping_add(tss_sp_offset));
                let ss = self.read_word_linear(bus, self.tr_base.wrapping_add(tss_ss_offset));
                (sp as u32, ss)
            };

            let old_ss = self.sregs[SegReg32::SS as usize];
            let old_sp = if self.use_esp() {
                self.regs.dword(crate::DwordReg::ESP)
            } else {
                self.regs.word(crate::WordReg::SP) as u32
            };
            let old_es = self.sregs[SegReg32::ES as usize];
            let old_ds = self.sregs[SegReg32::DS as usize];
            let old_fs = self.sregs[SegReg32::FS as usize];
            let old_gs = self.sregs[SegReg32::GS as usize];
            let old_eflags = self.eflags_upper | self.flags.compress() as u32;
            let old_cs = self.sregs[SegReg32::CS as usize];

            let ss_error_code = Self::segment_error_code(new_ss) + ext;
            if new_ss & 0xFFFC == 0 {
                self.raise_fault_with_code(10, ss_error_code, bus);
                return;
            }
            let Some(ss_descriptor) = self.decode_descriptor(new_ss, bus) else {
                self.raise_fault_with_code(10, ss_error_code, bus);
                return;
            };
            let ss_rights = ss_descriptor.rights;
            let ss_dpl = Self::descriptor_dpl(ss_rights);
            let ss_rpl = new_ss & 0x0003;
            if !Self::descriptor_is_segment(ss_rights) || !Self::descriptor_is_writable(ss_rights) {
                self.raise_fault_with_code(10, ss_error_code, bus);
                return;
            }
            if ss_dpl != new_dpl || ss_rpl != new_dpl {
                self.raise_fault_with_code(10, ss_error_code, bus);
                return;
            }
            if !Self::descriptor_present(ss_rights) {
                self.raise_fault_with_code(12, ss_error_code, bus);
                return;
            }
            self.set_accessed_bit(new_ss, bus);
            self.set_loaded_segment_cache(SegReg32::SS, new_ss, ss_descriptor);
            if self.use_esp() {
                self.regs.set_dword(crate::DwordReg::ESP, new_esp);
            } else {
                self.regs.set_word(crate::WordReg::SP, new_esp as u16);
            }
            self.eflags_upper &= !0x0003_0000; // Clear RF and VM before pushes.

            if is_386_gate {
                if from_vm86 {
                    self.push_dword(bus, old_gs as u32);
                    self.push_dword(bus, old_fs as u32);
                    self.push_dword(bus, old_ds as u32);
                    self.push_dword(bus, old_es as u32);
                }
                self.push_dword(bus, old_ss as u32);
                self.push_dword(bus, old_sp);
                self.push_dword(bus, old_eflags);
                self.push_dword(bus, old_cs as u32);
                self.push_dword(bus, return_eip);
                if let Some(code) = error_code {
                    self.push_dword(bus, code as u32);
                }
            } else {
                if from_vm86 {
                    self.push(bus, old_gs);
                    self.push(bus, old_fs);
                    self.push(bus, old_ds);
                    self.push(bus, old_es);
                }
                self.push(bus, old_ss);
                self.push(bus, old_sp as u16);
                self.push(bus, old_eflags as u16);
                self.push(bus, old_cs);
                self.push(bus, return_eip as u16);
                if let Some(code) = error_code {
                    self.push(bus, code);
                }
            }

            self.set_accessed_bit(gate_selector, bus);
            let adjusted_selector = (gate_selector & !3) | new_dpl;
            self.set_loaded_segment_cache(SegReg32::CS, adjusted_selector, descriptor);
            self.ip = gate_ip as u16;
            self.ip_upper = gate_ip & 0xFFFF_0000;

            self.flags.tf = false;
            self.flags.nt = false;
            if gate_type == INTGATE_286 || gate_type == INTGATE_386 {
                self.flags.if_flag = false;
            }

            if from_vm86 {
                self.set_null_segment(SegReg32::ES, 0);
                self.set_null_segment(SegReg32::DS, 0);
                self.set_null_segment(SegReg32::FS, 0);
                self.set_null_segment(SegReg32::GS, 0);
            }
            return;
        }

        // Same-privilege interrupt.
        if is_386_gate {
            let eflags = self.eflags_upper | self.flags.compress() as u32;
            let cs = self.sregs[SegReg32::CS as usize];
            self.push_dword(bus, eflags);
            self.push_dword(bus, cs as u32);
            self.push_dword(bus, return_eip);
            if let Some(code) = error_code {
                self.push_dword(bus, code as u32);
            }
        } else {
            let flags_val = self.flags.compress();
            let cs = self.sregs[SegReg32::CS as usize];
            self.push(bus, flags_val);
            self.push(bus, cs);
            self.push(bus, return_eip as u16);
            if let Some(code) = error_code {
                self.push(bus, code);
            }
        }

        self.set_accessed_bit(gate_selector, bus);
        let adjusted_selector = (gate_selector & !3) | cpl;
        self.set_loaded_segment_cache(SegReg32::CS, adjusted_selector, descriptor);
        self.ip = gate_ip as u16;
        self.ip_upper = gate_ip & 0xFFFF_0000;

        self.flags.tf = false;
        self.flags.nt = false;
        self.eflags_upper &= !0x0003_0000; // Clear RF and VM.
        if gate_type == INTGATE_286 || gate_type == INTGATE_386 {
            self.flags.if_flag = false;
        }
    }

    pub(super) fn raise_interrupt(&mut self, vector: u8, bus: &mut impl common::Bus) {
        let return_eip = if self.rep_active {
            self.ip_upper | self.rep_restart_ip as u32
        } else {
            self.ip_upper | self.ip as u32
        };
        self.interrupt_with_return_eip(vector, return_eip, None, false, true, bus);
    }

    pub(super) fn raise_software_interrupt(
        &mut self,
        vector: u8,
        is_int_n: bool,
        bus: &mut impl common::Bus,
    ) {
        let return_eip = if self.rep_active {
            self.ip_upper | self.rep_restart_ip as u32
        } else {
            self.ip_upper | self.ip as u32
        };
        // In VM86, only INT n (opcode 0xCD) is IOPL-sensitive.
        // INT 3 and INTO always go through the IDT without an IOPL check.
        if self.is_virtual_mode() && is_int_n && self.flags.iopl < 3 {
            self.raise_fault_with_code(13, 0, bus);
            return;
        }
        self.interrupt_with_return_eip(vector, return_eip, None, true, false, bus);
    }

    pub(super) fn raise_fault(&mut self, vector: u8, bus: &mut impl common::Bus) {
        if self.shutdown {
            return;
        }
        let return_eip = self.prev_ip_upper | self.prev_ip as u32;
        if self.is_protected_mode() {
            match self.check_double_fault(vector) {
                DoubleFaultResult::Shutdown => return,
                DoubleFaultResult::DoubleFault => {
                    self.interrupt_with_return_eip(8, return_eip, Some(0), false, false, bus);
                    self.trap_level = 0;
                    return;
                }
                DoubleFaultResult::Normal => {}
            }
        }
        self.interrupt_with_return_eip(vector, return_eip, None, false, false, bus);
        self.trap_level = 0;
    }

    pub(super) fn raise_fault_with_code(
        &mut self,
        vector: u8,
        error_code: u16,
        bus: &mut impl common::Bus,
    ) {
        if self.shutdown {
            return;
        }
        let return_eip = self.prev_ip_upper | self.prev_ip as u32;
        if self.is_protected_mode() {
            match self.check_double_fault(vector) {
                DoubleFaultResult::Shutdown => return,
                DoubleFaultResult::DoubleFault => {
                    self.interrupt_with_return_eip(8, return_eip, Some(0), false, false, bus);
                    self.trap_level = 0;
                    return;
                }
                DoubleFaultResult::Normal => {}
            }
        }
        self.interrupt_with_return_eip(vector, return_eip, Some(error_code), false, false, bus);
        self.trap_level = 0;
    }

    fn is_contributory_exception(vector: u8) -> bool {
        matches!(vector, 0 | 10 | 11 | 12 | 13)
    }

    fn check_double_fault(&mut self, vector: u8) -> DoubleFaultResult {
        if Self::is_contributory_exception(vector) || vector == 8 {
            self.trap_level += 1;
            if self.trap_level >= 3 {
                self.shutdown = true;
                return DoubleFaultResult::Shutdown;
            }
            if self.trap_level >= 2 {
                return DoubleFaultResult::DoubleFault;
            }
        }
        DoubleFaultResult::Normal
    }
}
