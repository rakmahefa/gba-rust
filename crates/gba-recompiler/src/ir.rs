use crate::decoder::{ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2, ThumbOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Reg(u8),
    Imm(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrOp {
    Nop,
    Mov { dst: u8, src: Value },
    Add { dst: u8, lhs: u8, rhs: Value },
    Sub { dst: u8, lhs: u8, rhs: Value },
    Cmp { lhs: u8, rhs: Value },
    Load { dst: u8, base: u8, offset: i32, byte: bool },
    Store { src: u8, base: u8, offset: i32, byte: bool },
    Branch { target: u32, condition: Condition, link: bool },
    BranchExchange { register: u8, link: bool },
    Unknown { address: u32, raw: u32, mode: Mode },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInstruction {
    pub address: u32,
    pub size: u8,
    pub ops: Vec<IrOp>,
}

fn operand(op: Operand2) -> Value {
    match op { Operand2::Imm(v) => Value::Imm(v), Operand2::Reg { rm, .. } => Value::Reg(rm) }
}

pub fn lower(ins: Instruction) -> IrInstruction {
    let op = match ins.kind {
        InstructionKind::Arm(ArmOp::Nop) | InstructionKind::Thumb(ThumbOp::Nop) => vec![IrOp::Nop],
        InstructionKind::Arm(ArmOp::Mov { rd, op2 }) => vec![IrOp::Mov { dst: rd, src: operand(op2) }],
        InstructionKind::Arm(ArmOp::Add { rd, rn, op2 }) => vec![IrOp::Add { dst: rd, lhs: rn, rhs: operand(op2) }],
        InstructionKind::Arm(ArmOp::Sub { rd, rn, op2 }) => vec![IrOp::Sub { dst: rd, lhs: rn, rhs: operand(op2) }],
        InstructionKind::Arm(ArmOp::Cmp { rn, op2 }) => vec![IrOp::Cmp { lhs: rn, rhs: operand(op2) }],
        InstructionKind::Arm(ArmOp::Load { rd, rn, offset, byte }) => vec![IrOp::Load { dst: rd, base: rn, offset, byte }],
        InstructionKind::Arm(ArmOp::Store { rd, rn, offset, byte }) => vec![IrOp::Store { src: rd, base: rn, offset, byte }],
        InstructionKind::Arm(ArmOp::Branch { target, condition, link }) => vec![IrOp::Branch { target, condition, link }],
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link }) => vec![IrOp::BranchExchange { register: rm, link }],
        InstructionKind::Thumb(ThumbOp::MovImm { rd, imm }) => vec![IrOp::Mov { dst: rd, src: Value::Imm(imm as u32) }],
        InstructionKind::Thumb(ThumbOp::AddImm { rd, rn, imm }) => vec![IrOp::Add { dst: rd, lhs: rn, rhs: Value::Imm(imm as u32) }],
        InstructionKind::Thumb(ThumbOp::SubImm { rd, rn, imm }) => vec![IrOp::Sub { dst: rd, lhs: rn, rhs: Value::Imm(imm as u32) }],
        InstructionKind::Thumb(ThumbOp::LoadImm { rd, rn, word_offset }) => vec![IrOp::Load { dst: rd, base: rn, offset: (word_offset as i32) * 4, byte: false }],
        InstructionKind::Thumb(ThumbOp::StoreImm { rd, rn, word_offset }) => vec![IrOp::Store { src: rd, base: rn, offset: (word_offset as i32) * 4, byte: false }],
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => vec![IrOp::Branch { target, condition, link: false }],
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => vec![IrOp::Branch { target, condition: Condition::Al, link: true }],
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm }) => vec![IrOp::BranchExchange { register: rm, link: false }],
        InstructionKind::Arm(ArmOp::Extended(_)) | InstructionKind::Arm(ArmOp::Unknown)
        | InstructionKind::Thumb(ThumbOp::Extended(_)) | InstructionKind::Thumb(ThumbOp::Unknown) => vec![IrOp::Unknown { address: ins.address, raw: ins.raw, mode: ins.mode }],
    };
    IrInstruction { address: ins.address, size: ins.size, ops: op }
}
