use std::collections::HashMap;

use crate::cfg::{BlockId, Program};
use crate::function::{FunctionControlFlowGraph, FunctionId};
use crate::ir::{IrControlEffect, IrInstruction, IrMemoryEffect, IrMemoryKind, IrMemoryWidth};
use crate::decoder::{ArmExtended, ThumbExtended};

use super::{
    MemoryEffect, MemoryWidth, SemanticBlock, SemanticInstruction, SemanticProgram,
    SemanticTerminator,
};

fn memory_width(width: IrMemoryWidth) -> MemoryWidth {
    match width {
        IrMemoryWidth::Byte => MemoryWidth::Byte,
        IrMemoryWidth::Halfword => MemoryWidth::Halfword,
        IrMemoryWidth::Word => MemoryWidth::Word,
    }
}

fn same_memory(source: Option<IrMemoryEffect>, semantic: Option<MemoryEffect>) -> bool {
    match (source, semantic) {
        (None, None) => true,
        (Some(source), Some(semantic)) => {
            let width = memory_width(source.width);
            match source.kind {
                IrMemoryKind::Read => {
                    semantic
                        == MemoryEffect::Read {
                            width,
                            base: source.base,
                            address_is_dynamic: source.address_is_dynamic,
                        }
                }
                IrMemoryKind::Write => {
                    semantic
                        == MemoryEffect::Write {
                            width,
                            base: source.base,
                            address_is_dynamic: source.address_is_dynamic,
                        }
                }
                IrMemoryKind::ReadWrite => {
                    semantic
                        == MemoryEffect::ReadWrite {
                            width,
                            base: source.base,
                            address_is_dynamic: source.address_is_dynamic,
                        }
                }
            }
        }
        _ => false,
    }
}

fn semantic_instruction(ir: &IrInstruction) -> SemanticInstruction {
    let memory = ir.memory().map(|memory| match memory.kind {
        IrMemoryKind::Read => MemoryEffect::Read {
            width: memory_width(memory.width),
            base: memory.base,
            address_is_dynamic: memory.address_is_dynamic,
        },
        IrMemoryKind::Write => MemoryEffect::Write {
            width: memory_width(memory.width),
            base: memory.base,
            address_is_dynamic: memory.address_is_dynamic,
        },
        IrMemoryKind::ReadWrite => MemoryEffect::ReadWrite {
            width: memory_width(memory.width),
            base: memory.base,
            address_is_dynamic: memory.address_is_dynamic,
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
            read_n: flags.read_n,
            read_z: flags.read_z,
            read_c: flags.read_c,
            read_v: flags.read_v,
            write_n: flags.write_n,
            write_z: flags.write_z,
            write_c: flags.write_c,
            write_v: flags.write_v,
        },
    }
}

fn software_interrupt_comment(block: &SemanticBlock) -> Option<u32> {
    block.instructions.last()?.ops.iter().rev().find_map(|op| match op {
        crate::ir::IrOp::ArmExtended {
            op: ArmExtended::SoftwareInterrupt { comment },
        }
        | crate::ir::IrOp::ThumbExtended {
            op: ThumbExtended::SoftwareInterrupt { comment },
        } => Some(*comment),
        _ => None,
    })
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
        IrControlEffect::Unknown => software_interrupt_comment(block)
            .map(|comment| SemanticTerminator::SoftwareInterrupt { comment })
            .unwrap_or(SemanticTerminator::Unknown),
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
        SemanticTerminator::SoftwareInterrupt { .. }
        | SemanticTerminator::Fallthrough
        | SemanticTerminator::Branch { .. }
        | SemanticTerminator::Call { .. }
        | SemanticTerminator::IndirectCall { .. } => source_successors.to_vec(),
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
            let block = program
                .cfg
                .blocks
                .get(block_id.0)
                .ok_or_else(|| format!("function {} references missing block {}", function.id.0, block_id.0))?;
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
