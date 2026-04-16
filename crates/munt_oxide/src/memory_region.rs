// Copyright (C) 2003, 2004, 2005, 2006, 2008, 2009 Dean Beeler, Jerome Fisher
// Copyright (C) 2011-2026 Dean Beeler, Jerome Fisher, Sergey V. Mikayev
//
//  This program is free software: you can redistribute it and/or modify
//  it under the terms of the GNU Lesser General Public License as published by
//  the Free Software Foundation, either version 2.1 of the License, or
//  (at your option) any later version.
//
//  This program is distributed in the hope that it will be useful,
//  but WITHOUT ANY WARRANTY; without even the implied warranty of
//  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//  GNU Lesser General Public License for more details.
//
//  You should have received a copy of the GNU Lesser General Public License
//  along with this program.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
    state::{MemoryRegionDescriptor, MemoryRegionDescriptors},
    structures::{
        PaddedTimbre, PatchParam, PatchTemp, RhythmTemp, SystemParam, TimbreParam, memaddr,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryRegionType {
    PatchTemp,
    RhythmTemp,
    TimbreTemp,
    Patches,
    Timbres,
    System,
    Reset,
}

pub(crate) const MEMORY_REGION_TYPES: [MemoryRegionType; 7] = [
    MemoryRegionType::PatchTemp,
    MemoryRegionType::RhythmTemp,
    MemoryRegionType::TimbreTemp,
    MemoryRegionType::Patches,
    MemoryRegionType::Timbres,
    MemoryRegionType::System,
    MemoryRegionType::Reset,
];

fn get_max_value(max_table: Option<&[u8]>, entry_size: u32, off: u32) -> u8 {
    match max_table {
        None => 0xFF,
        Some(table) => table[(off % entry_size) as usize],
    }
}

impl MemoryRegionDescriptor {
    pub(crate) fn last_touched(&self, addr: u32, len: u32) -> u32 {
        (self.offset(addr) + len - 1) / self.entry_size
    }

    pub(crate) fn first_touched_offset(&self, addr: u32) -> u32 {
        self.offset(addr) % self.entry_size
    }

    pub(crate) fn first_touched(&self, addr: u32) -> u32 {
        self.offset(addr) / self.entry_size
    }

    pub(crate) fn region_end(&self) -> u32 {
        self.start_addr + self.entry_size * self.entries
    }

    pub(crate) fn contains(&self, addr: u32) -> bool {
        addr >= self.start_addr && addr < self.region_end()
    }

    pub(crate) fn offset(&self, addr: u32) -> u32 {
        addr - self.start_addr
    }

    pub(crate) fn get_clamped_len(&self, addr: u32, len: u32) -> u32 {
        let end = self.region_end();
        if addr + len > end {
            return end - addr;
        }
        len
    }

    pub(crate) fn next_region_len(&self, addr: u32, len: u32) -> u32 {
        let end = self.region_end();
        if addr + len > end {
            return end - addr;
        }
        0
    }

    // This method should never be called with out-of-bounds parameters,
    // or on an unsupported region - seeing any of this debug output indicates a bug in the emulator
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write(
        &self,
        real_memory: Option<&mut [u8]>,
        max_table: Option<&[u8]>,
        entry: u32,
        off: u32,
        src: &[u8],
        len: u32,
        init: bool,
    ) {
        let mem_off = entry * self.entry_size + off;
        let mut len = len;

        // NOTE: The original C++ write() checks bounds using the relative `off` instead of the
        // absolute `memOff` (Synth.cpp), unlike C++ read() which correctly uses absolute
        // offsets (Synth.cpp). This causes writes to high-numbered entries to skip boundary
        // clamping. We fix this by using mem_off for bounds checking.
        if mem_off > self.entry_size * self.entries - 1 {
            return;
        }
        if mem_off + len > self.entry_size * self.entries {
            len = self.entry_size * self.entries - mem_off;
        }
        let dest = match real_memory {
            Some(mem) => mem,
            None => return,
        };

        for (mem_off, &src_value) in (mem_off..).zip(src.iter().take(len as usize)) {
            let mut desired_value = src_value;
            let max_value = get_max_value(max_table, self.entry_size, mem_off);
            // maxValue == 0 means write-protected unless called from initialisation code, in which case it really means the maximum value is 0.
            if max_value != 0 || init {
                if desired_value > max_value {
                    desired_value = max_value;
                }
                dest[mem_off as usize] = desired_value;
            }
        }
    }
}

impl MemoryRegionDescriptors {
    pub(crate) fn new() -> Self {
        Self {
            patch_temp: MemoryRegionDescriptor {
                start_addr: memaddr(0x030000),
                entry_size: PatchTemp::SIZE as u32,
                entries: 9,
            },
            rhythm_temp: MemoryRegionDescriptor {
                start_addr: memaddr(0x030110),
                entry_size: RhythmTemp::SIZE as u32,
                entries: 85,
            },
            timbre_temp: MemoryRegionDescriptor {
                start_addr: memaddr(0x040000),
                entry_size: TimbreParam::SIZE as u32,
                entries: 8,
            },
            patches: MemoryRegionDescriptor {
                start_addr: memaddr(0x050000),
                entry_size: PatchParam::SIZE as u32,
                entries: 128,
            },
            timbres: MemoryRegionDescriptor {
                start_addr: memaddr(0x080000),
                entry_size: PaddedTimbre::SIZE as u32,
                entries: 64 + 64 + 64 + 64,
            },
            system: MemoryRegionDescriptor {
                start_addr: memaddr(0x100000),
                entry_size: SystemParam::SIZE as u32,
                entries: 1,
            },
            reset: MemoryRegionDescriptor {
                start_addr: memaddr(0x7F0000),
                entry_size: 0x3FFF,
                entries: 1,
            },
        }
    }

    pub(crate) fn get_descriptor(&self, region_type: MemoryRegionType) -> &MemoryRegionDescriptor {
        match region_type {
            MemoryRegionType::PatchTemp => &self.patch_temp,
            MemoryRegionType::RhythmTemp => &self.rhythm_temp,
            MemoryRegionType::TimbreTemp => &self.timbre_temp,
            MemoryRegionType::Patches => &self.patches,
            MemoryRegionType::Timbres => &self.timbres,
            MemoryRegionType::System => &self.system,
            MemoryRegionType::Reset => &self.reset,
        }
    }

    pub(crate) fn find_region(&self, addr: u32) -> Option<MemoryRegionType> {
        for &region_type in &MEMORY_REGION_TYPES {
            let descriptor = self.get_descriptor(region_type);
            if descriptor.contains(addr) {
                return Some(region_type);
            }
        }
        None
    }
}
