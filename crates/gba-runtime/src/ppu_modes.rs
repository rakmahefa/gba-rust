use super::*;

impl Ppu {
    pub(super) fn render_mode0_scanline(
        &mut self,
        dispcnt: u16,
        y: usize,
        vram: &[u8],
        palette: &[u8],
    ) {
        let row = y * VISIBLE_WIDTH;
        if dispcnt & BG1_ENABLE != 0 {
            self.render_text_bg_scanline(row, y, self.bg1cnt, self.bg1hofs, self.bg1vofs, LAYER_BG1, vram, palette);
        }
        if dispcnt & BG0_ENABLE != 0 {
            self.render_text_bg_scanline(row, y, self.bg0cnt, self.bg0hofs, self.bg0vofs, LAYER_BG0, vram, palette);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_text_bg_scanline(
        &mut self,
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
                if tile_offset >= vram.len() { continue; }
                vram[tile_offset] as usize
            } else {
                let tile_offset = char_base + tile_index * 32 + py * 4 + (px / 2);
                if tile_offset >= vram.len() { continue; }
                let packed = vram[tile_offset];
                let nibble = if px & 1 == 0 { packed & 0xf } else { packed >> 4 };
                (palette_bank * 16) + nibble as usize
            };

            if palette_index == 0 || palette_index * 2 + 1 >= palette.len() {
                continue;
            }
            let offset = palette_index * 2;
            let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
            self.plot_layer_pixel(row + x, bgr555_to_rgba(color), bg_priority, 128 + layer as u16, layer, false);
        }
    }

    pub(super) fn render_mode3_scanline(&mut self, y: usize, vram: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        let base = y * VISIBLE_WIDTH * 2;
        if base + VISIBLE_WIDTH * 2 > vram.len() { return; }
        let priority = (self.bg2cnt & 0x3) as u8;
        for x in 0..VISIBLE_WIDTH {
            let offset = base + x * 2;
            let value = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            self.plot_layer_pixel(row + x, bgr555_to_rgba(value), priority, 128 + LAYER_BG2 as u16, LAYER_BG2, false);
        }
    }

    pub(super) fn render_mode4_scanline(&mut self, y: usize, frame1: bool, vram: &[u8], palette: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        let frame_offset = if frame1 { 0xa000 } else { 0 };
        let base = frame_offset + y * VISIBLE_WIDTH;
        if base + VISIBLE_WIDTH > vram.len() || palette.len() < 512 { return; }
        let priority = (self.bg2cnt & 0x3) as u8;
        for x in 0..VISIBLE_WIDTH {
            let index = vram[base + x] as usize;
            let palette_offset = index * 2;
            let color = u16::from_le_bytes([palette[palette_offset], palette[palette_offset + 1]]);
            self.plot_layer_pixel(row + x, bgr555_to_rgba(color), priority, 128 + LAYER_BG2 as u16, LAYER_BG2, false);
        }
    }

    pub(super) fn render_mode5_scanline(&mut self, y: usize, frame1: bool, vram: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        if !(MODE5_Y_OFFSET..MODE5_Y_OFFSET + MODE5_HEIGHT).contains(&y) { return; }
        let source_y = y - MODE5_Y_OFFSET;
        let frame_offset = if frame1 { 0xa000 } else { 0 };
        let base = frame_offset + source_y * MODE5_WIDTH * 2;
        if base + MODE5_WIDTH * 2 > vram.len() { return; }
        let priority = (self.bg2cnt & 0x3) as u8;
        for x in 0..MODE5_WIDTH {
            let offset = base + x * 2;
            let value = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            self.plot_layer_pixel(row + MODE5_X_OFFSET + x, bgr555_to_rgba(value), priority, 128 + LAYER_BG2 as u16, LAYER_BG2, false);
        }
    }
}
