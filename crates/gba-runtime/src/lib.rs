mod arm7tdmi;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use arm7tdmi::{condition_holds as architectural_condition_holds, exchange_target, link_address, shift_immediate, shift_register, ShiftKind, ShiftResult};
pub use arm7tdmi::Nzcv;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const REG_PC: usize = 15;
pub const REG_LR: usize = 14;

const CPSR_N: u32 = 1 << 31;
const CPSR_Z: u32 = 1 << 30;
const CPSR_C: u32 = 1 << 29;
const CPSR_V: u32 = 1 << 28;
const CPSR_T: u32 = 1 << 5;

#[derive(Debug, Clone)]
pub struct Cpu { pub r: [u32; 16], pub cpsr: u32, pub thumb: bool }
impl Default for Cpu { fn default() -> Self { Self { r: [0; 16], cpsr: 0x0000_001f, thumb: false } } }
impl Cpu {
    pub fn read_reg(&self, index: usize) -> u32 { self.r[index] }
    pub fn write_reg(&mut self, index: usize, value: u32) { self.r[index] = value; }
    pub fn nzcv(&self) -> Nzcv { Nzcv::from_cpsr(self.cpsr) }
    pub fn set_nzcv(&mut self, flags: Nzcv) { self.cpsr = (self.cpsr & !(CPSR_N | CPSR_Z | CPSR_C | CPSR_V)) | flags.bits(); }
    pub fn set_nzcv_masked(&mut self, flags: Nzcv, mask: u8) {
        if mask & 1 != 0 && flags.n { self.cpsr |= CPSR_N } else if mask & 1 != 0 { self.cpsr &= !CPSR_N; }
        if mask & 2 != 0 && flags.z { self.cpsr |= CPSR_Z } else if mask & 2 != 0 { self.cpsr &= !CPSR_Z; }
        if mask & 4 != 0 && flags.c { self.cpsr |= CPSR_C } else if mask & 4 != 0 { self.cpsr &= !CPSR_C; }
        if mask & 8 != 0 && flags.v { self.cpsr |= CPSR_V } else if mask & 8 != 0 { self.cpsr &= !CPSR_V; }
    }
    pub fn set_thumb(&mut self, thumb: bool) { self.thumb = thumb; if thumb { self.cpsr |= CPSR_T; } else { self.cpsr &= !CPSR_T; } }
}

#[derive(Debug, Clone)]
pub struct Ppu { pub framebuffer: Vec<u32>, pub frame: u64 }
impl Default for Ppu { fn default() -> Self { Self { framebuffer: vec![0; WIDTH * HEIGHT], frame: 0 } } }

#[derive(Debug, Clone, Default)]
pub struct Apu { pub samples_generated: u64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveType { None, Sram32K, Flash64K, Flash128K, Eeprom512B, Eeprom8K }

#[derive(Debug, Clone)]
pub struct SaveRam { kind: SaveType, data: Vec<u8>, path: Option<PathBuf>, dirty: bool }
impl SaveRam {
    pub fn new(kind: SaveType, path: Option<PathBuf>) -> Self {
        let len = match kind { SaveType::Sram32K => 0x8000, SaveType::Flash64K => 0x10000, SaveType::Flash128K => 0x20000, SaveType::Eeprom512B => 512, SaveType::Eeprom8K => 8192, SaveType::None => 0 };
        let mut data = vec![0xff; len];
        if let Some(p) = &path { if let Ok(existing) = fs::read(p) { if existing.len() == len { data = existing; } } }
        Self { kind, data, path, dirty: false }
    }
    pub fn kind(&self) -> SaveType { self.kind }
    pub fn read(&self, addr: usize) -> u8 { self.data.get(addr % self.data.len().max(1)).copied().unwrap_or(0xff) }
    pub fn write(&mut self, addr: usize, value: u8) { if !self.data.is_empty() { let i = addr % self.data.len(); if self.data[i] != value { self.data[i] = value; self.dirty = true; } } }
    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty { return Ok(()); }
        let Some(path) = &self.path else { return Ok(()); };
        let tmp = path.with_extension("sav.tmp");
        fs::write(&tmp, &self.data)?;
        if path.exists() { let backup = path.with_extension("sav.bak"); let _ = fs::copy(path, backup); }
        fs::rename(tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}
impl Drop for SaveRam { fn drop(&mut self) { let _ = self.flush(); } }

#[derive(Debug, Clone)]
pub struct Cartridge { pub rom: Vec<u8>, pub save: SaveRam }
impl Cartridge {
    pub fn from_rom(rom: Vec<u8>, save_dir: impl AsRef<Path>) -> Self {
        let title = rom.get(0xa0..0xac).unwrap_or_default();
        let stem = String::from_utf8_lossy(title).trim_matches('\0').trim().to_string();
        let kind = detect_save_type(&rom);
        let path = if kind == SaveType::None { None } else { Some(save_dir.as_ref().join(format!("{}.sav", if stem.is_empty() { "game" } else { &stem }))) };
        Self { rom, save: SaveRam::new(kind, path) }
    }
}

pub fn detect_save_type(rom: &[u8]) -> SaveType {
    let text = String::from_utf8_lossy(rom);
    if text.contains("EEPROM_V") { if text.contains("8K") { SaveType::Eeprom8K } else { SaveType::Eeprom512B } }
    else if text.contains("FLASH1M_V") { SaveType::Flash128K }
    else if text.contains("FLASH_V") { SaveType::Flash64K }
    else if text.contains("SRAM_V") { SaveType::Sram32K }
    else { SaveType::None }
}

#[derive(Debug, Default)]
pub struct Runtime { pub cpu: Cpu, pub ppu: Ppu, pub apu: Apu, pub cartridge: Option<Cartridge>, pub io: HashMap<u32, u8>, pub cycles: u64 }
impl Runtime {
    pub fn new() -> Self { Self::default() }
    pub fn load_cartridge(&mut self, cartridge: Cartridge) { self.cartridge = Some(cartridge); self.cpu.r[REG_PC] = 0x0800_0000; }

    pub fn read_reg(&self, index: usize) -> u32 { self.cpu.read_reg(index) }
    pub fn write_reg(&mut self, index: usize, value: u32) { self.cpu.write_reg(index, value); }
    pub fn set_flags(&mut self, flags: Nzcv) { self.cpu.set_nzcv(flags); }
    pub fn nzcv(&self) -> Nzcv { self.cpu.nzcv() }

    pub fn mov(&mut self, dst: usize, value: u32, set_flags: bool) {
        self.cpu.write_reg(dst, value);
        if set_flags {
            let current = self.cpu.nzcv();
            self.set_flags(Nzcv { n: value & 0x8000_0000 != 0, z: value == 0, c: current.c, v: current.v });
        }
    }
    pub fn add(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::add_with_carry(lhs, rhs, false);
        self.cpu.write_reg(dst, result);
        if set_flags { self.set_flags(flags); }
    }
    pub fn adc(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::add_with_carry(lhs, rhs, self.cpu.nzcv().c);
        self.cpu.write_reg(dst, result);
        if set_flags { self.set_flags(flags); }
    }
    pub fn sub(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, false);
        self.cpu.write_reg(dst, result);
        if set_flags { self.set_flags(flags); }
    }
    pub fn sbc(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, !self.cpu.nzcv().c);
        self.cpu.write_reg(dst, result);
        if set_flags { self.set_flags(flags); }
    }
    pub fn compare(&mut self, lhs: u32, rhs: u32) { let (_, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, false); self.set_flags(flags); }

    pub fn shift(&self, value: u32, kind: ShiftKind, amount: u8, register_shift: bool) -> ShiftResult {
        let carry = self.cpu.nzcv().c;
        if register_shift { arm7tdmi::shift_register(value, kind, amount, carry) } else { arm7tdmi::shift_immediate(value, kind, amount, carry) }
    }

    pub fn enter_instruction(&mut self, address: u32, thumb: bool) {
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = arm7tdmi::architectural_pc(address, thumb);
    }
    pub fn link_from_instruction(&mut self, address: u32, size: u8, thumb: bool) {
        self.cpu.r[REG_LR] = arm7tdmi::link_address(address, size, thumb);
    }

    pub fn condition_code(&self, code: u8) -> bool { arm7tdmi::condition_holds(self.cpu.cpsr, code) }

    pub fn step_recompiled(&mut self, cycles: u32) { self.cycles = self.cycles.wrapping_add(cycles as u64); }
    pub fn tick(&mut self, cycles: u32) { self.step_recompiled(cycles); }
    pub fn trace_recompiled(&mut self, _address: u32, _raw: u32) { self.step_recompiled(1); }
    pub fn frame(&mut self) { self.ppu.frame = self.ppu.frame.wrapping_add(1); }

    pub fn read8(&self, address: u32) -> u8 {
        if (0x0800_0000..0x0E00_0000).contains(&address) {
            self.cartridge.as_ref().and_then(|c| c.rom.get((address - 0x0800_0000) as usize)).copied().unwrap_or(0xff)
        } else if (0x0E00_0000..=0x0E00_FFFF).contains(&address) {
            self.cartridge.as_ref().map(|c| c.save.read((address - 0x0E00_0000) as usize)).unwrap_or(0xff)
        } else { *self.io.get(&address).unwrap_or(&0) }
    }
    pub fn read16(&self, address: u32) -> u16 { u16::from_le_bytes([self.read8(address), self.read8(address.wrapping_add(1))]) }
    pub fn read32(&self, address: u32) -> u32 { let raw = u32::from_le_bytes([self.read8(address & !3), self.read8((address & !3).wrapping_add(1)), self.read8((address & !3).wrapping_add(2)), self.read8((address & !3).wrapping_add(3))]); arm7tdmi::rotate_unaligned_word(raw, address) }
    pub fn write8(&mut self, address: u32, value: u8) {
        if (0x0E00_0000..=0x0E00_FFFF).contains(&address) {
            if let Some(cartridge) = self.cartridge.as_mut() { cartridge.save.write((address - 0x0E00_0000) as usize, value); }
        } else { self.io.insert(address, value); }
    }
    pub fn write16(&mut self, address: u32, value: u16) { for (i, byte) in value.to_le_bytes().into_iter().enumerate() { self.write8(address.wrapping_add(i as u32), byte); } }
    pub fn write32(&mut self, address: u32, value: u32) { for (i, byte) in value.to_le_bytes().into_iter().enumerate() { self.write8(address.wrapping_add(i as u32), byte); } }

    pub fn dispatch_mode(&mut self, address: u32, thumb: bool) -> ! {
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = address & if thumb { !1 } else { !3 };
        panic!("generated dispatch target {address:#010x} ({}) is not linked yet", if thumb { "Thumb" } else { "ARM" })
    }
    pub fn dispatch_exchange(&mut self, target: u32) -> ! {
        let (address, thumb) = arm7tdmi::exchange_target(target);
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = address;
        panic!("generated BX target {target:#010x} is not linked yet")
    }
    pub fn dispatch(&mut self, address: u32) -> ! { self.dispatch_mode(address, self.cpu.thumb) }
    pub fn halt(&mut self) -> ! { panic!("recompiled program halted") }
    pub fn unimplemented(&mut self, address: u32, raw: u32, mode: &str) -> ! { panic!("unimplemented {mode} instruction {raw:#010x} at {address:#010x}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_updates_all_nzcv_flags() {
        let mut runtime = Runtime::new();
        runtime.compare(0x7fff_ffff, u32::MAX);
        assert!(runtime.cpu.cpsr & CPSR_N != 0);
        assert!(runtime.cpu.cpsr & CPSR_C == 0);
        assert!(runtime.cpu.cpsr & CPSR_V != 0);
    }

    #[test]
    fn condition_contract_covers_signed_and_unsigned_relations() {
        let mut runtime = Runtime::new();
        runtime.cpu.cpsr = CPSR_V;
        assert!(runtime.condition_code(6));
        assert!(!runtime.condition_code(7));
        runtime.cpu.cpsr = CPSR_C;
        assert!(runtime.condition_code(2));
        assert!(runtime.condition_code(8));
        assert!(!runtime.condition_code(9));
    }

    #[test]
    fn mov_preserves_c_and_v_when_setting_logical_flags() {
        let mut runtime = Runtime::new();
        runtime.cpu.cpsr = CPSR_C | CPSR_V;
        runtime.mov(0, 0, true);
        assert!(runtime.cpu.cpsr & CPSR_Z != 0);
        assert!(runtime.cpu.cpsr & CPSR_C != 0);
        assert!(runtime.cpu.cpsr & CPSR_V != 0);
    }

    #[test]
    fn adc_and_sbc_consume_the_current_carry_bit() {
        let mut runtime = Runtime::new();

        runtime.cpu.cpsr = CPSR_C;
        runtime.adc(0, 1, 1, true);
        assert_eq!(runtime.read_reg(0), 3);

        runtime.cpu.cpsr = CPSR_C;
        runtime.sbc(1, 3, 1, true);
        assert_eq!(runtime.read_reg(1), 2);

        runtime.cpu.cpsr &= !CPSR_C;
        runtime.adc(2, 1, 1, true);
        assert_eq!(runtime.read_reg(2), 2);

        runtime.cpu.cpsr &= !CPSR_C;
        runtime.sbc(3, 3, 1, true);
        assert_eq!(runtime.read_reg(3), 1);
    }

    #[test]
    fn instruction_context_exposes_architectural_pc_and_link_value() {
        let mut runtime = Runtime::new();
        runtime.enter_instruction(0x0800_0100, false);
        assert_eq!(runtime.read_reg(REG_PC), 0x0800_0108);
        runtime.link_from_instruction(0x0800_0100, 4, false);
        assert_eq!(runtime.read_reg(REG_LR), 0... truncated