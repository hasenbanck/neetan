//! SHA-1 implementation.
//!
//! Original C code copyright (c) 2011, Micael Hildenborg.
//!
//! BSD licensed. Contributors: Gustav, several members in the gamedev.se forum, Gregory Petrosyan.

/// Rotate an integer value to left.
fn rol(value: u32, steps: u32) -> u32 {
    value.rotate_left(steps)
}

/// Sets the first 16 integers in the buffer to zero.
/// Used for clearing the W buffer.
fn clear_w_buffer(buffer: &mut [u32; 80]) {
    buffer[..16].fill(0);
}

fn inner_hash(result: &mut [u32; 5], w: &mut [u32; 80]) {
    let mut a = result[0];
    let mut b = result[1];
    let mut c = result[2];
    let mut d = result[3];
    let mut e = result[4];

    let mut round = 0usize;

    macro_rules! sha1macro {
        ($func:expr, $val:expr) => {{
            let t = rol(a, 5)
                .wrapping_add($func)
                .wrapping_add(e)
                .wrapping_add($val)
                .wrapping_add(w[round]);
            e = d;
            d = c;
            c = rol(b, 30);
            b = a;
            a = t;
        }};
    }

    while round < 16 {
        sha1macro!((b & c) | (!b & d), 0x5A827999u32);
        round += 1;
    }
    while round < 20 {
        w[round] = rol(
            w[round - 3] ^ w[round - 8] ^ w[round - 14] ^ w[round - 16],
            1,
        );
        sha1macro!((b & c) | (!b & d), 0x5A827999u32);
        round += 1;
    }
    while round < 40 {
        w[round] = rol(
            w[round - 3] ^ w[round - 8] ^ w[round - 14] ^ w[round - 16],
            1,
        );
        sha1macro!(b ^ c ^ d, 0x6ED9EBA1u32);
        round += 1;
    }
    while round < 60 {
        w[round] = rol(
            w[round - 3] ^ w[round - 8] ^ w[round - 14] ^ w[round - 16],
            1,
        );
        sha1macro!((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32);
        round += 1;
    }
    while round < 80 {
        w[round] = rol(
            w[round - 3] ^ w[round - 8] ^ w[round - 14] ^ w[round - 16],
            1,
        );
        sha1macro!(b ^ c ^ d, 0xCA62C1D6u32);
        round += 1;
    }

    result[0] = result[0].wrapping_add(a);
    result[1] = result[1].wrapping_add(b);
    result[2] = result[2].wrapping_add(c);
    result[3] = result[3].wrapping_add(d);
    result[4] = result[4].wrapping_add(e);
}

/// Compute SHA-1 hash of the given byte slice.
/// Returns a 20-byte digest.
pub(crate) fn sha1_calc(src: &[u8]) -> [u8; 20] {
    // Init the result array.
    let mut result: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    // The reusable round buffer.
    let mut w = [0u32; 80];

    let bytelength = src.len();

    // Loop through all complete 64byte blocks.
    let end_of_full_blocks: isize = bytelength as isize - 64;
    let mut current_block: isize = 0;

    while current_block <= end_of_full_blocks {
        let end_current_block = current_block as usize + 64;

        // Init the round buffer with the 64 byte block data.
        let mut round_pos = 0;
        let mut cb = current_block as usize;
        while cb < end_current_block {
            // This line will swap endian on big endian and keep endian on little endian.
            w[round_pos] = (src[cb + 3] as u32)
                | ((src[cb + 2] as u32) << 8)
                | ((src[cb + 1] as u32) << 16)
                | ((src[cb] as u32) << 24);
            round_pos += 1;
            cb += 4;
        }
        current_block = cb as isize;
        inner_hash(&mut result, &mut w);
    }

    // Handle the last and not full 64 byte block if existing.
    let current_block = current_block as usize;
    let end_current_block = bytelength - current_block;
    clear_w_buffer(&mut w);
    let mut last_block_bytes: usize = 0;
    while last_block_bytes < end_current_block {
        w[last_block_bytes >> 2] |=
            (src[last_block_bytes + current_block] as u32) << ((3 - (last_block_bytes & 3)) << 3);
        last_block_bytes += 1;
    }
    w[last_block_bytes >> 2] |= 0x80u32 << ((3 - (last_block_bytes & 3)) << 3);
    if end_current_block >= 56 {
        inner_hash(&mut result, &mut w);
        clear_w_buffer(&mut w);
    }
    w[15] = (bytelength << 3) as u32;
    inner_hash(&mut result, &mut w);

    // Store hash in result pointer, and make sure we get in in the correct order on both endian models.
    let mut hash = [0u8; 20];
    for hash_byte in (0..20).rev() {
        hash[hash_byte] = ((result[hash_byte >> 2]
            >> ((3u32.wrapping_sub(hash_byte as u32) & 0x3) << 3))
            & 0xFF) as u8;
    }
    hash
}

/// Convert a 20-byte SHA-1 hash to a 40-character lowercase hex string.
#[cfg(test)]
pub(crate) fn sha1_to_hex_string(hash: &[u8; 20]) -> [u8; 40] {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = [0u8; 40];
    for hash_byte in (0..20).rev() {
        hex[hash_byte << 1] = HEX_DIGITS[((hash[hash_byte] >> 4) & 0xF) as usize];
        hex[(hash_byte << 1) + 1] = HEX_DIGITS[(hash[hash_byte] & 0xF) as usize];
    }
    hex
}

/// Parse a hex string (40 chars) into a 20-byte SHA-1 digest.
pub(crate) fn sha1_from_hex(hex: &str) -> [u8; 20] {
    let mut digest = [0u8; 20];
    let bytes = hex.as_bytes();
    for i in 0..20 {
        let hi = hex_digit(bytes[i * 2]);
        let lo = hex_digit(bytes[i * 2 + 1]);
        digest[i] = (hi << 4) | lo;
    }
    digest
}

fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha1::sha1_to_hex_string;

    #[test]
    fn test_sha1_empty() {
        // SHA-1 of empty string: da39a3ee5e6b4b0d3255bfef95601890afd80709
        let hash = sha1_calc(&[]);
        let hex = sha1_to_hex_string(&hash);
        let hex_str = core::str::from_utf8(&hex).unwrap();
        assert_eq!(hex_str, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn test_sha1_abc() {
        // SHA-1 of "abc": a9993e364706816aba3e25717850c26c9cd0d89d
        let hash = sha1_calc(b"abc");
        let hex = sha1_to_hex_string(&hash);
        let hex_str = core::str::from_utf8(&hex).unwrap();
        assert_eq!(hex_str, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn test_sha1_from_hex_roundtrip() {
        let original = "5a5cb5a77d7d55ee69657c2f870416daed52dea7";
        let digest = sha1_from_hex(original);
        let hex = sha1_to_hex_string(&digest);
        let hex_str = core::str::from_utf8(&hex).unwrap();
        assert_eq!(hex_str, original);
    }

    #[test]
    fn test_sha1_additional_vectors() {
        // Standard SHA-1 hashes, verified against both C++ munt sha1::calc and Python hashlib.
        let cases: &[(&[u8], &str)] = &[
            (
                b"The quick brown fox jumps over the lazy dog",
                "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12",
            ),
            (&[0u8; 256], "b376885ac8452b6cbf9ced81b1080bfd570d9b91"),
            (
                &[0x41u8; 64], // exactly one SHA-1 block
                "30b86e44e6001403827a62c58b08893e77cf121f",
            ),
            (
                &[0x42u8; 65], // one block + 1 byte
                "550fdc7cb0c34885cf8632c33c7057947578142b",
            ),
        ];
        for &(input, expected_hex) in cases {
            let hash = sha1_calc(input);
            let hex = sha1_to_hex_string(&hash);
            let hex_str = core::str::from_utf8(&hex).unwrap();
            assert_eq!(
                hex_str,
                expected_hex,
                "SHA-1 mismatch for input len {}",
                input.len()
            );
        }
    }
}
