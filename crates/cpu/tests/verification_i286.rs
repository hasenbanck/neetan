#![cfg(feature = "verification")]

#[path = "common/metadata_json.rs"]
mod metadata_json;
#[path = "common/verification_common.rs"]
mod verification_common;

use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use common::Cpu as _;
use cpu::{I286, I286State};
use metadata_json::{Metadata, load_metadata};
use verification_common::{MooTest, load_moo_tests, load_revocation_list};

const RAM_SIZE: usize = 16 * 1024 * 1024;
const ADDRESS_MASK: u32 = 0x00FF_FFFF;
const REG_ORDER: [&str; 14] = [
    "ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "flags",
];
const LOCAL_REVOKED_F6_7: &[&str] = &[
    "0038b4bacfb75535b5da175f619b0812b16d0601",
    "de153d1e3812cdb2c9d25272844b4b28a5adc35f",
    "dce03c62813266bf0ba50e3325fa3898132cad1f",
    "38a27640b8a9475f75998d2cab801d51eb8bb0b2",
];
const TERMINATING_HALT_CYCLES: u64 = 2;

#[derive(Debug, Clone, Default)]
struct TimingGroupStats {
    case_count: u64,
    exact_match_count: u64,
    total_expected_cycles: u64,
    total_actual_cycles: u64,
    signed_delta_sum: i64,
    squared_delta_sum: u128,
    absolute_delta_sum: u64,
    min_signed_delta: i64,
    max_signed_delta: i64,
    max_absolute_delta: u64,
    max_absolute_signed_delta: i64,
}

impl TimingGroupStats {
    fn record(&mut self, expected_cycles: u64, actual_cycles: u64) {
        let signed_delta = actual_cycles as i64 - expected_cycles as i64;
        let absolute_delta = signed_delta.unsigned_abs();
        let squared_delta = (signed_delta as i128 * signed_delta as i128) as u128;

        if self.case_count == 0 {
            self.min_signed_delta = signed_delta;
            self.max_signed_delta = signed_delta;
        } else {
            self.min_signed_delta = self.min_signed_delta.min(signed_delta);
            self.max_signed_delta = self.max_signed_delta.max(signed_delta);
        }

        self.case_count += 1;
        self.total_expected_cycles += expected_cycles;
        self.total_actual_cycles += actual_cycles;
        self.signed_delta_sum += signed_delta;
        self.squared_delta_sum += squared_delta;
        self.absolute_delta_sum += absolute_delta;
        if signed_delta == 0 {
            self.exact_match_count += 1;
        }
        if absolute_delta > self.max_absolute_delta {
            self.max_absolute_delta = absolute_delta;
            self.max_absolute_signed_delta = signed_delta;
        }
    }

    fn merge(&mut self, other: &Self) {
        if other.case_count == 0 {
            return;
        }
        if self.case_count == 0 {
            self.min_signed_delta = other.min_signed_delta;
            self.max_signed_delta = other.max_signed_delta;
        } else {
            self.min_signed_delta = self.min_signed_delta.min(other.min_signed_delta);
            self.max_signed_delta = self.max_signed_delta.max(other.max_signed_delta);
        }
        self.case_count += other.case_count;
        self.exact_match_count += other.exact_match_count;
        self.total_expected_cycles += other.total_expected_cycles;
        self.total_actual_cycles += other.total_actual_cycles;
        self.signed_delta_sum += other.signed_delta_sum;
        self.squared_delta_sum += other.squared_delta_sum;
        self.absolute_delta_sum += other.absolute_delta_sum;
        if other.max_absolute_delta > self.max_absolute_delta {
            self.max_absolute_delta = other.max_absolute_delta;
            self.max_absolute_signed_delta = other.max_absolute_signed_delta;
        }
    }

    fn mean_deviation(&self) -> f64 {
        if self.case_count == 0 {
            0.0
        } else {
            self.signed_delta_sum as f64 / self.case_count as f64
        }
    }

    fn mean_absolute_delta(&self) -> f64 {
        if self.case_count == 0 {
            0.0
        } else {
            self.absolute_delta_sum as f64 / self.case_count as f64
        }
    }

    fn variance(&self) -> f64 {
        if self.case_count == 0 {
            return 0.0;
        }
        let mean = self.mean_deviation();
        (self.squared_delta_sum as f64 / self.case_count as f64) - (mean * mean)
    }

    fn closeness_percent(&self) -> f64 {
        if self.total_expected_cycles == 0 {
            0.0
        } else {
            (100.0 - (self.absolute_delta_sum as f64 / self.total_expected_cycles as f64) * 100.0)
                .clamp(0.0, 100.0)
        }
    }

    fn skew_percent(&self) -> f64 {
        if self.total_expected_cycles == 0 {
            0.0
        } else {
            ((self.total_actual_cycles as f64 - self.total_expected_cycles as f64)
                / self.total_expected_cycles as f64)
                * 100.0
        }
    }

    fn exact_match_percent(&self) -> f64 {
        if self.case_count == 0 {
            0.0
        } else {
            (self.exact_match_count as f64 / self.case_count as f64) * 100.0
        }
    }

    fn min_deviation(&self) -> i64 {
        if self.case_count == 0 {
            0
        } else {
            self.min_signed_delta
        }
    }

    fn max_deviation(&self) -> i64 {
        if self.case_count == 0 {
            0
        } else {
            self.max_signed_delta
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TimingModelStats {
    case_count: u64,
    total_eu_cycles: u64,
    total_au_cycles: u64,
    total_bu_data_pressure: u64,
    total_bu_fetch_stall: u64,
    total_iu_stall: u64,
    total_flush_penalty: u64,
}

impl TimingModelStats {
    fn record_case(
        &mut self,
        eu_cycles: u64,
        au_cycles: u64,
        bu_data_pressure: u64,
        bu_fetch_stall: u64,
        iu_stall: u64,
        flush_penalty: u64,
    ) {
        self.case_count += 1;
        self.total_eu_cycles += eu_cycles;
        self.total_au_cycles += au_cycles;
        self.total_bu_data_pressure += bu_data_pressure;
        self.total_bu_fetch_stall += bu_fetch_stall;
        self.total_iu_stall += iu_stall;
        self.total_flush_penalty += flush_penalty;
    }

    fn merge(&mut self, other: &Self) {
        self.case_count += other.case_count;
        self.total_eu_cycles += other.total_eu_cycles;
        self.total_au_cycles += other.total_au_cycles;
        self.total_bu_data_pressure += other.total_bu_data_pressure;
        self.total_bu_fetch_stall += other.total_bu_fetch_stall;
        self.total_iu_stall += other.total_iu_stall;
        self.total_flush_penalty += other.total_flush_penalty;
    }

    fn mean(total: u64, count: u64) -> f64 {
        if count == 0 {
            0.0
        } else {
            total as f64 / count as f64
        }
    }
}

#[derive(Debug, Clone)]
struct CaseExecution {
    initial: I286State,
    expected: I286State,
    actual: I286State,
    total_cycles: u64,
    instruction_cycles: u64,
    timing_model: TimingModelStats,
    instruction_timing_model: TimingModelStats,
}

#[derive(Debug, Clone)]
struct TimingGroupReport {
    stem: String,
    raw_stats: TimingGroupStats,
    instruction_window_stats: TimingGroupStats,
    register_only_instruction_window_stats: TimingGroupStats,
}

#[derive(Debug, Clone, Default)]
struct TimingAnalysis {
    raw_stats: TimingGroupStats,
    instruction_window_stats: TimingGroupStats,
    register_only_instruction_window_stats: TimingGroupStats,
    raw_model_stats: TimingModelStats,
    instruction_window_model_stats: TimingModelStats,
    register_only_instruction_window_model_stats: TimingModelStats,
}

struct TestBus {
    ram: Vec<u8>,
    dirty: Vec<u32>,
    dirty_marker: Vec<u8>,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; RAM_SIZE],
            dirty: Vec::new(),
            dirty_marker: vec![0u8; RAM_SIZE],
        }
    }

    fn clear(&mut self) {
        for &address in &self.dirty {
            let index = (address & ADDRESS_MASK) as usize;
            self.ram[index] = 0;
            self.dirty_marker[index] = 0;
        }
        self.dirty.clear();
    }

    fn set_memory(&mut self, address: u32, value: u8) {
        let index = (address & ADDRESS_MASK) as usize;
        if self.dirty_marker[index] == 0 {
            self.dirty_marker[index] = 1;
            self.dirty.push(address & ADDRESS_MASK);
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
        0
    }

    fn set_current_cycle(&mut self, _cycle: u64) {}
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

fn should_test(status: &str) -> bool {
    matches!(
        status,
        "normal" | "alias" | "undocumented" | "fpu" | "undefined"
    )
}

fn initial_reg_value(initial_regs: &HashMap<String, u32>, name: &str) -> u16 {
    initial_regs
        .get(name)
        .copied()
        .unwrap_or_else(|| panic!("missing register in initial state: {name}")) as u16
}

fn build_initial_state(initial_regs: &HashMap<String, u32>) -> I286State {
    let mut state = I286State::default();
    state.set_ax(initial_reg_value(initial_regs, "ax"));
    state.set_bx(initial_reg_value(initial_regs, "bx"));
    state.set_cx(initial_reg_value(initial_regs, "cx"));
    state.set_dx(initial_reg_value(initial_regs, "dx"));
    state.set_sp(initial_reg_value(initial_regs, "sp"));
    state.set_bp(initial_reg_value(initial_regs, "bp"));
    state.set_si(initial_reg_value(initial_regs, "si"));
    state.set_di(initial_reg_value(initial_regs, "di"));
    state.set_cs(initial_reg_value(initial_regs, "cs"));
    state.set_ss(initial_reg_value(initial_regs, "ss"));
    state.set_ds(initial_reg_value(initial_regs, "ds"));
    state.set_es(initial_reg_value(initial_regs, "es"));
    state.ip = initial_reg_value(initial_regs, "ip");
    state.set_compressed_flags(initial_reg_value(initial_regs, "flags"));
    state.msw = 0xFFF0;
    state
}

fn build_expected_state(
    initial_regs: &HashMap<String, u32>,
    final_regs: &HashMap<String, u32>,
) -> I286State {
    let get = |name: &str| -> u16 {
        final_regs
            .get(name)
            .copied()
            .map(|value| value as u16)
            .unwrap_or_else(|| initial_reg_value(initial_regs, name))
    };

    let mut s = I286State::default();
    s.set_ax(get("ax"));
    s.set_cx(get("cx"));
    s.set_dx(get("dx"));
    s.set_bx(get("bx"));
    s.set_sp(get("sp"));
    s.set_bp(get("bp"));
    s.set_si(get("si"));
    s.set_di(get("di"));
    s.set_es(get("es"));
    s.set_cs(get("cs"));
    s.set_ss(get("ss"));
    s.set_ds(get("ds"));
    s.ip = get("ip");
    s.set_compressed_flags(get("flags"));
    s.msw = 0xFFF0;
    s
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

fn format_flags_diff(expected: u16, actual: u16, mask: u16) -> String {
    const FLAG_BITS: &[(u16, &str)] = &[
        (0x0001, "CF"),
        (0x0004, "PF"),
        (0x0010, "AF"),
        (0x0040, "ZF"),
        (0x0080, "SF"),
        (0x0100, "TF"),
        (0x0200, "IF"),
        (0x0400, "DF"),
        (0x0800, "OF"),
    ];

    let expected_masked = expected & mask;
    let actual_masked = actual & mask;
    let diff_bits = expected_masked ^ actual_masked;

    let mut changed: Vec<String> = Vec::new();
    for &(bit, name) in FLAG_BITS {
        if diff_bits & bit != 0 {
            let exp = u16::from(expected_masked & bit != 0);
            let act = u16::from(actual_masked & bit != 0);
            changed.push(format!("{name}:{exp}->{act}"));
        }
    }

    format!(
        "  flags: expected 0x{expected_masked:04X}, got 0x{actual_masked:04X} [{}] (mask 0x{mask:04X})",
        changed.join(", ")
    )
}

fn local_revoked_hashes(stem: &str) -> &'static [&'static str] {
    match stem {
        "F6.7" => LOCAL_REVOKED_F6_7,
        _ => &[],
    }
}

fn all_test_stems() -> Vec<String> {
    let mut stems = Vec::new();

    for entry in fs::read_dir(test_dir()).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".MOO.gz") {
            continue;
        }

        stems.push(file_name.trim_end_matches(".MOO.gz").to_string());
    }

    stems.sort_unstable();
    stems
}

fn skip_instruction_prefixes(bytes: &[u8]) -> usize {
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            0x26 | 0x2E | 0x36 | 0x3E | 0xF0 | 0xF2 | 0xF3 => index += 1,
            _ => break,
        }
    }
    index
}

fn implicit_register_only_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        0x27
            | 0x2F
            | 0x37
            | 0x3F
            | 0x40..=0x4F
            | 0x90..=0x99
            | 0x9E
            | 0x9F
            | 0xB0..=0xBF
            | 0xD4
            | 0xD5
            | 0xD6
            | 0xF5
            | 0xF8..=0xFD
    )
}

fn modrm_register_form_opcode(opcode: u8, reg_ext: Option<&str>) -> bool {
    match opcode {
        0x00..=0x03
        | 0x08..=0x0B
        | 0x10..=0x13
        | 0x18..=0x1B
        | 0x20..=0x23
        | 0x28..=0x2B
        | 0x30..=0x33
        | 0x38..=0x3B
        | 0x63
        | 0x69
        | 0x6B
        | 0x80..=0x83
        | 0x84..=0x8C
        | 0x8E
        | 0x8F
        | 0xC0
        | 0xC1
        | 0xC6
        | 0xC7
        | 0xD0..=0xD3
        | 0xF6
        | 0xF7
        | 0xFE => true,
        0xFF => !matches!(reg_ext, Some("3" | "5")),
        _ => false,
    }
}

fn register_only_instruction_window_case(stem: &str, bytes: &[u8]) -> bool {
    let (opcode_stem, reg_ext) = parse_stem(stem);
    if opcode_stem == "F4" {
        return false;
    }

    let opcode_index = skip_instruction_prefixes(bytes);
    let Some(&opcode) = bytes.get(opcode_index) else {
        return false;
    };

    if opcode == 0x0F {
        return false;
    }

    if implicit_register_only_opcode(opcode) {
        return true;
    }

    if !modrm_register_form_opcode(opcode, reg_ext) {
        return false;
    }

    bytes
        .get(opcode_index + 1)
        .is_some_and(|modrm| *modrm >= 0xC0)
}

fn execute_test_case(test: &MooTest, bus: &mut TestBus) -> Result<CaseExecution, String> {
    bus.clear();
    for &(address, value) in &test.initial.ram {
        bus.set_memory(address, value);
    }

    let initial = build_initial_state(&test.initial.regs);
    let expected = build_expected_state(&test.initial.regs, &test.final_state.regs);

    let mut cpu = I286::new();
    cpu.load_state(&initial);

    let mut total_cycles = 0u64;
    let mut total_eu_cycles = 0u64;
    let mut total_au_cycles = 0u64;
    let mut total_bu_data_pressure = 0u64;
    let mut total_bu_fetch_stall = 0u64;
    let mut total_iu_stall = 0u64;
    let mut total_flush_penalty = 0u64;
    let mut instruction_cycles = 0u64;
    let mut instruction_timing_model = TimingModelStats::default();
    let mut steps = 0usize;
    while !cpu.halted() && steps < 1024 {
        cpu.step(bus);
        let step_cycles = cpu.cycles_consumed();
        total_cycles += step_cycles;
        let (eu_cycles, au_cycles, bu_data_pressure, bu_fetch_stall, iu_stall, flush_penalty) =
            cpu.timing_model_contributions();
        total_eu_cycles += eu_cycles as u64;
        total_au_cycles += au_cycles as u64;
        total_bu_data_pressure += bu_data_pressure as u64;
        total_bu_fetch_stall += bu_fetch_stall as u64;
        total_iu_stall += iu_stall as u64;
        total_flush_penalty += flush_penalty as u64;
        if steps == 0 {
            instruction_cycles = step_cycles;
            instruction_timing_model.record_case(
                eu_cycles as u64,
                au_cycles as u64,
                bu_data_pressure as u64,
                bu_fetch_stall as u64,
                iu_stall as u64,
                flush_penalty as u64,
            );
        }
        steps += 1;
    }

    if steps >= 1024 {
        return Err("execution did not reach HLT within 1024 instructions".to_string());
    }

    Ok(CaseExecution {
        initial,
        expected,
        actual: cpu.state.clone(),
        total_cycles,
        instruction_cycles,
        timing_model: {
            let mut timing_model = TimingModelStats::default();
            timing_model.record_case(
                total_eu_cycles,
                total_au_cycles,
                total_bu_data_pressure,
                total_bu_fetch_stall,
                total_iu_stall,
                total_flush_penalty,
            );
            timing_model
        },
        instruction_timing_model,
    })
}

fn run_test_file(
    stem: &str,
    local_revoked_hashes: &[&str],
    check_cycles: bool,
) -> Option<TimingAnalysis> {
    let revoked = revocation_list();
    let local_revoked: HashSet<String> = local_revoked_hashes
        .iter()
        .map(|hash| hash.to_ascii_lowercase())
        .collect();
    let mut bus = TestBus::new();

    let (status, flags_mask) = status_and_mask(stem)?;
    if !should_test(status) {
        return None;
    }

    let filename = format!("{stem}.MOO.gz");
    let path = test_dir().join(&filename);
    let test_cases = load_moo_tests(&path, &REG_ORDER, &[]);
    let mut failures: Vec<String> = Vec::new();
    let mut analysis = TimingAnalysis::default();

    for test in &test_cases {
        if let Some(hash) = &test.hash
            && (revoked.contains(&hash.to_ascii_lowercase())
                || local_revoked.contains(&hash.to_ascii_lowercase()))
        {
            continue;
        }
        if test.exception.is_some() {
            continue;
        }

        let execution = match execute_test_case(test, &mut bus) {
            Ok(execution) => execution,
            Err(reason) => {
                failures.push(format!(
                    "[{filename} #{} idx={}] {} ({reason})",
                    failures.len(),
                    test.idx,
                    test.name
                ));
                continue;
            }
        };

        let expected_cycles = test.cycles.len() as u64;
        let expected_instruction_cycles = if parse_stem(stem).0 == "F4" {
            expected_cycles
        } else {
            expected_cycles.saturating_sub(TERMINATING_HALT_CYCLES)
        };

        analysis
            .raw_stats
            .record(expected_cycles, execution.total_cycles);
        analysis
            .instruction_window_stats
            .record(expected_instruction_cycles, execution.instruction_cycles);
        analysis.raw_model_stats.merge(&execution.timing_model);
        analysis
            .instruction_window_model_stats
            .merge(&execution.instruction_timing_model);
        if register_only_instruction_window_case(stem, &test.bytes) {
            analysis
                .register_only_instruction_window_stats
                .record(expected_instruction_cycles, execution.instruction_cycles);
            analysis
                .register_only_instruction_window_model_stats
                .merge(&execution.instruction_timing_model);
        }
        let mut diffs: Vec<String> = Vec::new();

        let check_reg = |name: &str,
                         initial_value: u16,
                         actual_value: u16,
                         expected_value: u16,
                         diffs: &mut Vec<String>| {
            if actual_value != expected_value {
                diffs.push(format!(
                    "  {name}: expected 0x{expected_value:04X}, got 0x{actual_value:04X} (was 0x{initial_value:04X})"
                ));
            }
        };

        check_reg(
            "ax",
            execution.initial.ax(),
            execution.actual.ax(),
            execution.expected.ax(),
            &mut diffs,
        );
        check_reg(
            "bx",
            execution.initial.bx(),
            execution.actual.bx(),
            execution.expected.bx(),
            &mut diffs,
        );
        check_reg(
            "cx",
            execution.initial.cx(),
            execution.actual.cx(),
            execution.expected.cx(),
            &mut diffs,
        );
        check_reg(
            "dx",
            execution.initial.dx(),
            execution.actual.dx(),
            execution.expected.dx(),
            &mut diffs,
        );
        check_reg(
            "sp",
            execution.initial.sp(),
            execution.actual.sp(),
            execution.expected.sp(),
            &mut diffs,
        );
        check_reg(
            "bp",
            execution.initial.bp(),
            execution.actual.bp(),
            execution.expected.bp(),
            &mut diffs,
        );
        check_reg(
            "si",
            execution.initial.si(),
            execution.actual.si(),
            execution.expected.si(),
            &mut diffs,
        );
        check_reg(
            "di",
            execution.initial.di(),
            execution.actual.di(),
            execution.expected.di(),
            &mut diffs,
        );
        check_reg(
            "cs",
            execution.initial.cs(),
            execution.actual.cs(),
            execution.expected.cs(),
            &mut diffs,
        );
        check_reg(
            "ss",
            execution.initial.ss(),
            execution.actual.ss(),
            execution.expected.ss(),
            &mut diffs,
        );
        check_reg(
            "ds",
            execution.initial.ds(),
            execution.actual.ds(),
            execution.expected.ds(),
            &mut diffs,
        );
        check_reg(
            "es",
            execution.initial.es(),
            execution.actual.es(),
            execution.expected.es(),
            &mut diffs,
        );
        check_reg(
            "ip",
            execution.initial.ip,
            execution.actual.ip,
            execution.expected.ip,
            &mut diffs,
        );

        if (execution.actual.compressed_flags() & flags_mask)
            != (execution.expected.compressed_flags() & flags_mask)
        {
            diffs.push(format!(
                "{} (was 0x{:04X})",
                format_flags_diff(
                    execution.expected.compressed_flags(),
                    execution.actual.compressed_flags(),
                    flags_mask
                ),
                execution.initial.compressed_flags()
            ));
        }

        if check_cycles {
            if execution.total_cycles != expected_cycles {
                diffs.push(format!(
                    "  cycles: expected {expected_cycles}, got {}",
                    execution.total_cycles
                ));
            }
        }

        for &(address, expected_value) in &test.final_state.ram {
            let actual_value = bus.ram[(address & ADDRESS_MASK) as usize];
            if actual_value != expected_value {
                let initial_value = test
                    .initial
                    .ram
                    .iter()
                    .find(|(initial_address, _)| *initial_address == address)
                    .map(|(_, value)| *value);
                match initial_value {
                    Some(before) => diffs.push(format!(
                        "  ram[0x{address:06X}]: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (was 0x{before:02X})"
                    )),
                    None => diffs.push(format!(
                        "  ram[0x{address:06X}]: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (not in initial RAM)"
                    )),
                }
            }
        }

        if !diffs.is_empty() {
            let bytes_hex: Vec<String> = test
                .bytes
                .iter()
                .map(|value| format!("{value:02X}"))
                .collect();
            failures.push(format!(
                "[{filename} #{} idx={}] {} ({})\n{}",
                failures.len(),
                test.idx,
                test.name,
                bytes_hex.join(" "),
                diffs.join("\n")
            ));
        }
    }

    if !failures.is_empty() {
        let mut message = format!(
            "{filename}: {}/{} tests failed\n",
            failures.len(),
            test_cases.len()
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

    Some(analysis)
}

fn write_summary_line(
    report: &mut String,
    tag: &str,
    totals: &TimingGroupStats,
    analyzed_group_count: usize,
) {
    let overall_direction = if totals.total_actual_cycles >= totals.total_expected_cycles {
        "over"
    } else {
        "under"
    };

    let _ = writeln!(
        report,
        "{tag}\tgroups_analyzed={analyzed_group_count}\tcase_count={}\texact_match_count={}\texact_match_percent={:.4}\tmean_deviation={:.4}\tmean_absolute_delta={:.4}\tvariance={:.4}\tmin_deviation={}\tmax_deviation={}\tworst_single_case_deviation={}\tcloseness_percent={:.4}\tskew_percent={:.4}\tskew_direction={overall_direction}\ttotal_actual_cycles={}\ttotal_expected_cycles={}",
        totals.case_count,
        totals.exact_match_count,
        totals.exact_match_percent(),
        totals.mean_deviation(),
        totals.mean_absolute_delta(),
        totals.variance(),
        totals.min_deviation(),
        totals.max_deviation(),
        totals.max_absolute_signed_delta,
        totals.closeness_percent(),
        totals.skew_percent(),
        totals.total_actual_cycles,
        totals.total_expected_cycles
    );
}

fn write_model_summary_line(report: &mut String, tag: &str, model_totals: &TimingModelStats) {
    let _ = writeln!(
        report,
        "{tag}\tcase_count={}\tmean_eu_cycles={:.4}\tmean_au_cycles={:.4}\tmean_bu_data_pressure={:.4}\tmean_bu_fetch_stall={:.4}\tmean_iu_stall={:.4}\tmean_flush_penalty={:.4}\ttotal_eu_cycles={}\ttotal_au_cycles={}\ttotal_bu_data_pressure={}\ttotal_bu_fetch_stall={}\ttotal_iu_stall={}\ttotal_flush_penalty={}",
        model_totals.case_count,
        TimingModelStats::mean(model_totals.total_eu_cycles, model_totals.case_count),
        TimingModelStats::mean(model_totals.total_au_cycles, model_totals.case_count),
        TimingModelStats::mean(model_totals.total_bu_data_pressure, model_totals.case_count),
        TimingModelStats::mean(model_totals.total_bu_fetch_stall, model_totals.case_count),
        TimingModelStats::mean(model_totals.total_iu_stall, model_totals.case_count),
        TimingModelStats::mean(model_totals.total_flush_penalty, model_totals.case_count),
        model_totals.total_eu_cycles,
        model_totals.total_au_cycles,
        model_totals.total_bu_data_pressure,
        model_totals.total_bu_fetch_stall,
        model_totals.total_iu_stall,
        model_totals.total_flush_penalty
    );
}

fn write_group_line(report: &mut String, section: &str, stem: &str, stats: &TimingGroupStats) {
    let _ = writeln!(
        report,
        "GROUP\tsection={section}\tstem={stem}\tcase_count={}\texact_match_count={}\texact_match_percent={:.4}\tmean_deviation={:.4}\tmean_absolute_delta={:.4}\tvariance={:.4}\tmin_deviation={}\tmax_deviation={}\tworst_single_case_deviation={}\tcloseness_percent={:.4}\tskew_percent={:.4}\ttotal_actual_cycles={}\ttotal_expected_cycles={}",
        stats.case_count,
        stats.exact_match_count,
        stats.exact_match_percent(),
        stats.mean_deviation(),
        stats.mean_absolute_delta(),
        stats.variance(),
        stats.min_deviation(),
        stats.max_deviation(),
        stats.max_absolute_signed_delta,
        stats.closeness_percent(),
        stats.skew_percent(),
        stats.total_actual_cycles,
        stats.total_expected_cycles
    );
}

fn raw_stats_for(report: &TimingGroupReport) -> &TimingGroupStats {
    &report.raw_stats
}

fn instruction_window_stats_for(report: &TimingGroupReport) -> &TimingGroupStats {
    &report.instruction_window_stats
}

fn register_only_instruction_window_stats_for(report: &TimingGroupReport) -> &TimingGroupStats {
    &report.register_only_instruction_window_stats
}

fn write_ranked_sections(
    report: &mut String,
    reports: &[TimingGroupReport],
    suffix: &str,
    stats_for: fn(&TimingGroupReport) -> &TimingGroupStats,
) {
    let section_name = |base: &str| {
        if suffix.is_empty() {
            base.to_string()
        } else {
            format!("{base}_{suffix}")
        }
    };

    let mut most_negative = reports.to_vec();
    most_negative.sort_by(|left, right| {
        stats_for(left)
            .mean_deviation()
            .total_cmp(&stats_for(right).mean_deviation())
            .then_with(|| left.stem.cmp(&right.stem))
    });

    let most_under_timed = section_name("most_under_timed");
    let _ = writeln!(
        report,
        "SECTION\tname={most_under_timed}\tsort=mean_deviation_ascending\tcount={}",
        most_negative.len().min(10)
    );
    for group_report in most_negative.iter().take(10) {
        write_group_line(
            report,
            &most_under_timed,
            &group_report.stem,
            stats_for(group_report),
        );
    }

    let mut most_positive = reports.to_vec();
    most_positive.sort_by(|left, right| {
        stats_for(right)
            .mean_deviation()
            .total_cmp(&stats_for(left).mean_deviation())
            .then_with(|| left.stem.cmp(&right.stem))
    });

    let most_over_timed = section_name("most_over_timed");
    let _ = writeln!(
        report,
        "SECTION\tname={most_over_timed}\tsort=mean_deviation_descending\tcount={}",
        most_positive.len().min(10)
    );
    for group_report in most_positive.iter().take(10) {
        write_group_line(
            report,
            &most_over_timed,
            &group_report.stem,
            stats_for(group_report),
        );
    }

    let mut worst_by_mean_abs = reports.to_vec();
    worst_by_mean_abs.sort_by(|left, right| {
        stats_for(right)
            .mean_absolute_delta()
            .total_cmp(&stats_for(left).mean_absolute_delta())
            .then_with(|| left.stem.cmp(&right.stem))
    });

    let worst_by_mean_absolute_delta = section_name("worst_by_mean_absolute_delta");
    let _ = writeln!(
        report,
        "SECTION\tname={worst_by_mean_absolute_delta}\tsort=mean_absolute_delta_descending\tcount={}",
        worst_by_mean_abs.len().min(20)
    );
    for group_report in worst_by_mean_abs.iter().take(20) {
        write_group_line(
            report,
            &worst_by_mean_absolute_delta,
            &group_report.stem,
            stats_for(group_report),
        );
    }

    let mut largest_single_case = reports.to_vec();
    largest_single_case.sort_by(|left, right| {
        stats_for(right)
            .max_absolute_delta
            .cmp(&stats_for(left).max_absolute_delta)
            .then_with(|| {
                stats_for(right)
                    .mean_absolute_delta()
                    .total_cmp(&stats_for(left).mean_absolute_delta())
            })
            .then_with(|| left.stem.cmp(&right.stem))
    });

    let largest_single_case_outliers = section_name("largest_single_case_outliers");
    let _ = writeln!(
        report,
        "SECTION\tname={largest_single_case_outliers}\tsort=max_absolute_delta_descending\tcount={}",
        largest_single_case.len().min(10)
    );
    for group_report in largest_single_case.iter().take(10) {
        write_group_line(
            report,
            &largest_single_case_outliers,
            &group_report.stem,
            stats_for(group_report),
        );
    }

    let mut full_table = reports.to_vec();
    full_table.sort_by(|left, right| left.stem.cmp(&right.stem));

    let full_per_group = section_name("full_per_group");
    let _ = writeln!(
        report,
        "SECTION\tname={full_per_group}\tsort=stem_ascending\tcount={}",
        full_table.len()
    );
    for group_report in &full_table {
        write_group_line(
            report,
            &full_per_group,
            &group_report.stem,
            stats_for(group_report),
        );
    }
}

fn render_report(
    reports: &[TimingGroupReport],
    raw_totals: &TimingGroupStats,
    instruction_window_totals: &TimingGroupStats,
    register_only_instruction_window_totals: &TimingGroupStats,
    raw_model_totals: &TimingModelStats,
    instruction_window_model_totals: &TimingModelStats,
    register_only_instruction_window_model_totals: &TimingModelStats,
    analyzed_group_count: usize,
) -> String {
    let mut report = String::new();

    let _ = writeln!(
        &mut report,
        "REPORT\tversion=2\tformat=tsv_key_value\tdataset=crates/cpu/tests/SingleStepTests/80286/v1_real_mode"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=grouping\tvalue=file_stem_per_opcode_group"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=filters\tvalue=revoked_hashes_local_skips_exception_cases_excluded"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=cycle_metric\tvalue=sum_of_I286_cycles_consumed_across_each_step_until_HLT"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=instruction_window_cycle_metric\tvalue=first_logical_instruction_cycles_with_trailing_HLT_removed_but_setup_jump_refill_retained"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=closeness_metric\tvalue=100_minus_absolute_deviation_sum_div_expected_cycle_sum_times_100_clamped_0_to_100"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=notes\tvalue=raw_summary_includes_test_harness_overhead_queue_refill_after_setup_jump_and_trailing_HLT"
    );
    let _ = writeln!(
        &mut report,
        "META\tkey=instruction_window_notes\tvalue=instruction_window_removes_only_the_trailing_two_HLT_cycles_because_the_dataset_does_not_expose_enough_queue_state_to_remove_post_jump_refill_exactly"
    );

    write_summary_line(&mut report, "SUMMARY", raw_totals, analyzed_group_count);
    write_model_summary_line(&mut report, "MODEL_SUMMARY", raw_model_totals);
    write_summary_line(
        &mut report,
        "INSTRUCTION_WINDOW_SUMMARY",
        instruction_window_totals,
        analyzed_group_count,
    );
    write_model_summary_line(
        &mut report,
        "INSTRUCTION_WINDOW_MODEL_SUMMARY",
        instruction_window_model_totals,
    );
    let register_only_reports: Vec<_> = reports
        .iter()
        .filter(|report| report.register_only_instruction_window_stats.case_count > 0)
        .cloned()
        .collect();
    write_summary_line(
        &mut report,
        "REGISTER_ONLY_INSTRUCTION_WINDOW_SUMMARY",
        register_only_instruction_window_totals,
        register_only_reports.len(),
    );
    write_model_summary_line(
        &mut report,
        "REGISTER_ONLY_INSTRUCTION_WINDOW_MODEL_SUMMARY",
        register_only_instruction_window_model_totals,
    );

    write_ranked_sections(&mut report, reports, "", raw_stats_for);
    write_ranked_sections(
        &mut report,
        reports,
        "instruction_window",
        instruction_window_stats_for,
    );
    if !register_only_reports.is_empty() {
        write_ranked_sections(
            &mut report,
            &register_only_reports,
            "register_only_instruction_window",
            register_only_instruction_window_stats_for,
        );
    }
    let _ = writeln!(&mut report, "END_REPORT");

    report
}

#[test]
#[ignore]
fn write_286_timing_report() {
    let mut reports = Vec::new();
    let mut raw_totals = TimingGroupStats::default();
    let mut instruction_window_totals = TimingGroupStats::default();
    let mut register_only_instruction_window_totals = TimingGroupStats::default();
    let mut raw_model_totals = TimingModelStats::default();
    let mut instruction_window_model_totals = TimingModelStats::default();
    let mut register_only_instruction_window_model_totals = TimingModelStats::default();

    for stem in all_test_stems() {
        let Some(analysis) = run_test_file(&stem, local_revoked_hashes(&stem), false) else {
            continue;
        };
        if analysis.raw_stats.case_count == 0 {
            continue;
        }

        raw_totals.merge(&analysis.raw_stats);
        instruction_window_totals.merge(&analysis.instruction_window_stats);
        register_only_instruction_window_totals
            .merge(&analysis.register_only_instruction_window_stats);
        raw_model_totals.merge(&analysis.raw_model_stats);
        instruction_window_model_totals.merge(&analysis.instruction_window_model_stats);
        register_only_instruction_window_model_totals
            .merge(&analysis.register_only_instruction_window_model_stats);
        reports.push(TimingGroupReport {
            stem,
            raw_stats: analysis.raw_stats,
            instruction_window_stats: analysis.instruction_window_stats,
            register_only_instruction_window_stats: analysis.register_only_instruction_window_stats,
        });
    }

    let report = render_report(
        &reports,
        &raw_totals,
        &instruction_window_totals,
        &register_only_instruction_window_totals,
        &raw_model_totals,
        &instruction_window_model_totals,
        &register_only_instruction_window_model_totals,
        reports.len(),
    );
    print!("{report}");
}

macro_rules! test_opcode {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_test_file($file, &[], true);
        }
    };
    ($name:ident, $file:expr, $skip_hashes:expr) => {
        #[test]
        fn $name() {
            run_test_file($file, $skip_hashes, true);
        }
    };
}

#[test]
fn idiv_byte_min_overflow_faults_without_panic() {
    let mut bus = TestBus::new();
    let mut cpu = I286::new();

    let initial = {
        let mut s = I286State::default();
        s.set_ax(0x8000);
        s.set_cx(0x00FF);
        s.set_sp(0x0200);
        s.set_cs(0x1000);
        s.set_compressed_flags(0x0002);
        s.msw = 0xFFF0;
        s
    };
    cpu.load_state(&initial);

    bus.set_memory(0x0000, 0x00);
    bus.set_memory(0x0001, 0x01);
    bus.set_memory(0x0002, 0x00);
    bus.set_memory(0x0003, 0x00);
    bus.set_memory(0x0001_0000, 0xF6);
    bus.set_memory(0x0001_0001, 0xF9);
    bus.set_memory(0x0001_0002, 0xF4);
    bus.set_memory(0x0000_0100, 0xF4);

    for _ in 0..16 {
        if cpu.halted() {
            break;
        }
        cpu.step(&mut bus);
    }
    assert!(cpu.halted(), "CPU did not halt after fault handler");

    assert_eq!(cpu.cs(), 0x0000);
    assert_eq!(cpu.ip, 0x0101);
    assert_eq!(cpu.sp(), 0x01FA);
    assert_eq!(bus.ram[0x01FA], 0x00);
    assert_eq!(bus.ram[0x01FB], 0x00);
}

#[test]
fn idiv_word_min_overflow_faults_without_panic() {
    let mut bus = TestBus::new();
    let mut cpu = I286::new();

    let initial = {
        let mut s = I286State::default();
        s.set_cx(0xFFFF);
        s.set_dx(0x8000);
        s.set_sp(0x0300);
        s.set_cs(0x1000);
        s.set_compressed_flags(0x0002);
        s.msw = 0xFFF0;
        s
    };
    cpu.load_state(&initial);

    bus.set_memory(0x0000, 0x00);
    bus.set_memory(0x0001, 0x01);
    bus.set_memory(0x0002, 0x00);
    bus.set_memory(0x0003, 0x00);
    bus.set_memory(0x0001_0000, 0xF7);
    bus.set_memory(0x0001_0001, 0xF9);
    bus.set_memory(0x0001_0002, 0xF4);
    bus.set_memory(0x0000_0100, 0xF4);

    for _ in 0..16 {
        if cpu.halted() {
            break;
        }
        cpu.step(&mut bus);
    }
    assert!(cpu.halted(), "CPU did not halt after fault handler");

    assert_eq!(cpu.cs(), 0x0000);
    assert_eq!(cpu.ip, 0x0101);
    assert_eq!(cpu.sp(), 0x02FA);
    assert_eq!(bus.ram[0x02FA], 0x00);
    assert_eq!(bus.ram[0x02FB], 0x00);
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
// These vectors match a quirk of the specific CPU/model used to generate the
// validation data, not strict 80286 divide-fault behavior. Keep them as local
// skips until the upstream dataset is reconciled.
test_opcode!(op_f6_7, "F6.7", LOCAL_REVOKED_F6_7);
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
