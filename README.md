# gba-rust

Native Game Boy Advance emulator written in Rust with an egui/eframe frontend.

This branch is a clean-room GBA architecture inspired by the engineering lessons of `arcanite24/gb-recompiled`, but it does not copy its C/C++ runtime. The target is a compact, cache-friendly Rust core with a future block-recompiler boundary and a native egui UI.

## Current architecture

- `gba-core`: cartridge, memory bus, ARM7TDMI interpreter, framebuffer and persistent cartridge save device.
- `gba-egui`: native desktop frontend using only Rust + egui/eframe.
- No savestates: cartridge SRAM/Flash/EEPROM data is persisted as `saves/<rom>.sav`.
- Atomic save writes with a `.sav.bak` rolling backup.
- GBA framebuffer: 240x160 RGB555.

## Run

```bash
cargo run --release -- "roms/1636 - Pokemon Fire Red (U)(Squirrels).gba"
```

The save file is deliberately separate from the ROM. A game writing its cartridge save area marks the save dirty; the frontend flushes it periodically and again on exit.

## Roadmap

1. Complete ARM7TDMI condition/timing semantics.
2. Complete Thumb instruction coverage and BIOS compatibility layer.
3. Implement PPU modes 0/1/2 with BG/OBJ/window priority and timing.
4. Add DMA, timers, keypad, interrupts and scheduler.
5. Add APU with batched audio output.
6. Add EEPROM/Flash command protocols and save-type detection from ROM behavior.
7. Add cached basic-block execution/recompilation without changing the canonical interpreter semantics.
8. Add deterministic ROM regression tests and performance benchmarks.

The current implementation is intentionally structured so these subsystems can be replaced/optimized independently without coupling the UI to emulation state.
