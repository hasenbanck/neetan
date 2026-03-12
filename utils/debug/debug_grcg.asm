; grcg.asm — GRCG test ROM for Neetan
; Assembles to a 96KB ROM image loaded at physical 0xE8000–0xFFFFF
; Cycles through 8 fullscreen test patterns with Enter key

[bits 16]
[cpu 186]
[org 0x0000]

; ROM is loaded at segment 0xE800 (physical 0xE8000)
; Total size: 0x18000 (96KB)

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

; GRCG ports
GRCG_MODE       equ 0x7C
GRCG_TILE       equ 0x7E

NUM_PATTERNS    equ 9

; ============================================================================
; Entry point (jumped to from reset vector at end of ROM)
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
    mov al, 0x01        ; ADR=0, DT=1 → set bit 0
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
    ; Reset monochrome mode for clean state
    mov al, 0x02        ; ADR=1, DT=0 → clear bit 1 (GRAPHIC_MODE=color)
    out 0x68, al

    call clear_all_planes
    call clear_text_vram

    cmp si, 0
    jne .not_0
    jmp pattern_solid_white
.not_0:
    cmp si, 1
    jne .not_1
    jmp pattern_individual_planes
.not_1:
    cmp si, 2
    jne .not_2
    jmp pattern_tdw_full
.not_2:
    cmp si, 3
    jne .not_3
    jmp pattern_tdw_selective
.not_3:
    cmp si, 4
    jne .not_4
    jmp pattern_tcr
.not_4:
    cmp si, 5
    jne .not_5
    jmp pattern_rmw_all
.not_5:
    cmp si, 6
    jne .not_6
    jmp pattern_rmw_selective
.not_6:
    cmp si, 7
    jne .not_7
    jmp pattern_word_ops
.not_7:
    jmp pattern_monochrome

; ============================================================================
; Pattern 0: Solid White (Direct Write)
; Fill all 4 planes with 0xFF via direct rep stosw.
; ============================================================================
pattern_solid_white:
    mov al, 0xFF
    jmp fill_all_planes

; ============================================================================
; Pattern 1: Individual Planes (Direct Write)
; 4 bands of 100 lines: B only (0-99), R only (100-199),
;   G only (200-299), E only (300-399).
; ============================================================================
pattern_individual_planes:
    mov bx, VRAM_B
    mov al, 0xFF
    xor dx, dx
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_R
    mov al, 0xFF
    mov dx, 100
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_G
    mov al, 0xFF
    mov dx, 200
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_E
    mov al, 0xFF
    mov dx, 300
    mov bp, 100
    call fill_plane_rows

    ret

; ============================================================================
; Pattern 2: TDW Full Fill — all planes
; Mode 0x80, tiles: B=0xAA, R=0x55, G=0xF0, E=0x0F.
; Fill entire screen via GRCG (CPU data ignored in TDW).
; ============================================================================
pattern_tdw_full:
    mov al, 0x80
    out GRCG_MODE, al

    mov al, 0xAA
    out GRCG_TILE, al
    mov al, 0x55
    out GRCG_TILE, al
    mov al, 0xF0
    out GRCG_TILE, al
    mov al, 0x0F
    out GRCG_TILE, al

    mov bx, VRAM_B
    xor al, al
    call fill_plane_full

    xor al, al
    out GRCG_MODE, al

    ret

; ============================================================================
; Pattern 3: TDW Selective Plane Enable
; Pre-fill all planes with 0xFF. Mode 0x8A (TDW, R and E disabled).
; Tiles: B=0x00, G=0x00 (R,E tiles ignored since disabled).
; Expected: B=0x00, R=0xFF(unchanged), G=0x00, E=0xFF(unchanged).
; ============================================================================
pattern_tdw_selective:
    mov al, 0xFF
    call fill_all_planes

    mov al, 0x8A
    out GRCG_MODE, al

    mov al, 0x00
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al

    mov bx, VRAM_B
    xor al, al
    call fill_plane_full

    xor al, al
    out GRCG_MODE, al

    ret

; ============================================================================
; Pattern 4: TCR (Tile Compare Read) — 4 bands
;
; Band 0 (lines 0-99):   B=0xFF, R=0x00, G=0xFF, E=0x00
; Band 1 (lines 100-199): B=0xFF, R=0xFF, G=0xFF, E=0xFF
; Band 2 (lines 200-299): B=0x00, R=0x00, G=0x00, E=0x00 (from clear)
; Band 3 (lines 300-399): B=0xAA, R=0x55, G=0xAA, E=0x55
;
; Tiles: B=0xFF, R=0x00, G=0xFF, E=0x00
; TCR reads from lines 50, 150, 250, 350 → results at 0x0500-0x0503.
; ============================================================================
pattern_tcr:
    ; Band 0: Lines 0-99: B=0xFF, G=0xFF (R,E = 0x00 from clear)
    mov bx, VRAM_B
    mov al, 0xFF
    xor dx, dx
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_G
    mov al, 0xFF
    xor dx, dx
    mov bp, 100
    call fill_plane_rows

    ; Band 1: Lines 100-199: all 0xFF
    mov bx, VRAM_B
    mov al, 0xFF
    mov dx, 100
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_R
    mov al, 0xFF
    mov dx, 100
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_G
    mov al, 0xFF
    mov dx, 100
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_E
    mov al, 0xFF
    mov dx, 100
    mov bp, 100
    call fill_plane_rows

    ; Band 2: Lines 200-299: all 0x00 (already clear)

    ; Band 3: Lines 300-399: B=0xAA, R=0x55, G=0xAA, E=0x55
    mov bx, VRAM_B
    mov al, 0xAA
    mov dx, 300
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_R
    mov al, 0x55
    mov dx, 300
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_G
    mov al, 0xAA
    mov dx, 300
    mov bp, 100
    call fill_plane_rows

    mov bx, VRAM_E
    mov al, 0x55
    mov dx, 300
    mov bp, 100
    call fill_plane_rows

    ; Enable GRCG in TCR mode
    mov al, 0x80
    out GRCG_MODE, al

    mov al, 0xFF
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al
    mov al, 0xFF
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al

    ; Read byte 0 of representative lines via TCR
    mov ax, VRAM_B
    mov es, ax

    ; Line 50, byte 0 (offset 4000)
    mov al, [es:4000]
    mov [ss:0x0500], al

    ; Line 150, byte 0 (offset 12000)
    mov al, [es:12000]
    mov [ss:0x0501], al

    ; Line 250, byte 0 (offset 20000)
    mov al, [es:20000]
    mov [ss:0x0502], al

    ; Line 350, byte 0 (offset 28000)
    mov al, [es:28000]
    mov [ss:0x0503], al

    xor al, al
    out GRCG_MODE, al

    ret

; ============================================================================
; Pattern 5: RMW Mode — all planes
; Pre-fill all planes with 0x55. Mode 0xC0 (RMW).
; Tiles: B=0xFF, R=0x00, G=0xAA, E=0x55. CPU mask: 0xF0.
; Formula: new = (cpu & tile) | (~cpu & old)
;   B: (0xF0 & 0xFF) | (0x0F & 0x55) = 0xF0 | 0x05 = 0xF5
;   R: (0xF0 & 0x00) | (0x0F & 0x55) = 0x00 | 0x05 = 0x05
;   G: (0xF0 & 0xAA) | (0x0F & 0x55) = 0xA0 | 0x05 = 0xA5
;   E: (0xF0 & 0x55) | (0x0F & 0x55) = 0x50 | 0x05 = 0x55
; ============================================================================
pattern_rmw_all:
    mov al, 0x55
    call fill_all_planes

    mov al, 0xC0
    out GRCG_MODE, al

    mov al, 0xFF
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al
    mov al, 0xAA
    out GRCG_TILE, al
    mov al, 0x55
    out GRCG_TILE, al

    mov bx, VRAM_B
    mov al, 0xF0
    call fill_plane_full

    xor al, al
    out GRCG_MODE, al

    ret

; ============================================================================
; Pattern 6: RMW Selective Plane Enable
; Pre-fill all planes with 0xAA. Mode 0xCC (RMW, G+E disabled).
; Tiles: B=0xFF, R=0xFF. CPU mask: 0xFF.
; Expected: B=0xFF, R=0xFF, G=0xAA(unchanged), E=0xAA(unchanged).
; ============================================================================
pattern_rmw_selective:
    mov al, 0xAA
    call fill_all_planes

    mov al, 0xCC
    out GRCG_MODE, al

    mov al, 0xFF
    out GRCG_TILE, al
    mov al, 0xFF
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al

    mov bx, VRAM_B
    mov al, 0xFF
    call fill_plane_full

    xor al, al
    out GRCG_MODE, al

    ret

; ============================================================================
; Pattern 7: Word-Width GRCG Operations
;
; Part A (top, lines 0-199): TDW mode 0x80.
;   Tiles: B=0x33, R=0xCC, G=0x55, E=0xAA. rep stosw fill.
;
; Part B (bottom, lines 200-399): Pre-fill with 0xFF, RMW mode 0xC0.
;   Tiles: B=0xF0, R=0x0F, G=0xFF, E=0x00. CPU data: 0xAAAA via rep stosw.
;   new = (0xAA & tile) | (0x55 & 0xFF):
;     B: 0xA0 | 0x55 = 0xF5
;     R: 0x0A | 0x55 = 0x5F
;     G: 0xAA | 0x55 = 0xFF
;     E: 0x00 | 0x55 = 0x55
; ============================================================================
pattern_word_ops:
    ; --- Part A: TDW word fill top half (lines 0-199) ---
    mov al, 0x80
    out GRCG_MODE, al

    mov al, 0x33
    out GRCG_TILE, al
    mov al, 0xCC
    out GRCG_TILE, al
    mov al, 0x55
    out GRCG_TILE, al
    mov al, 0xAA
    out GRCG_TILE, al

    mov ax, VRAM_B
    mov es, ax
    xor di, di
    xor ax, ax
    mov cx, 8000            ; 200 lines * 80 bytes / 2 = 8000 words
    rep stosw

    xor al, al
    out GRCG_MODE, al

    ; --- Part B: RMW word fill bottom half (lines 200-399) ---
    ; Pre-fill bottom half with 0xFF (direct writes)
    mov bx, VRAM_B
    mov al, 0xFF
    mov dx, 200
    mov bp, 200
    call fill_plane_rows

    mov bx, VRAM_R
    mov al, 0xFF
    mov dx, 200
    mov bp, 200
    call fill_plane_rows

    mov bx, VRAM_G
    mov al, 0xFF
    mov dx, 200
    mov bp, 200
    call fill_plane_rows

    mov bx, VRAM_E
    mov al, 0xFF
    mov dx, 200
    mov bp, 200
    call fill_plane_rows

    mov al, 0xC0
    out GRCG_MODE, al

    mov al, 0xF0
    out GRCG_TILE, al
    mov al, 0x0F
    out GRCG_TILE, al
    mov al, 0xFF
    out GRCG_TILE, al
    mov al, 0x00
    out GRCG_TILE, al

    mov ax, VRAM_B
    mov es, ax
    mov di, 16000           ; line 200 * 80 bytes
    mov ax, 0xAAAA
    mov cx, 8000            ; 200 lines * 80 bytes / 2 = 8000 words
    rep stosw

    xor al, al
    out GRCG_MODE, al

    ret

; ============================================================================
; Pattern 8: Monochrome Mode Test
; Lines 0-319 (rows 0-19): G+E planes → color index 12 (analog green=0xF,
;   bit 3 set → mono ON, shows text attribute colors)
; Lines 320-399 (rows 20-24): B-plane only → color index 1 (analog green=0x0,
;   bit 3 clear → mono OFF, shows black)
; Text: 5 bands of colored attributes (white, red, green, cyan, yellow)
; ============================================================================
pattern_monochrome:
    ; Enable monochrome mode
    mov al, 0x03        ; ADR=1, DT=1 → set bit 1 (GRAPHIC_MODE=monochrome)
    out 0x68, al

    ; Write colored text attributes (5 bands of 5 rows)
    mov ax, TEXT_VRAM
    mov es, ax

    ; Fill character VRAM with spaces
    xor di, di
    mov ax, 0x0020
    mov cx, 80 * 25
    rep stosw

    ; Fill attribute VRAM with colored attributes per band
    mov di, 0x2000
    ; Band 0: rows 0-4, color 7 (white) → attr 0xE1
    mov ax, 0x00E1
    mov cx, 80 * 5
    rep stosw
    ; Band 1: rows 5-9, color 2 (red) → attr 0x41
    mov ax, 0x0041
    mov cx, 80 * 5
    rep stosw
    ; Band 2: rows 10-14, color 4 (green) → attr 0x81
    mov ax, 0x0081
    mov cx, 80 * 5
    rep stosw
    ; Band 3: rows 15-19, color 5 (cyan) → attr 0xA1
    mov ax, 0x00A1
    mov cx, 80 * 5
    rep stosw
    ; Band 4: rows 20-24, color 6 (yellow) → attr 0xC1
    mov ax, 0x00C1
    mov cx, 80 * 5
    rep stosw

    ; Graphics: lines 0-319 G+E planes (index 12), lines 320-399 B-plane (index 1)
    mov bx, VRAM_G
    mov al, 0xFF
    xor dx, dx
    mov bp, 320
    call fill_plane_rows

    mov bx, VRAM_E
    mov al, 0xFF
    xor dx, dx
    mov bp, 320
    call fill_plane_rows

    mov bx, VRAM_B
    mov al, 0xFF
    mov dx, 320
    mov bp, 80
    call fill_plane_rows

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
; Utility: fill_plane_rows — Fill rows in plane segment BX with byte AL
; DX = start line, BP = number of lines
; ============================================================================
fill_plane_rows:
    push ds
    mov es, bx

    push ax
    mov ax, dx
    mov cx, 80
    mul cx
    mov di, ax
    pop ax

    mov dx, bp
.row_fill_loop:
    push di
    mov cx, BYTES_PER_LINE
    rep stosb
    pop di
    add di, BYTES_PER_LINE
    dec dx
    jnz .row_fill_loop

    pop ds
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

; ============================================================================
; Pad to fill 96KB, with reset vector at the end
; ============================================================================

    ; Fill remaining space up to the reset vector with 0xFF
    times (0x18000 - 16) - ($ - $$) db 0xFF

; Reset vector at ROM offset 0x17FF0 (physical 0xFFFF0)
reset_vector:
    jmp ROM_SEGMENT:entry

    ; Pad remaining bytes after reset vector to fill exactly 96KB
    times 0x18000 - ($ - $$) db 0xFF
