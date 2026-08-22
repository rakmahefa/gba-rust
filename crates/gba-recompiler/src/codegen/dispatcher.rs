use std::fmt::Write;

use crate::semantic_ir::SemanticProgram;

use super::common::mode_bool;
use super::linking::collect_linked_blocks;

pub fn emit_dispatcher(out: &mut String, semantic: &SemanticProgram) {
    let linked = collect_linked_blocks(semantic);
    let _ = writeln!(
        out,
        "fn dispatch_block(rt: &mut Runtime, address: u32, thumb: bool) -> Result<GeneratedBlockExit, &'static str> {{"
    );
    let _ = writeln!(out, "    if rt.generated_irq_pending() {{ rt.enter_instruction(address, thumb); return Ok(GeneratedBlockExit::exception(gba_runtime::ExceptionKind::Irq)); }}");
    let _ = writeln!(out, "    match (address, thumb) {{");
    for block in &linked {
        let _ = writeln!(
            out,
            "        ({:#010x}, {}) => {}(rt),",
            block.address,
            mode_bool(if block.thumb { crate::decoder::Mode::Thumb } else { crate::decoder::Mode::Arm }),
            block.symbol
        );
    }
    let _ = writeln!(
        out,
        "        _ => Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG),\n    }}\n}}"
    );
}

pub fn emit_linked_predicate(out: &mut String, semantic: &SemanticProgram) {
    let linked = collect_linked_blocks(semantic);
    let _ = writeln!(out, "fn is_linked_block(address: u32, thumb: bool) -> bool {{");
    if linked.is_empty() {
        let _ = writeln!(out, "    false");
    } else {
        let _ = writeln!(out, "    matches!((address, thumb),");
        let mut line = String::from("        ");
        for (index, block) in linked.iter().enumerate() {
            if index > 0 {
                line.push_str(" | ");
            }
            line.push_str(&format!("({:#010x}, {})", block.address, block.thumb));
            if line.len() > 100 {
                let _ = writeln!(out, "{}", line);
                line = String::from("        ");
            }
        }
        let _ = writeln!(out, "{}\n    )", line);
    }
    let _ = writeln!(out, "}}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, build_semantic_program, discover_functions, Mode, ROM_BASE};

    #[test]
    fn dispatcher_and_link_predicate_share_the_same_canonical_targets() {
        let mut rom = Vec::new();
        rom.extend_from_slice(&0xe3a0_0001u32.to_le_bytes());
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let mut dispatcher = String::new();
        let mut predicate = String::new();

        emit_dispatcher(&mut dispatcher, &semantic);
        emit_linked_predicate(&mut predicate, &semantic);

        assert!(dispatcher.contains("(0x08000000, false)"));
        assert!(dispatcher.contains("Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG)"));
        assert!(predicate.contains("(0x08000000, false)"));
    }
}
