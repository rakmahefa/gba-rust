use crate::ppu::Ppu;

const WIDTH: usize = 240;
const HEIGHT: usize = 160;
const OAM_ENTRY_SIZE: usize = 8;
const OAM_ENTRIES: usize = 128;
const OBJ_VRAM_BASE: usize = 0x10000;

impl Ppu {
    /// Render normal and affine OBJ entries for the current scanline.
    pub fn render_sprites(
        &mut self,
        dispcnt: u16,
        vcount: u16,
        vram: &[u8],
        palette: &[u8],
        oam: &[u8],
    ) {
        if vcount >= HEIGHT as u16 || oam.len() < OAM_ENTRY_SIZE || dispcnt & (1 << 12) == 0 {
            return;
        }
        let one_dimensional = dispcnt & (1 << 6) != 0;
        let y = vcount as i32;
        let count = (oam.len() / OAM_ENTRY_SIZE).min(OAM_ENTRIES);

        for index in 0..count {
            let base = index * OAM_ENTRY_SIZE;
            let attr0 = u16::from_le_bytes([oam[base], oam[base + 1]]);
            let attr1 = u16::from_le_bytes([oam[base + 2], oam[base + 3]]);
            let attr2 = u16::from_le_bytes([oam[base + 4], oam[base + 5]]);
            let affine = attr0 & (1 << 8) != 0;
            let double_size = affine && attr0 & (1 << 9) != 0;
            let obj_mode = (attr0 >> 10) & 0x3;
            if obj_mode == 3 {
                continue;
            }
            let shape = (attr0 >> 14) & 0x3;
            let size = (attr1 >> 14) & 0x3;
            let Some((sprite_width, sprite_height)) = sprite_dimensions(shape, size) else {
                continue;
            };
            let x0 = wrap_coordinate((attr1 & 0x01ff) as i32, 512);
            let y0 = wrap_coordinate((attr0 & 0x00ff) as i32, 256);
            let mut local_y = y - y0;
            if local_y < 0 {
                local_y += 256;
            }
            let draw_width = if double_size { sprite_width * 2 } else { sprite_width };
            let draw_height = if double_size { sprite_height * 2 } else { sprite_height };
            if local_y < 0 || local_y >= draw_height as i32 {
                continue;
            }
            let color_8bpp = attr0 & (1 << 13) != 0;
            let priority = ((attr2 >> 10) & 0x3) as u8;
            let tile_number = (attr2 & 0x03ff) as usize;
            let palette_bank = ((attr2 >> 12) & 0xf) as usize;
            let normal_source_y = if !affine {
                if attr1 & (1 << 13) != 0 { sprite_height - 1 - local_y as usize } else { local_y as usize }
            } else {
                0
            };

            for screen_x in 0..WIDTH {
                let mut local_x = screen_x as i32 - x0;
                if local_x < 0 {
                    local_x += 512;
                }
                if local_x < 0 || local_x >= draw_width as i32 {
                    continue;
                }

                let (source_x, source_y) = if affine {
                    let matrix_index = ((attr1 >> 9) & 0x1f) as usize;
                    let (pa, pb, pc, pd) = read_affine_matrix(oam, matrix_index);
                    let dx = local_x - draw_width as i32 / 2;
                    let dy = local_y - draw_height as i32 / 2;
                    let sx = sprite_width as i32 / 2 + ((pa * dx + pb * dy) >> 8);
                    let sy = sprite_height as i32 / 2 + ((pc * dx + pd * dy) >> 8);
                    if sx < 0 || sy < 0 || sx >= sprite_width as i32 || sy >= sprite_height as i32 {
                        continue;
                    }
                    (sx as usize, sy as usize)
                } else {
                    let lx = local_x as usize;
                    let source_x = if attr1 & (1 << 12) != 0 { sprite_width - 1 - lx } else { lx };
                    (source_x, normal_source_y)
                };

                let tile_row = source_y / 8;
                let tile_col = source_x / 8;
                let fine_x = source_x & 7;
                let fine_y = source_y & 7;
                let tiles_wide = sprite_width / 8;
                let tile_index = if one_dimensional {
                    tile_number + tile_row * tiles_wide + tile_col
                } else {
                    tile_number + tile_row * 32 + tile_col
                };
                let bytes_per_tile = if color_8bpp { 64 } else { 32 };
                let tile_offset = OBJ_VRAM_BASE + tile_index * bytes_per_tile;
                let palette_index = if color_8bpp {
                    let offset = tile_offset + fine_y * 8 + fine_x;
                    if offset >= vram.len() { continue; }
                    vram[offset] as usize
                } else {
                    let offset = tile_offset + fine_y * 4 + fine_x / 2;
                    if offset >= vram.len() { continue; }
                    let packed = vram[offset];
                    let nibble = if fine_x & 1 == 0 { packed & 0xf } else { packed >> 4 };
                    if nibble == 0 { continue; }
                    palette_bank * 16 + nibble as usize
                };
                if palette_index == 0 || palette_index * 2 + 1 >= palette.len() {
                    continue;
                }

                if obj_mode == 2 {
                    self.set_obj_window_pixel(screen_x);
                    continue;
                }
                let offset = palette_index * 2;
                let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
                self.plot_layer_pixel(
                    vcount as usize * WIDTH + screen_x,
                    super::ppu::bgr555_to_rgba(color),
                    priority,
                    index as u16,
                    LAYER_OBJ,
                    obj_mode == 1,
                );
            }
        }
    }
}

fn read_affine_matrix(oam: &[u8], matrix: usize) -> (i32, i32, i32, i32) {
    if matrix >= 32 || oam.len() < 0x400 {
        return (0x100, 0, 0, 0x100);
    }
    let base = matrix * 32 + 6;
    let read = |offset: usize| -> i32 {
        i16::from_le_bytes([oam[offset], oam[offset + 1]]) as i32
    };
    (read(base), read(base + 8), read(base + 16), read(base + 24))
}

fn sprite_dimensions(shape: u16, size: u16) -> Option<(usize, usize)> {
    match (shape, size) {
        (0, 0) => Some((8, 8)),
        (0, 1) => Some((16, 16)),
        (0, 2) => Some((32, 32)),
        (0, 3) => Some((64, 64)),
        (1, 0) => Some((16, 8)),
        (1, 1) => Some((32, 8)),
        (1, 2) => Some((32, 16)),
        (1, 3) => Some((64, 32)),
        (2, 0) => Some((8, 16)),
        (2, 1) => Some((8, 32)),
        (2, 2) => Some((16, 32)),
        (2, 3) => Some((32, 64)),
        _ => None,
    }
}

fn wrap_coordinate(value: i32, modulo: i32) -> i32 {
    let value = value % modulo;
    if value < 0 { value + modulo } else { value }
}

const LAYER_OBJ: u8 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_dimensions_match_gba_shape_tables() {
        assert_eq!(sprite_dimensions(0, 0), Some((8, 8)));
        assert_eq!(sprite_dimensions(1, 2), Some((32, 16)));
        assert_eq!(sprite_dimensions(2, 3), Some((32, 64)));
        assert_eq!(sprite_dimensions(3, 0), None);
    }

    #[test]
    fn affine_matrix_uses_interleaved_oam_gaps() {
        let mut oam = vec![0; 0x400];
        let base = 3 * 32 + 6;
        for (offset, value) in [(0, 0x0100i16), (8, 0i16), (16, 0i16), (24, 0x0100i16)] {
            let bytes = value.to_le_bytes();
            oam[base + offset] = bytes[0];
            oam[base + offset + 1] = bytes[1];
        }
        assert_eq!(read_affine_matrix(&oam, 3), (0x100, 0, 0, 0x100));
    }

    #[test]
    fn transparent_obj_does_not_replace_existing_pixel() {
        let mut ppu = Ppu::default();
        let vram = vec![0; 0x18000];
        let palette = vec![0; 0x400];
        let oam = vec![0; 0x400];
        ppu.framebuffer[0] = 0xff00_ff00;
        ppu.render_sprites(1 << 12, 0, &vram, &palette, &oam);
        assert_eq!(ppu.framebuffer[0], 0xff00_ff00);
    }
}
