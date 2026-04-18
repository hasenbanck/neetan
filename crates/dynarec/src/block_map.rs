//! Block cache keyed by physical linear address.
//!
//! Tracks a per-page dirty map plus a per-page list of block start
//! addresses so SMC invalidations from the bus can drop only the blocks
//! that overlap a written page, instead of flushing the whole cache.

use std::collections::{HashMap, hash_map::Entry};

#[cfg(all(target_arch = "x86_64", unix))]
use crate::backend_x64::CompiledBlock;
use crate::ir::{Block, IrOp};

const DEFAULT_BLOCK_CAPACITY: usize = 1024;
/// Bytes of dirty-map coverage: one entry per 4 KiB page over a 256 MiB
/// window. 65536 bytes is small (64 KiB), and covers every PC-9821 RAM
/// configuration with headroom. Each byte is 0 (no code on that page)
/// or 1 (at least one translated block lives on that page).
const DIRTY_MAP_PAGES: usize = 64 * 1024;

pub(crate) struct CachedBlock {
    pub block: Block,
    #[cfg(all(target_arch = "x86_64", unix))]
    pub native: Option<CompiledBlock>,
}

impl CachedBlock {
    pub fn new(block: Block) -> Self {
        Self {
            block,
            #[cfg(all(target_arch = "x86_64", unix))]
            native: None,
        }
    }
}

/// A cache of compiled blocks indexed by the physical linear address of
/// their first byte.
///
/// Tracks which pages host translated code in [`BlockMap::dirty_map`] so
/// the SMC invalidator can short-circuit writes that do not land on a
/// translated page. When a write does land on such a page, the per-page
/// block list in [`BlockMap::page_blocks`] gives us the set of block
/// start addresses to drop.
pub struct BlockMap {
    blocks: HashMap<u32, Box<CachedBlock>>,
    dirty_map: Vec<u8>,
    page_blocks: HashMap<u32, Vec<u32>>,
}

impl Default for BlockMap {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockMap {
    /// Creates a new empty block map.
    pub fn new() -> Self {
        Self {
            blocks: HashMap::with_capacity(DEFAULT_BLOCK_CAPACITY),
            dirty_map: vec![0u8; DIRTY_MAP_PAGES],
            page_blocks: HashMap::new(),
        }
    }

    /// Looks up or reserves a slot for the block starting at `phys`.
    pub fn entry(&mut self, phys: u32) -> Entry<'_, u32, Box<CachedBlock>> {
        self.blocks.entry(phys)
    }

    /// Returns a shared reference to the cached block at `phys`, if any.
    pub fn get(&self, phys: u32) -> Option<&CachedBlock> {
        self.blocks.get(&phys).map(|boxed| boxed.as_ref())
    }

    /// Returns a mutable reference to the cached block at `phys`, if any.
    pub fn get_mut(&mut self, phys: u32) -> Option<&mut CachedBlock> {
        self.blocks.get_mut(&phys).map(|boxed| boxed.as_mut())
    }

    /// Records that a block exists at `phys_start` so later SMC writes
    /// in that page can invalidate it. Call after a successful insert.
    pub fn register_block_page(&mut self, phys_start: u32) {
        let page = phys_start >> 12;
        self.mark_page_dirty(page);
        self.page_blocks.entry(page).or_default().push(phys_start);
    }

    #[cfg(all(target_arch = "x86_64", unix))]
    pub fn clear_native(&mut self) {
        for cached in self.blocks.values_mut() {
            cached.native = None;
        }
    }

    /// Drops every block whose start page is in `[phys_start, phys_end)`.
    /// The recycled IR-op vectors are pushed into `ops_pool` so the
    /// decoder can reuse them. Returns `true` if at least one block was
    /// invalidated.
    pub fn invalidate_range(
        &mut self,
        phys_start: u32,
        phys_end: u32,
        ops_pool: &mut Vec<Vec<IrOp>>,
    ) -> bool {
        if phys_end == phys_start {
            return false;
        }
        let first_page = phys_start >> 12;
        let last_page = phys_end.wrapping_sub(1) >> 12;
        let mut invalidated = false;
        for page in first_page..=last_page {
            if !self.is_page_dirty(page) {
                continue;
            }
            let Some(entries) = self.page_blocks.remove(&page) else {
                self.clear_page_dirty(page);
                continue;
            };
            for block_start in entries {
                if let Some(cached) = self.blocks.remove(&block_start) {
                    let mut ops = cached.block.ops;
                    ops.clear();
                    ops_pool.push(ops);
                    invalidated = true;
                }
            }
            self.clear_page_dirty(page);
        }
        invalidated
    }

    /// Flushes all translated blocks and resets the dirty map.
    pub fn flush_into(&mut self, ops_pool: &mut Vec<Vec<IrOp>>) {
        for (_, cached) in self.blocks.drain() {
            let mut ops = cached.block.ops;
            ops.clear();
            ops_pool.push(ops);
        }
        self.page_blocks.clear();
        for byte in &mut self.dirty_map {
            *byte = 0;
        }
    }

    #[inline(always)]
    fn mark_page_dirty(&mut self, page: u32) {
        let idx = (page as usize) % self.dirty_map.len();
        self.dirty_map[idx] = 1;
    }

    #[inline(always)]
    fn clear_page_dirty(&mut self, page: u32) {
        let idx = (page as usize) % self.dirty_map.len();
        self.dirty_map[idx] = 0;
    }

    #[inline(always)]
    fn is_page_dirty(&self, page: u32) -> bool {
        let idx = (page as usize) % self.dirty_map.len();
        self.dirty_map[idx] != 0
    }
}
