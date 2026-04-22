#![cfg(feature = "verification")]

#[path = "common/metadata_json.rs"]
mod metadata_json;
#[path = "common/verification_common.rs"]
mod verification_common;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use common::Cpu as _;
use cpu::{I8086, I8086State};
use metadata_json::{Metadata, load_metadata};
use verification_common::{MooState, load_moo_tests};

const RAM_SIZE: usize = 1_048_576;
const ADDRESS_MASK: u32 = 0x000F_FFFF;
const REG_ORDER_I8086: [&str; 14] = [
    "ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "flags",
];

struct TestBus {
    ram: Box<[u8]>,
    dirty: Vec<u32>,
    dirty_marker: Box<[u8]>,
    current_cycle: u64,
    wait_cycles: i64,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; RAM_SIZE].into_boxed_slice(),
            dirty: Vec::new(),
            dirty_marker: vec![0u8; RAM_SIZE].into_boxed_slice(),
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

    fn io_read_byte(&mut self, port: u16) -> u8 {
        let _ = port;
        0xFF
    }

    fn io_write_byte(&mut self, port: u16, value: u8) {
        let _ = (port, value);
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

fn functionality_dir() -> &'static Path {
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/SingleStepTests/8088/v2_binary")
    });
    &DIR
}

fn functionality_metadata() -> &'static Metadata {
    static META: LazyLock<Metadata> = LazyLock::new(|| {
        load_metadata(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/SingleStepTests/8088/v2/metadata.json"),
        )
    });
    &META
}

fn timing_dir() -> &'static Path {
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/SingleStepTests/8086/v1_binary")
    });
    &DIR
}

fn timing_metadata() -> &'static Metadata {
    static META: LazyLock<Metadata> = LazyLock::new(|| {
        load_metadata(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/SingleStepTests/8086/v1/metadata.json"),
        )
    });
    &META
}

fn should_test(status: &str) -> bool {
    matches!(
        status,
        "normal" | "alias" | "undocumented" | "fpu" | "undefined"
    )
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
            let expected_bit = u16::from(expected_masked & bit != 0);
            let actual_bit = u16::from(actual_masked & bit != 0);
            changed.push(format!("{name}:{expected_bit}->{actual_bit}"));
        }
    }

    format!(
        "  flags: expected 0x{expected_masked:04X}, got 0x{actual_masked:04X} [{}] (mask 0x{mask:04X})",
        changed.join(", ")
    )
}

fn initial_reg_value(regs: &HashMap<String, u32>, name: &str) -> u16 {
    regs.get(name)
        .copied()
        .unwrap_or_else(|| panic!("missing register in initial state: {name}")) as u16
}

fn build_state(regs: &HashMap<String, u16>) -> I8086State {
    let get = |name: &str| -> u16 {
        regs.get(name)
            .copied()
            .unwrap_or_else(|| panic!("missing register: {name}"))
    };

    let mut state = I8086State::default();
    state.set_ax(get("ax"));
    state.set_cx(get("cx"));
    state.set_dx(get("dx"));
    state.set_bx(get("bx"));
    state.set_sp(get("sp"));
    state.set_bp(get("bp"));
    state.set_si(get("si"));
    state.set_di(get("di"));
    state.set_es(get("es"));
    state.set_cs(get("cs"));
    state.set_ss(get("ss"));
    state.set_ds(get("ds"));
    state.ip = get("ip");
    state.set_compressed_flags(get("flags"));
    state
}

fn resolve_final_regs(
    initial: &HashMap<String, u32>,
    final_state: &HashMap<String, u32>,
) -> HashMap<String, u16> {
    REG_ORDER_I8086
        .iter()
        .map(|name| {
            let value = final_state
                .get(*name)
                .copied()
                .unwrap_or_else(|| u32::from(initial_reg_value(initial, name)));
            ((*name).to_string(), value as u16)
        })
        .collect()
}

fn resolve_initial_regs(initial: &HashMap<String, u32>) -> HashMap<String, u16> {
    REG_ORDER_I8086
        .iter()
        .map(|name| ((*name).to_string(), initial_reg_value(initial, name)))
        .collect()
}

fn is_division_exception(
    opcode: &str,
    reg_ext: Option<&str>,
    bytes: &[u8],
    initial: &MooState,
    expected: &I8086State,
) -> bool {
    let is_div = matches!(
        (opcode, reg_ext),
        ("F6", Some("6")) | ("F6", Some("7")) | ("F7", Some("6")) | ("F7", Some("7"))
    ) || (opcode == "D4" && bytes.last() == Some(&0));
    if !is_div {
        return false;
    }

    let byte_at = |address: u32| -> u16 {
        initial
            .ram
            .iter()
            .find(|(candidate, _)| *candidate == address)
            .map(|(_, value)| u16::from(*value))
            .unwrap_or(0)
    };
    let handler_ip = byte_at(0) | (byte_at(1) << 8);
    let handler_cs = byte_at(2) | (byte_at(3) << 8);

    expected.cs() == handler_cs && expected.ip == handler_ip
}

fn parse_stem(stem: &str) -> (&str, Option<&str>) {
    if let Some(dot_pos) = stem.find('.') {
        (&stem[..dot_pos], Some(&stem[dot_pos + 1..]))
    } else {
        (stem, None)
    }
}

fn status_and_mask<'a>(metadata: &'a Metadata, stem: &str) -> Option<(&'a str, u16)> {
    let (opcode, reg_ext) = parse_stem(stem);
    let entry = metadata.opcodes.get(opcode)?;
    match (reg_ext, &entry.reg) {
        (Some(reg_ext), Some(reg_map)) => {
            let info = reg_map.get(reg_ext)?;
            Some((info.status.as_str(), info.flags_mask.unwrap_or(0xFFFF)))
        }
        (Some(_), None) => None,
        (None, _) => {
            if let Some(status) = &entry.status {
                Some((status.as_str(), entry.flags_mask.unwrap_or(0xFFFF)))
            } else if let Some(reg_map) = &entry.reg {
                let any_testable = reg_map.values().any(|info| should_test(&info.status));
                if !any_testable {
                    return None;
                }
                let mask = reg_map
                    .values()
                    .map(|info| info.flags_mask.unwrap_or(0xFFFF))
                    .fold(0xFFFFu16, |combined_mask, flags_mask| {
                        combined_mask & flags_mask
                    });
                Some(("normal", mask))
            } else {
                None
            }
        }
    }
}

fn run_test_file(stem: &str, test_dir: &Path, metadata: &Metadata, check_cycles: bool) {
    let Some((status, flags_mask)) = status_and_mask(metadata, stem) else {
        return;
    };
    if !should_test(status) {
        return;
    }

    let filename = format!("{stem}.MOO.gz");
    let path = test_dir.join(&filename);
    let test_cases = load_moo_tests(&path, &REG_ORDER_I8086, &[]);
    let (opcode, reg_ext) = parse_stem(stem);

    let mut bus = TestBus::new();
    let mut failures: Vec<String> = Vec::new();

    for (index, test) in test_cases.iter().enumerate() {
        bus.clear();
        for &(address, value) in &test.initial.ram {
            bus.set_memory(address, value);
        }

        let initial_regs16 = resolve_initial_regs(&test.initial.regs);
        let final_regs16 = resolve_final_regs(&test.initial.regs, &test.final_state.regs);
        let initial_state = build_state(&initial_regs16);
        let expected = build_state(&final_regs16);

        let mut cpu = I8086::new();
        cpu.load_state(&initial_state);
        cpu.install_prefetch_queue(&test.initial.queue);
        cpu.step(&mut bus);

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
            initial_state.ax(),
            cpu.ax(),
            expected.ax(),
            &mut diffs,
        );
        check_reg(
            "bx",
            initial_state.bx(),
            cpu.bx(),
            expected.bx(),
            &mut diffs,
        );
        check_reg(
            "cx",
            initial_state.cx(),
            cpu.cx(),
            expected.cx(),
            &mut diffs,
        );
        check_reg(
            "dx",
            initial_state.dx(),
            cpu.dx(),
            expected.dx(),
            &mut diffs,
        );
        check_reg(
            "sp",
            initial_state.sp(),
            cpu.sp(),
            expected.sp(),
            &mut diffs,
        );
        check_reg(
            "bp",
            initial_state.bp(),
            cpu.bp(),
            expected.bp(),
            &mut diffs,
        );
        check_reg(
            "si",
            initial_state.si(),
            cpu.si(),
            expected.si(),
            &mut diffs,
        );
        check_reg(
            "di",
            initial_state.di(),
            cpu.di(),
            expected.di(),
            &mut diffs,
        );
        check_reg(
            "cs",
            initial_state.cs(),
            cpu.cs(),
            expected.cs(),
            &mut diffs,
        );
        check_reg(
            "ss",
            initial_state.ss(),
            cpu.ss(),
            expected.ss(),
            &mut diffs,
        );
        check_reg(
            "ds",
            initial_state.ds(),
            cpu.ds(),
            expected.ds(),
            &mut diffs,
        );
        check_reg(
            "es",
            initial_state.es(),
            cpu.es(),
            expected.es(),
            &mut diffs,
        );
        check_reg("ip", initial_state.ip, cpu.ip, expected.ip, &mut diffs);

        let actual_flags_masked = cpu.compressed_flags() & flags_mask;
        let expected_flags_masked = expected.compressed_flags() & flags_mask;
        if actual_flags_masked != expected_flags_masked {
            diffs.push(format!(
                "{} (was 0x{:04X})",
                format_flags_diff(
                    expected.compressed_flags(),
                    cpu.compressed_flags(),
                    flags_mask
                ),
                initial_state.compressed_flags()
            ));
        }

        if check_cycles {
            let actual_cycles = cpu.cycles_consumed();
            let expected_cycles = test.cycles.len() as u64;
            if actual_cycles != expected_cycles {
                diffs.push(format!(
                    "  cycles: expected {expected_cycles}, got {actual_cycles}"
                ));
            }
        }

        let division_exception =
            is_division_exception(opcode, reg_ext, &test.bytes, &test.initial, &expected);
        if !division_exception {
            for &(address, expected_value) in &test.final_state.ram {
                let actual_value = bus.ram[(address & ADDRESS_MASK) as usize];
                if actual_value != expected_value {
                    let initial_value = test
                        .initial
                        .ram
                        .iter()
                        .find(|(candidate, _)| *candidate == address)
                        .map(|(_, value)| *value);
                    match initial_value {
                        Some(before) => diffs.push(format!(
                            "  ram[0x{address:05X}]: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (was 0x{before:02X})"
                        )),
                        None => diffs.push(format!(
                            "  ram[0x{address:05X}]: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (not in initial RAM)"
                        )),
                    }
                }
            }
        }

        if !diffs.is_empty() {
            let bytes_hex: Vec<String> = test
                .bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect();
            if let Ok(path) = std::env::var("VERIFICATION_FAILURE_DUMP") {
                use std::io::Write;
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = writeln!(
                        file,
                        "{filename}\t{index}\t{}\t{}\t{}\t{}\t{}",
                        test.name,
                        bytes_hex.join(" "),
                        test.initial.queue.len(),
                        test.initial
                            .queue
                            .iter()
                            .map(|byte| format!("{byte:02X}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                        diffs.join(" | ")
                    );
                }
            }
            failures.push(format!(
                "[{filename} #{index}] {} ({})\n{}",
                test.name,
                bytes_hex.join(" "),
                diffs.join("\n")
            ));
        }
    }

    if !failures.is_empty() {
        let fail_count = failures.len();
        let test_count = test_cases.len();
        let mut message = format!("{filename}: {fail_count}/{test_count} tests failed\n");
        let display_count = failures.len().min(5);
        for failure in &failures[..display_count] {
            message.push_str(failure);
            message.push('\n');
        }
        if failures.len() > 5 {
            message.push_str(&format!("  ... and {} more failures\n", failures.len() - 5));
        }
        panic!("{message}");
    }
}

fn run_8088_functionality_file(stem: &str) {
    run_test_file(stem, functionality_dir(), functionality_metadata(), false);
}

fn run_8086_timing_file(stem: &str) {
    run_test_file(stem, timing_dir(), timing_metadata(), true);
}

macro_rules! test_functionality_opcode {
    ($name:ident, $file:expr) => {
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            run_8088_functionality_file($file);
        }
    };
}

macro_rules! test_timing_opcode {
    ($name:ident, $file:expr) => {
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            run_8086_timing_file($file);
        }
    };
}

test_functionality_opcode!(functionality_op_00, "00");
test_functionality_opcode!(functionality_op_01, "01");
test_functionality_opcode!(functionality_op_02, "02");
test_functionality_opcode!(functionality_op_03, "03");
test_functionality_opcode!(functionality_op_04, "04");
test_functionality_opcode!(functionality_op_05, "05");
test_functionality_opcode!(functionality_op_06, "06");
test_functionality_opcode!(functionality_op_07, "07");
test_functionality_opcode!(functionality_op_08, "08");
test_functionality_opcode!(functionality_op_09, "09");
test_functionality_opcode!(functionality_op_0a, "0A");
test_functionality_opcode!(functionality_op_0b, "0B");
test_functionality_opcode!(functionality_op_0c, "0C");
test_functionality_opcode!(functionality_op_0d, "0D");
test_functionality_opcode!(functionality_op_0e, "0E");
test_functionality_opcode!(functionality_op_10, "10");
test_functionality_opcode!(functionality_op_11, "11");
test_functionality_opcode!(functionality_op_12, "12");
test_functionality_opcode!(functionality_op_13, "13");
test_functionality_opcode!(functionality_op_14, "14");
test_functionality_opcode!(functionality_op_15, "15");
test_functionality_opcode!(functionality_op_16, "16");
test_functionality_opcode!(functionality_op_17, "17");
test_functionality_opcode!(functionality_op_18, "18");
test_functionality_opcode!(functionality_op_19, "19");
test_functionality_opcode!(functionality_op_1a, "1A");
test_functionality_opcode!(functionality_op_1b, "1B");
test_functionality_opcode!(functionality_op_1c, "1C");
test_functionality_opcode!(functionality_op_1d, "1D");
test_functionality_opcode!(functionality_op_1e, "1E");
test_functionality_opcode!(functionality_op_1f, "1F");
test_functionality_opcode!(functionality_op_20, "20");
test_functionality_opcode!(functionality_op_21, "21");
test_functionality_opcode!(functionality_op_22, "22");
test_functionality_opcode!(functionality_op_23, "23");
test_functionality_opcode!(functionality_op_24, "24");
test_functionality_opcode!(functionality_op_25, "25");
test_functionality_opcode!(functionality_op_27, "27");
test_functionality_opcode!(functionality_op_28, "28");
test_functionality_opcode!(functionality_op_29, "29");
test_functionality_opcode!(functionality_op_2a, "2A");
test_functionality_opcode!(functionality_op_2b, "2B");
test_functionality_opcode!(functionality_op_2c, "2C");
test_functionality_opcode!(functionality_op_2d, "2D");
test_functionality_opcode!(functionality_op_2f, "2F");
test_functionality_opcode!(functionality_op_30, "30");
test_functionality_opcode!(functionality_op_31, "31");
test_functionality_opcode!(functionality_op_32, "32");
test_functionality_opcode!(functionality_op_33, "33");
test_functionality_opcode!(functionality_op_34, "34");
test_functionality_opcode!(functionality_op_35, "35");
test_functionality_opcode!(functionality_op_37, "37");
test_functionality_opcode!(functionality_op_38, "38");
test_functionality_opcode!(functionality_op_39, "39");
test_functionality_opcode!(functionality_op_3a, "3A");
test_functionality_opcode!(functionality_op_3b, "3B");
test_functionality_opcode!(functionality_op_3c, "3C");
test_functionality_opcode!(functionality_op_3d, "3D");
test_functionality_opcode!(functionality_op_3f, "3F");
test_functionality_opcode!(functionality_op_40, "40");
test_functionality_opcode!(functionality_op_41, "41");
test_functionality_opcode!(functionality_op_42, "42");
test_functionality_opcode!(functionality_op_43, "43");
test_functionality_opcode!(functionality_op_44, "44");
test_functionality_opcode!(functionality_op_45, "45");
test_functionality_opcode!(functionality_op_46, "46");
test_functionality_opcode!(functionality_op_47, "47");
test_functionality_opcode!(functionality_op_48, "48");
test_functionality_opcode!(functionality_op_49, "49");
test_functionality_opcode!(functionality_op_4a, "4A");
test_functionality_opcode!(functionality_op_4b, "4B");
test_functionality_opcode!(functionality_op_4c, "4C");
test_functionality_opcode!(functionality_op_4d, "4D");
test_functionality_opcode!(functionality_op_4e, "4E");
test_functionality_opcode!(functionality_op_4f, "4F");
test_functionality_opcode!(functionality_op_50, "50");
test_functionality_opcode!(functionality_op_51, "51");
test_functionality_opcode!(functionality_op_52, "52");
test_functionality_opcode!(functionality_op_53, "53");
test_functionality_opcode!(functionality_op_54, "54");
test_functionality_opcode!(functionality_op_55, "55");
test_functionality_opcode!(functionality_op_56, "56");
test_functionality_opcode!(functionality_op_57, "57");
test_functionality_opcode!(functionality_op_58, "58");
test_functionality_opcode!(functionality_op_59, "59");
test_functionality_opcode!(functionality_op_5a, "5A");
test_functionality_opcode!(functionality_op_5b, "5B");
test_functionality_opcode!(functionality_op_5c, "5C");
test_functionality_opcode!(functionality_op_5d, "5D");
test_functionality_opcode!(functionality_op_5e, "5E");
test_functionality_opcode!(functionality_op_5f, "5F");
test_functionality_opcode!(functionality_op_60, "60");
test_functionality_opcode!(functionality_op_61, "61");
test_functionality_opcode!(functionality_op_62, "62");
test_functionality_opcode!(functionality_op_63, "63");
test_functionality_opcode!(functionality_op_64, "64");
test_functionality_opcode!(functionality_op_65, "65");
test_functionality_opcode!(functionality_op_66, "66");
test_functionality_opcode!(functionality_op_67, "67");
test_functionality_opcode!(functionality_op_68, "68");
test_functionality_opcode!(functionality_op_69, "69");
test_functionality_opcode!(functionality_op_6a, "6A");
test_functionality_opcode!(functionality_op_6b, "6B");
test_functionality_opcode!(functionality_op_6c, "6C");
test_functionality_opcode!(functionality_op_6d, "6D");
test_functionality_opcode!(functionality_op_6e, "6E");
test_functionality_opcode!(functionality_op_6f, "6F");
test_functionality_opcode!(functionality_op_70, "70");
test_functionality_opcode!(functionality_op_71, "71");
test_functionality_opcode!(functionality_op_72, "72");
test_functionality_opcode!(functionality_op_73, "73");
test_functionality_opcode!(functionality_op_74, "74");
test_functionality_opcode!(functionality_op_75, "75");
test_functionality_opcode!(functionality_op_76, "76");
test_functionality_opcode!(functionality_op_77, "77");
test_functionality_opcode!(functionality_op_78, "78");
test_functionality_opcode!(functionality_op_79, "79");
test_functionality_opcode!(functionality_op_7a, "7A");
test_functionality_opcode!(functionality_op_7b, "7B");
test_functionality_opcode!(functionality_op_7c, "7C");
test_functionality_opcode!(functionality_op_7d, "7D");
test_functionality_opcode!(functionality_op_7e, "7E");
test_functionality_opcode!(functionality_op_7f, "7F");
test_functionality_opcode!(functionality_op_80_0, "80.0");
test_functionality_opcode!(functionality_op_80_1, "80.1");
test_functionality_opcode!(functionality_op_80_2, "80.2");
test_functionality_opcode!(functionality_op_80_3, "80.3");
test_functionality_opcode!(functionality_op_80_4, "80.4");
test_functionality_opcode!(functionality_op_80_5, "80.5");
test_functionality_opcode!(functionality_op_80_6, "80.6");
test_functionality_opcode!(functionality_op_80_7, "80.7");
test_functionality_opcode!(functionality_op_81_0, "81.0");
test_functionality_opcode!(functionality_op_81_1, "81.1");
test_functionality_opcode!(functionality_op_81_2, "81.2");
test_functionality_opcode!(functionality_op_81_3, "81.3");
test_functionality_opcode!(functionality_op_81_4, "81.4");
test_functionality_opcode!(functionality_op_81_5, "81.5");
test_functionality_opcode!(functionality_op_81_6, "81.6");
test_functionality_opcode!(functionality_op_81_7, "81.7");
test_functionality_opcode!(functionality_op_82_0, "82.0");
test_functionality_opcode!(functionality_op_82_1, "82.1");
test_functionality_opcode!(functionality_op_82_2, "82.2");
test_functionality_opcode!(functionality_op_82_3, "82.3");
test_functionality_opcode!(functionality_op_82_4, "82.4");
test_functionality_opcode!(functionality_op_82_5, "82.5");
test_functionality_opcode!(functionality_op_82_6, "82.6");
test_functionality_opcode!(functionality_op_82_7, "82.7");
test_functionality_opcode!(functionality_op_83_0, "83.0");
test_functionality_opcode!(functionality_op_83_1, "83.1");
test_functionality_opcode!(functionality_op_83_2, "83.2");
test_functionality_opcode!(functionality_op_83_3, "83.3");
test_functionality_opcode!(functionality_op_83_4, "83.4");
test_functionality_opcode!(functionality_op_83_5, "83.5");
test_functionality_opcode!(functionality_op_83_6, "83.6");
test_functionality_opcode!(functionality_op_83_7, "83.7");
test_functionality_opcode!(functionality_op_84, "84");
test_functionality_opcode!(functionality_op_85, "85");
test_functionality_opcode!(functionality_op_86, "86");
test_functionality_opcode!(functionality_op_87, "87");
test_functionality_opcode!(functionality_op_88, "88");
test_functionality_opcode!(functionality_op_89, "89");
test_functionality_opcode!(functionality_op_8a, "8A");
test_functionality_opcode!(functionality_op_8b, "8B");
test_functionality_opcode!(functionality_op_8c, "8C");
test_functionality_opcode!(functionality_op_8d, "8D");
test_functionality_opcode!(functionality_op_8e, "8E");
test_functionality_opcode!(functionality_op_8f, "8F");
test_functionality_opcode!(functionality_op_90, "90");
test_functionality_opcode!(functionality_op_91, "91");
test_functionality_opcode!(functionality_op_92, "92");
test_functionality_opcode!(functionality_op_93, "93");
test_functionality_opcode!(functionality_op_94, "94");
test_functionality_opcode!(functionality_op_95, "95");
test_functionality_opcode!(functionality_op_96, "96");
test_functionality_opcode!(functionality_op_97, "97");
test_functionality_opcode!(functionality_op_98, "98");
test_functionality_opcode!(functionality_op_99, "99");
test_functionality_opcode!(functionality_op_9a, "9A");
test_functionality_opcode!(functionality_op_9c, "9C");
test_functionality_opcode!(functionality_op_9d, "9D");
test_functionality_opcode!(functionality_op_9e, "9E");
test_functionality_opcode!(functionality_op_9f, "9F");
test_functionality_opcode!(functionality_op_a0, "A0");
test_functionality_opcode!(functionality_op_a1, "A1");
test_functionality_opcode!(functionality_op_a2, "A2");
test_functionality_opcode!(functionality_op_a3, "A3");
test_functionality_opcode!(functionality_op_a4, "A4");
test_functionality_opcode!(functionality_op_a5, "A5");
test_functionality_opcode!(functionality_op_a6, "A6");
test_functionality_opcode!(functionality_op_a7, "A7");
test_functionality_opcode!(functionality_op_a8, "A8");
test_functionality_opcode!(functionality_op_a9, "A9");
test_functionality_opcode!(functionality_op_aa, "AA");
test_functionality_opcode!(functionality_op_ab, "AB");
test_functionality_opcode!(functionality_op_ac, "AC");
test_functionality_opcode!(functionality_op_ad, "AD");
test_functionality_opcode!(functionality_op_ae, "AE");
test_functionality_opcode!(functionality_op_af, "AF");
test_functionality_opcode!(functionality_op_b0, "B0");
test_functionality_opcode!(functionality_op_b1, "B1");
test_functionality_opcode!(functionality_op_b2, "B2");
test_functionality_opcode!(functionality_op_b3, "B3");
test_functionality_opcode!(functionality_op_b4, "B4");
test_functionality_opcode!(functionality_op_b5, "B5");
test_functionality_opcode!(functionality_op_b6, "B6");
test_functionality_opcode!(functionality_op_b7, "B7");
test_functionality_opcode!(functionality_op_b8, "B8");
test_functionality_opcode!(functionality_op_b9, "B9");
test_functionality_opcode!(functionality_op_ba, "BA");
test_functionality_opcode!(functionality_op_bb, "BB");
test_functionality_opcode!(functionality_op_bc, "BC");
test_functionality_opcode!(functionality_op_bd, "BD");
test_functionality_opcode!(functionality_op_be, "BE");
test_functionality_opcode!(functionality_op_bf, "BF");
test_functionality_opcode!(functionality_op_c0, "C0");
test_functionality_opcode!(functionality_op_c1, "C1");
test_functionality_opcode!(functionality_op_c2, "C2");
test_functionality_opcode!(functionality_op_c3, "C3");
test_functionality_opcode!(functionality_op_c4, "C4");
test_functionality_opcode!(functionality_op_c5, "C5");
test_functionality_opcode!(functionality_op_c6, "C6");
test_functionality_opcode!(functionality_op_c7, "C7");
test_functionality_opcode!(functionality_op_c8, "C8");
test_functionality_opcode!(functionality_op_c9, "C9");
test_functionality_opcode!(functionality_op_ca, "CA");
test_functionality_opcode!(functionality_op_cb, "CB");
test_functionality_opcode!(functionality_op_cc, "CC");
test_functionality_opcode!(functionality_op_cd, "CD");
test_functionality_opcode!(functionality_op_ce, "CE");
test_functionality_opcode!(functionality_op_cf, "CF");
test_functionality_opcode!(functionality_op_d0_0, "D0.0");
test_functionality_opcode!(functionality_op_d0_1, "D0.1");
test_functionality_opcode!(functionality_op_d0_2, "D0.2");
test_functionality_opcode!(functionality_op_d0_3, "D0.3");
test_functionality_opcode!(functionality_op_d0_4, "D0.4");
test_functionality_opcode!(functionality_op_d0_5, "D0.5");
test_functionality_opcode!(functionality_op_d0_6, "D0.6");
test_functionality_opcode!(functionality_op_d0_7, "D0.7");
test_functionality_opcode!(functionality_op_d1_0, "D1.0");
test_functionality_opcode!(functionality_op_d1_1, "D1.1");
test_functionality_opcode!(functionality_op_d1_2, "D1.2");
test_functionality_opcode!(functionality_op_d1_3, "D1.3");
test_functionality_opcode!(functionality_op_d1_4, "D1.4");
test_functionality_opcode!(functionality_op_d1_5, "D1.5");
test_functionality_opcode!(functionality_op_d1_6, "D1.6");
test_functionality_opcode!(functionality_op_d1_7, "D1.7");
test_functionality_opcode!(functionality_op_d2_0, "D2.0");
test_functionality_opcode!(functionality_op_d2_1, "D2.1");
test_functionality_opcode!(functionality_op_d2_2, "D2.2");
test_functionality_opcode!(functionality_op_d2_3, "D2.3");
test_functionality_opcode!(functionality_op_d2_4, "D2.4");
test_functionality_opcode!(functionality_op_d2_5, "D2.5");
test_functionality_opcode!(functionality_op_d2_6, "D2.6");
test_functionality_opcode!(functionality_op_d2_7, "D2.7");
test_functionality_opcode!(functionality_op_d3_0, "D3.0");
test_functionality_opcode!(functionality_op_d3_1, "D3.1");
test_functionality_opcode!(functionality_op_d3_2, "D3.2");
test_functionality_opcode!(functionality_op_d3_3, "D3.3");
test_functionality_opcode!(functionality_op_d3_4, "D3.4");
test_functionality_opcode!(functionality_op_d3_5, "D3.5");
test_functionality_opcode!(functionality_op_d3_6, "D3.6");
test_functionality_opcode!(functionality_op_d3_7, "D3.7");
test_functionality_opcode!(functionality_op_d4, "D4");
test_functionality_opcode!(functionality_op_d5, "D5");
test_functionality_opcode!(functionality_op_d6, "D6");
test_functionality_opcode!(functionality_op_d7, "D7");
test_functionality_opcode!(functionality_op_d8, "D8");
test_functionality_opcode!(functionality_op_d9, "D9");
test_functionality_opcode!(functionality_op_da, "DA");
test_functionality_opcode!(functionality_op_db, "DB");
test_functionality_opcode!(functionality_op_dc, "DC");
test_functionality_opcode!(functionality_op_dd, "DD");
test_functionality_opcode!(functionality_op_de, "DE");
test_functionality_opcode!(functionality_op_df, "DF");
test_functionality_opcode!(functionality_op_e0, "E0");
test_functionality_opcode!(functionality_op_e1, "E1");
test_functionality_opcode!(functionality_op_e2, "E2");
test_functionality_opcode!(functionality_op_e3, "E3");
test_functionality_opcode!(functionality_op_e4, "E4");
test_functionality_opcode!(functionality_op_e5, "E5");
test_functionality_opcode!(functionality_op_e6, "E6");
test_functionality_opcode!(functionality_op_e7, "E7");
test_functionality_opcode!(functionality_op_e8, "E8");
test_functionality_opcode!(functionality_op_e9, "E9");
test_functionality_opcode!(functionality_op_ea, "EA");
test_functionality_opcode!(functionality_op_eb, "EB");
test_functionality_opcode!(functionality_op_ec, "EC");
test_functionality_opcode!(functionality_op_ed, "ED");
test_functionality_opcode!(functionality_op_ee, "EE");
test_functionality_opcode!(functionality_op_ef, "EF");
test_functionality_opcode!(functionality_op_f5, "F5");
test_functionality_opcode!(functionality_op_f6_0, "F6.0");
test_functionality_opcode!(functionality_op_f6_1, "F6.1");
test_functionality_opcode!(functionality_op_f6_2, "F6.2");
test_functionality_opcode!(functionality_op_f6_3, "F6.3");
test_functionality_opcode!(functionality_op_f6_4, "F6.4");
test_functionality_opcode!(functionality_op_f6_5, "F6.5");
test_functionality_opcode!(functionality_op_f6_6, "F6.6");
test_functionality_opcode!(functionality_op_f6_7, "F6.7");
test_functionality_opcode!(functionality_op_f7_0, "F7.0");
test_functionality_opcode!(functionality_op_f7_1, "F7.1");
test_functionality_opcode!(functionality_op_f7_2, "F7.2");
test_functionality_opcode!(functionality_op_f7_3, "F7.3");
test_functionality_opcode!(functionality_op_f7_4, "F7.4");
test_functionality_opcode!(functionality_op_f7_5, "F7.5");
test_functionality_opcode!(functionality_op_f7_6, "F7.6");
test_functionality_opcode!(functionality_op_f7_7, "F7.7");
test_functionality_opcode!(functionality_op_f8, "F8");
test_functionality_opcode!(functionality_op_f9, "F9");
test_functionality_opcode!(functionality_op_fa, "FA");
test_functionality_opcode!(functionality_op_fb, "FB");
test_functionality_opcode!(functionality_op_fc, "FC");
test_functionality_opcode!(functionality_op_fd, "FD");
test_functionality_opcode!(functionality_op_fe_0, "FE.0");
test_functionality_opcode!(functionality_op_fe_1, "FE.1");
test_functionality_opcode!(functionality_op_ff_0, "FF.0");
test_functionality_opcode!(functionality_op_ff_1, "FF.1");
test_functionality_opcode!(functionality_op_ff_2, "FF.2");
test_functionality_opcode!(functionality_op_ff_3, "FF.3");
test_functionality_opcode!(functionality_op_ff_4, "FF.4");
test_functionality_opcode!(functionality_op_ff_5, "FF.5");
test_functionality_opcode!(functionality_op_ff_6, "FF.6");
test_functionality_opcode!(functionality_op_ff_7, "FF.7");

test_timing_opcode!(timing_op_00, "00");
test_timing_opcode!(timing_op_01, "01");
test_timing_opcode!(timing_op_02, "02");
test_timing_opcode!(timing_op_03, "03");
test_timing_opcode!(timing_op_04, "04");
test_timing_opcode!(timing_op_05, "05");
test_timing_opcode!(timing_op_06, "06");
test_timing_opcode!(timing_op_07, "07");
test_timing_opcode!(timing_op_08, "08");
test_timing_opcode!(timing_op_09, "09");
test_timing_opcode!(timing_op_0a, "0A");
test_timing_opcode!(timing_op_0b, "0B");
test_timing_opcode!(timing_op_0c, "0C");
test_timing_opcode!(timing_op_0d, "0D");
test_timing_opcode!(timing_op_0e, "0E");
test_timing_opcode!(timing_op_10, "10");
test_timing_opcode!(timing_op_11, "11");
test_timing_opcode!(timing_op_12, "12");
test_timing_opcode!(timing_op_13, "13");
test_timing_opcode!(timing_op_14, "14");
test_timing_opcode!(timing_op_15, "15");
test_timing_opcode!(timing_op_16, "16");
test_timing_opcode!(timing_op_17, "17");
test_timing_opcode!(timing_op_18, "18");
test_timing_opcode!(timing_op_19, "19");
test_timing_opcode!(timing_op_1a, "1A");
test_timing_opcode!(timing_op_1b, "1B");
test_timing_opcode!(timing_op_1c, "1C");
test_timing_opcode!(timing_op_1d, "1D");
test_timing_opcode!(timing_op_1e, "1E");
test_timing_opcode!(timing_op_1f, "1F");
test_timing_opcode!(timing_op_20, "20");
test_timing_opcode!(timing_op_21, "21");
test_timing_opcode!(timing_op_22, "22");
test_timing_opcode!(timing_op_23, "23");
test_timing_opcode!(timing_op_24, "24");
test_timing_opcode!(timing_op_25, "25");
test_timing_opcode!(timing_op_27, "27");
test_timing_opcode!(timing_op_28, "28");
test_timing_opcode!(timing_op_29, "29");
test_timing_opcode!(timing_op_2a, "2A");
test_timing_opcode!(timing_op_2b, "2B");
test_timing_opcode!(timing_op_2c, "2C");
test_timing_opcode!(timing_op_2d, "2D");
test_timing_opcode!(timing_op_2f, "2F");
test_timing_opcode!(timing_op_30, "30");
test_timing_opcode!(timing_op_31, "31");
test_timing_opcode!(timing_op_32, "32");
test_timing_opcode!(timing_op_33, "33");
test_timing_opcode!(timing_op_34, "34");
test_timing_opcode!(timing_op_35, "35");
test_timing_opcode!(timing_op_37, "37");
test_timing_opcode!(timing_op_38, "38");
test_timing_opcode!(timing_op_39, "39");
test_timing_opcode!(timing_op_3a, "3A");
test_timing_opcode!(timing_op_3b, "3B");
test_timing_opcode!(timing_op_3c, "3C");
test_timing_opcode!(timing_op_3d, "3D");
test_timing_opcode!(timing_op_3f, "3F");
test_timing_opcode!(timing_op_40, "40");
test_timing_opcode!(timing_op_41, "41");
test_timing_opcode!(timing_op_42, "42");
test_timing_opcode!(timing_op_43, "43");
test_timing_opcode!(timing_op_44, "44");
test_timing_opcode!(timing_op_45, "45");
test_timing_opcode!(timing_op_46, "46");
test_timing_opcode!(timing_op_47, "47");
test_timing_opcode!(timing_op_48, "48");
test_timing_opcode!(timing_op_49, "49");
test_timing_opcode!(timing_op_4a, "4A");
test_timing_opcode!(timing_op_4b, "4B");
test_timing_opcode!(timing_op_4c, "4C");
test_timing_opcode!(timing_op_4d, "4D");
test_timing_opcode!(timing_op_4e, "4E");
test_timing_opcode!(timing_op_4f, "4F");
test_timing_opcode!(timing_op_50, "50");
test_timing_opcode!(timing_op_51, "51");
test_timing_opcode!(timing_op_52, "52");
test_timing_opcode!(timing_op_53, "53");
test_timing_opcode!(timing_op_54, "54");
test_timing_opcode!(timing_op_55, "55");
test_timing_opcode!(timing_op_56, "56");
test_timing_opcode!(timing_op_57, "57");
test_timing_opcode!(timing_op_58, "58");
test_timing_opcode!(timing_op_59, "59");
test_timing_opcode!(timing_op_5a, "5A");
test_timing_opcode!(timing_op_5b, "5B");
test_timing_opcode!(timing_op_5c, "5C");
test_timing_opcode!(timing_op_5d, "5D");
test_timing_opcode!(timing_op_5e, "5E");
test_timing_opcode!(timing_op_5f, "5F");
test_timing_opcode!(timing_op_60, "60");
test_timing_opcode!(timing_op_61, "61");
test_timing_opcode!(timing_op_62, "62");
test_timing_opcode!(timing_op_63, "63");
test_timing_opcode!(timing_op_64, "64");
test_timing_opcode!(timing_op_65, "65");
test_timing_opcode!(timing_op_66, "66");
test_timing_opcode!(timing_op_67, "67");
test_timing_opcode!(timing_op_68, "68");
test_timing_opcode!(timing_op_69, "69");
test_timing_opcode!(timing_op_6a, "6A");
test_timing_opcode!(timing_op_6b, "6B");
test_timing_opcode!(timing_op_6c, "6C");
test_timing_opcode!(timing_op_6d, "6D");
test_timing_opcode!(timing_op_6e, "6E");
test_timing_opcode!(timing_op_6f, "6F");
test_timing_opcode!(timing_op_70, "70");
test_timing_opcode!(timing_op_71, "71");
test_timing_opcode!(timing_op_72, "72");
test_timing_opcode!(timing_op_73, "73");
test_timing_opcode!(timing_op_74, "74");
test_timing_opcode!(timing_op_75, "75");
test_timing_opcode!(timing_op_76, "76");
test_timing_opcode!(timing_op_77, "77");
test_timing_opcode!(timing_op_78, "78");
test_timing_opcode!(timing_op_79, "79");
test_timing_opcode!(timing_op_7a, "7A");
test_timing_opcode!(timing_op_7b, "7B");
test_timing_opcode!(timing_op_7c, "7C");
test_timing_opcode!(timing_op_7d, "7D");
test_timing_opcode!(timing_op_7e, "7E");
test_timing_opcode!(timing_op_7f, "7F");
test_timing_opcode!(timing_op_80_0, "80.0");
test_timing_opcode!(timing_op_80_1, "80.1");
test_timing_opcode!(timing_op_80_2, "80.2");
test_timing_opcode!(timing_op_80_3, "80.3");
test_timing_opcode!(timing_op_80_4, "80.4");
test_timing_opcode!(timing_op_80_5, "80.5");
test_timing_opcode!(timing_op_80_6, "80.6");
test_timing_opcode!(timing_op_80_7, "80.7");
test_timing_opcode!(timing_op_81_0, "81.0");
test_timing_opcode!(timing_op_81_1, "81.1");
test_timing_opcode!(timing_op_81_2, "81.2");
test_timing_opcode!(timing_op_81_3, "81.3");
test_timing_opcode!(timing_op_81_4, "81.4");
test_timing_opcode!(timing_op_81_5, "81.5");
test_timing_opcode!(timing_op_81_6, "81.6");
test_timing_opcode!(timing_op_81_7, "81.7");
test_timing_opcode!(timing_op_82_0, "82.0");
test_timing_opcode!(timing_op_82_1, "82.1");
test_timing_opcode!(timing_op_82_2, "82.2");
test_timing_opcode!(timing_op_82_3, "82.3");
test_timing_opcode!(timing_op_82_4, "82.4");
test_timing_opcode!(timing_op_82_5, "82.5");
test_timing_opcode!(timing_op_82_6, "82.6");
test_timing_opcode!(timing_op_82_7, "82.7");
test_timing_opcode!(timing_op_83_0, "83.0");
test_timing_opcode!(timing_op_83_1, "83.1");
test_timing_opcode!(timing_op_83_2, "83.2");
test_timing_opcode!(timing_op_83_3, "83.3");
test_timing_opcode!(timing_op_83_4, "83.4");
test_timing_opcode!(timing_op_83_5, "83.5");
test_timing_opcode!(timing_op_83_6, "83.6");
test_timing_opcode!(timing_op_83_7, "83.7");
test_timing_opcode!(timing_op_84, "84");
test_timing_opcode!(timing_op_85, "85");
test_timing_opcode!(timing_op_86, "86");
test_timing_opcode!(timing_op_87, "87");
test_timing_opcode!(timing_op_88, "88");
test_timing_opcode!(timing_op_89, "89");
test_timing_opcode!(timing_op_8a, "8A");
test_timing_opcode!(timing_op_8b, "8B");
test_timing_opcode!(timing_op_8c, "8C");
test_timing_opcode!(timing_op_8d, "8D");
test_timing_opcode!(timing_op_8e, "8E");
test_timing_opcode!(timing_op_8f, "8F");
test_timing_opcode!(timing_op_90, "90");
test_timing_opcode!(timing_op_91, "91");
test_timing_opcode!(timing_op_92, "92");
test_timing_opcode!(timing_op_93, "93");
test_timing_opcode!(timing_op_94, "94");
test_timing_opcode!(timing_op_95, "95");
test_timing_opcode!(timing_op_96, "96");
test_timing_opcode!(timing_op_97, "97");
test_timing_opcode!(timing_op_98, "98");
test_timing_opcode!(timing_op_99, "99");
test_timing_opcode!(timing_op_9a, "9A");
test_timing_opcode!(timing_op_9c, "9C");
test_timing_opcode!(timing_op_9d, "9D");
test_timing_opcode!(timing_op_9e, "9E");
test_timing_opcode!(timing_op_9f, "9F");
test_timing_opcode!(timing_op_a0, "A0");
test_timing_opcode!(timing_op_a1, "A1");
test_timing_opcode!(timing_op_a2, "A2");
test_timing_opcode!(timing_op_a3, "A3");
test_timing_opcode!(timing_op_a4, "A4");
test_timing_opcode!(timing_op_a5, "A5");
test_timing_opcode!(timing_op_a6, "A6");
test_timing_opcode!(timing_op_a7, "A7");
test_timing_opcode!(timing_op_a8, "A8");
test_timing_opcode!(timing_op_a9, "A9");
test_timing_opcode!(timing_op_aa, "AA");
test_timing_opcode!(timing_op_ab, "AB");
test_timing_opcode!(timing_op_ac, "AC");
test_timing_opcode!(timing_op_ad, "AD");
test_timing_opcode!(timing_op_ae, "AE");
test_timing_opcode!(timing_op_af, "AF");
test_timing_opcode!(timing_op_b0, "B0");
test_timing_opcode!(timing_op_b1, "B1");
test_timing_opcode!(timing_op_b2, "B2");
test_timing_opcode!(timing_op_b3, "B3");
test_timing_opcode!(timing_op_b4, "B4");
test_timing_opcode!(timing_op_b5, "B5");
test_timing_opcode!(timing_op_b6, "B6");
test_timing_opcode!(timing_op_b7, "B7");
test_timing_opcode!(timing_op_b8, "B8");
test_timing_opcode!(timing_op_b9, "B9");
test_timing_opcode!(timing_op_ba, "BA");
test_timing_opcode!(timing_op_bb, "BB");
test_timing_opcode!(timing_op_bc, "BC");
test_timing_opcode!(timing_op_bd, "BD");
test_timing_opcode!(timing_op_be, "BE");
test_timing_opcode!(timing_op_bf, "BF");
test_timing_opcode!(timing_op_c0, "C0");
test_timing_opcode!(timing_op_c1, "C1");
test_timing_opcode!(timing_op_c2, "C2");
test_timing_opcode!(timing_op_c3, "C3");
test_timing_opcode!(timing_op_c4, "C4");
test_timing_opcode!(timing_op_c5, "C5");
test_timing_opcode!(timing_op_c6, "C6");
test_timing_opcode!(timing_op_c7, "C7");
test_timing_opcode!(timing_op_c8, "C8");
test_timing_opcode!(timing_op_c9, "C9");
test_timing_opcode!(timing_op_ca, "CA");
test_timing_opcode!(timing_op_cb, "CB");
test_timing_opcode!(timing_op_cc, "CC");
test_timing_opcode!(timing_op_cd, "CD");
test_timing_opcode!(timing_op_ce, "CE");
test_timing_opcode!(timing_op_cf, "CF");
test_timing_opcode!(timing_op_d0_0, "D0.0");
test_timing_opcode!(timing_op_d0_1, "D0.1");
test_timing_opcode!(timing_op_d0_2, "D0.2");
test_timing_opcode!(timing_op_d0_3, "D0.3");
test_timing_opcode!(timing_op_d0_4, "D0.4");
test_timing_opcode!(timing_op_d0_5, "D0.5");
test_timing_opcode!(timing_op_d0_6, "D0.6");
test_timing_opcode!(timing_op_d0_7, "D0.7");
test_timing_opcode!(timing_op_d1_0, "D1.0");
test_timing_opcode!(timing_op_d1_1, "D1.1");
test_timing_opcode!(timing_op_d1_2, "D1.2");
test_timing_opcode!(timing_op_d1_3, "D1.3");
test_timing_opcode!(timing_op_d1_4, "D1.4");
test_timing_opcode!(timing_op_d1_5, "D1.5");
test_timing_opcode!(timing_op_d1_6, "D1.6");
test_timing_opcode!(timing_op_d1_7, "D1.7");
test_timing_opcode!(timing_op_d2_0, "D2.0");
test_timing_opcode!(timing_op_d2_1, "D2.1");
test_timing_opcode!(timing_op_d2_2, "D2.2");
test_timing_opcode!(timing_op_d2_3, "D2.3");
test_timing_opcode!(timing_op_d2_4, "D2.4");
test_timing_opcode!(timing_op_d2_5, "D2.5");
test_timing_opcode!(timing_op_d2_6, "D2.6");
test_timing_opcode!(timing_op_d2_7, "D2.7");
test_timing_opcode!(timing_op_d3_0, "D3.0");
test_timing_opcode!(timing_op_d3_1, "D3.1");
test_timing_opcode!(timing_op_d3_2, "D3.2");
test_timing_opcode!(timing_op_d3_3, "D3.3");
test_timing_opcode!(timing_op_d3_4, "D3.4");
test_timing_opcode!(timing_op_d3_5, "D3.5");
test_timing_opcode!(timing_op_d3_6, "D3.6");
test_timing_opcode!(timing_op_d3_7, "D3.7");
test_timing_opcode!(timing_op_d4, "D4");
test_timing_opcode!(timing_op_d5, "D5");
test_timing_opcode!(timing_op_d6, "D6");
test_timing_opcode!(timing_op_d7, "D7");
test_timing_opcode!(timing_op_d8, "D8");
test_timing_opcode!(timing_op_d9, "D9");
test_timing_opcode!(timing_op_da, "DA");
test_timing_opcode!(timing_op_db, "DB");
test_timing_opcode!(timing_op_dc, "DC");
test_timing_opcode!(timing_op_dd, "DD");
test_timing_opcode!(timing_op_de, "DE");
test_timing_opcode!(timing_op_df, "DF");
test_timing_opcode!(timing_op_e0, "E0");
test_timing_opcode!(timing_op_e1, "E1");
test_timing_opcode!(timing_op_e2, "E2");
test_timing_opcode!(timing_op_e3, "E3");
test_timing_opcode!(timing_op_e4, "E4");
test_timing_opcode!(timing_op_e5, "E5");
test_timing_opcode!(timing_op_e6, "E6");
test_timing_opcode!(timing_op_e7, "E7");
test_timing_opcode!(timing_op_e8, "E8");
test_timing_opcode!(timing_op_e9, "E9");
test_timing_opcode!(timing_op_ea, "EA");
test_timing_opcode!(timing_op_eb, "EB");
test_timing_opcode!(timing_op_ec, "EC");
test_timing_opcode!(timing_op_ed, "ED");
test_timing_opcode!(timing_op_ee, "EE");
test_timing_opcode!(timing_op_ef, "EF");
test_timing_opcode!(timing_op_f5, "F5");
test_timing_opcode!(timing_op_f6_0, "F6.0");
test_timing_opcode!(timing_op_f6_1, "F6.1");
test_timing_opcode!(timing_op_f6_2, "F6.2");
test_timing_opcode!(timing_op_f6_3, "F6.3");
test_timing_opcode!(timing_op_f6_4, "F6.4");
test_timing_opcode!(timing_op_f6_5, "F6.5");
test_timing_opcode!(timing_op_f6_6, "F6.6");
test_timing_opcode!(timing_op_f6_7, "F6.7");
test_timing_opcode!(timing_op_f7_0, "F7.0");
test_timing_opcode!(timing_op_f7_1, "F7.1");
test_timing_opcode!(timing_op_f7_2, "F7.2");
test_timing_opcode!(timing_op_f7_3, "F7.3");
test_timing_opcode!(timing_op_f7_4, "F7.4");
test_timing_opcode!(timing_op_f7_5, "F7.5");
test_timing_opcode!(timing_op_f7_6, "F7.6");
test_timing_opcode!(timing_op_f7_7, "F7.7");
test_timing_opcode!(timing_op_f8, "F8");
test_timing_opcode!(timing_op_f9, "F9");
test_timing_opcode!(timing_op_fa, "FA");
test_timing_opcode!(timing_op_fb, "FB");
test_timing_opcode!(timing_op_fc, "FC");
test_timing_opcode!(timing_op_fd, "FD");
test_timing_opcode!(timing_op_fe_0, "FE.0");
test_timing_opcode!(timing_op_fe_1, "FE.1");
test_timing_opcode!(timing_op_ff_0, "FF.0");
test_timing_opcode!(timing_op_ff_1, "FF.1");
test_timing_opcode!(timing_op_ff_2, "FF.2");
test_timing_opcode!(timing_op_ff_3, "FF.3");
test_timing_opcode!(timing_op_ff_4, "FF.4");
test_timing_opcode!(timing_op_ff_5, "FF.5");
test_timing_opcode!(timing_op_ff_6, "FF.6");
test_timing_opcode!(timing_op_ff_7, "FF.7");
