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

// Central mutable state for the MT-32 emulator. Every free function receives
// `&mut MuntState` instead of scattered `this` pointers. The design mirrors
// the approach in `nuked_sc55_oxide::state::Sc55State`.

use crate::{
    enumerations::{DacInputMode, MidiDelayMode, PolyState, ReverbMode},
    structures::{
        ControlROMFeatureSet, ControlROMMap, ControlROMPCMStruct, MemParams, PCMWaveEntry,
        PatchCache,
    },
    tables::Tables,
};

pub(crate) const SAMPLE_RATE: u32 = 32000;
pub(crate) const DEFAULT_MAX_PARTIALS: usize = 32;
pub(crate) const MAX_PARTS: usize = 9;
pub(crate) const MAX_SAMPLES_PER_RUN: usize = 4096;
pub(crate) const SYSEX_BUFFER_SIZE: usize = 1000;
pub(crate) const MAX_STREAM_BUFFER_SIZE: usize = 32768;
pub(crate) const DEFAULT_MIDI_EVENT_QUEUE_SIZE: usize = 1024;
pub(crate) const CONTROL_ROM_SIZE: usize = 64 * 1024;

/// The maximum number of drum timbres in the rhythm part cache.
pub(crate) const DRUM_CACHE_COUNT: usize = 85;

pub(crate) const SYSEX_MANUFACTURER_ROLAND: u8 = 0x41;
pub(crate) const SYSEX_MDL_MT32: u8 = 0x16;
pub(crate) const SYSEX_CMD_RQ1: u8 = 0x11;
pub(crate) const SYSEX_CMD_DT1: u8 = 0x12;
pub(crate) const SYSEX_CMD_WSD: u8 = 0x40;
pub(crate) const SYSEX_CMD_RQD: u8 = 0x41;
pub(crate) const SYSEX_CMD_DAT: u8 = 0x42;
pub(crate) const SYSEX_CMD_EOD: u8 = 0x45;

/// Coarse LPF delay line length (must be a power of 2).
pub(crate) const COARSE_LPF_DELAY_LINE_LENGTH: usize = 8;

/// TVA envelope phases.
/// Note that when entering next_phase(), new_phase is set to phase + 1,
/// and the descriptions/names below refer to new_phase's value.
///
/// In this phase, the base amp (as calculated in calc_basic_amp()) is targeted with an instant time.
/// This phase is entered by reset() only if time[0] != 0.
pub(crate) const TVA_PHASE_BASIC: i32 = 0;
/// In this phase, level[0] is targeted within time[0], and velocity potentially affects time.
pub(crate) const TVA_PHASE_ATTACK: i32 = 1;
/// In this phase, level[1] is targeted within time[1].
pub(crate) const TVA_PHASE_2: i32 = 2;
/// In this phase, level[2] is targeted within time[2].
pub(crate) const TVA_PHASE_3: i32 = 3;
/// In this phase, level[3] is targeted within time[3].
pub(crate) const TVA_PHASE_4: i32 = 4;
/// In this phase, immediately goes to PHASE_RELEASE unless the poly is set to sustain.
/// Aborts the partial if level[3] is 0.
/// Otherwise level[3] is continued, no phase change will occur until some external influence
/// (like pedal release).
pub(crate) const TVA_PHASE_SUSTAIN: i32 = 5;
/// In this phase, 0 is targeted within time[4]
/// (the time calculation is quite different from the other phases).
pub(crate) const TVA_PHASE_RELEASE: i32 = 6;
/// It's PHASE_DEAD, Jim.
pub(crate) const TVA_PHASE_DEAD: i32 = 7;

/// LA32 pair type (master or slave).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum PairType {
    Master = 0,
    Slave = 1,
}

#[derive(Clone, Default)]
pub(crate) struct La32RampState {
    pub(crate) current: u32,
    pub(crate) large_target: u32,
    pub(crate) large_increment: u32,
    pub(crate) descending: bool,
    pub(crate) interrupt_countdown: i32,
    pub(crate) interrupt_raised: bool,
}

#[derive(Clone, Default)]
pub(crate) struct La32FloatWaveGeneratorState {
    pub(crate) active: bool,
    pub(crate) sawtooth_waveform: bool,
    pub(crate) resonance: u8,
    pub(crate) pulse_width: u8,

    /// PCM wave parameters (only valid when generating PCM output).
    pub(crate) pcm_wave_address_offset: u32,
    pub(crate) pcm_wave_length: u32,
    pub(crate) pcm_wave_looped: bool,
    pub(crate) pcm_wave_interpolated: bool,

    /// Internal variables.
    pub(crate) wave_pos: f32,
    pub(crate) last_freq: f32,
    pub(crate) pcm_position: f32,
}

#[derive(Clone, Default)]
pub(crate) struct La32PairState {
    pub(crate) master: La32FloatWaveGeneratorState,
    pub(crate) slave: La32FloatWaveGeneratorState,
    pub(crate) ring_modulated: bool,
    pub(crate) mixed: bool,
    pub(crate) master_output_sample: f32,
    pub(crate) slave_output_sample: f32,
}

#[derive(Clone, Default)]
pub(crate) struct TvaState {
    pub(crate) playing: bool,
    pub(crate) bias_amp_subtraction: i32,
    pub(crate) velo_amp_subtraction: i32,
    pub(crate) key_time_subtraction: i32,
    pub(crate) target: u8,
    pub(crate) phase: i32,
}

#[derive(Clone, Default)]
pub(crate) struct TvfState {
    pub(crate) base_cutoff: u8,
    pub(crate) key_time_subtraction: i32,
    pub(crate) level_mult: u32,
    pub(crate) target: u8,
    pub(crate) phase: u32,
}

#[derive(Clone, Default)]
pub(crate) struct TvpState {
    pub(crate) process_timer_increment: i32,
    pub(crate) counter: i32,
    pub(crate) time_elapsed: u32,

    pub(crate) phase: i32,
    pub(crate) base_pitch: u32,
    pub(crate) target_pitch_offset_without_lfo: i32,
    pub(crate) current_pitch_offset: i32,

    pub(crate) lfo_pitch_offset: i16,
    /// In range -12..36.
    pub(crate) time_keyfollow_subtraction: i8,

    pub(crate) pitch_offset_change_per_big_tick: i16,
    pub(crate) target_pitch_offset_reached_big_tick: u16,
    pub(crate) shifts: u32,

    pub(crate) pitch: u16,
}

#[derive(Clone, Default)]
pub(crate) struct PartialState {
    /// Number of the sample currently being rendered (debug only).
    pub(crate) sample_num: u32,

    /// Pan values. LA-32 receives only 3 bits as a pan setting, but we abuse
    /// these to emulate inverted partial mixing. Doubled for NicePanning mode.
    pub(crate) left_pan_value: i32,
    pub(crate) right_pan_value: i32,

    /// -1 if unassigned.
    pub(crate) owner_part: i32,
    pub(crate) mix_type: i32,
    /// 0 or 1 of a structure pair.
    pub(crate) structure_position: i32,

    /// Only used for PCM partials.
    pub(crate) pcm_num: i32,
    /// Index into the pcm_waves table, or None.
    pub(crate) pcm_wave_index: Option<usize>,

    /// Final pulse width value, with velfollow applied (range 0-255).
    pub(crate) pulse_width_val: i32,

    /// Index of the Poly that owns this partial.
    pub(crate) poly_index: Option<usize>,
    /// Index of the paired Partial, if any.
    pub(crate) pair_index: Option<usize>,
    /// Index into the rhythm_temp table (0-84), set during start_partial for rhythm parts.
    pub(crate) rhythm_temp_index: Option<usize>,

    pub(crate) tva: TvaState,
    pub(crate) tvp: TvpState,
    pub(crate) tvf: TvfState,

    pub(crate) amp_ramp: La32RampState,
    pub(crate) cutoff_modifier_ramp: La32RampState,

    pub(crate) la32_pair: La32PairState,

    pub(crate) patch_cache: PatchCache,
    pub(crate) cache_backup: PatchCache,

    pub(crate) already_outputed: bool,

    /// Whether this partial is active (allocated to a poly).
    pub(crate) active: bool,
}

#[derive(Clone, Default)]
pub(crate) struct PolyStateData {
    /// Index of the owning Part.
    pub(crate) part_index: Option<usize>,
    pub(crate) key: u32,
    pub(crate) velocity: u32,
    pub(crate) active_partial_count: u32,
    pub(crate) sustain: bool,
    pub(crate) state: PolyState,
    /// Indices into the partial pool. Up to 4 partials per poly.
    pub(crate) partial_indices: [Option<usize>; 4],
    /// Linked-list replacement: index of the next Poly, or None.
    pub(crate) next_index: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct PartState {
    /// 0=Part 1, .. 7=Part 8, 8=Rhythm.
    pub(crate) part_num: u32,
    pub(crate) hold_pedal: bool,
    pub(crate) active_partial_count: u32,
    pub(crate) active_non_releasing_poly_count: u32,
    pub(crate) patch_cache: [PatchCache; 4],

    /// Intrusive PolyList replaced by head/tail indices into the poly pool.
    pub(crate) active_polys_first: Option<usize>,
    pub(crate) active_polys_last: Option<usize>,

    /// Name: "Part 1".."Part 8", "Rhythm".
    pub(crate) name: [u8; 8],
    pub(crate) current_instr: [u8; 11],

    /// Values outside the valid range 0..100 imply no override.
    pub(crate) volume_override: u8,
    pub(crate) modulation: u8,
    pub(crate) expression: u8,
    pub(crate) pitch_bend: i32,
    pub(crate) nrpn: bool,
    pub(crate) rpn: u16,
    /// (patchTemp.patch.benderRange * 683) at the time of the last MIDI program change or MIDI data entry.
    pub(crate) pitch_bender_range: u16,

    /// True if this is the rhythm part (index 8).
    pub(crate) is_rhythm: bool,

    /// RhythmPart-specific: cached timbres/settings for each drum note.
    pub(crate) drum_cache: Vec<[PatchCache; 4]>,
}

impl Default for PartState {
    fn default() -> Self {
        Self {
            part_num: 0,
            hold_pedal: false,
            active_partial_count: 0,
            active_non_releasing_poly_count: 0,
            patch_cache: core::array::from_fn(|_| PatchCache::default()),
            active_polys_first: None,
            active_polys_last: None,
            name: [0; 8],
            current_instr: [0; 11],
            volume_override: 0xFF,
            modulation: 0,
            expression: 100,
            pitch_bend: 0,
            nrpn: false,
            rpn: 0xFFFF,
            pitch_bender_range: 0,
            is_rhythm: false,
            drum_cache: Vec::new(),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct PartialManagerState {
    pub(crate) num_reserved_partials_for_part: [u8; MAX_PARTS],
    pub(crate) free_polys: Vec<usize>,
    /// Holds indices of inactive partials in the partial table.
    pub(crate) inactive_partials: Vec<i32>,
    pub(crate) inactive_partial_count: u32,
}

#[derive(Clone, Default)]
pub(crate) struct MidiEvent {
    pub(crate) sysex_data: Option<Vec<u8>>,
    pub(crate) short_message_data: u32,
    pub(crate) timestamp: u32,
}

#[derive(Clone, Default)]
pub(crate) struct MidiEventQueueState {
    pub(crate) ring_buffer: Vec<MidiEvent>,
    pub(crate) ring_buffer_mask: u32,
    pub(crate) start_position: u32,
    pub(crate) end_position: u32,
}

#[derive(Clone)]
pub(crate) struct MidiStreamParserState {
    pub(crate) running_status: u8,
    pub(crate) stream_buffer: Vec<u8>,
    pub(crate) stream_buffer_size: u32,
}

impl Default for MidiStreamParserState {
    fn default() -> Self {
        Self {
            running_status: 0,
            stream_buffer: vec![0; SYSEX_BUFFER_SIZE],
            stream_buffer_size: 0,
        }
    }
}

/// Coarse LPF filter state. Float-only variant for AnalogOutputMode::Coarse.
#[derive(Clone)]
pub(crate) struct CoarseLpfState {
    pub(crate) ring_buffer: [f32; COARSE_LPF_DELAY_LINE_LENGTH],
    pub(crate) ring_buffer_position: u32,
}

impl Default for CoarseLpfState {
    fn default() -> Self {
        Self {
            ring_buffer: [0.0; COARSE_LPF_DELAY_LINE_LENGTH],
            ring_buffer_position: 0,
        }
    }
}

/// Analog stage state. Float-only, coarse mode only.
#[derive(Clone, Default)]
pub(crate) struct AnalogState {
    pub(crate) left_channel_lpf: CoarseLpfState,
    pub(crate) right_channel_lpf: CoarseLpfState,
    pub(crate) synth_gain: f32,
    pub(crate) reverb_gain: f32,
    pub(crate) old_mt32_analog_lpf: bool,
}

/// Ring buffer used by allpass and comb filters inside the reverb.
#[derive(Clone, Default)]
pub(crate) struct ReverbRingBuffer {
    pub(crate) buffer: Vec<f32>,
    pub(crate) size: u32,
    pub(crate) index: u32,
}

/// Allpass filter state inside the reverb.
#[derive(Clone, Default)]
pub(crate) struct ReverbAllpassState {
    pub(crate) ring: ReverbRingBuffer,
}

/// Comb filter state inside the reverb.
#[derive(Clone, Default)]
pub(crate) struct ReverbCombState {
    pub(crate) ring: ReverbRingBuffer,
    pub(crate) filter_factor: u8,
    pub(crate) feedback_factor: u8,
}

/// Tap-delay comb filter state (mode 3 reverb).
#[derive(Clone, Default)]
pub(crate) struct ReverbTapDelayCombState {
    pub(crate) comb: ReverbCombState,
    pub(crate) out_l: u32,
    pub(crate) out_r: u32,
}

/// Delay-with-LPF state (mode 0/1/2 entrance filter).
#[derive(Clone, Default)]
pub(crate) struct ReverbDelayWithLpfState {
    pub(crate) comb: ReverbCombState,
    pub(crate) amp: u8,
}

/// Per-mode reverb state. Virtual dispatch replaced by enum.
#[derive(Clone, Default)]
pub(crate) enum BReverbModelState {
    /// Modes 0 (Room), 1 (Hall), 2 (Plate): 3 allpasses + entrance delay + 3 combs.
    Standard {
        allpasses: Vec<ReverbAllpassState>,
        entrance_delay: ReverbDelayWithLpfState,
        combs: Vec<ReverbCombState>,
        dry_amp: u8,
        wet_level: u8,
        mt32_compatible: bool,
        mode: ReverbMode,
        opened: bool,
    },
    /// Mode 3 (Tap delay): no allpasses, single tap delay comb.
    TapDelay {
        tap_delay_comb: ReverbTapDelayCombState,
        dry_amp: u8,
        wet_level: u8,
        mt32_compatible: bool,
        opened: bool,
    },
    #[default]
    Closed,
}

/// Renderer temporary buffers. Float-only.
#[derive(Clone)]
pub(crate) struct RendererState {
    pub(crate) tmp_non_reverb_left: Vec<f32>,
    pub(crate) tmp_non_reverb_right: Vec<f32>,
    pub(crate) tmp_reverb_dry_left: Vec<f32>,
    pub(crate) tmp_reverb_dry_right: Vec<f32>,
    pub(crate) tmp_reverb_wet_left: Vec<f32>,
    pub(crate) tmp_reverb_wet_right: Vec<f32>,
    pub(crate) tmp_partial_left: Vec<f32>,
    pub(crate) tmp_partial_right: Vec<f32>,
}

impl Default for RendererState {
    fn default() -> Self {
        Self {
            tmp_non_reverb_left: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_non_reverb_right: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_reverb_dry_left: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_reverb_dry_right: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_reverb_wet_left: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_reverb_wet_right: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_partial_left: vec![0.0; MAX_SAMPLES_PER_RUN],
            tmp_partial_right: vec![0.0; MAX_SAMPLES_PER_RUN],
        }
    }
}

/// Memory region descriptor for sysex-addressable memory.
#[derive(Clone, Default)]
pub(crate) struct MemoryRegionDescriptor {
    pub(crate) start_addr: u32,
    pub(crate) entry_size: u32,
    pub(crate) entries: u32,
}

/// ROM data: parsed control ROM + PCM ROM data.
#[derive(Clone)]
pub(crate) struct RomData {
    pub(crate) control_rom_data: Vec<u8>,
    pub(crate) pcm_rom_data: Vec<i16>,
    pub(crate) control_rom_map: Option<&'static ControlROMMap>,
    pub(crate) control_rom_features: ControlROMFeatureSet,
    pub(crate) pcm_waves: Vec<PCMWaveEntry>,
    pub(crate) pcm_rom_structs: Vec<ControlROMPCMStruct>,
    /// Padded timbre max table from the control ROM.
    pub(crate) padded_timbre_max_table: Vec<u8>,
    /// Sound group index: for each standard timbre, the index of its sound group.
    pub(crate) sound_group_ix: [u8; 128],
    pub(crate) sound_group_names: Vec<[u8; 9]>,
}

impl Default for RomData {
    fn default() -> Self {
        Self {
            control_rom_data: vec![0; CONTROL_ROM_SIZE],
            pcm_rom_data: Vec::new(),
            control_rom_map: None,
            control_rom_features: ControlROMFeatureSet::default(),
            pcm_waves: Vec::new(),
            pcm_rom_structs: Vec::new(),
            padded_timbre_max_table: Vec::new(),
            sound_group_ix: [0; 128],
            sound_group_names: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ExtensionsState {
    pub(crate) master_tune_pitch_delta: i32,
    pub(crate) master_volume_override: u8,
    pub(crate) nice_amp_ramp: bool,
    pub(crate) nice_panning: bool,
    pub(crate) nice_partial_mixing: bool,
    /// Reverse mapping of assigned parts per MIDI channel.
    /// Value above 8 means that the channel is not assigned.
    pub(crate) chan_table: [[u8; MAX_PARTS]; 16],
    /// Index of Part in chan_table that failed to play and required partial abortion.
    pub(crate) aborting_part_ix: u32,
}

impl Default for ExtensionsState {
    fn default() -> Self {
        Self {
            master_tune_pitch_delta: 0,
            master_volume_override: 0xFF,
            nice_amp_ramp: true,
            nice_panning: false,
            nice_partial_mixing: false,
            chan_table: [[0xFF; MAX_PARTS]; 16],
            aborting_part_ix: 0,
        }
    }
}

/// The top-level emulator state struct. All mutable state lives here.
#[derive(Clone)]
pub(crate) struct MuntState {
    /// Synth open status.
    pub(crate) opened: bool,
    /// Synth activated status.
    pub(crate) activated: bool,

    /// Main sysex-addressable RAM and its power-on defaults.
    pub(crate) mt32_ram: MemParams,
    pub(crate) mt32_default: MemParams,

    /// ROM data (loaded at open time).
    pub(crate) rom: RomData,
    pub(crate) tables: Tables,

    /// Parts: indices 0..7 = melodic Part 1..8, index 8 = Rhythm.
    pub(crate) parts: [PartState; MAX_PARTS],

    /// Global partial pool. Sized to `partial_count` at open time.
    pub(crate) partials: Vec<PartialState>,
    pub(crate) partial_count: u32,

    /// Global poly pool. Sized to `partial_count * MAX_PARTS` at open time.
    pub(crate) polys: Vec<PolyStateData>,

    /// Partial manager bookkeeping.
    pub(crate) partial_manager: PartialManagerState,

    /// Reverb models for the 4 modes. Index = ReverbMode as usize.
    pub(crate) reverb_models: [BReverbModelState; 4],
    /// Index of the currently active reverb model.
    pub(crate) active_reverb_model: usize,
    pub(crate) reverb_overridden: bool,

    /// MIDI event queue.
    pub(crate) midi_queue: MidiEventQueueState,
    pub(crate) last_received_midi_event_timestamp: u32,
    pub(crate) rendered_sample_count: u32,

    /// MIDI stream parser.
    pub(crate) midi_stream_parser: MidiStreamParserState,

    /// When a partial needs to be aborted to free it up for use by a new Poly,
    /// the controller will busy-loop waiting for the sound to finish.
    /// We emulate this by delaying new MIDI events processing until abortion finishes.
    pub(crate) aborting_poly_index: Option<usize>,

    /// Analog output stage (coarse float mode).
    pub(crate) analog: AnalogState,

    /// Renderer temporary buffers.
    pub(crate) renderer: RendererState,

    /// Configuration.
    pub(crate) midi_delay_mode: MidiDelayMode,
    pub(crate) dac_input_mode: DacInputMode,
    pub(crate) output_gain: f32,
    pub(crate) reverb_output_gain: f32,
    pub(crate) reversed_stereo_enabled: bool,

    pub(crate) extensions: ExtensionsState,

    /// Memory region descriptors for sysex address mapping.
    pub(crate) memory_regions: MemoryRegionDescriptors,

    /// Lehmer64 PRNG state for pitch deviation noise.
    pub(crate) prng_state: u128,
}

/// Memory region descriptors.
#[derive(Clone, Default)]
pub(crate) struct MemoryRegionDescriptors {
    pub(crate) patch_temp: MemoryRegionDescriptor,
    pub(crate) rhythm_temp: MemoryRegionDescriptor,
    pub(crate) timbre_temp: MemoryRegionDescriptor,
    pub(crate) patches: MemoryRegionDescriptor,
    pub(crate) timbres: MemoryRegionDescriptor,
    pub(crate) system: MemoryRegionDescriptor,
    pub(crate) reset: MemoryRegionDescriptor,
}

impl Default for MuntState {
    fn default() -> Self {
        Self {
            opened: false,
            activated: false,

            mt32_ram: MemParams::default(),
            mt32_default: MemParams::default(),

            rom: RomData::default(),
            tables: Tables::new(),

            parts: core::array::from_fn(|_| PartState::default()),
            partials: Vec::new(),
            partial_count: DEFAULT_MAX_PARTIALS as u32,
            polys: Vec::new(),

            partial_manager: PartialManagerState::default(),

            reverb_models: core::array::from_fn(|_| BReverbModelState::default()),
            active_reverb_model: 0,
            reverb_overridden: false,

            midi_queue: MidiEventQueueState::default(),
            last_received_midi_event_timestamp: 0,
            rendered_sample_count: 0,

            midi_stream_parser: MidiStreamParserState::default(),

            aborting_poly_index: None,

            analog: AnalogState::default(),
            renderer: RendererState::default(),

            midi_delay_mode: MidiDelayMode::DelayShortMessagesOnly,
            dac_input_mode: DacInputMode::Nice,
            output_gain: 1.0,
            reverb_output_gain: 1.0,
            reversed_stereo_enabled: false,

            extensions: ExtensionsState::default(),

            memory_regions: MemoryRegionDescriptors::default(),

            // https://xkcd.com/221
            //
            // In this case this is fine.
            // We do not need real randomness here, only input independent
            // randomness for small pitch deviations.
            prng_state: 0x12345678_9ABCDEF0_13579BDF_2468ACE0u128,
        }
    }
}
