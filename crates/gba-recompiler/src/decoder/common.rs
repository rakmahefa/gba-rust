use super::types::{Condition, Operand2};

pub(super) fn sign_extend(value: u32, bits: u8) -> i32 {
    let shift = 32 - bits as u32;
    ((value << shift) as i32) >> shift
}

pub(super) fn arm_condition(raw: u32) -> Condition {
    match raw >> 28 {
        0x0 => Condition::Eq,
        0x1 => Condition::Ne,
        0x2 => Condition::Cs,
        0x3 => Condition::Cc,
        0x4 => Condition::Mi,
        0x5 => Condition::Pl,
        0x6 => Condition::Vs,
        0x7 => Condition::Vc,
        0x8 => Condition::Hi,
        0x9 => Condition::Ls,
        0xA => Condition::Ge,
        0xB => Condition::Lt,
        0xC => Condition::Gt,
        0xD => Condition::Le,
        _ => Condition::Al,
    }
}

pub(super) fn arm_operand2(raw: u32) -> Operand2 {
    if raw & (1 << 25) != 0 {
        let imm8 = raw & 0xFF;
        let rotate = ((raw >> 8) & 0xF) * 2;
        Operand2::Imm(imm8.rotate_right(rotate))
    } else {
        Operand2::Reg {
            rm: (raw & 0xF) as u8,
            shift: ((raw >> 7) & 0x1F) as u8,
        }
    }
}

pub(super) fn thumb_condition(raw: u16) -> Condition {
    match ((raw >> 8) & 0xF) as u8 {
        0 => Condition::Eq,
        1 => Condition::Ne,
        2 => Condition::Cs,
        3 => Condition::Cc,
        4 => Condition::Mi,
        5 => Condition::Pl,
        6 => Condition::Vs,
        7 => Condition::Vc,
        8 => Condition::Hi,
        9 => Condition::Ls,
        10 => Condition::Ge,
        11 => Condition::Lt,
        12 => Condition::Gt,
        13 => Condition::Le,
        _ => Condition::Al,
    }
}
