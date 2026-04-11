use common::{CdAudioState, CdAudioStatus, OsBootStage, Tracing};

#[derive(Default)]
struct RecordingTracer {
    events: Vec<&'static str>,
    boot_stages: Vec<OsBootStage>,
}

impl Tracing for RecordingTracer {
    fn trace_os_boot(
        &mut self,
        stage: OsBootStage,
        _cpu: &dyn common::CpuAccess,
        _memory: &dyn common::MemoryAccess,
    ) {
        self.boot_stages.push(stage);
    }

    fn trace_os_dispatch(
        &mut self,
        vector: u8,
        _cpu: &dyn common::CpuAccess,
        _memory: &dyn common::MemoryAccess,
    ) {
        if vector == 0x21 {
            self.events.push("dispatch");
        }
    }

    fn trace_int21h(&mut self, _cpu: &dyn common::CpuAccess, _memory: &dyn common::MemoryAccess) {
        self.events.push("int21");
    }

    fn trace_int21h_get_current_directory(
        &mut self,
        _cpu: &dyn common::CpuAccess,
        _memory: &dyn common::MemoryAccess,
    ) {
        self.events.push("getcwd");
    }
}

#[derive(Default)]
struct FakeCpu {
    ax: u16,
    bx: u16,
    cx: u16,
    dx: u16,
    si: u16,
    di: u16,
    ds: u16,
    es: u16,
    ss: u16,
    sp: u16,
    cs: u16,
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
    carry: bool,
}

impl common::CpuAccess for FakeCpu {
    fn ax(&self) -> u16 {
        self.ax
    }
    fn set_ax(&mut self, value: u16) {
        self.ax = value;
        self.eax = (self.eax & 0xFFFF_0000) | u32::from(value);
    }
    fn bx(&self) -> u16 {
        self.bx
    }
    fn set_bx(&mut self, value: u16) {
        self.bx = value;
        self.ebx = (self.ebx & 0xFFFF_0000) | u32::from(value);
    }
    fn cx(&self) -> u16 {
        self.cx
    }
    fn set_cx(&mut self, value: u16) {
        self.cx = value;
        self.ecx = (self.ecx & 0xFFFF_0000) | u32::from(value);
    }
    fn dx(&self) -> u16 {
        self.dx
    }
    fn set_dx(&mut self, value: u16) {
        self.dx = value;
        self.edx = (self.edx & 0xFFFF_0000) | u32::from(value);
    }
    fn si(&self) -> u16 {
        self.si
    }
    fn set_si(&mut self, value: u16) {
        self.si = value;
    }
    fn di(&self) -> u16 {
        self.di
    }
    fn set_di(&mut self, value: u16) {
        self.di = value;
    }
    fn ds(&self) -> u16 {
        self.ds
    }
    fn set_ds(&mut self, value: u16) {
        self.ds = value;
    }
    fn es(&self) -> u16 {
        self.es
    }
    fn set_es(&mut self, value: u16) {
        self.es = value;
    }
    fn ss(&self) -> u16 {
        self.ss
    }
    fn set_ss(&mut self, value: u16) {
        self.ss = value;
    }
    fn sp(&self) -> u16 {
        self.sp
    }
    fn set_sp(&mut self, value: u16) {
        self.sp = value;
    }
    fn cs(&self) -> u16 {
        self.cs
    }
    fn set_carry(&mut self, carry: bool) {
        self.carry = carry;
    }
    fn eax(&self) -> u32 {
        self.eax
    }
    fn set_eax(&mut self, value: u32) {
        self.eax = value;
        self.ax = value as u16;
    }
    fn ebx(&self) -> u32 {
        self.ebx
    }
    fn set_ebx(&mut self, value: u32) {
        self.ebx = value;
        self.bx = value as u16;
    }
    fn ecx(&self) -> u32 {
        self.ecx
    }
    fn set_ecx(&mut self, value: u32) {
        self.ecx = value;
        self.cx = value as u16;
    }
    fn edx(&self) -> u32 {
        self.edx
    }
    fn set_edx(&mut self, value: u32) {
        self.edx = value;
        self.dx = value as u16;
    }
}

struct FakeMemory {
    bytes: Vec<u8>,
}

impl Default for FakeMemory {
    fn default() -> Self {
        Self {
            bytes: vec![0; 2 * 1024 * 1024],
        }
    }
}

impl common::MemoryAccess for FakeMemory {
    fn read_byte(&self, address: u32) -> u8 {
        self.bytes[address as usize]
    }
    fn write_byte(&mut self, address: u32, value: u8) {
        self.bytes[address as usize] = value;
    }
    fn read_word(&self, address: u32) -> u16 {
        u16::from_le_bytes([self.read_byte(address), self.read_byte(address + 1)])
    }
    fn write_word(&mut self, address: u32, value: u16) {
        let bytes = value.to_le_bytes();
        self.write_byte(address, bytes[0]);
        self.write_byte(address + 1, bytes[1]);
    }
    fn read_block(&self, address: u32, buf: &mut [u8]) {
        let start = address as usize;
        buf.copy_from_slice(&self.bytes[start..start + buf.len()]);
    }
    fn write_block(&mut self, address: u32, data: &[u8]) {
        let start = address as usize;
        self.bytes[start..start + data.len()].copy_from_slice(data);
    }
}

#[derive(Default)]
struct FakeDisk;

impl common::DiskIo for FakeDisk {
    fn read_sectors(&mut self, _drive_da: u8, _lba: u32, _count: u32) -> Result<Vec<u8>, u8> {
        Err(0x0F)
    }
    fn write_sectors(&mut self, _drive_da: u8, _lba: u32, _data: &[u8]) -> Result<(), u8> {
        Err(0x0F)
    }
    fn sector_size(&self, _drive_da: u8) -> Option<u16> {
        None
    }
    fn total_sectors(&self, _drive_da: u8) -> Option<u32> {
        None
    }
    fn drive_geometry(&self, _drive_da: u8) -> Option<(u16, u8, u8)> {
        None
    }
}

impl common::CdromIo for FakeDisk {
    fn cdrom_present(&self) -> bool {
        false
    }
    fn cdrom_media_loaded(&self) -> bool {
        false
    }
    fn read_sector_cooked(&self, _lba: u32, _buf: &mut [u8]) -> Option<usize> {
        None
    }
    fn read_sector_raw(&self, _lba: u32, _buf: &mut [u8]) -> Option<usize> {
        None
    }
    fn track_count(&self) -> u8 {
        0
    }
    fn track_info(&self, _track_number: u8) -> Option<common::CdromTrackInfo> {
        None
    }
    fn leadout_lba(&self) -> u32 {
        0
    }
    fn total_sectors(&self) -> u32 {
        0
    }
    fn audio_play(&mut self, _start_lba: u32, _sector_count: u32) {}
    fn audio_stop(&mut self) {}
    fn audio_resume(&mut self) {}
    fn audio_state(&self) -> CdAudioStatus {
        CdAudioStatus {
            state: CdAudioState::Stopped,
            current_lba: 0,
            start_lba: 0,
            end_lba: 0,
        }
    }
    fn audio_channel_info(&self) -> common::AudioChannelInfo {
        common::AudioChannelInfo {
            input_channel: [0; 4],
            volume: [0; 4],
        }
    }
    fn set_audio_channel_info(&mut self, _info: &common::AudioChannelInfo) {}
}

#[test]
fn boot_emits_expected_os_boot_stages() {
    let mut os = os::NeetanOs::new();
    let mut cpu = FakeCpu::default();
    let mut memory = FakeMemory::default();
    let mut disk = FakeDisk;
    let mut tracer = RecordingTracer::default();

    os.boot(&mut cpu, &mut memory, &mut disk, &mut tracer);

    assert_eq!(
        tracer.boot_stages,
        vec![
            OsBootStage::Start,
            OsBootStage::DosDataStructuresReady,
            OsBootStage::DrivesReady,
            OsBootStage::ConfigApplied,
            OsBootStage::CdromReady,
            OsBootStage::InitialProcessReady,
            OsBootStage::MemoryManagerReady,
            OsBootStage::AutoexecReady,
            OsBootStage::ShellReady,
            OsBootStage::End,
        ]
    );
}

#[test]
fn int21_get_current_directory_emits_expected_trace_hooks() {
    let mut os = os::NeetanOs::new();
    let mut cpu = FakeCpu {
        ax: 0x4700,
        ds: 0x2000,
        si: 0x0010,
        ..FakeCpu::default()
    };
    let mut memory = FakeMemory::default();
    let mut disk = FakeDisk;
    let mut tracer = RecordingTracer::default();

    os.boot(&mut cpu, &mut memory, &mut disk, &mut tracer);
    tracer.events.clear();

    assert!(os.dispatch(0x21, &mut cpu, &mut memory, &mut disk, &mut tracer));
    assert_eq!(tracer.events, vec!["dispatch", "int21", "getcwd"]);
}
