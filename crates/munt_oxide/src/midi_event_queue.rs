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

// Simple queue implementation using a ring buffer to store incoming MIDI event before the synth
// actually processes it.
// It is intended to:
// - get rid of prerenderer while retaining graceful partial abortion
// - add fair emulation of the MIDI interface delays
// - extend the synth interface with the default implementation of a typical rendering loop.

use crate::state::{MidiEvent, MidiEventQueueState};

impl MidiEventQueueState {
    /// Must be called once after creating MidiEventQueueState.
    /// `ring_buffer_size` must be a power of 2.
    pub(crate) fn init(&mut self, ring_buffer_size: u32) {
        self.ring_buffer_mask = ring_buffer_size - 1;
        self.ring_buffer = vec![MidiEvent::default(); ring_buffer_size as usize];
        self.reset();
    }

    pub(crate) fn reset(&mut self) {
        self.start_position = 0;
        self.end_position = 0;
    }

    pub(crate) fn push_short_message(&mut self, short_message_data: u32, timestamp: u32) -> bool {
        let new_end_position = (self.end_position + 1) & self.ring_buffer_mask;
        if self.start_position == new_end_position {
            return false;
        }
        let new_event = &mut self.ring_buffer[self.end_position as usize];
        new_event.sysex_data = None;
        new_event.short_message_data = short_message_data;
        new_event.timestamp = timestamp;
        self.end_position = new_end_position;
        true
    }

    pub(crate) fn push_sysex(&mut self, sysex_data: &[u8], timestamp: u32) -> bool {
        let new_end_position = (self.end_position + 1) & self.ring_buffer_mask;
        if self.start_position == new_end_position {
            return false;
        }
        let new_event = &mut self.ring_buffer[self.end_position as usize];
        new_event.sysex_data = Some(sysex_data.to_vec());
        new_event.short_message_data = sysex_data.len() as u32;
        new_event.timestamp = timestamp;
        self.end_position = new_end_position;
        true
    }

    pub(crate) fn peek(&self) -> Option<&MidiEvent> {
        if self.is_empty() {
            None
        } else {
            Some(&self.ring_buffer[self.start_position as usize])
        }
    }

    pub(crate) fn drop_front(&mut self) {
        if self.is_empty() {
            return;
        }
        // Reclaim sysex storage by dropping the Vec.
        self.ring_buffer[self.start_position as usize].sysex_data = None;
        self.start_position = (self.start_position + 1) & self.ring_buffer_mask;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.start_position == self.end_position
    }
}

#[cfg(test)]
mod tests {
    use crate::state::MidiEventQueueState;

    fn make_queue(size: u32) -> MidiEventQueueState {
        let mut state = MidiEventQueueState {
            ring_buffer: Vec::new(),
            ring_buffer_mask: 0,
            start_position: 0,
            end_position: 0,
        };
        state.init(size);
        state
    }

    #[test]
    fn empty_on_creation() {
        let state = make_queue(8);
        assert!(state.is_empty());
        assert!(state.peek().is_none());
    }

    #[test]
    fn push_peek_drop_fifo() {
        let mut state = make_queue(8);
        assert!(state.push_short_message(0x007F3C90, 100));
        assert!(state.push_short_message(0x00003C90, 200));
        assert!(!state.is_empty());

        let event = state.peek().unwrap();
        assert_eq!(event.short_message_data, 0x007F3C90);
        assert_eq!(event.timestamp, 100);
        assert!(event.sysex_data.is_none());

        state.drop_front();

        let event = state.peek().unwrap();
        assert_eq!(event.short_message_data, 0x00003C90);
        assert_eq!(event.timestamp, 200);

        state.drop_front();
        assert!(state.is_empty());
    }

    #[test]
    fn sysex_data_preserved() {
        let mut state = make_queue(8);
        let sysex = vec![0xF0, 0x41, 0x10, 0x16, 0x12, 0xF7];
        assert!(state.push_sysex(&sysex, 300));

        let event = state.peek().unwrap();
        assert_eq!(event.sysex_data.as_ref().unwrap(), &sysex);
        assert_eq!(event.short_message_data, sysex.len() as u32);
        assert_eq!(event.timestamp, 300);

        state.drop_front();
        assert!(state.is_empty());
    }

    #[test]
    fn full_queue_rejects() {
        let mut state = make_queue(4); // capacity 4, usable slots = 3
        assert!(state.push_short_message(1, 0));
        assert!(state.push_short_message(2, 0));
        assert!(state.push_short_message(3, 0));
        assert!(!state.push_short_message(4, 0)); // full
    }

    #[test]
    fn wrap_around() {
        let mut state = make_queue(4);
        for round in 0..3u32 {
            for i in 0..3u32 {
                assert!(state.push_short_message(round * 10 + i, 0));
            }
            for i in 0..3u32 {
                let event = state.peek().unwrap();
                assert_eq!(event.short_message_data, round * 10 + i);
                state.drop_front();
            }
            assert!(state.is_empty());
        }
    }

    #[test]
    fn reset_clears() {
        let mut state = make_queue(8);
        state.push_short_message(1, 0);
        state.push_short_message(2, 0);
        state.reset();
        assert!(state.is_empty());
    }

    #[test]
    fn drop_on_empty_is_noop() {
        let mut state = make_queue(8);
        state.drop_front(); // should not panic
        assert!(state.is_empty());
    }
}
