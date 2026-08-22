use super::Runtime;
use crate::arm7tdmi::{self, Nzcv, ShiftKind, ShiftResult};
use crate::cpu::{CpuMode, ExceptionKind, REG_LR, REG_PC, REG_SP};

impl Runtime {
    pub fn load_cartridge(&mut self, cartridge: crate::cartridge::Cartridge) {
        self.cartridge = Some(cartridge);
        self.initialize_cartridge_boot_state();
    }

    /// Establish the CPU state handed to cartridge code by the GBA BIOS.
    ///
    /// The BIOS enters the cartridge in privileged System/ARM state and
    /// initializes the user/system, IRQ and Supervisor stack tops in IWRAM.
    /// This boundary is intentionally separate from `Runtime::default()` so
    /// unit tests can still construct a neutral machine state.
    pub fn initialize_cartridge_boot_state(&mut self) {
        self.cpu.switch_mode(CpuMode::System);
        self.cpu.set_thumb(false);
        self.cpu.cpsr = CpuMode::System as u32;
        self.cpu.banked.user_system_sp_lr[0] = 0x0300_7f00;
        self.cpu.banked.irq_sp_lr[0] = 0x0300_7fa0;
        self.cpu.banked.svc_sp_lr[0] = 0x0300_7fe0;
        self.cpu.r[REG_SP] = 0x0300_7f00;
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

    pub fn shift(&self, value: u32, kind: ShiftKind, amount: u8, register_shift: bool) -> ShiftResult {
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

    pub fn enter_exception(&mut self, kind: ExceptionKind) -> (u32, bool) {
        self.raise_exception(kind)
    }

    pub fn return_from_exception(&mut self, target: u32) -> Option<(u32, bool)> {
        self.exception_return(target)
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

    pub fn raise_exception_at_boundary(
        &mut self,
        kind: ExceptionKind,
        resume_address: u32,
        resume_thumb: bool,
    ) -> (u32, bool) {
        let old_cpsr = self.cpu.cpsr;
        let return_address = resume_address.wrapping_add(if resume_thumb { 2 } else { 4 });
        self.cpu.switch_mode(kind.mode());
        self.cpu.set_spsr(old_cpsr);
        self.cpu.cpsr |= kind.masks();
        self.cpu.set_thumb(false);
        self.cpu.r[REG_LR] = return_address;
        self.cpu.r[REG_PC] = kind.vector();
        (kind.vector(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cartridge_boot_state_matches_bios_stack_contract() {
        let mut runtime = Runtime::new();
        runtime.initialize_cartridge_boot_state();

        assert_eq!(runtime.mode(), CpuMode::System);
        assert!(!runtime.cpu.thumb);
        assert_eq!(runtime.cpu.r[REG_PC], 0x0800_0000);
        assert_eq!(runtime.cpu.r[REG_SP], 0x0300_7f00);
        assert_eq!(runtime.cpu.banked.user_system_sp_lr[0], 0x0300_7f00);
        assert_eq!(runtime.cpu.banked.irq_sp_lr[0], 0x0300_7fa0);
        assert_eq!(runtime.cpu.banked.svc_sp_lr[0], 0x0300_7fe0);
    }

    #[test]
    fn loading_cartridge_establishes_boot_state() {
        let mut runtime = Runtime::new();
        runtime.load_cartridge(crate::cartridge::Cartridge::from_rom(vec![0; 4], "saves"));

        assert_eq!(runtime.cpu.r[REG_PC], 0x0800_0000);
        assert_eq!(runtime.cpu.r[REG_SP], 0x0300_7f00);
        assert_eq!(runtime.mode(), CpuMode::System);
    }
}
