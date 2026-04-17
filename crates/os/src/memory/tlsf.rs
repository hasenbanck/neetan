//! Virtual Two-Level Segregated Fit (TLSF) memory allocator.
//!
//! This allocator provides O(1) time complexity for both allocation and deallocation,
//! making it suitable for real-time systems where predictable performance is critical.
//!
//! It's main limitation are:
//!     - The allocator can hold at most u32::MAX elements.
//!     - Maximum 65,535 simultaneous allocations: Uses u16 indices for memory efficiency.

use std::cmp;

/// Result of an allocation request.
///
/// Contains the offset where the allocation starts in the virtual memory pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Allocation {
    /// Offset in the memory pool where this allocation starts.
    offset: u32,
    /// Node index (used for freeing).
    node_index: u16,
}

impl Allocation {
    #[inline(always)]
    pub(crate) fn offset(&self) -> u32 {
        self.offset
    }
}

// Log2 of number of linear subdivisions of block sizes.
const SL_INDEX_COUNT_LOG2: u32 = 4; // 16 subdivisions
const SL_INDEX_COUNT: usize = 1 << SL_INDEX_COUNT_LOG2;

// Alignment constants (4 elements for 32-bit offsets).
const ALIGN_SIZE_LOG2: u32 = 2;
const ALIGN_SIZE: u32 = 1 << ALIGN_SIZE_LOG2;
pub(crate) const ALIGN_MASK: u64 = (ALIGN_SIZE as u64) - 1;

// FL_INDEX_MAX supports up to 4GB (u32 limit).
const FL_INDEX_MAX: usize = 30;
const FL_INDEX_SHIFT: usize = (SL_INDEX_COUNT_LOG2 + ALIGN_SIZE_LOG2) as usize;
const FL_INDEX_COUNT: usize = FL_INDEX_MAX - FL_INDEX_SHIFT + 1;

// Minimum block size.
const BLOCK_SIZE_MIN: u32 = 16;
const SMALL_BLOCK_SIZE: u32 = 1 << FL_INDEX_SHIFT;

/// Sentinel value indicating an unused node index.
const NODE_UNUSED: u16 = u16::MAX;

const MAX_ALLOCATIONS: u16 = u16::MAX;

/// Internal node representing a memory region (free or used).
///
/// Nodes are organized into two linked list structures:
/// - Free lists: Link nodes of similar size for allocation (TLSF bins).
/// - Physical neighbor lists: Link spatially adjacent nodes for coalescing.
#[derive(Debug, Clone, Copy)]
struct BlockNode {
    /// Offset of this region in the virtual memory pool.
    offset: u32,
    /// Size of the block with is_free flag packed in bit 0.
    /// Bit 0: 1 = free, 0 = used
    /// Bits 1-31: Actual size (aligned to 4 elements)
    size_and_flags: u32,

    /// Previous spatially adjacent node (NODE_UNUSED if none).
    phys_prev: u16,
    /// Next spatially adjacent node (NODE_UNUSED if none).
    phys_next: u16,

    /// Previous node in the free list (NODE_UNUSED if none).
    list_prev: u16,
    /// Next node in the free list (NODE_UNUSED if none).
    list_next: u16,
}

impl BlockNode {
    /// Create a new BlockNode with given parameters.
    #[inline(always)]
    fn new(offset: u32, size: u32, is_free: bool) -> Self {
        debug_assert_eq!(size & 1, 0, "Size must be aligned to ALIGN_SIZE");
        Self {
            offset,
            size_and_flags: size | (is_free as u32),
            phys_prev: NODE_UNUSED,
            phys_next: NODE_UNUSED,
            list_prev: NODE_UNUSED,
            list_next: NODE_UNUSED,
        }
    }

    /// Get the actual size of this block.
    #[inline(always)]
    fn size(&self) -> u32 {
        self.size_and_flags & !1
    }

    /// Set the size of this block.
    #[inline(always)]
    fn set_size(&mut self, size: u32) {
        debug_assert_eq!(size & 1, 0, "Size must be aligned to ALIGN_SIZE");
        self.size_and_flags = (self.size_and_flags & 1) | size;
    }

    /// Check if this block is free.
    #[inline(always)]
    fn is_free(&self) -> bool {
        (self.size_and_flags & 1) != 0
    }

    /// Set the free status of this block.
    #[inline(always)]
    fn set_free(&mut self, is_free: bool) {
        if is_free {
            self.size_and_flags |= 1;
        } else {
            self.size_and_flags &= !1;
        }
    }
}

const _: () = assert!(size_of::<BlockNode>() == 16);
const _: () = assert!(ALIGN_SIZE == 4);

/// A Two-Level Segregated Fit (TLSF) memory allocator with virtual addressing.
///
/// This allocator provides O(1) time complexity for both allocation and deallocation,
/// making it suitable for real-time systems where predictable performance is critical.
///
/// # Allocation Limits
///
/// The allocator uses `u16` indices to track memory blocks, limiting the maximum number
/// of simultaneous allocations to 65,535 (u16::MAX). This is a deliberate trade-off
/// to reduce memory overhead while supporting typical GPU memory management workloads.
///
/// # Architecture
///
/// The allocator uses a segregated fit mechanism with a two-level bitmap to index free blocks:
///
/// 1. First Level (FL): Classifies blocks by orders of magnitude (powers of 2).
/// 2. Second Level (SL): Linearly subdivides each FL bin into `SL_INDEX_COUNT` slots.
///
/// This structure allows the allocator to locate a suitable free block using constant-time
/// bitwise operations (FFS/FLS) rather than searching through lists.
#[derive(Debug)]
pub(crate) struct TlsfAllocator {
    /// Total size of the virtual memory pool.
    size: u32,
    /// First Level bitmap (FLI)
    fl_bitmap: u32,
    /// Second Level bitmaps (SLI)
    sl_bitmap: [u32; FL_INDEX_COUNT],
    /// Heads of the free lists.
    /// Stores indices into `nodes` for the first free block in each list.
    /// Indexed as `fl * SL_INDEX_COUNT + sl`.
    blocks: Vec<u16>,
    /// Metadata storage for all blocks.
    nodes: Vec<BlockNode>,
    /// Stack of free node indices.
    free_node_indices: Vec<u16>,
    /// Current top of the free_node_indices stack.
    free_offset: u16,
    /// Current active allocations count.
    alloc_count: u32,
}

impl TlsfAllocator {
    /// Create a new allocator with the default maximum allocation count.
    ///
    /// # Arguments
    pub(crate) fn new(size: u32) -> Self {
        let aligned_size = (size / ALIGN_SIZE) * ALIGN_SIZE;

        let mut allocator = Self {
            size: aligned_size,
            fl_bitmap: 0,
            sl_bitmap: [0; FL_INDEX_COUNT],
            blocks: vec![NODE_UNUSED; SL_INDEX_COUNT * FL_INDEX_COUNT],
            nodes: Vec::with_capacity(MAX_ALLOCATIONS as usize),
            free_node_indices: Vec::with_capacity(MAX_ALLOCATIONS as usize),
            free_offset: 0,
            alloc_count: 0,
        };

        allocator.reset();
        allocator
    }

    /// Reset the allocator to its initial state.
    ///
    /// This operation rebuilds the internal data structures and returns the
    /// allocator to its initial state with the entire pool free.
    pub(crate) fn reset(&mut self) {
        self.fl_bitmap = 0;
        self.sl_bitmap = [0; FL_INDEX_COUNT];
        self.blocks.fill(NODE_UNUSED);
        self.alloc_count = 0;
        self.free_offset = MAX_ALLOCATIONS - 1;

        self.nodes.clear();
        self.free_node_indices.clear();

        // Initialize all nodes.
        for _ in 0..MAX_ALLOCATIONS {
            self.nodes.push(BlockNode::new(0, 0, false));
        }

        // Initialize free node stack in reverse order so index 0 is used first.
        for i in 0..MAX_ALLOCATIONS {
            self.free_node_indices.push(MAX_ALLOCATIONS - i - 1);
        }

        // Create the initial free block covering the entire pool.
        if self.size > 0 {
            let node_index = self.create_node();
            self.nodes[node_index as usize] = BlockNode::new(0, self.size, true);
            self.insert_free_block(node_index);
        }
    }

    /// Check if the allocator is empty (has no active allocations).
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.alloc_count == 0
    }

    /// Get the number of active allocations.
    #[cfg(test)]
    pub(crate) fn allocation_count(&self) -> u32 {
        self.alloc_count
    }

    /// Allocate a block of memory of the given size.
    ///
    /// Returns `Some(Allocation)` if successful, `None` if OOM.
    pub(crate) fn allocate(&mut self, size: u32) -> Option<Allocation> {
        // Adjust request size: align to 4 elements and enforce minimum.
        let adjust = cmp::max((size + ALIGN_SIZE - 1) & !(ALIGN_SIZE - 1), BLOCK_SIZE_MIN);

        // Find suitable free block.
        let node_index = self.block_locate_free(adjust)?;

        // Prepare block (mark used, split if necessary).
        let prepared_index = self.block_prepare_used(node_index, adjust);

        self.alloc_count += 1;

        Some(Allocation {
            offset: self.nodes[prepared_index as usize].offset,
            node_index: prepared_index,
        })
    }

    /// Resize an allocation without moving it.
    ///
    /// Returns `Some(updated_allocation)` if the allocation could be resized
    /// in place. Returns `None` if growing in place is not possible.
    pub(crate) fn reallocate_in_place(
        &mut self,
        allocation: Allocation,
        size: u32,
    ) -> Option<Allocation> {
        let adjust = cmp::max((size + ALIGN_SIZE - 1) & !(ALIGN_SIZE - 1), BLOCK_SIZE_MIN);
        let node_index = allocation.node_index;
        let current_size = self.nodes[node_index as usize].size();

        if adjust == current_size {
            return Some(allocation);
        }

        if adjust < current_size {
            let remainder_size = current_size - adjust;
            if remainder_size >= BLOCK_SIZE_MIN {
                let original_phys_next = self.nodes[node_index as usize].phys_next;
                let remainder_index = self.create_node();
                self.nodes[node_index as usize].set_size(adjust);
                self.nodes[node_index as usize].phys_next = remainder_index;

                let mut remainder = BlockNode::new(
                    self.nodes[node_index as usize].offset + adjust,
                    remainder_size,
                    true,
                );
                remainder.phys_prev = node_index;
                remainder.phys_next = original_phys_next;
                self.nodes[remainder_index as usize] = remainder;

                if original_phys_next != NODE_UNUSED {
                    self.nodes[original_phys_next as usize].phys_prev = remainder_index;
                    if self.nodes[original_phys_next as usize].is_free() {
                        self.remove_free_block(original_phys_next);
                        let merged_size = self.nodes[remainder_index as usize].size()
                            + self.nodes[original_phys_next as usize].size();
                        let merged_next = self.nodes[original_phys_next as usize].phys_next;
                        self.nodes[remainder_index as usize].set_size(merged_size);
                        self.nodes[remainder_index as usize].phys_next = merged_next;
                        if merged_next != NODE_UNUSED {
                            self.nodes[merged_next as usize].phys_prev = remainder_index;
                        }
                        self.delete_node(original_phys_next);
                    }
                }

                self.insert_free_block(remainder_index);
            }

            return Some(allocation);
        }

        let next_index = self.nodes[node_index as usize].phys_next;
        if next_index == NODE_UNUSED || !self.nodes[next_index as usize].is_free() {
            return None;
        }

        let current_size = self.nodes[node_index as usize].size();
        let next_size = self.nodes[next_index as usize].size();
        let combined_size = current_size + next_size;
        if combined_size < adjust {
            return None;
        }

        self.remove_free_block(next_index);

        let remainder_size = combined_size - adjust;
        if remainder_size >= BLOCK_SIZE_MIN {
            let next_phys_next = self.nodes[next_index as usize].phys_next;
            self.nodes[node_index as usize].set_size(adjust);
            self.nodes[node_index as usize].phys_next = next_index;
            self.nodes[next_index as usize].offset =
                self.nodes[node_index as usize].offset + adjust;
            self.nodes[next_index as usize].set_size(remainder_size);
            self.nodes[next_index as usize].phys_prev = node_index;
            self.nodes[next_index as usize].phys_next = next_phys_next;
            self.nodes[next_index as usize].set_free(true);
            self.nodes[next_index as usize].list_prev = NODE_UNUSED;
            self.nodes[next_index as usize].list_next = NODE_UNUSED;
            if next_phys_next != NODE_UNUSED {
                self.nodes[next_phys_next as usize].phys_prev = next_index;
            }
            self.insert_free_block(next_index);
        } else {
            let next_phys_next = self.nodes[next_index as usize].phys_next;
            self.nodes[node_index as usize].set_size(combined_size);
            self.nodes[node_index as usize].phys_next = next_phys_next;
            if next_phys_next != NODE_UNUSED {
                self.nodes[next_phys_next as usize].phys_prev = node_index;
            }
            self.delete_node(next_index);
        }

        Some(allocation)
    }

    pub(crate) fn total_free_size(&self) -> u32 {
        self.nodes
            .iter()
            .filter(|node| node.size() != 0 && node.is_free())
            .map(BlockNode::size)
            .sum()
    }

    pub(crate) fn largest_free_block_size(&self) -> u32 {
        self.nodes
            .iter()
            .filter(|node| node.size() != 0 && node.is_free())
            .map(BlockNode::size)
            .max()
            .unwrap_or(0)
    }

    /// Deallocate a previously allocated block.
    ///
    /// This will automatically coalesce with adjacent free regions to reduce
    /// fragmentation.
    ///
    /// # Arguments
    ///
    /// * `allocation` - The allocation to free (returned from `allocate()`).
    ///
    /// # Panics
    ///
    /// In debug builds, panics if:
    /// - The allocation metadata is invalid.
    /// - The allocation has already been freed (double-free).
    pub(crate) fn deallocate(&mut self, allocation: Allocation) {
        let node_index = allocation.node_index;

        debug_assert!(
            (node_index as usize) < self.nodes.len(),
            "invalid allocation metadata"
        );
        debug_assert!(
            !self.nodes[node_index as usize].is_free(),
            "double free detected"
        );

        // Mark current as free.
        self.nodes[node_index as usize].set_free(true);

        // Coalesce with neighbors.
        let mut final_index = self.block_merge_prev(node_index);
        final_index = self.block_merge_next(final_index);

        // Insert back into free list.
        self.insert_free_block(final_index);

        if self.alloc_count > 0 {
            self.alloc_count -= 1;
        }
    }

    /// Get the size of an allocation.
    ///
    /// Returns `None` if the allocation metadata is invalid.
    #[cfg(test)]
    pub(crate) fn allocation_size(&self, allocation: Allocation) -> Option<u32> {
        self.nodes
            .get(allocation.node_index as usize)
            .map(|node| node.size())
    }

    /// Mapping function: Converts a size to First Level (fl) and Second Level (sl) indexes.
    fn mapping(&self, size: u32) -> (usize, usize) {
        if size < SMALL_BLOCK_SIZE {
            // Store small blocks in the first list.
            (
                0,
                (size / (SMALL_BLOCK_SIZE / SL_INDEX_COUNT as u32)) as usize,
            )
        } else {
            // fl = log2(size)
            let fl = (31 - size.leading_zeros()) as usize;
            let sl = ((size >> (fl as u32 - SL_INDEX_COUNT_LOG2)) as usize) ^ SL_INDEX_COUNT;
            (fl - (FL_INDEX_SHIFT - 1), sl)
        }
    }

    /// Locate a free block suitable for the request size.
    /// Searches the bitmaps for the "Good Fit".
    fn block_locate_free(&mut self, size: u32) -> Option<u16> {
        let search_size = if size >= SMALL_BLOCK_SIZE {
            let round = (1 << ((31 - size.leading_zeros()) - SL_INDEX_COUNT_LOG2)) - 1;
            size + round
        } else {
            size
        };

        let (mut fl, mut sl) = self.mapping(search_size);

        let sl_map = self.sl_bitmap[fl] & (!0u32 << sl);

        if sl_map != 0 {
            sl = sl_map.trailing_zeros() as usize;
        } else {
            let fl_map = self.fl_bitmap & (!0u32 << (fl + 1));
            if fl_map == 0 {
                return None;
            }

            fl = fl_map.trailing_zeros() as usize;
            sl = self.sl_bitmap[fl].trailing_zeros() as usize;
        }

        let node_index = self.blocks[fl * SL_INDEX_COUNT + sl];
        if node_index == NODE_UNUSED {
            return None;
        }

        self.remove_free_block(node_index);
        Some(node_index)
    }

    /// Prepare a block for use, potentially splitting it.
    fn block_prepare_used(&mut self, node_index: u16, size: u32) -> u16 {
        let block_size = self.nodes[node_index as usize].size();

        // Check if we can split (needs enough space for minimum block).
        if block_size >= size + BLOCK_SIZE_MIN {
            self.block_split(node_index, size);
        }

        self.nodes[node_index as usize].set_free(false);
        node_index
    }

    /// Split a block into two: the requested part (used) and the remainder (free).
    fn block_split(&mut self, original_index: u16, size: u32) {
        let original_node = &self.nodes[original_index as usize];
        let remainder_size = original_node.size() - size;
        let original_offset = original_node.offset;
        let original_phys_next = original_node.phys_next;

        // 1. Shrink the original node.
        self.nodes[original_index as usize].set_size(size);

        // 2. Create the new remainder node.
        let new_node_index = self.create_node();

        // 3. Setup the remainder node.
        let mut new_node = BlockNode::new(original_offset + size, remainder_size, true);
        new_node.phys_prev = original_index;
        new_node.phys_next = original_phys_next;
        self.nodes[new_node_index as usize] = new_node;

        // 4. Update the original node to point to the remainder.
        self.nodes[original_index as usize].phys_next = new_node_index;

        // 5. Update the next physical neighbor to point back to the new remainder.
        if original_phys_next != NODE_UNUSED {
            self.nodes[original_phys_next as usize].phys_prev = new_node_index;
        }

        // 6. Insert remainder into free list.
        self.insert_free_block(new_node_index);
    }

    /// Insert a block into the appropriate free list.
    fn insert_free_block(&mut self, node_index: u16) {
        let size = self.nodes[node_index as usize].size();
        let (fl, sl) = self.mapping(size);

        let current_head = self.blocks[fl * SL_INDEX_COUNT + sl];

        // Set links.
        self.nodes[node_index as usize].list_next = current_head;
        self.nodes[node_index as usize].list_prev = NODE_UNUSED;

        if current_head != NODE_UNUSED {
            self.nodes[current_head as usize].list_prev = node_index;
        }

        // Update head.
        self.blocks[fl * SL_INDEX_COUNT + sl] = node_index;

        // Update bitmaps.
        self.fl_bitmap |= 1 << fl;
        self.sl_bitmap[fl] |= 1 << sl;
    }

    /// Remove a block from the free list.
    fn remove_free_block(&mut self, node_index: u16) {
        let node = &self.nodes[node_index as usize];
        let size = node.size();
        let next = node.list_next;
        let prev = node.list_prev;

        let (fl, sl) = self.mapping(size);

        if prev != NODE_UNUSED {
            self.nodes[prev as usize].list_next = next;
        }

        if next != NODE_UNUSED {
            self.nodes[next as usize].list_prev = prev;
        }

        // If this was the head, update head.
        if self.blocks[fl * SL_INDEX_COUNT + sl] == node_index {
            self.blocks[fl * SL_INDEX_COUNT + sl] = next;

            // If list became empty, update bitmaps.
            if next == NODE_UNUSED {
                self.sl_bitmap[fl] &= !(1 << sl);
                if self.sl_bitmap[fl] == 0 {
                    self.fl_bitmap &= !(1 << fl);
                }
            }
        }
    }

    /// Merge with previous physical block.
    fn block_merge_prev(&mut self, current_index: u16) -> u16 {
        let prev_index = self.nodes[current_index as usize].phys_prev;

        if prev_index == NODE_UNUSED {
            return current_index;
        }

        if !self.nodes[prev_index as usize].is_free() {
            return current_index;
        }

        // Remove previous block from its free list.
        self.remove_free_block(prev_index);

        // Absorb current into previous.
        let current_size = self.nodes[current_index as usize].size();
        let current_phys_next = self.nodes[current_index as usize].phys_next;

        let new_size = self.nodes[prev_index as usize].size() + current_size;
        self.nodes[prev_index as usize].set_size(new_size);
        self.nodes[prev_index as usize].phys_next = current_phys_next;

        // Update the next physical neighbor to point back to prev.
        if current_phys_next != NODE_UNUSED {
            self.nodes[current_phys_next as usize].phys_prev = prev_index;
        }

        // Delete the current node.
        self.delete_node(current_index);

        prev_index
    }

    /// Merge with next physical block.
    fn block_merge_next(&mut self, current_index: u16) -> u16 {
        let next_index = self.nodes[current_index as usize].phys_next;

        if next_index == NODE_UNUSED {
            return current_index;
        }

        if !self.nodes[next_index as usize].is_free() {
            return current_index;
        }

        // Remove next block from its free list.
        self.remove_free_block(next_index);

        // Absorb next into current.
        let next_size = self.nodes[next_index as usize].size();
        let next_phys_next = self.nodes[next_index as usize].phys_next;

        let new_size = self.nodes[current_index as usize].size() + next_size;
        self.nodes[current_index as usize].set_size(new_size);
        self.nodes[current_index as usize].phys_next = next_phys_next;

        // Update the next-next physical neighbor to point back to current.
        if next_phys_next != NODE_UNUSED {
            self.nodes[next_phys_next as usize].phys_prev = current_index;
        }

        // Delete the next node.
        self.delete_node(next_index);

        current_index
    }

    /// Create a new node by popping from the free stack.
    fn create_node(&mut self) -> u16 {
        debug_assert!(self.free_offset != u16::MAX, "out of nodes");

        let node_index = self.free_node_indices[self.free_offset as usize];
        self.free_offset = self.free_offset.wrapping_sub(1);
        node_index
    }

    /// Delete a node by returning its index to the free stack.
    fn delete_node(&mut self, node_index: u16) {
        self.free_offset = self.free_offset.wrapping_add(1);
        self.free_node_indices[self.free_offset as usize] = node_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POOL_SIZE: u32 = 1 << 20;

    #[test]
    fn test_initialization() {
        let alloc = TlsfAllocator::new(POOL_SIZE);
        assert!(alloc.is_empty());
        assert_eq!(alloc.size, POOL_SIZE);
    }

    #[test]
    fn test_basic_allocation() {
        let mut alloc = TlsfAllocator::new(1024);

        let size = 100;
        let allocation = alloc.allocate(size).expect("Allocation failed");

        assert_eq!(allocation.offset % ALIGN_SIZE, 0);

        let stored_size = alloc.allocation_size(allocation).unwrap();
        assert!(stored_size >= size);
        assert_eq!(stored_size % ALIGN_SIZE, 0);

        assert!(!alloc.is_empty());
    }

    #[test]
    fn test_full_block_allocation() {
        const BLOCK_SIZE: u32 = 1 << 20;
        let mut alloc = TlsfAllocator::new(BLOCK_SIZE);

        let allocation = alloc.allocate(BLOCK_SIZE).expect("Allocation failed");

        assert_eq!(allocation.offset % ALIGN_SIZE, 0);

        let stored_size = alloc.allocation_size(allocation).unwrap();
        assert_eq!(stored_size, BLOCK_SIZE);
        assert_eq!(stored_size % ALIGN_SIZE, 0);

        assert!(!alloc.is_empty());
    }

    #[test]
    fn test_allocation_and_deallocation() {
        let mut alloc = TlsfAllocator::new(1024);
        let ptr1 = alloc.allocate(100).unwrap();
        let ptr2 = alloc.allocate(200).unwrap();

        assert_eq!(alloc.allocation_count(), 2);

        alloc.deallocate(ptr1);
        assert_eq!(alloc.allocation_count(), 1);

        alloc.deallocate(ptr2);
        assert!(alloc.is_empty());
    }

    #[test]
    fn test_alignment_and_min_size() {
        let mut alloc = TlsfAllocator::new(1024);

        let alloc_res = alloc.allocate(1).unwrap();
        let size = alloc.allocation_size(alloc_res).unwrap();

        assert!(size >= BLOCK_SIZE_MIN);
        assert_eq!(alloc_res.offset % ALIGN_SIZE, 0);
    }

    #[test]
    fn test_oom() {
        let total_size = 64;
        let mut alloc = TlsfAllocator::new(total_size);

        let _ptr = alloc.allocate(64).expect("Should succeed");

        let ptr_fail = alloc.allocate(16);
        assert!(ptr_fail.is_none());
    }

    #[test]
    fn test_reuse_freed_memory() {
        let mut alloc = TlsfAllocator::new(1024);

        let ptr1 = alloc.allocate(200).unwrap();
        let offset1 = ptr1.offset;

        let _ptr2 = alloc.allocate(100).unwrap();

        alloc.deallocate(ptr1);

        let ptr3 = alloc.allocate(200).unwrap();

        assert_eq!(ptr3.offset, offset1);
    }

    #[test]
    fn test_split_logic() {
        let mut alloc = TlsfAllocator::new(1024);

        let ptr1 = alloc.allocate(100).unwrap();
        let size1 = alloc.allocation_size(ptr1).unwrap();

        assert_eq!(size1, 100);

        let ptr2 = alloc.allocate(100).unwrap();

        let expected_offset = ptr1.offset + 100;
        assert_eq!(ptr2.offset, expected_offset);
    }

    #[test]
    fn test_coalescing_prev_and_next() {
        let mut alloc = TlsfAllocator::new(512);

        let a = alloc.allocate(100).unwrap();
        let b = alloc.allocate(100).unwrap();
        let c = alloc.allocate(100).unwrap();

        assert_eq!(a.offset, 0);
        assert_eq!(b.offset, 100);
        assert_eq!(c.offset, 200);

        alloc.deallocate(a);
        alloc.deallocate(c);
        // Free B (Middle) -> Should merge with A (prev) and C (next)
        alloc.deallocate(b);

        let big_chunk = alloc.allocate(300);
        assert!(big_chunk.is_some(), "Failed to coalesce blocks A, B, and C");

        assert_eq!(big_chunk.unwrap().offset, 0);
    }

    #[test]
    fn test_random_stress() {
        let mut alloc = TlsfAllocator::new(1024 * 1024);
        let mut allocations = Vec::new();

        let mut seed: u32 = 12345;
        let mut rand = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed
        };

        for _ in 0..1000 {
            let r = rand();

            // 60% chance to allocate, 40% to deallocate.
            if r % 10 < 6 {
                // Alloc 16 to 1024 elements.
                let size = (r % 1000) + 16;
                if let Some(ptr) = alloc.allocate(size) {
                    allocations.push(ptr);
                }
            } else if !allocations.is_empty() {
                let index = (r as usize) % allocations.len();
                let ptr = allocations.remove(index);
                alloc.deallocate(ptr);
            }
        }

        for ptr in allocations {
            alloc.deallocate(ptr);
        }

        assert!(alloc.is_empty(), "Allocator not empty after cleanup");

        // Fragmentation / Re-allocation Test
        //
        // TLSF uses a "Good Fit" strategy. It rounds the requested size UP to the next
        // subdivision to ensure O(1) lookup.
        //
        // With virtual addressing, the entire pool is available (no overhead for headers).
        // However, we still cannot allocate 100% of the capacity due to TLSF rounding.
        //
        // The rounding overhead for large allocations is approximately 1/SL_INDEX_COUNT
        // of the allocation size. For a 1MB pool, this is about 32KB.
        let capacity = 1024 * 1024;
        let rounding_overhead = capacity >> SL_INDEX_COUNT_LOG2;
        let safe_alloc_size = capacity - rounding_overhead - 16;

        let allocation = alloc.allocate(safe_alloc_size);
        assert!(allocation.is_some(),);
    }

    #[test]
    fn test_reset() {
        let mut alloc = TlsfAllocator::new(1024);
        let _ = alloc.allocate(100).unwrap();
        alloc.reset();
        assert!(alloc.is_empty());

        let capacity = 1024;
        assert!(alloc.allocate(capacity).is_some());
    }

    #[test]
    fn test_alloc_zero_size() {
        let mut alloc = TlsfAllocator::new(1024);
        let ptr = alloc.allocate(0).unwrap();
        let size = alloc.allocation_size(ptr).unwrap();
        assert_eq!(size, BLOCK_SIZE_MIN);
    }

    #[test]
    fn test_contiguous_allocations() {
        let mut alloc = TlsfAllocator::new(1024);

        let a1 = alloc.allocate(100).unwrap();
        let a2 = alloc.allocate(200).unwrap();
        let a3 = alloc.allocate(50).unwrap();

        assert_eq!(a1.offset, 0);
        assert_eq!(a2.offset, 100);
        assert_eq!(a3.offset, 300);
    }

    #[test]
    fn test_virtual_addressing_no_memory_overhead() {
        let pool_size = 1024;
        let mut alloc = TlsfAllocator::new(pool_size);

        let rounding_overhead = pool_size >> SL_INDEX_COUNT_LOG2;
        let safe_alloc_size = pool_size - rounding_overhead;

        let allocation = alloc.allocate(safe_alloc_size);
        assert!(allocation.is_some());
    }

    #[test]
    fn test_max_allocations_u16_limit() {
        let pool_size = (u16::MAX as u32) * BLOCK_SIZE_MIN;
        let mut alloc = TlsfAllocator::new(pool_size);
        let mut allocations = Vec::new();

        for _ in 0..u16::MAX {
            if let Some(allocation) = alloc.allocate(BLOCK_SIZE_MIN) {
                allocations.push(allocation);
            } else {
                break;
            }
        }

        assert!(
            allocations.len() >= (u16::MAX - 1) as usize,
            "Expected at least {} allocations, got {}",
            u16::MAX - 1,
            allocations.len()
        );

        assert!(
            alloc.allocate(BLOCK_SIZE_MIN).is_none(),
            "Expected allocation to fail after reaching u16::MAX limit"
        );

        for allocation in allocations {
            alloc.deallocate(allocation);
        }

        assert!(alloc.is_empty());
    }

    #[test]
    fn test_bit_packing() {
        let mut node = BlockNode::new(0, 64, true);
        assert_eq!(node.size(), 64);
        assert!(node.is_free());

        node.set_free(false);
        assert_eq!(node.size(), 64);
        assert!(!node.is_free());

        node.set_size(128);
        assert_eq!(node.size(), 128);
        assert!(!node.is_free());

        node.set_free(true);
        assert_eq!(node.size(), 128);
        assert!(node.is_free());
    }

    #[test]
    fn test_reallocate_in_place_grow_uses_adjacent_free_space() {
        let mut alloc = TlsfAllocator::new(1024);
        let first = alloc.allocate(100).unwrap();
        let second = alloc.allocate(100).unwrap();

        alloc.deallocate(second);

        let grown = alloc.reallocate_in_place(first, 180).unwrap();
        assert_eq!(grown.offset, 0);
        assert_eq!(alloc.allocation_size(grown).unwrap(), 180);

        let tail = alloc.allocate(16).unwrap();
        assert_eq!(tail.offset, 180);
    }

    #[test]
    fn test_reallocate_in_place_shrink_creates_reusable_tail() {
        let mut alloc = TlsfAllocator::new(1024);
        let first = alloc.allocate(200).unwrap();
        let shrunk = alloc.reallocate_in_place(first, 100).unwrap();

        assert_eq!(shrunk.offset, 0);
        assert_eq!(alloc.allocation_size(shrunk).unwrap(), 100);

        let tail = alloc.allocate(100).unwrap();
        assert_eq!(tail.offset, 100);
    }
}
