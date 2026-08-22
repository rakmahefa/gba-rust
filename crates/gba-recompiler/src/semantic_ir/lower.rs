use crate::cfg::{BlockId, Program};
use crate::function::FunctionControlFlowGraph;
use crate::ir::{IrControlEffect, IrInstruction, IrMemoryKind, IrMemoryWidth};

use super::{
    MemoryEffect, MemoryWidth, SemanticBlock, SemanticFunction, SemanticInstruction, SemanticProgram,
    SemanticTerminator,
};

fn memory_width(width: IrMemoryWidth) -> MemoryWidth {
    match width {
        IrMemoryWidth::Byte => MemoryWidth::Byte,
        IrMemoryWidth::Halfword => MemoryWidth::Halfword,
        IrMemoryWidth::Word => MemoryWidth::Word,
    }
}

fn semantic_instruction(ir: &IrInstruction) -> SemanticInstruction {
    let memory = ir.memory().map(|memory| match memory.kind {
        IrMemoryKind::Read => MemoryEffect::Read {
            width: memory_width(memory.width),
            base: memory.base,
        },
        IrMemoryKind::Write => MemoryEffect::Write {
            width: memory_width(memory.width),
            base: memory.base,
        },
        IrMemoryKind::ReadWrite => MemoryEffect::ReadWrite {
            width: memory_width(memory.width),
            base: memory.base,
        },
    });
    let flags = ir.flags();
    SemanticInstruction {
        address: ir.address,
        size: ir.size,
        ops: ir.ops.clone(),
        reads: ir.reads(),
        writes: ir.writes(),
        memory,
        flags: super::FlagEffect {
            read: flags.reads_any(),
            write: flags.writes_any(),
        },
    }
}

fn terminator(block: &SemanticBlock) -> SemanticTerminator {
    let Some(effect) = block
        .instructions
        .iter()
        .rev()
        .map(SemanticInstruction::control_effect)
        .find(|effect| !matches!(effect, IrControlEffect::None))
    else {
        return SemanticTerminator::Fallthrough;
    };

    match effect {
        IrControlEffect::Branch {
            target,
            condition,
            link: true,
        } => SemanticTerminator::Call { target, condition },
        IrControlEffect::Branch {
            target,
            condition,
            link: false,
        } => SemanticTerminator::Branch { target, condition },
        IrControlEffect::BranchExchange {
            register: 14,
            link: false,
        } => SemanticTerminator::Return,
        IrControlEffect::BranchExchange {
            register,
            link: true,
        } => SemanticTerminator::IndirectCall {
            register,
            mode: block.mode,
        },
        IrControlEffect::BranchExchange {
            register,
            link: false,
        } => SemanticTerminator::IndirectBranch { register },
        IrControlEffect::Unknown => SemanticTerminator::Unknown,
        IrControlEffect::None => SemanticTerminator::Fallthrough,
    }
}

fn semantic_successors(
    source_successors: &[BlockId],
    terminator: &SemanticTerminator,
) -> Vec<BlockId> {
    match terminator {
        SemanticTerminator::Return
        | SemanticTerminator::IndirectBranch { .. }
        | SemanticTerminator::Unknown => Vec::new(),
        _ => source_successors.to_vec(),
    }
}

pub fn build_semantic_program(
    program: &Program,
    functions: &FunctionControlFlowGraph,
) -> Result<SemanticProgram, String> {
    let mut semantic_functions = Vec::with_capacity(functions.functions.len());
    for function in &functions.functions {
        let mut blocks = Vec::with_capacity(function.blocks.len());
        for &block_id in &function.blocks {
            let block = program.cfg.blocks.get(block_id.0).ok_or_else(|| {
                format!(
                    "function {} references missing block {}",
                    function.id.0, block_id.0
                )
            })?;
            let instructions = block.ir.iter().map(semantic_instruction).collect::<Vec<_>>();
            let mut semantic = SemanticBlock {
                id: block.id,
                address: block.key.address,
                mode: block.key.mode,
                instructions,
                successors: Vec::new(),
                terminator: SemanticTerminator::Unknown,
            };
            semantic.terminator = terminator(&semantic);
            semantic.successors = semantic_successors(&block.successors, &semantic.terminator);
            blocks.push(semantic);
        }
        semantic_functions.push(SemanticFunction {
            id: function.id,
            entry: function.entry,
            blocks,
            successors: function.successors.clone(),
            calls: function.call_sites.clone(),
            returns: function.return_sites.clone(),
        });
    }

    let semantic = SemanticProgram {
        entry: functions.entry,
        functions: semantic_functions,
        block_to_function: functions.block_to_function.clone(),
    };
    super::validate::validate_semantic_program(program, functions, &semantic)?;
    Ok(semantic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{Mode, ROM_BASE};
    use crate::{analyze, discover_functions};

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn unknown_control_effect_drops_cfg_successors() {
        let program = analyze(&arm_rom(&[0xE7F0_0010]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = &semantic.functions[0].blocks[0];
        assert_eq!(block.terminator, SemanticTerminator::Unknown);
        assert!(block.successors.is_empty());
    }
}