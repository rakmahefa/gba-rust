mod arm7tdmi;
mod contract;
mod execution;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use arm7tdmi::Nzcv;
pub use arm7tdmi::{
    condition_holds as architectural_condition_holds, exchange_target, link_address,
    shift_immediate, shift_register, ShiftKind, ShiftResult,
};
pub use contract::{
    ArchitecturalState, GeneratedBlockExit, GeneratedBlockKey, GeneratedExecutionExit,
    GeneratedExecutionResult, RuntimeContract, GENERATED_TARGET_MISALIGNED,
    GENERATED_TARGET_OUTSIDE_CFG, RUNTIME_CONTRACT_VERSION,
};

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const REG_PC: usize = 15;
pub const REG_LR: usize = 14;
pub const REG_SP: usize = 13;

pub const CPSR_N: u32 = 1 << 31;
pub const CPSR_Z: u32 = 1 << 30;
pub const CPSR_C: u32 = 1 << 29;
pub const CPSR_V: u32 = 1 << 28;
const CPSR_I: u32 = 1 << 7;
const CPSR_F: u32 = 1 << 6;
const CPSR_T: u32 = 1 << 5;
const CPSR_MODE_MASK: u32 = 0x1f;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CpuMode {
    User = 0x10,
    Fiq = 0x11,
    Irq = 0x12,
    Supervisor = 0x13,
    Abort = 0x17,
    Undefined = 0x1b,
    System = 0x1f,
}
impl CpuMode {
    pub fn from_cpsr(cpsr: u32) -> Option<Self> {
        Some(match (cpsr & CPSR_MODE_MASK) as u8 {
            0x10 => Self::User,
            0x11 => Self::Fiq,
            0x12 => Self::Irq,
            0x13 => Self::Supervisor,
            0x17 => Self::Abort,
            0x1b => Self::Undefined,
            0x1f => Self::System,
            _ => return None,
        })
    }
    pub const fn privileged(self) -> bool {
        !matches!(self, Self::User)
    }
    pub const fn has_spsr(self) -> bool {
        !matches!(self, Self::User | Self::System)
    }
}

#[derive(Debug, Clone, Default)]
pub struct BankedRegisters {
    pub user_system_sp_lr: [u32; 2],
    pub fiq_r8_r12: [u32; 5],
    pub fiq_sp_lr: [u32; 2],
    pub irq_sp_lr: [u32; 2],
    pub svc_sp_lr: [u32; 2],
    pub abort_sp_lr: [u32; 2],
    pub undefined_sp_lr: [u32; 2],
    pub spsr_fiq: u32,
    pub spsr_irq: u32,
    pub spsr_svc: u32,
    pub spsr_abort: u32,
    pub spsr_undefined: u32,
}
impl BankedRegisters {
    fn save_mode(&mut self, mode: CpuMode, r: &[u32; 16]) {
        match mode {
            CpuMode::User | CpuMode::System => self.user_system_sp_lr = [r[13], r[14]],
            CpuMode::Fiq => {
                self.fiq_r8_r12.copy_from_slice(&r[8..13]);
                self.fiq_sp_lr = [r[13], r[14]];
            }
            CpuMode::Irq => self.irq_sp_lr = [r[13], r[14]],
            CpuMode::Supervisor => self.svc_sp_lr = [r[13], r[14]],
            CpuMode::Abort => self.abort_sp_lr = [r[13], r[14]],
            CpuMode::Undefined => self.undefined_sp_lr = [r[13], r[14]],
        }
    }
    fn load_mode(&self, mode: CpuMode, r: &mut [u32; 16]) {
        match mode {
            CpuMode::User | CpuMode::System => {
                r[13] = self.user_system_sp_lr[0];
                r[14] = self.user_system_sp_lr[1];
            }
            CpuMode::Fiq => {
                r[8..13].copy_from_slice(&self.fiq_r8_r12);
                r[13] = self.fiq_sp_lr[0];
                r[14] = self.fiq_sp_lr[1];
            }
            CpuMode::Irq => {
                r[13] = self.irq_sp_lr[0];
                r[14] = self.irq_sp_lr[1];
            }
            CpuMode::Supervisor => {
                r[13] = self.svc_sp_lr[0];
                r[14] = self.svc_sp_lr[1];
            }
            CpuMode::Abort => {
                r[13] = self.abort_sp_lr[0];
                r[14] = self.abort_sp_lr[1];
            }
            CpuMode::Undefined => {
                r[13] = self.undefined_sp_lr[0];
                r[14] = self.undefined_sp_lr[1];
            }
        }
    }
    fn spsr(&self, mode: CpuMode) -> Option<u32> {
        match mode {
            CpuMode::Fiq => Some(self.spsr_fiq),
            CpuMode::Irq => Some(self.spsr_irq),
            CpuMode::Supervisor => Some(self.spsr_svc),
            CpuMode::Abort => Some(self.spsr_abort),
            CpuMode::Undefined => Some(self.spsr_undefined),
            CpuMode::User | CpuMode::System => None,
        }
    }
    fn set_spsr(&mut self, mode: CpuMode, value: u32) -> bool {
        match mode {
            CpuMode::Fiq => self.spsr_fiq = value,
            CpuMode::Irq => self.spsr_irq = value,
            CpuMode::Supervisor => self.spsr_svc = value,
            CpuMode::Abort => self.spsr_abort = value,
            CpuMode::Undefined => self.spsr_undefined = value,
            CpuMode::User | CpuMode::System => return false,
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct Cpu {
    pub r: [u32; 16],
    pub cpsr: u32,
    pub thumb: bool,
    pub banked: BankedRegisters,
}
impl Default for Cpu {
    fn default() -> Self {
        Self {
            r: [0; 16],
            cpsr: CpuMode::System as u32,
            thumb: false,
            banked: BankedRegisters::default(),
        }
    }
}
impl Cpu {
    pub fn read_reg(&self, index: usize) -> u32 {
        self.r[index]
    }
    pub fn write_reg(&mut self, index: usize, value: u32) {
        self.r[index] = value;
    }
    pub fn nzcv(&self) -> Nzcv {
        Nzcv::from_cpsr(self.cpsr)
    }
    pub fn set_nzcv(&mut self, flags: Nzcv) {
        self.cpsr = (self.cpsr & !(CPSR_N | CPSR_Z | CPSR_C | CPSR_V)) | flags.bits();
    }
    pub fn set_nzcv_masked(&mut self, flags: Nzcv, mask: u8) {
        if mask & 1 != 0 {
            if flags.n {
                self.cpsr |= CPSR_N
            } else {
                self.cpsr &= !CPSR_N
            }
        }
        if mask & 2 != 0 {
            if flags.z {
                self.cpsr |= CPSR_Z
            } else {
                self.cpsr &= !CPSR_Z
            }
        }
        if mask & 4 != 0 {
            if flags.c {
                self.cpsr |= CPSR_C
            } else {
                self.cpsr &= !CPSR_C
            }
        }
        if mask & 8 != 0 {
            if flags.v {
                self.cpsr |= CPSR_V
            } else {
                self.cpsr &= !CPSR_V
            }
        }
    }
    pub fn mode(&self) -> CpuMode {
        CpuMode::from_cpsr(self.cpsr).unwrap_or(CpuMode::System)
    }
    pub fn set_thumb(&mut self, thumb: bool) {
        self.thumb = thumb;
        if thumb {
            self.cpsr |= CPSR_T
        } else {
            self.cpsr &= !CPSR_T
        }
    }
    pub fn switch_mode(&mut self, new_mode: CpuMode) {
        let old_mode = self.mode();
        if old_mode == new_mode {
            return;
        }
        self.banked.save_mode(old_mode, &self.r);
        if old_mode == CpuMode::Fiq && new_mode != CpuMode::Fiq {
            self.r[8..13].copy_from_slice(&self.banked.fiq_r8_r12);
        }
        self.banked.load_mode(new_mode, &mut self.r);
        self.cpsr = (self.cpsr & !CPSR_MODE_MASK) | new_mode as u32;
    }
    pub fn spsr(&self) -> Option<u32> {
        self.banked.spsr(self.mode())
    }
    pub fn set_spsr(&mut self, value: u32) -> bool {
        self.banked.set_spsr(self.mode(), value)
    }
    pub fn restore_exception_state(&mut self, spsr: u32) {
        let current_mode = self.mode();
        self.banked.save_mode(current_mode, &self.r);
        if current_mode == CpuMode::Fiq {
            self.r[8..13].copy_from_slice(&self.banked.fiq_r8_r12);
        }
        self.cpsr = spsr;
        let restored_mode = self.mode();
        self.banked.load_mode(restored_mode, &mut self.r);
        self.thumb = spsr & CPSR_T != 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionKind {
    Undefined,
    SoftwareInterrupt,
    PrefetchAbort,
    DataAbort,
    Irq,
    Fiq,
}
impl ExceptionKind {
    pub const fn mode(self) -> CpuMode {
        match self {
            Self::Undefined => CpuMode::Undefined,
            Self::SoftwareInterrupt => CpuMode::Supervisor,
            Self::PrefetchAbort | Self::DataAbort => CpuMode::Abort,
            Self::Irq => CpuMode::Irq,
            Self::Fiq => CpuMode::Fiq,
        }
    }
    pub const fn vector(self) -> u32 {
        match self {
            Self::Undefined => 0x04,
            Self::SoftwareInterrupt => 0x08,
            Self::PrefetchAbort => 0x0c,
            Self::DataAbort => 0x10,
            Self::Irq => 0x18,
            Self::Fiq => 0x1c,
        }
    }
    pub const fn masks(self) -> u32 {
        match self {
            Self::Fiq => CPSR_I | CPSR_F,
            Self::Irq => CPSR_I,
            _ => CPSR_I,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ppu {
    pub framebuffer: Vec<u32>,
    pub frame: u64,
}
impl Default for Ppu {
    fn default() -> Self {
        Self {
            framebuffer: vec![0; WIDTH * HEIGHT],
            frame: 0,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct Apu {
    pub samples_generated: u64,
}
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

#[derive(Debug, Default)]
pub struct Runtime {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub apu: Apu,
    pub cartridge: Option<Cartridge>,
    pub io: HashMap<u32, u8>,
    pub cycles: u64,
}
impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn load_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
        self.cpu.r[REG_PC] = 0x0800_0000;
    }
    pub fn read_reg(&self, index: usize) -> u32 {
        self.cpu.read_reg(index)
    }
    pub fn write_reg(&mut self, index: usize, value: u32) {
        self.cpu.write_reg(index, value);
    }
    pub fn set_flags(&mut self, flags: Nzcv) {
        self.cpu.set_nzcv(flags);
    }
    pub fn nzcv(&self) -> Nzcv {
        self.cpu.nzcv()
    }
    pub fn mode(&self) -> CpuMode {
        self.cpu.mode()
    }
    pub fn set_thumb(&mut self, thumb: bool) {
        self.cpu.set_thumb(thumb);
    }
    pub fn switch_mode(&mut self, mode: CpuMode) {
        self.cpu.switch_mode(mode);
    }
    pub fn mov(&mut self, dst: usize, value: u32, set_flags: bool) {
        self.cpu.write_reg(dst, value);
        if set_flags {
            let current = self.cpu.nzcv();
            self.set_flags(Nzcv {
                n: value & 0x8000_0000 != 0,
                z: value == 0,
                c: current.c,
                v: current.v,
            });
        }
    }
    pub fn add(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::add_with_carry(lhs, rhs, false);
        self.cpu.write_reg(dst, result);
        if set_flags {
            self.set_flags(flags);
        }
    }
    pub fn adc(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::add_with_carry(lhs, rhs, self.cpu.nzcv().c);
        self.cpu.write_reg(dst, result);
        if set_flags {
            self.set_flags(flags);
        }
    }
    pub fn sub(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, false);
        self.cpu.write_reg(dst, result);
        if set_flags {
            self.set_flags(flags);
        }
    }
    pub fn sbc(&mut self, dst: usize, lhs: u32, rhs: u32, set_flags: bool) {
        let (result, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, !self.cpu.nzcv().c);
        self.cpu.write_reg(dst, result);
        if set_flags {
            self.set_flags(flags);
        }
    }
    pub fn compare(&mut self, lhs: u32, rhs: u32) {
        let (_, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, false);
        self.set_flags(flags);
    }
    pub fn shift(
        &self,
        value: u32,
        kind: ShiftKind,
        amount: u8,
        register_shift: bool,
    ) -> ShiftResult {
        let carry = self.cpu.nzcv().c;
        if register_shift {
            arm7tdmi::shift_register(value, kind, amount, carry)
        } else {
            arm7tdmi::shift_immediate(value, kind, amount, carry)
        }
    }
    pub fn enter_instruction(&mut self, address: u32, thumb: bool) {
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = arm7tdmi::architectural_pc(address, thumb);
    }
    pub fn link_from_instruction(&mut self, address: u32, size: u8, thumb: bool) {
        self.cpu.r[REG_LR] = arm7tdmi::link_address(address, size, thumb);
    }
    pub fn condition_code(&self, code: u8) -> bool {
        arm7tdmi::condition_holds(self.cpu.cpsr, code)
    }
    pub fn exception_return(&mut self, target: u32) -> Option<(u32, bool)> {
        let spsr = self.cpu.spsr()?;
        self.cpu.restore_exception_state(spsr);
        let aligned = target & if self.cpu.thumb { !1 } else { !3 };
        self.cpu.r[REG_PC] = aligned;
        Some((aligned, self.cpu.thumb))
    }
    pub fn raise_exception(&mut self, kind: ExceptionKind) -> (u32, bool) {
        let old_cpsr = self.cpu.cpsr;
        let return_address = if self.cpu.thumb {
            self.read_reg(REG_PC).wrapping_sub(2)
        } else {
            self.read_reg(REG_PC).wrapping_sub(4)
        };
        self.cpu.switch_mode(kind.mode());
        self.cpu.set_spsr(old_cpsr);
        self.cpu.cpsr |= kind.masks();
        self.cpu.set_thumb(false);
        self.cpu.r[REG_LR] = return_address;
        self.cpu.r[REG_PC] = kind.vector();
        (kind.vector(), false)
    }
    pub fn step_recompiled(&mut self, cycles: u32) {
        self.cycles = self.cycles.wrapping_add(cycles as u64);
    }
    pub fn tick(&mut self, cycles: u32) {
        self.step_recompiled(cycles);
    }
    pub fn trace_recompiled(&mut self, _address: u32, _raw: u32) {
        self.step_recompiled(1);
    }
    pub fn frame(&mut self) {
        self.ppu.frame = self.ppu.frame.wrapping_add(1);
    }
    pub fn read8(&self, address: u32) -> u8 {
        if (0x0800_0000..0x0E00_0000).contains(&address) {
            self.cartridge
                .as_ref()
                .and_then(|c| c.rom.get((address - 0x0800_0000) as usize))
                .copied()
                .unwrap_or(0xff)
        } else if (0x0E00_0000..=0x0E00_FFFF).contains(&address) {
            self.cartridge
                .as_ref()
                .map(|c| c.save.read((address - 0x0E00_0000) as usize))
                .unwrap_or(0xff)
        } else {
            *self.io.get(&address).unwrap_or(&0)
        }
    }
    pub fn read16(&self, address: u32) -> u16 {
        u16::from_le_bytes([self.read8(address), self.read8(address.wrapping_add(1))])
    }
    pub fn read32(&self, address: u32) -> u32 {
        let aligned = address & !3;
        let raw = u32::from_le_bytes([
            self.read8(aligned),
            self.read8(aligned.wrapping_add(1)),
            self.read8(aligned.wrapping_add(2)),
            self.read8(aligned.wrapping_add(3)),
        ]);
        arm7tdmi::rotate_unaligned_word(raw, address)
    }
    pub fn write8(&mut self, address: u32, value: u8) {
        if (0x0E00_0000..=0x0E00_FFFF).contains(&address) {
            if let Some(cartridge) = self.cartridge.as_mut() {
                cartridge
                    .save
                    .write((address - 0x0E00_0000) as usize, value);
            }
        } else {
            self.io.insert(address, value);
        }
    }
    pub fn write16(&mut self, address: u32, value: u16) {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte)
        }
    }
    pub fn write32(&mut self, address: u32, value: u32) {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte)
        }
    }
    pub fn dispatch_mode(&mut self, address: u32, thumb: bool) -> ! {
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = address & if thumb { !1 } else { !3 };
        panic!(
            "generated dispatch target {address:#010x} ({}) is not linked yet",
            if thumb { "Thumb" } else { "ARM" }
        )
    }
    pub fn dispatch_exchange(&mut self, target: u32) -> ! {
        let (address, thumb) = arm7tdmi::exchange_target(target);
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = address;
        panic!("generated BX target {target:#010x} is not linked yet")
    }
    pub fn dispatch(&mut self, address: u32) -> ! {
        self.dispatch_mode(address, self.cpu.thumb)
    }
    pub fn halt(&mut self) -> ! {
        panic!("recompiled program halted")
    }
    pub fn unimplemented(&mut self, address: u32, raw: u32, mode: &str) -> ! {
        panic!("unimplemented {mode} instruction {raw:#010x} at {address:#010x}")
    }
    pub fn run_generated<F>(
        &mut self,
        address: u32,
        thumb: bool,
        max_steps: Option<u64>,
        mut dispatch: F,
    ) -> Result<(u32, bool), &'static str>
    where
        F: FnMut(&mut Runtime, u32, bool) -> Result<(u32, bool), &'static str>,
    {
        let mut next = (address & if thumb { !1 } else { !3 }, thumb);
        let mut steps = 0u64;
        loop {
            if let Some(limit) = max_steps {
                if steps >= limit {
                    return Err("generated execution step limit exceeded");
                }
            }
            self.cpu.set_thumb(next.1);
            self.cpu.r[REG_PC] = next.0;
            next = dispatch(self, next.0, next.1)?;
            steps = steps.wrapping_add(1);
        }
    }
    pub fn exchange_target_for_dispatch(&mut self, target: u32) -> (u32, bool) {
        let (address, thumb) = arm7tdmi::exchange_target(target);
        self.cpu.set_thumb(thumb);
        self.cpu.r[REG_PC] = address;
        (address, thumb)
    }
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
        runtime.cpu.cpsr = CPSR_V | CpuMode::System as u32;
        assert!(runtime.condition_code(6));
        assert!(!runtime.condition_code(7));
        runtime.cpu.cpsr = CPSR_C | CpuMode::System as u32;
        assert!(runtime.condition_code(2));
        assert!(runtime.condition_code(8));
        assert!(!runtime.condition_code(9));
    }
    #[test]
    fn mov_preserves_c_and_v_when_setting_logical_flags() {
        let mut runtime = Runtime::new();
        runtime.cpu.cpsr = CPSR_C | CPSR_V | CpuMode::System as u32;
        runtime.mov(0, 0, true);
        assert!(runtime.cpu.cpsr & CPSR_Z != 0);
        assert!(runtime.cpu.cpsr & CPSR_C != 0);
        assert!(runtime.cpu.cpsr & CPSR_V != 0);
    }
    #[test]
    fn adc_and_sbc_consume_the_current_carry_bit() {
        let mut runtime = Runtime::new();
        runtime.cpu.cpsr = CPSR_C | CpuMode::System as u32;
        runtime.adc(0, 1, 1, true);
        assert_eq!(runtime.read_reg(0), 3);
        runtime.cpu.cpsr = CPSR_C | CpuMode::System as u32;
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
        assert_eq!(runtime.read_reg(REG_LR), 0x0800_0104);
        runtime.enter_instruction(0x0800_0100, true);
        assert_eq!(runtime.read_reg(REG_PC), 0x0800_0104);
        runtime.link_from_instruction(0x0800_0100, 4, true);
        assert_eq!(runtime.read_reg(REG_LR), 0x0800_0105);
    }
    #[test]
    fn word_reads_apply_arm_unaligned_rotation() {
        let mut runtime = Runtime::new();
        runtime.write32(0x0400_0000, 0x4433_2211);
        assert_eq!(runtime.read32(0x0400_0001), 0x1144_3322);
        assert_eq!(runtime.read32(0x0400_0002), 0x2211_4433);
    }
    #[test]
    fn banked_modes_keep_distinct_stack_and_link_registers() {
        let mut runtime = Runtime::new();
        runtime.write_reg(REG_SP, 0x1000);
        runtime.write_reg(REG_LR, 0x2000);
        runtime.switch_mode(CpuMode::Supervisor);
        runtime.write_reg(REG_SP, 0x3000);
        runtime.write_reg(REG_LR, 0x4000);
        runtime.switch_mode(CpuMode::System);
        assert_eq!(runtime.read_reg(REG_SP), 0x1000);
        assert_eq!(runtime.read_reg(REG_LR), 0x2000);
        runtime.switch_mode(CpuMode::Supervisor);
        assert_eq!(runtime.read_reg(REG_SP), 0x3000);
        assert_eq!(runtime.read_reg(REG_LR), 0x4000);
    }
    #[test]
    fn swi_exception_saves_cpsr_and_restores_banks() {
        let mut runtime = Runtime::new();
        runtime.enter_instruction(0x0800_0100, false);
        runtime.write_reg(REG_SP, 0x1000);
        runtime.write_reg(REG_LR, 0x2000);
        let old = runtime.cpu.cpsr;
        let (vector, thumb) = runtime.raise_exception(ExceptionKind::SoftwareInterrupt);
        assert_eq!((vector, thumb), (0x08, false));
        assert_eq!(runtime.mode(), CpuMode::Supervisor);
        runtime.write_reg(REG_SP, 0x3000);
        runtime.write_reg(REG_LR, 0x4000);
        assert_eq!(runtime.cpu.banked.spsr(CpuMode::Supervisor), Some(old));
        let result = runtime
            .exception_return(0x0800_0104)
            .expect("exception return");
        assert_eq!(result, (0x0800_0104, false));
        assert_eq!(runtime.mode(), CpuMode::System);
        assert_eq!(runtime.read_reg(REG_SP), 0x1000);
        assert_eq!(runtime.read_reg(REG_LR), 0x2000);
    }
    #[test]
    fn generated_engine_dispatches_iteratively_without_recursive_calls() {
        let mut runtime = Runtime::new();
        let result =
            runtime.run_generated(0x0800_0000, false, Some(10_000), |rt, address, thumb| {
                rt.tick(1);
                if rt.cycles == 10_000 {
                    Err("done")
                } else {
                    Ok((address, thumb))
                }
            });
        assert_eq!(result, Err("done"));
        assert_eq!(runtime.cycles, 10_000);
    }
    #[test]
    fn exchange_target_for_dispatch_updates_thumb_and_pc() {
        let mut runtime = Runtime::new();
        assert_eq!(
            runtime.exchange_target_for_dispatch(0x0800_0101),
            (0x0800_0100, true)
        );
        assert!(runtime.cpu.thumb);
        assert_eq!(runtime.read_reg(REG_PC), 0x0800_0100);
    }
}
