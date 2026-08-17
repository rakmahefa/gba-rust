use crate::{Runtime, REG_PC};

pub const RUNTIME_CONTRACT_VERSION: u32 = 3;
pub const GENERATED_TARGET_OUTSIDE_CFG: &str = "generated target is outside the statically linked CFG";

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

/// Result produced by one generated basic block.
///
/// `Continue` is the normal control-flow edge between statically linked blocks.
/// `Return` preserves a function return as a first-class event so the execution
/// driver can distinguish it from an ordinary branch. An unlinked return target
/// becomes the terminal boundary of the generated entry program.
/// `Halt` is an explicit generated-program termination event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedBlockExit {
    Continue { address: u32, thumb: bool },
    Return { address: u32, thumb: bool },
    Halt { address: u32, thumb: bool },
}

impl GeneratedBlockExit {
    pub const fn continue_to(address: u32, thumb: bool) -> Self {
        Self::Continue { address, thumb }
    }

    pub const fn return_to(address: u32, thumb: bool) -> Self {
        Self::Return { address, thumb }
    }

    pub const fn halt(address: u32, thumb: bool) -> Self {
        Self::Halt { address, thumb }
    }

    pub const fn target(self) -> (u32, bool) {
        match self {
            Self::Continue { address, thumb }
            | Self::Return { address, thumb }
            | Self::Halt { address, thumb } => (address, thumb),
        }
    }
}

/// Terminal reason for a generated execution session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedExecutionExit {
    Returned { address: u32, thumb: bool },
    Halted { address: u32, thumb: bool },
    StepLimitExceeded { address: u32, thumb: bool },
}

/// Deterministic result for the generated block execution driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedExecutionResult {
    pub exit: GeneratedExecutionExit,
    pub steps: u64,
    pub state: ArchitecturalState,
}

impl GeneratedExecutionResult {
    pub const fn target(&self) -> (u32, bool) {
        match self.exit {
            GeneratedExecutionExit::Returned { address, thumb }
            | GeneratedExecutionExit::Halted { address, thumb }
            | GeneratedExecutionExit::StepLimitExceeded { address, thumb } => (address, thumb),
        }
    }
}

/// Stable instruction- and block-level execution surface consumed by generated code and
/// differential tests.
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

    fn run_generated_contract<F, L>(
        &mut self,
        address: u32,
        thumb: bool,
        max_steps: Option<u64>,
        dispatch: F,
        is_linked: L,
    ) -> Result<GeneratedExecutionResult, &'static str>
    where
        Self: Sized,
        F: FnMut(&mut Runtime, u32, bool) -> Result<GeneratedBlockExit, &'static str>,
        L: Fn(u32, bool) -> bool;
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

    fn run_generated_contract<F, L>(
        &mut self,
        address: u32,
        thumb: bool,
        max_steps: Option<u64>,
        mut dispatch: F,
        is_linked: L,
    ) -> Result<GeneratedExecutionResult, &'static str>
    where
        F: FnMut(&mut Runtime, u32, bool) -> Result<GeneratedBlockExit, &'static str>,
        L: Fn(u32, bool) -> bool,
    {
        fn align(address: u32, thumb: bool) -> u32 {
            address & if thumb { !1 } else { !3 }
        }

        let mut next = (align(address, thumb), thumb);
        let mut steps = 0u64;

        loop {
            if let Some(limit) = max_steps {
                if steps >= limit {
                    self.cpu.set_thumb(next.1);
                    self.cpu.r[REG_PC] = next.0;
                    return Ok(GeneratedExecutionResult {
                        exit: GeneratedExecutionExit::StepLimitExceeded { address: next.0, thumb: next.1 },
                        steps,
                        state: self.architectural_state(),
                    });
                }
            }

            self.cpu.set_thumb(next.1);
            self.cpu.r[REG_PC] = next.0;
            let exit = dispatch(self, next.0, next.1)?;
            steps = steps.wrapping_add(1);

            match exit {
                GeneratedBlockExit::Continue { address, thumb } => {
                    next = (align(address, thumb), thumb);
                }
                GeneratedBlockExit::Return { address, thumb } => {
                    let target = (align(address, thumb), thumb);
                    self.cpu.set_thumb(target.1);
                    self.cpu.r[REG_PC] = target.0;
                    if is_linked(target.0, target.1) {
                        next = target;
                    } else {
                        return Ok(GeneratedExecutionResult {
                            exit: GeneratedExecutionExit::Returned { address: target.0, thumb: target.1 },
                            steps,
                            state: self.architectural_state(),
                        });
                    }
                }
                GeneratedBlockExit::Halt { address, thumb } => {
                    let target = (align(address, thumb), thumb);
                    self.cpu.set_thumb(target.1);
                    self.cpu.r[REG_PC] = target.0;
                    return Ok(GeneratedExecutionResult {
                        exit: GeneratedExecutionExit::Halted { address: target.0, thumb: target.1 },
                        steps,
                        state: self.architectural_state(),
                    });
                }
            }
        }
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
        assert_eq!(RUNTIME_CONTRACT_VERSION, 3);
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

    #[test]
    fn generated_exit_target_is_stable() {
        assert_eq!(GeneratedBlockExit::continue_to(0x0800_0100, false).target(), (0x0800_0100, false));
        assert_eq!(GeneratedBlockExit::return_to(0x0800_0101, true).target(), (0x0800_0101, true));
        assert_eq!(GeneratedBlockExit::halt(0x0400_0000, false).target(), (0x0400_0000, false));
    }

    #[test]
    fn generated_contract_honors_step_limits_without_panicking() {
        let mut runtime = Runtime::new();
        let result = runtime
            .run_generated_contract(0x0800_0000, false, Some(2), |rt, address, thumb| {
                rt.tick(1);
                Ok(GeneratedBlockExit::continue_to(address, thumb))
            }, |_, _| true)
            .expect("step limit is an expected terminal result");
        assert_eq!(result.steps, 2);
        assert_eq!(result.exit, GeneratedExecutionExit::StepLimitExceeded { address: 0x0800_0000, thumb: false });
        assert_eq!(result.state.pc(), 0x0800_0000);
        assert_eq!(runtime.cycles, 2);
    }

    #[test]
    fn generated_contract_returns_at_an_unlinked_function_return() {
        let mut runtime = Runtime::new();
        let result = runtime
            .run_generated_contract(0x0800_0000, false, None, |_, _, _| {
                Ok(GeneratedBlockExit::return_to(0x0200_0001, true))
            }, |_, _| false)
            .expect("return is an expected terminal boundary");
        assert_eq!(result.steps, 1);
        assert_eq!(result.exit, GeneratedExecutionExit::Returned { address: 0x0200_0000, thumb: true });
        assert!(result.state.thumb);
        assert_eq!(result.state.pc(), 0x0200_0000);
    }
}
