//! Deterministic GBA serial I/O model for Phase C.
//!
//! Host link transports are intentionally outside this architectural layer.

pub const SIOCNT_MASK: u16 = 0xf1ff;
pub const SIODATA8_MASK: u8 = 0xff;
pub const RCNT_MASK: u16 = 0x800f;
pub const SIOCNT_BUSY: u16 = 1 << 7;
pub const SIOCNT_IRQ: u16 = 1 << 14;
pub const SIOCNT_START: u16 = 1 << 7;
pub const TRANSFER_8_CYCLES: u64 = 512;
pub const TRANSFER_32_CYCLES: u64 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SioMode {
    #[default]
    Normal8,
    Normal32,
    Multiplayer,
    Uart,
}

#[derive(Debug, Clone, Default)]
pub struct Sio {
    pub siocnt: u16,
    pub rcnt: u16,
    pub mode: SioMode,
    pub tx_last: Option<u16>,
    pub rx_pending: Option<u16>,
    pub irq_pending: bool,
    pub multi: [u16; 4],
    pub cycles_remaining: u64,
    pub transfer_active: bool,
    pub transfer_count: u64,
}

impl Sio {
    pub fn write_siocnt(&mut self, value: u16) {
        let start = value & SIOCNT_START != 0;
        self.siocnt = value & SIOCNT_MASK;
        self.mode = match (self.siocnt >> 12) & 0x3 { 0 => SioMode::Normal8, 1 => SioMode::Normal32, 2 => SioMode::Multiplayer, _ => SioMode::Uart };
        if start { self.start_transfer(); }
    }

    pub fn write_rcnt(&mut self, value: u16) { self.rcnt = value & RCNT_MASK; }

    pub fn write_data8(&mut self, value: u8) {
        self.tx_last = Some(u16::from(value));
        if self.mode == SioMode::Uart || self.mode == SioMode::Normal8 { self.start_transfer(); }
    }

    pub fn transmit(&mut self, value: u16) { self.tx_last = Some(value); self.start_transfer(); }

    pub fn receive(&mut self, value: u16) {
        self.rx_pending = Some(value);
        if self.siocnt & SIOCNT_IRQ != 0 { self.irq_pending = true; }
    }

    pub fn take_rx(&mut self) -> Option<u16> { self.rx_pending.take() }
    pub fn take_irq(&mut self) -> bool { let pending = self.irq_pending; self.irq_pending = false; pending }

    fn transfer_cycles(&self) -> u64 {
        match self.mode { SioMode::Normal8 | SioMode::Uart => TRANSFER_8_CYCLES, SioMode::Normal32 | SioMode::Multiplayer => TRANSFER_32_CYCLES }
    }

    fn start_transfer(&mut self) {
        self.transfer_active = true;
        self.cycles_remaining = self.transfer_cycles();
        self.siocnt |= SIOCNT_BUSY;
    }

    pub fn advance_cycles(&mut self, cycles: u64) -> bool {
        if !self.transfer_active { return false; }
        self.cycles_remaining = self.cycles_remaining.saturating_sub(cycles);
        if self.cycles_remaining != 0 { return false; }
        self.transfer_active = false;
        self.siocnt &= !SIOCNT_BUSY;
        self.transfer_count = self.transfer_count.wrapping_add(1);
        let tx = self.tx_last.unwrap_or(0);
        match self.mode {
            SioMode::Multiplayer => { self.multi[0] = tx; self.rx_pending = Some(self.multi[0]); }
            _ => { self.rx_pending = Some(tx); }
        }
        if self.siocnt & SIOCNT_IRQ != 0 { self.irq_pending = true; }
        true
    }

    pub fn serial_data8(&self) -> u8 { self.rx_pending.unwrap_or(0) as u8 }
    pub fn is_busy(&self) -> bool { self.transfer_active }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_control_masks_reserved_bits_and_selects_mode() { let mut sio = Sio::default(); sio.write_siocnt(0xffff); assert_eq!(sio.siocnt & !SIOCNT_BUSY, SIOCNT_MASK & !SIOCNT_BUSY); assert_eq!(sio.mode, SioMode::Uart); }
    #[test]
    fn normal8_transfer_completes_after_deterministic_cycles() { let mut sio = Sio::default(); sio.write_siocnt(SIOCNT_IRQ); sio.write_data8(0x34); assert!(sio.is_busy()); assert!(!sio.advance_cycles(TRANSFER_8_CYCLES - 1)); assert!(sio.advance_cycles(1)); assert_eq!(sio.serial_data8(), 0x34); assert!(sio.take_irq()); }
    #[test]
    fn normal32_transfer_uses_longer_deterministic_window() { let mut sio = Sio::default(); sio.write_siocnt((1 << 12) | SIOCNT_IRQ); sio.transmit(0x1234); assert!(!sio.advance_cycles(TRANSFER_8_CYCLES)); assert!(sio.advance_cycles(TRANSFER_32_CYCLES - TRANSFER_8_CYCLES)); assert_eq!(sio.take_rx(), Some(0x1234)); }
    #[test]
    fn multiplayer_transfer_updates_local_peer_slot() { let mut sio = Sio::default(); sio.write_siocnt(2 << 12); sio.transmit(0xabcd); sio.advance_cycles(TRANSFER_32_CYCLES); assert_eq!(sio.multi[0], 0xabcd); assert_eq!(sio.take_rx(), Some(0xabcd)); }
    #[test]
    fn receive_raises_optional_irq() { let mut sio = Sio::default(); sio.write_siocnt(SIOCNT_IRQ); sio.receive(0x1234); assert_eq!(sio.take_rx(), Some(0x1234)); assert!(sio.take_irq()); assert!(!sio.take_irq()); }
}