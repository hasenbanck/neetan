use common::Cpu as _;
use cpu::{I286, I286State, I386, I386State, V30, V30State};

const RAM_SIZE: usize = 1024 * 1024;
const ADDRESS_MASK: u32 = 0x000F_FFFF;
const IRQ_VECTOR: u8 = 0x20;

struct TestBus {
    ram: Vec<u8>,
    current_cycle: u64,
    irq_pending: bool,
    irq_ack_count: u64,
    irq_trigger_address: Option<u32>,
}

impl TestBus {
    fn new(irq_trigger_address: Option<u32>) -> Self {
        Self {
            ram: vec![0u8; RAM_SIZE],
            current_cycle: 0,
            irq_pending: false,
            irq_ack_count: 0,
            irq_trigger_address,
        }
    }

    fn peek(&self, address: u32) -> u8 {
        self.ram[(address & ADDRESS_MASK) as usize]
    }

    fn poke(&mut self, address: u32, value: u8) {
        self.ram[(address & ADDRESS_MASK) as usize] = value;
    }
}

impl common::Bus for TestBus {
    fn read_byte(&mut self, address: u32) -> u8 {
        self.ram[(address & ADDRESS_MASK) as usize]
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        let masked = address & ADDRESS_MASK;
        self.ram[masked as usize] = value;
        if let Some(trigger_address) = self.irq_trigger_address
            && masked == trigger_address
        {
            self.irq_pending = true;
            self.irq_trigger_address = None;
        }
    }

    fn io_read_byte(&mut self, _port: u16) -> u8 {
        0xFF
    }

    fn io_write_byte(&mut self, _port: u16, _value: u8) {}

    fn has_irq(&self) -> bool {
        self.irq_pending
    }

    fn acknowledge_irq(&mut self) -> u8 {
        self.irq_pending = false;
        self.irq_ack_count += 1;
        IRQ_VECTOR
    }

    fn has_nmi(&self) -> bool {
        false
    }

    fn acknowledge_nmi(&mut self) {}

    fn current_cycle(&self) -> u64 {
        self.current_cycle
    }

    fn set_current_cycle(&mut self, cycle: u64) {
        self.current_cycle = cycle;
    }
}

fn linear(segment: u16, offset: u16) -> u32 {
    ((u32::from(segment) << 4).wrapping_add(u32::from(offset))) & ADDRESS_MASK
}

fn place_code(bus: &mut TestBus, cs: u16, ip: u16, bytes: &[u8]) {
    let start = linear(cs, ip);
    for (index, byte) in bytes.iter().enumerate() {
        bus.poke(start.wrapping_add(index as u32), *byte);
    }
}

fn place_bytes(bus: &mut TestBus, address: u32, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        bus.poke(address.wrapping_add(index as u32), *byte);
    }
}

fn set_interrupt_vector(bus: &mut TestBus, vector: u8, target_segment: u16, target_offset: u16) {
    let base = u32::from(vector) * 4;
    bus.poke(base, (target_offset & 0x00FF) as u8);
    bus.poke(base + 1, (target_offset >> 8) as u8);
    bus.poke(base + 2, (target_segment & 0x00FF) as u8);
    bus.poke(base + 3, (target_segment >> 8) as u8);
}

trait RepCpuHarness {
    type Cpu: common::Cpu;

    fn build_cpu() -> Self::Cpu;
    fn set_state(cpu: &mut Self::Cpu, cs: u16, ip: u16, ds: u16, es: u16, ss: u16, sp: u16);
    fn set_si(cpu: &mut Self::Cpu, value: u16);
    fn set_di(cpu: &mut Self::Cpu, value: u16);
    fn set_cx(cpu: &mut Self::Cpu, value: u16);
    fn ip(cpu: &Self::Cpu) -> u16;
    fn si(cpu: &Self::Cpu) -> u16;
    fn di(cpu: &Self::Cpu) -> u16;
    fn cx(cpu: &Self::Cpu) -> u16;
}

struct V30Harness;
struct I286Harness;
struct I386Harness;

impl RepCpuHarness for V30Harness {
    type Cpu = V30;

    fn build_cpu() -> Self::Cpu {
        V30::new()
    }

    fn set_state(cpu: &mut Self::Cpu, cs: u16, ip: u16, ds: u16, es: u16, ss: u16, sp: u16) {
        let mut state = V30State::default();
        state.set_cs(cs);
        state.ip = ip;
        state.set_ds(ds);
        state.set_es(es);
        state.set_ss(ss);
        state.set_sp(sp);
        // IF=1, MF=1 (V30 native mode).
        state.set_compressed_flags(0x8202);
        cpu.load_state(&state);
    }

    fn set_si(cpu: &mut Self::Cpu, value: u16) {
        cpu.state.set_si(value);
    }

    fn set_di(cpu: &mut Self::Cpu, value: u16) {
        cpu.state.set_di(value);
    }

    fn set_cx(cpu: &mut Self::Cpu, value: u16) {
        cpu.state.set_cx(value);
    }

    fn ip(cpu: &Self::Cpu) -> u16 {
        cpu.state.ip
    }

    fn si(cpu: &Self::Cpu) -> u16 {
        cpu.state.si()
    }

    fn di(cpu: &Self::Cpu) -> u16 {
        cpu.state.di()
    }

    fn cx(cpu: &Self::Cpu) -> u16 {
        cpu.state.cx()
    }
}

impl RepCpuHarness for I286Harness {
    type Cpu = I286;

    fn build_cpu() -> Self::Cpu {
        I286::new()
    }

    fn set_state(cpu: &mut Self::Cpu, cs: u16, ip: u16, ds: u16, es: u16, ss: u16, sp: u16) {
        let mut state = I286State::default();
        state.set_cs(cs);
        state.ip = ip;
        state.set_ds(ds);
        state.set_es(es);
        state.set_ss(ss);
        state.set_sp(sp);
        state.set_compressed_flags(0x0202);
        cpu.load_state(&state);
    }

    fn set_si(cpu: &mut Self::Cpu, value: u16) {
        cpu.state.set_si(value);
    }

    fn set_di(cpu: &mut Self::Cpu, value: u16) {
        cpu.state.set_di(value);
    }

    fn set_cx(cpu: &mut Self::Cpu, value: u16) {
        cpu.state.set_cx(value);
    }

    fn ip(cpu: &Self::Cpu) -> u16 {
        cpu.state.ip
    }

    fn si(cpu: &Self::Cpu) -> u16 {
        cpu.state.si()
    }

    fn di(cpu: &Self::Cpu) -> u16 {
        cpu.state.di()
    }

    fn cx(cpu: &Self::Cpu) -> u16 {
        cpu.state.cx()
    }
}

impl RepCpuHarness for I386Harness {
    type Cpu = I386;

    fn build_cpu() -> Self::Cpu {
        I386::new()
    }

    fn set_state(cpu: &mut Self::Cpu, cs: u16, ip: u16, ds: u16, es: u16, ss: u16, sp: u16) {
        let mut state = I386State::default();
        state.set_cs(cs);
        state.set_eip(u32::from(ip));
        state.set_ds(ds);
        state.set_es(es);
        state.set_ss(ss);
        state.set_esp(u32::from(sp));
        state.set_eflags(0x0000_0202);
        cpu.load_state(&state);
    }

    fn set_si(cpu: &mut Self::Cpu, value: u16) {
        let esi = cpu.state.esi() & 0xFFFF_0000;
        cpu.state.set_esi(esi | u32::from(value));
    }

    fn set_di(cpu: &mut Self::Cpu, value: u16) {
        let edi = cpu.state.edi() & 0xFFFF_0000;
        cpu.state.set_edi(edi | u32::from(value));
    }

    fn set_cx(cpu: &mut Self::Cpu, value: u16) {
        let ecx = cpu.state.ecx() & 0xFFFF_0000;
        cpu.state.set_ecx(ecx | u32::from(value));
    }

    fn ip(cpu: &Self::Cpu) -> u16 {
        cpu.state.eip() as u16
    }

    fn si(cpu: &Self::Cpu) -> u16 {
        cpu.state.esi() as u16
    }

    fn di(cpu: &Self::Cpu) -> u16 {
        cpu.state.edi() as u16
    }

    fn cx(cpu: &Self::Cpu) -> u16 {
        cpu.state.ecx() as u16
    }
}

fn assert_rep_movsb_single_cycle_slice<H: RepCpuHarness>() {
    let mut bus = TestBus::new(None);

    // rep movsb; hlt
    place_code(&mut bus, 0x0000, 0x0100, &[0xF3, 0xA4, 0xF4]);
    place_bytes(&mut bus, 0x0300, &[0x11, 0x22, 0x33, 0x44]);

    let mut cpu = H::build_cpu();
    H::set_state(&mut cpu, 0x0000, 0x0100, 0x0000, 0x0000, 0x0000, 0xFF00);
    H::set_si(&mut cpu, 0x0300);
    H::set_di(&mut cpu, 0x0400);
    H::set_cx(&mut cpu, 0x0004);

    let _ = cpu.run_for(1, &mut bus);

    assert_eq!(H::ip(&cpu), 0x0102);
    assert_eq!(H::cx(&cpu), 0x0003);
    assert_eq!(H::si(&cpu), 0x0301);
    assert_eq!(H::di(&cpu), 0x0401);
    assert_eq!(bus.peek(0x0400), 0x11);
    assert_eq!(bus.peek(0x0401), 0x00);
}

fn assert_rep_movsb_irq_resume_after_iret<H: RepCpuHarness>() {
    let mut bus = TestBus::new(Some(0x0400));

    // mov si, 0x0300
    // mov di, 0x0400
    // mov cx, 0x0004
    // rep movsb
    // hlt
    place_code(
        &mut bus,
        0x0000,
        0x0100,
        &[
            0xBE, 0x00, 0x03, 0xBF, 0x00, 0x04, 0xB9, 0x04, 0x00, 0xF3, 0xA4, 0xF4,
        ],
    );
    place_bytes(&mut bus, 0x0300, &[0x11, 0x22, 0x33, 0x44]);

    // IRQ handler: mov byte [0x0500], 1; iret
    set_interrupt_vector(&mut bus, IRQ_VECTOR, 0x0000, 0x0200);
    place_code(
        &mut bus,
        0x0000,
        0x0200,
        &[0xC6, 0x06, 0x00, 0x05, 0x01, 0xCF],
    );

    let mut cpu = H::build_cpu();
    H::set_state(&mut cpu, 0x0000, 0x0100, 0x0000, 0x0000, 0x0000, 0xFF00);

    let _ = cpu.run_for(50_000, &mut bus);

    assert_eq!(bus.irq_ack_count, 1);
    assert_eq!(bus.peek(0x0500), 0x01);
    assert_eq!(
        [
            bus.peek(0x0400),
            bus.peek(0x0401),
            bus.peek(0x0402),
            bus.peek(0x0403)
        ],
        [0x11, 0x22, 0x33, 0x44]
    );
    assert_eq!(H::cx(&cpu), 0x0000);
}

#[test]
fn v30_rep_movsb_single_cycle_slice() {
    assert_rep_movsb_single_cycle_slice::<V30Harness>();
}

#[test]
fn i286_rep_movsb_single_cycle_slice() {
    assert_rep_movsb_single_cycle_slice::<I286Harness>();
}

#[test]
fn i386_rep_movsb_single_cycle_slice() {
    assert_rep_movsb_single_cycle_slice::<I386Harness>();
}

#[test]
fn v30_rep_movsb_with_irq_resumes_after_iret() {
    assert_rep_movsb_irq_resume_after_iret::<V30Harness>();
}

#[test]
fn i286_rep_movsb_with_irq_resumes_after_iret() {
    assert_rep_movsb_irq_resume_after_iret::<I286Harness>();
}

#[test]
fn i386_rep_movsb_with_irq_resumes_after_iret() {
    assert_rep_movsb_irq_resume_after_iret::<I386Harness>();
}
