use super::bios::{
    execute_swi as execute_bios_swi, service_pending_irq, BiosMemory, BiosResult, BiosSwi,
    PowerState, IRQ_VBLANK,
};
use super::Runtime;

impl Runtime {
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

    pub fn frame(&mut self) {
        self.ppu.frame();
        self.request_interrupt(IRQ_VBLANK);
    }
}
