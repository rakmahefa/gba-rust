use crate::{Cartridge, FRAME_CYCLES, HEIGHT, WIDTH};

const EWRAM_SIZE: usize = 0x40000;
const IWRAM_SIZE: usize = 0x8000;
const IO_SIZE: usize = 0x400;
const PAL_SIZE: usize = 0x400;
const VRAM_SIZE: usize = 0x18000;
const OAM_SIZE: usize = 0x400;

pub struct Bus {
    pub ewram: Box<[u8; EWRAM_SIZE]>,
    pub iwram: Box<[u8; IWRAM_SIZE]>,
    pub io: Box<[u8; IO_SIZE]>,
    pub palette: Box<[u8; PAL_SIZE]>,
    pub vram: Box<[u8; VRAM_SIZE]>,
    pub oam: Box<[u8; OAM_SIZE]>,
    pub cart: Cartridge,
    pub framebuffer: Box<[u16; WIDTH * HEIGHT]>,
    pub cycles: u64,
    pub frame: u64,
}

impl Bus {
    pub fn new(cart: Cartridge) -> Self {
        Self { ewram: Box::new([0; EWRAM_SIZE]), iwram: Box::new([0; IWRAM_SIZE]), io: Box::new([0; IO_SIZE]), palette: Box::new([0; PAL_SIZE]), vram: Box::new([0; VRAM_SIZE]), oam: Box::new([0; OAM_SIZE]), cart, framebuffer: Box::new([0; WIDTH * HEIGHT]), cycles: 0, frame: 0 }
    }

    #[inline(always)] pub fn read8(&self, addr: u32) -> u8 { match addr {
        0x0200_0000..=0x02FF_FFFF => self.ewram[((addr - 0x0200_0000) as usize) & (EWRAM_SIZE-1)],
        0x0300_0000..=0x03FF_FFFF => self.iwram[((addr - 0x0300_0000) as usize) & (IWRAM_SIZE-1)],
        0x0400_0000..=0x0400_03FF => self.io[(addr - 0x0400_0000) as usize],
        0x0500_0000..=0x05FF_FFFF => self.palette[((addr - 0x0500_0000) as usize) & (PAL_SIZE-1)],
        0x0600_0000..=0x06FF_FFFF => self.vram[((addr - 0x0600_0000) as usize) % VRAM_SIZE],
        0x0700_0000..=0x07FF_FFFF => self.oam[((addr - 0x0700_0000) as usize) & (OAM_SIZE-1)],
        0x0800_0000..=0x0DFF_FFFF => self.cart.rom[((addr - 0x0800_0000) as usize) % self.cart.rom.len()],
        0x0E00_0000..=0x0FFF_FFFF => { let d=self.cart.save.bytes(); if d.is_empty(){0xff}else{d[((addr-0x0E00_0000) as usize)%d.len()]} },
        _ => 0,
    }}
    #[inline(always)] pub fn read16(&self, a:u32)->u16 { u16::from_le_bytes([self.read8(a),self.read8(a+1)]) }
    #[inline(always)] pub fn read32(&self, a:u32)->u32 { u32::from_le_bytes([self.read8(a),self.read8(a+1),self.read8(a+2),self.read8(a+3)]) }
    #[inline(always)] pub fn write8(&mut self, addr:u32, v:u8) { match addr {
        0x0200_0000..=0x02FF_FFFF => self.ewram[((addr - 0x0200_0000) as usize) & (EWRAM_SIZE-1)] = v,
        0x0300_0000..=0x03FF_FFFF => self.iwram[((addr - 0x0300_0000) as usize) & (IWRAM_SIZE-1)] = v,
        0x0400_0000..=0x0400_03FF => self.io[(addr - 0x0400_0000) as usize] = v,
        0x0500_0000..=0x05FF_FFFF => self.palette[((addr - 0x0500_0000) as usize) & (PAL_SIZE-1)] = v,
        0x0600_0000..=0x06FF_FFFF => self.vram[((addr - 0x0600_0000) as usize) % VRAM_SIZE] = v,
        0x0700_0000..=0x07FF_FFFF => self.oam[((addr - 0x0700_0000) as usize) & (OAM_SIZE-1)] = v,
        0x0E00_0000..=0x0FFF_FFFF => { let d=self.cart.save.bytes_mut(); if !d.is_empty(){d[((addr-0x0E00_0000) as usize)%d.len()]=v;} },
        _ => {}
    }}
    #[inline(always)] pub fn write16(&mut self,a:u32,v:u16){let b=v.to_le_bytes();self.write8(a,b[0]);self.write8(a+1,b[1]);}
    #[inline(always)] pub fn write32(&mut self,a:u32,v:u32){let b=v.to_le_bytes();self.write8(a,b[0]);self.write8(a+1,b[1]);self.write8(a+2,b[2]);self.write8(a+3,b[3]);}

    pub fn tick(&mut self, cycles: u64) {
        self.cycles += cycles;
        while self.cycles >= FRAME_CYCLES { self.cycles -= FRAME_CYCLES; self.frame += 1; self.render(); }
    }

    fn render(&mut self) {
        let dispcnt = self.read16(0x0400_0000);
        match dispcnt & 7 {
            3 => self.render_mode3(),
            4 => self.render_mode4((dispcnt & (1<<4)) != 0),
            _ => self.render_mode3_fallback(),
        }
    }
    fn render_mode3(&mut self) { for y in 0..HEIGHT { let base=y*WIDTH*2; for x in 0..WIDTH { let p=u16::from_le_bytes([self.vram[base+x*2],self.vram[base+x*2+1]]); self.framebuffer[y*WIDTH+x]=p; } } }
    fn render_mode4(&mut self, page: bool) { let base=if page{0xA000}else{0}; for y in 0..HEIGHT { for x in 0..WIDTH { let i=base+y*WIDTH+x; let idx=self.vram[i]; self.framebuffer[y*WIDTH+x]=u16::from_le_bytes([self.palette[(idx as usize)*2],self.palette[(idx as usize)*2+1]]); } } }
    fn render_mode3_fallback(&mut self) { self.framebuffer.fill(0); }
}
