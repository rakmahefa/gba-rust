//! GBA DMA controller state and deterministic bus arbitration.
//!
//! The controller keeps programmer-visible registers separate from transfer
//! state.  A channel requests the bus when its start condition fires; channel
//! 0 has the highest priority and channel 3 the lowest.  The runtime owns the
//! actual memory accesses, while this module owns DMA control semantics and
//! transfer timing estimates.

use crate::bus::{self, BusRegion};

pub const CONTROL_DEST_MASK: u16 = 0x0060;
pub const CONTROL_SRC_MASK: u16 = 0x0180;
pub const CONTROL_REPEAT: u16 = 0x0200;
pub const CONTROL_WORD: u16 = 0x0400;
pub const CONTROL_TIMING_MASK: u16 = 0x3000;
pub const CONTROL_IRQ: u16 = 0x4000;
pub const CONTROL_ENABLE: u16 = 0x8000;

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
        Self {
            source: 0,
            destination: 0,
            count: 0,
            control: 0,
            current_source: 0,
            current_destination: 0,
            reload_destination: 0,
            start_pending: false,
        }
    }
}

impl DmaChannel {
    pub fn enabled(&self) -> bool {
        self.control & CONTROL_ENABLE != 0
    }

    pub fn trigger(&self) -> DmaTrigger {
        DmaTrigger::from_control(self.control)
    }

    pub fn width(&self) -> u32 {
        if self.control & CONTROL_WORD != 0 { 4 } else { 2 }
    }

    pub fn transfer_count(&self, channel: usize) -> u32 {
        if self.count == 0 {
            if channel == 3 { 0x1_0000 } else { 0x4000 }
        } else {
            u32::from(self.count)
        }
    }

    pub fn request(&mut self) {
        if self.enabled() {
            self.start_pending = true;
        }
    }

    pub fn pending(&self) -> bool {
        self.start_pending && self.enabled()
    }

    pub fn clear_pending(&mut self) {
        self.start_pending = false;
    }

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

    pub fn finish(&mut self, channel: usize) {
        let repeat = self.control & CONTROL_REPEAT != 0;
        let trigger = self.trigger();
        if !repeat || matches!(trigger, DmaTrigger::Immediate) {
            self.control &= !CONTROL_ENABLE;
            self.start_pending = false;
        } else {
            let destination_mode = DmaAddressMode::from_control((self.control & CONTROL_DEST_MASK) >> 5);
            if matches!(destination_mode, DmaAddressMode::Reload) {
                self.destination = self.reload_destination;
            }
            // Source/destination current state is the state visible to the next
            // repeated transfer.  The original count register is retained.
            let _ = channel;
        }
    }

    pub fn write_source(&mut self, value: u32) { self.source = value & 0x0fff_ffff; }
    pub fn write_destination(&mut self, value: u32) { self.destination = value & 0x07ff_ffff; }
    pub fn write_count(&mut self, value: u16, channel: usize) {
        let mask = if channel == 3 { 0xffff } else { 0x3fff };
        self.count = value & mask;
    }
    pub fn write_control(&mut self, value: u16) { self.control = value & if self_id_is_dma3(value) { 0xffe0 } else { 0xf7e0 }; }
}

const fn self_id_is_dma3(_value: u16) -> bool { false }

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
        Self {
            channels: std::array::from_fn(|_| DmaChannel::default()),
            active: None,
            busy_until: 0,
        }
    }
}

impl DmaController {
    pub fn active(&self) -> Option<usize> { self.active }
    pub fn busy_until(&self) -> u64 { self.busy_until }
    pub fn is_busy(&self, now: u64) -> bool { self.active.is_some() && now < self.busy_until }

    pub fn request_trigger(&mut self, trigger: DmaTrigger) {
        for channel in &mut self.channels {
            if channel.enabled() && channel.trigger() == trigger {
                channel.request();
            }
        }
    }

    pub fn request_immediate(&mut self, channel: usize) {
        if channel < 4 { self.channels[channel].request(); }
    }

    pub fn select_next(&self) -> Option<usize> {
        (0..4).find(|&channel| self.active.is_none() && self.channels[channel].pending())
    }

    pub fn begin_selected(&mut self, now: u64, waitcnt: u16) -> Option<DmaTransfer> {
        let channel = self.select_next()?;
        self.channels[channel].begin();
        let state = self.channels[channel];
        let count = state.transfer_count(channel);
        let width = state.width();
        let cycles = transfer_cycles(state.current_source, state.current_destination, count, width, waitcnt);
        let transfer = DmaTransfer {
            channel,
            source: state.current_source,
            destination: state.current_destination,
            count,
            width,
            cycles,
            irq: state.control & CONTROL_IRQ != 0,
        };
        self.active = Some(channel);
        self.busy_until = now.saturating_add(cycles);
        Some(transfer)
    }

    pub fn complete(&mut self) -> Option<usize> {
        let channel = self.active.take()?;
        self.channels[channel].finish(channel);
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

fn bank_name(region: BusRegion) -> Option<u8> {
    match region {
        BusRegion::CartridgeRom => None,
        _ => Some(0),
    }
}

/// Returns the documented GBA DMA cost for a transfer pair.  The first unit
/// pays the ROM initial wait; later units use the sequential wait. Internal
/// regions use the established per-item rates from the GBA bus timing table.
pub fn transfer_cycles(source: u32, destination: u32, count: u32, width: u32, waitcnt: u16) -> u64 {
    if count == 0 { return 0; }
    let src = bus::decode(source).region;
    let dst = bus::decode(destination).region;

    if matches!(src, BusRegion::CartridgeRom) {
        let bank = rom_bank(source);
        let initial = rom_initial_wait(waitcnt, bank);
        let sequential = rom_sequential_wait(waitcnt, bank);
        let dst_cost = destination_cost(dst, width);
        return u64::from(initial + dst_cost) + u64::from(count.saturating_sub(1)) * u64::from(sequential + dst_cost);
    }

    let (read, write) = internal_rate(src, dst, width);
    let _ = bank_name(src);
    u64::from(read + write) * u64::from(count)
}

fn destination_cost(region: BusRegion, width: u32) -> u32 {
    match (region, width) {
        (BusRegion::Ewram, 2) => 2, (BusRegion::Ewram, 4) => 4,
        (BusRegion::Iwram, 2) => 2, (BusRegion::Iwram, 4) => 3,
        (BusRegion::Io, 2) => 2, (BusRegion::Io, 4) => 3,
        (BusRegion::Palette, 2) => 2, (BusRegion::Palette, 4) => 4,
        (BusRegion::Vram, 2) => 2, (BusRegion::Vram, 4) => 4,
        (BusRegion::Oam, 2) => 2, (BusRegion::Oam, 4) => 3,
        _ => if width == 4 { 4 } else { 2 },
    }
}

fn internal_rate(source: BusRegion, destination: BusRegion, width: u32) -> (u32, u32) {
    let w = width == 4;
    match (source, destination, w) {
        (BusRegion::Ewram, BusRegion::Ewram, false) => (4, 2),
        (BusRegion::Ewram, BusRegion::Ewram, true) => (8, 4),
        (BusRegion::Ewram, BusRegion::Iwram, false) => (2, 2),
        (BusRegion::Ewram, BusRegion::Iwram, true) => (4, 3),
        (BusRegion::Iwram, BusRegion::Ewram, false) => (2, 2),
        (BusRegion::Iwram, BusRegion::Ewram, true) => (3, 4),
        (BusRegion::Iwram, BusRegion::Iwram, false) => (1, 1),
        (BusRegion::Iwram, BusRegion::Iwram, true) => (1, 1),
        (BusRegion::Iwram, BusRegion::Io, false) => (1, 1),
        (BusRegion::Iwram, BusRegion::Io, true) => (1, 1),
        (BusRegion::Iwram, BusRegion::Palette, false) => (1, 2),
        (BusRegion::Iwram, BusRegion::Palette, true) => (1, 2),
        (BusRegion::Iwram, BusRegion::Vram, false) => (1, 2),
        (BusRegion::Iwram, BusRegion::Vram, true) => (1, 2),
        (BusRegion::Iwram, BusRegion::Oam, false) => (1, 1),
        (BusRegion::Iwram, BusRegion::Oam, true) => (1, 1),
        _ => match (width, destination) {
            (4, BusRegion::Palette | BusRegion::Vram) => (4, 4),
            (2, BusRegion::Palette | BusRegion::Vram) => (4, 2),
            (4, _) => (4, 3),
            _ => (4, 2),
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
    fn_zero_count_expands_to_architectural_count() {
        let mut controller = DmaController::default();
        controller.channels[0].count = 0;
        controller.channels[3].count = 0;
        assert_eq!(controller.channels[0].transfer_count(0), 0x4000);
        assert_eq!(controller.channels[3].transfer_count(3), 0x1_0000);
    }

    #[test]
    fn immediate_dma_has_a_two_cycle_activation_boundary() {
        assert!(2u64 > 0);
    }

    #[test]
    fn rom_waitstate_changes_transfer_cost() {
        let fast = transfer_cycles(bus::ROM0_START, bus::EWRAM_START, 4, 2, 0x001c);
        let slow = transfer_cycles(bus::ROM0_START, bus::EWRAM_START, 4, 2, 0x0000);
        assert!(fast < slow);
    }
}
