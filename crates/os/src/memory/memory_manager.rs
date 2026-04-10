//! Unified EMS/XMS/UMB memory manager.
//!
//! All EMS/XMS data lives in the machine's extended RAM (accessed via
//! `MemoryAccess` at addresses 0x100000 + offset). The TLSF allocator
//! tracks which byte ranges within extended RAM are allocated.

use super::tlsf::{ALIGN_MASK, Allocation, TlsfAllocator};
use crate::{MemoryAccess, memory, tables::*};

pub(crate) const EXTENDED_RAM_BASE: u32 = 0x100000;

pub(crate) struct EmsMoveParams {
    pub region_length: u32,
    pub src_type: u8,
    pub src_handle: u16,
    pub src_offset: u16,
    pub src_seg_page: u16,
    pub dst_type: u8,
    pub dst_handle: u16,
    pub dst_offset: u16,
    pub dst_seg_page: u16,
}
const EMS_PAGE_FRAME_BASE: u32 = 0xC0000;
const EMS_PAGE_SIZE: u32 = 0x4000;
const MAX_EMS_HANDLES: usize = 255;
const MAX_XMS_HANDLES: usize = 128;
const PHYSICAL_PAGES: usize = 4;

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmsMapping {
    pub(crate) handle: u16,
    pub(crate) logical_page: u16,
    pub(crate) allocation_offset: u32,
}

struct EmsHandle {
    active: bool,
    pages: Vec<Allocation>,
    name: [u8; 8],
    save_context: Option<[Option<EmsMapping>; PHYSICAL_PAGES]>,
}

impl EmsHandle {
    fn new_inactive() -> Self {
        Self {
            active: false,
            pages: Vec::new(),
            name: [0u8; 8],
            save_context: None,
        }
    }
}

struct XmsHandle {
    active: bool,
    allocation: Option<Allocation>,
    size_kb: u32,
    lock_count: u8,
}

impl XmsHandle {
    fn new_inactive() -> Self {
        Self {
            active: false,
            allocation: None,
            size_kb: 0,
            lock_count: 0,
        }
    }
}

pub(crate) struct MemoryManager {
    allocator: TlsfAllocator,
    extended_memory_size: u32,

    ems_enabled: bool,
    ems_handles: Vec<EmsHandle>,
    ems_page_mapping: [Option<EmsMapping>; PHYSICAL_PAGES],

    xms_enabled: bool,
    xms_32_enabled: bool,
    xms_handles: Vec<XmsHandle>,
    hma_allocated: bool,

    umb_enabled: bool,

    ems_os_access_key: Option<u32>,
    ems_os_functions_enabled: bool,
}

impl MemoryManager {
    pub(crate) fn new(
        extended_memory_size: u32,
        ems_enabled: bool,
        xms_enabled: bool,
        xms_32_enabled: bool,
        mem: &mut dyn MemoryAccess,
    ) -> Self {
        let allocator = TlsfAllocator::new(extended_memory_size);

        if ems_enabled {
            mem.enable_ems_page_frame();
        }

        let umb_enabled = ems_enabled || xms_enabled;
        if umb_enabled {
            mem.enable_umb_region();
            memory::write_mcb(
                mem,
                UMB_FIRST_MCB_SEGMENT,
                0x5A,
                MCB_OWNER_FREE,
                UMB_TOTAL_PARAGRAPHS,
                b"\0\0\0\0\0\0\0\0",
            );
        }

        let mut ems_handles = Vec::with_capacity(MAX_EMS_HANDLES);
        for _ in 0..MAX_EMS_HANDLES {
            ems_handles.push(EmsHandle::new_inactive());
        }

        let mut xms_handles = Vec::with_capacity(MAX_XMS_HANDLES);
        for _ in 0..MAX_XMS_HANDLES {
            xms_handles.push(XmsHandle::new_inactive());
        }

        Self {
            allocator,
            extended_memory_size,
            ems_enabled,
            ems_handles,
            ems_page_mapping: [None; PHYSICAL_PAGES],
            xms_enabled,
            xms_32_enabled,
            xms_handles,
            hma_allocated: false,
            umb_enabled,
            ems_os_access_key: None,
            ems_os_functions_enabled: false,
        }
    }

    fn total_ems_pages(&self) -> u16 {
        (self.extended_memory_size / EMS_PAGE_SIZE) as u16
    }

    fn allocated_ems_pages(&self) -> u16 {
        let mut count: u16 = 0;
        for handle in &self.ems_handles {
            if handle.active {
                count += handle.pages.len() as u16;
            }
        }
        count
    }

    fn allocated_xms_kb(&self) -> u32 {
        let mut total: u32 = 0;
        for handle in &self.xms_handles {
            if handle.active {
                total += handle.size_kb;
            }
        }
        total
    }

    fn find_free_ems_handle(&self) -> Option<u16> {
        for (i, handle) in self.ems_handles.iter().enumerate() {
            if !handle.active {
                return Some(i as u16);
            }
        }
        None
    }

    fn find_free_xms_handle(&self) -> Option<u16> {
        for (i, handle) in self.xms_handles.iter().enumerate() {
            if !handle.active {
                return Some(i as u16);
            }
        }
        None
    }

    fn free_xms_handles_count(&self) -> u16 {
        self.xms_handles.iter().filter(|h| !h.active).count() as u16
    }

    fn save_page_frame_slot(&self, physical: usize, mem: &mut dyn MemoryAccess) {
        if let Some(mapping) = self.ems_page_mapping[physical] {
            let frame_addr = EMS_PAGE_FRAME_BASE + physical as u32 * EMS_PAGE_SIZE;
            let pool_addr = EXTENDED_RAM_BASE + mapping.allocation_offset;
            let mut buf = [0u8; EMS_PAGE_SIZE as usize];
            mem.read_block(frame_addr, &mut buf);
            mem.write_block(pool_addr, &buf);
        }
    }

    fn load_page_frame_slot(&self, physical: usize, offset: u32, mem: &mut dyn MemoryAccess) {
        let frame_addr = EMS_PAGE_FRAME_BASE + physical as u32 * EMS_PAGE_SIZE;
        let pool_addr = EXTENDED_RAM_BASE + offset;
        let mut buf = [0u8; EMS_PAGE_SIZE as usize];
        mem.read_block(pool_addr, &mut buf);
        mem.write_block(frame_addr, &buf);
    }

    pub(crate) fn ems_status(&self) -> u8 {
        if self.ems_enabled { 0x00 } else { 0x84 }
    }

    pub(crate) fn ems_page_frame_segment(&self) -> u16 {
        EMS_PAGE_FRAME_SEGMENT
    }

    pub(crate) fn ems_unallocated_pages(&self) -> (u16, u16) {
        let total = self.total_ems_pages();
        let allocated = self.allocated_ems_pages();
        (total.saturating_sub(allocated), total)
    }

    pub(crate) fn ems_allocate_pages(&mut self, count: u16) -> Result<u16, u8> {
        if count == 0 {
            return Err(0x89);
        }
        let handle_index = self.find_free_ems_handle().ok_or(0x85u8)?;
        let (free, _total) = self.ems_unallocated_pages();
        if count > free {
            return Err(0x88);
        }

        let mut pages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let size = (EMS_PAGE_SIZE as u64 + ALIGN_MASK) & !ALIGN_MASK;
            match self.allocator.allocate(size as u32) {
                Some(alloc) => pages.push(alloc),
                None => {
                    for alloc in pages {
                        self.allocator.deallocate(alloc);
                    }
                    return Err(0x88);
                }
            }
        }

        let handle = &mut self.ems_handles[handle_index as usize];
        handle.active = true;
        handle.pages = pages;
        handle.name = [0u8; 8];
        handle.save_context = None;
        Ok(handle_index)
    }

    pub(crate) fn ems_allocate_pages_zero_allowed(&mut self, count: u16) -> Result<u16, u8> {
        let handle_index = self.find_free_ems_handle().ok_or(0x85u8)?;
        if count > 0 {
            let (free, _total) = self.ems_unallocated_pages();
            if count > free {
                return Err(0x88);
            }
        }

        let mut pages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let size = (EMS_PAGE_SIZE as u64 + ALIGN_MASK) & !ALIGN_MASK;
            match self.allocator.allocate(size as u32) {
                Some(alloc) => pages.push(alloc),
                None => {
                    for alloc in pages {
                        self.allocator.deallocate(alloc);
                    }
                    return Err(0x88);
                }
            }
        }

        let handle = &mut self.ems_handles[handle_index as usize];
        handle.active = true;
        handle.pages = pages;
        handle.name = [0u8; 8];
        handle.save_context = None;
        Ok(handle_index)
    }

    pub(crate) fn ems_map_page(
        &mut self,
        handle_index: u16,
        logical_page: u16,
        physical_page: u8,
        mem: &mut dyn MemoryAccess,
    ) -> u8 {
        if physical_page as usize >= PHYSICAL_PAGES {
            return 0x8B;
        }
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return 0x83;
        }
        let handle = &self.ems_handles[handle_index as usize];
        if logical_page as usize >= handle.pages.len() {
            return 0x8A;
        }
        let allocation_offset = handle.pages[logical_page as usize].offset();

        self.save_page_frame_slot(physical_page as usize, mem);
        self.load_page_frame_slot(physical_page as usize, allocation_offset, mem);

        self.ems_page_mapping[physical_page as usize] = Some(EmsMapping {
            handle: handle_index,
            logical_page,
            allocation_offset,
        });
        0x00
    }

    pub(crate) fn ems_unmap_page(&mut self, physical_page: u8, mem: &mut dyn MemoryAccess) -> u8 {
        if physical_page as usize >= PHYSICAL_PAGES {
            return 0x8B;
        }
        self.save_page_frame_slot(physical_page as usize, mem);
        self.ems_page_mapping[physical_page as usize] = None;
        0x00
    }

    pub(crate) fn ems_deallocate(&mut self, handle_index: u16, mem: &mut dyn MemoryAccess) -> u8 {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return 0x83;
        }

        for slot in 0..PHYSICAL_PAGES {
            if let Some(mapping) = self.ems_page_mapping[slot]
                && mapping.handle == handle_index
            {
                self.save_page_frame_slot(slot, mem);
                self.ems_page_mapping[slot] = None;
            }
        }

        let handle = &mut self.ems_handles[handle_index as usize];
        let pages: Vec<Allocation> = handle.pages.drain(..).collect();
        for alloc in pages {
            self.allocator.deallocate(alloc);
        }
        handle.active = false;
        handle.name = [0u8; 8];
        handle.save_context = None;
        0x00
    }

    pub(crate) fn ems_save_page_map(&mut self, handle_index: u16) -> u8 {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return 0x83;
        }
        let handle = &mut self.ems_handles[handle_index as usize];
        if handle.save_context.is_some() {
            return 0x8D;
        }
        handle.save_context = Some(self.ems_page_mapping);
        0x00
    }

    pub(crate) fn ems_restore_page_map(
        &mut self,
        handle_index: u16,
        mem: &mut dyn MemoryAccess,
    ) -> u8 {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return 0x83;
        }
        let saved = match self.ems_handles[handle_index as usize].save_context {
            Some(ctx) => ctx,
            None => return 0x8E,
        };

        for slot in 0..PHYSICAL_PAGES {
            self.save_page_frame_slot(slot, mem);
        }
        for (slot, entry) in saved.iter().enumerate() {
            if let Some(mapping) = entry {
                self.load_page_frame_slot(slot, mapping.allocation_offset, mem);
            }
        }
        self.ems_page_mapping = saved;
        self.ems_handles[handle_index as usize].save_context = None;
        0x00
    }

    pub(crate) fn ems_get_page_map(&self) -> [Option<EmsMapping>; PHYSICAL_PAGES] {
        self.ems_page_mapping
    }

    pub(crate) fn ems_set_page_map(
        &mut self,
        saved: [Option<EmsMapping>; PHYSICAL_PAGES],
        mem: &mut dyn MemoryAccess,
    ) {
        for slot in 0..PHYSICAL_PAGES {
            self.save_page_frame_slot(slot, mem);
        }
        for (slot, entry) in saved.iter().enumerate() {
            if let Some(mapping) = entry {
                self.load_page_frame_slot(slot, mapping.allocation_offset, mem);
            }
        }
        self.ems_page_mapping = saved;
    }

    pub(crate) fn ems_page_map_size(&self) -> u16 {
        (PHYSICAL_PAGES * size_of::<u32>() * 2) as u16
    }

    pub(crate) fn ems_reallocate(&mut self, handle_index: u16, new_count: u16) -> Result<u16, u8> {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return Err(0x83);
        }
        let current_count = self.ems_handles[handle_index as usize].pages.len() as u16;
        if new_count == current_count {
            return Ok(new_count);
        }
        if new_count > current_count {
            let extra = new_count - current_count;
            let (free, _) = self.ems_unallocated_pages();
            if extra > free {
                return Err(0x88);
            }
            for _ in 0..extra {
                let size = (EMS_PAGE_SIZE as u64 + ALIGN_MASK) & !ALIGN_MASK;
                match self.allocator.allocate(size as u32) {
                    Some(alloc) => {
                        self.ems_handles[handle_index as usize].pages.push(alloc);
                    }
                    None => return Err(0x88),
                }
            }
        } else {
            let handle = &mut self.ems_handles[handle_index as usize];
            while handle.pages.len() > new_count as usize {
                if let Some(alloc) = handle.pages.pop() {
                    self.allocator.deallocate(alloc);
                }
            }
        }
        Ok(new_count)
    }

    pub(crate) fn ems_handle_count(&self) -> u16 {
        self.ems_handles.iter().filter(|h| h.active).count() as u16
    }

    pub(crate) fn ems_handle_pages(&self, handle_index: u16) -> Result<u16, u8> {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return Err(0x83);
        }
        Ok(self.ems_handles[handle_index as usize].pages.len() as u16)
    }

    pub(crate) fn ems_all_handle_pages(&self) -> Vec<(u16, u16)> {
        let mut result = Vec::new();
        for (i, handle) in self.ems_handles.iter().enumerate() {
            if handle.active {
                result.push((i as u16, handle.pages.len() as u16));
            }
        }
        result
    }

    pub(crate) fn ems_handle_name(&self, handle_index: u16) -> Result<[u8; 8], u8> {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return Err(0x83);
        }
        Ok(self.ems_handles[handle_index as usize].name)
    }

    pub(crate) fn ems_set_handle_name(&mut self, handle_index: u16, name: [u8; 8]) -> u8 {
        if handle_index as usize >= self.ems_handles.len()
            || !self.ems_handles[handle_index as usize].active
        {
            return 0x83;
        }
        if name != [0u8; 8] {
            for (i, h) in self.ems_handles.iter().enumerate() {
                if h.active && i as u16 != handle_index && h.name == name {
                    return 0x92;
                }
            }
        }
        self.ems_handles[handle_index as usize].name = name;
        0x00
    }

    pub(crate) fn ems_handle_directory(&self) -> Vec<(u16, [u8; 8])> {
        let mut result = Vec::new();
        for (i, handle) in self.ems_handles.iter().enumerate() {
            if handle.active {
                result.push((i as u16, handle.name));
            }
        }
        result
    }

    pub(crate) fn ems_search_handle_name(&self, name: &[u8; 8]) -> Result<u16, u8> {
        for (i, handle) in self.ems_handles.iter().enumerate() {
            if handle.active && handle.name == *name {
                return Ok(i as u16);
            }
        }
        Err(0xA0)
    }

    pub(crate) fn ems_is_valid_handle(&self, handle_index: u16) -> bool {
        (handle_index as usize) < self.ems_handles.len()
            && self.ems_handles[handle_index as usize].active
    }

    pub(crate) fn ems_page_allocation_offset(
        &self,
        handle_index: u16,
        logical_page: u16,
    ) -> Option<u32> {
        let handle = self.ems_handles.get(handle_index as usize)?;
        if !handle.active {
            return None;
        }
        let alloc = handle.pages.get(logical_page as usize)?;
        Some(alloc.offset())
    }

    pub(crate) fn ems_move_memory(&self, params: &EmsMoveParams, mem: &mut dyn MemoryAccess) -> u8 {
        let region_length = params.region_length;
        let src_type = params.src_type;
        let src_handle = params.src_handle;
        let src_offset = params.src_offset;
        let src_seg_page = params.src_seg_page;
        let dst_type = params.dst_type;
        let dst_handle = params.dst_handle;
        let dst_offset = params.dst_offset;
        let dst_seg_page = params.dst_seg_page;
        if region_length == 0 {
            return 0x00;
        }

        let src_linear = match src_type {
            0 => {
                let addr = (src_seg_page as u32) * 16 + src_offset as u32;
                if addr + region_length > 0x100000 {
                    return 0x96;
                }
                addr
            }
            1 => {
                if !self.ems_is_valid_handle(src_handle) {
                    return 0x83;
                }
                let offset = match self.ems_page_allocation_offset(src_handle, src_seg_page) {
                    Some(o) => o,
                    None => return 0x8A,
                };
                EXTENDED_RAM_BASE + offset + src_offset as u32
            }
            _ => return 0x98,
        };

        let dst_linear = match dst_type {
            0 => {
                let addr = (dst_seg_page as u32) * 16 + dst_offset as u32;
                if addr + region_length > 0x100000 {
                    return 0x96;
                }
                addr
            }
            1 => {
                if !self.ems_is_valid_handle(dst_handle) {
                    return 0x83;
                }
                let offset = match self.ems_page_allocation_offset(dst_handle, dst_seg_page) {
                    Some(o) => o,
                    None => return 0x8A,
                };
                EXTENDED_RAM_BASE + offset + dst_offset as u32
            }
            _ => return 0x98,
        };

        let mut buf = vec![0u8; region_length as usize];
        mem.read_block(src_linear, &mut buf);
        mem.write_block(dst_linear, &buf);
        0x00
    }

    pub(crate) fn xms_version(&self) -> (u16, u16, u16) {
        (
            0x0300,
            0x0001,
            if self.extended_memory_size > 0 { 1 } else { 0 },
        )
    }

    pub(crate) fn xms_request_hma(&mut self, size: u16) -> Result<(), u8> {
        if self.extended_memory_size == 0 {
            return Err(0x90);
        }
        if self.hma_allocated {
            return Err(0x91);
        }
        if size == 0 {
            return Err(0x92);
        }
        self.hma_allocated = true;
        Ok(())
    }

    pub(crate) fn xms_release_hma(&mut self) -> Result<(), u8> {
        if !self.hma_allocated {
            return Err(0x93);
        }
        self.hma_allocated = false;
        Ok(())
    }

    pub(crate) fn xms_query_free(&self) -> (u16, u16) {
        let total_kb = self.extended_memory_size / 1024;
        let used_kb = self.allocated_xms_kb();
        let free_kb = total_kb.saturating_sub(used_kb);
        let free_kb_u16 = free_kb.min(0xFFFF) as u16;
        (free_kb_u16, free_kb_u16)
    }

    pub(crate) fn xms_allocate(&mut self, size_kb: u16) -> Result<u16, u8> {
        let handle_index = self.find_free_xms_handle().ok_or(0xA1u8)?;
        let size_bytes = (size_kb as u64) * 1024;
        let aligned_size = ((size_bytes + ALIGN_MASK) & !ALIGN_MASK) as u32;

        if aligned_size == 0 {
            let handle = &mut self.xms_handles[handle_index as usize];
            handle.active = true;
            handle.allocation = None;
            handle.size_kb = 0;
            handle.lock_count = 0;
            return Ok(handle_index + 1);
        }

        let allocation = self.allocator.allocate(aligned_size).ok_or(0xA0u8)?;
        let handle = &mut self.xms_handles[handle_index as usize];
        handle.active = true;
        handle.allocation = Some(allocation);
        handle.size_kb = size_kb as u32;
        handle.lock_count = 0;
        Ok(handle_index + 1)
    }

    pub(crate) fn xms_free(&mut self, handle_id: u16) -> Result<(), u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        if self.xms_handles[index].lock_count > 0 {
            return Err(0xAB);
        }
        if let Some(alloc) = self.xms_handles[index].allocation.take() {
            self.allocator.deallocate(alloc);
        }
        self.xms_handles[index].active = false;
        self.xms_handles[index].size_kb = 0;
        Ok(())
    }

    pub(crate) fn xms_move(&self, mem: &mut dyn MemoryAccess, params_addr: u32) -> Result<(), u8> {
        let length =
            mem.read_word(params_addr) as u32 | ((mem.read_word(params_addr + 2) as u32) << 16);
        let src_handle_id = mem.read_word(params_addr + 4);
        let src_offset =
            mem.read_word(params_addr + 6) as u32 | ((mem.read_word(params_addr + 8) as u32) << 16);
        let dst_handle_id = mem.read_word(params_addr + 10);
        let dst_offset = mem.read_word(params_addr + 12) as u32
            | ((mem.read_word(params_addr + 14) as u32) << 16);

        if length == 0 {
            return Ok(());
        }

        let src_addr = if src_handle_id == 0 {
            src_offset
        } else {
            let index = src_handle_id.checked_sub(1).ok_or(0xA3u8)? as usize;
            if index >= self.xms_handles.len() || !self.xms_handles[index].active {
                return Err(0xA3);
            }
            let alloc = self.xms_handles[index].allocation.as_ref().ok_or(0xA4u8)?;
            let base = EXTENDED_RAM_BASE + alloc.offset();
            if src_offset > self.xms_handles[index].size_kb * 1024 {
                return Err(0xA4);
            }
            base + src_offset
        };

        let dst_addr = if dst_handle_id == 0 {
            dst_offset
        } else {
            let index = dst_handle_id.checked_sub(1).ok_or(0xA5u8)? as usize;
            if index >= self.xms_handles.len() || !self.xms_handles[index].active {
                return Err(0xA5);
            }
            let alloc = self.xms_handles[index].allocation.as_ref().ok_or(0xA6u8)?;
            let base = EXTENDED_RAM_BASE + alloc.offset();
            if dst_offset > self.xms_handles[index].size_kb * 1024 {
                return Err(0xA6);
            }
            base + dst_offset
        };

        let mut buf = vec![0u8; length as usize];
        mem.read_block(src_addr, &mut buf);
        mem.write_block(dst_addr, &buf);
        Ok(())
    }

    pub(crate) fn xms_lock(&mut self, handle_id: u16) -> Result<u32, u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        if self.xms_handles[index].lock_count == 0xFF {
            return Err(0xAC);
        }
        self.xms_handles[index].lock_count += 1;
        let addr = match self.xms_handles[index].allocation {
            Some(ref alloc) => EXTENDED_RAM_BASE + alloc.offset(),
            None => EXTENDED_RAM_BASE,
        };
        Ok(addr)
    }

    pub(crate) fn xms_unlock(&mut self, handle_id: u16) -> Result<(), u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        if self.xms_handles[index].lock_count == 0 {
            return Err(0xAA);
        }
        self.xms_handles[index].lock_count -= 1;
        Ok(())
    }

    pub(crate) fn xms_handle_info(&self, handle_id: u16) -> Result<(u8, u16, u16), u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        let lock_count = self.xms_handles[index].lock_count;
        let free_handles = self.free_xms_handles_count();
        let size_kb = self.xms_handles[index].size_kb as u16;
        Ok((lock_count, free_handles, size_kb))
    }

    pub(crate) fn xms_reallocate(&mut self, handle_id: u16, new_size_kb: u16) -> Result<(), u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        if self.xms_handles[index].lock_count > 0 {
            return Err(0xAB);
        }

        if let Some(old_alloc) = self.xms_handles[index].allocation.take() {
            self.allocator.deallocate(old_alloc);
        }

        if new_size_kb == 0 {
            self.xms_handles[index].allocation = None;
            self.xms_handles[index].size_kb = 0;
            return Ok(());
        }

        let size_bytes = (new_size_kb as u64) * 1024;
        let aligned_size = ((size_bytes + ALIGN_MASK) & !ALIGN_MASK) as u32;
        match self.allocator.allocate(aligned_size) {
            Some(alloc) => {
                self.xms_handles[index].allocation = Some(alloc);
                self.xms_handles[index].size_kb = new_size_kb as u32;
                Ok(())
            }
            None => {
                self.xms_handles[index].active = false;
                self.xms_handles[index].size_kb = 0;
                Err(0xA0)
            }
        }
    }

    pub(crate) fn umb_allocate(
        &self,
        paragraphs: u16,
        mem: &mut dyn MemoryAccess,
    ) -> Result<(u16, u16), (u8, u16)> {
        if !self.umb_enabled {
            return Err((0xB1, 0));
        }
        match memory::allocate(mem, UMB_FIRST_MCB_SEGMENT, paragraphs, MCB_OWNER_DOS, 0) {
            Ok(data_segment) => {
                let mcb_segment = data_segment - 1;
                let actual_size = memory::read_mcb_size_pub(mem, mcb_segment);
                Ok((data_segment, actual_size))
            }
            Err((_error_code, largest)) => {
                if largest > 0 {
                    Err((0xB0, largest))
                } else {
                    Err((0xB1, 0))
                }
            }
        }
    }

    pub(crate) fn umb_free(&self, segment: u16, mem: &mut dyn MemoryAccess) -> Result<(), u8> {
        if !self.umb_enabled {
            return Err(0xB2);
        }
        memory::free(mem, UMB_FIRST_MCB_SEGMENT, segment).map_err(|_| 0xB2)
    }

    pub(crate) fn umb_reallocate(
        &self,
        segment: u16,
        new_paragraphs: u16,
        mem: &mut dyn MemoryAccess,
    ) -> Result<(), (u8, u16)> {
        if !self.umb_enabled {
            return Err((0xB2, 0));
        }
        memory::resize(mem, UMB_FIRST_MCB_SEGMENT, segment, new_paragraphs)
            .map_err(|(_code, largest)| (0xB0, largest))
    }

    pub(crate) fn ems_total_kb(&self) -> u32 {
        if self.ems_enabled {
            self.extended_memory_size / 1024
        } else {
            0
        }
    }

    pub(crate) fn ems_free_kb(&self) -> u32 {
        if self.ems_enabled {
            let (free_pages, _) = self.ems_unallocated_pages();
            (free_pages as u32) * (EMS_PAGE_SIZE / 1024)
        } else {
            0
        }
    }

    pub(crate) fn xms_total_kb(&self) -> u32 {
        if self.xms_enabled {
            self.extended_memory_size / 1024
        } else {
            0
        }
    }

    pub(crate) fn xms_free_kb(&self) -> u32 {
        if self.xms_enabled {
            let (_, free) = self.xms_query_free();
            free as u32
        } else {
            0
        }
    }

    pub(crate) fn hma_is_allocated(&self) -> bool {
        self.hma_allocated
    }

    pub(crate) fn is_ems_enabled(&self) -> bool {
        self.ems_enabled
    }

    pub(crate) fn is_xms_enabled(&self) -> bool {
        self.xms_enabled
    }

    pub(crate) fn is_xms_32_enabled(&self) -> bool {
        self.xms_32_enabled
    }

    pub(crate) fn is_umb_enabled(&self) -> bool {
        self.umb_enabled
    }

    pub(crate) fn extended_memory_size_bytes(&self) -> u32 {
        self.extended_memory_size
    }

    pub(crate) fn xms_query_free_32(&self) -> (u32, u32) {
        let total_kb = self.extended_memory_size / 1024;
        let used_kb = self.allocated_xms_kb();
        let free_kb = total_kb.saturating_sub(used_kb);
        (free_kb, free_kb)
    }

    pub(crate) fn xms_allocate_32(&mut self, size_kb: u32) -> Result<u16, u8> {
        let handle_index = self.find_free_xms_handle().ok_or(0xA1u8)?;
        let size_bytes = (size_kb as u64) * 1024;
        let aligned_size = ((size_bytes + ALIGN_MASK) & !ALIGN_MASK) as u32;

        if aligned_size == 0 {
            let handle = &mut self.xms_handles[handle_index as usize];
            handle.active = true;
            handle.allocation = None;
            handle.size_kb = 0;
            handle.lock_count = 0;
            return Ok(handle_index + 1);
        }

        let allocation = self.allocator.allocate(aligned_size).ok_or(0xA0u8)?;
        let handle = &mut self.xms_handles[handle_index as usize];
        handle.active = true;
        handle.allocation = Some(allocation);
        handle.size_kb = size_kb;
        handle.lock_count = 0;
        Ok(handle_index + 1)
    }

    pub(crate) fn xms_handle_info_32(&self, handle_id: u16) -> Result<(u8, u16, u32), u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        let lock_count = self.xms_handles[index].lock_count;
        let free_handles = self.free_xms_handles_count();
        let size_kb = self.xms_handles[index].size_kb;
        Ok((lock_count, free_handles, size_kb))
    }

    pub(crate) fn xms_reallocate_32(&mut self, handle_id: u16, new_size_kb: u32) -> Result<(), u8> {
        let index = handle_id.checked_sub(1).ok_or(0xA2u8)? as usize;
        if index >= self.xms_handles.len() || !self.xms_handles[index].active {
            return Err(0xA2);
        }
        if self.xms_handles[index].lock_count > 0 {
            return Err(0xAB);
        }

        if let Some(old_alloc) = self.xms_handles[index].allocation.take() {
            self.allocator.deallocate(old_alloc);
        }

        if new_size_kb == 0 {
            self.xms_handles[index].allocation = None;
            self.xms_handles[index].size_kb = 0;
            return Ok(());
        }

        let size_bytes = (new_size_kb as u64) * 1024;
        let aligned_size = ((size_bytes + ALIGN_MASK) & !ALIGN_MASK) as u32;
        match self.allocator.allocate(aligned_size) {
            Some(alloc) => {
                self.xms_handles[index].allocation = Some(alloc);
                self.xms_handles[index].size_kb = new_size_kb;
                Ok(())
            }
            None => {
                self.xms_handles[index].active = false;
                self.xms_handles[index].size_kb = 0;
                Err(0xA0)
            }
        }
    }

    pub(crate) fn ems_enable_os_functions(&mut self, provided_key: u32) -> Result<Option<u32>, u8> {
        match self.ems_os_access_key {
            None => {
                let key = 0x4E45_4554;
                self.ems_os_access_key = Some(key);
                self.ems_os_functions_enabled = true;
                Ok(Some(key))
            }
            Some(stored_key) => {
                if provided_key != stored_key {
                    return Err(0xA4);
                }
                self.ems_os_functions_enabled = true;
                Ok(None)
            }
        }
    }

    pub(crate) fn ems_disable_os_functions(
        &mut self,
        provided_key: u32,
    ) -> Result<Option<u32>, u8> {
        match self.ems_os_access_key {
            None => {
                let key = 0x4E45_4554;
                self.ems_os_access_key = Some(key);
                self.ems_os_functions_enabled = false;
                Ok(Some(key))
            }
            Some(stored_key) => {
                if provided_key != stored_key {
                    return Err(0xA4);
                }
                self.ems_os_functions_enabled = false;
                Ok(None)
            }
        }
    }

    pub(crate) fn ems_return_os_access_key(&mut self, provided_key: u32) -> Result<(), u8> {
        match self.ems_os_access_key {
            Some(stored_key) if provided_key == stored_key => {
                self.ems_os_access_key = None;
                self.ems_os_functions_enabled = false;
                Ok(())
            }
            _ => Err(0xA4),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct MockMemory {
        data: HashMap<u32, u8>,
        ext_mem_size: u32,
    }

    impl MockMemory {
        fn new(ext_mem_size_kb: u32) -> Self {
            Self {
                data: HashMap::new(),
                ext_mem_size: ext_mem_size_kb * 1024,
            }
        }
    }

    impl MemoryAccess for MockMemory {
        fn read_byte(&self, address: u32) -> u8 {
            self.data.get(&address).copied().unwrap_or(0x00)
        }

        fn write_byte(&mut self, address: u32, value: u8) {
            self.data.insert(address, value);
        }

        fn read_word(&self, address: u32) -> u16 {
            let lo = self.read_byte(address) as u16;
            let hi = self.read_byte(address + 1) as u16;
            (hi << 8) | lo
        }

        fn write_word(&mut self, address: u32, value: u16) {
            self.write_byte(address, value as u8);
            self.write_byte(address + 1, (value >> 8) as u8);
        }

        fn read_block(&self, address: u32, buf: &mut [u8]) {
            for (i, byte) in buf.iter_mut().enumerate() {
                *byte = self.read_byte(address + i as u32);
            }
        }

        fn write_block(&mut self, address: u32, data: &[u8]) {
            for (i, &byte) in data.iter().enumerate() {
                self.write_byte(address + i as u32, byte);
            }
        }

        fn extended_memory_size(&self) -> u32 {
            self.ext_mem_size
        }
    }

    fn create_manager(pool_kb: u32) -> (MemoryManager, MockMemory) {
        create_manager_selective(pool_kb, true, true)
    }

    fn create_manager_selective(pool_kb: u32, ems: bool, xms: bool) -> (MemoryManager, MockMemory) {
        let mut mem = MockMemory::new(pool_kb);
        let mm = MemoryManager::new(pool_kb * 1024, ems, xms, true, &mut mem);
        (mm, mem)
    }

    #[test]
    fn test_new_both_enabled() {
        let (mm, _mem) = create_manager(1024);
        assert!(mm.is_ems_enabled());
        assert!(mm.is_xms_enabled());
        assert!(mm.is_umb_enabled());
        assert!(!mm.hma_is_allocated());
    }

    #[test]
    fn test_new_ems_only() {
        let (mm, _mem) = create_manager_selective(1024, true, false);
        assert!(mm.is_ems_enabled());
        assert!(!mm.is_xms_enabled());
        assert!(mm.is_umb_enabled());
    }

    #[test]
    fn test_new_xms_only() {
        let (mm, _mem) = create_manager_selective(1024, false, true);
        assert!(!mm.is_ems_enabled());
        assert!(mm.is_xms_enabled());
        assert!(mm.is_umb_enabled());
    }

    #[test]
    fn test_new_both_disabled() {
        let (mm, _mem) = create_manager_selective(1024, false, false);
        assert!(!mm.is_ems_enabled());
        assert!(!mm.is_xms_enabled());
        assert!(!mm.is_umb_enabled());
    }

    #[test]
    fn test_initial_free_pages() {
        let (mm, _mem) = create_manager(1024);
        let (free, total) = mm.ems_unallocated_pages();
        assert_eq!(total, 64);
        assert_eq!(free, 64);
        let (largest, total_free) = mm.xms_query_free();
        assert_eq!(total_free, 1024);
        assert_eq!(largest, 1024);
    }

    #[test]
    fn test_ems_status_enabled() {
        let (mm, _mem) = create_manager(1024);
        assert_eq!(mm.ems_status(), 0x00);
    }

    #[test]
    fn test_ems_status_disabled() {
        let (mm, _mem) = create_manager_selective(1024, false, true);
        assert_eq!(mm.ems_status(), 0x84);
    }

    #[test]
    fn test_ems_page_frame_segment() {
        let (mm, _mem) = create_manager(1024);
        assert_eq!(mm.ems_page_frame_segment(), 0xC000);
    }

    #[test]
    fn test_ems_total_and_free_kb() {
        let (mm, _mem) = create_manager(1024);
        assert_eq!(mm.ems_total_kb(), 1024);
        assert_eq!(mm.ems_free_kb(), 1024);

        let (mm_disabled, _mem) = create_manager_selective(1024, false, true);
        assert_eq!(mm_disabled.ems_total_kb(), 0);
        assert_eq!(mm_disabled.ems_free_kb(), 0);
    }

    #[test]
    fn test_ems_allocate_single_page() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_handle_count(), 1);
        assert_eq!(mm.ems_handle_pages(handle).unwrap(), 1);
        let (free, _) = mm.ems_unallocated_pages();
        assert_eq!(free, 63);
    }

    #[test]
    fn test_ems_allocate_multiple_pages() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(4).unwrap();
        assert_eq!(mm.ems_handle_pages(handle).unwrap(), 4);
        let (free, _) = mm.ems_unallocated_pages();
        assert_eq!(free, 60);
    }

    #[test]
    fn test_ems_allocate_two_handles() {
        let (mut mm, _mem) = create_manager(1024);
        let h1 = mm.ems_allocate_pages(2).unwrap();
        let h2 = mm.ems_allocate_pages(3).unwrap();
        assert_ne!(h1, h2);
        assert_eq!(mm.ems_handle_count(), 2);
        let (free, _) = mm.ems_unallocated_pages();
        assert_eq!(free, 59);
    }

    #[test]
    fn test_ems_allocate_zero_rejected() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.ems_allocate_pages(0), Err(0x89));
    }

    #[test]
    fn test_ems_allocate_zero_allowed() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages_zero_allowed(0).unwrap();
        assert_eq!(mm.ems_handle_pages(handle).unwrap(), 0);
        assert_eq!(mm.ems_handle_count(), 1);
    }

    #[test]
    fn test_ems_allocate_exceeds_free() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.ems_allocate_pages(65), Err(0x88));
    }

    #[test]
    fn test_ems_allocate_exhausts_handles() {
        let (mut mm, _mem) = create_manager(4096);
        for _ in 0..MAX_EMS_HANDLES {
            mm.ems_allocate_pages_zero_allowed(0).unwrap();
        }
        assert_eq!(mm.ems_allocate_pages_zero_allowed(0), Err(0x85));
    }

    #[test]
    fn test_ems_deallocate_valid() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(4).unwrap();
        assert_eq!(mm.ems_deallocate(handle, &mut mem), 0x00);
        let (free, total) = mm.ems_unallocated_pages();
        assert_eq!(free, total);
        assert!(!mm.ems_is_valid_handle(handle));
        assert_eq!(mm.ems_handle_count(), 0);
    }

    #[test]
    fn test_ems_deallocate_invalid_handle() {
        let (mut mm, mut mem) = create_manager(1024);
        assert_eq!(mm.ems_deallocate(200, &mut mem), 0x83);
    }

    #[test]
    fn test_ems_deallocate_frees_for_reuse() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(60).unwrap();
        assert_eq!(mm.ems_deallocate(handle, &mut mem), 0x00);
        let handle2 = mm.ems_allocate_pages(60).unwrap();
        assert_eq!(mm.ems_handle_pages(handle2).unwrap(), 60);
    }

    #[test]
    fn test_ems_map_page_success() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();
        assert_eq!(mm.ems_map_page(handle, 0, 0, &mut mem), 0x00);
    }

    #[test]
    fn test_ems_map_page_invalid_physical() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_map_page(handle, 0, 4, &mut mem), 0x8B);
    }

    #[test]
    fn test_ems_map_page_invalid_handle() {
        let (mut mm, mut mem) = create_manager(1024);
        assert_eq!(mm.ems_map_page(200, 0, 0, &mut mem), 0x83);
    }

    #[test]
    fn test_ems_map_page_invalid_logical() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();
        assert_eq!(mm.ems_map_page(handle, 2, 0, &mut mem), 0x8A);
    }

    #[test]
    fn test_ems_map_and_read_back_data() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        let offset = mm.ems_page_allocation_offset(handle, 0).unwrap();

        // Write a pattern into extended RAM at the allocation's offset.
        let pattern = [0xDE, 0xAD, 0xBE, 0xEF];
        mem.write_block(EXTENDED_RAM_BASE + offset, &pattern);

        // Map page to physical slot 0 (copies data into page frame at 0xC0000).
        assert_eq!(mm.ems_map_page(handle, 0, 0, &mut mem), 0x00);

        // Read from page frame; should match the pattern.
        let mut buf = [0u8; 4];
        mem.read_block(EMS_PAGE_FRAME_BASE, &mut buf);
        assert_eq!(buf, pattern);
    }

    #[test]
    fn test_ems_unmap_page() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        mm.ems_map_page(handle, 0, 0, &mut mem);
        assert_eq!(mm.ems_unmap_page(0, &mut mem), 0x00);
        assert_eq!(mm.ems_unmap_page(4, &mut mem), 0x8B);
    }

    #[test]
    fn test_ems_map_saves_previous_content() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();

        // Map logical page 0 to physical 0 and write a marker.
        mm.ems_map_page(handle, 0, 0, &mut mem);
        mem.write_byte(EMS_PAGE_FRAME_BASE, 0xAA);

        // Map logical page 1 to physical 0 (saves page 0's content back to extended RAM).
        mm.ems_map_page(handle, 1, 0, &mut mem);
        mem.write_byte(EMS_PAGE_FRAME_BASE, 0xBB);

        // Map logical page 0 back to physical 0 (saves page 1, loads page 0).
        mm.ems_map_page(handle, 0, 0, &mut mem);
        assert_eq!(mem.read_byte(EMS_PAGE_FRAME_BASE), 0xAA);
    }

    #[test]
    fn test_ems_save_page_map_success() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_save_page_map(handle), 0x00);
    }

    #[test]
    fn test_ems_save_already_saved() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        mm.ems_save_page_map(handle);
        assert_eq!(mm.ems_save_page_map(handle), 0x8D);
    }

    #[test]
    fn test_ems_save_invalid_handle() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.ems_save_page_map(200), 0x83);
    }

    #[test]
    fn test_ems_restore_success() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();

        mm.ems_map_page(handle, 0, 0, &mut mem);
        mem.write_byte(EMS_PAGE_FRAME_BASE, 0xAA);
        mm.ems_save_page_map(handle);

        mm.ems_map_page(handle, 1, 0, &mut mem);
        mem.write_byte(EMS_PAGE_FRAME_BASE, 0xBB);

        assert_eq!(mm.ems_restore_page_map(handle, &mut mem), 0x00);
        assert_eq!(mem.read_byte(EMS_PAGE_FRAME_BASE), 0xAA);
    }

    #[test]
    fn test_ems_restore_no_save() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_restore_page_map(handle, &mut mem), 0x8E);
    }

    #[test]
    fn test_ems_get_set_page_map() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();

        mm.ems_map_page(handle, 0, 0, &mut mem);
        mm.ems_map_page(handle, 1, 1, &mut mem);
        mem.write_byte(EMS_PAGE_FRAME_BASE, 0x11);
        mem.write_byte(EMS_PAGE_FRAME_BASE + EMS_PAGE_SIZE, 0x22);

        let saved = mm.ems_get_page_map();
        assert!(saved[0].is_some());
        assert!(saved[1].is_some());
        assert!(saved[2].is_none());
        assert!(saved[3].is_none());

        mm.ems_unmap_page(0, &mut mem);
        mm.ems_unmap_page(1, &mut mem);

        mm.ems_set_page_map(saved, &mut mem);
        assert_eq!(mem.read_byte(EMS_PAGE_FRAME_BASE), 0x11);
        assert_eq!(mem.read_byte(EMS_PAGE_FRAME_BASE + EMS_PAGE_SIZE), 0x22);
    }

    #[test]
    fn test_ems_reallocate_grow() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();
        assert_eq!(mm.ems_reallocate(handle, 4), Ok(4));
        assert_eq!(mm.ems_handle_pages(handle).unwrap(), 4);
    }

    #[test]
    fn test_ems_reallocate_shrink() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(4).unwrap();
        let (free_before, _) = mm.ems_unallocated_pages();
        assert_eq!(mm.ems_reallocate(handle, 2), Ok(2));
        assert_eq!(mm.ems_handle_pages(handle).unwrap(), 2);
        let (free_after, _) = mm.ems_unallocated_pages();
        assert_eq!(free_after, free_before + 2);
    }

    #[test]
    fn test_ems_reallocate_same() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(3).unwrap();
        assert_eq!(mm.ems_reallocate(handle, 3), Ok(3));
    }

    #[test]
    fn test_ems_reallocate_exceeds_free() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(2).unwrap();
        assert_eq!(mm.ems_reallocate(handle, 100), Err(0x88));
    }

    #[test]
    fn test_ems_handle_name_default() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_handle_name(handle).unwrap(), [0u8; 8]);
    }

    #[test]
    fn test_ems_set_and_get_name() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_set_handle_name(handle, *b"TESTNAME"), 0x00);
        assert_eq!(mm.ems_handle_name(handle).unwrap(), *b"TESTNAME");
    }

    #[test]
    fn test_ems_name_duplicate_rejected() {
        let (mut mm, _mem) = create_manager(1024);
        let h1 = mm.ems_allocate_pages(1).unwrap();
        let h2 = mm.ems_allocate_pages(1).unwrap();
        mm.ems_set_handle_name(h1, *b"MYNAME\0\0");
        assert_eq!(mm.ems_set_handle_name(h2, *b"MYNAME\0\0"), 0x92);
    }

    #[test]
    fn test_ems_name_zero_allows_duplicate() {
        let (mut mm, _mem) = create_manager(1024);
        let h1 = mm.ems_allocate_pages(1).unwrap();
        let h2 = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_set_handle_name(h1, [0u8; 8]), 0x00);
        assert_eq!(mm.ems_set_handle_name(h2, [0u8; 8]), 0x00);
    }

    #[test]
    fn test_ems_search_handle_name() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        mm.ems_set_handle_name(handle, *b"FINDME\0\0");
        assert_eq!(mm.ems_search_handle_name(b"FINDME\0\0"), Ok(handle));
        assert_eq!(mm.ems_search_handle_name(b"NOTEXIST"), Err(0xA0));
    }

    #[test]
    fn test_ems_handle_count() {
        let (mut mm, mut mem) = create_manager(1024);
        assert_eq!(mm.ems_handle_count(), 0);
        let h1 = mm.ems_allocate_pages(1).unwrap();
        let h2 = mm.ems_allocate_pages(1).unwrap();
        let h3 = mm.ems_allocate_pages(1).unwrap();
        assert_eq!(mm.ems_handle_count(), 3);
        mm.ems_deallocate(h2, &mut mem);
        assert_eq!(mm.ems_handle_count(), 2);
        mm.ems_deallocate(h1, &mut mem);
        mm.ems_deallocate(h3, &mut mem);
    }

    #[test]
    fn test_ems_all_handle_pages() {
        let (mut mm, _mem) = create_manager(1024);
        let h1 = mm.ems_allocate_pages(2).unwrap();
        let h2 = mm.ems_allocate_pages(5).unwrap();
        let all = mm.ems_all_handle_pages();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&(h1, 2)));
        assert!(all.contains(&(h2, 5)));
    }

    #[test]
    fn test_ems_handle_directory() {
        let (mut mm, _mem) = create_manager(1024);
        let h1 = mm.ems_allocate_pages(1).unwrap();
        let h2 = mm.ems_allocate_pages(1).unwrap();
        let h3 = mm.ems_allocate_pages(1).unwrap();
        mm.ems_set_handle_name(h1, *b"ALPHA\0\0\0");
        mm.ems_set_handle_name(h2, *b"BETA\0\0\0\0");
        mm.ems_set_handle_name(h3, *b"GAMMA\0\0\0");
        let dir = mm.ems_handle_directory();
        assert_eq!(dir.len(), 3);
        assert!(dir.contains(&(h1, *b"ALPHA\0\0\0")));
        assert!(dir.contains(&(h2, *b"BETA\0\0\0\0")));
        assert!(dir.contains(&(h3, *b"GAMMA\0\0\0")));
    }

    #[test]
    fn test_ems_move_conventional_to_ems() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();

        let pattern = [0xDE, 0xAD, 0xBE, 0xEF];
        mem.write_block(0x1000, &pattern);

        let params = EmsMoveParams {
            region_length: 4,
            src_type: 0,
            src_handle: 0,
            src_offset: 0,
            src_seg_page: 0x100,
            dst_type: 1,
            dst_handle: handle,
            dst_offset: 0,
            dst_seg_page: 0,
        };
        assert_eq!(mm.ems_move_memory(&params, &mut mem), 0x00);

        let offset = mm.ems_page_allocation_offset(handle, 0).unwrap();
        let mut buf = [0u8; 4];
        mem.read_block(EXTENDED_RAM_BASE + offset, &mut buf);
        assert_eq!(buf, pattern);
    }

    #[test]
    fn test_ems_move_ems_to_conventional() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.ems_allocate_pages(1).unwrap();
        let offset = mm.ems_page_allocation_offset(handle, 0).unwrap();

        let pattern = [0xCA, 0xFE, 0xBA, 0xBE];
        mem.write_block(EXTENDED_RAM_BASE + offset, &pattern);

        let params = EmsMoveParams {
            region_length: 4,
            src_type: 1,
            src_handle: handle,
            src_offset: 0,
            src_seg_page: 0,
            dst_type: 0,
            dst_handle: 0,
            dst_offset: 0,
            dst_seg_page: 0x300,
        };
        assert_eq!(mm.ems_move_memory(&params, &mut mem), 0x00);

        let mut buf = [0u8; 4];
        mem.read_block(0x3000, &mut buf);
        assert_eq!(buf, pattern);
    }

    #[test]
    fn test_ems_move_ems_to_ems() {
        let (mut mm, mut mem) = create_manager(1024);
        let h1 = mm.ems_allocate_pages(1).unwrap();
        let h2 = mm.ems_allocate_pages(1).unwrap();
        let offset1 = mm.ems_page_allocation_offset(h1, 0).unwrap();

        let pattern = [0x01, 0x02, 0x03, 0x04];
        mem.write_block(EXTENDED_RAM_BASE + offset1, &pattern);

        let params = EmsMoveParams {
            region_length: 4,
            src_type: 1,
            src_handle: h1,
            src_offset: 0,
            src_seg_page: 0,
            dst_type: 1,
            dst_handle: h2,
            dst_offset: 0,
            dst_seg_page: 0,
        };
        assert_eq!(mm.ems_move_memory(&params, &mut mem), 0x00);

        let offset2 = mm.ems_page_allocation_offset(h2, 0).unwrap();
        let mut buf = [0u8; 4];
        mem.read_block(EXTENDED_RAM_BASE + offset2, &mut buf);
        assert_eq!(buf, pattern);
    }

    #[test]
    fn test_ems_move_zero_length() {
        let (mm, mut mem) = create_manager(1024);
        let params = EmsMoveParams {
            region_length: 0,
            src_type: 0,
            src_handle: 0,
            src_offset: 0,
            src_seg_page: 0,
            dst_type: 0,
            dst_handle: 0,
            dst_offset: 0,
            dst_seg_page: 0,
        };
        assert_eq!(mm.ems_move_memory(&params, &mut mem), 0x00);
    }

    #[test]
    fn test_ems_move_error_cases() {
        let (mm, mut mem) = create_manager(1024);

        let invalid_handle = EmsMoveParams {
            region_length: 4,
            src_type: 1,
            src_handle: 200,
            src_offset: 0,
            src_seg_page: 0,
            dst_type: 0,
            dst_handle: 0,
            dst_offset: 0,
            dst_seg_page: 0,
        };
        assert_eq!(mm.ems_move_memory(&invalid_handle, &mut mem), 0x83);

        let invalid_type = EmsMoveParams {
            region_length: 4,
            src_type: 2,
            src_handle: 0,
            src_offset: 0,
            src_seg_page: 0,
            dst_type: 0,
            dst_handle: 0,
            dst_offset: 0,
            dst_seg_page: 0,
        };
        assert_eq!(mm.ems_move_memory(&invalid_type, &mut mem), 0x98);

        let overflow = EmsMoveParams {
            region_length: 0x100000,
            src_type: 0,
            src_handle: 0,
            src_offset: 0,
            src_seg_page: 0x1000,
            dst_type: 0,
            dst_handle: 0,
            dst_offset: 0,
            dst_seg_page: 0,
        };
        assert_eq!(mm.ems_move_memory(&overflow, &mut mem), 0x96);
    }

    #[test]
    fn test_xms_version() {
        let (mm, _mem) = create_manager(1024);
        let (version, revision, has_ext) = mm.xms_version();
        assert_eq!(version, 0x0300);
        assert_eq!(revision, 0x0001);
        assert_eq!(has_ext, 1);

        let (mm_zero, _mem) = create_manager_selective(0, false, true);
        let (_, _, has_ext_zero) = mm_zero.xms_version();
        assert_eq!(has_ext_zero, 0);
    }

    #[test]
    fn test_xms_total_and_free_kb() {
        let (mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_total_kb(), 1024);
        assert_eq!(mm.xms_free_kb(), 1024);

        let (mm_disabled, _mem) = create_manager_selective(1024, true, false);
        assert_eq!(mm_disabled.xms_total_kb(), 0);
        assert_eq!(mm_disabled.xms_free_kb(), 0);
    }

    #[test]
    fn test_xms_request_hma() {
        let (mut mm, _mem) = create_manager(1024);
        assert!(mm.xms_request_hma(64).is_ok());
        assert!(mm.hma_is_allocated());
    }

    #[test]
    fn test_xms_request_hma_twice() {
        let (mut mm, _mem) = create_manager(1024);
        mm.xms_request_hma(64).unwrap();
        assert_eq!(mm.xms_request_hma(64), Err(0x91));
    }

    #[test]
    fn test_xms_request_hma_zero_size() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_request_hma(0), Err(0x92));
    }

    #[test]
    fn test_xms_release_hma() {
        let (mut mm, _mem) = create_manager(1024);
        mm.xms_request_hma(64).unwrap();
        assert!(mm.xms_release_hma().is_ok());
        assert!(!mm.hma_is_allocated());
        assert_eq!(mm.xms_release_hma(), Err(0x93));
    }

    #[test]
    fn test_xms_allocate_basic() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        assert!(handle >= 1);
        let (largest, total) = mm.xms_query_free();
        assert!(total < 1024);
        assert!(largest <= total);
    }

    #[test]
    fn test_xms_allocate_zero_kb() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(0).unwrap();
        assert!(handle >= 1);
        let (_, _, size_kb) = mm.xms_handle_info(handle).unwrap();
        assert_eq!(size_kb, 0);
    }

    #[test]
    fn test_xms_allocate_multiple() {
        let (mut mm, _mem) = create_manager(1024);
        let h1 = mm.xms_allocate(100).unwrap();
        let h2 = mm.xms_allocate(200).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_xms_allocate_exceeds_free() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_allocate(2000), Err(0xA0));
    }

    #[test]
    fn test_xms_allocate_exhausts_handles() {
        let (mut mm, _mem) = create_manager(1024);
        for _ in 0..MAX_XMS_HANDLES {
            mm.xms_allocate(0).unwrap();
        }
        assert_eq!(mm.xms_allocate(0), Err(0xA1));
    }

    #[test]
    fn test_xms_free_basic() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        let (_, before) = mm.xms_query_free();
        assert!(mm.xms_free(handle).is_ok());
        let (_, after) = mm.xms_query_free();
        assert!(after > before);
    }

    #[test]
    fn test_xms_free_invalid_handle() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_free(0), Err(0xA2));
        assert_eq!(mm.xms_free(200), Err(0xA2));
    }

    #[test]
    fn test_xms_free_locked() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        mm.xms_lock(handle).unwrap();
        assert_eq!(mm.xms_free(handle), Err(0xAB));
    }

    #[test]
    fn test_xms_lock_returns_address() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        let addr = mm.xms_lock(handle).unwrap();
        assert!(addr >= EXTENDED_RAM_BASE);
    }

    #[test]
    fn test_xms_lock_increments_count() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        mm.xms_lock(handle).unwrap();
        mm.xms_lock(handle).unwrap();
        let (lock_count, _, _) = mm.xms_handle_info(handle).unwrap();
        assert_eq!(lock_count, 2);
    }

    #[test]
    fn test_xms_lock_overflow() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        for _ in 0..255 {
            mm.xms_lock(handle).unwrap();
        }
        assert_eq!(mm.xms_lock(handle), Err(0xAC));
    }

    #[test]
    fn test_xms_unlock_basic() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        mm.xms_lock(handle).unwrap();
        assert!(mm.xms_unlock(handle).is_ok());
    }

    #[test]
    fn test_xms_unlock_not_locked() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        assert_eq!(mm.xms_unlock(handle), Err(0xAA));
    }

    #[test]
    fn test_xms_handle_info() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        let (lock_count, free_handles, size_kb) = mm.xms_handle_info(handle).unwrap();
        assert_eq!(lock_count, 0);
        assert_eq!(free_handles, (MAX_XMS_HANDLES - 1) as u16);
        assert_eq!(size_kb, 64);
    }

    #[test]
    fn test_xms_handle_info_invalid() {
        let (mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_handle_info(0), Err(0xA2));
    }

    #[test]
    fn test_xms_reallocate_grow() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        assert!(mm.xms_reallocate(handle, 128).is_ok());
        let (_, _, size) = mm.xms_handle_info(handle).unwrap();
        assert_eq!(size, 128);
    }

    #[test]
    fn test_xms_reallocate_shrink() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(128).unwrap();
        assert!(mm.xms_reallocate(handle, 32).is_ok());
        let (_, _, size) = mm.xms_handle_info(handle).unwrap();
        assert_eq!(size, 32);
    }

    #[test]
    fn test_xms_reallocate_to_zero() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        assert!(mm.xms_reallocate(handle, 0).is_ok());
        let (_, _, size) = mm.xms_handle_info(handle).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_xms_reallocate_locked() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();
        mm.xms_lock(handle).unwrap();
        assert_eq!(mm.xms_reallocate(handle, 128), Err(0xAB));
    }

    #[test]
    fn test_xms_move_conventional_to_xms() {
        let (mut mm, mut mem) = create_manager(1024);
        let handle = mm.xms_allocate(64).unwrap();

        let pattern = [0xDE, 0xAD, 0xBE, 0xEF];
        mem.write_block(0x5000, &pattern);

        let params_addr = 0x6000u32;
        mem.write_word(params_addr, 4);
        mem.write_word(params_addr + 2, 0);
        mem.write_word(params_addr + 4, 0);
        mem.write_word(params_addr + 6, 0x5000);
        mem.write_word(params_addr + 8, 0);
        mem.write_word(params_addr + 10, handle);
        mem.write_word(params_addr + 12, 0);
        mem.write_word(params_addr + 14, 0);

        assert!(mm.xms_move(&mut mem, params_addr).is_ok());

        let addr = mm.xms_lock(handle).unwrap();
        let mut buf = [0u8; 4];
        mem.read_block(addr, &mut buf);
        assert_eq!(buf, pattern);
    }

    #[test]
    fn test_xms_move_xms_to_xms() {
        let (mut mm, mut mem) = create_manager(1024);
        let h1 = mm.xms_allocate(64).unwrap();
        let h2 = mm.xms_allocate(64).unwrap();

        let addr1 = mm.xms_lock(h1).unwrap();
        let pattern = [0xCA, 0xFE, 0xBA, 0xBE];
        mem.write_block(addr1, &pattern);

        let params_addr = 0x6000u32;
        mem.write_word(params_addr, 4);
        mem.write_word(params_addr + 2, 0);
        mem.write_word(params_addr + 4, h1);
        mem.write_word(params_addr + 6, 0);
        mem.write_word(params_addr + 8, 0);
        mem.write_word(params_addr + 10, h2);
        mem.write_word(params_addr + 12, 0);
        mem.write_word(params_addr + 14, 0);

        assert!(mm.xms_move(&mut mem, params_addr).is_ok());

        let addr2 = mm.xms_lock(h2).unwrap();
        let mut buf = [0u8; 4];
        mem.read_block(addr2, &mut buf);
        assert_eq!(buf, pattern);
    }

    #[test]
    fn test_xms_move_zero_length() {
        let (mm, mut mem) = create_manager(1024);
        let params_addr = 0x6000u32;
        mem.write_word(params_addr, 0);
        mem.write_word(params_addr + 2, 0);
        mem.write_word(params_addr + 4, 0);
        mem.write_word(params_addr + 6, 0);
        mem.write_word(params_addr + 8, 0);
        mem.write_word(params_addr + 10, 0);
        mem.write_word(params_addr + 12, 0);
        mem.write_word(params_addr + 14, 0);
        assert!(mm.xms_move(&mut mem, params_addr).is_ok());
    }

    #[test]
    fn test_umb_allocate_basic() {
        let (mm, mut mem) = create_manager(1024);
        let (segment, size) = mm.umb_allocate(16, &mut mem).unwrap();
        assert!(segment > UMB_FIRST_MCB_SEGMENT);
        assert!(size >= 16);
    }

    #[test]
    fn test_umb_allocate_disabled() {
        let (mm, mut mem) = create_manager_selective(1024, false, false);
        assert_eq!(mm.umb_allocate(16, &mut mem), Err((0xB1, 0)));
    }

    #[test]
    fn test_umb_allocate_too_large() {
        let (mm, mut mem) = create_manager(1024);
        let result = mm.umb_allocate(0xFFFF, &mut mem);
        assert!(result.is_err());
        let (code, largest) = result.unwrap_err();
        assert_eq!(code, 0xB0);
        assert!(largest > 0);
    }

    #[test]
    fn test_umb_free_basic() {
        let (mm, mut mem) = create_manager(1024);
        let (segment, _) = mm.umb_allocate(16, &mut mem).unwrap();
        assert!(mm.umb_free(segment, &mut mem).is_ok());
    }

    #[test]
    fn test_umb_reallocate() {
        let (mm, mut mem) = create_manager(1024);
        let (segment, _) = mm.umb_allocate(32, &mut mem).unwrap();
        assert!(mm.umb_reallocate(segment, 16, &mut mem).is_ok());
    }

    #[test]
    fn test_ems_and_xms_share_pool() {
        let (mut mm, _mem) = create_manager(1024);
        let (free_pages, _) = mm.ems_unallocated_pages();
        mm.ems_allocate_pages(free_pages).unwrap();

        // Pool is now fully consumed by EMS. XMS allocation should fail.
        assert!(mm.xms_allocate(1).is_err());
    }

    #[test]
    fn test_allocate_deallocate_cycle() {
        let (mut mm, mut mem) = create_manager(1024);
        let (_, initial_total) = mm.xms_query_free();

        let ems_h = mm.ems_allocate_pages(4).unwrap();
        let xms_h = mm.xms_allocate(64).unwrap();

        let (free_after_alloc, _) = mm.xms_query_free();
        assert!(free_after_alloc < initial_total);

        mm.xms_free(xms_h).unwrap();
        mm.ems_deallocate(ems_h, &mut mem);

        let (free_after_dealloc, _) = mm.xms_query_free();
        assert_eq!(free_after_dealloc, initial_total);
    }

    #[test]
    fn test_xms_32_enabled_flag() {
        let (mm, _mem) = create_manager(1024);
        assert!(mm.is_xms_32_enabled());

        let mut mem = MockMemory::new(1024);
        let mm2 = MemoryManager::new(1024 * 1024, true, true, false, &mut mem);
        assert!(!mm2.is_xms_32_enabled());
    }

    #[test]
    fn test_xms_query_free_32() {
        let (mm, _mem) = create_manager(1024);
        let (largest, total) = mm.xms_query_free_32();
        assert_eq!(largest, 1024);
        assert_eq!(total, 1024);
    }

    #[test]
    fn test_xms_allocate_32_basic() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(64).unwrap();
        assert!(handle >= 1);
        let (_, _, size_kb) = mm.xms_handle_info_32(handle).unwrap();
        assert_eq!(size_kb, 64);
    }

    #[test]
    fn test_xms_allocate_32_zero() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(0).unwrap();
        assert!(handle >= 1);
        let (_, _, size_kb) = mm.xms_handle_info_32(handle).unwrap();
        assert_eq!(size_kb, 0);
    }

    #[test]
    fn test_xms_allocate_32_exceeds_free() {
        let (mut mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_allocate_32(2000), Err(0xA0));
    }

    #[test]
    fn test_xms_allocate_32_exhausts_handles() {
        let (mut mm, _mem) = create_manager(1024);
        for _ in 0..MAX_XMS_HANDLES {
            mm.xms_allocate_32(0).unwrap();
        }
        assert_eq!(mm.xms_allocate_32(0), Err(0xA1));
    }

    #[test]
    fn test_xms_handle_info_32() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(64).unwrap();
        let (lock_count, free_handles, size_kb) = mm.xms_handle_info_32(handle).unwrap();
        assert_eq!(lock_count, 0);
        assert_eq!(free_handles, (MAX_XMS_HANDLES - 1) as u16);
        assert_eq!(size_kb, 64);
    }

    #[test]
    fn test_xms_handle_info_32_invalid() {
        let (mm, _mem) = create_manager(1024);
        assert_eq!(mm.xms_handle_info_32(0), Err(0xA2));
    }

    #[test]
    fn test_xms_reallocate_32_grow() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(64).unwrap();
        assert!(mm.xms_reallocate_32(handle, 128).is_ok());
        let (_, _, size) = mm.xms_handle_info_32(handle).unwrap();
        assert_eq!(size, 128);
    }

    #[test]
    fn test_xms_reallocate_32_shrink() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(128).unwrap();
        assert!(mm.xms_reallocate_32(handle, 32).is_ok());
        let (_, _, size) = mm.xms_handle_info_32(handle).unwrap();
        assert_eq!(size, 32);
    }

    #[test]
    fn test_xms_reallocate_32_to_zero() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(64).unwrap();
        assert!(mm.xms_reallocate_32(handle, 0).is_ok());
        let (_, _, size) = mm.xms_handle_info_32(handle).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_xms_reallocate_32_locked() {
        let (mut mm, _mem) = create_manager(1024);
        let handle = mm.xms_allocate_32(64).unwrap();
        mm.xms_lock(handle).unwrap();
        assert_eq!(mm.xms_reallocate_32(handle, 128), Err(0xAB));
    }

    #[test]
    fn test_ems_enable_os_functions_first_call() {
        let (mut mm, _mem) = create_manager(1024);
        let result = mm.ems_enable_os_functions(0);
        assert!(result.is_ok());
        let key = result.unwrap();
        assert!(key.is_some());
        assert_ne!(key.unwrap(), 0);
    }

    #[test]
    fn test_ems_enable_os_functions_subsequent_call() {
        let (mut mm, _mem) = create_manager(1024);
        let key = mm.ems_enable_os_functions(0).unwrap().unwrap();
        let result = mm.ems_enable_os_functions(key);
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_ems_enable_os_functions_wrong_key() {
        let (mut mm, _mem) = create_manager(1024);
        mm.ems_enable_os_functions(0).unwrap();
        assert_eq!(mm.ems_enable_os_functions(0xDEAD), Err(0xA4));
    }

    #[test]
    fn test_ems_disable_os_functions_first_call() {
        let (mut mm, _mem) = create_manager(1024);
        let result = mm.ems_disable_os_functions(0);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_ems_disable_os_functions_correct_key() {
        let (mut mm, _mem) = create_manager(1024);
        let key = mm.ems_enable_os_functions(0).unwrap().unwrap();
        assert_eq!(mm.ems_disable_os_functions(key), Ok(None));
    }

    #[test]
    fn test_ems_disable_os_functions_wrong_key() {
        let (mut mm, _mem) = create_manager(1024);
        mm.ems_enable_os_functions(0).unwrap();
        assert_eq!(mm.ems_disable_os_functions(0xDEAD), Err(0xA4));
    }

    #[test]
    fn test_ems_return_os_access_key() {
        let (mut mm, _mem) = create_manager(1024);
        let key = mm.ems_enable_os_functions(0).unwrap().unwrap();
        assert!(mm.ems_return_os_access_key(key).is_ok());
    }

    #[test]
    fn test_ems_return_os_access_key_wrong() {
        let (mut mm, _mem) = create_manager(1024);
        mm.ems_enable_os_functions(0).unwrap();
        assert_eq!(mm.ems_return_os_access_key(0xDEAD), Err(0xA4));
    }

    #[test]
    fn test_ems_return_os_access_key_enables_new_key() {
        let (mut mm, _mem) = create_manager(1024);
        let key1 = mm.ems_enable_os_functions(0).unwrap().unwrap();
        mm.ems_return_os_access_key(key1).unwrap();
        let key2 = mm.ems_enable_os_functions(0).unwrap().unwrap();
        assert_ne!(key2, 0);
    }
}
