; fdd_640k.asm - 640KB FDD HLE ROM stub for Neetan
;
; Assembles to a 4096-byte expansion ROM image mapped at physical 0xD6000.
; This is a PC-9801-09-style BIOS extension shim for early F-class machines:
; the ROM participates in the host BIOS expansion scan, offers a boot entry,
; and routes INT 1Bh floppy BIOS calls to Rust through trap port 0x07ED.
;
; Build: nasm -f bin -o fdd_640k.rom fdd_640k.asm

[bits 16]
[cpu 8086]
[org 0x0000]

TRAP_PORT               equ 0x07ED
MASTER_PIC_CMD          equ 0x00
SLAVE_PIC_CMD           equ 0x08

IPL_SEGMENT             equ 0x1FC0
IPL_SIZE                equ 0x0100
FDD_640K_DEVICE_BASE    equ 0x70

; --- Expansion ROM entry point vector table (offsets 0x00-0x2F) ---

; Entry 0 (offset 0x00): Unused.
    retf
    nop
    nop

; Entry 1 (offset 0x03): Unused.
    retf
    nop
    nop

; Entry 2 (offset 0x06): Unused.
    retf
    nop
    nop

; Expansion ROM signature (offset 0x09).
    db 0x55, 0xAA
    db 0x02

; Entry 4 (offset 0x0C): ROM init callback.
    jmp short init_rom
    nop

; Entry 5 (offset 0x0F): Vector and work-area setup.
    jmp short setup_vectors
    nop

; Entry 6 (offset 0x12): Unused.
    retf
    nop
    nop

; Entry 7 (offset 0x15): Boot from 640KB FDD.
    db 0xE9
    dw boot_fdd - ($ + 2)

; Entry 8 (offset 0x18): INT 1Bh disk BIOS handler.
    jmp short int1b_dispatch_handler
    nop

; Entries 9-15 (offsets 0x1B-0x2F): Unused.
    retf
    nop
    nop
    retf
    nop
    nop
    retf
    nop
    nop
    retf
    nop
    nop
    retf
    nop
    nop
    retf
    nop
    nop
    retf
    nop
    nop

int1b_dispatch_handler:
    mov dx, TRAP_PORT
    out dx, al
    pop ax
    pop bx
    pop cx
    pop dx
    pop bp
    pop es
    pop di
    pop si
    pop ds
    iret

int1b_direct_handler:
    push ax
    and al, 0xF0
    cmp al, 0x70
    je .handle_fdd
    cmp al, 0x50
    je .handle_fdd
    pop ax
    jmp far [cs:old_int1b_vector]
.handle_fdd:
    pop ax
    push ds
    push si
    push di
    push es
    push bp
    push dx
    push cx
    push bx
    push ax
    jmp short int1b_dispatch_handler

init_rom:
    mov byte [bx], 0xD9
    retf

setup_vectors:
    mov ax, cs
    mov bx, [0x006C]
    mov [old_int1b_vector], bx
    mov bx, [0x006E]
    mov [old_int1b_vector + 2], bx
    mov word [0x006C], int1b_direct_handler
    mov [0x006E], ax
    mov word [0x0048], irq_handler
    mov [0x004A], ax
    or byte [0x0480], 0x08
    or byte [0x055D], 0x30
    mov byte [0x0494], 0xC0
    mov byte [0x05CA], 0xFF
    mov word [0x05CC], format_table
    mov [0x05CE], ax
    retf

boot_fdd:
    mov bl, al
    and bl, 0x03
    or bl, FDD_640K_DEVICE_BASE
    mov ax, 0x0600
    mov al, bl
    push bx
    mov cx, IPL_SEGMENT
    mov es, cx
    xor bp, bp
    mov bx, IPL_SIZE
    mov cx, 0x0100
    xor dx, dx
    mov dl, 0x01
    int 0x1B
    pop bx
    jc .not_present
    mov [0x0584], bl
    call far IPL_SEGMENT:0x0000
.not_present:
    retf

irq_handler:
    push ax
    mov al, 0x20
    out SLAVE_PIC_CMD, al
    out MASTER_PIC_CMD, al
    pop ax
    iret

format_table:
    db 0, 0, 1, 1
    db 0, 0, 2, 1
    db 0, 0, 3, 1
    db 0, 0, 4, 1
    db 0, 0, 5, 1
    db 0, 0, 6, 1
    db 0, 0, 7, 1
    db 0, 0, 8, 1
    db 0, 0, 9, 1
    db 0, 0, 10, 1
    db 0, 0, 11, 1
    db 0, 0, 12, 1
    db 0, 0, 13, 1
    db 0, 0, 14, 1
    db 0, 0, 15, 1
    db 0, 0, 16, 1

old_int1b_vector:
    dw 0, 0

    times 4096 - ($ - $$) db 0xFF
