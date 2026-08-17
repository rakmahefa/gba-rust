use std::collections::HashMap;

use crate::ir::{IrInstruction, IrOp, Value};
use crate::semantic_ir::{SemanticInstruction, SemanticProgram};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationKind {
    IdentityMove,
    AddZero,
    SubZero,
    ConstantFold,
    ConstantPropagation,
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

fn propagated_rhs(value: &Value, constants: &HashMap<u8, u32>) -> (Value, bool) {
    match value {
        Value::Reg(reg) => constants
            .get(reg)
            .copied()
            .map(Value::Imm)
            .map_or((Value::Reg(*reg), false), |value| (value, true)),
        Value::Imm(value) => (Value::Imm(*value), false),
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
            // r15 is the program counter: even an apparently identity move can alter control flow.
            IrOp::Mov { dst, src } if *dst == 15 => {
                constants.clear();
                IrOp::Mov { dst: *dst, src: src.clone() }
            }
            IrOp::Mov { dst, src } => {
                if matches!(src, Value::Reg(reg) if *reg == *dst) {
                    constants.remove(dst);
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::IdentityMove,
                    });
                    IrOp::Nop
                } else {
                    let (src, changed) = propagated_rhs(src, constants);
                    if changed {
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::ConstantPropagation,
                        });
                    }
                    match src {
                        Value::Imm(value) => {
                            constants.insert(*dst, value);
                        }
                        Value::Reg(_) => {
                            constants.remove(dst);
                        }
                    }
                    IrOp::Mov { dst: *dst, src }
                }
            }
            // Keep PC writes opaque for the same reason as MOV PC,PC.
            IrOp::Add { dst, .. } if *dst == 15 => {
                constants.clear();
                op.clone()
            }
            IrOp::Sub { dst, .. } if *dst == 15 => {
                constants.clear();
                op.clone()
            }
            IrOp::Add { dst, lhs, rhs } => {
                let (rhs, changed) = propagated_rhs(rhs, constants);
                if changed {
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::ConstantPropagation,
                    });
                }
                match rhs {
                    Value::Imm(value) if value == 0 && *dst == *lhs => {
                        constants.remove(dst);
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::AddZero,
                        });
                        IrOp::Mov { dst: *dst, src: Value::Reg(*lhs) }
                    }
                    Value::Imm(rhs_value) => {
                        if let Some(lhs_value) = constants.get(lhs).copied() {
                            let value = lhs_value.wrapping_add(rhs_value);
                            constants.insert(*dst, value);
                            report.changes.push(OptimizationChange {
                                address: instruction.address,
                                kind: OptimizationKind::ConstantFold,
                            });
                            IrOp::Mov { dst: *dst, src: Value::Imm(value) }
                        } else {
                            constants.remove(dst);
                            IrOp::Add { dst: *dst, lhs: *lhs, rhs: Value::Imm(rhs_value) }
                        }
                    }
                    Value::Reg(reg) => {
                        constants.remove(dst);
                        IrOp::Add { dst: *dst, lhs: *lhs, rhs: Value::Reg(reg) }
                    }
                }
            }
            IrOp::Sub { dst, lhs, rhs } => {
                let (rhs, changed) = propagated_rhs(rhs, constants);
                if changed {
                    report.changes.push(OptimizationChange {
                        address: instruction.address,
                        kind: OptimizationKind::ConstantPropagation,
                    });
                }
                match rhs {
                    Value::Imm(value) if value == 0 && *dst == *lhs => {
                        constants.remove(dst);
                        report.changes.push(OptimizationChange {
                            address: instruction.address,
                            kind: OptimizationKind::SubZero,
                        });
                        IrOp::Mov { dst: *dst, src: Value::Reg(*lhs) }
                    }
                    Value::Imm(rhs_value) => {
                        if let Some(lhs_value) = constants.get(lhs).copied() {
                            let value = lhs_value.wrapping_sub(rhs_value);
                            constants.insert(*dst, value);
                            report.changes.push(OptimizationChange {
                                address: instruction.address,
                                kind: OptimizationKind::ConstantFold,
                            });
                            IrOp::Mov { dst: *dst, src: Value::Imm(value) }
                        } else {
                            constants.remove(dst);
                            IrOp::Sub { dst: *dst, lhs: *lhs, rhs: Value::Imm(rhs_value) }
                        }
                    }
                    Value::Reg(reg) => {
                        constants.remove(dst);
                        IrOp::Sub { dst: *dst, lhs: *lhs, rhs: Value::Reg(reg) }
                    }
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
    let flags = instruction.flags();
    SemanticInstruction {
        address: instruction.address,
        size: instruction.size,
        ops: instruction.ops.clone(),
        reads: instruction.reads(),
        writes: instruction.writes(),
        memory: instruction.memory().map(|memory| match memory.kind {
            crate::ir::IrMemoryKind::Read => crate::semantic_ir::MemoryEffect::Read {
                width: match memory.width {
                    crate::ir::IrMemoryWidth::Byte => crate::semantic_ir::MemoryWidth::Byte,
                    crate::ir::IrMemoryWidth::Word => crate::semantic_ir::MemoryWidth::Word,
                },
                base: memory.base,
            },
            crate::ir::IrMemoryKind::Write => crate::semantic_ir::MemoryEffect::Write {
                width: match memory.width {
                    crate::ir::IrMemoryWidth::Byte => crate::semantic_ir::MemoryWidth::Byte,
                    crate::ir::IrMemoryWidth::Word => crate::semantic_ir::MemoryWidth::Word,
                },
                base: memory.base,
            },
        }),
        flags: crate::semantic_ir::FlagEffect {
            read: flags.reads_any(),
            write: flags.writes_any(),
        },
    }
}

/// Conservatively normalizes and optimizes semantic IR while preserving instruction count,
/// architectural writes, control flow and explicit timing NOPs. More aggressive DCE must wait
/// until flags, timing and side effects are represented explicitly by the IR.
pub fn optimize_semantic_program(program: &SemanticProgram) -> (SemanticProgram, OptimizationReport) {
    let mut optimized = program.clone();
    let mut report = OptimizationReport::default();
    for function in &mut optimized.functions {
        for block in &mut function.blocks {
            let mut constants = HashMap::new();
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for instruction in &block.instructions {
                let source = IrInstruction::new(instruction.address, instruction.size, instruction.ops.clone());
                let ops = normalize_instruction(&source, &mut constants, &mut report);
                instructions.push(semantic_instruction_from_ir(&IrInstruction::new(
                    source.address,
                    source.size,
                    ops,
                )));
            }
            block.instructions = instructions;
        }
    }
    (optimized, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::BlockId;
    use crate::decoder::Mode;
    use crate::function::FunctionId;
    use crate::semantic_ir::{SemanticFunction, SemanticTerminator};

    fn program(instructions: Vec<IrOp>) -> SemanticProgram {
        let block = crate::semantic_ir::SemanticBlock {
            id: BlockId(0),
            address: 0x0800_0000,
            mode: Mode::Arm,
            instructions: instructions
                .into_iter()
                .enumerate()
                .map(|(i, op)| {
                    semantic_instruction_from_ir(&IrInstruction::new(
                        0x0800_0000 + i as u32 * 4,
                        4,
                        vec![op],
                    ))
                })
                .collect(),
            successors: Vec::new(),
            terminator: SemanticTerminator::Return,
        };
        SemanticProgram {
            entry: FunctionId(0),
            functions: vec![SemanticFunction {
                id: FunctionId(0),
                entry: BlockId(0),
                blocks: vec![block],
                successors: Vec::new(),
                calls: Vec::new(),
                returns: Vec::new(),
            }],
            block_to_function: [(BlockId(0), FunctionId(0))].into_iter().collect(),
        }
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
        assert_eq!(optimized.functions[0].blocks[0].instructions[1].ops, vec![IrOp::Mov { dst: 0, src: Value::Imm(7) }]);
        assert_eq!(optimized.functions[0].blocks[0].instructions[2].ops, vec![IrOp::Mov { dst: 1, src: Value::Imm(5) }]);
    }

    #[test]
    fn identity_move_becomes_timing_preserving_nop() {
        let input = program(vec![IrOp::Mov { dst: 0, src: Value::Reg(0) }]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changes.iter().any(|c| c.kind == OptimizationKind::IdentityMove));
        assert_eq!(optimized.functions[0].blocks[0].instructions[0].ops, vec![IrOp::Nop]);
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
    fn does_not_optimize_pc_writes() {
        let input = program(vec![
            IrOp::Mov { dst: 15, src: Value::Reg(15) },
            IrOp::Add { dst: 15, lhs: 15, rhs: Value::Imm(0) },
        ]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changes.is_empty());
        assert_eq!(optimized.functions[0].blocks[0].instructions[0].ops, input.functions[0].blocks[0].instructions[0].ops);
        assert_eq!(optimized.functions[0].blocks[0].instructions[1].ops, input.functions[0].blocks[0].instructions[1].ops);
    }
}
