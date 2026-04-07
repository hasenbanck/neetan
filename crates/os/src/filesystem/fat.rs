//! FAT12/FAT16 read/write implementation.

use crate::{DiskIo, filesystem::fat_bpb::Bpb};

/// A mounted FAT12 or FAT16 volume.
pub(crate) struct FatVolume {
    pub drive_da: u8,
    pub partition_offset: u32,
    pub bpb: Bpb,
    pub first_root_sector: u32,
    pub first_data_sector: u32,
    pub data_cluster_count: u32,
    pub is_fat16: bool,
    fat_cache: Vec<u8>,
    fat_dirty: bool,
}

impl FatVolume {
    /// Mounts a FAT volume by reading the boot sector and loading the FAT.
    pub fn mount(drive_da: u8, partition_offset: u32, disk: &mut dyn DiskIo) -> Result<Self, u16> {
        let boot_sector = disk
            .read_sectors(drive_da, partition_offset, 1)
            .map_err(|_| 0x001Fu16)?;
        let bpb = Bpb::parse(&boot_sector).ok_or(0x001Fu16)?;

        let first_root_sector = bpb.first_root_sector();
        let first_data_sector = bpb.first_data_sector();
        let data_cluster_count = bpb.data_cluster_count();
        let is_fat16 = bpb.is_fat16();

        let fat_start = partition_offset + bpb.reserved_sectors as u32;
        let fat_sectors = bpb.sectors_per_fat as u32;
        let fat_data = disk
            .read_sectors(drive_da, fat_start, fat_sectors)
            .map_err(|_| 0x001Fu16)?;

        Ok(Self {
            drive_da,
            partition_offset,
            bpb,
            first_root_sector,
            first_data_sector,
            data_cluster_count,
            is_fat16,
            fat_cache: fat_data,
            fat_dirty: false,
        })
    }

    /// Reads a FAT entry for the given cluster number.
    pub fn read_fat_entry(&self, cluster: u16) -> u16 {
        if self.is_fat16 {
            let offset = cluster as usize * 2;
            if offset + 1 >= self.fat_cache.len() {
                return 0xFFFF;
            }
            u16::from_le_bytes([self.fat_cache[offset], self.fat_cache[offset + 1]])
        } else {
            // FAT12: 12-bit entries packed into 1.5 bytes each
            let offset = (cluster as usize * 3) / 2;
            if offset + 1 >= self.fat_cache.len() {
                return 0xFFF;
            }
            let pair = u16::from_le_bytes([self.fat_cache[offset], self.fat_cache[offset + 1]]);
            if cluster & 1 != 0 {
                pair >> 4
            } else {
                pair & 0x0FFF
            }
        }
    }

    /// Writes a FAT entry for the given cluster number.
    pub fn write_fat_entry(&mut self, cluster: u16, value: u16) {
        if self.is_fat16 {
            let offset = cluster as usize * 2;
            if offset + 1 >= self.fat_cache.len() {
                return;
            }
            let bytes = value.to_le_bytes();
            self.fat_cache[offset] = bytes[0];
            self.fat_cache[offset + 1] = bytes[1];
        } else {
            let offset = (cluster as usize * 3) / 2;
            if offset + 1 >= self.fat_cache.len() {
                return;
            }
            let existing = u16::from_le_bytes([self.fat_cache[offset], self.fat_cache[offset + 1]]);
            let new_pair = if cluster & 1 != 0 {
                (existing & 0x000F) | ((value & 0x0FFF) << 4)
            } else {
                (existing & 0xF000) | (value & 0x0FFF)
            };
            let bytes = new_pair.to_le_bytes();
            self.fat_cache[offset] = bytes[0];
            self.fat_cache[offset + 1] = bytes[1];
        }
        self.fat_dirty = true;
    }

    /// Returns the next cluster in the chain, or None if end-of-chain.
    pub fn next_cluster(&self, cluster: u16) -> Option<u16> {
        let entry = self.read_fat_entry(cluster);
        let eoc = if self.is_fat16 { 0xFFF8 } else { 0x0FF8 };
        if entry >= eoc || entry < 2 {
            None
        } else {
            Some(entry)
        }
    }

    /// Allocates a free cluster and chains it after `after_cluster`.
    /// If `after_cluster` is 0, the new cluster is not chained (used for first cluster).
    /// Returns the newly allocated cluster number.
    pub fn allocate_cluster(&mut self, after_cluster: u16) -> Option<u16> {
        let max = self.data_cluster_count as u16 + 2;
        let eoc = if self.is_fat16 { 0xFFFF } else { 0x0FFF };
        for candidate in 2..max {
            if self.read_fat_entry(candidate) == 0 {
                self.write_fat_entry(candidate, eoc);
                if after_cluster >= 2 {
                    self.write_fat_entry(after_cluster, candidate);
                }
                return Some(candidate);
            }
        }
        None
    }

    /// Frees an entire cluster chain starting at `start`.
    pub fn free_chain(&mut self, start: u16) {
        let mut cluster = start;
        while cluster >= 2 {
            let next = self.next_cluster(cluster);
            self.write_fat_entry(cluster, 0x0000);
            match next {
                Some(n) => cluster = n,
                None => break,
            }
        }
    }

    /// Converts a cluster number to an absolute LBA sector number.
    pub fn cluster_to_lba(&self, cluster: u16) -> u32 {
        self.partition_offset
            + self.first_data_sector
            + (cluster as u32 - 2) * self.bpb.sectors_per_cluster as u32
    }

    /// Returns the absolute LBA of the first root directory sector.
    pub fn root_dir_lba(&self) -> u32 {
        self.partition_offset + self.first_root_sector
    }

    /// Number of sectors in the root directory.
    pub fn root_dir_sectors(&self) -> u32 {
        self.bpb.root_dir_sectors()
    }

    /// Reads a single cluster (all sectors) into a Vec.
    pub fn read_cluster(&self, cluster: u16, disk: &mut dyn DiskIo) -> Result<Vec<u8>, u16> {
        let lba = self.cluster_to_lba(cluster);
        let count = self.bpb.sectors_per_cluster as u32;
        disk.read_sectors(self.drive_da, lba, count)
            .map_err(|_| 0x001F)
    }

    /// Writes a full cluster to disk.
    pub fn write_cluster(
        &self,
        cluster: u16,
        data: &[u8],
        disk: &mut dyn DiskIo,
    ) -> Result<(), u16> {
        let lba = self.cluster_to_lba(cluster);
        disk.write_sectors(self.drive_da, lba, data)
            .map_err(|_| 0x001F)
    }

    /// Reads a single sector at an absolute LBA.
    pub fn read_sector_abs(&self, abs_lba: u32, disk: &mut dyn DiskIo) -> Result<Vec<u8>, u16> {
        disk.read_sectors(self.drive_da, abs_lba, 1)
            .map_err(|_| 0x001F)
    }

    /// Writes a single sector at an absolute LBA.
    pub fn write_sector_abs(
        &self,
        abs_lba: u32,
        data: &[u8],
        disk: &mut dyn DiskIo,
    ) -> Result<(), u16> {
        disk.write_sectors(self.drive_da, abs_lba, data)
            .map_err(|_| 0x001F)
    }

    /// Flushes the cached FAT to all FAT copies on disk.
    pub fn flush_fat(&mut self, disk: &mut dyn DiskIo) -> Result<(), u16> {
        if !self.fat_dirty {
            return Ok(());
        }
        for fat_idx in 0..self.bpb.num_fats as u32 {
            let fat_start = self.partition_offset
                + self.bpb.reserved_sectors as u32
                + fat_idx * self.bpb.sectors_per_fat as u32;
            disk.write_sectors(self.drive_da, fat_start, &self.fat_cache)
                .map_err(|_| 0x001Fu16)?;
        }
        self.fat_dirty = false;
        Ok(())
    }

    /// Walks the cluster chain from `start` and returns the cluster at position
    /// `index` in the chain (0 = start itself). Returns None if chain is shorter.
    pub fn cluster_at_index(&self, start: u16, index: u32) -> Option<u16> {
        let mut current = start;
        for _ in 0..index {
            current = self.next_cluster(current)?;
        }
        Some(current)
    }

    /// Returns the last cluster in the chain starting at `start`.
    pub fn last_cluster(&self, start: u16) -> u16 {
        let mut current = start;
        while let Some(next) = self.next_cluster(current) {
            current = next;
        }
        current
    }
}
