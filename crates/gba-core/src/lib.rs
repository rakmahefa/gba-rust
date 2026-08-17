mod cartridge;
mod bus;
mod cpu;

pub use cartridge::{Cartridge, GbaError, SaveKind, SaveStore};
pub use bus::{Bus, HEIGHT, WIDTH};
pub use cpu::Cpu;

pub struct Gba { pub cpu: Cpu, pub bus: Bus }
impl Gba {
    pub fn load(cart: Cartridge) -> Self { Self { cpu: Cpu::new(), bus: Bus::new(cart) } }
    pub fn run_frame(&mut self) { let start=self.bus.frame; while self.bus.frame==start { let cycles=self.cpu.step(&mut self.bus); self.bus.tick(cycles as u64); } }
    pub fn framebuffer(&self)->&[u16]{&self.bus.framebuffer}
    pub fn title(&self)->&str{&self.bus.cart.title}
    pub fn save_kind(&self)->SaveKind{self.bus.cart.save_kind}
    pub fn flush_save(&mut self)->std::io::Result<()> { self.bus.cart.save.flush() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn dimensions_are_gba(){assert_eq!((WIDTH,HEIGHT),(240,160));}
    #[test] fn save_store_roundtrip_in_memory(){let p=std::env::temp_dir().join(format!("gba-rust-test-{}",std::process::id()));let mut s=SaveStore::open(&p,"x",SaveKind::Sram32K).unwrap();s.bytes_mut()[7]=0x42;s.flush().unwrap();let s2=SaveStore::open(&p,"x",SaveKind::Sram32K).unwrap();assert_eq!(s2.bytes()[7],0x42);let _=std::fs::remove_dir_all(p);}
}
