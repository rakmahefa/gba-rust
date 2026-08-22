use crate::address_space::ImageMapping;
use crate::decoder::{
    ArmDataOp, ArmExtended, ArmOp, Instruction, InstructionKind, Mode, Operand2, ThumbExtended,
    ThumbOp,
};

use super::model::BlockKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum AbstractValue {
    #[default]
    Unknown,
    Constant(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct AbstractState {
    regs: [AbstractValue; 16],
}

impl AbstractState {
    pub(super) fn read(self, register: u8) -> AbstractValue {
        self.regs[register as usize]
    }

    pub(super) fn write(&mut self, register: u8, value: AbstractValue) {
        self.regs[register as usize] = value;
    }

    pub(super) fn join(self, other: Self) -> Self {
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

impl AbstractValue {
    fn map_not(self) -> Self {
        match self {
            Self::Constant(value) => Self::Constant(!value),
            Self::Unknown => Self::Unknown,
        }
    }
}

fn read_image32(image: &[u8], mapping: ImageMapping, address: u32) -> Option<u32> {
    if !mapping.contains(address) || address.checked_add(4)? > mapping.range().end {
        return None;
    }
    let offset = (address - mapping.base) as usize;
    if offset.checked_add(4)? > image.len() {
        return None;
    }
    Some(u32::from_le_bytes(
        image[offset..offset + 4].try_into().ok()?,
    ))
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

fn operand_value(state: AbstractState, instruction: Instruction, operand: Operand2) -> AbstractValue {
    match operand {
        Operand2::Imm(value) => AbstractValue::Constant(value),
        Operand2::Reg {
            rm: 15,
            shift: 0,
            by_register: false,
            ..
        } => AbstractValue::Constant(aligned_pc(instruction.address, instruction.mode)),
        Operand2::Reg {
            rm,
            shift: 0,
            by_register: false,
            ..
        } => state.read(rm),
        Operand2::Reg { .. } => AbstractValue::Unknown,
    }
}

fn source_value(state: AbstractState, instruction: Instruction, register: u8) -> AbstractValue {
    if register == 15 {
        AbstractValue::Constant(aligned_pc(instruction.address, instruction.mode))
    } else {
        state.read(register)
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

pub(super) fn transfer_instruction(
    rom: &[u8],
    instruction: Instruction,
    mut state: AbstractState,
    mapping: ImageMapping,
) -> AbstractState {
    match instruction.kind {
        InstructionKind::Arm(ArmOp::Mov { rd, op2 }) => {
            state.write(rd, operand_value(state, instruction, op2));
        }
        InstructionKind::Arm(ArmOp::Add { rd, rn, op2 }) => {
            state.write(
                rd,
                add_values(source_value(state, instruction, rn), operand_value(state, instruction, op2)),
            );
        }
        InstructionKind::Arm(ArmOp::Sub { rd, rn, op2 }) => {
            state.write(
                rd,
                sub_values(source_value(state, instruction, rn), operand_value(state, instruction, op2)),
            );
        }
        InstructionKind::Arm(ArmOp::Load { rd, rn: 15, offset, .. }) => {
            let address = add_signed(aligned_pc(instruction.address, Mode::Arm), offset);
            state.write(
                rd,
                read_image32(rom, mapping, address)
                    .map(AbstractValue::Constant)
                    .unwrap_or_default(),
            );
        }
        InstructionKind::Arm(ArmOp::Load { rd, .. }) => {
            state.write(rd, AbstractValue::Unknown);
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::DataProcessing {
            op, rd, rn, op2, ..
        })) => {
            let rhs = operand_value(state, instruction, op2);
            let lhs = source_value(state, instruction, rn);
            let value = match op {
                ArmDataOp::Mov => rhs,
                ArmDataOp::Mvn => rhs.map_not(),
                ArmDataOp::Add => add_values(lhs, rhs),
                ArmDataOp::Sub => sub_values(lhs, rhs),
                ArmDataOp::Rsb => sub_values(rhs, lhs),
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
                read_image32(rom, mapping, address)
                    .map(AbstractValue::Constant)
                    .unwrap_or_default(),
            );
        }
        InstructionKind::Arm(ArmOp::Extended(ArmExtended::SingleDataTransfer {
            load: true, rd, ..
        }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::HalfwordTransfer {
            load: true, rd, ..
        }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::Mrs { rd, .. }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::Swap { rd, .. }))
        | InstructionKind::Arm(ArmOp::Extended(ArmExtended::Multiply { rd, .. })) => {
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
            state.write(
                rd,
                add_values(source_value(state, instruction, rn), AbstractValue::Constant(imm as u32)),
            );
        }
        InstructionKind::Thumb(ThumbOp::SubImm { rd, rn, imm }) => {
            state.write(
                rd,
                sub_values(source_value(state, instruction, rn), AbstractValue::Constant(imm as u32)),
            );
        }
        InstructionKind::Thumb(ThumbOp::LoadImm { rd, rn: 15, word_offset }) => {
            let address = aligned_pc(instruction.address, Mode::Thumb)
                .wrapping_add(word_offset as u32 * 4);
            state.write(
                rd,
                read_image32(rom, mapping, address)
                    .map(AbstractValue::Constant)
                    .unwrap_or_default(),
            );
        }
        InstructionKind::Thumb(ThumbOp::LoadImm { rd, .. }) => {
            state.write(rd, AbstractValue::Unknown);
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
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreRegister {
            load: true, rd, ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreSignHalf {
            rd, ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreImmediate {
            load: true, rd, ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::LoadStoreHalfword {
            load: true, rd, ..
        }))
        | InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::SpRelativeLoadStore {
            load: true, rd, ..
        })) => {
            state.write(rd, AbstractValue::Unknown);
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::MoveShifted {
            kind: 0,
            rd,
            rs,
            offset,
        })) => {
            state.write(
                rd,
                match source_value(state, instruction, rs) {
                    AbstractValue::Constant(value) => AbstractValue::Constant(value << offset),
                    AbstractValue::Unknown => AbstractValue::Unknown,
                },
            );
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::AddSubImmediate {
            sub, rd, rs, imm,
        })) => {
            let value = if sub {
                sub_values(source_value(state, instruction, rs), AbstractValue::Constant(imm as u32))
            } else {
                add_values(source_value(state, instruction, rs), AbstractValue::Constant(imm as u32))
            };
            state.write(rd, value);
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::AddSubRegister {
            sub, rd, rs, rn,
        })) => {
            let value = if sub {
                sub_values(source_value(state, instruction, rs), source_value(state, instruction, rn))
            } else {
                add_values(source_value(state, instruction, rs), source_value(state, instruction, rn))
            };
            state.write(rd, value);
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::HighRegister {
            op: 2,
            rd,
            rs,
        })) => {
            state.write(rd, source_value(state, instruction, rs));
        }
        InstructionKind::Thumb(ThumbOp::Extended(ThumbExtended::HighRegister { rd, .. })) => {
            state.write(rd, AbstractValue::Unknown);
        }
        _ => {}
    }
    state
}

pub(super) fn resolved_exchange_target(
    state: AbstractState,
    register: u8,
) -> Option<BlockKey> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::decode_arm;

    #[test]
    fn arm_pc_reads_use_architectural_pipeline_offset() {
        let instruction = decode_arm(0x0000_0114, 0xE28F_0001);
        let mapping = ImageMapping::bios(0x4000);
        let state = transfer_instruction(&[], instruction, AbstractState::default(), mapping);

        assert_eq!(
            state.read(0),
            AbstractValue::Constant(0x0000_011d),
            "ADD r0, pc, #1 must use ARM's architectural PC value of address + 8"
        );
    }
}
