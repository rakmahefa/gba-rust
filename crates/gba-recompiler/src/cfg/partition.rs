use std::collections::{HashMap, HashSet};

use crate::ir::lower;

use super::edges::{is_fallthrough, next_key};
use super::model::{BasicBlock, BlockId, BlockKey, DiscoveredInstruction};

pub(super) fn collect_leaders(
    discovered: &HashMap<BlockKey, DiscoveredInstruction>,
    entry: &BlockKey,
) -> Vec<BlockKey> {
    let mut leaders = HashSet::<BlockKey>::new();
    leaders.insert(entry.clone());

    for node in discovered.values() {
        if !is_fallthrough(node.instruction, &node.successors) {
            for successor in &node.successors {
                if discovered.contains_key(successor) {
                    leaders.insert(successor.clone());
                }
            }
        }
    }

    let mut leaders = leaders.into_iter().collect::<Vec<_>>();
    leaders.sort_by(|a, b| {
        a.address.cmp(&b.address).then_with(|| {
            super::discovery::sort_mode(a.mode).cmp(&super::discovery::sort_mode(b.mode))
        })
    });
    leaders
}

pub(super) fn partition_blocks(
    discovered: &HashMap<BlockKey, DiscoveredInstruction>,
    leaders: &[BlockKey],
) -> (Vec<BasicBlock>, HashMap<BlockKey, BlockId>) {
    let leader_set = leaders.iter().cloned().collect::<HashSet<_>>();
    let mut blocks = Vec::new();
    let mut ids = HashMap::<BlockKey, BlockId>::new();

    for leader in leaders {
        let id = BlockId(blocks.len());
        ids.insert(leader.clone(), id);

        let mut instructions = Vec::new();
        let mut cursor = leader.clone();
        while let Some(node) = discovered.get(&cursor) {
            instructions.push(node.instruction);
            if !is_fallthrough(node.instruction, &node.successors) {
                break;
            }
            let next = next_key(node.instruction);
            if leader_set.contains(&next) {
                break;
            }
            cursor = next;
        }

        let ir = instructions.iter().copied().map(lower).collect::<Vec<_>>();
        debug_assert_eq!(
            instructions.len(),
            ir.len(),
            "IR must preserve one entry per instruction"
        );

        blocks.push(BasicBlock {
            id,
            key: leader.clone(),
            instructions,
            ir,
            successors: Vec::new(),
        });
    }

    for block in &mut blocks {
        let Some(last) = block.instructions.last().copied() else {
            continue;
        };
        let key = BlockKey {
            address: last.address,
            mode: last.mode,
        };
        let Some(node) = discovered.get(&key) else {
            continue;
        };

        let mut successors = node
            .successors
            .iter()
            .filter_map(|successor| ids.get(successor).copied())
            .collect::<Vec<_>>();
        successors.sort_unstable_by_key(|id| id.0);
        successors.dedup();
        block.successors = successors;
    }

    (blocks, ids)
}
