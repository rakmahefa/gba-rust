use thiserror::Error;

pub const ROM_BASE: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode { Arm, Thumb }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition { Al, Eq, Ne, Cs, Cc, Mi, Pl, Vs, Vc, Hi, Ls, Ge, Lt, Gt, Le }

impl Condition {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Al => "", Self::Eq => "_eq", Self::Ne => "_ne", Self::Cs => "_cs", Self::Cc => "_cc",
            Self::Mi => "_mi", Self::Pl => "_pl", Self::Vs => "_vs", Self::Vc => "_vc", Self::Hi => "_hi",
            Self::Ls => "_ls", Self::Ge => "_ge", Self::Lt => "_lt", Self::Gt => "_gt", Self::Le => "_le",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind { Branch, Call, Exchange, Return }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand2 { Imm(u32), Reg { rm: u8, shift: u8 } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmDataOp { And, Eor, Sub, Rsb, Add, Adc, Sbc, Rsc, Tst, Teq, Cmp, Cmn, Orr, Mov, Bic, Mvn }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmExtended {
    DataProcessing { op: ArmDataOp, rd: u8, rn: u8, op2: Operand2, set_flags: bool },
    Multiply { rd: u8, rn: u8, rs: u8, rm: u8, accumulate: bool, set_flags: bool },
    MultiplyLong { rd_hi: u8, rd_lo: u8, rs: u8, rm: u8, signed: bool, accumulate: bool, set_flags: bool },
    Swap { rd: u8, rn: u8, rm: u8, byte: bool },
    HalfwordTransfer { load: bool, signed: bool, halfword: bool, rd: u8, rn: u8, offset: i32, pre_index: bool, up: bool, write_back: bool },
    SingleDataTransfer { load: bool, byte: bool, rd: u8, rn: u8, offset: Operand2, pre_index: bool, up: bool, write_back: bool },
    BlockTransfer { load: bool, rn: u8, register_list: u16, pre_index: bool, up: bool, write_back: bool, user_mode: bool },
    Mrs { rd: u8, spsr: bool },
    Msr { spsr: bool, field_mask: u8, source: Operand2 },
    SoftwareInterrupt { comment: u32 },
    CoprocessorTransfer { load: bool, cp: u8, opcode1: u8, crd: u8, crn: u8, crm: u8, opcode2: u8, long: bool },
    CoprocessorData { cp: u8, opcode1: u8, crd: u8, crn: u8, crm: u8, opcode2: u8 },
    CoprocessorRegisterTransfer { to_arm: bool, cp: u8, opcode1: u8, rd: u8, crn: u8, crm: u8, opcode2: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmOp {
    Nop,
    Mov { rd: u8, op2: Operand2 },
    Add { rd: u8, rn: u8, op2: Operand2 },
    Sub { rd: u8, rn: u8, op2: Operand2 },
    Cmp { rn: u8, op2: Operand2 },
    Load { rd: u8, rn: u8, offset: i32, byte: bool },
    Store { rd: u8, rn: u8, offset: i32, byte: bool },
    Branch { target: u32, condition: Condition, link: bool },
    BranchExchange { rm: u8, link: bool },
    Extended(ArmExtended),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbAluOp { And, Eor, Lsl, Lsr, Asr, Adc, Sbc, Ror, Tst, Neg, Cmp, Cmn, Orr, Mul, Bic, Mvn }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbExtended {
    MoveShifted { kind: u8, rd: u8, rs: u8, offset: u8 },
    AddSubRegister { sub: bool, rd: u8, rs: u8, rn: u8 },
    AddSubImmediate { sub: bool, rd: u8, rs: u8, imm: u8 },
    Alu { op: ThumbAluOp, rd: u8, rs: u8 },
    HighRegister { op: u8, rd: u8, rs: u8 },
    PcRelativeLoad { rd: u8, word_offset: u8 },
    LoadStoreRegister { load: bool, byte: bool, rd: u8, rb: u8, ro: u8 },
    LoadStoreSignHalf { kind: u8, rd: u8, rb: u8, ro: u8 },
    LoadStoreImmediate { load: bool, byte: bool, rd: u8, rb: u8, offset: u8 },
    LoadStoreHalfword { load: bool, rd: u8, rb: u8, offset: u8 },
    SpRelativeLoadStore { load: bool, rd: u8, offset: u8 },
    Address { rd: u8, use_sp: bool, word_offset: u8 },
    AddSp { negative: bool, imm: u16 },
    PushPop { load: bool, registers: u8, extra_lr_pc: bool },
    MultipleLoadStore { load: bool, rb: u8, register_list: u8 },
    SoftwareInterrupt { comment: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbOp {
    Nop,
    MovImm { rd: u8, imm: u8 },
    AddImm { rd: u8, rn: u8, imm: u8 },
    SubImm { rd: u8, rn: u8, imm: u8 },
    LoadImm { rd: u8, rn: u8, word_offset: u8 },
    StoreImm { rd: u8, rn: u8, word_offset: u8 },
    Branch { target: u32, condition: Condition },
    BranchLink { target: u32 },
    BranchExchange { rm: u8 },
    Extended(ThumbExtended),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionKind { Arm(ArmOp), Thumb(ThumbOp) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub address: u32,
    pub mode: Mode,
    pub raw: u32,
    pub size: u8,
    pub condition: Condition,
    pub kind: InstructionKind,
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("address {0:#x} is outside the cartridge ROM")] OutOfRange(u32),
    #[error("truncated instruction at {0:#x}")] Truncated(u32),
}

fn sign_extend(value: u32, bits: u8) -> i32 {
    let shift = 32 - bits as u32;
    ((value << shift) as i32) >> shift
}

fn arm_condition(raw: u32) -> Condition {
    match raw >> 28 {
        0x0 => Condition::Eq, 0x1 => Condition::Ne, 0x2 => Condition::Cs, 0x3 => Condition::Cc,
        0x4 => Condition::Mi, 0x5 => Condition::Pl, 0x6 => Condition::Vs, 0x7 => Condition::Vc,
        0x8 => Condition::Hi, 0x9 => Condition::Ls, 0xA => Condition::Ge, 0xB => Condition::Lt,
        0xC => Condition::Gt, 0xD => Condition::Le, _ => Condition::Al,
    }
}

fn arm_operand2(raw: u32) -> Operand2 {
    if raw & (1 << 25) != 0 {
        let imm8 = raw & 0xFF;
        let rotate = ((raw >> 8) & 0xF) * 2;
        Operand2::Imm(imm8.rotate_right(rotate))
    } else {
        Operand2::Reg { rm: (raw & 0xF) as u8, shift: ((raw >> 7) & 0x1F) as u8 }
    }
}

pub fn decode_arm(address: u32, raw: u32) -> Instruction {
    let condition = arm_condition(raw);
    let opcode = ((raw >> 21) & 0xF) as u8;
    let rn = ((raw >> 16) & 0xF) as u8;
    let rd = ((raw >> 12) & 0xF) as u8;
    let op = if raw == 0xE1A0_0000 { ArmOp::Nop }
    else if (raw & 0x0FFF_FFF0) == 0x012F_FF30 { ArmOp::BranchExchange { rm: (raw & 0xF) as u8, link: true } }
    else if (raw & 0x0FFF_FFF0) == 0x012F_FF10 { ArmOp::BranchExchange { rm: (raw & 0xF) as u8, link: false } }
    else if (raw & 0x0F00_00F0) == 0x0100_0090 && (raw & 0x0200_0000) == 0 { ArmOp::Extended(ArmExtended::Swap { rd, rn, rm: (raw & 0xF) as u8, byte: raw & (1 << 22) != 0 }) }
    else if (raw & 0x0E00_0000) == 0x0A00_0000 {
        let imm24 = raw & 0x00FF_FFFF;
        let target = address.wrapping_add(8).wrapping_add(sign_extend(imm24 << 2, 26) as u32);
        ArmOp::Branch { target, condition, link: raw & (1 << 24) != 0 }
    } else if (raw & 0x0F00_0000) == 0x0F00_0000 {
        ArmOp::Extended(ArmExtended::SoftwareInterrupt { comment: raw & 0x00FF_FFFF })
    } else if (raw & 0x0FBF_0FFF) == 0x010F_0000 {
        ArmOp::Extended(ArmExtended::Mrs { rd, spsr: raw & (1 << 22) != 0 })
    } else if (raw & 0x0DB0_F000) == 0x0120_F000 {
        ArmOp::Extended(ArmExtended::Msr { spsr: raw & (1 << 22) != 0, field_mask: ((raw >> 16) & 0xF) as u8, source: arm_operand2(raw) })
    } else if (raw & 0x0FC0_00F0) == 0x0000_0090 {
        ArmOp::Extended(ArmExtended::Multiply { rd, rn, rs: ((raw >> 8) & 0xF) as u8, rm: (raw & 0xF) as u8, accumulate: raw & (1 << 21) != 0, set_flags: raw & (1 << 20) != 0 })
    } else if (raw & 0x0F80_00F0) == 0x0080_0090 {
        ArmOp::Extended(ArmExtended::MultiplyLong { rd_hi: ((raw >> 16) & 0xF) as u8, rd_lo: rd, rs: ((raw >> 8) & 0xF) as u8, rm: (raw & 0xF) as u8, signed: raw & (1 << 22) != 0, accumulate: raw & (1 << 21) != 0, set_flags: raw & (1 << 20) != 0 })
    } else if (raw & 0x0E00_0000) == 0x0800_0000 {
        ArmOp::Extended(ArmExtended::BlockTransfer { load: raw & (1 << 20) != 0, rn, register_list: (raw & 0xFFFF) as u16, pre_index: raw & (1 << 24) != 0, up: raw & (1 << 23) != 0, write_back: raw & (1 << 21) != 0, user_mode: raw & (1 << 22) != 0 })
    } else if (raw & 0x0E00_0000) == 0x0400_0000 {
        let load = raw & (1 << 20) != 0;
        let byte = raw & (1 << 22) != 0;
        let pre_index = raw & (1 << 24) != 0;
        let up = raw & (1 << 23) != 0;
        let write_back = raw & (1 << 21) != 0;
        let offset = arm_operand2(raw);
        if !pre_index && !write_back && matches!(offset, Operand2::Imm(_)) {
            let magnitude = match offset { Operand2::Imm(v) => v as i32, Operand2::Reg { .. } => 0 };
            let magnitude = if up { magnitude } else { -magnitude };
            if load { ArmOp::Load { rd, rn, offset: magnitude, byte } } else { ArmOp::Store { rd, rn, offset: magnitude, byte } }
        } else {
            ArmOp::Extended(ArmExtended::SingleDataTransfer { load, byte, rd, rn, offset, pre_index, up, write_back })
        }
    } else if (raw & 0x0E40_0090) == 0x0000_0090 {
        let load = raw & (1 << 20) != 0;
        let pre_index = raw & (1 << 24) != 0;
        let up = raw & (1 << 23) != 0;
        let write_back = raw & (1 << 21) != 0;
        let immediate = raw & (1 << 22) != 0;
        let signed = raw & (1 << 6) != 0;
        let halfword = !signed;
        let offset = if immediate { ((raw >> 4) & 0xF0) | (raw & 0xF) } else { raw & 0xF };
        let magnitude = if up { offset as i32 } else { -(offset as i32) };
        ArmOp::Extended(ArmExtended::HalfwordTransfer { load, signed, halfword, rd, rn, offset: magnitude, pre_index, up, write_back })
    } else if (raw & 0x0C00_0000) == 0 {
        let op2 = arm_operand2(raw);
        match opcode {
            0xD => ArmOp::Mov { rd, op2 },
            0x4 => ArmOp::Add { rd, rn, op2 },
            0x2 => ArmOp::Sub { rd, rn, op2 },
            0xA => ArmOp::Cmp { rn, op2 },
            op => ArmOp::Extended(ArmExtended::DataProcessing { op: match op { 0 => ArmDataOp::And, 1 => ArmDataOp::Eor, 2 => ArmDataOp::Sub, 3 => ArmDataOp::Rsb, 4 => ArmDataOp::Add, 5 => ArmDataOp::Adc, 6 => ArmDataOp::Sbc, 7 => ArmDataOp::Rsc, 8 => ArmDataOp::Tst, 9 => ArmDataOp::Teq, 10 => ArmDataOp::Cmp, 11 => ArmDataOp::Cmn, 12 => ArmDataOp::Orr, 13 => ArmDataOp::Mov, 14 => ArmDataOp::Bic, _ => ArmDataOp::Mvn }, rd, rn, op2, set_flags: raw & (1 << 20) != 0 }),
        }
    } else if (raw & 0x0C00_0010) == 0x0000_0010 {
        ArmOp::Extended(ArmExtended::CoprocessorRegisterTransfer { to_arm: raw & (1 << 20) != 0, cp: ((raw >> 8) & 0xF) as u8, opcode1: ((raw >> 21) & 7) as u8, rd, crn: rn, crm: (raw & 0xF) as u8, opcode2: ((raw >> 5) & 7) as u8 })
    } else if (raw & 0x0E00_0000) == 0x0C00_0000 {
        ArmOp::Extended(ArmExtended::CoprocessorTransfer { load: raw & (1 << 20) != 0, cp: ((raw >> 8) & 0xF) as u8, opcode1: ((raw >> 21) & 7) as u8, crd: rd, crn: rn, crm: (raw & 0xF) as u8, opcode2: ((raw >> 5) & 7) as u8, long: raw & (1 << 22) != 0 })
    } else if (raw & 0x0F00_0010) == 0x0E00_0010 {
        ArmOp::Extended(ArmExtended::CoprocessorData { cp: ((raw >> 8) & 0xF) as u8, opcode1: ((raw >> 20) & 0xF) as u8, crd: rd, crn: rn, crm: (raw & 0xF) as u8, opcode2: ((raw >> 5) & 7) as u8 })
    } else {
        ArmOp::Unknown
    };
    Instruction { address, mode: Mode::Arm, raw, size: 4, condition, kind: InstructionKind::Arm(op) }
}

fn thumb_condition(raw: u16) -> Condition {
    match ((raw >> 8) & 0xF) as u8 {
        0 => Condition::Eq, 1 => Condition::Ne, 2 => Condition::Cs, 3 => Condition::Cc, 4 => Condition::Mi, 5 => Condition::Pl,
        6 => Condition::Vs, 7 => Condition::Vc, 8 => Condition::Hi, 9 => Condition::Ls, 10 => Condition::Ge, 11 => Condition::Lt,
        12 => Condition::Gt, 13 => Condition::Le, _ => Condition::Al,
    }
}

pub fn decode_thumb(address: u32, raw: u16) -> Instruction {
    let top = raw >> 13;
    let rd = (raw & 7) as u8;
    let rs = ((raw >> 3) & 7) as u8;
    let op = if raw == 0x46C0 { ThumbOp::Nop }
    else if (raw & 0xE000) == 0x0000 {
        ThumbOp::Extended(ThumbExtended::MoveShifted { kind: ((raw >> 11) & 3) as u8, rd, rs, offset: ((raw >> 6) & 0x1F) as u8 })
    } else if (raw & 0xF800) == 0x1800 {
        let sub = raw & (1 << 9) != 0;
        let immediate = raw & (1 << 10) != 0;
        if immediate { ThumbOp::Extended(ThumbExtended::AddSubImmediate { sub, rd, rs, imm: ((raw >> 6) & 7) as u8 }) }
        else { ThumbOp::Extended(ThumbExtended::AddSubRegister { sub, rd, rs, rn: ((raw >> 6) & 7) as u8 }) }
    } else if (raw & 0xF800) == 0x2000 { ThumbOp::MovImm { rd: ((raw >> 8) & 7) as u8, imm: (raw & 0xFF) as u8 } }
    else if (raw & 0xF800) == 0x3000 { ThumbOp::AddImm { rd: ((raw >> 8) & 7) as u8, rn: ((raw >> 8) & 7) as u8, imm: (raw & 0xFF) as u8 } }
    else if (raw & 0xF800) == 0x3800 { ThumbOp::SubImm { rd: ((raw >> 8) & 7) as u8, rn: ((raw >> 8) & 7) as u8, imm: (raw & 0xFF) as u8 } }
    else if (raw & 0xFC00) == 0x4000 {
        let opcode = ((raw >> 6) & 0xF) as u8;
        ThumbOp::Extended(ThumbExtended::Alu { op: match opcode { 0 => ThumbAluOp::And, 1 => ThumbAluOp::Eor, 2 => ThumbAluOp::Lsl, 3 => ThumbAluOp::Lsr, 4 => ThumbAluOp::Asr, 5 => ThumbAluOp::Adc, 6 => ThumbAluOp::Sbc, 7 => ThumbAluOp::Ror, 8 => ThumbAluOp::Tst, 9 => ThumbAluOp::Neg, 10 => ThumbAluOp::Cmp, 11 => ThumbAluOp::Cmn, 12 => ThumbAluOp::Orr, 13 => ThumbAluOp::Mul, 14 => ThumbAluOp::Bic, _ => ThumbAluOp::Mvn }, rd, rs })
    } else if (raw & 0xFF87) == 0x4700 {
        ThumbOp::BranchExchange { rm: ((raw >> 3) & 0xF) as u8 }
    } else if (raw & 0xFC00) == 0x4400 {
        ThumbOp::Extended(ThumbExtended::HighRegister { op: ((raw >> 8) & 3) as u8, rd: (((raw >> 7) & 1) << 3 | (raw & 7)) as u8, rs: (((raw >> 6) & 1) << 3 | ((raw >> 3) & 7)) as u8 })
    } else if (raw & 0xF800) == 0x4800 { ThumbOp::LoadImm { rd: ((raw >> 8) & 7) as u8, rn: 15, word_offset: (raw & 0xFF) as u8 } }
    else if (raw & 0xF000) == 0x5000 {
        let opcode = ((raw >> 9) & 7) as u8;
        if opcode < 4 { ThumbOp::Extended(ThumbExtended::LoadStoreRegister { load: matches!(opcode, 3), byte: matches!(opcode, 2), rd, rb: rs, ro: ((raw >> 6) & 7) as u8 }) }
        else { ThumbOp::Extended(ThumbExtended::LoadStoreSignHalf { kind: opcode - 4, rd, rb: rs, ro: ((raw >> 6) & 7) as u8 }) }
    } else if (raw & 0xE000) == 0x6000 {
        ThumbOp::Extended(ThumbExtended::LoadStoreImmediate { load: raw & (1 << 11) != 0, byte: raw & (1 << 12) != 0, rd, rb: rs, offset: ((raw >> 6) & 0x1F) as u8 })
    } else if (raw & 0xF000) == 0x8000 {
        ThumbOp::Extended(ThumbExtended::LoadStoreHalfword { load: raw & (1 << 11) != 0, rd, rb: rs, offset: ((raw >> 6) & 0x1F) as u8 })
    } else if (raw & 0xF000) == 0x9000 {
        ThumbOp::Extended(ThumbExtended::SpRelativeLoadStore { load: raw & (1 << 11) != 0, rd: ((raw >> 8) & 7) as u8, offset: (raw & 0xFF) as u8 })
    } else if (raw & 0xF000) == 0xA000 {
        ThumbOp::Extended(ThumbExtended::Address { rd: ((raw >> 8) & 7) as u8, use_sp: raw & (1 << 11) != 0, word_offset: (raw & 0xFF) as u8 })
    } else if (raw & 0xFF80) == 0xB000 {
        ThumbOp::Extended(ThumbExtended::AddSp { negative: raw & (1 << 7) != 0, imm: ((raw & 0x7F) as u16) << 2 })
    } else if (raw & 0xF600) == 0xB400 || (raw & 0xF600) == 0xBC00 {
        ThumbOp::Extended(ThumbExtended::PushPop { load: raw & (1 << 11) != 0, registers: (raw & 0xFF) as u8, extra_lr_pc: raw & (1 << 8) != 0 })
    } else if (raw & 0xF000) == 0xC000 {
        ThumbOp::Extended(ThumbExtended::MultipleLoadStore { load: raw & (1 << 11) != 0, rb: ((raw >> 8) & 7) as u8, register_list: (raw & 0xFF) as u8 })
    } else if (raw & 0xF000) == 0xD000 && (raw & 0x0F00) != 0x0F00 {
        let cond = thumb_condition(raw);
        let offset = sign_extend(((raw & 0xFF) as u32) << 1, 9);
        ThumbOp::Branch { target: address.wrapping_add(4).wrapping_add(offset as u32), condition: cond }
    } else if (raw & 0xFF00) == 0xDF00 {
        ThumbOp::Extended(ThumbExtended::SoftwareInterrupt { comment: (raw & 0xFF) as u8 })
    } else if top == 0b11100 {
        let offset = sign_extend(((raw & 0x07FF) as u32) << 1, 12);
        ThumbOp::Branch { target: address.wrapping_add(4).wrapping_add(offset as u32), condition: Condition::Al }
    } else {
        ThumbOp::Unknown
    };
    Instruction { address, mode: Mode::Thumb, raw: raw as u32, size: 2, condition: Condition::Al, kind: InstructionKind::Thumb(op) }
}

pub fn decode_thumb_bl(address: u32, first: u16, second: u16) -> Instruction {
    let s = ((first >> 10) & 1) as u32;
    let imm10 = (first & 0x03FF) as u32;
    let j1 = ((second >> 13) & 1) as u32;
    let j2 = ((second >> 11) & 1) as u32;
    let i1 = (!(j1 ^ s)) & 1;
    let i2 = (!(j2 ^ s)) & 1;
    let imm11 = (second & 0x07FF) as u32;
    let immediate = (s << 24) | (i1 << 23) | (i2 << 22) | (imm10 << 12) | (imm11 << 1);
    let target = address.wrapping_add(4).wrapping_add(sign_extend(immediate, 25) as u32);
    Instruction { address, mode: Mode::Thumb, raw: ((first as u32) << 16) | second as u32, size: 4, condition: Condition::Al, kind: InstructionKind::Thumb(ThumbOp::BranchLink { target }) }
}

pub fn read_arm(rom: &[u8], address: u32) -> Result<u32, DecodeError> {
    let offset = address.checked_sub(ROM_BASE).ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset + 4 > rom.len() { return Err(DecodeError::Truncated(address)); }
    Ok(u32::from_le_bytes(rom[offset..offset + 4].try_into().unwrap()))
}

pub fn read_thumb(rom: &[u8], address: u32) -> Result<u16, DecodeError> {
    let offset = address.checked_sub(ROM_BASE).ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset + 2 > rom.len() { return Err(DecodeError::Truncated(address)); }
    Ok(u16::from_le_bytes(rom[offset..offset + 2].try_into().unwrap()))
}

pub fn read_thumb_bl(rom: &[u8], address: u32) -> Result<(u16, u16), DecodeError> {
    let offset = address.checked_sub(ROM_BASE).ok_or(DecodeError::OutOfRange(address))? as usize;
    if offset + 4 > rom.len() { return Err(DecodeError::Truncated(address)); }
    Ok((u16::from_le_bytes(rom[offset..offset + 2].try_into().unwrap()), u16::from_le_bytes(rom[offset + 2..offset + 4].try_into().unwrap())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_arm_data_processing_families() {
        for raw in [0xE0000000,0xE0200000,0xE0400000,0xE0600000,0xE0800000,0xE0A00000,0xE0C00000,0xE0E00000,0xE1100000,0xE1300000,0xE1500000,0xE1700000,0xE1800000,0xE1A00000,0xE1C00000,0xE1E00000] {
            assert!(!matches!(decode_arm(ROM_BASE, raw).kind, InstructionKind::Arm(ArmOp::Unknown)), "raw={raw:#010x}");
        }
    }

    #[test]
    fn decodes_arm_system_and_memory_families() {
        for raw in [0xE0000090,0xE0800090,0xE1000090,0xE1400090,0xE4000000,0xE4800000,0xE8000000,0xEF000000,0xEC000000,0xEE000000] {
            assert!(!matches!(decode_arm(ROM_BASE, raw).kind, InstructionKind::Arm(ArmOp::Unknown)), "raw={raw:#010x}");
        }
    }

    #[test]
    fn decodes_thumb_major_families() {
        for raw in [0x0000,0x1800,0x2000,0x3000,0x4000,0x4400,0x4700,0x4800,0x5000,0x6000,0x8000,0x9000,0xA000,0xB000,0xB400,0xC000,0xD000,0xDF00,0xE000] {
            assert!(!matches!(decode_thumb(ROM_BASE, raw).kind, InstructionKind::Thumb(ThumbOp::Unknown)), "raw={raw:#06x}");
        }
    }

    #[test]
    fn decodes_thumb_bx_as_control_flow() {
        assert_eq!(decode_thumb(ROM_BASE, 0x4700).kind, InstructionKind::Thumb(ThumbOp::BranchExchange { rm: 8 }));
    }

    #[test]
    fn decodes_arm_blx_and_bx() {
        assert_eq!(decode_arm(ROM_BASE, 0xE12F_FF31).kind, InstructionKind::Arm(ArmOp::BranchExchange { rm: 1, link: true }));
        assert_eq!(decode_arm(ROM_BASE, 0xE12F_FF11).kind, InstructionKind::Arm(ArmOp::BranchExchange { rm: 1, link: false }));
    }

    #[test]
    fn decodes_thumb_bl() {
        let instruction = decode_thumb_bl(ROM_BASE, 0xF000, 0xF800);
        assert_eq!(instruction.size, 4);
        assert!(matches!(instruction.kind, InstructionKind::Thumb(ThumbOp::BranchLink { .. })));
    }
}
