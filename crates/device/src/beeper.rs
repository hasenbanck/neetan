//! PC-98 beeper: PIT channel 1 square wave gated by PPI port C bit 3.
//!
//! Generates audio samples analytically without scheduler events. Mid-frame
//! state changes (PPI gate toggles and PIT counter reloads) are logged and
//! replayed during sample generation for cycle-accurate output.

use std::ops::{Deref, DerefMut};

/// Base amplitude for the beeper square wave.
const BEEPER_BASE_AMPLITUDE: f32 = 0.5;

/// Polynomial Band-Limited Step correction.
fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Fractional sample remainder for drift-free sample count accumulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampleRemainder(pub f64);

impl Eq for SampleRemainder {}

impl Default for SampleRemainder {
    fn default() -> Self {
        Self(0.0)
    }
}

/// The beeper state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeeperState {
    /// Whether the buzzer is enabled (PPI port C bit 3 inverted: 0 = sound on).
    pub buzzer_enabled: bool,
    /// PIT channel 1 reload value.
    pub pit_reload: u16,
    /// CPU cycle when PIT channel 1 was last loaded.
    pub pit_last_load_cycle: u64,
    /// CPU cycle at which the current audio frame started.
    pub frame_start_cycle: u64,
    /// Fractional sample remainder carried across frames.
    pub sample_remainder: SampleRemainder,
}

struct BuzzerTransition {
    cycle: u64,
    enabled: bool,
}

struct PitTransition {
    cycle: u64,
    reload: u16,
    last_load_cycle: u64,
}

/// PC-98 beeper device.
pub struct Beeper {
    /// Embedded state for save/restore.
    pub state: BeeperState,
    /// Buzzer state at the start of the current frame (before any transitions).
    pre_frame_buzzer: bool,
    /// PIT reload at the start of the current frame.
    pre_frame_pit_reload: u16,
    /// PIT last load cycle at the start of the current frame.
    pre_frame_pit_last_load: u64,
    buzzer_transitions: Vec<BuzzerTransition>,
    pit_transitions: Vec<PitTransition>,
}

impl Deref for Beeper {
    type Target = BeeperState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for Beeper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for Beeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Beeper {
    /// Creates a new beeper in the muted state.
    pub fn new() -> Self {
        Self {
            state: BeeperState {
                buzzer_enabled: false,
                pit_reload: 0,
                pit_last_load_cycle: 0,
                frame_start_cycle: 0,
                sample_remainder: SampleRemainder::default(),
            },
            pre_frame_buzzer: false,
            pre_frame_pit_reload: 0,
            pre_frame_pit_last_load: 0,
            buzzer_transitions: Vec::new(),
            pit_transitions: Vec::new(),
        }
    }

    /// Records a buzzer gate change. Called when PPI port C bit 3 changes.
    pub fn set_buzzer_enabled(&mut self, enabled: bool, cycle: u64) {
        if enabled != self.state.buzzer_enabled {
            self.buzzer_transitions
                .push(BuzzerTransition { cycle, enabled });
            self.state.buzzer_enabled = enabled;
        }
    }

    /// Records a PIT channel 1 reload. Called when PIT ch1 counter is loaded.
    pub fn set_pit_reload(&mut self, reload: u16, last_load_cycle: u64) {
        self.pit_transitions.push(PitTransition {
            cycle: last_load_cycle,
            reload,
            last_load_cycle,
        });
        self.state.pit_reload = reload;
        self.state.pit_last_load_cycle = last_load_cycle;
    }

    /// Fills `output` with interleaved stereo audio samples (`[L, R, L, R, …]`)
    /// for the current frame, returning the number of `f32` values written
    /// (i.e. `frames × 2`).
    ///
    /// Covers the interval `[frame_start_cycle, frame_end_cycle)`. After
    /// generation, advances `frame_start_cycle` and clears transition logs.
    pub fn generate_samples(
        &mut self,
        frame_end_cycle: u64,
        cpu_clock_hz: u32,
        pit_clock_hz: u32,
        sample_rate: u32,
        volume: f32,
        output: &mut [f32],
    ) -> usize {
        let frame_start = self.state.frame_start_cycle;
        let frame_cycles = frame_end_cycle.saturating_sub(frame_start);
        if frame_cycles == 0 || sample_rate == 0 {
            self.finish_frame(frame_end_cycle);
            return 0;
        }

        let frame_capacity = output.len() / 2;
        let exact_samples = (frame_cycles as f64 * f64::from(sample_rate))
            / f64::from(cpu_clock_hz)
            + self.state.sample_remainder.0;
        let frame_count = (exact_samples as usize).min(frame_capacity);
        self.state.sample_remainder = SampleRemainder(exact_samples - frame_count as f64);

        if frame_count == 0 {
            self.finish_frame(frame_end_cycle);
            return 0;
        }

        let amplitude = volume * BEEPER_BASE_AMPLITUDE;
        let pit_ratio = f64::from(pit_clock_hz) / f64::from(cpu_clock_hz);

        let mut current_buzzer = self.pre_frame_buzzer;
        let mut current_reload = self.pre_frame_pit_reload;
        let mut current_last_load = self.pre_frame_pit_last_load;

        let mut buz_idx = 0;
        let mut pit_idx = 0;

        let cycles_per_sample = frame_cycles as f64 / frame_count as f64;
        let mut dt = if current_reload > 0 {
            (pit_ratio * cycles_per_sample) / f64::from(current_reload)
        } else {
            0.0
        };

        for i in 0..frame_count {
            let cycle = frame_start + ((i as u64 * frame_cycles) / frame_count as u64);

            while buz_idx < self.buzzer_transitions.len()
                && self.buzzer_transitions[buz_idx].cycle <= cycle
            {
                current_buzzer = self.buzzer_transitions[buz_idx].enabled;
                buz_idx += 1;
            }

            while pit_idx < self.pit_transitions.len()
                && self.pit_transitions[pit_idx].cycle <= cycle
            {
                current_reload = self.pit_transitions[pit_idx].reload;
                current_last_load = self.pit_transitions[pit_idx].last_load_cycle;
                pit_idx += 1;
                dt = if current_reload > 0 {
                    (pit_ratio * cycles_per_sample) / f64::from(current_reload)
                } else {
                    0.0
                };
            }

            if !current_buzzer || current_reload == 0 {
                output[i * 2] = 0.0;
                output[i * 2 + 1] = 0.0;
                continue;
            }

            let elapsed_cpu = cycle.saturating_sub(current_last_load);
            let elapsed_pit = elapsed_cpu as f64 * pit_ratio;
            let reload = f64::from(current_reload);
            let phase = (elapsed_pit % reload) / reload;

            let mut value = if phase < 0.5 { 1.0 } else { -1.0 };
            value += poly_blep(phase, dt);
            value -= poly_blep((phase + 0.5) % 1.0, dt);
            let sample = amplitude * value as f32;
            output[i * 2] = sample;
            output[i * 2 + 1] = sample;
        }

        self.finish_frame(frame_end_cycle);
        frame_count * 2
    }

    fn finish_frame(&mut self, frame_end_cycle: u64) {
        self.buzzer_transitions.clear();
        self.pit_transitions.clear();
        self.state.frame_start_cycle = frame_end_cycle;
        self.pre_frame_buzzer = self.state.buzzer_enabled;
        self.pre_frame_pit_reload = self.state.pit_reload;
        self.pre_frame_pit_last_load = self.state.pit_last_load_cycle;
    }
}
