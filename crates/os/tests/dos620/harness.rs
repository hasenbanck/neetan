use common::{Bus, JisChar, MachineModel};

static FONT_ROM_DATA: &[u8] = include_bytes!("../../../../utils/font/font.rom");

pub const INJECT_CODE_SEGMENT: u16 = 0x2000;
pub const INJECT_CODE_BASE: u32 = (INJECT_CODE_SEGMENT as u32) << 4;
pub const INJECT_RESULT_OFFSET: u16 = 0x0100;
pub const INJECT_RESULT_BASE: u32 = INJECT_CODE_BASE + INJECT_RESULT_OFFSET as u32;
pub const INJECT_BUDGET: u64 = 50_000_000;
pub const INJECT_BUDGET_DISK_IO: u64 = 500_000_000;

const HLE_BOOT_MAX_CYCLES: u64 = 500_000_000;
const HLE_BOOT_CHECK_INTERVAL: u64 = 1_000_000;

fn set_fat12_entry(fat: &mut [u8], cluster: u16, value: u16) {
    let masked_value = value & 0x0FFF;
    let offset = (cluster as usize * 3) / 2;
    if cluster & 1 == 0 {
        fat[offset] = (masked_value & 0x00FF) as u8;
        fat[offset + 1] = (fat[offset + 1] & 0xF0) | ((masked_value >> 8) as u8 & 0x0F);
    } else {
        fat[offset] = (fat[offset] & 0x0F) | (((masked_value << 4) as u8) & 0xF0);
        fat[offset + 1] = (masked_value >> 4) as u8;
    }
}

/// Creates a machine with no disk images. The bootstrap will find no bootable
/// media and activate NEETAN OS HLE DOS automatically.
pub fn create_hle_machine() -> machine::Pc9801Ra {
    let mut machine = machine::Pc9801Ra::new(
        cpu::I386::new(),
        machine::Pc9801Bus::new(MachineModel::PC9801RA, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);
    machine.bus.set_xms_32_enabled(true);
    machine
}

/// Boots a machine with NEETAN OS HLE DOS (no disk images).
/// Returns the machine after the shell prompt (`>`) is visible in text VRAM.
pub fn boot_hle() -> machine::Pc9801Ra {
    boot_hle_with_time(None)
}

/// Boots a machine with NEETAN OS HLE DOS and an optional fixed time provider.
pub fn boot_hle_with_time(time_fn: Option<fn() -> [u8; 6]>) -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();
    if let Some(f) = time_fn {
        machine.bus.set_host_local_time_fn(f);
    }
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles",
            HLE_BOOT_MAX_CYCLES
        );
    }
    machine
}

pub fn boot_hle_with_forced_os(
    floppy: Option<device::floppy::FloppyImage>,
    hdd: Option<device::disk::HddImage>,
) -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();
    machine.bus.set_boot_device(machine::BootDevice::Os);

    let mut disk_equip_low = 0u8;
    let mut disk_equip_high = 0u8;

    if let Some(floppy_image) = floppy {
        disk_equip_low |= 0x01;
        machine.bus.insert_floppy(0, floppy_image, None);
    }

    if let Some(hdd_image) = hdd {
        disk_equip_high |= 0x01;
        machine.bus.insert_hdd(0, hdd_image, None);
    }

    machine.bus.write_byte(0x055C, disk_equip_low);
    machine.bus.write_byte(0x055D, disk_equip_high);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles",
            HLE_BOOT_MAX_CYCLES
        );
    }

    machine
}

pub fn hle_prompt_visible(bus: &machine::Pc9801Bus) -> bool {
    let vram = bus.text_vram();
    // Scan all rows for '>' (0x003E) which indicates the prompt is displayed
    for row in 0..25 {
        for col in 0..80 {
            let offset = (row * 80 + col) * 2;
            if offset + 1 >= vram.len() {
                break;
            }
            let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            if code == 0x003E {
                return true;
            }
        }
    }
    false
}

/// Known content for COMMAND.COM on the test floppy (100 bytes).
pub const TEST_COMMAND_COM: &[u8] = b"This is a fake COMMAND.COM for testing purposes. \
It contains exactly one hundred bytes of known content!!";

/// Known content for TESTFILE.TXT on the test floppy.
pub const TEST_FILE_CONTENT: &[u8] = b"HELLO WORLD\r\n";

/// Known content for README.TXT on the synthetic test CD-ROM.
pub const TEST_CDROM_README: &[u8] = b"NEETAN CD README\r\n";

/// Tiny .COM program for EXEC testing: terminates with exit code 0x42.
/// MOV AH, 4Ch ; B4 4C
/// MOV AL, 42h ; B0 42
/// INT 21h     ; CD 21
pub const TEST_COM_PROGRAM: &[u8] = &[0xB4, 0x4C, 0xB0, 0x42, 0xCD, 0x21];

/// Known date for files on the test floppy: 1995-01-01.
/// DOS date: ((15)<<9) | (1<<5) | 1 = 0x1E21
pub const TEST_FILE_DATE: u16 = 0x1E21;

/// Known time for files on the test floppy: 12:00:00.
/// DOS time: (12<<11) | (0<<5) | 0 = 0x6000
pub const TEST_FILE_TIME: u16 = 0x6000;

/// Creates a PC-98 2HD floppy (FAT12) with known test files as a D88 FloppyImage.
pub fn create_test_floppy() -> device::floppy::FloppyImage {
    use device::floppy::d88::{D88Disk, D88MediaType, D88Sector};

    // PC-98 2HD: 77 cylinders, 2 heads, 8 sectors/track, 1024 bytes/sector
    let cylinders = 77usize;
    let heads = 2usize;
    let spt = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;

    // Build flat sector data first, then convert to D88 tracks
    let total_sectors = cylinders * heads * spt;
    let mut disk_data = vec![0u8; total_sectors * sector_size];

    // Sector 0: Boot sector with BPB
    {
        let bpb = &mut disk_data[0..sector_size];
        bpb[0] = 0xEB;
        bpb[1] = 0x3C; // JMP short
        bpb[2] = 0x90; // NOP
        bpb[3..11].copy_from_slice(b"NEETAN  ");
        // BPB fields at offset 11
        bpb[11..13].copy_from_slice(&1024u16.to_le_bytes()); // bytes per sector
        bpb[13] = 1; // sectors per cluster
        bpb[14..16].copy_from_slice(&1u16.to_le_bytes()); // reserved sectors
        bpb[16] = 2; // number of FATs
        bpb[17..19].copy_from_slice(&192u16.to_le_bytes()); // root entry count
        bpb[19..21].copy_from_slice(&1232u16.to_le_bytes()); // total sectors (16-bit)
        bpb[21] = 0xFE; // media descriptor
        bpb[22..24].copy_from_slice(&2u16.to_le_bytes()); // sectors per FAT
        bpb[24..26].copy_from_slice(&8u16.to_le_bytes()); // sectors per track
        bpb[26..28].copy_from_slice(&2u16.to_le_bytes()); // number of heads
        // hidden sectors, total sectors 32 stay 0
    }

    // FAT layout:
    // Sector 0: boot, Sectors 1-2: FAT1, Sectors 3-4: FAT2
    // Sectors 5-10: Root directory (192 entries * 32 = 6144 = 6 sectors)
    // Sector 11+: Data area (cluster 2 = sector 11)

    // FAT1 (sectors 1-2)
    let fat1_offset = sector_size;
    disk_data[fat1_offset] = 0xFE; // media descriptor
    disk_data[fat1_offset + 1] = 0xFF;
    disk_data[fat1_offset + 2] = 0xFF;
    // Cluster 2: COMMAND.COM (end of chain = 0xFFF for FAT12)
    // FAT12 entry for cluster 2: bytes at offset 3 (cluster 2 = byte offset 3, even cluster)
    // cluster 2 value = 0xFFF (end of chain)
    // byte[3] = low 8 bits of cluster2 = 0xFF
    // byte[4] = high 4 bits of cluster2 (low nibble) | low 4 bits of cluster3 (high nibble)
    disk_data[fat1_offset + 3] = 0xFF;
    disk_data[fat1_offset + 4] = 0x0F; // cluster2=0xFFF, cluster3=0x000
    // Cluster 3: TESTFILE.TXT (end of chain = 0xFFF)
    // cluster 3 value = 0xFFF
    // byte[4] upper nibble already has cluster3 low nibble. cluster3 = 0xFFF
    // For odd cluster (3): byte[4] = (byte[4] & 0x0F) | ((0xFFF & 0x00F) << 4) = 0x0F | 0xF0 = 0xFF
    // byte[5] = 0xFFF >> 4 = 0xFF
    disk_data[fat1_offset + 4] = 0xFF;
    disk_data[fat1_offset + 5] = 0xFF;
    // Cluster 4: TEST.COM (end of chain = 0xFFF)
    // FAT12 entry for cluster 4 (even cluster): byte offset = 4 * 3 / 2 = 6
    // byte[6] = low 8 bits of cluster4 = 0xFF
    // byte[7] = (high 4 of cluster4) | (low 4 of cluster5 << 4) = 0x0F
    disk_data[fat1_offset + 6] = 0xFF;
    disk_data[fat1_offset + 7] = 0x0F;

    // FAT2 (sectors 3-4) -- copy of FAT1
    let fat2_offset = 3 * sector_size;
    let fat1_end = fat1_offset + 2 * sector_size;
    let fat1_copy: Vec<u8> = disk_data[fat1_offset..fat1_end].to_vec();
    disk_data[fat2_offset..fat2_offset + fat1_copy.len()].copy_from_slice(&fat1_copy);

    // Root directory (sectors 5-10)
    let root_offset = 5 * sector_size;

    // Entry 0: COMMAND.COM
    {
        let e = &mut disk_data[root_offset..root_offset + 32];
        e[0..11].copy_from_slice(b"COMMAND COM");
        e[11] = 0x20; // archive
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&2u16.to_le_bytes()); // start cluster
        e[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }

    // Entry 1: TESTFILE.TXT
    {
        let e = &mut disk_data[root_offset + 32..root_offset + 64];
        e[0..11].copy_from_slice(b"TESTFILETXT");
        e[11] = 0x20; // archive
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&3u16.to_le_bytes()); // start cluster
        e[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }

    // Entry 2: TEST.COM (tiny .COM program that exits with code 0x42)
    {
        let e = &mut disk_data[root_offset + 64..root_offset + 96];
        e[0..11].copy_from_slice(b"TEST    COM");
        e[11] = 0x20; // archive
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&4u16.to_le_bytes()); // start cluster
        e[28..32].copy_from_slice(&(TEST_COM_PROGRAM.len() as u32).to_le_bytes());
    }

    // Data area: cluster 2 = sector 11 -> COMMAND.COM content
    let cluster2_offset = 11 * sector_size;
    disk_data[cluster2_offset..cluster2_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);

    // Data area: cluster 3 = sector 12 -> TESTFILE.TXT content
    let cluster3_offset = 12 * sector_size;
    disk_data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);

    // Data area: cluster 4 = sector 13 -> TEST.COM content
    let cluster4_offset = 13 * sector_size;
    disk_data[cluster4_offset..cluster4_offset + TEST_COM_PROGRAM.len()]
        .copy_from_slice(TEST_COM_PROGRAM);

    // Build D88 tracks from flat sector data
    let mut tracks: Vec<Option<Vec<D88Sector>>> = Vec::with_capacity(total_tracks);
    for track_idx in 0..total_tracks {
        let cylinder = (track_idx / heads) as u8;
        let head = (track_idx % heads) as u8;
        let mut sectors = Vec::with_capacity(spt);
        for s in 0..spt {
            let lba = track_idx * spt + s;
            let data_offset = lba * sector_size;
            sectors.push(D88Sector {
                cylinder,
                head,
                record: (s + 1) as u8,
                size_code: 3, // 1024 bytes = 128 << 3
                sector_count: spt as u16,
                mfm_flag: 0x40,
                deleted: 0,
                status: 0,
                reserved: [0; 5],
                data: disk_data[data_offset..data_offset + sector_size].to_vec(),
            });
        }
        tracks.push(Some(sectors));
    }

    let d88 = D88Disk::from_tracks("TEST".to_string(), false, D88MediaType::Disk2HD, tracks);
    device::floppy::FloppyImage::from_d88(d88)
}

/// Creates a test floppy with custom program data at cluster 4.
/// `fcb_name` is the 11-byte FCB name (e.g. `b"TEST    COM"` or `b"TEST    EXE"`).
pub fn create_test_floppy_with_program(
    fcb_name: &[u8; 11],
    program_data: &[u8],
) -> device::floppy::FloppyImage {
    use device::floppy::d88::{D88Disk, D88MediaType, D88Sector};

    let cylinders = 77usize;
    let heads = 2usize;
    let spt = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;
    let total_sectors = cylinders * heads * spt;
    let mut disk_data = vec![0u8; total_sectors * sector_size];

    // Copy the standard floppy layout from create_test_floppy but override
    // cluster 4 (sector 13) with program_data and update the directory entry size.
    // Reuse the same BPB, FAT, and root directory structure.

    // Sector 0: Boot sector with BPB (identical to create_test_floppy)
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

    // FAT1 + FAT2
    let fat1_offset = sector_size;
    disk_data[fat1_offset] = 0xFE;
    disk_data[fat1_offset + 1] = 0xFF;
    disk_data[fat1_offset + 2] = 0xFF;
    disk_data[fat1_offset + 3] = 0xFF;
    disk_data[fat1_offset + 4] = 0xFF;
    disk_data[fat1_offset + 5] = 0xFF;
    disk_data[fat1_offset + 6] = 0xFF;
    disk_data[fat1_offset + 7] = 0x0F;
    let fat2_offset = 3 * sector_size;
    let fat1_end = fat1_offset + 2 * sector_size;
    let fat1_copy: Vec<u8> = disk_data[fat1_offset..fat1_end].to_vec();
    disk_data[fat2_offset..fat2_offset + fat1_copy.len()].copy_from_slice(&fat1_copy);

    // Root directory
    let root_offset = 5 * sector_size;
    {
        let e = &mut disk_data[root_offset..root_offset + 32];
        e[0..11].copy_from_slice(b"COMMAND COM");
        e[11] = 0x20;
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&2u16.to_le_bytes());
        e[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }
    {
        let e = &mut disk_data[root_offset + 32..root_offset + 64];
        e[0..11].copy_from_slice(b"TESTFILETXT");
        e[11] = 0x20;
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&3u16.to_le_bytes());
        e[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }
    {
        let e = &mut disk_data[root_offset + 64..root_offset + 96];
        e[0..11].copy_from_slice(fcb_name);
        e[11] = 0x20;
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&4u16.to_le_bytes());
        e[28..32].copy_from_slice(&(program_data.len() as u32).to_le_bytes());
    }

    let cluster2_offset = 11 * sector_size;
    disk_data[cluster2_offset..cluster2_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);
    let cluster3_offset = 12 * sector_size;
    disk_data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);
    let cluster4_offset = 13 * sector_size;
    disk_data[cluster4_offset..cluster4_offset + program_data.len()].copy_from_slice(program_data);

    let mut tracks: Vec<Option<Vec<D88Sector>>> = Vec::with_capacity(total_tracks);
    for track_idx in 0..total_tracks {
        let cylinder = (track_idx / heads) as u8;
        let head = (track_idx % heads) as u8;
        let mut sectors = Vec::with_capacity(spt);
        for s in 0..spt {
            let lba = track_idx * spt + s;
            let data_offset = lba * sector_size;
            sectors.push(D88Sector {
                cylinder,
                head,
                record: (s + 1) as u8,
                size_code: 3,
                sector_count: spt as u16,
                mfm_flag: 0x40,
                deleted: 0,
                status: 0,
                reserved: [0; 5],
                data: disk_data[data_offset..data_offset + sector_size].to_vec(),
            });
        }
        tracks.push(Some(sectors));
    }

    let d88 = D88Disk::from_tracks("TEST".to_string(), false, D88MediaType::Disk2HD, tracks);
    device::floppy::FloppyImage::from_d88(d88)
}

pub fn create_test_floppy_with_autoexec(autoexec_data: &[u8]) -> device::floppy::FloppyImage {
    use device::floppy::d88::{D88Disk, D88MediaType, D88Sector};

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
    set_fat12_entry(fat, 2, 0x0FFF);
    set_fat12_entry(fat, 3, 0x0FFF);
    set_fat12_entry(fat, 4, 0x0FFF);
    set_fat12_entry(fat, 5, 0x0FFF);

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
        entry[0..11].copy_from_slice(b"TEST    COM");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&4u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_COM_PROGRAM.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 96..root_offset + 128];
        entry[0..11].copy_from_slice(b"AUTOEXECBAT");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&5u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(autoexec_data.len() as u32).to_le_bytes());
    }

    let cluster2_offset = 11 * sector_size;
    disk_data[cluster2_offset..cluster2_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);
    let cluster3_offset = 12 * sector_size;
    disk_data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);
    let cluster4_offset = 13 * sector_size;
    disk_data[cluster4_offset..cluster4_offset + TEST_COM_PROGRAM.len()]
        .copy_from_slice(TEST_COM_PROGRAM);
    let cluster5_offset = 14 * sector_size;
    disk_data[cluster5_offset..cluster5_offset + autoexec_data.len()]
        .copy_from_slice(autoexec_data);

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

    let d88 = D88Disk::from_tracks("AUTOEXEC".to_string(), false, D88MediaType::Disk2HD, tracks);
    device::floppy::FloppyImage::from_d88(d88)
}

pub fn create_test_floppy_with_config_and_autoexec(
    config_data: &[u8],
    autoexec_data: &[u8],
) -> device::floppy::FloppyImage {
    use device::floppy::d88::{D88Disk, D88MediaType, D88Sector};

    let cylinders = 77usize;
    let heads = 2usize;
    let sectors_per_track = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;
    let total_sectors = cylinders * heads * sectors_per_track;
    let mut disk_data = vec![0u8; total_sectors * sector_size];

    {
        let boot_sector = &mut disk_data[0..sector_size];
        boot_sector[0] = 0xEB;
        boot_sector[1] = 0x3C;
        boot_sector[2] = 0x90;
        boot_sector[3..11].copy_from_slice(b"NEETAN  ");
        boot_sector[11..13].copy_from_slice(&1024u16.to_le_bytes());
        boot_sector[13] = 1;
        boot_sector[14..16].copy_from_slice(&1u16.to_le_bytes());
        boot_sector[16] = 2;
        boot_sector[17..19].copy_from_slice(&192u16.to_le_bytes());
        boot_sector[19..21].copy_from_slice(&1232u16.to_le_bytes());
        boot_sector[21] = 0xFE;
        boot_sector[22..24].copy_from_slice(&2u16.to_le_bytes());
        boot_sector[24..26].copy_from_slice(&8u16.to_le_bytes());
        boot_sector[26..28].copy_from_slice(&2u16.to_le_bytes());
    }

    let fat1_offset = sector_size;
    let fat = &mut disk_data[fat1_offset..fat1_offset + 2 * sector_size];
    fat[0] = 0xFE;
    fat[1] = 0xFF;
    fat[2] = 0xFF;
    set_fat12_entry(fat, 2, 0x0FFF);
    set_fat12_entry(fat, 3, 0x0FFF);
    set_fat12_entry(fat, 4, 0x0FFF);
    set_fat12_entry(fat, 5, 0x0FFF);
    set_fat12_entry(fat, 6, 0x0FFF);

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
        entry[0..11].copy_from_slice(b"TEST    COM");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&4u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_COM_PROGRAM.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 96..root_offset + 128];
        entry[0..11].copy_from_slice(b"CONFIG  SYS");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&5u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(config_data.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut disk_data[root_offset + 128..root_offset + 160];
        entry[0..11].copy_from_slice(b"AUTOEXECBAT");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&6u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(autoexec_data.len() as u32).to_le_bytes());
    }

    let cluster2_offset = 11 * sector_size;
    disk_data[cluster2_offset..cluster2_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);
    let cluster3_offset = 12 * sector_size;
    disk_data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);
    let cluster4_offset = 13 * sector_size;
    disk_data[cluster4_offset..cluster4_offset + TEST_COM_PROGRAM.len()]
        .copy_from_slice(TEST_COM_PROGRAM);
    let cluster5_offset = 14 * sector_size;
    disk_data[cluster5_offset..cluster5_offset + config_data.len()].copy_from_slice(config_data);
    let cluster6_offset = 15 * sector_size;
    disk_data[cluster6_offset..cluster6_offset + autoexec_data.len()]
        .copy_from_slice(autoexec_data);

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

    let d88 = D88Disk::from_tracks("CONFIG".to_string(), false, D88MediaType::Disk2HD, tracks);
    device::floppy::FloppyImage::from_d88(d88)
}

/// Boots an HLE machine, then inserts a test floppy as drive A:.
/// The floppy is inserted after boot so the BIOS doesn't try to boot from it.
/// BDA_DISK_EQUIP is set before boot so discover_drives() sees the FDD.
pub fn boot_hle_with_floppy() -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();

    // Set BDA DISK_EQUIP bit 0 (1MB FDD unit 0) before boot so HLE OS
    // creates CDS/DPB entries for drive A:.
    machine.bus.write_byte(0x055C, 0x01);

    // Boot HLE OS (no floppy image yet, so BIOS falls through to HLE activation)
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles",
            HLE_BOOT_MAX_CYCLES
        );
    }

    // Insert the test floppy after boot
    let floppy = create_test_floppy();
    machine.bus.insert_floppy(0, floppy, None);

    machine
}

/// Boots an HLE machine with a custom floppy image as drive A:.
pub fn boot_hle_with_floppy_image(floppy: device::floppy::FloppyImage) -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();
    machine.bus.write_byte(0x055C, 0x01);
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles",
            HLE_BOOT_MAX_CYCLES
        );
    }
    machine.bus.insert_floppy(0, floppy, None);
    machine
}

pub fn create_blank_floppy() -> device::floppy::FloppyImage {
    use device::floppy::d88::{D88Disk, D88MediaType, D88Sector};

    let cylinders = 77usize;
    let heads = 2usize;
    let spt = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;

    let mut tracks: Vec<Option<Vec<D88Sector>>> = Vec::with_capacity(total_tracks);
    for track_idx in 0..total_tracks {
        let cylinder = (track_idx / heads) as u8;
        let head = (track_idx % heads) as u8;
        let mut sectors = Vec::with_capacity(spt);
        for s in 0..spt {
            sectors.push(D88Sector {
                cylinder,
                head,
                record: (s + 1) as u8,
                size_code: 3,
                sector_count: spt as u16,
                mfm_flag: 0x40,
                deleted: 0,
                status: 0,
                reserved: [0; 5],
                data: vec![0u8; sector_size],
            });
        }
        tracks.push(Some(sectors));
    }

    let d88 = D88Disk::from_tracks("BLANK".to_string(), false, D88MediaType::Disk2HD, tracks);
    device::floppy::FloppyImage::from_d88(d88)
}

pub fn create_parsed_empty_d88_floppy() -> device::floppy::FloppyImage {
    use device::floppy::d88::{D88Disk, D88MediaType, D88Sector};

    let cylinders = 77usize;
    let heads = 2usize;
    let sectors_per_track = 8usize;
    let sector_size = 1024usize;
    let total_tracks = cylinders * heads;

    let mut tracks: Vec<Option<Vec<D88Sector>>> = Vec::with_capacity(total_tracks);
    for track_index in 0..total_tracks {
        let cylinder = (track_index / heads) as u8;
        let head = (track_index % heads) as u8;
        let mut sectors = Vec::with_capacity(sectors_per_track);
        for record in 1..=sectors_per_track as u8 {
            sectors.push(D88Sector {
                cylinder,
                head,
                record,
                size_code: 3,
                sector_count: sectors_per_track as u16,
                mfm_flag: 0x00,
                deleted: 0x00,
                status: 0x00,
                reserved: [0u8; 5],
                data: vec![0u8; sector_size],
            });
        }
        tracks.push(Some(sectors));
    }

    let d88 = D88Disk::from_tracks(String::new(), false, D88MediaType::Disk2HD, tracks);
    let bytes = d88.to_bytes();
    device::floppy::FloppyImage::from_d88_bytes(&bytes).expect("parse empty D88 floppy")
}

pub fn boot_hle_with_two_floppies() -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();

    // Set BDA DISK_EQUIP bits 0+1 (two 1MB FDD units) before boot.
    machine.bus.write_byte(0x055C, 0x03);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles",
            HLE_BOOT_MAX_CYCLES
        );
    }

    let floppy_a = create_test_floppy();
    let floppy_b = create_blank_floppy();
    machine.bus.insert_floppy(0, floppy_a, None);
    machine.bus.insert_floppy(1, floppy_b, None);

    machine
}

pub fn boot_hle_with_two_floppy_images(
    floppy_a: device::floppy::FloppyImage,
    floppy_b: device::floppy::FloppyImage,
) -> machine::Pc9801Ra {
    let mut machine = create_hle_machine();

    // Set BDA DISK_EQUIP bits 0+1 (two 1MB FDD units) before boot.
    machine.bus.write_byte(0x055C, 0x03);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles",
            HLE_BOOT_MAX_CYCLES
        );
    }

    machine.bus.insert_floppy(0, floppy_a, None);
    machine.bus.insert_floppy(1, floppy_b, None);

    machine
}

pub fn write_bytes(bus: &mut impl Bus, addr: u32, data: &[u8]) {
    for (i, &byte) in data.iter().enumerate() {
        bus.write_byte(addr + i as u32, byte);
    }
}

pub fn inject_and_run(machine: &mut machine::Pc9801Ra, code: &[u8]) {
    inject_and_run_with_budget(machine, code, INJECT_BUDGET);
}

pub fn inject_and_run_with_budget(machine: &mut machine::Pc9801Ra, code: &[u8], budget: u64) {
    write_bytes(&mut machine.bus, INJECT_CODE_BASE, code);

    let mut state = cpu::I386State {
        ip: 0x0000,
        ..Default::default()
    };
    state.set_cs(INJECT_CODE_SEGMENT);
    state.set_ss(INJECT_CODE_SEGMENT);
    state.set_ds(INJECT_CODE_SEGMENT);
    state.set_es(INJECT_CODE_SEGMENT);
    state.set_esp(0xFFFE);
    // Enable interrupts (IF flag) so DOS INT handlers work.
    state.set_eflags(state.eflags() | 0x0200);
    machine.cpu.load_state(&state);

    machine.run_for(budget);
}

/// Runs code via the INT 28h DOS idle hook.
///
/// COMMAND.COM's loop calls INT 28h on each iteration. This function hooks
/// INT 28h to run the given test code once, then restores the original
/// vector and IRETs. INT 21h functions with AH >= 0Ch (all file I/O) are
/// safe to call from within an INT 28h handler.
///
/// Layout at INJECT_CODE_BASE:
///   +0x0000: INT 28h hook stub (saves old vector, runs user code, restores, IRET)
///   +0x0080: user code (the actual test code)
///   +0x0100: result area
///   +0x0200: data area (filenames, buffers)
pub fn inject_and_run_via_int28(machine: &mut machine::Pc9801Ra, code: &[u8], budget: u64) {
    let base = INJECT_CODE_BASE;
    let seg_lo = (INJECT_CODE_SEGMENT & 0xFF) as u8;
    let seg_hi = (INJECT_CODE_SEGMENT >> 8) as u8;

    // Save old INT 28h vector (at IVT 0x00A0).
    let old_int28_off = read_word(&machine.bus, 0x00A0);
    let old_int28_seg = read_word(&machine.bus, 0x00A2);

    // Write user code at +0x0080.
    write_bytes(&mut machine.bus, base + 0x0080, code);

    let old_off_lo = (old_int28_off & 0xFF) as u8;
    let old_off_hi = (old_int28_off >> 8) as u8;
    let old_seg_lo = (old_int28_seg & 0xFF) as u8;
    let old_seg_hi = (old_int28_seg >> 8) as u8;
    // CALL rel16: target=0x0080, CALL at +0x09 (3 bytes), IP after=0x0C, rel=0x0080-0x0C=0x0074.
    #[rustfmt::skip]
    let stub: Vec<u8> = vec![
        0x1E,                               // PUSH DS                  ; +0x00
        0x06,                               // PUSH ES                  ; +0x01
        0xB8, seg_lo, seg_hi,               // MOV AX, seg              ; +0x02
        0x8E, 0xD8,                         // MOV DS, AX               ; +0x05
        0x8E, 0xC0,                         // MOV ES, AX               ; +0x07
        0xE8, 0x74, 0x00,                   // CALL 0080h               ; +0x09
        // After user code returns:
        0x07,                               // POP ES                   ; +0x0C
        0x1F,                               // POP DS                   ; +0x0D
        // Restore old INT 28h vector (write to IVT at 0000:00A0)
        0x50,                               // PUSH AX                  ; +0x0E
        0x53,                               // PUSH BX                  ; +0x0F
        0x1E,                               // PUSH DS                  ; +0x10
        0x31, 0xDB,                         // XOR BX, BX               ; +0x11
        0x8E, 0xDB,                         // MOV DS, BX               ; +0x13
        0xBB, 0xA0, 0x00,                   // MOV BX, 00A0h            ; +0x15
        0xC7, 0x07, old_off_lo, old_off_hi, // MOV [BX], old_offset    ; +0x18
        0xC7, 0x47, 0x02, old_seg_lo, old_seg_hi, // MOV [BX+2], old_segment ; +0x1C
        0x1F,                               // POP DS                   ; +0x21
        0x5B,                               // POP BX                   ; +0x22
        0x58,                               // POP AX                   ; +0x23
        0xCF,                               // IRET                     ; +0x24
    ];
    write_bytes(&mut machine.bus, base, &stub);

    // Set INT 28h vector to point to our stub.
    machine.bus.write_byte(0x00A0, 0x00); // offset low
    machine.bus.write_byte(0x00A1, 0x00); // offset high
    machine.bus.write_byte(0x00A2, seg_lo);
    machine.bus.write_byte(0x00A3, seg_hi);

    // Resume the machine. COMMAND.COM's loop calls INT 28h on each iteration,
    // which runs our stub, which runs the user code, restores INT 28h, and IRETs.
    machine.run_for(budget);
}

pub fn far_to_linear(segment: u16, offset: u16) -> u32 {
    ((segment as u32) << 4) + offset as u32
}

pub fn read_byte(bus: &machine::Pc9801Bus, addr: u32) -> u8 {
    bus.read_byte_direct(addr)
}

pub fn read_word(bus: &machine::Pc9801Bus, addr: u32) -> u16 {
    let low = bus.read_byte_direct(addr) as u16;
    let high = bus.read_byte_direct(addr + 1) as u16;
    low | (high << 8)
}

pub fn read_far_ptr(bus: &machine::Pc9801Bus, addr: u32) -> (u16, u16) {
    let offset = read_word(bus, addr);
    let segment = read_word(bus, addr + 2);
    (segment, offset)
}

pub fn read_bytes(bus: &machine::Pc9801Bus, addr: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| bus.read_byte_direct(addr + i as u32))
        .collect()
}

pub fn read_string(bus: &machine::Pc9801Bus, addr: u32, max_len: usize) -> Vec<u8> {
    let mut result = Vec::new();
    for i in 0..max_len {
        let byte = bus.read_byte_direct(addr + i as u32);
        if byte == 0 {
            break;
        }
        result.push(byte);
    }
    result
}

pub fn read_device_name(bus: &machine::Pc9801Bus, header_addr: u32) -> String {
    // Device header name field is at offset +0x0A, 8 bytes.
    let name_bytes = read_bytes(bus, header_addr + 0x0A, 8);
    String::from_utf8_lossy(&name_bytes).to_string()
}

pub fn result_byte(bus: &machine::Pc9801Bus, offset: u32) -> u8 {
    bus.read_byte_direct(INJECT_RESULT_BASE + offset)
}

pub fn result_word(bus: &machine::Pc9801Bus, offset: u32) -> u16 {
    read_word(bus, INJECT_RESULT_BASE + offset)
}

pub fn result_dword(bus: &machine::Pc9801Bus, offset: u32) -> u32 {
    let lo = result_word(bus, offset) as u32;
    let hi = result_word(bus, offset + 2) as u32;
    lo | (hi << 16)
}

pub fn get_sysvars_address(machine: &mut machine::Pc9801Ra) -> u32 {
    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x52,                         // MOV AH, 52h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, RES_LO, RES_HI,         // MOV [result+0], BX
        0x8C, 0x06, RES_LO + 2, RES_HI,     // MOV [result+2], ES
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    inject_and_run(machine, code);

    let offset = result_word(&machine.bus, 0);
    let segment = result_word(&machine.bus, 2);
    far_to_linear(segment, offset)
}

pub fn get_psp_segment(machine: &mut machine::Pc9801Ra) -> u16 {
    const RES_LO: u8 = (INJECT_RESULT_OFFSET & 0xFF) as u8;
    const RES_HI: u8 = (INJECT_RESULT_OFFSET >> 8) as u8;
    #[rustfmt::skip]
    let code: &[u8] = &[
        0xB4, 0x62,                         // MOV AH, 62h
        0xCD, 0x21,                         // INT 21h
        0x89, 0x1E, RES_LO, RES_HI,        // MOV [result+0], BX
        0xFA,                               // CLI
        0xF4,                               // HLT
    ];
    inject_and_run(machine, code);

    result_word(&machine.bus, 0)
}

/// Creates free memory by splitting the last MCB (Z block) in the chain.
/// COMMAND.COM owns all remaining memory after boot, so allocation tests
pub fn find_char_in_text_vram(bus: &machine::Pc9801Bus, char_code: u16) -> bool {
    let vram = bus.text_vram();
    for row in 0..25 {
        for col in 0..80 {
            let offset = (row * 80 + col) * 2;
            if offset + 1 >= vram.len() {
                return false;
            }
            let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            if code == char_code {
                return true;
            }
        }
    }
    false
}

pub fn find_string_in_text_vram(bus: &machine::Pc9801Bus, chars: &[u16]) -> bool {
    if chars.is_empty() {
        return true;
    }
    let vram = bus.text_vram();
    let total_chars = 80 * 25;
    for start in 0..total_chars {
        if start + chars.len() > total_chars {
            break;
        }
        let mut matched = true;
        for (i, &expected) in chars.iter().enumerate() {
            let offset = (start + i) * 2;
            if offset + 1 >= vram.len() {
                return false;
            }
            let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            if code != expected {
                matched = false;
                break;
            }
        }
        if matched {
            return true;
        }
    }
    false
}

pub fn find_jis_string_in_text_vram(bus: &machine::Pc9801Bus, chars: &[JisChar]) -> bool {
    if chars.is_empty() {
        return true;
    }

    let vram = bus.text_vram();
    let total_cells = 80 * 25;
    for start in 0..total_cells {
        let mut cell_index = start;
        let mut matched = true;

        for &expected in chars {
            if cell_index >= total_cells {
                matched = false;
                break;
            }

            let offset = cell_index * 2;
            let actual = JisChar::from_vram_bytes(vram[offset], vram[offset + 1]);
            if actual != expected {
                matched = false;
                break;
            }

            cell_index += 1;
            if !expected.is_ank() {
                if cell_index >= total_cells {
                    matched = false;
                    break;
                }

                let offset = cell_index * 2;
                let placeholder = JisChar::from_vram_bytes(vram[offset], vram[offset + 1]);
                if placeholder != JisChar::from_u16(0x0000) {
                    matched = false;
                    break;
                }
                cell_index += 1;
            }
        }

        if matched {
            return true;
        }
    }

    false
}

pub fn text_vram_row_to_string(bus: &machine::Pc9801Bus, row: usize) -> String {
    let vram = bus.text_vram();
    let mut result = String::with_capacity(80);
    for col in 0..80 {
        let offset = (row * 80 + col) * 2;
        if offset + 1 >= vram.len() {
            break;
        }
        let code = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
        if (0x20..=0x7E).contains(&code) {
            result.push(code as u8 as char);
        } else {
            result.push(' ');
        }
    }
    result
}

pub fn find_row_containing(bus: &machine::Pc9801Bus, needle: &str) -> Option<usize> {
    for row in 0..25 {
        let line = text_vram_row_to_string(bus, row);
        if line.contains(needle) {
            return Some(row);
        }
    }
    None
}

/// Creates a minimal in-memory HDD image with a FAT16 partition.
/// The image has a PC-98 partition table at sector 1 and a FAT16 volume at the partition offset.
/// `sector_size`: 256 or 512 bytes.
/// `test_files`: if true, populates COMMAND.COM and TESTFILE.TXT.
pub fn create_test_hdd(sector_size: u16) -> device::disk::HddImage {
    use device::disk::{HddFormat, HddGeometry, HddImage};

    let cylinders: u16 = 20;
    let heads: u8 = 8;
    let sectors_per_track: u8 = 17;
    let total_sectors = cylinders as u32 * heads as u32 * sectors_per_track as u32;
    let ss = sector_size as usize;
    let mut data = vec![0u8; total_sectors as usize * ss];

    // Sector 0: IPL (boot code stub)
    // Just put a JMP and "IPL1" signature
    data[0] = 0xEB;
    data[1] = 0x1E;
    data[4..8].copy_from_slice(b"IPL1");

    // Sector 1: PC-98 partition table
    // One active DOS partition starting at cylinder 1
    let part_offset = ss; // sector 1
    let part = &mut data[part_offset..part_offset + 32];
    part[0] = 0xA0; // mid: DOS (0x20) | bootable (0x80)
    part[1] = 0x91; // sid: FAT16 <32MB (0x11) | active (0x80)
    // IPL CHS: cylinder 1, head 0, sector 0
    part[4] = 0; // IPL sector
    part[5] = 0; // IPL head
    part[6] = 1; // IPL cylinder low
    part[7] = 0; // IPL cylinder high
    // Data start CHS: cylinder 1, head 0, sector 0
    part[8] = 0; // data sector
    part[9] = 0; // data head
    part[10] = 1; // data cylinder low
    part[11] = 0; // data cylinder high
    // End CHS: last cylinder, last head, last sector
    part[12] = sectors_per_track - 1;
    part[13] = heads - 1;
    part[14] = (cylinders - 1) as u8;
    part[15] = ((cylinders - 1) >> 8) as u8;
    part[16..32].copy_from_slice(b"MS-DOS 6.20\x00\x00\x00\x00\x00");

    // Partition starts at LBA = cylinder_1 * heads * spt
    let partition_lba = heads as u32 * sectors_per_track as u32;
    let partition_byte_offset = partition_lba as usize * ss;

    // Sectors per cluster: choose based on sector size
    let sectors_per_cluster: u8 = if sector_size == 256 { 8 } else { 4 };
    let reserved_sectors: u16 = 1;
    let num_fats: u8 = 2;
    let root_entry_count: u16 = 512;
    let root_dir_sectors = (root_entry_count as u32 * 32).div_ceil(sector_size as u32);
    let partition_sectors = total_sectors - partition_lba;
    let sectors_per_fat: u16 = 16;
    let first_data_sector =
        reserved_sectors as u32 + num_fats as u32 * sectors_per_fat as u32 + root_dir_sectors;

    // Boot sector at partition offset
    let bs = &mut data[partition_byte_offset..partition_byte_offset + ss];
    bs[0] = 0xEB;
    bs[1] = 0x3C;
    bs[2] = 0x90;
    bs[3..11].copy_from_slice(b"NEETAN  ");
    bs[11..13].copy_from_slice(&sector_size.to_le_bytes());
    bs[13] = sectors_per_cluster;
    bs[14..16].copy_from_slice(&reserved_sectors.to_le_bytes());
    bs[16] = num_fats;
    bs[17..19].copy_from_slice(&root_entry_count.to_le_bytes());
    // total_sectors_16: use if fits in u16
    if partition_sectors <= 0xFFFF {
        bs[19..21].copy_from_slice(&(partition_sectors as u16).to_le_bytes());
    }
    bs[21] = 0xF8; // media descriptor (HDD)
    bs[22..24].copy_from_slice(&sectors_per_fat.to_le_bytes());
    bs[24..26].copy_from_slice(&(sectors_per_track as u16).to_le_bytes());
    bs[26..28].copy_from_slice(&(heads as u16).to_le_bytes());

    // FAT1 at partition offset + reserved_sectors
    let fat1_byte_offset = partition_byte_offset + reserved_sectors as usize * ss;
    // Media descriptor in FAT: F8 FF FF FF (FAT16)
    data[fat1_byte_offset] = 0xF8;
    data[fat1_byte_offset + 1] = 0xFF;
    data[fat1_byte_offset + 2] = 0xFF;
    data[fat1_byte_offset + 3] = 0xFF;
    // Cluster 2: COMMAND.COM -> end of chain (0xFFFF)
    data[fat1_byte_offset + 4] = 0xFF;
    data[fat1_byte_offset + 5] = 0xFF;
    // Cluster 3: TESTFILE.TXT -> end of chain
    data[fat1_byte_offset + 6] = 0xFF;
    data[fat1_byte_offset + 7] = 0xFF;

    // FAT2: copy of FAT1
    let fat2_byte_offset = fat1_byte_offset + sectors_per_fat as usize * ss;
    let fat1_data: Vec<u8> =
        data[fat1_byte_offset..fat1_byte_offset + sectors_per_fat as usize * ss].to_vec();
    data[fat2_byte_offset..fat2_byte_offset + fat1_data.len()].copy_from_slice(&fat1_data);

    // Root directory
    let root_byte_offset = partition_byte_offset
        + (reserved_sectors as usize + num_fats as usize * sectors_per_fat as usize) * ss;

    // Entry 0: COMMAND.COM
    {
        let e = &mut data[root_byte_offset..root_byte_offset + 32];
        e[0..11].copy_from_slice(b"COMMAND COM");
        e[11] = 0x20; // archive
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&2u16.to_le_bytes()); // start cluster
        e[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }

    // Entry 1: TESTFILE.TXT
    {
        let e = &mut data[root_byte_offset + 32..root_byte_offset + 64];
        e[0..11].copy_from_slice(b"TESTFILETXT");
        e[11] = 0x20; // archive
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&3u16.to_le_bytes()); // start cluster
        e[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }

    // Data area: cluster 2 -> COMMAND.COM
    let data_byte_offset = partition_byte_offset + first_data_sector as usize * ss;
    data[data_byte_offset..data_byte_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);

    // Data area: cluster 3 -> TESTFILE.TXT
    let cluster3_offset = data_byte_offset + sectors_per_cluster as usize * ss;
    data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);

    let geometry = HddGeometry {
        cylinders,
        heads,
        sectors_per_track,
        sector_size,
    };
    HddImage::from_raw(geometry, HddFormat::Nhd, data)
}

pub fn create_test_hdd_with_autoexec(
    sector_size: u16,
    autoexec_data: &[u8],
) -> device::disk::HddImage {
    use device::disk::{HddFormat, HddGeometry, HddImage};

    let cylinders: u16 = 20;
    let heads: u8 = 8;
    let sectors_per_track: u8 = 17;
    let total_sectors = cylinders as u32 * heads as u32 * sectors_per_track as u32;
    let sector_size_usize = sector_size as usize;
    let mut data = vec![0u8; total_sectors as usize * sector_size_usize];

    data[0] = 0xEB;
    data[1] = 0x1E;
    data[4..8].copy_from_slice(b"IPL1");

    let part_offset = sector_size_usize;
    let part = &mut data[part_offset..part_offset + 32];
    part[0] = 0xA0;
    part[1] = 0x91;
    part[4] = 0;
    part[5] = 0;
    part[6] = 1;
    part[7] = 0;
    part[8] = 0;
    part[9] = 0;
    part[10] = 1;
    part[11] = 0;
    part[12] = sectors_per_track - 1;
    part[13] = heads - 1;
    part[14] = (cylinders - 1) as u8;
    part[15] = ((cylinders - 1) >> 8) as u8;
    part[16..32].copy_from_slice(b"MS-DOS 6.20\x00\x00\x00\x00\x00");

    let partition_lba = heads as u32 * sectors_per_track as u32;
    let partition_byte_offset = partition_lba as usize * sector_size_usize;
    let sectors_per_cluster: u8 = if sector_size == 256 { 8 } else { 4 };
    let reserved_sectors: u16 = 1;
    let num_fats: u8 = 2;
    let root_entry_count: u16 = 512;
    let root_dir_sectors = (root_entry_count as u32 * 32).div_ceil(sector_size as u32);
    let partition_sectors = total_sectors - partition_lba;
    let sectors_per_fat: u16 = 16;
    let first_data_sector =
        reserved_sectors as u32 + num_fats as u32 * sectors_per_fat as u32 + root_dir_sectors;

    let boot_sector = &mut data[partition_byte_offset..partition_byte_offset + sector_size_usize];
    boot_sector[0] = 0xEB;
    boot_sector[1] = 0x3C;
    boot_sector[2] = 0x90;
    boot_sector[3..11].copy_from_slice(b"NEETAN  ");
    boot_sector[11..13].copy_from_slice(&sector_size.to_le_bytes());
    boot_sector[13] = sectors_per_cluster;
    boot_sector[14..16].copy_from_slice(&reserved_sectors.to_le_bytes());
    boot_sector[16] = num_fats;
    boot_sector[17..19].copy_from_slice(&root_entry_count.to_le_bytes());
    if partition_sectors <= 0xFFFF {
        boot_sector[19..21].copy_from_slice(&(partition_sectors as u16).to_le_bytes());
    }
    boot_sector[21] = 0xF8;
    boot_sector[22..24].copy_from_slice(&sectors_per_fat.to_le_bytes());
    boot_sector[24..26].copy_from_slice(&(sectors_per_track as u16).to_le_bytes());
    boot_sector[26..28].copy_from_slice(&(heads as u16).to_le_bytes());

    let fat1_byte_offset = partition_byte_offset + reserved_sectors as usize * sector_size_usize;
    data[fat1_byte_offset] = 0xF8;
    data[fat1_byte_offset + 1] = 0xFF;
    data[fat1_byte_offset + 2] = 0xFF;
    data[fat1_byte_offset + 3] = 0xFF;
    data[fat1_byte_offset + 4] = 0xFF;
    data[fat1_byte_offset + 5] = 0xFF;
    data[fat1_byte_offset + 6] = 0xFF;
    data[fat1_byte_offset + 7] = 0xFF;
    data[fat1_byte_offset + 8] = 0xFF;
    data[fat1_byte_offset + 9] = 0xFF;

    let fat2_byte_offset = fat1_byte_offset + sectors_per_fat as usize * sector_size_usize;
    let fat1_data = data
        [fat1_byte_offset..fat1_byte_offset + sectors_per_fat as usize * sector_size_usize]
        .to_vec();
    data[fat2_byte_offset..fat2_byte_offset + fat1_data.len()].copy_from_slice(&fat1_data);

    let root_byte_offset = partition_byte_offset
        + (reserved_sectors as usize + num_fats as usize * sectors_per_fat as usize)
            * sector_size_usize;
    {
        let entry = &mut data[root_byte_offset..root_byte_offset + 32];
        entry[0..11].copy_from_slice(b"COMMAND COM");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&2u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut data[root_byte_offset + 32..root_byte_offset + 64];
        entry[0..11].copy_from_slice(b"TESTFILETXT");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&3u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }
    {
        let entry = &mut data[root_byte_offset + 64..root_byte_offset + 96];
        entry[0..11].copy_from_slice(b"AUTOEXECBAT");
        entry[11] = 0x20;
        entry[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        entry[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        entry[26..28].copy_from_slice(&4u16.to_le_bytes());
        entry[28..32].copy_from_slice(&(autoexec_data.len() as u32).to_le_bytes());
    }

    let data_byte_offset = partition_byte_offset + first_data_sector as usize * sector_size_usize;
    data[data_byte_offset..data_byte_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);
    let cluster3_offset = data_byte_offset + sectors_per_cluster as usize * sector_size_usize;
    data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);
    let cluster4_offset = data_byte_offset + 2 * sectors_per_cluster as usize * sector_size_usize;
    data[cluster4_offset..cluster4_offset + autoexec_data.len()].copy_from_slice(autoexec_data);

    let geometry = HddGeometry {
        cylinders,
        heads,
        sectors_per_track,
        sector_size,
    };
    HddImage::from_raw(geometry, HddFormat::Nhd, data)
}

/// Creates an HDD image where the physical sector size (256) differs from the
/// BPB logical sector size (1024). This is common on real PC-98 SASI drives.
/// The FAT volume uses 1024-byte logical sectors laid out across 256-byte physical sectors.
pub fn create_test_hdd_mismatched_sectors() -> device::disk::HddImage {
    use device::disk::{HddFormat, HddGeometry, HddImage};

    let physical_sector_size: u16 = 256;
    let logical_sector_size: u16 = 1024;
    let ratio = (logical_sector_size / physical_sector_size) as usize;

    let cylinders: u16 = 20;
    let heads: u8 = 8;
    let sectors_per_track: u8 = 17;
    let total_sectors = cylinders as u32 * heads as u32 * sectors_per_track as u32;
    let ps = physical_sector_size as usize;
    let ls = logical_sector_size as usize;
    let mut data = vec![0u8; total_sectors as usize * ps];

    // Sector 0: IPL
    data[0] = 0xEB;
    data[1] = 0x1E;
    data[4..8].copy_from_slice(b"IPL1");

    // Sector 1: PC-98 partition table
    let part_offset = ps;
    let part = &mut data[part_offset..part_offset + 32];
    part[0] = 0xA0;
    part[1] = 0x91;
    part[4] = 0;
    part[5] = 0;
    part[6] = 1;
    part[7] = 0;
    part[8] = 0;
    part[9] = 0;
    part[10] = 1;
    part[11] = 0;
    part[12] = sectors_per_track - 1;
    part[13] = heads - 1;
    part[14] = (cylinders - 1) as u8;
    part[15] = ((cylinders - 1) >> 8) as u8;
    part[16..32].copy_from_slice(b"MS-DOS 6.20\x00\x00\x00\x00\x00");

    // Partition starts at cylinder 1 in physical sectors.
    let partition_phys_lba = heads as u32 * sectors_per_track as u32;
    let partition_byte_offset = partition_phys_lba as usize * ps;

    // BPB parameters (all in 1024-byte logical sectors).
    let sectors_per_cluster: u8 = 4;
    let reserved_sectors: u16 = 1;
    let num_fats: u8 = 2;
    let root_entry_count: u16 = 192;
    let sectors_per_fat: u16 = 2;
    let partition_logical_sectors = (total_sectors - partition_phys_lba) / ratio as u32;

    let root_dir_logical_sectors = (root_entry_count as u32 * 32).div_ceil(ls as u32);
    let first_data_logical = reserved_sectors as u32
        + num_fats as u32 * sectors_per_fat as u32
        + root_dir_logical_sectors;

    // Boot sector at partition byte offset.
    let bs = &mut data[partition_byte_offset..partition_byte_offset + ps];
    bs[0] = 0xEB;
    bs[1] = 0x3C;
    bs[2] = 0x90;
    bs[3..11].copy_from_slice(b"NEETAN  ");
    bs[11..13].copy_from_slice(&logical_sector_size.to_le_bytes());
    bs[13] = sectors_per_cluster;
    bs[14..16].copy_from_slice(&reserved_sectors.to_le_bytes());
    bs[16] = num_fats;
    bs[17..19].copy_from_slice(&root_entry_count.to_le_bytes());
    if partition_logical_sectors <= 0xFFFF {
        bs[19..21].copy_from_slice(&(partition_logical_sectors as u16).to_le_bytes());
    }
    bs[21] = 0xF8;
    bs[22..24].copy_from_slice(&sectors_per_fat.to_le_bytes());
    bs[24..26].copy_from_slice(&(sectors_per_track as u16).to_le_bytes());
    bs[26..28].copy_from_slice(&(heads as u16).to_le_bytes());

    // FAT1 at logical sector 1 = byte offset partition + 1*ls.
    let fat1_byte_offset = partition_byte_offset + reserved_sectors as usize * ls;
    data[fat1_byte_offset] = 0xF8;
    data[fat1_byte_offset + 1] = 0xFF;
    data[fat1_byte_offset + 2] = 0xFF;
    // Clusters 2, 3, 4: end-of-chain (FAT12: 0xFFF packed).
    data[fat1_byte_offset + 3] = 0xFF;
    data[fat1_byte_offset + 4] = 0xFF;
    data[fat1_byte_offset + 5] = 0xFF;
    data[fat1_byte_offset + 6] = 0xFF;
    data[fat1_byte_offset + 7] = 0x0F;

    // FAT2: copy of FAT1.
    let fat2_byte_offset = fat1_byte_offset + sectors_per_fat as usize * ls;
    let fat1_data: Vec<u8> =
        data[fat1_byte_offset..fat1_byte_offset + sectors_per_fat as usize * ls].to_vec();
    data[fat2_byte_offset..fat2_byte_offset + fat1_data.len()].copy_from_slice(&fat1_data);

    // Root directory at logical sector (reserved + num_fats * spf).
    let root_logical = reserved_sectors as usize + num_fats as usize * sectors_per_fat as usize;
    let root_byte_offset = partition_byte_offset + root_logical * ls;

    {
        let e = &mut data[root_byte_offset..root_byte_offset + 32];
        e[0..11].copy_from_slice(b"COMMAND COM");
        e[11] = 0x20;
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&2u16.to_le_bytes());
        e[28..32].copy_from_slice(&(TEST_COMMAND_COM.len() as u32).to_le_bytes());
    }
    {
        let e = &mut data[root_byte_offset + 32..root_byte_offset + 64];
        e[0..11].copy_from_slice(b"TESTFILETXT");
        e[11] = 0x20;
        e[22..24].copy_from_slice(&TEST_FILE_TIME.to_le_bytes());
        e[24..26].copy_from_slice(&TEST_FILE_DATE.to_le_bytes());
        e[26..28].copy_from_slice(&3u16.to_le_bytes());
        e[28..32].copy_from_slice(&(TEST_FILE_CONTENT.len() as u32).to_le_bytes());
    }

    // Data area: cluster 2 at logical sector first_data_logical.
    let data_byte_offset = partition_byte_offset + first_data_logical as usize * ls;
    data[data_byte_offset..data_byte_offset + TEST_COMMAND_COM.len()]
        .copy_from_slice(TEST_COMMAND_COM);

    // Cluster 3.
    let cluster3_offset = data_byte_offset + sectors_per_cluster as usize * ls;
    data[cluster3_offset..cluster3_offset + TEST_FILE_CONTENT.len()]
        .copy_from_slice(TEST_FILE_CONTENT);

    let geometry = HddGeometry {
        cylinders,
        heads,
        sectors_per_track,
        sector_size: physical_sector_size,
    };
    HddImage::from_raw(geometry, HddFormat::Nhd, data)
}

/// Boots an HLE machine (PC-9801RA / SASI) with an HDD that has mismatched
/// physical (256) and BPB logical (1024) sector sizes.
pub fn boot_hle_with_sasi_hdd_mismatched_sectors() -> machine::Pc9801Ra {
    let mut machine = machine::Pc9801Ra::new(
        cpu::I386::new(),
        machine::Pc9801Bus::new(MachineModel::PC9801RA, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    machine.bus.write_byte(0x055C, 0x00);
    machine.bus.write_byte(0x055D, 0x01);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles (SASI mismatched)",
            HLE_BOOT_MAX_CYCLES
        );
    }

    let hdd = create_test_hdd_mismatched_sectors();
    machine.bus.insert_hdd(0, hdd, None);

    machine
}

/// Boots an HLE machine (PC-9801RA / SASI) with a test HDD as the first drive.
pub fn boot_hle_with_sasi_hdd(sector_size: u16) -> machine::Pc9801Ra {
    let mut machine = machine::Pc9801Ra::new(
        cpu::I386::new(),
        machine::Pc9801Bus::new(MachineModel::PC9801RA, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    // Set BDA DISK_EQUIP bit 8 (HDD unit 0)
    machine.bus.write_byte(0x055C, 0x00);
    machine.bus.write_byte(0x055D, 0x01);

    // Boot HLE (no disk yet, falls through to HLE activation)
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles (SASI)",
            HLE_BOOT_MAX_CYCLES
        );
    }

    // Insert HDD after boot
    let hdd = create_test_hdd(sector_size);
    machine.bus.insert_hdd(0, hdd, None);

    machine
}

/// Boots an HLE machine (PC-9821AP / IDE) with a test HDD as the first drive.
pub fn boot_hle_with_ide_hdd(sector_size: u16) -> machine::Pc9821Ap {
    let mut machine = machine::Pc9821Ap::new(
        cpu::I386::<{ cpu::CPU_MODEL_486 }>::new(),
        machine::Pc9801Bus::new(MachineModel::PC9821AP, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    // Set BDA DISK_EQUIP bit 8 (HDD unit 0)
    machine.bus.write_byte(0x055C, 0x00);
    machine.bus.write_byte(0x055D, 0x01);

    // Boot HLE
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles (IDE)",
            HLE_BOOT_MAX_CYCLES
        );
    }

    // Insert HDD after boot
    let hdd = create_test_hdd(sector_size);
    machine.bus.insert_hdd(0, hdd, None);

    machine
}

/// Creates an empty (all-zeros) HDD image suitable for testing FORMAT.
pub fn create_empty_hdd(sector_size: u16) -> device::disk::HddImage {
    use device::disk::{HddFormat, HddGeometry, HddImage};

    let cylinders: u16 = 20;
    let heads: u8 = 8;
    let sectors_per_track: u8 = 17;
    let total_sectors = cylinders as u32 * heads as u32 * sectors_per_track as u32;
    let data = vec![0u8; total_sectors as usize * sector_size as usize];

    let geometry = HddGeometry {
        cylinders,
        heads,
        sectors_per_track,
        sector_size,
    };
    HddImage::from_raw(geometry, HddFormat::Nhd, data)
}

/// Boots an HLE machine (PC-9801RA / SASI) with an empty HDD for format testing.
pub fn boot_hle_with_empty_sasi_hdd() -> machine::Pc9801Ra {
    let mut machine = machine::Pc9801Ra::new(
        cpu::I386::new(),
        machine::Pc9801Bus::new(MachineModel::PC9801RA, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    // Set BDA DISK_EQUIP bit 8 (HDD unit 0)
    machine.bus.write_byte(0x055C, 0x00);
    machine.bus.write_byte(0x055D, 0x01);

    let hdd = create_empty_hdd(256);
    machine.bus.insert_hdd(0, hdd, None);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles (empty SASI)",
            HLE_BOOT_MAX_CYCLES
        );
    }

    machine
}

/// Boots an HLE machine (PC-9821AP / IDE) with an empty HDD for format testing.
pub fn boot_hle_with_empty_ide_hdd() -> machine::Pc9821Ap {
    let mut machine = machine::Pc9821Ap::new(
        cpu::I386::<{ cpu::CPU_MODEL_486 }>::new(),
        machine::Pc9801Bus::new(MachineModel::PC9821AP, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    // Set BDA DISK_EQUIP bit 8 (HDD unit 0)
    machine.bus.write_byte(0x055C, 0x00);
    machine.bus.write_byte(0x055D, 0x01);

    let hdd = create_empty_hdd(512);
    machine.bus.insert_hdd(0, hdd, None);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles (empty IDE)",
            HLE_BOOT_MAX_CYCLES
        );
    }

    machine
}

/// Creates a minimal CD-ROM disc image with one data track and one audio track.
/// Uses raw (2352-byte) sectors throughout, as is standard for single-file BIN images.
pub fn create_test_cdimage() -> device::cdrom::CdImage {
    let cue = r#"FILE "test.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 00:02:00
"#;
    const ROOT_DIR_LBA: u32 = 20;
    const README_LBA: u32 = 21;
    const INSTALL_LBA: u32 = 22;

    fn write_both_endian_u16(dst: &mut [u8], value: u16) {
        dst[..2].copy_from_slice(&value.to_le_bytes());
        dst[2..4].copy_from_slice(&value.to_be_bytes());
    }

    fn write_both_endian_u32(dst: &mut [u8], value: u32) {
        dst[..4].copy_from_slice(&value.to_le_bytes());
        dst[4..8].copy_from_slice(&value.to_be_bytes());
    }

    fn recording_time() -> [u8; 7] {
        [95, 1, 1, 12, 0, 0, 0]
    }

    fn write_directory_record(
        buffer: &mut [u8],
        offset: &mut usize,
        identifier: &[u8],
        extent_lba: u32,
        data_length: u32,
        is_directory: bool,
    ) {
        let padding = usize::from(identifier.len().is_multiple_of(2));
        let length = 33 + identifier.len() + padding;
        let record = &mut buffer[*offset..*offset + length];
        record.fill(0);
        record[0] = length as u8;
        write_both_endian_u32(&mut record[2..10], extent_lba);
        write_both_endian_u32(&mut record[10..18], data_length);
        record[18..25].copy_from_slice(&recording_time());
        record[25] = if is_directory { 0x02 } else { 0x00 };
        write_both_endian_u16(&mut record[28..32], 1);
        record[32] = identifier.len() as u8;
        record[33..33 + identifier.len()].copy_from_slice(identifier);
        *offset += length;
    }

    fn make_raw_sector(user_data: &[u8; 2048]) -> Vec<u8> {
        let mut sector = vec![0u8; 2352];
        sector[0] = 0x00;
        for byte in &mut sector[1..11] {
            *byte = 0xFF;
        }
        sector[11] = 0x00;
        sector[15] = 0x01;
        sector[16..16 + 2048].copy_from_slice(user_data);
        sector
    }

    // Track 1: 150 raw data sectors (2352 bytes each, with sync+header+user data).
    let mut bin_data = Vec::with_capacity(2352 * 200);
    for sector_index in 0..150u32 {
        let mut user_data = [0u8; 2048];
        match sector_index {
            16 => {
                user_data[0] = 1;
                user_data[1..6].copy_from_slice(b"CD001");
                user_data[6] = 1;
                user_data[8..40].copy_from_slice(b"NEETAN TEST CD                  ");
                user_data[40..72].copy_from_slice(b"NEETAN_CD                       ");
                write_both_endian_u32(&mut user_data[80..88], 150);
                write_both_endian_u16(&mut user_data[120..124], 1);
                write_both_endian_u16(&mut user_data[124..128], 1);
                write_both_endian_u16(&mut user_data[128..132], 2048);
                write_both_endian_u32(&mut user_data[132..140], 10);
                let mut root_record_offset = 156usize;
                write_directory_record(
                    &mut user_data,
                    &mut root_record_offset,
                    &[0],
                    ROOT_DIR_LBA,
                    2048,
                    true,
                );
                let copyright = b"COPYRIGHT.TXT;1";
                user_data[702..702 + copyright.len()].copy_from_slice(copyright);
                for byte in &mut user_data[702 + copyright.len()..702 + 37] {
                    *byte = b' ';
                }
                let abstract_id = b"ABSTRACT.TXT;1";
                user_data[739..739 + abstract_id.len()].copy_from_slice(abstract_id);
                for byte in &mut user_data[739 + abstract_id.len()..739 + 37] {
                    *byte = b' ';
                }
                let biblio = b"BIBLIO.TXT;1";
                user_data[776..776 + biblio.len()].copy_from_slice(biblio);
                for byte in &mut user_data[776 + biblio.len()..776 + 37] {
                    *byte = b' ';
                }
            }
            17 => {
                user_data[0] = 0xFF;
                user_data[1..6].copy_from_slice(b"CD001");
                user_data[6] = 1;
            }
            ROOT_DIR_LBA => {
                let mut offset = 0usize;
                write_directory_record(&mut user_data, &mut offset, &[0], ROOT_DIR_LBA, 2048, true);
                write_directory_record(&mut user_data, &mut offset, &[1], ROOT_DIR_LBA, 2048, true);
                write_directory_record(
                    &mut user_data,
                    &mut offset,
                    b"README.TXT;1",
                    README_LBA,
                    TEST_CDROM_README.len() as u32,
                    false,
                );
                write_directory_record(
                    &mut user_data,
                    &mut offset,
                    b"INSTALL.EXE;1",
                    INSTALL_LBA,
                    4,
                    false,
                );
            }
            README_LBA => {
                user_data[..TEST_CDROM_README.len()].copy_from_slice(TEST_CDROM_README);
            }
            INSTALL_LBA => {
                user_data[..4].copy_from_slice(b"MZ\x90\x00");
            }
            _ => {
                user_data.fill(0x11);
            }
        }
        bin_data.extend_from_slice(&make_raw_sector(&user_data));
    }
    // Track 2: 50 audio sectors (2352 bytes each).
    bin_data.extend_from_slice(&vec![0xAAu8; 2352 * 50]);
    device::cdrom::CdImage::from_cue(cue, bin_data).unwrap()
}

/// Boots an HLE machine (PC-9821AP / IDE) with a test CD-ROM inserted.
/// The CD-ROM is inserted before boot so MSCDEX activates the Q: drive.
pub fn boot_hle_with_cdrom() -> machine::Pc9821Ap {
    let mut machine = machine::Pc9821Ap::new(
        cpu::I386::<{ cpu::CPU_MODEL_486 }>::new(),
        machine::Pc9801Bus::new(MachineModel::PC9821AP, 48000),
    );
    machine.bus.load_font_rom(FONT_ROM_DATA);

    // Insert CD-ROM before boot so cdrom_present() is true during boot.
    let cdimage = create_test_cdimage();
    machine.bus.insert_cdrom(cdimage);

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(HLE_BOOT_CHECK_INTERVAL);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < HLE_BOOT_MAX_CYCLES,
            "HLE OS did not show prompt within {} cycles (CDROM)",
            HLE_BOOT_MAX_CYCLES
        );
    }
    machine
}

pub fn inject_and_run_generic_with_budget<const M: u8>(
    machine: &mut machine::Machine<cpu::I386<M>>,
    code: &[u8],
    budget: u64,
) {
    write_bytes(&mut machine.bus, INJECT_CODE_BASE, code);

    let mut state = cpu::I386State {
        ip: 0x0000,
        ..Default::default()
    };
    state.set_cs(INJECT_CODE_SEGMENT);
    state.set_ss(INJECT_CODE_SEGMENT);
    state.set_ds(INJECT_CODE_SEGMENT);
    state.set_es(INJECT_CODE_SEGMENT);
    state.set_esp(0xFFFE);
    state.set_eflags(state.eflags() | 0x0200);
    machine.cpu.load_state(&state);

    machine.run_for(budget);
}

/// Injects ASCII characters into the PC-98 keyboard buffer.
pub fn type_string(bus: &mut machine::Pc9801Bus, text: &[u8]) {
    for &ch in text {
        let count = bus.read_byte_direct(0x0528);
        if count >= 0x10 {
            panic!("keyboard buffer full while injecting text");
        }
        let tail = read_word(bus, 0x0526) as u32;
        bus.write_byte(tail, ch);
        bus.write_byte(tail + 1, 0x00);
        let mut new_tail = tail + 2;
        if new_tail >= 0x0522 {
            new_tail = 0x0502;
        }
        write_word_raw(bus, 0x0526, new_tail as u16);
        bus.write_byte(0x0528, count + 1);
    }
}

pub const SCAN_DELETE: u8 = 0x39;
pub const SCAN_UP: u8 = 0x3A;
pub const SCAN_LEFT: u8 = 0x3B;

/// Injects a special key through the PC-98 keyboard pipeline.
/// Pushes make and break scancodes via the keyboard controller, then runs
/// the machine so the BIOS INT 09h handler processes them into KB_BUF.
pub fn type_special_key(machine: &mut machine::Pc9801Ra, scan_code: u8) {
    machine.bus.push_keyboard_scancode(scan_code); // press down
    machine.bus.push_keyboard_scancode(scan_code | 0x80); // release
    machine.run_for(100_000);
}

fn write_word_raw(bus: &mut machine::Pc9801Bus, addr: u32, value: u16) {
    bus.write_byte(addr, value as u8);
    bus.write_byte(addr + 1, (value >> 8) as u8);
}

/// Types a long string into the keyboard buffer, running the machine between
/// chunks to drain the 16-entry buffer. Use this for command strings longer
/// than ~14 characters.
pub fn type_string_long(machine: &mut machine::Pc9801Ra, text: &[u8]) {
    let chunk_size = 12;
    for chunk in text.chunks(chunk_size) {
        type_string(&mut machine.bus, chunk);
        machine.run_for(5_000_000);
    }
}

/// Runs the machine until the shell prompt (`>`) reappears in text VRAM.
pub fn run_until_prompt(machine: &mut machine::Pc9801Ra) {
    let max_cycles: u64 = 500_000_000;
    let check_interval: u64 = 100_000;
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(check_interval);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < max_cycles,
            "shell did not return to prompt within {max_cycles} cycles"
        );
    }
}

/// Types a long string for PC-9821AP machines.
pub fn type_string_long_ap(machine: &mut machine::Pc9821Ap, text: &[u8]) {
    let chunk_size = 12;
    for chunk in text.chunks(chunk_size) {
        type_string(&mut machine.bus, chunk);
        machine.run_for(5_000_000);
    }
}

/// Runs the PC-9821AP machine until the shell prompt reappears.
pub fn run_until_prompt_ap(machine: &mut machine::Pc9821Ap) {
    let max_cycles: u64 = 500_000_000;
    let check_interval: u64 = 100_000;
    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(check_interval);
        if hle_prompt_visible(&machine.bus) {
            break;
        }
        assert!(
            total_cycles < max_cycles,
            "shell did not return to prompt within {max_cycles} cycles"
        );
    }
}
