//! PC-9801 system bus implementing [`common::Bus`].
//!
//! Routes memory accesses to RAM/VRAM/ROM and I/O port accesses to
//! the appropriate peripheral (PIC, PIT, etc.).

mod bios;

use std::path::PathBuf;

use common::{
    CpuType, DisplaySnapshotUpload, EventKind, MachineModel, Scheduler,
    cast_u32_slice_as_bytes_mut, debug, warn,
};
use device::{
    beeper::Beeper,
    cgrom::Cgrom,
    disk::HddImage,
    display_control::DisplayControl,
    egc::Egc,
    floppy::FloppyImage,
    grcg::{self, Grcg},
    i8237_dma::I8237Dma,
    i8251_keyboard::I8251Keyboard,
    i8251_serial::I8251Serial,
    i8253_pit::{I8253Pit, PIT_FLAG_I},
    i8255_mouse_ppi::I8255MousePpi,
    i8255_system_ppi::I8255SystemPpi,
    i8259a_pic::I8259aPic,
    palette::Palette,
    printer::Printer,
    sasi::{SasiAction, SasiController, SasiPhase},
    soundboard_26k::{Soundboard26k, Soundboard26kAction},
    soundboard_86::{Soundboard86, Soundboard86Action},
    upd765a_fdc::{
        FdcAction, FdcCommand, FloppyController, ST0_NOT_READY, ST1_MISSING_ADDRESS_MARK,
        ST1_NOT_WRITABLE,
    },
    upd4990a_rtc::Upd4990aRtc,
    upd7220_gdc::{DOT_CLOCK_200LINE, DOT_CLOCK_400LINE, Gdc, GdcAction, STATUS_DRAWING, VramOp},
    upd52611_crtc::Upd52611Crtc,
};

use crate::{
    config::ClockConfig,
    memory::Pc9801Memory,
    trace::{NoTracing, Tracing},
};

/// Text RAM (0xA0000-0xA3FFF) access wait penalty in CPU cycles.
const TRAM_WAIT_CYCLES: i64 = 1;

/// Graphics VRAM (0xA8000-0xBFFFF) access wait penalty in CPU cycles (display period).
/// During VSYNC blanking, this drops to 1 cycle.
const VRAM_WAIT_CYCLES: i64 = 6;

/// GRCG VRAM access wait penalty in CPU cycles (display period).
/// During VSYNC blanking, this drops to 1 cycle.
/// Used for TCR reads, TDW writes, and RMW writes. RMW reads use VRAM_WAIT_CYCLES instead.
const GRCG_WAIT_CYCLES: i64 = 8;

/// I/O bus access wait penalty in CPU cycles.
/// Each byte-sized I/O read or write incurs this penalty.
const IO_WAIT_CYCLES: i64 = 1;

/// DMA access control register (port 0x0439) default value at boot.
/// Bit 2 set: mask DMA physical addresses to 20 bits (normal mode).
/// Ref: undoc98 `io_dma.txt` (port 0x0439).
const DMA_ACCESS_CTRL_DEFAULT: u8 = 0x04;

/// System status register (port 0xF0 read) default for a minimal VM config.
/// All bits clear = no sound board, no IDE interface installed.
/// Ref: undoc98 `io_cpu.txt` (port 0xF0)
const SYSTEM_STATUS_DEFAULT: u8 = 0x00;

/// Normal/hi-res mode detection register (port 0x0431 read).
/// Bit 2 = 1 means normal mode (640x400/640x200).
/// Hi-res mode (1120x750) is only on PC-H98, PC-98XA/XL/RL, and some PC-9821 models.
/// Ref: undoc98 `io_hires.txt` (port 0x0431)
const MODE_DETECT_NORMAL: u8 = 0x04;

/// 1MB FDC external circuit input register value (port 0x0094 read).
/// Bit 6: FINT0 = 1 (fixed for dual-mode FD I/F).
/// Bit 2: TYP0 = 1 (internal drives are #1, #2, DIP SW 1-4 OFF).
/// Ref: undoc98 `io_fdd.txt`
const FDC_1MB_INPUT_REGISTER: u8 = 0x44;

/// 640KB FDC external circuit input register value (port 0x00CC read).
/// Bit 6: FINT0 = 1 (fixed for dual-mode FD I/F).
/// Bit 5: DMACH = 1 (fixed for 640KB I/F mode).
/// Bit 4: RDY = 1 (drive ready).
/// Bit 2: TYP0 = 1 (internal drives are #1, #2, DIP SW 1-4 OFF).
/// Ref: undoc98 `io_fdd.txt`
const FDC_640K_INPUT_REGISTER: u8 = 0x74;

/// FDC media read mask: bits 0-1 from stored value, upper bits fixed at 1.
const FDC_MEDIA_READ_FIXED_BITS: u8 = 0xF8;

/// Mouse interrupt timer register default (port 0xBFDB).
///
/// Lower 2 bits select periodic interrupt rate: 0x00 = 120 Hz (default).
const MOUSE_TIMER_DEFAULT_SETTING: u8 = 0x00;

/// Mouse timer IRQ line on PC-98 (slave IR5 -> INT 15h).
const MOUSE_TIMER_IRQ_LINE: u8 = 13;

/// Default host local time function: returns advancing BCD time from the system clock.
fn default_local_time() -> [u8; 6] {
    fn to_bcd(value: u8) -> u8 {
        ((value / 10) << 4) | (value % 10)
    }
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = (secs / 86400) as u32;
    let time_of_day = (secs % 86400) as u32;
    let hour = (time_of_day / 3600) as u8;
    let minute = ((time_of_day % 3600) / 60) as u8;
    let second = (time_of_day % 60) as u8;
    // Simple date calculation from days since epoch (1970-01-01, Thursday=4).
    let dow = ((days + 4) % 7) as u8;
    let mut y = 1970u32;
    let mut remaining = days;
    loop {
        let ydays = if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
            366
        } else {
            365
        };
        if remaining < ydays {
            break;
        }
        remaining -= ydays;
        y += 1;
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let mdays = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u8;
    for &md in &mdays {
        if remaining < md {
            break;
        }
        remaining -= md;
        month += 1;
    }
    let day = remaining as u8 + 1;
    // BCD: [year, month<<4|day_of_week, day, hour, minute, second]
    [
        to_bcd((y % 100) as u8),
        (month << 4) | dow,
        to_bcd(day),
        to_bcd(hour),
        to_bcd(minute),
        to_bcd(second),
    ]
}

/// Graphics VRAM bytes per page for the B/R/G planes.
const GRAPHICS_PAGE_SIZE_BYTES: usize = 0x18000;

/// E-plane VRAM bytes per page.
const E_PLANE_PAGE_SIZE_BYTES: usize = 0x8000;

/// PC-9801 system bus.
pub struct Pc9801Bus<T: Tracing = NoTracing> {
    pub(crate) current_cycle: u64,
    pub(crate) next_event_cycle: u64,
    pub(crate) nmi_enabled: bool,
    memory: Pc9801Memory,
    pub(crate) pic: I8259aPic,
    pub(crate) scheduler: Scheduler,
    clocks: ClockConfig,
    pit: I8253Pit,
    dma: I8237Dma,
    keyboard: I8251Keyboard,
    serial: I8251Serial,
    gdc_master: Gdc,
    gdc_slave: Gdc,
    /// PC-98 floppy controller (both FDC interfaces + drive storage).
    floppy: FloppyController,
    system_ppi: I8255SystemPpi,
    printer: Printer,
    display_control: DisplayControl,
    cgrom: Cgrom,
    crtc: Upd52611Crtc,
    grcg: Grcg,
    egc: Egc,
    palette: Palette,
    soundboard_26k: Option<Soundboard26k>,
    soundboard_86: Option<Soundboard86>,
    beeper: Beeper,
    rtc: Upd4990aRtc,
    /// Returns the current host local time as 6-byte BCD:
    /// `[year, month<<4|day_of_week, day, hour, minute, second]`.
    host_local_time_fn: fn() -> [u8; 6],
    mouse_ppi: I8255MousePpi,
    /// Mouse interrupt timer register (port 0xBFDB).
    mouse_timer_setting: u8,
    /// PC-9801-27 SASI hard disk controller.
    sasi: SasiController,
    /// BIOS HLE trap controller.
    bios: device::bios::BiosController,
    a20_enabled: bool,
    machine_model: MachineModel,
    reset_pending: bool,
    /// Set when the guest triggers a SYSTEM SHUTDOWN (SHUT0=1, SHUT1=0 when
    /// port 0xF0 is written). The host application should exit cleanly.
    shutdown_requested: bool,
    /// Set when a cold reset (port 0xF0 write) has occurred. The HLE
    /// VEC_ITF_ENTRY handler checks this to decide whether to reinitialize
    /// all devices to post-BIOS state. Cleared after the HLE handler processes it.
    needs_full_reinit: bool,
    /// Warm-reset context captured at the moment of the port 0xF0 write.
    /// On real hardware the CPU stops immediately; in our emulator the CPU
    /// continues until the machine loop checks, so we snapshot the state.
    warm_reset_context: Option<(u16, u16, u16, u16)>,
    vsync_snapshot: Box<DisplaySnapshotUpload>,
    /// DMA access control register (port 0x0439). Bit 2: mask DMA above 1MB.
    dma_access_ctrl: u8,
    /// VRAM/EMS bank register (write-only via port 0x043F).
    vram_ems_bank: u8,
    /// RAM window register (write-only via port 0x0461).
    ram_window: u8,
    /// 15M hole control register (port 0x043B). Controls F00000-FFFFFF accessibility.
    hole_15m_control: u8,
    /// Protected memory registration register (port 0x0567).
    protected_memory_max: u8,
    /// Whether NEC B-bank EMS (port 0x043F bit 1) is supported.
    /// Switches B0000-BFFFF between graphics VRAM and extended RAM at 0x100000.
    /// Present on RA and later 386+ models.
    /// Ref: undoc98 memsys.txt line 1767, io_mem.txt port 0x043F.
    b_bank_ems: bool,
    /// Whether the 16-color graphics extension (E-plane VRAM) is installed.
    graphics_extension_enabled: bool,
    pending_wait_cycles: i64,
    /// Undocumented RTC control/mode latch (port 0x0022).
    rtc_control_22: u8,
    /// Key-down sense latch (port 0x00EC).
    key_sense_0ec: u8,
    /// Expansion-slot socket processing latch (port 0x043A).
    external_interrupt_43a: u8,
    /// Current text RAM access wait penalty in CPU cycles.
    /// Switched between display-period and VSYNC-blanking values by the
    /// GdcVsync / GdcDisplayStart event handlers.
    tram_wait: i64,
    /// Current graphics VRAM access wait penalty in CPU cycles.
    vram_wait: i64,
    /// Current GRCG VRAM access wait penalty in CPU cycles.
    grcg_wait: i64,
    /// Whether the one-shot post-BIOS timer fixup has been applied.
    ///
    /// On a real PC-98, the BIOS (or the application via INT 1Ch AH=02/03)
    /// unmasks IRQ 0 in the PIC to start the system timer.
    tracer: T,
    /// HLE BIOS: per-drive seek cylinder position.
    fdd_seek_cylinder: [u8; 4],
    /// Cached CR0 from the CPU, set before HLE dispatch.
    hle_cr0: u32,
    /// Cached CR3 from the CPU, set before HLE dispatch.
    hle_cr3: u32,
}

impl<T: Tracing> Pc9801Bus<T> {
    /// Creates a new bus configured for the given machine model.
    pub fn new(machine_model: MachineModel, sample_rate: u32) -> Self {
        let clocks = ClockConfig {
            cpu_clock_hz: machine_model.cpu_clock_hz(),
            pit_clock_hz: machine_model.pit_clock_hz(),
        };
        let is_8mhz_lineage = machine_model.is_8mhz_pit_lineage();

        let mut bus = Self {
            current_cycle: 0,
            next_event_cycle: u64::MAX,
            nmi_enabled: false,
            clocks,
            memory: Pc9801Memory::new(machine_model, machine_model.extended_ram_default_size()),
            pic: I8259aPic::new(),
            scheduler: Scheduler::new(),
            pit: I8253Pit::new(is_8mhz_lineage),
            dma: I8237Dma::new(),
            keyboard: I8251Keyboard::new(),
            serial: I8251Serial::new(),
            gdc_master: Gdc::new_master(clocks.cpu_clock_hz),
            gdc_slave: Gdc::new(),
            floppy: FloppyController::new(),
            system_ppi: I8255SystemPpi::new(is_8mhz_lineage),
            printer: Printer::new(),
            display_control: DisplayControl::new(),
            cgrom: Cgrom::new(),
            crtc: Upd52611Crtc::new(),
            grcg: Grcg::new(machine_model.grcg_chip_version()),
            egc: Egc::new(),
            palette: Palette::new(),
            soundboard_26k: None,
            soundboard_86: None,
            beeper: Beeper::new(sample_rate),
            rtc: Upd4990aRtc::new(),
            host_local_time_fn: default_local_time,
            mouse_ppi: I8255MousePpi::new(),
            mouse_timer_setting: MOUSE_TIMER_DEFAULT_SETTING,
            sasi: SasiController::new(),
            bios: device::bios::BiosController::new(),
            a20_enabled: false,
            machine_model,
            reset_pending: false,
            shutdown_requested: false,
            needs_full_reinit: false,
            warm_reset_context: None,
            vsync_snapshot: Box::new(DisplaySnapshotUpload::default()),
            dma_access_ctrl: DMA_ACCESS_CTRL_DEFAULT,
            vram_ems_bank: 0x00,
            ram_window: 0x08,
            hole_15m_control: 0x00,
            protected_memory_max: 0x00,
            b_bank_ems: machine_model.has_b_bank_ems(),
            graphics_extension_enabled: false,
            pending_wait_cycles: 0,
            rtc_control_22: 0x00,
            key_sense_0ec: 0xFF,
            external_interrupt_43a: 0xFF,
            tram_wait: TRAM_WAIT_CYCLES,
            vram_wait: VRAM_WAIT_CYCLES,
            grcg_wait: GRCG_WAIT_CYCLES,
            tracer: T::default(),
            fdd_seek_cylinder: [0; 4],
            hle_cr0: 0,
            hle_cr3: 0,
        };

        if machine_model.has_cg_ram() {
            bus.set_cg_ram(true);
        }

        // Emulate a machine with the analog 16-color graphics extension installed.
        bus.set_graphics_extension_enabled(true);

        // Set the mouse interpolation clock from the CPU clock.
        bus.mouse_ppi.set_cpu_clock(clocks.cpu_clock_hz);

        // Schedule the first VSYNC event after one display period.
        bus.scheduler
            .schedule(EventKind::GdcVsync, bus.gdc_master.state.display_period);
        bus.system_ppi
            .set_cpu_mode_bit(machine_model == MachineModel::PC9801VM);
        bus.update_next_event_cycle();
        bus.initialize_post_boot_state();
        bus
    }

    /// Populates the IVT at 0x0000–0x03FF from the stub BIOS ROM's vector table.
    ///
    /// The ROM contains a vector initialization table at a known offset within
    /// the BIOS code segment (0xFD80). Each entry is a (vector_number, handler_offset)
    /// pair of 16-bit words, terminated by 0xFFFF. The handler offsets are relative
    /// to segment 0xFD80.
    fn populate_ivt_from_stub_bios(&mut self) {
        const BIOS_CODE_SEG: u16 = 0xFD80;
        const BIOS_CODE_OFFSET: usize = 0x15800;
        // Read the vector table offset from the metadata header at the start of
        // the BIOS code region (+0 = vector table segment offset).
        let vector_table_seg_offset = u16::from_le_bytes([
            self.memory.rom_byte(BIOS_CODE_OFFSET),
            self.memory.rom_byte(BIOS_CODE_OFFSET + 1),
        ]) as usize;

        let mut rom_pos = BIOS_CODE_OFFSET + vector_table_seg_offset;

        loop {
            let vector_num = u16::from_le_bytes([
                self.memory.rom_byte(rom_pos),
                self.memory.rom_byte(rom_pos + 1),
            ]);
            if vector_num == 0xFFFF {
                break;
            }
            let handler_offset = u16::from_le_bytes([
                self.memory.rom_byte(rom_pos + 2),
                self.memory.rom_byte(rom_pos + 3),
            ]);
            let ivt_addr = (vector_num as usize) * 4;
            self.memory.state.ram[ivt_addr] = handler_offset as u8;
            self.memory.state.ram[ivt_addr + 1] = (handler_offset >> 8) as u8;
            self.memory.state.ram[ivt_addr + 2] = BIOS_CODE_SEG as u8;
            self.memory.state.ram[ivt_addr + 3] = (BIOS_CODE_SEG >> 8) as u8;
            rom_pos += 4;
        }
    }

    /// Populates BDA fields based on machine model and clock lineage.
    fn populate_bda(&mut self) {
        let cpu_type = self.machine_model.cpu_type();
        let is_v30 = cpu_type == CpuType::V30;

        // MSW3 from text VRAM memory switch area (stride 4, at offset 0x3FEA).
        let msw3 = self.memory.state.text_vram[0x3FEA];

        // BIOS_FLAG0 (0x0500): 0x03 = base | 1MB FDD.
        self.memory.state.ram[0x0500] = 0x03;

        // BIOS_FLAG1 (0x0501):
        //   bit 7 = 1 if 8MHz lineage
        //   bit 6 = 1 if V30 CPU
        //   bit 5 = 1 (always set)
        //   bits 0-2 = real-mode memory size from MSW3 (128KB units above base 128KB)
        let bios_flag1 = 0x20u8
            | if self.machine_model.is_8mhz_pit_lineage() {
                0x80
            } else {
                0x00
            }
            | if is_v30 { 0x40 } else { 0x00 }
            | (msw3 & 0x07);
        self.memory.state.ram[0x0501] = bios_flag1;

        // BIOS_FLAG2 (0x0400): 386 machines get extended memory + protected mode bits.
        let bios_flag2 = match cpu_type {
            CpuType::I386 => 0x06,
            _ => 0x00,
        };
        self.memory.state.ram[0x0400] = bios_flag2;

        // SYS_TYPE (0x0480): CPU type + hardware detection flags.
        //   bits 0-1: CPU type (V30=0x00, I286=0x01, I386=0x03)
        //   bit 3: dual-use FDD present
        //   bit 6: EGC / protected mode test passed
        let sys_type = match cpu_type {
            CpuType::V30 => 0x00,
            CpuType::I286 => 0x01,
            CpuType::I386 => 0x4B,
        };
        self.memory.state.ram[0x0480] = sys_type;

        // USER_SP / USER_SS (0x0404 / 0x0406): saved stack from the ITF
        // protected mode test on 386+ CPUs.
        if cpu_type >= CpuType::I386 {
            self.memory.state.ram[0x0404] = 0xF8;
            self.memory.state.ram[0x0405] = 0x00;
            self.memory.state.ram[0x0406] = 0x30;
            self.memory.state.ram[0x0407] = 0x00;
        }

        // EXPMMSZ (0x0401): extended memory size in 128KB units.
        let expmmsz = (self.memory.state.extended_ram.len() / 0x20000) as u8;
        self.memory.state.ram[0x0401] = expmmsz;

        // BIOS_FLAG3 (0x0481): 386 machines have bit 5 set.
        self.memory.state.ram[0x0481] = match cpu_type {
            CpuType::I386 => 0x20,
            CpuType::I286 | CpuType::V30 => 0x00,
        };

        // F2HD_MODE (0x0493): all drives 2HD.
        self.memory.state.ram[0x0493] = 0xFF;

        // F2DD_MODE (0x05CA): all drives normal density.
        self.memory.state.ram[0x05CA] = 0xFF;

        // F2DD_POINTER (0x05CC) / F2HD_POINTER (0x05F8): far pointers to format
        // tables in ROM. The offsets differ between BIOS generations (RA vs others).
        let (f2hd_off, f2dd_off): (u16, u16) = match self.machine_model {
            MachineModel::PC9801RA => (0x1AAF, 0x1AD7),
            MachineModel::PC9801VM | MachineModel::PC9801VX => (0x1AB4, 0x1ADC),
        };
        self.memory.state.ram[0x05CC..0x05D0].copy_from_slice(&[
            f2dd_off as u8,
            (f2dd_off >> 8) as u8,
            0x80,
            0xFD,
        ]);
        self.memory.state.ram[0x05F8..0x05FC].copy_from_slice(&[
            f2hd_off as u8,
            (f2hd_off >> 8) as u8,
            0x80,
            0xFD,
        ]);

        // CRT_RASTER (0x053B): raster count.
        self.memory.state.ram[0x053B] = 0x0F;

        // CRT_STS_FLAG (0x053C): display status.
        self.memory.state.ram[0x053C] = 0x84;

        // PRXCRT (0x054C): display config (color, GRCG present, 8MHz).
        self.memory.state.ram[0x054C] = 0x4E;

        // PRXDUPD (0x054D): graphics mode / GRCG version (0x50 = EGC present).
        self.memory.state.ram[0x054D] = if self.machine_model.has_egc() {
            0x50
        } else {
            0x00
        };

        // DISK_EQUIP (0x055C): 4 FDD drives present.
        self.memory.state.ram[0x055C] = 0x0F;

        // Keyboard shift table pointer → ROM shift table at FD80:0B28.
        self.memory.state.ram[0x0522] = 0x28; // KB_SHIFT_TBL low
        self.memory.state.ram[0x0523] = 0x0B; // KB_SHIFT_TBL high

        // Keyboard buffer pointers.
        self.memory.state.ram[0x0524] = 0x02; // KB_BUF_HEAD low
        self.memory.state.ram[0x0525] = 0x05; // KB_BUF_HEAD high
        self.memory.state.ram[0x0526] = 0x02; // KB_BUF_TAIL low
        self.memory.state.ram[0x0527] = 0x05; // KB_BUF_TAIL high
        self.memory.state.ram[0x0528] = 0x00; // KB_COUNT low
        self.memory.state.ram[0x0529] = 0x00; // KB_COUNT high

        // Keyboard code table pointer → ROM code table at FD80:0B28.
        self.memory.state.ram[0x05C6] = 0x28; // KB_CODE_OFF low
        self.memory.state.ram[0x05C7] = 0x0B; // KB_CODE_OFF high
        self.memory.state.ram[0x05C8] = 0x80; // KB_CODE_SEG low
        self.memory.state.ram[0x05C9] = 0xFD; // KB_CODE_SEG high

        // IVT: INT 1Eh at 0x0078 → E800:000A.
        self.memory.state.ram[0x0078] = 0x0A; // offset low
        self.memory.state.ram[0x0079] = 0x00; // offset high
        self.memory.state.ram[0x007A] = 0x00; // segment low
        self.memory.state.ram[0x007B] = 0xE8; // segment high
    }

    /// Loads BIOS ROM data (mapped at E8000-FFFFF, up to 96 KB).
    ///
    /// Clears the IVT and BDA entries that were populated for the embedded
    /// stub BIOS during construction. A real BIOS ROM sets these up during
    /// its own boot sequence; stale stub entries would point to wrong handler
    /// offsets and cause crashes on interrupt.
    pub fn load_bios_rom(&mut self, data: &[u8]) {
        self.memory.load_rom(data);
        self.memory.state.ram[0x0000..0x0400].fill(0);
        self.memory.state.ram[0x0400] = 0;
        self.memory.state.ram[0x0480] = 0;
        self.memory.state.ram[0x0500] = 0;
        self.memory.state.ram[0x0501] = 0;
        // Reset HLE-specific state that would interfere with real BIOS execution.
        // Shadow RAM redirect must be off so the CPU reads code from ROM.
        self.memory.set_shadow_control(0);
        self.vram_ems_bank = 0;
        self.protected_memory_max = 0;
    }

    /// Loads a V98-format font ROM into the CGROM buffer.
    pub fn load_font_rom(&mut self, data: &[u8]) {
        self.memory.load_font_rom(data);
    }

    /// Loads the PC-9801-26K sound ROM (16 KB at CC000-CFFFF).
    ///
    /// Pass `Some(data)` for a full ROM dump, or `None` to install a
    /// minimal stub that provides a no-op INT D2h handler.
    pub fn load_sound_rom(&mut self, data: Option<&[u8]>) {
        self.memory.load_sound_rom(data);
    }

    /// Inserts a floppy disk image into the specified drive (0-3).
    pub fn insert_floppy(&mut self, drive: usize, image: FloppyImage, path: Option<PathBuf>) {
        self.floppy.insert_drive(drive, image, path);
    }

    /// Returns a reference to the disk image in the given drive, if present.
    pub fn floppy_disk(&self, drive: usize) -> Option<&FloppyImage> {
        self.floppy.drive(drive)
    }

    /// Returns whether the disk in the given drive has been modified.
    pub fn is_floppy_dirty(&self, drive: usize) -> bool {
        self.floppy.is_drive_dirty(drive)
    }

    /// Ejects the floppy disk from the specified drive, flushing if dirty.
    pub fn eject_floppy(&mut self, drive: usize) {
        self.floppy.eject_drive(drive);
    }

    /// Writes the floppy image back to its file if it has been modified.
    pub fn flush_floppy(&mut self, drive: usize) {
        self.floppy.flush_drive(drive);
    }

    /// Flushes all dirty floppy images to disk.
    pub fn flush_all_floppies(&mut self) {
        self.floppy.flush_all_drives();
    }

    /// Inserts a hard disk image into the specified drive (0-1).
    pub fn insert_hdd(&mut self, drive: usize, image: HddImage, path: Option<PathBuf>) {
        self.sasi.insert_drive(drive, image, path);
    }

    /// Writes the HDD image back to its file if it has been modified.
    pub fn flush_hdd(&mut self, drive: usize) {
        self.sasi.flush_drive(drive);
    }

    /// Flushes all dirty HDD images to disk.
    pub fn flush_all_hdds(&mut self) {
        self.sasi.flush_all_drives();
    }

    /// Attaches a file handle for printer output.
    pub fn attach_printer(&mut self, file: std::fs::File) {
        self.printer.attach(file);
    }

    /// Flushes the printer output file.
    pub fn flush_printer(&mut self) {
        self.printer.flush();
    }

    /// Injects one keyboard scan code and raises IRQ1.
    pub fn push_keyboard_scancode(&mut self, code: u8) {
        self.keyboard.push_scancode(code);
        self.pic.set_irq(1);
    }

    /// Injects one serial byte and raises IRQ4.
    pub fn push_serial_byte(&mut self, data: u8) {
        self.serial.push_received_byte(data);
        self.pic.set_irq(4);
    }

    /// Injects mouse movement deltas for the current frame.
    pub fn push_mouse_delta(&mut self, dx: i16, dy: i16) {
        self.mouse_ppi.sync_frame(dx, dy, self.current_cycle);
    }

    /// Updates mouse button state.
    pub fn set_mouse_buttons(&mut self, left: bool, right: bool, middle: bool) {
        self.mouse_ppi.set_buttons(left, right, middle);
    }

    /// Returns the CPU clock frequency in Hz.
    pub fn cpu_clock_hz(&self) -> u32 {
        self.clocks.cpu_clock_hz
    }

    /// Returns the PIT clock frequency in Hz.
    pub fn pit_clock_hz(&self) -> u32 {
        self.clocks.pit_clock_hz
    }

    /// Returns a reference to the tracer.
    pub fn tracer(&self) -> &T {
        &self.tracer
    }

    /// Returns a mutable reference to the tracer.
    pub fn tracer_mut(&mut self) -> &mut T {
        &mut self.tracer
    }

    /// Returns and clears the CPU reset pending flag. If a warm-reset
    /// context was captured at the time of the port 0xF0 write, it is
    /// returned as `Some((ss, sp, cs, ip))`.
    pub fn take_reset_pending(&mut self) -> Option<Option<(u16, u16, u16, u16)>> {
        if std::mem::replace(&mut self.reset_pending, false) {
            Some(self.warm_reset_context.take())
        } else {
            None
        }
    }

    /// Returns `true` if the guest triggered a SYSTEM SHUTDOWN
    /// (SHUT0=1, SHUT1=0 when port 0xF0 was written).
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    /// Reads the warm-reset resume context stored by the BIOS at
    /// 0000:0404 (SP) and 0000:0406 (SS), then pops the far return
    /// address from that stack. Returns `(ss, sp_after_pop, cs, ip)`.
    ///
    /// This emulates what the ITF ROM does on warm reset:
    ///   SS ← [0000:0406], SP ← [0000:0404], RETF
    fn read_warm_reset_context(&self) -> (u16, u16, u16, u16) {
        let sp = self.read_word_direct(0x0404);
        let ss = self.read_word_direct(0x0406);
        let stack_base = (ss as u32) * 16 + sp as u32;
        let ret_ip = self.read_word_direct(stack_base);
        let ret_cs = self.read_word_direct(stack_base + 2);
        (ss, sp.wrapping_add(4), ret_cs, ret_ip)
    }

    /// Reads a 16-bit little-endian word directly from physical memory
    /// without side effects.
    fn read_word_direct(&self, physical_address: u32) -> u16 {
        let lo = self.read_byte_direct(physical_address) as u16;
        let hi = self.read_byte_direct(physical_address + 1) as u16;
        lo | (hi << 8)
    }

    /// Selects the ITF ROM bank for the F8000-FFFFF window.
    pub fn select_rom_bank_itf(&mut self) {
        self.memory.select_banked_rom_window(false);
    }

    /// Returns the CPU type configured for this bus.
    pub fn cpu_type(&self) -> CpuType {
        self.machine_model.cpu_type()
    }

    /// Enables CG RAM mode (VX+). All character codes become writable.
    fn set_cg_ram(&mut self, enabled: bool) {
        self.cgrom.state.cg_ram = enabled;
    }

    /// Sets the host local time provider for the µPD4990A RTC.
    ///
    /// Also updates Memory Switch 8 (`A000:3FFEh`) with the BCD year byte,
    /// since the µPD1990A (used by VM-class machines) has no year register
    /// and the BIOS reads the year from the memory switch instead.
    pub fn set_host_local_time_fn(&mut self, f: fn() -> [u8; 6]) {
        self.host_local_time_fn = f;
        self.memory.state.text_vram[0x3FFE] = f()[0];
    }

    /// Enables/disables the 16-color graphics extension board.
    pub fn set_graphics_extension_enabled(&mut self, enabled: bool) {
        self.graphics_extension_enabled = enabled;
        self.system_ppi.set_graphics_extension_bit(enabled);
        self.update_plane_e_mapping();
    }

    /// Installs the PC-9801-26K sound board (YM2203 OPN).
    ///
    /// When `alternate_timers` is `true`, the board uses `FmTimer2A`/`FmTimer2B`
    /// event kinds instead of `FmTimerA`/`FmTimerB` (for dual-board configurations
    /// where the 86 board uses the primary timer events).
    pub fn install_soundboard_26k(&mut self, alternate_timers: bool) {
        let sample_rate = self.beeper.state.sample_rate;
        self.soundboard_26k = Some(Soundboard26k::new(
            self.clocks.cpu_clock_hz,
            sample_rate,
            alternate_timers,
        ));
        self.resolve_dual_soundboard_irq_conflict();
    }

    /// Installs the PC-9801-86 sound board (YM2608 OPNA + PCM86).
    ///
    /// `rhythm_rom` is the optional 8 KB `ym2608.rom` ADPCM-A rhythm ROM.
    /// When installed, the 86 board replaces the 26K for FM/SSG ports
    /// and adds extended register and PCM86 ports.
    pub fn install_soundboard_86(&mut self, rhythm_rom: Option<&[u8]>) {
        let sample_rate = self.beeper.state.sample_rate;
        self.soundboard_86 = Some(Soundboard86::new(
            self.clocks.cpu_clock_hz,
            sample_rate,
            rhythm_rom,
        ));
        self.resolve_dual_soundboard_irq_conflict();
    }

    /// Returns `true` if the PC-9801-86 sound board is installed.
    pub fn has_soundboard_86(&self) -> bool {
        self.soundboard_86.is_some()
    }

    fn resolve_dual_soundboard_irq_conflict(&mut self) {
        let (Some(soundboard_26k), Some(soundboard_86)) =
            (&mut self.soundboard_26k, &self.soundboard_86)
        else {
            return;
        };

        // 86+26K dual-board setups must not share the same IRQ line.
        // NP21W resolves the default 12/12 collision by moving the 26K
        // board to IRQ10.
        if soundboard_26k.state.irq_line == soundboard_86.state.irq_line {
            soundboard_26k.state.irq_line = if soundboard_26k.state.irq_line == 12 {
                10
            } else {
                12
            };
        }
    }

    /// Returns the CPU cycle at which the next scheduled event fires, if any.
    pub fn next_event_cycle(&self) -> Option<u64> {
        self.scheduler.next_event_cycle()
    }

    /// Returns whether the PIC has a pending IRQ for the CPU.
    pub fn has_irq_pending(&self) -> bool {
        self.pic.has_pending_irq()
    }

    /// Returns PIC master chip debug info (IRR, IMR, ISR).
    pub fn pic_debug(&self) -> (u8, u8, u8) {
        let c = &self.pic.state.chips[0];
        (c.irr, c.imr, c.isr)
    }

    /// Returns PIT channel 0 debug info (ctrl, value, flag).
    pub fn pit_debug(&self) -> (u8, u16, u8) {
        let ch = &self.pit.state.channels[0];
        (ch.ctrl, ch.value, ch.flag)
    }

    /// Returns the next scheduled event cycle (if any).
    pub fn next_event_debug(&self) -> Option<u64> {
        self.scheduler.next_event_cycle()
    }

    fn update_plane_e_mapping(&mut self) {
        self.memory.set_e_plane_enabled(
            self.graphics_extension_enabled && self.display_control.is_palette_analog_mode(),
        );
    }

    fn mouse_timer_irq_enabled(&self) -> bool {
        (self.mouse_ppi.state.port_c & 0x10) == 0
    }

    fn mouse_timer_period_cycles(&self) -> u64 {
        let hz = match self.mouse_timer_setting & 0x03 {
            0x00 => 120u64,
            0x01 => 60u64,
            0x02 => 30u64,
            _ => 15u64,
        };
        let cpu = u64::from(self.clocks.cpu_clock_hz);
        cpu.div_ceil(hz)
    }

    fn schedule_mouse_timer(&mut self) {
        let next = self
            .current_cycle
            .wrapping_add(self.mouse_timer_period_cycles().max(1));
        self.scheduler.schedule(EventKind::MouseTimer, next);
    }

    fn handle_mouse_timer_control_change(&mut self, was_enabled: bool) {
        let enabled = self.mouse_timer_irq_enabled();
        if enabled && !was_enabled {
            self.schedule_mouse_timer();
            self.update_next_event_cycle();
        } else if !enabled && was_enabled {
            self.scheduler.cancel(EventKind::MouseTimer);
            self.pic.clear_irq(MOUSE_TIMER_IRQ_LINE);
            self.tracer.trace_irq_clear(MOUSE_TIMER_IRQ_LINE);
            self.update_next_event_cycle();
        }
    }

    fn current_dot_clock_hz(&self) -> u32 {
        let base = if self.display_control.state.display_line_count & 1 != 0 {
            DOT_CLOCK_400LINE
        } else {
            DOT_CLOCK_200LINE
        };
        if self.display_control.is_gdc_5mhz() {
            base.saturating_mul(2)
        } else {
            base
        }
    }

    fn apply_gdc_dot_clock(&mut self) {
        let dot_clock = self.current_dot_clock_hz();
        self.gdc_master.set_dot_clock(dot_clock);
        self.gdc_slave.set_dot_clock(dot_clock);
    }

    fn access_page_index(&self) -> usize {
        usize::from(self.display_control.state.access_page & 1)
    }

    fn display_page_index(&self) -> usize {
        usize::from(self.display_control.state.display_page & 1)
    }

    fn graphics_plane_read_byte_from_page(&self, page: usize, plane: usize, offset: usize) -> u8 {
        let page_base = page * GRAPHICS_PAGE_SIZE_BYTES;
        match plane {
            0..=2 => self.memory.state.graphics_vram[page_base + plane * 0x8000 + offset],
            3 => {
                let e_page_base = page * E_PLANE_PAGE_SIZE_BYTES;
                self.memory.state.e_plane_vram[e_page_base + offset]
            }
            _ => unreachable!("graphics plane index out of range: {plane}"),
        }
    }

    fn graphics_plane_write_byte_to_page(
        &mut self,
        page: usize,
        plane: usize,
        offset: usize,
        value: u8,
    ) {
        let page_base = page * GRAPHICS_PAGE_SIZE_BYTES;
        match plane {
            0..=2 => self.memory.state.graphics_vram[page_base + plane * 0x8000 + offset] = value,
            3 => {
                let e_page_base = page * E_PLANE_PAGE_SIZE_BYTES;
                self.memory.state.e_plane_vram[e_page_base + offset] = value;
            }
            _ => unreachable!("graphics plane index out of range: {plane}"),
        }
    }

    /// Returns the current ARTIC timestamp counter (24-bit, 307.2 kHz).
    ///
    /// Ref: undoc98 `io_tstmp.txt` (ports 0x005C-0x005F).
    fn artic_counter(&self) -> u32 {
        let ticks = (u128::from(self.current_cycle) * 307_200u128
            / u128::from(self.clocks.cpu_clock_hz)) as u32;
        ticks & 0x00FF_FFFF
    }

    /// Returns the hardware wait penalty for `OUT 0x005F,AL` (>= 0.6 us).
    fn artic_wait_cycles(&self) -> i64 {
        let cycles = (u64::from(self.clocks.cpu_clock_hz) * 6).div_ceil(10_000_000);
        cycles.max(1) as i64
    }

    fn graphics_plane_read_byte(&self, plane: usize, offset: usize) -> u8 {
        self.graphics_plane_read_byte_from_page(self.access_page_index(), plane, offset)
    }

    fn graphics_plane_write_byte(&mut self, plane: usize, offset: usize, value: u8) {
        self.graphics_plane_write_byte_to_page(self.access_page_index(), plane, offset, value);
    }

    fn is_memory_switch_address(address: u32) -> bool {
        (0xA3FE2..=0xA3FFE).contains(&address) && (address - 0xA3FE2).is_multiple_of(4)
    }

    fn read_byte_with_access_page(&self, address: u32) -> u8 {
        if (0x80000..=0x9FFFF).contains(&address) && self.ram_window != 0x08 {
            let physical = ((self.ram_window & 0xFE) as u32) * 0x10000 + (address - 0x80000);
            if (0xE0000..=0xFFFFF).contains(&physical)
                && (self.memory.state.shadow_control & 0x04) != 0
            {
                return 0xFF;
            }
            return self.memory.read_byte(physical);
        }
        match address {
            0xA4000..=0xA4FFF if self.grcg.state.chip >= 2 => {
                let window = self
                    .cgrom
                    .compute_window(self.display_control.is_font_7x13_mode());
                let line = ((address >> 1) & 0x0F) as usize;
                if address & 1 != 0 {
                    self.memory.font_read(window.high + line)
                } else {
                    self.memory.font_read(window.low + line)
                }
            }
            0xA8000..=0xAFFFF => {
                let page_base = self.access_page_index() * GRAPHICS_PAGE_SIZE_BYTES;
                self.memory.state.graphics_vram[page_base + (address - 0xA8000) as usize]
            }
            0xB0000..=0xBFFFF => {
                if self.b_bank_ems && self.vram_ems_bank & 0x02 != 0 {
                    self.memory.read_byte(0x100000 + (address - 0xB0000))
                } else {
                    let page_base = self.access_page_index() * GRAPHICS_PAGE_SIZE_BYTES;
                    self.memory.state.graphics_vram[page_base + (address - 0xA8000) as usize]
                }
            }
            // SASI HLE ROM overlay (expansion ROM area).
            0xD7000..=0xD7FFF => {
                if self.sasi.rom_installed() {
                    self.sasi.read_rom_byte((address - 0xD7000) as usize)
                } else {
                    self.memory.read_byte(address)
                }
            }
            0xE0000..=0xE7FFF => {
                if self.memory.state.e_plane_enabled {
                    let page_base = self.access_page_index() * E_PLANE_PAGE_SIZE_BYTES;
                    self.memory.state.e_plane_vram[page_base + (address - 0xE0000) as usize]
                } else {
                    0xFF
                }
            }
            _ => self.memory.read_byte(address),
        }
    }

    fn write_byte_with_access_page(&mut self, address: u32, value: u8) {
        if Self::is_memory_switch_address(address)
            && !self.display_control.is_memory_switch_write_enabled()
        {
            return;
        }

        if (0x80000..=0x9FFFF).contains(&address) && self.ram_window != 0x08 {
            let physical = ((self.ram_window & 0xFE) as u32) * 0x10000 + (address - 0x80000);
            if (0xE0000..=0xFFFFF).contains(&physical)
                && (self.memory.state.shadow_control & 0x04) != 0
            {
                return;
            }
            self.memory.write_byte(physical, value);
            return;
        }

        match address {
            0xA4000..=0xA4FFF if self.grcg.state.chip >= 2 => {
                let window = self
                    .cgrom
                    .compute_window(self.display_control.is_font_7x13_mode());
                if (address & 1 != 0) && window.writable {
                    let line = ((address >> 1) & 0x0F) as usize;
                    self.memory.font_write(window.high + line, value);
                }
            }
            0xA8000..=0xAFFFF => {
                let page_base = self.access_page_index() * GRAPHICS_PAGE_SIZE_BYTES;
                self.memory.state.graphics_vram[page_base + (address - 0xA8000) as usize] = value;
            }
            0xB0000..=0xBFFFF => {
                if self.b_bank_ems && self.vram_ems_bank & 0x02 != 0 {
                    self.memory
                        .write_byte(0x100000 + (address - 0xB0000), value);
                } else {
                    let page_base = self.access_page_index() * GRAPHICS_PAGE_SIZE_BYTES;
                    self.memory.state.graphics_vram[page_base + (address - 0xA8000) as usize] =
                        value;
                }
            }
            0xE0000..=0xE7FFF => {
                if self.memory.state.e_plane_enabled {
                    let page_base = self.access_page_index() * E_PLANE_PAGE_SIZE_BYTES;
                    self.memory.state.e_plane_vram[page_base + (address - 0xE0000) as usize] =
                        value;
                }
            }
            _ => self.memory.write_byte(address, value),
        }
    }

    fn grcg_write_byte(&mut self, address: u32, value: u8) {
        self.pending_wait_cycles += self.grcg_wait;
        let offset = Self::graphics_vram_offset(address);
        if !self.grcg.is_rmw() {
            // TDW: write tile to each enabled plane, ignore CPU data
            for p in 0..4 {
                if p == 3 && !self.graphics_extension_enabled {
                    continue;
                }
                if self.grcg.plane_enabled(p) {
                    self.graphics_plane_write_byte(p, offset, self.grcg.state.tile[p]);
                }
            }
        } else {
            // RMW: bit-select between tile and existing VRAM
            for p in 0..4 {
                if p == 3 && !self.graphics_extension_enabled {
                    continue;
                }
                if self.grcg.plane_enabled(p) {
                    let current = self.graphics_plane_read_byte(p, offset);
                    let next = (value & self.grcg.state.tile[p]) | (!value & current);
                    self.graphics_plane_write_byte(p, offset, next);
                }
            }
        }
    }

    fn grcg_read_byte(&mut self, address: u32) -> u8 {
        if self.grcg.is_rmw() {
            // RMW reads use standard VRAM wait.
            self.pending_wait_cycles += self.vram_wait;
            return self.read_byte_with_access_page(address);
        }
        // TCR reads use GRCG wait.
        self.pending_wait_cycles += self.grcg_wait;
        // TCR: compare VRAM against tiles, return match bitmask
        let offset = Self::graphics_vram_offset(address);
        let mut result = 0xFF;
        for p in 0..4 {
            if p == 3 && !self.graphics_extension_enabled {
                continue;
            }
            if self.grcg.plane_enabled(p) {
                result &= !(self.graphics_plane_read_byte(p, offset) ^ self.grcg.state.tile[p]);
            }
        }
        result
    }

    fn grcg_write_word(&mut self, address: u32, value: u16) {
        self.pending_wait_cycles += self.grcg_wait;
        let offset = Self::graphics_vram_offset(address);
        let low = value as u8;
        let high = (value >> 8) as u8;
        if !self.grcg.is_rmw() {
            for p in 0..4 {
                if p == 3 && !self.graphics_extension_enabled {
                    continue;
                }
                if self.grcg.plane_enabled(p) {
                    self.graphics_plane_write_byte(p, offset, self.grcg.state.tile[p]);
                    self.graphics_plane_write_byte(p, offset + 1, self.grcg.state.tile[p]);
                }
            }
        } else {
            for p in 0..4 {
                if p == 3 && !self.graphics_extension_enabled {
                    continue;
                }
                if self.grcg.plane_enabled(p) {
                    let cur_lo = self.graphics_plane_read_byte(p, offset);
                    let cur_hi = self.graphics_plane_read_byte(p, offset + 1);
                    let next_lo = (low & self.grcg.state.tile[p]) | (!low & cur_lo);
                    let next_hi = (high & self.grcg.state.tile[p]) | (!high & cur_hi);
                    self.graphics_plane_write_byte(p, offset, next_lo);
                    self.graphics_plane_write_byte(p, offset + 1, next_hi);
                }
            }
        }
    }

    fn grcg_read_word(&mut self, address: u32) -> u16 {
        if self.grcg.is_rmw() {
            self.pending_wait_cycles += self.vram_wait;
            let low = self.read_byte_with_access_page(address) as u16;
            let high = self.read_byte_with_access_page(address.wrapping_add(1)) as u16;
            return low | (high << 8);
        }
        self.pending_wait_cycles += self.grcg_wait;
        let offset = Self::graphics_vram_offset(address);
        let mut result_lo: u8 = 0xFF;
        let mut result_hi: u8 = 0xFF;
        for p in 0..4 {
            if p == 3 && !self.graphics_extension_enabled {
                continue;
            }
            if self.grcg.plane_enabled(p) {
                result_lo &= !(self.graphics_plane_read_byte(p, offset) ^ self.grcg.state.tile[p]);
                result_hi &=
                    !(self.graphics_plane_read_byte(p, offset + 1) ^ self.grcg.state.tile[p]);
            }
        }
        u16::from(result_lo) | (u16::from(result_hi) << 8)
    }

    /// Returns true if EGC mode is currently effective (EGC mode enabled + GRCG active).
    fn is_egc_effective(&self) -> bool {
        self.display_control.is_egc_extended_mode_effective() && self.grcg.is_active()
    }

    /// Converts a CPU address in the graphics VRAM range to a byte offset (0..0x7FFF).
    /// Works for both 0xA8000-0xBFFFF (B/R/G planes) and 0xE0000-0xE7FFF (E plane).
    fn graphics_vram_offset(address: u32) -> usize {
        if address >= 0xE0000 {
            (address - 0xE0000) as usize & 0x7FFF
        } else {
            (address - 0xA8000) as usize & 0x7FFF
        }
    }

    fn egc_read_byte(&mut self, address: u32) -> u8 {
        self.pending_wait_cycles += self.grcg_wait;
        self.egc_read_byte_inner(address)
    }

    fn egc_read_byte_inner(&mut self, address: u32) -> u8 {
        let offset = Self::graphics_vram_offset(address);
        let vram = [
            self.graphics_plane_read_byte(0, offset),
            self.graphics_plane_read_byte(1, offset),
            self.graphics_plane_read_byte(2, offset),
            if self.graphics_extension_enabled {
                self.graphics_plane_read_byte(3, offset)
            } else {
                0
            },
        ];
        self.egc.read_byte(address, vram)
    }

    fn egc_write_byte(&mut self, address: u32, value: u8) {
        self.pending_wait_cycles += self.grcg_wait;
        self.egc_write_byte_inner(address, value);
    }

    fn egc_write_byte_inner(&mut self, address: u32, value: u8) {
        let offset = Self::graphics_vram_offset(address);
        let vram = [
            self.graphics_plane_read_byte(0, offset),
            self.graphics_plane_read_byte(1, offset),
            self.graphics_plane_read_byte(2, offset),
            if self.graphics_extension_enabled {
                self.graphics_plane_read_byte(3, offset)
            } else {
                0
            },
        ];
        let (data, mask) = self.egc.write_byte(address, value, vram);
        if mask != 0 {
            for (p, &plane_data) in data.iter().enumerate() {
                if p == 3 && !self.graphics_extension_enabled {
                    continue;
                }
                if self.egc.plane_write_enabled(p) {
                    let current = self.graphics_plane_read_byte(p, offset);
                    let result = (current & !mask) | (plane_data & mask);
                    self.graphics_plane_write_byte(p, offset, result);
                }
            }
        }
    }

    fn egc_read_word(&mut self, address: u32) -> u16 {
        self.pending_wait_cycles += self.grcg_wait;
        if address & 1 != 0 {
            // Misaligned: decompose into two byte operations (no extra wait charge).
            return if !self.egc.is_descending() {
                let lo = self.egc_read_byte_inner(address) as u16;
                let hi = self.egc_read_byte_inner(address + 1) as u16;
                lo | (hi << 8)
            } else {
                let hi = self.egc_read_byte_inner(address + 1) as u16;
                let lo = self.egc_read_byte_inner(address) as u16;
                lo | (hi << 8)
            };
        }
        let offset = Self::graphics_vram_offset(address);
        let vram = [
            self.graphics_plane_read_word(0, offset),
            self.graphics_plane_read_word(1, offset),
            self.graphics_plane_read_word(2, offset),
            if self.graphics_extension_enabled {
                self.graphics_plane_read_word(3, offset)
            } else {
                0
            },
        ];
        self.egc.read_word(address, vram)
    }

    fn egc_write_word(&mut self, address: u32, value: u16) {
        self.pending_wait_cycles += self.grcg_wait;
        self.egc_write_word_inner(address, value);
    }

    fn egc_write_word_inner(&mut self, address: u32, value: u16) {
        if address & 1 != 0 {
            // Misaligned: decompose into two byte operations (no extra wait charge).
            if !self.egc.is_descending() {
                self.egc_write_byte_inner(address, value as u8);
                self.egc_write_byte_inner(address + 1, (value >> 8) as u8);
            } else {
                self.egc_write_byte_inner(address + 1, (value >> 8) as u8);
                self.egc_write_byte_inner(address, value as u8);
            }
            return;
        }
        let offset = Self::graphics_vram_offset(address);
        let vram = [
            self.graphics_plane_read_word(0, offset),
            self.graphics_plane_read_word(1, offset),
            self.graphics_plane_read_word(2, offset),
            if self.graphics_extension_enabled {
                self.graphics_plane_read_word(3, offset)
            } else {
                0
            },
        ];
        let (data, mask) = self.egc.write_word(address, value, vram);
        if mask != 0 {
            for (p, &plane_data) in data.iter().enumerate() {
                if p == 3 && !self.graphics_extension_enabled {
                    continue;
                }
                if self.egc.plane_write_enabled(p) {
                    let current = self.graphics_plane_read_word(p, offset);
                    let result = (current & !mask) | (plane_data & mask);
                    self.graphics_plane_write_word(p, offset, result);
                }
            }
        }
    }

    fn graphics_plane_read_word(&self, plane: usize, offset: usize) -> u16 {
        let lo = self.graphics_plane_read_byte(plane, offset) as u16;
        let hi = self.graphics_plane_read_byte(plane, offset + 1) as u16;
        lo | (hi << 8)
    }

    fn graphics_plane_write_word(&mut self, plane: usize, offset: usize, value: u16) {
        self.graphics_plane_write_byte(plane, offset, value as u8);
        self.graphics_plane_write_byte(plane, offset + 1, (value >> 8) as u8);
    }

    /// Returns a reference to the last VSYNC display snapshot.
    pub fn vsync_snapshot(&self) -> &DisplaySnapshotUpload {
        &self.vsync_snapshot
    }

    /// Captures the current display state into the internal VSYNC snapshot buffer.
    pub fn capture_vsync_snapshot(&mut self) {
        let display_page_base = self.display_page_index() * GRAPHICS_PAGE_SIZE_BYTES;
        let e_page_base = self.display_page_index() * E_PLANE_PAGE_SIZE_BYTES;

        // Display flags:
        // bit 0 = GDC started,
        // bit 1 = blink visible,
        // bit 2 = hide odd rasters,
        // bit 3 = 16-color mode,
        // bit 4 = text display enabled (master GDC DE),
        // bit 5 = graphics display enabled (slave GDC DE),
        // bit 6 = global display enable (mode1 bit 7).
        // Blink timing: derive a phase counter from the monotonic VSYNC blink_counter.
        //   threshold = cursor_blink_rate * 2, or 64 when rate == 0
        //   count increments every `threshold` VSYNCs
        //   cursor: count & 1 (50% duty), text: (count & 3) != 0 (75/25% duty)
        let blink_rate = u16::from(self.gdc_master.state.cursor_blink_rate);
        let blink_threshold = if blink_rate == 0 {
            64u16
        } else {
            blink_rate * 2
        };
        let blink_count = self.gdc_master.state.blink_counter / blink_threshold;
        let text_blink_visible = (blink_count & 3) != 0;
        let video_mode = self.display_control.state.video_mode;
        let hide_odd_rasters = u32::from(self.display_control.is_hide_odd_rasters_enabled());
        let is_16_color = u32::from(self.display_control.is_16_color());
        let text_display_enabled = u32::from(self.gdc_master.state.display_enabled);
        let graphics_display_enabled = u32::from(self.gdc_slave.state.display_enabled);
        let global_display_enabled = u32::from(self.display_control.is_display_enabled_global());
        let is_graphics_monochrome = self.display_control.is_graphics_monochrome();
        let is_palette_analog_mode = self.display_control.is_palette_analog_mode();
        let is_kac_dot_access_mode = self.display_control.is_kac_dot_access_mode();
        let interlace_on = self.gdc_slave.state.interlace_mode == 0x09;

        let snapshot = &mut *self.vsync_snapshot;

        // Palette: pack analog [green_4bit, red_4bit, blue_4bit] → u32 as 0xFF_BB_GG_RR.
        for i in 0..16 {
            let [g4, r4, b4] = self.palette.state.analog[i];
            let r8 = (r4 & 0x0F) * 17;
            let g8 = (g4 & 0x0F) * 17;
            let b8 = (b4 & 0x0F) * 17;
            snapshot.palette_rgba[i] =
                u32::from(r8) | (u32::from(g8) << 8) | (u32::from(b8) << 16) | 0xFF00_0000;
        }

        // Display flags.
        snapshot.display_flags = 1
            | (u32::from(text_blink_visible) << 1)
            | (hide_odd_rasters << 2)
            | (is_16_color << 3)
            | (text_display_enabled << 4)
            | (graphics_display_enabled << 5)
            | (global_display_enabled << 6);

        // GDC text pitch.
        snapshot.gdc_text_pitch = u32::from(self.gdc_master.state.pitch);

        // GDC text scroll areas.
        for i in 0..4 {
            let area = &self.gdc_master.state.scroll[i];
            snapshot.gdc_scroll_start_line[i] =
                area.start_address | (u32::from(area.line_count) << 16);
        }

        // GDC graphics pitch.
        snapshot.gdc_graphics_pitch = u32::from(self.gdc_slave.state.pitch);

        // Video mode register (port 0x68).
        snapshot.video_mode = u32::from(video_mode);

        // Monochrome mask: bitmask of graphics color indices that are "on".
        snapshot.graphics_monochrome_mask = if is_graphics_monochrome {
            let mut mask: u32 = 0;
            if is_palette_analog_mode {
                for i in 0..16u32 {
                    if self.palette.state.analog[i as usize][0] & 0x08 != 0 {
                        mask |= 1 << i;
                    }
                }
            } else {
                for i in 0..4u32 {
                    let dp = self.palette.state.digital[i as usize];
                    if dp & 0x40 != 0 {
                        mask |= (1 << i) | (1 << (i + 8));
                    }
                    if dp & 0x04 != 0 {
                        mask |= (1 << (i + 4)) | (1 << (i + 12));
                    }
                }
            }
            mask
        } else {
            0
        };

        // Graphics GDC line repeat factor from CSRFORM.
        snapshot.gdc_graphics_lines_per_row = u32::from(self.gdc_slave.state.lines_per_row);

        // Graphics GDC display zoom factor.
        snapshot.gdc_graphics_zoom_display = u32::from(self.gdc_slave.state.zoom_display);

        // Interlace mode.
        snapshot.gdc_interlace_mode = u32::from(self.gdc_slave.state.interlace_mode);

        // GDC graphics scroll areas — double partition line counts for interlace ON mode.
        for i in 0..4 {
            let area = &self.gdc_slave.state.scroll[i];
            let line_count = if interlace_on {
                area.line_count.saturating_mul(2)
            } else {
                area.line_count
            };
            snapshot.gdc_graphics_scroll[i] = area.start_address | (u32::from(line_count) << 16);
        }

        // Text VRAM.
        let text_vram_words = cast_u32_slice_as_bytes_mut(&mut snapshot.text_vram_words);
        text_vram_words.copy_from_slice(&*self.memory.state.text_vram);

        // KAC-mode-derived mask for kanji high-byte detection in compose.
        snapshot.gdc_text_kanji_high_mask = if is_kac_dot_access_mode { 0x00 } else { 0xFF };

        // CRTC registers.
        snapshot.crtc_pl_bl =
            u32::from(self.crtc.state.regs[0]) | (u32::from(self.crtc.state.regs[1]) << 16);
        snapshot.crtc_cl_ssl =
            u32::from(self.crtc.state.regs[2]) | (u32::from(self.crtc.state.regs[3]) << 16);
        snapshot.crtc_sur_sdr =
            u32::from(self.crtc.state.regs[4]) | (u32::from(self.crtc.state.regs[5]) << 16);

        // Text cursor (master GDC EAD = character address in text VRAM).
        // Bit layout: [31] visible, [27:23] cursor_bottom, [22:18] cursor_top, [17:0] address.
        let cursor_blink_visible = if self.gdc_master.state.cursor_blink {
            true
        } else {
            (blink_count & 1) != 0
        };
        let cursor_enabled = self.gdc_master.state.cursor_display && cursor_blink_visible;
        let cursor_addr = self.gdc_master.state.ead;
        let cursor_top = u32::from(self.gdc_master.state.cursor_top & 0x1F);
        let cursor_bottom = u32::from(self.gdc_master.state.cursor_bottom & 0x1F);
        snapshot.text_cursor = if cursor_enabled {
            cursor_addr | (cursor_top << 18) | (cursor_bottom << 23) | 0x8000_0000
        } else {
            0
        };

        // Graphics VRAM planes (selected display page).
        let b_plane = cast_u32_slice_as_bytes_mut(&mut snapshot.graphics_b_plane);
        b_plane.copy_from_slice(
            &self.memory.state.graphics_vram[display_page_base..display_page_base + 0x8000],
        );

        let r_plane = cast_u32_slice_as_bytes_mut(&mut snapshot.graphics_r_plane);
        r_plane.copy_from_slice(
            &self.memory.state.graphics_vram
                [display_page_base + 0x8000..display_page_base + 0x10000],
        );

        let g_plane = cast_u32_slice_as_bytes_mut(&mut snapshot.graphics_g_plane);
        g_plane.copy_from_slice(
            &self.memory.state.graphics_vram
                [display_page_base + 0x10000..display_page_base + 0x18000],
        );

        let e_plane = cast_u32_slice_as_bytes_mut(&mut snapshot.graphics_e_plane);
        e_plane.copy_from_slice(
            &self.memory.state.e_plane_vram[e_page_base..e_page_base + E_PLANE_PAGE_SIZE_BYTES],
        );
    }

    /// Generates audio samples for the current frame.
    ///
    /// Mixes beeper (PIT ch1 square wave) with YM2203 FM + SSG output.
    pub fn generate_audio_samples(&mut self, volume: f32, output: &mut [f32]) -> usize {
        let beeper_count = self.beeper.generate_samples(
            self.current_cycle,
            self.clocks.cpu_clock_hz,
            self.clocks.pit_clock_hz,
            volume,
            output,
        );

        if let Some(ref mut sb86) = self.soundboard_86 {
            sb86.generate_samples(self.current_cycle, self.clocks.cpu_clock_hz, volume, output);
        }
        if let Some(ref mut sb26k) = self.soundboard_26k {
            sb26k.generate_samples(self.current_cycle, self.clocks.cpu_clock_hz, volume, output);
        }

        beeper_count
    }

    /// Reads a single byte directly from the full address space without side effects.
    pub fn read_byte_direct(&self, physical_address: u32) -> u8 {
        self.read_byte_with_access_page(physical_address)
    }

    /// Returns a reference to the raw text VRAM contents (16 KB).
    pub fn text_vram(&self) -> &[u8] {
        self.memory.state.text_vram.as_slice()
    }

    /// Returns a reference to the raw graphics VRAM (B/R/G planes, 2 pages).
    pub fn graphics_vram(&self) -> &[u8] {
        self.memory.state.graphics_vram.as_slice()
    }

    /// Returns a reference to the E-plane VRAM (2 pages).
    pub fn e_plane_vram(&self) -> &[u8] {
        self.memory.state.e_plane_vram.as_slice()
    }

    /// Returns the kanji font ROM data (512 KB, double-byte 16×16 glyphs).
    pub fn font_rom_data(&self) -> &[u8] {
        self.memory.font_rom_data()
    }

    /// Returns `true` if gaiji were written since the last call, and clears the flag.
    pub fn take_font_rom_dirty(&mut self) -> bool {
        self.memory.take_font_rom_dirty()
    }

    fn a20_mask(&self, address: u32) -> u32 {
        if self.a20_enabled {
            address
        } else {
            address & !0x0010_0000
        }
    }

    pub(crate) fn save_state(&self, cpu: crate::CpuState) -> crate::MachineState {
        crate::MachineState {
            cpu,
            machine_model: self.machine_model,
            memory: self.memory.state.clone(),
            clocks: self.clocks,
            pic: self.pic.state.clone(),
            scheduler: self.scheduler.state.clone(),
            pit: self.pit.state.clone(),
            gdc_master: self.gdc_master.state.clone(),
            gdc_slave: self.gdc_slave.state.clone(),
            current_cycle: self.current_cycle,
            next_event_cycle: self.next_event_cycle,
            nmi_enabled: self.nmi_enabled,
            keyboard: self.keyboard.state.clone(),
            serial: self.serial.state.clone(),
            a20_enabled: self.a20_enabled,
            fdc_1mb: self.floppy.fdc_1mb().state.clone(),
            fdc_640k: self.floppy.fdc_640k().state.clone(),
            fdc_media: self.floppy.fdc_media(),
            vram_ems_bank: self.vram_ems_bank,
            ram_window: self.ram_window,
            system_ppi: self.system_ppi.state.clone(),
            printer: self.printer.state.clone(),
            cgrom: self.cgrom.state.clone(),
            grcg: self.grcg.state.clone(),
            egc: self.egc.state.clone(),
            display_control: self.display_control.state.clone(),
            crtc: self.crtc.state.clone(),
            palette: self.palette.state.clone(),
            soundboard_26k: self.soundboard_26k.as_ref().map(|sb| sb.save_state()),
            soundboard_86: self.soundboard_86.as_ref().map(|sb| sb.save_state()),
            beeper: self.beeper.state.clone(),
            mouse_ppi: self.mouse_ppi.state.clone(),
            mouse_timer_setting: self.mouse_timer_setting,
            hole_15m_control: self.hole_15m_control,
            protected_memory_max: self.protected_memory_max,
            b_bank_ems: self.b_bank_ems,
            tram_wait: self.tram_wait,
            vram_wait: self.vram_wait,
            grcg_wait: self.grcg_wait,
        }
    }

    pub(crate) fn load_peripherals(&mut self, state: &crate::MachineState) {
        self.machine_model = state.machine_model;
        self.memory.state = state.memory.clone();
        self.pic.state = state.pic.clone();
        self.scheduler.state = state.scheduler.clone();
        self.current_cycle = state.current_cycle;
        self.next_event_cycle = state.next_event_cycle;
        self.nmi_enabled = state.nmi_enabled;
        self.clocks = state.clocks;
        self.pit.state = state.pit.clone();
        self.gdc_master.state = state.gdc_master.clone();
        self.gdc_slave.state = state.gdc_slave.clone();
        self.keyboard.state = state.keyboard.clone();
        self.serial.state = state.serial.clone();
        self.a20_enabled = state.a20_enabled;
        self.floppy.fdc_1mb_mut().state = state.fdc_1mb.clone();
        self.floppy.fdc_640k_mut().state = state.fdc_640k.clone();
        self.floppy.set_fdc_media(state.fdc_media);
        self.vram_ems_bank = state.vram_ems_bank;
        self.ram_window = state.ram_window;
        self.system_ppi.state = state.system_ppi.clone();
        self.printer.state = state.printer.clone();
        self.cgrom.state = state.cgrom.clone();
        self.grcg.state = state.grcg.clone();
        self.egc.state = state.egc.clone();
        self.display_control.state = state.display_control.clone();
        self.crtc.state = state.crtc.clone();
        self.palette.state = state.palette.clone();
        if let (Some(sb26k), Some(saved)) = (&mut self.soundboard_26k, &state.soundboard_26k) {
            sb26k.load_state(
                saved,
                self.clocks.cpu_clock_hz,
                state.beeper.sample_rate,
                state.current_cycle,
            );
        }
        if let (Some(sb86), Some(saved)) = (&mut self.soundboard_86, &state.soundboard_86) {
            sb86.load_state(
                saved,
                self.clocks.cpu_clock_hz,
                state.beeper.sample_rate,
                state.current_cycle,
                None,
            );
        }
        self.beeper.state = state.beeper.clone();
        self.mouse_ppi.state = state.mouse_ppi.clone();
        self.mouse_ppi.set_cpu_clock(self.clocks.cpu_clock_hz);
        self.mouse_timer_setting = state.mouse_timer_setting;
        self.hole_15m_control = state.hole_15m_control;
        self.protected_memory_max = state.protected_memory_max;
        self.b_bank_ems = state.b_bank_ems;
        self.tram_wait = state.tram_wait;
        self.vram_wait = state.vram_wait;
        self.grcg_wait = state.grcg_wait;
        self.reset_pending = false;
        self.shutdown_requested = false;
    }

    fn process_soundboard_86_actions(&mut self) {
        if let Some(ref mut sb86) = self.soundboard_86 {
            for action in sb86.drain_actions() {
                match action {
                    Soundboard86Action::ScheduleTimer { kind, fire_cycle } => {
                        self.scheduler.schedule(kind, fire_cycle);
                    }
                    Soundboard86Action::CancelTimer { kind } => {
                        self.scheduler.cancel(kind);
                    }
                    Soundboard86Action::AssertIrq { irq } => {
                        self.pic.set_irq(irq);
                        self.tracer.trace_irq_raise(irq);
                    }
                    Soundboard86Action::DeassertIrq { irq } => {
                        self.pic.clear_irq(irq);
                    }
                }
            }
        }
        self.update_next_event_cycle();
    }

    fn process_soundboard_actions(&mut self) {
        if let Some(ref mut sb26k) = self.soundboard_26k {
            for action in sb26k.drain_actions() {
                match action {
                    Soundboard26kAction::ScheduleTimer { kind, fire_cycle } => {
                        self.scheduler.schedule(kind, fire_cycle);
                    }
                    Soundboard26kAction::CancelTimer { kind } => {
                        self.scheduler.cancel(kind);
                    }
                    Soundboard26kAction::AssertIrq { irq } => {
                        self.pic.set_irq(irq);
                        self.tracer.trace_irq_raise(irq);
                    }
                    Soundboard26kAction::DeassertIrq { irq } => {
                        self.pic.clear_irq(irq);
                        self.tracer.trace_irq_clear(irq);
                    }
                }
            }
        }
        self.update_next_event_cycle();
    }

    fn update_next_event_cycle(&mut self) {
        self.next_event_cycle = self.scheduler.next_event_cycle().unwrap_or(u64::MAX);
    }

    fn reschedule_gdc_events(&mut self) {
        self.scheduler.schedule(
            EventKind::GdcVsync,
            self.current_cycle + self.gdc_master.state.display_period,
        );
        self.update_next_event_cycle();
    }

    fn process_events(&mut self) {
        let events = self.scheduler.pop_due_events(self.current_cycle);

        for event in &events {
            self.tracer.set_cycle(self.current_cycle);
            self.tracer.trace_event(event);
            match event.kind {
                EventKind::PitTimer0 => {
                    let raise_irq = self.pit.on_timer0_event(
                        &mut self.scheduler,
                        self.clocks.cpu_clock_hz,
                        self.clocks.pit_clock_hz,
                        self.current_cycle,
                    );
                    if raise_irq {
                        self.pic.set_irq(0);
                        self.tracer.trace_irq_raise(0);
                    }
                }
                EventKind::GdcVsync => {
                    self.capture_vsync_snapshot();
                    self.tram_wait = 1;
                    self.vram_wait = 1;
                    self.grcg_wait = 1;
                    self.gdc_master.on_vsync_event();
                    self.gdc_slave.set_vsync(true);
                    if self.display_control.state.vsync_irq_enabled {
                        self.display_control.state.vsync_irq_enabled = false;
                        self.pic.set_irq(2);
                        self.tracer.trace_irq_raise(2);
                    }
                    self.scheduler.schedule(
                        EventKind::GdcDisplayStart,
                        self.current_cycle + self.gdc_master.state.vsync_blanking_period,
                    );
                }
                EventKind::GdcDisplayStart => {
                    self.tram_wait = TRAM_WAIT_CYCLES;
                    self.vram_wait = VRAM_WAIT_CYCLES;
                    self.grcg_wait = GRCG_WAIT_CYCLES;
                    self.gdc_master.set_vsync(false);
                    self.gdc_slave.set_vsync(false);
                    self.scheduler.schedule(
                        EventKind::GdcVsync,
                        self.current_cycle + self.gdc_master.state.display_period,
                    );
                }
                EventKind::FdcExecution => {
                    self.handle_fdc_execution();
                }
                EventKind::FdcInterrupt => {
                    self.handle_fdc_interrupt();
                }
                EventKind::GdcDrawingComplete => {
                    self.gdc_slave.on_drawing_complete();
                }
                EventKind::MouseTimer => {
                    if self.mouse_timer_irq_enabled() {
                        self.pic.set_irq(MOUSE_TIMER_IRQ_LINE);
                        self.tracer.trace_irq_raise(MOUSE_TIMER_IRQ_LINE);
                        self.schedule_mouse_timer();
                    }
                }
                EventKind::FmTimerA => {
                    if let Some(ref mut sb86) = self.soundboard_86 {
                        sb86.timer_expired(0, self.current_cycle);
                        self.process_soundboard_86_actions();
                    } else if let Some(ref mut sb26k) = self.soundboard_26k {
                        sb26k.timer_expired(0, self.current_cycle);
                        self.process_soundboard_actions();
                    }
                }
                EventKind::FmTimerB => {
                    if let Some(ref mut sb86) = self.soundboard_86 {
                        sb86.timer_expired(1, self.current_cycle);
                        self.process_soundboard_86_actions();
                    } else if let Some(ref mut sb26k) = self.soundboard_26k {
                        sb26k.timer_expired(1, self.current_cycle);
                        self.process_soundboard_actions();
                    }
                }
                EventKind::FmTimer2A => {
                    if let Some(ref mut sb26k) = self.soundboard_26k {
                        sb26k.timer_expired(0, self.current_cycle);
                        self.process_soundboard_actions();
                    }
                }
                EventKind::FmTimer2B => {
                    if let Some(ref mut sb26k) = self.soundboard_26k {
                        sb26k.timer_expired(1, self.current_cycle);
                        self.process_soundboard_actions();
                    }
                }
                EventKind::SasiExecution => {
                    self.handle_sasi_execution();
                }
                EventKind::SasiInterrupt => {
                    self.handle_sasi_interrupt();
                }
            }
        }
        self.update_next_event_cycle();
    }

    fn write_pit_counter(&mut self, channel: usize, value: u8) {
        if self.pit.write_counter(channel, value) {
            return;
        }
        self.pit.channels[channel].last_load_cycle = self.current_cycle;
        if channel == 1 {
            self.beeper
                .set_pit_reload(self.pit.channels[1].value, self.current_cycle);
        }
        if channel == 0 {
            self.pic.clear_irq(0);
            self.tracer.trace_irq_clear(0);
            self.pit.channels[0].flag |= PIT_FLAG_I;
            self.pit.schedule_timer0(
                &mut self.scheduler,
                self.clocks.cpu_clock_hz,
                self.clocks.pit_clock_hz,
                self.current_cycle,
            );
            self.update_next_event_cycle();
        }
    }

    fn write_pit_control(&mut self, value: u8) {
        let channel = ((value >> 6) & 3) as usize;
        if channel >= 3 {
            return;
        }
        self.pit.write_control(
            channel,
            value,
            self.current_cycle,
            self.clocks.cpu_clock_hz,
            self.clocks.pit_clock_hz,
        );
        if channel == 0 {
            self.pic.clear_irq(0);
            self.tracer.trace_irq_clear(0);
            if value & 0x30 != 0 {
                self.pit.channels[0].flag |= PIT_FLAG_I;
            }
        }
    }

    fn read_gdc_b_plane_word_from_access_page(&self, address: u32) -> u16 {
        let byte_offset = (address as usize & 0x3FFF) * 2;
        let page = self.access_page_index();
        let low = self.graphics_plane_read_byte_from_page(page, 0, byte_offset);
        let high = self.graphics_plane_read_byte_from_page(page, 0, byte_offset + 1);
        u16::from(low) | (u16::from(high) << 8)
    }

    fn write_gdc_b_plane_word_to_access_page(&mut self, address: u32, value: u16) {
        let byte_offset = (address as usize & 0x3FFF) * 2;
        let page = self.access_page_index();
        self.graphics_plane_write_byte_to_page(page, 0, byte_offset, value as u8);
        self.graphics_plane_write_byte_to_page(page, 0, byte_offset + 1, (value >> 8) as u8);
    }

    fn read_gdc_slave_dmar(&mut self) -> u8 {
        let vram_word = self
            .gdc_slave
            .dmar_next_address()
            .map(|address| self.read_gdc_b_plane_word_from_access_page(address));
        self.gdc_slave.dack_read(vram_word)
    }

    fn handle_gdc_slave_action(&mut self, action: GdcAction) {
        match action {
            GdcAction::None => {}
            GdcAction::Draw(result) => {
                for op in &result.writes {
                    self.apply_gdc_vram_op(op);
                }
                if result.dot_count > 0 {
                    self.schedule_drawing_timing(result.dot_count);
                } else {
                    self.gdc_slave.state.status &= !STATUS_DRAWING;
                }
            }
            GdcAction::ReadVram(_request) => {
                // Feed VRAM words to the GDC for RDAT.
                while let Some(address) = self.gdc_slave.rdat_next_address() {
                    let word = self.read_gdc_b_plane_word_from_access_page(address);
                    let needs_more = self.gdc_slave.provide_rdat_word(word);
                    if !needs_more {
                        break;
                    }
                    // Stop if FIFO is getting full (leave room for CPU reads).
                    if self.gdc_slave.state.fifo.count >= 14 {
                        break;
                    }
                }
            }
            GdcAction::TimingChanged => {}
        }
    }

    /// Converts between GDC bit ordering and CPU/VRAM bit ordering.
    /// The GDC places the leftmost pixel at bit 0 (LSB) within each byte,
    /// while the CPU/VRAM layout places the leftmost pixel at bit 7 (MSB).
    fn reverse_bits_in_bytes(word: u16) -> u16 {
        let lo = (word as u8).reverse_bits();
        let hi = ((word >> 8) as u8).reverse_bits();
        u16::from(lo) | (u16::from(hi) << 8)
    }

    fn apply_gdc_vram_op(&mut self, op: &VramOp) {
        if self.grcg.gdc_with_grcg_enabled() {
            if self.display_control.is_egc_extended_mode_effective() {
                self.apply_gdc_vram_op_egc(op);
                return;
            }
            self.apply_gdc_vram_op_grcg(op);
            return;
        }

        let current = self.read_gdc_b_plane_word_from_access_page(op.address);

        let mask = Self::reverse_bits_in_bytes(op.mask);
        let data = Self::reverse_bits_in_bytes(op.data);
        let result = match op.mode {
            0 => (current & !mask) | (data & mask),
            1 => current ^ (data & mask),
            2 => current & !(data & mask),
            3 => current | (data & mask),
            _ => current,
        };

        self.write_gdc_b_plane_word_to_access_page(op.address, result);
    }

    fn apply_gdc_vram_op_grcg(&mut self, op: &VramOp) {
        let byte_offset = (op.address as usize & 0x3FFF) * 2;
        let raw_mask = op.data & op.mask;
        if raw_mask == 0 {
            return;
        }
        let active_mask = Self::reverse_bits_in_bytes(raw_mask);

        for p in 0..4 {
            if p == 3 && !self.graphics_extension_enabled {
                continue;
            }
            let tile_word =
                u16::from(self.grcg.state.tile[p]) | (u16::from(self.grcg.state.tile[p]) << 8);

            if !self.grcg.is_rmw() {
                // TDW: write tile word directly, ignoring GDC mask/data/mode.
                self.graphics_plane_write_byte(p, byte_offset, tile_word as u8);
                self.graphics_plane_write_byte(p, byte_offset + 1, (tile_word >> 8) as u8);
            } else {
                // RMW: use active draw bits as selector, tile as color source.
                let current_low = self.graphics_plane_read_byte(p, byte_offset);
                let current_high = self.graphics_plane_read_byte(p, byte_offset + 1);
                let current = u16::from(current_low) | (u16::from(current_high) << 8);
                let result = (current & !active_mask) | (tile_word & active_mask);
                self.graphics_plane_write_byte(p, byte_offset, result as u8);
                self.graphics_plane_write_byte(p, byte_offset + 1, (result >> 8) as u8);
            }
        }
    }

    /// Routes a GDC VRAM operation through the EGC.
    /// Each per-dot VramOp is converted to a word-aligned EGC write with the pixel
    /// bit set in the corresponding position within the word.
    ///
    /// Uses the inner variant to avoid charging CPU wait cycles
    /// (GDC drawing has its own timing model.
    fn apply_gdc_vram_op_egc(&mut self, op: &VramOp) {
        let raw_mask = op.data & op.mask;
        if raw_mask == 0 {
            return;
        }
        let active_mask = Self::reverse_bits_in_bytes(raw_mask);
        // GDC word address → byte offset. Each GDC word is 2 bytes.
        let byte_offset = (op.address as usize & 0x3FFF) * 2;
        // Convert the 16-bit active mask to an EGC word write at the word-aligned address.
        // The address is already word-aligned (GDC addresses are word addresses).
        let addr = 0xA8000 + byte_offset as u32;
        self.egc_write_word_inner(addr, active_mask);
    }

    fn schedule_drawing_timing(&mut self, dot_count: u32) {
        let dots = dot_count as u64;
        let is_8mhz_lineage = self.clocks.pit_clock_hz == 1_996_800;
        let factor = if is_8mhz_lineage { 22464u64 } else { 27648u64 };
        let clk = dots * factor / 15625 + 30;
        self.gdc_slave.state.status |= STATUS_DRAWING;
        self.scheduler
            .schedule(EventKind::GdcDrawingComplete, self.current_cycle + clk);
        self.update_next_event_cycle();
    }

    /// Seek delay in CPU cycles (~500µs at 10 MHz = 5000 cycles).
    const SEEK_DELAY_CYCLES: u64 = 5000;
    /// Execution delay in CPU cycles (data ready after command).
    const EXECUTION_DELAY_CYCLES: u64 = 512;
    /// Interrupt delay after data transfer completes.
    const INTERRUPT_DELAY_CYCLES: u64 = 512;

    fn handle_fdc_action(&mut self, action: FdcAction, interface: u8) {
        match action {
            FdcAction::None => {}
            FdcAction::ScheduleSeekInterrupt => {
                {
                    let fdc = if interface == 0 {
                        self.floppy.fdc_1mb()
                    } else {
                        self.floppy.fdc_640k()
                    };
                    let drive = fdc.current_drive();
                    let cyl = fdc.state.drive_cylinder[drive];
                    let hd_us = fdc.state.hd_us;
                    self.tracer.trace_fdc_seek(drive, cyl, hd_us);
                }
                self.floppy.set_active_interface(interface);
                self.scheduler.schedule(
                    EventKind::FdcInterrupt,
                    self.current_cycle + Self::SEEK_DELAY_CYCLES,
                );
                self.update_next_event_cycle();
            }
            FdcAction::StartReadData
            | FdcAction::StartReadId
            | FdcAction::StartWriteData
            | FdcAction::StartFormatTrack => {
                self.floppy.set_active_interface(interface);
                self.scheduler.schedule(
                    EventKind::FdcExecution,
                    self.current_cycle + Self::EXECUTION_DELAY_CYCLES,
                );
                self.update_next_event_cycle();
            }
        }
    }

    fn handle_fdc_execution(&mut self) {
        let command = self.floppy.active_fdc().state.active_command;
        match command {
            FdcCommand::ReadData => self.handle_fdc_read_data(),
            FdcCommand::ReadId => self.handle_fdc_read_id(),
            FdcCommand::WriteData => self.handle_fdc_write_data(),
            FdcCommand::FormatTrack => self.handle_fdc_format_track(),
            FdcCommand::None => {}
        }
    }

    fn handle_fdc_read_data(&mut self) {
        let drive = self.floppy.active_fdc().current_drive();
        let track_index = self.floppy.active_fdc().current_track_index();
        {
            let fdc = self.floppy.active_fdc();
            self.tracer.trace_fdc_read(
                drive,
                track_index,
                fdc.state.c,
                fdc.state.h,
                fdc.state.r,
                fdc.state.n,
            );
        }

        if !self.floppy.has_drive(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(ST0_NOT_READY, 0x00, 0x00);
        } else if !self.floppy.density_matches(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(0x00, ST1_MISSING_ADDRESS_MARK, 0x00);
        } else {
            let mask_20bit = self.dma_access_ctrl & 0x04 != 0;
            let dma_channel = self.floppy.dma_channel();

            loop {
                let active_fdc = self.floppy.active_fdc_mut();
                let c = active_fdc.state.c;
                let h = active_fdc.state.h;
                let r = active_fdc.state.r;
                let n = active_fdc.state.n;

                let sector_data = self.floppy.read_sector_data(drive, track_index, c, h, r, n);

                match sector_data {
                    Some(data) => {
                        let dma_result = self.dma.transfer_write_to_memory(dma_channel, data);

                        for (addr, byte) in &dma_result.writes {
                            let addr = if mask_20bit { *addr & 0xF_FFFF } else { *addr };
                            self.memory.write_byte(addr, *byte);
                        }

                        let active_fdc = self.floppy.active_fdc_mut();

                        if dma_result.terminal_count {
                            active_fdc.signal_terminal_count();
                            active_fdc.advance_sector();
                            active_fdc.complete_success();
                            break;
                        }

                        let eot_reached = active_fdc.advance_sector();
                        if eot_reached {
                            active_fdc.complete_success();
                            break;
                        }
                    }
                    None => {
                        self.floppy.active_fdc_mut().complete_error(
                            0x00,
                            ST1_MISSING_ADDRESS_MARK,
                            0x00,
                        );
                        break;
                    }
                }
            }
        }

        self.scheduler.schedule(
            EventKind::FdcInterrupt,
            self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
        );
        self.update_next_event_cycle();
    }

    fn handle_fdc_read_id(&mut self) {
        let drive = self.floppy.active_fdc().current_drive();

        if !self.floppy.has_drive(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(ST0_NOT_READY, 0x00, 0x00);
        } else if !self.floppy.density_matches(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(0x00, ST1_MISSING_ADDRESS_MARK, 0x00);
        } else {
            let track_index = self.floppy.active_fdc().current_track_index();
            let crcn = self.floppy.active_fdc().state.crcn as usize;

            let sector_info = self.floppy.read_id_at_index(drive, track_index, crcn);

            match sector_info {
                Some((c, h, r, n)) => {
                    let sector_count = self.floppy.sector_count(drive, track_index);

                    let active_fdc = self.floppy.active_fdc_mut();

                    active_fdc.provide_read_id(c, h, r, n);

                    let next_crcn = if sector_count > 0 {
                        ((crcn + 1) % sector_count) as u8
                    } else {
                        0
                    };
                    active_fdc.state.crcn = next_crcn;
                    active_fdc.complete_success();
                }
                None => {
                    self.floppy.active_fdc_mut().complete_error(
                        0x00,
                        ST1_MISSING_ADDRESS_MARK,
                        0x00,
                    );
                }
            }
        }

        self.scheduler.schedule(
            EventKind::FdcInterrupt,
            self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
        );
        self.update_next_event_cycle();
    }

    fn handle_fdc_write_data(&mut self) {
        let drive = self.floppy.active_fdc().current_drive();
        let track_index = self.floppy.active_fdc().current_track_index();

        if !self.floppy.has_drive(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(ST0_NOT_READY, 0x00, 0x00);
        } else if self.floppy.is_write_protected(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(0x00, ST1_NOT_WRITABLE, 0x00);
        } else if !self.floppy.density_matches(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(0x00, ST1_MISSING_ADDRESS_MARK, 0x00);
        } else {
            let mask_20bit = self.dma_access_ctrl & 0x04 != 0;
            let dma_channel = self.floppy.dma_channel();

            loop {
                let active_fdc = self.floppy.active_fdc_mut();

                let c = active_fdc.state.c;
                let h = active_fdc.state.h;
                let r = active_fdc.state.r;
                let n = active_fdc.state.n;

                let sector_size = 128usize << (n as usize).min(7);

                let sector_exists = self
                    .floppy
                    .read_sector_data(drive, track_index, c, h, r, n)
                    .is_some();

                if !sector_exists {
                    self.floppy.active_fdc_mut().complete_error(
                        0x00,
                        ST1_MISSING_ADDRESS_MARK,
                        0x00,
                    );
                    break;
                }

                let dma_result = self.dma.transfer_read_from_memory(dma_channel, sector_size);

                let mut sector_data = Vec::with_capacity(dma_result.addresses.len());
                for &addr in &dma_result.addresses {
                    let addr = if mask_20bit { addr & 0xF_FFFF } else { addr };
                    sector_data.push(self.memory.read_byte(addr));
                }

                self.floppy
                    .write_sector_data(drive, track_index, c, h, r, n, &sector_data);

                let active_fdc = self.floppy.active_fdc_mut();

                if dma_result.terminal_count {
                    active_fdc.signal_terminal_count();
                    active_fdc.advance_sector();
                    active_fdc.complete_success();
                    break;
                }

                let eot_reached = active_fdc.advance_sector();
                if eot_reached {
                    active_fdc.complete_success();
                    break;
                }
            }
        }

        self.scheduler.schedule(
            EventKind::FdcInterrupt,
            self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
        );
        self.update_next_event_cycle();
    }

    fn handle_fdc_format_track(&mut self) {
        let drive = self.floppy.active_fdc().current_drive();
        let track_index = self.floppy.active_fdc().current_track_index();

        if !self.floppy.has_drive(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(ST0_NOT_READY, 0x00, 0x00);
        } else if self.floppy.is_write_protected(drive) {
            self.floppy
                .active_fdc_mut()
                .complete_error(0x00, ST1_NOT_WRITABLE, 0x00);
        } else {
            let mask_20bit = self.dma_access_ctrl & 0x04 != 0;
            let dma_channel = self.floppy.dma_channel();
            let data_n = self.floppy.active_fdc().state.n;
            let sector_count = self.floppy.active_fdc().state.eot as usize;
            let fill_byte = self.floppy.active_fdc().state.dtl;

            let mut chrn = Vec::with_capacity(sector_count);
            for _ in 0..sector_count {
                let dma_result = self.dma.transfer_read_from_memory(dma_channel, 4);
                let mut id = [0u8; 4];
                for (i, &addr) in dma_result.addresses.iter().enumerate() {
                    let addr = if mask_20bit { addr & 0xF_FFFF } else { addr };
                    id[i] = self.memory.read_byte(addr);
                }
                chrn.push((id[0], id[1], id[2], id[3]));
            }

            self.floppy
                .format_track(drive, track_index, &chrn, data_n, fill_byte);
            self.floppy.active_fdc_mut().complete_success();
        }

        self.scheduler.schedule(
            EventKind::FdcInterrupt,
            self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
        );
        self.update_next_event_cycle();
    }

    fn handle_fdc_interrupt(&mut self) {
        let irq = self.floppy.irq_line();
        if self.floppy.active_fdc_mut().take_interrupt_pending() {
            self.pic.set_irq(irq);
            self.tracer.trace_irq_raise(irq);
        }
    }

    fn process_sasi_action(&mut self, action: SasiAction) {
        match action {
            SasiAction::None => {}
            SasiAction::ScheduleCompletion | SasiAction::FormatTrack => {
                self.scheduler.schedule(
                    EventKind::SasiExecution,
                    self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
                );
                self.update_next_event_cycle();
            }
            SasiAction::DmaReady => {
                self.handle_sasi_dma();
            }
        }
    }

    fn handle_sasi_dma(&mut self) {
        let mask_20bit = self.dma_access_ctrl & 0x04 != 0;

        match self.sasi.phase() {
            SasiPhase::Read => loop {
                let (byte, action) = self.sasi.dma_read_byte();
                let dma_result = self.dma.transfer_write_to_memory(0, &[byte]);
                for (addr, b) in &dma_result.writes {
                    let addr = if mask_20bit { *addr & 0xF_FFFF } else { *addr };
                    self.memory.write_byte(addr, *b);
                }
                if matches!(action, SasiAction::ScheduleCompletion) || dma_result.terminal_count {
                    self.scheduler.schedule(
                        EventKind::SasiExecution,
                        self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
                    );
                    self.update_next_event_cycle();
                    break;
                }
                if !self.sasi.dma_ready() {
                    break;
                }
            },
            SasiPhase::Write => loop {
                let dma_result = self.dma.transfer_read_from_memory(0, 1);
                if dma_result.addresses.is_empty() {
                    break;
                }
                let addr = dma_result.addresses[0];
                let addr = if mask_20bit { addr & 0xF_FFFF } else { addr };
                let byte = self.memory.read_byte(addr);
                let action = self.sasi.dma_write_byte(byte);
                if matches!(action, SasiAction::ScheduleCompletion) {
                    self.scheduler.schedule(
                        EventKind::SasiExecution,
                        self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
                    );
                    self.update_next_event_cycle();
                    break;
                }
                if dma_result.terminal_count || !self.sasi.dma_ready() {
                    break;
                }
            },
            _ => {}
        }
    }

    fn handle_sasi_execution(&mut self) {
        let raise_irq = self.sasi.complete_operation();
        if raise_irq {
            self.scheduler.schedule(
                EventKind::SasiInterrupt,
                self.current_cycle + Self::INTERRUPT_DELAY_CYCLES,
            );
            self.update_next_event_cycle();
        }
    }

    fn handle_sasi_interrupt(&mut self) {
        // SASI uses IRQ 9 (slave PIC IRQ 1).
        self.pic.set_irq(9);
        self.tracer.trace_irq_raise(9);
    }

    /// Returns `true` if a SASI HLE trap is pending.
    pub fn sasi_hle_pending(&self) -> bool {
        self.sasi.hle_pending()
    }

    /// Returns `true` if a BIOS HLE trap is pending.
    pub fn bios_hle_pending(&self) -> bool {
        self.bios.hle_pending()
    }

    /// Executes the pending SASI HLE operation using the CPU's stack frame.
    ///
    /// The real BIOS pushes DS, SI, DI, ES, BP, DX, CX, BX, AX (9 words)
    /// before dispatching to the SASI ROM entry. The ROM triggers the trap
    /// via `OUT TRAP_PORT, AL`, then pops all registers and IRETs.
    /// The stack frame at SS:SP has:
    /// SP+0x00: AX, SP+0x02: BX, SP+0x04: CX, SP+0x06: DX, SP+0x08: BP,
    /// SP+0x0A: ES, SP+0x0C: DI, SP+0x0E: SI, SP+0x10: DS
    /// After these 9 words, the INT frame follows:
    /// SP+0x12: IP, SP+0x14: CS, SP+0x16: FLAGS
    pub fn execute_sasi_hle(&mut self, ss: u16, sp: u16) {
        let stack_base = (u32::from(ss) << 4).wrapping_add(u32::from(sp));

        let ax = self.read_word_direct(stack_base);
        let bx = self.read_word_direct(stack_base + 0x02);
        let cx = self.read_word_direct(stack_base + 0x04);
        let dx = self.read_word_direct(stack_base + 0x06);
        let bp = self.read_word_direct(stack_base + 0x08);
        let es = self.read_word_direct(stack_base + 0x0A);

        let function_code = (ax >> 8) as u8;
        let drive_select = ax as u8;
        let drive_idx = device::sasi::drive_index(drive_select);
        let function = function_code & 0x0F;

        let result_ah = match function {
            0x03 => {
                let current_lo = self.read_byte_with_access_page(0x055C);
                let current_hi = self.read_byte_with_access_page(0x055D);
                let current_equip = u16::from(current_lo) | (u16::from(current_hi) << 8);
                let disk_equip = self.sasi.execute_init(current_equip);
                self.write_byte_with_access_page(0x055C, disk_equip as u8);
                self.write_byte_with_access_page(0x055D, (disk_equip >> 8) as u8);

                // Unmask IRQ 0 (system timer). The real PC-98 BIOS calls
                // INT 1Ch AH=02 during init which unmasks the timer via
                // `pic.imr &= ~PIC_SYSTEMTIMER`.
                self.pic.state.chips[0].imr &= !0x01;

                0x00
            }
            0x04 => match function_code {
                // New Sense: returns geometry in AH/BX/CX/DX.
                0x84 => {
                    let sense_result = self.sasi.execute_sense(drive_idx);
                    if sense_result >= 0x20 {
                        sense_result
                    } else {
                        if let Some(geometry) = self.sasi.drive_geometry(drive_idx) {
                            let write_stack = |bus: &mut Self, offset: u32, value: u16| {
                                let addr = stack_base + offset;
                                bus.memory.write_byte(addr, value as u8);
                                bus.memory.write_byte(addr + 1, (value >> 8) as u8);
                            };
                            write_stack(self, 0x02, geometry.sector_size);
                            write_stack(self, 0x04, geometry.cylinders.saturating_sub(1));
                            let dx_value = ((u16::from(geometry.heads)) << 8)
                                | u16::from(geometry.sectors_per_track);
                            write_stack(self, 0x06, dx_value);
                        }
                        sense_result
                    }
                }
                _ => self.sasi.execute_sense(drive_idx),
            },
            0x05 => {
                let xfer = device::sasi::transfer_size(bx);
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = device::sasi::buffer_address(es, bp);
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &self.memory;
                self.sasi.execute_write(drive_idx, xfer, pos, addr, |a| {
                    let phys = bios::hle_page_translate(cr0, cr3, a, memory);
                    memory.read_byte(phys)
                })
            }
            0x06 => {
                let xfer = device::sasi::transfer_size(bx);
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                let addr = device::sasi::buffer_address(es, bp);
                let cr0 = self.hle_cr0;
                let cr3 = self.hle_cr3;
                let memory = &mut self.memory;
                self.sasi
                    .execute_read(drive_idx, xfer, pos, addr, |a, byte| {
                        let phys = bios::hle_page_translate(cr0, cr3, a, memory);
                        memory.write_byte(phys, byte);
                    })
            }
            0x07 | 0x0F => 0x00, // Retract: no-op
            0x0D => {
                let geometry = self.sasi.drive_geometry(drive_idx);
                let pos = geometry
                    .map(|g| device::sasi::sector_position(drive_select, cx, dx, &g))
                    .unwrap_or(0);
                self.sasi.execute_format(drive_idx, pos)
            }
            0x0E => {
                let mode_set_result = self.sasi.execute_mode_set(drive_idx);
                if mode_set_result == 0x00 {
                    self.apply_sasi_mode_set(drive_idx, function_code);
                }
                mode_set_result
            }
            0x01 => 0x00, // Verify: no-op
            _ => 0x40,    // Unsupported: Equipment Check error
        };

        self.tracer
            .trace_sasi_hle(function_code, drive_select, result_ah, bx, cx, dx, es, bp);

        // Write result AH back to stack (high byte of AX word at stack_base).
        self.memory.write_byte(stack_base + 1, result_ah);

        // Update FLAGS on the stack: set CF on error, clear on success.
        let flags_addr = stack_base + 0x16;
        let mut flags = self.read_word_direct(flags_addr);
        if result_ah >= 0x20 {
            flags |= 0x0001; // Set CF (error)
        } else {
            flags &= !0x0001; // Clear CF (success or informational)
        }
        self.memory.write_byte(flags_addr, flags as u8);
        self.memory.write_byte(flags_addr + 1, (flags >> 8) as u8);

        self.sasi.clear_hle_pending();
    }

    fn apply_sasi_mode_set(&mut self, drive_idx: usize, function_code: u8) {
        if let Some((offset, segment)) = self
            .sasi
            .mode_set_parameter_pointer(drive_idx, function_code)
        {
            let pointer_address = match drive_idx {
                0 => Some(0x05E8u32),
                1 => Some(0x05ECu32),
                _ => None,
            };

            if let Some(address) = pointer_address {
                self.write_byte_with_access_page(address, offset as u8);
                self.write_byte_with_access_page(address + 1, (offset >> 8) as u8);
                self.write_byte_with_access_page(address + 2, segment as u8);
                self.write_byte_with_access_page(address + 3, (segment >> 8) as u8);
            }
        }

        if drive_idx <= 1 {
            let flag_bit = 1u8 << drive_idx;
            let mode_is_half_height = function_code & 0x80 != 0;
            let sector_size = self
                .sasi
                .drive_geometry(drive_idx)
                .map(|geometry| geometry.sector_size)
                .unwrap_or(256);
            let mut mode_flags = self.read_byte_with_access_page(0x0481);

            // Full-height mode sets the compatibility bit for 512-byte sectors.
            if !mode_is_half_height && sector_size == 512 {
                mode_flags |= flag_bit;
            } else {
                mode_flags &= !flag_bit;
            }
            self.write_byte_with_access_page(0x0481, mode_flags);
        }
    }
}

impl<T: Tracing> common::Bus for Pc9801Bus<T> {
    fn read_byte(&mut self, address: u32) -> u8 {
        let address = self.a20_mask(address);
        let ems_b_bank = self.b_bank_ems
            && self.vram_ems_bank & 0x02 != 0
            && (0xB0000..=0xBFFFF).contains(&address);
        let in_grcg_range = !ems_b_bank
            && ((0xA8000..=0xBFFFF).contains(&address) || (0xE0000..=0xE7FFF).contains(&address));
        if self.grcg.is_active() && in_grcg_range {
            if self.is_egc_effective() {
                let value = self.egc_read_byte(address);
                self.tracer.trace_mem_read(address, value);
                return value;
            }
            let value = self.grcg_read_byte(address);
            self.tracer.trace_mem_read(address, value);
            return value;
        }
        if !ems_b_bank
            && ((0xA8000..=0xBFFFF).contains(&address) || (0xE0000..=0xE7FFF).contains(&address))
        {
            self.pending_wait_cycles += self.vram_wait;
        } else if (0xA0000..=0xA3FFF).contains(&address) {
            self.pending_wait_cycles += self.tram_wait;
        }
        let value = self.read_byte_with_access_page(address);
        self.tracer.trace_mem_read(address, value);
        value
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        let address = self.a20_mask(address);
        let ems_b_bank = self.b_bank_ems
            && self.vram_ems_bank & 0x02 != 0
            && (0xB0000..=0xBFFFF).contains(&address);
        let in_grcg_range = !ems_b_bank
            && ((0xA8000..=0xBFFFF).contains(&address) || (0xE0000..=0xE7FFF).contains(&address));
        if self.grcg.is_active() && in_grcg_range {
            if self.is_egc_effective() {
                self.egc_write_byte(address, value);
                self.tracer.trace_mem_write(address, value);
                return;
            }
            self.grcg_write_byte(address, value);
            self.tracer.trace_mem_write(address, value);
            return;
        }
        if !ems_b_bank
            && ((0xA8000..=0xBFFFF).contains(&address) || (0xE0000..=0xE7FFF).contains(&address))
        {
            self.pending_wait_cycles += self.vram_wait;
        } else if (0xA0000..=0xA3FFF).contains(&address) {
            self.pending_wait_cycles += self.tram_wait;
        }
        self.write_byte_with_access_page(address, value);
        self.tracer.trace_mem_write(address, value);
    }

    fn read_word(&mut self, address: u32) -> u16 {
        let address = self.a20_mask(address);
        let ems_b_bank = self.b_bank_ems
            && self.vram_ems_bank & 0x02 != 0
            && ((0xB0000..=0xBFFFF).contains(&address)
                || (0xB0000..=0xBFFFF).contains(&(address + 1)));
        let in_grcg_range = !ems_b_bank
            && ((0xA8000..=0xBFFFF).contains(&address) || (0xE0000..=0xE7FFF).contains(&address))
            && ((0xA8000..=0xBFFFF).contains(&(address + 1))
                || (0xE0000..=0xE7FFF).contains(&(address + 1)));
        if self.grcg.is_active() && in_grcg_range {
            if self.is_egc_effective() {
                let value = self.egc_read_word(address);
                self.tracer.trace_mem_read_word(address, value);
                return value;
            }
            let value = self.grcg_read_word(address);
            self.tracer.trace_mem_read_word(address, value);
            return value;
        }
        if in_grcg_range {
            self.pending_wait_cycles += self.vram_wait;
        } else if (0xA0000..=0xA3FFF).contains(&address)
            && (0xA0000..=0xA3FFF).contains(&(address + 1))
        {
            self.pending_wait_cycles += self.tram_wait;
        }
        let low = self.read_byte_with_access_page(address) as u16;
        let high = self.read_byte_with_access_page(address.wrapping_add(1)) as u16;
        let value = low | (high << 8);
        self.tracer.trace_mem_read_word(address, value);
        value
    }

    fn write_word(&mut self, address: u32, value: u16) {
        let address = self.a20_mask(address);
        let ems_b_bank = self.b_bank_ems
            && self.vram_ems_bank & 0x02 != 0
            && ((0xB0000..=0xBFFFF).contains(&address)
                || (0xB0000..=0xBFFFF).contains(&(address + 1)));
        let in_grcg_range = !ems_b_bank
            && ((0xA8000..=0xBFFFF).contains(&address) || (0xE0000..=0xE7FFF).contains(&address))
            && ((0xA8000..=0xBFFFF).contains(&(address + 1))
                || (0xE0000..=0xE7FFF).contains(&(address + 1)));
        if self.grcg.is_active() && in_grcg_range {
            if self.is_egc_effective() {
                self.egc_write_word(address, value);
                self.tracer.trace_mem_write_word(address, value);
                return;
            }
            self.grcg_write_word(address, value);
            self.tracer.trace_mem_write_word(address, value);
            return;
        }
        if in_grcg_range {
            self.pending_wait_cycles += self.vram_wait;
        } else if (0xA0000..=0xA3FFF).contains(&address)
            && (0xA0000..=0xA3FFF).contains(&(address + 1))
        {
            self.pending_wait_cycles += self.tram_wait;
        }
        self.write_byte_with_access_page(address, value as u8);
        self.write_byte_with_access_page(address.wrapping_add(1), (value >> 8) as u8);
        self.tracer.trace_mem_write_word(address, value);
    }

    fn io_read_byte(&mut self, port: u16) -> u8 {
        self.pending_wait_cycles += IO_WAIT_CYCLES;
        self.tracer.set_cycle(self.current_cycle);
        let value = match port {
            // PIC
            0x00 | 0x08 => self.pic.read_port0(((port >> 3) & 1) as usize),
            0x02 | 0x0A => self.pic.read_port2(((port >> 3) & 1) as usize),

            // DMA channel registers
            0x01 => self.dma.read_address(0),
            0x03 => self.dma.read_count(0),
            0x05 => self.dma.read_address(1),
            0x07 => self.dma.read_count(1),
            0x09 => self.dma.read_address(2),
            0x0B => self.dma.read_count(2),
            0x0D => self.dma.read_address(3),
            0x0F => self.dma.read_count(3),
            0x11 => self.dma.read_status(),
            // DMA write-only control registers return open bus on read.
            // Ref: undoc98 `io_dma.txt` (READ: 禁止 for these ports).
            0x13 | 0x15 | 0x17 | 0x19 | 0x1B | 0x1D | 0x1F | 0x29 => 0xFF,

            // DMA page registers are write-only; reads return open-bus.
            // Ref: undoc98 io_dma.txt: READ 禁止 for 0x21/0x23/0x25/0x27.
            0x21 | 0x23 | 0x25 | 0x27 => 0xFF,

            // Undocumented RTC control/mode latch.
            // Used by some TSRs/boot utilities during CPU probing.
            0x22 => self.rtc_control_22,

            // RS-232C i8251 data register.
            0x30 => {
                let (data, clear_irq, retrigger_irq) = self.serial.read_data();
                if clear_irq {
                    self.pic.clear_irq(4);
                }
                if retrigger_irq {
                    self.pic.set_irq(4);
                }
                data
            }
            // RS-232C i8251 status register.
            0x32 => self.serial.read_status(),

            // DIP switches and system ports
            0x31 => self.system_ppi.read_dip_switch_2(),
            0x33 => self.system_ppi.read_rs232c_status() | self.rtc.cdat(),
            0x35 => self.system_ppi.read_port_c(),

            // ARTIC timestamp (307.2 kHz, 24-bit).
            // 0x005D and 0x005E both expose bits 15-8, and 0x005F bits 23-16.
            0x005C => (self.artic_counter() & 0xFF) as u8,
            0x005D | 0x005E => ((self.artic_counter() >> 8) & 0xFF) as u8,
            0x005F => ((self.artic_counter() >> 16) & 0xFF) as u8,

            // Printer i8255 PPI Port A — data latch (read).
            0x40 => self.printer.read_data(),

            // i8255 PPI Port B — system configuration status (read-only).
            // BUSY# (bit 2) is composed from the printer device's ready state.
            0x42 => {
                let mut value = self.system_ppi.read_port_b();
                if self.printer.is_ready() {
                    value |= 0x04;
                }
                value
            }

            // Printer i8255 PPI Port C — printer control (read).
            0x44 => self.printer.read_port_c(),

            // Keyboard µPD8251A data register (port 0x41 read).
            0x41 => {
                let (data, clear_irq, retrigger_irq) = self.keyboard.read_data();
                if clear_irq {
                    self.pic.clear_irq(1);
                    self.tracer.trace_irq_clear(1);
                }
                if retrigger_irq {
                    self.pic.set_irq(1);
                    self.tracer.trace_irq_raise(1);
                }
                data
            }
            // Keyboard µPD8251A status register (port 0x43 read).
            0x43 => self.keyboard.read_status(),

            // GDC master (text)
            0x60 => self.gdc_master.read_status(),
            0x62 => self.gdc_master.read_data(),

            // Video mode
            0x68 => self.display_control.read_video_mode(),
            // Some software probes this odd-address alias as open bus.
            0x69 => 0xFF,

            // GRCG mode register read.
            // undoc98 io_disp.txt line 1118 says "READ: なし" (write-only).
            // MAME returns 0xFF. NP21W returns the actual mode register value.
            0x7C => self.grcg.state.mode,

            // PIT
            0x71 | 0x3FD9 => self.pit.read_counter(
                0,
                self.current_cycle,
                self.clocks.cpu_clock_hz,
                self.clocks.pit_clock_hz,
            ),
            0x73 | 0x3FDB => self.pit.read_counter(
                1,
                self.current_cycle,
                self.clocks.cpu_clock_hz,
                self.clocks.pit_clock_hz,
            ),
            0x75 | 0x3FDD => self.pit.read_counter(
                2,
                self.current_cycle,
                self.clocks.cpu_clock_hz,
                self.clocks.pit_clock_hz,
            ),

            // SASI hard disk controller
            0x80 => self.sasi.read_data(),
            0x82 => self.sasi.read_status(),

            // GDC slave (graphics)
            0xA0 => self.gdc_slave.read_status(),
            0xA2 => {
                if self.gdc_slave.state.dma_active && !self.gdc_slave.state.dma_is_write {
                    self.read_gdc_slave_dmar()
                } else {
                    self.gdc_slave.read_data()
                }
            }

            // CGROM glyph data read
            0xA9 => {
                if let Some(addr) = self
                    .cgrom
                    .read_address(self.display_control.is_kac_dot_access_mode())
                {
                    self.memory.font_read(addr)
                } else {
                    0
                }
            }

            // FDC uPD765A — 1MB interface (active when PORT EXC = 1).
            0x90 => {
                if self.floppy.port_exc_is_1mb() {
                    self.floppy.fdc_1mb_mut().read_status()
                } else {
                    0xFF
                }
            }
            0x92 => {
                if self.floppy.port_exc_is_1mb() {
                    self.floppy.fdc_1mb_mut().read_data()
                } else {
                    0xFF
                }
            }
            0x94 => {
                if self.floppy.port_exc_is_1mb() {
                    FDC_1MB_INPUT_REGISTER
                } else {
                    0xFF
                }
            }
            // FDC uPD765A — 640KB interface (active when PORT EXC = 0).
            0xC8 => {
                if !self.floppy.port_exc_is_1mb() {
                    self.floppy.fdc_640k_mut().read_status()
                } else {
                    0xFF
                }
            }
            0xCA => {
                if !self.floppy.port_exc_is_1mb() {
                    self.floppy.fdc_640k_mut().read_data()
                } else {
                    0xFF
                }
            }
            0xCC => {
                if !self.floppy.port_exc_is_1mb() {
                    FDC_640K_INPUT_REGISTER
                } else {
                    0xFF
                }
            }

            // VRAM drawing page select.
            0xA6 => self.display_control.read_access_page(),

            // Dual-mode FDC interface control. Bits 0-1 reflect effective mode
            // after hardware density detection override.
            0xBE => (self.floppy.effective_fdc_media() & 3) | FDC_MEDIA_READ_FIXED_BITS,

            // 26K alternate base status (dual-board mode: 26K at 0x0088).
            0x0088 => {
                if self.soundboard_86.is_some()
                    && let Some(soundboard_26k) = self.soundboard_26k.as_mut()
                {
                    let value = soundboard_26k.read_status(self.current_cycle);
                    self.process_soundboard_actions();
                    value
                } else {
                    0xFF
                }
            }
            // 26K alternate base data read (dual-board mode: 26K at 0x008A).
            0x008A => {
                if self.soundboard_26k.is_some() && self.soundboard_86.is_some() {
                    let sb26k = self.soundboard_26k.as_mut().unwrap();
                    if sb26k.address() == 0x0E {
                        let irq_bits = match sb26k.irq_line() {
                            3 => 0x00,
                            13 => 0x01,
                            10 => 0x02,
                            12 => 0x03,
                            _ => 0x03,
                        };
                        (irq_bits << 6) | 0x3F
                    } else {
                        let value = sb26k.read_data(self.current_cycle);
                        self.process_soundboard_actions();
                        value
                    }
                } else {
                    0xFF
                }
            }

            // FM sound board status (OPN / OPNA low bank).
            0x0188 => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    let value = sb86.read_status(self.current_cycle);
                    self.process_soundboard_86_actions();
                    value
                } else if let Some(ref mut sb26k) = self.soundboard_26k {
                    let value = sb26k.read_status(self.current_cycle);
                    self.process_soundboard_actions();
                    value
                } else {
                    0xFF
                }
            }
            // FM sound board data read (OPN / OPNA low bank).
            0x018A => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    let value = sb86.read_data(self.current_cycle);
                    self.process_soundboard_86_actions();
                    value
                } else if let Some(ref mut sb26k) = self.soundboard_26k {
                    if sb26k.address() == 0x0E {
                        let irq_bits = match sb26k.irq_line() {
                            3 => 0x00,
                            13 => 0x01,
                            10 => 0x02,
                            12 => 0x03,
                            _ => 0x03,
                        };
                        (irq_bits << 6) | 0x3F
                    } else {
                        let value = sb26k.read_data(self.current_cycle);
                        self.process_soundboard_actions();
                        value
                    }
                } else {
                    0xFF
                }
            }
            // OPNA extended status (high bank).
            0x018C => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    let value = sb86.read_status_hi(self.current_cycle);
                    self.process_soundboard_86_actions();
                    value
                } else {
                    0xFF
                }
            }
            // OPNA extended data read (high bank).
            0x018E => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.read_data_hi(self.current_cycle)
                } else {
                    0xFF
                }
            }

            // System status register (no sound board, no IDE).
            0xF0 => SYSTEM_STATUS_DEFAULT,
            // A20 gate state: 0xFF when enabled, 0xFE when disabled.
            0xF2 => 0xFF - (!self.a20_enabled as u8),
            // A20 line control status (386+ CPU port).
            // Bit 0: A20 mask state (1=masked), bit 1: NMI enable (1=enabled).
            0xF6 => {
                if self.machine_model.has_a20_nmi_port() {
                    (!self.a20_enabled as u8) | ((self.nmi_enabled as u8) << 1)
                } else {
                    0xFF
                }
            }

            // Hi-res/normal mode detection (bit 2: 1=normal, 0=hi-res).
            // All PC-9801 targets use normal mode (640x400/640x200).
            // Hi-res (1120x750) is only on PC-H98, PC-98XA/XL/RL, and some PC-9821 models.
            0x0431 => MODE_DETECT_NORMAL,

            // DMA access control (bit 2: mask DMA above 1MB).
            0x0439 => self.dma_access_ctrl,

            // Expansion-slot socket processing latch.
            // Ref: undoc98 `io_memsys.txt`: bits 3:0 = sockets B0h/A0h/90h/80h processing active.
            0x043A => {
                let value = self.external_interrupt_43a;
                debug!("Port 0x043A (expansion socket processing latch) read: {value:#04X}");
                value
            }

            // 15M hole control readback (stub — not yet implemented).
            0x043B => {
                warn!(
                    "unhandled read from 15M hole control port 0x043B (returning {:#04X})",
                    self.hole_15m_control
                );
                self.hole_15m_control
            }

            // ROM bank select readback (write-only, no read on 386).
            // On 486+ machines this returns cache hit status in bit 2;
            // for RA-class (386) it is unconnected.
            // Ref: undoc98 `io_mem.txt` lines 240-260.
            0x043D => 0xFF,

            // Port 0x043E: not a documented I/O port.
            // Some BIOS revisions probe it during POST; ignore silently.
            0x043E => {
                debug!("Silently ignore read to undocumented port 0x043E");
                0xFF
            }

            // Protected memory registration readback (VX/RA only).
            0x0567 => {
                if self.machine_model.has_protected_memory_register() {
                    self.protected_memory_max
                } else {
                    0xFF
                }
            }

            // SCSI controller status (no SCSI controller present).
            0x0CC4 => 0xFF,

            // Key-down sense probe latch.
            0x00EC => {
                let value = self.key_sense_0ec;
                debug!("Port 0x00EC (undocumented key-down sense latch) read: {value:#04X}");
                value
            }

            // PCM86 DAC ports.
            0xA460 | 0xA466 | 0xA468 | 0xA46A | 0xA46C | 0xA46E => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.pcm86_read(port, self.current_cycle, self.clocks.cpu_clock_hz)
                } else {
                    0xFF
                }
            }

            // Mouse interface i8255 PPI ports (read).
            0x7FD9 => self.mouse_ppi.read_port_a(self.current_cycle),
            0x7FDB => self.mouse_ppi.read_port_b(),
            0x7FDD => self.mouse_ppi.read_port_c(),
            // Mouse interrupt timer setting (write-only on PC-9801VM).
            0xBFDB => 0xFF,

            // Alternate FM sound board base addresses (0x0288, 0x0388).
            // Games probe these to detect sound hardware at non-primary bases.
            // No hardware present — return 0xFF silently.
            0x0288 | 0x028A | 0x028C | 0x028E | 0x0388 | 0x038A | 0x038C | 0x038E => 0xFF,

            _ => {
                self.tracer.trace_io_unhandled_read(port);
                warn!("Unhandled I/O read: port={port:#06X}");
                0xFF
            }
        };
        self.tracer.trace_io_read(port, value);
        value
    }

    fn io_write_byte(&mut self, port: u16, value: u8) {
        self.pending_wait_cycles += IO_WAIT_CYCLES;
        self.tracer.set_cycle(self.current_cycle);
        self.tracer.trace_io_write(port, value);
        match port {
            // PIC
            0x00 | 0x08 => self.pic.write_port0(((port >> 3) & 1) as usize, value),
            0x02 | 0x0A => self.pic.write_port2(((port >> 3) & 1) as usize, value),

            // DMA channel registers
            0x01 => self.dma.write_address(0, value),
            0x03 => self.dma.write_count(0, value),
            0x05 => self.dma.write_address(1, value),
            0x07 => self.dma.write_count(1, value),
            0x09 => self.dma.write_address(2, value),
            0x0B => self.dma.write_count(2, value),
            0x0D => self.dma.write_address(3, value),
            0x0F => self.dma.write_count(3, value),
            0x11 => self.dma.write_command(value),
            // DMA request register (software-triggered DMA, not used).
            0x13 => {}
            0x15 => self.dma.write_single_mask(value),
            0x17 => self.dma.write_mode(value),
            0x19 => self.dma.clear_flip_flop(),
            0x1B => self.dma.master_clear(),
            0x1D => self.dma.state.mask = 0,
            0x1F => self.dma.write_all_mask(value),
            0x21 => self.dma.write_page(1, value & 0x0F),
            0x23 => self.dma.write_page(2, value & 0x0F),
            0x25 => self.dma.write_page(3, value & 0x0F),
            0x27 => self.dma.write_page(0, value & 0x0F),
            // DMA auto-increment boundary mode register.
            0x29 => self.dma.write_bound(value),

            // Undocumented RTC control/mode latch.
            0x22 => {
                self.rtc_control_22 = value;
            }

            // System port C
            0x35 => {
                self.system_ppi.write_port_c(value);
                self.beeper
                    .set_buzzer_enabled(value & 0x08 == 0, self.current_cycle);
            }
            // i8255 PPI control port
            0x37 => {
                self.system_ppi.write_control(value);
                let enabled = self.system_ppi.state.port_c & 0x08 == 0;
                self.beeper.set_buzzer_enabled(enabled, self.current_cycle);
            }

            // RS-232C i8251 data register.
            0x30 => self.serial.write_data(value),
            // RS-232C i8251 mode/command register.
            0x32 => self.serial.write_command(value),

            // Printer i8255 PPI Port A — data latch (write).
            0x40 => self.printer.write_data(value),

            // Keyboard µPD8251A data register (port 0x41 write).
            0x41 => self.keyboard.write_data(value),
            // Keyboard µPD8251A control register (port 0x43 write).
            0x43 => self.keyboard.write_command(value),

            // Printer i8255 PPI Port C — printer control (write).
            0x44 => self.printer.write_port_c(value),
            // Printer i8255 PPI control register (write-only).
            0x46 => self.printer.write_control(value),

            // NMI control
            0x50 => self.nmi_enabled = false,
            0x52 => self.nmi_enabled = true,

            // GDC master (text)
            0x60 => {
                let action = self.gdc_master.write_data(value);
                if matches!(action, GdcAction::TimingChanged) {
                    self.reschedule_gdc_events();
                }
            }
            0x62 => {
                let action = self.gdc_master.write_command(value);
                if matches!(action, GdcAction::TimingChanged) {
                    self.reschedule_gdc_events();
                }
            }

            // VSYNC IRQ control and acknowledge
            0x64 => {
                self.display_control.write_vsync_control(value);
            }

            // Video mode
            0x68 => {
                self.display_control.write_video_mode(value);
                self.update_plane_e_mapping();
            }
            // Mode F/F odd-address alias: ignored.
            0x69 => {}
            // Mode register 2 (GDC clock + color depth + accelerator mode).
            0x6A => {
                let has_egc = self.grcg.state.chip == grcg::GRCG_CHIP_EGC;
                self.display_control.write_mode2(value, has_egc);
                self.update_plane_e_mapping();
                self.apply_gdc_dot_clock();
                self.reschedule_gdc_events();
            }
            // Border color.
            0x6C => self.display_control.write_border_color(value),
            // Display line count (CRT frequency select).
            0x6E => {
                self.display_control.write_display_line_count(value);
                self.apply_gdc_dot_clock();
                self.reschedule_gdc_events();
            }

            // CRTC uPD52611 text line counter registers.
            0x70 | 0x72 | 0x74 | 0x76 | 0x78 | 0x7A => {
                self.crtc
                    .write_register(((port - 0x70) >> 1) as usize, value);
            }

            // ARTIC hardware wait: `OUT 0x005F,AL` inserts >=0.6 us delay.
            0x005F => {
                self.pending_wait_cycles += self.artic_wait_cycles();
            }

            // PIT
            0x71 | 0x3FD9 => self.write_pit_counter(0, value),
            0x73 | 0x3FDB => self.write_pit_counter(1, value),
            0x75 | 0x3FDD => self.write_pit_counter(2, value),
            0x77 | 0x3FDF => self.write_pit_control(value),

            // GRCG mode register — resets tile counter.
            0x7C => self.grcg.write_mode(value),
            // GRCG tile register — cycles through planes 0-3.
            0x7E => self.grcg.write_tile(value),

            // SASI hard disk controller
            0x80 => {
                let action = self.sasi.write_data(value);
                self.process_sasi_action(action);
            }
            0x82 => {
                let action = self.sasi.write_control(value);
                self.process_sasi_action(action);
            }

            // FDC uPD765A — 1MB interface (active when PORT EXC = 1).
            0x92 => {
                if self.floppy.port_exc_is_1mb() {
                    let action = self.floppy.fdc_1mb_mut().write_data(value);
                    self.handle_fdc_action(action, 0);
                }
            }
            0x94 => {
                if self.floppy.port_exc_is_1mb() {
                    self.floppy.fdc_1mb_mut().write_control(value);
                }
            }
            // FDC uPD765A — 640KB interface (active when PORT EXC = 0).
            0xCA => {
                if !self.floppy.port_exc_is_1mb() {
                    let action = self.floppy.fdc_640k_mut().write_data(value);
                    self.handle_fdc_action(action, 1);
                }
            }
            0xCC => {
                if !self.floppy.port_exc_is_1mb() {
                    self.floppy.fdc_640k_mut().write_control(value);
                }
            }

            // GDC slave (graphics)
            0xA0 => {
                let action = self.gdc_slave.write_data(value);
                self.handle_gdc_slave_action(action);
            }
            0xA2 => {
                let action = self.gdc_slave.write_command(value);
                self.handle_gdc_slave_action(action);
            }
            // Graphics display page select.
            0xA4 => self.display_control.write_display_page(value),
            // VRAM drawing page select.
            0xA6 => self.display_control.write_access_page(value),
            // Palette registers (mode-dependent via mode2 bit 0).
            // 16-color analog: 0xA8=index select, 0xAA=green, 0xAC=red, 0xAE=blue.
            // 8-color digital: all 4 ports store packed nibble pairs directly.
            0xA8 => {
                if self.display_control.is_palette_analog_mode() {
                    self.palette.write_index(value);
                } else {
                    self.palette.write_digital(0, value);
                }
            }
            0xAA => {
                if self.display_control.is_palette_analog_mode() {
                    self.palette.write_analog(0, value);
                } else {
                    self.palette.write_digital(1, value);
                }
            }
            0xAC => {
                if self.display_control.is_palette_analog_mode() {
                    self.palette.write_analog(1, value);
                } else {
                    self.palette.write_digital(2, value);
                }
            }
            0xAE => {
                if self.display_control.is_palette_analog_mode() {
                    self.palette.write_analog(2, value);
                } else {
                    self.palette.write_digital(3, value);
                }
            }
            // CGROM character code high byte
            0xA1 => self.cgrom.write_code_high(value),
            // CGROM character code low byte
            0xA3 => self.cgrom.write_code_low(value),
            // CGROM line selector + left/right half
            0xA5 => self.cgrom.write_line_selector(value),
            // CGROM glyph data write (user-definable range only)
            0xA9 => {
                if let Some(addr) = self
                    .cgrom
                    .write_address(self.display_control.is_kac_dot_access_mode())
                {
                    self.memory.font_write(addr, value);
                }
            }

            // A20 gate disable + V30 CPU mode switch + CPU reset/shutdown.
            0xF0 => {
                self.a20_enabled = false;
                if self.machine_model == MachineModel::PC9801VM {
                    self.system_ppi.set_cpu_mode_bit(false);
                }

                let port_c = self.system_ppi.state.port_c;
                let shut0 = port_c & 0x80 != 0;
                let shut1 = port_c & 0x20 != 0;

                match (shut0, shut1) {
                    (true, false) => {
                        debug!("System shutdown (SHUT0=1, SHUT1=0)");
                        self.shutdown_requested = true;
                        self.reset_pending = true;
                    }
                    (false, _) => {
                        debug!("Warm reset (SHUT0=0)");
                        self.warm_reset_context = Some(self.read_warm_reset_context());
                        self.needs_full_reinit = true;
                        self.reset_pending = true;
                    }
                    (true, true) => {
                        debug!("Cold reset (SHUT0=1, SHUT1=1)");
                        self.warm_reset_context = None;
                        self.needs_full_reinit = true;
                        self.reset_pending = true;
                    }
                }
            }
            // Dual-mode FDC interface control.
            0xBE => {
                self.floppy.set_fdc_media(value);
            }

            // A20 gate enable + restore V30 native mode (no reset).
            0xF2 => {
                self.a20_enabled = true;
                if self.machine_model == MachineModel::PC9801VM {
                    self.system_ppi.set_cpu_mode_bit(true);
                }
            }
            // A20 line control + DIP switch bank select.
            0xF6 => {
                if self.machine_model.has_a20_nmi_port() {
                    match value {
                        0x02 => self.a20_enabled = true,
                        0x03 => self.a20_enabled = false,
                        _ => {}
                    }
                }
            }

            // 26K alternate base register select (dual-board mode: 26K at 0x0088).
            0x0088 => {
                if self.soundboard_26k.is_some() && self.soundboard_86.is_some() {
                    self.soundboard_26k
                        .as_mut()
                        .unwrap()
                        .write_address(value, self.current_cycle);
                    self.process_soundboard_actions();
                }
            }
            // 26K alternate base data write (dual-board mode: 26K at 0x008A).
            0x008A => {
                if self.soundboard_26k.is_some() && self.soundboard_86.is_some() {
                    self.soundboard_26k
                        .as_mut()
                        .unwrap()
                        .write_data(value, self.current_cycle);
                    self.process_soundboard_actions();
                }
            }

            // FM sound board register select (OPN / OPNA low bank).
            0x0188 => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.write_address(value, self.current_cycle);
                    self.process_soundboard_86_actions();
                } else if let Some(ref mut sb26k) = self.soundboard_26k {
                    sb26k.write_address(value, self.current_cycle);
                    self.process_soundboard_actions();
                }
            }
            // FM sound board data write (OPN / OPNA low bank).
            0x018A => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.write_data(value, self.current_cycle);
                    self.process_soundboard_86_actions();
                } else if let Some(ref mut sb26k) = self.soundboard_26k {
                    sb26k.write_data(value, self.current_cycle);
                    self.process_soundboard_actions();
                }
            }
            // OPNA extended register select (high bank).
            0x018C => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.write_address_hi(value, self.current_cycle);
                    self.process_soundboard_86_actions();
                }
            }
            // OPNA extended data write (high bank).
            0x018E => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.write_data_hi(value, self.current_cycle);
                    self.process_soundboard_86_actions();
                }
            }

            // DMA access control (bit 2: mask DMA above 1MB).
            0x0439 => {
                self.dma_access_ctrl = value;
            }
            // Expansion-slot socket processing latch.
            // Ref: undoc98 `io_memsys.txt`: bits 3:0 = sockets B0h/A0h/90h/80h processing active.
            0x043A => {
                debug!("Port 0x043A (expansion socket processing latch) write: {value:#04X}");
                self.external_interrupt_43a = value;
            }
            // 15M hole control (port 0x043B, stub — not yet implemented).
            0x043B => {
                warn!("unhandled write to 15M hole control port 0x043B: {value:#04X}");
                self.hole_15m_control = value;
            }
            // ROM bank select (port 0x043D).
            // 0x10/0x00/0x18 = ITF bank, 0x12 = BIOS bank.
            0x043D => {
                if self.machine_model.is_dual_bank_bios() {
                    match value {
                        0x00 | 0x10 | 0x18 => self.memory.select_banked_rom_window(false),
                        0x12 => self.memory.select_banked_rom_window(true),
                        _ => {}
                    }
                }
            }
            // Port 0x043E: not a documented I/O port. Ignore writes silently.
            0x043E => {}
            // VRAM/EMS banking.
            0x043F => {
                self.vram_ems_bank = value;
            }
            // RAM window.
            0x0461 => {
                self.ram_window = value;
            }
            // Shadow RAM control.
            0x053D => {
                if self.machine_model.has_shadow_ram() {
                    self.memory.set_shadow_control(value);
                }
            }
            // Protected memory registration.
            0x0567 => {
                if self.machine_model.has_protected_memory_register() {
                    self.protected_memory_max = value;
                }
            }

            // SASI HLE trap port.
            0x07EF => {
                self.sasi.write_trap_port(value);
            }

            // BIOS HLE trap port.
            0x07F0 => {
                self.bios.write_trap_port(value);
                // TODO: Calibrate these by comparing against the real VM target BIOS calls,
                //       once we have a verified V30 cycle accurate core.
                //       There are some VM era games, like Dragon Knight that will get
                //       audio problems when 0x18 is not tuned correctly for example.
                let cost = match self.bios.pending_vector() {
                    0x09 | 0x0C | 0x12 | 0x13 => 50,
                    0x18 => 20,
                    0x19 | 0x1A | 0x1B | 0x1C | 0x1F => 20,
                    0x08 => 20,
                    0xF2 => 1000,
                    _ => 0,
                };
                self.pending_wait_cycles += cost;
            }

            // Hi-res mode notification to expansion boards (write-only).
            // Bit 1: 1 = hi-res mode, 0 = normal mode. PC-9801VM is always normal mode.
            0x0467 => {}
            // Mouse interface i8255 PPI ports (write).
            0x7FD9 => self.mouse_ppi.write_port_a(value),
            0x7FDB => self.mouse_ppi.write_port_b(value),
            0x7FDD => {
                let was_enabled = self.mouse_timer_irq_enabled();
                let hc_rising = self.mouse_ppi.write_port_c(value);
                if hc_rising {
                    self.mouse_ppi.latch(self.current_cycle);
                }
                self.handle_mouse_timer_control_change(was_enabled);
            }
            0x7FDF => {
                let was_enabled = self.mouse_timer_irq_enabled();
                let hc_rising = self.mouse_ppi.write_ctrl(value);
                if hc_rising {
                    self.mouse_ppi.latch(self.current_cycle);
                }
                self.handle_mouse_timer_control_change(was_enabled);
            }
            // Mouse interrupt timer setting.
            // Only bits 1:0 select frequency. Ignore writes with upper bits set
            // (games like jastrike write non-frequency values to this port).
            0xBFDB => {
                if value & 0xFC != 0 {
                    return;
                }
                self.mouse_timer_setting = value;
                if self.mouse_timer_irq_enabled() {
                    self.schedule_mouse_timer();
                    self.update_next_event_cycle();
                }
            }

            // Key-down sense probe latch.
            0x00EC => {
                debug!("Port 0x00EC (undocumented key-down sense latch) write: {value:#04X}");
                self.key_sense_0ec = value;
            }

            // µPD4990A RTC strobe/command (port 0x20 write).
            0x20 => {
                let host_time = (self.host_local_time_fn)();
                self.rtc.write_port(value, &host_time);
            }

            // PCM86 DAC ports.
            0xA460 | 0xA466 | 0xA468 | 0xA46A | 0xA46C | 0xA46E => {
                if let Some(ref mut sb86) = self.soundboard_86 {
                    sb86.pcm86_write(port, value, self.current_cycle, self.clocks.cpu_clock_hz);
                }
            }

            // EGC registers (byte access).
            0x04A0..=0x04AF => {
                if self.machine_model.has_egc()
                    && self.display_control.is_egc_extended_mode_effective()
                    && self.grcg.is_active()
                {
                    self.egc.write_register_byte((port & 0x0F) as u8, value);
                }
            }

            // DMA extended bank registers (A24-A31, 386+ only).
            0x0E05 | 0x0E07 | 0x0E09 | 0x0E0B => {
                if self.machine_model.has_extended_dma() {
                    let channel = ((port - 0x0E05) / 2) as usize;
                    self.dma.write_extended_page(channel, value);
                }
            }

            // Alternate FM sound board base addresses — silently ignore writes.
            0x0288 | 0x028A | 0x028C | 0x028E | 0x0388 | 0x038A | 0x038C | 0x038E => {}

            _ => {
                self.tracer.trace_io_unhandled_write(port, value);
                warn!("Unhandled I/O write: port={port:#06X} value={value:#04X}");
            }
        }
    }

    fn io_write_word(&mut self, port: u16, value: u16) {
        match port {
            // EGC registers: atomic word write avoids double recalculate_shift()
            // that the default byte-split path would cause on shift (0x04AC) and
            // length (0x04AE) registers.
            0x04A0..=0x04AE if port & 1 == 0 => {
                self.pending_wait_cycles += IO_WAIT_CYCLES;
                self.tracer.set_cycle(self.current_cycle);
                if self.machine_model.has_egc()
                    && self.display_control.is_egc_extended_mode_effective()
                    && self.grcg.is_active()
                {
                    self.egc.write_register_word((port & 0x0F) as u8, value);
                }
            }
            _ => {
                self.io_write_byte(port, value as u8);
                self.io_write_byte(port.wrapping_add(1), (value >> 8) as u8);
            }
        }
    }

    fn has_irq(&self) -> bool {
        self.pic.has_pending_irq()
    }

    fn acknowledge_irq(&mut self) -> u8 {
        let vector = self.pic.acknowledge();
        let irq = vector.wrapping_sub(self.pic.state.chips[0].icw[1]);
        self.tracer.set_cycle(self.current_cycle);
        self.tracer.trace_irq_acknowledge(irq, vector);
        vector
    }

    fn has_nmi(&self) -> bool {
        false
    }

    fn acknowledge_nmi(&mut self) {}

    fn current_cycle(&self) -> u64 {
        self.current_cycle
    }

    fn set_current_cycle(&mut self, cycle: u64) {
        self.current_cycle = cycle;
        if cycle >= self.next_event_cycle {
            self.process_events();
        }
    }

    fn drain_wait_cycles(&mut self) -> i64 {
        let cycles = self.pending_wait_cycles;
        self.pending_wait_cycles = 0;
        cycles
    }

    fn reset_pending(&self) -> bool {
        self.reset_pending
    }

    fn cpu_should_yield(&self) -> bool {
        self.sasi.take_yield_requested() || self.bios.take_yield_requested()
    }
}

#[cfg(test)]
mod tests {
    use common::{Bus, MachineModel};

    use super::{DOT_CLOCK_200LINE, DOT_CLOCK_400LINE, NoTracing, Pc9801Bus, VramOp};

    fn compose_halfwidth_font_address(
        video_mode: u8,
        attr_byte: u8,
        char_low: u8,
        glyph_y_16: u32,
    ) -> u32 {
        let font_select_8x16 = (video_mode & 0x08) != 0;
        let attr_semigraphics_mode = (video_mode & 0x01) != 0;
        let semigraphics = attr_semigraphics_mode && ((attr_byte & 0x10) != 0);

        if font_select_8x16 {
            let mut font_base = 0x80000 + u32::from(char_low) * 16 + glyph_y_16;
            if semigraphics {
                font_base += 0x1000;
            }
            font_base
        } else {
            let font_line = glyph_y_16 / 2;
            let mut font_base = 0x82000 + u32::from(char_low) * 16 + font_line;
            if semigraphics {
                font_base += 8;
            }
            font_base
        }
    }

    #[test]
    fn capture_vsync_snapshot_populates_typed_fields() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        bus.palette.state.analog[1] = [0x0A, 0x02, 0x0F];
        bus.gdc_master.state.pitch = 80;
        bus.gdc_master.state.blink_counter = 64;
        bus.gdc_master.state.scroll[0].start_address = 0x1234;
        bus.gdc_master.state.scroll[0].line_count = 0x00AB;
        bus.memory.state.text_vram[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        bus.memory.state.text_vram[0x1234..0x1238].copy_from_slice(&[9, 10, 11, 12]);
        bus.memory.state.text_vram[0x3FFC..0x4000].copy_from_slice(&[13, 14, 15, 16]);

        bus.gdc_slave.state.scroll[0].start_address = 0x5678;
        bus.gdc_slave.state.scroll[0].line_count = 0x00CD;
        bus.gdc_slave.state.pitch = 40;
        bus.display_control.state.video_mode = 0x1C;

        bus.memory.state.graphics_vram[0] = 0xAA;
        bus.memory.state.graphics_vram[0x8000] = 0xBB;
        bus.memory.state.graphics_vram[0x10000] = 0xCC;
        bus.memory.state.e_plane_vram[0] = 0xDD;

        bus.capture_vsync_snapshot();
        let snapshot = bus.vsync_snapshot();

        assert_eq!(snapshot.palette_rgba[1], 0xFFFF_AA22);
        assert_eq!(snapshot.display_flags, 0b0111);
        assert_eq!(snapshot.gdc_text_pitch, 80);
        assert_eq!(snapshot.gdc_scroll_start_line[0], 0x00AB_1234);
        assert_eq!(snapshot.gdc_graphics_scroll[0], 0x00CD_5678);
        assert_eq!(snapshot.gdc_graphics_pitch, 40);
        assert_eq!(snapshot.video_mode, 0x1C);
        assert_eq!(
            snapshot.text_vram_words[0],
            u32::from_le_bytes([1, 2, 3, 4])
        );
        assert_eq!(
            snapshot.text_vram_words[1],
            u32::from_le_bytes([5, 6, 7, 8])
        );
        assert_eq!(
            snapshot.text_vram_words[0x1234 / 4],
            u32::from_le_bytes([9, 10, 11, 12])
        );
        assert_eq!(
            snapshot.text_vram_words[0x3FFC / 4],
            u32::from_le_bytes([13, 14, 15, 16])
        );
        assert_eq!(snapshot.graphics_b_plane[0] & 0xFF, 0xAA);
        assert_eq!(snapshot.graphics_r_plane[0] & 0xFF, 0xBB);
        assert_eq!(snapshot.graphics_g_plane[0] & 0xFF, 0xCC);
        assert_eq!(snapshot.graphics_e_plane[0] & 0xFF, 0xDD);
    }

    #[test]
    fn capture_vsync_snapshot_sets_kanji_high_mask_from_kac_mode() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        bus.capture_vsync_snapshot();
        assert_eq!(bus.vsync_snapshot().gdc_text_kanji_high_mask, 0xFF);

        // Set mode1 bit 5 (KAC dot-access mode).
        bus.io_write_byte(0x68, 0x0B);
        bus.capture_vsync_snapshot();
        assert_eq!(bus.vsync_snapshot().gdc_text_kanji_high_mask, 0x00);

        // Clear mode1 bit 5 (KAC code-access mode).
        bus.io_write_byte(0x68, 0x0A);
        bus.capture_vsync_snapshot();
        assert_eq!(bus.vsync_snapshot().gdc_text_kanji_high_mask, 0xFF);
    }

    #[test]
    fn compose_halfwidth_font_address_follows_8x16_6x8_and_attr_bit0_contract() {
        let char_code = 0x34;
        let glyph_y = 13;
        let attr_with_bit4 = 0x10;
        let attr_without_bit4 = 0x00;

        // 8x16 mode, attr semigraphics disabled: bit4 is vertical-line attribute, not semigraphics.
        assert_eq!(
            compose_halfwidth_font_address(0x08, attr_with_bit4, char_code, glyph_y),
            0x80000 + 0x34 * 16 + 13
        );
        assert_eq!(
            compose_halfwidth_font_address(0x08, attr_without_bit4, char_code, glyph_y),
            0x80000 + 0x34 * 16 + 13
        );

        // 8x16 mode, attr semigraphics enabled: bit4 selects chargraph16 bank.
        assert_eq!(
            compose_halfwidth_font_address(0x09, attr_with_bit4, char_code, glyph_y),
            0x81000 + 0x34 * 16 + 13
        );
        assert_eq!(
            compose_halfwidth_font_address(0x09, attr_without_bit4, char_code, glyph_y),
            0x80000 + 0x34 * 16 + 13
        );

        // 6x8 mode halves glyph line index.
        assert_eq!(
            compose_halfwidth_font_address(0x00, attr_with_bit4, char_code, glyph_y),
            0x82000 + 0x34 * 16 + 6
        );
        assert_eq!(
            compose_halfwidth_font_address(0x01, attr_with_bit4, char_code, glyph_y),
            0x82000 + 0x34 * 16 + 8 + 6
        );
    }

    #[test]
    fn mode2_and_line_count_update_both_gdc_dot_clocks() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        // Boot state has display_line_count=0x01 (400-line dot clock).
        assert_eq!(bus.gdc_master.state.dot_clock_hz, DOT_CLOCK_400LINE);
        assert_eq!(bus.gdc_slave.state.dot_clock_hz, DOT_CLOCK_400LINE);

        // Switch to 200-line base clock.
        bus.io_write_byte(0x6E, 0x00);
        assert_eq!(bus.gdc_master.state.dot_clock_hz, DOT_CLOCK_200LINE);
        assert_eq!(bus.gdc_slave.state.dot_clock_hz, DOT_CLOCK_200LINE);

        // Back to 400-line base clock.
        bus.io_write_byte(0x6E, 0x01);
        assert_eq!(bus.gdc_master.state.dot_clock_hz, DOT_CLOCK_400LINE);
        assert_eq!(bus.gdc_slave.state.dot_clock_hz, DOT_CLOCK_400LINE);

        // Only one 5MHz bit set: still base clock.
        bus.io_write_byte(0x6A, 0x83);
        assert_eq!(bus.gdc_master.state.dot_clock_hz, DOT_CLOCK_400LINE);
        assert_eq!(bus.gdc_slave.state.dot_clock_hz, DOT_CLOCK_400LINE);

        // Both 5MHz bits set: doubled clock.
        bus.io_write_byte(0x6A, 0x85);
        assert_eq!(bus.gdc_master.state.dot_clock_hz, DOT_CLOCK_400LINE * 2);
        assert_eq!(bus.gdc_slave.state.dot_clock_hz, DOT_CLOCK_400LINE * 2);
    }

    #[test]
    fn gdc_grcg_tdw_ignores_pattern_off_writes() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);

        bus.grcg.write_mode(0x80);
        bus.grcg.state.tile = [0x5A, 0xA5, 0x3C, 0xC3];

        bus.memory.state.graphics_vram[0] = 0x11;
        bus.memory.state.graphics_vram[1] = 0x22;

        bus.apply_gdc_vram_op(&VramOp {
            address: 0,
            data: 0x0000,
            mask: 0x0080,
            mode: 0,
        });

        assert_eq!(bus.memory.state.graphics_vram[0], 0x11);
        assert_eq!(bus.memory.state.graphics_vram[1], 0x22);

        bus.apply_gdc_vram_op(&VramOp {
            address: 0,
            data: 0x0080,
            mask: 0x0080,
            mode: 0,
        });

        assert_eq!(bus.memory.state.graphics_vram[0], 0x5A);
        assert_eq!(bus.memory.state.graphics_vram[1], 0x5A);
    }

    #[test]
    fn gdc_grcg_rmw_uses_active_mask_bits_only() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);

        bus.grcg.write_mode(0xC0);
        bus.grcg.state.tile = [0xFF, 0x00, 0x00, 0x00];

        bus.memory.state.graphics_vram[0] = 0xAA;
        bus.memory.state.graphics_vram[1] = 0xAA;

        bus.apply_gdc_vram_op(&VramOp {
            address: 0,
            data: 0x0000,
            mask: 0x00FF,
            mode: 0,
        });

        assert_eq!(bus.memory.state.graphics_vram[0], 0xAA);
        assert_eq!(bus.memory.state.graphics_vram[1], 0xAA);

        bus.apply_gdc_vram_op(&VramOp {
            address: 0,
            data: 0x00F0,
            mask: 0x00FF,
            mode: 0,
        });

        // raw_mask = 0x00F0, after bit reversal within bytes = 0x000F
        // Plane 0: (0xAA & !0x0F) | (0xFF & 0x0F) = 0xA0 | 0x0F = 0xAF
        assert_eq!(bus.memory.state.graphics_vram[0], 0xAF);
        assert_eq!(bus.memory.state.graphics_vram[1], 0xAA);
    }

    /// Helper: enable EGC extended mode via port 0x6A (mode2 bit3=permission, bit2=EGC).
    fn enable_egc_mode(bus: &mut Pc9801Bus<NoTracing>) {
        bus.io_write_byte(0x6A, 0x07); // set bit3 (permission)
        bus.io_write_byte(0x6A, 0x05); // set bit2 (EGC extended mode)
    }

    #[test]
    fn egc_io_write_word_sets_register_atomically() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        enable_egc_mode(&mut bus);
        bus.grcg.write_mode(0x80); // activate GRCG

        bus.io_write_word(0x04AC, 0x1033);
        assert_eq!(bus.egc.state.sft, 0x1033);

        bus.io_write_word(0x04A8, 0xAAAA);
        assert_eq!(bus.egc.state.mask, 0xAAAA);
    }

    #[test]
    fn egc_io_write_word_noop_when_egc_inactive() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);
        let old_sft = bus.egc.state.sft;
        bus.io_write_word(0x04AC, 0x1033);
        assert_eq!(bus.egc.state.sft, old_sft);
    }

    #[test]
    fn grcg_tdw_write_byte_intercepts_e_plane() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        bus.set_graphics_extension_enabled(true);
        bus.grcg.write_mode(0x80); // TDW mode, all planes enabled

        bus.grcg.state.tile = [0x5A, 0xA5, 0x3C, 0xC3];

        // Write to E-plane address 0xE0000 (offset 0).
        bus.write_byte(0xE0000, 0xFF);

        // GRCG TDW should write tile values to all 4 planes at offset 0.
        assert_eq!(bus.memory.state.graphics_vram[0], 0x5A); // B-plane
        assert_eq!(bus.memory.state.graphics_vram[0x8000], 0xA5); // R-plane
        assert_eq!(bus.memory.state.graphics_vram[0x10000], 0x3C); // G-plane
        assert_eq!(bus.memory.state.e_plane_vram[0], 0xC3); // E-plane
    }

    #[test]
    fn grcg_tcr_read_byte_intercepts_e_plane() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        bus.set_graphics_extension_enabled(true);
        bus.grcg.write_mode(0x80); // TDW/TCR mode, all planes enabled

        // Set tile (compare) values and VRAM content to match.
        bus.grcg.state.tile = [0xAA, 0xBB, 0xCC, 0xDD];
        bus.memory.state.graphics_vram[0x100] = 0xAA; // B
        bus.memory.state.graphics_vram[0x8100] = 0xBB; // R
        bus.memory.state.graphics_vram[0x10100] = 0xCC; // G
        bus.memory.state.e_plane_vram[0x100] = 0xDD; // E

        // TCR read from E-plane address at offset 0x100.
        let result = bus.read_byte(0xE0100);
        assert_eq!(result, 0xFF, "all bits match tile values");
    }

    #[test]
    fn grcg_tdw_write_word_intercepts_e_plane() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        bus.set_graphics_extension_enabled(true);
        bus.grcg.write_mode(0x80); // TDW mode
        bus.grcg.state.tile = [0x11, 0x22, 0x33, 0x44];

        bus.write_word(0xE0000, 0xFFFF);

        assert_eq!(bus.memory.state.graphics_vram[0], 0x11);
        assert_eq!(bus.memory.state.graphics_vram[1], 0x11);
        assert_eq!(bus.memory.state.graphics_vram[0x8000], 0x22);
        assert_eq!(bus.memory.state.graphics_vram[0x8001], 0x22);
        assert_eq!(bus.memory.state.graphics_vram[0x10000], 0x33);
        assert_eq!(bus.memory.state.graphics_vram[0x10001], 0x33);
        assert_eq!(bus.memory.state.e_plane_vram[0], 0x44);
        assert_eq!(bus.memory.state.e_plane_vram[1], 0x44);
    }

    #[test]
    fn egc_aligned_word_write_charges_one_grcg_wait() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        enable_egc_mode(&mut bus);
        bus.grcg.write_mode(0x80);

        bus.pending_wait_cycles = 0;
        bus.egc_write_word(0xA8000, 0x1234);
        assert_eq!(bus.pending_wait_cycles, 8);
    }

    #[test]
    fn egc_misaligned_word_write_charges_one_grcg_wait() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        enable_egc_mode(&mut bus);
        bus.grcg.write_mode(0x80);

        bus.pending_wait_cycles = 0;
        bus.egc_write_word(0xA8001, 0x1234); // misaligned
        assert_eq!(
            bus.pending_wait_cycles, 8,
            "misaligned should charge exactly 1x grcg_wait"
        );
    }

    #[test]
    fn egc_misaligned_word_read_charges_one_grcg_wait() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        enable_egc_mode(&mut bus);
        bus.grcg.write_mode(0x80);

        bus.pending_wait_cycles = 0;
        let _ = bus.egc_read_word(0xA8001); // misaligned
        assert_eq!(
            bus.pending_wait_cycles, 8,
            "misaligned should charge exactly 1x grcg_wait"
        );
    }

    #[test]
    fn gdc_grcg_tdw_writes_all_planes_regardless_of_plane_enable() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        // TDW mode with only planes 0,2 enabled (bits 1,3 set = planes 1,3 disabled).
        bus.grcg.write_mode(0x8A);
        bus.grcg.state.tile = [0x11, 0x22, 0x33, 0x44];

        bus.apply_gdc_vram_op(&VramOp {
            address: 0,
            data: 0x0080,
            mask: 0x0080,
            mode: 0,
        });

        // All planes should be written despite plane-enable bits.
        assert_eq!(bus.memory.state.graphics_vram[0], 0x11); // B (plane 0)
        assert_eq!(bus.memory.state.graphics_vram[0x8000], 0x22); // R (plane 1, "disabled")
        assert_eq!(bus.memory.state.graphics_vram[0x10000], 0x33); // G (plane 2)
    }

    #[test]
    fn port_7c_read_returns_grcg_mode_register() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);

        bus.grcg.write_mode(0xCA);
        let value = bus.io_read_byte(0x7C);
        assert_eq!(value, 0xCA);

        bus.grcg.write_mode(0x80);
        let value = bus.io_read_byte(0x7C);
        assert_eq!(value, 0x80);
    }

    #[test]
    fn gdc_egc_write_does_not_charge_cpu_wait() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        enable_egc_mode(&mut bus);
        bus.grcg.write_mode(0x80);

        bus.pending_wait_cycles = 0;
        bus.apply_gdc_vram_op(&VramOp {
            address: 0,
            data: 0x0080,
            mask: 0x0080,
            mode: 0,
        });
        assert_eq!(
            bus.pending_wait_cycles, 0,
            "GDC-EGC should not charge CPU wait"
        );
    }

    #[test]
    fn e_plane_read_charges_vram_wait() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);
        bus.set_graphics_extension_enabled(true);

        bus.pending_wait_cycles = 0;
        let _ = bus.read_byte(0xE0000);
        assert!(
            bus.pending_wait_cycles > 0,
            "E-plane read should charge vram_wait"
        );
    }

    #[test]
    fn e_plane_write_charges_vram_wait() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);
        bus.set_graphics_extension_enabled(true);

        bus.pending_wait_cycles = 0;
        bus.write_byte(0xE0000, 0x42);
        assert!(
            bus.pending_wait_cycles > 0,
            "E-plane write should charge vram_wait"
        );
    }

    #[test]
    fn access_page_selects_vram_bank_for_cpu_writes() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        // Write to page 0 (default).
        bus.write_byte(0xA8000, 0xAA);
        assert_eq!(bus.memory.state.graphics_vram[0], 0xAA);

        // Switch to page 1 via port 0xA6.
        bus.io_write_byte(0xA6, 0x01);
        bus.write_byte(0xA8000, 0xBB);

        // Page 0 unchanged, page 1 written.
        assert_eq!(bus.memory.state.graphics_vram[0], 0xAA);
        let page1_base = super::GRAPHICS_PAGE_SIZE_BYTES;
        assert_eq!(bus.memory.state.graphics_vram[page1_base], 0xBB);
    }

    #[test]
    fn access_page_selects_vram_bank_for_cpu_reads() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        let page1_base = super::GRAPHICS_PAGE_SIZE_BYTES;
        bus.memory.state.graphics_vram[0] = 0x11;
        bus.memory.state.graphics_vram[page1_base] = 0x22;

        assert_eq!(bus.read_byte(0xA8000), 0x11);

        bus.io_write_byte(0xA6, 0x01);
        assert_eq!(bus.read_byte(0xA8000), 0x22);

        bus.io_write_byte(0xA6, 0x00);
        assert_eq!(bus.read_byte(0xA8000), 0x11);
    }

    #[test]
    fn access_page_selects_e_plane_bank() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);
        bus.display_control.state.mode2 |= 0x01;
        bus.set_graphics_extension_enabled(true);

        let e_page1_base = super::E_PLANE_PAGE_SIZE_BYTES;
        bus.memory.state.e_plane_vram[0] = 0x33;
        bus.memory.state.e_plane_vram[e_page1_base] = 0x44;

        assert_eq!(bus.read_byte(0xE0000), 0x33);

        bus.io_write_byte(0xA6, 0x01);
        assert_eq!(bus.read_byte(0xE0000), 0x44);

        bus.write_byte(0xE0000, 0x55);
        assert_eq!(bus.memory.state.e_plane_vram[e_page1_base], 0x55);
        assert_eq!(bus.memory.state.e_plane_vram[0], 0x33);
    }

    #[test]
    fn display_page_selects_snapshot_bank() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        let page1_base = super::GRAPHICS_PAGE_SIZE_BYTES;
        bus.memory.state.graphics_vram[0] = 0xAA; // page 0 B-plane
        bus.memory.state.graphics_vram[page1_base] = 0xBB; // page 1 B-plane

        // Snapshot from page 0 (default).
        bus.capture_vsync_snapshot();
        assert_eq!(bus.vsync_snapshot().graphics_b_plane[0] & 0xFF, 0xAA);

        // Switch display page to 1 via port 0xA4.
        bus.io_write_byte(0xA4, 0x01);
        bus.capture_vsync_snapshot();
        assert_eq!(bus.vsync_snapshot().graphics_b_plane[0] & 0xFF, 0xBB);
    }

    #[test]
    fn grcg_tdw_operates_on_access_page() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);

        // Seed page 0 B-plane with a known value before enabling GRCG.
        bus.memory.state.graphics_vram[0] = 0x42;

        bus.grcg.write_mode(0x80); // TDW, all planes enabled
        bus.grcg.state.tile = [0x5A, 0xA5, 0x3C, 0x00];

        // Switch to page 1 and write through GRCG.
        bus.io_write_byte(0xA6, 0x01);
        bus.write_byte(0xA8000, 0xFF);

        // Page 0 B-plane untouched.
        assert_eq!(bus.memory.state.graphics_vram[0], 0x42);

        // Page 1 has tile values.
        let page1_base = super::GRAPHICS_PAGE_SIZE_BYTES;
        assert_eq!(bus.memory.state.graphics_vram[page1_base], 0x5A); // B
        assert_eq!(bus.memory.state.graphics_vram[page1_base + 0x8000], 0xA5); // R
        assert_eq!(bus.memory.state.graphics_vram[page1_base + 0x10000], 0x3C); // G
    }

    #[test]
    fn grcg_tcr_reads_from_access_page() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        bus.grcg.write_mode(0x80); // TCR mode, all planes enabled
        bus.grcg.state.tile = [0xAA, 0xBB, 0xCC, 0x00];

        // Place matching data on page 1 only.
        let page1_base = super::GRAPHICS_PAGE_SIZE_BYTES;
        bus.memory.state.graphics_vram[page1_base] = 0xAA; // B
        bus.memory.state.graphics_vram[page1_base + 0x8000] = 0xBB; // R
        bus.memory.state.graphics_vram[page1_base + 0x10000] = 0xCC; // G

        // TCR read from page 0: no match (all zeros vs tile).
        bus.io_write_byte(0xA6, 0x00);
        let result_page0 = bus.read_byte(0xA8000);
        assert_eq!(result_page0, !0xAA & !0xBB & !0xCC);

        // TCR read from page 1: all match.
        bus.io_write_byte(0xA6, 0x01);
        let result_page1 = bus.read_byte(0xA8000);
        assert_eq!(result_page1, 0xFF);
    }

    #[test]
    fn port_a6_read_returns_access_page() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VM, 48000);

        assert_eq!(bus.io_read_byte(0xA6), 0x00);

        bus.io_write_byte(0xA6, 0x01);
        assert_eq!(bus.io_read_byte(0xA6), 0x01);

        // Only bit 0 matters.
        bus.io_write_byte(0xA6, 0xFE);
        assert_eq!(bus.io_read_byte(0xA6), 0x00);
    }

    #[test]
    fn port_053d_shadow_ram_control() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        bus.memory.set_shadow_control(0x00);
        assert_eq!(bus.memory.state.shadow_control, 0x00);
        bus.io_write_byte(0x053D, 0x82);
        assert_eq!(bus.memory.state.shadow_control, 0x82);
    }

    #[test]
    fn port_053d_shadow_ram_boot_sequence() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        bus.memory.set_shadow_control(0x00);
        let mut rom = vec![0xFFu8; 0x18000];
        rom[0] = 0xAA;
        rom[0x17FFF] = 0xBB;
        bus.memory.load_rom(&rom);

        // ROM mode (shadow_control=0x00): reads come from ROM.
        assert_eq!(bus.memory.read_byte(0xE8000), 0xAA);
        assert_eq!(bus.memory.read_byte(0xFFFFF), 0xBB);

        // Read ROM contents and write back while still in ROM-read mode.
        // Writes go to shadow RAM independently of the read selector.
        let val_e8 = bus.memory.read_byte(0xE8000);
        let val_ff = bus.memory.read_byte(0xFFFFF);
        bus.memory.write_byte(0xE8000, val_e8);
        bus.memory.write_byte(0xFFFFF, val_ff);

        // Shadow RAM now has the copied values, but reads still return ROM.
        assert_eq!(bus.memory.read_byte(0xE8000), 0xAA);

        // Switch to shadow RAM read mode (bit 1 set).
        bus.io_write_byte(0x053D, 0x02);

        // Reads now come from shadow RAM and should return the copied values.
        assert_eq!(bus.memory.read_byte(0xE8000), 0xAA);
        assert_eq!(bus.memory.read_byte(0xFFFFF), 0xBB);
    }

    #[test]
    fn port_043b_readback() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        assert_eq!(bus.hole_15m_control, 0x00);
        bus.io_write_byte(0x043B, 0x55);
        assert_eq!(bus.hole_15m_control, 0x55);
        assert_eq!(bus.io_read_byte(0x043B), 0x55);
    }

    #[test]
    fn ram_window_blocked_when_bios_access_disabled() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        // Shadow RAM read mode (bit 1) so we can read back writes to the BIOS range.
        bus.memory.set_shadow_control(0x02);
        // Map RAM window to E0000-FFFFF range (window value 0x0E → physical base 0xE0000).
        // Use offset 0x88000 to reach physical 0xE8000 (BIOS ROM / shadow RAM area).
        bus.ram_window = 0x0E;
        bus.write_byte_with_access_page(0x88000, 0xAB);
        assert_eq!(bus.read_byte_with_access_page(0x88000), 0xAB);

        // Set bit 2 of shadow_control: disable BIOS RAM access via window.
        bus.io_write_byte(0x053D, 0x06);

        // Reads to the BIOS range via window should now return 0xFF.
        assert_eq!(bus.read_byte_with_access_page(0x88000), 0xFF);

        // Writes should be dropped.
        bus.write_byte_with_access_page(0x88000, 0xCD);
        // Clear bit 2, re-read to verify write was dropped.
        bus.io_write_byte(0x053D, 0x02);
        assert_eq!(bus.read_byte_with_access_page(0x88000), 0xAB);
    }

    #[test]
    fn port_00f6_a20_control() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        bus.a20_enabled = false;
        // 0x02 = release A20 mask.
        bus.io_write_byte(0xF6, 0x02);
        assert!(bus.a20_enabled);
        // 0x03 = set A20 mask.
        bus.io_write_byte(0xF6, 0x03);
        assert!(!bus.a20_enabled);
        // Read back: bit 0 = mask state (1=masked).
        assert_eq!(bus.io_read_byte(0xF6) & 1, 1);
        bus.io_write_byte(0xF6, 0x02);
        assert_eq!(bus.io_read_byte(0xF6) & 1, 0);
    }

    #[test]
    fn port_0567_readback() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        assert_eq!(bus.io_read_byte(0x0567), 0xE0);
        bus.io_write_byte(0x0567, 0x42);
        assert_eq!(bus.io_read_byte(0x0567), 0x42);
    }

    #[test]
    fn port_0cc4_returns_ff() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        assert_eq!(bus.io_read_byte(0x0CC4), 0xFF);
    }

    #[test]
    fn sound_ports_0288_and_0388_do_not_alias_low_bank() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        bus.install_soundboard_86(None);

        // Primary base 0x0188 works: reg 0xFF returns chip ID 0x01.
        bus.io_write_byte(0x0188, 0xFF);
        assert_eq!(bus.io_read_byte(0x018A), 0x01);

        // Ports 0x0288 and 0x0388 must not reach the sound board.
        assert_eq!(bus.io_read_byte(0x028A), 0xFF);
        assert_eq!(bus.io_read_byte(0x038A), 0xFF);
    }

    #[test]
    fn port_a460_reports_86_id_and_mask_controls_opna() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        bus.install_soundboard_86(None);

        assert_eq!(bus.io_read_byte(0xA460), 0x40);

        bus.io_write_byte(0xA460, 0x03);
        assert_eq!(bus.io_read_byte(0xA460), 0x43);

        bus.io_write_byte(0xA460, 0x02);
        bus.io_write_byte(0x0188, 0xFF);
        assert_eq!(bus.io_read_byte(0x018A), 0xFF);

        bus.io_write_byte(0xA460, 0x00);
        bus.io_write_byte(0x0188, 0xFF);
        assert_eq!(bus.io_read_byte(0x018A), 0x01);
    }

    #[test]
    fn ram_window_default_identity() {
        let bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        assert_eq!(bus.ram_window, 0x08);
    }

    #[test]
    fn ram_window_remaps_to_extended_ram() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801RA, 48000);
        // Set RAM window to 0x10 → physical base 0x100000 (1 MB).
        bus.ram_window = 0x10;
        // Write via remapped window.
        bus.write_byte_with_access_page(0x80000, 0xAB);
        // Should land in extended RAM at offset 0.
        assert_eq!(bus.memory.state.extended_ram[0], 0xAB);
        // Read back via remapped window.
        assert_eq!(bus.read_byte_with_access_page(0x80000), 0xAB);
        // Original RAM at 0x80000 should be untouched.
        assert_eq!(bus.memory.state.ram[0x80000], 0x00);
    }

    #[test]
    fn port_f0_cold_reset_preserves_shut_bits() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        // Set SHUT0=1 (bit 7) and SHUT1=1 (bit 5) via PPI control port.
        bus.io_write_byte(0x37, 0x0F); // set SHUT0
        bus.io_write_byte(0x37, 0x0B); // set SHUT1
        assert_ne!(bus.system_ppi.state.port_c & 0x80, 0, "SHUT0 should be set");
        assert_ne!(bus.system_ppi.state.port_c & 0x20, 0, "SHUT1 should be set");

        bus.io_write_byte(0xF0, 0x00);

        // Cold reset must leave both SHUT bits intact so the ITF
        // can read SHUT0=1, SHUT1=1 and perform a normal reset.
        assert!(bus.reset_pending);
        assert!(!bus.shutdown_requested);
        assert!(bus.warm_reset_context.is_none());
        assert_ne!(
            bus.system_ppi.state.port_c & 0x80,
            0,
            "SHUT0 must survive cold reset"
        );
        assert_ne!(
            bus.system_ppi.state.port_c & 0x20,
            0,
            "SHUT1 must survive cold reset"
        );
    }

    #[test]
    fn port_f0_warm_reset_captures_context() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        // Clear SHUT0 (bit 7) — leaves warm-reset mode.
        bus.io_write_byte(0x37, 0x0E); // clear SHUT0
        assert_eq!(
            bus.system_ppi.state.port_c & 0x80,
            0,
            "SHUT0 should be clear"
        );

        // Plant a warm-reset context at 0000:0404 (SP) and 0000:0406 (SS).
        // The ITF reads SS:SP from there, then pops CS:IP via RETF.
        // SS=0x0000, SP=0x0600: stack at 0000:0600 contains IP=0x1234, CS=0x5678.
        bus.memory.state.ram[0x0404] = 0x00; // SP low
        bus.memory.state.ram[0x0405] = 0x06; // SP high (0x0600)
        bus.memory.state.ram[0x0406] = 0x00; // SS low
        bus.memory.state.ram[0x0407] = 0x00; // SS high (0x0000)
        bus.memory.state.ram[0x0600] = 0x34; // IP low
        bus.memory.state.ram[0x0601] = 0x12; // IP high
        bus.memory.state.ram[0x0602] = 0x78; // CS low
        bus.memory.state.ram[0x0603] = 0x56; // CS high

        bus.io_write_byte(0xF0, 0x00);

        assert!(bus.reset_pending);
        assert!(!bus.shutdown_requested);
        let (ss, sp, cs, ip) = bus.warm_reset_context.unwrap();
        assert_eq!(ss, 0x0000);
        assert_eq!(sp, 0x0604); // SP advanced past the popped RETF frame
        assert_eq!(cs, 0x5678);
        assert_eq!(ip, 0x1234);
    }

    #[test]
    fn port_f0_shutdown_sets_flag() {
        let mut bus = Pc9801Bus::<NoTracing>::new(MachineModel::PC9801VX, 48000);
        // Set SHUT0=1 (bit 7), clear SHUT1 (bit 5) → shutdown.
        bus.io_write_byte(0x37, 0x0F); // set SHUT0
        bus.io_write_byte(0x37, 0x0A); // clear SHUT1
        assert_ne!(bus.system_ppi.state.port_c & 0x80, 0, "SHUT0 should be set");
        assert_eq!(
            bus.system_ppi.state.port_c & 0x20,
            0,
            "SHUT1 should be clear"
        );

        bus.io_write_byte(0xF0, 0x00);

        assert!(bus.reset_pending);
        assert!(bus.shutdown_requested);
        assert!(bus.warm_reset_context.is_none());
    }
}
