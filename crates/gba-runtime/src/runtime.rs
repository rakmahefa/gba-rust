use std::collections::HashMap;

use super::bios::{InterruptController, PowerState};
use super::cartridge::Cartridge;
use super::cpu::Cpu;
use super::{Apu, Ppu};

const EWRAM_LEN: usize = 0x40000;
const IWRAM_LEN: usize = 0x8000;
const PALETTE_LEN: usize = 0x400;
const VRAM_LEN: usize = 0x18000;
const OAM_LEN: usize = 0x400;
const KEYINPUT_DEFAULT: u16 = 0x03ff;

#[path = "runtime_bios.rs"]
mod runtime_bios;
#[path = "runtime_cpu.rs"]
mod runtime_cpu;
#[path = "runtime_execution.rs"]
mod runtime_execution;
#[path = "runtime_memory.rs"]
mod runtime_memory;

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[derive(Debug)]
pub struct Runtime {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub apu: Apu,
    pub cartridge: Option<Cartridge>,
    pub io: HashMap<u32, u8>,
    pub ewram: [u8; EWRAM_LEN],
    pub iwram: [u8; IWRAM_LEN],
    pub palette: [u8; PALETTE_LEN],
    pub vram: [u8; VRAM_LEN],
    pub oam: [u8; OAM_LEN],
    pub interrupts: InterruptController,
    pub power: PowerState,
    pub waitcnt: u16,
    pub postflg: u8,
    pub keyinput: u16,
    pub dispstat: u16,
    pub cycles: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            cpu: Cpu::default(),
            ppu: Ppu::default(),
            apu: Apu::default(),
            cartridge: None,
            io: HashMap::new(),
            ewram: [0; EWRAM_LEN],
            iwram: [0; IWRAM_LEN],
            palette: [0; PALETTE_LEN],
            vram: [0; VRAM_LEN],
            oam: [0; OAM_LEN],
            interrupts: InterruptController::default(),
            power: PowerState::default(),
            waitcnt: 0,
            postflg: 0,
            keyinput: KEYINPUT_DEFAULT,
            dispstat: 0,
            cycles: 0,
        }
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }
}
