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

use crate::{enumerations::PolyState, state::MuntState, structures::PatchCache};

pub(crate) fn poly_reset(
    state: &mut MuntState,
    poly_index: usize,
    new_key: u32,
    new_velocity: u32,
    new_sustain: bool,
    new_partial_indices: [Option<usize>; 4],
) {
    if poly_is_active(state, poly_index) {
        // This should never happen
        // C++: printDebug("Resetting active poly. Active partial count: %i", activePartialCount)
        for i in 0..4 {
            if let Some(partial_index) = state.polys[poly_index].partial_indices[i]
                && state.partials[partial_index].active
            {
                partial_deactivate(state, partial_index);
                state.polys[poly_index].active_partial_count -= 1;
            }
        }
        poly_set_state(state, poly_index, PolyState::Inactive);
    }

    state.polys[poly_index].key = new_key;
    state.polys[poly_index].velocity = new_velocity;
    state.polys[poly_index].sustain = new_sustain;

    state.polys[poly_index].active_partial_count = 0;
    for (i, new_partial) in new_partial_indices.iter().enumerate() {
        state.polys[poly_index].partial_indices[i] = *new_partial;
        if new_partial.is_some() {
            state.polys[poly_index].active_partial_count += 1;
            poly_set_state(state, poly_index, PolyState::Playing);
        }
    }
}

pub(crate) fn poly_note_off(state: &mut MuntState, poly_index: usize, pedal_held: bool) -> bool {
    // Generally, non-sustaining instruments ignore note off. They die away eventually anyway.
    // Key 0 (only used by special cases on rhythm part) reacts to note off even if non-sustaining or pedal held.
    let poly_state = state.polys[poly_index].state;
    if poly_state == PolyState::Inactive || poly_state == PolyState::Releasing {
        return false;
    }
    if pedal_held {
        if poly_state == PolyState::Held {
            return false;
        }
        poly_set_state(state, poly_index, PolyState::Held);
    } else {
        poly_start_decay(state, poly_index);
    }
    true
}

pub(crate) fn poly_stop_pedal_hold(state: &mut MuntState, poly_index: usize) -> bool {
    if state.polys[poly_index].state != PolyState::Held {
        return false;
    }
    poly_start_decay(state, poly_index)
}

pub(crate) fn poly_start_decay(state: &mut MuntState, poly_index: usize) -> bool {
    let poly_state = state.polys[poly_index].state;
    if poly_state == PolyState::Inactive || poly_state == PolyState::Releasing {
        return false;
    }
    poly_set_state(state, poly_index, PolyState::Releasing);

    for t in 0..4 {
        if let Some(partial_index) = state.polys[poly_index].partial_indices[t] {
            partial_start_decay_all(state, partial_index);
        }
    }
    true
}

pub(crate) fn poly_start_abort(state: &mut MuntState, poly_index: usize) -> bool {
    let poly_state = state.polys[poly_index].state;
    if poly_state == PolyState::Inactive || state.aborting_poly_index.is_some() {
        return false;
    }
    for t in 0..4 {
        if let Some(partial_index) = state.polys[poly_index].partial_indices[t] {
            partial_start_abort(state, partial_index);
            state.aborting_poly_index = Some(poly_index);
        }
    }
    true
}

fn poly_set_state(state: &mut MuntState, poly_index: usize, new_state: PolyState) {
    let old_state = state.polys[poly_index].state;
    if old_state == new_state {
        return;
    }
    state.polys[poly_index].state = new_state;
    let part_index = state.polys[poly_index]
        .part_index
        .expect("Poly must have a part assigned");
    part_poly_state_changed(state, part_index, old_state, new_state);
}

pub(crate) fn poly_backup_cache_to_partials(
    state: &mut MuntState,
    poly_index: usize,
    cache: &[PatchCache; 4],
) {
    for (partial_num, cache_entry) in cache.iter().enumerate() {
        if let Some(partial_index) = state.polys[poly_index].partial_indices[partial_num] {
            partial_backup_cache(state, partial_index, cache_entry);
        }
    }
}

pub(crate) fn poly_is_active(state: &MuntState, poly_index: usize) -> bool {
    state.polys[poly_index].state != PolyState::Inactive
}

/// This is called by Partial to inform the poly that the Partial has deactivated
pub(crate) fn poly_partial_deactivated(
    state: &mut MuntState,
    poly_index: usize,
    partial_index: usize,
) {
    for i in 0..4 {
        if state.polys[poly_index].partial_indices[i] == Some(partial_index) {
            state.polys[poly_index].partial_indices[i] = None;
            state.polys[poly_index].active_partial_count -= 1;
        }
    }
    if state.polys[poly_index].active_partial_count == 0 {
        poly_set_state(state, poly_index, PolyState::Inactive);
        if state.aborting_poly_index == Some(poly_index) {
            state.aborting_poly_index = None;
        }
    }
    let part_index = state.polys[poly_index]
        .part_index
        .expect("Poly must have a part assigned");
    part_partial_deactivated(state, part_index, poly_index);
}

fn partial_deactivate(state: &mut MuntState, partial_index: usize) {
    state.partials[partial_index].active = false;
}

fn partial_start_decay_all(state: &mut MuntState, partial_index: usize) {
    crate::partial::start_decay_all(state, partial_index);
}

fn partial_start_abort(state: &mut MuntState, partial_index: usize) {
    crate::partial::start_abort(state, partial_index);
}

fn partial_backup_cache(state: &mut MuntState, partial_index: usize, cache: &PatchCache) {
    state.partials[partial_index].cache_backup = cache.clone();
}

fn part_poly_state_changed(
    state: &mut MuntState,
    part_index: usize,
    old_state: PolyState,
    new_state: PolyState,
) {
    // Mirrors Part::polyStateChanged - adjusts active_non_releasing_poly_count.
    if old_state == PolyState::Playing || old_state == PolyState::Held {
        if new_state != PolyState::Playing && new_state != PolyState::Held {
            state.parts[part_index].active_non_releasing_poly_count -= 1;
        }
    } else if new_state == PolyState::Playing || new_state == PolyState::Held {
        state.parts[part_index].active_non_releasing_poly_count += 1;
    }
}

fn part_partial_deactivated(state: &mut MuntState, part_index: usize, poly_index: usize) {
    crate::part::partial_deactivated(state, part_index, poly_index);
}
