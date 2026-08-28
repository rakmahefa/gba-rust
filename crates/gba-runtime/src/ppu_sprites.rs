use crate::ppu::Ppu;

const WIDTH: usize = 240;
const HEIGHT: usize = 160;
const OAM_ENTRY_SIZE: usize = 8;
const OBJ_VRAM_BASE: usize = 0x10000;

/// Render the non-affine, non-mosaic OBJ subset defined by OAM.
///
/// This deliberately handles the architectural sprite path first: normal OBJ
/// mode, 4/8bpp tiles, flips, OBJ priority and 1D/2D tile mapping. Affine,
/// mosaic and OBJ-window/blending behavior remain explicit follow-up work in
/// Phase B rather than being approximated here.
impl Ppu {
    pub fn render_sprites(
        &mut self,
        dispcnt: u16,
        vcount: u16,
        vram: &[u8],
        palette: &[u8],
        oam: &[u8],
    ) {
        if vcount >= HEIGHT as u16 || oam.len() < OAM_ENTRY_SIZE {
            return;
        }

        // DISPCNT bit 6 selects OBJ tile mapping: 0 = 2D, 1 = 1D.
        let one_dimensional = dispcnt & (1 << 6) != 0;
        let y = vcount as usize;
        let mut obj_priority = [4u8; WIDTH];

        for index in 0..(oam.len() / OAM_ENTRY_SIZE).min(128) {
            let base = index * OAM_ENTRY_SIZE;
            let attr0 = u16::from_le_bytes([oam[base], oam[base + 1]]);
            let attr1 = u16::from_le_bytes([oam[base + 2], oam[base + 3]]);
            let attr2 = u16::from_le_bytes([oam[base + 4], oam[base + 5]]);

            // Disabled/unsupported affine OBJ modes are skipped rather than
            // silently rendered with incorrect geometry.
            if attr0 & (1 << 8) != 0 || (attr0 >> 10) & 0x3 != 0 {
                continue;
            }

            let shape = (attr0 >> 14) & 0x3;
            let size = (attr1 >> 14) & 0x3;
            let Some((sprite_width, sprite_height)) = sprite_dimensions(shape, size) else {
                continue;
            };

            let obj_y = (attr0 & 0xff) as i32;
            let obj_x = (attr1 & 0x1ff) as i32;
            let y0 = wrap_coordinate(obj_y, 256);
            let x0 = wrap_coordinate(obj_x, 512);
            let screen_y = y as i32;
            let mut local_y = screen_y - y0;
            if local_y < 0 {
                // GBA OBJ Y wraps through 256 lines.
                local_y += 256;
            }
            if local_y < 0 || local_y >= sprite_height as i32 {
                continue;
            }

            let vflip = attr1 & (1 << 13) != 0;
            let hflip = attr1 & (1 << 12) != 0;
            let color_8bpp = attr0 & (1 << 13) != 0;
            let priority = ((attr2 >> 10) & 0x3) as u8;
            let tile_number = (attr2 & 0x03ff) as usize;
            let palette_bank = ((attr2 >> 12) & 0xf) as usize;

            let source_y = if vflip {
                sprite_height - 1 - local_y as usize
            } else {
                local_y as usize
            };
            let tile_row = source_y / 8;
            let fine_y = source_y & 7;
            let tiles_wide = sprite_width / 8;

            for (screen_x, priority_slot) in obj_priority.iter_mut().enumerate().take(WIDTH) {
                let mut local_x = screen_x as i32 - x0;
                if local_x < 0 {
                    local_x += 512;
                }
                if local_x < 0 || local_x >= sprite_width as i32 {
                    continue;
                }
                let local_x = local_x as usize;
                let source_x = if hflip {
                    sprite_width - 1 - local_x
                } else {
                    local_x
                };
                let tile_col = source_x / 8;
                let fine_x = source_x & 7;

                let tile_index = if one_dimensional {
                    tile_number + tile_row * tiles_wide + tile_col
                } else {
                    // 2D OBJ mapping advances each tile row by 32 tiles.
                    tile_number + tile_row * 32 + tile_col
                };
                let bytes_per_tile = if color_8bpp { 64 } else { 32 };
                let tile_offset = OBJ_VRAM_BASE + tile_index * bytes_per_tile;
                let palette_index = if color_8bpp {
                    let offset = tile_offset + fine_y * 8 + fine_x;
                    if offset >= vram.len() {
                        continue;
                    }
                    vram[offset] as usize
                } else {
                    let offset = tile_offset + fine_y * 4 + fine_x / 2;
                    if offset >= vram.len() {
                        continue;
                    }
                    let packed = vram[offset];
                    let nibble = if fine_x & 1 == 0 { packed & 0xf } else { packed >> 4 };
                    if nibble == 0 {
                        continue;
                    }
                    palette_bank * 16 + nibble as usize
                };

                // Index zero is transparent for both OBJ palette formats.
                if palette_index == 0 || palette_index * 2 + 1 >= palette.len() {
                    continue;
                }
                if priority > *priority_slot {
                    continue;
                }

                let offset = palette_index * 2;
                let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
                self.framebuffer[y * WIDTH + screen_x] = bgr555_to_rgba(color);
                *priority_slot = priority;
            }
        }
    }
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

#[inline]
fn bgr555_to_rgba(value: u16) -> u32 {
    let value = value & 0x7fff;
    let r = ((value & 0x1f) as u32) * 255 / 31;
    let g = (((value >> 5) & 0x1f) as u32) * 255 / 31;
    let b = (((value >> 10) & 0x1f) as u32) * 255 / 31;
    0xff00_0000 | (r << 16) | (g << 8) | b
}

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
    fn normal_4bpp_sprite_renders_from_oam() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let mut palette = vec![0; 0x400];
        let mut oam = vec![0; 0x400];

        // OBJ 0: x=0, y=0, 8x8, normal OBJ, 4bpp, tile 0, priority 0.
        oam[4] = 1;
        // One 4bpp tile row is four bytes. Fill the complete first row so
        // every pixel asserted below contains palette index 1 rather than
        // only the first two pixels.
        vram[0x10000..0x10004].fill(0x11);
        palette[2] = 0x1f;
        ppu.render_sprites(1 << 12, 0, &vram, &palette, &oam);

        assert_eq!(ppu.framebuffer[0], 0xffff_0000);
        assert_eq!(ppu.framebuffer[7], 0xffff_0000);
        assert_eq!(ppu.framebuffer[8], 0);
    }

    #[test]
    fn transparent_sprite_pixel_does_not_replace_background() {
        let mut ppu = Ppu::default();
        let vram = vec![0; 0x18000];
        let palette = vec![0; 0x400];
        let mut oam = vec![0; 0x400];
        ppu.framebuffer[0] = 0xff00_ff00;
        oam[4] = 0;
        ppu.render_sprites(0, 0, &vram, &palette, &oam);
        assert_eq!(ppu.framebuffer[0], 0xff00_ff00);
    }
}
