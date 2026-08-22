use std::collections::HashMap;

use super::arm7tdmi::{self, Nzcv, ShiftKind, ShiftResult};
use super::cartridge::Cartridge;
use super::cpu::{Cpu, CpuMode, ExceptionKind, REG_LR, REG_PC};
use super::{Apu, Ppu};

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
        self.ppu.frame();
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
            if rt.cycles == 10_000 { Err("done") } else { Ok((address, thumb)) }
        });
        assert_eq!(result, Err("done"));
        assert_eq!(runtime.cycles, 10_000);
    }

    #[test]
    fn exchange_target_for_dispatch_updates_thumb_and_pc() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.exchange_target_for_dispatch(0x0800_0101), (0x0800_0100, true));
        assert!(runtime.cpu.thumb);
        assert_eq!(runtime.read_reg(REG_PC), 0x0800_0100);
    }
}
