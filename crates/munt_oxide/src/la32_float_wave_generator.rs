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

use std::f32::consts::{PI, TAU};

use crate::{state::*, tables::RES_AMP_DECAY_FACTOR_TABLE};

const MIDDLE_CUTOFF_VALUE: f32 = 128.0;
const RESONANCE_DECAY_THRESHOLD_CUTOFF_VALUE: f32 = 144.0;
const MAX_CUTOFF_VALUE: f32 = 240.0;

// LA32WaveGenerator is aimed to represent the exact model of LA32 wave generator.
// The output square wave is created by adding high / low linear segments in-between
// the rising and falling cosine segments. Basically, it's very similar to the phase distortion synthesis.
// Behaviour of a true resonance filter is emulated by adding decaying sine wave.
// The beginning and the ending of the resonant sine is multiplied by a cosine window.
// To synthesise sawtooth waves, the resulting square wave is multiplied by synchronous cosine wave.

#[inline]
fn exp2f(x: f32) -> f32 {
    f32::exp2(x)
}

#[inline]
fn produce_distorted_sample(sample: f32) -> f32 {
    if sample < -1.0 {
        sample + 2.0
    } else if 1.0 < sample {
        sample - 2.0
    } else {
        sample
    }
}

impl La32FloatWaveGeneratorState {
    fn get_pcm_sample(&self, pcm_rom_data: &[i16], position: u32) -> f32 {
        let mut position = position;
        if position >= self.pcm_wave_length {
            if !self.pcm_wave_looped {
                return 0.0;
            }
            position %= self.pcm_wave_length;
        }
        let address = self.pcm_wave_address_offset as usize + position as usize;
        let pcm_sample = pcm_rom_data[address];
        let sample_value = exp2f(((pcm_sample as i32 & 32767) as f32 - 32787.0) / 2048.0);
        if (pcm_sample as i32 & 32768) == 0 {
            sample_value
        } else {
            -sample_value
        }
    }

    /// Initialise the WG engine for generation of synth partial samples and set up the invariant parameters
    pub(crate) fn init_synth(&mut self, sawtooth_waveform: bool, pulse_width: u8, resonance: u8) {
        self.sawtooth_waveform = sawtooth_waveform;
        self.pulse_width = pulse_width;
        self.resonance = resonance;

        self.wave_pos = 0.0;
        self.last_freq = 0.0;

        self.pcm_wave_address_offset = 0;
        self.pcm_wave_length = 0;
        self.active = true;
    }

    /// Initialise the WG engine for generation of PCM partial samples and set up the invariant parameters
    pub(crate) fn init_pcm(
        &mut self,
        pcm_wave_address_offset: u32,
        pcm_wave_length: u32,
        pcm_wave_looped: bool,
        pcm_wave_interpolated: bool,
    ) {
        self.pcm_wave_address_offset = pcm_wave_address_offset;
        self.pcm_wave_length = pcm_wave_length;
        self.pcm_wave_looped = pcm_wave_looped;
        self.pcm_wave_interpolated = pcm_wave_interpolated;

        self.pcm_position = 0.0;
        self.active = true;
    }

    /// Update parameters with respect to TVP, TVA and TVF, and generate next sample
    /// ampVal - Logarithmic amp of the wave generator
    /// pitch - Logarithmic frequency of the resulting wave
    /// cutoffRampVal - Composed of the base cutoff in range [78..178] left-shifted by 18 bits and the TVF modifier
    pub(crate) fn generate_next_sample(
        &mut self,
        pcm_rom_data: &[i16],
        amp_val: u32,
        pitch: u16,
        cutoff_ramp_val: u32,
    ) -> f32 {
        if !self.active {
            return 0.0;
        }

        let mut sample: f32;

        // SEMI-CONFIRMED: From sample analysis:
        // (1) Tested with a single partial playing PCM wave 77 with pitchCoarse 36 and no keyfollow, velocity follow, etc.
        // This gives results within +/- 2 at the output (before any DAC bitshifting)
        // when sustaining at levels 156 - 255 with no modifiers.
        // (2) Tested with a special square wave partial (internal capture ID tva5) at TVA envelope levels 155-255.
        // This gives deltas between -1 and 0 compared to the real output. Note that this special partial only produces
        // positive amps, so negative still needs to be explored, as well as lower levels.
        //
        // Also still partially unconfirmed is the behaviour when ramping between levels, as well as the timing.

        let amp = exp2f(amp_val as f32 / -1024.0 / 4096.0);
        let freq = exp2f(pitch as f32 / 4096.0 - 16.0) * SAMPLE_RATE as f32;

        if self.is_pcm_wave() {
            // Render PCM waveform
            let len = self.pcm_wave_length as i32;
            let int_pcm_position = self.pcm_position as i32;
            if int_pcm_position >= len && !self.pcm_wave_looped {
                // We're now past the end of a non-looping PCM waveform so it's time to die.
                self.deactivate();
                return 0.0;
            }
            let position_delta = freq * 2048.0 / SAMPLE_RATE as f32;

            // Linear interpolation
            let first_sample = self.get_pcm_sample(pcm_rom_data, int_pcm_position as u32);
            // We observe that for partial structures with ring modulation the interpolation is not applied to the slave PCM partial.
            // It's assumed that the multiplication circuitry intended to perform the interpolation on the slave PCM partial
            // is borrowed by the ring modulation circuit (or the LA32 chip has a similar lack of resources assigned to each partial pair).
            if self.pcm_wave_interpolated {
                sample = first_sample
                    + (self.get_pcm_sample(pcm_rom_data, (int_pcm_position + 1) as u32)
                        - first_sample)
                        * (self.pcm_position - int_pcm_position as f32);
            } else {
                sample = first_sample;
            }

            let new_pcm_position = self.pcm_position + position_delta;
            if self.pcm_wave_looped {
                self.pcm_position = new_pcm_position % self.pcm_wave_length as f32;
            } else {
                self.pcm_position = new_pcm_position;
            }
        } else {
            // Render synthesised waveform
            self.wave_pos *= self.last_freq / freq;
            self.last_freq = freq;

            let res_amp = exp2f(1.0 - (32 - self.resonance as i32) as f32 / 4.0);
            {
                //static const float resAmpFactor = EXP2F(-7);
                //resAmp = EXP2I(resonance << 10) * resAmpFactor;
            }

            // The cutoffModifier may not be supposed to be directly added to the cutoff -
            // it may for example need to be multiplied in some way.
            // The 240 cutoffVal limit was determined via sample analysis (internal Munt capture IDs: glop3, glop4).
            // More research is needed to be sure that this is correct, however.
            let mut cutoff_val = cutoff_ramp_val as f32 / 262144.0;
            if cutoff_val > MAX_CUTOFF_VALUE {
                cutoff_val = MAX_CUTOFF_VALUE;
            }

            // Wave length in samples
            let wave_len = SAMPLE_RATE as f32 / freq;

            // Init cosineLen
            let mut cosine_len = 0.5 * wave_len;
            if cutoff_val > MIDDLE_CUTOFF_VALUE {
                cosine_len *= exp2f((cutoff_val - MIDDLE_CUTOFF_VALUE) / -16.0); // found from sample analysis
            }

            // Start playing in center of first cosine segment
            // relWavePos is shifted by a half of cosineLen
            let mut rel_wave_pos = self.wave_pos + 0.5 * cosine_len;
            if rel_wave_pos > wave_len {
                rel_wave_pos -= wave_len;
            }

            // Ratio of positive segment to wave length
            let mut pulse_len: f32 = 0.5;
            if self.pulse_width > 128 {
                pulse_len = exp2f((64 - self.pulse_width as i32) as f32 / 64.0);
                //static const float pulseLenFactor = EXP2F(-192 / 64);
                //pulseLen = EXP2I((256 - pulseWidthVal) << 6) * pulseLenFactor;
            }
            pulse_len *= wave_len;

            let mut h_len = pulse_len - cosine_len;

            // Ignore pulsewidths too high for given freq
            if h_len < 0.0 {
                h_len = 0.0;
            }

            // Correct resAmp for cutoff in range 50..66
            let mut res_amp = res_amp;
            if (MIDDLE_CUTOFF_VALUE..RESONANCE_DECAY_THRESHOLD_CUTOFF_VALUE).contains(&cutoff_val) {
                res_amp *= (PI * (cutoff_val - MIDDLE_CUTOFF_VALUE) / 32.0).sin();
            }

            // Produce filtered square wave with 2 cosine waves on slopes

            // 1st cosine segment
            if rel_wave_pos < cosine_len {
                sample = -(PI * rel_wave_pos / cosine_len).cos();
            }
            // high linear segment
            else if rel_wave_pos < (cosine_len + h_len) {
                sample = 1.0;
            }
            // 2nd cosine segment
            else if rel_wave_pos < (2.0 * cosine_len + h_len) {
                sample = (PI * (rel_wave_pos - (cosine_len + h_len)) / cosine_len).cos();
            } else {
                // low linear segment
                sample = -1.0;
            }

            if cutoff_val < MIDDLE_CUTOFF_VALUE {
                // Attenuate samples below cutoff 50
                // Found by sample analysis
                sample *= exp2f(-0.125 * (MIDDLE_CUTOFF_VALUE - cutoff_val));
            } else {
                // Add resonance sine. Effective for cutoff > 50 only
                let mut res_sample: f32 = 1.0;

                // Resonance decay speed factor
                let mut res_amp_decay_factor =
                    RES_AMP_DECAY_FACTOR_TABLE[(self.resonance >> 2) as usize] as f32;

                // Now relWavePos counts from the middle of first cosine
                rel_wave_pos = self.wave_pos;

                // negative segments
                if rel_wave_pos >= (cosine_len + h_len) {
                    res_sample = -res_sample;
                    rel_wave_pos -= cosine_len + h_len;

                    // From the digital captures, the decaying speed of the resonance sine is found a bit different for the positive and the negative segments
                    res_amp_decay_factor += 0.25;
                }

                // Resonance sine WG
                res_sample *= (PI * rel_wave_pos / cosine_len).sin();

                // Resonance sine amp
                let res_amp_fade_log2 = -0.125 * res_amp_decay_factor * (rel_wave_pos / cosine_len); // seems to be exact
                let mut res_amp_fade = exp2f(res_amp_fade_log2);

                // Now relWavePos set negative to the left from center of any cosine
                rel_wave_pos = self.wave_pos;

                // negative segment
                if self.wave_pos >= (wave_len - 0.5 * cosine_len) {
                    rel_wave_pos -= wave_len;
                }
                // positive segment
                else if self.wave_pos >= (h_len + 0.5 * cosine_len) {
                    rel_wave_pos -= cosine_len + h_len;
                }

                // To ensure the output wave has no breaks, two different windows are applied to the beginning and the ending of the resonance sine segment
                if rel_wave_pos < 0.5 * cosine_len {
                    let sync_sine = (PI * rel_wave_pos / cosine_len).sin();
                    if rel_wave_pos < 0.0 {
                        // The window is synchronous square sine here
                        res_amp_fade *= sync_sine * sync_sine;
                    } else {
                        // The window is synchronous sine here
                        res_amp_fade *= sync_sine;
                    }
                }

                sample += res_sample * res_amp * res_amp_fade;
            }

            // sawtooth waves
            if self.sawtooth_waveform {
                sample *= (TAU * self.wave_pos / wave_len).cos();
            }

            self.wave_pos += 1.0;

            // wavePos isn't supposed to be > waveLen
            if self.wave_pos > wave_len {
                self.wave_pos -= wave_len;
            }
        }

        // Multiply sample with current TVA value
        sample *= amp;
        sample
    }

    pub(crate) fn deactivate(&mut self) {
        self.active = false;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    /// Return true if the WG engine generates PCM wave samples
    pub(crate) fn is_pcm_wave(&self) -> bool {
        // In the C++ code this checks pcmWaveAddress != NULL.
        // We use pcm_wave_length > 0 as the equivalent (pcm_wave_length is only set in initPCM).
        self.pcm_wave_length > 0
    }
}

impl La32PairState {
    /// ringModulated should be set to false for the structures with mixing or stereo output
    /// ringModulated should be set to true for the structures with ring modulation
    /// mixed is used for the structures with ring modulation and indicates whether the master partial output is mixed to the ring modulator output
    pub(crate) fn init(&mut self, ring_modulated: bool, mixed: bool) {
        self.ring_modulated = ring_modulated;
        self.mixed = mixed;
        self.master_output_sample = 0.0;
        self.slave_output_sample = 0.0;
    }

    /// Initialise the WG engine for generation of synth partial samples and set up the invariant parameters
    pub(crate) fn init_synth(
        &mut self,
        pair_type: PairType,
        sawtooth_waveform: bool,
        pulse_width: u8,
        resonance: u8,
    ) {
        if pair_type == PairType::Master {
            self.master
                .init_synth(sawtooth_waveform, pulse_width, resonance);
        } else {
            self.slave
                .init_synth(sawtooth_waveform, pulse_width, resonance);
        }
    }

    /// Initialise the WG engine for generation of PCM partial samples and set up the invariant parameters
    pub(crate) fn init_pcm(
        &mut self,
        pair_type: PairType,
        pcm_wave_address_offset: u32,
        pcm_wave_length: u32,
        pcm_wave_looped: bool,
    ) {
        if pair_type == PairType::Master {
            self.master.init_pcm(
                pcm_wave_address_offset,
                pcm_wave_length,
                pcm_wave_looped,
                true,
            );
        } else {
            self.slave.init_pcm(
                pcm_wave_address_offset,
                pcm_wave_length,
                pcm_wave_looped,
                !self.ring_modulated,
            );
        }
    }

    /// Update parameters with respect to TVP, TVA and TVF, and generate next sample
    pub(crate) fn generate_next_sample(
        &mut self,
        pcm_rom_data: &[i16],
        pair_type: PairType,
        amp: u32,
        pitch: u16,
        cutoff: u32,
    ) {
        if pair_type == PairType::Master {
            self.master_output_sample =
                self.master
                    .generate_next_sample(pcm_rom_data, amp, pitch, cutoff);
        } else {
            self.slave_output_sample =
                self.slave
                    .generate_next_sample(pcm_rom_data, amp, pitch, cutoff);
        }
    }

    /// Perform mixing / ring modulation and return the result
    pub(crate) fn next_out_sample(&self) -> f32 {
        // Note, LA32FloatWaveGenerator produces each sample normalised in terms of a single playing partial,
        // so the unity sample corresponds to the internal LA32 logarithmic fixed-point unity sample.
        // However, each logarithmic sample is then unlogged to a 14-bit signed integer value, i.e. the max absolute value is 8192.
        // Thus, considering that samples are further mapped to a 16-bit signed integer,
        // we apply a conversion factor 0.25 to produce properly normalised float samples.
        if !self.ring_modulated {
            return 0.25 * (self.master_output_sample + self.slave_output_sample);
        }
        // SEMI-CONFIRMED: Ring modulation model derived from sample analysis of specially constructed patches which exploit distortion.
        // LA32 ring modulator found to produce distorted output in case if the absolute value of maximal amplitude of one of the input partials exceeds 8191.
        // This is easy to reproduce using synth partials with resonance values close to the maximum. It looks like an integer overflow happens in this case.
        // As the distortion is strictly bound to the amplitude of the complete mixed square + resonance wave in the linear space,
        // it is reasonable to assume the ring modulation is performed also in the linear space by sample multiplication.
        // Most probably the overflow is caused by limited precision of the multiplication circuit as the very similar distortion occurs with panning.
        let ring_modulated_sample = produce_distorted_sample(self.master_output_sample)
            * produce_distorted_sample(self.slave_output_sample);
        if self.mixed {
            0.25 * (self.master_output_sample + ring_modulated_sample)
        } else {
            0.25 * ring_modulated_sample
        }
    }

    pub(crate) fn deactivate(&mut self, pair_type: PairType) {
        if pair_type == PairType::Master {
            self.master.deactivate();
            self.master_output_sample = 0.0;
        } else {
            self.slave.deactivate();
            self.slave_output_sample = 0.0;
        }
    }

    pub(crate) fn is_active(&self, pair_type: PairType) -> bool {
        if pair_type == PairType::Master {
            self.master.is_active()
        } else {
            self.slave.is_active()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Checksums (sum, sum_abs) generated by the C++ LA32FloatWaveGenerator reference.
    // Tolerance: sum_abs within 0.1% relative error.

    fn run_synth(
        sawtooth: bool,
        pw: u8,
        res: u8,
        amp: u32,
        pitch: u16,
        cutoff: u32,
        n: u32,
    ) -> (f64, f64) {
        let mut state = La32FloatWaveGeneratorState::default();
        state.init_synth(sawtooth, pw, res);
        let pcm_rom: [i16; 0] = [];
        let mut sum = 0.0f64;
        let mut sum_abs = 0.0f64;
        for _ in 0..n {
            let s = state.generate_next_sample(&pcm_rom, amp, pitch, cutoff) as f64;
            sum += s;
            sum_abs += s.abs();
        }
        (sum, sum_abs)
    }

    fn assert_checksum(label: &str, got: (f64, f64), expected_sum: f64, expected_abs: f64) {
        let tol = expected_abs * 1e-3 + 1e-6;
        assert!(
            (got.0 - expected_sum).abs() < tol,
            "{label}: sum mismatch: got {}, expected {expected_sum}",
            got.0
        );
        assert!(
            (got.1 - expected_abs).abs() < tol,
            "{label}: abs mismatch: got {}, expected {expected_abs}",
            got.1
        );
    }

    #[test]
    fn wave_gen_square_basic() {
        let r = run_synth(false, 128, 1, 0, 32768, 128 << 18, 512);
        assert_checksum("square_basic", r, 0.0, 325.9329652672);
    }

    #[test]
    fn wave_gen_sawtooth_basic() {
        let r = run_synth(true, 128, 1, 0, 32768, 128 << 18, 512);
        assert_checksum("saw_basic", r, -0.0000113991, 162.9419418119);
    }

    #[test]
    fn wave_gen_high_resonance() {
        let r = run_synth(false, 128, 31, 0, 32768, 160 << 18, 512);
        assert_checksum("high_res", r, -1.3869414888, 539.4439182205);
    }

    #[test]
    fn wave_gen_cutoff_regions() {
        let r = run_synth(false, 128, 8, 0, 32768, 100 << 18, 256);
        assert_checksum("low_cutoff", r, 0.0, 14.4043378958);
        let r = run_synth(false, 128, 8, 0, 32768, 128 << 18, 256);
        assert_checksum("mid_cutoff", r, 0.0, 162.9664826336);
        let r = run_synth(false, 128, 8, 0, 32768, 240 << 18, 256);
        assert_checksum("high_cutoff", r, 0.0, 254.0000000874);
    }

    #[test]
    fn wave_gen_pulse_width_sweep() {
        let r = run_synth(false, 0, 8, 0, 32768, 140 << 18, 256);
        assert_checksum("pw_0", r, 0.0037899576, 201.8385918679);
        let r = run_synth(false, 192, 8, 0, 32768, 140 << 18, 256);
        assert_checksum("pw_192", r, -103.5930887866, 201.7428439748);
        let r = run_synth(false, 255, 8, 0, 32768, 140 << 18, 256);
        assert_checksum("pw_255", r, -103.5930887866, 201.7428439748);
    }

    #[test]
    fn wave_gen_deactivated_produces_zero() {
        let mut state = La32FloatWaveGeneratorState::default();
        state.init_synth(false, 128, 8);
        state.deactivate();
        let pcm_rom: [i16; 0] = [];
        for _ in 0..64 {
            assert_eq!(
                state.generate_next_sample(&pcm_rom, 0, 32768, 128 << 18),
                0.0
            );
        }
    }

    #[test]
    fn wave_gen_pair_mixing() {
        let pcm_rom: [i16; 0] = [];
        let mut state = La32PairState::default();
        state.init(false, false);
        state.init_synth(PairType::Master, false, 128, 8);
        state.init_synth(PairType::Slave, false, 128, 4);
        let mut sum = 0.0f64;
        let mut sum_abs = 0.0f64;
        for _ in 0..256 {
            state.generate_next_sample(&pcm_rom, PairType::Master, 0, 32768, 140 << 18);
            state.generate_next_sample(&pcm_rom, PairType::Slave, 0, 36864, 140 << 18);
            let s = state.next_out_sample() as f64;
            sum += s;
            sum_abs += s.abs();
        }
        assert_checksum("pair_mix", (sum, sum_abs), 0.0014280104, 57.3779823524);
    }

    #[test]
    fn wave_gen_pair_ring_modulation() {
        let expected: [(bool, f64, f64); 2] = [
            (false, -0.0028566485, 52.9393187710),
            (true, -0.0055828858, 60.3613168946),
        ];
        let pcm_rom: [i16; 0] = [];
        for (mixed, exp_sum, exp_abs) in expected {
            let mut state = La32PairState::default();
            state.init(true, mixed);
            state.init_synth(PairType::Master, false, 128, 16);
            state.init_synth(PairType::Slave, false, 128, 8);
            let mut sum = 0.0f64;
            let mut sum_abs = 0.0f64;
            for _ in 0..256 {
                state.generate_next_sample(&pcm_rom, PairType::Master, 0, 32768, 160 << 18);
                state.generate_next_sample(&pcm_rom, PairType::Slave, 0, 36864, 160 << 18);
                let s = state.next_out_sample() as f64;
                sum += s;
                sum_abs += s.abs();
            }
            assert_checksum(
                &format!("pair_ring_mixed={mixed}"),
                (sum, sum_abs),
                exp_sum,
                exp_abs,
            );
        }
    }
}
