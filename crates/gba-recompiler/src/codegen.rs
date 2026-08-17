use std::fmt::Write;

use crate::cfg::{BlockId, Program};
use crate::decoder::{ArmDataOp, ArmExtended, Condition, Mode, Operand2, ThumbAluOp, ThumbExtended};
use crate::ir::{IrOp, Value};
use crate::semantic_ir::{SemanticBlock, SemanticProgram, SemanticTerminator};

#[derive(Debug, Clone)]
pub struct RustModule { pub source: String }

fn condition_code(condition: Condition) -> u8 {
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

fn mode_bool(mode: Mode) -> bool { matches!(mode, Mode::Thumb) }

fn block_name(block_id: BlockId, mode: Mode, address: u32) -> String {
    format!("block_{}_{}_{address:08x}", block_id.0, if mode_bool(mode) { "thumb" } else { "arm" })
}

fn value_expr(value: &Value) -> String {
    match value {
        Value::Reg(reg) => format!("rt.read_reg({reg})"),
        Value::Imm(value) => format!("{value:#010x}"),
    }
}

fn emit_flags_from_logic(out: &mut String, value: &str, carry: &str) {
    let _ = writeln!(out, "    let old = rt.nzcv();");
    let _ = writeln!(out, "    rt.set_flags(gba_runtime::Nzcv::new({value} & 0x8000_0000 != 0, {value} == 0, {carry}.unwrap_or(old.c), old.v));");
}

fn emit_cmp_add(out: &mut String, lhs: &str, rhs: &str) {
    let _ = writeln!(out, "    let lhs_value = {lhs};");
    let _ = writeln!(out, "    let rhs_value = {rhs};");
    let _ = writeln!(out, "    let result = lhs_value.wrapping_add(rhs_value);");
    let _ = writeln!(out, "    let carry = result < lhs_value;");
    let _ = writeln!(out, "    let overflow = ((lhs_value ^ result) & (rhs_value ^ result) & 0x8000_0000) != 0;");
    let _ = writeln!(out, "    rt.set_flags(gba_runtime::Nzcv::new(result & 0x8000_0000 != 0, result == 0, carry, overflow));");
}

fn emit_cmp_sub(out: &mut String, lhs: &str, rhs: &str) {
    let _ = writeln!(out, "    let lhs_value = {lhs};");
    let _ = writeln!(out, "    let rhs_value = {rhs};");
    let _ = writeln!(out, "    let result = lhs_value.wrapping_sub(rhs_value);");
    let _ = writeln!(out, "    let carry = lhs_value >= rhs_value;");
    let _ = writeln!(out, "    let overflow = ((lhs_value ^ rhs_value) & (lhs_value ^ result) & 0x8000_0000) != 0;");
    let _ = writeln!(out, "    rt.set_flags(gba_runtime::Nzcv::new(result & 0x8000_0000 != 0, result == 0, carry, overflow));");
}

fn arm_operand2(out: &mut String, raw: u32) -> (String, String) {
    if raw & (1 << 25) != 0 {
        let imm = raw & 0xff;
        let rotate = ((raw >> 8) & 0xf) * 2;
        if rotate == 0 {
            (format!("{imm:#010x}"), "None".into())
        } else {
            (
                format!("({imm:#010x}).rotate_right({rotate})"),
                format!("Some((({imm:#010x}).rotate_right({rotate}) & 0x8000_0000) != 0)"),
            )
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
        let _ = writeln!(out, "    let shifted = rt.shift(rt.read_reg({rm}), {kind}, {amount}, {by_register});");
        ("shifted.value".into(), "Some(shifted.carry)".into())
    }
}

fn structured_operand2(out: &mut String, operand: Operand2) -> (String, String) {
    match operand {
        Operand2::Imm(value) => (format!("{value:#010x}"), "None".into()),
        Operand2::Reg { rm, shift, shift_kind, by_register, shift_register } => {
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
            let _ = writeln!(out, "    let shifted = rt.shift(rt.read_reg({rm}), {kind}, {amount}, {by_register});");
            ("shifted.value".into(), "Some(shifted.carry)".into())
        }
    }
}

fn emit_arm_extended(out: &mut String, op: ArmExtended) {
    match op {
        ArmExtended::DataProcessing { op, rd, rn, op2, set_flags } => {
            let (rhs, carry) = structured_operand2(out, op2);
            let lhs = format!("rt.read_reg({rn})");
            match op {
                ArmDataOp::And => {
                    let _ = writeln!(out, "    let value = {lhs} & {rhs};");
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                    if set_flags { emit_flags_from_logic(out, "value", &carry); }
                }
                ArmDataOp::Eor => {
                    let _ = writeln!(out, "    let value = {lhs} ^ {rhs};");
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                    if set_flags { emit_flags_from_logic(out, "value", &carry); }
                }
                ArmDataOp::Orr => {
                    let _ = writeln!(out, "    let value = {lhs} | {rhs};");
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                    if set_flags { emit_flags_from_logic(out, "value", &carry); }
                }
                ArmDataOp::Bic => {
                    let _ = writeln!(out, "    let value = {lhs} & !{rhs};");
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                    if set_flags { emit_flags_from_logic(out, "value", &carry); }
                }
                ArmDataOp::Mvn => {
                    let _ = writeln!(out, "    let value = !{rhs};");
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                    if set_flags { emit_flags_from_logic(out, "value", &carry); }
                }
                ArmDataOp::Mov => {
                    let _ = writeln!(out, "    rt.write_reg({rd}, {rhs});");
                    if set_flags { emit_flags_from_logic(out, &rhs, &carry); }
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
        ArmExtended::Multiply { rd, rn, rs, rm, accumulate, set_flags } => {
            let _ = writeln!(out, "    let result = rt.read_reg({rm}).wrapping_mul(rt.read_reg({rs})){};", if accumulate { format!(".wrapping_add(rt.read_reg({rn}))") } else { String::new() });
            let _ = writeln!(out, "    rt.write_reg({rd}, result);");
            if set_flags { emit_flags_from_logic(out, "result", "None"); }
        }
        ArmExtended::MultiplyLong { rd_hi, rd_lo, rs, rm, signed, accumulate, set_flags } => {
            let expr = if signed {
                format!("(rt.read_reg({rm}) as i32 as i64).wrapping_mul(rt.read_reg({rs}) as i32 as i64) as u64")
            } else {
                format!("(rt.read_reg({rm}) as u64).wrapping_mul(rt.read_reg({rs}) as u64)")
            };
            let _ = writeln!(out, "    let mut result = {expr};");
            if accumulate {
                let _ = writeln!(out, "    result = result.wrapping_add((u64::from(rt.read_reg({rd_hi})) << 32) | u64::from(rt.read_reg({rd_lo}))); ");
            }
            let _ = writeln!(out, "    rt.write_reg({rd_lo}, result as u32);");
            let _ = writeln!(out, "    rt.write_reg({rd_hi}, (result >> 32) as u32);");
            if set_flags { emit_flags_from_logic(out, "result as u32", "None"); }
        }
        ArmExtended::Swap { rd, rn, rm, byte } => {
            let _ = writeln!(out, "    let address = rt.read_reg({rn});");
            if byte {
                let _ = writeln!(out, "    let old = rt.read8(address); rt.write8(address, rt.read_reg({rm}) as u8); rt.write_reg({rd}, old as u32);");
            } else {
                let _ = writeln!(out, "    let old = rt.read32(address); rt.write32(address & !3, rt.read_reg({rm})); rt.write_reg({rd}, old);");
            }
        }
        ArmExtended::HalfwordTransfer { load, signed, halfword, rd, rn, offset, pre_index, up, write_back } => {
            let off = offset.unsigned_abs();
            let _ = writeln!(out, "    let base = rt.read_reg({rn});");
            let _ = writeln!(out, "    let offset = {off}u32;");
            let _ = writeln!(out, "    let address = if {pre_index} {{ if {up} {{ base.wrapping_add(offset) }} else {{ base.wrapping_sub(offset) }} }} else {{ base }};");
            if load {
                let _ = writeln!(out, "    let mut value = if {halfword} {{ rt.read16(address) as u32 }} else {{ rt.read8(address) as u32 }};");
                if signed {
                    let _ = writeln!(out, "    if {halfword} && value & 0x8000 != 0 {{ value |= 0xffff_0000; }}");
                    let _ = writeln!(out, "    if !{halfword} && value & 0x80 != 0 {{ value |= 0xffff_ff00; }}");
                }
                if rd == 15 {
                    let _ = writeln!(out, "    rt.write_reg(15, value & !3); rt.tick(1); return Ok(GeneratedBlockExit::continue_to(value & !3, false));");
                } else {
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                }
            } else if halfword {
                let _ = writeln!(out, "    rt.write16(address, rt.read_reg({rd}) as u16);");
            } else {
                let _ = writeln!(out, "    rt.write8(address, rt.read_reg({rd}) as u8);");
            }
            if write_back || !pre_index { let _ = writeln!(out, "    rt.write_reg({rn}, if {up} {{ base.wrapping_add(offset) }} else {{ base.wrapping_sub(offset) }});"); }
        }
        ArmExtended::SingleDataTransfer { load, byte, rd, rn, offset, pre_index, up, write_back } => {
            let (off, _) = structured_operand2(out, offset);
            let _ = writeln!(out, "    let base = rt.read_reg({rn});");
            let _ = writeln!(out, "    let address = if {pre_index} {{ if {up} {{ base.wrapping_add({off}) }} else {{ base.wrapping_sub({off}) }} }} else {{ base }};");
            if load {
                let _ = writeln!(out, "    let value = if {byte} {{ rt.read8(address) as u32 }} else {{ rt.read32(address) }};");
                if rd == 15 {
                    let _ = writeln!(out, "    rt.write_reg(15, value & !3); rt.tick(1); return Ok(GeneratedBlockExit::continue_to(value & !3, false));");
                } else {
                    let _ = writeln!(out, "    rt.write_reg({rd}, value);");
                }
            } else if byte {
                let _ = writeln!(out, "    rt.write8(address, rt.read_reg({rd}) as u8);");
            } else {
                let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({rd}));");
            }
            if write_back || !pre_index { let _ = writeln!(out, "    rt.write_reg({rn}, if {up} {{ base.wrapping_add({off}) }} else {{ base.wrapping_sub({off}) }});"); }
        }
        ArmExtended::BlockTransfer { load, rn, register_list, pre_index, up, write_back, .. } => {
            let _ = writeln!(out, "    let base = rt.read_reg({rn});");
            let _ = writeln!(out, "    let count = ({register_list:#06x}u32).count_ones();");
            let _ = writeln!(out, "    let mut address = if {up} {{ base.wrapping_add(if {pre_index} {{ 4 }} else {{ 0 }}) }} else {{ base.wrapping_sub(if {pre_index} {{ count * 4 }} else {{ count.saturating_sub(1) * 4 }}) }};");
            let _ = writeln!(out, "    let register_list = {register_list:#06x}u32;");
            let _ = writeln!(out, "    for register in 0..16usize {{ if register_list & (1u32 << register) == 0 {{ continue; }}");
            if load { let _ = writeln!(out, "        rt.write_reg(register, rt.read32(address));"); } else { let _ = writeln!(out, "        rt.write32(address & !3, rt.read_reg(register));"); }
            let _ = writeln!(out, "        address = address.wrapping_add(4); }}");
            if write_back { let _ = writeln!(out, "    rt.write_reg({rn}, if {up} {{ base.wrapping_add(count * 4) }} else {{ base.wrapping_sub(count * 4) }});"); }
            if load && register_list & (1 << 15) != 0 { let _ = writeln!(out, "    let target = rt.read_reg(15) & !3; rt.tick(1); return Ok(GeneratedBlockExit::continue_to(target, false));"); }
        }
        ArmExtended::Mrs { rd, spsr } => {
            let _ = writeln!(out, "    rt.write_reg({rd}, {});", if spsr { "rt.cpu.spsr().unwrap_or(rt.cpu.cpsr)" } else { "rt.cpu.cpsr" });
        }
        ArmExtended::Msr { spsr, field_mask, source } => {
            let (value, _) = structured_operand2(out, source);
            let _ = writeln!(out, "    let value = {value};");
            if spsr {
                let _ = writeln!(out, "    let _ = rt.cpu.set_spsr(value);");
            } else {
                let _ = writeln!(out, "    if rt.mode().privileged() {{ let mask = {field_mask:#04x}u32; let mut cpsr = rt.cpu.cpsr; if mask & 1 != 0 {{ cpsr = (cpsr & !0x0000_00ff) | (value & 0x0000_00ff); }} if mask & 2 != 0 {{ cpsr = (cpsr & !0x0000_ff00) | (value & 0x0000_ff00); }} if mask & 4 != 0 {{ cpsr = (cpsr & !0x00ff_0000) | (value & 0x00ff_0000); }} if mask & 8 != 0 {{ cpsr = (cpsr & !0xff00_0000) | (value & 0xff00_0000); }} rt.cpu.cpsr = cpsr; rt.set_thumb(cpsr & (1 << 5) != 0); }}");
            }
        }
        ArmExtended::SoftwareInterrupt { .. } => {
            let _ = writeln!(out, "    let (target, thumb) = rt.raise_exception(gba_runtime::ExceptionKind::SoftwareInterrupt); rt.tick(1); return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
        ArmExtended::CoprocessorTransfer { .. } | ArmExtended::CoprocessorData { .. } | ArmExtended::CoprocessorRegisterTransfer { .. } => {
            let _ = writeln!(out, "    return Err(\"unsupported coprocessor instruction in specialized codegen\");");
        }
    }
}

fn emit_thumb_extended(out: &mut String, op: ThumbExtended) {
    match op {
        ThumbExtended::MoveShifted { kind, rd, rs, offset } => {
            let shift_kind = match kind { 0 => "ShiftKind::Lsl", 1 => "ShiftKind::Lsr", _ => "ShiftKind::Asr" };
            let _ = writeln!(out, "    let shifted = rt.shift(rt.read_reg({rs}), {shift_kind}, {offset}, false); rt.write_reg({rd}, shifted.value);");
            emit_flags_from_logic(out, "shifted.value", "Some(shifted.carry)");
        }
        ThumbExtended::AddSubRegister { sub, rd, rs, rn } => {
            if sub { let _ = writeln!(out, "    rt.sub({rd}, rt.read_reg({rs}), rt.read_reg({rn}), true);"); }
            else { let _ = writeln!(out, "    rt.add({rd}, rt.read_reg({rs}), rt.read_reg({rn}), true);"); }
        }
        ThumbExtended::AddSubImmediate { sub, rd, rs, imm } => {
            if sub { let _ = writeln!(out, "    rt.sub({rd}, rt.read_reg({rs}), {imm}, true);"); }
            else { let _ = writeln!(out, "    rt.add({rd}, rt.read_reg({rs}), {imm}, true);"); }
        }
        ThumbExtended::Alu { op, rd, rs } => {
            let lhs = format!("rt.read_reg({rd})");
            let rhs = format!("rt.read_reg({rs})");
            match op {
                ThumbAluOp::And => { let _ = writeln!(out, "    let value = {lhs} & {rhs}; rt.write_reg({rd}, value);"); emit_flags_from_logic(out, "value", "None"); }
                ThumbAluOp::Eor => { let _ = writeln!(out, "    let value = {lhs} ^ {rhs}; rt.write_reg({rd}, value);"); emit_flags_from_logic(out, "value", "None"); }
                ThumbAluOp::Orr => { let _ = writeln!(out, "    let value = {lhs} | {rhs}; rt.write_reg({rd}, value);"); emit_flags_from_logic(out, "value", "None"); }
                ThumbAluOp::Bic => { let _ = writeln!(out, "    let value = {lhs} & !{rhs}; rt.write_reg({rd}, value);"); emit_flags_from_logic(out, "value", "None"); }
                ThumbAluOp::Mvn => { let _ = writeln!(out, "    let value = !{rhs}; rt.write_reg({rd}, value);"); emit_flags_from_logic(out, "value", "None"); }
                ThumbAluOp::Neg => { let _ = writeln!(out, "    rt.sub({rd}, 0, {rhs}, true);"); }
                ThumbAluOp::Adc => { let _ = writeln!(out, "    rt.adc({rd}, {lhs}, {rhs}, true);"); }
                ThumbAluOp::Sbc => { let _ = writeln!(out, "    rt.sbc({rd}, {lhs}, {rhs}, true);"); }
                ThumbAluOp::Cmp => emit_cmp_sub(out, &lhs, &rhs),
                ThumbAluOp::Cmn => emit_cmp_add(out, &lhs, &rhs),
                ThumbAluOp::Tst => { let _ = writeln!(out, "    let value = {lhs} & {rhs};"); emit_flags_from_logic(out, "value", "None"); }
                ThumbAluOp::Lsl | ThumbAluOp::Lsr | ThumbAluOp::Asr | ThumbAluOp::Ror => {
                    let kind = match op { ThumbAluOp::Lsl => "ShiftKind::Lsl", ThumbAluOp::Lsr => "ShiftKind::Lsr", ThumbAluOp::Asr => "ShiftKind::Asr", ThumbAluOp::Ror => "ShiftKind::Ror", _ => unreachable!() };
                    let _ = writeln!(out, "    let shifted = rt.shift({lhs}, {kind}, ({rhs} & 0xff) as u8, true); rt.write_reg({rd}, shifted.value);");
                    emit_flags_from_logic(out, "shifted.value", "Some(shifted.carry)");
                }
                ThumbAluOp::Mul => { let _ = writeln!(out, "    let value = {lhs}.wrapping_mul({rhs}); rt.write_reg({rd}, value);"); emit_flags_from_logic(out, "value", "None"); }
            }
        }
        ThumbExtended::HighRegister { op, rd, rs } => match op {
            0 => { let _ = writeln!(out, "    rt.add({rd}, rt.read_reg({rd}), rt.read_reg({rs}), false);"); }
            1 => emit_cmp_sub(out, &format!("rt.read_reg({rd})"), &format!("rt.read_reg({rs})")),
            2 => { let _ = writeln!(out, "    rt.mov({rd}, rt.read_reg({rs}), false);"); }
            _ => { let _ = writeln!(out, "    return Err(\"unsupported high-register Thumb operation in specialized codegen\");"); }
        },
        ThumbExtended::PcRelativeLoad { rd, word_offset } => {
            let _ = writeln!(out, "    let address = (rt.read_reg(15) & !3).wrapping_add({}u32 * 4); rt.write_reg({rd}, rt.read32(address));", word_offset);
        }
        ThumbExtended::LoadStoreRegister { load, byte, rd, rb, ro } => {
            let _ = writeln!(out, "    let address = rt.read_reg({rb}).wrapping_add(rt.read_reg({ro}));");
            if load { let _ = writeln!(out, "    rt.write_reg({rd}, if {byte} {{ rt.read8(address) as u32 }} else {{ rt.read32(address) }});"); }
            else if byte { let _ = writeln!(out, "    rt.write8(address, rt.read_reg({rd}) as u8);"); }
            else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({rd}));"); }
        }
        ThumbExtended::LoadStoreSignHalf { kind, rd, rb, ro } => {
            let _ = writeln!(out, "    let address = rt.read_reg({rb}).wrapping_add(rt.read_reg({ro}));");
            match kind {
                0 => { let _ = writeln!(out, "    rt.write16(address, rt.read_reg({rd}) as u16);"); }
                1 => { let _ = writeln!(out, "    let value = rt.read8(address) as u32; rt.write_reg({rd}, if value & 0x80 != 0 {{ value | 0xffff_ff00 }} else {{ value }});"); }
                2 => { let _ = writeln!(out, "    rt.write_reg({rd}, rt.read16(address) as u32);"); }
                3 => { let _ = writeln!(out, "    let value = rt.read16(address) as u32; rt.write_reg({rd}, if value & 0x8000 != 0 {{ value | 0xffff_0000 }} else {{ value }});"); }
                _ => { let _ = writeln!(out, "    return Err(\"unsupported sign/halfword Thumb operation in specialized codegen\");"); }
            }
        }
        ThumbExtended::LoadStoreImmediate { load, byte, rd, rb, offset } => {
            let scale = if byte { 1 } else { 4 };
            let _ = writeln!(out, "    let address = rt.read_reg({rb}).wrapping_add({}u32 * {scale});", offset);
            if load { let _ = writeln!(out, "    rt.write_reg({rd}, if {byte} {{ rt.read8(address) as u32 }} else {{ rt.read32(address) }});"); }
            else if byte { let _ = writeln!(out, "    rt.write8(address, rt.read_reg({rd}) as u8);"); }
            else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({rd}));"); }
        }
        ThumbExtended::LoadStoreHalfword { load, rd, rb, offset } => {
            let _ = writeln!(out, "    let address = rt.read_reg({rb}).wrapping_add({}u32 * 2);", offset);
            if load { let _ = writeln!(out, "    rt.write_reg({rd}, rt.read16(address) as u32);"); }
            else { let _ = writeln!(out, "    rt.write16(address, rt.read_reg({rd}) as u16);"); }
        }
        ThumbExtended::SpRelativeLoadStore { load, rd, offset } => {
            let _ = writeln!(out, "    let address = rt.read_reg(13).wrapping_add({}u32 * 4);", offset);
            if load { let _ = writeln!(out, "    rt.write_reg({rd}, rt.read32(address));"); }
            else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({rd}));"); }
        }
        ThumbExtended::Address { rd, use_sp, word_offset } => {
            let _ = writeln!(out, "    rt.write_reg({rd}, (if {use_sp} {{ rt.read_reg(13) }} else {{ rt.read_reg(15) & !3 }}).wrapping_add({}u32 * 4));", word_offset);
        }
        ThumbExtended::AddSp { negative, imm } => {
            if negative { let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_sub({imm}));"); }
            else { let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_add({imm}));"); }
        }
        ThumbExtended::PushPop { load, registers, extra_lr_pc } => {
            let count = registers.count_ones() + u32::from(extra_lr_pc);
            if load {
                let _ = writeln!(out, "    let mut address = rt.read_reg(13);");
                for reg in 0..8u8 {
                    if registers & (1 << reg) != 0 { let _ = writeln!(out, "    rt.write_reg({reg}, rt.read32(address)); address = address.wrapping_add(4);"); }
                }
                if extra_lr_pc { let _ = writeln!(out, "    let target = rt.read32(address) & !3; rt.write_reg(13, rt.read_reg(13).wrapping_add({count} * 4)); rt.tick(1); return Ok(GeneratedBlockExit::continue_to(target, true));"); }
                else { let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_add({count} * 4));"); }
            } else {
                let _ = writeln!(out, "    let mut address = rt.read_reg(13).wrapping_sub({count} * 4);");
                for reg in 0..8u8 {
                    if registers & (1 << reg) != 0 { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({reg})); address = address.wrapping_add(4);"); }
                }
                if extra_lr_pc { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg(14)); address = address.wrapping_add(4);"); }
                let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_sub({count} * 4));");
            }
        }
        ThumbExtended::MultipleLoadStore { load, rb, register_list } => {
            let _ = writeln!(out, "    let mut address = rt.read_reg({rb});");
            for reg in 0..8u8 {
                if register_list & (1 << reg) != 0 {
                    if load { let _ = writeln!(out, "    rt.write_reg({reg}, rt.read32(address));"); }
                    else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({reg}));"); }
                    let _ = writeln!(out, "    address = address.wrapping_add(4);");
                }
            }
            let _ = writeln!(out, "    rt.write_reg({rb}, address);");
        }
        ThumbExtended::SoftwareInterrupt { .. } => {
            let _ = writeln!(out, "    let (target, thumb) = rt.raise_exception(gba_runtime::ExceptionKind::SoftwareInterrupt); rt.tick(1); return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
    }
}

fn emit_inner_op(out: &mut String, ins_raw: u32, mode: Mode, op: &IrOp) {
    match op {
        IrOp::Nop => {}
        IrOp::Mov { dst, src, set_flags } => {
            if mode == Mode::Arm {
                let (rhs, carry) = arm_operand2(out, ins_raw);
                let _ = writeln!(out, "    rt.mov({dst}, {rhs}, false);");
                if *set_flags { emit_flags_from_logic(out, &rhs, &carry); }
            } else {
                let value = value_expr(src);
                let _ = writeln!(out, "    rt.mov({dst}, {value}, {set_flags});");
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
            if *byte { let _ = writeln!(out, "    rt.write_reg({dst}, rt.read8(address) as u32);"); }
            else { let _ = writeln!(out, "    rt.write_reg({dst}, rt.read32(address));"); }
        }
        IrOp::Store { src, base, offset, byte } => {
            let _ = writeln!(out, "    let address = rt.read_reg({base}).wrapping_add({offset}i32 as u32);");
            if *byte { let _ = writeln!(out, "    rt.write8(address, rt.read_reg({src}) as u8);"); }
            else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({src}));"); }
        }
        IrOp::Branch { .. } | IrOp::BranchExchange { .. } => unreachable!("control flow is emitted by the semantic terminator"),
        IrOp::ArmExtended { op } => emit_arm_extended(out, *op),
        IrOp::ThumbExtended { op } => emit_thumb_extended(out, *op),
        IrOp::Unknown { .. } => { let _ = writeln!(out, "    return Err(\"unsupported instruction in specialized codegen\");"); }
    }
}

fn emit_op(out: &mut String, ins_address: u32, ins_raw: u32, mode: Mode, op: &IrOp) {
    let _ = writeln!(out, "    rt.enter_instruction({ins_address:#010x}, {});", mode_bool(mode));
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

fn fallthrough_target(block: &SemanticBlock, program: &Program, target: u32) -> Option<(u32, Mode)> {
    block.successors.iter().map(|id| &program.cfg.blocks[id.0]).find(|successor| successor.key.address != target).map(|successor| (successor.key.address, successor.key.mode))
}

fn emit_direct_terminator(out: &mut String, block: &SemanticBlock, program: &Program, target: u32, mode: Mode, condition: Condition, link: bool, ins_address: u32, ins_size: u8) {
    let target_mode = mode_bool(mode);
    if link { let _ = writeln!(out, "    rt.link_from_instruction({ins_address:#010x}, {ins_size}, {target_mode});"); }
    if condition == Condition::Al { let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({target:#010x}, {target_mode}));"); return; }
    let _ = writeln!(out, "    if rt.condition_code({}) {{ return Ok(GeneratedBlockExit::continue_to({target:#010x}, {target_mode})); }}", condition_code(condition));
    if let Some((address, next_mode)) = fallthrough_target(block, program, target) { let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({address:#010x}, {}));", mode_bool(next_mode)); }
    else { let halt = ins_address.wrapping_add(ins_size as u32); let _ = writeln!(out, "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {target_mode}));"); }
}

fn emit_terminator(out: &mut String, block: &SemanticBlock, program: &Program) {
    let last = block.instructions.last();
    let (address, size) = last.map(|instruction| (instruction.address, instruction.size)).unwrap_or((block.address, 0));
    match block.terminator {
        SemanticTerminator::Return => { let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg(14)); return Ok(GeneratedBlockExit::return_to(target, thumb));"); }
        SemanticTerminator::IndirectBranch { register } => { let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register})); return Ok(GeneratedBlockExit::continue_to(target, thumb));"); }
        SemanticTerminator::IndirectCall { register, .. } => { let _ = writeln!(out, "    rt.link_from_instruction({address:#010x}, {size}, {}); let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register})); return Ok(GeneratedBlockExit::continue_to(target, thumb));", mode_bool(block.mode)); }
        SemanticTerminator::Branch { condition, target } => emit_direct_terminator(out, block, program, target, block.mode, condition, false, address, size),
        SemanticTerminator::Call { condition, target } => emit_direct_terminator(out, block, program, target, block.mode, condition, true, address, size),
        SemanticTerminator::Fallthrough => { if let Some(successor) = block.successors.first().and_then(|id| program.cfg.blocks.get(id.0)) { let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({:#010x}, {}));", successor.key.address, mode_bool(successor.key.mode)); } else { let halt = address.wrapping_add(size as u32); let _ = writeln!(out, "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {}));", mode_bool(block.mode)); } }
        SemanticTerminator::Unknown => { let _ = writeln!(out, "    return Err(\"generated program reached an unknown terminator\");"); }
    }
}

fn emit_block(out: &mut String, program: &Program, semantic: &SemanticProgram, block_id: BlockId) {
    let semantic_block = semantic.functions.iter().flat_map(|function| function.blocks.iter()).find(|block| block.id == block_id).unwrap_or_else(|| panic!("semantic block {block_id:?} missing during code generation"));
    let source_block = &program.cfg.blocks[block_id.0];
    let name = block_name(semantic_block.id, semantic_block.mode, semantic_block.address);
    let _ = writeln!(out, "#[inline(always)]");
    let _ = writeln!(out, "fn {name}(rt: &mut Runtime) -> Result<GeneratedBlockExit, &'static str> {{");
    for (instruction, source_ir) in semantic_block.instructions.iter().zip(&source_block.ir) {
        for op in &instruction.ops { emit_op(out, instruction.address, source_ir.source_raw, semantic_block.mode, op); }
    }
    emit_terminator(out, semantic_block, program);
    let _ = writeln!(out, "}}\n");
}

fn emit_dispatcher(out: &mut String, semantic: &SemanticProgram) {
    let _ = writeln!(out, "fn dispatch_block(rt: &mut Runtime, address: u32, thumb: bool) -> Result<GeneratedBlockExit, &'static str> {{");
    let _ = writeln!(out, "    match (address, thumb) {{");
    for function in &semantic.functions { for block in &function.blocks { let name = block_name(block.id, block.mode, block.address); let _ = writeln!(out, "        ({:#010x}, {}) => {name}(rt),", block.address, mode_bool(block.mode)); } }
    let _ = writeln!(out, "        _ => Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG),");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}\n");
}

fn emit_linked_predicate(out: &mut String, semantic: &SemanticProgram) {
    let _ = writeln!(out, "fn is_linked_block(address: u32, thumb: bool) -> bool {{");
    let _ = writeln!(out, "    matches!((address, thumb),");
    let mut first = true;
    let mut line = String::from("        ");
    for function in &semantic.functions { for block in &function.blocks {
        if !first { line.push_str(" | "); }
        line.push_str(&format!("({:#010x}, {})", block.address, mode_bool(block.mode)));
        first = false;
        if line.len() > 100 { let _ = writeln!(out, "{}", line); line = String::from("        "); }
    }}
    if line.trim().is_empty() { line = String::from("        _"); }
    let _ = writeln!(out, "{}", line);
    let _ = writeln!(out, "    )");
    let _ = writeln!(out, "}}\n");
}

pub fn generate_semantic(program: &Program, semantic: &SemanticProgram, module_name: &str) -> RustModule {
    assert!(!semantic.functions.is_empty(), "cannot generate an empty semantic program");
    let mut out = String::new();
    let _ = writeln!(out, "// @generated by gba-recompiler; do not edit.\n");
    let _ = writeln!(out, "use gba_runtime::{{GeneratedBlockExit, Runtime, ShiftKind}};\n");
    let entry = &semantic.functions[semantic.entry.0];
    let entry_block = program.cfg.blocks.get(entry.entry.0).expect("semantic entry block missing");
    let entry_address = entry_block.key.address;
    let entry_mode = mode_bool(entry_block.key.mode);
    let _ = writeln!(out, "pub fn {module_name}(rt: &mut Runtime) -> Result<gba_runtime::GeneratedExecutionResult, &'static str> {{");
    let _ = writeln!(out, "    <Runtime as gba_runtime::RuntimeContract>::run_generated_contract(rt, {entry_address:#010x}, {entry_mode}, None, dispatch_block, is_linked_block)");
    let _ = writeln!(out, "}}\n");
    let _ = writeln!(out, "pub fn {module_name}_with_limit(rt: &mut Runtime, max_steps: u64) -> Result<gba_runtime::GeneratedExecutionResult, &'static str> {{");
    let _ = writeln!(out, "    <Runtime as gba_runtime::RuntimeContract>::run_generated_contract(rt, {entry_address:#010x}, {entry_mode}, Some(max_steps), dispatch_block, is_linked_block)");
    let _ = writeln!(out, "}}\n");
    emit_dispatcher(&mut out, semantic);
    emit_linked_predicate(&mut out, semantic);
    for function in &semantic.functions { for block in &function.blocks { emit_block(&mut out, program, semantic, block.id); } }
    RustModule { source: out }
}

pub fn generate(program: &Program, module_name: &str) -> RustModule {
    let functions = crate::function::discover_functions(program);
    let semantic = crate::semantic_ir::build_semantic_program(program, &functions).expect("program must satisfy the semantic execution contract before code generation");
    generate_semantic(program, &semantic, module_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{Mode, ROM_BASE};

    #[test]
    fn emits_specialized_operations_instead_of_raw_instruction_execution() {
        let rom = [0xE3A0_0001u32, 0xE280_0001u32].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("rt.mov(0"));
        assert!(generated.source.contains("rt.add(0, rt.read_reg(0)"));
        assert!(!generated.source.contains("execute_arm_instruction"));
        assert!(!generated.source.contains("execute_thumb_instruction"));
    }

    #[test]
    fn specialized_arm_condition_is_guarded() {
        let rom = 0x13A0_0001u32.to_le_bytes().to_vec();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("if rt.condition_code(1)"));
    }

    #[test]
    fn specialized_extended_multiply_has_no_raw_dispatch() {
        let rom = 0xE000_0090u32.to_le_bytes().to_vec();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("wrapping_mul"));
        assert!(!generated.source.contains("execute_arm_instruction"));
    }

    #[test]
    fn self_loop_returns_a_next_state_instead_of_recursive_block_calls() {
        let rom = 0xEAFF_FFFEu32.to_le_bytes().to_vec();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("GeneratedBlockExit::continue_to(0x08000000, false)"));
        assert!(!generated.source.contains("return block_0_arm_08000000(rt)"));
    }
}
