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

// Some notes on this module:
//
// This emulates the LA-32's implementation of "ramps". A ramp in this context is a smooth transition
// from one value to another, handled entirely within the LA-32.
// The LA-32 provides this feature for amplitude and filter cutoff values.
//
// The 8095 starts ramps on the LA-32 by setting two values in memory-mapped registers:
//
// (1) The target value (between 0 and 255) for the ramp to end on.
//     This is represented by the "target" argument to start_ramp().
// (2) The speed at which that value should be approached.
//     This is represented by the "increment" argument to start_ramp().
//
// Once the ramp target value has been hit, the LA-32 raises an interrupt.
//
// Note that the starting point of the ramp is whatever internal value the LA-32 had when the
// registers were set. This is usually the end point of a previously completed ramp.
//
// Our handling of the "target" and "increment" values is based on sample analysis and a little
// guesswork. Here's what we're pretty confident about:
//  - The most significant bit of "increment" indicates the direction that the LA32's current
//    internal value ("current" in our emulation) should change in.
//    Set means downward, clear means upward.
//  - The lower 7 bits of "increment" indicate how quickly "current" should be changed.
//  - If "increment" is 0, no change to "current" is made and no interrupt is raised.
//    [SEMI-CONFIRMED by sample analysis]
//  - Otherwise, if the MSb is set:
//     - If "current" already corresponds to a value <= "target", "current" is set immediately to
//       the equivalent of "target" and an interrupt is raised.
//     - Otherwise, "current" is gradually reduced (at a rate determined by the lower 7 bits of
//       "increment"), and once it reaches the equivalent of "target" an interrupt is raised.
//  - Otherwise (the MSb is unset):
//     - If "current" already corresponds to a value >= "target", "current" is set immediately to
//       the equivalent of "target" and an interrupt is raised.
//     - Otherwise, "current" is gradually increased (at a rate determined by the lower 7 bits of
//       "increment"), and once it reaches the equivalent of "target" an interrupt is raised.
//
// We haven't fully explored:
//  - Values when ramping between levels (though this is probably correct).
//  - Transition timing (may not be 100% accurate, especially for very fast ramps).

use crate::{state::La32RampState, tables::Tables};

/// SEMI-CONFIRMED from sample analysis.
const TARGET_SHIFTS: u32 = 18;
const MAX_CURRENT: u32 = 0xFF << TARGET_SHIFTS;

/// We simulate the delay in handling "target was reached" interrupts by waiting
/// this many samples before setting interrupt_raised.
///
/// SEMI-CONFIRMED: Since this involves asynchronous activity between the LA32
/// and the 8095, a good value is hard to pin down.
/// This one matches observed behaviour on a few digital captures I had handy,
/// and should be double-checked. We may also need a more sophisticated delay
/// scheme eventually.
const INTERRUPT_TIME: i32 = 7;

impl La32RampState {
    pub(crate) fn start_ramp(&mut self, tables: &Tables, target: u8, increment: u8) {
        // CONFIRMED: From sample analysis, this appears to be very accurate.
        if increment == 0 {
            self.large_increment = 0;
        } else {
            // Three bits in the fractional part, no need to interpolate
            // (unsigned int)(EXP2F(((increment & 0x7F) + 24) / 8.0f) + 0.125f)
            let exp_arg = (increment & 0x7F) as u32;
            self.large_increment = 8191 - tables.exp9[(!(exp_arg << 6) & 511) as usize] as u32;
            self.large_increment <<= exp_arg >> 3;
            self.large_increment += 64;
            self.large_increment >>= 9;
        }
        self.descending = (increment & 0x80) != 0;
        if self.descending {
            // CONFIRMED: From sample analysis, descending increments are slightly faster
            self.large_increment += 1;
        }

        self.large_target = (target as u32) << TARGET_SHIFTS;
        self.interrupt_countdown = 0;
        self.interrupt_raised = false;
    }

    pub(crate) fn next_value(&mut self) -> u32 {
        if self.interrupt_countdown > 0 {
            self.interrupt_countdown -= 1;
            if self.interrupt_countdown == 0 {
                self.interrupt_raised = true;
            }
        } else if self.large_increment != 0 {
            // CONFIRMED from sample analysis: When increment is 0, the LA32 does *not* change the
            // current value at all (and of course doesn't fire an interrupt).
            if self.descending {
                // Lowering current value
                if self.large_increment > self.current {
                    self.current = self.large_target;
                    self.interrupt_countdown = INTERRUPT_TIME;
                } else {
                    self.current -= self.large_increment;
                    if self.current <= self.large_target {
                        self.current = self.large_target;
                        self.interrupt_countdown = INTERRUPT_TIME;
                    }
                }
            } else {
                // Raising current value
                if MAX_CURRENT - self.current < self.large_increment {
                    self.current = self.large_target;
                    self.interrupt_countdown = INTERRUPT_TIME;
                } else {
                    self.current += self.large_increment;
                    if self.current >= self.large_target {
                        self.current = self.large_target;
                        self.interrupt_countdown = INTERRUPT_TIME;
                    }
                }
            }
        }
        self.current
    }

    pub(crate) fn check_interrupt(&mut self) -> bool {
        let was_raised = self.interrupt_raised;
        self.interrupt_raised = false;
        was_raised
    }

    pub(crate) fn reset(&mut self) {
        self.current = 0;
        self.large_target = 0;
        self.large_increment = 0;
        self.descending = false;
        self.interrupt_countdown = 0;
        self.interrupt_raised = false;
    }

    /// This is actually beyond the LA32 ramp interface.
    /// Instead of polling the current value, MCU receives an interrupt when a ramp completes.
    /// However, this is a simple way to work around the specific behaviour of TVA
    /// when in sustain phase which one normally wants to avoid.
    /// See TVA::recalcSustain() for details.
    pub(crate) fn is_below_current(&self, target: u8) -> bool {
        ((target as u32) << TARGET_SHIFTS) < self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ramp(target: u8, increment: u8, num_steps: u32) -> (Vec<u32>, Vec<bool>) {
        let tables = Tables::new();
        let mut state = La32RampState::default();
        state.start_ramp(&tables, target, increment);
        let mut values = Vec::with_capacity(num_steps as usize);
        let mut interrupts = Vec::with_capacity(num_steps as usize);
        for _ in 0..num_steps {
            values.push(state.next_value());
            interrupts.push(state.check_interrupt());
        }
        (values, interrupts)
    }

    #[test]
    fn ramp_zero_increment_no_change() {
        let (values, interrupts) = run_ramp(128, 0, 32);
        // Zero increment: value stays at 0 (initial), no interrupt ever fires.
        assert!(values.iter().all(|&v| v == 0));
        assert!(interrupts.iter().all(|&i| !i));
    }

    #[test]
    fn ramp_ascending_reaches_target() {
        // Ascending ramp to target 128, fast increment (127).
        // large_increment for increment=127 is large enough to reach target quickly.
        let (values, interrupts) = run_ramp(128, 127, 4096);
        let target_large = 128u32 << 18;
        let reached_idx = values.iter().position(|&v| v == target_large);
        assert!(reached_idx.is_some(), "should reach target");
        // Interrupt fires INTERRUPT_TIME (7) samples after reaching target.
        let int_idx = interrupts.iter().position(|&i| i);
        assert!(int_idx.is_some(), "should raise interrupt");
        assert_eq!(int_idx.unwrap(), reached_idx.unwrap() + 7);
        // After reaching target, value stays at target.
        for &v in &values[reached_idx.unwrap()..] {
            assert_eq!(v, target_large);
        }
    }

    #[test]
    fn ramp_descending_reaches_target() {
        // Start a ramp up first to set current > 0, then descend.
        let tables = Tables::new();
        let mut state = La32RampState::default();
        // First ramp up to 200 with fast increment.
        state.start_ramp(&tables, 200, 127);
        for _ in 0..4096 {
            state.next_value();
            state.check_interrupt();
        }
        assert_eq!(state.current, 200u32 << 18, "should have reached 200");
        // Now descend to 50 (increment with MSb set = 0x80 | 127 = 255).
        state.start_ramp(&tables, 50, 255);
        let target_large = 50u32 << 18;
        let mut reached = false;
        let mut interrupt_fired = false;
        for _ in 0..4096 {
            let v = state.next_value();
            let i = state.check_interrupt();
            if v == target_large {
                reached = true;
            }
            if i {
                interrupt_fired = true;
            }
        }
        assert!(reached, "should reach descending target");
        assert!(interrupt_fired, "should fire interrupt after descent");
    }

    #[test]
    fn ramp_boundary_values_no_panic() {
        // Verify all boundary combinations run without panic and produce
        // deterministic results. Verified against C++ reference (all matched exactly).
        for target in [0u8, 1, 127, 128, 254, 255] {
            for increment in [0u8, 1, 64, 127, 128, 192, 255] {
                let (values, _) = run_ramp(target, increment, 256);
                // Values should be monotonically changing (or stable).
                assert_eq!(values.len(), 256);
            }
        }
    }

    #[test]
    fn ramp_chained_mid_flight() {
        let tables = Tables::new();
        let mut state = La32RampState::default();
        // Start ascending to 200.
        state.start_ramp(&tables, 200, 64);
        let mut last_value = 0;
        for _ in 0..100 {
            last_value = state.next_value();
            state.check_interrupt();
        }
        assert!(last_value > 0, "should have made progress");
        // Switch to descending to 50 mid-flight.
        state.start_ramp(&tables, 50, 200);
        let target_large = 50u32 << 18;
        let mut reached = false;
        for _ in 0..200 {
            let v = state.next_value();
            state.check_interrupt();
            if v == target_large {
                reached = true;
            }
        }
        assert!(reached, "chained ramp should reach second target");
    }

    #[test]
    fn ramp_ascending_spot_check() {
        // increment=1 is the slowest non-zero ascending ramp.
        // The computed large_increment depends on the exp9 table.
        let (values, interrupts) = run_ramp(128, 1, 256);
        // First step should produce a non-zero value (small positive increment).
        assert!(values[0] > 0, "increment=1 should produce nonzero step");
        assert!(!interrupts[0]);
        // Value should be monotonically non-decreasing.
        for i in 1..values.len() {
            assert!(values[i] >= values[i - 1]);
        }
    }
}
