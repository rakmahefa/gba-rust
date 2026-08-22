# gba-rust

**gba-rust** is a Rust-first **static Game Boy Advance recompilation project**.

The project analyzes executable code from a GBA ROM, recovers reachable control flow and functions, lowers instructions into typed and semantic intermediate representations, performs conservative effect-aware optimization, and generates Rust code backed by a separate GBA runtime.

> **Recompiler-first, not interpreter-first.** The architectural goal is statically generated native Rust/LLVM code plus explicit runtime services, rather than a conventional instruction-by-instruction interpreter.

Inspired by the engineering direction of [`arcanite24/gb-recompiled`](https://github.com/arcanite24/gb-recompiled), but implemented as an independent Rust-first GBA architecture.

## Current status

`gba-rust` has moved well beyond the initial decoder/CFG prototype into a multi-stage static recompilation pipeline with an explicit ARM7TDMI execution contract.

The current `main` includes:

- ARM and original GBA Thumb (ARMv4T) decoding;
- reachable CFG recovery with ARM/Thumb-aware addresses;
- dedicated CFG discovery, edge construction, abstract-state analysis, partitioning and hardening modules;
- basic-block partitioning and function discovery;
- typed IR and semantic IR with structural validation;
- explicit register, flag, memory and control-flow effects;
- conservative effect-aware optimization;
- deterministic Rust code generation;
- an explicit ARM7TDMI architectural runtime model;
- deterministic generated-block execution contracts with halt, exception and step-limit behavior;
- architectural regression fixtures and differential generated-execution validation;
- BIOS SWI/exception execution foundations;
- architectural exception entry/return and reentrant IRQ execution;
- BIOS exception-graph analysis and real BIOS exception-vector execution tests;
- cartridge save foundations for SRAM, Flash and EEPROM variants.

The project is **not yet a complete playable GBA implementation**. The main remaining work is broader instruction coverage, complete generated-block dispatch/linking, larger real-ROM execution coverage, and the remaining GBA hardware contract: the full memory map and memory-mapped I/O, DMA, timers, interrupt scheduling, keypad, PPU, APU and complete cartridge timing/protocol behavior.

## Architecture

```text
                         GBA ROM (.gba)
                              |
                              v
                 +---------------------------+
                 |      gba-recompiler       |
                 |                           |
                 | ARM / Thumb decoder       |
                 | CFG recovery              |
                 | Function discovery        |
                 | Typed IR                  |
                 | Semantic IR               |
                 | Conservative optimizer    |
                 | Rust code generation      |
                 +-------------+-------------+
                               |
                               v
                        Generated Rust
                               |
                               v
                 +---------------------------+
                 |        gba-runtime         |
                 |                           |
                 | ARM7TDMI state           |
                 | Exception / BIOS model   |
                 | Memory / cartridge       |
                 | Hardware-facing APIs     |
                 | Timing / execution       |
                 +-------------+-------------+
                               |
                               v
                        Native Rust / LLVM
```

The layers deliberately have different responsibilities:

- **`gba-recompiler`** analyzes ROM code and produces CFGs, IR, semantic representations and generated Rust.
- **Generated Rust** represents the statically recovered program logic and emits explicit execution-boundary transitions.
- **`gba-runtime`** provides CPU state, exception handling, memory, cartridge and hardware-facing services consumed by generated code.
- **`gba-cli`** is the development harness for ROM analysis, generated-source emission and runtime bootstrapping.
- **`gba-core`** contains lower-level CPU/bus/cartridge foundations and is currently outside the root workspace.
- **`gba-egui`** contains an `egui`/`eframe` frontend prototype and is currently outside the root workspace.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the architectural contract.

## Workspace

The root Cargo workspace currently contains:

```text
crates/
├── gba-recompiler/
│   └── ARM/Thumb decoding, CFG, functions, IR, optimization and codegen
├── gba-runtime/
│   └── ARM7TDMI state, exceptions, memory, cartridge/save and runtime services
└── gba-cli/
    └── ROM analysis and generated-source development harness
```

Additional prototype crates remain in the repository but outside the root workspace:

```text
crates/
├── gba-core/
│   └── CPU, bus and cartridge foundations
└── gba-egui/
    └── egui/eframe analysis/debug frontend
```

The workspace uses **Rust 2021**. Release builds use thin LTO, one codegen unit and `panic = "abort"`.

## Static recompiler pipeline

### 1. Decode

Instructions retain their address, size, execution mode and decoded operation. ARM and original GBA Thumb decoding are organized into focused modules for instruction families, memory operations, classification and shared semantic helpers.

GBA CPUs use **ARM state and the original ARMv4T Thumb instruction set**. This project is therefore **not a Thumb-2 recompiler**.

### 2. Discover reachable code

Analysis starts from an explicit ROM entry address and execution mode. Statically known successors are followed while preserving the ARM/Thumb mode in the CFG key.

```text
entry
  |
  +-- sequential successor
  +-- conditional branch target
  +-- unconditional branch target
  +-- statically known exchange/call target
```

Unknown instructions and unresolved dynamic targets constrain static discovery instead of inventing unsafe successors.

### 3. Build and harden the CFG

CFG leaders come from the entry point and control-flow targets. Blocks retain their execution mode and stop at control-flow boundaries.

The CFG implementation is split into focused responsibilities:

```text
cfg/
├── abstract_state.rs
├── discovery.rs
├── edges.rs
├── hardening.rs
├── model.rs
├── partition.rs
└── mod.rs
```

Each basic block carries:

- a stable `BlockId`;
- an address and ARM/Thumb mode;
- decoded instructions;
- lowered IR instructions;
- statically known successors.

Validation protects invariants such as instruction ownership, address continuity, block identity and successor references.

The CFG analysis also uses conservative abstract register information where useful. Statically known values can resolve some indirect control-flow targets, including ARM/Thumb exchange cases, while unresolved targets remain unknown instead of being guessed.

### 4. Recover functions

Function analysis records:

- function entry blocks;
- direct call sites;
- function-to-function successors;
- return sites;
- return continuations;
- unresolved indirect call/branch cases.

The recovered structure is consumed by semantic analysis and code generation.

### 5. Lower to typed IR

The typed IR keeps architectural operations explicit instead of leaking decoder-specific representations into later stages.

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
- exception;
- unknown.

Semantic validation protects block ownership, instruction identity, successor validity, call continuations and consistency between recovered functions and semantic blocks.

Memory-effect propagation is hardened for the represented extended ARM and Thumb memory operations because optimization safety depends on accurate reads, writes and architectural effects.

### 7. Optimize conservatively

The optimizer favors transformations justified by the modeled semantics, including:

- identity-move elimination to `Nop`;
- `Add 0` / `Sub 0` normalization;
- local constant propagation;
- local constant folding.

Optimization is effect-aware. Register, flag, memory and control-flow effects constrain transformations so correctness remains the priority.

More aggressive dead-code elimination and global optimization are deferred until the remaining architectural effects are precise enough to prove safety.

### 8. Generate Rust

`gba-recompiler` emits deterministic Rust representing recovered program structure and basic blocks. Generated code goes through the runtime execution contract for registers, memory, flags, condition evaluation, PC/Thumb state, control flow and architectural exceptions.

Generated Rust is currently a **development and execution artifact**, not a packaged standalone game executable.

### 9. Execute generated blocks deterministically

The runtime exposes a deterministic generated-execution contract with explicit block exits, halt behavior, step limits and exception boundaries.

Normal control flow follows statically linked or validated CFG targets. Exceptions cross the same generated/runtime boundary rather than mutating CPU state in the middle of a generated instruction.

The intended execution shape is:

```text
ROM fixture
    |
    v
Static analysis
    |
    +-- CFG / functions
    +-- semantic IR
    +-- generated Rust
             |
             v
     Generated block A
             |
             v
      runtime boundary
             |
       +-----+-----+
       |           |
       v           v
   block B      exception
       |           |
       |           v
       |      exception entry
       |           |
       +-----<-----+
             |
             v
     CPU / CPSR / memory
```

The current regression layer covers representative ARM execution paths, memory effects, branch/loop behavior, ARM/Thumb state transitions, IRQ entry/return and real BIOS exception-vector execution. The next expansion is broader multi-block generated execution and larger real-ROM control-flow regions.

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

Architectural primitives remain independently testable where practical, keeping runtime correctness separate from the recompiler crate.

## BIOS and exception execution

The runtime now models BIOS-triggered exception behavior as an architectural execution boundary rather than as an unrelated helper call.

### SWI boundary

For generated BIOS SWIs:

1. the runtime enters Supervisor mode;
2. the appropriate exception state is captured in the architectural CPU state;
3. the modeled BIOS service executes against the runtime state;
4. returning SWIs restore the caller's CPSR and banked registers;
5. non-returning services such as HALT/STOP retain the expected privileged execution state.

### IRQ boundary

Generated execution observes pending enabled IRQs at a **block boundary**. The dispatcher establishes the architectural PC for that boundary and then reuses the runtime exception-entry contract.

This preserves:

- the interrupted resume point;
- CPSR state;
- banked IRQ registers;
- Supervisor/privileged execution context where applicable.

The IRQ does not mutate CPU mode in the middle of a generated instruction.

### Exception returns

ARM instructions that write `PC` with the `S` bit set are treated as architectural exception returns. The generated path evaluates the return target, asks the runtime to restore the active SPSR/banked state, and only then performs the CFG transition.

Ordinary `BX LR`/function returns remain separate from architectural exception restoration.

### Reentrant exceptions

Nested exception entry is supported by the same boundary model. An IRQ taken while privileged can capture the current mode's state, use IRQ banked registers and later restore the interrupted context through the architectural exception-return primitive.

### BIOS exception graph

The recompiler exposes BIOS exception-graph analysis so exception vectors, handler structure and return sites can participate in static analysis instead of being treated as opaque addresses.

A real BIOS exception vector is now exercised through generated code as a regression path. The graph and execution contract remain deliberately conservative when a target cannot be represented by the generated CFG.

## Runtime

`gba-runtime` is the hardware-facing boundary consumed by generated code.

Current foundations include:

- ARM7TDMI register and status state;
- banked registers and exception-state support;
- BIOS/SWI execution foundations;
- exception entry/return support;
- 240x160 framebuffer storage and frame counter;
- APU state foundation;
- cartridge ROM storage;
- byte and little-endian 32-bit memory access;
- ARM unaligned-word behavior;
- simple I/O backing storage;
- cartridge save access;
- cycle/tick accounting;
- condition-code and execution support;
- generated-code dispatch, halt, exception and unsupported-instruction hooks.

### Runtime limitations

The runtime is still a foundation rather than a complete GBA hardware implementation. Major remaining areas include:

- complete GBA memory map and memory-mapped I/O register semantics;
- complete ARM7TDMI pipeline/timing behavior;
- remaining architectural corner cases and instruction coverage;
- full PPU/video modes, sprites and windows;
- complete APU channels and audio output;
- DMA;
- timers;
- interrupt scheduling beyond the current generated IRQ boundary contract;
- keypad/input behavior;
- complete cartridge protocol/timing behavior;
- complete generated block dispatch/linking and fast-path linking;
- a full deterministic real-ROM execution loop.

The immediate hardware goal is therefore not to implement every device at once, but to establish a correct bus/memory contract that generated code can rely on.

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
       |
       v
 Cartridge save device
       |
       v
    SaveRam
       |
       +-- saves/<game>.sav
       +-- saves/<game>.sav.bak
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

`gba-egui` is a separate `egui`/`eframe` frontend prototype backed by `gba-core`. It remains outside the root workspace while its APIs are still evolving.

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

GitHub Actions mirrors these validation stages through separate formatting, test, check and Clippy jobs. Keep all four clean before merging architectural changes.

## Validation strategy

Validation is intentionally cross-stage: individual modules can be correct while disagreeing at their boundaries, so the project tests the chain progressively.

Current and planned validation layers are:

1. ARM7TDMI architectural primitive tests;
2. decoder and CFG regression fixtures;
3. semantic IR structural/effect validation;
4. semantic-to-codegen integration tests;
5. generated-execution fixtures with deterministic CPU/CPSR/memory state;
6. exception/IRQ regression fixtures;
7. differential tests against independent reference semantics for selected instructions;
8. BIOS exception-vector execution fixtures;
9. ROM-level regression fixtures for real GBA programs;
10. deterministic performance benchmarks for generated native code and runtime hot paths.

Generated-execution differential fixtures and the BIOS exception-vector path are now established regression layers rather than future-only goals.

## Optimization strategy

Optimization is staged behind correctness:

1. expand correct ARM/Thumb decoding;
2. strengthen CFG and function recovery on real GBA control flow;
3. make typed/semantic side-effect modeling precise;
4. complete deterministic generated-block execution coverage;
5. formalize the bus/memory contract used by generated code;
6. extend constant propagation/folding across valid control-flow regions;
7. specialize safe memory accesses;
8. add basic-block dispatch, linking and branch-target specialization;
9. rely on Rust/LLVM for native-level optimization;
10. add runtime fast paths for hot hardware operations.

The guiding principle is to perform as much work as possible at **compile time** whenever the ROM makes the information statically recoverable.

## Roadmap

### Recompiler

- [x] Rust workspace and layer separation.
- [x] ARM/Thumb decoder foundation.
- [x] Reachable CFG recovery.
- [x] ARM/Thumb-aware block partitioning.
- [x] CFG module decomposition and control-flow hardening.
- [x] Typed IR foundation.
- [x] Function discovery foundation.
- [x] Semantic IR and structural validation.
- [x] Memory-effect hardening for represented instruction classes.
- [x] Effect-aware conservative optimization foundation.
- [x] Deterministic Rust code generation.
- [x] Generated-execution contract.
- [x] Differential validation fixtures for representative execution paths.
- [x] BIOS exception-graph analysis.
- [x] Real BIOS exception-vector execution fixture.
- [ ] Broaden ARM/Thumb instruction coverage and reference fixtures.
- [ ] Complete generated multi-block dispatch/linking.
- [ ] Execute larger real-ROM control-flow regions deterministically.
- [ ] Add native block specialization and hot-path optimization.

### Runtime

- [x] ARM7TDMI architectural state foundation.
- [x] Banked registers and exception-state foundations.
- [x] CPSR/SPSR and condition-code support.
- [x] ARM/Thumb exchange foundations.
- [x] BIOS/SWI exception-entry foundations.
- [x] IRQ boundary and architectural exception-return foundations.
- [x] Cartridge ROM and save-device foundations.
- [x] Basic memory and cycle/tick services.
- [ ] Complete GBA memory map and memory-mapped I/O.
- [ ] Complete DMA and timers.
- [ ] Complete interrupt scheduling.
- [ ] Complete keypad/input behavior.
- [ ] Complete PPU/video implementation.
- [ ] Complete APU/audio implementation.
- [ ] Complete cartridge protocol/timing behavior.
- [ ] Complete integration with a full real-ROM execution loop.

### Tooling / frontend

- [x] CLI analysis and generated-source harness.
- [x] `gba-core` prototype.
- [x] `gba-egui` prototype.
- [ ] Integrate frontend crates into the workspace when their APIs stabilize.
- [ ] Add interactive CFG/IR/runtime inspection.
- [ ] Add deterministic ROM regression dashboards.

## Immediate next phase

The next architectural phase is **generated block dispatch and multi-block execution**, not another BIOS-only milestone.

```text
Static analysis
      |
      v
Generated blocks
      |
      v
Validated block targets
      |
      v
Deterministic dispatcher
      |
      v
Multi-block execution
      |
      v
Bus / memory contract
      |
      v
DMA + timers + IRQ scheduling
      |
      v
PPU / APU / keypad
      |
      v
Large real-ROM execution
      |
      v
Native specialization / optimization
```

The immediate goals are:

1. complete generated-block dispatch and linked CFG transitions;
2. expand deterministic execution across larger real-ROM regions;
3. establish the complete CPU-to-bus memory contract;
4. then add hardware devices in dependency order.

This keeps the project focused on **execution correctness before hardware breadth and optimization**.

## Architectural priorities

The current priority order is:

```text
1. Execution correctness
2. Generated multi-block dispatch
3. CPU / bus / memory correctness
4. Interrupt and timing integration
5. GBA hardware completeness
6. Native specialization and optimization
```

The project deliberately avoids broad optimization or full-device implementation while the generated execution boundary and hardware contract are still evolving.

## Project principles

1. **Static first** — recover and specialize everything the ROM makes statically knowable.
2. **Semantic correctness before optimization** — never optimize away an architectural effect that is not modeled.
3. **Explicit boundaries** — decoder, analysis, generated code and runtime remain independently testable.
4. **Deterministic execution** — generated execution and regression fixtures should produce reproducible state and control flow.
5. **Conservative uncertainty** — unresolved indirect behavior should remain explicit instead of being guessed.
6. **Compile-time specialization** — move work from runtime to analysis whenever the ROM makes it safe to do so.
7. **Hardware as a contract** — generated code depends on explicit runtime services rather than hidden emulator state.
8. **Real ROMs as validation** — synthetic fixtures prove local semantics; real BIOS and cartridge code validate the recovered system as a whole.
