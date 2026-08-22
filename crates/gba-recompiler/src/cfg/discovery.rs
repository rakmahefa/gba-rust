use std::collections::{HashMap, VecDeque};

use crate::decoder::{DecodeError, Mode};

use super::abstract_state::AbstractState;
use super::edges::{decode_at, in_rom, instruction_successors};
use super::model::{BlockKey, DiscoveredInstruction};

pub(super) fn discover_reachable(
    rom: &[u8],
    entry: BlockKey,
) -> Result<(Vec<BlockKey>, HashMap<BlockKey, DiscoveredInstruction>), DecodeError> {
    let mut order = Vec::new();
    let mut discovered = HashMap::<BlockKey, DiscoveredInstruction>::new();
    let mut states = HashMap::<BlockKey, AbstractState>::new();
    let mut queue = VecDeque::<BlockKey>::new();

    states.insert(entry.clone(), AbstractState::default());
    queue.push_back(entry);

    while let Some(key) = queue.pop_front() {
        let state = states.get(&key).copied().unwrap_or_default();
        let instruction = decode_at(rom, key.clone())?;
        let state_after = super::abstract_state::transfer_instruction(rom, instruction, state);
        let successors = instruction_successors(rom, instruction, state_after);

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
            if !in_rom(rom, successor.address) {
                continue;
            }

            let should_queue = match states.get(&successor).copied() {
                Some(existing) => {
                    let joined = existing.join(state_after);
                    if joined != existing {
                        states.insert(successor.clone(), joined);
                        true
                    } else {
                        edges_changed
                    }
                }
                None => {
                    states.insert(successor.clone(), state_after);
                    true
                }
            };

            if should_queue {
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
