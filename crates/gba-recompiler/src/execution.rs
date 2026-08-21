use crate::architecture;
use crate::decoder::{Condition, Mode};

/// Stable execution-contract facade kept for callers that already import
/// `gba_recompiler::execution`. The architectural rules themselves live in
/// `architecture`, so decoder, semantic IR and runtime-facing codegen can use
/// the same pure definitions.
pub const REG_COUNT: usize = 16;
pub const REG_PC: u8 = 15;
pub const REG_LR: u8 = 14;
pub const REG_SP: u8 = 13;

pub const CPSR_N: u32 = 1 << 31;
pub const CPSR_Z: u32 = 1 << 30;
pub const CPSR_C: u32 = 1 << 29;
pub const CPSR_V: u32 = 1 << 28;
pub const CPSR_T: u32 = 1 << 5;

pub use architecture::{
    add_with_carry, architectural_pc, exchange_target, link_address, rotate_unaligned_word,
    shift_immediate, shift_register, sub_with_borrow,
};
pub use architecture::{Nzcv, NzcvMask, ShiftResult, ShiftType};

pub fn add_flags(lhs: u32, rhs: u32, result: u32) -> Nzcv {
    let c = (lhs as u64 + rhs as u64) > u32::MAX as u64;
    let v = (!(lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0;
    Nzcv::new(result & 0x8000_0000 != 0, result == 0, c, v)
}

pub fn sub_flags(lhs: u32, rhs: u32, result: u32) -> Nzcv {
    let c = lhs >= rhs;
    let v = ((lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0;
    Nzcv::new(result & 0x8000_0000 != 0, result == 0, c, v)
}

pub fn condition_holds(cpsr: u32, condition: Condition) -> bool {
    let nzcv = Nzcv {
        n: cpsr & CPSR_N != 0,
        z: cpsr & CPSR_Z != 0,
        c: cpsr & CPSR_C != 0,
        v: cpsr & CPSR_V != 0,
    };
    architecture::condition_holds(nzcv, condition)
}

pub fn branch_target(raw_target: u32, mode: Mode) -> u32 {
    architecture::branch_target(raw_target, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_add_flags_keeps_legacy_signature() {
        let result = 0x8000_0000u32;
        let flags = add_flags(0x7fff_ffff, 1, result);
        assert!(flags.n && flags.v);
        assert!(!flags.c && !flags.z);
    }

    #[test]
    fn compatibility_sub_flags_keeps_c_as_not_borrow() {
        let flags = sub_flags(3, 1, 2);
        assert!(flags.c);
        let flags = sub_flags(1, 3, u32::MAX - 1);
        assert!(!flags.c);
    }

    #[test]
    fn every_condition_uses_the_architectural_nzcv_relation() {
        let cpsr = CPSR_C;
        assert!(condition_holds(cpsr, Condition::Cs));
        assert!(condition_holds(cpsr, Condition::Hi));
        assert!(!condition_holds(cpsr, Condition::Ls));
        assert!(condition_holds(cpsr, Condition::Al));
    }

    #[test]
    fn branch_targets_are_aligned_to_the_execution_state() {
        assert_eq!(branch_target(0x0800_0003, Mode::Arm), 0x0800_0000);
        assert_eq!(branch_target(0x0800_0003, Mode::Thumb), 0x0800_0002);
    }
}
