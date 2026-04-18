; Neetan test386.asm configuration override.
;
; This file is picked up by NASM's include search path (see Makefile) in place
; of the upstream reference/test386.asm-master/src/configuration.asm.
;
; Keep the EQU names in sync with the upstream configuration.asm.

; Diagnostic port for POST codes. Our TestBus traps writes to this port.
POST_PORT equ 0x190

; Parallel / serial output disabled.
LPT_PORT equ 0
LPT_STROBING equ 0
COM_PORT equ 0
COM_PORT_DIV equ 0x0001

; Single-byte ASCII sink used during POST 0xEE. Bochs debug-port convention.
; Our TestBus traps writes to this port and collects bytes for Layer 3.
OUT_PORT equ 0xE9

; Defined-flag coverage only on the first pass. Flip to 1 later to exercise
; i386-specific undefined-flag semantics.
TEST_UNDEF equ 0
CPU_FAMILY equ 3

BOCHS equ 0
IBM_PS1 equ 0
DEBUG equ 0
