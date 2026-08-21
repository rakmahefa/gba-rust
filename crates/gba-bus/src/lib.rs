use thiserror::Error;

pub const BIOS_START: u32 = 0x0000_0000;
pub const BIOS_END: u32 = 0x0000_3fff;
pub const EWRAM_START: u32 = 0x0200_0000;
pub const EWRAM_END: u32 = 0x0203_ffff;
pub const IWRAM_START: u32 = 0x0300_0000;
pub const IWRAM_END: u32 = 0x0300_7fff;
pub const IO_START: u32 = 0x0400_0000;
pub const IO_END: u32 = 0x0400_03ff;
pub const PALETTE_START: u32 = 0x0500_0000;
pub const PALETTE_END: u32 = 0x0500_03ff;
pub const VRAM_START: u32 = 0x0600_0000;
pub const VRAM_END: u32 = 0x0601_7fff;
pub const OAM_START: u32 = 0x0700_0000;
pub const OAM_END: u32 = 0x0700_03ff;
pub const ROM0_START: u32 = 0x0800_0000;
pub const ROM0_END: u32 = 0x09ff_ffff;
pub const SRAM_START: u32 = 0x0e00_0000;
pub const SRAM_END: u32 = 0x0fff_ffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Bios,
    Ewram,
    Iwram,
    Io,
    Palette,
    Vram,
    Oam,
    Rom,
    Save,
    Unmapped,
}

impl Region {
    pub const fn contains(self, address: u32) -> bool {
        match self {
            Self::Bios => (BIOS_START..=BIOS_END).contains(&address),
            Self::Ewram => (EWRAM_START..=EWRAM_END).contains(&address),
            Self::Iwram => (IWRAM_START..=IWRAM_END).contains(&address),
            Self::Io => (IO_START..=IO_END).contains(&address),
            Self::Palette => (PALETTE_START..=PALETTE_END).contains(&address),
            Self::Vram => (VRAM_START..=VRAM_END).contains(&address),
            Self::Oam => (OAM_START..=OAM_END).contains(&address),
            Self::Rom => (ROM0_START..=ROM0_END).contains(&address),
            Self::Save => (SRAM_START..=SRAM_END).contains(&address),
            Self::Unmapped => true,
        }
    }

    pub const fn base(self) -> Option<u32> {
        match self {
            Self::Bios => Some(BIOS_START),
            Self::Ewram => Some(EWRAM_START),
            Self::Iwram => Some(IWRAM_START),
            Self::Io => Some(IO_START),
            Self::Palette => Some(PALETTE_START),
            Self::Vram => Some(VRAM_START),
            Self::Oam => Some(OAM_START),
            Self::Rom => Some(ROM0_START),
            Self::Save => Some(SRAM_START),
            Self::Unmapped => None,
        }
    }
}

pub const fn classify(address: u32) -> Region {
    if Region::Bios.contains(address) { Region::Bios }
    else if Region::Ewram.contains(address) { Region::Ewram }
    else if Region::Iwram.contains(address) { Region::Iwram }
    else if Region::Io.contains(address) { Region::Io }
    else if Region::Palette.contains(address) { Region::Palette }
    else if Region::Vram.contains(address) { Region::Vram }
    else if Region::Oam.contains(address) { Region::Oam }
    else if Region::Rom.contains(address) { Region::Rom }
    else if Region::Save.contains(address) { Region::Save }
    else { Region::Unmapped }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessWidth {
    Byte = 1,
    Halfword = 2,
    Word = 4,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    #[error("unmapped address {address:#010x}")]
    Unmapped { address: u32 },
    #[error("access {width:?} at {address:#010x} crosses a region boundary")]
    CrossesRegion { address: u32, width: AccessWidth },
}

pub trait BusDevice {
    fn read8(&mut self, offset: usize) -> u8;
    fn write8(&mut self, offset: usize, value: u8);
}

#[derive(Debug, Clone)]
pub struct GbaBus {
    pub bios: Vec<u8>,
    pub ewram: Vec<u8>,
    pub iwram: Vec<u8>,
    pub io: Vec<u8>,
    pub palette: Vec<u8>,
    pub vram: Vec<u8>,
    pub oam: Vec<u8>,
    pub rom: Vec<u8>,
    pub save: Vec<u8>,
}

impl Default for GbaBus {
    fn default() -> Self {
        Self::new(Vec::new(), 0x20000, 0x8000, 0x400, 0x400, 0x18000, 0x400, Vec::new(), 0x8000)
    }
}

impl GbaBus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bios: Vec<u8>,
        ewram: usize,
        iwram: usize,
        io: usize,
        palette: usize,
        vram: usize,
        oam: usize,
        rom: Vec<u8>,
        save: usize,
    ) -> Self {
        Self {
            bios,
            ewram: vec![0; ewram],
            iwram: vec![0; iwram],
            io: vec![0; io],
            palette: vec![0; palette],
            vram: vec![0; vram],
            oam: vec![0; oam],
            rom,
            save: vec![0; save],
        }
    }

    pub fn read8(&self, address: u32) -> Result<u8, BusError> {
        let (region, offset) = self.resolve(address, AccessWidth::Byte)?;
        Ok(match region {
            Region::Bios => self.read_mem(&self.bios, offset),
            Region::Ewram => self.read_mem(&self.ewram, offset & 0x1ffff),
            Region::Iwram => self.read_mem(&self.iwram, offset & 0x7fff),
            Region::Io => self.read_mem(&self.io, offset & 0x3ff),
            Region::Palette => self.read_mem(&self.palette, offset & 0x3ff),
            Region::Vram => self.read_mem(&self.vram, offset & 0x17fff),
            Region::Oam => self.read_mem(&self.oam, offset & 0x3ff),
            Region::Rom => self.read_mem(&self.rom, offset % self.rom.len().max(1)),
            Region::Save => self.read_mem(&self.save, offset % self.save.len().max(1)),
            Region::Unmapped => return Err(BusError::Unmapped { address }),
        })
    }

    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        let lo = self.read8(address)?;
        let hi = self.read8(address.wrapping_add(1))?;
        Ok(u16::from_le_bytes([lo, hi]))
    }

    pub fn read32(&self, address: u32) -> Result<u32, BusError> {
        let aligned = address & !3;
        let raw = u32::from_le_bytes([
            self.read8(aligned)?,
            self.read8(aligned + 1)?,
            self.read8(aligned + 2)?,
            self.read8(aligned + 3)?,
        ]);
        Ok(raw.rotate_right((address & 3) * 8))
    }

    pub fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        let (region, offset) = self.resolve(address, AccessWidth::Byte)?;
        match region {
            Region::Bios | Region::Rom => {}
            Region::Ewram => self.write_mem(&mut self.ewram, offset & 0x1ffff, value),
            Region::Iwram => self.write_mem(&mut self.iwram, offset & 0x7fff, value),
            Region::Io => self.write_mem(&mut self.io, offset & 0x3ff, value),
            Region::Palette => self.write_mem(&mut self.palette, offset & 0x3ff, value),
            Region::Vram => self.write_mem(&mut self.vram, offset & 0x17fff, value),
            Region::Oam => self.write_mem(&mut self.oam, offset & 0x3ff, value),
            Region::Save => self.write_mem(&mut self.save, offset % self.save.len().max(1), value),
            Region::Unmapped => return Err(BusError::Unmapped { address }),
        }
        Ok(())
    }

    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        let bytes = value.to_le_bytes();
        self.write8(address, bytes[0])?;
        self.write8(address.wrapping_add(1), bytes[1])?;
        Ok(())
    }

    pub fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte)?;
        }
        Ok(())
    }

    fn resolve(&self, address: u32, width: AccessWidth) -> Result<(Region, usize), BusError> {
        let region = classify(address);
        if region == Region::Unmapped {
            return Err(BusError::Unmapped { address });
        }
        let last = address.saturating_add(width as u32 - 1);
        if classify(last) != region {
            return Err(BusError::CrossesRegion { address, width });
        }
        let base = region.base().expect("mapped regions always have a base");
        Ok((region, (address - base) as usize))
    }

    fn read_mem(&self, data: &[u8], offset: usize) -> u8 {
        data.get(offset).copied().unwrap_or(0)
    }

    fn write_mem(&mut self, data: &mut [u8], offset: usize, value: u8) {
        if let Some(slot) = data.get_mut(offset) {
            *slot = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_core_gba_regions() {
        assert_eq!(classify(0x0200_0000), Region::Ewram);
        assert_eq!(classify(0x0300_0000), Region::Iwram);
        assert_eq!(classify(0x0400_0000), Region::Io);
        assert_eq!(classify(0x0600_0000), Region::Vram);
        assert_eq!(classify(0x0800_0000), Region::Rom);
        assert_eq!(classify(0x0e00_0000), Region::Save);
    }

    #[test]
    fn preserves_little_endian_and_unaligned_word_rotation() {
        let mut bus = GbaBus::default();
        bus.write32(0x0200_0000, 0x1122_3344).unwrap();
        assert_eq!(bus.read16(0x0200_0000).unwrap(), 0x3344);
        assert_eq!(bus.read32(0x0200_0001).unwrap(), 0x4411_2233);
    }

    #[test]
    fn rejects_cross_region_access() {
        let bus = GbaBus::default();
        assert_eq!(
            bus.read16(EWRAM_END).unwrap_err(),
            BusError::CrossesRegion { address: EWRAM_END, width: AccessWidth::Halfword }
        );
    }
}
