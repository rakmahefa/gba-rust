use gba_runtime::{
    GeneratedBlockExit, GeneratedBlockKey, GeneratedExecutionExit, Runtime, RuntimeContract,
    GENERATED_TARGET_MISALIGNED, REG_PC,
};

#[test]
fn generated_block_key_canonicalizes_only_through_explicit_constructor() {
    let arm = GeneratedBlockKey::new(0x0800_0004, false);
    assert_eq!(arm.tuple(), (0x0800_0004, false));
    assert!(GeneratedBlockKey::is_aligned(0x0800_0004, false));

    let thumb = GeneratedBlockKey::new(0x0800_0005, true);
    assert_eq!(thumb.tuple(), (0x0800_0004, true));
    assert!(!GeneratedBlockKey::is_aligned(0x0800_0005, true));
}

#[test]
fn generated_contract_rejects_misaligned_continue_targets() {
    let mut runtime = Runtime::new();
    let result = runtime.run_generated_contract(
        0x0800_0000,
        false,
        Some(4),
        |_, _, _| Ok(GeneratedBlockExit::continue_to(0x0800_0002, false)),
        |_, _| true,
    );

    assert_eq!(result, Err(GENERATED_TARGET_MISALIGNED));
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0000);
}

#[test]
fn generated_contract_preserves_thumb_bit_for_terminal_targets() {
    let mut runtime = Runtime::new();
    let result = runtime
        .run_generated_contract(
            0x0800_0000,
            true,
            Some(1),
            |_, _, _| Ok(GeneratedBlockExit::halt(0x0800_0004, true)),
            |_, _| false,
        )
        .expect("halt is a normal terminal outcome");

    assert_eq!(
        result.exit,
        GeneratedExecutionExit::Halted {
            address: 0x0800_0004,
            thumb: true,
        }
    );
    assert_eq!(result.state.pc(), 0x0800_0004);
    assert!(result.state.thumb);
}
