use std::collections::HashMap;

use super::bios::{InterruptController, PowerState};
use super::bios_memory::{Bios, BiosLoadError};
use super::cartridge::Cartridge;
use super::cpu::Cpu;
use super::dma::DmaController;
use super::scheduler::TimingScheduler;
use super::sio::Sio;
use super::timers::{Timer, TIMER_COUNT};
use super::{Apu, Ppu};

const EWRAM_LEN: usize = 0x40000;
const IWRAM_LEN: usize = 0x8000;
const PALETTE_LEN: usize = 0x400;
const VRAM_LEN: usize = 0x18000;
const OAM_LEN: usize = 0x400;
const KEYINPUT_DEFAULT: u16 = 0x03ff;
const KEYCNT_DEFAULT: u16 = 0;

#[path = "runtime_bios.rs"] mod runtime_bios;
#[path = "runtime_cpu.rs"] mod runtime_cpu;
#[path = "runtime_execution.rs"] mod runtime_execution;
#[path = "runtime_memory.rs"] mod runtime_memory;
#[path = "runtime_dma.rs"] mod runtime_dma;
#[cfg(test)] #[path = "runtime_tests.rs"] mod tests;
#[cfg(test)] #[path = "mmio_tests.rs"] mod mmio_tests;
#[cfg(test)] #[path = "timer_tests.rs"] mod timer_tests;

#[derive(Debug)]
pub struct Runtime {
    pub cpu: Cpu,
    pub bios: Bios,
    pub ppu: Ppu,
    pub apu: Apu,
    pub sio: Sio,
    pub cartridge: Option<Cartridge>,
    pub io: HashMap<u32, u8>,
    pub ewram: [u8; EWRAM_LEN],
    pub iwram: [u8; IWRAM_LEN],
    pub palette: [u8; PALETTE_LEN],
    pub vram: [u8; VRAM_LEN],
    pub oam: [u8; OAM_LEN],
    pub interrupts: InterruptController,
    pub timers: [Timer; TIMER_COUNT],
    pub dma: DmaController,
    pub power: PowerState,
    pub dispcnt: u16,
    pub waitcnt: u16,
    pub postflg: u8,
    pub keyinput: u16,
    pub keycnt: u16,
    pub dispstat: u16,
    pub vcount: u16,
    pub scheduler: TimingScheduler,
    pub cycles: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        let mut scheduler = TimingScheduler::new();
        scheduler.schedule_at(super::scheduler::HBLANK_START_CYCLES, super::scheduler::EventKind::PpuHBlankStart);
        scheduler.schedule_at(super::scheduler::CYCLES_PER_SCANLINE, super::scheduler::EventKind::PpuScanline);
        Self {
            cpu: Cpu::default(), bios: Bios::default(), ppu: Ppu::default(), apu: Apu::default(), sio: Sio::default(), cartridge: None, io: HashMap::new(),
            ewram: [0; EWRAM_LEN], iwram: [0; IWRAM_LEN], palette: [0; PALETTE_LEN], vram: [0; VRAM_LEN], oam: [0; OAM_LEN],
            interrupts: InterruptController::default(), timers: std::array::from_fn(|_| Timer::default()), dma: DmaController::default(), power: PowerState::default(),
            dispcnt: 0, waitcnt: 0, postflg: 0, keyinput: KEYINPUT_DEFAULT, keycnt: KEYCNT_DEFAULT, dispstat: 0, vcount: 0, scheduler, cycles: 0,
        }
    }
}

impl Runtime {
    pub fn new() -> Self { Self::default() }
    pub fn load_bios(&mut self, bytes: &[u8]) -> Result<(), BiosLoadError> { self.bios = Bios::from_bytes(bytes)?; Ok(()) }
    pub fn bios(&self) -> &Bios { &self.bios }

    /// Update one of the GBA's 10 active-low keypad bits and evaluate KEYCNT.
    pub fn set_key_pressed(&mut self, key: u8, pressed: bool) {
        if key >= 10 { return; }
        let bit = 1u16 << key;
        if pressed { self.keyinput &= !bit; } else { self.keyinput |= bit; }
        self.update_keypad_irq();
    }

    /// Replace the complete active-low keypad state and evaluate KEYCNT.
    pub fn set_key_input(&mut self, pressed_mask: u16) {
        self.keyinput = KEYINPUT_DEFAULT & !(pressed_mask & 0x03ff);
        self.update_keypad_irq();
    }

    fn update_keypad_irq(&mut self) {
        if self.keycnt & super::mmio::KEYCNT_IRQ_ENABLE == 0 { return; }
        let selected = self.keycnt & super::mmio::KEYCNT_KEY_MASK;
        if selected == 0 { return; }
        let pressed = (!self.keyinput) & super::mmio::KEYCNT_KEY_MASK;
        let matched = if self.keycnt & super::mmio::KEYCNT_AND != 0 { pressed & selected == selected } else { pressed & selected != 0 };
        if matched { self.interrupts.request(super::bios::IRQ_KEYPAD); }
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use crate::bios::IRQ_KEYPAD;
    use crate::mmio;

    #[test] fn key_input_is_active_low() { let mut runtime = Runtime::new(); assert_eq!(runtime.keyinput, 0x03ff); runtime.set_key_pressed(0, true); assert_eq!(runtime.keyinput & 1, 0); runtime.set_key_pressed(0, false); assert_ne!(runtime.keyinput & 1, 0); }
    #[test] fn key_input_ignores_non_architectural_bits() { let mut runtime = Runtime::new(); runtime.set_key_input(0xffff); assert_eq!(runtime.keyinput, 0); runtime.set_key_pressed(10, true); assert_eq!(runtime.keyinput, 0); }
    #[test] fn keypad_irq_supports_or_and_modes() {
        let mut runtime = Runtime::new(); runtime.interrupts.ie = IRQ_KEYPAD; runtime.keycnt = mmio::KEYCNT_IRQ_ENABLE | 0b11;
        runtime.set_key_pressed(0, true); assert_ne!(runtime.interrupts.iflags & IRQ_KEYPAD, 0);
        runtime.interrupts.iflags = 0; runtime.keycnt |= mmio::KEYCNT_AND; runtime.set_key_pressed(0, false); assert_eq!(runtime.interrupts.iflags & IRQ_KEYPAD, 0);
        runtime.set_key_pressed(1, true); assert_eq!(runtime.interrupts.iflags & IRQ_KEYPAD, 0); runtime.set_key_pressed(0, true); assert_ne!(runtime.interrupts.iflags & IRQ_KEYPAD, 0);
    }
}