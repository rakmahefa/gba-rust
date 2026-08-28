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
        let backdrop = if palette.len() >= 2 {
            u16::from_le_bytes([palette[0], palette[1]])
        } else {
            0
        };
        self.framebuffer[row..row + VISIBLE_WIDTH].fill(bgr555_to_rgba(backdrop));

        let mut priority = [4u8; VISIBLE_WIDTH];
        if dispcnt & BG1_ENABLE != 0 {
            self.render_text_bg_scanline(row, y, self.bg1cnt, self.bg1hofs, self.bg1vofs, vram, palette, &mut priority);
        }
        if dispcnt & BG0_ENABLE != 0 {
            self.render_text_bg_scanline(row, y, self.bg0cnt, self.bg0hofs, self.bg0vofs, vram, palette, &mut priority);
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
        vram: &[u8],
        palette: &[u8],
        priority: &mut [u8; VISIBLE_WIDTH],
    ) {
        let bg_priority = (bgcnt & 0x3) as u8;
        let char_base = (((bgcnt >> 2) & 0x3) as usize) * 0x4000;
        let color_8bpp = bgcnt & (1 << 7) != 0;
        let screen_base = (((bgcnt >> 8) & 0x1f) as usize) * 0x800;
        let size = ((bgcnt >> 14) & 0x3) as usize;
        let (width, height) = match size { 0 => (256, 256), 1 => (512, 256), 2 => (256, 512), _ => (512, 512) };
        let tile_row = ((y + vofs as usize) % height) / 8;
        let fine_y = (y + vofs as usize) & 7;

        for (x, priority_at_x) in priority.iter_mut().enumerate() {
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
                (palette_bank * 16) + nibble as usize
            };

            if palette_index == 0 || palette_index * 2 + 1 >= palette.len() { continue; }
            if bg_priority > *priority_at_x { continue; }
            let offset = palette_index * 2;
            let color = u16::from_le_bytes([palette[offset], palette[offset + 1]]);
            self.framebuffer[row + x] = bgr555_to_rgba(color);
            *priority_at_x = bg_priority;
        }
    }

    pub(super) fn render_mode3_scanline(&mut self, y: usize, vram: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        let base = y * VISIBLE_WIDTH * 2;
        if base + VISIBLE_WIDTH * 2 > vram.len() { return; }
        for x in 0..VISIBLE_WIDTH {
            let offset = base + x * 2;
            let value = u16::from_le_bytes([vram[offset], vram[offset + 1]]);
            self.framebuffer[row + x] = bgr555_to_rgba(value);
        }
    }

    pub(super) fn render_mode4_scanline(&mut self, y: usize, frame1: bool, vram: &[u8], palette: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        let frame_offset = if frame1 { 0xA000 } else { 0 };
        let base = frame_offset + y * VISIBLE_WIDTH;
        if base + VISIBLE_WIDTH > vram.len() || palette.len() < 512 { return; }
        for x in 0..VISIBLE_WIDTH {
            let index = vram[base + x] as usize;
            let palette_offset = index * 2;
            let color = u16::from_le_bytes([palette[palette_offset], palette[palette_offset + 1]]);
            self.framebuffer[row + x] = bgr555_to_rgba(color);
        }
    }

    pub(super) fn render_mode5_scanline(&mut self, y: usize, frame1: bool, vram: &[u8]) {
        let row = y * VISIBLE_WIDTH;
        for pixel in &mut self.framebuffer[row..row + VISIBLE_WIDTH] { *pixel = 0; }
        if !(MODE5_Y_OFFSET..MODE5_Y_OFFSET + MODE5_HEIGHT).contains(&y) { return; }
        let source_y = y - MODE5_Y_OFFSET;
        let frame_offset = if frame1 { 0xA000 } else { 0 };
        let base = frame_offset + source_y * MODE5_WIDTH * 2;
        if base + MODE5_WIDTH * 2 > vram.len() { return; }
        for x in 0..MODE5_WIDTH {
            let offset = base + x * 2;
            let value = u16::from_le_bytes([vram[offset], vram[offset + 1]]) & BGR555_MASK;
            self.framebuffer[row + MODE5_X_OFFSET + x] = bgr555_to_rgba(value);
        }
    }
}
