use std::collections::HashMap;

use super::*;

const WIN0H: u32 = 0x0400_0040;
const WIN1H: u32 = 0x0400_0042;
const WIN0V: u32 = 0x0400_0044;
const WIN1V: u32 = 0x0400_0046;
const WININ: u32 = 0x0400_0048;
const WINOUT: u32 = 0x0400_004a;
const MOSAIC: u32 = 0x0400_004c;
const BLDCNT: u32 = 0x0400_0050;
const BLDALPHA: u32 = 0x0400_0052;
const BLDY: u32 = 0x0400_0054;
const DISPCNT_WIN0: u16 = 1 << 13;
const DISPCNT_WIN1: u16 = 1 << 14;
const DISPCNT_OBJWIN: u16 = 1 << 15;
const EFFECT_BIT: u8 = 1 << 5;

impl Ppu {
    pub fn apply_scanline_effects(&mut self, dispcnt: u16, y: u16, io: &HashMap<u32, u8>) {
        if y >= VISIBLE_HEIGHT as u16 {
            return;
        }
        self.apply_horizontal_mosaic(io);
        let win0h = io_half(io, WIN0H);
        let win1h = io_half(io, WIN1H);
        let win0v = io_half(io, WIN0V);
        let win1v = io_half(io, WIN1V);
        let winin = io_half(io, WININ);
        let winout = io_half(io, WINOUT);
        let bldcnt = io_half(io, BLDCNT);
        let bldalpha = io_half(io, BLDALPHA);
        let bldy = (io_half(io, BLDY) & 0x1f) as u32;

        for x in 0..VISIBLE_WIDTH {
            let mask = window_mask(
                dispcnt,
                x as u16,
                y,
                win0h,
                win1h,
                win0v,
                win1v,
                winin,
                winout,
                self.line_obj_window[x],
            );
            if !visible_in_mask(self.line_source[x], mask) {
                self.promote_second_candidate(x);
            }
            if mask & EFFECT_BIT != 0 {
                self.apply_color_effect(x, bldcnt, bldalpha, bldy);
            }
        }
    }

    fn promote_second_candidate(&mut self, x: usize) {
        self.framebuffer[x] = self.line_second_color[x];
        self.line_source[x] = self.line_second_source[x];
        self.line_priority[x] = self.line_second_priority[x];
        self.line_tie_rank[x] = self.line_second_tie_rank[x];
        self.line_semi_transparent[x] = false;
    }

    fn apply_color_effect(&mut self, x: usize, bldcnt: u16, bldalpha: u16, bldy: u32) {
        let mode = ((bldcnt >> 6) & 0x3) as u8;
        if mode == 0 {
            return;
        }
        let source = self.line_source[x];
        let first = layer_selected(bldcnt & 0x3f, source)
            || (self.line_semi_transparent[x] && source == LAYER_OBJ);
        if !first {
            return;
        }
        match mode {
            1 if layer_selected((bldcnt >> 8) & 0x3f, self.line_second_source[x]) => {
                let eva = (bldalpha & 0x1f).min(16) as u32;
                let evb = ((bldalpha >> 8) & 0x1f).min(16) as u32;
                self.framebuffer[x] = alpha_blend(self.framebuffer[x], self.line_second_color[x], eva, evb);
            }
            2 => self.framebuffer[x] = brighten(self.framebuffer[x], bldy),
            3 => self.framebuffer[x] = darken(self.framebuffer[x], bldy),
            _ => {}
        }
    }

    fn apply_horizontal_mosaic(&mut self, io: &HashMap<u32, u8>) {
        let mosaic = io_half(io, MOSAIC);
        let bg_size = (mosaic & 0x000f) as usize + 1;
        let obj_size = ((mosaic >> 8) & 0x000f) as usize + 1;
        for x in 0..VISIBLE_WIDTH {
            let size = if self.line_source[x] == LAYER_OBJ { obj_size } else { bg_size };
            let source = x - (x % size);
            if source != x {
                self.framebuffer[x] = self.framebuffer[source];
                self.line_source[x] = self.line_source[source];
                self.line_priority[x] = self.line_priority[source];
                self.line_tie_rank[x] = self.line_tie_rank[source];
                self.line_semi_transparent[x] = self.line_semi_transparent[source];
                self.line_second_color[x] = self.line_second_color[source];
                self.line_second_source[x] = self.line_second_source[source];
                self.line_second_priority[x] = self.line_second_priority[source];
                self.line_second_tie_rank[x] = self.line_second_tie_rank[source];
            }
        }
    }
}

fn window_mask(
    dispcnt: u16,
    x: u16,
    y: u16,
    win0h: u16,
    win1h: u16,
    win0v: u16,
    win1v: u16,
    winin: u16,
    winout: u16,
    obj_window: bool,
) -> u8 {
    if dispcnt & DISPCNT_WIN0 != 0 && inside_window(x, y, win0h, win0v) {
        return (winin & 0x3f) as u8;
    }
    if dispcnt & DISPCNT_WIN1 != 0 && inside_window(x, y, win1h, win1v) {
        return ((winin >> 8) & 0x3f) as u8;
    }
    if dispcnt & DISPCNT_OBJWIN != 0 && obj_window {
        return ((winout >> 8) & 0x3f) as u8;
    }
    (winout & 0x3f) as u8
}

fn inside_window(x: u16, y: u16, h: u16, v: u16) -> bool {
    axis_inside(x, (h >> 8) & 0xff, h & 0xff)
        && axis_inside(y, (v >> 8) & 0xff, v & 0xff)
}

fn axis_inside(value: u16, start: u16, end: u16) -> bool {
    if start < end { value >= start && value < end } else if start > end { value >= start || value < end } else { false }
}

fn visible_in_mask(source: u8, mask: u8) -> bool {
    source == LAYER_BACKDROP || (source <= LAYER_OBJ && mask & (1 << source) != 0)
}

fn layer_selected(mask: u16, source: u8) -> bool {
    source <= LAYER_BACKDROP && mask & (1 << source) != 0
}

fn alpha_blend(foreground: u32, background: u32, eva: u32, evb: u32) -> u32 {
    let blend = |a: u32, b: u32| ((a * eva + b * evb) / 16).min(255);
    0xff00_0000
        | blend((foreground >> 16) & 0xff, (background >> 16) & 0xff) << 16
        | blend((foreground >> 8) & 0xff, (background >> 8) & 0xff) << 8
        | blend(foreground & 0xff, background & 0xff)
}

fn brighten(color: u32, coefficient: u32) -> u32 {
    let coefficient = coefficient.min(16);
    let b = |c: u32| (c + ((255 - c) * coefficient) / 16).min(255);
    0xff00_0000 | b((color >> 16) & 0xff) << 16 | b((color >> 8) & 0xff) << 8 | b(color & 0xff)
}

fn darken(color: u32, coefficient: u32) -> u32 {
    let coefficient = coefficient.min(16);
    let d = |c: u32| c.saturating_sub((c * coefficient) / 16);
    0xff00_0000 | d((color >> 16) & 0xff) << 16 | d((color >> 8) & 0xff) << 8 | d(color & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_window_interval_is_supported() {
        assert!(axis_inside(250, 240, 10));
        assert!(axis_inside(5, 240, 10));
        assert!(!axis_inside(100, 240, 10));
    }

    #[test]
    fn blend_and_brightness_are_clamped() {
        assert_eq!(alpha_blend(0xffff_0000, 0xff00_00ff, 16, 16), 0xff7f_007f);
        assert_eq!(brighten(0xff00_0000, 16), 0xffff_ffff);
        assert_eq!(darken(0xffff_ffff, 16), 0xff00_0000);
    }
}
