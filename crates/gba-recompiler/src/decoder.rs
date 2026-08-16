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
    Unknown,
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

pub fn decode_arm(address: u32, raw: u32) -> Instruction {
    let condition = arm_condition(raw);
    let op = if raw == 0xE1A0_0000 {
        ArmOp::Nop
    } else if (raw & 0x0FFF_FFF0) == 0x012F_FF10 {
        ArmOp::BranchExchange { rm: (raw & 0xF) as u8, link: false }
    } else if (raw & 0x0F00_00F0) == 0x0100_0090 && (raw & 0x0200_0000) == 0 {
        ArmOp::BranchExchange { rm: (raw & 0xF) as u8, link: true }
    } else if (raw & 0x0E00_0000) == 0x0A00_0000 {
        let imm24 = raw & 0x00FF_FFFF;
        let target = address.wrapping_add(8).wrapping_add((sign_extend(imm24 << 2, 26)) as u32);
        ArmOp::Branch { target, condition, link: (raw & (1 << 24)) != 0 }
    } else if (raw & 0x0C00_0000) == 0x0400_0000 {
        let load = (raw & (1 << 20)) != 0;
        let byte = (raw & (1 << 22)) != 0;
        let up = (raw & (1 << 23)) != 0;
        let rn = ((raw >> 16) & 0xF) as u8;
        let rd = ((raw >> 12) & 0xF) as u8;
        let magnitude = (raw & 0xFFF) as i32;
        let offset = if up { magnitude } else { -magnitude };
        if load { ArmOp::Load { rd, rn, offset, byte } } else { ArmOp::Store { rd, rn, offset, byte } }
    } else if (raw & 0x0C00_0000) == 0 {
        let opcode = ((raw >> 21) & 0xF) as u8;
        let rn = ((raw >> 16) & 0xF) as u8;
        let rd = ((raw >> 12) & 0xF) as u8;
        let immediate = (raw & (1 << 25)) != 0;
        let op2 = if immediate {
            let imm8 = raw & 0xFF;
            let rotate = ((raw >> 8) & 0xF) * 2;
            Operand2::Imm(imm8.rotate_right(rotate))
        } else {
            Operand2::Reg { rm: (raw & 0xF) as u8, shift: ((raw >> 7) & 0x1F) as u8 }
        };
        match opcode {
            0xD => ArmOp::Mov { rd, op2 },
            0x4 => ArmOp::Add { rd, rn, op2 },
            0x2 => ArmOp::Sub { rd, rn, op2 },
            0xA => ArmOp::Cmp { rn, op2 },
            _ => ArmOp::Unknown,
        }
    } else {
        ArmOp::Unknown
    };
    Instruction { address, mode: Mode::Arm, raw, size: 4, condition, kind: InstructionKind::Arm(op) }
}

pub fn decode_thumb(address: u32, raw: u16) -> Instruction {
    let top = raw >> 13;
    let op = if raw == 0x46C0 {
        ThumbOp::Nop
    } else if (raw & 0xF800) == 0x2000 {
        ThumbOp::MovImm { rd: ((raw >> 8) & 7) as u8, imm: (raw & 0xFF) as u8 }
    } else if (raw & 0xF800) == 0x3000 {
        ThumbOp::AddImm { rd: ((raw >> 8) & 7) as u8, rn: ((raw >> 8) & 7) as u8, imm: (raw & 0xFF) as u8 }
    } else if (raw & 0xF800) == 0x3800 {
        ThumbOp::SubImm { rd: ((raw >> 8) & 7) as u8, rn: ((raw >> 8) & 7) as u8, imm: (raw & 0xFF) as u8 }
    } else if (raw & 0xF800) == 0x4800 {
        let rd = ((raw >> 8) & 7) as u8;
        ThumbOp::LoadImm { rd, rn: 15, word_offset: (raw & 0xFF) as u8 }
    } else if (raw & 0xF800) == 0x6000 {
        ThumbOp::StoreImm { rd: (raw & 7) as u8, rn: ((raw >> 3) & 7) as u8, word_offset: ((raw >> 6) & 0x1F) as u8 }
    } else if top == 0b11100 {
        let offset = sign_extend(((raw & 0x07FF) as u32) << 1, 12);
        ThumbOp::Branch { target: address.wrapping_add(4).wrapping_add(offset as u32), condition: Condition::Al }
    } else if (raw & 0xF000) == 0xD000 && (raw & 0x0F00) != 0x0F00 {
        let cond = match ((raw >> 8) & 0xF) as u8 {
            0 => Condition::Eq, 1 => Condition::Ne, 2 => Condition::Cs, 3 => Condition::Cc,
            4 => Condition::Mi, 5 => Condition::Pl, 6 => Condition::Vs, 7 => Condition::Vc,
            8 => Condition::Hi, 9 => Condition::Ls, 10 => Condition::Ge, 11 => Condition::Lt,
            12 => Condition::Gt, 13 => Condition::Le, _ => Condition::Al,
        };
        let offset = sign_extend(((raw & 0xFF) as u32) << 1, 9);
        ThumbOp::Branch { target: address.wrapping_add(4).wrapping_add(offset as u32), condition: cond }
    } else if (raw & 0xFF87) == 0x4700 {
        ThumbOp::BranchExchange { rm: ((raw >> 3) & 0xF) as u8 }
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
    Instruction {
        address,
        mode: Mode::Thumb,
        raw: ((first as u32) << 16) | second as u32,
        size: 4,
        condition: Condition::Al,
        kind: InstructionKind::Thumb(ThumbOp::BranchLink { target }),
    }
}

pub fn read_arm(rom: &[u8], address: u32) -> Result<u32, DecodeError> {
    let offset = address.checked_sub(ROM_BASE).ok_or(DecodeError::OutOfRange(address))? as usize;
    let bytes = rom.get(offset..offset + 4).ok_or(DecodeError::Truncated(address))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub fn read_thumb(rom: &[u8], address: u32) -> Result<u16, DecodeError> {
    let offset = address.checked_sub(ROM_BASE).ok_or(DecodeError::OutOfRange(address))? as usize;
    let bytes = rom.get(offset..offset + 2).ok_or(DecodeError::Truncated(address))?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}

pub fn read_thumb_bl(rom: &[u8], address: u32) -> Result<(u16, u16), DecodeError> {
    let first = read_thumb(rom, address)?;
    let second = read_thumb(rom, address + 2)?;
    if (first & 0xF800) != 0xF000 || (second & 0xF800) != 0xF800 {
        return Err(DecodeError::OutOfRange(address));
    }
    Ok((first, second))
}
