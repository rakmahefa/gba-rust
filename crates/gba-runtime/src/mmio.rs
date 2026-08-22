//! Architectural GBA memory-mapped I/O register definitions.
//!
//! This module owns register addresses and bit definitions shared by the
//! runtime memory bus and device models. Device state remains in their
//! respective runtime components; this module is intentionally declarative.

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

pub const DISPSTAT_VBLANK: u16 = 1 << 0;
pub const DISPSTAT_HBLANK: u16 = 1 << 1;
pub const DISPSTAT_VCOUNT_IRQ: u16 = 1 << 2;
pub const DISPSTAT_VCOUNT_MASK: u16 = 0xff << 8;
pub const DISPSTAT_STATUS_MASK: u16 = DISPSTAT_VBLANK | DISPSTAT_HBLANK;

pub const DISPSTAT_HI: u32 = DISPSTAT + 1;
pub const VCOUNT_HI: u32 = VCOUNT + 1;
pub const KEYINPUT_HI: u32 = KEYINPUT + 1;
pub const IE_HI: u32 = IE + 1;
pub const IF_HI: u32 = IF + 1;
pub const WAITCNT_HI: u32 = WAITCNT + 1;
pub const IME_HI: u32 = IME + 1;

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
}
