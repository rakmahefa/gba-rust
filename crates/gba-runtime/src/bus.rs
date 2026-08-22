//! GBA CPU bus address contract.
//!
//! This module contains address-space classification and canonical offsets.
//! Device semantics stay in `Runtime` so the bus contract can be reused by DMA,
//! timers, the PPU/APU and future timing-aware devices without duplicating the
//! physical address map.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusRegion {
    Bios,
    Ewram,
    Iwram,
    Io,
    Palette,
    Vram,
    Oam,
    CartridgeRom,
    CartridgeSave,
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusAddress {
    pub region: BusRegion,
    pub offset: usize,
}

pub const BIOS_START: u32 = 0x0000_0000;
pub const BIOS_END: u32 = 0x0000_3fff;
pub const EWRAM_START: u32 = 0x0200_0000;
pub const EWRAM_END: u32 = 0x02ff_ffff;
pub const EWRAM_SIZE: usize = 0x0004_0000;
pub const IWRAM_START: u32 = 0x0300_0000;
pub const IWRAM_END: u32 = 0x03ff_ffff;
pub const IWRAM_SIZE: usize = 0x0000_8000;
pub const IO_START: u32 = 0x0400_0000;
pub const IO_END: u32 = 0x0400_03ff;
pub const PALETTE_START: u32 = 0x0500_0000;
pub const PALETTE_END: u32 = 0x05ff_ffff;
pub const PALETTE_SIZE: usize = 0x0000_0400;
pub const VRAM_START: u32 = 0x0600_0000;
pub const VRAM_END: u32 = 0x06ff_ffff;
pub const VRAM_SIZE: usize = 0x0001_8000;
pub const OAM_START: u32 = 0x0700_0000;
pub const OAM_END: u32 = 0x07ff_ffff;
pub const OAM_SIZE: usize = 0x0000_0400;
pub const ROM0_START: u32 = 0x0800_0000;
pub const ROM0_END: u32 = 0x09ff_ffff;
pub const ROM1_START: u32 = 0x0a00_0000;
pub const ROM1_END: u32 = 0x0bff_ffff;
pub const ROM2_START: u32 = 0x0c00_0000;
pub const ROM2_END: u32 = 0x0dff_ffff;
pub const SAVE_START: u32 = 0x0e00_0000;
pub const SAVE_END: u32 = 0x0fff_ffff;

#[inline]
fn vram_offset(address: u32) -> usize {
    let offset = (address - VRAM_START) as usize % 0x20_000;
    if offset < 0x18_000 {
        offset
    } else {
        offset - 0x8_000
    }
}

#[inline]
pub fn decode(address: u32) -> BusAddress {
    match address {
        BIOS_START..=BIOS_END => BusAddress { region: BusRegion::Bios, offset: address as usize },
        EWRAM_START..=EWRAM_END => BusAddress { region: BusRegion::Ewram, offset: ((address - EWRAM_START) as usize) % EWRAM_SIZE },
        IWRAM_START..=IWRAM_END => BusAddress { region: BusRegion::Iwram, offset: ((address - IWRAM_START) as usize) % IWRAM_SIZE },
        IO_START..=IO_END => BusAddress { region: BusRegion::Io, offset: (address - IO_START) as usize },
        PALETTE_START..=PALETTE_END => BusAddress { region: BusRegion::Palette, offset: ((address - PALETTE_START) as usize) % PALETTE_SIZE },
        VRAM_START..=VRAM_END => BusAddress { region: BusRegion::Vram, offset: vram_offset(address) },
        OAM_START..=OAM_END => BusAddress { region: BusRegion::Oam, offset: ((address - OAM_START) as usize) % OAM_SIZE },
        ROM0_START..=ROM2_END => BusAddress { region: BusRegion::CartridgeRom, offset: ((address - ROM0_START) & 0x01ff_ffff) as usize },
        SAVE_START..=SAVE_END => BusAddress { region: BusRegion::CartridgeSave, offset: ((address - SAVE_START) as usize) % 0x1_0000 },
        _ => BusAddress { region: BusRegion::Unmapped, offset: address as usize },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_primary_regions_and_rom_waitstate_aliases() {
        assert_eq!(decode(BIOS_START).region, BusRegion::Bios);
        assert_eq!(decode(EWRAM_START).region, BusRegion::Ewram);
        assert_eq!(decode(IWRAM_START).region, BusRegion::Iwram);
        assert_eq!(decode(IO_START).region, BusRegion::Io);
        assert_eq!(decode(PALETTE_START).region, BusRegion::Palette);
        assert_eq!(decode(VRAM_START).region, BusRegion::Vram);
        assert_eq!(decode(OAM_START).region, BusRegion::Oam);
        assert_eq!(decode(ROM0_START).region, BusRegion::CartridgeRom);
        assert_eq!(decode(ROM1_START).region, BusRegion::CartridgeRom);
        assert_eq!(decode(ROM2_START).region, BusRegion::CartridgeRom);
        assert_eq!(decode(SAVE_START).region, BusRegion::CartridgeSave);
        assert_eq!(decode(0x0100_0000).region, BusRegion::Unmapped);
    }

    #[test]
    fn canonicalizes_internal_memory_mirrors() {
        assert_eq!(decode(EWRAM_START + EWRAM_SIZE as u32).offset, 0);
        assert_eq!(decode(IWRAM_START + IWRAM_SIZE as u32).offset, 0);
        assert_eq!(decode(PALETTE_START + PALETTE_SIZE as u32).offset, 0);
        assert_eq!(decode(OAM_START + OAM_SIZE as u32).offset, 0);
        assert_eq!(decode(VRAM_START + 0x18_000).offset, 0x10_000);
        assert_eq!(decode(VRAM_START + 0x20_000).offset, 0);
    }

    #[test]
    fn aliases_rom_waitstates_and_mirrors_save() {
        assert_eq!(decode(ROM0_START + 0x123456).offset, 0x123456);
        assert_eq!(decode(ROM1_START + 0x123456).offset, 0x123456);
        assert_eq!(decode(ROM2_START + 0x123456).offset, 0x123456);
        assert_eq!(decode(SAVE_START + 0x1_0000).offset, 0);
        assert_eq!(decode(0x0f00_0000).offset, 0);
    }
}
