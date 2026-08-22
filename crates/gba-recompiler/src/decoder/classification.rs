use super::common::{arm_matches, thumb_matches};

const BX_LINK_MASK: u32 = 0x0FFF_FFF0;
const BX_LINK_PATTERN: u32 = 0x012F_FF30;
const BX_MASK: u32 = 0x0FFF_FFF0;
const BX_PATTERN: u32 = 0x012F_FF10;
const SWP_MASK: u32 = 0x0F00_00F0;
const SWP_PATTERN: u32 = 0x0100_0090;
const BRANCH_MASK: u32 = 0x0E00_0000;
const BRANCH_PATTERN: u32 = 0x0A00_0000;
const SWI_MASK: u32 = 0x0F00_0000;
const SWI_PATTERN: u32 = 0x0F00_0000;
const MRS_MASK: u32 = 0x0FBF_0FFF;
const MRS_PATTERN: u32 = 0x010F_0000;
const MSR_MASK: u32 = 0x0DB0_F000;
const MSR_PATTERN: u32 = 0x0120_F000;
const MULTIPLY_MASK: u32 = 0x0FC0_00F0;
const MULTIPLY_PATTERN: u32 = 0x0000_0090;
const MULTIPLY_LONG_MASK: u32 = 0x0F80_00F0;
const MULTIPLY_LONG_PATTERN: u32 = 0x0080_0090;
const BLOCK_TRANSFER_MASK: u32 = 0x0E00_0000;
const BLOCK_TRANSFER_PATTERN: u32 = 0x0800_0000;
const SINGLE_TRANSFER_MASK: u32 = 0x0C00_0000;
const SINGLE_TRANSFER_PATTERN: u32 = 0x0400_0000;
const HALFWORD_MASK: u32 = 0x0E00_0090;
const HALFWORD_PATTERN: u32 = 0x0000_0090;
const DATA_PROCESSING_MASK: u32 = 0x0C00_0000;
const DATA_PROCESSING_PATTERN: u32 = 0x0000_0000;
const COPROC_REG_MASK: u32 = 0x0C00_0010;
const COPROC_REG_PATTERN: u32 = 0x0000_0010;
const COPROC_TRANSFER_MASK: u32 = 0x0E00_0000;
const COPROC_TRANSFER_PATTERN: u32 = 0x0C00_0000;
const COPROC_DATA_MASK_LO: u32 = 0x0F00_0010;
const COPROC_DATA_PATTERN_LO: u32 = 0x0E00_0000;
const COPROC_DATA_MASK_HI: u32 = 0x0F00_0010;
const COPROC_DATA_PATTERN_HI: u32 = 0x0E00_0010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmClass {
    Nop,
    BranchExchange,
    Swap,
    Branch,
    SoftwareInterrupt,
    Mrs,
    Msr,
    Multiply,
    MultiplyLong,
    BlockTransfer,
    SingleDataTransfer,
    HalfwordTransfer,
    DataProcessing,
    CoprocessorRegisterTransfer,
    CoprocessorTransfer,
    CoprocessorData,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbClass {
    Nop,
    MoveShifted,
    AddSub,
    MovImmediate,
    AddImmediate,
    SubImmediate,
    Alu,
    BranchExchange,
    HighRegister,
    PcRelativeLoad,
    LoadStoreRegister,
    LoadStoreSignHalf,
    LoadStoreImmediate,
    LoadStoreHalfword,
    SpRelativeLoadStore,
    Address,
    AddSp,
    PushPop,
    MultipleLoadStore,
    ConditionalBranch,
    SoftwareInterrupt,
    Branch,
    Unknown,
}

pub fn classify_arm(raw: u32) -> ArmClass {
    if raw == 0xE1A0_0000 {
        return ArmClass::Nop;
    }
    if arm_matches(raw, BX_LINK_MASK, BX_LINK_PATTERN) || arm_matches(raw, BX_MASK, BX_PATTERN) {
        return ArmClass::BranchExchange;
    }
    if arm_matches(raw, SWP_MASK, SWP_PATTERN) && raw & (1 << 25) == 0 {
        return ArmClass::Swap;
    }
    if arm_matches(raw, BRANCH_MASK, BRANCH_PATTERN) {
        return ArmClass::Branch;
    }
    // ARM7TDMI SWI uses bits [27:24] = 0b1111 with a valid condition code
    // in bits [31:28]. Condition 0b1111 belongs to the separate ARMv5
    // unconditional/extended encoding space and must not be treated as SWI.
    if (raw >> 28) != 0xF && arm_matches(raw, SWI_MASK, SWI_PATTERN) {
        return ArmClass::SoftwareInterrupt;
    }
    if arm_matches(raw, MRS_MASK, MRS_PATTERN) {
        return ArmClass::Mrs;
    }
    if arm_matches(raw, MSR_MASK, MSR_PATTERN) {
        return ArmClass::Msr;
    }
    if arm_matches(raw, MULTIPLY_MASK, MULTIPLY_PATTERN) {
        return ArmClass::Multiply;
    }
    if arm_matches(raw, MULTIPLY_LONG_MASK, MULTIPLY_LONG_PATTERN) {
        return ArmClass::MultiplyLong;
    }
    if arm_matches(raw, BLOCK_TRANSFER_MASK, BLOCK_TRANSFER_PATTERN) {
        return ArmClass::BlockTransfer;
    }
    if arm_matches(raw, SINGLE_TRANSFER_MASK, SINGLE_TRANSFER_PATTERN) {
        return ArmClass::SingleDataTransfer;
    }
    if arm_matches(raw, HALFWORD_MASK, HALFWORD_PATTERN) {
        return ArmClass::HalfwordTransfer;
    }
    if arm_matches(raw, DATA_PROCESSING_MASK, DATA_PROCESSING_PATTERN) {
        return ArmClass::DataProcessing;
    }
    if arm_matches(raw, COPROC_REG_MASK, COPROC_REG_PATTERN) {
        return ArmClass::CoprocessorRegisterTransfer;
    }
    if arm_matches(raw, COPROC_TRANSFER_MASK, COPROC_TRANSFER_PATTERN) {
        return ArmClass::CoprocessorTransfer;
    }
    if arm_matches(raw, COPROC_DATA_MASK_LO, COPROC_DATA_PATTERN_LO)
        || arm_matches(raw, COPROC_DATA_MASK_HI, COPROC_DATA_PATTERN_HI)
    {
        return ArmClass::CoprocessorData;
    }
    ArmClass::Unknown
}

pub fn classify_thumb(raw: u16) -> ThumbClass {
    if raw == 0x46C0 {
        return ThumbClass::Nop;
    }
    if thumb_matches(raw, 0xE000, 0x0000) {
        return ThumbClass::MoveShifted;
    }
    if thumb_matches(raw, 0xF800, 0x1800) {
        return ThumbClass::AddSub;
    }
    if thumb_matches(raw, 0xF800, 0x2000) {
        return ThumbClass::MovImmediate;
    }
    if thumb_matches(raw, 0xF800, 0x3000) {
        return ThumbClass::AddImmediate;
    }
    if thumb_matches(raw, 0xF800, 0x3800) {
        return ThumbClass::SubImmediate;
    }
    if thumb_matches(raw, 0xFC00, 0x4000) {
        return ThumbClass::Alu;
    }
    if thumb_matches(raw, 0xFF87, 0x4700) {
        return ThumbClass::BranchExchange;
    }
    if thumb_matches(raw, 0xFC00, 0x4400) {
        return ThumbClass::HighRegister;
    }
    if thumb_matches(raw, 0xF800, 0x4800) {
        return ThumbClass::PcRelativeLoad;
    }
    if thumb_matches(raw, 0xF000, 0x5000) {
        let opcode = ((raw >> 9) & 7) as u8;
        return if opcode < 4 {
            ThumbClass::LoadStoreRegister
        } else {
            ThumbClass::LoadStoreSignHalf
        };
    }
    if thumb_matches(raw, 0xE000, 0x6000) {
        return ThumbClass::LoadStoreImmediate;
    }
    if thumb_matches(raw, 0xF000, 0x8000) {
        return ThumbClass::LoadStoreHalfword;
    }
    if thumb_matches(raw, 0xF000, 0x9000) {
        return ThumbClass::SpRelativeLoadStore;
    }
    if thumb_matches(raw, 0xF000, 0xA000) {
        return ThumbClass::Address;
    }
    if thumb_matches(raw, 0xFF80, 0xB000) {
        return ThumbClass::AddSp;
    }
    if thumb_matches(raw, 0xFE00, 0xB400) || thumb_matches(raw, 0xFE00, 0xBC00) {
        return ThumbClass::PushPop;
    }
    if thumb_matches(raw, 0xF000, 0xC000) {
        return ThumbClass::MultipleLoadStore;
    }
    if thumb_matches(raw, 0xF000, 0xD000) && !thumb_matches(raw, 0x0F00, 0x0F00) {
        return ThumbClass::ConditionalBranch;
    }
    if thumb_matches(raw, 0xFF00, 0xDF00) {
        return ThumbClass::SoftwareInterrupt;
    }
    if thumb_matches(raw, 0xF800, 0xE000) {
        return ThumbClass::Branch;
    }
    ThumbClass::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_single_transfer_class_accepts_both_offset_encodings() {
        assert_eq!(classify_arm(0xE5C0_1004), ArmClass::SingleDataTransfer);
        assert_eq!(classify_arm(0xE7C0_1004), ArmClass::SingleDataTransfer);
    }

    #[test]
    fn arm_swi_requires_a_valid_condition_field() {
        assert_eq!(classify_arm(0xEF00_0012), ArmClass::SoftwareInterrupt);
        assert_ne!(classify_arm(0xFF00_0012), ArmClass::SoftwareInterrupt);
        assert_eq!(classify_arm(0xFFFFFFFF), ArmClass::Unknown);
    }
}
