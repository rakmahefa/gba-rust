use gba_runtime::bus;
use gba_runtime::mmio::{DISPSTAT_HBLANK_IRQ, DISPSTAT_VBLANK_IRQ};
use gba_runtime::mmio_devices::{DMA0CNT_H, DMA0CNT_L, DMA0DAD, DMA0SAD, TIMER0CNT_H, TIMER0CNT_L};
use gba_runtime::scheduler::{CYCLES_PER_SCANLINE, HBLANK_START_CYCLES, VBLANK_START_LINE};
use gba_runtime::Runtime;

const DMA_ENABLE: u16 = 1 << 15;
const DMA_IRQ: u16 = 1 << 14;
const DMA_REPEAT: u16 = 1 << 9;
const DMA_HBLANK: u16 = 2 << 12;
const TIMER_ENABLE: u16 = 1 << 7;
const TIMER_IRQ: u16 = 1 << 6;

#[test]
fn mmio_programmed_immediate_dma_reaches_memory_and_irq_boundary() {
    let mut runtime = Runtime::new();
    runtime.write16(bus::EWRAM_START, 0x1234);
    runtime.write32(DMA0SAD.address, bus::EWRAM_START);
    runtime.write32(DMA0DAD.address, bus::EWRAM_START + 0x100);
    runtime.write16(DMA0CNT_L.address, 1);
    runtime.interrupts.ie = gba_runtime::IRQ_DMA0;
    runtime.write16(DMA0CNT_H.address, DMA_ENABLE | DMA_IRQ);

    runtime.advance_cycles(2);
    assert_eq!(runtime.read16(bus::EWRAM_START + 0x100), 0x1234);
    assert_eq!(runtime.dma.active(), Some(0));

    let completion = runtime.dma.busy_until();
    runtime.advance_cycles((completion - runtime.scheduler.now()) as u32);
    assert_eq!(runtime.dma.active(), None);
    assert_ne!(runtime.interrupts.iflags & gba_runtime::IRQ_DMA0, 0);
}

#[test]
fn hblank_dma_is_triggered_by_the_ppu_timeline_and_can_raise_irq() {
    let mut runtime = Runtime::new();
    runtime.write16(bus::EWRAM_START, 0xabcd);
    runtime.write32(DMA0SAD.address, bus::EWRAM_START);
    runtime.write32(DMA0DAD.address, bus::EWRAM_START + 0x200);
    runtime.write16(DMA0CNT_L.address, 1);
    runtime.write16(DMA0CNT_H.address, DMA_ENABLE | DMA_IRQ | DMA_REPEAT | DMA_HBLANK);
    runtime.interrupts.ie = gba_runtime::IRQ_DMA0;

    runtime.advance_cycles(HBLANK_START_CYCLES as u32);
    assert_eq!(runtime.ppu.frame, 0);
    assert_eq!(runtime.read16(bus::EWRAM_START + 0x200), 0xabcd);
    assert_eq!(runtime.dma.active(), Some(0));

    let completion = runtime.dma.busy_until();
    runtime.advance_cycles((completion - runtime.scheduler.now()) as u32);
    assert_eq!(runtime.dma.active(), None);
    assert!(runtime.read16(DMA0CNT_H.address) & DMA_ENABLE != 0);
    assert_ne!(runtime.interrupts.iflags & gba_runtime::IRQ_DMA0, 0);
}

#[test]
fn timer_mmio_programming_advances_on_the_same_scheduler_clock() {
    let mut runtime = Runtime::new();
    runtime.interrupts.ie = gba_runtime::IRQ_TIMER0;
    runtime.write8(TIMER0CNT_L.address, 0xff);
    runtime.write8(TIMER0CNT_L.address + 1, 0xff);
    runtime.write8(TIMER0CNT_H.address, (TIMER_ENABLE | TIMER_IRQ) as u8);
    runtime.write8(TIMER0CNT_H.address + 1, 0);

    assert_eq!(runtime.read16(TIMER0CNT_L.address), 0xffff);
    assert_eq!(runtime.read16(TIMER0CNT_H.address), TIMER_ENABLE | TIMER_IRQ);
    runtime.advance_cycles(1);
    assert_eq!(runtime.timers[0].counter(), 0xffff);
    assert_ne!(runtime.interrupts.iflags & gba_runtime::IRQ_TIMER0, 0);
}

#[test]
fn ppu_hblank_and_vblank_irq_sources_share_the_machine_timeline() {
    let mut runtime = Runtime::new();
    runtime.dispstat = DISPSTAT_HBLANK_IRQ | DISPSTAT_VBLANK_IRQ;
    runtime.interrupts.ie = gba_runtime::IRQ_HBLANK | gba_runtime::IRQ_VBLANK;

    runtime.advance_cycles(HBLANK_START_CYCLES as u32);
    assert_ne!(runtime.interrupts.iflags & gba_runtime::IRQ_HBLANK, 0);
    assert_eq!(runtime.vcount, 0);

    let to_vblank = CYCLES_PER_SCANLINE * VBLANK_START_LINE as u64;
    runtime.advance_cycles((to_vblank - HBLANK_START_CYCLES) as u32);
    assert_eq!(runtime.vcount, VBLANK_START_LINE);
    assert_ne!(runtime.interrupts.iflags & gba_runtime::IRQ_VBLANK, 0);
}
