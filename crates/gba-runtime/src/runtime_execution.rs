use super::arm7tdmi;
use super::Runtime;

impl Runtime {
    pub fn step_recompiled(&mut self, cycles: u32) {
        self.cycles = self.cycles.wrapping_add(cycles as u64);
        if self.power != super::bios::PowerState::Stopped {
            self.service_interrupts();
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        self.step_recompiled(cycles);
    }

    pub fn trace_recompiled(&mut self, _address: u32, _raw: u32) {
        self.step_recompiled(1);
    }

    pub fn dispatch_mode(&mut self, address: u32, thumb: bool) -> ! {
        self.cpu.set_thumb(thumb);
        self.cpu.r[super::cpu::REG_PC] = address & if thumb { !1 } else { !3 };
        panic!(
            "generated dispatch target {address:#010x} ({}) is not linked yet",
            if thumb { "Thumb" } else { "ARM" }
        )
    }

    pub fn dispatch_exchange(&mut self, target: u32) -> ! {
        let (address, thumb) = arm7tdmi::exchange_target(target);
        self.cpu.set_thumb(thumb);
        self.cpu.r[super::cpu::REG_PC] = address;
        panic!("generated BX target {target:#010x} is not linked yet")
    }

    pub fn dispatch(&mut self, address: u32) -> ! {
        self.dispatch_mode(address, self.cpu.thumb)
    }

    pub fn halt(&mut self) -> ! {
        self.power = super::bios::PowerState::Halted;
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
            if self.power == super::bios::PowerState::Stopped {
                return Err("runtime is stopped");
            }
            if self.power == super::bios::PowerState::Halted {
                self.step_recompiled(1);
                if self.power == super::bios::PowerState::Halted {
                    return Err("runtime is halted");
                }
            }
            self.cpu.set_thumb(next.1);
            self.cpu.r[super::cpu::REG_PC] = next.0;
            next = dispatch(self, next.0, next.1)?;
            steps = steps.wrapping_add(1);
        }
    }

    pub fn exchange_target_for_dispatch(&mut self, target: u32) -> (u32, bool) {
        let (address, thumb) = arm7tdmi::exchange_target(target);
        self.cpu.set_thumb(thumb);
        self.cpu.r[super::cpu::REG_PC] = address;
        (address, thumb)
    }
}
