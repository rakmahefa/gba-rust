//! Deterministic Game Pak timing and prefetch model for Phase D.
//!
//! The GBA exposes three cartridge ROM banks (WS0/WS1/WS2) and one SRAM/save
//! wait-state setting through WAITCNT. ROM transfers are 16-bit wide; SRAM is
//! 8-bit. WAITCNT bit 14 enables the eight-halfword prefetch buffer.

use crate::bus::{ROM0_START, ROM1_START, ROM2_START, SAVE_START};

const WAITCNT_SRAM_MASK: u16 = 0x0003;
const WAITCNT_WS0_MASK: u16 = 0x000c;
const WAITCNT_WS0_N_MASK: u16 = 0x0010;
const WAITCNT_WS1_MASK: u16 = 0x0060;
const WAITCNT_WS1_N_MASK: u16 = 0x0080;
const WAITCNT_WS2_MASK: u16 = 0x0300;
const WAITCNT_WS2_N_MASK: u16 = 0x0400;
const WAITCNT_PREFETCH: u16 = 1 << 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitStateConfig {
    pub sram_cycles: u8,
    pub ws0_first: u8,
    pub ws0_second: u8,
    pub ws1_first: u8,
    pub ws1_second: u8,
    pub ws2_first: u8,
    pub ws2_second: u8,
    pub prefetch: bool,
}

impl Default for WaitStateConfig {
    fn default() -> Self {
        Self::from_waitcnt(0)
    }
}

impl WaitStateConfig {
    pub fn from_waitcnt(waitcnt: u16) -> Self {
        Self {
            sram_cycles: decode_wait(waitcnt & WAITCNT_SRAM_MASK),
            ws0_first: decode_wait((waitcnt & WAITCNT_WS0_MASK) >> 2),
            ws0_second: decode_second(waitcnt & WAITCNT_WS0_N_MASK, 0),
            ws1_first: decode_wait((waitcnt & WAITCNT_WS1_MASK) >> 5),
            ws1_second: decode_second(waitcnt & WAITCNT_WS1_N_MASK, 1),
            ws2_first: decode_wait((waitcnt & WAITCNT_WS2_MASK) >> 8),
            ws2_second: decode_second(waitcnt & WAITCNT_WS2_N_MASK, 2),
            prefetch: waitcnt & WAITCNT_PREFETCH != 0,
        }
    }

    pub fn rom_cycles_for_address(self, address: u32, sequential: bool) -> Option<u8> {
        let bank = Self::bank_for_address(address)?;
        Some(match bank {
            0 => if sequential { self.ws0_second } else { self.ws0_first },
            1 => if sequential { self.ws1_second } else { self.ws1_first },
            2 => if sequential { self.ws2_second } else { self.ws2_first },
            _ => unreachable!(),
        })
    }

    pub fn save_cycles(self) -> u8 { self.sram_cycles }

    pub fn bank_for_address(address: u32) -> Option<u8> {
        match address {
            ROM0_START..=0x09ff_ffff => Some(0),
            ROM1_START..=0x0bff_ffff => Some(1),
            ROM2_START..=0x0dff_ffff => Some(2),
            SAVE_START..=0x0fff_ffff => None,
            _ => None,
        }
    }
}

fn decode_wait(value: u16) -> u8 {
    match value & 0x3 {
        0 => 4,
        1 => 3,
        2 => 2,
        _ => 8,
    }
}

fn decode_second(value: u16, bank: u8) -> u8 {
    if value != 0 {
        1
    } else {
        match bank {
            0 => 2,
            1 => 4,
            2 => 8,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefetchBuffer {
    enabled: bool,
    next_address: Option<u32>,
    words: u8,
}

impl PrefetchBuffer {
    pub const CAPACITY: u8 = 8;

    pub fn new(enabled: bool) -> Self {
        Self { enabled, next_address: None, words: 0 }
    }

    pub fn enabled(&self) -> bool { self.enabled }

    pub fn invalidate(&mut self) {
        self.next_address = None;
        self.words = 0;
    }

    pub fn observe_fetch(&mut self, address: u32) -> bool {
        if !self.enabled { return false; }
        let hit = self.next_address == Some(address) && self.words != 0;
        if hit {
            self.next_address = Some(address.wrapping_add(2));
            self.words -= 1;
        }
        hit
    }

    pub fn refill(&mut self, next_address: u32) {
        if self.enabled {
            self.next_address = Some(next_address);
            self.words = Self::CAPACITY;
        } else {
            self.invalidate();
        }
    }

    pub fn available_words(&self) -> u8 { self.words }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waitcnt_decodes_default_timings() {
        let config = WaitStateConfig::from_waitcnt(0);
        assert_eq!(config.sram_cycles, 4);
        assert_eq!(config.ws0_first, 4);
        assert_eq!(config.ws0_second, 2);
        assert_eq!(config.ws1_first, 4);
        assert_eq!(config.ws1_second, 4);
        assert_eq!(config.ws2_first, 4);
        assert_eq!(config.ws2_second, 8);
        assert!(!config.prefetch);
    }

    #[test]
    fn waitcnt_decodes_fastest_second_waitstates_and_prefetch() {
        let waitcnt = WAITCNT_WS0_N_MASK | WAITCNT_WS1_N_MASK | WAITCNT_WS2_N_MASK | WAITCNT_PREFETCH;
        let config = WaitStateConfig::from_waitcnt(waitcnt);
        assert_eq!(config.ws0_second, 1);
        assert_eq!(config.ws1_second, 1);
        assert_eq!(config.ws2_second, 1);
        assert!(config.prefetch);
    }

    #[test]
    fn address_selects_correct_rom_waitstate_bank() {
        let config = WaitStateConfig::default();
        assert_eq!(config.rom_cycles_for_address(0x0800_0000, false), Some(4));
        assert_eq!(config.rom_cycles_for_address(0x0800_0002, true), Some(2));
        assert_eq!(config.rom_cycles_for_address(0x0a00_0000, false), Some(4));
        assert_eq!(config.rom_cycles_for_address(0x0c00_0000, false), Some(4));
        assert_eq!(config.rom_cycles_for_address(0x0e00_0000, false), None);
    }

    #[test]
    fn prefetch_buffer_is_eight_halfwords_and_tracks_sequential_hits() {
        let mut buffer = PrefetchBuffer::new(true);
        buffer.refill(0x0800_0100);
        assert_eq!(buffer.available_words(), 8);
        assert!(buffer.observe_fetch(0x0800_0100));
        assert_eq!(buffer.available_words(), 7);
        assert!(buffer.observe_fetch(0x0800_0102));
        assert_eq!(buffer.available_words(), 6);
        assert!(!buffer.observe_fetch(0x0800_0200));
        assert_eq!(buffer.available_words(), 6);
        buffer.invalidate();
        assert_eq!(buffer.available_words(), 0);
    }
}
