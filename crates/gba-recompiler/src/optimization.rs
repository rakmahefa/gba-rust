use std::collections::HashMap;

use crate::ir::{IrInstruction, IrOp, Value};
use crate::semantic_ir::{FlagEffect, MemoryEffect, SemanticBlock, SemanticInstruction, SemanticProgram};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationKind {
    IdentityMove,
    AddZero,
    SubZero,
    ConstantFold,
    ConstantPropagation,
    RemoveNop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationChange {
    pub address: u32,
    pub kind: OptimizationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptimizationReport {
    pub changes: Vec<OptimizationChange>,
}

impl OptimizationReport {
    pub fn changed(&self) -> bool {
        !self.changes.is_empty()
    }
}

fn canonicalize_value(value: Value, constants: &HashMap<u8, u32>) -> (Value, bool) {
    match value {
        Value::Reg(reg) => match constants.get(&reg).copied() {
            Some(value) => (Value::Imm(value), true),
            None => (Value::Reg(reg), false),
        },
        Value::Imm(value) => (Value::Imm(value), false),
    }
}

fn normalize_instruction(
    instruction: &IrInstruction,
    constants: &mut HashMap<u8, u32>,
    report: &mut OptimizationReport,
) -> Vec<IrOp> {
    let mut normalized = Vec::with_capacity(instruction.ops.len());

    for op in &instruction.ops {
        let op = match op {
            IrOp::Mov { dst, src } => {
                let (src, propagated) = canonicalize_value(src.clone(), constants);
                if propagated {
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::ConstantPropagation,
                    });
                }
                if matches!(src, Value::Reg(reg) if reg == *dst) {
                    constants.remove(dst);
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::IdentityMove,
                    });
                    IrOp::Nop
                } else {
                    if let Value::Imm(value) = src {
                        constants.insert(*dst, value);
                    } else {
                        constants.remove(dst);
                    }
                    IrOp::Mov { dst: *dst, src }
                }
            }
            IrOp::Add { dst, lhs, rhs } => {
                let (rhs, rhs_propagated) = canonicalize_value(rhs.clone(), constants);
                let (lhs, lhs_propagated) = constants
                    .get(lhs)
                    .copied()
                    .map(|value| (Value::Imm(value), true))
                    .unwrap_or((Value::Reg(*lhs), false));
                if rhs_propagated || lhs_propagated {
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::ConstantPropagation,
                    });
                }

                match (&lhs, &rhs) {
                    (Value::Imm(lhs), Value::Imm(rhs)) => {
                        let value = lhs.wrapping_add(*rhs);
                        constants.insert(*dst, value);
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::ConstantFold,
                        });
                        IrOp::Mov { dst: *dst, src: Value::Imm(value) }
                    }
                    (_, Value::Imm(0)) if *dst == *lhs => {
                        constants.remove(dst);
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::AddZero,
                        });
                        IrOp::Mov { dst: *dst, src: lhs }
                    }
                    (Value::Reg(lhs), Value::Imm(value)) => {
                        constants.remove(dst);
                        IrOp::Add { dst: *dst, lhs: *lhs, rhs: Value::Imm(*value) }
                    }
                    (Value::Reg(lhs), rhs) => {
                        constants.remove(dst);
                        IrOp::Add { dst: *dst, lhs: *lhs, rhs: rhs.clone() }
                    }
                    _ => unreachable!(),
                }
            }
            IrOp::Sub { dst, lhs, rhs } => {
                let (rhs, rhs_propagated) = canonicalize_value(rhs.clone(), constants);
                let (lhs, lhs_propagated) = constants
                    .get(lhs)
                    .copied()
                    .map(|value| (Value::Imm(value), true))
                    .unwrap_or((Value::Reg(*lhs), false));
                if rhs_propagated || lhs_propagated {
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::ConstantPropagation,
                    });
                }

                match (&lhs, &rhs) {
                    (Value::Imm(lhs), Value::Imm(rhs)) => {
                        let value = lhs.wrapping_sub(*rhs);
                        constants.insert(*dst, value);
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::ConstantFold,
                        });
                        IrOp::Mov { dst: *dst, src: Value::Imm(value) }
                    }
                    (_, Value::Imm(0)) if *dst == *lhs => {
                        constants.remove(dst);
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::SubZero,
                        });
                        IrOp::Mov { dst: *dst, src: lhs }
                    }
                    (Value::Reg(lhs), Value::Imm(value)) => {
                        constants.remove(dst);
                        IrOp::Sub { dst: *dst, lhs: *lhs, rhs: Value::Imm(*value) }
                    }
                    (Value::Reg(lhs), rhs) => {
                        constants.remove(dst);
                        IrOp::Sub { dst: *dst, lhs: *lhs, rhs: rhs.clone() }
                    }
                    _ => unreachable!(),
                }
            }
            IrOp::Load { dst, base, offset, byte } => {
                constants.remove(dst);
                IrOp::Load { dst: *dst, base: *base, offset: *offset, byte: *byte }
            }
            IrOp::Store { src, base, offset, byte } => {
                IrOp::Store { src: *src, base: *base, offset: *offset, byte: *byte }
            }
            IrOp::Cmp { lhs, rhs } => {
                constants.clear();
                IrOp::Cmp { lhs: *lhs, rhs: rhs.clone() }
            }
            IrOp::Branch { target, condition, link } => {
                if *link {
                    constants.clear();
                }
                IrOp::Branch { target: *target, condition: *condition, link: *link }
            }
            IrOp::BranchExchange { register, link } => {
                constants.clear();
                IrOp::BranchExchange { register: *register, link: *link }
            }
            IrOp::Nop => IrOp::Nop,
            IrOp::Unknown { address, raw, mode } => {
                constants.clear();
                IrOp::Unknown { address: *address, raw: *raw, mode: *mode }
            }
        };
        normalized.push(op);
    }

    normalized
}

fn semantic_instruction_from_ir(instruction: &IrInstruction) -> SemanticInstruction {
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut memory: Option<MemoryEffect> = None;
    let mut flags = FlagEffect { read: false, write: false };

    for op in &instruction.ops {
        match op {
            IrOp::Mov { dst, src } => {
                if let Value::Reg(reg) = src { reads.push(*reg); }
                writes.push(*dst);
            }
            IrOp::Add { dst, lhs, rhs } | IrOp::Sub { dst, lhs, rhs } => {
                reads.push(*lhs);
                if let Value::Reg(reg) = rhs { reads.push(*reg); }
                writes.push(*dst);
            }
            IrOp::Cmp { lhs, rhs } => {
                reads.push(*lhs);
                if let Value::Reg(reg) = rhs { reads.push(*reg); }
                flags.write = true;
            }
            IrOp::Load { dst, base, byte, .. } => {
                reads.push(*base);
                writes.push(*dst);
                memory = Some(MemoryEffect::Read {
                    width: if *byte { crate::semantic_ir::MemoryWidth::Byte } else { crate::semantic_ir::MemoryWidth::Word },
                    base: *base,
                });
            }
            IrOp::Store { src, base, byte, .. } => {
                reads.extend([*src, *base]);
                memory = Some(MemoryEffect::Write {
                    width: if *byte { crate::semantic_ir::MemoryWidth::Byte } else { crate::semantic_ir::MemoryWidth::Word },
                    base: *base,
                });
            }
            IrOp::Branch { condition, link, .. } => {
                flags.read = *condition != crate::decoder::Condition::Al;
                if *link { writes.push(14); }
            }
            IrOp::BranchExchange { register, link } => {
                reads.push(*register);
                if *link { writes.push(14); }
            }
            IrOp::Nop | IrOp::Unknown { .. } => {}
        }
    }

    reads.sort_unstable();
    reads.dedup();
    writes.sort_unstable();
    writes.dedup();
    SemanticInstruction {
        address: instruction.address,
        size: instruction.size,
        ops: instruction.ops.clone(),
        reads,
        writes,
        memory,
        flags,
    }
}

fn remove_redundant_nops(block: &mut SemanticBlock, report: &mut OptimizationReport) {
    let before = block.instructions.len();
    block.instructions.retain(|instruction| {
        let removable = instruction.ops.len() == 1 && matches!(instruction.ops[0], IrOp::Nop);
        if removable {
            report.changes.push(OptimizationChange {
                address: instruction.address,
                kind: OptimizationKind::RemoveNop,
            });
        }
        !removable
    });
    debug_assert!(block.instructions.len() <= before);
}

/// Normalize and conservatively optimize a semantic program.
///
/// This pass deliberately preserves architectural writes and control flow. It performs
/// only transformations that are valid under the current IR semantics: identity moves,
/// zero-add/sub normalization, local constant propagation/folding, and NOP elimination.
pub fn optimize_semantic_program(program: &SemanticProgram) -> (SemanticProgram, OptimizationReport) {
    let mut optimized = program.clone();
    let mut report = OptimizationReport::default();

    for function in &mut optimized.functions {
        for block in &mut function.blocks {
            let mut constants = HashMap::new();
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for instruction in &block.instructions {
                let source = IrInstruction {
                    address: instruction.address,
                    size: instruction.size,
                    ops: instruction.ops.clone(),
                };
                let ops = normalize_instruction(&source, &mut constants, &mut report);
                let normalized = IrInstruction { address: source.address, size: source.size, ops };
                instructions.push(semantic_instruction_from_ir(&normalized));
            }
            block.instructions = instructions;
            remove_redundant_nops(block, &mut report);
        }
    }

    (optimized, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{BlockId, Program};
    use crate::decoder::{Condition, Mode};
    use crate::function::{Function, FunctionControlFlowGraph, FunctionId};
    use crate::semantic_ir::{SemanticFunction, SemanticTerminator};

    fn program(instructions: Vec<IrOp>) -> SemanticProgram {
        let block = SemanticBlock {
            id: BlockId(0),
            address: 0x0800_0000,
            mode: Mode::Arm,
            instructions: instructions.into_iter().map(|ops| semantic_instruction_from_ir(&IrInstruction { address: 0x0800_0000, size: 4, ops: vec![ops] })).collect(),
            successors: Vec::new(),
            terminator: SemanticTerminator::Return,
        };
        let function = SemanticFunction {
            id: FunctionId(0),
            entry: BlockId(0),
            blocks: vec![block],
            successors: Vec::new(),
            calls: Vec::new(),
            returns: Vec::new(),
        };
        SemanticProgram { entry: FunctionId(0), functions: vec![function], block_to_function: [(BlockId(0), FunctionId(0))].into_iter().collect() }
    }

    #[test]
    fn folds_local_constants() {
        let input = program(vec![
            IrOp::Mov { dst: 0, src: Value::Imm(4) },
            IrOp::Add { dst: 0, lhs: 0, rhs: Value::Imm(3) },
            IrOp::Sub { dst: 1, lhs: 0, rhs: Value::Imm(2) },
        ]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changed());
        assert_eq!(optimized.functions[0].blocks[0].instructions[0].ops, vec![IrOp::Mov { dst: 0, src: Value::Imm(4) }]);
        assert_eq!(optimized.functions[0].blocks[0].instructions[1].ops, vec![IrOp::Mov { dst: 0, src: Value::Imm(7) }]);
        assert_eq!(optimized.functions[0].blocks[0].instructions[2].ops, vec![IrOp::Mov { dst: 1, src: Value::Imm(5) }]);
    }

    #[test]
    fn removes_identity_move() {
        let input = program(vec![IrOp::Mov { dst: 0, src: Value::Reg(0) }]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changes.iter().any(|change| change.kind == OptimizationKind::IdentityMove));
        assert!(optimized.functions[0].blocks[0].instructions.is_empty());
    }

    #[test]
    fn does_not_propagate_across_cmp() {
        let input = program(vec![
            IrOp::Mov { dst: 0, src: Value::Imm(1) },
            IrOp::Cmp { lhs: 0, rhs: Value::Imm(1) },
            IrOp::Add { dst: 1, lhs: 0, rhs: Value::Imm(1) },
        ]);
        let (optimized, _) = optimize_semantic_program(&input);
        assert!(matches!(optimized.functions[0].blocks[0].instructions[2].ops[0], IrOp::Add { .. }));
    }

    #[test]
    fn preserves_conditional_branch_semantics() {
        let input = program(vec![IrOp::Branch { target: 0x0800_0010, condition: Condition::Eq, link: false }]);
        let (optimized, _) = optimize_semantic_program(&input);
        assert_eq!(optimized.functions[0].blocks[0].instructions[0].ops, input.functions[0].blocks[0].instructions[0].ops);
    }
}
