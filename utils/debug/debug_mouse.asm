; debug_mouse.asm - Mouse integration test ROM for Neetan
; Assembles to a 96KB ROM image loaded at physical 0xE8000–0xFFFFF
; Draws a 4x4 white pixel cursor controlled by the mouse.
; Right Ctrl in the emulator toggles mouse capture.

[bits 16]
[cpu 186]
[org 0x0000]

ROM_SEGMENT     equ 0xE800

; VRAM plane segments
VRAM_B          equ 0xA800

; Text VRAM
TEXT_VRAM       equ 0xA000

; Screen dimensions
BYTES_PER_LINE  equ 80
PLANE_SIZE      equ BYTES_PER_LINE * 400

; GRCG ports
GRCG_MODE       equ 0x7C
GRCG_TILE       equ 0x7E

; Mouse PPI ports
MOUSE_PORT_A    equ 0x7FD9
MOUSE_PORT_C    equ 0x7FDD
MOUSE_CTRL      equ 0x7FDF

; Cursor limits (640-4, 400-4)
CURSOR_X_MAX    equ 636
CURSOR_Y_MAX    equ 396

; Variables in low memory (SS = 0x0000)
VAR_CURSOR_X    equ 0x0500
VAR_CURSOR_Y    equ 0x0502
VAR_OLD_X       equ 0x0504
VAR_OLD_Y       equ 0x0506

; ============================================================================
; Entry point (jumped to from reset vector at end of ROM)
; ============================================================================
entry:
    cli

    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00

    mov ax, ROM_SEGMENT
    mov ds, ax

    ; Enable 16-color analog palette (mode2 bit 0)
    mov al, 0x01
    out 0x6A, al

    ; Set analog palette entries 0 (black) and 15 (white)
    call set_palette

    ; Start GDC slave (graphics)
    mov al, 0x6B
    out 0xA2, al

    ; Start GDC master (text)
    mov al, 0x6B
    out 0x62, al

    ; Clear text and graphics VRAM
    call clear_text_vram
    call clear_all_planes

    ; Display title and instructions
    call draw_text

    ; Initialize mouse PPI: mode 0x93 (port A input, port C upper out / lower in, port B input)
    mov dx, MOUSE_CTRL
    mov al, 0x93
    out dx, al

    ; Disable mouse timer interrupt (set INT# = port C bit 4)
    mov dx, MOUSE_PORT_C
    mov al, 0x10
    out dx, al

    ; Set initial cursor position to screen center
    mov word [ss:VAR_CURSOR_X], 320
    mov word [ss:VAR_CURSOR_Y], 200
    mov word [ss:VAR_OLD_X], 320
    mov word [ss:VAR_OLD_Y], 200

    ; Draw initial cursor
    call draw_cursor

.main_loop:
    call wait_vsync
    call flush_keyboard
    call read_mouse

    ; Check if position changed
    mov ax, [ss:VAR_CURSOR_X]
    cmp ax, [ss:VAR_OLD_X]
    jne .redraw
    mov ax, [ss:VAR_CURSOR_Y]
    cmp ax, [ss:VAR_OLD_Y]
    je .main_loop

.redraw:
    call erase_cursor
    call draw_cursor

    ; Update old position
    mov ax, [ss:VAR_CURSOR_X]
    mov [ss:VAR_OLD_X], ax
    mov ax, [ss:VAR_CURSOR_Y]
    mov [ss:VAR_OLD_Y], ax

    jmp .main_loop

; ============================================================================
; read_mouse - Read mouse deltas via PPI and update cursor position
; ============================================================================
read_mouse:
    ; Clear HC, keep INT# disabled
    mov dx, MOUSE_PORT_C
    mov al, 0x10
    out dx, al

    ; Set HC=1 (rising edge latches and resets counters)
    mov al, 0x90            ; HC=1, SXY=0, SHL=0, INT#=1
    out dx, al

    ; Read X low nibble
    push dx
    mov dx, MOUSE_PORT_A
    in al, dx
    and al, 0x0F
    mov bl, al

    ; Read X high nibble (HC=1, SXY=0, SHL=1, INT#=1)
    mov dx, MOUSE_PORT_C
    mov al, 0xB0
    out dx, al
    mov dx, MOUSE_PORT_A
    in al, dx
    and al, 0x0F
    shl al, 4
    or bl, al               ; BL = X delta (signed byte)

    ; Read Y low nibble (HC=1, SXY=1, SHL=0, INT#=1)
    mov dx, MOUSE_PORT_C
    mov al, 0xD0
    out dx, al
    mov dx, MOUSE_PORT_A
    in al, dx
    and al, 0x0F
    mov bh, al

    ; Read Y high nibble (HC=1, SXY=1, SHL=1, INT#=1)
    mov dx, MOUSE_PORT_C
    mov al, 0xF0
    out dx, al
    mov dx, MOUSE_PORT_A
    in al, dx
    and al, 0x0F
    shl al, 4
    or bh, al               ; BH = Y delta (signed byte)

    ; Clear HC for next frame
    mov dx, MOUSE_PORT_C
    mov al, 0x10
    out dx, al
    pop dx

    ; Apply X delta to cursor position
    mov al, bl
    cbw                      ; AX = sign-extended X delta
    add ax, [ss:VAR_CURSOR_X]
    cmp ax, 0
    jge .x_not_neg
    xor ax, ax
.x_not_neg:
    cmp ax, CURSOR_X_MAX
    jle .x_ok
    mov ax, CURSOR_X_MAX
.x_ok:
    mov [ss:VAR_CURSOR_X], ax

    ; Apply Y delta to cursor position
    mov al, bh
    cbw                      ; AX = sign-extended Y delta
    add ax, [ss:VAR_CURSOR_Y]
    cmp ax, 0
    jge .y_not_neg
    xor ax, ax
.y_not_neg:
    cmp ax, CURSOR_Y_MAX
    jle .y_ok
    mov ax, CURSOR_Y_MAX
.y_ok:
    mov [ss:VAR_CURSOR_Y], ax

    ret

; ============================================================================
; erase_cursor - Erase cursor at old position using GRCG TDW (all planes)
; ============================================================================
erase_cursor:
    ; GRCG TDW mode, tiles = 0x00 (black on all planes)
    mov al, 0x80
    out GRCG_MODE, al
    xor al, al
    out GRCG_TILE, al       ; B = 0
    out GRCG_TILE, al       ; R = 0
    out GRCG_TILE, al       ; G = 0
    out GRCG_TILE, al       ; E = 0

    mov ax, [ss:VAR_OLD_X]
    mov bx, [ss:VAR_OLD_Y]
    call render_cursor

    xor al, al
    out GRCG_MODE, al
    ret

; ============================================================================
; draw_cursor - Draw cursor at current position using GRCG RMW (all planes)
; ============================================================================
draw_cursor:
    ; GRCG RMW mode, tiles = 0xFF (white on all planes)
    mov al, 0xC0
    out GRCG_MODE, al
    mov al, 0xFF
    out GRCG_TILE, al       ; B = 0xFF
    out GRCG_TILE, al       ; R = 0xFF
    out GRCG_TILE, al       ; G = 0xFF
    out GRCG_TILE, al       ; E = 0xFF

    mov ax, [ss:VAR_CURSOR_X]
    mov bx, [ss:VAR_CURSOR_Y]
    call render_cursor

    xor al, al
    out GRCG_MODE, al
    ret

; ============================================================================
; render_cursor - Write 4x4 cursor pattern at (AX=x, BX=y)
;
; GRCG must be configured by caller. Writes go through VRAM_B segment;
; GRCG applies the operation to all enabled planes simultaneously.
;   TDW mode (erase): CPU data ignored, tile value written to all planes.
;   RMW mode (draw):  new = (cpu & tile) | (~cpu & old). With tile=0xFF
;                      and old=0x00: new = cpu (writes the bitmask).
; ============================================================================
render_cursor:
    ; ES = VRAM_B (GRCG target segment)
    push es
    push ax
    mov cx, VRAM_B
    mov es, cx
    pop ax

    ; Mask table index: SI = (x % 8) * 2
    mov cx, ax
    and cx, 7
    shl cx, 1
    mov si, cx

    ; VRAM offset: DI = y * BYTES_PER_LINE + x / 8
    push ax
    mov ax, bx
    mov dx, BYTES_PER_LINE
    mul dx
    mov di, ax
    pop ax
    shr ax, 3
    add di, ax

    ; Write 4 rows of the cursor
    mov cx, 4
.row:
    mov al, [cursor_masks + si]
    mov [es:di], al
    mov al, [cursor_masks + si + 1]
    test al, al
    jz .no_byte2
    mov [es:di + 1], al
.no_byte2:
    add di, BYTES_PER_LINE
    dec cx
    jnz .row

    pop es
    ret

; Cursor bitmask lookup: (mask1, mask2) indexed by (x % 8) * 2
cursor_masks:
    db 0xF0, 0x00           ; x%8=0: ####.... ........
    db 0x78, 0x00           ; x%8=1: .####... ........
    db 0x3C, 0x00           ; x%8=2: ..####.. ........
    db 0x1E, 0x00           ; x%8=3: ...####. ........
    db 0x0F, 0x00           ; x%8=4: ....#### ........
    db 0x07, 0x80           ; x%8=5: .....### #.......
    db 0x03, 0xC0           ; x%8=6: ......## ##......
    db 0x01, 0xE0           ; x%8=7: .......# ###.....

; ============================================================================
; flush_keyboard - Drain any pending scan codes from the keyboard FIFO
; ============================================================================
flush_keyboard:
    in al, 0x43
    test al, 0x02
    jz .done
    in al, 0x41             ; read and discard scan code
    jmp flush_keyboard
.done:
    ret

; ============================================================================
; wait_vsync - Wait for GDC master vertical blanking transition (0 -> 1)
; ============================================================================
wait_vsync:
.wait_active:
    in al, 0x60
    test al, 0x20
    jnz .wait_active        ; spin while VSYNC is still active
.wait_blank:
    in al, 0x60
    test al, 0x20
    jz .wait_blank          ; spin until VSYNC starts
    ret

; ============================================================================
; draw_text - Write title and instructions to text VRAM
; ============================================================================
draw_text:
    mov ax, TEXT_VRAM
    mov es, ax

    ; Row 0: "MOUSE TEST"
    mov si, str_title
    xor di, di
    call print_string

    ; Row 1: "RIGHT CTRL TO CAPTURE"
    mov si, str_help
    mov di, 160             ; (1 * 80) * 2 = 160
    call print_string

    ret

; print_string: NUL-terminated string from DS:SI to text VRAM ES:DI
print_string:
    lodsb
    test al, al
    jz .done
    xor ah, ah
    mov [es:di], ax
    mov word [es:di + 0x2000], 0x00E1
    add di, 2
    jmp print_string
.done:
    ret

str_title:  db 'MOUSE TEST', 0
str_help:   db 'RIGHT CTRL TO CAPTURE', 0

; ============================================================================
; set_palette - Set analog palette entries 0 (black) and 15 (white)
; ============================================================================
set_palette:
    ; Entry 0: Black
    xor al, al
    out 0xA8, al            ; palette index 0
    out 0xAA, al            ; G = 0
    out 0xAC, al            ; R = 0
    out 0xAE, al            ; B = 0

    ; Entry 15: Bright White
    mov al, 15
    out 0xA8, al            ; palette index 15
    mov al, 0x0F
    out 0xAA, al            ; G = 0xF
    out 0xAC, al            ; R = 0xF
    out 0xAE, al            ; B = 0xF

    ret

; ============================================================================
; clear_all_planes - Zero all 4 VRAM planes
; ============================================================================
clear_all_planes:
    mov bx, VRAM_B
    call clear_plane
    mov bx, 0xB000          ; VRAM_R
    call clear_plane
    mov bx, 0xB800          ; VRAM_G
    call clear_plane
    mov bx, 0xE000          ; VRAM_E
    jmp clear_plane

clear_plane:
    mov es, bx
    xor di, di
    xor ax, ax
    mov cx, PLANE_SIZE / 2
    rep stosw
    ret

; ============================================================================
; clear_text_vram - Fill text VRAM with spaces and invisible attributes
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
; Pad to 96KB with reset vector at end
; ============================================================================

    times (0x18000 - 16) - ($ - $$) db 0xFF

reset_vector:
    jmp ROM_SEGMENT:entry

    times 0x18000 - ($ - $$) db 0xFF
