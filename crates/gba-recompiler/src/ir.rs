use crate::decoder::{ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2, ThumbOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Reg(u8),
    Imm(u32),
}

impl Value {
    pub fn register(&self) -> Option<u8> {
        match self {
            Self::Reg(reg) => Some(*reg),
            Self::Imm(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrMemoryWidth { Byte, Word }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrMemoryKind { Read, Write }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrMemoryEffect {
    pub kind: IrMemoryKind,
    pub width: IrMemoryWidth,
    pub base: u8,
    pub address_is_dynamic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IrFlags {
    pub read_n: bool,
    pub read_z: bool,
    pub read_c: bool,
    pub read_v: bool,
    pub write_n: bool,
    pub write_z: bool,
    pub write_c: bool,
    pub write_v: bool,
}

impl IrFlags {
    pub fn condition_read(condition: Condition) -> Self {
        match condition {
            Condition::Al => Self::default(),
            Condition::Eq | Condition::Ne => Self { read_z: true, ..Self::default() },
            Condition::Cs | Condition::Cc => Self { read_c: true, ..Self::default() },
            Condition::Mi | Condition::Pl => Self { read_n: true, ..Self::default() },
            Condition::Vs | Condition::Vc => Self { read_v: true, ..Self::default() },
            Condition::Hi | Condition::Ls => Self { read_c: true, read_z: true, ..Self::default() },
            Condition::Ge | Condition::Lt => Self { read_n: true, read_v: true, ..Self::default() },
            Condition::Gt | Condition::Le => Self { read_n: true, read_v: true, read_z: true, ..Self::default() },
        }
    }

    pub const fn arithmetic_write(set_flags: bool) -> Self {
        if set_flags {
            Self { write_n: true, write_z: true, write_c: true, write_v: true, ..Self::default() }
        } else {
            Self::default()
        }
    }

    pub const fn compare_write() -> Self {
        Self { write_n: true, write_z: true, write_c: true, write_v: true, ..Self::default() }
    }

    pub fn reads_any(&self) -> bool { self.read_n || self.read_z || self.read_c || self.read_v }
    pub fn writes_any(&self) -> bool { self.write_n || self.write_z || self.write_c || self.write_v }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrControlEffect {
    None,
    Branch { target: u32, condition: Condition, link: bool },
    BranchExchange { register: u8, link: bool },
    Unknown,
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

impl IrOp {
    pub fn reads(&self) -> Vec<u8> {
        match self {
            Self::Nop | Self::Unknown { .. } => Vec::new(),
            Self::Mov { src, .. } => src.register().into_iter().collect(),
            Self::Add { lhs, rhs, .. } | Self::Sub { lhs, rhs, .. } => {
                let mut reads = vec![*lhs];
                if let Some(register) = rhs.register() { reads.push(register); }
                reads
            }
            Self::Cmp { lhs, rhs } => {
                let mut reads = vec![*lhs];
                if let Some(register) = rhs.register() { reads.push(register); }
                reads
            }
            Self::Load { base, .. } => vec![*base],
            Self::Store { src, base, .. } => vec![*src, *base],
            Self::Branch { .. } => Vec::new(),
            Self::BranchExchange { register, .. } => vec![*register],
        }
    }

    pub fn writes(&self) -> Vec<u8> {
        match self {
            Self::Mov { dst, .. } => vec![*dst],
            Self::Add { dst, .. } | Self::Sub { dst, .. } => vec![*dst],
            Self::Load { dst, .. } => vec![*dst],
            Self::Branch { link: true, .. } | Self::BranchExchange { link: true, .. } => vec![14],
            Self::Nop | Self::Cmp { .. } | Self::Store { .. } | Self::Branch { link: false, .. }
            | Self::BranchExchange { link: false, .. } | Self::Unknown { .. } => Vec::new(),
        }
    }

    pub fn flags(&self) -> IrFlags {
        match self {
            Self::Add { .. } | Self::Sub { .. } => IrFlags::default(),
            Self::Cmp { .. } => IrFlags::compare_write(),
            Self::Branch { condition, .. } => IrFlags::condition_read(*condition),
            Self::Nop | Self::Mov { .. } | Self::Load { .. } | Self::Store { .. }
            | Self::BranchExchange { .. } | Self::Unknown { .. } => IrFlags::default(),
        }
    }

    pub fn memory(&self) -> Option<IrMemoryEffect> {
        match self {
            Self::Load { base, byte, offset, .. } => Some(IrMemoryEffect {
                kind: IrMemoryKind::Read,
                width: if *byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word },
                base: *base,
                address_is_dynamic: *offset != 0,
            }),
            Self::Store { base, byte, offset, .. } => Some(IrMemoryEffect {
                kind: IrMemoryKind::Write,
                width: if *byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word },
                base: *base,
                address_is_dynamic: *offset != 0,
            }),
            _ => None,
        }
    }

    pub fn control(&self) -> IrControlEffect {
        match self {
            Self::Branch { target, condition, link } => IrControlEffect::Branch { target: *target, condition: *condition, link: *link },
            Self::BranchExchange { register, link } => IrControlEffect::BranchExchange { register: *register, link: *link },
            Self::Unknown { .. } => IrControlEffect::Unknown,
            _ => IrControlEffect::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInstruction {
    pub address: u32,
    pub size: u8,
    pub ops: Vec<IrOp>,
}

impl IrInstruction {
    pub fn new(address: u32, size: u8, ops: Vec<IrOp>) -> Self { Self { address, size, ops } }

    pub fn reads(&self) -> Vec<u8> {
        let mut reads = self.ops.iter().flat_map(IrOp::reads).collect::<Vec<_>>();
        reads.sort_unstable();
        reads.dedup();
        reads
    }

    pub fn writes(&self) -> Vec<u8> {
        let mut writes = self.ops.iter().flat_map(IrOp::writes).collect::<Vec<_>>();
        writes.sort_unstable();
        writes.dedup();
        writes
    }

    pub fn flags(&self) -> IrFlags {
        self.ops.iter().fold(IrFlags::default(), |mut flags, op| {
            let current = op.flags();
            flags.read_n |= current.read_n;
            flags.read_z |= current.read_z;
            flags.read_c |= current.read_c;
            flags.read_v |= current.read_v;
            flags.write_n |= current.write_n;
            flags.write_z |= current.write_z;
            flags.write_c |= current.write_c;
            flags.write_v |= current.write_v;
            flags
        })
    }

    pub fn memory(&self) -> Option<IrMemoryEffect> { self.ops.iter().find_map(IrOp::memory) }

    pub fn control(&self) -> IrControlEffect {
        self.ops
            .iter()
            .rev()
            .map(IrOp::control)
            .find(|effect| !matches!(effect, &IrControlEffect::None))
            .unwrap_or(IrControlEffect::None)
    }
}

fn operand(op: Operand2) -> Value {
    match op {
        Operand2::Imm(v) => Value::Imm(v),
        Operand2::Reg { rm, .. } => Value::Reg(rm),
    }
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
        InstructionKind::Arm(ArmOp::Extended(_))
        | InstructionKind::Arm(ArmOp::Unknown)
        | InstructionKind::Thumb(ThumbOp::Extended(_))
        | InstructionKind::Thumb(ThumbOp::Unknown) => vec![IrOp::Unknown { address: ins.address, raw: ins.raw, mode: ins.mode }],
    };
    IrInstruction::new(ins.address, ins.size, op)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_effects_are_derived_once() {
        let instruction = IrInstruction::new(0x0800_0000, 4, vec![IrOp::Add { dst: 0, lhs: 1, rhs: Value::Reg(2) }]);
        assert_eq!(instruction.reads(), vec![1, 2]);
        assert_eq!(instruction.writes(), vec![0]);
        assert_eq!(instruction.memory(), None);
        assert_eq!(instruction.control(), IrControlEffect::None);
    }

    #[test]
    fn condition_flags_are_explicit() {
        let instruction = IrInstruction::new(0x0800_0000, 4, vec![IrOp::Branch { target: 0x0800_0010, condition: Condition::Gt, link: false }]);
        let flags = instruction.flags();
        assert!(flags.read_n && flags.read_v && flags.read_z);
        assert!(!flags.writes_any());
    }

    #[test]
    fn memory_effect_keeps_width_and_base() {
        let instruction = IrInstruction::new(0x0800_0000, 4, vec![IrOp::Load { dst: 0, base: 1, offset: 4, byte: false }]);
        assert_eq!(instruction.memory(), Some(IrMemoryEffect { kind: IrMemoryKind::Read, width: IrMemoryWidth::Word, base: 1, address_is_dynamic: true }));
    }
}
