#![cfg(feature = "verification")]

use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone)]
pub struct Metadata {
    pub opcodes: HashMap<String, OpcodeEntry>,
}

#[derive(Debug, Clone)]
pub struct OpcodeEntry {
    pub status: Option<String>,
    pub flags_mask: Option<u16>,
    pub reg: Option<HashMap<String, OpcodeInfo>>,
}

#[derive(Debug, Clone)]
pub struct OpcodeInfo {
    pub status: String,
    pub flags_mask: Option<u16>,
}

#[derive(Debug)]
enum Value {
    Null,
    Bool,
    Number(u64),
    String(String),
    Object(HashMap<String, Value>),
}

struct Parser<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.offset < self.input.len() {
            match self.input[self.offset] {
                b' ' | b'\t' | b'\n' | b'\r' => self.offset += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> u8 {
        self.input[self.offset]
    }

    fn expect(&mut self, byte: u8) {
        assert_eq!(
            self.input[self.offset], byte,
            "expected {:?} at offset {}",
            byte as char, self.offset
        );
        self.offset += 1;
    }

    fn parse_value(&mut self) -> Value {
        self.skip_whitespace();
        match self.peek() {
            b'{' => self.parse_object(),
            b'"' => Value::String(self.parse_string()),
            b't' | b'f' => self.parse_bool(),
            b'n' => self.parse_null(),
            b'0'..=b'9' | b'-' => Value::Number(self.parse_number()),
            other => panic!(
                "unexpected byte {:?} at offset {}",
                other as char, self.offset
            ),
        }
    }

    fn parse_string(&mut self) -> String {
        self.expect(b'"');
        let mut out = Vec::new();
        while self.input[self.offset] != b'"' {
            let byte = self.input[self.offset];
            if byte == b'\\' {
                self.offset += 1;
                let escaped = self.input[self.offset];
                self.offset += 1;
                match escaped {
                    b'"' => out.push(b'"'),
                    b'\\' => out.push(b'\\'),
                    b'/' => out.push(b'/'),
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    other => panic!("unsupported JSON escape \\{}", other as char),
                }
            } else {
                out.push(byte);
                self.offset += 1;
            }
        }
        self.expect(b'"');
        String::from_utf8(out).unwrap()
    }

    fn parse_number(&mut self) -> u64 {
        let start = self.offset;
        if self.input[self.offset] == b'-' {
            self.offset += 1;
        }
        while self.offset < self.input.len() {
            match self.input[self.offset] {
                b'0'..=b'9' => self.offset += 1,
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.input[start..self.offset]).unwrap();
        text.parse::<u64>()
            .unwrap_or_else(|_| panic!("invalid number {text:?}"))
    }

    fn parse_bool(&mut self) -> Value {
        if self.input[self.offset..].starts_with(b"true") {
            self.offset += 4;
            Value::Bool
        } else if self.input[self.offset..].starts_with(b"false") {
            self.offset += 5;
            Value::Bool
        } else {
            panic!("invalid bool literal at offset {}", self.offset);
        }
    }

    fn parse_null(&mut self) -> Value {
        assert!(self.input[self.offset..].starts_with(b"null"));
        self.offset += 4;
        Value::Null
    }

    fn parse_object(&mut self) -> Value {
        self.expect(b'{');
        let mut map = HashMap::new();
        self.skip_whitespace();
        if self.peek() == b'}' {
            self.offset += 1;
            return Value::Object(map);
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string();
            self.skip_whitespace();
            self.expect(b':');
            let value = self.parse_value();
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                b',' => {
                    self.offset += 1;
                }
                b'}' => {
                    self.offset += 1;
                    break;
                }
                other => panic!(
                    "unexpected byte {:?} in object at offset {}",
                    other as char, self.offset
                ),
            }
        }
        Value::Object(map)
    }
}

fn take_object(value: Value) -> HashMap<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected object, got {other:?}"),
    }
}

fn take_string(value: Value) -> String {
    match value {
        Value::String(text) => text,
        other => panic!("expected string, got {other:?}"),
    }
}

fn take_u16(value: Value) -> u16 {
    match value {
        Value::Number(number) => number
            .try_into()
            .unwrap_or_else(|_| panic!("number {number} does not fit in u16")),
        other => panic!("expected number, got {other:?}"),
    }
}

fn build_opcode_info(value: Value) -> OpcodeInfo {
    let mut fields = take_object(value);
    let status = take_string(
        fields
            .remove("status")
            .unwrap_or_else(|| panic!("reg entry missing status")),
    );
    let flags_mask = fields.remove("flags-mask").map(take_u16);
    OpcodeInfo { status, flags_mask }
}

fn build_opcode_entry(value: Value) -> OpcodeEntry {
    let mut fields = take_object(value);
    let status = fields.remove("status").map(take_string);
    let flags_mask = fields.remove("flags-mask").map(take_u16);
    let reg = fields.remove("reg").map(|reg_value| {
        take_object(reg_value)
            .into_iter()
            .map(|(name, entry)| (name, build_opcode_info(entry)))
            .collect()
    });
    OpcodeEntry {
        status,
        flags_mask,
        reg,
    }
}

pub fn load_metadata(path: &Path) -> Metadata {
    let raw = fs::read(path).unwrap();
    let mut parser = Parser::new(&raw);
    let mut root = take_object(parser.parse_value());
    let opcodes_value = root
        .remove("opcodes")
        .unwrap_or_else(|| panic!("metadata.json has no opcodes field"));
    let opcodes = take_object(opcodes_value)
        .into_iter()
        .map(|(opcode, entry)| (opcode, build_opcode_entry(entry)))
        .collect();
    Metadata { opcodes }
}
