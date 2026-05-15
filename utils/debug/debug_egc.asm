; debug_egc.asm - EGC test ROM for Neetan
; Assembles to a 192 KB dual-bank ROM image for PC-9801VX/RA class machines.
;   Bank 0 (first 96 KB, file offset 0x00000): F8000-FFFFF - reset vector only
;   Bank 1 (second 96 KB, file offset 0x18000): E8000-F7FFF - all code and data
;
; Reads the mode-selector byte from physical address 0x0500 on startup:
;   0  Interactive: cycle through modes 1..4 with Enter
;   1  EGC FGC fill (TL=1 blue, TR=2 red, BL=4 green, BR=8 dark gray)
;   2  EGC BGC fill (TL=3 magenta, TR=5 cyan, BL=6 yellow, BR=15 white)
;   3  EGC CPU broadcast with plane access (TL=B-only, TR=R-only,
;      BL=B+G, BR=R+E)
;   4  EGC ROP coverage: same source (S = all 0xFF) + same destination
;      pre-fill (D = R+G planes 0xFF -> palette 6 yellow), 4 different ROPs
;      (TL=0xF0 S -> white, TR=0x0F ~S -> black, BL=0xCC D -> yellow,
;      BR=0x33 ~D -> bright blue)
;
; Non-zero mode values render the page once and HLT, for integration tests.
; Zero (the default for zero-initialized RAM) keeps an interactive UX so the
; ROM stays usable by hand when paired with a keyboard.

[bits 16]
[cpu 186]

ROM_SEGMENT     equ 0xE800

VRAM_B          equ 0xA800
VRAM_R          equ 0xB000
VRAM_G          equ 0xB800
VRAM_E          equ 0xE000

TEXT_VRAM       equ 0xA000

SCREEN_WIDTH    equ 640
SCREEN_HEIGHT   equ 400
BYTES_PER_LINE  equ 80
PLANE_SIZE      equ BYTES_PER_LINE * SCREEN_HEIGHT  ; 32000 bytes

; EGC register ports (word-accessible at base + 0, base + 1).
EGC_ACCESS      equ 0x04A0
EGC_FGBG        equ 0x04A2
EGC_OPE         equ 0x04A4
EGC_FG          equ 0x04A6
EGC_BG          equ 0x04AA
EGC_SFT         equ 0x04AC
EGC_LENG        equ 0x04AE

GRCG_MODE       equ 0x7C

MODE_BYTE_ADDR  equ 0x0500

; Quadrant start offsets within plane VRAM.
Q_TL            equ 0
Q_TR            equ 40
Q_BL            equ 200 * BYTES_PER_LINE
Q_BR            equ 200 * BYTES_PER_LINE + 40

; Source data base offset (beyond visible VRAM area).
SRC_BASE        equ PLANE_SIZE

; Bank 0 - mapped to F8000-FFFFF. Reset vector only.
section bank0 start=0 vstart=0

    times (0x18000 - 16) db 0xFF

    ; Reset vector at physical 0xFFFF0 - jumps to bank 1 entry at E800:0000.
    db 0xEA             ; far jmp opcode
    dw 0x0000           ; IP = 0 (start of bank 1 code)
    dw ROM_SEGMENT      ; CS = E800
    times 0x18000 - ($ - $$) db 0xFF

; Bank 1 - mapped to E8000-F7FFF. Contains all executable code.
section bank1 start=0x18000 vstart=0

entry:
    cli

    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00

    mov ds, ax
    mov bl, [MODE_BYTE_ADDR]
    push bx                ; preserve mode byte across BX-clobbering helpers

    mov ax, ROM_SEGMENT
    mov ds, ax

    ; Enable 16-color analog palette (mode2 bit 0 via port 0x6A).
    mov al, 0x01
    out 0x6A, al

    call set_palette

    ; Force the slave GDC into 400-line mode (lines_per_row = 1) so the
    ; renderer composes every raster of the 400-line display, not a
    ; skipline-doubled 200-line image.
    call gdc_set_lines_per_row_1

    ; Start GDC slave (graphics) and master (text).
    mov al, 0x6B
    out 0xA2, al
    mov al, 0x6B
    out 0x62, al

    call clear_text_vram
    call clear_all_planes

    pop bx                 ; restore mode byte

    cmp bl, 0
    je interactive_loop
    cmp bl, 1
    je .mode_1
    cmp bl, 2
    je .mode_2
    cmp bl, 3
    je .mode_3
    cmp bl, 4
    je .mode_4
    jmp halt_forever

.mode_1:
    call render_mode_1_fgc_fill
    jmp halt_forever
.mode_2:
    call render_mode_2_bgc_fill
    jmp halt_forever
.mode_3:
    call render_mode_3_cpu_broadcast
    jmp halt_forever
.mode_4:
    call render_mode_4_rop_copy
    jmp halt_forever

halt_forever:
    hlt
    jmp halt_forever

interactive_loop:
.loop:
    call render_mode_1_fgc_fill
    call wait_enter
    call clear_all_planes
    call render_mode_2_bgc_fill
    call wait_enter
    call clear_all_planes
    call render_mode_3_cpu_broadcast
    call wait_enter
    call clear_all_planes
    call render_mode_4_rop_copy
    call wait_enter
    call clear_all_planes
    jmp .loop

; enable_egc - Switch into EGC-active state.
;   Mode2: analog (bit 0) + EGC permission (bit 3) + EGC request (bit 2).
;   GRCG mode 0x80 (CG mode on, all planes enabled) routes writes through
;   EGC.
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

; disable_egc - Drop GRCG mode so subsequent writes hit raw VRAM again.
disable_egc:
    xor al, al
    out 0x7C, al
    ret

; write_egc_word - Write 16-bit value AX to EGC register DX (port pair DX,
;   DX+1). Clobbers DX (+1) and AL.
write_egc_word:
    out dx, al
    inc dx
    mov al, ah
    out dx, al
    ret

; fill_half_lines - Fill half-width band via rep stosw.
;   ES = VRAM_B, DI = start offset, AX = fill value, BP = number of lines.
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

; Mode 1 - Foreground Color Fill.
;   Uses EGC FGC register as data source (CPU write value ignored).
;   TL=1(blue), TR=2(red), BL=4(green), BR=8(dark gray).
render_mode_1_fgc_fill:
    call enable_egc

    mov ax, VRAM_B
    mov es, ax

    ; TL: fg=1 (blue).
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

    ; TR: fg=2 (red).
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

    ; BL: fg=4 (green).
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

    ; BR: fg=8 (dark gray).
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

; Mode 2 - Background Color Fill.
;   Uses EGC BGC register as data source (CPU write value ignored).
;   TL=3(magenta), TR=5(cyan), BL=6(yellow), BR=15(white).
render_mode_2_bgc_fill:
    call enable_egc

    mov ax, VRAM_B
    mov es, ax

    ; TL: bg=3 (magenta).
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

    ; TR: bg=5 (cyan).
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

    ; BL: bg=6 (yellow).
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

    ; BR: bg=15 (white).
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

; Mode 3 - CPU Broadcast with Plane Access Control.
;   Uses ope=0 (CPU broadcast). Access register selects writable planes.
;   CPU writes 0xFFFF; only enabled planes receive data.
;   TL: B only (blue), TR: R only (red), BL: B+G (cyan), BR: R+E (bright red).
render_mode_3_cpu_broadcast:
    call enable_egc

    mov ax, VRAM_B
    mov es, ax

    ; TL: access=0xFFFE (B only writable).
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

    ; TR: access=0xFFFD (R only writable).
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

    ; BL: access=0xFFFA (B+G writable).
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

    ; BR: access=0xFFF5 (R+E writable).
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

; Mode 4 - EGC ROP coverage: same source + same destination pre-fill, but
;   4 different ROP codes across 4 quadrants. This exercises 4 distinct
;   truth-table entries instead of just one.
;
;   Source S (in non-visible VRAM, all planes = 0xFFFF): per-pixel index 15.
;   Destination D pre-fill (R, G planes = 0xFF; B, E planes = 0x00):
;     per-pixel index 6 (yellow).
;
;     TL (ope=0x08F0, ROP S):  result = source       -> index 15 bright white
;     TR (ope=0x080F, ROP ~S): result = ~source = 0  -> index 0  black
;     BL (ope=0x08CC, ROP D):  result = destination  -> index 6  yellow
;     BR (ope=0x0833, ROP ~D): result = ~destination -> index 9  bright blue
render_mode_4_rop_copy:
    ; Pre-fill destination: R and G planes = 0xFF everywhere. B and E
    ; remain 0x00 from clear_all_planes at entry.
    mov bx, VRAM_R
    mov al, 0xFF
    call fill_plane_full
    mov bx, VRAM_G
    mov al, 0xFF
    call fill_plane_full

    ; Pre-fill source: one word per plane at SRC_BASE, all 0xFFFF so that
    ; S = 1 on every plane bit for the entire 320-pixel quadrant width
    ; (rop_copy_half_lines re-reads the same word for every dest word).
    mov ax, VRAM_B
    mov es, ax
    mov word [es:SRC_BASE], 0xFFFF
    mov ax, VRAM_R
    mov es, ax
    mov word [es:SRC_BASE], 0xFFFF
    mov ax, VRAM_G
    mov es, ax
    mov word [es:SRC_BASE], 0xFFFF
    mov ax, VRAM_E
    mov es, ax
    mov word [es:SRC_BASE], 0xFFFF

    ; Enable EGC for ROP copy.
    call enable_egc

    ; ES = VRAM_B for all EGC operations.
    mov ax, VRAM_B
    mov es, ax

    ; sft = 0x0000 (ascending, no shift).
    mov dx, EGC_SFT
    xor ax, ax
    call write_egc_word

    ; access = 0xFFF0 (all planes enabled).
    mov dx, EGC_ACCESS
    mov ax, 0xFFF0
    call write_egc_word

    ; TL: ROP 0xF0 (S = source copy).
    mov dx, EGC_OPE
    mov ax, 0x08F0
    call write_egc_word
    mov bx, SRC_BASE
    mov di, Q_TL
    mov bp, 200
    call rop_copy_half_lines

    ; TR: ROP 0x0F (~S = invert source).
    mov dx, EGC_OPE
    mov ax, 0x080F
    call write_egc_word
    mov bx, SRC_BASE
    mov di, Q_TR
    mov bp, 200
    call rop_copy_half_lines

    ; BL: ROP 0xCC (D = destination unchanged).
    mov dx, EGC_OPE
    mov ax, 0x08CC
    call write_egc_word
    mov bx, SRC_BASE
    mov di, Q_BL
    mov bp, 200
    call rop_copy_half_lines

    ; BR: ROP 0x33 (~D = invert destination).
    mov dx, EGC_OPE
    mov ax, 0x0833
    call write_egc_word
    mov bx, SRC_BASE
    mov di, Q_BR
    mov bp, 200
    call rop_copy_half_lines

    call disable_egc
    ret

; rop_copy_half_lines - ROP copy from source to half-width band.
;   ES = VRAM_B, BX = source word offset, DI = dest start offset,
;   BP = number of lines.
rop_copy_half_lines:
.line_loop:
    push di
    mov cx, 20          ; 20 words per half-line
.word_loop:
    ; Reset shift pipeline: write leng=0x000F (16 bits).
    mov dx, EGC_LENG
    mov ax, 0x000F
    call write_egc_word

    ; Read source word (loads 4-plane data into shift buffer).
    mov ax, [es:bx]

    ; Write destination word (outputs through ROP to all planes).
    mov [es:di], ax
    add di, 2

    loop .word_loop

    pop di
    add di, BYTES_PER_LINE
    dec bp
    jnz .line_loop
    ret

; gdc_set_lines_per_row_1 - Slave GDC CCHAR command with P0 = 0 so each
; logical raster maps to one physical scanline (400-line display, no
; skipline doubling).
gdc_set_lines_per_row_1:
    mov al, 0x4B
    out 0xA2, al
    xor al, al             ; P0: lines_per_row - 1 = 0
    out 0xA0, al
    xor al, al             ; P1
    out 0xA0, al
    xor al, al             ; P2
    out 0xA0, al
    ret

; set_palette - Program the 16-entry analog palette with the standard
; PC-98 colors (G,R,B per entry, 4-bit components).
set_palette:
    xor cx, cx

.pal_loop:
    mov al, cl
    out 0xA8, al

    mov bx, cx
    imul bx, 3
    add bx, palette_data

    ; Green.
    mov al, [cs:bx]
    out 0xAA, al

    ; Red.
    mov al, [cs:bx+1]
    out 0xAC, al

    ; Blue.
    mov al, [cs:bx+2]
    out 0xAE, al

    inc cx
    cmp cx, 16
    jb .pal_loop
    ret

; Palette data: G, R, B per entry.
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
    db      0, 0x0F,   0      ; 10 Bright Red
    db      0, 0x0F, 0x0F     ; 11 Bright Magenta
    db   0x0F,   0,   0       ; 12 Bright Green
    db   0x0F,   0, 0x0F      ; 13 Bright Cyan
    db   0x0F, 0x0F,   0      ; 14 Bright Yellow
    db   0x0F, 0x0F, 0x0F     ; 15 Bright White

; fill_all_planes - Fill all 4 plane segments with byte AL.
fill_all_planes:
    mov bx, VRAM_B
    call fill_plane_full
    mov bx, VRAM_R
    call fill_plane_full
    mov bx, VRAM_G
    call fill_plane_full
    mov bx, VRAM_E
    jmp fill_plane_full

; clear_all_planes - Zero out B, R, G, E planes.
clear_all_planes:
    xor al, al
    jmp fill_all_planes

; fill_plane_full - Fill entire plane segment BX with byte AL.
fill_plane_full:
    mov es, bx
    xor di, di
    mov ah, al
    mov cx, PLANE_SIZE / 2
    rep stosw
    ret

; clear_text_vram - Fill text VRAM with spaces + invisible attribute.
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

; wait_enter - Block until Enter is pressed and released.
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

; Pad bank 1 to exactly 96 KB.
    times 0x18000 - ($ - $$) db 0xFF
