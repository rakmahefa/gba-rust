use crate::decoder::{
    ArmDataOp, ArmExtended, ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2,
    ThumbAluOp, ThumbExtended, ThumbOp,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Reg(u8),
    Imm(u32),
}
