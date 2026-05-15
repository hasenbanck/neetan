; debug_pegc.asm - PEGC test ROM for Neetan
; Assembles to a 192KB dual-bank ROM image for PC-9821 machines.
;   Bank 0 (first 96KB, file offset 0x00000): F8000-FFFFF - reset vector only
;   Bank 1 (second 96KB, file offset 0x18000): E8000-F7FFF - all code and data
;
; Reads the mode-selector byte from physical address 0x0500 on startup:
;   0  Interactive: cycle through modes 1..4 with Enter
;   1  PEGC 256-color packed, 640x400 two-screen
;   2  PEGC 256-color packed, 640x480 one-screen (port 09A8h + GDC SYNC 480)
;   3  PEGC 256-color plane mode, 640x400; 8 full-width horizontal strips
;      covering palette indices 0x11, 0x33, 0x55, 0x77, 0x99, 0xBB, 0xDD, 0xFF
;   4  PEGC 256-color plane mode, 640x480; same 8-strip palette layout as
;      mode 3
;
; Non-zero mode values render the page once and HLT, for integration tests.
; Zero (the default for zero-initialized RAM) keeps the original interactive UX.

[bits 16]
[cpu 186]

ROM_SEGMENT     equ 0xE800

TEXT_VRAM       equ 0xA000

PEGC_VRAM_A     equ 0xA800
PEGC_MMIO       equ 0xE000

MMIO_BANK_A8    equ 0x0004
MMIO_MODE       equ 0x0100
MMIO_PLANE_ACC  equ 0x0104
MMIO_ROP_LOW    equ 0x0108
MMIO_MASK_LOW   equ 0x010C
MMIO_MASK_HIGH  equ 0x010E
MMIO_LENGTH     equ 0x0110
MMIO_SHIFT_R    equ 0x0112
MMIO_SHIFT_W    equ 0x0113
MMIO_PATTERN    equ 0x0120

BANK_SIZE       equ 0x8000
BYTES_PER_LINE  equ 80
GRID_ROWS       equ 16
GRID_COLS       equ 16
CELL_WIDTH      equ 40

; Mode-selector byte at physical 0x00500 (zero-initialized RAM => interactive).
MODE_BYTE_ADDR  equ 0x0500

; Bank 0 - mapped to F8000-FFFFF. Reset vector only.
section bank0 start=0 vstart=0

    times (0x18000 - 16) db 0xFF

    db 0xEA
    dw 0x0000
    dw ROM_SEGMENT
    times 0x18000 - ($ - $$) db 0xFF

; Bank 1 - mapped to E8000-F7FFF. All executable code and data.
section bank1 start=0x18000 vstart=0

entry:
    cli

    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00

    mov ds, ax
    mov bl, [MODE_BYTE_ADDR]

    mov ax, ROM_SEGMENT
    mov ds, ax

    mov al, 0x01
    out 0x6A, al
    mov al, 0x07
    out 0x6A, al

    mov al, 0x6B
    out 0xA2, al
    mov al, 0x6B
    out 0x62, al

    call clear_text_vram

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
    call render_mode_1_packed_400
    jmp halt_forever
.mode_2:
    call render_mode_2_packed_480
    jmp halt_forever
.mode_3:
    call render_mode_3_plane_400
    jmp halt_forever
.mode_4:
    call render_mode_4_plane_480
    jmp halt_forever

halt_forever:
    hlt
    jmp halt_forever

interactive_loop:
.loop:
    call render_mode_1_packed_400
    call wait_enter
    call render_mode_2_packed_480
    call wait_enter
    call render_mode_3_plane_400
    call wait_enter
    call render_mode_4_plane_480
    call wait_enter
    jmp .loop

; Mode 1 - PEGC packed 256-color, 640x400 two-screen.
render_mode_1_packed_400:
    call crt_400_line

    mov al, 0x21
    out 0x6A, al

    call pegc_set_packed_mode
    call set_palette_256

    mov al, 0x68
    out 0x6A, al

    mov bp, 25
    call fill_screen_packed
    ret

; Mode 2 - PEGC packed 256-color, 640x480 one-screen.
render_mode_2_packed_480:
    mov al, 0x21
    out 0x6A, al

    call pegc_set_packed_mode
    call set_palette_256

    call crt_480_line
    call gdc_sync_480

    mov al, 0x69
    out 0x6A, al

    mov bp, 30
    call fill_screen_packed
    ret

; Mode 3 - PEGC plane-mode strip fill, 640x400. 8 horizontal strips of 50
;   lines each cover palette indices 0x11, 0x33, 0x55, 0x77, 0x99, 0xBB,
;   0xDD, 0xFF (top to bottom).
render_mode_3_plane_400:
    call crt_400_line

    mov al, 0x21
    out 0x6A, al
    call set_palette_256

    mov al, 0x68
    out 0x6A, al

    mov bp, 50
    call fill_strips_plane
    ret

; Mode 4 - PEGC plane-mode strip fill, 640x480. 8 horizontal strips of 60
;   lines each, same palette indices as mode 3.
render_mode_4_plane_480:
    mov al, 0x21
    out 0x6A, al
    call set_palette_256

    call crt_480_line
    call gdc_sync_480

    mov al, 0x69
    out 0x6A, al

    mov bp, 60
    call fill_strips_plane
    ret

; crt_400_line - Write port 09A8h bit 0 = 0 (24.823 kHz scan).
crt_400_line:
    push ax
    push dx
    mov dx, 0x09A8
    xor al, al
    out dx, al
    pop dx
    pop ax
    ret

; crt_480_line - Write port 09A8h bit 0 = 1 (31.778 kHz scan).
crt_480_line:
    push ax
    push dx
    mov dx, 0x09A8
    mov al, 0x01
    out dx, al
    pop dx
    pop ax
    ret

; pegc_set_packed_mode - Force MMIO E0100h bit 0 to 0 (packed CPU access).
pegc_set_packed_mode:
    push es
    push ax
    mov ax, PEGC_MMIO
    mov es, ax
    xor ax, ax
    mov [es:MMIO_MODE], ax
    pop ax
    pop es
    ret

; gdc_sync_480 - Reprogram GDC slave for 31.778 kHz 480-line timing.
gdc_sync_480:
    mov al, 0x0F
    out 0xA2, al
    xor al, al
    out 0xA0, al
    mov al, 0x26
    out 0xA0, al
    mov al, 0x03
    out 0xA0, al
    mov al, 0x11
    out 0xA0, al
    mov al, 0x03
    out 0xA0, al
    mov al, 0x07
    out 0xA0, al
    mov al, 0xE0
    out 0xA0, al           ; P7 AL_low (480 & 0xFF)
    mov al, 0x65
    out 0xA0, al           ; P8 AL_high | VBP<<2
    ret

; fill_screen_packed - Fill PEGC VRAM with 16x16 grid of 256 colors via
; bank-switched window A (A8000-AFFFF) using MMIO bank select at E0004h.
; Input: BP = lines per row (25 for 400-line, 30 for 480-line).
fill_screen_packed:
    xor dx, dx
    call set_bank_a8
    xor di, di

    mov ax, PEGC_VRAM_A
    mov es, ax

    xor bh, bh
    mov si, GRID_ROWS

.row_loop:
    push bp
    mov cx, bp

.line_loop:
    push cx
    xor bl, bl

.col_loop:
    mov al, bh
    add al, bl
    call write_cell_pixels
    inc bl
    cmp bl, GRID_COLS
    jb .col_loop

    pop cx
    dec cx
    jnz .line_loop

    pop bp
    add bh, 16
    dec si
    jnz .row_loop
    ret

; write_cell_pixels - Write CELL_WIDTH bytes of AL to PEGC VRAM via window A,
; advancing DI and switching to the next bank when the boundary is crossed.
; Input: AL = fill byte, DI = offset in current bank, DX = bank index,
;        ES = PEGC_VRAM_A. Clobbers CX.
write_cell_pixels:
    mov cx, BANK_SIZE
    sub cx, di
    cmp cx, CELL_WIDTH
    jae .no_split

    push cx
    rep stosb

    inc dx
    call set_bank_a8
    xor di, di

    pop cx
    neg cx
    add cx, CELL_WIDTH
    rep stosb
    ret

.no_split:
    mov cx, CELL_WIDTH
    rep stosb

    cmp di, BANK_SIZE
    jb .done
    inc dx
    call set_bank_a8
    xor di, di
.done:
    ret

; set_bank_a8 - Set PEGC bank for window A (A8000-AFFFF). DL = bank (0-15).
set_bank_a8:
    push es
    push ax
    mov ax, PEGC_MMIO
    mov es, ax
    mov [es:MMIO_BANK_A8], dl
    pop ax
    pop es
    ret

; fill_strips_plane - Render 8 full-width horizontal strips via plane-mode
; pattern fill. Each strip is BP rows tall and uses a distinct palette index
; from strip_color_table. Input: BP = rows per strip.
fill_strips_plane:
    call pegc_plane_setup

    xor si, si              ; SI = strip index (0..7)
.strip_loop:
    ; Set pattern color from strip_color_table[SI].
    mov bx, strip_color_table
    add bx, si
    mov al, [cs:bx]
    call pattern_broadcast_color

    ; AX = SI * BP * BYTES_PER_LINE (starting byte offset for this strip).
    mov ax, si
    mul bp                  ; DX:AX = SI * BP (fits in AX for our values)
    mov bx, BYTES_PER_LINE
    mul bx                  ; DX:AX = AX * 80

    ; Fill BP rows starting at offset AX.
    mov cx, bp
    call fill_full_strip_plane

    inc si
    cmp si, 8
    jb .strip_loop
    ret

strip_color_table:
    db 0x11, 0x33, 0x55, 0x77, 0x99, 0xBB, 0xDD, 0xFF

; pegc_plane_setup - Configure MMIO for plane-mode pattern-fill writes.
; ROP register layout: bit 15 = transposed pattern access, bit 12 = ROP enabled,
; bits 11..10 = pattern method 0 (from pattern register), bit 8 = source from
; CPU (skip last-vram-data path), bits 7..0 = ROP code 0xAA (D := P).
pegc_plane_setup:
    push es
    push ax
    mov ax, PEGC_MMIO
    mov es, ax

    mov byte [es:MMIO_MODE], 0x01
    mov byte [es:MMIO_PLANE_ACC], 0x00
    mov word [es:MMIO_MASK_LOW], 0xFFFF
    mov word [es:MMIO_MASK_HIGH], 0xFFFF
    mov word [es:MMIO_LENGTH], 0x0FFF
    mov byte [es:MMIO_SHIFT_R], 0x00
    mov byte [es:MMIO_SHIFT_W], 0x00
    mov word [es:MMIO_ROP_LOW], 0x91AA

    pop ax
    pop es
    ret

; pattern_broadcast_color - Write color in AL to all 16 transposed pattern
; slots (E0120h + 0, +4, +8, ..., +60) so each of the 16 pixels in the pattern
; register has the same 8-bit color.
pattern_broadcast_color:
    push es
    push cx
    push di
    push ax

    mov cx, PEGC_MMIO
    mov es, cx
    mov di, MMIO_PATTERN
    mov cx, 16
.loop:
    mov [es:di], al
    add di, 4
    loop .loop

    pop ax
    pop di
    pop cx
    pop es
    ret

; fill_full_strip_plane - Issue plane-mode word writes for a 640-pixel-wide
; (full-screen-width) strip. Input: AX = starting byte offset,
; CX = rows. Each row is 40 word writes = 80 bytes = 640 pixels.
fill_full_strip_plane:
    push es
    push bx
    push di
    push ax
    push cx

    mov bx, PEGC_VRAM_A
    mov es, bx
    mov di, ax

.row_loop:
    push cx
    push di
    mov cx, 40
    mov ax, 0xFFFF
.word_loop:
    mov [es:di], ax
    add di, 2
    loop .word_loop
    pop di
    add di, BYTES_PER_LINE
    pop cx
    loop .row_loop

    pop cx
    pop ax
    pop di
    pop bx
    pop es
    ret

; set_palette_256 - Program all 256 PEGC palette entries from palette_256.
set_palette_256:
    xor cx, cx

.pal_loop:
    mov al, cl
    out 0xA8, al

    imul bx, cx, 3
    add bx, palette_256

    mov al, [bx]
    out 0xAA, al
    mov al, [bx+1]
    out 0xAC, al
    mov al, [bx+2]
    out 0xAE, al

    inc cx
    cmp cx, 256
    jb .pal_loop
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

; 256-color PEGC palette data: G, R, B per entry (8-bit values).
; Full HSV hue cycle: H = i/256 * 360 deg, S = 1.0, V = 1.0.
; All 256 entries are distinct fully-saturated colors.
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

    times 0x18000 - ($ - $$) db 0xFF
