use super::super::types::{ArmExtended, ArmOp, InstructionKind, Operand2, ThumbOp};
use super::{decode_arm, decode_thumb};

#[cfg(test)]
mod tests {
    use super::*;

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
        // 0xE7AF_2558 is a legal ARM single-data-transfer encoding and must
        // no longer be classified as Unknown after the I-bit classification fix.
        assert!(matches!(
            decode_arm(0x0800_0000, 0xE7AF_2558).kind,
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer { .. }))
        ));
        assert!(matches!(
            decode_thumb(0x0800_0000, 0xBE00).kind,
            InstructionKind::Thumb(ThumbOp::Unknown)
        ));
    }
}
