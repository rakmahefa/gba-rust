use std::fmt::Write;

use crate::cfg::{BlockId, Program};
use crate::decoder::{Condition, Mode};
use crate::ir::{IrOp, Value};
use crate::semantic_ir::{SemanticProgram, SemanticTerminator};

#[derive(Debug, Clone)]
pub struct RustModule { pub source: String }

fn value(v: &Value) -> String {
    match v {
        Value::Reg(r) => format!("rt.read_reg({r})"),
        Value::Imm(v) => format!("{v:#x}"),
    }
}
fn condition_code(condition: Condition) -> u8 { condition as u8 }
fn mode_suffix(mode: Mode) -> &'static str { match mode { Mode::Arm => "arm", Mode::Thumb => "thumb" } }
fn mode_bool(mode: Mode) -> bool { matches!(mode, Mode::Thumb) }
fn block_name(block_id: BlockId, mode: Mode, address: u32) -> String { format!("block_{}_{}_{address:08x}", block_id.0, mode_suffix(mode),) }

fn emit_unimplemented(out: &mut String, address: u32, raw: u32, mode: &str) {
    let _ = writeln!(out, "    rt.unimplemented({address:#010x}, {raw:#010x}, \"{mode}\");");
}

fn emit_op(out: &mut String, ins_address: u32, ins_size: u8, mode: Mode, op: &IrOp) {
    let _ = writeln!(out, "    rt.enter_instruction({ins_address:#010x}, {});", mode_bool(mode));
    match op {
        IrOp::Nop => {}
        IrOp::Mov { dst, src, set_flags } => { let _ = writeln!(out, "    rt.mov({dst}, {}, {set_flags});", value(src)); }
        IrOp::Add { dst, lhs, rhs, set_flags } => { let _ = writeln!(out, "    rt.add({dst}, rt.read_reg({lhs}), {}, {set_flags});", value(rhs)); }
        IrOp::Sub { dst, lhs, rhs, set_flags } => { let _ = writeln!(out, "    rt.sub({dst}, rt.read_reg({lhs}), {}, {set_flags});", value(rhs)); }
        IrOp::Cmp { lhs, rhs } => { let _ = writeln!(out, "    rt.compare(rt.read_reg({lhs}), {});", value(rhs)); }
        IrOp::Load { dst, base, offset, byte } => {
            let address = format!("rt.read_reg({base}).wrapping_add({offset}i32 as u32)");
            if *byte { let _ = writeln!(out, "    rt.write_reg({dst}, rt.read8({address}) as u32);"); }
            else { let _ = writeln!(out, "    rt.write_reg({dst}, rt.read32({address}));"); }
        }
        IrOp::Store { src, base, offset, byte } => {
            let address = format!("rt.read_reg({base}).wrapping_add({offset}i32 as u32)");
            if *byte { let _ = writeln!(out, "    rt.write8({address}, rt.read_reg({src}) as u8);"); }
            else { let _ = writeln!(out, "    rt.write32({address}, rt.read_reg({src}));"); }
        }
        IrOp::Branch { target, condition, link } => {
            if *link { let _ = writeln!(out, "    rt.link_from_instruction({ins_address:#010x}, {ins_size}, {});", mode_bool(mode)); }
            if *condition == Condition::Al { let _ = writeln!(out, "    return rt.dispatch_mode({target:#010x}, {});", mode_bool(mode)); }
            else { let _ = writeln!(out, "    if rt.condition_code({}) {{ return rt.dispatch_mode({target:#010x}, {}); }}", condition_code(*condition), mode_bool(mode)); }
        }
        IrOp::BranchExchange { register, link } => {
            if *link { let _ = writeln!(out, "    rt.link_from_instruction({ins_address:#010x}, {ins_size}, {});", mode_bool(mode)); }
            let _ = writeln!(out, "    return rt.dispatch_exchange(rt.read_reg({register}));");
        }
        IrOp::ArmExtended { .. } => emit_unimplemented(out, ins_address, 0, "ArmExtended"),
        IrOp::ThumbExtended { .. } => emit_unimplemented(out, ins_address, 0, "ThumbExtended"),
        IrOp::Unknown { address, raw, mode } => emit_unimplemented(out, *address, *raw, match mode { Mode::Arm => "Arm", Mode::Thumb => "Thumb" }),
    }
    let _ = writeln!(out, "    rt.tick(1);");
}

fn emit_successor(out: &mut String, program: &Program, successor: BlockId) {
    let next = &program.cfg.blocks[successor.0];
    let name = block_name(next.id, next.key.mode, next.key.address);
    let _ = writeln!(out, "    return {name}(rt);");
}

fn emit_block(out: &mut String, program: &Program, semantic: &SemanticProgram, block_id: BlockId) {
    let semantic_block = semantic.functions.iter().flat_map(|function| function.blocks.iter()).find(|block| block.id == block_id).unwrap_or_else(|| panic!("semantic block {block_id:?} missing during code generation"));
    let name = block_name(semantic_block.id, semantic_block.mode, semantic_block.address);
    let _ = writeln!(out, "#[inline(always)]");
    let _ = writeln!(out, "pub fn {name}(rt: &mut Runtime) -> ! {{");
    for instruction in &semantic_block.instructions { for op in &instruction.ops { emit_op(out, instruction.address, instruction.size, semantic_block.mode, op); } }

    match &semantic_block.terminator {
        SemanticTerminator::Return => { let _ = writeln!(out, "    return rt.dispatch_exchange(rt.read_reg(14));"); }
        SemanticTerminator::IndirectBranch { .. } | SemanticTerminator::IndirectCall { .. } => { let _ = writeln!(out, "    unreachable!(\"dynamic branch terminator emitted above\");"); }
        SemanticTerminator::Branch { condition, target } | SemanticTerminator::Call { condition, target } => {
            if *condition != Condition::Al {
                if let Some(next) = semantic_block.successors.iter().copied().find(|id| program.cfg.blocks[id.0].key.address != *target) { emit_successor(out, program, next); }
                else { let _ = writeln!(out, "    return rt.halt();"); }
            }
        }
        SemanticTerminator::Fallthrough => {
            if let Some(next) = semantic_block.successors.first().copied() { emit_successor(out, program, next); }
            else { let _ = writeln!(out, "    return rt.halt();"); }
        }
        SemanticTerminator::Unknown => { let _ = writeln!(out, "    return rt.halt();"); }
    }
    let _ = writeln!(out, "}}\n");
}

pub fn generate_semantic(program: &Program, semantic: &SemanticProgram, module_name: &str) -> RustModule {
    assert!(!semantic.functions.is_empty(), "cannot generate an empty semantic program");
    let mut out = String::new();
    let _ = writeln!(out, "// @generated by gba-recompiler; do not edit.\n");
    let _ = writeln!(out, "use gba_runtime::Runtime;\n");
    let entry = &semantic.functions[semantic.entry.0];
    let entry_block = program.cfg.blocks.get(entry.entry.0).expect("semantic entry block missing");
    let entry_name = block_name(entry_block.id, entry_block.key.mode, entry_block.key.address);
    let _ = writeln!(out, "pub fn {module_name}(rt: &mut Runtime) -> ! {{ {entry_name}(rt) }}\n");
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
    fn emits_runtime_contract_calls() {
        let rom = [0xE3A0_0001u32, 0xE280_0001u32].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("rt.enter_instruction(0x08000000, false)"));
        assert!(generated.source.contains("rt.mov(0, 0x1, false)"));
        assert!(generated.source.contains("rt.add(0, rt.read_reg(0), 0x1, false)"));
        assert!(generated.source.contains("rt.tick(1)"));
    }

    #[test]
    fn conditional_branch_uses_runtime_flags_and_static_fallthrough() {
        let rom = [0x0A00_0000u32, 0xE1A0_0000u32].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("rt.condition_code(0)"));
    }

    #[test]
    fn return_and_exchange_use_concrete_runtime_dispatch() {
        let rom = 0xE12F_FF1Eu32.to_le_bytes().to_vec();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("rt.dispatch_exchange(rt.read_reg(14))"));
    }
}
