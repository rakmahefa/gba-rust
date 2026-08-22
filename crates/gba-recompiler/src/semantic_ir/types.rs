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
    Read {
        width: MemoryWidth,
        base: u8,
        address_is_dynamic: bool,
    },
    Write {
        width: MemoryWidth,
        base: u8,
        address_is_dynamic: bool,
    },
    ReadWrite {
        width: MemoryWidth,
        base: u8,
        address_is_dynamic: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlagEffect {
    pub read_n: bool,
    pub read_z: bool,
    pub read_c: bool,
    pub read_v: bool,
    pub write_n: bool,
    pub write_z: bool,
    pub write_c: bool,
    pub write_v: bool,
}

impl FlagEffect {
    pub fn reads_any(self) -> bool {
        self.read_n || self.read_z || self.read_c || self.read_v
    }

    pub fn writes_any(self) -> bool {
        self.write_n || self.write_z || self.write_c || self.write_v
    }
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
