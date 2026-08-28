//! Deterministic APU state for the Phase C audio baseline.
//!
//! This module models runtime-facing audio state only. Host audio output is
//! deliberately outside the architectural layer.

pub const CPU_HZ: u64 = 16_777_216;
pub const SAMPLE_HZ: u64 = 32_768;
pub const CYCLES_PER_SAMPLE: u64 = CPU_HZ / SAMPLE_HZ;
pub const FIFO_CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fifo {
    data: [i8; FIFO_CAPACITY],
    read: usize,
    write: usize,
    len: usize,
}

impl Default for Fifo {
    fn default() -> Self { Self { data: [0; FIFO_CAPACITY], read: 0, write: 0, len: 0 } }
}

impl Fifo {
    pub fn clear(&mut self) { self.read = 0; self.write = 0; self.len = 0; }
    pub fn push(&mut self, sample: i8) -> bool {
        if self.len == FIFO_CAPACITY { return false; }
        self.data[self.write] = sample;
        self.write = (self.write + 1) % FIFO_CAPACITY;
        self.len += 1;
        true
    }
    pub fn pop(&mut self) -> Option<i8> {
        if self.len == 0 { return None; }
        let sample = self.data[self.read];
        self.read = (self.read + 1) % FIFO_CAPACITY;
        self.len -= 1;
        Some(sample)
    }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn is_full(&self) -> bool { self.len == FIFO_CAPACITY }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApuChannel {
    pub enabled: bool,
    pub frequency: u16,
    pub volume: u8,
}

impl Default for ApuChannel {
    fn default() -> Self { Self { enabled: false, frequency: 0, volume: 0 } }
}

#[derive(Debug, Clone, Default)]
pub struct Apu {
    pub samples_generated: u64,
    pub cycles_accumulated: u64,
    pub soundcnt_l: u16,
    pub soundcnt_h: u16,
    pub soundcnt_x: u16,
    pub channel: [ApuChannel; 4],
    fifo_a: Fifo,
    fifo_b: Fifo,
}

impl Apu {
    pub fn tick(&mut self, samples: u64) { self.samples_generated = self.samples_generated.wrapping_add(samples); }

    pub fn advance_cycles(&mut self, cycles: u64) -> u64 {
        self.cycles_accumulated = self.cycles_accumulated.saturating_add(cycles);
        let samples = self.cycles_accumulated / CYCLES_PER_SAMPLE;
        self.cycles_accumulated %= CYCLES_PER_SAMPLE;
        self.tick(samples);
        samples
    }

    pub fn write_fifo_a(&mut self, value: u32) {
        for sample in value.to_le_bytes().map(|byte| byte as i8) { let _ = self.fifo_a.push(sample); }
    }

    pub fn write_fifo_b(&mut self, value: u32) {
        for sample in value.to_le_bytes().map(|byte| byte as i8) { let _ = self.fifo_b.push(sample); }
    }

    pub fn pop_fifo_a(&mut self) -> Option<i8> { self.fifo_a.pop() }
    pub fn pop_fifo_b(&mut self) -> Option<i8> { self.fifo_b.pop() }
    pub fn fifo_a_len(&self) -> usize { self.fifo_a.len() }
    pub fn fifo_b_len(&self) -> usize { self.fifo_b.len() }

    pub fn reset_fifos(&mut self, fifo_a: bool, fifo_b: bool) {
        if fifo_a { self.fifo_a.clear(); }
        if fifo_b { self.fifo_b.clear(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_clock_is_deterministic_across_partial_cycle_segments() {
        let mut apu = Apu::default();
        assert_eq!(apu.advance_cycles(CYCLES_PER_SAMPLE - 1), 0);
        assert_eq!(apu.samples_generated, 0);
        assert_eq!(apu.advance_cycles(1), 1);
        assert_eq!(apu.samples_generated, 1);
    }

    #[test]
    fn fifo_preserves_order_and_has_architectural_capacity() {
        let mut fifo = Fifo::default();
        for sample in 0..FIFO_CAPACITY as i8 { assert!(fifo.push(sample)); }
        assert!(fifo.is_full());
        assert!(!fifo.push(127));
        for sample in 0..FIFO_CAPACITY as i8 { assert_eq!(fifo.pop(), Some(sample)); }
        assert!(fifo.is_empty());
        assert_eq!(fifo.pop(), None);
    }

    #[test]
    fn fifo_writes_are_little_endian_byte_streams() {
        let mut apu = Apu::default();
        apu.write_fifo_a(0x0403_0201);
        assert_eq!(apu.pop_fifo_a(), Some(1));
        assert_eq!(apu.pop_fifo_a(), Some(2));
        assert_eq!(apu.pop_fifo_a(), Some(3));
        assert_eq!(apu.pop_fifo_a(), Some(4));
    }

    #[test]
    fn fifo_reset_is_independent_per_channel() {
        let mut apu = Apu::default();
        apu.write_fifo_a(1);
        apu.write_fifo_b(2);
        apu.reset_fifos(true, false);
        assert_eq!(apu.fifo_a_len(), 0);
        assert_eq!(apu.fifo_b_len(), 1);
    }
}
