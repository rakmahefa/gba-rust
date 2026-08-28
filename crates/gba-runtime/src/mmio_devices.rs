//! Peripheral MMIO contracts layered on the core register definitions.
//!
//! Device implementations consume these descriptors so widths, access policy
//! and architectural masks stay centralized.

use crate::mmio::{MmioAccess, MmioRegister, MmioWidth};

pub const DMA_ADDRESS_MASK: u32 = 0x0fff_ffff;
pub const DMA_COUNT_MASK: u16 = 0x3fff;
pub const DMA3_COUNT_MASK: u16 = 0xffff;
pub const DMA_CONTROL_MASK: u16 = 0xf7e0;
pub const DMA3_CONTROL_MASK: u16 = 0xffe0;
pub const TIMER_CONTROL_MASK: u16 = 0x00c7;
pub const BG_TEXT_CONTROL_MASK: u16 = 0xdfcf;
pub const BG_AFFINE_CONTROL_MASK: u16 = 0xffcf;
pub const BG_SCROLL_MASK: u16 = 0x03ff;
pub const WINDOW_COORDINATE_MASK: u16 = 0xffff;
pub const WINDOW_CONTROL_MASK: u16 = 0x3f3f;
pub const MOSAIC_MASK: u16 = 0xffff;
pub const BLEND_CONTROL_MASK: u16 = 0x3fcf;
pub const BLEND_ALPHA_MASK: u16 = 0x1f1f;
pub const BLEND_Y_MASK: u16 = 0x001f;
pub const SOUND_FIFO_MASK: u32 = u32::MAX;

const fn dma_address(address: u32) -> MmioRegister { MmioRegister::new(address, MmioWidth::Word, MmioAccess::ReadWrite, DMA_ADDRESS_MASK) }
const fn dma_count(address: u32, mask: u16) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, mask as u32) }
const fn dma_control(address: u32, mask: u16) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, mask as u32) }
const fn timer_data(address: u32) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, u16::MAX as u32) }
const fn timer_control(address: u32) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, TIMER_CONTROL_MASK as u32) }
const fn ppu_control(address: u32, mask: u16) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, mask as u32) }
const fn ppu_write_only(address: u32, mask: u16) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::WriteOnly, mask as u32) }
const fn ppu_word(address: u32) -> MmioRegister { MmioRegister::new(address, MmioWidth::Word, MmioAccess::ReadWrite, u32::MAX) }
const fn sound_control(address: u32, mask: u16) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, mask as u32) }
const fn sound_fifo(address: u32) -> MmioRegister { MmioRegister::new(address, MmioWidth::Word, MmioAccess::WriteOnly, SOUND_FIFO_MASK) }
const fn serial_control(address: u32, mask: u16) -> MmioRegister { MmioRegister::new(address, MmioWidth::Halfword, MmioAccess::ReadWrite, mask as u32) }

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
pub const BG0CNT: MmioRegister = ppu_control(0x0400_0008, BG_TEXT_CONTROL_MASK);
pub const BG1CNT: MmioRegister = ppu_control(0x0400_000a, BG_TEXT_CONTROL_MASK);
pub const BG2CNT: MmioRegister = ppu_control(0x0400_000c, BG_AFFINE_CONTROL_MASK);
pub const BG3CNT: MmioRegister = ppu_control(0x0400_000e, BG_AFFINE_CONTROL_MASK);
pub const BG0HOFS: MmioRegister = ppu_write_only(0x0400_0010, BG_SCROLL_MASK);
pub const BG0VOFS: MmioRegister = ppu_write_only(0x0400_0012, BG_SCROLL_MASK);
pub const BG1HOFS: MmioRegister = ppu_write_only(0x0400_0014, BG_SCROLL_MASK);
pub const BG1VOFS: MmioRegister = ppu_write_only(0x0400_0016, BG_SCROLL_MASK);
pub const BG2HOFS: MmioRegister = ppu_write_only(0x0400_0018, BG_SCROLL_MASK);
pub const BG2VOFS: MmioRegister = ppu_write_only(0x0400_001a, BG_SCROLL_MASK);
pub const BG3HOFS: MmioRegister = ppu_write_only(0x0400_001c, BG_SCROLL_MASK);
pub const BG3VOFS: MmioRegister = ppu_write_only(0x0400_001e, BG_SCROLL_MASK);
pub const BG2PA: MmioRegister = ppu_control(0x0400_0020, u16::MAX);
pub const BG2PB: MmioRegister = ppu_control(0x0400_0022, u16::MAX);
pub const BG2PC: MmioRegister = ppu_control(0x0400_0024, u16::MAX);
pub const BG2PD: MmioRegister = ppu_control(0x0400_0026, u16::MAX);
pub const BG2X: MmioRegister = ppu_word(0x0400_0028);
pub const BG2Y: MmioRegister = ppu_word(0x0400_002c);
pub const BG3PA: MmioRegister = ppu_control(0x0400_0030, u16::MAX);
pub const BG3PB: MmioRegister = ppu_control(0x0400_0032, u16::MAX);
pub const BG3PC: MmioRegister = ppu_control(0x0400_0034, u16::MAX);
pub const BG3PD: MmioRegister = ppu_control(0x0400_0036, u16::MAX);
pub const BG3X: MmioRegister = ppu_word(0x0400_0038);
pub const BG3Y: MmioRegister = ppu_word(0x0400_003c);
pub const WIN0H: MmioRegister = ppu_write_only(0x0400_0040, WINDOW_COORDINATE_MASK);
pub const WIN1H: MmioRegister = ppu_write_only(0x0400_0042, WINDOW_COORDINATE_MASK);
pub const WIN0V: MmioRegister = ppu_write_only(0x0400_0044, WINDOW_COORDINATE_MASK);
pub const WIN1V: MmioRegister = ppu_write_only(0x0400_0046, WINDOW_COORDINATE_MASK);
pub const WININ: MmioRegister = ppu_control(0x0400_0048, WINDOW_CONTROL_MASK);
pub const WINOUT: MmioRegister = ppu_control(0x0400_004a, WINDOW_CONTROL_MASK);
pub const MOSAIC: MmioRegister = ppu_write_only(0x0400_004c, MOSAIC_MASK);
pub const BLDCNT: MmioRegister = ppu_control(0x0400_0050, BLEND_CONTROL_MASK);
pub const BLDALPHA: MmioRegister = ppu_write_only(0x0400_0052, BLEND_ALPHA_MASK);
pub const BLDY: MmioRegister = ppu_write_only(0x0400_0054, BLEND_Y_MASK);

pub const SOUNDCNT_L: MmioRegister = sound_control(0x0400_0060, crate::mmio::SOUNDCNT_L_WRITABLE_MASK);
pub const SOUNDCNT_H: MmioRegister = sound_control(0x0400_0062, crate::mmio::SOUNDCNT_H_WRITABLE_MASK);
pub const SOUNDCNT_X: MmioRegister = sound_control(0x0400_0064, crate::mmio::SOUNDCNT_X_WRITABLE_MASK);
pub const SOUNDBIAS: MmioRegister = sound_control(0x0400_0088, crate::mmio::SOUNDBIAS_WRITABLE_MASK);
pub const FIFO_A: MmioRegister = sound_fifo(0x0400_00a0);
pub const FIFO_B: MmioRegister = sound_fifo(0x0400_00a4);
pub const SIOMULTI0: MmioRegister = MmioRegister::new(0x0400_0120, MmioWidth::Halfword, MmioAccess::ReadOnly, 0);
pub const SIOMULTI1: MmioRegister = MmioRegister::new(0x0400_0122, MmioWidth::Halfword, MmioAccess::ReadOnly, 0);
pub const SIOMULTI2: MmioRegister = MmioRegister::new(0x0400_0124, MmioWidth::Halfword, MmioAccess::ReadOnly, 0);
pub const SIOMULTI3: MmioRegister = MmioRegister::new(0x0400_0126, MmioWidth::Halfword, MmioAccess::ReadOnly, 0);
pub const SIOCNT: MmioRegister = serial_control(0x0400_0128, crate::mmio::SIOCNT_WRITABLE_MASK);
pub const SIODATA8: MmioRegister = MmioRegister::new(0x0400_012a, MmioWidth::Byte, MmioAccess::ReadWrite, 0xff);
pub const RCNT: MmioRegister = serial_control(0x0400_0134, crate::mmio::RCNT_WRITABLE_MASK);

#[inline]
pub const fn register(address: u32) -> Option<MmioRegister> {
    match address {
        0x0400_00b0..=0x0400_00b3 => Some(DMA0SAD), 0x0400_00b4..=0x0400_00b7 => Some(DMA0DAD),
        0x0400_00b8 | 0x0400_00b9 => Some(DMA0CNT_L), 0x0400_00ba | 0x0400_00bb => Some(DMA0CNT_H),
        0x0400_00bc..=0x0400_00bf => Some(DMA1SAD), 0x0400_00c0..=0x0400_00c3 => Some(DMA1DAD),
        0x0400_00c4 | 0x0400_00c5 => Some(DMA1CNT_L), 0x0400_00c6 | 0x0400_00c7 => Some(DMA1CNT_H),
        0x0400_00c8..=0x0400_00cb => Some(DMA2SAD), 0x0400_00cc..=0x0400_00cf => Some(DMA2DAD),
        0x0400_00d0 | 0x0400_00d1 => Some(DMA2CNT_L), 0x0400_00d2 | 0x0400_00d3 => Some(DMA2CNT_H),
        0x0400_00d4..=0x0400_00d7 => Some(DMA3SAD), 0x0400_00d8..=0x0400_00db => Some(DMA3DAD),
        0x0400_00dc | 0x0400_00dd => Some(DMA3CNT_L), 0x0400_00de | 0x0400_00df => Some(DMA3CNT_H),
        0x0400_0100 | 0x0400_0101 => Some(TIMER0CNT_L), 0x0400_0102 | 0x0400_0103 => Some(TIMER0CNT_H),
        0x0400_0104 | 0x0400_0105 => Some(TIMER1CNT_L), 0x0400_0106 | 0x0400_0107 => Some(TIMER1CNT_H),
        0x0400_0108 | 0x0400_0109 => Some(TIMER2CNT_L), 0x0400_010a | 0x0400_010b => Some(TIMER2CNT_H),
        0x0400_010c | 0x0400_010d => Some(TIMER3CNT_L), 0x0400_010e | 0x0400_010f => Some(TIMER3CNT_H),
        0x0400_0008 | 0x0400_0009 => Some(BG0CNT), 0x0400_000a | 0x0400_000b => Some(BG1CNT),
        0x0400_000c | 0x0400_000d => Some(BG2CNT), 0x0400_000e | 0x0400_000f => Some(BG3CNT),
        0x0400_0010 | 0x0400_0011 => Some(BG0HOFS), 0x0400_0012 | 0x0400_0013 => Some(BG0VOFS),
        0x0400_0014 | 0x0400_0015 => Some(BG1HOFS), 0x0400_0016 | 0x0400_0017 => Some(BG1VOFS),
        0x0400_0018 | 0x0400_0019 => Some(BG2HOFS), 0x0400_001a | 0x0400_001b => Some(BG2VOFS),
        0x0400_001c | 0x0400_001d => Some(BG3HOFS), 0x0400_001e | 0x0400_001f => Some(BG3VOFS),
        0x0400_0020 | 0x0400_0021 => Some(BG2PA), 0x0400_0022 | 0x0400_0023 => Some(BG2PB),
        0x0400_0024 | 0x0400_0025 => Some(BG2PC), 0x0400_0026 | 0x0400_0027 => Some(BG2PD),
        0x0400_0028..=0x0400_002b => Some(BG2X), 0x0400_002c..=0x0400_002f => Some(BG2Y),
        0x0400_0030 | 0x0400_0031 => Some(BG3PA), 0x0400_0032 | 0x0400_0033 => Some(BG3PB),
        0x0400_0034 | 0x0400_0035 => Some(BG3PC), 0x0400_0036 | 0x0400_0037 => Some(BG3PD),
        0x0400_0038..=0x0400_003b => Some(BG3X), 0x0400_003c..=0x0400_003f => Some(BG3Y),
        0x0400_0040 | 0x0400_0041 => Some(WIN0H), 0x0400_0042 | 0x0400_0043 => Some(WIN1H),
        0x0400_0044 | 0x0400_0045 => Some(WIN0V), 0x0400_0046 | 0x0400_0047 => Some(WIN1V),
        0x0400_0048 | 0x0400_0049 => Some(WININ), 0x0400_004a | 0x0400_004b => Some(WINOUT),
        0x0400_004c | 0x0400_004d => Some(MOSAIC), 0x0400_0050 | 0x0400_0051 => Some(BLDCNT),
        0x0400_0052 | 0x0400_0053 => Some(BLDALPHA), 0x0400_0054 | 0x0400_0055 => Some(BLDY),
        0x0400_0060 | 0x0400_0061 => Some(SOUNDCNT_L), 0x0400_0062 | 0x0400_0063 => Some(SOUNDCNT_H),
        0x0400_0064 | 0x0400_0065 => Some(SOUNDCNT_X), 0x0400_0088 | 0x0400_0089 => Some(SOUNDBIAS),
        0x0400_00a0..=0x0400_00a3 => Some(FIFO_A), 0x0400_00a4..=0x0400_00a7 => Some(FIFO_B),
        0x0400_0120 | 0x0400_0121 => Some(SIOMULTI0), 0x0400_0122 | 0x0400_0123 => Some(SIOMULTI1),
        0x0400_0124 | 0x0400_0125 => Some(SIOMULTI2), 0x0400_0126 | 0x0400_0127 => Some(SIOMULTI3),
        0x0400_0128 | 0x0400_0129 => Some(SIOCNT), 0x0400_012a => Some(SIODATA8),
        0x0400_0134 | 0x0400_0135 => Some(RCNT), _ => None,
    }
}
