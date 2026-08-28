use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

const GBA_HEADER_MIN_SIZE: usize = 0xc0;
const GBA_HEADER_FIXED_BYTE: u8 = 0x96;
const GBA_CARTRIDGE_BASE: u32 = 0x0800_0000;
const FLASH_SECTOR_SIZE: usize = 0x1000;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FlashCommandState {
    #[default]
    Ready,
    Unlock1,
    Unlock2,
    Program,
    EraseUnlock1,
    EraseUnlock2,
    EraseCommand,
    BankSwitch,
}

#[derive(Debug, Clone)]
pub struct SaveRam {
    kind: SaveType,
    data: Vec<u8>,
    path: Option<PathBuf>,
    dirty: bool,
    flash_state: FlashCommandState,
    flash_bank: usize,
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
            flash_state: FlashCommandState::Ready,
            flash_bank: 0,
        }
    }

    pub fn kind(&self) -> SaveType {
        self.kind
    }

    pub fn read(&self, addr: usize) -> u8 {
        if self.data.is_empty() {
            return 0xff;
        }
        let index = match self.kind {
            SaveType::Flash128K => self.flash_bank * 0x10000 + (addr & 0xffff),
            _ => addr % self.data.len(),
        };
        self.data.get(index).copied().unwrap_or(0xff)
    }

    pub fn write(&mut self, addr: usize, value: u8) {
        match self.kind {
            SaveType::Sram32K | SaveType::Eeprom512B | SaveType::Eeprom8K => {
                self.write_storage(addr, value)
            }
            SaveType::Flash64K | SaveType::Flash128K => self.write_flash(addr, value),
            SaveType::None => {}
        }
    }

    fn write_storage(&mut self, addr: usize, value: u8) {
        if self.data.is_empty() {
            return;
        }
        let index = addr % self.data.len();
        if self.data[index] != value {
            self.data[index] = value;
            self.dirty = true;
        }
    }

    fn write_flash(&mut self, addr: usize, value: u8) {
        let address = addr & 0xffff;
        match self.flash_state {
            FlashCommandState::Ready => {
                if address == 0x5555 && value == 0xaa {
                    self.flash_state = FlashCommandState::Unlock1;
                }
            }
            FlashCommandState::Unlock1 => {
                self.flash_state = if address == 0x2aaa && value == 0x55 {
                    FlashCommandState::Unlock2
                } else {
                    FlashCommandState::Ready
                };
            }
            FlashCommandState::Unlock2 => {
                self.flash_state = match value {
                    0xa0 => FlashCommandState::Program,
                    0x80 => FlashCommandState::EraseUnlock1,
                    0xb0 if self.kind == SaveType::Flash128K => FlashCommandState::BankSwitch,
                    _ => FlashCommandState::Ready,
                };
            }
            FlashCommandState::Program => {
                self.write_flash_storage(addr, value);
                self.flash_state = FlashCommandState::Ready;
            }
            FlashCommandState::EraseUnlock1 => {
                self.flash_state = if address == 0x5555 && value == 0xaa {
                    FlashCommandState::EraseUnlock2
                } else {
                    FlashCommandState::Ready
                };
            }
            FlashCommandState::EraseUnlock2 => {
                self.flash_state = if address == 0x2aaa && value == 0x55 {
                    FlashCommandState::EraseCommand
                } else {
                    FlashCommandState::Ready
                };
            }
            FlashCommandState::EraseCommand => {
                match value {
                    0x10 if address == 0x5555 => self.erase_all(),
                    0x30 => self.erase_sector(addr),
                    _ => {}
                }
                self.flash_state = FlashCommandState::Ready;
            }
            FlashCommandState::BankSwitch => {
                if self.kind == SaveType::Flash128K && value <= 1 {
                    self.flash_bank = usize::from(value);
                }
                self.flash_state = FlashCommandState::Ready;
            }
        }
    }

    fn write_flash_storage(&mut self, addr: usize, value: u8) {
        let index = if self.kind == SaveType::Flash128K {
            self.flash_bank * 0x10000 + (addr & 0xffff)
        } else {
            addr % self.data.len().max(1)
        };
        if let Some(byte) = self.data.get_mut(index) {
            let next = *byte & value;
            if next != *byte {
                *byte = next;
                self.dirty = true;
            }
        }
    }

    fn erase_sector(&mut self, addr: usize) {
        let bank_base = if self.kind == SaveType::Flash128K {
            self.flash_bank * 0x10000
        } else {
            0
        };
        let start = bank_base + (addr & 0xffff) / FLASH_SECTOR_SIZE * FLASH_SECTOR_SIZE;
        let end = (start + FLASH_SECTOR_SIZE).min(self.data.len());
        if start < end && self.data[start..end].iter().any(|&byte| byte != 0xff) {
            self.data[start..end].fill(0xff);
            self.dirty = true;
        }
    }

    fn erase_all(&mut self) {
        if self.data.iter().any(|&byte| byte != 0xff) {
            self.data.fill(0xff);
            self.dirty = true;
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

    #[test]
    fn sram_writes_and_wraps() {
        let mut save = SaveRam::new(SaveType::Sram32K, None);
        save.write(0x0000, 0x12);
        save.write(0x8000, 0x34);
        assert_eq!(save.read(0x0000), 0x34);
    }

    #[test]
    fn flash_requires_unlock_before_programming() {
        let mut save = SaveRam::new(SaveType::Flash64K, None);
        save.write(0x0000, 0x12);
        assert_eq!(save.read(0), 0xff);

        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xa0);
        save.write(0x0000, 0x12);
        assert_eq!(save.read(0), 0x12);

        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xa0);
        save.write(0x0000, 0xff);
        assert_eq!(save.read(0), 0x12);
    }

    #[test]
    fn flash_sector_erase_restores_erased_bytes() {
        let mut save = SaveRam::new(SaveType::Flash64K, None);
        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xa0);
        save.write(0x1234, 0x00);
        assert_eq!(save.read(0x1234), 0x00);

        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0x80);
        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x1234, 0x30);
        assert_eq!(save.read(0x1234), 0xff);
    }

    #[test]
    fn flash128_bank_switch_isolated() {
        let mut save = SaveRam::new(SaveType::Flash128K, None);

        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xa0);
        save.write(0x0010, 0x11);

        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xb0);
        save.write(0x0000, 0x01);
        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xa0);
        save.write(0x0010, 0x22);

        assert_eq!(save.read(0x0010), 0x22);

        save.write(0x5555, 0xaa);
        save.write(0x2aaa, 0x55);
        save.write(0x5555, 0xb0);
        save.write(0x0000, 0x00);
        assert_eq!(save.read(0x0010), 0x11);
    }
}
