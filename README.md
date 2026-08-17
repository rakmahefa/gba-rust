# gba-rust

**gba-rust** is a Rust-first **static Game Boy Advance recompilation project**. It analyzes executable code from a GBA ROM, recovers reachable control flow and functions, lowers instructions into typed and semantic intermediate representations, performs conservative optimizations, and generates Rust source intended to execute against a separate GBA runtime.

The project is **recompiler-first** rather than interpreter-first: the long-term execution model is statically generated native Rust/LLVM code backed by explicit runtime services.

```text
GBA ROM
  │
  ▼
ARM / Thumb decoding
  │
  ▼
Reachable CFG recovery
  │
  ▼
Function discovery
  │
  ▼
Typed IR
  │
  ▼
Semantic IR
  │
  ▼
Conservative optimization
  │
  ▼
Rust code generation
  │
  ▼
Native Rust / LLVM
  │
  ▼
GBA runtime services
```

> Inspired by the engineering direction of [`arcanite24/gb-recompiled`](https://github.com/arcanite24/gb-recompiled), but implemented as an independent Rust-first GBA architecture.

## Current status

The project has moved beyond the initial decoder/CFG prototype. The main branch currently contains an end-to-end **static analysis → semantic IR → optimization → Rust code generation** pipeline, with the semantic layer being actively hardened around architectural side effects.

Recent work has specifically strengthened:

- ARM and GBA Thumb decoding coverage;
- ARM/Thumb-aware reachable CFG recovery and block partitioning;
- function discovery with call and return structure;
- typed IR register, control-flow, memory and flag effects;
- semantic IR preservation of instruction identity and architectural effects;
- complete modeling of the currently represented memory-effect kinds, including extended and Thumb memory operations;
- optimizer safeguards so transformations respect modeled IR effects;
- code generation for the expanded IR operation set;
- regression tests for semantic completeness and non-trivial instruction fixtures;
- deterministic Rust source generation.

The generated program is **not yet a complete playable GBA game**. Full ARM7TDMI behavior, the GBA hardware contract, generated-block linking/dispatch, and broad instruction coverage remain under active development.

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
                 │ IR optimization          │
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
                 │ CPU state               │
                 │ Memory / cartridge      │
                 │ Save devices             │
                 │ PPU / APU foundations   │
                 │ Timing / runtime API    │
                 └────────────┬─────────────┘
                              │
                              ▼
                       Native Rust
```

The separation is intentional:

- **`gba-recompiler`** understands ROM code and produces analysis data, semantic IR and generated Rust.
- **Generated Rust** represents statically recovered program logic.
- **`gba-runtime`** provides hardware-facing services consumed by generated code.
- **`gba-cli`** is the development harness for ROM analysis and generated-source emission.
- **`gba-core`** contains lower-level CPU, bus and cartridge foundations.
- **`gba-egui`** contains an `egui`/`eframe` frontend prototype.

`gba-core` and `gba-egui` are present in the repository but are currently **not declared as root workspace members**. They should therefore be considered auxiliary/prototype layers until they are integrated into the workspace architecture.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the architectural contract and long-term roadmap.

## Workspace

The root workspace currently declares:

```text
crates/
├── gba-recompiler/
│   └── ARM/Thumb decoding, CFG, function analysis, IR, optimization and codegen
│
├── gba-runtime/
│   └── CPU, memory, cartridge/save and runtime services
│
└── gba-cli/
    └── ROM analysis and generated-source development harness
```

Additional repository crates:

```text
crates/
├── gba-core/
│   └── CPU, bus and cartridge foundations
│
└── gba-egui/
    └── egui/eframe frontend prototype
```

The workspace uses **Rust 2021**. The release profile enables thin LTO, a single codegen unit, and `panic = "abort"`.

## Static recompiler pipeline

### 1. Decode

The decoder represents each instruction with its address, size, execution mode and operation. The decoder is organized into focused modules covering ARM and GBA Thumb decoding, memory operations, classification and shared instruction semantics.

GBA hardware uses **ARM state and the original ARMv4T Thumb instruction set**; this project should therefore not be described as a Thumb-2 implementation.

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

Unknown instructions and dynamic targets that cannot be recovered statically constrain discovery instead of inventing unsafe successors.

### 3. Recover basic blocks

Leaders are derived from the entry point and control-flow targets. Sequential instructions remain in the same block until a control-flow boundary is reached.

Each block carries:

- a stable `BlockId`;
- a starting address and ARM/Thumb mode;
- decoded instructions;
- lowered IR instructions;
- statically known successors.

CFG validation checks instruction ownership, address continuity, block identity and successor references.

### 4. Recover functions

The function-analysis layer groups CFG blocks into functions and tracks:

- function entry blocks;
- function-to-function successors;
- direct call sites;
- explicit return sites;
- return continuations;
- indirect call/branch cases that cannot be fully resolved statically.

This provides the structure consumed by the semantic IR.

### 5. Lower to typed IR

The typed IR keeps architectural operations explicit rather than exposing decoder-specific representations directly to later passes.

Representative operations include:

- `Mov`
- arithmetic and logical operations
- `Cmp`
- `Load`
- `Store`
- branches and exchange operations
- `Nop`
- `Unknown`

IR instructions also expose architectural effects such as register reads/writes, memory access width and kind, flag effects, and control-flow effects.

### 6. Build semantic IR

The semantic layer makes control flow and side effects explicit. Semantic blocks carry terminators such as:

- fallthrough;
- conditional branch;
- direct call;
- indirect call;
- return;
- indirect branch;
- unknown.

Semantic instructions preserve the effects needed by later optimization and code generation, including modeled register dependencies, flag effects and memory effects.

Recent hardening work specifically corrected and completed memory-effect propagation for extended instructions and Thumb memory operations. This is important because optimization correctness depends on the semantic layer accurately describing what an instruction can read, write or affect.

Semantic-program validation protects invariants such as:

- block ownership;
- instruction identity;
- successor validity;
- call continuations;
- structural consistency between recovered functions and semantic blocks.

### 7. Optimize conservatively

The optimizer currently prioritizes transformations that are justified by the modeled semantics, including:

- identity-move elimination to timing-preserving `Nop`;
- `Add 0` / `Sub 0` normalization;
- local constant propagation;
- local constant folding.

Optimization passes are deliberately effect-aware. Operations with modeled register, flag, memory or control-flow effects are not simplified merely because their value result looks redundant.

More aggressive dead-code elimination and global optimization should wait until the remaining architectural effects are represented precisely enough to prove safety.

### 8. Generate Rust

`gba-recompiler` emits deterministic Rust representing recovered program structure and basic blocks. Generated code uses runtime services for register state, memory operations, condition evaluation, timing and control-flow dispatch.

The generator now handles the expanded IR operation set needed by the semantic hardening work. The generated source remains a **development artifact and intermediate execution target**, not a packaged standalone game executable.

## Recompiler crate API

The public `gba-recompiler` surface is organized around the major compilation stages:

```rust
pub mod cfg;
pub mod codegen;
pub mod decoder;
pub mod function;
pub mod ir;
pub mod optimization;
pub mod semantic_ir;
```

The crate exposes the main pipeline building blocks, including:

```text
decode_arm / decode_thumb
        │
        ▼
analyze
        │
        ▼
discover_functions
        │
        ▼
build_semantic_program
        │
        ▼
validate_semantic_program
        │
        ▼
optimize_semantic_program
        │
        ▼
generate
```

A full-pipeline regression test in the crate verifies that analysis reaches semantic IR, optimization and Rust code generation as a single flow.

## Runtime

`gba-runtime` is the hardware-facing boundary consumed by generated code.

Current foundations include:

- ARM7TDMI register state (`r[0..15]`), CPSR and Thumb state;
- 240×160 framebuffer storage and frame counter;
- APU state foundation;
- cartridge ROM storage;
- byte and little-endian 32-bit memory access;
- simple I/O backing storage;
- cartridge save access;
- cycle/tick accounting;
- basic condition-code evaluation;
- hooks for generated-code dispatch, halt and unsupported instructions.

The runtime is intentionally separate from the source ROM and from the frontend.

### Runtime limitations

The runtime is still a foundation rather than a complete GBA hardware implementation. Major missing areas include:

- complete GBA memory map and I/O register semantics;
- complete ARM7TDMI instruction and exception behavior;
- full PPU/video mode implementation, sprites and windows;
- complete APU channels and audio output;
- DMA;
- timers;
- interrupts and scheduling;
- keypad/input behavior;
- complete cartridge protocol/timing behavior;
- generated block linking and a full execution loop.

## Cartridge saves

Battery-backed storage is modeled as part of the cartridge, not as an emulator savestate.

Current save variants are:

```text
SRAM 32 KiB
Flash 64 KiB
Flash 128 KiB
EEPROM 512 B
EEPROM 8 KiB
```

The persistence model is:

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

Save handling currently provides:

- ROM-based save-type detection using known signatures;
- dirty tracking on writes;
- reuse of an existing save when its size matches the detected device;
- temporary-file replacement through `.sav.tmp`;
- preservation of the previous save as `.sav.bak` when possible;
- final flush from `SaveRam` on drop.

Save files remain outside the ROM so the source cartridge image stays immutable.

## CLI

The development CLI accepts an optional ROM path. When no argument is supplied, it uses the development FireRed ROM path:

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
5. initializes the runtime and loads the ROM into a `Cartridge` with the `saves/` directory as its persistence location.

It does **not yet execute a complete generated game loop**.

## Frontend prototype

The repository contains a separate `gba-egui` crate using `egui` and `eframe`, backed by `gba-core`. It is currently a prototype and is not yet part of the root Cargo workspace.

The intended role of the frontend is to provide a native development/debugging surface without coupling UI concerns to the recompiler core.

## Test ROMs

Development ROMs currently present under `roms/` include:

- `1636 - Pokemon Fire Red (U)(Squirrels).gba`
- `1986 - Pokemon Emerald (U)(TrashMan).gba`

These files are development/test inputs. Ensure that your use of any ROM complies with applicable copyright and ownership rules.

## Development

Prerequisites:

- Rust stable toolchain
- Cargo

Run the root workspace validation locally with:

```bash
cargo fmt
cargo test --workspace
cargo check --workspace --all-targets
```

The repository uses GitHub Actions for automated validation. The latest merged work has focused on semantic IR correctness, memory-effect completeness, optimizer effect-awareness and expanded code-generation coverage.

## Optimization strategy

Optimization is deliberately staged behind correctness and deterministic analysis:

1. Expand correct ARM/Thumb decoding.
2. Make CFG and function discovery robust across real GBA control flow.
3. Strengthen typed and semantic IR side-effect modeling.
4. Extend constant propagation/folding safely across valid control-flow regions.
5. Add dead-code elimination only when flags, timing and memory side effects are explicit enough to prove safety.
6. Specialize safe memory accesses.
7. Add basic-block linking and branch-target specialization.
8. Let Rust/LLVM optimize generated code.
9. Add runtime fast paths for hot hardware operations.
10. Build deterministic ROM regression and benchmark suites.

The guiding principle is to perform as much work as possible at **compile time**, whenever the ROM makes that information statically recoverable.

## Roadmap

### Recompiler

- [x] Rust workspace and layer separation.
- [x] ARM/Thumb decoder foundation.
- [x] Reachable CFG recovery.
- [x] ARM/Thumb-aware block partitioning.
- [x] Initial typed IR.
- [x] Function discovery foundation.
- [x] Semantic IR and structural validation.
- [x] Semantic memory-effect hardening for represented instruction classes.
- [x] Effect-aware conservative optimization foundation.
- [x] Rust code generation for the expanded IR operation set.
- [ ] Expand ARM/Thumb instruction coverage toward real game code.
- [ ] Improve function/call/return recovery for real-world ROM control flow.
- [ ] Strengthen timing, exception and remaining architectural side-effect modeling.
- [ ] Add control-flow-aware/global optimization passes.
- [ ] Replace generated dispatch placeholders with linked block execution.

### Runtime

- [x] CPU/register state foundation.
- [x] Cartridge and save-device foundations.
- [x] Cycle/tick accounting foundation.
- [ ] Complete GBA memory/I/O contract.
- [ ] Complete PPU/video behavior.
- [ ] Complete APU/audio behavior.
- [ ] DMA, timers, IRQs and keypad.
- [ ] Complete cartridge protocols and hardware timing.

### Integration and validation

- [x] CLI development harness.
- [x] Initial `egui`/`eframe` frontend prototype.
- [ ] Integrate `gba-core` and `gba-egui` into the workspace architecture.
- [ ] Add deterministic FireRed/Emerald regression suites.
- [ ] Add generated-code/runtime differential tests.
- [ ] Benchmark generated native code and runtime hot paths.
- [ ] Reach a genuinely playable end-to-end generated ROM.

## Project philosophy

`gba-rust` favors:

- **static analysis over runtime interpretation**;
- **explicit boundaries between generated code and hardware services**;
- **semantic information before aggressive optimization**;
- **effect-aware transformations**;
- **deterministic compilation stages**;
- **correctness before performance**;
- **small, testable Rust components**;
- **native-code execution as the eventual performance target**.

The project should therefore be understood as an actively developed **static GBA recompilation system with emulator/runtime foundations**, not as a finished general-purpose GBA emulator.

## License

The root Cargo workspace declares **MIT** in its package metadata. See the repository's authoritative licensing files for the terms applicable to the project.
