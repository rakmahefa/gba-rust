use gba_runtime::{GeneratedBlockExit, GeneratedExecutionExit, Runtime, RuntimeContract};

#[test]
fn max_steps_counts_blocks_not_runtime_cycles() {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            0x0800_0000,
            false,
            Some(3),
            |rt, address, thumb| {
                rt.tick(7);
                Ok(GeneratedBlockExit::continue_to(address, thumb))
            },
            |_, _| true,
        )
        .expect("execution should terminate by step limit");

    assert_eq!(result.steps, 3);
    assert!(matches!(
        result.exit,
        GeneratedExecutionExit::StepLimitExceeded { .. }
    ));
    assert_eq!(result.state.cycles, 21);
}

#[test]
fn continue_target_must_be_linked() {
    let mut runtime = Runtime::new();
    let error = runtime.run_generated_contract(
        0x0800_0000,
        false,
        Some(1),
        |_, address, thumb| Ok(GeneratedBlockExit::continue_to(address + 4, thumb)),
        |address, thumb| address == 0x0800_0000 && !thumb,
    );

    assert_eq!(error, Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG));
}

#[test]
fn continue_target_rejects_misaligned_arm_address() {
    let mut runtime = Runtime::new();
    let error = runtime.run_generated_contract(
        0x0800_0000,
        false,
        Some(1),
        |_, address, thumb| Ok(GeneratedBlockExit::continue_to(address + 2, thumb)),
        |_, _| true,
    );

    assert_eq!(error, Err(gba_runtime::GENERATED_TARGET_MISALIGNED));
}
