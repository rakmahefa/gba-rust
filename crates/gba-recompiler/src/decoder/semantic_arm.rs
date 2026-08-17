use super::classification::ArmClass;
use super::common::{arm_condition, arm_matches, arm_operand2, sign_extend};
use super::types::{ArmDataOp, ArmExtended, ArmOp, Instruction, InstructionKind, Mode, Operand2};

const BX_LINK_MASK: u32 = 0x0FFF_FFF0;
const BX_LINK_PATTERN: u32 = 0x012F_FF30;
const BX_MASK: u32 = 0x0FFF_FFF0;
const BX_PATTERN: u32 = 0x012F_FF10;

pub fn decode(address: u32, raw: u32, class: ArmClass) -> Instruction {
    let op = match class {
        ArmClass::Nop
        | ArmClass::BranchExchange
        | ArmClass::Branch
        | ArmClass::Swap
        | ArmClass::SoftwareInterrupt
        | ArmClass::Mrs
        | ArmClass::Msr
        | ArmClass::Multiply
        | ArmClass::MultiplyLong
        | ArmClass::BlockTransfer => decode_control_special(raw, address, class),
        ArmClass::SingleDataTransfer | ArmClass::HalfwordTransfer => decode_memory(raw, class),
        ArmClass::DataProcessing => decode_data_processing(raw),
        ArmClass::CoprocessorRegisterTransfer
        | ArmClass::CoprocessorTransfer
        | ArmClass::CoprocessorData => decode_coprocessor(raw, class),
        ArmClass::Unknown => ArmOp::Unknown,
    };

    Instruction {
        address,
        mode: Mode::Arm,
        raw,
        size: 4,
        condition: arm_condition(raw),
        kind: InstructionKind::Arm(op),
    }
}

fn decode_control_special(raw: u32, address: u32, class: ArmClass) -> ArmOp {
    match class {
        ArmClass::Nop => ArmOp::Nop,
        ArmClass::BranchExchange => ArmOp::BranchExchange {
            rm: (raw & 0xF) as u8,
            link: arm_matches(raw, BX_LINK_MASK, BX_LINK_PATTERN),
        },
        ArmClass::Swap => ArmOp::Extended(ArmExtended::Swap {
            rd: ((raw >> 12) & 0xF) as u8,
            rn: ((raw >> 16) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            byte: raw & (1 << 22) != 0,
        }),
        ArmClass::Branch => {
            let imm24 = raw & 0x00FF_FFFF;
            let target = address
                .wrapping_add(8)
                .wrapping_add(sign_extend(imm24 << 2, 26) as u32);
            ArmOp::Branch {
                target,
                condition: arm_condition(raw),
                link: raw & (1 << 24) != 0,
            }
        }
        ArmClass::SoftwareInterrupt => ArmOp::Extended(ArmExtended::SoftwareInterrupt {
            comment: raw & 0x00FF_FFFF,
        }),
        ArmClass::Mrs => ArmOp::Extended(ArmExtended::Mrs {
            rd: ((raw >> 12) & 0xF) as u8,
            spsr: raw & (1 << 22) != 0,
        }),
        ArmClass::Msr => ArmOp::Extended(ArmExtended::Msr {
            spsr: raw & (1 << 22) != 0,
            field_mask: ((raw >> 16) & 0xF) as u8,
            source: arm_operand2(raw),
        }),
        ArmClass::Multiply => ArmOp::Extended(ArmExtended::Multiply {
            rd: ((raw >> 16) & 0xF) as u8,
            rn: ((raw >> 12) & 0xF) as u8,
            rs: ((raw >> 8) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            accumulate: raw & (1 << 21) != 0,
            set_flags: raw & (1 << 20) != 0,
        }),
        ArmClass::MultiplyLong => ArmOp::Extended(ArmExtended::MultiplyLong {
            rd_hi: ((raw >> 16) & 0xF) as u8,
            rd_lo: ((raw >> 12) & 0xF) as u8,
            rs: ((raw >> 8) & 0xF) as u8,
            rm: (raw & 0xF) as u8,
            signed: raw & (1 << 22) != 0,
            accumulate: raw & (1 << 21) != 0,
            set_flags: raw & (1 << 20) != 0,
        }),
        ArmClass::BlockTransfer => ArmOp::Extended(ArmExtended::BlockTransfer {
            load: raw & (1 << 20) != 0,
            rn: ((raw >> 16) & 0xF) as u8,
            register_list: (raw & 0xFFFF) as u16,
            pre_index: raw & (1 << 24) != 0,
            up: raw & (1 << 23) != 0,
            write_back: raw & (1 << 21) != 0,
            user_mode: raw & (1 << 22) != 0,
        }),
        _ => ArmOp::Unknown,
    }
}

fn decode_memory(raw: u32, class: ArmClass) -> ArmOp {
    match class {
        ArmClass::SingleDataTransfer => decode_single_transfer(raw),
        ArmClass::HalfwordTransfer => decode_halfword_transfer(raw),
        _ => ArmOp::Unknown,
    }
}

fn decode_single_transfer(raw: u32) -> ArmOp {
    let rd = ((raw >> 12) & 0xF) as u8;
    let rn = ((raw >> 16) & 0xF) as u8;
    let load = raw & (1 << 20) != 0;
    let byte = raw & (1 << 22) != 0;
    let pre_index = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let write_back = raw & (1 << 21) != 0;
    // For ARM single data transfers, I=0 encodes a 12-bit immediate offset
    // in bits [11:0]; it is not the data-processing Operand2 encoding.
    // Only I=1 uses the register+barrel-shifter form represented by Operand2.
    let offset = if raw & (1 << 25) == 0 {
        Operand2::Imm(raw & 0x0FFF)
    } else {
        arm_operand2(raw)
    };

    if !pre_index && !write_back {
        if let Operand2::Imm(value) = offset {
            let magnitude = if up { value as i32 } else { -(value as i32) };
            return if load {
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
            };
        }
    }

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

fn decode_data_processing(raw: u32) -> ArmOp {
    let opcode = ((raw >> 21) & 0xF) as u8;
    let rd = ((raw >> 12) & 0xF) as u8;
    let rn = ((raw >> 16) & 0xF) as u8;
    let op2 = arm_operand2(raw);

    match opcode {
        0xD => ArmOp::Mov { rd, op2 },
        0x4 => ArmOp::Add { rd, rn, op2 },
        0x2 => ArmOp::Sub { rd, rn, op2 },
        0xA => ArmOp::Cmp { rn, op2 },
        _ => ArmOp::Extended(ArmExtended::DataProcessing {
            op: arm_data_op(opcode),
            rd,
            rn,
            op2,
            set_flags: raw & (1 << 20) != 0,
        }),
    }
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

fn decode_coprocessor(raw: u32, class: ArmClass) -> ArmOp {
    match class {
        ArmClass::CoprocessorRegisterTransfer => ArmOp::Extended(ArmExtended::CoprocessorRegisterTransfer {
            to_arm: raw & (1 << 20) != 0,
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 21) & 7) as u8,
            rd: ((raw >> 12) & 0xF) as u8,
            crn: ((raw >> 16) & 0xF) as u8,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
        }),
        ArmClass::CoprocessorTransfer => ArmOp::Extended(ArmExtended::CoprocessorTransfer {
            load: raw & (1 << 20) != 0,
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 21) & 7) as u8,
            crd: ((raw >> 12) & 0xF) as u8,
            crn: ((raw >> 16) & 0xF) as u8,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 7) as u8,
            long: raw & (1 << 22) != 0,
        }),
        ArmClass::CoprocessorData => ArmOp::Extended(ArmExtended::CoprocessorData {
            cp: ((raw >> 8) & 0xF) as u8,
            opcode1: ((raw >> 20) & 0xF) as u8,
            crd: ((raw >> 12) & 0xF) as u8,
            crn: ((raw >> 16) & 0xF) as u8,
            crm: (raw & 0xF) as u8,
            opcode2: ((raw >> 5) & 0x7) as u8,
        }),
        _ => ArmOp::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::classification::classify_arm;

    #[test]
    fn family_decoders_preserve_representative_arm_semantics() {
        for raw in [
            0xE1A0_0000,
            0xE281_2004,
            0xE401_2004,
            0xE1D1_20B0,
            0xEA00_0001,
            0xE12F_FF11,
            0xEF00_0001,
            0xE800_0000,
            0xEE00_0010,
        ] {
            let class = classify_arm(raw);
            let instruction = decode(0x0800_0000, raw, class);
            assert!(
                !matches!(instruction.kind, InstructionKind::Arm(ArmOp::Unknown)),
                "{raw:#010x}"
            );
        }
    }

    #[test]
    fn single_data_transfer_immediate_offset_is_not_decoded_as_shifted_register() {
        let raw = 0xE5C0_1004; // STRB r1, [r0, #4]
        let class = classify_arm(raw);
        let instruction = decode(0x0800_0000, raw, class);
        match instruction.kind {
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer { offset, .. })) => {
                assert_eq!(offset, Operand2::Imm(4));
            }
            other => panic!("unexpected decode: {other:?}"),
        }
    }

    #[test]
    fn single_data_transfer_register_offset_keeps_barrel_shifter_semantics() {
        let raw = 0xE7C0_1004; // STRB r1, [r0, r4] with I=1
        let class = classify_arm(raw);
        let instruction = decode(0x0800_0000, raw, class);
        match instruction.kind {
            InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer { offset, .. })) => {
                assert!(matches!(offset, Operand2::Reg { rm: 4, .. }));
            }
            other => panic!("unexpected decode: {other:?}"),
        }
    }
}