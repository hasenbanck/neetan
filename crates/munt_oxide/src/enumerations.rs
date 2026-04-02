// Copyright (C) 2003, 2004, 2005, 2006, 2008, 2009 Dean Beeler, Jerome Fisher
// Copyright (C) 2011-2022 Dean Beeler, Jerome Fisher, Sergey V. Mikayev
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

/// Methods for emulating the connection between the LA32 and the DAC, which involves
/// some hacks in the real devices for doubling the volume.
/// See also http://en.wikipedia.org/wiki/Roland_MT-32#Digital_overflow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum DacInputMode {
    /// Produces samples at double the volume, without tricks.
    /// Nicer overdrive characteristics than the DAC hacks (it simply clips samples within range)
    /// Higher quality than the real devices
    Nice = 0,
}

/// Methods for emulating the effective delay of incoming MIDI messages introduced by a MIDI interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum MidiDelayMode {
    /// Process incoming MIDI events immediately.
    Immediate = 0,

    /// Delay incoming short MIDI messages as if they were transferred via a MIDI cable
    /// to a real hardware unit and immediate sysex processing.
    /// This ensures more accurate timing of simultaneous NoteOn messages.
    DelayShortMessagesOnly = 1,

    /// Delay all incoming MIDI events as if they were transferred via a MIDI cable
    /// to a real hardware unit.
    DelayAll = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum PolyState {
    Playing = 0,
    // This marks keys that have been released on the keyboard, but are being held by the pedal
    Held = 1,
    Releasing = 2,
    #[default]
    Inactive = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum ReverbMode {
    Room = 0,
    Hall = 1,
    Plate = 2,
    TapDelay = 3,
}
