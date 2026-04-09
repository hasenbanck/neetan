//! INT 67h: Expanded Memory Manager (EMS 4.0).

use crate::{CpuAccess, MemoryAccess, NeetanOs, memory::memory_manager::EmsMoveParams};

impl NeetanOs {
    pub(crate) fn int67h(&mut self, cpu: &mut dyn CpuAccess, memory: &mut dyn MemoryAccess) {
        let mm = match self.state.memory_manager {
            Some(ref mut mm) if mm.is_ems_enabled() => mm,
            _ => {
                cpu.set_ax((cpu.ax() & 0x00FF) | 0x8400);
                return;
            }
        };

        let ah = (cpu.ax() >> 8) as u8;
        match ah {
            0x40 => {
                let status = mm.ems_status();
                cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
            }
            0x41 => {
                let segment = mm.ems_page_frame_segment();
                cpu.set_bx(segment);
                cpu.set_ax(cpu.ax() & 0x00FF);
            }
            0x42 => {
                let (free, total) = mm.ems_unallocated_pages();
                cpu.set_bx(free);
                cpu.set_dx(total);
                cpu.set_ax(cpu.ax() & 0x00FF);
            }
            0x43 => {
                let count = cpu.bx();
                match mm.ems_allocate_pages(count) {
                    Ok(handle) => {
                        cpu.set_dx(handle);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    Err(code) => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                    }
                }
            }
            0x44 => {
                let physical_page = cpu.ax() as u8;
                let logical_page = cpu.bx();
                let handle = cpu.dx();
                let status = if logical_page == 0xFFFF {
                    mm.ems_unmap_page(physical_page, memory)
                } else {
                    mm.ems_map_page(handle, logical_page, physical_page, memory)
                };
                cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
            }
            0x45 => {
                let handle = cpu.dx();
                let status = mm.ems_deallocate(handle, memory);
                cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
            }
            0x46 => {
                cpu.set_ax(0x0040);
            }
            0x47 => {
                let handle = cpu.dx();
                let status = mm.ems_save_page_map(handle);
                cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
            }
            0x48 => {
                let handle = cpu.dx();
                let status = mm.ems_restore_page_map(handle, memory);
                cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
            }
            0x4B => {
                let count = mm.ems_handle_count();
                cpu.set_bx(count);
                cpu.set_ax(cpu.ax() & 0x00FF);
            }
            0x4C => {
                let handle = cpu.dx();
                match mm.ems_handle_pages(handle) {
                    Ok(pages) => {
                        cpu.set_bx(pages);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    Err(code) => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                    }
                }
            }
            0x4D => {
                let buffer_addr = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                let handles = mm.ems_all_handle_pages();
                for (i, &(handle, pages)) in handles.iter().enumerate() {
                    let entry_addr = buffer_addr + (i as u32) * 4;
                    memory.write_word(entry_addr, handle);
                    memory.write_word(entry_addr + 2, pages);
                }
                cpu.set_bx(handles.len() as u16);
                cpu.set_ax(cpu.ax() & 0x00FF);
            }
            0x4E => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let dest_addr = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                        let map = mm.ems_get_page_map();
                        for (i, slot) in map.iter().enumerate() {
                            let addr = dest_addr + (i as u32) * 4;
                            match slot {
                                Some(m) => {
                                    memory.write_word(addr, m.handle);
                                    memory.write_word(addr + 2, m.logical_page);
                                }
                                None => {
                                    memory.write_word(addr, 0xFFFF);
                                    memory.write_word(addr + 2, 0xFFFF);
                                }
                            }
                        }
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    0x01 => {
                        let src_addr = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
                        let map = read_page_map_from_memory(mm, memory, src_addr);
                        mm.ems_set_page_map(map, memory);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    0x02 => {
                        let dest_addr = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                        let old_map = mm.ems_get_page_map();
                        for (i, slot) in old_map.iter().enumerate() {
                            let addr = dest_addr + (i as u32) * 4;
                            match slot {
                                Some(m) => {
                                    memory.write_word(addr, m.handle);
                                    memory.write_word(addr + 2, m.logical_page);
                                }
                                None => {
                                    memory.write_word(addr, 0xFFFF);
                                    memory.write_word(addr + 2, 0xFFFF);
                                }
                            }
                        }

                        let src_addr = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
                        let new_map = read_page_map_from_memory(mm, memory, src_addr);
                        mm.ems_set_page_map(new_map, memory);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    0x03 => {
                        cpu.set_ax(mm.ems_page_map_size());
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x50 => {
                let al = cpu.ax() as u8;
                let handle = cpu.dx();
                let count = cpu.cx();
                let src_addr = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
                match al {
                    0x00 | 0x01 => {
                        let mut status = 0x00u8;
                        for i in 0..count {
                            let entry_addr = src_addr + (i as u32) * 4;
                            let logical_page = memory.read_word(entry_addr);
                            let physical_page = memory.read_word(entry_addr + 2);
                            let result = if logical_page == 0xFFFF {
                                if al == 0x00 {
                                    mm.ems_unmap_page(physical_page as u8, memory)
                                } else {
                                    let segment = physical_page;
                                    let phys = ((segment as u32 - 0xC000) / 0x400) as u8;
                                    mm.ems_unmap_page(phys, memory)
                                }
                            } else if al == 0x00 {
                                mm.ems_map_page(handle, logical_page, physical_page as u8, memory)
                            } else {
                                let segment = physical_page;
                                let phys = ((segment as u32 - 0xC000) / 0x400) as u8;
                                mm.ems_map_page(handle, logical_page, phys, memory)
                            };
                            if result != 0x00 {
                                status = result;
                                break;
                            }
                        }
                        cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x51 => {
                let new_count = cpu.bx();
                let handle = cpu.dx();
                match mm.ems_reallocate(handle, new_count) {
                    Ok(actual) => {
                        cpu.set_bx(actual);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    Err(code) => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                    }
                }
            }
            0x52 => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let handle = cpu.dx();
                        if mm.ems_is_valid_handle(handle) {
                            cpu.set_ax(0x0000);
                        } else {
                            cpu.set_ax(0x8300);
                        }
                    }
                    0x01 => {
                        let handle = cpu.dx();
                        if mm.ems_is_valid_handle(handle) {
                            cpu.set_ax(cpu.ax() & 0x00FF);
                        } else {
                            cpu.set_ax((cpu.ax() & 0x00FF) | 0x8300);
                        }
                    }
                    0x02 => {
                        cpu.set_ax(0x0001);
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x53 => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let handle = cpu.dx();
                        match mm.ems_handle_name(handle) {
                            Ok(name) => {
                                let dest = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                                memory.write_block(dest, &name);
                                cpu.set_ax(cpu.ax() & 0x00FF);
                            }
                            Err(code) => {
                                cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                            }
                        }
                    }
                    0x01 => {
                        let handle = cpu.dx();
                        let src = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
                        let mut name = [0u8; 8];
                        memory.read_block(src, &mut name);
                        let status = mm.ems_set_handle_name(handle, name);
                        cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x54 => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let dest = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                        let dir = mm.ems_handle_directory();
                        for (i, &(handle, ref name)) in dir.iter().enumerate() {
                            let addr = dest + (i as u32) * 10;
                            memory.write_word(addr, handle);
                            memory.write_block(addr + 2, name);
                        }
                        cpu.set_ax(dir.len() as u16);
                    }
                    0x01 => {
                        let src = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
                        let mut name = [0u8; 8];
                        memory.read_block(src, &mut name);
                        match mm.ems_search_handle_name(&name) {
                            Ok(handle) => {
                                cpu.set_dx(handle);
                                cpu.set_ax(cpu.ax() & 0x00FF);
                            }
                            Err(code) => {
                                cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                            }
                        }
                    }
                    0x02 => {
                        cpu.set_ax(MAX_EMS_HANDLES as u16);
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x57 => {
                let al = cpu.ax() as u8;
                let src = ((cpu.ds() as u32) << 4) + cpu.si() as u32;
                let params = EmsMoveParams {
                    region_length: memory.read_word(src) as u32
                        | ((memory.read_word(src + 2) as u32) << 16),
                    src_type: memory.read_byte(src + 4),
                    src_handle: memory.read_word(src + 5),
                    src_offset: memory.read_word(src + 7),
                    src_seg_page: memory.read_word(src + 9),
                    dst_type: memory.read_byte(src + 11),
                    dst_handle: memory.read_word(src + 12),
                    dst_offset: memory.read_word(src + 14),
                    dst_seg_page: memory.read_word(src + 16),
                };

                let status = match al {
                    0x00 | 0x01 => mm.ems_move_memory(&params, memory),
                    _ => 0x8F,
                };
                cpu.set_ax((cpu.ax() & 0x00FF) | ((status as u16) << 8));
            }
            0x58 => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let dest = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                        let segments: [u16; 4] = [0xC000, 0xC400, 0xC800, 0xCC00];
                        for (i, &seg) in segments.iter().enumerate() {
                            let addr = dest + (i as u32) * 4;
                            memory.write_word(addr, seg);
                            memory.write_word(addr + 2, i as u16);
                        }
                        cpu.set_cx(4);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    0x01 => {
                        cpu.set_cx(4);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x59 => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let dest = ((cpu.es() as u32) << 4) + cpu.di() as u32;
                        memory.write_word(dest, 0x0400);
                        memory.write_word(dest + 2, 0x0000);
                        memory.write_word(dest + 4, 0x0000);
                        memory.write_word(dest + 6, 0x0000);
                        memory.write_word(dest + 8, 0x0000);
                        memory.write_word(dest + 10, 0x0000);
                        memory.write_word(dest + 12, 0x0000);
                        memory.write_word(dest + 14, 0x0000);
                        memory.write_word(dest + 16, 0x0000);
                        memory.write_word(dest + 18, 0x0000);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    0x01 => {
                        let (free, total) = mm.ems_unallocated_pages();
                        cpu.set_bx(free);
                        cpu.set_dx(total);
                        cpu.set_ax(cpu.ax() & 0x00FF);
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            0x5A => {
                let al = cpu.ax() as u8;
                match al {
                    0x00 => {
                        let count = cpu.bx();
                        match mm.ems_allocate_pages_zero_allowed(count) {
                            Ok(handle) => {
                                cpu.set_dx(handle);
                                cpu.set_ax(cpu.ax() & 0x00FF);
                            }
                            Err(code) => {
                                cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                            }
                        }
                    }
                    0x01 => {
                        let count = cpu.bx();
                        match mm.ems_allocate_pages_zero_allowed(count) {
                            Ok(handle) => {
                                cpu.set_dx(handle);
                                cpu.set_ax(cpu.ax() & 0x00FF);
                            }
                            Err(code) => {
                                cpu.set_ax((cpu.ax() & 0x00FF) | ((code as u16) << 8));
                            }
                        }
                    }
                    _ => {
                        cpu.set_ax((cpu.ax() & 0x00FF) | 0x8F00);
                    }
                }
            }
            _ => {
                cpu.set_ax((cpu.ax() & 0x00FF) | 0x8400);
            }
        }
    }
}

const MAX_EMS_HANDLES: usize = 255;

fn read_page_map_from_memory(
    mm: &crate::memory::memory_manager::MemoryManager,
    memory: &dyn crate::MemoryAccess,
    src_addr: u32,
) -> [Option<crate::memory::memory_manager::EmsMapping>; 4] {
    let mut map = [None; 4];
    for (i, slot) in map.iter_mut().enumerate() {
        let addr = src_addr + (i as u32) * 4;
        let handle = memory.read_word(addr);
        let page = memory.read_word(addr + 2);
        if handle != 0xFFFF
            && let Some(offset) = mm.ems_page_allocation_offset(handle, page)
        {
            *slot = Some(crate::memory::memory_manager::EmsMapping {
                handle,
                logical_page: page,
                allocation_offset: offset,
            });
        }
    }
    map
}
