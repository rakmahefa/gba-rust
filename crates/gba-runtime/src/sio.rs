//! Deterministic serial I/O (SIO) baseline for Phase C.
//!
//! The model keeps the architectural control register and a small transmit /
//! receive state. Link protocols and multiplayer timing build on this boundary.

// SIOCNT exposes the serial mode in bits 12-13 and the IRQ/busy controls in
// bits 14-15. Bits 9-11 are reserved in this baseline and are masked out.
pub const SIOCNT_MASK: u16 = 0xf1ff;
pub const SIODATA8_MASK: u8 = 0xff;
pub const RCNT_MASK: u16 = 0x800f;

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
}

impl Sio {
    pub fn write_siocnt(&mut self, value: u16) {
        self.siocnt = value & SIOCNT_MASK;
        self.mode = match (self.siocnt >> 12) & 0x3 {
            0 => SioMode::Normal8,
            1 => SioMode::Normal32,
            2 => SioMode::Multiplayer,
            _ => SioMode::Uart,
        };
    }

    pub fn write_rcnt(&mut self, value: u16) { self.rcnt = value & RCNT_MASK; }

    pub fn transmit(&mut self, value: u16) {
        self.tx_last = Some(value);
        if self.siocnt & (1 << 14) != 0 { self.irq_pending = true; }
    }

    pub fn receive(&mut self, value: u16) {
        self.rx_pending = Some(value);
        if self.siocnt & (1 << 14) != 0 { self.irq_pending = true; }
    }

    pub fn take_rx(&mut self) -> Option<u16> { self.rx_pending.take() }
    pub fn take_irq(&mut self) -> bool { let pending = self.irq_pending; self.irq_pending = false; pending }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_control_masks_reserved_bits_and_selects_mode() {
        let mut sio = Sio::default();
        sio.write_siocnt(0xffff);
        assert_eq!(sio.siocnt, SIOCNT_MASK);
        assert_eq!(sio.mode, SioMode::Uart);
    }

    #[test]
    fn serial_receive_is_consumable_and_raises_optional_irq() {
        let mut sio = Sio::default();
        sio.write_siocnt(1 << 14);
        sio.receive(0x1234);
        assert_eq!(sio.take_rx(), Some(0x1234));
        assert!(sio.take_irq());
        assert!(!sio.take_irq());
    }

    #[test]
    fn serial_transmit_retains_last_word() {
        let mut sio = Sio::default();
        sio.transmit(0xabcd);
        assert_eq!(sio.tx_last, Some(0xabcd));
    }
}
