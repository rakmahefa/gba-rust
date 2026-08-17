# gba-rust

**gba-rust** is a Rust-first **static Game Boy Advance recompilation project**. It analyzes executable code from a GBA ROM, recovers reachable control flow and functions, lowers instructions into typed and semantic intermediate representations, performs conservative optimizations, and generates Rust code backed by a separate GBA runtime.

The project is **recompiler-first**, not interpreter-first: the long-term execution model is statically generated native Rust/LLVM code plus explicit runtime services.

> Inspired by the engineering direction of [`arcanite24/gb-recompiled`](https://github.com/arcanite24/gb-recompiled), but implemented as an independent Rust-first GBA architecture.

## Current status

`gba-rust` has progressed from a decoder/CFG prototype into a multi-stage static recompilation pipeline with an explicit ARM7TDMI execution contract.

The current `main` branch includes:

- ARM and original GBA Thumb (ARMv4T) decoding;
- reachable CFG recovery with ARM/Thumb mode-aware addresses;
- basic-block partitioning and function discovery;
- typed IR and semantic IR with structural validation;
- explicit register, flag, control-flow and memory effects;
- conservative, effect-aware optimization;
- deterministic Rust code generation;
- an ARM7TDMI architectural runtime model;
- generated-block execution contracts with deterministic halt/step-limit behavior;
- architectural regression fixtures and generated-execution differential validation;
- cartridge save foundations for SRAM, Flash and EEPROM variants.

The latest validation work adds independent reference-model fixtures for representative ARM execution, memory effects, branch/loop behavior and instruction identity. These tests are intended to catch semantic mismatches between analysis, generated-code contracts and the runtime.

**The project is not yet a complete playable GBA implementation.** The main remaining work is broader instruction/hardware coverage, a complete generated block dispatch/linking loop, and the GBA hardware contract: PPU, APU, DMA, timers, interrupts, keypad, memory-mapped I/O and cartridge timing/protocol details.

## Architecture

```text
                         GBA ROM (.gba)
                              │
                              ▼
                 ┌──────────────────────────┐
                 │      gba-recompiler      │
                 │                          │
                 │ ARM / Thumb decoder      │
                 │ CFG recovery             │
                 │ Function discovery       │
                 │ Typed IR                 │
                 │ Semantic IR              │
                 │ Conservative optimizer   │
                 │ Rust code generation     │
                 └────────────┬─────────────┘
                              │
                              ▼
                       Generated Rust
                              │
                              ▼
                 ┌──────────────────────────┐
                 │       gba-runtime        │
                 │                          │
                 │ ARM7TDMI state          │
                 │ Memory / cartridge       │
                 │ Save devices             │
                 │ PPU / APU foundations   │
                 │ Timing / execution API  │
                 └────────────┬─────────────┘
                              │
                              ▼
                       Native Rust / LLVM
```

The layers have deliberately different responsibilities:

- **`gba-recompiler`** analyzes ROM code and produces CFGs, IR and generated Rust.
- **Generated Rust** represents statically recovered program logic.
- **`gba-runtime`** provides CPU state, memory, cartridge and hardware-facing services.
- **`gba-cli`** is the development harness for ROM analysis and generated-source emission.
- **`gba-core`** contains lower-level CPU/bus/cartridge foundations and is currently outside the root workspace.
- **`gba-egui`** contains an `egui`/`eframe` frontend prototype and is currently outside the root workspace.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the architectural contract.

## Workspace

The root Cargo workspace currently contains:

```text
crates/
├── gba-recompiler/
│   └── ARM/Thumb decoding, CFG, function analysis, IR, optimization and codegen
│
├── gba-runtime/
│   └── ARM7TDMI state, memory, cartridge/save and runtime services
│
└── gba-cli/
    └── ROM analysis and generated-source development harness
```

The repository also contains:

```text
crates/
├── gba-core/
│   └── CPU, bus and cartridge foundations (prototype)
│
└── gba-egui/
    └── egui/eframe frontend (prototype)
```

The workspace uses **Rust 2021**. Release builds use thin LTO, one codegen unit and `panic = "abort"`.

## Static recompiler pipeline

### 1. Decode

The decoder represents instructions using their address, size, execution mode and operation. ARM and GBA Thumb decoding is separated into focused modules covering instruction families, memory operations and shared semantics.

GBA CPUs use **ARM state and the original ARMv4T Thumb instruction set**. This project is therefore **not a Thumb-2 recompiler**.

### 2. Discover reachable code

Analysis starts from an explicit ROM entry address and execution mode. Statically known successors are followed while preserving the ARM/Thumb mode in the CFG key.

```text
entry
  │
  ├── sequential successor
  ├── conditional branch target
  ├── unconditional branch target
  └── statically known exchange/call target
```

Unknown instructions and unresolved dynamic targets constrain static discovery rather than inventing unsafe successors.

### 3. Recover basic blocks

CFG leaders come from the entry point and control-flow targets. Blocks retain their execution mode and stop at control-flow boundaries.

Each block carries:

- a stable `BlockId`;
- an address and ARM/Thumb mode;
- decoded instructions;
- lowered IR instructions;
- statically known successors.

CFG validation checks instruction ownership, address continuity, block identity and successor references.

### 4. Recover functions

Function analysis groups CFG blocks and records:

- function entry blocks;
- direct call sites;
- function-to-function successors;
- explicit return sites;
- return continuations;
- unresolved indirect call/branch cases.

This recovered structure is consumed by semantic analysis and code generation.

### 5. Lower to typed IR

The typed IR keeps architectural operations explicit instead of exposing decoder-specific representations to later stages.

Representative operations include:

- `Mov`;
- arithmetic and logical operations;
- `Cmp`;
- `Load` / `Store`;
- branches and exchange operations;
- `Nop`;
- `Unknown`.

IR instructions expose architectural effects such as register reads/writes, memory access width/kind, flags and control flow.

### 6. Build semantic IR

The semantic layer makes control flow and side effects explicit. Semantic blocks can terminate with:

- fallthrough;
- conditional branch;
- direct call;
- indirect call;
- return;
- indirect branch;
- unknown.

Semantic validation protects invariants such as block ownership, instruction identity, successor validity, call continuations and consistency between recovered functions and semantic blocks.

Memory-effect propagation has been hardened for the represented extended and Thumb memory operations. This is important because optimization safety depends on the semantic layer accurately describing reads, writes and architectural effects.

### 7. Optimize conservatively

The optimizer currently favors transformations that can be justified by the modeled semantics, including:

- identity-move elimination to `Nop`;
- `Add 0` / `Sub 0` normalization;
- local constant propagation;
- local constant folding.

Optimization is effect-aware: modeled register, flag, memory and control-flow effects prevent unsafe simplification.

More aggressive dead-code elimination and global optimization are intentionally deferred until the remaining architectural effects are precise enough to prove correctness.

### 8. Generate Rust

`gba-recompiler` emits deterministic Rust representing recovered program structure and basic blocks. Generated code is expressed through the runtime execution contract for registers, memory, flags, condition evaluation, PC/Thumb state and control flow.

Generated Rust is currently a **development and execution artifact**, not a packaged standalone game executable.

### 9. Validate generated execution

The runtime exposes a deterministic generated-execution contract with explicit block exits, halt behavior and step limits.

The current differential fixtures validate representative paths against a small independent reference model:

```text
ROM fixture
    │
    ▼
Static analysis
    │
    ├── CFG / functions
    ├── semantic IR
    └── generated Rust contract
             │
             ▼
       Runtime execution
             │
             ▼
     Independent reference
             │
             ▼
   CPU / CPSR / memory comparison
```

The next step is to broaden this from representative fixtures to larger generated-block execution coverage and eventually complete block dispatch/linking for real ROM control flow.

## ARM7TDMI execution model

`gba-runtime` contains an explicit ARM7TDMI architectural state model rather than a collection of generic integer helpers.

Current foundations include:

- user/system, FIQ, IRQ, Supervisor, Abort and Undefined modes;
- banked SP/LR registers and FIQ banked `r8-r12`;
- SPSR handling for exception modes;
- CPSR `N/Z/C/V`, interrupt masks, mode bits and Thumb state;
- condition-code evaluation;
- ARM/Thumb exchange through BX;
- architectural PC and link-address rules;
- ARM shift edge cases and ROR behavior;
- add/subtract carry and borrow semantics;
- ARM unaligned word rotation behavior;
- exception entry and restoration foundations.

Architectural primitives are kept testable independently where practical, allowing the runtime contract and recompiler-facing semantics to be validated without making the runtime depend on the recompiler crate.

## Runtime

`gba-runtime` is the hardware-facing boundary consumed by generated code.

Current foundations include:

- ARM7TDMI register and status state;
- banked registers and exception-state support;
- 240×160 framebuffer storage and frame counter;
- APU state foundation;
- cartridge ROM storage;
- byte and little-endian 32-bit memory access;
- ARM unaligned-word behavior;
- simple I/O backing storage;
- cartridge save access;
- cycle/tick accounting;
- condition-code and execution support;
- generated-code dispatch, halt and unsupported-instruction hooks.

### Runtime limitations

The runtime is still a foundation rather than a complete GBA hardware implementation. Major missing areas include:

- complete GBA memory map and I/O register semantics;
- complete ARM7TDMI pipeline/timing behavior;
- remaining architectural corner cases and instruction coverage;
- full PPU/video modes, sprites and windows;
- complete APU channels and audio output;
- DMA;
- timers;
- interrupt scheduling;
- keypad/input behavior;
- complete cartridge protocol/timing behavior;
- complete generated block dispatch/linking;
- a full ROM execution loop.

## Cartridge saves

Battery-backed storage belongs to the cartridge model. It is **not an emulator savestate**.

Supported save-device foundations are:

```text
SRAM    32 KiB
Flash   64 KiB
Flash   128 KiB
EEPROM  512 B
EEPROM  8 KiB
```

Persistence follows this model:

```text
Generated game code
       │
       ▼
 Cartridge save device
       │
       ▼
    SaveRam
       │
       ├── saves/<game>.sav
       └── saves/<game>.sav.bak
```

Save handling includes ROM-based save-type detection, dirty tracking, matching-size save reuse, temporary-file replacement and preservation of the previous save when possible.

Save files remain outside the ROM so the source cartridge image stays immutable.

## CLI

The development CLI accepts an optional ROM path. Without an argument it uses the development FireRed ROM path:

```bash
cargo run -p gba-cli --release
```

With an explicit ROM:

```bash
cargo run -p gba-cli --release -- \
  "roms/1636 - Pokemon Fire Red (U)(Squirrels).gba"
```

The CLI currently:

1. reads the ROM;
2. starts static analysis at `0x0800_0000` in ARM mode;
3. reports the recovered entry, block count and instruction count;
4. emits generated Rust to `target/gba_generated.rs`;
5. initializes the runtime and loads the ROM into a `Cartridge` using `saves/` for persistence.

The CLI is a development harness; it does **not yet launch a complete playable generated ROM**.

## Frontend prototype

`gba-egui` is a separate `egui`/`eframe` frontend prototype backed by `gba-core`. It is currently outside the root Cargo workspace.

Its intended role is to provide a native analysis/debugging surface without coupling UI concerns to the recompiler core.

## Test ROMs

Development/test ROMs currently present under `roms/` include:

- `1636 - Pokemon Fire Red (U)(Squirrels).gba`;
- `1986 - Pokemon Emerald (U)(TrashMan).gba`.

These are development inputs. Use of ROMs should comply with applicable copyright and ownership rules.

## Development

Prerequisites:

- Rust stable toolchain;
- Cargo.

Recommended local validation:

```bash
cargo fmt --check
cargo test --workspace --all-targets
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

The repository is validated through GitHub Actions. Keep formatting, compilation, tests and Clippy clean before merging architectural changes.

## Validation strategy

Validation is intentionally cross-stage: individual modules can be correct while disagreeing at their boundaries, so the project tests the full chain progressively.

Current and planned validation layers are:

1. ARM7TDMI architectural primitive tests;
2. decoder and CFG regression fixtures;
3. semantic IR structural/effect validation;
4. semantic-to-codegen integration tests;
5. generated-execution fixtures with deterministic CPU/CPSR/memory state;
6. differential tests against independent reference semantics for selected instructions;
7. ROM-level regression fixtures for real GBA programs;
8. deterministic performance benchmarks for generated native code and runtime hot paths.

The generated-execution differential fixtures are now an established regression layer rather than a future-only goal.

## Optimization strategy

Optimization is staged behind correctness:

1. expand correct ARM/Thumb decoding;
2. strengthen CFG and function recovery on real GBA control flow;
3. make typed/semantic side-effect modeling precise;
4. expand deterministic generated-block execution coverage;
5. extend constant propagation/folding across valid control-flow regions;
6. introduce dead-code elimination only when architectural effects are explicit enough to prove safety;
7. specialize safe memory accesses;
8. add basic-block linking and branch-target specialization;
9. rely on Rust/LLVM for native-level optimization;
10. add runtime fast paths for hot hardware operations.

The guiding principle is to perform as much work as possible at **compile time** whenever the ROM makes the information statically recoverable.

## Roadmap

### Recompiler

- [x] Rust workspace and layer separation.
- [x] ARM/Thumb decoder foundation.
- [x] Reachable CFG recovery.
- [x] ARM/Thumb-aware block partitioning.
- [x] Typed IR foundation.
- [x] Function discovery foundation.
- [x] Semantic IR and structural validation.
- [x] Memory-effect hardening for represented instruction classes.
- [x] Effect-aware conservative optimization foundation.
- [x] Deterministic Rust code generation.
- [x] Generated-execution contract.
- [x] Differential validation fixtures for representative ARM execution paths.
- [ ] Broaden ARM/Thumb instruction coverage and reference fixtures.
- [ ] Complete generated block dispatch/linking.
- [ ] Execute larger real-ROM control-flow regions deterministically.
- [ ] Native block specialization and hot-path optimization.

### Runtime

- [x] ARM7TDMI architectural state foundation.
- [x] Banked registers and exception-state foundations.
- [x] CPSR/SPSR and condition-code support.
- [x] ARM/Thumb exchange foundations.
- [x] Cartridge ROM and save-device foundations.
- [x] Basic memory and cycle/tick services.
- [ ] Complete memory map and memory-mapped I/O.
- [ ] Complete DMA and timers.
- [ ] Complete IRQ scheduling.
- [ ] Complete keypad/input behavior.
- [ ] Complete PPU/video implementation.
- [ ] Complete APU/audio implementation.
- [ ] Complete cartridge protocol/timing behavior.

### Tooling / frontend

- [x] CLI analysis and generated-source harness.
- [x] `gba-core` prototype.
- [x] `gba-egui` prototype.
- [ ] Integrate the frontend crates into the workspace when their APIs stabilize.
- [ ] Add interactive CFG/IR/runtime inspection.
- [ ] Add deterministic ROM regression dashboards.

## Project principles

1. **Static first** — recover and specialize everything the ROM makes statically knowable.
2. **Semantic correctness before optimization** — never optimize away an architectural effect that is not modeled.
3. **Explicit boundaries** — decoder, analysis, generated code and runtime should remain independently testable.
4. **Deterministic execution** — generated execution and regression fixtures should produce reproducible state and control-flow results.
5. **Hardware honesty** — unsupported GBA behavior should be represented explicitly instead of silently approximated.
6. **Native output** — the end goal is generated Rust that can be optimized by Rust/LLVM, not a permanent interpreter loop.

## License

MIT. See [`Cargo.toml`](Cargo.toml) for the workspace license declaration.
