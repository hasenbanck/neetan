//! PC-9801-86 sound board: YM2608 (OPNA) stereo FM + SSG + ADPCM + PCM86 DAC.

use std::cell::{Cell, RefCell};

use common::EventKind;
use resampler::{Attenuation, Latency, ResamplerFir};
use ymfm_oxide::{Ym2608, Ym2608Callbacks, YmfmAccessClass, YmfmOpnFidelity, YmfmOutput3};

use crate::soundboard_26k::FmSampleRemainder;

/// YM2608 input clock: 7.987200 MHz crystal.
const YM2608_CLOCK: u32 = 7_987_200;

const FIDELITY: YmfmOpnFidelity = YmfmOpnFidelity::Max;

/// 256 KB ADPCM-B sample RAM.
const ADPCM_B_RAM_SIZE: usize = 256 * 1024;

/// ADPCM-A rhythm ROM size (ym2608.rom).
const RHYTHM_ROM_SIZE: usize = 8192;

/// PCM86 sample rates indexed by (fifo & 7).
const PCM86_RATES: [u32; 8] = [44100, 33075, 22050, 16538, 11025, 8269, 5513, 4134];

/// PCM86 step bit table indexed by (dactrl >> 4) & 7.
const PCM86_BITS: [u8; 8] = [1, 1, 1, 2, 0, 0, 0, 1];

/// PCM86 buffer size (64 KB ring buffer).
const PCM86_BUFSIZE: usize = 1 << 16;

const PCM86_BUFMASK: u32 = (PCM86_BUFSIZE as u32) - 1;

/// PCM86 logical buffer capacity.
const PCM86_LOGICAL_BUF: i32 = 0x8000;

/// Embedded rhythm samples at 48000 Hz, packed as 6×u32 LE header + i16 LE data.
/// These are not the original ROM files but recordings of the samples.
/// Used under fair use and de minimis.
static RHYTHM_BIN: &[u8] = include_bytes!("../../../utils/rhythm.bin");

/// Snapshot of the PC-9801-86 sound board state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Soundboard86State {
    /// Low bank address latch (write via port 0x0188).
    pub address_lo: u8,
    /// High bank address latch (write via port 0x018C).
    pub address_hi: u8,
    /// IRQ line number for the FM chip.
    pub irq_line: u8,
    /// Whether the IRQ output is currently asserted.
    pub irq_asserted: bool,
    /// CPU cycle at which the busy flag clears.
    pub busy_end_cycle: u64,
    /// CPU cycle at which the current audio frame started.
    pub audio_frame_start_cycle: u64,
    /// Fractional sample remainder carried across frames.
    pub sample_remainder: FmSampleRemainder,
    /// Whether extended mode (6-ch FM + ADPCM) is enabled.
    pub extend_enabled: bool,
    /// Last data written to a register (returned for reads of addr >= 0x10).
    pub last_written_data: u8,
    /// PCM86 state.
    pub pcm86: Pcm86State,
}

impl Default for Soundboard86State {
    fn default() -> Self {
        Self {
            address_lo: 0,
            address_hi: 0,
            irq_line: 12,
            irq_asserted: false,
            busy_end_cycle: 0,
            audio_frame_start_cycle: 0,
            sample_remainder: FmSampleRemainder::default(),
            extend_enabled: false,
            last_written_data: 0,
            pcm86: Pcm86State::default(),
        }
    }
}

/// PCM86 DAC state snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm86State {
    /// Sound flags register (port 0xA460).
    pub sound_flags: u8,
    /// FIFO control register (port 0xA468).
    pub fifo: u8,
    /// DAC control register (port 0xA46A).
    pub dactrl: u8,
    /// Volume (4-bit, port 0xA466).
    pub volume: u8,
    /// FIFO interrupt threshold.
    pub fifo_size: i32,
    /// Write position in ring buffer.
    pub write_pos: u32,
    /// Read position in ring buffer.
    pub read_pos: u32,
    /// Physical buffer count.
    pub real_buf: i32,
    /// Virtual buffer count (capped at PCM86_LOGICAL_BUF).
    pub vir_buf: i32,
    /// Step bit (bytes-per-sample shift).
    pub step_bit: u8,
    /// Step mask.
    pub step_mask: u32,
    /// Audio frame start cycle.
    pub audio_frame_start_cycle: u64,
    /// Fractional sample remainder.
    pub sample_remainder: FmSampleRemainder,
}

impl Default for Pcm86State {
    fn default() -> Self {
        Self {
            // PC-9801-86 ID (0x4x) at 0x018x base; mask/extend bits clear at reset.
            sound_flags: 0x40,
            fifo: 0,
            dactrl: 0,
            volume: 0,
            fifo_size: 0,
            write_pos: 0,
            read_pos: 0,
            real_buf: 0,
            vir_buf: 0,
            step_bit: 1,
            step_mask: 1,
            audio_frame_start_cycle: 0,
            sample_remainder: FmSampleRemainder::default(),
        }
    }
}

enum PendingChipAction {
    SetTimer {
        timer_id: u32,
        duration_in_clocks: i32,
    },
    UpdateIrq {
        asserted: bool,
    },
}

struct ChipBridge {
    current_cycle: Cell<u64>,
    busy_end_cycle: Cell<u64>,
    cpu_clock_hz: u32,
    pending: RefCell<Vec<PendingChipAction>>,
    rhythm_rom: Box<[u8; RHYTHM_ROM_SIZE]>,
    adpcm_b_ram: RefCell<Box<[u8; ADPCM_B_RAM_SIZE]>>,
}

impl ChipBridge {
    fn new(cpu_clock_hz: u32, rhythm_rom: Option<&[u8]>) -> Self {
        let mut rom = Box::new([0u8; RHYTHM_ROM_SIZE]);
        if let Some(data) = rhythm_rom {
            let len = data.len().min(RHYTHM_ROM_SIZE);
            rom[..len].copy_from_slice(&data[..len]);
        }
        Self {
            current_cycle: Cell::new(0),
            busy_end_cycle: Cell::new(0),
            cpu_clock_hz,
            pending: RefCell::new(Vec::new()),
            rhythm_rom: rom,
            adpcm_b_ram: RefCell::new(Box::new([0u8; ADPCM_B_RAM_SIZE])),
        }
    }
}

impl Ym2608Callbacks for ChipBridge {
    fn set_timer(&self, timer_id: u32, duration_in_clocks: i32) {
        self.pending.borrow_mut().push(PendingChipAction::SetTimer {
            timer_id,
            duration_in_clocks,
        });
    }

    fn set_busy_end(&self, clocks: u32) {
        let cpu_clocks = u64::from(clocks) * u64::from(self.cpu_clock_hz) / u64::from(YM2608_CLOCK);
        self.busy_end_cycle
            .set(self.current_cycle.get() + cpu_clocks);
    }

    fn is_busy(&self) -> bool {
        self.current_cycle < self.busy_end_cycle
    }

    fn update_irq(&self, asserted: bool) {
        self.pending
            .borrow_mut()
            .push(PendingChipAction::UpdateIrq { asserted });
    }

    fn external_read(&self, access_class: YmfmAccessClass, address: u32) -> u8 {
        match access_class {
            YmfmAccessClass::AdpcmA => {
                let addr = address as usize;
                if addr < RHYTHM_ROM_SIZE {
                    self.rhythm_rom[addr]
                } else {
                    0
                }
            }
            YmfmAccessClass::AdpcmB => {
                let addr = (address as usize) & (ADPCM_B_RAM_SIZE - 1);
                self.adpcm_b_ram.borrow()[addr]
            }
            _ => 0,
        }
    }

    fn external_write(&self, access_class: YmfmAccessClass, address: u32, data: u8) {
        if access_class == YmfmAccessClass::AdpcmB {
            let addr = (address as usize) & (ADPCM_B_RAM_SIZE - 1);
            self.adpcm_b_ram.borrow_mut()[addr] = data;
        }
    }
}

/// Procedural rhythm track for fallback when no ROM is loaded.
struct RhythmTrack {
    samples: Vec<i16>,
    position: usize,
    playing: bool,
    volume: u8,
    pan: u8,
}

/// Procedural rhythm section for ROM-less fallback.
struct RhythmSection {
    enabled: bool,
    tracks: [RhythmTrack; 6],
    master_volume: u8,
}

impl RhythmSection {
    fn new(enabled: bool) -> Self {
        let tracks = if enabled {
            Self::load_tracks_from_bin()
        } else {
            std::array::from_fn(|_| RhythmTrack {
                samples: Vec::new(),
                position: 0,
                playing: false,
                volume: 31,
                pan: 0b11,
            })
        };
        Self {
            enabled,
            tracks,
            master_volume: 63,
        }
    }

    fn load_tracks_from_bin() -> [RhythmTrack; 6] {
        let header_size = 6 * 4;
        let header = &RHYTHM_BIN[..header_size];
        let mut counts = [0u32; 6];
        for (i, count) in counts.iter_mut().enumerate() {
            let offset = i * 4;
            *count = u32::from_le_bytes([
                header[offset],
                header[offset + 1],
                header[offset + 2],
                header[offset + 3],
            ]);
        }

        let mut data_offset = header_size;
        std::array::from_fn(|i| {
            let sample_count = counts[i] as usize;
            let byte_len = sample_count * 2;
            let bytes = &RHYTHM_BIN[data_offset..data_offset + byte_len];
            let samples: Vec<i16> = bytes
                .chunks_exact(2)
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            data_offset += byte_len;
            RhythmTrack {
                samples,
                position: 0,
                playing: false,
                volume: 31,
                pan: 0b11,
            }
        })
    }

    fn key_on(&mut self, mask: u8) {
        for i in 0..6 {
            if mask & (1 << i) != 0 {
                self.tracks[i].position = 0;
                self.tracks[i].playing = true;
            }
        }
    }

    fn key_off(&mut self, mask: u8) {
        for i in 0..6 {
            if mask & (1 << i) != 0 {
                self.tracks[i].playing = false;
            }
        }
    }

    fn write_register(&mut self, register: u8, value: u8) {
        match register {
            0x10 => {
                let mask = value & 0x3F;
                if value & 0x80 != 0 {
                    self.key_off(mask);
                } else {
                    self.key_on(mask);
                }
            }
            0x11 => {
                self.master_volume = value & 0x3F;
            }
            0x18..=0x1D => {
                let idx = (register - 0x18) as usize;
                if idx < 6 {
                    self.tracks[idx].volume = value & 0x1F;
                    self.tracks[idx].pan = (value >> 6) & 0x03;
                }
            }
            _ => {}
        }
    }

    fn generate_stereo_sample(&mut self) -> (f32, f32) {
        if !self.enabled {
            return (0.0, 0.0);
        }

        let master_scale = volume_scale(self.master_volume as u32, 63);
        let mut left = 0.0f32;
        let mut right = 0.0f32;
        for track in &mut self.tracks {
            if !track.playing {
                continue;
            }
            if track.position >= track.samples.len() {
                track.playing = false;
                continue;
            }
            let raw = track.samples[track.position] as f32 / 32768.0;
            let vol_scale = volume_scale(track.volume as u32, 31);
            let sample = raw * vol_scale;
            // pan: bit 1 = left, bit 0 = right
            if track.pan & 0b10 != 0 {
                left += sample;
            }
            if track.pan & 0b01 != 0 {
                right += sample;
            }
            track.position += 1;
        }
        (left * master_scale, right * master_scale)
    }
}

fn volume_scale(level: u32, max: u32) -> f32 {
    if level == 0 {
        return 0.0;
    }
    let normalized = level as f32 / max as f32;
    normalized * normalized
}

/// PCM86 DAC FIFO-based PCM playback engine.
struct Pcm86 {
    state: Pcm86State,
    buffer: Box<[u8; PCM86_BUFSIZE]>,
    pcm_resampler: ResamplerFir,
    pcm_resample_input: Vec<f32>,
    pcm_resample_output: Vec<f32>,
}

impl Pcm86 {
    fn new(sample_rate: u32) -> Self {
        let initial_pcm_rate = PCM86_RATES[0];
        let resampler = ResamplerFir::new(
            1,
            initial_pcm_rate,
            sample_rate,
            Latency::Sample64,
            Attenuation::Db60,
        );
        let output_size = resampler.buffer_size_output();
        Self {
            state: Pcm86State::default(),
            buffer: Box::new([0u8; PCM86_BUFSIZE]),
            pcm_resampler: resampler,
            pcm_resample_input: vec![0.0; 4096],
            pcm_resample_output: vec![0.0; output_size],
        }
    }

    fn pcm_rate(&self) -> u32 {
        PCM86_RATES[(self.state.fifo & 7) as usize]
    }

    fn is_playing(&self) -> bool {
        self.state.fifo & 0x80 != 0
    }

    fn irq_enabled(&self) -> bool {
        self.state.fifo & 0x20 != 0
    }

    fn read_port(&mut self, port: u16, current_cycle: u64, cpu_clock_hz: u32) -> u8 {
        match port {
            0xA460 => self.state.sound_flags,
            0xA466 => {
                self.update_buffer_state(current_cycle, cpu_clock_hz);
                let mut ret = 0u8;
                if self.state.vir_buf >= PCM86_LOGICAL_BUF {
                    ret |= 0x80;
                } else if self.state.vir_buf <= self.state.step_mask as i32 {
                    ret |= 0x40;
                }
                ret
            }
            0xA468 => {
                let mut ret = self.state.fifo & !0x10;
                if self.check_irq(current_cycle, cpu_clock_hz) {
                    ret |= 0x10;
                }
                ret
            }
            0xA46A => self.state.dactrl,
            _ => 0x00,
        }
    }

    fn write_port(
        &mut self,
        port: u16,
        value: u8,
        current_cycle: u64,
        cpu_clock_hz: u32,
        sample_rate: u32,
    ) {
        match port {
            0xA460 => {
                // Bits 7-2 are board ID/read-only. Bits 1:0 are mask/extend controls.
                self.state.sound_flags = (self.state.sound_flags & 0xFC) | (value & 0x03);
            }
            0xA466 => {
                if (value >> 5) & 7 == 0b101 {
                    self.state.volume = value & 0x0F;
                }
            }
            0xA468 => {
                if (value & 0x08) != 0 && (self.state.fifo & 0x08) == 0 {
                    self.state.write_pos = 0;
                    self.state.read_pos = 0;
                    self.state.real_buf = 0;
                    self.state.vir_buf = 0;
                    self.state.audio_frame_start_cycle = current_cycle;
                    self.state.sample_remainder = FmSampleRemainder::default();
                }
                self.state.fifo = value;
                self.update_step_clock(cpu_clock_hz);
                self.pcm_resampler = ResamplerFir::new(
                    1,
                    self.pcm_rate(),
                    sample_rate,
                    Latency::Sample64,
                    Attenuation::Db60,
                );
                self.pcm_resample_output
                    .resize(self.pcm_resampler.buffer_size_output(), 0.0);
            }
            0xA46A => {
                if self.state.fifo & 0x20 != 0 {
                    if value != 0xFF {
                        self.state.fifo_size = (i32::from(value) + 1) << 7;
                    } else {
                        self.state.fifo_size = 0x7FFC;
                    }
                } else {
                    self.state.dactrl = value;
                    self.state.step_bit = PCM86_BITS[((value >> 4) & 7) as usize];
                    self.state.step_mask = (1u32 << self.state.step_bit) - 1;
                }
            }
            0xA46C => {
                if self.state.vir_buf < PCM86_LOGICAL_BUF {
                    self.state.vir_buf += 1;
                }
                self.buffer[self.state.write_pos as usize] = value;
                self.state.write_pos = (self.state.write_pos + 1) & PCM86_BUFMASK;
                self.state.real_buf += 1;
            }
            _ => {}
        }
    }

    fn update_step_clock(&mut self, _cpu_clock_hz: u32) {
        // Step clock is recomputed from the rate each time fifo changes.
    }

    fn update_buffer_state(&mut self, current_cycle: u64, cpu_clock_hz: u32) {
        if !self.is_playing() {
            return;
        }
        let elapsed = current_cycle.saturating_sub(self.state.audio_frame_start_cycle);
        if elapsed == 0 {
            return;
        }
        let rate = self.pcm_rate() as u64;
        let bytes_per_sample = 1u64 << self.state.step_bit;
        let samples_elapsed = (elapsed as f64 * rate as f64 / f64::from(cpu_clock_hz)) as i32;
        let bytes_consumed = samples_elapsed as i64 * bytes_per_sample as i64;
        let consumed = bytes_consumed.min(self.state.vir_buf as i64) as i32;
        self.state.vir_buf -= consumed;
        self.state.real_buf -= consumed;
        if self.state.real_buf < 0 {
            self.state.real_buf = 0;
        }
        if self.state.vir_buf < 0 {
            self.state.vir_buf = 0;
        }
        self.state.read_pos = (self.state.read_pos + consumed as u32) & PCM86_BUFMASK;
        self.state.audio_frame_start_cycle = current_cycle;
    }

    fn check_irq(&mut self, current_cycle: u64, cpu_clock_hz: u32) -> bool {
        if !self.irq_enabled() || !self.is_playing() {
            return false;
        }
        self.update_buffer_state(current_cycle, cpu_clock_hz);
        self.state.vir_buf <= self.state.fifo_size
    }

    fn drain_samples(&mut self, count: usize) -> Vec<f32> {
        if !self.is_playing() || self.state.real_buf <= 0 {
            return vec![0.0; count];
        }

        let is_16bit = self.state.dactrl & 0x40 == 0;
        let bytes_per_sample = 1usize << self.state.step_bit;
        let vol = pcm86_volume(self.state.volume);

        let mut output = Vec::with_capacity(count);
        for _ in 0..count {
            if self.state.real_buf < bytes_per_sample as i32 {
                output.push(0.0);
                continue;
            }

            let sample = if is_16bit {
                let lo = self.buffer[self.state.read_pos as usize];
                let hi = self.buffer[((self.state.read_pos + 1) & PCM86_BUFMASK) as usize];
                let raw = i16::from_le_bytes([lo, hi]);
                raw as f32 / 32768.0
            } else {
                let raw = self.buffer[self.state.read_pos as usize] as i8;
                raw as f32 / 128.0
            };

            self.state.read_pos = (self.state.read_pos + bytes_per_sample as u32) & PCM86_BUFMASK;
            self.state.real_buf -= bytes_per_sample as i32;
            self.state.vir_buf -= bytes_per_sample as i32;
            if self.state.real_buf < 0 {
                self.state.real_buf = 0;
            }
            if self.state.vir_buf < 0 {
                self.state.vir_buf = 0;
            }

            output.push(sample * vol);
        }
        output
    }
}

fn pcm86_volume(level: u8) -> f32 {
    let inverted = 15u8.saturating_sub(level & 0x0F);
    inverted as f32 / 15.0
}

/// Action the bus must process after a sound board operation.
pub enum Soundboard86Action {
    /// Schedule a timer to fire at the given cycle.
    ScheduleTimer {
        /// Timer event kind.
        kind: EventKind,
        /// CPU cycle at which the timer should fire.
        fire_cycle: u64,
    },
    /// Cancel a previously scheduled timer.
    CancelTimer {
        /// Timer event kind.
        kind: EventKind,
    },
    /// Assert an IRQ line on the PIC.
    AssertIrq {
        /// IRQ line number.
        irq: u8,
    },
    /// Deassert an IRQ line on the PIC.
    DeassertIrq {
        /// IRQ line number.
        irq: u8,
    },
}

/// PC-9801-86 sound board: YM2608 (OPNA) FM + SSG + ADPCM + PCM86 DAC.
pub struct Soundboard86 {
    /// Current device state (saveable).
    pub state: Soundboard86State,
    chip: Ym2608<ChipBridge>,
    cpu_clock_hz: u32,
    native_rate: u32,
    native_buffer: Vec<YmfmOutput3>,
    pending_native: Vec<YmfmOutput3>,
    resampler: ResamplerFir,
    resample_input: Vec<f32>,
    resample_output: Vec<f32>,
    rhythm: RhythmSection,
    pcm86: Pcm86,
    sample_rate: u32,
}

impl Soundboard86 {
    #[inline]
    fn opna_masked(&self) -> bool {
        self.pcm86.state.sound_flags & 0x02 != 0
    }

    /// Creates a new PC-9801-86 sound board instance.
    ///
    /// `rhythm_rom` is the optional 8 KB `ym2608.rom` ADPCM-A rhythm data.
    /// When `None`, procedural rhythm fallback is enabled.
    pub fn new(cpu_clock_hz: u32, sample_rate: u32, rhythm_rom: Option<&[u8]>) -> Self {
        let has_rom = rhythm_rom.is_some();
        let bridge = ChipBridge::new(cpu_clock_hz, rhythm_rom);
        let mut chip = Ym2608::new(bridge);
        chip.reset();
        chip.set_fidelity(FIDELITY);

        let native_rate = chip.sample_rate(YM2608_CLOCK);
        let resampler = ResamplerFir::new(
            2,
            native_rate,
            sample_rate,
            Latency::Sample64,
            Attenuation::Db60,
        );
        let resample_output_size = resampler.buffer_size_output();

        Self {
            state: Soundboard86State::default(),
            chip,
            cpu_clock_hz,
            native_rate,
            native_buffer: vec![YmfmOutput3 { data: [0; 3] }; 4096],
            pending_native: Vec::new(),
            resampler,
            resample_input: vec![0.0; 4096 * 2],
            resample_output: vec![0.0; resample_output_size],
            rhythm: RhythmSection::new(!has_rom),
            pcm86: Pcm86::new(sample_rate),
            sample_rate,
        }
    }

    /// Returns the low bank address latch value.
    pub fn address_lo(&self) -> u8 {
        self.state.address_lo
    }

    /// Returns the configured IRQ line number.
    pub fn irq_line(&self) -> u8 {
        self.state.irq_line
    }

    /// Returns whether extended mode (6-ch FM + ADPCM) is enabled.
    pub fn extend_enabled(&self) -> bool {
        self.state.extend_enabled
    }

    /// Sets the extend enabled flag (controlled by port 0xA460 bit 0).
    pub fn set_extend_enabled(&mut self, enabled: bool) {
        self.state.extend_enabled = enabled;
    }

    /// Advances the YM2608 chip clock to `current_cycle` by generating native
    /// samples, buffering them for later resampling in `generate_samples()`.
    fn sync_to_cycle(&mut self, current_cycle: u64) {
        let frame_start = self.state.audio_frame_start_cycle;
        let frame_cycles = current_cycle.saturating_sub(frame_start);
        if frame_cycles == 0 {
            return;
        }

        let native_rate = u64::from(self.native_rate);
        let exact_native = (frame_cycles as f64 * native_rate as f64)
            / f64::from(self.cpu_clock_hz)
            + self.state.sample_remainder.0;
        let native_count = exact_native as usize;
        if native_count == 0 {
            return;
        }

        self.state.sample_remainder = FmSampleRemainder(exact_native - native_count as f64);
        self.state.audio_frame_start_cycle = current_cycle;

        if self.native_buffer.len() < native_count {
            self.native_buffer
                .resize(native_count, YmfmOutput3 { data: [0; 3] });
        }
        self.chip.generate(&mut self.native_buffer[..native_count]);
        self.pending_native
            .extend_from_slice(&self.native_buffer[..native_count]);
    }

    /// Reads the chip status register (port 0x0188 read).
    pub fn read_status(&mut self, current_cycle: u64) -> u8 {
        if self.opna_masked() {
            return 0xFF;
        }
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.read_status()
    }

    /// Reads data from the currently addressed register (port 0x018A read).
    pub fn read_data(&mut self, current_cycle: u64) -> u8 {
        if self.opna_masked() {
            return 0xFF;
        }
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        let addr = self.state.address_lo;
        if addr == 0x0E {
            let irq_bits = match self.state.irq_line {
                3 => 0x00u8,
                13 => 0x01,
                10 => 0x02,
                12 => 0x03,
                _ => 0x03,
            };
            (irq_bits << 6) | 0x3F
        } else if addr == 0xFF {
            1
        } else if addr < 0x10 {
            self.chip.read_data()
        } else {
            self.state.last_written_data
        }
    }

    /// Reads the high bank status register (port 0x018C read).
    pub fn read_status_hi(&mut self, current_cycle: u64) -> u8 {
        if self.opna_masked() {
            return 0xFF;
        }
        if !self.state.extend_enabled {
            return 0xFF;
        }
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.read_status_hi()
    }

    /// Reads data from the high bank (port 0x018E read).
    pub fn read_data_hi(&mut self, current_cycle: u64) -> u8 {
        if self.opna_masked() {
            return 0xFF;
        }
        if !self.state.extend_enabled {
            return 0xFF;
        }
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        let addr = self.state.address_hi;
        if addr == 0x08 || addr == 0x0F {
            self.chip.read_data_hi()
        } else {
            0xFF
        }
    }

    /// Writes the register address latch for the low bank (port 0x0188 write).
    pub fn write_address(&mut self, value: u8, current_cycle: u64) {
        if self.opna_masked() {
            return;
        }
        self.state.address_lo = value;
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.write_address(value);
    }

    /// Writes data to the currently addressed register in the low bank (port 0x018A write).
    pub fn write_data(&mut self, value: u8, current_cycle: u64) {
        if self.opna_masked() {
            return;
        }
        self.state.last_written_data = value;
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.write_data(value);

        let addr = self.state.address_lo;
        if self.rhythm.enabled && (0x10..=0x1D).contains(&addr) {
            self.rhythm.write_register(addr, value);
        }
    }

    /// Writes the register address latch for the high bank (port 0x018C write).
    pub fn write_address_hi(&mut self, value: u8, current_cycle: u64) {
        if self.opna_masked() {
            return;
        }
        if !self.state.extend_enabled {
            return;
        }
        self.state.address_hi = value;
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.write_address_hi(value);
    }

    /// Writes data to the currently addressed register in the high bank (port 0x018E write).
    pub fn write_data_hi(&mut self, value: u8, current_cycle: u64) {
        if self.opna_masked() {
            return;
        }
        if !self.state.extend_enabled {
            return;
        }
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.write_data_hi(value);
    }

    /// Notifies the chip that a timer has expired.
    pub fn timer_expired(&mut self, timer_id: u32, current_cycle: u64) {
        self.sync_to_cycle(current_cycle);
        self.chip.callbacks_mut().current_cycle.set(current_cycle);
        self.chip.timer_expired(timer_id);
    }

    /// Reads a PCM86 port.
    pub fn pcm86_read(&mut self, port: u16, current_cycle: u64, cpu_clock_hz: u32) -> u8 {
        self.pcm86.read_port(port, current_cycle, cpu_clock_hz)
    }

    /// Writes a PCM86 port.
    pub fn pcm86_write(&mut self, port: u16, value: u8, current_cycle: u64, cpu_clock_hz: u32) {
        if port == 0xA460 {
            self.state.extend_enabled = value & 1 != 0;
        }
        self.pcm86
            .write_port(port, value, current_cycle, cpu_clock_hz, self.sample_rate);
    }

    /// Drains pending actions from the chip bridge.
    pub fn drain_actions(&mut self) -> Vec<Soundboard86Action> {
        let opna_masked = self.opna_masked();
        let bridge = self.chip.callbacks_mut();
        self.state.busy_end_cycle = bridge.busy_end_cycle.get();
        let current_cycle = bridge.current_cycle.get();
        let cpu_clock_hz = bridge.cpu_clock_hz;

        let mut actions = Vec::new();
        for pending in bridge.pending.borrow_mut().drain(..) {
            match pending {
                PendingChipAction::SetTimer {
                    timer_id,
                    duration_in_clocks,
                } => {
                    let kind = if timer_id == 0 {
                        EventKind::FmTimerA
                    } else {
                        EventKind::FmTimerB
                    };
                    if duration_in_clocks < 0 {
                        actions.push(Soundboard86Action::CancelTimer { kind });
                    } else {
                        let cpu_cycles = u64::from(duration_in_clocks as u32)
                            * u64::from(cpu_clock_hz)
                            / u64::from(YM2608_CLOCK);
                        actions.push(Soundboard86Action::ScheduleTimer {
                            kind,
                            fire_cycle: current_cycle + cpu_cycles,
                        });
                    }
                }
                PendingChipAction::UpdateIrq { asserted } => {
                    self.state.irq_asserted = asserted;
                    if opna_masked {
                        continue;
                    }
                    if asserted {
                        actions.push(Soundboard86Action::AssertIrq {
                            irq: self.state.irq_line,
                        });
                    } else {
                        actions.push(Soundboard86Action::DeassertIrq {
                            irq: self.state.irq_line,
                        });
                    }
                }
            }
        }
        actions
    }

    /// Generates resampled stereo audio and mixes it into `output`.
    ///
    /// `output` is interleaved stereo (`[L, R, L, R, …]`).
    pub fn generate_samples(
        &mut self,
        current_cycle: u64,
        cpu_clock_hz: u32,
        volume: f32,
        output: &mut [f32],
    ) {
        // Save frame start before sync may advance it (needed for PCM86 timing).
        let pcm_frame_start = self.state.audio_frame_start_cycle;

        if output.is_empty() {
            self.sync_to_cycle(current_cycle);
            self.pending_native.clear();
            self.state.audio_frame_start_cycle = current_cycle;
            return;
        }

        // Generate remaining FM native samples since last sync.
        let frame_start = self.state.audio_frame_start_cycle;
        let frame_cycles = current_cycle.saturating_sub(frame_start);
        let remaining_native = if frame_cycles > 0 {
            let native_rate = u64::from(self.native_rate);
            let exact_native = (frame_cycles as f64 * native_rate as f64) / f64::from(cpu_clock_hz)
                + self.state.sample_remainder.0;
            let count = exact_native as usize;
            self.state.sample_remainder = FmSampleRemainder(exact_native - count as f64);
            count
        } else {
            0
        };

        if remaining_native > 0 {
            if self.native_buffer.len() < remaining_native {
                self.native_buffer
                    .resize(remaining_native, YmfmOutput3 { data: [0; 3] });
            }
            self.chip
                .generate(&mut self.native_buffer[..remaining_native]);
        }

        let pending_count = self.pending_native.len();
        let total_native = pending_count + remaining_native;

        if total_native > 0 {
            let total_interleaved = total_native * 2;
            if self.resample_input.len() < total_interleaved {
                self.resample_input.resize(total_interleaved, 0.0);
            }

            const FM_SCALE: f32 = 1.25 / 32768.0;
            const SSG_SCALE: f32 = 1.0 / 32768.0;

            for i in 0..pending_count {
                let s = &self.pending_native[i];
                let ssg = s.data[2] as f32 * SSG_SCALE;
                self.resample_input[i * 2] = s.data[0] as f32 * FM_SCALE + ssg;
                self.resample_input[i * 2 + 1] = s.data[1] as f32 * FM_SCALE + ssg;
            }
            for i in 0..remaining_native {
                let s = &self.native_buffer[i];
                let ssg = s.data[2] as f32 * SSG_SCALE;
                let j = pending_count + i;
                self.resample_input[j * 2] = s.data[0] as f32 * FM_SCALE + ssg;
                self.resample_input[j * 2 + 1] = s.data[1] as f32 * FM_SCALE + ssg;
            }

            let mut input_offset = 0;
            let mut output_offset = 0;
            let sample_count = output.len();
            while input_offset < total_interleaved && output_offset < sample_count {
                let Ok((consumed, produced)) = self.resampler.resample(
                    &self.resample_input[input_offset..total_interleaved],
                    &mut self.resample_output,
                ) else {
                    break;
                };
                let usable = produced.min(sample_count - output_offset);
                for (out, &resampled) in output[output_offset..output_offset + usable]
                    .iter_mut()
                    .zip(&self.resample_output[..usable])
                {
                    *out += resampled * volume;
                }
                input_offset += consumed;
                output_offset += usable;
                if consumed == 0 {
                    break;
                }
            }
        }

        // Mix in rhythm section at output rate (48 kHz embedded samples).
        if self.rhythm.enabled {
            for frame in output.chunks_exact_mut(2) {
                let (left, right) = self.rhythm.generate_stereo_sample();
                frame[0] += left * volume;
                frame[1] += right * volume;
            }
        }

        // Mix in PCM86 output using the full elapsed time since last generate call.
        // PCM86 is mono; duplicate to both channels.
        let pcm_frame_cycles = current_cycle.saturating_sub(pcm_frame_start);
        if self.pcm86.is_playing() && self.pcm86.state.real_buf > 0 && pcm_frame_cycles > 0 {
            let pcm_rate = self.pcm86.pcm_rate();
            let exact_pcm = (pcm_frame_cycles as f64 * pcm_rate as f64) / f64::from(cpu_clock_hz)
                + self.pcm86.state.sample_remainder.0;
            let pcm_count = exact_pcm as usize;
            self.pcm86.state.sample_remainder = FmSampleRemainder(exact_pcm - pcm_count as f64);

            if pcm_count > 0 {
                let pcm_samples = self.pcm86.drain_samples(pcm_count);
                if self.pcm86.pcm_resample_input.len() < pcm_count {
                    self.pcm86.pcm_resample_input.resize(pcm_count, 0.0);
                }
                self.pcm86.pcm_resample_input[..pcm_count]
                    .copy_from_slice(&pcm_samples[..pcm_count]);

                let mut input_offset = 0;
                let mut output_frame_offset = 0;
                let frame_count = output.len() / 2;
                while input_offset < pcm_count && output_frame_offset < frame_count {
                    let Ok((consumed, produced)) = self.pcm86.pcm_resampler.resample(
                        &self.pcm86.pcm_resample_input[input_offset..pcm_count],
                        &mut self.pcm86.pcm_resample_output,
                    ) else {
                        break;
                    };
                    let usable = produced.min(frame_count - output_frame_offset);
                    for i in 0..usable {
                        let sample = self.pcm86.pcm_resample_output[i] * volume;
                        output[(output_frame_offset + i) * 2] += sample;
                        output[(output_frame_offset + i) * 2 + 1] += sample;
                    }
                    input_offset += consumed;
                    output_frame_offset += usable;
                    if consumed == 0 {
                        break;
                    }
                }
            }
        }

        self.pending_native.clear();
        self.state.audio_frame_start_cycle = current_cycle;
    }

    /// Creates a snapshot of the current state for save/restore.
    pub fn save_state(&self) -> Soundboard86State {
        let mut state = self.state.clone();
        state.pcm86 = self.pcm86.state.clone();
        state
    }

    /// Restores from a saved state, recreating the ymfm chip.
    pub fn load_state(
        &mut self,
        saved: &Soundboard86State,
        cpu_clock_hz: u32,
        sample_rate: u32,
        current_cycle: u64,
        rhythm_rom: Option<&[u8]>,
    ) {
        self.state = saved.clone();
        self.pcm86.state = saved.pcm86.clone();
        let has_rom = rhythm_rom.is_some();
        // TODO: Save/restore ymfm internal state
        let bridge = ChipBridge::new(cpu_clock_hz, rhythm_rom);
        bridge.busy_end_cycle.set(self.state.busy_end_cycle);
        bridge.current_cycle.set(current_cycle);
        self.chip = Ym2608::new(bridge);
        self.chip.reset();
        self.chip.set_fidelity(FIDELITY);
        self.native_rate = self.chip.sample_rate(YM2608_CLOCK);
        self.resampler = ResamplerFir::new(
            2,
            self.native_rate,
            sample_rate,
            Latency::Sample64,
            Attenuation::Db60,
        );
        self.resample_output
            .resize(self.resampler.buffer_size_output(), 0.0);
        self.rhythm = RhythmSection::new(!has_rom);
        self.sample_rate = sample_rate;
    }
}
