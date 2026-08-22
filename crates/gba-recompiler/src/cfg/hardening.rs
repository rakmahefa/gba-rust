use std::collections::{HashMap, HashSet, VecDeque};

use crate::decoder::{ArmOp, Condition, InstructionKind, Mode, ThumbOp};

use super::edges::next_key;
use super::model::{BasicBlock, BlockId, BlockKey, ControlFlowGraph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    EmptyGraph,
    InvalidEntry(BlockId),
    NonContiguousBlockId { expected: BlockId, actual: BlockId },
    DuplicateBlockKey(BlockKey),
    EmptyBlock(BlockId),
    BlockKeyMismatch { block: BlockId, expected: BlockKey, actual: BlockKey },
    DuplicateInstruction { address: u32, mode: Mode },
    NonContiguousInstructions { block: BlockId, previous: u32, next: u32 },
    ModeChangedWithinBlock { block: BlockId, previous: Mode, next: Mode },
    InvalidSuccessor { block: BlockId, successor: BlockId },
    DuplicateSuccessor { block: BlockId, successor: BlockId },
    InstructionIrMismatch { block: BlockId, instructions: usize, ir: usize },
    InstructionCountMismatch { expected: usize, actual: usize },
    UnreachableBlock(BlockId),
    MissingEdge { block: BlockId, target: BlockKey },
    UnexpectedEdge { block: BlockId, target: BlockKey },
}

fn block_keys(cfg: &ControlFlowGraph) -> HashMap<BlockId, BlockKey> {
    cfg.blocks
        .iter()
        .map(|block| (block.id, block.key.clone()))
        .collect()
}

fn instruction_owners(cfg: &ControlFlowGraph) -> HashMap<BlockKey, BlockId> {
    cfg.blocks
        .iter()
        .flat_map(|block| {
            block.instructions.iter().map(move |instruction| {
                (
                    BlockKey {
                        address: instruction.address,
                        mode: instruction.mode,
                    },
                    block.id,
                )
            })
        })
        .collect()
}

fn successor_keys(
    block: &BasicBlock,
    block_keys: &HashMap<BlockId, BlockKey>,
) -> HashSet<BlockKey> {
    block
        .successors
        .iter()
        .filter_map(|id| block_keys.get(id).cloned())
        .collect()
}

fn require_edge(
    block: &BasicBlock,
    target: &BlockKey,
    owners: &HashMap<BlockKey, BlockId>,
    keys: &HashMap<BlockId, BlockKey>,
) -> Result<(), ValidationError> {
    if owners.contains_key(target) && !successor_keys(block, keys).contains(target) {
        return Err(ValidationError::MissingEdge {
            block: block.id,
            target: target.clone(),
        });
    }
    Ok(())
}

fn validate_terminator_edges(
    block: &BasicBlock,
    owners: &HashMap<BlockKey, BlockId>,
    keys: &HashMap<BlockId, BlockKey>,
) -> Result<(), ValidationError> {
    let Some(last) = block.instructions.last().copied() else {
        return Ok(());
    };

    let next = next_key(last);
    let actual = successor_keys(block, keys);

    match last.kind {
        InstructionKind::Arm(ArmOp::Branch {
            target,
            condition,
            link,
        }) => {
            let target = BlockKey {
                address: target,
                mode: Mode::Arm,
            };
            let mut allowed = HashSet::new();
            if owners.contains_key(&target) {
                allowed.insert(target.clone());
                require_edge(block, &target, owners, keys)?;
            }
            if condition != Condition::Al || link {
                if owners.contains_key(&next) {
                    allowed.insert(next.clone());
                    require_edge(block, &next, owners, keys)?;
                }
            }
            if let Some(unexpected) = actual.iter().find(|key| !allowed.contains(*key)) {
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: unexpected.clone(),
                });
            }
        }
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => {
            let target = BlockKey {
                address: target,
                mode: Mode::Thumb,
            };
            let mut allowed = HashSet::new();
            if owners.contains_key(&target) {
                allowed.insert(target.clone());
                require_edge(block, &target, owners, keys)?;
            }
            if condition != Condition::Al && owners.contains_key(&next) {
                allowed.insert(next.clone());
                require_edge(block, &next, owners, keys)?;
            }
            if let Some(unexpected) = actual.iter().find(|key| !allowed.contains(*key)) {
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: unexpected.clone(),
                });
            }
        }
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => {
            let target = BlockKey {
                address: target,
                mode: Mode::Thumb,
            };
            let mut allowed = HashSet::new();
            if owners.contains_key(&target) {
                allowed.insert(target.clone());
                require_edge(block, &target, owners, keys)?;
            }
            if owners.contains_key(&next) {
                allowed.insert(next.clone());
                require_edge(block, &next, owners, keys)?;
            }
            if let Some(unexpected) = actual.iter().find(|key| !allowed.contains(*key)) {
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: unexpected.clone(),
                });
            }
        }
        InstructionKind::Arm(ArmOp::BranchExchange { link, .. }) => {
            if actual.len() > usize::from(link) + 1 {
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: next,
                });
            }
            if link && owners.contains_key(&next) && !actual.contains(&next) {
                return Err(ValidationError::MissingEdge {
                    block: block.id,
                    target: next,
                });
            }
        }
        InstructionKind::Thumb(ThumbOp::BranchExchange { .. }) => {
            if actual.len() > 1 {
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: next,
                });
            }
        }
        InstructionKind::Arm(ArmOp::Unknown) | InstructionKind::Thumb(ThumbOp::Unknown) => {
            if let Some(unexpected) = actual.into_iter().next() {
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: unexpected,
                });
            }
        }
        _ => {
            require_edge(block, &next, owners, keys)?;
            if actual.iter().any(|key| key != &next) {
                let unexpected = actual.iter().find(|key| *key != &next).unwrap().clone();
                return Err(ValidationError::UnexpectedEdge {
                    block: block.id,
                    target: unexpected,
                });
            }
        }
    }

    Ok(())
}

fn validate_reachability(cfg: &ControlFlowGraph) -> Result<(), ValidationError> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([cfg.entry]);

    while let Some(id) = queue.pop_front() {
        if !reachable.insert(id) {
            continue;
        }
        queue.extend(cfg.blocks[id.0].successors.iter().copied());
    }

    if let Some(block) = cfg.blocks.iter().find(|block| !reachable.contains(&block.id)) {
        return Err(ValidationError::UnreachableBlock(block.id));
    }
    Ok(())
}

fn validate_block(
    block: &BasicBlock,
    block_count: usize,
    owners: &mut HashMap<BlockKey, BlockId>,
) -> Result<(), ValidationError> {
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
    let expected = BlockKey {
        address: first.address,
        mode: first.mode,
    };
    if block.key != expected {
        return Err(ValidationError::BlockKeyMismatch {
            block: block.id,
            expected,
            actual: block.key.clone(),
        });
    }

    for (index, instruction) in block.instructions.iter().enumerate() {
        let key = BlockKey {
            address: instruction.address,
            mode: instruction.mode,
        };
        if owners.insert(key, block.id).is_some() {
            return Err(ValidationError::DuplicateInstruction {
                address: instruction.address,
                mode: instruction.mode,
            });
        }
        if let Some(next) = block.instructions.get(index + 1) {
            if instruction.address.wrapping_add(instruction.size as u32) != next.address {
                return Err(ValidationError::NonContiguousInstructions {
                    block: block.id,
                    previous: instruction.address,
                    next: next.address,
                });
            }
            if instruction.mode != next.mode {
                return Err(ValidationError::ModeChangedWithinBlock {
                    block: block.id,
                    previous: instruction.mode,
                    next: instruction.mode,
                });
            }
        }
    }

    let mut successors = HashSet::new();
    for successor in &block.successors {
        if successor.0 >= block_count {
            return Err(ValidationError::InvalidSuccessor {
                block: block.id,
                successor: *successor,
            });
        }
        if !successors.insert(*successor) {
            return Err(ValidationError::DuplicateSuccessor {
                block: block.id,
                successor: *successor,
            });
        }
    }

    Ok(())
}

pub(super) fn validate_cfg(
    cfg: &ControlFlowGraph,
    expected_instruction_count: usize,
) -> Result<(), ValidationError> {
    if cfg.blocks.is_empty() {
        return Err(ValidationError::EmptyGraph);
    }
    if cfg.entry.0 >= cfg.blocks.len() {
        return Err(ValidationError::InvalidEntry(cfg.entry));
    }

    let mut seen = HashSet::<BlockKey>::new();
    let mut instruction_owners = HashMap::<BlockKey, BlockId>::new();

    for (index, block) in cfg.blocks.iter().enumerate() {
        let expected_id = BlockId(index);
        if block.id != expected_id {
            return Err(ValidationError::NonContiguousBlockId {
                expected: expected_id,
                actual: block.id,
            });
        }
        if !seen.insert(block.key.clone()) {
            return Err(ValidationError::DuplicateBlockKey(block.key.clone()));
        }
        validate_block(block, cfg.blocks.len(), &mut instruction_owners)?;
    }

    if instruction_owners.len() != expected_instruction_count {
        return Err(ValidationError::InstructionCountMismatch {
            expected: expected_instruction_count,
            actual: instruction_owners.len(),
        });
    }

    validate_reachability(cfg)?;

    let keys = block_keys(cfg);
    for block in &cfg.blocks {
        validate_terminator_edges(block, &instruction_owners, &keys)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::Mode;

    fn empty_block(id: usize, address: u32) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            key: BlockKey {
                address,
                mode: Mode::Arm,
            },
            instructions: Vec::new(),
            ir: Vec::new(),
            successors: Vec::new(),
        }
    }

    #[test]
    fn rejects_empty_graph() {
        let cfg = ControlFlowGraph {
            entry: BlockId(0),
            blocks: Vec::new(),
        };
        assert_eq!(validate_cfg(&cfg, 0), Err(ValidationError::EmptyGraph));
    }

    #[test]
    fn rejects_invalid_entry() {
        let cfg = ControlFlowGraph {
            entry: BlockId(1),
            blocks: vec![empty_block(0, 0x0800_0000)],
        };
        assert_eq!(
            validate_cfg(&cfg, 0),
            Err(ValidationError::InvalidEntry(BlockId(1)))
        );
    }
}
