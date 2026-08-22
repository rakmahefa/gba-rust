use super::dma::{DmaController, DmaTrigger};
use super::scheduler::EventKind;
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
        if self.dma.active().is_some() { return; }
        let Some(transfer) = self.dma.begin_selected(self.scheduler.now(), self.waitcnt) else { return; };
        self.execute_dma_transfer(transfer.channel, transfer.source, transfer.destination, transfer.count, transfer.width);
        self.scheduler.schedule_at(
            self.dma.busy_until(),
            EventKind::DmaComplete { channel: transfer.channel as u8 },
        );
    }

    fn execute_dma_transfer(&mut self, channel: usize, source: u32, destination: u32, count: u32, width: u32) {
        for _ in 0..count {
            if width == 4 {
                let value = self.read32(source);
                self.write32(destination, value);
            } else {
                let value = self.read16(source);
                self.write16(destination, value);
            }
            self.dma.channels[channel].advance_addresses();
        }

        let final_source = self.dma.channels[channel].current_source();
        let final_destination = self.dma.channels[channel].current_destination();
        self.dma.channels[channel].source = final_source;
        self.dma.channels[channel].destination = final_destination;
        let base = DMA_BASES[channel];
        self.set_io_word(base, final_source);
        self.set_io_word(base + 4, final_destination);
    }

    pub(crate) fn complete_dma(&mut self, channel: u8) {
        if channel >= 4 { return; }
        let Some(active) = self.dma.active() else {
            self.interrupts.request(1 << (8 + channel));
            return;
        };
        if active != channel as usize { return; }
        let irq = self.dma.channels[active].control & 0x4000 != 0;
        self.dma.complete();
        let base = DMA_BASES[active];
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

    pub fn dma_controller(&self) -> &DmaController { &self.dma }
    pub fn dma_controller_mut(&mut self) -> &mut DmaController { &mut self.dma }
}
