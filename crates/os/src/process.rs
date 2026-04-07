//! PSP creation, EXEC, terminate, process stack.

use crate::{MemoryAccess, tables::*};

/// Writes a 256-byte Program Segment Prefix at the given segment.
///
/// `psp_segment`: segment where the PSP is placed.
/// `parent_psp`: parent PSP segment (equals own segment for COMMAND.COM).
/// `env_segment`: segment of the environment block.
/// `mem_top`: segment of top of available memory (typically 0xA000).
pub(crate) fn write_psp(
    mem: &mut dyn MemoryAccess,
    psp_segment: u16,
    parent_psp: u16,
    env_segment: u16,
    mem_top: u16,
) {
    let base = (psp_segment as u32) << 4;

    // Zero the entire 256-byte PSP.
    let zeros = [0u8; 256];
    mem.write_block(base, &zeros);

    // +0x00: INT 20h instruction (CD 20)
    mem.write_byte(base + PSP_OFF_INT20, 0xCD);
    mem.write_byte(base + PSP_OFF_INT20 + 1, 0x20);

    // +0x02: Segment of memory top
    mem.write_word(base + PSP_OFF_MEM_TOP, mem_top);

    // +0x05: Far call to INT 21h dispatcher (CALL FAR PSP:0050h)
    // Opcode 9A = CALL FAR ptr16:16
    // Target is the INT 21h/RETF stub at PSP:0050h.
    mem.write_byte(base + PSP_OFF_FAR_CALL, 0x9A);
    mem.write_word(base + PSP_OFF_FAR_CALL + 1, 0x0050); // offset
    mem.write_word(base + PSP_OFF_FAR_CALL + 3, psp_segment); // segment

    // +0x0A: Saved INT 22h vector (read from IVT at 0x0088)
    let int22_off = mem.read_word(0x0088);
    let int22_seg = mem.read_word(0x008A);
    write_far_ptr(mem, base + PSP_OFF_INT22_VEC, int22_seg, int22_off);

    // +0x0E: Saved INT 23h vector (read from IVT at 0x008C)
    let int23_off = mem.read_word(0x008C);
    let int23_seg = mem.read_word(0x008E);
    write_far_ptr(mem, base + PSP_OFF_INT23_VEC, int23_seg, int23_off);

    // +0x12: Saved INT 24h vector (read from IVT at 0x0090)
    let int24_off = mem.read_word(0x0090);
    let int24_seg = mem.read_word(0x0092);
    write_far_ptr(mem, base + PSP_OFF_INT24_VEC, int24_seg, int24_off);

    // +0x16: Parent PSP segment
    mem.write_word(base + PSP_OFF_PARENT_PSP, parent_psp);

    // +0x18: Job File Table (20 bytes)
    // Handles 0-4 map to SFT indices 0-4, rest = 0xFF (closed).
    for i in 0..5u32 {
        mem.write_byte(base + PSP_OFF_JFT + i, i as u8);
    }
    for i in 5..20u32 {
        mem.write_byte(base + PSP_OFF_JFT + i, 0xFF);
    }

    // +0x2C: Environment segment
    mem.write_word(base + PSP_OFF_ENV_SEG, env_segment);

    // +0x32: Handle table size (WORD, default 20)
    mem.write_word(base + PSP_OFF_HANDLE_SIZE, 20);

    // +0x34: Far pointer to handle table (default: PSP:0018h)
    write_far_ptr(mem, base + PSP_OFF_HANDLE_PTR, psp_segment, 0x0018);

    // +0x50: INT 21h / RETF stub (CD 21 CB)
    mem.write_byte(base + PSP_OFF_INT21_STUB, 0xCD);
    mem.write_byte(base + PSP_OFF_INT21_STUB + 1, 0x21);
    mem.write_byte(base + PSP_OFF_INT21_STUB + 2, 0xCB);

    // +0x80: Command tail (length=0, terminated by CR)
    mem.write_byte(base + PSP_OFF_CMD_TAIL_LEN, 0x00);
    mem.write_byte(base + PSP_OFF_CMD_TAIL, 0x0D);
}

/// Writes the default COMMAND.COM environment block at the given segment.
///
/// Contents:
///   COMSPEC=Z:\COMMAND.COM\0
///   PATH=Z:\;A:\;B:\;C:\;\0
///   PROMPT=$P$G\0
///   \0                       (double-null terminator)
///   \x01\x00                 (WORD count = 1)
///   Z:\COMMAND.COM\0        (program pathname)
pub(crate) fn write_environment_block(mem: &mut dyn MemoryAccess, env_segment: u16) {
    let base = (env_segment as u32) << 4;

    // Zero the entire environment block first.
    let zeros = [0u8; ENV_BLOCK_PARAGRAPHS as usize * 16];
    mem.write_block(base, &zeros);

    let mut offset = 0u32;

    // COMSPEC=Z:\COMMAND.COM
    let comspec = b"COMSPEC=Z:\\COMMAND.COM";
    mem.write_block(base + offset, comspec);
    offset += comspec.len() as u32;
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // PATH=Z:\;A:\;B:\;C:\;
    let path = b"PATH=Z:\\;A:\\;B:\\;C:\\;";
    mem.write_block(base + offset, path);
    offset += path.len() as u32;
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // PROMPT=$P$G
    let prompt = b"PROMPT=$P$G";
    mem.write_block(base + offset, prompt);
    offset += prompt.len() as u32;
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // Double-null terminator (second NUL after last string's NUL)
    mem.write_byte(base + offset, 0x00);
    offset += 1;

    // WORD count = 1 (number of additional strings following)
    mem.write_word(base + offset, 0x0001);
    offset += 2;

    // Program pathname
    let pathname = b"Z:\\COMMAND.COM";
    mem.write_block(base + offset, pathname);
    offset += pathname.len() as u32;
    mem.write_byte(base + offset, 0x00);
}

/// Writes the COMMAND.COM code stub at PSP:0100h.
///
/// ```text
/// loop:
///     MOV AH, FFh     ; B4 FF
///     INT 21h          ; CD 21
///     JMP SHORT loop   ; EB FA
/// ```
pub(crate) fn write_command_com_stub(mem: &mut dyn MemoryAccess, psp_segment: u16) {
    let base = (psp_segment as u32) << 4;
    let entry = base + 0x0100;

    mem.write_byte(entry, 0xB4); // MOV AH, imm8
    mem.write_byte(entry + 1, 0xFF); // FFh
    mem.write_byte(entry + 2, 0xCD); // INT
    mem.write_byte(entry + 3, 0x21); // 21h
    mem.write_byte(entry + 4, 0xEB); // JMP SHORT
    mem.write_byte(entry + 5, 0xFA); // -6 (back to MOV AH)
}
