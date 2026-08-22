use crate::cpu::{CpuMode, ExceptionKind, REG_LR, REG_PC, REG_SP};

pub const BIOS_START: u32 = 0x0000_0000;
pub const BIOS_END: u32 = 0x0000_3fff;
pub const EWRAM_START: u32 = 0x0200_0000;
pub const EWRAM_END: u32 = 0x0203_ffff;
pub const IWRAM_START: u32 = 0x0300_0000;
pub const IWRAM_END: u32 = 0x0300_7fff;
pub const IO_START: u32 = 0x0400_0000;
pub const IO_END: u32 = 0x0400_03ff;
pub const PALETTE_START: u32 = 0x0500_0000;
pub const PALETTE_END: u32 = 0x0500_03ff;
pub const VRAM_START: u32 = 0x0600_0000;
pub const VRAM_END: u32 = 0x0601_7fff;
pub const OAM_START: u32 = 0x0700_0000;
pub const OAM_END: u32 = 0x0700_03ff;
pub const OBJ_AFFINE_START: u32 = 0x0700_0060;
pub const OBJ_AFFINE_END: u32 = 0x0700_03ff;

pub const IE: u32 = 0x0400_0200;
pub const IF: u32 = 0x0400_0202;
pub const IME: u32 = 0x0400_0208;
pub const WAITCNT: u32 = 0x0400_0204;
pub const HALTCNT: u32 = 0x0400_0301;
pub const POSTFLG: u32 = 0x0400_0300;
pub const KEYINPUT: u32 = 0x0400_0130;
pub const DISPSTAT: u32 = 0x0400_0004;

pub const IRQ_VBLANK: u16 = 1 << 0;
pub const IRQ_HBLANK: u16 = 1 << 1;
pub const IRQ_VCOUNT: u16 = 1 << 2;
pub const IRQ_TIMER0: u16 = 1 << 3;
pub const IRQ_TIMER1: u16 = 1 << 4;
pub const IRQ_TIMER2: u16 = 1 << 5;
pub const IRQ_TIMER3: u16 = 1 << 6;
pub const IRQ_SERIAL: u16 = 1 << 7;
pub const IRQ_DMA0: u16 = 1 << 8;
pub const IRQ_DMA1: u16 = 1 << 9;
pub const IRQ_DMA2: u16 = 1 << 10;
pub const IRQ_DMA3: u16 = 1 << 11;
pub const IRQ_KEYPAD: u16 = 1 << 12;
pub const IRQ_GAMEPAK: u16 = 1 << 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Running,
    Halted,
    Stopped,
}

impl Default for PowerState {
    fn default() -> Self {
        Self::Running
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiosSwi {
    SoftReset = 0x00,
    RegisterRamReset = 0x01,
    Halt = 0x02,
    Stop = 0x03,
    IntrWait = 0x04,
    VBlankIntrWait = 0x05,
}

impl BiosSwi {
    pub fn from_number(number: u8) -> Option<Self> {
        Some(match number {
            0x00 => Self::SoftReset,
            0x01 => Self::RegisterRamReset,
            0x02 => Self::Halt,
            0x03 => Self::Stop,
            0x04 => Self::IntrWait,
            0x05 => Self::VBlankIntrWait,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptController {
    pub ie: u16,
    pub iflags: u16,
    pub ime: bool,
}

impl Default for InterruptController {
    fn default() -> Self {
        Self {
            ie: 0,
            iflags: 0,
            ime: false,
        }
    }
}

impl InterruptController {
    pub fn request(&mut self, mask: u16) {
        self.iflags |= mask;
    }

    pub fn acknowledge(&mut self, mask: u16) {
        self.iflags &= !mask;
    }

    pub fn pending(&self) -> u16 {
        self.ie & self.iflags
    }

    pub fn irq_pending(&self) -> bool {
        self.ime && self.pending() != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiosResult {
    pub returned: bool,
    pub next_pc: Option<u32>,
    pub next_thumb: bool,
}

impl BiosResult {
    pub const returned: Self = Self {
        returned: true,
        next_pc: None,
        next_thumb: false,
    };

    pub const non_returning: Self = Self {
        returned: false,
        next_pc: None,
        next_thumb: false,
    };
}

pub fn swi_number(raw: u32, thumb: bool) -> u8 {
    if thumb {
        (raw & 0xff) as u8
    } else {
        ((raw >> 16) & 0xff) as u8
    }
}

pub fn reset_state(cpu: &mut crate::cpu::Cpu, power: &mut PowerState) {
    cpu.switch_mode(CpuMode::Supervisor);
    cpu.r[REG_SP] = 0x0300_7fe0;
    cpu.r[REG_LR] = 0;
    cpu.switch_mode(CpuMode::Irq);
    cpu.r[REG_SP] = 0x0300_7fa0;
    cpu.r[REG_LR] = 0;
    cpu.switch_mode(CpuMode::System);
    cpu.r[REG_SP] = 0x0300_7f00;
    cpu.r[0..13].fill(0);
    cpu.r[REG_PC] = 0x0800_0000;
    cpu.set_thumb(false);
    cpu.cpsr = (cpu.cpsr & !(0xff | (1 << 5))) | CpuMode::System as u32;
    *power = PowerState::Running;
}

pub fn execute_swi(
    cpu: &mut crate::cpu::Cpu,
    power: &mut PowerState,
    interrupts: &mut InterruptController,
    ewram: &mut [u8],
    iwram: &mut [u8],
    palette: &mut [u8],
    vram: &mut [u8],
    oam: &mut [u8],
    swi: BiosSwi,
) -> BiosResult {
    match swi {
        BiosSwi::SoftReset => {
            let target_flag = iwram.get(0x7ffa).copied().unwrap_or(0);
            reset_state(cpu, power);
            if target_flag == 0 {
                cpu.r[REG_PC] = 0x0800_0000;
            } else {
                cpu.r[REG_PC] = 0x0200_0000;
            }
            BiosResult::non_returning
        }
        BiosSwi::RegisterRamReset => {
            let flags = cpu.r[0];
            if flags & 1 != 0 {
                ewram.fill(0);
            }
            if flags & 2 != 0 {
                iwram[..0x7e00.min(iwram.len())].fill(0);
            }
            if flags & 4 != 0 {
                palette.fill(0);
            }
            if flags & 8 != 0 {
                vram.fill(0);
            }
            if flags & 16 != 0 {
                oam.fill(0);
            }
            if flags & 32 != 0 {
                interrupts.ie = 0;
                interrupts.iflags = 0;
            }
            if flags & 64 != 0 {
                // Sound state is modeled by the runtime APU and reset by its owner.
            }
            if flags & 128 != 0 {
                interrupts.ime = false;
            }
            BiosResult::returned
        }
        BiosSwi::Halt => {
            *power = PowerState::Halted;
            BiosResult::non_returning
        }
        BiosSwi::Stop => {
            *power = PowerState::Stopped;
            BiosResult::non_returning
        }
        BiosSwi::IntrWait => {
            let discard_old = cpu.r[0] != 0;
            let mask = cpu.r[1] as u16;
            if discard_old {
                interrupts.acknowledge(mask);
            }
            interrupts.ime = true;
            if interrupts.pending() & mask != 0 {
                interrupts.acknowledge(mask);
                BiosResult::returned
            } else {
                *power = PowerState::Halted;
                BiosResult::non_returning
            }
        }
        BiosSwi::VBlankIntrWait => {
            cpu.r[0] = 1;
            cpu.r[1] = IRQ_VBLANK as u32;
            execute_swi(
                cpu,
                power,
                interrupts,
                ewram,
                iwram,
                palette,
                vram,
                oam,
                BiosSwi::IntrWait,
            )
        }
    }
}

pub fn service_pending_irq(cpu: &mut crate::cpu::Cpu, interrupts: &mut InterruptController) -> bool {
    if !interrupts.irq_pending() || cpu.cpsr & (1 << 7) != 0 {
        return false;
    }
    let return_address = cpu.r[REG_PC];
    let old_cpsr = cpu.cpsr;
    cpu.switch_mode(CpuMode::Irq);
    cpu.set_spsr(old_cpsr);
    cpu.cpsr |= 1 << 7;
    cpu.set_thumb(false);
    cpu.r[REG_LR] = return_address.wrapping_add(if old_cpsr & (1 << 5) != 0 { 4 } else { 0 });
    cpu.r[REG_PC] = 0x18;
    *interrupts = InterruptController { iflags: interrupts.iflags, ..*interrupts };
    true
}

pub fn in_range(address: u32, start: u32, end: u32) -> bool {
    (start..=end).contains(&address)
}
