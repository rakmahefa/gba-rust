//! Architectural GBA memory-mapped I/O register definitions.
//!
//! This module owns the register contract shared by the CPU bus and device
//! models. Each register has an explicit width, access policy and writable
//! mask so device state cannot accidentally inherit generic byte-array
//! semantics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioWidth {
    Byte,
    Halfword,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioAccess {
    ReadOnly,
    ReadWrite,
    WriteOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioRegister {
    pub address: u32,
    pub width: MmioWidth,
    pub access: MmioAccess,
    pub writable_mask: u32,
}

impl MmioRegister {
    pub const fn new(
        address: u32,
        width: MmioWidth,
        access: MmioAccess,
        writable_mask: u32,
    ) -> Self {
        Self {
            address,
            width,
            access,
            writable_mask,
        }
    }
}

pub const DISPCNT: u32 = 0x0400_0000;
pub const DISPSTAT: u32 = 0x0400_0004;
pub const VCOUNT: u32 = 0x0400_0006;
pub const KEYINPUT: u32 = 0x0400_0130;
pub const IE: u32 = 0x0400_0200;
pub const IF: u32 = 0x0400_0202;
pub const WAITCNT: u32 = 0x0400_0204;
pub const IME: u32 = 0x0400_0208;
pub const POSTFLG: u32 = 0x0400_0300;
pub const HALTCNT: u32 = 0x0400_0301;

pub const DISPCNT_HI: u32 = DISPCNT + 1;
pub const DISPSTAT_HI: u32 = DISPSTAT + 1;
pub const VCOUNT_HI: u32 = VCOUNT + 1;
pub const KEYINPUT_HI: u32 = KEYINPUT + 1;
pub const IE_HI: u32 = IE + 1;
pub const IF_HI: u32 = IF + 1;
pub const WAITCNT_HI: u32 = WAITCNT + 1;
pub const IME_HI: u32 = IME + 1;

/// DISPCNT bit 3 is read-only on the GBA; the other control bits are writable.
pub const DISPCNT_WRITABLE_MASK: u16 = 0xfff7;
/// DISPSTAT status bits 0..2 are hardware-owned; bits 3..5 and 8..15 are writable.
pub const DISPSTAT_WRITABLE_MASK: u16 = 0xff38;
/// Interrupt controller exposes fourteen interrupt sources.
pub const INTERRUPT_SOURCE_MASK: u16 = 0x3fff;
/// WAITCNT bits 0..12 and 14 are writable; bit 13 is reserved and bit 15 is read-only.
pub const WAITCNT_WRITABLE_MASK: u16 = 0x5fff;
/// IME is a single-bit master interrupt enable register.
pub const IME_WRITABLE_MASK: u16 = 0x0001;
pub const POSTFLG_WRITABLE_MASK: u8 = 0x01;

pub const DISPSTAT_VBLANK: u16 = 1 << 0;
pub const DISPSTAT_HBLANK: u16 = 1 << 1;
pub const DISPSTAT_VCOUNT: u16 = 1 << 2;
pub const DISPSTAT_VBLANK_IRQ: u16 = 1 << 3;
pub const DISPSTAT_HBLANK_IRQ: u16 = 1 << 4;
pub const DISPSTAT_VCOUNT_IRQ: u16 = 1 << 5;
pub const DISPSTAT_VCOUNT_MASK: u16 = 0xff << 8;
pub const DISPSTAT_STATUS_MASK: u16 = DISPSTAT_VBLANK | DISPSTAT_HBLANK | DISPSTAT_VCOUNT;

pub const DISPCNT_REGISTER: MmioRegister = MmioRegister::new(
    DISPCNT,
    MmioWidth::Halfword,
    MmioAccess::ReadWrite,
    DISPCNT_WRITABLE_MASK as u32,
);
pub const DISPSTAT_REGISTER: MmioRegister = MmioRegister::new(
    DISPSTAT,
    MmioWidth::Halfword,
    MmioAccess::ReadWrite,
    DISPSTAT_WRITABLE_MASK as u32,
);
pub const VCOUNT_REGISTER: MmioRegister = MmioRegister::new(
    VCOUNT,
    MmioWidth::Halfword,
    MmioAccess::ReadOnly,
    0,
);
pub const KEYINPUT_REGISTER: MmioRegister = MmioRegister::new(
    KEYINPUT,
    MmioWidth::Halfword,
    MmioAccess::ReadOnly,
    0,
);
pub const IE_REGISTER: MmioRegister = MmioRegister::new(
    IE,
    MmioWidth::Halfword,
    MmioAccess::ReadWrite,
    INTERRUPT_SOURCE_MASK as u32,
);
pub const IF_REGISTER: MmioRegister = MmioRegister::new(
    IF,
    MmioWidth::Halfword,
    MmioAccess::ReadWrite,
    INTERRUPT_SOURCE_MASK as u32,
);
pub const WAITCNT_REGISTER: MmioRegister = MmioRegister::new(
    WAITCNT,
    MmioWidth::Halfword,
    MmioAccess::ReadWrite,
    WAITCNT_WRITABLE_MASK as u32,
);
pub const IME_REGISTER: MmioRegister = MmioRegister::new(
    IME,
    MmioWidth::Halfword,
    MmioAccess::ReadWrite,
    IME_WRITABLE_MASK as u32,
);
pub const POSTFLG_REGISTER: MmioRegister = MmioRegister::new(
    POSTFLG,
    MmioWidth::Byte,
    MmioAccess::ReadWrite,
    POSTFLG_WRITABLE_MASK as u32,
);
pub const HALTCNT_REGISTER: MmioRegister = MmioRegister::new(
    HALTCNT,
    MmioWidth::Byte,
    MmioAccess::WriteOnly,
    0xff,
);

/// Returns the canonical register contract for an exact register address.
/// High bytes of halfword registers deliberately resolve to the same device
/// register because the runtime supports byte-granular CPU bus accesses.
#[inline]
pub const fn register(address: u32) -> Option<MmioRegister> {
    match address {
        DISPCNT | DISPCNT_HI => Some(DISPCNT_REGISTER),
        DISPSTAT | DISPSTAT_HI => Some(DISPSTAT_REGISTER),
        VCOUNT | VCOUNT_HI => Some(VCOUNT_REGISTER),
        KEYINPUT | KEYINPUT_HI => Some(KEYINPUT_REGISTER),
        IE | IE_HI => Some(IE_REGISTER),
        IF | IF_HI => Some(IF_REGISTER),
        WAITCNT | WAITCNT_HI => Some(WAITCNT_REGISTER),
        IME | IME_HI => Some(IME_REGISTER),
        POSTFLG => Some(POSTFLG_REGISTER),
        HALTCNT => Some(HALTCNT_REGISTER),
        _ => None,
    }
}

#[inline]
pub const fn dispstat_vcount(value: u16) -> u8 {
    ((value >> 8) & 0xff) as u8
}

#[inline]
pub const fn with_dispstat_vcount(value: u16, compare: u8) -> u16 {
    (value & !DISPSTAT_VCOUNT_MASK) | ((compare as u16) << 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispstat_vcount_compare_round_trips_without_touching_status_bits() {
        let value = DISPSTAT_VBLANK | DISPSTAT_HBLANK;
        let updated = with_dispstat_vcount(value, 123);
        assert_eq!(dispstat_vcount(updated), 123);
        assert_eq!(updated & DISPSTAT_STATUS_MASK, value);
    }

    #[test]
    fn architectural_mmio_addresses_are_in_the_io_window() {
        for address in [DISPCNT, DISPSTAT, VCOUNT, KEYINPUT, IE, IF, WAITCNT, IME, POSTFLG, HALTCNT] {
            assert!((0x0400_0000..=0x0400_03ff).contains(&address));
        }
    }

    #[test]
    fn dispstat_irq_enables_are_distinct_from_status_bits() {
        assert_eq!(
            DISPSTAT_STATUS_MASK & (DISPSTAT_VBLANK_IRQ | DISPSTAT_HBLANK_IRQ | DISPSTAT_VCOUNT_IRQ),
            0
        );
    }

    #[test]
    fn register_contract_exposes_access_policy_and_masks() {
        assert_eq!(register(DISPCNT), Some(DISPCNT_REGISTER));
        assert_eq!(register(DISPCNT_HI), Some(DISPCNT_REGISTER));
        assert_eq!(register(VCOUNT).unwrap().access, MmioAccess::ReadOnly);
        assert_eq!(register(HALTCNT).unwrap().access, MmioAccess::WriteOnly);
        assert_eq!(register(DISPCNT).unwrap().writable_mask, 0xfff7);
        assert_eq!(register(DISPSTAT).unwrap().writable_mask, 0xff38);
        assert_eq!(register(IE).unwrap().writable_mask, 0x3fff);
        assert_eq!(register(WAITCNT).unwrap().writable_mask, 0x5fff);
        assert!(register(0x0400_001f).is_none());
    }
}
