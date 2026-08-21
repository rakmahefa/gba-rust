use std::collections::{HashMap, HashSet, VecDeque};

use crate::decoder::{
    decode_arm, decode_thumb, decode_thumb_bl, read_arm, read_thumb, read_thumb_bl, ArmDataOp,
    ArmExtended, ArmOp, Condition, DecodeError, Instruction, InstructionKind, Mode, Operand2,
    ThumbExtended, ThumbOp, ROM_BASE,
};
use crate::ir::{lower, IrInstruction};
use thiserror::Error;

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
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub entry: BlockId,
    pub cfg: ControlFlowGraph,
}
#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("entry {0:#x} is outside the cartridge ROM")]
    InvalidEntry(u32),
}

#[derive(Debug, Clone)]
struct DiscoveredInstruction {
    instruction: Instruction,
    successors: Vec<BlockKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbstractValue {
    Unknown,
    Constant(u32),
}
impl Default for AbstractValue {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct AbstractState {
    regs: [AbstractValue; 16],
}
impl AbstractState {
    fn read(self, register: u8) -> AbstractValue {
        self.regs[register as usize]
    }
    fn write(&mut self, register: u8, value: AbstractValue) {
        self.regs[register as usize] = value;
    }
    fn join(self, other: Self) -> Self {
        let mut joined = self;
        for index in 0..16 {
            joined.regs[index] = if self.regs[index] == other.regs[index] {
                self.regs[index]
            } else {
                AbstractValue::Unknown
            };
        }
        joined
    }
}

fn next_key(i: Instruction) -> BlockKey {
    BlockKey {
        address: i.address + i.size as u32,
        mode: i.mode,
    }
}
fn in_rom(rom: &[u8], address: u32) -> bool {
    address >= ROM_BASE && address - ROM_BASE < rom.len() as u32
}
fn read_rom32(rom: &[u8], address: u32) -> Option<u32> {
    if !in_rom(rom, address) || address - ROM_BASE > rom.len().saturating_sub(4) as u32 {
        return None;
    }
    let offset = (address - ROM_BASE) as usize;
    Some(u32::from_le_bytes(rom[offset..offset + 4].try_into().ok()?))
}
fn aligned_pc(address: u32, mode: Mode) -> u32 {
    match mode {
        Mode::Arm => (address + 8) & !3,
        Mode::Thumb => (address + 4) & !3,
    }
}
fn add_signed(base: u32, offset: i32) -> u32 {
    if offset >= 0 {
        base.wrapping_add(offset as u32)
    } else {
        base.wrapping_sub((-offset) as u32)
    }
}
fn operand_value(state: AbstractState, operand: Operand2) -> AbstractValue {
    match operand {
        Operand2::Imm(value) => AbstractValue::Constant(value),
        Operand2::Reg {
            rm,
            shift: 0,
            by_register: false,
            ..
        } => state.read(rm),
        Operand2::Reg { .. } => AbstractValue::Unknown,
    }
}
fn add_values(lhs: AbstractValue, rhs: AbstractValue) -> AbstractValue {
    match (lhs, rhs) {
        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
            AbstractValue::Constant(a.wrapping_add(b))
        }
        _ => AbstractValue::Unknown,
    }
}
fn sub_values(lhs: AbstractValue, rhs: AbstractValue) -> AbstractValue {
    match (lhs, rhs) {
        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
            AbstractValue::Constant(a.wrapping_sub(b))
        }
        _ => AbstractValue::Unknown,
    }
}

fn transfer_instruction(rom: &[u8], instruction: Instruction, mut state: AbstractState) -> AbstractState {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Mov { rd, op2 }) => {
            state.write(rd, operand_value(state, op2));
        }
        InstructionKind::Arm(ArmOp::Add { rd, rn, op2 }) => {
            state.write(rd, add_values(state.read(rn), operand_value(state, op2)));
        }
        InstructionKind::Arm(ArmOp::Sub { rd, rn, op2 }) => {
            state.write(rd, sub_values(state.read(rn), operand_value(state, op2)));
        }
        InstructionKind::Arm(ArmOp::Load { rd, rn, offset, .. }) if rn == 15 => {
            let address = add_signed(aligned_pc(instruction.address, Mode::Arm), offset);
            state.write(
                rd,
                read_rom32(rom, address)
                    .map(AbstractValue::Constant)
                    .unwrap_or(AbstractValue::Unknown),
            );
        }
        InstructionKind::Arm(ArmOp::Load { rd, .. }) => {
            state.write(rd, AbstractValue::Unknown);
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::DataProcessing {
            op, rd, rn, op2, ..
        })) => {
            let rhs = operand_value(state, op2);
            let value = match op {
                ArmDataOp::Mov => rhs,
                ArmDataOp::Mvn => match rhs {
                    AbstractValue::Constant(value) => AbstractValue::Constant(!value),
                    AbstractValue::Unknown => AbstractValue::Unknown,
                },
                ArmDataOp::Add => add_values(state.read(rn), rhs),
                ArmDataOp::Sub => sub_values(state.read(rn), rhs),
                ArmDataOp::Rsb => sub_values(rhs, state.read(rn)),
                _ => AbstractValue::Unknown,
            };
            state.write(rd, value);
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer {
            load: true,
            rd,
            rn: 15,
            offset: Operand2::Imm(offset),
            pre_index: true,
            up,
            write_back: false,
            ..
        })) => {
            let signed_offset = if up { offset as i32 } else { -(offset as i32) };
            let address = add_signed(aligned_pc(instruction.address, Mode::Arm), signed_offset);
            state.write(
                rd,
                read_rom32(rom, address)
                    .map(AbstractValue::Constant)
                    .unwrap_or(AbstractValue::Unknown),
            );
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer {
            load: true, rd, ..
        }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::HalfwordTransfer {
            load: true, rd, ..
        }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::Mrs { rd, .. }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::Swap { rd, .. })) => {
            state.write(rd, AbstractValue::Unknown);
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::Multiply { rd, .. })) => {
            state.write(rd, AbstractValue::Unknown);
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::MultiplyLong {
            rd_hi, rd_lo, ..
        })) => {
            state.write(rd_hi, AbstractValue::Unknown);
            state.write(rd_lo, AbstractValue::Unknown);
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::BlockTransfer {
            load: true,
            rn,
            register_list,
            ..
        })) => {
            for register in 0..16 {
                if register_list & (1 << register) != 0 {
                    state.write(register as u8, AbstractValue::Unknown);
                }
            }
            state.write(rn, AbstractValue::Unknown);
        }
        InstructionKind::Thumb(ThumbOp::MovImm { rd, imm }) => {
            state.write(rd, AbstractValue::Constant(imm as u32));
        }
        InstructionKind::Thumb(ThumbOp::AddImm { rd, rn, imm }) => {
            state.write(rd, add_values(state.read(rn), AbstractValue::Constant(imm as u32)));
        }
        InstructionKind::Thumb(ThumbOp::SubImm { rd, rn, imm }) => {
            state.write(rd, sub_values(state.read(rn), AbstractValue::Constant(imm as u32)));
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::PcRelativeLoad {
            rd,
            word_offset,
        })) => {
            let address = aligned_pc(instruction.address, Mode::Thumb).wrapping_add(word_offset as u32 * 4);
            state.write(
                rd,
                read_rom32(rom, address)
                    .map(AbstractValue::Constant)
                    .unwrap_or(AbstractValue::Unknown),
            );
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::Address {
            rd,
            use_sp: false,
            word_offset,
        })) => {
            state.write(
                rd,
                AbstractValue::Constant(
                    aligned_pc(instruction.address, Mode::Thumb)
                        .wrapping_add(word_offset as u32 * 4),
                ),
            );
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::PcRelativeLoad { .. }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::Address { use_sp: true, .. })) => {}
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreRegister {
            load: true, rd, ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreSignHalf { rd, .. }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreImmediate {
            load: true,
            rd,
            ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreHalfword {
            load: true,
            rd,
            ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::SpRelativeLoadStore {
            load: true,
            rd,
            ..
        })) => {
            state.write(rd, AbstractValue::Unknown);
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::MoveShifted {
            kind,
            rd,
            rs,
            offset,
        })) if kind == 0 => {
            state.write(rd, match state.read(rs) {
                AbstractValue::Constant(value) => AbstractValue::Constant(value << offset),
                AbstractValue::Unknown => AbstractValue::Unknown,
            });
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::AddSubImmediate {
            sub,
            rd,
            rs,
            imm,
        })) => {
            let value = if sub {
                sub_values(state.read(rs), AbstractValue::Constant(imm as u32))
            } else {
                add_values(state.read(rs), AbstractValue::Constant(imm as u32))
            };
            state.write(rd, value);
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::AddSubRegister {
            sub,
            rd,
            rs,
            rn,
        })) => {
            let value = if sub {
                sub_values(state.read(rs), state.read(rn))
            } else {
                add_values(state.read(rs), state.read(rn))
            };
            state.write(rd, value);
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::HighRegister { op, rd, rs }))
            if op == 0 => {
            state.write(rd, state.read(rs));
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::HighRegister { op, rd, .. }))
            if op == 1 || op == 2 || op == 3 => {
            state.write(rd, AbstractValue::Unknown);
        }
        _ => {}
    }
    state
}

fn resolved_exchange_target(state: AbstractState, register: u8) -> Option<BlockKey> {
    let AbstractValue::Constant(target) = state.read(register) else {
        return None;
    };
    let mode = if target & 1 != 0 { Mode::Thumb } else { Mode::Arm };
    let address = match mode {
        Mode::Arm => target & !3,
        Mode::Thumb => target & !1,
    };
    Some(BlockKey { address, mode })
}

fn instruction_successors(
    rom: &[u8],
    instruction: Instruction,
    state: AbstractState,
) -> Vec<BlockKey> {
    let next = next_key(instruction);
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Branch {
            target,
            condition,
            link,
        }) => {
            let mut successors = vec![BlockKey {
                address: target,
                mode: Mode::Arm,
            }];
            if condition != Condition::Al || link {
                successors.push(next);
            }
            successors
        }
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link }) => {
            let mut successors = Vec::new();
            if let Some(target) = resolved_exchange_target(state, rm) {
                if in_rom(rom, target.address) {
                    successors.push(target);
                }
            }
            if link {
                successors.push(next);
            }
            successors
        }
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => {
            let mut successors = vec![BlockKey {
                address: target,
                mode: Mode::Thumb,
            }];
            if condition != Condition::Al {
                successors.push(next);
            }
            successors
        }
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => vec![
            BlockKey {
                address: target,
                mode: Mode::Thumb,
            },
            next,
        ],
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm }) => resolved_exchange_target(state, rm)
            .filter(|target| in_rom(rom, target.address))
            .into_iter()
            .collect(),
        InstructionKind::Arm(ArmOp::Unknown) | InstructionKind::Thumb(ThumbOp::Unknown) => {
            Vec::new()
        }
        _ => vec![next],
    }
}

fn is_call(instruction: Instruction) -> bool {
    matches!(
        instruction.kind,
        InstructionKind::Arm(ArmOp::Branch { link: true, .. })
            | InstructionKind::Arm(ArmOp::BranchExchange { link: true, .. })
            | InstructionKind::Thumb(ThumbOp::BranchLink { .. })
    )
}

fn decode_at(rom: &[u8], key: BlockKey) -> Result<Instruction, DecodeError> {
    match key.mode {
        Mode::Arm => Ok(decode_arm(key.address, read_arm(rom, key.address)?)),
        Mode::Thumb => {
            let raw = read_thumb(rom, key.address)?;
            if (raw & 0xF800) == 0xF000 {
                let (first, second) = read_thumb_bl(rom, key.address)?;
                Ok(decode_thumb_bl(key.address, first, second))
            } else {
                Ok(decode_thumb(key.address, raw))
            }
        }
    }
}

fn discover_reachable(
    rom: &[u8],
    entry: BlockKey,
) -> Result<(Vec<BlockKey>, HashMap<BlockKey, DiscoveredInstruction>), DecodeError> {
    let mut order = Vec::new();
    let mut discovered = HashMap::<BlockKey, DiscoveredInstruction>::new();
    let mut states = HashMap::<BlockKey, AbstractState>::new();
    let mut queue = VecDeque::<BlockKey>::new();
    states.insert(entry.clone(), AbstractState::default());
    queue.push_back(entry);

    while let Some(key) = queue.pop_front() {
        let state = states.get(&key).copied().unwrap_or_default();
        let instruction = decode_at(rom, key.clone())?;
        let state_after = transfer_instruction(rom, instruction, state);
        let successors = instruction_successors(rom, instruction, state_after);

        if !discovered.contains_key(&key) {
            order.push(key.clone());
        }
        let previous = discovered.insert(
            key.clone(),
            DiscoveredInstruction {
                instruction,
                successors: successors.clone(),
            },
        );
        let edges_changed = previous
            .as_ref()
            .map(|node| node.successors != successors)
            .unwrap_or(true);

        for successor in successors {
            if !in_rom(rom, successor.address) {
                continue;
            }
            let should_queue = match states.get(&successor).copied() {
                Some(existing) => {
                    let joined = existing.join(state_after);
                    if joined != existing {
                        states.insert(successor.clone(), joined);
                        true
                    } else {
                        edges_changed
                    }
                }
                None => {
                    states.insert(successor.clone(), state_after);
                    true
                }
            };
            if should_queue {
                queue.push_back(successor);
            }
        }
    }
    Ok((order, discovered))
}

fn is_fallthrough(node: &DiscoveredInstruction) -> bool {
    node.successors.len() == 1
        && node.successors[0] == next_key(node.instruction)
        && !is_call(node.instruction)
}

fn collect_leaders(
    _order: &[BlockKey],
    discovered: &HashMap<BlockKey, DiscoveredInstruction>,
    entry: &BlockKey,
) -> Vec<BlockKey> {
    let mut leaders = HashSet::<BlockKey>::new();
    leaders.insert(entry.clone());
    for node in discovered.values() {
        if !is_fallthrough(node) {
            for successor in &node.successors {
                if discovered.contains_key(successor) {
                    leaders.insert(successor.clone());
                }
            }
        }
    }
    let mut leaders = leaders.into_iter().collect::<Vec<_>>();
    leaders.sort_by(|a, b| {
        a.address
            .cmp(&b.address)
            .then_with(|| mode_sort_key(a.mode).cmp(&mode_sort_key(b.mode)))
    });
    leaders
}
fn mode_sort_key(mode: Mode) -> u8 {
    match mode {
        Mode::Arm => 0,
        Mode::Thumb => 1,
    }
}

fn partition_blocks(
    _order: &[BlockKey],
    discovered: &HashMap<BlockKey, DiscoveredInstruction>,
    leaders: &[BlockKey],
) -> (Vec<BasicBlock>, HashMap<BlockKey, BlockId>) {
    let leader_set = leaders.iter().cloned().collect::<HashSet<_>>();
    let mut blocks = Vec::new();
    let mut ids = HashMap::<BlockKey, BlockId>::new();
    for leader in leaders {
        let id = BlockId(blocks.len());
        ids.insert(leader.clone(), id);
        let mut instructions = Vec::new();
        let mut cursor = leader.clone();
        while let Some(node) = discovered.get(&cursor) {
            instructions.push(node.instruction);
            if !is_fallthrough(node) {
                break;
            }
            let next = node.successors[0].clone();
            if leader_set.contains(&next) {
                break;
            }
            cursor = next;
        }
        let ir = instructions.iter().copied().map(lower).collect::<Vec<_>>();
        assert_eq!(
            instructions.len(),
            ir.len(),
            "IR must preserve one entry per instruction"
        );
        blocks.push(BasicBlock {
            id,
            key: leader.clone(),
            instructions,
            ir,
            successors: Vec::new(),
        });
    }
    for block in &mut blocks {
        let Some(last) = block.instructions.last().copied() else {
            continue;
        };
        let key = BlockKey {
            address: last.address,
            mode: last.mode,
        };
        let Some(node) = discovered.get(&key) else {
            continue;
        };
        let mut successors = node
            .successors
            .iter()
            .filter_map(|successor| ids.get(successor).copied())
            .collect::<Vec<_>>();
        successors.sort_unstable_by_key(|id| id.0);
        successors.dedup();
        block.successors = successors;
    }
    (blocks, ids)
}

fn validate_cfg(cfg: &ControlFlowGraph, expected_instruction_count: usize) {
    assert!(!cfg.blocks.is_empty());
    assert!(cfg.entry.0 < cfg.blocks.len());
    let mut seen = HashMap::<BlockKey, BlockId>::new();
    let mut owners = HashMap::<BlockKey, BlockId>::new();
    for block in &cfg.blocks {
        assert_eq!(block.id, BlockId(seen.len()));
        assert!(seen.insert(block.key.clone(), block.id).is_none());
        assert!(!block.instructions.is_empty());
        assert_eq!(block.instructions.len(), block.ir.len());
        let first = block.instructions[0];
        assert_eq!(
            block.key,
            BlockKey {
                address: first.address,
                mode: first.mode
            }
        );
        for (index, instruction) in block.instructions.iter().enumerate() {
            let key = BlockKey {
                address: instruction.address,
                mode: instruction.mode,
            };
            assert!(owners.insert(key, block.id).is_none());
            if let Some(next) = block.instructions.get(index + 1) {
                assert_eq!(instruction.address + instruction.size as u32, next.address);
                assert_eq!(instruction.mode, next.mode);
            }
        }
        for successor in &block.successors {
            assert!(successor.0 < cfg.blocks.len());
        }
    }
    assert_eq!(owners.len(), expected_instruction_count);
}

pub fn analyze(rom: &[u8], entry: u32, entry_mode: Mode) -> Result<Program, AnalysisError> {
    if !in_rom(rom, entry) {
        return Err(AnalysisError::InvalidEntry(entry));
    }
    let entry_key = BlockKey {
        address: entry,
        mode: entry_mode,
    };
    let (order, discovered) = discover_reachable(rom, entry_key.clone())?;
    let leaders = collect_leaders(&order, &discovered, &entry_key);
    let (blocks, ids) = partition_blocks(&order, &discovered, &leaders);
    let entry_id = ids[&entry_key];
    let cfg = ControlFlowGraph { entry: entry_id, blocks };
    validate_cfg(&cfg, discovered.len());
    Ok(Program { entry: entry_id, cfg })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm_rom(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    #[test]
    fn discovery_preserves_sequential_arm_tail() {
        let bytes = arm_rom(&[0xE3A0_0001, 0xE280_0001]);
        let entry = BlockKey { address: ROM_BASE, mode: Mode::Arm };
        let (order, discovered) = discover_reachable(&bytes, entry).unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(discovered.len(), 2);
    }

    #[test]
    fn sequential_arm_instructions_remain_in_one_block() {
        let bytes = arm_rom(&[0xE3A0_0001, 0xE280_0001]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert_eq!(program.cfg.blocks[0].instructions.len(), 2);
    }

    #[test]
    fn discovers_arm_branch_and_fallthrough() {
        let bytes = arm_rom(&[0xEA00_0000, 0xE1A0_0000, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].successors, vec![BlockId(1)]);
    }

    #[test]
    fn conditional_branch_splits_fallthrough_and_target() {
        let bytes = vec![0x00, 0xD0, 0xC0, 0x46, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 3);
        assert_eq!(program.cfg.blocks[0].successors.len(), 2);
    }

    #[test]
    fn backward_branch_splits_an_already_discovered_block() {
        let bytes = arm_rom(&[0xEAFF_FFFE, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
    }

    #[test]
    fn conditional_backward_branch_does_not_overlap_blocks() {
        let bytes = vec![0x00, 0xD0, 0xC0, 0x46, 0xFC, 0xD0, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        let mut keys = HashSet::new();
        for block in &program.cfg.blocks {
            for instruction in &block.instructions {
                assert!(keys.insert((instruction.address, instruction.mode)));
            }
        }
    }

    #[test]
    fn unknown_instruction_terminates_discovery() {
        let bytes = arm_rom(&[0xFFFFFFFF, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
    }

    #[test]
    fn thumb_bl_is_a_four_byte_call_with_fallthrough() {
        let bytes = vec![0x00, 0xF0, 0x00, 0xF8, 0xC0, 0x46, 0xC0, 0x46];
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].instructions[0].size, 4);
    }

    #[test]
    fn unresolved_indirect_call_keeps_continuation_block() {
        let program = analyze(&arm_rom(&[0xE12F_FF31, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 2);
        assert_eq!(program.cfg.blocks[0].successors, vec![BlockId(1)]);
    }

    #[test]
    fn resolves_arm_bx_from_pc_relative_literal() {
        let target = ROM_BASE + 8;
        let bytes = arm_rom(&[0xE59F_0000, target, 0xE1A0_0000]);
        let program = analyze(&bytes, ROM_BASE, Mode::Arm).unwrap();
        assert!(program
            .cfg
            .blocks
            .iter()
            .any(|block| block.key == BlockKey { address: target, mode: Mode::Arm }));
    }

    #[test]
    fn resolves_thumb_bx_from_pc_relative_literal() {
        let target = ROM_BASE + 8;
        let mut bytes = vec![0x00, 0x48, 0x00, 0x47];
        bytes.extend_from_slice(&target.to_le_bytes());
        bytes.extend_from_slice(&0x46C0u16.to_le_bytes());
        let program = analyze(&bytes, ROM_BASE, Mode::Thumb).unwrap();
        assert!(program
            .cfg
            .blocks
            .iter()
            .any(|block| block.key == BlockKey { address: target, mode: Mode::Arm }));
    }

    #[test]
    fn unresolved_indirect_branch_does_not_invent_a_target() {
        let program = analyze(&arm_rom(&[0xE12F_FF10, 0xE1A0_0000]), ROM_BASE, Mode::Arm).unwrap();
        assert_eq!(program.cfg.blocks.len(), 1);
        assert!(program.cfg.blocks[0].successors.is_empty());
    }
}