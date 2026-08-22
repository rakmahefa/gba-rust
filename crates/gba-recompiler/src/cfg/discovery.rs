use std::collections::{HashMap, VecDeque};

use crate::address_space::ImageMapping;
use crate::decoder::{DecodeError, Mode};

use super::abstract_state::AbstractState;
use super::edges::{decode_at, in_image, instruction_successors};
use super::model::{BlockKey, DiscoveredInstruction};

const DEBUG_ABSTRACT_STATE_START: u32 = 0x0000_0100;
const DEBUG_ABSTRACT_STATE_END: u32 = 0x0000_0118;

fn debug_address(key: &BlockKey) -> bool {
    (DEBUG_ABSTRACT_STATE_START..=DEBUG_ABSTRACT_STATE_END).contains(&key.address)
}

fn debug_abstract_state(
    key: &BlockKey,
    state_before: AbstractState,
    state_after: AbstractState,
    successors: &[BlockKey],
) {
    if !debug_address(key) {
        return;
    }

    eprintln!(
        "[cfg-debug] instruction-state: address={:#010x} mode={:?}\n    state_before={state_before:?}\n    state_after ={state_after:?}\n    successors  ={successors:?}",
        key.address, key.mode
    );
}

fn debug_incoming_edge(
    source: &BlockKey,
    successor: &BlockKey,
    state_after: AbstractState,
    existing: Option<AbstractState>,
    joined: AbstractState,
    enqueued: bool,
) {
    if successor.address != DEBUG_ABSTRACT_STATE_END {
        return;
    }

    eprintln!(
        "[cfg-debug] incoming edge:\n    source={:#010x}/{:?}\n    target={:#010x}/{:?}\n    source_state_after={state_after:?}\n    target_state_before={existing:?}\n    joined_state={joined:?}\n    decision={}",
        source.address,
        source.mode,
        successor.address,
        successor.mode,
        if enqueued { "enqueue" } else { "skip" }
    );
}

pub(super) fn discover_reachable(
    rom: &[u8],
    entry: BlockKey,
    mapping: ImageMapping,
) -> Result<(Vec<BlockKey>, HashMap<BlockKey, DiscoveredInstruction>), DecodeError> {
    let mut order = Vec::new();
    let mut discovered = HashMap::<BlockKey, DiscoveredInstruction>::new();
    let mut states = HashMap::<BlockKey, AbstractState>::new();
    let mut queue = VecDeque::<BlockKey>::new();

    states.insert(entry.clone(), AbstractState::default());
    queue.push_back(entry);

    while let Some(key) = queue.pop_front() {
        let state = states.get(&key).copied().unwrap_or_default();
        let instruction = decode_at(rom, key.clone(), mapping)?;
        let state_after =
            super::abstract_state::transfer_instruction(rom, instruction, state, mapping);
        let successors = instruction_successors(rom, instruction, state_after, mapping);

        debug_abstract_state(&key, state, state_after, &successors);

        if !discovered.contains_key(&key) {
            order.push(key.clone());
        }

        let previous = discovered.insert(
            key.clone(),
            DiscoveredInstruction {
                instruction,
                successors: successors.clone(),
            },
        );
        let edges_changed = previous
            .as_ref()
            .map(|node| node.successors != successors)
            .unwrap_or(true);

        for successor in successors {
            if !in_image(mapping, successor.address) {
                continue;
            }

            let existing = states.get(&successor).copied();
            let (joined, should_queue) = match existing {
                Some(existing) => {
                    let joined = existing.join(state_after);
                    let changed = joined != existing;
                    (joined, changed || edges_changed)
                }
                None => (state_after, true),
            };

            debug_incoming_edge(
                &key,
                &successor,
                state_after,
                existing,
                joined,
                should_queue,
            );

            if should_queue {
                states.insert(successor.clone(), joined);
                queue.push_back(successor);
            }
        }
    }

    Ok((order, discovered))
}

pub(super) fn sort_mode(mode: Mode) -> u8 {
    match mode {
        Mode::Arm => 0,
        Mode::Thumb => 1,
    }
}
