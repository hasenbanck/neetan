use common::Cpu as _;
use cpu::{CPU_MODEL_486, I386};

const RAM_SIZE: usize = 2 * 1024 * 1024;
const ADDRESS_MASK: u32 = 0x001F_FFFF;

struct TestBus {
    ram: Vec<u8>,
    irq_pending: bool,
    irq_vector: u8,
    io_log: Vec<(u16, u8)>,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; RAM_SIZE],
            irq_pending: false,
            irq_vector: 0,
            io_log: Vec::new(),
        }
    }
}

impl common::Bus for TestBus {
    fn read_byte(&mut self, address: u32) -> u8 {
        self.ram[(address & ADDRESS_MASK) as usize]
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        self.ram[(address & ADDRESS_MASK) as usize] = value;
    }

    fn io_read_byte(&mut self, _port: u16) -> u8 {
        0xFF
    }

    fn io_write_byte(&mut self, port: u16, value: u8) {
        self.io_log.push((port, value));
    }

    fn has_irq(&self) -> bool {
        self.irq_pending
    }

    fn acknowledge_irq(&mut self) -> u8 {
        self.irq_pending = false;
        self.irq_vector
    }

    fn has_nmi(&self) -> bool {
        false
    }

    fn acknowledge_nmi(&mut self) {}

    fn current_cycle(&self) -> u64 {
        0
    }

    fn set_current_cycle(&mut self, _cycle: u64) {}
}

// --- Memory layout ---
//
// Physical memory map (2MB):
//   0x00000 - 0x00FFF : page for stack (SS base = 0)
//   0x10000 - 0x10FFF : page for data (DS base)
//   0x50000 - 0x50FFF : page for code (CS base)
//   0x60000 - 0x60FFF : page for ring-3 code
//   0x70000 - 0x70FFF : TSS
//   0x80000 - 0x80FFF : GDT
//   0x90000 - 0x97FFF : IDT (256 * 8 = 2048 bytes)
//   0xA0000 - 0xA0FFF : page directory
//   0xA1000 - 0xA1FFF : page table 0 (maps linear 0x00000000..0x003FFFFF)
//   0xB0000 - 0xB0FFF : remapped physical page (for testing remap)
//   0xC0000 - 0xC0FFF : ring-3 data page
//   0xD0000 - 0xD0FFF : read-only page (no R/W bit)
//   0x20000 - 0x20FFF : ring-3 stack

const PG_GDT_BASE: u32 = 0x80000;
const PG_IDT_BASE: u32 = 0x90000;
const PG_CODE_BASE: u32 = 0x50000;
const PG_DATA_BASE: u32 = 0x10000;
const PG_STACK_BASE: u32 = 0x00000;
const PG_RING3_CODE_BASE: u32 = 0x60000;
const PG_RING3_STACK_BASE: u32 = 0x20000;
const PG_TSS_BASE: u32 = 0x70000;
const PG_PAGE_DIR: u32 = 0xA0000;
const PG_PAGE_TABLE_0: u32 = 0xA1000;
const PG_REMAP_PAGE: u32 = 0xB0000;
const _PG_RING3_DATA_PAGE: u32 = 0xC0000;
const _PG_READONLY_PAGE: u32 = 0xD0000;

const PG_CS_SEL: u16 = 0x0008;
const PG_DS_SEL: u16 = 0x0010;
const PG_SS_SEL: u16 = 0x0018;
const PG_RING3_CS_SEL: u16 = 0x0023;
const PG_RING3_DS_SEL: u16 = 0x002B;
const PG_RING3_SS_SEL: u16 = 0x0033;
const PG_TSS_SEL: u16 = 0x0038;

const PG_GP_HANDLER_IP: u16 = 0x8000;
const PG_PF_HANDLER_IP: u16 = 0x9000;

// PTE/PDE flags
const PTE_P: u32 = 0x01;
const PTE_RW: u32 = 0x02;
const PTE_US: u32 = 0x04;
const PTE_A: u32 = 0x20;
const PTE_D: u32 = 0x40;

fn place_at(bus: &mut TestBus, addr: u32, code: &[u8]) {
    for (i, &byte) in code.iter().enumerate() {
        bus.ram[addr as usize + i] = byte;
    }
}

fn read_word_at(bus: &TestBus, addr: u32) -> u16 {
    bus.ram[addr as usize] as u16 | ((bus.ram[addr as usize + 1] as u16) << 8)
}

fn read_dword_at(bus: &TestBus, addr: u32) -> u32 {
    bus.ram[addr as usize] as u32
        | ((bus.ram[addr as usize + 1] as u32) << 8)
        | ((bus.ram[addr as usize + 2] as u32) << 16)
        | ((bus.ram[addr as usize + 3] as u32) << 24)
}

fn write_word_at(bus: &mut TestBus, addr: u32, value: u16) {
    bus.ram[addr as usize] = value as u8;
    bus.ram[addr as usize + 1] = (value >> 8) as u8;
}

fn write_dword_at(bus: &mut TestBus, addr: u32, value: u32) {
    bus.ram[addr as usize] = value as u8;
    bus.ram[addr as usize + 1] = (value >> 8) as u8;
    bus.ram[addr as usize + 2] = (value >> 16) as u8;
    bus.ram[addr as usize + 3] = (value >> 24) as u8;
}

fn write_gdt_entry16(
    bus: &mut TestBus,
    gdt_base: u32,
    entry_index: u16,
    base: u32,
    limit: u16,
    rights: u8,
) {
    let addr = (gdt_base + (entry_index as u32) * 8) as usize;
    bus.ram[addr] = limit as u8;
    bus.ram[addr + 1] = (limit >> 8) as u8;
    bus.ram[addr + 2] = base as u8;
    bus.ram[addr + 3] = (base >> 8) as u8;
    bus.ram[addr + 4] = (base >> 16) as u8;
    bus.ram[addr + 5] = rights;
    bus.ram[addr + 6] = 0;
    bus.ram[addr + 7] = (base >> 24) as u8;
}

fn write_gdt_entry32(
    bus: &mut TestBus,
    gdt_base: u32,
    entry_index: u16,
    base: u32,
    limit: u32,
    rights: u8,
    granularity: u8,
) {
    let addr = (gdt_base + (entry_index as u32) * 8) as usize;
    bus.ram[addr] = limit as u8;
    bus.ram[addr + 1] = (limit >> 8) as u8;
    bus.ram[addr + 2] = base as u8;
    bus.ram[addr + 3] = (base >> 8) as u8;
    bus.ram[addr + 4] = (base >> 16) as u8;
    bus.ram[addr + 5] = rights;
    bus.ram[addr + 6] = granularity | ((limit >> 16) as u8 & 0x0F);
    bus.ram[addr + 7] = (base >> 24) as u8;
}

fn write_idt_gate(
    bus: &mut TestBus,
    idt_base: u32,
    vector: u8,
    offset: u32,
    selector: u16,
    gate_type: u8,
    dpl: u8,
) {
    let addr = (idt_base + (vector as u32) * 8) as usize;
    bus.ram[addr] = offset as u8;
    bus.ram[addr + 1] = (offset >> 8) as u8;
    bus.ram[addr + 2] = selector as u8;
    bus.ram[addr + 3] = (selector >> 8) as u8;
    bus.ram[addr + 4] = 0;
    bus.ram[addr + 5] = 0x80 | (dpl << 5) | gate_type;
    bus.ram[addr + 6] = (offset >> 16) as u8;
    bus.ram[addr + 7] = (offset >> 24) as u8;
}

/// Sets up an identity-mapped page table covering linear 0x000000..0x1FFFFF.
/// All pages are supervisor, read/write, present.
fn setup_identity_page_tables(bus: &mut TestBus) {
    // Page directory: entry 0 points to page table 0.
    write_dword_at(bus, PG_PAGE_DIR, PG_PAGE_TABLE_0 | PTE_P | PTE_RW);

    // Page table 0: identity-map 1024 pages (4MB), covering our 2MB RAM.
    for i in 0..512u32 {
        let phys = i * 0x1000;
        write_dword_at(bus, PG_PAGE_TABLE_0 + i * 4, phys | PTE_P | PTE_RW);
    }
}

/// Sets up protected mode with paging enabled (CR0 = PE+PG).
/// Identity-maps all memory. Returns state at CPL 0.
fn setup_paged_protected_mode(bus: &mut TestBus) -> cpu::I386State {
    // GDT: null, CS (ring 0), DS (ring 0), SS (ring 0),
    //       CS (ring 3), DS (ring 3), SS (ring 3), TSS
    write_gdt_entry16(bus, PG_GDT_BASE, 0, 0, 0, 0);
    write_gdt_entry16(bus, PG_GDT_BASE, 1, PG_CODE_BASE, 0xFFFF, 0x9B);
    write_gdt_entry16(bus, PG_GDT_BASE, 2, PG_DATA_BASE, 0xFFFF, 0x93);
    write_gdt_entry16(bus, PG_GDT_BASE, 3, PG_STACK_BASE, 0xFFFF, 0x93);
    write_gdt_entry16(bus, PG_GDT_BASE, 4, PG_RING3_CODE_BASE, 0xFFFF, 0xFB);
    write_gdt_entry16(bus, PG_GDT_BASE, 5, PG_DATA_BASE, 0xFFFF, 0xF3);
    write_gdt_entry16(bus, PG_GDT_BASE, 6, PG_RING3_STACK_BASE, 0xFFFF, 0xF3);
    write_gdt_entry16(bus, PG_GDT_BASE, 7, PG_TSS_BASE, 103, 0x89);

    // IDT: #GP (13) and #PF (14) handlers.
    write_idt_gate(
        bus,
        PG_IDT_BASE,
        13,
        PG_GP_HANDLER_IP as u32,
        PG_CS_SEL,
        14,
        0,
    );
    write_idt_gate(
        bus,
        PG_IDT_BASE,
        14,
        PG_PF_HANDLER_IP as u32,
        PG_CS_SEL,
        14,
        0,
    );

    // HLT at each handler entry.
    bus.ram[(PG_CODE_BASE + PG_GP_HANDLER_IP as u32) as usize] = 0xF4;
    bus.ram[(PG_CODE_BASE + PG_PF_HANDLER_IP as u32) as usize] = 0xF4;

    // TSS: ESP0 at offset 4, SS0 at offset 8.
    write_dword_at(bus, PG_TSS_BASE + 4, 0xFFF0);
    write_word_at(bus, PG_TSS_BASE + 8, PG_SS_SEL);

    // Page tables: identity-mapped.
    setup_identity_page_tables(bus);

    let mut state = cpu::I386State {
        cr0: 0x8000_0001, // PE + PG
        cr3: PG_PAGE_DIR,
        ip: 0x0000,
        ..Default::default()
    };

    state.set_esp(0xFFF0);
    state.set_cs(PG_CS_SEL);
    state.set_ds(PG_DS_SEL);
    state.set_ss(PG_SS_SEL);
    state.set_es(PG_DS_SEL);

    state.seg_bases[cpu::SegReg32::ES as usize] = PG_DATA_BASE;
    state.seg_bases[cpu::SegReg32::CS as usize] = PG_CODE_BASE;
    state.seg_bases[cpu::SegReg32::SS as usize] = PG_STACK_BASE;
    state.seg_bases[cpu::SegReg32::DS as usize] = PG_DATA_BASE;

    state.seg_limits = [0xFFFF; 6];
    state.seg_rights[cpu::SegReg32::ES as usize] = 0x93;
    state.seg_rights[cpu::SegReg32::CS as usize] = 0x9B;
    state.seg_rights[cpu::SegReg32::SS as usize] = 0x93;
    state.seg_rights[cpu::SegReg32::DS as usize] = 0x93;
    state.seg_valid = [true, true, true, true, false, false];

    state.gdt_base = PG_GDT_BASE;
    state.gdt_limit = 8 * 8 - 1;
    state.idt_base = PG_IDT_BASE;
    state.idt_limit = 256 * 8 - 1;

    state.tr = PG_TSS_SEL;
    state.tr_base = PG_TSS_BASE;
    state.tr_limit = 103;
    state.tr_rights = 0x8B;

    state
}

fn make_ring3(state: &mut cpu::I386State) {
    state.set_cs(PG_RING3_CS_SEL);
    state.seg_bases[cpu::SegReg32::CS as usize] = PG_RING3_CODE_BASE;
    state.seg_rights[cpu::SegReg32::CS as usize] = 0xFB;

    state.set_ss(PG_RING3_SS_SEL);
    state.seg_bases[cpu::SegReg32::SS as usize] = PG_RING3_STACK_BASE;
    state.seg_rights[cpu::SegReg32::SS as usize] = 0xF3;

    state.set_ds(PG_RING3_DS_SEL);
    state.seg_bases[cpu::SegReg32::DS as usize] = PG_DATA_BASE;
    state.seg_rights[cpu::SegReg32::DS as usize] = 0xF3;

    state.set_es(PG_RING3_DS_SEL);
    state.seg_bases[cpu::SegReg32::ES as usize] = PG_DATA_BASE;
    state.seg_rights[cpu::SegReg32::ES as usize] = 0xF3;
}

/// Identity-mapped paging: MOV AL,[DS:offset] reads through page tables correctly.
#[test]
fn paging_identity_map_read_byte() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Write test value at DS:0x0042 = physical PG_DATA_BASE + 0x42.
    bus.ram[(PG_DATA_BASE + 0x42) as usize] = 0xBE;

    // MOV AL, [0x0042] ; A0 42 00
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x42, 0x00]);
    cpu.step(&mut bus);

    assert_eq!(
        cpu.al(),
        0xBE,
        "identity-mapped read should return correct value"
    );
}

/// Identity-mapped paging: MOV [DS:offset], AL writes through page tables correctly.
#[test]
fn paging_identity_map_write_byte() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Set AL = 0x5A, then MOV [0x0080], AL ; A2 80 00
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xB0, 0x5A, // MOV AL, 0x5A
            0xA2, 0x80, 0x00, // MOV [0x0080], AL
        ],
    );
    cpu.step(&mut bus); // MOV AL, imm8
    cpu.step(&mut bus); // MOV [moffs], AL

    assert_eq!(
        bus.ram[(PG_DATA_BASE + 0x80) as usize],
        0x5A,
        "identity-mapped write should store correct value"
    );
}

/// Identity-mapped paging: word read across page-aligned addresses.
#[test]
fn paging_identity_map_read_word() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Write 0xCAFE at DS:0x0100.
    write_word_at(&mut bus, PG_DATA_BASE + 0x100, 0xCAFE);

    // MOV AX, [0x0100] ; A1 00 01
    place_at(&mut bus, PG_CODE_BASE, &[0xA1, 0x00, 0x01]);
    cpu.step(&mut bus);

    assert_eq!(cpu.ax(), 0xCAFE);
}

/// Identity-mapped paging: dword read.
#[test]
fn paging_identity_map_read_dword() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    write_dword_at(&mut bus, PG_DATA_BASE + 0x200, 0xDEAD_BEEF);

    // MOV EAX, [0x0200] ; 66 A1 00 02
    place_at(&mut bus, PG_CODE_BASE, &[0x66, 0xA1, 0x00, 0x02]);
    cpu.step(&mut bus);

    assert_eq!(cpu.eax(), 0xDEAD_BEEF);
}

/// Remapped page: linear address maps to a different physical page.
#[test]
fn paging_remap_read() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Remap linear page at DS:0x0000 (linear = PG_DATA_BASE = 0x10000,
    // page index = 0x10) to physical PG_REMAP_PAGE instead of identity.
    let pte_index = PG_DATA_BASE >> 12; // = 0x10
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );

    // Write test value at the REMAPPED physical page.
    bus.ram[(PG_REMAP_PAGE + 0x0010) as usize] = 0x77;

    // MOV AL, [0x0010] ; A0 10 00
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x10, 0x00]);
    cpu.step(&mut bus);

    assert_eq!(
        cpu.al(),
        0x77,
        "read through remapped PTE should hit the remapped physical page"
    );
}

/// Remapped page: write goes to remapped physical page.
#[test]
fn paging_remap_write() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Remap data page to PG_REMAP_PAGE.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );

    // MOV AL, 0xAA ; MOV [0x0020], AL
    place_at(&mut bus, PG_CODE_BASE, &[0xB0, 0xAA, 0xA2, 0x20, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert_eq!(
        bus.ram[(PG_REMAP_PAGE + 0x20) as usize],
        0xAA,
        "write through remapped PTE should store in the remapped physical page"
    );
    assert_eq!(
        bus.ram[(PG_DATA_BASE + 0x20) as usize],
        0x00,
        "original identity-mapped physical page should be untouched"
    );
}

/// Reading a page sets Accessed bit on PDE and PTE.
#[test]
fn paging_accessed_bit_on_read() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Clear A bits on the PDE and the PTE for the data page.
    let pte_index = PG_DATA_BASE >> 12;
    let pde_before = read_dword_at(&bus, PG_PAGE_DIR);
    let pte_before = read_dword_at(&bus, PG_PAGE_TABLE_0 + pte_index * 4);
    write_dword_at(&mut bus, PG_PAGE_DIR, pde_before & !PTE_A);
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        pte_before & !PTE_A,
    );

    // MOV AL, [0x0000] ; A0 00 00
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x00, 0x00]);
    cpu.step(&mut bus);

    let pde_after = read_dword_at(&bus, PG_PAGE_DIR);
    let pte_after = read_dword_at(&bus, PG_PAGE_TABLE_0 + pte_index * 4);

    assert_ne!(
        pde_after & PTE_A,
        0,
        "PDE Accessed bit must be set after read"
    );
    assert_ne!(
        pte_after & PTE_A,
        0,
        "PTE Accessed bit must be set after read"
    );
    assert_eq!(
        pte_after & PTE_D,
        0,
        "PTE Dirty bit must NOT be set after read"
    );
}

/// Writing a page sets both Accessed and Dirty bits on PTE.
#[test]
fn paging_dirty_bit_on_write() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Clear A+D bits.
    let pte_index = PG_DATA_BASE >> 12;
    let pde_before = read_dword_at(&bus, PG_PAGE_DIR);
    let pte_before = read_dword_at(&bus, PG_PAGE_TABLE_0 + pte_index * 4);
    write_dword_at(&mut bus, PG_PAGE_DIR, pde_before & !(PTE_A | PTE_D));
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        pte_before & !(PTE_A | PTE_D),
    );

    // MOV BYTE [0x0000], 0x55 ; C6 06 00 00 55
    place_at(&mut bus, PG_CODE_BASE, &[0xC6, 0x06, 0x00, 0x00, 0x55]);
    cpu.step(&mut bus);

    let pde_after = read_dword_at(&bus, PG_PAGE_DIR);
    let pte_after = read_dword_at(&bus, PG_PAGE_TABLE_0 + pte_index * 4);

    assert_ne!(
        pde_after & PTE_A,
        0,
        "PDE Accessed bit must be set after write"
    );
    assert_ne!(
        pte_after & PTE_A,
        0,
        "PTE Accessed bit must be set after write"
    );
    assert_ne!(
        pte_after & PTE_D,
        0,
        "PTE Dirty bit must be set after write"
    );
}

/// #PF on read from a not-present PDE. Error code = 0 (not present, read, supervisor).
/// Uses PDE 1 (linear 0x400000+) for data so clearing it doesn't affect code/stack.
#[test]
fn paging_fault_pde_not_present_read() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    // Move DS/ES to linear 0x400000 (covered by PDE 1, not PDE 0).
    state.seg_bases[cpu::SegReg32::DS as usize] = 0x00400000;
    state.seg_bases[cpu::SegReg32::ES as usize] = 0x00400000;
    cpu.load_state(&state);

    // Set up PDE 1 with a page table, then clear it.
    let pt1_base = 0xA2000u32;
    write_dword_at(&mut bus, PG_PAGE_DIR + 4, pt1_base | PTE_P | PTE_RW);
    write_dword_at(&mut bus, pt1_base, PG_DATA_BASE | PTE_P | PTE_RW);
    // Now clear PDE 1 - code/stack remain under PDE 0.
    write_dword_at(&mut bus, PG_PAGE_DIR + 4, 0);

    // MOV AL, [0x0000] - DS:0 = linear 0x400000, PDE 1 not present -> #PF.
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x00, 0x00]);
    cpu.step(&mut bus); // faults
    cpu.step(&mut bus); // executes HLT in #PF handler

    assert!(cpu.halted(), "CPU should halt in #PF handler");
    assert_eq!(
        cpu.ip(),
        PG_PF_HANDLER_IP as u32 + 1,
        "should be in #PF handler"
    );

    // Error code pushed on stack: 0 (not present, read, supervisor).
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(
        error_code, 0x0000,
        "error code: not present, read, supervisor"
    );

    // CR2 = faulting linear address = DS base + 0 = 0x400000.
    assert_eq!(cpu.cr2, 0x00400000, "CR2 must hold faulting linear address");
}

/// #PF on read from a not-present PTE.
#[test]
fn paging_fault_pte_not_present_read() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Clear Present bit on the PTE for the data page.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(&mut bus, PG_PAGE_TABLE_0 + pte_index * 4, 0);

    // MOV AL, [0x0042]
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x42, 0x00]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);

    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(
        error_code, 0x0000,
        "error code: not present, read, supervisor"
    );
    assert_eq!(cpu.cr2, PG_DATA_BASE + 0x42);
}

/// #PF on write to a not-present PTE. Error code bit 1 (W/R) set.
#[test]
fn paging_fault_pte_not_present_write() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(&mut bus, PG_PAGE_TABLE_0 + pte_index * 4, 0);

    // MOV BYTE [0x0010], 0xFF ; C6 06 10 00 FF
    place_at(&mut bus, PG_CODE_BASE, &[0xC6, 0x06, 0x10, 0x00, 0xFF]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(
        error_code, 0x0002,
        "error code: not present, write, supervisor"
    );
    assert_eq!(cpu.cr2, PG_DATA_BASE + 0x10);
}

/// Ring-3 read from a supervisor-only page (#PF with P=1, U/S=1).
#[test]
fn paging_fault_user_reads_supervisor_page() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    // PDE: present + R/W + U/S (user can traverse directory).
    let pde = read_dword_at(&bus, PG_PAGE_DIR);
    write_dword_at(&mut bus, PG_PAGE_DIR, pde | PTE_US);

    // Make ring-3 code page user-accessible so code fetch succeeds.
    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Make ring-3 stack page user-accessible.
    let stack_pte_index = PG_RING3_STACK_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + stack_pte_index * 4,
        PG_RING3_STACK_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Make the data page supervisor-only: present + R/W but NO U/S.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_RW,
    );

    // MOV AL, [0x0000] from ring 3 code.
    place_at(&mut bus, PG_RING3_CODE_BASE, &[0xA0, 0x00, 0x00]);
    cpu.step(&mut bus); // faults on data read (data PTE lacks U/S)
    cpu.step(&mut bus); // HLT in handler

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);

    // Error code: present=1, read=0, user=1 -> 0x05.
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(error_code, 0x0005, "error code: present, read, user");
}

/// Ring-3 write to a read-only user page (#PF with P=1, W/R=1, U/S=1).
#[test]
fn paging_fault_user_writes_readonly_page() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    // PDE: present + R/W + U/S (user can traverse directory).
    let pde = read_dword_at(&bus, PG_PAGE_DIR);
    write_dword_at(&mut bus, PG_PAGE_DIR, pde | PTE_US);

    // Make ring-3 code page user-accessible.
    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Make ring-3 stack page user-accessible.
    let stack_pte_index = PG_RING3_STACK_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + stack_pte_index * 4,
        PG_RING3_STACK_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Make the data page user-accessible but read-only: present + U/S, no R/W.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_US,
    );

    // MOV BYTE [0x0000], 0x11 from ring 3.
    place_at(
        &mut bus,
        PG_RING3_CODE_BASE,
        &[0xC6, 0x06, 0x00, 0x00, 0x11],
    );
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(cpu.halted());
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(error_code, 0x0007, "error code: present, write, user");
}

/// On a 386, supervisor (CPL 0) can write to a page with R/W=0.
#[test]
fn paging_supervisor_writes_readonly_page() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Make the data page read-only (present, no R/W, no U/S).
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P,
    );

    // MOV BYTE [0x0050], 0xCC ; C6 06 50 00 CC
    place_at(&mut bus, PG_CODE_BASE, &[0xC6, 0x06, 0x50, 0x00, 0xCC]);
    cpu.step(&mut bus);

    assert_eq!(
        bus.ram[(PG_DATA_BASE + 0x50) as usize],
        0xCC,
        "386 supervisor must be able to write read-only pages (no WP bit)"
    );
    assert!(!cpu.halted(), "should NOT fault");
}

/// Writing CR3 flushes TLB: remapping a page after a CR3 write takes effect.
#[test]
fn paging_cr3_write_flushes_tlb() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Place ALL code upfront to avoid prefetch cache issues.
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xA0, 0x10, 0x00, // MOV AL, [0x10]
            0x0F, 0x20, 0xD8, // MOV EAX, CR3
            0x0F, 0x22, 0xD8, // MOV CR3, EAX
            0xA0, 0x10, 0x00, // MOV AL, [0x10]
        ],
    );

    // First read to prime the TLB.
    bus.ram[(PG_DATA_BASE + 0x10) as usize] = 0x11;
    cpu.step(&mut bus); // MOV AL, [0x10]
    assert_eq!(cpu.al(), 0x11);

    // Now remap the data page to PG_REMAP_PAGE.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );
    bus.ram[(PG_REMAP_PAGE + 0x10) as usize] = 0x22;

    cpu.step(&mut bus); // MOV EAX, CR3
    cpu.step(&mut bus); // MOV CR3, EAX (flushes TLB)
    cpu.step(&mut bus); // MOV AL, [0x10] - should use new mapping

    assert_eq!(
        cpu.al(),
        0x22,
        "after CR3 write (TLB flush), read should use remapped page"
    );
}

/// Code fetch goes through paging: remap the code page and verify execution.
#[test]
fn paging_code_fetch_through_page_tables() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Remap the code page to PG_REMAP_PAGE.
    let pte_index = PG_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );

    // Place code in the REMAPPED physical page: MOV AL, 0xEE ; HLT
    bus.ram[PG_REMAP_PAGE as usize] = 0xB0; // MOV AL, imm8
    bus.ram[(PG_REMAP_PAGE + 1) as usize] = 0xEE;
    bus.ram[(PG_REMAP_PAGE + 2) as usize] = 0xF4; // HLT

    cpu.step(&mut bus); // MOV AL, 0xEE - fetched from remapped page
    assert_eq!(cpu.al(), 0xEE, "code fetch must go through paging");

    cpu.step(&mut bus); // HLT
    assert!(cpu.halted());
}

/// PUSH/POP go through paging with a remapped stack page.
#[test]
fn paging_push_pop_through_page_tables() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    state.set_esp(0x0100); // Put ESP in page 0 so remapping page 0 affects it.
    cpu.load_state(&state);

    // Remap page 0 (linear 0x0000-0x0FFF) to PG_REMAP_PAGE.
    write_dword_at(&mut bus, PG_PAGE_TABLE_0, PG_REMAP_PAGE | PTE_P | PTE_RW);

    // PUSH 0x1234 ; POP BX
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0x68, 0x34, 0x12, // PUSH 0x1234
            0x5B, // POP BX
        ],
    );

    let sp_before = cpu.esp();
    cpu.step(&mut bus); // PUSH
    cpu.step(&mut bus); // POP

    assert_eq!(
        cpu.bx(),
        0x1234,
        "POP should read value pushed through paged stack"
    );
    assert_eq!(cpu.esp(), sp_before, "SP should be restored after PUSH+POP");

    // Verify the push went to the remapped physical page.
    let push_addr = (sp_before - 2) as usize;
    assert_eq!(
        bus.ram[PG_REMAP_PAGE as usize + push_addr],
        0x34,
        "PUSH should write to remapped stack page (low byte)"
    );
    assert_eq!(
        bus.ram[PG_REMAP_PAGE as usize + push_addr + 1],
        0x12,
        "PUSH should write to remapped stack page (high byte)"
    );
}

/// MOVSB copies data between two differently-mapped pages.
#[test]
fn paging_movsb_between_remapped_pages() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Source: DS:SI -> data page (identity-mapped at PG_DATA_BASE).
    // Dest: ES:DI -> we'll remap ES's target page.
    //
    // DS base = PG_DATA_BASE = 0x10000
    // ES base = PG_DATA_BASE = 0x10000
    // We'll use SI=0x0000 (source) and DI=0x0100 (dest).
    // Remap the dest area: PTE for linear (PG_DATA_BASE + 0x0100) is the
    // same page (PG_DATA_BASE >> 12 = 0x10), so we need a different approach.
    //
    // Use a different segment setup: put source data at DS:0, dest at ES:0
    // with ES pointing to a remapped page.

    // Write source data.
    bus.ram[PG_DATA_BASE as usize] = 0xAA;
    bus.ram[(PG_DATA_BASE + 1) as usize] = 0xBB;
    bus.ram[(PG_DATA_BASE + 2) as usize] = 0xCC;

    // Set up: CLD; MOV CX, 3; REP MOVSB
    // SI=0x0000, DI=0x0100 (both within same page, identity-mapped)
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xFC, // CLD
            0xB9, 0x03, 0x00, // MOV CX, 3
            0xF3, 0xA4, // REP MOVSB
        ],
    );

    // Set SI=0, DI=0x100 via state reload.
    let mut s = state;
    s.set_esi(0x0000);
    s.set_edi(0x0100);
    cpu.load_state(&s);

    cpu.step(&mut bus); // CLD
    cpu.step(&mut bus); // MOV CX, 3
    cpu.step(&mut bus); // REP MOVSB (3 iterations)

    assert_eq!(bus.ram[(PG_DATA_BASE + 0x100) as usize], 0xAA);
    assert_eq!(bus.ram[(PG_DATA_BASE + 0x101) as usize], 0xBB);
    assert_eq!(bus.ram[(PG_DATA_BASE + 0x102) as usize], 0xCC);
    assert_eq!(cpu.cx(), 0, "CX should be 0 after REP MOVSB");
}

/// #PF on instruction fetch from a not-present code page.
#[test]
fn paging_fault_on_code_fetch() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Unmap the code page.
    let pte_index = PG_CODE_BASE >> 12;
    write_dword_at(&mut bus, PG_PAGE_TABLE_0 + pte_index * 4, 0);

    cpu.step(&mut bus); // fetch faults
    cpu.step(&mut bus); // HLT in #PF handler

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);
    assert_eq!(
        cpu.cr2, PG_CODE_BASE,
        "CR2 should hold the faulting code fetch address"
    );
}

/// #PF when PUSH writes to a not-present stack page.
/// Uses ring 3 so the #PF handler gets a TSS stack switch to a valid stack.
#[test]
fn paging_fault_on_stack_push() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    state.set_esp(0x0100); // Put ring-3 ESP in page 0 of ring-3 stack.
    cpu.load_state(&state);

    // PDE: present + R/W + U/S (user can traverse directory).
    let pde = read_dword_at(&bus, PG_PAGE_DIR);
    write_dword_at(&mut bus, PG_PAGE_DIR, pde | PTE_US);

    // Make ring-3 code page user-accessible.
    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Unmap ring-3 stack page (PG_RING3_STACK_BASE >> 12 = page 0x20).
    // The #PF handler switches to TSS ESP0=0xFFF0 / SS0 (page 0xF), still mapped.
    let stack_pte_index = PG_RING3_STACK_BASE >> 12;
    write_dword_at(&mut bus, PG_PAGE_TABLE_0 + stack_pte_index * 4, 0);

    // PUSH AX (0x50)
    place_at(&mut bus, PG_RING3_CODE_BASE, &[0x50]);
    cpu.step(&mut bus); // faults on stack write
    cpu.step(&mut bus); // HLT in #PF handler

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);

    // Error code should have W/R=1 (write) and U/S=1 (user).
    // Handler stack is at SS0:ESP0 from TSS (ESP0=0xFFF0, page 0xF, SS base=0).
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(
        error_code & 0x06,
        0x06,
        "stack push #PF error code should have W/R=1 and U/S=1"
    );
}

/// User-mode access denied when PDE lacks U/S, even if PTE has U/S.
/// Since all pages are under PDE 0, removing U/S from PDE blocks ALL user
/// access including code fetch. This test verifies the code fetch fault.
#[test]
fn paging_fault_user_denied_by_pde() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    // PDE: present + R/W but NO U/S. This blocks all user-mode access.
    write_dword_at(&mut bus, PG_PAGE_DIR, PG_PAGE_TABLE_0 | PTE_P | PTE_RW);

    // PTE for ring-3 code page: present + R/W + U/S. The PDE still blocks it.
    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );

    place_at(&mut bus, PG_RING3_CODE_BASE, &[0xA0, 0x00, 0x00]);
    cpu.step(&mut bus); // code fetch faults (PDE lacks U/S)
    cpu.step(&mut bus); // HLT in #PF handler (runs at ring 0, supervisor access OK)

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(
        error_code, 0x0005,
        "user denied by PDE: present + read + user"
    );
    assert_eq!(
        cpu.cr2, PG_RING3_CODE_BASE,
        "CR2 = faulting code fetch address"
    );
}

/// Ring-3 read from a user-accessible page succeeds.
#[test]
fn paging_user_read_succeeds() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    // PDE: P + RW + US.
    write_dword_at(
        &mut bus,
        PG_PAGE_DIR,
        PG_PAGE_TABLE_0 | PTE_P | PTE_RW | PTE_US,
    );

    // PTE for data page: P + RW + US.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Also make ring-3 code page user-accessible.
    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Also make ring-3 stack page user-accessible.
    let stack_pte_index = PG_RING3_STACK_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + stack_pte_index * 4,
        PG_RING3_STACK_BASE | PTE_P | PTE_RW | PTE_US,
    );

    bus.ram[(PG_DATA_BASE + 0x30) as usize] = 0x99;

    // MOV AL, [0x0030]
    place_at(&mut bus, PG_RING3_CODE_BASE, &[0xA0, 0x30, 0x00]);
    cpu.step(&mut bus);

    assert_eq!(cpu.al(), 0x99, "ring-3 read from user page should succeed");
    assert!(!cpu.halted(), "should not fault");
}

/// Ring-3 write to a user R/W page succeeds.
#[test]
fn paging_user_write_succeeds() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    // PDE: P + RW + US.
    write_dword_at(
        &mut bus,
        PG_PAGE_DIR,
        PG_PAGE_TABLE_0 | PTE_P | PTE_RW | PTE_US,
    );

    // All user pages: P + RW + US.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_RW | PTE_US,
    );
    let code_pte = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );
    let stack_pte = PG_RING3_STACK_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + stack_pte * 4,
        PG_RING3_STACK_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // MOV BYTE [0x0060], 0x77
    place_at(
        &mut bus,
        PG_RING3_CODE_BASE,
        &[0xC6, 0x06, 0x60, 0x00, 0x77],
    );
    cpu.step(&mut bus);

    assert_eq!(
        bus.ram[(PG_DATA_BASE + 0x60) as usize],
        0x77,
        "ring-3 write to user R/W page should succeed"
    );
    assert!(!cpu.halted());
}

/// With paging disabled (CR0.PG=0), addresses pass through with 24-bit mask.
#[test]
fn paging_disabled_identity_passthrough() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    // Set up protected mode WITHOUT paging (CR0 = PE only).
    let mut state = setup_paged_protected_mode(&mut bus);
    state.cr0 = 0x0000_0001; // PE only, PG=0
    cpu.load_state(&state);

    bus.ram[(PG_DATA_BASE + 0x10) as usize] = 0x42;

    // MOV AL, [0x0010]
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x10, 0x00]);
    cpu.step(&mut bus);

    assert_eq!(
        cpu.al(),
        0x42,
        "with PG=0, memory access should use linear address directly"
    );
}

/// MOV CR3, EAX stores the page directory base and flushes TLB.
#[test]
fn paging_mov_cr3_changes_page_directory() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Set up a second page directory at 0x100000.
    let pd2 = 0x100000u32;
    let pt2 = 0x101000u32;
    // PDE 0 -> pt2.
    write_dword_at(&mut bus, pd2, pt2 | PTE_P | PTE_RW);
    // Map the code page identity.
    let code_pte = PG_CODE_BASE >> 12;
    write_dword_at(&mut bus, pt2 + code_pte * 4, PG_CODE_BASE | PTE_P | PTE_RW);
    // Map the data page to PG_REMAP_PAGE.
    let data_pte = PG_DATA_BASE >> 12;
    write_dword_at(&mut bus, pt2 + data_pte * 4, PG_REMAP_PAGE | PTE_P | PTE_RW);
    // Map the stack page identity.
    write_dword_at(&mut bus, pt2, PG_STACK_BASE | PTE_P | PTE_RW);

    bus.ram[(PG_REMAP_PAGE + 0x20) as usize] = 0xDD;

    // MOV EAX, pd2 ; MOV CR3, EAX ; MOV AL, [0x20]
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0x66,
            0xB8,
            pd2 as u8,
            (pd2 >> 8) as u8,
            (pd2 >> 16) as u8,
            (pd2 >> 24) as u8, // MOV EAX, pd2
            0x0F,
            0x22,
            0xD8, // MOV CR3, EAX
            0xA0,
            0x20,
            0x00, // MOV AL, [0x20]
        ],
    );

    cpu.step(&mut bus); // MOV EAX, pd2
    cpu.step(&mut bus); // MOV CR3, EAX
    assert_eq!(cpu.state.cr3, pd2);

    cpu.step(&mut bus); // MOV AL, [0x20]
    assert_eq!(
        cpu.al(),
        0xDD,
        "after MOV CR3, data read should use the new page directory"
    );
}

/// Two distinct linear pages (in the same page table) map to different physical pages.
#[test]
fn paging_two_pages_different_physical() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // DS base is PG_DATA_BASE = 0x10000.
    // DS:0x0000 -> page 0x10 (identity -> phys 0x10000).
    // DS:0x1000 -> page 0x11 (we'll remap to PG_REMAP_PAGE).
    let pte_11 = (PG_DATA_BASE >> 12) + 1; // page index 0x11
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_11 * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );

    bus.ram[PG_DATA_BASE as usize] = 0x11;
    bus.ram[PG_REMAP_PAGE as usize] = 0x22;

    // MOV AL, [0x0000] ; MOV AH, [0x1000]
    // Note: [0x1000] needs 67h prefix for 32-bit addressing in 16-bit mode,
    // or we can use a register-indirect approach.
    // Simpler: MOV AL, [0x0000] then change SI and do MOV AH, [SI].
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xA0, 0x00, 0x00, // MOV AL, [0x0000]
            0xBE, 0x00, 0x10, // MOV SI, 0x1000
            0x8A, 0x24, // MOV AH, [SI]
        ],
    );

    cpu.step(&mut bus); // MOV AL, [0x0000]
    assert_eq!(cpu.al(), 0x11);

    cpu.step(&mut bus); // MOV SI, 0x1000
    cpu.step(&mut bus); // MOV AH, [SI]
    assert_eq!(
        cpu.ah(),
        0x22,
        "second page should read from remapped physical page"
    );
}

/// Paging requires both CR0.PG=1 AND CR0.PE=1.
#[test]
fn paging_requires_pe_and_pg() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    // Set up protected mode with paging.
    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Verify paging is active.
    assert_eq!(cpu.state.cr0 & 0x8000_0001, 0x8000_0001);

    // Remap data page so we can distinguish paged vs unpaged.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );
    bus.ram[PG_REMAP_PAGE as usize] = 0xAA;
    bus.ram[PG_DATA_BASE as usize] = 0xBB;

    // Read should go through page tables -> get 0xAA from remapped page.
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x00, 0x00]);
    cpu.step(&mut bus);
    assert_eq!(
        cpu.al(),
        0xAA,
        "with paging enabled, should read remapped page"
    );
}

const PG_CS32_SEL: u16 = 0x0040;

fn setup_paged_protected_mode_32bit(bus: &mut TestBus) -> cpu::I386State {
    let mut state = setup_paged_protected_mode(bus);

    // Add a 32-bit code segment at GDT index 8 (selector 0x40).
    // D-bit (0x40 in granularity byte) = 1 means 32-bit default operand/address size.
    write_gdt_entry32(bus, PG_GDT_BASE, 8, PG_CODE_BASE, 0xFFFF, 0x9B, 0x40);

    state.set_cs(PG_CS32_SEL);
    state.seg_bases[cpu::SegReg32::CS as usize] = PG_CODE_BASE;
    state.seg_rights[cpu::SegReg32::CS as usize] = 0x9B;
    state.seg_granularity[cpu::SegReg32::CS as usize] = 0x40;

    state
}

#[test]
fn paging_tlb_hit_read_write() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    bus.ram[(PG_DATA_BASE + 0x20) as usize] = 0xAA;

    // Clear A/D bits on PDE and PTE for data page.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(&mut bus, PG_PAGE_DIR, PG_PAGE_TABLE_0 | PTE_P | PTE_RW);
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_RW,
    );

    // MOV AL, [0x0020] (TLB miss); MOV AL, [0x0020] (TLB hit);
    // MOV [0x0021], AL (write, TLB hit); HLT
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xA0, 0x20, 0x00, // MOV AL, [0x0020]
            0xA0, 0x20, 0x00, // MOV AL, [0x0020]
            0xA2, 0x21, 0x00, // MOV [0x0021], AL
            0xF4, // HLT
        ],
    );
    cpu.step(&mut bus); // first read (TLB miss)
    cpu.step(&mut bus); // second read (TLB hit)
    cpu.step(&mut bus); // write (TLB hit)
    cpu.step(&mut bus); // HLT

    assert_eq!(cpu.al(), 0xAA);
    assert_eq!(bus.ram[(PG_DATA_BASE + 0x21) as usize], 0xAA);
    assert!(cpu.halted());

    let pte = read_dword_at(&bus, PG_PAGE_TABLE_0 + pte_index * 4);
    assert_ne!(pte & PTE_A, 0, "PTE Accessed bit must be set");
    assert_ne!(pte & PTE_D, 0, "PTE Dirty bit must be set after write");
}

#[test]
fn paging_accessed_dirty_bits_already_set() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    let pte_index = PG_DATA_BASE >> 12;
    // Pre-set A+D on PDE and PTE.
    write_dword_at(
        &mut bus,
        PG_PAGE_DIR,
        PG_PAGE_TABLE_0 | PTE_P | PTE_RW | PTE_A | PTE_D,
    );
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_RW | PTE_A | PTE_D,
    );

    bus.ram[(PG_DATA_BASE + 0x30) as usize] = 0xBB;

    // MOV BYTE [0x0030], 0xCC ; HLT
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[0xC6, 0x06, 0x30, 0x00, 0xCC, 0xF4],
    );
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert_eq!(bus.ram[(PG_DATA_BASE + 0x30) as usize], 0xCC);
    assert!(cpu.halted());

    let pde = read_dword_at(&bus, PG_PAGE_DIR);
    let pte = read_dword_at(&bus, PG_PAGE_TABLE_0 + pte_index * 4);
    assert_eq!(
        pde,
        PG_PAGE_TABLE_0 | PTE_P | PTE_RW | PTE_A | PTE_D,
        "PDE bits should be unchanged (skip-write optimization)"
    );
    assert_eq!(
        pte,
        PG_DATA_BASE | PTE_P | PTE_RW | PTE_A | PTE_D,
        "PTE bits should be unchanged (skip-write optimization)"
    );
}

#[test]
fn paging_pde_pte_permission_conflict() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    // Scenario (a): PDE has U/S+R/W, PTE has U/S but NO R/W. User write -> error 0x0007.
    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    write_dword_at(
        &mut bus,
        PG_PAGE_DIR,
        PG_PAGE_TABLE_0 | PTE_P | PTE_RW | PTE_US,
    );

    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );
    let stack_pte_index = PG_RING3_STACK_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + stack_pte_index * 4,
        PG_RING3_STACK_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Data PTE: present + U/S but NO R/W.
    let data_pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + data_pte_index * 4,
        PG_DATA_BASE | PTE_P | PTE_US,
    );

    // MOV BYTE [0x0000], 0x11 from ring 3.
    place_at(
        &mut bus,
        PG_RING3_CODE_BASE,
        &[0xC6, 0x06, 0x00, 0x00, 0x11],
    );
    cpu.step(&mut bus); // fault
    cpu.step(&mut bus); // HLT in handler

    assert!(cpu.halted());
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(error_code, 0x0007, "present + write + user");

    // Scenario (b): PDE has NO U/S, PTE has U/S+R/W. User read -> error 0x0005.
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    cpu.load_state(&state);

    // PDE: present + R/W but NO U/S.
    write_dword_at(&mut bus, PG_PAGE_DIR, PG_PAGE_TABLE_0 | PTE_P | PTE_RW);

    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );

    place_at(&mut bus, PG_RING3_CODE_BASE, &[0xA0, 0x00, 0x00]);
    cpu.step(&mut bus); // code fetch faults (PDE lacks U/S)
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    let sp = cpu.esp();
    let error_code = read_word_at(&bus, PG_STACK_BASE + sp);
    assert_eq!(error_code, 0x0005, "present + read + user");
}

#[test]
fn paging_stosb_cross_page_boundary() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    state.seg_bases[cpu::SegReg32::ES as usize] = 0;
    state.set_edi(0x0FFC);
    state.set_ecx(8);
    state.flags.df = false;
    cpu.load_state(&state);

    // Set AL=0xDD, then CLD; REP STOSB; HLT
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xB0, 0xDD, // MOV AL, 0xDD
            0xFC, // CLD
            0xF3, 0xAA, // REP STOSB
            0xF4, // HLT
        ],
    );
    cpu.step(&mut bus); // MOV AL, 0xDD
    cpu.step(&mut bus); // CLD
    cpu.step(&mut bus); // REP STOSB
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    for i in 0..8u32 {
        assert_eq!(
            bus.ram[(0x0FFC + i) as usize],
            0xDD,
            "byte at 0x{:04X} should be 0xDD",
            0x0FFC + i
        );
    }

    // Check A+D bits on both pages.
    let pte_page0 = read_dword_at(&bus, PG_PAGE_TABLE_0);
    let pte_page1 = read_dword_at(&bus, PG_PAGE_TABLE_0 + 4);
    assert_ne!(pte_page0 & (PTE_A | PTE_D), 0, "page 0 should have A+D");
    assert_ne!(pte_page1 & (PTE_A | PTE_D), 0, "page 1 should have A+D");
}

#[test]
fn paging_cmpsb_cross_page_boundary() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    // DS base=0, ES base=0 for direct linear addressing.
    state.seg_bases[cpu::SegReg32::DS as usize] = 0;
    state.seg_bases[cpu::SegReg32::ES as usize] = 0;
    state.set_esi(0x0FFE);
    state.set_edi(0x10FFE);
    state.set_ecx(4);
    state.flags.df = false;
    cpu.load_state(&state);

    // Identity-map page 0x10 and 0x11 (for ES:EDI region at 0x10000-0x11FFF).
    // These should already be identity-mapped by setup_identity_page_tables.

    // Write identical patterns at the page boundaries.
    let pattern = [0xAA, 0xBB, 0xCC, 0xDD];
    for (i, &byte) in pattern.iter().enumerate() {
        bus.ram[0x0FFE + i] = byte;
        bus.ram[0x10FFE + i] = byte;
    }

    // CLD; REPE CMPSB; HLT
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xFC, // CLD
            0xF3, 0xA6, // REPE CMPSB
            0xF4, // HLT
        ],
    );
    cpu.step(&mut bus); // CLD
    cpu.step(&mut bus); // REPE CMPSB
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    assert!(cpu.state.flags.zf(), "ZF should be set (strings equal)");
    assert_eq!(cpu.cx(), 0, "CX should be 0 (all compared)");
}

#[test]
fn paging_push_dword_cross_page() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    // SS base=0, ESP=0x1002 so PUSH dword writes to 0x0FFE-0x1001 (crosses 0x1000).
    state.seg_bases[cpu::SegReg32::SS as usize] = 0;
    state.set_esp(0x1002);
    state.set_eax(0xDEADBEEF);
    cpu.load_state(&state);

    // 66 50 = PUSH EAX (in 16-bit code segment, 0x66 prefix promotes to dword)
    // F4 = HLT
    place_at(&mut bus, PG_CODE_BASE, &[0x66, 0x50, 0xF4]);
    cpu.step(&mut bus); // PUSH EAX
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    assert_eq!(cpu.esp(), 0x0FFE);
    assert_eq!(bus.ram[0x0FFE], 0xEF);
    assert_eq!(bus.ram[0x0FFF], 0xBE);
    assert_eq!(bus.ram[0x1000], 0xAD);
    assert_eq!(bus.ram[0x1001], 0xDE);

    let pte_page0 = read_dword_at(&bus, PG_PAGE_TABLE_0);
    let pte_page1 = read_dword_at(&bus, PG_PAGE_TABLE_0 + 4);
    assert_ne!(pte_page0 & (PTE_A | PTE_D), 0, "page 0 should have A+D");
    assert_ne!(pte_page1 & (PTE_A | PTE_D), 0, "page 1 should have A+D");
}

#[test]
fn paging_outsb_translates_source() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);

    // Remap DS data page (linear 0x10000) -> physical PG_REMAP_PAGE (0xB0000).
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );

    // Write 0x42 to the remapped physical location.
    bus.ram[(PG_REMAP_PAGE + 0x20) as usize] = 0x42;

    // Set SI=0x0020, DX=0x1234.
    state.set_esi(0x0020);
    state.set_edx(0x1234);
    state.flags.df = false;
    cpu.load_state(&state);

    // OUTSB; HLT (opcode 0x6E, F4)
    place_at(&mut bus, PG_CODE_BASE, &[0x6E, 0xF4]);
    cpu.step(&mut bus); // OUTSB
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    assert!(
        bus.io_log.contains(&(0x1234, 0x42)),
        "OUTSB should write 0x42 to port 0x1234, got {:?}",
        bus.io_log
    );
    assert_eq!(cpu.state.esi() & 0xFFFF, 0x0021, "ESI should advance by 1");
}

#[test]
fn paging_lgdt_from_paged_memory() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Remap DS data page (linear 0x10000) -> physical PG_REMAP_PAGE (0xB0000).
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_REMAP_PAGE | PTE_P | PTE_RW,
    );

    // Write a 6-byte LGDT operand at physical 0xB0050: limit=0x00FF, base=0x00012345.
    bus.ram[(PG_REMAP_PAGE + 0x50) as usize] = 0xFF; // limit low
    bus.ram[(PG_REMAP_PAGE + 0x51) as usize] = 0x00; // limit high
    bus.ram[(PG_REMAP_PAGE + 0x52) as usize] = 0x45; // base byte 0
    bus.ram[(PG_REMAP_PAGE + 0x53) as usize] = 0x23; // base byte 1
    bus.ram[(PG_REMAP_PAGE + 0x54) as usize] = 0x01; // base byte 2
    bus.ram[(PG_REMAP_PAGE + 0x55) as usize] = 0x00; // base byte 3 (16-bit LGDT uses 3 bytes)

    // LGDT [0x0050] ; HLT
    // 16-bit mode: 0F 01 16 50 00 = LGDT [0x0050]
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[0x0F, 0x01, 0x16, 0x50, 0x00, 0xF4],
    );
    cpu.step(&mut bus); // LGDT
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    assert_eq!(cpu.state.gdt_limit, 0x00FF, "GDT limit should be 0x00FF");
    assert_eq!(
        cpu.state.gdt_base & 0x00FFFFFF,
        0x00012345,
        "GDT base should be 0x00012345 (24-bit in 16-bit mode)"
    );
}

#[test]
fn paging_prefix_66_idempotent() {
    // 16-bit code segment: double 0x66 should still result in 32-bit push (idempotent).
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    state.set_esp(0xFFF0);
    state.set_eax(0x12345678);
    cpu.load_state(&state);

    // 66 66 50 F4 = double prefix + PUSH EAX + HLT
    place_at(&mut bus, PG_CODE_BASE, &[0x66, 0x66, 0x50, 0xF4]);
    cpu.step(&mut bus); // PUSH EAX (with double 0x66 prefix)
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    let esp = cpu.esp();
    assert_eq!(
        0xFFF0 - esp,
        4,
        "double 0x66 in 16-bit segment should be idempotent (32-bit push)"
    );
    let pushed = read_dword_at(&bus, PG_STACK_BASE + esp);
    assert_eq!(pushed, 0x12345678);

    // 32-bit code segment: double 0x66 should result in 16-bit push (idempotent override).
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode_32bit(&mut bus);
    state.set_esp(0xFFF0);
    state.set_eax(0xAABBCCDD);
    cpu.load_state(&state);

    // 66 66 50 F4 = double prefix + PUSH AX + HLT (in 32-bit segment, 0x66 = 16-bit)
    place_at(&mut bus, PG_CODE_BASE, &[0x66, 0x66, 0x50, 0xF4]);
    cpu.step(&mut bus); // PUSH AX (16-bit due to idempotent override)
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    let esp = cpu.esp();
    assert_eq!(
        0xFFF0 - esp,
        2,
        "double 0x66 in 32-bit segment should be idempotent (16-bit push)"
    );
    let pushed = read_word_at(&bus, PG_STACK_BASE + esp);
    assert_eq!(pushed, 0xCCDD);
}

#[test]
fn paging_prefix_67_idempotent() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    bus.ram[(PG_DATA_BASE + 0x100) as usize] = 0x99;

    // 67 67 8A 05 00 01 00 00 F4
    // Double 0x67 prefix + MOV AL, [disp32] with disp32=0x00000100 + HLT
    // In 16-bit segment, 0x67 switches to 32-bit addressing.
    // Idempotent: still 32-bit addressing. Toggle: back to 16-bit (wrong decode).
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[0x67, 0x67, 0x8A, 0x05, 0x00, 0x01, 0x00, 0x00, 0xF4],
    );
    cpu.step(&mut bus); // MOV AL, [0x00000100]
    cpu.step(&mut bus); // HLT

    assert!(cpu.halted());
    assert_eq!(
        cpu.al(),
        0x99,
        "double 0x67 in 16-bit segment should be idempotent (32-bit address)"
    );
}

#[test]
fn paging_supervisor_override_ring3_interrupt() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    make_ring3(&mut state);
    state.flags.if_flag = false;
    state.flags.iopl = 3;
    cpu.load_state(&state);

    // PDE: P + RW + US.
    write_dword_at(
        &mut bus,
        PG_PAGE_DIR,
        PG_PAGE_TABLE_0 | PTE_P | PTE_RW | PTE_US,
    );

    // Ring-3 code page: user-accessible.
    let code_pte_index = PG_RING3_CODE_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + code_pte_index * 4,
        PG_RING3_CODE_BASE | PTE_P | PTE_RW | PTE_US,
    );
    // Ring-3 stack page: user-accessible.
    let stack_pte_index = PG_RING3_STACK_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + stack_pte_index * 4,
        PG_RING3_STACK_BASE | PTE_P | PTE_RW | PTE_US,
    );

    // Mark IDT pages as supervisor-only (clear U/S).
    let idt_pte_start = PG_IDT_BASE >> 12;
    for i in 0..2u32 {
        write_dword_at(
            &mut bus,
            PG_PAGE_TABLE_0 + (idt_pte_start + i) * 4,
            (PG_IDT_BASE + i * 0x1000) | PTE_P | PTE_RW,
        );
    }

    // Set up IRQ vector 0x40 -> ring-0 handler at a known offset in code segment.
    let irq_handler_offset: u32 = 0xA000;
    write_idt_gate(
        &mut bus,
        PG_IDT_BASE,
        0x40,
        irq_handler_offset,
        PG_CS_SEL,
        14,
        0,
    );
    // Place HLT at the handler.
    bus.ram[(PG_CODE_BASE + irq_handler_offset) as usize] = 0xF4;

    // Ensure the handler's page is mapped (supervisor-only is fine for ring-0 handler).
    let handler_page = (PG_CODE_BASE + irq_handler_offset) >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + handler_page * 4,
        (PG_CODE_BASE + irq_handler_offset) & 0xFFFFF000 | PTE_P | PTE_RW,
    );

    // Ring-3 code: STI; JMP $-2 (infinite loop, waiting for IRQ).
    // STI = 0xFB, JMP short -2 = EB FE
    place_at(&mut bus, PG_RING3_CODE_BASE, &[0xFB, 0xEB, 0xFE]);

    // Set up IRQ.
    bus.irq_pending = true;
    bus.irq_vector = 0x40;
    cpu.signal_irq();

    cpu.step(&mut bus); // STI (inhibits IRQ for one instruction)
    cpu.step(&mut bus); // JMP $-2 - IRQ fires after this instruction
    cpu.step(&mut bus); // HLT in IRQ handler

    assert!(
        cpu.halted(),
        "CPU should halt in IRQ handler (IDT on supervisor page should be accessible)"
    );
    assert_eq!(
        cpu.ip(),
        irq_handler_offset + 1,
        "should be in the IRQ handler"
    );
}

#[test]
fn paging_fault_pending_stops_dispatch() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // Unmap the data page.
    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(&mut bus, PG_PAGE_TABLE_0 + pte_index * 4, 0);

    // Sentinel: ensure physical 0x10010 is 0x00.
    bus.ram[(PG_DATA_BASE + 0x10) as usize] = 0x00;

    // MOV [0x0000], AL (fault); MOV BYTE [0x0010], 0xFF (should NOT execute); HLT
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xA2, 0x00, 0x00, // MOV [0x0000], AL
            0xC6, 0x06, 0x10, 0x00, 0xFF, // MOV BYTE [0x0010], 0xFF
            0xF4, // HLT
        ],
    );
    cpu.step(&mut bus); // faults on first MOV
    cpu.step(&mut bus); // HLT in #PF handler

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);
    assert_eq!(cpu.cr2, PG_DATA_BASE, "CR2 should be faulting address");
    assert_eq!(
        bus.ram[(PG_DATA_BASE + 0x10) as usize],
        0x00,
        "second MOV should not have executed"
    );
}

#[test]
fn paging_rep_movsb_fault_mid_operation() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    // Source: DS base=0x10000, SI=0. Fill source with 0x11-0x18.
    for i in 0..8u32 {
        bus.ram[(PG_DATA_BASE + i) as usize] = (0x11 + i) as u8;
    }

    // Destination: ES base=0x30000, DI=0x0FFC. Linear addresses 0x30FFC-0x31003.
    // Page 0x30 is identity-mapped, page 0x31 is unmapped -> fault at 0x31000.
    state.seg_bases[cpu::SegReg32::ES as usize] = 0x30000;
    state.set_esi(0x0000);
    state.set_edi(0x0FFC);
    state.set_ecx(8);
    state.flags.df = false;

    // Ensure page 0x30 is mapped.
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + 0x30 * 4,
        0x30000 | PTE_P | PTE_RW,
    );
    // Unmap page 0x31.
    write_dword_at(&mut bus, PG_PAGE_TABLE_0 + 0x31 * 4, 0);

    cpu.load_state(&state);

    // CLD; REP MOVSB; HLT
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[
            0xFC, // CLD
            0xF3, 0xA4, // REP MOVSB
            0xF4, // HLT
        ],
    );
    cpu.step(&mut bus); // CLD
    cpu.step(&mut bus); // REP MOVSB - faults mid-operation at destination 0x31000
    cpu.step(&mut bus); // HLT in #PF handler

    assert!(cpu.halted());
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);
    assert_eq!(
        cpu.cr2, 0x31000,
        "CR2 should be 0x31000 (unmapped dest page)"
    );

    // First 4 bytes should have been written (0x30FFC-0x30FFF).
    assert_eq!(bus.ram[0x30FFC], 0x11);
    assert_eq!(bus.ram[0x30FFD], 0x12);
    assert_eq!(bus.ram[0x30FFE], 0x13);
    assert_eq!(bus.ram[0x30FFF], 0x14);

    // ECX should reflect remaining count (4 remaining).
    assert_eq!(cpu.cx(), 4, "CX should be 4 (4 iterations remaining)");
}

#[test]
fn cr0_wp_bit_masked_on_386() {
    let mut cpu: I386 = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // MOV EAX, 0xFFFFFFFF ; 66 B8 FF FF FF FF
    // MOV CR0, EAX        ; 0F 22 C0
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[0x66, 0xB8, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x22, 0xC0],
    );
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert_eq!(
        cpu.cr0 & 0x0001_0000,
        0,
        "WP bit (16) must be masked off on 386"
    );
}

#[test]
fn cr0_wp_bit_accepted_on_486() {
    let mut cpu: I386<{ CPU_MODEL_486 }> = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);

    // MOV EAX, 0xFFFFFFFF ; 66 B8 FF FF FF FF
    // MOV CR0, EAX        ; 0F 22 C0
    place_at(
        &mut bus,
        PG_CODE_BASE,
        &[0x66, 0xB8, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x22, 0xC0],
    );
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert_ne!(
        cpu.cr0 & 0x0001_0000,
        0,
        "WP bit (16) must be accepted on 486"
    );
}

#[test]
fn supervisor_write_to_readonly_page_without_wp() {
    let mut cpu: I386<{ CPU_MODEL_486 }> = I386::new();
    let mut bus = TestBus::new();

    let state = setup_paged_protected_mode(&mut bus);
    cpu.load_state(&state);
    // WP=0 (default): supervisor writes to read-only pages should succeed.

    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P,
    );

    // MOV BYTE [0x0050], 0xCC ; C6 06 50 00 CC
    place_at(&mut bus, PG_CODE_BASE, &[0xC6, 0x06, 0x50, 0x00, 0xCC]);
    cpu.step(&mut bus);

    assert_eq!(bus.ram[(PG_DATA_BASE + 0x50) as usize], 0xCC);
    assert!(
        !cpu.halted(),
        "486 supervisor with WP=0 must write read-only pages"
    );
}

#[test]
fn supervisor_write_to_readonly_page_with_wp() {
    let mut cpu: I386<{ CPU_MODEL_486 }> = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    // Set WP bit (16) in CR0.
    state.cr0 |= 0x0001_0000;
    cpu.load_state(&state);

    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P,
    );

    // MOV BYTE [0x0050], 0xCC ; C6 06 50 00 CC
    place_at(&mut bus, PG_CODE_BASE, &[0xC6, 0x06, 0x50, 0x00, 0xCC]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(
        cpu.halted(),
        "486 supervisor with WP=1 must fault on write to read-only page"
    );
    assert_eq!(cpu.ip(), PG_PF_HANDLER_IP as u32 + 1);

    // Error code: P=1 (protection violation), W/R=1 (write), U/S=0 (supervisor).
    // That's bits 0 and 1 = 0x03.
    let handler_esp = cpu.esp();
    let error_code = read_dword_at(&bus, PG_STACK_BASE + handler_esp);
    assert_eq!(error_code, 0x03, "error code should be P=1, W/R=1, U/S=0");
}

#[test]
fn supervisor_read_from_readonly_page_with_wp() {
    let mut cpu: I386<{ CPU_MODEL_486 }> = I386::new();
    let mut bus = TestBus::new();

    let mut state = setup_paged_protected_mode(&mut bus);
    state.cr0 |= 0x0001_0000;
    cpu.load_state(&state);

    let pte_index = PG_DATA_BASE >> 12;
    write_dword_at(
        &mut bus,
        PG_PAGE_TABLE_0 + pte_index * 4,
        PG_DATA_BASE | PTE_P,
    );

    bus.ram[(PG_DATA_BASE + 0x50) as usize] = 0xAB;

    // MOV AL, [0x0050] ; A0 50 00
    place_at(&mut bus, PG_CODE_BASE, &[0xA0, 0x50, 0x00]);
    cpu.step(&mut bus);

    assert!(
        !cpu.halted(),
        "WP only affects writes, reads should succeed"
    );
    assert_eq!(cpu.al(), 0xAB);
}
