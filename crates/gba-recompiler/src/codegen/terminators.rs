use std::fmt::Write;

use crate::cfg::Program;
use crate::decoder::{Condition, Mode};
use crate::semantic_ir::{SemanticBlock, SemanticTerminator};

use super::common::{condition_code, mode_bool};

fn fallthrough_target(
    block: &SemanticBlock,
    program: &Program,
    target: u32,
) -> Option<(u32, Mode)> {
    block
        .successors
        .iter()
        .map(|id| &program.cfg.blocks[id.0])
        .find(|successor| successor.key.address != target)
        .map(|successor| (successor.key.address, successor.key.mode))
}

fn source_successor(program: &Program, block: &SemanticBlock) -> Option<(u32, Mode)> {
    let successors = &program.cfg.blocks[block.id.0].successors;
    (successors.len() == 1).then(|| {
        let successor = &program.cfg.blocks[successors[0].0];
        (successor.key.address, successor.key.mode)
    })
}

fn indirect_call_target(program: &Program, block: &SemanticBlock) -> Option<(u32, Mode)> {
    let source = &program.cfg.blocks[block.id.0];
    let next = block
        .instructions
        .last()
        .map(|instruction| instruction.address.wrapping_add(instruction.size as u32));
    source
        .successors
        .iter()
        .map(|id| &program.cfg.blocks[id.0])
        .find(|successor| Some(successor.key.address) != next)
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
        let _ = writeln!(
            out,
            "    rt.link_from_instruction({address:#010x}, {size}, {thumb});"
        );
    }
    if condition == Condition::Al {
        let _ = writeln!(
            out,
            "    return Ok(GeneratedBlockExit::continue_to({target:#010x}, {thumb}));"
        );
        return;
    }
    let fallthrough = fallthrough_target(block, program, target);
    let _ = writeln!(
        out,
        "    let branch_taken = rt.condition_code({});",
        condition_code(condition)
    );
    let _ = writeln!(
        out,
        "    if std::env::var(\"GBA_GENERATED_TRACE\").is_ok() {{ eprintln!(\"[generated-branch] source={address:#010x}/{mode:?} condition={:?} nzcv={{:?}} taken={{}} target={target:#010x} fallthrough={:?}\", rt.nzcv(), branch_taken); }}",
        condition,
        fallthrough
    );
    let _ = writeln!(out, "    if branch_taken {{ return Ok(GeneratedBlockExit::continue_to({target:#010x}, {thumb})); }}");
    if let Some((address, next_mode)) = fallthrough {
        let _ = writeln!(
            out,
            "    return Ok(GeneratedBlockExit::continue_to({address:#010x}, {}));",
            mode_bool(next_mode)
        );
    } else {
        let halt = address.wrapping_add(size as u32);
        let _ = writeln!(
            out,
            "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {thumb}));"
        );
    }
}

fn emit_software_interrupt(
    out: &mut String,
    block: &SemanticBlock,
    program: &Program,
    comment: u32,
) {
    let thumb = mode_bool(block.mode);
    let (address, size) = block
        .instructions
        .last()
        .map(|instruction| (instruction.address, instruction.size))
        .unwrap_or((block.address, 0));
    let next_pc = address.wrapping_add(size as u32);

    let _ = writeln!(
        out,
        "    let bios_result = rt.execute_bios_swi_comment({comment:#010x}, {thumb}).map_err(|error| -> &'static str {{ Box::<str>::leak(error.into_boxed_str()) as &'static str }})?;"
    );
    let _ = writeln!(out, "    if bios_result.returned {{");
    if let Some((target, target_mode)) = source_successor(program, block) {
        let _ = writeln!(
            out,
            "        return Ok(GeneratedBlockExit::continue_to({target:#010x}, {}));",
            mode_bool(target_mode)
        );
    } else {
        let _ = writeln!(
            out,
            "        return Ok(GeneratedBlockExit::halt({next_pc:#010x}, {thumb}));"
        );
    }
    let _ = writeln!(out, "    }}");
    let _ = writeln!(
        out,
        "    if let Some(next_pc) = bios_result.next_pc {{ return Ok(GeneratedBlockExit::dynamic_to(next_pc, bios_result.next_thumb)); }}"
    );
    let _ = writeln!(
        out,
        "    return Ok(GeneratedBlockExit::halt({next_pc:#010x}, {thumb}));"
    );
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
            if let Some((target, thumb)) = source_successor(program, block) {
                let _ = writeln!(
                    out,
                    "    return Ok(GeneratedBlockExit::continue_to({target:#010x}, {}));",
                    mode_bool(thumb)
                );
            } else {
                let _ = writeln!(out, "    let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register})); eprintln!(\"generated dynamic branch: source={address:#010x} register=r{register} target={{target:#010x}} thumb={{thumb}}\"); return Ok(GeneratedBlockExit::dynamic_to(target, thumb));");
            }
        }
        SemanticTerminator::IndirectCall { register, .. } => {
            if let Some((target, thumb)) = indirect_call_target(program, block) {
                let _ = writeln!(out, "    rt.link_from_instruction({address:#010x}, {size}, {}); return Ok(GeneratedBlockExit::continue_to({target:#010x}, {}));", mode_bool(block.mode), mode_bool(thumb));
            } else {
                let _ = writeln!(out, "    rt.link_from_instruction({address:#010x}, {size}, {}); let (target, thumb) = rt.exchange_target_for_dispatch(rt.read_reg({register})); eprintln!(\"generated dynamic call: source={address:#010x} register=r{register} target={{target:#010x}} thumb={{thumb}}\"); return Ok(GeneratedBlockExit::dynamic_to(target, thumb));", mode_bool(block.mode));
            }
        }
        SemanticTerminator::Branch { condition, target } => {
            emit_direct_terminator(out, block, program, target, condition, false)
        }
        SemanticTerminator::Call { condition, target } => {
            emit_direct_terminator(out, block, program, target, condition, true)
        }
        SemanticTerminator::Fallthrough => {
            if let Some(successor) = block
                .successors
                .first()
                .and_then(|id| program.cfg.blocks.get(id.0))
            {
                let _ = writeln!(
                    out,
                    "    return Ok(GeneratedBlockExit::continue_to({:#010x}, {}));",
                    successor.key.address,
                    mode_bool(successor.key.mode)
                );
            } else {
                let halt = address.wrapping_add(size as u32);
                let _ = writeln!(
                    out,
                    "    return Ok(GeneratedBlockExit::halt({halt:#010x}, {}));",
                    mode_bool(block.mode)
                );
            }
        }
        SemanticTerminator::SoftwareInterrupt { comment } => {
            emit_software_interrupt(out, block, program, comment);
        }
        SemanticTerminator::Unknown => {
            let _ = writeln!(
                out,
                "    return Err(\"generated program reached an unknown terminator\");"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{Mode, ROM_BASE};
    use crate::{analyze, build_semantic_program, discover_functions};

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn resolved_indirect_branch_is_emitted_as_linked_transition() {
        let rom = arm_rom(&[0xE59F_3000, 0xE12F_FF13, ROM_BASE]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| block.terminator == SemanticTerminator::IndirectBranch { register: 3 })
            .expect("resolved indirect branch block");

        let mut generated = String::new();
        emit_terminator(&mut generated, block, &program);

        assert!(generated.contains("GeneratedBlockExit::continue_to(0x08000000, false)"));
        assert!(!generated.contains("GeneratedBlockExit::dynamic_to"));
    }

    #[test]
    fn resolved_indirect_call_keeps_link_and_static_target() {
        let rom = arm_rom(&[
            0xE59F_3000,
            0xE12F_FF33,
            ROM_BASE + 0x0000_000c,
            0xE1A0_0000,
        ]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| {
                matches!(
                    block.terminator,
                    SemanticTerminator::IndirectCall { register: 3, .. }
                )
            })
            .expect("resolved indirect call block");

        let mut generated = String::new();
        emit_terminator(&mut generated, block, &program);

        assert!(generated.contains("rt.link_from_instruction"));
        assert!(generated.contains("GeneratedBlockExit::continue_to"));
        assert!(!generated.contains("GeneratedBlockExit::dynamic_to"));
    }

    #[test]
    fn conditional_branch_emits_runtime_decision_trace() {
        let rom = arm_rom(&[0xAA00_0000, 0xE1A0_0000, 0xE1A0_0000]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = &semantic.functions[0].blocks[0];
        let mut generated = String::new();
        emit_terminator(&mut generated, block, &program);
        assert!(generated.contains("let branch_taken = rt.condition_code(10);"));
        assert!(generated.contains("[generated-branch]"));
        assert!(generated.contains("branch_taken"));
    }

    #[test]
    fn software_interrupt_calls_the_runtime_bios_contract() {
        let rom = arm_rom(&[0xEF00_0002, 0xE1A0_0000]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| matches!(block.terminator, SemanticTerminator::SoftwareInterrupt { comment: 2 }))
            .expect("software interrupt block");

        let mut generated = String::new();
        emit_terminator(&mut generated, block, &program);

        assert!(generated.contains("rt.execute_bios_swi_comment(0x00000002, false).map_err"));
        assert!(generated.contains("as &'static str"));
        assert!(generated.contains("bios_result.returned"));
        assert!(!generated.contains("software interrupt execution is not implemented"));
        assert!(generated.contains("GeneratedBlockExit::dynamic_to(next_pc, bios_result.next_thumb)"));
    }
}
