#![cfg(feature = "verification")]

#[path = "common/verification_common.rs"]
mod verification_common;

use std::{collections::HashMap, path::Path, sync::LazyLock};

use common::CpuZ80 as _;
use cpu::{Z80, Z80State};
use verification_common::{MooPort, load_moo_tests};

const REG_ORDER_Z80: [&str; 25] = [
    "a", "b", "c", "d", "e", "f", "h", "l", "i", "r", "ei", "wz", "ix", "iy", "af_", "bc_", "de_",
    "hl_", "im", "p", "q", "iff1", "iff2", "pc", "sp",
];

struct TestBus {
    ram: Box<[u8; 65_536]>,
    expected_ports: Vec<MooPort>,
    observed_ports: Vec<MooPort>,
    read_index: usize,
    current_cycle: u64,
    wait_cycles: i64,
}

impl TestBus {
    fn new(expected_ports: Vec<MooPort>) -> Self {
        Self {
            ram: vec![0u8; 65_536].into_boxed_slice().try_into().unwrap(),
            expected_ports,
            observed_ports: Vec::new(),
            read_index: 0,
            current_cycle: 0,
            wait_cycles: 0,
        }
    }
}

impl common::Bus for TestBus {
    fn read_byte(&mut self, address: u32) -> u8 {
        self.ram[(address & 0xFFFF) as usize]
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        self.ram[(address & 0xFFFF) as usize] = value;
    }

    fn io_read_byte(&mut self, port: u16) -> u8 {
        let expected = self
            .expected_ports
            .get(self.read_index)
            .unwrap_or_else(|| panic!("unexpected port read from 0x{port:04X}"));
        assert_eq!(
            expected.direction, b'r',
            "unexpected port read ordering at index {}",
            self.read_index
        );
        assert_eq!(
            expected.address, port,
            "unexpected port read address at index {}",
            self.read_index
        );
        self.read_index += 1;
        self.observed_ports.push(expected.clone());
        expected.value
    }

    fn io_write_byte(&mut self, port: u16, value: u8) {
        self.observed_ports.push(MooPort {
            address: port,
            value,
            direction: b'w',
        });
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

fn test_dir() -> &'static Path {
    static DIR: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/SingleStepTests/z80/v1")
    });
    &DIR
}

fn initial_reg_value(regs: &HashMap<String, u32>, name: &str) -> u32 {
    regs.get(name)
        .copied()
        .unwrap_or_else(|| panic!("missing register in initial state: {name}"))
}

fn resolve_regs(
    initial: &HashMap<String, u32>,
    final_state: &HashMap<String, u32>,
) -> HashMap<String, u32> {
    REG_ORDER_Z80
        .iter()
        .map(|name| {
            let value = final_state
                .get(*name)
                .copied()
                .unwrap_or_else(|| initial_reg_value(initial, name));
            ((*name).to_string(), value)
        })
        .collect()
}

fn build_state(regs: &HashMap<String, u32>) -> Z80State {
    let get = |name: &str| -> u32 {
        regs.get(name)
            .copied()
            .unwrap_or_else(|| panic!("missing register: {name}"))
    };

    let mut state = Z80State {
        a: get("a") as u8,
        b: get("b") as u8,
        c: get("c") as u8,
        d: get("d") as u8,
        e: get("e") as u8,
        h: get("h") as u8,
        l: get("l") as u8,
        i: get("i") as u8,
        ei: get("ei") as u8,
        wz: get("wz") as u16,
        ix: get("ix") as u16,
        iy: get("iy") as u16,
        af_alt: get("af_") as u16,
        bc_alt: get("bc_") as u16,
        de_alt: get("de_") as u16,
        hl_alt: get("hl_") as u16,
        im: get("im") as u8,
        p: get("p") as u8,
        q: get("q") as u8,
        iff1: get("iff1") != 0,
        iff2: get("iff2") != 0,
        pc: get("pc") as u16,
        sp: get("sp") as u16,
        ..Z80State::default()
    };
    state.flags.expand(get("f") as u8);
    state.set_r(get("r") as u8);
    state
}

fn format_flags_diff(expected: u8, actual: u8) -> String {
    const FLAG_BITS: &[(u8, &str)] = &[
        (0x80, "S"),
        (0x40, "Z"),
        (0x20, "Y"),
        (0x10, "H"),
        (0x08, "X"),
        (0x04, "PV"),
        (0x02, "N"),
        (0x01, "C"),
    ];

    let diff_bits = expected ^ actual;
    let mut changed = Vec::new();
    for &(bit, name) in FLAG_BITS {
        if diff_bits & bit != 0 {
            changed.push(format!(
                "{name}:{}->{}",
                u8::from(expected & bit != 0),
                u8::from(actual & bit != 0)
            ));
        }
    }
    format!(
        "  f: expected 0x{expected:02X}, got 0x{actual:02X} [{}]",
        changed.join(", ")
    )
}

fn run_test_file(stem: &str) {
    let path = test_dir().join(format!("{stem}.moo.gz"));
    let test_cases = load_moo_tests(&path, &[], &REG_ORDER_Z80);

    let mut failures = Vec::new();

    for (index, test) in test_cases.iter().enumerate() {
        let mut bus = TestBus::new(test.ports.clone());
        for &(address, value) in &test.initial.ram {
            bus.ram[(address & 0xFFFF) as usize] = value;
        }

        let initial_regs = resolve_regs(&test.initial.regs, &HashMap::new());
        let final_regs = resolve_regs(&test.initial.regs, &test.final_state.regs);
        let initial_state = build_state(&initial_regs);
        let expected = build_state(&final_regs);

        let mut cpu = Z80::new(4_000_000);
        cpu.load_state(&initial_state);
        cpu.step(&mut bus);

        let mut diffs = Vec::new();

        let check_u8 = |name: &str,
                        initial_value: u8,
                        actual_value: u8,
                        expected_value: u8,
                        diffs: &mut Vec<String>| {
            if actual_value != expected_value {
                diffs.push(format!(
                        "  {name}: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (was 0x{initial_value:02X})"
                    ));
            }
        };
        let check_u16 = |name: &str,
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

        check_u8("a", initial_state.a, cpu.a, expected.a, &mut diffs);
        check_u8("b", initial_state.b, cpu.b, expected.b, &mut diffs);
        check_u8("c", initial_state.c, cpu.c, expected.c, &mut diffs);
        check_u8("d", initial_state.d, cpu.d, expected.d, &mut diffs);
        check_u8("e", initial_state.e, cpu.e, expected.e, &mut diffs);
        if cpu.flags.compress() != expected.flags.compress() {
            diffs.push(format!(
                "{} (was 0x{:02X})",
                format_flags_diff(expected.flags.compress(), cpu.flags.compress()),
                initial_state.flags.compress()
            ));
        }
        check_u8("h", initial_state.h, cpu.h, expected.h, &mut diffs);
        check_u8("l", initial_state.l, cpu.l, expected.l, &mut diffs);
        check_u8("i", initial_state.i, cpu.i, expected.i, &mut diffs);
        check_u8("r", initial_state.r(), cpu.r(), expected.r(), &mut diffs);
        check_u8("ei", initial_state.ei, cpu.ei, expected.ei, &mut diffs);
        check_u16("wz", initial_state.wz, cpu.wz, expected.wz, &mut diffs);
        check_u16("ix", initial_state.ix, cpu.ix, expected.ix, &mut diffs);
        check_u16("iy", initial_state.iy, cpu.iy, expected.iy, &mut diffs);
        check_u16(
            "af_",
            initial_state.af_alt,
            cpu.af_alt,
            expected.af_alt,
            &mut diffs,
        );
        check_u16(
            "bc_",
            initial_state.bc_alt,
            cpu.bc_alt,
            expected.bc_alt,
            &mut diffs,
        );
        check_u16(
            "de_",
            initial_state.de_alt,
            cpu.de_alt,
            expected.de_alt,
            &mut diffs,
        );
        check_u16(
            "hl_",
            initial_state.hl_alt,
            cpu.hl_alt,
            expected.hl_alt,
            &mut diffs,
        );
        check_u8("im", initial_state.im, cpu.im, expected.im, &mut diffs);
        check_u8("p", initial_state.p, cpu.p, expected.p, &mut diffs);
        check_u8("q", initial_state.q, cpu.q, expected.q, &mut diffs);
        check_u8(
            "iff1",
            u8::from(initial_state.iff1),
            u8::from(cpu.iff1),
            u8::from(expected.iff1),
            &mut diffs,
        );
        check_u8(
            "iff2",
            u8::from(initial_state.iff2),
            u8::from(cpu.iff2),
            u8::from(expected.iff2),
            &mut diffs,
        );
        check_u16("pc", initial_state.pc, cpu.pc, expected.pc, &mut diffs);
        check_u16("sp", initial_state.sp, cpu.sp, expected.sp, &mut diffs);
        let actual_cycles = cpu.cycles_consumed();
        let expected_cycles = test.cycles.len() as u64;
        if actual_cycles != expected_cycles {
            diffs.push(format!(
                "  cycles: expected {expected_cycles}, got {actual_cycles}"
            ));
        }

        for &(address, expected_value) in &test.final_state.ram {
            let actual_value = bus.ram[(address & 0xFFFF) as usize];
            if actual_value != expected_value {
                let initial_value = test
                    .initial
                    .ram
                    .iter()
                    .find(|(candidate, _)| *candidate == address)
                    .map(|(_, value)| *value);
                match initial_value {
                    Some(before) => diffs.push(format!(
                        "  ram[0x{address:04X}]: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (was 0x{before:02X})"
                    )),
                    None => diffs.push(format!(
                        "  ram[0x{address:04X}]: expected 0x{expected_value:02X}, got 0x{actual_value:02X} (not in initial RAM)"
                    )),
                }
            }
        }

        if bus.observed_ports != test.ports {
            diffs.push(format!(
                "  ports: expected {:?}, got {:?}",
                test.ports, bus.observed_ports
            ));
        }

        if !diffs.is_empty() {
            let bytes_hex: Vec<String> = test
                .bytes
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect();
            failures.push(format!(
                "[{} #{index}] {} ({})\n{}",
                path.file_name().unwrap().to_string_lossy(),
                test.name,
                bytes_hex.join(" "),
                diffs.join("\n")
            ));
        }
    }

    if !failures.is_empty() {
        let fail_count = failures.len();
        let test_count = test_cases.len();
        let mut message = format!("{stem}.moo.gz: {fail_count}/{test_count} tests failed\n");
        for failure in failures.iter().take(5) {
            message.push_str(failure);
            message.push('\n');
        }
        if failures.len() > 5 {
            message.push_str(&format!("  ... and {} more failures\n", failures.len() - 5));
        }
        panic!("{message}");
    }
}

macro_rules! test_opcode {
    ($name:ident, $file:expr) => {
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            run_test_file($file);
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
test_opcode!(op_0a, "0a");
test_opcode!(op_0b, "0b");
test_opcode!(op_0c, "0c");
test_opcode!(op_0d, "0d");
test_opcode!(op_0e, "0e");
test_opcode!(op_0f, "0f");
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
test_opcode!(op_1a, "1a");
test_opcode!(op_1b, "1b");
test_opcode!(op_1c, "1c");
test_opcode!(op_1d, "1d");
test_opcode!(op_1e, "1e");
test_opcode!(op_1f, "1f");
test_opcode!(op_20, "20");
test_opcode!(op_21, "21");
test_opcode!(op_22, "22");
test_opcode!(op_23, "23");
test_opcode!(op_24, "24");
test_opcode!(op_25, "25");
test_opcode!(op_26, "26");
test_opcode!(op_27, "27");
test_opcode!(op_28, "28");
test_opcode!(op_29, "29");
test_opcode!(op_2a, "2a");
test_opcode!(op_2b, "2b");
test_opcode!(op_2c, "2c");
test_opcode!(op_2d, "2d");
test_opcode!(op_2e, "2e");
test_opcode!(op_2f, "2f");
test_opcode!(op_30, "30");
test_opcode!(op_31, "31");
test_opcode!(op_32, "32");
test_opcode!(op_33, "33");
test_opcode!(op_34, "34");
test_opcode!(op_35, "35");
test_opcode!(op_36, "36");
test_opcode!(op_37, "37");
test_opcode!(op_38, "38");
test_opcode!(op_39, "39");
test_opcode!(op_3a, "3a");
test_opcode!(op_3b, "3b");
test_opcode!(op_3c, "3c");
test_opcode!(op_3d, "3d");
test_opcode!(op_3e, "3e");
test_opcode!(op_3f, "3f");
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
test_opcode!(op_4a, "4a");
test_opcode!(op_4b, "4b");
test_opcode!(op_4c, "4c");
test_opcode!(op_4d, "4d");
test_opcode!(op_4e, "4e");
test_opcode!(op_4f, "4f");
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
test_opcode!(op_5a, "5a");
test_opcode!(op_5b, "5b");
test_opcode!(op_5c, "5c");
test_opcode!(op_5d, "5d");
test_opcode!(op_5e, "5e");
test_opcode!(op_5f, "5f");
test_opcode!(op_60, "60");
test_opcode!(op_61, "61");
test_opcode!(op_62, "62");
test_opcode!(op_63, "63");
test_opcode!(op_64, "64");
test_opcode!(op_65, "65");
test_opcode!(op_66, "66");
test_opcode!(op_67, "67");
test_opcode!(op_68, "68");
test_opcode!(op_69, "69");
test_opcode!(op_6a, "6a");
test_opcode!(op_6b, "6b");
test_opcode!(op_6c, "6c");
test_opcode!(op_6d, "6d");
test_opcode!(op_6e, "6e");
test_opcode!(op_6f, "6f");
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
test_opcode!(op_7a, "7a");
test_opcode!(op_7b, "7b");
test_opcode!(op_7c, "7c");
test_opcode!(op_7d, "7d");
test_opcode!(op_7e, "7e");
test_opcode!(op_7f, "7f");
test_opcode!(op_80, "80");
test_opcode!(op_81, "81");
test_opcode!(op_82, "82");
test_opcode!(op_83, "83");
test_opcode!(op_84, "84");
test_opcode!(op_85, "85");
test_opcode!(op_86, "86");
test_opcode!(op_87, "87");
test_opcode!(op_88, "88");
test_opcode!(op_89, "89");
test_opcode!(op_8a, "8a");
test_opcode!(op_8b, "8b");
test_opcode!(op_8c, "8c");
test_opcode!(op_8d, "8d");
test_opcode!(op_8e, "8e");
test_opcode!(op_8f, "8f");
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
test_opcode!(op_9a, "9a");
test_opcode!(op_9b, "9b");
test_opcode!(op_9c, "9c");
test_opcode!(op_9d, "9d");
test_opcode!(op_9e, "9e");
test_opcode!(op_9f, "9f");
test_opcode!(op_a0, "a0");
test_opcode!(op_a1, "a1");
test_opcode!(op_a2, "a2");
test_opcode!(op_a3, "a3");
test_opcode!(op_a4, "a4");
test_opcode!(op_a5, "a5");
test_opcode!(op_a6, "a6");
test_opcode!(op_a7, "a7");
test_opcode!(op_a8, "a8");
test_opcode!(op_a9, "a9");
test_opcode!(op_aa, "aa");
test_opcode!(op_ab, "ab");
test_opcode!(op_ac, "ac");
test_opcode!(op_ad, "ad");
test_opcode!(op_ae, "ae");
test_opcode!(op_af, "af");
test_opcode!(op_b0, "b0");
test_opcode!(op_b1, "b1");
test_opcode!(op_b2, "b2");
test_opcode!(op_b3, "b3");
test_opcode!(op_b4, "b4");
test_opcode!(op_b5, "b5");
test_opcode!(op_b6, "b6");
test_opcode!(op_b7, "b7");
test_opcode!(op_b8, "b8");
test_opcode!(op_b9, "b9");
test_opcode!(op_ba, "ba");
test_opcode!(op_bb, "bb");
test_opcode!(op_bc, "bc");
test_opcode!(op_bd, "bd");
test_opcode!(op_be, "be");
test_opcode!(op_bf, "bf");
test_opcode!(op_c0, "c0");
test_opcode!(op_c1, "c1");
test_opcode!(op_c2, "c2");
test_opcode!(op_c3, "c3");
test_opcode!(op_c4, "c4");
test_opcode!(op_c5, "c5");
test_opcode!(op_c6, "c6");
test_opcode!(op_c7, "c7");
test_opcode!(op_c8, "c8");
test_opcode!(op_c9, "c9");
test_opcode!(op_ca, "ca");
test_opcode!(op_cb_00, "cb 00");
test_opcode!(op_cb_01, "cb 01");
test_opcode!(op_cb_02, "cb 02");
test_opcode!(op_cb_03, "cb 03");
test_opcode!(op_cb_04, "cb 04");
test_opcode!(op_cb_05, "cb 05");
test_opcode!(op_cb_06, "cb 06");
test_opcode!(op_cb_07, "cb 07");
test_opcode!(op_cb_08, "cb 08");
test_opcode!(op_cb_09, "cb 09");
test_opcode!(op_cb_0a, "cb 0a");
test_opcode!(op_cb_0b, "cb 0b");
test_opcode!(op_cb_0c, "cb 0c");
test_opcode!(op_cb_0d, "cb 0d");
test_opcode!(op_cb_0e, "cb 0e");
test_opcode!(op_cb_0f, "cb 0f");
test_opcode!(op_cb_10, "cb 10");
test_opcode!(op_cb_11, "cb 11");
test_opcode!(op_cb_12, "cb 12");
test_opcode!(op_cb_13, "cb 13");
test_opcode!(op_cb_14, "cb 14");
test_opcode!(op_cb_15, "cb 15");
test_opcode!(op_cb_16, "cb 16");
test_opcode!(op_cb_17, "cb 17");
test_opcode!(op_cb_18, "cb 18");
test_opcode!(op_cb_19, "cb 19");
test_opcode!(op_cb_1a, "cb 1a");
test_opcode!(op_cb_1b, "cb 1b");
test_opcode!(op_cb_1c, "cb 1c");
test_opcode!(op_cb_1d, "cb 1d");
test_opcode!(op_cb_1e, "cb 1e");
test_opcode!(op_cb_1f, "cb 1f");
test_opcode!(op_cb_20, "cb 20");
test_opcode!(op_cb_21, "cb 21");
test_opcode!(op_cb_22, "cb 22");
test_opcode!(op_cb_23, "cb 23");
test_opcode!(op_cb_24, "cb 24");
test_opcode!(op_cb_25, "cb 25");
test_opcode!(op_cb_26, "cb 26");
test_opcode!(op_cb_27, "cb 27");
test_opcode!(op_cb_28, "cb 28");
test_opcode!(op_cb_29, "cb 29");
test_opcode!(op_cb_2a, "cb 2a");
test_opcode!(op_cb_2b, "cb 2b");
test_opcode!(op_cb_2c, "cb 2c");
test_opcode!(op_cb_2d, "cb 2d");
test_opcode!(op_cb_2e, "cb 2e");
test_opcode!(op_cb_2f, "cb 2f");
test_opcode!(op_cb_30, "cb 30");
test_opcode!(op_cb_31, "cb 31");
test_opcode!(op_cb_32, "cb 32");
test_opcode!(op_cb_33, "cb 33");
test_opcode!(op_cb_34, "cb 34");
test_opcode!(op_cb_35, "cb 35");
test_opcode!(op_cb_36, "cb 36");
test_opcode!(op_cb_37, "cb 37");
test_opcode!(op_cb_38, "cb 38");
test_opcode!(op_cb_39, "cb 39");
test_opcode!(op_cb_3a, "cb 3a");
test_opcode!(op_cb_3b, "cb 3b");
test_opcode!(op_cb_3c, "cb 3c");
test_opcode!(op_cb_3d, "cb 3d");
test_opcode!(op_cb_3e, "cb 3e");
test_opcode!(op_cb_3f, "cb 3f");
test_opcode!(op_cb_40, "cb 40");
test_opcode!(op_cb_41, "cb 41");
test_opcode!(op_cb_42, "cb 42");
test_opcode!(op_cb_43, "cb 43");
test_opcode!(op_cb_44, "cb 44");
test_opcode!(op_cb_45, "cb 45");
test_opcode!(op_cb_46, "cb 46");
test_opcode!(op_cb_47, "cb 47");
test_opcode!(op_cb_48, "cb 48");
test_opcode!(op_cb_49, "cb 49");
test_opcode!(op_cb_4a, "cb 4a");
test_opcode!(op_cb_4b, "cb 4b");
test_opcode!(op_cb_4c, "cb 4c");
test_opcode!(op_cb_4d, "cb 4d");
test_opcode!(op_cb_4e, "cb 4e");
test_opcode!(op_cb_4f, "cb 4f");
test_opcode!(op_cb_50, "cb 50");
test_opcode!(op_cb_51, "cb 51");
test_opcode!(op_cb_52, "cb 52");
test_opcode!(op_cb_53, "cb 53");
test_opcode!(op_cb_54, "cb 54");
test_opcode!(op_cb_55, "cb 55");
test_opcode!(op_cb_56, "cb 56");
test_opcode!(op_cb_57, "cb 57");
test_opcode!(op_cb_58, "cb 58");
test_opcode!(op_cb_59, "cb 59");
test_opcode!(op_cb_5a, "cb 5a");
test_opcode!(op_cb_5b, "cb 5b");
test_opcode!(op_cb_5c, "cb 5c");
test_opcode!(op_cb_5d, "cb 5d");
test_opcode!(op_cb_5e, "cb 5e");
test_opcode!(op_cb_5f, "cb 5f");
test_opcode!(op_cb_60, "cb 60");
test_opcode!(op_cb_61, "cb 61");
test_opcode!(op_cb_62, "cb 62");
test_opcode!(op_cb_63, "cb 63");
test_opcode!(op_cb_64, "cb 64");
test_opcode!(op_cb_65, "cb 65");
test_opcode!(op_cb_66, "cb 66");
test_opcode!(op_cb_67, "cb 67");
test_opcode!(op_cb_68, "cb 68");
test_opcode!(op_cb_69, "cb 69");
test_opcode!(op_cb_6a, "cb 6a");
test_opcode!(op_cb_6b, "cb 6b");
test_opcode!(op_cb_6c, "cb 6c");
test_opcode!(op_cb_6d, "cb 6d");
test_opcode!(op_cb_6e, "cb 6e");
test_opcode!(op_cb_6f, "cb 6f");
test_opcode!(op_cb_70, "cb 70");
test_opcode!(op_cb_71, "cb 71");
test_opcode!(op_cb_72, "cb 72");
test_opcode!(op_cb_73, "cb 73");
test_opcode!(op_cb_74, "cb 74");
test_opcode!(op_cb_75, "cb 75");
test_opcode!(op_cb_76, "cb 76");
test_opcode!(op_cb_77, "cb 77");
test_opcode!(op_cb_78, "cb 78");
test_opcode!(op_cb_79, "cb 79");
test_opcode!(op_cb_7a, "cb 7a");
test_opcode!(op_cb_7b, "cb 7b");
test_opcode!(op_cb_7c, "cb 7c");
test_opcode!(op_cb_7d, "cb 7d");
test_opcode!(op_cb_7e, "cb 7e");
test_opcode!(op_cb_7f, "cb 7f");
test_opcode!(op_cb_80, "cb 80");
test_opcode!(op_cb_81, "cb 81");
test_opcode!(op_cb_82, "cb 82");
test_opcode!(op_cb_83, "cb 83");
test_opcode!(op_cb_84, "cb 84");
test_opcode!(op_cb_85, "cb 85");
test_opcode!(op_cb_86, "cb 86");
test_opcode!(op_cb_87, "cb 87");
test_opcode!(op_cb_88, "cb 88");
test_opcode!(op_cb_89, "cb 89");
test_opcode!(op_cb_8a, "cb 8a");
test_opcode!(op_cb_8b, "cb 8b");
test_opcode!(op_cb_8c, "cb 8c");
test_opcode!(op_cb_8d, "cb 8d");
test_opcode!(op_cb_8e, "cb 8e");
test_opcode!(op_cb_8f, "cb 8f");
test_opcode!(op_cb_90, "cb 90");
test_opcode!(op_cb_91, "cb 91");
test_opcode!(op_cb_92, "cb 92");
test_opcode!(op_cb_93, "cb 93");
test_opcode!(op_cb_94, "cb 94");
test_opcode!(op_cb_95, "cb 95");
test_opcode!(op_cb_96, "cb 96");
test_opcode!(op_cb_97, "cb 97");
test_opcode!(op_cb_98, "cb 98");
test_opcode!(op_cb_99, "cb 99");
test_opcode!(op_cb_9a, "cb 9a");
test_opcode!(op_cb_9b, "cb 9b");
test_opcode!(op_cb_9c, "cb 9c");
test_opcode!(op_cb_9d, "cb 9d");
test_opcode!(op_cb_9e, "cb 9e");
test_opcode!(op_cb_9f, "cb 9f");
test_opcode!(op_cb_a0, "cb a0");
test_opcode!(op_cb_a1, "cb a1");
test_opcode!(op_cb_a2, "cb a2");
test_opcode!(op_cb_a3, "cb a3");
test_opcode!(op_cb_a4, "cb a4");
test_opcode!(op_cb_a5, "cb a5");
test_opcode!(op_cb_a6, "cb a6");
test_opcode!(op_cb_a7, "cb a7");
test_opcode!(op_cb_a8, "cb a8");
test_opcode!(op_cb_a9, "cb a9");
test_opcode!(op_cb_aa, "cb aa");
test_opcode!(op_cb_ab, "cb ab");
test_opcode!(op_cb_ac, "cb ac");
test_opcode!(op_cb_ad, "cb ad");
test_opcode!(op_cb_ae, "cb ae");
test_opcode!(op_cb_af, "cb af");
test_opcode!(op_cb_b0, "cb b0");
test_opcode!(op_cb_b1, "cb b1");
test_opcode!(op_cb_b2, "cb b2");
test_opcode!(op_cb_b3, "cb b3");
test_opcode!(op_cb_b4, "cb b4");
test_opcode!(op_cb_b5, "cb b5");
test_opcode!(op_cb_b6, "cb b6");
test_opcode!(op_cb_b7, "cb b7");
test_opcode!(op_cb_b8, "cb b8");
test_opcode!(op_cb_b9, "cb b9");
test_opcode!(op_cb_ba, "cb ba");
test_opcode!(op_cb_bb, "cb bb");
test_opcode!(op_cb_bc, "cb bc");
test_opcode!(op_cb_bd, "cb bd");
test_opcode!(op_cb_be, "cb be");
test_opcode!(op_cb_bf, "cb bf");
test_opcode!(op_cb_c0, "cb c0");
test_opcode!(op_cb_c1, "cb c1");
test_opcode!(op_cb_c2, "cb c2");
test_opcode!(op_cb_c3, "cb c3");
test_opcode!(op_cb_c4, "cb c4");
test_opcode!(op_cb_c5, "cb c5");
test_opcode!(op_cb_c6, "cb c6");
test_opcode!(op_cb_c7, "cb c7");
test_opcode!(op_cb_c8, "cb c8");
test_opcode!(op_cb_c9, "cb c9");
test_opcode!(op_cb_ca, "cb ca");
test_opcode!(op_cb_cb, "cb cb");
test_opcode!(op_cb_cc, "cb cc");
test_opcode!(op_cb_cd, "cb cd");
test_opcode!(op_cb_ce, "cb ce");
test_opcode!(op_cb_cf, "cb cf");
test_opcode!(op_cb_d0, "cb d0");
test_opcode!(op_cb_d1, "cb d1");
test_opcode!(op_cb_d2, "cb d2");
test_opcode!(op_cb_d3, "cb d3");
test_opcode!(op_cb_d4, "cb d4");
test_opcode!(op_cb_d5, "cb d5");
test_opcode!(op_cb_d6, "cb d6");
test_opcode!(op_cb_d7, "cb d7");
test_opcode!(op_cb_d8, "cb d8");
test_opcode!(op_cb_d9, "cb d9");
test_opcode!(op_cb_da, "cb da");
test_opcode!(op_cb_db, "cb db");
test_opcode!(op_cb_dc, "cb dc");
test_opcode!(op_cb_dd, "cb dd");
test_opcode!(op_cb_de, "cb de");
test_opcode!(op_cb_df, "cb df");
test_opcode!(op_cb_e0, "cb e0");
test_opcode!(op_cb_e1, "cb e1");
test_opcode!(op_cb_e2, "cb e2");
test_opcode!(op_cb_e3, "cb e3");
test_opcode!(op_cb_e4, "cb e4");
test_opcode!(op_cb_e5, "cb e5");
test_opcode!(op_cb_e6, "cb e6");
test_opcode!(op_cb_e7, "cb e7");
test_opcode!(op_cb_e8, "cb e8");
test_opcode!(op_cb_e9, "cb e9");
test_opcode!(op_cb_ea, "cb ea");
test_opcode!(op_cb_eb, "cb eb");
test_opcode!(op_cb_ec, "cb ec");
test_opcode!(op_cb_ed, "cb ed");
test_opcode!(op_cb_ee, "cb ee");
test_opcode!(op_cb_ef, "cb ef");
test_opcode!(op_cb_f0, "cb f0");
test_opcode!(op_cb_f1, "cb f1");
test_opcode!(op_cb_f2, "cb f2");
test_opcode!(op_cb_f3, "cb f3");
test_opcode!(op_cb_f4, "cb f4");
test_opcode!(op_cb_f5, "cb f5");
test_opcode!(op_cb_f6, "cb f6");
test_opcode!(op_cb_f7, "cb f7");
test_opcode!(op_cb_f8, "cb f8");
test_opcode!(op_cb_f9, "cb f9");
test_opcode!(op_cb_fa, "cb fa");
test_opcode!(op_cb_fb, "cb fb");
test_opcode!(op_cb_fc, "cb fc");
test_opcode!(op_cb_fd, "cb fd");
test_opcode!(op_cb_fe, "cb fe");
test_opcode!(op_cb_ff, "cb ff");
test_opcode!(op_cc, "cc");
test_opcode!(op_cd, "cd");
test_opcode!(op_ce, "ce");
test_opcode!(op_cf, "cf");
test_opcode!(op_d0, "d0");
test_opcode!(op_d1, "d1");
test_opcode!(op_d2, "d2");
test_opcode!(op_d3, "d3");
test_opcode!(op_d4, "d4");
test_opcode!(op_d5, "d5");
test_opcode!(op_d6, "d6");
test_opcode!(op_d7, "d7");
test_opcode!(op_d8, "d8");
test_opcode!(op_d9, "d9");
test_opcode!(op_da, "da");
test_opcode!(op_db, "db");
test_opcode!(op_dc, "dc");
test_opcode!(op_dd_00, "dd 00");
test_opcode!(op_dd_01, "dd 01");
test_opcode!(op_dd_02, "dd 02");
test_opcode!(op_dd_03, "dd 03");
test_opcode!(op_dd_04, "dd 04");
test_opcode!(op_dd_05, "dd 05");
test_opcode!(op_dd_06, "dd 06");
test_opcode!(op_dd_07, "dd 07");
test_opcode!(op_dd_08, "dd 08");
test_opcode!(op_dd_09, "dd 09");
test_opcode!(op_dd_0a, "dd 0a");
test_opcode!(op_dd_0b, "dd 0b");
test_opcode!(op_dd_0c, "dd 0c");
test_opcode!(op_dd_0d, "dd 0d");
test_opcode!(op_dd_0e, "dd 0e");
test_opcode!(op_dd_0f, "dd 0f");
test_opcode!(op_dd_10, "dd 10");
test_opcode!(op_dd_11, "dd 11");
test_opcode!(op_dd_12, "dd 12");
test_opcode!(op_dd_13, "dd 13");
test_opcode!(op_dd_14, "dd 14");
test_opcode!(op_dd_15, "dd 15");
test_opcode!(op_dd_16, "dd 16");
test_opcode!(op_dd_17, "dd 17");
test_opcode!(op_dd_18, "dd 18");
test_opcode!(op_dd_19, "dd 19");
test_opcode!(op_dd_1a, "dd 1a");
test_opcode!(op_dd_1b, "dd 1b");
test_opcode!(op_dd_1c, "dd 1c");
test_opcode!(op_dd_1d, "dd 1d");
test_opcode!(op_dd_1e, "dd 1e");
test_opcode!(op_dd_1f, "dd 1f");
test_opcode!(op_dd_20, "dd 20");
test_opcode!(op_dd_21, "dd 21");
test_opcode!(op_dd_22, "dd 22");
test_opcode!(op_dd_23, "dd 23");
test_opcode!(op_dd_24, "dd 24");
test_opcode!(op_dd_25, "dd 25");
test_opcode!(op_dd_26, "dd 26");
test_opcode!(op_dd_27, "dd 27");
test_opcode!(op_dd_28, "dd 28");
test_opcode!(op_dd_29, "dd 29");
test_opcode!(op_dd_2a, "dd 2a");
test_opcode!(op_dd_2b, "dd 2b");
test_opcode!(op_dd_2c, "dd 2c");
test_opcode!(op_dd_2d, "dd 2d");
test_opcode!(op_dd_2e, "dd 2e");
test_opcode!(op_dd_2f, "dd 2f");
test_opcode!(op_dd_30, "dd 30");
test_opcode!(op_dd_31, "dd 31");
test_opcode!(op_dd_32, "dd 32");
test_opcode!(op_dd_33, "dd 33");
test_opcode!(op_dd_34, "dd 34");
test_opcode!(op_dd_35, "dd 35");
test_opcode!(op_dd_36, "dd 36");
test_opcode!(op_dd_37, "dd 37");
test_opcode!(op_dd_38, "dd 38");
test_opcode!(op_dd_39, "dd 39");
test_opcode!(op_dd_3a, "dd 3a");
test_opcode!(op_dd_3b, "dd 3b");
test_opcode!(op_dd_3c, "dd 3c");
test_opcode!(op_dd_3d, "dd 3d");
test_opcode!(op_dd_3e, "dd 3e");
test_opcode!(op_dd_3f, "dd 3f");
test_opcode!(op_dd_40, "dd 40");
test_opcode!(op_dd_41, "dd 41");
test_opcode!(op_dd_42, "dd 42");
test_opcode!(op_dd_43, "dd 43");
test_opcode!(op_dd_44, "dd 44");
test_opcode!(op_dd_45, "dd 45");
test_opcode!(op_dd_46, "dd 46");
test_opcode!(op_dd_47, "dd 47");
test_opcode!(op_dd_48, "dd 48");
test_opcode!(op_dd_49, "dd 49");
test_opcode!(op_dd_4a, "dd 4a");
test_opcode!(op_dd_4b, "dd 4b");
test_opcode!(op_dd_4c, "dd 4c");
test_opcode!(op_dd_4d, "dd 4d");
test_opcode!(op_dd_4e, "dd 4e");
test_opcode!(op_dd_4f, "dd 4f");
test_opcode!(op_dd_50, "dd 50");
test_opcode!(op_dd_51, "dd 51");
test_opcode!(op_dd_52, "dd 52");
test_opcode!(op_dd_53, "dd 53");
test_opcode!(op_dd_54, "dd 54");
test_opcode!(op_dd_55, "dd 55");
test_opcode!(op_dd_56, "dd 56");
test_opcode!(op_dd_57, "dd 57");
test_opcode!(op_dd_58, "dd 58");
test_opcode!(op_dd_59, "dd 59");
test_opcode!(op_dd_5a, "dd 5a");
test_opcode!(op_dd_5b, "dd 5b");
test_opcode!(op_dd_5c, "dd 5c");
test_opcode!(op_dd_5d, "dd 5d");
test_opcode!(op_dd_5e, "dd 5e");
test_opcode!(op_dd_5f, "dd 5f");
test_opcode!(op_dd_60, "dd 60");
test_opcode!(op_dd_61, "dd 61");
test_opcode!(op_dd_62, "dd 62");
test_opcode!(op_dd_63, "dd 63");
test_opcode!(op_dd_64, "dd 64");
test_opcode!(op_dd_65, "dd 65");
test_opcode!(op_dd_66, "dd 66");
test_opcode!(op_dd_67, "dd 67");
test_opcode!(op_dd_68, "dd 68");
test_opcode!(op_dd_69, "dd 69");
test_opcode!(op_dd_6a, "dd 6a");
test_opcode!(op_dd_6b, "dd 6b");
test_opcode!(op_dd_6c, "dd 6c");
test_opcode!(op_dd_6d, "dd 6d");
test_opcode!(op_dd_6e, "dd 6e");
test_opcode!(op_dd_6f, "dd 6f");
test_opcode!(op_dd_70, "dd 70");
test_opcode!(op_dd_71, "dd 71");
test_opcode!(op_dd_72, "dd 72");
test_opcode!(op_dd_73, "dd 73");
test_opcode!(op_dd_74, "dd 74");
test_opcode!(op_dd_75, "dd 75");
test_opcode!(op_dd_76, "dd 76");
test_opcode!(op_dd_77, "dd 77");
test_opcode!(op_dd_78, "dd 78");
test_opcode!(op_dd_79, "dd 79");
test_opcode!(op_dd_7a, "dd 7a");
test_opcode!(op_dd_7b, "dd 7b");
test_opcode!(op_dd_7c, "dd 7c");
test_opcode!(op_dd_7d, "dd 7d");
test_opcode!(op_dd_7e, "dd 7e");
test_opcode!(op_dd_7f, "dd 7f");
test_opcode!(op_dd_80, "dd 80");
test_opcode!(op_dd_81, "dd 81");
test_opcode!(op_dd_82, "dd 82");
test_opcode!(op_dd_83, "dd 83");
test_opcode!(op_dd_84, "dd 84");
test_opcode!(op_dd_85, "dd 85");
test_opcode!(op_dd_86, "dd 86");
test_opcode!(op_dd_87, "dd 87");
test_opcode!(op_dd_88, "dd 88");
test_opcode!(op_dd_89, "dd 89");
test_opcode!(op_dd_8a, "dd 8a");
test_opcode!(op_dd_8b, "dd 8b");
test_opcode!(op_dd_8c, "dd 8c");
test_opcode!(op_dd_8d, "dd 8d");
test_opcode!(op_dd_8e, "dd 8e");
test_opcode!(op_dd_8f, "dd 8f");
test_opcode!(op_dd_90, "dd 90");
test_opcode!(op_dd_91, "dd 91");
test_opcode!(op_dd_92, "dd 92");
test_opcode!(op_dd_93, "dd 93");
test_opcode!(op_dd_94, "dd 94");
test_opcode!(op_dd_95, "dd 95");
test_opcode!(op_dd_96, "dd 96");
test_opcode!(op_dd_97, "dd 97");
test_opcode!(op_dd_98, "dd 98");
test_opcode!(op_dd_99, "dd 99");
test_opcode!(op_dd_9a, "dd 9a");
test_opcode!(op_dd_9b, "dd 9b");
test_opcode!(op_dd_9c, "dd 9c");
test_opcode!(op_dd_9d, "dd 9d");
test_opcode!(op_dd_9e, "dd 9e");
test_opcode!(op_dd_9f, "dd 9f");
test_opcode!(op_dd_a0, "dd a0");
test_opcode!(op_dd_a1, "dd a1");
test_opcode!(op_dd_a2, "dd a2");
test_opcode!(op_dd_a3, "dd a3");
test_opcode!(op_dd_a4, "dd a4");
test_opcode!(op_dd_a5, "dd a5");
test_opcode!(op_dd_a6, "dd a6");
test_opcode!(op_dd_a7, "dd a7");
test_opcode!(op_dd_a8, "dd a8");
test_opcode!(op_dd_a9, "dd a9");
test_opcode!(op_dd_aa, "dd aa");
test_opcode!(op_dd_ab, "dd ab");
test_opcode!(op_dd_ac, "dd ac");
test_opcode!(op_dd_ad, "dd ad");
test_opcode!(op_dd_ae, "dd ae");
test_opcode!(op_dd_af, "dd af");
test_opcode!(op_dd_b0, "dd b0");
test_opcode!(op_dd_b1, "dd b1");
test_opcode!(op_dd_b2, "dd b2");
test_opcode!(op_dd_b3, "dd b3");
test_opcode!(op_dd_b4, "dd b4");
test_opcode!(op_dd_b5, "dd b5");
test_opcode!(op_dd_b6, "dd b6");
test_opcode!(op_dd_b7, "dd b7");
test_opcode!(op_dd_b8, "dd b8");
test_opcode!(op_dd_b9, "dd b9");
test_opcode!(op_dd_ba, "dd ba");
test_opcode!(op_dd_bb, "dd bb");
test_opcode!(op_dd_bc, "dd bc");
test_opcode!(op_dd_bd, "dd bd");
test_opcode!(op_dd_be, "dd be");
test_opcode!(op_dd_bf, "dd bf");
test_opcode!(op_dd_c0, "dd c0");
test_opcode!(op_dd_c1, "dd c1");
test_opcode!(op_dd_c2, "dd c2");
test_opcode!(op_dd_c3, "dd c3");
test_opcode!(op_dd_c4, "dd c4");
test_opcode!(op_dd_c5, "dd c5");
test_opcode!(op_dd_c6, "dd c6");
test_opcode!(op_dd_c7, "dd c7");
test_opcode!(op_dd_c8, "dd c8");
test_opcode!(op_dd_c9, "dd c9");
test_opcode!(op_dd_ca, "dd ca");
test_opcode!(op_dd_cb____00, "dd cb __ 00");
test_opcode!(op_dd_cb____01, "dd cb __ 01");
test_opcode!(op_dd_cb____02, "dd cb __ 02");
test_opcode!(op_dd_cb____03, "dd cb __ 03");
test_opcode!(op_dd_cb____04, "dd cb __ 04");
test_opcode!(op_dd_cb____05, "dd cb __ 05");
test_opcode!(op_dd_cb____06, "dd cb __ 06");
test_opcode!(op_dd_cb____07, "dd cb __ 07");
test_opcode!(op_dd_cb____08, "dd cb __ 08");
test_opcode!(op_dd_cb____09, "dd cb __ 09");
test_opcode!(op_dd_cb____0a, "dd cb __ 0a");
test_opcode!(op_dd_cb____0b, "dd cb __ 0b");
test_opcode!(op_dd_cb____0c, "dd cb __ 0c");
test_opcode!(op_dd_cb____0d, "dd cb __ 0d");
test_opcode!(op_dd_cb____0e, "dd cb __ 0e");
test_opcode!(op_dd_cb____0f, "dd cb __ 0f");
test_opcode!(op_dd_cb____10, "dd cb __ 10");
test_opcode!(op_dd_cb____11, "dd cb __ 11");
test_opcode!(op_dd_cb____12, "dd cb __ 12");
test_opcode!(op_dd_cb____13, "dd cb __ 13");
test_opcode!(op_dd_cb____14, "dd cb __ 14");
test_opcode!(op_dd_cb____15, "dd cb __ 15");
test_opcode!(op_dd_cb____16, "dd cb __ 16");
test_opcode!(op_dd_cb____17, "dd cb __ 17");
test_opcode!(op_dd_cb____18, "dd cb __ 18");
test_opcode!(op_dd_cb____19, "dd cb __ 19");
test_opcode!(op_dd_cb____1a, "dd cb __ 1a");
test_opcode!(op_dd_cb____1b, "dd cb __ 1b");
test_opcode!(op_dd_cb____1c, "dd cb __ 1c");
test_opcode!(op_dd_cb____1d, "dd cb __ 1d");
test_opcode!(op_dd_cb____1e, "dd cb __ 1e");
test_opcode!(op_dd_cb____1f, "dd cb __ 1f");
test_opcode!(op_dd_cb____20, "dd cb __ 20");
test_opcode!(op_dd_cb____21, "dd cb __ 21");
test_opcode!(op_dd_cb____22, "dd cb __ 22");
test_opcode!(op_dd_cb____23, "dd cb __ 23");
test_opcode!(op_dd_cb____24, "dd cb __ 24");
test_opcode!(op_dd_cb____25, "dd cb __ 25");
test_opcode!(op_dd_cb____26, "dd cb __ 26");
test_opcode!(op_dd_cb____27, "dd cb __ 27");
test_opcode!(op_dd_cb____28, "dd cb __ 28");
test_opcode!(op_dd_cb____29, "dd cb __ 29");
test_opcode!(op_dd_cb____2a, "dd cb __ 2a");
test_opcode!(op_dd_cb____2b, "dd cb __ 2b");
test_opcode!(op_dd_cb____2c, "dd cb __ 2c");
test_opcode!(op_dd_cb____2d, "dd cb __ 2d");
test_opcode!(op_dd_cb____2e, "dd cb __ 2e");
test_opcode!(op_dd_cb____2f, "dd cb __ 2f");
test_opcode!(op_dd_cb____30, "dd cb __ 30");
test_opcode!(op_dd_cb____31, "dd cb __ 31");
test_opcode!(op_dd_cb____32, "dd cb __ 32");
test_opcode!(op_dd_cb____33, "dd cb __ 33");
test_opcode!(op_dd_cb____34, "dd cb __ 34");
test_opcode!(op_dd_cb____35, "dd cb __ 35");
test_opcode!(op_dd_cb____36, "dd cb __ 36");
test_opcode!(op_dd_cb____37, "dd cb __ 37");
test_opcode!(op_dd_cb____38, "dd cb __ 38");
test_opcode!(op_dd_cb____39, "dd cb __ 39");
test_opcode!(op_dd_cb____3a, "dd cb __ 3a");
test_opcode!(op_dd_cb____3b, "dd cb __ 3b");
test_opcode!(op_dd_cb____3c, "dd cb __ 3c");
test_opcode!(op_dd_cb____3d, "dd cb __ 3d");
test_opcode!(op_dd_cb____3e, "dd cb __ 3e");
test_opcode!(op_dd_cb____3f, "dd cb __ 3f");
test_opcode!(op_dd_cb____40, "dd cb __ 40");
test_opcode!(op_dd_cb____41, "dd cb __ 41");
test_opcode!(op_dd_cb____42, "dd cb __ 42");
test_opcode!(op_dd_cb____43, "dd cb __ 43");
test_opcode!(op_dd_cb____44, "dd cb __ 44");
test_opcode!(op_dd_cb____45, "dd cb __ 45");
test_opcode!(op_dd_cb____46, "dd cb __ 46");
test_opcode!(op_dd_cb____47, "dd cb __ 47");
test_opcode!(op_dd_cb____48, "dd cb __ 48");
test_opcode!(op_dd_cb____49, "dd cb __ 49");
test_opcode!(op_dd_cb____4a, "dd cb __ 4a");
test_opcode!(op_dd_cb____4b, "dd cb __ 4b");
test_opcode!(op_dd_cb____4c, "dd cb __ 4c");
test_opcode!(op_dd_cb____4d, "dd cb __ 4d");
test_opcode!(op_dd_cb____4e, "dd cb __ 4e");
test_opcode!(op_dd_cb____4f, "dd cb __ 4f");
test_opcode!(op_dd_cb____50, "dd cb __ 50");
test_opcode!(op_dd_cb____51, "dd cb __ 51");
test_opcode!(op_dd_cb____52, "dd cb __ 52");
test_opcode!(op_dd_cb____53, "dd cb __ 53");
test_opcode!(op_dd_cb____54, "dd cb __ 54");
test_opcode!(op_dd_cb____55, "dd cb __ 55");
test_opcode!(op_dd_cb____56, "dd cb __ 56");
test_opcode!(op_dd_cb____57, "dd cb __ 57");
test_opcode!(op_dd_cb____58, "dd cb __ 58");
test_opcode!(op_dd_cb____59, "dd cb __ 59");
test_opcode!(op_dd_cb____5a, "dd cb __ 5a");
test_opcode!(op_dd_cb____5b, "dd cb __ 5b");
test_opcode!(op_dd_cb____5c, "dd cb __ 5c");
test_opcode!(op_dd_cb____5d, "dd cb __ 5d");
test_opcode!(op_dd_cb____5e, "dd cb __ 5e");
test_opcode!(op_dd_cb____5f, "dd cb __ 5f");
test_opcode!(op_dd_cb____60, "dd cb __ 60");
test_opcode!(op_dd_cb____61, "dd cb __ 61");
test_opcode!(op_dd_cb____62, "dd cb __ 62");
test_opcode!(op_dd_cb____63, "dd cb __ 63");
test_opcode!(op_dd_cb____64, "dd cb __ 64");
test_opcode!(op_dd_cb____65, "dd cb __ 65");
test_opcode!(op_dd_cb____66, "dd cb __ 66");
test_opcode!(op_dd_cb____67, "dd cb __ 67");
test_opcode!(op_dd_cb____68, "dd cb __ 68");
test_opcode!(op_dd_cb____69, "dd cb __ 69");
test_opcode!(op_dd_cb____6a, "dd cb __ 6a");
test_opcode!(op_dd_cb____6b, "dd cb __ 6b");
test_opcode!(op_dd_cb____6c, "dd cb __ 6c");
test_opcode!(op_dd_cb____6d, "dd cb __ 6d");
test_opcode!(op_dd_cb____6e, "dd cb __ 6e");
test_opcode!(op_dd_cb____6f, "dd cb __ 6f");
test_opcode!(op_dd_cb____70, "dd cb __ 70");
test_opcode!(op_dd_cb____71, "dd cb __ 71");
test_opcode!(op_dd_cb____72, "dd cb __ 72");
test_opcode!(op_dd_cb____73, "dd cb __ 73");
test_opcode!(op_dd_cb____74, "dd cb __ 74");
test_opcode!(op_dd_cb____75, "dd cb __ 75");
test_opcode!(op_dd_cb____76, "dd cb __ 76");
test_opcode!(op_dd_cb____77, "dd cb __ 77");
test_opcode!(op_dd_cb____78, "dd cb __ 78");
test_opcode!(op_dd_cb____79, "dd cb __ 79");
test_opcode!(op_dd_cb____7a, "dd cb __ 7a");
test_opcode!(op_dd_cb____7b, "dd cb __ 7b");
test_opcode!(op_dd_cb____7c, "dd cb __ 7c");
test_opcode!(op_dd_cb____7d, "dd cb __ 7d");
test_opcode!(op_dd_cb____7e, "dd cb __ 7e");
test_opcode!(op_dd_cb____7f, "dd cb __ 7f");
test_opcode!(op_dd_cb____80, "dd cb __ 80");
test_opcode!(op_dd_cb____81, "dd cb __ 81");
test_opcode!(op_dd_cb____82, "dd cb __ 82");
test_opcode!(op_dd_cb____83, "dd cb __ 83");
test_opcode!(op_dd_cb____84, "dd cb __ 84");
test_opcode!(op_dd_cb____85, "dd cb __ 85");
test_opcode!(op_dd_cb____86, "dd cb __ 86");
test_opcode!(op_dd_cb____87, "dd cb __ 87");
test_opcode!(op_dd_cb____88, "dd cb __ 88");
test_opcode!(op_dd_cb____89, "dd cb __ 89");
test_opcode!(op_dd_cb____8a, "dd cb __ 8a");
test_opcode!(op_dd_cb____8b, "dd cb __ 8b");
test_opcode!(op_dd_cb____8c, "dd cb __ 8c");
test_opcode!(op_dd_cb____8d, "dd cb __ 8d");
test_opcode!(op_dd_cb____8e, "dd cb __ 8e");
test_opcode!(op_dd_cb____8f, "dd cb __ 8f");
test_opcode!(op_dd_cb____90, "dd cb __ 90");
test_opcode!(op_dd_cb____91, "dd cb __ 91");
test_opcode!(op_dd_cb____92, "dd cb __ 92");
test_opcode!(op_dd_cb____93, "dd cb __ 93");
test_opcode!(op_dd_cb____94, "dd cb __ 94");
test_opcode!(op_dd_cb____95, "dd cb __ 95");
test_opcode!(op_dd_cb____96, "dd cb __ 96");
test_opcode!(op_dd_cb____97, "dd cb __ 97");
test_opcode!(op_dd_cb____98, "dd cb __ 98");
test_opcode!(op_dd_cb____99, "dd cb __ 99");
test_opcode!(op_dd_cb____9a, "dd cb __ 9a");
test_opcode!(op_dd_cb____9b, "dd cb __ 9b");
test_opcode!(op_dd_cb____9c, "dd cb __ 9c");
test_opcode!(op_dd_cb____9d, "dd cb __ 9d");
test_opcode!(op_dd_cb____9e, "dd cb __ 9e");
test_opcode!(op_dd_cb____9f, "dd cb __ 9f");
test_opcode!(op_dd_cb____a0, "dd cb __ a0");
test_opcode!(op_dd_cb____a1, "dd cb __ a1");
test_opcode!(op_dd_cb____a2, "dd cb __ a2");
test_opcode!(op_dd_cb____a3, "dd cb __ a3");
test_opcode!(op_dd_cb____a4, "dd cb __ a4");
test_opcode!(op_dd_cb____a5, "dd cb __ a5");
test_opcode!(op_dd_cb____a6, "dd cb __ a6");
test_opcode!(op_dd_cb____a7, "dd cb __ a7");
test_opcode!(op_dd_cb____a8, "dd cb __ a8");
test_opcode!(op_dd_cb____a9, "dd cb __ a9");
test_opcode!(op_dd_cb____aa, "dd cb __ aa");
test_opcode!(op_dd_cb____ab, "dd cb __ ab");
test_opcode!(op_dd_cb____ac, "dd cb __ ac");
test_opcode!(op_dd_cb____ad, "dd cb __ ad");
test_opcode!(op_dd_cb____ae, "dd cb __ ae");
test_opcode!(op_dd_cb____af, "dd cb __ af");
test_opcode!(op_dd_cb____b0, "dd cb __ b0");
test_opcode!(op_dd_cb____b1, "dd cb __ b1");
test_opcode!(op_dd_cb____b2, "dd cb __ b2");
test_opcode!(op_dd_cb____b3, "dd cb __ b3");
test_opcode!(op_dd_cb____b4, "dd cb __ b4");
test_opcode!(op_dd_cb____b5, "dd cb __ b5");
test_opcode!(op_dd_cb____b6, "dd cb __ b6");
test_opcode!(op_dd_cb____b7, "dd cb __ b7");
test_opcode!(op_dd_cb____b8, "dd cb __ b8");
test_opcode!(op_dd_cb____b9, "dd cb __ b9");
test_opcode!(op_dd_cb____ba, "dd cb __ ba");
test_opcode!(op_dd_cb____bb, "dd cb __ bb");
test_opcode!(op_dd_cb____bc, "dd cb __ bc");
test_opcode!(op_dd_cb____bd, "dd cb __ bd");
test_opcode!(op_dd_cb____be, "dd cb __ be");
test_opcode!(op_dd_cb____bf, "dd cb __ bf");
test_opcode!(op_dd_cb____c0, "dd cb __ c0");
test_opcode!(op_dd_cb____c1, "dd cb __ c1");
test_opcode!(op_dd_cb____c2, "dd cb __ c2");
test_opcode!(op_dd_cb____c3, "dd cb __ c3");
test_opcode!(op_dd_cb____c4, "dd cb __ c4");
test_opcode!(op_dd_cb____c5, "dd cb __ c5");
test_opcode!(op_dd_cb____c6, "dd cb __ c6");
test_opcode!(op_dd_cb____c7, "dd cb __ c7");
test_opcode!(op_dd_cb____c8, "dd cb __ c8");
test_opcode!(op_dd_cb____c9, "dd cb __ c9");
test_opcode!(op_dd_cb____ca, "dd cb __ ca");
test_opcode!(op_dd_cb____cb, "dd cb __ cb");
test_opcode!(op_dd_cb____cc, "dd cb __ cc");
test_opcode!(op_dd_cb____cd, "dd cb __ cd");
test_opcode!(op_dd_cb____ce, "dd cb __ ce");
test_opcode!(op_dd_cb____cf, "dd cb __ cf");
test_opcode!(op_dd_cb____d0, "dd cb __ d0");
test_opcode!(op_dd_cb____d1, "dd cb __ d1");
test_opcode!(op_dd_cb____d2, "dd cb __ d2");
test_opcode!(op_dd_cb____d3, "dd cb __ d3");
test_opcode!(op_dd_cb____d4, "dd cb __ d4");
test_opcode!(op_dd_cb____d5, "dd cb __ d5");
test_opcode!(op_dd_cb____d6, "dd cb __ d6");
test_opcode!(op_dd_cb____d7, "dd cb __ d7");
test_opcode!(op_dd_cb____d8, "dd cb __ d8");
test_opcode!(op_dd_cb____d9, "dd cb __ d9");
test_opcode!(op_dd_cb____da, "dd cb __ da");
test_opcode!(op_dd_cb____db, "dd cb __ db");
test_opcode!(op_dd_cb____dc, "dd cb __ dc");
test_opcode!(op_dd_cb____dd, "dd cb __ dd");
test_opcode!(op_dd_cb____de, "dd cb __ de");
test_opcode!(op_dd_cb____df, "dd cb __ df");
test_opcode!(op_dd_cb____e0, "dd cb __ e0");
test_opcode!(op_dd_cb____e1, "dd cb __ e1");
test_opcode!(op_dd_cb____e2, "dd cb __ e2");
test_opcode!(op_dd_cb____e3, "dd cb __ e3");
test_opcode!(op_dd_cb____e4, "dd cb __ e4");
test_opcode!(op_dd_cb____e5, "dd cb __ e5");
test_opcode!(op_dd_cb____e6, "dd cb __ e6");
test_opcode!(op_dd_cb____e7, "dd cb __ e7");
test_opcode!(op_dd_cb____e8, "dd cb __ e8");
test_opcode!(op_dd_cb____e9, "dd cb __ e9");
test_opcode!(op_dd_cb____ea, "dd cb __ ea");
test_opcode!(op_dd_cb____eb, "dd cb __ eb");
test_opcode!(op_dd_cb____ec, "dd cb __ ec");
test_opcode!(op_dd_cb____ed, "dd cb __ ed");
test_opcode!(op_dd_cb____ee, "dd cb __ ee");
test_opcode!(op_dd_cb____ef, "dd cb __ ef");
test_opcode!(op_dd_cb____f0, "dd cb __ f0");
test_opcode!(op_dd_cb____f1, "dd cb __ f1");
test_opcode!(op_dd_cb____f2, "dd cb __ f2");
test_opcode!(op_dd_cb____f3, "dd cb __ f3");
test_opcode!(op_dd_cb____f4, "dd cb __ f4");
test_opcode!(op_dd_cb____f5, "dd cb __ f5");
test_opcode!(op_dd_cb____f6, "dd cb __ f6");
test_opcode!(op_dd_cb____f7, "dd cb __ f7");
test_opcode!(op_dd_cb____f8, "dd cb __ f8");
test_opcode!(op_dd_cb____f9, "dd cb __ f9");
test_opcode!(op_dd_cb____fa, "dd cb __ fa");
test_opcode!(op_dd_cb____fb, "dd cb __ fb");
test_opcode!(op_dd_cb____fc, "dd cb __ fc");
test_opcode!(op_dd_cb____fd, "dd cb __ fd");
test_opcode!(op_dd_cb____fe, "dd cb __ fe");
test_opcode!(op_dd_cb____ff, "dd cb __ ff");
test_opcode!(op_dd_cc, "dd cc");
test_opcode!(op_dd_cd, "dd cd");
test_opcode!(op_dd_ce, "dd ce");
test_opcode!(op_dd_cf, "dd cf");
test_opcode!(op_dd_d0, "dd d0");
test_opcode!(op_dd_d1, "dd d1");
test_opcode!(op_dd_d2, "dd d2");
test_opcode!(op_dd_d3, "dd d3");
test_opcode!(op_dd_d4, "dd d4");
test_opcode!(op_dd_d5, "dd d5");
test_opcode!(op_dd_d6, "dd d6");
test_opcode!(op_dd_d7, "dd d7");
test_opcode!(op_dd_d8, "dd d8");
test_opcode!(op_dd_d9, "dd d9");
test_opcode!(op_dd_da, "dd da");
test_opcode!(op_dd_db, "dd db");
test_opcode!(op_dd_dc, "dd dc");
test_opcode!(op_dd_de, "dd de");
test_opcode!(op_dd_df, "dd df");
test_opcode!(op_dd_e0, "dd e0");
test_opcode!(op_dd_e1, "dd e1");
test_opcode!(op_dd_e2, "dd e2");
test_opcode!(op_dd_e3, "dd e3");
test_opcode!(op_dd_e4, "dd e4");
test_opcode!(op_dd_e5, "dd e5");
test_opcode!(op_dd_e6, "dd e6");
test_opcode!(op_dd_e7, "dd e7");
test_opcode!(op_dd_e8, "dd e8");
test_opcode!(op_dd_e9, "dd e9");
test_opcode!(op_dd_ea, "dd ea");
test_opcode!(op_dd_eb, "dd eb");
test_opcode!(op_dd_ec, "dd ec");
test_opcode!(op_dd_ee, "dd ee");
test_opcode!(op_dd_ef, "dd ef");
test_opcode!(op_dd_f0, "dd f0");
test_opcode!(op_dd_f1, "dd f1");
test_opcode!(op_dd_f2, "dd f2");
test_opcode!(op_dd_f3, "dd f3");
test_opcode!(op_dd_f4, "dd f4");
test_opcode!(op_dd_f5, "dd f5");
test_opcode!(op_dd_f6, "dd f6");
test_opcode!(op_dd_f7, "dd f7");
test_opcode!(op_dd_f8, "dd f8");
test_opcode!(op_dd_f9, "dd f9");
test_opcode!(op_dd_fa, "dd fa");
test_opcode!(op_dd_fb, "dd fb");
test_opcode!(op_dd_fc, "dd fc");
test_opcode!(op_dd_fe, "dd fe");
test_opcode!(op_dd_ff, "dd ff");
test_opcode!(op_de, "de");
test_opcode!(op_df, "df");
test_opcode!(op_e0, "e0");
test_opcode!(op_e1, "e1");
test_opcode!(op_e2, "e2");
test_opcode!(op_e3, "e3");
test_opcode!(op_e4, "e4");
test_opcode!(op_e5, "e5");
test_opcode!(op_e6, "e6");
test_opcode!(op_e7, "e7");
test_opcode!(op_e8, "e8");
test_opcode!(op_e9, "e9");
test_opcode!(op_ea, "ea");
test_opcode!(op_eb, "eb");
test_opcode!(op_ec, "ec");
test_opcode!(op_ed_40, "ed 40");
test_opcode!(op_ed_41, "ed 41");
test_opcode!(op_ed_42, "ed 42");
test_opcode!(op_ed_43, "ed 43");
test_opcode!(op_ed_44, "ed 44");
test_opcode!(op_ed_45, "ed 45");
test_opcode!(op_ed_46, "ed 46");
test_opcode!(op_ed_47, "ed 47");
test_opcode!(op_ed_48, "ed 48");
test_opcode!(op_ed_49, "ed 49");
test_opcode!(op_ed_4a, "ed 4a");
test_opcode!(op_ed_4b, "ed 4b");
test_opcode!(op_ed_4c, "ed 4c");
test_opcode!(op_ed_4d, "ed 4d");
test_opcode!(op_ed_4e, "ed 4e");
test_opcode!(op_ed_4f, "ed 4f");
test_opcode!(op_ed_50, "ed 50");
test_opcode!(op_ed_51, "ed 51");
test_opcode!(op_ed_52, "ed 52");
test_opcode!(op_ed_53, "ed 53");
test_opcode!(op_ed_54, "ed 54");
test_opcode!(op_ed_55, "ed 55");
test_opcode!(op_ed_56, "ed 56");
test_opcode!(op_ed_57, "ed 57");
test_opcode!(op_ed_58, "ed 58");
test_opcode!(op_ed_59, "ed 59");
test_opcode!(op_ed_5a, "ed 5a");
test_opcode!(op_ed_5b, "ed 5b");
test_opcode!(op_ed_5c, "ed 5c");
test_opcode!(op_ed_5d, "ed 5d");
test_opcode!(op_ed_5e, "ed 5e");
test_opcode!(op_ed_5f, "ed 5f");
test_opcode!(op_ed_60, "ed 60");
test_opcode!(op_ed_61, "ed 61");
test_opcode!(op_ed_62, "ed 62");
test_opcode!(op_ed_63, "ed 63");
test_opcode!(op_ed_64, "ed 64");
test_opcode!(op_ed_65, "ed 65");
test_opcode!(op_ed_66, "ed 66");
test_opcode!(op_ed_67, "ed 67");
test_opcode!(op_ed_68, "ed 68");
test_opcode!(op_ed_69, "ed 69");
test_opcode!(op_ed_6a, "ed 6a");
test_opcode!(op_ed_6b, "ed 6b");
test_opcode!(op_ed_6c, "ed 6c");
test_opcode!(op_ed_6d, "ed 6d");
test_opcode!(op_ed_6e, "ed 6e");
test_opcode!(op_ed_6f, "ed 6f");
test_opcode!(op_ed_70, "ed 70");
test_opcode!(op_ed_71, "ed 71");
test_opcode!(op_ed_72, "ed 72");
test_opcode!(op_ed_73, "ed 73");
test_opcode!(op_ed_74, "ed 74");
test_opcode!(op_ed_75, "ed 75");
test_opcode!(op_ed_76, "ed 76");
test_opcode!(op_ed_77, "ed 77");
test_opcode!(op_ed_78, "ed 78");
test_opcode!(op_ed_79, "ed 79");
test_opcode!(op_ed_7a, "ed 7a");
test_opcode!(op_ed_7b, "ed 7b");
test_opcode!(op_ed_7c, "ed 7c");
test_opcode!(op_ed_7d, "ed 7d");
test_opcode!(op_ed_7e, "ed 7e");
test_opcode!(op_ed_7f, "ed 7f");
test_opcode!(op_ed_a0, "ed a0");
test_opcode!(op_ed_a1, "ed a1");
test_opcode!(op_ed_a2, "ed a2");
test_opcode!(op_ed_a3, "ed a3");
test_opcode!(op_ed_a8, "ed a8");
test_opcode!(op_ed_a9, "ed a9");
test_opcode!(op_ed_aa, "ed aa");
test_opcode!(op_ed_ab, "ed ab");
test_opcode!(op_ed_b0, "ed b0");
test_opcode!(op_ed_b1, "ed b1");
test_opcode!(op_ed_b2, "ed b2");
test_opcode!(op_ed_b3, "ed b3");
test_opcode!(op_ed_b8, "ed b8");
test_opcode!(op_ed_b9, "ed b9");
test_opcode!(op_ed_ba, "ed ba");
test_opcode!(op_ed_bb, "ed bb");
test_opcode!(op_ee, "ee");
test_opcode!(op_ef, "ef");
test_opcode!(op_f0, "f0");
test_opcode!(op_f1, "f1");
test_opcode!(op_f2, "f2");
test_opcode!(op_f3, "f3");
test_opcode!(op_f4, "f4");
test_opcode!(op_f5, "f5");
test_opcode!(op_f6, "f6");
test_opcode!(op_f7, "f7");
test_opcode!(op_f8, "f8");
test_opcode!(op_f9, "f9");
test_opcode!(op_fa, "fa");
test_opcode!(op_fb, "fb");
test_opcode!(op_fc, "fc");
test_opcode!(op_fd_00, "fd 00");
test_opcode!(op_fd_01, "fd 01");
test_opcode!(op_fd_02, "fd 02");
test_opcode!(op_fd_03, "fd 03");
test_opcode!(op_fd_04, "fd 04");
test_opcode!(op_fd_05, "fd 05");
test_opcode!(op_fd_06, "fd 06");
test_opcode!(op_fd_07, "fd 07");
test_opcode!(op_fd_08, "fd 08");
test_opcode!(op_fd_09, "fd 09");
test_opcode!(op_fd_0a, "fd 0a");
test_opcode!(op_fd_0b, "fd 0b");
test_opcode!(op_fd_0c, "fd 0c");
test_opcode!(op_fd_0d, "fd 0d");
test_opcode!(op_fd_0e, "fd 0e");
test_opcode!(op_fd_0f, "fd 0f");
test_opcode!(op_fd_10, "fd 10");
test_opcode!(op_fd_11, "fd 11");
test_opcode!(op_fd_12, "fd 12");
test_opcode!(op_fd_13, "fd 13");
test_opcode!(op_fd_14, "fd 14");
test_opcode!(op_fd_15, "fd 15");
test_opcode!(op_fd_16, "fd 16");
test_opcode!(op_fd_17, "fd 17");
test_opcode!(op_fd_18, "fd 18");
test_opcode!(op_fd_19, "fd 19");
test_opcode!(op_fd_1a, "fd 1a");
test_opcode!(op_fd_1b, "fd 1b");
test_opcode!(op_fd_1c, "fd 1c");
test_opcode!(op_fd_1d, "fd 1d");
test_opcode!(op_fd_1e, "fd 1e");
test_opcode!(op_fd_1f, "fd 1f");
test_opcode!(op_fd_20, "fd 20");
test_opcode!(op_fd_21, "fd 21");
test_opcode!(op_fd_22, "fd 22");
test_opcode!(op_fd_23, "fd 23");
test_opcode!(op_fd_24, "fd 24");
test_opcode!(op_fd_25, "fd 25");
test_opcode!(op_fd_26, "fd 26");
test_opcode!(op_fd_27, "fd 27");
test_opcode!(op_fd_28, "fd 28");
test_opcode!(op_fd_29, "fd 29");
test_opcode!(op_fd_2a, "fd 2a");
test_opcode!(op_fd_2b, "fd 2b");
test_opcode!(op_fd_2c, "fd 2c");
test_opcode!(op_fd_2d, "fd 2d");
test_opcode!(op_fd_2e, "fd 2e");
test_opcode!(op_fd_2f, "fd 2f");
test_opcode!(op_fd_30, "fd 30");
test_opcode!(op_fd_31, "fd 31");
test_opcode!(op_fd_32, "fd 32");
test_opcode!(op_fd_33, "fd 33");
test_opcode!(op_fd_34, "fd 34");
test_opcode!(op_fd_35, "fd 35");
test_opcode!(op_fd_36, "fd 36");
test_opcode!(op_fd_37, "fd 37");
test_opcode!(op_fd_38, "fd 38");
test_opcode!(op_fd_39, "fd 39");
test_opcode!(op_fd_3a, "fd 3a");
test_opcode!(op_fd_3b, "fd 3b");
test_opcode!(op_fd_3c, "fd 3c");
test_opcode!(op_fd_3d, "fd 3d");
test_opcode!(op_fd_3e, "fd 3e");
test_opcode!(op_fd_3f, "fd 3f");
test_opcode!(op_fd_40, "fd 40");
test_opcode!(op_fd_41, "fd 41");
test_opcode!(op_fd_42, "fd 42");
test_opcode!(op_fd_43, "fd 43");
test_opcode!(op_fd_44, "fd 44");
test_opcode!(op_fd_45, "fd 45");
test_opcode!(op_fd_46, "fd 46");
test_opcode!(op_fd_47, "fd 47");
test_opcode!(op_fd_48, "fd 48");
test_opcode!(op_fd_49, "fd 49");
test_opcode!(op_fd_4a, "fd 4a");
test_opcode!(op_fd_4b, "fd 4b");
test_opcode!(op_fd_4c, "fd 4c");
test_opcode!(op_fd_4d, "fd 4d");
test_opcode!(op_fd_4e, "fd 4e");
test_opcode!(op_fd_4f, "fd 4f");
test_opcode!(op_fd_50, "fd 50");
test_opcode!(op_fd_51, "fd 51");
test_opcode!(op_fd_52, "fd 52");
test_opcode!(op_fd_53, "fd 53");
test_opcode!(op_fd_54, "fd 54");
test_opcode!(op_fd_55, "fd 55");
test_opcode!(op_fd_56, "fd 56");
test_opcode!(op_fd_57, "fd 57");
test_opcode!(op_fd_58, "fd 58");
test_opcode!(op_fd_59, "fd 59");
test_opcode!(op_fd_5a, "fd 5a");
test_opcode!(op_fd_5b, "fd 5b");
test_opcode!(op_fd_5c, "fd 5c");
test_opcode!(op_fd_5d, "fd 5d");
test_opcode!(op_fd_5e, "fd 5e");
test_opcode!(op_fd_5f, "fd 5f");
test_opcode!(op_fd_60, "fd 60");
test_opcode!(op_fd_61, "fd 61");
test_opcode!(op_fd_62, "fd 62");
test_opcode!(op_fd_63, "fd 63");
test_opcode!(op_fd_64, "fd 64");
test_opcode!(op_fd_65, "fd 65");
test_opcode!(op_fd_66, "fd 66");
test_opcode!(op_fd_67, "fd 67");
test_opcode!(op_fd_68, "fd 68");
test_opcode!(op_fd_69, "fd 69");
test_opcode!(op_fd_6a, "fd 6a");
test_opcode!(op_fd_6b, "fd 6b");
test_opcode!(op_fd_6c, "fd 6c");
test_opcode!(op_fd_6d, "fd 6d");
test_opcode!(op_fd_6e, "fd 6e");
test_opcode!(op_fd_6f, "fd 6f");
test_opcode!(op_fd_70, "fd 70");
test_opcode!(op_fd_71, "fd 71");
test_opcode!(op_fd_72, "fd 72");
test_opcode!(op_fd_73, "fd 73");
test_opcode!(op_fd_74, "fd 74");
test_opcode!(op_fd_75, "fd 75");
test_opcode!(op_fd_76, "fd 76");
test_opcode!(op_fd_77, "fd 77");
test_opcode!(op_fd_78, "fd 78");
test_opcode!(op_fd_79, "fd 79");
test_opcode!(op_fd_7a, "fd 7a");
test_opcode!(op_fd_7b, "fd 7b");
test_opcode!(op_fd_7c, "fd 7c");
test_opcode!(op_fd_7d, "fd 7d");
test_opcode!(op_fd_7e, "fd 7e");
test_opcode!(op_fd_7f, "fd 7f");
test_opcode!(op_fd_80, "fd 80");
test_opcode!(op_fd_81, "fd 81");
test_opcode!(op_fd_82, "fd 82");
test_opcode!(op_fd_83, "fd 83");
test_opcode!(op_fd_84, "fd 84");
test_opcode!(op_fd_85, "fd 85");
test_opcode!(op_fd_86, "fd 86");
test_opcode!(op_fd_87, "fd 87");
test_opcode!(op_fd_88, "fd 88");
test_opcode!(op_fd_89, "fd 89");
test_opcode!(op_fd_8a, "fd 8a");
test_opcode!(op_fd_8b, "fd 8b");
test_opcode!(op_fd_8c, "fd 8c");
test_opcode!(op_fd_8d, "fd 8d");
test_opcode!(op_fd_8e, "fd 8e");
test_opcode!(op_fd_8f, "fd 8f");
test_opcode!(op_fd_90, "fd 90");
test_opcode!(op_fd_91, "fd 91");
test_opcode!(op_fd_92, "fd 92");
test_opcode!(op_fd_93, "fd 93");
test_opcode!(op_fd_94, "fd 94");
test_opcode!(op_fd_95, "fd 95");
test_opcode!(op_fd_96, "fd 96");
test_opcode!(op_fd_97, "fd 97");
test_opcode!(op_fd_98, "fd 98");
test_opcode!(op_fd_99, "fd 99");
test_opcode!(op_fd_9a, "fd 9a");
test_opcode!(op_fd_9b, "fd 9b");
test_opcode!(op_fd_9c, "fd 9c");
test_opcode!(op_fd_9d, "fd 9d");
test_opcode!(op_fd_9e, "fd 9e");
test_opcode!(op_fd_9f, "fd 9f");
test_opcode!(op_fd_a0, "fd a0");
test_opcode!(op_fd_a1, "fd a1");
test_opcode!(op_fd_a2, "fd a2");
test_opcode!(op_fd_a3, "fd a3");
test_opcode!(op_fd_a4, "fd a4");
test_opcode!(op_fd_a5, "fd a5");
test_opcode!(op_fd_a6, "fd a6");
test_opcode!(op_fd_a7, "fd a7");
test_opcode!(op_fd_a8, "fd a8");
test_opcode!(op_fd_a9, "fd a9");
test_opcode!(op_fd_aa, "fd aa");
test_opcode!(op_fd_ab, "fd ab");
test_opcode!(op_fd_ac, "fd ac");
test_opcode!(op_fd_ad, "fd ad");
test_opcode!(op_fd_ae, "fd ae");
test_opcode!(op_fd_af, "fd af");
test_opcode!(op_fd_b0, "fd b0");
test_opcode!(op_fd_b1, "fd b1");
test_opcode!(op_fd_b2, "fd b2");
test_opcode!(op_fd_b3, "fd b3");
test_opcode!(op_fd_b4, "fd b4");
test_opcode!(op_fd_b5, "fd b5");
test_opcode!(op_fd_b6, "fd b6");
test_opcode!(op_fd_b7, "fd b7");
test_opcode!(op_fd_b8, "fd b8");
test_opcode!(op_fd_b9, "fd b9");
test_opcode!(op_fd_ba, "fd ba");
test_opcode!(op_fd_bb, "fd bb");
test_opcode!(op_fd_bc, "fd bc");
test_opcode!(op_fd_bd, "fd bd");
test_opcode!(op_fd_be, "fd be");
test_opcode!(op_fd_bf, "fd bf");
test_opcode!(op_fd_c0, "fd c0");
test_opcode!(op_fd_c1, "fd c1");
test_opcode!(op_fd_c2, "fd c2");
test_opcode!(op_fd_c3, "fd c3");
test_opcode!(op_fd_c4, "fd c4");
test_opcode!(op_fd_c5, "fd c5");
test_opcode!(op_fd_c6, "fd c6");
test_opcode!(op_fd_c7, "fd c7");
test_opcode!(op_fd_c8, "fd c8");
test_opcode!(op_fd_c9, "fd c9");
test_opcode!(op_fd_ca, "fd ca");
test_opcode!(op_fd_cb____00, "fd cb __ 00");
test_opcode!(op_fd_cb____01, "fd cb __ 01");
test_opcode!(op_fd_cb____02, "fd cb __ 02");
test_opcode!(op_fd_cb____03, "fd cb __ 03");
test_opcode!(op_fd_cb____04, "fd cb __ 04");
test_opcode!(op_fd_cb____05, "fd cb __ 05");
test_opcode!(op_fd_cb____06, "fd cb __ 06");
test_opcode!(op_fd_cb____07, "fd cb __ 07");
test_opcode!(op_fd_cb____08, "fd cb __ 08");
test_opcode!(op_fd_cb____09, "fd cb __ 09");
test_opcode!(op_fd_cb____0a, "fd cb __ 0a");
test_opcode!(op_fd_cb____0b, "fd cb __ 0b");
test_opcode!(op_fd_cb____0c, "fd cb __ 0c");
test_opcode!(op_fd_cb____0d, "fd cb __ 0d");
test_opcode!(op_fd_cb____0e, "fd cb __ 0e");
test_opcode!(op_fd_cb____0f, "fd cb __ 0f");
test_opcode!(op_fd_cb____10, "fd cb __ 10");
test_opcode!(op_fd_cb____11, "fd cb __ 11");
test_opcode!(op_fd_cb____12, "fd cb __ 12");
test_opcode!(op_fd_cb____13, "fd cb __ 13");
test_opcode!(op_fd_cb____14, "fd cb __ 14");
test_opcode!(op_fd_cb____15, "fd cb __ 15");
test_opcode!(op_fd_cb____16, "fd cb __ 16");
test_opcode!(op_fd_cb____17, "fd cb __ 17");
test_opcode!(op_fd_cb____18, "fd cb __ 18");
test_opcode!(op_fd_cb____19, "fd cb __ 19");
test_opcode!(op_fd_cb____1a, "fd cb __ 1a");
test_opcode!(op_fd_cb____1b, "fd cb __ 1b");
test_opcode!(op_fd_cb____1c, "fd cb __ 1c");
test_opcode!(op_fd_cb____1d, "fd cb __ 1d");
test_opcode!(op_fd_cb____1e, "fd cb __ 1e");
test_opcode!(op_fd_cb____1f, "fd cb __ 1f");
test_opcode!(op_fd_cb____20, "fd cb __ 20");
test_opcode!(op_fd_cb____21, "fd cb __ 21");
test_opcode!(op_fd_cb____22, "fd cb __ 22");
test_opcode!(op_fd_cb____23, "fd cb __ 23");
test_opcode!(op_fd_cb____24, "fd cb __ 24");
test_opcode!(op_fd_cb____25, "fd cb __ 25");
test_opcode!(op_fd_cb____26, "fd cb __ 26");
test_opcode!(op_fd_cb____27, "fd cb __ 27");
test_opcode!(op_fd_cb____28, "fd cb __ 28");
test_opcode!(op_fd_cb____29, "fd cb __ 29");
test_opcode!(op_fd_cb____2a, "fd cb __ 2a");
test_opcode!(op_fd_cb____2b, "fd cb __ 2b");
test_opcode!(op_fd_cb____2c, "fd cb __ 2c");
test_opcode!(op_fd_cb____2d, "fd cb __ 2d");
test_opcode!(op_fd_cb____2e, "fd cb __ 2e");
test_opcode!(op_fd_cb____2f, "fd cb __ 2f");
test_opcode!(op_fd_cb____30, "fd cb __ 30");
test_opcode!(op_fd_cb____31, "fd cb __ 31");
test_opcode!(op_fd_cb____32, "fd cb __ 32");
test_opcode!(op_fd_cb____33, "fd cb __ 33");
test_opcode!(op_fd_cb____34, "fd cb __ 34");
test_opcode!(op_fd_cb____35, "fd cb __ 35");
test_opcode!(op_fd_cb____36, "fd cb __ 36");
test_opcode!(op_fd_cb____37, "fd cb __ 37");
test_opcode!(op_fd_cb____38, "fd cb __ 38");
test_opcode!(op_fd_cb____39, "fd cb __ 39");
test_opcode!(op_fd_cb____3a, "fd cb __ 3a");
test_opcode!(op_fd_cb____3b, "fd cb __ 3b");
test_opcode!(op_fd_cb____3c, "fd cb __ 3c");
test_opcode!(op_fd_cb____3d, "fd cb __ 3d");
test_opcode!(op_fd_cb____3e, "fd cb __ 3e");
test_opcode!(op_fd_cb____3f, "fd cb __ 3f");
test_opcode!(op_fd_cb____40, "fd cb __ 40");
test_opcode!(op_fd_cb____41, "fd cb __ 41");
test_opcode!(op_fd_cb____42, "fd cb __ 42");
test_opcode!(op_fd_cb____43, "fd cb __ 43");
test_opcode!(op_fd_cb____44, "fd cb __ 44");
test_opcode!(op_fd_cb____45, "fd cb __ 45");
test_opcode!(op_fd_cb____46, "fd cb __ 46");
test_opcode!(op_fd_cb____47, "fd cb __ 47");
test_opcode!(op_fd_cb____48, "fd cb __ 48");
test_opcode!(op_fd_cb____49, "fd cb __ 49");
test_opcode!(op_fd_cb____4a, "fd cb __ 4a");
test_opcode!(op_fd_cb____4b, "fd cb __ 4b");
test_opcode!(op_fd_cb____4c, "fd cb __ 4c");
test_opcode!(op_fd_cb____4d, "fd cb __ 4d");
test_opcode!(op_fd_cb____4e, "fd cb __ 4e");
test_opcode!(op_fd_cb____4f, "fd cb __ 4f");
test_opcode!(op_fd_cb____50, "fd cb __ 50");
test_opcode!(op_fd_cb____51, "fd cb __ 51");
test_opcode!(op_fd_cb____52, "fd cb __ 52");
test_opcode!(op_fd_cb____53, "fd cb __ 53");
test_opcode!(op_fd_cb____54, "fd cb __ 54");
test_opcode!(op_fd_cb____55, "fd cb __ 55");
test_opcode!(op_fd_cb____56, "fd cb __ 56");
test_opcode!(op_fd_cb____57, "fd cb __ 57");
test_opcode!(op_fd_cb____58, "fd cb __ 58");
test_opcode!(op_fd_cb____59, "fd cb __ 59");
test_opcode!(op_fd_cb____5a, "fd cb __ 5a");
test_opcode!(op_fd_cb____5b, "fd cb __ 5b");
test_opcode!(op_fd_cb____5c, "fd cb __ 5c");
test_opcode!(op_fd_cb____5d, "fd cb __ 5d");
test_opcode!(op_fd_cb____5e, "fd cb __ 5e");
test_opcode!(op_fd_cb____5f, "fd cb __ 5f");
test_opcode!(op_fd_cb____60, "fd cb __ 60");
test_opcode!(op_fd_cb____61, "fd cb __ 61");
test_opcode!(op_fd_cb____62, "fd cb __ 62");
test_opcode!(op_fd_cb____63, "fd cb __ 63");
test_opcode!(op_fd_cb____64, "fd cb __ 64");
test_opcode!(op_fd_cb____65, "fd cb __ 65");
test_opcode!(op_fd_cb____66, "fd cb __ 66");
test_opcode!(op_fd_cb____67, "fd cb __ 67");
test_opcode!(op_fd_cb____68, "fd cb __ 68");
test_opcode!(op_fd_cb____69, "fd cb __ 69");
test_opcode!(op_fd_cb____6a, "fd cb __ 6a");
test_opcode!(op_fd_cb____6b, "fd cb __ 6b");
test_opcode!(op_fd_cb____6c, "fd cb __ 6c");
test_opcode!(op_fd_cb____6d, "fd cb __ 6d");
test_opcode!(op_fd_cb____6e, "fd cb __ 6e");
test_opcode!(op_fd_cb____6f, "fd cb __ 6f");
test_opcode!(op_fd_cb____70, "fd cb __ 70");
test_opcode!(op_fd_cb____71, "fd cb __ 71");
test_opcode!(op_fd_cb____72, "fd cb __ 72");
test_opcode!(op_fd_cb____73, "fd cb __ 73");
test_opcode!(op_fd_cb____74, "fd cb __ 74");
test_opcode!(op_fd_cb____75, "fd cb __ 75");
test_opcode!(op_fd_cb____76, "fd cb __ 76");
test_opcode!(op_fd_cb____77, "fd cb __ 77");
test_opcode!(op_fd_cb____78, "fd cb __ 78");
test_opcode!(op_fd_cb____79, "fd cb __ 79");
test_opcode!(op_fd_cb____7a, "fd cb __ 7a");
test_opcode!(op_fd_cb____7b, "fd cb __ 7b");
test_opcode!(op_fd_cb____7c, "fd cb __ 7c");
test_opcode!(op_fd_cb____7d, "fd cb __ 7d");
test_opcode!(op_fd_cb____7e, "fd cb __ 7e");
test_opcode!(op_fd_cb____7f, "fd cb __ 7f");
test_opcode!(op_fd_cb____80, "fd cb __ 80");
test_opcode!(op_fd_cb____81, "fd cb __ 81");
test_opcode!(op_fd_cb____82, "fd cb __ 82");
test_opcode!(op_fd_cb____83, "fd cb __ 83");
test_opcode!(op_fd_cb____84, "fd cb __ 84");
test_opcode!(op_fd_cb____85, "fd cb __ 85");
test_opcode!(op_fd_cb____86, "fd cb __ 86");
test_opcode!(op_fd_cb____87, "fd cb __ 87");
test_opcode!(op_fd_cb____88, "fd cb __ 88");
test_opcode!(op_fd_cb____89, "fd cb __ 89");
test_opcode!(op_fd_cb____8a, "fd cb __ 8a");
test_opcode!(op_fd_cb____8b, "fd cb __ 8b");
test_opcode!(op_fd_cb____8c, "fd cb __ 8c");
test_opcode!(op_fd_cb____8d, "fd cb __ 8d");
test_opcode!(op_fd_cb____8e, "fd cb __ 8e");
test_opcode!(op_fd_cb____8f, "fd cb __ 8f");
test_opcode!(op_fd_cb____90, "fd cb __ 90");
test_opcode!(op_fd_cb____91, "fd cb __ 91");
test_opcode!(op_fd_cb____92, "fd cb __ 92");
test_opcode!(op_fd_cb____93, "fd cb __ 93");
test_opcode!(op_fd_cb____94, "fd cb __ 94");
test_opcode!(op_fd_cb____95, "fd cb __ 95");
test_opcode!(op_fd_cb____96, "fd cb __ 96");
test_opcode!(op_fd_cb____97, "fd cb __ 97");
test_opcode!(op_fd_cb____98, "fd cb __ 98");
test_opcode!(op_fd_cb____99, "fd cb __ 99");
test_opcode!(op_fd_cb____9a, "fd cb __ 9a");
test_opcode!(op_fd_cb____9b, "fd cb __ 9b");
test_opcode!(op_fd_cb____9c, "fd cb __ 9c");
test_opcode!(op_fd_cb____9d, "fd cb __ 9d");
test_opcode!(op_fd_cb____9e, "fd cb __ 9e");
test_opcode!(op_fd_cb____9f, "fd cb __ 9f");
test_opcode!(op_fd_cb____a0, "fd cb __ a0");
test_opcode!(op_fd_cb____a1, "fd cb __ a1");
test_opcode!(op_fd_cb____a2, "fd cb __ a2");
test_opcode!(op_fd_cb____a3, "fd cb __ a3");
test_opcode!(op_fd_cb____a4, "fd cb __ a4");
test_opcode!(op_fd_cb____a5, "fd cb __ a5");
test_opcode!(op_fd_cb____a6, "fd cb __ a6");
test_opcode!(op_fd_cb____a7, "fd cb __ a7");
test_opcode!(op_fd_cb____a8, "fd cb __ a8");
test_opcode!(op_fd_cb____a9, "fd cb __ a9");
test_opcode!(op_fd_cb____aa, "fd cb __ aa");
test_opcode!(op_fd_cb____ab, "fd cb __ ab");
test_opcode!(op_fd_cb____ac, "fd cb __ ac");
test_opcode!(op_fd_cb____ad, "fd cb __ ad");
test_opcode!(op_fd_cb____ae, "fd cb __ ae");
test_opcode!(op_fd_cb____af, "fd cb __ af");
test_opcode!(op_fd_cb____b0, "fd cb __ b0");
test_opcode!(op_fd_cb____b1, "fd cb __ b1");
test_opcode!(op_fd_cb____b2, "fd cb __ b2");
test_opcode!(op_fd_cb____b3, "fd cb __ b3");
test_opcode!(op_fd_cb____b4, "fd cb __ b4");
test_opcode!(op_fd_cb____b5, "fd cb __ b5");
test_opcode!(op_fd_cb____b6, "fd cb __ b6");
test_opcode!(op_fd_cb____b7, "fd cb __ b7");
test_opcode!(op_fd_cb____b8, "fd cb __ b8");
test_opcode!(op_fd_cb____b9, "fd cb __ b9");
test_opcode!(op_fd_cb____ba, "fd cb __ ba");
test_opcode!(op_fd_cb____bb, "fd cb __ bb");
test_opcode!(op_fd_cb____bc, "fd cb __ bc");
test_opcode!(op_fd_cb____bd, "fd cb __ bd");
test_opcode!(op_fd_cb____be, "fd cb __ be");
test_opcode!(op_fd_cb____bf, "fd cb __ bf");
test_opcode!(op_fd_cb____c0, "fd cb __ c0");
test_opcode!(op_fd_cb____c1, "fd cb __ c1");
test_opcode!(op_fd_cb____c2, "fd cb __ c2");
test_opcode!(op_fd_cb____c3, "fd cb __ c3");
test_opcode!(op_fd_cb____c4, "fd cb __ c4");
test_opcode!(op_fd_cb____c5, "fd cb __ c5");
test_opcode!(op_fd_cb____c6, "fd cb __ c6");
test_opcode!(op_fd_cb____c7, "fd cb __ c7");
test_opcode!(op_fd_cb____c8, "fd cb __ c8");
test_opcode!(op_fd_cb____c9, "fd cb __ c9");
test_opcode!(op_fd_cb____ca, "fd cb __ ca");
test_opcode!(op_fd_cb____cb, "fd cb __ cb");
test_opcode!(op_fd_cb____cc, "fd cb __ cc");
test_opcode!(op_fd_cb____cd, "fd cb __ cd");
test_opcode!(op_fd_cb____ce, "fd cb __ ce");
test_opcode!(op_fd_cb____cf, "fd cb __ cf");
test_opcode!(op_fd_cb____d0, "fd cb __ d0");
test_opcode!(op_fd_cb____d1, "fd cb __ d1");
test_opcode!(op_fd_cb____d2, "fd cb __ d2");
test_opcode!(op_fd_cb____d3, "fd cb __ d3");
test_opcode!(op_fd_cb____d4, "fd cb __ d4");
test_opcode!(op_fd_cb____d5, "fd cb __ d5");
test_opcode!(op_fd_cb____d6, "fd cb __ d6");
test_opcode!(op_fd_cb____d7, "fd cb __ d7");
test_opcode!(op_fd_cb____d8, "fd cb __ d8");
test_opcode!(op_fd_cb____d9, "fd cb __ d9");
test_opcode!(op_fd_cb____da, "fd cb __ da");
test_opcode!(op_fd_cb____db, "fd cb __ db");
test_opcode!(op_fd_cb____dc, "fd cb __ dc");
test_opcode!(op_fd_cb____dd, "fd cb __ dd");
test_opcode!(op_fd_cb____de, "fd cb __ de");
test_opcode!(op_fd_cb____df, "fd cb __ df");
test_opcode!(op_fd_cb____e0, "fd cb __ e0");
test_opcode!(op_fd_cb____e1, "fd cb __ e1");
test_opcode!(op_fd_cb____e2, "fd cb __ e2");
test_opcode!(op_fd_cb____e3, "fd cb __ e3");
test_opcode!(op_fd_cb____e4, "fd cb __ e4");
test_opcode!(op_fd_cb____e5, "fd cb __ e5");
test_opcode!(op_fd_cb____e6, "fd cb __ e6");
test_opcode!(op_fd_cb____e7, "fd cb __ e7");
test_opcode!(op_fd_cb____e8, "fd cb __ e8");
test_opcode!(op_fd_cb____e9, "fd cb __ e9");
test_opcode!(op_fd_cb____ea, "fd cb __ ea");
test_opcode!(op_fd_cb____eb, "fd cb __ eb");
test_opcode!(op_fd_cb____ec, "fd cb __ ec");
test_opcode!(op_fd_cb____ed, "fd cb __ ed");
test_opcode!(op_fd_cb____ee, "fd cb __ ee");
test_opcode!(op_fd_cb____ef, "fd cb __ ef");
test_opcode!(op_fd_cb____f0, "fd cb __ f0");
test_opcode!(op_fd_cb____f1, "fd cb __ f1");
test_opcode!(op_fd_cb____f2, "fd cb __ f2");
test_opcode!(op_fd_cb____f3, "fd cb __ f3");
test_opcode!(op_fd_cb____f4, "fd cb __ f4");
test_opcode!(op_fd_cb____f5, "fd cb __ f5");
test_opcode!(op_fd_cb____f6, "fd cb __ f6");
test_opcode!(op_fd_cb____f7, "fd cb __ f7");
test_opcode!(op_fd_cb____f8, "fd cb __ f8");
test_opcode!(op_fd_cb____f9, "fd cb __ f9");
test_opcode!(op_fd_cb____fa, "fd cb __ fa");
test_opcode!(op_fd_cb____fb, "fd cb __ fb");
test_opcode!(op_fd_cb____fc, "fd cb __ fc");
test_opcode!(op_fd_cb____fd, "fd cb __ fd");
test_opcode!(op_fd_cb____fe, "fd cb __ fe");
test_opcode!(op_fd_cb____ff, "fd cb __ ff");
test_opcode!(op_fd_cc, "fd cc");
test_opcode!(op_fd_cd, "fd cd");
test_opcode!(op_fd_ce, "fd ce");
test_opcode!(op_fd_cf, "fd cf");
test_opcode!(op_fd_d0, "fd d0");
test_opcode!(op_fd_d1, "fd d1");
test_opcode!(op_fd_d2, "fd d2");
test_opcode!(op_fd_d3, "fd d3");
test_opcode!(op_fd_d4, "fd d4");
test_opcode!(op_fd_d5, "fd d5");
test_opcode!(op_fd_d6, "fd d6");
test_opcode!(op_fd_d7, "fd d7");
test_opcode!(op_fd_d8, "fd d8");
test_opcode!(op_fd_d9, "fd d9");
test_opcode!(op_fd_da, "fd da");
test_opcode!(op_fd_db, "fd db");
test_opcode!(op_fd_dc, "fd dc");
test_opcode!(op_fd_de, "fd de");
test_opcode!(op_fd_df, "fd df");
test_opcode!(op_fd_e0, "fd e0");
test_opcode!(op_fd_e1, "fd e1");
test_opcode!(op_fd_e2, "fd e2");
test_opcode!(op_fd_e3, "fd e3");
test_opcode!(op_fd_e4, "fd e4");
test_opcode!(op_fd_e5, "fd e5");
test_opcode!(op_fd_e6, "fd e6");
test_opcode!(op_fd_e7, "fd e7");
test_opcode!(op_fd_e8, "fd e8");
test_opcode!(op_fd_e9, "fd e9");
test_opcode!(op_fd_ea, "fd ea");
test_opcode!(op_fd_eb, "fd eb");
test_opcode!(op_fd_ec, "fd ec");
test_opcode!(op_fd_ee, "fd ee");
test_opcode!(op_fd_ef, "fd ef");
test_opcode!(op_fd_f0, "fd f0");
test_opcode!(op_fd_f1, "fd f1");
test_opcode!(op_fd_f2, "fd f2");
test_opcode!(op_fd_f3, "fd f3");
test_opcode!(op_fd_f4, "fd f4");
test_opcode!(op_fd_f5, "fd f5");
test_opcode!(op_fd_f6, "fd f6");
test_opcode!(op_fd_f7, "fd f7");
test_opcode!(op_fd_f8, "fd f8");
test_opcode!(op_fd_f9, "fd f9");
test_opcode!(op_fd_fa, "fd fa");
test_opcode!(op_fd_fb, "fd fb");
test_opcode!(op_fd_fc, "fd fc");
test_opcode!(op_fd_fe, "fd fe");
test_opcode!(op_fd_ff, "fd ff");
test_opcode!(op_fe, "fe");
test_opcode!(op_ff, "ff");
