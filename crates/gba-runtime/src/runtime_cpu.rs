use crate::arm7tdmi::{self, Nzcv, ShiftKind, ShiftResult};
use crate::cpu::{CpuMode, ExceptionKind, REG_LR, REG_PC};
use super::Runtime;

impl Runtime {
    pub fn load_cartridge(&mut self, cartridge: crate::cartridge::Cartridge) {
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
}
