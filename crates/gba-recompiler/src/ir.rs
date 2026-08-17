use crate::decoder::{
    ArmDataOp, ArmExtended, ArmOp, Condition, Instruction, InstructionKind, Mode, Operand2,
    ThumbAluOp, ThumbExtended, ThumbOp,
};

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
pub enum IrMemoryWidth {
    Byte,
    Halfword,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrMemoryKind {
    Read,
    Write,
    ReadWrite,
}

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
            Condition::Gt | Condition::Le => Self {
                read_n: true,
                read_v: true,
                read_z: true,
                ..Self::default()
            },
        }
    }

    pub fn arithmetic_write(set_flags: bool) -> Self {
        if set_flags {
            Self {
                write_n: true,
                write_z: true,
                write_c: true,
                write_v: true,
                ..Self::default()
            }
        } else {
            Self::default()
        }
    }

    pub fn logical_write(set_flags: bool) -> Self {
        if set_flags {
            Self {
                write_n: true,
                write_z: true,
                // The shifter may update C; keep this conservative until its carry-out is modeled.
                write_c: true,
                ..Self::default()
            }
        } else {
            Self::default()
        }
    }

    pub fn compare_write() -> Self {
        Self {
            write_n: true,
            write_z: true,
            write_c: true,
            write_v: true,
            ..Self::default()
        }
    }

    pub fn shift_write() -> Self {
        Self {
            write_n: true,
            write_z: true,
            write_c: true,
            ..Self::default()
        }
    }

    pub fn reads_any(&self) -> bool {
        self.read_n || self.read_z || self.read_c || self.read_v
    }

    pub fn writes_any(&self) -> bool {
        self.write_n || self.write_z || self.write_c || self.write_v
    }
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

fn push_unique(reads: &mut Vec<u8>, reg: u8) {
    if !reads.contains(&reg) {
        reads.push(reg);
    }
}

fn operand_reads(reads: &mut Vec<u8>, operand: Operand2) {
    if let Operand2::Reg { rm, .. } = operand {
        push_unique(reads, rm);
    }
}

fn arm_extended_reads(op: ArmExtended) -> Vec<u8> {
    let mut reads = Vec::new();
    match op {
        ArmExtended::DataProcessing { op, rn, op2, .. } => {
            if !matches!(op, ArmDataOp::Mov | ArmDataOp::Mvn) {
                push_unique(&mut reads, rn);
            }
            operand_reads(&mut reads, op2);
            if matches!(op, ArmDataOp::Adc | ArmDataOp::Sbc | ArmDataOp::Rsc) {
                // C is represented in flags(), not as a GPR read.
            }
        }
        ArmExtended::Multiply { rn, rs, rm, accumulate, .. } => {
            push_unique(&mut reads, rm);
            push_unique(&mut reads, rs);
            if accumulate {
                push_unique(&mut reads, rn);
            }
        }
        ArmExtended::MultiplyLong { rd_hi, rd_lo, rs, rm, accumulate, .. } => {
            push_unique(&mut reads, rm);
            push_unique(&mut reads, rs);
            if accumulate {
                push_unique(&mut reads, rd_lo);
                push_unique(&mut reads, rd_hi);
            }
        }
        ArmExtended::Swap { rn, rm, .. } => {
            push_unique(&mut reads, rn);
            push_unique(&mut reads, rm);
        }
        ArmExtended::HalfwordTransfer { rn, .. } => push_unique(&mut reads, rn),
        ArmExtended::SingleDataTransfer { rn, offset, .. } => {
            push_unique(&mut reads, rn);
            operand_reads(&mut reads, offset);
        }
        ArmExtended::BlockTransfer { rn, register_list, load, .. } => {
            push_unique(&mut reads, rn);
            if !load {
                for reg in 0..16 {
                    if register_list & (1 << reg) != 0 {
                        push_unique(&mut reads, reg as u8);
                    }
                }
            }
        }
        ArmExtended::Mrs { .. } => {}
        ArmExtended::Msr { source, .. } => operand_reads(&mut reads, source),
        ArmExtended::SoftwareInterrupt { .. } => {}
        ArmExtended::CoprocessorTransfer { crd, crn, .. } => {
            push_unique(&mut reads, crn);
            push_unique(&mut reads, crd);
        }
        ArmExtended::CoprocessorData { crn, crd, .. } => {
            push_unique(&mut reads, crn);
            push_unique(&mut reads, crd);
        }
        ArmExtended::CoprocessorRegisterTransfer { rd, crn, .. } => {
            push_unique(&mut reads, rd);
            push_unique(&mut reads, crn);
        }
    }
    reads
}

fn arm_extended_writes(op: ArmExtended) -> Vec<u8> {
    let mut writes = Vec::new();
    match op {
        ArmExtended::DataProcessing { op, rd, .. } => {
            if !matches!(op, ArmDataOp::Tst | ArmDataOp::Teq | ArmDataOp::Cmp | ArmDataOp::Cmn) {
                writes.push(rd);
            }
        }
        ArmExtended::Multiply { rd, .. } => writes.push(rd),
        ArmExtended::MultiplyLong { rd_hi, rd_lo, .. } => {
            writes.push(rd_lo);
            if rd_hi != rd_lo {
                writes.push(rd_hi);
            }
        }
        ArmExtended::Swap { rd, .. } => writes.push(rd),
        ArmExtended::HalfwordTransfer { load, rd, rn, write_back, .. } => {
            if load {
                writes.push(rd);
            }
            if write_back && rn != rd {
                writes.push(rn);
            }
        }
        ArmExtended::SingleDataTransfer { load, rd, rn, write_back, .. } => {
            if load {
                writes.push(rd);
            }
            if write_back && rn != rd {
                writes.push(rn);
            }
        }
        ArmExtended::BlockTransfer { load, rn, register_list, write_back, .. } => {
            if load {
                for reg in 0..16 {
                    if register_list & (1 << reg) != 0 {
                        writes.push(reg as u8);
                    }
                }
            }
            if write_back && !writes.contains(&rn) {
                writes.push(rn);
            }
        }
        ArmExtended::Mrs { rd, .. } => writes.push(rd),
        ArmExtended::Msr { .. } | ArmExtended::SoftwareInterrupt { .. } => {}
        ArmExtended::CoprocessorTransfer { load, crd, .. }
        | ArmExtended::CoprocessorRegisterTransfer { to_arm: load, rd: crd, .. } => {
            if load {
                writes.push(crd);
            }
        }
        ArmExtended::CoprocessorData { .. } => {}
    }
    writes
}

fn arm_extended_flags(op: ArmExtended) -> IrFlags {
    match op {
        ArmExtended::DataProcessing { op, set_flags, .. } => match op {
            ArmDataOp::Tst | ArmDataOp::Teq | ArmDataOp::Cmp | ArmDataOp::Cmn => IrFlags::compare_write(),
            ArmDataOp::And | ArmDataOp::Eor | ArmDataOp::Orr | ArmDataOp::Bic | ArmDataOp::Mov | ArmDataOp::Mvn => {
                IrFlags::logical_write(set_flags)
            }
            ArmDataOp::Sub | ArmDataOp::Rsb | ArmDataOp::Add | ArmDataOp::Adc | ArmDataOp::Sbc | ArmDataOp::Rsc => {
                let mut flags = IrFlags::arithmetic_write(set_flags);
                if matches!(op, ArmDataOp::Adc | ArmDataOp::Sbc | ArmDataOp::Rsc) {
                    flags.read_c = true;
                }
                flags
            }
        },
        ArmExtended::Multiply { set_flags, .. } | ArmExtended::MultiplyLong { set_flags, .. } => {
            if set_flags {
                IrFlags { write_n: true, write_z: true, ..IrFlags::default() }
            } else {
                IrFlags::default()
            }
        }
        ArmExtended::Mrs { .. }
        | ArmExtended::Msr { .. }
        | ArmExtended::Swap { .. }
        | ArmExtended::HalfwordTransfer { .. }
        | ArmExtended::SingleDataTransfer { .. }
        | ArmExtended::BlockTransfer { .. }
        | ArmExtended::SoftwareInterrupt { .. }
        | ArmExtended::CoprocessorTransfer { .. }
        | ArmExtended::CoprocessorData { .. }
        | ArmExtended::CoprocessorRegisterTransfer { .. } => IrFlags::default(),
    }
}

fn thumb_extended_reads(op: ThumbExtended) -> Vec<u8> {
    let mut reads = Vec::new();
    match op {
        ThumbExtended::MoveShifted { rs, .. } => push_unique(&mut reads, rs),
        ThumbExtended::AddSubRegister { rd, rs, rn, .. } => {
            push_unique(&mut reads, rs);
            push_unique(&mut reads, rn);
            push_unique(&mut reads, rd);
        }
        ThumbExtended::AddSubImmediate { rd, rs, .. } => {
            push_unique(&mut reads, rs);
            push_unique(&mut reads, rd);
        }
        ThumbExtended::Alu { op, rd, rs } => {
            push_unique(&mut reads, rs);
            if !matches!(op, ThumbAluOp::Tst | ThumbAluOp::Cmp | ThumbAluOp::Cmn) {
                push_unique(&mut reads, rd);
            } else {
                push_unique(&mut reads, rd);
            }
            if matches!(op, ThumbAluOp::Adc | ThumbAluOp::Sbc | ThumbAluOp::Ror) {
                // C is represented in flags(), not as a GPR read.
            }
        }
        ThumbExtended::HighRegister { op, rd, rs } => {
            push_unique(&mut reads, rs);
            if op == 0 || op == 1 {
                push_unique(&mut reads, rd);
            }
        }
        ThumbExtended::PcRelativeLoad { .. } => push_unique(&mut reads, 15),
        ThumbExtended::LoadStoreRegister { load, rd, rb, ro, .. } => {
            push_unique(&mut reads, rb);
            push_unique(&mut reads, ro);
            if !load {
                push_unique(&mut reads, rd);
            }
        }
        ThumbExtended::LoadStoreSignHalf { kind, rd, rb, ro } => {
            push_unique(&mut reads, rb);
            push_unique(&mut reads, ro);
            if kind == 0 {
                push_unique(&mut reads, rd);
            }
        }
        ThumbExtended::LoadStoreImmediate { load, rd, rb, .. } => {
            push_unique(&mut reads, rb);
            if !load {
                push_unique(&mut reads, rd);
            }
        }
        ThumbExtended::LoadStoreHalfword { load, rd, rb, .. } => {
            push_unique(&mut reads, rb);
            if !load {
                push_unique(&mut reads, rd);
            }
        }
        ThumbExtended::SpRelativeLoadStore { load, rd, .. } => {
            push_unique(&mut reads, 13);
            if !load {
                push_unique(&mut reads, rd);
            }
        }
        ThumbExtended::Address { use_sp, .. } => push_unique(&mut reads, if use_sp { 13 } else { 15 }),
        ThumbExtended::AddSp { .. } => push_unique(&mut reads, 13),
        ThumbExtended::PushPop { load, registers, extra_lr_pc } => {
            push_unique(&mut reads, 13);
            if !load {
                for reg in 0..8 {
                    if registers & (1 << reg) != 0 {
                        push_unique(&mut reads, reg as u8);
                    }
                }
                if extra_lr_pc {
                    push_unique(&mut reads, 14);
                }
            }
        }
        ThumbExtended::MultipleLoadStore { load, rb, register_list } => {
            push_unique(&mut reads, rb);
            if !load {
                for reg in 0..8 {
                    if register_list & (1 << reg) != 0 {
                        push_unique(&mut reads, reg as u8);
                    }
                }
            }
        }
        ThumbExtended::SoftwareInterrupt { .. } => {}
    }
    reads
}

fn thumb_extended_writes(op: ThumbExtended) -> Vec<u8> {
    let mut writes = Vec::new();
    match op {
        ThumbExtended::MoveShifted { rd, .. }
        | ThumbExtended::AddSubRegister { rd, .. }
        | ThumbExtended::AddSubImmediate { rd, .. } => writes.push(rd),
        ThumbExtended::Alu { op, rd, .. } => {
            if !matches!(op, ThumbAluOp::Tst | ThumbAluOp::Cmp | ThumbAluOp::Cmn) {
                writes.push(rd);
            }
        }
        ThumbExtended::HighRegister { op, rd, .. } => {
            if op == 0 || op == 2 {
                writes.push(rd);
            }
        }
        ThumbExtended::PcRelativeLoad { rd, .. }
        | ThumbExtended::LoadStoreImmediate { load: true, rd, .. }
        | ThumbExtended::LoadStoreHalfword { load: true, rd, .. }
        | ThumbExtended::SpRelativeLoadStore { load: true, rd, .. } => writes.push(rd),
        ThumbExtended::LoadStoreRegister { load: true, rd, .. }
        | ThumbExtended::LoadStoreSignHalf { kind: 1..=3, rd, .. } => writes.push(rd),
        ThumbExtended::Address { rd, .. } => writes.push(rd),
        ThumbExtended::AddSp { .. } => writes.push(13),
        ThumbExtended::PushPop { load, registers, extra_lr_pc } => {
            if load {
                for reg in 0..8 {
                    if registers & (1 << reg) != 0 {
                        writes.push(reg as u8);
                    }
                }
                if extra_lr_pc {
                    writes.push(15);
                }
            }
            writes.push(13);
        }
        ThumbExtended::MultipleLoadStore { load, rb, register_list } => {
            if load {
                for reg in 0..8 {
                    if register_list & (1 << reg) != 0 {
                        writes.push(reg as u8);
                    }
                }
            }
            writes.push(rb);
        }
        ThumbExtended::LoadStoreRegister { load: false, .. }
        | ThumbExtended::LoadStoreSignHalf { kind: 0, .. }
        | ThumbExtended::LoadStoreImmediate { load: false, .. }
        | ThumbExtended::LoadStoreHalfword { load: false, .. }
        | ThumbExtended::SpRelativeLoadStore { load: false, .. }
        | ThumbExtended::SoftwareInterrupt { .. } => {}
    }
    writes
}

fn thumb_extended_flags(op: ThumbExtended) -> IrFlags {
    match op {
        ThumbExtended::MoveShifted { .. }
        | ThumbExtended::AddSubRegister { .. }
        | ThumbExtended::AddSubImmediate { .. } => IrFlags::arithmetic_write(true),
        ThumbExtended::Alu { op, .. } => match op {
            ThumbAluOp::Tst | ThumbAluOp::Cmp | ThumbAluOp::Cmn => IrFlags::compare_write(),
            ThumbAluOp::And
            | ThumbAluOp::Eor
            | ThumbAluOp::Lsl
            | ThumbAluOp::Lsr
            | ThumbAluOp::Asr
            | ThumbAluOp::Adc
            | ThumbAluOp::Sbc
            | ThumbAluOp::Ror
            | ThumbAluOp::Neg
            | ThumbAluOp::Orr
            | ThumbAluOp::Mul
            | ThumbAluOp::Bic
            | ThumbAluOp::Mvn => {
                let mut flags = if matches!(op, ThumbAluOp::Adc | ThumbAluOp::Sbc | ThumbAluOp::Ror) {
                    IrFlags { read_c: true, ..IrFlags::default() }
                } else {
                    IrFlags::default()
                };
                if matches!(op, ThumbAluOp::Lsl | ThumbAluOp::Lsr | ThumbAluOp::Asr | ThumbAluOp::Ror) {
                    flags = IrFlags::shift_write();
                    if matches!(op, ThumbAluOp::Ror) {
                        flags.read_c = true;
                    }
                } else {
                    flags.write_n = true;
                    flags.write_z = true;
                    flags.write_c = true;
                    if matches!(op, ThumbAluOp::Adc | ThumbAluOp::Sbc | ThumbAluOp::Neg | ThumbAluOp::Mul) {
                        flags.write_v = matches!(op, ThumbAluOp::Adc | ThumbAluOp::Sbc | ThumbAluOp::Neg);
                    }
                }
                flags
            }
        },
        ThumbExtended::HighRegister { op, .. } => match op {
            1 => IrFlags::compare_write(),
            _ => IrFlags::default(),
        },
        ThumbExtended::SoftwareInterrupt { .. }
        | ThumbExtended::PcRelativeLoad { .. }
        | ThumbExtended::LoadStoreRegister { .. }
        | ThumbExtended::LoadStoreSignHalf { .. }
        | ThumbExtended::LoadStoreImmediate { .. }
        | ThumbExtended::LoadStoreHalfword { .. }
        | ThumbExtended::SpRelativeLoadStore { .. }
        | ThumbExtended::Address { .. }
        | ThumbExtended::AddSp { .. }
        | ThumbExtended::PushPop { .. }
        | ThumbExtended::MultipleLoadStore { .. } => IrFlags::default(),
    }
}

impl IrOp {
    pub fn reads(&self) -> Vec<u8> {
        let mut reads = match self {
            Self::Nop | Self::Unknown { .. } => Vec::new(),
            Self::Mov { src, .. } => src.register().into_iter().collect(),
            Self::Add { lhs, rhs, .. } | Self::Sub { lhs, rhs, .. } | Self::Cmp { lhs, rhs } => {
                let mut reads = vec![*lhs];
                if let Some(register) = rhs.register() {
                    reads.push(register);
                }
                reads
            }
            Self::Load { base, .. } => vec![*base],
            Self::Store { src, base, .. } => vec![*src, *base],
            Self::Branch { .. } => Vec::new(),
            Self::BranchExchange { register, .. } => vec![*register],
            Self::ArmExtended { op } => arm_extended_reads(*op),
            Self::ThumbExtended { op } => thumb_extended_reads(*op),
        };
        reads.sort_unstable();
        reads.dedup();
        reads
    }

    pub fn writes(&self) -> Vec<u8> {
        let mut writes = match self {
            Self::Mov { dst, .. }
            | Self::Add { dst, .. }
            | Self::Sub { dst, .. }
            | Self::Load { dst, .. } => vec![*dst],
            Self::Branch { link: true, .. } | Self::BranchExchange { link: true, .. } => vec![14],
            Self::ArmExtended { op } => arm_extended_writes(*op),
            Self::ThumbExtended { op } => thumb_extended_writes(*op),
            Self::Nop
            | Self::Cmp { .. }
            | Self::Store { .. }
            | Self::Branch { link: false, .. }
            | Self::BranchExchange { link: false, .. }
            | Self::Unknown { .. } => Vec::new(),
        };
        writes.sort_unstable();
        writes.dedup();
        writes
    }

    pub fn flags(&self) -> IrFlags {
        match self {
            Self::Mov { set_flags, .. } => IrFlags::logical_write(*set_flags),
            Self::Add { set_flags, .. } | Self::Sub { set_flags, .. } => IrFlags::arithmetic_write(*set_flags),
            Self::Cmp { .. } => IrFlags::compare_write(),
            Self::Branch { condition, .. } => IrFlags::condition_read(*condition),
            Self::ArmExtended { op } => arm_extended_flags(*op),
            Self::ThumbExtended { op } => thumb_extended_flags(*op),
            Self::Nop
            | Self::Load { .. }
            | Self::Store { .. }
            | Self::BranchExchange { .. }
            | Self::Unknown { .. } => IrFlags::default(),
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
            Self::ArmExtended { op } => arm_extended_memory(*op),
            Self::ThumbExtended { op } => thumb_extended_memory(*op),
            _ => None,
        }
    }

    pub fn control(&self) -> IrControlEffect {
        match self {
            Self::Branch { target, condition, link } => IrControlEffect::Branch {
                target: *target,
                condition: *condition,
                link: *link,
            },
            Self::BranchExchange { register, link } => IrControlEffect::BranchExchange {
                register: *register,
                link: *link,
            },
            Self::Unknown { .. } | Self::ArmExtended { op: ArmExtended::SoftwareInterrupt { .. } }
            | Self::ThumbExtended { op: ThumbExtended::SoftwareInterrupt { .. } } => IrControlEffect::Unknown,
            _ => IrControlEffect::None,
        }
    }

    pub fn is_barrier(&self) -> bool {
        matches!(
            self,
            Self::Unknown { .. }
                | Self::ArmExtended { op: ArmExtended::SoftwareInterrupt { .. } }
                | Self::ArmExtended { op: ArmExtended::Msr { .. } }
                | Self::ThumbExtended { op: ThumbExtended::SoftwareInterrupt { .. } }
                | Self::ArmExtended { op: ArmExtended::CoprocessorTransfer { .. } }
                | Self::ArmExtended { op: ArmExtended::CoprocessorData { .. } }
                | Self::ArmExtended { op: ArmExtended::CoprocessorRegisterTransfer { .. } }
        )
    }
}

fn arm_extended_memory(op: ArmExtended) -> Option<IrMemoryEffect> {
    match op {
        ArmExtended::Swap { rn, byte, .. } => Some(IrMemoryEffect {
            kind: IrMemoryKind::ReadWrite,
            width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word },
            base: rn,
            address_is_dynamic: true,
        }),
        ArmExtended::HalfwordTransfer { rn, halfword, .. } => Some(IrMemoryEffect {
            kind: if matches!(op, ArmExtended::HalfwordTransfer { load: true, .. }) {
                IrMemoryKind::Read
            } else {
                IrMemoryKind::Write
            },
            width: if halfword { IrMemoryWidth::Halfword } else { IrMemoryWidth::Byte },
            base: rn,
            address_is_dynamic: true,
        }),
        ArmExtended::SingleDataTransfer { rn, load, byte, offset, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word },
            base: rn,
            address_is_dynamic: matches!(offset, Operand2::Reg { .. }),
        }),
        ArmExtended::BlockTransfer { rn, load, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: IrMemoryWidth::Word,
            base: rn,
            address_is_dynamic: true,
        }),
        ArmExtended::CoprocessorTransfer { rn: _, load, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: IrMemoryWidth::Word,
            base: 0,
            address_is_dynamic: true,
        }),
        _ => None,
    }
}

fn thumb_extended_memory(op: ThumbExtended) -> Option<IrMemoryEffect> {
    match op {
        ThumbExtended::PcRelativeLoad { .. } => Some(IrMemoryEffect {
            kind: IrMemoryKind::Read,
            width: IrMemoryWidth::Word,
            base: 15,
            address_is_dynamic: true,
        }),
        ThumbExtended::LoadStoreRegister { load, byte, rb, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word },
            base: rb,
            address_is_dynamic: true,
        }),
        ThumbExtended::LoadStoreSignHalf { kind, rb, .. } => Some(IrMemoryEffect {
            kind: if kind == 0 { IrMemoryKind::Write } else { IrMemoryKind::Read },
            width: if kind == 1 { IrMemoryWidth::Byte } else { IrMemoryWidth::Halfword },
            base: rb,
            address_is_dynamic: true,
        }),
        ThumbExtended::LoadStoreImmediate { load, byte, rb, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: if byte { IrMemoryWidth::Byte } else { IrMemoryWidth::Word },
            base: rb,
            address_is_dynamic: true,
        }),
        ThumbExtended::LoadStoreHalfword { load, rb, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: IrMemoryWidth::Halfword,
            base: rb,
            address_is_dynamic: true,
        }),
        ThumbExtended::SpRelativeLoadStore { load, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: IrMemoryWidth::Word,
            base: 13,
            address_is_dynamic: true,
        }),
        ThumbExtended::PushPop { load, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: IrMemoryWidth::Word,
            base: 13,
            address_is_dynamic: true,
        }),
        ThumbExtended::MultipleLoadStore { load, rb, .. } => Some(IrMemoryEffect {
            kind: if load { IrMemoryKind::Read } else { IrMemoryKind::Write },
            width: IrMemoryWidth::Word,
            base: rb,
            address_is_dynamic: true,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInstruction {
    pub address: u32,
    pub size: u8,
    pub ops: Vec<IrOp>,
}

impl IrInstruction {
    pub fn new(address: u32, size: u8, ops: Vec<IrOp>) -> Self {
        Self { address, size, ops }
    }

    pub fn reads(&self) -> Vec<u8> {
        self.ops.iter().flat_map(IrOp::reads).fold(Vec::new(), |mut reads, op_reads| {
            for reg in op_reads {
                push_unique(&mut reads, reg);
            }
            reads.sort_unstable();
            reads
        })
    }

    pub fn writes(&self) -> Vec<u8> {
        self.ops.iter().flat_map(IrOp::writes).fold(Vec::new(), |mut writes, op_writes| {
            for reg in op_writes {
                push_unique(&mut writes, reg);
            }
            writes.sort_unstable();
            writes
        })
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

    pub fn memory(&self) -> Option<IrMemoryEffect> {
        self.ops.iter().find_map(IrOp::memory)
    }

    pub fn control(&self) -> IrControlEffect {
        self.ops
            .iter()
            .rev()
            .map(IrOp::control)
            .find(|effect| !matches!(effect, IrControlEffect::None))
            .unwrap_or(IrControlEffect::None)
    }

    pub fn is_barrier(&self) -> bool {
        self.ops.iter().any(IrOp::is_barrier)
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
        InstructionKind::Arm(ArmOp::Mov { rd, op2 }) => vec![IrOp::Mov {
            dst: rd,
            src: operand(op2),
            set_flags: ins.raw & (1 << 20) != 0,
        }],
        InstructionKind::Arm(ArmOp::Add { rd, rn, op2 }) => vec![IrOp::Add {
            dst: rd,
            lhs: rn,
            rhs: operand(op2),
            set_flags: ins.raw & (1 << 20) != 0,
        }],
        InstructionKind::Arm(ArmOp::Sub { rd, rn, op2 }) => vec![IrOp::Sub {
            dst: rd,
            lhs: rn,
            rhs: operand(op2),
            set_flags: ins.raw & (1 << 20) != 0,
        }],
        InstructionKind::Arm(ArmOp::Cmp { rn, op2 }) => vec![IrOp::Cmp { lhs: rn, rhs: operand(op2) }],
        InstructionKind::Arm(ArmOp::Load { rd, rn, offset, byte }) => vec![IrOp::Load {
            dst: rd,
            base: rn,
            offset,
            byte,
        }],
        InstructionKind::Arm(ArmOp::Store { rd, rn, offset, byte }) => vec![IrOp::Store {
            src: rd,
            base: rn,
            offset,
            byte,
        }],
        InstructionKind::Arm(ArmOp::Branch { target, condition, link }) => vec![IrOp::Branch {
            target,
            condition,
            link,
        }],
        InstructionKind::Arm(ArmOp::BranchExchange { rm, link }) => vec![IrOp::BranchExchange {
            register: rm,
            link,
        }],
        InstructionKind::Arm(ArmOp::Extended(ext)) => vec![IrOp::ArmExtended { op: ext }],
        InstructionKind::Thumb(ThumbOp::MovImm { rd, imm }) => vec![IrOp::Mov {
            dst: rd,
            src: Value::Imm(imm as u32),
            set_flags: true,
        }],
        InstructionKind::Thumb(ThumbOp::AddImm { rd, rn, imm }) => vec![IrOp::Add {
            dst: rd,
            lhs: rn,
            rhs: Value::Imm(imm as u32),
            set_flags: true,
        }],
        InstructionKind::Thumb(ThumbOp::SubImm { rd, rn, imm }) => vec![IrOp::Sub {
            dst: rd,
            lhs: rn,
            rhs: Value::Imm(imm as u32),
            set_flags: true,
        }],
        InstructionKind::Thumb(ThumbOp::LoadImm { rd, rn, word_offset }) => vec![IrOp::Load {
            dst: rd,
            base: rn,
            offset: (word_offset as i32) * 4,
            byte: false,
        }],
        InstructionKind::Thumb(ThumbOp::StoreImm { rd, rn, word_offset }) => vec![IrOp::Store {
            src: rd,
            base: rn,
            offset: (word_offset as i32) * 4,
            byte: false,
        }],
        InstructionKind::Thumb(ThumbOp::Branch { target, condition }) => vec![IrOp::Branch {
            target,
            condition,
            link: false,
        }],
        InstructionKind::Thumb(ThumbOp::BranchLink { target }) => vec![IrOp::Branch {
            target,
            condition: Condition::Al,
            link: true,
        }],
        InstructionKind::Thumb(ThumbOp::BranchExchange { rm }) => vec![IrOp::BranchExchange {
            register: rm,
            link: false,
        }],
        InstructionKind::Thumb(ThumbOp::Extended(ext)) => vec![IrOp::ThumbExtended { op: ext }],
        InstructionKind::Arm(ArmOp::Unknown) | InstructionKind::Thumb(ThumbOp::Unknown) => vec![IrOp::Unknown {
            address: ins.address,
            raw: ins.raw,
            mode: ins.mode,
        }],
    };
    IrInstruction::new(ins.address, ins.size, op)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_effects_are_derived_once() {
        let instruction = IrInstruction::new(
            0x0800_0000,
            4,
            vec![IrOp::Add { dst: 0, lhs: 1, rhs: Value::Reg(2), set_flags: false }],
        );
        assert_eq!(instruction.reads(), vec![1, 2]);
        assert_eq!(instruction.writes(), vec![0]);
        assert_eq!(instruction.memory(), None);
        assert_eq!(instruction.control(), IrControlEffect::None);
    }

    #[test]
    fn extended_arm_instruction_is_not_downgraded_to_unknown() {
        let instruction = lower(Instruction {
            address: 0x0800_0000,
            mode: Mode::Arm,
            raw: 0xE000_0090,
            size: 4,
            condition: Condition::Al,
            kind: InstructionKind::Arm(ArmOp::Extended(ArmExtended::Multiply {
                rd: 0,
                rn: 0,
                rs: 2,
                rm: 1,
                accumulate: false,
                set_flags: false,
            })),
        });
        assert!(matches!(instruction.ops.as_slice(), [IrOp::ArmExtended { .. }]));
        assert_eq!(instruction.reads(), vec![1, 2]);
        assert_eq!(instruction.writes(), vec![0]);
    }

    #[test]
    fn arm_set_flags_survive_lowering() {
        let instruction = lower(Instruction {
            address: 0x0800_0000,
            mode: Mode::Arm,
            raw: 0xE291_0001,
            size: 4,
            condition: Condition::Al,
            kind: InstructionKind::Arm(ArmOp::Add {
                rd: 0,
                rn: 1,
                op2: Operand2::Imm(1),
            }),
        });
        assert!(instruction.flags().write_n);
        assert!(instruction.flags().write_z);
        assert!(instruction.flags().write_c);
        assert!(instruction.flags().write_v);
    }

    #[test]
    fn memory_effect_supports_halfword_and_read_write() {
        let instruction = IrInstruction::new(
            0x0800_0000,
            4,
            vec![IrOp::ArmExtended {
                op: ArmExtended::Swap { rd: 0, rn: 1, rm: 2, byte: false },
            }],
        );
        assert_eq!(
            instruction.memory(),
            Some(IrMemoryEffect {
                kind: IrMemoryKind::ReadWrite,
                width: IrMemoryWidth::Word,
                base: 1,
                address_is_dynamic: true,
            })
        );
    }
}
