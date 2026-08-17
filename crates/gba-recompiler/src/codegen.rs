use std::fmt::Write;

use crate::cfg::{BlockId, Program};
use crate::decoder::{Condition, Mode};
use crate::ir::IrOp;
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

fn mode_suffix(mode: Mode) -> &'static str { match mode { Mode::Arm => "arm", Mode::Thumb => "thumb" } }
fn mode_bool(mode: Mode) -> bool { matches!(mode, Mode::Thumb) }
fn block_name(block_id: BlockId, mode: Mode, address: u32) -> String { format!("block_{}_{}_{address:08x}", block_id.0, mode_suffix(mode)) }

fn emit_unimplemented(out: &mut String, address: u32, raw: u32, mode: &str) {
    let _ = writeln!(out, "    return Err(\"unimplemented instruction in generated program\"); // {mode} {address:#010x} {raw:#010x}");
}

fn fallthrough_target(block: &SemanticBlock, program: &Program, target: u32) -> Option<(u32, Mode)> {
    block.successors.iter().map(|id| &program.cfg.blocks[id.0]).find(|successor| successor.key.address != target).map(|successor| (successor.key.address, successor.key.mode))
}

fn emit_direct_terminator(out: &mut String, block: &SemanticBlock, program: &Program, target: u32, mode: Mode, condition: Condition, link: bool, ins_address: u32, ins_size: u8) {
    let target_mode = mode_bool(mode);
    let emit_taken = |out: &mut String| {
        if link { let _ = writeln!(out, "        rt.link_from_instruction({ins_address:#010x}, {ins_size}, {target_mode});"); }
        let _ = writeln!(out, "        return Ok(GeneratedBlockExit::continue_to({target:#010x}, {target_mode}));");
    };
    if condition == Condition::Al { emit_taken(out); return; }
    let _ = writeln!(out, "    if rt.condition_code({}) {{", condition_code(condition));
    emit_taken(out);
    let _ = writeln!(out, "    }}");
    if let Some((address, next_mode)) = fallthrough_target(block, program, target) {
        let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({address:#010x}, {}));", mode_bool(next_mode));
    } else {
        let halt = ins_address.wrapping_add(ins_size as u32);
        let _ = writeln!(out, "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {target_mode}));");
    }
}

fn emit_op(out: &mut String, ins_address: u32, ins_raw: u32, ins_size: u8, mode: Mode, op: &IrOp) {
    let _ = writeln!(out, "    rt.enter_instruction({ins_address:#010x}, {});", mode_bool(mode));
    match op {
        IrOp::Nop | IrOp::Mov { .. } | IrOp::Add { .. } | IrOp::Sub { .. } | IrOp::Cmp { .. } | IrOp::Load { .. } | IrOp::Store { .. } | IrOp::ArmExtended { .. } | IrOp::ThumbExtended { .. } => {
            if mode == Mode::Arm {
                let _ = writeln!(out, "    if let Some((target, thumb)) = rt.execute_arm_instruction({ins_raw:#010x}) {{ return Ok(GeneratedBlockExit::continue_to(target, thumb)); }}");
            } else {
                let _ = writeln!(out, "    if let Some((target, thumb)) = rt.execute_thumb_instruction({ins_raw:#06x}) {{ return Ok(GeneratedBlockExit::continue_to(target, thumb)); }}");
            }
        }
        IrOp::Branch { .. } | IrOp::BranchExchange { .. } => unreachable!("terminal control ops must be emitted by the semantic terminator"),
        IrOp::Unknown { address, raw, mode } => emit_unimplemented(out, *address, *raw, match mode { Mode::Arm => "Arm", Mode::Thumb => "Thumb" }),
    }
    let _ = writeln!(out, "    rt.tick(1);");
    let _ = ins_size;
}

fn emit_terminator(out: &mut String, block: &SemanticBlock, program: &Program) {
    let last_instruction = block.instructions.last();
    let (ins_address, ins_size) = last_instruction.map(|instruction| (instruction.address, instruction.size)).unwrap_or((block.address, 0));
    match &block.terminator {
        SemanticTerminator::Return => {
            let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg(14));");
            let _ = writeln!(out, "    return Ok(GeneratedBlockExit::return_to(target, thumb));");
        }
        SemanticTerminator::IndirectBranch { register } => {
            let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register}));");
            let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
        SemanticTerminator::IndirectCall { register, .. } => {
            let _ = writeln!(out, "    rt.link_from_instruction({ins_address:#010x}, {ins_size}, {});", mode_bool(block.mode));
            let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register}));");
            let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
        SemanticTerminator::Branch { condition, target } => emit_direct_terminator(out, block, program, *target, block.mode, *condition, false, ins_address, ins_size),
        SemanticTerminator::Call { condition, target } => emit_direct_terminator(out, block, program, *target, block.mode, *condition, true, ins_address, ins_size),
        SemanticTerminator::Fallthrough => {
            if let Some(successor) = block.successors.first().and_then(|id| program.cfg.blocks.get(id.0)) {
                let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({:#010x}, {}));", successor.key.address, mode_bool(successor.key.mode));
            } else {
                let halt = ins_address.wrapping_add(ins_size as u32);
                let _ = writeln!(out, "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {}));", mode_bool(block.mode));
            }
        }
        SemanticTerminator::Unknown => { let _ = writeln!(out, "    return Err(\"generated program reached an unknown terminator\");"); }
    }
}

fn emit_block(out: &mut String, program: &Program, semantic: &SemanticProgram, block_id: BlockId) {
    let semantic_block = semantic.functions.iter().flat_map(|function| function.blocks.iter()).find(|block| block.id == block_id).unwrap_or_else(|| panic!("semantic block {block_id:?} missing during code generation"));
    let source_block = &program.cfg.blocks[block_id.0];
    let name = block_name(semantic_block.id, semantic_block.mode, semantic_block.address);
    let _ = writeln!(out, "#[inline(always)]");
    let _ = writeln!(out, "fn {name}(rt: &mut Runtime) -> Result<GeneratedBlockExit, &'static str> {{");
    for (index, (instruction, source_ir)) in semantic_block.instructions.iter().zip(&source_block.ir).enumerate() {
        debug_assert_eq!(instruction.address, source_ir.address);
        debug_assert_eq!(instruction.size, source_ir.size);
        let is_terminal = index + 1 == semantic_block.instructions.len();
        for op in &instruction.ops {
            if is_terminal && matches!(op, IrOp::Branch { .. } | IrOp::BranchExchange { .. }) { continue; }
            emit_op(out, instruction.address, source_ir.source_raw, instruction.size, semantic_block.mode, op);
        }
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
    let _ = writeln!(out, "use gba_runtime::{{GeneratedBlockExit, Runtime}};\n");
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
    fn emits_iterative_execution_contract() {
        let rom = [0xE3A0_0001u32, 0xE280_0001u32].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("run_generated_contract"));
        assert!(generated.source.contains("fn dispatch_block"));
        assert!(generated.source.contains("fn is_linked_block"));
        assert!(generated.source.contains("rt.execute_arm_instruction(0xe3a00001)"));
        assert!(generated.source.contains("rt.execute_arm_instruction(0xe2800001)"));
        assert!(generated.source.contains("GeneratedBlockExit::continue_to"));
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
    #[test]
    fn conditional_branch_emits_taken_and_fallthrough_states() {
        let rom = [0x1A00_0000u32, 0xE1A0_0000u32].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("if rt.condition_code(1)"));
        assert!(generated.source.contains("GeneratedBlockExit::continue_to(0x08000008, false)") || generated.source.contains("GeneratedBlockExit::halt"));
    }
    #[test]
    fn exchange_paths_use_non_panicking_runtime_contract() {
        let rom = 0xE12F_FF1Eu32.to_le_bytes().to_vec();
        let program = crate::analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = crate::discover_functions(&program);
        let semantic = crate::build_semantic_program(&program, &functions).unwrap();
        let generated = generate_semantic(&program, &semantic, "entry");
        assert!(generated.source.contains("GeneratedBlockExit::return_to"));
    }
}
