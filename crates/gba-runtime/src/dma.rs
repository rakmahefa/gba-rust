//! GBA DMA controller state and deterministic bus arbitration.

use crate::bus::{self, BusRegion};

pub const CONTROL_DEST_MASK: u16 = 0x0060;
pub const CONTROL_SRC_MASK: u16 = 0x0180;
pub const CONTROL_REPEAT: u16 = 0x0200;
pub const CONTROL_WORD: u16 = 0x0400;
pub const CONTROL_TIMING_MASK: u16 = 0x3000;
pub const CONTROL_IRQ: u16 = 0x4000;
pub const CONTROL_ENABLE: u16 = 0x8000;
pub const DMA_CONTROL_MASK: u16 = 0xf7e0;
pub const DMA3_CONTROL_MASK: u16 = 0xffe0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaTrigger {
    Immediate,
    VBlank,
    HBlank,
    Special,
}

impl DmaTrigger {
    pub fn from_control(control: u16) -> Self {
        match (control & CONTROL_TIMING_MASK) >> 12 {
            0 => Self::Immediate,
            1 => Self::VBlank,
            2 => Self::HBlank,
            _ => Self::Special,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaAddressMode {
    Increment,
    Decrement,
    Fixed,
    Reload,
}

impl DmaAddressMode {
    fn from_control(value: u16) -> Self {
        match value & 0b11 {
            0 => Self::Increment,
            1 => Self::Decrement,
            2 => Self::Fixed,
            _ => Self::Reload,
        }
    }

    fn step(self, address: u32, width: u32) -> u32 {
        match self {
            Self::Increment | Self::Reload => address.wrapping_add(width),
            Self::Decrement => address.wrapping_sub(width),
            Self::Fixed => address,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaChannel {
    pub source: u32,
    pub destination: u32,
    pub count: u16,
    pub control: u16,
    current_source: u32,
    current_destination: u32,
    reload_destination: u32,
    start_pending: bool,
}

impl Default for DmaChannel {
    fn default() -> Self {
        Self { source: 0, destination: 0, count: 0, control: 0, current_source: 0, current_destination: 0, reload_destination: 0, start_pending: false }
    }
}

impl DmaChannel {
    pub fn enabled(&self) -> bool { self.control & CONTROL_ENABLE != 0 }
    pub fn trigger(&self) -> DmaTrigger { DmaTrigger::from_control(self.control) }
    pub fn width(&self) -> u32 { if self.control & CONTROL_WORD != 0 { 4 } else { 2 } }

    pub fn transfer_count(&self, channel: usize) -> u32 {
        if self.count == 0 {
            if channel == 3 { 0x1_0000 } else { 0x4000 }
        } else {
            u32::from(self.count)
        }
    }

    pub fn request(&mut self) { if self.enabled() { self.start_pending = true; } }
    pub fn pending(&self) -> bool { self.start_pending && self.enabled() }
    pub fn clear_pending(&mut self) { self.start_pending = false; }

    pub fn begin(&mut self) {
        self.current_source = self.source;
        self.current_destination = self.destination;
        self.reload_destination = self.destination;
        self.start_pending = false;
    }

    pub fn current_source(&self) -> u32 { self.current_source }
    pub fn current_destination(&self) -> u32 { self.current_destination }

    pub fn advance_addresses(&mut self) {
        let width = self.width();
        let destination_mode = DmaAddressMode::from_control((self.control & CONTROL_DEST_MASK) >> 5);
        let source_mode = DmaAddressMode::from_control((self.control & CONTROL_SRC_MASK) >> 7);
        self.current_destination = destination_mode.step(self.current_destination, width);
        self.current_source = source_mode.step(self.current_source, width);
    }

    pub fn finish(&mut self) {
        let repeat = self.control & CONTROL_REPEAT != 0;
        if !repeat || matches!(self.trigger(), DmaTrigger::Immediate) {
            self.control &= !CONTROL_ENABLE;
            self.start_pending = false;
            return;
        }

        let destination_mode = DmaAddressMode::from_control((self.control & CONTROL_DEST_MASK) >> 5);
        if matches!(destination_mode, DmaAddressMode::Reload) {
            self.destination = self.reload_destination;
        }
    }

    pub fn write_source(&mut self, value: u32) { self.source = value & 0x0fff_ffff; }
    pub fn write_destination(&mut self, value: u32) { self.destination = value & 0x07ff_ffff; }
    pub fn write_count(&mut self, value: u16, channel: usize) {
        self.count = value & if channel == 3 { 0xffff } else { 0x3fff };
    }
    pub fn write_control(&mut self, value: u16, channel: usize) {
        let mask = if channel == 3 { DMA3_CONTROL_MASK } else { DMA_CONTROL_MASK };
        let was_enabled = self.enabled();
        self.control = value & mask;
        if !was_enabled && self.enabled() && matches!(self.trigger(), DmaTrigger::Immediate) {
            self.start_pending = true;
        }
        if !self.enabled() { self.start_pending = false; }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaTransfer {
    pub channel: usize,
    pub source: u32,
    pub destination: u32,
    pub count: u32,
    pub width: u32,
    pub cycles: u64,
    pub irq: bool,
}

#[derive(Debug, Clone)]
pub struct DmaController {
    pub channels: [DmaChannel; 4],
    active: Option<usize>,
    busy_until: u64,
}

impl Default for DmaController {
    fn default() -> Self {
        Self { channels: std::array::from_fn(|_| DmaChannel::default()), active: None, busy_until: 0 }
    }
}

impl DmaController {
    pub fn active(&self) -> Option<usize> { self.active }
    pub fn busy_until(&self) -> u64 { self.busy_until }
    pub fn is_busy(&self, now: u64) -> bool { self.active.is_some() && now < self.busy_until }

    pub fn request_trigger(&mut self, trigger: DmaTrigger) {
        for channel in &mut self.channels {
            if channel.enabled() && channel.trigger() == trigger { channel.request(); }
        }
    }

    pub fn request_immediate(&mut self, channel: usize) { if channel < 4 { self.channels[channel].request(); } }

    pub fn select_next(&self) -> Option<usize> {
        if self.active.is_some() { return None; }
        (0..4).find(|&channel| self.channels[channel].pending())
    }

    pub fn begin_selected(&mut self, now: u64, waitcnt: u16) -> Option<DmaTransfer> {
        let channel = self.select_next()?;
        self.channels[channel].begin();
        let state = self.channels[channel];
        let count = state.transfer_count(channel);
        let width = state.width();
        let cycles = transfer_cycles(state.current_source, state.current_destination, count, width, waitcnt);
        self.active = Some(channel);
        self.busy_until = now.saturating_add(cycles.max(1));
        Some(DmaTransfer {
            channel,
            source: state.current_source,
            destination: state.current_destination,
            count,
            width,
            cycles: cycles.max(1),
            irq: state.control & CONTROL_IRQ != 0,
        })
    }

    pub fn complete(&mut self) -> Option<usize> {
        let channel = self.active.take()?;
        self.channels[channel].finish();
        self.busy_until = 0;
        Some(channel)
    }
}

fn rom_bank(address: u32) -> u8 {
    match address {
        bus::ROM0_START..=bus::ROM0_END => 0,
        bus::ROM1_START..=bus::ROM1_END => 1,
        _ => 2,
    }
}

fn rom_initial_wait(waitcnt: u16, bank: u8) -> u32 {
    let bits = match bank {
        0 => (waitcnt >> 2) & 0x3,
        1 => (waitcnt >> 5) & 0x3,
        _ => (waitcnt >> 8) & 0x3,
    };
    match bits { 0 => 4, 1 => 3, 2 => 2, _ => 8 }
}

fn rom_sequential_wait(waitcnt: u16, bank: u8) -> u32 {
    match bank {
        0 => if waitcnt & 0x10 != 0 { 1 } else { 2 },
        1 => if waitcnt & 0x80 != 0 { 1 } else { 4 },
        _ => if waitcnt & 0x400 != 0 { 1 } else { 8 },
    }
}

pub fn transfer_cycles(source: u32, destination: u32, count: u32, width: u32, waitcnt: u16) -> u64 {
    if count == 0 { return 0; }
    let src = bus::decode(source).region;
    let dst = bus::decode(destination).region;

    if matches!(src, BusRegion::CartridgeRom) {
        let bank = rom_bank(source);
        let initial = rom_initial_wait(waitcnt, bank);
        let sequential = rom_sequential_wait(waitcnt, bank);
        let dst_cost = destination_cost(dst, width);
        return u64::from(initial + dst_cost)
            + u64::from(count.saturating_sub(1)) * u64::from(sequential + dst_cost);
    }

    let (read, write) = internal_rate(src, dst, width);
    u64::from(read + write) * u64::from(count)
}

fn destination_cost(region: BusRegion, width: u32) -> u32 {
    match (region, width) {
        (BusRegion::Ewram, 2) => 2,
        (BusRegion::Ewram, 4) => 4,
        (BusRegion::Iwram, 2) => 2,
        (BusRegion::Iwram, 4) => 3,
        (BusRegion::Io, 2) => 2,
        (BusRegion::Io, 4) => 3,
        (BusRegion::Palette, 2) => 2,
        (BusRegion::Palette, 4) => 4,
        (BusRegion::Vram, 2) => 2,
        (BusRegion::Vram, 4) => 4,
        (BusRegion::Oam, 2) => 2,
        (BusRegion::Oam, 4) => 3,
        _ => if width == 4 { 4 } else { 2 },
    }
}

fn internal_rate(source: BusRegion, destination: BusRegion, width: u32) -> (u32, u32) {
    match (source, destination, width) {
        (BusRegion::Ewram, BusRegion::Ewram, 2) => (4, 2),
        (BusRegion::Ewram, BusRegion::Ewram, 4) => (8, 4),
        (BusRegion::Ewram, BusRegion::Iwram, 2) => (4, 2),
        (BusRegion::Ewram, BusRegion::Iwram, 4) => (7, 4),
        (BusRegion::Iwram, BusRegion::Ewram, 2) => (4, 2),
        (BusRegion::Iwram, BusRegion::Ewram, 4) => (7, 3),
        (BusRegion::Iwram, BusRegion::Iwram, 2) => (2, 0),
        (BusRegion::Iwram, BusRegion::Iwram, 4) => (2, 0),
        (BusRegion::Iwram, BusRegion::Io, 2) => (2, 0),
        (BusRegion::Iwram, BusRegion::Io, 4) => (2, 0),
        (BusRegion::Iwram, BusRegion::Palette, 2) => (2, 1),
        (BusRegion::Iwram, BusRegion::Palette, 4) => (2, 1),
        (BusRegion::Iwram, BusRegion::Vram, 2) => (2, 1),
        (BusRegion::Iwram, BusRegion::Vram, 4) => (2, 1),
        (BusRegion::Iwram, BusRegion::Oam, 2) => (2, 0),
        (BusRegion::Iwram, BusRegion::Oam, 4) => (2, 0),
        (BusRegion::Ewram, BusRegion::Io, 2) => (6, 1),
        (BusRegion::Ewram, BusRegion::Io, 4) => (12, 2),
        (BusRegion::Ewram, BusRegion::Palette, 2) => (6, 2),
        (BusRegion::Ewram, BusRegion::Palette, 4) => (12, 4),
        (BusRegion::Ewram, BusRegion::Vram, 2) => (6, 2),
        (BusRegion::Ewram, BusRegion::Vram, 4) => (12, 4),
        (BusRegion::Ewram, BusRegion::Oam, 2) => (6, 1),
        (BusRegion::Ewram, BusRegion::Oam, 4) => (12, 2),
        _ => match width {
            4 => (7, 1),
            _ => (4, 1),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_zero_wins_bus_arbitration() {
        let mut controller = DmaController::default();
        controller.channels[2].control = CONTROL_ENABLE;
        controller.channels[0].control = CONTROL_ENABLE;
        controller.channels[2].request();
        controller.channels[0].request();
        assert_eq!(controller.select_next(), Some(0));
    }

    #[test]
    fn zero_count_expands_to_architectural_count() {
        let mut controller = DmaController::default();
        controller.channels[0].count = 0;
        controller.channels[3].count = 0;
        assert_eq!(controller.channels[0].transfer_count(0), 0x4000);
        assert_eq!(controller.channels[3].transfer_count(3), 0x1_0000);
    }

    #[test]
    fn control_masks_keep_dma3_bit_15_configuration_valid() {
        let mut channel = DmaChannel::default();
        channel.write_control(0xffff, 3);
        assert_eq!(channel.control, DMA3_CONTROL_MASK);
    }

    #[test]
    fn rom_waitstate_changes_transfer_cost() {
        let fast = transfer_cycles(bus::ROM0_START, bus::EWRAM_START, 4, 2, 0x001c);
        let slow = transfer_cycles(bus::ROM0_START, bus::EWRAM_START, 4, 2, 0x0000);
        assert!(fast < slow);
    }
}
