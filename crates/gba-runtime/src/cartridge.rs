use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

const GBA_HEADER_MIN_SIZE: usize = 0xc0;
const GBA_HEADER_FIXED_BYTE: u8 = 0x96;
const GBA_CARTRIDGE_BASE: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveType {
    None,
    Sram32K,
    Flash64K,
    Flash128K,
    Eeprom512B,
    Eeprom8K,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CartridgeHeaderError {
    #[error("ROM is too small for a GBA cartridge header: {0} bytes < 0xc0")]
    TooSmall(usize),
    #[error("invalid GBA cartridge header fixed byte at 0xb2: expected 0x96, got {0:#04x}")]
    InvalidFixedByte(u8),
    #[error("invalid GBA cartridge header checksum: stored={stored:#04x}, expected={expected:#04x}")]
    InvalidChecksum { stored: u8, expected: u8 },
    #[error("GBA cartridge entry at 0x08000000 is not an ARM B/BL instruction: {0:#010x}")]
    InvalidEntryOpcode(u32),
    #[error("GBA cartridge entry target {target:#010x} lies outside ROM image of {size:#x} bytes")]
    EntryOutsideRom { target: u32, size: usize },
    #[error("GBA cartridge game code contains invalid bytes")]
    InvalidGameCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeHeader {
    pub entry_target: u32,
    pub title: String,
    pub game_code: String,
    pub maker_code: String,
    pub version: u8,
    pub checksum: u8,
}

#[derive(Debug, Clone)]
pub struct SaveRam {
    kind: SaveType,
    data: Vec<u8>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl SaveRam {
    pub fn new(kind: SaveType, path: Option<PathBuf>) -> Self {
        let len = match kind {
            SaveType::Sram32K => 0x8000,
            SaveType::Flash64K => 0x10000,
            SaveType::Flash128K => 0x20000,
            SaveType::Eeprom512B => 512,
            SaveType::Eeprom8K => 8192,
            SaveType::None => 0,
        };
        let mut data = vec![0xff; len];
        if let Some(p) = &path {
            if let Ok(existing) = fs::read(p) {
                if existing.len() == len {
                    data = existing;
                }
            }
        }
        Self {
            kind,
            data,
            path,
            dirty: false,
        }
    }

    pub fn kind(&self) -> SaveType {
        self.kind
    }

    pub fn read(&self, addr: usize) -> u8 {
        self.data
            .get(addr % self.data.len().max(1))
            .copied()
            .unwrap_or(0xff)
    }

    pub fn write(&mut self, addr: usize, value: u8) {
        if !self.data.is_empty() {
            let i = addr % self.data.len();
            if self.data[i] != value {
                self.data[i] = value;
                self.dirty = true;
            }
        }
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let Some(path) = &self.path else {
            return Ok(());
        };
        let tmp = path.with_extension("sav.tmp");
        fs::write(&tmp, &self.data)?;
        if path.exists() {
            let backup = path.with_extension("sav.bak");
            let _ = fs::copy(path, backup);
        }
        fs::rename(tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}

impl Drop for SaveRam {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[derive(Debug, Clone)]
pub struct Cartridge {
    pub rom: Vec<u8>,
    pub save: SaveRam,
}

impl Cartridge {
    pub fn from_rom(rom: Vec<u8>, save_dir: impl AsRef<Path>) -> Self {
        let title = rom.get(0xa0..0xac).unwrap_or_default();
        let stem = String::from_utf8_lossy(title)
            .trim_matches('\0')
            .trim()
            .to_string();
        let kind = detect_save_type(&rom);
        let path = if kind == SaveType::None {
            None
        } else {
            Some(save_dir.as_ref().join(format!(
                "{}.sav",
                if stem.is_empty() { "game" } else { &stem }
            )))
        };
        Self {
            rom,
            save: SaveRam::new(kind, path),
        }
    }

    pub fn validate_header(&self) -> Result<CartridgeHeader, CartridgeHeaderError> {
        validate_header(&self.rom)
    }
}

pub fn validate_header(rom: &[u8]) -> Result<CartridgeHeader, CartridgeHeaderError> {
    if rom.len() < GBA_HEADER_MIN_SIZE {
        return Err(CartridgeHeaderError::TooSmall(rom.len()));
    }

    if rom[0xb2] != GBA_HEADER_FIXED_BYTE {
        return Err(CartridgeHeaderError::InvalidFixedByte(rom[0xb2]));
    }

    let expected_checksum = (0u8).wrapping_sub(
        0x19u8.wrapping_add(rom[0xa0..=0xbc].iter().copied().fold(0u8, u8::wrapping_add)),
    );
    if rom[0xbd] != expected_checksum {
        return Err(CartridgeHeaderError::InvalidChecksum {
            stored: rom[0xbd],
            expected: expected_checksum,
        });
    }

    let entry_opcode = u32::from_le_bytes([rom[0], rom[1], rom[2], rom[3]]);
    if (entry_opcode & 0x0e00_0000) != 0x0a00_0000 {
        return Err(CartridgeHeaderError::InvalidEntryOpcode(entry_opcode));
    }
    let offset = ((entry_opcode & 0x00ff_ffff) << 2) as i32;
    let offset = (offset << 6) >> 6;
    let entry_target = GBA_CARTRIDGE_BASE
        .wrapping_add(8)
        .wrapping_add(offset as u32);
    if entry_target < GBA_CARTRIDGE_BASE
        || (entry_target - GBA_CARTRIDGE_BASE) as usize >= rom.len()
    {
        return Err(CartridgeHeaderError::EntryOutsideRom {
            target: entry_target,
            size: rom.len(),
        });
    }

    let game_code = String::from_utf8_lossy(&rom[0xac..0xb0]).to_string();
    if !game_code
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(CartridgeHeaderError::InvalidGameCode);
    }

    Ok(CartridgeHeader {
        entry_target,
        title: String::from_utf8_lossy(&rom[0xa0..0xac])
            .trim_end_matches('\0')
            .trim()
            .to_string(),
        game_code,
        maker_code: String::from_utf8_lossy(&rom[0xb0..0xb2]).to_string(),
        version: rom[0xbc],
        checksum: rom[0xbd],
    })
}

pub fn detect_save_type(rom: &[u8]) -> SaveType {
    let text = String::from_utf8_lossy(rom);
    if text.contains("EEPROM_V") {
        if text.contains("8K") {
            SaveType::Eeprom8K
        } else {
            SaveType::Eeprom512B
        }
    } else if text.contains("FLASH1M_V") {
        SaveType::Flash128K
    } else if text.contains("FLASH_V") {
        SaveType::Flash64K
    } else if text.contains("SRAM_V") {
        SaveType::Sram32K
    } else {
        SaveType::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x200];
        rom[0..4].copy_from_slice(&0xea00_002eu32.to_le_bytes());
        rom[0xa0..0xac].copy_from_slice(b"TESTROM1234\0");
        rom[0xac..0xb0].copy_from_slice(b"TRST");
        rom[0xb0..0xb2].copy_from_slice(b"01");
        rom[0xb2] = GBA_HEADER_FIXED_BYTE;
        rom[0xbc] = 0;
        rom[0xbd] = (0u8).wrapping_sub(
            0x19u8.wrapping_add(rom[0xa0..=0xbc].iter().copied().fold(0u8, u8::wrapping_add)),
        );
        rom
    }

    #[test]
    fn validates_real_gba_boot_header() {
        let header = validate_header(&valid_rom()).expect("valid GBA header");
        assert_eq!(header.entry_target, 0x0800_00c0);
        assert_eq!(header.title, "TESTROM1234");
        assert_eq!(header.game_code, "TRST");
        assert_eq!(header.maker_code, "01");
        assert_eq!(header.version, 0);
    }

    #[test]
    fn rejects_rom_without_branch_entry() {
        let mut rom = valid_rom();
        rom[0..4].copy_from_slice(&0xe1a0_0000u32.to_le_bytes());
        assert!(matches!(
            validate_header(&rom),
            Err(CartridgeHeaderError::InvalidEntryOpcode(_))
        ));
    }

    #[test]
    fn rejects_bad_header_checksum() {
        let mut rom = valid_rom();
        rom[0xbd] ^= 0xff;
        assert!(matches!(
            validate_header(&rom),
            Err(CartridgeHeaderError::InvalidChecksum { .. })
        ));
    }
}
