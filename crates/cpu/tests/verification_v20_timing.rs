#![cfg(feature = "verification")]

#[path = "common/metadata_json.rs"]
mod metadata_json;
#[path = "common/verification_common.rs"]
mod verification_common;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
};

use cpu::{V30, V30BusPhase, V30CycleTraceEntry, V30QueueOpTrace, V30State, V30TraceBusStatus};
use metadata_json::{Metadata, load_metadata};
use verification_common::{MooCycle, MooTest, MooV20Cycle, load_moo_tests, load_revocation_list};

const RAM_SIZE: usize = 1024 * 1024;
const ADDRESS_MASK: u32 = 0x000F_FFFF;
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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/SingleStepTests/v20/v1_native")
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
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/SingleStepTests/v20/revocation_list.txt");
        if path.exists() {
            load_revocation_list(&path)
        } else {
            HashSet::new()
        }
    });
    &REVOKED
}

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

fn build_initial_state(test: &MooTest) -> V30State {
    let get = |name: &str| -> u16 {
        test.initial
            .regs
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("missing register in initial state: {name}")) as u16
    };

    let mut state = V30State::default();
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
    state
}

fn collect_timing_trace(test: &MooTest) -> Result<(Vec<V30CycleTraceEntry>, u64), String> {
    let initial_state = build_initial_state(test);
    let mut bus = TestBus::new();
    bus.clear();
    for &(address, value) in &test.initial.ram {
        bus.set_memory(address, value);
    }

    let mut cpu = V30::new();
    cpu.load_state(&initial_state);
    cpu.set_cycle_trace_capture(true);
    if !test.initial.queue.is_empty() {
        cpu.install_prefetch_queue(&test.initial.queue);
    }

    cpu.step(&mut bus);
    let total_cycles = cpu.cycles_consumed();
    let trace = cpu.drain_cycle_trace();

    Ok((trace, total_cycles))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedV20Cycle {
    bus_phase: V30BusPhase,
    bus_status: V30TraceBusStatus,
    address: Option<u32>,
    data: Option<u8>,
}

fn map_v20_bus_status(status: &str) -> V30TraceBusStatus {
    match status {
        "CODE" => V30TraceBusStatus::Code,
        "MEMR" => V30TraceBusStatus::MemoryRead,
        "MEMW" => V30TraceBusStatus::MemoryWrite,
        "IOR" => V30TraceBusStatus::IoRead,
        "IOW" => V30TraceBusStatus::IoWrite,
        "HALT" => V30TraceBusStatus::Halt,
        "INTA" => V30TraceBusStatus::InterruptAck,
        _ => V30TraceBusStatus::Passive,
    }
}

fn map_v20_t_state(t_state: &str) -> V30BusPhase {
    match t_state {
        "T1" => V30BusPhase::T1,
        "T2" => V30BusPhase::T2,
        "T3" => V30BusPhase::T3,
        "Tw" => V30BusPhase::Tw,
        "T4" => V30BusPhase::T4,
        _ => V30BusPhase::Ti,
    }
}

fn comparable_data(status: V30TraceBusStatus) -> bool {
    matches!(
        status,
        V30TraceBusStatus::Code
            | V30TraceBusStatus::MemoryRead
            | V30TraceBusStatus::MemoryWrite
            | V30TraceBusStatus::IoRead
            | V30TraceBusStatus::IoWrite
    )
}

fn normalize_expected_cycles(cycles: &[MooCycle]) -> Vec<NormalizedV20Cycle> {
    cycles
        .iter()
        .filter_map(|cycle| match cycle {
            MooCycle::V20(MooV20Cycle {
                bus_value,
                data,
                bus_status,
                t_state,
                pin_bitfield,
                ..
            }) => {
                let status = map_v20_bus_status(bus_status);
                let phase = map_v20_t_state(t_state);
                let ale = (pin_bitfield & 0x01) != 0;
                let address = if ale {
                    Some(*bus_value & ADDRESS_MASK)
                } else {
                    None
                };
                let data = if comparable_data(status) {
                    Some(*data)
                } else {
                    None
                };
                Some(NormalizedV20Cycle {
                    bus_phase: phase,
                    bus_status: status,
                    address,
                    data,
                })
            }
            MooCycle::Legacy(_) | MooCycle::I286(_) => None,
        })
        .collect()
}

fn actual_trace_start_index(trace: &[V30CycleTraceEntry]) -> usize {
    trace
        .iter()
        .position(|entry| entry.queue_op == V30QueueOpTrace::First)
        .unwrap_or(0)
}

fn normalize_actual_cycles_from(
    trace: &[V30CycleTraceEntry],
    start_index: usize,
) -> Vec<NormalizedV20Cycle> {
    trace[start_index.min(trace.len())..]
        .iter()
        .map(|entry| NormalizedV20Cycle {
            bus_phase: entry.phase,
            bus_status: entry.status,
            address: if entry.ale {
                Some(entry.address & ADDRESS_MASK)
            } else {
                None
            },
            data: if comparable_data(entry.status) {
                Some(entry.data)
            } else {
                None
            },
        })
        .collect()
}

fn normalize_actual_cycles(trace: &[V30CycleTraceEntry]) -> Vec<NormalizedV20Cycle> {
    normalize_actual_cycles_from(trace, actual_trace_start_index(trace))
}

fn describe_cycle(cycle: Option<&NormalizedV20Cycle>) -> String {
    let Some(cycle) = cycle else {
        return "<missing>".to_string();
    };
    let address = cycle
        .address
        .map(|value| format!("0x{value:05X}"))
        .unwrap_or_else(|| "-".to_string());
    let data = cycle
        .data
        .map(|value| format!("0x{value:02X}"))
        .unwrap_or_else(|| "-".to_string());
    format!(
        "{:?} {:?} address={} data={}",
        cycle.bus_phase, cycle.bus_status, address, data
    )
}

fn find_cycle_mismatch_index(
    expected: &[NormalizedV20Cycle],
    actual: &[NormalizedV20Cycle],
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

fn format_cycle_diff(expected: &[NormalizedV20Cycle], actual: &[NormalizedV20Cycle]) -> String {
    let Some(index) = find_cycle_mismatch_index(expected, actual) else {
        return String::new();
    };
    format!(
        "  first cycle mismatch at index {index}\n  expected: {}\n  actual: {}",
        describe_cycle(expected.get(index)),
        describe_cycle(actual.get(index))
    )
}

fn timing_test_stems() -> Vec<String> {
    let entries = std::fs::read_dir(test_dir()).expect("reading V20 test directory");
    let mut stems: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".MOO.gz").map(ToOwned::to_owned)
        })
        .collect();
    stems.sort();
    stems
}

fn skipped_timing_test(test: &MooTest, revoked: &HashSet<String>) -> bool {
    if let Some(hash) = &test.hash
        && revoked.contains(&hash.to_ascii_lowercase())
    {
        return true;
    }

    test.exception.is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum V20ResidualEaClass {
    None,
    Reg,
    Mem(u8, u8),
}

fn is_prefix(byte: u8) -> bool {
    matches!(
        byte,
        0x26 | 0x2E | 0x36 | 0x3E | 0xF0 | 0x64 | 0x65 | 0xF2 | 0xF3
    )
}

fn count_prefixes(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|&&byte| is_prefix(byte)).count()
}

fn opcode_after_prefixes(bytes: &[u8]) -> Option<(usize, u8)> {
    let opcode_index = count_prefixes(bytes);
    bytes
        .get(opcode_index)
        .copied()
        .map(|opcode| (opcode_index, opcode))
}

fn opcode_uses_modrm(opcode: u8) -> bool {
    matches!(
        opcode,
        0x00..=0x03
            | 0x08..=0x0B
            | 0x10..=0x13
            | 0x18..=0x1B
            | 0x20..=0x23
            | 0x28..=0x2B
            | 0x30..=0x33
            | 0x38..=0x3B
            | 0x62
            | 0x69
            | 0x6B
            | 0x80..=0x8F
            | 0xC0..=0xC1
            | 0xC4..=0xC7
            | 0xD0..=0xD3
            | 0xD8..=0xDF
            | 0xF6..=0xF7
            | 0xFE..=0xFF
    )
}

fn extended_opcode_uses_modrm(opcode: u8) -> bool {
    matches!(
        opcode,
        0x10..=0x1F | 0x20 | 0x22 | 0x26 | 0x28 | 0x2A | 0x31 | 0x33 | 0x39 | 0x3B
    )
}

fn residual_ea_class(bytes: &[u8]) -> V20ResidualEaClass {
    let Some((opcode_index, opcode)) = opcode_after_prefixes(bytes) else {
        return V20ResidualEaClass::None;
    };

    let modrm_index = if opcode == 0x0F {
        let Some(&extended_opcode) = bytes.get(opcode_index + 1) else {
            return V20ResidualEaClass::None;
        };
        if !extended_opcode_uses_modrm(extended_opcode) {
            return V20ResidualEaClass::None;
        }
        opcode_index + 2
    } else {
        if !opcode_uses_modrm(opcode) {
            return V20ResidualEaClass::None;
        }
        opcode_index + 1
    };

    let Some(&modrm) = bytes.get(modrm_index) else {
        return V20ResidualEaClass::None;
    };
    if modrm >= 0xC0 {
        V20ResidualEaClass::Reg
    } else {
        V20ResidualEaClass::Mem(modrm >> 6, modrm & 0x07)
    }
}

fn residual_ea_label(class: V20ResidualEaClass) -> String {
    match class {
        V20ResidualEaClass::None => "----     ".to_string(),
        V20ResidualEaClass::Reg => "reg      ".to_string(),
        V20ResidualEaClass::Mem(mode, rm) => format!("mem({mode},{rm})"),
    }
}

#[test]
#[ignore]
fn residual_report_per_opcode() {
    let mut rows: Vec<(String, i64, u64, u64, u64)> = Vec::new();
    let mut total_tests = 0u64;
    let mut failed_traces = 0u64;

    for stem in timing_test_stems() {
        let Some((status, _flags_mask)) = status_and_mask(&stem) else {
            continue;
        };
        if !should_test(status) {
            continue;
        }

        let revoked = timing_revocation_list(&stem, &[]);
        let path = test_dir().join(format!("{stem}.MOO.gz"));
        let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);
        let mut sum_residual = 0i64;
        let mut count = 0u64;
        let mut failures = 0u64;
        let mut worst_abs = 0u64;

        for test in &test_cases {
            if skipped_timing_test(test, &revoked) {
                continue;
            }

            total_tests += 1;
            let (trace, _raw_total_cycles) = match collect_timing_trace(test) {
                Ok(result) => result,
                Err(_) => {
                    failed_traces += 1;
                    continue;
                }
            };

            let expected_cycles = normalize_expected_cycles(&test.cycles);
            let actual_cycles = normalize_actual_cycles(&trace);
            let residual = actual_cycles.len() as i64 - expected_cycles.len() as i64;
            if residual != 0 || expected_cycles != actual_cycles {
                failures += 1;
            }
            sum_residual += residual;
            count += 1;
            worst_abs = worst_abs.max(residual.unsigned_abs());
        }

        if count > 0 {
            rows.push((stem, sum_residual, count, failures, worst_abs));
        }
    }

    rows.sort_by_key(|(_, sum_residual, _, _, _)| std::cmp::Reverse(sum_residual.unsigned_abs()));

    println!("tests visited: {total_tests} (trace collection failures: {failed_traces})");
    println!("stem      sum_residual    count  failures  mean_residual  worst_abs");
    for (stem, sum_residual, count, failures, worst_abs) in rows {
        let mean = sum_residual as f64 / count as f64;
        println!(
            "{stem:<8}  {sum_residual:>12}  {count:>6}  {failures:>8}  {mean:>12.3}  {worst_abs:>8}"
        );
    }
}

#[test]
#[ignore]
fn residual_report() {
    type BucketKey = (usize, usize, usize, V20ResidualEaClass);

    let mut buckets: HashMap<BucketKey, (i64, u64, u64, u64)> = HashMap::new();
    let mut total_tests = 0u64;
    let mut failed_traces = 0u64;

    for stem in timing_test_stems() {
        let Some((status, _flags_mask)) = status_and_mask(&stem) else {
            continue;
        };
        if !should_test(status) {
            continue;
        }

        let revoked = timing_revocation_list(&stem, &[]);
        let path = test_dir().join(format!("{stem}.MOO.gz"));
        let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);

        for test in &test_cases {
            if skipped_timing_test(test, &revoked) {
                continue;
            }

            total_tests += 1;
            let (trace, _raw_total_cycles) = match collect_timing_trace(test) {
                Ok(result) => result,
                Err(_) => {
                    failed_traces += 1;
                    continue;
                }
            };

            let expected_cycles = normalize_expected_cycles(&test.cycles);
            let actual_cycles = normalize_actual_cycles(&trace);
            let residual = actual_cycles.len() as i64 - expected_cycles.len() as i64;
            let key = (
                test.initial.queue.len(),
                count_prefixes(&test.bytes),
                test.bytes.len(),
                residual_ea_class(&test.bytes),
            );
            let entry = buckets.entry(key).or_insert((0, 0, 0, 0));
            entry.0 += residual;
            entry.1 += 1;
            if residual != 0 || expected_cycles != actual_cycles {
                entry.2 += 1;
            }
            entry.3 = entry.3.max(residual.unsigned_abs());
        }
    }

    let mut rows: Vec<(BucketKey, (i64, u64, u64, u64))> = buckets.into_iter().collect();
    rows.sort_by_key(|(_, (sum_residual, count, _, _))| {
        std::cmp::Reverse(sum_residual.unsigned_abs().saturating_mul(*count))
    });

    println!("tests visited: {total_tests} (trace collection failures: {failed_traces})");
    println!(
        "queue_len  prefixes  length  ea_class   sum_residual    count  failures  mean_residual  worst_abs"
    );
    for ((queue_len, prefix_count, length, ea_class), (sum_residual, count, failures, worst_abs)) in
        rows
    {
        let mean = sum_residual as f64 / count as f64;
        let ea_label = residual_ea_label(ea_class);
        println!(
            "{queue_len:>9}  {prefix_count:>8}  {length:>6}  {ea_label}  {sum_residual:>12}  {count:>6}  {failures:>8}  {mean:>12.3}  {worst_abs:>8}"
        );
    }
}

#[test]
#[ignore]
fn residual_report_for_stem() {
    type BucketKey = (i64, usize, usize, usize, V20ResidualEaClass);

    let stem = std::env::var("V20_TIMING_DEBUG_STEM")
        .unwrap_or_else(|_| panic!("missing V20_TIMING_DEBUG_STEM"));
    let Some((status, _flags_mask)) = status_and_mask(&stem) else {
        return;
    };
    if !should_test(status) {
        return;
    }

    let revoked = timing_revocation_list(&stem, &[]);
    let path = test_dir().join(format!("{stem}.MOO.gz"));
    let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);
    let mut buckets: HashMap<BucketKey, (u64, u64, u32)> = HashMap::new();
    let mut total_tests = 0u64;
    let mut failed_traces = 0u64;

    for test in &test_cases {
        if skipped_timing_test(test, &revoked) {
            continue;
        }

        total_tests += 1;
        let (trace, _raw_total_cycles) = match collect_timing_trace(test) {
            Ok(result) => result,
            Err(_) => {
                failed_traces += 1;
                continue;
            }
        };

        let expected_cycles = normalize_expected_cycles(&test.cycles);
        let actual_cycles = normalize_actual_cycles(&trace);
        let residual = actual_cycles.len() as i64 - expected_cycles.len() as i64;
        let failed = residual != 0 || expected_cycles != actual_cycles;
        let key = (
            residual,
            test.initial.queue.len(),
            count_prefixes(&test.bytes),
            test.bytes.len(),
            residual_ea_class(&test.bytes),
        );
        let entry = buckets.entry(key).or_insert((0, 0, test.idx));
        entry.0 += 1;
        if failed {
            entry.1 += 1;
        }
    }

    let mut rows: Vec<(BucketKey, (u64, u64, u32))> = buckets.into_iter().collect();
    rows.sort_by_key(
        |((residual, queue_len, prefix_count, length, ea_class), _)| {
            (
                *residual,
                *queue_len,
                *prefix_count,
                *length,
                residual_ea_label(*ea_class),
            )
        },
    );

    println!("stem: {stem}");
    println!("tests visited: {total_tests} (trace collection failures: {failed_traces})");
    println!("residual  queue_len  prefixes  length  ea_class      count  failures  sample");
    for ((residual, queue_len, prefix_count, length, ea_class), (count, failures, sample_index)) in
        rows
    {
        let ea_label = residual_ea_label(ea_class);
        println!(
            "{residual:>8}  {queue_len:>9}  {prefix_count:>8}  {length:>6}  {ea_label}  {count:>9}  {failures:>8}  {sample_index:>6}"
        );
    }
}

fn print_cycle_window(label: &str, cycles: &[NormalizedV20Cycle], center: usize, window: usize) {
    let start = center.saturating_sub(window);
    let end = (center + window + 1).min(cycles.len());
    println!("{label}:");
    for (index, cycle) in cycles[start..end].iter().enumerate() {
        let absolute_index = start + index;
        println!("  [{absolute_index:>3}] {}", describe_cycle(Some(cycle)));
    }
}

fn print_trace_window(label: &str, trace: &[V30CycleTraceEntry], center: usize, window: usize) {
    let start = center.saturating_sub(window);
    let end = (center + window + 1).min(trace.len());
    println!("{label}:");
    for (index, entry) in trace[start..end].iter().enumerate() {
        let absolute_index = start + index;
        let address = if entry.ale {
            format!("0x{:05X}", entry.address & ADDRESS_MASK)
        } else {
            "-".to_string()
        };
        println!(
            "  [{absolute_index:>3}] cycle={} phase={:?} status={:?} latch={:?} t={:?} ta={:?} fetch={:?}/{} queue_len={} queue={:?}/0x{:02X} address={} data=0x{:02X}",
            entry.cycle,
            entry.phase,
            entry.status,
            entry.bus_status_latch,
            entry.t_cycle,
            entry.ta_cycle,
            entry.fetch_state,
            entry.fetch_delay,
            entry.queue_len,
            entry.queue_op,
            entry.queue_byte,
            address,
            entry.data,
        );
    }
}

#[test]
#[ignore]
fn debug_v20_timing_case() {
    let stem = std::env::var("V20_TIMING_DEBUG_STEM")
        .unwrap_or_else(|_| panic!("missing V20_TIMING_DEBUG_STEM"));
    let test_index = std::env::var("V20_TIMING_DEBUG_INDEX")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("missing or invalid V20_TIMING_DEBUG_INDEX"));
    let window = std::env::var("V20_TIMING_DEBUG_WINDOW")
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
    let actual_start_index = actual_trace_start_index(&trace);
    let actual_cycles = normalize_actual_cycles_from(&trace, actual_start_index);
    let mismatch_index = find_cycle_mismatch_index(&expected_cycles, &actual_cycles)
        .unwrap_or(expected_cycles.len());

    println!("file: {filename}");
    println!("idx: {}", test.idx);
    println!("name: {}", test.name);
    println!(
        "bytes: {}",
        test.bytes
            .iter()
            .map(|value| format!("{value:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!(
        "initial_regs: ax={:04X} bx={:04X} cx={:04X} dx={:04X} sp={:04X} bp={:04X} si={:04X} di={:04X}",
        test.initial.regs.get("ax").copied().unwrap_or_default(),
        test.initial.regs.get("bx").copied().unwrap_or_default(),
        test.initial.regs.get("cx").copied().unwrap_or_default(),
        test.initial.regs.get("dx").copied().unwrap_or_default(),
        test.initial.regs.get("sp").copied().unwrap_or_default(),
        test.initial.regs.get("bp").copied().unwrap_or_default(),
        test.initial.regs.get("si").copied().unwrap_or_default(),
        test.initial.regs.get("di").copied().unwrap_or_default(),
    );
    println!("expected_total_cycles: {}", expected_cycles.len());
    println!("actual_total_cycles: {}", actual_cycles.len());
    if actual_cycles.len() as u64 != total_cycles {
        println!("raw_actual_total_cycles: {total_cycles}");
    }
    println!("actual_trace_start_index: {actual_start_index}");
    println!("mismatch_index: {mismatch_index}");
    print_cycle_window("expected", &expected_cycles, mismatch_index, window);
    print_cycle_window("actual", &actual_cycles, mismatch_index, window);
    let trace_center = actual_start_index
        .saturating_add(mismatch_index)
        .min(trace.len().saturating_sub(1));
    print_trace_window("actual_trace", &trace, trace_center, window);
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

        let (trace, _raw_total_cycles) = match collect_timing_trace(test) {
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
        let actual_cycles = normalize_actual_cycles(&trace);
        let expected_total = expected_cycles.len() as u64;
        let actual_total = actual_cycles.len() as u64;

        if expected_total != actual_total || expected_cycles != actual_cycles {
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
                expected_total,
                actual_total
            );
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
            "{filename}: {}/{} compared tests failed ({} total, {} revoked, {} exception)\n",
            failures.len(),
            compared_tests,
            test_cases.len(),
            skipped_revoked,
            skipped_exception,
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
test_opcode!(op_0f10, "0F10");
test_opcode!(op_0f11, "0F11");
test_opcode!(op_0f12, "0F12");
test_opcode!(op_0f13, "0F13");
test_opcode!(op_0f14, "0F14");
test_opcode!(op_0f15, "0F15");
test_opcode!(op_0f16, "0F16");
test_opcode!(op_0f17, "0F17");
test_opcode!(op_0f18, "0F18");
test_opcode!(op_0f19, "0F19");
test_opcode!(op_0f1a, "0F1A");
test_opcode!(op_0f1b, "0F1B");
test_opcode!(op_0f1c, "0F1C");
test_opcode!(op_0f1d, "0F1D");
test_opcode!(op_0f1e, "0F1E");
test_opcode!(op_0f1f, "0F1F");
test_opcode!(op_0f20, "0F20");
test_opcode!(op_0f22, "0F22");
test_opcode!(op_0f26, "0F26");
test_opcode!(op_0f28, "0F28");
test_opcode!(op_0f2a, "0F2A");
test_opcode!(op_0f31, "0F31");
test_opcode!(op_0f33, "0F33");
test_opcode!(op_0f39, "0F39");
test_opcode!(op_0f3b, "0F3B");
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
test_opcode!(op_63, "63");
test_opcode!(op_66, "66");
test_opcode!(op_67, "67");
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
test_opcode!(op_d9, "D9");
test_opcode!(op_da, "DA");
test_opcode!(op_db, "DB");
test_opcode!(op_dc, "DC");
test_opcode!(op_dd, "DD");
test_opcode!(op_de, "DE");
test_opcode!(op_df, "DF");
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
test_opcode!(op_f5, "F5");
test_opcode!(op_f6_0, "F6.0");
test_opcode!(op_f6_1, "F6.1");
test_opcode!(op_f6_2, "F6.2");
test_opcode!(op_f6_3, "F6.3");
test_opcode!(op_f6_4, "F6.4");
test_opcode!(op_f6_5, "F6.5");
test_opcode!(op_f6_6, "F6.6");
test_opcode!(op_f6_7, "F6.7");
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
test_opcode!(op_ff_7, "FF.7");
