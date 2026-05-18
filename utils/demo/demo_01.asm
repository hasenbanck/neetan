; PC-9801VX/RA 1 KiB boot-sector demo.
; Animated GDC wireframe sphere over a star field.

[bits 16]
[cpu 286]
[org 0x0000]

TEXT_VRAM       equ 0xA000
VRAM_B          equ 0xA800
VRAM_R          equ 0xB000
VRAM_G          equ 0xB800
VRAM_E          equ 0xE000

PAGE_BYTES      equ 16000
WORDS_PER_LINE  equ 40
CENTER_X        equ 320
CENTER_Y        equ 105
SPHERE_X_RADIUS equ 100
SPHERE_Y_RADIUS equ 50
LATITUDE_START  equ 96
LATITUDE_STEP   equ 8
LATITUDE_RINGS  equ 7
LATITUDE_LINES  equ 8
LONGITUDE_STEP  equ 8
LONGITUDE_LINES equ 16
STAR_COUNT      equ 90

start:
    cli
    push cs
    pop ds
    mov ax, cs
    mov ss, ax
    mov sp, 0xFFFE

    xor al, al
    out 0x7C, al

    mov al, 0x01
    out 0x6A, al           ; 16-color analog
    mov al, 0x82
    out 0x6A, al           ; GDC clock bit 9 off
    mov al, 0x84
    out 0x6A, al           ; GDC clock bit 10 off

    mov al, 0x02
    out 0x68, al           ; color graphics
    mov al, 0x09
    out 0x68, al           ; hide odd rasters
    mov al, 0x0F
    out 0x68, al           ; display enable

    call set_palette
    call clear_text
    call gdc_200_line
    mov al, 0x78
    out 0xA2, al
    mov al, 0xFF
    out 0xA0, al
    out 0xA0, al

    xor al, al
    out 0xA6, al
    call init_page
    mov al, 1
    out 0xA6, al
    call init_page

    xor ax, ax
    mov [page], ax
    mov [frac], ax

main_loop:
    mov al, [page]
    xor al, 1
    mov [page], al
    out 0xA6, al

    call clear_r_plane
    call draw_sphere
    call wait_vsync_low
    call wait_vsync_high

    mov al, [page]
    out 0xA4, al

    add byte [frac], 8
    cmp byte [frac], 15
    jb main_loop
    sub byte [frac], 15
    inc byte [angle]
    and byte [angle], 127
    jmp main_loop

set_palette:
    mov al, 0x00
    call pal_black
    mov al, 0x01
    mov bl, 0x0F
    mov bh, 0x0F
    mov cl, 0x0F
    call pal_grb
    mov al, 0x02
    mov bl, 0x0F
    mov bh, 0x0F
    mov cl, 0x00
    call pal_grb
    mov al, 0x03
    call pal_grb
    mov al, 0x04
    xor bl, bl
    mov bh, 0x06
    call pal_grb
    mov al, 0x08
    mov bh, 0x0A
    call pal_grb
    ret

pal_black:
    xor bx, bx
    xor cx, cx
pal_grb:
    out 0xA8, al
    mov al, bl
    out 0xAA, al
    mov al, bh
    out 0xAC, al
    mov al, cl
    out 0xAE, al
    ret

clear_text:
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

gdc_200_line:
    mov al, 0x4B
    out 0xA2, al
    mov al, 1
    out 0xA0, al
    xor al, al
    out 0xA0, al
    out 0xA0, al
    mov al, 0x6B
    out 0xA2, al
    out 0x62, al
    ret

init_page:
    call clear_r_plane
    mov bp, 0xACE1
    mov ax, VRAM_B
    call clear_plane
    call draw_background
    mov ax, VRAM_G
    call clear_plane
    call draw_background
    mov ax, VRAM_E
    call clear_plane

draw_background:
    mov si, STAR_COUNT
.star:
    mov ax, bp
    shr ax, 1
    jnc .seed_ready
    xor ax, 0xB400
.seed_ready:
    mov bp, ax
    mov di, ax
    and di, 0x3FFF
    cmp di, PAGE_BYTES
    jb .offset_ready
    sub di, PAGE_BYTES
.offset_ready:
    mov cl, ah
    and cl, 7
    mov al, 0x80
    shr al, cl
    or [es:di], al
    dec si
    jnz .star
    ret

clear_r_plane:
    mov ax, VRAM_R
clear_plane:
    mov es, ax
    xor di, di
    xor ax, ax
    mov cx, PAGE_BYTES / 2
    rep stosw
    ret

wait_vsync_low:
    in al, 0xA0
    test al, 0x20
    jnz wait_vsync_low
    ret

wait_vsync_high:
    in al, 0xA0
    test al, 0x20
    jz wait_vsync_high
    ret

draw_sphere:
    mov byte [latitude_angle], LATITUDE_START + LATITUDE_STEP
    mov byte [latitude_left], LATITUDE_RINGS
.ring:
    mov byte [longitude_angle], 0
    mov byte [longitude_left], LONGITUDE_LINES
.ring_segment:
    call project_point
    mov [x0], ax
    mov [y0], dx
    add byte [longitude_angle], LONGITUDE_STEP
    and byte [longitude_angle], 127
    call project_point
    mov [x1], ax
    mov [y1], dx
    call draw_line
    dec byte [longitude_left]
    jnz .ring_segment
    add byte [latitude_angle], LATITUDE_STEP
    and byte [latitude_angle], 127
    dec byte [latitude_left]
    jnz .ring

    mov byte [longitude_angle], 0
    mov byte [longitude_left], LONGITUDE_LINES
.meridian:
    mov byte [latitude_angle], LATITUDE_START
    mov byte [latitude_left], LATITUDE_LINES
.meridian_segment:
    call project_point
    mov [x0], ax
    mov [y0], dx
    add byte [latitude_angle], LATITUDE_STEP
    and byte [latitude_angle], 127
    call project_point
    mov [x1], ax
    mov [y1], dx
    call draw_line
    dec byte [latitude_left]
    jnz .meridian_segment
    add byte [longitude_angle], LONGITUDE_STEP
    and byte [longitude_angle], 127
    dec byte [longitude_left]
    jnz .meridian
    ret

project_point:
    mov bl, [latitude_angle]
    xor bh, bh
    mov al, [sines + bx]
    cbw
    mov bx, SPHERE_Y_RADIUS
    imul bx
    sar ax, 7
    add ax, CENTER_Y
    mov [sphere_y], ax

    mov bl, [latitude_angle]
    add bl, 32
    and bl, 127
    xor bh, bh
    mov al, [sines + bx]
    cbw
    mov bx, SPHERE_X_RADIUS
    imul bx
    sar ax, 7
    mov [sphere_radius], ax

    mov bl, [longitude_angle]
    add bl, [angle]
    and bl, 127
    xor bh, bh
    mov al, [sines + bx]
    cbw
    imul word [sphere_radius]
    sar ax, 7
    add ax, CENTER_X
    push ax

    add bl, 32
    and bl, 127
    mov al, [sines + bx]
    cbw
    imul word [sphere_radius]
    sar ax, 9
    add ax, [sphere_y]
    mov dx, ax
    pop ax
    ret

draw_line:
    mov ax, [x1]
    sub ax, [x0]
    mov [dxs], ax
    cwd
    xor ax, dx
    sub ax, dx
    mov [adx], ax

    mov ax, [y1]
    sub ax, [y0]
    mov [dys], ax
    cwd
    xor ax, dx
    sub ax, dx
    mov [ady], ax

    mov ax, [adx]
    mov bx, [ady]
    cmp ax, bx
    jb .y_major
.x_major:
    mov [major], ax
    mov [minor], bx
    cmp word [dxs], 0
    jl .xm_xneg
    mov al, 1
    cmp word [dys], 0
    jge .dir_done
    inc al
    jmp .dir_done
.xm_xneg:
    mov al, 5
    cmp word [dys], 0
    jl .dir_done
    inc al
    jmp .dir_done
.y_major:
    mov [major], bx
    mov [minor], ax
    cmp word [dxs], 0
    jl .ym_xneg
    xor al, al
    cmp word [dys], 0
    jge .dir_done
    mov al, 3
    jmp .dir_done
.ym_xneg:
    mov al, 4
    cmp word [dys], 0
    jl .dir_done
    mov al, 7
.dir_done:
    mov [dir], al

    mov ax, [y0]
    mov bx, WORDS_PER_LINE
    mul bx
    mov bx, [x0]
    shr bx, 4
    add ax, bx
    add ax, 0x8000
    mov [gdc_addr], ax

    mov al, 0x49
    out 0xA2, al
    mov ax, [gdc_addr]
    out 0xA0, al
    mov al, ah
    out 0xA0, al
    mov ax, [x0]
    and al, 0x0F
    shl al, 4
    out 0xA0, al

    mov al, 0x4C
    out 0xA2, al
    mov al, [dir]
    or al, 0x08
    out 0xA0, al

    mov ax, [major]
    call out_gdc_word
    mov ax, [minor]
    shl ax, 1
    sub ax, [major]
    call out_gdc_word
    mov ax, [minor]
    sub ax, [major]
    shl ax, 1
    call out_gdc_word
    mov ax, [minor]
    shl ax, 1
    call out_gdc_word
    xor ax, ax
    call out_gdc_word

    mov al, 0x23
    out 0xA2, al
    mov al, 0x6C
    out 0xA2, al
    ret

out_gdc_word:
    out 0xA0, al
    mov al, ah
    and al, 0x3F
    out 0xA0, al
    ret

sines:
    db    0,    6,   12,   19,   25,   31,   37,   43,   49,   54,   60,   65,   71,   76,   81,   85
    db   90,   94,   98,  102,  106,  109,  112,  115,  117,  120,  122,  123,  125,  126,  126,  127
    db  127,  127,  126,  126,  125,  123,  122,  120,  117,  115,  112,  109,  106,  102,   98,   94
    db   90,   85,   81,   76,   71,   65,   60,   54,   49,   43,   37,   31,   25,   19,   12,    6
    db    0,   -6,  -12,  -19,  -25,  -31,  -37,  -43,  -49,  -54,  -60,  -65,  -71,  -76,  -81,  -85
    db  -90,  -94,  -98, -102, -106, -109, -112, -115, -117, -120, -122, -123, -125, -126, -126, -127
    db -127, -127, -126, -126, -125, -123, -122, -120, -117, -115, -112, -109, -106, -102,  -98,  -94
    db  -90,  -85,  -81,  -76,  -71,  -65,  -60,  -54,  -49,  -43,  -37,  -31,  -25,  -19,  -12,   -6

page:      db 0
angle:     db 0
frac:      db 0
dir:       db 0
latitude_angle:  db 0
latitude_left:   db 0
longitude_angle: db 0
longitude_left:  db 0
x0:        dw 0
y0:        dw 0
x1:        dw 0
y1:        dw 0
dxs:       dw 0
dys:       dw 0
adx:       dw 0
ady:       dw 0
major:     dw 0
minor:     dw 0
gdc_addr:  dw 0
sphere_radius: dw 0
sphere_y:      dw 0

times 1024 - ($ - $$) db 0
