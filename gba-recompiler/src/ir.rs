use crate::decoder::{
    ArmDataOp, ArmExtended, ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2,
    ThumbAluOp, ThumbExtended, ThumbOp,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value { Reg(u8), Imm(u32) }
impl Value { pub fn register(&self) -> Option<u8> { match self { Self::Reg(r) => Some(*r), Self::Imm(_) => None } } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrMemoryWidth { Byte, Halfword, Word }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrMemoryKind { Read, Write, ReadWrite }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrMemoryEffect { pub kind: IrMemoryKind, pub width: IrMemoryWidth, pub base: u8, pub address_is_dynamic: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IrFlags {
    pub read_n: bool, pub read_z: bool, pub read_c: bool, pub read_v: bool,
    pub write_n: bool, pub write_z: bool, pub write_c: bool, pub write_v: bool,
}
impl IrFlags {
    pub fn condition_read(c: Condition) -> Self {
        match c {
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
    pub fn arithmetic_write(set: bool) -> Self { if set { Self { write_n: true, write_z: true, write_c: true, write_v: true, ..Self::default() } } else { Self::default() } }
    pub fn logical_write(set: bool) -> Self { if set { Self { write_n: true, write_z: true, write_c: true, ..Self::default() } } else { Self::default() } }
    pub fn compare_write() -> Self { Self { write_n: true, write_z: true, write_c: true, write_v: true, ..Self::default() } }
    pub fn shift_write() -> Self { Self { write_n: true, write_z: true, write_c: true, ..Self::default() } }
    pub fn reads_any(&self) -> bool { self.read_n || self.read_z || self.read_c || self.read_v }
    pub fn writes_any(&self) -> bool { self.write_n || self.write_z || self.write_c || self.write_v }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrControlEffect { None, Branch { target: u32, condition: Condition, link: bool }, BranchExchange { register: u8, link: bool }, Unknown }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrOp {
    Nop,
    Mov { dst: u8, src: Value, set_flags: bool },
    Add { dst: u8, lhs: u8, rhs: Value, set_flags: bool },
    Sub { dst: u8, lhs: u8, rhs: Value, set_flags: bool },
    Cmp { lhs: u8, rhs: Value },
    Load { dst: u8, base: u8, offset: i32, byte: bool },
    Store { src: u8, base: u8, offset: i32, byte: bool },
    Branch { target: u32, condition: Condition, link: bool },
    BranchExchange { register: u8, link: bool },
    ArmExtended { op: ArmExtended },
    ThumbExtended { op: ThumbExtended },
    Unknown { address: u32, raw: u32, mode: Mode },
}

fn add_unique(v: &mut Vec<u8>, r: u8) { if !v.contains(&r) { v.push(r); } }
fn operand_reads(v: &mut Vec<u8>, op: Operand2) { if let Operand2::Reg { rm, by_register, shift_register, .. } = op { add_unique(v, rm); if by_register { add_unique(v, shift_register); } } }
fn reg_list(v: &mut Vec<u8>, list: u16) { for r in 0..16 { if list & (1 << r) != 0 { add_unique(v, r as u8); } } }
fn reg_list8(v: &mut Vec<u8>, list: u8) { for r in 0..8 { if list & (1 << r) != 0 { add_unique(v, r as u8); } } }

fn arm_reads(op: ArmExtended) -> Vec<u8> {
    let mut v = Vec::new();
    match op {
        ArmExtended::DataProcessing { op, rn, op2, .. } => { if !matches!(op, ArmDataOp::Mov | ArmDataOp::Mvn) { add_unique(&mut v, rn); } operand_reads(&mut v, op2); }
        ArmExtended::Multiply { rn, rs, rm, accumulate, .. } => { add_unique(&mut v, rm); add_unique(&mut v, rs); if accumulate { add_unique(&mut v, rn); } }
        ArmExtended::MultiplyLong { rd_hi, rd_lo, rs, rm, accumulate, .. } => { add_unique(&mut v, rm); add_unique(&mut v, rs); if accumulate { add_unique(&mut v, rd_lo); add_unique(&mut v, rd_hi); } }
        ArmExtended::Swap { rn, rm, .. } => { add_unique(&mut v, rn); add_unique(&mut v, rm); }
        ArmExtended::HalfwordTransfer { rn, .. } => add_unique(&mut v, rn),
        ArmExtended::SingleDataTransfer { rn, offset, .. } => { add_unique(&mut v, rn); operand_reads(&mut v, offset); }
        ArmExtended::BlockTransfer { rn, register_list, load, .. } => { add_unique(&mut v, rn); if !load { reg_list(&mut v, register_list); } }
        ArmExtended::Mrs { .. } => {}
        ArmExtended::Msr { source, .. } => operand_reads(&mut v, source),
        ArmExtended::SoftwareInterrupt { .. } => {}
        ArmExtended::CoprocessorTransfer { crn, crd, .. } => { add_unique(&mut v, crn); add_unique(&mut v, crd); }
        ArmExtended::CoprocessorData { crn, crd, .. } => { add_unique(&mut v, crn); add_unique(&mut v, crd); }
        ArmExtended::CoprocessorRegisterTransfer { rd, crn, .. } => { add_unique(&mut v, rd); add_unique(&mut v, crn); }
    }
    v
}
fn arm_writes(op: ArmExtended) -> Vec<u8> {
    let mut v = Vec::new();
    match op {
        ArmExtended::DataProcessing { op, rd, .. } => if !matches!(op, ArmDataOp::Tst | ArmDataOp::Teq | ArmDataOp::Cmp | ArmDataOp::Cmn) { v.push(rd); },
        ArmExtended::Multiply { rd, .. } => v.push(rd),
        ArmExtended::MultiplyLong { rd_hi, rd_lo, .. } => { v.push(rd_lo); if rd_hi != rd_lo { v.push(rd_hi); } }
        ArmExtended::Swap { rd, .. } => v.push(rd),
        ArmExtended::HalfwordTransfer { load, rd, rn, write_back, .. } | ArmExtended::SingleDataTransfer { load, rd, rn, write_back, .. } => { if load { v.push(rd); } if write_back && rn != rd { v.push(rn); } }
        ArmExtended::BlockTransfer { load, rn, register_list, write_back, .. } => { if load { reg_list(&mut v, register_list); } if write_back { add_unique(&mut v, rn); } }
        ArmExtended::Mrs { rd, .. } => v.push(rd),
        ArmExtended::Msr { .. } | ArmExtended::SoftwareInterrupt { .. } | ArmExtended::CoprocessorData { .. } => {}
        ArmExtended::CoprocessorTransfer { load, crd, .. } => if load { v.push(crd) },
        ArmExtended::CoprocessorRegisterTransfer { to_arm, rd, .. } => if to_arm { v.push(rd) },
    }
    v
}
fn arm_flags(op: ArmExtended) -> IrFlags {
    match op {
        ArmExtended::DataProcessing { op, set_flags, .. } => match op {
            ArmDataOp::Tst | ArmDataOp::Teq | ArmDataOp::Cmp | ArmDataOp::Cmn => IrFlags::compare_write(),
            ArmDataOp::And | ArmDataOp::Eor | ArmDataOp::Orr | ArmDataOp::Bic | ArmDataOp::Mov | ArmDataOp::Mvn => IrFlags::logical_write(set_flags),
            ArmDataOp::Sub | ArmDataOp::Rsb | ArmDataOp::Add | ArmDataOp::Adc | ArmDataOp::Sbc | ArmDataOp::Rsc => {
                let mut f = IrFlags::arithmetic_write(set_flags);
                if matches!(op, ArmDataOp::Adc | ArmDataOp::Sbc | ArmDataOp::Rsc) { f.read_c = true; }
                f
            }
        },
        ArmExtended::Multiply { set_flags, .. } | ArmExtended::MultiplyLong { set_flags, .. } => if set_flags { IrFlags { write_n: true, write_z: true, ..IrFlags::default() } } else { IrFlags::default() },
        _ => IrFlags::default(),
    }
}
fn arm_memory(op: ArmExtended) -> Option<IrMemoryEffect> {
    match op {
        ArmExtended::Swap { rn, byte, .. } => Some(IrMemoryEffect { kind: IrMemoryKind::ReadWrite, width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word }, base: rn, address_is_dynamic: true }),
        ArmExtended::HalfwordTransfer { rn, load, halfword, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: if halfword { IrMemoryWidth::Halfword } else { IrMemoryWidth::Byte }, base: rn, address_is_dynamic: true }),
        ArmExtended::SingleDataTransfer { rn, load, byte, offset, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word }, base: rn, address_is_dynamic: matches!(offset, Operand2::Reg { .. }) }),
        ArmExtended::BlockTransfer { rn, load, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: IrMemoryWidth::Word, base: rn, address_is_dynamic: true }),
        ArmExtended::CoprocessorTransfer { crn, load, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: IrMemoryWidth::Word, base: crn, address_is_dynamic: true }),
        _ => None,
    }
}

fn thumb_reads(op: ThumbExtended) -> Vec<u8> {
    let mut v = Vec::new();
    match op {
        ThumbExtended::MoveShifted { rs, .. } => add_unique(&mut v, rs),
        ThumbExtended::AddSubRegister { rd, rs, rn, .. } => { add_unique(&mut v, rd); add_unique(&mut v, rs); add_unique(&mut v, rn); }
        ThumbExtended::AddSubImmediate { rd, rs, .. } => { add_unique(&mut v, rd); add_unique(&mut v, rs); }
        ThumbExtended::Alu { rd, rs, .. } => { add_unique(&mut v, rd); add_unique(&mut v, rs); }
        ThumbExtended::HighRegister { op, rd, rs } => { add_unique(&mut v, rs); if op == 0 || op == 1 { add_unique(&mut v, rd); } }
        ThumbExtended::PcRelativeLoad { .. } => add_unique(&mut v, 15),
        ThumbExtended::LoadStoreRegister { load, rd, rb, ro, .. } => { add_unique(&mut v, rb); add_unique(&mut v, ro); if !load { add_unique(&mut v, rd); } }
        ThumbExtended::LoadStoreSignHalf { kind, rd, rb, ro } => { add_unique(&mut v, rb); add_unique(&mut v, ro); if kind == 0 { add_unique(&mut v, rd); } }
        ThumbExtended::LoadStoreImmediate { load, rd, rb, .. } | ThumbExtended::LoadStoreHalfword { load, rd, rb, .. } => { add_unique(&mut v, rb); if !load { add_unique(&mut v, rd); } }
        ThumbExtended::SpRelativeLoadStore { load, rd, .. } => { add_unique(&mut v, 13); if !load { add_unique(&mut v, rd); } }
        ThumbExtended::Address { use_sp, .. } => add_unique(&mut v, if use_sp { 13 } else { 15 }),
        ThumbExtended::AddSp { .. } => add_unique(&mut v, 13),
        ThumbExtended::PushPop { load, registers, extra_lr_pc } => { add_unique(&mut v, 13); if !load { reg_list8(&mut v, registers); if extra_lr_pc { add_unique(&mut v, 14); } } }
        ThumbExtended::MultipleLoadStore { load, rb, register_list } => { add_unique(&mut v, rb); if !load { reg_list8(&mut v, register_list); } }
        ThumbExtended::SoftwareInterrupt { .. } => {}
    }
    v
}
fn thumb_writes(op: ThumbExtended) -> Vec<u8> {
    let mut v = Vec::new();
    match op {
        ThumbExtended::MoveShifted { rd, .. } | ThumbExtended::AddSubRegister { rd, .. } | ThumbExtended::AddSubImmediate { rd, .. } => v.push(rd),
        ThumbExtended::Alu { op, rd, .. } => if !matches!(op, ThumbAluOp::Tst | ThumbAluOp::Cmp | ThumbAluOp::Cmn) { v.push(rd) },
        ThumbExtended::HighRegister { op, rd, .. } => if op == 0 || op == 2 { v.push(rd) },
        ThumbExtended::PcRelativeLoad { rd, .. }
        | ThumbExtended::LoadStoreImmediate { load: true, rd, .. }
        | ThumbExtended::LoadStoreHalfword { load: true, rd, .. }
        | ThumbExtended::SpRelativeLoadStore { load: true, rd, .. }
        | ThumbExtended::LoadStoreRegister { load: true, rd, .. }
        | ThumbExtended::LoadStoreSignHalf { kind: 1..=3, rd, .. }
        | ThumbExtended::Address { rd, .. } => v.push(rd),
        ThumbExtended::AddSp { .. } => v.push(13),
        ThumbExtended::PushPop { load, registers, extra_lr_pc } => { if load { reg_list8(&mut v, registers); if extra_lr_pc { v.push(15); } } v.push(13); }
        ThumbExtended::MultipleLoadStore { load, rb, register_list } => { if load { reg_list8(&mut v, register_list); } v.push(rb); }
        ThumbExtended::LoadStoreRegister { load: false, .. }
        | ThumbExtended::LoadStoreSignHalf { kind: 0, .. }
        | ThumbExtended::LoadStoreSignHalf { kind: 4..=u8::MAX, .. }
        | ThumbExtended::LoadStoreImmediate { load: false, .. }
        | ThumbExtended::LoadStoreHalfword { load: false, .. }
        | ThumbExtended::SpRelativeLoadStore { load: false, .. }
        | ThumbExtended::SoftwareInterrupt { .. } => {}
    }
    v
}
fn thumb_flags(op: ThumbExtended) -> IrFlags {
    match op {
        ThumbExtended::MoveShifted { .. } => IrFlags::shift_write(),
        ThumbExtended::AddSubRegister { .. } | ThumbExtended::AddSubImmediate { .. } => IrFlags::arithmetic_write(true),
        ThumbExtended::Alu { op, .. } => match op {
            ThumbAluOp::Tst | ThumbAluOp::Cmp | ThumbAluOp::Cmn => IrFlags::compare_write(),
            ThumbAluOp::Lsl | ThumbAluOp::Lsr | ThumbAluOp::Asr => IrFlags::shift_write(),
            ThumbAluOp::Ror => IrFlags { read_c: true, write_n: true, write_z: true, write_c: true, ..IrFlags::default() },
            _ => {
                let mut f = IrFlags { write_n: true, write_z: true, write_c: true, ..IrFlags::default() };
                if matches!(op, ThumbAluOp::Adc | ThumbAluOp::Sbc) { f.read_c = true; f.write_v = true; }
                if matches!(op, ThumbAluOp::Neg | ThumbAluOp::Mul) { f.write_v = matches!(op, ThumbAluOp::Neg); }
                f
            }
        },
        ThumbExtended::HighRegister { op: 1, .. } => IrFlags::compare_write(),
        _ => IrFlags::default(),
    }
}
fn thumb_memory(op: ThumbExtended) -> Option<IrMemoryEffect> {
    match op {
        ThumbExtended::PcRelativeLoad { .. } => Some(IrMemoryEffect { kind: IrMemoryKind::Read, width: IrMemoryWidth::Word, base: 15, address_is_dynamic: true }),
        ThumbExtended::LoadStoreRegister { load, byte, rb, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word }, base: rb, address_is_dynamic: true }),
        ThumbExtended::LoadStoreSignHalf { kind, rb, .. } => Some(IrMemoryEffect { kind: if kind == 0 { IrMemoryKind::Write } else { IrMemoryKind::Read }, width: if kind == 1 { IrMemoryWidth::Byte } else { IrMemoryWidth::Halfword }, base: rb, address_is_dynamic: true }),
        ThumbExtended::LoadStoreImmediate { load, byte, rb, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word }, base: rb, address_is_dynamic: true }),
        ThumbExtended::LoadStoreHalfword { load, rb, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: IrMemoryWidth::Halfword, base: rb, address_is_dynamic: true }),
        ThumbExtended::SpRelativeLoadStore { load, .. } | ThumbExtended::PushPop { load, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: IrMemoryWidth::Word, base: 13, address_is_dynamic: true }),
        ThumbExtended::MultipleLoadStore { load, rb, .. } => Some(IrMemoryEffect { kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write }, width: IrMemoryWidth::Word, base: rb, address_is_dynamic: true }),
        _ => None,
    }
}

impl IrOp {
    pub fn reads(&self) -> Vec<u8> {
        let mut v = match self {
            Self::Nop | Self::Unknown { .. } => Vec::new(),
            Self::Mov { src, .. } => src.register().into_iter().collect(),
            Self::Add { lhs, rhs, .. } | Self::Sub { lhs, rhs, .. } | Self::Cmp { lhs, rhs } => { let mut r = vec![*lhs]; if let Some(x) = rhs.register() { r.push(x); } r }
            Self::Load { base, .. } => vec![*base],
            Self::Store { src, base, .. } => vec![*src, *base],
            Self::Branch { .. } => Vec::new(),
            Self::BranchExchange { register, .. } => vec![*register],
            Self::ArmExtended { op } => arm_reads(*op),
            Self::ThumbExtended { op } => thumb_reads(*op),
        };
        v.sort_unstable(); v.dedup(); v
    }
    pub fn writes(&self) -> Vec<u8> {
        let mut v = match self {
            Self::Mov { dst, .. } | Self::Add { dst, .. } | Self::Sub { dst, .. } | Self::Load { dst, .. } => vec![*dst],
            Self::Branch { link: true, .. } | Self::BranchExchange { link: true, .. } => vec![14],
            Self::ArmExtended { op } => arm_writes(*op),
            Self::ThumbExtended { op } => thumb_writes(*op),
            _ => Vec::new(),
        };
        v.sort_unstable(); v.dedup(); v
    }
    pub fn flags(&self) -> IrFlags {
        match self {
            Self::Mov { set_flags, .. } => IrFlags::logical_write(*set_flags),
            Self::Add { set_flags, .. } | Self::Sub { set_flags, .. } => IrFlags::arithmetic_write(*set_flags),
            Self::Cmp { .. } => IrFlags::compare_write(),
            Self::Branch { condition, .. } => IrFlags::condition_read(*condition),
            Self::ArmExtended { op } => arm_flags(*op),
            Self::ThumbExtended { op } => thumb_flags(*op),
            _ => IrFlags::default(),
        }
    }
    pub fn memory(&self) -> Option<IrMemoryEffect> {
        match self {
            Self::Load { base, byte, offset, .. } => Some(IrMemoryEffect { kind: IrMemoryKind::Read, width: if *byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word }, base: *base, address_is_dynamic: *offset != 0 }),
            Self::Store { base, byte, offset, .. } => Some(IrMemoryEffect { kind: IrMemoryKind::Write, width: if *byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word }, base: *base, address_is_dynamic: *offset != 0 }),
            Self::ArmExtended { op } => arm_memory(*op),
            Self::ThumbExtended { op } => thumb_memory(*op),
            _ => None,
        }
    }
    pub fn control(&self) -> IrControlEffect {
        match self {
            Self::Branch { target, condition, link } => IrControlEffect::Branch { target: *target, condition: *condition, link: *link },
            Self::BranchExchange { register, link } => IrControlEffect::BranchExchange { register: *register, link: *link },
            Self::Unknown { .. } | Self::ArmExtended { op: ArmExtended::SoftwareInterrupt { .. } } | Self::ThumbExtended { op: ThumbExtended::SoftwareInterrupt { .. } } => IrControlEffect::Unknown,
            _ => IrControlEffect::None,
        }
    }
    pub fn is_barrier(&self) -> bool {
        matches!(self,
            Self::Unknown { .. }
            | Self::ArmExtended { op: ArmExtended::SoftwareInterrupt { .. } | ArmExtended::Msr { .. } | ArmExtended::CoprocessorTransfer { .. } | ArmExtended::CoprocessorData { .. } | ArmExtended::CoprocessorRegisterTransfer { .. } }
            | Self::ThumbExtended { op: ThumbExtended::SoftwareInterrupt { .. } })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInstruction { pub address: u32, pub source_raw: u32, pub size: u8, pub ops: Vec<IrOp> }
impl IrInstruction {
    pub fn new(address: u32, size: u8, ops: Vec<IrOp>) -> Self { Self { address, source_raw: 0, size, ops } }
    pub fn with_source_raw(mut self, raw: u32) -> Self { self.source_raw = raw; self }
    pub fn reads(&self) -> Vec<u8> { let mut v = Vec::new(); for r in self.ops.iter().flat_map(IrOp::reads) { add_unique(&mut v, r); } v.sort_unstable(); v }
    pub fn writes(&self) -> Vec<u8> { let mut v = Vec::new(); for r in self.ops.iter().flat_map(IrOp::writes) { add_unique(&mut v, r); } v.sort_unstable(); v }
    pub fn flags(&self) -> IrFlags { self.ops.iter().fold(IrFlags::default(), |mut a, op| { let b = op.flags(); a.read_n |= b.read_n; a.read_z |= b.read_z; a.read_c |= b.read_c; a.read_v |= b.read_v; a.write_n |= b.write_n; a.write_z |= b.write_z; a.write_c |= b.write_c; a.write_v |= b.write_v; a }) }
    pub fn memory(&self) -> Option<IrMemoryEffect> { self.ops.iter().find_map(IrOp::memory) }
    pub fn control(&self) -> IrControlEffect { self.ops.iter().rev().map(IrOp::control).find(|e| !matches!(e, IrControlEffect::None)).unwrap_or(IrControlEffect::None) }
    pub fn is_barrier(&self) -> bool { self.ops.iter().any(IrOp::is_barrier) }
}

fn operand(op: Operand2) -> Value { match op { Operand2::Imm(v) => Value::Imm(v), Operand2::Reg { rm, .. } => Value::Reg(rm) } }

pub fn lower(ins: Instruction) -> IrInstruction {
    let op = match ins.kind {
        InstructionKind::Arm(ArmOp::Nop) | InstructionKind::Thumb(ThumbOp::Nop) => IrOp::Nop,
        InstructionKind::Arm(ArmOp::Mov { rd, op2 }) => IrOp::Mov { dst: rd, src: operand(op2), set_flags: ins.raw & (1 << 20) != 0 },
        InstructionKind::Arm(ArmOp::Add { rd, rn, op2 }) => IrOp::Add { dst: rd, lhs: rn, rhs: operand(op2), set_flags: ins.raw & (1 << 20) != 0 },
        InstructionKind::Arm(ArmOp::Sub { rd, rn, op2 }) => IrOp::Sub { dst: rd, lhs: rn, rhs: operand(op2), set_flags: ins.raw & (1 << 20) != 0 },
        InstructionKind::Arm(ArmOp::Cmp { rn, op2 }) => IrOp::Cmp { lhs: rn, rhs: operand(op2) },
        InstructionKind::Arm(ArmOp::Load { rd, rn, offset, byte }) => IrOp::Load { dst: rd, base: rn, offset, byte },
        InstructionKind::Arm(ArmOp::Store { rd, rn, offset, byte }) => IrOp::Store { src: rd, base: rn, offset, byte },
        InstructionKind::Arm(ArmOp::Branch { target, condition, link }) => IrOp::Branch { target, condition, link },
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link }) => IrOp::BranchExchange { register: rm, link },
        InstructionKind::Arm(ArmOp::Extended(op)) => IrOp::ArmExtended { op },
        InstructionKind::Arm(ArmOp::Unknown) => IrOp::Unknown { address: ins.address, raw: ins.raw, mode: ins.mode },
        InstructionKind::Thumb(ThumbOp::MovImm { rd, imm }) => IrOp::Mov { dst: rd, src: Value::Imm(imm as u32), set_flags: true },
        InstructionKind::Thumb(ThumbOp::AddImm { rd, rn, imm }) => IrOp::Add { dst: rd, lhs: rn, rhs: Value::Imm(imm as u32), set_flags: true },
        InstructionKind::Thumb(ThumbOp::SubImm { rd, rn, imm }) => IrOp::Sub { dst: rd, lhs: rn, rhs: Value::Imm(imm as u32), set_flags: true },
        InstructionKind::Thumb(ThumbOp::LoadImm { rd, rn, word_offset }) => IrOp::Load { dst: rd, base: rn, offset: word_offset as i32 * 4, byte: false },
        InstructionKind::Thumb(ThumbOp::StoreImm { rd, rn, word_offset }) => IrOp::Store { src: rd, base: rn, offset: word_offset as i32 * 4, byte: false },
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => IrOp::Branch { target, condition, link: false },
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => IrOp::Branch { target, condition: Condition::Al, link: true },
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm }) => IrOp::BranchExchange { register: rm, link: false },
        InstructionKind::Thumb(ThumbOp::Extended(op)) => IrOp::ThumbExtended { op },
        InstructionKind::Thumb(ThumbOp::Unknown) => IrOp::Unknown { address: ins.address, raw: ins.raw, mode: ins.mode },
    };
    IrInstruction::new(ins.address, ins.size, vec![op]).with_source_raw(ins.raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extended_instruction_keeps_identity_and_effects() {
        let instruction = lower(Instruction { address: 0x0800_0000, mode: Mode::Arm, raw: 0xE000_0090, size: 4, condition: Condition::Al, kind: InstructionKind::Arm(ArmOp::Extended(ArmExtended::Multiply { rd: 0, rn: 0, rs: 2, rm: 1, accumulate: false, set_flags: false })) });
        assert!(matches!(&instruction.ops[0], IrOp::ArmExtended { .. }));
        assert_eq!(instruction.reads(), vec![1, 2]);
        assert_eq!(instruction.writes(), vec![0]);
        assert_eq!(instruction.source_raw, 0xE000_0090);
    }
    #[test]
    fn set_flags_are_preserved_for_arm_arithmetic() {
        let instruction = lower(Instruction { address: 0x0800_0000, mode: Mode::Arm, raw: 0xE291_0001, size: 4, condition: Condition::Al, kind: InstructionKind::Arm(ArmOp::Add { rd: 0, rn: 1, op2: Operand2::Imm(1) }) });
        let f = instruction.flags();
        assert!(f.write_n && f.write_z && f.write_c && f.write_v);
    }
    #[test]
    fn swap_is_read_write_memory() {
        let instruction = IrInstruction::new(0x0800_0000, 4, vec![IrOp::ArmExtended { op: ArmExtended::Swap { rd: 0, rn: 1, rm: 2, byte: false } }]);
        assert_eq!(instruction.memory(), Some(IrMemoryEffect { kind: IrMemoryKind::ReadWrite, width: IrMemoryWidth::Word, base: 1, address_is_dynamic: true }));
    }
}
