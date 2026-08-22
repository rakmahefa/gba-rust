use gba_recompiler::{build_semantic_program, discover_functions, analyze, Mode, ROM_BASE, SemanticTerminator};

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
        0xef00_0012, // SWI 0x12
        0xe1a0_0000, // MOV r0, r0
        0xef00_0034, // SWI 0x34
        0xe1a0_1001, // MOV r1, r1
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
            .filter(|instruction| !matches!(instruction.control_effect(), gba_recompiler::IrControlEffect::None))
            .count()
            <= 1
    }));
}

#[test]
fn thumb_swi_creates_a_terminal_semantic_block_with_continuation() {
    let rom = thumb_rom(&[
        0xdf12, // SWI 0x12
        0x1c00, // ADD r0, r0, #0
        0xdf34, // SWI 0x34
        0x1c09, // ADD r1, r1, #0
    ]);
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
