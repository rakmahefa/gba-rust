use super::arm;
use super::classification::{classify_arm, classify_thumb, ArmClass, ThumbClass};
use super::common::arm_condition;
use super::thumb;
use super::types::{ArmOp, Condition, Instruction, InstructionKind, Mode, ThumbOp};

/// Decode an ARM instruction after opcode-space classification.
///
/// Classification is deliberately kept independent from operand extraction and
/// semantic construction. The existing ARM implementation remains the semantic
/// backend during this migration; this boundary makes the separation explicit
/// and gives us a stable place for per-family semantic decoders to move into.
pub fn decode_arm(address: u32, raw: u32) -> Instruction {
    let class = classify_arm(raw);
    decode_arm_classified(address, raw, class)
}

fn decode_arm_classified(address: u32, raw: u32, class: ArmClass) -> Instruction {
    if matches!(class, ArmClass::Unknown) {
        return unknown_arm(address, raw);
    }

    let instruction = arm::decode_arm(address, raw);
    debug_assert!(
        !matches!(instruction.kind, InstructionKind::Arm(ArmOp::Unknown)),
        "classified ARM opcode {class:?} decoded as unknown: {raw:#010x}"
    );
    instruction
}

/// Decode a Thumb instruction after opcode-space classification.
pub fn decode_thumb(address: u32, raw: u16) -> Instruction {
    let class = classify_thumb(raw);
    decode_thumb_classified(address, raw, class)
}

fn decode_thumb_classified(address: u32, raw: u16, class: ThumbClass) -> Instruction {
    if matches!(class, ThumbClass::Unknown) {
        return unknown_thumb(address, raw);
    }

    let instruction = thumb::decode_thumb(address, raw);
    debug_assert!(
        !matches!(instruction.kind, InstructionKind::Thumb(ThumbOp::Unknown)),
        "classified Thumb opcode {class:?} decoded as unknown: {raw:#06x}"
    );
    instruction
}

pub fn decode_thumb_bl(address: u32, first: u16, second: u16) -> Instruction {
    thumb::decode_thumb_bl(address, first, second)
}

fn unknown_arm(address: u32, raw: u32) -> Instruction {
    Instruction {
        address,
        mode: Mode::Arm,
        raw,
        size: 4,
        condition: arm_condition(raw),
        kind: InstructionKind::Arm(ArmOp::Unknown),
    }
}

fn unknown_thumb(address: u32, raw: u16) -> Instruction {
    Instruction {
        address,
        mode: Mode::Thumb,
        raw: raw as u32,
        size: 2,
        condition: Condition::Al,
        kind: InstructionKind::Thumb(ThumbOp::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{ArmExtended, Operand2};

    #[test]
    fn semantic_arm_path_preserves_existing_decode() {
        let instruction = decode_arm(0x0800_0000, 0xE281_2004);
        assert_eq!(
            instruction.kind,
            InstructionKind::Arm(ArmOp::Add {
                rd: 2,
                rn: 1,
                op2: Operand2::Imm(4),
            })
        );
    }

    #[test]
    fn semantic_arm_path_keeps_extended_memory_ops() {
        let instruction = decode_arm(0x0800_0000, 0xE1D1_20B0);
        assert!(matches!(
            instruction.kind,
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::HalfwordTransfer { .. }))
        ));
    }

    #[test]
    fn semantic_unknown_is_explicit() {
        assert!(matches!(
            decode_arm(0x0800_0000, 0xE7AF_2558).kind,
            InstructionKind::Arm(ArmOp::Unknown)
        ));
        assert!(matches!(
            decode_thumb(0x0800_0000, 0xBE00).kind,
            InstructionKind::Thumb(ThumbOp::Unknown)
        ));
    }
}
