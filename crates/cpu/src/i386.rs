//! Implements the Intel 80386+ family emulation.
//!
//! The CPU model is selected via the const generic parameter `CPU_MODEL`:
//! - [`CPU_MODEL_386`] - Intel 80386DX cycle timings.
//! - [`CPU_MODEL_486`] - Intel 80486DX cycle timings.
//!
//! Following references were used to write the emulator:
//!
//! - Intel Corporation, "80386 Programmer's Reference Manual".
//! - Intel Corporation, "i486 Processor Programmer's Reference Manual".

mod alu;
mod execute;
mod execute_0f;
mod execute_group;
mod flags;
mod fpu;
mod fpu_dispatch;
mod fpu_ops;
mod interrupt;
mod modrm;
mod paging;
mod rep;
mod state;
mod string_ops;

use std::ops::{Deref, DerefMut};

use common::Cpu as _;
pub use flags::I386Flags;
pub use paging::{TLB_MASK, TLB_SIZE, TlbCache};
pub use state::I386State;

use crate::{ByteReg, DwordReg, SegReg32, WordReg};

/// CPU model constant for Intel 80386DX.
pub const CPU_MODEL_386: u8 = 0;
/// CPU model constant for Intel 80486DX.
pub const CPU_MODEL_486: u8 = 1;

#[derive(Clone, Copy)]
struct SegmentDescriptor {
    base: u32,
    limit: u32,
    rights: u8,
    granularity: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum TaskType {
    Iret,
    Jmp,
    Call,
}

/// Intel 80386+ CPU emulator.
///
/// The const generic `CPU_MODEL` selects the instruction timings and feature set.
/// Use [`CPU_MODEL_386`] for an 80386DX or [`CPU_MODEL_486`] for an 80486DX.
pub struct I386<const CPU_MODEL: u8 = { CPU_MODEL_386 }> {
    /// Embedded state for save/restore.
    pub state: I386State,

    prev_ip: u16,
    prev_ip_upper: u32,
    seg_prefix: bool,
    prefix_seg: SegReg32,
    operand_size_override: bool,
    address_size_override: bool,
    lock_prefix: bool,

    halted: bool,
    fault_pending: bool,
    supervisor_override: bool,
    pending_irq: u8,
    no_interrupt: u8,
    inhibit_all: u8,

    rep_ip: u16,
    rep_restart_ip: u16,
    rep_seg_prefix: bool,
    rep_prefix_seg: SegReg32,
    rep_opcode: u8,
    rep_type: u8,
    rep_operand_size_override: bool,
    rep_address_size_override: bool,
    rep_active: bool,
    rep_completed: bool,

    cycles_remaining: i64,
    run_start_cycle: u64,
    run_budget: u64,

    ea: u32,
    eo: u16,
    eo32: u32,
    ea_seg: SegReg32,

    fetch_page_valid: bool,
    fetch_page_tag: u32,
    fetch_page_phys: u32,

    prefetch_valid: bool,
    prefetch_addr: u32,
    prefetch_byte: u8,

    debug_trap_pending: bool,
    trap_level: u8,
    prev_exception_class: u8,
    shutdown: bool,
}

impl<const CPU_MODEL: u8> Deref for I386<CPU_MODEL> {
    type Target = I386State;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<const CPU_MODEL: u8> DerefMut for I386<CPU_MODEL> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl<const CPU_MODEL: u8> Default for I386<CPU_MODEL> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CPU_MODEL: u8> I386<CPU_MODEL> {
    /// Creates a new I386 CPU in its reset state.
    pub fn new() -> Self {
        let mut cpu = Self {
            state: I386State::default(),
            prev_ip: 0,
            seg_prefix: false,
            prefix_seg: SegReg32::DS,
            operand_size_override: false,
            address_size_override: false,
            lock_prefix: false,
            halted: false,
            fault_pending: false,
            supervisor_override: false,
            pending_irq: 0,
            no_interrupt: 0,
            inhibit_all: 0,
            rep_ip: 0,
            rep_restart_ip: 0,
            rep_seg_prefix: false,
            rep_prefix_seg: SegReg32::DS,
            rep_opcode: 0,
            rep_type: 0,
            rep_operand_size_override: false,
            rep_address_size_override: false,
            rep_active: false,
            rep_completed: false,
            cycles_remaining: 0,
            run_start_cycle: 0,
            run_budget: 0,
            ea: 0,
            eo: 0,
            eo32: 0,
            ea_seg: SegReg32::DS,
            fetch_page_valid: false,
            fetch_page_tag: 0,
            fetch_page_phys: 0,
            prefetch_valid: false,
            prefetch_addr: 0,
            prefetch_byte: 0,
            debug_trap_pending: false,
            trap_level: 0,
            prev_exception_class: 0,
            shutdown: false,
            prev_ip_upper: 0,
        };
        cpu.reset();
        cpu
    }

    /// Selects between 386 and 486 cycle counts at compile time.
    #[inline(always)]
    const fn timing(t386: i32, t486: i32) -> i32 {
        match CPU_MODEL {
            CPU_MODEL_386 => t386,
            CPU_MODEL_486 => t486,
            _ => unreachable!(),
        }
    }

    #[inline(always)]
    fn clk(&mut self, cycles: i32) {
        self.cycles_remaining -= cycles as i64;
    }

    #[inline(always)]
    fn clk_modrm(&mut self, modrm: u8, reg_cycles: i32, mem_cycles: i32) {
        if modrm >= 0xC0 {
            self.clk(reg_cycles);
        } else {
            self.clk(mem_cycles);
        }
    }

    #[inline(always)]
    fn clk_modrm_word(&mut self, modrm: u8, reg_cycles: i32, mem_cycles: i32, word_accesses: i32) {
        if modrm >= 0xC0 {
            self.clk(reg_cycles);
        } else {
            let alignment_mask = match word_accesses {
                1 => 0x1,
                2 => {
                    if self.operand_size_override {
                        0x3
                    } else {
                        0x1
                    }
                }
                4 => 0x3,
                _ => {
                    if self.operand_size_override {
                        0x3
                    } else {
                        0x1
                    }
                }
            };
            let misaligned = self.ea & alignment_mask != 0;
            let penalty = if misaligned { Self::timing(4, 3) } else { 0 };
            self.clk(mem_cycles + penalty);
        }
    }

    #[inline(always)]
    fn sp_penalty(&self) -> i32 {
        let sp = if self.use_esp() {
            self.regs.dword(crate::DwordReg::ESP)
        } else {
            self.regs.word(WordReg::SP) as u32
        };
        let misaligned = if self.operand_size_override {
            sp & 3 != 0
        } else {
            sp & 1 != 0
        };
        if misaligned { Self::timing(4, 3) } else { 0 }
    }

    #[inline(always)]
    fn code_segment_32bit(&self) -> bool {
        if self.is_virtual_mode() {
            return false;
        }
        self.is_protected_mode()
            && self.seg_valid[SegReg32::CS as usize]
            && (self.seg_granularity[SegReg32::CS as usize] & 0x40) != 0
    }

    fn save_cs_state(&self) -> (u16, u32, u32, u8, u8, u16, u32) {
        (
            self.sregs[SegReg32::CS as usize],
            self.seg_bases[SegReg32::CS as usize],
            self.seg_limits[SegReg32::CS as usize],
            self.seg_rights[SegReg32::CS as usize],
            self.seg_granularity[SegReg32::CS as usize],
            self.ip,
            self.ip_upper,
        )
    }

    fn restore_cs_state(&mut self, saved: &(u16, u32, u32, u8, u8, u16, u32)) {
        self.sregs[SegReg32::CS as usize] = saved.0;
        self.seg_bases[SegReg32::CS as usize] = saved.1;
        self.seg_limits[SegReg32::CS as usize] = saved.2;
        self.seg_rights[SegReg32::CS as usize] = saved.3;
        self.seg_granularity[SegReg32::CS as usize] = saved.4;
        self.ip = saved.5;
        self.ip_upper = saved.6;
    }

    #[inline(always)]
    fn effective_eip(&self) -> u32 {
        self.ip_upper | self.ip as u32
    }

    #[inline(always)]
    fn advance_ip_byte(&mut self) {
        let next = (self.ip_upper | self.ip as u32).wrapping_add(1);
        self.ip = next as u16;
        self.ip_upper = next & 0xFFFF_0000;
    }

    #[inline(always)]
    fn advance_ip_by(&mut self, n: u32) {
        let next = self.effective_eip().wrapping_add(n);
        self.ip = next as u16;
        self.ip_upper = next & 0xFFFF_0000;
    }

    /// Applies a signed 8-bit branch displacement to EIP, respecting the
    /// current operand size: 32-bit mode preserves the full EIP, while
    /// 16-bit mode truncates to 16 bits.
    #[inline(always)]
    fn apply_branch_disp8(&mut self, disp: i8) {
        if self.operand_size_override {
            let eip = self.effective_eip().wrapping_add(disp as i32 as u32);
            self.ip = eip as u16;
            self.ip_upper = eip & 0xFFFF_0000;
        } else {
            self.ip = self.ip.wrapping_add(disp as u16);
            self.ip_upper = 0;
        }
    }

    #[inline(always)]
    fn next_instruction_length_approx(&self, bus: &mut impl common::Bus) -> i32 {
        let code_32bit = self.code_segment_32bit();
        let mut eip = self.effective_eip();
        let cs_base = self.seg_bases[SegReg32::CS as usize];
        let mut length = 0i32;

        loop {
            let addr = cs_base.wrapping_add(eip) & 0xFFFFFF;
            let byte = bus.read_byte(addr);
            match byte {
                0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 => {
                    length += 1;
                    if code_32bit {
                        eip = eip.wrapping_add(1);
                    } else {
                        eip = (eip as u16).wrapping_add(1) as u32;
                    }
                }
                _ => break,
            }
        }

        let opcode_addr = cs_base.wrapping_add(eip) & 0xFFFFFF;
        let opcode = bus.read_byte(opcode_addr);
        length += 1;

        if opcode == 0x0F {
            let opcode2_addr = (opcode_addr.wrapping_add(1)) & 0xFFFFFF;
            let opcode2 = bus.read_byte(opcode2_addr);
            length += 1;
            if Self::opcode_0f_has_modrm(opcode2) {
                length += 1;
            }
        } else if Self::opcode_has_modrm(opcode) {
            length += 1;
        }
        length
    }

    #[inline(always)]
    const fn opcode_0f_has_modrm(opcode: u8) -> bool {
        !matches!(
            opcode,
            0x06 | 0x08 | 0x09 | 0x0B | 0x80..=0x8F | 0xA0 | 0xA1 | 0xA2 | 0xA8 | 0xA9
        )
    }

    #[inline(always)]
    const fn opcode_has_modrm(opcode: u8) -> bool {
        const HAS_MODRM: [u32; 8] = [
            0x0F0F_0F0F,
            0x0F0F_0F0F,
            0x0000_0000,
            0x0000_0A0C,
            0x0000_FFFF,
            0x0000_0000,
            0xFF0F_00F3,
            0xC0C0_0000,
        ];
        let idx = (opcode >> 5) as usize;
        let bit = opcode & 0x1F;
        (HAS_MODRM[idx] >> bit) & 1 != 0
    }

    #[inline(always)]
    fn fetch(&mut self, bus: &mut impl common::Bus) -> u8 {
        if self.fault_pending {
            return 0;
        }
        let eip = self.effective_eip();
        let linear = self.seg_bases[SegReg32::CS as usize].wrapping_add(eip);
        let page = linear >> 12;
        let addr = if self.fetch_page_valid && self.fetch_page_tag == page {
            self.fetch_page_phys | (linear & 0xFFF)
        } else {
            let Some(addr) = self.translate_linear(linear, false, bus) else {
                self.prefetch_valid = false;
                return 0;
            };
            self.fetch_page_valid = true;
            self.fetch_page_tag = page;
            self.fetch_page_phys = addr & !0xFFF;
            addr
        };
        let value = if self.prefetch_valid && self.prefetch_addr == addr {
            self.prefetch_valid = false;
            self.prefetch_byte
        } else {
            bus.read_byte(addr)
        };
        self.advance_ip_byte();
        // Prefetch next byte to model the 386 prefetch queue.
        // This ensures bytes already fetched before a REP string op
        // overwrites them are still seen correctly.
        let next_addr = addr.wrapping_add(1);
        if next_addr & 0xFFF != 0 {
            self.prefetch_addr = next_addr;
            self.prefetch_byte = bus.read_byte(next_addr);
            self.prefetch_valid = true;
        } else {
            self.prefetch_valid = false;
        }
        value
    }

    #[inline(always)]
    fn fetchword(&mut self, bus: &mut impl common::Bus) -> u16 {
        if !self.fault_pending {
            let eip = self.effective_eip();
            let linear = self.seg_bases[SegReg32::CS as usize].wrapping_add(eip);
            if (linear & 0xFFF) <= 0xFFE {
                let page = linear >> 12;
                let addr = if self.fetch_page_valid && self.fetch_page_tag == page {
                    self.fetch_page_phys | (linear & 0xFFF)
                } else {
                    let Some(addr) = self.translate_linear(linear, false, bus) else {
                        return 0;
                    };
                    self.fetch_page_valid = true;
                    self.fetch_page_tag = page;
                    self.fetch_page_phys = addr & !0xFFF;
                    addr
                };
                let value = bus.read_word(addr);
                self.advance_ip_by(2);
                return value;
            }
        }
        let low = self.fetch(bus) as u16;
        let high = self.fetch(bus) as u16;
        low | (high << 8)
    }

    #[inline(always)]
    fn fetchdword(&mut self, bus: &mut impl common::Bus) -> u32 {
        if !self.fault_pending {
            let eip = self.effective_eip();
            let linear = self.seg_bases[SegReg32::CS as usize].wrapping_add(eip);
            if (linear & 0xFFF) <= 0xFFC {
                let page = linear >> 12;
                let addr = if self.fetch_page_valid && self.fetch_page_tag == page {
                    self.fetch_page_phys | (linear & 0xFFF)
                } else {
                    let Some(addr) = self.translate_linear(linear, false, bus) else {
                        return 0;
                    };
                    self.fetch_page_valid = true;
                    self.fetch_page_tag = page;
                    self.fetch_page_phys = addr & !0xFFF;
                    addr
                };
                let value = bus.read_dword(addr);
                self.advance_ip_by(4);
                return value;
            }
        }
        let b0 = self.fetch(bus) as u32;
        let b1 = self.fetch(bus) as u32;
        let b2 = self.fetch(bus) as u32;
        let b3 = self.fetch(bus) as u32;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }

    #[inline(always)]
    fn default_seg(&self, seg: SegReg32) -> SegReg32 {
        if self.seg_prefix && matches!(seg, SegReg32::DS | SegReg32::SS) {
            self.prefix_seg
        } else {
            seg
        }
    }

    #[inline(always)]
    fn default_base(&self, seg: SegReg32) -> u32 {
        self.seg_bases[self.default_seg(seg) as usize]
    }

    #[inline(always)]
    fn seg_base(&self, seg: SegReg32) -> u32 {
        self.seg_bases[seg as usize]
    }

    /// Computes the linear address for a byte at `eo32 + delta`.
    /// In 16-bit address mode, the offset wraps at 16 bits.
    #[inline(always)]
    fn seg_addr(&self, delta: u32) -> u32 {
        let offset = if self.address_size_override {
            self.eo32.wrapping_add(delta)
        } else {
            (self.eo32 as u16).wrapping_add(delta as u16) as u32
        };
        self.seg_base(self.ea_seg).wrapping_add(offset)
    }

    #[inline(always)]
    fn cpl(&self) -> u16 {
        if self.is_virtual_mode() {
            return 3;
        }
        if !self.is_protected_mode() {
            return 0;
        }
        self.stored_cpl
    }

    #[inline(always)]
    fn is_protected_mode(&self) -> bool {
        self.cr0 & 1 != 0
    }

    #[inline(always)]
    fn is_virtual_mode(&self) -> bool {
        self.is_protected_mode() && (self.eflags_upper & 0x0002_0000) != 0
    }

    /// Returns `true` if the current privilege level and IOPL allow I/O access to `port`.
    /// When access is denied and the IOPB does not grant it, raises #GP(0) and returns `false`.
    /// CLI/STI/HLT use separate IOPL/CPL checks - this function is only for I/O port instructions.
    fn check_io_privilege(&mut self, port: u16, size: u8, bus: &mut impl common::Bus) -> bool {
        if !self.is_protected_mode() {
            return true;
        }
        if !self.is_virtual_mode() && self.cpl() <= u16::from(self.flags.iopl) {
            return true;
        }
        if bus.is_io_port_unrestricted(port) {
            return true;
        }
        // CPL > IOPL (or VM86 with IOPL < 3): consult I/O Permission Bitmap in TSS.
        // TSS must be a 386 TSS (type field >= 9) and large enough to hold the IOPB pointer.
        if self.tr_limit < 0x67 || (self.tr_rights & 0x0F) < 9 {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        let iopb = self.read_word_linear(bus, self.tr_base.wrapping_add(0x66)) as u32;
        let byte_idx = iopb.wrapping_add(u32::from(port) / 8);
        // Read two consecutive bytes from the IOPB to handle accesses that span a
        // byte boundary (e.g. a dword access starting at port 7 touches bits in two
        // bytes). The +1 also satisfies the Intel spec requirement for a 0xFF sentinel
        // byte immediately after the IOPB inside the TSS.
        if byte_idx + 1 > self.tr_limit {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        let linear = self.tr_base.wrapping_add(byte_idx);
        let phys = self.translate_linear(linear, false, bus).unwrap_or(0);
        let lo = bus.read_byte(phys) as u16;
        let hi = bus.read_byte(phys.wrapping_add(1) & 0x00FF_FFFF) as u16;
        let map = lo | (hi << 8);
        let mask = (1u16 << size) - 1;
        if (map >> (port & 7)) & mask != 0 {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        true
    }

    fn is_lockable(opcode: u8) -> bool {
        matches!(
            opcode,
            // ADD r/m, r | ADD r/m, imm (via 80/81/83)
            0x00 | 0x01
            // OR r/m, r
            | 0x08 | 0x09
            // ADC r/m, r
            | 0x10 | 0x11
            // SBB r/m, r
            | 0x18 | 0x19
            // AND r/m, r
            | 0x20 | 0x21
            // SUB r/m, r
            | 0x28 | 0x29
            // XOR r/m, r
            | 0x30 | 0x31
            // ADD/OR/ADC/SBB/AND/SUB/XOR/CMP r/m, imm (0x82 is alias for 0x80)
            | 0x80 | 0x81 | 0x82 | 0x83
            // XCHG r/m, r
            | 0x86 | 0x87
            // NOT/NEG r/m (F6 /2, /3 and F7 /2, /3)
            | 0xF6 | 0xF7
            // INC/DEC r/m (FE /0, /1 and FF /0, /1)
            | 0xFE | 0xFF
        )
    }

    #[inline(always)]
    fn segment_error_code(selector: u16) -> u16 {
        selector & 0xFFFC
    }

    fn set_real_segment_cache(&mut self, seg: SegReg32, selector: u16) {
        self.seg_bases[seg as usize] = (selector as u32) << 4;
        self.seg_limits[seg as usize] = 0xFFFF;
        self.seg_rights[seg as usize] = if seg == SegReg32::CS { 0x9B } else { 0x93 };
        self.seg_granularity[seg as usize] = 0;
        self.seg_valid[seg as usize] = true;
        if seg == SegReg32::CS {
            self.stored_cpl = 0;
        }
    }

    fn set_loaded_segment_cache(
        &mut self,
        seg: SegReg32,
        selector: u16,
        descriptor: SegmentDescriptor,
    ) {
        self.sregs[seg as usize] = selector;
        self.seg_bases[seg as usize] = descriptor.base;
        self.seg_limits[seg as usize] = descriptor.limit;
        self.seg_rights[seg as usize] = descriptor.rights;
        self.seg_granularity[seg as usize] = descriptor.granularity;
        self.seg_valid[seg as usize] = true;
        if seg == SegReg32::CS {
            self.stored_cpl = selector & 3;
        }
    }

    fn set_null_segment(&mut self, seg: SegReg32, selector: u16) {
        self.sregs[seg as usize] = selector;
        self.seg_bases[seg as usize] = 0;
        self.seg_limits[seg as usize] = 0;
        self.seg_rights[seg as usize] = 0;
        self.seg_granularity[seg as usize] = 0;
        self.seg_valid[seg as usize] = false;
    }

    fn decode_descriptor(
        &mut self,
        selector: u16,
        bus: &mut impl common::Bus,
    ) -> Option<SegmentDescriptor> {
        let addr = self.descriptor_addr_checked(selector)?;
        Some(self.decode_descriptor_at(addr, bus))
    }

    fn descriptor_dpl(rights: u8) -> u16 {
        ((rights >> 5) & 0x03) as u16
    }

    fn descriptor_is_segment(rights: u8) -> bool {
        rights & 0x10 != 0
    }

    fn descriptor_is_code(rights: u8) -> bool {
        rights & 0x08 != 0
    }

    fn descriptor_is_conforming_code(rights: u8) -> bool {
        Self::descriptor_is_code(rights) && rights & 0x04 != 0
    }

    fn descriptor_is_readable(rights: u8) -> bool {
        !Self::descriptor_is_code(rights) || rights & 0x02 != 0
    }

    fn descriptor_is_writable(rights: u8) -> bool {
        !Self::descriptor_is_code(rights) && rights & 0x02 != 0
    }

    fn descriptor_is_expand_down(rights: u8) -> bool {
        !Self::descriptor_is_code(rights) && rights & 0x04 != 0
    }

    fn descriptor_present(rights: u8) -> bool {
        rights & 0x80 != 0
    }

    fn raise_segment_not_present(
        &mut self,
        seg: SegReg32,
        selector: u16,
        bus: &mut impl common::Bus,
    ) {
        let vector = if seg == SegReg32::SS { 12 } else { 11 };
        self.raise_fault_with_code(vector, Self::segment_error_code(selector), bus);
    }

    fn raise_segment_protection(
        &mut self,
        seg: SegReg32,
        selector: u16,
        bus: &mut impl common::Bus,
    ) {
        let vector = if seg == SegReg32::SS { 12 } else { 13 };
        self.raise_fault_with_code(vector, Self::segment_error_code(selector), bus);
    }

    fn load_protected_segment(
        &mut self,
        seg: SegReg32,
        selector: u16,
        bus: &mut impl common::Bus,
    ) -> bool {
        if matches!(
            seg,
            SegReg32::DS | SegReg32::ES | SegReg32::FS | SegReg32::GS
        ) && selector & 0xFFFC == 0
        {
            self.set_null_segment(seg, selector);
            return true;
        }
        if selector & 0xFFFC == 0 {
            // Null selector for CS or SS: always #GP(0) per 386 manual.
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }

        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        };
        let rights = descriptor.rights;
        let cpl = self.cpl();
        let rpl = selector & 0x0003;
        let dpl = Self::descriptor_dpl(rights);

        match seg {
            SegReg32::CS => {
                if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_code(rights) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if Self::descriptor_is_conforming_code(rights) {
                    if dpl > cpl {
                        self.raise_segment_protection(seg, selector, bus);
                        return false;
                    }
                } else if dpl != cpl || rpl > cpl {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if !Self::descriptor_present(rights) {
                    self.raise_segment_not_present(seg, selector, bus);
                    return false;
                }
                self.set_accessed_bit(selector, bus);
                if Self::descriptor_is_conforming_code(rights) {
                    let adjusted = (selector & !3) | cpl;
                    self.set_loaded_segment_cache(seg, adjusted, descriptor);
                } else {
                    let adjusted = (selector & !3) | dpl;
                    self.set_loaded_segment_cache(seg, adjusted, descriptor);
                }
                return true;
            }
            SegReg32::SS => {
                if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_writable(rights) {
                    self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                    return false;
                }
                if dpl != cpl || rpl != cpl {
                    self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                    return false;
                }
                if !Self::descriptor_present(rights) {
                    self.raise_fault_with_code(12, Self::segment_error_code(selector), bus);
                    return false;
                }
            }
            SegReg32::DS | SegReg32::ES | SegReg32::FS | SegReg32::GS => {
                if !Self::descriptor_is_segment(rights) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if !Self::descriptor_is_readable(rights) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if !Self::descriptor_is_conforming_code(rights) && dpl < cpl.max(rpl) {
                    self.raise_segment_protection(seg, selector, bus);
                    return false;
                }
                if !Self::descriptor_present(rights) {
                    self.raise_segment_not_present(seg, selector, bus);
                    return false;
                }
            }
        }

        self.set_accessed_bit(selector, bus);
        self.set_loaded_segment_cache(seg, selector, descriptor);
        true
    }

    fn load_cs_for_return(
        &mut self,
        selector: u16,
        new_ip: u32,
        bus: &mut impl common::Bus,
    ) -> bool {
        if selector & 0xFFFC == 0 {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        };
        let rights = descriptor.rights;
        let cpl = self.cpl();
        let rpl = selector & 0x0003;
        let dpl = Self::descriptor_dpl(rights);

        if rpl < cpl {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_code(rights) {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if Self::descriptor_is_conforming_code(rights) {
            if dpl > rpl {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                return false;
            }
        } else if dpl != rpl {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
            return false;
        }
        if new_ip > descriptor.limit {
            self.raise_fault_with_code(13, 0, bus);
            return false;
        }
        self.set_accessed_bit(selector, bus);
        let adjusted = (selector & !3) | rpl;
        self.set_loaded_segment_cache(SegReg32::CS, adjusted, descriptor);
        true
    }

    fn check_segment_access(
        &mut self,
        seg: SegReg32,
        offset: u32,
        size: u32,
        write: bool,
        bus: &mut impl common::Bus,
    ) -> bool {
        if !self.is_protected_mode() {
            return true;
        }
        if self.is_virtual_mode() {
            return true;
        }

        if !self.seg_valid[seg as usize] {
            let vector = if seg == SegReg32::SS { 12 } else { 13 };
            self.raise_fault_with_code(vector, 0, bus);
            return false;
        }

        let rights = self.seg_rights[seg as usize];
        let end = offset.saturating_add(size.saturating_sub(1));
        let wrapped = offset.checked_add(size.saturating_sub(1)).is_none();
        let limit = self.seg_limits[seg as usize];
        if Self::descriptor_is_expand_down(rights) {
            let upper = if self.seg_granularity[seg as usize] & 0x40 != 0 {
                0xFFFF_FFFF
            } else {
                0xFFFF
            };
            if offset <= limit || end > upper || wrapped {
                self.raise_fault_with_code(if seg == SegReg32::SS { 12 } else { 13 }, 0, bus);
                return false;
            }
        } else if end > limit || wrapped {
            self.raise_fault_with_code(if seg == SegReg32::SS { 12 } else { 13 }, 0, bus);
            return false;
        }

        if write {
            if !Self::descriptor_is_writable(rights) {
                let vector = if seg == SegReg32::SS { 12 } else { 13 };
                self.raise_fault_with_code(vector, 0, bus);
                return false;
            }
        } else if !Self::descriptor_is_readable(rights) {
            let vector = if seg == SegReg32::SS { 12 } else { 13 };
            self.raise_fault_with_code(vector, 0, bus);
            return false;
        }

        true
    }

    #[inline(always)]
    fn seg_read_word_at(&mut self, bus: &mut impl common::Bus, delta: u32) -> u16 {
        let offset = if self.address_size_override {
            self.eo32.wrapping_add(delta)
        } else {
            (self.eo32 as u16).wrapping_add(delta as u16) as u32
        };
        if !self.check_segment_access(self.ea_seg, offset, 2, false, bus) {
            return 0;
        }
        let l0 = self.seg_addr(delta);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFE
        } else {
            l0 & 0xFFF <= 0xFFE && (offset as u16) <= 0xFFFE
        };
        if same_page {
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            return bus.read_word(a0);
        }
        let l1 = self.seg_addr(delta.wrapping_add(1));
        let Some(a0) = self.translate_linear(l0, false, bus) else {
            return 0;
        };
        let Some(a1) = self.translate_linear(l1, false, bus) else {
            return 0;
        };
        bus.read_byte(a0) as u16 | ((bus.read_byte(a1) as u16) << 8)
    }

    #[inline(always)]
    fn seg_read_word(&mut self, bus: &mut impl common::Bus) -> u16 {
        self.seg_read_word_at(bus, 0)
    }

    #[inline(always)]
    fn seg_read_dword_at(&mut self, bus: &mut impl common::Bus, delta: u32) -> u32 {
        let offset = if self.address_size_override {
            self.eo32.wrapping_add(delta)
        } else {
            (self.eo32 as u16).wrapping_add(delta as u16) as u32
        };
        if !self.check_segment_access(self.ea_seg, offset, 4, false, bus) {
            return 0;
        }
        let l0 = self.seg_addr(delta);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFC
        } else {
            l0 & 0xFFF <= 0xFFC && (offset as u16) <= 0xFFFC
        };
        if same_page {
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            return bus.read_dword(a0);
        }
        let l1 = self.seg_addr(delta.wrapping_add(1));
        let l2 = self.seg_addr(delta.wrapping_add(2));
        let l3 = self.seg_addr(delta.wrapping_add(3));
        let Some(a0) = self.translate_linear(l0, false, bus) else {
            return 0;
        };
        let Some(a1) = self.translate_linear(l1, false, bus) else {
            return 0;
        };
        let Some(a2) = self.translate_linear(l2, false, bus) else {
            return 0;
        };
        let Some(a3) = self.translate_linear(l3, false, bus) else {
            return 0;
        };
        bus.read_byte(a0) as u32
            | ((bus.read_byte(a1) as u32) << 8)
            | ((bus.read_byte(a2) as u32) << 16)
            | ((bus.read_byte(a3) as u32) << 24)
    }

    #[inline(always)]
    fn seg_read_dword(&mut self, bus: &mut impl common::Bus) -> u32 {
        self.seg_read_dword_at(bus, 0)
    }

    #[inline(always)]
    fn seg_write_word(&mut self, bus: &mut impl common::Bus, value: u16) {
        if !self.check_segment_access(self.ea_seg, self.eo32, 2, true, bus) {
            return;
        }
        let l0 = self.seg_addr(0);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFE
        } else {
            l0 & 0xFFF <= 0xFFE && (self.eo32 as u16) <= 0xFFFE
        };
        if same_page {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return;
            };
            bus.write_word(a0, value);
            return;
        }
        let l1 = self.seg_addr(1);
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
    }

    #[inline(always)]
    fn seg_write_dword(&mut self, bus: &mut impl common::Bus, value: u32) {
        if !self.check_segment_access(self.ea_seg, self.eo32, 4, true, bus) {
            return;
        }
        let l0 = self.seg_addr(0);
        let same_page = if self.address_size_override {
            l0 & 0xFFF <= 0xFFC
        } else {
            l0 & 0xFFF <= 0xFFC && (self.eo32 as u16) <= 0xFFFC
        };
        if same_page {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return;
            };
            bus.write_dword(a0, value);
            return;
        }
        let l1 = self.seg_addr(1);
        let l2 = self.seg_addr(2);
        let l3 = self.seg_addr(3);
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        let Some(a2) = self.translate_linear(l2, true, bus) else {
            return;
        };
        let Some(a3) = self.translate_linear(l3, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        bus.write_byte(a2, (value >> 16) as u8);
        bus.write_byte(a3, (value >> 24) as u8);
    }

    /// Reads a byte from segment:offset, performing the same segment
    /// limit check and paging translation as the interpreter's normal
    /// memory path. Returns 0 on fault; the caller should inspect
    /// `fault_pending` to detect the fault. Exposed for the dynarec
    /// bytecode backend.
    #[inline(always)]
    pub fn read_byte_seg(&mut self, bus: &mut impl common::Bus, seg: SegReg32, offset: u32) -> u8 {
        if !self.check_segment_access(seg, offset, 1, false, bus) {
            return 0;
        }
        let linear = self.seg_base(seg).wrapping_add(offset);
        let Some(addr) = self.translate_linear(linear, false, bus) else {
            return 0;
        };
        bus.read_byte(addr)
    }

    /// Writes a byte to segment:offset with segment-limit and paging
    /// checks. Sets `fault_pending` on fault. Exposed for the dynarec
    /// bytecode backend.
    #[inline(always)]
    pub fn write_byte_seg(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg32,
        offset: u32,
        value: u8,
    ) {
        if !self.check_segment_access(seg, offset, 1, true, bus) {
            return;
        }
        let linear = self.seg_base(seg).wrapping_add(offset);
        let Some(addr) = self.translate_linear(linear, true, bus) else {
            return;
        };
        bus.write_byte(addr, value);
    }

    /// Reads a word from segment:offset, splitting on a page boundary
    /// to match interpreter byte-level fault semantics. Exposed for the
    /// dynarec bytecode backend.
    #[inline(always)]
    pub fn read_word_seg(&mut self, bus: &mut impl common::Bus, seg: SegReg32, offset: u32) -> u16 {
        if !self.check_segment_access(seg, offset, 2, false, bus) {
            return 0;
        }
        let base = self.seg_base(seg);
        let l0 = base.wrapping_add(offset);
        if l0 & 0xFFF <= 0xFFE {
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            return bus.read_word(a0);
        }
        let l1 = base.wrapping_add(offset.wrapping_add(1));
        let Some(a0) = self.translate_linear(l0, false, bus) else {
            return 0;
        };
        let Some(a1) = self.translate_linear(l1, false, bus) else {
            return 0;
        };
        bus.read_byte(a0) as u16 | ((bus.read_byte(a1) as u16) << 8)
    }

    /// Reads a dword from segment:offset, splitting on a page boundary.
    /// Exposed for the dynarec bytecode backend.
    #[inline(always)]
    pub fn read_dword_seg(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg32,
        offset: u32,
    ) -> u32 {
        if !self.check_segment_access(seg, offset, 4, false, bus) {
            return 0;
        }
        let base = self.seg_base(seg);
        let l0 = base.wrapping_add(offset);
        if l0 & 0xFFF <= 0xFFC {
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            return bus.read_dword(a0);
        }
        let l1 = base.wrapping_add(offset.wrapping_add(1));
        let l2 = base.wrapping_add(offset.wrapping_add(2));
        let l3 = base.wrapping_add(offset.wrapping_add(3));
        let Some(a0) = self.translate_linear(l0, false, bus) else {
            return 0;
        };
        let Some(a1) = self.translate_linear(l1, false, bus) else {
            return 0;
        };
        let Some(a2) = self.translate_linear(l2, false, bus) else {
            return 0;
        };
        let Some(a3) = self.translate_linear(l3, false, bus) else {
            return 0;
        };
        bus.read_byte(a0) as u32
            | ((bus.read_byte(a1) as u32) << 8)
            | ((bus.read_byte(a2) as u32) << 16)
            | ((bus.read_byte(a3) as u32) << 24)
    }

    /// Writes a word to segment:offset, splitting on a page boundary.
    /// Exposed for the dynarec bytecode backend.
    #[inline(always)]
    pub fn write_word_seg(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg32,
        offset: u32,
        value: u16,
    ) {
        if !self.check_segment_access(seg, offset, 2, true, bus) {
            return;
        }
        let base = self.seg_base(seg);
        let l0 = base.wrapping_add(offset);
        if l0 & 0xFFF <= 0xFFE {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return;
            };
            bus.write_word(a0, value);
            return;
        }
        let l1 = base.wrapping_add(offset.wrapping_add(1));
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
    }

    /// Writes a dword to segment:offset, splitting on a page boundary.
    /// Exposed for the dynarec bytecode backend.
    #[inline(always)]
    pub fn write_dword_seg(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg32,
        offset: u32,
        value: u32,
    ) {
        if !self.check_segment_access(seg, offset, 4, true, bus) {
            return;
        }
        let base = self.seg_base(seg);
        let l0 = base.wrapping_add(offset);
        if l0 & 0xFFF <= 0xFFC {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return;
            };
            bus.write_dword(a0, value);
            return;
        }
        let l1 = base.wrapping_add(offset.wrapping_add(1));
        let l2 = base.wrapping_add(offset.wrapping_add(2));
        let l3 = base.wrapping_add(offset.wrapping_add(3));
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        let Some(a2) = self.translate_linear(l2, true, bus) else {
            return;
        };
        let Some(a3) = self.translate_linear(l3, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        bus.write_byte(a2, (value >> 16) as u8);
        bus.write_byte(a3, (value >> 24) as u8);
    }

    #[inline(always)]
    fn use_esp(&self) -> bool {
        if self.is_virtual_mode() {
            return false;
        }
        self.seg_granularity[SegReg32::SS as usize] & 0x40 != 0
    }

    fn push(&mut self, bus: &mut impl common::Bus, value: u16) {
        let sp = if self.use_esp() {
            self.regs.dword(crate::DwordReg::ESP).wrapping_sub(2)
        } else {
            self.regs.word(WordReg::SP).wrapping_sub(2) as u32
        };
        if !self.check_segment_access(SegReg32::SS, sp, 2, true, bus) {
            return;
        }
        if self.use_esp() {
            self.regs.set_dword(crate::DwordReg::ESP, sp);
        } else {
            self.regs.set_word(WordReg::SP, sp as u16);
        }
        let base = self.seg_base(SegReg32::SS);
        let l0 = base.wrapping_add(sp);
        if l0 & 0xFFF <= 0xFFE {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return;
            };
            bus.write_word(a0, value);
            return;
        }
        let l1 = base.wrapping_add(sp.wrapping_add(1));
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
    }

    fn pop(&mut self, bus: &mut impl common::Bus) -> u16 {
        let sp = if self.use_esp() {
            self.regs.dword(crate::DwordReg::ESP)
        } else {
            self.regs.word(WordReg::SP) as u32
        };
        if !self.check_segment_access(SegReg32::SS, sp, 2, false, bus) {
            return 0;
        }
        let base = self.seg_base(SegReg32::SS);
        let l0 = base.wrapping_add(sp);
        let value = if l0 & 0xFFF <= 0xFFE {
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            bus.read_word(a0)
        } else {
            let l1 = base.wrapping_add(sp.wrapping_add(1));
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            let Some(a1) = self.translate_linear(l1, false, bus) else {
                return 0;
            };
            bus.read_byte(a0) as u16 | ((bus.read_byte(a1) as u16) << 8)
        };
        if self.use_esp() {
            self.regs
                .set_dword(crate::DwordReg::ESP, sp.wrapping_add(2));
        } else {
            self.regs.set_word(WordReg::SP, sp.wrapping_add(2) as u16);
        }
        value
    }

    fn push_dword(&mut self, bus: &mut impl common::Bus, value: u32) {
        let sp = if self.use_esp() {
            self.regs.dword(crate::DwordReg::ESP).wrapping_sub(4)
        } else {
            self.regs.word(WordReg::SP).wrapping_sub(4) as u32
        };
        if !self.check_segment_access(SegReg32::SS, sp, 4, true, bus) {
            return;
        }
        if self.use_esp() {
            self.regs.set_dword(crate::DwordReg::ESP, sp);
        } else {
            self.regs.set_word(WordReg::SP, sp as u16);
        }
        let base = self.seg_base(SegReg32::SS);
        let l0 = base.wrapping_add(sp);
        if l0 & 0xFFF <= 0xFFC {
            let Some(a0) = self.translate_linear(l0, true, bus) else {
                return;
            };
            bus.write_dword(a0, value);
            return;
        }
        let l1 = base.wrapping_add(sp.wrapping_add(1));
        let l2 = base.wrapping_add(sp.wrapping_add(2));
        let l3 = base.wrapping_add(sp.wrapping_add(3));
        let Some(a0) = self.translate_linear(l0, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(l1, true, bus) else {
            return;
        };
        let Some(a2) = self.translate_linear(l2, true, bus) else {
            return;
        };
        let Some(a3) = self.translate_linear(l3, true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        bus.write_byte(a2, (value >> 16) as u8);
        bus.write_byte(a3, (value >> 24) as u8);
    }

    fn pop_dword(&mut self, bus: &mut impl common::Bus) -> u32 {
        let sp = if self.use_esp() {
            self.regs.dword(crate::DwordReg::ESP)
        } else {
            self.regs.word(WordReg::SP) as u32
        };
        if !self.check_segment_access(SegReg32::SS, sp, 4, false, bus) {
            return 0;
        }
        let base = self.seg_base(SegReg32::SS);
        let l0 = base.wrapping_add(sp);
        let value = if l0 & 0xFFF <= 0xFFC {
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            bus.read_dword(a0)
        } else {
            let l1 = base.wrapping_add(sp.wrapping_add(1));
            let l2 = base.wrapping_add(sp.wrapping_add(2));
            let l3 = base.wrapping_add(sp.wrapping_add(3));
            let Some(a0) = self.translate_linear(l0, false, bus) else {
                return 0;
            };
            let Some(a1) = self.translate_linear(l1, false, bus) else {
                return 0;
            };
            let Some(a2) = self.translate_linear(l2, false, bus) else {
                return 0;
            };
            let Some(a3) = self.translate_linear(l3, false, bus) else {
                return 0;
            };
            bus.read_byte(a0) as u32
                | ((bus.read_byte(a1) as u32) << 8)
                | ((bus.read_byte(a2) as u32) << 16)
                | ((bus.read_byte(a3) as u32) << 24)
        };
        if self.use_esp() {
            self.regs
                .set_dword(crate::DwordReg::ESP, sp.wrapping_add(4));
        } else {
            self.regs.set_word(WordReg::SP, sp.wrapping_add(4) as u16);
        }
        value
    }

    fn load_segment(&mut self, seg: SegReg32, selector: u16, bus: &mut impl common::Bus) -> bool {
        if !self.is_protected_mode() || self.is_virtual_mode() {
            self.sregs[seg as usize] = selector;
            self.set_real_segment_cache(seg, selector);
            return true;
        }
        self.load_protected_segment(seg, selector, bus)
    }

    fn descriptor_addr_checked(&self, selector: u16) -> Option<u32> {
        if selector & 0xFFFC == 0 {
            return None;
        }
        let (table_base, table_limit) = if selector & 4 != 0 {
            (self.ldtr_base, self.ldtr_limit)
        } else {
            (self.gdt_base, self.gdt_limit as u32)
        };
        let index = (selector & !7) as u32;
        if index.wrapping_add(7) > table_limit {
            return None;
        }
        Some(table_base.wrapping_add(index))
    }

    fn set_accessed_bit(&mut self, selector: u16, bus: &mut impl common::Bus) {
        if let Some(addr) = self.descriptor_addr_checked(selector) {
            let linear = addr.wrapping_add(5);
            let phys = self.translate_linear(linear, true, bus).unwrap_or(0);
            let rights = bus.read_byte(phys);
            if rights & 0x01 == 0 {
                bus.write_byte(phys, rights | 0x01);
            }
        }
    }

    fn invalidate_segment_if_needed(&mut self, seg: SegReg32, new_cpl: u16) {
        if !self.seg_valid[seg as usize] {
            return;
        }
        let rights = self.seg_rights[seg as usize];
        if !Self::descriptor_is_segment(rights) {
            self.set_null_segment(seg, 0);
            return;
        }
        if Self::descriptor_is_conforming_code(rights) {
            return;
        }
        let dpl = Self::descriptor_dpl(rights);
        let rpl = self.sregs[seg as usize] & 3;
        if dpl < new_cpl || dpl < rpl {
            self.set_null_segment(seg, 0);
        }
    }

    fn read_word_linear(&mut self, bus: &mut impl common::Bus, addr: u32) -> u16 {
        if addr & 0xFFF <= 0xFFE {
            let Some(a0) = self.translate_linear(addr, false, bus) else {
                return 0;
            };
            return bus.read_word(a0);
        }
        let a0 = self.translate_linear(addr, false, bus).unwrap_or(0);
        let a1 = self
            .translate_linear(addr.wrapping_add(1), false, bus)
            .unwrap_or(0);
        bus.read_byte(a0) as u16 | ((bus.read_byte(a1) as u16) << 8)
    }

    fn write_word_linear(&mut self, bus: &mut impl common::Bus, addr: u32, value: u16) {
        if addr & 0xFFF <= 0xFFE {
            let Some(a0) = self.translate_linear(addr, true, bus) else {
                return;
            };
            bus.write_word(a0, value);
            return;
        }
        let Some(a0) = self.translate_linear(addr, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(addr.wrapping_add(1), true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
    }

    fn read_dword_linear(&mut self, bus: &mut impl common::Bus, addr: u32) -> u32 {
        if addr & 0xFFF <= 0xFFC {
            let Some(a0) = self.translate_linear(addr, false, bus) else {
                return 0;
            };
            return bus.read_dword(a0);
        }
        let a0 = self.translate_linear(addr, false, bus).unwrap_or(0);
        let a1 = self
            .translate_linear(addr.wrapping_add(1), false, bus)
            .unwrap_or(0);
        let a2 = self
            .translate_linear(addr.wrapping_add(2), false, bus)
            .unwrap_or(0);
        let a3 = self
            .translate_linear(addr.wrapping_add(3), false, bus)
            .unwrap_or(0);
        bus.read_byte(a0) as u32
            | ((bus.read_byte(a1) as u32) << 8)
            | ((bus.read_byte(a2) as u32) << 16)
            | ((bus.read_byte(a3) as u32) << 24)
    }

    fn write_dword_linear(&mut self, bus: &mut impl common::Bus, addr: u32, value: u32) {
        if addr & 0xFFF <= 0xFFC {
            let Some(a0) = self.translate_linear(addr, true, bus) else {
                return;
            };
            bus.write_dword(a0, value);
            return;
        }
        let Some(a0) = self.translate_linear(addr, true, bus) else {
            return;
        };
        let Some(a1) = self.translate_linear(addr.wrapping_add(1), true, bus) else {
            return;
        };
        let Some(a2) = self.translate_linear(addr.wrapping_add(2), true, bus) else {
            return;
        };
        let Some(a3) = self.translate_linear(addr.wrapping_add(3), true, bus) else {
            return;
        };
        bus.write_byte(a0, value as u8);
        bus.write_byte(a1, (value >> 8) as u8);
        bus.write_byte(a2, (value >> 16) as u8);
        bus.write_byte(a3, (value >> 24) as u8);
    }

    fn switch_task(&mut self, ntask: u16, task_type: TaskType, bus: &mut impl common::Bus) {
        if ntask & 0x0004 != 0 {
            self.raise_fault_with_code(10, Self::segment_error_code(ntask), bus);
            return;
        }

        let Some(naddr) = self.descriptor_addr_checked(ntask) else {
            self.raise_fault_with_code(10, Self::segment_error_code(ntask), bus);
            return;
        };

        let ndesc = self.decode_descriptor_at(naddr, bus);
        let ndesc_limit = ndesc.limit;
        let ndesc_base = ndesc.base;
        let ndesc_rights = ndesc.rights;

        let r = ndesc_rights;
        let tss_type = r & 0x0F;
        if Self::descriptor_is_segment(r) || !matches!(tss_type, 1 | 3 | 9 | 11) {
            self.raise_fault_with_code(13, Self::segment_error_code(ntask), bus);
            return;
        }
        if task_type == TaskType::Iret {
            if r & 0x02 == 0 {
                self.raise_fault_with_code(13, Self::segment_error_code(ntask), bus);
                return;
            }
        } else if r & 0x02 != 0 {
            self.raise_fault_with_code(13, Self::segment_error_code(ntask), bus);
            return;
        }

        if !Self::descriptor_present(r) {
            self.raise_fault_with_code(11, Self::segment_error_code(ntask), bus);
            return;
        }

        let is_386_tss = matches!(tss_type, 9 | 11);
        let min_limit: u32 = if is_386_tss { 103 } else { 43 };
        if ndesc_limit < min_limit {
            self.raise_fault_with_code(10, Self::segment_error_code(ntask), bus);
            return;
        }

        let mut flags = self.flags.compress();

        if task_type == TaskType::Iret {
            flags &= !0x4000;
        }

        // Save current state to old TSS.
        let old_base = self.tr_base;
        let old_is_386 = self.tr_rights & 0x0F >= 9;

        if old_is_386 {
            let eip = self.ip_upper | self.ip as u32;
            let eflags = self.eflags_upper | flags as u32;
            self.write_dword_linear(bus, old_base.wrapping_add(32), eip);
            self.write_dword_linear(bus, old_base.wrapping_add(36), eflags);
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(40),
                self.regs.dword(crate::DwordReg::EAX),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(44),
                self.regs.dword(crate::DwordReg::ECX),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(48),
                self.regs.dword(crate::DwordReg::EDX),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(52),
                self.regs.dword(crate::DwordReg::EBX),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(56),
                self.regs.dword(crate::DwordReg::ESP),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(60),
                self.regs.dword(crate::DwordReg::EBP),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(64),
                self.regs.dword(crate::DwordReg::ESI),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(68),
                self.regs.dword(crate::DwordReg::EDI),
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(72),
                self.sregs[SegReg32::ES as usize] as u32,
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(76),
                self.sregs[SegReg32::CS as usize] as u32,
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(80),
                self.sregs[SegReg32::SS as usize] as u32,
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(84),
                self.sregs[SegReg32::DS as usize] as u32,
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(88),
                self.sregs[SegReg32::FS as usize] as u32,
            );
            self.write_dword_linear(
                bus,
                old_base.wrapping_add(92),
                self.sregs[SegReg32::GS as usize] as u32,
            );
        } else {
            self.write_word_linear(bus, old_base.wrapping_add(14), self.ip);
            self.write_word_linear(bus, old_base.wrapping_add(16), flags);
            self.write_word_linear(bus, old_base.wrapping_add(18), self.regs.word(WordReg::AX));
            self.write_word_linear(bus, old_base.wrapping_add(20), self.regs.word(WordReg::CX));
            self.write_word_linear(bus, old_base.wrapping_add(22), self.regs.word(WordReg::DX));
            self.write_word_linear(bus, old_base.wrapping_add(24), self.regs.word(WordReg::BX));
            self.write_word_linear(bus, old_base.wrapping_add(26), self.regs.word(WordReg::SP));
            self.write_word_linear(bus, old_base.wrapping_add(28), self.regs.word(WordReg::BP));
            self.write_word_linear(bus, old_base.wrapping_add(30), self.regs.word(WordReg::SI));
            self.write_word_linear(bus, old_base.wrapping_add(32), self.regs.word(WordReg::DI));
            self.write_word_linear(
                bus,
                old_base.wrapping_add(34),
                self.sregs[SegReg32::ES as usize],
            );
            self.write_word_linear(
                bus,
                old_base.wrapping_add(36),
                self.sregs[SegReg32::CS as usize],
            );
            self.write_word_linear(
                bus,
                old_base.wrapping_add(38),
                self.sregs[SegReg32::SS as usize],
            );
            self.write_word_linear(
                bus,
                old_base.wrapping_add(40),
                self.sregs[SegReg32::DS as usize],
            );
        }

        // Write back-link to new TSS after old state is saved.
        if task_type == TaskType::Call {
            self.write_word_linear(bus, ndesc_base, self.tr);
        }

        // Read all fields from new TSS.
        let new_base = ndesc_base;

        let (
            ntss_eip,
            ntss_eflags,
            ntss_eax,
            ntss_ecx,
            ntss_edx,
            ntss_ebx,
            ntss_esp,
            ntss_ebp,
            ntss_esi,
            ntss_edi,
            ntss_es,
            ntss_cs,
            ntss_ss,
            ntss_ds,
            ntss_fs,
            ntss_gs,
            ntss_ldt,
        );

        if is_386_tss {
            ntss_eip = self.read_dword_linear(bus, new_base.wrapping_add(32));
            ntss_eflags = self.read_dword_linear(bus, new_base.wrapping_add(36));
            ntss_eax = self.read_dword_linear(bus, new_base.wrapping_add(40));
            ntss_ecx = self.read_dword_linear(bus, new_base.wrapping_add(44));
            ntss_edx = self.read_dword_linear(bus, new_base.wrapping_add(48));
            ntss_ebx = self.read_dword_linear(bus, new_base.wrapping_add(52));
            ntss_esp = self.read_dword_linear(bus, new_base.wrapping_add(56));
            ntss_ebp = self.read_dword_linear(bus, new_base.wrapping_add(60));
            ntss_esi = self.read_dword_linear(bus, new_base.wrapping_add(64));
            ntss_edi = self.read_dword_linear(bus, new_base.wrapping_add(68));
            ntss_es = self.read_word_linear(bus, new_base.wrapping_add(72));
            ntss_cs = self.read_word_linear(bus, new_base.wrapping_add(76));
            ntss_ss = self.read_word_linear(bus, new_base.wrapping_add(80));
            ntss_ds = self.read_word_linear(bus, new_base.wrapping_add(84));
            ntss_fs = self.read_word_linear(bus, new_base.wrapping_add(88));
            ntss_gs = self.read_word_linear(bus, new_base.wrapping_add(92));
            ntss_ldt = self.read_word_linear(bus, new_base.wrapping_add(96));
        } else {
            ntss_eip = self.read_word_linear(bus, new_base.wrapping_add(14)) as u32;
            ntss_eflags = self.read_word_linear(bus, new_base.wrapping_add(16)) as u32;
            ntss_eax = self.read_word_linear(bus, new_base.wrapping_add(18)) as u32;
            ntss_ecx = self.read_word_linear(bus, new_base.wrapping_add(20)) as u32;
            ntss_edx = self.read_word_linear(bus, new_base.wrapping_add(22)) as u32;
            ntss_ebx = self.read_word_linear(bus, new_base.wrapping_add(24)) as u32;
            ntss_esp = self.read_word_linear(bus, new_base.wrapping_add(26)) as u32;
            ntss_ebp = self.read_word_linear(bus, new_base.wrapping_add(28)) as u32;
            ntss_esi = self.read_word_linear(bus, new_base.wrapping_add(30)) as u32;
            ntss_edi = self.read_word_linear(bus, new_base.wrapping_add(32)) as u32;
            ntss_es = self.read_word_linear(bus, new_base.wrapping_add(34));
            ntss_cs = self.read_word_linear(bus, new_base.wrapping_add(36));
            ntss_ss = self.read_word_linear(bus, new_base.wrapping_add(38));
            ntss_ds = self.read_word_linear(bus, new_base.wrapping_add(40));
            ntss_ldt = self.read_word_linear(bus, new_base.wrapping_add(42));
            ntss_fs = 0;
            ntss_gs = 0;
        }

        // Mark old TSS idle (JMP/IRET).
        if task_type != TaskType::Call
            && let Some(oaddr) = self.descriptor_addr_checked(self.tr)
        {
            let olinear = oaddr.wrapping_add(5);
            let ophys = self.translate_linear(olinear, true, bus).unwrap_or(0);
            let old_rights = bus.read_byte(ophys);
            bus.write_byte(ophys, old_rights & !0x02);
        }

        // Mark new TSS busy (CALL/JMP).
        if task_type != TaskType::Iret {
            let nlinear = naddr.wrapping_add(5);
            let nphys = self.translate_linear(nlinear, true, bus).unwrap_or(0);
            let new_rights = bus.read_byte(nphys);
            bus.write_byte(nphys, new_rights | 0x02);
        }

        // Update TR.
        self.tr = ntask;
        self.tr_limit = ndesc_limit;
        self.tr_base = ndesc_base;
        let nlinear5 = naddr.wrapping_add(5);
        let nphys5 = self.translate_linear(nlinear5, false, bus).unwrap_or(0);
        self.tr_rights = bus.read_byte(nphys5);

        // Load registers from new TSS.
        self.flags.expand(ntss_eflags as u16);
        self.eflags_upper = ntss_eflags & 0xFFFF_0000;
        self.regs.set_dword(crate::DwordReg::EAX, ntss_eax);
        self.regs.set_dword(crate::DwordReg::ECX, ntss_ecx);
        self.regs.set_dword(crate::DwordReg::EDX, ntss_edx);
        self.regs.set_dword(crate::DwordReg::EBX, ntss_ebx);
        self.regs.set_dword(crate::DwordReg::ESP, ntss_esp);
        self.regs.set_dword(crate::DwordReg::EBP, ntss_ebp);
        self.regs.set_dword(crate::DwordReg::ESI, ntss_esi);
        self.regs.set_dword(crate::DwordReg::EDI, ntss_edi);

        // Load LDT from new TSS.
        if ntss_ldt & 0x0004 != 0 {
            self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
            return;
        }
        if ntss_ldt & 0xFFFC != 0 {
            let Some(ldtaddr) = self.descriptor_addr_checked(ntss_ldt) else {
                self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
                return;
            };
            let ldt_desc = self.decode_descriptor_at(ldtaddr, bus);
            let lr = ldt_desc.rights;
            if Self::descriptor_is_segment(lr) || (lr & 0x0F) != 0x02 {
                self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
                return;
            }
            if !Self::descriptor_present(lr) {
                self.raise_fault_with_code(10, Self::segment_error_code(ntss_ldt), bus);
                return;
            }
            let ldt_base = ldt_desc.base;
            let ldt_limit = ldt_desc.limit;
            self.ldtr = ntss_ldt;
            self.ldtr_base = ldt_base;
            self.ldtr_limit = ldt_limit;
        } else {
            self.ldtr = 0;
            self.ldtr_base = 0;
            self.ldtr_limit = 0;
        }

        if task_type == TaskType::Call {
            self.flags.nt = true;
        }

        self.cr0 |= 8;

        // TSS T-bit (offset 100, bit 0): schedule a debug trap (#DB) after the
        // first instruction of the new task. Sets DR6.BT (bit 15).
        if is_386_tss {
            let debug_trap_word = self.read_word_linear(bus, new_base.wrapping_add(100));
            if debug_trap_word & 1 != 0 {
                self.debug_trap_pending = true;
            }
        }

        if is_386_tss && (ntss_eflags & 0x0002_0000) != 0 {
            // VM86 task: segment selectors are interpreted with real-mode
            // bases/limits, not protected-mode descriptors.
            self.sregs[SegReg32::ES as usize] = ntss_es;
            self.set_real_segment_cache(SegReg32::ES, ntss_es);
            self.sregs[SegReg32::CS as usize] = ntss_cs;
            self.set_real_segment_cache(SegReg32::CS, ntss_cs);
            self.sregs[SegReg32::SS as usize] = ntss_ss;
            self.set_real_segment_cache(SegReg32::SS, ntss_ss);
            self.sregs[SegReg32::DS as usize] = ntss_ds;
            self.set_real_segment_cache(SegReg32::DS, ntss_ds);
            self.sregs[SegReg32::FS as usize] = ntss_fs;
            self.set_real_segment_cache(SegReg32::FS, ntss_fs);
            self.sregs[SegReg32::GS as usize] = ntss_gs;
            self.set_real_segment_cache(SegReg32::GS, ntss_gs);
            self.ip = ntss_eip as u16;
            self.ip_upper = 0;
            return;
        }

        // Load segment registers from new TSS. SS first (uses new CS RPL as CPL).
        let new_cpl = ntss_cs & 3;
        self.load_task_data_segment(SegReg32::SS, ntss_ss, new_cpl, bus);
        self.load_task_code_segment(ntss_cs, ntss_eip, bus);
        let cpl = self.cpl();
        self.load_task_data_segment(SegReg32::ES, ntss_es, cpl, bus);
        self.load_task_data_segment(SegReg32::DS, ntss_ds, cpl, bus);
        self.load_task_data_segment(SegReg32::FS, ntss_fs, cpl, bus);
        self.load_task_data_segment(SegReg32::GS, ntss_gs, cpl, bus);
    }

    fn load_task_data_segment(
        &mut self,
        seg: SegReg32,
        selector: u16,
        required_cpl: u16,
        bus: &mut impl common::Bus,
    ) {
        if selector & 0xFFFC == 0 {
            self.set_null_segment(seg, selector);
            return;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        };
        let rights = descriptor.rights;
        if seg == SegReg32::SS {
            if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_writable(rights) {
                self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                return;
            }
            let dpl = Self::descriptor_dpl(rights);
            let rpl = selector & 3;
            if dpl != required_cpl || rpl != required_cpl {
                self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                return;
            }
        } else {
            if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_readable(rights) {
                self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                return;
            }
            if !Self::descriptor_is_conforming_code(rights) {
                let dpl = Self::descriptor_dpl(rights);
                let rpl = selector & 3;
                if dpl < required_cpl.max(rpl) {
                    self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
                    return;
                }
            }
        }
        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        }
        self.set_accessed_bit(selector, bus);
        self.set_loaded_segment_cache(seg, selector, descriptor);
    }

    fn load_task_code_segment(&mut self, selector: u16, offset: u32, bus: &mut impl common::Bus) {
        if selector & 0xFFFC == 0 {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        }
        let Some(descriptor) = self.decode_descriptor(selector, bus) else {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        };
        let rights = descriptor.rights;
        if !Self::descriptor_is_segment(rights) || !Self::descriptor_is_code(rights) {
            self.raise_fault_with_code(10, Self::segment_error_code(selector), bus);
            return;
        }
        if !Self::descriptor_present(rights) {
            self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
            return;
        }
        if offset > descriptor.limit {
            self.raise_fault_with_code(10, 0, bus);
            return;
        }
        self.set_accessed_bit(selector, bus);
        let cpl = selector & 3;
        let adjusted = (selector & !3) | cpl;
        self.set_loaded_segment_cache(SegReg32::CS, adjusted, descriptor);
        self.ip = offset as u16;
        self.ip_upper = offset & 0xFFFF_0000;
    }

    fn code_descriptor(
        &mut self,
        selector: u16,
        offset: u32,
        gate: TaskType,
        old_cs: u16,
        old_eip: u32,
        bus: &mut impl common::Bus,
    ) -> bool {
        let Some(addr) = self.descriptor_addr_checked(selector) else {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        };

        let desc = self.decode_descriptor_at(addr, bus);
        let r = desc.rights;
        let cpl = self.cpl();
        let rpl = selector & 3;

        if Self::descriptor_is_segment(r) {
            if !Self::descriptor_is_code(r) {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                return false;
            }
            if Self::descriptor_is_conforming_code(r) {
                if Self::descriptor_dpl(r) > cpl {
                    self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                    return false;
                }
            } else if rpl > cpl || Self::descriptor_dpl(r) != cpl {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                return false;
            }
            if !Self::descriptor_present(r) {
                self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
                return false;
            }
            if offset > desc.limit {
                self.raise_fault_with_code(13, 0, bus);
                return false;
            }
            self.set_accessed_bit(selector, bus);
            let adjusted = (selector & !3) | cpl;
            self.set_loaded_segment_cache(SegReg32::CS, adjusted, desc);
            self.ip = offset as u16;
            self.ip_upper = offset & 0xFFFF_0000;
            if gate == TaskType::Call {
                if self.operand_size_override {
                    self.push_dword(bus, old_cs as u32);
                    self.push_dword(bus, old_eip);
                } else {
                    self.push(bus, old_cs);
                    self.push(bus, old_eip as u16);
                }
            }
            return true;
        }

        // System descriptor.
        let dpl = Self::descriptor_dpl(r);
        if dpl < cpl.max(rpl) {
            self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
            return false;
        }
        if !Self::descriptor_present(r) {
            self.raise_fault_with_code(11, Self::segment_error_code(selector), bus);
            return false;
        }

        let gate_type = r & 0x0F;
        match gate_type {
            4 | 12 => {
                // Call gate (4=286, 12=386).
                let is_386_gate = gate_type == 12;
                let gate_selector = self.read_word_linear(bus, addr.wrapping_add(2));
                let gc_linear = addr.wrapping_add(4);
                let gc_phys = self.translate_linear(gc_linear, false, bus).unwrap_or(0);
                let gate_count = bus.read_byte(gc_phys) & 0x1F;
                let gate_offset = if is_386_gate {
                    let lo = self.read_word_linear(bus, addr);
                    let hi = self.read_word_linear(bus, addr.wrapping_add(6));
                    lo as u32 | ((hi as u32) << 16)
                } else {
                    self.read_word_linear(bus, addr) as u32
                };

                let Some(target_addr) = self.descriptor_addr_checked(gate_selector) else {
                    self.raise_fault_with_code(13, Self::segment_error_code(gate_selector), bus);
                    return false;
                };
                let target_desc = self.decode_descriptor_at(target_addr, bus);
                let tr = target_desc.rights;
                if !Self::descriptor_is_code(tr) || !Self::descriptor_is_segment(tr) {
                    self.raise_fault_with_code(13, Self::segment_error_code(gate_selector), bus);
                    return false;
                }
                let target_dpl = Self::descriptor_dpl(tr);
                if target_dpl > cpl {
                    self.raise_fault_with_code(13, Self::segment_error_code(gate_selector), bus);
                    return false;
                }
                if !Self::descriptor_present(tr) {
                    self.raise_fault_with_code(11, Self::segment_error_code(gate_selector), bus);
                    return false;
                }
                if gate_offset > target_desc.limit {
                    self.raise_fault_with_code(13, 0, bus);
                    return false;
                }

                if !Self::descriptor_is_conforming_code(tr) && target_dpl < cpl {
                    // Inter-privilege call via call gate.
                    if gate == TaskType::Jmp {
                        self.raise_fault_with_code(
                            13,
                            Self::segment_error_code(gate_selector),
                            bus,
                        );
                        return false;
                    }

                    let is_386_tss = self.tr_rights & 0x0F >= 9;
                    let tss_sp_offset = if is_386_tss {
                        4 + target_dpl as u32 * 8
                    } else {
                        2 + target_dpl as u32 * 4
                    };
                    let tss_ss_offset = tss_sp_offset + if is_386_tss { 4 } else { 2 };
                    let tss_esp = if is_386_tss {
                        self.read_dword_linear(bus, self.tr_base.wrapping_add(tss_sp_offset))
                    } else {
                        self.read_word_linear(bus, self.tr_base.wrapping_add(tss_sp_offset)) as u32
                    };
                    let tss_ss =
                        self.read_word_linear(bus, self.tr_base.wrapping_add(tss_ss_offset));

                    let saved_ss = self.sregs[SegReg32::SS as usize];
                    let saved_esp = if self.use_esp() {
                        self.regs.dword(crate::DwordReg::ESP)
                    } else {
                        self.regs.word(WordReg::SP) as u32
                    };
                    let old_ss_base = self.seg_base(SegReg32::SS);

                    self.load_task_data_segment(SegReg32::SS, tss_ss, target_dpl, bus);
                    if self.use_esp() {
                        self.regs.set_dword(crate::DwordReg::ESP, tss_esp);
                    } else {
                        self.regs.set_word(WordReg::SP, tss_esp as u16);
                    }

                    if is_386_gate {
                        self.push_dword(bus, saved_ss as u32);
                        self.push_dword(bus, saved_esp);
                        for i in (0..gate_count as u32).rev() {
                            let param = self.read_dword_linear(
                                bus,
                                old_ss_base.wrapping_add(saved_esp.wrapping_add(i * 4)),
                            );
                            self.push_dword(bus, param);
                        }
                    } else {
                        self.push(bus, saved_ss);
                        self.push(bus, saved_esp as u16);
                        for i in (0..gate_count as u16).rev() {
                            let param = self.read_word_linear(
                                bus,
                                old_ss_base
                                    .wrapping_add((saved_esp as u16).wrapping_add(i * 2) as u32),
                            );
                            self.push(bus, param);
                        }
                    }
                }

                self.set_accessed_bit(gate_selector, bus);
                let adjusted = (gate_selector & !3) | target_dpl;
                self.set_loaded_segment_cache(SegReg32::CS, adjusted, target_desc);
                self.ip = gate_offset as u16;
                self.ip_upper = gate_offset & 0xFFFF_0000;
                if gate == TaskType::Call {
                    if is_386_gate {
                        self.push_dword(bus, old_cs as u32);
                        self.push_dword(bus, old_eip);
                    } else {
                        self.push(bus, old_cs);
                        self.push(bus, old_eip as u16);
                    }
                }
                true
            }
            5 => {
                // Task gate.
                let task_selector = self.read_word_linear(bus, addr.wrapping_add(2));
                self.switch_task(task_selector, gate, bus);
                let flags_val = self.flags.compress();
                let cpl = self.cpl();
                self.flags.load_flags(flags_val, cpl, true);
                true
            }
            1 | 9 => {
                // Available TSS descriptor.
                self.switch_task(selector, gate, bus);
                let flags_val = self.flags.compress();
                let cpl = self.cpl();
                self.flags.load_flags(flags_val, cpl, true);
                true
            }
            3 | 11 => {
                // Busy TSS.
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                false
            }
            _ => {
                self.raise_fault_with_code(13, Self::segment_error_code(selector), bus);
                false
            }
        }
    }

    fn decode_descriptor_at(&mut self, addr: u32, bus: &mut impl common::Bus) -> SegmentDescriptor {
        fn translate_byte<const M: u8>(
            cpu: &mut I386<M>,
            bus: &mut impl common::Bus,
            linear: u32,
        ) -> u8 {
            let phys = cpu.translate_linear(linear, false, bus).unwrap_or(0);
            bus.read_byte(phys)
        }
        let b0 = translate_byte::<CPU_MODEL>(self, bus, addr);
        let b1 = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(1));
        let b2 = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(2));
        let b3 = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(3));
        let b4 = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(4));
        let rights = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(5));
        let b6 = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(6));
        let b7 = translate_byte::<CPU_MODEL>(self, bus, addr.wrapping_add(7));

        let raw_limit = b0 as u32 | ((b1 as u32) << 8) | (((b6 & 0x0F) as u32) << 16);
        let base = b2 as u32 | ((b3 as u32) << 8) | ((b4 as u32) << 16) | ((b7 as u32) << 24);
        let limit = if b6 & 0x80 != 0 {
            (raw_limit << 12) | 0xFFF
        } else {
            raw_limit
        };
        SegmentDescriptor {
            base,
            limit,
            rights,
            granularity: b6,
        }
    }

    fn execute_one(&mut self, bus: &mut impl common::Bus) {
        self.prefetch_valid &= self.rep_active | self.rep_completed;
        self.rep_completed = false;
        self.prev_ip = self.ip;
        self.prev_ip_upper = self.ip_upper;
        self.fault_pending = false;

        if self.pending_irq != 0 {
            self.check_interrupts(bus);
        }
        if self.no_interrupt > 0 {
            self.no_interrupt -= 1;
        }
        let inhibit = self.inhibit_all > 0;
        if self.inhibit_all > 0 {
            self.inhibit_all -= 1;
        }

        let tf_was_set = self.flags.tf;

        self.seg_prefix = false;
        self.lock_prefix = false;
        let d_bit = self.code_segment_32bit();
        self.operand_size_override = d_bit;
        self.address_size_override = d_bit;

        if self.rep_active {
            self.continue_rep(bus);
        } else {
            let mut opcode = self.fetch(bus);
            while !self.fault_pending {
                match opcode {
                    0x26 => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg32::ES;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x2E => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg32::CS;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x36 => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg32::SS;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x3E => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg32::DS;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x64 => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg32::FS;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x65 => {
                        self.seg_prefix = true;
                        self.prefix_seg = SegReg32::GS;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x66 => {
                        self.operand_size_override = !self.code_segment_32bit();
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0x67 => {
                        self.address_size_override = !self.code_segment_32bit();
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    0xF0 => {
                        self.lock_prefix = true;
                        self.clk(Self::timing(0, 1));
                        opcode = self.fetch(bus);
                    }
                    _ => {
                        if self.lock_prefix && opcode != 0x0F && !Self::is_lockable(opcode) {
                            self.raise_fault(6, bus);
                        } else {
                            self.dispatch(opcode, bus);
                        }
                        break;
                    }
                }
            }
        }

        if tf_was_set && !self.fault_pending && !inhibit {
            self.raise_trap(1, bus);
        }

        if self.debug_trap_pending && !self.fault_pending {
            self.debug_trap_pending = false;
            self.dr6 |= 0x8000; // BT (bit 15) - task switch debug trap
            self.raise_trap(1, bus);
        }
    }

    /// Executes exactly one logical instruction (should only be used in tests).
    pub fn step(&mut self, bus: &mut impl common::Bus) {
        self.cycles_remaining = i64::MAX;
        self.execute_one(bus);
    }

    /// Returns the number of cycles consumed by the last `step()` call.
    pub fn cycles_consumed(&self) -> u64 {
        (i64::MAX - self.cycles_remaining) as u64
    }

    /// Returns the last computed effective address (for alignment checks).
    pub fn last_ea(&self) -> u32 {
        self.ea
    }

    /// Signals a maskable interrupt request (IRQ).
    pub fn signal_irq(&mut self) {
        self.pending_irq |= crate::PENDING_IRQ;
    }

    /// Signals a non-maskable interrupt (NMI).
    pub fn signal_nmi(&mut self) {
        self.pending_irq |= crate::PENDING_NMI;
    }

    /// Returns `true` if an IRQ or NMI is pending delivery.
    pub fn has_pending_interrupt(&self) -> bool {
        self.pending_irq != 0
    }

    /// Ticks the post-STI/POPF interrupt inhibit window by one instruction.
    ///
    /// The interpreter decrements `no_interrupt` at the top of every
    /// `execute_one`; the JIT must call this after each directly-executed
    /// instruction (e.g. block exits that bypass `execute_one` such as
    /// `HLT`) so that the inhibit window closes on schedule.
    pub fn tick_interrupt_window(&mut self) {
        if self.no_interrupt > 0 {
            self.no_interrupt -= 1;
        }
        if self.inhibit_all > 0 {
            self.inhibit_all -= 1;
        }
    }

    /// Executes exactly one logical instruction against the current guest state
    /// and cycle budget. Unlike [`step`](Self::step), this does not touch
    /// `cycles_remaining`, so the caller's budget accounting is preserved.
    ///
    /// The JIT fallback path calls this to delegate unsupported opcodes to the
    /// interpreter without disturbing block-level cycle accounting.
    pub fn step_instruction(&mut self, bus: &mut impl common::Bus) {
        self.execute_one(bus);
    }

    /// Returns the physical linear address of the byte at CS:EIP, or `None`
    /// if the address faults during translation. On fault, `fault_pending` is
    /// set as a side effect. The JIT uses this as the block lookup key.
    pub fn current_eip_phys(&mut self, bus: &mut impl common::Bus) -> Option<u32> {
        let linear = self
            .state
            .seg_bases
            .get(SegReg32::CS as usize)
            .copied()
            .unwrap_or(0)
            .wrapping_add(self.state.eip());
        self.translate_linear(linear, false, bus)
    }

    /// Returns the remaining cycle budget held by the CPU core.
    pub fn cycles_remaining(&self) -> i64 {
        self.cycles_remaining
    }

    /// Sets the remaining cycle budget held by the CPU core.
    pub fn set_cycles_remaining(&mut self, cycles: i64) {
        self.cycles_remaining = cycles;
    }

    /// Returns the cycle count recorded at the start of the current `run_for`
    /// invocation.
    pub fn run_start_cycle(&self) -> u64 {
        self.run_start_cycle
    }

    /// Sets the cycle count recorded at the start of the current `run_for`
    /// invocation. The JIT dispatcher uses this to mirror the interpreter's
    /// accounting semantics.
    pub fn set_run_start_cycle(&mut self, cycle: u64) {
        self.run_start_cycle = cycle;
    }

    /// Returns the budget requested for the current `run_for` invocation.
    pub fn run_budget(&self) -> u64 {
        self.run_budget
    }

    /// Sets the budget requested for the current `run_for` invocation.
    pub fn set_run_budget(&mut self, budget: u64) {
        self.run_budget = budget;
    }

    /// Returns `true` if the CPU is currently halted.
    pub fn halted_flag(&self) -> bool {
        self.halted
    }

    /// Sets the halted flag. Used by the JIT dispatcher when a guest HLT
    /// instruction is executed as a block exit.
    pub fn set_halted_flag(&mut self, halted: bool) {
        self.halted = halted;
    }

    /// Returns whether `HLT` can be handled directly by the JIT.
    ///
    /// Ring-3 `HLT` in protected mode raises #GP, so the JIT keeps those
    /// cases on the interpreter path for exact exception delivery.
    pub fn jit_can_halt(&self) -> bool {
        !self.is_protected_mode() || self.cpl() == 0
    }

    /// Returns `true` if a fault was raised during the most recent execution
    /// step. The JIT consults this after memory/IO helper calls and after
    /// interpreter fallbacks to detect exception delivery.
    pub fn fault_pending(&self) -> bool {
        self.fault_pending
    }

    /// Clears any `fault_pending` residue. The JIT calls this between
    /// compiled blocks because `translate_linear` leaves the flag set
    /// after a page fault has been delivered. The interpreter's
    /// `execute_one` does the same at the top of every step.
    pub fn clear_fault_pending(&mut self) {
        self.fault_pending = false;
    }

    /// Applies the interpreter's segment-access checks for a JIT memory
    /// operand without performing the subsequent translation or memory access.
    pub fn jit_check_segment_access(
        &mut self,
        bus: &mut impl common::Bus,
        seg: SegReg32,
        offset: u32,
        size: u8,
        write: bool,
    ) -> bool {
        self.check_segment_access(seg, offset, u32::from(size), write, bus)
    }

    /// Returns `true` when the code segment is currently 32-bit (CS D-bit set
    /// and running outside virtual-8086 mode). The JIT needs this at block
    /// decode time to pick the default operand/address size.
    pub fn is_code_segment_32bit(&self) -> bool {
        self.code_segment_32bit()
    }

    /// Returns `true` when the stack segment is currently 32-bit (SS B-bit set).
    /// Determines whether PUSH/POP use ESP or SP.
    pub fn is_stack_segment_32bit(&self) -> bool {
        self.use_esp()
    }

    /// Captures `ip`/`ip_upper` into `prev_ip`/`prev_ip_upper` so a
    /// subsequent fault raised by a JIT helper (PUSHF/POPF in V8086
    /// with IOPL<3, stack faults from PUSHA/POPA, page faults from
    /// memory ops, etc.) reports the correct faulting EIP. The
    /// interpreter's `execute_one` does this at the top of every step;
    /// the JIT must refresh it at block entry so a fault raised by the
    /// first instruction's ops pushes the block-start EIP rather than
    /// whatever the last interpreter fallback left behind. Call from
    /// the JIT block dispatcher once per block.
    pub fn jit_refresh_prev_ip(&mut self) {
        self.prev_ip = self.ip;
        self.prev_ip_upper = self.ip_upper;
    }

    /// Re-raises `fault_pending` if the interpreter helper delivered a
    /// fault during a JIT helper call. `raise_fault_with_code` clears
    /// `fault_pending` after successful delivery, which is correct for
    /// the interpreter (the handler's EIP is live and the next step
    /// starts clean) but loses the signal the JIT needs to bail out of
    /// the current compiled block.
    ///
    /// We use the pre-call EIP snapshot to detect delivery: the JIT
    /// helper's semantic primitive (pushf, popf, pusha, popa) never
    /// modifies EIP on its own, so a mismatch implies the interpreter
    /// ran an IDT gate. Callers must capture `pre_eip` before invoking
    /// the primitive.
    fn jit_signal_if_faulted(&mut self, pre_eip: u32) {
        if self.state.eip() != pre_eip {
            self.fault_pending = true;
        }
    }

    /// Executes `PUSHA`/`PUSHAD` via the interpreter's stack primitives.
    /// Cycles are accounted by the JIT at block-entry.
    pub fn jit_pusha(&mut self, bus: &mut impl common::Bus, size: u8) {
        self.jit_refresh_prev_ip();
        let pre_eip = self.state.eip();
        let saved = (
            self.operand_size_override,
            self.seg_prefix,
            self.address_size_override,
        );
        self.operand_size_override = size == 4;
        self.seg_prefix = false;
        self.address_size_override = false;
        self.pusha(bus);
        self.operand_size_override = saved.0;
        self.seg_prefix = saved.1;
        self.address_size_override = saved.2;
        self.jit_signal_if_faulted(pre_eip);
    }

    /// Executes `POPA`/`POPAD` via the interpreter's stack primitives.
    pub fn jit_popa(&mut self, bus: &mut impl common::Bus, size: u8) {
        self.jit_refresh_prev_ip();
        let pre_eip = self.state.eip();
        let saved = (
            self.operand_size_override,
            self.seg_prefix,
            self.address_size_override,
        );
        self.operand_size_override = size == 4;
        self.seg_prefix = false;
        self.address_size_override = false;
        self.popa(bus);
        self.operand_size_override = saved.0;
        self.seg_prefix = saved.1;
        self.address_size_override = saved.2;
        self.jit_signal_if_faulted(pre_eip);
    }

    /// Executes `PUSHF`/`PUSHFD` via the interpreter, including the V8086
    /// IOPL check that can raise `#GP`.
    pub fn jit_pushf(&mut self, bus: &mut impl common::Bus, size: u8) {
        self.jit_refresh_prev_ip();
        let pre_eip = self.state.eip();
        let saved = self.operand_size_override;
        self.operand_size_override = size == 4;
        self.pushf(bus);
        self.operand_size_override = saved;
        self.jit_signal_if_faulted(pre_eip);
    }

    /// Executes `POPF`/`POPFD` via the interpreter, including the V8086
    /// IOPL check and CPL-based flag masking.
    pub fn jit_popf(&mut self, bus: &mut impl common::Bus, size: u8) {
        self.jit_refresh_prev_ip();
        let pre_eip = self.state.eip();
        let saved = self.operand_size_override;
        self.operand_size_override = size == 4;
        self.popf(bus);
        self.operand_size_override = saved;
        self.jit_signal_if_faulted(pre_eip);
    }

    /// Computes `NEG value` under the given operand size, updating flags
    /// exactly like the interpreter's `alu_neg_*` helpers.
    pub fn jit_neg(&mut self, value: u32, size: u8) -> u32 {
        match size {
            1 => u32::from(self.alu_neg_byte(value as u8)),
            2 => u32::from(self.alu_neg_word(value as u16)),
            4 => self.alu_neg_dword(value),
            _ => unreachable!("jit_neg: unsupported size {size}"),
        }
    }

    /// Executes one-operand `MUL`/`IMUL` on the accumulator pair, using
    /// `value` as the multiplicand. Updates AH/DX/EDX and CF/OF.
    pub fn jit_mul(&mut self, value: u32, size: u8, signed: bool) {
        match (size, signed) {
            (1, false) => {
                let al = self.state.regs.byte(ByteReg::AL);
                let result = u16::from(al) * u16::from(value as u8);
                self.state.regs.set_word(WordReg::AX, result);
                self.state.flags.carry_val = if result & 0xFF00 != 0 { 1 } else { 0 };
                self.state.flags.overflow_val = self.state.flags.carry_val;
            }
            (1, true) => {
                let al = self.state.regs.byte(ByteReg::AL) as i8 as i16;
                let src = (value as u8) as i8 as i16;
                let result = al * src;
                self.state.regs.set_word(WordReg::AX, result as u16);
                let ah = (result >> 8) as i8;
                let al_signed = result as i8;
                self.state.flags.carry_val = if ah != (al_signed >> 7) { 1 } else { 0 };
                self.state.flags.overflow_val = self.state.flags.carry_val;
            }
            (2, false) => {
                let ax = self.state.regs.word(WordReg::AX);
                let result = u32::from(ax) * u32::from(value as u16);
                self.state.regs.set_word(WordReg::AX, result as u16);
                self.state.regs.set_word(WordReg::DX, (result >> 16) as u16);
                self.state.flags.carry_val = u32::from((result >> 16) != 0);
                self.state.flags.overflow_val = self.state.flags.carry_val;
            }
            (2, true) => {
                let ax = self.state.regs.word(WordReg::AX) as i16 as i32;
                let src = (value as u16) as i16 as i32;
                let result = ax * src;
                self.state.regs.set_word(WordReg::AX, result as u16);
                self.state.regs.set_word(WordReg::DX, (result >> 16) as u16);
                let lower_signed = (result as i16) as i32;
                self.state.flags.carry_val = u32::from(result != lower_signed);
                self.state.flags.overflow_val = self.state.flags.carry_val;
            }
            (4, false) => {
                let eax = self.state.regs.dword(DwordReg::EAX);
                let result = u64::from(eax) * u64::from(value);
                self.state.regs.set_dword(DwordReg::EAX, result as u32);
                self.state
                    .regs
                    .set_dword(DwordReg::EDX, (result >> 32) as u32);
                self.state.flags.carry_val = u32::from((result >> 32) != 0);
                self.state.flags.overflow_val = self.state.flags.carry_val;
            }
            (4, true) => {
                let eax = self.state.regs.dword(DwordReg::EAX) as i32 as i64;
                let src = value as i32 as i64;
                let result = eax * src;
                self.state.regs.set_dword(DwordReg::EAX, result as u32);
                self.state
                    .regs
                    .set_dword(DwordReg::EDX, (result >> 32) as u32);
                let lower_signed = (result as i32) as i64;
                self.state.flags.carry_val = u32::from(result != lower_signed);
                self.state.flags.overflow_val = self.state.flags.carry_val;
            }
            _ => unreachable!("jit_mul: unsupported size {size}"),
        }
    }

    /// Executes one-operand `DIV`/`IDIV`. Raises `#DE` via the normal
    /// interpreter path on divide-by-zero or quotient overflow; callers
    /// must check [`fault_pending`](Self::fault_pending) after return.
    pub fn jit_div(&mut self, value: u32, size: u8, signed: bool, bus: &mut impl common::Bus) {
        match (size, signed) {
            (1, false) => {
                let src = value as u8 as u16;
                if src == 0 {
                    self.raise_fault(0, bus);
                    return;
                }
                let ax = self.state.regs.word(WordReg::AX);
                let quotient = ax / src;
                if quotient > 0xFF {
                    self.raise_fault(0, bus);
                    return;
                }
                let remainder = ax % src;
                self.state.regs.set_byte(ByteReg::AL, quotient as u8);
                self.state.regs.set_byte(ByteReg::AH, remainder as u8);
                self.state.flags.carry_val = u32::from(!self.state.flags.cf());
                self.state.flags.aux_val ^= 0x10;
            }
            (1, true) => {
                let src = (value as u8) as i8 as i16;
                if src == 0 {
                    self.raise_fault(0, bus);
                    return;
                }
                let ax = self.state.regs.word(WordReg::AX) as i16;
                let Some(quotient) = ax.checked_div(src) else {
                    self.raise_fault(0, bus);
                    return;
                };
                if !(-128..=127).contains(&quotient) {
                    self.raise_fault(0, bus);
                    return;
                }
                let remainder = ax.checked_rem(src).unwrap_or(0);
                self.state.regs.set_byte(ByteReg::AL, quotient as u8);
                self.state.regs.set_byte(ByteReg::AH, remainder as u8);
                self.state.flags.carry_val = u32::from(!self.state.flags.cf());
                self.state.flags.aux_val ^= 0x10;
            }
            (2, false) => {
                let src = u32::from(value as u16);
                if src == 0 {
                    self.raise_fault(0, bus);
                    return;
                }
                let dx = u32::from(self.state.regs.word(WordReg::DX));
                let ax = u32::from(self.state.regs.word(WordReg::AX));
                let dividend = (dx << 16) | ax;
                let quotient = dividend / src;
                if quotient > 0xFFFF {
                    self.raise_fault(0, bus);
                    return;
                }
                let remainder = dividend % src;
                self.state.regs.set_word(WordReg::AX, quotient as u16);
                self.state.regs.set_word(WordReg::DX, remainder as u16);
                self.state.flags.carry_val = u32::from(!self.state.flags.cf());
                self.state.flags.aux_val ^= 0x10;
            }
            (2, true) => {
                let src = (value as u16) as i16 as i32;
                if src == 0 {
                    self.raise_fault(0, bus);
                    return;
                }
                let dx = u32::from(self.state.regs.word(WordReg::DX));
                let ax = u32::from(self.state.regs.word(WordReg::AX));
                let dividend = ((dx << 16) | ax) as i32;
                let Some(quotient) = dividend.checked_div(src) else {
                    self.raise_fault(0, bus);
                    return;
                };
                if !(i16::MIN as i32..=i16::MAX as i32).contains(&quotient) {
                    self.raise_fault(0, bus);
                    return;
                }
                let remainder = dividend.checked_rem(src).unwrap_or(0);
                self.state.regs.set_word(WordReg::AX, quotient as u16);
                self.state.regs.set_word(WordReg::DX, remainder as u16);
                self.state.flags.carry_val = u32::from(!self.state.flags.cf());
                self.state.flags.aux_val ^= 0x10;
            }
            (4, false) => {
                let src = u64::from(value);
                if src == 0 {
                    self.raise_fault(0, bus);
                    return;
                }
                let edx = u64::from(self.state.regs.dword(DwordReg::EDX));
                let eax = u64::from(self.state.regs.dword(DwordReg::EAX));
                let dividend = (edx << 32) | eax;
                let quotient = dividend / src;
                if quotient > u32::MAX as u64 {
                    self.raise_fault(0, bus);
                    return;
                }
                let remainder = dividend % src;
                self.state.regs.set_dword(DwordReg::EAX, quotient as u32);
                self.state.regs.set_dword(DwordReg::EDX, remainder as u32);
                self.state.flags.carry_val = u32::from(!self.state.flags.cf());
                self.state.flags.aux_val ^= 0x10;
            }
            (4, true) => {
                let src = (value as i32) as i64;
                if src == 0 {
                    self.raise_fault(0, bus);
                    return;
                }
                let edx = u64::from(self.state.regs.dword(DwordReg::EDX));
                let eax = u64::from(self.state.regs.dword(DwordReg::EAX));
                let dividend = ((edx << 32) | eax) as i64;
                let Some(quotient) = dividend.checked_div(src) else {
                    self.raise_fault(0, bus);
                    return;
                };
                if !(i32::MIN as i64..=i32::MAX as i64).contains(&quotient) {
                    self.raise_fault(0, bus);
                    return;
                }
                let remainder = dividend.checked_rem(src).unwrap_or(0);
                self.state.regs.set_dword(DwordReg::EAX, quotient as u32);
                self.state.regs.set_dword(DwordReg::EDX, remainder as u32);
                self.state.flags.carry_val = u32::from(!self.state.flags.cf());
                self.state.flags.aux_val ^= 0x10;
            }
            _ => unreachable!("jit_div: unsupported size {size}"),
        }
    }
}

impl<const CPU_MODEL: u8> common::Cpu for I386<CPU_MODEL> {
    crate::impl_cpu_run_for!();

    fn reset(&mut self) {
        self.state = I386State::default();
        self.sregs[SegReg32::CS as usize] = 0xFFFF;
        match CPU_MODEL {
            CPU_MODEL_386 => {
                // 386DX reset: ET=1 (80387 coprocessor present).
                self.cr0 = 0x0000_0010;
            }
            CPU_MODEL_486 => {
                // 486DX reset: ET=1 (on-chip FPU present). ET is hardwired on 486.
                self.cr0 = 0x0000_0010;
            }
            _ => {
                unreachable!("Unhandled CPU_MODEL")
            }
        }
        self.dr6 = 0xFFFF_0FF0;

        self.idt_limit = 0x03FF;

        for seg_idx in 0..6 {
            let seg = SegReg32::from_index(seg_idx);
            self.set_real_segment_cache(seg, self.sregs[seg as usize]);
        }
        self.seg_bases[SegReg32::CS as usize] = 0xFFFF0;

        self.prev_ip = 0;
        self.prev_ip_upper = 0;
        self.halted = false;
        self.pending_irq = 0;
        self.no_interrupt = 0;
        self.inhibit_all = 0;
        self.rep_active = false;
        self.rep_completed = false;
        self.rep_restart_ip = 0;
        self.rep_type = 0;
        self.rep_operand_size_override = false;
        self.rep_address_size_override = false;
        self.seg_prefix = false;
        self.operand_size_override = false;
        self.address_size_override = false;
        self.ea = 0;
        self.eo = 0;
        self.eo32 = 0;
        self.ea_seg = SegReg32::DS;
        self.fetch_page_valid = false;
        self.fetch_page_tag = 0;
        self.fetch_page_phys = 0;
        self.prefetch_valid = false;
        self.prefetch_addr = 0;
        self.prefetch_byte = 0;
        self.trap_level = 0;
        self.shutdown = false;
    }

    fn halted(&self) -> bool {
        self.halted || self.shutdown
    }

    fn warm_reset(&mut self, ss: u16, sp: u16, cs: u16, ip: u16) {
        self.reset();
        self.sregs[SegReg32::SS as usize] = ss;
        self.set_real_segment_cache(SegReg32::SS, ss);
        self.regs.set_word(WordReg::SP, sp);
        self.sregs[SegReg32::CS as usize] = cs;
        self.set_real_segment_cache(SegReg32::CS, cs);
        self.ip = ip;
    }

    fn ax(&self) -> u16 {
        self.state.eax() as u16
    }

    fn set_ax(&mut self, v: u16) {
        let upper = self.state.eax() & 0xFFFF_0000;
        self.state.set_eax(upper | v as u32);
    }

    fn bx(&self) -> u16 {
        self.state.ebx() as u16
    }

    fn set_bx(&mut self, v: u16) {
        let upper = self.state.ebx() & 0xFFFF_0000;
        self.state.set_ebx(upper | v as u32);
    }

    fn cx(&self) -> u16 {
        self.state.ecx() as u16
    }

    fn set_cx(&mut self, v: u16) {
        let upper = self.state.ecx() & 0xFFFF_0000;
        self.state.set_ecx(upper | v as u32);
    }

    fn dx(&self) -> u16 {
        self.state.edx() as u16
    }

    fn set_dx(&mut self, v: u16) {
        let upper = self.state.edx() & 0xFFFF_0000;
        self.state.set_edx(upper | v as u32);
    }

    fn eax(&self) -> u32 {
        self.state.eax()
    }

    fn set_eax(&mut self, v: u32) {
        self.state.set_eax(v);
    }

    fn ebx(&self) -> u32 {
        self.state.ebx()
    }

    fn set_ebx(&mut self, v: u32) {
        self.state.set_ebx(v);
    }

    fn ecx(&self) -> u32 {
        self.state.ecx()
    }

    fn set_ecx(&mut self, v: u32) {
        self.state.set_ecx(v);
    }

    fn edx(&self) -> u32 {
        self.state.edx()
    }

    fn set_edx(&mut self, v: u32) {
        self.state.set_edx(v);
    }

    fn sp(&self) -> u16 {
        self.state.esp() as u16
    }

    fn set_sp(&mut self, v: u16) {
        let upper = self.state.esp() & 0xFFFF_0000;
        self.state.set_esp(upper | v as u32);
    }

    fn bp(&self) -> u16 {
        self.state.ebp() as u16
    }

    fn set_bp(&mut self, v: u16) {
        let upper = self.state.ebp() & 0xFFFF_0000;
        self.state.set_ebp(upper | v as u32);
    }

    fn si(&self) -> u16 {
        self.state.esi() as u16
    }

    fn set_si(&mut self, v: u16) {
        let upper = self.state.esi() & 0xFFFF_0000;
        self.state.set_esi(upper | v as u32);
    }

    fn di(&self) -> u16 {
        self.state.edi() as u16
    }

    fn set_di(&mut self, v: u16) {
        let upper = self.state.edi() & 0xFFFF_0000;
        self.state.set_edi(upper | v as u32);
    }

    fn es(&self) -> u16 {
        self.state.es()
    }

    fn set_es(&mut self, v: u16) {
        self.state.set_es(v);
    }

    fn cs(&self) -> u16 {
        self.state.cs()
    }

    fn set_cs(&mut self, v: u16) {
        self.state.set_cs(v);
    }

    fn ss(&self) -> u16 {
        self.state.ss()
    }

    fn set_ss(&mut self, v: u16) {
        self.state.set_ss(v);
    }

    fn ds(&self) -> u16 {
        self.state.ds()
    }

    fn set_ds(&mut self, v: u16) {
        self.state.set_ds(v);
    }

    fn ip(&self) -> u16 {
        self.state.ip
    }

    fn set_ip(&mut self, v: u16) {
        self.state.ip = v;
        self.state.ip_upper = 0;
    }

    fn flags(&self) -> u16 {
        self.state.flags.compress()
    }

    fn set_flags(&mut self, v: u16) {
        self.state.flags.expand(v);
    }

    fn cpu_type(&self) -> common::CpuType {
        common::CpuType::I386
    }

    fn cr0(&self) -> u32 {
        self.state.cr0
    }

    fn cr3(&self) -> u32 {
        self.state.cr3
    }

    fn load_segment_real_mode(&mut self, seg: common::SegmentRegister, selector: u16) {
        let seg32 = match seg {
            common::SegmentRegister::ES => SegReg32::ES,
            common::SegmentRegister::CS => SegReg32::CS,
            common::SegmentRegister::SS => SegReg32::SS,
            common::SegmentRegister::DS => SegReg32::DS,
        };
        self.state.sregs[seg32 as usize] = selector;
        self.set_real_segment_cache(seg32, selector);
    }

    fn segment_base(&self, seg: common::SegmentRegister) -> u32 {
        let seg32 = match seg {
            common::SegmentRegister::ES => SegReg32::ES,
            common::SegmentRegister::CS => SegReg32::CS,
            common::SegmentRegister::SS => SegReg32::SS,
            common::SegmentRegister::DS => SegReg32::DS,
        };
        self.state.seg_bases[seg32 as usize]
    }
}
