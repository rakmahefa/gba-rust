use crate::{Runtime, REG_PC};

pub const RUNTIME_CONTRACT_VERSION: u32 = 1;

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

/// Stable execution surface consumed by generated code.
///
/// The inherent `Runtime` methods remain the zero-overhead implementation path; this trait
/// makes the contract explicit and gives differential tests a backend-neutral interface.
pub trait RuntimeContract {
    fn architectural_state(&self) -> ArchitecturalState;
    fn enter_instruction(&mut self, address: u32, thumb: bool);
    fn link_from_instruction(&mut self, address: u32, size: u8, thumb: bool);
    fn condition_code(&self, condition: u8) -> bool;
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

    fn execute_arm_instruction(&mut self, raw: u32) -> Option<(u32, bool)> {
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
        assert_eq!(RUNTIME_CONTRACT_VERSION, 1);
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
}
