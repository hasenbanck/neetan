; Dhrystone Benchmark Version 2.1, NASM port for NEC PC-98.
; Licensed under MIT-0.
;
; Translated from dhry.p (Pascal, Reinhold P. Weicker, 1988).
;
; The program is a self-contained bootable image. The disk geometry is
; selected at assembly time via -DFDD_TYPE=2DD or -DFDD_TYPE=2HD; both
; layouts boot the same payload:
;   - PC-98 BIOS loads the first sector (256 B for 2DD, 1024 B for 2HD)
;     to 1FC0:0000 (this IPL).
;   - The IPL reads the remaining sectors from the floppy via INT 1Bh
;     into 1FC0:offset..1FFF (~7 KB), all in cylinder 0.
;   - Control transfers to main_entry at the byte after the IPL pad
;     (offset 0x0100 for 2DD, 0x0400 for 2HD).
;   - The program runs without an OS, halts via cli/hlt loop when finished.
;
; Calling convention (Borland Turbo Pascal style):
;   - Arguments pushed left-to-right.
;   - Callee removes them via "ret n".
;   - 16-bit values returned in AX. 32-bit in DX:AX. Booleans/enums in AL.
;   - var (reference) parameters pass a near 16-bit offset.
;   - All near pointers; the entire program lives in segment CS = DS = ES = SS.

[bits 16]
[cpu 8086]
[org 0x0000]

; ============================================================================
; Constants
; ============================================================================

TEXT_VRAM_SEG       equ 0xA000
TEXT_ATTR_OFFSET    equ 0x2000
TEXT_COLS           equ 80
TEXT_ROWS           equ 25
TEXT_ATTR           equ 0xE1            ; normal-intensity readable text

BIOS_BOOT_DAUA_ADDR equ 0x0584          ; BDA byte: boot DAUA passed by BIOS

%ifndef FDD_TYPE
%define FDD_TYPE 2DD
%endif

%ifidn FDD_TYPE, 2DD
SECTOR_SIZE         equ 256
SECTOR_SIZE_CODE    equ 1               ; PC-98 INT 1Bh "N": 128 << N = 256
TOTAL_SECTORS       equ 32              ; 8192 / 256 = 32 sectors total program
%elifidn FDD_TYPE, 2HD
SECTOR_SIZE         equ 1024
SECTOR_SIZE_CODE    equ 3               ; PC-98 INT 1Bh "N": 128 << N = 1024
TOTAL_SECTORS       equ 8               ; 8192 / 1024 = 8 sectors total program
%else
%error "FDD_TYPE must be 2DD or 2HD"
%endif
SECTORS_TO_LOAD     equ TOTAL_SECTORS - 1

; Pascal enum (Ident1..Ident5) -> 0..4.
IDENT1              equ 0
IDENT2              equ 1
IDENT3              equ 2
IDENT4              equ 3
IDENT5              equ 4

; RecordType layout (36 bytes, only Ident1 variant ever used):
REC_PointerComp     equ 0               ; word
REC_Discr           equ 2               ; byte
REC_EnumComp        equ 3               ; byte (aliases Char1Comp / Enum2Comp)
REC_IntComp         equ 4               ; word (aliases Char2Comp / first 2 bytes of String2Comp)
REC_StringComp      equ 6               ; 30 bytes
REC_SIZE            equ 36

; Dhrystone timing (100 Hz PC-98 interval timer).
MicrosecondsPerClock equ 10000          ; 1 tick = 10 ms = 10000 us
BenchmarkTicks       equ 1000           ; 10 seconds at 100 Hz
VaxDhrystonesPerSec  equ 1757           ; VAX 11/780, nominal 1 MIPS

; ============================================================================
; IPL stub (offset 0x0000..0x00FF)
; ============================================================================

ipl_entry:
    cli
    mov ax, cs
    mov ss, ax
    mov sp, 0xFFFE
    mov ds, ax
    mov es, ax
    sti

    ; Read the DAUA the BIOS stored in the BIOS data area at 0:0584.
    push ds
    xor ax, ax
    mov ds, ax
    mov al, [BIOS_BOOT_DAUA_ADDR]
    pop ds
    mov bl, al                      ; preserve DAUA in BL across reads

    push cs
    pop es

%ifidn FDD_TYPE, 2DD
    ; Segment 0x1FC0 spans linear 0x1FC00..0x2FBFF, which crosses the 64 KB
    ; physical-page boundary at 0x20000. The PC-98 INT 1Bh read rejects any
    ; transfer that crosses such a boundary (DMA 64KB limit), so we issue
    ; two separate reads:
    ;   - Pass 1: sectors 2..4 (3 sectors = 768 B) into offset 0x100..0x3FF
    ;             (linear 0x1FD00..0x1FFFF -- still in page 1).
    ;   - Pass 2: sectors 5..16 of head 0 + 1..16 of head 1 (28 sectors via
    ;             multi-track) into offset 0x400..0x1FFF (linear 0x20000+).

    ; ----- Pass 1: sectors 2..4 (3 sectors) at offset 0x0100..0x03FF -----
    mov ah, 0x96                    ; multi-track + update seek + read
    mov ch, SECTOR_SIZE_CODE
    mov cl, 0x00
    mov dh, 0x00
    mov dl, 0x02
    mov al, bl
    push bx
    mov bx, 3 * SECTOR_SIZE
    mov bp, 0x0100
    int 0x1B
    pop bx
    jc .boot_error

    ; ----- Pass 2: sectors 5..16 of head 0 + 1..16 of head 1 (28 sectors)
    ; via multi-track at offset 0x0400..0x1FFF -----
    mov ah, 0x86                    ; multi-track + read (no seek; cyl unchanged)
    mov ch, SECTOR_SIZE_CODE
    mov cl, 0x00
    mov dh, 0x00
    mov dl, 0x05
    mov al, bl
    push bx
    mov bx, 28 * SECTOR_SIZE
    mov bp, 0x0400
    int 0x1B
    pop bx
    jc .boot_error
%elifidn FDD_TYPE, 2HD
    ; 2HD has 8 sectors of 1024 B per track, so the entire 8 KB program is
    ; one cylinder. Sector 1 is the IPL; we load sectors 2..8 of head 0 to
    ; offset 0x0400 (linear 0x20000), which lies exactly on the 64 KB page
    ; boundary, so the 7 * 1024 = 7168 B transfer stays inside page 2.
    mov ah, 0x86                    ; multi-track + read (no seek)
    mov ch, SECTOR_SIZE_CODE
    mov cl, 0x00
    mov dh, 0x00
    mov dl, 0x02
    mov al, bl
    push bx
    mov bx, SECTORS_TO_LOAD * SECTOR_SIZE
    mov bp, 0x0400
    int 0x1B
    pop bx
    jc .boot_error
%endif

    jmp main_entry

.boot_error:
    mov ax, TEXT_VRAM_SEG
    mov es, ax
    xor di, di
    mov si, ipl_str_boot_error
.be_loop:
    mov al, [cs:si]
    inc si
    or al, al
    jz .be_halt
    mov [es:di], al
    mov byte [es:di + 1], 0
    mov byte [es:di + TEXT_ATTR_OFFSET], TEXT_ATTR
    mov byte [es:di + TEXT_ATTR_OFFSET + 1], 0
    add di, 2
    jmp .be_loop
.be_halt:
    cli
    hlt
    jmp .be_halt

ipl_str_boot_error: db "BOOT ERR", 0

; Pad IPL to exactly one sector so main_entry lands at the start of the
; second sector (offset 0x0100 for 2DD, 0x0400 for 2HD).
times SECTOR_SIZE - ($ - $$) db 0

; ============================================================================
; main_entry
; ============================================================================

main_entry:
    ; Segments are already CS = DS = ES = SS, SP = 0xFFFE from the IPL.
    cli

    ; Save the current INT 8 vector and install our tick-counter hook.
    push ds
    xor ax, ax
    mov ds, ax
    mov ax, [0x0020]                ; old INT 8 offset
    mov bx, [0x0022]                ; old INT 8 segment
    pop ds
    mov [old_int8_off], ax
    mov [old_int8_seg], bx

    push ds
    xor ax, ax
    mov ds, ax
    mov word [0x0020], int8_handler
    mov [0x0022], cs
    pop ds

    ; Zero the 32-bit tick counter.
    mov word [tick_count], 0
    mov word [tick_count + 2], 0

    ; Start the BIOS interval timer so IRQ 0 fires periodically (100 Hz).
    ; Without this the PIT is left in mode 0 (one-shot) with IRQ 0 masked.
    ; INT 1Ch AH=02h programs PIT ch0 mode 3 + unmasks IRQ 0; the user
    ; callback (set via ES:BX) only fires when CX hits 0, so we point it
    ; at a do-nothing IRET and pick CX=0xFFFF (~10.9 minutes of headroom).
    mov ah, 0x02
    push cs
    pop es
    mov bx, dummy_timer_callback
    mov cx, 0xFFFF
    int 0x1C
    sti

    ; Enable the text screen.
    mov ah, 0x0C
    int 0x18

    ; Program the 16-entry analog palette to a known state.
    ;
    ; Briefly enter 16-colour analog mode (mode2 bit 0 = 1) so writes to
    ; ports 0xA8/AA/AC/AE address the analog palette (index/G/R/B)
    ; instead of the digital palette pack registers.
    mov al, 0x01
    out 0x6A, al

    mov si, analog_palette_table
    mov cx, 16
    xor bl, bl
.set_palette_entry:
    mov al, bl
    out 0xA8, al                    ; palette index
    lodsb
    out 0xAA, al                    ; green
    lodsb
    out 0xAC, al                    ; red
    lodsb
    out 0xAE, al                    ; blue
    inc bl
    loop .set_palette_entry

    ; Restore 8-colour digital palette mode (mode2 bit 0 = 0).
    mov al, 0x00
    out 0x6A, al

    ; Initialize cursor + clear screen.
    mov byte [cursor_row], 0
    mov byte [cursor_col], 0
    call clear_screen

    ; ----- Dhrystone initializations -----
    ; new(NextPointerGlob); new(PointerGlob);
    ; PointerGlob^.PointerComp := NextPointerGlob;
    ; PointerGlob^.Discr       := Ident1;
    ; PointerGlob^.EnumComp    := Ident3;
    ; PointerGlob^.IntComp     := 40;
    ; PointerGlob^.StringComp  := 'DHRYSTONE PROGRAM, SOME STRING';
    ;
    ; (NextPointerGlob is left zero-initialised; the canonical final values
    ;  rely on Discr=0 i.e. Ident1.)
    mov word [NextPointerGlob], NextPointerGlob_storage
    mov word [PointerGlob], PointerGlob_storage

    mov bx, [PointerGlob]
    mov ax, [NextPointerGlob]
    mov [bx + REC_PointerComp], ax
    mov byte [bx + REC_Discr], IDENT1
    mov byte [bx + REC_EnumComp], IDENT3
    mov word [bx + REC_IntComp], 40

    mov si, str_some_string
    mov di, bx
    add di, REC_StringComp
    mov cx, 30
    rep movsb

    ; String1Glob := 'DHRYSTONE PROGRAM, 1''ST STRING';
    mov si, str_first_string
    mov di, String1Glob
    mov cx, 30
    rep movsb

    ; Array2Glob[8,7] := 10;  (offset = (8-1)*100 + (7-1)*2 = 712)
    mov word [Array2Glob + (7 * 100) + (6 * 2)], 10

    ; ----- Banner -----
    mov si, str_banner
    call print_string
    mov si, str_exec_starts
    call print_string

    ; ----- Start the 10-second measurement window. -----
    cli
    mov word [NumberOfRuns],     0
    mov word [NumberOfRuns + 2], 0
    mov byte [benchmark_done], 0
    mov word [benchmark_ticks_remaining], BenchmarkTicks
    mov byte [benchmark_active], 1
    mov ax, [tick_count]
    mov dx, [tick_count + 2]
    mov [BeginClock], ax
    mov [BeginClock + 2], dx
    sti

main_loop:
    cmp byte [benchmark_done], 0
    jne main_loop_done
    cmp word [NumberOfRuns + 2], 0xFFFF
    jne .body
    cmp word [NumberOfRuns], 0xFFFF
    je main_loop_done
.body:

    call Proc5
    call Proc4

    mov word [Int1Glob], 2
    mov word [Int2Glob], 3

    mov si, str_second_string
    mov di, String2Glob
    mov cx, 30
    rep movsb

    mov byte [EnumGlob], IDENT2

    ; BoolGlob := not Func2(String1Glob, String2Glob);
    mov ax, String1Glob
    push ax
    mov ax, String2Glob
    push ax
    call Func2
    xor al, 1
    mov [BoolGlob], al

    ; while Int1Glob < Int2Glob do ...
.while_int:
    mov ax, [Int1Glob]
    cmp ax, [Int2Glob]
    jge .while_int_done
    ; Int3Glob := 5 * Int1Glob - Int2Glob;
    mov ax, [Int1Glob]
    mov bx, 5
    imul bx                         ; AX = Int1Glob * 5 (low 16 bits = wrap)
    sub ax, [Int2Glob]
    mov [Int3Glob], ax
    ; Proc7(Int1Glob, Int2Glob, Int3Glob);
    push word [Int1Glob]
    push word [Int2Glob]
    mov ax, Int3Glob
    push ax
    call Proc7
    ; Int1Glob := Int1Glob + 1;
    inc word [Int1Glob]
    jmp .while_int
.while_int_done:

    ; Proc8(Array1Glob, Array2Glob, Int1Glob, Int3Glob);
    mov ax, Array1Glob
    push ax
    mov ax, Array2Glob
    push ax
    push word [Int1Glob]
    push word [Int3Glob]
    call Proc8

    ; Proc1(PointerGlob);
    mov ax, [PointerGlob]
    push ax
    call Proc1

    ; for CharIndex := 'A' to Char2Glob do ...
    mov al, 'A'
    mov [CharIndex], al
.for_char:
    mov al, [CharIndex]
    cmp al, [Char2Glob]
    ja .for_char_done
    ; if EnumGlob = Func1(CharIndex, 'C') then ...
    ; LTR push: CharIndex (Char1ParVal) first, then 'C' (Char2ParVal).
    mov al, [CharIndex]
    xor ah, ah
    push ax
    mov ax, 'C'
    push ax
    call Func1                      ; AL = enum result
    cmp al, [EnumGlob]
    jne .for_char_skip
    ; (not executed branch in canonical Dhrystone, but coded for fidelity)
    mov ax, IDENT1
    push ax
    mov ax, EnumGlob
    push ax
    call Proc6
    mov si, str_third_string
    mov di, String2Glob
    mov cx, 30
    rep movsb
    mov ax, [NumberOfRuns]
    mov [Int2Glob], ax
    mov [IntGlob], ax
.for_char_skip:
    inc byte [CharIndex]
    jmp .for_char
.for_char_done:

    ; Int2Glob := Int2Glob * Int1Glob;
    mov ax, [Int2Glob]
    imul word [Int1Glob]
    mov [Int2Glob], ax

    ; Int1Glob := Int2Glob div Int3Glob;
    mov ax, [Int2Glob]
    cwd
    idiv word [Int3Glob]
    mov [Int1Glob], ax

    ; Int2Glob := 7 * (Int2Glob - Int3Glob) - Int1Glob;
    mov ax, [Int2Glob]
    sub ax, [Int3Glob]
    mov bx, 7
    imul bx
    sub ax, [Int1Glob]
    mov [Int2Glob], ax

    ; Proc2(Int1Glob);
    mov ax, Int1Glob
    push ax
    call Proc2

    add word [NumberOfRuns],     1
    adc word [NumberOfRuns + 2], 0
    jmp main_loop

main_loop_done:
    ; EndClock := clock;
    cli
    mov byte [benchmark_active], 0
    mov ax, [tick_count]
    mov dx, [tick_count + 2]
    sti
    mov [EndClock], ax
    mov [EndClock + 2], dx

    mov si, str_exec_ends
    call print_string
    call do_self_check

    ; ----- SumClocks := EndClock - BeginClock; (32-bit) -----
    mov ax, [EndClock]
    sub ax, [BeginClock]
    mov [SumClocks], ax
    mov ax, [EndClock + 2]
    sbb ax, [BeginClock + 2]
    mov [SumClocks + 2], ax

    call print_perf_results

    cli
.halt_loop:
    hlt
    jmp .halt_loop

; ----------------------------------------------------------------------------
; do_self_check - compare every Dhrystone final value against its canonical
; expected value and print "Self check: OK" or "Self check: FAILED".
; First failing comparison short-circuits to the FAILED branch.
; Trashes AX, BX, CX, SI, DI.
; ----------------------------------------------------------------------------

do_self_check:
    cmp word [IntGlob], 5
    jne .fail
    cmp byte [BoolGlob], 1
    jne .fail
    cmp byte [Char1Glob], 'A'
    jne .fail
    cmp byte [Char2Glob], 'B'
    jne .fail
    cmp word [Array1Glob + (7 * 2)], 7
    jne .fail
    mov ax, [NumberOfRuns]
    add ax, 10
    cmp [Array2Glob + (7 * 100) + (6 * 2)], ax
    jne .fail

    cmp byte [PointerGlob_storage + REC_Discr], IDENT1
    jne .fail
    cmp byte [PointerGlob_storage + REC_EnumComp], IDENT3
    jne .fail
    cmp word [PointerGlob_storage + REC_IntComp], 17
    jne .fail
    mov si, str_some_string
    lea di, [PointerGlob_storage + REC_StringComp]
    mov cx, 30
    repe cmpsb
    jne .fail

    cmp byte [NextPointerGlob_storage + REC_Discr], IDENT1
    jne .fail
    cmp byte [NextPointerGlob_storage + REC_EnumComp], IDENT2
    jne .fail
    cmp word [NextPointerGlob_storage + REC_IntComp], 18
    jne .fail
    mov si, str_some_string
    lea di, [NextPointerGlob_storage + REC_StringComp]
    mov cx, 30
    repe cmpsb
    jne .fail

    cmp word [Int1Glob], 5
    jne .fail
    cmp word [Int2Glob], 13
    jne .fail
    cmp word [Int3Glob], 7
    jne .fail
    cmp byte [EnumGlob], IDENT2
    jne .fail

    mov si, str_first_string
    mov di, String1Glob
    mov cx, 30
    repe cmpsb
    jne .fail

    mov si, str_second_string
    mov di, String2Glob
    mov cx, 30
    repe cmpsb
    jne .fail

    mov si, str_self_check_ok
    call print_string
    ret
.fail:
    mov si, str_self_check_failed
    call print_string
    ret

; ----------------------------------------------------------------------------
; print_perf_results -- prints the Microseconds/Dhrystone, Dhrystones/Sec, and
; DMIPS lines using 32-bit integer math (one-decimal fixed-point).
;
;   Microseconds (per run) = SumClocks * 10000 / NumberOfRuns
;   DhrystonesPerSecond    = NumberOfRuns * 1000 / SumClocks  (10x), then /10
;   DMIPS                  = DhrystonesPerSecond / 1757
;
; DhrystonesPerSecond is computed as quotient and remainder terms to avoid
; truncating NumberOfRuns * 1000 before division.
; ----------------------------------------------------------------------------

print_perf_results:
    ; Measured runtime in seconds (SumClocks counts 10 ms ticks).
    mov si, str_seconds
    call print_string

    mov ax, [SumClocks]
    mov [div_num], ax
    mov ax, [SumClocks + 2]
    mov [div_num + 2], ax
    mov word [div_den],     10
    mov word [div_den + 2], 0
    call udiv32_32                  ; div_quot = SumClocks / 10

    mov ax, [div_quot]
    mov [div_num], ax
    mov ax, [div_quot + 2]
    mov [div_num + 2], ax
    mov word [div_den],     10
    mov word [div_den + 2], 0
    call udiv32_32                  ; div_quot = integer seconds, div_rem = decimal digit

    mov ax, 8
    push ax                         ; width
    push word [div_quot + 2]        ; integer high
    push word [div_quot]            ; integer low
    push word [div_rem]             ; decimal digit (low byte used)
    call print_real_field
    call print_newline

    mov si, str_us_per_run
    call print_string

    ; If NumberOfRuns is zero (impossible in practice) skip the line.
    mov ax, [NumberOfRuns]
    or ax, [NumberOfRuns + 2]
    jz .skip_us

    ; div_num := SumClocks * 10000
    mov ax, [SumClocks]
    mov [div_num], ax
    mov ax, [SumClocks + 2]
    mov [div_num + 2], ax
    mov bx, 10000
    call mul32_16

    ; div_den := NumberOfRuns
    mov ax, [NumberOfRuns]
    mov [div_den], ax
    mov ax, [NumberOfRuns + 2]
    mov [div_den + 2], ax

    call udiv32_32
    ; div_quot = integer microseconds, div_rem = remainder

    ; Save the integer part on the stack while we extract the first decimal
    ; digit from (div_rem * 10) / div_den.
    push word [div_quot + 2]
    push word [div_quot]

    mov ax, [div_rem]
    mov [div_num], ax
    mov ax, [div_rem + 2]
    mov [div_num + 2], ax
    mov bx, 10
    call mul32_16
    call udiv32_32
    ; div_quot = first decimal digit (low byte 0..9)

    pop ax                          ; integer low
    pop dx                          ; integer high
    mov bx, 8
    push bx                         ; width
    push dx                         ; integer high
    push ax                         ; integer low
    push word [div_quot]            ; decimal digit (low byte used)
    call print_real_field

.skip_us:
    call print_newline

    mov si, str_drys_per_sec
    call print_string

    mov word [DhrystonesPerSecondTimes10], 0
    mov word [DhrystonesPerSecondTimes10 + 2], 0

    ; If SumClocks is zero, skip.
    mov ax, [SumClocks]
    or ax, [SumClocks + 2]
    jz .skip_dps

    ; div_den := SumClocks
    mov ax, [SumClocks]
    mov [div_den], ax
    mov ax, [SumClocks + 2]
    mov [div_den + 2], ax

    ; div_num := NumberOfRuns
    mov ax, [NumberOfRuns]
    mov [div_num], ax
    mov ax, [NumberOfRuns + 2]
    mov [div_num + 2], ax
    call udiv32_32
    ; div_quot = whole runs per tick, div_rem = remaining runs.

    mov ax, [div_quot]
    mov [div_num], ax
    mov ax, [div_quot + 2]
    mov [div_num + 2], ax
    mov bx, 1000
    call mul32_16
    mov ax, [div_num]
    mov [DhrystonesPerSecondTimes10], ax
    mov ax, [div_num + 2]
    mov [DhrystonesPerSecondTimes10 + 2], ax

    mov ax, [div_rem]
    mov [div_num], ax
    mov ax, [div_rem + 2]
    mov [div_num + 2], ax
    mov bx, 1000
    call mul32_16
    call udiv32_32
    mov ax, [div_quot]
    add [DhrystonesPerSecondTimes10], ax
    mov ax, [div_quot + 2]
    adc [DhrystonesPerSecondTimes10 + 2], ax

    ; Split 10x DhrystonesPerSecond into integer + first decimal digit.
    mov ax, [DhrystonesPerSecondTimes10]
    mov [div_num], ax
    mov ax, [DhrystonesPerSecondTimes10 + 2]
    mov [div_num + 2], ax
    mov word [div_den],     10
    mov word [div_den + 2], 0
    call udiv32_32
    ; div_quot = integer Dhrystones/sec, div_rem (low byte) = decimal digit

    mov ax, 8
    push ax                         ; width
    push word [div_quot + 2]        ; integer high
    push word [div_quot]            ; integer low
    push word [div_rem]             ; decimal digit (low byte used)
    call print_real_field

.skip_dps:
    call print_newline

    mov si, str_dmips
    call print_string

    ; DMIPS, scaled by 10, is (DhrystonesPerSecond * 10) / 1757.
    mov ax, [DhrystonesPerSecondTimes10]
    mov [div_num], ax
    mov ax, [DhrystonesPerSecondTimes10 + 2]
    mov [div_num + 2], ax
    mov word [div_den],     VaxDhrystonesPerSec
    mov word [div_den + 2], 0
    call udiv32_32

    ; Split 10x DMIPS into integer + first decimal digit.
    mov ax, [div_quot]
    mov [div_num], ax
    mov ax, [div_quot + 2]
    mov [div_num + 2], ax
    mov word [div_den],     10
    mov word [div_den + 2], 0
    call udiv32_32

    mov ax, 8
    push ax                         ; width
    push word [div_quot + 2]        ; integer high
    push word [div_quot]            ; integer low
    push word [div_rem]             ; decimal digit (low byte used)
    call print_real_field
    call print_newline
    ret

; ============================================================================
; Dhrystone procedures - Pascal calling convention:
;   args pushed left-to-right, callee pops via "ret n".
; ============================================================================

; ----------------------------------------------------------------------------
; Proc1(PointerParVal: RecordPointer)
;   stack on entry: [BP+4] = PointerParVal (word)
; ----------------------------------------------------------------------------
Proc1:
    push bp
    mov bp, sp
    push si
    push di

    mov bx, [bp + 4]                ; BX = PointerParVal
    mov si, [bx + REC_PointerComp]  ; SI = PointerParVal^.PointerComp (= NextPointerGlob)

    ; PointerParVal^.PointerComp^ := PointerGlob^;
    ; (copy 36 bytes from PointerGlob to SI)
    push si
    mov di, si
    mov si, [PointerGlob]
    mov cx, REC_SIZE
    rep movsb
    pop si

    ; PointerParVal^.IntComp := 5;
    mov word [bx + REC_IntComp], 5

    ; IntComp := PointerParVal^.IntComp;          (i.e. SI^.IntComp := 5)
    mov word [si + REC_IntComp], 5

    ; PointerComp := PointerParVal^.PointerComp;
    ; Both are SI (since PointerParVal^.PointerComp = SI), so SI^.PointerComp := SI.
    mov [si + REC_PointerComp], si

    ; Proc3(PointerComp);   var-ref to SI.PointerComp
    lea ax, [si + REC_PointerComp]
    push ax
    call Proc3

    ; Proc3 (via Proc7) clobbers BX, so reload PointerParVal before any
    ; further [BX + ...] accesses below.
    mov bx, [bp + 4]

    ; if Discr = Ident1 then ...
    cmp byte [si + REC_Discr], IDENT1
    jne .else_branch

    ; IntComp := 6;
    mov word [si + REC_IntComp], 6

    ; Proc6(PointerParVal^.EnumComp, EnumComp);
    mov al, [bx + REC_EnumComp]
    xor ah, ah
    push ax
    lea ax, [si + REC_EnumComp]
    push ax
    call Proc6

    ; PointerComp := PointerGlob^.PointerComp;
    mov bx, [PointerGlob]
    mov ax, [bx + REC_PointerComp]
    mov [si + REC_PointerComp], ax

    ; Proc7(IntComp, 10, IntComp);
    push word [si + REC_IntComp]
    mov ax, 10
    push ax
    lea ax, [si + REC_IntComp]
    push ax
    call Proc7
    jmp .done

.else_branch:
    ; PointerParVal^ := PointerParVal^.PointerComp^;
    push di
    push si
    mov di, bx
    ; SI is already the source PointerParVal^.PointerComp.
    mov cx, REC_SIZE
    rep movsb
    pop si
    pop di

.done:
    pop di
    pop si
    pop bp
    ret 2

; ----------------------------------------------------------------------------
; Proc2(var IntParRef: OneToFifty)
;   stack on entry: [BP+4] = IntParRef offset (word)
; locals: IntLoc (word), EnumLoc (byte)
; ----------------------------------------------------------------------------
Proc2:
    push bp
    mov bp, sp
    sub sp, 4                       ; [BP-2]=IntLoc, [BP-4]=EnumLoc (byte; word for align)

    mov bx, [bp + 4]                ; BX = IntParRef
    mov ax, [bx]
    add ax, 10
    mov [bp - 2], ax                ; IntLoc := IntParRef + 10
.repeat:
    cmp byte [Char1Glob], 'A'
    jne .check_until
    dec word [bp - 2]               ; IntLoc := IntLoc - 1
    mov ax, [bp - 2]
    sub ax, [IntGlob]
    mov bx, [bp + 4]
    mov [bx], ax                    ; IntParRef := IntLoc - IntGlob
    mov byte [bp - 4], IDENT1
.check_until:
    cmp byte [bp - 4], IDENT1
    jne .repeat

    mov sp, bp
    pop bp
    ret 2

; ----------------------------------------------------------------------------
; Proc3(var PointerParRef: RecordPointer)
;   stack on entry: [BP+4] = PointerParRef offset (word, points to a "word")
; ----------------------------------------------------------------------------
Proc3:
    push bp
    mov bp, sp

    ; Pascal: if PointerGlob <> nil then PointerParRef := PointerGlob^.PointerComp;
    mov ax, [PointerGlob]
    or ax, ax
    jz .after_pointer_assignment
    mov bx, ax
    mov ax, [bx + REC_PointerComp]
    mov bx, [bp + 4]
    mov [bx], ax
.after_pointer_assignment:

    ; Proc7(10, IntGlob, var PointerGlob^.IntComp);
    mov ax, 10
    push ax
    push word [IntGlob]
    mov bx, [PointerGlob]
    lea ax, [bx + REC_IntComp]
    push ax
    call Proc7

    pop bp
    ret 2

; ----------------------------------------------------------------------------
; Proc4 (no params)  - executed once
; locals: BoolLoc (byte)
; ----------------------------------------------------------------------------
Proc4:
    push bp
    mov bp, sp
    sub sp, 2                       ; BoolLoc

    xor al, al
    cmp byte [Char1Glob], 'A'
    jne .b_set
    mov al, 1
.b_set:
    mov [bp - 2], al                ; BoolLoc

    mov al, [bp - 2]
    or al, [BoolGlob]
    mov [BoolGlob], al

    mov byte [Char2Glob], 'B'

    mov sp, bp
    pop bp
    ret

; ----------------------------------------------------------------------------
; Proc5 (no params)  - executed once
; ----------------------------------------------------------------------------
Proc5:
    mov byte [Char1Glob], 'A'
    mov byte [BoolGlob], 0
    ret

; ----------------------------------------------------------------------------
; Proc6(EnumParVal: Enumeration; var EnumParRef: Enumeration)
;   stack: [BP+6] = EnumParVal (low byte), [BP+4] = EnumParRef offset
; ----------------------------------------------------------------------------
Proc6:
    push bp
    mov bp, sp
    push bx

    mov bx, [bp + 4]                ; BX = EnumParRef
    mov al, [bp + 6]                ; AL = EnumParVal byte
    mov [bx], al                    ; EnumParRef := EnumParVal

    ; if not Func3(EnumParVal) then EnumParRef := Ident4
    push word [bp + 6]
    call Func3                      ; AL = boolean (Func3's `ret 2` pops the arg)
    or al, al
    jnz .case
    mov bx, [bp + 4]
    mov byte [bx], IDENT4

.case:
    mov al, [bp + 6]
    mov bx, [bp + 4]
    cmp al, IDENT1
    je .ident1
    cmp al, IDENT2
    je .ident2
    cmp al, IDENT3
    je .ident3
    cmp al, IDENT4
    je .ident4
    cmp al, IDENT5
    je .ident5
    jmp .case_done
.ident1:
    mov byte [bx], IDENT1
    jmp .case_done
.ident2:
    cmp word [IntGlob], 100
    jle .ident2_else
    mov byte [bx], IDENT1
    jmp .case_done
.ident2_else:
    mov byte [bx], IDENT4
    jmp .case_done
.ident3:
    mov byte [bx], IDENT2
    jmp .case_done
.ident4:
    jmp .case_done
.ident5:
    mov byte [bx], IDENT3
.case_done:

    pop bx
    pop bp
    ret 4

; ----------------------------------------------------------------------------
; Proc7(Int1ParVal, Int2ParVal: OneToFifty; var IntParRef: OneToFifty)
;   stack: [BP+8]=Int1ParVal, [BP+6]=Int2ParVal, [BP+4]=IntParRef
; ----------------------------------------------------------------------------
Proc7:
    push bp
    mov bp, sp

    mov ax, [bp + 8]                ; Int1ParVal
    add ax, 2                       ; IntLoc := Int1ParVal + 2
    add ax, [bp + 6]                ; Int2ParVal + IntLoc
    mov bx, [bp + 4]
    mov [bx], ax

    pop bp
    ret 6

; ----------------------------------------------------------------------------
; Proc8(var Array1ParRef: Array1DimInteger;
;       var Array2ParRef: Array2DimInteger;
;           Int1ParVal, Int2ParVal: integer)
;   stack: [BP+10]=Array1, [BP+8]=Array2, [BP+6]=Int1, [BP+4]=Int2
; locals: IntLoc (word) at [BP-2], IntIndex (word) at [BP-4]
; ----------------------------------------------------------------------------
Proc8:
    push bp
    mov bp, sp
    sub sp, 4
    push si
    push di

    ; IntLoc := Int1ParVal + 5;
    mov ax, [bp + 6]
    add ax, 5
    mov [bp - 2], ax

    ; Array1ParRef[IntLoc] := Int2ParVal;
    mov bx, [bp + 10]               ; Array1 base
    mov ax, [bp - 2]                ; IntLoc
    dec ax                          ; 1-indexed -> 0-indexed
    shl ax, 1                       ; *2 for word offset
    add bx, ax
    mov ax, [bp + 4]                ; Int2ParVal
    mov [bx], ax

    ; Array1ParRef[IntLoc+1] := Array1ParRef[IntLoc];
    mov bx, [bp + 10]
    mov ax, [bp - 2]
    dec ax
    shl ax, 1
    add bx, ax
    mov ax, [bx]                    ; AX = Array1[IntLoc]
    mov [bx + 2], ax                ; Array1[IntLoc+1]

    ; Array1ParRef[IntLoc+30] := IntLoc;
    mov bx, [bp + 10]
    mov ax, [bp - 2]
    add ax, 30 - 1                  ; (IntLoc+30) - 1 for 0-indexed
    shl ax, 1
    add bx, ax
    mov ax, [bp - 2]
    mov [bx], ax

    ; for IntIndex := IntLoc to IntLoc+1 do
    ;   Array2ParRef[IntLoc, IntIndex] := IntLoc;
    mov ax, [bp - 2]
    mov [bp - 4], ax                ; IntIndex := IntLoc
.for8:
    mov ax, [bp - 4]
    mov bx, [bp - 2]
    inc bx
    cmp ax, bx
    jg .for8_done

    ; offset = base + (IntLoc-1)*100 + (IntIndex-1)*2
    mov bx, [bp + 8]                ; Array2 base
    mov ax, [bp - 2]
    dec ax
    mov cx, 100
    mul cx
    add bx, ax
    mov ax, [bp - 4]
    dec ax
    shl ax, 1
    add bx, ax
    mov ax, [bp - 2]
    mov [bx], ax

    inc word [bp - 4]
    jmp .for8
.for8_done:

    ; Array2ParRef[IntLoc, IntLoc-1] := Array2ParRef[IntLoc, IntLoc-1] + 1;
    mov bx, [bp + 8]
    mov ax, [bp - 2]
    dec ax
    mov cx, 100
    mul cx
    add bx, ax
    mov ax, [bp - 2]
    sub ax, 2                       ; (IntLoc-1) - 1 for 0-indexed
    shl ax, 1
    add bx, ax
    inc word [bx]

    ; Array2ParRef[IntLoc+20, IntLoc] := Array1ParRef[IntLoc];
    mov bx, [bp + 10]
    mov ax, [bp - 2]
    dec ax
    shl ax, 1
    add bx, ax
    mov dx, [bx]                    ; DX = Array1[IntLoc]
    mov bx, [bp + 8]
    mov ax, [bp - 2]
    add ax, 20 - 1
    mov cx, 100
    mul cx
    add bx, ax
    mov ax, [bp - 2]
    dec ax
    shl ax, 1
    add bx, ax
    mov [bx], dx

    ; IntGlob := 5;
    mov word [IntGlob], 5

    pop di
    pop si
    mov sp, bp
    pop bp
    ret 8

; ----------------------------------------------------------------------------
; Func1(Char1ParVal, Char2ParVal: CapitalLetter): Enumeration
;   stack: [BP+6]=Char1ParVal (word), [BP+4]=Char2ParVal (word)
; ----------------------------------------------------------------------------
Func1:
    push bp
    mov bp, sp
    sub sp, 4                       ; [BP-2]=Char1Loc, [BP-4]=Char2Loc

    mov al, [bp + 6]                ; Char1ParVal
    mov [bp - 2], al                ; Char1Loc

    ; Pascal: Char2Loc := Char1Loc;
    mov al, [bp - 2]
    mov [bp - 4], al

    ; if Char2Loc <> Char2ParVal then Func1 := Ident1
    ; else begin Char1Glob := Char1Loc; Func1 := Ident2 end;
    mov al, [bp - 4]
    cmp al, [bp + 4]
    je .equal
    mov al, IDENT1
    jmp .done
.equal:
    mov bl, [bp - 2]
    mov [Char1Glob], bl
    mov al, IDENT2
.done:
    mov sp, bp
    pop bp
    ret 4

; ----------------------------------------------------------------------------
; Func2(var String1ParRef, String2ParRef: String30): boolean
;   stack: [BP+6]=String1Ref, [BP+4]=String2Ref
;   IntLoc:OneToThirty (word) at [BP-2], CharLoc:CapitalLetter at [BP-4]
; ----------------------------------------------------------------------------
Func2:
    push bp
    mov bp, sp
    sub sp, 4

    mov word [bp - 2], 2
    mov byte [bp - 4], 0
.while2:
    cmp word [bp - 2], 2
    jg .after_while
    ; Func1(String1ParRef[IntLoc], String2ParRef[IntLoc+1])
    mov bx, [bp + 6]
    mov ax, [bp - 2]
    dec ax
    add bx, ax                      ; &String1[IntLoc] (1-indexed)
    mov al, [bx]
    xor ah, ah
    push ax
    mov bx, [bp + 4]
    mov ax, [bp - 2]
    add bx, ax                      ; &String2[IntLoc+1]  (= base + (IntLoc+1-1) = base+IntLoc)
    mov al, [bx]
    xor ah, ah
    push ax
    call Func1
    cmp al, IDENT1
    jne .while2
    mov byte [bp - 4], 'A'
    inc word [bp - 2]
    jmp .while2
.after_while:

    ; if (CharLoc >= 'W') and (CharLoc < 'Z') then IntLoc := 7;
    cmp byte [bp - 4], 'W'
    jb .skip_w
    cmp byte [bp - 4], 'Z'
    jae .skip_w
    mov word [bp - 2], 7
.skip_w:

    ; if CharLoc = 'R' then Func2 := true
    cmp byte [bp - 4], 'R'
    jne .else_r
    mov al, 1
    jmp .done
.else_r:
    ; if String1ParRef > String2ParRef then ... else Func2 := false
    push si
    push di
    mov si, [bp + 6]
    mov di, [bp + 4]
    mov cx, 30
.cmp_loop:
    mov al, [si]
    mov bl, [di]
    cmp al, bl
    ja .gt
    jb .lt_or_eq
    inc si
    inc di
    loop .cmp_loop
    jmp .lt_or_eq                   ; equal -> not greater
.gt:
    pop di
    pop si
    add word [bp - 2], 7
    mov ax, [bp - 2]
    mov [IntGlob], ax
    mov al, 1
    jmp .done
.lt_or_eq:
    pop di
    pop si
    xor al, al
.done:
    mov sp, bp
    pop bp
    ret 4

; ----------------------------------------------------------------------------
; Func3(EnumParVal: Enumeration): boolean
;   stack: [BP+4]=EnumParVal (word)
; ----------------------------------------------------------------------------
Func3:
    push bp
    mov bp, sp
    sub sp, 2                       ; EnumLoc

    ; Pascal: EnumLoc := EnumParVal;
    mov al, [bp + 4]
    mov [bp - 2], al

    cmp byte [bp - 2], IDENT3
    jne .false
    mov al, 1
    jmp .done
.false:
    xor al, al
.done:
    mov sp, bp
    pop bp
    ret 2

; ============================================================================
; I/O routines
; ============================================================================

; ----------------------------------------------------------------------------
; clear_screen - zero text VRAM and attribute plane.
; ----------------------------------------------------------------------------
clear_screen:
    push es
    push di
    mov ax, TEXT_VRAM_SEG
    mov es, ax
    xor di, di
    mov cx, TEXT_COLS * TEXT_ROWS
    mov ax, 0x0000
    rep stosw                       ; clear char plane (chars + reserved byte)
    mov di, TEXT_ATTR_OFFSET
    mov cx, TEXT_COLS * TEXT_ROWS
    mov ax, TEXT_ATTR
    rep stosw                       ; clear attr plane (attribute byte + 0)
    mov byte [cursor_row], 0
    mov byte [cursor_col], 0
    pop di
    pop es
    ret

; ----------------------------------------------------------------------------
; scroll_one_line - shift rows 1..24 up to 0..23, clear row 24.
; Trashes AX, CX, SI, DI, ES.
; ----------------------------------------------------------------------------
scroll_one_line:
    push es
    push ds
    push si
    push di

    mov ax, TEXT_VRAM_SEG
    mov es, ax
    mov ds, ax

    ; Char plane: copy 24*80*2 = 3840 bytes from offset 160 to 0.
    xor di, di
    mov si, TEXT_COLS * 2
    mov cx, TEXT_COLS * 2 * (TEXT_ROWS - 1) / 2
    rep movsw

    ; Clear last row in char plane.
    mov di, TEXT_COLS * 2 * (TEXT_ROWS - 1)
    mov cx, TEXT_COLS
    xor ax, ax
    rep stosw

    ; Attr plane.
    mov di, TEXT_ATTR_OFFSET
    mov si, TEXT_ATTR_OFFSET + TEXT_COLS * 2
    mov cx, TEXT_COLS * 2 * (TEXT_ROWS - 1) / 2
    rep movsw

    mov di, TEXT_ATTR_OFFSET + TEXT_COLS * 2 * (TEXT_ROWS - 1)
    mov cx, TEXT_COLS
    mov ax, TEXT_ATTR
    rep stosw

    pop di
    pop si
    pop ds
    pop es
    ret

; ----------------------------------------------------------------------------
; print_newline - emit CRLF via print_char.
; ----------------------------------------------------------------------------
print_newline:
    mov al, 0x0D
    call print_char
    mov al, 0x0A
    call print_char
    ret

; ----------------------------------------------------------------------------
; print_char(AL) - write a character at the cursor position; handle CR/LF/wrap.
; ----------------------------------------------------------------------------
print_char:
    push ax
    push bx
    push cx
    push dx
    push di
    push es

    cmp al, 0x0D
    je .cr
    cmp al, 0x0A
    je .lf

    mov bl, al                      ; preserve char in BL

    xor ax, ax
    mov al, [cursor_row]
    mov dl, TEXT_COLS
    mul dl                          ; AX = row * 80
    xor cx, cx
    mov cl, [cursor_col]
    add ax, cx
    shl ax, 1                       ; byte offset in plane
    mov di, ax

    mov ax, TEXT_VRAM_SEG
    mov es, ax
    mov [es:di], bl
    mov byte [es:di + 1], 0
    mov byte [es:di + TEXT_ATTR_OFFSET], TEXT_ATTR
    mov byte [es:di + TEXT_ATTR_OFFSET + 1], 0

    inc byte [cursor_col]
    cmp byte [cursor_col], TEXT_COLS
    jb .done
    mov byte [cursor_col], 0
    jmp .advance_row

.cr:
    mov byte [cursor_col], 0
    jmp .done

.lf:
.advance_row:
    mov al, [cursor_row]
    inc al
    cmp al, TEXT_ROWS
    jb .row_store
    call scroll_one_line
    mov al, TEXT_ROWS - 1
.row_store:
    mov [cursor_row], al

.done:
    pop es
    pop di
    pop dx
    pop cx
    pop bx
    pop ax
    ret

; ----------------------------------------------------------------------------
; print_string(SI) - print null-terminated string at DS:SI.
; Trashes AX.
; ----------------------------------------------------------------------------
print_string:
    push si
.loop:
    lodsb
    or al, al
    jz .done
    call print_char
    jmp .loop
.done:
    pop si
    ret

; ----------------------------------------------------------------------------
; print_int32_field - print an unsigned 32-bit integer right-aligned in a field.
; Stack on entry: [BP+8] = width (word), [BP+4..7] = value (dword: low at +4).
; ----------------------------------------------------------------------------
print_int32_field:
    push bp
    mov bp, sp
    push ax
    push bx
    push cx
    push dx
    push si

    mov ax, [bp + 4]
    mov [div_num], ax
    mov ax, [bp + 6]
    mov [div_num + 2], ax

    mov si, int_buffer + 11         ; end of buffer; we fill backwards
    mov byte [si], 0

    ; Repeatedly divide div_num by 10, recording the remainder as a digit.
    ; 32/16 division on 8086 is done in two halves: high half first to seed
    ; DX with the remainder, then low half with that DX as the upper word.
    mov cx, 10
.div_loop:
    mov ax, [div_num + 2]
    xor dx, dx
    div cx                          ; AX = quot_high, DX = remainder
    mov [div_num + 2], ax
    mov ax, [div_num]
    div cx                          ; AX = quot_low, DX = digit (0..9)
    mov [div_num], ax
    add dl, '0'
    dec si
    mov [si], dl
    mov ax, [div_num]
    or ax, [div_num + 2]
    jnz .div_loop

    ; Compute used length.
    mov bx, int_buffer + 11
    sub bx, si                      ; BX = digit count
    mov ax, [bp + 8]                ; width
    sub ax, bx                      ; spaces to print
    jle .print_digits
    mov cx, ax
.pad:
    mov al, ' '
    call print_char
    loop .pad
.print_digits:
    call print_string

    pop si
    pop dx
    pop cx
    pop bx
    pop ax
    pop bp
    ret 6

int_buffer: times 12 db 0

; ----------------------------------------------------------------------------
; print_real_field - emit "<int>.<digit>" right-aligned in a field.
; Stack on entry: [BP+10]=width, [BP+6..9]=integer (dword), [BP+4]=decimal digit.
; Uses div_num as scratch for the digit-extraction loop.
; ----------------------------------------------------------------------------
print_real_field:
    push bp
    mov bp, sp
    push ax
    push bx
    push cx
    push dx
    push si

    ; Render "<int>.<digit>" into int_buffer.
    mov si, int_buffer + 11
    mov byte [si], 0
    dec si
    mov al, [bp + 4]
    add al, '0'
    mov [si], al
    dec si
    mov byte [si], '.'

    ; Load 32-bit integer into div_num.
    mov ax, [bp + 6]
    mov [div_num], ax
    mov ax, [bp + 8]
    mov [div_num + 2], ax

    or ax, [div_num]
    jnz .render_int
    dec si
    mov byte [si], '0'
    jmp .pad

.render_int:
    mov cx, 10
.di:
    mov ax, [div_num + 2]
    xor dx, dx
    div cx
    mov [div_num + 2], ax
    mov ax, [div_num]
    div cx
    mov [div_num], ax
    add dl, '0'
    dec si
    mov [si], dl
    mov ax, [div_num]
    or ax, [div_num + 2]
    jnz .di

.pad:
    mov bx, int_buffer + 11
    sub bx, si
    mov ax, [bp + 10]
    sub ax, bx
    jle .print_real
    mov cx, ax
.padloop:
    mov al, ' '
    call print_char
    loop .padloop

.print_real:
    call print_string

    pop si
    pop dx
    pop cx
    pop bx
    pop ax
    pop bp
    ret 8

; ----------------------------------------------------------------------------
; mul32_16 - div_num := div_num * BX (32-bit truncated unsigned multiply).
; The high word of the (lo*BX) partial product is added to the low word of
; the (hi*BX) partial product; the high word of (hi*BX) is discarded since
; the result is intentionally truncated to 32 bits.
; In : div_num (32-bit), BX (16-bit multiplier).
; Out: div_num updated. Trashes AX, CX, DX.
; ----------------------------------------------------------------------------
mul32_16:
    mov ax, [div_num + 2]
    mul bx                          ; DX:AX = hi * BX (DX discarded)
    mov cx, ax                      ; CX = (hi * BX) low word
    mov ax, [div_num]
    mul bx                          ; DX:AX = lo * BX
    add dx, cx                      ; DX = high word of full product
    mov [div_num], ax
    mov [div_num + 2], dx
    ret

; ----------------------------------------------------------------------------
; udiv32_32 - div_quot, div_rem := div_num / div_den, div_num mod div_den.
; Restoring long division, 32 iterations. div_num is consumed (shifted out).
; Caller must ensure div_den != 0.
; Trashes AX, CX. div_num is destroyed; div_den is preserved.
; ----------------------------------------------------------------------------
udiv32_32:
    push cx
    xor ax, ax
    mov [div_quot],     ax
    mov [div_quot + 2], ax
    mov [div_rem],      ax
    mov [div_rem + 2],  ax

    mov cx, 32
.loop:
    ; Q <<= 1
    shl word [div_quot],     1
    rcl word [div_quot + 2], 1
    ; Shift N left, capturing old MSB into CF.
    shl word [div_num],      1
    rcl word [div_num + 2],  1
    ; R <<= 1, bringing CF (old MSB of N) into bit 0.
    rcl word [div_rem],      1
    rcl word [div_rem + 2],  1

    ; If R >= D (unsigned 32-bit), subtract D from R and set Q's bit 0.
    mov ax, [div_rem + 2]
    cmp ax, [div_den + 2]
    ja .ge
    jb .next
    mov ax, [div_rem]
    cmp ax, [div_den]
    jb .next
.ge:
    mov ax, [div_rem]
    sub ax, [div_den]
    mov [div_rem], ax
    mov ax, [div_rem + 2]
    sbb ax, [div_den + 2]
    mov [div_rem + 2], ax
    or word [div_quot], 1
.next:
    loop .loop
    pop cx
    ret

; ----------------------------------------------------------------------------
; get_clock -> DX:AX : full 32-bit tick counter (interrupt-safe read).
; ----------------------------------------------------------------------------
get_clock:
    cli
    mov ax, [tick_count]
    mov dx, [tick_count + 2]
    sti
    ret

; ============================================================================
; INT 8 (timer tick) hook - increment 32-bit tick_count, chain to old handler.
; ============================================================================

int8_handler:
    ; We deliberately do NOT chain to the BIOS stub: the BIOS stub assumes
    ; the IRET frame layout that comes from a CD-CD-CD INT instruction, and
    ; chaining via `jmp far` after our own `push ax` would have an extra word
    ; under the SP that the HLE side would mistakenly pop as part of the
    ; saved AX/DX restore. PIT channel 0 was programmed in mode 3 (auto-reload)
    ; by INT 1Ch AH=02h, so we just bump our counter, EOI, and IRET.
    push ax
    add word [cs:tick_count], 1
    adc word [cs:tick_count + 2], 0
    cmp byte [cs:benchmark_active], 0
    je .eoi
    sub word [cs:benchmark_ticks_remaining], 1
    jnz .eoi
    mov byte [cs:benchmark_active], 0
    mov byte [cs:benchmark_done], 1
.eoi:
    mov al, 0x20                    ; non-specific EOI
    out 0x00, al
    pop ax
    iret

; ----------------------------------------------------------------------------
; dummy_timer_callback - INT 1Ch AH=02h fires this when its countdown hits 0.
; We don't actually want it to do anything; we use INT 1Ch only to start the
; periodic 100 Hz interrupt. The callback runs ~11 minutes in if it ever fires.
; ----------------------------------------------------------------------------
dummy_timer_callback:
    iret

; ============================================================================
; Initialized data
; ============================================================================

; Dhrystone strings (exactly 30 chars each).
str_first_string:    db "DHRYSTONE PROGRAM, 1'ST STRING"
str_second_string:   db "DHRYSTONE PROGRAM, 2'ND STRING"
str_third_string:    db "DHRYSTONE PROGRAM, 3'RD STRING"
str_some_string:     db "DHRYSTONE PROGRAM, SOME STRING"

; Banner / prompts / status (CRLF terminated where the Pascal prints newlines).
str_banner:          db 0x0D, 0x0A, "Dhrystone Benchmark, Version 2.1 (NASM port)", 0x0D, 0x0A, 0x0D, 0x0A, 0
str_exec_starts:     db "Execution starts, benchmark will run for 10 seconds", 0x0D, 0x0A, 0
str_exec_ends:       db "Execution ends", 0x0D, 0x0A, 0x0D, 0x0A, 0

str_self_check_ok:     db "Self check: OK", 0x0D, 0x0A, 0x0D, 0x0A, 0
str_self_check_failed: db "Self check: FAILED", 0x0D, 0x0A, 0x0D, 0x0A, 0

str_seconds:         db "Measured runtime in Seconds:                ", 0
str_us_per_run:      db "Microseconds for one run through Dhrystone: ", 0
str_drys_per_sec:    db "Dhrystones per Second:                      ", 0
str_dmips:           db "DMIPS:                                      ", 0

; Standard 16-entry analog palette (3 bytes per entry: green, red, blue).
analog_palette_table:
    db 0x00, 0x00, 0x00         ; 0: black
    db 0x00, 0x00, 0x07         ; 1: dim blue
    db 0x00, 0x07, 0x00         ; 2: dim red
    db 0x00, 0x07, 0x07         ; 3: dim magenta
    db 0x07, 0x00, 0x00         ; 4: dim green
    db 0x07, 0x00, 0x07         ; 5: dim cyan
    db 0x07, 0x07, 0x00         ; 6: dim yellow
    db 0x07, 0x07, 0x07         ; 7: dim white
    db 0x04, 0x04, 0x04         ; 8: half-bright grey
    db 0x00, 0x00, 0x0F         ; 9: bright blue
    db 0x00, 0x0F, 0x00         ; 10: bright red
    db 0x00, 0x0F, 0x0F         ; 11: bright magenta
    db 0x0F, 0x00, 0x00         ; 12: bright green
    db 0x0F, 0x00, 0x0F         ; 13: bright cyan
    db 0x0F, 0x0F, 0x00         ; 14: bright yellow
    db 0x0F, 0x0F, 0x0F         ; 15: bright white

; Pad the binary to TOTAL_SECTORS * SECTOR_SIZE so the IPL load size matches.
times TOTAL_SECTORS * SECTOR_SIZE - ($ - $$) db 0

; ============================================================================
; BSS - declared via `absolute` so NASM does NOT emit zero bytes in the
; -f bin output and the labels resolve to addresses past the loaded code.
; (`section .bss vstart=...` does not work in -f bin: vstart leaves the
; displacements unchanged, so the BSS labels would alias back into .text.)
; ============================================================================

absolute TOTAL_SECTORS * SECTOR_SIZE

PointerGlob:             resw 1
NextPointerGlob:         resw 1
PointerGlob_storage:     resb REC_SIZE
NextPointerGlob_storage: resb REC_SIZE

Int1Glob:        resw 1
Int2Glob:        resw 1
Int3Glob:        resw 1
IntGlob:         resw 1
BoolGlob:        resb 1
Char1Glob:       resb 1
Char2Glob:       resb 1
CharIndex:       resb 1
EnumGlob:        resb 1
String1Glob:     resb 30
String2Glob:     resb 30
Array1Glob:      resw 50
Array2Glob:      resw 2500           ; 50 * 50

NumberOfRuns:    resd 1
BeginClock:      resd 1
EndClock:        resd 1
SumClocks:       resd 1
DhrystonesPerSecondTimes10: resd 1

div_num:         resd 1
div_den:         resd 1
div_quot:        resd 1
div_rem:         resd 1

tick_count:      resd 1
benchmark_ticks_remaining: resw 1
benchmark_active: resb 1
benchmark_done:  resb 1
old_int8_off:    resw 1
old_int8_seg:    resw 1

cursor_row:      resb 1
cursor_col:      resb 1
