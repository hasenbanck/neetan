# x87 FPU Specification

Primary sources:

- Intel, i486 Processor Programmer's Reference Manual_ (1990), Chapter 16 "Numeric Processor".
- Cycle timings: https://www2.math.uni-wuppertal.de/~fpf/Uebungen/GdR-SS02/opcode_f.html

## Scope

This document specifies the x87 FPU hardware behavior for the Intel 386DX (external 80387
coprocessor) and 486DX (on-chip FPU), targeting the `CPU_MODEL_386DX` and `CPU_MODEL_486DX`
const generic variants of `I386<CPU_MODEL>` in the neetan CPU crate. All soft-float operations
(`Fp80` type, arithmetic, conversions, comparisons, transcendentals) are implemented in the
`softfloat` crate (`crates/softfloat/`).

It covers:

1. The 80-bit extended precision format (encoding, value classes, classification).
2. All FP80 operations required by the x87 instruction set (special-value behavior, exception rules).
3. The x87 FPU state machine (registers, control/status/tag words, exceptions).
4. All x87 instructions with encodings, behavior, and cycle timings (387 and 486).
5. Escape opcode dispatch tables (D8–DF).

**Excluded (post-486DX):**

- FCMOVcc conditional moves (8 variants) - Pentium Pro / P6+.
- FCOMI, FCOMIP, FUCOMI, FUCOMIP - Pentium Pro / P6+.
- FXSAVE, FXRSTOR - Pentium+.

---

## 1. FP80 Type Specification

### 1.1 Encoding

The 80-bit extended precision format uses 10 bytes:

```
Bit 79        Bits 78..64       Bit 63          Bits 62..0
┌──────┐  ┌───────────────┐  ┌──────────┐  ┌──────────────────┐
│ Sign │  │   Exponent    │  │  J-bit   │  │    Fraction      │
│ 1 bit│  │   15 bits     │  │ (integer)│  │    63 bits       │
└──────┘  └───────────────┘  └──────────┘  └──────────────────┘
```

- **Sign** (bit 79): 0 = positive, 1 = negative.
- **Exponent** (bits 78–64): biased by 16383 (0x3FFF). Range 0x0000–0x7FFF.
- **J-bit** (bit 63): the **explicit** integer bit of the significand. Unlike IEEE 754 binary32/binary64,
  this bit is not implicit - it is stored and must be maintained by software.
- **Fraction** (bits 62–0): the fractional part of the significand.

The **significand** is bits 63–0 (J-bit concatenated with fraction), giving 64 bits of precision.

**Exponent bias:** 16383. The true exponent of a normal number is `biased_exponent - 16383`.

**Memory layout (little-endian):** 8 bytes significand (bits 63–0) followed by 2 bytes sign+exponent (bits 79–64).

### 1.2 Rust Representation

The `Fp80` struct, all supporting types (`RoundingMode`, `Precision`, `ExceptionFlags`,
`FpOrdering`, `FpClass`), and classification predicates listed in section 1.4 are implemented
in the `softfloat` crate (`crates/softfloat/src/lib.rs`).

### 1.3 Value Classes

| Class                | Sign | Exponent      | J-bit | Fraction             | Notes                                             |
| -------------------- | ---- | ------------- | ----- | -------------------- |---------------------------------------------------|
| Positive zero        | 0    | 0x0000        | 0     | 0                    | `+0.0`                                            |
| Negative zero        | 1    | 0x0000        | 0     | 0                    | `-0.0`                                            |
| Positive denormal    | 0    | 0x0000        | 0     | nonzero              | Subnormal, effective exponent = -16382            |
| Negative denormal    | 1    | 0x0000        | 0     | nonzero              | Subnormal, effective exponent = -16382            |
| Pseudo-denormal      | 0/1  | 0x0000        | 1     | any                  | J=1 with zero exponent; treated as denormal       |
| Positive normal      | 0    | 0x0001–0x7FFE | 1     | any                  | Standard normalized number                        |
| Negative normal      | 1    | 0x0001–0x7FFE | 1     | any                  | Standard normalized number                        |
| Unnormal             | 0/1  | 0x0001–0x7FFE | 0     | any                  | J=0 with nonzero exponent; unsupported on 386/486 |
| Positive infinity    | 0    | 0x7FFF        | 1     | 0                    | `+∞` (significand = 0x8000000000000000)           |
| Negative infinity    | 1    | 0x7FFF        | 1     | 0                    | `-∞` (significand = 0x8000000000000000)           |
| Pseudo-infinity      | 0/1  | 0x7FFF        | 0     | 0                    | J=0 with max exponent and zero fraction           |
| Quiet NaN (QNaN)     | 0/1  | 0x7FFF        | 1     | bit 62 = 1           | Does not signal IE on use                         |
| Signaling NaN (SNaN) | 0/1  | 0x7FFF        | 1     | bit 62 = 0, frac ≠ 0 | Signals IE on use                                 |
| Pseudo-NaN           | 0/1  | 0x7FFF        | 0     | nonzero              | J=0 with max exponent; unsupported                |

### 1.4 Classification Predicates

| Predicate            | Condition                                                       |
| -------------------- | --------------------------------------------------------------- |
| `is_zero`            | exponent = 0 AND significand = 0                                |
| `is_denormal`        | exponent = 0 AND J-bit = 0 AND fraction ≠ 0                     |
| `is_pseudo_denormal` | exponent = 0 AND J-bit = 1                                      |
| `is_normal`          | exponent ∈ [0x0001, 0x7FFE] AND J-bit = 1                       |
| `is_unnormal`        | exponent ∈ [0x0001, 0x7FFE] AND J-bit = 0                       |
| `is_infinity`        | exponent = 0x7FFF AND significand = 0x8000000000000000          |
| `is_nan`             | exponent = 0x7FFF AND (significand & 0x7FFFFFFFFFFFFFFF) ≠ 0    |
| `is_signaling_nan`   | exponent = 0x7FFF AND J-bit = 1 AND bit 62 = 0 AND fraction ≠ 0 |
| `is_quiet_nan`       | exponent = 0x7FFF AND J-bit = 1 AND bit 62 = 1                  |
| `is_unsupported`     | `is_unnormal` OR `is_pseudo_infinity` OR `is_pseudo_nan`        |
| `is_negative`        | sign bit = 1                                                    |

### 1.5 Default NaN (Indefinite)

The x86 default NaN returned for invalid operations:

```
sign_exponent = 0xFFFF    (sign=1, exponent=0x7FFF)
significand   = 0xC000000000000000  (J=1, bit62=1, rest=0)
```

This is a negative quiet NaN with zero payload.

### 1.6 Built-in Constants

These are the exact bit patterns pushed by the FLDxxx instructions. Constants marked with `†` have
rounding-mode-dependent least significant bits.

| Constant    | Value   | sign_exponent | significand (higher) | significand (lower)  |
| ----------- | ------- | ------------- | -------------------- |----------------------|
| +0.0        | 0       | `0x0000`      | `0x0000000000000000` | -                    |
| +1.0        | 1       | `0x3FFF`      | `0x8000000000000000` | -                    |
| log₂(10) †¹ | 3.3219… | `0x4000`      | `0xD49A784BCD1B8AFF` | `0xD49A784BCD1B8AFE` |
| log₂(e) †²  | 1.4427… | `0x3FFF`      | `0xB8AA3B295C17F0BC` | `0xB8AA3B295C17F0BB` |
| π †²        | 3.1415… | `0x4000`      | `0xC90FDAA22168C235` | `0xC90FDAA22168C234` |
| log₁₀(2) †² | 0.3010… | `0x3FFD`      | `0x9A209A84FBCFF799` | `0x9A209A84FBCFF798` |
| ln(2) †²    | 0.6931… | `0x3FFE`      | `0xB17217F7D1CF79AC` | `0xB17217F7D1CF79AB` |

The `+0.0` and `+1.0` constants are exact and do not depend on rounding mode. Rounding-mode-dependent
constants use different significands depending on the current RC setting:

- **†¹ log₂(10):** Higher significand for Round Up only. Lower significand for Round to Nearest,
  Round Down, and Round toward Zero. (The true value is closer to the lower significand.)
- **†² All others:** Higher significand for Round Up and Round to Nearest. Lower significand for
  Round Down and Round toward Zero.

---

## 2. Rounding and Precision Control

### 2.1 Rounding Modes

Selected by CW bits 11–10 (RC field):

| RC  | Mode                  | Description                                                                                                                     |
| --- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| 00  | Round to nearest even | Default. If the value is exactly halfway between two representable values, round to the one with an even least significant bit. |
| 01  | Round toward −∞       | Floor. Always round toward negative infinity.                                                                                   |
| 10  | Round toward +∞       | Ceiling. Always round toward positive infinity.                                                                                 |
| 11  | Round toward zero     | Truncate. Discard fractional bits toward zero.                                                                                  |

### 2.2 Precision Control

Selected by CW bits 9–8 (PC field):

| PC  | Precision | Significand bits | Exponent range |
| --- | --------- | ---------------- | -------------- |
| 00  | Single    | 24               | Full 15-bit    |
| 01  | Reserved  | Treat as Double  | -              |
| 10  | Double    | 53               | Full 15-bit    |
| 11  | Extended  | 64 (default)     | Full 15-bit    |

**Affected operations:** `add`, `sub`, `mul`, `div`, `sqrt`.

**Unaffected operations:** all others (transcendentals, loads, stores, comparisons, constants, etc.).

When precision control is set to Single or Double, the result significand is rounded to the specified
number of bits. The exponent range remains the full 15-bit extended range - only the significand
precision is reduced. This differs from actually performing the operation in IEEE binary32/binary64.

---

## 3. Exception System

### 3.1 Exception Flags

Six exception types, each with a flag bit in the Status Word (SW) and a mask bit in the Control Word (CW):

| Exception                 | SW bit | CW mask    | Condition                                                                                                      |
| ------------------------- | ------ | ---------- | -------------------------------------------------------------------------------------------------------------- |
| Invalid Operation (IE)    | 0      | IM (bit 0) | SNaN operand, unsupported format, stack overflow/underflow, invalid operation (0/0, ∞−∞, ∞×0, sqrt(neg), etc.) |
| Denormalized Operand (DE) | 1      | DM (bit 1) | One or both operands are denormalized                                                                          |
| Zero Divide (ZE)          | 2      | ZM (bit 2) | Finite nonzero dividend / zero divisor                                                                         |
| Overflow (OE)             | 3      | OM (bit 3) | Result magnitude exceeds the destination format's maximum finite value                                         |
| Underflow (UE)            | 4      | UM (bit 4) | Result is nonzero but too small to represent as a normalized number                                            |
| Precision (PE)            | 5      | PM (bit 5) | Result was rounded (not exact)                                                                                 |

### 3.2 Stack Fault (SF)

The Stack Fault flag (SW bit 6) is set alongside IE when:

- **Stack overflow:** a push operation (`FLD`, `FILD`, etc.) targets a non-empty register.
  C1 is set to 1.
- **Stack underflow:** a read operation references an empty register.
  C1 is cleared to 0.

SF is a sub-case of IE - it is always accompanied by IE being set.

### 3.3 Error Summary (ES)

ES (SW bit 7) indicates whether any unmasked exception is pending:

```
ES = (SW & ~CW & 0x3F) != 0
```

The Busy flag (SW bit 15) mirrors ES on the 386/486.

### 3.4 Masked Exception Responses

When an exception is masked (corresponding CW mask bit = 1), the FPU produces a default result
instead of signaling to the CPU:

| Exception        | Masked Response                                                                                                                                                                        |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| IE (invalid)     | Return the default NaN (indefinite): `0xFFFF_C000_0000_0000_0000`. For stack faults, the indefinite value is placed in the destination.                                                |
| IE (SNaN)        | Quieten the SNaN by setting bit 62 of the significand. Return the resulting QNaN.                                                                                                      |
| DE (denormal)    | Proceed with the operation using the denormal operand as-is.                                                                                                                           |
| ZE (zero divide) | Return ±∞ with the sign determined by XOR of operand signs.                                                                                                                            |
| OE (overflow)    | Return ±∞ (for round-to-nearest and round-toward-infinity in the overflow direction), or ±MAX_FINITE (for round-toward-zero and round-toward-infinity opposite to overflow direction). |
| UE (underflow)   | Return the denormalized result (gradual underflow).                                                                                                                                    |
| PE (precision)   | Return the rounded result.                                                                                                                                                             |

### 3.5 Unmasked Exception Behavior

When an exception is unmasked (CW mask bit = 0) and the exception fires:

- **CR0.NE = 1 (native mode):** The CPU raises exception #MF (Math Fault, vector 16).
- **CR0.NE = 0 (DOS-compatible mode):** The FPU asserts the FERR# pin, which is typically routed
  to IRQ 13 via external logic.

### 3.6 CR0 Interactions

| CR0 bit | Name                     | Effect on x87                                                                     |
| ------- | ------------------------ | --------------------------------------------------------------------------------- |
| Bit 1   | MP (Monitor coProcessor) | When MP=1 AND TS=1, WAIT/FWAIT generates #NM.                                     |
| Bit 2   | EM (Emulation)           | When EM=1, all ESC opcodes (D8–DF) generate #NM. Used for software FPU emulation. |
| Bit 3   | TS (Task Switched)       | When TS=1, ESC opcodes generate #NM. Allows lazy FPU context switching.           |
| Bit 4   | ET (Extension Type)      | Always 1 when FPU present (486DX). 0 on 486SX. On 386, indicates 387 vs 287.      |
| Bit 5   | NE (Numeric Error)       | Controls FPU error reporting: 1 = native #MF, 0 = external FERR#.                 |

**ESC opcode behavior:**

1. If CR0.EM = 1 → #NM (device not available, vector 7).
2. Else if CR0.TS = 1 → #NM.
3. Else if ES = 1 (pending unmasked exception) → #MF (vector 16) or FERR#.
4. Else → execute the FPU instruction.

**WAIT/FWAIT behavior:**

1. If CR0.MP = 1 AND CR0.TS = 1 → #NM.
2. Else if ES = 1 (pending unmasked exception) → #MF (vector 16) or FERR#.
3. Else → no operation.

---

## 4. x87 FPU State

### 4.1 Register Stack

Eight 80-bit physical registers R0–R7. Access is stack-relative through the TOP pointer:

```
ST(i) = R[(TOP + i) mod 8]
```

TOP is a 3-bit field in SW bits 13–11.

- **Push (decrement TOP):** TOP = (TOP − 1) mod 8. The new ST(0) is written.
- **Pop (increment TOP):** The old ST(0) is freed, TOP = (TOP + 1) mod 8.

### 4.2 Control Word (CW)

| Bits  | Name | Description                                                       |
| ----- | ---- | ----------------------------------------------------------------- |
| 0     | IM   | Invalid Operation mask (1 = masked)                               |
| 1     | DM   | Denormalized Operand mask                                         |
| 2     | ZM   | Zero Divide mask                                                  |
| 3     | OM   | Overflow mask                                                     |
| 4     | UM   | Underflow mask                                                    |
| 5     | PM   | Precision mask                                                    |
| 6–7   | -    | Reserved                                                          |
| 8–9   | PC   | Precision Control: 00=Single, 01=Reserved, 10=Double, 11=Extended |
| 10–11 | RC   | Rounding Control: 00=Nearest, 01=Down, 10=Up, 11=Zero             |
| 12    | IC   | Infinity Control (ignored on 386/486, included for compatibility) |
| 13–15 | -    | Reserved                                                          |

**Reset value:** `0x037F` (all exceptions masked, extended precision, round-to-nearest).

### 4.3 Status Word (SW)

| Bits  | Name | Description                         |
| ----- | ---- | ----------------------------------- |
| 0     | IE   | Invalid Operation exception flag    |
| 1     | DE   | Denormalized Operand exception flag |
| 2     | ZE   | Zero Divide exception flag          |
| 3     | OE   | Overflow exception flag             |
| 4     | UE   | Underflow exception flag            |
| 5     | PE   | Precision exception flag            |
| 6     | SF   | Stack Fault                         |
| 7     | ES   | Error Summary                       |
| 8     | C0   | Condition Code 0                    |
| 9     | C1   | Condition Code 1                    |
| 10    | C2   | Condition Code 2                    |
| 11–13 | TOP  | Stack Top Pointer (0–7)             |
| 14    | C3   | Condition Code 3                    |
| 15    | B    | Busy (mirrors ES on the 386/486)    |

**Reset value:** `0x0000`.

### 4.4 Tag Word (TW)

16 bits total, 2 bits per physical register R0–R7:

| Tag value | Meaning                                                       |
| --------- | ------------------------------------------------------------- |
| 00        | Valid - contains a normal finite value                        |
| 01        | Zero - contains +0.0 or -0.0                                  |
| 10        | Special - contains NaN, infinity, denormal, or unsupported format |
| 11        | Empty - register is uninitialized                             |

Bits 1–0 correspond to R0, bits 3–2 to R1, … , bits 15–14 to R7.

**Reset value:** `0xFFFF` (all registers tagged Empty).

**Tag assignment rules:** When writing a value to a register, the tag is set based on the value class:

- Zero → tag = 01 (Zero)
- NaN, infinity, denormal, unnormal, unsupported → tag = 10 (Special)
- Normal → tag = 00 (Valid)
- On FFREE or FINIT → tag = 11 (Empty)

### 4.5 Additional State

| State                         | Size    | Description                                                                      |
| ----------------------------- | ------- | -------------------------------------------------------------------------------- |
| FPU Instruction Pointer (FIP) | 48 bits | 32-bit offset + 16-bit CS selector of the last non-control FPU instruction       |
| FPU Data Pointer (FDP)        | 48 bits | 32-bit offset + 16-bit segment selector of the last memory operand               |
| FPU Opcode                    | 11 bits | Low 11 bits of the ESC opcode (first byte bits 2–0 concatenated with ModRM byte) |

These are updated for every FPU instruction except FINIT, FCLEX, FLDCW, FSTCW, FSTSW, FSTENV,
FLDENV, FSAVE, FRSTOR, FWAIT, and FNOP.

### 4.6 FINIT Reset State

| State  | Value    |
| ------ | -------- |
| CW     | `0x037F` |
| SW     | `0x0000` |
| TW     | `0xFFFF` |
| FIP    | `0`      |
| FDP    | `0`      |
| Opcode | `0`      |

---

## 5. Softfloat Crate Reference

All Fp80 operations specified in section 6 are implemented in the `softfloat` crate
(`crates/softfloat/`). The crate provides:

- `Fp80` type with constructors, classification predicates, and serialization.
- Supporting types: `RoundingMode`, `Precision`, `ExceptionFlags`, `FpOrdering`, `FpClass`.
- Core arithmetic: `add`, `sub`, `mul`, `div`, `sqrt`, `round_to_int`.
- Conversions: `from_i16`/`from_i32`/`from_i64`, `to_i16`/`to_i32`/`to_i64`,
  `from_f32`/`from_f64`, `to_f32`/`to_f64`, `from_bcd`/`to_bcd`.
- Comparisons: `compare` (ordered), `compare_quiet` (unordered).
- Transcendentals: `f2xm1`, `fyl2x`, `fyl2xp1`, `fsin`, `fcos`, `fsincos`, `fptan`, `fpatan`.
- Other: `scale`, `extract`, `partial_remainder`, `ieee_remainder`.
- Bitwise: `negate`, `abs`.
- NaN handling: `quieten`, `propagate_nan`.

### 5.1 x87 Instruction to Fp80 Method Mapping

| x87 Instruction       | Fp80 Method                | Notes                                                   |
| --------------------- | -------------------------- | ------------------------------------------------------- |
| FLD m32real           | `Fp80::from_f32`           |                                                         |
| FLD m64real           | `Fp80::from_f64`           |                                                         |
| FLD m80real           | `Fp80::from_le_bytes`      |                                                         |
| FLD ST(i)             | (register copy)            | No Fp80 method needed                                   |
| FILD m16int           | `Fp80::from_i16`           |                                                         |
| FILD m32int           | `Fp80::from_i32`           |                                                         |
| FILD m64int           | `Fp80::from_i64`           |                                                         |
| FBLD                  | `Fp80::from_bcd`           |                                                         |
| FST/FSTP m32real      | `to_f32`                   |                                                         |
| FST/FSTP m64real      | `to_f64`                   |                                                         |
| FST/FSTP m80real      | `to_le_bytes`              |                                                         |
| FST/FSTP ST(i)        | (register copy)            | No Fp80 method needed                                   |
| FIST/FISTP m16int     | `to_i16`                   |                                                         |
| FIST/FISTP m32int     | `to_i32`                   |                                                         |
| FISTP m64int          | `to_i64`                   |                                                         |
| FBSTP                 | `to_bcd`                   |                                                         |
| FXCH                  | (register swap)            | No Fp80 method needed                                   |
| FLD1/FLDZ             | `Fp80::ONE` / `Fp80::ZERO` |                                                         |
| FLDL2T                | `Fp80::LOG2_10_UP/DOWN`    | Select by RC                                            |
| FLDL2E                | `Fp80::LOG2_E_UP/DOWN`     | Select by RC                                            |
| FLDPI                 | `Fp80::PI_UP/DOWN`         | Select by RC                                            |
| FLDLG2                | `Fp80::LOG10_2_UP/DOWN`    | Select by RC                                            |
| FLDLN2                | `Fp80::LN_2_UP/DOWN`       | Select by RC                                            |
| FADD/FADDP/FIADD      | `add`                      | Convert integer operand via `from_i16`/`from_i32` first |
| FSUB/FSUBP/FISUB      | `sub`                      |                                                         |
| FSUBR/FSUBRP/FISUBR   | `sub`                      | Swap operand order                                      |
| FMUL/FMULP/FIMUL      | `mul`                      |                                                         |
| FDIV/FDIVP/FIDIV      | `div`                      |                                                         |
| FDIVR/FDIVRP/FIDIVR   | `div`                      | Swap operand order                                      |
| FSQRT                 | `sqrt`                     |                                                         |
| FABS                  | `abs`                      |                                                         |
| FCHS                  | `negate`                   |                                                         |
| FRNDINT               | `round_to_int`             |                                                         |
| FSCALE                | `scale`                    |                                                         |
| FXTRACT               | `extract`                  |                                                         |
| FPREM                 | `partial_remainder`        |                                                         |
| FPREM1                | `ieee_remainder`           |                                                         |
| F2XM1                 | `f2xm1`                    |                                                         |
| FYL2X                 | `fyl2x`                    |                                                         |
| FYL2XP1               | `fyl2xp1`                  |                                                         |
| FPTAN                 | `fptan`                    | Caller pushes 1.0                                       |
| FPATAN                | `fpatan`                   |                                                         |
| FSIN                  | `fsin`                     |                                                         |
| FCOS                  | `fcos`                     |                                                         |
| FSINCOS               | `fsincos`                  |                                                         |
| FCOM/FCOMP/FCOMPP     | `compare`                  | Convert memory operand first                            |
| FUCOM/FUCOMP/FUCOMPP  | `compare_quiet`            |                                                         |
| FICOM/FICOMP          | `compare`                  | Convert integer operand first                           |
| FTST                  | `compare`                  | Compare against `Fp80::ZERO`                            |
| FXAM                  | `classify` + `sign`        | Map FpClass to C3/C2/C0, sign to C1                     |
| FINIT/FCLEX/FLDCW/etc | (FPU state ops)            | No Fp80 method needed                                   |

---

## 6. FP80 Operations

This section specifies every soft-float operation required by the x87 instruction set.

### 6.1 NaN Propagation Rules

These rules apply uniformly across all arithmetic and comparison operations:

1. **SNaN input (other operand is non-NaN):** Raise IE. Quieten the SNaN by setting bit 62 of the significand. Return the resulting QNaN.
2. **SNaN + QNaN:** Raise IE. Return the QNaN operand (the SNaN payload is discarded).
3. **Two SNaN inputs:** Raise IE. Quieten both, then return the one with the larger significand magnitude (see rule 4).
4. **Two QNaN inputs:** Return the QNaN with the larger significand magnitude. If significands are equal, return the one with the positive sign; if both have the same sign, return `b`.
5. **One QNaN, one non-NaN:** Return the QNaN operand.
6. **Invalid operation** (0/0, ∞−∞, ∞×0, sqrt(negative), etc.): Return the default NaN (indefinite).

### 6.2 Core Arithmetic

#### add(a, b, rc, pc) → Fp80

Computes `a + b`.

Respects precision control. Respects rounding mode.

**Special values:**

| a      | b              | Result                | Exception |
| ------ | -------------- | --------------------- | --------- |
| any    | SNaN           | QNaN(SNaN)            | IE        |
| SNaN   | any            | QNaN(SNaN)            | IE        |
| QNaN   | any            | QNaN(a)               | -         |
| any    | QNaN           | QNaN(b)               | -         |
| +∞     | +∞             | +∞                    | -         |
| −∞     | −∞             | −∞                    | -         |
| +∞     | −∞             | Indefinite            | IE        |
| −∞     | +∞             | Indefinite            | IE        |
| ∞      | finite         | ∞ (same sign)         | -         |
| finite | ∞              | ∞ (same sign)         | -         |
| +0     | −0             | +0 (or −0 if RC=down) | -         |
| −0     | +0             | +0 (or −0 if RC=down) | -         |
| ±0     | ±0 (same sign) | ±0 (same sign)        | -         |
| x      | −x             | +0 (or −0 if RC=down) | -         |

When both operands are zero with opposite signs, the result is +0 except when rounding toward −∞,
in which case the result is −0.

#### sub(a, b, rc, pc) → Fp80

Computes `a − b`. Equivalent to `add(a, negate(b), rc, pc)`.

Same special value rules as `add` with `b`'s sign flipped.

#### mul(a, b, rc, pc) → Fp80

Computes `a × b`.

Respects precision control. Respects rounding mode.

**Special values:**

| a   | b              | Result                  | Exception |
| --- | -------------- | ----------------------- | --------- |
| ∞   | 0              | Indefinite              | IE        |
| 0   | ∞              | Indefinite              | IE        |
| ∞   | ∞              | ∞ (sign = XOR of signs) | -         |
| ∞   | finite nonzero | ∞ (sign = XOR of signs) | -         |
| 0   | 0              | 0 (sign = XOR of signs) | -         |
| 0   | finite nonzero | 0 (sign = XOR of signs) | -         |

The sign of the result is always the XOR of the operand signs, even for zeros.

#### div(a, b, rc, pc) → Fp80

Computes `a / b`.

Respects precision control. Respects rounding mode.

**Special values:**

| a              | b              | Result                  | Exception |
| -------------- | -------------- | ----------------------- | --------- |
| 0              | 0              | Indefinite              | IE        |
| ∞              | ∞              | Indefinite              | IE        |
| finite nonzero | 0              | ∞ (sign = XOR of signs) | ZE        |
| 0              | finite nonzero | 0 (sign = XOR of signs) | -         |
| ∞              | finite         | ∞ (sign = XOR of signs) | -         |
| finite         | ∞              | 0 (sign = XOR of signs) | -         |

#### sqrt(a, rc, pc) → Fp80

Computes `√a`.

Respects precision control. Respects rounding mode.

**Special values:**

| a                | Result     | Exception |
| ---------------- | ---------- | --------- |
| +0               | +0         | -         |
| −0               | −0         | -         |
| +∞               | +∞         | -         |
| −∞               | Indefinite | IE        |
| negative nonzero | Indefinite | IE        |
| SNaN             | QNaN(SNaN) | IE        |
| QNaN             | QNaN(a)    | -         |

#### round_to_int(a, rc) → Fp80

Rounds `a` to an integer value, returned as Fp80.

Does **not** respect precision control - always uses full 64-bit significand.

**Special values:** NaN and infinity are returned unchanged (NaN may raise IE if signaling).

### 6.3 Conversions

#### Integer to Fp80

| Conversion                   | Notes                                                         |
| ---------------------------- | ------------------------------------------------------------- |
| `i16_to_fp80(v: i16) → Fp80` | Exact. No rounding. Zero produces +0.                         |
| `i32_to_fp80(v: i32) → Fp80` | Exact. No rounding.                                           |
| `i64_to_fp80(v: i64) → Fp80` | Exact. No rounding. All i64 values are representable in Fp80. |

#### Fp80 to Integer

| Conversion                 | Overflow/Invalid result           | Notes                |
| -------------------------- | --------------------------------- | -------------------- |
| `fp80_to_i16(v, rc) → i16` | `0x8000` (−32768) + IE            | Range: −32768..32767 |
| `fp80_to_i32(v, rc) → i32` | `0x80000000` (−2147483648) + IE   | Range: −2^31..2^31−1 |
| `fp80_to_i64(v, rc) → i64` | `0x8000000000000000` (−2^63) + IE | Range: −2^63..2^63−1 |

For all Fp80→integer conversions:

- The value is first rounded to an integer using the specified rounding mode.
- If the rounded integer is outside the destination range, or the source is NaN or ∞, the result is
  the "integer indefinite" value (the most negative representable value) and IE is raised.
- ±0 converts to integer 0.

#### Float to Fp80

| Conversion                   | Notes                                                                         |
| ---------------------------- | ----------------------------------------------------------------------------- |
| `f32_to_fp80(v: f32) → Fp80` | Exact widening. No rounding needed. Preserves NaN payload (SNaN → QNaN + IE). |
| `f64_to_fp80(v: f64) → Fp80` | Exact widening. No rounding needed. Preserves NaN payload (SNaN → QNaN + IE). |

#### Fp80 to Float

| Conversion                 | Notes                                                                          |
| -------------------------- | ------------------------------------------------------------------------------ |
| `fp80_to_f32(v, rc) → f32` | May overflow (→ ∞ + OE) or underflow (→ denormal/zero + UE). Rounding applies. |
| `fp80_to_f64(v, rc) → f64` | May overflow (→ ∞ + OE) or underflow (→ denormal/zero + UE). Rounding applies. |

#### BCD Conversions

**BCD format:** 10 bytes (80 bits). 18 packed BCD digits in bytes 0–8 (2 digits per byte, low nibble
first), sign in bit 7 of byte 9. Range: ±999,999,999,999,999,999.

```
Byte 0:  digits 1 (high nibble) and 0 (low nibble)
Byte 1:  digits 3 and 2
...
Byte 8:  digits 17 and 16
Byte 9:  bit 7 = sign (1=negative), bits 6–0 = unused
```

| Conversion                            | Notes                                                                                                                        |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `bcd_to_fp80(bytes: [u8; 10]) → Fp80` | Convert packed BCD to Fp80 via i64 intermediate.                                                                             |
| `fp80_to_bcd(v, rc) → [u8; 10]`       | Convert Fp80 to packed BCD. Rounds to integer first. Out-of-range → IE + indefinite BCD (`0xFF_FF_C0_00_00_00_00_00_00_00`). |

### 6.4 Comparison Operations

#### compare(a, b) → Ordering

**Ordered comparison** (used by FCOM, FCOMP, FCOMPP, FICOM, FICOMP, FTST).

- Returns one of: `LessThan`, `Equal`, `GreaterThan`, `Unordered`.
- If either operand is any NaN (quiet or signaling): result is `Unordered` and IE is raised.
- +0 and −0 compare as `Equal`.

#### compare_unordered(a, b) → Ordering

**Unordered comparison** (used by FUCOM, FUCOMP, FUCOMPP).

- Same as `compare`, but IE is raised **only** for signaling NaN, not for quiet NaN.
- If either operand is a QNaN: result is `Unordered`, no IE.
- If either operand is an SNaN: result is `Unordered` and IE is raised.

#### Condition Code Mapping

| Ordering        | C3  | C2  | C0  |
| --------------- | --- | --- | --- |
| ST(0) > operand | 0   | 0   | 0   |
| ST(0) < operand | 0   | 0   | 1   |
| ST(0) = operand | 1   | 0   | 0   |
| Unordered       | 1   | 1   | 1   |

C1 is cleared to 0 by comparison operations (unless a stack fault occurs, in which case C1 indicates
the direction: 1 = overflow, 0 = underflow).

### 6.5 Transcendental Functions

These operations require more than 80 bits of intermediate precision. The `softfloat` crate uses
double-double (`f64 × 2`) intermediate arithmetic with approximately 106 bits of precision.

Transcendental functions do **not** respect precision control - they always operate at full
extended precision.

#### 6.5.1 f2xm1(x) → Fp80

Computes `2^x − 1`.

**Domain:** −1.0 ≤ x ≤ +1.0. Behavior outside this range is undefined (the 486 manual
specifies this restricted domain).

**Algorithm:** Multiply by ln(2) using extended-precision intermediates, evaluate a polynomial
series for `e^y − 1`, round to Fp80.

**Special values:**

| x    | Result | Exception |
| ---- | ------ | --------- |
| +0   | +0     | -         |
| −0   | −0     | -         |
| −1.0 | −0.5   | PE        |
| +1.0 | +1.0   | PE        |
| SNaN | QNaN   | IE        |
| QNaN | QNaN   | -         |

#### 6.5.2 fyl2x(x, y) → Fp80

Computes `y × log₂(x)`. Operands: x = ST(0), y = ST(1). Result replaces ST(1), ST(0) is popped.

**Domain:** x > 0, y is any real.

**Algorithm:** Argument reduction - normalize x to [√2/2, √2) by extracting the exponent.
Compute log₂(significand) via `(x−1)/(x+1)` substitution and a 9-coefficient odd polynomial.
Multiply by y using extended-precision intermediates.

**Special values:**

| x    | y        | Result           | Exception |
| ---- | -------- | ---------------- | --------- |
| < 0  | any      | Indefinite       | IE        |
| 0    | 0        | Indefinite       | IE        |
| 0    | nonzero  | ±∞ (sign opposite to y) | ZE        |
| +∞   | 0        | Indefinite       | IE        |
| 1    | ∞        | Indefinite       | IE        |
| +∞   | positive | +∞               | -         |
| +∞   | negative | −∞               | -         |
| SNaN | any      | QNaN             | IE        |
| any  | SNaN     | QNaN             | IE        |

#### 6.5.3 fyl2xp1(x, y) → Fp80

Computes `y × log₂(x + 1)`. Operands: x = ST(0), y = ST(1). Result replaces ST(1), ST(0) is popped.

**Domain:** −(1 − √2/2) ≤ x ≤ √2 − 1 (approximately −0.2929 to +0.4142). Provides better precision
than FYL2X for values of x near zero.

**Algorithm:** Uses `x/(x+2)` substitution for the logarithm polynomial, avoiding catastrophic
cancellation in `log₂(1 + x)` when x is small.

**Special values:**

| x    | y    | Result          | Exception |
| ---- | ---- | --------------- | --------- |
| +0   | any  | ±0 (sign = sign(x) XOR sign(y)) | -         |
| −0   | any  | ±0 (sign = sign(x) XOR sign(y)) | -         |
| SNaN | any  | QNaN            | IE        |
| any  | SNaN | QNaN            | IE        |

#### 6.5.4 fsin(x) → Fp80

Computes `sin(x)`.

**Domain:** |x| < 2^63. If |x| ≥ 2^63, C2 is set to 1 and x is returned unchanged (the application
must perform argument reduction).

**Algorithm:** Argument reduction modulo π/2 using a 66-bit π constant matching 486DX hardware. Compute
`n = round(x / (π/2))`, reduce to `|r| ≤ π/4`. The quadrant `n mod 4` determines whether to
evaluate the sine or cosine polynomial and the result sign. The sine polynomial uses 11 coefficients
(odd terms), the cosine polynomial uses 11 coefficients (even terms).

**Special values:**

| x            | Result            | Exception |
| ------------ | ----------------- | --------- |
| +0           | +0                | -         |
| −0           | −0                | -         |
| ±∞           | Indefinite        | IE        |
| SNaN         | QNaN              | IE        |
| QNaN         | QNaN              | -         |
| \|x\| ≥ 2^63 | x unchanged, C2=1 | -         |

#### 6.5.5 fcos(x) → Fp80

Computes `cos(x)`. Same domain and argument reduction as FSIN.

**Special values:**

| x            | Result            | Exception |
| ------------ | ----------------- | --------- |
| ±0           | +1.0              | -         |
| ±∞           | Indefinite        | IE        |
| SNaN         | QNaN              | IE        |
| QNaN         | QNaN              | -         |
| \|x\| ≥ 2^63 | x unchanged, C2=1 | -         |

#### 6.5.6 fsincos(x) → (Fp80, Fp80)

Computes both `sin(x)` and `cos(x)` simultaneously. Same algorithm as FSIN/FCOS with a single
argument reduction pass.

Stack effect: sin(x) replaces ST(0), cos(x) is pushed (becomes new ST(0)). After the instruction,
ST(0) = cos(x), ST(1) = sin(x).

Same special values as FSIN. C2 = 1 if out of range.

#### 6.5.7 fptan(x) → Fp80

Computes `tan(x)` and pushes 1.0.

**Domain:** |x| < 2^63. If out of range, C2 = 1 and x is unchanged (no 1.0 push).

**Algorithm:** Compute sin(x)/cos(x) using the same argument reduction. After computing the tangent,
push the constant 1.0 onto the stack.

Stack effect: ST(0) = tan(x), then push 1.0 so ST(0) = 1.0 and ST(1) = tan(x).

**Special values:**

| x            | ST(1) result | ST(0) pushed       | Exception |
| ------------ | ------------ | ------------------ | --------- |
| ±0           | ±0           | 1.0                | -         |
| ±∞           | Indefinite   | 1.0                | IE        |
| \|x\| ≥ 2^63 | x unchanged  | (not pushed), C2=1 | -         |

#### 6.5.8 fpatan(y, x) → Fp80

Computes `atan2(y, x)` = arctan(ST(1) / ST(0)). Operands: ST(0) = x, ST(1) = y.
Result replaces ST(1), ST(0) is popped.

**Domain:** Unrestricted (handles all quadrants).

**Algorithm:** Compute |y/x| (or |x/y| if |y| > |x|, then adjust). Range reduction using three
zones based on comparison with √3 thresholds. 11-coefficient odd polynomial for the reduced
argument. Add correction terms (π/6, π/4, π/2, π, 3π/4) based on quadrant.

**Special values:**

| y (ST1) | x (ST0) | Result | Notes       |
| ------- | ------- | ------ | ----------- |
| +0      | +x      | +0     |             |
| +0      | −x      | +π     |             |
| −0      | +x      | −0     |             |
| −0      | −x      | −π     |             |
| +y      | +0      | +π/2   |             |
| +y      | −0      | +π/2   |             |
| −y      | +0      | −π/2   |             |
| −y      | −0      | −π/2   |             |
| +∞      | +∞      | +π/4   |             |
| +∞      | −∞      | +3π/4  |             |
| −∞      | +∞      | −π/4   |             |
| −∞      | −∞      | −3π/4  |             |
| ±∞      | finite  | ±π/2   |             |
| finite  | +∞      | ±0     | Sign from y |
| finite  | −∞      | ±π     | Sign from y |
| SNaN    | any     | QNaN   | IE          |
| any     | SNaN    | QNaN   | IE          |
| QNaN    | any     | QNaN   | -           |
| any     | QNaN    | QNaN   | -           |

### 6.6 Other Operations

#### scale(a, b) → Fp80

FSCALE: Computes `a × 2^⌊b⌋`. The exponent of `a` is adjusted by the truncated integer value of `b`.

**Special values:**

| a      | b      | Result           | Exception |
| ------ | ------ | ---------------- | --------- |
| ±0     | −∞     | ±0               | -         |
| ±0     | +∞     | Indefinite       | IE        |
| ±∞     | −∞     | Indefinite       | IE        |
| ±∞     | +∞     | ±∞               | -         |
| finite | 0      | a (unchanged)    | -         |
| 0      | finite | ±0 (sign from a) | -         |
| SNaN   | any    | QNaN             | IE        |
| any    | SNaN   | QNaN             | IE        |
| QNaN   | any    | QNaN             | -         |
| any    | QNaN   | QNaN             | -         |

If the scale factor exceeds the representable exponent range, the result overflows to ±∞ or
underflows to ±0 with the appropriate exceptions.

#### extract(a) → (Fp80, Fp80)

FXTRACT: Separates `a` into its unbiased exponent (as a float) and significand.

After the instruction: ST(0) = significand (with exponent set to 0x3FFF, i.e., the value is in
range [1.0, 2.0)), ST(1) = exponent (as Fp80).

Stack effect: the exponent replaces ST(0), then the significand is pushed (becomes new ST(0)).

**Special values:**

| a   | Significand (ST0) | Exponent (ST1) | Exception    |
| --- | ----------------- | -------------- | ------------ |
| ±0  | ±0                | −∞             | ZE           |
| ±∞  | ±∞                | +∞             | -            |
| NaN | NaN               | NaN            | IE (if SNaN) |

#### partial_remainder(a, b) → (Fp80, u8, bool)

FPREM: Computes the partial remainder using truncation (round-toward-zero for the quotient).

```
q = trunc(a / b)
result = a − q × b
```

Returns: (remainder, quotient_low_3_bits, complete).

- If the exponent difference |exp(a) − exp(b)| < 64, the computation completes in one step:
  `complete = true`, C2 = 0.
- If the exponent difference ≥ 64, the algorithm reduces the exponent by up to 63 bits per iteration:
  `complete = false`, C2 = 1. The application must re-execute FPREM until C2 = 0.

**Quotient bits in condition codes:** C0 = Q2, C3 = Q1, C1 = Q0 (low 3 bits of the quotient).

**Special values:**

| a      | b       | Result        | Exception |
| ------ | ------- | ------------- | --------- |
| ∞      | any     | Indefinite    | IE        |
| any    | 0       | Indefinite    | IE        |
| 0      | nonzero | ±0            | -         |
| finite | ∞       | a (unchanged) | -         |

#### ieee_remainder(a, b) → (Fp80, u8, bool)

FPREM1: Same as FPREM but uses round-to-nearest for the quotient (IEEE 754 remainder).

```
q = round_nearest(a / b)
result = a − q × b
```

Same condition code encoding and iterative behavior as FPREM.

---

## 7. Environment Save/Restore Layouts

### 7.1 FSTENV / FLDENV

The environment consists of CW, SW, TW, instruction pointer, data pointer, and opcode. The layout
depends on the operating mode and operand size.

#### 16-bit Real Mode (14 bytes)

| Offset | Size | Content                          |
| ------ | ---- | -------------------------------- |
| +0     | 16   | Control Word                     |
| +2     | 16   | Status Word                      |
| +4     | 16   | Tag Word                         |
| +6     | 16   | FPU IP offset [15:0]             |
| +8     | 16   | Opcode [10:0] ∣ IP [19:16] << 12 |
| +10    | 16   | FPU data pointer [15:0]          |
| +12    | 16   | FPU data pointer [19:16] << 12   |

#### 16-bit Protected Mode (14 bytes)

| Offset | Size | Content                 |
| ------ | ---- | ----------------------- |
| +0     | 16   | Control Word            |
| +2     | 16   | Status Word             |
| +4     | 16   | Tag Word                |
| +6     | 16   | FPU IP offset [15:0]    |
| +8     | 16   | CS selector             |
| +10    | 16   | FPU data pointer [15:0] |
| +12    | 16   | Data segment selector   |

#### 32-bit Real Mode (28 bytes)

| Offset | Size | Content                           |
| ------ | ---- | --------------------------------- |
| +0     | 32   | `0xFFFF0000 ∣ CW`                 |
| +4     | 32   | `0xFFFF0000 ∣ SW`                 |
| +8     | 32   | `0xFFFF0000 ∣ TW`                 |
| +12    | 32   | `0xFFFF0000 ∣ FPU IP [15:0]`      |
| +16    | 32   | `Opcode [10:0] ∣ IP [31:16] << 12` |
| +20    | 32   | `0xFFFF0000 ∣ FPU DP [15:0]`      |
| +24    | 32   | `DP [31:16] << 12`                |

#### 32-bit Protected Mode (28 bytes)

| Offset | Size | Content                    |
| ------ | ---- | -------------------------- |
| +0     | 32   | `0xFFFF0000 ∣ CW`          |
| +4     | 32   | `0xFFFF0000 ∣ SW`          |
| +8     | 32   | `0xFFFF0000 ∣ TW`          |
| +12    | 32   | FPU IP [31:0]              |
| +16    | 32   | `Opcode [10:0] << 16 ∣ CS` |
| +20    | 32   | FPU DP [31:0]              |
| +24    | 32   | `0xFFFF0000 ∣ DS`          |

### 7.2 FSAVE / FRSTOR

FSAVE stores the environment (14 or 28 bytes) followed by all 8 FPU registers (80 bytes), for a
total of 94 bytes (16-bit mode) or 108 bytes (32-bit mode).

Registers are stored in physical order R0–R7, each as 10 bytes in the Fp80 memory format
(8 bytes significand + 2 bytes sign/exponent, little-endian).

```
Offset env_size + 0*10:  Register R0 (10 bytes)
Offset env_size + 1*10:  Register R1 (10 bytes)
...
Offset env_size + 7*10:  Register R7 (10 bytes)
```

**Important:** FSAVE reinitializes the FPU to its reset state (equivalent to FINIT) after saving.
FRSTOR does not reinitialize - it simply loads the saved state.

After FSTENV, all exception masks in CW are set to 1 (masked). The environment in memory reflects
the state before masking.

---

## 8. x87 Instruction Set Reference

### 8.1 Data Transfer

| Mnemonic       | Encoding  | Description                      | FP80 Operation | Stack | 387 | 486 |
| -------------- | --------- | -------------------------------- | -------------- | ----- | --- | --- |
| `FLD m32real`  | `D9 /0`   | Load single-precision float      | `f32_to_fp80`  | push  | 20  | 3   |
| `FLD m64real`  | `DD /0`   | Load double-precision float      | `f64_to_fp80`  | push  | 25  | 3   |
| `FLD m80real`  | `DB /5`   | Load extended-precision float    | direct load    | push  | 44  | 6   |
| `FLD ST(i)`    | `D9 C0+i` | Duplicate ST(i) to top of stack  | copy           | push  | 14  | 4   |
| `FILD m16int`  | `DF /0`   | Load 16-bit signed integer       | `i16_to_fp80`  | push  | 61  | 13  |
| `FILD m32int`  | `DB /0`   | Load 32-bit signed integer       | `i32_to_fp80`  | push  | 45  | 9   |
| `FILD m64int`  | `DF /5`   | Load 64-bit signed integer       | `i64_to_fp80`  | push  | 56  | 10  |
| `FBLD m80bcd`  | `DF /4`   | Load packed BCD                  | `bcd_to_fp80`  | push  | 266 | 70  |
| `FST m32real`  | `D9 /2`   | Store as single-precision        | `fp80_to_f32`  | -     | 44  | 7   |
| `FST m64real`  | `DD /2`   | Store as double-precision        | `fp80_to_f64`  | -     | 45  | 8   |
| `FST ST(i)`    | `DD D0+i` | Copy ST(0) to ST(i)              | copy           | -     | 11  | 3   |
| `FSTP m32real` | `D9 /3`   | Store and pop single-precision   | `fp80_to_f32`  | pop   | 44  | 7   |
| `FSTP m64real` | `DD /3`   | Store and pop double-precision   | `fp80_to_f64`  | pop   | 45  | 8   |
| `FSTP m80real` | `DB /7`   | Store and pop extended-precision | direct store   | pop   | 53  | 6   |
| `FSTP ST(i)`   | `DD D8+i` | Copy ST(0) to ST(i) and pop      | copy           | pop   | 12  | 3   |
| `FIST m16int`  | `DF /2`   | Store as 16-bit integer          | `fp80_to_i16`  | -     | 82  | 29  |
| `FIST m32int`  | `DB /2`   | Store as 32-bit integer          | `fp80_to_i32`  | -     | 79  | 28  |
| `FISTP m16int` | `DF /3`   | Store and pop 16-bit integer     | `fp80_to_i16`  | pop   | 82  | 29  |
| `FISTP m32int` | `DB /3`   | Store and pop 32-bit integer     | `fp80_to_i32`  | pop   | 79  | 28  |
| `FISTP m64int` | `DF /7`   | Store and pop 64-bit integer     | `fp80_to_i64`  | pop   | 80  | 28  |
| `FBSTP m80bcd` | `DF /6`   | Store and pop packed BCD         | `fp80_to_bcd`  | pop   | 512 | 172 |
| `FXCH`         | `D9 C9`   | Exchange ST(0) and ST(1)         | swap           | -     | 18  | 4   |
| `FXCH ST(i)`   | `D9 C8+i` | Exchange ST(0) and ST(i)         | swap           | -     | 18  | 4   |

### 8.2 Load Constants

| Mnemonic | Encoding | Value    | Stack | 387 | 486 |
| -------- | -------- | -------- | ----- | --- | --- |
| `FLD1`   | `D9 E8`  | +1.0     | push  | 24  | 4   |
| `FLDL2T` | `D9 E9`  | log₂(10) | push  | 40  | 8   |
| `FLDL2E` | `D9 EA`  | log₂(e)  | push  | 40  | 8   |
| `FLDPI`  | `D9 EB`  | π        | push  | 40  | 8   |
| `FLDLG2` | `D9 EC`  | log₁₀(2) | push  | 41  | 8   |
| `FLDLN2` | `D9 ED`  | ln(2)    | push  | 41  | 8   |
| `FLDZ`   | `D9 EE`  | +0.0     | push  | 20  | 4   |

All constants are pushed with the exact bit patterns from section 1.6. The rounding-mode-dependent
constants use the current RC setting to select the appropriate significand variant.

### 8.3 Arithmetic

#### Addition

| Mnemonic             | Encoding  | Operands                   | Description            | 387 | 486 |
| -------------------- | --------- | -------------------------- | ---------------------- | --- | --- |
| `FADD m32real`       | `D8 /0`   | ST(0) ← ST(0) + m32        | Add single from memory | 24  | 8   |
| `FADD m64real`       | `DC /0`   | ST(0) ← ST(0) + m64        | Add double from memory | 29  | 8   |
| `FADD ST(0), ST(i)`  | `D8 C0+i` | ST(0) ← ST(0) + ST(i)      | Add register to ST(0)  | 23  | 8   |
| `FADD ST(i), ST(0)`  | `DC C0+i` | ST(i) ← ST(i) + ST(0)      | Add ST(0) to register  | 23  | 8   |
| `FADDP ST(i), ST(0)` | `DE C0+i` | ST(i) ← ST(i) + ST(0); pop | Add and pop            | 23  | 8   |
| `FIADD m32int`       | `DA /0`   | ST(0) ← ST(0) + m32int     | Add 32-bit integer     | 57  | 19  |
| `FIADD m16int`       | `DE /0`   | ST(0) ← ST(0) + m16int     | Add 16-bit integer     | 71  | 20  |

Precision control and rounding mode apply. FP80 operations: memory loads use `f32_to_fp80` /
`f64_to_fp80` / `i32_to_fp80` / `i16_to_fp80`, then `add(a, b, rc, pc)`.

#### Subtraction

| Mnemonic              | Encoding  | Operands                   | Description                     | 387 | 486 |
| --------------------- | --------- | -------------------------- | ------------------------------- | --- | --- |
| `FSUB m32real`        | `D8 /4`   | ST(0) ← ST(0) − m32        | Subtract single from memory     | 24  | 8   |
| `FSUB m64real`        | `DC /4`   | ST(0) ← ST(0) − m64        | Subtract double from memory     | 28  | 8   |
| `FSUB ST(0), ST(i)`   | `D8 E0+i` | ST(0) ← ST(0) − ST(i)      | Subtract register from ST(0)    | 26  | 8   |
| `FSUB ST(i), ST(0)`   | `DC E8+i` | ST(i) ← ST(i) − ST(0)      | Subtract ST(0) from register    | 26  | 8   |
| `FSUBP ST(i), ST(0)`  | `DE E8+i` | ST(i) ← ST(i) − ST(0); pop | Subtract and pop                | 26  | 8   |
| `FISUB m32int`        | `DA /4`   | ST(0) ← ST(0) − m32int     | Subtract 32-bit integer         | 57  | 19  |
| `FISUB m16int`        | `DE /4`   | ST(0) ← ST(0) − m16int     | Subtract 16-bit integer         | 71  | 20  |
| `FSUBR m32real`       | `D8 /5`   | ST(0) ← m32 − ST(0)        | Reverse subtract single         | 24  | 8   |
| `FSUBR m64real`       | `DC /5`   | ST(0) ← m64 − ST(0)        | Reverse subtract double         | 28  | 8   |
| `FSUBR ST(0), ST(i)`  | `D8 E8+i` | ST(0) ← ST(i) − ST(0)      | Reverse subtract register       | 26  | 8   |
| `FSUBR ST(i), ST(0)`  | `DC E0+i` | ST(i) ← ST(0) − ST(i)      | Reverse subtract ST(0)          | 26  | 8   |
| `FSUBRP ST(i), ST(0)` | `DE E0+i` | ST(i) ← ST(0) − ST(i); pop | Reverse subtract and pop        | 26  | 8   |
| `FISUBR m32int`       | `DA /5`   | ST(0) ← m32int − ST(0)     | Reverse subtract 32-bit integer | 57  | 19  |
| `FISUBR m16int`       | `DE /5`   | ST(0) ← m16int − ST(0)     | Reverse subtract 16-bit integer | 71  | 20  |

#### Multiplication

| Mnemonic             | Encoding  | Operands                   | Description                    | 387 | 486 |
| -------------------- | --------- | -------------------------- | ------------------------------ | --- | --- |
| `FMUL m32real`       | `D8 /1`   | ST(0) ← ST(0) × m32        | Multiply by single from memory | 27  | 11  |
| `FMUL m64real`       | `DC /1`   | ST(0) ← ST(0) × m64        | Multiply by double from memory | 32  | 14  |
| `FMUL ST(0), ST(i)`  | `D8 C8+i` | ST(0) ← ST(0) × ST(i)      | Multiply register              | 46  | 16  |
| `FMUL ST(i), ST(0)`  | `DC C8+i` | ST(i) ← ST(i) × ST(0)      | Multiply by ST(0)              | 46  | 16  |
| `FMULP ST(i), ST(0)` | `DE C8+i` | ST(i) ← ST(i) × ST(0); pop | Multiply and pop               | 29  | 16  |
| `FIMUL m32int`       | `DA /1`   | ST(0) ← ST(0) × m32int     | Multiply by 32-bit integer     | 61  | 22  |
| `FIMUL m16int`       | `DE /1`   | ST(0) ← ST(0) × m16int     | Multiply by 16-bit integer     | 76  | 23  |

#### Division

| Mnemonic              | Encoding  | Operands                   | Description                   | 387 | 486 |
| --------------------- | --------- | -------------------------- | ----------------------------- | --- | --- |
| `FDIV m32real`        | `D8 /6`   | ST(0) ← ST(0) / m32        | Divide by single from memory  | 89  | 73  |
| `FDIV m64real`        | `DC /6`   | ST(0) ← ST(0) / m64        | Divide by double from memory  | 94  | 73  |
| `FDIV ST(0), ST(i)`   | `D8 F0+i` | ST(0) ← ST(0) / ST(i)      | Divide by register            | 88  | 73  |
| `FDIV ST(i), ST(0)`   | `DC F8+i` | ST(i) ← ST(i) / ST(0)      | Divide register by ST(0)      | 88  | 73  |
| `FDIVP ST(i), ST(0)`  | `DE F8+i` | ST(i) ← ST(i) / ST(0); pop | Divide and pop                | 91  | 73  |
| `FIDIV m32int`        | `DA /6`   | ST(0) ← ST(0) / m32int     | Divide by 32-bit integer      | 120 | 84  |
| `FIDIV m16int`        | `DE /6`   | ST(0) ← ST(0) / m16int     | Divide by 16-bit integer      | 136 | 85  |
| `FDIVR m32real`       | `D8 /7`   | ST(0) ← m32 / ST(0)        | Reverse divide single         | 89  | 73  |
| `FDIVR m64real`       | `DC /7`   | ST(0) ← m64 / ST(0)        | Reverse divide double         | 94  | 73  |
| `FDIVR ST(0), ST(i)`  | `D8 F8+i` | ST(0) ← ST(i) / ST(0)      | Reverse divide register       | 88  | 73  |
| `FDIVR ST(i), ST(0)`  | `DC F0+i` | ST(i) ← ST(0) / ST(i)      | Reverse divide ST(0)          | 88  | 73  |
| `FDIVRP ST(i), ST(0)` | `DE F0+i` | ST(i) ← ST(0) / ST(i); pop | Reverse divide and pop        | 91  | 73  |
| `FIDIVR m32int`       | `DA /7`   | ST(0) ← m32int / ST(0)     | Reverse divide 32-bit integer | 121 | 84  |
| `FIDIVR m16int`       | `DE /7`   | ST(0) ← m16int / ST(0)     | Reverse divide 16-bit integer | 135 | 85  |

### 8.4 Unary and Miscellaneous Arithmetic

| Mnemonic  | Encoding | Operation                         | CC affected | Stack | 387 | 486 |
| --------- | -------- | --------------------------------- | ----------- | ----- | --- | --- |
| `FSQRT`   | `D9 FA`  | ST(0) ← √ST(0)                    | C1          | -     | 122 | 83  |
| `FABS`    | `D9 E1`  | ST(0) ← \|ST(0)\|                 | C1 cleared  | -     | 22  | 3   |
| `FCHS`    | `D9 E0`  | ST(0) ← −ST(0)                    | C1 cleared  | -     | 24  | 6   |
| `FRNDINT` | `D9 FC`  | ST(0) ← round(ST(0))              | C1          | -     | 66  | 21  |
| `FSCALE`  | `D9 FD`  | ST(0) ← ST(0) × 2^⌊ST(1)⌋         | C1          | -     | 67  | 30  |
| `FXTRACT` | `D9 F4`  | Separate exponent and significand | C1          | push  | 70  | 16  |
| `FPREM`   | `D9 F8`  | Partial remainder (truncation)    | C0,C1,C2,C3 | -     | 74  | 70  |
| `FPREM1`  | `D9 F5`  | IEEE partial remainder (nearest)  | C0,C1,C2,C3 | -     | 95  | 72  |

**FABS** and **FCHS** are pure bitwise operations (clear/flip the sign bit). No exceptions other
than stack underflow.

**FSQRT** respects precision control.

**FSCALE:** ST(1) is truncated to an integer before use. Does not pop ST(1).

**FXTRACT:** ST(0) is replaced by the exponent (as an Fp80 float), then the significand
(exponent set to 0x3FFF) is pushed onto the stack. After the instruction: ST(0) = significand,
ST(1) = exponent.

**FPREM/FPREM1 condition codes:**

| Code   | Meaning                           |
| ------ | --------------------------------- |
| C2 = 0 | Reduction complete                |
| C2 = 1 | Reduction incomplete (re-execute) |
| C0     | Q2 (bit 2 of quotient)            |
| C3     | Q1 (bit 1 of quotient)            |
| C1     | Q0 (bit 0 of quotient)            |

### 8.5 Transcendentals

| Mnemonic  | Encoding | Operation                                       | CC affected | Stack | 387 | 486 |
| --------- | -------- | ----------------------------------------------- | ----------- | ----- | --- | --- |
| `F2XM1`   | `D9 F0`  | ST(0) ← 2^ST(0) − 1                             | C1          | -     | 211 | 140 |
| `FYL2X`   | `D9 F1`  | ST(1) ← ST(1) × log₂(ST(0)); pop                | C1          | pop   | 120 | 196 |
| `FYL2XP1` | `D9 F9`  | ST(1) ← ST(1) × log₂(ST(0)+1); pop              | C1          | pop   | 257 | 171 |
| `FPTAN`   | `D9 F2`  | ST(0) ← tan(ST(0)); push 1.0                    | C1, C2      | push  | 191 | 200 |
| `FPATAN`  | `D9 F3`  | ST(1) ← atan2(ST(1), ST(0)); pop                | C1          | pop   | 314 | 218 |
| `FSIN`    | `D9 FE`  | ST(0) ← sin(ST(0))                              | C1, C2      | -     | 122 | 257 |
| `FCOS`    | `D9 FF`  | ST(0) ← cos(ST(0))                              | C1, C2      | -     | 123 | 257 |
| `FSINCOS` | `D9 FB`  | temp ← ST(0); ST(0) ← sin(temp); push cos(temp) | C1, C2      | push  | 194 | 292 |

**C2 flag:** Set to 1 by FSIN, FCOS, FSINCOS, and FPTAN when |operand| ≥ 2^63 (out of range).
When C2 = 1, the operand is returned unchanged and the application must perform argument reduction.

### 8.6 Comparison

| Mnemonic        | Encoding  | Description                           | CC set      | Stack | 387 | 486 |
| --------------- | --------- | ------------------------------------- | ----------- | ----- | --- | --- |
| `FCOM m32real`  | `D8 /2`   | Compare ST(0) with m32                | C0,C2,C3    | -     | 26  | 4   |
| `FCOM m64real`  | `DC /2`   | Compare ST(0) with m64                | C0,C2,C3    | -     | 31  | 4   |
| `FCOM ST(i)`    | `D8 D0+i` | Compare ST(0) with ST(i)              | C0,C2,C3    | -     | 24  | 4   |
| `FCOMP m32real` | `D8 /3`   | Compare and pop                       | C0,C2,C3    | pop   | 26  | 4   |
| `FCOMP m64real` | `DC /3`   | Compare and pop                       | C0,C2,C3    | pop   | 26  | 4   |
| `FCOMP ST(i)`   | `D8 D8+i` | Compare and pop                       | C0,C2,C3    | pop   | 26  | 4   |
| `FCOMPP`        | `DE D9`   | Compare ST(0) with ST(1) and pop both | C0,C2,C3    | pop×2 | 26  | 5   |
| `FUCOM ST(i)`   | `DD E0+i` | Unordered compare                     | C0,C2,C3    | -     | 24  | 4   |
| `FUCOMP ST(i)`  | `DD E8+i` | Unordered compare and pop             | C0,C2,C3    | pop   | 26  | 4   |
| `FUCOMPP`       | `DA E9`   | Unordered compare and pop both        | C0,C2,C3    | pop×2 | 26  | 5   |
| `FICOM m32int`  | `DA /2`   | Compare ST(0) with m32int             | C0,C2,C3    | -     | 56  | 15  |
| `FICOM m16int`  | `DE /2`   | Compare ST(0) with m16int             | C0,C2,C3    | -     | 71  | 16  |
| `FICOMP m32int` | `DA /3`   | Compare and pop m32int                | C0,C2,C3    | pop   | 56  | 15  |
| `FICOMP m16int` | `DE /3`   | Compare and pop m16int                | C0,C2,C3    | pop   | 71  | 16  |
| `FTST`          | `D9 E4`   | Compare ST(0) with +0.0               | C0,C2,C3    | -     | 28  | 4   |
| `FXAM`          | `D9 E5`   | Examine ST(0)                         | C0,C1,C2,C3 | -     | 30  | 8   |

**FCOM/FCOMP/FCOMPP:** Ordered comparison. IE raised on any NaN.

**FUCOM/FUCOMP/FUCOMPP:** Unordered comparison. IE raised only on SNaN.

**FTST:** Compares ST(0) against +0.0 using ordered comparison rules.

**FXAM classification:**

| C3  | C2  | C0  | Class       |
| --- | --- | --- | ----------- |
| 0   | 0   | 0   | Unsupported |
| 0   | 0   | 1   | NaN         |
| 0   | 1   | 0   | Normal      |
| 0   | 1   | 1   | Infinity    |
| 1   | 0   | 0   | Zero        |
| 1   | 0   | 1   | Empty       |
| 1   | 1   | 0   | Denormal    |

C1 = sign bit of ST(0) (1 if negative, 0 if positive). C1 is set even for empty registers (based
on the raw sign bit in the physical register).

### 8.7 Control

| Mnemonic             | Encoding  | Description                                              | 387 | 486 |
| -------------------- | --------- | -------------------------------------------------------- | --- | --- |
| `FINIT`              | `DB E3`   | Initialize FPU (preceded by implicit FWAIT)              | 33  | 17  |
| `FNINIT`             | `DB E3`   | Initialize FPU (no FWAIT)                                | 33  | 17  |
| `FCLEX`              | `DB E2`   | Clear exceptions (preceded by implicit FWAIT)            | 11  | 7   |
| `FNCLEX`             | `DB E2`   | Clear exceptions (no FWAIT)                              | 11  | 7   |
| `FLDCW m2byte`       | `D9 /5`   | Load control word from memory                            | 19  | 4   |
| `FSTCW m2byte`       | `D9 /7`   | Store control word (preceded by implicit FWAIT)          | 15  | 3   |
| `FNSTCW m2byte`      | `D9 /7`   | Store control word (no FWAIT)                            | 15  | 3   |
| `FSTSW AX`           | `DF E0`   | Store status word to AX (preceded by implicit FWAIT)     | 13  | 3   |
| `FNSTSW AX`          | `DF E0`   | Store status word to AX (no FWAIT)                       | 13  | 3   |
| `FSTSW m2byte`       | `DD /7`   | Store status word to memory (preceded by implicit FWAIT) | 15  | 3   |
| `FNSTSW m2byte`      | `DD /7`   | Store status word to memory (no FWAIT)                   | 15  | 3   |
| `FLDENV m14/28byte`  | `D9 /4`   | Load FPU environment                                     | 71  | 44  |
| `FSTENV m14/28byte`  | `D9 /6`   | Store FPU environment (preceded by implicit FWAIT)       | 103 | 67  |
| `FNSTENV m14/28byte` | `D9 /6`   | Store FPU environment (no FWAIT)                         | 103 | 67  |
| `FSAVE m94/108byte`  | `DD /6`   | Save FPU state (preceded by implicit FWAIT)              | 375 | 154 |
| `FNSAVE m94/108byte` | `DD /6`   | Save FPU state (no FWAIT)                                | 375 | 154 |
| `FRSTOR m94/108byte` | `DD /4`   | Restore FPU state                                        | 308 | 131 |
| `FINCSTP`            | `D9 F7`   | Increment TOP (no tag change)                            | 21  | 3   |
| `FDECSTP`            | `D9 F6`   | Decrement TOP (no tag change)                            | 22  | 3   |
| `FFREE ST(i)`        | `DD C0+i` | Set tag of ST(i) to Empty                                | 18  | 3   |
| `FNOP`               | `D9 D0`   | No operation                                             | 12  | 3   |
| `FWAIT`              | `9B`      | Wait for FPU (not an ESC opcode)                         | 6   | 1   |

**FINIT/FNINIT:** Resets FPU to state described in section 4.6.

**FCLEX/FNCLEX:** Clears SW bits 0–7 (exception flags, SF, ES) and bit 15 (B). Does not modify
the condition codes or TOP.

**FLDCW:** Updates the control word and immediately applies the new rounding mode and precision
control to subsequent operations.

**FINCSTP/FDECSTP:** Modify TOP without changing any tag word entries or register contents.
FINCSTP sets C1 = 0. FDECSTP sets C1 = 0.

**FWAIT distinction:** Mnemonics starting with `F` (FINIT, FCLEX, FSTCW, FSTSW, FSTENV, FSAVE)
are assembled as FWAIT + the `FN` variant. The `FN` variants (FNINIT, FNCLEX, FNSTCW, FNSTSW,
FNSTENV, FNSAVE) omit the FWAIT prefix. The distinction matters because FWAIT checks for pending
unmasked FPU exceptions before allowing the control instruction to execute.

---

## 9. Escape Opcode Dispatch Tables

x87 instructions are encoded as ESC opcodes (D8–DF) followed by a ModRM byte. Dispatch depends
on whether the ModRM byte indicates a memory operand (< 0xC0) or a register operand (≥ 0xC0).

### D8 - Single-Precision Arithmetic

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand |
| --- | -------- | ------- |
| 0   | FADD     | m32real |
| 1   | FMUL     | m32real |
| 2   | FCOM     | m32real |
| 3   | FCOMP    | m32real |
| 4   | FSUB     | m32real |
| 5   | FSUBR    | m32real |
| 6   | FDIV     | m32real |
| 7   | FDIVR    | m32real |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic | Operands     |
| ----- | -------- | ------------ |
| C0–C7 | FADD     | ST(0), ST(i) |
| C8–CF | FMUL     | ST(0), ST(i) |
| D0–D7 | FCOM     | ST(i)        |
| D8–DF | FCOMP    | ST(i)        |
| E0–E7 | FSUB     | ST(0), ST(i) |
| E8–EF | FSUBR    | ST(0), ST(i) |
| F0–F7 | FDIV     | ST(0), ST(i) |
| F8–FF | FDIVR    | ST(0), ST(i) |

### D9 - Load/Store, Transcendentals, Control

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand    |
| --- | -------- | ---------- |
| 0   | FLD      | m32real    |
| 1   | -        | (reserved) |
| 2   | FST      | m32real    |
| 3   | FSTP     | m32real    |
| 4   | FLDENV   | m14/28byte |
| 5   | FLDCW    | m2byte     |
| 6   | FSTENV   | m14/28byte |
| 7   | FSTCW    | m2byte     |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic   |
| ----- | ---------- |
| C0–C7 | FLD ST(i)  |
| C8–CF | FXCH ST(i) |
| D0    | FNOP       |
| D8–DF | FSTP ST(i) |
| E0    | FCHS       |
| E1    | FABS       |
| E4    | FTST       |
| E5    | FXAM       |
| E8    | FLD1       |
| E9    | FLDL2T     |
| EA    | FLDL2E     |
| EB    | FLDPI      |
| EC    | FLDLG2     |
| ED    | FLDLN2     |
| EE    | FLDZ       |
| F0    | F2XM1      |
| F1    | FYL2X      |
| F2    | FPTAN      |
| F3    | FPATAN     |
| F4    | FXTRACT    |
| F5    | FPREM1     |
| F6    | FDECSTP    |
| F7    | FINCSTP    |
| F8    | FPREM      |
| F9    | FYL2XP1    |
| FA    | FSQRT      |
| FB    | FSINCOS    |
| FC    | FRNDINT    |
| FD    | FSCALE     |
| FE    | FSIN       |
| FF    | FCOS       |

### DA - 32-bit Integer Arithmetic

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand |
| --- | -------- | ------- |
| 0   | FIADD    | m32int  |
| 1   | FIMUL    | m32int  |
| 2   | FICOM    | m32int  |
| 3   | FICOMP   | m32int  |
| 4   | FISUB    | m32int  |
| 5   | FISUBR   | m32int  |
| 6   | FIDIV    | m32int  |
| 7   | FIDIVR   | m32int  |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic | Notes                                                                                   |
| ----- | -------- | --------------------------------------------------------------------------------------- |
| E9    | FUCOMPP  | Only this one entry; all other register-form ModRM values are reserved/invalid on the FPU |

### DB - 32-bit Integer Load/Store, Extended Load/Store

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand    |
| --- | -------- | ---------- |
| 0   | FILD     | m32int     |
| 1   | -        | (reserved) |
| 2   | FIST     | m32int     |
| 3   | FISTP    | m32int     |
| 4   | -        | (reserved) |
| 5   | FLD      | m80real    |
| 6   | -        | (reserved) |
| 7   | FSTP     | m80real    |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic | Notes                        |
| ----- | -------- | ---------------------------- |
| E0    | FNOP     | Legacy FENI (8087 compat)    |
| E1    | FNOP     | Legacy FDISI (8087 compat)   |
| E2    | FCLEX    | Clear exceptions             |
| E3    | FINIT    | Initialize FPU               |
| E4    | FNOP     | Legacy FSETPM (80287 compat) |

All other register-form ModRM values in DB are reserved on the FPU.

### DC - Double-Precision Arithmetic

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand |
| --- | -------- | ------- |
| 0   | FADD     | m64real |
| 1   | FMUL     | m64real |
| 2   | FCOM     | m64real |
| 3   | FCOMP    | m64real |
| 4   | FSUB     | m64real |
| 5   | FSUBR    | m64real |
| 6   | FDIV     | m64real |
| 7   | FDIVR    | m64real |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic | Operands     |
| ----- | -------- | ------------ |
| C0–C7 | FADD     | ST(i), ST(0) |
| C8–CF | FMUL     | ST(i), ST(0) |
| E0–E7 | FSUBR    | ST(i), ST(0) |
| E8–EF | FSUB     | ST(i), ST(0) |
| F0–F7 | FDIVR    | ST(i), ST(0) |
| F8–FF | FDIV     | ST(i), ST(0) |

Note: In the register form of DC, the FSUB/FSUBR and FDIV/FDIVR mappings are **swapped** compared
to D8. This is the well-known x87 encoding quirk. The assembler handles this - `FSUB ST(i), ST(0)`
encodes as `DC E8+i` (which is the FSUBR bit pattern), but the operation performed is subtraction
in the expected order.

### DD - Double-Precision Memory, Stack Management

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand     |
| --- | -------- | ----------- |
| 0   | FLD      | m64real     |
| 1   | -        | (reserved)  |
| 2   | FST      | m64real     |
| 3   | FSTP     | m64real     |
| 4   | FRSTOR   | m94/108byte |
| 5   | -        | (reserved)  |
| 6   | FSAVE    | m94/108byte |
| 7   | FSTSW    | m2byte      |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic     |
| ----- | ------------ |
| C0–C7 | FFREE ST(i)  |
| C8–CF | FXCH ST(i)   |
| D0–D7 | FST ST(i)    |
| D8–DF | FSTP ST(i)   |
| E0–E7 | FUCOM ST(i)  |
| E8–EF | FUCOMP ST(i) |

### DE - 16-bit Integer Arithmetic, Pop Variants

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand |
| --- | -------- | ------- |
| 0   | FIADD    | m16int  |
| 1   | FIMUL    | m16int  |
| 2   | FICOM    | m16int  |
| 3   | FICOMP   | m16int  |
| 4   | FISUB    | m16int  |
| 5   | FISUBR   | m16int  |
| 6   | FIDIV    | m16int  |
| 7   | FIDIVR   | m16int  |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic | Operands                               |
| ----- | -------- | -------------------------------------- |
| C0–C7 | FADDP    | ST(i), ST(0)                           |
| C8–CF | FMULP    | ST(i), ST(0)                           |
| D9    | FCOMPP   | (compares ST(0) with ST(1), pops both) |
| E0–E7 | FSUBRP   | ST(i), ST(0)                           |
| E8–EF | FSUBP    | ST(i), ST(0)                           |
| F0–F7 | FDIVRP   | ST(i), ST(0)                           |
| F8–FF | FDIVP    | ST(i), ST(0)                           |

### DF - 16/64-bit Integer, BCD, Status Word

**Memory (ModRM < C0):**

| /r  | Mnemonic | Operand    |
| --- | -------- | ---------- |
| 0   | FILD     | m16int     |
| 1   | -        | (reserved) |
| 2   | FIST     | m16int     |
| 3   | FISTP    | m16int     |
| 4   | FBLD     | m80bcd     |
| 5   | FILD     | m64int     |
| 6   | FBSTP    | m80bcd     |
| 7   | FISTP    | m64int     |

**Register (ModRM ≥ C0):**

| ModRM | Mnemonic | Notes                            |
| ----- | -------- | -------------------------------- |
| E0    | FSTSW AX | Store status word to AX register |

All other register-form ModRM values in DF are reserved on the FPU.

---

## 10. FPU Cycle Timing Table

All cycle counts assume a cache hit and aligned data. The 387 column gives the 80387 coprocessor
timings (used with the 386DX), the 486 column gives the on-chip FPU timings. Values are from
https://www2.math.uni-wuppertal.de/~fpf/Uebungen/GdR-SS02/opcode_f.html (low end of range used
where applicable).

| Mnemonic            | Form    | 387 | 486 |
|---------------------|---------|-----|-----|
| **Data Transfer**   |         |     |     |
| FLD                 | m32real | 20  | 3   |
| FLD                 | m64real | 25  | 3   |
| FLD                 | m80real | 44  | 6   |
| FLD                 | ST(i)   | 14  | 4   |
| FST                 | m32real | 44  | 7   |
| FST                 | m64real | 45  | 8   |
| FST                 | ST(i)   | 11  | 3   |
| FSTP                | m32real | 44  | 7   |
| FSTP                | m64real | 45  | 8   |
| FSTP                | m80real | 53  | 6   |
| FSTP                | ST(i)   | 12  | 3   |
| FILD                | m16int  | 61  | 13  |
| FILD                | m32int  | 45  | 9   |
| FILD                | m64int  | 56  | 10  |
| FIST                | m16int  | 82  | 29  |
| FIST                | m32int  | 79  | 28  |
| FISTP               | m16int  | 82  | 29  |
| FISTP               | m32int  | 79  | 28  |
| FISTP               | m64int  | 80  | 28  |
| FBLD                | m80bcd  | 266 | 70  |
| FBSTP               | m80bcd  | 512 | 172 |
| FXCH                | ST(i)   | 18  | 4   |
| **Constants**       |         |     |     |
| FLD1                |         | 24  | 4   |
| FLDZ                |         | 20  | 4   |
| FLDL2T              |         | 40  | 8   |
| FLDL2E              |         | 40  | 8   |
| FLDPI               |         | 40  | 8   |
| FLDLG2              |         | 41  | 8   |
| FLDLN2              |         | 41  | 8   |
| **Arithmetic**      |         |     |     |
| FADD                | reg     | 23  | 8   |
| FADD                | mem32   | 24  | 8   |
| FADD                | mem64   | 29  | 8   |
| FADDP               |         | 23  | 8   |
| FSUB                | reg     | 26  | 8   |
| FSUB                | mem32   | 24  | 8   |
| FSUB                | mem64   | 28  | 8   |
| FSUBP               |         | 26  | 8   |
| FSUBR               | reg     | 26  | 8   |
| FSUBR               | mem32   | 24  | 8   |
| FSUBR               | mem64   | 28  | 8   |
| FSUBRP              |         | 26  | 8   |
| FMUL                | reg     | 46  | 16  |
| FMUL                | mem32   | 27  | 11  |
| FMUL                | mem64   | 32  | 14  |
| FMULP               |         | 29  | 16  |
| FDIV                | reg     | 88  | 73  |
| FDIV                | mem32   | 89  | 73  |
| FDIV                | mem64   | 94  | 73  |
| FDIVP               |         | 91  | 73  |
| FDIVR               | reg     | 88  | 73  |
| FDIVR               | mem32   | 89  | 73  |
| FDIVR               | mem64   | 94  | 73  |
| FDIVRP              |         | 91  | 73  |
| FIADD               | m16int  | 71  | 20  |
| FIADD               | m32int  | 57  | 19  |
| FISUB               | m16int  | 71  | 20  |
| FISUB               | m32int  | 57  | 19  |
| FISUBR              | m16int  | 71  | 20  |
| FISUBR              | m32int  | 57  | 19  |
| FIMUL               | m16int  | 76  | 23  |
| FIMUL               | m32int  | 61  | 22  |
| FIDIV               | m16int  | 136 | 85  |
| FIDIV               | m32int  | 120 | 84  |
| FIDIVR              | m16int  | 135 | 85  |
| FIDIVR              | m32int  | 121 | 84  |
| FSQRT               |         | 122 | 83  |
| FABS                |         | 22  | 3   |
| FCHS                |         | 24  | 6   |
| FRNDINT             |         | 66  | 21  |
| FSCALE              |         | 67  | 30  |
| FXTRACT             |         | 70  | 16  |
| FPREM               |         | 74  | 70  |
| FPREM1              |         | 95  | 72  |
| **Transcendentals** |         |     |     |
| F2XM1               |         | 211 | 140 |
| FYL2X               |         | 120 | 196 |
| FYL2XP1             |         | 257 | 171 |
| FPTAN               |         | 191 | 200 |
| FPATAN              |         | 314 | 218 |
| FSIN                |         | 122 | 257 |
| FCOS                |         | 123 | 257 |
| FSINCOS             |         | 194 | 292 |
| **Comparison**      |         |     |     |
| FCOM                | reg     | 24  | 4   |
| FCOM                | mem32   | 26  | 4   |
| FCOM                | mem64   | 31  | 4   |
| FCOMP               |         | 26  | 4   |
| FCOMPP              |         | 26  | 5   |
| FUCOM               |         | 24  | 4   |
| FUCOMP              |         | 26  | 4   |
| FUCOMPP             |         | 26  | 5   |
| FICOM               | m16int  | 71  | 16  |
| FICOM               | m32int  | 56  | 15  |
| FICOMP              | m16int  | 71  | 16  |
| FICOMP              | m32int  | 56  | 15  |
| FTST                |         | 28  | 4   |
| FXAM                |         | 30  | 8   |
| **Control**         |         |     |     |
| FINIT               |         | 33  | 17  |
| FCLEX               |         | 11  | 7   |
| FLDCW               |         | 19  | 4   |
| FSTCW               |         | 15  | 3   |
| FSTSW               | mem     | 15  | 3   |
| FSTSW               | AX      | 13  | 3   |
| FLDENV              |         | 71  | 44  |
| FSTENV              |         | 103 | 67  |
| FSAVE               |         | 375 | 154 |
| FRSTOR              |         | 308 | 131 |
| FINCSTP             |         | 21  | 3   |
| FDECSTP             |         | 22  | 3   |
| FFREE               |         | 18  | 3   |
| FNOP                |         | 12  | 3   |
| FWAIT               |         | 6   | 1   |

---

## 11. Implementation Reference

The soft-float operations specified in section 6 are implemented in the `softfloat` crate
(`crates/softfloat/`). Key source files:

| File                                     | Content                                                                 |
|------------------------------------------|-------------------------------------------------------------------------|
| `crates/softfloat/src/lib.rs`            | `Fp80` type, supporting types, constants, classification predicates     |
| `crates/softfloat/src/arithmetic.rs`     | Core arithmetic (add, sub, mul, div, sqrt, round_to_int)                |
| `crates/softfloat/src/compare.rs`        | Ordered and unordered comparisons                                       |
| `crates/softfloat/src/convert.rs`        | Integer, float, and BCD conversions                                     |
| `crates/softfloat/src/transcendental.rs` | Transcendental functions, polynomial coefficients, argument reduction   |
| `crates/softfloat/src/double_f64.rs`     | Double-double (`f64 × 2`) intermediate arithmetic (~106 bits precision) |
| `crates/softfloat/src/other.rs`          | FSCALE, FXTRACT, FPREM/FPREM1 iterative algorithms                      |
