#![cfg(feature = "verification")]

#[path = "../../cpu/tests/common/verification_common.rs"]
mod verification_common;

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use common::Cpu as _;
use cpu::{I386State, SegReg32};
use dynarec::I386Jit;
use verification_common::{load_moo_tests, load_revocation_list};

const RAM_SIZE: usize = 4 * 1024 * 1024;
const ADDRESS_MASK: u32 = 0x00FF_FFFF;
const REG_ORDER_386: [&str; 20] = [
    "cr0", "cr3", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "cs", "ds", "es", "fs",
    "gs", "ss", "eip", "eflags", "dr6", "dr7",
];

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

    fn io_read_byte(&mut self, port: u16) -> u8 {
        match port {
            0x22 => 0x7F,
            0x23 => 0x42,
            _ => 0xFF,
        }
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
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../cpu/tests/SingleStepTests/80386/v1_ex_real_mode")
    });
    &DIR
}

fn revocation_list() -> &'static HashSet<String> {
    static REVOKED: LazyLock<HashSet<String>> = LazyLock::new(|| {
        load_revocation_list(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../cpu/tests/SingleStepTests/80386/revocation_list.txt"),
        )
    });
    &REVOKED
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                field.push('"');
                let _ = chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if ch == ',' && !in_quotes {
            fields.push(field);
            field = String::new();
        } else {
            field.push(ch);
        }
    }

    fields.push(field);
    fields
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u16::from_str_radix(raw, 16).ok()
}

fn flags_compare_masks() -> &'static HashMap<(String, Option<u8>), u16> {
    static MAP: LazyLock<HashMap<(String, Option<u8>), u16>> = LazyLock::new(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../cpu/tests/SingleStepTests/80386/80386.csv");
        let file = fs::File::open(path).unwrap();
        let mut lines = std::io::BufRead::lines(BufReader::new(file));
        let header = lines.next().unwrap().unwrap();
        let columns = split_csv_line(&header);

        let mut index_op = None;
        let mut index_ex = None;
        let mut index_f_umask = None;

        for (index, column) in columns.iter().enumerate() {
            match column.as_str() {
                "op" => index_op = Some(index),
                "ex" => index_ex = Some(index),
                "f_umask" => index_f_umask = Some(index),
                _ => {}
            }
        }

        let op_index = index_op.unwrap();
        let ex_index = index_ex.unwrap();
        let f_umask_index = index_f_umask.unwrap();

        let mut result = HashMap::new();

        for line in lines {
            let line = line.unwrap();
            if line.trim().is_empty() {
                continue;
            }
            let fields = split_csv_line(&line);
            if fields.len() <= f_umask_index {
                continue;
            }

            let op = fields[op_index].trim();
            if op.is_empty() {
                continue;
            }

            let ex = fields[ex_index].trim();
            let ex = if ex.is_empty() {
                None
            } else {
                Some(ex.parse::<u8>().unwrap())
            };

            let Some(compare_mask) = parse_hex_u16(&fields[f_umask_index]) else {
                continue;
            };
            let key = (op.to_ascii_uppercase(), ex);
            result.insert(key, compare_mask);
        }

        result
    });

    &MAP
}

fn parse_stem(stem: &str) -> (&str, Option<u8>) {
    let mut opcode = stem;
    loop {
        if let Some(rest) = opcode.strip_prefix("66") {
            opcode = rest;
        } else if let Some(rest) = opcode.strip_prefix("67") {
            opcode = rest;
        } else {
            break;
        }
    }

    if let Some(dot_pos) = opcode.find('.') {
        (
            &opcode[..dot_pos],
            Some(opcode[dot_pos + 1..].parse::<u8>().unwrap()),
        )
    } else {
        (opcode, None)
    }
}

fn flags_compare_mask(stem: &str) -> u32 {
    let masks = flags_compare_masks();
    let (opcode, reg_ext) = parse_stem(stem);
    let opcode_upper = opcode.to_ascii_uppercase();
    let low16_mask = masks
        .get(&(opcode_upper.clone(), reg_ext))
        .copied()
        .or_else(|| match (opcode_upper.as_str(), reg_ext) {
            ("0FAF", None) => Some(0xFF2B),
            _ => None,
        })
        .unwrap_or(0xFFFF);
    0xFFFF_0000 | low16_mask as u32
}

fn initial_reg_value(initial_regs: &HashMap<String, u32>, name: &str) -> u32 {
    initial_regs
        .get(name)
        .copied()
        .unwrap_or_else(|| panic!("missing register in initial state: {name}"))
}

fn build_state(regs: &HashMap<String, u32>, fallback: &HashMap<String, u32>) -> I386State {
    let get = |name: &str| -> u32 {
        regs.get(name)
            .copied()
            .unwrap_or_else(|| initial_reg_value(fallback, name))
    };

    let mut s = I386State::default();
    s.cr0 = get("cr0");
    s.cr3 = get("cr3");
    s.set_eax(get("eax"));
    s.set_ebx(get("ebx"));
    s.set_ecx(get("ecx"));
    s.set_edx(get("edx"));
    s.set_esi(get("esi"));
    s.set_edi(get("edi"));
    s.set_ebp(get("ebp"));
    s.set_esp(get("esp"));
    s.set_cs(get("cs") as u16);
    s.set_ds(get("ds") as u16);
    s.set_es(get("es") as u16);
    s.set_fs(get("fs") as u16);
    s.set_gs(get("gs") as u16);
    s.set_ss(get("ss") as u16);
    s.set_eip(get("eip"));
    s.set_eflags(get("eflags"));
    s.dr6 = get("dr6");
    s.dr7 = get("dr7");
    s
}

fn run_test_file(stem: &str, local_revoked_hashes: &[&str]) {
    let revoked = revocation_list();
    let local_revoked: HashSet<String> = local_revoked_hashes
        .iter()
        .map(|hash| hash.to_ascii_lowercase())
        .collect();

    let filename = format!("{stem}.MOO.gz");
    let path = test_dir().join(&filename);
    let test_cases = load_moo_tests(&path, &[], &REG_ORDER_386);
    let mut failures: Vec<String> = Vec::new();
    let mut total_jit_instrs: u64 = 0;
    let mut total_fallback: u64 = 0;

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

        let initial = build_state(&test.initial.regs, &test.initial.regs);
        let expected = build_state(&test.final_state.regs, &test.initial.regs);
        let mut bus = TestBus::new();
        for &(address, value) in &test.initial.ram {
            bus.set_memory(address, value);
        }

        let mut cpu: I386Jit = I386Jit::new();
        cpu.load_state(&initial);

        let mut rounds = 0usize;
        while !cpu.halted() && rounds < 16 {
            cpu.run_for(4096, &mut bus);
            let stats = cpu.stats();
            total_jit_instrs += stats.jit_instrs_executed;
            total_fallback += stats.fallback_instrs;
            rounds += 1;
        }

        if !cpu.halted() {
            let hash_suffix = test
                .hash
                .as_ref()
                .map(|hash| format!(" hash={hash}"))
                .unwrap_or_default();
            failures.push(format!(
                "[{filename} #{} idx={}] {}{} (JIT dispatcher did not halt within budget)",
                failures.len(),
                test.idx,
                test.name,
                hash_suffix
            ));
            continue;
        }

        let mut diffs: Vec<String> = Vec::new();
        let actual = cpu.state();

        let check_u32 = |name: &str,
                         initial_value: u32,
                         actual_value: u32,
                         expected_value: u32,
                         diffs: &mut Vec<String>| {
            if actual_value != expected_value {
                diffs.push(format!(
                        "  {name}: expected 0x{expected_value:08X}, got 0x{actual_value:08X} (was 0x{initial_value:08X})"
                    ));
            }
        };

        check_u32("cr0", initial.cr0, actual.cr0, expected.cr0, &mut diffs);
        check_u32("cr3", initial.cr3, actual.cr3, expected.cr3, &mut diffs);
        check_u32(
            "eax",
            initial.eax(),
            actual.eax(),
            expected.eax(),
            &mut diffs,
        );
        check_u32(
            "ebx",
            initial.ebx(),
            actual.ebx(),
            expected.ebx(),
            &mut diffs,
        );
        check_u32(
            "ecx",
            initial.ecx(),
            actual.ecx(),
            expected.ecx(),
            &mut diffs,
        );
        check_u32(
            "edx",
            initial.edx(),
            actual.edx(),
            expected.edx(),
            &mut diffs,
        );
        check_u32(
            "esi",
            initial.esi(),
            actual.esi(),
            expected.esi(),
            &mut diffs,
        );
        check_u32(
            "edi",
            initial.edi(),
            actual.edi(),
            expected.edi(),
            &mut diffs,
        );
        check_u32(
            "ebp",
            initial.ebp(),
            actual.ebp(),
            expected.ebp(),
            &mut diffs,
        );
        check_u32(
            "esp",
            initial.esp(),
            actual.esp(),
            expected.esp(),
            &mut diffs,
        );
        check_u32(
            "eip",
            initial.eip(),
            actual.eip(),
            expected.eip(),
            &mut diffs,
        );
        check_u32("dr6", initial.dr6, actual.dr6, expected.dr6, &mut diffs);
        check_u32("dr7", initial.dr7, actual.dr7, expected.dr7, &mut diffs);

        let check_u16 = |name: &str,
                         actual_value: u16,
                         expected_value: u16,
                         initial_value: u16,
                         diffs: &mut Vec<String>| {
            if actual_value != expected_value {
                diffs.push(format!(
                        "  {name}: expected 0x{expected_value:04X}, got 0x{actual_value:04X} (was 0x{initial_value:04X})"
                    ));
            }
        };

        check_u16("cs", actual.cs(), expected.cs(), initial.cs(), &mut diffs);
        check_u16("ds", actual.ds(), expected.ds(), initial.ds(), &mut diffs);
        check_u16("es", actual.es(), expected.es(), initial.es(), &mut diffs);
        let fs_seg = actual.sregs[SegReg32::FS as usize];
        let gs_seg = actual.sregs[SegReg32::GS as usize];
        check_u16(
            "fs",
            fs_seg,
            expected.sregs[SegReg32::FS as usize],
            initial.sregs[SegReg32::FS as usize],
            &mut diffs,
        );
        check_u16(
            "gs",
            gs_seg,
            expected.sregs[SegReg32::GS as usize],
            initial.sregs[SegReg32::GS as usize],
            &mut diffs,
        );
        check_u16("ss", actual.ss(), expected.ss(), initial.ss(), &mut diffs);

        let eflags_mask = flags_compare_mask(stem);
        if (actual.eflags() & eflags_mask) != (expected.eflags() & eflags_mask) {
            diffs.push(format!(
                "  eflags: expected 0x{:08X}, got 0x{:08X} (was 0x{:08X}, mask 0x{:08X})",
                expected.eflags() & eflags_mask,
                actual.eflags() & eflags_mask,
                initial.eflags(),
                eflags_mask
            ));
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
        let display_count = failures.len().min(10);
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

    // Ensure the JIT decoder actually handled the tested opcode
    // rather than silently falling back. Some instructions (LOOP,
    // JCXZ, indirect JMP/CALL, direct CALL/JMP/RET) emit zero IR ops
    // but still go through apply_exit, so we accept either a non-zero
    // jit_instrs count or a JIT-serviced block with non-zero fallback
    // count lower than test count (proving at least some vector went
    // through JIT rather than each one falling back on the first byte).
    let fallbacks_strict = total_fallback < test_cases.len() as u64;
    assert!(
        total_jit_instrs > 0 || fallbacks_strict,
        "{filename}: JIT executed zero IR ops across {} tests and fell back on all of them; decoder is not emitting code for this opcode",
        test_cases.len(),
    );
}

macro_rules! test_opcode {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            run_test_file($file, &[]);
        }
    };
    ($name:ident, $file:expr, [$($skip_hash:expr),* $(,)?]) => {
        #[test]
        fn $name() {
            run_test_file($file, &[$($skip_hash),*]);
        }
    };
}

// ALU register/memory/immediate forms (ADD/OR/ADC/SBB/AND/SUB/XOR/CMP).
test_opcode!(op_00, "00");
test_opcode!(op_01, "01");
test_opcode!(op_02, "02");
test_opcode!(op_03, "03");
test_opcode!(op_04, "04");
test_opcode!(op_05, "05");
test_opcode!(op_08, "08");
test_opcode!(op_09, "09");
test_opcode!(op_0a, "0A");
test_opcode!(op_0b, "0B");
test_opcode!(op_0c, "0C");
test_opcode!(op_0d, "0D");
test_opcode!(op_10, "10");
test_opcode!(op_11, "11");
test_opcode!(op_12, "12");
test_opcode!(op_13, "13");
test_opcode!(op_14, "14");
test_opcode!(op_15, "15");
test_opcode!(op_18, "18");
test_opcode!(op_19, "19");
test_opcode!(op_1a, "1A");
test_opcode!(op_1b, "1B");
test_opcode!(op_1c, "1C");
test_opcode!(op_1d, "1D");
test_opcode!(op_20, "20");
test_opcode!(op_21, "21");
test_opcode!(op_22, "22");
test_opcode!(op_23, "23");
test_opcode!(op_24, "24");
test_opcode!(op_25, "25");
test_opcode!(op_28, "28");
test_opcode!(op_29, "29");
test_opcode!(op_2a, "2A");
test_opcode!(op_2b, "2B");
test_opcode!(op_2c, "2C");
test_opcode!(op_2d, "2D");
test_opcode!(op_30, "30");
test_opcode!(op_31, "31");
test_opcode!(op_32, "32");
test_opcode!(op_33, "33");
test_opcode!(op_34, "34");
test_opcode!(op_35, "35");
test_opcode!(op_38, "38");
test_opcode!(op_39, "39");
test_opcode!(op_3a, "3A");
test_opcode!(op_3b, "3B");
test_opcode!(op_3c, "3C");
test_opcode!(op_3d, "3D");

// INC/DEC register.
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

// PUSH/POP register (16-bit operand).
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

// PUSH imm / IMUL imm.
test_opcode!(op_68, "68");
test_opcode!(op_69, "69");
test_opcode!(op_6a, "6A");
test_opcode!(op_6b, "6B");

// Short Jcc.
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

// Group 1.
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

// TEST reg/mem, MOV reg/mem, LEA, XCHG reg-acc, CBW/CWD.
test_opcode!(op_84, "84");
test_opcode!(op_85, "85");
test_opcode!(op_88, "88");
test_opcode!(op_89, "89");
test_opcode!(op_8a, "8A");
test_opcode!(op_8b, "8B");
test_opcode!(op_8d, "8D");
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

// MOV moffs, TEST imm, MOV r, imm.
test_opcode!(op_a0, "A0");
test_opcode!(op_a1, "A1");
test_opcode!(op_a2, "A2");
test_opcode!(op_a3, "A3");
test_opcode!(op_a8, "A8");
test_opcode!(op_a9, "A9");
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

// Shifts / rotates.
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

// MOV imm to r/m, RET, LOOP, JMP/CALL near.
test_opcode!(op_c2, "C2");
test_opcode!(op_c3, "C3");
test_opcode!(op_c6, "C6");
test_opcode!(op_c7, "C7");
test_opcode!(op_e0, "E0");
test_opcode!(op_e1, "E1");
test_opcode!(op_e2, "E2");
test_opcode!(op_e3, "E3");
test_opcode!(op_e8, "E8");
test_opcode!(op_e9, "E9");
test_opcode!(op_eb, "EB");

// Flag ops.
test_opcode!(op_f5, "F5");
test_opcode!(op_f8, "F8");
test_opcode!(op_f9, "F9");
test_opcode!(op_fc, "FC");
test_opcode!(op_fd, "FD");

// Unary mem/reg (INC/DEC, PUSH r/m, indirect CALL/JMP).
test_opcode!(op_fe_0, "FE.0");
test_opcode!(op_fe_1, "FE.1");
test_opcode!(op_ff_0, "FF.0");
test_opcode!(op_ff_1, "FF.1");
test_opcode!(op_ff_2, "FF.2");
test_opcode!(op_ff_4, "FF.4");
test_opcode!(op_ff_6, "FF.6");

// Long Jcc.
test_opcode!(op_0f80, "0F80");
test_opcode!(op_0f81, "0F81");
test_opcode!(op_0f82, "0F82");
test_opcode!(op_0f83, "0F83");
test_opcode!(op_0f84, "0F84");
test_opcode!(op_0f85, "0F85");
test_opcode!(op_0f86, "0F86");
test_opcode!(op_0f87, "0F87");
test_opcode!(op_0f88, "0F88");
test_opcode!(op_0f89, "0F89");
test_opcode!(op_0f8a, "0F8A");
test_opcode!(op_0f8b, "0F8B");
test_opcode!(op_0f8c, "0F8C");
test_opcode!(op_0f8d, "0F8D");
test_opcode!(op_0f8e, "0F8E");
test_opcode!(op_0f8f, "0F8F");

// SETcc.
test_opcode!(op_0f90, "0F90");
test_opcode!(op_0f91, "0F91");
test_opcode!(op_0f92, "0F92");
test_opcode!(op_0f93, "0F93");
test_opcode!(op_0f94, "0F94");
test_opcode!(op_0f95, "0F95");
test_opcode!(op_0f96, "0F96");
test_opcode!(op_0f97, "0F97");
test_opcode!(op_0f98, "0F98");
test_opcode!(op_0f99, "0F99");
test_opcode!(op_0f9a, "0F9A");
test_opcode!(op_0f9b, "0F9B");
test_opcode!(op_0f9c, "0F9C");
test_opcode!(op_0f9d, "0F9D");
test_opcode!(op_0f9e, "0F9E");
test_opcode!(op_0f9f, "0F9F");

// IMUL r,r/m and MOVZX/MOVSX.
test_opcode!(op_0faf, "0FAF");
test_opcode!(op_0fb6, "0FB6");
test_opcode!(op_0fb7, "0FB7");
test_opcode!(op_0fbe, "0FBE");
test_opcode!(op_0fbf, "0FBF");
