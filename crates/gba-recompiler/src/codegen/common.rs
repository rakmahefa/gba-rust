use std::fmt::Write;

use crate::cfg::BlockId;
use crate::decoder::{Condition, Mode};
use crate::ir::Value;

#[derive(Debug, Clone)]
pub struct RustModule {
    pub source: String,
}

pub fn condition_code(condition: Condition) -> u8 {
    match condition {
        Condition::Eq => 0x0,
        Condition::Ne => 0x1,
        Condition::Cs => 0x2,
        Condition::Cc => 0x3,
        Condition::Mi => 0x4,
        Condition::Pl => 0x5,
        Condition::Vs => 0x6,
        Condition::Vc => 0x7,
        Condition::Hi => 0x8,
        Condition::Ls => 0x9,
        Condition::Ge => 0xA,
        Condition::Lt => 0xB,
        Condition::Gt => 0xC,
        Condition::Le => 0xD,
        Condition::Al => 0xE,
    }
}

pub fn mode_bool(mode: Mode) -> bool {
    matches!(mode, Mode::Thumb)
}

pub fn block_name(block_id: BlockId, mode: Mode, address: u32) -> String {
    format!(
        "block_{}_{}_{address:08x}",
        block_id.0,
        if mode_bool(mode) { "thumb" } else { "arm" }
    )
}

pub fn value_expr(value: &Value) -> String {
    match value {
        Value::Reg(reg) => format!("rt.read_reg({reg})"),
        Value::Imm(value) => format!("{value:#010x}u32"),
    }
}

pub fn emit_flags_from_logic(out: &mut String, value: &str, carry: &str) {
    let _ = writeln!(out, "    let old = rt.nzcv();");
    let carry_expr = if carry == "None" {
        "old.c".to_string()
    } else {
        format!("{carry}.unwrap_or(old.c)")
    };
    let _ = writeln!(
        out,
        "    rt.set_flags(gba_runtime::Nzcv::new(({value}) & 0x8000_0000u32 != 0, ({value}) == 0u32, {carry_expr}, old.v));"
    );
}

pub fn emit_cmp_add(out: &mut String, lhs: &str, rhs: &str) {
    let _ = writeln!(
        out,
        "    let lhs_value = {lhs}; let rhs_value = {rhs}; let result = lhs_value.wrapping_add(rhs_value); let carry = result < lhs_value; let overflow = ((lhs_value ^ result) & (rhs_value ^ result) & 0x8000_0000u32) != 0; rt.set_flags(gba_runtime::Nzcv::new(result & 0x8000_0000u32 != 0, result == 0u32, carry, overflow));"
    );
}

pub fn emit_cmp_sub(out: &mut String, lhs: &str, rhs: &str) {
    let _ = writeln!(
        out,
        "    let lhs_value = {lhs}; let rhs_value = {rhs}; let result = lhs_value.wrapping_sub(rhs_value); let carry = lhs_value >= rhs_value; let overflow = ((lhs_value ^ rhs_value) & (lhs_value ^ result) & 0x8000_0000u32) != 0; rt.set_flags(gba_runtime::Nzcv::new(result & 0x8000_0000u32 != 0, result == 0u32, carry, overflow));"
    );
}
