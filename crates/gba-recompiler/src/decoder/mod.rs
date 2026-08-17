mod arm;
mod common;
mod memory;
mod thumb;
pub mod types;

pub use arm::decode_arm;
pub use memory::{read_arm, read_thumb, read_thumb_bl, DecodeError, ROM_BASE};
pub use thumb::{decode_thumb, decode_thumb_bl};
pub use types::{
    ArmDataOp, ArmExtended, ArmOp, BranchKind, Condition, Instruction, InstructionKind, Mode,
    Operand2, ThumbAluOp, ThumbExtended, ThumbOp,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_arm_data_processing_families() {
        for raw in [
            0xE0000000,
            0xE0200000,
            0xE0400000,
            0xE0600000,
            0xE0800000,
            0xE0A00000,
            0xE0C00000,
            0xE0E00000,
            0xE1100000,
            0xE1300000,
            0xE1500000,
            0xE1700000,
            0xE1800000,
            0xE1A00000,
            0xE1C00000,
            0xE1E00000,
        ] {
            assert!(
                !matches!(decode_arm(ROM_BASE, raw).kind, InstructionKind::Arm(ArmOp::Unknown)),
                "raw={raw:#010x}"
            );
        }
    }

    #[test]
    fn decodes_arm_system_and_memory_families() {
        for raw in [
            0xE0000090,
            0xE0800090,
            0xE1000090,
            0xE1400090,
            0xE4000000,
            0xE4800000,
            0xE8000000,
            0xEF000000,
            0xEC000000,
            0xEE000000,
        ] {
            assert!(
                !matches!(decode_arm(ROM_BASE, raw).kind, InstructionKind::Arm(ArmOp::Unknown)),
                "raw={raw:#010x}"
            );
        }
    }

    #[test]
    fn decodes_thumb_major_families() {
        for raw in [
            0x0000,
            0x1800,
            0x2000,
            0x3000,
            0x4000,
            0x4400,
            0x4700,
            0x4800,
            0x5000,
            0x6000,
            0x8000,
            0x9000,
            0xA000,
            0xB000,
            0xB400,
            0xC000,
            0xD000,
            0xDF00,
            0xE000,
        ] {
            assert!(
                !matches!(decode_thumb(ROM_BASE, raw).kind, InstructionKind::Thumb(ThumbOp::Unknown)),
                "raw={raw:#06x}"
            );
        }
    }

    #[test]
    fn decodes_thumb_bx_as_control_flow() {
        assert_eq!(
            decode_thumb(ROM_BASE, 0x4700).kind,
            InstructionKind::Thumb(ThumbOp::BranchExchange { rm: 0 })
        );
    }

    #[test]
    fn decodes_arm_blx_and_bx() {
        assert_eq!(
            decode_arm(ROM_BASE, 0xE12F_FF31).kind,
            InstructionKind::Arm(ArmOp::BranchExchange { rm: 1, link: true })
        );
        assert_eq!(
            decode_arm(ROM_BASE, 0xE12F_FF11).kind,
            InstructionKind::Arm(ArmOp::BranchExchange { rm: 1, link: false })
        );
    }

    #[test]
    fn decodes_thumb_bl() {
        let instruction = decode_thumb_bl(ROM_BASE, 0xF000, 0xF800);
        assert_eq!(instruction.size, 4);
        assert!(matches!(
            instruction.kind,
            InstructionKind::Thumb(ThumbOp::BranchLink { .. })
        ));
    }
}
