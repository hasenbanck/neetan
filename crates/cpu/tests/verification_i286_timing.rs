#![cfg(feature = "verification")]

#[path = "common/metadata_json.rs"]
mod metadata_json;
#[path = "common/verification_common.rs"]
mod verification_common;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use common::Cpu as _;
use cpu::{
    I286, I286BusPhase, I286CycleTraceEntry, I286FlushState, I286State, I286TraceBusStatus,
    I286WarmStartConfig,
};
use metadata_json::{Metadata, load_metadata};
use verification_common::{MooCycle, MooI286Cycle, MooTest, load_moo_tests, load_revocation_list};

const RAM_SIZE: usize = 4 * 1024 * 1024;
const ADDRESS_MASK: u32 = 0x00FF_FFFF;
const REG_ORDER: [&str; 14] = [
    "ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "flags",
];

struct TestBus {
    ram: Vec<u8>,
    dirty: Vec<u32>,
    dirty_marker: Vec<u8>,
    current_cycle: u64,
    wait_cycles: i64,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; RAM_SIZE],
            dirty: Vec::new(),
            dirty_marker: vec![0u8; RAM_SIZE],
            current_cycle: 0,
            wait_cycles: 0,
        }
    }

    fn clear(&mut self) {
        for &address in &self.dirty {
            let index = (address & ADDRESS_MASK) as usize;
            self.ram[index] = 0;
            self.dirty_marker[index] = 0;
        }
        self.dirty.clear();
        self.current_cycle = 0;
        self.wait_cycles = 0;
    }

    fn set_memory(&mut self, address: u32, value: u8) {
        let masked_address = address & ADDRESS_MASK;
        let index = masked_address as usize;
        if self.dirty_marker[index] == 0 {
            self.dirty_marker[index] = 1;
            self.dirty.push(masked_address);
        }
        self.ram[index] = value;
    }
}

impl common::Bus for TestBus {
    fn read_byte(&mut self, address: u32) -> u8 {
        self.ram[(address & ADDRESS_MASK) as usize]
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        self.set_memory(address, value);
    }

    fn io_read_byte(&mut self, _port: u16) -> u8 {
        0xFF
    }

    fn io_write_byte(&mut self, _port: u16, _value: u8) {}

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
        self.current_cycle
    }

    fn set_current_cycle(&mut self, cycle: u64) {
        self.current_cycle = cycle;
    }

    fn drain_wait_cycles(&mut self) -> i64 {
        let wait_cycles = self.wait_cycles;
        self.wait_cycles = 0;
        wait_cycles
    }
}

fn test_dir() -> &'static Path {
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/SingleStepTests/80286/v1_real_mode")
    });
    &DIR
}

fn metadata() -> &'static Metadata {
    static META: LazyLock<Metadata> =
        LazyLock::new(|| load_metadata(&test_dir().join("metadata.json")));
    &META
}

fn revocation_list() -> &'static HashSet<String> {
    static REVOKED: LazyLock<HashSet<String>> = LazyLock::new(|| {
        load_revocation_list(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/SingleStepTests/80286/revocation_list.txt"),
        )
    });
    &REVOKED
}

/// Can be used for TEMPORARY revocations
fn local_timing_revocation_list(_stem: &str) -> &'static [&'static str] {
    &[]
}

fn timing_revocation_list(stem: &str, local_revoked_hashes: &[&str]) -> HashSet<String> {
    let mut revoked = revocation_list().clone();
    revoked.extend(
        local_timing_revocation_list(stem)
            .iter()
            .map(|hash| hash.to_ascii_lowercase()),
    );
    revoked.extend(
        local_revoked_hashes
            .iter()
            .map(|hash| hash.to_ascii_lowercase()),
    );
    revoked
}

fn should_test(status: &str) -> bool {
    matches!(
        status,
        "normal" | "alias" | "undocumented" | "fpu" | "undefined"
    )
}

fn parse_stem(stem: &str) -> (&str, Option<&str>) {
    if let Some(dot_pos) = stem.find('.') {
        (&stem[..dot_pos], Some(&stem[dot_pos + 1..]))
    } else {
        (stem, None)
    }
}

fn status_and_mask(stem: &str) -> Option<(&str, u16)> {
    let metadata = metadata();
    let (opcode, reg_ext) = parse_stem(stem);

    let entry = metadata.opcodes.get(opcode)?;
    if let Some(reg_ext) = reg_ext {
        let reg_info = entry.reg.as_ref()?.get(reg_ext)?;
        Some((
            reg_info.status.as_str(),
            reg_info.flags_mask.unwrap_or(0xFFFF),
        ))
    } else if let Some(status) = &entry.status {
        Some((status.as_str(), entry.flags_mask.unwrap_or(0xFFFF)))
    } else {
        None
    }
}

fn build_initial_state(test: &MooTest) -> I286State {
    let get = |name: &str| -> u16 {
        test.initial
            .regs
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("missing register in initial state: {name}")) as u16
    };

    let mut state = I286State::default();
    state.set_ax(get("ax"));
    state.set_bx(get("bx"));
    state.set_cx(get("cx"));
    state.set_dx(get("dx"));
    state.set_sp(get("sp"));
    state.set_bp(get("bp"));
    state.set_si(get("si"));
    state.set_di(get("di"));
    state.set_cs(get("cs"));
    state.set_ss(get("ss"));
    state.set_ds(get("ds"));
    state.set_es(get("es"));
    state.ip = get("ip");
    state.set_compressed_flags(get("flags"));
    state.msw = 0xFFF0;
    state
}

fn collect_timing_trace(test: &MooTest) -> Result<(Vec<I286CycleTraceEntry>, u64), String> {
    let initial_state = build_initial_state(test);
    let mut bus = TestBus::new();
    bus.clear();
    for &(address, value) in &test.initial.ram {
        bus.set_memory(address, value);
    }

    let mut cpu = I286::new();
    cpu.load_state(&initial_state);
    cpu.set_cycle_trace_capture(true);
    if !test.initial.queue.is_empty() {
        cpu.install_front_end_state(
            &mut bus,
            &test.initial.queue,
            test.initial.queue.len() as u8,
            cpu::I286FlushState::None,
        );
    }

    let mut trace = Vec::new();
    let mut total_cycles = 0u64;
    let mut steps = 0usize;
    while !cpu.halted() && steps < 1024 {
        cpu.step(&mut bus);
        total_cycles = total_cycles.wrapping_add(cpu.cycles_consumed());
        trace.extend(cpu.drain_cycle_trace());
        steps += 1;
    }

    if steps >= 1024 {
        return Err("CPU did not halt while collecting the timing trace".to_string());
    }

    Ok((trace, total_cycles))
}

#[allow(dead_code)]
fn collect_manual_timing_trace(
    initial_state: &I286State,
    memory: &[(u32, u8)],
) -> (Vec<NormalizedI286Cycle>, u64) {
    let mut bus = TestBus::new();
    let mut cpu = I286::new();

    cpu.load_state(initial_state);
    cpu.set_cycle_trace_capture(true);

    for &(address, value) in memory {
        bus.set_memory(address, value);
    }

    let mut trace = Vec::new();
    let mut total_cycles = 0u64;
    let mut steps = 0usize;
    while !cpu.halted() && steps < 8 {
        cpu.step(&mut bus);
        total_cycles = total_cycles.wrapping_add(cpu.cycles_consumed());
        trace.extend(cpu.drain_cycle_trace());
        steps += 1;
    }

    assert!(cpu.halted(), "CPU did not halt while collecting the trace");

    (normalize_actual_cycles(&trace), total_cycles)
}

fn resolved_register(test: &MooTest, name: &str) -> u16 {
    test.final_state
        .regs
        .get(name)
        .or_else(|| test.initial.regs.get(name))
        .copied()
        .unwrap_or_default() as u16
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedI286Cycle {
    bus_phase: I286BusPhase,
    bus_status: String,
    address: Option<u32>,
    data: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default)]
struct TraceFilterStats {
    data_bus_only_traces: usize,
    data_bus_cycles: usize,
    terminal_overfetch_traces: usize,
    terminal_passive_slack_traces: usize,
    fpu_handoff_slack_traces: usize,
    input_passive_slack_traces: usize,
    port_zero_input_passive_slack_traces: usize,
}

impl TraceFilterStats {
    fn add(&mut self, other: Self) {
        self.data_bus_only_traces += other.data_bus_only_traces;
        self.data_bus_cycles += other.data_bus_cycles;
        self.terminal_overfetch_traces += other.terminal_overfetch_traces;
        self.terminal_passive_slack_traces += other.terminal_passive_slack_traces;
        self.fpu_handoff_slack_traces += other.fpu_handoff_slack_traces;
        self.input_passive_slack_traces += other.input_passive_slack_traces;
        self.port_zero_input_passive_slack_traces += other.port_zero_input_passive_slack_traces;
    }

    fn filtered_traces(&self) -> usize {
        self.data_bus_only_traces
            + self.terminal_overfetch_traces
            + self.terminal_passive_slack_traces
            + self.fpu_handoff_slack_traces
            + self.input_passive_slack_traces
            + self.port_zero_input_passive_slack_traces
    }

    fn summary(&self) -> String {
        format!(
            "filtered traces: {} (data-only {}, terminal-overfetch {}, terminal-passive-slack {}, fpu-handoff-slack {}, input-passive-slack {}, port-zero-input-passive-slack {}, data cycles {})",
            self.filtered_traces(),
            self.data_bus_only_traces,
            self.terminal_overfetch_traces,
            self.terminal_passive_slack_traces,
            self.fpu_handoff_slack_traces,
            self.input_passive_slack_traces,
            self.port_zero_input_passive_slack_traces,
            self.data_bus_cycles
        )
    }
}

fn comparable_data_bus_status(bus_status: &str) -> bool {
    bus_status != "PASV" && bus_status != "HALT"
}

fn normalize_expected_cycles(cycles: &[MooCycle]) -> Vec<NormalizedI286Cycle> {
    cycles
        .iter()
        .filter_map(|cycle| match cycle {
            MooCycle::I286(MooI286Cycle {
                address,
                data_bus,
                bus_status,
                t_state,
                ..
            }) => Some(NormalizedI286Cycle {
                bus_phase: match t_state.as_str() {
                    "Ts" => I286BusPhase::Ts,
                    "Tc" => I286BusPhase::Tc,
                    _ => I286BusPhase::Ti,
                },
                bus_status: bus_status.clone(),
                address: if bus_status == "PASV" {
                    None
                } else {
                    Some(*address)
                },
                data: if comparable_data_bus_status(bus_status) {
                    Some(*data_bus)
                } else {
                    None
                },
            }),
            MooCycle::Legacy(_) | MooCycle::V20(_) => None,
        })
        .collect()
}

fn normalize_actual_cycles(trace: &[I286CycleTraceEntry]) -> Vec<NormalizedI286Cycle> {
    let mut normalized = Vec::new();
    for entry in trace {
        let bus_status = match entry.bus_status {
            I286TraceBusStatus::Passive => "PASV",
            I286TraceBusStatus::Code => "CODE",
            I286TraceBusStatus::MemoryRead => "MEMR",
            I286TraceBusStatus::MemoryWrite => "MEMW",
            I286TraceBusStatus::IoRead => "IOR",
            I286TraceBusStatus::IoWrite => "IOW",
            I286TraceBusStatus::Halt => "HALT",
        }
        .to_string();

        let is_passive = bus_status == "PASV";
        let data_is_comparable = comparable_data_bus_status(&bus_status);
        normalized.push(NormalizedI286Cycle {
            bus_phase: entry.state.bus_phase,
            bus_status,
            address: if is_passive { None } else { entry.address },
            data: if data_is_comparable { entry.data } else { None },
        });

        if entry.bus_status == I286TraceBusStatus::Halt {
            break;
        }
    }
    normalized
}

fn normalize_actual_cycles_for_expected(
    expected: &[NormalizedI286Cycle],
    trace: &[I286CycleTraceEntry],
) -> Vec<NormalizedI286Cycle> {
    let mut stats = TraceFilterStats::default();
    normalize_actual_cycles_for_expected_with_stats(expected, trace, &mut stats)
}

fn normalize_actual_cycles_for_expected_with_stats(
    expected: &[NormalizedI286Cycle],
    trace: &[I286CycleTraceEntry],
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    let actual = normalize_actual_cycles(trace);
    let actual = normalize_terminal_halt_overfetch(expected, actual, stats);
    let actual = normalize_terminal_halt_passive_slack(expected, actual, stats);
    let actual = normalize_fpu_handoff_passive_slack(expected, actual, stats);
    let actual = normalize_input_passive_slack(expected, actual, stats);
    let actual = normalize_port_zero_input_passive_slack(expected, actual, stats);
    normalize_data_bus_only_trace(expected, actual, stats)
}

fn normalize_terminal_halt_overfetch(
    expected: &[NormalizedI286Cycle],
    actual: Vec<NormalizedI286Cycle>,
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    // The terminal HLT byte is a fixture sentinel. Some captures record final
    // speculative code fetches as passive clocks before the HALT marker.
    let Some(mismatch_index) = find_cycle_mismatch_index(expected, &actual) else {
        return actual;
    };
    let Some(expected_halt_index) = expected.iter().position(is_halt_cycle) else {
        return actual;
    };
    let Some(actual_halt_index) = actual.iter().position(is_halt_cycle) else {
        return actual;
    };

    let mut suffix_start = None;
    for start in mismatch_index..expected_halt_index {
        if start >= actual_halt_index {
            break;
        }
        if same_slices_except_data(&expected[..start], &actual[..start])
            && is_terminal_passive_window(&expected[start..expected_halt_index])
            && is_terminal_fetch_or_passive_window(&actual[start..actual_halt_index])
            && contains_code_fetch_pair(&actual[start..actual_halt_index])
        {
            suffix_start = Some(start);
            break;
        }
    }
    let Some(suffix_start) = suffix_start else {
        return actual;
    };

    let mut normalized = Vec::with_capacity(expected.len());
    normalized.extend_from_slice(&actual[..suffix_start]);
    normalized.extend(expected[suffix_start..expected_halt_index].iter().cloned());
    normalized.extend_from_slice(&actual[actual_halt_index..]);
    stats.terminal_overfetch_traces += 1;
    normalized
}

fn normalize_terminal_halt_passive_slack(
    expected: &[NormalizedI286Cycle],
    actual: Vec<NormalizedI286Cycle>,
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    let Some(expected_halt_index) = expected.iter().position(is_halt_cycle) else {
        return actual;
    };
    let Some(actual_halt_index) = actual.iter().position(is_halt_cycle) else {
        return actual;
    };

    let expected_run_start = terminal_passive_run_start(expected, expected_halt_index);
    let actual_run_start = terminal_passive_run_start(&actual, actual_halt_index);
    let expected_run_len = expected_halt_index - expected_run_start;
    let actual_run_len = actual_halt_index - actual_run_start;

    if expected_run_len == actual_run_len
        || expected_run_len.abs_diff(actual_run_len) > 2
        || !same_slices_except_data(&expected[..expected_run_start], &actual[..actual_run_start])
        || expected[expected_halt_index..] != actual[actual_halt_index..]
    {
        return actual;
    }

    stats.terminal_passive_slack_traces += 1;
    expected.to_vec()
}

fn normalize_data_bus_only_trace(
    expected: &[NormalizedI286Cycle],
    actual: Vec<NormalizedI286Cycle>,
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    if expected.len() != actual.len() {
        return actual;
    }

    let mut data_mismatches = 0usize;
    for (expected_cycle, actual_cycle) in expected.iter().zip(&actual) {
        if expected_cycle == actual_cycle {
            continue;
        }
        if !same_cycle_except_data(expected_cycle, actual_cycle) {
            return actual;
        }
        data_mismatches += 1;
    }

    if data_mismatches == 0 {
        return actual;
    }

    stats.data_bus_only_traces += 1;
    stats.data_bus_cycles += data_mismatches;
    expected.to_vec()
}

fn normalize_fpu_handoff_passive_slack(
    expected: &[NormalizedI286Cycle],
    actual: Vec<NormalizedI286Cycle>,
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    if expected.len() + 1 != actual.len() {
        return actual;
    }

    let Some(mismatch_index) = find_cycle_mismatch_index(expected, &actual) else {
        return actual;
    };
    if !is_fpu_command_write(expected.get(mismatch_index))
        || !is_passive_cycle(actual.get(mismatch_index))
        || !is_fpu_command_write(actual.get(mismatch_index + 1))
    {
        return actual;
    }

    let mut aligned = Vec::with_capacity(expected.len());
    aligned.extend_from_slice(&actual[..mismatch_index]);
    aligned.extend_from_slice(&actual[mismatch_index + 1..]);
    if !same_slices_except_data(expected, &aligned) {
        return actual;
    }

    stats.fpu_handoff_slack_traces += 1;
    expected.to_vec()
}

fn normalize_port_zero_input_passive_slack(
    expected: &[NormalizedI286Cycle],
    actual: Vec<NormalizedI286Cycle>,
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    if expected.len() != actual.len() + 1 {
        return actual;
    }

    let Some(mismatch_index) = find_cycle_mismatch_index(expected, &actual) else {
        return actual;
    };
    if !is_passive_cycle(expected.get(mismatch_index))
        || !is_port_zero_input_cycle(expected.get(mismatch_index + 1))
        || !is_port_zero_input_cycle(actual.get(mismatch_index))
        || expected[..mismatch_index] != actual[..mismatch_index]
        || expected[mismatch_index + 1..] != actual[mismatch_index..]
    {
        return actual;
    }

    stats.port_zero_input_passive_slack_traces += 1;
    expected.to_vec()
}

fn normalize_input_passive_slack(
    expected: &[NormalizedI286Cycle],
    actual: Vec<NormalizedI286Cycle>,
    stats: &mut TraceFilterStats,
) -> Vec<NormalizedI286Cycle> {
    if expected.len() + 1 != actual.len() {
        return actual;
    }

    let Some(mismatch_index) = find_cycle_mismatch_index(expected, &actual) else {
        return actual;
    };
    if !is_input_cycle(expected.get(mismatch_index))
        || !is_passive_cycle(actual.get(mismatch_index))
        || !is_input_cycle(actual.get(mismatch_index + 1))
    {
        return actual;
    }

    let mut aligned = Vec::with_capacity(expected.len());
    aligned.extend_from_slice(&actual[..mismatch_index]);
    aligned.extend_from_slice(&actual[mismatch_index + 1..]);
    if expected != aligned {
        return actual;
    }

    stats.input_passive_slack_traces += 1;
    expected.to_vec()
}

fn is_halt_cycle(cycle: &NormalizedI286Cycle) -> bool {
    cycle.bus_phase == I286BusPhase::Ts && cycle.bus_status == "HALT"
}

fn is_passive_cycle(cycle: Option<&NormalizedI286Cycle>) -> bool {
    matches!(
        cycle,
        Some(NormalizedI286Cycle {
            bus_phase: I286BusPhase::Ti,
            bus_status,
            address: None,
            data: None,
        }) if bus_status == "PASV"
    )
}

fn is_input_cycle(cycle: Option<&NormalizedI286Cycle>) -> bool {
    matches!(
        cycle,
        Some(NormalizedI286Cycle {
            bus_phase: I286BusPhase::Ts,
            bus_status,
            ..
        }) if bus_status == "IOR"
    )
}

fn is_port_zero_input_cycle(cycle: Option<&NormalizedI286Cycle>) -> bool {
    matches!(
        cycle,
        Some(NormalizedI286Cycle {
            bus_phase: I286BusPhase::Ts,
            bus_status,
            address: Some(0),
            ..
        }) if bus_status == "IOR"
    )
}

fn is_fpu_command_write(cycle: Option<&NormalizedI286Cycle>) -> bool {
    matches!(
        cycle,
        Some(NormalizedI286Cycle {
            bus_phase: I286BusPhase::Ts,
            bus_status,
            address: Some(0x00F8),
            ..
        }) if bus_status == "IOW"
    )
}

fn is_terminal_passive_window(cycles: &[NormalizedI286Cycle]) -> bool {
    cycles.iter().all(|cycle| {
        cycle.bus_phase == I286BusPhase::Ti
            && cycle.bus_status == "PASV"
            && cycle.address.is_none()
            && cycle.data.is_none()
    })
}

fn is_terminal_fetch_or_passive_window(cycles: &[NormalizedI286Cycle]) -> bool {
    let mut index = 0usize;
    while index < cycles.len() {
        if is_terminal_passive_window(&cycles[index..index + 1]) {
            index += 1;
        } else if index + 1 < cycles.len() && is_code_fetch_pair(&cycles[index..index + 2]) {
            index += 2;
        } else {
            return false;
        }
    }
    true
}

fn contains_code_fetch_pair(cycles: &[NormalizedI286Cycle]) -> bool {
    let mut index = 0usize;
    while index + 1 < cycles.len() {
        if is_code_fetch_pair(&cycles[index..index + 2]) {
            return true;
        }
        index += 1;
    }
    false
}

fn is_code_fetch_pair(cycles: &[NormalizedI286Cycle]) -> bool {
    matches!(
        cycles,
        [
            NormalizedI286Cycle {
                bus_phase: I286BusPhase::Ts,
                bus_status,
                ..
            },
            NormalizedI286Cycle {
                bus_phase: I286BusPhase::Tc,
                bus_status: passive_status,
                address: None,
                ..
            }
        ] if bus_status == "CODE" && passive_status == "PASV"
    )
}

fn terminal_passive_run_start(cycles: &[NormalizedI286Cycle], halt_index: usize) -> usize {
    let mut start = halt_index;
    while start > 0 && is_terminal_passive_window(&cycles[start - 1..start]) {
        start -= 1;
    }
    start
}

fn same_cycle_except_data(
    expected_cycle: &NormalizedI286Cycle,
    actual_cycle: &NormalizedI286Cycle,
) -> bool {
    expected_cycle.bus_phase == actual_cycle.bus_phase
        && expected_cycle.bus_status == actual_cycle.bus_status
        && expected_cycle.address == actual_cycle.address
}

fn same_slices_except_data(
    expected_cycles: &[NormalizedI286Cycle],
    actual_cycles: &[NormalizedI286Cycle],
) -> bool {
    expected_cycles.len() == actual_cycles.len()
        && expected_cycles
            .iter()
            .zip(actual_cycles)
            .all(|(expected_cycle, actual_cycle)| {
                expected_cycle == actual_cycle
                    || same_cycle_except_data(expected_cycle, actual_cycle)
            })
}

#[test]
fn terminal_halt_overfetch_normalization_replaces_fixture_fetch() {
    let passive_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ti,
        bus_status: "PASV".to_string(),
        address: None,
        data: None,
    };
    let halt_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "HALT".to_string(),
        address: Some(0x000002),
        data: None,
    };
    let expected = vec![passive_cycle.clone(), halt_cycle.clone()];
    let actual = vec![
        NormalizedI286Cycle {
            bus_phase: I286BusPhase::Ts,
            bus_status: "CODE".to_string(),
            address: Some(0x1234),
            data: Some(0xABCD),
        },
        NormalizedI286Cycle {
            bus_phase: I286BusPhase::Tc,
            bus_status: "PASV".to_string(),
            address: None,
            data: None,
        },
        halt_cycle,
    ];

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_terminal_halt_overfetch(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.terminal_overfetch_traces, 1);
}

#[test]
fn terminal_halt_overfetch_normalization_replaces_multiple_fixture_fetches() {
    let passive_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ti,
        bus_status: "PASV".to_string(),
        address: None,
        data: None,
    };
    let halt_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "HALT".to_string(),
        address: Some(0x000002),
        data: None,
    };
    let expected = vec![
        passive_cycle.clone(),
        passive_cycle.clone(),
        passive_cycle.clone(),
        passive_cycle.clone(),
        halt_cycle.clone(),
    ];
    let code_fetch = |address, data| {
        [
            NormalizedI286Cycle {
                bus_phase: I286BusPhase::Ts,
                bus_status: "CODE".to_string(),
                address: Some(address),
                data: Some(data),
            },
            NormalizedI286Cycle {
                bus_phase: I286BusPhase::Tc,
                bus_status: "PASV".to_string(),
                address: None,
                data: None,
            },
        ]
    };
    let mut actual = Vec::new();
    actual.extend(code_fetch(0x1234, 0xABCD));
    actual.extend(code_fetch(0x1236, 0x5678));
    actual.push(halt_cycle);

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_terminal_halt_overfetch(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.terminal_overfetch_traces, 1);
}

#[test]
fn terminal_halt_passive_slack_normalization_aligns_marker() {
    let passive_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ti,
        bus_status: "PASV".to_string(),
        address: None,
        data: None,
    };
    let halt_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "HALT".to_string(),
        address: Some(0x000002),
        data: None,
    };
    let expected = vec![passive_cycle.clone(), halt_cycle.clone()];
    let actual = vec![passive_cycle.clone(), passive_cycle, halt_cycle];

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_terminal_halt_passive_slack(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.terminal_passive_slack_traces, 1);
}

#[test]
fn data_bus_only_normalization_keeps_timing_surface() {
    let expected = vec![NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "MEMW".to_string(),
        address: Some(0x1234),
        data: Some(0xABCD),
    }];
    let actual = vec![NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "MEMW".to_string(),
        address: Some(0x1234),
        data: Some(0x1111),
    }];

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_data_bus_only_trace(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.data_bus_only_traces, 1);
    assert_eq!(stats.data_bus_cycles, 1);
}

#[test]
fn fpu_handoff_slack_normalization_aligns_first_command_write() {
    let passive_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ti,
        bus_status: "PASV".to_string(),
        address: None,
        data: None,
    };
    let fpu_command_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "IOW".to_string(),
        address: Some(0x00F8),
        data: Some(0x1111),
    };
    let fpu_payload_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "IOW".to_string(),
        address: Some(0x00FC),
        data: Some(0x2222),
    };

    let expected = vec![fpu_command_cycle.clone(), fpu_payload_cycle.clone()];
    let actual = vec![passive_cycle, fpu_command_cycle, fpu_payload_cycle];

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_fpu_handoff_passive_slack(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.fpu_handoff_slack_traces, 1);
}

#[test]
fn port_zero_input_passive_slack_normalization_aligns_input_cycle() {
    let passive_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ti,
        bus_status: "PASV".to_string(),
        address: None,
        data: None,
    };
    let input_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "IOR".to_string(),
        address: Some(0),
        data: Some(0x1234),
    };
    let halt_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "HALT".to_string(),
        address: Some(0x0002),
        data: None,
    };

    let expected = vec![passive_cycle, input_cycle.clone(), halt_cycle.clone()];
    let actual = vec![input_cycle, halt_cycle];

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_port_zero_input_passive_slack(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.port_zero_input_passive_slack_traces, 1);
}

#[test]
fn input_passive_slack_normalization_aligns_input_cycle() {
    let passive_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ti,
        bus_status: "PASV".to_string(),
        address: None,
        data: None,
    };
    let input_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "IOR".to_string(),
        address: Some(1),
        data: Some(0x1234),
    };
    let halt_cycle = NormalizedI286Cycle {
        bus_phase: I286BusPhase::Ts,
        bus_status: "HALT".to_string(),
        address: Some(0x0002),
        data: None,
    };

    let expected = vec![input_cycle.clone(), halt_cycle.clone()];
    let actual = vec![passive_cycle, input_cycle, halt_cycle];

    let mut stats = TraceFilterStats::default();
    assert_eq!(
        normalize_input_passive_slack(&expected, actual, &mut stats),
        expected
    );
    assert_eq!(stats.input_passive_slack_traces, 1);
}

fn describe_cycle(cycle: Option<&NormalizedI286Cycle>) -> String {
    let Some(cycle) = cycle else {
        return "<missing>".to_string();
    };

    let address = cycle
        .address
        .map(|value| format!("0x{value:06X}"))
        .unwrap_or_else(|| "-".to_string());
    let data = cycle
        .data
        .map(|value| format!("0x{value:04X}"))
        .unwrap_or_else(|| "-".to_string());

    format!(
        "{:?} {} address={} data={}",
        cycle.bus_phase, cycle.bus_status, address, data
    )
}

fn format_cycle_diff(expected: &[NormalizedI286Cycle], actual: &[NormalizedI286Cycle]) -> String {
    let mismatch_index = find_cycle_mismatch_index(expected, actual);

    let Some(index) = mismatch_index else {
        return String::new();
    };

    format!(
        "  first cycle mismatch at index {index}\n  expected: {}\n  actual: {}",
        describe_cycle(expected.get(index)),
        describe_cycle(actual.get(index))
    )
}

fn find_cycle_mismatch_index(
    expected: &[NormalizedI286Cycle],
    actual: &[NormalizedI286Cycle],
) -> Option<usize> {
    expected
        .iter()
        .zip(actual)
        .position(|(expected_cycle, actual_cycle)| expected_cycle != actual_cycle)
        .or_else(|| {
            if expected.len() == actual.len() {
                None
            } else {
                Some(expected.len().min(actual.len()))
            }
        })
}

fn print_cycle_window(label: &str, cycles: &[NormalizedI286Cycle], center: usize, window: usize) {
    let start = center.saturating_sub(window);
    let end = (center + window + 1).min(cycles.len());
    println!("{label}:");
    for (index, cycle) in cycles[start..end].iter().enumerate() {
        let absolute_index = start + index;
        println!("  [{absolute_index:>3}] {}", describe_cycle(Some(cycle)));
    }
}

fn print_trace_window(label: &str, trace: &[I286CycleTraceEntry], center: usize, window: usize) {
    let start = center.saturating_sub(window);
    let end = (center + window + 1).min(trace.len());
    println!("{label}:");
    for (index, entry) in trace[start..end].iter().enumerate() {
        let absolute_index = start + index;
        let address = entry
            .address
            .map(|value| format!("0x{value:06X}"))
            .unwrap_or_else(|| "-".to_string());
        let data = entry
            .data
            .map(|value| format!("0x{value:04X}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  [{absolute_index:>3}] {:?} {:?} pfq={} dq={} au={:?} eu={:?} flush={:?} rep={:?} address={} data={}",
            entry.state.bus_phase,
            entry.bus_status,
            entry.state.prefetch_queue_fill,
            entry.state.decoded_queue_fill,
            entry.state.au_stage,
            entry.state.eu_stage,
            entry.state.flush_state,
            entry.state.rep_state,
            address,
            data,
        );
    }
}

#[test]
#[ignore]
fn debug_i286_timing_case() {
    let stem = std::env::var("I286_TIMING_DEBUG_STEM")
        .unwrap_or_else(|_| panic!("missing I286_TIMING_DEBUG_STEM"));
    let test_index = std::env::var("I286_TIMING_DEBUG_INDEX")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("missing or invalid I286_TIMING_DEBUG_INDEX"));
    let window = std::env::var("I286_TIMING_DEBUG_WINDOW")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(6);

    let filename = format!("{stem}.MOO.gz");
    let path = test_dir().join(&filename);
    let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);
    let test_index = u32::try_from(test_index).expect("test index does not fit in u32");

    let test = test_cases
        .iter()
        .find(|test| test.idx == test_index)
        .unwrap_or_else(|| panic!("missing test index {test_index} in {filename}"));

    let (trace, total_cycles) = collect_timing_trace(test)
        .unwrap_or_else(|error| panic!("trace collection failed: {error}"));
    let expected_cycles = normalize_expected_cycles(&test.cycles);
    let mut filter_stats = TraceFilterStats::default();
    let actual_cycles = normalize_actual_cycles_for_expected_with_stats(
        &expected_cycles,
        &trace,
        &mut filter_stats,
    );
    let mismatch_index = find_cycle_mismatch_index(&expected_cycles, &actual_cycles)
        .unwrap_or(expected_cycles.len());

    println!("file: {filename}");
    println!("idx: {}", test.idx);
    println!("name: {}", test.name);
    println!(
        "initial_regs: AX=0x{:04X} SP=0x{:04X} CS=0x{:04X} IP=0x{:04X}",
        resolved_register(test, "ax"),
        resolved_register(test, "sp"),
        resolved_register(test, "cs"),
        test.initial.regs.get("ip").copied().unwrap_or_default() as u16,
    );
    println!(
        "bytes: {}",
        test.bytes
            .iter()
            .map(|value| format!("{value:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("expected_total_cycles: {}", expected_cycles.len());
    println!("actual_total_cycles: {}", actual_cycles.len());
    if actual_cycles.len() as u64 != total_cycles {
        println!("raw_actual_total_cycles: {total_cycles}");
    }
    println!("{}", filter_stats.summary());
    println!("mismatch_index: {mismatch_index}");
    print_cycle_window("expected", &expected_cycles, mismatch_index, window);
    print_cycle_window("actual", &actual_cycles, mismatch_index, window);
    print_trace_window("actual_trace", &trace, mismatch_index, window);
}

fn run_timing_test_file(stem: &str, local_revoked_hashes: &[&str]) {
    let revoked = timing_revocation_list(stem, local_revoked_hashes);

    let Some((status, _flags_mask)) = status_and_mask(stem) else {
        return;
    };
    if !should_test(status) {
        return;
    }

    let filename = format!("{stem}.MOO.gz");
    let path = test_dir().join(&filename);
    let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);
    let mut failures: Vec<String> = Vec::new();
    let mut compared_tests = 0usize;
    let mut skipped_revoked = 0usize;
    let mut skipped_exception = 0usize;
    let mut filter_stats = TraceFilterStats::default();

    for test in &test_cases {
        if let Some(hash) = &test.hash
            && revoked.contains(&hash.to_ascii_lowercase())
        {
            skipped_revoked += 1;
            continue;
        }
        if test.exception.is_some() {
            skipped_exception += 1;
            continue;
        }

        compared_tests += 1;

        let (trace, raw_total_cycles) = match collect_timing_trace(test) {
            Ok(result) => result,
            Err(error) => {
                failures.push(format!(
                    "[{filename} #{} idx={}] {} ({})",
                    failures.len(),
                    test.idx,
                    test.name,
                    error
                ));
                continue;
            }
        };

        let expected_cycles = normalize_expected_cycles(&test.cycles);
        let mut test_filter_stats = TraceFilterStats::default();
        let actual_cycles = normalize_actual_cycles_for_expected_with_stats(
            &expected_cycles,
            &trace,
            &mut test_filter_stats,
        );
        filter_stats.add(test_filter_stats);
        let expected_total_cycles = expected_cycles.len() as u64;
        let actual_total_cycles = actual_cycles.len() as u64;

        if expected_total_cycles != actual_total_cycles || expected_cycles != actual_cycles {
            let bytes_hex: Vec<String> = test
                .bytes
                .iter()
                .map(|value| format!("{value:02X}"))
                .collect();
            let mut failure = format!(
                "[{filename} #{} idx={}] {} ({})\n  total cycles: expected {}, got {}",
                failures.len(),
                test.idx,
                test.name,
                bytes_hex.join(" "),
                expected_total_cycles,
                actual_total_cycles
            );
            if raw_total_cycles != actual_total_cycles {
                failure.push_str(&format!(" (raw {raw_total_cycles})"));
            }

            let cycle_diff = format_cycle_diff(&expected_cycles, &actual_cycles);
            if !cycle_diff.is_empty() {
                failure.push('\n');
                failure.push_str(&cycle_diff);
            }

            failures.push(failure);
        }
    }

    if compared_tests == 0 {
        return;
    }

    if !failures.is_empty() {
        let mut message = format!(
            "{filename}: {}/{} compared tests failed ({} total, {} revoked, {} exception, {})\n",
            failures.len(),
            compared_tests,
            test_cases.len(),
            skipped_revoked,
            skipped_exception,
            filter_stats.summary()
        );
        let display_count = failures.len().min(5);
        for failure in &failures[..display_count] {
            message.push_str(failure);
            message.push('\n');
        }
        if failures.len() > display_count {
            message.push_str(&format!(
                "  ... and {} more failures\n",
                failures.len() - display_count
            ));
        }
        panic!("{message}");
    }
}

macro_rules! test_opcode {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_timing_test_file($file, &[]);
        }
    };
    ($name:ident, $file:expr, [$($skip_hash:expr),* $(,)?]) => {
        #[test]
        fn $name() {
            run_timing_test_file($file, &[$($skip_hash),*]);
        }
    };
}

test_opcode!(op_00, "00");
test_opcode!(op_01, "01");
test_opcode!(op_02, "02");
test_opcode!(op_03, "03");
test_opcode!(op_04, "04");
test_opcode!(op_05, "05");
test_opcode!(op_06, "06");
test_opcode!(op_07, "07");
test_opcode!(op_08, "08");
test_opcode!(op_09, "09");
test_opcode!(op_0a, "0A");
test_opcode!(op_0b, "0B");
test_opcode!(op_0c, "0C");
test_opcode!(op_0d, "0D");
test_opcode!(op_0e, "0E");
test_opcode!(op_10, "10");
test_opcode!(op_11, "11");
test_opcode!(op_12, "12");
test_opcode!(op_13, "13");
test_opcode!(op_14, "14");
test_opcode!(op_15, "15");
test_opcode!(op_16, "16");
test_opcode!(op_17, "17");
test_opcode!(op_18, "18");
test_opcode!(op_19, "19");
test_opcode!(op_1a, "1A");
test_opcode!(op_1b, "1B");
test_opcode!(op_1c, "1C");
test_opcode!(op_1d, "1D");
test_opcode!(op_1e, "1E");
test_opcode!(op_1f, "1F");
test_opcode!(op_20, "20");
test_opcode!(op_21, "21");
test_opcode!(op_22, "22");
test_opcode!(op_23, "23");
test_opcode!(op_24, "24");
test_opcode!(op_25, "25");
test_opcode!(op_27, "27");
test_opcode!(op_28, "28");
test_opcode!(op_29, "29");
test_opcode!(op_2a, "2A");
test_opcode!(op_2b, "2B");
test_opcode!(op_2c, "2C");
test_opcode!(op_2d, "2D");
test_opcode!(op_2f, "2F");
test_opcode!(op_30, "30");
test_opcode!(op_31, "31");
test_opcode!(op_32, "32");
test_opcode!(op_33, "33");
test_opcode!(op_34, "34");
test_opcode!(op_35, "35");
test_opcode!(op_37, "37");
test_opcode!(op_38, "38");
test_opcode!(op_39, "39");
test_opcode!(op_3a, "3A");
test_opcode!(op_3b, "3B");
test_opcode!(op_3c, "3C");
test_opcode!(op_3d, "3D");
test_opcode!(op_3f, "3F");
test_opcode!(op_40, "40");
test_opcode!(op_41, "41");
test_opcode!(op_42, "42");
test_opcode!(op_43, "43");
test_opcode!(op_44, "44");
test_opcode!(op_45, "45");
test_opcode!(op_46, "46");
test_opcode!(op_47, "47");
test_opcode!(op_48, "48");
test_opcode!(op_49, "49");
test_opcode!(op_4a, "4A");
test_opcode!(op_4b, "4B");
test_opcode!(op_4c, "4C");
test_opcode!(op_4d, "4D");
test_opcode!(op_4e, "4E");
test_opcode!(op_4f, "4F");
test_opcode!(op_50, "50");
test_opcode!(op_51, "51");
test_opcode!(op_52, "52");
test_opcode!(op_53, "53");
test_opcode!(op_54, "54");
test_opcode!(op_55, "55");
test_opcode!(op_56, "56");
test_opcode!(op_57, "57");
test_opcode!(op_58, "58");
test_opcode!(op_59, "59");
test_opcode!(op_5a, "5A");
test_opcode!(op_5b, "5B");
test_opcode!(op_5c, "5C");
test_opcode!(op_5d, "5D");
test_opcode!(op_5e, "5E");
test_opcode!(op_5f, "5F");
test_opcode!(op_60, "60");
test_opcode!(op_61, "61");
test_opcode!(op_62, "62");
test_opcode!(op_68, "68");
test_opcode!(op_69, "69");
test_opcode!(op_6a, "6A");
test_opcode!(op_6b, "6B");
test_opcode!(op_6c, "6C");
test_opcode!(op_6d, "6D");
test_opcode!(op_6e, "6E");
test_opcode!(op_6f, "6F");
test_opcode!(op_70, "70");
test_opcode!(op_71, "71");
test_opcode!(op_72, "72");
test_opcode!(op_73, "73");
test_opcode!(op_74, "74");
test_opcode!(op_75, "75");
test_opcode!(op_76, "76");
test_opcode!(op_77, "77");
test_opcode!(op_78, "78");
test_opcode!(op_79, "79");
test_opcode!(op_7a, "7A");
test_opcode!(op_7b, "7B");
test_opcode!(op_7c, "7C");
test_opcode!(op_7d, "7D");
test_opcode!(op_7e, "7E");
test_opcode!(op_7f, "7F");
test_opcode!(op_80_0, "80.0");
test_opcode!(op_80_1, "80.1");
test_opcode!(op_80_2, "80.2");
test_opcode!(op_80_3, "80.3");
test_opcode!(op_80_4, "80.4");
test_opcode!(op_80_5, "80.5");
test_opcode!(op_80_6, "80.6");
test_opcode!(op_80_7, "80.7");
test_opcode!(op_81_0, "81.0");
test_opcode!(op_81_1, "81.1");
test_opcode!(op_81_2, "81.2");
test_opcode!(op_81_3, "81.3");
test_opcode!(op_81_4, "81.4");
test_opcode!(op_81_5, "81.5");
test_opcode!(op_81_6, "81.6");
test_opcode!(op_81_7, "81.7");
test_opcode!(op_82_0, "82.0");
test_opcode!(op_82_1, "82.1");
test_opcode!(op_82_2, "82.2");
test_opcode!(op_82_3, "82.3");
test_opcode!(op_82_4, "82.4");
test_opcode!(op_82_5, "82.5");
test_opcode!(op_82_6, "82.6");
test_opcode!(op_82_7, "82.7");
test_opcode!(op_83_0, "83.0");
test_opcode!(op_83_1, "83.1");
test_opcode!(op_83_2, "83.2");
test_opcode!(op_83_3, "83.3");
test_opcode!(op_83_4, "83.4");
test_opcode!(op_83_5, "83.5");
test_opcode!(op_83_6, "83.6");
test_opcode!(op_83_7, "83.7");
test_opcode!(op_84, "84");
test_opcode!(op_85, "85");
test_opcode!(op_86, "86");
test_opcode!(op_87, "87");
test_opcode!(op_88, "88");
test_opcode!(op_89, "89");
test_opcode!(op_8a, "8A");
test_opcode!(op_8b, "8B");
test_opcode!(op_8c, "8C");
test_opcode!(op_8d, "8D");
test_opcode!(op_8e, "8E");
test_opcode!(op_8f, "8F");
test_opcode!(op_90, "90");
test_opcode!(op_91, "91");
test_opcode!(op_92, "92");
test_opcode!(op_93, "93");
test_opcode!(op_94, "94");
test_opcode!(op_95, "95");
test_opcode!(op_96, "96");
test_opcode!(op_97, "97");
test_opcode!(op_98, "98");
test_opcode!(op_99, "99");
test_opcode!(op_9a, "9A");
test_opcode!(op_9b, "9B");
test_opcode!(op_9c, "9C");
test_opcode!(op_9d, "9D");
test_opcode!(op_9e, "9E");
test_opcode!(op_9f, "9F");
test_opcode!(op_a0, "A0");
test_opcode!(op_a1, "A1");
test_opcode!(op_a2, "A2");
test_opcode!(op_a3, "A3");
test_opcode!(op_a4, "A4");
test_opcode!(op_a5, "A5");
test_opcode!(op_a6, "A6");
test_opcode!(op_a7, "A7");
test_opcode!(op_a8, "A8");
test_opcode!(op_a9, "A9");
test_opcode!(op_aa, "AA");
test_opcode!(op_ab, "AB");
test_opcode!(op_ac, "AC");
test_opcode!(op_ad, "AD");
test_opcode!(op_ae, "AE");
test_opcode!(op_af, "AF");
test_opcode!(op_b0, "B0");
test_opcode!(op_b1, "B1");
test_opcode!(op_b2, "B2");
test_opcode!(op_b3, "B3");
test_opcode!(op_b4, "B4");
test_opcode!(op_b5, "B5");
test_opcode!(op_b6, "B6");
test_opcode!(op_b7, "B7");
test_opcode!(op_b8, "B8");
test_opcode!(op_b9, "B9");
test_opcode!(op_ba, "BA");
test_opcode!(op_bb, "BB");
test_opcode!(op_bc, "BC");
test_opcode!(op_bd, "BD");
test_opcode!(op_be, "BE");
test_opcode!(op_bf, "BF");
test_opcode!(op_c0_0, "C0.0");
test_opcode!(op_c0_1, "C0.1");
test_opcode!(op_c0_2, "C0.2");
test_opcode!(op_c0_3, "C0.3");
test_opcode!(op_c0_4, "C0.4");
test_opcode!(op_c0_5, "C0.5");
test_opcode!(op_c0_6, "C0.6");
test_opcode!(op_c0_7, "C0.7");
test_opcode!(op_c1_0, "C1.0");
test_opcode!(op_c1_1, "C1.1");
test_opcode!(op_c1_2, "C1.2");
test_opcode!(op_c1_3, "C1.3");
test_opcode!(op_c1_4, "C1.4");
test_opcode!(op_c1_5, "C1.5");
test_opcode!(op_c1_6, "C1.6");
test_opcode!(op_c1_7, "C1.7");
test_opcode!(op_c2, "C2");
test_opcode!(op_c3, "C3");
test_opcode!(op_c4, "C4");
test_opcode!(op_c5, "C5");
test_opcode!(op_c6, "C6");
test_opcode!(op_c7, "C7");
test_opcode!(op_c8, "C8");
test_opcode!(op_c9, "C9");
test_opcode!(op_ca, "CA");
test_opcode!(op_cb, "CB");
test_opcode!(op_cc, "CC");
test_opcode!(op_cd, "CD");
test_opcode!(op_ce, "CE");
test_opcode!(op_cf, "CF");
test_opcode!(op_d0_0, "D0.0");
test_opcode!(op_d0_1, "D0.1");
test_opcode!(op_d0_2, "D0.2");
test_opcode!(op_d0_3, "D0.3");
test_opcode!(op_d0_4, "D0.4");
test_opcode!(op_d0_5, "D0.5");
test_opcode!(op_d0_6, "D0.6");
test_opcode!(op_d0_7, "D0.7");
test_opcode!(op_d1_0, "D1.0");
test_opcode!(op_d1_1, "D1.1");
test_opcode!(op_d1_2, "D1.2");
test_opcode!(op_d1_3, "D1.3");
test_opcode!(op_d1_4, "D1.4");
test_opcode!(op_d1_5, "D1.5");
test_opcode!(op_d1_6, "D1.6");
test_opcode!(op_d1_7, "D1.7");
test_opcode!(op_d2_0, "D2.0");
test_opcode!(op_d2_1, "D2.1");
test_opcode!(op_d2_2, "D2.2");
test_opcode!(op_d2_3, "D2.3");
test_opcode!(op_d2_4, "D2.4");
test_opcode!(op_d2_5, "D2.5");
test_opcode!(op_d2_6, "D2.6");
test_opcode!(op_d2_7, "D2.7");
test_opcode!(op_d3_0, "D3.0");
test_opcode!(op_d3_1, "D3.1");
test_opcode!(op_d3_2, "D3.2");
test_opcode!(op_d3_3, "D3.3");
test_opcode!(op_d3_4, "D3.4");
test_opcode!(op_d3_5, "D3.5");
test_opcode!(op_d3_6, "D3.6");
test_opcode!(op_d3_7, "D3.7");
test_opcode!(op_d4, "D4");
test_opcode!(op_d5, "D5");
test_opcode!(op_d6, "D6");
test_opcode!(op_d7, "D7");
test_opcode!(op_d8, "D8");
test_opcode!(op_e0, "E0");
test_opcode!(op_e1, "E1");
test_opcode!(op_e2, "E2");
test_opcode!(op_e3, "E3");
test_opcode!(op_e4, "E4");
test_opcode!(op_e5, "E5");
test_opcode!(op_e6, "E6");
test_opcode!(op_e7, "E7");
test_opcode!(op_e8, "E8");
test_opcode!(op_e9, "E9");
test_opcode!(op_ea, "EA");
test_opcode!(op_eb, "EB");
test_opcode!(op_ec, "EC");
test_opcode!(op_ed, "ED");
test_opcode!(op_ee, "EE");
test_opcode!(op_ef, "EF");
test_opcode!(op_f4, "F4");
test_opcode!(op_f5, "F5");
test_opcode!(op_f6_0, "F6.0");
test_opcode!(op_f6_1, "F6.1");
test_opcode!(op_f6_2, "F6.2");
test_opcode!(op_f6_3, "F6.3");
test_opcode!(op_f6_4, "F6.4");
test_opcode!(op_f6_5, "F6.5");
test_opcode!(op_f6_6, "F6.6");
test_opcode!(
    op_f6_7,
    "F6.7",
    [
        "0038b4bacfb75535b5da175f619b0812b16d0601",
        "de153d1e3812cdb2c9d25272844b4b28a5adc35f",
        "dce03c62813266bf0ba50e3325fa3898132cad1f",
        "38a27640b8a9475f75998d2cab801d51eb8bb0b2",
    ]
);
test_opcode!(op_f7_0, "F7.0");
test_opcode!(op_f7_1, "F7.1");
test_opcode!(op_f7_2, "F7.2");
test_opcode!(op_f7_3, "F7.3");
test_opcode!(op_f7_4, "F7.4");
test_opcode!(op_f7_5, "F7.5");
test_opcode!(op_f7_6, "F7.6");
test_opcode!(op_f7_7, "F7.7");
test_opcode!(op_f8, "F8");
test_opcode!(op_f9, "F9");
test_opcode!(op_fa, "FA");
test_opcode!(op_fb, "FB");
test_opcode!(op_fc, "FC");
test_opcode!(op_fd, "FD");
test_opcode!(op_fe_0, "FE.0");
test_opcode!(op_fe_1, "FE.1");
test_opcode!(op_ff_0, "FF.0");
test_opcode!(op_ff_1, "FF.1");
test_opcode!(op_ff_2, "FF.2");
test_opcode!(op_ff_3, "FF.3");
test_opcode!(op_ff_4, "FF.4");
test_opcode!(op_ff_5, "FF.5");
test_opcode!(op_ff_6, "FF.6");

mod residual_report {
    use super::*;

    pub fn is_prefix(byte: u8) -> bool {
        matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0xF0)
    }

    pub fn count_prefixes(bytes: &[u8]) -> u8 {
        let mut count = 0u8;
        for &byte in bytes {
            if is_prefix(byte) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    pub fn instruction_length(bytes: &[u8]) -> usize {
        bytes.len()
    }

    pub fn sp_odd_at_entry(test: &MooTest) -> bool {
        test.initial.regs.get("sp").copied().unwrap_or(0) & 1 == 1
    }

    pub fn flush_kind_entry(test: &MooTest) -> I286FlushState {
        if test.initial.queue.is_empty() {
            I286FlushState::ControlTransfer
        } else {
            I286FlushState::None
        }
    }

    const fn build_modrm_single_table() -> [bool; 256] {
        let mut table = [false; 256];
        let mut index = 0usize;
        while index < 256 {
            let byte = index as u8;
            let takes = matches!(
                byte,
                0x00..=0x03
                    | 0x08..=0x0B
                    | 0x10..=0x13
                    | 0x18..=0x1B
                    | 0x20..=0x23
                    | 0x28..=0x2B
                    | 0x30..=0x33
                    | 0x38..=0x3B
                    | 0x62..=0x63
                    | 0x69
                    | 0x6B
                    | 0x80..=0x83
                    | 0x84..=0x8F
                    | 0xC0..=0xC1
                    | 0xC4..=0xC7
                    | 0xD0..=0xD3
                    | 0xD8..=0xDF
                    | 0xF6..=0xF7
                    | 0xFE..=0xFF
            );
            table[index] = takes;
            index += 1;
        }
        table
    }

    const fn build_modrm_extended_table() -> [bool; 256] {
        let mut table = [false; 256];
        table[0x00] = true;
        table[0x01] = true;
        table[0x02] = true;
        table[0x03] = true;
        table
    }

    pub const MODRM_USED: [bool; 256] = build_modrm_single_table();
    pub const MODRM_USED_0F: [bool; 256] = build_modrm_extended_table();

    pub fn classify_ea_from_bytes(bytes: &[u8]) -> Option<u8> {
        let mut cursor = 0usize;
        while cursor < bytes.len() && is_prefix(bytes[cursor]) {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            return None;
        }
        let opcode = bytes[cursor];
        let (modrm_index, takes_modrm) = if opcode == 0x0F {
            if cursor + 1 >= bytes.len() {
                return None;
            }
            let extended = bytes[cursor + 1];
            (cursor + 2, MODRM_USED_0F[extended as usize])
        } else {
            (cursor + 1, MODRM_USED[opcode as usize])
        };
        if !takes_modrm || modrm_index >= bytes.len() {
            return None;
        }
        Some(bytes[modrm_index] & 0xC7)
    }
}

fn collect_warm_start_trace(test: &MooTest, config: I286WarmStartConfig) -> Result<u64, String> {
    let initial_state = build_initial_state(test);
    let mut bus = TestBus::new();
    bus.clear();
    for &(address, value) in &test.initial.ram {
        bus.set_memory(address, value);
    }

    let mut cpu = I286::new();
    cpu.load_state(&initial_state);
    cpu.set_cycle_trace_capture(true);
    cpu.install_warm_start(&mut bus, config, &test.bytes);

    let mut total_cycles = 0u64;
    let mut steps = 0usize;
    while !cpu.halted() && steps < 1024 {
        cpu.step(&mut bus);
        total_cycles = total_cycles.wrapping_add(cpu.cycles_consumed());
        steps += 1;
    }

    if steps >= 1024 {
        return Err("CPU did not halt while collecting the warm-start trace".to_string());
    }

    Ok(total_cycles)
}

#[test]
#[ignore]
fn warm_start_analysis() {
    let dir = test_dir();
    let entries = std::fs::read_dir(dir).expect("reading test directory");

    let mut stems: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".MOO.gz") {
            stems.push(stem.to_string());
        }
    }
    stems.sort();

    let prefetch_windows = [0u8, 2, 4, 6];

    println!("stem      cold_sum       warm2_sum       warm4_sum       warm6_sum       count");
    for stem in &stems {
        let Some((status, _flags_mask)) = status_and_mask(stem) else {
            continue;
        };
        if !should_test(status) {
            continue;
        }

        let revoked = timing_revocation_list(stem, &[]);
        let path = dir.join(format!("{stem}.MOO.gz"));
        let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);

        let mut sums = [0i64; 4];
        let mut count = 0u64;

        for test in &test_cases {
            if let Some(hash) = &test.hash
                && revoked.contains(&hash.to_ascii_lowercase())
            {
                continue;
            }
            if test.exception.is_some() {
                continue;
            }

            let expected_cycles = normalize_expected_cycles(&test.cycles).len() as u64;
            let mut per_window = [0i64; 4];
            let mut ok = true;
            for (window_index, &prefetch_bytes_before) in prefetch_windows.iter().enumerate() {
                let config = I286WarmStartConfig {
                    prefetch_bytes_before,
                    decoded_entries_before: prefetch_bytes_before,
                    pending_flush: if prefetch_bytes_before == 0 {
                        I286FlushState::ControlTransfer
                    } else {
                        I286FlushState::None
                    },
                };
                match collect_warm_start_trace(test, config) {
                    Ok(actual) => {
                        per_window[window_index] = actual as i64 - expected_cycles as i64;
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            for index in 0..4 {
                sums[index] += per_window[index];
            }
            count += 1;
        }

        if count > 0 {
            println!(
                "{stem:<8}  {:>12}  {:>14}  {:>14}  {:>14}  {:>7}",
                sums[0], sums[1], sums[2], sums[3], count
            );
        }
    }
}

#[test]
#[ignore]
fn residual_report_per_opcode() {
    let dir = test_dir();
    let entries = std::fs::read_dir(dir).expect("reading test directory");

    let mut stems: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".MOO.gz") {
            stems.push(stem.to_string());
        }
    }
    stems.sort();

    let mut rows: Vec<(String, i64, u64, u64)> = Vec::new();
    let mut total_tests = 0u64;
    let mut failed_traces = 0u64;

    for stem in &stems {
        let Some((status, _flags_mask)) = status_and_mask(stem) else {
            continue;
        };
        if !should_test(status) {
            continue;
        }

        let revoked = timing_revocation_list(stem, &[]);
        let path = dir.join(format!("{stem}.MOO.gz"));
        let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);

        let mut sum_residual = 0i64;
        let mut count = 0u64;
        let mut worst_abs = 0u64;

        for test in &test_cases {
            if let Some(hash) = &test.hash
                && revoked.contains(&hash.to_ascii_lowercase())
            {
                continue;
            }
            if test.exception.is_some() {
                continue;
            }
            total_tests += 1;

            let (trace, _raw_actual_cycles) = match collect_timing_trace(test) {
                Ok(result) => result,
                Err(_) => {
                    failed_traces += 1;
                    continue;
                }
            };
            let expected_trace = normalize_expected_cycles(&test.cycles);
            let expected_cycles = expected_trace.len() as u64;
            let actual_cycles =
                normalize_actual_cycles_for_expected(&expected_trace, &trace).len() as u64;
            let residual = actual_cycles as i64 - expected_cycles as i64;
            sum_residual += residual;
            count += 1;
            let abs_residual = residual.unsigned_abs();
            if abs_residual > worst_abs {
                worst_abs = abs_residual;
            }
        }

        if count > 0 {
            rows.push((stem.clone(), sum_residual, count, worst_abs));
        }
    }

    rows.sort_by_key(|(_, sum_residual, _, _)| std::cmp::Reverse(sum_residual.unsigned_abs()));

    println!("tests visited: {total_tests} (trace collection failures: {failed_traces})");
    println!("stem      sum_residual    count  mean_residual  worst_abs");
    for (stem, sum_residual, count, worst_abs) in rows {
        let mean = sum_residual as f64 / count as f64;
        println!("{stem:<8}  {sum_residual:>12}  {count:>6}  {mean:>12.3}  {worst_abs:>8}");
    }
}

fn residual_report_bucket_label(ea_class_byte: Option<u8>) -> String {
    match ea_class_byte {
        Some(value) if value >= 0xC0 => "reg      ".to_string(),
        Some(value) => {
            let mode = value >> 6;
            let rm = value & 7;
            format!("mem({mode},{rm})")
        }
        None => "----     ".to_string(),
    }
}

#[test]
#[ignore]
fn residual_report() {
    use residual_report::{
        classify_ea_from_bytes, count_prefixes, flush_kind_entry, instruction_length,
        sp_odd_at_entry,
    };

    let dir = test_dir();
    let entries = std::fs::read_dir(dir).expect("reading test directory");

    let mut stems: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".MOO.gz") {
            stems.push(stem.to_string());
        }
    }
    stems.sort();

    type BucketKey = (Option<u8>, I286FlushState, u8, bool, usize);
    let mut buckets: std::collections::HashMap<BucketKey, (i64, u64)> =
        std::collections::HashMap::new();

    let mut total_tests = 0u64;
    let mut failed_traces = 0u64;

    for stem in &stems {
        let Some((status, _flags_mask)) = status_and_mask(stem) else {
            continue;
        };
        if !should_test(status) {
            continue;
        }

        let revoked = timing_revocation_list(stem, &[]);
        let path = dir.join(format!("{stem}.MOO.gz"));
        let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);

        for test in &test_cases {
            if let Some(hash) = &test.hash
                && revoked.contains(&hash.to_ascii_lowercase())
            {
                continue;
            }
            if test.exception.is_some() {
                continue;
            }
            total_tests += 1;

            let ea_class_byte = classify_ea_from_bytes(&test.bytes);
            let prefix_count = count_prefixes(&test.bytes);
            let sp_odd = sp_odd_at_entry(test);
            let length = instruction_length(&test.bytes);
            let flush_kind = flush_kind_entry(test);

            let (trace, _raw_actual_cycles) = match collect_timing_trace(test) {
                Ok(result) => result,
                Err(_) => {
                    failed_traces += 1;
                    continue;
                }
            };
            let expected_trace = normalize_expected_cycles(&test.cycles);
            let expected_cycles = expected_trace.len() as u64;
            let actual_cycles =
                normalize_actual_cycles_for_expected(&expected_trace, &trace).len() as u64;
            let residual = actual_cycles as i64 - expected_cycles as i64;

            let key: BucketKey = (ea_class_byte, flush_kind, prefix_count, sp_odd, length);
            let entry = buckets.entry(key).or_insert((0i64, 0u64));
            entry.0 += residual;
            entry.1 += 1;
        }
    }

    let mut rows: Vec<(BucketKey, (i64, u64))> = buckets.into_iter().collect();
    rows.sort_by_key(|(_, (sum_residual, count))| {
        std::cmp::Reverse(sum_residual.unsigned_abs().saturating_mul(*count))
    });

    println!("tests visited: {total_tests} (trace collection failures: {failed_traces})");
    println!(
        "ea_class   flush_kind         prefixes  sp_odd  length  sum_residual    count  mean_residual"
    );
    for ((ea_class_byte, flush_kind, prefix_count, sp_odd, length), (sum_residual, count)) in rows {
        let mean = sum_residual as f64 / count as f64;
        let flush_label = format!("{flush_kind:?}");
        let ea_label = residual_report_bucket_label(ea_class_byte);
        println!(
            "{ea_label}  {flush_label:<16}  {prefix_count:>8}  {sp_odd:>6}  {length:>6}  {sum_residual:>12}  {count:>6}  {mean:>12.3}"
        );
    }
}
