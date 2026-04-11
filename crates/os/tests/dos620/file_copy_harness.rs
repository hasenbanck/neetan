use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use common::Bus;
use device::{
    disk::{self, HddImage},
    floppy::{
        FloppyImage,
        d88::{D88Disk, D88MediaType, D88Sector},
    },
};

use crate::harness::*;

pub const RANDOM_FILE_FCB: [u8; 11] = *b"RAND    BIN";
pub const RANDOM_FILE_SIZE: usize = 0x4977;
const FAT12_EOC: u16 = 0x0FFF;

#[derive(Debug, Clone, Copy)]
struct BpbInfo {
    partition_offset: u32,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entry_count: u16,
    sectors_per_fat: u16,
    is_fat16: bool,
}

pub fn prng_bytes(len: usize) -> Vec<u8> {
    let mut state = 0x1234_5678u32;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        out.push((state >> 16) as u8);
    }
    out
}

fn set_fat12_entry(fat: &mut [u8], cluster: u16, value: u16) {
    let value = value & 0x0FFF;
    let offset = (cluster as usize * 3) / 2;
    if cluster & 1 == 0 {
        fat[offset] = (value & 0x00FF) as u8;
        fat[offset + 1] = (fat[offset + 1] & 0xF0) | ((value >> 8) as u8 & 0x0F);
    } else {
        fat[offset] = (fat[offset] & 0x0F) | (((value << 4) as u8) & 0xF0);
        fat[offset + 1] = (value >> 4) as u8;
    }
}

pub fn create_random_file_floppy(file_data: &[u8]) -> FloppyImage {
    create_random_file_floppy_with_name(&RANDOM_FILE_FCB, file_data)
}

pub fn create_random_file_floppy_with_name(fcb_name: &[u8; 11], file_data: &[u8]) -> FloppyImage {
    let cylinders = 77usize;
    let heads = 2usize;
    let sectors_per_track = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;
    let total_sectors = cylinders * heads * sectors_per_track;
    let mut disk_data = vec![0u8; total_sectors * sector_size];

    {
        let bpb = &mut disk_data[0..sector_size];
        bpb[0] = 0xEB;
        bpb[1] = 0x3C;
        bpb[2] = 0x90;
        bpb[3..11].copy_from_slice(b"NEETAN  ");
        bpb[11..13].copy_from_slice(&1024u16.to_le_bytes());
        bpb[13] = 1;
        bpb[14..16].copy_from_slice(&1u16.to_le_bytes());
        bpb[16] = 2;
        bpb[17..19].copy_from_slice(&192u16.to_le_bytes());
        bpb[19..21].copy_from_slice(&1232u16.to_le_bytes());
        bpb[21] = 0xFE;
        bpb[22..24].copy_from_slice(&2u16.to_le_bytes());
        bpb[24..26].copy_from_slice(&8u16.to_le_bytes());
        bpb[26..28].copy_from_slice(&2u16.to_le_bytes());
    }

    let fat1_offset = sector_size;
    let fat = &mut disk_data[fat1_offset..fat1_offset + 2 * sector_size];
    fat[0] = 0xFE;
    fat[1] = 0xFF;
    fat[2] = 0xFF;
    set_fat12_entry(fat, 2, FAT12_EOC);
    set_fat12_entry(fat, 3, FAT12_EOC);

    let random_cluster_count = file_data.len().div_ceil(sector_size);
    for index in 0..random_cluster_count {
        let cluster = 4 + index as u16;
        let next = if index + 1 == random_cluster_count {
            FAT12_EOC
        } else {
            cluster + 1
        };
        set_fat12_entry(fat, cluster, next);
    }

    let fat2_offset = 3 * sector_size;
    let fat_copy = disk_data[fat1_offset..fat1_offset + 2 * sector_size].to_vec();
    disk_data[fat2_offset..fat2_offset + fat_copy.len()].copy_from_slice(&fat_copy);

    let root_offset = 5 * sector_size;
    {
        let entry = &mut disk_data[root_offset..root_offset + 32];
        entry[0..11].copy_from_slice(b"COMMAND COM");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&2u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 32..root_offset + 64];
        entry[0..11].copy_from_slice(b"TESTFILETXT");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&3u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 64..root_offset + 96];
        entry[0..11].copy_from_slice(fcb_name);
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&4u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(file_data.len() as u32).to_le_bytes());
    }

    let cluster2_offset = 11 * sector_size;
    disk_data[cluster2_offset..cluster2_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);
    let cluster3_offset = 12 * sector_size;
    disk_data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);

    for (index, chunk) in file_data.chunks(sector_size).enumerate() {
        let lba = 13 + index;
        let offset = lba * sector_size;
        disk_data[offset..offset + chunk.len()].copy_from_slice(chunk);
    }

    let mut tracks = Vec::with_capacity(total_tracks);
    for track_index in 0..total_tracks {
        let cylinder = (track_index / heads) as u8;
        let head = (track_index % heads) as u8;
        let mut sectors = Vec::with_capacity(sectors_per_track);
        for sector in 0..sectors_per_track {
            let lba = track_index * sectors_per_track + sector;
            let offset = lba * sector_size;
            sectors.push(D88Sector {
                cylinder,
                head,
                record: (sector + 1) as u8,
                size_code: 3,
                sector_count: sectors_per_track as u16,
                mfm_flag: 0x40,
                deleted: 0,
                status: 0,
                reserved: [0; 5],
                data: disk_data[offset..offset + sector_size].to_vec(),
            });
        }
        tracks.push(Some(sectors));
    }

    let d88 = D88Disk::from_tracks("XRAND".to_string(), false, D88MediaType::Disk2HD, tracks);
    FloppyImage::from_d88(d88)
}

pub fn create_broken_chain_floppy_with_name(
    fcb_name: &[u8; 11],
    advertised_size: usize,
) -> FloppyImage {
    let first_cluster_data = prng_bytes(1024);
    let cylinders = 77usize;
    let heads = 2usize;
    let sectors_per_track = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;
    let total_sectors = cylinders * heads * sectors_per_track;
    let mut disk_data = vec![0u8; total_sectors * sector_size];

    {
        let bpb = &mut disk_data[0..sector_size];
        bpb[0] = 0xEB;
        bpb[1] = 0x3C;
        bpb[2] = 0x90;
        bpb[3..11].copy_from_slice(b"NEETAN  ");
        bpb[11..13].copy_from_slice(&1024u16.to_le_bytes());
        bpb[13] = 1;
        bpb[14..16].copy_from_slice(&1u16.to_le_bytes());
        bpb[16] = 2;
        bpb[17..19].copy_from_slice(&192u16.to_le_bytes());
        bpb[19..21].copy_from_slice(&1232u16.to_le_bytes());
        bpb[21] = 0xFE;
        bpb[22..24].copy_from_slice(&2u16.to_le_bytes());
        bpb[24..26].copy_from_slice(&8u16.to_le_bytes());
        bpb[26..28].copy_from_slice(&2u16.to_le_bytes());
    }

    let fat1_offset = sector_size;
    let fat = &mut disk_data[fat1_offset..fat1_offset + 2 * sector_size];
    fat[0] = 0xFE;
    fat[1] = 0xFF;
    fat[2] = 0xFF;
    set_fat12_entry(fat, 2, FAT12_EOC);
    set_fat12_entry(fat, 3, FAT12_EOC);
    set_fat12_entry(fat, 4, FAT12_EOC);

    let fat2_offset = 3 * sector_size;
    let fat_copy = disk_data[fat1_offset..fat1_offset + 2 * sector_size].to_vec();
    disk_data[fat2_offset..fat2_offset + fat_copy.len()].copy_from_slice(&fat_copy);

    let root_offset = 5 * sector_size;
    {
        let entry = &mut disk_data[root_offset..root_offset + 32];
        entry[0..11].copy_from_slice(b"COMMAND COM");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&2u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 32..root_offset + 64];
        entry[0..11].copy_from_slice(b"TESTFILETXT");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&3u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 64..root_offset + 96];
        entry[0..11].copy_from_slice(fcb_name);
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&4u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(advertised_size as u32).to_le_bytes());
    }

    let cluster2_offset = 11 * sector_size;
    disk_data[cluster2_offset..cluster2_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);
    let cluster3_offset = 12 * sector_size;
    disk_data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);
    let cluster4_offset = 13 * sector_size;
    disk_data[cluster4_offset..cluster4_offset + first_cluster_data.len()]
        .copy_from_slice(&first_cluster_data);

    let mut tracks = Vec::with_capacity(total_tracks);
    for track_index in 0..total_tracks {
        let cylinder = (track_index / heads) as u8;
        let head = (track_index % heads) as u8;
        let mut sectors = Vec::with_capacity(sectors_per_track);
        for sector in 0..sectors_per_track {
            let lba = track_index * sectors_per_track + sector;
            let offset = lba * sector_size;
            sectors.push(D88Sector {
                cylinder,
                head,
                record: (sector + 1) as u8,
                size_code: 3,
                sector_count: sectors_per_track as u16,
                mfm_flag: 0x40,
                deleted: 0,
                status: 0,
                reserved: [0; 5],
                data: disk_data[offset..offset + sector_size].to_vec(),
            });
        }
        tracks.push(Some(sectors));
    }

    let d88 = D88Disk::from_tracks("BROKEN".to_string(), false, D88MediaType::Disk2HD, tracks);
    FloppyImage::from_d88(d88)
}

pub fn make_temp_hdd_path(prefix: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after unix epoch");
    std::env::temp_dir().join(format!(
        "neetan-{prefix}-{}-{}.nhd",
        std::process::id(),
        now.as_nanos()
    ))
}

pub fn load_hdd_from_path(path: &Path) -> HddImage {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    disk::load_hdd_image(path, &bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

pub fn boot_hle_with_temp_hdd_and_floppy(
    hdd_path: &Path,
    floppy: FloppyImage,
) -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();
    machine.bus.write_byte(0x055C, 0x01);
    machine.bus.write_byte(0x055D, 0x01);
    machine.bus.insert_hdd(
        0,
        load_hdd_from_path(hdd_path),
        Some(hdd_path.to_path_buf()),
    );

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(1_000_000);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < 500_000_000,
            "HLE OS did not show prompt with HDD+FDD test setup"
        );
    }

    machine.bus.insert_floppy(0, floppy, None);
    machine
}

fn find_partition_offset(hdd: &HddImage) -> u32 {
    let sector = hdd.read_sector(1).expect("partition table sector");
    for entry in sector.chunks_exact(32).take(16) {
        if entry[0] == 0 && entry[1] == 0 {
            break;
        }
        let mid = entry[0];
        let sid = entry[1];
        if sid & 0x80 == 0 || mid & 0x70 != 0x20 {
            continue;
        }
        let start_sector = entry[8] as u32;
        let start_head = entry[9] as u32;
        let start_cylinder = u16::from_le_bytes([entry[10], entry[11]]) as u32;
        return (start_cylinder * hdd.geometry.heads as u32 + start_head)
            * hdd.geometry.sectors_per_track as u32
            + start_sector;
    }
    0
}

fn read_bpb(hdd: &HddImage) -> BpbInfo {
    let partition_offset = find_partition_offset(hdd);
    let boot = hdd
        .read_sector(partition_offset)
        .expect("partition boot sector");
    let total_sectors_16 = u16::from_le_bytes([boot[19], boot[20]]) as u32;
    let total_sectors_32 = u32::from_le_bytes([boot[32], boot[33], boot[34], boot[35]]);
    let total_sectors = if total_sectors_16 != 0 {
        total_sectors_16
    } else {
        total_sectors_32
    };
    let reserved_sectors = u16::from_le_bytes([boot[14], boot[15]]);
    let num_fats = boot[16];
    let root_entry_count = u16::from_le_bytes([boot[17], boot[18]]);
    let sectors_per_fat = u16::from_le_bytes([boot[22], boot[23]]);
    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let root_dir_sectors = (root_entry_count as u32 * 32).div_ceil(bytes_per_sector as u32);
    let first_data_sector =
        reserved_sectors as u32 + num_fats as u32 * sectors_per_fat as u32 + root_dir_sectors;
    let data_cluster_count =
        total_sectors.saturating_sub(first_data_sector) / sectors_per_cluster as u32;
    BpbInfo {
        partition_offset,
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        root_entry_count,
        sectors_per_fat,
        is_fat16: data_cluster_count >= 4085,
    }
}

fn read_logical_sector(hdd: &HddImage, bpb: &BpbInfo, lba: u32) -> Option<Vec<u8>> {
    let physical = hdd.geometry.sector_size as usize;
    let logical = bpb.bytes_per_sector as usize;
    let ratio = logical / physical;
    let mut data = Vec::with_capacity(logical);
    for part in 0..ratio as u32 {
        data.extend_from_slice(hdd.read_sector(bpb.partition_offset + lba * ratio as u32 + part)?);
    }
    Some(data)
}

fn read_fat_entry(fat: &[u8], cluster: u16, is_fat16: bool) -> u16 {
    if is_fat16 {
        let offset = cluster as usize * 2;
        u16::from_le_bytes([fat[offset], fat[offset + 1]])
    } else {
        let offset = (cluster as usize * 3) / 2;
        let pair = u16::from_le_bytes([fat[offset], fat[offset + 1]]);
        if cluster & 1 == 0 {
            pair & 0x0FFF
        } else {
            pair >> 4
        }
    }
}

pub fn extract_root_file(hdd_path: &Path, fcb_name: &[u8; 11]) -> Result<Vec<u8>, String> {
    let hdd = load_hdd_from_path(hdd_path);
    let bpb = read_bpb(&hdd);
    let root_dir_sectors = (bpb.root_entry_count as u32 * 32).div_ceil(bpb.bytes_per_sector as u32);
    let fat_start = bpb.reserved_sectors as u32;
    let root_start = fat_start + bpb.num_fats as u32 * bpb.sectors_per_fat as u32;
    let data_start = root_start + root_dir_sectors;

    let mut fat = Vec::with_capacity(bpb.sectors_per_fat as usize * bpb.bytes_per_sector as usize);
    for sector in 0..bpb.sectors_per_fat as u32 {
        let bytes = read_logical_sector(&hdd, &bpb, fat_start + sector)
            .ok_or_else(|| format!("failed to read FAT logical sector {}", fat_start + sector))?;
        fat.extend_from_slice(&bytes);
    }

    let mut root = Vec::with_capacity(root_dir_sectors as usize * bpb.bytes_per_sector as usize);
    for sector in 0..root_dir_sectors {
        let bytes = read_logical_sector(&hdd, &bpb, root_start + sector).ok_or_else(|| {
            format!(
                "failed to read root directory logical sector {}",
                root_start + sector
            )
        })?;
        root.extend_from_slice(&bytes);
    }

    let mut start_cluster = None;
    let mut file_size = None;
    for entry in root.chunks_exact(32) {
        if entry[0] == 0x00 {
            break;
        }
        if entry[0] == 0xE5 {
            continue;
        }
        if entry[0..11] == fcb_name[..] {
            start_cluster = Some(u16::from_le_bytes([entry[26], entry[27]]));
            file_size = Some(u32::from_le_bytes([
                entry[28], entry[29], entry[30], entry[31],
            ]));
            break;
        }
    }

    let mut cluster = start_cluster.ok_or_else(|| "destination file not found".to_string())?;
    let file_size = file_size.ok_or_else(|| "destination file size missing".to_string())?;
    let mut data = Vec::with_capacity(file_size as usize);
    let mut chain = Vec::new();

    let eoc = if bpb.is_fat16 { 0xFFF8 } else { 0x0FF8 };
    while cluster >= 2 && cluster < eoc && data.len() < file_size as usize {
        chain.push(cluster);
        let first_sector = data_start + (cluster as u32 - 2) * bpb.sectors_per_cluster as u32;
        for sector in 0..bpb.sectors_per_cluster as u32 {
            let logical_sector = first_sector + sector;
            let bytes = read_logical_sector(&hdd, &bpb, logical_sector).ok_or_else(|| {
                format!(
                    "cluster chain left disk: cluster={cluster:04X} logical_sector={logical_sector} chain={chain:?}"
                )
            })?;
            data.extend_from_slice(&bytes);
        }
        if data.len() >= file_size as usize {
            break;
        }
        let next = read_fat_entry(&fat, cluster, bpb.is_fat16);
        if next == 0 || next == cluster {
            break;
        }
        cluster = next;
    }

    data.truncate(file_size as usize);
    Ok(data)
}

pub fn mismatch_offsets(lhs: &[u8], rhs: &[u8], limit: usize) -> Vec<usize> {
    lhs.iter()
        .zip(rhs.iter())
        .enumerate()
        .filter_map(|(index, (left, right))| (left != right).then_some(index))
        .take(limit)
        .collect()
}
