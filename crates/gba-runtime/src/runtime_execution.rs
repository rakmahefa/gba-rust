use super::Runtime;
use crate::arm7tdmi;
use crate::bios::{PowerState, IRQ_DMA0, IRQ_HBLANK, IRQ_VBLANK, IRQ_VCOUNT};
use crate::cpu::REG_PC;
use crate::mmio::{
    DISPSTAT_HBLANK, DISPSTAT_HBLANK_IRQ, DISPSTAT_VBLANK, DISPSTAT_VBLANK_IRQ,
    DISPSTAT_VCOUNT_IRQ, DISPSTAT_VCOUNT_MASK,
};
use crate::scheduler::{
    EventKind, CYCLES_PER_SCANLINE, HBLANK_START_CYCLES, SCANLINES_PER_FRAME,
    VBLANK_START_LINE,
};

impl Runtime {
    /// Advance the machine clock and all time-driven devices to `target`.
    ///
    /// Device state is advanced in monotonic segments between queued events.
    /// This makes CPU cycles, timers and asynchronous hardware observe exactly
    /// the same timeline without injecting an IRQ in the middle of a generated
    /// instruction.
    pub fn advance_cycles(&mut self, cycles: u32) {
        let target = self.scheduler.now().saturating_add(cycles as u64);

        while let Some(event) = self.scheduler.next_event() {
            if event.cycle > target {
                break;
            }

            let delta = event.cycle.saturating_sub(self.scheduler.now());
            self.tick_timers(delta as u32);
            self.scheduler.advance_to(event.cycle);
            self.cycles = event.cycle;
            let event = self.scheduler.pop_event().expect("peeked event must exist");
            self.process_timing_event(event.kind);
        }

        let delta = target.saturating_sub(self.scheduler.now());
        self.tick_timers(delta as u32);
        self.scheduler.advance_to(target);
        self.cycles = target;
    }

    pub fn step_recompiled(&mut self, cycles: u32) {
        self.advance_cycles(cycles);
    }

    pub fn tick(&mut self, cycles: u32) {
        self.advance_cycles(cycles);
    }

    fn tick_timers(&mut self, cycles: u32) {
        if cycles == 0 {
            return;
        }

        let mut cascade_edges = 0u32;
        for index in 0..self.timers.len() {
            let cascade = index != 0 && self.timers[index].control().cascade;
            let overflows = if cascade {
                self.timers[index].tick_cascade(cascade_edges)
            } else {
                self.timers[index].tick_cycles(cycles)
            };

            cascade_edges = overflows;
            if overflows == 0 {
                continue;
            }

            if self.timers[index].control().irq {
                let mask = 1 << (3 + index);
                self.request_interrupt(mask);
            }
        }
    }

    fn process_timing_event(&mut self, event: EventKind) {
        match event {
            EventKind::PpuHBlankStart => {
                self.dispstat |= DISPSTAT_HBLANK;
                if self.dispstat & DISPSTAT_HBLANK_IRQ != 0 {
                    self.request_interrupt(IRQ_HBLANK);
                }
                self.scheduler.schedule_in(
                    CYCLES_PER_SCANLINE,
                    EventKind::PpuHBlankStart,
                );
            }
            EventKind::PpuScanline => {
                self.dispstat &= !DISPSTAT_HBLANK;
                self.vcount = self.vcount.wrapping_add(1);
                if self.vcount >= SCANLINES_PER_FRAME {
                    self.vcount = 0;
                    self.dispstat &= !DISPSTAT_VBLANK;
                    self.ppu.frame();
                }

                if self.vcount == VBLANK_START_LINE {
                    self.dispstat |= DISPSTAT_VBLANK;
                    if self.dispstat & DISPSTAT_VBLANK_IRQ != 0 {
                        self.request_interrupt(IRQ_VBLANK);
                    }
                }

                let compare = ((self.dispstat & DISPSTAT_VCOUNT_MASK) >> 8) as u16;
                if self.vcount == compare {
                    if self.dispstat & DISPSTAT_VCOUNT_IRQ != 0 {
                        self.request_interrupt(IRQ_VCOUNT);
                    }
                }

                self.scheduler.schedule_in(
                    CYCLES_PER_SCANLINE,
                    EventKind::PpuScanline,
                );
            }
            EventKind::PpuVBlankStart => {
                self.dispstat |= DISPSTAT_VBLANK;
                if self.dispstat & DISPSTAT_VBLANK_IRQ != 0 {
                    self.request_interrupt(IRQ_VBLANK);
                }
            }
            EventKind::DmaComplete { channel } => {
                if channel < 4 {
                    self.request_interrupt(IRQ_DMA0 << channel);
                }
            }
            EventKind::IrqSample => {
                let _ = self.service_interrupts();
            }
        }
    }

    pub fn schedule_dma_completion(&mut self, channel: u8, cycles_from_now: u64) {
        assert!(channel < 4, "GBA has four DMA channels");
        self.scheduler
            .schedule_in(cycles_from_now, EventKind::DmaComplete { channel });
    }

    pub fn schedule_irq_sample(&mut self, cycles_from_now: u64) {
        self.scheduler
            .schedule_in(cycles_from_now, EventKind::IrqSample);
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
                self.advance_cycles(1);
                let _ = self.service_interrupts();
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
