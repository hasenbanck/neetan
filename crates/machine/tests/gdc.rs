use common::{Bus, CpuMode, MachineModel};
use machine::Pc9801Bus;

const GDC_SLAVE_DATA_PORT: u16 = 0xA0;
const GDC_SLAVE_COMMAND_PORT: u16 = 0xA2;

const VRAM_B: u32 = 0xA8000;
const VRAM_R: u32 = 0xB0000;
const VRAM_G: u32 = 0xB8000;

fn gdc_slave_set_cursor(bus: &mut Pc9801Bus, address: u32, dot_address: u8) {
    bus.io_write_byte(GDC_SLAVE_COMMAND_PORT, 0x49);
    bus.io_write_byte(GDC_SLAVE_DATA_PORT, address as u8);
    bus.io_write_byte(GDC_SLAVE_DATA_PORT, (address >> 8) as u8);
    bus.io_write_byte(
        GDC_SLAVE_DATA_PORT,
        ((address >> 16) as u8 & 0x03) | ((dot_address & 0x0F) << 4),
    );
}

fn gdc_slave_write_masked_word(bus: &mut Pc9801Bus, value: u16) {
    bus.io_write_byte(GDC_SLAVE_COMMAND_PORT, 0x20);
    bus.io_write_byte(GDC_SLAVE_DATA_PORT, value as u8);
    bus.io_write_byte(GDC_SLAVE_DATA_PORT, (value >> 8) as u8);
}

fn gdc_slave_draw_single_bit(bus: &mut Pc9801Bus, address: u32, dot_address: u8) {
    let mask = 1u16 << dot_address;
    gdc_slave_set_cursor(bus, address, dot_address);
    gdc_slave_write_masked_word(bus, mask);
}

#[test]
fn gdc_slave_direct_writes_select_vram_plane_from_address() {
    let mut bus = Pc9801Bus::new(MachineModel::PC9801VX, CpuMode::Low, 48000);
    bus.io_write_byte(0x7C, 0x00);

    gdc_slave_draw_single_bit(&mut bus, 0x04016, 15);
    gdc_slave_draw_single_bit(&mut bus, 0x08016, 14);
    gdc_slave_draw_single_bit(&mut bus, 0x0C016, 13);

    let byte_offset = 0x0016 * 2 + 1;
    assert_eq!(
        bus.read_byte_direct(VRAM_B + byte_offset),
        0x01,
        "GDC address 0x04016 should write the B plane"
    );
    assert_eq!(
        bus.read_byte_direct(VRAM_R + byte_offset),
        0x02,
        "GDC address 0x08016 should write the R plane"
    );
    assert_eq!(
        bus.read_byte_direct(VRAM_G + byte_offset),
        0x04,
        "GDC address 0x0C016 should write the G plane"
    );
}
