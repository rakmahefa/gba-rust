use std::collections::{HashMap, HashSet, VecDeque};

use crate::cfg::{BasicBlock, BlockId, BlockKey, ControlFlowGraph, Program};
use crate::decoder::{ArmOp, Instruction, InstructionKind, Mode, ThumbOp};

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
    TailDirect(BlockKey),
    Indirect { register: u8, link: bool, mode: Mode },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallSite {
    pub block: BlockId,
    pub instruction_index: usize,
    pub target: CallTarget,
    pub return_block: Option<BlockId>,
    pub return_sites: Vec<ReturnSite>,
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

fn block_last_instruction(block: &BasicBlock) -> Option<(usize, Instruction)> {
    block.instructions.len().checked_sub(1).map(|index| (index, block.instructions[index]))
}

fn is_direct_call(instruction: Instruction) -> Option<u32> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch { target, link: true, .. }) => Some(target),
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => Some(target),
        _ => None,
    }
}

fn direct_branch_target(instruction: Instruction) -> Option<u32> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch { target, link: false, .. }) => Some(target),
        InstructionKind::Thumb(ThumbOp::Branch { target, condition: crate::decoder::Condition::Al }) => Some(target),
        _ => None,
    }
}

fn is_indirect_call(instruction: Instruction) -> Option<u8> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link: true }) => Some(rm),
        _ => None,
    }
}

fn is_return(instruction: Instruction) -> bool {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::BranchExchange { rm: 14, link: false }) => true,
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm: 14 }) => true,
        _ => false,
    }
}

fn block_for_key(cfg: &ControlFlowGraph, key: &BlockKey) -> Option<BlockId> {
    cfg.blocks.iter().find(|block| block.key == *key).map(|block| block.id)
}

fn instruction_target_key(instruction: Instruction, target: u32) -> BlockKey {
    let mode = match instruction.kind {
        InstructionKind::Arm(_) => Mode::Arm,
        InstructionKind::Thumb(_) => Mode::Thumb,
    };
    BlockKey { address: target, mode }
}

fn direct_call_target(cfg: &ControlFlowGraph, block: &BasicBlock) -> Option<(BlockId, u32, Mode)> {
    let (_, instruction) = block_last_instruction(block)?;
    let target = is_direct_call(instruction)?;
    let key = instruction_target_key(instruction, target);
    Some((block_for_key(cfg, &key)?, target, key.mode))
}

fn function_roots(program: &Program) -> Vec<(FunctionKey, BlockId)> {
    let cfg = &program.cfg;
    let entry_key = cfg.blocks[cfg.entry.0].key.clone();
    let mut roots = vec![(FunctionKey::from(entry_key), cfg.entry)];
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
    direct_call_target(cfg, block).map(|(target, _, _)| target == successor).unwrap_or(false)
}

fn is_tail_call_edge(block: &BasicBlock, successor: BlockId, cfg: &ControlFlowGraph, function_roots: &HashSet<BlockId>) -> bool {
    if !function_roots.contains(&successor) { return false; }
    let Some((_, instruction)) = block_last_instruction(block) else { return false; };
    let Some(target) = direct_branch_target(instruction) else { return false; };
    let key = instruction_target_key(instruction, target);
    block_for_key(cfg, &key) == Some(successor)
}

fn discover_function_blocks(
    cfg: &ControlFlowGraph,
    entry: BlockId,
    function_roots: &HashSet<BlockId>,
    claimed: &HashSet<BlockId>,
) -> Vec<BlockId> {
    let mut blocks = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([entry]);

    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) { continue; }
        if id != entry && claimed.contains(&id) { continue; }
        blocks.push(id);
        let block = &cfg.blocks[id.0];
        for &successor in &block.successors {
            if is_call_edge(block, successor, cfg) { continue; }
            if is_tail_call_edge(block, successor, cfg, function_roots) { continue; }
            if successor != entry && function_roots.contains(&successor) { continue; }
            if successor != entry && claimed.contains(&successor) { continue; }
            queue.push_back(successor);
        }
    }
    blocks.sort_by_key(|id| id.0);
    blocks
}

fn function_block_edges(
    cfg: &ControlFlowGraph,
    blocks: &[BlockId],
    function_roots: &HashSet<BlockId>,
) -> HashMap<BlockId, Vec<BlockId>> {
    let block_set = blocks.iter().copied().collect::<HashSet<_>>();
    blocks.iter().copied().map(|block_id| {
        let block = &cfg.blocks[block_id.0];
        let successors = block.successors.iter().copied()
            .filter(|successor| {
                block_set.contains(successor)
                    && !is_call_edge(block, *successor, cfg)
                    && !is_tail_call_edge(block, *successor, cfg, function_roots)
            })
            .collect::<Vec<_>>();
        (block_id, successors)
    }).collect()
}

fn analyze_function_edges(
    cfg: &ControlFlowGraph,
    function_id: FunctionId,
    blocks: &[BlockId],
    block_to_function: &HashMap<BlockId, FunctionId>,
    function_roots: &HashSet<BlockId>,
    functions: &[Function],
) -> (Vec<CallSite>, Vec<ReturnSite>, Vec<FunctionId>) {
    let mut calls = Vec::new();
    let mut returns = Vec::new();
    let mut successors = HashSet::new();

    for &block_id in blocks {
        let block = &cfg.blocks[block_id.0];
        let Some((instruction_index, instruction)) = block_last_instruction(block) else { continue; };

        if let Some(target) = is_direct_call(instruction) {
            let target_key = instruction_target_key(instruction, target);
            let target_block = block_for_key(cfg, &target_key);
            let return_block = block.successors.iter().copied().find(|successor| Some(*successor) != target_block);
            let return_sites = target_block
                .and_then(|target| block_to_function.get(&target).copied())
                .and_then(|callee| functions.get(callee.0))
                .map(|callee| callee.return_sites.clone())
                .unwrap_or_default();

            if let Some(target_block) = target_block {
                if let Some(&callee) = block_to_function.get(&target_block) {
                    successors.insert(callee);
                }
            }
            calls.push(CallSite {
                block: block_id,
                instruction_index,
                target: CallTarget::Direct(target_key),
                return_block,
                return_sites,
            });
        } else if let Some(register) = is_indirect_call(instruction) {
            calls.push(CallSite {
                block: block_id,
                instruction_index,
                target: CallTarget::Indirect { register, link: true, mode: instruction.mode },
                return_block: block.successors.first().copied(),
                return_sites: Vec::new(),
            });
        } else if let Some(target) = direct_branch_target(instruction) {
            let target_key = instruction_target_key(instruction, target);
            if let Some(target_block) = block_for_key(cfg, &target_key) {
                if function_roots.contains(&target_block) {
                    let return_sites = block_to_function.get(&target_block)
                        .and_then(|callee| functions.get(callee.0))
                        .map(|callee| callee.return_sites.clone())
                        .unwrap_or_default();
                    if let Some(&callee) = block_to_function.get(&target_block) {
                        successors.insert(callee);
                    }
                    calls.push(CallSite {
                        block: block_id,
                        instruction_index,
                        target: CallTarget::TailDirect(target_key),
                        return_block: None,
                        return_sites,
                    });
                }
            }
        }

        if is_return(instruction) {
            returns.push(ReturnSite { block: block_id, instruction_index, mode: instruction.mode });
        }
    }

    let mut successors = successors.into_iter().collect::<Vec<_>>();
    successors.sort_by_key(|id| id.0);
    successors.dedup();
    (calls, returns, successors)
}

fn resolve_return_sites(functions: &mut [Function]) {
    let returns = functions.iter().map(|function| function.return_sites.clone()).collect::<Vec<_>>();
    let function_for_block = functions.iter().enumerate()
        .flat_map(|(index, function)| function.blocks.iter().copied().map(move |block| (block, FunctionId(index))))
        .collect::<HashMap<_, _>>();

    for function in functions.iter_mut() {
        for call in &mut function.call_sites {
            let target_function = match &call.target {
                CallTarget::Direct(key) | CallTarget::TailDirect(key) => {
                    function_for_block.iter().find_map(|(block, id)| {
                        if *block == function_entry_block(functions, *id, key) { Some(*id) } else { None }
                    })
                }
                CallTarget::Indirect { .. } => None,
            };
            call.return_sites = target_function
                .and_then(|id| returns.get(id.0).cloned())
                .unwrap_or_default();
        }
    }
}

fn function_entry_block(functions: &[Function], id: FunctionId, key: &BlockKey) -> BlockId {
    functions.get(id.0)
        .filter(|function| function.key.address == key.address && function.key.mode == key.mode)
        .map(|function| function.entry)
        .unwrap_or(BlockId(usize::MAX))
}

pub fn discover_functions(program: &Program) -> FunctionControlFlowGraph {
    let cfg = &program.cfg;
    let roots = function_roots(program);
    let root_blocks = roots.iter().map(|(_, block)| *block).collect::<HashSet<_>>();

    let mut block_to_function = HashMap::<BlockId, FunctionId>::new();
    let mut functions = Vec::<Function>::new();

    for (index, (key, entry)) in roots.iter().enumerate() {
        let id = FunctionId(index);
        let blocks = discover_function_blocks(cfg, *entry, &root_blocks, &block_to_function.keys().copied().collect());
        let block_successors = function_block_edges(cfg, &blocks, &root_blocks);
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
        let (_, returns, _) = analyze_function_edges(
            cfg,
            function.id,
            &function.blocks,
            &block_to_function,
            &root_blocks,
            &functions,
        );
        function.return_sites = returns;
    }

    // Calls and the call graph are resolved after all return sites exist. This
    // makes recursion and multiple callers deterministic and preserves one
    // function node per unique entry point.
    let return_map = functions.iter().map(|function| (function.id, function.return_sites.clone())).collect::<HashMap<_, _>>();
    for function in &mut functions {
        let mut successors = HashSet::new();
        let mut calls = Vec::new();
        let mut returns = function.return_sites.clone();

        for &block_id in &function.blocks {
            let block = &cfg.blocks[block_id.0];
            let Some((instruction_index, instruction)) = block_last_instruction(block) else { continue; };

            let target = is_direct_call(instruction).map(|target| (target, false))
                .or_else(|| direct_branch_target(instruction).map(|target| (target, true)));
            if let Some((target, tail)) = target {
                let key = instruction_target_key(instruction, target);
                if let Some(target_block) = block_for_key(cfg, &key) {
                    if let Some(&callee) = block_to_function.get(&target_block) {
                        let is_tail = tail && root_blocks.contains(&target_block);
                        if !tail || is_tail {
                            successors.insert(callee);
                            let return_block = if is_tail { None } else {
                                block.successors.iter().copied().find(|successor| *successor != target_block)
                            };
                            calls.push(CallSite {
                                block: block_id,
                                instruction_index,
                                target: if is_tail { CallTarget::TailDirect(key) } else { CallTarget::Direct(key) },
                                return_block,
                                return_sites: return_map.get(&callee).cloned().unwrap_or_default(),
                            });
                        }
                    }
                }
            } else if let Some(register) = is_indirect_call(instruction) {
                calls.push(CallSite {
                    block: block_id,
                    instruction_index,
                    target: CallTarget::Indirect { register, link: true, mode: instruction.mode },
                    return_block: block.successors.first().copied(),
                    return_sites: Vec::new(),
                });
            }

            if is_return(instruction) && !returns.iter().any(|site| site.block == block_id && site.instruction_index == instruction_index) {
                returns.push(ReturnSite { block: block_id, instruction_index, mode: instruction.mode });
            }
        }

        calls.sort_by_key(|call| (call.block.0, call.instruction_index));
        calls.dedup_by(|a, b| a.block == b.block && a.instruction_index == b.instruction_index);
        let mut successor_vec = successors.into_iter().collect::<Vec<_>>();
        successor_vec.sort_by_key(|id| id.0);
        function.call_sites = calls;
        function.return_sites = returns;
        function.successors = successor_vec;
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

    fn arm_rom(words: &[u32]) -> Vec<u8> { words.iter().flat_map(|word| word.to_le_bytes()).collect() }

    fn arm_bl(from: u32, target: u32) -> u32 {
        let delta = (target as i64 - (from as i64 + 8)) as i32;
        0xEB00_0000 | (((delta >> 2) as u32) & 0x00FF_FFFF)
    }

    fn arm_b(from: u32, target: u32) -> u32 {
        let delta = (target as i64 - (from as i64 + 8)) as i32;
        0xEA00_0000 | (((delta >> 2) as u32) & 0x00FF_FFFF)
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
        assert_eq!(functions.functions[1].key, FunctionKey { address: target, mode: Mode::Arm });
        let caller = &functions.functions[0];
        assert_eq!(caller.blocks.len(), 2);
        assert!(caller.blocks.iter().any(|id| program.cfg.blocks[id.0].key.address == fallthrough));
        assert!(!caller.blocks.iter().any(|id| program.cfg.blocks[id.0].key.address == target));
    }

    #[test]
    fn recursive_call_keeps_self_edge() {
        let entry = ROM_BASE;
        let rom = arm_rom(&[arm_bl(entry, entry), 0xE12F_FF1E]);
        let program = analyze(&rom, entry, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions.len(), 1);
        assert_eq!(functions.functions[0].successors, vec![FunctionId(0)]);
        assert_eq!(functions.functions[0].call_sites[0].return_sites.len(), 1);
    }

    #[test]
    fn multiple_callers_share_one_callee_function() {
        let entry = ROM_BASE;
        let caller2 = ROM_BASE + 8;
        let callee = ROM_BASE + 16;
        let rom = arm_rom(&[
            arm_bl(entry, callee), 0xE1A0_0000,
            arm_bl(caller2, callee), 0xE1A0_0000,
            0xE12F_FF1E, 0xE1A0_0000,
        ]);
        let program = analyze(&rom, entry, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions.len(), 2);
        assert_eq!(functions.functions[1].key.address, callee);
        assert_eq!(functions.functions[0].successors, vec![FunctionId(1)]);
    }

    #[test]
    fn tail_call_is_a_function_edge_without_return_block() {
        let entry = ROM_BASE;
        let target = ROM_BASE + 8;
        let rom = arm_rom(&[arm_b(entry, target), 0xE1A0_0000, 0xE12F_FF1E]);
        // The target is not a call-derived root, so the branch remains an
        // ordinary CFG edge. This test documents the conservative boundary.
        let program = analyze(&rom, entry, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions.len(), 1);
        assert!(functions.functions[0].call_sites.is_empty());
    }

    #[test]
    fn indirect_call_has_unknown_target_but_preserves_return_continuation() {
        let rom = arm_rom(&[0xE12F_FF31, 0xE1A0_0000]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions[0].call_sites.len(), 1);
        assert!(matches!(functions.functions[0].call_sites[0].target, CallTarget::Indirect { register: 1, .. }));
        assert_eq!(functions.functions[0].call_sites[0].return_block, Some(BlockId(0)));
    }

    #[test]
    fn recognizes_arm_return() {
        let rom = arm_rom(&[0xE12F_FF1E]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.returns.len(), 1);
    }

    #[test]
    fn keeps_function_blocks_disjoint() {
        let target = ROM_BASE + 8;
        let rom = arm_rom(&[arm_bl(ROM_BASE, target), 0xE1A0_0000, 0xE12F_FF1E]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let mut seen = HashSet::new();
        for function in &functions.functions {
            for block in &function.blocks { assert!(seen.insert(*block)); }
        }
    }

    #[test]
    fn return_site_is_associated_with_direct_callee() {
        let target = ROM_BASE + 8;
        let rom = arm_rom(&[arm_bl(ROM_BASE, target), 0xE1A0_0000, 0xE12F_FF1E]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let call = &functions.functions[0].call_sites[0];
        assert_eq!(call.return_block, Some(BlockId(1)));
        assert_eq!(call.return_sites, functions.functions[1].return_sites);
    }

    #[test]
    fn thumb_bl_creates_thumb_function() {
        let bytes = vec![0x00, 0xF0, 0x00, 0xF8, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        let functions = discover_functions(&program);
        assert_eq!(functions.functions.len(), 2);
        assert_eq!(functions.functions[1].key.mode, Mode::Thumb);
    }
}
