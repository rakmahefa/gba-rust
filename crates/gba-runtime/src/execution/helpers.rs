use super::Runtime;
use crate::arm7tdmi::{self, ShiftKind};

pub(super) fn condition(raw: u32) -> u8 {
    (raw >> 28) as u8
}

pub(super) fn arm_operand2(rt: &Runtime, raw: u32) -> (u32, Option<bool>) {
    if raw & (1 << 25) != 0 {
        let imm = raw & 0xff;
        let rotate = ((raw >> 8) & 0xf) * 2;
        let value = imm.rotate_right(rotate);
        return (
            value,
            if rotate == 0 {
                None
            } else {
                Some(value & 0x8000_0000 != 0)
            },
        );
    }

    let value = rt.read_reg((raw & 0xf) as usize);
    let kind = match (raw >> 5) & 3 {
        0 => ShiftKind::Lsl,
        1 => ShiftKind::Lsr,
        2 => ShiftKind::Asr,
        _ => ShiftKind::Ror,
    };
    let amount = if raw & 0x10 == 0 {
        ((raw >> 7) & 0x1f) as u8
    } else {
        (rt.read_reg(((raw >> 8) & 0xf) as usize) & 0xff) as u8
    };
    let result = if raw & 0x10 == 0 {
        arm7tdmi::shift_immediate(value, kind, amount, rt.nzcv().c)
    } else {
        arm7tdmi::shift_register(value, kind, amount, rt.nzcv().c)
    };
    let carry = if amount == 0 && (raw & 0x10 != 0 || matches!(kind, ShiftKind::Lsl)) {
        None
    } else {
        Some(result.carry)
    };
    (result.value, carry)
}

pub(super) fn set_logic_flags(rt: &mut Runtime, value: u32, carry: Option<bool>) {
    let old = rt.nzcv();
    rt.set_flags(arm7tdmi::Nzcv::new(
        value & 0x8000_0000 != 0,
        value == 0,
        carry.unwrap_or(old.c),
        old.v,
    ));
}
