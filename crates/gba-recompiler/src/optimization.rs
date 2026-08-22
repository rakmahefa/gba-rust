use std::collections::HashMap;

use crate::ir::{IrInstruction, IrOp, Value};
use crate::semantic_ir::{FlagEffect, MemoryEffect, MemoryWidth, SemanticInstruction, SemanticProgram};

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
            IrOp::Mov {
                dst,
                src,
                set_flags,
            } if *dst == 15 => {
                constants.clear();
                IrOp::Mov {
                    dst: *dst,
                    src: src.clone(),
                    set_flags: *set_flags,
                }
            }
            IrOp::Mov {
                dst,
                src,
                set_flags,
            } if !*set_flags => {
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
                    IrOp::Mov {
                        dst: *dst,
                        src,
                        set_flags: false,
                    }
                }
            }
            IrOp::Mov {
                dst,
                src,
                set_flags,
            } => {
                constants.remove(dst);
                IrOp::Mov {
                    dst: *dst,
                    src: src.clone(),
                    set_flags: *set_flags,
                }
            }
            IrOp::Add { dst, .. } if *dst == 15 => {
                constants.clear();
                op.clone()
            }
            IrOp::Sub { dst, .. } if *dst == 15 => {
                constants.clear();
                op.clone()
            }
            IrOp::Add {
                dst,
                lhs,
                rhs,
                set_flags,
            } if !*set_flags => {
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
                        IrOp::Mov {
                            dst: *dst,
                            src: Value::Reg(*lhs),
                            set_flags: false,
                        }
                    }
                    Value::Imm(rhs_value) => {
                        if let Some(lhs_value) = constants.get(lhs).copied() {
                            let value = lhs_value.wrapping_add(rhs_value);
                            constants.insert(*dst, value);
                            report.changes.push(OptimizationChange {
                                address: instruction.address,
                                kind: OptimizationKind::ConstantFold,
                            });
                            IrOp::Mov {
                                dst: *dst,
                                src: Value::Imm(value),
                                set_flags: false,
                            }
                        } else {
                            constants.remove(dst);
                            IrOp::Add {
                                dst: *dst,
                                lhs: *lhs,
                                rhs: Value::Imm(rhs_value),
                                set_flags: false,
                            }
                        }
                    }
                    Value::Reg(reg) => {
                        constants.remove(dst);
                        IrOp::Add {
                            dst: *dst,
                            lhs: *lhs,
                            rhs: Value::Reg(reg),
                            set_flags: false,
                        }
                    }
                }
            }
            IrOp::Add {
                dst,
                lhs,
                rhs,
                set_flags,
            } => {
                constants.clear();
                IrOp::Add {
                    dst: *dst,
                    lhs: *lhs,
                    rhs: rhs.clone(),
                    set_flags: *set_flags,
                }
            }
            IrOp::Sub {
                dst,
                lhs,
                rhs,
                set_flags,
            } if !*set_flags => {
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
                        IrOp::Mov {
                            dst: *dst,
                            src: Value::Reg(*lhs),
                            set_flags: false,
                        }
                    }
                    Value::Imm(rhs_value) => {
                        if let Some(lhs_value) = constants.get(lhs).copied() {
                            let value = lhs_value.wrapping_sub(rhs_value);
                            constants.insert(*dst, value);
                            report.changes.push(OptimizationChange {
                                address: instruction.address,
                                kind: OptimizationKind::ConstantFold,
                            });
                            IrOp::Mov {
                                dst: *dst,
                                src: Value::Imm(value),
                                set_flags: false,
                            }
                        } else {
                            constants.remove(dst);
                            IrOp::Sub {
                                dst: *dst,
                                lhs: *lhs,
                                rhs: Value::Imm(rhs_value),
                                set_flags: false,
                            }
                        }
                    }
                    Value::Reg(reg) => {
                        constants.remove(dst);
                        IrOp::Sub {
                            dst: *dst,
                            lhs: *lhs,
                            rhs: Value::Reg(reg),
                            set_flags: false,
                        }
                    }
                }
            }
            IrOp::Sub {
                dst,
                lhs,
                rhs,
                set_flags,
            } => {
                constants.clear();
                IrOp::Sub {
                    dst: *dst,
                    lhs: *lhs,
                    rhs: rhs.clone(),
                    set_flags: *set_flags,
                }
            }
            IrOp::Load {
                dst,
                base,
                offset,
                byte,
            } => {
                constants.remove(dst);
                IrOp::Load {
                    dst: *dst,
                    base: *base,
                    offset: *offset,
                    byte: *byte,
                }
            }
            IrOp::Store {
                src,
                base,
                offset,
                byte,
            } => IrOp::Store {
                src: *src,
                base: *base,
                offset: *offset,
                byte: *byte,
            },
            IrOp::Cmp { lhs, rhs } => {
                constants.clear();
                IrOp::Cmp {
                    lhs: *lhs,
                    rhs: rhs.clone(),
                }
            }
            IrOp::Branch {
                target,
                condition,
                link,
            } => {
                if *link {
                    constants.clear();
                }
                IrOp::Branch {
                    target: *target,
                    condition: *condition,
                    link: *link,
                }
            }
            IrOp::BranchExchange { register, link } => {
                constants.clear();
                IrOp::BranchExchange {
                    register: *register,
                    link: *link,
                }
            }
            IrOp::ArmExtended { op } => {
                constants.clear();
                IrOp::ArmExtended { op: *op }
            }
            IrOp::ThumbExtended { op } => {
                constants.clear();
                IrOp::ThumbExtended { op: *op }
            }
            IrOp::Unknown { address, raw, mode } => {
                constants.clear();
                IrOp::Unknown {
                    address: *address,
                    raw: *raw,
                    mode: *mode,
                }
            }
            IrOp::Nop => IrOp::Nop,
        };
        if op.is_barrier() {
            constants.clear();
        }
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
        memory: instruction.memory().map(|memory| {
            let width = match memory.width {
                crate::ir::IrMemoryWidth::Byte => MemoryWidth::Byte,
                crate::ir::IrMemoryWidth::Halfword => MemoryWidth::Halfword,
                crate::ir::IrMemoryWidth::Word => MemoryWidth::Word,
            };
            match memory.kind {
                crate::ir::IrMemoryKind::Read => MemoryEffect::Read {
                    width,
                    base: memory.base,
                    address_is_dynamic: memory.address_is_dynamic,
                },
                crate::ir::IrMemoryKind::Write => MemoryEffect::Write {
                    width,
                    base: memory.base,
                    address_is_dynamic: memory.address_is_dynamic,
                },
                crate::ir::IrMemoryKind::ReadWrite => MemoryEffect::ReadWrite {
                    width,
                    base: memory.base,
                    address_is_dynamic: memory.address_is_dynamic,
                },
            }
        }),
        flags: FlagEffect {
            read_n: flags.read_n,
            read_z: flags.read_z,
            read_c: flags.read_c,
            read_v: flags.read_v,
            write_n: flags.write_n,
            write_z: flags.write_z,
            write_c: flags.write_c,
            write_v: flags.write_v,
        },
    }
}

pub fn optimize_semantic_program(
    program: &SemanticProgram,
) -> (SemanticProgram, OptimizationReport) {
    let mut optimized = program.clone();
    let mut report = OptimizationReport::default();
    for function in &mut optimized.functions {
        for block in &mut function.blocks {
            let mut constants = HashMap::new();
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for instruction in &block.instructions {
                let source = IrInstruction::new(
                    instruction.address,
                    instruction.size,
                    instruction.ops.clone(),
                );
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
    fn folds_local_constants_without_flags() {
        let input = program(vec![
            IrOp::Mov {
                dst: 0,
                src: Value::Imm(4),
                set_flags: false,
            },
            IrOp::Add {
                dst: 0,
                lhs: 0,
                rhs: Value::Imm(3),
                set_flags: false,
            },
            IrOp::Sub {
                dst: 1,
                lhs: 0,
                rhs: Value::Imm(2),
                set_flags: false,
            },
        ]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changed());
        assert_eq!(
            optimized.functions[0].blocks[0].instructions[1].ops,
            vec![IrOp::Mov {
                dst: 0,
                src: Value::Imm(7),
                set_flags: false
            }]
        );
        assert_eq!(
            optimized.functions[0].blocks[0].instructions[2].ops,
            vec![IrOp::Mov {
                dst: 1,
                src: Value::Imm(5),
                set_flags: false
            }]
        );
    }

    #[test]
    fn identity_move_becomes_timing_preserving_nop() {
        let input = program(vec![IrOp::Mov {
            dst: 0,
            src: Value::Reg(0),
            set_flags: false,
        }]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report
            .changes
            .iter()
            .any(|c| c.kind == OptimizationKind::IdentityMove));
        assert_eq!(
            optimized.functions[0].blocks[0].instructions[0].ops,
            vec![IrOp::Nop]
        );
    }

    #[test]
    fn does_not_optimize_flag_setting_arithmetic() {
        let input = program(vec![IrOp::Add {
            dst: 0,
            lhs: 0,
            rhs: Value::Imm(0),
            set_flags: true,
        }]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changes.is_empty());
        assert_eq!(
            optimized.functions[0].blocks[0].instructions[0].ops,
            input.functions[0].blocks[0].instructions[0].ops
        );
    }

    #[test]
    fn does_not_propagate_across_cmp() {
        let input = program(vec![
            IrOp::Mov {
                dst: 0,
                src: Value::Imm(1),
                set_flags: false,
            },
            IrOp::Cmp {
                lhs: 0,
                rhs: Value::Imm(1),
            },
            IrOp::Add {
                dst: 1,
                lhs: 0,
                rhs: Value::Imm(1),
                set_flags: false,
            },
        ]);
        let (optimized, _) = optimize_semantic_program(&input);
        assert!(matches!(
            optimized.functions[0].blocks[0].instructions[2].ops[0],
            IrOp::Add { .. }
        ));
    }

    #[test]
    fn extended_instruction_is_an_optimization_barrier() {
        let input = program(vec![
            IrOp::Mov {
                dst: 0,
                src: Value::Imm(1),
                set_flags: false,
            },
            IrOp::ThumbExtended {
                op: crate::decoder::ThumbExtended::SoftwareInterrupt { comment: 0 },
            },
            IrOp::Add {
                dst: 1,
                lhs: 0,
                rhs: Value::Imm(1),
                set_flags: false,
            },
        ]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changes.is_empty());
        assert!(matches!(
            optimized.functions[0].blocks[0].instructions[2].ops[0],
            IrOp::Add { .. }
        ));
    }

    #[test]
    fn does_not_optimize_pc_writes() {
        let input = program(vec![
            IrOp::Mov {
                dst: 15,
                src: Value::Reg(15),
                set_flags: false,
            },
            IrOp::Add {
                dst: 15,
                lhs: 15,
                rhs: Value::Imm(0),
                set_flags: false,
            },
        ]);
        let (optimized, report) = optimize_semantic_program(&input);
        assert!(report.changes.is_empty());
        assert_eq!(
            optimized.functions[0].blocks[0].instructions[0].ops,
            input.functions[0].blocks[0].instructions[0].ops
        );
        assert_eq!(
            optimized.functions[0].blocks[0].instructions[1].ops,
            input.functions[0].blocks[0].instructions[1].ops
        );
    }
}
