use std::collections::HashMap;

use super::arm7tdmi::{self, Nzcv, ShiftKind, ShiftResult};
use super::bios::{
    execute_swi as execute_bios_swi, service_pending_irq, BiosMemory, BiosResult, BiosSwi,
    InterruptController, PowerState, HALTCNT, IE, IF, IME, KEYINPUT, WAITCNT,
};
use super::cartridge::Cartridge;
use super::cpu::{Cpu, CpuMode, ExceptionKind, REG_LR, REG_PC};
use super::{Apu, Ppu};

const EWRAM_LEN: usize = 0x40000;
const IWRAM_LEN: usize = 0x8000;
const PALETTE_LEN: usize = 0x400;
const VRAM_LEN: usize = 0x18000;
const OAM_LEN: usize = 0x400;
const KEYINPUT_DEFAULT: u16 = 0x03ff;
const KEYINPUT_HIGH: u32 = KEYINPUT + 1;
const WAITCNT_HIGH: u32 = WAITCNT + 1;

#[derive(Debug, Default)]
pub struct Runtime {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub apu: Apu,
    pub cartridge: Option<Cartridge>,
    pub io: HashMap<u32, u8>,
    pub ewram: [u8; EWRAM_LEN],
    pub iwram: [u8; IWRAM_LEN],
    pub palette: [u8; PALETTE_LEN],
    pub vram: [u8; VRAM_LEN],
    pub oam: [u8; OAM_LEN],
    pub interrupts: InterruptController,
    pub power: PowerState,
    pub waitcnt: u16,
    pub postflg: u8,
    pub keyinput: u16,
    pub dispstat: u16,
    pub cycles: u64,
}

impl Runtime {
    pub fn new() -> Self {
        let mut runtime = Self::default();
        runtime.keyinput = KEYINPUT_DEFAULT;
        runtime
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

    pub fn bios_swi(&mut self, swi: BiosSwi) -> BiosResult {
        let mut memory = BiosMemory {
            ewram: &mut self.ewram,
            iwram: &mut self.iwram,
            palette: &mut self.palette,
            vram: &mut self.vram,
            oam: &mut self.oam,
        };
        execute_bios_swi(
            &mut self.cpu,
            &mut self.power,
            &mut self.interrupts,
            &mut memory,
            swi,
        )
    }

    pub fn bios_swi_number(&mut self, raw: u32, thumb: bool) -> Option<BiosResult> {
        let number = super::bios::swi_number(raw, thumb);
        BiosSwi::from_number(number).map(|swi| self.bios_swi(swi))
    }

    pub fn request_interrupt(&mut self, mask: u16) {
        self.interrupts.request(mask);
        self.wake_from_interrupt(mask);
        self.service_interrupts();
    }

    pub fn service_interrupts(&mut self) -> bool {
        if self.power == PowerState::Stopped {
            return false;
        }
        service_pending_irq(&mut self.cpu, &self.interrupts)
    }

    pub fn wake_from_interrupt(&mut self, mask: u16) {
        if self.power == PowerState::Halted && self.interrupts.ie & mask != 0 {
            self.power = PowerState::Running;
        }
    }

    pub fn step_recompiled(&mut self, cycles: u32) {
        self.cycles = self.cycles.wrapping_add(cycles as u64);
        if self.power != PowerState::Stopped {
            self.service_interrupts();
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        self.step_recompiled(cycles);
    }

    pub fn trace_recompiled(&mut self, _address: u32, _raw: u32) {
        self.step_recompiled(1);
    }

    pub fn frame(&mut self) {
        self.ppu.frame();
        self.request_interrupt(super::bios::IRQ_VBLANK);
    }

    pub fn read8(&self, address: u32) -> u8 {
        match address {
            0x0000_0000..=0x0000_3fff => 0xff,
            0x0200_0000..=0x0203_ffff => self.ewram[(address - 0x0200_0000) as usize],
            0x0300_0000..=0x0300_7fff => self.iwram[(address - 0x0300_0000) as usize],
            0x0400_0000..=0x0400_03ff => self.read_mmio8(address),
            0x0500_0000..=0x0500_03ff => self.palette[(address - 0x0500_0000) as usize],
            0x0600_0000..=0x0601_7fff => self.vram[(address - 0x0600_0000) as usize],
            0x0700_0000..=0x0700_03ff => self.oam[(address - 0x0700_0000) as usize],
            0x0800_0000..0x0e00_0000 => self
                .cartridge
                .as_ref()
                .and_then(|c| c.rom.get((address - 0x0800_0000) as usize))
                .copied()
                .unwrap_or(0xff),
            0x0e00_0000..=0x0e00_ffff => self
                .cartridge
                .as_ref()
                .map(|c| c.save.read((address - 0x0e00_0000) as usize))
                .unwrap_or(0xff),
            _ => *self.io.get(&address).unwrap_or(&0),
        }
    }

    fn read_mmio8(&self, address: u32) -> u8 {
        match address {
            0x0400_0004 => self.dispstat as u8,
            0x0400_0005 => (self.dispstat >> 8) as u8,
            KEYINPUT => self.keyinput as u8,
            KEYINPUT_HIGH => (self.keyinput >> 8) as u8,
            IE => self.interrupts.ie as u8,
            0x0400_0201 => (self.interrupts.ie >> 8) as u8,
            IF => self.interrupts.iflags as u8,
            0x0400_0203 => (self.interrupts.iflags >> 8) as u8,
            WAITCNT => self.waitcnt as u8,
            WAITCNT_HIGH => (self.waitcnt >> 8) as u8,
            IME => u8::from(self.interrupts.ime),
            0x0400_0300 => self.postflg,
            HALTCNT => 0,
            _ => *self.io.get(&address).unwrap_or(&0),
        }
    }

    pub fn read16(&self, address: u32) -> u16 {
        if (0x0400_0000..=0x0400_03ff).contains(&address) {
            u16::from_le_bytes([
                self.read_mmio8(address),
                self.read_mmio8(address.wrapping_add(1)),
            ])
        } else {
            u16::from_le_bytes([self.read8(address), self.read8(address.wrapping_add(1))])
        }
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
        match address {
            0x0200_0000..=0x0203_ffff => {
                self.ewram[(address - 0x0200_0000) as usize] = value;
            }
            0x0300_0000..=0x0300_7fff => {
                self.iwram[(address - 0x0300_0000) as usize] = value;
            }
            0x0400_0000..=0x0400_03ff => self.write_mmio8(address, value),
            0x0500_0000..=0x0500_03ff => {
                self.palette[(address - 0x0500_0000) as usize] = value;
            }
            0x0600_0000..=0x0601_7fff => {
                self.vram[(address - 0x0600_0000) as usize] = value;
            }
            0x0700_0000..=0x0700_03ff => {
                self.oam[(address - 0x0700_0000) as usize] = value;
            }
            0x0e00_0000..=0x0e00_ffff => {
                if let Some(cartridge) = self.cartridge.as_mut() {
                    cartridge
                        .save
                        .write((address - 0x0e00_0000) as usize, value);
                }
            }
            _ => {
                self.io.insert(address, value);
            }
        }
    }

    fn write_mmio8(&mut self, address: u32, value: u8) {
        match address {
            0x0400_0004 => self.dispstat = (self.dispstat & 0xff00) | value as u16,
            0x0400_0005 => self.dispstat = (self.dispstat & 0x00ff) | ((value as u16) << 8),
            KEYINPUT => {}
            KEYINPUT_HIGH => {}
            IE => self.interrupts.ie = (self.interrupts.ie & 0xff00) | value as u16,
            0x0400_0201 => {
                self.interrupts.ie = (self.interrupts.ie & 0x00ff) | ((value as u16) << 8)
            }
            IF => self.interrupts.acknowledge(value as u16),
            0x0400_0203 => self.interrupts.acknowledge((value as u16) << 8),
            WAITCNT => self.waitcnt = (self.waitcnt & 0xff00) | value as u16,
            WAITCNT_HIGH => {
                self.waitcnt = (self.waitcnt & 0x00ff) | ((value as u16) << 8)
            }
            IME => {
                self.interrupts.ime = value & 1 != 0;
                if self.interrupts.ime {
                    self.service_interrupts();
                }
            }
            0x0400_0300 => self.postflg = value & 1,
            HALTCNT => {
                if value & 0x80 != 0 {
                    self.power = PowerState::Stopped;
                } else {
                    self.power = PowerState::Halted;
                }
            }
            _ => self.io.insert(address, value),
        }
    }

    pub fn write16(&mut self, address: u32, value: u16) {
        if address == IF {
            self.interrupts.acknowledge(value);
            return;
        }
        if address == IE {
            self.interrupts.ie = value;
            self.service_interrupts();
            return;
        }
        if address == IME {
            self.interrupts.ime = value & 1 != 0;
            if self.interrupts.ime {
                self.service_interrupts();
            }
            return;
        }
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte);
        }
    }

    pub fn write32(&mut self, address: u32, value: u32) {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte);
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
        self.power = PowerState::Halted;
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
            if self.power == PowerState::Stopped {
                return Err("runtime is stopped");
            }
            if self.power == PowerState::Halted {
                self.step_recompiled(1);
                if self.power == PowerState::Halted {
                    return Err("runtime is halted");
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
    use crate::bios::{BiosSwi, IRQ_HBLANK, IRQ_VBLANK};
    use crate::cpu::{CPSR_C, CPSR_N, CPSR_V, CPSR_Z, CpuMode, REG_LR, REG_PC, REG_SP};

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
        assert_eq!(runtime.cpu.spsr(), Some(old));
        let result = runtime.exception_return(0x0800_0104).expect("exception return");
        assert_eq!(result, (0x0800_0104, false));
        assert_eq!(runtime.mode(), CpuMode::System);
        assert_eq!(runtime.read_reg(REG_SP), 0x1000);
        assert_eq!(runtime.read_reg(REG_LR), 0x2000);
    }

    #[test]
    fn generated_engine_dispatches_iteratively_without_recursive_calls() {
        let mut runtime = Runtime::new();
        let result = runtime.run_generated(0x0800_0000, false, Some(10_000), |rt, address, thumb| {
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

    #[test]
    fn mmio_interrupt_registers_are_backed_by_runtime_state() {
        let mut runtime = Runtime::new();
        runtime.write16(IE, IRQ_VBLANK);
        runtime.write16(IME, 1);
        runtime.request_interrupt(IRQ_VBLANK);
        assert_eq!(runtime.read16(IE), IRQ_VBLANK);
        assert_eq!(runtime.read16(IF), IRQ_VBLANK);
        assert_eq!(runtime.mode(), CpuMode::Irq);
        assert_eq!(runtime.read_reg(REG_PC), 0x18);
    }

    #[test]
    fn mmio_if_write_acknowledges_only_requested_bits() {
        let mut runtime = Runtime::new();
        runtime.interrupts.iflags = IRQ_VBLANK | IRQ_HBLANK;
        runtime.write16(IF, IRQ_VBLANK);
        assert_eq!(runtime.read16(IF), IRQ_HBLANK);
    }

    #[test]
    fn halt_and_stop_mmio_change_runtime_power_state() {
        let mut runtime = Runtime::new();
        runtime.write8(HALTCNT, 0);
        assert_eq!(runtime.power, PowerState::Halted);
        runtime.request_interrupt(IRQ_VBLANK);
        assert_eq!(runtime.power, PowerState::Running);
        runtime.write8(HALTCNT, 0x80);
        assert_eq!(runtime.power, PowerState::Stopped);
    }

    #[test]
    fn bios_swi_updates_runtime_power_and_memory() {
        let mut runtime = Runtime::new();
        runtime.iwram[0x0100] = 0xaa;
        runtime.cpu.r[0] = 2;
        let result = runtime.bios_swi(BiosSwi::RegisterRamReset);
        assert_eq!(result, BiosResult::RETURNED);
        assert_eq!(runtime.iwram[0x0100], 0);
        runtime.bios_swi(BiosSwi::Halt);
        assert_eq!(runtime.power, PowerState::Halted);
    }

    #[test]
    fn vblank_frame_request_reaches_the_integrated_irq_path() {
        let mut runtime = Runtime::new();
        runtime.write16(IE, IRQ_VBLANK);
        runtime.write16(IME, 1);
        runtime.frame();
        assert_eq!(runtime.mode(), CpuMode::Irq);
        assert_eq!(runtime.read_reg(REG_PC), 0x18);
    }
}
