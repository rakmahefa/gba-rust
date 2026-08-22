use std::fmt::Write;

use crate::cfg::{BlockId, Program};
use crate::semantic_ir::SemanticProgram;

use super::common::block_name;
use super::ops::emit_op;
use super::terminators::emit_terminator;

pub fn emit_block(
    out: &mut String,
    program: &Program,
    semantic: &SemanticProgram,
    block_id: BlockId,
) {
    let block = semantic
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .find(|block| block.id == block_id)
        .unwrap_or_else(|| panic!("semantic block {block_id:?} missing"));
    let source = &program.cfg.blocks[block_id.0];
    let name = block_name(block.id, block.mode, block.address);
    let _ = writeln!(out, "#[inline(always)]\nfn {name}(rt: &mut Runtime) -> Result<GeneratedBlockExit, &'static str> {{");
    for (instruction, source_ir) in block.instructions.iter().zip(&source.ir) {
        for op in &instruction.ops {
            emit_op(
                out,
                instruction.address,
                source_ir.source_raw,
                block.mode,
                op,
            );
        }
    }
    emit_terminator(out, block, program);
    let _ = writeln!(out, "}}\n");
}
