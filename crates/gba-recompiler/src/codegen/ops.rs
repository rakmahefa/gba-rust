use std::fmt::Write;

use crate::decoder::Mode;
use crate::ir::IrOp;

use super::arm::emit_arm_extended;
use super::common::{emit_cmp_sub, emit_flags_from_logic, value_expr};
use super::operands::arm_operand2;
use super::thumb::emit_thumb_extended;

fn emit_inner_op(out: &mut String, ins_raw: u32, mode: Mode, op: &IrOp) {
    match op {
        IrOp::Nop => {}
        IrOp::Mov { dst, src, set_flags } => {
            let (rhs, carry) = if mode == Mode::Arm {
                arm_operand2(out, ins_raw)
            } else {
                (value_expr(src), "None".into())
            };
            let _ = writeln!(out, "    rt.mov({dst}, {rhs}, false);");
            if *set_flags {
                emit_flags_from_logic(out, &rhs, &carry);
            }
        }
        IrOp::Add { dst, lhs, rhs, set_flags } => {
            let rhs = if mode == Mode::Arm { arm_operand2(out, ins_raw).0 } else { value_expr(rhs) };
            let _ = writeln!(out, "    rt.add({dst}, rt.read_reg({lhs}), {rhs}, {set_flags});");
        }
        IrOp::Sub { dst, lhs, rhs, set_flags } => {
            let rhs = if mode == Mode::Arm { arm_operand2(out, ins_raw).0 } else { value_expr(rhs) };
            let _ = writeln!(out, "    rt.sub({dst}, rt.read_reg({lhs}), {rhs}, {set_flags});");
        }
        IrOp::Cmp { lhs, rhs } => {
            let rhs = if mode == Mode::Arm { arm_operand2(out, ins_raw).0 } else { value_expr(rhs) };
            emit_cmp_sub(out, &format!("rt.read_reg({lhs})"), &rhs);
        }
        IrOp::Load { dst, base, offset, byte } => {
            let _ = writeln!(out, "    let address = rt.read_reg({base}).wrapping_add({offset}i32 as u32);");
            if *byte {
                let _ = writeln!(out, "    rt.write_reg({dst}, rt.read8(address) as u32);");
            } else {
                let _ = writeln!(out, "    rt.write_reg({dst}, rt.read32(address));");
            }
        }
        IrOp::Store { src, base, offset, byte } => {
            let _ = writeln!(out, "    let address = rt.read_reg({base}).wrapping_add({offset}i32 as u32);");
            if *byte {
                let _ = writeln!(out, "    rt.write8(address, rt.read_reg({src}) as u8);");
            } else {
                let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({src}));");
            }
        }
        IrOp::Branch { .. } | IrOp::BranchExchange { .. } => {}
        IrOp::ArmExtended { op } => emit_arm_extended(out, *op),
        IrOp::ThumbExtended { op } => emit_thumb_extended(out, *op),
        IrOp::Unknown { .. } => {
            let _ = writeln!(out, "    return Err(\"unsupported instruction in specialized codegen\");");
        }
    }
}

pub fn emit_op(out: &mut String, ins_address: u32, ins_raw: u32, mode: Mode, op: &IrOp) {
    let _ = writeln!(out, "    rt.enter_instruction({ins_address:#010x}, {});", matches!(mode, Mode::Thumb));
    if mode == Mode::Arm {
        let condition = (ins_raw >> 28) & 0xf;
        if condition != 0xe {
            let _ = writeln!(out, "    if rt.condition_code({condition}) {{");
            emit_inner_op(out, ins_raw, mode, op);
            let _ = writeln!(out, "    }}");
        } else {
            emit_inner_op(out, ins_raw, mode, op);
        }
    } else {
        emit_inner_op(out, ins_raw, mode, op);
    }
    let _ = writeln!(out, "    rt.tick(1);");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_and_mode_are_emitted_by_the_operation_boundary() {
        let mut out = String::new();
        emit_op(&mut out, 0x0800_0000, 0xE3A0_0001, Mode::Arm, &IrOp::Nop);
        assert!(out.contains("rt.enter_instruction(0x08000000, false)"));
        assert!(out.contains("rt.tick(1)"));
    }

    #[test]
    fn immediate_values_keep_the_generated_u32_contract() {
        assert_eq!(value_expr(&crate::ir::Value::Imm(0x12)), "0x00000012u32");
    }

    #[test]
    fn cmp_helpers_are_reachable_from_ir_emission_layer() {
        let mut out = String::new();
        emit_cmp_sub(&mut out, "2u32", "1u32");
        assert!(out.contains("wrapping_sub"));
    }
}
