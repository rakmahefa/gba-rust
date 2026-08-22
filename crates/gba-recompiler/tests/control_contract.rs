use gba_recompiler::{
    analyze, build_semantic_program, discover_functions, IrControlEffect, Mode, ROM_BASE,
    SemanticTerminator,
};

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn thumb_rom(halfwords: &[u16]) -> Vec<u8> {
    halfwords
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

#[test]
fn arm_swi_creates_a_terminal_semantic_block_with_continuation() {
    let rom = arm_rom(&[
        0xef00_0012,
        0xe1a0_0000,
        0xef00_0034,
        0xe1a0_1001,
    ]);
    let program = analyze(&rom, ROM_BASE, Mode::Arm).expect("ARM CFG analysis should succeed");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions)
        .expect("ARM semantic control contract should accept separated SWIs");

    let blocks = semantic
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .collect::<Vec<_>>();
    let swi_blocks = blocks
        .iter()
        .filter(|block| matches!(block.terminator, SemanticTerminator::SoftwareInterrupt { .. }))
        .collect::<Vec<_>>();

    assert_eq!(swi_blocks.len(), 2);
    assert!(swi_blocks.iter().all(|block| block.successors.len() == 1));
    assert!(blocks.iter().all(|block| {
        block
            .instructions
            .iter()
            .filter(|instruction| !matches!(instruction.control_effect(), IrControlEffect::None))
            .count()
            <= 1
    }));
}

#[test]
fn thumb_swi_creates_a_terminal_semantic_block_with_continuation() {
    let rom = thumb_rom(&[0xdf12, 0x1c00, 0xdf34, 0x1c09]);
    let program = analyze(&rom, ROM_BASE, Mode::Thumb).expect("Thumb CFG analysis should succeed");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions)
        .expect("Thumb semantic control contract should accept separated SWIs");

    let swi_blocks = semantic
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .filter(|block| matches!(block.terminator, SemanticTerminator::SoftwareInterrupt { .. }))
        .collect::<Vec<_>>();

    assert_eq!(swi_blocks.len(), 2);
    assert!(swi_blocks.iter().all(|block| block.successors.len() == 1));
}

#[test]
fn unsupported_arm_coprocessor_instruction_is_a_hard_cfg_boundary() {
    let program = analyze(&arm_rom(&[0xFFFF_FFFF, 0xE1A0_0000]), ROM_BASE, Mode::Arm)
        .expect("CFG analysis should accept unsupported instructions as terminal boundaries");
    assert_eq!(program.cfg.blocks.len(), 1);
    assert_eq!(program.cfg.blocks[0].instructions.len(), 1);
    assert!(program.cfg.blocks[0].successors.is_empty());
}
