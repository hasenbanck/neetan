# Intel 80486 Instruction Execution Clock Cycles

**Primary source:** Intel, *i486 Processor Programmer's Reference Manual* (1990), Appendix E — "Instruction Format and Timing", Tables 10.1 (integer), 10.2 (I/O).

## Scope

- This document is an extracted timing reference for 80486 instruction forms listed in the manual's instruction set detail section.
- Clock values are copied from the manual's `Cache Hit` column and preserve its notation.
- A separate `Penalty if Cache Miss` column exists in the Intel manual; miss penalties are noted where significant.
- The 486SX lacks an on-chip FPU — x87 instructions generate #NM (INT 7). FPU timings are not included.
- CPUID (`0F A2`) is not available on early 486SX steppings and is omitted.

## Timing Model

The 80486 is a 5-stage pipelined processor (Prefetch, Decode 1, Decode 2, Execute, Write Back)
with an 8KB unified on-chip cache. The clock counts in the manual assume:

1. Data and instruction accesses hit in the cache.
2. No exceptions during execution.
3. Accesses are aligned.
4. The external bus is available for reads or writes at all times.
5. No wait states.

Unlike the 80386, the 486 does **not** use the `m` variable (next-instruction component count)
for control-transfer timing — control transfers have fixed cycle counts.

### Effective Address Calculation

- An effective address using a base register (not the destination of the preceding instruction)
  and no index register adds **0** extra clocks.
- If the effective address uses an index register, **1** clock **may** be added.
- If the base register is the destination register of the preceding instruction, add **1** clock
  (back-to-back PUSH and POP are not affected by this rule).

### Prefixes

Each prefix byte costs **1** clock (address size, operand size, LOCK, segment override).

### Misalignment

Add **+3** clocks for each misaligned memory access (386 was +4).

### Cache Miss Penalty

A cache miss forces an external bus cycle. The 486 uses a 32-bit burst bus defined as
`r − b − w` where `r` = first-cycle clocks, `b` = subsequent burst clocks, `w` = write clocks.
The fastest supported bus is `2−1−2` (0 wait states). The cache miss penalty column in the
tables below assumes this `2−1−2` timing.

### Task Switch Times

When an exception causes a task switch, the instruction execution time is increased by the
task switch overhead:

| Method                                   | Cache Hit | Miss Penalty |
|------------------------------------------|----------:|-------------:|
| VM/486 CPU/286 TSS to 486 CPU TSS       |     `162` |         `55` |
| VM/486 CPU/286 TSS to 286 TSS           |     `143` |         `31` |
| VM/486 CPU/286 TSS to VM TSS            |     `140` |         `37` |

### Interrupt Clock Counts

| Method                                   | Cache Hit  | Miss Penalty |
|------------------------------------------|------------|-------------:|
| Real Mode                                | `26`       |          `2` |
| Protected Mode: same level               | `44`       |          `6` |
| Protected Mode: different level          | `71`       |         `17` |
| Protected Mode: Task Gate                | `37 + TS`  |          `3` |
| Virtual Mode: different level            | `82`       |         `17` |
| Virtual Mode: Task Gate                  | `37 + TS`  |          `3` |

## Notation

- `reg/mem` format (e.g. `1/3`) means register-form clocks first, memory-form clocks second.
- `pm=` denotes protected-mode-specific clocks.
- `ts` denotes task-switch-dependent timing.
- `n`, `N`, `c` are manual variables used in per-instruction formulas.
- `T/NT` = taken/not taken. `L/NL` = loop/no loop. `H/NH` = hit/no hit.
- `MN/MX` = minimum/maximum. `U/L` = unlocked/locked.
- `16/32` = 16-bit/32-bit modes. `RV/P` = real+virtual mode / protected mode.
- `R` = real mode. `P` = protected mode.
- Cache miss penalty values are from the manual's separate column; `TS` refers to the Task Switch table above.

## Extracted Clock Table

| Mnemonic                    | Opcode        | Instruction            | Clocks                                                              |
|-----------------------------|---------------|------------------------|---------------------------------------------------------------------|
| `AAA`                       | `37`          | `AAA`                  | `3`                                                                 |
| `AAD`                       | `D5 0A`       | `AAD`                  | `14`                                                                |
| `AAM`                       | `D4 0A`       | `AAM`                  | `15`                                                                |
| `AAS`                       | `3F`          | `AAS`                  | `3`                                                                 |
| `ADC`                       | `14 ib`       | `ADC AL,imm8`          | `1`                                                                 |
| `ADC`                       | `15 iw`       | `ADC AX,imm16`         | `1`                                                                 |
| `ADC`                       | `15 id`       | `ADC EAX,imm32`        | `1`                                                                 |
| `ADC`                       | `80 /2 ib`    | `ADC r/m8,imm8`        | `1/3`                                                               |
| `ADC`                       | `81 /2 iw`    | `ADC r/m16,imm16`      | `1/3`                                                               |
| `ADC`                       | `81 /2 id`    | `ADC r/m32,imm32`      | `1/3`                                                               |
| `ADC`                       | `83 /2 ib`    | `ADC r/m16,imm8`       | `1/3`                                                               |
| `ADC`                       | `83 /2 ib`    | `ADC r/m32,imm8`       | `1/3`                                                               |
| `ADC`                       | `10 /r`       | `ADC r/m8,r8`          | `1/3`                                                               |
| `ADC`                       | `11 /r`       | `ADC r/m16,r16`        | `1/3`                                                               |
| `ADC`                       | `11 /r`       | `ADC r/m32,r32`        | `1/3`                                                               |
| `ADC`                       | `12 /r`       | `ADC r8,r/m8`          | `1/2`                                                               |
| `ADC`                       | `13 /r`       | `ADC r16,r/m16`        | `1/2`                                                               |
| `ADC`                       | `13 /r`       | `ADC r32,r/m32`        | `1/2`                                                               |
| `ADD`                       | `04 ib`       | `ADD AL,imm8`          | `1`                                                                 |
| `ADD`                       | `05 iw`       | `ADD AX,imm16`         | `1`                                                                 |
| `ADD`                       | `05 id`       | `ADD EAX,imm32`        | `1`                                                                 |
| `ADD`                       | `80 /0 ib`    | `ADD r/m8,imm8`        | `1/3`                                                               |
| `ADD`                       | `81 /0 iw`    | `ADD r/m16,imm16`      | `1/3`                                                               |
| `ADD`                       | `81 /0 id`    | `ADD r/m32,imm32`      | `1/3`                                                               |
| `ADD`                       | `83 /0 ib`    | `ADD r/m16,imm8`       | `1/3`                                                               |
| `ADD`                       | `83 /0 ib`    | `ADD r/m32,imm8`       | `1/3`                                                               |
| `ADD`                       | `00 /r`       | `ADD r/m8,r8`          | `1/3`                                                               |
| `ADD`                       | `01 /r`       | `ADD r/m16,r16`        | `1/3`                                                               |
| `ADD`                       | `01 /r`       | `ADD r/m32,r32`        | `1/3`                                                               |
| `ADD`                       | `02 /r`       | `ADD r8,r/m8`          | `1/2`                                                               |
| `ADD`                       | `03 /r`       | `ADD r16,r/m16`        | `1/2`                                                               |
| `ADD`                       | `03 /r`       | `ADD r32,r/m32`        | `1/2`                                                               |
| `AND`                       | `24 ib`       | `AND AL,imm8`          | `1`                                                                 |
| `AND`                       | `25 iw`       | `AND AX,imm16`         | `1`                                                                 |
| `AND`                       | `25 id`       | `AND EAX,imm32`        | `1`                                                                 |
| `AND`                       | `80 /4 ib`    | `AND r/m8,imm8`        | `1/3`                                                               |
| `AND`                       | `81 /4 iw`    | `AND r/m16,imm16`      | `1/3`                                                               |
| `AND`                       | `81 /4 id`    | `AND r/m32,imm32`      | `1/3`                                                               |
| `AND`                       | `83 /4 ib`    | `AND r/m16,imm8`       | `1/3`                                                               |
| `AND`                       | `83 /4 ib`    | `AND r/m32,imm8`       | `1/3`                                                               |
| `AND`                       | `20 /r`       | `AND r/m8,r8`          | `1/3`                                                               |
| `AND`                       | `21 /r`       | `AND r/m16,r16`        | `1/3`                                                               |
| `AND`                       | `21 /r`       | `AND r/m32,r32`        | `1/3`                                                               |
| `AND`                       | `22 /r`       | `AND r8,r/m8`          | `1/2`                                                               |
| `AND`                       | `23 /r`       | `AND r16,r/m16`        | `1/2`                                                               |
| `AND`                       | `23 /r`       | `AND r32,r/m32`        | `1/2`                                                               |
| `ARPL`                      | `63 /r`       | `ARPL r/m16,r16`       | `pm=9`                                                              |
| `BOUND`                     | `62 /r`       | `BOUND r16,m16&16`     | `7`                                                                 |
| `BOUND`                     | `62 /r`       | `BOUND r32,m32&32`     | `7`                                                                 |
| `BSF`                       | `0F BC`       | `BSF r16,r/m16`        | `6-42/7-43`                                                         |
| `BSF`                       | `0F BC`       | `BSF r32,r/m32`        | `6-42/7-43`                                                         |
| `BSR`                       | `0F BD`       | `BSR r16,r/m16`        | `6-103/7-104`                                                       |
| `BSR`                       | `0F BD`       | `BSR r32,r/m32`        | `6-103/7-104`                                                       |
| `BT`                        | `0F A3`       | `BT r/m16,r16`         | `3/8`                                                               |
| `BT`                        | `0F A3`       | `BT r/m32,r32`         | `3/8`                                                               |
| `BT`                        | `0F BA /4 ib` | `BT r/m16,imm8`        | `3/3`                                                               |
| `BT`                        | `0F BA /4 ib` | `BT r/m32,imm8`        | `3/3`                                                               |
| `BTC`                       | `0F BB`       | `BTC r/m16,r16`        | `6/13`                                                              |
| `BTC`                       | `0F BB`       | `BTC r/m32,r32`        | `6/13`                                                              |
| `BTC`                       | `0F BA /7 ib` | `BTC r/m16,imm8`       | `6/8`                                                               |
| `BTC`                       | `0F BA /7 ib` | `BTC r/m32,imm8`       | `6/8`                                                               |
| `BTR`                       | `0F B3`       | `BTR r/m16,r16`        | `6/13`                                                              |
| `BTR`                       | `0F B3`       | `BTR r/m32,r32`        | `6/13`                                                              |
| `BTR`                       | `0F BA /6 ib` | `BTR r/m16,imm8`       | `6/8`                                                               |
| `BTR`                       | `0F BA /6 ib` | `BTR r/m32,imm8`       | `6/8`                                                               |
| `BTS`                       | `0F AB`       | `BTS r/m16,r16`        | `6/13`                                                              |
| `BTS`                       | `0F AB`       | `BTS r/m32,r32`        | `6/13`                                                              |
| `BTS`                       | `0F BA /5 ib` | `BTS r/m16,imm8`       | `6/8`                                                               |
| `BTS`                       | `0F BA /5 ib` | `BTS r/m32,imm8`       | `6/8`                                                               |
| `BSWAP`                     | `0F C8+rd`    | `BSWAP r32`            | `1`                                                                 |
| `CALL`                      | `E8 cw`       | `CALL rel16`           | `3`                                                                 |
| `CALL`                      | `E8 cd`       | `CALL rel32`           | `3`                                                                 |
| `CALL`                      | `FF /2`       | `CALL r/m16`           | `5/5`                                                               |
| `CALL`                      | `FF /2`       | `CALL r/m32`           | `5/5`                                                               |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `18,pm=20`                                                          |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=35 (gate, same level)`                                          |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=69 (gate, inner, no params)`                                    |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=77+4x (gate, inner, x params)`                                  |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=37+TS (task gate)`                                              |
| `CALL`                      | `9A cd`       | `CALL ptr16:16`        | `pm=38+TS (task gate)`                                              |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `17,pm=20`                                                          |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=35 (gate, same level)`                                          |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=69 (gate, inner, no params)`                                    |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=77+4x (gate, inner, x params)`                                  |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=37+TS (task gate)`                                              |
| `CALL`                      | `FF /3`       | `CALL m16:16`          | `pm=38+TS (task gate)`                                              |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `18,pm=20`                                                          |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=35 (gate, same level)`                                          |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=69 (gate, inner, no params)`                                    |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=77+4x (gate, inner, x params)`                                  |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=37+TS (task gate)`                                              |
| `CALL`                      | `9A cp`       | `CALL ptr16:32`        | `pm=38+TS (task gate)`                                              |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `17,pm=20`                                                          |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=35 (gate, same level)`                                          |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=69 (gate, inner, no params)`                                    |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=77+4x (gate, inner, x params)`                                  |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=37+TS (task gate)`                                              |
| `CALL`                      | `FF /3`       | `CALL m16:32`          | `pm=38+TS (task gate)`                                              |
| `CBW/CWDE`                  | `98`          | `CBW`                  | `3`                                                                 |
| `CBW/CWDE`                  | `98`          | `CWDE`                 | `3`                                                                 |
| `CLC`                       | `F8`          | `CLC`                  | `2`                                                                 |
| `CLD`                       | `FC`          | `CLD`                  | `2`                                                                 |
| `CLI`                       | `FA`          | `CLI`                  | `5`                                                                 |
| `CLTS`                      | `0F 06`       | `CLTS`                 | `7`                                                                 |
| `CMC`                       | `F5`          | `CMC`                  | `2`                                                                 |
| `CMP`                       | `3C ib`       | `CMP AL,imm8`          | `1`                                                                 |
| `CMP`                       | `3D iw`       | `CMP AX,imm16`         | `1`                                                                 |
| `CMP`                       | `3D id`       | `CMP EAX,imm32`        | `1`                                                                 |
| `CMP`                       | `80 /7 ib`    | `CMP r/m8,imm8`        | `1/2`                                                               |
| `CMP`                       | `81 /7 iw`    | `CMP r/m16,imm16`      | `1/2`                                                               |
| `CMP`                       | `81 /7 id`    | `CMP r/m32,imm32`      | `1/2`                                                               |
| `CMP`                       | `83 /7 ib`    | `CMP r/m16,imm8`       | `1/2`                                                               |
| `CMP`                       | `83 /7 ib`    | `CMP r/m32,imm8`       | `1/2`                                                               |
| `CMP`                       | `38 /r`       | `CMP r/m8,r8`          | `1/2`                                                               |
| `CMP`                       | `39 /r`       | `CMP r/m16,r16`        | `1/2`                                                               |
| `CMP`                       | `39 /r`       | `CMP r/m32,r32`        | `1/2`                                                               |
| `CMP`                       | `3A /r`       | `CMP r8,r/m8`          | `1/2`                                                               |
| `CMP`                       | `3B /r`       | `CMP r16,r/m16`        | `1/2`                                                               |
| `CMP`                       | `3B /r`       | `CMP r32,r/m32`        | `1/2`                                                               |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A6`          | `CMPS m8,m8`           | `8`                                                                 |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPS m16,m16`         | `8`                                                                 |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPS m32,m32`         | `8`                                                                 |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A6`          | `CMPSB`                | `8`                                                                 |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPSW`                | `8`                                                                 |
| `CMPS/CMPSB/CMPSW/CMPSD`    | `A7`          | `CMPSD`                | `8`                                                                 |
| `CMPXCHG`                   | `0F B0 /r`    | `CMPXCHG r/m8,r8`      | `6/7-10`                                                            |
| `CMPXCHG`                   | `0F B1 /r`    | `CMPXCHG r/m16,r16`    | `6/7-10`                                                            |
| `CMPXCHG`                   | `0F B1 /r`    | `CMPXCHG r/m32,r32`    | `6/7-10`                                                            |
| `CWD/CDQ`                   | `99`          | `CWD`                  | `3`                                                                 |
| `CWD/CDQ`                   | `99`          | `CDQ`                  | `3`                                                                 |
| `DAA`                       | `27`          | `DAA`                  | `2`                                                                 |
| `DAS`                       | `2F`          | `DAS`                  | `2`                                                                 |
| `DEC`                       | `FE /1`       | `DEC r/m8`             | `1/3`                                                               |
| `DEC`                       | `FF /1`       | `DEC r/m16`            | `1/3`                                                               |
| `DEC`                       | `FF /1`       | `DEC r/m32`            | `1/3`                                                               |
| `DEC`                       | `48+rw`       | `DEC r16`              | `1`                                                                 |
| `DEC`                       | `48+rd`       | `DEC r32`              | `1`                                                                 |
| `DIV`                       | `F6 /6`       | `DIV AL,r/m8`          | `16`                                                                |
| `DIV`                       | `F7 /6`       | `DIV AX,r/m16`         | `24`                                                                |
| `DIV`                       | `F7 /6`       | `DIV EAX,r/m32`        | `40`                                                                |
| `ENTER`                     | `C8 iw 00`    | `ENTER imm16,0`        | `14`                                                                |
| `ENTER`                     | `C8 iw 01`    | `ENTER imm16,1`        | `17`                                                                |
| `ENTER`                     | `C8 iw ib`    | `ENTER imm16,imm8`     | `17+3L`                                                             |
| `HLT`                       | `F4`          | `HLT`                  | `4`                                                                 |
| `IDIV`                      | `F6 /7`       | `IDIV r/m8`            | `19/20`                                                             |
| `IDIV`                      | `F7 /7`       | `IDIV AX,r/m16`        | `27/28`                                                             |
| `IDIV`                      | `F7 /7`       | `IDIV EAX,r/m32`       | `43/44`                                                             |
| `IMUL`                      | `F6 /5`       | `IMUL r/m8`            | `13-18/13-18`                                                       |
| `IMUL`                      | `F7 /5`       | `IMUL r/m16`           | `13-26/13-26`                                                       |
| `IMUL`                      | `F7 /5`       | `IMUL r/m32`           | `13-42/13-42`                                                       |
| `IMUL`                      | `0F AF /r`    | `IMUL r16,r/m16`       | `13-26/13-26`                                                       |
| `IMUL`                      | `0F AF /r`    | `IMUL r32,r/m32`       | `13-42/13-42`                                                       |
| `IMUL`                      | `6B /r ib`    | `IMUL r16,r/m16,imm8`  | `13-18/13-18`                                                       |
| `IMUL`                      | `6B /r ib`    | `IMUL r32,r/m32,imm8`  | `13-18/13-18`                                                       |
| `IMUL`                      | `6B /r ib`    | `IMUL r16,imm8`        | `13-18/13-18`                                                       |
| `IMUL`                      | `6B /r ib`    | `IMUL r32,imm8`        | `13-18/13-18`                                                       |
| `IMUL`                      | `69 /r iw`    | `IMUL r16,r/m16,imm16` | `13-26/13-26`                                                       |
| `IMUL`                      | `69 /r id`    | `IMUL r32,r/m32,imm32` | `13-42/13-42`                                                       |
| `IMUL`                      | `69 /r iw`    | `IMUL r16,imm16`       | `13-26/13-26`                                                       |
| `IMUL`                      | `69 /r id`    | `IMUL r32,imm32`       | `13-42/13-42`                                                       |
| `IN`                        | `E4 ib`       | `IN AL,imm8`           | `14,pm=9/29,v86=27`                                                 |
| `IN`                        | `E5 ib`       | `IN AX,imm8`           | `14,pm=9/29,v86=27`                                                 |
| `IN`                        | `E5 ib`       | `IN EAX,imm8`          | `14,pm=9/29,v86=27`                                                 |
| `IN`                        | `EC`          | `IN AL,DX`             | `14,pm=8/28,v86=27`                                                 |
| `IN`                        | `ED`          | `IN AX,DX`             | `14,pm=8/28,v86=27`                                                 |
| `IN`                        | `ED`          | `IN EAX,DX`            | `14,pm=8/28,v86=27`                                                 |
| `INC`                       | `FE /0`       | `INC r/m8`             | `1/3`                                                               |
| `INC`                       | `FF /0`       | `INC r/m16`            | `1/3`                                                               |
| `INC`                       | `FF /0`       | `INC r/m32`            | `1/3`                                                               |
| `INC`                       | `40+rw`       | `INC r16`              | `1`                                                                 |
| `INC`                       | `40+rd`       | `INC r32`              | `1`                                                                 |
| `INS/INSB/INSW/INSD`        | `6C`          | `INS r/m8,DX`          | `17,pm=10/32,v86=30`                                                |
| `INS/INSB/INSW/INSD`        | `6D`          | `INS r/m16,DX`         | `17,pm=10/32,v86=30`                                                |
| `INS/INSB/INSW/INSD`        | `6D`          | `INS r/m32,DX`         | `17,pm=10/32,v86=30`                                                |
| `INS/INSB/INSW/INSD`        | `6C`          | `INSB`                 | `17,pm=10/32,v86=30`                                                |
| `INS/INSB/INSW/INSD`        | `6D`          | `INSW`                 | `17,pm=10/32,v86=30`                                                |
| `INS/INSB/INSW/INSD`        | `6D`          | `INSD`                 | `17,pm=10/32,v86=30`                                                |
| `INT/INTO`                  | `CC`          | `INT 3`                | `INT+0 (see Interrupt table)`                                       |
| `INT/INTO`                  | `CD ib`       | `INT imm8`             | `INT+4/0 (rv/p, see Interrupt table)`                               |
| `INT/INTO`                  | `CE`          | `INTO`                 | `Fail:3; Pass:INT+2`                                                |
| `INVD`                      | `0F 08`       | `INVD`                 | `4`                                                                 |
| `INVLPG`                    | `0F 01 /7`    | `INVLPG m`             | `12/11 (H/NH)`                                                      |
| `IRET/IRETD`                | `CF`          | `IRET`                 | `15,pm=20/36`                                                       |
| `IRET/IRETD`                | `CF`          | `IRET`                 | `pm=TS+32 (nested task)`                                            |
| `IRET/IRETD`                | `CF`          | `IRETD`                | `15,pm=20/36`                                                       |
| `IRET/IRETD`                | `CF`          | `IRETD`                | `pm=TS+32 (nested task)`                                            |
| `Jcc`                       | `77 cb`       | `JA rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `73 cb`       | `JAE rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `72 cb`       | `JB rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `76 cb`       | `JBE rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `72 cb`       | `JC rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `E3 cb`       | `JCXZ rel8`            | `8/5 (T/NT)`                                                        |
| `Jcc`                       | `E3 cb`       | `JECXZ rel8`           | `8/5 (T/NT)`                                                        |
| `Jcc`                       | `74 cb`       | `JE rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `74 cb`       | `JZ rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7F cb`       | `JG rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7D cb`       | `JGE rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7C cb`       | `JL rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7E cb`       | `JLE rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `76 cb`       | `JNA rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `72 cb`       | `JNAE rel8`            | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `73 cb`       | `JNB rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `77 cb`       | `JNBE rel8`            | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `73 cb`       | `JNC rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `75 cb`       | `JNE rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7E cb`       | `JNG rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7C cb`       | `JNGE rel8`            | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7D cb`       | `JNL rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7F cb`       | `JNLE rel8`            | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `71 cb`       | `JNO rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7B cb`       | `JNP rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `79 cb`       | `JNS rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `75 cb`       | `JNZ rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `70 cb`       | `JO rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7A cb`       | `JP rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7A cb`       | `JPE rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `7B cb`       | `JPO rel8`             | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `78 cb`       | `JS rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `74 cb`       | `JZ rel8`              | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 87 cw/cd` | `JA rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 83 cw/cd` | `JAE rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 82 cw/cd` | `JB rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 86 cw/cd` | `JBE rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 82 cw/cd` | `JC rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 84 cw/cd` | `JE rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 84 cw/cd` | `JZ rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8F cw/cd` | `JG rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8D cw/cd` | `JGE rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8C cw/cd` | `JL rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8E cw/cd` | `JLE rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 86 cw/cd` | `JNA rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 82 cw/cd` | `JNAE rel16/32`        | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 83 cw/cd` | `JNB rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 87 cw/cd` | `JNBE rel16/32`        | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 83 cw/cd` | `JNC rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 85 cw/cd` | `JNE rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8E cw/cd` | `JNG rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8C cw/cd` | `JNGE rel16/32`        | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8D cw/cd` | `JNL rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8F cw/cd` | `JNLE rel16/32`        | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 81 cw/cd` | `JNO rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8B cw/cd` | `JNP rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 89 cw/cd` | `JNS rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 85 cw/cd` | `JNZ rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 80 cw/cd` | `JO rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8A cw/cd` | `JP rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8A cw/cd` | `JPE rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 8B cw/cd` | `JPO rel16/32`         | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 88 cw/cd` | `JS rel16/32`          | `3/1 (T/NT)`                                                        |
| `Jcc`                       | `0F 84 cw/cd` | `JZ rel16/32`          | `3/1 (T/NT)`                                                        |
| `JMP`                       | `EB cb`       | `JMP rel8`             | `3`                                                                 |
| `JMP`                       | `E9 cw`       | `JMP rel16`            | `3`                                                                 |
| `JMP`                       | `E9 cd`       | `JMP rel32`            | `3`                                                                 |
| `JMP`                       | `FF /4`       | `JMP r/m16`            | `5/5`                                                               |
| `JMP`                       | `FF /4`       | `JMP r/m32`            | `5/5`                                                               |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `17,pm=19`                                                          |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `pm=32 (call gate, same level)`                                     |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `pm=42+TS (TSS)`                                                    |
| `JMP`                       | `EA cd`       | `JMP ptr16:16`         | `pm=43+TS (task gate)`                                              |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `13,pm=18`                                                          |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `pm=31 (call gate, same level)`                                     |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `pm=41+TS (TSS)`                                                    |
| `JMP`                       | `FF /5`       | `JMP m16:16`           | `pm=42+TS (task gate)`                                              |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `17,pm=19`                                                          |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `pm=32 (call gate, same level)`                                     |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `pm=42+TS (TSS)`                                                    |
| `JMP`                       | `EA cp`       | `JMP ptr16:32`         | `pm=43+TS (task gate)`                                              |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `13,pm=18`                                                          |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `pm=31 (call gate, same level)`                                     |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `pm=41+TS (TSS)`                                                    |
| `JMP`                       | `FF /5`       | `JMP m16:32`           | `pm=42+TS (task gate)`                                              |
| `LAHF`                      | `9F`          | `LAHF`                 | `3`                                                                 |
| `LAR`                       | `0F 02 /r`    | `LAR r16,r/m16`        | `pm=11/11`                                                          |
| `LAR`                       | `0F 02 /r`    | `LAR r32,r/m32`        | `pm=11/11`                                                          |
| `LEA`                       | `8D /r`       | `LEA r16,m`            | `1` (no index), `2` (with index)                                    |
| `LEA`                       | `8D /r`       | `LEA r32,m`            | `1` (no index), `2` (with index)                                    |
| `LEAVE`                     | `C9`          | `LEAVE`                | `5`                                                                 |
| `LGDT/LIDT`                 | `0F 01 /2`    | `LGDT m16&32`          | `12`                                                                |
| `LGDT/LIDT`                 | `0F 01 /3`    | `LIDT m16&32`          | `12`                                                                |
| `LGS/LSS/LDS/LES/LFS`       | `C5 /r`       | `LDS r16,m16:16`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `C5 /r`       | `LDS r32,m16:32`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `0F B2 /r`    | `LSS r16,m16:16`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `0F B2 /r`    | `LSS r32,m16:32`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `C4 /r`       | `LES r16,m16:16`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `C4 /r`       | `LES r32,m16:32`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `0F B4 /r`    | `LFS r16,m16:16`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `0F B4 /r`    | `LFS r32,m16:32`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `0F B5 /r`    | `LGS r16,m16:16`       | `6/12,pm=6/12`                                                      |
| `LGS/LSS/LDS/LES/LFS`       | `0F B5 /r`    | `LGS r32,m16:32`       | `6/12,pm=6/12`                                                      |
| `LLDT`                      | `0F 00 /2`    | `LLDT r/m16`           | `11/11`                                                             |
| `LMSW`                      | `0F 01 /6`    | `LMSW r/m16`           | `13/13`                                                             |
| `LOCK`                      | `F0`          | `LOCK`                 | `1`                                                                 |
| `LODS/LODSB/LODSW/LODSD`    | `AC`          | `LODS m8`              | `5`                                                                 |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODS m16`             | `5`                                                                 |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODS m32`             | `5`                                                                 |
| `LODS/LODSB/LODSW/LODSD`    | `AC`          | `LODSB`                | `5`                                                                 |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODSW`                | `5`                                                                 |
| `LODS/LODSB/LODSW/LODSD`    | `AD`          | `LODSD`                | `5`                                                                 |
| `LOOP/LOOPcond`             | `E2 cb`       | `LOOP rel8`            | `7/6 (L/NL)`                                                        |
| `LOOP/LOOPcond`             | `E1 cb`       | `LOOPE rel8`           | `9/6 (L/NL)`                                                        |
| `LOOP/LOOPcond`             | `E1 cb`       | `LOOPZ rel8`           | `9/6 (L/NL)`                                                        |
| `LOOP/LOOPcond`             | `E0 cb`       | `LOOPNE rel8`          | `9/6 (L/NL)`                                                        |
| `LOOP/LOOPcond`             | `E0 cb`       | `LOOPNZ rel8`          | `9/6 (L/NL)`                                                        |
| `LSL`                       | `0F 03 /r`    | `LSL r16,r/m16`        | `pm=10/10`                                                          |
| `LSL`                       | `0F 03 /r`    | `LSL r32,r/m32`        | `pm=10/10`                                                          |
| `LTR`                       | `0F 00 /3`    | `LTR r/m16`            | `pm=20/20`                                                          |
| `MOV`                       | `88 /r`       | `MOV r/m8,r8`          | `1/1`                                                               |
| `MOV`                       | `89 /r`       | `MOV r/m16,r16`        | `1/1`                                                               |
| `MOV`                       | `89 /r`       | `MOV r/m32,r32`        | `1/1`                                                               |
| `MOV`                       | `8A /r`       | `MOV r8,r/m8`          | `1/1`                                                               |
| `MOV`                       | `8B /r`       | `MOV r16,r/m16`        | `1/1`                                                               |
| `MOV`                       | `8B /r`       | `MOV r32,r/m32`        | `1/1`                                                               |
| `MOV`                       | `8C /r`       | `MOV r/m16,Sreg`       | `3/3`                                                               |
| `MOV`                       | `8E /r`       | `MOV Sreg,r/m16`       | `3/9,pm=3/9`                                                        |
| `MOV`                       | `A0`          | `MOV AL,moffs8`        | `1`                                                                 |
| `MOV`                       | `A1`          | `MOV AX,moffs16`       | `1`                                                                 |
| `MOV`                       | `A1`          | `MOV EAX,moffs32`      | `1`                                                                 |
| `MOV`                       | `A2`          | `MOV moffs8,AL`        | `1`                                                                 |
| `MOV`                       | `A3`          | `MOV moffs16,AX`       | `1`                                                                 |
| `MOV`                       | `A3`          | `MOV moffs32,EAX`      | `1`                                                                 |
| `MOV`                       | `B0+rb`       | `MOV reg8,imm8`        | `1`                                                                 |
| `MOV`                       | `B8+rw`       | `MOV reg16,imm16`      | `1`                                                                 |
| `MOV`                       | `B8+rd`       | `MOV reg32,imm32`      | `1`                                                                 |
| `MOV`                       | `C6`          | `MOV r/m8,imm8`        | `1/1`                                                               |
| `MOV`                       | `C7`          | `MOV r/m16,imm16`      | `1/1`                                                               |
| `MOV`                       | `C7`          | `MOV r/m32,imm32`      | `1/1`                                                               |
| `MOV`                       | `0F 20 /r`    | `MOV r32,CR0`          | `4`                                                                 |
| `MOV`                       | `0F 20 /r`    | `MOV r32,CR2/CR3`      | `4`                                                                 |
| `MOV`                       | `0F 22 /r`    | `MOV CR0,r32`          | `17`                                                                |
| `MOV`                       | `0F 22 /r`    | `MOV CR2/CR3,r32`      | `4`                                                                 |
| `MOV`                       | `0F 21 /r`    | `MOV r32,DR0--3`       | `9`                                                                 |
| `MOV`                       | `0F 21 /r`    | `MOV r32,DR6/DR7`      | `9`                                                                 |
| `MOV`                       | `0F 23 /r`    | `MOV DR0--3,r32`       | `10`                                                                |
| `MOV`                       | `0F 23 /r`    | `MOV DR6/DR7,r32`      | `10`                                                                |
| `MOV`                       | `0F 24 /r`    | `MOV r32,TR3`          | `3`                                                                 |
| `MOV`                       | `0F 24 /r`    | `MOV r32,TR4--7`       | `4`                                                                 |
| `MOV`                       | `0F 26 /r`    | `MOV TR3,r32`          | `4`                                                                 |
| `MOV`                       | `0F 26 /r`    | `MOV TR4--7,r32`       | `4`                                                                 |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A4`          | `MOVS m8,m8`           | `7`                                                                 |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVS m16,m16`         | `7`                                                                 |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVS m32,m32`         | `7`                                                                 |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A4`          | `MOVSB`                | `7`                                                                 |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVSW`                | `7`                                                                 |
| `MOVS/MOVSB/MOVSW/MOVSD`    | `A5`          | `MOVSD`                | `7`                                                                 |
| `MOVSX`                     | `0F BE /r`    | `MOVSX r16,r/m8`       | `3/3`                                                               |
| `MOVSX`                     | `0F BE /r`    | `MOVSX r32,r/m8`       | `3/3`                                                               |
| `MOVSX`                     | `0F BF /r`    | `MOVSX r32,r/m16`      | `3/3`                                                               |
| `MOVZX`                     | `0F B6 /r`    | `MOVZX r16,r/m8`       | `3/3`                                                               |
| `MOVZX`                     | `0F B6 /r`    | `MOVZX r32,r/m8`       | `3/3`                                                               |
| `MOVZX`                     | `0F B7 /r`    | `MOVZX r32,r/m16`      | `3/3`                                                               |
| `MUL`                       | `F6 /4`       | `MUL AL,r/m8`          | `13-18/13-18`                                                       |
| `MUL`                       | `F7 /4`       | `MUL AX,r/m16`         | `13-26/13-26`                                                       |
| `MUL`                       | `F7 /4`       | `MUL EAX,r/m32`        | `13-42/13-42`                                                       |
| `NEG`                       | `F6 /3`       | `NEG r/m8`             | `1/3`                                                               |
| `NEG`                       | `F7 /3`       | `NEG r/m16`            | `1/3`                                                               |
| `NEG`                       | `F7 /3`       | `NEG r/m32`            | `1/3`                                                               |
| `NOP`                       | `90`          | `NOP`                  | `1`                                                                 |
| `NOT`                       | `F6 /2`       | `NOT r/m8`             | `1/3`                                                               |
| `NOT`                       | `F7 /2`       | `NOT r/m16`            | `1/3`                                                               |
| `NOT`                       | `F7 /2`       | `NOT r/m32`            | `1/3`                                                               |
| `OR`                        | `0C ib`       | `OR AL,imm8`           | `1`                                                                 |
| `OR`                        | `0D iw`       | `OR AX,imm16`          | `1`                                                                 |
| `OR`                        | `0D id`       | `OR EAX,imm32`         | `1`                                                                 |
| `OR`                        | `80 /1 ib`    | `OR r/m8,imm8`         | `1/3`                                                               |
| `OR`                        | `81 /1 iw`    | `OR r/m16,imm16`       | `1/3`                                                               |
| `OR`                        | `81 /1 id`    | `OR r/m32,imm32`       | `1/3`                                                               |
| `OR`                        | `83 /1 ib`    | `OR r/m16,imm8`        | `1/3`                                                               |
| `OR`                        | `83 /1 ib`    | `OR r/m32,imm8`        | `1/3`                                                               |
| `OR`                        | `08 /r`       | `OR r/m8,r8`           | `1/3`                                                               |
| `OR`                        | `09 /r`       | `OR r/m16,r16`         | `1/3`                                                               |
| `OR`                        | `09 /r`       | `OR r/m32,r32`         | `1/3`                                                               |
| `OR`                        | `0A /r`       | `OR r8,r/m8`           | `1/2`                                                               |
| `OR`                        | `0B /r`       | `OR r16,r/m16`         | `1/2`                                                               |
| `OR`                        | `0B /r`       | `OR r32,r/m32`         | `1/2`                                                               |
| `OUT`                       | `E6 ib`       | `OUT imm8,AL`          | `16,pm=11/31,v86=29`                                                |
| `OUT`                       | `E7 ib`       | `OUT imm8,AX`          | `16,pm=11/31,v86=29`                                                |
| `OUT`                       | `E7 ib`       | `OUT imm8,EAX`         | `16,pm=11/31,v86=29`                                                |
| `OUT`                       | `EE`          | `OUT DX,AL`            | `16,pm=10/30,v86=29`                                                |
| `OUT`                       | `EF`          | `OUT DX,AX`            | `16,pm=10/30,v86=29`                                                |
| `OUT`                       | `EF`          | `OUT DX,EAX`           | `16,pm=10/30,v86=29`                                                |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6E`          | `OUTS DX,r/m8`         | `17,pm=10/32,v86=30`                                                |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTS DX,r/m16`        | `17,pm=10/32,v86=30`                                                |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTS DX,r/m32`        | `17,pm=10/32,v86=30`                                                |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6E`          | `OUTSB`                | `17,pm=10/32,v86=30`                                                |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTSW`                | `17,pm=10/32,v86=30`                                                |
| `OUTS/OUTSB/OUTSW/OUTSD`    | `6F`          | `OUTSD`                | `17,pm=10/32,v86=30`                                                |
| `POP`                       | `8F /0`       | `POP m16`              | `5`                                                                 |
| `POP`                       | `8F /0`       | `POP m32`              | `5`                                                                 |
| `POP`                       | `58+rw`       | `POP r16`              | `4`                                                                 |
| `POP`                       | `58+rd`       | `POP r32`              | `4`                                                                 |
| `POP`                       | `1F`          | `POP DS`               | `3,pm=9`                                                            |
| `POP`                       | `07`          | `POP ES`               | `3,pm=9`                                                            |
| `POP`                       | `17`          | `POP SS`               | `3,pm=9`                                                            |
| `POP`                       | `0F A1`       | `POP FS`               | `3,pm=9`                                                            |
| `POP`                       | `0F A9`       | `POP GS`               | `3,pm=9`                                                            |
| `POPA/POPAD`                | `61`          | `POPA`                 | `9`                                                                 |
| `POPA/POPAD`                | `61`          | `POPAD`                | `9`                                                                 |
| `POPF/POPFD`                | `9D`          | `POPF`                 | `9/6 (rv/p)`                                                        |
| `POPF/POPFD`                | `9D`          | `POPFD`                | `9/6 (rv/p)`                                                        |
| `PUSH`                      | `FF /6`       | `PUSH m16`             | `4`                                                                 |
| `PUSH`                      | `FF /6`       | `PUSH m32`             | `4`                                                                 |
| `PUSH`                      | `50+rw`       | `PUSH r16`             | `1`                                                                 |
| `PUSH`                      | `50+rd`       | `PUSH r32`             | `1`                                                                 |
| `PUSH`                      | `6A`          | `PUSH imm8`            | `1`                                                                 |
| `PUSH`                      | `68`          | `PUSH imm16`           | `1`                                                                 |
| `PUSH`                      | `68`          | `PUSH imm32`           | `1`                                                                 |
| `PUSH`                      | `0E`          | `PUSH CS`              | `3`                                                                 |
| `PUSH`                      | `16`          | `PUSH SS`              | `3`                                                                 |
| `PUSH`                      | `1E`          | `PUSH DS`              | `3`                                                                 |
| `PUSH`                      | `06`          | `PUSH ES`              | `3`                                                                 |
| `PUSH`                      | `0F A0`       | `PUSH FS`              | `3`                                                                 |
| `PUSH`                      | `0F A8`       | `PUSH GS`              | `3`                                                                 |
| `PUSHA/PUSHAD`              | `60`          | `PUSHA`                | `11`                                                                |
| `PUSHA/PUSHAD`              | `60`          | `PUSHAD`               | `11`                                                                |
| `PUSHF/PUSHFD`              | `9C`          | `PUSHF`                | `4/3 (rv/p)`                                                        |
| `PUSHF/PUSHFD`              | `9C`          | `PUSHFD`               | `4/3 (rv/p)`                                                        |
| `RCL/RCR/ROL/ROR`           | `D0 /0`       | `ROL r/m8,1`           | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D2 /0`       | `ROL r/m8,CL`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `C0 /0 ib`    | `ROL r/m8,imm8`        | `2/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D1 /0`       | `ROL r/m16,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /0`       | `ROL r/m16,CL`         | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `C1 /0 ib`    | `ROL r/m16,imm8`       | `2/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D1 /0`       | `ROL r/m32,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /0`       | `ROL r/m32,CL`         | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `C1 /0 ib`    | `ROL r/m32,imm8`       | `2/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D0 /1`       | `ROR r/m8,1`           | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D2 /1`       | `ROR r/m8,CL`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `C0 /1 ib`    | `ROR r/m8,imm8`        | `2/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D1 /1`       | `ROR r/m16,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /1`       | `ROR r/m16,CL`         | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `C1 /1 ib`    | `ROR r/m16,imm8`       | `2/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D1 /1`       | `ROR r/m32,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /1`       | `ROR r/m32,CL`         | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `C1 /1 ib`    | `ROR r/m32,imm8`       | `2/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D0 /2`       | `RCL r/m8,1`           | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D2 /2`       | `RCL r/m8,CL`          | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `C0 /2 ib`    | `RCL r/m8,imm8`        | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `D1 /2`       | `RCL r/m16,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /2`       | `RCL r/m16,CL`         | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `C1 /2 ib`    | `RCL r/m16,imm8`       | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `D1 /2`       | `RCL r/m32,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /2`       | `RCL r/m32,CL`         | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `C1 /2 ib`    | `RCL r/m32,imm8`       | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `D0 /3`       | `RCR r/m8,1`           | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D2 /3`       | `RCR r/m8,CL`          | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `C0 /3 ib`    | `RCR r/m8,imm8`        | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `D1 /3`       | `RCR r/m16,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /3`       | `RCR r/m16,CL`         | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `C1 /3 ib`    | `RCR r/m16,imm8`       | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `D1 /3`       | `RCR r/m32,1`          | `3/4`                                                               |
| `RCL/RCR/ROL/ROR`           | `D3 /3`       | `RCR r/m32,CL`         | `8-30/9-31`                                                         |
| `RCL/RCR/ROL/ROR`           | `C1 /3 ib`    | `RCR r/m32,imm8`       | `8-30/9-31`                                                         |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6C`       | `REP INS r/m8,DX`      | `16+8c,pm=10+8c/30+8c,v86=29+8c`                                    |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6D`       | `REP INS r/m16,DX`     | `16+8c,pm=10+8c/30+8c,v86=29+8c`                                    |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6D`       | `REP INS r/m32,DX`     | `16+8c,pm=10+8c/30+8c,v86=29+8c`                                    |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A4`       | `REP MOVS m8,m8`       | `c=0:5, c=1:13, c>1:12+3c`                                          |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A5`       | `REP MOVS m16,m16`     | `c=0:5, c=1:13, c>1:12+3c`                                          |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A5`       | `REP MOVS m32,m32`     | `c=0:5, c=1:13, c>1:12+3c`                                          |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6E`       | `REP OUTS DX,r/m8`     | `17+5c,pm=11+5c/31+5c,v86=30+5c`                                    |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6F`       | `REP OUTS DX,r/m16`    | `17+5c,pm=11+5c/31+5c,v86=30+5c`                                    |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 6F`       | `REP OUTS DX,r/m32`    | `17+5c,pm=11+5c/31+5c,v86=30+5c`                                    |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AA`       | `REP STOS m8`          | `c=0:5, c>0:7+4c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AB`       | `REP STOS m16`         | `c=0:5, c>0:7+4c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AB`       | `REP STOS m32`         | `c=0:5, c>0:7+4c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AC`       | `REP LODS m8`          | `c=0:5, c>0:7+4c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AD`       | `REP LODS m16`         | `c=0:5, c>0:7+4c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AD`       | `REP LODS m32`         | `c=0:5, c>0:7+4c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A6`       | `REPE CMPS m8,m8`      | `c=0:5, c>0:7+7c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A7`       | `REPE CMPS m16,m16`    | `c=0:5, c>0:7+7c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 A7`       | `REPE CMPS m32,m32`    | `c=0:5, c>0:7+7c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AE`       | `REPE SCAS m8`         | `c=0:5, c>0:7+5c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AF`       | `REPE SCAS m16`        | `c=0:5, c>0:7+5c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F3 AF`       | `REPE SCAS m32`        | `c=0:5, c>0:7+5c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 A6`       | `REPNE CMPS m8,m8`     | `c=0:5, c>0:7+7c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 A7`       | `REPNE CMPS m16,m16`   | `c=0:5, c>0:7+7c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 A7`       | `REPNE CMPS m32,m32`   | `c=0:5, c>0:7+7c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 AE`       | `REPNE SCAS m8`        | `c=0:5, c>0:7+5c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 AF`       | `REPNE SCAS m16`       | `c=0:5, c>0:7+5c`                                                   |
| `REP/REPE/REPZ/REPNE/REPNZ` | `F2 AF`       | `REPNE SCAS m32`       | `c=0:5, c>0:7+5c`                                                   |
| `RET`                       | `C3`          | `RET`                  | `5`                                                                 |
| `RET`                       | `C2 iw`       | `RET imm16`            | `5`                                                                 |
| `RET`                       | `CB`          | `RET (interseg)`       | `13,pm=17`                                                          |
| `RET`                       | `CB`          | `RET (interseg)`       | `pm=35 (outer level)`                                               |
| `RET`                       | `CA iw`       | `RET imm16 (interseg)` | `14,pm=18`                                                          |
| `RET`                       | `CA iw`       | `RET imm16 (interseg)` | `pm=36 (outer level)`                                               |
| `SAHF`                      | `9E`          | `SAHF`                 | `2`                                                                 |
| `SAL/SAR/SHL/SHR`           | `D0 /4`       | `SAL r/m8,1`           | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D2 /4`       | `SAL r/m8,CL`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C0 /4 ib`    | `SAL r/m8,imm8`        | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SAL r/m16,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SAL r/m16,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SAL r/m16,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SAL r/m32,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SAL r/m32,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SAL r/m32,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D0 /7`       | `SAR r/m8,1`           | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D2 /7`       | `SAR r/m8,CL`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C0 /7 ib`    | `SAR r/m8,imm8`        | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /7`       | `SAR r/m16,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /7`       | `SAR r/m16,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /7 ib`    | `SAR r/m16,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /7`       | `SAR r/m32,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /7`       | `SAR r/m32,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /7 ib`    | `SAR r/m32,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D0 /4`       | `SHL r/m8,1`           | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D2 /4`       | `SHL r/m8,CL`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C0 /4 ib`    | `SHL r/m8,imm8`        | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SHL r/m16,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SHL r/m16,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SHL r/m16,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /4`       | `SHL r/m32,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /4`       | `SHL r/m32,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /4 ib`    | `SHL r/m32,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D0 /5`       | `SHR r/m8,1`           | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D2 /5`       | `SHR r/m8,CL`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C0 /5 ib`    | `SHR r/m8,imm8`        | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /5`       | `SHR r/m16,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /5`       | `SHR r/m16,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /5 ib`    | `SHR r/m16,imm8`       | `2/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D1 /5`       | `SHR r/m32,1`          | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `D3 /5`       | `SHR r/m32,CL`         | `3/4`                                                               |
| `SAL/SAR/SHL/SHR`           | `C1 /5 ib`    | `SHR r/m32,imm8`       | `2/4`                                                               |
| `SBB`                       | `1C ib`       | `SBB AL,imm8`          | `1`                                                                 |
| `SBB`                       | `1D iw`       | `SBB AX,imm16`         | `1`                                                                 |
| `SBB`                       | `1D id`       | `SBB EAX,imm32`        | `1`                                                                 |
| `SBB`                       | `80 /3 ib`    | `SBB r/m8,imm8`        | `1/3`                                                               |
| `SBB`                       | `81 /3 iw`    | `SBB r/m16,imm16`      | `1/3`                                                               |
| `SBB`                       | `81 /3 id`    | `SBB r/m32,imm32`      | `1/3`                                                               |
| `SBB`                       | `83 /3 ib`    | `SBB r/m16,imm8`       | `1/3`                                                               |
| `SBB`                       | `83 /3 ib`    | `SBB r/m32,imm8`       | `1/3`                                                               |
| `SBB`                       | `18 /r`       | `SBB r/m8,r8`          | `1/3`                                                               |
| `SBB`                       | `19 /r`       | `SBB r/m16,r16`        | `1/3`                                                               |
| `SBB`                       | `19 /r`       | `SBB r/m32,r32`        | `1/3`                                                               |
| `SBB`                       | `1A /r`       | `SBB r8,r/m8`          | `1/2`                                                               |
| `SBB`                       | `1B /r`       | `SBB r16,r/m16`        | `1/2`                                                               |
| `SBB`                       | `1B /r`       | `SBB r32,r/m32`        | `1/2`                                                               |
| `SCAS/SCASB/SCASW/SCASD`    | `AE`          | `SCAS m8`              | `6`                                                                 |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCAS m16`             | `6`                                                                 |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCAS m32`             | `6`                                                                 |
| `SCAS/SCASB/SCASW/SCASD`    | `AE`          | `SCASB`                | `6`                                                                 |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCASW`                | `6`                                                                 |
| `SCAS/SCASB/SCASW/SCASD`    | `AF`          | `SCASD`                | `6`                                                                 |
| `SETcc`                     | `0F 97`       | `SETA r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 93`       | `SETAE r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 92`       | `SETB r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 96`       | `SETBE r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 92`       | `SETC r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 94`       | `SETE r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9F`       | `SETG r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9D`       | `SETGE r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9C`       | `SETL r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9E`       | `SETLE r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 96`       | `SETNA r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 92`       | `SETNAE r/m8`          | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 93`       | `SETNB r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 97`       | `SETNBE r/m8`          | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 93`       | `SETNC r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 95`       | `SETNE r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9E`       | `SETNG r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9C`       | `SETNGE r/m8`          | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9D`       | `SETNL r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9F`       | `SETNLE r/m8`          | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 91`       | `SETNO r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9B`       | `SETNP r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 99`       | `SETNS r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 95`       | `SETNZ r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 90`       | `SETO r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9A`       | `SETP r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9A`       | `SETPE r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 9B`       | `SETPO r/m8`           | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 98`       | `SETS r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SETcc`                     | `0F 94`       | `SETZ r/m8`            | `4/3 (T/NT) reg, 3/4 (T/NT) mem`                                    |
| `SGDT/SIDT`                 | `0F 01 /0`    | `SGDT m`               | `10`                                                                |
| `SGDT/SIDT`                 | `0F 01 /1`    | `SIDT m`               | `10`                                                                |
| `SHLD`                      | `0F A4`       | `SHLD r/m16,r16,imm8`  | `2/3`                                                               |
| `SHLD`                      | `0F A4`       | `SHLD r/m32,r32,imm8`  | `2/3`                                                               |
| `SHLD`                      | `0F A5`       | `SHLD r/m16,r16,CL`    | `3/4`                                                               |
| `SHLD`                      | `0F A5`       | `SHLD r/m32,r32,CL`    | `3/4`                                                               |
| `SHRD`                      | `0F AC`       | `SHRD r/m16,r16,imm8`  | `2/3`                                                               |
| `SHRD`                      | `0F AC`       | `SHRD r/m32,r32,imm8`  | `2/3`                                                               |
| `SHRD`                      | `0F AD`       | `SHRD r/m16,r16,CL`    | `3/4`                                                               |
| `SHRD`                      | `0F AD`       | `SHRD r/m32,r32,CL`    | `3/4`                                                               |
| `SLDT`                      | `0F 00 /0`    | `SLDT r/m16`           | `pm=2/3`                                                            |
| `SMSW`                      | `0F 01 /4`    | `SMSW r/m16`           | `2/3`                                                               |
| `STC`                       | `F9`          | `STC`                  | `2`                                                                 |
| `STD`                       | `FD`          | `STD`                  | `2`                                                                 |
| `STI`                       | `FB`          | `STI`                  | `5`                                                                 |
| `STOS/STOSB/STOSW/STOSD`    | `AA`          | `STOS m8`              | `5`                                                                 |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOS m16`             | `5`                                                                 |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOS m32`             | `5`                                                                 |
| `STOS/STOSB/STOSW/STOSD`    | `AA`          | `STOSB`                | `5`                                                                 |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOSW`                | `5`                                                                 |
| `STOS/STOSB/STOSW/STOSD`    | `AB`          | `STOSD`                | `5`                                                                 |
| `STR`                       | `0F 00 /1`    | `STR r/m16`            | `pm=2/3`                                                            |
| `SUB`                       | `2C ib`       | `SUB AL,imm8`          | `1`                                                                 |
| `SUB`                       | `2D iw`       | `SUB AX,imm16`         | `1`                                                                 |
| `SUB`                       | `2D id`       | `SUB EAX,imm32`        | `1`                                                                 |
| `SUB`                       | `80 /5 ib`    | `SUB r/m8,imm8`        | `1/3`                                                               |
| `SUB`                       | `81 /5 iw`    | `SUB r/m16,imm16`      | `1/3`                                                               |
| `SUB`                       | `81 /5 id`    | `SUB r/m32,imm32`      | `1/3`                                                               |
| `SUB`                       | `83 /5 ib`    | `SUB r/m16,imm8`       | `1/3`                                                               |
| `SUB`                       | `83 /5 ib`    | `SUB r/m32,imm8`       | `1/3`                                                               |
| `SUB`                       | `28 /r`       | `SUB r/m8,r8`          | `1/3`                                                               |
| `SUB`                       | `29 /r`       | `SUB r/m16,r16`        | `1/3`                                                               |
| `SUB`                       | `29 /r`       | `SUB r/m32,r32`        | `1/3`                                                               |
| `SUB`                       | `2A /r`       | `SUB r8,r/m8`          | `1/2`                                                               |
| `SUB`                       | `2B /r`       | `SUB r16,r/m16`        | `1/2`                                                               |
| `SUB`                       | `2B /r`       | `SUB r32,r/m32`        | `1/2`                                                               |
| `TEST`                      | `A8 ib`       | `TEST AL,imm8`         | `1`                                                                 |
| `TEST`                      | `A9 iw`       | `TEST AX,imm16`        | `1`                                                                 |
| `TEST`                      | `A9 id`       | `TEST EAX,imm32`       | `1`                                                                 |
| `TEST`                      | `F6 /0 ib`    | `TEST r/m8,imm8`       | `1/2`                                                               |
| `TEST`                      | `F7 /0 iw`    | `TEST r/m16,imm16`     | `1/2`                                                               |
| `TEST`                      | `F7 /0 id`    | `TEST r/m32,imm32`     | `1/2`                                                               |
| `TEST`                      | `84 /r`       | `TEST r/m8,r8`         | `1/2`                                                               |
| `TEST`                      | `85 /r`       | `TEST r/m16,r16`       | `1/2`                                                               |
| `TEST`                      | `85 /r`       | `TEST r/m32,r32`       | `1/2`                                                               |
| `VERR`                      | `0F 00 /4`    | `VERR r/m16`           | `pm=11/11`                                                          |
| `VERW`                      | `0F 00 /5`    | `VERW r/m16`           | `pm=11/11`                                                          |
| `WAIT`                      | `9B`          | `WAIT`                 | `1-3`                                                               |
| `WBINVD`                    | `0F 09`       | `WBINVD`               | `5`                                                                 |
| `XADD`                      | `0F C0 /r`    | `XADD r/m8,r8`         | `3/4`                                                               |
| `XADD`                      | `0F C1 /r`    | `XADD r/m16,r16`       | `3/4`                                                               |
| `XADD`                      | `0F C1 /r`    | `XADD r/m32,r32`       | `3/4`                                                               |
| `XCHG`                      | `90+r`        | `XCHG AX,r16`          | `3`                                                                 |
| `XCHG`                      | `90+r`        | `XCHG r16,AX`          | `3`                                                                 |
| `XCHG`                      | `90+r`        | `XCHG EAX,r32`         | `3`                                                                 |
| `XCHG`                      | `90+r`        | `XCHG r32,EAX`         | `3`                                                                 |
| `XCHG`                      | `86 /r`       | `XCHG r/m8,r8`         | `3/5`                                                               |
| `XCHG`                      | `86 /r`       | `XCHG r8,r/m8`         | `3/5`                                                               |
| `XCHG`                      | `87 /r`       | `XCHG r/m16,r16`       | `3/5`                                                               |
| `XCHG`                      | `87 /r`       | `XCHG r16,r/m16`       | `3/5`                                                               |
| `XCHG`                      | `87 /r`       | `XCHG r/m32,r32`       | `3/5`                                                               |
| `XCHG`                      | `87 /r`       | `XCHG r32,r/m32`       | `3/5`                                                               |
| `XLAT`                      | `D7`          | `XLAT src-table`       | `4`                                                                 |
| `XOR`                       | `34 ib`       | `XOR AL,imm8`          | `1`                                                                 |
| `XOR`                       | `35 iw`       | `XOR AX,imm16`         | `1`                                                                 |
| `XOR`                       | `35 id`       | `XOR EAX,imm32`        | `1`                                                                 |
| `XOR`                       | `80 /6 ib`    | `XOR r/m8,imm8`        | `1/3`                                                               |
| `XOR`                       | `81 /6 iw`    | `XOR r/m16,imm16`      | `1/3`                                                               |
| `XOR`                       | `81 /6 id`    | `XOR r/m32,imm32`      | `1/3`                                                               |
| `XOR`                       | `83 /6 ib`    | `XOR r/m16,imm8`       | `1/3`                                                               |
| `XOR`                       | `83 /6 ib`    | `XOR r/m32,imm8`       | `1/3`                                                               |
| `XOR`                       | `30 /r`       | `XOR r/m8,r8`          | `1/3`                                                               |
| `XOR`                       | `31 /r`       | `XOR r/m16,r16`        | `1/3`                                                               |
| `XOR`                       | `31 /r`       | `XOR r/m32,r32`        | `1/3`                                                               |
| `XOR`                       | `32 /r`       | `XOR r8,r/m8`          | `1/2`                                                               |
| `XOR`                       | `33 /r`       | `XOR r16,r/m16`        | `1/2`                                                               |
| `XOR`                       | `33 /r`       | `XOR r32,r/m32`        | `1/2`                                                               |

## I/O Instruction Timing Detail

The 486 has distinct I/O timings for four CPU modes. All values from Table 10.2.

| Instruction              | Real Mode | PM (CPL<=IOPL) | PM (CPL>IOPL) | Virtual-86 Mode |
|--------------------------|----------:|----------------:|---------------:|----------------:|
| `IN` fixed port          |      `14` |             `9` |           `29` |            `27` |
| `IN` variable port (DX)  |      `14` |             `8` |           `28` |            `27` |
| `OUT` fixed port         |      `16` |            `11` |           `31` |            `29` |
| `OUT` variable port (DX) |      `16` |            `10` |           `30` |            `29` |
| `INS`                    |      `17` |            `10` |           `32` |            `30` |
| `OUTS`                   |      `17` |            `10` |           `32` |            `30` |
| `REP INS`                | `16+8c`   |        `10+8c`  |       `30+8c`  |        `29+8c`  |
| `REP OUTS`               | `17+5c`   |        `11+5c`  |       `31+5c`  |        `30+5c`  |

`c` = count in CX or ECX. Two clock cache miss penalty in all modes for single I/O.
For REP INS/OUTS, add 2 clocks to cache miss penalty per 16 bytes.

## Notes from Intel Manual

1. Assuming operand address and stack address fall in different cache sets.
2. Always locked, no cache hit case.
3. Clocks = 10 + max(log₂(|m|), n); m = multiplier value (min clocks for m=0); n = 3/5 for ±m.
4. RCL/RCR by CL: MN/MX = 8/30 for register, 9/31 for memory.
5. RCL/RCR by imm8: MN/MX = 8/30 for register, 9/31 for memory.
6. CMPXCHG: equal/not-equal cases — penalty is the same regardless of lock.
7. Addresses for memory read (for indirection), stack push/pop, and branch fall in different cache sets.
8. Penalty for cache miss: add 6 clocks for every 16 bytes copied to new stack frame.
9. Add 11 clocks for each unaccessed descriptor load.
10. Refer to task switch clock counts table for value of TS.
11. Add 4 extra clocks to the cache miss penalty for each 16 bytes.
12. BSF: Clocks = 8 + 4(b+1) + 3(i+1) + 3(n+1); = 6 if second operand = 0.
13. BSF: Clocks = 9 + 4(b+1) + 3(i+1) + 3(n+1); = 7 if second operand = 0.
14. BSR: Clocks = 7 + 3(32−n); 6 if second operand = 0.
15. BSR: Clocks = 8 + 3(32−n); 7 if second operand = 0.
16. Assuming two string addresses fall in different cache sets.
17. Cache miss penalty: add 6 clocks for every 16 bytes compared. Entire penalty on first compare.
18. Cache miss penalty: add 2 clocks for every 16 bytes of data. Entire penalty on first load.
19. Cache miss penalty: add 4 clocks for every 16 bytes moved (1 clock for first operation and 3 for the second).
20. Cache miss penalty: add 4 clocks for every 16 bytes scanned (2 clocks each for first and second operations).
21. Refer to interrupt clock counts table for value of INT.
22. Clock count includes one clock for using both displacement and immediate.
23. Refer to assumption 6 in the case of a cache miss.
