//! Country info, DBCS lead byte table, date/time formats.

use crate::MemoryAccess;

pub const COUNTRY_CODE: u16 = 81;
const COUNTRY_INFO_SIZE: u32 = 34;

/// Shift-JIS DBCS lead byte ranges (81-9F, E0-FC) with double-null terminator.
pub const DBCS_LEAD_BYTES: [u8; 6] = [0x81, 0x9F, 0xE0, 0xFC, 0x00, 0x00];

/// Writes the 34-byte country info buffer (Japan, code 81) to the given linear address.
pub fn write_country_info(mem: &mut dyn MemoryAccess, addr: u32) {
    #[rustfmt::skip]
    const COUNTRY_INFO: [u8; 34] = [
        0x02, 0x00,                         // +0x00: date format (YY/MM/DD)
        0x5C, 0x00, 0x00, 0x00, 0x00,       // +0x02: currency symbol (yen on PC-98)
        b',', 0x00,                         // +0x07: thousands separator
        b'.', 0x00,                         // +0x09: decimal separator
        b'/', 0x00,                         // +0x0B: date separator
        b':', 0x00,                         // +0x0D: time separator
        0x00,                               // +0x0F: currency format (symbol before value)
        0x00,                               // +0x10: decimal places (0 for yen)
        0x01,                               // +0x11: time format (24-hour)
        0xFF, 0xFF, 0xFF, 0xFF,             // +0x12: case map call (null far pointer)
        b',', 0x00,                         // +0x16: data list separator
        0x00, 0x00, 0x00, 0x00, 0x00,       // +0x18: reserved
        0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    mem.write_block(addr, &COUNTRY_INFO);
}

/// Writes extended country info (AH=65h AL=01h format) to the given buffer.
///
/// Format:
///   +0x00: info ID byte (01h)
///   +0x01: size (WORD) - total size including header
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
///   +0x01: size (WORD) - size of following data
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
    mem.write_block(addr + 3, &DBCS_LEAD_BYTES);
    needed
}
