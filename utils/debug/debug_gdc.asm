; debug_gdc.asm - GDC planar graphics test ROM for Neetan
; Assembles to a 96 KB ROM image loaded at physical 0xE8000-0xFFFFF
; (single-bank layout for PC-9801F / PC-9801VM class machines).
;
; Reads the mode-selector byte from physical address 0x0500 on startup:
;   0  Interactive: cycle through modes 1..4 with Enter
;   1  8-color digital, 400-line  (lines_per_row = 1)
;   2  16-color analog,  400-line  (lines_per_row = 1)
;   3  8-color digital, 200-line  (lines_per_row = 2)
;   4  16-color analog,  200-line  (lines_per_row = 2)
;
; Non-zero mode values render the page once and HLT, for integration tests.
; Zero (the default for zero-initialized RAM) keeps an interactive UX so
; the ROM stays usable by hand when paired with a keyboard.
;
; Layout per mode: vertical bands of equal width across 640x400 output.
;   8-color  modes:  8 bands x 80 px, color N at band N.
;   16-color modes: 16 bands x 40 px, color N at band N.
;
; Planes (B,R,G,E) are written directly (no GRCG, no EGC). The E plane
; is only mapped while mode2 bit 0 = 1 (analog palette), so the 8-color
; paths leave it alone.

[bits 16]
[cpu 8086]
[org 0x0000]

ROM_SEGMENT     equ 0xE800

TEXT_VRAM       equ 0xA000
VRAM_B          equ 0xA800
VRAM_R          equ 0xB000
VRAM_G          equ 0xB800
VRAM_E          equ 0xE000

PLANE_LINES     equ 400
BYTES_PER_LINE  equ 80

MODE_BYTE_ADDR  equ 0x0500

entry:
    cli

    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00

    mov ds, ax
    mov bl, [MODE_BYTE_ADDR]

    mov ax, ROM_SEGMENT
    mov ds, ax

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
    call render_mode_1_8color_400
    jmp halt_forever
.mode_2:
    call render_mode_2_16color_400
    jmp halt_forever
.mode_3:
    call render_mode_3_8color_200
    jmp halt_forever
.mode_4:
    call render_mode_4_16color_200
    jmp halt_forever

halt_forever:
    hlt
    jmp halt_forever

interactive_loop:
.loop:
    call render_mode_1_8color_400
    call wait_enter
    call render_mode_2_16color_400
    call wait_enter
    call render_mode_3_8color_200
    call wait_enter
    call render_mode_4_16color_200
    call wait_enter
    jmp .loop

; Mode 1 - 8 vertical bands of 80 px, digital palette, lines_per_row = 1.
render_mode_1_8color_400:
    call setup_digital_palette
    call gdc_set_lines_per_row_1
    call paint_8color_bands
    call start_gdc
    ret

; Mode 2 - 16 vertical bands of 40 px, analog palette, lines_per_row = 1.
render_mode_2_16color_400:
    call setup_analog_palette
    call gdc_set_lines_per_row_1
    call paint_16color_bands
    call start_gdc
    ret

; Mode 3 - same VRAM as mode 1, slave GDC lines_per_row = 2.
render_mode_3_8color_200:
    call setup_digital_palette
    call gdc_set_lines_per_row_2
    call paint_8color_bands
    call start_gdc
    ret

; Mode 4 - same VRAM as mode 2, slave GDC lines_per_row = 2.
render_mode_4_16color_200:
    call setup_analog_palette
    call gdc_set_lines_per_row_2
    call paint_16color_bands
    call start_gdc
    ret

; setup_digital_palette - mode2 bit 0 = 0 (digital), write identity
; digital palette so GDC color N maps to the standard BRG bits of N.
; The packed-pair format in port 0xA8/AA/AC/AE follows the scrambled
; layout decoded by `digital_palette_register_index` in the renderer
; (color 0 - reg 3 high, color 7 - reg 0 low, etc.); the values below
; encode (color N -> N) for N = 0..7.
setup_digital_palette:
    mov al, 0x00
    out 0x6A, al

    mov al, 0x37
    out 0xA8, al           ; digital[0]: color 3 high (=3), color 7 low (=7)
    mov al, 0x15
    out 0xAA, al           ; digital[1]: color 1 high (=1), color 5 low (=5)
    mov al, 0x26
    out 0xAC, al           ; digital[2]: color 2 high (=2), color 6 low (=6)
    mov al, 0x04
    out 0xAE, al           ; digital[3]: color 0 high (=0), color 4 low (=4)
    ret

; setup_analog_palette - mode2 bit 0 = 1 (analog), then write a
; 16-entry grayscale ramp so palette[N] = (N, N, N) in 4-bit GRB
; components. After the renderer's *17 expansion this gives RGB
; (N*17, N*17, N*17) for each band - 16 distinct gray levels.
setup_analog_palette:
    mov al, 0x01
    out 0x6A, al

    xor cl, cl
.pal_loop:
    mov al, cl
    out 0xA8, al           ; palette index
    mov al, cl
    out 0xAA, al           ; green = N
    mov al, cl
    out 0xAC, al           ; red = N
    mov al, cl
    out 0xAE, al           ; blue = N
    inc cl
    cmp cl, 16
    jb .pal_loop
    ret

; gdc_set_lines_per_row_1 - slave GDC CCHAR command with P0 = 0.
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

; gdc_set_lines_per_row_2 - slave GDC CCHAR command with P0 = 1.
gdc_set_lines_per_row_2:
    mov al, 0x4B
    out 0xA2, al
    mov al, 0x01           ; P0: lines_per_row - 1 = 1
    out 0xA0, al
    xor al, al
    out 0xA0, al
    xor al, al
    out 0xA0, al
    ret

; start_gdc - START on slave (graphics) and master (text) GDCs.
start_gdc:
    mov al, 0x6B
    out 0xA2, al
    mov al, 0x6B
    out 0x62, al
    ret

; paint_8color_bands - 3 planes (B,R,G), 8 bands x 10 cells each.
paint_8color_bands:
    mov bx, VRAM_B
    mov al, 0x01           ; plane bit mask = bit 0 of index
    mov dh, 10             ; cells per band
    mov dl, 8              ; band count
    call paint_plane

    mov bx, VRAM_R
    mov al, 0x02
    mov dh, 10
    mov dl, 8
    call paint_plane

    mov bx, VRAM_G
    mov al, 0x04
    mov dh, 10
    mov dl, 8
    call paint_plane
    ret

; paint_16color_bands - 4 planes (B,R,G,E), 16 bands x 5 cells each.
paint_16color_bands:
    mov bx, VRAM_B
    mov al, 0x01
    mov dh, 5
    mov dl, 16
    call paint_plane

    mov bx, VRAM_R
    mov al, 0x02
    mov dh, 5
    mov dl, 16
    call paint_plane

    mov bx, VRAM_G
    mov al, 0x04
    mov dh, 5
    mov dl, 16
    call paint_plane

    mov bx, VRAM_E
    mov al, 0x08
    mov dh, 5
    mov dl, 16
    call paint_plane
    ret

; paint_plane - Replicate the row pattern across all PLANE_LINES rasters.
; Each row contains `dl` bands; each band has `dh` cells of 8 pixels (one
; byte per cell). A band's byte is 0xFF when (band_index & plane_mask)
; is set, otherwise 0x00. This lets one helper drive every plane.
;
; Input:
;   BX = plane segment (VRAM_B / VRAM_R / VRAM_G / VRAM_E)
;   AL = plane bit mask (1, 2, 4, 8)
;   DH = cells per band
;   DL = band count (matches `dl` value used in the helper)
paint_plane:
    mov es, bx
    mov ah, al             ; AH = plane bit mask

    xor di, di
    mov bx, PLANE_LINES    ; lines remaining

.line_loop:
    xor cl, cl             ; band index

.band_loop:
    mov al, cl
    test al, ah
    jz .zero_byte
    mov al, 0xFF
    jmp .write_band
.zero_byte:
    xor al, al
.write_band:
    push cx                ; preserve band index across rep stosb
    xor ch, ch
    mov cl, dh             ; CX = cells per band
    rep stosb
    pop cx

    inc cl
    cmp cl, dl
    jb .band_loop

    dec bx
    jnz .line_loop
    ret

; clear_text_vram - Fill text VRAM with spaces + zero attribute.
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
