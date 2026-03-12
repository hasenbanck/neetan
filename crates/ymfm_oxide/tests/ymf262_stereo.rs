mod common;

use common::harness::*;

#[allow(dead_code)]
mod golden {
    include!("golden/ymf262_stereo.rs");
}

#[test]
fn output_a_only() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01 | 0x10);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_A_ONLY.len());
    assert_samples_4(&samples, golden::OUTPUT_A_ONLY);
}

#[test]
fn output_b_only() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01 | 0x20);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_B_ONLY.len());
    assert_samples_4(&samples, golden::OUTPUT_B_ONLY);
}

#[test]
fn output_c_only() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01 | 0x40);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_C_ONLY.len());
    assert_samples_4(&samples, golden::OUTPUT_C_ONLY);
}

#[test]
fn output_d_only() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01 | 0x80);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_D_ONLY.len());
    assert_samples_4(&samples, golden::OUTPUT_D_ONLY);
}

#[test]
fn output_ab() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01 | 0x30);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_AB.len());
    assert_samples_4(&samples, golden::OUTPUT_AB);
}

#[test]
fn output_all() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01 | 0xF0);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_ALL.len());
    assert_samples_4(&samples, golden::OUTPUT_ALL);
}

#[test]
fn output_mute() {
    let mut chip = setup_ymf262();
    write_reg_opl3(&mut chip, 0xC0, 0x01);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    write_reg_opl3(&mut chip, 0xB0, 0x31);
    let samples = generate_4_opl3(&mut chip, golden::OUTPUT_MUTE.len());
    assert_samples_4(&samples, golden::OUTPUT_MUTE);
}

#[test]
fn per_channel_independent_routing() {
    let mut chip = setup_ymf262();
    // ch0 → output A
    write_reg_opl3(&mut chip, 0xC0, 0x11);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA0, 0x41);
    // ch1 → output B
    write_reg_opl3(&mut chip, 0xC1, 0x21);
    for op in 0..2u8 {
        let off = opl_op_offset(1, op);
        write_reg_opl3(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3(&mut chip, 0xA1, 0x81);
    key_on_opl3(&mut chip, 0);
    key_on_opl3(&mut chip, 1);
    let samples = generate_4_opl3(&mut chip, golden::INDEPENDENT_ROUTING.len());
    assert_samples_4(&samples, golden::INDEPENDENT_ROUTING);
}

#[test]
fn high_bank_routing() {
    let mut chip = setup_ymf262();
    write_reg_opl3_hi(&mut chip, 0xC0, 0x11);
    for op in 0..2u8 {
        let off = opl_op_offset(0, op);
        write_reg_opl3_hi(&mut chip, 0x20 + off, 0x21);
        write_reg_opl3_hi(&mut chip, 0x40 + off, 0x00);
        write_reg_opl3_hi(&mut chip, 0x60 + off, 0xF0);
        write_reg_opl3_hi(&mut chip, 0x80 + off, 0x0F);
        write_reg_opl3_hi(&mut chip, 0xE0 + off, 0x00);
    }
    write_reg_opl3_hi(&mut chip, 0xA0, 0x41);
    key_on_opl3(&mut chip, 9);
    let samples = generate_4_opl3(&mut chip, golden::HIGH_BANK_ROUTING.len());
    assert_samples_4(&samples, golden::HIGH_BANK_ROUTING);
}
