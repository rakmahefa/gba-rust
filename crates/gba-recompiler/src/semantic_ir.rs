use std::collections::HashMap;

use crate::cfg::{BlockId, Program};
use crate::decoder::{Condition, Mode};
use crate::function::{CallSite, FunctionControlFlowGraph, FunctionId, ReturnSite};
use crate::ir::{IrInstruction, IrMemoryKind, IrMemoryWidth, IrOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryWidth {
    Byte,
    Halfword,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEffect {
    Read { width: MemoryWidth, base: u8 },
    Write { width: MemoryWidth, base: u8 },
    ReadWrite { width: MemoryWidth, base: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagEffect {
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticTerminator {
    Fallthrough,
    Branch { target: u32, condition: Condition },
    Call { target: u32, condition: Condition },
    IndirectCall { register: u8, mode: Mode },
    IndirectBranch { register: u8 },
    Return,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticInstruction {
    pub address: u32,
    pub size: u8,
    pub ops: Vec<IrOp>,
    pub reads: Vec<u8>,
    pub writes: Vec<u8>,
    pub memory: Option<MemoryEffect>,
    pub flags: FlagEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticBlock {
    pub id: BlockId,
    pub address: u32,
    pub mode: Mode,
    pub instructions: Vec<SemanticInstruction>,
    pub successors: Vec<BlockId>,
    pub terminator: SemanticTerminator,
}

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub id: FunctionId,
    pub entry: BlockId,
    pub blocks: Vec<SemanticBlock>,
    pub successors: Vec<FunctionId>,
    pub calls: Vec<CallSite>,
    pub returns: Vec<ReturnSite>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticProgram {
    pub entry: FunctionId,
    pub functions: Vec<SemanticFunction>,
    pub block_to_function: HashMap<BlockId, FunctionId>,
}

fn memory_width(width: IrMemoryWidth) -> MemoryWidth {
    match width {
        IrMemoryWidth::Byte => MemoryWidth::Byte,
        IrMemoryWidth::Halfword => MemoryWidth::Halfword,
        IrMemoryWidth::Word => MemoryWidth::Word,
    }
}

fn semantic_instruction(ir: &IrInstruction) -> SemanticInstruction {
    let memory = ir.memory().map(|memory| match memory.kind {
        IrMemoryKind::Read => MemoryEffect::Read { width: memory_width(memory.width), base: memory.base },
        IrMemoryKind::Write => MemoryEffect::Write { width: memory_width(memory.width), base: memory.base },
        IrMemoryKind::ReadWrite => MemoryEffect::ReadWrite { width: memory_width(memory.width), base: memory.base },
    });
    let flags = ir.flags();
    SemanticInstruction {
        address: ir.address,
        size: ir.size,
        ops: ir.ops.clone(),
        reads: ir.reads(),
        writes: ir.writes(),
        memory,
        flags: FlagEffect { read: flags.reads_any(), write: flags.writes_any() },
    }
}

fn terminator(block: &SemanticBlock) -> SemanticTerminator {
    let Some(instruction) = block.instructions.last() else { return SemanticTerminator::Unknown; };
    let Some(op) = instruction.ops.last() else { return SemanticTerminator::Unknown; };
    match op {
        IrOp::Branch { target, condition, link } if *link => SemanticTerminator::Call { target: *target, condition: *condition },
        IrOp::Branch { target, condition, .. } => SemanticTerminator::Branch { target: *target, condition: *condition },
        IrOp::BranchExchange { register: 14, link: false } => SemanticTerminator::Return,
        IrOp::BranchExchange { register, link: true } => SemanticTerminator::IndirectCall { register: *register, mode: block.mode },
        IrOp::BranchExchange { register, link: false } => SemanticTerminator::IndirectBranch { register: *register },
        IrOp::Unknown { .. }
        | IrOp::ArmExtended { op: crate::decoder::ArmExtended::SoftwareInterrupt { .. } }
        | IrOp::ThumbExtended { op: crate::decoder::ThumbExtended::SoftwareInterrupt { .. } } => SemanticTerminator::Unknown,
        _ => SemanticTerminator::Fallthrough,
    }
}

pub fn build_semantic_program(program: &Program, functions: &FunctionControlFlowGraph) -> Result<SemanticProgram, String> {
    let mut semantic_functions = Vec::with_capacity(functions.functions.len());
    for function in &functions.functions {
        let mut blocks = Vec::with_capacity(function.blocks.len());
        for &block_id in &function.blocks {
            let block = program.cfg.blocks.get(block_id.0).ok_or_else(|| format!("function {} references missing block {}", function.id.0, block_id.0))?;
            let instructions = block.ir.iter().map(semantic_instruction).collect::<Vec<_>>();
            let mut semantic = SemanticBlock { id: block.id, address: block.key.address, mode: block.key.mode, instructions, successors: block.successors.clone(), terminator: SemanticTerminator::Unknown };
            semantic.terminator = terminator(&semantic);
            blocks.push(semantic);
        }
        semantic_functions.push(SemanticFunction { id: function.id, entry: function.entry, blocks, successors: function.successors.clone(), calls: function.call_sites.clone(), returns: function.return_sites.clone() });
    }
    let semantic = SemanticProgram { entry: functions.entry, functions: semantic_functions, block_to_function: functions.block_to_function.clone() };
    validate_semantic_program(program, functions, &semantic)?;
    Ok(semantic)
}

pub fn validate_semantic_program(program: &Program, functions: &FunctionControlFlowGraph, semantic: &SemanticProgram) -> Result<(), String> {
    if semantic.functions.len() != functions.functions.len() { return Err("semantic/function count mismatch".into()); }
    let mut owned = HashMap::<BlockId, FunctionId>::new();
    for function in &semantic.functions {
        if function.id.0 >= semantic.functions.len() || function.entry.0 >= program.cfg.blocks.len() { return Err(format!("invalid semantic function {}", function.id.0)); }
        for block in &function.blocks {
            if block.id.0 >= program.cfg.blocks.len() { return Err(format!("function {} references invalid block {}", function.id.0, block.id.0)); }
            if owned.insert(block.id, function.id).is_some() { return Err(format!("block {} belongs to multiple functions", block.id.0)); }
            let source = &program.cfg.blocks[block.id.0];
            if source.instructions.len() != block.instructions.len() { return Err(format!("block {} instruction count changed during semantic lowering", block.id.0)); }
            for (source, semantic) in source.ir.iter().zip(&block.instructions) {
                if source.address != semantic.address || source.size != semantic.size { return Err(format!("block {} instruction identity changed", block.id.0)); }
            }
            for successor in &block.successors { if successor.0 >= program.cfg.blocks.len() { return Err(format!("block {} has invalid successor {}", block.id.0, successor.0)); } }
            match &block.terminator {
                SemanticTerminator::Call { .. } | SemanticTerminator::IndirectCall { .. } => { if block.successors.is_empty() { return Err(format!("call block {} lost its continuation", block.id.0)); } }
                SemanticTerminator::Return | SemanticTerminator::IndirectBranch { .. } => { if !block.successors.is_empty() { return Err(format!("terminating block {} has successors", block.id.0)); } }
                _ => {}
            }
        }
    }
    if owned != semantic.block_to_function { return Err("semantic block ownership differs from function recovery".into()); }
    for call in semantic.functions.iter().flat_map(|f| f.calls.iter()) {
        if let Some(return_block) = call.return_block {
            if !owned.contains_key(&return_block) { return Err(format!("call continuation {} is not owned by a function", return_block.0)); }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, discover_functions};
    use crate::decoder::{Mode, ROM_BASE};

    fn arm_rom(words: &[u32]) -> Vec<u8> { words.iter().flat_map(|word| word.to_le_bytes()).collect() }

    #[test]
    fn semantic_lowering_preserves_instruction_effects() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = &semantic.functions[0].blocks[0];
        assert_eq!(block.instructions[0].reads, Vec::<u8>::new());
        assert_eq!(block.instructions[0].writes, vec![0]);
        assert_eq!(block.instructions[1].reads, vec![0]);
        assert_eq!(block.instructions[1].writes, vec![0]);
    }

    #[test]
    fn semantic_condition_preserves_flag_dependency() {
        let program = analyze(&arm_rom(&[0x0A00_0000]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let flags = semantic.functions[0].blocks[0].instructions[0].flags;
        assert!(flags.read);
        assert!(!flags.write);
    }

    #[test]
    fn semantic_extended_instruction_has_effects() {
        let program = analyze(&arm_rom(&[0xE000_0090]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let instruction = &semantic.functions[0].blocks[0].instructions[0];
        assert_eq!(instruction.reads, vec![1, 2]);
        assert_eq!(instruction.writes, vec![0]);
        assert!(!instruction.flags.write);
    }

    #[test]
    fn semantic_call_has_explicit_continuation() {
        let program = analyze(&arm_rom(&[0xEB00_0000, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert!(matches!(semantic.functions[0].blocks[0].terminator, SemanticTerminator::Call { .. }));
        assert_eq!(semantic.functions[0].blocks[0].successors, vec![BlockId(1)]);
    }

    #[test]
    fn semantic_return_has_no_successor() {
        let program = analyze(&arm_rom(&[0xE12F_FF1E]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert_eq!(semantic.functions[0].blocks[0].terminator, SemanticTerminator::Return);
        assert!(semantic.functions[0].blocks[0].successors.is_empty());
    }
}
