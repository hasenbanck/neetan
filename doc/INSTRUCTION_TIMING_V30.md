# NEC V20/V30 Instruction Execution Clock Cycles

**Source:** NEC "16-bit V Series User's Manual — Instruction Edition" (U11301EJ5V0UMJ1), Table 2-8 "Number of Instruction Execution Clocks".

## Conventions

- **Mnemonic column** uses NEC native mnemonics. Intel 8086 equivalents are shown in parentheses where they differ.
- **Byte / Reg** column: clock count for byte-sized operations, or register-only operations where word alignment is irrelevant.
- **Word Even** column: clock count for word-sized operations when the operand is at an even (aligned) address on the V30 (16-bit bus).
- **Word Odd** column: clock count for word-sized operations when the operand is at an odd (misaligned) address on the V30.
- V20 and V30 values are identical for all instructions listed here.
- A dash (`—`) means the column is not applicable for that operand combination.
- `n` = shift/rotate count. `m` = number of BCD digit pairs. `rep` = repeat count (value in CW register).
- `acc` = AL (byte) or AW (word). `dmem` = direct memory address. `sreg` = segment register.
- `imm` = immediate value. `reg` / `reg'` = general-purpose register. `mem` = memory operand.

---

## Data Transfer

| Mnemonic       | Operand           | Byte / Reg | Word Even | Word Odd | Notes                        |
|----------------|-------------------|------------|-----------|----------|------------------------------|
| MOV            | reg, reg'         | 2          | —         | —        |                              |
| MOV            | mem, reg          | 9          | 9         | 13       |                              |
| MOV            | reg, mem          | 11         | 11        | 15       |                              |
| MOV            | mem, imm          | 11         | 11        | 15       |                              |
| MOV            | reg, imm          | 4          | —         | —        |                              |
| MOV            | acc, dmem         | 10         | 10        | 14       | Direct memory to accumulator |
| MOV            | dmem, acc         | 9          | 9         | 13       | Accumulator to direct memory |
| MOV            | sreg, reg16       | 2          | —         | —        |                              |
| MOV            | sreg, mem16       | —          | 11        | 15       |                              |
| MOV            | reg16, sreg       | 2          | —         | —        |                              |
| MOV            | mem16, sreg       | —          | 10        | 14       |                              |
| MOV (LDS)      | DS0, reg16, mem32 | —          | 18        | 26       | Load pointer into DS0        |
| MOV (LES)      | DS1, reg16, mem32 | —          | 18        | 26       | Load pointer into DS1        |
| MOV (LAHF)     | AH, PSW           | 2          | —         | —        | Load flags into AH           |
| MOV (SAHF)     | PSW, AH           | 3          | —         | —        | Store AH into flags          |
| XCH (XCHG)     | reg, reg'         | 3          | —         | —        |                              |
| XCH (XCHG)     | mem, reg          | 16         | 16        | 24       |                              |
| XCH (XCHG)     | reg, mem          | 16         | 16        | 24       |                              |
| XCH (XCHG)     | AW, reg16         | 3          | —         | —        |                              |
| PUSH           | mem16             | —          | 18        | 26       |                              |
| PUSH           | reg16             | —          | 8         | 12       |                              |
| PUSH           | sreg              | —          | 8         | 12       |                              |
| PUSH (PUSHF)   | PSW               | —          | 8         | 12       |                              |
| PUSH (PUSHA)   | R                 | —          | 35        | 67       | Push all registers           |
| PUSH           | imm8              | —          | 7         | 11       | Sign-extended to 16 bits     |
| PUSH           | imm16             | —          | 8         | 12       |                              |
| POP            | mem16             | —          | 17        | 25       |                              |
| POP            | reg16             | —          | 8         | 12       |                              |
| POP            | sreg              | —          | 8         | 12       |                              |
| POP (POPF)     | PSW               | —          | 8         | 12       |                              |
| POP (POPA)     | R                 | —          | 43        | 75       | Pop all registers            |
| LDEA (LEA)     | reg16, mem16      | 4          | —         | —        | Load effective address       |
| TRANS (XLAT)   | —                 | 9          | —         | —        | Table lookup translation     |
| TRANSB (XLATB) | —                 | 9          | —         | —        | Table lookup translation     |

---

## Arithmetic — ADD / ADDC / SUB / SUBC

ADD, ADDC (ADC), SUB, and SUBC (SBB) all share identical timings.

| Mnemonic                | Operand   | Byte / Reg | Word Even | Word Odd | Notes |
|-------------------------|-----------|------------|-----------|----------|-------|
| ADD / ADDC / SUB / SUBC | reg, reg' | 2          | —         | —        |       |
| ADD / ADDC / SUB / SUBC | mem, reg  | 16         | 16        | 24       |       |
| ADD / ADDC / SUB / SUBC | reg, mem  | 11         | 11        | 15       |       |
| ADD / ADDC / SUB / SUBC | reg, imm  | 4          | —         | —        |       |
| ADD / ADDC / SUB / SUBC | mem, imm  | 18         | 18        | 26       |       |
| ADD / ADDC / SUB / SUBC | acc, imm  | 4          | —         | —        |       |

## Arithmetic — CMP

| Mnemonic | Operand   | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-----------|------------|-----------|----------|-------|
| CMP      | reg, reg' | 2          | —         | —        |       |
| CMP      | mem, reg  | 11         | 11        | 15       |       |
| CMP      | reg, mem  | 11         | 11        | 15       |       |
| CMP      | reg, imm  | 4          | —         | —        |       |
| CMP      | mem, imm  | 13         | 13        | 17       |       |
| CMP      | acc, imm  | 4          | —         | —        |       |

## Arithmetic — INC / DEC / NEG

| Mnemonic | Operand | Byte / Reg | Word Even | Word Odd | Notes |
|----------|---------|------------|-----------|----------|-------|
| INC      | reg8    | 2          | —         | —        |       |
| INC      | reg16   | 2          | —         | —        |       |
| INC      | mem     | 16         | 16        | 24       |       |
| DEC      | reg8    | 2          | —         | —        |       |
| DEC      | reg16   | 2          | —         | —        |       |
| DEC      | mem     | 16         | 16        | 24       |       |
| NEG      | reg     | 2          | —         | —        |       |
| NEG      | mem     | 16         | 16        | 24       |       |

## Arithmetic — Signed Multiply (MUL / IMUL)

NEC MUL is signed multiply (Intel IMUL).

| Mnemonic   | Operand              | Byte / Reg | Word Even | Word Odd | Notes          |
|------------|----------------------|------------|-----------|----------|----------------|
| MUL (IMUL) | reg8                 | 33–39      | —         | —        |                |
| MUL (IMUL) | mem8                 | 39–45      | —         | —        |                |
| MUL (IMUL) | reg16                | 41–47      | —         | —        |                |
| MUL (IMUL) | mem16                | —          | 47–53     | 51–57    |                |
| MUL (IMUL) | reg16, imm8          | 28–34      | —         | —        | 3-operand form |
| MUL (IMUL) | reg16, imm16         | 36–42      | —         | —        | 3-operand form |
| MUL (IMUL) | reg16, reg16', imm8  | 28–34      | —         | —        | 3-operand form |
| MUL (IMUL) | reg16, mem16, imm8   | —          | 34–40     | 38–44    | 3-operand form |
| MUL (IMUL) | reg16, reg16', imm16 | 36–42      | —         | —        | 3-operand form |
| MUL (IMUL) | reg16, mem16, imm16  | —          | 42–48     | 46–52    | 3-operand form |

## Arithmetic — Unsigned Multiply (MULU / MUL)

NEC MULU is unsigned multiply (Intel MUL).

| Mnemonic   | Operand | Byte / Reg | Word Even | Word Odd | Notes |
|------------|---------|------------|-----------|----------|-------|
| MULU (MUL) | reg8    | 21–22      | —         | —        |       |
| MULU (MUL) | mem8    | 27–28      | —         | —        |       |
| MULU (MUL) | reg16   | 29–30      | —         | —        |       |
| MULU (MUL) | mem16   | —          | 35–36     | 39–40    |       |

## Arithmetic — Signed Divide (DIV / IDIV)

NEC DIV is signed divide (Intel IDIV).

| Mnemonic   | Operand | Byte / Reg | Word Even | Word Odd | Notes |
|------------|---------|------------|-----------|----------|-------|
| DIV (IDIV) | reg8    | 29–34      | —         | —        |       |
| DIV (IDIV) | mem8    | 35–40      | —         | —        |       |
| DIV (IDIV) | reg16   | 38–43      | —         | —        |       |
| DIV (IDIV) | mem16   | —          | 44–49     | 48–53    |       |

## Arithmetic — Unsigned Divide (DIVU / DIV)

NEC DIVU is unsigned divide (Intel DIV).

| Mnemonic   | Operand | Byte / Reg | Word Even | Word Odd | Notes |
|------------|---------|------------|-----------|----------|-------|
| DIVU (DIV) | reg8    | 19         | —         | —        |       |
| DIVU (DIV) | mem8    | 25         | —         | —        |       |
| DIVU (DIV) | reg16   | 25         | —         | —        |       |
| DIVU (DIV) | mem16   | —          | 31        | 35       |       |

---

## BCD / Conversion

| Mnemonic    | Operand | Byte / Reg | Word Even | Word Odd | Notes                            |
|-------------|---------|------------|-----------|----------|----------------------------------|
| ADJ4A (DAA) | —       | 3          | —         | —        | Decimal adjust after addition    |
| ADJ4S (DAS) | —       | 7          | —         | —        | Decimal adjust after subtraction |
| ADJBA (AAA) | —       | 3          | —         | —        | ASCII adjust after addition      |
| ADJBS (AAS) | —       | 7          | —         | —        | ASCII adjust after subtraction   |
| CVTBD (AAM) | —       | 15         | —         | —        | ASCII adjust for multiply        |
| CVTDB (AAD) | —       | 7          | —         | —        | ASCII adjust for division        |
| CVTBW (CBW) | —       | 2          | —         | —        | Byte to word sign-extend         |
| CVTWL (CWD) | —       | 4–5        | —         | —        | Word to doubleword sign-extend   |

## BCD String Operations

| Mnemonic | Operand | Clocks    | Notes                                  |
|----------|---------|-----------|----------------------------------------|
| ADD4S    | —       | 19\*m + 7 | m = CL / 2 (number of BCD digit pairs) |
| SUB4S    | —       | 19\*m + 7 | m = CL / 2                             |
| CMP4S    | —       | 19\*m + 7 | m = CL / 2                             |

---

## Logic — AND / OR / XOR

AND, OR, and XOR all share identical timings.

| Mnemonic       | Operand   | Byte / Reg | Word Even | Word Odd | Notes |
|----------------|-----------|------------|-----------|----------|-------|
| AND / OR / XOR | reg, reg' | 2          | —         | —        |       |
| AND / OR / XOR | mem, reg  | 16         | 16        | 24       |       |
| AND / OR / XOR | reg, mem  | 11         | 11        | 15       |       |
| AND / OR / XOR | reg, imm  | 4          | —         | —        |       |
| AND / OR / XOR | mem, imm  | 18         | 18        | 26       |       |
| AND / OR / XOR | acc, imm  | 4          | —         | —        |       |

## Logic — NOT / TEST

| Mnemonic | Operand   | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-----------|------------|-----------|----------|-------|
| NOT      | reg       | 2          | —         | —        |       |
| NOT      | mem       | 16         | 16        | 24       |       |
| TEST     | reg, reg' | 2          | —         | —        |       |
| TEST     | mem, reg  | 10         | 10        | 14       |       |
| TEST     | reg, mem  | 10         | 10        | 14       |       |
| TEST     | reg, imm  | 4          | —         | —        |       |
| TEST     | mem, imm  | 11         | 11        | 15       |       |
| TEST     | acc, imm  | 4          | —         | —        |       |

---

## Shift and Rotate

All shift/rotate instructions (ROL, ROR, ROLC, RORC, SHL, SHR, SHRA) share identical timings. `n` is the shift count.

| Mnemonic                                                     | Operand   | Byte / Reg | Word Even | Word Odd | Notes          |
|--------------------------------------------------------------|-----------|------------|-----------|----------|----------------|
| ROL / ROR / ROLC (RCL) / RORC (RCR) / SHL / SHR / SHRA (SAR) | reg, 1    | 2          | —         | —        |                |
| ROL / ROR / ROLC / RORC / SHL / SHR / SHRA                   | mem, 1    | 16         | 16        | 24       |                |
| ROL / ROR / ROLC / RORC / SHL / SHR / SHRA                   | reg, CL   | 7 + n      | —         | —        | n = CL value   |
| ROL / ROR / ROLC / RORC / SHL / SHR / SHRA                   | mem, CL   | 19 + n     | 19 + n    | 27 + n   | n = CL value   |
| ROL / ROR / ROLC / RORC / SHL / SHR / SHRA                   | reg, imm8 | 7 + n      | —         | —        | n = imm8 value |
| ROL / ROR / ROLC / RORC / SHL / SHR / SHRA                   | mem, imm8 | 19 + n     | 19 + n    | 27 + n   | n = imm8 value |

## Nibble Rotate (V20/V30 specific)

| Mnemonic | Operand | Byte / Reg | Word Even | Word Odd | Notes               |
|----------|---------|------------|-----------|----------|---------------------|
| ROL4     | reg8    | 25         | —         | —        | Rotate nibble left  |
| ROL4     | mem8    | 28         | —         | —        |                     |
| ROR4     | reg8    | 29         | —         | —        | Rotate nibble right |
| ROR4     | mem8    | 33         | —         | —        |                     |

---

## Bit Manipulation (V20/V30 specific)

### CLR1 — Clear Bit

| Mnemonic | Operand     | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-------------|------------|-----------|----------|-------|
| CLR1     | reg8, CL    | 5          | —         | —        |       |
| CLR1     | mem8, CL    | 14         | —         | —        |       |
| CLR1     | reg16, CL   | 5          | —         | —        |       |
| CLR1     | mem16, CL   | —          | 14        | 22       |       |
| CLR1     | reg8, imm3  | 6          | —         | —        |       |
| CLR1     | mem8, imm3  | 15         | —         | —        |       |
| CLR1     | reg16, imm4 | 6          | —         | —        |       |
| CLR1     | mem16, imm4 | —          | 15        | 23       |       |

### SET1 — Set Bit

| Mnemonic | Operand     | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-------------|------------|-----------|----------|-------|
| SET1     | reg8, CL    | 4          | —         | —        |       |
| SET1     | mem8, CL    | 13         | —         | —        |       |
| SET1     | reg16, CL   | 4          | —         | —        |       |
| SET1     | mem16, CL   | —          | 13        | 21       |       |
| SET1     | reg8, imm3  | 5          | —         | —        |       |
| SET1     | mem8, imm3  | 14         | —         | —        |       |
| SET1     | reg16, imm4 | 5          | —         | —        |       |
| SET1     | mem16, imm4 | —          | 14        | 22       |       |

### NOT1 — Complement Bit

| Mnemonic | Operand     | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-------------|------------|-----------|----------|-------|
| NOT1     | reg8, CL    | 4          | —         | —        |       |
| NOT1     | mem8, CL    | 13         | —         | —        |       |
| NOT1     | reg16, CL   | 4          | —         | —        |       |
| NOT1     | mem16, CL   | —          | 13        | 21       |       |
| NOT1     | reg8, imm3  | 5          | —         | —        |       |
| NOT1     | mem8, imm3  | 14         | —         | —        |       |
| NOT1     | reg16, imm4 | 5          | —         | —        |       |
| NOT1     | mem16, imm4 | —          | 14        | 22       |       |

### TEST1 — Test Bit

| Mnemonic | Operand     | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-------------|------------|-----------|----------|-------|
| TEST1    | reg8, CL    | 3          | —         | —        |       |
| TEST1    | mem8, CL    | 8          | —         | —        |       |
| TEST1    | reg16, CL   | 3          | —         | —        |       |
| TEST1    | mem16, CL   | —          | 8         | 12       |       |
| TEST1    | reg8, imm3  | 4          | —         | —        |       |
| TEST1    | mem8, imm3  | 9          | —         | —        |       |
| TEST1    | reg16, imm4 | 4          | —         | —        |       |
| TEST1    | mem16, imm4 | —          | 9         | 13       |       |

---

## String / Block Operations

For repeated string instructions, `rep` is the value in the CW register.

| Mnemonic          | Operand  | Byte / Reg  | Word Even   | Word Odd     | Notes                                       |
|-------------------|----------|-------------|-------------|--------------|---------------------------------------------|
| MOVBK (MOVSB)     | single   | 11          | —           | —            |                                             |
| MOVBK (MOVSW)     | single   | —           | 11          | 19           | Odd = both src and dst odd                  |
| MOVBK (REP MOVSB) | with REP | 11 + 8\*rep | —           | —            |                                             |
| MOVBK (REP MOVSW) | with REP | —           | 11 + 8\*rep | 11 + 16\*rep | Odd,odd worst case; odd,even = 11 + 12\*rep |
| CMPBK (REP CMPSB) | with REP | 7 + 14\*rep | —           | —            |                                             |
| CMPBK (REP CMPSW) | with REP | —           | 7 + 14\*rep | 7 + 22\*rep  | Odd,odd worst case                          |
| CMPM (REP SCASB)  | with REP | 7 + 10\*rep | —           | —            |                                             |
| CMPM (REP SCASW)  | with REP | —           | 7 + 10\*rep | 7 + 14\*rep  |                                             |
| STM (REP STOSB)   | with REP | 7 + 4\*rep  | —           | —            |                                             |
| STM (REP STOSW)   | with REP | —           | 7 + 4\*rep  | 7 + 8\*rep   |                                             |
| LDM (REP LODSB)   | with REP | 7 + 9\*rep  | —           | —            |                                             |
| LDM (REP LODSW)   | with REP | —           | 7 + 9\*rep  | 7 + 13\*rep  |                                             |

---

## I/O

| Mnemonic         | Operand   | Byte / Reg | Word Even  | Word Odd    | Notes              |
|------------------|-----------|------------|------------|-------------|--------------------|
| IN               | acc, imm8 | 9          | 9          | 13          | Fixed port         |
| IN               | acc, DW   | 8          | 8          | 12          | Variable port (DX) |
| OUT              | imm8, acc | 8          | 8          | 12          | Fixed port         |
| OUT              | DW, acc   | 8          | 8          | 12          | Variable port (DX) |
| INM (REP INSB)   | with REP  | 9 + 8\*rep | —          | —           |                    |
| INM (REP INSW)   | with REP  | —          | 9 + 8\*rep | 9 + 16\*rep | Odd,odd worst case |
| OUTM (REP OUTSB) | with REP  | 9 + 8\*rep | —          | —           |                    |
| OUTM (REP OUTSW) | with REP  | —          | 9 + 8\*rep | 9 + 16\*rep | Odd,odd worst case |

---

## Unconditional Branches

| Mnemonic | Operand     | Byte / Reg | Word Even | Word Odd | Notes |
|----------|-------------|------------|-----------|----------|-------|
| BR (JMP) | near-label  | 12         | —         | —        |       |
| BR (JMP) | short-label | 12         | —         | —        |       |
| BR (JMP) | regptr16    | 11         | —         | —        |       |
| BR (JMP) | memptr16    | —          | 20        | 24       |       |
| BR (JMP) | far-label   | 15         | —         | —        |       |
| BR (JMP) | memptr32    | —          | 27        | 35       |       |

## CALL / RET

| Mnemonic    | Operand         | Clocks Even | Clocks Odd | Notes                      |
|-------------|-----------------|-------------|------------|----------------------------|
| CALL        | near-proc       | 16          | 20         |                            |
| CALL        | regptr16        | 14          | 18         |                            |
| CALL        | memptr16        | 23          | 31         |                            |
| CALL        | far-proc        | 21          | 29         |                            |
| CALL        | memptr32        | 31          | 47         |                            |
| RET (RETN)  | near            | 15          | 19         |                            |
| RET (RETF)  | far             | 21          | 29         |                            |
| RET (RETN)  | near, pop-value | 20          | 24         | Pop imm16 bytes from stack |
| RET (RETF)  | far, pop-value  | 24          | 32         | Pop imm16 bytes from stack |
| RETI (IRET) | —               | 27          | 39         |                            |

---

## Conditional Branches

All conditional branches use short-label (relative 8-bit displacement).

| Mnemonic  | Intel Equivalent | Taken | Not Taken | Condition                    |
|-----------|------------------|-------|-----------|------------------------------|
| BC / BL   | JC / JB          | 14    | 4         | CY = 1                       |
| BNC / BNL | JNC / JAE        | 14    | 4         | CY = 0                       |
| BZ / BE   | JZ / JE          | 14    | 4         | Z = 1                        |
| BNZ / BNE | JNZ / JNE        | 14    | 4         | Z = 0                        |
| BN / BM   | JS               | 14    | 4         | S = 1                        |
| BP        | JNS              | 14    | 4         | S = 0                        |
| BV        | JO               | 14    | 4         | V = 1                        |
| BNV       | JNO              | 14    | 4         | V = 0                        |
| BPE       | JP               | 14    | 4         | P = 1                        |
| BPO       | JNP              | 14    | 4         | P = 0                        |
| BLT       | JL               | 14    | 4         | S xor V = 1                  |
| BGE       | JGE              | 4     | 14        | S xor V = 0 (swapped)        |
| BLE       | JLE              | 14    | 4         | (S xor V) or Z = 1           |
| BGT       | JG               | 4     | 14        | (S xor V) or Z = 0 (swapped) |
| BH        | JA               | 4     | 14        | CY or Z = 0 (swapped)        |
| BNH       | JBE              | 14    | 4         | CY or Z = 1                  |

### Loop / Count Branches

| Mnemonic | Intel Equivalent | Condition Met | Condition Not Met | Notes               |
|----------|------------------|---------------|-------------------|---------------------|
| BCWZ     | JCXZ             | 13 (CW = 0)   | 5 (CW != 0)       |                     |
| DBNZ     | LOOP             | 13 (CW != 0)  | 5 (CW = 0)        | Decrements CW first |
| DBNZE    | LOOPZ / LOOPE    | 14 (taken)    | 5 (not taken)     | CW != 0 and Z = 1   |
| DBNZNE   | LOOPNZ / LOOPNE  | 14 (taken)    | 5 (not taken)     | CW != 0 and Z = 0   |

---

## Interrupt / Control

| Mnemonic       | Operand | Clocks Even | Clocks Odd | Notes                         |
|----------------|---------|-------------|------------|-------------------------------|
| BRK 3 (INT 3)  | —       | 38          | 50         | Breakpoint interrupt          |
| BRK (INT)      | imm8    | 38          | 50         | Software interrupt            |
| BRKV (INTO)    | V = 1   | 40          | 52         | Overflow trap taken           |
| BRKV (INTO)    | V = 0   | 3           | —          | Overflow trap not taken       |
| RETI (IRET)    | —       | 27          | 39         | Return from interrupt         |
| NOP            | —       | 3           | —          |                               |
| HALT (HLT)     | —       | 2           | —          | Halt until interrupt          |
| BUSLOCK (LOCK) | —       | 2           | —          | Prefix: lock bus              |
| CALLN (BRKCS)  | imm8    | 38          | 58         | V20/V30 native mode interrupt |
| BRKEM          | imm8    | 38          | 50         | Break to emulation mode       |
| RETEM          | —       | 27          | 39         | Return from emulation mode    |

---

## Prefix Overheads

| Prefix                                           | Clocks | Notes                     |
|--------------------------------------------------|--------|---------------------------|
| REP / REPC / REPE / REPNC / REPNE / REPNZ / REPZ | 2      | Added to instruction time |
| DS0: (ES:)                                       | 2      | Segment override prefix   |
| DS1: (CS:)                                       | 2      | Segment override prefix   |
| SS:                                              | 2      | Segment override prefix   |
| PS: (DS:)                                        | 2      | Segment override prefix   |

---

## Bit Field Operations (V20/V30 specific)

| Mnemonic | Operand     | Byte / Reg | Word Even | Word Odd | Notes             |
|----------|-------------|------------|-----------|----------|-------------------|
| INS      | reg8, reg8' | 35–113     | 31–117    | 35–113   | Insert bit field  |
| INS      | reg8, imm4  | 75–103     | 67–87     | 75–103   | Insert bit field  |
| EXT      | reg8, reg8' | 34–59      | 26–55     | 34–59    | Extract bit field |
| EXT      | reg8, imm4  | 25–52      | 21–44     | 25–52    | Extract bit field |

---

## Miscellaneous

| Mnemonic        | Operand                     | Clocks Even   | Clocks Odd     | Notes                |
|-----------------|-----------------------------|---------------|----------------|----------------------|
| CHKIND (BOUND)  | reg16, mem32 (interrupt)    | 53–56         | 73–76          | Interrupt generated  |
| CHKIND (BOUND)  | reg16, mem32 (no interrupt) | 18            | 26             | No interrupt         |
| PREPARE (ENTER) | imm16, 0                    | 12            | 16             | Nesting level = 0    |
| PREPARE (ENTER) | imm16, L (L >= 1)           | 19 + 8\*(L-1) | 23 + 16\*(L-1) | L = nesting level    |
| DISPOSE (LEAVE) | —                           | 6             | 10             |                      |
| FP01/FP02 (ESC) | reg                         | 2             | —              | FPU escape, register |
| FP01/FP02 (ESC) | mem                         | 11            | 15             | FPU escape, memory   |
| POLL            | —                           | 2 + 5\*poll   | —              | Poll pin check       |

## Flag Control

| Mnemonic             | Intel Equivalent | Clocks | Notes                      |
|----------------------|------------------|--------|----------------------------|
| CY (CLC / STC / CMC) | CLC / STC / CMC  | 2      | Clear/set/complement carry |
| DIR (CLD / STD)      | CLD / STD        | 2      | Clear/set direction        |
| DI (CLI)             | CLI              | 2      | Disable interrupts         |
| EI (STI)             | STI              | 2      | Enable interrupts          |
