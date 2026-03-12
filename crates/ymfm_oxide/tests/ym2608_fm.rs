mod common;

#[allow(dead_code)]
mod golden {
    include!("golden/ym2608_fm.rs");
}

use common::harness::*;
use ymfm_oxide::YmfmOpnFidelity;

const YM2608_CLOCK: u32 = 7_987_200;

#[test]
fn sample_rate() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    assert_eq!(chip.sample_rate(YM2608_CLOCK), 998_400);
}

#[test]
fn silence_after_reset() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    let samples = generate_3(&mut chip, golden::SILENCE.len());
    assert_samples_3(&samples, golden::SILENCE);
}

#[test]
fn low_bank_single_tone() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    key_on_2608(&mut chip, 0);
    let samples = generate_3(&mut chip, golden::LOW_BANK_TONE.len());
    assert_samples_3(&samples, golden::LOW_BANK_TONE);
}

#[test]
fn high_bank_channels_3_5() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 3, 7, 0);
    key_on_2608(&mut chip, 3);
    let samples = generate_3(&mut chip, golden::HIGH_BANK_CH3.len());
    assert_samples_3(&samples, golden::HIGH_BANK_CH3);
}

#[test]
fn all_6_channels_simultaneous() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);

    let freqs: [(u8, u8); 6] = [
        (0x22, 0x69),
        (0x24, 0x80),
        (0x26, 0xD5),
        (0x22, 0x40),
        (0x28, 0x50),
        (0x2A, 0xA0),
    ];

    for ch in 0..6u8 {
        setup_ym2608_simple_tone(&mut chip, ch, 7, 0);
        let (hi, lo) = freqs[ch as usize];
        if ch < 3 {
            write_reg_2608(&mut chip, 0xA4 + ch, hi);
            write_reg_2608(&mut chip, 0xA0 + ch, lo);
        } else {
            write_reg_hi(&mut chip, 0xA4 + (ch - 3), hi);
            write_reg_hi(&mut chip, 0xA0 + (ch - 3), lo);
        }
        key_on_2608(&mut chip, ch);
    }

    let samples = generate_3(&mut chip, golden::ALL_6_CHANNELS.len());
    assert_samples_3(&samples, golden::ALL_6_CHANNELS);
}

#[test]
fn all_8_algorithms() {
    let golden_algos: [&[[i32; 3]]; 8] = [
        golden::ALGO_0,
        golden::ALGO_1,
        golden::ALGO_2,
        golden::ALGO_3,
        golden::ALGO_4,
        golden::ALGO_5,
        golden::ALGO_6,
        golden::ALGO_7,
    ];

    for algo in 0..8u8 {
        let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
        add_ssg_bg_2608(&mut chip);

        write_reg_2608(&mut chip, 0xB0, algo);

        for (op_offset, tl) in [(0x00, 0x20), (0x04, 0x20), (0x08, 0x20), (0x0C, 0x00)] {
            write_reg_2608(&mut chip, 0x30 + op_offset, 0x01);
            write_reg_2608(&mut chip, 0x40 + op_offset, tl);
            write_reg_2608(&mut chip, 0x50 + op_offset, 0x1F);
            write_reg_2608(&mut chip, 0x60 + op_offset, 0x00);
            write_reg_2608(&mut chip, 0x70 + op_offset, 0x00);
            write_reg_2608(&mut chip, 0x80 + op_offset, 0x0F);
            write_reg_2608(&mut chip, 0x90 + op_offset, 0x00);
        }

        write_reg_2608(&mut chip, 0xA4, 0x22);
        write_reg_2608(&mut chip, 0xA0, 0x69);
        write_reg_2608(&mut chip, 0xB4, 0xC0);
        key_on_2608(&mut chip, 0);

        let expected = golden_algos[algo as usize];
        let samples = generate_3(&mut chip, expected.len());
        assert_samples_3(&samples, expected);
    }
}

#[test]
fn lfo_enable_and_modulation() {
    // LFO off
    {
        let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
        add_ssg_bg_2608(&mut chip);

        write_reg_2608(&mut chip, 0xB0, 0x00);

        for (op_offset, tl) in [(0x00, 0x20), (0x04, 0x20), (0x08, 0x20), (0x0C, 0x00)] {
            write_reg_2608(&mut chip, 0x30 + op_offset, 0x01);
            write_reg_2608(&mut chip, 0x40 + op_offset, tl);
            write_reg_2608(&mut chip, 0x50 + op_offset, 0x1F);
            write_reg_2608(&mut chip, 0x60 + op_offset, 0x00);
            write_reg_2608(&mut chip, 0x70 + op_offset, 0x00);
            write_reg_2608(&mut chip, 0x80 + op_offset, 0x0F);
            write_reg_2608(&mut chip, 0x90 + op_offset, 0x00);
        }

        write_reg_2608(&mut chip, 0xA4, 0x22);
        write_reg_2608(&mut chip, 0xA0, 0x69);
        write_reg_2608(&mut chip, 0xB4, 0xC0);
        key_on_2608(&mut chip, 0);

        let samples = generate_3(&mut chip, golden::LFO_OFF.len());
        assert_samples_3(&samples, golden::LFO_OFF);
    }

    // LFO on
    {
        let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
        add_ssg_bg_2608(&mut chip);

        write_reg_2608(&mut chip, 0x22, 0x0F); // LFO on, rate=7
        write_reg_2608(&mut chip, 0xB0, 0x00);

        for (op_offset, tl) in [(0x00, 0x20), (0x04, 0x20), (0x08, 0x20), (0x0C, 0x00)] {
            write_reg_2608(&mut chip, 0x30 + op_offset, 0x01);
            write_reg_2608(&mut chip, 0x40 + op_offset, tl);
            write_reg_2608(&mut chip, 0x50 + op_offset, 0x1F);
            write_reg_2608(&mut chip, 0x60 + op_offset, 0x80); // AM=1
            write_reg_2608(&mut chip, 0x70 + op_offset, 0x00);
            write_reg_2608(&mut chip, 0x80 + op_offset, 0x0F);
            write_reg_2608(&mut chip, 0x90 + op_offset, 0x00);
        }

        write_reg_2608(&mut chip, 0xA4, 0x22);
        write_reg_2608(&mut chip, 0xA0, 0x69);
        write_reg_2608(&mut chip, 0xB4, 0xFF); // AMS=3, PMS=7
        key_on_2608(&mut chip, 0);

        let samples = generate_3(&mut chip, golden::LFO_ON.len());
        assert_samples_3(&samples, golden::LFO_ON);
    }
}

#[test]
fn lfo_rate_sweep() {
    let golden_rates: [&[[i32; 3]]; 8] = [
        golden::LFO_RATE_0,
        golden::LFO_RATE_1,
        golden::LFO_RATE_2,
        golden::LFO_RATE_3,
        golden::LFO_RATE_4,
        golden::LFO_RATE_5,
        golden::LFO_RATE_6,
        golden::LFO_RATE_7,
    ];

    for rate in 0..8u8 {
        let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
        add_ssg_bg_2608(&mut chip);

        write_reg_2608(&mut chip, 0x22, 0x08 | rate);
        write_reg_2608(&mut chip, 0xB0, 0x00);

        for (op_offset, tl) in [(0x00, 0x20), (0x04, 0x20), (0x08, 0x20), (0x0C, 0x00)] {
            write_reg_2608(&mut chip, 0x30 + op_offset, 0x01);
            write_reg_2608(&mut chip, 0x40 + op_offset, tl);
            write_reg_2608(&mut chip, 0x50 + op_offset, 0x1F);
            write_reg_2608(&mut chip, 0x60 + op_offset, 0x80); // AM=1
            write_reg_2608(&mut chip, 0x70 + op_offset, 0x00);
            write_reg_2608(&mut chip, 0x80 + op_offset, 0x0F);
            write_reg_2608(&mut chip, 0x90 + op_offset, 0x00);
        }

        write_reg_2608(&mut chip, 0xA4, 0x22);
        write_reg_2608(&mut chip, 0xA0, 0x69);
        write_reg_2608(&mut chip, 0xB4, 0xFF);
        key_on_2608(&mut chip, 0);

        let expected = golden_rates[rate as usize];
        let samples = generate_3(&mut chip, expected.len());
        assert_samples_3(&samples, expected);
    }
}

#[test]
fn ssg_output_in_ym2608() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    key_on_2608(&mut chip, 0);

    write_reg_2608(&mut chip, 0x00, 0x10);
    write_reg_2608(&mut chip, 0x01, 0x00);
    write_reg_2608(&mut chip, 0x07, 0x3E);
    write_reg_2608(&mut chip, 0x08, 0x0F);

    let samples = generate_3(&mut chip, golden::SSG_OUTPUT.len());
    assert_samples_3(&samples, golden::SSG_OUTPUT);
}
