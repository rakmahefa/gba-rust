//! Deterministic GBA event/timing scheduler.
//!
//! CPU execution advances the scheduler clock. Device state is advanced up to
//! each queued architectural event boundary, so timers and asynchronous
//! hardware events observe the same monotonic cycle timeline.
//!
//! The display timing constants model the nominal GBA LCD timing: 160 visible
//! lines + 68 VBlank lines, with each line split into 1004 HDraw cycles and
//! 228 HBlank cycles.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub const HDRAW_CYCLES: u64 = 1_004;
pub const HBLANK_CYCLES: u64 = 228;
pub const CYCLES_PER_SCANLINE: u64 = HDRAW_CYCLES + HBLANK_CYCLES;
pub const VISIBLE_SCANLINES: u16 = 160;
pub const VBLANK_SCANLINES: u16 = 68;
pub const SCANLINES_PER_FRAME: u16 = VISIBLE_SCANLINES + VBLANK_SCANLINES;
pub const HBLANK_START_CYCLES: u64 = HDRAW_CYCLES;
pub const VBLANK_START_LINE: u16 = VISIBLE_SCANLINES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    PpuHBlankStart,
    PpuScanline,
    PpuVBlankStart,
    DmaArbitrate,
    DmaComplete { channel: u8 },
    IrqSample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub cycle: u64,
    pub sequence: u64,
    pub kind: EventKind,
}

impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cycle
            .cmp(&self.cycle)
            .then_with(|| other.sequence.cmp(&self.sequence))
            .then_with(|| other.kind.cmp(&self.kind))
    }
}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct TimingScheduler {
    now: u64,
    next_sequence: u64,
    queue: BinaryHeap<ScheduledEvent>,
}

impl Default for TimingScheduler {
    fn default() -> Self { Self::new() }
}

impl TimingScheduler {
    pub fn new() -> Self {
        Self { now: 0, next_sequence: 0, queue: BinaryHeap::new() }
    }

    pub fn now(&self) -> u64 { self.now }
    pub fn pending_events(&self) -> usize { self.queue.len() }

    pub fn schedule_at(&mut self, cycle: u64, kind: EventKind) {
        assert!(cycle >= self.now, "cannot schedule an event in the past");
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.queue.push(ScheduledEvent { cycle, sequence, kind });
    }

    pub fn schedule_in(&mut self, delta: u64, kind: EventKind) {
        self.schedule_at(self.now.saturating_add(delta), kind);
    }

    pub fn next_event(&self) -> Option<ScheduledEvent> { self.queue.peek().copied() }
    pub fn pop_event(&mut self) -> Option<ScheduledEvent> { self.queue.pop() }

    pub fn advance_to(&mut self, cycle: u64) {
        assert!(cycle >= self.now, "scheduler clock cannot move backwards");
        self.now = cycle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_ordered_by_cycle_then_insertion_sequence() {
        let mut scheduler = TimingScheduler::new();
        scheduler.schedule_at(20, EventKind::IrqSample);
        scheduler.schedule_at(10, EventKind::PpuScanline);
        scheduler.schedule_at(20, EventKind::DmaComplete { channel: 0 });
        assert_eq!(scheduler.pop_event().unwrap().cycle, 10);
        assert_eq!(scheduler.pop_event().unwrap().kind, EventKind::IrqSample);
        assert_eq!(scheduler.pop_event().unwrap().kind, EventKind::DmaComplete { channel: 0 });
    }

    #[test]
    fn scheduler_clock_is_monotonic() {
        let mut scheduler = TimingScheduler::new();
        scheduler.advance_to(123);
        assert_eq!(scheduler.now(), 123);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scheduler.advance_to(122)));
        assert!(result.is_err());
    }

    #[test]
    fn schedule_in_uses_current_cycle() {
        let mut scheduler = TimingScheduler::new();
        scheduler.advance_to(100);
        scheduler.schedule_in(25, EventKind::IrqSample);
        assert_eq!(scheduler.next_event().unwrap().cycle, 125);
    }

    #[test]
    fn display_timing_matches_nominal_gba_scanline_and_frame_lengths() {
        assert_eq!(HDRAW_CYCLES, 1_004);
        assert_eq!(HBLANK_CYCLES, 228);
        assert_eq!(CYCLES_PER_SCANLINE, 1_232);
        assert_eq!(SCANLINES_PER_FRAME, 228);
        assert_eq!(CYCLES_PER_SCANLINE * u64::from(SCANLINES_PER_FRAME), 280_896);
    }

    #[test]
    fn vblank_starts_after_the_160_visible_scanlines() {
        assert_eq!(VBLANK_START_LINE, 160);
        assert_eq!(VISIBLE_SCANLINES + VBLANK_SCANLINES, SCANLINES_PER_FRAME);
    }
}
