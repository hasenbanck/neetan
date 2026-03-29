; debug_pegc.asm - PEGC 256-color test ROM for Neetan
; Assembles to a 192KB dual-bank ROM image for PC-9821 machines.
;   Bank 0 (first 96KB, file offset 0x00000): F8000-FFFFF — reset vector only
;   Bank 1 (second 96KB, file offset 0x18000): E8000-F7FFF — all code and data
; Cycles through 3 fullscreen test patterns with Enter key:
;   Page 1: 16-color analog quadrant pattern (blue, red, green, white)
;   Page 2: PEGC 256-color 640x400 two-screen mode (256 hue blocks)
;   Page 3: PEGC 256-color 640x480 one-screen mode (256 hue blocks)

[bits 16]
[cpu 186]

ROM_SEGMENT     equ 0xE800

; Text VRAM
TEXT_VRAM       equ 0xA000

; VRAM plane segments (16-color mode)
VRAM_B          equ 0xA800
VRAM_R          equ 0xB000
VRAM_G          equ 0xB800
VRAM_E          equ 0xE000

; PEGC VRAM bank window A
PEGC_VRAM_A     equ 0xA800

; PEGC MMIO segment (replaces E-plane when PEGC is active)
PEGC_MMIO       equ 0xE000

; PEGC MMIO register: bank select for A8000 window
MMIO_BANK_A8    equ 0x0004

; PEGC bank size
BANK_SIZE       equ 0x8000

; Screen parameters
BYTES_PER_LINE  equ 80
PLANE_SIZE      equ BYTES_PER_LINE * 400   ; 32000 bytes per plane
GRID_ROWS       equ 16
GRID_COLS       equ 16
CELL_WIDTH      equ 40          ; 640 / 16 = 40 pixels per cell

; Quadrant start offsets (half-width = 40 bytes, half-height = 200 lines)
Q_TL            equ 0
Q_TR            equ 40
Q_BL            equ 200 * BYTES_PER_LINE
Q_BR            equ 200 * BYTES_PER_LINE + 40

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

    ; Enable 16-color analog palette (mode2 bit 0)
    mov al, 0x01
    out 0x6A, al

    ; Set mode change permission (mode2 bit 3, needed for PEGC later)
    mov al, 0x07
    out 0x6A, al

    ; Start GDC slave (graphics)
    mov al, 0x6B
    out 0xA2, al

    ; Start GDC master (text)
    mov al, 0x6B
    out 0x62, al

    ; Clear text VRAM
    call clear_text_vram

    ; Main loop: cycle through 3 pages
.main_loop:
    ; === Page 1: 16-color analog quadrant pattern ===
    ; Disable PEGC (no-op on first iteration, needed on loop back)
    mov al, 0x20
    out 0x6A, al

    call set_16color_palette
    call clear_all_planes
    call draw_16color_pattern
    call wait_enter

    ; === Page 2: PEGC 256-color 640x400 ===
    mov al, 0x21
    out 0x6A, al            ; enable PEGC 256-color
    mov al, 0x63
    out 0x6A, al            ; packed pixel mode
    call set_palette_256

    mov al, 0x68
    out 0x6A, al            ; two-screen mode (640x400)
    mov bp, 25              ; 400 / 16 = 25 lines per hue band
    call fill_screen
    call wait_enter

    ; === Page 3: PEGC 256-color 640x480 ===
    mov al, 0x69
    out 0x6A, al            ; one-screen mode (640x480)
    mov bp, 30              ; 480 / 16 = 30 lines per hue band
    call fill_screen
    call wait_enter

    jmp .main_loop

; ============================================================================
; draw_16color_pattern — Fill 4 quadrants with different 16-color colors.
; TL=1 (blue), TR=2 (red), BL=4 (green), BR=7 (white)
; ============================================================================
draw_16color_pattern:
    ; TL: color 1 = B plane only
    mov ax, VRAM_B
    mov es, ax
    mov di, Q_TL
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ; TR: color 2 = R plane only
    mov ax, VRAM_R
    mov es, ax
    mov di, Q_TR
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ; BL: color 4 = G plane only
    mov ax, VRAM_G
    mov es, ax
    mov di, Q_BL
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ; BR: color 7 = B+R+G planes
    mov ax, VRAM_B
    mov es, ax
    mov di, Q_BR
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    mov ax, VRAM_R
    mov es, ax
    mov di, Q_BR
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    mov ax, VRAM_G
    mov es, ax
    mov di, Q_BR
    mov ax, 0xFFFF
    mov bp, 200
    call fill_half_lines

    ret

; ============================================================================
; fill_half_lines — Fill half-width band (40 bytes/line) via rep stosw
; ES = plane segment, DI = start offset, AX = fill value, BP = line count
; ============================================================================
fill_half_lines:
.loop:
    push di
    mov cx, 20              ; 20 words = 40 bytes per half-line
    rep stosw
    pop di
    add di, BYTES_PER_LINE
    dec bp
    jnz .loop
    ret

; ============================================================================
; fill_screen — Fill PEGC VRAM with 16x16 grid of 256 color blocks
; Input: BP = lines per row (25 for 640x400, 30 for 640x480)
; Uses bank window A (A8000-AFFFF) with MMIO bank switching at E0004h.
; ============================================================================
fill_screen:
    ; Reset to bank 0
    xor dx, dx
    call set_bank_a8
    xor di, di              ; DI = offset within current bank

    ; Set ES = PEGC VRAM window A
    mov ax, PEGC_VRAM_A
    mov es, ax

    xor bh, bh             ; BH = base palette index (row * 16)
    mov si, GRID_ROWS       ; SI = row counter

.row_loop:
    push bp                 ; save lines_per_row
    mov cx, bp              ; CX = line counter

.line_loop:
    push cx
    xor bl, bl              ; BL = column index (0-15)

.col_loop:
    mov al, bh
    add al, bl              ; AL = palette index (row * 16 + col)
    call write_cell_pixels
    inc bl
    cmp bl, GRID_COLS
    jb .col_loop

    pop cx
    dec cx
    jnz .line_loop

    pop bp                  ; restore lines_per_row
    add bh, 16             ; next row base palette
    dec si
    jnz .row_loop
    ret

; ============================================================================
; write_cell_pixels — Write 40 bytes of AL to VRAM, handling bank boundaries
; Input: AL = fill byte, DI = offset in bank, DX = bank, ES = PEGC_VRAM_A
; Output: DI updated, DX updated if bank changed
; Clobbers: CX
; ============================================================================
write_cell_pixels:
    mov cx, BANK_SIZE
    sub cx, di              ; CX = space remaining in current bank
    cmp cx, CELL_WIDTH
    jae .no_split

    ; Split: CX bytes fit in current bank, remainder goes to next bank
    push cx                 ; save first chunk size
    rep stosb               ; write first chunk (AL unchanged by rep stosb)

    ; Switch to next bank
    inc dx
    call set_bank_a8
    xor di, di

    ; Compute remaining = 40 - first_chunk
    pop cx
    neg cx
    add cx, CELL_WIDTH      ; CX = CELL_WIDTH - first_chunk
    rep stosb               ; write remaining bytes
    ret

.no_split:
    mov cx, CELL_WIDTH
    rep stosb

    ; Check if we exactly hit the bank boundary
    cmp di, BANK_SIZE
    jb .done
    inc dx
    call set_bank_a8
    xor di, di
.done:
    ret

; ============================================================================
; Utility: set_bank_a8 — Set PEGC VRAM bank for window A (A8000-AFFFF)
; Input: DL = bank number (0-15)
; Preserves: all registers except flags
; ============================================================================
set_bank_a8:
    push es
    push ax
    mov ax, PEGC_MMIO
    mov es, ax
    mov [es:MMIO_BANK_A8], dl
    pop ax
    pop es
    ret

; ============================================================================
; Utility: set_16color_palette — Set standard 16-color analog palette
; ============================================================================
set_16color_palette:
    xor cx, cx

.pal_loop:
    mov al, cl
    out 0xA8, al

    mov bx, cx
    imul bx, 3
    add bx, palette_16

    mov al, [bx]
    out 0xAA, al            ; green
    mov al, [bx+1]
    out 0xAC, al            ; red
    mov al, [bx+2]
    out 0xAE, al            ; blue

    inc cx
    cmp cx, 16
    jb .pal_loop
    ret

; ============================================================================
; Utility: set_palette_256 — Program 256-entry PEGC palette from table
; ============================================================================
set_palette_256:
    xor cx, cx              ; CX = palette index (0-255)

.pal_loop:
    mov al, cl
    out 0xA8, al            ; select palette index

    imul bx, cx, 3          ; BX = index * 3
    add bx, palette_256     ; BX = pointer into palette table

    mov al, [bx]
    out 0xAA, al            ; green

    mov al, [bx+1]
    out 0xAC, al            ; red

    mov al, [bx+2]
    out 0xAE, al            ; blue

    inc cx
    cmp cx, 256
    jb .pal_loop
    ret

; ============================================================================
; Utility: clear_all_planes — Zero out B, R, G, E planes
; ============================================================================
clear_all_planes:
    mov bx, VRAM_B
    call .fill_zero
    mov bx, VRAM_R
    call .fill_zero
    mov bx, VRAM_G
    call .fill_zero
    mov bx, VRAM_E
.fill_zero:
    mov es, bx
    xor di, di
    xor ax, ax
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

; ============================================================================
; 16-color analog palette data: G, R, B per entry (4-bit values, 0-0x0F)
; ============================================================================
palette_16:
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
; 256-color PEGC palette data: G, R, B per entry (8-bit values).
; Full HSV hue cycle: H = i/256 * 360 deg, S = 1.0, V = 1.0.
; All 256 entries are distinct fully-saturated colors.
; Formula: h6 = i*6, sector = h6/256, f = h6%256, t = f, q = 255-f
;   Sector 0 (red->yellow):   G=t, R=255, B=0
;   Sector 1 (yellow->green): G=255, R=q, B=0
;   Sector 2 (green->cyan):   G=255, R=0, B=t
;   Sector 3 (cyan->blue):    G=q, R=0, B=255
;   Sector 4 (blue->magenta): G=0, R=t, B=255
;   Sector 5 (magenta->red):  G=0, R=255, B=q
; ============================================================================
palette_256:
    db    0,255,  0,    6,255,  0,   12,255,  0,   18,255,  0  ; i=0..3
    db   24,255,  0,   30,255,  0,   36,255,  0,   42,255,  0  ; i=4..7
    db   48,255,  0,   54,255,  0,   60,255,  0,   66,255,  0  ; i=8..11
    db   72,255,  0,   78,255,  0,   84,255,  0,   90,255,  0  ; i=12..15
    db   96,255,  0,  102,255,  0,  108,255,  0,  114,255,  0  ; i=16..19
    db  120,255,  0,  126,255,  0,  132,255,  0,  138,255,  0  ; i=20..23
    db  144,255,  0,  150,255,  0,  156,255,  0,  162,255,  0  ; i=24..27
    db  168,255,  0,  174,255,  0,  180,255,  0,  186,255,  0  ; i=28..31
    db  192,255,  0,  198,255,  0,  204,255,  0,  210,255,  0  ; i=32..35
    db  216,255,  0,  222,255,  0,  228,255,  0,  234,255,  0  ; i=36..39
    db  240,255,  0,  246,255,  0,  252,255,  0,  255,253,  0  ; i=40..43
    db  255,247,  0,  255,241,  0,  255,235,  0,  255,229,  0  ; i=44..47
    db  255,223,  0,  255,217,  0,  255,211,  0,  255,205,  0  ; i=48..51
    db  255,199,  0,  255,193,  0,  255,187,  0,  255,181,  0  ; i=52..55
    db  255,175,  0,  255,169,  0,  255,163,  0,  255,157,  0  ; i=56..59
    db  255,151,  0,  255,145,  0,  255,139,  0,  255,133,  0  ; i=60..63
    db  255,127,  0,  255,121,  0,  255,115,  0,  255,109,  0  ; i=64..67
    db  255,103,  0,  255, 97,  0,  255, 91,  0,  255, 85,  0  ; i=68..71
    db  255, 79,  0,  255, 73,  0,  255, 67,  0,  255, 61,  0  ; i=72..75
    db  255, 55,  0,  255, 49,  0,  255, 43,  0,  255, 37,  0  ; i=76..79
    db  255, 31,  0,  255, 25,  0,  255, 19,  0,  255, 13,  0  ; i=80..83
    db  255,  7,  0,  255,  1,  0,  255,  0,  4,  255,  0, 10  ; i=84..87
    db  255,  0, 16,  255,  0, 22,  255,  0, 28,  255,  0, 34  ; i=88..91
    db  255,  0, 40,  255,  0, 46,  255,  0, 52,  255,  0, 58  ; i=92..95
    db  255,  0, 64,  255,  0, 70,  255,  0, 76,  255,  0, 82  ; i=96..99
    db  255,  0, 88,  255,  0, 94,  255,  0,100,  255,  0,106  ; i=100..103
    db  255,  0,112,  255,  0,118,  255,  0,124,  255,  0,130  ; i=104..107
    db  255,  0,136,  255,  0,142,  255,  0,148,  255,  0,154  ; i=108..111
    db  255,  0,160,  255,  0,166,  255,  0,172,  255,  0,178  ; i=112..115
    db  255,  0,184,  255,  0,190,  255,  0,196,  255,  0,202  ; i=116..119
    db  255,  0,208,  255,  0,214,  255,  0,220,  255,  0,226  ; i=120..123
    db  255,  0,232,  255,  0,238,  255,  0,244,  255,  0,250  ; i=124..127
    db  255,  0,255,  249,  0,255,  243,  0,255,  237,  0,255  ; i=128..131
    db  231,  0,255,  225,  0,255,  219,  0,255,  213,  0,255  ; i=132..135
    db  207,  0,255,  201,  0,255,  195,  0,255,  189,  0,255  ; i=136..139
    db  183,  0,255,  177,  0,255,  171,  0,255,  165,  0,255  ; i=140..143
    db  159,  0,255,  153,  0,255,  147,  0,255,  141,  0,255  ; i=144..147
    db  135,  0,255,  129,  0,255,  123,  0,255,  117,  0,255  ; i=148..151
    db  111,  0,255,  105,  0,255,   99,  0,255,   93,  0,255  ; i=152..155
    db   87,  0,255,   81,  0,255,   75,  0,255,   69,  0,255  ; i=156..159
    db   63,  0,255,   57,  0,255,   51,  0,255,   45,  0,255  ; i=160..163
    db   39,  0,255,   33,  0,255,   27,  0,255,   21,  0,255  ; i=164..167
    db   15,  0,255,    9,  0,255,    3,  0,255,    0,  2,255  ; i=168..171
    db    0,  8,255,    0, 14,255,    0, 20,255,    0, 26,255  ; i=172..175
    db    0, 32,255,    0, 38,255,    0, 44,255,    0, 50,255  ; i=176..179
    db    0, 56,255,    0, 62,255,    0, 68,255,    0, 74,255  ; i=180..183
    db    0, 80,255,    0, 86,255,    0, 92,255,    0, 98,255  ; i=184..187
    db    0,104,255,    0,110,255,    0,116,255,    0,122,255  ; i=188..191
    db    0,128,255,    0,134,255,    0,140,255,    0,146,255  ; i=192..195
    db    0,152,255,    0,158,255,    0,164,255,    0,170,255  ; i=196..199
    db    0,176,255,    0,182,255,    0,188,255,    0,194,255  ; i=200..203
    db    0,200,255,    0,206,255,    0,212,255,    0,218,255  ; i=204..207
    db    0,224,255,    0,230,255,    0,236,255,    0,242,255  ; i=208..211
    db    0,248,255,    0,254,255,    0,255,251,    0,255,245  ; i=212..215
    db    0,255,239,    0,255,233,    0,255,227,    0,255,221  ; i=216..219
    db    0,255,215,    0,255,209,    0,255,203,    0,255,197  ; i=220..223
    db    0,255,191,    0,255,185,    0,255,179,    0,255,173  ; i=224..227
    db    0,255,167,    0,255,161,    0,255,155,    0,255,149  ; i=228..231
    db    0,255,143,    0,255,137,    0,255,131,    0,255,125  ; i=232..235
    db    0,255,119,    0,255,113,    0,255,107,    0,255,101  ; i=236..239
    db    0,255, 95,    0,255, 89,    0,255, 83,    0,255, 77  ; i=240..243
    db    0,255, 71,    0,255, 65,    0,255, 59,    0,255, 53  ; i=244..247
    db    0,255, 47,    0,255, 41,    0,255, 35,    0,255, 29  ; i=248..251
    db    0,255, 23,    0,255, 17,    0,255, 11,    0,255,  5  ; i=252..255

; Pad bank 1 to exactly 96KB
    times 0x18000 - ($ - $$) db 0xFF
