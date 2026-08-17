//! Pure ARM7TDMI helpers used by the concrete runtime.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nzcv { pub n: bool, pub z: bool, pub c: bool, pub v: bool }
impl Nzcv {
    pub const fn new(n: bool, z: bool, c: bool, v: bool) -> Self { Self { n, z, c, v } }
    pub fn from_cpsr(cpsr: u32) -> Self { Self { n: cpsr & (1 << 31) != 0, z: cpsr & (1 << 30) != 0, c: cpsr & (1 << 29) != 0, v: cpsr & (1 << 28) != 0 } }
    pub fn bits(self) -> u32 { (if self.n { 1 << 31 } else { 0 }) | (if self.z { 1 << 30 } else { 0 }) | (if self.c { 1 << 29 } else { 0 }) | (if self.v { 1 << 28 } else { 0 }) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShiftResult { pub value: u32, pub carry: bool }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftKind { Lsl, Lsr, Asr, Ror }

pub fn add_with_carry(lhs: u32, rhs: u32, carry_in: bool) -> (u32, Nzcv) {
    let wide = lhs as u64 + rhs as u64 + u64::from(carry_in);
    let result = wide as u32;
    (result, Nzcv { n: result & 0x8000_0000 != 0, z: result == 0, c: wide > u32::MAX as u64, v: (!(lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0 })
}

pub fn sub_with_borrow(lhs: u32, rhs: u32, borrow_in: bool) -> (u32, Nzcv) {
    let rhs_wide = rhs as u64 + u64::from(borrow_in);
    let result = lhs.wrapping_sub(rhs).wrapping_sub(u32::from(borrow_in));
    (result, Nzcv { n: result & 0x8000_0000 != 0, z: result == 0, c: lhs as u64 >= rhs_wide, v: ((lhs ^ rhs) & (lhs ^ result) & 0x8000_0000) != 0 })
}

pub fn shift_immediate(value: u32, kind: ShiftKind, amount: u8, carry_in: bool) -> ShiftResult {
    match kind {
        ShiftKind::Lsl => match amount {
            0 => ShiftResult { value, carry: carry_in },
            1..=31 => ShiftResult { value: value << amount, carry: value & (1 << (32 - amount)) != 0 },
            32 => ShiftResult { value: 0, carry: value & 1 != 0 },
            _ => ShiftResult { value: 0, carry: false },
        },
        ShiftKind::Lsr => {
            let amount = if amount == 0 { 32 } else { amount as u32 };
            if amount < 32 { ShiftResult { value: value >> amount, carry: value & (1 << (amount - 1)) != 0 } }
            else if amount == 32 { ShiftResult { value: 0, carry: value & 0x8000_0000 != 0 } }
            else { ShiftResult { value: 0, carry: false } }
        }
        ShiftKind::Asr => {
            let amount = if amount == 0 { 32 } else { amount as u32 };
            if amount < 32 { ShiftResult { value: ((value as i32) >> amount) as u32, carry: value & (1 << (amount - 1)) != 0 } }
            else { ShiftResult { value: if value & 0x8000_0000 != 0 { u32::MAX } else { 0 }, carry: value & 0x8000_0000 != 0 } }
        }
        ShiftKind::Ror => {
            if amount == 0 { return ShiftResult { value: (u32::from(carry_in) << 31) | (value >> 1), carry: value & 1 != 0 }; }
            let amount = (amount as u32) & 31;
            if amount == 0 { return ShiftResult { value, carry: value & 0x8000_0000 != 0 }; }
            ShiftResult { value: value.rotate_right(amount), carry: value & (1 << (amount - 1)) != 0 }
        }
    }
}

pub fn shift_register(value: u32, kind: ShiftKind, amount: u8, carry_in: bool) -> ShiftResult {
    match amount {
        0 => ShiftResult { value, carry: carry_in },
        1..=31 => shift_immediate(value, kind, amount, carry_in),
        32 => match kind {
            ShiftKind::Lsl => ShiftResult { value: 0, carry: value & 1 != 0 },
            ShiftKind::Lsr => ShiftResult { value: 0, carry: value & 0x8000_0000 != 0 },
            ShiftKind::Asr => ShiftResult { value: if value & 0x8000_0000 != 0 { u32::MAX } else { 0 }, carry: value & 0x8000_0000 != 0 },
            ShiftKind::Ror => ShiftResult { value, carry: value & 0x8000_0000 != 0 },
        },
        _ => ShiftResult { value: 0, carry: false },
    }
}

pub fn condition_holds(cpsr: u32, condition: u8) -> bool {
    let f = Nzcv::from_cpsr(cpsr);
    match condition { 0 => f.z, 1 => !f.z, 2 => f.c, 3 => !f.c, 4 => f.n, 5 => !f.n, 6 => f.v, 7 => !f.v, 8 => f.c && !f.z, 9 => !f.c || f.z, 10 => f.n == f.v, 11 => f.n != f.v, 12 => !f.z && f.n == f.v, 13 => f.z || f.n != f.v, 14 => true, _ => false }
}

pub fn architectural_pc(address: u32, thumb: bool) -> u32 { address.wrapping_add(if thumb { 4 } else { 8 }) }
pub fn link_address(address: u32, size: u8, thumb: bool) -> u32 { let value = address.wrapping_add(size as u32); if thumb { value | 1 } else { value } }
pub fn exchange_target(value: u32) -> (u32, bool) { let thumb = value & 1 != 0; (value & if thumb { !1 } else { !3 }, thumb) }
pub fn rotate_unaligned_word(value: u32, address: u32) -> u32 { value.rotate_right((address & 3) * 8) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constructor_round_trips_bits() { let flags = Nzcv::new(true, false, true, false); assert_eq!(Nzcv::from_cpsr(flags.bits()), flags); }
}
