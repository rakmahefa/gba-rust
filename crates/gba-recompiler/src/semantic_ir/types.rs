use crate::cfg::BlockId;
use crate::decoder::{Condition, Mode};
use crate::function::{CallSite, FunctionId, ReturnSite};
use crate::ir::{IrControlEffect, IrOp};

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
    pub block_to_function: std::collections::HashMap<BlockId, FunctionId>,
}
