use super::*;
use crate::mmio;
use crate::timers::{
    CONTROL_CASCADE, CONTROL_ENABLE, CONTROL_IRQ, TIMER0CNT_H, TIMER0CNT_L, TIMER1CNT_H,
    TIMER1CNT_L,
};
use crate::bios::{IRQ_TIMER0, IRQ_TIMER1};
use crate::cpu::CpuMode;

#[test]
fn timer0_overflow_sets_if_without_taking_irq_inside_the_instruction() {
    let mut runtime = Runtime::new();
    runtime.write16(TIMER0CNT_L, 0xffff);
    runtime.write16(TIMER0CNT_H, CONTROL_ENABLE | CONTROL_IRQ);
    runtime.write16(mmio::IE, IRQ_TIMER0);
    runtime.write16(mmio::IME, 1);

    runtime.tick(1);

    assert_eq!(runtime.read16(TIMER0CNT_L), 0xffff);
    assert_eq!(runtime.read16(crate::bios::IF), IRQ_TIMER0);
    assert_eq!(runtime.mode(), CpuMode::System);
    assert!(runtime.generated_irq_pending());
}

#[test]
fn timer_prescaler_accumulates_partial_cycles() {
    let mut runtime = Runtime::new();
    runtime.write16(TIMER0CNT_L, 0);
    runtime.write16(TIMER0CNT_H, CONTROL_ENABLE | 1);

    runtime.tick(63);
    assert_eq!(runtime.read16(TIMER0CNT_L), 0);
    runtime.tick(1);
    assert_eq!(runtime.read16(TIMER0CNT_L), 1);
}

#[test]
fn cascade_timer_consumes_predecessor_overflow_and_can_request_its_own_irq() {
    let mut runtime = Runtime::new();
    runtime.write16(TIMER0CNT_L, 0xffff);
    runtime.write16(TIMER0CNT_H, CONTROL_ENABLE);
    runtime.write16(TIMER1CNT_L, 0xffff);
    runtime.write16(TIMER1CNT_H, CONTROL_ENABLE | CONTROL_CASCADE | CONTROL_IRQ);

    runtime.tick(1);

    assert_eq!(runtime.read16(TIMER0CNT_L), 0xffff);
    assert_eq!(runtime.read16(TIMER1CNT_L), 0xffff);
    assert_eq!(runtime.read16(crate::bios::IF), IRQ_TIMER1);
}

#[test]
fn timer_control_mmio_round_trips_only_architectural_bits() {
    let mut runtime = Runtime::new();
    runtime.write16(TIMER0CNT_H, 0xffff);
    assert_eq!(runtime.read16(TIMER0CNT_H), 0x00c7);
}

#[test]
fn dispstat_compare_is_writable_while_status_bits_remain_read_only() {
    let mut runtime = Runtime::new();
    runtime.dispstat = mmio::DISPSTAT_VBLANK | mmio::DISPSTAT_HBLANK;
    runtime.write16(mmio::DISPSTAT, 0xabf8);
    assert_eq!(runtime.dispstat & mmio::DISPSTAT_STATUS_MASK, 0x0003);
    assert_eq!((runtime.read16(mmio::DISPSTAT) >> 8) as u8, 0xab);
}

#[test]
fn vcount_register_is_read_only_until_a_video_scheduler_updates_it() {
    let mut runtime = Runtime::new();
    runtime.vcount = 77;
    runtime.write16(mmio::VCOUNT, 1234);
    assert_eq!(runtime.read16(mmio::VCOUNT), 77);
}
