//! PEGC (Packed-pixel Extended Graphics Controller) - 256-color graphics for PC-9821.
//!
//! Provides two operating modes:
//! - **Packed pixel mode**: Each VRAM byte = one 8-bit palette index.
//!   Two 32 KB bank-switched windows at A8000-AFFFF and B0000-B7FFF
//!   select from 16 banks of 32 KB within 512 KB extended VRAM.
//! - **Plane mode**: 8-plane architecture with ROP operations, similar to EGC
//!   but extended to 8 planes for 256 colors.
//!
//! MMIO registers live at E0000-E7FFF (replacing E-plane VRAM when active).
//! 256-color palette uses the same I/O ports (0xA8/0xAA/0xAC/0xAE) as the
//! analog palette but with 8-bit index (0-255) and 8-bit color components.

/// PEGC extended VRAM size: 512 KB.
pub const PEGC_VRAM_SIZE: usize = 0x80000;

/// PEGC bank size: 32 KB.
const BANK_SIZE: usize = 0x8000;

/// MMIO region 1 offsets (E0000-E00FF): bank select registers.
const MMIO1_BANK_A8: u32 = 0x0004;
const MMIO1_BANK_B0: u32 = 0x0006;

/// MMIO region 2 base offset.
const MMIO2_BASE: u32 = 0x0100;

/// MMIO region 2 offsets (relative to E0100).
const REG_MODE: u32 = 0x00;
const REG_VRAM_ENABLE: u32 = 0x02;
const REG_PLANE_ACCESS: u32 = 0x04;
const REG_PLANE_ROP_LOW: u32 = 0x08;
const REG_PLANE_ROP_HIGH: u32 = 0x09;
const REG_DATA_SELECT: u32 = 0x0A;
const REG_MASK_BYTE0: u32 = 0x0C;
const REG_MASK_BYTE1: u32 = 0x0D;
const REG_MASK_BYTE2: u32 = 0x0E;
const REG_MASK_BYTE3: u32 = 0x0F;
const REG_LENGTH_LOW: u32 = 0x10;
const REG_LENGTH_HIGH: u32 = 0x11;
const REG_SHIFT_READ: u32 = 0x12;
const REG_SHIFT_WRITE: u32 = 0x13;
const REG_PALETTE1: u32 = 0x14;
const REG_PALETTE2: u32 = 0x18;
const REG_PATTERN: u32 = 0x20;

/// Screen mode for PEGC 256-color display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PegcScreenMode {
    /// 640x400, 2-screen mode (two 256 KB pages).
    TwoScreen,
    /// 640x480, 1-screen mode (single 512 KB page).
    OneScreen,
}

/// Snapshot of the PEGC state for save/restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PegcState {
    /// Master 256-color mode enable (port 0x6A commands 0x20/0x21).
    pub enabled: bool,
    /// Screen mode selection (port 0x6A commands 0x68/0x69).
    pub screen_mode: PegcScreenMode,
    /// Bank index (0-15) for the A8000-AFFFF window (MMIO E0004h).
    pub bank_a8: u8,
    /// Bank index (0-15) for the B0000-B7FFF window (MMIO E0006h).
    pub bank_b0: u8,
    /// Mode register (MMIO E0100h). Bit 0: 0 = packed pixel, 1 = plane mode.
    pub mode_register: u8,
    /// Upper VRAM enable (MMIO E0102h bit 0). Enables flat access at F00000h.
    pub upper_vram_enabled: bool,
    /// Plane access mask (MMIO E0104h). Bit per plane: 0 = allow write, 1 = inhibit.
    pub plane_access_mask: u8,
    /// ROP register (MMIO E0108h). See bit field documentation on the struct methods.
    pub rop_register: u16,
    /// Data select register (MMIO E010Ah).
    pub data_select: u8,
    /// Write mask (MMIO E010Ch-E010Fh). Per-pixel write enable: 1 = allow, 0 = inhibit.
    pub write_mask: u32,
    /// Block transfer length minus 1 (MMIO E0110h, 12 bits).
    pub block_length: u16,
    /// Read shift amount (MMIO E0112h bits 4-0).
    pub shift_read: u8,
    /// Write shift amount (MMIO E0113h bits 4-0).
    pub shift_write: u8,
    /// Foreground palette color (MMIO E0114h).
    pub palette_color_1: u8,
    /// Background palette color (MMIO E0118h).
    pub palette_color_2: u8,
    /// Pattern register raw bytes (MMIO E0120h-E019Fh, 128 bytes).
    pub pattern_data: Box<[u8; 128]>,
    /// Last VRAM data read buffer for plane mode source operations.
    pub last_vram_data: [u8; 64],
    /// Number of valid bytes in `last_vram_data`.
    pub last_data_length: i32,
    /// Remaining block transfer count for plane mode.
    pub remain: u32,
    /// 256-color palette: 256 entries of [green, red, blue], 8-bit components.
    pub palette_256: Box<[[u8; 3]; 256]>,
    /// Currently selected palette index (port 0xA8 in PEGC mode).
    pub palette_index: u8,
}

/// PEGC controller.
pub struct Pegc {
    /// Embedded state for save/restore.
    pub state: PegcState,
}

impl Default for Pegc {
    fn default() -> Self {
        Self::new()
    }
}

impl Pegc {
    /// Creates a new PEGC controller in its default (disabled) state.
    pub fn new() -> Self {
        Self {
            state: PegcState {
                enabled: false,
                screen_mode: PegcScreenMode::TwoScreen,
                bank_a8: 0,
                bank_b0: 0,
                mode_register: 0,
                upper_vram_enabled: false,
                plane_access_mask: 0,
                rop_register: 0,
                data_select: 0,
                write_mask: 0,
                block_length: 0,
                shift_read: 0,
                shift_write: 0,
                palette_color_1: 0,
                palette_color_2: 0,
                pattern_data: Box::new([0u8; 128]),
                last_vram_data: [0u8; 64],
                last_data_length: 0,
                remain: 0,
                palette_256: Box::new([[0u8; 3]; 256]),
                palette_index: 0,
            },
        }
    }

    /// Returns whether 256-color mode is active.
    pub fn is_256_color_active(&self) -> bool {
        self.state.enabled
    }

    /// Returns whether packed pixel mode is selected (mode register bit 0 = 0).
    pub fn is_packed_pixel_mode(&self) -> bool {
        self.state.mode_register & 1 == 0
    }

    /// Returns whether plane mode is selected (mode register bit 0 = 1).
    pub fn is_plane_mode(&self) -> bool {
        self.state.mode_register & 1 != 0
    }

    /// Returns whether flat VRAM access at F00000h is enabled.
    pub fn is_upper_vram_enabled(&self) -> bool {
        self.state.upper_vram_enabled
    }

    /// Enables or disables 256-color mode (called from port 0x6A handler).
    pub fn set_256_color_enabled(&mut self, enabled: bool) {
        self.state.enabled = enabled;
    }

    /// Sets the screen mode (called from port 0x6A handler).
    pub fn set_screen_mode(&mut self, one_screen: bool) {
        self.state.screen_mode = if one_screen {
            PegcScreenMode::OneScreen
        } else {
            PegcScreenMode::TwoScreen
        };
    }

    /// Sets VRAM access mode to plane (called from port 0x6A command 0x62).
    pub fn set_vram_access_mode_plane(&mut self) {
        self.state.mode_register = 1;
    }

    /// Sets VRAM access mode to packed pixel (called from port 0x6A command 0x63).
    pub fn set_vram_access_mode_packed(&mut self) {
        self.state.mode_register = 0;
    }

    /// Writes the 256-color palette index register (port 0xA8 in PEGC mode).
    pub fn write_palette_index(&mut self, value: u8) {
        self.state.palette_index = value;
    }

    /// Writes a 256-color palette component for the currently selected index.
    ///
    /// `component`: 0 = green (0xAA), 1 = red (0xAC), 2 = blue (0xAE).
    pub fn write_palette_component(&mut self, component: usize, value: u8) {
        let index = self.state.palette_index as usize;
        self.state.palette_256[index][component] = value;
    }

    /// Reads a 256-color palette component for the currently selected index.
    ///
    /// `component`: 0 = green (0xAA), 1 = red (0xAC), 2 = blue (0xAE).
    pub fn read_palette_component(&self, component: usize) -> u8 {
        let index = self.state.palette_index as usize;
        self.state.palette_256[index][component]
    }

    /// Reads a byte from the PEGC MMIO register space (offset within E0000-E7FFF).
    pub fn mmio_read_byte(&self, offset: u32) -> u8 {
        match offset {
            MMIO1_BANK_A8 => self.state.bank_a8,
            MMIO1_BANK_B0 => self.state.bank_b0,
            o if o >= MMIO2_BASE => self.mmio2_read_byte(o - MMIO2_BASE),
            _ => 0x00,
        }
    }

    /// Writes a byte to the PEGC MMIO register space (offset within E0000-E7FFF).
    pub fn mmio_write_byte(&mut self, offset: u32, value: u8) {
        match offset {
            MMIO1_BANK_A8 => self.state.bank_a8 = value & 0x0F,
            MMIO1_BANK_B0 => self.state.bank_b0 = value & 0x0F,
            o if o >= MMIO2_BASE => self.mmio2_write_byte(o - MMIO2_BASE, value),
            _ => {}
        }
    }

    /// Reads a word from the PEGC MMIO register space (offset within E0000-E7FFF).
    pub fn mmio_read_word(&self, offset: u32) -> u16 {
        if offset >= MMIO2_BASE {
            return self.mmio2_read_word(offset - MMIO2_BASE);
        }
        let low = self.mmio_read_byte(offset) as u16;
        let high = self.mmio_read_byte(offset + 1) as u16;
        low | (high << 8)
    }

    /// Writes a word to the PEGC MMIO register space (offset within E0000-E7FFF).
    pub fn mmio_write_word(&mut self, offset: u32, value: u16) {
        if offset >= MMIO2_BASE {
            self.mmio2_write_word(offset - MMIO2_BASE, value);
            return;
        }
        self.mmio_write_byte(offset, value as u8);
        self.mmio_write_byte(offset + 1, (value >> 8) as u8);
    }

    fn mmio2_read_byte(&self, offset: u32) -> u8 {
        if (REG_PATTERN..=0x9F).contains(&offset) {
            return self.pattern_read_byte(offset - REG_PATTERN);
        }
        match offset {
            REG_MODE => self.state.mode_register,
            REG_VRAM_ENABLE => u8::from(self.state.upper_vram_enabled),
            REG_PLANE_ACCESS => self.state.plane_access_mask,
            REG_PLANE_ROP_LOW => self.state.rop_register as u8,
            REG_PLANE_ROP_HIGH => (self.state.rop_register >> 8) as u8,
            REG_DATA_SELECT => self.state.data_select,
            REG_MASK_BYTE0 => self.state.write_mask as u8,
            REG_MASK_BYTE1 => (self.state.write_mask >> 8) as u8,
            REG_MASK_BYTE2 => (self.state.write_mask >> 16) as u8,
            REG_MASK_BYTE3 => (self.state.write_mask >> 24) as u8,
            REG_LENGTH_LOW => self.state.block_length as u8,
            REG_LENGTH_HIGH => (self.state.block_length >> 8) as u8,
            REG_SHIFT_READ => self.state.shift_read,
            REG_SHIFT_WRITE => self.state.shift_write,
            REG_PALETTE1 => self.state.palette_color_1,
            REG_PALETTE2 => self.state.palette_color_2,
            _ => 0x00,
        }
    }

    fn mmio2_write_byte(&mut self, offset: u32, value: u8) {
        if (REG_PATTERN..=0x9F).contains(&offset) {
            self.pattern_write_byte(offset - REG_PATTERN, value);
            return;
        }
        match offset {
            REG_MODE => self.state.mode_register = value & 0x01,
            REG_VRAM_ENABLE => self.state.upper_vram_enabled = value & 0x01 != 0,
            REG_PLANE_ACCESS => self.state.plane_access_mask = value,
            REG_PLANE_ROP_LOW => {
                self.state.rop_register = (self.state.rop_register & 0xFF00) | u16::from(value);
            }
            REG_PLANE_ROP_HIGH => {
                self.state.rop_register =
                    (self.state.rop_register & 0x00FF) | (u16::from(value) << 8);
            }
            REG_DATA_SELECT => self.state.data_select = value,
            REG_MASK_BYTE0 => {
                self.state.write_mask = (self.state.write_mask & 0xFFFF_FF00) | u32::from(value);
            }
            REG_MASK_BYTE1 => {
                self.state.write_mask =
                    (self.state.write_mask & 0xFFFF_00FF) | (u32::from(value) << 8);
            }
            REG_MASK_BYTE2 => {
                self.state.write_mask =
                    (self.state.write_mask & 0xFF00_FFFF) | (u32::from(value) << 16);
            }
            REG_MASK_BYTE3 => {
                self.state.write_mask =
                    (self.state.write_mask & 0x00FF_FFFF) | (u32::from(value) << 24);
            }
            REG_LENGTH_LOW => {
                self.state.block_length = (self.state.block_length & 0x0F00) | u16::from(value);
            }
            REG_LENGTH_HIGH => {
                self.state.block_length =
                    (self.state.block_length & 0x00FF) | ((u16::from(value) & 0x0F) << 8);
            }
            REG_SHIFT_READ => self.state.shift_read = value & 0x1F,
            REG_SHIFT_WRITE => self.state.shift_write = value & 0x1F,
            REG_PALETTE1 => self.state.palette_color_1 = value,
            REG_PALETTE2 => self.state.palette_color_2 = value,
            _ => {}
        }
    }

    fn mmio2_read_word(&self, offset: u32) -> u16 {
        if (REG_PATTERN..=0x9F).contains(&offset) {
            return self.pattern_read_word(offset - REG_PATTERN);
        }
        let low = self.mmio2_read_byte(offset) as u16;
        let high = self.mmio2_read_byte(offset + 1) as u16;
        low | (high << 8)
    }

    fn mmio2_write_word(&mut self, offset: u32, value: u16) {
        if (REG_PATTERN..=0x9F).contains(&offset) {
            self.pattern_write_word(offset - REG_PATTERN, value);
            return;
        }
        self.mmio2_write_byte(offset, value as u8);
        self.mmio2_write_byte(offset + 1, (value >> 8) as u8);
    }

    fn pattern_read_byte(&self, pattern_pos: u32) -> u8 {
        if pattern_pos & 0x03 != 0 {
            return 0x00;
        }
        if self.state.rop_register & 0x8000 != 0 {
            if pattern_pos >= 0x60 {
                return 0x00;
            }
            let bit = (pattern_pos / 4) as u16;
            let mut color = 0u8;
            for plane in 0..8u32 {
                let plane_offset = (plane * 4) as usize;
                let word = u16::from(self.state.pattern_data[plane_offset])
                    | (u16::from(self.state.pattern_data[plane_offset + 1]) << 8);
                color |= (((word >> bit) & 1) as u8) << plane;
            }
            color
        } else {
            if pattern_pos >= 0x40 {
                return 0x00;
            }
            self.state.pattern_data[pattern_pos as usize]
        }
    }

    fn pattern_write_byte(&mut self, pattern_pos: u32, value: u8) {
        if pattern_pos & 0x03 != 0 {
            return;
        }
        if self.state.rop_register & 0x8000 != 0 {
            if pattern_pos >= 0x60 {
                return;
            }
            let bit = (pattern_pos / 4) as u16;
            let mut val = value;
            for plane in 0..8u32 {
                let plane_offset = (plane * 4) as usize;
                let mut word = u16::from(self.state.pattern_data[plane_offset])
                    | (u16::from(self.state.pattern_data[plane_offset + 1]) << 8);
                word = (word & !(1 << bit)) | (u16::from(val & 1) << bit);
                self.state.pattern_data[plane_offset] = word as u8;
                self.state.pattern_data[plane_offset + 1] = (word >> 8) as u8;
                val >>= 1;
            }
        } else {
            if pattern_pos >= 0x40 {
                return;
            }
            self.state.pattern_data[pattern_pos as usize] = value;
        }
    }

    fn pattern_read_word(&self, pattern_pos: u32) -> u16 {
        if pattern_pos & 0x03 != 0 {
            return 0x0000;
        }
        if self.state.rop_register & 0x8000 != 0 {
            if pattern_pos >= 0x60 {
                return 0x0000;
            }
            let bit = (pattern_pos / 4) as u16;
            let mut color = 0u16;
            for plane in 0..8u32 {
                let plane_offset = (plane * 4) as usize;
                let word = u16::from(self.state.pattern_data[plane_offset])
                    | (u16::from(self.state.pattern_data[plane_offset + 1]) << 8);
                color |= ((word >> bit) & 1) << plane;
            }
            color
        } else {
            if pattern_pos >= 0x40 {
                return 0x0000;
            }
            u16::from(self.state.pattern_data[pattern_pos as usize])
                | (u16::from(self.state.pattern_data[pattern_pos as usize + 1]) << 8)
        }
    }

    fn pattern_write_word(&mut self, pattern_pos: u32, value: u16) {
        if pattern_pos & 0x03 != 0 {
            return;
        }
        if self.state.rop_register & 0x8000 != 0 {
            if pattern_pos >= 0x60 {
                return;
            }
            let bit = (pattern_pos / 4) as u16;
            let mut val = value;
            for plane in 0..8u32 {
                let plane_offset = (plane * 4) as usize;
                let mut word = u16::from(self.state.pattern_data[plane_offset])
                    | (u16::from(self.state.pattern_data[plane_offset + 1]) << 8);
                word = (word & !(1 << bit)) | ((val & 1) << bit);
                self.state.pattern_data[plane_offset] = word as u8;
                self.state.pattern_data[plane_offset + 1] = (word >> 8) as u8;
                val >>= 1;
            }
        } else {
            if pattern_pos >= 0x40 {
                return;
            }
            self.state.pattern_data[pattern_pos as usize] = value as u8;
            self.state.pattern_data[pattern_pos as usize + 1] = (value >> 8) as u8;
        }
    }

    /// Reads a byte from PEGC VRAM in packed pixel mode via bank-switched window.
    ///
    /// `window`: 0 = A8000-AFFFF, 1 = B0000-B7FFF.
    /// `offset`: byte offset within the 32 KB window.
    pub fn packed_read_byte(&self, window: u8, offset: u32, vram: &[u8]) -> u8 {
        let bank = if window == 0 {
            self.state.bank_a8
        } else {
            self.state.bank_b0
        };
        let address = bank as usize * BANK_SIZE + (offset as usize & (BANK_SIZE - 1));
        vram[address & (PEGC_VRAM_SIZE - 1)]
    }

    /// Writes a byte to PEGC VRAM in packed pixel mode via bank-switched window.
    pub fn packed_write_byte(&self, window: u8, offset: u32, value: u8, vram: &mut [u8]) {
        let bank = if window == 0 {
            self.state.bank_a8
        } else {
            self.state.bank_b0
        };
        let address = bank as usize * BANK_SIZE + (offset as usize & (BANK_SIZE - 1));
        vram[address & (PEGC_VRAM_SIZE - 1)] = value;
    }

    /// Reads a word from PEGC VRAM in packed pixel mode via bank-switched window.
    pub fn packed_read_word(&self, window: u8, offset: u32, vram: &[u8]) -> u16 {
        let low = self.packed_read_byte(window, offset, vram) as u16;
        let high = self.packed_read_byte(window, offset + 1, vram) as u16;
        low | (high << 8)
    }

    /// Writes a word to PEGC VRAM in packed pixel mode via bank-switched window.
    pub fn packed_write_word(&self, window: u8, offset: u32, value: u16, vram: &mut [u8]) {
        self.packed_write_byte(window, offset, value as u8, vram);
        self.packed_write_byte(window, offset + 1, (value >> 8) as u8, vram);
    }

    /// Reads a word from PEGC VRAM in plane mode.
    ///
    /// Compares 16 consecutive VRAM pixels against palette color 1, returning
    /// a 16-bit mask where bit N is set if pixel N differs from palette color 1
    /// (considering only the planes allowed by the plane access mask).
    /// Optionally updates the pattern register from the read data.
    pub fn plane_read_word(&mut self, offset: u32, vram: &[u8]) -> u16 {
        let rop_reg = self.state.rop_register;
        let source_shift = u32::from(self.state.shift_read);
        let dest_shift = u32::from(self.state.shift_write);
        let block_length = u32::from(self.state.block_length);
        let shift_direction_decrement = rop_reg & (1 << 9) != 0;
        let source_from_cpu = rop_reg & (1 << 8) != 0;
        let pattern_update = rop_reg & (1 << 13) != 0;
        let plane_mask = self.state.plane_access_mask;
        let palette1 = self.state.palette_color_1;

        let base_address = offset.wrapping_mul(8).wrapping_add(source_shift);
        let base_address = if !shift_direction_decrement {
            if self.state.remain != block_length + 1 {
                base_address.wrapping_sub(dest_shift)
            } else {
                base_address
            }
        } else if self.state.remain != block_length + 1 {
            base_address.wrapping_sub(dest_shift)
        } else {
            base_address
        };

        let mut result: u16 = 0;
        self.state.last_data_length = 0;

        if !source_from_cpu {
            for i in 0u32..16 {
                let pixel_address = if shift_direction_decrement {
                    base_address.wrapping_sub(i) & (PEGC_VRAM_SIZE as u32 - 1)
                } else {
                    base_address.wrapping_add(i) & (PEGC_VRAM_SIZE as u32 - 1)
                };

                let data = vram[pixel_address as usize];

                if (data ^ palette1) & !plane_mask != 0 {
                    result |= 1 << i;
                }

                self.state.last_vram_data[i as usize] = data;

                if pattern_update {
                    for plane in (0..8).rev() {
                        let pattern_offset = (plane * 4) as usize;
                        let mut reg_data = u16::from(self.state.pattern_data[pattern_offset])
                            | (u16::from(self.state.pattern_data[pattern_offset + 1]) << 8);
                        reg_data = (reg_data & !(1 << i)) | (u16::from((data >> plane) & 1) << i);
                        self.state.pattern_data[pattern_offset] = reg_data as u8;
                        self.state.pattern_data[pattern_offset + 1] = (reg_data >> 8) as u8;
                    }
                }
            }
        }

        if self.state.last_data_length < 32 {
            self.state.last_data_length += 16;
        }

        result
    }

    /// Writes a word to PEGC VRAM in plane mode with ROP operations.
    ///
    /// Processes 16 consecutive pixels, applying the configured raster operation
    /// (source, destination, pattern truth table) with plane masking and pixel masking.
    pub fn plane_write_word(&mut self, offset: u32, value: u16, vram: &mut [u8]) {
        let rop_reg = self.state.rop_register;
        let rop_code = (rop_reg & 0xFF) as u8;
        let rop_method = ((rop_reg >> 10) & 0x03) as u8;
        let rop_enabled = rop_reg & (1 << 12) != 0;
        let source_from_cpu = rop_reg & (1 << 8) != 0;
        let shift_direction_decrement = rop_reg & (1 << 9) != 0;
        let plane_mask = self.state.plane_access_mask;
        let pixel_mask = self.state.write_mask;
        let block_length = u32::from(self.state.block_length);
        let dest_shift = u32::from(self.state.shift_write);

        if self.state.remain == 0 {
            self.state.remain = block_length + 1;
            self.state.last_data_length = 0;
        }

        let extended_shift_mode = !source_from_cpu || ((block_length + 1) & 0x0F) != 0;

        let mut base_address = offset.wrapping_mul(8);
        let mut data_length: i32 = 16;

        if extended_shift_mode {
            if self.state.remain == block_length + 1 {
                base_address = base_address.wrapping_add(dest_shift);
                data_length -= dest_shift as i32;
            }
        } else {
            base_address = base_address.wrapping_add(dest_shift);
        }
        base_address &= PEGC_VRAM_SIZE as u32 - 1;

        if !shift_direction_decrement {
            for i in 0..data_length {
                let pixel_address =
                    base_address.wrapping_add(i as u32) & (PEGC_VRAM_SIZE as u32 - 1);
                let pixel_mask_bit = pixel_mask_position(i as u32);

                if pixel_mask & pixel_mask_bit != 0 {
                    let source = if source_from_cpu {
                        if value & pixel_mask_bit as u16 != 0 {
                            0xFF
                        } else {
                            0x00
                        }
                    } else {
                        self.state.last_vram_data[i as usize]
                    };

                    let destination = vram[pixel_address as usize];

                    if rop_enabled {
                        let (pattern1, pattern2) = self.get_pattern_colors(rop_method, i as u32);
                        let result = apply_rop(
                            rop_code,
                            source,
                            destination,
                            pattern1,
                            pattern2,
                            plane_mask,
                        );
                        vram[pixel_address as usize] = (destination & plane_mask) | result;
                    } else {
                        vram[pixel_address as usize] =
                            apply_source_copy(source, destination, plane_mask);
                    }
                }

                self.state.remain -= 1;
                if self.state.remain == 0 {
                    break;
                }
            }
        } else {
            for i in 0..data_length {
                let pixel_address =
                    base_address.wrapping_sub(i as u32) & (PEGC_VRAM_SIZE as u32 - 1);
                let pixel_mask_bit = pixel_mask_position(i as u32);

                if pixel_mask & pixel_mask_bit != 0 {
                    let source = if source_from_cpu {
                        if value & pixel_mask_bit as u16 != 0 {
                            0xFF
                        } else {
                            0x00
                        }
                    } else {
                        self.state.last_vram_data[i as usize]
                    };

                    let destination = vram[pixel_address as usize];

                    if rop_enabled {
                        let (pattern1, pattern2) = self.get_pattern_colors(rop_method, i as u32);
                        let result = apply_rop(
                            rop_code,
                            source,
                            destination,
                            pattern1,
                            pattern2,
                            plane_mask,
                        );
                        vram[pixel_address as usize] = (destination & plane_mask) | result;
                    } else {
                        vram[pixel_address as usize] =
                            apply_source_copy(source, destination, plane_mask);
                    }
                }

                self.state.remain -= 1;
                if self.state.remain == 0 {
                    break;
                }
            }
        }

        if self.state.last_data_length > 16 {
            self.state.last_data_length -= 16;
        } else {
            self.state.last_data_length = 0;
        }
    }

    fn get_pattern_colors(&self, method: u8, pixel_index: u32) -> (u8, u8) {
        match method {
            0 => {
                let mut color = 0u8;
                for plane in (0..8).rev() {
                    let pattern_offset = (plane * 4) as usize;
                    let reg_data = u16::from(self.state.pattern_data[pattern_offset])
                        | (u16::from(self.state.pattern_data[pattern_offset + 1]) << 8);
                    color |= (((reg_data >> pixel_index) & 1) as u8) << plane;
                }
                (color, color)
            }
            1 => {
                let color = self.state.palette_color_2;
                (color, color)
            }
            2 => {
                let color = self.state.palette_color_1;
                (color, color)
            }
            3 => (self.state.palette_color_1, self.state.palette_color_2),
            _ => (0, 0),
        }
    }
}

/// Computes the pixel mask bit position for a given pixel index within a 16-pixel group.
///
/// The bit layout maps pixel index to bit position within the 32-bit mask:
/// pixels 0-7 map to bits 7-0 (reversed), pixels 8-15 map to bits 15-8 (reversed).
fn pixel_mask_position(pixel_index: u32) -> u32 {
    let byte_group = (pixel_index / 8) * 8;
    let bit_within_byte = 7 - (pixel_index & 7);
    1 << (byte_group + bit_within_byte)
}

/// Applies the 8-bit ROP truth table to source, destination, and pattern values.
///
/// The ROP code encodes 8 possible outcomes for the (S, D, P) truth table:
/// - Bit 7: S=1, D=1, P=1
/// - Bit 6: S=1, D=1, P=0
/// - Bit 5: S=1, D=0, P=1
/// - Bit 4: S=1, D=0, P=0
/// - Bit 3: S=0, D=1, P=1  (uses pattern2)
/// - Bit 2: S=0, D=1, P=0  (uses pattern2)
/// - Bit 1: S=0, D=0, P=1  (uses pattern2)
/// - Bit 0: S=0, D=0, P=0  (uses pattern2)
fn apply_rop(
    rop_code: u8,
    source: u8,
    destination: u8,
    pattern1: u8,
    pattern2: u8,
    plane_mask: u8,
) -> u8 {
    let not_mask = !plane_mask;
    let mut result: u8 = 0;
    if rop_code & (1 << 7) != 0 {
        result |= (source & destination & pattern1) & not_mask;
    }
    if rop_code & (1 << 6) != 0 {
        result |= (source & destination & !pattern1) & not_mask;
    }
    if rop_code & (1 << 5) != 0 {
        result |= (source & !destination & pattern1) & not_mask;
    }
    if rop_code & (1 << 4) != 0 {
        result |= (source & !destination & !pattern1) & not_mask;
    }
    if rop_code & (1 << 3) != 0 {
        result |= (!source & destination & pattern2) & not_mask;
    }
    if rop_code & (1 << 2) != 0 {
        result |= (!source & destination & !pattern2) & not_mask;
    }
    if rop_code & (1 << 1) != 0 {
        result |= (!source & !destination & pattern2) & not_mask;
    }
    if rop_code & 1 != 0 {
        result |= (!source & !destination & !pattern2) & not_mask;
    }
    result
}

/// Applies source copy with plane masking (non-ROP mode).
///
/// For each bit: if source bit is 1, result = (!plane_mask | destination);
/// if source bit is 0, result = (plane_mask & destination).
fn apply_source_copy(source: u8, destination: u8, plane_mask: u8) -> u8 {
    let mut result: u8 = 0;
    for bit in 0..8u8 {
        let mask = 1 << bit;
        if source & mask != 0 {
            result |= (!plane_mask | destination) & mask;
        } else {
            result |= (plane_mask & destination) & mask;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pegc_defaults_to_disabled() {
        let pegc = Pegc::new();
        assert!(!pegc.is_256_color_active());
        assert!(pegc.is_packed_pixel_mode());
        assert!(!pegc.is_plane_mode());
        assert!(!pegc.is_upper_vram_enabled());
        assert_eq!(pegc.state.bank_a8, 0);
        assert_eq!(pegc.state.bank_b0, 0);
        assert_eq!(pegc.state.screen_mode, PegcScreenMode::TwoScreen);
    }

    #[test]
    fn pegc_enable_disable() {
        let mut pegc = Pegc::new();
        pegc.set_256_color_enabled(true);
        assert!(pegc.is_256_color_active());
        pegc.set_256_color_enabled(false);
        assert!(!pegc.is_256_color_active());
    }

    #[test]
    fn pegc_screen_mode_switching() {
        let mut pegc = Pegc::new();
        assert_eq!(pegc.state.screen_mode, PegcScreenMode::TwoScreen);
        pegc.set_screen_mode(true);
        assert_eq!(pegc.state.screen_mode, PegcScreenMode::OneScreen);
        pegc.set_screen_mode(false);
        assert_eq!(pegc.state.screen_mode, PegcScreenMode::TwoScreen);
    }

    #[test]
    fn vram_access_mode_plane_and_packed() {
        let mut pegc = Pegc::new();
        assert!(pegc.is_packed_pixel_mode());

        pegc.set_vram_access_mode_plane();
        assert!(pegc.is_plane_mode());
        assert!(!pegc.is_packed_pixel_mode());

        pegc.set_vram_access_mode_packed();
        assert!(pegc.is_packed_pixel_mode());
        assert!(!pegc.is_plane_mode());
    }

    #[test]
    fn mmio_bank_select_a8_read_write() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO1_BANK_A8, 0x0A);
        assert_eq!(pegc.mmio_read_byte(MMIO1_BANK_A8), 0x0A);
        pegc.mmio_write_byte(MMIO1_BANK_A8, 0xFF);
        assert_eq!(pegc.mmio_read_byte(MMIO1_BANK_A8), 0x0F);
    }

    #[test]
    fn mmio_bank_select_b0_read_write() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO1_BANK_B0, 0x05);
        assert_eq!(pegc.mmio_read_byte(MMIO1_BANK_B0), 0x05);
        pegc.mmio_write_byte(MMIO1_BANK_B0, 0xF3);
        assert_eq!(pegc.mmio_read_byte(MMIO1_BANK_B0), 0x03);
    }

    #[test]
    fn mmio_mode_register_packed_vs_plane() {
        let mut pegc = Pegc::new();
        assert!(pegc.is_packed_pixel_mode());
        assert!(!pegc.is_plane_mode());

        pegc.mmio_write_byte(MMIO2_BASE + REG_MODE, 0x01);
        assert!(!pegc.is_packed_pixel_mode());
        assert!(pegc.is_plane_mode());

        pegc.mmio_write_byte(MMIO2_BASE + REG_MODE, 0x00);
        assert!(pegc.is_packed_pixel_mode());
    }

    #[test]
    fn mmio_upper_vram_enable() {
        let mut pegc = Pegc::new();
        assert!(!pegc.is_upper_vram_enabled());

        pegc.mmio_write_byte(MMIO2_BASE + REG_VRAM_ENABLE, 0x01);
        assert!(pegc.is_upper_vram_enabled());
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_VRAM_ENABLE), 0x01);

        pegc.mmio_write_byte(MMIO2_BASE + REG_VRAM_ENABLE, 0x00);
        assert!(!pegc.is_upper_vram_enabled());
    }

    #[test]
    fn mmio_plane_access_mask() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_PLANE_ACCESS, 0xA5);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PLANE_ACCESS), 0xA5);
        assert_eq!(pegc.state.plane_access_mask, 0xA5);
    }

    #[test]
    fn mmio_rop_register_bit_fields() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_word(MMIO2_BASE + REG_PLANE_ROP_LOW, 0xABCD);
        assert_eq!(pegc.state.rop_register, 0xABCD);

        let rop_code = pegc.state.rop_register & 0xFF;
        assert_eq!(rop_code, 0xCD);

        let source_from_cpu = (pegc.state.rop_register >> 8) & 1;
        assert_eq!(source_from_cpu, 1);

        let shift_direction = (pegc.state.rop_register >> 9) & 1;
        assert_eq!(shift_direction, 1);

        let rop_method = (pegc.state.rop_register >> 10) & 3;
        assert_eq!(rop_method, 2);

        let rop_enabled = (pegc.state.rop_register >> 12) & 1;
        assert_eq!(rop_enabled, 0);

        let pattern_update = (pegc.state.rop_register >> 13) & 1;
        assert_eq!(pattern_update, 1);
    }

    #[test]
    fn mmio_write_mask() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_MASK_BYTE0, 0x12);
        pegc.mmio_write_byte(MMIO2_BASE + REG_MASK_BYTE1, 0x34);
        pegc.mmio_write_byte(MMIO2_BASE + REG_MASK_BYTE2, 0x56);
        pegc.mmio_write_byte(MMIO2_BASE + REG_MASK_BYTE3, 0x78);
        assert_eq!(pegc.state.write_mask, 0x78563412);
    }

    #[test]
    fn mmio_block_length() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_LENGTH_LOW, 0xFF);
        pegc.mmio_write_byte(MMIO2_BASE + REG_LENGTH_HIGH, 0xFF);
        assert_eq!(pegc.state.block_length, 0x0FFF);
    }

    #[test]
    fn mmio_shift_registers() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_SHIFT_READ, 0x1F);
        assert_eq!(pegc.state.shift_read, 0x1F);
        pegc.mmio_write_byte(MMIO2_BASE + REG_SHIFT_READ, 0xFF);
        assert_eq!(pegc.state.shift_read, 0x1F);
        pegc.mmio_write_byte(MMIO2_BASE + REG_SHIFT_WRITE, 0x0A);
        assert_eq!(pegc.state.shift_write, 0x0A);
    }

    #[test]
    fn mmio_palette_colors() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_PALETTE1, 0x42);
        assert_eq!(pegc.state.palette_color_1, 0x42);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PALETTE1), 0x42);
        pegc.mmio_write_byte(MMIO2_BASE + REG_PALETTE2, 0xAB);
        assert_eq!(pegc.state.palette_color_2, 0xAB);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PALETTE2), 0xAB);
    }

    #[test]
    fn mmio_pattern_register_normal_mode_aligned() {
        let mut pegc = Pegc::new();
        for i in (0..0x40u32).step_by(4) {
            let value = (i as u8).wrapping_mul(7);
            pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN + i, value);
        }
        for i in (0..0x40u32).step_by(4) {
            let expected = (i as u8).wrapping_mul(7);
            assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN + i), expected,);
        }
    }

    #[test]
    fn mmio_pattern_register_normal_mode_rejects_unaligned() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN + 1, 0xAA);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN + 1), 0x00);
        pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN + 2, 0xBB);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN + 2), 0x00);
    }

    #[test]
    fn mmio_pattern_register_normal_mode_rejects_above_0x40() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN + 0x40, 0xCC);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN + 0x40), 0x00);
    }

    #[test]
    fn mmio_pattern_register_transposed_mode_scatter_gather() {
        let mut pegc = Pegc::new();
        pegc.state.rop_register = 0x8000;

        pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN, 0x42);

        let readback = pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN);
        assert_eq!(readback, 0x42);
    }

    #[test]
    fn mmio_pattern_register_transposed_mode_all_pixels() {
        let mut pegc = Pegc::new();
        pegc.state.rop_register = 0x8000;

        for pixel in 0..16u32 {
            let color = (pixel as u8).wrapping_mul(17);
            pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN + pixel * 4, color);
        }
        for pixel in 0..16u32 {
            let expected = (pixel as u8).wrapping_mul(17);
            assert_eq!(
                pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN + pixel * 4),
                expected,
            );
        }
    }

    #[test]
    fn mmio_pattern_register_transposed_rejects_above_0x60() {
        let mut pegc = Pegc::new();
        pegc.state.rop_register = 0x8000;
        pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN + 0x60, 0xDD);
        assert_eq!(pegc.mmio_read_byte(MMIO2_BASE + REG_PATTERN + 0x60), 0x00);
    }

    #[test]
    fn mmio_pattern_register_word_normal_mode() {
        let mut pegc = Pegc::new();
        pegc.mmio_write_word(MMIO2_BASE + REG_PATTERN, 0xBEEF);
        assert_eq!(pegc.mmio_read_word(MMIO2_BASE + REG_PATTERN), 0xBEEF);
        assert_eq!(pegc.state.pattern_data[0], 0xEF);
        assert_eq!(pegc.state.pattern_data[1], 0xBE);
    }

    #[test]
    fn mmio_pattern_register_word_transposed_mode() {
        let mut pegc = Pegc::new();
        pegc.state.rop_register = 0x8000;
        pegc.mmio_write_word(MMIO2_BASE + REG_PATTERN, 0x00FF);
        let readback = pegc.mmio_read_word(MMIO2_BASE + REG_PATTERN);
        assert_eq!(readback, 0x00FF);
    }

    #[test]
    fn packed_pixel_bank_read_write() {
        let pegc = Pegc::new();
        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.packed_write_byte(0, 0x100, 0x42, &mut vram);
        assert_eq!(pegc.packed_read_byte(0, 0x100, &vram), 0x42);
        assert_eq!(vram[0x100], 0x42);
    }

    #[test]
    fn packed_pixel_bank_switching() {
        let mut pegc = Pegc::new();
        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.state.bank_a8 = 0;
        pegc.packed_write_byte(0, 0, 0xAA, &mut vram);

        pegc.state.bank_a8 = 1;
        pegc.packed_write_byte(0, 0, 0xBB, &mut vram);

        pegc.state.bank_a8 = 0;
        assert_eq!(pegc.packed_read_byte(0, 0, &vram), 0xAA);

        pegc.state.bank_a8 = 1;
        assert_eq!(pegc.packed_read_byte(0, 0, &vram), 0xBB);

        assert_eq!(vram[0], 0xAA);
        assert_eq!(vram[BANK_SIZE], 0xBB);
    }

    #[test]
    fn packed_pixel_both_windows_independent() {
        let mut pegc = Pegc::new();
        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.state.bank_a8 = 2;
        pegc.state.bank_b0 = 5;

        pegc.packed_write_byte(0, 0, 0x11, &mut vram);
        pegc.packed_write_byte(1, 0, 0x22, &mut vram);

        assert_eq!(vram[2 * BANK_SIZE], 0x11);
        assert_eq!(vram[5 * BANK_SIZE], 0x22);
    }

    #[test]
    fn packed_pixel_cross_bank_visibility() {
        let mut pegc = Pegc::new();
        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.state.bank_a8 = 3;
        pegc.packed_write_byte(0, 42, 0xCC, &mut vram);

        pegc.state.bank_b0 = 3;
        assert_eq!(pegc.packed_read_byte(1, 42, &vram), 0xCC);
    }

    #[test]
    fn palette_256_all_entries_independent() {
        let mut pegc = Pegc::new();

        for i in 0..=255u8 {
            pegc.write_palette_index(i);
            pegc.write_palette_component(0, i);
            pegc.write_palette_component(1, i.wrapping_mul(2));
            pegc.write_palette_component(2, i.wrapping_mul(3));
        }

        for i in 0..=255u8 {
            pegc.write_palette_index(i);
            assert_eq!(pegc.read_palette_component(0), i);
            assert_eq!(pegc.read_palette_component(1), i.wrapping_mul(2));
            assert_eq!(pegc.read_palette_component(2), i.wrapping_mul(3));
        }
    }

    #[test]
    fn palette_256_8bit_components() {
        let mut pegc = Pegc::new();
        pegc.write_palette_index(100);
        pegc.write_palette_component(0, 0xFF);
        pegc.write_palette_component(1, 0x80);
        pegc.write_palette_component(2, 0x00);
        assert_eq!(pegc.read_palette_component(0), 0xFF);
        assert_eq!(pegc.read_palette_component(1), 0x80);
        assert_eq!(pegc.read_palette_component(2), 0x00);
    }

    #[test]
    fn palette_256_index_full_range() {
        let mut pegc = Pegc::new();
        pegc.write_palette_index(255);
        pegc.write_palette_component(0, 0x42);
        assert_eq!(pegc.state.palette_index, 255);
        assert_eq!(pegc.state.palette_256[255][0], 0x42);
    }

    #[test]
    fn mmio_register_write_does_not_reset_block_transfer() {
        let mut pegc = Pegc::new();
        pegc.state.block_length = 99;
        pegc.state.remain = 42;
        pegc.state.last_data_length = 16;

        pegc.mmio_write_byte(MMIO2_BASE + REG_PLANE_ACCESS, 0xFF);
        assert_eq!(pegc.state.remain, 42);
        assert_eq!(pegc.state.last_data_length, 16);
    }

    #[test]
    fn plane_write_resets_state_when_remain_zero() {
        let mut pegc = Pegc::new();
        pegc.state.mode_register = 1;
        pegc.state.plane_access_mask = 0x00;
        pegc.state.rop_register = 0x0100;
        pegc.state.write_mask = 0xFFFF;
        pegc.state.block_length = 31;
        pegc.state.remain = 0;
        pegc.state.last_data_length = 16;

        let mut vram = vec![0u8; PEGC_VRAM_SIZE];
        pegc.plane_write_word(0, 0xFFFF, &mut vram);

        assert_eq!(pegc.state.last_data_length, 0);
        assert!(
            pegc.state.remain <= 32,
            "remain should have been re-initialized from block_length + 1"
        );
    }

    #[test]
    fn mmio_pattern_write_does_not_reset_block_transfer() {
        let mut pegc = Pegc::new();
        pegc.state.block_length = 99;
        pegc.state.remain = 42;
        pegc.state.last_data_length = 16;

        pegc.mmio_write_byte(MMIO2_BASE + REG_PATTERN, 0xFF);
        assert_eq!(pegc.state.remain, 42);
        assert_eq!(pegc.state.last_data_length, 16);
    }

    #[test]
    fn plane_mode_word_read_compare() {
        let mut pegc = Pegc::new();
        pegc.state.mode_register = 1;
        pegc.state.palette_color_1 = 0x42;
        pegc.state.plane_access_mask = 0x00;
        pegc.state.rop_register = 0;

        let mut vram = vec![0u8; PEGC_VRAM_SIZE];
        let base = 0u32;
        for i in 0..16u32 {
            vram[i as usize] = if i % 2 == 0 { 0x42 } else { 0x00 };
        }

        let result = pegc.plane_read_word(base / 8, &vram);
        for i in 0..16 {
            let bit_set = result & (1 << i) != 0;
            if i % 2 == 0 {
                assert!(!bit_set, "pixel {i} matches palette1, bit should be clear");
            } else {
                assert!(
                    bit_set,
                    "pixel {i} differs from palette1, bit should be set"
                );
            }
        }
    }

    #[test]
    fn plane_mode_word_write_simple_no_rop() {
        let mut pegc = Pegc::new();
        pegc.state.mode_register = 1;
        pegc.state.plane_access_mask = 0x00;
        pegc.state.rop_register = 0x0100;
        pegc.state.write_mask = 0xFFFF;
        pegc.state.block_length = 0x0FFF;

        let mut vram = vec![0u8; PEGC_VRAM_SIZE];
        for byte in vram.iter_mut().take(16) {
            *byte = 0xAA;
        }

        pegc.plane_write_word(0, 0xFF00, &mut vram);

        for i in 0..8 {
            let expected_source: u8 = if 0xFF00u16 & pixel_mask_position(i) as u16 != 0 {
                0xFF
            } else {
                0x00
            };
            let expected = apply_source_copy(expected_source, 0xAA, 0x00);
            assert_eq!(
                vram[i as usize], expected,
                "pixel {i}: expected {expected:#04X}"
            );
        }
    }

    #[test]
    fn plane_mode_write_mask_inhibits() {
        let mut pegc = Pegc::new();
        pegc.state.mode_register = 1;
        pegc.state.plane_access_mask = 0x00;
        pegc.state.rop_register = 0x0100;
        pegc.state.write_mask = 0x0000;
        pegc.state.block_length = 0x0FFF;

        let mut vram = vec![0xBB; PEGC_VRAM_SIZE];

        pegc.plane_write_word(0, 0xFFFF, &mut vram);

        for (i, byte) in vram.iter().enumerate().take(16) {
            assert_eq!(*byte, 0xBB, "pixel {i} should be unchanged (mask = 0)");
        }
    }

    #[test]
    fn plane_mode_rop_source_copy() {
        let mut pegc = Pegc::new();
        pegc.state.mode_register = 1;
        pegc.state.plane_access_mask = 0x00;
        pegc.state.rop_register = 0x10F0 | (1 << 8);
        pegc.state.write_mask = 0xFFFF;
        pegc.state.block_length = 0x0FFF;
        pegc.state.palette_color_1 = 0xFF;

        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.plane_write_word(0, 0xFFFF, &mut vram);

        for (i, byte) in vram.iter().enumerate().take(16) {
            assert_eq!(
                *byte, 0xFF,
                "pixel {i} should be 0xFF after ROP 0xF0 with all-ones source"
            );
        }
    }

    #[test]
    fn plane_mode_block_length_countdown() {
        let mut pegc = Pegc::new();
        pegc.state.mode_register = 1;
        pegc.state.plane_access_mask = 0x00;
        pegc.state.rop_register = 0x0100;
        pegc.state.write_mask = 0xFFFF;
        pegc.state.block_length = 7;

        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.plane_write_word(0, 0xFFFF, &mut vram);

        assert_eq!(pegc.state.remain, 0);

        for (i, byte) in vram.iter().enumerate().take(8) {
            assert_ne!(*byte, 0, "pixel {i} within block should be written");
        }
    }

    #[test]
    fn packed_pixel_word_operations() {
        let mut pegc = Pegc::new();
        let mut vram = vec![0u8; PEGC_VRAM_SIZE];

        pegc.state.bank_a8 = 0;
        pegc.packed_write_word(0, 0, 0xBEEF, &mut vram);
        assert_eq!(pegc.packed_read_word(0, 0, &vram), 0xBEEF);
        assert_eq!(vram[0], 0xEF);
        assert_eq!(vram[1], 0xBE);
    }

    #[test]
    fn apply_rop_copy_source() {
        let result = apply_rop(0xF0, 0xFF, 0x00, 0xFF, 0xFF, 0x00);
        assert_eq!(result, 0xFF);
        let result = apply_rop(0xF0, 0x00, 0xFF, 0xFF, 0xFF, 0x00);
        assert_eq!(result, 0x00);
    }

    #[test]
    fn apply_rop_copy_destination() {
        let result = apply_rop(0xCC, 0x00, 0xFF, 0x00, 0x00, 0x00);
        assert_eq!(result, 0xFF);
    }

    #[test]
    fn apply_rop_with_plane_mask() {
        let result = apply_rop(0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F);
        assert_eq!(result, 0xF0);
    }

    #[test]
    fn apply_source_copy_basic() {
        assert_eq!(apply_source_copy(0xFF, 0x00, 0x00), 0xFF);
        assert_eq!(apply_source_copy(0x00, 0xFF, 0x00), 0x00);
        assert_eq!(apply_source_copy(0xFF, 0xAA, 0xF0), 0xAF);
    }

    #[test]
    fn pixel_mask_position_layout() {
        assert_eq!(pixel_mask_position(0), 0x80);
        assert_eq!(pixel_mask_position(1), 0x40);
        assert_eq!(pixel_mask_position(7), 0x01);
        assert_eq!(pixel_mask_position(8), 0x8000);
        assert_eq!(pixel_mask_position(15), 0x0100);
    }
}
