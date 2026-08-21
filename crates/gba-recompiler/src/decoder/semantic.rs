use super::classification::{classify_arm, classify_thumb};
use super::memory::{read_arm, read_thumb, read_thumb_bl, DecodeError};
use super::semantic_arm;
use super::semantic_thumb;
use super::types::Instruction;

pub fn decode_arm(address: u32, raw: u32) -> Instruction {
    semantic_arm::decode(address, raw, classify_arm(raw))
}

pub fn decode_thumb(address: u32, raw: u16) -> Instruction {
    semantic_thumb::decode(address, raw, classify_thumb(raw))
}

pub fn decode_thumb_bl(address: u32, first: u16, second: u16) -> Instruction {
    semantic_thumb::decode_bl(address, first, second)
}

pub fn decode_arm_from_rom(rom: &[u8], address: u32) -> Result<Instruction, DecodeError> {
    Ok(decode_arm(address, read_arm(rom, address)?))
}

pub fn decode_thumb_from_rom(rom: &[u8], address: u32) -> Result<Instruction, DecodeError> {
    Ok(decode_thumb(address, read_thumb(rom, address)?))
}

pub fn decode_thumb_bl_from_rom(rom: &[u8], address: u32) -> Result<Instruction, DecodeError> {
    let (first, second) = read_thumb_bl(rom, address)?;
    if !matches!(
        classify_thumb(first),
        super::classification::ThumbClass::Branch
    ) || (second & 0xF800) != 0xF800
    {
        return Ok(decode_thumb(address, first));
    }
    Ok(decode_thumb_bl(address, first, second))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::types::{ArmExtended, ArmOp, InstructionKind, Mode, ThumbOp};

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
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer { .. }))
        ));
        assert!(matches!(
            decode_thumb(0x0800_0000, 0xBE00).kind,
            InstructionKind::Thumb(ThumbOp::Unknown)
        ));
    }

    #[test]
    fn rom_wrappers_preserve_instruction_identity() {
        let rom = 0xE3A0_0001u32.to_le_bytes().to_vec();
        let instruction = decode_arm_from_rom(&rom, 0x0800_0000).expect("ARM decode");
        assert_eq!(instruction.address, 0x0800_0000);
        assert_eq!(instruction.size, 4);
        assert_eq!(instruction.mode, Mode::Arm);
    }
}
