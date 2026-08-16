use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

use crate::decoder::{decode_arm, decode_thumb, read_arm, read_thumb, ArmOp, BranchKind, DecodeError, Instruction, InstructionKind, Mode, ThumbOp, ROM_BASE};
use crate::ir::{lower, IrInstruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockKey {
    pub address: u32,
    pub mode: Mode,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub key: BlockKey,
    pub instructions: Vec<Instruction>,
    pub ir: Vec<IrInstruction>,
    pub successors: Vec<BlockId>,
}

#[derive(Debug, Clone, Default)]
pub struct ControlFlowGraph {
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub entry: BlockId,
    pub cfg: ControlFlowGraph,
}

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("entry {0:#x} is outside the cartridge ROM")]
    InvalidEntry(u32),
}

fn branch_targets(ins: Instruction) -> (Option<(u32, Mode)>, bool) {
    match ins.kind {
        InstructionKind::Arm(ArmOp::Branch { target, condition, link }) => {
            let conditional = condition != crate::decoder::Condition::Al;
            (Some((target, Mode::Arm)), !conditional && !link)
        }
        InstructionKind::Arm(ArmOp::BranchExchange { .. }) => (None, true),
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => {
            let conditional = condition != crate::decoder::Condition::Al;
            (Some((target, Mode::Thumb)), !conditional)
        }
        InstructionKind::Thumb(ThumbOp::BranchExchange { .. }) => (None, true),
        _ => (None, false),
    }
}

fn next_address(ins: Instruction) -> u32 { ins.address + ins.size as u32 }

pub fn analyze(rom: &[u8], entry: u32, entry_mode: Mode) -> Result<Program, AnalysisError> {
    if entry < ROM_BASE || entry - ROM_BASE >= rom.len() as u32 {
        return Err(AnalysisError::InvalidEntry(entry));
    }

    let mut blocks = Vec::<BasicBlock>::new();
    let mut ids = HashMap::<BlockKey, BlockId>::new();
    let mut queue = VecDeque::<BlockKey>::new();
    let entry_key = BlockKey { address: entry, mode: entry_mode };
    queue.push_back(entry_key.clone());
    ids.insert(entry_key, BlockId(0));

    while let Some(key) = queue.pop_front() {
        let id = ids[&key];
        let mut pc = key.address;
        let mut instructions = Vec::new();

        loop {
            let instruction = match key.mode {
                Mode::Arm => decode_arm(pc, read_arm(rom, pc)?),
                Mode::Thumb => decode_thumb(pc, read_thumb(rom, pc)?),
            };
            instructions.push(instruction);
            let (target, terminate) = branch_targets(instruction);
            let next = next_address(instruction);
            let terminal_unknown_exchange = matches!(
                instruction.kind,
                InstructionKind::Arm(ArmOp::BranchExchange { .. }) | InstructionKind::Thumb(ThumbOp::BranchExchange { .. })
            );
            if target.is_some() || terminate || terminal_unknown_exchange || matches!(instruction.kind, InstructionKind::Arm(ArmOp::Branch { .. }) | InstructionKind::Thumb(ThumbOp::Branch { .. })) {
                break;
            }
            pc = next;
            if pc < ROM_BASE || pc - ROM_BASE >= rom.len() as u32 { break; }
        }

        let ir = instructions.iter().copied().map(lower).collect::<Vec<_>>();
        let mut block = BasicBlock { id, key: key.clone(), instructions, ir, successors: Vec::new() };
        let last = *block.instructions.last().expect("block contains at least one instruction");

        let mut successor_keys = Vec::new();
        match last.kind {
            InstructionKind::Arm(ArmOp::Branch { target, condition, link }) => {
                successor_keys.push(BlockKey { address: target, mode: Mode::Arm });
                if condition != crate::decoder::Condition::Al || link {
                    successor_keys.push(BlockKey { address: next_address(last), mode: Mode::Arm });
                }
            }
            InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => {
                successor_keys.push(BlockKey { address: target, mode: Mode::Thumb });
                if condition != crate::decoder::Condition::Al {
                    successor_keys.push(BlockKey { address: next_address(last), mode: Mode::Thumb });
                }
            }
            InstructionKind::Arm(ArmOp::BranchExchange { .. }) | InstructionKind::Thumb(ThumbOp::BranchExchange { .. }) => {}
            _ => successor_keys.push(BlockKey { address: next_address(last), mode: key.mode }),
        }

        for successor in successor_keys {
            if successor.address < ROM_BASE || successor.address - ROM_BASE >= rom.len() as u32 { continue; }
            let successor_id = if let Some(existing) = ids.get(&successor) { *existing } else {
                let new = BlockId(ids.len());
                ids.insert(successor.clone(), new);
                queue.push_back(successor);
                new
            };
            block.successors.push(successor_id);
        }
        while blocks.len() <= id.0 { blocks.push(BasicBlock { id: BlockId(blocks.len()), key: BlockKey { address: 0, mode: Mode::Arm }, instructions: Vec::new(), ir: Vec::new(), successors: Vec::new() }); }
        blocks[id.0] = block;
    }

    for block in &mut blocks {
        block.successors.sort_unstable_by_key(|b| b.0);
        block.successors.dedup();
    }
    let cfg = ControlFlowGraph { entry: BlockId(0), blocks };
    Ok(Program { entry: BlockId(0), cfg })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(words: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in words { bytes.extend_from_slice(&word.to_le_bytes()); }
        bytes
    }

    #[test]
    fn discovers_arm_branch_and_fallthrough() {
        let bytes = rom(&[0xEA00_0001, 0xE1A0_0000, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].successors.len(), 1);
    }

    #[test]
    fn keeps_thumb_mode_for_conditional_branch() {
        let bytes = vec![0x00, 0x28, 0x00, 0xE0, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].key.mode, Mode::Thumb);
    }
}
