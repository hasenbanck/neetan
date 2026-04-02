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

// This module emulates the calculations performed by the 8095 microcontroller in order to configure
// the LA-32's amplitude ramp for a single partial at each stage of its TVA envelope.
// Unless we introduced bugs, it should be pretty much 100% accurate according to Mok's specifications.

use crate::{
    state::{
        MuntState, TVA_PHASE_2, TVA_PHASE_3, TVA_PHASE_4, TVA_PHASE_ATTACK, TVA_PHASE_BASIC,
        TVA_PHASE_DEAD, TVA_PHASE_RELEASE, TVA_PHASE_SUSTAIN,
    },
    structures::{PartialParam, RhythmTemp, SystemParam},
};

// CONFIRMED: Matches a table in ROM - haven't got around to coming up with a formula for it yet.
static BIAS_LEVEL_TO_AMP_SUBTRACTION_COEFF: [u8; 13] =
    [255, 187, 137, 100, 74, 54, 40, 29, 21, 15, 10, 5, 0];

fn start_ramp(
    state: &mut MuntState,
    partial_index: usize,
    new_target: u8,
    new_increment: u8,
    new_phase: i32,
) {
    state.partials[partial_index].tva.target = new_target;
    state.partials[partial_index].tva.phase = new_phase;
    state.partials[partial_index]
        .amp_ramp
        .start_ramp(&state.tables, new_target, new_increment);
}

fn end(state: &mut MuntState, partial_index: usize, new_phase: i32) {
    state.partials[partial_index].tva.phase = new_phase;
    state.partials[partial_index].tva.playing = false;
}

fn mult_bias(bias_level: u8, bias: i32) -> i32 {
    (bias * BIAS_LEVEL_TO_AMP_SUBTRACTION_COEFF[bias_level as usize] as i32) >> 5
}

fn calc_bias_amp_subtraction(bias_point: u8, bias_level: u8, key: i32) -> i32 {
    if (bias_point & 0x40) == 0 {
        let bias = bias_point as i32 + 33 - key;
        if bias > 0 {
            return mult_bias(bias_level, bias);
        }
    } else {
        let bias = bias_point as i32 - 31 - key;
        if bias < 0 {
            let bias = -bias;
            return mult_bias(bias_level, bias);
        }
    }
    0
}

fn calc_bias_amp_subtractions(partial_param: &PartialParam, key: i32) -> i32 {
    let bias_amp_subtraction1 = calc_bias_amp_subtraction(
        partial_param.tva.bias_point1,
        partial_param.tva.bias_level1,
        key,
    );
    if bias_amp_subtraction1 > 255 {
        return 255;
    }
    let bias_amp_subtraction2 = calc_bias_amp_subtraction(
        partial_param.tva.bias_point2,
        partial_param.tva.bias_level2,
        key,
    );
    if bias_amp_subtraction2 > 255 {
        return 255;
    }
    let bias_amp_subtraction = bias_amp_subtraction1 + bias_amp_subtraction2;
    if bias_amp_subtraction > 255 {
        return 255;
    }
    bias_amp_subtraction
}

fn calc_velo_amp_subtraction(velo_sensitivity: u8, velocity: u32) -> i32 {
    // FIXME:KG: Better variable names
    let velocity_mult = velo_sensitivity as i32 - 50;
    let abs_velocity_mult = if velocity_mult < 0 {
        -velocity_mult
    } else {
        velocity_mult
    };
    let velocity_mult = ((velocity_mult * (velocity as i32 - 64)) as u32) << 2;
    let velocity_mult = velocity_mult as i32;
    abs_velocity_mult - (velocity_mult >> 8) // PORTABILITY NOTE: Assumes arithmetic shift
}

#[allow(clippy::too_many_arguments)]
fn calc_basic_amp(
    state: &MuntState,
    partial_index: usize,
    system: &SystemParam,
    part_volume: u8,
    rhythm_temp: Option<&RhythmTemp>,
    bias_amp_subtraction: i32,
    velo_amp_subtraction: i32,
    expression: u8,
    has_ring_mod_quirk: bool,
) -> i32 {
    let mut amp: i32 = 155;

    let partial_state = &state.partials[partial_index];
    let partial_param = &partial_state.patch_cache.src_partial;

    let is_ring_modulating = if has_ring_mod_quirk {
        // isRingModulatingNoMix():
        // pair != NULL && ((structurePosition == 1 && mixType == 1) || mixType == 2)
        partial_state.pair_index.is_some()
            && ((partial_state.structure_position == 1 && partial_state.mix_type == 1)
                || partial_state.mix_type == 2)
    } else {
        // isRingModulatingSlave():
        // pair != NULL && structurePosition == 1 && (mixType == 1 || mixType == 2)
        partial_state.pair_index.is_some()
            && partial_state.structure_position == 1
            && (partial_state.mix_type == 1 || partial_state.mix_type == 2)
    };

    if !is_ring_modulating {
        amp -= state.tables.master_vol_to_amp_subtraction[system.master_vol as usize] as i32;
        if amp < 0 {
            return 0;
        }
        amp -= state.tables.level_to_amp_subtraction[part_volume as usize] as i32;
        if amp < 0 {
            return 0;
        }
        amp -= state.tables.level_to_amp_subtraction[expression as usize] as i32;
        if amp < 0 {
            return 0;
        }
        if let Some(rt) = rhythm_temp {
            amp -= state.tables.level_to_amp_subtraction[rt.output_level as usize] as i32;
            if amp < 0 {
                return 0;
            }
        }
    }
    amp -= bias_amp_subtraction;
    if amp < 0 {
        return 0;
    }
    amp -= state.tables.level_to_amp_subtraction[partial_param.tva.level as usize] as i32;
    if amp < 0 {
        return 0;
    }
    amp -= velo_amp_subtraction;
    if amp < 0 {
        return 0;
    }
    if amp > 155 {
        amp = 155;
    }
    amp -= (partial_param.tvf.resonance >> 1) as i32;
    if amp < 0 {
        return 0;
    }
    amp
}

fn calc_key_time_subtraction(env_time_keyfollow: u8, key: i32) -> i32 {
    if env_time_keyfollow == 0 {
        return 0;
    }
    (key - 60) >> (5 - env_time_keyfollow) // PORTABILITY NOTE: Assumes arithmetic shift
}

pub(crate) fn tva_reset(
    state: &mut MuntState,
    partial_index: usize,
    part_index: usize,
    rhythm_temp: Option<&RhythmTemp>,
) {
    state.partials[partial_index].tva.playing = true;

    let poly_index = state.partials[partial_index]
        .poly_index
        .expect("partial must have a poly");
    let key = state.polys[poly_index].key as i32;
    let velocity = state.polys[poly_index].velocity;

    let partial_param = &state.partials[partial_index].patch_cache.src_partial;
    let key_time_subtraction = calc_key_time_subtraction(partial_param.tva.env_time_keyfollow, key);
    let bias_amp_subtraction = calc_bias_amp_subtractions(partial_param, key);
    let velo_amp_subtraction =
        calc_velo_amp_subtraction(partial_param.tva.velo_sensitivity, velocity);
    let env_time_0 = partial_param.tva.env_time[0];
    let env_level_0 = partial_param.tva.env_level[0];

    state.partials[partial_index].tva.key_time_subtraction = key_time_subtraction;
    state.partials[partial_index].tva.bias_amp_subtraction = bias_amp_subtraction;
    state.partials[partial_index].tva.velo_amp_subtraction = velo_amp_subtraction;

    let part_volume = {
        let ps = &state.parts[part_index];
        if ps.volume_override <= 100 {
            ps.volume_override
        } else {
            state.mt32_ram.patch_temp(part_index).output_level
        }
    };
    let expression = state.parts[part_index].expression;
    let quirk_ring_modulation_no_mix = state.rom.control_rom_features.quirk_ring_modulation_no_mix;
    let system = state.mt32_ram.system();

    let mut new_target = calc_basic_amp(
        state,
        partial_index,
        &system,
        part_volume,
        rhythm_temp,
        bias_amp_subtraction,
        velo_amp_subtraction,
        expression,
        quirk_ring_modulation_no_mix,
    );
    let new_phase = if env_time_0 == 0 {
        // Initially go to the TVA_PHASE_ATTACK target amp, and spend the next phase going from
        // there to the TVA_PHASE_2 target amp.
        // Note that this means that velocity never affects time for this partial.
        new_target += env_level_0 as i32;
        TVA_PHASE_ATTACK // The first target used in next_phase() will be TVA_PHASE_2
    } else {
        // Initially go to the base amp determined by TVA level, part volume, etc., and spend the
        // next phase going from there to the full TVA_PHASE_ATTACK target amp.
        TVA_PHASE_BASIC // The first target used in next_phase() will be TVA_PHASE_ATTACK
    };

    state.partials[partial_index].amp_ramp.reset(); //currentAmp = 0;

    // "Go downward as quickly as possible".
    // Since the current value is 0, the LA32Ramp will notice that we're already at or below the
    // target and trying to go downward, and therefore jump to the target immediately and raise an
    // interrupt.
    start_ramp(
        state,
        partial_index,
        new_target as u8,
        0x80 | 127,
        new_phase,
    );
}

pub(crate) fn tva_start_abort(state: &mut MuntState, partial_index: usize) {
    start_ramp(state, partial_index, 64, 0x80 | 127, TVA_PHASE_RELEASE);
}

pub(crate) fn tva_start_decay(state: &mut MuntState, partial_index: usize) {
    if state.partials[partial_index].tva.phase >= TVA_PHASE_RELEASE {
        return;
    }
    let env_time_4 = state.partials[partial_index]
        .patch_cache
        .src_partial
        .tva
        .env_time[4];
    let new_increment = if env_time_4 == 0 {
        1u8
    } else {
        (-(env_time_4 as i8)) as u8
    };
    // The next time next_phase() is called, it will think TVA_PHASE_RELEASE has finished and
    // the partial will be aborted
    start_ramp(state, partial_index, 0, new_increment, TVA_PHASE_RELEASE);
}

pub(crate) fn tva_handle_interrupt(state: &mut MuntState, partial_index: usize, part_index: usize) {
    tva_next_phase(state, partial_index, part_index);
}

pub(crate) fn tva_recalc_sustain(
    state: &mut MuntState,
    partial_index: usize,
    part_index: usize,
    rhythm_temp: Option<&RhythmTemp>,
) {
    // We get pinged periodically by the pitch code to recalculate our values when in sustain.
    // This is done so that the TVA will respond to things like MIDI expression and volume changes
    // while it's sustaining, which it otherwise wouldn't do.

    // The check for envLevel[3] == 0 strikes me as slightly dumb. FIXME: Explain why
    let env_level_3 = state.partials[partial_index]
        .patch_cache
        .src_partial
        .tva
        .env_level[3];
    if state.partials[partial_index].tva.phase != TVA_PHASE_SUSTAIN || env_level_3 == 0 {
        return;
    }
    // We're sustaining. Recalculate all the values
    let part_volume = {
        let ps = &state.parts[part_index];
        if ps.volume_override <= 100 {
            ps.volume_override
        } else {
            state.mt32_ram.patch_temp(part_index).output_level
        }
    };
    let expression = state.parts[part_index].expression;
    let bias_amp_subtraction = state.partials[partial_index].tva.bias_amp_subtraction;
    let velo_amp_subtraction = state.partials[partial_index].tva.velo_amp_subtraction;
    let quirk_ring_modulation_no_mix = state.rom.control_rom_features.quirk_ring_modulation_no_mix;
    let system = state.mt32_ram.system();

    let mut new_target = calc_basic_amp(
        state,
        partial_index,
        &system,
        part_volume,
        rhythm_temp,
        bias_amp_subtraction,
        velo_amp_subtraction,
        expression,
        quirk_ring_modulation_no_mix,
    );
    new_target += env_level_3 as i32;

    // Although we're in TVA_PHASE_SUSTAIN at this point, we cannot be sure that there is no
    // active ramp at the moment. In case the channel volume or the expression changes frequently,
    // the previously started ramp may still be in progress. Real hardware units ignore this
    // possibility and rely on the assumption that the target is the current amp. This is OK in most
    // situations but when the ramp that is currently in progress needs to change direction due to
    // a volume/expression update, this leads to a jump in the amp that is audible as an unpleasant
    // click. To avoid that, we compare the newTarget with the actual current ramp value and correct
    // the direction if necessary.
    let target = state.partials[partial_index].tva.target;
    let target_delta = new_target - target as i32;

    // Calculate an increment to get to the new amp value in a short, more or less consistent
    // amount of time
    let descending = target_delta < 0;
    let mut new_increment = if !descending {
        state.tables.env_logarithmic_time[target_delta as u8 as usize].wrapping_sub(2)
    } else {
        (state.tables.env_logarithmic_time[(-target_delta) as u8 as usize].wrapping_sub(2)) | 0x80
    };
    let nice_amp_ramp = state.extensions.nice_amp_ramp;
    if nice_amp_ramp
        && (descending
            != state.partials[partial_index]
                .amp_ramp
                .is_below_current(new_target as u8))
    {
        new_increment ^= 0x80;
    }

    // Configure so that once the transition's complete and next_phase() is called, we'll just
    // re-enter sustain phase (or decay phase, depending on parameters at the time).
    start_ramp(
        state,
        partial_index,
        new_target as u8,
        new_increment,
        TVA_PHASE_SUSTAIN - 1,
    );
}

fn tva_next_phase(state: &mut MuntState, partial_index: usize, part_index: usize) {
    let phase = state.partials[partial_index].tva.phase;

    if phase >= TVA_PHASE_DEAD || !state.partials[partial_index].tva.playing {
        return;
    }
    let new_phase = phase + 1;

    if new_phase == TVA_PHASE_DEAD {
        end(state, partial_index, new_phase);
        return;
    }

    let quirk_tva_zero_env_levels = state.rom.control_rom_features.quirk_tva_zero_env_levels;

    // Read envelope levels/times from partial_param into locals (all Copy u8).
    let pp = &state.partials[partial_index].patch_cache.src_partial;
    let env_level = pp.tva.env_level;
    let env_time = pp.tva.env_time;
    let env_time_velo_sensitivity = pp.tva.env_time_velo_sensitivity;

    let mut all_levels_zero_from_now_on = false;
    if env_level[3] == 0 {
        if new_phase == TVA_PHASE_4 {
            all_levels_zero_from_now_on = true;
        } else if !quirk_tva_zero_env_levels && env_level[2] == 0 {
            if new_phase == TVA_PHASE_3 {
                all_levels_zero_from_now_on = true;
            } else if env_level[1] == 0 {
                if new_phase == TVA_PHASE_2 {
                    all_levels_zero_from_now_on = true;
                } else if env_level[0] == 0 && new_phase == TVA_PHASE_ATTACK {
                    // this line added, missing in ROM - FIXME: Add description of repercussions
                    all_levels_zero_from_now_on = true;
                }
            }
        }
    }

    let mut new_target;
    let mut new_increment: i32 = 0; // Initialised to please compilers
    let env_point_index = phase as usize;

    if !all_levels_zero_from_now_on {
        let part_volume = {
            let ps = &state.parts[part_index];
            if ps.volume_override <= 100 {
                ps.volume_override
            } else {
                state.mt32_ram.patch_temp(part_index).output_level
            }
        };
        let expression = state.parts[part_index].expression;
        let bias_amp_subtraction = state.partials[partial_index].tva.bias_amp_subtraction;
        let velo_amp_subtraction = state.partials[partial_index].tva.velo_amp_subtraction;
        let quirk_ring_modulation_no_mix =
            state.rom.control_rom_features.quirk_ring_modulation_no_mix;

        // Read rhythm_temp for this partial using the stored index from start_partial.
        let rhythm_temp = state.partials[partial_index]
            .rhythm_temp_index
            .map(|i| state.mt32_ram.rhythm_temp(i));
        let system = state.mt32_ram.system();

        new_target = calc_basic_amp(
            state,
            partial_index,
            &system,
            part_volume,
            rhythm_temp.as_ref(),
            bias_amp_subtraction,
            velo_amp_subtraction,
            expression,
            quirk_ring_modulation_no_mix,
        );

        if new_phase == TVA_PHASE_SUSTAIN || new_phase == TVA_PHASE_RELEASE {
            if env_level[3] == 0 {
                end(state, partial_index, new_phase);
                return;
            }
            let poly_index = state.partials[partial_index]
                .poly_index
                .expect("partial must have a poly");
            let can_sustain = state.polys[poly_index].sustain;
            if !can_sustain {
                let new_phase = TVA_PHASE_RELEASE;
                new_target = 0;
                new_increment = -(env_time[4] as i32);
                if new_increment == 0 {
                    // We can't let the increment be 0, or there would be no emulated interrupt.
                    // So we do an "upward" increment, which should set the amp to 0 extremely
                    // quickly and cause an "interrupt" to bring us back to next_phase().
                    new_increment = 1;
                }
                start_ramp(
                    state,
                    partial_index,
                    new_target as u8,
                    new_increment as u8,
                    new_phase,
                );
                return;
            } else {
                new_target += env_level[3] as i32;
                new_increment = 0;
            }
        } else {
            new_target += env_level[env_point_index] as i32;
        }
    } else {
        new_target = 0;
    }

    if (new_phase != TVA_PHASE_SUSTAIN && new_phase != TVA_PHASE_RELEASE)
        || all_levels_zero_from_now_on
    {
        let mut env_time_setting = env_time[env_point_index] as i32;

        if new_phase == TVA_PHASE_ATTACK {
            let poly_index = state.partials[partial_index]
                .poly_index
                .expect("partial must have a poly");
            let velocity = state.polys[poly_index].velocity as i32;
            env_time_setting -= (velocity - 64) >> (6 - env_time_velo_sensitivity); // PORTABILITY NOTE: Assumes arithmetic shift

            if env_time_setting <= 0 && env_time[env_point_index] != 0 {
                env_time_setting = 1;
            }
        } else {
            env_time_setting -= state.partials[partial_index].tva.key_time_subtraction;
        }
        let target = state.partials[partial_index].tva.target;
        if env_time_setting > 0 {
            let mut target_delta = new_target - target as i32;
            if target_delta <= 0 {
                if target_delta == 0 {
                    // target and newTarget are the same.
                    // We can't have an increment of 0 or we wouldn't get an emulated interrupt.
                    // So instead make the target one less than it really should be and set
                    // targetDelta accordingly.
                    target_delta = -1;
                    new_target -= 1;
                    if new_target < 0 {
                        // Oops, newTarget is less than zero now, so let's do it the other way:
                        // Make newTarget one more than it really should've been and set
                        // targetDelta accordingly.
                        // FIXME (apparent bug in real firmware):
                        // This means targetDelta will be positive just below here where it's
                        // inverted, and we'll end up using envLogarithmicTime[-1], and we'll be
                        // setting newIncrement to be descending later on, etc..
                        target_delta = 1;
                        new_target = -new_target;
                    }
                }
                target_delta = -target_delta;
                new_increment = state.tables.env_logarithmic_time[target_delta as u8 as usize]
                    as i32
                    - env_time_setting;
                if new_increment <= 0 {
                    new_increment = 1;
                }
                new_increment |= 0x80;
            } else {
                // FIXME: The last 22 or so entries in this table are 128 - surely that fucks
                // things up, since that ends up being -128 signed?
                new_increment = state.tables.env_logarithmic_time[target_delta as u8 as usize]
                    as i32
                    - env_time_setting;
                if new_increment <= 0 {
                    new_increment = 1;
                }
            }
        } else {
            new_increment = if new_target >= target as i32 {
                0x80 | 127
            } else {
                127
            };
        }

        // FIXME: What's the point of this? It's checked or set to non-zero everywhere above
        if new_increment == 0 {
            new_increment = 1;
        }
    }

    start_ramp(
        state,
        partial_index,
        new_target as u8,
        new_increment as u8,
        new_phase,
    );
}
