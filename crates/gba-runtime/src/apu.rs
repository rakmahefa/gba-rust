//! Deterministic GBA audio device model for Phase C.
//!
//! The module owns architectural audio state only. Host audio output is not
//! part of the runtime contract.

pub const CPU_HZ: u64 = 16_777_216;
pub const SAMPLE_HZ: u64 = 32_768;
pub const CYCLES_PER_SAMPLE: u64 = CPU_HZ / SAMPLE_HZ;
pub const FRAME_SEQUENCE_HZ: u64 = 512;
pub const CYCLES_PER_FRAME_STEP: u64 = CPU_HZ / FRAME_SEQUENCE_HZ;
pub const FIFO_CAPACITY: usize = 32;
pub const FIFO_REFILL_THRESHOLD: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fifo { data: [i8; FIFO_CAPACITY], read: usize, write: usize, len: usize }
impl Default for Fifo { fn default() -> Self { Self { data: [0; FIFO_CAPACITY], read: 0, write: 0, len: 0 } } }
impl Fifo {
    pub fn clear(&mut self) { self.read = 0; self.write = 0; self.len = 0; }
    pub fn push(&mut self, sample: i8) -> bool { if self.len == FIFO_CAPACITY { return false; } self.data[self.write] = sample; self.write = (self.write + 1) % FIFO_CAPACITY; self.len += 1; true }
    pub fn pop(&mut self) -> Option<i8> { if self.len == 0 { return None; } let sample = self.data[self.read]; self.read = (self.read + 1) % FIFO_CAPACITY; self.len -= 1; Some(sample) }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn is_full(&self) -> bool { self.len == FIFO_CAPACITY }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsgKind { Square1, Square2, Wave, Noise }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ApuChannel {
    pub enabled: bool,
    pub frequency: u16,
    pub volume: u8,
    pub duty: u8,
    pub length_counter: u8,
    pub envelope_period: u8,
    pub envelope_increase: bool,
    pub envelope_timer: u8,
    pub sweep_period: u8,
    pub sweep_shift: u8,
    pub sweep_negate: bool,
    pub phase: u16,
    pub lfsr: u16,
}
impl ApuChannel {
    fn new(kind: PsgKind) -> Self { Self { enabled: false, frequency: 0, volume: 0, duty: if matches!(kind, PsgKind::Square1 | PsgKind::Square2) { 2 } else { 0 }, length_counter: 0, envelope_period: 0, envelope_increase: false, envelope_timer: 0, sweep_period: 0, sweep_shift: 0, sweep_negate: false, phase: 0, lfsr: 0x7fff } }
    fn frame_length_tick(&mut self) { if self.length_counter != 0 { self.length_counter = self.length_counter.saturating_sub(1); if self.length_counter == 0 { self.enabled = false; } } }
    fn frame_envelope_tick(&mut self) {
        if self.envelope_period == 0 || !self.enabled { return; }
        if self.envelope_timer > 0 { self.envelope_timer -= 1; } else { self.envelope_timer = self.envelope_period; }
        if self.envelope_timer == 0 {
            self.envelope_timer = self.envelope_period;
            if self.envelope_increase { self.volume = self.volume.saturating_add(1).min(15); } else { self.volume = self.volume.saturating_sub(1); }
        }
    }
    fn frame_sweep_tick(&mut self, kind: PsgKind) {
        if !matches!(kind, PsgKind::Square1) || self.sweep_period == 0 || self.sweep_shift == 0 || !self.enabled { return; }
        self.frequency = if self.sweep_negate { self.frequency.saturating_sub(self.frequency >> self.sweep_shift) } else { self.frequency.saturating_add(self.frequency >> self.sweep_shift) };
        if self.frequency >= 2048 { self.enabled = false; }
    }
    fn sample(&mut self, kind: PsgKind) -> i16 {
        if !self.enabled { return 0; }
        match kind {
            PsgKind::Square1 | PsgKind::Square2 => { let duty_threshold = match self.duty & 3 { 0 => 32, 1 => 64, 2 => 128, _ => 192 }; let value = if (self.phase & 0xff) < duty_threshold { 1 } else { -1 }; self.phase = self.phase.wrapping_add((2048u16.saturating_sub(self.frequency)).max(1)); value * i16::from(self.volume) * 256 }
            PsgKind::Wave => { const WAVE: [i16; 16] = [1, 3, 5, 7, 7, 5, 3, 1, -1, -3, -5, -7, -7, -5, -3, -1]; let value = WAVE[((self.phase >> 8) & 15) as usize]; self.phase = self.phase.wrapping_add((2048u16.saturating_sub(self.frequency)).max(1)); value * i16::from(self.volume.saturating_mul(2)) * 32 }
            PsgKind::Noise => { let bit = ((self.lfsr ^ (self.lfsr >> 1)) & 1) != 0; self.lfsr = (self.lfsr >> 1) | if bit { 0x4000 } else { 0 }; if bit { i16::from(self.volume) * 256 } else { -(i16::from(self.volume) * 256) } }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SquareConfig { pub frequency: u16, pub duty: u8, pub length: u8, pub volume: u8, pub envelope_period: u8, pub envelope_increase: bool }

#[derive(Debug, Clone)]
pub struct Apu { pub samples_generated: u64, pub cycles_accumulated: u64, pub frame_step_accumulated: u64, pub frame_sequence_step: u8, pub soundcnt_l: u16, pub soundcnt_h: u16, pub soundcnt_x: u16, pub soundbias: u16, pub channel: [ApuChannel; 4], pub fifo_a_samples: u64, pub fifo_b_samples: u64, pub fifo_a_underruns: u64, pub fifo_b_underruns: u64, pub mixed_samples: u64, fifo_a_refill_pending: bool, fifo_b_refill_pending: bool, fifo_a: Fifo, fifo_b: Fifo }
impl Default for Apu { fn default() -> Self { Self { samples_generated: 0, cycles_accumulated: 0, frame_step_accumulated: 0, frame_sequence_step: 0, soundcnt_l: 0, soundcnt_h: 0, soundcnt_x: 0, soundbias: 0x0200, channel: [ApuChannel::new(PsgKind::Square1), ApuChannel::new(PsgKind::Square2), ApuChannel::new(PsgKind::Wave), ApuChannel::new(PsgKind::Noise)], fifo_a_samples: 0, fifo_b_samples: 0, fifo_a_underruns: 0, fifo_b_underruns: 0, mixed_samples: 0, fifo_a_refill_pending: false, fifo_b_refill_pending: false, fifo_a: Fifo::default(), fifo_b: Fifo::default() } } }
impl Apu {
    pub fn tick(&mut self, samples: u64) { self.samples_generated = self.samples_generated.wrapping_add(samples); for _ in 0..samples { let _ = self.mix_sample(); } }
    pub fn advance_cycles(&mut self, cycles: u64) -> u64 { self.cycles_accumulated = self.cycles_accumulated.saturating_add(cycles); self.frame_step_accumulated = self.frame_step_accumulated.saturating_add(cycles); let frame_steps = self.frame_step_accumulated / CYCLES_PER_FRAME_STEP; self.frame_step_accumulated %= CYCLES_PER_FRAME_STEP; for _ in 0..frame_steps { self.step_frame_sequence(); } let samples = self.cycles_accumulated / CYCLES_PER_SAMPLE; self.cycles_accumulated %= CYCLES_PER_SAMPLE; self.tick(samples); samples }
    fn step_frame_sequence(&mut self) {
        let step = self.frame_sequence_step;
        if step.is_multiple_of(2) { for channel in &mut self.channel { channel.frame_length_tick(); } }
        if step == 7 { for (index, channel) in self.channel.iter_mut().enumerate() { channel.frame_envelope_tick(); channel.frame_sweep_tick(match index { 0 => PsgKind::Square1, 1 => PsgKind::Square2, 2 => PsgKind::Wave, _ => PsgKind::Noise }); } }
        self.frame_sequence_step = (step + 1) & 7;
    }
    pub fn write_fifo_a(&mut self, value: u32) { for sample in value.to_le_bytes().map(|byte| byte as i8) { let _ = self.fifo_a.push(sample); } self.fifo_a_refill_pending = false; }
    pub fn write_fifo_b(&mut self, value: u32) { for sample in value.to_le_bytes().map(|byte| byte as i8) { let _ = self.fifo_b.push(sample); } self.fifo_b_refill_pending = false; }
    pub fn pop_fifo_a(&mut self) -> Option<i8> { self.fifo_a.pop() }
    pub fn pop_fifo_b(&mut self) -> Option<i8> { self.fifo_b.pop() }
    pub fn fifo_a_len(&self) -> usize { self.fifo_a.len() }
    pub fn fifo_b_len(&self) -> usize { self.fifo_b.len() }
    pub fn take_fifo_a_refill_request(&mut self) -> bool { let pending = self.fifo_a_refill_pending; self.fifo_a_refill_pending = false; pending }
    pub fn take_fifo_b_refill_request(&mut self) -> bool { let pending = self.fifo_b_refill_pending; self.fifo_b_refill_pending = false; pending }
    pub fn on_timer_overflow(&mut self, timer_index: usize) {
        let timer_a = usize::from((self.soundcnt_h >> 10) & 1); let timer_b = usize::from((self.soundcnt_h >> 14) & 1); let a_enabled = self.soundcnt_h & 0x0300 != 0; let b_enabled = self.soundcnt_h & 0x3000 != 0;
        if a_enabled && timer_index == timer_a { if self.fifo_a.pop().is_some() { self.fifo_a_samples += 1; self.fifo_a_refill_pending |= self.fifo_a.len() <= FIFO_REFILL_THRESHOLD; } else { self.fifo_a_underruns += 1; self.fifo_a_refill_pending = true; } }
        if b_enabled && timer_index == timer_b { if self.fifo_b.pop().is_some() { self.fifo_b_samples += 1; self.fifo_b_refill_pending |= self.fifo_b.len() <= FIFO_REFILL_THRESHOLD; } else { self.fifo_b_underruns += 1; self.fifo_b_refill_pending = true; } }
    }
    pub fn reset_fifos(&mut self, fifo_a: bool, fifo_b: bool) {
        if fifo_a { self.fifo_a.clear(); self.fifo_a_refill_pending = false; }
        if fifo_b { self.fifo_b.clear(); self.fifo_b_refill_pending = false; }
    }
    pub fn configure_square(&mut self, channel: usize, config: SquareConfig) {
        if let Some(channel) = self.channel.get_mut(channel.min(1)) { channel.enabled = true; channel.frequency = config.frequency.min(2047); channel.duty = config.duty & 3; channel.length_counter = config.length; channel.volume = config.volume.min(15); channel.envelope_period = config.envelope_period & 7; channel.envelope_timer = channel.envelope_period; channel.envelope_increase = config.envelope_increase; }
    }
    pub fn configure_wave(&mut self, frequency: u16, length: u8, volume: u8) { let channel = &mut self.channel[2]; channel.enabled = true; channel.frequency = frequency.min(2047); channel.length_counter = length; channel.volume = volume.min(15); }
    pub fn configure_noise(&mut self, length: u8, volume: u8, envelope_period: u8, envelope_increase: bool) { let channel = &mut self.channel[3]; channel.enabled = true; channel.length_counter = length; channel.volume = volume.min(15); channel.envelope_period = envelope_period & 7; channel.envelope_timer = channel.envelope_period; channel.envelope_increase = envelope_increase; channel.lfsr = 0x7fff; }
    fn mix_sample(&mut self) -> i16 { if self.soundcnt_x & 0x0080 == 0 { return 0; } let mut psg = 0i32; psg += i32::from(self.channel[0].sample(PsgKind::Square1)); psg += i32::from(self.channel[1].sample(PsgKind::Square2)); psg += i32::from(self.channel[2].sample(PsgKind::Wave)); psg += i32::from(self.channel[3].sample(PsgKind::Noise)); let direct_a = if self.soundcnt_h & 0x0004 != 0 { self.fifo_a.pop().map(i32::from).unwrap_or(0) * 256 } else { 0 }; let direct_b = if self.soundcnt_h & 0x0008 != 0 { self.fifo_b.pop().map(i32::from).unwrap_or(0) * 256 } else { 0 }; let master = ((self.soundcnt_l & 0x0007) + ((self.soundcnt_l >> 4) & 0x0007)) as i32 + 2; let total = (psg * master / 16) + direct_a + direct_b; self.mixed_samples = self.mixed_samples.wrapping_add(1); total.clamp(i16::MIN as i32, i16::MAX as i32) as i16 }
    pub fn mixed_sample_count(&self) -> u64 { self.mixed_samples }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sample_clock_is_deterministic_across_partial_cycle_segments() { let mut apu = Apu::default(); assert_eq!(apu.advance_cycles(CYCLES_PER_SAMPLE - 1), 0); assert_eq!(apu.samples_generated, 0); assert_eq!(apu.advance_cycles(1), 1); assert_eq!(apu.samples_generated, 1); }
    #[test] fn frame_sequencer_runs_at_512_hz() { let mut apu = Apu::default(); assert_eq!(apu.advance_cycles(CYCLES_PER_FRAME_STEP - 1), 63); assert_eq!(apu.frame_sequence_step, 0); assert_eq!(apu.advance_cycles(1), 1); assert_eq!(apu.frame_sequence_step, 1); }
    #[test] fn frame_sequence_has_eight_deterministic_steps() { let mut apu = Apu::default(); apu.advance_cycles(CYCLES_PER_FRAME_STEP * 8); assert_eq!(apu.frame_sequence_step, 0); }
    #[test] fn length_counter_disables_channel() { let mut apu = Apu::default(); apu.channel[0].enabled = true; apu.channel[0].length_counter = 1; apu.advance_cycles(CYCLES_PER_FRAME_STEP * 2); assert!(!apu.channel[0].enabled); }
    #[test] fn envelope_changes_on_frame_step_seven() { let mut apu = Apu::default(); apu.channel[0].enabled = true; apu.channel[0].volume = 5; apu.channel[0].envelope_period = 1; apu.channel[0].envelope_timer = 1; apu.channel[0].envelope_increase = true; apu.advance_cycles(CYCLES_PER_FRAME_STEP * 8); assert_eq!(apu.channel[0].volume, 6); }
    #[test] fn timer_overflow_selects_enabled_fifo_and_requests_refill() { let mut apu = Apu { soundcnt_h: 0x0300 | 0x3000 | 0x4000, ..Default::default() }; apu.write_fifo_a(0x0403_0201); apu.write_fifo_b(0x0807_0605); apu.on_timer_overflow(0); assert_eq!(apu.fifo_a_len(), 3); assert_eq!(apu.fifo_b_len(), 4); assert!(apu.take_fifo_a_refill_request()); apu.on_timer_overflow(1); assert_eq!(apu.fifo_b_len(), 3); assert!(apu.take_fifo_b_refill_request()); }
    #[test] fn underrun_requests_refill_and_counts_it() { let mut apu = Apu { soundcnt_h: 0x0300, ..Default::default() }; apu.on_timer_overflow(0); assert_eq!(apu.fifo_a_underruns, 1); assert!(apu.take_fifo_a_refill_request()); }
    #[test] fn square_channel_can_produce_mixed_samples() { let mut apu = Apu { soundcnt_x: 0x0080, ..Default::default() }; apu.configure_square(0, SquareConfig { frequency: 512, duty: 2, length: 10, volume: 8, envelope_period: 0, envelope_increase: false }); apu.advance_cycles(CYCLES_PER_SAMPLE); assert!(apu.mixed_sample_count() >= 1); }
}
