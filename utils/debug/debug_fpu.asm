; debug_fpu.asm — x87 FPU test ROM for Neetan with ULP-based precision verification
; Assembles to a 192KB dual-bank ROM image for RA/AP/AS machines.
;   Bank 0 (first 96KB, file offset 0x00000): F8000-FFFFF — reset vector only
;   Bank 1 (second 96KB, file offset 0x18000): E8000-F7FFF — all code and data
; Cycles through 3 test pages with Enter key:
;   Page 1: FPU constants (FLDPI, FLD1, FLDZ, FLDL2T, FLDL2E, FLDLG2, FLDLN2)
;   Page 2: Basic arithmetic (FADD, FSUB, FMUL, FDIV)
;   Page 3: Transcendentals (FSQRT, FSIN, FCOS, FPTAN, FPATAN, F2XM1, FYL2X, FYL2XP1, FSCALE, Machin)

[bits 16]
[cpu 386]

ROM_SEGMENT                 equ 0xE800
TEXT_VRAM                   equ 0xA000
TOTAL_PAGE_COUNT            equ 4

VAR_CURRENT_PAGE            equ 0x0500

TEMP_CW                     equ 0x0510
TEMP_CW_TRUNC               equ 0x0512
TEMP_INT                    equ 0x0514
TEMP_DIGIT                  equ 0x0518

COMPUTED_BUF                equ 0x0520

LABEL_COL                   equ 0
VALUE_COL                   equ 20
ULP_COL                     equ 42
STATUS_COL                  equ 53

; ============================================================================
; Bank 0 — mapped to F8000-FFFFF. Contains only the reset vector.
; ============================================================================
section bank0 start=0 vstart=0

    times (0x18000 - 16) db 0xFF

; Reset vector at physical 0xFFFF0 — jumps to bank 1 entry at E800:0000
    db 0xEA             ; far jmp opcode
    dw 0x0000           ; IP = 0 (start of bank 1 code)
    dw ROM_SEGMENT      ; CS = E800
    times 0x18000 - ($ - $$) db 0xFF

; ============================================================================
; Bank 1 — always mapped to E8000-F7FFF. Contains all executable code.
; ============================================================================
section bank1 start=0x18000 vstart=0

; Entry point (jumped to from reset vector in bank 0)
entry:
    cli

    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00

    mov ax, ROM_SEGMENT
    mov ds, ax

    ; Enable analog palette mode.
    mov al, 0x01
    out 0x6A, al

    ; Select 8x16 text font.
    mov al, 0x07
    out 0x68, al

    ; Force 80-column mode.
    mov al, 0x04
    out 0x68, al

    ; Keep KAC in code-access mode.
    mov al, 0x0A
    out 0x68, al

    ; Ensure global display enable.
    mov al, 0x0F
    out 0x68, al

    ; Start GDC slave (graphics).
    mov al, 0x6B
    out 0xA2, al

    ; Start GDC master (text).
    mov al, 0x6B
    out 0x62, al

    ; Initialize the x87 FPU.
    fninit

    mov byte [ss:VAR_CURRENT_PAGE], 0

.main_loop:
    call render_current_page
    call wait_enter

    mov al, [ss:VAR_CURRENT_PAGE]
    inc al
    cmp al, TOTAL_PAGE_COUNT
    jb .store_page
    xor al, al

.store_page:
    mov [ss:VAR_CURRENT_PAGE], al
    jmp .main_loop

render_current_page:
    mov al, [ss:VAR_CURRENT_PAGE]
    cmp al, 0
    je render_page_constants
    cmp al, 1
    je render_page_arithmetic
    cmp al, 2
    je render_page_transcendentals
    cmp al, 3
    je render_page_golden
    ret

; ============================================================================
; verify_and_display — Core ULP verification routine
; Input:  ST(0) = computed value, BH = row, DS:SI = ptr to 14-byte entry
;         (10b expected Fp80 + 4b ULP threshold dword)
; Output: Displays ftoa value, ULP distance, and OK/FAIL. ST(0) consumed.
; ============================================================================
verify_and_display:
    push eax
    push ebx
    push ecx
    push edx
    push si

    ; Step 1: Store ST(0) as raw 10-byte tword.
    fstp tword [ss:COMPUTED_BUF]
    fwait

    ; Step 2: Display Fp80 hex at (row, VALUE_COL): "XXXX XXXXXXXXXXXXXXXX".
    mov bl, VALUE_COL
    call set_cursor

    ; Sign+exponent (bytes 9,8 = MSB first).
    mov al, [ss:COMPUTED_BUF + 9]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 8]
    call write_hex_byte

    mov al, ' '
    call write_char

    ; Significand (bytes 7,6,...,0 = MSB first).
    mov al, [ss:COMPUTED_BUF + 7]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 6]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 5]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 4]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 3]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 2]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 1]
    call write_hex_byte
    mov al, [ss:COMPUTED_BUF + 0]
    call write_hex_byte

    ; Step 4: Check if both are zero (special case — sign/exp may differ for +0 vs -0).
    ; If expected significand and computed significand are both 0, it's a match.
    mov eax, [ss:COMPUTED_BUF]
    or eax, [ss:COMPUTED_BUF + 4]
    jnz .not_both_zero
    mov eax, [ds:si]
    or eax, [ds:si + 4]
    jnz .not_both_zero
    ; Both significands are zero — ULP = 0.
    xor eax, eax
    jmp .display_ulp

.not_both_zero:
    ; Step 5: Compare sign_exponent fields.
    mov ax, [ss:COMPUTED_BUF + 8]
    cmp ax, [ds:si + 8]
    jne .exp_mismatch

    ; Step 6: 64-bit unsigned subtraction of significands.
    mov eax, [ss:COMPUTED_BUF]
    sub eax, [ds:si]
    mov edx, [ss:COMPUTED_BUF + 4]
    sbb edx, [ds:si + 4]

    ; If borrow (CF was set after SBB, meaning result is negative), negate.
    jnc .no_negate
    not eax
    not edx
    add eax, 1
    adc edx, 0

.no_negate:
    ; If high dword != 0, ULP > 2^32 — force fail.
    or edx, edx
    jnz .exp_mismatch

    ; EAX = ULP distance.
    jmp .display_ulp

.exp_mismatch:
    mov eax, 0xFFFFFFFF

.display_ulp:
    ; Save ULP in ECX for later comparison.
    mov ecx, eax

    ; Step 7: Display ULP distance at (row, ULP_COL).
    mov bl, ULP_COL
    call set_cursor
    call write_uint32

    ; Step 8: Load threshold from [SI+10].
    mov eax, [ds:si + 10]

    ; Step 9: Compare ULP (ECX) <= threshold (EAX).
    mov bl, STATUS_COL
    call set_cursor
    cmp ecx, eax
    ja .fail

    ; OK: write "OK" in green.
    mov al, 'O'
    call write_char_green
    mov al, 'K'
    call write_char_green
    jmp .verify_done

.fail:
    ; FAIL: write "FAIL" in red.
    mov al, 'F'
    call write_char_red
    mov al, 'A'
    call write_char_red
    mov al, 'I'
    call write_char_red
    mov al, 'L'
    call write_char_red

.verify_done:
    pop si
    pop edx
    pop ecx
    pop ebx
    pop eax
    ret

; ============================================================================
; Page 1: FPU Constants
; ============================================================================
render_page_constants:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, str_header_1
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, str_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; FLDPI
    mov si, str_fldpi
    mov bh, 3
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    mov bh, 3
    mov si, exp_fldpi
    call verify_and_display

    ; FLD1
    mov si, str_fld1
    mov bh, 5
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld1
    mov bh, 5
    mov si, exp_fld1
    call verify_and_display

    ; FLDZ
    mov si, str_fldz
    mov bh, 7
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldz
    mov bh, 7
    mov si, exp_fldz
    call verify_and_display

    ; FLDL2T
    mov si, str_fldl2t
    mov bh, 9
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldl2t
    mov bh, 9
    mov si, exp_fldl2t
    call verify_and_display

    ; FLDL2E
    mov si, str_fldl2e
    mov bh, 11
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldl2e
    mov bh, 11
    mov si, exp_fldl2e
    call verify_and_display

    ; FLDLG2
    mov si, str_fldlg2
    mov bh, 13
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldlg2
    mov bh, 13
    mov si, exp_fldlg2
    call verify_and_display

    ; FLDLN2
    mov si, str_fldln2
    mov bh, 15
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldln2
    mov bh, 15
    mov si, exp_fldln2
    call verify_and_display

    ret

; ============================================================================
; Page 2: Basic Arithmetic
; ============================================================================
render_page_arithmetic:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, str_header_2
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, str_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; 3 + 4 = 7
    mov si, str_add
    mov bh, 3
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_3]
    fld qword [const_4]
    faddp st1, st0
    mov bh, 3
    mov si, exp_add
    call verify_and_display

    ; 10 - 3 = 7
    mov si, str_sub
    mov bh, 5
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_10]
    fld qword [const_3]
    fsubp st1, st0
    mov bh, 5
    mov si, exp_sub
    call verify_and_display

    ; 6 * 7 = 42
    mov si, str_mul
    mov bh, 7
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_6]
    fld qword [const_7]
    fmulp st1, st0
    mov bh, 7
    mov si, exp_mul
    call verify_and_display

    ; 355 / 113
    mov si, str_div
    mov bh, 9
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_355]
    fld qword [const_113]
    fdivp st1, st0
    mov bh, 9
    mov si, exp_div
    call verify_and_display

    ; -5 + 5 = 0
    mov si, str_neg
    mov bh, 11
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_neg5]
    fld qword [const_5]
    faddp st1, st0
    mov bh, 11
    mov si, exp_negadd
    call verify_and_display

    ret

; ============================================================================
; Page 3: Transcendentals (all 10 tests)
; ============================================================================
render_page_transcendentals:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, str_header_3
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, str_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; SQRT(2)
    mov si, str_sqrt2
    mov bh, 3
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_2]
    fsqrt
    mov bh, 3
    mov si, exp_sqrt2
    call verify_and_display

    ; SIN(PI/6)
    mov si, str_sin
    mov bh, 5
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fdiv qword [const_6]
    fsin
    mov bh, 5
    mov si, exp_sin_pi6
    call verify_and_display

    ; COS(PI/3)
    mov si, str_cos
    mov bh, 7
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fdiv qword [const_3]
    fcos
    mov bh, 7
    mov si, exp_cos_pi3
    call verify_and_display

    ; TAN(PI/4)
    mov si, str_tan
    mov bh, 9
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fdiv qword [const_4]
    fptan
    fstp st0
    mov bh, 9
    mov si, exp_tan_pi4
    call verify_and_display

    ; 4*ATAN(1) = PI
    mov si, str_atan
    mov bh, 11
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld1
    fld1
    fpatan
    fmul qword [const_4]
    mov bh, 11
    mov si, exp_4atan1
    call verify_and_display

    ; F2XM1(0.5) = 2^0.5 - 1
    mov si, str_f2xm1
    mov bh, 13
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_half]
    f2xm1
    mov bh, 13
    mov si, exp_f2xm1
    call verify_and_display

    ; FYL2X(1, 8) = log2(8) = 3
    mov si, str_fyl2x
    mov bh, 15
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld1
    fld qword [const_8]
    fyl2x
    mov bh, 15
    mov si, exp_fyl2x
    call verify_and_display

    ; FYL2XP1(2, 0.25) = 2 * log2(1.25)
    mov si, str_fyl2xp1
    mov bh, 17
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_2]
    fld qword [const_quarter]
    fyl2xp1
    mov bh, 17
    mov si, exp_fyl2xp1
    call verify_and_display

    ; FSCALE(1.5, 2) = 1.5 * 2^2 = 6
    mov si, str_fscale
    mov bh, 19
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld qword [const_2]
    fld qword [const_1_5]
    fscale
    fstp st1
    mov bh, 19
    mov si, exp_fscale
    call verify_and_display

    ; Machin PI = 16*atan(1/5) - 4*atan(1/239)
    mov si, str_machin
    mov bh, 21
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld1
    fld qword [const_5]
    fpatan
    fmul qword [const_16]
    fld1
    fld qword [const_239]
    fpatan
    fmul qword [const_4]
    fsubp st1, st0
    mov bh, 21
    mov si, exp_machin
    call verify_and_display

    ret

; ============================================================================
; Page 4: x87 Trig Quirks (golden vectors from real hardware)
; These verify the rounded-period behavior: FSIN/FCOS/FPTAN compute
; sin(x*pi/p) rather than sin(x), where p is the 66-bit approximation of pi.
; ============================================================================
render_page_golden:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, str_header_4
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, str_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; FSIN(PI) = -2^-64, not 0 — maximum cancellation from 66-bit p
    mov si, str_golden_fsin_pi
    mov bh, 3
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fsin
    mov bh, 3
    mov si, exp_golden_fsin_pi
    call verify_and_display

    ; FSIN(-PI) = 2^-64, not 0
    mov si, str_golden_fsin_negpi
    mov bh, 5
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fchs
    fsin
    mov bh, 5
    mov si, exp_golden_fsin_negpi
    call verify_and_display

    ; FCOS(PI/2) = -2^-65, not 0
    mov si, str_golden_fcos_pi2
    mov bh, 7
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fdiv qword [const_2]
    fcos
    mov bh, 7
    mov si, exp_golden_fcos_pi2
    call verify_and_display

    ; FSIN(2*PI) = 2^-63, not 0
    mov si, str_golden_fsin_2pi
    mov bh, 9
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fmul qword [const_2]
    fsin
    mov bh, 9
    mov si, exp_golden_fsin_2pi
    call verify_and_display

    ; FSIN(3*PI) = -7*2^-64, not 0
    mov si, str_golden_fsin_3pi
    mov bh, 11
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fld tword [const_3pi]
    fsin
    mov bh, 11
    mov si, exp_golden_fsin_3pi
    call verify_and_display

    ; FCOS(PI) = -1.0 exactly (reduction lands on exact quadrant boundary)
    mov si, str_golden_fcos_pi
    mov bh, 13
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fcos
    mov bh, 13
    mov si, exp_golden_fcos_pi
    call verify_and_display

    ; FSIN(PI/2) = 1.0 exactly (reduction lands on exact quadrant boundary)
    mov si, str_golden_fsin_pi2
    mov bh, 15
    mov bl, LABEL_COL
    mov ah, 0xE1
    call write_string_at
    fldpi
    fdiv qword [const_2]
    fsin
    mov bh, 15
    mov si, exp_golden_fsin_pi2
    call verify_and_display

    ret

; ============================================================================
; ftoa — Convert ST(0) to decimal ASCII string at ES:DI.
; Input:  ST(0) = value to convert, ES:DI = VRAM cursor position
; Output: DI advanced past the written string. ST(0) consumed (popped).
; ============================================================================
ftoa:
    push eax
    push ecx
    push edx

    ; Save FPU control word.
    fnstcw [ss:TEMP_CW]
    fwait
    mov ax, [ss:TEMP_CW]
    or ax, 0x0C00
    mov [ss:TEMP_CW_TRUNC], ax

    ; Check sign via FTST.
    ftst
    fnstsw ax
    test ah, 0x01
    jz .positive

    mov al, '-'
    call write_char
    fabs

.positive:
    ; Duplicate value to extract integer part.
    fld st0
    fldcw [ss:TEMP_CW_TRUNC]
    fistp dword [ss:TEMP_INT]
    fldcw [ss:TEMP_CW]

    ; Compute fraction = value - integer part.
    fild dword [ss:TEMP_INT]
    fsubp st1, st0
    fabs

    ; Write integer part as decimal.
    mov eax, [ss:TEMP_INT]
    call write_uint32

    ; Write decimal point.
    mov al, '.'
    call write_char

    ; Extract 15 fractional digits.
    mov ecx, 15

.digit_loop:
    fmul qword [const_10]

    fld st0
    fldcw [ss:TEMP_CW_TRUNC]
    fistp word [ss:TEMP_DIGIT]
    fldcw [ss:TEMP_CW]

    ; Clamp digit to 0-9.
    mov ax, [ss:TEMP_DIGIT]
    cmp ax, 9
    jbe .clamp_low
    mov ax, 9
.clamp_low:
    cmp ax, 0
    jge .clamp_done
    xor ax, ax
.clamp_done:

    ; Subtract extracted digit from fraction.
    mov [ss:TEMP_DIGIT], ax
    fild word [ss:TEMP_DIGIT]
    fsubp st1, st0
    fabs

    ; Write digit character.
    mov al, [ss:TEMP_DIGIT]
    add al, '0'
    call write_char

    dec ecx
    jnz .digit_loop

    ; Discard remaining fraction.
    fstp st0

    pop edx
    pop ecx
    pop eax
    ret

; ============================================================================
; write_char — Write a single ASCII character at ES:DI with white attribute.
; Input:  AL = character, ES:DI = VRAM position
; Output: DI += 2
; ============================================================================
write_char:
    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov byte [es:di + 0x2000], 0xE1
    mov byte [es:di + 0x2001], 0x00
    add di, 2
    ret

; ============================================================================
; write_char_green — Write a single ASCII character at ES:DI with green attribute (0x81).
; ============================================================================
write_char_green:
    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov byte [es:di + 0x2000], 0x81
    mov byte [es:di + 0x2001], 0x00
    add di, 2
    ret

; ============================================================================
; write_char_red — Write a single ASCII character at ES:DI with red attribute (0x41).
; ============================================================================
write_char_red:
    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov byte [es:di + 0x2000], 0x41
    mov byte [es:di + 0x2001], 0x00
    add di, 2
    ret

; ============================================================================
; write_uint32 — Write an unsigned 32-bit integer as decimal at ES:DI.
; Input:  EAX = value, ES:DI = VRAM position
; Output: DI advanced past digits
; ============================================================================
write_uint32:
    push eax
    push ebx
    push ecx
    push edx

    xor ecx, ecx
    mov ebx, 10

    or eax, eax
    jnz .divide_loop

    mov al, '0'
    call write_char
    jmp .done

.divide_loop:
    xor edx, edx
    div ebx
    push dx
    inc ecx
    or eax, eax
    jnz .divide_loop

.write_digits:
    pop ax
    add al, '0'
    call write_char
    dec ecx
    jnz .write_digits

.done:
    pop edx
    pop ecx
    pop ebx
    pop eax
    ret

; ============================================================================
; write_hex_byte — Write a byte as two hex digits at ES:DI.
; Input:  AL = byte value, ES:DI = VRAM position
; Output: DI += 4 (two characters written)
; ============================================================================
write_hex_byte:
    push ax
    push cx
    mov cl, al
    shr al, 4
    call .write_nibble
    mov al, cl
    and al, 0x0F
    call .write_nibble
    pop cx
    pop ax
    ret

.write_nibble:
    cmp al, 10
    jb .decimal
    add al, 'A' - 10
    jmp .emit
.decimal:
    add al, '0'
.emit:
    call write_char
    ret

; ============================================================================
; set_cursor — Compute DI from row/column.
; Input:  BH = row, BL = column
; Output: DI = (row * 80 + col) * 2
; ============================================================================
set_cursor:
    push ax
    push cx

    xor ax, ax
    mov al, bh
    mov cx, ax
    shl ax, 6
    shl cx, 4
    add ax, cx

    xor cx, cx
    mov cl, bl
    add ax, cx
    shl ax, 1
    mov di, ax

    pop cx
    pop ax
    ret

; ============================================================================
; write_string_at — Write a null-terminated ASCII string to text VRAM.
; Input:  ES = TEXT_VRAM, DS:SI = string, BH = row, BL = column, AH = attribute
; ============================================================================
write_string_at:
    push ax
    push bx
    push cx
    push dx
    push di

    mov dl, ah

    xor ax, ax
    mov al, bh
    mov cx, ax
    shl ax, 6
    shl cx, 4
    add ax, cx

    xor cx, cx
    mov cl, bl
    add ax, cx
    shl ax, 1
    mov di, ax

.write_loop:
    lodsb
    or al, al
    jz .done

    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov [es:di + 0x2000], dl
    mov byte [es:di + 0x2001], 0x00

    add di, 2
    jmp .write_loop

.done:
    pop di
    pop dx
    pop cx
    pop bx
    pop ax
    ret

clear_text_vram:
    push ax
    push cx
    push di

    mov ax, TEXT_VRAM
    mov es, ax

    xor di, di
    mov ax, 0x0020
    mov cx, 80 * 25
    rep stosw

    mov di, 0x2000
    mov ax, 0x00E1
    mov cx, 80 * 25
    rep stosw

    pop di
    pop cx
    pop ax
    ret

wait_enter:
.wait_make:
    in al, 0x43
    test al, 0x02
    jz .wait_make
    in al, 0x41
    cmp al, 0x1C
    jne .wait_make

.wait_break:
    in al, 0x43
    test al, 0x02
    jz .wait_break
    in al, 0x41

    ret

; String constants.
str_header_1:   db 'DEBUG_FPU 1/4 FPU CONSTANTS', 0
str_header_2:   db 'DEBUG_FPU 2/4 BASIC ARITHMETIC', 0
str_header_3:   db 'DEBUG_FPU 3/4 TRANSCENDENTALS', 0
str_header_4:   db 'DEBUG_FPU 4/4 X87 TRIG QUIRKS', 0

str_hint:       db 'ENTER: NEXT PAGE', 0

str_fldpi:      db 'FLDPI  PI       = ', 0
str_fld1:       db 'FLD1   1.0      = ', 0
str_fldz:       db 'FLDZ   0.0      = ', 0
str_fldl2t:     db 'FLDL2T LOG2(10) = ', 0
str_fldl2e:     db 'FLDL2E LOG2(E)  = ', 0
str_fldlg2:     db 'FLDLG2 LOG10(2) = ', 0
str_fldln2:     db 'FLDLN2 LN(2)    = ', 0

str_add:        db '3 + 4           = ', 0
str_sub:        db '10 - 3          = ', 0
str_mul:        db '6 * 7           = ', 0
str_div:        db '355 / 113       = ', 0
str_neg:        db '-5 + 5          = ', 0

str_sqrt2:      db 'SQRT(2)         = ', 0
str_sin:        db 'SIN(PI/6)       = ', 0
str_cos:        db 'COS(PI/3)       = ', 0
str_tan:        db 'TAN(PI/4)       = ', 0
str_atan:       db '4*ATAN(1)       = ', 0

str_f2xm1:      db 'F2XM1(0.5)      = ', 0
str_fyl2x:      db 'FYL2X(1,8)      = ', 0
str_fyl2xp1:    db 'FYL2XP1(2,0.25) = ', 0
str_fscale:     db 'FSCALE(1.5,2)   = ', 0
str_machin:     db 'MACHIN PI       = ', 0

str_golden_fsin_pi:     db 'FSIN(PI)        = ', 0
str_golden_fsin_negpi:  db 'FSIN(-PI)       = ', 0
str_golden_fcos_pi2:    db 'FCOS(PI/2)      = ', 0
str_golden_fsin_2pi:    db 'FSIN(2*PI)      = ', 0
str_golden_fsin_3pi:    db 'FSIN(3*PI)      = ', 0
str_golden_fcos_pi:     db 'FCOS(PI)        = ', 0
str_golden_fsin_pi2:    db 'FSIN(PI/2)      = ', 0

; Floating-point constants (IEEE 754 double-precision).
const_quarter:  dq 0.25
const_half:     dq 0.5
const_1_5:      dq 1.5
const_2:        dq 2.0
const_3:        dq 3.0
const_4:        dq 4.0
const_5:        dq 5.0
const_6:        dq 6.0
const_7:        dq 7.0
const_8:        dq 8.0
const_10:       dq 10.0
const_16:       dq 16.0
const_113:      dq 113.0
const_239:      dq 239.0
const_355:      dq 355.0
const_neg5:     dq -5.0

; 3*PI as Fp80 tword (0x4002_96CBE3F9990E91A8) — matches x87_golden.rs test input.
const_3pi:      db 0xA8, 0x91, 0x0E, 0x99, 0xF9, 0xE3, 0xCB, 0x96, 0x02, 0x40

; Expected Fp80 bit patterns for ULP verification.
; Each entry: 10 bytes Fp80 (8 significand LE + 2 sign_exp LE), then dd ULP threshold.
; Constants and arithmetic values are exact hardware bit patterns or correctly-rounded
; results computed with Python3 mpmath (mp.prec=256) and round-to-nearest-even to Fp80.
; Transcendental thresholds are measured against the softfloat implementation.
; To regenerate: use mpmath's mpf_to_fp80_rne() at 256-bit precision, then pack as
; struct.pack("<QH", significand, sign_exponent).

exp_fldpi:
    db 0x35, 0xC2, 0x68, 0x21, 0xA2, 0xDA, 0x0F, 0xC9, 0x00, 0x40  ; PI NearestEven
    dd 0

exp_fld1:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFF, 0x3F  ; 1.0 exact
    dd 0

exp_fldz:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00  ; 0.0 exact
    dd 0

exp_fldl2t:
    db 0xFE, 0x8A, 0x1B, 0xCD, 0x4B, 0x78, 0x9A, 0xD4, 0x00, 0x40  ; LOG2(10) NearestEven
    dd 0

exp_fldl2e:
    db 0xBC, 0xF0, 0x17, 0x5C, 0x29, 0x3B, 0xAA, 0xB8, 0xFF, 0x3F  ; LOG2(E) NearestEven
    dd 0

exp_fldlg2:
    db 0x99, 0xF7, 0xCF, 0xFB, 0x84, 0x9A, 0x20, 0x9A, 0xFD, 0x3F  ; LOG10(2) NearestEven
    dd 0

exp_fldln2:
    db 0xAC, 0x79, 0xCF, 0xD1, 0xF7, 0x17, 0x72, 0xB1, 0xFE, 0x3F  ; LN(2) NearestEven
    dd 0

exp_add:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x01, 0x40  ; 3 + 4 = 7 exact
    dd 0

exp_sub:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x01, 0x40  ; 10 - 3 = 7 exact
    dd 0

exp_mul:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA8, 0x04, 0x40  ; 6 * 7 = 42 exact
    dd 0

exp_div:
    db 0x09, 0xBC, 0xFD, 0x90, 0xC0, 0xDB, 0x0F, 0xC9, 0x00, 0x40  ; 355 / 113 correctly rounded
    dd 0

exp_negadd:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00  ; -5 + 5 = +0.0 exact
    dd 0

exp_sqrt2:
    db 0x84, 0x64, 0xDE, 0xF9, 0x33, 0xF3, 0x04, 0xB5, 0xFF, 0x3F  ; sqrt(2) correctly rounded
    dd 0

exp_sin_pi6:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFE, 0x3F  ; sin(pi/6) ~ 0.5
    dd 0

exp_cos_pi3:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFE, 0x3F  ; cos(pi/3) ~ 0.5
    dd 0

exp_tan_pi4:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFF, 0x3F  ; tan(pi/4) ~ 1.0
    dd 0

exp_4atan1:
    db 0x35, 0xC2, 0x68, 0x21, 0xA2, 0xDA, 0x0F, 0xC9, 0x00, 0x40  ; 4*atan(1) ~ pi
    dd 0

exp_f2xm1:
    db 0x11, 0x92, 0x79, 0xE7, 0xCF, 0xCC, 0x13, 0xD4, 0xFD, 0x3F  ; 2^0.5 - 1
    dd 0

exp_fyl2x:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x40  ; 1 * log2(8) = 3 exact
    dd 0

exp_fyl2xp1:
    db 0xF2, 0x57, 0xDC, 0x68, 0x5E, 0xC2, 0xD3, 0xA4, 0xFE, 0x3F  ; 2 * log2(1.25)
    dd 0

exp_fscale:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x01, 0x40  ; 1.5 * 2^2 = 6 exact
    dd 0

exp_machin:
    db 0x35, 0xC2, 0x68, 0x21, 0xA2, 0xDA, 0x0F, 0xC9, 0x00, 0x40  ; Machin pi
    dd 0

; Golden vectors from real x87 hardware (Intel, x86_64 Linux).
; These verify the rounded-period trig behavior: sin(x*pi/p), not sin(x).

exp_golden_fsin_pi:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xBF, 0xBF  ; FSIN(PI) = -2^-64, not 0
    dd 1

exp_golden_fsin_negpi:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xBF, 0x3F  ; FSIN(-PI) = 2^-64, not 0
    dd 1

exp_golden_fcos_pi2:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xBE, 0xBF  ; FCOS(PI/2) = -2^-65, not 0
    dd 1

exp_golden_fsin_2pi:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xC0, 0x3F  ; FSIN(2*PI) = 2^-63, not 0
    dd 1

exp_golden_fsin_3pi:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0xC1, 0xBF  ; FSIN(3*PI) = -7*2^-64, not 0
    dd 1

exp_golden_fcos_pi:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFF, 0xBF  ; FCOS(PI) = -1.0 exactly
    dd 1

exp_golden_fsin_pi2:
    db 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xFF, 0x3F  ; FSIN(PI/2) = 1.0 exactly
    dd 1

; Pad bank 1 to exactly 96KB
    times 0x18000 - ($ - $$) db 0xFF
