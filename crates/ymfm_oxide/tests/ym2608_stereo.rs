mod common;

use common::harness::*;
use ymfm_oxide::YmfmOpnFidelity;

#[allow(dead_code)]
mod golden {
    include!("golden/ym2608_stereo.rs");
}

#[test]
fn default_pan_center() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    key_on_2608(&mut chip, 0);
    let samples = generate_3(&mut chip, golden::CENTER_PAN.len());
    assert_samples_3(&samples, golden::CENTER_PAN);
}

#[test]
fn pan_left_only() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    write_reg_2608(&mut chip, 0xB4, 0x80);
    key_on_2608(&mut chip, 0);
    let samples = generate_3(&mut chip, golden::LEFT_PAN.len());
    assert_samples_3(&samples, golden::LEFT_PAN);
}

#[test]
fn pan_right_only() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    write_reg_2608(&mut chip, 0xB4, 0x40);
    key_on_2608(&mut chip, 0);
    let samples = generate_3(&mut chip, golden::RIGHT_PAN.len());
    assert_samples_3(&samples, golden::RIGHT_PAN);
}

#[test]
fn pan_both() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    write_reg_2608(&mut chip, 0xB4, 0xC0);
    key_on_2608(&mut chip, 0);
    let samples = generate_3(&mut chip, golden::BOTH_PAN.len());
    assert_samples_3(&samples, golden::BOTH_PAN);
}

#[test]
fn pan_mute() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    write_reg_2608(&mut chip, 0xB4, 0x00);
    key_on_2608(&mut chip, 0);
    let samples = generate_3(&mut chip, golden::MUTE_PAN.len());
    assert_samples_3(&samples, golden::MUTE_PAN);
}

#[test]
fn per_channel_independent_panning() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 0, 7, 0);
    write_reg_2608(&mut chip, 0xB4, 0x80);
    setup_ym2608_simple_tone(&mut chip, 1, 7, 0);
    write_reg_2608(&mut chip, 0xA5, 0x26);
    write_reg_2608(&mut chip, 0xA1, 0xD5);
    write_reg_2608(&mut chip, 0xB5, 0x40);
    key_on_2608(&mut chip, 0);
    key_on_2608(&mut chip, 1);
    let samples = generate_3(&mut chip, golden::INDEPENDENT_PAN.len());
    assert_samples_3(&samples, golden::INDEPENDENT_PAN);
}

#[test]
fn high_bank_panning() {
    let mut chip = setup_ym2608(YmfmOpnFidelity::Max);
    add_ssg_bg_2608(&mut chip);
    setup_ym2608_simple_tone(&mut chip, 3, 7, 0);
    write_reg_hi(&mut chip, 0xB4, 0x80);
    key_on_2608(&mut chip, 3);
    let samples = generate_3(&mut chip, golden::HIGH_BANK_LEFT_PAN.len());
    assert_samples_3(&samples, golden::HIGH_BANK_LEFT_PAN);
}
