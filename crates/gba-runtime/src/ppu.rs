use crate::{HEIGHT, WIDTH};

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
}
