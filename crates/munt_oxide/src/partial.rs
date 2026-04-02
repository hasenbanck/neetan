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
    state::{MuntState, PairType},
    structures::PatchCache,
};

static PAN_NUMERATOR_MASTER: [u8; 15] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7];
static PAN_NUMERATOR_SLAVE: [u8; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7];

pub(crate) fn is_active(state: &MuntState, partial_index: usize) -> bool {
    state.partials[partial_index].owner_part > -1
}

pub(crate) fn deactivate(state: &mut MuntState, partial_index: usize) {
    if !is_active(state, partial_index) {
        return;
    }
    state.partials[partial_index].owner_part = -1;

    crate::partial_manager::partial_deactivated(state, partial_index as i32);

    if let Some(poly_index) = state.partials[partial_index].poly_index {
        crate::poly::poly_partial_deactivated(state, poly_index, partial_index);
    }

    if is_ring_modulating_slave(state, partial_index) {
        let pair_index = state.partials[partial_index].pair_index.unwrap();
        state.partials[pair_index]
            .la32_pair
            .deactivate(PairType::Slave);
    } else {
        state.partials[partial_index]
            .la32_pair
            .deactivate(PairType::Master);
        if has_ring_modulating_slave(state, partial_index) {
            let pair_index = state.partials[partial_index].pair_index.unwrap();
            deactivate(state, pair_index);
            state.partials[partial_index].pair_index = None;
        }
    }
    if let Some(pair_index) = state.partials[partial_index].pair_index {
        state.partials[pair_index].pair_index = None;
    }
}

pub(crate) fn start_partial(
    state: &mut MuntState,
    partial_index: usize,
    part_index: usize,
    poly_index: usize,
    use_patch_cache: &PatchCache,
    rhythm_temp_index: Option<usize>,
    pair_partial_index: Option<usize>,
) {
    state.partials[partial_index].patch_cache = use_patch_cache.clone();
    state.partials[partial_index].poly_index = Some(poly_index);
    state.partials[partial_index].rhythm_temp_index = rhythm_temp_index;
    state.partials[partial_index].mix_type =
        state.partials[partial_index].patch_cache.structure_mix as i32;
    state.partials[partial_index].structure_position =
        state.partials[partial_index].patch_cache.structure_position;

    let pan_setting = if let Some(rt_index) = rhythm_temp_index {
        state.mt32_ram.rhythm_temp(rt_index).panpot
    } else {
        state.mt32_ram.patch_temp(part_index).panpot
    };
    let mut pan_setting = pan_setting;

    let mut mix_type = state.partials[partial_index].mix_type;
    let structure_position = state.partials[partial_index].structure_position;
    let mut pair_partial = pair_partial_index;

    if mix_type == 3 {
        if structure_position == 0 {
            pan_setting = PAN_NUMERATOR_MASTER[pan_setting as usize] << 1;
        } else {
            pan_setting = PAN_NUMERATOR_SLAVE[pan_setting as usize] << 1;
        }
        // Do a normal mix independent of any pair partial.
        mix_type = 0;
        pair_partial = None;
    } else if !state.extensions.nice_panning {
        // Mok wanted an option for smoother panning, and we love Mok.
        // CONFIRMED by Mok: exactly bytes like this (right shifted) are sent to the LA32.
        pan_setting &= 0x0E;
    }

    state.partials[partial_index].mix_type = mix_type;

    let mut left_pan_value: i32 = if state.reversed_stereo_enabled {
        14 - pan_setting as i32
    } else {
        pan_setting as i32
    };
    let mut right_pan_value: i32 = 14 - left_pan_value;

    // Float mode: we do NOT convert pan values via getPanFactor (that's only for integer mode).
    // The C++ code only calls getPanFactor when !floatMode. Since we only port the float path,
    // we skip that conversion.

    // SEMI-CONFIRMED: From sample analysis:
    // Found that timbres with 3 or 4 partials (i.e. one using two partial pairs) are mixed in two different ways.
    // Either partial pairs are added or subtracted, it depends on how the partial pairs are allocated.
    // It seems that partials are grouped into quarters and if the partial pairs are allocated in different quarters the subtraction happens.
    // Though, this matters little for the majority of timbres, it becomes crucial for timbres which contain several partials that sound very close.
    // In this case that timbre can sound totally different depending on the way it is mixed up.
    // Most easily this effect can be displayed with the help of a special timbre consisting of several identical square wave partials (3 or 4).
    // Say, it is 3-partial timbre. Just play any two notes simultaneously and the polys very probably are mixed differently.
    // Moreover, the partial allocator retains the last partial assignment it did and all the subsequent notes will sound the same as the last released one.
    // The situation is better with 4-partial timbres since then a whole quarter is assigned for each poly. However, if a 3-partial timbre broke the normal
    // whole-quarter assignment or after some partials got aborted, even 4-partial timbres can be found sounding differently.
    // This behaviour is also confirmed with two more special timbres: one with identical sawtooth partials, and one with PCM wave 02.
    // For my personal taste, this behaviour rather enriches the sounding and should be emulated.
    if !state.extensions.nice_partial_mixing && (partial_index & 4) != 0 {
        left_pan_value = -left_pan_value;
        right_pan_value = -right_pan_value;
    }

    state.partials[partial_index].left_pan_value = left_pan_value;
    state.partials[partial_index].right_pan_value = right_pan_value;

    if state.partials[partial_index].patch_cache.pcm_partial {
        let mut pcm_num = state.partials[partial_index].patch_cache.pcm;
        if let Some(control_rom_map) = state.rom.control_rom_map
            && control_rom_map.pcm_count > 128
        {
            // CM-32L, etc. support two "banks" of PCMs, selectable by waveform type parameter.
            if state.partials[partial_index].patch_cache.waveform > 1 {
                pcm_num += 128;
            }
        }
        state.partials[partial_index].pcm_num = pcm_num;
        state.partials[partial_index].pcm_wave_index = Some(pcm_num as usize);
    } else {
        state.partials[partial_index].pcm_wave_index = None;
    }

    // CONFIRMED: pulseWidthVal calculation is based on information from Mok
    let velocity = state.polys[poly_index].velocity as i32;
    let pulse_width_velo_sensitivity = state.partials[partial_index]
        .patch_cache
        .src_partial
        .wg
        .pulse_width_velo_sensitivity as i32;
    let pulse_width = state.partials[partial_index]
        .patch_cache
        .src_partial
        .wg
        .pulse_width as usize;
    let mut pulse_width_val = (velocity - 64) * (pulse_width_velo_sensitivity - 7)
        + state.tables.pulse_width_100_to_255[pulse_width] as i32;
    pulse_width_val = pulse_width_val.clamp(0, 255);
    state.partials[partial_index].pulse_width_val = pulse_width_val;

    state.partials[partial_index].pair_index = pair_partial;
    state.partials[partial_index].already_outputed = false;

    // TVA reset
    {
        let rhythm_temp = rhythm_temp_index.map(|i| state.mt32_ram.rhythm_temp(i));
        crate::tva::tva_reset(state, partial_index, part_index, rhythm_temp.as_ref());
    }

    // TVP reset
    crate::tvp::tvp_reset(state, partial_index, part_index);

    // TVF reset
    {
        let base_pitch = state.partials[partial_index].tvp.base_pitch;
        crate::tvf::tvf_reset(state, partial_index, base_pitch);
    }

    let is_slave = is_ring_modulating_slave(state, partial_index);
    let has_slave = has_ring_modulating_slave(state, partial_index);
    let mix_type = state.partials[partial_index].mix_type;

    if is_slave {
        let pair_index = state.partials[partial_index].pair_index.unwrap();
        // For a ring-modulating slave, we use the master's (pair's) la32_pair
        if is_pcm(state, partial_index) {
            let pcm_wave_index = state.partials[partial_index].pcm_wave_index.unwrap();
            let addr = state.rom.pcm_waves[pcm_wave_index].addr;
            let len = state.rom.pcm_waves[pcm_wave_index].len;
            let loop_flag = state.rom.pcm_waves[pcm_wave_index].loop_flag;
            state.partials[pair_index]
                .la32_pair
                .init_pcm(PairType::Slave, addr, len, loop_flag);
        } else {
            let waveform = state.partials[partial_index].patch_cache.waveform;
            let pulse_width_val = state.partials[partial_index].pulse_width_val as u8;
            let resonance = state.partials[partial_index]
                .patch_cache
                .src_partial
                .tvf
                .resonance
                + 1;
            state.partials[pair_index].la32_pair.init_synth(
                PairType::Slave,
                (waveform & 1) != 0,
                pulse_width_val,
                resonance,
            );
        }
    } else {
        state.partials[partial_index]
            .la32_pair
            .init(has_slave, mix_type == 1);
        if is_pcm(state, partial_index) {
            let pcm_wave_index = state.partials[partial_index].pcm_wave_index.unwrap();
            let addr = state.rom.pcm_waves[pcm_wave_index].addr;
            let len = state.rom.pcm_waves[pcm_wave_index].len;
            let loop_flag = state.rom.pcm_waves[pcm_wave_index].loop_flag;
            state.partials[partial_index].la32_pair.init_pcm(
                PairType::Master,
                addr,
                len,
                loop_flag,
            );
        } else {
            let waveform = state.partials[partial_index].patch_cache.waveform;
            let pulse_width_val = state.partials[partial_index].pulse_width_val as u8;
            let resonance = state.partials[partial_index]
                .patch_cache
                .src_partial
                .tvf
                .resonance
                + 1;
            state.partials[partial_index].la32_pair.init_synth(
                PairType::Master,
                (waveform & 1) != 0,
                pulse_width_val,
                resonance,
            );
        }
    }
    if !has_slave {
        state.partials[partial_index]
            .la32_pair
            .deactivate(PairType::Slave);
    }
}

/// SEMI-CONFIRMED: From sample analysis:
/// (1) Tested with a single partial playing PCM wave 77 with pitchCoarse 36 and no keyfollow, velocity follow, etc.
/// This gives results within +/- 2 at the output (before any DAC bitshifting)
/// when sustaining at levels 156 - 255 with no modifiers.
/// (2) Tested with a special square wave partial (internal capture ID tva5) at TVA envelope levels 155-255.
/// This gives deltas between -1 and 0 compared to the real output. Note that this special partial only produces
/// positive amps, so negative still needs to be explored, as well as lower levels.
///
/// Also still partially unconfirmed is the behaviour when ramping between levels, as well as the timing.
pub(crate) fn get_amp_value(state: &mut MuntState, partial_index: usize) -> u32 {
    let amp_ramp_val =
        67117056u32.wrapping_sub(state.partials[partial_index].amp_ramp.next_value());
    if state.partials[partial_index].amp_ramp.check_interrupt() {
        tva_handle_interrupt(state, partial_index);
    }
    amp_ramp_val
}

pub(crate) fn get_cutoff_value(state: &mut MuntState, partial_index: usize) -> u32 {
    if is_pcm(state, partial_index) {
        return 0;
    }
    let cutoff_modifier_ramp_val = state.partials[partial_index]
        .cutoff_modifier_ramp
        .next_value();
    if state.partials[partial_index]
        .cutoff_modifier_ramp
        .check_interrupt()
    {
        crate::tvf::tvf_handle_interrupt(state, partial_index);
    }
    ((state.partials[partial_index].tvf.base_cutoff as u32) << 18) + cutoff_modifier_ramp_val
}

pub(crate) fn has_ring_modulating_slave(state: &MuntState, partial_index: usize) -> bool {
    state.partials[partial_index].pair_index.is_some()
        && state.partials[partial_index].structure_position == 0
        && (state.partials[partial_index].mix_type == 1
            || state.partials[partial_index].mix_type == 2)
}

pub(crate) fn is_ring_modulating_slave(state: &MuntState, partial_index: usize) -> bool {
    state.partials[partial_index].pair_index.is_some()
        && state.partials[partial_index].structure_position == 1
        && (state.partials[partial_index].mix_type == 1
            || state.partials[partial_index].mix_type == 2)
}

pub(crate) fn is_pcm(state: &MuntState, partial_index: usize) -> bool {
    state.partials[partial_index].pcm_wave_index.is_some()
}

fn can_produce_output(state: &MuntState, partial_index: usize) -> bool {
    if !is_active(state, partial_index)
        || state.partials[partial_index].already_outputed
        || is_ring_modulating_slave(state, partial_index)
    {
        return false;
    }
    if state.partials[partial_index].poly_index.is_none() {
        return false;
    }
    true
}

fn generate_next_sample(state: &mut MuntState, partial_index: usize) -> bool {
    if !state.partials[partial_index].tva.playing
        || !state.partials[partial_index]
            .la32_pair
            .is_active(PairType::Master)
    {
        deactivate(state, partial_index);
        return false;
    }

    let amp_value = get_amp_value(state, partial_index);
    let pitch = crate::tvp::tvp_next_pitch(state, partial_index);
    let cutoff_value = get_cutoff_value(state, partial_index);

    // Split borrow: we need &state.rom.pcm_rom_data immutably and &mut state.partials[..].la32_pair mutably.
    {
        let MuntState {
            ref rom,
            ref mut partials,
            ..
        } = *state;
        partials[partial_index].la32_pair.generate_next_sample(
            &rom.pcm_rom_data,
            PairType::Master,
            amp_value,
            pitch,
            cutoff_value,
        );
    }

    if has_ring_modulating_slave(state, partial_index) {
        let pair_index = state.partials[partial_index].pair_index.unwrap();
        let pair_amp_value = get_amp_value(state, pair_index);
        let pair_pitch = crate::tvp::tvp_next_pitch(state, pair_index);
        let pair_cutoff_value = get_cutoff_value(state, pair_index);

        // The slave generates into the master's la32_pair
        {
            let MuntState {
                ref rom,
                ref mut partials,
                ..
            } = *state;
            partials[partial_index].la32_pair.generate_next_sample(
                &rom.pcm_rom_data,
                PairType::Slave,
                pair_amp_value,
                pair_pitch,
                pair_cutoff_value,
            );
        }

        let pair_tva_not_playing = !state.partials[pair_index].tva.playing;
        let slave_not_active = !state.partials[partial_index]
            .la32_pair
            .is_active(PairType::Slave);
        if pair_tva_not_playing || slave_not_active {
            deactivate(state, pair_index);
            let mix_type = state.partials[partial_index].mix_type;
            if mix_type == 2 {
                deactivate(state, partial_index);
                return false;
            }
        }
    }
    true
}

fn produce_and_mix_sample(state: &mut MuntState, partial_index: usize, buf_index: usize) {
    let sample = state.partials[partial_index].la32_pair.next_out_sample();
    let left_out = (sample * state.partials[partial_index].left_pan_value as f32) / 14.0;
    let right_out = (sample * state.partials[partial_index].right_pan_value as f32) / 14.0;
    state.renderer.tmp_partial_left[buf_index] += left_out;
    state.renderer.tmp_partial_right[buf_index] += right_out;
}

/// Returns true only if data written to buffer.
/// These functions produce processed stereo samples
/// made from combining this single partial with its pair, if it has one.
pub(crate) fn produce_output(state: &mut MuntState, partial_index: usize, length: u32) -> bool {
    if !can_produce_output(state, partial_index) {
        return false;
    }
    state.partials[partial_index].already_outputed = true;

    let mut sample_num: u32 = 0;
    while sample_num < length {
        state.partials[partial_index].sample_num = sample_num;
        if !generate_next_sample(state, partial_index) {
            break;
        }
        produce_and_mix_sample(state, partial_index, sample_num as usize);
        sample_num += 1;
    }
    state.partials[partial_index].sample_num = 0;
    true
}

pub(crate) fn start_abort(state: &mut MuntState, partial_index: usize) {
    // This is called when the partial manager needs to terminate partials for re-use by a new Poly.
    crate::tva::tva_start_abort(state, partial_index);
}

pub(crate) fn start_decay_all(state: &mut MuntState, partial_index: usize) {
    crate::tva::tva_start_decay(state, partial_index);
    tvp_start_decay(state, partial_index);
    crate::tvf::tvf_start_decay(state, partial_index);
}

// Wrapper for tva_handle_interrupt that extracts the needed parameters from state.
fn tva_handle_interrupt(state: &mut MuntState, partial_index: usize) {
    let owner_part = state.partials[partial_index].owner_part;
    if owner_part < 0 {
        return;
    }
    let part_index = owner_part as usize;
    crate::tva::tva_handle_interrupt(state, partial_index, part_index);
}

fn tvp_start_decay(state: &mut MuntState, partial_index: usize) {
    crate::tvp::tvp_start_decay(state, partial_index);
}
