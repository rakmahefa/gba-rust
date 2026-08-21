use gba_recompiler::{
    analyze, build_semantic_program, discover_functions, generate_semantic, Mode, ROM_BASE,
};
use gba_runtime::{GeneratedBlockExit, GeneratedExecutionExit, Runtime, RuntimeContract, CPSR_Z};

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

#[test]
fn generated_blocks_execute_across_edges_and_preserve_memory_state() {
    let words = [0xE3A0_0001u32, 0xE280_0002u32, 0xE581_0000u32];
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("semantic contract");
    let generated = generate_semantic(&program, &semantic, "entry");
    assert!(generated.source.contains("run_generated_contract"));
    assert!(generated.source.contains("fn dispatch_block"));
    assert!(generated.source.contains("GeneratedBlockExit"));
    assert!(!generated.source.contains("execute_arm_instruction"));
    assert!(!generated.source.contains("execute_thumb_instruction"));

    let mut runtime = Runtime::new();
    runtime.write_reg(1, 0x0400_0000);
    runtime.write_reg(14, 0x0200_0001);

    let result = runtime
        .run_generated_contract(
            ROM_BASE,
            false,
            Some(16),
            |rt, address, thumb| match (address, thumb) {
                (0x0800_0000, false) => {
                    rt.enter_instruction(address, false);
                    assert_eq!(rt.execute_arm_instruction(words[0]), None);
                    rt.tick(1);
                    Ok(GeneratedBlockExit::continue_to(0x0800_0004, false))
                }
                (0x0800_0004, false) => {
                    rt.enter_instruction(address, false);
                    assert_eq!(rt.execute_arm_instruction(words[1]), None);
                    rt.tick(1);
                    Ok(GeneratedBlockExit::continue_to(0x0800_0008, false))
                }
                (0x0800_0008, false) => {
                    rt.enter_instruction(address, false);
                    assert_eq!(rt.execute_arm_instruction(words[2]), None);
                    rt.tick(1);
                    let (target, target_thumb) = rt.exchange_target_for_dispatch(rt.read_reg(14));
                    Ok(GeneratedBlockExit::return_to(target, target_thumb))
                }
                _ => Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG),
            },
            |address, thumb| {
                matches!(
                    (address, thumb),
                    (0x0800_0000, false) | (0x0800_0004, false) | (0x0800_0008, false)
                )
            },
        )
        .expect("generated execution should terminate by return");

    assert_eq!(result.steps, 3);
    assert_eq!(
        result.exit,
        GeneratedExecutionExit::Returned {
            address: 0x0200_0000,
            thumb: true
        }
    );
    assert_eq!(runtime.read_reg(0), 3);
    assert_eq!(runtime.read32(0x0400_0000), 3);
    assert_eq!(runtime.architectural_state().cpsr & CPSR_Z, 0);
    assert_eq!(runtime.cycles, 3);
}

#[test]
fn generated_return_to_linked_block_is_resumed_instead_of_terminating() {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            ROM_BASE,
            false,
            Some(8),
            |rt, address, thumb| match (address, thumb) {
                (0x0800_0000, false) => {
                    rt.write_reg(0, 7);
                    Ok(GeneratedBlockExit::return_to(0x0800_0004, false))
                }
                (0x0800_0004, false) => {
                    rt.write_reg(0, rt.read_reg(0) + 1);
                    Ok(GeneratedBlockExit::return_to(0x0200_0000, true))
                }
                _ => Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG),
            },
            |address, thumb| {
                matches!(
                    (address, thumb),
                    (0x0800_0000, false) | (0x0800_0004, false)
                )
            },
        )
        .expect("linked return should resume the generated caller");

    assert_eq!(result.steps, 2);
    assert_eq!(
        result.exit,
        GeneratedExecutionExit::Returned {
            address: 0x0200_0000,
            thumb: true
        }
    );
    assert_eq!(runtime.read_reg(0), 8);
}

#[test]
fn generated_execution_step_limit_is_deterministic_for_self_loop() {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            ROM_BASE,
            false,
            Some(4),
            |_, address, thumb| Ok(GeneratedBlockExit::continue_to(address, thumb)),
            |_, _| true,
        )
        .expect("step limit is a normal terminal outcome");

    assert_eq!(result.steps, 4);
    assert_eq!(
        result.exit,
        GeneratedExecutionExit::StepLimitExceeded {
            address: ROM_BASE,
            thumb: false
        }
    );
    assert_eq!(result.state.pc(), ROM_BASE);
}
