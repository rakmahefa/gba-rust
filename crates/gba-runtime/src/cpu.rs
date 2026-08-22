use crate::arm7tdmi::Nzcv;

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

    pub fn set_nzcv_masked(&mut self, flags: Nzcv, mask: u8) {
        if mask & 1 != 0 {
            if flags.n { self.cpsr |= CPSR_N } else { self.cpsr &= !CPSR_N }
        }
        if mask & 2 != 0 {
            if flags.z { self.cpsr |= CPSR_Z } else { self.cpsr &= !CPSR_Z }
        }
        if mask & 4 != 0 {
            if flags.c { self.cpsr |= CPSR_C } else { self.cpsr &= !CPSR_C }
        }
        if mask & 8 != 0 {
            if flags.v { self.cpsr |= CPSR_V } else { self.cpsr &= !CPSR_V }
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
