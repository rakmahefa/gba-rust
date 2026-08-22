use super::Runtime;
use crate::arm7tdmi;
use crate::bios::PowerState;
use crate::cpu::REG_PC;

impl Runtime {
    pub fn step_recompiled(&mut self, cycles: u32) {
        self.cycles = self.cycles.wrapping_add(cycles as u64);
        self.tick_timers(cycles);
    }

    pub fn tick(&mut self, cycles: u32) {
        self.step_recompiled(cycles);
    }

    fn tick_timers(&mut self, cycles: u32) {
        let mut cascade_edges = 0u32;
        for index in 0..self.timers.len() {
            let cascade = index != 0 && self.timers[index].control().cascade;
            let overflows = if cascade {
                self.timers[index].tick_cascade(cascade_edges)
            } else {
                self.timers[index].tick_cycles(cycles)
            };

            if overflows == 0 {
                cascade_edges = 0;
                continue;
            }

            if self.timers[index].control().irq {
                let mask = 1 << (3 + index);
                // Timer-generated IRQs become pending hardware state. The
                // generated-block dispatcher consumes them at the next
                // architectural boundary rather than mutating CPU mode inside
                // an instruction.
                self.interrupts.request(mask);
                self.wake_from_interrupt(mask);
            }
            cascade_edges = overflows;
        }
    }

    pub fn trace_recompiled(&mut self, _address: u32, _raw: u32) {
        self.step_recompiled(1);
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
                self.service_interrupts();
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
