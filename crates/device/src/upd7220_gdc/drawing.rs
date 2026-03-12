use super::{DrawResult, Gdc, VramOp};

fn rotate_left(v: u16) -> u16 {
    v.rotate_left(1)
}

fn rotate_right(v: u16) -> u16 {
    v.rotate_right(1)
}

/// Sign-extend a 14-bit value to i16.
fn sign_extend_14(val: u16) -> i16 {
    let val = val & 0x3FFF;
    if val & 0x2000 != 0 {
        (val | 0xC000) as i16
    } else {
        val as i16
    }
}

/// Advances EAD by one whole word address in the given direction (for DMA transfers).
/// Unlike `advance_ead()`, this does not rotate the mask — DMA operates on whole words.
pub(crate) fn advance_ead_dma(ead: &mut u32, direction: u8, pitch: u16) {
    const X_DIR: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
    const Y_DIR: [i32; 8] = [1, 1, 0, -1, -1, -1, 0, 1];
    let dir = (direction & 7) as usize;
    let delta = X_DIR[dir] + Y_DIR[dir] * pitch as i32;
    *ead = ((*ead as i32).wrapping_add(delta) as u32) & 0x3FFFF;
}

/// Advances EAD and mask by one pixel in the given direction.
pub(crate) fn advance_ead(ead: &mut u32, mask: &mut u16, direction: u8, pitch: u16) {
    let p = pitch as u32;
    match direction & 7 {
        0 => {
            // Down.
            *ead = ead.wrapping_add(p);
        }
        1 => {
            // Down-right.
            *ead = ead.wrapping_add(p);
            if *mask & 0x8000 != 0 {
                *ead = ead.wrapping_add(1);
            }
            *mask = rotate_left(*mask);
        }
        2 => {
            // Right.
            if *mask & 0x8000 != 0 {
                *ead = ead.wrapping_add(1);
            }
            *mask = rotate_left(*mask);
        }
        3 => {
            // Up-right.
            *ead = ead.wrapping_sub(p);
            if *mask & 0x8000 != 0 {
                *ead = ead.wrapping_add(1);
            }
            *mask = rotate_left(*mask);
        }
        4 => {
            // Up.
            *ead = ead.wrapping_sub(p);
        }
        5 => {
            // Up-left.
            *ead = ead.wrapping_sub(p);
            if *mask & 0x01 != 0 {
                *ead = ead.wrapping_sub(1);
            }
            *mask = rotate_right(*mask);
        }
        6 => {
            // Left.
            if *mask & 0x01 != 0 {
                *ead = ead.wrapping_sub(1);
            }
            *mask = rotate_right(*mask);
        }
        7 => {
            // Down-left.
            *ead = ead.wrapping_add(p);
            if *mask & 0x01 != 0 {
                *ead = ead.wrapping_sub(1);
            }
            *mask = rotate_right(*mask);
        }
        _ => unreachable!(),
    }
    *ead &= 0x3FFFF;
}

impl Gdc {
    fn make_vram_op(&self, data: u16) -> VramOp {
        VramOp {
            address: self.ead & 0x3FFFF,
            data,
            mask: self.mask,
            mode: self.bitmap_mod,
        }
    }

    fn next_pixel(&mut self, direction: u8) {
        let pitch = self.get_effective_pitch();
        let mut ead = self.ead;
        let mut mask = self.mask;
        advance_ead(&mut ead, &mut mask, direction, pitch);
        self.ead = ead;
        self.mask = mask;
    }

    /// Executes the drawing operation defined by current FIGS parameters.
    pub(crate) fn execute_drawing(&mut self) -> DrawResult {
        match self.figure_type {
            0 => self.draw_pixel(),
            1 => self.draw_line(),
            4 => self.draw_arc(),
            8 => self.draw_rectangle(),
            _ => DrawResult {
                writes: Vec::new(),
                dot_count: 0,
            },
        }
    }

    /// Executes character drawing (GCHRD/TEXTE).
    pub(crate) fn execute_gchrd(&mut self) -> DrawResult {
        if self.figure_type & 0x0F == 2 {
            self.draw_character()
        } else {
            DrawResult {
                writes: Vec::new(),
                dot_count: 0,
            }
        }
    }

    /// Draws DC+1 pixels along DIR (figure_type=0).
    fn draw_pixel(&mut self) -> DrawResult {
        let dc = self.drawing_dc;
        let dir = self.drawing_dir;
        let mut writes = Vec::with_capacity((dc + 1) as usize);

        for i in 0..=dc {
            let pattern = self.get_pattern(i & 0xF);
            writes.push(self.make_vram_op(pattern));
            self.next_pixel(dir);
        }

        DrawResult {
            dot_count: (dc + 1) as u32,
            writes,
        }
    }

    /// Bresenham-like line drawing (figure_type=1).
    fn draw_line(&mut self) -> DrawResult {
        let dc = self.drawing_dc;
        let octant = self.drawing_dir;
        let mut d = sign_extend_14(self.drawing_d) as i32;
        let d1 = sign_extend_14(self.drawing_d1) as i32;
        let d2 = sign_extend_14(self.drawing_d2) as i32;
        let mut writes = Vec::with_capacity((dc + 1) as usize);

        for i in 0..=dc {
            let pattern = self.get_pattern(i & 0xF);
            writes.push(self.make_vram_op(pattern));

            let dir = if octant & 1 != 0 {
                // Diagonal-dominant octant.
                if d < 0 { (octant + 1) & 7 } else { octant }
            } else {
                // Axis-dominant octant.
                if d < 0 { octant } else { (octant + 1) & 7 }
            };

            d += if d < 0 { d1 } else { d2 };
            self.next_pixel(dir);
        }

        DrawResult {
            dot_count: (dc + 1) as u32,
            writes,
        }
    }

    /// Arc drawing using midpoint circle algorithm (figure_type=4).
    fn draw_arc(&mut self) -> DrawResult {
        let dc = self.drawing_dc;
        let octant = self.drawing_dir;
        // The real µPD7220 treats DM > DC as "draw all" — some BIOS ROMs
        // (notably the PC-9801RA) send DM=0x3FFF without clearing it for
        // full-circle outlines, relying on this hardware clamping behavior.
        let dm = if self.drawing_dm > dc {
            0
        } else {
            self.drawing_dm
        };
        let mut err = -(self.drawing_d as i32);
        let mut d = self.drawing_d as i32 + 1;
        let mut writes = Vec::new();
        let mut dot_count = 0u32;

        for i in 0..=dc {
            let pattern = self.get_pattern(i % 0xF);

            if i >= dm {
                writes.push(self.make_vram_op(pattern));
                dot_count += 1;
            }

            let dir = if err < 0 {
                if octant & 1 != 0 {
                    (octant + 1) & 7
                } else {
                    octant
                }
            } else if octant & 1 != 0 {
                octant
            } else {
                (octant + 1) & 7
            };

            if err < 0 {
                err += (i as i32 + 1) << 1;
            } else {
                d -= 1;
                err += (i as i32 - d + 1) << 1;
            }

            self.next_pixel(dir);
        }

        DrawResult { writes, dot_count }
    }

    /// Rectangle outline drawing (figure_type=8).
    fn draw_rectangle(&mut self) -> DrawResult {
        let d_width = self.drawing_d;
        let d2_height = self.drawing_d2;
        let mut dir = self.drawing_dir;
        let mut writes = Vec::new();
        let mut dot_count = 0u32;

        for side in 0..=3u16 {
            let dist = if side & 1 != 0 { d2_height } else { d_width };

            for j in 0..dist {
                let pattern = self.get_pattern(j & 0xF);
                writes.push(self.make_vram_op(pattern));
                dot_count += 1;

                if side > 0 && j == 0 {
                    dir = (dir + 2) & 7;
                }
                self.next_pixel(dir);
            }
        }

        DrawResult { writes, dot_count }
    }

    /// Graphics character drawing using RA[8..15] font data.
    fn draw_character(&mut self) -> DrawResult {
        let dc = self.drawing_dc;
        let d_width = self.drawing_d;
        let gchr = self.zoom_gchr as u16;
        let mut dir = self.drawing_dir;
        let mut writes = Vec::new();
        let mut dot_count = 0u32;

        let figure_type = self.figure_type;
        let scan_type = if figure_type & 0x10 != 0 { 1 } else { 0 };

        // Direction change tables.
        let dir_change: [[i8; 4]; 2] = [
            [2, 2, -2, -2], // orthogonal scan
            [1, 3, -3, -1], // diagonal scan
        ];

        let mut di: u16 = 0;

        for i in 0..=dc {
            let ra_byte = self.ra[15 - (i as usize & 7)];
            self.pattern = u16::from(ra_byte) | (u16::from(ra_byte) << 8);

            for _zdc in 0..=gchr {
                for j in 0..d_width {
                    let pat = if !di.is_multiple_of(2) {
                        self.get_pattern(15u16.wrapping_sub(j & 0xF) & 0xF)
                    } else {
                        self.get_pattern(j & 0xF)
                    };

                    for zd in 0..=gchr {
                        writes.push(self.make_vram_op(pat));
                        if pat != 0 {
                            dot_count += 1;
                        }

                        if j != d_width - 1 || zd != gchr {
                            self.next_pixel(dir);
                        }
                    }
                }

                // Change direction for next scan line.
                let idx0 = ((di % 2) << 1) as usize;
                let idx1 = idx0 + 1;
                dir = ((dir as i8 + dir_change[scan_type][idx0]) & 7) as u8;
                self.next_pixel(dir);
                dir = ((dir as i8 + dir_change[scan_type][idx1]) & 7) as u8;
                di += 1;
            }
        }

        DrawResult { writes, dot_count }
    }

    /// WDAT bulk write: writes (DC+1) words to VRAM.
    pub(crate) fn execute_wdat(
        &mut self,
        data: u16,
        transfer_type: u8,
        transfer_mode: u8,
    ) -> DrawResult {
        let dc = self.drawing_dc;
        let effective_mask = match transfer_type {
            0 => self.mask,
            2 => self.mask & 0x00FF,
            3 => self.mask & 0xFF00,
            _ => self.mask,
        };

        let masked_data = data & effective_mask;
        let mut writes = Vec::with_capacity((dc + 1) as usize);

        for _i in 0..=dc {
            writes.push(VramOp {
                address: self.ead & 0x3FFFF,
                data: masked_data,
                mask: effective_mask,
                mode: transfer_mode,
            });

            let dir = self.drawing_dir;
            let pitch = self.get_effective_pitch();
            advance_ead_dma(&mut self.ead, dir, pitch);
        }

        DrawResult {
            dot_count: (dc + 1) as u32,
            writes,
        }
    }
}
