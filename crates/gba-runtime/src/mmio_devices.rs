//! Peripheral MMIO contracts layered on the core register definitions.
//!
//! This module is deliberately declarative. DMA and timer implementations can
//! consume these descriptors without duplicating widths, access policies, or
//! architectural writable masks.

use crate::mmio::{MmioAccess, MmioRegister, MmioWidth};

pub const DMA_ADDRESS_MASK: u32 = 0x0fff_ffff;
pub const DMA_COUNT_MASK: u16 = 0x3fff;
pub const DMA3_COUNT_MASK: u16 = 0xffff;
pub const DMA_CONTROL_MASK: u16 = 0xf7e0;
pub const DMA3_CONTROL_MASK: u16 = 0xffe0;
pub const TIMER_CONTROL_MASK: u16 = 0x00c7;

const fn dma_address(address: u32) -> MmioRegister {
    MmioRegister::new(address, MmioWidth::Word, MmioAccess::WriteOnly, DMA_ADDRESS_MASK)
}

const fn dma_count(address: u32, mask: u16) -> MmioRegister {
    MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::WriteOnly, mask as u32)
}

const fn dma_control(address: u32, mask: u16) -> MmioRegister {
    MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, mask as u32)
}

const fn timer_data(address: u32) -> MmioRegister {
    MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, u16::MAX as u32)
}

const fn timer_control(address: u32) -> MmioRegister {
    MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, TIMER_CONTROL_MASK as u32)
}

pub const DMA0SAD: MmioRegister = dma_address(0x0400_00b0);
pub const DMA0DAD: MmioRegister = dma_address(0x0400_00b4);
pub const DMA0CNT_L: MmioRegister = dma_count(0x0400_00b8, DMA_COUNT_MASK);
pub const DMA0CNT_H: MmioRegister = dma_control(0x0400_00ba, DMA_CONTROL_MASK);

pub const DMA1SAD: MmioRegister = dma_address(0x0400_00bc);
pub const DMA1DAD: MmioRegister = dma_address(0x0400_00c0);
pub const DMA1CNT_L: MmioRegister = dma_count(0x0400_00c4, DMA_COUNT_MASK);
pub const DMA1CNT_H: MmioRegister = dma_control(0x0400_00c6, DMA_CONTROL_MASK);

pub const DMA2SAD: MmioRegister = dma_address(0x0400_00c8);
pub const DMA2DAD: MmioRegister = dma_address(0x0400_00cc);
pub const DMA2CNT_L: MmioRegister = dma_count(0x0400_00d0, DMA_COUNT_MASK);
pub const DMA2CNT_H: MmioRegister = dma_control(0x0400_00d2, DMA_CONTROL_MASK);

pub const DMA3SAD: MmioRegister = dma_address(0x0400_00d4);
pub const DMA3DAD: MmioRegister = dma_address(0x0400_00d8);
pub const DMA3CNT_L: MmioRegister = dma_count(0x0400_00dc, DMA3_COUNT_MASK);
pub const DMA3CNT_H: MmioRegister = dma_control(0x0400_00de, DMA3_CONTROL_MASK);

pub const TIMER0CNT_L: MmioRegister = timer_data(0x0400_0100);
pub const TIMER0CNT_H: MmioRegister = timer_control(0x0400_0102);
pub const TIMER1CNT_L: MmioRegister = timer_data(0x0400_0104);
pub const TIMER1CNT_H: MmioRegister = timer_control(0x0400_0106);
pub const TIMER2CNT_L: MmioRegister = timer_data(0x0400_0108);
pub const TIMER2CNT_H: MmioRegister = timer_control(0x0400_010a);
pub const TIMER3CNT_L: MmioRegister = timer_data(0x0400_010c);
pub const TIMER3CNT_H: MmioRegister = timer_control(0x0400_010e);

#[inline]
pub const fn register(address: u32) -> Option<MmioRegister> {
    match address {
        0x0400_00b0..=0x0400_00b3 => Some(DMA0SAD),
        0x0400_00b4..=0x0400_00b7 => Some(DMA0DAD),
        0x0400_00b8 | 0x0400_00b9 => Some(DMA0CNT_L),
        0x0400_00ba | 0x0400_00bb => Some(DMA0CNT_H),
        0x0400_00bc..=0x0400_00bf => Some(DMA1SAD),
        0x0400_00c0..=0x0400_00c3 => Some(DMA1DAD),
        0x0400_00c4 | 0x0400_00c5 => Some(DMA1CNT_L),
        0x0400_00c6 | 0x0400_00c7 => Some(DMA1CNT_H),
        0x0400_00c8..=0x0400_00cb => Some(DMA2SAD),
        0x0400_00cc..=0x0400_00cf => Some(DMA2DAD),
        0x0400_00d0 | 0x0400_00d1 => Some(DMA2CNT_L),
        0x0400_00d2 | 0x0400_00d3 => Some(DMA2CNT_H),
        0x0400_00d4..=0x0400_00d7 => Some(DMA3SAD),
        0x0400_00d8..=0x0400_00db => Some(DMA3DAD),
        0x0400_00dc | 0x0400_00dd => Some(DMA3CNT_L),
        0x0400_00de | 0x0400_00df => Some(DMA3CNT_H),
        0x0400_0100 | 0x0400_0101 => Some(TIMER0CNT_L),
        0x0400_0102 | 0x0400_0103 => Some(TIMER0CNT_H),
        0x0400_0104 | 0x0400_0105 => Some(TIMER1CNT_L),
        0x0400_0106 | 0x0400_0107 => Some(TIMER1CNT_H),
        0x0400_0108 | 0x0400_0109 => Some(TIMER2CNT_L),
        0x0400_010a | 0x0400_010b => Some(TIMER2CNT_H),
        0x0400_010c | 0x0400_010d => Some(TIMER3CNT_L),
        0x0400_010e | 0x0400_010f => Some(TIMER3CNT_H),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dma_registers_preserve_width_and_access_direction() {
        assert_eq!(DMA0SAD.width, MmioWidth::Word);
        assert_eq!(DMA0SAD.access, MmioAccess::WriteOnly);
        assert_eq!(DMA0CNT_L.writable_mask, DMA_COUNT_MASK as u32);
        assert_eq!(DMA3CNT_L.writable_mask, DMA3_COUNT_MASK as u32);
        assert_eq!(DMA3CNT_H.writable_mask, DMA3_CONTROL_MASK as u32);
    }

    #[test]
    fn timer_contract_matches_runtime_timer_shape() {
        assert_eq!(TIMER0CNT_L.width, MmioWidth::Halfword);
        assert_eq!(TIMER0CNT_H.writable_mask, TIMER_CONTROL_MASK as u32);
        assert_eq!(TIMER3CNT_H.address, 0x0400_010e);
    }

    #[test]
    fn byte_addresses_resolve_to_their_parent_register() {
        assert_eq!(register(0x0400_00b3), Some(DMA0SAD));
        assert_eq!(register(0x0400_00ba), Some(DMA0CNT_H));
        assert_eq!(register(0x0400_010f), Some(TIMER3CNT_H));
        assert!(register(0x0400_0110).is_none());
    }
}
