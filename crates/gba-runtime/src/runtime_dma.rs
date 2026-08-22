use crate::dma::{DmaController, DmaTrigger};
use crate::scheduler::EventKind;
use super::Runtime;

const DMA_BASES: [u32; 4] = [0x0400_00b0, 0x0400_00bc, 0x0400_00c8, 0x0400_00d4];

impl Runtime {
    fn io_word(&self, address: u32) -> u32 {
        u32::from_le_bytes([
            *self.io.get(&address).unwrap_or(&0),
            *self.io.get(&(address + 1)).unwrap_or(&0),
            *self.io.get(&(address + 2)).unwrap_or(&0),
            *self.io.get(&(address + 3)).unwrap_or(&0),
        ])
    }

    fn io_half(&self, address: u32) -> u16 {
        u16::from_le_bytes([
            *self.io.get(&address).unwrap_or(&0),
            *self.io.get(&(address + 1)).unwrap_or(&0),
        ])
    }

    fn set_io_word(&mut self, address: u32, value: u32) {
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.io.insert(address + offset as u32, byte);
        }
    }

    fn set_io_half(&mut self, address: u32, value: u16) {
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.io.insert(address + offset as u32, byte);
        }
    }

    pub(crate) fn sync_dma_registers(&mut self) {
        for (channel, base) in DMA_BASES.into_iter().enumerate() {
            let source = self.io_word(base);
            let destination = self.io_word(base + 4);
            let count = self.io_half(base + 8);
            let control = self.io_half(base + 10);
            let previous_control = self.dma.channels[channel].control;
            self.dma.channels[channel].write_source(source);
            self.dma.channels[channel].write_destination(destination);
            self.dma.channels[channel].write_count(count, channel);
            self.dma.channels[channel].write_control(control, channel);

            let became_enabled = previous_control & 0x8000 == 0 && control & 0x8000 != 0;
            if became_enabled && matches!(self.dma.channels[channel].trigger(), DmaTrigger::Immediate) {
                self.dma.request_immediate(channel);
                self.scheduler.schedule_in(2, EventKind::DmaArbitrate);
            }
        }
    }

    pub(crate) fn service_dma_arbitration(&mut self) {
        self.sync_dma_registers();
        if self.dma.active().is_some() {
            return;
        }
        let Some(transfer) = self.dma.begin_selected(self.scheduler.now(), self.waitcnt) else {
            return;
        };
        self.execute_dma_transfer(transfer.channel, transfer.source, transfer.destination, transfer.count, transfer.width);
        self.scheduler.schedule_at(
            self.dma.busy_until(),
            EventKind::DmaComplete { channel: transfer.channel as u8 },
        );
    }

    fn execute_dma_transfer(
        &mut self,
        channel: usize,
        source: u32,
        destination: u32,
        count: u32,
        width: u32,
    ) {
        let mut current_source = source;
        let mut current_destination = destination;
        for _ in 0..count {
            if width == 4 {
                let value = self.read32(current_source);
                self.write32(current_destination, value);
            } else {
                let value = self.read16(current_source);
                self.write16(current_destination, value);
            }
            self.dma.channels[channel].advance_addresses();
            current_source = self.dma.channels[channel].current_source();
            current_destination = self.dma.channels[channel].current_destination();
        }

        self.dma.channels[channel].source = current_source;
        self.dma.channels[channel].destination = current_destination;
        let base = DMA_BASES[channel];
        self.set_io_word(base, current_source);
        self.set_io_word(base + 4, current_destination);
    }

    pub(crate) fn complete_dma(&mut self, channel: u8) {
        if channel >= 4 {
            return;
        }
        let Some(active) = self.dma.active() else {
            self.interrupts.request(1 << (8 + channel));
            return;
        };
        if active != channel as usize {
            return;
        }
        let irq = self.dma.channels[active].control & 0x4000 != 0;
        let Some(completed) = self.dma.complete() else {
            return;
        };
        debug_assert_eq!(completed, active);

        let base = DMA_BASES[active];
        self.set_io_half(base + 8, self.dma.channels[active].count);
        self.set_io_half(base + 10, self.dma.channels[active].control);

        if irq {
            self.interrupts.request(1 << (8 + active));
        }
        if self.dma.select_next().is_some() {
            self.service_dma_arbitration();
        }
    }

    pub(crate) fn trigger_dma(&mut self, trigger: DmaTrigger) {
        self.sync_dma_registers();
        self.dma.request_trigger(trigger);
        self.service_dma_arbitration();
    }

    pub(crate) fn dma_bus_busy(&self) -> bool {
        self.dma.is_busy(self.scheduler.now())
    }

    pub fn dma_controller(&self) -> &DmaController {
        &self.dma
    }

    pub fn dma_controller_mut(&mut self) -> &mut DmaController {
        &mut self.dma
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmio_devices::{DMA0CNT_H, DMA0CNT_L, DMA0DAD, DMA0SAD};
    use crate::{bus, IRQ_DMA0};

    fn program_channel(runtime: &mut Runtime, base: u32, source: u32, destination: u32, count: u16, control: u16) {
        runtime.write32(base, source);
        runtime.write32(base + 4, destination);
        runtime.write16(base + 8, count);
        runtime.write16(base + 10, control);
    }

    #[test]
    fn immediate_dma_copies_halfwords_and_disables_after_completion() {
        let mut runtime = Runtime::new();
        runtime.write16(bus::EWRAM_START, 0x1234);
        runtime.write16(bus::EWRAM_START + 2, 0xabcd);
        program_channel(&mut runtime, DMA0SAD.address, bus::EWRAM_START, bus::EWRAM_START + 0x100, 2, 0x8000 | 0x4000);

        runtime.advance_cycles(2);
        assert_eq!(runtime.read16(bus::EWRAM_START + 0x100), 0x1234);
        assert_eq!(runtime.read16(bus::EWRAM_START + 0x102), 0xabcd);
        assert_eq!(runtime.dma.active(), Some(0));
        assert!(runtime.dma.busy_until() > runtime.scheduler.now());

        let remaining = runtime.dma.busy_until() - runtime.scheduler.now();
        runtime.advance_cycles(remaining as u32);
        assert_eq!(runtime.dma.active(), None);
        assert_eq!(runtime.read16(DMA0CNT_L.address), 0);
        assert_eq!(runtime.read16(DMA0CNT_H.address), 0);
        assert_eq!(runtime.read32(DMA0SAD.address), bus::EWRAM_START + 4);
        assert_eq!(runtime.read32(DMA0DAD.address), bus::EWRAM_START + 0x104);
        assert_ne!(runtime.interrupts.iflags & IRQ_DMA0, 0);
    }

    #[test]
    fn simultaneous_requests_are_granted_by_channel_priority() {
        let mut runtime = Runtime::new();
        runtime.write16(bus::EWRAM_START, 0x1111);
        runtime.write16(bus::EWRAM_START + 2, 0x2222);
        program_channel(&mut runtime, DMA0SAD.address, bus::EWRAM_START, bus::EWRAM_START + 0x100, 1, 0x8000);
        program_channel(&mut runtime, DMA0SAD.address + 0x0c, bus::EWRAM_START + 2, bus::EWRAM_START + 0x102, 1, 0x8000);

        runtime.advance_cycles(2);
        assert_eq!(runtime.dma.active(), Some(0));
        assert_eq!(runtime.read16(bus::EWRAM_START + 0x100), 0x1111);
        let completion = runtime.dma.busy_until();
        runtime.advance_cycles((completion - runtime.scheduler.now()) as u32);
        assert_eq!(runtime.dma.active(), Some(1));
        assert_eq!(runtime.read16(bus::EWRAM_START + 0x102), 0x2222);
    }

    #[test]
    fn hblank_repeat_dma_retriggers_without_reenabling_the_channel() {
        let mut runtime = Runtime::new();
        runtime.write16(bus::EWRAM_START, 0xbeef);
        let repeat_hblank = 0x8000 | 0x0200 | 0x2000 | 0x4000;
        program_channel(&mut runtime, DMA0SAD.address, bus::EWRAM_START, bus::EWRAM_START + 0x200, 1, repeat_hblank);

        runtime.advance_cycles(1006);
        assert_eq!(runtime.read16(bus::EWRAM_START + 0x200), 0xbeef);
        let first_completion = runtime.dma.busy_until();
        runtime.advance_cycles((first_completion - runtime.scheduler.now()) as u32);
        assert_eq!(runtime.dma.active(), None);
        assert_ne!(runtime.read16(DMA0CNT_H.address) & 0x8000, 0);
    }
}
