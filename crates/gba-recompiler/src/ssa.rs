use std::collections::{BTreeSet, HashMap};

use crate::cfg::BlockId;
use crate::ir::{IrOp, Value};
use crate::semantic_ir::{SemanticBlock, SemanticFunction, SemanticInstruction, SemanticProgram, SemanticTerminator};

/// A stable identity for a value produced while lowering register state into SSA-like form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SsaValue {
    Value(ValueId),
    Imm(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueDef {
    Entry { register: u8 },
    Phi { block: BlockId, register: u8 },
    Instruction { block: BlockId, instruction: usize, register: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaPhi {
    pub value: ValueId,
    pub register: u8,
    pub incoming: Vec<(BlockId, ValueId)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsaOp {
    Nop,
    Mov { dst: ValueId, src: SsaValue },
    Add { dst: ValueId, lhs: ValueId, rhs: SsaValue },
    Sub { dst: ValueId, lhs: ValueId, rhs: SsaValue },
    Cmp { lhs: ValueId, rhs: SsaValue },
    Load { dst: ValueId, base: ValueId, offset: i32, byte: bool },
    Store { src: ValueId, base: ValueId, offset: i32, byte: bool },
    Branch { target: u32, condition: crate::decoder::Condition, link: bool },
    BranchExchange { register: ValueId, link: bool },
    Unknown { address: u32, raw: u32, mode: crate::decoder::Mode },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaInstruction {
    pub address: u32,
    pub size: u8,
    pub op: SsaOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsaBlock {
    pub id: BlockId,
    pub address: u32,
    pub mode: crate::decoder::Mode,
    pub phis: Vec<SsaPhi>,
    pub instructions: Vec<SsaInstruction>,
    pub successors: Vec<BlockId>,
    pub terminator: SemanticTerminator,
}

#[derive(Debug, Clone)]
pub struct SsaFunction {
    pub id: crate::function::FunctionId,
    pub entry: BlockId,
    pub blocks: Vec<SsaBlock>,
}

#[derive(Debug, Clone, Default)]
pub struct SsaProgram {
    pub entry: crate::function::FunctionId,
    pub functions: Vec<SsaFunction>,
    pub definitions: HashMap<ValueId, ValueDef>,
}

#[derive(Default)]
struct ValueAllocator {
    next: u32,
    definitions: HashMap<ValueId, ValueDef>,
}

impl ValueAllocator {
    fn fresh(&mut self, definition: ValueDef) -> ValueId {
        let id = ValueId(self.next);
        self.next += 1;
        self.definitions.insert(id, definition);
        id
    }
}

type RegisterState = [Option<ValueId>; 16];

fn predecessors(blocks: &[SemanticBlock]) -> HashMap<BlockId, Vec<BlockId>> {
    let mut result = HashMap::<BlockId, Vec<BlockId>>::new();
    for block in blocks {
        result.entry(block.id).or_default();
        for &successor in &block.successors {
            result.entry(successor).or_default().push(block.id);
        }
    }
    for values in result.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    result
}

fn merge_state(
    block: &SemanticBlock,
    preds: &[BlockId],
    out_states: &HashMap<BlockId, RegisterState>,
    phis: &mut HashMap<(BlockId, u8), ValueId>,
    allocator: &mut ValueAllocator,
) -> RegisterState {
    let mut state = [None; 16];
    for reg in 0..16u8 {
        let incoming: Vec<ValueId> = preds.iter().filter_map(|pred| out_states.get(pred).and_then(|s| s[reg as usize])).collect();
        if incoming.is_empty() {
            continue;
        }
        if incoming.iter().all(|value| *value == incoming[0]) {
            state[reg as usize] = Some(incoming[0]);
        } else {
            let value = *phis.entry((block.id, reg)).or_insert_with(|| allocator.fresh(ValueDef::Phi { block: block.id, register: reg }));
            state[reg as usize] = Some(value);
        }
    }
    state
}

fn lower_operand(value: &Value, state: &RegisterState) -> SsaValue {
    match value {
        Value::Imm(value) => SsaValue::Imm(*value),
        Value::Reg(reg) => SsaValue::Value(state[*reg as usize].expect("register use must have an SSA value")),
    }
}

fn lower_instruction(
    instruction: &SemanticInstruction,
    block: &SemanticBlock,
    index: usize,
    state: &mut RegisterState,
    allocator: &mut ValueAllocator,
) -> SsaInstruction {
    let fresh = |allocator: &mut ValueAllocator, register: u8| allocator.fresh(ValueDef::Instruction { block: block.id, instruction: index, register });
    let op = instruction.ops.first().expect("semantic instruction must have an op");
    let ssa = match op {
        IrOp::Nop => SsaOp::Nop,
        IrOp::Mov { dst, src } => {
            let src = lower_operand(src, state);
            let value = fresh(allocator, *dst); state[*dst as usize] = Some(value);
            SsaOp::Mov { dst: value, src }
        }
        IrOp::Add { dst, lhs, rhs } => {
            let lhs_value = state[*lhs as usize].expect("add lhs must have an SSA value");
            let rhs = lower_operand(rhs, state);
            let value = fresh(allocator, *dst); state[*dst as usize] = Some(value);
            SsaOp::Add { dst: value, lhs: lhs_value, rhs }
        }
        IrOp::Sub { dst, lhs, rhs } => {
            let lhs_value = state[*lhs as usize].expect("sub lhs must have an SSA value");
            let rhs = lower_operand(rhs, state);
            let value = fresh(allocator, *dst); state[*dst as usize] = Some(value);
            SsaOp::Sub { dst: value, lhs: lhs_value, rhs }
        }
        IrOp::Cmp { lhs, rhs } => {
            let lhs = state[*lhs as usize].expect("cmp lhs must have an SSA value");
            let rhs = lower_operand(rhs, state);
            SsaOp::Cmp { lhs, rhs }
        }
        IrOp::Load { dst, base, offset, byte } => {
            let base = state[*base as usize].expect("load base must have an SSA value");
            let value = fresh(allocator, *dst); state[*dst as usize] = Some(value);
            SsaOp::Load { dst: value, base, offset: *offset, byte: *byte }
        }
        IrOp::Store { src, base, offset, byte } => {
            let src = state[*src as usize].expect("store src must have an SSA value");
            let base = state[*base as usize].expect("store base must have an SSA value");
            SsaOp::Store { src, base, offset: *offset, byte: *byte }
        }
        IrOp::Branch { target, condition, link } => {
            if *link { state[14] = Some(fresh(allocator, 14)); }
            SsaOp::Branch { target: *target, condition: *condition, link: *link }
        }
        IrOp::BranchExchange { register, link } => {
            let register_value = state[*register as usize].expect("branch exchange register must have an SSA value");
            if *link { state[14] = Some(fresh(allocator, 14)); }
            SsaOp::BranchExchange { register: register_value, link: *link }
        }
        IrOp::Unknown { address, raw, mode } => SsaOp::Unknown { address: *address, raw: *raw, mode: *mode },
    };
    SsaInstruction { address: instruction.address, size: instruction.size, op: ssa }
}

fn initial_state(allocator: &mut ValueAllocator) -> RegisterState {
    let mut state = [None; 16];
    for register in 0..16u8 {
        state[register as usize] = Some(allocator.fresh(ValueDef::Entry { register }));
    }
    state
}

fn lower_function(function: &SemanticFunction, allocator: &mut ValueAllocator) -> SsaFunction {
    let preds = predecessors(&function.blocks);
    let mut out_states = HashMap::<BlockId, RegisterState>::new();
    let mut phis = HashMap::<(BlockId, u8), ValueId>::new();

    // Register dataflow converges to the same entry state at every join. We only use
    // predecessor states here; no instruction is rewritten until the states stabilize.
    for _ in 0..(function.blocks.len().saturating_mul(4).max(4)) {
        let mut changed = false;
        for block in &function.blocks {
            let mut state = if block.id == function.entry {
                initial_state(allocator)
            } else {
                merge_state(block, preds.get(&block.id).map(Vec::as_slice).unwrap_or(&[]), &out_states, &mut phis, allocator)
            };
            for (index, instruction) in block.instructions.iter().enumerate() {
                match instruction.ops.first() {
                    Some(IrOp::Mov { dst, .. }) | Some(IrOp::Add { dst, .. }) | Some(IrOp::Sub { dst, .. }) | Some(IrOp::Load { dst, .. }) => {
                        state[*dst as usize] = Some(ValueId(u32::MAX - index as u32));
                    }
                    Some(IrOp::Branch { link: true, .. }) | Some(IrOp::BranchExchange { link: true, .. }) => state[14] = Some(ValueId(u32::MAX - index as u32)),
                    _ => {}
                }
            }
            if out_states.get(&block.id) != Some(&state) {
                out_states.insert(block.id, state);
                changed = true;
            }
        }
        if !changed { break; }
    }

    let mut blocks = Vec::with_capacity(function.blocks.len());
    for block in &function.blocks {
        let mut state = if block.id == function.entry {
            let mut entry = [None; 16];
            for register in 0..16u8 {
                entry[register as usize] = Some(allocator.fresh(ValueDef::Entry { register }));
            }
            entry
        } else {
            merge_state(block, preds.get(&block.id).map(Vec::as_slice).unwrap_or(&[]), &out_states, &mut phis, allocator)
        };
        let mut instructions = Vec::with_capacity(block.instructions.len());
        for (index, instruction) in block.instructions.iter().enumerate() {
            instructions.push(lower_instruction(instruction, block, index, &mut state, allocator));
        }
        let mut phi_nodes = Vec::new();
        for reg in 0..16u8 {
            if let Some(&value) = phis.get(&(block.id, reg)) {
                let incoming = preds.get(&block.id).into_iter().flat_map(|items| items.iter()).filter_map(|pred| out_states.get(pred).and_then(|s| s[reg as usize]).map(|v| (*pred, v))).collect::<Vec<_>>();
                if incoming.len() > 1 { phi_nodes.push(SsaPhi { value, register: reg, incoming }); }
            }
        }
        phi_nodes.sort_by_key(|phi| phi.register);
        blocks.push(SsaBlock { id: block.id, address: block.address, mode: block.mode, phis: phi_nodes, instructions, successors: block.successors.clone(), terminator: block.terminator.clone() });
    }
    SsaFunction { id: function.id, entry: function.entry, blocks }
}

pub fn build_ssa_program(semantic: &SemanticProgram) -> SsaProgram {
    let mut allocator = ValueAllocator::default();
    let functions = semantic.functions.iter().map(|function| lower_function(function, &mut allocator)).collect();
    SsaProgram { entry: semantic.entry, functions, definitions: allocator.definitions }
}

pub fn validate_ssa_program(ssa: &SsaProgram) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for function in &ssa.functions {
        let block_ids: BTreeSet<_> = function.blocks.iter().map(|block| block.id).collect();
        if !block_ids.contains(&function.entry) { return Err(format!("function {} has no entry block", function.id.0)); }
        for block in &function.blocks {
            for phi in &block.phis {
                if !seen.insert(phi.value) { return Err(format!("SSA value {:?} has multiple definitions", phi.value)); }
                if phi.incoming.len() < 2 { return Err(format!("phi for r{} in block {} has fewer than two inputs", phi.register, block.id.0)); }
                for (pred, value) in &phi.incoming {
                    if !block_ids.contains(pred) || !ssa.definitions.contains_key(value) { return Err(format!("invalid phi input in block {}", block.id.0)); }
                }
            }
            for instruction in &block.instructions {
                match instruction.op {
                    SsaOp::Mov { dst, .. } | SsaOp::Add { dst, .. } | SsaOp::Sub { dst, .. } | SsaOp::Load { dst, .. } => {
                        if !seen.insert(dst) || !ssa.definitions.contains_key(&dst) { return Err(format!("invalid SSA definition {:?}", dst)); }
                    }
                    _ => {}
                }
            }
            for successor in &block.successors { if !block_ids.contains(successor) { return Err(format!("block {} has foreign successor {}", block.id.0, successor.0)); } }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, build_semantic_program, discover_functions};
    use crate::decoder::{Mode, ROM_BASE};

    fn arm_rom(words: &[u32]) -> Vec<u8> { words.iter().flat_map(|word| word.to_le_bytes()).collect() }

    #[test]
    fn linear_register_writes_get_distinct_values() {
        let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let ssa = build_ssa_program(&semantic);
        let block = &ssa.functions[0].blocks[0];
        assert!(matches!(block.instructions[0].op, SsaOp::Mov { .. }));
        let first = match block.instructions[0].op { SsaOp::Mov { dst, .. } => dst, _ => unreachable!() };
        let second = match block.instructions[1].op { SsaOp::Add { dst, lhs, .. } => { assert_eq!(lhs, first); dst }, _ => unreachable!() };
        assert_ne!(first, second);
        validate_ssa_program(&ssa).unwrap();
    }

    #[test]
    fn branch_join_gets_a_phi_for_divergent_register_state() {
        // CMP r0,#0
        // BEQ target
        // MOV r0,#1
        // B join
        // target: MOV r0,#2
        // join: ADD r0,r0,#3
        let program = analyze(&arm_rom(&[
            0xE350_0000,
            0x0A00_0002,
            0xE3A0_0001,
            0xEA00_0000,
            0xE3A0_0002,
            0xE280_0003,
        ]), ROM_BASE, Mode::Arm).unwrap();
        let functions = discover_functions(&program);
        let semantic = build_semantic_program(&program, &functions).unwrap();
        let ssa = build_ssa_program(&semantic);
        validate_ssa_program(&ssa).unwrap();
        assert!(ssa.functions.iter().flat_map(|f| f.blocks.iter()).flat_map(|b| b.phis.iter()).any(|phi| phi.register == 0));
    }
}
