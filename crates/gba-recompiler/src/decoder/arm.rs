use super::common::{arm_condition, arm_operand2, sign_extend};
use super::types::{ArmDataOp, ArmExtended, ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2};

pub fn decode_arm(address: u32, raw: u32) -> Instruction {
    let condition = arm_condition(raw);
    let opcode = ((raw >> 21) & 0xF) as u8;
    let rn = ((raw >> 16) & 0xF) as u8;
    let rd = ((raw >> 12) & 0xF) as u8;
    let op = if raw == 0xE1A0_0000 {
        ArmOp::Nop
    } else if (raw & 0x0FFF_FFF0) == 0x012F_FF30 {
        ArmOp::BranchExchange {
            rm: (raw & 0xF) as u8,
            link: true,
        }
    } else if (raw & 0x0FFF_FFF0) == 0x012F_FF10 {
        ArmOp::BranchExchange {
            rm: (raw & 0xF) as u8,
            link: false,
        }
    } else if (raw & 0x0F00_00F0) == 0x0100_0090 && (raw & 0x0200_0000) == 0 {
        ArmOp::Extended(ArmExtended::Swap {
            rd,
            rn,
            rm: (raw & 0xF) as u8,
            byte: raw & (1 << 22) != 0,
        })
    } else if (raw & 0x0E00_0000) == 0x0A00_0000 {
        let imm24 = raw & 0x00FF_FFFF;
        let target = address
            .wrapping_add(8)
            .wrapping_add(sign_extend(imm24 << 2, 26) as u32);
        ArmOp::Branch {
            target,
            condition,
            link: raw & (1 << 24) != 0,
        }
    } else if (raw & 0x0F00_0000) == 0x0F00_0000 {
        ArmOp::Extended(ArmExtended::SoftwareInterrupt {
            comment: raw & 0x00FF_FFFF,
        })
    } else if (raw & 0x0FBF_0FFF) == 0x010F_0000 {
        ArmOp::Extended(ArmExtended::Mrs {
            rd,
            spsr: raw & (1 << 22) != 0,
        })
    } else if (raw & 0x0DB0_F000) == 0x0120_F000 {
        ArmOp::Extended(ArmExtended::Msr {
            spsr: raw & (1 << 22) != 0,
            field_mask: ((raw >> 16) & 0xF) as u8,
            source: arm_operand2(raw),
        })
    } else if (raw & 0x0FC0_00F0) == 0x0000_0090 {
        ArmOp::Extended(ArmExtended::Multiply {
            rd,
            rn,
            rs: ((raw >> 8) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            accumulate: raw & (1 << 21) != 0,
            set_flags: raw & (1 << 20) != 0,
        })
    } else if (raw & 0x0F80_00F0) == 0x0080_0090 {
        ArmOp::Extended(ArmExtended::MultiplyLong {
            rd_hi: ((raw >> 16) & 0xF) as u8,
            rd_lo: rd,
            rs: ((raw >> 8) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            signed: raw & (1 << 22) != 0,
            accumulate: raw & (1 << 21) != 0,
            set_flags: raw & (1 << 20) != 0,
        })
    } else if (raw & 0x0E00_0000) == 0x0800_0000 {
        ArmOp::Extended(ArmExtended::BlockTransfer {
            load: raw & (1 << 20) != 0,
            rn,
            register_list: (raw & 0xFFFF) as u16,
            pre_index: raw & (1 << 24) != 0,
            up: raw & (1 << 23) != 0,
            write_back: raw & (1 << 21) != 0,
            user_mode: raw & (1 << 22) != 0,
        })
    } else if (raw & 0x0F00_0010) == 0x0E00_0000 {
        ArmOp::Extended(ArmExtended::CoprocessorData {
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 20) & 0xF) as u8,
            crd: ((raw >> 12) & 0xF) as u8,
            crn: ((raw >> 16) & 0xF) as u8,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 0x7) as u8,
        })
    } else if (raw & 0x0E00_0000) == 0x0400_0000 {
        let load = raw & (1 << 20) != 0;
        let byte = raw & (1 << 22) != 0;
        let pre_index = raw & (1 << 24) != 0;
        let up = raw & (1 << 23) != 0;
        let write_back = raw & (1 << 21) != 0;
        let offset = arm_operand2(raw);
        if !pre_index && !write_back && matches!(offset, Operand2::Imm(_)) {
            let magnitude = match offset {
                Operand2::Imm(v) => v as i32,
                Operand2::Reg { .. } => 0,
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
    } else if (raw & 0x0E40_0090) == 0x0000_0090 {
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
            rd,
            rn,
            offset: magnitude,
            pre_index,
            up,
            write_back,
        })
    } else if (raw & 0x0C00_0000) == 0 {
        let op2 = arm_operand2(raw);
        match opcode {
            0xD => ArmOp::Mov { rd, op2 },
            0x4 => ArmOp::Add { rd, rn, op2 },
            0x2 => ArmOp::Sub { rd, rn, op2 },
            0xA => ArmOp::Cmp { rn, op2 },
            op => ArmOp::Extended(ArmExtended::DataProcessing {
                op: match op {
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
                },
                rd,
                rn,
                op2,
                set_flags: raw & (1 << 20) != 0,
            }),
        }
    } else if (raw & 0x0C00_0010) == 0x0000_0010 {
        ArmOp::Extended(ArmExtended::CoprocessorRegisterTransfer {
            to_arm: raw & (1 << 20) != 0,
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 21) & 7) as u8,
            rd,
            crn: rn,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
        })
    } else if (raw & 0x0E00_0000) == 0x0C00_0000 {
        ArmOp::Extended(ArmExtended::CoprocessorTransfer {
            load: raw & (1 << 20) != 0,
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 21) & 7) as u8,
            crd: rd,
            crn: rn,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
            long: raw & (1 << 22) != 0,
        })
    } else if (raw & 0x0F00_0010) == 0x0E00_0010 {
        ArmOp::Extended(ArmExtended::CoprocessorData {
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 20) & 0xF) as u8,
            crd: rd,
            crn: rn,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
        })
    } else {
        ArmOp::Unknown
    };

    Instruction {
        address,
        mode: Mode::Arm,
        raw,
        size: 4,
        condition,
        kind: InstructionKind::Arm(op),
    }
}
