use std::collections::HashMap;

use crate::{HEIGHT, WIDTH};

const MODE_MASK: u16 = 0x0007;
const FRAME_SELECT: u16 = 1 << 4;
const BG0_ENABLE: u16 = 1 << 8;
const BG1_ENABLE: u16 = 1 << 9;
const BG2_ENABLE: u16 = 1 << 10;
const VISIBLE_WIDTH: usize = 240;
const VISIBLE_HEIGHT: usize = 160;
const MODE5_WIDTH: usize = 160;
const MODE5_HEIGHT: usize = 128;
const MODE5_X_OFFSET: usize = 40;
const MODE5_Y_OFFSET: usize = 16;
const BGR555_MASK: u16 = 0x7fff;
const BG_CNT_BASE: u32 = 0x0400_0008;
const BG0_HOFS: u32 = 0x0400_0010;
const BG0_VOFS: u32 = 0x0400_0012;
const BG1_HOFS: u32 = 0x0400_0014;
const BG1_VOFS: u32 = 0x0400_0016;
const BG1_CNT: u32 = 0x0400_000a;

#[derive(Debug, Clone)]
pub struct Ppu {
    pub framebuffer: Vec<u32>,
    pub frame: u64,
    bg0cnt: u16,
    bg1cnt: u16,
    bg0hofs: u16,
    bg0vofs: u16,
    bg1hofs: u16,
    bg1vofs: u16,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            framebuffer: vec![0; WIDTH * HEIGHT],
            frame: 0,
            bg0cnt: 0,
            bg1cnt: 0,
            bg0hofs: 0,
            bg0vofs: 0,
            bg1hofs: 0,
            bg1vofs: 0,
        }
    }
}

impl Ppu {
    pub fn frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Synchronize the register-backed subset used by the renderer.
    pub fn sync_registers(&mut self, io: &HashMap<u32, u8>) {
        self.bg0cnt = io_half(io, BG_CNT_BASE);
        self.bg1cnt = io_half(io, BG1_CNT);
        self.bg0hofs = io_half(io, BG0_HOFS) & 0x03ff;
        self.bg0vofs = io_half(io, BG0_VOFS) & 0x03ff;
        self.bg1hofs = io_half(io, BG1_HOFS) & 0x03ff;
        self.bg1vofs = io_half(io, BG1_VOFS) & 0x03ff;
    }

    /// Render one visible scanline from the GBA bitmap and tiled display modes.
    /// Rendering happens at the HBlank boundary so the framebuffer observes the
    /// same scheduler timeline as scanline IRQs and HBlank DMA.
    pub fn render_scanline(
        &mut self,
        dispcnt: u16,
        vcount: u16,
        vram: &[u8],
        palette: &[u8],
    ) {
        if vcount >= VISIBLE_HEIGHT as u16 {
            return;
        }

        match dispcnt & MODE_MASK {
            0 => self.render_mode0_scanline(dispcnt, vcount as usize, vram, palette),
            3 if dispcnt & BG2_ENABLE != 0 => {
                self.render_mode3_scanline(vcount as usize, vram)
            }
            4 if dispcnt & BG2_ENABLE != 0 => self.render_mode4_scanline(
                vcount as usize,
                dispcnt & FRAME_SELECT != 0,
                vram,
                palette,
            ),
            5 if dispcnt & BG2_ENABLE != 0 => {
                self.render_mode5_scanline(vcount as usize, dispcnt & FRAME_SELECT != 0, vram)
            }
            _ => {
                if dispcnt & BG2_ENABLE == 0 && dispcnt & (BG0_ENABLE | BG1_ENABLE) == 0 {
                    let row = vcount as usize * VISIBLE_WIDTH;
                    for pixel in &mut self.framebuffer[row..row + VISIBLE_WIDTH] {
                        *pixel = bgr555_to_rgba(0);
                    }
                }
            }
        }
    }

    fn render_mode0_scanline(
        &mut self,
        dispcnt: u16,
        y: usize,
        vram: &[u8],
        palette: &[u8],
    ) {
        let row = y * VISIBLE_WIDTH;
        let backdrop = if palette.len() >= 2 {
            u16::from_le_bytes([palette[0], palette[1]])
        } else {
            0
        };
        self.framebuffer[row..row + VISIBLE_WIDTH].fill(bgr555_to_rgba(backdrop));

        let mut priority = [4u8; VISIBLE_WIDTH];
        if dispcnt & BG1_ENABLE != 0 {
            self.render_text_bg_scanline(
                row,
                y,
                self.bg1cnt,
                self.bg1hofs,
                self.bg1vofs,
                vram,
                palette,
                &mut priority,
            );
        }
        if dispcnt & BG0_ENABLE != 0 {
            self.render_text_bg_scanline(
                row,
                y,
                self.bg0cnt,
                self.bg0hofs,
                self.bg0vofs,
                vram,
                palette,
                &mut priority,
            );
        }
    }

    fn render_text_bg_scanline(
        &mut self,
        row: usize,
        y: usize,
        bgcnt: u16,
        hofs: u16,
        vofs: u16,
        vram: &[u8],
        palette: &[u8],
        priority: &mut [u8; VISIBLE_WIDTH],
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

        for x in 0..VISIBLE_WIDTH {
            let sx = (x + hofs as usize) % width;
            let tile_col = sx / 8;
            let fine_x = sx & 7;
            let map_offset = screen_base + tile_row * (width / 8) * 2 + tile_col * 2;
            if map_offset + 1 >= vram.len() {
                continue;
            }
            let entry = u16::from_le_bytes([vram[map_offset], vram[map_offset + 1]]);
            let tile_index = (entry & 0x03ff) as usize;
            let hflip = entry & (1 << 10) != 0;
            let vflip = entry & (1 << 11) != 0;
            let palette_bank = ((entry >> 12) & 0xf) as usize;
            let px = if hflip { 7 - fine_x } else { fine_x };
            let py = if vflip { 7 - fine_y } else { fine_y };

            let palette_index = if color_8bpp {
                let tile_offset = char_base + tile_index * 64 + py * 8 + px;
                if tile_offset >= vram.len() {
                    continue;
                }
                vram[tile_offset] as usize
            } else {
                let tile_offset = char_base + tile_index * 32 + py * 4 + (px / 2);
                if tile_offset >= vram.len() {
                    continue;
                }
                let packed = vram[tile_offset];
                let nibble = if px & 1 == 0 { packed & 0xf } else { packed >> 4 };
                (palette_bank * 16) + nibble as usize
            };

            if palette_index == 0 || palette_index * 2 + 1 >= palette.len() {
                continue;
            }
            if bg_priority > priority[x] {
                continue;
            }
            let offset = palette_index * 2;
            let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
            self.framebuffer[row + x] = bgr555_to_rgba(color);
            priority[x] = bg_priority;
        }
    }

    fn render_mode3_scanline(&mut self, y: usize, vram: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        let base = y * VISIBLE_WIDTH * 2;
        if base + VISIBLE_WIDTH * 2 > vram.len() {
            return;
        }
        for x in 0..VISIBLE_WIDTH {
            let offset = base + x * 2;
            let value = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            self.framebuffer[row + x] = bgr555_to_rgba(value);
        }
    }

    fn render_mode4_scanline(&mut self, y: usize, frame1: bool, vram: &[u8], palette: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        let frame_offset = if frame1 { 0xA000 } else { 0 };
        let base = frame_offset + y * VISIBLE_WIDTH;
        if base + VISIBLE_WIDTH > vram.len() || palette.len() < 512 {
            return;
        }
        for x in 0..VISIBLE_WIDTH {
            let index = vram[base + x] as usize;
            let palette_offset = index * 2;
            let color = u16::from_le_bytes([palette[palette_offset], palette[palette_offset + 1]]);
            self.framebuffer[row + x] = bgr555_to_rgba(color);
        }
    }

    fn render_mode5_scanline(&mut self, y: usize, frame1: bool, vram: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        for pixel in &mut self.framebuffer[row..row + VISIBLE_WIDTH] {
            *pixel = 0;
        }
        if !(MODE5_Y_OFFSET..MODE5_Y_OFFSET + MODE5_HEIGHT).contains(&y) {
            return;
        }

        let source_y = y - MODE5_Y_OFFSET;
        let frame_offset = if frame1 { 0xA000 } else { 0 };
        let base = frame_offset + source_y * MODE5_WIDTH * 2;
        if base + MODE5_WIDTH * 2 > vram.len() {
            return;
        }

        for x in 0..MODE5_WIDTH {
            let offset = base + x * 2;
            let value = u16::from_le_bytes([vram[offset], vram[offset + 1]]) & BGR555_MASK;
            self.framebuffer[row + MODE5_X_OFFSET + x] = bgr555_to_rgba(value);
        }
    }
}

#[inline]
fn io_half(io: &HashMap<u32, u8>, address: u32) -> u16 {
    u16::from_le_bytes([
        *io.get(&address).unwrap_or(&0),
        *io.get(&(address + 1)).unwrap_or(&0),
    ])
}

#[inline]
fn bgr555_to_rgba(value: u16) -> u32 {
    let value = value & BGR555_MASK;
    let r = ((value & 0x1f) as u32) * 255 / 31;
    let g = (((value >> 5) & 0x1f) as u32) * 255 / 31;
    let b = (((value >> 10) & 0x1f) as u32) * 255 / 31;
    0xff00_0000 | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode3_renders_bgr555_pixels_at_hardware_coordinates() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let palette = vec![0; 0x400];
        vram[2] = 0x1f;
        vram[3] = 0x00;
        ppu.render_scanline(BG2_ENABLE | 3, 0, &vram, &palette);
        assert_eq!(ppu.framebuffer[1], 0xffff_0000);
    }

    #[test]
    fn mode4_uses_selected_palette_entry() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let mut palette = vec![0; 0x400];
        vram[0] = 3;
        palette[6] = 0xe0;
        palette[7] = 0x03;
        ppu.render_scanline(BG2_ENABLE | 4, 0, &vram, &palette);
        assert_eq!(ppu.framebuffer[0], 0xff00_ff00);
    }

    #[test]
    fn mode4_frame_select_uses_second_bitmap_page() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let mut palette = vec![0; 0x400];
        vram[0] = 1;
        vram[0xa000] = 2;
        palette[2] = 0x1f;
        palette[4] = 0xe0;
        palette[5] = 0x03;
        ppu.render_scanline(BG2_ENABLE | 4 | FRAME_SELECT, 0, &vram, &palette);
        assert_eq!(ppu.framebuffer[0], 0xff00_ff00);
    }

    #[test]
    fn mode5_is_centered_and_blank_outside_visible_window() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let palette = vec![0; 0x400];
        vram[0] = 0x1f;
        ppu.render_scanline(BG2_ENABLE | 5, MODE5_Y_OFFSET as u16, &vram, &palette);
        assert_eq!(
            ppu.framebuffer[MODE5_Y_OFFSET * VISIBLE_WIDTH + MODE5_X_OFFSET],
            0xffff_0000
        );
        assert_eq!(ppu.framebuffer[0], 0);
    }

    #[test]
    fn mode0_renders_a_4bpp_text_tile_with_scroll() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let mut palette = vec![0; 0x400];
        let mut io = HashMap::new();
        io.insert(BG_CNT_BASE, 0);
        io.insert(BG_CNT_BASE + 1, 0);
        io.insert(BG0_HOFS, 1);
        io.insert(BG0_HOFS + 1, 0);
        ppu.sync_registers(&io);
        vram[0] = 0x01;
        palette[2] = 0x1f;
        ppu.render_scanline(BG0_ENABLE, 0, &vram, &palette);
        assert_eq!(ppu.framebuffer[7], 0xffff_0000);
    }
}
