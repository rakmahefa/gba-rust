use std::{fs, io, path::{Path, PathBuf}};

use thiserror::Error;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const FRAME_CYCLES: u64 = 280_896;

#[derive(Debug, Error)]
pub enum GbaError {
    #[error("ROM is too small or invalid")]
    InvalidRom,
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveKind { None, Sram32K, Flash64K, Flash128K, Eeprom512B, Eeprom8K }

#[derive(Debug)]
pub struct SaveStore {
    path: PathBuf,
    backup: PathBuf,
    data: Vec<u8>,
    dirty: bool,
    generation: u64,
}

impl SaveStore {
    pub fn open(dir: impl AsRef<Path>, stem: &str, kind: SaveKind) -> io::Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        let size = match kind { SaveKind::Sram32K | SaveKind::Flash64K => 0x8000, SaveKind::Flash128K => 0x20000, SaveKind::Eeprom512B => 0x200, SaveKind::Eeprom8K => 0x2000, SaveKind::None => 0 };
        let path = dir.as_ref().join(format!("{stem}.sav"));
        let backup = dir.as_ref().join(format!("{stem}.sav.bak"));
        let mut data = vec![0xff; size];
        if size != 0 { if let Ok(bytes) = fs::read(&path) { let n = bytes.len().min(size); data[..n].copy_from_slice(&bytes[..n]); } }
        Ok(Self { path, backup, data, dirty: false, generation: 0 })
    }
    pub fn bytes(&self) -> &[u8] { &self.data }
    pub fn bytes_mut(&mut self) -> &mut [u8] { self.dirty = true; self.generation = self.generation.wrapping_add(1); &mut self.data }
    pub fn mark_dirty(&mut self) { self.dirty = true; self.generation = self.generation.wrapping_add(1); }
    pub fn dirty(&self) -> bool { self.dirty }
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty || self.data.is_empty() { return Ok(()); }
        let tmp = self.path.with_extension("sav.tmp");
        fs::write(&tmp, &self.data)?;
        if self.path.exists() { let _ = fs::copy(&self.path, &self.backup); }
        fs::rename(tmp, &self.path)?;
        self.dirty = false;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Cartridge { pub rom: Vec<u8>, pub save: SaveStore, pub save_kind: SaveKind, pub title: String, pub game_code: [u8;4] }

impl Cartridge {
    pub fn load(path: impl AsRef<Path>, save_dir: impl AsRef<Path>) -> Result<Self, GbaError> {
        let rom = fs::read(path.as_ref())?;
        if rom.len() < 0xC0 { return Err(GbaError::InvalidRom); }
        let title = String::from_utf8_lossy(&rom[0xA0..0xAC]).trim_end_matches('\0').trim().to_string();
        let mut game_code = [0;4]; game_code.copy_from_slice(&rom[0xAC..0xB0]);
        let save_kind = detect_save_kind(&rom);
        let stem = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap_or("game");
        let save = SaveStore::open(save_dir, stem, save_kind)?;
        Ok(Self { rom, save, save_kind, title, game_code })
    }
}

fn detect_save_kind(rom: &[u8]) -> SaveKind {
    let sample = &rom[..rom.len().min(rom.len())];
    if sample.windows(12).any(|w| w == b"EEPROM_V124") { SaveKind::Eeprom8K }
    else if sample.windows(11).any(|w| w == b"EEPROM_V111") { SaveKind::Eeprom512B }
    else if sample.windows(10).any(|w| w == b"FLASH1M_V") || sample.windows(9).any(|w| w == b"FLASH512") { SaveKind::Flash128K }
    else if sample.windows(8).any(|w| w == b"FLASH_V") { SaveKind::Flash64K }
    else if sample.windows(8).any(|w| w == b"SRAM_V") { SaveKind::Sram32K }
    else { SaveKind::None }
}
