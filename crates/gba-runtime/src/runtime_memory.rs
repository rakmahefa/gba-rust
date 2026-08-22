use super::Runtime;
use crate::arm7tdmi;
use crate::bios::PowerState;
use crate::bus::{self, BusRegion};
use crate::mmio;
use crate::timers::{timer_index, timer_register_is_control};

impl Runtime {
    pub fn read8(&self, address: u32) -> u8 {
        let bus = bus::decode(address);
        match bus.region {
            BusRegion::Bios => self.bios.read8(bus.offset),
            BusRegion::Ewram => self.ewram[bus.offset],
            BusRegion::Iwram => self.iwram[bus.offset],
            BusRegion::Io => self.read_mmio8(address),
            BusRegion::Palette => self.palette[bus.offset],
            BusRegion::Vram => self.vram[bus.offset],
            BusRegion::Oam => self.oam[bus.offset],
            BusRegion::CartridgeRom => self
                .cartridge
                .as_ref()
                .and_then(|c| c.rom.get(bus.offset))
                .copied()
                .unwrap_or(0xff),
            BusRegion::CartridgeSave => self
                .cartridge
                .as_ref()
                .map(|c| c.save.read(bus.offset))
                .unwrap_or(0xff),
            BusRegion::Unmapped => *self.io.get(&address).unwrap_or(&0),
        }
    }

    fn read_mmio8(&self, address: u32) -> u8 {
        if let Some(index) = timer_index(address) {
            if timer_register_is_control(address) {
                return self.timers[index].read_control() as u8;
            }
            return self.timers[index].counter() as u8;
        }

        match address {
            mmio::DISPCNT => self.dispcnt as u8,
            mmio::DISPCNT_HI => (self.dispcnt >> 8) as u8,
            mmio::DISPSTAT => self.dispstat as u8,
            mmio::DISPSTAT_HI => (self.dispstat >> 8) as u8,
            mmio::VCOUNT => self.vcount as u8,
            mmio::VCOUNT_HI => (self.vcount >> 8) as u8,
            mmio::KEYINPUT => self.keyinput as u8,
            mmio::KEYINPUT_HI => (self.keyinput >> 8) as u8,
            mmio::IE => self.interrupts.ie as u8,
            mmio::IE_HI => (self.interrupts.ie >> 8) as u8,
            mmio::IF => self.interrupts.iflags as u8,
            mmio::IF_HI => (self.interrupts.iflags >> 8) as u8,
            mmio::WAITCNT => self.waitcnt as u8,
            mmio::WAITCNT_HI => (self.waitcnt >> 8) as u8,
            mmio::IME => u8::from(self.interrupts.ime),
            mmio::IME_HI => 0,
            mmio::POSTFLG => self.postflg,
            // HALTCNT is write-only; reads do not expose stored power state.
            mmio::HALTCNT => 0,
            _ => *self.io.get(&address).unwrap_or(&0),
        }
    }

    fn write_mmio8(&mut self, address: u32, value: u8) {
        if let Some(index) = timer_index(address) {
            if timer_register_is_control(address) {
                self.timers[index].write_control(value as u16);
            } else {
                let current = self.timers[index].reload();
                self.timers[index].write_reload((current & 0xff00) | value as u16);
            }
            return;
        }

        match address {
            mmio::DISPCNT => {
                self.dispcnt = (self.dispcnt & 0xff00) | u16::from(value);
            }
            mmio::DISPCNT_HI => {
                self.dispcnt = (self.dispcnt & 0x00ff) | (u16::from(value) << 8);
            }
            mmio::DISPSTAT => {
                self.dispstat = (self.dispstat & 0xff07) | (u16::from(value) & 0x38);
            }
            mmio::DISPSTAT_HI => {
                self.dispstat = (self.dispstat & 0x00ff) | (u16::from(value) << 8);
            }
            // VCOUNT and KEYINPUT are read-only hardware state.
            mmio::VCOUNT | mmio::VCOUNT_HI | mmio::KEYINPUT | mmio::KEYINPUT_HI => {}
            mmio::IE => {
                self.interrupts.ie = (self.interrupts.ie & 0x3f00) | (u16::from(value) & 0x3f);
                self.service_interrupts();
            }
            mmio::IE_HI => {
                self.interrupts.ie =
                    (self.interrupts.ie & 0x00ff) | ((u16::from(value) << 8) & 0x3f00);
                self.service_interrupts();
            }
            mmio::IF => self.interrupts.acknowledge(u16::from(value) & mmio::INTERRUPT_SOURCE_MASK),
            mmio::IF_HI => self
                .interrupts
                .acknowledge((u16::from(value) << 8) & mmio::INTERRUPT_SOURCE_MASK),
            mmio::WAITCNT => {
                self.waitcnt = (self.waitcnt & 0xff00) | u16::from(value);
            }
            mmio::WAITCNT_HI => {
                self.waitcnt = (self.waitcnt & 0x00ff)
                    | ((u16::from(value) << 8) & (mmio::WAITCNT_WRITABLE_MASK & 0xff00));
            }
            mmio::IME => {
                self.interrupts.ime = value & 1 != 0;
                if self.interrupts.ime {
                    self.service_interrupts();
                }
            }
            mmio::IME_HI => {}
            mmio::POSTFLG => self.postflg = value & mmio::POSTFLG_WRITABLE_MASK,
            // HALTCNT consumes only writes; reads remain write-only above.
            mmio::HALTCNT => {
                self.power = if value & 0x80 != 0 {
                    PowerState::Stopped
                } else {
                    PowerState::Halted
                }
            }
            _ => {
                self.io.insert(address, value);
            }
        }
    }

    pub fn read16(&self, address: u32) -> u16 {
        if matches!(bus::decode(address).region, BusRegion::CartridgeSave) {
            let value = self.read8(address);
            return u16::from_le_bytes([value, value]);
        }
        if let Some(index) = timer_index(address) {
            return if timer_register_is_control(address) {
                self.timers[index].read_control()
            } else {
                self.timers[index].counter()
            };
        }
        u16::from_le_bytes([self.read8(address), self.read8(address.wrapping_add(1))])
    }

    pub fn read32(&self, address: u32) -> u32 {
        if matches!(bus::decode(address).region, BusRegion::CartridgeSave) {
            let value = self.read8(address);
            return u32::from_le_bytes([value, value, value, value]);
        }
        let aligned = address & !3;
        let raw = u32::from_le_bytes([
            self.read8(aligned),
            self.read8(aligned.wrapping_add(1)),
            self.read8(aligned.wrapping_add(2)),
            self.read8(aligned.wrapping_add(3)),
        ]);
        arm7tdmi::rotate_unaligned_word(raw, address)
    }

    pub fn write8(&mut self, address: u32, value: u8) {
        let bus = bus::decode(address);
        match bus.region {
            BusRegion::Ewram => self.ewram[bus.offset] = value,
            BusRegion::Iwram => self.iwram[bus.offset] = value,
            BusRegion::Io => self.write_mmio8(address, value),
            BusRegion::Palette | BusRegion::Vram => {
                let memory = match bus.region {
                    BusRegion::Palette => &mut self.palette[..],
                    BusRegion::Vram => &mut self.vram[..],
                    _ => unreachable!(),
                };
                let base = bus.offset & !1;
                if base + 1 < memory.len() {
                    memory[base] = value;
                    memory[base + 1] = value;
                }
            }
            BusRegion::Oam => {}
            BusRegion::CartridgeSave => {
                if let Some(cartridge) = self.cartridge.as_mut() {
                    cartridge.save.write(bus.offset, value);
                }
            }
            BusRegion::Bios | BusRegion::CartridgeRom => {}
            BusRegion::Unmapped => {
                self.io.insert(address, value);
            }
        }
    }

    pub fn write16(&mut self, address: u32, value: u16) {
        if let Some(index) = timer_index(address) {
            if timer_register_is_control(address) {
                self.timers[index].write_control(value);
            } else {
                self.timers[index].write_reload(value);
            }
            return;
        }

        match bus::decode(address).region {
            BusRegion::Palette => {
                let o = bus::decode(address).offset & !1;
                self.palette[o..o + 2].copy_from_slice(&value.to_le_bytes());
            }
            BusRegion::Vram => {
                let o = bus::decode(address).offset & !1;
                self.vram[o..o + 2].copy_from_slice(&value.to_le_bytes());
            }
            BusRegion::Oam => {
                let o = bus::decode(address).offset & !1;
                self.oam[o..o + 2].copy_from_slice(&value.to_le_bytes());
            }
            BusRegion::CartridgeSave => self.write8(address, value.to_le_bytes()[0]),
            _ if address == mmio::DISPCNT => {
                self.dispcnt = value & mmio::DISPCNT_WRITABLE_MASK;
            }
            _ if address == mmio::DISPSTAT => {
                self.dispstat = (self.dispstat & mmio::DISPSTAT_STATUS_MASK)
                    | (value & mmio::DISPSTAT_WRITABLE_MASK);
            }
            _ if address == mmio::VCOUNT => {}
            _ if address == mmio::KEYINPUT => {}
            _ if address == mmio::IE => {
                self.interrupts.ie = value & mmio::INTERRUPT_SOURCE_MASK;
                self.service_interrupts();
            }
            _ if address == mmio::IF => {
                self.interrupts
                    .acknowledge(value & mmio::INTERRUPT_SOURCE_MASK)
            }
            _ if address == mmio::WAITCNT => {
                self.waitcnt = value & mmio::WAITCNT_WRITABLE_MASK;
            }
            _ if address == mmio::IME => {
                self.interrupts.ime = value & mmio::IME_WRITABLE_MASK != 0;
                if self.interrupts.ime {
                    self.service_interrupts();
                }
            }
            _ if address == mmio::POSTFLG => {
                self.postflg = (value as u8) & mmio::POSTFLG_WRITABLE_MASK;
            }
            _ if address == mmio::HALTCNT => {
                self.write8(address, value as u8);
            }
            _ => {
                for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                    self.write8(address.wrapping_add(i as u32), byte);
                }
            }
        }
    }

    pub fn write32(&mut self, address: u32, value: u32) {
        let region = bus::decode(address).region;
        match region {
            BusRegion::Palette => {
                let o = bus::decode(address).offset & !3;
                self.palette[o..o + 4].copy_from_slice(&value.to_le_bytes());
            }
            BusRegion::Vram => {
                let o = bus::decode(address).offset & !3;
                self.vram[o..o + 4].copy_from_slice(&value.to_le_bytes());
            }
            BusRegion::Oam => {
                let o = bus::decode(address).offset & !3;
                self.oam[o..o + 4].copy_from_slice(&value.to_le_bytes());
            }
            BusRegion::CartridgeSave => {
                let byte = value.rotate_right((address & 3) * 8) as u8;
                self.write8(address, byte);
            }
            _ => {
                for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                    self.write8(address.wrapping_add(i as u32), byte);
                }
            }
        }
    }
}