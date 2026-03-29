# Intel 80286 Instruction Execution Clock Cycles

**Primary source:** Intel, *80286 Programmer's Reference Manual*, Instruction Dictionary timing tables (`Clocks` column and per-instruction footnotes).

## Scope

- This document targets the instruction forms currently implemented in `crates/cpu/src/i286/`.
- Mnemonics use Intel 80286 naming.
- Values are best-case core clocks. Memory and bus penalties are listed explicitly.
- Protected-mode-only alternate timings from the manual are noted where relevant, but the current `i286` core primarily models real-mode timing paths.

## Global Timing Rules (Manual + Current Core)

- `+2 clocks` per 16-bit memory transfer at an odd physical address (manual rule).
  In the current core this is modeled as `+4` per odd 16-bit transfer because each misaligned word costs two extra byte bus cycles.
- `+1 clock` for base+index+displacement effective-address form (manual rule).
  Applied inside `calc_ea()` for modrm modes 1/2 with rm 0–3 (BX+SI, BX+DI, BP+SI, BP+DI with displacement).
  LEA compensates for this since it computes an address without memory access.
- `+1 clock` per memory-read wait state (manual rule).
- Many control-transfer instructions in the manual add clocks based on bytes of the next instruction.
  Current `i286` uses fixed values matching its per-opcode timing model.
- Segment override prefix: `2` clocks.
- `LOCK` prefix: `2` clocks.

## Notation

- `reg` means register operand form (`modrm >= C0h`).
- `mem` means memory operand form (`modrm < C0h`).
- `odd` means odd-address penalty when 16-bit access is misaligned.
- `n` is shift/rotate count.
- `SP odd` means stack pointer odd-alignment penalty (`+4` per pushed/popped word in this core).

---

## Data Transfer

| Mnemonic | Operand/Form   |       Base Clocks |                    Alt Clocks | Notes                    |
|----------|----------------|------------------:|------------------------------:|--------------------------|
| `MOV`    | `r/m8, r8`     |           `reg 2` |                       `mem 9` |                          |
| `MOV`    | `r/m16, r16`   |           `reg 2` |              `mem 9 (+4 odd)` |                          |
| `MOV`    | `r8, r/m8`     |           `reg 2` |                      `mem 11` |                          |
| `MOV`    | `r16, r/m16`   |           `reg 2` |             `mem 11 (+4 odd)` |                          |
| `MOV`    | `r/m8, imm8`   |           `reg 4` |                      `mem 11` |                          |
| `MOV`    | `r/m16, imm16` |           `reg 4` |             `mem 11 (+4 odd)` |                          |
| `MOV`    | `r8, imm8`     |               `4` |                             - |                          |
| `MOV`    | `r16, imm16`   |               `4` |                             - |                          |
| `MOV`    | `AL, moffs8`   |              `10` |                             - |                          |
| `MOV`    | `AX, moffs16`  |              `10` |                   `14 if odd` |                          |
| `MOV`    | `moffs8, AL`   |               `9` |                             - |                          |
| `MOV`    | `moffs16, AX`  |               `9` |                   `13 if odd` |                          |
| `MOV`    | `r/m16, Sreg`  |           `reg 2` |             `mem 10 (+4 odd)` |                          |
| `MOV`    | `Sreg, r/m16`  |           `reg 2` |             `mem 11 (+4 odd)` |                          |
| `LEA`    | `r16, m`       |               `4` |                             — |                          |
| `LES`    | `r16, m16:16`  |              `18` |                   `26 if odd` | 2 word reads             |
| `LDS`    | `r16, m16:16`  |              `18` |                   `26 if odd` | 2 word reads             |
| `XCHG`   | `r8, r/m8`     |           `reg 3` |                      `mem 16` |                          |
| `XCHG`   | `r16, r/m16`   |           `reg 3` |             `mem 16 (+8 odd)` | read+write word          |
| `XCHG`   | `AX, r16`      |               `3` |                             — | `90h` with `AX` is `NOP` |
| `POP`    | `r/m16`        | `reg 8 (+SP odd)` | `mem 17 (+SP odd, +4 odd EA)` | opcode `8F /0`           |
| `XLAT`   | implicit       |               `9` |                             — |                          |

---

## Arithmetic and Logic

### ADD / ADC / SBB / SUB / AND / OR / XOR

| Form                      | Base Clocks |        Alt Clocks | Notes           |
|---------------------------|------------:|------------------:|-----------------|
| `r/m8, r8`                |     `reg 2` |          `mem 16` |                 |
| `r/m16, r16`              |     `reg 2` | `mem 16 (+8 odd)` | word read+write |
| `r8, r/m8`                |     `reg 2` |          `mem 11` |                 |
| `r16, r/m16`              |     `reg 2` | `mem 11 (+4 odd)` |                 |
| `AL/AX, imm`              |         `4` |                 — |                 |
| `r/m8, imm8`              |     `reg 4` |          `mem 18` | group `80`      |
| `r/m16, imm16/imm8(sext)` |     `reg 4` | `mem 18 (+8 odd)` | groups `81/83`  |

### CMP

| Form                      | Base Clocks |        Alt Clocks | Notes                |
|---------------------------|------------:|------------------:|----------------------|
| `r/m8, r8`                |     `reg 2` |          `mem 11` |                      |
| `r/m16, r16`              |     `reg 2` | `mem 11 (+4 odd)` |                      |
| `r8, r/m8`                |     `reg 2` |          `mem 11` |                      |
| `r16, r/m16`              |     `reg 2` | `mem 11 (+4 odd)` |                      |
| `AL/AX, imm`              |         `4` |                 — |                      |
| `r/m8, imm8`              |     `reg 4` |          `mem 13` | group `80/82`, `/7`  |
| `r/m16, imm16/imm8(sext)` |     `reg 4` | `mem 13 (+4 odd)` | groups `81/83`, `/7` |

### TEST / NOT / NEG / INC / DEC

| Mnemonic | Operand/Form   | Base Clocks |        Alt Clocks | Notes      |
|----------|----------------|------------:|------------------:|------------|
| `TEST`   | `r/m8, r8`     |     `reg 2` |          `mem 10` |            |
| `TEST`   | `r/m16, r16`   |     `reg 2` | `mem 10 (+4 odd)` |            |
| `TEST`   | `AL/AX, imm`   |         `4` |                 — |            |
| `TEST`   | `r/m8, imm8`   |     `reg 4` |          `mem 11` | `F6 /0,/1` |
| `TEST`   | `r/m16, imm16` |     `reg 4` | `mem 11 (+4 odd)` | `F7 /0,/1` |
| `NOT`    | `r/m8`         |     `reg 2` |          `mem 16` | `F6 /2`    |
| `NOT`    | `r/m16`        |     `reg 2` | `mem 16 (+8 odd)` | `F7 /2`    |
| `NEG`    | `r/m8`         |     `reg 2` |          `mem 16` | `F6 /3`    |
| `NEG`    | `r/m16`        |     `reg 2` | `mem 16 (+8 odd)` | `F7 /3`    |
| `INC`    | `r16`          |         `2` |                 — | `40..47`   |
| `DEC`    | `r16`          |         `2` |                 — | `48..4F`   |
| `INC`    | `r/m8`         |     `reg 2` |          `mem 16` | `FE /0`    |
| `DEC`    | `r/m8`         |     `reg 2` |          `mem 16` | `FE /1`    |
| `INC`    | `r/m16`        |     `reg 2` | `mem 16 (+8 odd)` | `FF /0`    |
| `DEC`    | `r/m16`        |     `reg 2` | `mem 16 (+8 odd)` | `FF /1`    |

### Decimal/Adjust and Conversion

| Mnemonic | Clocks | Notes        |
|----------|-------:|--------------|
| `DAA`    |    `3` |              |
| `DAS`    |    `3` |              |
| `AAA`    |    `3` |              |
| `AAS`    |    `3` |              |
| `AAM`    |   `16` |              |
| `AAD`    |   `14` |              |
| `CBW`    |    `2` |              |
| `CWD`    |    `5` |              |
| `SALC`   |    `2` | Undocumented |

### BOUND

| Mnemonic | Form                     |                       Clocks | Notes                                              |
|----------|--------------------------|-----------------------------:|----------------------------------------------------|
| `BOUND`  | `r16, m16:16` (in-range) |             `18 (+8 odd EA)` |                                                    |
| `BOUND`  | `r16, m16:16` (fault)    | `56 (+8 odd EA, +12 SP odd)` | Includes fault stack push overhead in current core |

---

## Multiply and Divide

| Mnemonic | Operand/Form        | Base Clocks |        Alt Clocks | Notes   |
|----------|---------------------|------------:|------------------:|---------|
| `MUL`    | `r/m8`              |    `reg 22` |          `mem 28` | `F6 /4` |
| `IMUL`   | `r/m8`              |    `reg 39` |          `mem 45` | `F6 /5` |
| `DIV`    | `r/m8`              |    `reg 19` |          `mem 25` | `F6 /6` |
| `IDIV`   | `r/m8`              |    `reg 34` |          `mem 40` | `F6 /7` |
| `MUL`    | `r/m16`             |    `reg 30` | `mem 36 (+4 odd)` | `F7 /4` |
| `IMUL`   | `r/m16`             |    `reg 47` | `mem 53 (+4 odd)` | `F7 /5` |
| `DIV`    | `r/m16`             |    `reg 25` | `mem 31 (+4 odd)` | `F7 /6` |
| `IDIV`   | `r/m16`             |    `reg 43` | `mem 49 (+4 odd)` | `F7 /7` |
| `IMUL`   | `r16, r/m16, imm16` |    `reg 42` | `mem 48 (+4 odd)` | `69 /r` |
| `IMUL`   | `r16, r/m16, imm8`  |    `reg 34` | `mem 40 (+4 odd)` | `6B /r` |

---

## Shift and Rotate

Applies to `ROL`, `ROR`, `RCL`, `RCR`, `SHL/SAL`, `SHR`, `SAR` (including undocumented `/6` alias as `SHL`).

| Form          | Base Clocks |          Alt Clocks | Notes          |
|---------------|------------:|--------------------:|----------------|
| `r/m8, 1`     |     `reg 2` |            `mem 16` | `D0`           |
| `r/m16, 1`    |     `reg 2` |   `mem 16 (+8 odd)` | `D1`           |
| `r/m8, CL`    |   `reg 7+n` |          `mem 19+n` | `D2`, `n=CL`   |
| `r/m16, CL`   |   `reg 7+n` | `mem 19+n (+8 odd)` | `D3`, `n=CL`   |
| `r/m8, imm8`  |   `reg 7+n` |          `mem 19+n` | `C0`, `n=imm8` |
| `r/m16, imm8` |   `reg 7+n` | `mem 19+n (+8 odd)` | `C1`, `n=imm8` |

---

## String and REP

### Single-Instruction String Ops

| Mnemonic | Base Clocks |           Alt Clocks | Notes                                       |
|----------|------------:|---------------------:|---------------------------------------------|
| `MOVSB`  |        `11` |                    — | `8` internal + `3` dispatch overhead (`A4`) |
| `MOVSW`  |        `11` | `19 if SI or DI odd` | `8` internal + `3` dispatch overhead (`A5`) |
| `CMPSB`  |        `14` |                    — |                                             |
| `CMPSW`  |        `14` | `22 if SI or DI odd` |                                             |
| `STOSB`  |         `4` |                    — |                                             |
| `STOSW`  |         `4` |        `8 if DI odd` |                                             |
| `LODSB`  |         `9` |                    — |                                             |
| `LODSW`  |         `9` |       `13 if SI odd` |                                             |
| `SCASB`  |        `10` |                    — |                                             |
| `SCASW`  |        `10` |       `14 if DI odd` |                                             |
| `INSB`   |         `8` |                    — |                                             |
| `INSW`   |         `8` |       `16 if DI odd` |                                             |
| `OUTSB`  |         `8` |                    — |                                             |
| `OUTSW`  |         `8` |       `16 if SI odd` |                                             |

### REP Prefix Startup

| Prefix + Opcode Class                              | Startup Clocks | Notes                            |
|----------------------------------------------------|---------------:|----------------------------------|
| `REP MOVSB/MOVSW`                                  |           `11` |                                  |
| `REP/REPE/REPNE CMPS*/SCAS*/STOS*/LODS*`           |            `7` |                                  |
| `REP INS*/OUTS*`                                   |            `9` |                                  |
| `REP*` before non-string opcode                    |            `2` | Falls through to normal dispatch |
| Additional segment override between REP and opcode |      `+2 each` | `ES`, `CS`, `SS`, `DS`           |
| `LOCK` between REP and opcode                      |           `+2` |                                  |

REP loop body uses the same per-iteration clocks as non-REP forms listed above.

---

## Control Transfer

### Conditional Jumps and Loops

| Mnemonic Group                                  | Taken | Not Taken | Notes                                            |
|-------------------------------------------------|------:|----------:|--------------------------------------------------|
| `JO/JNO/JB/JNB/JZ/JNZ/JBE/JS/JNS/JP/JNP/JL/JLE` |   `7` |       `3` | short forms `70..7E` excluding `77/7D/7F`        |
| `JA/JGE/JG`                                     |   `7` |       `3` |                                                  |
| `LOOPNE/LOOPE`                                  |   `8` |       `4` |                                                  |
| `LOOP`                                          |   `8` |       `4` |                                                  |
| `JCXZ`                                          |   `8` |       `4` |                                                  |

### CALL / JMP / RET

| Mnemonic | Operand/Form                    |        Base Clocks |                    Alt Clocks | Notes |
|----------|---------------------------------|-------------------:|------------------------------:|-------|
| `CALL`   | near relative (`E8`)            |                `7` |                `11 if SP odd` |       |
| `CALL`   | far immediate ptr16:16 (`9A`)   |               `13` |                `21 if SP odd` |       |
| `CALL`   | near indirect `r/m16` (`FF /2`) |  `reg 7 (+SP odd)` | `mem 11 (+SP odd, +4 odd EA)` |       |
| `CALL`   | far indirect `m16:16` (`FF /3`) |               `29` |        `+8 odd EA, +8 SP odd` |       |
| `JMP`    | near relative (`E9`)            |                `7` |                             — |       |
| `JMP`    | short relative (`EB`)           |                `7` |                             — |       |
| `JMP`    | far immediate ptr16:16 (`EA`)   |               `11` |                             — |       |
| `JMP`    | near indirect `r/m16` (`FF /4`) |            `reg 7` |          `mem 11 (+4 odd EA)` |       |
| `JMP`    | far indirect `m16:16` (`FF /5`) |               `15` |                `23 if odd EA` |       |
| `RET`    | near (`C3`)                     |               `11` |                `15 if SP odd` |       |
| `RET`    | near + imm16 (`C2`)             |               `11` |                `15 if SP odd` |       |
| `RET`    | far (`CB`)                      |               `15` |                `23 if SP odd` |       |
| `RET`    | far + imm16 (`CA`)              |               `15` |                `23 if SP odd` |       |

---

## Stack, Flags, Interrupt, and Prefix-Control

| Mnemonic                      | Operand/Form         |       Base Clocks |                    Alt Clocks | Notes                                 |
|-------------------------------|----------------------|------------------:|------------------------------:|---------------------------------------|
| `PUSH`                        | `r16`                |               `8` |                `12 if SP odd` |                                       |
| `PUSH`                        | `Sreg`               |               `8` |                `12 if SP odd` |                                       |
| `PUSH`                        | `imm16`              |               `8` |                `12 if SP odd` |                                       |
| `PUSH`                        | `imm8`               |               `7` |                `11 if SP odd` | sign-extended                         |
| `PUSHA`                       | implicit             |              `35` |                `67 if SP odd` |                                       |
| `PUSH`                        | `r/m16` (`FF /6,/7`) | `reg 8 (+SP odd)` | `mem 18 (+SP odd, +4 odd EA)` | `/7` treated as alias in current core |
| `POP`                         | `r16`                |               `8` |                `12 if SP odd` |                                       |
| `POP`                         | `Sreg`               |               `8` |                `12 if SP odd` |                                       |
| `POPA`                        | implicit             |              `43` |                `75 if SP odd` |                                       |
| `PUSHF`                       | implicit             |               `8` |                `12 if SP odd` |                                       |
| `POPF`                        | implicit             |               `8` |                `12 if SP odd` |                                       |
| `SAHF`                        | implicit             |               `3` |                             — |                                       |
| `LAHF`                        | implicit             |               `2` |                             — |                                       |
| `ENTER`                       | `imm16, 0`           |              `12` |                `16 if SP odd` |                                       |
| `ENTER`                       | `imm16, L>=1`        |    `19 + 8*(L-1)` |     `23 + 16*(L-1) if SP odd` |                                       |
| `LEAVE`                       | implicit             |               `6` |                `10 if SP odd` |                                       |
| `INT 3`                       | implicit             |              `38` |                `50 if SP odd` |                                       |
| `INT`                         | `imm8`               |              `38` |                `50 if SP odd` |                                       |
| `INTO`                        | overflow set         |              `40` |                `52 if SP odd` |                                       |
| `INTO`                        | overflow clear       |               `3` |                             — |                                       |
| `IRET`                        | implicit             |              `27` |                `39 if SP odd` |                                       |
| `CLC/STC/CLI/STI/CLD/STD/CMC` | implicit             |               `2` |                             — |                                       |
| `NOP`                         | implicit             |               `3` |                             — |                                       |
| `WAIT`                        | implicit             |               `3` |                             — |                                       |
| `HLT`                         | implicit             |               `2` |                             — |                                       |
| `LOCK`                        | prefix               |               `2` |                             — |                                       |
| Segment override              | prefix               |               `2` |                             — | `ES`, `CS`, `SS`, `DS`                |

---

## I/O

| Mnemonic | Operand/Form | Base Clocks |       Alt Clocks | Notes |
|----------|--------------|------------:|-----------------:|-------|
| `IN`     | `AL, imm8`   |         `5` |                — |       |
| `IN`     | `AX, imm8`   |         `5` |                — |       |
| `OUT`    | `imm8, AL`   |         `3` |                — |       |
| `OUT`    | `imm8, AX`   |         `3` |                — |       |
| `IN`     | `AL, DX`     |         `5` |                — |       |
| `IN`     | `AX, DX`     |         `5` |                — |       |
| `OUT`    | `DX, AL`     |         `3` |                — |       |
| `OUT`    | `DX, AX`     |         `3` |                — |       |

---

## System / Protection Instructions (`0F xx`)

| Opcode/Form | Mnemonic     | Base Clocks | Alt Clocks | Notes |
|-------------|--------------|------------:|-----------:|-------|
| `0F 01 /4`  | `SMSW r/m16` |     `reg 2` |    `mem 3` |       |
| `0F 01 /6`  | `LMSW r/m16` |     `reg 3` |    `mem 6` |       |

Other `0F` sub-opcodes and other `0F 01 /r` forms currently raise invalid opcode fault (`#UD`, interrupt 6 path).

---

## FPU Escape

| Mnemonic         | Operand/Form  | Base Clocks |     Alt Clocks | Notes                                      |
|------------------|---------------|------------:|---------------:|--------------------------------------------|
| `ESC` (`D8..DF`) | register form |         `2` |              — | Modeled as NOP-like escape in current core |
| `ESC` (`D8..DF`) | memory form   |        `11` | `15 if odd EA` |                                            |
