use crate::address_space::{AddressSpace, ImageMapping};
use crate::decoder::{Instruction, Mode};
use crate::ir::IrInstruction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockKey {
    pub address: u32,
    pub mode: Mode,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub key: BlockKey,
    pub instructions: Vec<Instruction>,
    pub ir: Vec<IrInstruction>,
    pub successors: Vec<BlockId>,
}

#[derive(Debug, Clone, Default)]
pub struct ControlFlowGraph {
    pub entry: BlockId,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub entry: BlockId,
    pub cfg: ControlFlowGraph,
    pub image: ImageMapping,
    pub address_space: AddressSpace,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredInstruction {
    pub instruction: Instruction,
    pub successors: Vec<BlockKey>,
}
