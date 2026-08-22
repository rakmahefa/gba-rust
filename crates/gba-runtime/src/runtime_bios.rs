use super::Runtime;
use crate::bios::{
    execute_swi as execute_bios_swi, service_pending_irq, BiosMemory, BiosResult, BiosSwi,
    PowerState, IRQ_VBLANK,
};
use crate::cpu::{ExceptionKind, REG_LR, REG_PC};

const BIOS_TRANSFER_COUNT_MASK: u32 = 0x001f_ffff;
const BIOS_TRANSFER_FILL: u32 = 1 << 24;
const BIOS_CPU_SET_WORD: u32 = 1 << 26;

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

    fn execute_cpu_set(&mut self, fast: bool) -> BiosResult {
        self.raise_exception(ExceptionKind::SoftwareInterrupt);

        let source = self.read_reg(0);
        let destination = self.read_reg(1);
        let control = self.read_reg(2);
        let word = fast || control & BIOS_CPU_SET_WORD != 0;
        let alignment = if word { !3 } else { !1 };
        let mut source_address = source & alignment;
        let mut destination_address = destination & alignment;
        let count = if fast {
            (control & BIOS_TRANSFER_COUNT_MASK).saturating_add(7) & !7
        } else {
            control & BIOS_TRANSFER_COUNT_MASK
        };
        let fill = control & BIOS_TRANSFER_FILL != 0;

        if count != 0 {
            if word {
                let first = self.read32(source_address);
                for index in 0..count {
                    let value = if fill || index == 0 {
                        first
                    } else {
                        source_address = source_address.wrapping_add(4);
                        self.read32(source_address)
                    };
                    self.write32(destination_address, value);
                    destination_address = destination_address.wrapping_add(4);
                }
            } else {
                let first = self.read16(source_address);
                for index in 0..count {
                    let value = if fill || index == 0 {
                        first
                    } else {
                        source_address = source_address.wrapping_add(2);
                        self.read16(source_address)
                    };
                    self.write16(destination_address, value);
                    destination_address = destination_address.wrapping_add(2);
                }
            }
        }

        let return_address = self.cpu.read_reg(REG_LR);
        let _ = self.exception_return(return_address);
        BiosResult::RETURNED
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
        match number {
            0x0b => Ok(self.execute_cpu_set(false)),
            0x0c => Ok(self.execute_cpu_set(true)),
            _ => self.bios_swi_number(comment, thumb).ok_or_else(|| {
                format!(
                    "generated BIOS SWI number is not implemented: number={number:#04x} comment={comment:#010x} thumb={thumb}"
                )
            }),
        }
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
