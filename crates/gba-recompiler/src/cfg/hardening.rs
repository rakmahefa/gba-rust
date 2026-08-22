use std::collections::HashMap;

use super::model::{BasicBlock, BlockId, BlockKey, ControlFlowGraph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidationError {
    EmptyGraph,
    InvalidEntry(BlockId),
    NonContiguousBlockId { expected: BlockId, actual: BlockId },
    DuplicateBlockKey(BlockKey),
    EmptyBlock(BlockId),
    BlockKeyMismatch { block: BlockId, expected: BlockKey, actual: BlockKey },
    DuplicateInstruction { address: u32, mode: crate::decoder::Mode },
    NonContiguousInstructions { block: BlockId, previous: u32, next: u32 },
    ModeChangedWithinBlock { block: BlockId, previous: crate::decoder::Mode, next: crate::decoder::Mode },
    InvalidSuccessor { block: BlockId, successor: BlockId },
    InstructionIrMismatch { block: BlockId, instructions: usize, ir: usize },
    InstructionCountMismatch { expected: usize, actual: usize },
}

fn validate_block(block: &BasicBlock, block_count: usize, owners: &mut HashMap<BlockKey, BlockId>) -> Result<(), ValidationError> {
    if block.instructions.is_empty() {
        return Err(ValidationError::EmptyBlock(block.id));
    }
    if block.instructions.len() != block.ir.len() {
        return Err(ValidationError::InstructionIrMismatch {
            block: block.id,
            instructions: block.instructions.len(),
            ir: block.ir.len(),
        });
    }
    let first = block.instructions[0];
    let expected = BlockKey { address: first.address, mode: first.mode };
    if block.key != expected {
        return Err(ValidationError::BlockKeyMismatch { block: block.id, expected, actual: block.key.clone() });
    }
    for (index, instruction) in block.instructions.iter().enumerate() {
        let key = BlockKey { address: instruction.address, mode: instruction.mode };
        if owners.insert(key.clone(), block.id).is_some() {
            return Err(ValidationError::DuplicateInstruction { address: instruction.address, mode: instruction.mode });
        }
        if let Some(next) = block.instructions.get(index + 1) {
            if instruction.address.wrapping_add(instruction.size as u32) != next.address {
                return Err(ValidationError::NonContiguousInstructions { block: block.id, previous: instruction.address, next: next.address });
            }
            if instruction.mode != next.mode {
                return Err(ValidationError::ModeChangedWithinBlock { block: block.id, previous: instruction.mode, next: next.mode });
            }
        }
    }
    for successor in &block.successors {
        if successor.0 >= block_count {
            return Err(ValidationError::InvalidSuccessor { block: block.id, successor: *successor });
        }
    }
    Ok(())
}

pub(crate) fn validate_cfg(cfg: &ControlFlowGraph, expected_instruction_count: usize) -> Result<(), ValidationError> {
    if cfg.blocks.is_empty() {
        return Err(ValidationError::EmptyGraph);
    }
    if cfg.entry.0 >= cfg.blocks.len() {
        return Err(ValidationError::InvalidEntry(cfg.entry));
    }

    let mut seen = HashMap::<BlockKey, BlockId>::new();
    let mut owners = HashMap::<BlockKey, BlockId>::new();
    for (index, block) in cfg.blocks.iter().enumerate() {
        let expected_id = BlockId(index);
        if block.id != expected_id {
            return Err(ValidationError::NonContiguousBlockId { expected: expected_id, actual: block.id });
        }
        if seen.insert(block.key.clone(), block.id).is_some() {
            return Err(ValidationError::DuplicateBlockKey(block.key.clone()));
        }
        validate_block(block, cfg.blocks.len(), &mut owners)?;
    }

    if owners.len() != expected_instruction_count {
        return Err(ValidationError::InstructionCountMismatch { expected: expected_instruction_count, actual: owners.len() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::Mode;
    use crate::ir::IrInstruction;

    fn empty_block(id: usize, address: u32) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            key: BlockKey { address, mode: Mode::Arm },
            instructions: Vec::new(),
            ir: Vec::<IrInstruction>::new(),
            successors: Vec::new(),
        }
    }

    #[test]
    fn rejects_empty_graph() {
        let cfg = ControlFlowGraph { entry: BlockId(0), blocks: Vec::new() };
        assert_eq!(validate_cfg(&cfg, 0), Err(ValidationError::EmptyGraph));
    }

    #[test]
    fn rejects_invalid_entry() {
        let cfg = ControlFlowGraph { entry: BlockId(1), blocks: vec![empty_block(0, 0x0800_0000)] };
        assert_eq!(validate_cfg(&cfg, 0), Err(ValidationError::InvalidEntry(BlockId(1))));
    }
}
