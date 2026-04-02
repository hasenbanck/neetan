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

// Analog stage emulation - coarse float-only path.
//
// Analog class is dedicated to perform fair emulation of analogue circuitry of hardware units that is responsible
// for processing output signal after the DAC. It appears that the analogue circuit labeled "LPF" on the schematic
// also applies audible changes to the signal spectra. There is a significant boost of higher frequencies observed
// aside from quite poor attenuation of the mirror spectra above 16 kHz which is due to a relatively low filter order.
//
// As the final mixing of multiplexed output signal is performed after the DAC, this function is migrated here from Synth.
// Saying precisely, mixing is performed within the LPF as the entrance resistors are actually components of a LPF
// designed using the multiple feedback topology. Nevertheless, the schematic separates them.

use crate::state::{AnalogState, COARSE_LPF_DELAY_LINE_LENGTH, CoarseLpfState};

/// FIR approximation of the overall impulse response of the cascade composed of the sample & hold circuit and the low pass filter
/// of the MT-32 first generation.
/// The coefficients below are found by windowing the inverse DFT of the 1024 pin frequency response converted to the minimum phase.
/// The frequency response of the LPF is computed directly, the effect of the S&H is approximated by multiplying the LPF frequency
/// response by the corresponding sinc. Although, the LPF has DC gain of 3.2, we ignore this in the emulation and use normalised model.
/// The peak gain of the normalised cascade appears about 1.7 near 11.8 kHz. Relative error doesn't exceed 1% for the frequencies
/// below 12.5 kHz. In the higher frequency range, the relative error is below 8%. Peak error value is at 16 kHz.
const COARSE_LPF_FLOAT_TAPS_MT32: [f32; 9] = [
    1.272_473_7,
    -0.220_267_79,
    -0.158_039_9,
    0.179_603_79,
    -0.111_484_097,
    0.054_137_5,
    -0.023_518_03,
    0.010_997_169,
    -0.006_935_698,
];

/// Similar approximation for new MT-32 and CM-32L/LAPC-I LPF. As the voltage controlled amplifier was introduced, LPF has unity DC gain.
/// The peak gain value shifted towards higher frequencies and a bit higher about 1.83 near 13 kHz.
const COARSE_LPF_FLOAT_TAPS_CM32L: [f32; 9] = [
    1.340_615_6,
    -0.403_331_7,
    0.036_005_517,
    0.066_156_84,
    -0.069_672_53,
    0.049_563_806,
    -0.031_113_416,
    0.019_169_774,
    -0.012_421_368,
];

/// According to the CM-64 PCB schematic, there is a difference in the values of the LPF entrance resistors for the reverb and non-reverb channels.
/// This effectively results in non-unity LPF DC gain for the reverb channel of 0.68 while the LPF has unity DC gain for the LA32 output channels.
/// In emulation, the reverb output gain is multiplied by this factor to compensate for the LPF gain difference.
const CM32L_REVERB_TO_LA32_ANALOG_OUTPUT_GAIN_FACTOR: f32 = 0.68;

const DELAY_LINE_MASK: usize = COARSE_LPF_DELAY_LINE_LENGTH - 1;

fn get_lpf_taps(old_mt32_analog_lpf: bool) -> &'static [f32; 9] {
    if old_mt32_analog_lpf {
        &COARSE_LPF_FLOAT_TAPS_MT32
    } else {
        &COARSE_LPF_FLOAT_TAPS_CM32L
    }
}

fn get_actual_reverb_output_gain(reverb_gain: f32, mt32_reverb_compatibility_mode: bool) -> f32 {
    if mt32_reverb_compatibility_mode {
        reverb_gain
    } else {
        reverb_gain * CM32L_REVERB_TO_LA32_ANALOG_OUTPUT_GAIN_FACTOR
    }
}

impl CoarseLpfState {
    /// CoarseLowPassFilter::process - float specialisation.
    /// For float, clipSampleEx is identity and normaliseSample is identity.
    pub(crate) fn process(&mut self, old_mt32_analog_lpf: bool, in_sample: f32) -> f32 {
        let lpf_taps = get_lpf_taps(old_mt32_analog_lpf);
        let position = self.ring_buffer_position as usize;

        // Tap at index COARSE_LPF_DELAY_LINE_LENGTH (i.e. taps[8]) applied to the oldest sample.
        let mut sample = lpf_taps[COARSE_LPF_DELAY_LINE_LENGTH] * self.ring_buffer[position];

        // clipSampleEx(float) is identity, so we just store the input sample directly.
        self.ring_buffer[position] = in_sample;

        for (i, tap) in lpf_taps
            .iter()
            .enumerate()
            .take(COARSE_LPF_DELAY_LINE_LENGTH)
        {
            sample += tap * self.ring_buffer[(i + position) & DELAY_LINE_MASK];
        }

        self.ring_buffer_position = ((position.wrapping_sub(1)) & DELAY_LINE_MASK) as u32;

        // normaliseSample(float) is identity.
        sample
    }
}

impl AnalogState {
    pub(crate) fn set_synth_output_gain(&mut self, synth_gain: f32) {
        // Float specialisation: store the gain directly.
        self.synth_gain = synth_gain;
    }

    pub(crate) fn set_reverb_output_gain(
        &mut self,
        reverb_gain: f32,
        mt32_reverb_compatibility_mode: bool,
    ) {
        self.reverb_gain =
            get_actual_reverb_output_gain(reverb_gain, mt32_reverb_compatibility_mode);
    }

    // AnalogImpl<FloatSample>::produceOutput - coarse float path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        out_stream: &mut [f32],
        non_reverb_left: &[f32],
        non_reverb_right: &[f32],
        reverb_dry_left: &[f32],
        reverb_dry_right: &[f32],
        reverb_wet_left: &[f32],
        reverb_wet_right: &[f32],
        out_length: u32,
    ) {
        let old_mt32_analog_lpf = self.old_mt32_analog_lpf;
        let synth_gain = self.synth_gain;
        let reverb_gain = self.reverb_gain;

        for i in 0..out_length as usize {
            // Coarse LPF hasNextSample() always returns false, so we always take the else branch.
            let in_sample_l = (non_reverb_left[i] + reverb_dry_left[i]) * synth_gain
                + reverb_wet_left[i] * reverb_gain;
            let in_sample_r = (non_reverb_right[i] + reverb_dry_right[i]) * synth_gain
                + reverb_wet_right[i] * reverb_gain;

            // normaliseSample(float) is identity, so we pass directly to the LPF.
            let out_sample_l = self
                .left_channel_lpf
                .process(old_mt32_analog_lpf, in_sample_l);
            let out_sample_r = self
                .right_channel_lpf
                .process(old_mt32_analog_lpf, in_sample_r);

            // clipSampleEx(float) is identity.
            out_stream[i * 2] = out_sample_l;
            out_stream[i * 2 + 1] = out_sample_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analog_reverb_gain_factor() {
        // CM-32L reverb path multiplies by 0.68 when not in MT-32 compat mode.
        let mut state = AnalogState {
            old_mt32_analog_lpf: false,
            synth_gain: 0.0,
            reverb_gain: 0.0,
            left_channel_lpf: CoarseLpfState::default(),
            right_channel_lpf: CoarseLpfState::default(),
        };
        state.set_reverb_output_gain(1.0, false);
        assert!((state.reverb_gain - 0.68).abs() < 1e-7);
        state.set_reverb_output_gain(1.0, true);
        assert!((state.reverb_gain - 1.0).abs() < 1e-7);
    }
}
