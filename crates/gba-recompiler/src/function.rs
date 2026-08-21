use std::collections::{HashMap, HashSet, VecDeque};

use crate::cfg::{BasicBlock, BlockId, BlockKey, ControlFlowGraph, Program};
use crate::decoder::{ArmOp, Condition, Instruction, InstructionKind, Mode, ThumbOp};

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
    Indirect {
        register: u8,
        link: bool,
        mode: Mode,
    },
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

fn last(block: &BasicBlock) -> Option<(usize, Instruction)> {
    block
        .instructions
        .len()
        .checked_sub(1)
        .map(|i| (i, block.instructions[i]))
}

fn target_key(instruction: Instruction, address: u32) -> BlockKey {
    BlockKey {
        address,
        mode: instruction.mode,
    }
}

fn direct_call(instruction: Instruction) -> Option<u32> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch {
            target, link: true, ..
        }) => Some(target),
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => Some(target),
        _ => None,
    }
}

fn direct_branch(instruction: Instruction) -> Option<u32> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch {
            target,
            link: false,
            ..
        }) => Some(target),
        InstructionKind::Thumb(ThumbOp::Branch {
            target,
            condition: Condition::Al,
        }) => Some(target),
        _ => None,
    }
}

fn indirect_call(instruction: Instruction) -> Option<u8> {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link: true }) => Some(rm),
        _ => None,
    }
}

fn is_return(instruction: Instruction) -> bool {
    matches!(
        instruction.kind,
        InstructionKind::Arm(ArmOp::BranchExchange {
            rm: 14,
            link: false
        }) | InstructionKind::Thumb(ThumbOp::BranchExchange { rm: 14 })
    )
}

fn block_for_key(cfg: &ControlFlowGraph, key: &BlockKey) -> Option<BlockId> {
    cfg.blocks
        .iter()
        .find(|block| block.key == *key)
        .map(|block| block.id)
}

fn roots(program: &Program) -> Vec<(FunctionKey, BlockId)> {
    let cfg = &program.cfg;
    let entry_key = cfg.blocks[cfg.entry.0].key.clone();
    let mut result = vec![(
        FunctionKey {
            address: entry_key.address,
            mode: entry_key.mode,
        },
        cfg.entry,
    )];
    let mut seen = HashSet::from([cfg.entry]);
    for block in &cfg.blocks {
        if let Some((_, instruction)) = last(block) {
            if let Some(address) = direct_call(instruction) {
                let key = target_key(instruction, address);
                if let Some(id) = block_for_key(cfg, &key) {
                    if seen.insert(id) {
                        result.push((
                            FunctionKey {
                                address,
                                mode: key.mode,
                            },
                            id,
                        ));
                    }
                }
            }
        }
    }
    result.sort_by_key(|(_, id)| id.0);
    result
}

fn is_direct_call_edge(cfg: &ControlFlowGraph, block: &BasicBlock, successor: BlockId) -> bool {
    last(block)
        .and_then(|(_, i)| direct_call(i))
        .and_then(|address| block_for_key(cfg, &target_key(block.instructions[0], address)))
        .map(|id| id == successor)
        .unwrap_or(false)
}

fn discover_blocks(
    cfg: &ControlFlowGraph,
    entry: BlockId,
    roots: &HashSet<BlockId>,
    claimed: &HashSet<BlockId>,
) -> Vec<BlockId> {
    let mut queue = VecDeque::from([entry]);
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) || (id != entry && claimed.contains(&id)) {
            continue;
        }
        result.push(id);
        let block = &cfg.blocks[id.0];
        for &successor in &block.successors {
            if is_direct_call_edge(cfg, block, successor) {
                continue;
            }
            if id != entry && roots.contains(&successor) {
                continue;
            }
            if roots.contains(&successor) {
                continue;
            }
            if claimed.contains(&successor) {
                continue;
            }
            queue.push_back(successor);
        }
    }
    result.sort_by_key(|id| id.0);
    result
}

fn block_edges(
    cfg: &ControlFlowGraph,
    blocks: &[BlockId],
    roots: &HashSet<BlockId>,
) -> HashMap<BlockId, Vec<BlockId>> {
    let owned = blocks.iter().copied().collect::<HashSet<_>>();
    blocks
        .iter()
        .copied()
        .map(|id| {
            let block = &cfg.blocks[id.0];
            let successors = block
                .successors
                .iter()
                .copied()
                .filter(|successor| {
                    owned.contains(successor)
                        && !is_direct_call_edge(cfg, block, *successor)
                        && !roots.contains(successor)
                })
                .collect();
            (id, successors)
        })
        .collect()
}

fn make_functions(
    program: &Program,
) -> (
    Vec<Function>,
    HashMap<BlockId, FunctionId>,
    HashSet<BlockId>,
) {
    let cfg = &program.cfg;
    let root_list = roots(program);
    let root_set = root_list.iter().map(|(_, id)| *id).collect::<HashSet<_>>();
    let mut claimed = HashSet::new();
    let mut mapping = HashMap::new();
    let mut functions = Vec::new();

    for (index, (key, entry)) in root_list.iter().enumerate() {
        let id = FunctionId(index);
        let blocks = discover_blocks(cfg, *entry, &root_set, &claimed);
        for block in &blocks {
            claimed.insert(*block);
            mapping.insert(*block, id);
        }
        let edges = block_edges(cfg, &blocks, &root_set);
        functions.push(Function {
            id,
            key: *key,
            entry: *entry,
            blocks,
            block_successors: edges,
            successors: Vec::new(),
            call_sites: Vec::new(),
            return_sites: Vec::new(),
        });
    }
    (functions, mapping, root_set)
}

pub fn discover_functions(program: &Program) -> FunctionControlFlowGraph {
    let cfg = &program.cfg;
    let (mut functions, block_to_function, root_set) = make_functions(program);
    let return_map = {
        let mut map = HashMap::new();
        for function in &mut functions {
            for &block_id in &function.blocks {
                if let Some((instruction_index, instruction)) = last(&cfg.blocks[block_id.0]) {
                    if is_return(instruction) {
                        function.return_sites.push(ReturnSite {
                            block: block_id,
                            instruction_index,
                            mode: instruction.mode,
                        });
                    }
                }
            }
            map.insert(function.id, function.return_sites.clone());
        }
        map
    };

    for function in &mut functions {
        let mut successors = HashSet::new();
        let mut calls = Vec::new();
        for &block_id in &function.blocks {
            let block = &cfg.blocks[block_id.0];
            let Some((instruction_index, instruction)) = last(block) else {
                continue;
            };

            if let Some(address) = direct_call(instruction) {
                let key = target_key(instruction, address);
                if let Some(target_block) = block_for_key(cfg, &key) {
                    if let Some(&callee) = block_to_function.get(&target_block) {
                        successors.insert(callee);
                        let return_block = block
                            .successors
                            .iter()
                            .copied()
                            .find(|id| *id != target_block);
                        calls.push(CallSite {
                            block: block_id,
                            instruction_index,
                            target: CallTarget::Direct(key),
                            return_block,
                            return_sites: return_map.get(&callee).cloned().unwrap_or_default(),
                        });
                    }
                }
                continue;
            }

            if let Some(register) = indirect_call(instruction) {
                calls.push(CallSite {
                    block: block_id,
                    instruction_index,
                    target: CallTarget::Indirect {
                        register,
                        link: true,
                        mode: instruction.mode,
                    },
                    return_block: block.successors.first().copied(),
                    return_sites: Vec::new(),
                });
                continue;
            }

            if let Some(address) = direct_branch(instruction) {
                let key = target_key(instruction, address);
                if let Some(target_block) = block_for_key(cfg, &key) {
                    if root_set.contains(&target_block)
                        && !function
                            .block_successors
                            .get(&block_id)
                            .is_some_and(|edges| edges.contains(&target_block))
                    {
                        if let Some(&callee) = block_to_function.get(&target_block) {
                            successors.insert(callee);
                            calls.push(CallSite {
                                block: block_id,
                                instruction_index,
                                target: CallTarget::TailDirect(key),
                                return_block: None,
                                return_sites: return_map.get(&callee).cloned().unwrap_or_default(),
                            });
                        }
                    }
                }
            }
        }
        calls.sort_by_key(|call| (call.block.0, call.instruction_index));
        calls.dedup_by(|a, b| a.block == b.block && a.instruction_index == b.instruction_index);
        let mut successor_list = successors.into_iter().collect::<Vec<_>>();
        successor_list.sort_by_key(|id| id.0);
        function.call_sites = calls;
        function.successors = successor_list;
    }

    let calls = functions
        .iter()
        .flat_map(|f| f.call_sites.iter().cloned())
        .collect();
    let returns = functions
        .iter()
        .flat_map(|f| f.return_sites.iter().copied())
        .collect();
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
    fn arm_branch(from: u32, target: u32, link: bool) -> u32 {
        let delta = (target as i64 - (from as i64 + 8)) as i32;
        0xEA00_0000 | ((link as u32) << 24) | (((delta >> 2) as u32) & 0x00FF_FFFF)
    }

    #[test]
    fn discovers_direct_arm_call_as_function_root() {
        let target = ROM_BASE + 8;
        let program = analyze(
            &arm_rom(&[arm_branch(ROM_BASE, target, true), 0xE1A0_0000, 0xE1A0_0000]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let graph = discover_functions(&program);
        assert_eq!(graph.functions.len(), 2);
        assert_eq!(
            graph.functions[1].key,
            FunctionKey {
                address: target,
                mode: Mode::Arm
            }
        );
        assert!(!graph.functions[0]
            .blocks
            .contains(&graph.functions[1].entry));
    }

    #[test]
    fn recursive_call_keeps_self_edge_and_return_association() {
        let program = analyze(
            &arm_rom(&[arm_branch(ROM_BASE, ROM_BASE, true), 0xE12F_FF1E]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let graph = discover_functions(&program);
        assert_eq!(graph.functions.len(), 1);
        assert_eq!(graph.functions[0].successors, vec![FunctionId(0)]);
        assert_eq!(graph.functions[0].call_sites[0].return_sites.len(), 1);
    }

    #[test]
    fn multiple_callers_share_one_callee_node() {
        let a = ROM_BASE + 8;
        let callee = ROM_BASE + 16;
        let program = analyze(
            &arm_rom(&[
                arm_branch(ROM_BASE, a, true),
                0xE1A0_0000,
                arm_branch(a, callee, true),
                0xE12F_FF1E,
                0xE12F_FF1E,
            ]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let graph = discover_functions(&program);
        assert_eq!(graph.functions.len(), 3);
        assert_eq!(graph.functions[1].successors, vec![FunctionId(2)]);
        assert_eq!(graph.functions[2].return_sites.len(), 1);
    }

    #[test]
    fn tail_call_is_explicit_and_has_no_return_block() {
        let target = ROM_BASE + 8;
        let program = analyze(
            &arm_rom(&[
                arm_branch(ROM_BASE, target, true),
                arm_branch(ROM_BASE + 4, target, false),
                0xE12F_FF1E,
            ]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let graph = discover_functions(&program);
        assert!(graph.functions[0]
            .call_sites
            .iter()
            .any(|call| matches!(call.target, CallTarget::TailDirect(_))
                && call.return_block.is_none()));
    }

    #[test]
    fn indirect_call_preserves_return_continuation() {
        let program = analyze(&arm_rom(&[0xE12F_FF31, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        let graph = discover_functions(&program);
        assert!(matches!(
            graph.functions[0].call_sites[0].target,
            CallTarget::Indirect { register: 1, .. }
        ));
        assert_eq!(
            graph.functions[0].call_sites[0].return_block,
            Some(BlockId(1))
        );
    }

    #[test]
    fn function_blocks_are_disjoint() {
        let target = ROM_BASE + 8;
        let program = analyze(
            &arm_rom(&[arm_branch(ROM_BASE, target, true), 0xE1A0_0000, 0xE12F_FF1E]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let graph = discover_functions(&program);
        let mut seen = HashSet::new();
        for function in &graph.functions {
            for block in &function.blocks {
                assert!(seen.insert(*block));
            }
        }
    }

    #[test]
    fn return_site_is_associated_with_direct_callee() {
        let target = ROM_BASE + 8;
        let program = analyze(
            &arm_rom(&[arm_branch(ROM_BASE, target, true), 0xE1A0_0000, 0xE12F_FF1E]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let graph = discover_functions(&program);
        assert_eq!(
            graph.functions[0].call_sites[0].return_block,
            Some(BlockId(1))
        );
        assert_eq!(
            graph.functions[0].call_sites[0].return_sites,
            graph.functions[1].return_sites
        );
    }

    #[test]
    fn thumb_bl_creates_thumb_function() {
        let bytes = vec![0x00, 0xF0, 0x00, 0xF8, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        let graph = discover_functions(&program);
        assert_eq!(graph.functions.len(), 2);
        assert_eq!(graph.functions[1].key.mode, Mode::Thumb);
    }
}
