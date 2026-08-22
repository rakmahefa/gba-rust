# Phase 7 — Real-ROM Execution & Hardware Boundary Validation

Phase 7 validates the boundary between a statically generated GBA program and the runtime hardware contract without committing copyrighted game ROM data.

## Validation pipeline

```text
local .gba ROM
     |
     v
Cartridge image mapping at 0x08000000
     |
     v
reachable CFG analysis
     |
     v
Rust code generation
     |
     v
GeneratedExecutionContract
     |
     v
GBA runtime
     |
     +-- cartridge bus reads
     +-- CPU architectural state
     +-- timing scheduler
     +-- hardware IRQ boundaries
     +-- BIOS / exception boundary
```

## Harness

`crates/gba-cli/tests/real_rom_execution.rs` is opt-in through `GBA_REAL_ROM`.

Without that variable, the integration test exits successfully and reports that no external ROM was configured. With a local ROM it performs:

1. minimum GBA image-size validation;
2. static analysis from `0x08000000` in ARM state;
3. validation that discovered blocks remain inside the cartridge mapping;
4. byte/halfword/word cartridge bus read preflight against the source image;
5. generated Rust compilation against `gba-runtime`;
6. bounded generated execution with a deterministic 512-step limit;
7. ARM/Thumb PC alignment validation;
8. explicit failure if execution escapes into an unlinked exception vector.

## Running it

```bash
GBA_REAL_ROM=/path/to/game.gba cargo test -p gba-cli --test real_rom_execution -- --nocapture
```

The ROM is never copied into tracked repository fixtures.

## Scope

A successful bounded run establishes that the analyzed ROM crosses the current static-analysis, generated-code, cartridge-bus and runtime execution boundaries without violating the current contract. It does **not** claim full game compatibility.

The next validation expansions are long-running real-ROM regions, broader ARM/Thumb coverage, DMA and timer interactions, complete MMIO semantics, PPU/APU behavior, cartridge wait states and save protocols, and larger interrupt/exception paths.
