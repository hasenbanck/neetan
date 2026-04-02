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

use crate::{state::*, structures::*, tva::tva_recalc_sustain};

static LOWER_DURATION_TO_DIVISOR: [u16; 8] =
    [34078, 37162, 40526, 44194, 48194, 52556, 57312, 62499];

/// These values represent unique options with no consistent pattern, so we have to use something
/// like a table in any case.
/// The table matches exactly what the manual claims (when divided by 8192):
/// -1, -1/2, -1/4, 0, 1/8, 1/4, 3/8, 1/2, 5/8, 3/4, 7/8, 1, 5/4, 3/2, 2, s1, s2
/// ...except for the last two entries, which are supposed to be "1 cent above 1" and "2 cents
/// above 1", respectively. They can only be roughly approximated with this integer math.
static PITCH_KEYFOLLOW_MULT: [i16; 17] = [
    -8192, -4096, -2048, 0, 1024, 2048, 3072, 4096, 5120, 6144, 7168, 8192, 10240, 12288, 16384,
    8198, 8226,
];

/// Note: Keys < 60 use keyToPitchTable[60 - key], keys >= 60 use keyToPitchTable[key - 60].
static KEY_TO_PITCH_TABLE: [u16; 68] = [
    0, 341, 683, 1024, 1365, 1707, 2048, 2389, 2731, 3072, 3413, 3755, 4096, 4437, 4779, 5120,
    5461, 5803, 6144, 6485, 6827, 7168, 7509, 7851, 8192, 8533, 8875, 9216, 9557, 9899, 10240,
    10581, 10923, 11264, 11605, 11947, 12288, 12629, 12971, 13312, 13653, 13995, 14336, 14677,
    15019, 15360, 15701, 16043, 16384, 16725, 17067, 17408, 17749, 18091, 18432, 18773, 19115,
    19456, 19797, 20139, 20480, 20821, 21163, 21504, 21845, 22187, 22528, 22869,
];

/// We want to do processing 4000 times per second.
const NOMINAL_PROCESS_TIMER_PERIOD_SAMPLES: i32 = SAMPLE_RATE as i32 / 4000;

// In all hardware units we emulate, the main clock frequency of the MCU is 12MHz.
// However, the MCU used in the 3rd-gen sound modules (like CM-500 and LAPC-N)
// is significantly faster. Importantly, the software timer also works faster,
// yet this fact has been seemingly missed. To be more specific, the software timer
// ticks each 8 "state times", and 1 state time equals to 3 clock periods
// for 8095 and 8098 but 2 clock periods for 80C198. That is, on MT-32 and CM-32L,
// the software timer tick rate is 12,000,000 / 3 / 8 = 500kHz, but on the 3rd-gen
// devices it's 12,000,000 / 2 / 8 = 750kHz instead.

/// For 1st- and 2nd-gen devices, the timer ticks at 500kHz. This is how much to increment
/// timeElapsed once 16 samples passes. We multiply by 16 to get rid of the fraction
/// and deal with just integers.
const PROCESS_TIMER_TICKS_PER_SAMPLE_X16_1N2_GEN: i32 = (500000 << 4) / SAMPLE_RATE as i32;
/// For 3rd-gen devices, the timer ticks at 750kHz. This is how much to increment
/// timeElapsed once 16 samples passes. We multiply by 16 to get rid of the fraction
/// and deal with just integers.
const PROCESS_TIMER_TICKS_PER_SAMPLE_X16_3_GEN: i32 = (750000 << 4) / SAMPLE_RATE as i32;

fn get_process_timer_ticks_per_sample_x16(state: &MuntState) -> i32 {
    if state.rom.control_rom_features.quirk_fast_pitch_changes {
        return PROCESS_TIMER_TICKS_PER_SAMPLE_X16_3_GEN;
    }
    PROCESS_TIMER_TICKS_PER_SAMPLE_X16_1N2_GEN
}

fn key_to_pitch(key: u32) -> i16 {
    // We're using a table to do: return round_to_nearest_or_even((key - 60) * (4096.0 / 12.0))
    // Banker's rounding is just slightly annoying to do in C++
    let k = key as i32;
    let pitch = KEY_TO_PITCH_TABLE[(k - 60).unsigned_abs() as usize];
    if key < 60 {
        -(pitch as i16)
    } else {
        pitch as i16
    }
}

fn coarse_to_pitch(coarse: u8) -> i32 {
    (coarse as i32 - 36) * 4096 / 12 // One semitone per coarse offset
}

fn fine_to_pitch(fine: u8) -> i32 {
    (fine as i32 - 50) * 4096 / 1200 // One cent per fine offset
}

fn calc_base_pitch(
    state: &MuntState,
    partial_index: usize,
    patch_temp: &PatchTemp,
    key: u32,
) -> u32 {
    let features = &state.rom.control_rom_features;
    let partial_param = &state.partials[partial_index].patch_cache.src_partial;

    let mut base_pitch = key_to_pitch(key) as i32;
    base_pitch =
        (base_pitch * PITCH_KEYFOLLOW_MULT[partial_param.wg.pitch_keyfollow as usize] as i32) >> 13; // PORTABILITY NOTE: Assumes arithmetic shift
    base_pitch += coarse_to_pitch(partial_param.wg.pitch_coarse);
    base_pitch += fine_to_pitch(partial_param.wg.pitch_fine);
    if features.quirk_key_shift {
        // NOTE:Mok: This is done on MT-32, but not LAPC-I:
        base_pitch += coarse_to_pitch(patch_temp.patch.key_shift + 12);
    }
    base_pitch += fine_to_pitch(patch_temp.patch.fine_tune);

    let control_rom_pcm_struct = get_control_rom_pcm_struct(state, partial_index);
    if let Some(pcm_struct) = control_rom_pcm_struct {
        base_pitch += ((pcm_struct.pitch_msb as i32) << 8) | (pcm_struct.pitch_lsb as i32);
    } else if (partial_param.wg.waveform & 1) == 0 {
        base_pitch += 37133; // This puts Middle C at around 261.64Hz (assuming no other modifications, masterTune of 64, etc.)
    } else {
        // Sawtooth waves are effectively double the frequency of square waves.
        // Thus we add 4096 less than for square waves here, which results in halving the frequency.
        base_pitch += 33037;
    }

    // MT-32 GEN0 does 16-bit calculations here, allowing an integer overflow.
    // This quirk is observable playing the patch defined for timbre "HIT BOTTOM" in Larry 3.
    // Note, the upper bound isn't checked either.
    if features.quirk_base_pitch_overflow {
        base_pitch &= 0xFFFF;
    } else {
        base_pitch = base_pitch.clamp(0, 59392);
    }
    base_pitch as u32
}

fn get_control_rom_pcm_struct(
    state: &MuntState,
    partial_index: usize,
) -> Option<&ControlROMPCMStruct> {
    let pcm_wave_index = state.partials[partial_index].pcm_wave_index?;
    let pcm_wave = &state.rom.pcm_waves[pcm_wave_index];
    let struct_index = pcm_wave.control_rom_pcm_struct?;
    Some(&state.rom.pcm_rom_structs[struct_index])
}

fn is_pcm(state: &MuntState, partial_index: usize) -> bool {
    state.partials[partial_index].pcm_wave_index.is_some()
}

fn calc_velo_mult(velo_sensitivity: u8, velocity: u32) -> u32 {
    if velo_sensitivity == 0 {
        return 21845; // aka floor(4096 / 12 * 64), aka ~64 semitones
    }
    let reversed_velocity = 127 - velocity;
    let scaled_reversed_velocity = if velo_sensitivity > 3 {
        // Note that on CM-32L/LAPC-I veloSensitivity is never > 3, since it's clipped to 3 by
        // the max tables.
        // MT-32 GEN0 has a bug here that leads to unspecified behaviour. We assume it is as
        // follows.
        (reversed_velocity << 8) >> ((3u32.wrapping_sub(velo_sensitivity as u32)) & 0x1F)
    } else {
        reversed_velocity << (5 + velo_sensitivity)
    };
    // When velocity is 127, the multiplier is 21845, aka ~64 semitones (regardless of
    // veloSensitivity).
    // The lower the velocity, the lower the multiplier. The veloSensitivity determines the amount
    // decreased per velocity value.
    // The minimum multiplier on CM-32L/LAPC-I (with velocity 0, veloSensitivity 3) is 170
    // (~half a semitone).
    ((32768 - scaled_reversed_velocity) * 21845) >> 15
}

fn calc_target_pitch_offset_without_lfo(
    partial_param: &PartialParam,
    level_index: usize,
    velocity: u32,
) -> i32 {
    let velo_mult = calc_velo_mult(partial_param.pitch_env.velo_sensitivity, velocity) as i32;
    let mut target_pitch_offset_without_lfo =
        partial_param.pitch_env.level[level_index] as i32 - 50;
    target_pitch_offset_without_lfo =
        (target_pitch_offset_without_lfo * velo_mult) >> (16 - partial_param.pitch_env.depth); // PORTABILITY NOTE: Assumes arithmetic shift
    target_pitch_offset_without_lfo
}

// Calls tva_recalc_sustain with the context derived from state.
// The C++ code calls partial->getTVA()->recalcSustain() which uses stored pointers.
// In the Rust port we must gather the parameters and pass them explicitly.
fn call_tva_recalc_sustain(state: &mut MuntState, partial_index: usize) {
    let part_index = state.partials[partial_index].owner_part;
    if part_index < 0 {
        return;
    }
    let part_index = part_index as usize;

    // Read rhythm_temp for this partial using the stored index from start_partial.
    let rhythm_temp = state.partials[partial_index]
        .rhythm_temp_index
        .map(|i| state.mt32_ram.rhythm_temp(i));

    tva_recalc_sustain(state, partial_index, part_index, rhythm_temp.as_ref());
}

pub(crate) fn tvp_reset(state: &mut MuntState, partial_index: usize, part_index: usize) {
    let patch_temp = state.mt32_ram.patch_temp(part_index);

    let poly_index = state.partials[partial_index]
        .poly_index
        .expect("tvp_reset: partial must have a poly");
    let key = state.polys[poly_index].key;
    let velocity = state.polys[poly_index].velocity;

    // FIXME: We're using a per-TVP timer instead of a system-wide one for convenience.
    let tvp = &mut state.partials[partial_index].tvp;
    tvp.time_elapsed = 0;
    tvp.process_timer_increment = 0;

    let base_pitch = calc_base_pitch(state, partial_index, &patch_temp, key);
    let partial_param = &state.partials[partial_index].patch_cache.src_partial;
    let current_pitch_offset = calc_target_pitch_offset_without_lfo(partial_param, 0, velocity);
    let time_keyfollow = partial_param.pitch_env.time_keyfollow;

    let tvp = &mut state.partials[partial_index].tvp;
    tvp.base_pitch = base_pitch;
    tvp.current_pitch_offset = current_pitch_offset;
    tvp.target_pitch_offset_without_lfo = current_pitch_offset;
    tvp.phase = 0;

    if time_keyfollow != 0 {
        tvp.time_keyfollow_subtraction = ((key as i32 - 60) >> (5 - time_keyfollow)) as i8;
        // PORTABILITY NOTE: Assumes arithmetic shift
    } else {
        tvp.time_keyfollow_subtraction = 0;
    }
    tvp.lfo_pitch_offset = 0;
    tvp.counter = 0;
    tvp.pitch = base_pitch as u16;

    // These don't really need to be initialised, but it aids debugging.
    tvp.pitch_offset_change_per_big_tick = 0;
    tvp.target_pitch_offset_reached_big_tick = 0;
    tvp.shifts = 0;
}

fn tvp_update_pitch(state: &mut MuntState, partial_index: usize) {
    let tvp = &state.partials[partial_index].tvp;
    let mut new_pitch = tvp.base_pitch as i32 + tvp.current_pitch_offset;

    let pcm = is_pcm(state, partial_index);
    if !pcm || get_control_rom_pcm_struct(state, partial_index).is_some_and(|s| (s.len & 0x01) == 0)
    {
        // FIXME: Use !partial->pcmWaveEntry->unaffectedByMasterTune instead
        // FIXME: There are various bugs not yet emulated 171 is ~half a semitone.
        new_pitch += state.extensions.master_tune_pitch_delta;
    }
    let pitch_bender_enabled = state.partials[partial_index]
        .patch_cache
        .src_partial
        .wg
        .pitch_bender_enabled;
    if (pitch_bender_enabled & 1) != 0 {
        let part_index = state.partials[partial_index].owner_part;
        if part_index >= 0 {
            new_pitch += state.parts[part_index as usize].pitch_bend;
        }
    }

    // MT-32 GEN0 does 16-bit calculations here, allowing an integer overflow.
    // This quirk is exploited e.g. in Colonel's Bequest timbres "Lightning" and "SwmpBackgr".
    let features = &state.rom.control_rom_features;
    if features.quirk_pitch_envelope_overflow {
        new_pitch &= 0xFFFF;
    } else if new_pitch < 0 {
        new_pitch = 0;
    }
    // This check is present in every unit.
    if new_pitch > 59392 {
        new_pitch = 59392;
    }
    state.partials[partial_index].tvp.pitch = new_pitch as u16;

    // FIXME: We're doing this here because that's what the CM-32L does - we should probably move
    // this somewhere more appropriate in future.
    call_tva_recalc_sustain(state, partial_index);
}

fn tvp_target_pitch_offset_reached(state: &mut MuntState, partial_index: usize) {
    let tvp = &state.partials[partial_index].tvp;
    let current_pitch_offset = tvp.target_pitch_offset_without_lfo + tvp.lfo_pitch_offset as i32;
    state.partials[partial_index].tvp.current_pitch_offset = current_pitch_offset;

    let phase = state.partials[partial_index].tvp.phase;
    match phase {
        3 | 4 => {
            let part_index = state.partials[partial_index].owner_part;
            let modulation = if part_index >= 0 {
                state.parts[part_index as usize].modulation
            } else {
                0
            };
            let pp = &state.partials[partial_index].patch_cache.src_partial;
            let lfo_mod_sensitivity = pp.pitch_lfo.mod_sensitivity;
            let lfo_depth = pp.pitch_lfo.depth;
            let lfo_rate = pp.pitch_lfo.rate;

            let mut new_lfo_pitch_offset = (modulation as i32 * lfo_mod_sensitivity as i32) >> 7;
            new_lfo_pitch_offset = (new_lfo_pitch_offset + lfo_depth as i32) << 1;
            if state.partials[partial_index]
                .tvp
                .pitch_offset_change_per_big_tick
                > 0
            {
                // Go in the opposite direction to last time
                new_lfo_pitch_offset = -new_lfo_pitch_offset;
            }
            state.partials[partial_index].tvp.lfo_pitch_offset = new_lfo_pitch_offset as i16;
            let target_pitch_offset = state.partials[partial_index]
                .tvp
                .target_pitch_offset_without_lfo
                + state.partials[partial_index].tvp.lfo_pitch_offset as i32;
            tvp_setup_pitch_change(state, partial_index, target_pitch_offset, 101 - lfo_rate);
            tvp_update_pitch(state, partial_index);
        }
        6 => {
            tvp_update_pitch(state, partial_index);
        }
        _ => {
            tvp_next_phase(state, partial_index);
        }
    }
}

fn tvp_next_phase(state: &mut MuntState, partial_index: usize) {
    state.partials[partial_index].tvp.phase += 1;
    let phase = state.partials[partial_index].tvp.phase;
    let env_index = if phase == 6 { 4 } else { phase } as usize;

    let poly_index = state.partials[partial_index]
        .poly_index
        .expect("tvp_next_phase: partial must have a poly");
    let velocity = state.polys[poly_index].velocity;

    let partial_param = &state.partials[partial_index].patch_cache.src_partial;
    let target_pitch_offset_without_lfo =
        calc_target_pitch_offset_without_lfo(partial_param, env_index, velocity);
    let env_time = partial_param.pitch_env.time[env_index - 1];

    // pitch we'll reach at the end
    state.partials[partial_index]
        .tvp
        .target_pitch_offset_without_lfo = target_pitch_offset_without_lfo;

    let time_keyfollow_subtraction = state.partials[partial_index].tvp.time_keyfollow_subtraction;
    let mut change_duration = env_time as i32;
    change_duration -= time_keyfollow_subtraction as i32;
    if change_duration > 0 {
        tvp_setup_pitch_change(
            state,
            partial_index,
            target_pitch_offset_without_lfo,
            change_duration as u8,
        ); // changeDuration between 0 and 112 now
        tvp_update_pitch(state, partial_index);
    } else {
        tvp_target_pitch_offset_reached(state, partial_index);
    }
}

// Shifts val to the left until bit 31 is 1 and returns the number of shifts
fn normalise(val: &mut u32) -> u8 {
    let mut left_shifts: u8 = 0;
    while left_shifts < 31 {
        if (*val & 0x80000000) != 0 {
            break;
        }
        *val <<= 1;
        left_shifts += 1;
    }
    left_shifts
}

fn tvp_setup_pitch_change(
    state: &mut MuntState,
    partial_index: usize,
    target_pitch_offset: i32,
    change_duration: u8,
) {
    let current_pitch_offset = state.partials[partial_index].tvp.current_pitch_offset;
    let negative_delta = target_pitch_offset < current_pitch_offset;
    let mut pitch_offset_delta = target_pitch_offset - current_pitch_offset;
    if !(-32768..=32767).contains(&pitch_offset_delta) {
        pitch_offset_delta = 32767;
    }
    if negative_delta {
        pitch_offset_delta = -pitch_offset_delta;
    }
    // We want to maximise the number of bits of the Bit16s "pitchOffsetChangePerBigTick" we use
    // in order to get the best possible precision later
    let mut abs_pitch_offset_delta = ((pitch_offset_delta as u32) & 0xFFFF) << 16;
    let normalisation_shifts = normalise(&mut abs_pitch_offset_delta);
    // FIXME: Double-check: normalisationShifts is usually between 0 and 15 here, unless the
    // delta is 0, in which case it's 31
    abs_pitch_offset_delta >>= 1; // Make room for the sign bit

    let change_duration_minus_one = change_duration.wrapping_sub(1);
    // changeDuration's now between 0 and 111
    let upper_duration = (change_duration_minus_one >> 3) as u32;
    // upperDuration's now between 0 and 13
    let shifts = normalisation_shifts as u32 + upper_duration + 2;
    let divisor = LOWER_DURATION_TO_DIVISOR[(change_duration_minus_one & 7) as usize];
    let mut new_pitch_offset_change_per_big_tick =
        (((abs_pitch_offset_delta & 0xFFFF0000) / divisor as u32) >> 1) as i16;
    // Result now fits within 15 bits. FIXME: Check nothing's getting sign-extended incorrectly
    if negative_delta {
        new_pitch_offset_change_per_big_tick = -new_pitch_offset_change_per_big_tick;
    }

    let tvp = &mut state.partials[partial_index].tvp;
    tvp.pitch_offset_change_per_big_tick = new_pitch_offset_change_per_big_tick;
    tvp.shifts = shifts;

    let current_big_tick = (tvp.time_elapsed >> 8) as i32;
    let shift_amount = 12i32 - upper_duration as i32;
    let mut duration_in_big_ticks = if shift_amount >= 0 {
        (divisor as i32) >> shift_amount
    } else {
        0
    };
    if duration_in_big_ticks > 32767 {
        duration_in_big_ticks = 32767;
    }
    // The result of the addition may exceed 16 bits, but wrapping is fine and intended here.
    tvp.target_pitch_offset_reached_big_tick = (current_big_tick + duration_in_big_ticks) as u16;
}

pub(crate) fn tvp_start_decay(state: &mut MuntState, partial_index: usize) {
    let tvp = &mut state.partials[partial_index].tvp;
    tvp.phase = 5;
    tvp.lfo_pitch_offset = 0;
    tvp.target_pitch_offset_reached_big_tick = (tvp.time_elapsed >> 8) as u16;
    // FIXME: Afaict there's no good reason for this - check
}

// Lehmer64 PRNG for input-independent pitch deviation noise.
fn lehmer64(state: &mut u128) -> u64 {
    *state = state.wrapping_mul(0xDA942042E4DD58B5);
    (*state >> 64) as u64
}

pub(crate) fn tvp_next_pitch(state: &mut MuntState, partial_index: usize) -> u16 {
    // We emulate MCU software timer using these counter and processTimerIncrement variables.
    // The value of NOMINAL_PROCESS_TIMER_PERIOD_SAMPLES approximates the period in samples
    // between subsequent firings of the timer that normally occur.
    // However, accurate emulation is quite complicated because the timer is not guaranteed to
    // fire in time.
    // This makes pitch variations on real unit non-deterministic and dependent on various
    // factors.
    let counter = state.partials[partial_index].tvp.counter;
    if counter == 0 {
        let process_timer_increment = state.partials[partial_index].tvp.process_timer_increment;
        let time_elapsed = state.partials[partial_index].tvp.time_elapsed;
        state.partials[partial_index].tvp.time_elapsed =
            (time_elapsed + process_timer_increment as u32) & 0x00FFFFFF;
        // This roughly emulates pitch deviations observed on real units when playing a single
        // partial that uses TVP/LFO.
        let rand_val = lehmer64(&mut state.prng_state);
        let new_counter = NOMINAL_PROCESS_TIMER_PERIOD_SAMPLES + (rand_val as i32 & 3);
        state.partials[partial_index].tvp.counter = new_counter;
        let process_timer_ticks_per_sample_x16 = get_process_timer_ticks_per_sample_x16(state);
        state.partials[partial_index].tvp.process_timer_increment =
            (process_timer_ticks_per_sample_x16 * new_counter) >> 4;
        tvp_process(state, partial_index);
    }
    state.partials[partial_index].tvp.counter -= 1;
    state.partials[partial_index].tvp.pitch
}

fn tvp_process(state: &mut MuntState, partial_index: usize) {
    let phase = state.partials[partial_index].tvp.phase;
    if phase == 0 {
        tvp_target_pitch_offset_reached(state, partial_index);
        return;
    }
    if phase == 5 {
        tvp_next_phase(state, partial_index);
        return;
    }
    if phase > 7 {
        tvp_update_pitch(state, partial_index);
        return;
    }

    let time_elapsed = state.partials[partial_index].tvp.time_elapsed;
    let target_big_tick = state.partials[partial_index]
        .tvp
        .target_pitch_offset_reached_big_tick;
    let negative_big_ticks_remaining =
        ((time_elapsed >> 8).wrapping_sub(target_big_tick as u32)) as i16;
    if negative_big_ticks_remaining >= 0 {
        // We've reached the time for a phase change
        tvp_target_pitch_offset_reached(state, partial_index);
        return;
    }
    // FIXME: Write explanation for this stuff
    // NOTE: Value of shifts may happily exceed the maximum of 31 specified for the 8095 MCU.
    // We assume the device performs a shift with the rightmost 5 bits of the counter regardless
    // of argument size, since shift instructions of any size have the same maximum.
    let mut right_shifts = state.partials[partial_index].tvp.shifts as i32;
    let mut remaining = negative_big_ticks_remaining as i32;
    if right_shifts > 13 {
        right_shifts -= 13;
        remaining >>= right_shifts & 0x1F;
        // PORTABILITY NOTE: Assumes arithmetic shift
        right_shifts = 13;
    }
    let pitch_offset_change_per_big_tick = state.partials[partial_index]
        .tvp
        .pitch_offset_change_per_big_tick;
    let new_result = (remaining * pitch_offset_change_per_big_tick as i32) >> (right_shifts & 0x1F); // PORTABILITY NOTE: Assumes arithmetic shift
    let target_pitch_offset_without_lfo = state.partials[partial_index]
        .tvp
        .target_pitch_offset_without_lfo;
    let lfo_pitch_offset = state.partials[partial_index].tvp.lfo_pitch_offset;
    state.partials[partial_index].tvp.current_pitch_offset =
        new_result + target_pitch_offset_without_lfo + lfo_pitch_offset as i32;
    tvp_update_pitch(state, partial_index);
}
