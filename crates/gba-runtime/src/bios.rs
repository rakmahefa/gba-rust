use crate::cpu::{CpuMode, REG_LR, REG_PC, REG_SP};

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

pub const IE: u32 = 0x0400_0200;
pub const IF: u32 = 0x0400_0202;
pub const WAITCNT: u32 = 0x0400_0204;
pub const IME: u32 = 0x0400_0208;
pub const POSTFLG: u32 = 0x0400_0300;
pub const HALTCNT: u32 = 0x0400_0301;
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

const CPSR_I: u32 = 1 << 7;
const CPSR_T: u32 = 1 << 5;

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
    pub const RETURNED: Self = Self {
        returned: true,
        next_pc: None,
        next_thumb: false,
    };

    pub const NON_RETURNING: Self = Self {
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
    let target_mode = CpuMode::System;
    cpu.switch_mode(CpuMode::Supervisor);
    cpu.r[REG_SP] = 0x0300_7fe0;
    cpu.r[REG_LR] = 0;
    cpu.switch_mode(CpuMode::Irq);
    cpu.r[REG_SP] = 0x0300_7fa0;
    cpu.r[REG_LR] = 0;
    cpu.switch_mode(target_mode);
    cpu.r[REG_SP] = 0x0300_7f00;
    cpu.r[0..13].fill(0);
    cpu.r[REG_PC] = 0x0800_0000;
    cpu.set_thumb(false);
    cpu.cpsr = (cpu.cpsr & !(0xff | CPSR_T)) | CpuMode::System as u32;
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
            cpu.r[REG_PC] = if target_flag == 0 {
                0x0800_0000
            } else {
                0x0200_0000
            };
            BiosResult::NON_RETURNING
        }
        BiosSwi::RegisterRamReset => {
            let flags = cpu.r[0];
            if flags & 1 != 0 {
                ewram.fill(0);
            }
            if flags & 2 != 0 {
                let reset_len = 0x7e00.min(iwram.len());
                iwram[..reset_len].fill(0);
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
            if flags & 128 != 0 {
                interrupts.ime = false;
            }
            BiosResult::RETURNED
        }
        BiosSwi::Halt => {
            *power = PowerState::Halted;
            BiosResult::NON_RETURNING
        }
        BiosSwi::Stop => {
            *power = PowerState::Stopped;
            BiosResult::NON_RETURNING
        }
        BiosSwi::IntrWait => {
            let discard_old = cpu.r[0] != 0;
            let mask = cpu.r[1] as u16;
            if discard_old {
                interrupts.acknowledge(mask);
            }
            if interrupts.pending() & mask != 0 {
                interrupts.acknowledge(mask);
                *power = PowerState::Running;
                BiosResult::RETURNED
            } else {
                *power = PowerState::Halted;
                BiosResult::NON_RETURNING
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

pub fn service_pending_irq(cpu: &mut crate::cpu::Cpu, interrupts: &InterruptController) -> bool {
    if !interrupts.irq_pending() || cpu.cpsr & CPSR_I != 0 {
        return false;
    }
    let return_address = cpu.r[REG_PC];
    let old_cpsr = cpu.cpsr;
    cpu.switch_mode(CpuMode::Irq);
    cpu.set_spsr(old_cpsr);
    cpu.cpsr |= CPSR_I;
    cpu.set_thumb(false);
    cpu.r[REG_LR] = return_address;
    cpu.r[REG_PC] = 0x18;
    true
}

pub fn in_range(address: u32, start: u32, end: u32) -> bool {
    (start..=end).contains(&address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{Cpu, CpuMode, REG_PC, REG_SP};

    fn memory() -> ([u8; 0x100], [u8; 0x8000], [u8; 0x400], [u8; 0x18000], [u8; 0x400]) {
        ([0; 0x100], [0; 0x8000], [0; 0x400], [0; 0x18000], [0; 0x400])
    }

    #[test]
    fn swi_number_uses_gba_arm_and_thumb_encodings() {
        assert_eq!(swi_number(0x0004_0000, false), 4);
        assert_eq!(swi_number(0x0000_0005, true), 5);
    }

    #[test]
    fn soft_reset_sets_the_three_gba_default_stacks() {
        let mut cpu = Cpu::default();
        let mut power = PowerState::Stopped;
        reset_state(&mut cpu, &mut power);
        assert_eq!(cpu.mode(), CpuMode::System);
        assert_eq!(cpu.r[REG_SP], 0x0300_7f00);
        assert_eq!(cpu.r[REG_PC], 0x0800_0000);
        assert_eq!(power, PowerState::Running);
    }

    #[test]
    fn register_ram_reset_preserves_the_bios_reserved_iwram_tail() {
        let (mut ewram, mut iwram, mut palette, mut vram, mut oam) = memory();
        iwram[0x7dff] = 0xaa;
        iwram[0x7ffa] = 0x01;
        let mut cpu = Cpu::default();
        cpu.r[0] = 0x02;
        let mut power = PowerState::Running;
        let mut interrupts = InterruptController::default();
        execute_swi(
            &mut cpu,
            &mut power,
            &mut interrupts,
            &mut ewram,
            &mut iwram,
            &mut palette,
            &mut vram,
            &mut oam,
            BiosSwi::RegisterRamReset,
        );
        assert_eq!(iwram[0x7dff], 0);
        assert_eq!(iwram[0x7ffa], 0x01);
    }

    #[test]
    fn soft_reset_reads_the_boot_flag_before_register_ram_reset() {
        let (mut ewram, mut iwram, mut palette, mut vram, mut oam) = memory();
        iwram[0x7ffa] = 1;
        let mut cpu = Cpu::default();
        let mut power = PowerState::Running;
        let mut interrupts = InterruptController::default();
        let result = execute_swi(
            &mut cpu,
            &mut power,
            &mut interrupts,
            &mut ewram,
            &mut iwram,
            &mut palette,
            &mut vram,
            &mut oam,
            BiosSwi::SoftReset,
        );
        assert_eq!(result, BiosResult::NON_RETURNING);
        assert_eq!(cpu.r[REG_PC], 0x0200_0000);
    }

    #[test]
    fn halt_wakes_when_the_requested_interrupt_is_pending() {
        let (mut ewram, mut iwram, mut palette, mut vram, mut oam) = memory();
        let mut cpu = Cpu::default();
        cpu.r[0] = 0;
        cpu.r[1] = IRQ_VBLANK as u32;
        let mut power = PowerState::Halted;
        let mut interrupts = InterruptController {
            ie: IRQ_VBLANK,
            iflags: IRQ_VBLANK,
            ime: false,
        };
        let result = execute_swi(
            &mut cpu,
            &mut power,
            &mut interrupts,
            &mut ewram,
            &mut iwram,
            &mut palette,
            &mut vram,
            &mut oam,
            BiosSwi::IntrWait,
        );
        assert_eq!(result, BiosResult::RETURNED);
        assert_eq!(power, PowerState::Running);
        assert_eq!(interrupts.iflags & IRQ_VBLANK, 0);
    }

    #[test]
    fn irq_entry_switches_to_irq_mode_and_vectors_to_bios() {
        let mut cpu = Cpu::default();
        cpu.r[REG_PC] = 0x0800_0100;
        let interrupts = InterruptController {
            ie: IRQ_VBLANK,
            iflags: IRQ_VBLANK,
            ime: true,
        };
        assert!(service_pending_irq(&mut cpu, &interrupts));
        assert_eq!(cpu.mode(), CpuMode::Irq);
        assert_eq!(cpu.r[REG_PC], 0x18);
        assert_eq!(cpu.r[REG_LR], 0x0800_0100);
    }
}
