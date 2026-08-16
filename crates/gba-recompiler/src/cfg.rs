use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;
use crate::decoder::{decode_arm, decode_thumb, decode_thumb_bl, read_arm, read_thumb, read_thumb_bl, ArmOp, Condition, DecodeError, Instruction, InstructionKind, Mode, ThumbOp, ROM_BASE};
use crate::ir::{lower, IrInstruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BlockId(pub usize);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockKey { pub address: u32, pub mode: Mode }
#[derive(Debug, Clone)]
pub struct BasicBlock { pub id: BlockId, pub key: BlockKey, pub instructions: Vec<Instruction>, pub ir: Vec<IrInstruction>, pub successors: Vec<BlockId> }
#[derive(Debug, Clone, Default)]
pub struct ControlFlowGraph { pub entry: BlockId, pub blocks: Vec<BasicBlock> }
#[derive(Debug, Clone, Default)]
pub struct Program { pub entry: BlockId, pub cfg: ControlFlowGraph }
#[derive(Debug, Error)]
pub enum AnalysisError { #[error(transparent)] Decode(#[from] DecodeError), #[error("entry {0:#x} is outside the cartridge ROM")] InvalidEntry(u32) }
#[derive(Debug, Clone)]
struct DiscoveredInstruction { instruction: Instruction, successors: Vec<BlockKey> }
