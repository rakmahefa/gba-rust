use crate::{GeneratedBlockExit, GeneratedExecutionExit, GeneratedExecutionResult, Runtime, RuntimeContract, ArchitecturalState, GeneratedBlockKey, REG_PC};

/// A statically generated block that can be invoked directly by its predecessor.
pub type GeneratedLinkedBlock =
    fn(&mut Runtime) -> Result<LinkedBlockExit, &'static str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkedBlockExit {
    Next {
        block: GeneratedLinkedBlock,
        address: u32,
        thumb: bool,
    },
    Return {
        address: u32,
        thumb: bool,
    },
    Halt {
        address: u32,
        thumb: bool,
    },
    Exception(crate::ExceptionKind),
}

/// Executes statically linked generated blocks without an address-based dispatcher
/// between linked successors. Dynamic/exception boundaries remain explicit exits.
pub fn run_generated_linked(
    runtime: &mut Runtime,
    entry: GeneratedLinkedBlock,
    address: u32,
    thumb: bool,
    max_steps: Option<u64>,
) -> Result<GeneratedExecutionResult, &'static str> {
    let mut block = entry;
    let mut next = GeneratedBlockKey::new(address, thumb);
    let mut steps = 0u64;

    loop {
        if let Some(limit) = max_steps {
            if steps >= limit {
                runtime.cpu.set_thumb(next.thumb);
                runtime.cpu.r[REG_PC] = next.address;
                return Ok(GeneratedExecutionResult {
                    exit: GeneratedExecutionExit::StepLimitExceeded {
                        address: next.address,
                        thumb: next.thumb,
                    },
                    steps,
                    state: ArchitecturalState {
                        registers: runtime.cpu.r,
                        cpsr: runtime.cpu.cpsr,
                        thumb: runtime.cpu.thumb,
                        cycles: runtime.cycles,
                    },
                });
            }
        }

        runtime.cpu.set_thumb(next.thumb);
        runtime.cpu.r[REG_PC] = next.address;
        let exit = block(runtime)?;
        steps = steps.saturating_add(1);

        match exit {
            LinkedBlockExit::Next {
                block: target,
                address,
                thumb,
            } => {
                if !GeneratedBlockKey::is_aligned(address, thumb) {
                    return Err(crate::GENERATED_TARGET_MISALIGNED);
                }
                block = target;
                next = GeneratedBlockKey::new(address, thumb);
            }
            LinkedBlockExit::Return { address, thumb } => {
                runtime.cpu.set_thumb(thumb);
                runtime.cpu.r[REG_PC] = GeneratedBlockKey::align(address, thumb);
                return Ok(GeneratedExecutionResult {
                    exit: GeneratedExecutionExit::Returned {
                        address: GeneratedBlockKey::align(address, thumb),
                        thumb,
                    },
                    steps,
                    state: runtime.architectural_state(),
                });
            }
            LinkedBlockExit::Halt { address, thumb } => {
                runtime.cpu.set_thumb(thumb);
                runtime.cpu.r[REG_PC] = GeneratedBlockKey::align(address, thumb);
                return Ok(GeneratedExecutionResult {
                    exit: GeneratedExecutionExit::Halted {
                        address: GeneratedBlockKey::align(address, thumb),
                        thumb,
                    },
                    steps,
                    state: runtime.architectural_state(),
                });
            }
            LinkedBlockExit::Exception(kind) => {
                let (vector, vector_thumb) = runtime.enter_exception(kind);
                return Ok(GeneratedExecutionResult {
                    exit: GeneratedExecutionExit::ExceptionVector {
                        kind,
                        address: GeneratedBlockKey::align(vector, vector_thumb),
                        thumb: vector_thumb,
                    },
                    steps,
                    state: runtime.architectural_state(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(runtime: &mut Runtime) -> Result<LinkedBlockExit, &'static str> {
        let _ = runtime;
        Ok(LinkedBlockExit::Next {
            block: second,
            address: 0x0800_0004,
            thumb: false,
        })
    }

    fn second(runtime: &mut Runtime) -> Result<LinkedBlockExit, &'static str> {
        runtime.tick(3);
        Ok(LinkedBlockExit::Halt {
            address: 0x0800_0008,
            thumb: false,
        })
    }

    #[test]
    fn linked_successor_executes_without_address_dispatch() {
        let mut runtime = Runtime::new();
        let result = run_generated_linked(
            &mut runtime,
            first,
            0x0800_0000,
            false,
            Some(2),
        )
        .expect("linked execution should terminate");

        assert_eq!(result.steps, 2);
        assert_eq!(result.state.cycles, 3);
        assert!(matches!(
            result.exit,
            GeneratedExecutionExit::Halted {
                address: 0x0800_0008,
                thumb: false
            }
        ));
    }
}
