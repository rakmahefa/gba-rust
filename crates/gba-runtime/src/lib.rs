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
