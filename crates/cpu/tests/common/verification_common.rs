#![cfg(feature = "verification")]
#![allow(dead_code)]

use std::{collections::HashMap, fmt::Write, fs, path::Path};

use zlib_rs::{InflateConfig, ReturnCode, decompress_slice};

#[derive(Debug, Clone)]
pub struct MooState {
    pub regs: HashMap<String, u32>,
    pub ram: Vec<(u32, u8)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MooCycle {
    pub address: Option<u16>,
    pub data: Option<u8>,
    pub status: [u8; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MooPort {
    pub address: u16,
    pub value: u8,
    pub direction: u8,
}

#[cfg(feature = "verification")]
#[derive(Debug, Clone)]
pub struct MooException {}

#[derive(Debug, Clone)]
pub struct MooTest {
    pub idx: u32,
    pub name: String,
    pub bytes: Vec<u8>,
    pub initial: MooState,
    pub final_state: MooState,
    pub cycles: Vec<MooCycle>,
    pub ports: Vec<MooPort>,
    pub exception: Option<MooException>,
    pub hash: Option<String>,
}

fn read_u16(bytes: &[u8], offset: &mut usize) -> u16 {
    let end = *offset + 2;
    let value = u16::from_le_bytes(bytes[*offset..end].try_into().unwrap());
    *offset = end;
    value
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> u32 {
    let end = *offset + 4;
    let value = u32::from_le_bytes(bytes[*offset..end].try_into().unwrap());
    *offset = end;
    value
}

fn read_tag(bytes: &[u8], offset: &mut usize) -> [u8; 4] {
    let end = *offset + 4;
    let value = bytes[*offset..end].try_into().unwrap();
    *offset = end;
    value
}

fn parse_regs16(payload: &[u8], reg_order: &[&str]) -> HashMap<String, u32> {
    let mut offset = 0;
    let mask = read_u16(payload, &mut offset);
    let mut regs = HashMap::new();

    for (index, name) in reg_order.iter().enumerate() {
        if mask & (1 << index) != 0 {
            let value = read_u16(payload, &mut offset) as u32;
            regs.insert((*name).to_string(), value);
        }
    }

    regs
}

fn parse_regs32(payload: &[u8], reg_order: &[&str]) -> HashMap<String, u32> {
    let mut offset = 0;
    let mask = read_u32(payload, &mut offset);
    let mut regs = HashMap::new();

    for (index, name) in reg_order.iter().enumerate() {
        if mask & (1 << index) != 0 {
            let value = read_u32(payload, &mut offset);
            regs.insert((*name).to_string(), value);
        }
    }

    regs
}

fn parse_ram(payload: &[u8]) -> Vec<(u32, u8)> {
    let mut offset = 0;
    let count = read_u32(payload, &mut offset) as usize;
    let mut entries = Vec::with_capacity(count);

    for _ in 0..count {
        let address = read_u32(payload, &mut offset);
        let value = payload[offset];
        offset += 1;
        entries.push((address, value));
    }

    entries
}

fn parse_cycles(payload: &[u8]) -> Vec<MooCycle> {
    let mut offset = 0;
    let count = read_u32(payload, &mut offset) as usize;
    let mut cycles = Vec::with_capacity(count);

    for _ in 0..count {
        let flags = payload[offset];
        offset += 1;
        let address = read_u16(payload, &mut offset);
        let data = payload[offset];
        offset += 1;
        let status = read_tag(payload, &mut offset);

        cycles.push(MooCycle {
            address: if flags & 0x01 != 0 {
                Some(address)
            } else {
                None
            },
            data: if flags & 0x02 != 0 { Some(data) } else { None },
            status,
        });
    }

    cycles
}

fn parse_ports(payload: &[u8]) -> Vec<MooPort> {
    let mut offset = 0;
    let count = read_u32(payload, &mut offset) as usize;
    let mut ports = Vec::with_capacity(count);

    for _ in 0..count {
        let address = read_u16(payload, &mut offset);
        let value = payload[offset];
        offset += 1;
        let direction = payload[offset];
        offset += 1;
        ports.push(MooPort {
            address,
            value,
            direction,
        });
    }

    ports
}

fn parse_cpu_state(payload: &[u8], reg_order16: &[&str], reg_order32: &[&str]) -> MooState {
    let mut offset = 0;
    let mut regs = HashMap::new();
    let mut ram = Vec::new();

    while offset < payload.len() {
        let tag = read_tag(payload, &mut offset);
        let length = read_u32(payload, &mut offset) as usize;
        let end = offset + length;
        let sub_payload = &payload[offset..end];

        match &tag {
            b"REGS" => regs = parse_regs16(sub_payload, reg_order16),
            b"RG32" => regs = parse_regs32(sub_payload, reg_order32),
            b"RAM " => ram = parse_ram(sub_payload),
            b"QUEU" | b"EA32" => {}
            _ => {}
        }

        offset = end;
    }

    MooState { regs, ram }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn parse_test_chunk(payload: &[u8], reg_order16: &[&str], reg_order32: &[&str]) -> MooTest {
    let mut offset = 0;
    let idx = read_u32(payload, &mut offset);
    let mut name = String::new();
    let mut bytes = Vec::new();
    let mut initial = MooState {
        regs: HashMap::new(),
        ram: Vec::new(),
    };
    let mut final_state = MooState {
        regs: HashMap::new(),
        ram: Vec::new(),
    };
    let mut cycles = Vec::new();
    let mut ports = Vec::new();
    let mut exception = None;
    let mut hash = None;

    while offset < payload.len() {
        let tag = read_tag(payload, &mut offset);
        let length = read_u32(payload, &mut offset) as usize;
        let end = offset + length;
        let sub_payload = &payload[offset..end];

        match &tag {
            b"NAME" => {
                let mut name_offset = 0;
                let name_len = read_u32(sub_payload, &mut name_offset) as usize;
                name = String::from_utf8(sub_payload[name_offset..name_offset + name_len].to_vec())
                    .unwrap();
            }
            b"BYTS" => {
                let mut bytes_offset = 0;
                let byte_count = read_u32(sub_payload, &mut bytes_offset) as usize;
                bytes = sub_payload[bytes_offset..bytes_offset + byte_count].to_vec();
            }
            b"INIT" => initial = parse_cpu_state(sub_payload, reg_order16, reg_order32),
            b"FINA" => final_state = parse_cpu_state(sub_payload, reg_order16, reg_order32),
            b"CYCL" => cycles = parse_cycles(sub_payload),
            b"PORT" => ports = parse_ports(sub_payload),
            b"EXCP" if !sub_payload.is_empty() => {
                exception = Some(MooException {});
            }
            b"EXCP" => {}
            b"HASH" => {
                hash = Some(bytes_to_hex(sub_payload));
            }
            b"GMET" => {}
            _ => {}
        }

        offset = end;
    }

    MooTest {
        idx,
        name,
        bytes,
        initial,
        final_state,
        cycles,
        ports,
        exception,
        hash,
    }
}

fn read_gzip_to_vec(path: &Path) -> Vec<u8> {
    let compressed = fs::read(path).unwrap();
    assert!(
        compressed.len() >= 18,
        "{path:?}: file too short to be a valid gzip stream"
    );
    let isize_le: [u8; 4] = compressed[compressed.len() - 4..].try_into().unwrap();
    let output_len = u32::from_le_bytes(isize_le) as usize;

    let mut output = vec![0u8; output_len];
    let (decompressed, code) =
        decompress_slice(&mut output, &compressed, InflateConfig { window_bits: 31 });
    assert_eq!(code, ReturnCode::Ok, "{path:?}: inflate failed ({code:?})");
    assert_eq!(
        decompressed.len(),
        output_len,
        "{path:?}: decoded size does not match gzip ISIZE"
    );
    output
}

pub fn load_moo_tests(path: &Path, reg_order16: &[&str], reg_order32: &[&str]) -> Vec<MooTest> {
    let data = read_gzip_to_vec(path);

    let mut offset = 0;
    assert_eq!(&data[0..4], b"MOO ");
    offset += 4;

    let header_len = read_u32(&data, &mut offset) as usize;
    offset += header_len;

    let mut tests = Vec::new();
    while offset < data.len() {
        let tag = read_tag(&data, &mut offset);
        let chunk_len = read_u32(&data, &mut offset) as usize;
        let end = offset + chunk_len;

        if &tag == b"TEST" {
            tests.push(parse_test_chunk(
                &data[offset..end],
                reg_order16,
                reg_order32,
            ));
        }

        offset = end;
    }

    tests
}

pub fn load_revocation_list(path: &Path) -> std::collections::HashSet<String> {
    let text = fs::read_to_string(path).unwrap();
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                Some(trimmed.to_ascii_lowercase())
            }
        })
        .collect()
}
