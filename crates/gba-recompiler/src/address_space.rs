use crate::decoder::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressRegion {
    Bios,
    Ewram,
    Iwram,
    Io,
    PaletteRam,
    Vram,
    Oam,
    CartridgeRom,
    CartridgeRam,
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AddressRange {
    pub start: u32,
    pub end: u32,
}

impl AddressRange {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub const fn contains(self, address: u32) -> bool {
        address >= self.start && address < self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AddressSpaceRegion {
    pub region: AddressRegion,
    pub range: AddressRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageKind {
    Bios,
    CartridgeRom,
}

impl ImageKind {
    pub const fn region(self) -> AddressRegion {
        match self {
            Self::Bios => AddressRegion::Bios,
            Self::CartridgeRom => AddressRegion::CartridgeRom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageMapping {
    pub kind: ImageKind,
    pub base: u32,
    pub size: u32,
    pub entry: u32,
    pub entry_mode: Mode,
}

impl ImageMapping {
    pub const fn new(
        kind: ImageKind,
        base: u32,
        size: u32,
        entry: u32,
        entry_mode: Mode,
    ) -> Self {
        Self {
            kind,
            base,
            size,
            entry,
            entry_mode,
        }
    }

    pub const fn range(self) -> AddressRange {
        AddressRange::new(self.base, self.base.saturating_add(self.size))
    }

    pub const fn contains(self, address: u32) -> bool {
        self.range().contains(address)
    }

    pub const fn bios(size: u32) -> Self {
        Self::new(ImageKind::Bios, 0x0000_0000, size, 0x0000_0000, Mode::Arm)
    }

    pub const fn cartridge_rom(size: u32) -> Self {
        Self::new(
            ImageKind::CartridgeRom,
            0x0800_0000,
            size,
            0x0800_0000,
            Mode::Arm,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressSpace {
    regions: Vec<AddressSpaceRegion>,
}

impl Default for AddressSpace {
    fn default() -> Self {
        Self::gba()
    }
}

impl AddressSpace {
    pub fn gba() -> Self {
        Self {
            regions: vec![
                AddressSpaceRegion {
                    region: AddressRegion::Bios,
                    range: AddressRange::new(0x0000_0000, 0x0000_4000),
                },
                AddressSpaceRegion {
                    region: AddressRegion::Ewram,
                    range: AddressRange::new(0x0200_0000, 0x0204_0000),
                },
                AddressSpaceRegion {
                    region: AddressRegion::Iwram,
                    range: AddressRange::new(0x0300_0000, 0x0300_8000),
                },
                AddressSpaceRegion {
                    region: AddressRegion::Io,
                    range: AddressRange::new(0x0400_0000, 0x0400_0400),
                },
                AddressSpaceRegion {
                    region: AddressRegion::PaletteRam,
                    range: AddressRange::new(0x0500_0000, 0x0500_0400),
                },
                AddressSpaceRegion {
                    region: AddressRegion::Vram,
                    range: AddressRange::new(0x0600_0000, 0x0601_8000),
                },
                AddressSpaceRegion {
                    region: AddressRegion::Oam,
                    range: AddressRange::new(0x0700_0000, 0x0700_0400),
                },
                AddressSpaceRegion {
                    region: AddressRegion::CartridgeRom,
                    range: AddressRange::new(0x0800_0000, 0x0E00_0000),
                },
                AddressSpaceRegion {
                    region: AddressRegion::CartridgeRam,
                    range: AddressRange::new(0x0E00_0000, 0x1000_0000),
                },
            ],
        }
    }

    pub fn regions(&self) -> &[AddressSpaceRegion] {
        &self.regions
    }

    pub fn region_at(&self, address: u32) -> AddressRegion {
        self.regions
            .iter()
            .find(|region| region.range.contains(address))
            .map(|region| region.region)
            .unwrap_or(AddressRegion::Unmapped)
    }

    pub fn map_image(&mut self, mapping: ImageMapping) -> Result<(), MappingError> {
        let range = mapping.range();
        if range.start >= range.end {
            return Err(MappingError::EmptyImage);
        }

        if let Some(existing) = self.regions.iter().find(|region| {
            region.region != AddressRegion::Unmapped
                && region.range.start < range.end
                && range.start < region.range.end
        }) {
            return Err(MappingError::OverlapsRegion {
                image: mapping.kind,
                region: existing.region,
            });
        }

        self.regions.push(AddressSpaceRegion {
            region: mapping.kind.region(),
            range,
        });
        self.regions
            .sort_by_key(|region| region.range.start);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingError {
    EmptyImage,
    OverlapsRegion {
        image: ImageKind,
        region: AddressRegion,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gba_address_space_classifies_core_regions() {
        let space = AddressSpace::gba();
        assert_eq!(space.region_at(0x0000_0000), AddressRegion::Bios);
        assert_eq!(space.region_at(0x0200_0000), AddressRegion::Ewram);
        assert_eq!(space.region_at(0x0300_0000), AddressRegion::Iwram);
        assert_eq!(space.region_at(0x0400_0000), AddressRegion::Io);
        assert_eq!(space.region_at(0x0800_0000), AddressRegion::CartridgeRom);
        assert_eq!(space.region_at(0x0E00_0000), AddressRegion::CartridgeRam);
        assert_eq!(space.region_at(0x0100_0000), AddressRegion::Unmapped);
    }

    #[test]
    fn image_mapping_tracks_base_and_entry() {
        let mapping = ImageMapping::bios(0x4000);
        assert_eq!(mapping.range(), AddressRange::new(0, 0x4000));
        assert!(mapping.contains(0x3FFC));
        assert!(!mapping.contains(0x4000));
        assert_eq!(mapping.entry_mode, Mode::Arm);
    }

    #[test]
    fn mapping_image_rejects_overlap() {
        let mut space = AddressSpace {
            regions: Vec::new(),
        };
        space
            .map_image(ImageMapping::new(
                ImageKind::Bios,
                0x0000_0000,
                0x4000,
                0,
                Mode::Arm,
            ))
            .unwrap();
        let error = space
            .map_image(ImageMapping::new(
                ImageKind::CartridgeRom,
                0x0000_2000,
                0x4000,
                0x2000,
                Mode::Arm,
            ))
            .unwrap_err();
        assert_eq!(
            error,
            MappingError::OverlapsRegion {
                image: ImageKind::CartridgeRom,
                region: AddressRegion::Bios,
            }
        );
    }
}
