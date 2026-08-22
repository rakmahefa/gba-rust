use thiserror::Error;

use crate::decoder::{DecodeError, Mode, ROM_BASE};

mod abstract_state;
mod discovery;
mod edges;
mod hardening;
mod model;
mod partition;

pub use hardening::ValidationError;
pub use model::{BasicBlock, BlockId, BlockKey, ControlFlowGraph, Program};

use discovery::discover_reachable;
use edges::in_rom;
use hardening::validate_cfg;
use partition::{collect_leaders, partition_blocks};

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("entry {0:#x} is outside the cartridge ROM")]
    InvalidEntry(u32),
    #[error("CFG invariant violation: {0:?}")]
    InvalidCfg(#[from] ValidationError),
}

pub fn analyze(rom: &[u8], entry: u32, entry_mode: Mode) -> Result<Program, AnalysisError> {
    if !in_rom(rom, entry) {
        return Err(AnalysisError::InvalidEntry(entry));
    }

    let entry_key = BlockKey {
        address: entry,
        mode: entry_mode,
    };
    let (discovered_order, discovered) = discover_reachable(rom, entry_key.clone())?;
    let leaders = collect_leaders(&discovered, &entry_key);
    let (blocks, ids) = partition_blocks(&discovered, &leaders);
    let entry_id = *ids
        .get(&entry_key)
        .ok_or(AnalysisError::InvalidEntry(entry))?;

    let cfg = ControlFlowGraph {
        entry: entry_id,
        blocks,
    };
    validate_cfg(&cfg, discovered_order.len())?;

    Ok(Program {
        entry: entry_id,
        cfg,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn sequential_arm_instructions_remain_in_one_block() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].instructions.len(), 2);
    }

    #[test]
    fn discovers_arm_branch_and_fallthrough() {
        let program = analyze(
            &arm_rom(&[0xEA00_0000, 0xE1A0_0000, 0xE1A0_0000]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].successors, vec![BlockId(1)]);
    }

    #[test]
    fn conditional_branch_splits_fallthrough_and_target() {
        let bytes = vec![0x00, 0xD0, 0xC0, 0x46, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 3);
        assert_eq!(program.cfg.blocks[0].successors.len(), 2);
    }

    #[test]
    fn backward_branch_does_not_overlap_blocks() {
        let program = analyze(&arm_rom(&[0xEAFF_FFFE, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
    }

    #[test]
    fn conditional_backward_branch_preserves_instruction_ownership() {
        let bytes = vec![0x00, 0xD0, 0xC0, 0x46, 0xFC, 0xD0, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        let mut keys = HashSet::new();
        for block in &program.cfg.blocks {
            for instruction in &block.instructions {
                assert!(keys.insert((instruction.address, instruction.mode)));
            }
        }
    }

    #[test]
    fn unknown_instruction_terminates_discovery() {
        let program = analyze(&arm_rom(&[0xFFFFFFFF, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
    }

    #[test]
    fn thumb_bl_is_a_four_byte_call_with_fallthrough() {
        let bytes = vec![0x00, 0xF0, 0x00, 0xF8, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].instructions[0].size, 4);
    }

    #[test]
    fn unresolved_indirect_call_keeps_continuation_block() {
        let program = analyze(&arm_rom(&[0xE12F_FF31, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].successors, vec![BlockId(1)]);
    }

    #[test]
    fn resolves_arm_bx_from_pc_relative_literal() {
        let target = ROM_BASE + 12;
        let bytes = arm_rom(&[0xE59F_0000, 0xE12F_FF10, target, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert!(program
            .cfg
            .blocks
            .iter()
            .any(|block| block.key == BlockKey { address: target, mode: Mode::Arm }));
    }

    #[test]
    fn resolves_thumb_bx_from_pc_relative_literal() {
        let target = ROM_BASE + 8;
        let mut bytes = vec![0x00, 0x48, 0x00, 0x47];
        bytes.extend_from_slice(&target.to_le_bytes());
        bytes.extend_from_slice(&0xE1A0_0000u32.to_le_bytes());
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert!(program
            .cfg
            .blocks
            .iter()
            .any(|block| block.key == BlockKey { address: target, mode: Mode::Arm }));
    }

    #[test]
    fn unresolved_indirect_branch_does_not_invent_a_target() {
        let program = analyze(&arm_rom(&[0xE12F_FF10, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert!(program.cfg.blocks[0].successors.is_empty());
    }
}
