use std::collections::HashMap;

use crate::{HEIGHT, WIDTH};

#[path = "ppu_modes.rs"]
mod modes;

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
