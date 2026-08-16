# gba-rust architecture

`gba-rust` is a **static GBA recompiler**, not a conventional interpreter emulator.

The ROM is statically decoded into an intermediate representation and Rust source. The generated Rust executes against a separate Rust runtime implementing the GBA hardware contract.

## Layers

- `gba-recompiler`: ARM/Thumb decoding, CFG discovery, IR and Rust code generation.
- `gba-runtime`: CPU state, bus/cartridge, PPU, APU and timing-facing runtime services.
- `gba-cli`: development harness for analyzing a ROM and bootstrapping the runtime.

The runtime is intentionally independent from the generated program. The normal execution path is generated native Rust code; an instruction interpreter is not the architectural center.

## Cartridge saves

Battery-backed SRAM/Flash/EEPROM belongs to the cartridge model. It is persisted as `<game>.sav`; this is **not** a savestate. Writes are dirty-tracked, flushed atomically, and the previous save is retained as `<game>.sav.bak` when possible.

## Roadmap

1. Complete ARM7TDMI ARM/Thumb decoder and static CFG recovery.
2. Introduce a typed IR and basic-block/function analysis.
3. Generate executable Rust with explicit runtime calls for memory, branches, DMA and I/O.
4. Implement PPU modes 0-5, sprites and windows.
5. Implement APU, timers, DMA, IRQ and keypad.
6. Complete SRAM/Flash/EEPROM protocols and save detection.
7. Add egui frontend and deterministic regression tests against FireRed/Emerald.
8. Add native-code-oriented optimizations: block chaining, constant propagation, memory specialization and hot-path inlining.
