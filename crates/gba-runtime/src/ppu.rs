use std::collections::HashMap;

use crate::{HEIGHT, WIDTH};

#[path = "ppu_modes.rs"]
mod modes;
#[path = "ppu_effects.rs"]
mod effects;

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
const BG0CNT: u32 = 0x0400_0008;
const BG1CNT: u32 = 0x0400_000a;
const BG2CNT: u32 = 0x0400_000c;
const BG3CNT: u32 = 0x0400_000e;
const BG0_HOFS: u32 = 0x0400_0010;
const BG0_VOFS: u32 = 0x0400_0012;
const BG1_HOFS: u32 = 0x0400_0014;
const BG1_VOFS: u32 = 0x0400_0016;

pub(super) const LAYER_BG0: u8 = 0;
pub(super) const LAYER_BG1: u8 = 1;
pub(super) const LAYER_BG2: u8 = 2;
pub(super) const LAYER_BG3: u8 = 3;
pub(super) const LAYER_OBJ: u8 = 4;
pub(super) const LAYER_BACKDROP: u8 = 5;

#[derive(Debug, Clone)]
pub struct Ppu {
    pub framebuffer: Vec<u32>,
    pub frame: u64,
    pub(super) line_active: bool,
    pub(super) line_priority: [u8; VISIBLE_WIDTH],
    pub(super) line_tie_rank: [u16; VISIBLE_WIDTH],
    pub(super) line_source: [u8; VISIBLE_WIDTH],
    pub(super) line_second_priority: [u8; VISIBLE_WIDTH],
    pub(super) line_second_tie_rank: [u16; VISIBLE_WIDTH],
    pub(super) line_second_color: [u32; VISIBLE_WIDTH],
    pub(super) line_second_source: [u8; VISIBLE_WIDTH],
    pub(super) line_obj_window: [bool; VISIBLE_WIDTH],
    pub(super) line_semi_transparent: [bool; VISIBLE_WIDTH],
    bg0cnt: u16,
    bg1cnt: u16,
    bg2cnt: u16,
    bg3cnt: u16,
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
            line_active: false,
            line_priority: [4; VISIBLE_WIDTH],
            line_tie_rank: [255; VISIBLE_WIDTH],
            line_source: [LAYER_BACKDROP; VISIBLE_WIDTH],
            line_second_priority: [5; VISIBLE_WIDTH],
            line_second_tie_rank: [u16::MAX; VISIBLE_WIDTH],
            line_second_color: [0; VISIBLE_WIDTH],
            line_second_source: [LAYER_BACKDROP; VISIBLE_WIDTH],
            line_obj_window: [false; VISIBLE_WIDTH],
            line_semi_transparent: [false; VISIBLE_WIDTH],
            bg0cnt: 0,
            bg1cnt: 0,
            bg2cnt: 0,
            bg3cnt: 0,
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

    pub fn sync_registers(&mut self, io: &HashMap<u32, u8>) {
        self.bg0cnt = io_half(io, BG0CNT);
        self.bg1cnt = io_half(io, BG1CNT);
        self.bg2cnt = io_half(io, BG2CNT);
        self.bg3cnt = io_half(io, BG3CNT);
        self.bg0hofs = io_half(io, BG0_HOFS) & 0x03ff;
        self.bg0vofs = io_half(io, BG0_VOFS) & 0x03ff;
        self.bg1hofs = io_half(io, BG1_HOFS) & 0x03ff;
        self.bg1vofs = io_half(io, BG1_VOFS) & 0x03ff;
    }

    pub(super) fn begin_scanline(&mut self, y: usize, backdrop: u32) {
        let row = y * VISIBLE_WIDTH;
        self.framebuffer[row..row + VISIBLE_WIDTH].fill(backdrop);
        self.line_priority.fill(4);
        self.line_tie_rank.fill(255);
        self.line_source.fill(LAYER_BACKDROP);
        self.line_second_priority.fill(5);
        self.line_second_tie_rank.fill(u16::MAX);
        self.line_second_color.fill(backdrop);
        self.line_second_source.fill(LAYER_BACKDROP);
        self.line_obj_window.fill(false);
        self.line_semi_transparent.fill(false);
        self.line_active = true;
    }

    pub(super) fn plot_layer_pixel(
        &mut self,
        pixel: usize,
        color: u32,
        priority: u8,
        tie_rank: u16,
        source: u8,
        semi_transparent: bool,
    ) {
        if pixel >= self.framebuffer.len() {
            return;
        }
        let x = pixel % VISIBLE_WIDTH;
        let incoming = (priority, tie_rank);
        let top = (self.line_priority[x], self.line_tie_rank[x]);
        let second = (self.line_second_priority[x], self.line_second_tie_rank[x]);

        if incoming < top {
            self.line_second_priority[x] = self.line_priority[x];
            self.line_second_tie_rank[x] = self.line_tie_rank[x];
            self.line_second_color[x] = self.framebuffer[pixel];
            self.line_second_source[x] = self.line_source[x];
            self.framebuffer[pixel] = color;
            self.line_priority[x] = priority;
            self.line_tie_rank[x] = tie_rank;
            self.line_source[x] = source;
            self.line_semi_transparent[x] = semi_transparent;
        } else if incoming > top && incoming < second {
            self.line_second_priority[x] = priority;
            self.line_second_tie_rank[x] = tie_rank;
            self.line_second_color[x] = color;
            self.line_second_source[x] = source;
        }
    }

    pub(super) fn set_obj_window_pixel(&mut self, x: usize) {
        if x < VISIBLE_WIDTH {
            self.line_obj_window[x] = true;
        }
    }

    pub(super) fn apply_scanline_effects(&mut self, dispcnt: u16, y: u16, io: &HashMap<u32, u8>) {
        self.apply_scanline_effects_impl(dispcnt, y, io);
    }

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
        let y = vcount as usize;
        let backdrop = if palette.len() >= 2 {
            bgr555_to_rgba(u16::from_le_bytes([palette[0], palette[1]]))
        } else {
            bgr555_to_rgba(0)
        };
        self.begin_scanline(y, backdrop);

        match dispcnt & MODE_MASK {
            0 => self.render_mode0_scanline(dispcnt, y, vram, palette),
            1 | 2 => {}
            3 if dispcnt & BG2_ENABLE != 0 => self.render_mode3_scanline(y, vram),
            4 if dispcnt & BG2_ENABLE != 0 => self.render_mode4_scanline(
                y,
                dispcnt & FRAME_SELECT != 0,
                vram,
                palette,
            ),
            5 if dispcnt & BG2_ENABLE != 0 => {
                self.render_mode5_scanline(y, dispcnt & FRAME_SELECT != 0, vram)
            }
            _ => {}
        }
    }
}

#[inline]
pub(super) fn bgr555_to_rgba(value: u16) -> u32 {
    let value = value & BGR555_MASK;
    let r = ((value & 0x1f) as u32) * 255 / 31;
    let g = (((value >> 5) & 0x1f) as u32) * 255 / 31;
    let b = (((value >> 10) & 0x1f) as u32) * 255 / 31;
    0xff00_0000 | (r << 16) | (g << 8) | b
}

#[inline]
pub(super) fn io_half(io: &HashMap<u32, u8>, address: u32) -> u16 {
    u16::from_le_bytes([
        *io.get(&address).unwrap_or(&0),
        *io.get(&(address + 1)).unwrap_or(&0),
    ])
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
        io.insert(BG0CNT, 0);
        io.insert(BG0CNT + 1, 0);
        io.insert(BG0_HOFS, 1);
        io.insert(BG0_HOFS + 1, 0);
        ppu.sync_registers(&io);
        vram[0] = 0x01;
        palette[2] = 0x1f;
        ppu.render_scanline(BG0_ENABLE, 0, &vram, &palette);
        assert_eq!(ppu.framebuffer[7], 0xffff_0000);
    }

    #[test]
    fn compositor_orders_obj_above_bg_at_equal_priority() {
        let mut ppu = Ppu::default();
        ppu.begin_scanline(0, 0);
        ppu.plot_layer_pixel(0, 1, 1, 129, LAYER_BG1, false);
        ppu.plot_layer_pixel(0, 2, 1, 128, LAYER_BG0, false);
        ppu.plot_layer_pixel(0, 3, 1, 7, LAYER_OBJ, false);
        assert_eq!(ppu.framebuffer[0], 3);
        assert_eq!(ppu.line_second_source[0], LAYER_BG0);
    }
}
