mod common;

#[allow(dead_code)]
mod golden {
    include!("golden/ym2608_adpcm.rs");
}

use common::{callbacks::*, harness::*};
use ymfm_oxide::{Ym2608, YmfmOpnFidelity};

#[test]
fn adpcm_a_key_on() {
    let adpcm_data = create_adpcm_rom();
    let mut chip = Ym2608::new(RecordingCallbacks2608::with_adpcm_data(adpcm_data));
    chip.reset();
    chip.set_fidelity(YmfmOpnFidelity::Max);
    write_reg_2608(&mut chip, 0x29, 0x80);
    add_ssg_bg_2608(&mut chip);

    // ADPCM-A register 8: channel 0 pan(L+R=0xC0) + level(0x1F)
    write_reg_2608(&mut chip, 0x18, 0xDF);
    // ADPCM-A register 1: total level = 0 (no attenuation)
    write_reg_2608(&mut chip, 0x11, 0x3F);
    // ADPCM-A register 0: key on channel 0
    write_reg_2608(&mut chip, 0x10, 0x01);

    let samples = generate_3(&mut chip, golden::ADPCM_A_KEY_ON.len());
    assert_samples_3(&samples, golden::ADPCM_A_KEY_ON);
}

#[test]
fn adpcm_a_key_on_off() {
    let adpcm_data = create_adpcm_rom();
    let mut chip = Ym2608::new(RecordingCallbacks2608::with_adpcm_data(adpcm_data));
    chip.reset();
    chip.set_fidelity(YmfmOpnFidelity::Max);
    write_reg_2608(&mut chip, 0x29, 0x80);
    add_ssg_bg_2608(&mut chip);

    write_reg_2608(&mut chip, 0x18, 0xDF);
    write_reg_2608(&mut chip, 0x11, 0x3F);
    write_reg_2608(&mut chip, 0x10, 0x01);

    let on_samples = generate_3(&mut chip, golden::ADPCM_A_ON.len());
    assert_samples_3(&on_samples, golden::ADPCM_A_ON);

    // Key off: bit 7 = dump, bits 0-5 = channels to dump
    write_reg_2608(&mut chip, 0x10, 0x80 | 0x01);

    let off_samples = generate_3(&mut chip, golden::ADPCM_A_OFF.len());
    assert_samples_3(&off_samples, golden::ADPCM_A_OFF);
}

#[test]
fn adpcm_a_all_6_channels() {
    let adpcm_data = create_adpcm_rom();
    let mut chip = Ym2608::new(RecordingCallbacks2608::with_adpcm_data(adpcm_data));
    chip.reset();
    chip.set_fidelity(YmfmOpnFidelity::Max);
    write_reg_2608(&mut chip, 0x29, 0x80);
    add_ssg_bg_2608(&mut chip);

    // Set pan+level for all 6 channels (register 8+ch via bus 0x18+ch)
    for ch in 0..6u8 {
        write_reg_2608(&mut chip, 0x18 + ch, 0xDF);
    }
    // Total level = 0 (no attenuation)
    write_reg_2608(&mut chip, 0x11, 0x3F);
    // Key on all 6 channels
    write_reg_2608(&mut chip, 0x10, 0x3F);

    let samples = generate_3(&mut chip, golden::ADPCM_A_ALL_6CH.len());
    assert_samples_3(&samples, golden::ADPCM_A_ALL_6CH);
}

#[test]
fn adpcm_b_playback() {
    let adpcm_data = create_adpcm_rom();
    let mut chip = Ym2608::new(RecordingCallbacks2608::with_adpcm_data(adpcm_data));
    chip.reset();
    chip.set_fidelity(YmfmOpnFidelity::Max);
    write_reg_2608(&mut chip, 0x29, 0x80);
    add_ssg_bg_2608(&mut chip);

    // ADPCM-B register 1: pan L+R
    write_reg_hi(&mut chip, 0x01, 0xC0);
    // Start address = 0x0020 (data at 0x2000 in ROM)
    write_reg_hi(&mut chip, 0x02, 0x20);
    write_reg_hi(&mut chip, 0x03, 0x00);
    // End address = 0x0023
    write_reg_hi(&mut chip, 0x04, 0x23);
    write_reg_hi(&mut chip, 0x05, 0x00);
    // Delta-N = 0x0C49
    write_reg_hi(&mut chip, 0x09, 0x49);
    write_reg_hi(&mut chip, 0x0A, 0x0C);
    // Level = max
    write_reg_hi(&mut chip, 0x0B, 0xFF);
    // Control 1: start + external
    write_reg_hi(&mut chip, 0x00, 0xA0);

    let samples = generate_3(&mut chip, golden::ADPCM_B_PLAYBACK.len());
    assert_samples_3(&samples, golden::ADPCM_B_PLAYBACK);
}

#[test]
fn adpcm_b_end_of_sample() {
    let adpcm_data = create_adpcm_rom();
    let mut chip = Ym2608::new(RecordingCallbacks2608::with_adpcm_data(adpcm_data));
    chip.reset();
    chip.set_fidelity(YmfmOpnFidelity::Max);
    write_reg_2608(&mut chip, 0x29, 0x80);
    add_ssg_bg_2608(&mut chip);

    write_reg_hi(&mut chip, 0x01, 0xC0);
    // Very short sample: start=0x0020, end=0x0020 (256 bytes)
    write_reg_hi(&mut chip, 0x02, 0x20);
    write_reg_hi(&mut chip, 0x03, 0x00);
    write_reg_hi(&mut chip, 0x04, 0x20);
    write_reg_hi(&mut chip, 0x05, 0x00);
    // Max playback rate
    write_reg_hi(&mut chip, 0x09, 0xFF);
    write_reg_hi(&mut chip, 0x0A, 0xFF);
    write_reg_hi(&mut chip, 0x0B, 0xFF);
    write_reg_hi(&mut chip, 0x00, 0xA0);

    let samples = generate_3(&mut chip, golden::ADPCM_B_SHORT_SAMPLE.len());
    assert_samples_3(&samples, golden::ADPCM_B_SHORT_SAMPLE);

    let status_hi = chip.read_status_hi();
    assert_eq!(
        status_hi,
        golden::ADPCM_B_EOS_STATUS_HI,
        "status_hi mismatch: got 0x{status_hi:02X}, expected 0x{:02X}",
        golden::ADPCM_B_EOS_STATUS_HI
    );
}

#[test]
fn external_write() {
    let mut chip = Ym2608::new(RecordingCallbacks2608::new());
    chip.reset();
    chip.set_fidelity(YmfmOpnFidelity::Max);
    write_reg_2608(&mut chip, 0x29, 0x80);
    add_ssg_bg_2608(&mut chip);
    chip.callbacks().take_events();

    // ADPCM-B register 0: reset first
    write_reg_hi(&mut chip, 0x00, 0x01);
    // ADPCM-B register 1: pan L+R
    write_reg_hi(&mut chip, 0x01, 0xC0);
    // Start address
    write_reg_hi(&mut chip, 0x02, 0x00);
    write_reg_hi(&mut chip, 0x03, 0x00);
    // End address
    write_reg_hi(&mut chip, 0x04, 0xFF);
    write_reg_hi(&mut chip, 0x05, 0xFF);
    // Control 1: external + record
    write_reg_hi(&mut chip, 0x00, 0x60);
    // Write a data byte via register 8
    write_reg_hi(&mut chip, 0x08, 0xAB);

    let samples = generate_3(&mut chip, golden::EXTERNAL_WRITE.len());
    assert_samples_3(&samples, golden::EXTERNAL_WRITE);
}
