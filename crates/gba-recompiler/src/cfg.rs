use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

use crate::decoder::{
    decode_arm, decode_thumb, decode_thumb_bl, read_arm, read_thumb, read_thumb_bl, ArmOp, Condition, DecodeError, Instruction,
    InstructionKind, Mode, ThumbOp, ROM_BASE,
};
use crate::ir::{lower, IrInstruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockKey { pub address: u32, pub mode: Mode }

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub key: BlockKey,
    pub instructions: Vec<Instruction>,
    pub ir: Vec<IrInstruction>,
    pub successors: Vec<BlockId>,
}

#[derive(Debug, Clone, Default)]
pub struct ControlFlowGraph { pub entry: BlockId, pub blocks: Vec<BasicBlock> }

#[derive(Debug, Clone, Default)]
pub struct Program { pub entry: BlockId, pub cfg: ControlFlowGraph }

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)] Decode(#[from] DecodeError),
    #[error("entry {0:#x} is outside the cartridge ROM")] InvalidEntry(u32),
}

#[derive(Debug, Clone)]
struct DiscoveredInstruction { instruction: Instruction, successors: Vec<BlockKey> }

fn next_key(instruction: Instruction) -> BlockKey {
    BlockKey { address: instruction.address + instruction.size as u32, mode: instruction.mode }
}

fn in_rom(rom: &[u8], address: u32) -> bool {
    address >= ROM_BASE && address - ROM_BASE < rom.len() as u32
}

fn instruction_successors(instruction: Instruction) -> Vec<BlockKey> {
    let next = next_key(instruction);
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch { target, condition, link }) => {
            let mut successors = vec![BlockKey { address: target, mode: Mode::Arm }];
            if condition != Condition::Al || link { successors.push(next); }
            successors
        }
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => {
            let mut successors = vec![BlockKey { address: target, mode: Mode::Thumb }];
            if condition != Condition::Al { successors.push(next); }
            successors
        }
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => vec![
            BlockKey { address: target, mode: Mode::Thumb },
            next,
        ],
        InstructionKind::Arm(ArmOp::BranchExchange { .. })
        | InstructionKind::Thumb(ThumbOp::BranchExchange { .. })
        | InstructionKind::Arm(ArmOp::Unknown)
        | InstructionKind::Thumb(ThumbOp::Unknown) => Vec::new(),
        _ => vec![next],
    }
}

fn decode_at(rom: &[u8], key: BlockKey) -> Result<Instruction, DecodeError> {
    match key.mode {
        Mode::Arm => Ok(decode_arm(key.address, read_arm(rom, key.address)?)),
        Mode::Thumb => {
            let raw = read_thumb(rom, key.address)?;
            if (raw & 0xF800) == 0xF000 {
                let (first, second) = read_thumb_bl(rom, key.address)?;
                Ok(decode_thumb_bl(key.address, first, second))
            } else {
                Ok(decode_thumb(key.address, raw))
            }
        }
    }
}

fn discover_reachable(
    rom: &[u8], entry: BlockKey,
) -> Result<(Vec<BlockKey>, HashMap<BlockKey, DiscoveredInstruction>), DecodeError> {
    let mut order = Vec::new();
    let mut discovered = HashMap::<BlockKey, DiscoveredInstruction>::new();
    let mut queued = HashSet::<BlockKey>::from([entry.clone()]);
    let mut queue = VecDeque::from([entry]);

    while let Some(key) = queue.pop_front() {
        if discovered.contains_key(&key) { continue; }
        let instruction = decode_at(rom, key.clone())?;
        let successors = instruction_successors(instruction);
        for successor in &successors {
            if in_rom(rom, successor.address) && queued.insert(successor.clone()) {
                queue.push_back(successor.clone());
            }
        }
        order.push(key.clone());
        discovered.insert(key, DiscoveredInstruction { instruction, successors });
    }
    Ok((order, discovered))
}

fn is_fallthrough(node: &DiscoveredInstruction) -> bool {
    node.successors.len() == 1 && node.successors[0] == next_key(node.instruction)
}

fn collect_leaders(order: &[BlockKey], discovered: &HashMap<BlockKey, DiscoveredInstruction>, entry: &BlockKey) -> Vec<BlockKey> {
    let mut leaders = HashSet::<BlockKey>::new();
    leaders.insert(entry.clone());
    for key in order {
        let Some(node) = discovered.get(key) else { continue; };
        if !is_fallthrough(node) {
            for successor in &node.successors {
                if discovered.contains_key(successor) { leaders.insert(successor.clone()); }
            }
        }
    }
    order.iter().filter(|key| leaders.contains(*key)).cloned().collect()
}

fn partition_blocks(order: &[BlockKey], discovered: &HashMap<BlockKey, DiscoveredInstruction>, leaders: &[BlockKey]) -> (Vec<BasicBlock>, HashMap<BlockKey, BlockId>) {
    let leader_set = leaders.iter().cloned().collect::<HashSet<_>>();
    let mut blocks = Vec::new();
    let mut ids = HashMap::<BlockKey, BlockId>::new();

    for leader in leaders {
        let id = BlockId(blocks.len());
        ids.insert(leader.clone(), id);
        let mut instructions = Vec::new();
        let mut cursor = leader.clone();
        loop {
            let Some(node) = discovered.get(&cursor) else { break; };
            instructions.push(node.instruction);
            if !is_fallthrough(node) { break; }
            let next = node.successors[0].clone();
            if leader_set.contains(&next) { break; }
            cursor = next;
        }
        let ir = instructions.iter().copied().map(lower).collect::<Vec<_>>();
        assert_eq!(instructions.len(), ir.len(), "IR must preserve one entry per instruction");
        blocks.push(BasicBlock { id, key: leader.clone(), instructions, ir, successors: Vec::new() });
    }

    for block in &mut blocks {
        let Some(last) = block.instructions.last().copied() else { continue; };
        let key = BlockKey { address: last.address, mode: last.mode };
        let Some(node) = discovered.get(&key) else { continue; };
        let mut successors = node.successors.iter().filter_map(|successor| ids.get(successor).copied()).collect::<Vec<_>>();
        successors.sort_unstable_by_key(|id| id.0);
        successors.dedup();
        block.successors = successors;
    }
    (blocks, ids)
}

fn validate_cfg(cfg: &ControlFlowGraph, expected_instruction_count: usize) {
    assert!(!cfg.blocks.is_empty());
    assert!(cfg.entry.0 < cfg.blocks.len());
    let mut seen = HashMap::<BlockKey, BlockId>::new();
    let mut owners = HashMap::<BlockKey, BlockId>::new();
    for block in &cfg.blocks {
        assert_eq!(block.id, BlockId(seen.len()));
        assert!(seen.insert(block.key.clone(), block.id).is_none());
        assert!(!block.instructions.is_empty());
        assert_eq!(block.instructions.len(), block.ir.len());
        let first = block.instructions[0];
        assert_eq!(block.key, BlockKey { address: first.address, mode: first.mode });
        for (index, instruction) in block.instructions.iter().enumerate() {
            let key = BlockKey { address: instruction.address, mode: instruction.mode };
            assert!(owners.insert(key, block.id).is_none());
            if let Some(next) = block.instructions.get(index + 1) {
                assert_eq!(instruction.address + instruction.size as u32, next.address);
                assert_eq!(instruction.mode, next.mode);
            }
        }
        for successor in &block.successors { assert!(successor.0 < cfg.blocks.len()); }
    }
    assert_eq!(owners.len(), expected_instruction_count);
}

pub fn analyze(rom: &[u8], entry: u32, entry_mode: Mode) -> Result<Program, AnalysisError> {
    if !in_rom(rom, entry) { return Err(AnalysisError::InvalidEntry(entry)); }
    let entry_key = BlockKey { address: entry, mode: entry_mode };
    let (order, discovered) = discover_reachable(rom, entry_key.clone())?;
    let leaders = collect_leaders(&order, &discovered, &entry_key);
    let (blocks, ids) = partition_blocks(&order, &discovered, &leaders);
    let entry_id = ids[&entry_key];
    let cfg = ControlFlowGraph { entry: entry_id, blocks };
    validate_cfg(&cfg, discovered.len());
    Ok(Program { entry: entry_id, cfg })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arm_rom(words: &[u32]) -> Vec<u8> { words.iter().flat_map(|word| word.to_le_bytes()).collect() }

    #[test]
    fn discovery_preserves_sequential_arm_tail() {
        let bytes = arm_rom(&[0xE3A0_0001, 0xE280_0001]);
        let entry = BlockKey { address: ROM_BASE, mode: Mode::Arm };
        let (order, discovered) = discover_reachable(&bytes, entry).unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(discovered.len(), 2);
        assert!(discovered.contains_key(&BlockKey { address: ROM_BASE, mode: Mode::Arm }));
        assert!(discovered.contains_key(&BlockKey { address: ROM_BASE + 4, mode: Mode::Arm }));
    }

    #[test]
    fn sequential_arm_instructions_remain_in_one_block() {
        let bytes = arm_rom(&[0xE3A0_0001, 0xE280_0001]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].instructions.len(), 2);
        assert_eq!(program.cfg.blocks[0].ir.len(), 2);
    }

    #[test]
    fn discovers_arm_branch_and_fallthrough() {
        let bytes = arm_rom(&[0xEA00_0000, 0xE1A0_0000, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].successors, vec![BlockId(1)]);
    }

    #[test]
    fn conditional_branch_splits_fallthrough_and_target() {
        let bytes = vec![0x00, 0xD0, 0xC0, 0x46, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 3);
        assert_eq!(program.cfg.blocks[0].successors.len(), 2);
        assert!(program.cfg.blocks.iter().all(|block| block.key.mode == Mode::Thumb));
    }

    #[test]
    fn backward_branch_splits_an_already_discovered_block() {
        let bytes = arm_rom(&[0xEAFF_FFFE, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn conditional_backward_branch_does_not_overlap_blocks() {
        let bytes = vec![0x00, 0xD0, 0xC0, 0x46, 0xFC, 0xD0, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        let mut keys = HashSet::new();
        for block in &program.cfg.blocks { for instruction in &block.instructions { assert!(keys.insert((instruction.address, instruction.mode))); } }
    }

    #[test]
    fn unknown_instruction_terminates_discovery() {
        let bytes = arm_rom(&[0xFFFFFFFF, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].instructions.len(), 1);
        assert!(matches!(program.cfg.blocks[0].instructions[0].kind, InstructionKind::Arm(ArmOp::Unknown)));
    }

    #[test]
    fn thumb_bl_is_a_four_byte_call_with_fallthrough() {
        let bytes = vec![0x00, 0xF0, 0x00, 0xF8, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].instructions[0].size, 4);
        assert_eq!(program.cfg.blocks[0].successors.len(), 1);
    }
}
