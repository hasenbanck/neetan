mod common;

use common::harness::*;
use ymfm_oxide::YmfmOpnFidelity;

#[allow(dead_code)]
mod golden {
    include!("golden/ym2203_ssg.rs");
}

#[test]
fn silence_after_reset() {
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    let samples = generate_4(&mut chip, golden::SILENCE.len());
    assert_samples_4(&samples, golden::SILENCE);
}

#[test]
fn tone_channel_a() {
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x00, 0x10);
    write_reg(&mut chip, 0x01, 0x00);
    write_reg(&mut chip, 0x07, 0x3E);
    write_reg(&mut chip, 0x08, 0x0F);
    let samples = generate_4(&mut chip, golden::TONE_A.len());
    assert_samples_4(&samples, golden::TONE_A);
}

#[test]
fn all_three_channels_independent() {
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x00, 0x10);
    write_reg(&mut chip, 0x01, 0x00);
    write_reg(&mut chip, 0x02, 0x20);
    write_reg(&mut chip, 0x03, 0x00);
    write_reg(&mut chip, 0x04, 0x40);
    write_reg(&mut chip, 0x05, 0x00);
    write_reg(&mut chip, 0x07, 0x38);
    write_reg(&mut chip, 0x08, 0x0F);
    write_reg(&mut chip, 0x09, 0x0F);
    write_reg(&mut chip, 0x0A, 0x0F);
    let samples = generate_4(&mut chip, golden::THREE_CHANNELS.len());
    assert_samples_4(&samples, golden::THREE_CHANNELS);
}

#[test]
fn noise_channel() {
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x06, 0x01);
    write_reg(&mut chip, 0x07, 0x37);
    write_reg(&mut chip, 0x08, 0x0F);
    let samples = generate_4(&mut chip, golden::NOISE.len());
    assert_samples_4(&samples, golden::NOISE);
}

#[test]
fn tone_plus_noise_mixer() {
    // Tone only
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x00, 0x10);
    write_reg(&mut chip, 0x01, 0x00);
    write_reg(&mut chip, 0x06, 0x0A);
    write_reg(&mut chip, 0x07, 0x3E);
    write_reg(&mut chip, 0x08, 0x0F);
    let samples = generate_4(&mut chip, golden::MIXER_TONE_ONLY.len());
    assert_samples_4(&samples, golden::MIXER_TONE_ONLY);

    // Noise only
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x00, 0x10);
    write_reg(&mut chip, 0x01, 0x00);
    write_reg(&mut chip, 0x06, 0x0A);
    write_reg(&mut chip, 0x07, 0x37);
    write_reg(&mut chip, 0x08, 0x0F);
    let samples = generate_4(&mut chip, golden::MIXER_NOISE_ONLY.len());
    assert_samples_4(&samples, golden::MIXER_NOISE_ONLY);

    // Both tone + noise
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x00, 0x10);
    write_reg(&mut chip, 0x01, 0x00);
    write_reg(&mut chip, 0x06, 0x0A);
    write_reg(&mut chip, 0x07, 0x36);
    write_reg(&mut chip, 0x08, 0x0F);
    let samples = generate_4(&mut chip, golden::MIXER_TONE_AND_NOISE.len());
    assert_samples_4(&samples, golden::MIXER_TONE_AND_NOISE);
}

#[test]
fn envelope_mode() {
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
    add_fm_bg_2203(&mut chip);
    write_reg(&mut chip, 0x00, 0x10);
    write_reg(&mut chip, 0x01, 0x00);
    write_reg(&mut chip, 0x07, 0x3E);
    write_reg(&mut chip, 0x08, 0x10);
    write_reg(&mut chip, 0x0B, 0x20);
    write_reg(&mut chip, 0x0C, 0x00);
    write_reg(&mut chip, 0x0D, 0x08);
    let samples = generate_4(&mut chip, golden::ENVELOPE.len());
    assert_samples_4(&samples, golden::ENVELOPE);
}

#[test]
fn all_16_envelope_shapes() {
    let golden_shapes: [&[[i32; 4]]; 16] = [
        golden::ENVELOPE_SHAPE_0,
        golden::ENVELOPE_SHAPE_1,
        golden::ENVELOPE_SHAPE_2,
        golden::ENVELOPE_SHAPE_3,
        golden::ENVELOPE_SHAPE_4,
        golden::ENVELOPE_SHAPE_5,
        golden::ENVELOPE_SHAPE_6,
        golden::ENVELOPE_SHAPE_7,
        golden::ENVELOPE_SHAPE_8,
        golden::ENVELOPE_SHAPE_9,
        golden::ENVELOPE_SHAPE_10,
        golden::ENVELOPE_SHAPE_11,
        golden::ENVELOPE_SHAPE_12,
        golden::ENVELOPE_SHAPE_13,
        golden::ENVELOPE_SHAPE_14,
        golden::ENVELOPE_SHAPE_15,
    ];

    for shape in 0..16u8 {
        let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
        add_fm_bg_2203(&mut chip);
        write_reg(&mut chip, 0x00, 0x10);
        write_reg(&mut chip, 0x01, 0x00);
        write_reg(&mut chip, 0x07, 0x3E);
        write_reg(&mut chip, 0x08, 0x10);
        write_reg(&mut chip, 0x0B, 0x10);
        write_reg(&mut chip, 0x0C, 0x00);
        write_reg(&mut chip, 0x0D, shape);
        let expected = golden_shapes[shape as usize];
        let samples = generate_4(&mut chip, expected.len());
        assert_samples_4(&samples, expected);
    }
}

#[test]
fn ssg_register_readback() {
    let mut chip = setup_ym2203(YmfmOpnFidelity::Max);

    for &(addr, value, mask, expected_readback) in golden::REGISTER_READBACK {
        write_reg(&mut chip, addr, value);
        chip.write_address(addr);
        let readback = chip.read_data();
        assert_eq!(
            readback & mask,
            expected_readback & mask,
            "SSG register 0x{addr:02X} readback mismatch: got 0x{readback:02X}, expected 0x{expected_readback:02X}",
        );
    }
}

#[test]
fn ssg_output_at_different_fidelities() {
    let cases: [(&[[i32; 4]], YmfmOpnFidelity); 3] = [
        (golden::FIDELITY_MAX, YmfmOpnFidelity::Max),
        (golden::FIDELITY_MED, YmfmOpnFidelity::Med),
        (golden::FIDELITY_MIN, YmfmOpnFidelity::Min),
    ];

    for (expected, fidelity) in cases {
        let mut chip = setup_ym2203(fidelity);
        add_fm_bg_2203(&mut chip);
        write_reg(&mut chip, 0x00, 0x10);
        write_reg(&mut chip, 0x01, 0x00);
        write_reg(&mut chip, 0x07, 0x3E);
        write_reg(&mut chip, 0x08, 0x0F);
        let samples = generate_4(&mut chip, expected.len());
        assert_samples_4(&samples, expected);
    }
}

#[test]
fn amplitude_levels() {
    let cases: [(u8, &[[i32; 4]]); 4] = [
        (0x00, golden::AMPLITUDE_00),
        (0x05, golden::AMPLITUDE_05),
        (0x0A, golden::AMPLITUDE_0A),
        (0x0F, golden::AMPLITUDE_0F),
    ];

    for (amp, expected) in cases {
        let mut chip = setup_ym2203(YmfmOpnFidelity::Max);
        add_fm_bg_2203(&mut chip);
        write_reg(&mut chip, 0x00, 0x10);
        write_reg(&mut chip, 0x01, 0x00);
        write_reg(&mut chip, 0x07, 0x3E);
        write_reg(&mut chip, 0x08, amp);
        let samples = generate_4(&mut chip, expected.len());
        assert_samples_4(&samples, expected);
    }
}
