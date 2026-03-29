; debug_egc.asm - EGC test ROM for Neetan
; Assembles to a 192KB dual-bank ROM image for VX/RA machines.
;   Bank 0 (first 96KB, file offset 0x00000): F8000-FFFFF — reset vector only
;   Bank 1 (second 96KB, file offset 0x18000): E8000-F7FFF — all code and data
; Cycles through 4 fullscreen test patterns with Enter key.

[bits 16]
[cpu 186]

ROM_SEGMENT     equ 0xE800

; VRAM plane segments
VRAM_B          equ 0xA800
VRAM_R          equ 0xB000
VRAM_G          equ 0xB800
VRAM_E          equ 0xE000

; Text VRAM
TEXT_VRAM       equ 0xA000

; Screen dimensions
SCREEN_WIDTH    equ 640
SCREEN_HEIGHT   equ 400
BYTES_PER_LINE  equ 80
PLANE_SIZE      equ BYTES_PER_LINE * SCREEN_HEIGHT  ; 32000 bytes

; EGC register ports
EGC_ACCESS      equ 0x04A0
EGC_FGBG        equ 0x04A2
EGC_OPE         equ 0x04A4
EGC_FG          equ 0x04A6
EGC_BG          equ 0x04AA
EGC_SFT         equ 0x04AC
EGC_LENG        equ 0x04AE

; GRCG port
GRCG_MODE       equ 0x7C

NUM_PATTERNS    equ 4

; Quadrant start offsets
Q_TL            equ 0
Q_TR            equ 40
Q_BL            equ 200 * BYTES_PER_LINE
Q_BR            equ 200 * BYTES_PER_LINE + 40

; Source data base offset (beyond visible VRAM area)
SRC_BASE        equ PLANE_SIZE

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

; ============================================================================
; Entry point (jumped to from reset vector in bank 0)
; ============================================================================
entry:
    cli

    ; Set up stack
    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00

    ; Set DS = CS = ROM segment
    mov ax, ROM_SEGMENT
    mov ds, ax

    ; Enable 16-color analog palette (mode2 bit 0 via port 0x6A)
    mov al, 0x01        ; ADR=0, DT=1 -> set bit 0
    out 0x6A, al

    ; Set 16-color analog palette
    call set_palette

    ; Start GDC slave (graphics)
    mov al, 0x6B
    out 0xA2, al

    ; Start GDC master (text)
    mov al, 0x6B
    out 0x62, al

    ; Clear text VRAM
    call clear_text_vram

    ; Main loop: cycle through patterns
    xor si, si

.main_loop:
    call draw_pattern
    call wait_enter

    inc si
    cmp si, NUM_PATTERNS
    jb .main_loop
    xor si, si
    jmp .main_loop

; ============================================================================
; draw_pattern — Dispatch to pattern routine based on SI
; ============================================================================
draw_pattern:
    call clear_all_planes
    call clear_text_vram

    cmp si, 0
    jne .not_0
    jmp pattern_fgc_fill
.not_0:
    cmp si, 1
    jne .not_1
    jmp pattern_bgc_fill
.not_1:
    cmp si, 2
    jne .not_2
    jmp pattern_cpu_broadcast
.not_2:
    jmp pattern_rop_copy

; ============================================================================
; EGC Enable/Disable
; ============================================================================
enable_egc:
    mov al, 0x01
    out 0x6A, al        ; analog mode (mode2 bit 0)
    mov al, 0x07
    out 0x6A, al        ; EGC permission (mode2 bit 3)
    mov al, 0x05
    out 0x6A, al        ; EGC request (mode2 bit 2)
    mov al, 0x80
    out 0x7C, al        ; GRCG enable -> EGC active
    ret

disable_egc:
    xor al, al
    out 0x7C, al
    ret

; ============================================================================
; write_egc_word — Write 16-bit value to EGC register
; DX = port, AX = value. Clobbers DX (+1) and AL.
; ============================================================================
write_egc_word:
    out dx, al
    inc dx
    mov al, ah
    out dx, al
    ret

; ============================================================================
; fill_half_lines — Fill half-width band via rep stosw
; ES = VRAM_B (0xA800), DI = start offset, AX = fill value
; BP = number of lines
; ============================================================================
fill_half_lines:
.loop:
    push di
    mov cx, 20          ; 20 words = 40 bytes per half-line
    rep stosw
    pop di
    add di, BYTES_PER_LINE
    dec bp
    jnz .loop
    ret

; ============================================================================
; Pattern 0: Foreground Color Fill
; Uses EGC FGC register as data source (CPU write value ignored).
; TL=1(blue), TR=2(red), BL=4(green), BR=8(dark gray)
; ============================================================================
pattern_fgc_fill:
    call enable_egc

    mov ax, VRAM_B
    mov es, ax

    ; TL: fg=1 (blue)
    mov dx, EGC_FG
    mov ax, 0x0001
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x4000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_TL
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    ; TR: fg=2 (red)
    mov dx, EGC_FG
    mov ax, 0x0002
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x4000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_TR
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    ; BL: fg=4 (green)
    mov dx, EGC_FG
    mov ax, 0x0004
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x4000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_BL
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    ; BR: fg=8 (dark gray)
    mov dx, EGC_FG
    mov ax, 0x0008
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x4000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_BR
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    call disable_egc
    ret

; ============================================================================
; Pattern 1: Background Color Fill
; Uses EGC BGC register as data source (CPU write value ignored).
; TL=3(magenta), TR=5(cyan), BL=6(yellow), BR=15(white)
; ============================================================================
pattern_bgc_fill:
    call enable_egc

    mov ax, VRAM_B
    mov es, ax

    ; TL: bg=3 (magenta)
    mov dx, EGC_BG
    mov ax, 0x0003
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x2000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_TL
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    ; TR: bg=5 (cyan)
    mov dx, EGC_BG
    mov ax, 0x0005
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x2000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_TR
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    ; BL: bg=6 (yellow)
    mov dx, EGC_BG
    mov ax, 0x0006
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x2000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_BL
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    ; BR: bg=15 (white)
    mov dx, EGC_BG
    mov ax, 0x000F
    call write_egc_word
    mov dx, EGC_FGBG
    mov ax, 0x2000
    call write_egc_word
    mov dx, EGC_OPE
    mov ax, 0x1000
    call write_egc_word
    mov di, Q_BR
    xor ax, ax
    mov bp, 200
    call fill_half_lines

    call disable_egc
    ret

; ============================================================================
; Pattern 2: CPU Broadcast with Plane Access Control
; Uses ope=0 (CPU broadcast). Access register selects writable planes.
; CPU writes 0xFFFF; only enabled planes receive data.
; TL: B only (blue), TR: R only (red), BL: B+G (cyan), BR: R+E (bright red)
; ============================================================================
pattern_cpu_broadcast:
    call enable_egc

    mov ax, VRAM_B
    mov es, ax

    ; TL: access=0xFFFE (B only writable)
    mov dx, EGC_ACCESS
    mov ax, 0xFFFE
    call write_egc_word
    mov dx, EGC_OPE
    xor ax, ax
    call write_egc_word
    mov di, Q_TL
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ; TR: access=0xFFFD (R only writable)
    mov dx, EGC_ACCESS
    mov ax, 0xFFFD
    call write_egc_word
    mov dx, EGC_OPE
    xor ax, ax
    call write_egc_word
    mov di, Q_TR
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ; BL: access=0xFFFA (B+G writable)
    mov dx, EGC_ACCESS
    mov ax, 0xFFFA
    call write_egc_word
    mov dx, EGC_OPE
    xor ax, ax
    call write_egc_word
    mov di, Q_BL
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ; BR: access=0xFFF5 (R+E writable)
    mov dx, EGC_ACCESS
    mov ax, 0xFFF5
    call write_egc_word
    mov dx, EGC_OPE
    xor ax, ax
    call write_egc_word
    mov di, Q_BR
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    call disable_egc
    ret

; ============================================================================
; Pattern 3: ROP Block Copy (VRAM Source)
; Pre-fills source words in non-visible VRAM (offset >= 32000), then uses
; EGC shift+ROP to copy each source into its quadrant.
; TL=9(bright blue), TR=6(yellow), BL=12(bright green), BR=3(magenta)
; ============================================================================
pattern_rop_copy:
    ; Pre-fill source patterns in non-visible VRAM (EGC disabled)
    ; B plane sources
    mov ax, VRAM_B
    mov es, ax
    mov word [es:SRC_BASE+0], 0xFFFF    ; src 0: B=FF
    mov word [es:SRC_BASE+2], 0x0000    ; src 1: B=00
    mov word [es:SRC_BASE+4], 0x0000    ; src 2: B=00
    mov word [es:SRC_BASE+6], 0xFFFF    ; src 3: B=FF

    ; R plane sources
    mov ax, VRAM_R
    mov es, ax
    mov word [es:SRC_BASE+0], 0x0000    ; src 0: R=00
    mov word [es:SRC_BASE+2], 0xFFFF    ; src 1: R=FF
    mov word [es:SRC_BASE+4], 0x0000    ; src 2: R=00
    mov word [es:SRC_BASE+6], 0xFFFF    ; src 3: R=FF

    ; G plane sources
    mov ax, VRAM_G
    mov es, ax
    mov word [es:SRC_BASE+0], 0x0000    ; src 0: G=00
    mov word [es:SRC_BASE+2], 0xFFFF    ; src 1: G=FF
    mov word [es:SRC_BASE+4], 0xFFFF    ; src 2: G=FF
    mov word [es:SRC_BASE+6], 0x0000    ; src 3: G=00

    ; E plane sources
    mov ax, VRAM_E
    mov es, ax
    mov word [es:SRC_BASE+0], 0xFFFF    ; src 0: E=FF
    mov word [es:SRC_BASE+2], 0x0000    ; src 1: E=00
    mov word [es:SRC_BASE+4], 0xFFFF    ; src 2: E=FF
    mov word [es:SRC_BASE+6], 0x0000    ; src 3: E=00

    ; Enable EGC for ROP copy
    call enable_egc

    ; Set ES = VRAM_B for all EGC operations
    mov ax, VRAM_B
    mov es, ax

    ; EGC setup: ope=0x08F0 (shift+ROP, read=VRAM, ROP=0xF0 source copy)
    mov dx, EGC_OPE
    mov ax, 0x08F0
    call write_egc_word

    ; sft=0x0000 (ascending, no shift)
    mov dx, EGC_SFT
    xor ax, ax
    call write_egc_word

    ; access=0xFFF0 (all planes enabled)
    mov dx, EGC_ACCESS
    mov ax, 0xFFF0
    call write_egc_word

    ; TL: source offset SRC_BASE+0
    mov bx, SRC_BASE
    mov di, Q_TL
    mov bp, 200
    call rop_copy_half_lines

    ; TR: source offset SRC_BASE+2
    mov bx, SRC_BASE + 2
    mov di, Q_TR
    mov bp, 200
    call rop_copy_half_lines

    ; BL: source offset SRC_BASE+4
    mov bx, SRC_BASE + 4
    mov di, Q_BL
    mov bp, 200
    call rop_copy_half_lines

    ; BR: source offset SRC_BASE+6
    mov bx, SRC_BASE + 6
    mov di, Q_BR
    mov bp, 200
    call rop_copy_half_lines

    call disable_egc
    ret

; ============================================================================
; rop_copy_half_lines — ROP copy from source to half-width band
; ES = VRAM_B, BX = source word offset, DI = dest start offset
; BP = number of lines
; ============================================================================
rop_copy_half_lines:
.line_loop:
    push di
    mov cx, 20          ; 20 words per half-line
.word_loop:
    ; Reset shift pipeline: write leng=0x000F (16 bits)
    mov dx, EGC_LENG
    mov ax, 0x000F
    call write_egc_word

    ; Read source word (loads 4-plane data into shift buffer)
    mov ax, [es:bx]

    ; Write destination word (outputs through ROP to all planes)
    mov [es:di], ax
    add di, 2

    loop .word_loop

    pop di
    add di, BYTES_PER_LINE
    dec bp
    jnz .line_loop
    ret

; ============================================================================
; Utility: set_palette — Set 16-color analog palette
; ============================================================================
set_palette:
    xor cx, cx

.pal_loop:
    mov al, cl
    out 0xA8, al

    mov bx, cx
    imul bx, 3
    add bx, palette_data

    ; Green
    mov al, [cs:bx]
    out 0xAA, al

    ; Red
    mov al, [cs:bx+1]
    out 0xAC, al

    ; Blue
    mov al, [cs:bx+2]
    out 0xAE, al

    inc cx
    cmp cx, 16
    jb .pal_loop
    ret

; Palette data: G, R, B per entry
palette_data:
    ;       G    R    B
    db      0,   0,   0       ; 0  Black
    db      0,   0,   7       ; 1  Blue
    db      0,   7,   0       ; 2  Red
    db      0,   7,   7       ; 3  Magenta
    db      7,   0,   0       ; 4  Green
    db      7,   0,   7       ; 5  Cyan
    db      7,   7,   0       ; 6  Yellow
    db      7,   7,   7       ; 7  White (dim)
    db      4,   4,   4       ; 8  Dark Gray
    db      0,   0, 0x0F      ; 9  Bright Blue
    db      0, 0x0F,   0       ; 10 Bright Red
    db      0, 0x0F, 0x0F     ; 11 Bright Magenta
    db   0x0F,   0,   0       ; 12 Bright Green
    db   0x0F,   0, 0x0F      ; 13 Bright Cyan
    db   0x0F, 0x0F,   0       ; 14 Bright Yellow
    db   0x0F, 0x0F, 0x0F     ; 15 Bright White

; ============================================================================
; Utility: fill_all_planes — Fill all 4 planes with byte AL
; ============================================================================
fill_all_planes:
    mov bx, VRAM_B
    call fill_plane_full
    mov bx, VRAM_R
    call fill_plane_full
    mov bx, VRAM_G
    call fill_plane_full
    mov bx, VRAM_E
    jmp fill_plane_full

; ============================================================================
; Utility: clear_all_planes — Zero out B, R, G, E planes
; ============================================================================
clear_all_planes:
    xor al, al
    jmp fill_all_planes

; ============================================================================
; Utility: fill_plane_full — Fill entire plane segment BX with byte AL
; ============================================================================
fill_plane_full:
    mov es, bx
    xor di, di
    mov ah, al
    mov cx, PLANE_SIZE / 2
    rep stosw
    ret

; ============================================================================
; Utility: clear_text_vram — Fill text VRAM with spaces + invisible attribute
; ============================================================================
clear_text_vram:
    mov ax, TEXT_VRAM
    mov es, ax
    xor di, di
    mov ax, 0x0020
    mov cx, 80 * 25
    rep stosw
    mov di, 0x2000
    xor ax, ax
    mov cx, 80 * 25
    rep stosw
    ret

; ============================================================================
; Utility: wait_enter — Wait for Enter key press
; ============================================================================
wait_enter:
.wait_make:
    in al, 0x43
    test al, 0x02
    jz .wait_make
    in al, 0x41
    cmp al, 0x1C
    jne .wait_make

    ; Drain the break code
.wait_break:
    in al, 0x43
    test al, 0x02
    jz .wait_break
    in al, 0x41

    ret

; Pad bank 1 to exactly 96KB
    times 0x18000 - ($ - $$) db 0xFF
