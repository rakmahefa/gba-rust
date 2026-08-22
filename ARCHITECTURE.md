# gba-rust architecture

`gba-rust` is a **static GBA recompiler**, not a conventional interpreter emulator.

The ROM is statically decoded into an intermediate representation and Rust source. The generated Rust executes against a separate Rust runtime implementing the GBA hardware contract.

## Layers

- `gba-recompiler`: ARM/Thumb decoding, CFG discovery, IR and Rust code generation.
- `gba-runtime`: CPU state, bus/cartridge, PPU, APU and timing-facing runtime services.
- `gba-cli`: development harness for analyzing a ROM and bootstrapping the runtime.

The runtime is intentionally independent from the generated program. The normal execution path is generated native Rust code; an instruction interpreter is not the architectural center.

## Generated execution contract

Generated basic blocks terminate through `GeneratedBlockExit`. Direct and dynamic CFG transitions must satisfy alignment and linked-CFG validation before the runtime dispatches the next block.

Architectural exceptions use the same boundary. `GeneratedBlockExit::Exception` asks the runtime to perform exception entry, including mode banking, SPSR capture, CPSR masking, Thumb clearing and vector selection. When the exception vector is not part of the generated CFG, the runtime returns `GeneratedExecutionExit::ExceptionVector` instead of silently dispatching an unresolved address.

BIOS SWIs use this exception state as well: the runtime enters Supervisor mode before executing the modeled BIOS service, restores the caller's CPSR/banked registers for returning SWIs, and leaves the Supervisor state active for non-returning services such as HALT/STOP.

### Phase 4: reentrant BIOS/IRQ execution

Generated execution treats an IRQ as a **block-boundary transition**, not as a side effect of `tick()`. A pending enabled IRQ is observed by the generated dispatcher before the next block executes; the dispatcher establishes the architectural PC for that boundary and then reuses the runtime exception-entry contract. This preserves the interrupted resume point, CPSR and banked registers while preventing an IRQ from mutating CPU mode in the middle of a generated instruction.

Exception-return instructions that write `PC` with the `S` bit set are handled as real architectural exception returns. The generated ARM path evaluates the return target, asks the runtime to restore the active SPSR/banked state, and only then emits the CFG return transition. Ordinary `BX LR`/function returns remain distinct from these architectural restores at the runtime level.

Nested exception entry is therefore reentrant: an IRQ taken while executing Supervisor/other privileged code captures the current mode's CPSR in `SPSR_irq`, uses the IRQ banked `SP/LR`, and can restore the interrupted privileged context through the same exception-return primitive.

BIOS HALT/IntrWait continue to unmask the IRQ path while waiting. The generated execution contract keeps the asynchronous hardware mechanism in the runtime while the generated CFG remains responsible only for linked control-flow targets.

## GBA bus and memory contract

The runtime exposes a single CPU bus address decoder before device-specific semantics. `BusRegion` classifies addresses and produces a canonical physical offset so generated code, DMA and future timing-aware devices can share one memory map.

The contract currently models:

- BIOS `0x00000000-0x00003FFF` as read-only;
- EWRAM `0x02000000-0x02FFFFFF` mirrored every 256 KiB;
- IWRAM `0x03000000-0x03FFFFFF` mirrored every 32 KiB;
- MMIO `0x04000000-0x040003FF` as an explicit device boundary;
- palette RAM `0x05000000-0x05FFFFFF` mirrored every 1 KiB;
- VRAM `0x06000000-0x06FFFFFF` with the GBA-specific 128 KiB mirror pattern over 96 KiB of storage;
- OAM `0x07000000-0x07FFFFFF` mirrored every 1 KiB;
- Game Pak ROM wait-state windows `0x08000000-0x0DFFFFFF` as aliases of one cartridge image;
- SRAM/Flash `0x0E000000-0x0FFFFFFF` mirrored over the 64 KiB bus window.

Device behavior remains separate from address classification. Video memory byte writes follow their GBA halfword rules, cartridge save reads/writes follow the narrow external bus behavior, and ARM unaligned word reads continue through the architectural rotation primitive.

Timing, waitstates, DMA arbitration and MMIO register completeness are deliberately not folded into the first bus layer; they build on this stable address contract.

## Central event and timing scheduler

`gba-runtime::TimingScheduler` is the single monotonic machine clock for time-driven hardware. Generated CPU execution advances the scheduler through `Runtime::advance_cycles(cycles)`. The scheduler does not execute CPU instructions itself; instead it identifies the next hardware boundary and lets the runtime advance continuous devices exactly up to that cycle before processing the event.

The event queue is deterministic and ordered by `(cycle, insertion sequence, event kind)`. This gives CPU-visible hardware behavior a stable ordering when multiple devices become due on the same cycle.

Current scheduler event classes are:

- PPU HBlank start;
- PPU scanline boundary and VBlank transition;
- DMA channel completion boundaries;
- explicit IRQ sampling boundaries.

Timers are continuous time-driven devices on this same clock. When `advance_cycles` crosses an event boundary, all timer state is first advanced for the exact elapsed segment. Timer overflows therefore become pending IRQ hardware state at a precise cycle without forcing an exception transition in the middle of a generated instruction. The generated dispatcher or an explicit IRQ-sample event performs the architectural IRQ entry.

PPU timing currently establishes HBlank, scanline, VBlank and VCOUNT interrupt boundaries without claiming that the complete renderer has been implemented. DMA completion events establish the timing contract for the future DMA engine; actual transfer arbitration and bus ownership remain separate work.

This architecture deliberately separates three concepts:

1. **time progression** — the scheduler clock;
2. **hardware side effects** — timers, PPU, DMA and IRQ requests;
3. **architectural CPU transitions** — generated-block dispatch, exception entry and exception return.

This keeps asynchronous hardware deterministic while preserving the existing block-boundary exception model.

## Cartridge saves

Battery-backed SRAM/Flash/EEPROM belongs to the cartridge model. It is persisted as `<game>.sav`; this is **not** a savestate. Writes are dirty-tracked, flushed atomically, and the previous save is retained as `<game>.sav.bak` when possible.

## Roadmap

1. Complete ARM7TDMI ARM/Thumb decoder and static CFG recovery.
2. Introduce a typed IR and basic-block/function analysis.
3. Generate executable Rust with explicit runtime calls for memory, branches, DMA, I/O and BIOS exception services.
4. Establish the GBA bus and memory contract before layering timing-sensitive hardware devices.
5. Establish deterministic event/timing scheduling across CPU, timers, PPU, DMA and IRQ.
6. Implement PPU modes 0-5, sprites and windows on the scheduler timeline.
7. Implement DMA arbitration/transfer semantics, APU, keypad and remaining IRQ sources.
8. Complete SRAM/Flash/EEPROM protocols and save detection.
9. Add egui frontend and deterministic regression tests against FireRed/Emerald.
10. Add native-code-oriented optimizations: block chaining, constant propagation, memory specialization and hot-path inlining.
