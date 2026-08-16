pub mod cfg;
pub mod codegen;
pub mod decoder;
pub mod ir;

pub use cfg::{analyze, AnalysisError, BasicBlock, BlockId, BlockKey, ControlFlowGraph, Program};
pub use codegen::{generate, RustModule};
pub use decoder::{decode_arm, decode_thumb, ArmOp, Condition, DecodeError, Instruction, InstructionKind, Mode, ThumbOp, ROM_BASE};
pub use ir::{lower, IrInstruction, IrOp, Value};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pipeline_reaches_codegen() {
        let mut rom = Vec::new();
        rom.extend_from_slice(&0xE3A0_0001u32.to_le_bytes());
        rom.extend_from_slice(&0xE280_0001u32.to_le_bytes());
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].ir.len(), 2);
        let module = generate(&program, "entry");
        assert!(module.source.contains("rt.cpu.r[0]"));
        assert!(module.source.contains("block_08000000"));
    }
}
