use std::fmt::Write;

use crate::decoder::{ThumbAluOp, ThumbExtended};

use super::common::{emit_cmp_add, emit_cmp_sub, emit_flags_from_logic};

pub fn emit_thumb_extended(out: &mut String, op: ThumbExtended) {
    match op {
        ThumbExtended::MoveShifted { kind, rd, rs, offset } => {
            let shift_kind = match kind {
                0 => "gba_runtime::ShiftKind::Lsl",
                1 => "gba_runtime::ShiftKind::Lsr",
                _ => "gba_runtime::ShiftKind::Asr",
            };
            let _ = writeln!(out, "    let shifted = rt.shift(rt.read_reg({rs}), {shift_kind}, {offset}, false); rt.write_reg({rd}, shifted.value);");
            emit_flags_from_logic(out, "shifted.value", "Some(shifted.carry)");
        }
        ThumbExtended::AddSubRegister { sub, rd, rs, rn } => {
            if sub {
                let _ = writeln!(out, "    rt.sub({rd}, rt.read_reg({rs}), rt.read_reg({rn}), true);");
            } else {
                let _ = writeln!(out, "    rt.add({rd}, rt.read_reg({rs}), rt.read_reg({rn}), true);");
            }
        }
        ThumbExtended::AddSubImmediate { sub, rd, rs, imm } => {
            if sub {
                let _ = writeln!(out, "    rt.sub({rd}, rt.read_reg({rs}), {imm}u32, true);");
            } else {
                let _ = writeln!(out, "    rt.add({rd}, rt.read_reg({rs}), {imm}u32, true);");
            }
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
                    let kind = match op {
                        ThumbAluOp::Lsl => "gba_runtime::ShiftKind::Lsl",
                        ThumbAluOp::Lsr => "gba_runtime::ShiftKind::Lsr",
                        ThumbAluOp::Asr => "gba_runtime::ShiftKind::Asr",
                        ThumbAluOp::Ror => "gba_runtime::ShiftKind::Ror",
                        _ => unreachable!(),
                    };
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
            else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({rd}) as u32);"); }
        }
        ThumbExtended::Address { rd, use_sp, word_offset } => {
            let _ = writeln!(out, "    rt.write_reg({rd}, (if {use_sp} {{ rt.read_reg(13) }} else {{ rt.read_reg(15) & !3 }}).wrapping_add({}u32 * 4));", word_offset);
        }
        ThumbExtended::AddSp { negative, imm } => {
            if negative { let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_sub({imm}u32));"); }
            else { let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_add({imm}u32));"); }
        }
        ThumbExtended::PushPop { load, registers, extra_lr_pc } => {
            let count = registers.count_ones() + u32::from(extra_lr_pc);
            if load {
                let load_regs: Vec<u8> = (0..8u8).filter(|reg| registers & (1 << reg) != 0).collect();
                let needs_address = !load_regs.is_empty() || extra_lr_pc;
                let needs_address_mut = load_regs.len() + usize::from(extra_lr_pc) > 1;
                if needs_address {
                    if needs_address_mut {
                        let _ = writeln!(out, "    let mut address = rt.read_reg(13);");
                    } else {
                        let _ = writeln!(out, "    let address = rt.read_reg(13);");
                    }
                    for (index, reg) in load_regs.iter().enumerate() {
                        let next = index + 1 < load_regs.len() || extra_lr_pc;
                        if next { let _ = writeln!(out, "    rt.write_reg({reg}, rt.read32(address)); address = address.wrapping_add(4);"); }
                        else { let _ = writeln!(out, "    rt.write_reg({reg}, rt.read32(address));"); }
                    }
                    if extra_lr_pc {
                        let _ = writeln!(out, "    let target = rt.read32(address); rt.write_reg(13, rt.read_reg(13).wrapping_add({count} * 4)); return Ok(GeneratedBlockExit::continue_to(target & !1, true));");
                    }
                }
                let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_add({count} * 4));");
            } else {
                let store_regs: Vec<u8> = (0..8u8).filter(|reg| registers & (1 << reg) != 0).collect();
                let needs_address = !store_regs.is_empty() || extra_lr_pc;
                let needs_address_mut = store_regs.len() + usize::from(extra_lr_pc) > 1;
                if needs_address {
                    if needs_address_mut {
                        let _ = writeln!(out, "    let mut address = rt.read_reg(13).wrapping_sub({count} * 4);");
                    } else {
                        let _ = writeln!(out, "    let address = rt.read_reg(13).wrapping_sub({count} * 4);");
                    }
                    for (index, reg) in store_regs.iter().enumerate() {
                        let next = index + 1 < store_regs.len() || extra_lr_pc;
                        if next { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({reg})); address = address.wrapping_add(4);"); }
                        else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({reg}));"); }
                    }
                    if extra_lr_pc { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg(14));"); }
                }
                let _ = writeln!(out, "    rt.write_reg(13, rt.read_reg(13).wrapping_sub({count} * 4));");
            }
        }
        ThumbExtended::MultipleLoadStore { load, rb, register_list } => {
            let has_registers = register_list != 0;
            if has_registers {
                let _ = writeln!(out, "    let mut address = rt.read_reg({rb});");
                for reg in 0..8u8 {
                    if register_list & (1 << reg) != 0 {
                        if load { let _ = writeln!(out, "    rt.write_reg({reg}, rt.read32(address));"); }
                        else { let _ = writeln!(out, "    rt.write32(address & !3, rt.read_reg({reg}));"); }
                        let _ = writeln!(out, "    address = address.wrapping_add(4);");
                    }
                }
                let _ = writeln!(out, "    rt.write_reg({rb}, address);");
            } else {
                let _ = writeln!(out, "    rt.write_reg({rb}, rt.read_reg({rb}));");
            }
        }
        ThumbExtended::SoftwareInterrupt { .. } => {
            let _ = writeln!(out, "    let (target, thumb) = rt.raise_exception(gba_runtime::ExceptionKind::SoftwareInterrupt); return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
    }
}
