#![cfg(feature = "verification")]

use std::{collections::HashMap, fs, io::BufReader, path::Path, sync::LazyLock};

use cpu::{V30, V30State};
use flate2::read::GzDecoder;
use serde::Deserialize;

struct TestBus {
    ram: Box<[u8; 1_048_576]>,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; 1_048_576].into_boxed_slice().try_into().unwrap(),
        }
    }
}

impl common::Bus for TestBus {
    fn read_byte(&mut self, address: u32) -> u8 {
        self.ram[(address & 0xFFFFF) as usize]
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        self.ram[(address & 0xFFFFF) as usize] = value;
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

#[derive(Deserialize)]
struct TestCase {
    name: String,
    bytes: Vec<u8>,
    initial: InitialState,
    #[serde(rename = "final")]
    final_state: FinalState,
}

#[derive(Deserialize)]
struct InitialState {
    regs: Registers,
    ram: Vec<(u32, u8)>,
}

#[derive(Deserialize)]
struct FinalState {
    regs: HashMap<String, u16>,
    ram: Vec<(u32, u8)>,
}

#[derive(Deserialize)]
struct Registers {
    ax: u16,
    bx: u16,
    cx: u16,
    dx: u16,
    sp: u16,
    bp: u16,
    si: u16,
    di: u16,
    cs: u16,
    ss: u16,
    ds: u16,
    es: u16,
    ip: u16,
    flags: u16,
}

#[derive(Deserialize)]
struct Metadata {
    opcodes: HashMap<String, OpcodeEntry>,
}

#[derive(Deserialize)]
struct OpcodeEntry {
    status: Option<String>,
    #[serde(rename = "flags-mask")]
    flags_mask: Option<u16>,
    reg: Option<HashMap<String, OpcodeInfo>>,
}

#[derive(Deserialize)]
struct OpcodeInfo {
    status: String,
    #[serde(rename = "flags-mask")]
    flags_mask: Option<u16>,
}

fn test_dir() -> &'static Path {
    static DIR: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/SingleStepTests/v20/v1_native")
    });
    &DIR
}

fn metadata() -> &'static Metadata {
    static META: LazyLock<Metadata> = LazyLock::new(|| {
        serde_json::from_reader(BufReader::new(
            fs::File::open(test_dir().join("metadata.json")).unwrap(),
        ))
        .unwrap()
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
        (0x8000, "MD"),
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

fn initial_reg_value(regs: &Registers, name: &str) -> u16 {
    match name {
        "ax" => regs.ax,
        "bx" => regs.bx,
        "cx" => regs.cx,
        "dx" => regs.dx,
        "sp" => regs.sp,
        "bp" => regs.bp,
        "si" => regs.si,
        "di" => regs.di,
        "cs" => regs.cs,
        "ss" => regs.ss,
        "ds" => regs.ds,
        "es" => regs.es,
        "ip" => regs.ip,
        "flags" => regs.flags,
        _ => panic!("unknown register: {name}"),
    }
}

fn build_expected_state(initial: &Registers, final_regs: &HashMap<String, u16>) -> V30State {
    let get = |name: &str| -> u16 {
        final_regs
            .get(name)
            .copied()
            .unwrap_or_else(|| initial_reg_value(initial, name))
    };

    let mut s = V30State::default();
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
    s
}

fn is_division_exception(
    opcode: &str,
    reg_ext: Option<&str>,
    initial: &InitialState,
    final_state: &FinalState,
) -> bool {
    let is_div = matches!(
        (opcode, reg_ext),
        ("F6", Some("6")) | ("F6", Some("7")) | ("F7", Some("6")) | ("F7", Some("7"))
    );
    if !is_div {
        return false;
    }

    let handler_ip = initial
        .ram
        .iter()
        .find(|(addr, _)| *addr == 0)
        .map(|(_, v)| *v as u16)
        .unwrap_or(0)
        | initial
            .ram
            .iter()
            .find(|(addr, _)| *addr == 1)
            .map(|(_, v)| (*v as u16) << 8)
            .unwrap_or(0);
    let handler_cs = initial
        .ram
        .iter()
        .find(|(addr, _)| *addr == 2)
        .map(|(_, v)| *v as u16)
        .unwrap_or(0)
        | initial
            .ram
            .iter()
            .find(|(addr, _)| *addr == 3)
            .map(|(_, v)| (*v as u16) << 8)
            .unwrap_or(0);

    let expected = build_expected_state(&initial.regs, &final_state.regs);
    expected.cs() == handler_cs && expected.ip == handler_ip
}

fn run_test_file(stem: &str) {
    let metadata = metadata();
    let filename = format!("{stem}.json.gz");

    let (opcode, reg_ext) = if let Some(dot_pos) = stem.find('.') {
        (&stem[..dot_pos], Some(&stem[dot_pos + 1..]))
    } else {
        (stem, None)
    };

    let (status, flags_mask) = match metadata.opcodes.get(opcode) {
        Some(entry) => {
            if let Some(reg_ext) = reg_ext {
                match &entry.reg {
                    Some(reg_map) => match reg_map.get(reg_ext) {
                        Some(info) => (info.status.as_str(), info.flags_mask.unwrap_or(0xFFFF)),
                        None => return,
                    },
                    None => return,
                }
            } else {
                match &entry.status {
                    Some(s) => (s.as_str(), entry.flags_mask.unwrap_or(0xFFFF)),
                    None => match &entry.reg {
                        Some(reg_map) => {
                            let any_testable =
                                reg_map.values().any(|info| should_test(&info.status));
                            if !any_testable {
                                return;
                            }
                            let mask = reg_map
                                .values()
                                .map(|info| info.flags_mask.unwrap_or(0xFFFF))
                                .fold(0xFFFF, |acc, m| acc & m);
                            ("normal", mask)
                        }
                        None => return,
                    },
                }
            }
        }
        None => return,
    };

    if !should_test(status) {
        return;
    }

    let file = fs::File::open(test_dir().join(&filename)).unwrap();
    let decoder = GzDecoder::new(BufReader::new(file));
    let test_cases: Vec<TestCase> = serde_json::from_reader(decoder).unwrap();

    let mut failures: Vec<String> = Vec::new();

    for (index, test) in test_cases.iter().enumerate() {
        let mut bus = TestBus::new();
        for &(addr, val) in &test.initial.ram {
            bus.ram[(addr & 0xFFFFF) as usize] = val;
        }

        let initial_state = {
            let mut s = V30State::default();
            s.set_ax(test.initial.regs.ax);
            s.set_cx(test.initial.regs.cx);
            s.set_dx(test.initial.regs.dx);
            s.set_bx(test.initial.regs.bx);
            s.set_sp(test.initial.regs.sp);
            s.set_bp(test.initial.regs.bp);
            s.set_si(test.initial.regs.si);
            s.set_di(test.initial.regs.di);
            s.set_es(test.initial.regs.es);
            s.set_cs(test.initial.regs.cs);
            s.set_ss(test.initial.regs.ss);
            s.set_ds(test.initial.regs.ds);
            s.ip = test.initial.regs.ip;
            s.set_compressed_flags(test.initial.regs.flags);
            s
        };

        let mut cpu = V30::new();
        cpu.load_state(&initial_state);
        cpu.step(&mut bus);

        let expected = build_expected_state(&test.initial.regs, &test.final_state.regs);

        let mut diffs: Vec<String> = Vec::new();

        let check_reg = |name: &str,
                         initial_val: u16,
                         cpu_val: u16,
                         expected_val: u16,
                         diffs: &mut Vec<String>| {
            if cpu_val != expected_val {
                diffs.push(format!(
                    "  {name}: expected 0x{expected_val:04X}, got 0x{cpu_val:04X} (was 0x{initial_val:04X})"
                ));
            }
        };

        let ini = &test.initial.regs;
        check_reg("ax", ini.ax, cpu.ax(), expected.ax(), &mut diffs);
        check_reg("bx", ini.bx, cpu.bx(), expected.bx(), &mut diffs);
        check_reg("cx", ini.cx, cpu.cx(), expected.cx(), &mut diffs);
        check_reg("dx", ini.dx, cpu.dx(), expected.dx(), &mut diffs);
        check_reg("sp", ini.sp, cpu.sp(), expected.sp(), &mut diffs);
        check_reg("bp", ini.bp, cpu.bp(), expected.bp(), &mut diffs);
        check_reg("si", ini.si, cpu.si(), expected.si(), &mut diffs);
        check_reg("di", ini.di, cpu.di(), expected.di(), &mut diffs);
        check_reg("cs", ini.cs, cpu.cs(), expected.cs(), &mut diffs);
        check_reg("ss", ini.ss, cpu.ss(), expected.ss(), &mut diffs);
        check_reg("ds", ini.ds, cpu.ds(), expected.ds(), &mut diffs);
        check_reg("es", ini.es, cpu.es(), expected.es(), &mut diffs);
        check_reg("ip", ini.ip, cpu.ip, expected.ip, &mut diffs);

        let cpu_flags_masked = cpu.compressed_flags() & flags_mask;
        let expected_flags_masked = expected.compressed_flags() & flags_mask;
        if cpu_flags_masked != expected_flags_masked {
            diffs.push(format!(
                "{} (was 0x{:04X})",
                format_flags_diff(
                    expected.compressed_flags(),
                    cpu.compressed_flags(),
                    flags_mask
                ),
                ini.flags
            ));
        }

        let div_exception =
            is_division_exception(opcode, reg_ext, &test.initial, &test.final_state);

        if !div_exception {
            for &(addr, expected_val) in &test.final_state.ram {
                let actual_val = bus.ram[(addr & 0xFFFFF) as usize];
                if actual_val != expected_val {
                    let initial_val = test
                        .initial
                        .ram
                        .iter()
                        .find(|(a, _)| *a == addr)
                        .map(|(_, v)| *v);
                    match initial_val {
                        Some(init) => diffs.push(format!(
                            "  ram[0x{addr:05X}]: expected 0x{expected_val:02X}, got 0x{actual_val:02X} (was 0x{init:02X})"
                        )),
                        None => diffs.push(format!(
                            "  ram[0x{addr:05X}]: expected 0x{expected_val:02X}, got 0x{actual_val:02X} (not in initial RAM)"
                        )),
                    }
                }
            }
        }

        if !diffs.is_empty() {
            let bytes_hex: Vec<String> = test.bytes.iter().map(|b| format!("{b:02X}")).collect();
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
        let mut msg = format!("{filename}: {fail_count}/{test_count} tests failed\n");
        let display_count = failures.len().min(5);
        for f in &failures[..display_count] {
            msg.push_str(f);
            msg.push('\n');
        }
        if failures.len() > 5 {
            msg.push_str(&format!("  ... and {} more failures\n", failures.len() - 5));
        }
        panic!("{msg}");
    }
}

macro_rules! test_opcode {
    ($name:ident, $file:expr) => {
        #[test]
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
