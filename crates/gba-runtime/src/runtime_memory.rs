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
            BusRegion::CartridgeRom => self.cartridge.as_ref().and_then(|c| c.rom.get(bus.offset)).copied().unwrap_or(0xff),
            BusRegion::CartridgeSave => self.cartridge.as_ref().map(|c| c.save.read(bus.offset)).unwrap_or(0xff),
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
        if matches!(bus::decode(address).region, BusRegion::CartridgeSave) {
            let value = self.read8(address);
            return u16::from_le_bytes([value, value]);
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
                let memory = match bus.region { BusRegion::Palette => &mut self.palette[..], _ => &mut self.vram[..] };
                let base = bus.offset & !1;
                if base + 1 < memory.len() { memory[base] = value; memory[base + 1] = value; }
            }
            BusRegion::Oam => {}
            BusRegion::CartridgeSave => {
                if let Some(cartridge) = self.cartridge.as_mut() { cartridge.save.write(bus.offset, value); }
            }
            BusRegion::Bios | BusRegion::CartridgeRom => {}
            BusRegion::Unmapped => { self.io.insert(address, value); }
        }
    }

    fn write_mmio8(&mut self, address: u32, value: u8) {
        match address {
            0x0400_0004 => self.dispstat = (self.dispstat & 0xff00) | value as u16,
            0x0400_0005 => self.dispstat = (self.dispstat & 0x00ff) | ((value as u16) << 8),
            KEYINPUT | KEYINPUT_HIGH => {}
            IE => self.interrupts.ie = (self.interrupts.ie & 0xff00) | value as u16,
            0x0400_0201 => self.interrupts.ie = (self.interrupts.ie & 0x00ff) | ((value as u16) << 8),
            IF => self.interrupts.acknowledge(value as u16),
            0x0400_0203 => self.interrupts.acknowledge((value as u16) << 8),
            WAITCNT => self.waitcnt = (self.waitcnt & 0xff00) | value as u16,
            WAITCNT_HIGH => self.waitcnt = (self.waitcnt & 0x00ff) | ((value as u16) << 8),
            IME => { self.interrupts.ime = value & 1 != 0; if self.interrupts.ime { self.service_interrupts(); } }
            0x0400_0300 => self.postflg = value & 1,
            HALTCNT => self.power = if value & 0x80 != 0 { PowerState::Stopped } else { PowerState::Halted },
            _ => { self.io.insert(address, value); }
        }
    }

    pub fn write16(&mut self, address: u32, value: u16) {
        match bus::decode(address).region {
            BusRegion::Palette => self.palette[bus::decode(address).offset & !1..=(bus::decode(address).offset & !1) + 1].copy_from_slice(&value.to_le_bytes()),
            BusRegion::Vram => { let o = bus::decode(address).offset & !1; self.vram[o..o + 2].copy_from_slice(&value.to_le_bytes()); }
            BusRegion::Oam => { let o = bus::decode(address).offset & !1; self.oam[o..o + 2].copy_from_slice(&value.to_le_bytes()); }
            BusRegion::CartridgeSave => { self.write8(address, value.to_le_bytes()[0]); }
            _ if address == IF => self.interrupts.acknowledge(value),
            _ if address == IE => { self.interrupts.ie = value; self.service_interrupts(); }
            _ if address == IME => { self.interrupts.ime = value & 1 != 0; if self.interrupts.ime { self.service_interrupts(); } }
            _ => for (i, byte) in value.to_le_bytes().into_iter().enumerate() { self.write8(address.wrapping_add(i as u32), byte); },
        }
    }

    pub fn write32(&mut self, address: u32, value: u32) {
        if matches!(bus::decode(address).region, BusRegion::CartridgeSave) {
            let byte = value.rotate_right((address & 3) * 8) as u8;
            self.write8(address, byte);
            return;
        }
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() { self.write8(address.wrapping_add(i as u32), byte); }
    }
}
