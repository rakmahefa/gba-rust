//! ARM7TDMI architectural primitives shared by the recompiler execution contract.
//!
//! This module is deliberately pure: it contains no runtime state and no I/O.
//! The goal is to make the architectural rules independently testable before
//! they are consumed by semantic lowering, generated code, or the concrete
//! GBA runtime.

use crate::decoder::{Condition, Mode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NzcvMask {
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
}
impl NzcvMask {
    pub const NONE: Self = Self {
        n: false,
        z: false,
        c: false,
        v: false,
    };
    pub const ALL: Self = Self {
        n: true,
        z: true,
        c: true,
        v: true,
    };
    pub const NZC: Self = Self {
        n: true,
        z: true,
        c: true,
        v: false,
    };
    pub const NV: Self = Self {
        n: true,
        z: false,
        c: false,
        v: true,
    };
    pub const fn contains(self, other: Self) -> bool {
        (!other.n || self.n) && (!other.z || self.z) && (!other.c || self.c) && (!other.v || self.v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShiftResult {
    pub value: u32,
    pub carry: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftType {
    Lsl,
    Lsr,
    Asr,
    Ror,
}

pub fn shift_immediate(value: u32, kind: ShiftType, amount: u8, carry_in: bool) -> ShiftResult {
    match kind {
        ShiftType::Lsl => {
            if amount == 0 {
                return ShiftResult {
                    value,
                    carry: Some(carry_in),
                };
            }
            if amount < 32 {
                return ShiftResult {
                    value: value << amount,
                    carry: Some(value & (1 << (32 - amount)) != 0),
                };
            }
            if amount == 32 {
                return ShiftResult {
                    value: 0,
                    carry: Some(value & 1 != 0),
                };
            }
            ShiftResult {
                value: 0,
                carry: Some(false),
            }
        }
        ShiftType::Lsr => {
            let amount = if amount == 0 { 32 } else { amount as u32 };
            if amount < 32 {
                ShiftResult {
                    value: value >> amount,
                    carry: Some(value & (1 << (amount - 1)) != 0),
                }
            } else if amount == 32 {
                ShiftResult {
                    value: 0,
                    carry: Some(value & 0x8000_0000 != 0),
                }
            } else {
                ShiftResult {
                    value: 0,
                    carry: Some(false),
                }
            }
        }
        ShiftType::Asr => {
            let amount = if amount == 0 { 32 } else { amount as u32 };
            if amount < 32 {
                ShiftResult {
                    value: ((value as i32) >> amount) as u32,
                    carry: Some(value & (1 << (amount - 1)) != 0),
                }
            } else {
                ShiftResult {
                    value: if value & 0x8000_0000 != 0 {
                        u32::MAX
                    } else {
                        0
                    },
                    carry: Some(value & 0x8000_0000 != 0),
                }
            }
        }
        ShiftType::Ror => {
            if amount == 0 {
                // ARM operand2 ROR #0 is the RRX encoding: carry becomes bit 0
                // and the old carry becomes bit 31.
                return ShiftResult {
                    value: (u32::from(carry_in) << 31) | (value >> 1),
                    carry: Some(value & 1 != 0),
                };
            }
            let amount = (amount as u32) & 31;
            if amount == 0 {
                return ShiftResult {
                    value,
                    carry: Some(value & 0x8000_0000 != 0),
                };
            }
            ShiftResult {
                value: value.rotate_right(amount),
                carry: Some(value & (1 << (amount - 1)) != 0),
            }
        }
    }
}

pub fn shift_register(value: u32, kind: ShiftType, amount: u8, carry_in: bool) -> ShiftResult {
    match amount {
        0 => ShiftResult {
            value,
            carry: Some(carry_in),
        },
        1..=31 => shift_immediate(value, kind, amount, carry_in),
        32 => match kind {
            ShiftType::Lsl | ShiftType::Lsr => ShiftResult {
                value: 0,
                carry: Some(
                    value
                        & if matches!(kind, ShiftType::Lsl) {
                            1
                        } else {
                            0x8000_0000
                        }
                        != 0,
                ),
            },
            ShiftType::Asr => ShiftResult {
                value: if value & 0x8000_0000 != 0 {
                    u32::MAX
                } else {
                    0
                },
                carry: Some(value & 0x8000_0000 != 0),
            },
            ShiftType::Ror => ShiftResult {
                value,
                carry: Some(value & 0x8000_0000 != 0),
            },
        },
        _ => ShiftResult {
            value: 0,
            carry: Some(false),
        },
    }
}

pub fn add_with_carry(lhs: u32, rhs: u32, carry_in: bool) -> (u32, Nzcv) {
    let wide = lhs as u64 + rhs as u64 + u64::from(carry_in);
    let result = wide as u32;
    let c = wide > u32::MAX as u64;
    let v = (!(lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0;
    (
        result,
        Nzcv::new(result & 0x8000_0000 != 0, result == 0, c, v),
    )
}

pub fn sub_with_borrow(lhs: u32, rhs: u32, borrow_in: bool) -> (u32, Nzcv) {
    let rhs_wide = rhs as u64 + u64::from(borrow_in);
    let result = lhs.wrapping_sub(rhs).wrapping_sub(u32::from(borrow_in));
    let c = lhs as u64 >= rhs_wide;
    let v = ((lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0;
    (
        result,
        Nzcv::new(result & 0x8000_0000 != 0, result == 0, c, v),
    )
}

pub fn condition_holds(nzcv: Nzcv, condition: Condition) -> bool {
    match condition {
        Condition::Eq => nzcv.z,
        Condition::Ne => !nzcv.z,
        Condition::Cs => nzcv.c,
        Condition::Cc => !nzcv.c,
        Condition::Mi => nzcv.n,
        Condition::Pl => !nzcv.n,
        Condition::Vs => nzcv.v,
        Condition::Vc => !nzcv.v,
        Condition::Hi => nzcv.c && !nzcv.z,
        Condition::Ls => !nzcv.c || nzcv.z,
        Condition::Ge => nzcv.n == nzcv.v,
        Condition::Lt => nzcv.n != nzcv.v,
        Condition::Gt => !nzcv.z && nzcv.n == nzcv.v,
        Condition::Le => nzcv.z || nzcv.n != nzcv.v,
        Condition::Al => true,
    }
}

pub fn architectural_pc(address: u32, mode: Mode) -> u32 {
    address.wrapping_add(match mode {
        Mode::Arm => 8,
        Mode::Thumb => 4,
    })
}
pub fn branch_target(target: u32, mode: Mode) -> u32 {
    target
        & match mode {
            Mode::Arm => !3,
            Mode::Thumb => !1,
        }
}
pub fn exchange_target(value: u32) -> (u32, Mode) {
    let mode = if value & 1 != 0 {
        Mode::Thumb
    } else {
        Mode::Arm
    };
    (
        value & if matches!(mode, Mode::Thumb) { !1 } else { !3 },
        mode,
    )
}
pub fn link_address(address: u32, size: u8, mode: Mode) -> u32 {
    let value = address.wrapping_add(size as u32);
    if matches!(mode, Mode::Thumb) {
        value | 1
    } else {
        value
    }
}
pub fn rotate_unaligned_word(raw_le: u32, address: u32) -> u32 {
    raw_le.rotate_right((address & 3) * 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_with_carry_matches_arm_carry_and_overflow_rules() {
        let (r, f) = add_with_carry(0x7fff_ffff, 0, true);
        assert_eq!(r, 0x8000_0000);
        assert!(f.n && !f.z && !f.c && f.v);
        let (r, f) = add_with_carry(u32::MAX, 0, true);
        assert_eq!(r, 0);
        assert!(!f.n && f.z && f.c && !f.v);
    }

    #[test]
    fn subtract_with_borrow_uses_c_as_not_borrow() {
        let (_, f) = sub_with_borrow(3, 1, false);
        assert!(f.c);
        let (_, f) = sub_with_borrow(1, 3, false);
        assert!(!f.c);
        let (_, f) = sub_with_borrow(1, 0, true);
        assert!(f.c);
    }

    #[test]
    fn immediate_shift_special_cases_are_explicit() {
        assert_eq!(
            shift_immediate(1, ShiftType::Lsr, 0, true),
            ShiftResult {
                value: 0,
                carry: Some(false)
            }
        );
        let rrx = shift_immediate(0x0000_0001, ShiftType::Ror, 0, true);
        assert_eq!(rrx.value, 0x8000_0000);
        assert_eq!(rrx.carry, Some(true));
    }

    #[test]
    fn register_shift_zero_preserves_carry_and_value() {
        assert_eq!(
            shift_register(0x1234, ShiftType::Ror, 0, true),
            ShiftResult {
                value: 0x1234,
                carry: Some(true)
            }
        );
    }

    #[test]
    fn pc_and_exchange_semantics_are_state_aware() {
        assert_eq!(architectural_pc(0x0800_0100, Mode::Arm), 0x0800_0108);
        assert_eq!(architectural_pc(0x0800_0100, Mode::Thumb), 0x0800_0104);
        assert_eq!(exchange_target(0x0800_0101), (0x0800_0100, Mode::Thumb));
        assert_eq!(exchange_target(0x0800_0100), (0x0800_0100, Mode::Arm));
    }

    #[test]
    fn unaligned_word_load_matches_arm_rotation_rule() {
        assert_eq!(rotate_unaligned_word(0x4433_2211, 1), 0x1144_3322);
        assert_eq!(rotate_unaligned_word(0x4433_2211, 2), 0x2211_4433);
        assert_eq!(rotate_unaligned_word(0x4433_2211, 3), 0x3322_1144);
    }
}
