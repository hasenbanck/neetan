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

use crate::{state::MuntState, structures::PartialParam};

// Note that when entering next_phase(), new_phase is set to phase + 1, and the descriptions/names below refer to
// new_phase's value.

// When this is the target phase, level[1] is targeted within time[1]
const PHASE_2: u32 = 2;

// When this is the target phase, immediately goes to PHASE_RELEASE unless the poly is set to sustain.
// Otherwise level[3] is continued with increment 0 - no phase change will occur until some external influence (like pedal release)
const PHASE_SUSTAIN: u32 = 5;

// 0 is targeted within time[4] (the time calculation is quite different from the other phases)
const PHASE_RELEASE: u32 = 6;

// 0 is targeted with increment 0 (thus theoretically staying that way forever)
const PHASE_DONE: u32 = 7;

fn calc_base_cutoff(
    partial_param: &PartialParam,
    base_pitch: u32,
    key: u32,
    quirk_tvf_base_cutoff_limit: bool,
) -> u8 {
    // This table matches the values used by a real LAPC-I.
    const BIAS_LEVEL_TO_BIAS_MULT: [i8; 15] =
        [85, 42, 21, 16, 10, 5, 2, 0, -2, -5, -10, -16, -21, -74, -85];
    // These values represent unique options with no consistent pattern, so we have to use something like a table in any case.
    // The table entries, when divided by 21, match approximately what the manual claims:
    // -1, -1/2, -1/4, 0, 1/8, 1/4, 3/8, 1/2, 5/8, 3/4, 7/8, 1, 5/4, 3/2, 2, s1, s2
    // Note that the entry for 1/8 is rounded to 2 (from 1/8 * 21 = 2.625), which seems strangely inaccurate compared to the others.
    const KEYFOLLOW_MULT21: [i8; 17] = [
        -21, -10, -5, 0, 2, 5, 8, 10, 13, 16, 18, 21, 26, 32, 42, 21, 21,
    ];
    let mut base_cutoff: i32 = KEYFOLLOW_MULT21[partial_param.tvf.keyfollow as usize] as i32
        - KEYFOLLOW_MULT21[partial_param.wg.pitch_keyfollow as usize] as i32;
    // base_cutoff range now: -63 to 63
    base_cutoff *= key as i32 - 60;
    // base_cutoff range now: -3024 to 3024
    let bias_point = partial_param.tvf.bias_point as i32;
    if (bias_point & 0x40) == 0 {
        // bias_point range here: 0 to 63
        let bias = bias_point + 33 - key as i32; // bias range here: -75 to 84
        if bias > 0 {
            let bias = -bias; // bias range here: -1 to -84
            base_cutoff +=
                bias * BIAS_LEVEL_TO_BIAS_MULT[partial_param.tvf.bias_level as usize] as i32; // Calculation range: -7140 to 7140
            // base_cutoff range now: -10164 to 10164
        }
    } else {
        // bias_point range here: 64 to 127
        let bias = bias_point - 31 - key as i32; // bias range here: -75 to 84
        if bias < 0 {
            base_cutoff +=
                bias * BIAS_LEVEL_TO_BIAS_MULT[partial_param.tvf.bias_level as usize] as i32; // Calculation range: -6375 to 6375
            // base_cutoff range now: -9399 to 9399
        }
    }
    // base_cutoff range now: -10164 to 10164
    base_cutoff += ((partial_param.tvf.cutoff as i32) << 4) - 800;
    // base_cutoff range now: -10964 to 10964
    if base_cutoff >= 0 {
        // FIXME: Potentially bad if base_cutoff ends up below -2056?
        let pitch_delta_thing = (base_pitch as i32 >> 4) + base_cutoff - 3584;
        if pitch_delta_thing > 0 {
            base_cutoff -= pitch_delta_thing;
        }
    } else if quirk_tvf_base_cutoff_limit {
        if base_cutoff <= -0x400 {
            base_cutoff = -400;
        }
    } else if base_cutoff < -2048 {
        base_cutoff = -2048;
    }
    base_cutoff += 2056;
    base_cutoff >>= 4; // PORTABILITY NOTE: Hmm... Depends whether it could've been below -2056, but maybe arithmetic shift assumed?
    if base_cutoff > 255 {
        base_cutoff = 255;
    }
    base_cutoff as u8
}

fn start_ramp(
    state: &mut MuntState,
    partial_index: usize,
    new_target: u8,
    new_increment: u8,
    new_phase: u32,
) {
    state.partials[partial_index].tvf.target = new_target;
    state.partials[partial_index].tvf.phase = new_phase;
    state.partials[partial_index]
        .cutoff_modifier_ramp
        .start_ramp(&state.tables, new_target, new_increment);
}

pub(crate) fn tvf_reset(state: &mut MuntState, partial_index: usize, base_pitch: u32) {
    let poly_index = state.partials[partial_index]
        .poly_index
        .expect("tvf_reset: partial must have a poly");
    let key = state.polys[poly_index].key;
    let velocity = state.polys[poly_index].velocity;

    let quirk_tvf_base_cutoff_limit = state.rom.control_rom_features.quirk_tvf_base_cutoff_limit;
    let partial_param = &state.partials[partial_index].patch_cache.src_partial;

    let base_cutoff = calc_base_cutoff(partial_param, base_pitch, key, quirk_tvf_base_cutoff_limit);
    let env_velo_sensitivity = partial_param.tvf.env_velo_sensitivity;
    let env_depth_keyfollow = partial_param.tvf.env_depth_keyfollow;
    let env_depth = partial_param.tvf.env_depth;
    let env_time_keyfollow = partial_param.tvf.env_time_keyfollow;
    let env_level_0 = partial_param.tvf.env_level[0];
    let env_time_0 = partial_param.tvf.env_time[0];

    state.partials[partial_index].tvf.base_cutoff = base_cutoff;

    let mut new_level_mult = (velocity as i32) * (env_velo_sensitivity as i32);
    new_level_mult >>= 6;
    new_level_mult += 109 - env_velo_sensitivity as i32;
    new_level_mult += (key as i32 - 60) >> (4 - env_depth_keyfollow);
    if new_level_mult < 0 {
        new_level_mult = 0;
    }
    new_level_mult *= env_depth as i32;
    new_level_mult >>= 6;
    if new_level_mult > 255 {
        new_level_mult = 255;
    }
    let level_mult = new_level_mult as u32;
    state.partials[partial_index].tvf.level_mult = level_mult;

    if env_time_keyfollow != 0 {
        state.partials[partial_index].tvf.key_time_subtraction =
            (key as i32 - 60) >> (5 - env_time_keyfollow);
    } else {
        state.partials[partial_index].tvf.key_time_subtraction = 0;
    }

    let new_target = ((level_mult * env_level_0 as u32) >> 8) as i32;
    let env_time_setting =
        env_time_0 as i32 - state.partials[partial_index].tvf.key_time_subtraction;
    let new_increment = if env_time_setting <= 0 {
        0x80 | 127
    } else {
        let inc = state.tables.env_logarithmic_time[new_target as usize] as i32 - env_time_setting;
        if inc <= 0 { 1 } else { inc }
    };
    state.partials[partial_index].cutoff_modifier_ramp.reset();
    start_ramp(
        state,
        partial_index,
        new_target as u8,
        new_increment as u8,
        PHASE_2 - 1,
    );
}

/// Returns the base cutoff (without envelope modification).
pub(crate) fn tvf_handle_interrupt(state: &mut MuntState, partial_index: usize) {
    tvf_next_phase(state, partial_index);
}

pub(crate) fn tvf_start_decay(state: &mut MuntState, partial_index: usize) {
    if state.partials[partial_index].tvf.phase >= PHASE_RELEASE {
        return;
    }
    let env_time4 = state.partials[partial_index]
        .patch_cache
        .src_partial
        .tvf
        .env_time[4];
    if env_time4 == 0 {
        start_ramp(state, partial_index, 0, 1, PHASE_DONE - 1);
    } else {
        start_ramp(
            state,
            partial_index,
            0,
            (-(env_time4 as i8 as i32)) as u8,
            PHASE_DONE - 1,
        );
    }
}

fn tvf_next_phase(state: &mut MuntState, partial_index: usize) {
    let new_phase = state.partials[partial_index].tvf.phase + 1;

    match new_phase {
        PHASE_DONE => {
            start_ramp(state, partial_index, 0, 0, new_phase);
            return;
        }
        PHASE_SUSTAIN | PHASE_RELEASE => {
            // FIXME: Afaict new_phase should never be PHASE_RELEASE here. And if it were, this is an odd way to handle it.
            let poly_index = state.partials[partial_index]
                .poly_index
                .expect("tvf_next_phase: partial must have a poly");
            let can_sustain = state.polys[poly_index].sustain;
            if !can_sustain {
                state.partials[partial_index].tvf.phase = new_phase; // FIXME: Correct?
                tvf_start_decay(state, partial_index); // FIXME: This should actually start decay even if phase is already 6. Does that matter?
                return;
            }
            let level_mult = state.partials[partial_index].tvf.level_mult;
            let env_level3 = state.partials[partial_index]
                .patch_cache
                .src_partial
                .tvf
                .env_level[3] as u32;
            let target = ((level_mult * env_level3) >> 8) as u8;
            start_ramp(state, partial_index, target, 0, new_phase);
            return;
        }
        _ => {}
    }

    let env_point_index = state.partials[partial_index].tvf.phase as usize;
    let key_time_subtraction = state.partials[partial_index].tvf.key_time_subtraction;
    let env_time_setting = state.partials[partial_index]
        .patch_cache
        .src_partial
        .tvf
        .env_time[env_point_index] as i32
        - key_time_subtraction;

    let level_mult = state.partials[partial_index].tvf.level_mult;
    let env_level = state.partials[partial_index]
        .patch_cache
        .src_partial
        .tvf
        .env_level[env_point_index] as u32;
    let mut new_target = ((level_mult * env_level) >> 8) as i32;
    let target = state.partials[partial_index].tvf.target;
    let new_increment = if env_time_setting > 0 {
        let mut target_delta = new_target - target as i32;
        if target_delta == 0 {
            if new_target == 0 {
                target_delta = 1;
                new_target = 1;
            } else {
                target_delta = -1;
                new_target -= 1;
            }
        }
        let abs_target_delta = if target_delta < 0 {
            -target_delta
        } else {
            target_delta
        };
        let mut inc =
            state.tables.env_logarithmic_time[abs_target_delta as usize] as i32 - env_time_setting;
        if inc <= 0 {
            inc = 1;
        }
        if target_delta < 0 {
            inc |= 0x80;
        }
        inc
    } else if new_target >= target as i32 {
        0x80 | 127
    } else {
        127
    };
    start_ramp(
        state,
        partial_index,
        new_target as u8,
        new_increment as u8,
        new_phase,
    );
}
