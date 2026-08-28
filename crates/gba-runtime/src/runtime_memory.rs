use super::Runtime;
use crate::arm7tdmi;
use crate::bios::PowerState;
use crate::bus::{self, BusRegion};
use crate::mmio;
use crate::mmio_devices;
use crate::timers::{timer_index, timer_register_is_control};

impl Runtime {
    pub fn read8(&self, address: u32) -> u8 {
        let bus = bus::decode(address);
        match bus.region {
            BusRegion::Bios => self.bios.read8(bus.offset), BusRegion::Ewram => self.ewram[bus.offset], BusRegion::Iwram => self.iwram[bus.offset],
            BusRegion::Io => self.read_mmio8(address), BusRegion::Palette => self.palette[bus.offset], BusRegion::Vram => self.vram[bus.offset], BusRegion::Oam => self.oam[bus.offset],
            BusRegion::CartridgeRom => self.cartridge.as_ref().and_then(|c| c.rom.get(bus.offset)).copied().unwrap_or(0xff),
            BusRegion::CartridgeSave => self.cartridge.as_ref().map(|c| c.save.read(bus.offset)).unwrap_or(0xff), BusRegion::Unmapped => *self.io.get(&address).unwrap_or(&0),
        }
    }
    fn contract_for_mmio(address: u32) -> Option<mmio::MmioRegister> { mmio::register(address).or_else(|| mmio_devices::register(address)) }
    fn read_mmio8(&self, address: u32) -> u8 {
        if let Some(index) = timer_index(address) {
            let value = if timer_register_is_control(address) { self.timers[index].read_control() } else { self.timers[index].counter() };
            return if address & 1 == 0 { value as u8 } else { (value >> 8) as u8 };
        }
        match address {
            mmio::DISPCNT => self.dispcnt as u8, mmio::DISPCNT_HI => (self.dispcnt >> 8) as u8,
            mmio::DISPSTAT => self.dispstat as u8, mmio::DISPSTAT_HI => (self.dispstat >> 8) as u8,
            mmio::VCOUNT => self.vcount as u8, mmio::VCOUNT_HI => (self.vcount >> 8) as u8,
            mmio::KEYINPUT => self.keyinput as u8, mmio::KEYINPUT_HI => (self.keyinput >> 8) as u8,
            mmio::KEYCNT => self.keycnt as u8, mmio::KEYCNT_HI => (self.keycnt >> 8) as u8,
            mmio::IE => self.interrupts.ie as u8, mmio::IE_HI => (self.interrupts.ie >> 8) as u8,
            mmio::IF => self.interrupts.iflags as u8, mmio::IF_HI => (self.interrupts.iflags >> 8) as u8,
            mmio::WAITCNT => self.waitcnt as u8, mmio::WAITCNT_HI => (self.waitcnt >> 8) as u8,
            mmio::IME => u8::from(self.interrupts.ime), mmio::IME_HI => 0, mmio::POSTFLG => self.postflg, mmio::HALTCNT => 0,
            _ => { if let Some(register) = Self::contract_for_mmio(address) { if !register.access.can_read() { return 0; } return *self.io.get(&address).unwrap_or(&0); } *self.io.get(&address).unwrap_or(&0) }
        }
    }
    fn write_mmio8(&mut self, address: u32, value: u8) {
        if let Some(index) = timer_index(address) {
            if timer_register_is_control(address) {
                let current = self.timers[index].read_control(); let next = if address & 1 == 0 { (current & 0xff00) | u16::from(value) } else { (current & 0x00ff) | (u16::from(value) << 8) }; self.timers[index].write_control(next);
            } else {
                let current = self.timers[index].reload(); let next = if address & 1 == 0 { (current & 0xff00) | u16::from(value) } else { (current & 0x00ff) | (u16::from(value) << 8) }; self.timers[index].write_reload(next);
            }
            return;
        }
        match address {
            mmio::DISPCNT => { let writable = u16::from(value) & 0x00f7; self.dispcnt = (self.dispcnt & !0x00f7) | writable; }
            mmio::DISPCNT_HI => self.dispcnt = (self.dispcnt & 0x00ff) | (u16::from(value) << 8),
            mmio::DISPSTAT => self.dispstat = (self.dispstat & 0xff07) | (u16::from(value) & 0x38), mmio::DISPSTAT_HI => self.dispstat = (self.dispstat & 0x00ff) | (u16::from(value) << 8),
            mmio::VCOUNT | mmio::VCOUNT_HI | mmio::KEYINPUT | mmio::KEYINPUT_HI => {},
            mmio::KEYCNT => { self.keycnt = (self.keycnt & 0xff00) | (u16::from(value) & 0x00ff); self.update_keypad_irq(); },
            mmio::KEYCNT_HI => { self.keycnt = (self.keycnt & 0x00ff) | ((u16::from(value) << 8) & 0xc000); self.update_keypad_irq(); },
            mmio::IE => { self.interrupts.ie = (self.interrupts.ie & 0x3f00) | u16::from(value); self.service_interrupts(); }, mmio::IE_HI => { self.interrupts.ie = (self.interrupts.ie & 0x00ff) | ((u16::from(value) << 8) & 0x3f00); self.service_interrupts(); },
            mmio::IF => self.interrupts.acknowledge(u16::from(value) & mmio::INTERRUPT_SOURCE_MASK), mmio::IF_HI => self.interrupts.acknowledge((u16::from(value) << 8) & mmio::INTERRUPT_SOURCE_MASK),
            mmio::WAITCNT => self.waitcnt = (self.waitcnt & 0xff00) | u16::from(value), mmio::WAITCNT_HI => self.waitcnt = (self.waitcnt & 0x00ff) | ((u16::from(value) << 8) & 0x5000) | (self.waitcnt & 0x8000),
            mmio::IME => { self.interrupts.ime = value & 1 != 0; if self.interrupts.ime { self.service_interrupts(); } }, mmio::IME_HI => {},
            mmio::POSTFLG => self.postflg = value & mmio::POSTFLG_WRITABLE_MASK,
            mmio::HALTCNT => self.power = if value & 0x80 != 0 { PowerState::Stopped } else { PowerState::Halted },
            _ => { if let Some(register) = Self::contract_for_mmio(address) { if !register.access.can_write() { return; } let mask = register.writable_byte_mask(address); if mask == 0 { return; } let current = *self.io.get(&address).unwrap_or(&0); self.io.insert(address, (current & !mask) | (value & mask)); if (0x0400_00b0..=0x0400_00df).contains(&address) { self.sync_dma_registers(); } return; } self.io.insert(address, value); }
        }
    }
    pub fn read16(&self, address: u32) -> u16 { if matches!(bus::decode(address).region, BusRegion::CartridgeSave) { let value = self.read8(address); return u16::from_le_bytes([value, value]); } if let Some(index) = timer_index(address) { return if timer_register_is_control(address) { self.timers[index].read_control() } else { self.timers[index].counter() }; } u16::from_le_bytes([self.read8(address), self.read8(address.wrapping_add(1))]) }
    pub fn read32(&self, address: u32) -> u32 { if matches!(bus::decode(address).region, BusRegion::CartridgeSave) { let value = self.read8(address); return u32::from_le_bytes([value, value, value, value]); } let aligned = address & !3; let raw = u32::from_le_bytes([self.read8(aligned), self.read8(aligned.wrapping_add(1)), self.read8(aligned.wrapping_add(2)), self.read8(aligned.wrapping_add(3))]); arm7tdmi::rotate_unaligned_word(raw, address) }
    pub fn write8(&mut self, address: u32, value: u8) { let bus = bus::decode(address); match bus.region {
        BusRegion::Ewram => self.ewram[bus.offset] = value, BusRegion::Iwram => self.iwram[bus.offset] = value, BusRegion::Io => self.write_mmio8(address, value),
        BusRegion::Palette | BusRegion::Vram => { let memory = match bus.region { BusRegion::Palette => &mut self.palette[..], BusRegion::Vram => &mut self.vram[..], _ => unreachable!() }; let base = bus.offset & !1; if base + 1 < memory.len() { memory[base] = value; memory[base + 1] = value; } }
        BusRegion::Oam => {}, BusRegion::CartridgeSave => { if let Some(cartridge) = self.cartridge.as_mut() { cartridge.save.write(bus.offset, value); } }, BusRegion::Bios | BusRegion::CartridgeRom => {}, BusRegion::Unmapped => { self.io.insert(address, value); }
    } }
    pub fn write16(&mut self, address: u32, value: u16) { if let Some(index) = timer_index(address) { if timer_register_is_control(address) { self.timers[index].write_control(value); } else { self.timers[index].write_reload(value); } return; } match bus::decode(address).region {
        BusRegion::Palette => { let o = bus::decode(address).offset & !1; self.palette[o..o + 2].copy_from_slice(&value.to_le_bytes()); }, BusRegion::Vram => { let o = bus::decode(address).offset & !1; self.vram[o..o + 2].copy_from_slice(&value.to_le_bytes()); }, BusRegion::Oam => { let o = bus::decode(address).offset & !1; self.oam[o..o + 2].copy_from_slice(&value.to_le_bytes()); }, BusRegion::CartridgeSave => self.write8(address, value.to_le_bytes()[0]),
        _ if address == mmio::DISPCNT => self.dispcnt = (self.dispcnt & !mmio::DISPCNT_WRITABLE_MASK) | (value & mmio::DISPCNT_WRITABLE_MASK), _ if address == mmio::DISPSTAT => self.dispstat = (self.dispstat & mmio::DISPSTAT_STATUS_MASK) | (value & mmio::DISPSTAT_WRITABLE_MASK), _ if address == mmio::VCOUNT || address == mmio::KEYINPUT => {}, _ if address == mmio::KEYCNT => { self.keycnt = value & mmio::KEYCNT_WRITABLE_MASK; self.update_keypad_irq(); },
        _ if address == mmio::IE => { self.interrupts.ie = value & mmio::INTERRUPT_SOURCE_MASK; self.service_interrupts(); }, _ if address == mmio::IF => self.interrupts.acknowledge(value & mmio::INTERRUPT_SOURCE_MASK), _ if address == mmio::WAITCNT => self.waitcnt = (self.waitcnt & 0x8000) | (value & mmio::WAITCNT_WRITABLE_MASK),
        _ if address == mmio::IME => { self.interrupts.ime = value & mmio::IME_WRITABLE_MASK != 0; if self.interrupts.ime { self.service_interrupts(); } }, _ if address == mmio::POSTFLG => { let bytes = value.to_le_bytes(); self.write8(mmio::POSTFLG, bytes[0]); self.write8(mmio::HALTCNT, bytes[1]); }, _ if address == mmio::HALTCNT => self.write8(address, value as u8),
        _ => { for (i, byte) in value.to_le_bytes().into_iter().enumerate() { self.write8(address.wrapping_add(i as u32), byte); } }
    } }
    pub fn write32(&mut self, address: u32, value: u32) { match bus::decode(address).region {
        BusRegion::Palette => { let o = bus::decode(address).offset & !3; self.palette[o..o + 4].copy_from_slice(&value.to_le_bytes()); }, BusRegion::Vram => { let o = bus::decode(address).offset & !3; self.vram[o..o + 4].copy_from_slice(&value.to_le_bytes()); }, BusRegion::Oam => { let o = bus::decode(address).offset & !3; self.oam[o..o + 4].copy_from_slice(&value.to_le_bytes()); }, BusRegion::CartridgeSave => { let byte = value.rotate_right((address & 3) * 8) as u8; self.write8(address, byte); }, _ => { for (i, byte) in value.to_le_bytes().into_iter().enumerate() { self.write8(address.wrapping_add(i as u32), byte); } }
    } }
}

#[cfg(test)]
mod mmio_device_tests {
    use super::*; use crate::bios::{IRQ_DMA0, IRQ_KEYPAD, IRQ_TIMER0}; use crate::mmio_devices::{DMA0CNT_H, DMA0CNT_L, DMA0DAD, DMA0SAD, TIMER0CNT_H, TIMER0CNT_L}; use crate::bus;
    #[test] fn dma_immediate_control_write_is_observable_as_a_device_trigger() { let mut runtime = Runtime::new(); runtime.write16(bus::EWRAM_START, 0xbeef); runtime.write32(DMA0SAD.address, bus::EWRAM_START); runtime.write32(DMA0DAD.address, bus::EWRAM_START + 0x100); runtime.write16(DMA0CNT_L.address, 1); runtime.interrupts.ie = IRQ_DMA0; runtime.write16(DMA0CNT_H.address, 0x8000 | 0x4000); assert_eq!(runtime.dma.active(), None); runtime.advance_cycles(2); assert_eq!(runtime.read16(bus::EWRAM_START + 0x100), 0xbeef); assert_eq!(runtime.dma.active(), Some(0)); }
    #[test] fn timer_byte_accesses_preserve_the_other_byte() { let mut runtime = Runtime::new(); runtime.write8(TIMER0CNT_L.address, 0x34); runtime.write8(TIMER0CNT_L.address + 1, 0x12); runtime.write8(TIMER0CNT_H.address, 0x80); runtime.write8(TIMER0CNT_H.address + 1, 0x00); assert_eq!(runtime.read16(TIMER0CNT_L.address), 0x1234); assert_eq!(runtime.read16(TIMER0CNT_H.address), 0x0080); assert_eq!(runtime.timers[0].counter(), 0x1234); }
    #[test] fn timer_irq_is_raised_from_mmio_programming_and_scheduler_cycles() { let mut runtime = Runtime::new(); runtime.interrupts.ie = IRQ_TIMER0; runtime.write16(TIMER0CNT_L.address, u16::MAX); runtime.write16(TIMER0CNT_H.address, 0x00c0 | 0x0080); runtime.advance_cycles(1); assert_ne!(runtime.interrupts.iflags & IRQ_TIMER0, 0); }
    #[test] fn keycnt_is_mmio_backed_and_can_raise_keypad_irq() { let mut runtime = Runtime::new(); runtime.interrupts.ie = IRQ_KEYPAD; runtime.write16(crate::mmio::KEYCNT, crate::mmio::KEYCNT_IRQ_ENABLE | 1); assert_eq!(runtime.read16(crate::mmio::KEYCNT), crate::mmio::KEYCNT_IRQ_ENABLE | 1); runtime.set_key_pressed(0, true); assert_ne!(runtime.interrupts.iflags & IRQ_KEYPAD, 0); }
}
