//! GBA programmable timers (TM0..TM3).
//!
//! Timers advance from CPU cycles. Timer 0 is the source for the non-cascade
//! chain; later timers can count the overflow edge of their predecessor.

pub const TIMER_COUNT: usize = 4;
pub const TIMER0CNT_L: u32 = 0x0400_0100;
pub const TIMER0CNT_H: u32 = 0x0400_0102;
pub const TIMER1CNT_L: u32 = 0x0400_0104;
pub const TIMER1CNT_H: u32 = 0x0400_0106;
pub const TIMER2CNT_L: u32 = 0x0400_0108;
pub const TIMER2CNT_H: u32 = 0x0400_010a;
pub const TIMER3CNT_L: u32 = 0x0400_010c;
pub const TIMER3CNT_H: u32 = 0x0400_010e;

const PRESCALE: [u32; 4] = [1, 64, 256, 1024];
pub const CONTROL_PRESCALER_MASK: u16 = 0b11;
pub const CONTROL_CASCADE: u16 = 1 << 2;
pub const CONTROL_IRQ: u16 = 1 << 6;
pub const CONTROL_ENABLE: u16 = 1 << 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimerControl {
    pub prescaler: u8,
    pub cascade: bool,
    pub irq: bool,
    pub enable: bool,
}

impl TimerControl {
    pub const fn from_raw(raw: u16) -> Self {
        Self {
            prescaler: (raw & CONTROL_PRESCALER_MASK) as u8,
            cascade: raw & CONTROL_CASCADE != 0,
            irq: raw & CONTROL_IRQ != 0,
            enable: raw & CONTROL_ENABLE != 0,
        }
    }

    pub const fn raw(self) -> u16 {
        (self.prescaler as u16 & CONTROL_PRESCALER_MASK)
            | if self.cascade { CONTROL_CASCADE } else { 0 }
            | if self.irq { CONTROL_IRQ } else { 0 }
            | if self.enable { CONTROL_ENABLE } else { 0 }
    }

    #[inline]
    pub const fn period(self) -> u32 {
        let index = if self.prescaler > 3 { 3 } else { self.prescaler };
        PRESCALE[index as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy)]
pub struct Timer {
    reload: u16,
    counter: u16,
    control: TimerControl,
    cycle_accumulator: u32,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            reload: 0,
            counter: 0,
            control: TimerControl::default(),
            cycle_accumulator: 0,
        }
    }
}

impl Timer {
    pub fn reload(&self) -> u16 {
        self.reload
    }

    pub fn counter(&self) -> u16 {
        self.counter
    }

    pub fn control(&self) -> TimerControl {
        self.control
    }
