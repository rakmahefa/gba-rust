use std::collections::HashMap;

use crate::cfg::{BlockId, Program};
use crate::function::{FunctionControlFlowGraph, FunctionId};
use crate::ir::{IrControlEffect, IrInstruction, IrMemoryEffect, IrMemoryKind, IrMemoryWidth};

use super::{
    MemoryEffect, MemoryWidth, SemanticBlock, SemanticInstruction, SemanticProgram,
    SemanticTerminator,
};

fn memory_width(width: IrMemoryWidth) -> MemoryWidth {
    match width {
        IrMemoryWidth::Byte => MemoryWidth::Byte,
        IrMemoryWidth::Halfword => MemoryWidth::Halfword,
        IrMemoryWidth::Word => MemoryWidth::Word,
    }
}

fn same_memory(source: Option<IrMemoryEffect>, semantic: Option<MemoryEffect>) -> bool {
    match (source, semantic) {
        (None, None) => true,
        (Some(source), Some(semantic)) => {
            let width = memory_width(source.width);
            match source.kind {
                IrMemoryKind::Read => {
                    semantic
                        == MemoryEffect::Read {
                            width,
                            base: source.base,
                            address_is_dynamic: source.address_is_dynamic,
                        }
                }
                IrMemoryKind::Write => {
                    semantic
                        == MemoryEffect::Write {
                            width,
                            base: source.base,
                            address_is_dynamic: source.address_is_dynamic,
                        }
                }
                IrMemoryKind::ReadWrite => {
                    semantic
                        == MemoryEffect::ReadWrite {
                            width,
                            base: source.base,
                            address_is_dynamic: source.address_is_dynamic,
                        }
                }
            }
        }
        _ => false,
    }
}

fn validate_instruction(
    source: &IrInstruction,
    semantic: &SemanticInstruction,
    block_id: BlockId,
) -> Result<(), String> {
    if source.address != semantic.address || source.size != semantic.size {
        return Err(format!("block {} instruction identity changed", block_id.0));
    }
    if source.control() != semantic.control_effect() {
        return Err(format!(
            "block {} instruction control effect changed",
            block_id.0
        ));
    }
    if source.ops != semantic.ops {
        return Err(format!(
            "block {} instruction operations changed",
            block_id.0
        ));
    }
    if semantic.ops.is_empty() {
        return Err(format!(
            "block {} contains an empty semantic instruction",
            block_id.0
        ));
    }
    if source.reads() != semantic.reads {
        return Err(format!("block {} instruction reads changed", block_id.0));
    }
    if source.writes() != semantic.writes {
        return Err(format!("block {} instruction writes changed", block_id.0));
    }
    let flags = source.flags();
    let semantic_flags = semantic.flags;
    if (
        flags.read_n,
        flags.read_z,
        flags.read_c,
        flags.read_v,
        flags.write_n,
        flags.write_z,
        flags.write_c,
        flags.write_v,
    ) != (
        semantic_flags.read_n,
        semantic_flags.read_z,
        semantic_flags.read_c,
        semantic_flags.read_v,
        semantic_flags.write_n,
        semantic_flags.write_z,
        semantic_flags.write_c,
        semantic_flags.write_v,
    ) {
        return Err(format!(
            "block {} instruction flag effects changed",
            block_id.0
        ));
    }
    if !same_memory(source.memory(), semantic.memory) {
        return Err(format!(
            "block {} instruction memory effects changed",
            block_id.0
        ));
    }
    Ok(())
}

pub(crate) fn validate_control_placement(block: &SemanticBlock) -> Result<(), String> {
    let mut control_index = None;
    for (index, instruction) in block.instructions.iter().enumerate() {
        if matches!(instruction.control_effect(), IrControlEffect::None) {
            continue;
        }
        if control_index.replace(index).is_some() {
            return Err(format!(
                "block {} contains multiple control-effect instructions",
                block.id.0
            ));
        }
    }
    if let Some(index) = control_index {
        if index + 1 != block.instructions.len() {
            return Err(format!(
                "block {} has control effect before its final instruction",
                block.id.0
            ));
        }
    }
    Ok(())
}

fn successor_matches_target(program: &Program, block: &SemanticBlock, target: u32) -> bool {
    block.successors.iter().any(|id| {
        program
            .cfg
            .blocks
            .get(id.0)
            .is_some_and(|successor| successor.key.address == target)
    })
}

fn validate_block_against_source(program: &Program, block: &SemanticBlock) -> Result<(), String> {
    let source = program
        .cfg
        .blocks
        .get(block.id.0)
        .ok_or_else(|| format!("semantic block {} does not exist in source CFG", block.id.0))?;
    if source.id != block.id {
        return Err(format!(
            "semantic block {} has mismatched source identity",
            block.id.0
        ));
    }
    if source.key.address != block.address || source.key.mode != block.mode {
        return Err(format!(
            "block {} address/mode changed during semantic lowering",
            block.id.0
        ));
    }
    if source.instructions.len() != block.instructions.len() {
        return Err(format!(
            "block {} instruction count changed during semantic lowering",
            block.id.0
        ));
    }
    for (source_ir, semantic_instruction) in source.ir.iter().zip(&block.instructions) {
        validate_instruction(source_ir, semantic_instruction, block.id)?;
    }
    if block.instructions.is_empty() {
        return Err(format!(
            "block {} contains no semantic instructions",
            block.id.0
        ));
    }
    validate_control_placement(block)?;
    Ok(())
}

fn validate_successors(program: &Program, block: &SemanticBlock) -> Result<(), String> {
    for successor in &block.successors {
        if successor.0 >= program.cfg.blocks.len() {
            return Err(format!(
                "block {} has invalid successor {}",
                block.id.0, successor.0
            ));
        }
    }
    let expected = match &block.terminator {
        SemanticTerminator::Return
        | SemanticTerminator::IndirectBranch { .. }
        | SemanticTerminator::Unknown => Vec::new(),
        _ => program
            .cfg
            .blocks
            .get(block.id.0)
            .map(|source| source.successors.clone())
            .ok_or_else(|| format!("block {} does not exist in source CFG", block.id.0))?,
    };
    if block.successors != expected {
        return Err(format!(
            "block {} semantic successors differ from source control-flow contract",
            block.id.0
        ));
    }
    Ok(())
}

fn validate_terminator(program: &Program, block: &SemanticBlock) -> Result<(), String> {
    let terminal_effect = block
        .instructions
        .last()
        .map(SemanticInstruction::control_effect)
        .unwrap_or(IrControlEffect::None);
    match (&block.terminator, terminal_effect) {
        (SemanticTerminator::Fallthrough, IrControlEffect::None) => {}
        (
            SemanticTerminator::Branch { target, condition },
            IrControlEffect::Branch {
                target: effect_target,
                condition: effect_condition,
                link: false,
            },
        ) if *target == effect_target && *condition == effect_condition => {
            if !successor_matches_target(program, block, *target) {
                return Err(format!(
                    "branch block {} lost its direct target successor",
                    block.id.0
                ));
            }
        }
        (
            SemanticTerminator::Call { target, condition },
            IrControlEffect::Branch {
                target: effect_target,
                condition: effect_condition,
                link: true,
            },
        ) if *target == effect_target && *condition == effect_condition => {
            if block.successors.is_empty() {
                return Err(format!("call block {} lost its continuation", block.id.0));
            }
        }
        (
            SemanticTerminator::Return,
            IrControlEffect::BranchExchange {
                register: 14,
                link: false,
            },
        ) => {}
        (
            SemanticTerminator::IndirectCall { register, mode },
            IrControlEffect::BranchExchange {
                register: effect_register,
                link: true,
            },
        ) if *register == effect_register && *mode == block.mode => {
            if block.successors.is_empty() {
                return Err(format!(
                    "indirect call block {} lost its continuation",
                    block.id.0
                ));
            }
        }
        (
            SemanticTerminator::IndirectBranch { register },
            IrControlEffect::BranchExchange {
                register: effect_register,
                link: false,
            },
        ) if *register == effect_register => {}
        (SemanticTerminator::SoftwareInterrupt { .. }, IrControlEffect::Unknown) => {
            if block.successors.is_empty() {
                return Err(format!(
                    "software-interrupt block {} lost its continuation",
                    block.id.0
                ));
            }
        }
        (SemanticTerminator::Unknown, IrControlEffect::Unknown) => {}
        _ => {
            return Err(format!(
                "block {} terminator disagrees with terminal control effect",
                block.id.0
            ))
        }
    }
    match block.terminator {
        SemanticTerminator::Return
        | SemanticTerminator::IndirectBranch { .. }
        | SemanticTerminator::Unknown
            if !block.successors.is_empty() =>
        {
            Err(format!("terminating block {} has successors", block.id.0))
        }
        _ => Ok(()),
    }
}

fn validate_function_metadata(
    program: &Program,
    functions: &FunctionControlFlowGraph,
    semantic: &SemanticProgram,
) -> Result<(), String> {
    if semantic.functions.len() != functions.functions.len() {
        return Err("semantic/function count mismatch".into());
    }
    if functions.entry.0 >= functions.functions.len() || semantic.entry != functions.entry {
        return Err("semantic entry function differs from function recovery".into());
    }
    if semantic.entry.0 >= semantic.functions.len() {
        return Err(format!(
            "semantic entry function {} does not exist",
            semantic.entry.0
        ));
    }
    if functions.functions.get(semantic.entry.0).map(|f| f.entry) != Some(program.cfg.entry) {
        return Err("semantic entry function does not own the CFG entry block".into());
    }
    for (index, function) in semantic.functions.iter().enumerate() {
        let source = &functions.functions[index];
        if function.id.0 != index || function.id != source.id {
            return Err(format!(
                "semantic function id {} does not match function recovery",
                function.id.0
            ));
        }
        if function.entry != source.entry {
            return Err(format!("semantic function {} entry changed", function.id.0));
        }
        let semantic_block_ids = function
            .blocks
            .iter()
            .map(|block| block.id)
            .collect::<Vec<_>>();
        if semantic_block_ids != source.blocks {
            return Err(format!(
                "semantic function {} block membership changed",
                function.id.0
            ));
        }
        if function.successors != source.successors {
            return Err(format!(
                "semantic function {} successors changed",
                function.id.0
            ));
        }
        if function.calls != source.call_sites {
            return Err(format!(
                "semantic function {} call metadata changed",
                function.id.0
            ));
        }
        if function.returns != source.return_sites {
            return Err(format!(
                "semantic function {} return metadata changed",
                function.id.0
            ));
        }
        if function.entry.0 >= program.cfg.blocks.len() {
            return Err(format!(
                "invalid semantic function {} entry block",
                function.id.0
            ));
        }
        if function
            .blocks
            .iter()
            .all(|block| block.id != function.entry)
        {
            return Err(format!(
                "function {} does not contain its entry block {}",
                function.id.0, function.entry.0
            ));
        }
        for successor in &function.successors {
            if successor.0 >= semantic.functions.len() {
                return Err(format!(
                    "function {} has invalid function successor {}",
                    function.id.0, successor.0
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_semantic_program(
    program: &Program,
    functions: &FunctionControlFlowGraph,
    semantic: &SemanticProgram,
) -> Result<(), String> {
    validate_function_metadata(program, functions, semantic)?;
    let mut owned = HashMap::<BlockId, FunctionId>::new();
    for function in &semantic.functions {
        for block in &function.blocks {
            validate_block_against_source(program, block)?;
            if owned.insert(block.id, function.id).is_some() {
                return Err(format!(
                    "block {} belongs to multiple functions",
                    block.id.0
                ));
            }
            validate_successors(program, block)?;
            validate_terminator(program, block)?;
        }
    }
    if owned.len() != semantic.block_to_function.len() || owned != semantic.block_to_function {
        return Err("semantic block ownership differs from function recovery".into());
    }
    if !owned.contains_key(&program.cfg.entry) || owned[&program.cfg.entry] != semantic.entry {
        return Err("CFG entry block is not owned by semantic entry function".into());
    }
    for call in semantic
        .functions
        .iter()
        .flat_map(|function| function.calls.iter())
    {
        if call.block.0 >= program.cfg.blocks.len() {
            return Err(format!(
                "call site references invalid block {}",
                call.block.0
            ));
        }
        if call.instruction_index >= program.cfg.blocks[call.block.0].ir.len() {
            return Err(format!(
                "call site references invalid instruction {} in block {}",
                call.instruction_index, call.block.0
            ));
        }
        if let Some(return_block) = call.return_block {
            if !owned.contains_key(&return_block) {
                return Err(format!(
                    "call continuation {} is not owned by a function",
                    return_block.0
                ));
            }
        }
        for return_site in &call.return_sites {
            if return_site.block.0 >= program.cfg.blocks.len()
                || return_site.instruction_index
                    >= program.cfg.blocks[return_site.block.0].instructions.len()
            {
                return Err(format!(
                    "call site references invalid return site {}:{}",
                    return_site.block.0, return_site.instruction_index
                ));
            }
        }
    }
    for return_site in semantic
        .functions
        .iter()
        .flat_map(|function| function.returns.iter())
    {
        if return_site.block.0 >= program.cfg.blocks.len()
            || return_site.instruction_index
                >= program.cfg.blocks[return_site.block.0].instructions.len()
        {
            return Err(format!(
                "return site references invalid instruction {}:{}",
                return_site.block.0, return_site.instruction_index
            ));
        }
    }
    Ok(())
}
