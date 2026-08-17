use super::common::{arm_condition, arm_matches, arm_operand2, sign_extend};
use super::types::{ArmDataOp, ArmExtended, ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2};

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
const SINGLE_TRANSFER_MASK: u32 = 0x0E00_0000;
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

pub fn decode_arm(address: u32, raw: u32) -> Instruction {
    let condition = arm_condition(raw);
    let op = decode_special(raw, address)
        .or_else(|| decode_memory(raw))
        .or_else(|| decode_data_processing(raw))
        .or_else(|| decode_coprocessor(raw))
        .unwrap_or(ArmOp::Unknown);

    Instruction {
        address,
        mode: Mode::Arm,
        raw,
        size: 4,
        condition,
        kind: InstructionKind::Arm(op),
    }
}

fn decode_special(raw: u32, address: u32) -> Option<ArmOp> {
    if raw == 0xE1A0_0000 {
        return Some(ArmOp::Nop);
    }

    if arm_matches(raw, BX_LINK_MASK, BX_LINK_PATTERN) {
        return Some(ArmOp::BranchExchange {
            rm: (raw & 0xF) as u8,
            link: true,
        });
    }

    if arm_matches(raw, BX_MASK, BX_PATTERN) {
        return Some(ArmOp::BranchExchange {
            rm: (raw & 0xF) as u8,
            link: false,
        });
    }

    if arm_matches(raw, SWP_MASK, SWP_PATTERN) && raw & (1 << 25) == 0 {
        return Some(ArmOp::Extended(ArmExtended::Swap {
            rd: ((raw >> 12) & 0xF) as u8,
            rn: ((raw >> 16) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            byte: raw & (1 << 22) != 0,
        }));
    }

    if arm_matches(raw, BRANCH_MASK, BRANCH_PATTERN) {
        let imm24 = raw & 0x00FF_FFFF;
        let target = address
            .wrapping_add(8)
            .wrapping_add(sign_extend(imm24 << 2, 26) as u32);
        return Some(ArmOp::Branch {
            target,
            condition: arm_condition(raw),
            link: raw & (1 << 24) != 0,
        });
    }

    if arm_matches(raw, SWI_MASK, SWI_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::SoftwareInterrupt {
            comment: raw & 0x00FF_FFFF,
        }));
    }

    if arm_matches(raw, MRS_MASK, MRS_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::Mrs {
            rd: ((raw >> 12) & 0xF) as u8,
            spsr: raw & (1 << 22) != 0,
        }));
    }

    if arm_matches(raw, MSR_MASK, MSR_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::Msr {
            spsr: raw & (1 << 22) != 0,
            field_mask: ((raw >> 16) & 0xF) as u8,
            source: arm_operand2(raw),
        }));
    }

    if arm_matches(raw, MULTIPLY_MASK, MULTIPLY_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::Multiply {
            rd: ((raw >> 16) & 0xF) as u8,
            rn: ((raw >> 12) & 0xF) as u8,
            rs: ((raw >> 8) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            accumulate: raw & (1 << 21) != 0,
            set_flags: raw & (1 << 20) != 0,
        }));
    }

    if arm_matches(raw, MULTIPLY_LONG_MASK, MULTIPLY_LONG_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::MultiplyLong {
            rd_hi: ((raw >> 16) & 0xF) as u8,
            rd_lo: ((raw >> 12) & 0xF) as u8,
            rs: ((raw >> 8) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            signed: raw & (1 << 22) != 0,
            accumulate: raw & (1 << 21) != 0,
            set_flags: raw & (1 << 20) != 0,
        }));
    }

    if arm_matches(raw, BLOCK_TRANSFER_MASK, BLOCK_TRANSFER_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::BlockTransfer {
            load: raw & (1 << 20) != 0,
            rn: ((raw >> 16) & 0xF) as u8,
            register_list: (raw & 0xFFFF) as u16,
            pre_index: raw & (1 << 24) != 0,
            up: raw & (1 << 23) != 0,
            write_back: raw & (1 << 21) != 0,
            user_mode: raw & (1 << 22) != 0,
        }));
    }

    None
}

fn decode_memory(raw: u32) -> Option<ArmOp> {
    if arm_matches(raw, SINGLE_TRANSFER_MASK, SINGLE_TRANSFER_PATTERN) {
        return Some(decode_single_transfer(raw));
    }

    if arm_matches(raw, HALFWORD_MASK, HALFWORD_PATTERN) {
        return Some(decode_halfword_transfer(raw));
    }

    None
}

fn decode_single_transfer(raw: u32) -> ArmOp {
    let rd = ((raw >> 12) & 0xF) as u8;
    let rn = ((raw >> 16) & 0xF) as u8;
    let load = raw & (1 << 20) != 0;
    let byte = raw & (1 << 22) != 0;
    let pre_index = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let write_back = raw & (1 << 21) != 0;
    let offset = arm_operand2(raw);

    if !pre_index && !write_back && matches!(offset, Operand2::Imm(_)) {
        let magnitude = match offset {
            Operand2::Imm(value) => value as i32,
            Operand2::Reg { .. } => unreachable!(),
        };
        let magnitude = if up { magnitude } else { -magnitude };
        if load {
            ArmOp::Load {
                rd,
                rn,
                offset: magnitude,
                byte,
            }
        } else {
            ArmOp::Store {
                rd,
                rn,
                offset: magnitude,
                byte,
            }
        }
    } else {
        ArmOp::Extended(ArmExtended::SingleDataTransfer {
            load,
            byte,
            rd,
            rn,
            offset,
            pre_index,
            up,
            write_back,
        })
    }
}

fn decode_halfword_transfer(raw: u32) -> ArmOp {
    let load = raw & (1 << 20) != 0;
    let pre_index = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let write_back = raw & (1 << 21) != 0;
    let immediate = raw & (1 << 22) != 0;
    let signed = raw & (1 << 6) != 0;
    let halfword = !signed;
    let offset = if immediate {
        ((raw >> 4) & 0xF0) | (raw & 0xF)
    } else {
        raw & 0xF
    };
    let magnitude = if up { offset as i32 } else { -(offset as i32) };

    ArmOp::Extended(ArmExtended::HalfwordTransfer {
        load,
        signed,
        halfword,
        rd: ((raw >> 12) & 0xF) as u8,
        rn: ((raw >> 16) & 0xF) as u8,
        offset: magnitude,
        pre_index,
        up,
        write_back,
    })
}

fn decode_data_processing(raw: u32) -> Option<ArmOp> {
    if !arm_matches(raw, DATA_PROCESSING_MASK, DATA_PROCESSING_PATTERN) {
        return None;
    }

    let opcode = ((raw >> 21) & 0xF) as u8;
    let rd = ((raw >> 12) & 0xF) as u8;
    let rn = ((raw >> 16) & 0xF) as u8;
    let op2 = arm_operand2(raw);

    let simple = match opcode {
        0xD => Some(ArmOp::Mov { rd, op2 }),
        0x4 => Some(ArmOp::Add { rd, rn, op2 }),
        0x2 => Some(ArmOp::Sub { rd, rn, op2 }),
        0xA => Some(ArmOp::Cmp { rn, op2 }),
        _ => None,
    };

    simple.or_else(|| {
        Some(ArmOp::Extended(ArmExtended::DataProcessing {
            op: arm_data_op(opcode),
            rd,
            rn,
            op2,
            set_flags: raw & (1 << 20) != 0,
        }))
    })
}

fn arm_data_op(opcode: u8) -> ArmDataOp {
    match opcode {
        0 => ArmDataOp::And,
        1 => ArmDataOp::Eor,
        2 => ArmDataOp::Sub,
        3 => ArmDataOp::Rsb,
        4 => ArmDataOp::Add,
        5 => ArmDataOp::Adc,
        6 => ArmDataOp::Sbc,
        7 => ArmDataOp::Rsc,
        8 => ArmDataOp::Tst,
        9 => ArmDataOp::Teq,
        10 => ArmDataOp::Cmp,
        11 => ArmDataOp::Cmn,
        12 => ArmDataOp::Orr,
        13 => ArmDataOp::Mov,
        14 => ArmDataOp::Bic,
        _ => ArmDataOp::Mvn,
    }
}

fn decode_coprocessor(raw: u32) -> Option<ArmOp> {
    if arm_matches(raw, COPROC_REG_MASK, COPROC_REG_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::CoprocessorRegisterTransfer {
            to_arm: raw & (1 << 20) != 0,
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 21) & 7) as u8,
            rd: ((raw >> 12) & 0xF) as u8,
            crn: ((raw >> 16) & 0xF) as u8,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
        }));
    }

    if arm_matches(raw, COPROC_TRANSFER_MASK, COPROC_TRANSFER_PATTERN) {
        return Some(ArmOp::Extended(ArmExtended::CoprocessorTransfer {
            load: raw & (1 << 20) != 0,
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 21) & 7) as u8,
            crd: ((raw >> 12) & 0xF) as u8,
            crn: ((raw >> 16) & 0xF) as u8,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
            long: raw & (1 << 22) != 0,
        }));
    }

    if arm_matches(raw, COPROC_DATA_MASK_LO, COPROC_DATA_PATTERN_LO)
        || arm_matches(raw, COPROC_DATA_MASK_HI, COPROC_DATA_PATTERN_HI)
    {
        return Some(decode_coprocessor_data(raw));
    }

    None
}

fn decode_coprocessor_data(raw: u32) -> ArmOp {
    ArmOp::Extended(ArmExtended::CoprocessorData {
        cp: ((raw >> 8) & 0xF) as u8,
        opcode1: ((raw >> 20) & 0xF) as u8,
        crd: ((raw >> 12) & 0xF) as u8,
        crn: ((raw >> 16) & 0xF) as u8,
        crm: (raw & 0xF) as u8,
        opcode2: ((raw >> 5) & 0x7) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::types::{ArmExtended, ArmOp};

    #[test]
    fn arm_mask_constants_are_consistent() {
        for (mask, pattern) in [
            (BX_LINK_MASK, BX_LINK_PATTERN),
            (BX_MASK, BX_PATTERN),
            (SWP_MASK, SWP_PATTERN),
            (BRANCH_MASK, BRANCH_PATTERN),
            (SWI_MASK, SWI_PATTERN),
            (MRS_MASK, MRS_PATTERN),
            (MSR_MASK, MSR_PATTERN),
            (MULTIPLY_MASK, MULTIPLY_PATTERN),
            (MULTIPLY_LONG_MASK, MULTIPLY_LONG_PATTERN),
            (BLOCK_TRANSFER_MASK, BLOCK_TRANSFER_PATTERN),
            (SINGLE_TRANSFER_MASK, SINGLE_TRANSFER_PATTERN),
            (HALFWORD_MASK, HALFWORD_PATTERN),
            (DATA_PROCESSING_MASK, DATA_PROCESSING_PATTERN),
            (COPROC_REG_MASK, COPROC_REG_PATTERN),
            (COPROC_TRANSFER_MASK, COPROC_TRANSFER_PATTERN),
            (COPROC_DATA_MASK_LO, COPROC_DATA_PATTERN_LO),
            (COPROC_DATA_MASK_HI, COPROC_DATA_PATTERN_HI),
        ] {
            assert_eq!(pattern & !mask, 0);
        }
    }

    #[test]
    fn decode_arm_keeps_control_flow_priority() {
        assert_eq!(
            decode_arm(0x0800_0000, 0xE12F_FF11).kind,
            InstructionKind::Arm(ArmOp::BranchExchange { rm: 1, link: false })
        );
    }

    #[test]
    fn decode_arm_keeps_extended_memory_ops() {
        let instruction = decode_arm(0, 0xE1D0_00B0);
        assert!(matches!(
            instruction.kind,
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::HalfwordTransfer { .. }))
        ));

        let register_offset = decode_arm(0, 0xE1D0_00B1);
        assert!(matches!(
            register_offset.kind,
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::HalfwordTransfer { .. }))
        ));
    }
}
