mod common;

use common::harness::*;

#[allow(dead_code)]
mod golden {
    include!("golden/ym3812_fm.rs");
}

#[test]
fn silence_after_reset() {
    let mut chip = setup_ym3812();
    let samples = generate_1_opl2(&mut chip, golden::SILENCE.len());
    assert_samples_1(&samples, golden::SILENCE);
}

#[test]
fn single_tone_algo1_additive() {
    let mut chip = setup_ym3812();
    setup_opl2_simple_tone(&mut chip, 0, 1, 0);
    key_on_opl2(&mut chip, 0);
    let samples = generate_1_opl2(&mut chip, golden::SINGLE_TONE_ALGO1.len());
    assert_samples_1(&samples, golden::SINGLE_TONE_ALGO1);
}

#[test]
fn single_tone_algo0_fm() {
    let mut chip = setup_ym3812();
    setup_opl2_simple_tone(&mut chip, 0, 0, 0);
    key_on_opl2(&mut chip, 0);
    let samples = generate_1_opl2(&mut chip, golden::SINGLE_TONE_ALGO0.len());
    assert_samples_1(&samples, golden::SINGLE_TONE_ALGO0);
}

#[test]
fn feedback_sweep() {
    let golden_fbs: [&[[i32; 1]]; 8] = [
        golden::FEEDBACK_0,
        golden::FEEDBACK_1,
        golden::FEEDBACK_2,
        golden::FEEDBACK_3,
        golden::FEEDBACK_4,
        golden::FEEDBACK_5,
        golden::FEEDBACK_6,
        golden::FEEDBACK_7,
    ];

    for (fb, expected) in golden_fbs.iter().enumerate() {
        let mut chip = setup_ym3812();
        setup_opl2_simple_tone(&mut chip, 0, 0, fb as u8);
        key_on_opl2(&mut chip, 0);
        let samples = generate_1_opl2(&mut chip, expected.len());
        assert_samples_1(&samples, expected);
    }
}

#[test]
fn full_adsr_envelope_cycle() {
    let mut chip = setup_ym3812();
    setup_opl2_simple_tone(&mut chip, 0, 1, 0);
    key_on_opl2(&mut chip, 0);
    let sustain = generate_1_opl2(&mut chip, golden::ADSR_SUSTAIN.len());
    assert_samples_1(&sustain, golden::ADSR_SUSTAIN);

    key_off_opl2(&mut chip, 0);
    let release = generate_1_opl2(&mut chip, golden::ADSR_RELEASE.len());
    assert_samples_1(&release, golden::ADSR_RELEASE);
}

#[test]
fn rhythm_mode() {
    let mut chip = setup_ym3812();
    for ch in 6..9u8 {
        setup_opl2_simple_tone(&mut chip, ch, 1, 0);
    }
    write_reg_opl2(&mut chip, 0xBD, 0x3F);
    let samples = generate_1_opl2(&mut chip, golden::RHYTHM_MODE.len());
    assert_samples_1(&samples, golden::RHYTHM_MODE);
}

#[test]
fn two_channels() {
    let mut chip = setup_ym3812();
    setup_opl2_simple_tone(&mut chip, 0, 1, 0);
    setup_opl2_simple_tone(&mut chip, 1, 1, 0);
    write_reg_opl2(&mut chip, 0xA1, 0x81);
    key_on_opl2(&mut chip, 0);
    key_on_opl2(&mut chip, 1);
    let samples = generate_1_opl2(&mut chip, golden::TWO_CHANNELS.len());
    assert_samples_1(&samples, golden::TWO_CHANNELS);
}

#[test]
fn all_9_channels() {
    let mut chip = setup_ym3812();
    for ch in 0..9u8 {
        setup_opl2_simple_tone(&mut chip, ch, 1, 0);
        write_reg_opl2(&mut chip, 0xA0 + ch, 0x41 + ch * 8);
        key_on_opl2(&mut chip, ch);
    }
    let samples = generate_1_opl2(&mut chip, golden::ALL_9_CHANNELS.len());
    assert_samples_1(&samples, golden::ALL_9_CHANNELS);
}

#[test]
fn waveform_select() {
    let golden_wfs: [&[[i32; 1]]; 4] = [
        golden::WAVEFORM_0,
        golden::WAVEFORM_1,
        golden::WAVEFORM_2,
        golden::WAVEFORM_3,
    ];

    for (wf, expected) in golden_wfs.iter().enumerate() {
        let mut chip = setup_ym3812();
        write_reg_opl2(&mut chip, 0x01, 0x20); // enable waveform select
        setup_opl2_simple_tone(&mut chip, 0, 1, 0);
        for op in 0..2u8 {
            let off = opl_op_offset(0, op);
            write_reg_opl2(&mut chip, 0xE0 + off, wf as u8);
        }
        key_on_opl2(&mut chip, 0);
        let samples = generate_1_opl2(&mut chip, expected.len());
        assert_samples_1(&samples, expected);
    }
}

#[test]
fn waveform_gate_disabled_vs_enabled() {
    // Without waveform select enabled
    {
        let mut chip = setup_ym3812();
        setup_opl2_simple_tone(&mut chip, 0, 1, 0);
        for op in 0..2u8 {
            let off = opl_op_offset(0, op);
            write_reg_opl2(&mut chip, 0xE0 + off, 0x02);
        }
        key_on_opl2(&mut chip, 0);
        let samples = generate_1_opl2(&mut chip, golden::WAVEFORM_DISABLED.len());
        assert_samples_1(&samples, golden::WAVEFORM_DISABLED);
    }

    // With waveform select enabled
    {
        let mut chip = setup_ym3812();
        write_reg_opl2(&mut chip, 0x01, 0x20);
        setup_opl2_simple_tone(&mut chip, 0, 1, 0);
        for op in 0..2u8 {
            let off = opl_op_offset(0, op);
            write_reg_opl2(&mut chip, 0xE0 + off, 0x02);
        }
        key_on_opl2(&mut chip, 0);
        let samples = generate_1_opl2(&mut chip, golden::WAVEFORM_ENABLED.len());
        assert_samples_1(&samples, golden::WAVEFORM_ENABLED);
    }
}

#[test]
fn key_reon_during_release() {
    let mut chip = setup_ym3812();
    setup_opl2_simple_tone(&mut chip, 0, 1, 0);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl2(&mut chip, 0x80 + off, 0x84);
    }
    key_on_opl2(&mut chip, 0);
    let sustain = generate_1_opl2(&mut chip, golden::REON_SUSTAIN.len());
    assert_samples_1(&sustain, golden::REON_SUSTAIN);

    key_off_opl2(&mut chip, 0);
    let release = generate_1_opl2(&mut chip, golden::REON_RELEASE.len());
    assert_samples_1(&release, golden::REON_RELEASE);

    key_on_opl2(&mut chip, 0);
    let reon = generate_1_opl2(&mut chip, golden::REON_AFTER.len());
    assert_samples_1(&reon, golden::REON_AFTER);
}
