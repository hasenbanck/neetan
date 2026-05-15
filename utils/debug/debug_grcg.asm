; debug_grcg.asm - GRCG test ROM for Neetan
; Assembles to a 96 KB ROM image loaded at physical 0xE8000-0xFFFFF
; (single-bank layout for PC-9801VM class machines with GRCG v1).
;
; Reads the mode-selector byte from physical address 0x0500 on startup:
;   0  Interactive: cycle through modes 1..9 with Enter
;   1  Solid white via direct rep stosw (all planes 0xFF)
;   2  Individual planes via direct write (4 horizontal bands)
;   3  TDW full fill (B=0xAA, R=0x55, G=0xF0, E=0x0F)
;   4  TDW selective plane enable (4 quadrants, 4 different plane-disable
;      masks over pre-fill 0xFF, tile 0x00)
;   5  TCR (4 bands; results stored at RAM 0x0500..0x0503)
;   6  RMW all planes (pre-fill 0x55, CPU mask 0xF0)
;   7  RMW selective (4 quadrants, 4 different plane-disable masks over
;      pre-fill 0xAA, tile 0xFF, CPU 0xFF)
;   8  Word-width ops (TDW top half, RMW bottom half)
;   9  Monochrome overlay (graphics G+E top, B-plane bottom, text attributes)
;
; Non-zero mode values render the page once and HLT, for integration tests.
; Zero (the default for zero-initialized RAM) keeps an interactive UX so the
; ROM stays usable by hand when paired with a keyboard.

[bits 16]
[cpu 186]
[org 0x0000]

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

GRCG_MODE       equ 0x7C
GRCG_TILE       equ 0x7E

MODE_BYTE_ADDR  equ 0x0500

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

    call reset_state

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
    cmp bl, 5
    je .mode_5
    cmp bl, 6
    je .mode_6
    cmp bl, 7
    je .mode_7
    cmp bl, 8
    je .mode_8
    cmp bl, 9
    je .mode_9
    jmp halt_forever

.mode_1:
    call render_mode_1_solid_white
    jmp halt_forever
.mode_2:
    call render_mode_2_individual_planes
    jmp halt_forever
.mode_3:
    call render_mode_3_tdw_full
    jmp halt_forever
.mode_4:
    call render_mode_4_tdw_selective
    jmp halt_forever
.mode_5:
    call render_mode_5_tcr
    jmp halt_forever
.mode_6:
    call render_mode_6_rmw_all
    jmp halt_forever
.mode_7:
    call render_mode_7_rmw_selective
    jmp halt_forever
.mode_8:
    call render_mode_8_word_ops
    jmp halt_forever
.mode_9:
    call render_mode_9_monochrome
    jmp halt_forever

halt_forever:
    hlt
    jmp halt_forever

interactive_loop:
.loop:
    call render_mode_1_solid_white
    call wait_enter
    call reset_state
    call render_mode_2_individual_planes
    call wait_enter
    call reset_state
    call render_mode_3_tdw_full
    call wait_enter
    call reset_state
    call render_mode_4_tdw_selective
    call wait_enter
    call reset_state
    call render_mode_5_tcr
    call wait_enter
    call reset_state
    call render_mode_6_rmw_all
    call wait_enter
    call reset_state
    call render_mode_7_rmw_selective
    call wait_enter
    call reset_state
    call render_mode_8_word_ops
    call wait_enter
    call reset_state
    call render_mode_9_monochrome
    call wait_enter
    call reset_state
    jmp .loop

; reset_state - Clear monochrome bit, wipe text and graphics VRAM so the next
; render starts from a known state.
reset_state:
    mov al, 0x02        ; port 0x68: ADR=1, DT=0 -> clear bit 1 (color mode)
    out 0x68, al
    call clear_all_planes
    call clear_text_vram
    ret

; Mode 1 - Fill all 4 planes with 0xFF via direct rep stosw.
render_mode_1_solid_white:
    mov al, 0xFF
    jmp fill_all_planes

; Mode 2 - 4 bands of 100 lines: B only (0-99), R only (100-199),
;   G only (200-299), E only (300-399).
render_mode_2_individual_planes:
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

; Mode 3 - GRCG TDW fill all planes. Mode 0x80, tiles B=0xAA, R=0x55,
;   G=0xF0, E=0x0F. CPU data ignored.
render_mode_3_tdw_full:
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

; Mode 4 - GRCG TDW with 4 different plane-disable masks across 4 quadrants.
;   Pre-fills all planes with 0xFF, then each quadrant uses a different
;   mask so that two specific planes preserve 0xFF (disabled) and the
;   other two are overwritten to 0x00 (enabled tile = 0x00). Per pixel:
;     TL (mask 0x8A, R+E disabled): B=0 R=1 G=0 E=1 -> index 10 bright red
;     TR (mask 0x8C, G+E disabled): B=0 R=0 G=1 E=1 -> index 12 bright green
;     BL (mask 0x86, R+G disabled): B=0 R=1 G=1 E=0 -> index 6  yellow
;     BR (mask 0x83, B+R disabled): B=1 R=1 G=0 E=0 -> index 3  magenta
render_mode_4_tdw_selective:
    mov al, 0xFF
    call fill_all_planes

    ; TL quadrant: mask 0x8A.
    mov al, 0x8A
    out GRCG_MODE, al
    xor al, al
    call set_grcg_tiles
    mov di, 0
    mov bp, 200
    xor al, al
    call fill_quadrant_grcg

    ; TR quadrant: mask 0x8C.
    mov al, 0x8C
    out GRCG_MODE, al
    xor al, al
    call set_grcg_tiles
    mov di, 40
    mov bp, 200
    xor al, al
    call fill_quadrant_grcg

    ; BL quadrant: mask 0x86.
    mov al, 0x86
    out GRCG_MODE, al
    xor al, al
    call set_grcg_tiles
    mov di, 200 * BYTES_PER_LINE
    mov bp, 200
    xor al, al
    call fill_quadrant_grcg

    ; BR quadrant: mask 0x83.
    mov al, 0x83
    out GRCG_MODE, al
    xor al, al
    call set_grcg_tiles
    mov di, 200 * BYTES_PER_LINE + 40
    mov bp, 200
    xor al, al
    call fill_quadrant_grcg

    xor al, al
    out GRCG_MODE, al

    ret

; Mode 5 - TCR (Tile Compare Read) - 4 bands.
;   Band 0 (lines 0-99):    B=0xFF, R=0x00, G=0xFF, E=0x00
;   Band 1 (lines 100-199): B=0xFF, R=0xFF, G=0xFF, E=0xFF
;   Band 2 (lines 200-299): B=0x00, R=0x00, G=0x00, E=0x00 (from clear)
;   Band 3 (lines 300-399): B=0xAA, R=0x55, G=0xAA, E=0x55
;   Tiles: B=0xFF, R=0x00, G=0xFF, E=0x00. TCR reads from one byte of
;   each band; results land at RAM 0x0500..0x0503.
render_mode_5_tcr:
    ; Band 0: Lines 0-99: B=0xFF, G=0xFF (R,E = 0x00 from clear).
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

    ; Band 1: Lines 100-199: all 0xFF.
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

    ; Band 2: Lines 200-299: all 0x00 (already clear).

    ; Band 3: Lines 300-399: B=0xAA, R=0x55, G=0xAA, E=0x55.
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

    ; Enable GRCG in TCR mode.
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

    ; Read byte 0 of representative lines via TCR.
    mov ax, VRAM_B
    mov es, ax

    ; Line 50, byte 0 (offset 4000) -> all planes match -> 0xFF.
    mov al, [es:4000]
    mov [ss:0x0500], al

    ; Line 150, byte 0 (offset 12000) -> R,E mismatch -> 0x00.
    mov al, [es:12000]
    mov [ss:0x0501], al

    ; Line 250, byte 0 (offset 20000) -> B,G mismatch -> 0x00.
    mov al, [es:20000]
    mov [ss:0x0502], al

    ; Line 350, byte 0 (offset 28000) -> partial match -> 0xAA.
    mov al, [es:28000]
    mov [ss:0x0503], al

    xor al, al
    out GRCG_MODE, al

    ret

; Mode 6 - GRCG RMW on all planes. Pre-fill all planes with 0x55, mode 0xC0,
;   tiles B=0xFF, R=0x00, G=0xAA, E=0x55, CPU mask 0xF0.
;   new = (cpu & tile) | (~cpu & old)
;     B: (0xF0 & 0xFF) | (0x0F & 0x55) = 0xF5
;     R: (0xF0 & 0x00) | (0x0F & 0x55) = 0x05
;     G: (0xF0 & 0xAA) | (0x0F & 0x55) = 0xA5
;     E: (0xF0 & 0x55) | (0x0F & 0x55) = 0x55
render_mode_6_rmw_all:
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

; Mode 7 - GRCG RMW with 4 different plane-disable masks across 4 quadrants.
;   Pre-fill 0xAA all planes. Tile = 0xFF on all four planes (disabled
;   planes' tiles are ignored). CPU writes 0xFF.
;   RMW formula: new = (cpu & tile) | (~cpu & old) = 0xFF on enabled,
;   unchanged 0xAA on disabled.
;     TL (mask 0xCC, G+E disabled): bit7 all 1 -> 15 bright white;
;                                   bit6 B=R=1,G=E=0 -> 3  magenta
;     TR (mask 0xCA, R+E disabled): bit7 15; bit6 B=G=1,R=E=0 -> 5  cyan
;     BL (mask 0xC6, R+G disabled): bit7 15; bit6 B=E=1,R=G=0 -> 9  bright blue
;     BR (mask 0xC3, B+R disabled): bit7 15; bit6 G=E=1,B=R=0 -> 12 bright green
render_mode_7_rmw_selective:
    mov al, 0xAA
    call fill_all_planes

    ; TL quadrant: mask 0xCC.
    mov al, 0xCC
    out GRCG_MODE, al
    mov al, 0xFF
    call set_grcg_tiles
    mov di, 0
    mov bp, 200
    mov al, 0xFF
    call fill_quadrant_grcg

    ; TR quadrant: mask 0xCA.
    mov al, 0xCA
    out GRCG_MODE, al
    mov al, 0xFF
    call set_grcg_tiles
    mov di, 40
    mov bp, 200
    mov al, 0xFF
    call fill_quadrant_grcg

    ; BL quadrant: mask 0xC6.
    mov al, 0xC6
    out GRCG_MODE, al
    mov al, 0xFF
    call set_grcg_tiles
    mov di, 200 * BYTES_PER_LINE
    mov bp, 200
    mov al, 0xFF
    call fill_quadrant_grcg

    ; BR quadrant: mask 0xC3.
    mov al, 0xC3
    out GRCG_MODE, al
    mov al, 0xFF
    call set_grcg_tiles
    mov di, 200 * BYTES_PER_LINE + 40
    mov bp, 200
    mov al, 0xFF
    call fill_quadrant_grcg

    xor al, al
    out GRCG_MODE, al

    ret

; Mode 8 - Word-width GRCG ops.
;   Top half (lines 0-199): TDW mode 0x80, tiles B=0x33, R=0xCC, G=0x55,
;     E=0xAA, rep stosw fill.
;   Bottom half (lines 200-399): Pre-fill 0xFF, RMW mode 0xC0, tiles
;     B=0xF0, R=0x0F, G=0xFF, E=0x00, CPU data 0xAAAA via rep stosw.
;     new = (0xAA & tile) | (0x55 & 0xFF):
;       B: 0xA0 | 0x55 = 0xF5
;       R: 0x0A | 0x55 = 0x5F
;       G: 0xAA | 0x55 = 0xFF
;       E: 0x00 | 0x55 = 0x55
render_mode_8_word_ops:
    ; Part A: TDW word fill top half (lines 0-199).
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

    ; Part B: RMW word fill bottom half (lines 200-399).
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

; Mode 9 - Monochrome mode test.
;   Lines 0-319 (rows 0-19): G+E planes -> color index 12 (analog green=0xF,
;     bit 3 set -> mono ON, shows text attribute colors).
;   Lines 320-399 (rows 20-24): B-plane only -> color index 1 (analog green=0x0,
;     bit 3 clear -> mono OFF, shows blue).
;   Text: 5 bands of colored attributes (white, red, green, cyan, yellow).
render_mode_9_monochrome:
    ; Enable monochrome mode.
    mov al, 0x03        ; port 0x68: ADR=1, DT=1 -> set bit 1 (monochrome)
    out 0x68, al

    ; Write colored text attributes (5 bands of 5 rows).
    mov ax, TEXT_VRAM
    mov es, ax

    ; Fill character VRAM with spaces.
    xor di, di
    mov ax, 0x0020
    mov cx, 80 * 25
    rep stosw

    ; Fill attribute VRAM with colored attributes per band.
    mov di, 0x2000
    ; Band 0: rows 0-4, color 7 (white)   -> attr 0xE1.
    mov ax, 0x00E1
    mov cx, 80 * 5
    rep stosw
    ; Band 1: rows 5-9, color 2 (red)     -> attr 0x41.
    mov ax, 0x0041
    mov cx, 80 * 5
    rep stosw
    ; Band 2: rows 10-14, color 4 (green) -> attr 0x81.
    mov ax, 0x0081
    mov cx, 80 * 5
    rep stosw
    ; Band 3: rows 15-19, color 5 (cyan)  -> attr 0xA1.
    mov ax, 0x00A1
    mov cx, 80 * 5
    rep stosw
    ; Band 4: rows 20-24, color 6 (yellow) -> attr 0xC1.
    mov ax, 0x00C1
    mov cx, 80 * 5
    rep stosw

    ; Graphics: lines 0-319 G+E planes (index 12), lines 320-399 B-plane (index 1).
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

; set_grcg_tiles - Write byte AL to all 4 GRCG tile registers (B,R,G,E).
set_grcg_tiles:
    out GRCG_TILE, al
    out GRCG_TILE, al
    out GRCG_TILE, al
    out GRCG_TILE, al
    ret

; fill_quadrant_grcg - Write a 40-byte-wide x BP-line region inside plane B's
; segment starting at offset DI.
fill_quadrant_grcg:
    push ax
    mov ax, VRAM_B
    mov es, ax
    pop ax
.line_loop:
    push di
    mov cx, 40              ; 40 bytes = 320 pixels per quadrant line
    rep stosb
    pop di
    add di, BYTES_PER_LINE
    dec bp
    jnz .line_loop
    ret

; fill_plane_rows - Fill rows in plane segment BX with byte AL.
; DX = start line, BP = number of lines.
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

; Pad to fill 96 KB with reset vector at the end.
    times (0x18000 - 16) - ($ - $$) db 0xFF

reset_vector:
    jmp ROM_SEGMENT:entry

    times 0x18000 - ($ - $$) db 0xFF
