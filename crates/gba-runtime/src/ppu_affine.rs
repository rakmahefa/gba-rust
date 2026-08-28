use std::collections::HashMap;

use super::ppu::{bgr555_to_rgba, io_half, Ppu, LAYER_BG0, LAYER_BG1, LAYER_BG2, LAYER_BG3};

const WIDTH: usize = 240;
const HEIGHT: usize = 160;
const BG0_ENABLE: u16 = 1 << 8;
const BG1_ENABLE: u16 = 1 << 9;
const BG2_ENABLE: u16 = 1 << 10;
const BG3_ENABLE: u16 = 1 << 11;
const BG0CNT: u32 = 0x0400_0008;
const BG1CNT: u32 = 0x0400_000a;
const BG2CNT: u32 = 0x0400_000c;
const BG3CNT: u32 = 0x0400_000e;
const BG2PA: u32 = 0x0400_0020;
const BG2PB: u32 = 0x0400_0022;
const BG2PC: u32 = 0x0400_0024;
const BG2PD: u32 = 0x0400_0026;
const BG2X: u32 = 0x0400_0028;
const BG2Y: u32 = 0x0400_002c;
const BG3PA: u32 = 0x0400_0030;
const BG3PB: u32 = 0x0400_0032;
const BG3PC: u32 = 0x0400_0034;
const BG3PD: u32 = 0x0400_0036;
const BG3X: u32 = 0x0400_0038;
const BG3Y: u32 = 0x0400_003c;

impl Ppu {
    pub fn render_affine_mode_scanline(
        &mut self,
        dispcnt: u16,
        vcount: u16,
        vram: &[u8],
        palette: &[u8],
        io: &HashMap<u32, u8>,
    ) {
        if vcount as usize >= HEIGHT {
            return;
        }
        let mode = dispcnt & 0x7;
        if mode != 1 && mode != 2 {
            return;
        }
        if !self.line_active {
            let backdrop = read16_from_slice(palette, 0);
            self.begin_scanline(vcount as usize, bgr555_to_rgba(backdrop));
        }

        let row = vcount as usize * WIDTH;
        if mode == 1 {
            if dispcnt & BG1_ENABLE != 0 {
                render_text_bg(self, row, vcount as usize, read16(io, BG1CNT), read16(io, 0x0400_0014) & 0x03ff, read16(io, 0x0400_0016) & 0x03ff, LAYER_BG1, vram, palette);
            }
            if dispcnt & BG0_ENABLE != 0 {
                render_text_bg(self, row, vcount as usize, read16(io, BG0CNT), read16(io, 0x0400_0010) & 0x03ff, read16(io, 0x0400_0012) & 0x03ff, LAYER_BG0, vram, palette);
            }
            if dispcnt & BG2_ENABLE != 0 {
                render_affine_bg(self, row, vcount as usize, read16(io, BG2CNT), AffineRegs::read(io, BG2PA, BG2PB, BG2PC, BG2PD, BG2X, BG2Y), LAYER_BG2, vram, palette);
            }
        } else {
            if dispcnt & BG3_ENABLE != 0 {
                render_affine_bg(self, row, vcount as usize, read16(io, BG3CNT), AffineRegs::read(io, BG3PA, BG3PB, BG3PC, BG3PD, BG3X, BG3Y), LAYER_BG3, vram, palette);
            }
            if dispcnt & BG2_ENABLE != 0 {
                render_affine_bg(self, row, vcount as usize, read16(io, BG2CNT), AffineRegs::read(io, BG2PA, BG2PB, BG2PC, BG2PD, BG2X, BG2Y), LAYER_BG2, vram, palette);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AffineRegs {
    pa: i32,
    pb: i32,
    pc: i32,
    pd: i32,
    x: i32,
    y: i32,
}

impl AffineRegs {
    fn read(io: &HashMap<u32, u8>, pa: u32, pb: u32, pc: u32, pd: u32, x: u32, y: u32) -> Self {
        Self {
            pa: i16::from_le_bytes(read16(io, pa).to_le_bytes()).into(),
            pb: i16::from_le_bytes(read16(io, pb).to_le_bytes()).into(),
            pc: i16::from_le_bytes(read16(io, pc).to_le_bytes()).into(),
            pd: i16::from_le_bytes(read16(io, pd).to_le_bytes()).into(),
            x: sign_extend_28(read32(io, x)),
            y: sign_extend_28(read32(io, y)),
        }
    }
}

fn render_affine_bg(
    ppu: &mut Ppu,
    row: usize,
    y: usize,
    bgcnt: u16,
    regs: AffineRegs,
    layer: u8,
    vram: &[u8],
    palette: &[u8],
) {
    let bg_priority = (bgcnt & 0x3) as u8;
    let char_base = (((bgcnt >> 2) & 0x3) as usize) * 0x4000;
    let screen_base = (((bgcnt >> 8) & 0x1f) as usize) * 0x800;
    let size_code = ((bgcnt >> 14) & 0x3) as usize;
    let map_size = 128usize << size_code;
    let wrap = bgcnt & (1 << 13) != 0;
    let mut tex_x = regs.x + regs.pb * y as i32;
    let mut tex_y = regs.y + regs.pd * y as i32;

    for x in 0..WIDTH {
        if let Some((sx, sy)) = affine_coordinate(tex_x, tex_y, map_size, wrap) {
            let tile_x = sx / 8;
            let tile_y = sy / 8;
            let map_offset = screen_base + tile_y * (map_size / 8) + tile_x;
            if map_offset < vram.len() {
                let tile_index = vram[map_offset] as usize;
                let tile_offset = char_base + tile_index * 64 + (sy & 7) * 8 + (sx & 7);
                if tile_offset < vram.len() {
                    let palette_index = vram[tile_offset] as usize;
                    if palette_index != 0 && palette_index * 2 + 1 < palette.len() {
                        let offset = palette_index * 2;
                        let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
                        ppu.plot_layer_pixel(row + x, bgr555_to_rgba(color), bg_priority, 128 + layer as u16, layer, false);
                    }
                }
            }
        }
        tex_x += regs.pa;
        tex_y += regs.pc;
    }
}

fn render_text_bg(
    ppu: &mut Ppu,
    row: usize,
    y: usize,
    bgcnt: u16,
    hofs: u16,
    vofs: u16,
    layer: u8,
    vram: &[u8],
    palette: &[u8],
) {
    let bg_priority = (bgcnt & 0x3) as u8;
    let char_base = (((bgcnt >> 2) & 0x3) as usize) * 0x4000;
    let color_8bpp = bgcnt & (1 << 7) != 0;
    let screen_base = (((bgcnt >> 8) & 0x1f) as usize) * 0x800;
    let size = ((bgcnt >> 14) & 0x3) as usize;
    let (width, height) = match size {
        0 => (256, 256),
        1 => (512, 256),
        2 => (256, 512),
        _ => (512, 512),
    };
    let tile_row = ((y + vofs as usize) % height) / 8;
    let fine_y = (y + vofs as usize) & 7;

    for x in 0..WIDTH {
        let sx = (x + hofs as usize) % width;
        let tile_col = sx / 8;
        let fine_x = sx & 7;
        let map_offset = screen_base + tile_row * (width / 8) * 2 + tile_col * 2;
        if map_offset + 1 >= vram.len() { continue; }
        let entry = u16::from_le_bytes([vram[map_offset], vram[map_offset + 1]]);
        let tile_index = (entry & 0x03ff) as usize;
        let hflip = entry & (1 << 10) != 0;
        let vflip = entry & (1 << 11) != 0;
        let palette_bank = ((entry >> 12) & 0xf) as usize;
        let px = if hflip { 7 - fine_x } else { fine_x };
        let py = if vflip { 7 - fine_y } else { fine_y };
        let palette_index = if color_8bpp {
            let tile_offset = char_base + tile_index * 64 + py * 8 + px;
            if tile_offset >= vram.len() { continue; }
            vram[tile_offset] as usize
        } else {
            let tile_offset = char_base + tile_index * 32 + py * 4 + (px / 2);
            if tile_offset >= vram.len() { continue; }
            let packed = vram[tile_offset];
            let nibble = if px & 1 == 0 { packed & 0xf } else { packed >> 4 };
            palette_bank * 16 + nibble as usize
        };
        if palette_index == 0 || palette_index * 2 + 1 >= palette.len() { continue; }
        let offset = palette_index * 2;
        let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
        ppu.plot_layer_pixel(row + x, bgr555_to_rgba(color), bg_priority, 128 + layer as u16, layer, false);
    }
}

fn affine_coordinate(x: i32, y: i32, size: usize, wrap: bool) -> Option<(usize, usize)> {
    let sx = x >> 8;
    let sy = y >> 8;
    if wrap {
        let size = size as i32;
        Some((sx.rem_euclid(size) as usize, sy.rem_euclid(size) as usize))
    } else if sx < 0 || sy < 0 || sx >= size as i32 || sy >= size as i32 {
        None
    } else {
        Some((sx as usize, sy as usize))
    }
}

fn read16(io: &HashMap<u32, u8>, address: u32) -> u16 { io_half(io, address) }
fn read16_from_slice(bytes: &[u8], offset: usize) -> u16 { if offset + 1 < bytes.len() { u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) } else { 0 } }
fn read32(io: &HashMap<u32, u8>, address: u32) -> u32 { u32::from_le_bytes([*io.get(&address).unwrap_or(&0), *io.get(&(address + 1)).unwrap_or(&0), *io.get(&(address + 2)).unwrap_or(&0), *io.get(&(address + 3)).unwrap_or(&0)]) }
fn sign_extend_28(value: u32) -> i32 { let value = value & 0x0fff_ffff; if value & 0x0800_0000 != 0 { (value | 0xf000_0000) as i32 } else { value as i32 } }

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u16(io: &mut HashMap<u32, u8>, address: u32, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        io.insert(address, lo);
        io.insert(address + 1, hi);
    }

    fn put_u32(io: &mut HashMap<u32, u8>, address: u32, value: u32) {
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            io.insert(address + offset as u32, byte);
        }
    }

    #[test]
    fn identity_affine_mapping_renders_tile_zero() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let mut palette = vec![0; 0x400];
        let mut io = HashMap::new();
        put_u16(&mut io, BG2CNT, 0x4000);
        put_u16(&mut io, BG2PA, 0x0100);
        put_u16(&mut io, BG2PB, 0);
        put_u16(&mut io, BG2PC, 0);
        put_u16(&mut io, BG2PD, 0x0100);
        palette[2] = 0x1f;
        vram[0] = 1;
        vram[64] = 1;
        ppu.render_affine_mode_scanline(BG2_ENABLE | 1, 0, &vram, &palette, &io);
        assert_eq!(ppu.framebuffer[0], 0xffff_0000);
    }

    #[test]
    fn signed_reference_point_is_sign_extended_from_28_bits() {
        assert_eq!(sign_extend_28(0x0800_0000), -0x0800_0000);
        assert_eq!(sign_extend_28(0x07ff_ffff), 0x07ff_ffff);
    }

    #[test]
    fn short_palette_read_is_safe() {
        assert_eq!(read16_from_slice(&[], 0), 0);
        assert_eq!(read16_from_slice(&[0x1f], 0), 0);
    }
}
