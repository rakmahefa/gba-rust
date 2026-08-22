use super::classification::ThumbClass;
use super::common::{sign_extend, thumb_condition};
use super::types::{
    Condition, Instruction, InstructionKind, Mode, ThumbAluOp, ThumbExtended, ThumbOp,
};

pub fn decode(address: u32, raw: u16, class: ThumbClass) -> Instruction {
    let op = match class {
        ThumbClass::Nop => ThumbOp::Nop,
        ThumbClass::MoveShifted => decode_move_shifted(raw),
        ThumbClass::AddSub => decode_add_sub(raw),
        ThumbClass::MovImmediate => ThumbOp::MovImm {
            rd: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        },
        ThumbClass::CmpImmediate => ThumbOp::CmpImm {
            rn: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        },
        ThumbClass::AddImmediate => ThumbOp::AddImm {
            rd: ((raw >> 8) & 7) as u8,
            rn: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        },
        ThumbClass::SubImmediate => ThumbOp::SubImm {
            rd: ((raw >> 8) & 7) as u8,
            rn: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        },
        ThumbClass::Alu => decode_alu(raw),
        ThumbClass::BranchExchange => ThumbOp::BranchExchange {
            rm: ((raw >> 3) & 0xF) as u8,
        },
        ThumbClass::HighRegister => decode_high_register(raw),
        ThumbClass::PcRelativeLoad => ThumbOp::LoadImm {
            rd: ((raw >> 8) & 7) as u8,
            rn: 15,
            word_offset: (raw & 0xFF) as u8,
        },
        ThumbClass::LoadStoreRegister | ThumbClass::LoadStoreSignHalf => {
            decode_register_memory(raw, class)
        }
        ThumbClass::LoadStoreImmediate => ThumbOp::Extended(ThumbExtended::LoadStoreImmediate {
            load: raw & (1 << 11) != 0,
            byte: raw & (1 << 12) != 0,
            rd: (raw & 7) as u8,
            rb: ((raw >> 3) & 7) as u8,
            offset: ((raw >> 6) & 0x1F) as u8,
        }),
        ThumbClass::LoadStoreHalfword => ThumbOp::Extended(ThumbExtended::LoadStoreHalfword {
            load: raw & (1 << 11) != 0,
            rd: (raw & 7) as u8,
            rb: ((raw >> 3) & 7) as u8,
            offset: ((raw >> 6) & 0x1F) as u8,
        }),
        ThumbClass::SpRelativeLoadStore => ThumbOp::Extended(ThumbExtended::SpRelativeLoadStore {
            load: raw & (1 << 11) != 0,
            rd: ((raw >> 8) & 7) as u8,
            offset: (raw & 0xFF) as u8,
        }),
        ThumbClass::Address => ThumbOp::Extended(ThumbExtended::Address {
            rd: ((raw >> 8) & 7) as u8,
            use_sp: raw & (1 << 11) != 0,
            word_offset: (raw & 0xFF) as u8,
        }),
        ThumbClass::AddSp => ThumbOp::Extended(ThumbExtended::AddSp {
            negative: raw & (1 << 7) != 0,
            imm: (raw & 0x7F) << 2,
        }),
        ThumbClass::PushPop => ThumbOp::Extended(ThumbExtended::PushPop {
            load: raw & (1 << 11) != 0,
            registers: (raw & 0xFF) as u8,
            extra_lr_pc: raw & (1 << 8) != 0,
        }),
        ThumbClass::MultipleLoadStore => ThumbOp::Extended(ThumbExtended::MultipleLoadStore {
            load: raw & (1 << 11) != 0,
            rb: ((raw >> 8) & 7) as u8,
            register_list: (raw & 0xFF) as u8,
        }),
        ThumbClass::ConditionalBranch => decode_conditional_branch(address, raw),
        ThumbClass::SoftwareInterrupt => ThumbOp::Extended(ThumbExtended::SoftwareInterrupt {
            comment: (raw & 0xFF) as u8,
        }),
        ThumbClass::Branch => decode_branch(address, raw),
        ThumbClass::Unknown => ThumbOp::Unknown,
    };

    Instruction {
        address,
        mode: Mode::Thumb,
        raw: raw as u32,
        size: 2,
        condition: Condition::Al,
        kind: InstructionKind::Thumb(op),
    }
}

fn decode_move_shifted(raw: u16) -> ThumbOp {
    ThumbOp::Extended(ThumbExtended::MoveShifted {
        kind: ((raw >> 11) & 3) as u8,
        rd: (raw & 7) as u8,
        rs: ((raw >> 3) & 7) as u8,
        offset: ((raw >> 6) & 0x1F) as u8,
    })
}

fn decode_add_sub(raw: u16) -> ThumbOp {
    let rd = (raw & 7) as u8;
    let rs = ((raw >> 3) & 7) as u8;
    let sub = raw & (1 << 9) != 0;
    if raw & (1 << 10) != 0 {
        ThumbOp::Extended(ThumbExtended::AddSubImmediate {
            sub,
            rd,
            rs,
            imm: ((raw >> 6) & 7) as u8,
        })
    } else {
        ThumbOp::Extended(ThumbExtended::AddSubRegister {
            sub,
            rd,
            rs,
            rn: ((raw >> 6) & 7) as u8,
        })
    }
}

fn decode_alu(raw: u16) -> ThumbOp {
    let opcode = ((raw >> 6) & 0xF) as u8;
    let op = match opcode {
        0 => ThumbAluOp::And,
        1 => ThumbAluOp::Eor,
        2 => ThumbAluOp::Lsl,
        3 => ThumbAluOp::Lsr,
        4 => ThumbAluOp::Asr,
        5 => ThumbAluOp::Adc,
        6 => ThumbAluOp::Sbc,
        7 => ThumbAluOp::Ror,
        8 => ThumbAluOp::Tst,
        9 => ThumbAluOp::Neg,
        10 => ThumbAluOp::Cmp,
        11 => ThumbAluOp::Cmn,
        12 => ThumbAluOp::Orr,
        13 => ThumbAluOp::Mul,
        14 => ThumbAluOp::Bic,
        _ => ThumbAluOp::Mvn,
    };
    ThumbOp::Extended(ThumbExtended::Alu {
        op,
        rd: (raw & 7) as u8,
        rs: ((raw >> 3) & 7) as u8,
    })
}

fn decode_high_register(raw: u16) -> ThumbOp {
    ThumbOp::Extended(ThumbExtended::HighRegister {
        op: ((raw >> 8) & 3) as u8,
        rd: (((raw >> 7) & 1) << 3 | (raw & 7)) as u8,
        rs: (((raw >> 6) & 1) << 3 | ((raw >> 3) & 7)) as u8,
    })
}

fn decode_register_memory(raw: u16, class: ThumbClass) -> ThumbOp {
    let rd = (raw & 7) as u8;
    let rb = ((raw >> 3) & 7) as u8;
    let ro = ((raw >> 6) & 7) as u8;
    let opcode = ((raw >> 9) & 7) as u8;
    match class {
        ThumbClass::LoadStoreRegister => ThumbOp::Extended(ThumbExtended::LoadStoreRegister {
            load: opcode == 3,
            byte: opcode == 2,
            rd,
            rb,
            ro,
        }),
        ThumbClass::LoadStoreSignHalf => ThumbOp::Extended(ThumbExtended::LoadStoreSignHalf {
            kind: opcode - 4,
            rd,
            rb,
            ro,
        }),
        _ => ThumbOp::Unknown,
    }
}

fn decode_conditional_branch(address: u32, raw: u16) -> ThumbOp {
    let condition = thumb_condition(raw);
    let offset = sign_extend(((raw & 0xFF) as u32) << 1, 9);
    ThumbOp::Branch {
        target: address.wrapping_add(4).wrapping_add(offset as u32),
        condition,
    }
}

fn decode_branch(address: u32, raw: u16) -> ThumbOp {
    let offset = sign_extend(((raw & 0x07FF) as u32) << 1, 12);
    ThumbOp::Branch {
        target: address.wrapping_add(4).wrapping_add(offset as u32),
        condition: Condition::Al,
    }
}

pub fn decode_bl(address: u32, first: u16, second: u16) -> Instruction {
    let s = ((first >> 10) & 1) as u32;
    let imm10 = (first & 0x03FF) as u32;
    let j1 = ((second >> 13) & 1) as u32;
    let j2 = ((second >> 11) & 1) as u32;
    let i1 = (!(j1 ^ s)) & 1;
    let i2 = (!(j2 ^ s)) & 1;
    let imm11 = (second & 0x07FF) as u32;
    let immediate = (s << 24) | (i1 << 23) | (i2 << 22) | (imm10 << 12) | (imm11 << 1);
    let target = address
        .wrapping_add(4)
        .wrapping_add(sign_extend(immediate, 25) as u32);

    Instruction {
        address,
        mode: Mode::Thumb,
        raw: ((first as u32) << 16) | second as u32,
        size: 4,
        condition: Condition::Al,
        kind: InstructionKind::Thumb(ThumbOp::BranchLink { target }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::classification::classify_thumb;

    #[test]
    fn family_decoders_cover_major_thumb_classes() {
        for raw in [
            0x0000, 0x1800, 0x2000, 0x2800, 0x3000, 0x4000, 0x4400, 0x4700, 0x4800, 0x5000,
            0x6000, 0x8000, 0x9000, 0xA000, 0xB000, 0xB400, 0xC000, 0xD000, 0xDF00, 0xE000,
        ] {
            let class = classify_thumb(raw);
            let instruction = decode(0x0800_0000, raw, class);
            assert!(
                !matches!(instruction.kind, InstructionKind::Thumb(ThumbOp::Unknown)),
                "{raw:#06x}"
            );
        }
    }

    #[test]
    fn decodes_thumb_cmp_immediate() {
        let instruction = decode(0x0800_0990, 0x2A5F, ThumbClass::CmpImmediate);
        assert_eq!(
            instruction.kind,
            InstructionKind::Thumb(ThumbOp::CmpImm { rn: 2, imm: 0x5F })
        );
        assert_eq!(instruction.raw, 0x2A5F);
        assert_eq!(instruction.size, 2);
    }
}
