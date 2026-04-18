//! End-to-end JIT verification using the test386.asm self-test ROM.
//!
//! This mirrors `crates/cpu/tests/test386.rs` but runs the same ROM
//! through [`dynarec::I386Jit`] instead of the [`cpu::I386`]
//! interpreter. The ROM is vendored under `crates/cpu/tests/test386/`
//! and boots a real/protected/paging/VM86 self-test that HLTs with
//! POST code 0xFF on success. See that directory's `README.md` for the
//! full POST table.
//!
//! The dynarec gets far more interesting coverage from this ROM than
//! from SingleStepTests: it decodes a real boot sequence that mixes
//! segment loads, paging setup, far jumps, protected-mode transitions,
//! and software interrupts, so any gap in decoder coverage or
//! BlockExit ordering surfaces quickly.

use common::{Bus, Cpu};
use cpu::CPU_MODEL_386;
use dynarec::{I386Jit, JitBackend};

const ROM_BYTES: &[u8] = include_bytes!("../../cpu/tests/test386/test386.bin");
const EE_REFERENCE: &str =
    include_str!("../../cpu/tests/test386/upstream/test386-EE-reference.txt");

const RAM_SIZE: usize = 16 * 1024 * 1024;
const ADDRESS_MASK: u32 = 0x00FF_FFFF;
const ROM_BASE: usize = 0x000F_0000;

// Must match the EQUs in crates/cpu/tests/test386/configuration.asm.
const POST_PORT: u16 = 0x0190;
const OUT_PORT: u16 = 0x00E9;

/// Upper bound on JIT cycle budget. The ROM finishes in well under a
/// billion cycles on either backend; this cap only fires when a bug
/// causes the ROM to loop indefinitely.
const MAX_CYCLES: u64 = 2_000_000_000;

/// Cycle slice per `run_for` call. Small enough that we notice `halted`
/// promptly, large enough that dispatcher overhead stays amortized.
const SLICE_CYCLES: u64 = 100_000;

struct TestBus {
    ram: Vec<u8>,
    post_history: Vec<u8>,
    last_post: Option<u8>,
    ascii_output: Vec<u8>,
    cycle: u64,
}

impl TestBus {
    fn new() -> Self {
        let mut ram = vec![0u8; RAM_SIZE];
        ram[ROM_BASE..ROM_BASE + ROM_BYTES.len()].copy_from_slice(ROM_BYTES);
        Self {
            ram,
            post_history: Vec::new(),
            last_post: None,
            ascii_output: Vec::new(),
            cycle: 0,
        }
    }
}

impl Bus for TestBus {
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
        match port {
            POST_PORT => {
                self.post_history.push(value);
                self.last_post = Some(value);
            }
            OUT_PORT => {
                self.ascii_output.push(value);
            }
            _ => {}
        }
    }

    fn has_irq(&self) -> bool {
        false
    }

    fn acknowledge_irq(&mut self) -> u8 {
        0
    }

    fn has_nmi(&self) -> bool {
        false
    }

    fn acknowledge_nmi(&mut self) {}

    fn current_cycle(&self) -> u64 {
        self.cycle
    }

    fn set_current_cycle(&mut self, cycle: u64) {
        self.cycle = cycle;
    }
}

fn run_until_halt(cpu: &mut I386Jit<CPU_MODEL_386>, bus: &mut TestBus) -> u64 {
    let mut total = 0u64;
    while !cpu.halted() && total < MAX_CYCLES {
        let ran = cpu.run_for(SLICE_CYCLES, bus);
        if ran == 0 && !cpu.halted() {
            // Dispatcher made no forward progress and the CPU is not
            // halted: bail out so the test reports a meaningful error
            // instead of spinning until the outer timeout.
            break;
        }
        total += ran;
    }
    total
}

fn format_post_history(history: &[u8]) -> String {
    history
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_reaches_post_ff(backend: JitBackend) {
    let mut cpu: I386Jit<CPU_MODEL_386> = I386Jit::new_with_backend(backend);
    let mut bus = TestBus::new();

    let cycles = run_until_halt(&mut cpu, &mut bus);

    assert!(
        cpu.halted(),
        "JIT ({:?}) did not HLT within {MAX_CYCLES} cycles (ran {cycles}); \
         last POST: {:?}, history: [{}]",
        cpu.backend(),
        bus.last_post,
        format_post_history(&bus.post_history),
    );

    let last = bus.last_post.unwrap_or(0);
    assert_eq!(
        last,
        0xFF,
        "test386.asm failed on {:?} at POST 0x{last:02X} after {cycles} cycles; \
         history: [{}]; consult crates/cpu/tests/test386/upstream/README.md \
         POST table and crates/cpu/tests/test386/test386.lst",
        cpu.backend(),
        format_post_history(&bus.post_history),
    );
}

fn run_ee_matches_reference(backend: JitBackend) {
    let mut cpu: I386Jit<CPU_MODEL_386> = I386Jit::new_with_backend(backend);
    let mut bus = TestBus::new();
    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(
        bus.last_post,
        Some(0xFF),
        "precondition ({:?}): ROM must reach POST 0xFF; last POST: {:?}",
        cpu.backend(),
        bus.last_post,
    );

    let produced = std::str::from_utf8(&bus.ascii_output)
        .expect("EE output must be valid UTF-8 (ROM emits ASCII only)");

    if produced.lines().eq(EE_REFERENCE.lines()) {
        return;
    }

    let mismatch = produced
        .lines()
        .zip(EE_REFERENCE.lines())
        .enumerate()
        .find(|(_, (got, want))| got != want);

    match mismatch {
        Some((lineno, (got, want))) => panic!(
            "EE output diverges at line {} ({:?}):\n  got:  {got}\n  want: {want}",
            lineno + 1,
            cpu.backend(),
        ),
        None => {
            let produced_lines = produced.lines().count();
            let expected_lines = EE_REFERENCE.lines().count();
            panic!(
                "EE output length differs ({:?}): produced {produced_lines} lines, \
                 expected {expected_lines} lines",
                cpu.backend(),
            );
        }
    }
}

#[test]
fn test386_reaches_post_ff_bytecode() {
    run_reaches_post_ff(JitBackend::Bytecode);
}

#[test]
fn test386_ee_output_matches_reference_bytecode() {
    run_ee_matches_reference(JitBackend::Bytecode);
}

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn test386_reaches_post_ff_x64() {
    run_reaches_post_ff(JitBackend::X64);
}

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn test386_ee_output_matches_reference_x64() {
    run_ee_matches_reference(JitBackend::X64);
}
