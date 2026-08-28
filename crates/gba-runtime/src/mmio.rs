//! Architectural GBA memory-mapped I/O register definitions.
//!
//! This module owns the register contract shared by the CPU bus and device
//! models. Each register has an explicit width, access policy and writable
//! mask so device state cannot accidentally inherit generic byte-array
//! semantics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioWidth { Byte, Halfword, Word }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioAccess { ReadOnly, ReadWrite, WriteOnly }
impl MmioAccess { #[inline] pub const fn can_read(self) -> bool { matches!(self, Self::ReadOnly | Self::ReadWrite) } #[inline] pub const fn can_write(self) -> bool { matches!(self, Self::ReadWrite | Self::WriteOnly) } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioRegister { pub address: u32, pub width: MmioWidth, pub access: MmioAccess, pub writable_mask: u32 }
impl MmioRegister {
    pub const fn new(address: u32, width: MmioWidth, access: MmioAccess, writable_mask: u32) -> Self { Self { address, width, access, writable_mask } }
    #[inline] pub const fn allows_bus_width(self, width: MmioWidth) -> bool { match self.width { MmioWidth::Byte => matches!(width, MmioWidth::Byte), MmioWidth::Halfword => matches!(width, MmioWidth::Byte | MmioWidth::Halfword), MmioWidth::Word => true } }
    #[inline] pub const fn writable_byte_mask(self, address: u32) -> u8 { let offset = address.wrapping_sub(self.address); if offset >= 4 { return 0; } ((self.writable_mask >> (offset * 8)) & 0xff) as u8 }
}

pub const DISPCNT: u32 = 0x0400_0000;
pub const DISPSTAT: u32 = 0x0400_0004;
pub const VCOUNT: u32 = 0x0400_0006;
pub const SOUNDCNT_L: u32 = 0x0400_0060;
pub const SOUNDCNT_H: u32 = 0x0400_0062;
pub const SOUNDCNT_X: u32 = 0x0400_0064;
pub const SOUNDBIAS: u32 = 0x0400_0088;
pub const FIFO_A: u32 = 0x0400_00a0;
pub const FIFO_B: u32 = 0x0400_00a4;
pub const SIOMULTI0: u32 = 0x0400_0120;
pub const SIOMULTI1: u32 = 0x0400_0122;
pub const SIOMULTI2: u32 = 0x0400_0124;
pub const SIOMULTI3: u32 = 0x0400_0126;
pub const SIOCNT: u32 = 0x0400_0128;
pub const SIODATA8: u32 = 0x0400_012a;
pub const KEYINPUT: u32 = 0x0400_0130;
pub const KEYCNT: u32 = 0x0400_0132;
pub const RCNT: u32 = 0x0400_0134;
pub const IE: u32 = 0x0400_0200;
pub const IF: u32 = 0x0400_0202;
pub const WAITCNT: u32 = 0x0400_0204;
pub const IME: u32 = 0x0400_0208;
pub const POSTFLG: u32 = 0x0400_0300;
pub const HALTCNT: u32 = 0x0400_0301;
pub const DISPCNT_HI: u32 = DISPCNT + 1;
pub const DISPSTAT_HI: u32 = DISPSTAT + 1;
pub const VCOUNT_HI: u32 = VCOUNT + 1;
pub const SOUNDCNT_L_HI: u32 = SOUNDCNT_L + 1;
pub const SOUNDCNT_H_HI: u32 = SOUNDCNT_H + 1;
pub const SOUNDCNT_X_HI: u32 = SOUNDCNT_X + 1;
pub const SOUNDBIAS_HI: u32 = SOUNDBIAS + 1;
pub const KEYINPUT_HI: u32 = KEYINPUT + 1;
pub const KEYCNT_HI: u32 = KEYCNT + 1;
pub const SIOCNT_HI: u32 = SIOCNT + 1;
pub const RCNT_HI: u32 = RCNT + 1;
pub const IE_HI: u32 = IE + 1;
pub const IF_HI: u32 = IF + 1;
pub const WAITCNT_HI: u32 = WAITCNT + 1;
pub const IME_HI: u32 = IME + 1;
pub const DISPCNT_WRITABLE_MASK: u16 = 0xfff7;
pub const DISPSTAT_WRITABLE_MASK: u16 = 0xff38;
pub const INTERRUPT_SOURCE_MASK: u16 = 0x3fff;
pub const WAITCNT_WRITABLE_MASK: u16 = 0x5fff;
pub const IME_WRITABLE_MASK: u16 = 0x0001;
pub const POSTFLG_WRITABLE_MASK: u8 = 0x01;
pub const KEYCNT_WRITABLE_MASK: u16 = 0xc3ff;
pub const KEYCNT_KEY_MASK: u16 = 0x03ff;
pub const KEYCNT_IRQ_ENABLE: u16 = 1 << 14;
pub const KEYCNT_AND: u16 = 1 << 15;
pub const SOUNDCNT_L_WRITABLE_MASK: u16 = 0x7777;
pub const SOUNDCNT_H_WRITABLE_MASK: u16 = 0xff0f;
pub const SOUNDCNT_X_WRITABLE_MASK: u16 = 0x0080;
pub const SOUNDBIAS_WRITABLE_MASK: u16 = 0xc3ff;
pub const SIOCNT_WRITABLE_MASK: u16 = 0xf1ff;
pub const RCNT_WRITABLE_MASK: u16 = 0x800f;
pub const DISPSTAT_VBLANK: u16 = 1 << 0;
pub const DISPSTAT_HBLANK: u16 = 1 << 1;
pub const DISPSTAT_VCOUNT: u16 = 1 << 2;
pub const DISPSTAT_VBLANK_IRQ: u16 = 1 << 3;
pub const DISPSTAT_HBLANK_IRQ: u16 = 1 << 4;
pub const DISPSTAT_VCOUNT_IRQ: u16 = 1 << 5;
pub const DISPSTAT_VCOUNT_MASK: u16 = 0xff << 8;
pub const DISPSTAT_STATUS_MASK: u16 = DISPSTAT_VBLANK | DISPSTAT_HBLANK | DISPSTAT_VCOUNT;
pub const DISPCNT_REGISTER: MmioRegister = MmioRegister::new(DISPCNT, MmioWidth::Halfword, MmioAccess::ReadWrite, DISPCNT_WRITABLE_MASK as u32);
pub const DISPSTAT_REGISTER: MmioRegister = MmioRegister::new(DISPSTAT, MmioWidth::Halfword, MmioAccess::ReadWrite, DISPSTAT_WRITABLE_MASK as u32);
pub const VCOUNT_REGISTER: MmioRegister = MmioRegister::new(VCOUNT, MmioWidth::Halfword, MmioAccess::ReadOnly, 0);
pub const KEYINPUT_REGISTER: MmioRegister = MmioRegister::new(KEYINPUT, MmioWidth::Halfword, MmioAccess::ReadOnly, 0);
pub const KEYCNT_REGISTER: MmioRegister = MmioRegister::new(KEYCNT, MmioWidth::Halfword, MmioAccess::ReadWrite, KEYCNT_WRITABLE_MASK as u32);
pub const IE_REGISTER: MmioRegister = MmioRegister::new(IE, MmioWidth::Halfword, MmioAccess::ReadWrite, INTERRUPT_SOURCE_MASK as u32);
pub const IF_REGISTER: MmioRegister = MmioRegister::new(IF, MmioWidth::Halfword, MmioAccess::ReadWrite, INTERRUPT_SOURCE_MASK as u32);
pub const WAITCNT_REGISTER: MmioRegister = MmioRegister::new(WAITCNT, MmioWidth::Halfword, MmioAccess::ReadWrite, WAITCNT_WRITABLE_MASK as u32);
pub const IME_REGISTER: MmioRegister = MmioRegister::new(IME, MmioWidth::Halfword, MmioAccess::ReadWrite, IME_WRITABLE_MASK as u32);
pub const POSTFLG_REGISTER: MmioRegister = MmioRegister::new(POSTFLG, MmioWidth::Byte, MmioAccess::ReadWrite, POSTFLG_WRITABLE_MASK as u32);
pub const HALTCNT_REGISTER: MmioRegister = MmioRegister::new(HALTCNT, MmioWidth::Byte, MmioAccess::WriteOnly, 0xff);
#[inline] pub const fn register(address: u32) -> Option<MmioRegister> { match address { DISPCNT | DISPCNT_HI => Some(DISPCNT_REGISTER), DISPSTAT | DISPSTAT_HI => Some(DISPSTAT_REGISTER), VCOUNT | VCOUNT_HI => Some(VCOUNT_REGISTER), KEYINPUT | KEYINPUT_HI => Some(KEYINPUT_REGISTER), KEYCNT | KEYCNT_HI => Some(KEYCNT_REGISTER), IE | IE_HI => Some(IE_REGISTER), IF | IF_HI => Some(IF_REGISTER), WAITCNT | WAITCNT_HI => Some(WAITCNT_REGISTER), IME | IME_HI => Some(IME_REGISTER), POSTFLG => Some(POSTFLG_REGISTER), HALTCNT => Some(HALTCNT_REGISTER), _ => None } }
#[inline] pub const fn dispstat_vcount(value: u16) -> u8 { ((value >> 8) & 0xff) as u8 }
#[inline] pub const fn with_dispstat_vcount(value: u16, compare: u8) -> u16 { (value & !DISPSTAT_VCOUNT_MASK) | ((compare as u16) << 8) }
