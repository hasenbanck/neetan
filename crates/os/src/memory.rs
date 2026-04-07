//! Memory Control block (MCB) chain management (allocate, free, resize).

use crate::{MemoryAccess, tables::*};

const MCB_TYPE_M: u8 = 0x4D;
const MCB_TYPE_Z: u8 = 0x5A;
const MAX_CHAIN_WALK: usize = 4096;

// DOS error codes
const ERR_MCB_DESTROYED: u8 = 7;
const ERR_INSUFFICIENT_MEMORY: u8 = 8;
const ERR_INVALID_BLOCK: u8 = 9;

fn mcb_addr(segment: u16) -> u32 {
    (segment as u32) << 4
}

fn read_mcb_type(mem: &dyn MemoryAccess, segment: u16) -> u8 {
    mem.read_byte(mcb_addr(segment) + MCB_OFF_TYPE)
}

fn read_mcb_owner(mem: &dyn MemoryAccess, segment: u16) -> u16 {
    mem.read_word(mcb_addr(segment) + MCB_OFF_OWNER)
}

fn read_mcb_size(mem: &dyn MemoryAccess, segment: u16) -> u16 {
    mem.read_word(mcb_addr(segment) + MCB_OFF_SIZE)
}

fn write_mcb_type(mem: &mut dyn MemoryAccess, segment: u16, block_type: u8) {
    mem.write_byte(mcb_addr(segment) + MCB_OFF_TYPE, block_type);
}

fn write_mcb_owner(mem: &mut dyn MemoryAccess, segment: u16, owner: u16) {
    mem.write_word(mcb_addr(segment) + MCB_OFF_OWNER, owner);
}

fn write_mcb_size(mem: &mut dyn MemoryAccess, segment: u16, size: u16) {
    mem.write_word(mcb_addr(segment) + MCB_OFF_SIZE, size);
}

fn clear_mcb_name(mem: &mut dyn MemoryAccess, segment: u16) {
    let addr = mcb_addr(segment);
    for i in 0..8u32 {
        mem.write_byte(addr + MCB_OFF_NAME + i, 0x00);
    }
}

fn is_valid_mcb_type(t: u8) -> bool {
    t == MCB_TYPE_M || t == MCB_TYPE_Z
}

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
///
/// Strategy: 0 = first fit, 1 = best fit, 2 = last fit.
/// Returns Ok(data_segment) on success, where data_segment = MCB segment + 1.
/// Returns Err((error_code, largest_available)) on failure.
pub(crate) fn allocate(
    mem: &mut dyn MemoryAccess,
    first_mcb_segment: u16,
    paragraphs: u16,
    owner: u16,
    strategy: u16,
) -> Result<u16, (u8, u16)> {
    // Walk the chain once to find the best candidate and the largest free block.
    let mut current = first_mcb_segment;
    let mut largest_free: u16 = 0;
    let mut candidate: Option<(u16, u16)> = None; // (segment, size)

    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            return Err((ERR_MCB_DESTROYED, largest_free));
        }

        let block_owner = read_mcb_owner(mem, current);
        let block_size = read_mcb_size(mem, current);

        if block_owner == MCB_OWNER_FREE {
            if block_size > largest_free {
                largest_free = block_size;
            }

            if block_size >= paragraphs {
                let use_this = match strategy {
                    // First fit: take the first match.
                    0 => candidate.is_none(),
                    // Best fit: take the smallest sufficient block.
                    1 => candidate.is_none() || block_size < candidate.unwrap().1,
                    // Last fit: always prefer the later block.
                    2 => true,
                    // Unknown strategy: fall back to first fit.
                    _ => candidate.is_none(),
                };

                if use_this {
                    candidate = Some((current, block_size));
                    // For first fit we can stop scanning immediately.
                    if strategy == 0 {
                        commit_allocation(mem, current, block_size, paragraphs, owner);
                        return Ok(current + 1);
                    }
                }
            }
        }

        if block_type == MCB_TYPE_Z {
            break;
        }

        current = current + block_size + 1;
    }

    // For best-fit and last-fit, commit the chosen candidate after the full walk.
    if let Some((segment, size)) = candidate {
        commit_allocation(mem, segment, size, paragraphs, owner);
        Ok(segment + 1)
    } else {
        Err((ERR_INSUFFICIENT_MEMORY, largest_free))
    }
}

/// Commits an allocation at `segment` by setting the owner and optionally splitting.
fn commit_allocation(
    mem: &mut dyn MemoryAccess,
    segment: u16,
    block_size: u16,
    paragraphs: u16,
    owner: u16,
) {
    let block_type = read_mcb_type(mem, segment);

    if block_size > paragraphs + 1 {
        // Split: create a new free MCB after the allocated block.
        let remainder_segment = segment + 1 + paragraphs;
        let remainder_size = block_size - paragraphs - 1;

        write_mcb_type(mem, remainder_segment, block_type);
        write_mcb_owner(mem, remainder_segment, MCB_OWNER_FREE);
        write_mcb_size(mem, remainder_segment, remainder_size);
        clear_mcb_name(mem, remainder_segment);
        let addr = mcb_addr(remainder_segment);
        mem.write_byte(addr + 5, 0);
        mem.write_byte(addr + 6, 0);
        mem.write_byte(addr + 7, 0);

        // Current block becomes M type (there's a block after it now).
        write_mcb_type(mem, segment, MCB_TYPE_M);
        write_mcb_size(mem, segment, paragraphs);
    }
    // If block_size == paragraphs: exact fit, no split needed.
    // If block_size == paragraphs + 1: can't split (0-paragraph remainder), give the extra.

    write_mcb_owner(mem, segment, owner);
    clear_mcb_name(mem, segment);
}

/// Frees the memory block whose data starts at `data_segment`.
///
/// Returns Ok(()) on success.
/// Returns Err(error_code) on failure.
pub(crate) fn free(
    mem: &mut dyn MemoryAccess,
    first_mcb_segment: u16,
    data_segment: u16,
) -> Result<(), u8> {
    let target_mcb = data_segment.wrapping_sub(1);

    // Walk the chain to verify the target MCB exists.
    let mut current = first_mcb_segment;
    let mut found = false;

    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            return Err(ERR_MCB_DESTROYED);
        }

        if current == target_mcb {
            found = true;
            break;
        }

        if block_type == MCB_TYPE_Z {
            break;
        }

        let block_size = read_mcb_size(mem, current);
        current = current + block_size + 1;
    }

    if !found {
        return Err(ERR_INVALID_BLOCK);
    }

    // Verify block is actually owned (not already free)
    if read_mcb_owner(mem, target_mcb) == MCB_OWNER_FREE {
        return Err(ERR_INVALID_BLOCK);
    }

    // Free the block
    write_mcb_owner(mem, target_mcb, MCB_OWNER_FREE);
    clear_mcb_name(mem, target_mcb);

    // Coalesce with next block if it's also free
    coalesce_forward(mem, target_mcb);

    Ok(())
}

/// Resizes the memory block at `data_segment` to `new_paragraphs`.
///
/// Returns Ok(()) on success.
/// Returns Err((error_code, max_available)) on failure.
pub(crate) fn resize(
    mem: &mut dyn MemoryAccess,
    first_mcb_segment: u16,
    data_segment: u16,
    new_paragraphs: u16,
) -> Result<(), (u8, u16)> {
    let target_mcb = data_segment.wrapping_sub(1);

    // Walk chain to verify MCB exists
    let mut current = first_mcb_segment;
    let mut found = false;

    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            return Err((ERR_MCB_DESTROYED, 0));
        }

        if current == target_mcb {
            found = true;
            break;
        }

        if block_type == MCB_TYPE_Z {
            break;
        }

        let block_size = read_mcb_size(mem, current);
        current = current + block_size + 1;
    }

    if !found {
        return Err((ERR_INVALID_BLOCK, 0));
    }

    let current_size = read_mcb_size(mem, target_mcb);
    let block_type = read_mcb_type(mem, target_mcb);

    if new_paragraphs == current_size {
        return Ok(());
    }

    if new_paragraphs < current_size {
        // Shrink: split the block and create a free remainder
        if current_size > new_paragraphs + 1 {
            let remainder_segment = target_mcb + 1 + new_paragraphs;
            let remainder_size = current_size - new_paragraphs - 1;

            write_mcb_type(mem, remainder_segment, block_type);
            write_mcb_owner(mem, remainder_segment, MCB_OWNER_FREE);
            write_mcb_size(mem, remainder_segment, remainder_size);
            clear_mcb_name(mem, remainder_segment);
            let addr = mcb_addr(remainder_segment);
            mem.write_byte(addr + 5, 0);
            mem.write_byte(addr + 6, 0);
            mem.write_byte(addr + 7, 0);

            write_mcb_type(mem, target_mcb, MCB_TYPE_M);
            write_mcb_size(mem, target_mcb, new_paragraphs);

            // Coalesce new free block with next if also free
            coalesce_forward(mem, remainder_segment);
        }
        // If current_size == new_paragraphs + 1, can't split; leave as-is (waste 1 paragraph)

        Ok(())
    } else {
        // Grow: check if next block is free and has enough space
        if block_type == MCB_TYPE_Z {
            // Z block: can't grow (nothing after it)
            return Err((ERR_INSUFFICIENT_MEMORY, current_size));
        }

        let next_segment = target_mcb + current_size + 1;
        let next_type = read_mcb_type(mem, next_segment);
        if !is_valid_mcb_type(next_type) {
            return Err((ERR_MCB_DESTROYED, current_size));
        }

        let next_owner = read_mcb_owner(mem, next_segment);
        let next_size = read_mcb_size(mem, next_segment);

        if next_owner != MCB_OWNER_FREE {
            // Next block is not free; can't grow
            return Err((ERR_INSUFFICIENT_MEMORY, current_size));
        }

        // Total available = current size + 1 (MCB of next) + next size
        let total_available = current_size as u32 + 1 + next_size as u32;
        if (new_paragraphs as u32) > total_available {
            return Err((ERR_INSUFFICIENT_MEMORY, total_available.min(0xFFFF) as u16));
        }

        // Merge with next block
        let merged_size = (total_available) as u16;

        if new_paragraphs == merged_size {
            // Use entire merged space
            write_mcb_type(mem, target_mcb, next_type);
            write_mcb_size(mem, target_mcb, merged_size);
        } else if merged_size > new_paragraphs + 1 {
            // Split: grow target, create new free remainder
            let remainder_segment = target_mcb + 1 + new_paragraphs;
            let remainder_size = merged_size - new_paragraphs - 1;

            write_mcb_type(mem, remainder_segment, next_type);
            write_mcb_owner(mem, remainder_segment, MCB_OWNER_FREE);
            write_mcb_size(mem, remainder_segment, remainder_size);
            clear_mcb_name(mem, remainder_segment);
            let addr = mcb_addr(remainder_segment);
            mem.write_byte(addr + 5, 0);
            mem.write_byte(addr + 6, 0);
            mem.write_byte(addr + 7, 0);

            write_mcb_type(mem, target_mcb, MCB_TYPE_M);
            write_mcb_size(mem, target_mcb, new_paragraphs);
        } else {
            // merged_size == new_paragraphs + 1: give the extra paragraph
            write_mcb_type(mem, target_mcb, next_type);
            write_mcb_size(mem, target_mcb, merged_size);
        }

        Ok(())
    }
}

/// Frees all MCB blocks owned by `owner_psp`.
///
/// Walks the chain, collects data segments, then frees each one.
/// The existing `free()` coalesces adjacent free blocks automatically.
pub(crate) fn free_process_blocks(
    mem: &mut dyn MemoryAccess,
    first_mcb_segment: u16,
    owner_psp: u16,
) {
    let mut to_free = Vec::new();
    let mut current = first_mcb_segment;
    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            break;
        }
        if read_mcb_owner(mem, current) == owner_psp {
            to_free.push(current + 1);
        }
        if block_type == MCB_TYPE_Z {
            break;
        }
        current = current + read_mcb_size(mem, current) + 1;
    }
    for data_seg in to_free {
        let _ = free(mem, first_mcb_segment, data_seg);
    }
}

/// Frees all MCB blocks owned by `owner_psp` except the PSP's own block,
/// which is resized to `keep_paragraphs` (for TSR termination).
pub(crate) fn free_process_blocks_tsr(
    mem: &mut dyn MemoryAccess,
    first_mcb_segment: u16,
    owner_psp: u16,
    keep_paragraphs: u16,
) {
    let mut to_free = Vec::new();
    let mut current = first_mcb_segment;
    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            break;
        }
        if read_mcb_owner(mem, current) == owner_psp {
            if current + 1 == owner_psp {
                // PSP's own MCB: resize instead of freeing.
                let _ = resize(mem, first_mcb_segment, owner_psp, keep_paragraphs);
            } else {
                to_free.push(current + 1);
            }
        }
        if block_type == MCB_TYPE_Z {
            break;
        }
        current = current + read_mcb_size(mem, current) + 1;
    }
    for data_seg in to_free {
        let _ = free(mem, first_mcb_segment, data_seg);
    }
}

/// Coalesces a free MCB with the next MCB if it is also free.
fn coalesce_forward(mem: &mut dyn MemoryAccess, segment: u16) {
    let block_type = read_mcb_type(mem, segment);
    if block_type != MCB_TYPE_M {
        return; // Z block or invalid: nothing after it
    }

    let block_size = read_mcb_size(mem, segment);
    let next_segment = segment + block_size + 1;

    let next_type = read_mcb_type(mem, next_segment);
    if !is_valid_mcb_type(next_type) {
        return;
    }

    let next_owner = read_mcb_owner(mem, next_segment);
    if next_owner != MCB_OWNER_FREE {
        return;
    }

    // Merge: absorb next block (its MCB + data) into current
    let next_size = read_mcb_size(mem, next_segment);
    let merged_size = block_size + 1 + next_size;
    write_mcb_size(mem, segment, merged_size);
    write_mcb_type(mem, segment, next_type); // Inherit M/Z from next
}
