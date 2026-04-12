//! Memory Control block (MCB) chain management (allocate, free, resize).

pub(crate) mod memory_manager;
mod tlsf;

use crate::{MemoryAccess, OsState, tables::*};

const MCB_TYPE_M: u8 = 0x4D;
const MCB_TYPE_Z: u8 = 0x5A;
const MAX_CHAIN_WALK: usize = 4096;
const CONVENTIONAL_MEMORY_TOTAL_BYTES: u32 = 640 * 1024;
const HMA_TOTAL_BYTES: u32 = 0xFFF0;

// DOS error codes
const ERR_MCB_DESTROYED: u8 = 7;
const ERR_INSUFFICIENT_MEMORY: u8 = 8;
const ERR_INVALID_BLOCK: u8 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryAmount {
    pub(crate) total_bytes: u32,
    pub(crate) used_bytes: u32,
    pub(crate) free_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HmaMemoryAmount {
    pub(crate) total_bytes: u32,
    pub(crate) used_bytes: u32,
    pub(crate) free_bytes: u32,
    pub(crate) allocated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryOverview {
    pub(crate) conventional: MemoryAmount,
    pub(crate) umb: MemoryAmount,
    pub(crate) hma: HmaMemoryAmount,
    pub(crate) ems: MemoryAmount,
    pub(crate) xms: MemoryAmount,
    pub(crate) extended_pool: MemoryAmount,
    pub(crate) largest_conventional_free_bytes: u32,
    pub(crate) largest_umb_free_bytes: u32,
}

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

fn walk_mcb_chain_usage(mem: &dyn MemoryAccess, first_segment: u16) -> (u32, u32) {
    let mut used_paragraphs: u32 = 0;
    let mut free_paragraphs: u32 = 0;
    let mut current = first_segment;

    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            break;
        }

        let owner = read_mcb_owner(mem, current);
        let size = read_mcb_size(mem, current) as u32;
        if owner == MCB_OWNER_FREE {
            free_paragraphs += size;
        } else {
            used_paragraphs += size;
        }

        if block_type == MCB_TYPE_Z {
            break;
        }

        current = current + size as u16 + 1;
    }

    (used_paragraphs, free_paragraphs)
}

fn largest_free_block_paragraphs(mem: &dyn MemoryAccess, first_segment: u16) -> u32 {
    let mut largest: u32 = 0;
    let mut current = first_segment;

    for _ in 0..MAX_CHAIN_WALK {
        let block_type = read_mcb_type(mem, current);
        if !is_valid_mcb_type(block_type) {
            break;
        }

        let owner = read_mcb_owner(mem, current);
        let size = read_mcb_size(mem, current) as u32;
        if owner == MCB_OWNER_FREE && size > largest {
            largest = size;
        }

        if block_type == MCB_TYPE_Z {
            break;
        }

        current = current + size as u16 + 1;
    }

    largest
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

#[derive(Clone, Copy)]
pub(crate) struct InitialMcbLayout {
    pub env_segment: u16,
    pub env_paragraphs: u16,
    pub command_mcb_segment: u16,
    pub psp_segment: u16,
    pub free_mcb_segment: u16,
}

pub(crate) fn initial_mcb_layout(env_paragraphs: u16) -> InitialMcbLayout {
    let env_segment = FIRST_MCB_SEGMENT + 1;
    let command_mcb_segment = env_segment + env_paragraphs;
    let psp_segment = command_mcb_segment + 1;
    let free_mcb_segment = psp_segment + COMMAND_BLOCK_PARAGRAPHS;

    InitialMcbLayout {
        env_segment,
        env_paragraphs,
        command_mcb_segment,
        psp_segment,
        free_mcb_segment,
    }
}

pub(crate) fn collect_memory_overview(state: &OsState, mem: &dyn MemoryAccess) -> MemoryOverview {
    let (conventional_used_paragraphs, _) = walk_mcb_chain_usage(mem, FIRST_MCB_SEGMENT);
    let conventional_used_bytes = conventional_used_paragraphs * 16;
    let conventional = MemoryAmount {
        total_bytes: CONVENTIONAL_MEMORY_TOTAL_BYTES,
        used_bytes: conventional_used_bytes,
        free_bytes: CONVENTIONAL_MEMORY_TOTAL_BYTES.saturating_sub(conventional_used_bytes),
    };

    let memory_manager = state.memory_manager.as_ref();

    let umb = if let Some(manager) = memory_manager {
        if manager.is_umb_enabled() {
            let (umb_used_paragraphs, umb_free_paragraphs) =
                walk_mcb_chain_usage(mem, UMB_FIRST_MCB_SEGMENT);
            let total_bytes = (umb_used_paragraphs + umb_free_paragraphs) * 16;
            let used_bytes = umb_used_paragraphs * 16;
            MemoryAmount {
                total_bytes,
                used_bytes,
                free_bytes: total_bytes.saturating_sub(used_bytes),
            }
        } else {
            MemoryAmount {
                total_bytes: 0,
                used_bytes: 0,
                free_bytes: 0,
            }
        }
    } else {
        MemoryAmount {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
        }
    };

    let hma_total_bytes = if memory_manager.is_some_and(|manager| manager.is_xms_enabled()) {
        HMA_TOTAL_BYTES
    } else {
        0
    };
    let hma_allocated = memory_manager.is_some_and(|manager| manager.hma_is_allocated());
    let hma = HmaMemoryAmount {
        total_bytes: hma_total_bytes,
        used_bytes: if hma_allocated { hma_total_bytes } else { 0 },
        free_bytes: if hma_allocated { 0 } else { hma_total_bytes },
        allocated: hma_allocated,
    };

    let ems_total_bytes = memory_manager.map_or(0, |manager| manager.ems_total_kb() * 1024);
    let ems_free_bytes = memory_manager.map_or(0, |manager| manager.ems_free_kb() * 1024);
    let ems = MemoryAmount {
        total_bytes: ems_total_bytes,
        used_bytes: ems_total_bytes.saturating_sub(ems_free_bytes),
        free_bytes: ems_free_bytes,
    };

    let xms_total_bytes = memory_manager.map_or(0, |manager| manager.xms_total_kb() * 1024);
    let xms_free_bytes = memory_manager.map_or(0, |manager| manager.xms_free_kb() * 1024);
    let xms = MemoryAmount {
        total_bytes: xms_total_bytes,
        used_bytes: xms_total_bytes.saturating_sub(xms_free_bytes),
        free_bytes: xms_free_bytes,
    };

    let extended_pool = if let Some(manager) = memory_manager {
        MemoryAmount {
            total_bytes: manager.extended_pool_total_bytes(),
            used_bytes: manager.extended_pool_used_bytes(),
            free_bytes: manager.extended_pool_free_bytes(),
        }
    } else {
        MemoryAmount {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
        }
    };

    MemoryOverview {
        conventional,
        umb,
        hma,
        ems,
        xms,
        extended_pool,
        largest_conventional_free_bytes: largest_free_block_paragraphs(mem, FIRST_MCB_SEGMENT) * 16,
        largest_umb_free_bytes: if umb.total_bytes > 0 {
            largest_free_block_paragraphs(mem, UMB_FIRST_MCB_SEGMENT) * 16
        } else {
            0
        },
    }
}

pub(crate) fn format_host_memory_overview(overview: &MemoryOverview) -> Vec<String> {
    vec![
        "Memory overview (HLE DOS)".to_string(),
        format!(
            "Conventional: total={} used={} free={}",
            format_debug_size(overview.conventional.total_bytes),
            format_debug_size(overview.conventional.used_bytes),
            format_debug_size(overview.conventional.free_bytes),
        ),
        format!(
            "UMB: total={} used={} free={}",
            format_debug_size(overview.umb.total_bytes),
            format_debug_size(overview.umb.used_bytes),
            format_debug_size(overview.umb.free_bytes),
        ),
        format!(
            "HMA: total={} used={} free={} state={}",
            format_debug_size(overview.hma.total_bytes),
            format_debug_size(overview.hma.used_bytes),
            format_debug_size(overview.hma.free_bytes),
            if overview.hma.allocated {
                "allocated"
            } else {
                "free"
            },
        ),
        format!(
            "EMS: total={} used={} free={}",
            format_debug_size(overview.ems.total_bytes),
            format_debug_size(overview.ems.used_bytes),
            format_debug_size(overview.ems.free_bytes),
        ),
        format!(
            "XMS: total={} used={} free={}",
            format_debug_size(overview.xms.total_bytes),
            format_debug_size(overview.xms.used_bytes),
            format_debug_size(overview.xms.free_bytes),
        ),
        format!(
            "Extended backing pool (EMS+XMS): total={} used={} free={}",
            format_debug_size(overview.extended_pool.total_bytes),
            format_debug_size(overview.extended_pool.used_bytes),
            format_debug_size(overview.extended_pool.free_bytes),
        ),
        "Note: EMS and XMS share the same backing pool.".to_string(),
    ]
}

fn format_debug_size(bytes: u32) -> String {
    if bytes == 0 {
        return "0 bytes".to_string();
    }
    if bytes.is_multiple_of(1024) {
        return format!(
            "{}K ({} bytes)",
            format_debug_number(bytes / 1024),
            format_debug_number(bytes),
        );
    }
    format!("{} bytes", format_debug_number(bytes))
}

fn format_debug_number(value: u32) -> String {
    let digits = value.to_string();
    let mut result = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

pub(crate) fn read_mcb_size_pub(mem: &dyn MemoryAccess, segment: u16) -> u16 {
    read_mcb_size(mem, segment)
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
pub(crate) fn write_initial_mcb_chain(mem: &mut dyn MemoryAccess, layout: InitialMcbLayout) {
    // MCB[0]: environment block
    write_mcb(
        mem,
        FIRST_MCB_SEGMENT,
        0x4D, // 'M'
        MCB_OWNER_DOS,
        layout.env_paragraphs,
        b"SD\0\0\0\0\0\0",
    );

    // MCB[1]: COMMAND.COM (PSP + code stub)
    write_mcb(
        mem,
        layout.command_mcb_segment,
        0x4D, // 'M'
        layout.psp_segment,
        COMMAND_BLOCK_PARAGRAPHS,
        b"COMMAND\0",
    );

    // MCB[2]: free memory (Z block)
    let free_paragraphs = MEMORY_TOP_SEGMENT - layout.free_mcb_segment - 1;
    write_mcb(
        mem,
        layout.free_mcb_segment,
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
    for data_seg in to_free.into_iter().rev() {
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
    for data_seg in to_free.into_iter().rev() {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMemory {
        data: Vec<u8>,
    }

    impl MockMemory {
        fn new(size: usize) -> Self {
            Self {
                data: vec![0; size],
            }
        }
    }

    impl MemoryAccess for MockMemory {
        fn read_byte(&self, address: u32) -> u8 {
            self.data[address as usize]
        }

        fn write_byte(&mut self, address: u32, value: u8) {
            self.data[address as usize] = value;
        }

        fn read_word(&self, address: u32) -> u16 {
            let lo = self.read_byte(address) as u16;
            let hi = self.read_byte(address + 1) as u16;
            lo | (hi << 8)
        }

        fn write_word(&mut self, address: u32, value: u16) {
            self.write_byte(address, value as u8);
            self.write_byte(address + 1, (value >> 8) as u8);
        }

        fn read_block(&self, address: u32, buf: &mut [u8]) {
            let start = address as usize;
            let end = start + buf.len();
            buf.copy_from_slice(&self.data[start..end]);
        }

        fn write_block(&mut self, address: u32, data: &[u8]) {
            let start = address as usize;
            let end = start + data.len();
            self.data[start..end].copy_from_slice(data);
        }
    }

    #[test]
    fn free_process_blocks_frees_in_reverse_to_fully_coalesce() {
        let mut mem = MockMemory::new(0x20000);
        let first_mcb = 0x1000;
        let owner = 0x2222;

        write_mcb(
            &mut mem,
            first_mcb,
            MCB_TYPE_M,
            MCB_OWNER_DOS,
            1,
            b"DOS     ",
        );
        write_mcb(&mut mem, 0x1002, MCB_TYPE_M, owner, 2, b"OWN1    ");
        write_mcb(&mut mem, 0x1005, MCB_TYPE_M, owner, 3, b"OWN2    ");
        write_mcb(&mut mem, 0x1009, MCB_TYPE_Z, MCB_OWNER_FREE, 4, b"FREE    ");

        free_process_blocks(&mut mem, first_mcb, owner);

        assert_eq!(read_mcb_owner(&mem, 0x1002), MCB_OWNER_FREE);
        assert_eq!(read_mcb_type(&mem, 0x1002), MCB_TYPE_Z);
        assert_eq!(
            read_mcb_size(&mem, 0x1002),
            2 + 1 + 3 + 1 + 4,
            "owned blocks should coalesce with the trailing free block into one Z block"
        );
    }

    #[test]
    fn free_process_blocks_tsr_frees_following_blocks_in_reverse_to_fully_coalesce() {
        let mut mem = MockMemory::new(0x20000);
        let first_mcb = 0x1000;
        let owner_psp = 0x1003;

        write_mcb(
            &mut mem,
            first_mcb,
            MCB_TYPE_M,
            MCB_OWNER_DOS,
            1,
            b"DOS     ",
        );
        write_mcb(&mut mem, 0x1002, MCB_TYPE_M, owner_psp, 2, b"PSP     ");
        write_mcb(&mut mem, 0x1005, MCB_TYPE_M, owner_psp, 2, b"AUX1    ");
        write_mcb(&mut mem, 0x1008, MCB_TYPE_M, owner_psp, 3, b"AUX2    ");
        write_mcb(&mut mem, 0x100C, MCB_TYPE_Z, MCB_OWNER_FREE, 4, b"FREE    ");

        free_process_blocks_tsr(&mut mem, first_mcb, owner_psp, 2);

        assert_eq!(
            read_mcb_owner(&mem, 0x1002),
            owner_psp,
            "TSR should retain the PSP block"
        );
        assert_eq!(read_mcb_size(&mem, 0x1002), 2);
        assert_eq!(read_mcb_owner(&mem, 0x1005), MCB_OWNER_FREE);
        assert_eq!(read_mcb_type(&mem, 0x1005), MCB_TYPE_Z);
        assert_eq!(
            read_mcb_size(&mem, 0x1005),
            2 + 1 + 3 + 1 + 4,
            "all non-PSP blocks should coalesce into one trailing free Z block"
        );
    }
}
