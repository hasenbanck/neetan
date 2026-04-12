pub(crate) fn blake3_digest(data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    let mut digest = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut digest);
    digest
}

#[cfg(test)]
pub(crate) fn blake3_digest_to_hex_string(digest: &[u8; 32]) -> [u8; 64] {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex_string = [0u8; 64];
    for (index, digest_byte) in digest.iter().copied().enumerate() {
        hex_string[index * 2] = HEX_DIGITS[((digest_byte >> 4) & 0x0F) as usize];
        hex_string[index * 2 + 1] = HEX_DIGITS[(digest_byte & 0x0F) as usize];
    }
    hex_string
}

pub(crate) fn blake3_digest_from_hex(hex_string: &str) -> [u8; 32] {
    let mut digest = [0u8; 32];
    let hex_bytes = hex_string.as_bytes();
    for index in 0..32 {
        let high_nibble = hex_digit(hex_bytes[index * 2]);
        let low_nibble = hex_digit(hex_bytes[index * 2 + 1]);
        digest[index] = (high_nibble << 4) | low_nibble;
    }
    digest
}

fn hex_digit(character: u8) -> u8 {
    match character {
        b'0'..=b'9' => character - b'0',
        b'a'..=b'f' => character - b'a' + 10,
        b'A'..=b'F' => character - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blake3_empty() {
        let digest = blake3_digest(&[]);
        let hex_string = blake3_digest_to_hex_string(&digest);
        let utf8_hex_string = core::str::from_utf8(&hex_string).unwrap();
        assert_eq!(
            utf8_hex_string,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn test_blake3_abc() {
        let digest = blake3_digest(b"abc");
        let hex_string = blake3_digest_to_hex_string(&digest);
        let utf8_hex_string = core::str::from_utf8(&hex_string).unwrap();
        assert_eq!(
            utf8_hex_string,
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
    }

    #[test]
    fn test_blake3_from_hex_roundtrip() {
        let original_hex_string =
            "9102699229706ff459a718924884559d50a6a8749a2d27fe58548f3c0606f66a";
        let digest = blake3_digest_from_hex(original_hex_string);
        let hex_string = blake3_digest_to_hex_string(&digest);
        let utf8_hex_string = core::str::from_utf8(&hex_string).unwrap();
        assert_eq!(utf8_hex_string, original_hex_string);
    }

    #[test]
    fn test_blake3_additional_vectors() {
        let cases: &[(&[u8], &str)] = &[
            (
                &[0u8; 256],
                "bdc73c75432532814ec2d008761b965a6d8e4193f4e2a3cf4ff2d9701c6c607c",
            ),
            (
                &[0x41u8; 64],
                "e028424e46205e56b2ed1ce1bf7087054072e6c4e41f843bed1e749db635792c",
            ),
            (
                &[0x42u8; 65],
                "25bf6528ad7860999d1a0af1b04a1eedac3ebdd1dff975b69bcb422041b7e3fe",
            ),
        ];

        for &(input, expected_hex_string) in cases {
            let digest = blake3_digest(input);
            let hex_string = blake3_digest_to_hex_string(&digest);
            let utf8_hex_string = core::str::from_utf8(&hex_string).unwrap();
            assert_eq!(
                utf8_hex_string,
                expected_hex_string,
                "BLAKE3 mismatch for input len {}",
                input.len()
            );
        }
    }
}
