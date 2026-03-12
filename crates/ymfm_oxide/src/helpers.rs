// Extract a bitfield from the given value, starting at bit `start` for `length` bits.
pub(crate) fn bitfield(value: u32, start: i32, length: i32) -> u32 {
    (value >> start) & ((1 << length) - 1)
}

pub(crate) fn bit(value: u32, start: i32) -> u32 {
    (value >> start) & 1
}

pub(crate) fn clamp(value: i32, minval: i32, maxval: i32) -> i32 {
    if value < minval {
        minval
    } else if value > maxval {
        maxval
    } else {
        value
    }
}

// Return the number of leading zeros in a 32-bit value.
pub(crate) fn count_leading_zeros(value: u32) -> u8 {
    if value == 0 {
        32
    } else {
        value.leading_zeros() as u8
    }
}

// Simulate the precision loss of the OPN DAC's floating-point round trip
// (encode to 3.10 FP then decode back). The Yamaha FM chips emit a
// floating-point value to the DAC. This description only makes sense if the
// "internal" format treats sign as 1=positive and 0=negative, so the helper
// below presumes that.
//
// Internal OPx data      16-bit signed data     Exp Sign Mantissa
// =================      =================      === ==== ========
// 1 1xxxxxxxx------  ->  0 1xxxxxxxx------  ->  111   1  1xxxxxxx
// 1 01xxxxxxxx-----  ->  0 01xxxxxxxx-----  ->  110   1  1xxxxxxx
// 1 001xxxxxxxx----  ->  0 001xxxxxxxx----  ->  101   1  1xxxxxxx
// 1 0001xxxxxxxx---  ->  0 0001xxxxxxxx---  ->  100   1  1xxxxxxx
// 1 00001xxxxxxxx--  ->  0 00001xxxxxxxx--  ->  011   1  1xxxxxxx
// 1 000001xxxxxxxx-  ->  0 000001xxxxxxxx-  ->  010   1  1xxxxxxx
// 1 000000xxxxxxxxx  ->  0 000000xxxxxxxxx  ->  001   1  xxxxxxxx
// 0 111111xxxxxxxxx  ->  1 111111xxxxxxxxx  ->  001   0  xxxxxxxx
// 0 111110xxxxxxxx-  ->  1 111110xxxxxxxx-  ->  010   0  0xxxxxxx
// 0 11110xxxxxxxx--  ->  1 11110xxxxxxxx--  ->  011   0  0xxxxxxx
// 0 1110xxxxxxxx---  ->  1 1110xxxxxxxx---  ->  100   0  0xxxxxxx
// 0 110xxxxxxxx----  ->  1 110xxxxxxxx----  ->  101   0  0xxxxxxx
// 0 10xxxxxxxx-----  ->  1 10xxxxxxxx-----  ->  110   0  0xxxxxxx
// 0 0xxxxxxxx------  ->  1 0xxxxxxxx------  ->  111   0  0xxxxxxx
pub(crate) fn roundtrip_fp(value: i32) -> i16 {
    if value < -32768 {
        return -32768;
    }
    if value > 32767 {
        return 32767;
    }
    // We can use count_leading_zeros if we invert negative values.
    let scanvalue = value ^ (value >> 31);
    // Exponent is related to the number of leading bits starting from bit 14.
    let mut exponent = 7i32 - count_leading_zeros((scanvalue << 17) as u32) as i32;
    // Smallest exponent value allowed is 1.
    if exponent < 1 {
        exponent = 1;
    }
    // Apply the shift back and forth to zero out bits that are lost.
    let exponent = (exponent - 1) as u32;
    let mask = (1i32 << exponent) - 1;
    (value & !mask) as i16
}
