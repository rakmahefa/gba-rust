use gba_runtime::{
    CpuMode, ExceptionKind, GeneratedBlockExit, GeneratedExecutionExit, Runtime, RuntimeContract,
};

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
fn continue_target_is_statically_linked_without_cfg_probe() {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            0x0800_0000,
            false,
            Some(1),
            |_, address, thumb| Ok(GeneratedBlockExit::continue_to(address + 4, thumb)),
            |_, _| panic!("static continue transitions must not probe CFG membership"),
        )
        .expect("statically linked continue target should dispatch directly");

    assert_eq!(result.steps, 1);
    assert!(matches!(
        result.exit,
        GeneratedExecutionExit::StepLimitExceeded {
            address: 0x0800_0004,
            thumb: false,
        }
    ));
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

#[test]
fn exception_exit_delivers_an_unlinked_exception_vector() {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            0x0800_0000,
            false,
            Some(1),
            |_, _, _| Ok(GeneratedBlockExit::exception(ExceptionKind::Irq)),
            |_, _| false,
        )
        .expect("exception vector should be an execution result");

    assert_eq!(result.steps, 1);
    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert!(matches!(
        result.exit,
        GeneratedExecutionExit::ExceptionVector {
            kind: ExceptionKind::Irq,
            address: 0x18,
            thumb: false,
        }
    ));
}
