use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveType {
    None,
    Sram32K,
    Flash64K,
    Flash128K,
    Eeprom512B,
    Eeprom8K,
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
