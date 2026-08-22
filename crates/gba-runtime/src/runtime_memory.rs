use super::Runtime;
use crate::arm7tdmi;
use crate::bios::{PowerState, HALTCNT, IE, IF, IME, KEYINPUT, WAITCNT};
use crate::bus::{self, BusRegion};

const KEYINPUT_HIGH: u32 = KEYINPUT + 1;
const WAITCNT_HIGH: u32 = WAITCNT + 1;

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
                .and_then(|cartridge| cartridge.rom.get(bus.offset))
                .copied()
                .unwrap_or(0xff),
            BusRegion::CartridgeSave => self
                .cartridge
                .as_ref()
                .map(|cartridge| cartridge.save.read(bus.offset))
                .unwrap_or(0xff),
            BusRegion::Unmapped => *self.io.get(&address).unwrap_or(&0),
        }
    }

    fn read_mmio8(&self, address: u32) -> u8 {
        match address {
            0x0400_0004 => self.dispstat as u8,
            0x0400_0005 => (self.dispstat >> 8) as u8,
            KEYINPUT => self.keyinput as u8,
            KEYINPUT_HIGH => (self.keyinput >> 8) as u8,
            IE => self.interrupts.ie as u8,
            0x0400_0201 => (self.interrupts.ie >> 8) as u8,
            IF => self.interrupts.iflags as u8,
            0x0400_0203 => (self.interrupts.iflags >> 8) as u8,
            WAITCNT => self.waitcnt as u8,
            WAITCNT_HIGH => (self.waitcnt >> 8) as u8,
            IME => u8::from(self.interrupts.ime),
            0x0400_0300 => self.postflg,
            HALTCNT => 0,
            _ => *self.io.get(&address).unwrap_or(&0),
        }
    }

    pub fn read16(&self, address: u32) -> u16 {
        if matches!(bus::decode(address).region, BusRegion::Io) {
            u16::from_le_bytes([self.read_mmio8(address), self.read_mmio8(address.wrapping_add(1))])
        } else if matches!(bus::decode(address).region, BusRegion::CartridgeSave) {
            let byte = self.read8(address);
            u16::from_le_bytes([byte, byte])
        } else {
            u16::from_le_bytes([self.read8(address), self.read8(address.wrapping_add(1))])
        }
    }

    pub fn read32(&self, address: u32) -> u32 {
        let aligned = address & !3;
        let raw = u32::from_le_bytes([
            self.read8(aligned),
            self.read8(aligned.wrapping_add(1)),
            self.read8(aligned.wrapping_add(2)),
            self.read8(aligned.wrapping_add(3)),
        ]);
        if matches!(bus::decode(address).region, BusRegion::CartridgeSave) {
            u32::from_le_bytes([self.read8(address), self.read8(address), self.read8(address), self.read8(address)])
        } else {
            arm7tdmi::rotate_unaligned_word(raw, address)
        }
    }

    pub fn write8(&mut self, address: u32, value: u8) {
        let bus = bus::decode(address);
        match bus.region {
            BusRegion::Ewram => self.ewram[bus.offset] = value,
            BusRegion::Iwram => self.iwram[bus.offset] = value,
            BusRegion::Io => self.write_mmio8(address, value),
            BusRegion::Palette => {
                let base = bus.offset & !1;
                self.palette[base] = value;
                self.palette[base + 1] = value;
            }
            BusRegion::Vram => {
                let base = bus.offset & !1;
                if base + 1 < self.vram.len() {
                    self.vram[base] = value;
                    self.vram[base + 1] = value;
                }
            }
            BusRegion::Oam => {}
            BusRegion::CartridgeSave => {
                if let Some(cartridge) = self.cartridge.as_mut() {
                    cartridge.save.write(bus.offset, value);
                }
            }
            BusRegion::Bios | BusRegion::CartridgeRom | BusRegion::Unmapped => {
                if matches!(bus.region, BusRegion::Unmapped) {
                    self.io.insert(address, value);
                }
            }
        }
    }

    fn write_mmio8(&mut self, address: u32, value: u8) {
        match address {
            0x0400_0004 => self.dispstat = (self.dispstat & 0xff00) | value as u16,
            0x0400_0005 => self.dispstat = (self.dispstat & 0x00ff) | ((value as u16) << 8),
            KEYINPUT | KEYINPUT_HIGH => {}
            IE => self.interrupts.ie = (self.interrupts.ie & 0xff00) | value as u16,
            0x0400_0201 => {
                self.interrupts.ie = (self.interrupts.ie & 0x00ff) | ((value as u16) << 8)
            }
            IF => self.interrupts.acknowledge(value as u16),
            0x0400_0203 => self.interrupts.acknowledge((value as u16) << 8),
            WAITCNT => self.waitcnt = (self.waitcnt & 0xff00) | value as u16,
            WAITCNT_HIGH => self.waitcnt = (self.waitcnt & 0x00ff) | ((value as u16) << 8),
            IME => {
                self.interrupts.ime = value & 1 != 0;
                if self.interrupts.ime {
                    self.service_interrupts();
                }
            }
            0x0400_0300 => self.postflg = value & 1,
            HALTCNT => {
                self.power = if value & 0x80 != 0 {
                    PowerState::Stopped
                } else {
                    PowerState::Halted
                };
            }
            _ => {
                self.io.insert(address, value);
            }
        }
    }

    pub fn write16(&mut self, address: u32, value: u16) {
        if address == IF {
            self.interrupts.acknowledge(value);
            return;
        }
        if address == IE {
            self.interrupts.ie = value;
            self.service_interrupts();
            return;
        }
        if address == IME {
            self.interrupts.ime = value & 1 != 0;
            if self.interrupts.ime {
                self.service_interrupts();
            }
            return;
        }
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte);
        }
    }

    pub fn write32(&mut self, address: u32, value: u32) {
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte);
        }
    }
}
