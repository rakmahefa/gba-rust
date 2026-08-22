use super::Runtime;
use crate::bios::{
    execute_swi as execute_bios_swi, service_pending_irq, BiosMemory, BiosResult, BiosSwi,
    PowerState, IRQ_VBLANK,
};
use crate::cpu::{ExceptionKind, REG_LR, REG_PC};

impl Runtime {
    pub fn bios_swi(&mut self, swi: BiosSwi) -> BiosResult {
        self.raise_exception(ExceptionKind::SoftwareInterrupt);

        let mut memory = BiosMemory {
            ewram: &mut self.ewram,
            iwram: &mut self.iwram,
            palette: &mut self.palette,
            vram: &mut self.vram,
            oam: &mut self.oam,
        };
        let result = execute_bios_swi(
            &mut self.cpu,
            &mut self.power,
            &mut self.interrupts,
            &mut memory,
            swi,
        );

        if result.returned {
            let return_address = self.cpu.read_reg(REG_LR);
            let _ = self.exception_return(return_address);
        } else if let Some(next_pc) = result.next_pc {
            self.cpu.r[REG_PC] = next_pc & !3;
            self.cpu.set_thumb(result.next_thumb);
        }

        result
    }

    pub fn bios_swi_number(&mut self, raw: u32, thumb: bool) -> Option<BiosResult> {
        let number = crate::bios::swi_number(raw, thumb);
        BiosSwi::from_number(number).map(|swi| self.bios_swi(swi))
    }

    pub fn execute_bios_swi_comment(
        &mut self,
        comment: u32,
        thumb: bool,
    ) -> Result<BiosResult, String> {
        let number = crate::bios::swi_number(comment, thumb);
        self.bios_swi_number(comment, thumb).ok_or_else(|| {
            format!(
                "generated BIOS SWI number is not implemented: number={number:#04x} comment={comment:#010x} thumb={thumb}"
            )
        })
    }

    pub fn request_interrupt(&mut self, mask: u16) {
        self.interrupts.request(mask);
        self.wake_from_interrupt(mask);
        self.service_interrupts();
    }

    pub fn generated_irq_pending(&mut self) -> bool {
        let pending = self.interrupts.pending();
        if pending == 0 {
            return false;
        }
        self.wake_from_interrupt(pending);
        self.interrupts.ime && self.cpu.cpsr & (1 << 7) == 0
    }

    pub fn service_interrupts(&mut self) -> bool {
        if self.power == PowerState::Stopped {
            return false;
        }
        service_pending_irq(&mut self.cpu, &self.interrupts)
    }

    pub fn deliver_pending_interrupt(&mut self) -> Option<(u32, bool)> {
        if self.power == PowerState::Stopped {
            return None;
        }

        let pending = self.interrupts.pending();
        if pending == 0 {
            return None;
        }

        self.wake_from_interrupt(pending);
        self.service_interrupts()
            .then_some((self.cpu.r[REG_PC], self.cpu.thumb))
    }

    pub fn wake_from_interrupt(&mut self, mask: u16) {
        if self.power == PowerState::Halted && self.interrupts.ie & mask != 0 {
            self.power = PowerState::Running;
        }
    }

    pub fn frame(&mut self) {
        self.ppu.frame();
        self.request_interrupt(IRQ_VBLANK);
    }
}
