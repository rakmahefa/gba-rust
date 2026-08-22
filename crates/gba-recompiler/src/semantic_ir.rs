use std::collections::HashMap;

use crate::cfg::{BlockId, Program};
use crate::decoder::{Condition, Mode};
use crate::function::{CallSite, FunctionControlFlowGraph, FunctionId, ReturnSite};
use crate::ir::{
    IrControlEffect, IrFlags, IrInstruction, IrMemoryEffect, IrMemoryKind, IrMemoryWidth, IrOp,
};

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

impl SemanticInstruction {
    pub fn control_effect(&self) -> IrControlEffect {
        self.ops
            .iter()
            .rev()
            .map(IrOp::control)
            .find(|effect| !matches!(effect, IrControlEffect::None))
            .unwrap_or(IrControlEffect::None)
    }
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
        IrMemoryKind::Read => MemoryEffect::Read {
            width: memory_width(memory.width),
            base: memory.base,
        },
        IrMemoryKind::Write => MemoryEffect::Write {
            width: memory_width(memory.width),
            base: memory.base,
        },
        IrMemoryKind::ReadWrite => MemoryEffect::ReadWrite {
            width: memory_width(memory.width),
            base: memory.base,
        },
    });
    let flags = ir.flags();
    SemanticInstruction {
        address: ir.address,
        size: ir.size,
        ops: ir.ops.clone(),
        reads: ir.reads(),
        writes: ir.writes(),
        memory,
        flags: FlagEffect {
            read: flags.reads_any(),
            write: flags.writes_any(),
        },
    }
}

fn terminator(block: &SemanticBlock) -> SemanticTerminator {
    let Some(instruction) = block.instructions.iter().rev().find(|instruction| {
        !matches!(instruction.control_effect(), IrControlEffect::None)
    }) else {
        return SemanticTerminator::Fallthrough;
    };

    match instruction.control_effect() {
        IrControlEffect::Branch {
            target,
            condition,
            link: true,
        } => SemanticTerminator::Call { target, condition },
        IrControlEffect::Branch {
            target,
            condition,
            link: false,
        } => SemanticTerminator::Branch { target, condition },
        IrControlEffect::BranchExchange {
            register: 14,
            link: false,
        } => SemanticTerminator::Return,
        IrControlEffect::BranchExchange {
            register,
            link: true,
        } => SemanticTerminator::IndirectCall {
            register,
            mode: block.mode,
        },
        IrControlEffect::BranchExchange {
            register,
            link: false,
        } => SemanticTerminator::IndirectBranch { register },
        IrControlEffect::Unknown => SemanticTerminator::Unknown,
        IrControlEffect::None => SemanticTerminator::Fallthrough,
    }
}

fn semantic_successors(
    source_successors: &[BlockId],
    terminator: &SemanticTerminator,
) -> Vec<BlockId> {
    match terminator {
        SemanticTerminator::Return | SemanticTerminator::IndirectBranch { .. } => Vec::new(),
        _ => source_successors.to_vec(),
    }
}

pub fn build_semantic_program(
    program: &Program,
    functions: &FunctionControlFlowGraph,
) -> Result<SemanticProgram, String> {
    let mut semantic_functions = Vec::with_capacity(functions.functions.len());
    for function in &functions.functions {
        let mut blocks = Vec::with_capacity(function.blocks.len());
        for &block_id in &function.blocks {
            let block = program.cfg.blocks.get(block_id.0).ok_or_else(|| {
                format!(
                    "function {} references missing block {}",
                    function.id.0, block_id.0
                )
            })?;
            let instructions = block
                .ir
                .iter()
                .map(semantic_instruction)
                .collect::<Vec<_>>();
            let mut semantic = SemanticBlock {
                id: block.id,
                address: block.key.address,
                mode: block.key.mode,
                instructions,
                successors: Vec::new(),
                terminator: SemanticTerminator::Unknown,
            };
            semantic.terminator = terminator(&semantic);
            semantic.successors = semantic_successors(&block.successors, &semantic.terminator);
            blocks.push(semantic);
        }
        semantic_functions.push(SemanticFunction {
            id: function.id,
            entry: function.entry,
            blocks,
            successors: function.successors.clone(),
            calls: function.call_sites.clone(),
            returns: function.return_sites.clone(),
        });
    }
    let semantic = SemanticProgram {
        entry: functions.entry,
        functions: semantic_functions,
        block_to_function: functions.block_to_function.clone(),
    };
    validate_semantic_program(program, functions, &semantic)?;
    Ok(semantic)
}

fn same_memory(a: Option<IrMemoryEffect>, b: Option<MemoryEffect>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let width = memory_width(a.width);
            match a.kind {
                IrMemoryKind::Read => {
                    b == MemoryEffect::Read {
                        width,
                        base: a.base,
                    }
                }
                IrMemoryKind::Write => {
                    b == MemoryEffect::Write {
                        width,
                        base: a.base,
                    }
                }
                IrMemoryKind::ReadWrite => {
                    b == MemoryEffect::ReadWrite {
                        width,
                        base: a.base,
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
    let flags: IrFlags = source.flags();
    if (flags.reads_any(), flags.writes_any()) != (semantic.flags.read, semantic.flags.write) {
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
    if source.control() != semantic.control_effect() {
        return Err(format!(
            "block {} instruction control effect changed",
            block_id.0
        ));
    }
    Ok(())
}

fn validate_control_placement(block: &SemanticBlock) -> Result<(), String> {
    let mut control_instruction = None;
    for (index, instruction) in block.instructions.iter().enumerate() {
        if matches!(instruction.control_effect(), IrControlEffect::None) {
            continue;
        }
        if control_instruction.replace(index).is_some() {
            return Err(format!(
                "block {} contains multiple control-effect instructions",
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

pub fn validate_semantic_program(
    program: &Program,
    functions: &FunctionControlFlowGraph,
    semantic: &SemanticProgram,
) -> Result<(), String> {
    if semantic.functions.len() != functions.functions.len() {
        return Err("semantic/function count mismatch".into());
    }
    if semantic.functions.get(semantic.entry.0).is_none() {
        return Err(format!(
            "semantic entry function {} does not exist",
            semantic.entry.0
        ));
    }

    let mut owned = HashMap::<BlockId, FunctionId>::new();
    for function in &semantic.functions {
        if function.id.0 >= semantic.functions.len() || function.entry.0 >= program.cfg.blocks.len()
        {
            return Err(format!("invalid semantic function {}", function.id.0));
        }
        if !function
            .blocks
            .iter()
            .any(|block| block.id == function.entry)
        {
            return Err(format!(
                "function {} does not contain its entry block {}",
                function.id.0, function.entry.0
            ));
        }
        for block in &function.blocks {
            if block.id.0 >= program.cfg.blocks.len() {
                return Err(format!(
                    "function {} references invalid block {}",
                    function.id.0, block.id.0
                ));
            }
            if owned.insert(block.id, function.id).is_some() {
                return Err(format!(
                    "block {} belongs to multiple functions",
                    block.id.0
                ));
            }
            let source = &program.cfg.blocks[block.id.0];
            if source.instructions.len() != block.instructions.len() {
                return Err(format!(
                    "block {} instruction count changed during semantic lowering",
                    block.id.0
                ));
            }
            for (source_ir, semantic_instruction) in
                source.ir.iter().zip(&block.instructions)
            {
                validate_instruction(source_ir, semantic_instruction, block.id)?;
            }
            validate_control_placement(block)?;
            for successor in &block.successors {
                if successor.0 >= program.cfg.blocks.len() {
                    return Err(format!(
                        "block {} has invalid successor {}",
                        block.id.0, successor.0
                    ));
                }
            }
            match &block.terminator {
                SemanticTerminator::Branch { target, .. } => {
                    if !successor_matches_target(program, block, *target) {
                        return Err(format!(
                            "branch block {} lost its direct target successor",
                            block.id.0
                        ));
                    }
                    if block.successors.is_empty() {
                        return Err(format!("branch block {} lost its successor", block.id.0));
                    }
                }
                SemanticTerminator::Call { .. } | SemanticTerminator::IndirectCall { .. } => {
                    if block.successors.is_empty() {
                        return Err(format!("call block {} lost its continuation", block.id.0));
                    }
                }
                SemanticTerminator::Return | SemanticTerminator::IndirectBranch { .. } => {
                    if !block.successors.is_empty() {
                        return Err(format!("terminating block {} has successors", block.id.0));
                    }
                }
                SemanticTerminator::Fallthrough => {}
                SemanticTerminator::Unknown => {
                    if !block.instructions.iter().rev().any(|instruction| {
                        matches!(instruction.control_effect(), IrControlEffect::Unknown)
                    }) {
                        return Err(format!(
                            "unknown semantic terminator in block {} is not backed by an unknown control effect",
                            block.id.0
                        ));
                    }
                }
            }
        }
    }
    if owned != semantic.block_to_function {
        return Err("semantic block ownership differs from function recovery".into());
    }
    for call in semantic.functions.iter().flat_map(|f| f.calls.iter()) {
        if let Some(return_block) = call.return_block {
            if !owned.contains_key(&return_block) {
                return Err(format!(
                    "call continuation {} is not owned by a function",
                    return_block.0
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{Mode, ROM_BASE};
    use crate::{analyze, discover_functions};

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

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
        assert_eq!(
            block.instructions[0].control_effect(),
            IrControlEffect::None
        );
    }

    #[test]
    fn semantic_condition_preserves_flag_dependency() {
        let program = analyze(
            &arm_rom(&[0x0A00_0000, 0xE1A0_0000, 0xE1A0_0000]),
            ROM_BASE,
            Mode::Arm,
        )
        .unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let flags = semantic.functions[0].blocks[0].instructions[0].flags;
        assert!(flags.read);
        assert!(!flags.write);
    }

    #[test]
    fn semantic_extended_instruction_has_effects() {
        let program = analyze(&arm_rom(&[0xE000_0291]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let instruction = &semantic.functions[0].blocks[0].instructions[0];
        assert_eq!(instruction.reads, vec![1, 2]);
        assert_eq!(instruction.writes, vec![0]);
        assert!(!instruction.flags.write);
        assert_eq!(instruction.control_effect(), IrControlEffect::None);
    }

    #[test]
    fn semantic_call_has_explicit_continuation() {
        let program = analyze(&arm_rom(&[0xEB00_0000, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert!(matches!(
            semantic.functions[0].blocks[0].terminator,
            SemanticTerminator::Call { .. }
        ));
        assert!(!semantic.functions[0].blocks[0].successors.is_empty());
    }

    #[test]
    fn semantic_return_has_no_successor() {
        let program = analyze(&arm_rom(&[0xE12F_FF1E]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        assert_eq!(
            semantic.functions[0].blocks[0].terminator,
            SemanticTerminator::Return
        );
        assert!(semantic.functions[0].blocks[0].successors.is_empty());
    }

    #[test]
    fn resolved_bx_lr_is_a_semantic_return_without_executable_successor() {
        let rom = arm_rom(&[
            0xE59F_E000, // ldr lr, [pc]
            0xE12F_FF1E, // bx lr
            0x0800_0008, // resolved LR target / literal pool
        ]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| block.address == ROM_BASE + 4)
            .expect("resolved BX LR block must be present");
        assert_eq!(block.terminator, SemanticTerminator::Return);
        assert!(block.successors.is_empty());
    }

    #[test]
    fn resolved_indirect_branch_is_dynamic_without_executable_successor() {
        let rom = arm_rom(&[
            0xE59F_3000, // ldr r3, [pc]
            0xE12F_FF13, // bx r3
            0x0800_0008, // resolved r3 target / literal pool
        ]);
        let program = analyze(&rom, ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let block = semantic
            .functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .find(|block| block.address == ROM_BASE + 4)
            .expect("resolved BX r3 block must be present");
        assert_eq!(
            block.terminator,
            SemanticTerminator::IndirectBranch { register: 3 }
        );
        assert!(block.successors.is_empty());
    }

    #[test]
    fn semantic_validation_rejects_changed_reads() {
        let program = analyze(&arm_rom(&[0xE3A0_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let mut semantic = build_semantic_program(&program, &functions).unwrap();
        semantic.functions[0].blocks[0].instructions[0]
            .reads
            .push(1);
        let error = validate_semantic_program(&program, &functions, &semantic).unwrap_err();
        assert!(error.contains("instruction reads changed"));
    }

    #[test]
    fn semantic_validation_rejects_changed_control_effect() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let mut semantic = build_semantic_program(&program, &functions).unwrap();
        semantic.functions[0].blocks[0].instructions[0]
            .ops
            .push(IrOp::Branch {
                target: ROM_BASE,
                condition: Condition::Al,
                link: false,
            });
        let error = validate_semantic_program(&program, &functions, &semantic).unwrap_err();
        assert!(error.contains("instruction control effect changed"));
    }

    #[test]
    fn semantic_validation_rejects_multiple_control_effects() {
        let mut semantic = SemanticBlock {
            id: BlockId(0),
            address: ROM_BASE,
            mode: Mode::Arm,
            instructions: vec![
                SemanticInstruction {
                    address: ROM_BASE,
                    size: 4,
                    ops: vec![IrOp::Branch {
                        target: ROM_BASE,
                        condition: Condition::Al,
                        link: false,
                    }],
                    reads: Vec::new(),
                    writes: Vec::new(),
                    memory: None,
                    flags: FlagEffect {
                        read: false,
                        write: false,
                    },
                },
                SemanticInstruction {
                    address: ROM_BASE + 4,
                    size: 4,
                    ops: vec![IrOp::BranchExchange {
                        register: 14,
                        link: false,
                    }],
                    reads: vec![14],
                    writes: Vec::new(),
                    memory: None,
                    flags: FlagEffect {
                        read: false,
                        write: false,
                    },
                },
            ],
            successors: Vec::new(),
            terminator: SemanticTerminator::Return,
        };
        let error = validate_control_placement(&semantic).unwrap_err();
        assert!(error.contains("multiple control-effect instructions"));
        semantic.instructions.clear();
    }
}
