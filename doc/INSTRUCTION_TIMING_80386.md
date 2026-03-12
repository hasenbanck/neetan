# Intel 80386 Instruction Execution Clock Cycles

**Primary source:** Intel, *80386 Programmer's Reference Manual* (1986), instruction timing tables.

## Scope

- This document is an extracted timing reference for 80386 instruction forms listed in the manual's instruction set detail section.
- Clock values are copied from the manual's `Clocks` column and preserve its notation (`pm=`, `ts`, `m`, `n`, etc.).
- The source PDF's text layer has known extraction artifacts. Where needed, rows were normalized from adjacent lines in the same table (notably `REP` and `RET`).
- For `INC`, the `Clocks` values are missing in this PDF text layer on page 303 and are marked accordingly.
- The filename was originally `INSTRUCTION_TIMING_V30.md` but this document is for **Intel 80386** timing.

## Timing Model

The 80386 is a pipelined processor. The clock counts in the manual (Section 17.2.2.3)
assume the instruction has been prefetched, decoded, and is ready for execution, with no
wait states and aligned memory operands.

Unlike the 80286, the 386 pipeline absorbs prefix decode and EA calculation costs. We
therefore model:

- **Prefixes:** 0 clocks for segment overrides, operand-size (`66`), address-size (`67`),
  and `LOCK` (`F0`). The 80286 charges +2 for each; the 386 does not.
- **EA calculation:** No additive penalty. Scaling (×2, ×4, ×8) is free.
- **Misalignment:** A flat +4 penalty when a word operand is at an odd address or a dword
  operand crosses a 4-byte boundary (`ea & 3 != 0`). The manual says misalignment causes
  "extra bus cycles" but gives no formula; +4 is a reasonable approximation.

True cycle accuracy for a pipelined chip like the 386 is impractical from documentation
alone — it would require empirical measurement or a transistor-level replica.

### Control-Transfer `m` Variable

Many control-transfer instructions use the variable `m` in their clock counts, representing
the number of components in the next instruction executed. Components are counted as follows:
the entire displacement (if any) counts as one component, the entire immediate data (if any)
counts as one component, and every other byte of the instruction (including prefix bytes)
each counts as one component.

### Task Switch Times for Exceptions

When an exception causes a task switch, the instruction execution time is increased by the
task switch overhead, which depends on the TSS types and V86 mode:

| Old Task       | New Task: 386 TSS (VM=0) | New Task: 286 TSS |
|----------------|-------------------------:|------------------:|
| 386 TSS (VM=0) |                    `309` |             `282` |
| 386 TSS (VM=1) |                    `314` |             `231` |
| 286 TSS        |                    `307` |             `282` |

## Notation

- `reg/mem` format (e.g. `2/7`) means register-form clocks first, memory-form clocks second.
- `pm=` denotes protected-mode-specific clocks from the manual.
- `ts` denotes task-switch-dependent timing.
- `m`, `n`, `N`, `x` are manual variables used in per-instruction formulas.

## Extracted Clock Table

| Mnemonic                    | Opcode        | Instruction            | Clocks                                                                |
|-----------------------------|---------------|------------------------|-----------------------------------------------------------------------|
| `CMC`                       | `F5`          | `CMC`                  | `2`                                                                   |
| `AAA`                       | `37`          | `AAA`                  | `4`                                                                   |
| `AAD`                       | `D5 0A`       | `AAD`                  | `19`                                                                  |
| `AAM`                       | `D4 0A`       | `AAM`                  | `17`                                                                  |
| `AAS`                       | `3F`          | `AAS`                  | `4`                                                                   |
| `ADC`                       | `14 ib`       | `ADC AL,imm8`          | `2`                                                                   |
| `ADC`                       | `15 iw`       | `ADC AX,imm16`         | `2`                                                                   |
| `ADC`                       | `15 id`       | `ADC EAX,imm32`        | `2`                                                                   |
| `ADC`                       | `80 /2 ib`    | `ADC r/m8,imm8`        | `2/7`                                                                 |
| `ADC`                       | `81 /2 iw`    | `ADC r/m16,imm16`      | `2/7`                                                                 |
| `ADC`                       | `81 /2 id`    | `ADC r/m32,imm32`      | `2/7`                                                                 |
| `ADC`                       | `83 /2 ib`    | `ADC r/m16,imm8`       | `2/7`                                                                 |
| `ADC`                       | `83 /2 ib`    | `ADC r/m32,imm8`       | `2/7`                                                                 |
| `ADC`                       | `10 /r`       | `ADC r/m8,r8`          | `2/7`                                                                 |
| `ADC`                       | `11 /r`       | `ADC r/m16,r16`        | `2/7`                                                                 |
| `ADC`                       | `11 /r`       | `ADC r/m32,r32`        | `2/7`                                                                 |
| `ADC`                       | `12 /r`       | `ADC r8,r/m8`          | `2/6`                                                                 |
| `ADC`                       | `13 /r`       | `ADC r16,r/m16`        | `2/6`                                                                 |
| `ADC`                       | `13 /r`       | `ADC r32,r/m32`        | `2/6`                                                                 |
| `ADD`                       | `04 ib`       | `ADD AL,imm8`          | `2`                                                                   |
| `ADD`                       | `05 iw`       | `ADD AX,imm16`         | `2`                                                                   |
| `ADD`                       | `05 id`       | `ADD EAX,imm32`        | `2`                                                                   |
| `ADD`                       | `80 /0 ib`    | `ADD r/m8,imm8`        | `2/7`                                                                 |
| `ADD`                       | `81 /0 iw`    | `ADD r/m16,imm16`      | `2/7`                                                                 |
| `ADD`                       | `81 /0 id`    | `ADD r/m32,imm32`      | `2/7`                                                                 |
| `ADD`                       | `83 /0 ib`    | `ADD r/m16,imm8`       | `2/7`                                                                 |
| `ADD`                       | `83 /0 ib`    | `ADD r/m32,imm8`       | `2/7`                                                                 |
| `ADD`                       | `00 /r`       | `ADD r/m8,r8`          | `2/7`                                                                 |
| `ADD`                       | `01 /r`       | `ADD r/m16,r16`        | `2/7`                                                                 |
| `ADD`                       | `01 /r`       | `ADD r/m32,r32`        | `2/7`                                                                 |
| `ADD`                       | `02 /r`       | `ADD r8,r/m8`          | `2/6`                                                                 |
| `ADD`                       | `03 /r`       | `ADD r16,r/m16`        | `2/6`                                                                 |
| `ADD`                       | `03 /r`       | `ADD r32,r/m32`        | `2/6`                                                                 |
| `AND`                       | `24 ib`       | `AND AL,imm8`          | `2`                                                                   |
| `AND`                       | `25 iw`       | `AND AX,imm16`         | `2`                                                                   |
| `AND`                       | `25 id`       | `AND EAX,imm32`        | `2`                                                                   |
| `AND`                       | `80 /4 ib`    | `AND r/m8,imm8`        | `2/7`                                                                 |
| `AND`                       | `81 /4 iw`    | `AND r/m16,imm16`      | `2/7`                                                                 |
| `AND`                       | `81 /4 id`    | `AND r/m32,imm32`      | `2/7`                                                                 |
| `AND`                       | `83 /4 ib`    | `AND r/m16,imm8`       | `2/7`                                                                 |
| `AND`                       | `83 /4 ib`    | `AND r/m32,imm8`       | `2/7`                                                                 |
| `AND`                       | `20 /r`       | `AND r/m8,r8`          | `2/7`                                                                 |
| `AND`                       | `21 /r`       | `AND r/m16,r16`        | `2/7`                                                                 |
| `AND`                       | `21 /r`       | `AND r/m32,r32`        | `2/7`                                                                 |
| `AND`                       | `22 /r`       | `AND r8,r/m8`          | `2/6`                                                                 |
| `AND`                       | `23 /r`       | `AND r16,r/m16`        | `2/6`                                                                 |
| `AND`                       | `23 /r`       | `AND r32,r/m32`        | `2/6`                                                                 |
| `ARPL`                      | `63 /r`       | `ARPL r/m16,r16`       | `pm=20/21`                                                            |
| `BOUND`                     | `62 /r`       | `BOUND r16,m16&16`     | `10`                                                                  |
| `BOUND`                     | `62 /r`       | `BOUND r32,m32&32`     | `10`                                                                  |
| `BSF`                       | `0F BC`       | `BSF r16,r/m16`        | `10+3n`                                                               |
| `BSF`                       | `0F BC`       | `BSF r32,r/m32`        | `10+3n`                                                               |
| `BSR`                       | `0F BD`       | `BSR r16,r/m16`        | `10+3n`                                                               |
| `BSR`                       | `0F BD`       | `BSR r32,r/m32`        | `10+3n`                                                               |
| `BT`                        | `0F A3`       | `BT r/m16,r16`         | `3/12`                                                                |
| `BT`                        | `0F A3`       | `BT r/m32,r32`         | `3/12`                                                                |
| `BT`                        | `0F BA /4 ib` | `BT r/m16,imm8`        | `3/6`                                                                 |
| `BT`                        | `0F BA /4 ib` | `BT r/m32,imm8`        | `3/6`                                                                 |
| `BTC`                       | `0F BB`       | `BTC r/m16,r16`        | `6/13`                                                                |
| `BTC`                       | `0F BB`       | `BTC r/m32,r32`        | `6/13`                                                                |
| `BTC`                       | `0F BA /7 ib` | `BTC r/m16,imm8`       | `6/8`                                                                 |
| `BTC`                       | `0F BA /7 ib` | `BTC r/m32,imm8`       | `6/8`                                                                 |
| `BTR`                       | `0F B3`       | `BTR r/m16,r16`        | `6/13`                                                                |
| `BTR`                       | `0F B3`       | `BTR r/m32,r32`        | `6/13`                                                                |
| `BTR`                       | `0F BA /6 ib` | `BTR r/m16,imm8`       | `6/8`                                                                 |
| `BTR`                       | `0F BA /6 ib` | `BTR r/m32,imm8`       | `6/8`                                                                 |
| `BTS`                       | `0F AB`       | `BTS r/m16,r16`        | `6/13`                                                                |
| `BTS`                       | `0F AB`       | `BTS r/m32,r32`        | `6/13`                                                                |
| `BTS`                       | `0F BA /5 ib` | `BTS r/m16,imm8`       | `6/8`                                                                 |
| `BTS`                       | `0F BA /5 ib` | `BTS r/m32,imm8`       | `6/8`                                                                 |
| `CALL`                      | `E8 cw`       | `CALL rel16`           | `7+m`                                                                 |
| `CALL`                      | `FF /2`       | `CALL r/m16`           | `7+m/10+m`                                                            |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `17+m,pm=34+m`                                                        |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=52+m`                                                             |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=86+m`                                                             |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=94+4x+m`                                                          |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `ts`                                                                  |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `22+m,pm=38+m`                                                        |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=56+m`                                                             |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=90+m`                                                             |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=98+4x+m`                                                          |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `5 + ts`                                                              |
| `CALL`                      | `E8 cd`       | `CALL rel32`           | `7+m`                                                                 |
| `CALL`                      | `FF /2`       | `CALL r/m32`           | `7+m/10+m`                                                            |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `17+m,pm=34+m`                                                        |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=52+m`                                                             |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=86+m`                                                             |
| `CALL`                      | `9A cp`       | `CALL ptr32:32`        | `pm=94+4x+m`                                                          |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `ts`                                                                  |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `22+m,pm=38+m`                                                        |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=56+m`                                                             |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=90+m`                                                             |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=98+4x+m`                                                          |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `5 + ts`                                                              |
| `CBW/CWDE`                  | `98`          | `CBW`                  | `3`                                                                   |
| `CBW/CWDE`                  | `98`          | `CWDE`                 | `3`                                                                   |
| `CLC`                       | `F8`          | `CLC`                  | `2`                                                                   |
| `CLD`                       | `FC`          | `CLD`                  | `2`                                                                   |
| `CLI`                       | `FA`          | `CLI`                  | `3`                                                                   |
| `CLTS`                      | `0F 06`       | `CLTS`                 | `5`                                                                   |
| `CMP`                       | `3C ib`       | `CMP AL,imm8`          | `2`                                                                   |
| `CMP`                       | `3D iw`       | `CMP AX,imm16`         | `2`                                                                   |
| `CMP`                       | `3D id`       | `CMP EAX,imm32`        | `2`                                                                   |
| `CMP`                       | `80 /7 ib`    | `CMP r/m8,imm8`        | `2/5`                                                                 |
| `CMP`                       | `81 /7 iw`    | `CMP r/m16,imm16`      | `2/5`                                                                 |
| `CMP`                       | `81 /7 id`    | `CMP r/m32,imm32`      | `2/5`                                                                 |
| `CMP`                       | `83 /7 ib`    | `CMP r/m16,imm8`       | `2/5`                                                                 |
| `CMP`                       | `83 /7 ib`    | `CMP r/m32,imm8`       | `2/5`                                                                 |
| `CMP`                       | `38 /r`       | `CMP r/m8,r8`          | `2/5`                                                                 |
| `CMP`                       | `39 /r`       | `CMP r/m16,r16`        | `2/5`                                                                 |
| `CMP`                       | `39 /r`       | `CMP r/m32,r32`        | `2/5`                                                                 |
| `CMP`                       | `3A /r`       | `CMP r8,r/m8`          | `2/6`                                                                 |
| `CMP`                       | `3B /r`       | `CMP r16,r/m16`        | `2/6`                                                                 |
| `CMP`                       | `3B /r`       | `CMP r32,r/m32`        | `2/6`                                                                 |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A6`          | `CMPS m8,m8`           | `10`                                                                  |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPS m16,m16`         | `10`                                                                  |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPS m32,m32`         | `10`                                                                  |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A6`          | `CMPSB`                | `10`                                                                  |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPSW`                | `10`                                                                  |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPSD`                | `10`                                                                  |
| `CWD/CDQ`                   | `99`          | `CWD`                  | `2`                                                                   |
| `CWD/CDQ`                   | `99`          | `CDQ`                  | `2`                                                                   |
| `DAA`                       | `27`          | `DAA`                  | `4`                                                                   |
| `DAS`                       | `2F`          | `DAS`                  | `4`                                                                   |
| `DEC`                       | `FE /1`       | `DEC r/m8`             | `2/6`                                                                 |
| `DEC`                       | `FF /1`       | `DEC r/m16`            | `2/6`                                                                 |
| `DEC`                       | `48+rw`       | `DEC r16`              | `2`                                                                   |
| `DEC`                       | `48+rw`       | `DEC r32`              | `2`                                                                   |
| `DIV`                       | `F6 /6`       | `DIV AL,r/m8`          | `14/17`                                                               |
| `DIV`                       | `F7 /6`       | `DIV AX,r/m16`         | `22/25`                                                               |
| `DIV`                       | `F7 /6`       | `DIV EAX,r/m32`        | `38/41`                                                               |
| `ENTER`                     | `C8 iw 00`    | `ENTER imm16,0`        | `10`                                                                  |
| `ENTER`                     | `C8 iw 01`    | `ENTER imm16,1`        | `12`                                                                  |
| `ENTER`                     | `C8 iw ib`    | `ENTER imm16,imm8`     | `15+4(n-1)`                                                           |
| `HLT`                       | `F4`          | `HLT`                  | `5`                                                                   |
| `IDIV`                      | `F6 /7`       | `IDIV r/m8`            | `19`                                                                  |
| `IDIV`                      | `F7 /7`       | `IDIV AX,r/m16`        | `27`                                                                  |
| `IDIV`                      | `F7 /7`       | `IDIV EAX,r/m32`       | `43`                                                                  |
| `IMUL`                      | `F6 /5`       | `IMUL r/m8`            | `9-14/12-17`                                                          |
| `IMUL`                      | `F7 /5`       | `IMUL r/m16`           | `9-22/12-25`                                                          |
| `IMUL`                      | `F7 /5`       | `IMUL r/m32`           | `9-38/12-41`                                                          |
| `IMUL`                      | `0F AF /r`    | `IMUL r16,r/m16`       | `9-22/12-25`                                                          |
| `IMUL`                      | `0F AF /r`    | `IMUL r32,r/m32`       | `9-38/12-41`                                                          |
| `IMUL`                      | `6B /r ib`    | `IMUL r16,r/m16,imm8`  | `9-14/12-17`                                                          |
| `IMUL`                      | `6B /r ib`    | `IMUL r32,r/m32,imm8`  | `9-14/12-17`                                                          |
| `IMUL`                      | `6B /r ib`    | `IMUL r16,imm8`        | `9-14/12-17`                                                          |
| `IMUL`                      | `6B /r ib`    | `IMUL r32,imm8`        | `9-14/12-17`                                                          |
| `IMUL`                      | `69 /r iw`    | `IMUL r16,r/m16,imm16` | `9-22/12-25`                                                          |
| `IMUL`                      | `69 /r id`    | `IMUL r32,r/m32,imm32` | `9-38/12-41`                                                          |
| `IMUL`                      | `69 /r iw`    | `IMUL r16,imm16`       | `9-22/12-25`                                                          |
| `IMUL`                      | `69 /r id`    | `IMUL r32,imm32`       | `9-38/12-41`                                                          |
| `IN`                        | `E4 ib`       | `IN AL,imm8`           | `12,pm=6*/26**`                                                       |
| `IN`                        | `E5 ib`       | `IN AX,imm8`           | `12,pm=6*/26**`                                                       |
| `IN`                        | `E5 ib`       | `IN EAX,imm8`          | `12,pm=6*/26**`                                                       |
| `IN`                        | `EC`          | `IN AL,DX`             | `13,pm=7*/27**`                                                       |
| `IN`                        | `ED`          | `IN AX,DX`             | `13,pm=7*/27**`                                                       |
| `IN`                        | `ED`          | `IN EAX,DX`            | `13,pm=7*/27**`                                                       |
| `INC`                       | `FE /0`       | `INC r/m8`             | `N/A (missing in PDF text layer)`                                     |
| `INC`                       | `FF /0`       | `INC r/m16`            | `N/A (missing in PDF text layer)`                                     |
| `INC`                       | `FF /6`       | `INC r/m32`            | `N/A (missing in PDF text layer)`                                     |
| `INC`                       | `40 + rw`     | `INC r16`              | `N/A (missing in PDF text layer)`                                     |
| `INC`                       | `40 + rd`     | `INC r32`              | `N/A (missing in PDF text layer)`                                     |
| `INS/INSB/INSW/INSD`        | `6C`          | `INS r/m8,DX`          | `15,pm=9*/29**`                                                       |
| `INS/INSB/INSW/INSD`        | `6D`          | `INS r/m16,DX`         | `15,pm=9*/29**`                                                       |
| `INS/INSB/INSW/INSD`        | `6D`          | `INS r/m32,DX`         | `15,pm=9*/29**`                                                       |
| `INS/INSB/INSW/INSD`        | `6C`          | `INSB`                 | `15,pm=9*/29**`                                                       |
| `INS/INSB/INSW/INSD`        | `6D`          | `INSW`                 | `15,pm=9*/29**`                                                       |
| `INS/INSB/INSW/INSD`        | `6D`          | `INSD`                 | `15,pm=9*/29**`                                                       |
| `INT/INTO`                  | `CC`          | `INT 3`                | `33`                                                                  |
| `INT/INTO`                  | `CC`          | `INT 3`                | `pm=59`                                                               |
| `INT/INTO`                  | `CC`          | `INT 3`                | `pm=99`                                                               |
| `INT/INTO`                  | `CC`          | `INT 3`                | `pm=119`                                                              |
| `INT/INTO`                  | `CC`          | `INT 3`                | `ts`                                                                  |
| `INT/INTO`                  | `CD ib`       | `INT imm8`             | `37`                                                                  |
| `INT/INTO`                  | `CD ib`       | `INT imm8`             | `pm=59`                                                               |
| `INT/INTO`                  | `CD ib`       | `INT imm8`             | `pm=99`                                                               |
| `INT/INTO`                  | `CD ib`       | `INT imm8`             | `pm=119`                                                              |
| `INT/INTO`                  | `CD ib`       | `INT imm8`             | `ts`                                                                  |
| `INT/INTO`                  | `CE`          | `INTO`                 | `Fail:3,pm=3;`                                                        |
| `INT/INTO`                  | `CE`          | `INTO`                 | `Pass:35`                                                             |
| `INT/INTO`                  | `CE`          | `INTO`                 | `pm=59`                                                               |
| `INT/INTO`                  | `CE`          | `INTO`                 | `pm=99`                                                               |
| `INT/INTO`                  | `CE`          | `INTO`                 | `pm=119`                                                              |
| `INT/INTO`                  | `CE`          | `INTO`                 | `ts`                                                                  |
| `IRET/IRETD`                | `CF`          | `IRET`                 | `22,pm=38`                                                            |
| `IRET/IRETD`                | `CF`          | `IRET`                 | `pm=82`                                                               |
| `IRET/IRETD`                | `CF`          | `IRET`                 | `ts`                                                                  |
| `IRET/IRETD`                | `CF`          | `IRETD`                | `22,pm=38`                                                            |
| `IRET/IRETD`                | `CF`          | `IRETD`                | `pm=82`                                                               |
| `IRET/IRETD`                | `CF`          | `IRETD`                | `pm=60`                                                               |
| `IRET/IRETD`                | `CF`          | `IRETD`                | `ts`                                                                  |
| `Jcc`                       | `77 cb`       | `JA rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `73 cb`       | `JAE rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `72 cb`       | `JB rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `76 cb`       | `JBE rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `72 cb`       | `JC rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `E3 cb`       | `JCXZ rel8`            | `9+m,5`                                                               |
| `Jcc`                       | `E3 cb`       | `JECXZ rel8`           | `9+m,5`                                                               |
| `Jcc`                       | `74 cb`       | `JE rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `74 cb`       | `JZ rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `7F cb`       | `JG rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `7D cb`       | `JGE rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `7C cb`       | `JL rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `7E cb`       | `JLE rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `76 cb`       | `JNA rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `72 cb`       | `JNAE rel8`            | `7+m,3`                                                               |
| `Jcc`                       | `73 cb`       | `JNB rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `77 cb`       | `JNBE rel8`            | `7+m,3`                                                               |
| `Jcc`                       | `73 cb`       | `JNC rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `75 cb`       | `JNE rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `7E cb`       | `JNG rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `7C cb`       | `JNGE rel8`            | `7+m,3`                                                               |
| `Jcc`                       | `7D cb`       | `JNL rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `7F cb`       | `JNLE rel8`            | `7+m,3`                                                               |
| `Jcc`                       | `71 cb`       | `JNO rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `7B cb`       | `JNP rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `79 cb`       | `JNS rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `75 cb`       | `JNZ rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `70 cb`       | `JO rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `7A cb`       | `JP rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `7A cb`       | `JPE rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `7B cb`       | `JPO rel8`             | `7+m,3`                                                               |
| `Jcc`                       | `78 cb`       | `JS rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `74 cb`       | `JZ rel8`              | `7+m,3`                                                               |
| `Jcc`                       | `0F 87 cw/cd` | `JA rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 83 cw/cd` | `JAE rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 82 cw/cd` | `JB rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 86 cw/cd` | `JBE rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 82 cw/cd` | `JC rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 84 cw/cd` | `JE rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 84 cw/cd` | `JZ rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 8F cw/cd` | `JG rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 8D cw/cd` | `JGE rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 8C cw/cd` | `JL rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 8E cw/cd` | `JLE rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 86 cw/cd` | `JNA rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 82 cw/cd` | `JNAE rel16/32`        | `7+m,3`                                                               |
| `Jcc`                       | `0F 83 cw/cd` | `JNB rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 87 cw/cd` | `JNBE rel16/32`        | `7+m,3`                                                               |
| `Jcc`                       | `0F 83 cw/cd` | `JNC rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 85 cw/cd` | `JNE rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 8E cw/cd` | `JNG rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 8C cw/cd` | `JNGE rel16/32`        | `7+m,3`                                                               |
| `Jcc`                       | `0F 8D cw/cd` | `JNL rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 8F cw/cd` | `JNLE rel16/32`        | `7+m,3`                                                               |
| `Jcc`                       | `0F 81 cw/cd` | `JNO rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 8B cw/cd` | `JNP rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 89 cw/cd` | `JNS rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 85 cw/cd` | `JNZ rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 80 cw/cd` | `JO rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 8A cw/cd` | `JP rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 8A cw/cd` | `JPE rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 8B cw/cd` | `JPO rel16/32`         | `7+m,3`                                                               |
| `Jcc`                       | `0F 88 cw/cd` | `JS rel16/32`          | `7+m,3`                                                               |
| `Jcc`                       | `0F 84 cw/cd` | `JZ rel16/32`          | `7+m,3`                                                               |
| `JMP`                       | `EB cb`       | `JMP rel8`             | `7+m`                                                                 |
| `JMP`                       | `E9 cw`       | `JMP rel16`            | `7+m`                                                                 |
| `JMP`                       | `FF /4`       | `JMP r/m16`            | `7+m/10+m`                                                            |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `12+m,pm=27+m`                                                        |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `pm=45+m`                                                             |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `ts`                                                                  |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `ts`                                                                  |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `43+m,pm=31+m`                                                        |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `pm=49+m`                                                             |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `5 + ts`                                                              |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `5 + ts`                                                              |
| `JMP`                       | `E9 cd`       | `JMP rel32`            | `7+m`                                                                 |
| `JMP`                       | `FF /4`       | `JMP r/m32`            | `7+m,10+m`                                                            |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `12+m,pm=27+m`                                                        |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `pm=45+m`                                                             |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `ts`                                                                  |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `ts`                                                                  |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `43+m,pm=31+m`                                                        |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `pm=49+m`                                                             |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `5 + ts`                                                              |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `5 + ts`                                                              |
| `LAHF`                      | `9F`          | `LAHF`                 | `2`                                                                   |
| `LAR`                       | `0F 02 /r`    | `LAR r16,r/m16`        | `pm=15/16`                                                            |
| `LAR`                       | `0F 02 /r`    | `LAR r32,r/m32`        | `pm=15/16`                                                            |
| `LEA`                       | `8D /r`       | `LEA r16,m`            | `2`                                                                   |
| `LEA`                       | `8D /r`       | `LEA r32,m`            | `2`                                                                   |
| `LEA`                       | `8D /r`       | `LEA r16,m`            | `2`                                                                   |
| `LEA`                       | `8D /r`       | `LEA r32,m`            | `2`                                                                   |
| `LEAVE`                     | `C9`          | `LEAVE`                | `4`                                                                   |
| `LEAVE`                     | `C9`          | `LEAVE`                | `4`                                                                   |
| `LGDT/LIDT`                 | `0F 01 /2`    | `LGDT m16&32`          | `11`                                                                  |
| `LGDT/LIDT`                 | `0F 01 /3`    | `LIDT m16&32`          | `11`                                                                  |
| `LGS/LSS/LDS/LES/LFS`       | `C5 /r`       | `LDS r16,m16:16`       | `7,p=22`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `C5 /r`       | `LDS r32,m16:32`       | `7,p=22`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `0F B2 /r`    | `LSS r16,m16:16`       | `7,p=22`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `0F B2 /r`    | `LSS r32,m16:32`       | `7,p=22`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `C4 /r`       | `LES r16,m16:16`       | `7,p=22`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `C4 /r`       | `LES r32,m16:32`       | `7,p=22`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `0F B4 /r`    | `LFS r16,m16:16`       | `7,p=25`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `0F B4 /r`    | `LFS r32,m16:32`       | `7,p=25`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `0F B5 /r`    | `LGS r16,m16:16`       | `7,p=25`                                                              |
| `LGS/LSS/LDS/LES/LFS`       | `0F B5 /r`    | `LGS r32,m16:32`       | `7,p=25`                                                              |
| `LLDT`                      | `0F 00 /2`    | `LLDT r/m16`           | `20`                                                                  |
| `LMSW`                      | `0F 01 /6`    | `LMSW r/m16`           | `10/13`                                                               |
| `LOCK`                      | `F0`          | `LOCK`                 | `0`                                                                   |
| `LODS/LODSB/LODSW/LODSD`    | `AC`          | `LODS m8`              | `5`                                                                   |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODS m16`             | `5`                                                                   |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODS m32`             | `5`                                                                   |
| `LODS/LODSB/LODSW/LODSD`    | `AC`          | `LODSB`                | `5`                                                                   |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODSW`                | `5`                                                                   |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODSD`                | `5`                                                                   |
| `LOOP/LOOPcond`             | `E2 cb`       | `LOOP rel8`            | `11+m`                                                                |
| `LOOP/LOOPcond`             | `E1 cb`       | `LOOPE rel8`           | `11+m`                                                                |
| `LOOP/LOOPcond`             | `E1 cb`       | `LOOPZ rel8`           | `11+m`                                                                |
| `LOOP/LOOPcond`             | `E0 cb`       | `LOOPNE rel8`          | `11+m`                                                                |
| `LOOP/LOOPcond`             | `E0 cb`       | `LOOPNZ rel8`          | `11+m`                                                                |
| `LSL`                       | `0F 03 /r`    | `LSL r16,r/m16`        | `pm=20/21`                                                            |
| `LSL`                       | `0F 03 /r`    | `LSL r32,r/m32`        | `pm=20/21`                                                            |
| `LSL`                       | `0F 03 /r`    | `LSL r16,r/m16`        | `pm=25/26`                                                            |
| `LSL`                       | `0F 03 /r`    | `LSL r32,r/m32`        | `pm=25/26`                                                            |
| `LTR`                       | `0F 00 /3`    | `LTR r/m16`            | `pm=23/27`                                                            |
| `MOV`                       | `88 /r`       | `MOV r/m8,r8`          | `2/2`                                                                 |
| `MOV`                       | `89 /r`       | `MOV r/m16,r16`        | `2/2`                                                                 |
| `MOV`                       | `89 /r`       | `MOV r/m32,r32`        | `2/2`                                                                 |
| `MOV`                       | `8A /r`       | `MOV r8,r/m8`          | `2/4`                                                                 |
| `MOV`                       | `8B /r`       | `MOV r16,r/m16`        | `2/4`                                                                 |
| `MOV`                       | `8B /r`       | `MOV r32,r/m32`        | `2/4`                                                                 |
| `MOV`                       | `8C /r`       | `MOV r/m16,Sreg`       | `2/2`                                                                 |
| `MOV`                       | `8D /r`       | `MOV Sreg,r/m16`       | `2/5,pm=18/19`                                                        |
| `MOV`                       | `A0`          | `MOV AL,moffs8`        | `4`                                                                   |
| `MOV`                       | `A1`          | `MOV AX,moffs16`       | `4`                                                                   |
| `MOV`                       | `A1`          | `MOV EAX,moffs32`      | `4`                                                                   |
| `MOV`                       | `A2`          | `MOV moffs8,AL`        | `2`                                                                   |
| `MOV`                       | `A3`          | `MOV moffs16,AX`       | `2`                                                                   |
| `MOV`                       | `A3`          | `MOV moffs32,EAX`      | `2`                                                                   |
| `MOV`                       | `B0 + rb`     | `MOV reg8,imm8`        | `2`                                                                   |
| `MOV`                       | `B8 + rw`     | `MOV reg16,imm16`      | `2`                                                                   |
| `MOV`                       | `B8 + rd`     | `MOV reg32,imm32`      | `2`                                                                   |
| `MOV`                       | `Ciiiiii`     | `MOV r/m8,imm8`        | `2/2`                                                                 |
| `MOV`                       | `C7`          | `MOV r/m16,imm16`      | `2/2`                                                                 |
| `MOV`                       | `C7`          | `MOV r/m32,imm32`      | `2/2`                                                                 |
| `MOV`                       | `0F 20 /r`    | `MOV r32,CR0/CR2/CR3`  | `6`                                                                   |
| `MOV`                       | `0F 22 /r`    | `MOV CR0/CR2/CR3,r32`  | `10/4/5`                                                              |
| `MOV`                       | `0F 21 /r`    | `MOV r32,DR0 -- 3`     | `22`                                                                  |
| `MOV`                       | `0F 21 /r`    | `MOV r32,DR6/DR7`      | `14`                                                                  |
| `MOV`                       | `0F 23 /r`    | `MOV DR0 -- 3,r32`     | `22`                                                                  |
| `MOV`                       | `0F 23 /r`    | `MOV DR6/DR7,r32`      | `16`                                                                  |
| `MOV`                       | `0F 24 /r`    | `MOV r32,TR6/TR7`      | `12`                                                                  |
| `MOV`                       | `0F 26 /r`    | `MOV TR6/TR7,r32`      | `12`                                                                  |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A4`          | `MOVS m8,m8`           | `7`                                                                   |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVS m16,m16`         | `7`                                                                   |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVS m32,m32`         | `7`                                                                   |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A4`          | `MOVSB`                | `7`                                                                   |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVSW`                | `7`                                                                   |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVSD`                | `7`                                                                   |
| `MOVSX`                     | `0F BE /r`    | `MOVSX r16,r/m8`       | `3/6`                                                                 |
| `MOVSX`                     | `0F BE /r`    | `MOVSX r32,r/m8`       | `3/6`                                                                 |
| `MOVSX`                     | `0F BF /r`    | `MOVSX r32,r/m16`      | `3/6`                                                                 |
| `MOVZX`                     | `0F B6 /r`    | `MOVZX r16,r/m8`       | `3/6`                                                                 |
| `MOVZX`                     | `0F B6 /r`    | `MOVZX r32,r/m8`       | `3/6`                                                                 |
| `MOVZX`                     | `0F B7 /r`    | `MOVZX r32,r/m16`      | `3/6`                                                                 |
| `MUL`                       | `F6 /4`       | `MUL AL,r/m8`          | `9-14/12-17`                                                          |
| `MUL`                       | `F7 /4`       | `MUL AX,r/m16`         | `9-22/12-25`                                                          |
| `MUL`                       | `F7 /4`       | `MUL EAX,r/m32`        | `9-38/12-41`                                                          |
| `NEG`                       | `F6 /3`       | `NEG r/m8`             | `2/6`                                                                 |
| `NEG`                       | `F7 /3`       | `NEG r/m16`            | `2/6`                                                                 |
| `NEG`                       | `F7 /3`       | `NEG r/m32`            | `2/6`                                                                 |
| `NOP`                       | `90`          | `NOP`                  | `3`                                                                   |
| `NOT`                       | `F6 /2`       | `NOT r/m8`             | `2/6`                                                                 |
| `NOT`                       | `F7 /2`       | `NOT r/m16`            | `2/6`                                                                 |
| `NOT`                       | `F7 /2`       | `NOT r/m32`            | `2/6`                                                                 |
| `OR`                        | `0C ib`       | `OR AL,imm8`           | `2`                                                                   |
| `OR`                        | `0D iw`       | `OR AX,imm16`          | `2`                                                                   |
| `OR`                        | `0D id`       | `OR EAX,imm32`         | `2`                                                                   |
| `OR`                        | `80 /1 ib`    | `OR r/m8,imm8`         | `2/7`                                                                 |
| `OR`                        | `81 /1 iw`    | `OR r/m16,imm16`       | `2/7`                                                                 |
| `OR`                        | `81 /1 id`    | `OR r/m32,imm32`       | `2/7`                                                                 |
| `OR`                        | `83 /1 ib`    | `OR r/m16,imm8`        | `2/7`                                                                 |
| `OR`                        | `83 /1 ib`    | `OR r/m32,imm8`        | `2/7`                                                                 |
| `OR`                        | `08 /r`       | `OR r/m8,r8`           | `2/7`                                                                 |
| `OR`                        | `09 /r`       | `OR r/m16,r16`         | `2/7`                                                                 |
| `OR`                        | `09 /r`       | `OR r/m32,r32`         | `2/7`                                                                 |
| `OR`                        | `0A /r`       | `OR r8,r/m8`           | `2/6`                                                                 |
| `OR`                        | `0B /r`       | `OR r16,r/m16`         | `2/6`                                                                 |
| `OR`                        | `0B /r`       | `OR r32,r/m32`         | `2/6`                                                                 |
| `OUT`                       | `E6 ib`       | `OUT imm8,AL`          | `10,pm=4*/24**`                                                       |
| `OUT`                       | `E7 ib`       | `OUT imm8,AX`          | `10,pm=4*/24**`                                                       |
| `OUT`                       | `E7 ib`       | `OUT imm8,EAX`         | `10,pm=4*/24**`                                                       |
| `OUT`                       | `EE`          | `OUT DX,AL`            | `11,pm=5*/25**`                                                       |
| `OUT`                       | `EF`          | `OUT DX,AX`            | `11,pm=5*/25**`                                                       |
| `OUT`                       | `EF`          | `OUT DX,EAX`           | `11,pm=5*/25**`                                                       |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6E`          | `OUTS DX,r/m8`         | `14,pm=8*/28**`                                                       |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTS DX,r/m16`        | `14,pm=8*/28**`                                                       |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTS DX,r/m32`        | `14,pm=8*/28**`                                                       |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6E`          | `OUTSB`                | `14,pm=8*/28**`                                                       |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTSW`                | `14,pm=8*/28**`                                                       |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTSD`                | `14,pm=8*/28**`                                                       |
| `POP`                       | `8F /0`       | `POP m16`              | `5`                                                                   |
| `POP`                       | `8F /0`       | `POP m32`              | `5`                                                                   |
| `POP`                       | `58 + rw`     | `POP r16`              | `4`                                                                   |
| `POP`                       | `58 + rd`     | `POP r32`              | `4`                                                                   |
| `POP`                       | `1F`          | `POP DS`               | `7,pm=21`                                                             |
| `POP`                       | `07`          | `POP ES`               | `7,pm=21`                                                             |
| `POP`                       | `17`          | `POP SS`               | `7,pm=21`                                                             |
| `POP`                       | `0F A1`       | `POP FS`               | `7,pm=21`                                                             |
| `POP`                       | `0F A9`       | `POP GS`               | `7,pm=21`                                                             |
| `POPA/POPAD`                | `61`          | `POPA`                 | `24`                                                                  |
| `POPA/POPAD`                | `61`          | `POPAD`                | `24`                                                                  |
| `POPF/POPFD`                | `9D`          | `POPF`                 | `5`                                                                   |
| `POPF/POPFD`                | `9D`          | `POPFD`                | `5`                                                                   |
| `PUSH`                      | `FF /6`       | `PUSH m16`             | `5`                                                                   |
| `PUSH`                      | `FF /6`       | `PUSH m32`             | `5`                                                                   |
| `PUSH`                      | `50 + /r`     | `PUSH r16`             | `2`                                                                   |
| `PUSH`                      | `50 + /r`     | `PUSH r32`             | `2`                                                                   |
| `PUSH`                      | `6A`          | `PUSH imm8`            | `2`                                                                   |
| `PUSH`                      | `68`          | `PUSH imm16`           | `2`                                                                   |
| `PUSH`                      | `68`          | `PUSH imm32`           | `2`                                                                   |
| `PUSH`                      | `0E`          | `PUSH CS`              | `2`                                                                   |
| `PUSH`                      | `16`          | `PUSH SS`              | `2`                                                                   |
| `PUSH`                      | `1E`          | `PUSH DS`              | `2`                                                                   |
| `PUSH`                      | `06`          | `PUSH ES`              | `2`                                                                   |
| `PUSH`                      | `0F A0`       | `PUSH FS`              | `2`                                                                   |
| `PUSH`                      | `OF A8`       | `PUSH GS`              | `2`                                                                   |
| `PUSHA/PUSHAD`              | `60`          | `PUSHA`                | `18`                                                                  |
| `PUSHA/PUSHAD`              | `60`          | `PUSHAD`               | `18`                                                                  |
| `PUSHF/PUSHFD`              | `9C`          | `PUSHF`                | `4`                                                                   |
| `PUSHF/PUSHFD`              | `9C`          | `PUSHFD`               | `4`                                                                   |
| `RCL/RCR/ROL/ROR`           | `D0 /2`       | `RCL r/m8,1`           | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D2 /2`       | `RCL r/m8,CL`          | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `C0 /2 ib`    | `RCL r/m8,imm8`        | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D1 /2`       | `RCL r/m16,1`          | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D3 /2`       | `RCL r/m16,CL`         | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `C1 /2 ib`    | `RCL r/m16,imm8`       | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D1 /2`       | `RCL r/m32,1`          | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D3 /2`       | `RCL r/m32,CL`         | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `C1 /2 ib`    | `RCL r/m32,imm8`       | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D0 /3`       | `RCR r/m8,1`           | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D2 /3`       | `RCR r/m8,CL`          | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `C0 /3 ib`    | `RCR r/m8,imm8`        | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D1 /3`       | `RCR r/m16,1`          | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D3 /3`       | `RCR r/m16,CL`         | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `C1 /3 ib`    | `RCR r/m16,imm8`       | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D1 /3`       | `RCR r/m32,1`          | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D3 /3`       | `RCR r/m32,CL`         | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `C1 /3 ib`    | `RCR r/m32,imm8`       | `9/10`                                                                |
| `RCL/RCR/ROL/ROR`           | `D0 /0`       | `ROL r/m8,1`           | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D2 /0`       | `ROL r/m8,CL`          | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `C0 /0 ib`    | `ROL r/m8,imm8`        | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D1 /0`       | `ROL r/m16,1`          | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D3 /0`       | `ROL r/m16,CL`         | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `C1 /0 ib`    | `ROL r/m16,imm8`       | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D1 /0`       | `ROL r/m32,1`          | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D3 /0`       | `ROL r/m32,CL`         | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `C1 /0 ib`    | `ROL r/m32,imm8`       | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D0 /1`       | `ROR r/m8,1`           | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D2 /1`       | `ROR r/m8,CL`          | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `C0 /1 ib`    | `ROR r/m8,imm8`        | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D1 /1`       | `ROR r/m16,1`          | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D3 /1`       | `ROR r/m16,CL`         | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `C1 /1 ib`    | `ROR r/m16,imm8`       | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D1 /1`       | `ROR r/m32,1`          | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `D3 /1`       | `ROR r/m32,CL`         | `3/7`                                                                 |
| `RCL/RCR/ROL/ROR`           | `C1 /1 ib`    | `ROR r/m32,imm8`       | `3/7`                                                                 |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6C`       | `REP INS r/m8, DX`     | `13+6*(E)CX, pm=7+6*(E)CX (CPL<=IOPL) / 27+6*(E)CX (CPL>IOPL or V86)` |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6D`       | `REP INS r/m16,DX`     | `13+6*(E)CX, pm=7+6*(E)CX (CPL<=IOPL) / 27+6*(E)CX (CPL>IOPL or V86)` |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6D`       | `REP INS r/m32,DX`     | `13+6*(E)CX, pm=7+6*(E)CX (CPL<=IOPL) / 27+6*(E)CX (CPL>IOPL or V86)` |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A4`       | `REP MOVS m8,m8`       | `5+4*(E)CX`                                                           |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A5`       | `REP MOVS m16,m16`     | `5+4*(E)CX`                                                           |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A5`       | `REP MOVS m32,m32`     | `5+4*(E)CX`                                                           |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6E`       | `REP OUTS DX,r/m8`     | `5+12*(E)CX, pm=6+5*(E)CX (CPL<=IOPL) / 26+5*(E)CX (CPL>IOPL or V86)` |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6F`       | `REP OUTS DX,r/m16`    | `5+12*(E)CX, pm=6+5*(E)CX (CPL<=IOPL) / 26+5*(E)CX (CPL>IOPL or V86)` |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6F`       | `REP OUTS DX,r/m32`    | `5+12*(E)CX, pm=6+5*(E)CX (CPL<=IOPL) / 26+5*(E)CX (CPL>IOPL or V86)` |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AA`       | `REP STOS m8`          | `5+5*(E)CX`                                                           |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AB`       | `REP STOS m16`         | `5+5*(E)CX`                                                           |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AB`       | `REP STOS m32`         | `5+5*(E)CX`                                                           |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A6`       | `REPE CMPS m8,m8`      | `5+9*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A7`       | `REPE CMPS m16,m16`    | `5+9*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A7`       | `REPE CMPS m32,m32`    | `5+9*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AE`       | `REPE SCAS m8`         | `5+8*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AF`       | `REPE SCAS m16`        | `5+8*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AF`       | `REPE SCAS m32`        | `5+8*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 A6`       | `REPNE CMPS m8,m8`     | `5+9*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 A7`       | `REPNE CMPS m16,m16`   | `5+9*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 A7`       | `REPNE CMPS m32,m32`   | `5+9*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 AE`       | `REPNE SCAS m8`        | `5+8*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 AF`       | `REPNE SCAS m16`       | `5+8*N`                                                               |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 AF`       | `REPNE SCAS m32`       | `5+8*N`                                                               |
| `SAHF`                      | `9E`          | `SAHF`                 | `3`                                                                   |
| `SAL/SAR/SHL/SHR`           | `D0 /4`       | `SAL r/m8,1`           | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D2 /4`       | `SAL r/m8,CL`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C0 /4 ib`    | `SAL r/m8,imm8`        | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SAL r/m16,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SAL r/m16,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SAL r/m16,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SAL r/m32,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SAL r/m32,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SAL r/m32,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D0 /7`       | `SAR r/m8,1`           | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D2 /7`       | `SAR r/m8,CL`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C0 /7 ib`    | `SAR r/m8,imm8`        | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /7`       | `SAR r/m16,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /7`       | `SAR r/m16,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /7 ib`    | `SAR r/m16,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /7`       | `SAR r/m32,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /7`       | `SAR r/m32,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /7 ib`    | `SAR r/m32,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D0 /4`       | `SHL r/m8,1`           | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D2 /4`       | `SHL r/m8,CL`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C0 /4 ib`    | `SHL r/m8,imm8`        | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SHL r/m16,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SHL r/m16,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SHL r/m16,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SHL r/m32,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SHL r/m32,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SHL r/m32,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D0 /5`       | `SHR r/m8,1`           | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D2 /5`       | `SHR r/m8,CL`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C0 /5 ib`    | `SHR r/m8,imm8`        | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /5`       | `SHR r/m16,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /5`       | `SHR r/m16,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /5 ib`    | `SHR r/m16,imm8`       | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D1 /5`       | `SHR r/m32,1`          | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D3 /5`       | `SHR r/m32,CL`         | `3/7`                                                                 |
| `SAL/SAR/SHL/SHR`           | `C1 /5 ib`    | `SHR r/m32,imm8`       | `3/7`                                                                 |
| `SBB`                       | `1C ib`       | `SBB AL,imm8`          | `2`                                                                   |
| `SBB`                       | `1D iw`       | `SBB AX,imm16`         | `2`                                                                   |
| `SBB`                       | `1D id`       | `SBB EAX,imm32`        | `2`                                                                   |
| `SBB`                       | `80 /3 ib`    | `SBB r/m8,imm8`        | `2/7`                                                                 |
| `SBB`                       | `81 /3 iw`    | `SBB r/m16,imm16`      | `2/7`                                                                 |
| `SBB`                       | `81 /3 id`    | `SBB r/m32,imm32`      | `2/7`                                                                 |
| `SBB`                       | `83 /3 ib`    | `SBB r/m16,imm8`       | `2/7`                                                                 |
| `SBB`                       | `83 /3 ib`    | `SBB r/m32,imm8`       | `2/7`                                                                 |
| `SBB`                       | `18 /r`       | `SBB r/m8,r8`          | `2/7`                                                                 |
| `SBB`                       | `19 /r`       | `SBB r/m16,r16`        | `2/7`                                                                 |
| `SBB`                       | `19 /r`       | `SBB r/m32,r32`        | `2/7`                                                                 |
| `SBB`                       | `1A /r`       | `SBB r8,r/m8`          | `2/6`                                                                 |
| `SBB`                       | `1B /r`       | `SBB r16,r/m16`        | `2/6`                                                                 |
| `SBB`                       | `1B /r`       | `SBB r32,r/m32`        | `2/6`                                                                 |
| `SCAS/SCASB/SCASW/SCASD`    | `AE`          | `SCAS m8`              | `7`                                                                   |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCAS m16`             | `7`                                                                   |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCAS m32`             | `7`                                                                   |
| `SCAS/SCASB/SCASW/SCASD`    | `AE`          | `SCASB`                | `7`                                                                   |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCASW`                | `7`                                                                   |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCASD`                | `7`                                                                   |
| `SETcc`                     | `0F 97`       | `SETA r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 93`       | `SETAE r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 92`       | `SETB r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 96`       | `SETBE r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 92`       | `SETC r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 94`       | `SETE r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 9F`       | `SETG r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 9D`       | `SETGE r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 9C`       | `SETL r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 9E`       | `SETLE r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 96`       | `SETNA r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 92`       | `SETNAE r/m8`          | `4/5`                                                                 |
| `SETcc`                     | `0F 93`       | `SETNB r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 97`       | `SETNBE r/m8`          | `4/5`                                                                 |
| `SETcc`                     | `0F 93`       | `SETNC r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 95`       | `SETNE r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 9E`       | `SETNG r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 9C`       | `SETNGE r/m8`          | `4/5`                                                                 |
| `SETcc`                     | `0F 9D`       | `SETNL r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 9F`       | `SETNLE r/m8`          | `4/5`                                                                 |
| `SETcc`                     | `0F 91`       | `SETNO r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 9B`       | `SETNP r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 99`       | `SETNS r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 95`       | `SETNZ r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 90`       | `SETO r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 9A`       | `SETP r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 9A`       | `SETPE r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 9B`       | `SETPO r/m8`           | `4/5`                                                                 |
| `SETcc`                     | `0F 98`       | `SETS r/m8`            | `4/5`                                                                 |
| `SETcc`                     | `0F 94`       | `SETZ r/m8`            | `4/5`                                                                 |
| `SGDT/SIDT`                 | `0F 01 /0`    | `SGDT m`               | `9`                                                                   |
| `SGDT/SIDT`                 | `0F 01 /1`    | `SIDT m`               | `9`                                                                   |
| `SHLD`                      | `0F A4`       | `SHLD r/m16,r16,imm8`  | `3/7`                                                                 |
| `SHLD`                      | `0F A4`       | `SHLD r/m32,r32,imm8`  | `3/7`                                                                 |
| `SHLD`                      | `0F A5`       | `SHLD r/m16,r16,CL`    | `3/7`                                                                 |
| `SHLD`                      | `0F A5`       | `SHLD r/m32,r32,CL`    | `3/7`                                                                 |
| `SHRD`                      | `0F AC`       | `SHRD r/m16,r16,imm8`  | `3/7`                                                                 |
| `SHRD`                      | `0F AC`       | `SHRD r/m32,r32,imm8`  | `3/7`                                                                 |
| `SHRD`                      | `0F AD`       | `SHRD r/m16,r16,CL`    | `3/7`                                                                 |
| `SHRD`                      | `0F AD`       | `SHRD r/m32,r32,CL`    | `3/7`                                                                 |
| `SLDT`                      | `0F 00 /0`    | `SLDT r/m16`           | `pm=2/2`                                                              |
| `SMSW`                      | `0F 01 /4`    | `SMSW r/m16`           | `2/3,pm=2/2`                                                          |
| `STC`                       | `F9`          | `STC`                  | `2`                                                                   |
| `STD`                       | `FD`          | `STD`                  | `2`                                                                   |
| `STI`                       | `FB`          | `STI`                  | `3`                                                                   |
| `STOS/STOSB/STOSW/STOSD`    | `AA`          | `STOS m8`              | `4`                                                                   |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOS m16`             | `4`                                                                   |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOS m32`             | `4`                                                                   |
| `STOS/STOSB/STOSW/STOSD`    | `AA`          | `STOSB`                | `4`                                                                   |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOSW`                | `4`                                                                   |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOSD`                | `4`                                                                   |
| `STR`                       | `0F 00 /1`    | `STR r/m16`            | `pm=23/27`                                                            |
| `SUB`                       | `2C ib`       | `SUB AL,imm8`          | `2`                                                                   |
| `SUB`                       | `2D iw`       | `SUB AX,imm16`         | `2`                                                                   |
| `SUB`                       | `2D id`       | `SUB EAX,imm32`        | `2`                                                                   |
| `SUB`                       | `80 /5 ib`    | `SUB r/m8,imm8`        | `2/7`                                                                 |
| `SUB`                       | `81 /5 iw`    | `SUB r/m16,imm16`      | `2/7`                                                                 |
| `SUB`                       | `81 /5 id`    | `SUB r/m32,imm32`      | `2/7`                                                                 |
| `SUB`                       | `83 /5 ib`    | `SUB r/m16,imm8`       | `2/7`                                                                 |
| `SUB`                       | `83 /5 ib`    | `SUB r/m32,imm8`       | `2/7`                                                                 |
| `SUB`                       | `28 /r`       | `SUB r/m8,r8`          | `2/7`                                                                 |
| `SUB`                       | `29 /r`       | `SUB r/m16,r16`        | `2/7`                                                                 |
| `SUB`                       | `29 /r`       | `SUB r/m32,r32`        | `2/7`                                                                 |
| `SUB`                       | `2A /r`       | `SUB r8,r/m8`          | `2/6`                                                                 |
| `SUB`                       | `2B /r`       | `SUB r16,r/m16`        | `2/6`                                                                 |
| `SUB`                       | `2B /r`       | `SUB r32,r/m32`        | `2/6`                                                                 |
| `TEST`                      | `A8 ib`       | `TEST AL,imm8`         | `2`                                                                   |
| `TEST`                      | `A9 iw`       | `TEST AX,imm16`        | `2`                                                                   |
| `TEST`                      | `A9 id`       | `TEST EAX,imm32`       | `2`                                                                   |
| `TEST`                      | `F6 /0 ib`    | `TEST r/m8,imm8`       | `2/5`                                                                 |
| `TEST`                      | `F7 /0 iw`    | `TEST r/m16,imm16`     | `2/5`                                                                 |
| `TEST`                      | `F7 /0 id`    | `TEST r/m32,imm32`     | `2/5`                                                                 |
| `TEST`                      | `84 /r`       | `TEST r/m8,r8`         | `2/5`                                                                 |
| `TEST`                      | `85 /r`       | `TEST r/m16,r16`       | `2/5`                                                                 |
| `TEST`                      | `85 /r`       | `TEST r/m32,r32`       | `2/5`                                                                 |
| `VERR`                      | `0F 00 /4`    | `VERR r/m16`           | `pm=10/11`                                                            |
| `VERR`                      | `0F 00 /5`    | `VERW r/m16`           | `pm=15/16`                                                            |
| `WAIT`                      | `9B`          | `WAIT`                 | `6 min.`                                                              |
| `XCHG`                      | `90 + r`      | `XCHG AX,r16`          | `3`                                                                   |
| `XCHG`                      | `90 + r`      | `XCHG r16,AX`          | `3`                                                                   |
| `XCHG`                      | `90 + r`      | `XCHG EAX,r32`         | `3`                                                                   |
| `XCHG`                      | `90 + r`      | `XCHG r32,EAX`         | `3`                                                                   |
| `XCHG`                      | `86 /r`       | `XCHG r/m8,r8`         | `3`                                                                   |
| `XCHG`                      | `86 /r`       | `XCHG r8,r/m8`         | `3/5`                                                                 |
| `XCHG`                      | `87 /r`       | `XCHG r/m16,r16`       | `3`                                                                   |
| `XCHG`                      | `87 /r`       | `XCHG r16,r/m16`       | `3/5`                                                                 |
| `XCHG`                      | `87 /r`       | `XCHG r/m32,r32`       | `3`                                                                   |
| `XCHG`                      | `87 /r`       | `XCHG r32,r/m32`       | `3/5`                                                                 |
| `XOR`                       | `34 ib`       | `XOR AL,imm8`          | `2`                                                                   |
| `XOR`                       | `35 iw`       | `XOR AX,imm16`         | `2`                                                                   |
| `XOR`                       | `35 id`       | `XOR EAX,imm32`        | `2`                                                                   |
| `XOR`                       | `80 /6 ib`    | `XOR r/m8,imm8`        | `2/7`                                                                 |
| `XOR`                       | `81 /6 iw`    | `XOR r/m16,imm16`      | `2/7`                                                                 |
| `XOR`                       | `81 /6 id`    | `XOR r/m32,imm32`      | `2/7`                                                                 |
| `XOR`                       | `83 /6 ib`    | `XOR r/m16,imm8`       | `2/7`                                                                 |
| `XOR`                       | `83 /6 ib`    | `XOR r/m32,imm8`       | `2/7`                                                                 |
| `XOR`                       | `30 /r`       | `XOR r/m8,r8`          | `2/7`                                                                 |
| `XOR`                       | `31 /r`       | `XOR r/m16,r16`        | `2/7`                                                                 |
| `XOR`                       | `31 /r`       | `XOR r/m32,r32`        | `2/7`                                                                 |
| `XOR`                       | `32 /r`       | `XOR r8,r/m8`          | `2/6`                                                                 |
| `XOR`                       | `33 /r`       | `XOR r16,r/m16`        | `2/6`                                                                 |
| `XOR`                       | `33 /r`       | `XOR r32,r/m32`        | `2/6`                                                                 |
| `XLAT`                      | `D7`          | `XLAT src-table`       | `5`                                                                   |
| `RET`                       | `C3`          | `RET`                  | `10+m`                                                                |
| `RET`                       | `CB`          | `RET`                  | `18+m,pm=32+m`                                                        |
| `RET`                       | `CB`          | `RET`                  | `pm=68`                                                               |
| `RET`                       | `C2 iw`       | `RET imm16`            | `10+m`                                                                |
| `RET`                       | `CA iw`       | `RET imm16`            | `18+m,pm=32+m`                                                        |
| `RET`                       | `CA iw`       | `RET imm16`            | `pm=68`                                                               |
