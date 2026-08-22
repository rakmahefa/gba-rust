use crate::{HEIGHT, WIDTH};

const MODE_MASK: u16 = 0x0007;
const FRAME_SELECT: u16 = 1 << 4;
const BG2_ENABLE: u16 = 1 << 10;
const VISIBLE_WIDTH: usize = 240;
const VISIBLE_HEIGHT: usize = 160;
const MODE5_WIDTH: usize = 160;
const MODE5_HEIGHT: usize = 128;
const MODE5_X_OFFSET: usize = 40;
const MODE5_Y_OFFSET: usize = 16;
const BGR555_MASK: u16 = 0x7fff;

#[derive(Debug, Clone)]
pub struct Ppu {
    pub framebuffer: Vec<u32>,
    pub frame: u64,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            framebuffer: vec![0; WIDTH * HEIGHT],
            frame: 0,
        }
    }
}

impl Ppu {
    pub fn frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Render one visible scanline from the GBA bitmap display modes.
    ///
    /// Modes 3, 4 and 5 are deliberately implemented before tiled modes because
    /// they exercise the real VRAM/palette/display pipeline without introducing a
    /// second abstract tile renderer. Rendering happens at the HBlank boundary,
    /// so the framebuffer observes the same scheduler timeline as scanline IRQs
    /// and HBlank DMA.
    pub fn render_scanline(
        &mut self,
        dispcnt: u16,
        vcount: u16,
        vram: &[u8],
        palette: &[u8],
    ) {
        if vcount >= VISIBLE_HEIGHT as u16 || dispcnt & BG2_ENABLE == 0 {
            return;
        }

        match dispcnt & MODE_MASK {
            3 => self.render_mode3_scanline(vcount as usize, vram),
            4 => self.render_mode4_scanline(vcount as usize, dispcnt & FRAME_SELECT != 0, vram, palette),
            5 => self.render_mode5_scanline(vcount as usize, dispcnt & FRAME_SELECT != 0, vram),
            _ => {}
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
        // Mode 5 displays a centered 160x128 image inside the 240x160 LCD.
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
        vram[2] = 0x00;
        vram[3] = 0x7c;
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
        ppu.render_scanline(BG2_ENABLE | 4 | FRAME_SELECT, 0, &vram, &palette);
        assert_eq!(ppu.framebuffer[0], 0xffff_0000);
    }

    #[test]
    fn mode5_is_centered_and_blank_outside_visible_window() {
        let mut ppu = Ppu::default();
        let mut vram = vec![0; 0x18000];
        let palette = vec![0; 0x400];
        vram[0] = 0x1f;
        ppu.render_scanline(BG2_ENABLE | 5, MODE5_Y_OFFSET as u16, &vram, &palette);
        assert_eq!(ppu.framebuffer[MODE5_Y_OFFSET * VISIBLE_WIDTH + MODE5_X_OFFSET], 0xffff_0000);
        assert_eq!(ppu.framebuffer[0], 0);
    }
}
