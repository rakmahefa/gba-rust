use std::fmt::Write;

use crate::decoder::Operand2;

pub fn arm_operand2(out: &mut String, raw: u32) -> (String, String) {
    if raw & (1 << 25) != 0 {
        let imm = raw & 0xff;
        let rotate = ((raw >> 8) & 0xf) * 2;
        if rotate == 0 {
            (format!("{imm:#010x}u32"), "None".into())
        } else {
            let value = format!("({imm:#010x}u32).rotate_right({rotate})");
            let carry = format!("Some((({value}) & 0x8000_0000u32) != 0)");
            (value, carry)
        }
    } else {
        let rm = raw & 0xf;
        let by_register = raw & 0x10 != 0;
        let kind = match (raw >> 5) & 3 {
            0 => "ShiftKind::Lsl",
            1 => "ShiftKind::Lsr",
            2 => "ShiftKind::Asr",
            _ => "ShiftKind::Ror",
        };
        let amount = if by_register {
            format!("(rt.read_reg({}) & 0xff) as u8", (raw >> 8) & 0xf)
        } else {
            format!("{}", (raw >> 7) & 0x1f)
        };
        let _ = writeln!(
            out,
            "    let shifted = rt.shift(rt.read_reg({rm}), {kind}, {amount}, {by_register});"
        );
        ("shifted.value".into(), "Some(shifted.carry)".into())
    }
}

pub fn structured_operand2(out: &mut String, operand: Operand2) -> (String, String) {
    match operand {
        Operand2::Imm(value) => (format!("{value:#010x}u32"), "None".into()),
        Operand2::Reg {
            rm,
            shift,
            shift_kind,
            by_register,
            shift_register,
        } => {
            let kind = match shift_kind {
                0 => "ShiftKind::Lsl",
                1 => "ShiftKind::Lsr",
                2 => "ShiftKind::Asr",
                _ => "ShiftKind::Ror",
            };
            let amount = if by_register {
                format!("(rt.read_reg({shift_register}) & 0xff) as u8")
            } else {
                format!("{shift}")
            };
            let _ = writeln!(
                out,
                "    let shifted = rt.shift(rt.read_reg({rm}), {kind}, {amount}, {by_register});"
            );
            ("shifted.value".into(), "Some(shifted.carry)".into())
        }
    }
}
