use std::fmt::Write;

use crate::decoder::{ArmDataOp, ArmExtended};

use super::common::{emit_cmp_add, emit_cmp_sub, emit_flags_from_logic};
use super::operands::structured_operand2;

pub fn emit_arm_extended(out: &mut String, op: ArmExtended) {
    match op {
        ArmExtended::DataProcessing {
            op,
            rd,
            rn,
            op2,
            set_flags,
        } => {
            let (rhs, carry) = structured_operand2(out, op2);
            let lhs = format!("rt.read_reg({rn})");
            match op {
                ArmDataOp::And => {
                    let _ = writeln!(
                        out,
                        "    let value = {lhs} & {rhs}; rt.write_reg({rd}, value);"
                    );
                    if set_flags {
                        emit_flags_from_logic(out, "value", &carry);
                    }
                }
                ArmDataOp::Eor => {
                    let _ = writeln!(
                        out,
                        "    let value = {lhs} ^ {rhs}; rt.write_reg({rd}, value);"
                    );
                    if set_flags {
                        emit_flags_from_logic(out, "value", &carry);
                    }
                }
                ArmDataOp::Orr => {
                    let _ = writeln!(
                        out,
                        "    let value = {lhs} | {rhs}; rt.write_reg({rd}, value);"
                    );
                    if set_flags {
                        emit_flags_from_logic(out, "value", &carry);
                    }
                }
                ArmDataOp::Bic => {
                    let _ = writeln!(
                        out,
                        "    let value = {lhs} & !{rhs}; rt.write_reg({rd}, value);"
                    );
                    if set_flags {
                        emit_flags_from_logic(out, "value", &carry);
                    }
                }
                ArmDataOp::Mvn => {
                    let _ = writeln!(out, "    let value = !{rhs}; rt.write_reg({rd}, value);");
                    if set_flags {
                        emit_flags_from_logic(out, "value", &carry);
                    }
                }
                ArmDataOp::Mov => {
                    let _ = writeln!(out, "    rt.write_reg({rd}, {rhs});");
                    if set_flags {
                        emit_flags_from_logic(out, &rhs, &carry);
                    }
                }
                ArmDataOp::Tst => {
                    let _ = writeln!(out, "    let value = {lhs} & {rhs};");
                    emit_flags_from_logic(out, "value", &carry);
                }
                ArmDataOp::Teq => {
                    let _ = writeln!(out, "    let value = {lhs} ^ {rhs};");
                    emit_flags_from_logic(out, "value", &carry);
                }
                ArmDataOp::Add => {
                    let _ = writeln!(out, "    rt.add({rd}, {lhs}, {rhs}, {set_flags});");
                }
                ArmDataOp::Adc => {
                    let _ = writeln!(out, "    rt.adc({rd}, {lhs}, {rhs}, {set_flags});");
                }
                ArmDataOp::Sub => {
                    let _ = writeln!(out, "    rt.sub({rd}, {lhs}, {rhs}, {set_flags});");
                }
                ArmDataOp::Rsb => {
                    let _ = writeln!(out, "    rt.sub({rd}, {rhs}, {lhs}, {set_flags});");
                }
                ArmDataOp::Sbc => {
                    let _ = writeln!(out, "    rt.sbc({rd}, {lhs}, {rhs}, {set_flags});");
                }
                ArmDataOp::Rsc => {
                    let _ = writeln!(out, "    rt.sbc({rd}, {rhs}, {lhs}, {set_flags});");
                }
                ArmDataOp::Cmp => emit_cmp_sub(out, &lhs, &rhs),
                ArmDataOp::Cmn => emit_cmp_add(out, &lhs, &rhs),
            }
        }
        ArmExtended::Multiply {
            rd,
            rn,
            rs,
            rm,
            accumulate,
            set_flags,
        } => {
            let suffix = if accumulate {
                format!(".wrapping_add(rt.read_reg({rn}))")
            } else {
                String::new()
            };
            let _ = writeln!(out, "    let result = rt.read_reg({rm}).wrapping_mul(rt.read_reg({rs})){suffix}; rt.write_reg({rd}, result);");
            if set_flags {
                emit_flags_from_logic(out, "result", "None");
            }
        }
        ArmExtended::MultiplyLong {
            rd_hi,
            rd_lo,
            rs,
            rm,
            signed,
            accumulate,
            set_flags,
        } => {
            let expr = if signed {
                format!("(rt.read_reg({rm}) as i32 as i64).wrapping_mul(rt.read_reg({rs}) as i32 as i64) as u64")
            } else {
                format!("(rt.read_reg({rm}) as u64).wrapping_mul(rt.read_reg({rs}) as u64)")
            };
            let _ = writeln!(out, "    let mut result = {expr};");
            if accumulate {
                let _ = writeln!(out, "    result = result.wrapping_add((u64::from(rt.read_reg({rd_hi})) << 32) | u64::from(rt.read_reg({rd_lo}))); ");
            }
            let _ = writeln!(out, "    rt.write_reg({rd_lo}, result as u32); rt.write_reg({rd_hi}, (result >> 32) as u32);");
            if set_flags {
                emit_flags_from_logic(out, "result as u32", "None");
            }
        }
        ArmExtended::Swap { rd, rn, rm, byte } => {
            let _ = writeln!(out, "    let address = rt.read_reg({rn});");
            if byte {
                let _ = writeln!(out, "    let old = rt.read8(address); rt.write8(address, rt.read_reg({rm}) as u8); rt.write_reg({rd}, old as u32);");
            } else {
                let _ = writeln!(out, "    let old = rt.read32(address); rt.write32(address & !3, rt.read_reg({rm})); rt.write_reg({rd}, old);");
            }
        }
        ArmExtended::HalfwordTransfer {
            load,
            signed,
            halfword,
            rd,
            rn,
            offset,
            pre_index,
            up,
            write_back,
        } => {
            let off = offset.unsigned_abs();
            let _ = writeln!(out, "    let base = rt.read_reg({rn}); let offset = {off}u32; let address = if {pre_index} {{ if {up} {{ base.wrapping_add(offset) }} else {{ base.wrapping_sub(offset) }} }} else {{ base }};");
            if load {
                let _ = writeln!(out, "    let mut value = if {halfword} {{ rt.read16(address) as u32 }} else {{ rt.read8(address) as u32 }};");
                if signed {
                    let _ = writeln!(out, "    if {halfword} && value & 0x8000 != 0 {{ value |= 0xffff_0000; }} if !{halfword} && value & 0x80 != 0 {{ value |= 0xffff_ff00; }}");
                }
                let _ = writeln!(out, "    rt.write_reg({rd}, value);");
            } else if halfword {
                let _ = writeln!(out, "    rt.write16(address, rt.read_reg({rd}) as u16);");
            } else {
                let _ = writeln!(out, "    rt.write8(address, rt.read_reg({rd}) as u8);");
            }
            if write_back || !pre_index {
                let _ = writeln!(out, "    rt.write_reg({rn}, if {up} {{ base.wrapping_add(offset) }} else {{ base.wrapping_sub(offset) }});");
            }
        }
        ArmExtended::SingleDataTransfer {
            load,
            byte,
            rd,
            rn,
            offset,
            pre_index,
            up,
            write_back,
        } => {
            let (off, _) = structured_operand2(out, offset);
            let _ = writeln!(out, "    let base = rt.read_reg({rn}); let address = if {pre_index} {{ if {up} {{ base.wrapping_add({off}) }} else {{ base.wrapping_sub({off}) }} }} else {{ base }};");
            if load {
                let _ = writeln!(out, "    rt.write_reg({rd}, if {byte} {{ rt.read8(address) as u32 }} else {{ rt.read32(address) }});");
            } else if byte {
                let _ = writeln!(out, "    rt.write8(address, rt.read_reg({rd}) as u8);");
            } else {
                let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({rd}));");
            }
            if write_back || !pre_index {
                let _ = writeln!(out, "    rt.write_reg({rn}, if {up} {{ base.wrapping_add({off}) }} else {{ base.wrapping_sub({off}) }});");
            }
        }
        ArmExtended::BlockTransfer {
            load,
            rn,
            register_list,
            pre_index,
            up,
            write_back,
            ..
        } => {
            let _ = writeln!(out, "    let base = rt.read_reg({rn}); let count = ({register_list:#06x}u32).count_ones(); let mut address = if {up} {{ base.wrapping_add(if {pre_index} {{ 4 }} else {{ 0 }}) }} else {{ base.wrapping_sub(if {pre_index} {{ count * 4 }} else {{ count.saturating_sub(1) * 4 }}) }};");
            let _ = writeln!(out, "    let register_list = {register_list:#06x}u32; for register in 0..16usize {{ if register_list & (1u32 << register) == 0 {{ continue; }}");
            if load {
                let _ = writeln!(out, "        rt.write_reg(register, rt.read32(address));");
            } else {
                let _ = writeln!(
                    out,
                    "        rt.write32(address & !3, rt.read_reg(register));"
                );
            }
            let _ = writeln!(out, "        address = address.wrapping_add(4); }}");
            if write_back {
                let _ = writeln!(out, "    rt.write_reg({rn}, if {up} {{ base.wrapping_add(count * 4) }} else {{ base.wrapping_sub(count * 4) }});");
            }
        }
        ArmExtended::Mrs { rd, spsr } => {
            let _ = writeln!(
                out,
                "    rt.write_reg({rd}, {});",
                if spsr {
                    "rt.cpu.spsr().unwrap_or(rt.cpu.cpsr)"
                } else {
                    "rt.cpu.cpsr"
                }
            );
        }
        ArmExtended::Msr {
            spsr,
            field_mask,
            source,
        } => {
            let (value, _) = structured_operand2(out, source);
            let _ = writeln!(out, "    let value = {value};");
            if spsr {
                let _ = writeln!(out, "    let _ = rt.cpu.set_spsr(value);");
            } else {
                let _ = writeln!(out, "    if rt.mode().privileged() {{ let mask = {field_mask:#04x}u32; let mut cpsr = rt.cpu.cpsr; if mask & 1 != 0 {{ cpsr = (cpsr & !0x0000_00ff) | (value & 0x0000_00ff); }} if mask & 2 != 0 {{ cpsr = (cpsr & !0x0000_ff00) | (value & 0x0000_ff00); }} if mask & 4 != 0 {{ cpsr = (cpsr & !0x00ff_0000) | (value & 0x00ff_0000); }} if mask & 8 != 0 {{ cpsr = (cpsr & !0xff00_0000) | (value & 0xff00_0000); }} rt.cpu.cpsr = cpsr; rt.set_thumb(cpsr & (1 << 5) != 0); }}");
            }
        }
        ArmExtended::SoftwareInterrupt { .. } => {
            let _ = writeln!(out, "    let (target, thumb) = rt.raise_exception(gba_runtime::ExceptionKind::SoftwareInterrupt); return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
        ArmExtended::CoprocessorTransfer { .. }
        | ArmExtended::CoprocessorData { .. }
        | ArmExtended::CoprocessorRegisterTransfer { .. } => {
            let _ = writeln!(
                out,
                "    return Err(\"unsupported coprocessor instruction in specialized codegen\");"
            );
        }
    }
}
