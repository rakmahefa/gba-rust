# gba-rust

**gba-rust** is a Rust-first **static Game Boy Advance recompilation project**. The project analyzes executable code from a GBA ROM, recovers reachable control flow and functions, lowers the result into typed intermediate representations, applies conservative IR optimizations, generates Rust source, and provides a separate runtime layer for GBA-facing services.

The architecture is deliberately **recompiler-first**, rather than centered on instruction-by-instruction interpretation.

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

The project has moved well beyond the initial decoder/CFG prototype. The main branch currently contains a working analysis pipeline that spans decoding, CFG recovery, function recovery, typed IR, semantic IR, conservative optimization, and deterministic Rust code generation.

Implemented foundations include:

- ARM and Thumb decoding infrastructure, including Thumb-1/Thumb-2-aware decoder organization;
- reachable-code discovery from an explicit ROM entry point;
- ARM/Thumb-aware basic-block and control-flow graph recovery;
- leader-based block partitioning that preserves sequential instruction tails;
- function discovery with call sites, return sites and function-level control flow;
- typed IR with register, memory, control-flow and flag-effect information;
- semantic IR with explicit terminators, register dependencies and memory/flag effects;
- semantic-program validation to protect instruction identity, ownership and CFG invariants;
- conservative optimization passes for identity moves, zero-add/sub normalization, constant propagation and local constant folding;
- deterministic Rust source generation from recovered program structure;
- a dedicated GBA runtime layer for CPU state, cartridge access, memory access, timing and display/audio foundations;
- cartridge save modeling for SRAM, Flash and EEPROM variants;
- dirty tracking and temporary-file save replacement with `.sav.bak` backup retention;
- a CLI development harness for ROM analysis and generated-source emission;
- an `egui`/`eframe` frontend crate prototype;
- workspace tests and GitHub Actions validation.

The generated program is **not yet a complete playable GBA game**. Full ARM7TDMI semantics, hardware emulation, generated-block linking/dispatch and broad instruction coverage are still under active development.

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
- **`gba-runtime`** provides the hardware-facing services consumed by generated code.
- **`gba-cli`** is the development harness for ROM analysis and generated-source emission.
- **`gba-core`** contains lower-level emulator/runtime foundations such as CPU, bus and cartridge models.
- **`gba-egui`** contains an `egui`/`eframe` frontend prototype.

The last two crates currently exist in the repository but are **not yet listed as workspace members** in the root `Cargo.toml`; they should therefore be treated as auxiliary/prototype layers until integrated into the workspace build.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the higher-level architectural contract and roadmap.

## Workspace

The root workspace currently declares these members:

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

Additional crates currently present in the repository tree:

```text
crates/
├── gba-core/
│   └── CPU, bus and cartridge foundations
│
└── gba-egui/
    └── egui/eframe frontend prototype
```

The workspace uses **Rust 2021**. The release profile currently enables thin LTO, a single codegen unit and `panic = "abort"`.

## Static recompiler pipeline

### 1. Decode

The decoder represents each decoded instruction with its address, size, execution mode and operation. Decoder logic is split into focused modules for common handling, classification, memory operations, semantic ARM/Thumb decoding and shared instruction types.

### 2. Discover reachable code

Analysis starts from an explicit ROM entry address and instruction mode. The analyzer follows statically known successors while preserving the ARM/Thumb mode in the CFG key.

```text
entry
  │
  ├── sequential successor
  ├── conditional branch target
  ├── unconditional branch target
  └── statically known exchange/call target
```

Unknown instructions and dynamic targets that cannot be recovered statically terminate or constrain discovery rather than inventing unsafe successors.

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

This stage provides the structural information required by the semantic IR.

### 5. Lower to typed IR

The initial IR keeps operations explicit instead of exposing decoder-specific representations directly to later passes.

Representative operations include:

- `Mov`
- `Add`
- `Sub`
- `Cmp`
- `Load`
- `Store`
- `Branch`
- `BranchExchange`
- `Nop`
- `Unknown`

The IR also derives architectural effects such as register reads/writes, memory access width/kind, flag dependencies and control-flow effects.

### 6. Build semantic IR

The semantic layer makes control flow and side effects explicit. Semantic blocks carry a terminator such as:

- fallthrough;
- conditional branch;
- direct call;
- indirect call;
- return;
- indirect branch;
- unknown.

Semantic validation protects invariants such as block ownership, instruction identity, valid successors and call continuations.

### 7. Optimize conservatively

The optimizer currently focuses on transformations that can be justified by the modeled semantics, including:

- identity-move elimination to timing-preserving `Nop`;
- `Add 0` / `Sub 0` normalization;
- local constant propagation;
- local constant folding.

Control-flow-sensitive and side-effect-sensitive operations such as PC writes, flag-changing comparisons and indirect branches are deliberately treated conservatively. More aggressive dead-code elimination and global optimization should wait until the IR models all required architectural effects explicitly.

### 8. Generate Rust

`gba-recompiler` emits deterministic Rust source representing recovered basic blocks. Generated code accesses runtime services for register state, memory, condition evaluation, timing and control-flow dispatch.

The generated source is currently a **development artifact and intermediate execution target**, not a packaged standalone game executable.

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
5. initializes a runtime and loads the ROM into a `Cartridge` with the `saves/` directory as its persistence location.

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

The CI workflow validates formatting, workspace tests and workspace compilation checks.

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

Near-term priorities are:

- [x] Establish Rust workspace and layer separation.
- [x] ARM/Thumb decoder foundation.
- [x] Reachable CFG recovery.
- [x] ARM/Thumb-aware block partitioning.
- [x] Initial typed IR.
- [x] Function discovery foundation.
- [x] Semantic IR and structural validation.
- [x] Conservative IR optimization foundation.
- [x] Initial Rust code generator.
- [x] Cartridge save-device model.
- [x] Initial `egui`/`eframe` frontend prototype.
- [ ] Expand ARM/Thumb instruction coverage toward real game code.
- [ ] Improve function/call/return recovery for real-world ROM control flow.
- [ ] Strengthen IR semantics for flags, timing, memory and exceptions.
- [ ] Add control-flow-aware/global optimization passes.
- [ ] Replace generated dispatch placeholders with linked block execution.
- [ ] Implement the complete GBA memory/hardware contract.
- [ ] Integrate `gba-core` and `gba-egui` into the workspace architecture.
- [ ] Add deterministic FireRed/Emerald regression suites.
- [ ] Benchmark generated native code and runtime hot paths.
- [ ] Reach a genuinely playable end-to-end generated ROM.

## Project philosophy

`gba-rust` favors:

- **static analysis over runtime interpretation**;
- **explicit boundaries between generated code and hardware services**;
- **semantic information before aggressive optimization**;
- **deterministic transformations**;
- **correctness before performance**;
- **small, testable Rust components**;
- **native-code execution as the eventual performance target**.

The project should therefore be understood as an actively developed **static GBA recompilation system with emulator/runtime foundations**, not as a finished general-purpose GBA emulator.

## License

The root Cargo workspace declares **MIT** in its package metadata. See the repository's authoritative licensing files for the terms applicable to the project.
