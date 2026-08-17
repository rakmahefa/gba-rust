use crate::decoder::{Condition, Mode};

/// Architectural execution contract shared by semantic IR lowering, generated Rust,
/// and the concrete runtime. This module intentionally contains no runtime state;
/// it defines the operations the runtime must implement faithfully.
pub const REG_COUNT: usize = 16;
pub const REG_PC: u8 = 15;
pub const REG_LR: u8 = 14;
pub const REG_SP: u8 = 13;

pub const CPSR_N: u32 = 1 << 31;
pub const CPSR_Z: u32 = 1 << 30;
pub const CPSR_C: u32 = 1 << 29;
pub const CPSR_V: u32 = 1 << 28;
pub const CPSR_T: u32 = 1 << 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nzcv {
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
}

impl Nzcv {
    pub const fn new(n: bool, z: bool, c: bool, v: bool) -> Self {
        Self { n, z, c, v }
    }

    pub fn bits(self) -> u32 {
        (if self.n { CPSR_N } else { 0 })
            | (if self.z { CPSR_Z } else { 0 })
            | (if self.c { CPSR_C } else { 0 })
            | (if self.v { CPSR_V } else { 0 })
    }
}

pub fn add_flags(lhs: u32, rhs: u32, result: u32) -> Nzcv {
    let wide = (lhs as u64) + (rhs as u64);
    let carry = wide > u32::MAX as u64;
    let overflow = ((!(lhs ^ rhs)) & (lhs ^ result) & (1 << 31)) != 0;
    Nzcv::new(result & CPSR_N != 0, result == 0, carry, overflow)
}

pub fn sub_flags(lhs: u32, rhs: u32, result: u32) -> Nzcv {
    let borrow = lhs < rhs;
    let overflow = (((lhs ^ rhs) & (lhs ^ result)) & (1 << 31)) != 0;
    Nzcv::new(result & CPSR_N != 0, result == 0, !borrow, overflow)
}

pub fn condition_holds(cpsr: u32, condition: Condition) -> bool {
    let n = cpsr & CPSR_N != 0;
    let z = cpsr & CPSR_Z != 0;
    let c = cpsr & CPSR_C != 0;
    let v = cpsr & CPSR_V != 0;
    match condition {
        Condition::Eq => z,
        Condition::Ne => !z,
        Condition::Cs => c,
        Condition::Cc => !c,
        Condition::Mi => n,
        Condition::Pl => !n,
        Condition::Vs => v,
        Condition::Vc => !v,
        Condition::Hi => c && !z,
        Condition::Ls => !c || z,
        Condition::Ge => n == v,
        Condition::Lt => n != v,
        Condition::Gt => !z && (n == v),
        Condition::Le => z || (n != v),
        Condition::Al => true,
    }
}

pub fn branch_target(raw_target: u32, mode: Mode) -> u32 {
    match mode {
        Mode::Arm => raw_target & !3,
        Mode::Thumb => raw_target & !1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::Condition;

    #[test]
    fn add_flags_reports_carry_and_overflow() {
        let result = 0x7fff_ffffu32.wrapping_add(1);
        let flags = add_flags(0x7fff_ffff, 1, result);
        assert!(flags.n);
        assert!(!flags.z);
        assert!(!flags.c);
        assert!(flags.v);

        let result = u32::MAX.wrapping_add(1);
        let flags = add_flags(u32::MAX, 1, result);
        assert!(flags.z);
        assert!(flags.c);
        assert!(!flags.v);
    }

    #[test]
    fn sub_flags_reports_no_borrow_as_carry() {
        let flags = sub_flags(3, 1, 2);
        assert!(!flags.n);
        assert!(!flags.z);
        assert!(flags.c);
        assert!(!flags.v);

        let flags = sub_flags(1, 3, u32::MAX - 1);
        assert!(flags.n);
        assert!(!flags.z);
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
