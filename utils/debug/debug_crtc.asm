; debug_crtc.asm — CRTC text rendering effects test ROM for Neetan
; Assembles to a 96KB ROM image loaded at physical 0xE8000-0xFFFFF
; 8 pages testing attribute effects, colors, and CRTC register behavior

[bits 16]
[cpu 186]
[org 0x0000]

ROM_SEGMENT                 equ 0xE800
TEXT_VRAM                   equ 0xA000
TOTAL_PAGE_COUNT            equ 8

; CRTC register ports
CRTC_PL                     equ 0x70
CRTC_BL                     equ 0x72
CRTC_CL                     equ 0x74
CRTC_SSL                    equ 0x76
CRTC_SUR                    equ 0x78
CRTC_SDR                    equ 0x7A

; CRTC default values (24.8kHz 400-line mode)
DEFAULT_PL                  equ 0x00
DEFAULT_BL                  equ 0x0F
DEFAULT_CL                  equ 0x10
DEFAULT_SSL                 equ 0x00
DEFAULT_SUR                 equ 0x00
DEFAULT_SDR                 equ 0x00

; Variables in stack segment
VAR_CURRENT_PAGE            equ 0x0500
VAR_SUB_PAGE                equ 0x0502

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

    call reset_crtc

    mov byte [ss:VAR_CURRENT_PAGE], 0

.main_loop:
    call render_current_page

    mov al, [ss:VAR_CURRENT_PAGE]
    inc al
    cmp al, TOTAL_PAGE_COUNT
    jb .store_page
    xor al, al

.store_page:
    mov [ss:VAR_CURRENT_PAGE], al
    jmp .main_loop

; ============================================================================
; Page dispatcher
; ============================================================================
render_current_page:
    call reset_crtc

    mov al, [ss:VAR_CURRENT_PAGE]
    cmp al, 0
    je render_page_attributes
    cmp al, 1
    je render_page_colors
    cmp al, 2
    je render_page_cl
    cmp al, 3
    je render_page_bl
    cmp al, 4
    je render_page_pl
    cmp al, 5
    je render_page_ssl
    cmp al, 6
    je render_page_sur
    cmp al, 7
    je render_page_underline_bleed
    ret

; ============================================================================
; Page 1: Text Attribute Effects
; ============================================================================
render_page_attributes:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_1
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Row 3: Normal text
    mov si, label_normal
    mov bh, 3
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 3
    mov bl, 20
    mov ah, 0xE1
    call write_string_at

    ; Row 5: Secret (bit 0 = 0 → hidden)
    mov si, label_secret
    mov bh, 5
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 5
    mov bl, 20
    mov ah, 0xE0
    call write_string_at

    mov si, str_invisible
    mov bh, 5
    mov bl, 42
    mov ah, 0xE1
    call write_string_at

    ; Row 7: Blink (bit 1)
    mov si, label_blink
    mov bh, 7
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 7
    mov bl, 20
    mov ah, 0xE3
    call write_string_at

    ; Row 9: Reverse (bit 2)
    mov si, label_reverse
    mov bh, 9
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 9
    mov bl, 20
    mov ah, 0xE5
    call write_string_at

    ; Row 11: Underline (bit 3)
    mov si, label_underline
    mov bh, 11
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 11
    mov bl, 20
    mov ah, 0xE9
    call write_string_at

    ; Row 13: Vertical Line (bit 4)
    mov si, label_vline
    mov bh, 13
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 13
    mov bl, 20
    mov ah, 0xF1
    call write_string_at

    ; Row 15: Reverse + Underline
    mov si, label_rev_ul
    mov bh, 15
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 15
    mov bl, 20
    mov ah, 0xED
    call write_string_at

    ; Row 17: Blink + Reverse
    mov si, label_blink_rev
    mov bh, 17
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 17
    mov bl, 20
    mov ah, 0xE7
    call write_string_at

    ; Row 19: All effects (secret=visible, blink, reverse, underline, vline)
    mov si, label_all
    mov bh, 19
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 19
    mov bl, 20
    mov ah, 0xFF
    call write_string_at

    call wait_enter
    ret

; ============================================================================
; Page 2: Text Colors
; ============================================================================
render_page_colors:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_2
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Column headers
    mov si, col_normal
    mov bh, 3
    mov bl, 20
    mov ah, 0xE1
    call write_string_at

    mov si, col_reverse
    mov bh, 3
    mov bl, 36
    mov ah, 0xE1
    call write_string_at

    mov si, col_underline
    mov bh, 3
    mov bl, 52
    mov ah, 0xE1
    call write_string_at

    ; Color 0: Black — label in white
    mov si, color_0_label
    mov bh, 5
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, color_sample
    mov bh, 5
    mov bl, 20
    mov ah, 0x01
    call write_string_at

    mov si, color_sample
    mov bh, 5
    mov bl, 36
    mov ah, 0x05
    call write_string_at

    mov si, color_sample
    mov bh, 5
    mov bl, 52
    mov ah, 0x09
    call write_string_at

    ; Color 1: Blue
    mov si, color_1_label
    mov bh, 7
    mov bl, 2
    mov ah, 0x21
    call write_string_at

    mov si, color_sample
    mov bh, 7
    mov bl, 20
    mov ah, 0x21
    call write_string_at

    mov si, color_sample
    mov bh, 7
    mov bl, 36
    mov ah, 0x25
    call write_string_at

    mov si, color_sample
    mov bh, 7
    mov bl, 52
    mov ah, 0x29
    call write_string_at

    ; Color 2: Red
    mov si, color_2_label
    mov bh, 9
    mov bl, 2
    mov ah, 0x41
    call write_string_at

    mov si, color_sample
    mov bh, 9
    mov bl, 20
    mov ah, 0x41
    call write_string_at

    mov si, color_sample
    mov bh, 9
    mov bl, 36
    mov ah, 0x45
    call write_string_at

    mov si, color_sample
    mov bh, 9
    mov bl, 52
    mov ah, 0x49
    call write_string_at

    ; Color 3: Magenta
    mov si, color_3_label
    mov bh, 11
    mov bl, 2
    mov ah, 0x61
    call write_string_at

    mov si, color_sample
    mov bh, 11
    mov bl, 20
    mov ah, 0x61
    call write_string_at

    mov si, color_sample
    mov bh, 11
    mov bl, 36
    mov ah, 0x65
    call write_string_at

    mov si, color_sample
    mov bh, 11
    mov bl, 52
    mov ah, 0x69
    call write_string_at

    ; Color 4: Green
    mov si, color_4_label
    mov bh, 13
    mov bl, 2
    mov ah, 0x81
    call write_string_at

    mov si, color_sample
    mov bh, 13
    mov bl, 20
    mov ah, 0x81
    call write_string_at

    mov si, color_sample
    mov bh, 13
    mov bl, 36
    mov ah, 0x85
    call write_string_at

    mov si, color_sample
    mov bh, 13
    mov bl, 52
    mov ah, 0x89
    call write_string_at

    ; Color 5: Cyan
    mov si, color_5_label
    mov bh, 15
    mov bl, 2
    mov ah, 0xA1
    call write_string_at

    mov si, color_sample
    mov bh, 15
    mov bl, 20
    mov ah, 0xA1
    call write_string_at

    mov si, color_sample
    mov bh, 15
    mov bl, 36
    mov ah, 0xA5
    call write_string_at

    mov si, color_sample
    mov bh, 15
    mov bl, 52
    mov ah, 0xA9
    call write_string_at

    ; Color 6: Yellow
    mov si, color_6_label
    mov bh, 17
    mov bl, 2
    mov ah, 0xC1
    call write_string_at

    mov si, color_sample
    mov bh, 17
    mov bl, 20
    mov ah, 0xC1
    call write_string_at

    mov si, color_sample
    mov bh, 17
    mov bl, 36
    mov ah, 0xC5
    call write_string_at

    mov si, color_sample
    mov bh, 17
    mov bl, 52
    mov ah, 0xC9
    call write_string_at

    ; Color 7: White
    mov si, color_7_label
    mov bh, 19
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, color_sample
    mov bh, 19
    mov bl, 20
    mov ah, 0xE1
    call write_string_at

    mov si, color_sample
    mov bh, 19
    mov bl, 36
    mov ah, 0xE5
    call write_string_at

    mov si, color_sample
    mov bh, 19
    mov bl, 52
    mov ah, 0xE9
    call write_string_at

    call wait_enter
    ret

; ============================================================================
; Page 3: CRTC CL Register (Character Line Count)
; ============================================================================
render_page_cl:
    mov byte [ss:VAR_SUB_PAGE], 0

.cl_loop:
    call clear_text_vram
    call reset_crtc

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_3
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint_sub
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Show current CL value
    mov si, str_cl_eq
    mov bh, 3
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:cl_values + bx]
    call write_decimal_at_cursor

    ; Apply CL value
    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:cl_values + bx]
    out CRTC_CL, al

    ; Fill rows 5-22 with sample text
    mov ch, 5
.cl_fill_loop:
    mov si, sample_row_text
    mov bh, ch
    mov bl, 4
    mov ah, 0xE1
    call write_string_at
    inc ch
    cmp ch, 23
    jb .cl_fill_loop

    call wait_enter

    mov al, [ss:VAR_SUB_PAGE]
    inc al
    cmp al, CL_VALUE_COUNT
    jb .cl_store
    call reset_crtc
    ret

.cl_store:
    mov [ss:VAR_SUB_PAGE], al
    jmp .cl_loop

CL_VALUE_COUNT              equ 5
cl_values:
    db 0x10, 0x0C, 0x08, 0x04, 0x01

; ============================================================================
; Page 4: CRTC BL Register (Body Line Count)
; ============================================================================
render_page_bl:
    mov byte [ss:VAR_SUB_PAGE], 0

.bl_loop:
    call clear_text_vram
    call reset_crtc

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_4
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint_sub
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Show current BL value
    mov si, str_bl_eq
    mov bh, 3
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:bl_values + bx]
    call write_decimal_at_cursor

    ; Apply BL value
    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:bl_values + bx]
    out CRTC_BL, al

    ; Fill rows 5-22 with sample text
    mov ch, 5
.bl_fill_loop:
    mov si, sample_row_text
    mov bh, ch
    mov bl, 4
    mov ah, 0xE1
    call write_string_at
    inc ch
    cmp ch, 23
    jb .bl_fill_loop

    call wait_enter

    mov al, [ss:VAR_SUB_PAGE]
    inc al
    cmp al, BL_VALUE_COUNT
    jb .bl_store
    call reset_crtc
    ret

.bl_store:
    mov [ss:VAR_SUB_PAGE], al
    jmp .bl_loop

BL_VALUE_COUNT              equ 3
bl_values:
    db 0x0F, 0x1F, 0x07

; ============================================================================
; Page 5: CRTC PL Register (Lines Per Row offset)
; ============================================================================
render_page_pl:
    mov byte [ss:VAR_SUB_PAGE], 0

.pl_loop:
    call clear_text_vram
    call reset_crtc

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_5
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint_sub
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Show current PL value
    mov si, str_pl_eq
    mov bh, 3
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:pl_values + bx]
    call write_decimal_at_cursor

    ; Apply PL value
    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:pl_values + bx]
    out CRTC_PL, al

    ; Fill rows 5-22 with sample text
    mov ch, 5
.pl_fill_loop:
    mov si, sample_row_text
    mov bh, ch
    mov bl, 4
    mov ah, 0xE1
    call write_string_at
    inc ch
    cmp ch, 23
    jb .pl_fill_loop

    call wait_enter

    mov al, [ss:VAR_SUB_PAGE]
    inc al
    cmp al, PL_VALUE_COUNT
    jb .pl_store
    call reset_crtc
    ret

.pl_store:
    mov [ss:VAR_SUB_PAGE], al
    jmp .pl_loop

PL_VALUE_COUNT              equ 5
pl_values:
    db 0x00, 0x04, 0x08, 0x1E, 0x1C

; ============================================================================
; Page 6: CRTC SSL Register (Smooth Scroll)
; ============================================================================
render_page_ssl:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_6
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint_sub
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Fill rows 3-24 with numbered rows
    mov ch, 3
.ssl_fill_loop:
    mov si, sample_row_text
    mov bh, ch
    mov bl, 4
    mov ah, 0xE1
    call write_string_at
    inc ch
    cmp ch, 25
    jb .ssl_fill_loop

    mov byte [ss:VAR_SUB_PAGE], 0

.ssl_loop:
    ; Update SSL display
    mov si, str_ssl_eq
    mov bh, 3
    mov bl, 50
    mov ah, 0xC1
    call write_string_at

    mov al, [ss:VAR_SUB_PAGE]
    call write_decimal_at_cursor

    ; Write spaces to clear old value
    mov si, str_spaces
    call write_string_continue

    ; Apply SSL value
    mov al, [ss:VAR_SUB_PAGE]
    out CRTC_SSL, al

    call wait_enter

    mov al, [ss:VAR_SUB_PAGE]
    inc al
    cmp al, 16
    jb .ssl_store

    ; Done cycling, reset
    call reset_crtc
    ret

.ssl_store:
    mov [ss:VAR_SUB_PAGE], al
    jmp .ssl_loop

; ============================================================================
; Page 7: CRTC SUR Register (Scroll Upper Limit)
; ============================================================================
render_page_sur:
    mov byte [ss:VAR_SUB_PAGE], 0

.sur_loop:
    call clear_text_vram
    call reset_crtc

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_7
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint_sub
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Show current SUR value
    mov si, str_sur_eq
    mov bh, 3
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:sur_values + bx]
    call write_decimal_at_cursor

    ; Show fixed rows info
    mov si, str_fixed_rows
    mov bh, 3
    mov bl, 20
    mov ah, 0xE1
    call write_string_at

    ; Calculate and display 32-SUR
    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:sur_values + bx]
    mov cl, 32
    sub cl, al
    mov al, cl
    call write_decimal_at_cursor

    ; Apply SUR value
    mov al, [ss:VAR_SUB_PAGE]
    xor ah, ah
    mov bx, ax
    mov al, [cs:sur_values + bx]
    out CRTC_SUR, al

    ; Also apply SSL=8 to show scrolling effect in lower zone
    mov al, 8
    out CRTC_SSL, al

    ; Fill rows 5-24 with sample text
    mov ch, 5
.sur_fill_loop:
    mov si, sample_row_text
    mov bh, ch
    mov bl, 4
    mov ah, 0xE1
    call write_string_at
    inc ch
    cmp ch, 25
    jb .sur_fill_loop

    call wait_enter

    mov al, [ss:VAR_SUB_PAGE]
    inc al
    cmp al, SUR_VALUE_COUNT
    jb .sur_store
    call reset_crtc
    ret

.sur_store:
    mov [ss:VAR_SUB_PAGE], al
    jmp .sur_loop

SUR_VALUE_COUNT             equ 4
sur_values:
    db 0x00, 0x1F, 0x1E, 0x1C

; ============================================================================
; Page 8: Underline Bleed Effect
; ============================================================================
render_page_underline_bleed:
    call clear_text_vram

    mov ax, TEXT_VRAM
    mov es, ax

    mov si, header_page_8
    mov bh, 0
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    mov si, header_hint
    mov bh, 1
    mov bl, 0
    mov ah, 0xE1
    call write_string_at

    ; Row 3: Continuous underline (all chars underlined)
    mov si, label_continuous_ul
    mov bh, 3
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, sample_text
    mov bh, 4
    mov bl, 4
    mov ah, 0xE9
    call write_string_at

    ; Row 6: Underline bleed (underlined -> not underlined)
    mov si, label_bleed_right
    mov bh, 6
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    ; Write underlined chars followed by normal chars
    mov si, str_underlined_part
    mov bh, 7
    mov bl, 4
    mov ah, 0xE9
    call write_string_at

    mov si, str_normal_part
    mov ah, 0xE1
    call write_string_continue

    ; Row 9: Not underlined -> underlined (no bleed left)
    mov si, label_bleed_left
    mov bh, 9
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    mov si, str_normal_part
    mov bh, 10
    mov bl, 4
    mov ah, 0xE1
    call write_string_at

    mov si, str_underlined_part
    mov ah, 0xE9
    call write_string_continue

    ; Row 12: Color bleed (white underline -> red char receives bleed in red)
    mov si, label_color_bleed
    mov bh, 12
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    ; White underlined text
    mov si, str_white_ul
    mov bh, 13
    mov bl, 4
    mov ah, 0xE9
    call write_string_at

    ; Red non-underlined text (bleed should appear in red)
    mov si, str_red_next
    mov ah, 0x41
    call write_string_continue

    ; Row 15: Different colors underlined side by side
    mov si, label_multi_color_ul
    mov bh, 15
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    ; Blue underlined
    mov si, str_blue
    mov bh, 16
    mov bl, 4
    mov ah, 0x29
    call write_string_at

    ; Red underlined (bleed from blue appears in red)
    mov si, str_red
    mov ah, 0x49
    call write_string_continue

    ; Green underlined (bleed from red appears in green)
    mov si, str_green
    mov ah, 0x89
    call write_string_continue

    ; Yellow underlined (bleed from green appears in yellow)
    mov si, str_yellow
    mov ah, 0xC9
    call write_string_continue

    ; White underlined (bleed from yellow appears in white)
    mov si, str_white
    mov ah, 0xE9
    call write_string_continue

    ; Row 18: Single underlined char (shows bleed into next)
    mov si, label_single_ul
    mov bh, 18
    mov bl, 2
    mov ah, 0xE1
    call write_string_at

    ; One underlined char, rest normal
    mov bh, 19
    mov bl, 4
    mov al, 'X'
    mov ah, 0xE9
    call write_cell_at

    mov si, str_after_single
    mov bh, 19
    mov bl, 5
    mov ah, 0xE1
    call write_string_at

    call wait_enter
    ret

; ============================================================================
; Utility: write_string_at
; ES = text VRAM segment, DS:SI = null-terminated string
; BH = row, BL = column, AH = attribute
; Preserves all registers except DI (left pointing past last char)
; ============================================================================
write_string_at:
    push ax
    push bx
    push cx
    push dx

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
    pop dx
    pop cx
    pop bx
    pop ax
    ret

; ============================================================================
; Utility: write_string_continue
; Continue writing at current DI position with new attribute
; DS:SI = null-terminated string, AH = attribute
; ============================================================================
write_string_continue:
    push ax
    push dx

    mov dl, ah

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
    pop dx
    pop ax
    ret

; ============================================================================
; Utility: write_cell_at
; ES = text VRAM segment
; BH = row, BL = column, AL = char, AH = attribute
; ============================================================================
write_cell_at:
    push bx
    push cx
    push dx
    push di

    mov dl, al
    mov dh, ah

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

    mov [es:di], dl
    mov byte [es:di + 1], 0x00
    mov [es:di + 0x2000], dh
    mov byte [es:di + 0x2001], 0x00

    pop di
    pop dx
    pop cx
    pop bx
    ret

; ============================================================================
; Utility: write_decimal_at_cursor
; Write AL as decimal number (0-255) at current DI position in ES
; Uses attribute 0xC1 (yellow) for the number
; ============================================================================
write_decimal_at_cursor:
    push ax
    push cx
    push dx

    xor ah, ah
    mov cl, 100
    div cl
    mov ch, al
    mov cl, ah

    or ch, ch
    jz .skip_hundreds
    add al, '0'
    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov byte [es:di + 0x2000], 0xC1
    mov byte [es:di + 0x2001], 0x00
    add di, 2
.skip_hundreds:
    mov al, cl
    xor ah, ah
    mov cl, 10
    div cl
    mov cl, ah

    or al, al
    jnz .write_tens
    or ch, ch
    jz .skip_tens
.write_tens:
    add al, '0'
    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov byte [es:di + 0x2000], 0xC1
    mov byte [es:di + 0x2001], 0x00
    add di, 2
.skip_tens:
    mov al, cl
    add al, '0'
    mov [es:di], al
    mov byte [es:di + 1], 0x00
    mov byte [es:di + 0x2000], 0xC1
    mov byte [es:di + 0x2001], 0x00
    add di, 2

    pop dx
    pop cx
    pop ax
    ret

; ============================================================================
; Utility: clear_text_vram
; ============================================================================
clear_text_vram:
    push ax
    push cx
    push di
    push es

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

    pop es
    pop di
    pop cx
    pop ax
    ret

; ============================================================================
; Utility: reset_crtc
; ============================================================================
reset_crtc:
    push ax

    mov al, DEFAULT_PL
    out CRTC_PL, al
    mov al, DEFAULT_BL
    out CRTC_BL, al
    mov al, DEFAULT_CL
    out CRTC_CL, al
    mov al, DEFAULT_SSL
    out CRTC_SSL, al
    mov al, DEFAULT_SUR
    out CRTC_SUR, al
    mov al, DEFAULT_SDR
    out CRTC_SDR, al

    pop ax
    ret

; ============================================================================
; Utility: wait_enter
; ============================================================================
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

; ============================================================================
; String data
; ============================================================================
header_page_1:
    db 'DEBUG_CRTC 1/8 TEXT ATTRIBUTES', 0
header_page_2:
    db 'DEBUG_CRTC 2/8 TEXT COLORS', 0
header_page_3:
    db 'DEBUG_CRTC 3/8 CRTC CL (CHAR LINE)', 0
header_page_4:
    db 'DEBUG_CRTC 4/8 CRTC BL (BODY LINE)', 0
header_page_5:
    db 'DEBUG_CRTC 5/8 CRTC PL (TOP LINE)', 0
header_page_6:
    db 'DEBUG_CRTC 6/8 CRTC SSL (SMOOTH SCROLL)', 0
header_page_7:
    db 'DEBUG_CRTC 7/8 CRTC SUR (UPPER LIMIT)', 0
header_page_8:
    db 'DEBUG_CRTC 8/8 UNDERLINE BLEED', 0

header_hint:
    db 'ENTER: NEXT PAGE', 0
header_hint_sub:
    db 'ENTER: NEXT VALUE (LAST=NEXT PAGE)', 0

sample_text:
    db 'ABCDEFGHIJKLMNOPQRST', 0
sample_row_text:
    db 'The quick brown fox jumps over the lazy dog. 0123456789!', 0

label_normal:
    db 'NORMAL (0xE1):', 0
label_secret:
    db 'SECRET (0xE0):', 0
label_blink:
    db 'BLINK  (0xE3):', 0
label_reverse:
    db 'REVERSE(0xE5):', 0
label_underline:
    db 'UNDERLN(0xE9):', 0
label_vline:
    db 'VLINE  (0xF1):', 0
label_rev_ul:
    db 'REV+UL (0xED):', 0
label_blink_rev:
    db 'BLK+REV(0xE7):', 0
label_all:
    db 'ALL    (0xFF):', 0

str_invisible:
    db '<- HIDDEN TEXT HERE', 0

col_normal:
    db 'NORMAL', 0
col_reverse:
    db 'REVERSE', 0
col_underline:
    db 'UNDERLINE', 0

color_0_label:
    db '0: BLACK   (0x01)', 0
color_1_label:
    db '1: BLUE    (0x21)', 0
color_2_label:
    db '2: RED     (0x41)', 0
color_3_label:
    db '3: MAGENTA (0x61)', 0
color_4_label:
    db '4: GREEN   (0x81)', 0
color_5_label:
    db '5: CYAN    (0xA1)', 0
color_6_label:
    db '6: YELLOW  (0xC1)', 0
color_7_label:
    db '7: WHITE   (0xE1)', 0

color_sample:
    db 'SAMPLE TEXT', 0

str_cl_eq:
    db 'CL=', 0
str_bl_eq:
    db 'BL=', 0
str_pl_eq:
    db 'PL=', 0
str_ssl_eq:
    db 'SSL=', 0
str_sur_eq:
    db 'SUR=', 0
str_fixed_rows:
    db 'FIXED ROWS=', 0

str_spaces:
    db '   ', 0

label_continuous_ul:
    db 'CONTINUOUS UNDERLINE:', 0
label_bleed_right:
    db 'BLEED RIGHT (UL->NORMAL):', 0
label_bleed_left:
    db 'NO BLEED LEFT (NORMAL->UL):', 0
label_color_bleed:
    db 'COLOR BLEED (WHITE UL -> RED CHAR):', 0
label_multi_color_ul:
    db 'MULTI-COLOR UNDERLINE:', 0
label_single_ul:
    db 'SINGLE UNDERLINED CHAR:', 0

str_underlined_part:
    db 'UNDERLINED', 0
str_normal_part:
    db 'NORMAL', 0
str_white_ul:
    db 'WHITE-UL', 0
str_red_next:
    db 'RED-NEXT', 0

str_blue:
    db 'BLUE', 0
str_red:
    db 'RED', 0
str_green:
    db 'GREEN', 0
str_yellow:
    db 'YELLOW', 0
str_white:
    db 'WHITE', 0

str_after_single:
    db '<- BLEED VISIBLE IN FIRST 4 PIXELS', 0

    times (0x18000 - 16) - ($ - $$) db 0xFF

reset_vector:
    jmp ROM_SEGMENT:entry

    times 0x18000 - ($ - $$) db 0xFF
