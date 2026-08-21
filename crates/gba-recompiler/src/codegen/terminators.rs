use std::fmt::Write;

use crate::cfg::Program;
use crate::decoder::{Condition, Mode};
use crate::semantic_ir::{SemanticBlock, SemanticTerminator};

use super::common::{condition_code, mode_bool};

fn fallthrough_target(block: &SemanticBlock, program: &Program, target: u32) -> Option<(u32, Mode)> {
    block
        .successors
        .iter()
        .map(|id| &program.cfg.blocks[id.0])
        .find(|successor| successor.key.address != target)
        .map(|successor| (successor.key.address, successor.key.mode))
}

fn emit_direct_terminator(
    out: &mut String,
    block: &SemanticBlock,
    program: &Program,
    target: u32,
    condition: Condition,
    link: bool,
) {
    let mode = block.mode;
    let (address, size) = block
        .instructions
        .last()
        .map(|instruction| (instruction.address, instruction.size))
        .unwrap_or((block.address, 0));
    let thumb = mode_bool(mode);
    if link {
        let _ = writeln!(out, "    rt.link_from_instruction({address:#010x}, {size}, {thumb});");
    }
    if condition == Condition::Al {
        let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({target:#010x}, {thumb}));");
        return;
    }
    let _ = writeln!(out, "    if rt.condition_code({}) {{ return Ok(GeneratedBlockExit::continue_to({target:#010x}, {thumb})); }}", condition_code(condition));
    if let Some((address, next_mode)) = fallthrough_target(block, program, target) {
        let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({address:#010x}, {}));", mode_bool(next_mode));
    } else {
        let halt = address.wrapping_add(size as u32);
        let _ = writeln!(out, "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {thumb}));");
    }
}

pub fn emit_terminator(out: &mut String, block: &SemanticBlock, program: &Program) {
    let (address, size) = block
        .instructions
        .last()
        .map(|instruction| (instruction.address, instruction.size))
        .unwrap_or((block.address, 0));
    match block.terminator {
        SemanticTerminator::Return => {
            let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg(14)); return Ok(GeneratedBlockExit::return_to(target, thumb));");
        }
        SemanticTerminator::IndirectBranch { register } => {
            let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register})); return Ok(GeneratedBlockExit::continue_to(target, thumb));");
        }
        SemanticTerminator::IndirectCall { register, .. } => {
            let _ = writeln!(out, "    rt.link_from_instruction({address:#010x}, {size}, {}); let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register})); return Ok(GeneratedBlockExit::continue_to(target, thumb));", mode_bool(block.mode));
        }
        SemanticTerminator::Branch { condition, target } => emit_direct_terminator(out, block, program, target, condition, false),
        SemanticTerminator::Call { condition, target } => emit_direct_terminator(out, block, program, target, condition, true),
        SemanticTerminator::Fallthrough => {
            if let Some(successor) = block.successors.first().and_then(|id| program.cfg.blocks.get(id.0)) {
                let _ = writeln!(out, "    return Ok(GeneratedBlockExit::continue_to({:#010x}, {}));", successor.key.address, mode_bool(successor.key.mode));
            } else {
                let halt = address.wrapping_add(size as u32);
                let _ = writeln!(out, "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {}));", mode_bool(block.mode));
            }
        }
        SemanticTerminator::Unknown => {
            let _ = writeln!(out, "    return Err(\"generated program reached an unknown terminator\");");
        }
    }
}
