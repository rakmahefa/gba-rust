use std::fmt::Write;

use crate::semantic_ir::SemanticProgram;

use super::common::{block_name, mode_bool};

pub fn emit_dispatcher(out: &mut String, semantic: &SemanticProgram) {
    let _ = writeln!(out, "fn dispatch_block(rt: &mut Runtime, address: u32, thumb: bool) -> Result<GeneratedBlockExit, &'static str> {{");
    let _ = writeln!(out, "    match (address, thumb) {{");
    for function in &semantic.functions {
        for block in &function.blocks {
            let name = block_name(block.id, block.mode, block.address);
            let _ = writeln!(out, "        ({:#010x}, {}) => {name}(rt),", block.address, mode_bool(block.mode));
        }
    }
    let _ = writeln!(out, "        _ => Err(gba_runtime::GENERATED_TARGET_OUTSIDE_CFG),\n    }}\n}}");
}

pub fn emit_linked_predicate(out: &mut String, semantic: &SemanticProgram) {
    let _ = writeln!(out, "fn is_linked_block(address: u32, thumb: bool) -> bool {{");
    let _ = writeln!(out, "    matches!((address, thumb),");
    let mut first = true;
    let mut line = String::from("        ");
    for function in &semantic.functions {
        for block in &function.blocks {
            if !first { line.push_str(" | "); }
            line.push_str(&format!("({:#010x}, {})", block.address, mode_bool(block.mode)));
            first = false;
            if line.len() > 100 {
                let _ = writeln!(out, "{}", line);
                line = String::from("        ");
            }
        }
    }
    if line.trim().is_empty() { line = String::from("        _"); }
    let _ = writeln!(out, "{}\n    )\n}}", line);
}
