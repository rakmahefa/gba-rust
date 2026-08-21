use std::collections::{HashMap, HashSet};

use crate::decoder::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstructionKey {
    pub address: u32,
    pub mode: Mode,
    pub size: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryWidth {
    Byte,
    Halfword,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryEffect {
    pub access: MemoryAccess,
    pub width: MemoryWidth,
    pub base_register: Option<u8>,
    pub dynamic_address: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FlagEffects {
    pub read_n: bool,
    pub read_z: bool,
    pub read_c: bool,
    pub read_v: bool,
    pub write_n: bool,
    pub write_z: bool,
    pub write_c: bool,
    pub write_v: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlEffect {
    Fallthrough,
    Branch { target: u32, conditional: bool, link: bool },
    Exchange { register: u8, link: bool },
    Return { register: u8 },
    Exception,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitecturalInstruction {
    pub key: InstructionKey,
    pub reads: HashSet<u8>,
    pub writes: HashSet<u8>,
    pub flags: FlagEffects,
    pub memory: Option<MemoryEffect>,
    pub control: ControlEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitecturalBlock {
    pub address: u32,
    pub mode: Mode,
    pub instructions: Vec<ArchitecturalInstruction>,
    pub successors: HashSet<(u32, Mode)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    DuplicateInstruction(InstructionKey),
    NonContiguousBlock { block: u32, expected: u32, actual: u32 },
    InvalidSuccessor { block: u32, address: u32, mode: Mode },
    EmptyBlock(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchitecturalProgram {
    pub blocks: HashMap<(u32, Mode), ArchitecturalBlock>,
}

impl ArchitecturalProgram {
    pub fn validate(&self) -> Result<(), ContractError> {
        let mut instructions = HashSet::new();
        for ((address, mode), block) in &self.blocks {
            if block.instructions.is_empty() {
                return Err(ContractError::EmptyBlock(*address));
            }
            let mut expected = *address;
            for instruction in &block.instructions {
                if !instructions.insert(instruction.key) {
                    return Err(ContractError::DuplicateInstruction(instruction.key));
                }
                if instruction.key.address != expected || instruction.key.mode != *mode {
                    return Err(ContractError::NonContiguousBlock {
                        block: *address,
                        expected,
                        actual: instruction.key.address,
                    });
                }
                expected = expected.wrapping_add(u32::from(instruction.key.size));
            }
            for (target, target_mode) in &block.successors {
                if !self.blocks.contains_key(&(*target, *target_mode)) {
                    return Err(ContractError::InvalidSuccessor {
                        block: *address,
                        address: *target,
                        mode: *target_mode,
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(address: u32, mode: Mode, size: u8) -> ArchitecturalInstruction {
        ArchitecturalInstruction {
            key: InstructionKey { address, mode, size },
            reads: HashSet::new(),
            writes: HashSet::new(),
            flags: FlagEffects::default(),
            memory: None,
            control: ControlEffect::Fallthrough,
        }
    }

    #[test]
    fn validates_instruction_continuity_and_successors() {
        let mut program = ArchitecturalProgram::default();
        program.blocks.insert(
            (0x0800_0000, Mode::Arm),
            ArchitecturalBlock {
                address: 0x0800_0000,
                mode: Mode::Arm,
                instructions: vec![
                    instruction(0x0800_0000, Mode::Arm, 4),
                    instruction(0x0800_0004, Mode::Arm, 4),
                ],
                successors: HashSet::new(),
            },
        );
        assert!(program.validate().is_ok());
    }

    #[test]
    fn rejects_unknown_successor() {
        let mut program = ArchitecturalProgram::default();
        let mut successors = HashSet::new();
        successors.insert((0x0800_0010, Mode::Thumb));
        program.blocks.insert(
            (0x0800_0000, Mode::Arm),
            ArchitecturalBlock {
                address: 0x0800_0000,
                mode: Mode::Arm,
                instructions: vec![instruction(0x0800_0000, Mode::Arm, 4)],
                successors,
            },
        );
        assert!(matches!(program.validate(), Err(ContractError::InvalidSuccessor { .. })));
    }
}
