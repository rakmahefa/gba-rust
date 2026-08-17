use crate::{Runtime, REG_PC};

pub const RUNTIME_CONTRACT_VERSION: u32 = 2;

/// Architectural state exposed to differential tests and alternate generated backends.
/// This deliberately excludes host-only runtime state (PPU/APU buffers, cartridge caches,
/// I/O maps) so comparisons stay focused on CPU execution semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitecturalState {
    pub registers: [u32; 16],
    pub cpsr: u32,
    pub thumb: bool,
    pub cycles: u64,
}

impl ArchitecturalState {
    pub fn pc(&self) -> u32 {
        self.registers[REG_PC]
    }
}

/// Stable execution surface consumed by generated code and differential tests.
///
/// The inherent `Runtime` methods remain the zero-overhead implementation path; this trait
/// names the architectural contract explicitly and keeps host-only state out of comparisons.
pub trait RuntimeContract {
    fn architectural_state(&self) -> ArchitecturalState;
    fn enter_instruction(&mut self, address: u32, thumb: bool);
    fn link_from_instruction(&mut self, address: u32, size: u8, thumb: bool);
    fn condition_code(&self, condition: u8) -> bool;
    fn read8(&self, address: u32) -> u8;
    fn read16(&self, address: u32) -> u16;
    fn read32(&self, address: u32) -> u32;
    fn write8(&mut self, address: u32, value: u8);
    fn write16(&mut self, address: u32, value: u16);
    fn write32(&mut self, address: u32, value: u32);
    fn execute_arm_instruction(&mut self, raw: u32) -> Option<(u32, bool)>;
    fn execute_thumb_instruction(&mut self, raw: u16) -> Option<(u32, bool)>;
    fn exchange_target_for_dispatch(&mut self, target: u32) -> (u32, bool);
    fn tick(&mut self, cycles: u32);
}

impl RuntimeContract for Runtime {
    fn architectural_state(&self) -> ArchitecturalState {
        ArchitecturalState {
            registers: self.cpu.r,
            cpsr: self.cpu.cpsr,
            thumb: self.cpu.thumb,
            cycles: self.cycles,
        }
    }

    fn enter_instruction(&mut self, address: u32, thumb: bool) {
        Runtime::enter_instruction(self, address, thumb);
    }

    fn link_from_instruction(&mut self, address: u32, size: u8, thumb: bool) {
        Runtime::link_from_instruction(self, address, size, thumb);
    }

    fn condition_code(&self, condition: u8) -> bool {
        Runtime::condition_code(self, condition)
    }

    fn read8(&self, address: u32) -> u8 {
        Runtime::read8(self, address)
    }

    fn read16(&self, address: u32) -> u16 {
        Runtime::read16(self, address)
    }

    fn read32(&self, address: u32) -> u32 {
        Runtime::read32(self, address)
    }

    fn write8(&mut self, address: u32, value: u8) {
        Runtime::write8(self, address, value);
    }

    fn write16(&mut self, address: u32, value: u16) {
        Runtime::write16(self, address, value);
    }

    fn write32(&mut self, address: u32, value: u32) {
        Runtime::write32(self, address, value);
    }

    fn execute_arm_instruction(&mut self, raw: u32) -> Option<(u32, bool)> {
        if raw & 0x0fff_fff0 == 0x012f_ff10 || raw & 0x0fff_fff0 == 0x012f_ff30 {
            let target = self.read_reg((raw & 0x0f) as usize);
            return Some(self.exchange_target_for_dispatch(target));
        }
        Runtime::execute_arm_instruction(self, raw)
    }

    fn execute_thumb_instruction(&mut self, raw: u16) -> Option<(u32, bool)> {
        Runtime::execute_thumb_instruction(self, raw)
    }

    fn exchange_target_for_dispatch(&mut self, target: u32) -> (u32, bool) {
        Runtime::exchange_target_for_dispatch(self, target)
    }

    fn tick(&mut self, cycles: u32) {
        Runtime::tick(self, cycles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_version_is_explicit_and_snapshot_is_architectural() {
        let mut runtime = Runtime::new();
        runtime.enter_instruction(0x0800_0100, false);
        runtime.tick(3);
        let state = runtime.architectural_state();
        assert_eq!(RUNTIME_CONTRACT_VERSION, 2);
        assert_eq!(state.pc(), 0x0800_0108);
        assert_eq!(state.cycles, 3);
    }

    #[test]
    fn contract_dispatch_surface_matches_inherent_runtime_semantics() {
        let mut runtime = Runtime::new();
        RuntimeContract::enter_instruction(&mut runtime, 0x0800_0100, false);
        let before = runtime.architectural_state();
        let result = RuntimeContract::execute_arm_instruction(&mut runtime, 0xE3A0_0001);
        assert_eq!(result, None);
        let after = runtime.architectural_state();
        assert_eq!(after.registers[0], 1);
        assert_eq!(after.pc(), before.pc());
    }

    #[test]
    fn contract_bx_uses_architectural_exchange_instead_of_dispatch_panic() {
        let mut runtime = Runtime::new();
        runtime.write_reg(0, 0x0800_0101);
        let result = RuntimeContract::execute_arm_instruction(&mut runtime, 0xE12F_FF10);
        assert_eq!(result, Some((0x0800_0100, true)));
        assert!(runtime.architectural_state().thumb);
    }

    #[test]
    fn contract_memory_surface_round_trips_without_touching_cpu_state() {
        let mut runtime = Runtime::new();
        let before = runtime.architectural_state();
        RuntimeContract::write32(&mut runtime, 0x0400_0000, 0x4433_2211);
        assert_eq!(RuntimeContract::read8(&runtime, 0x0400_0000), 0x11);
        assert_eq!(RuntimeContract::read16(&runtime, 0x0400_0000), 0x2211);
        assert_eq!(RuntimeContract::read32(&runtime, 0x0400_0000), 0x4433_2211);
        assert_eq!(RuntimeContract::read32(&runtime, 0x0400_0001), 0x1144_3322);
        assert_eq!(runtime.architectural_state(), before);
    }
}