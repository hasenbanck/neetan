//! MCB chain management (allocate, free, resize).

use crate::{MemoryAccess, tables::*};

/// Writes a 16-byte MCB header at the given segment.
pub(crate) fn write_mcb(
    mem: &mut dyn MemoryAccess,
    segment: u16,
    block_type: u8,
    owner: u16,
    size: u16,
    name: &[u8; 8],
) {
    let addr = (segment as u32) << 4;
    mem.write_byte(addr + MCB_OFF_TYPE, block_type);
    mem.write_word(addr + MCB_OFF_OWNER, owner);
    mem.write_word(addr + MCB_OFF_SIZE, size);
    // Reserved bytes (offset 5-7) are zero.
    mem.write_byte(addr + 5, 0);
    mem.write_byte(addr + 6, 0);
    mem.write_byte(addr + 7, 0);
    mem.write_block(addr + MCB_OFF_NAME, name);
}

/// Creates the initial 3-MCB chain after DOS data structures:
///   MCB[0]: environment block (owner=DOS, name="SD")
///   MCB[1]: COMMAND.COM PSP + code stub (owner=PSP_SEGMENT, name="COMMAND\0")
///   MCB[2]: free memory to 640 KB (owner=free)
pub(crate) fn write_initial_mcb_chain(mem: &mut dyn MemoryAccess) {
    // MCB[0]: environment block
    write_mcb(
        mem,
        FIRST_MCB_SEGMENT,
        0x4D, // 'M'
        MCB_OWNER_DOS,
        ENV_BLOCK_PARAGRAPHS,
        b"SD\0\0\0\0\0\0",
    );

    // MCB[1]: COMMAND.COM (PSP + code stub)
    write_mcb(
        mem,
        COMMAND_MCB_SEGMENT,
        0x4D, // 'M'
        PSP_SEGMENT,
        COMMAND_BLOCK_PARAGRAPHS,
        b"COMMAND\0",
    );

    // MCB[2]: free memory (Z block)
    let free_paragraphs = MEMORY_TOP_SEGMENT - FREE_MCB_SEGMENT - 1;
    write_mcb(
        mem,
        FREE_MCB_SEGMENT,
        0x5A, // 'Z'
        MCB_OWNER_FREE,
        free_paragraphs,
        b"\0\0\0\0\0\0\0\0",
    );
}

/// Allocates a block of `paragraphs` paragraphs from the MCB chain.
pub(crate) fn allocate(
    _mem: &mut dyn MemoryAccess,
    _first_mcb_segment: u16,
    _paragraphs: u16,
) -> Result<u16, u8> {
    unimplemented!("MCB allocate")
}

/// Frees the memory block whose data starts at `data_segment`.
pub(crate) fn free(
    _mem: &mut dyn MemoryAccess,
    _first_mcb_segment: u16,
    _data_segment: u16,
) -> Result<(), u8> {
    unimplemented!("MCB free")
}

/// Resizes the memory block at `data_segment` to `new_paragraphs`.
pub(crate) fn resize(
    _mem: &mut dyn MemoryAccess,
    _first_mcb_segment: u16,
    _data_segment: u16,
    _new_paragraphs: u16,
) -> Result<(), u8> {
    unimplemented!("MCB resize")
}
