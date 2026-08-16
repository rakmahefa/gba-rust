use std::collections::{HashMap, HashSet, VecDeque};

use crate::cfg::{BasicBlock, BlockId, BlockKey, ControlFlowGraph, Program};
use crate::decoder::{ArmOp, InstructionKind, Mode, ThumbOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FunctionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionKey {
    pub address: u32,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallTarget {
    Direct(BlockKey),
    Indirect { register: u8, link: bool, mode: Mode },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallSite {
    pub block: BlockId,
    pub instruction_index: usize,
    pub target: CallTarget,
    pub return_block: Option<BlockId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReturnSite {
    pub block: BlockId,
    pub instruction_index: usize,
    pub mode: Mode,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub id: FunctionId,
    pub key: FunctionKey,
    pub entry: BlockId,
    pub blocks: Vec<BlockId>,
    pub block_successors: HashMap<BlockId, Vec<BlockId>>,
    pub successors: Vec<FunctionId>,
    pub call_sites: Vec<CallSite>,
    pub return_sites: Vec<ReturnSite>,
}

#[derive(Debug, Clone, Default)]
pub struct FunctionControlFlowGraph {
    pub entry: FunctionId,
    pub functions: Vec<Function>,
    pub block_to_function: HashMap<BlockId, FunctionId>,
    pub calls: Vec<CallSite>,
    pub returns: Vec<ReturnSite>,
}

fn block_last_instruction(block: &BasicBlock) -> Option<(usize, crate::decoder::Instruction)> {
    block.instructions.len().checked_sub(1).map(|index| (index, block.instructions[index]))
}

fn is_direct_call(instruction: crate::decoder::Instruction) -> Option<u32> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch { target, link: true, .. }) => Some(target),
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => Some(target),
        _ => None,
    }
}

fn is_indirect_call(instruction: crate::decoder::Instruction) -> Option<u8> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link: true }) => Some(rm),
        _ => None,
    }
}

fn is_return(instruction: crate::decoder::Instruction) -> bool {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::BranchExchange { rm: 14, link: false }) => true,
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm: 14 }) => true,
        _ => false,
    }
}

fn block_for_key(cfg: &ControlFlowGraph, key: &BlockKey) -> Option<BlockId> {
    cfg.blocks.iter().find(|block| block.key == *key).map(|block| block.id)
}

fn direct_call_target(cfg: &ControlFlowGraph, block: &BasicBlock) -> Option<(BlockId, u32, Mode)> {
    let (_, instruction) = block_last_instruction(block)?;
    let target = is_direct_call(instruction)?;
    let mode = match instruction.kind {
        InstructionKind::Arm(_) => Mode::Arm,
        InstructionKind::Thumb(_) => Mode::Thumb,
    };
    let target_key = BlockKey { address: target, mode };
    Some((block_for_key(cfg, &target_key)?, target, mode))
}

fn function_roots(program: &Program) -> Vec<(FunctionKey, BlockId)> {
    let cfg = &program.cfg;
    let mut roots = vec![(cfg.blocks[cfg.entry.0].key.into(), cfg.entry)];
    let mut seen = HashSet::<BlockId>::from([cfg.entry]);

    for block in &cfg.blocks {
        if let Some((target, address, mode)) = direct_call_target(cfg, block) {
            if seen.insert(target) {
                roots.push((FunctionKey { address, mode }, target));
            }
        }
    }

    roots.sort_by_key(|(_, block)| block.0);
    roots
}

impl From<BlockKey> for FunctionKey {
    fn from(value: BlockKey) -> Self { Self { address: value.address, mode: value.mode } }
}

fn is_call_edge(block: &BasicBlock, successor: BlockId, cfg: &ControlFlowGraph) -> bool {
    let Some((target, _, _)) = direct_call_target(cfg, block) else { return false; };
    target == successor
}

fn discover_function_blocks(cfg: &ControlFlowGraph, entry: BlockId, function_roots: &HashSet<BlockId>) -> Vec<BlockId> {
    let mut blocks = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([entry]);

    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) { continue; }
        blocks.push(id);
        let block = &cfg.blocks[id.0];
        for &successor in &block.successors {
            if is_call_edge(block, successor, cfg) { continue; }
            if successor != entry && function_roots.contains(&successor) { continue; }
            queue.push_back(successor);
        }
    }
    blocks.sort_by_key(|id| id.0);
    blocks
}

fn function_block_edges(cfg: &ControlFlowGraph, blocks: &[BlockId]) -> HashMap<BlockId, Vec<BlockId>> {
    let block_set = blocks.iter().copied().collect::<HashSet<_>>();
    blocks.iter().copied().map(|block_id| {
        let block = &cfg.blocks[block_id.0];
        let successors = block.successors.iter().copied()
            .filter(|successor| block_set.contains(successor) && !is_call_edge(block, *successor, cfg))
            .collect::<Vec<_>>();
        (block_id, successors)
    }).collect()
}

fn analyze_function_edges(
    cfg: &ControlFlowGraph,
    function_id: FunctionId,
    blocks: &[BlockId],
    block_to_function: &HashMap<BlockId, FunctionId>,
) -> (Vec<CallSite>, Vec<ReturnSite>, Vec<FunctionId>) {
    let mut calls = Vec::new();
    let mut returns = Vec::new();
    let mut successors = HashSet::new();

    for &block_id in blocks {
        let block = &cfg.blocks[block_id.0];
        let Some((instruction_index, instruction)) = block_last_instruction(block) else { continue; };

        if let Some(target) = is_direct_call(instruction) {
            let mode = match instruction.kind {
                InstructionKind::Arm(_) => Mode::Arm,
                InstructionKind::Thumb(_) => Mode::Thumb,
            };
            let target_key = BlockKey { address: target, mode };
            let target_block = block_for_key(cfg, &target_key);
            let return_block = block.successors.iter().copied().find(|successor| Some(*successor) != target_block);
            if let Some(target_block) = target_block {
                if let Some(&callee) = block_to_function.get(&target_block) {
                    if callee != function_id { successors.insert(callee); }
                }
            }
            calls.push(CallSite {
                block: block_id,
                instruction_index,
                target: CallTarget::Direct(target_key),
                return_block,
            });
        } else if let Some(register) = is_indirect_call(instruction) {
            calls.push(CallSite {
                block: block_id,
                instruction_index,
                target: CallTarget::Indirect { register, link: true, mode: instruction.mode },
                return_block: block.successors.first().copied(),
            });
        }

        if is_return(instruction) {
            returns.push(ReturnSite { block: block_id, instruction_index, mode: instruction.mode });
        }
    }

    let mut successors = successors.into_iter().collect::<Vec<_>>();
    successors.sort_by_key(|id| id.0);
    (calls, returns, successors)
}

pub fn discover_functions(program: &Program) -> FunctionControlFlowGraph {
    let cfg = &program.cfg;
    let roots = function_roots(program);
    let root_blocks = roots.iter().map(|(_, block)| *block).collect::<HashSet<_>>();

    let mut block_to_function = HashMap::<BlockId, FunctionId>::new();
    let mut functions = Vec::<Function>::new();

    for (index, (key, entry)) in roots.iter().enumerate() {
        let id = FunctionId(index);
        let blocks = discover_function_blocks(cfg, *entry, &root_blocks);
        let block_successors = function_block_edges(cfg, &blocks);
        for &block in &blocks {
            block_to_function.entry(block).or_insert(id);
        }
        functions.push(Function {
            id,
            key: *key,
            entry: *entry,
            blocks,
            block_successors,
            successors: Vec::new(),
            call_sites: Vec::new(),
            return_sites: Vec::new(),
        });
    }

    for function in &mut functions {
        let (calls, returns, successors) = analyze_function_edges(cfg, function.id, &function.blocks, &block_to_function);
        function.call_sites = calls;
        function.return_sites = returns;
        function.successors = successors;
    }

    let calls = functions.iter().flat_map(|function| function.call_sites.iter().cloned()).collect();
    let returns = functions.iter().flat_map(|function| function.return_sites.iter().copied()).collect();

    FunctionControlFlowGraph {
        entry: FunctionId(0),
        functions,
        block_to_function,
        calls,
        returns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::analyze;
    use crate::decoder::{Mode, ROM_BASE};

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn arm_bl(from: u32, target: u32) -> u32 {
        let delta = (target as i64 - (from as i64 + 8)) as i32;
        0xEB00_0000 | (((delta >> 2) as u32) & 0x00FF_FFFF)
    }

    #[test]
    fn discovers_direct_arm_call_as_function_root() {
        let entry = ROM_BASE;
        let target = ROM_BASE + 8;
        let fallthrough = ROM_BASE + 4;
        let rom = arm_rom(&[arm_bl(entry, target), 0xE1A0_0000, 0xE1A0_0000]);
        let program = analyze(&rom, entry, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions.len(), 2);
        assert_eq!(functions.functions[0].entry, program.cfg.entry);
        assert_eq!(functions.functions[1].key, FunctionKey { address: target, mode: Mode::Arm });

        let caller = &functions.functions[0];
        assert_eq!(caller.blocks.len(), 2);
        assert!(caller.blocks.contains(&program.cfg.entry));
        assert!(caller.blocks.iter().any(|id| program.cfg.blocks[id.0].key.address == fallthrough));
        assert!(!caller.blocks.iter().any(|id| program.cfg.blocks[id.0].key.address == target));

        let callee = &functions.functions[1];
        assert_eq!(callee.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[callee.entry.0].key.address, target);
    }

    #[test]
    fn separates_call_edge_from_fallthrough() {
        let entry = ROM_BASE;
        let target = ROM_BASE + 8;
        let rom = arm_rom(&[arm_bl(entry, target), 0xE1A0_0000, 0xE1A0_0000]);
        let program = analyze(&rom, entry, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let call = &functions.functions[0].call_sites[0];
        assert_eq!(call.target, CallTarget::Direct(BlockKey { address: target, mode: Mode::Arm }));
        assert!(call.return_block.is_some());
        assert_eq!(functions.functions[0].successors, vec![FunctionId(1)]);
    }

    #[test]
    fn recognizes_arm_bx_lr_as_return() {
        let rom = arm_rom(&[0xE12F_FF1E]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.returns.len(), 1);
        assert_eq!(functions.returns[0].mode, Mode::Arm);
    }

    #[test]
    fn keeps_internal_blocks_in_function_cfg() {
        let rom = arm_rom(&[0xE3A0_0001, 0xE280_0001, 0xE1A0_0000]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions.len(), 1);
        assert_eq!(functions.functions[0].blocks.len(), 1);
        assert!(functions.functions[0].block_successors[&BlockId(0)].is_empty());
    }
}
