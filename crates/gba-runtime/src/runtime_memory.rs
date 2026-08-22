use super::arm7tdmi;
use super::bios::{HALTCNT, IE, IF, IME, KEYINPUT, WAITCNT};
use super::Runtime;

const KEYINPUT_HIGH: u32 = KEYINPUT + 1;
const WAITCNT_HIGH: u32 = WAITCNT + 1;

impl Runtime {
    pub fn read8(&self, address: u32) -> u8 {
        match address {
            0x0000_0000..=0x0000_3fff => 0xff,
            0x0200_0000..=0x0203_ffff => self.ewram[(address - 0x0200_0000) as usize],
            0x0300_0000..=0x0300_7fff => self.iwram[(address - 0x0300_0000) as usize],
            0x0400_0000..=0x0400_03ff => self.read_mmio8(address),
            0x0500_0000..=0x0500_03ff => self.palette[(address - 0x0500_0000) as usize],
            0x0600_0000..=0x0601_7fff => self.vram[(address - 0x0600_0000) as usize],
            0x0700_0000..=0x0700_03ff => self.oam[(address - 0x0700_0000) as usize],
            0x0800_0000..0x0e00_0000 => self
                .cartridge
                .as_ref()
                .and_then(|c| c.rom.get((address - 0x0800_0000) as usize))
                .copied()
                .unwrap_or(0xff),
            0x0e00_0000..=0x0e00_ffff => self
                .cartridge
                .as_ref()
                .map(|c| c.save.read((address - 0x0e00_0000) as usize))
                .unwrap_or(0xff),
            _ => *self.io.get(&address).unwrap_or(&0),
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
        if (0x0400_0000..=0x0400_03ff).contains(&address) {
            u16::from_le_bytes([
                self.read_mmio8(address),
                self.read_mmio8(address.wrapping_add(1)),
            ])
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
        arm7tdmi::rotate_unaligned_word(raw, address)
    }

    pub fn write8(&mut self, address: u32, value: u8) {
        match address {
            0x0200_0000..=0x0203_ffff => {
                self.ewram[(address - 0x0200_0000) as usize] = value;
            }
            0x0300_0000..=0x0300_7fff => {
                self.iwram[(address - 0x0300_0000) as usize] = value;
            }
            0x0400_0000..=0x0400_03ff => self.write_mmio8(address, value),
            0x0500_0000..=0x0500_03ff => {
                self.palette[(address - 0x0500_0000) as usize] = value;
            }
            0x0600_0000..=0x0601_7fff => {
                self.vram[(address - 0x0600_0000) as usize] = value;
            }
            0x0700_0000..=0x0700_03ff => {
                self.oam[(address - 0x0700_0000) as usize] = value;
            }
            0x0e00_0000..=0x0e00_ffff => {
                if let Some(cartridge) = self.cartridge.as_mut() {
                    cartridge
                        .save
                        .write((address - 0x0e00_0000) as usize, value);
                }
            }
            _ => {
                self.io.insert(address, value);
            }
        }
    }

    fn write_mmio8(&mut self, address: u32, value: u8) {
        match address {
            0x0400_0004 => self.dispstat = (self.dispstat & 0xff00) | value as u16,
            0x0400_0005 => self.dispstat = (self.dispstat & 0x00ff) | ((value as u16) << 8),
            KEYINPUT => {}
            KEYINPUT_HIGH => {}
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
                    super::bios::PowerState::Stopped
                } else {
                    super::bios::PowerState::Halted
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
