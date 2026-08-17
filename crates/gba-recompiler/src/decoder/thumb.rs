use super::common::{sign_extend, thumb_condition, thumb_matches};
use super::types::{Condition, Instruction, InstructionKind, Mode, ThumbAluOp, ThumbExtended, ThumbOp};

pub fn decode_thumb(address: u32, raw: u16) -> Instruction {
    let rd = (raw & 7) as u8;
    let rs = ((raw >> 3) & 7) as u8;
    let op = if raw == 0x46C0 {
        ThumbOp::Nop
    } else if thumb_matches(raw, 0xE000, 0x0000) {
        ThumbOp::Extended(ThumbExtended::MoveShifted {
            kind: ((raw >> 11) & 3) as u8,
            rd,
            rs,
            offset: ((raw >> 6) & 0x1F) as u8,
        })
    } else if thumb_matches(raw, 0xF800, 0x1800) {
        let sub = raw & (1 << 9) != 0;
        let immediate = raw & (1 << 10) != 0;
        if immediate {
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
    } else if thumb_matches(raw, 0xF800, 0x2000) {
        ThumbOp::MovImm {
            rd: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        }
    } else if thumb_matches(raw, 0xF800, 0x3000) {
        ThumbOp::AddImm {
            rd: ((raw >> 8) & 7) as u8,
            rn: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        }
    } else if thumb_matches(raw, 0xF800, 0x3800) {
        ThumbOp::SubImm {
            rd: ((raw >> 8) & 7) as u8,
            rn: ((raw >> 8) & 7) as u8,
            imm: (raw & 0xFF) as u8,
        }
    } else if thumb_matches(raw, 0xFC00, 0x4000) {
        let opcode = ((raw >> 6) & 0xF) as u8;
        ThumbOp::Extended(ThumbExtended::Alu {
            op: match opcode {
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
            },
            rd,
            rs,
        })
    } else if thumb_matches(raw, 0xFF87, 0x4700) {
        ThumbOp::BranchExchange {
            rm: ((raw >> 3) & 0xF) as u8,
        }
    } else if thumb_matches(raw, 0xFC00, 0x4400) {
        ThumbOp::Extended(ThumbExtended::HighRegister {
            op: ((raw >> 8) & 3) as u8,
            rd: (((raw >> 7) & 1) << 3 | (raw & 7)) as u8,
            rs: (((raw >> 6) & 1) << 3 | ((raw >> 3) & 7)) as u8,
        })
    } else if thumb_matches(raw, 0xF800, 0x4800) {
        ThumbOp::LoadImm {
            rd: ((raw >> 8) & 7) as u8,
            rn: 15,
            word_offset: (raw & 0xFF) as u8,
        }
    } else if thumb_matches(raw, 0xF000, 0x5000) {
        let opcode = ((raw >> 9) & 7) as u8;
        if opcode < 4 {
            ThumbOp::Extended(ThumbExtended::LoadStoreRegister {
                load: matches!(opcode, 3),
                byte: matches!(opcode, 2),
                rd,
                rb: rs,
                ro: ((raw >> 6) & 7) as u8,
            })
        } else {
            ThumbOp::Extended(ThumbExtended::LoadStoreSignHalf {
                kind: opcode - 4,
                rd,
                rb: rs,
                ro: ((raw >> 6) & 7) as u8,
            })
        }
    } else if thumb_matches(raw, 0xE000, 0x6000) {
        ThumbOp::Extended(ThumbExtended::LoadStoreImmediate {
            load: raw & (1 << 11) != 0,
            byte: raw & (1 << 12) != 0,
            rd,
            rb: rs,
            offset: ((raw >> 6) & 0x1F) as u8,
        })
    } else if thumb_matches(raw, 0xF000, 0x8000) {
        ThumbOp::Extended(ThumbExtended::LoadStoreHalfword {
            load: raw & (1 << 11) != 0,
            rd,
            rb: rs,
            offset: ((raw >> 6) & 0x1F) as u8,
        })
    } else if thumb_matches(raw, 0xF000, 0x9000) {
        ThumbOp::Extended(ThumbExtended::SpRelativeLoadStore {
            load: raw & (1 << 11) != 0,
            rd: ((raw >> 8) & 7) as u8,
            offset: (raw & 0xFF) as u8,
        })
    } else if thumb_matches(raw, 0xF000, 0xA000) {
        ThumbOp::Extended(ThumbExtended::Address {
            rd: ((raw >> 8) & 7) as u8,
            use_sp: raw & (1 << 11) != 0,
            word_offset: (raw & 0xFF) as u8,
        })
    } else if thumb_matches(raw, 0xFF80, 0xB000) {
        ThumbOp::Extended(ThumbExtended::AddSp {
            negative: raw & (1 << 7) != 0,
            imm: ((raw & 0x7F) as u16) << 2,
        })
    } else if thumb_matches(raw, 0xFE00, 0xB400) || thumb_matches(raw, 0xFE00, 0xBC00) {
        ThumbOp::Extended(ThumbExtended::PushPop {
            load: raw & (1 << 11) != 0,
            registers: (raw & 0xFF) as u8,
            extra_lr_pc: raw & (1 << 8) != 0,
        })
    } else if thumb_matches(raw, 0xF000, 0xC000) {
        ThumbOp::Extended(ThumbExtended::MultipleLoadStore {
            load: raw & (1 << 11) != 0,
            rb: ((raw >> 8) & 7) as u8,
            register_list: (raw & 0xFF) as u8,
        })
    } else if thumb_matches(raw, 0xF000, 0xD000) && !thumb_matches(raw, 0x0F00, 0x0F00) {
        let cond = thumb_condition(raw);
        let offset = sign_extend(((raw & 0xFF) as u32) << 1, 9);
        ThumbOp::Branch {
            target: address.wrapping_add(4).wrapping_add(offset as u32),
            condition: cond,
        }
    } else if thumb_matches(raw, 0xFF00, 0xDF00) {
        ThumbOp::Extended(ThumbExtended::SoftwareInterrupt {
            comment: (raw & 0xFF) as u8,
        })
    } else if thumb_matches(raw, 0xF800, 0xE000) {
        let offset = sign_extend(((raw & 0x07FF) as u32) << 1, 12);
        ThumbOp::Branch {
            target: address.wrapping_add(4).wrapping_add(offset as u32),
            condition: Condition::Al,
        }
    } else {
        ThumbOp::Unknown
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

pub fn decode_thumb_bl(address: u32, first: u16, second: u16) -> Instruction {
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

    #[test]
    fn push_and_pop_use_disjoint_masks() {
        assert!(thumb_matches(0xB400, 0xFE00, 0xB400));
        assert!(thumb_matches(0xBC00, 0xFE00, 0xBC00));
        assert!(!thumb_matches(0xBE00, 0xFE00, 0xB400));
        assert!(!thumb_matches(0xBE00, 0xFE00, 0xBC00));
    }
}
