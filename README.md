# gba-rust

Static Game Boy Advance recompiler that generates Rust and executes the generated program against a native Rust GBA runtime.

The project is designed around one principle: **the GBA ROM is compiled ahead of time into Rust; the runtime provides the hardware contract**. It is not intended to be a conventional instruction-by-instruction emulator with a UI layered on top.

> This project is inspired by the engineering direction of [`arcanite24/gb-recompiled`](https://github.com/arcanite24/gb-recompiled), but the implementation target here is a clean, Rust-first GBA architecture.

## Architecture

```text
                  GBA ROM (.gba)
                         │
                         ▼
              ┌─────────────────────┐
              │  Static Recompiler  │
              │                     │
              │ ARM / Thumb decode  │
              │ CFG recovery        │
              │ Function discovery  │
              │ IR / optimization   │
              │ Rust code generation│
              └──────────┬──────────┘
                         │
                         ▼
                  Generated Rust
                         │
                         ▼
              ┌─────────────────────┐
              │     GBA Runtime     │
              │                     │
              │ CPU / ARM7TDMI      │
              │ Memory / Bus        │
              │ PPU                 │
              │ APU                 │
              │ DMA / Timers / IRQ  │
              │ Cartridge / Mappers │
              │ Save memory         │
              └──────────┬──────────┘
                         │
                         ▼
                    egui / eframe
```

The separation is intentional:

- **Recompiler:** understands the ROM and produces Rust.
- **Generated code:** contains the statically recovered game logic.
- **Runtime:** implements the hardware services that generated code depends on.
- **egui:** presents the runtime and emulator controls without owning emulation semantics.

## Workspace

```text
crates/
├── gba-recompiler/
│   └── Static ROM analysis and Rust code generation
│
├── gba-runtime/
│   └── Rust GBA hardware/runtime layer
│
└── gba-cli/
    └── Development and recompilation harness
```

`ARCHITECTURE.md` contains the current architectural contract and roadmap.

## Recompiler

The recompiler is the long-term execution path of the project.

Its responsibilities are:

1. Parse the GBA ROM and cartridge metadata.
2. Decode ARM and Thumb code.
3. Discover executable regions, functions and basic blocks.
4. Recover a control-flow graph.
5. Translate instructions into a typed intermediate representation.
6. Apply safe static optimizations.
7. Generate deterministic Rust source.
8. Let the Rust compiler produce optimized native code.

The canonical direction is therefore:

```text
GBA machine code
    → static analysis
    → GBA IR
    → optimized IR
    → generated Rust
    → native machine code
```

An instruction interpreter may exist as a development/reference mechanism, but it is **not the architectural center of gba-rust**.

## Runtime

The runtime is the hardware boundary consumed by generated code.

Targeted subsystems include:

- ARM7TDMI CPU state and execution support
- memory bus and GBA memory map
- PPU and 240×160 framebuffer
- APU/audio generation
- DMA
- timers
- interrupts and scheduler
- keypad/input
- cartridge hardware
- SRAM, Flash and EEPROM save devices
- mapper/protocol-specific cartridge behavior

The runtime is kept independent from the source ROM and from egui so individual subsystems can be optimized or replaced without changing the frontend architecture.

## Cartridge saves

gba-rust deliberately **does not use savestates as the game's save mechanism**.

The save system models the real persistent cartridge storage used by GBA games:

```text
Game code
   │
   ▼
Cartridge save device
   │
   ├── SRAM
   ├── Flash 64 KiB
   ├── Flash 128 KiB
   └── EEPROM
          │
          ▼
      Save Manager
          │
          ├── <game>.sav
          └── <game>.sav.bak
```

The intended behavior is:

- cartridge writes mark save memory dirty;
- dirty data is flushed to disk;
- writes are performed atomically through a temporary file;
- the previous `.sav` is retained as `.sav.bak` when possible;
- the save is flushed again during shutdown;
- no CPU/PPU/RAM snapshot is required for normal game saving.

Save files live outside the ROM directory so the ROM remains immutable.

## Test ROMs

The repository contains development ROMs under `roms/`, including:

- `1636 - Pokemon Fire Red (U)(Squirrels).gba`
- `1986 - Pokemon Emerald (U)(TrashMan).gba`

The CLI defaults to the FireRed ROM when no path is supplied.

```bash
cargo run -p gba-cli --release
```

Or provide an explicit ROM:

```bash
cargo run -p gba-cli --release -- \
  "roms/1636 - Pokemon Fire Red (U)(Squirrels).gba"
```

## Development

Prerequisites:

- Rust stable toolchain
- Cargo

Format, test and check the workspace with:

```bash
cargo fmt
cargo test --workspace
cargo check --workspace --all-targets
```

CI runs the same core validation through GitHub Actions.

## Optimization strategy

Performance work is deliberately staged so correctness remains measurable:

1. Correct static ARM/Thumb decoding.
2. Deterministic CFG and function discovery.
3. Typed IR with explicit side effects.
4. Constant propagation and dead-code elimination.
5. Memory-access specialization where provably safe.
6. Basic-block linking and branch-target specialization.
7. Rust/LLVM optimization of generated code.
8. Runtime fast paths for hot hardware operations.
9. Deterministic regression and benchmark suites.

The project favors **compile-time work over runtime interpretation** whenever the information is statically recoverable from the ROM.

## Current status

The repository is in the foundation phase. The workspace already separates the static recompiler from the Rust runtime and includes the persistent cartridge-save model. The next major milestone is completing the ARM/Thumb static analysis pipeline and replacing the initial decoder with a real CFG/IR/code-generation pipeline.

This repository should therefore be understood as an actively developed **static GBA recompilation system**, not as a finished general-purpose GBA emulator.

## License

See the repository license for the current project terms.
