use std::env;

use crate::{Runtime, REG_PC};

pub const RUNTIME_CONTRACT_VERSION: u32 = 5;
pub const GENERATED_TARGET_OUTSIDE_CFG: &str =
    "generated direct target is outside the statically linked CFG";
pub const GENERATED_TARGET_DYNAMIC_UNRESOLVED: &str =
    "generated indirect target is unresolved or outside the statically linked CFG";
pub const GENERATED_TARGET_MISALIGNED: &str =
    "generated target cannot be represented by the requested execution mode";

const GENERATED_TRACE_ENV: &str = "GBA_GENERATED_TRACE";
const GENERATED_TRACE_LIMIT_ENV: &str = "GBA_GENERATED_TRACE_LIMIT";
const DEFAULT_GENERATED_TRACE_LIMIT: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GeneratedTraceConfig {
    enabled: bool,
    limit: u64,
}
impl GeneratedTraceConfig {
    fn from_env() -> Self {
        let enabled = env::var(GENERATED_TRACE_ENV)
            .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        let limit = env::var(GENERATED_TRACE_LIMIT_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_GENERATED_TRACE_LIMIT);
        Self { enabled, limit }
    }

    #[inline]
    fn should_log(&self, step: u64) -> bool {
        self.enabled && step < self.limit
    }

    #[inline]
    fn log_transition(
        &self,
        step: u64,
        source: GeneratedBlockKey,
        exit: GeneratedBlockExit,
        target: Option<GeneratedBlockKey>,
        cycles: u64,
    ) {
        if !self.should_log(step) {
            return;
        }

        eprintln!(
            "[generated-trace] step={step} source={:#010x}/{} exit={exit:?} target={} cycles={cycles}",
            source.address,
            if source.thumb { "Thumb" } else { "Arm" },
            match target {
                Some(target) => format!(
                    "{:#010x}/{}",
                    target.address,
                    if target.thumb { "Thumb" } else { "Arm" }
                ),
                None => "<none>".to_string(),
            },
        );
    }

    #[inline]
    fn log_dispatch_error(&self, step: u64, source: GeneratedBlockKey, error: &'static str) {
        if !self.should_log(step) {
            return;
        }
        eprintln!(
            "[generated-trace] step={step} source={:#010x}/{} dispatch_error={error}",
            source.address,
            if source.thumb { "Thumb" } else { "Arm" },
        );
    }

    #[inline]
    fn log_step_limit(&self, step: u64, next: GeneratedBlockKey) {
        if !self.enabled {
            return;
        }
        eprintln!(
            "[generated-trace] step-limit steps={step} next={:#010x}/{} trace_limit={}",
            next.address,
            if next.thumb { "Thumb" } else { "Arm" },
            self.limit,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneratedBlockKey {
    pub address: u32,
    pub thumb: bool,
}
impl GeneratedBlockKey {
    pub const fn new(address: u32, thumb: bool) -> Self {
        Self {
            address: Self::align(address, thumb),
            thumb,
        }
    }
    pub const fn align(address: u32, thumb: bool) -> u32 {
        address & if thumb { !1 } else { !3 }
    }
    pub const fn tuple(self) -> (u32, bool) {
        (self.address, self.thumb)
    }
    pub const fn is_aligned(address: u32, thumb: bool) -> bool {
        Self::align(address, thumb) == address
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedBlockExit {
    Continue { address: u32, thumb: bool },
    Dynamic { address: u32, thumb: bool },
    Return { address: u32, thumb: bool },
    Halt { address: u32, thumb: bool },
}
impl GeneratedBlockExit {
    pub const fn continue_to(address: u32, thumb: bool) -> Self {
        Self::Continue { address, thumb }
    }
    pub const fn dynamic_to(address: u32, thumb: bool) -> Self {
        Self::Dynamic { address, thumb }
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
            | Self::Dynamic { address, thumb }
            | Self::Return { address, thumb }
            | Self::Halt { address, thumb } => (address, thumb),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedExecutionExit {
    Returned { address: u32, thumb: bool },
    Halted { address: u32, thumb: bool },
    StepLimitExceeded { address: u32, thumb: bool },
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        let trace = GeneratedTraceConfig::from_env();
        let mut next = GeneratedBlockKey::new(address, thumb);
        let mut steps = 0u64;
        loop {
            if let Some(limit) = max_steps {
                if steps >= limit {
                    self.cpu.set_thumb(next.thumb);
                    self.cpu.r[REG_PC] = next.address;
                    trace.log_step_limit(steps, next);
                    return Ok(GeneratedExecutionResult {
                        exit: GeneratedExecutionExit::StepLimitExceeded {
                            address: next.address,
                            thumb: next.thumb,
                        },
                        steps,
                        state: self.architectural_state(),
                    });
                }
            }

            self.cpu.set_thumb(next.thumb);
            self.cpu.r[REG_PC] = next.address;
            let source = next;
            let exit = match dispatch(self, next.address, next.thumb) {
                Ok(exit) => exit,
                Err(error) => {
                    trace.log_dispatch_error(steps, source, error);
                    return Err(error);
                }
            };
            steps = steps.saturating_add(1);

            let checked = |address: u32, thumb: bool| -> Result<GeneratedBlockKey, &'static str> {
                if !GeneratedBlockKey::is_aligned(address, thumb) {
                    return Err(GENERATED_TARGET_MISALIGNED);
                }
                Ok(GeneratedBlockKey::new(address, thumb))
            };

            match exit {
                GeneratedBlockExit::Continue { address, thumb } => {
                    let target = checked(address, thumb)?;
                    trace.log_transition(steps - 1, source, exit, Some(target), self.cycles);
                    if !is_linked(target.address, target.thumb) {
                        return Err(GENERATED_TARGET_OUTSIDE_CFG);
                    }
                    next = target;
                }
                GeneratedBlockExit::Dynamic { address, thumb } => {
                    let target = checked(address, thumb)?;
                    trace.log_transition(steps - 1, source, exit, Some(target), self.cycles);
                    if !is_linked(target.address, target.thumb) {
                        return Err(GENERATED_TARGET_DYNAMIC_UNRESOLVED);
                    }
                    next = target;
                }
                GeneratedBlockExit::Return { address, thumb } => {
                    let target = checked(address, thumb)?;
                    trace.log_transition(steps - 1, source, exit, Some(target), self.cycles);
                    self.cpu.set_thumb(target.thumb);
                    self.cpu.r[REG_PC] = target.address;
                    if is_linked(target.address, target.thumb) {
                        next = target;
                    } else {
                        return Ok(GeneratedExecutionResult {
                            exit: GeneratedExecutionExit::Returned {
                                address: target.address,
                                thumb: target.thumb,
                            },
                            steps,
                            state: self.architectural_state(),
                        });
                    }
                }
                GeneratedBlockExit::Halt { address, thumb } => {
                    let target = checked(address, thumb)?;
                    trace.log_transition(steps - 1, source, exit, Some(target), self.cycles);
                    self.cpu.set_thumb(target.thumb);
                    self.cpu.r[REG_PC] = target.address;
                    return Ok(GeneratedExecutionResult {
                        exit: GeneratedExecutionExit::Halted {
                            address: target.address,
                            thumb: target.thumb,
                        },
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
    fn trace_config_is_disabled_by_default_without_environment() {
        let config = GeneratedTraceConfig::from_env();
        assert!(!config.enabled || config.limit > 0);
    }

    #[test]
    fn generated_block_key_trace_identity_preserves_mode() {
        let key = GeneratedBlockKey::new(0x120, true);
        assert_eq!(key.tuple(), (0x120, true));
    }
}
