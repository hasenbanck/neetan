//! Country info, DBCS lead byte table, date/time formats.

use crate::MemoryAccess;

pub const COUNTRY_CODE: u16 = 81;
const COUNTRY_INFO_SIZE: u32 = 34;

/// Writes the 34-byte country info buffer (Japan, code 81) to the given linear address.
///
/// Layout:
///   +0x00: date format (WORD) -- 2 = YY/MM/DD
///   +0x02: currency symbol (5 bytes ASCIIZ) -- 0x5C (yen on PC-98)
///   +0x07: thousands separator (2 bytes) -- ","
///   +0x09: decimal separator (2 bytes) -- "."
///   +0x0B: date separator (2 bytes) -- "/"
///   +0x0D: time separator (2 bytes) -- ":"
///   +0x0F: currency format (BYTE) -- 0 (symbol before value)
///   +0x10: decimal places (BYTE) -- 0 (no decimals for yen)
///   +0x11: time format (BYTE) -- 1 (24-hour)
///   +0x12: case map call (DWORD far pointer) -- FFFF:FFFF (null)
///   +0x16: data list separator (2 bytes) -- ","
///   +0x18: reserved (10 bytes of zeros)
pub fn write_country_info(mem: &mut dyn MemoryAccess, addr: u32) {
    // Date format: YY/MM/DD
    mem.write_word(addr, 2);
    // Currency symbol: yen (0x5C on PC-98)
    mem.write_byte(addr + 0x02, 0x5C);
    mem.write_byte(addr + 0x03, 0x00);
    mem.write_byte(addr + 0x04, 0x00);
    mem.write_byte(addr + 0x05, 0x00);
    mem.write_byte(addr + 0x06, 0x00);
    // Thousands separator
    mem.write_byte(addr + 0x07, b',');
    mem.write_byte(addr + 0x08, 0x00);
    // Decimal separator
    mem.write_byte(addr + 0x09, b'.');
    mem.write_byte(addr + 0x0A, 0x00);
    // Date separator
    mem.write_byte(addr + 0x0B, b'/');
    mem.write_byte(addr + 0x0C, 0x00);
    // Time separator
    mem.write_byte(addr + 0x0D, b':');
    mem.write_byte(addr + 0x0E, 0x00);
    // Currency format: symbol before value, no space
    mem.write_byte(addr + 0x0F, 0x00);
    // Decimal places
    mem.write_byte(addr + 0x10, 0x00);
    // Time format: 24-hour
    mem.write_byte(addr + 0x11, 0x01);
    // Case map call: null far pointer
    mem.write_word(addr + 0x12, 0xFFFF);
    mem.write_word(addr + 0x14, 0xFFFF);
    // Data list separator
    mem.write_byte(addr + 0x16, b',');
    mem.write_byte(addr + 0x17, 0x00);
    // Reserved (10 bytes of zeros)
    for i in 0..10u32 {
        mem.write_byte(addr + 0x18 + i, 0x00);
    }
}

/// Writes extended country info (AH=65h AL=01h format) to the given buffer.
///
/// Format:
///   +0x00: info ID byte (01h)
///   +0x01: size (WORD) -- total size including header
///   +0x03: country code (WORD)
///   +0x05: code page (WORD)
///   +0x07: 34-byte country info
///
/// Returns the number of bytes written.
pub fn write_extended_country_info(mem: &mut dyn MemoryAccess, addr: u32, max_bytes: u16) -> u16 {
    let needed = 7 + COUNTRY_INFO_SIZE;
    if (max_bytes as u32) < needed {
        return 0;
    }
    // Info ID
    mem.write_byte(addr, 0x01);
    // Size (includes header bytes 1-6 + 34 bytes country info = 41 total after ID)
    mem.write_word(addr + 1, (needed - 1) as u16);
    // Country code
    mem.write_word(addr + 3, COUNTRY_CODE);
    // Code page (932 = Shift-JIS)
    mem.write_word(addr + 5, 932);
    // Country info
    write_country_info(mem, addr + 7);
    needed as u16
}

/// Writes extended DBCS info (AH=65h AL=07h format) to the given buffer.
///
/// Format:
///   +0x00: info ID byte (07h)
///   +0x01: size (WORD) -- size of following data
///   +0x03: DBCS lead byte ranges (81-9F, E0-FC, 00,00)
///
/// Returns the number of bytes written.
pub fn write_extended_dbcs_info(mem: &mut dyn MemoryAccess, addr: u32, max_bytes: u16) -> u16 {
    let needed: u16 = 9; // 1 (ID) + 2 (size) + 6 (DBCS data)
    if max_bytes < needed {
        return 0;
    }
    mem.write_byte(addr, 0x07);
    mem.write_word(addr + 1, 6); // 6 bytes of DBCS data
    mem.write_byte(addr + 3, 0x81);
    mem.write_byte(addr + 4, 0x9F);
    mem.write_byte(addr + 5, 0xE0);
    mem.write_byte(addr + 6, 0xFC);
    mem.write_byte(addr + 7, 0x00);
    mem.write_byte(addr + 8, 0x00);
    needed
}
