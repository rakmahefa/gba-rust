use crate::decoder::{
    ArmExtended, ArmOp, Condition, DecodeError, Instruction, InstructionKind, Mode,
    ThumbExtended, ThumbOp,
};

use super::abstract_state::{resolved_exchange_target, AbstractState};
use super::model::BlockKey;

pub(super) fn next_key(instruction: Instruction) -> BlockKey {
    BlockKey {
        address: instruction
            .address
            .wrapping_add(instruction.size as u32),
        mode: instruction.mode,
    }
}

pub(super) fn in_rom(rom: &[u8], address: u32) -> bool {
    address >= crate::decoder::ROM_BASE
        && address - crate::decoder::ROM_BASE < rom.len() as u32
}

/// Whether an instruction is architecturally a basic-block control boundary.
pub(super) fn is_control_boundary(instruction: Instruction) -> bool {
    matches!(
        instruction.kind,
        InstructionKind::Arm(ArmOp::Branch { .. })
            | InstructionKind::Arm(ArmOp::BranchExchange { .. })
            | InstructionKind::Arm(ArmOp::Extended(ArmExtended::SoftwareInterrupt { .. }))
            | InstructionKind::Thumb(ThumbOp::Branch { .. })
            | InstructionKind::Thumb(ThumbOp::BranchLink { .. })
            | InstructionKind::Thumb(ThumbOp::BranchExchange { .. })
            | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::SoftwareInterrupt { .. }))
            | InstructionKind::Arm(ArmOp::Unknown)
            | InstructionKind::Thumb(ThumbOp::Unknown)
    )
}

pub(super) fn instruction_successors(
    rom: &[u8],
    instruction: Instruction,
    state: AbstractState,
) -> Vec<BlockKey> {
    let next = next_key(instruction);
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch {
            target,
            condition,
            link,
        }) => {
            let mut successors = vec![BlockKey {
                address: target,
                mode: Mode::Arm,
            }];
            if condition != Condition::Al || link {
                successors.push(next);
            }
            successors
        }
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link }) => {
            let mut successors = Vec::new();
            if let Some(target) = resolved_exchange_target(state, rm) {
                if in_rom(rom, target.address) {
                    successors.push(target);
                }
            }
            if link {
                successors.push(next);
            }
            successors
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::SoftwareInterrupt { .. }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::SoftwareInterrupt { .. })) => {
            // SWI enters the supervisor exception path but has an architectural
            // return continuation. Keep that continuation in the CFG while
            // forcing a block boundary at the SWI instruction itself.
            vec![next]
        }
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => {
            let mut successors = vec![BlockKey {
                address: target,
                mode: Mode::Thumb,
            }];
            if condition != Condition::Al {
                successors.push(next);
            }
            successors
        }
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => {
            vec![
                BlockKey {
                    address: target,
                    mode: Mode::Thumb,
                },
                next,
            ]
        }
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm }) => resolved_exchange_target(
            state,
            rm,
        )
        .filter(|target| in_rom(rom, target.address))
        .into_iter()
        .collect(),
        InstructionKind::Arm(ArmOp::Unknown) | InstructionKind::Thumb(ThumbOp::Unknown) => {
            Vec::new()
        }
        _ => vec![next],
    }
}

pub(super) fn is_call(instruction: Instruction) -> bool {
    matches!(
        instruction.kind,
        InstructionKind::Arm(ArmOp::Branch { link: true, .. })
            | InstructionKind::Arm(ArmOp::BranchExchange { link: true, .. })
            | InstructionKind::Thumb(ThumbOp::BranchLink { .. })
    )
}

pub(super) fn is_fallthrough(
    instruction: Instruction,
    successors: &[BlockKey],
) -> bool {
    successors.len() == 1
        && successors[0] == next_key(instruction)
        && !is_call(instruction)
        && !is_control_boundary(instruction)
}

pub(super) fn decode_at(
    rom: &[u8],
    key: BlockKey,
) -> Result<Instruction, DecodeError> {
    match key.mode {
        Mode::Arm => Ok(crate::decoder::decode_arm(
            key.address,
            crate::decoder::read_arm(rom, key.address)?,
        )),
        Mode::Thumb => {
            let raw = crate::decoder::read_thumb(rom, key.address)?;
            if (raw & 0xF800) == 0xF000 {
                let (first, second) = crate::decoder::read_thumb_bl(rom, key.address)?;
                Ok(crate::decoder::decode_thumb_bl(key.address, first, second))
            } else {
                Ok(crate::decoder::decode_thumb(key.address, raw))
            }
        }
    }
}
