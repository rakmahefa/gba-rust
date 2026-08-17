pub mod architecture;
pub mod cfg;
pub mod codegen;
pub mod decoder;
pub mod execution;
pub mod function;
pub mod ir;
pub mod optimization;
pub mod semantic_ir;

pub use architecture::{add_with_carry, architectural_pc, condition_holds as architectural_condition_holds, exchange_target, link_address, rotate_unaligned_word, shift_immediate, shift_register, NzcvMask, ShiftResult, ShiftType};
pub use cfg::{analyze, AnalysisError, BasicBlock, BlockId, BlockKey, ControlFlowGraph, Program};
pub use codegen::{generate, generate_semantic, RustModule};
pub use decoder::{decode_arm, decode_thumb, decode_thumb_bl, ArmOp, Condition, DecodeError, Instruction, InstructionKind, Mode, ThumbOp, ROM_BASE};
pub use execution::{add_flags, branch_target, condition_holds, sub_flags, Nzcv, CPSR_C, CPSR_N, CPSR_T, CPSR_V, CPSR_Z, REG_COUNT, REG_LR, REG_PC, REG_SP};
pub use function::{discover_functions, CallSite, CallTarget, Function, FunctionControlFlowGraph, FunctionId, FunctionKey, ReturnSite};
pub use ir::{lower, IrControlEffect, IrFlags, IrInstruction, IrMemoryEffect, IrMemoryKind, IrMemoryWidth, IrOp, Value};
pub use optimization::{optimize_semantic_program, OptimizationChange, OptimizationKind, OptimizationReport};
pub use semantic_ir::{build_semantic_program, validate_semantic_program, FlagEffect, MemoryEffect, MemoryWidth, SemanticBlock, SemanticFunction, SemanticInstruction, SemanticProgram, SemanticTerminator};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pipeline_reaches_specialized_codegen() {
        let mut rom = Vec::new();
        rom.extend_from_slice(&0xE3A0_0001u32.to_le_bytes());
        rom.extend_from_slice(&0xE280_0001u32.to_le_bytes());
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].ir.len(), 2);
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert_eq!(semantic.functions.len(), 1);
        let (optimized, report) = optimize_semantic_program(&semantic);
        assert!(report.changed());
        assert_eq!(optimized.functions[0].blocks[0].instructions.len(), semantic.functions[0].blocks[0].instructions.len());
        let module = generate_semantic(&program, &optimized, "entry");
        assert!(module.source.contains("rt.enter_instruction"));
        assert!(module.source.contains("rt.mov(0"));
        assert!(module.source.contains("rt.add(0, rt.read_reg(0)"));
        assert!(!module.source.contains("rt.execute_arm_instruction"));
        assert!(!module.source.contains("rt.execute_thumb_instruction"));
    }
}
