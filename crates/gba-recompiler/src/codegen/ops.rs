use std::fmt::Write;

use crate::decoder::{ArmExtended, Mode, ThumbExtended};
use crate::ir::IrOp;

use super::arm::emit_arm_extended;
use super::common::{emit_cmp_sub, emit_flags_from_logic, value_expr};
use super::operands::arm_operand2;
use super::thumb::emit_thumb_extended;

fn emit_inner_op(out: &mut String, ins_raw: u32, mode: Mode, op: &IrOp) -> bool {
    match op {
        IrOp::Nop => true,
        IrOp::Mov {
            dst,
            src,
            set_flags,
        } => {
            let (rhs, carry) = if mode == Mode::Arm {
                arm_operand2(out, ins_raw)
            } else {
                (value_expr(src), "None".into())
            };
            let _ = writeln!(out, "    rt.mov({dst}, {rhs}, false);");
            if *set_flags {
                emit_flags_from_logic(out, &rhs, &carry);
            }
            true
        }
        IrOp::Add {
            dst,
            lhs,
            rhs,
            set_flags,
        } => {
            let rhs = if mode == Mode::Arm {
                arm_operand2(out, ins_raw).0
            } else {
                value_expr(rhs)
            };
            let _ = writeln!(
                out,
                "    rt.add({dst}, rt.read_reg({lhs}), {rhs}, {set_flags});"
            );
            true
        }
        IrOp::Sub {
            dst,
            lhs,
            rhs,
            set_flags,
        } => {
            let rhs = if mode == Mode::Arm {
                arm_operand2(out, ins_raw).0
            } else {
                value_expr(rhs)
            };
            let _ = writeln!(
                out,
                "    rt.sub({dst}, rt.read_reg({lhs}), {rhs}, {set_flags});"
            );
            true
        }
        IrOp::Cmp { lhs, rhs } => {
            let rhs = if mode == Mode::Arm {
                arm_operand2(out, ins_raw).0
            } else {
                value_expr(rhs)
            };
            emit_cmp_sub(out, &format!("rt.read_reg({lhs})"), &rhs);
            true
        }
        IrOp::Load {
            dst,
            base,
            offset,
            byte,
        } => {
            let base_expr = if mode == Mode::Thumb && *base == 15 {
                "(rt.read_reg(15) & !3)".to_string()
            } else {
                format!("rt.read_reg({base})")
            };
            let _ = writeln!(
                out,
                "    let address = {base_expr}.wrapping_add({offset}i32 as u32);"
            );
            if *byte {
                let _ = writeln!(out, "    rt.write_reg({dst}, rt.read8(address) as u32);");
            } else {
                let _ = writeln!(out, "    rt.write_reg({dst}, rt.read32(address));");
            }
            true
        }
        IrOp::Store {
            src,
            base,
            offset,
            byte,
        } => {
            let _ = writeln!(
                out,
                "    let address = rt.read_reg({base}).wrapping_add({offset}i32 as u32);"
            );
            if *byte {
                let _ = writeln!(out, "    rt.write8(address, rt.read_reg({src}) as u8);");
            } else {
                let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({src}));");
            }
            true
        }
        IrOp::Branch { .. } | IrOp::BranchExchange { .. } => true,
        IrOp::ArmExtended { op } => {
            emit_arm_extended(out, *op);
            true
        }
        IrOp::ThumbExtended { op } => {
            emit_thumb_extended(out, *op);
            true
        }
        IrOp::Unknown { address, raw, mode } => {
            let _ = writeln!(
                out,
                "    return Err(format!(\"unsupported instruction in specialized codegen: pc={{:#010x}} mode={{:?}} raw={{:#010x}}\", {address:#010x}, {mode:?}, {raw:#010x}).leak());"
            );
            false
        }
    }
}

fn is_software_interrupt(op: &IrOp) -> bool {
    matches!(
        op,
        IrOp::ArmExtended {
            op: ArmExtended::SoftwareInterrupt { .. }
        } | IrOp::ThumbExtended {
            op: ThumbExtended::SoftwareInterrupt { .. }
        }
    )
}

pub fn emit_op(out: &mut String, ins_address: u32, ins_raw: u32, mode: Mode, op: &IrOp) {
    let _ = writeln!(
        out,
        "    rt.enter_instruction({ins_address:#010x}, {});",
        matches!(mode, Mode::Thumb)
    );
    if !is_software_interrupt(op) {
        let emitted = if mode == Mode::Arm {
            let condition = (ins_raw >> 28) & 0xf;
            if condition != 0xe {
                let _ = writeln!(out, "    if rt.condition_code({condition}) {{");
                let emitted = emit_inner_op(out, ins_raw, mode, op);
                let _ = writeln!(out, "    }}");
                emitted
            } else {
                emit_inner_op(out, ins_raw, mode, op)
            }
        } else {
            emit_inner_op(out, ins_raw, mode, op)
        };

        if emitted {
            let _ = writeln!(out, "    rt.tick(1);");
        }
    } else {
        let _ = writeln!(out, "    rt.tick(1);");
    }
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
    fn software_interrupt_is_deferred_to_the_terminator() {
        let mut out = String::new();
        emit_op(
            &mut out,
            0x0000_0000,
            0xEF00_0002,
            Mode::Arm,
            &IrOp::ArmExtended {
                op: ArmExtended::SoftwareInterrupt { comment: 2 },
            },
        );
        assert!(out.contains("rt.enter_instruction(0x00000000, false)"));
        assert!(out.contains("rt.tick(1)"));
        assert!(!out.contains("raise_exception"));
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

    #[test]
    fn thumb_pc_relative_generic_load_aligns_architectural_pc() {
        let mut out = String::new();
        emit_op(
            &mut out,
            0x0000_011e,
            0x4800,
            Mode::Thumb,
            &IrOp::Load {
                dst: 1,
                base: 15,
                offset: 0x160,
                byte: false,
            },
        );
        assert!(out.contains("let address = (rt.read_reg(15) & !3).wrapping_add(352i32 as u32);"));
    }

    #[test]
    fn unknown_instruction_emits_structured_diagnostic_without_unreachable_tick() {
        let mut out = String::new();
        emit_op(
            &mut out,
            0x0800_1234,
            0xE7FF_FF00,
            Mode::Arm,
            &IrOp::Unknown {
                address: 0x0800_1234,
                raw: 0xE7FF_FF00,
                mode: Mode::Arm,
            },
        );
        assert!(out.contains("unsupported instruction in specialized codegen"));
        assert!(out.contains("0x08001234"));
        assert!(out.contains("0xe7ffff00"));
        assert!(out.contains("mode=Arm"));
        assert_eq!(out.matches("rt.tick(1);").count(), 0);
    }
}
