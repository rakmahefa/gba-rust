mod lower;
mod types;
pub(crate) mod validate;

pub use lower::build_semantic_program;
pub use types::{
    FlagEffect, MemoryEffect, MemoryWidth, SemanticBlock, SemanticFunction, SemanticInstruction,
    SemanticProgram, SemanticTerminator,
};
pub use validate::validate_semantic_program;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{Condition, Mode, ROM_BASE};
    use crate::ir::{IrControlEffect, IrOp};
    use crate::{analyze, discover_functions};

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn semantic_lowering_preserves_instruction_effects() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = &semantic.functions[0].blocks[0];
        assert_eq!(block.instructions[0].reads, Vec::<u8>::new());
        assert_eq!(block.instructions[0].writes, vec![0]);
        assert_eq!(block.instructions[1].reads, vec![0]);
        assert_eq!(block.instructions[1].writes, vec![0]);
        assert_eq!(block.instructions[0].control_effect(), IrControlEffect::None);
    }

    #[test]
    fn semantic_condition_preserves_flag_dependency() {
        let program = analyze(
            &arm_rom(&[0x0A00_0000, 0xE1A0_0000, 0xE1A0_0000]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let flags = semantic.functions[0].blocks[0].instructions[0].flags;
        assert!(flags.read);
        assert!(!flags.write);
    }

    #[test]
    fn semantic_extended_instruction_has_effects() {
        let program = analyze(&arm_rom(&[0xE000_0291]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let instruction = &semantic.functions[0].blocks[0].instructions[0];
        assert_eq!(instruction.reads, vec![1, 2]);
        assert_eq!(instruction.writes, vec![0]);
        assert!(!instruction.flags.write);
        assert_eq!(instruction.control_effect(), IrControlEffect::None);
    }

    #[test]
    fn semantic_call_has_explicit_continuation() {
        let program = analyze(&arm_rom(&[0xEB00_0000, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert!(matches!(
            semantic.functions[0].blocks[0].terminator,
            SemanticTerminator::Call { .. }
        ));
        assert!(!semantic.functions[0].blocks[0].successors.is_empty());
    }

    #[test]
    fn semantic_return_has_no_successor() {
        let program = analyze(&arm_rom(&[0xE12F_FF1E]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert_eq!(
            semantic.functions[0].blocks[0].terminator,
            SemanticTerminator::Return
        );
        assert!(semantic.functions[0].blocks[0].successors.is_empty());
    }

    #[test]
    fn resolved_bx_lr_is_a_semantic_return_without_executable_successor() {
        let rom = arm_rom([
            0xE59F_E000,
            0xE12F_FF1E,
            0xE1A0_0000,
        ].as_slice());
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| {
                block.terminator == SemanticTerminator::Return
                    && block.instructions.last().map(|i| i.address) == Some(ROM_BASE + 4)
            })
            .expect("resolved BX LR semantic return block must be present");
        assert_eq!(block.terminator, SemanticTerminator::Return);
        assert!(block.successors.is_empty());
    }

    #[test]
    fn resolved_indirect_branch_is_dynamic_without_executable_successor() {
        let rom = arm_rom([
            0xE59F_3000,
            0xE12F_FF13,
            0xE1A0_0000,
        ].as_slice());
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| {
                block.terminator == SemanticTerminator::IndirectBranch { register: 3 }
                    && block.instructions.last().map(|i| i.address) == Some(ROM_BASE + 4)
            })
            .expect("resolved BX r3 semantic indirect-branch block must be present");
        assert_eq!(
            block.terminator,
            SemanticTerminator::IndirectBranch { register: 3 }
        );
        assert!(block.successors.is_empty());
    }

    #[test]
    fn semantic_validation_rejects_changed_reads() {
        let program = analyze(&arm_rom(&[0xE3A0_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let mut semantic = build_semantic_program(&program, &functions).unwrap();
        semantic.functions[0].blocks[0].instructions[0].reads.push(1);
        let error = validate_semantic_program(&program, &functions, &semantic).unwrap_err();
        assert!(error.contains("instruction reads changed"));
    }

    #[test]
    fn semantic_validation_rejects_changed_control_effect() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let mut semantic = build_semantic_program(&program, &functions).unwrap();
        semantic.functions[0].blocks[0].instructions[0].ops.push(IrOp::Branch {
            target: ROM_BASE,
            condition: Condition::Al,
            link: false,
        });
        let error = validate_semantic_program(&program, &functions, &semantic).unwrap_err();
        assert!(error.contains("instruction control effect changed"));
    }

    #[test]
    fn semantic_validation_rejects_multiple_control_effects() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| block.instructions.len() == 2)
            .expect("two-instruction block must be present");
        let mut invalid_block = block.clone();
        invalid_block.instructions[0].ops.push(IrOp::BranchExchange {
            register: 14,
            link: false,
        });
        invalid_block.instructions[1].ops.push(IrOp::Branch {
            target: ROM_BASE,
            condition: Condition::Al,
            link: false,
        });
        let error = validate::validate_control_placement(&invalid_block).unwrap_err();
        assert!(error.contains("multiple control-effect instructions"));
    }
}
