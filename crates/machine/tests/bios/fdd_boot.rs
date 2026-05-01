use common::Cpu;
use device::floppy::FloppyImage;

fn make_2hd_128_byte_sector_int1b_boot_disk() -> FloppyImage {
    let mut boot_sector = vec![0u8; 128];
    let code = [
        0xFA, // CLI
        0xB8, 0x60, 0x00, // MOV AX,0060h
        0x8E, 0xC0, // MOV ES,AX
        0xB8, 0x90, 0x16, // MOV AX,1690h
        0xBB, 0x00, 0x02, // MOV BX,0200h
        0xB9, 0x00, 0x00, // MOV CX,0000h
        0xBA, 0x01, 0x00, // MOV DX,0001h
        0x33, 0xED, // XOR BP,BP
        0xCD, 0x1B, // INT 1Bh
        0x72, 0x06, // JC fail
        0xEA, 0x00, 0x01, 0x60, 0x00, // JMP 0060:0100
        0xF4, // fail: HLT
    ];
    boot_sector[..code.len()].copy_from_slice(&code);

    let sector2 = vec![0x22u8; 128];
    let mut sector3 = vec![0u8; 128];
    sector3[0] = 0xFA; // CLI
    sector3[1] = 0xF4; // HLT
    let sector4 = vec![0x44u8; 128];

    let sectors: &[(u8, &[u8])] = &[
        (1, &boot_sector),
        (2, &sector2),
        (3, &sector3),
        (4, &sector4),
    ];
    let tracks = &[(0, 0, sectors)];
    let d88 = super::build_2hd_d88(tracks, false);
    FloppyImage::from_d88_bytes(&d88).expect("2HD 128-byte INT 1Bh boot disk")
}

#[test]
fn hle_bootstrap_pc9801f_2hd_128_byte_boot_sector_can_read_with_da90h() {
    let mut machine = super::create_machine_f();
    machine
        .bus
        .insert_floppy(0, make_2hd_128_byte_sector_int1b_boot_disk(), None);

    const MAX_CYCLES: u64 = 500_000_000;
    const CHECK_INTERVAL: u64 = 1_000_000;

    let mut total_cycles = 0u64;
    loop {
        total_cycles += machine.run_for(CHECK_INTERVAL);
        if machine.cpu.halted() {
            break;
        }
        assert!(
            total_cycles < MAX_CYCLES,
            "Machine did not halt within {MAX_CYCLES} cycles"
        );
    }

    let state = machine.save_state();
    assert_eq!(state.memory.ram[0x0584], 0x90, "2HD boot device");
    assert_eq!(machine.cpu.cs(), 0x0060, "stage-two CS");
    assert_eq!(state.memory.ram[0x00600], 0xFA, "sector 1 copied");
    assert_eq!(state.memory.ram[0x00680], 0x22, "sector 2 copied");
    assert_eq!(state.memory.ram[0x00700], 0xFA, "sector 3 copied");
    assert_eq!(state.memory.ram[0x00701], 0xF4, "stage-two HLT copied");
    assert_eq!(state.memory.ram[0x00780], 0x44, "sector 4 copied");
}
