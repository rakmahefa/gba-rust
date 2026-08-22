# Phase 8.2 — DMA architectural execution and bus arbitration

## Contract

The GBA has four DMA channels. Arbitration priority is fixed by channel number:

`DMA0 > DMA1 > DMA2 > DMA3`.

The DMA controller is a runtime device. Programmer-visible registers continue to
live in the existing MMIO backing map; the DMA controller synchronizes that
architectural state at runtime timing boundaries.

## Start conditions

- Immediate DMA has a two-cycle activation boundary after enable.
- VBlank DMA is requested when the scheduler enters the VBlank scanline.
- HBlank DMA is requested at each HBlank boundary.
- Special timing mode remains an explicit extension point for FIFO/refresh
  request sources and is not silently treated as ordinary HBlank/VBlank DMA.

## Transfer state

Each active channel keeps independent current source/destination addresses.
The implementation models:

- 16-bit and 32-bit transfers;
- source increment, decrement and fixed modes;
- destination increment, decrement, fixed and reload modes;
- zero-count expansion (`0x4000` transfers for DMA0..2 and `0x10000` for DMA3);
- repeat behavior for timed DMA;
- automatic disable after non-repeating/immediate completion;
- DMA IRQ requests on completion.

The final current source/destination addresses are reflected back into the
MMIO backing store, while the enable bit is cleared when architectural
completion disables the channel.

## Bus arbitration

A pending DMA request owns the CPU bus once the highest-priority ready channel
is selected. Lower-priority channels remain pending until the active channel
releases the bus. The runtime exposes `dma_bus_busy()` and generated execution
advances to the DMA completion boundary before executing another generated
block.

DMA completion is represented by the central `TimingScheduler`, so timer and
PPU state advance to the exact completion cycle before the completion event is
processed.

## Timing foundation

Transfer duration is derived from the source/destination bus regions and the
current Game Pak wait-state configuration. ROM transfers use the configured
initial and sequential waits; internal-memory pair timings use the established
GBA DMA timing table as the runtime foundation.

This is deliberately a bus-ownership and execution contract, not yet a complete
DMA subsystem for sound FIFOs, refresh requests, or full hardware contention
with a finished PPU renderer. Those request sources build on this arbitration
layer in later phases.
